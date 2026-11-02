//! Conservative accounting observation; never raw disk usage or cleanup authority.
use serde::Serialize;
#[cfg(target_os = "linux")]
use super::attachment_capacity::{configured_limit, usage_report};
struct UsageReport { retained_bytes: i64, logical_retained_bytes: i64, pending_bytes: i64 }

#[derive(Debug, Serialize)]
pub(super) struct CapacityObservation {
    pub quota_status: &'static str,
    pub quota_bytes: Option<String>,
    pub accounting_status: &'static str,
    pub logical_retained_bytes: Option<String>,
    pub charged_retained_bytes: Option<String>,
    pub reserved_maximum_bytes: Option<String>,
    pub charged_total_bytes: Option<String>,
    pub scope: &'static str,
    #[cfg(target_os = "linux")]
    #[serde(skip_serializing_if = "Option::is_none")]
    readonly_known_usage: Option<super::restoration::linux::ReadonlyKnownUsageObservation>,
}
impl CapacityObservation {
    fn configuration(limit: anyhow::Result<Option<i64>>) -> Self {
        let (quota_status, quota_bytes) = match limit {
            Ok(Some(bytes)) if bytes >= 0 => ("configured", Some(bytes.to_string())),
            Ok(None) => ("unlimited", None),
            _ => ("unavailable", None),
        };
        Self {quota_status, quota_bytes, accounting_status: "unavailable",
            logical_retained_bytes: None, charged_retained_bytes: None,
            reserved_maximum_bytes: None, charged_total_bytes: None,
            scope: "validated_attachment_charge_and_reservations",
            #[cfg(target_os = "linux")]
            readonly_known_usage: None}
    }
    #[cfg(target_os = "linux")]
    pub(super) fn historical_configuration(purpose: &super::restoration::linux::BorrowedHistoricalHealthPurpose<'_>, pool: &sqlx::AnyPool) -> std::io::Result<Self> {
        purpose.check_pool(pool)?;
        // No invocation of usage_report: that owner takes writer exclusions.
        let mut report = Self::configuration(configured_limit());
        report.readonly_known_usage = Some(purpose.observation()?);
        Ok(report)
    }
    fn accounting(mut self, report: UsageReport) -> Self {
        // Any malformed, undercharged or overflowing report remains unavailable.
        if report.logical_retained_bytes < 0 || report.retained_bytes < report.logical_retained_bytes
            || report.pending_bytes < 0 {return self;}
        let Some(total) = report.retained_bytes.checked_add(report.pending_bytes) else {return self;};
        self.accounting_status = "available";
        self.logical_retained_bytes = Some(report.logical_retained_bytes.to_string());
        self.charged_retained_bytes = Some(report.retained_bytes.to_string());
        self.reserved_maximum_bytes = Some(report.pending_bytes.to_string());
        self.charged_total_bytes = Some(total.to_string());
        self
    }
}
// The observation deadline bounds the async response, not a synchronous syscall
// or worker lifetime. Cancellation is noticed before queued work or at the next
// yield of a running report; dropping that future releases its owned evidence.
#[cfg(target_os = "linux")]
struct ObservationCancel {
    cancelled: std::sync::atomic::AtomicBool,
    wake: tokio::sync::Notify,
}
#[cfg(target_os = "linux")]
struct ObservationOwner(std::sync::Arc<ObservationCancel>);
#[cfg(target_os = "linux")]
impl Drop for ObservationOwner {
    fn drop(&mut self) {
        self.0.cancelled.store(true, std::sync::atomic::Ordering::SeqCst);
        // One worker waits; a retained permit also covers cancellation before
        // its cancellation future is first polled.
        self.0.wake.notify_one();
    }
}
#[cfg(target_os = "linux")]
async fn bounded_observation<T, F, Fut>(deadline: std::time::Duration, work: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> Fut + Send + 'static,
    Fut: std::future::Future<Output = anyhow::Result<T>> + Send + 'static,
{
    use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
    let cancellation = Arc::new(ObservationCancel {
        cancelled: AtomicBool::new(false), wake: tokio::sync::Notify::new(),
    });
    let _owner = ObservationOwner(cancellation.clone());
    let runtime = tokio::runtime::Handle::current();
    let worker_cancel = cancellation.clone();
    let worker = tokio::task::spawn_blocking(move || {
        if worker_cancel.cancelled.load(Ordering::SeqCst) { return None; }
        runtime.block_on(async {
            tokio::select! {
                biased;
                _ = async {
                    loop {
                        let notified = worker_cancel.wake.notified();
                        if worker_cancel.cancelled.load(Ordering::SeqCst) { return; }
                        notified.await;
                    }
                } => None,
                result = async {
                    if worker_cancel.cancelled.load(Ordering::SeqCst) { return None; }
                    work().await.ok()
                } => {
                    if worker_cancel.cancelled.load(Ordering::SeqCst) { None } else { result }
                }
            }
        })
    });
    match tokio::time::timeout(deadline, worker).await {
        Ok(Ok(result)) if !cancellation.cancelled.load(Ordering::SeqCst) => result,
        _ => None,
    }
}
pub(super) async fn observe(pool: &sqlx::AnyPool) -> CapacityObservation {
    #[cfg(not(target_os = "linux"))]
    {let _ = pool; CapacityObservation::configuration(Err(anyhow::anyhow!("Native accounting unavailable")))}
    #[cfg(target_os = "linux")]
    {
    let observation = CapacityObservation::configuration(configured_limit());
    // The accounting owner takes ordered exclusion locks and explicitly rolls
    // back. Timeout/error never implies zero or physical absence.
    let pool = pool.clone();
    match bounded_observation(std::time::Duration::from_secs(2), move || async move {
        usage_report(&pool).await
    }).await {
        Some(report) => observation.accounting(UsageReport {retained_bytes: report.retained_bytes,
            logical_retained_bytes: report.logical_retained_bytes, pending_bytes: report.pending_bytes}),
        None => observation,
    }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_configuration_and_unavailable_accounting_stay_independent() {
        let configured = CapacityObservation::configuration(Ok(Some(0)));
        assert_eq!(configured.quota_status, "configured");
        assert_eq!(configured.quota_bytes.as_deref(), Some("0"));
        assert_eq!(configured.accounting_status, "unavailable");
        assert!(configured.charged_total_bytes.is_none());
        assert_eq!(CapacityObservation::configuration(Ok(None)).quota_status, "unlimited");
        assert_eq!(CapacityObservation::configuration(Err(anyhow::anyhow!("bad configuration"))).quota_status, "unavailable");
    }
    #[test]
    fn capacity_charges_are_exact_conservative_and_malformed_evidence_is_unavailable() {
        let report = CapacityObservation::configuration(Ok(None)).accounting(UsageReport {
            retained_bytes: 9007199254740993, logical_retained_bytes: 3, pending_bytes: 7});
        assert_eq!(report.charged_total_bytes.as_deref(), Some("9007199254741000"));
        assert_eq!(report.charged_retained_bytes.as_deref(), Some("9007199254740993"));
        for (physical, logical, pending) in [(2,3,0),(-1,0,0),(3,-1,0),(3,3,-1),(i64::MAX,1,1)] {
            let report = CapacityObservation::configuration(Ok(None)).accounting(UsageReport {
                retained_bytes: physical, logical_retained_bytes: logical, pending_bytes: pending});
            assert_eq!(report.accounting_status, "unavailable");
            assert!(report.charged_total_bytes.is_none());
            assert!(report.logical_retained_bytes.is_none());
        }
    }
    #[cfg(target_os = "linux")]
    struct DroppedReport(Option<tokio::sync::oneshot::Sender<()>>);
    #[cfg(target_os = "linux")]
    impl Drop for DroppedReport {
        fn drop(&mut self) { if let Some(done) = self.0.take() { let _ = done.send(()); } }
    }
    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "current_thread")]
    async fn synchronous_observation_phase_does_not_block_deadline_or_heartbeat() {
        use std::sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc};
        use std::time::Duration;
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, blocked) = mpsc::channel();
        let (dropped, released) = tokio::sync::oneshot::channel();
        let late = Arc::new(AtomicBool::new(false));
        let later = late.clone();
        let observation = tokio::spawn(bounded_observation(Duration::from_millis(100), move || async move {
            let _guard = DroppedReport(Some(dropped));
            let _ = entered.send(());
            blocked.recv().unwrap(); // Genuine synchronous phase on the worker.
            tokio::task::yield_now().await;
            later.store(true, Ordering::SeqCst);
            Ok(7u8)
        }));
        started.await.unwrap();
        // The only async executor thread must remain runnable while work blocks.
        let heartbeat = tokio::spawn(async { tokio::task::yield_now().await; 1u8 });
        assert_eq!(heartbeat.await.unwrap(), 1);
        assert!(observation.await.unwrap().is_none());
        assert!(!late.load(Ordering::SeqCst));
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), released).await.unwrap().unwrap();
        assert!(!late.load(Ordering::SeqCst), "cancelled report advanced after its next yield");
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn cancelled_queued_observation_never_starts_usage_work() {
        use std::sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc};
        use std::time::Duration;
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all()
            .max_blocking_threads(1).build().unwrap();
        let (entered, started) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let occupying = runtime.spawn_blocking(move || { entered.send(()).unwrap(); blocked.recv().unwrap(); });
        started.recv_timeout(Duration::from_secs(2)).unwrap();
        let ran = Arc::new(AtomicBool::new(false));
        let worker_ran = ran.clone();
        let report = runtime.block_on(bounded_observation(Duration::from_millis(30), move || async move {
            worker_ran.store(true, Ordering::SeqCst); Ok(1u8)
        }));
        assert!(report.is_none());
        release.send(()).unwrap();
        runtime.block_on(occupying).unwrap();
        // FIFO blocking queue: this barrier follows the canceled observation.
        runtime.block_on(runtime.spawn_blocking(|| ())).unwrap();
        assert!(!ran.load(Ordering::SeqCst));
        runtime.shutdown_timeout(Duration::from_secs(2));
    }
    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "current_thread")]
    async fn dropping_observation_cancels_running_report_at_next_yield() {
        use std::sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc};
        use std::time::Duration;
        let (entered, started) = tokio::sync::oneshot::channel();
        let (release, blocked) = mpsc::channel();
        let (dropped, released) = tokio::sync::oneshot::channel();
        let late = Arc::new(AtomicBool::new(false)); let later = late.clone();
        let observation = tokio::spawn(bounded_observation(Duration::from_secs(60), move || async move {
            let _guard = DroppedReport(Some(dropped)); let _ = entered.send(());
            blocked.recv().unwrap(); tokio::task::yield_now().await;
            later.store(true, Ordering::SeqCst); Ok(1u8)
        }));
        started.await.unwrap(); observation.abort(); assert!(observation.await.unwrap_err().is_cancelled());
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), released).await.unwrap().unwrap();
        assert!(!late.load(Ordering::SeqCst));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "current_thread")]
    async fn actual_sqlite_usage_observation_refuses_foreign_blob_writer_without_inventing_zero() {
        use std::{fs, os::unix::fs::OpenOptionsExt, time::Duration};
        let _scope = crate::storage::fixture_root::FixtureRoot::private();
        let root = crate::storage::data_dir();
        sqlx::any::install_default_drivers();
        let pool = sqlx::any::AnyPoolOptions::new().max_connections(2)
            .after_connect(|connection, _| Box::pin(async move {
                sqlx::query("PRAGMA busy_timeout=10000").execute(&mut *connection).await?;
                sqlx::query("PRAGMA journal_mode=WAL").execute(connection).await?;
                Ok(())
            }))
            .connect(&format!("sqlite://{}?mode=rwc", root.join("health-fixture.sqlite").display()))
            .await.unwrap();
        crate::db::selected_migrator(crate::db::DatabaseEngine::Sqlite).run(&pool).await.unwrap();
        // Genuine migrated legacy metadata: no fabricated digest, upload receipt,
        // version/publication history or producer custody is introduced.
        let relative = "spaces/health/attachments/page/legacy.bin";
        sqlx::query("INSERT INTO attachment_retained_byte_ledger(asset_id,byte_length,filename,original_name,mime_type,uploader_id,original_created_at,original_storage_path,evidence_kind) VALUES('health-legacy',8,'legacy.bin','Legacy.bin','application/octet-stream','fixture-owner','original',$1,'legacy_upgrade')")
            .bind(relative).execute(&pool).await.unwrap();
        let blob = root.join(relative);
        fs::create_dir_all(blob.parent().unwrap()).unwrap();
        fs::write(&blob, b"ORIGINAL").unwrap();
        let report = super::usage_report(&pool).await.unwrap();
        assert_eq!((report.retained_bytes, report.logical_retained_bytes, report.pending_bytes), (8, 8, 0));
        let available = observe(&pool).await;
        assert_eq!(available.accounting_status, "available");
        assert_eq!(available.logical_retained_bytes.as_deref(), Some("8"));
        assert_eq!(available.charged_retained_bytes.as_deref(), Some("8"));
        assert_eq!(available.reserved_maximum_bytes.as_deref(), Some("0"));
        assert_eq!(available.charged_total_bytes.as_deref(), Some("8"));
        // Deferred release of the completed observation may still be finishing.
        // Nonblocking opens cannot stall this async test or mutate the blob.
        let writer = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                match fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(&blob) {
                    Ok(file) => break file,
                    Err(error) if error.raw_os_error() == Some(libc::EAGAIN) => tokio::time::sleep(Duration::from_millis(5)).await,
                    Err(error) => panic!("Foreign fixture writer could not open: {error}"),
                }
            }
        }).await.expect("Completed observation must release its blob custody");
        assert!(super::usage_report(&pool).await.is_err());
        let unavailable = observe(&pool).await;
        assert_eq!(unavailable.quota_status, available.quota_status);
        assert_eq!(unavailable.quota_bytes, available.quota_bytes);
        let json = serde_json::to_value(&unavailable).unwrap();
        assert_eq!(json["accounting_status"], "unavailable");
        for field in ["logical_retained_bytes", "charged_retained_bytes", "reserved_maximum_bytes", "charged_total_bytes"] {
            assert!(json[field].is_null(), "Unavailable accounting must retain null {field}");
        }
        assert_eq!(fs::read(&blob).unwrap(), b"ORIGINAL");
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachment_capacity_reservations").fetch_one(&pool).await.unwrap(), 0);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT byte_length FROM attachment_retained_byte_ledger WHERE asset_id='health-legacy'").fetch_one(&pool).await.unwrap(), 8);
        drop(writer);
        let recovered = observe(&pool).await;
        assert_eq!(recovered.accounting_status, "available");
        assert_eq!(recovered.charged_total_bytes.as_deref(), Some("8"));
        pool.close().await;
    }

}
