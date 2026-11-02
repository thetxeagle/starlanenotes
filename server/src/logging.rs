//! Bounded on-disk tracing output for self-hosted deployments.
//!
//! The container's normal stdout stream remains available for Docker tooling. This
//! writer adds a durable copy under `<DATA_DIR>/logging`, rotates before a file
//! reaches 64 MiB, and prunes old files on startup and rotation.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tracing_subscriber::fmt::MakeWriter;

const MAX_LOG_AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);
const MAX_LOG_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

struct LogFileState {
    directory: PathBuf,
    file: Option<File>,
    path: Option<PathBuf>,
    bytes: u64,
}

#[derive(Clone)]
pub struct FileLogWriter(Arc<Mutex<LogFileState>>);

pub struct LogWriter(FileLogWriter);

impl<'a> MakeWriter<'a> for FileLogWriter {
    type Writer = LogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogWriter(self.clone())
    }
}

impl Write for LogWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let mut state = self
            .0
             .0
            .lock()
            .map_err(|_| io::Error::other("log writer lock poisoned"))?;
        state.ensure_file(buffer.len() as u64)?;
        let file = state
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("log file unavailable"))?;
        let written = file.write(buffer)?;
        state.bytes += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut state = self
            .0
             .0
            .lock()
            .map_err(|_| io::Error::other("log writer lock poisoned"))?;
        if let Some(file) = state.file.as_mut() {
            file.flush()?;
        }
        Ok(())
    }
}

impl LogFileState {
    fn ensure_file(&mut self, incoming_bytes: u64) -> io::Result<()> {
        let rotate =
            self.file.is_none() || self.bytes.saturating_add(incoming_bytes) > MAX_FILE_BYTES;
        if rotate {
            self.file = None;
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let path = self
                .directory
                .join(format!("starlane-notes-{timestamp}.log"));
            self.file = Some(OpenOptions::new().create(true).append(true).open(&path)?);
            self.path = Some(path);
            self.bytes = self
                .file
                .as_ref()
                .map(|file| file.metadata().map(|m| m.len()).unwrap_or(0))
                .unwrap_or(0);
            prune_logs(&self.directory, self.path.as_deref())?;
        }
        Ok(())
    }
}

pub fn init() -> io::Result<FileLogWriter> {
    let directory = data_dir().join("logging");
    fs::create_dir_all(&directory)?;
    prune_logs(&directory, None)?;
    Ok(FileLogWriter(Arc::new(Mutex::new(LogFileState {
        directory,
        file: None,
        path: None,
        bytes: 0,
    }))))
}

fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string()))
}

fn prune_logs(directory: &Path, active_path: Option<&Path>) -> io::Result<()> {
    let now = SystemTime::now();
    let mut files = fs::read_dir(directory)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("log") {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            Some((
                path,
                metadata.len(),
                metadata.modified().unwrap_or(UNIX_EPOCH),
            ))
        })
        .collect::<Vec<_>>();

    for (path, _, modified) in &files {
        if active_path.is_some_and(|active| active == path) {
            continue;
        }
        if now.duration_since(*modified).unwrap_or_default() > MAX_LOG_AGE {
            let _ = fs::remove_file(path);
        }
    }
    files.retain(|(path, _, _)| path.exists());

    let mut total = files.iter().map(|(_, bytes, _)| *bytes).sum::<u64>();
    files.sort_by_key(|(_, _, modified)| *modified);
    for (path, bytes, _) in files {
        if total <= MAX_LOG_BYTES || active_path.is_some_and(|active| active == path) {
            continue;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(bytes);
        }
    }
    Ok(())
}
