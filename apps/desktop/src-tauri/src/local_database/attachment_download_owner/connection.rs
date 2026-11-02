//! Supervisor-owned actual connection; callbacks retain their registration through close.
use super::*;
use sqlx::{sqlite::SqliteLockingMode, Connection};
pub(super) struct Database {
    connection: Option<sqlx::SqliteConnection>,
    vfs: Option<vfs::NativeVfs>,
}
impl Database {
    pub async fn prepare(dir: filesystem::Directory, main: &str) -> Result<Self, String> {
        Self::connect(vfs::NativeVfs::new(dir, main)?, false).await
    }
    async fn connect(vfs: vfs::NativeVfs, invalid_pragma: bool) -> Result<Self, String> {
        let mut options = SqliteConnectOptions::new()
            .filename(vfs.filename())
            .vfs(vfs.name())
            .locking_mode(SqliteLockingMode::Exclusive)
            .synchronous(SqliteSynchronous::Full);
        // No Pool acquire timeout can cancel establishment. A main-file lease
        // retains registration if SQLx drops an initialized connection on PRAGMA error.
        if invalid_pragma {
            options = options.pragma("cache_size", "INVALID SQL");
        }
        let connection = match sqlx::SqliteConnection::connect_with(&options).await {
            Ok(connection) => connection,
            Err(error) => {
                vfs.wait_closed().await;
                return Err(format!("Native connection establishment failed: {error}"));
            }
        };
        Ok(Self {
            connection: Some(connection),
            vfs: Some(vfs),
        })
    }
    #[cfg(test)]
    pub async fn prepare_gated(
        dir: filesystem::Directory,
        main: &str,
        open: Box<dyn Fn() + Send + Sync>,
        retire: Box<dyn Fn(&str) + Send + Sync>,
    ) -> Result<Self, String> {
        let vfs = vfs::NativeVfs::new(dir, main)?;
        vfs.test_gates(open, retire);
        Self::connect(vfs, false).await
    }
    #[cfg(test)]
    pub async fn prepare_invalid_pragma(
        dir: filesystem::Directory,
        main: &str,
        close: Box<dyn Fn(&str) + Send + Sync>,
    ) -> Result<Self, String> {
        let vfs = vfs::NativeVfs::new(dir, main)?;
        let name = vfs.name();
        vfs.test_close_gate(Box::new(move || close(&name)));
        Self::connect(vfs, true).await
    }
    pub fn connection(&mut self) -> &mut sqlx::SqliteConnection {
        self.connection.as_mut().expect("Live connection")
    }
    pub fn fence_owner(&self) -> vfs::NativeVfs {
        self.vfs.as_ref().expect("Live VFS").clone()
    }
    pub fn fence(&self) -> Result<(), String> {
        self.vfs.as_ref().expect("Live VFS").check()
    }
    pub fn detail(&self) -> Option<String> {
        self.vfs.as_ref().and_then(vfs::NativeVfs::last_error)
    }
    pub async fn retire(mut self) -> Result<(), String> {
        let connection = self.connection.take().expect("Live connection");
        let result = connection
            .close()
            .await
            .map_err(|e| format!("Native worker close failed: {e}"));
        drop(self.vfs.take());
        result
    }
}
// Caller cancellation drops only the reply. The private runtime owns the full
// operation, snapshot mutex, establishment, SQL execution and completed close.
type Job = std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>>;
fn supervisor() -> Result<&'static tokio::sync::mpsc::Sender<Job>, String> {
    static SUPERVISOR: OnceLock<Result<tokio::sync::mpsc::Sender<Job>, String>> = OnceLock::new();
    SUPERVISOR
        .get_or_init(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("Native operation supervisor unavailable: {e}"))?;
            let (send, mut receive) = tokio::sync::mpsc::channel::<Job>(8);
            std::thread::Builder::new()
                .name("native-download-owner".into())
                .spawn(move || {
                    while let Some(job) = receive.blocking_recv() {
                        runtime.block_on(job);
                    }
                })
                .map_err(|e| format!("Native operation supervisor unavailable: {e}"))?;
            Ok(send)
        })
        .as_ref()
        .map_err(Clone::clone)
}
pub(super) async fn supervise<T, F>(operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: std::future::Future<Output = Result<T, String>> + Send + 'static,
{
    let (send, receive) = tokio::sync::oneshot::channel();
    supervisor()?
        .send(Box::pin(async move {
            let result = operation.await;
            let _ = send.send(result);
        }))
        .await
        .map_err(|_| "Native operation supervisor stopped".to_string())?;
    receive
        .await
        .map_err(|_| "Native operation supervisor did not complete".to_string())?
}
