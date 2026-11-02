//! Read-only observations. Availability and applied history do not certify recovery.
use serde::Serialize;
use sqlx::Row;
use std::time::Duration;
use crate::{AppState, db::{DatabaseEngine, selected_migrator}};

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Serialize)]
pub(crate) struct StorageHealthReport {
    observed_at: String,
    database: DatabaseObservation,
    files: FileObservation,
    recorded_attachment_usage: AttachmentUsage,
    limits: LimitObservation,
    attachment_capacity: super::health_capacity::CapacityObservation,
    backup: RecoveryObservation,
    verified_restore: RecoveryObservation,
    application_restore_history: serde_json::Value,
}
#[derive(Serialize)]
struct DatabaseObservation {
    engine: &'static str,
    connection_status: &'static str,
    migration_history: MigrationObservation,
}
#[derive(Serialize)]
struct MigrationObservation {
    status: &'static str,
    expected_count: usize,
    applied_count: Option<usize>,
    scope: &'static str,
}
#[derive(Serialize)]
struct FileObservation {
    backend: &'static str,
    location: String,
    read_status: &'static str,
    integrity_status: &'static str,
}
#[derive(Serialize)]
struct AttachmentUsage {
    status: &'static str,
    attachments: Option<String>,
    bytes: Option<String>,
    scope: &'static str,
}
#[derive(Serialize)]
struct LimitObservation {
    max_upload_bytes: Option<String>,
    quota_status: &'static str,
}
#[derive(Serialize)]
struct RecoveryObservation {
    status: &'static str,
    completed_at: Option<String>,
}

fn history_matches(actual: &[(i64, Vec<u8>, i64)], expected: &[(i64, Vec<u8>)]) -> bool {
    actual.len() == expected.len() && actual.iter().zip(expected).all(|(a, e)| {
        a.0 == e.0 && a.1 == e.1 && a.2 == 1
    })
}

pub(crate) async fn observe(state: &AppState) -> StorageHealthReport {
    observe_with_capacity(state, super::health_capacity::observe(&state.db)).await
}
#[cfg(target_os = "linux")]
pub(crate) async fn observe_borrowed_historical(state: &AppState, purpose: &super::restoration::linux::BorrowedHistoricalHealthPurpose<'_>) -> std::io::Result<StorageHealthReport> {
    let capacity = super::health_capacity::CapacityObservation::historical_configuration(purpose, &state.db)?;
    Ok(observe_with_capacity(state, async { capacity }).await)
}
#[cfg(target_os = "linux")]
pub(crate) fn finalize_borrowed_historical(report: &mut StorageHealthReport, state: &AppState, purpose: &super::restoration::linux::BorrowedHistoricalHealthPurpose<'_>) -> std::io::Result<()> {
    // Called only after the request owner repeats the actual logical/native/SQL
    // observation. The purpose cannot be constructed from caller-provided totals.
    report.attachment_capacity = super::health_capacity::CapacityObservation::historical_configuration(purpose, &state.db)?;
    report.observed_at = chrono::Utc::now().to_rfc3339();
    Ok(())
}
async fn observe_with_capacity(state: &AppState, capacity: impl std::future::Future<Output = super::health_capacity::CapacityObservation>) -> StorageHealthReport {
    // Use the connected pool, never mutable process configuration or a serialized URL.
    let engine = DatabaseEngine::from_url(state.db.connect_options().database_url.as_str()).ok();
    let engine_name = match engine {
        Some(DatabaseEngine::Sqlite) => "sqlite",
        Some(DatabaseEngine::Postgres) => "postgresql",
        None => "unknown",
    };
    let connection = tokio::time::timeout(PROBE_TIMEOUT,
        sqlx::query_scalar::<_, i64>("SELECT 1").fetch_one(&state.db)).await;
    let connected = matches!(connection, Ok(Ok(1)));
    let expected: Vec<(i64, Vec<u8>)> = engine.map(|e| selected_migrator(e).iter()
        .map(|m| (m.version, m.checksum.to_vec())).collect()).unwrap_or_default();
    let mut migrations = MigrationObservation {
        status: "unavailable", expected_count: expected.len(), applied_count: None,
        scope: "applied_history_only",
    };
    if connected && engine.is_some() {
        if let Ok(Ok(rows)) = tokio::time::timeout(PROBE_TIMEOUT,
            sqlx::query_as::<_, (i64, Vec<u8>, i64)>(
                "SELECT version,checksum,CAST(success AS INTEGER) FROM _sqlx_migrations ORDER BY version"
            ).fetch_all(&state.db)).await {
            migrations.applied_count = Some(rows.len());
            migrations.status = if history_matches(&rows, &expected) { "matching" } else { "mismatch" };
        }
    }
    let root = crate::storage::data_dir();
    let readable = tokio::time::timeout(PROBE_TIMEOUT, async {
        let metadata = tokio::fs::metadata(&root).await?;
        if !metadata.is_dir() { return Err(std::io::Error::other("not a directory")); }
        // Opening a directory is a read probe, not a write or integrity guarantee.
        let mut entries = tokio::fs::read_dir(&root).await?;
        let _ = entries.next_entry().await?;
        Ok::<_, std::io::Error>(())
    }).await;
    let mut usage = AttachmentUsage { status: "unavailable", attachments: None, bytes: None,
        scope: "attachment_database_records_only" };
    if connected {
        if let Ok(Ok(row)) = tokio::time::timeout(PROBE_TIMEOUT, sqlx::query(
            "SELECT COUNT(*) AS attachment_count, CAST(COALESCE(SUM(size_bytes),0) AS BIGINT) AS attachment_bytes, COUNT(CASE WHEN size_bytes<0 THEN 1 END) AS invalid_sizes FROM attachments"
        ).fetch_one(&state.db)).await {
            if let (Ok(count), Ok(bytes), Ok(0)) = (row.try_get::<i64,_>("attachment_count"),
                row.try_get::<i64,_>("attachment_bytes"), row.try_get::<i64,_>("invalid_sizes")) {
                if count >= 0 && bytes >= 0 {
                    usage.status = "available";
                    usage.attachments = Some(count.to_string());
                    usage.bytes = Some(bytes.to_string());
                }
            }
        }
    }
    let limit = match tokio::time::timeout(PROBE_TIMEOUT,
        crate::spaces::attachments::max_upload_size_bytes(state)).await {
        Ok(Ok(bytes)) => Some(bytes.to_string()), _ => None,
    };
    let mut backup = RecoveryObservation { status: "unknown", completed_at: None };
    #[cfg(target_os = "linux")]
    if let Some(records) = std::env::var_os("STORAGE_OPERATION_RECORDS_DIR") {
        if engine == Some(DatabaseEngine::Sqlite) && connected {
            // Bind the actual connected database, not just DATA_DIR or a record path.
            let bound = match tokio::time::timeout(PROBE_TIMEOUT,
                sqlx::query("PRAGMA database_list").fetch_all(&state.db)).await {
                Ok(Ok(rows)) => rows.iter().any(|row| {
                    row.try_get::<String,_>("name").ok().as_deref() == Some("main") &&
                    row.try_get::<String,_>("file").ok().and_then(|file| std::fs::canonicalize(file).ok())
                        == std::fs::canonicalize(root.join("starlane-notes.db")).ok().filter(|_| root.join("starlane-notes.db").is_file())
                }), _ => false,
            };
            if let Some(source) = bound.then(|| super::recovery_records::pool_source(&state.db, &root)).flatten() {
                let records = std::path::PathBuf::from(records);
                match tokio::time::timeout(PROBE_TIMEOUT, tokio::task::spawn_blocking(move ||
                    super::recovery_records::read_bound(&records, &source))).await {
                    Ok(Ok(Ok(Some(time)))) => { backup.status = "recorded_completed"; backup.completed_at = Some(time); }
                    Ok(Ok(Ok(None))) => (),
                    _ => backup.status = "unavailable",
                }
            } else { backup.status = "unavailable"; }
        }
    }
    #[cfg(target_os="linux")]
    if engine==Some(DatabaseEngine::Postgres){let observed=super::restoration::linux::engine_transfer::observe_normal_pg_backup_history(state).await;backup.status=observed.status;backup.completed_at=observed.completed_at;}
    let capacity = capacity.await;
    StorageHealthReport {
        observed_at: chrono::Utc::now().to_rfc3339(),
        database: DatabaseObservation { engine: engine_name,
            connection_status: if connected { "available" } else { "unavailable" },
            migration_history: migrations },
        files: FileObservation { backend: "local", location: root.to_string_lossy().into_owned(),
            read_status: if matches!(readable, Ok(Ok(()))) { "available" } else { "unavailable" },
            integrity_status: "not_checked" },
        recorded_attachment_usage: usage,
        limits: LimitObservation { max_upload_bytes: limit, quota_status: capacity.quota_status },
        attachment_capacity: capacity,
        // Completed byte-backup history is separate from verified application restore.
        // Never infer restore success from health or restoration metadata.
        backup,
        verified_restore: RecoveryObservation { status: "unknown", completed_at: None },
        application_restore_history: application_history_observation(state).await,
    }
}

fn application_history_unavailable()->serde_json::Value{serde_json::json!({"status":"unavailable","completed_at":null,"scope":null,"operation":null,"pages":null,"drafts":null,"registered_assets":null,"legacy_assets":null,"exact_binary_bytes":null,"sampled_humans":null,"semantics":"historical_completed_isolated_application_test_at_recorded_time","full_supported_restore_verified":false})}
async fn application_history_observation(state:&AppState)->serde_json::Value{
 #[cfg(target_os="linux")]{if let Some(report)=super::restoration::linux::engine_transfer::observe_sqlite_ordinary_history(state).await{return report;}return serde_json::to_value(super::restoration::linux::engine_transfer::observe_normal_application_history(state).await).unwrap_or_else(|_|application_history_unavailable());}
 #[cfg(not(target_os="linux"))]{let _=state;application_history_unavailable()}
}
#[cfg(test)]mod application_history_shape_controls{
 #[test]fn unavailable_history_never_carries_completion_or_full_verification(){let value=super::application_history_unavailable();assert_eq!(value["status"],"unavailable");assert_eq!(value["full_supported_restore_verified"],false);assert_eq!(value["semantics"],"historical_completed_isolated_application_test_at_recorded_time");for key in ["completed_at","scope","operation","pages","drafts","registered_assets","legacy_assets","exact_binary_bytes","sampled_humans"]{assert!(value.get(key).is_some_and(serde_json::Value::is_null),"Unavailable history omitted or claimed {key}");}}
}
