use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Pool, Row, Sqlite};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

use crate::vault;

/// Frontend capability discovery must not infer owner support from Tauri presence.
#[tauri::command]
pub(crate) fn get_native_attachment_capabilities() -> Value {
    json!({ "format": 1, "bounded_version_download": cfg!(target_os = "linux"), "bounded_version_preview": cfg!(target_os = "linux"), "receipt_bound_export": cfg!(target_os = "linux"), "complete_version_image_stream": cfg!(target_os="linux") })
}

const LOCAL_DATABASE_SCHEMA_VERSION: u64 = 1;
const COLLECTIONS: &[&str] = &[
    "spaces",
    "folders",
    "documents",
    "drafts",
    "assets",
    "templates",
    "workflow_templates",
    "document_workflows",
    "document_workflow_events",
    "property_schemas",
    "property_values",
    "tags",
    "doc_tags",
    "search_index",
    "reviewed_move_records",
    "reviewed_draft_transfer_records",
];

#[derive(Debug, Clone, Serialize)]
pub struct NativeLocalDatabaseDescriptor {
    schema_version: u64,
    provider: String,
    store_kind: String,
    database_path: String,
    supports_snapshot_bridge: bool,
    supports_atomic_snapshot_commit: bool,
    supports_lossless_replay_queue: bool,
    supports_reviewed_move_evidence: bool,
    supports_reviewed_move_destination_binding: bool,
    supports_reviewed_draft_transfer_evidence: bool,
    supports_operation_queue: bool,
    replay_backends: Vec<String>,
}

#[tauri::command]
pub fn describe_native_local_database(
    app: AppHandle,
    provider: String,
) -> Result<NativeLocalDatabaseDescriptor, String> {
    let database_path = database_path(&app, &provider)?;
    Ok(NativeLocalDatabaseDescriptor {
        schema_version: LOCAL_DATABASE_SCHEMA_VERSION,
        provider,
        store_kind: "sqlite".to_string(),
        database_path: database_path.to_string_lossy().to_string(),
        supports_snapshot_bridge: true,
        supports_atomic_snapshot_commit: true,
        supports_lossless_replay_queue: true,
        supports_reviewed_move_evidence: !cfg!(any(target_os="ios",target_os="android")),
        supports_reviewed_move_destination_binding: !cfg!(any(target_os="ios",target_os="android")),
        supports_reviewed_draft_transfer_evidence: true,
        supports_operation_queue: true,
        replay_backends: vec!["sqlite".to_string(), "postgresql".to_string()],
    })
}

// Serialize canonical snapshot commits with mirror work across native callers.
fn snapshot_commit_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[derive(Debug, Clone, Serialize)]
pub struct NativeSnapshotCommitReceipt {
    canonical_committed: bool,
    mirror_pending: bool,
    mirror_error: Option<String>,
    canonical_revision: String,
}

#[tauri::command]
pub async fn read_native_local_database_snapshot(
    app: AppHandle,
    provider: String,
    expected_destination: Option<NativeSnapshotExpectedDestination>,
) -> Result<Option<Value>, String> {
    if let Some(expected)=expected_destination.as_ref() {
        #[cfg(any(target_os="ios",target_os="android"))]
        {let _=expected;return Err("Reviewed native moves require an updated desktop shell".into());}
        #[cfg(not(any(target_os="ios",target_os="android")))]
        {
            let request=vault::capture_snapshot_request(&app,&provider)?;
            verify_reviewed_move_destination(&request,&provider,expected)?;
            return read_bound_snapshot_after_wait(&request,&provider,expected).await;
        }
    }
    let _guard = snapshot_commit_lock().lock().await;
    read_native_snapshot_inner(app, provider).await
}

#[cfg(not(any(target_os="ios",target_os="android")))]
async fn read_bound_snapshot_after_wait(
    request:&vault::SnapshotRequestBinding,provider:&str,expected:&NativeSnapshotExpectedDestination,
)->Result<Option<Value>,String> {
    let _guard=snapshot_commit_lock().lock().await;
    // Noncreating: an absent canonical DB cannot prove a committed candidate.
    verify_reviewed_move_destination(request,provider,expected)?;
    let path=request.expected_database_path()?;
    if !path.exists() {
        verify_reviewed_move_destination(request,provider,expected)?;
        return Ok(None);
    }
    let pool=open_database(&path).await?;
    migrate_database(&pool).await?;
    read_snapshot_from_pool_checked(&pool,provider,||verify_reviewed_move_destination(request,provider,expected)).await
}

async fn read_snapshot_from_pool_checked(
    pool:&Pool<Sqlite>,provider:&str,final_destination:impl FnOnce()->Result<(),String>,
)->Result<Option<Value>,String> {
    let result=read_snapshot_from_pool(pool,provider).await?;
    // Last synchronous proof follows ALL SQL waits; no current-root mirror
    // fallback may turn a foreign or merely portable snapshot into an ACK.
    final_destination()?;
    Ok(result)
}

async fn read_native_snapshot_inner(app: AppHandle, provider: String) -> Result<Option<Value>, String> {
    let database_path = database_path(&app, &provider)?;
    if !database_path.exists() {
        return read_legacy_snapshot_without_revision(app, provider);
    }

    let pool = open_database(&database_path).await?;
    migrate_database(&pool).await?;

    let snapshot=read_snapshot_from_pool(&pool,&provider).await?;
    if snapshot.is_some() {Ok(snapshot)} else {read_legacy_snapshot_without_revision(app,provider)}
}

async fn read_snapshot_from_pool(pool:&Pool<Sqlite>,provider:&str)->Result<Option<Value>,String> {
    let mut connection=pool.acquire().await.map_err(|e|e.to_string())?;
    read_snapshot_from_connection(&mut connection, provider).await
}

async fn read_snapshot_from_connection(connection:&mut sqlx::SqliteConnection,provider:&str)->Result<Option<Value>,String> {
    let has_snapshot: Option<(String,)> =
        sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key = 'schema_version'")
            .fetch_optional(&mut *connection)
            .await
            .map_err(|e| e.to_string())?;
    if has_snapshot.is_none() {
        return Ok(None);
    }

    let initial_revision: Option<(String,)> = sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key = 'canonical_revision'")
        .fetch_optional(&mut *connection).await.map_err(|error| error.to_string())?;
    let mut snapshot = Map::new();
    snapshot.insert(
        "schema_version".to_string(),
        json!(LOCAL_DATABASE_SCHEMA_VERSION),
    );

    for collection in COLLECTIONS {
        let values = match *collection {
            "spaces" => {
                read_normalized_payload_collection_connection(connection, "local_spaces", "name, id").await?
            }
            "documents" => {
                read_normalized_payload_collection_connection(connection,
                    "local_documents",
                    "space_id, folder_id, title, id",
                )
                .await?
            }
            "drafts" => read_snapshot_bridge_collection_connection(connection, collection).await?,
            "folders" => {
                read_normalized_payload_collection_connection(connection,
                    "local_folders",
                    "space_id, parent_id, name, id",
                )
                .await?
            }
            "tags" => {
                read_normalized_payload_collection_connection(connection, "local_tags", "space_id, name, id")
                    .await?
            }
            "property_schemas" => {
                read_normalized_payload_collection_connection(connection, "local_properties", "space_id, name, id")
                    .await?
            }
            "assets" => {
                read_normalized_payload_collection_connection(connection,
                    "local_assets",
                    "space_id, document_id, name, id",
                )
                .await?
            }
            "templates" => {
                read_normalized_payload_collection_connection(connection, "local_templates", "space_id, title, id")
                    .await?
            }
            "workflow_templates" => {
                read_normalized_payload_collection_connection(connection,
                    "local_workflow_templates",
                    "space_id, name, id",
                )
                .await?
            }
            "document_workflows" => {
                read_normalized_payload_collection_connection(connection,
                    "local_document_workflows",
                    "space_id, document_id, id",
                )
                .await?
            }
            "document_workflow_events" => {
                read_normalized_payload_collection_connection(connection,
                    "local_document_workflow_events",
                    "document_id, created_at, id",
                )
                .await?
            }
            "property_values" => {
                read_normalized_payload_collection_connection(connection,
                    "local_property_values",
                    "document_id, property_id, id",
                )
                .await?
            }
            "doc_tags" => {
                read_normalized_payload_collection_connection(connection,
                    "local_doc_tags",
                    "document_id, tag_id, id",
                )
                .await?
            }
            "search_index" => {
                read_normalized_payload_collection_connection(connection,
                    "local_search_index",
                    "space_id, updated_at, document_id",
                )
                .await?
            }
            _ => Vec::new(),
        };
        let values = if values.is_empty() {
            read_snapshot_bridge_collection_connection(connection, collection).await?
        } else {
            values
        };
        snapshot.insert((*collection).to_string(), Value::Array(values));
    }

    if let Some(sync_state) = read_normalized_sync_state_connection(connection, provider).await? {
        snapshot.insert("sync_state".to_string(), sync_state);
    } else if let Some((raw_sync_state,)) = sqlx::query_as::<_, (String,)>(
        "SELECT value FROM local_vault_metadata WHERE key = 'sync_state'",
    )
    .fetch_optional(&mut *connection)
    .await
    .map_err(|e| e.to_string())?
    {
        if let Ok(value) = serde_json::from_str::<Value>(&raw_sync_state) {
            snapshot.insert("sync_state".to_string(), value);
        }
    }

    if let Some(catalogs) = read_maintenance_catalogs_connection(connection).await? {
        snapshot.insert("maintenance_target_catalogs".to_string(), catalogs);
    }

    let revision: Option<(String,)> = sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key = 'canonical_revision'")
        .fetch_optional(&mut *connection).await.map_err(|error| error.to_string())?;
    if revision != initial_revision {
        return Err("Local vault changed while reading; reload the committed snapshot and retry".into());
    }
    snapshot.insert("canonical_revision".into(), revision.map(|row| Value::String(row.0)).unwrap_or(Value::Null));
    let pending: Option<(String,)> = sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key = 'mirror_pending'")
        .fetch_optional(&mut *connection).await.map_err(|error| error.to_string())?;
    snapshot.insert("mirror_pending".into(), Value::Bool(pending.is_some()));
    Ok(Some(Value::Object(snapshot)))
}

fn read_legacy_snapshot_without_revision(app: AppHandle, provider: String) -> Result<Option<Value>, String> {
    let mut snapshot = vault::read_vault_snapshot(app, provider)?;
    if let Some(Value::Object(snapshot)) = &mut snapshot {
        // A portable mirror is not evidence that a canonical SQLite revision exists.
        snapshot.insert("canonical_revision".into(), Value::Null);
        snapshot.insert("mirror_pending".into(), Value::Bool(false));
    }
    Ok(snapshot)
}

async fn read_snapshot_bridge_collection(
    pool: &Pool<Sqlite>,
    collection: &str,
) -> Result<Vec<Value>, String> {
    let mut connection=pool.acquire().await.map_err(|e|e.to_string())?;
    read_snapshot_bridge_collection_connection(&mut connection, collection).await
}

async fn read_snapshot_bridge_collection_connection(
    connection: &mut sqlx::SqliteConnection,
    collection: &str,
) -> Result<Vec<Value>, String> {
    // Sort only row identities. Including large payloads in the sorter can
    // spill to an ambient temporary file, which custody-owned VFS readers refuse.
    // These rowids are query cursors, never native ownership evidence. The
    // caller's existing transaction/revision fences still bind the snapshot.
    let keys: Vec<i64> = sqlx::query_scalar(
        "SELECT rowid FROM local_vault_records WHERE collection = ?1 ORDER BY row_order, id",
    ).bind(collection).fetch_all(&mut *connection).await.map_err(|e| e.to_string())?;
    let mut records = Vec::with_capacity(keys.len());
    for key in keys {
        records.push(sqlx::query("SELECT id,payload FROM local_vault_records WHERE rowid=?1 AND collection=?2")
            .bind(key).bind(collection).fetch_one(&mut *connection).await.map_err(|e|e.to_string())?);
    }

    if collection == "reviewed_move_records" {
        let values:Vec<Value> = records.iter().map(|row| {
            let payload:String=row.get("payload");
            let value:Value=serde_json::from_str(&payload).map_err(|_|"Reviewed move evidence is malformed; retain the vault for recovery".to_string())?;
            let id:String=row.get("id");
            if value.get("id").and_then(Value::as_str)!=Some(id.as_str()) {return Err("Reviewed move evidence identity changed; retain the vault for recovery".to_string());}
            Ok(value)
        }).collect::<Result<_,_>>()?;
        validate_reviewed_move_records(&values)?;
        return Ok(values);
    }
    if collection == "reviewed_draft_transfer_records" {
        let values:Vec<Value> = records.iter().map(|row| {
            let payload:String=row.get("payload");
            let value:Value=serde_json::from_str(&payload).map_err(|_|"Reviewed draft transfer evidence is malformed; retain the vault for recovery".to_string())?;
            let id:String=row.get("id");
            if value.get("operation_id").and_then(Value::as_str)!=Some(id.as_str()) {return Err("Reviewed draft transfer evidence identity changed; retain the vault for recovery".to_string());}
            Ok(value)
        }).collect::<Result<_,_>>()?;
        validate_reviewed_draft_transfer_records(&values)?;
        return Ok(values);
    }
    Ok(records
        .into_iter()
        .filter_map(|row| {
            let payload: String = row.get("payload");
            serde_json::from_str::<Value>(&payload).ok()
        })
        .collect())
}

async fn read_normalized_payload_collection(
    pool: &Pool<Sqlite>,
    table: &str,
    order_by: &str,
) -> Result<Vec<Value>, String> {
    let mut connection=pool.acquire().await.map_err(|e|e.to_string())?;
    read_normalized_payload_collection_connection(&mut connection, table, order_by).await
}

async fn read_normalized_payload_collection_connection(
    connection: &mut sqlx::SqliteConnection,
    table: &str,
    order_by: &str,
) -> Result<Vec<Value>, String> {
    // Preserve SQLite's original ordering without putting whole Markdown,
    // queue or search payloads into its temporary-file sorter.
    let keys: Vec<i64> = sqlx::query_scalar(&format!("SELECT rowid FROM {table} ORDER BY {order_by}"))
        .fetch_all(&mut *connection).await.map_err(|e| e.to_string())?;
    let mut rows = Vec::with_capacity(keys.len());
    for key in keys {
        rows.push(sqlx::query(&format!("SELECT payload FROM {table} WHERE rowid=?1"))
            .bind(key).fetch_one(&mut *connection).await.map_err(|e|e.to_string())?);
    }

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let payload: String = row.get("payload");
            serde_json::from_str::<Value>(&payload).ok()
        })
        .collect())
}

#[tauri::command]
pub async fn write_native_local_database_snapshot(
    app: AppHandle,
    provider: String,
    snapshot: Value,
) -> Result<(), String> {
    // Compatibility callers must not interpret a postcommit mirror error as a rollback.
    #[cfg(not(any(target_os="ios",target_os="android")))]
    let request=vault::capture_snapshot_request(&app,&provider)?;
    #[cfg(not(any(target_os="ios",target_os="android")))]
    let (_guard,path)=snapshot_database_after_wait(&request).await?;
    #[cfg(any(target_os="ios",target_os="android"))]
    let _guard=snapshot_commit_lock().lock().await;
    #[cfg(any(target_os="ios",target_os="android"))]
    let path=database_path(&app,&provider)?;
    let pool = open_database(&path).await?;
    migrate_database(&pool).await?;
    let revision = write_snapshot_to_database(&pool, &provider, &snapshot).await?;
    let _receipt = finish_snapshot_mirror(&pool, &revision, || {
        #[cfg(not(any(target_os="ios",target_os="android")))]
        {request.write_mirror(&snapshot)}
        #[cfg(any(target_os="ios",target_os="android"))]
        {vault::write_vault_snapshot(app,provider.clone(),snapshot)}
    }).await;
    Ok(())
}

/// Original accepted preview destination, not a newly queried commit baseline.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSnapshotExpectedDestination {
    schema_version: u64,
    provider: String,
    store_kind: String,
    database_path: String,
    supports_reviewed_move_evidence: bool,
}

#[cfg(not(any(target_os="ios",target_os="android")))]
fn verify_reviewed_move_destination(
    request: &vault::SnapshotRequestBinding,
    provider: &str,
    expected: &NativeSnapshotExpectedDestination,
) -> Result<(), String> {
    if expected.schema_version != LOCAL_DATABASE_SCHEMA_VERSION
        || provider != "none" || expected.provider != provider
        || expected.store_kind != "sqlite" || !expected.supports_reviewed_move_evidence
        || expected.database_path.is_empty()
        || request.expected_database_path()?.to_string_lossy() != expected.database_path
    {
        return Err("Reviewed move destination changed; review the original vault again".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn commit_native_local_database_snapshot(
    app: AppHandle,
    provider: String,
    snapshot: Value,
    expected_revision: Option<String>,
    expected_destination: Option<NativeSnapshotExpectedDestination>,
) -> Result<NativeSnapshotCommitReceipt, String> {
    #[cfg(not(any(target_os="ios",target_os="android")))]
    let request=vault::capture_snapshot_request(&app,&provider)?;
    #[cfg(not(any(target_os="ios",target_os="android")))]
    if let Some(expected)=expected_destination.as_ref() {
        verify_reviewed_move_destination(&request,&provider,expected)?;
    }
    #[cfg(any(target_os="ios",target_os="android"))]
    if expected_destination.is_some() {
        return Err("Reviewed native moves require an updated desktop shell".into());
    }
    #[cfg(not(any(target_os="ios",target_os="android")))]
    let (_guard,path)=snapshot_candidate_database_after_wait(&request,&snapshot).await?;
    #[cfg(not(any(target_os="ios",target_os="android")))]
    if let Some(expected)=expected_destination.as_ref() {
        verify_reviewed_move_destination(&request,&provider,expected)?;
    }
    #[cfg(any(target_os="ios",target_os="android"))]
    let _guard=snapshot_commit_lock().lock().await;
    #[cfg(any(target_os="ios",target_os="android"))]
    vault::preflight_vault_snapshot(&app,&snapshot)?;
    #[cfg(any(target_os="ios",target_os="android"))]
    let path=database_path(&app,&provider)?;
    let pool = open_database(&path).await?;
    migrate_database(&pool).await?;
    let revision=write_snapshot_with_revision_checked(
        &pool,&provider,&snapshot,expected_revision.as_deref(),true,|| {
            #[cfg(not(any(target_os="ios",target_os="android")))]
            {
                if let Some(expected)=expected_destination.as_ref() {
                    verify_reviewed_move_destination(&request,&provider,expected)?;
                }
                request.preflight_snapshot(&snapshot)
            }
            #[cfg(any(target_os="ios",target_os="android"))]
            {vault::preflight_vault_snapshot(&app,&snapshot)}
        },
    ).await?;
    Ok(finish_snapshot_mirror(&pool,&revision,|| {
        #[cfg(not(any(target_os="ios",target_os="android")))]
        {request.write_mirror(&snapshot)}
        #[cfg(any(target_os="ios",target_os="android"))]
        {vault::write_vault_snapshot(app,provider.clone(),snapshot.clone())}
    }).await)
}

#[cfg(not(any(target_os="ios",target_os="android")))]
async fn snapshot_database_after_wait(request:&vault::SnapshotRequestBinding)->Result<(tokio::sync::MutexGuard<'static,()>,PathBuf),String> {
    let guard=snapshot_commit_lock().lock().await;
    // Verify immediately after the actual shared snapshot wait, before any path
    // creation, legacy rename or database open; consume ONLY the captured root.
    let path=request.database_path()?;
    Ok((guard,path))
}

#[cfg(not(any(target_os="ios",target_os="android")))]
async fn snapshot_candidate_database_after_wait(request:&vault::SnapshotRequestBinding,snapshot:&Value)->Result<(tokio::sync::MutexGuard<'static,()>,PathBuf),String> {
    let guard=snapshot_commit_lock().lock().await;
    request.preflight_snapshot(snapshot)?;
    let path=request.database_path()?;
    Ok((guard,path))
}

#[cfg(test)]
async fn commit_snapshot_and_mirror(
    pool: &Pool<Sqlite>,
    provider: &str,
    snapshot: &Value,
    expected_revision: Option<&str>,
    mirror: impl FnOnce() -> Result<(), String>,
) -> Result<NativeSnapshotCommitReceipt, String> {
    let revision = write_snapshot_with_revision(pool, provider, snapshot, expected_revision, true).await?;
    // From here canonical data and its replay queue are durably committed. Failures
    // updating derived JSON/Markdown cannot turn this into a rejected mutation.
    Ok(finish_snapshot_mirror(pool, &revision, mirror).await)
}

async fn finish_snapshot_mirror(
    pool: &Pool<Sqlite>,
    revision: &str,
    mirror: impl FnOnce() -> Result<(), String>,
) -> NativeSnapshotCommitReceipt {
    let result = match mirror() {
        Ok(()) => {
            let cleared = sqlx::query("DELETE FROM local_vault_metadata WHERE key = 'mirror_pending' AND value = ?1")
                .bind(revision).execute(pool).await.map_err(|error| error.to_string());
            match cleared {
                Err(error) => Err(error),
                Ok(_) => match sqlx::query_as::<_, (String,)>("SELECT value FROM local_vault_metadata WHERE key = 'mirror_pending'")
                    .fetch_optional(pool).await {
                    Ok(None) => Ok(()),
                    Ok(Some(_)) => Err("A newer canonical snapshot is awaiting mirror refresh".into()),
                    Err(error) => Err(error.to_string()),
                },
            }
        },
        Err(error) => Err(error),
    };
    NativeSnapshotCommitReceipt {
        canonical_committed: true,
        mirror_pending: result.is_err(),
        mirror_error: result.err(),
        canonical_revision: revision.to_string(),
    }
}

#[tauri::command]
pub async fn upsert_native_local_database_record(
    app: AppHandle,
    provider: String,
    collection: String,
    id: String,
    payload: Value,
    row_order: Option<i64>,
) -> Result<(), String> {
    let _guard = snapshot_commit_lock().lock().await;
    let database_path = database_path(&app, &provider)?;
    let pool = open_database(&database_path).await?;
    migrate_database(&pool).await?;

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO local_vault_metadata (key, value) VALUES ('schema_version', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(LOCAL_DATABASE_SCHEMA_VERSION.to_string())
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    upsert_snapshot_bridge_record(&mut tx, &collection, &id, &payload, row_order).await?;
    upsert_normalized_record(&mut tx, &provider, &collection, &id, &payload).await?;
    stamp_canonical_revision(&mut tx).await?;
    tx.commit().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_native_local_database_record(
    app: AppHandle,
    provider: String,
    collection: String,
    id: String,
) -> Result<(), String> {
    let _guard = snapshot_commit_lock().lock().await;
    let database_path = database_path(&app, &provider)?;
    let pool = open_database(&database_path).await?;
    migrate_database(&pool).await?;

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    delete_snapshot_bridge_and_normalized_record(&mut tx, &collection, &id).await?;
    stamp_canonical_revision(&mut tx).await?;
    tx.commit().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_native_local_database_mirror(
    app: AppHandle,
    provider: String,
) -> Result<(), String> {
    let _guard = snapshot_commit_lock().lock().await;
    let snapshot = read_native_snapshot_inner(app.clone(), provider.clone())
        .await?
        .ok_or_else(|| "No native local database snapshot was found".to_string())?;
    let pool = open_database(&database_path(&app, &provider)?).await?;
    migrate_database(&pool).await?;
    let revision: Option<(String,)> = sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key = 'mirror_pending'")
        .fetch_optional(&pool).await.map_err(|error| error.to_string())?;
    let revision = revision.map(|row| row.0).unwrap_or_default();
    let receipt = finish_snapshot_mirror(&pool, &revision, || vault::write_vault_snapshot(app, provider.clone(), snapshot)).await;
    receipt.mirror_error.map_or(Ok(()), Err)
}

#[tauri::command]
pub async fn reset_native_local_database(app: AppHandle, provider: String) -> Result<(), String> {
    let _guard = snapshot_commit_lock().lock().await;
    vault::preflight_vault_mirror(&app)?;
    let database_path = database_path(&app, &provider)?;
    #[cfg(all(unix, not(any(target_os="ios",target_os="android"))))]
    joint_archive::reset_allowed(&database_path).await?;
    if database_path.exists() {
        std::fs::remove_file(&database_path).map_err(|e| e.to_string())?;
    }
    vault::reset_vault_snapshot(app, provider)
}

fn database_path(app: &AppHandle, provider: &str) -> Result<PathBuf, String> {
    vault::resolve_native_database_path(app, provider)
}

async fn open_database(database_path: &PathBuf) -> Result<Pool<Sqlite>, String> {
    if let Some(parent) = database_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|e| e.to_string())
}

async fn migrate_database(pool: &Pool<Sqlite>) -> Result<(), String> {
    let mut connection=pool.acquire().await.map_err(|e|e.to_string())?;
    migrate_database_connection(&mut connection).await
}

async fn migrate_database_connection(connection: &mut sqlx::SqliteConnection) -> Result<(), String> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_vault_metadata (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_vault_records (
            collection TEXT NOT NULL,
            id TEXT NOT NULL,
            row_order INTEGER NOT NULL,
            payload TEXT NOT NULL,
            updated_at TEXT,
            PRIMARY KEY (collection, id)
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_spaces (
            id TEXT PRIMARY KEY NOT NULL,
            name TEXT NOT NULL,
            updated_at TEXT,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_documents (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            title TEXT NOT NULL,
            folder_id TEXT,
            updated_at TEXT,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_folders (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            parent_id TEXT,
            name TEXT NOT NULL,
            updated_at TEXT,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_tags (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            name TEXT NOT NULL,
            color TEXT,
            updated_at TEXT,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_properties (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            name TEXT NOT NULL,
            property_type TEXT NOT NULL,
            updated_at TEXT,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_assets (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT,
            document_id TEXT,
            name TEXT NOT NULL,
            updated_at TEXT,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_templates (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            title TEXT NOT NULL,
            updated_at TEXT,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_workflow_templates (
            id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            name TEXT NOT NULL,
            updated_at TEXT,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_document_workflows (
            id TEXT PRIMARY KEY NOT NULL,
            document_id TEXT NOT NULL,
            space_id TEXT,
            workflow_template_id TEXT,
            updated_at TEXT,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_document_workflow_events (
            id TEXT PRIMARY KEY NOT NULL,
            document_id TEXT NOT NULL,
            workflow_id TEXT,
            created_at TEXT,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_property_values (
            id TEXT PRIMARY KEY NOT NULL,
            document_id TEXT NOT NULL,
            property_id TEXT NOT NULL,
            updated_at TEXT,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_doc_tags (
            id TEXT PRIMARY KEY NOT NULL,
            document_id TEXT NOT NULL,
            tag_id TEXT NOT NULL,
            space_id TEXT,
            updated_at TEXT,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_search_index (
            document_id TEXT PRIMARY KEY NOT NULL,
            space_id TEXT NOT NULL,
            title TEXT NOT NULL,
            updated_at TEXT,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_queued_operations (
            id TEXT PRIMARY KEY NOT NULL,
            entity_type TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            action TEXT NOT NULL,
            queued_at TEXT NOT NULL,
            attempts INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            payload TEXT
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_sync_cursors (
            provider TEXT PRIMARY KEY NOT NULL,
            sync_state TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_replay_checkpoints (
            id TEXT PRIMARY KEY NOT NULL,
            provider TEXT NOT NULL,
            space_id TEXT,
            source_server_url TEXT,
            downloaded_by_user_id TEXT,
            downloaded_at TEXT,
            payload TEXT NOT NULL
        )",
    )
    .execute(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

async fn write_snapshot_to_database(
    pool: &Pool<Sqlite>,
    provider: &str,
    snapshot: &Value,
) -> Result<String, String> {
    write_snapshot_with_revision(pool, provider, snapshot, None, false).await
}

async fn stamp_canonical_revision(tx: &mut sqlx::Transaction<'_, Sqlite>) -> Result<String, String> {
    let revision = format!("{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_nanos());
    for key in ["canonical_revision", "mirror_pending"] {
        sqlx::query("INSERT INTO local_vault_metadata (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value")
            .bind(key).bind(&revision).execute(&mut **tx).await.map_err(|error| error.to_string())?;
    }
    Ok(revision)
}

async fn write_snapshot_with_revision(
    pool: &Pool<Sqlite>, provider: &str, snapshot: &Value, expected_revision: Option<&str>, enforce_revision: bool,
) -> Result<String, String> {
    write_snapshot_with_revision_checked(pool,provider,snapshot,expected_revision,enforce_revision,||Ok(())).await
}

async fn write_snapshot_with_revision_checked(
    pool: &Pool<Sqlite>, provider: &str, snapshot: &Value, expected_revision: Option<&str>, enforce_revision: bool,
    final_destination: impl FnOnce()->Result<(),String>,
) -> Result<String,String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    if enforce_revision {
        let current: Option<(String,)> = sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key = 'canonical_revision'")
            .fetch_optional(&mut *tx).await.map_err(|error| error.to_string())?;
        if current.as_ref().map(|row| row.0.as_str()) != expected_revision {
            return Err("Local vault changed in another window; reload the committed snapshot and retry".into());
        }
    }

    let revision = write_snapshot_in_tx(&mut tx, provider, snapshot).await?;
    // All snapshot SQL awaits completed. A failed original-destination proof
    // discards this transaction; no mirror callback runs before known commit.
    final_destination()?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(revision)
}

/// Write snapshot SQL inside a caller-owned transaction; the revision is pending,
/// never evidence of commit. Commit successfully before mirror/receipt claims.
/// On ANY helper or subsequent caller error, the caller MUST roll back or discard
/// the transaction and MUST NOT commit it. SQLite statement constraint errors do
/// not undo prior writes; this helper does not perform transaction rollback.
pub(crate) async fn write_snapshot_in_tx(
    tx: &mut sqlx::Transaction<'_, Sqlite>, provider: &str, snapshot: &Value,
) -> Result<String,String> {
    if let Some(records)=snapshot.get("reviewed_move_records") {
        let records=records.as_array().ok_or("Reviewed move evidence must be an array")?;
        validate_reviewed_move_records(records)?;
    }
    if let Some(records)=snapshot.get("reviewed_draft_transfer_records") {
        let records=records.as_array().ok_or("Reviewed draft transfer evidence must be an array")?;
        validate_reviewed_draft_transfer_records(records)?;
    }
    sqlx::query("DELETE FROM local_vault_metadata")
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM local_vault_records")
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    for table in [
        "local_spaces",
        "local_documents",
        "local_folders",
        "local_tags",
        "local_properties",
        "local_assets",
        "local_templates",
        "local_workflow_templates",
        "local_document_workflows",
        "local_document_workflow_events",
        "local_property_values",
        "local_doc_tags",
        "local_search_index",
        "local_queued_operations",
        "local_sync_cursors",
        "local_replay_checkpoints",
    ] {
        sqlx::query(&format!("DELETE FROM {table}"))
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
    }

    sqlx::query("INSERT INTO local_vault_metadata (key, value) VALUES ('schema_version', ?1)")
        .bind(LOCAL_DATABASE_SCHEMA_VERSION.to_string())
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(sync_state) = snapshot.get("sync_state") {
        sqlx::query("INSERT INTO local_vault_metadata (key, value) VALUES ('sync_state', ?1)")
            .bind(serde_json::to_string(sync_state).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
    }

    if let Some(catalogs) = snapshot.get("maintenance_target_catalogs") {
        if !catalogs.is_array() { return Err("Maintenance target catalogs are malformed".into()); }
        sqlx::query("INSERT INTO local_vault_metadata(key,value) VALUES('maintenance_target_catalogs',?1)")
            .bind(serde_json::to_string(catalogs).map_err(|e| e.to_string())?).execute(&mut **tx).await.map_err(|e| e.to_string())?;
    }

    for collection in COLLECTIONS {
        let Some(items) = snapshot.get(*collection).and_then(Value::as_array) else {
            continue;
        };
        for (index, item) in items.iter().enumerate() {
            let id = record_id(collection, item, index);
            let updated_at = item
                .get("updated_at")
                .or_else(|| item.get("created_at"))
                .and_then(Value::as_str)
                .map(str::to_string);
            sqlx::query(
                "INSERT INTO local_vault_records
                    (collection, id, row_order, payload, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(collection)
            .bind(id)
            .bind(index as i64)
            .bind(serde_json::to_string(item).map_err(|e| e.to_string())?)
            .bind(updated_at)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    write_normalized_snapshot_tables(tx, provider, snapshot).await?;
    sqlx::query("INSERT INTO local_vault_metadata(key,value) VALUES('replay_queue_order_format','1:array')")
        .execute(&mut **tx).await.map_err(|e|e.to_string())?;
    let revision = stamp_canonical_revision(tx).await?;

    Ok(revision)
}

fn replay_queue_immutable_tuple_matches(raw: &Value, row: &sqlx::sqlite::SqliteRow) -> Result<bool,String> {
    for field in ["id","entity_type","entity_id","action","queued_at"] {
        if raw.get(field).and_then(Value::as_str) != Some(row.get::<String,_>(field).as_str()) { return Ok(false); }
    }
    let payload=row.get::<Option<String>,_>("payload").map(|value|serde_json::from_str::<Value>(&value).map_err(|_|"Replay operation payload is malformed".to_string())).transpose()?;
    Ok(raw.get("payload")==payload.as_ref())
}

// A stale immutable tuple is not current evidence. A CURRENT tuple's invalid
// present metadata is corruption, never permission to downgrade to unbound.
fn current_replay_cursor_witness(cursor: &[Value], row: &sqlx::sqlite::SqliteRow) -> Result<Option<Value>,String> {
    let id=row.get::<String,_>("id");
    let Some(raw)=cursor.iter().find(|raw|raw.get("id").and_then(Value::as_str)==Some(id.as_str())) else { return Ok(None); };
    if !replay_queue_immutable_tuple_matches(raw,row)? { return Ok(None); }
    require_replay_queue_witness(raw,row)?;
    Ok(Some(raw.clone()))
}

fn require_replay_queue_witness(
    raw: &Value,
    row: &sqlx::sqlite::SqliteRow,
) -> Result<Map<String, Value>, String> {
    let action = row.get::<String, _>("action");
    if row.get::<i64,_>("attempts") < 0 { return Err("Replay attempts are invalid".into()); }
    let payload = row.get::<Option<String>, _>("payload").map(|value|
        serde_json::from_str::<Value>(&value).map_err(|_| "Replay operation payload is malformed".to_string())
    ).transpose()?;
    for field in ["id", "entity_type", "entity_id", "action", "queued_at"] {
        let raw_field = raw.get(field).and_then(Value::as_str).filter(|value| !value.trim().is_empty());
        if raw_field.is_none() || raw_field != Some(row.get::<String, _>(field).as_str()) {
            return Err("Replay operation evidence differs from its canonical tuple".to_string());
        }
    }
    if raw.get("payload") != payload.as_ref() {
        return Err("Replay operation payload evidence differs".to_string());
    }
    let value = raw.as_object().cloned().ok_or_else(|| "Replay operation evidence is malformed".to_string())?;
    if let Some(provider) = value.get("replay_provider") {
        if !provider.as_str().is_some_and(|provider| ["none", "icloud-drive", "google-drive"].contains(&provider)) {
            return Err("Replay provider binding is invalid".to_string());
        }
    }
    if let Some(context) = value.get("server_context") {
        if !context.is_object() || ["server_base", "account_id", "session_scope"].iter().any(|field|
            !context.get(*field).and_then(Value::as_str).is_some_and(|value| !value.trim().is_empty())) {
            return Err("Replay account binding is invalid".to_string());
        }
    }
    if matches!(action.as_str(), "update_identifier" | "update_maintenance") && !value.contains_key("replay_provider") {
        return Err("Identifier recovery provider binding is missing".into());
    }
    if value.get("attempts").is_some_and(|value| !value.as_i64().is_some_and(|attempts| attempts >= 0)) {
        return Err("Replay attempts evidence is invalid".into());
    }
    if value.get("last_error").is_some_and(|value| !value.is_string()) {
        return Err("Replay error evidence is invalid".into());
    }
    Ok(value)
}

// Legacy partial writes used zero for every bridge position. A complete current
// cursor can prove order; otherwise the incumbent queued_at/id read/dispatch
// order is compatibility behavior, NOT evidence of authored historical order.
fn replay_queue_order_plan(
    rows: &[sqlx::sqlite::SqliteRow],
    raw: &std::collections::HashMap<String, Value>,
    orders: &std::collections::HashMap<String, i64>,
    cursor: &[Value],
    marker: Option<&str>,
) -> Result<(std::collections::HashMap<String, i64>, &'static str), String> {
    if let Some(marker) = marker {
        if !["1:array", "1:legacy_cursor", "1:legacy_dispatch"].contains(&marker) {
            return Err("Replay queue order format is invalid".into());
        }
        if raw.len() != rows.len() || orders.len() != rows.len() {
            return Err("Replay queue order evidence is incomplete".into());
        }
        let mut used = std::collections::HashSet::new();
        for row in rows {
            let id = row.get::<String, _>("id");
            require_replay_queue_witness(raw.get(&id).ok_or("Replay queue order evidence is incomplete")?, row)?;
            let order = *orders.get(&id).ok_or("Replay queue order evidence is incomplete")?;
            if order < 0 || !used.insert(order) { return Err("Replay queue order evidence is ambiguous".into()); }
        }
        return Ok((orders.clone(), "verified"));
    }
    // Every retained bridge must still match its current normalized tuple.
    for (id, witness) in raw {
        let row = rows.iter().find(|row| row.get::<String, _>("id") == *id)
            .ok_or("Replay operation evidence has no canonical tuple")?;
        require_replay_queue_witness(witness, row)?;
    }
    if orders.values().any(|order| *order != 0) {
        return Err("Legacy replay queue order evidence is invalid".into());
    }
    let mut positions = std::collections::HashMap::new();
    let mut complete = cursor.len() == rows.len();
    for (index, witness) in cursor.iter().enumerate() {
        let Some(id) = witness.get("id").and_then(Value::as_str) else { complete = false; break; };
        let Some(row) = rows.iter().find(|row| row.get::<String, _>("id") == id) else { complete = false; break; };
        if !replay_queue_immutable_tuple_matches(witness,row)? || positions.insert(id.to_string(), index as i64).is_some() {
            complete = false; break;
        }
        require_replay_queue_witness(witness,row)?;
        if let Some(current) = raw.get(id) {
            let mut old = witness.as_object().cloned().ok_or("Replay cursor evidence is malformed")?;
            let mut current = current.as_object().cloned().ok_or("Replay evidence is malformed")?;
            for field in ["attempts", "last_error"] { old.remove(field); current.remove(field); }
            if old != current { complete = false; break; }
        }
    }
    if complete { return Ok((positions, "legacy_cursor")); }
    Ok((rows.iter().enumerate().map(|(index,row)| (row.get::<String,_>("id"), index as i64)).collect(), "legacy_dispatch"))
}

async fn adopt_legacy_replay_queue_order(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
) -> Result<(), String> {
    let marker: Option<(String,)> = sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key='replay_queue_order_format'")
        .fetch_optional(&mut **tx).await.map_err(|e| e.to_string())?;
    let rows = sqlx::query("SELECT id, entity_type, entity_id, action, queued_at, attempts, last_error, payload FROM local_queued_operations ORDER BY queued_at,id")
        .fetch_all(&mut **tx).await.map_err(|e| e.to_string())?;
    let records: Vec<(String,String,i64)> = sqlx::query_as("SELECT id,payload,row_order FROM local_vault_records WHERE collection='queued_operations'")
        .fetch_all(&mut **tx).await.map_err(|e| e.to_string())?;
    let mut raw = std::collections::HashMap::new();
    let mut orders = std::collections::HashMap::new();
    for (id,payload,order) in records {
        raw.insert(id.clone(), serde_json::from_str::<Value>(&payload).map_err(|_| "Replay operation evidence is malformed")?);
        orders.insert(id,order);
    }
    let cursors: Vec<(String,)> = sqlx::query_as("SELECT payload FROM local_sync_cursors")
        .fetch_all(&mut **tx).await.map_err(|e| e.to_string())?;
    let mut cursor = Vec::new();
    let single_cursor = cursors.len() == 1;
    for (payload,) in cursors {
        let value: Value = serde_json::from_str(&payload).map_err(|_| "Replay cursor evidence is malformed")?;
        if !value.is_object() { return Err("Replay cursor evidence is malformed".into()); }
        let values = match value.get("queued_operations") { Some(Value::Array(values)) => values.clone(), None => Vec::new(), _ => return Err("Replay cursor queue evidence is malformed".into()) };
        let mut ids = std::collections::HashSet::new();
        for operation in &values {
            let id = operation.get("id").and_then(Value::as_str).filter(|id|!id.trim().is_empty()).ok_or("Replay cursor evidence is malformed")?;
            if !ids.insert(id) { return Err("Replay cursor operation evidence is ambiguous".into()); }
        }
        if single_cursor { cursor = values; }
    }
    let (positions, origin) = replay_queue_order_plan(&rows,&raw,&orders,&cursor,marker.as_ref().map(|row|row.0.as_str()))?;
    if marker.is_some() { return Ok(()); }
    for row in rows {
        let id = row.get::<String,_>("id");
        let witness = if let Some(witness) = raw.get(&id) { witness.clone() }
            else if let Some(witness) = current_replay_cursor_witness(&cursor,&row)? { witness }
            else { legacy_replay_queue_columns(&row)? };
        sqlx::query("INSERT INTO local_vault_records(collection,id,row_order,payload,updated_at) VALUES('queued_operations',?1,?2,?3,NULL) ON CONFLICT(collection,id) DO UPDATE SET row_order=excluded.row_order")
            .bind(&id).bind(positions[&id]).bind(witness.to_string()).execute(&mut **tx).await.map_err(|e|e.to_string())?;
    }
    sqlx::query("INSERT INTO local_vault_metadata(key,value) VALUES('replay_queue_order_format',?1)")
        .bind(format!("1:{origin}")).execute(&mut **tx).await.map_err(|e|e.to_string())?;
    Ok(())
}

fn legacy_replay_queue_columns(row: &sqlx::sqlite::SqliteRow) -> Result<Value,String> {
    let action = row.get::<String,_>("action");
    if matches!(action.as_str(), "update_identifier" | "update_maintenance") { return Err("Replay recovery binding is missing".into()); }
    let mut value = Map::new();
    for field in ["id","entity_type","entity_id","action","queued_at"] { value.insert(field.into(),Value::String(row.get::<String,_>(field))); }
    let attempts = row.get::<i64,_>("attempts");
    if attempts < 0 { return Err("Replay attempts are invalid".into()); }
    value.insert("attempts".into(),Value::Number(attempts.into()));
    if let Some(error) = row.get::<Option<String>,_>("last_error") { value.insert("last_error".into(),Value::String(error)); }
    if let Some(payload) = row.get::<Option<String>,_>("payload") { value.insert("payload".into(),serde_json::from_str(&payload).map_err(|_| "Replay operation payload is malformed")?); }
    Ok(Value::Object(value))
}

async fn delete_snapshot_bridge_and_normalized_record(
    tx: &mut sqlx::Transaction<'_, Sqlite>, collection: &str, id: &str,
) -> Result<(),String> {
    if collection == "queued_operations" { adopt_legacy_replay_queue_order(tx).await?; }
    sqlx::query("DELETE FROM local_vault_records WHERE collection=?1 AND id=?2")
        .bind(collection).bind(id).execute(&mut **tx).await.map_err(|e|e.to_string())?;
    delete_normalized_record(tx, collection, id).await
}

async fn upsert_snapshot_bridge_record(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    collection: &str,
    id: &str,
    payload: &Value,
    row_order: Option<i64>,
) -> Result<(), String> {
    if collection == "reviewed_move_records" {
        validate_reviewed_move_records(std::slice::from_ref(payload))?;
        if payload.get("id").and_then(Value::as_str)!=Some(id) {return Err("Reviewed move evidence identity changed".into());}
    }
    if collection == "reviewed_draft_transfer_records" {
        validate_reviewed_draft_transfer_records(std::slice::from_ref(payload))?;
        if payload.get("operation_id").and_then(Value::as_str)!=Some(id) {return Err("Reviewed draft transfer evidence identity changed".into());}
    }
    let updated_at = payload
        .get("updated_at")
        .or_else(|| payload.get("created_at"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let row_order = if collection == "queued_operations" {
        adopt_legacy_replay_queue_order(tx).await?;
        let existing: Option<(i64,)> = sqlx::query_as("SELECT row_order FROM local_vault_records WHERE collection='queued_operations' AND id=?1")
            .bind(id).fetch_optional(&mut **tx).await.map_err(|e|e.to_string())?;
        if let Some((order,)) = existing { Some(order) } else {
            let (next,): (i64,) = sqlx::query_as("SELECT COALESCE(MAX(row_order),-1)+1 FROM local_vault_records WHERE collection='queued_operations'")
                .fetch_one(&mut **tx).await.map_err(|e|e.to_string())?;
            Some(next)
        }
    } else { row_order };
    sqlx::query(
        "INSERT INTO local_vault_records
            (collection, id, row_order, payload, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(collection, id) DO UPDATE SET
            row_order = excluded.row_order,
            payload = excluded.payload,
            updated_at = excluded.updated_at",
    )
    .bind(collection)
    .bind(id)
    .bind(row_order.unwrap_or(0))
    .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
    .bind(updated_at)
    .execute(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

async fn upsert_normalized_record(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    provider: &str,
    collection: &str,
    id: &str,
    payload: &Value,
) -> Result<(), String> {
    match collection {
        "spaces" => {
            sqlx::query(
                "INSERT INTO local_spaces (id, name, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    updated_at = excluded.updated_at,
                    payload = excluded.payload",
            )
            .bind(id)
            .bind(value_string(payload, "name").unwrap_or_default())
            .bind(
                value_string(payload, "updated_at").or_else(|| value_string(payload, "created_at")),
            )
            .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        "documents" => {
            sqlx::query(
                "INSERT INTO local_documents (id, space_id, title, folder_id, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    space_id = excluded.space_id,
                    title = excluded.title,
                    folder_id = excluded.folder_id,
                    updated_at = excluded.updated_at,
                    payload = excluded.payload",
            )
            .bind(id)
            .bind(value_string(payload, "space_id").unwrap_or_default())
            .bind(
                value_string(payload, "title")
                    .or_else(|| value_string(payload, "name"))
                    .unwrap_or_default(),
            )
            .bind(value_string(payload, "folder_id"))
            .bind(value_string(payload, "updated_at"))
            .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        "folders" => {
            sqlx::query(
                "INSERT INTO local_folders (id, space_id, parent_id, name, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    space_id = excluded.space_id,
                    parent_id = excluded.parent_id,
                    name = excluded.name,
                    updated_at = excluded.updated_at,
                    payload = excluded.payload",
            )
            .bind(id)
            .bind(value_string(payload, "space_id").unwrap_or_default())
            .bind(value_string(payload, "parent_id"))
            .bind(value_string(payload, "name").unwrap_or_default())
            .bind(value_string(payload, "updated_at"))
            .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        "tags" => {
            sqlx::query(
                "INSERT INTO local_tags (id, space_id, name, color, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    space_id = excluded.space_id,
                    name = excluded.name,
                    color = excluded.color,
                    updated_at = excluded.updated_at,
                    payload = excluded.payload",
            )
            .bind(id)
            .bind(value_string(payload, "space_id").unwrap_or_default())
            .bind(value_string(payload, "name").unwrap_or_default())
            .bind(value_string(payload, "color"))
            .bind(
                value_string(payload, "updated_at").or_else(|| value_string(payload, "created_at")),
            )
            .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        "property_schemas" => {
            sqlx::query(
                "INSERT INTO local_properties (id, space_id, name, property_type, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    space_id = excluded.space_id,
                    name = excluded.name,
                    property_type = excluded.property_type,
                    updated_at = excluded.updated_at,
                    payload = excluded.payload",
            )
            .bind(id)
            .bind(value_string(payload, "space_id").unwrap_or_default())
            .bind(value_string(payload, "name").unwrap_or_default())
            .bind(value_string(payload, "property_type").unwrap_or_default())
            .bind(value_string(payload, "updated_at").or_else(|| value_string(payload, "created_at")))
            .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        "assets" => {
            sqlx::query(
                "INSERT INTO local_assets (id, space_id, document_id, name, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    space_id = excluded.space_id,
                    document_id = excluded.document_id,
                    name = excluded.name,
                    updated_at = excluded.updated_at,
                    payload = excluded.payload",
            )
            .bind(id)
            .bind(value_string(payload, "space_id"))
            .bind(value_string(payload, "document_id"))
            .bind(value_string(payload, "name").unwrap_or_default())
            .bind(
                value_string(payload, "updated_at").or_else(|| value_string(payload, "created_at")),
            )
            .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        "templates" => {
            sqlx::query(
                "INSERT INTO local_templates (id, space_id, title, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    space_id = excluded.space_id,
                    title = excluded.title,
                    updated_at = excluded.updated_at,
                    payload = excluded.payload",
            )
            .bind(id)
            .bind(value_string(payload, "space_id").unwrap_or_default())
            .bind(value_string(payload, "title").unwrap_or_default())
            .bind(
                value_string(payload, "updated_at").or_else(|| value_string(payload, "created_at")),
            )
            .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        "workflow_templates" => {
            sqlx::query(
                "INSERT INTO local_workflow_templates (id, space_id, name, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    space_id = excluded.space_id,
                    name = excluded.name,
                    updated_at = excluded.updated_at,
                    payload = excluded.payload",
            )
            .bind(id)
            .bind(value_string(payload, "space_id").unwrap_or_default())
            .bind(value_string(payload, "name").unwrap_or_default())
            .bind(
                value_string(payload, "updated_at").or_else(|| value_string(payload, "created_at")),
            )
            .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        "document_workflows" => {
            sqlx::query(
                "INSERT INTO local_document_workflows
                    (id, document_id, space_id, workflow_template_id, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    document_id = excluded.document_id,
                    space_id = excluded.space_id,
                    workflow_template_id = excluded.workflow_template_id,
                    updated_at = excluded.updated_at,
                    payload = excluded.payload",
            )
            .bind(id)
            .bind(value_string(payload, "document_id").unwrap_or_default())
            .bind(value_string(payload, "space_id"))
            .bind(value_string(payload, "workflow_template_id"))
            .bind(
                value_string(payload, "updated_at").or_else(|| value_string(payload, "created_at")),
            )
            .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        "document_workflow_events" => {
            sqlx::query(
                "INSERT INTO local_document_workflow_events
                    (id, document_id, workflow_id, created_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    document_id = excluded.document_id,
                    workflow_id = excluded.workflow_id,
                    created_at = excluded.created_at,
                    payload = excluded.payload",
            )
            .bind(id)
            .bind(value_string(payload, "document_id").unwrap_or_default())
            .bind(
                value_string(payload, "document_workflow_id")
                    .or_else(|| value_string(payload, "workflow_id")),
            )
            .bind(value_string(payload, "created_at"))
            .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        "property_values" => {
            sqlx::query(
                "INSERT INTO local_property_values
                    (id, document_id, property_id, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    document_id = excluded.document_id,
                    property_id = excluded.property_id,
                    updated_at = excluded.updated_at,
                    payload = excluded.payload",
            )
            .bind(id)
            .bind(value_string(payload, "document_id").unwrap_or_default())
            .bind(value_string(payload, "property_id").unwrap_or_default())
            .bind(
                value_string(payload, "updated_at").or_else(|| value_string(payload, "created_at")),
            )
            .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        "doc_tags" => {
            sqlx::query(
                "INSERT INTO local_doc_tags
                    (id, document_id, tag_id, space_id, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET
                    document_id = excluded.document_id,
                    tag_id = excluded.tag_id,
                    space_id = excluded.space_id,
                    updated_at = excluded.updated_at,
                    payload = excluded.payload",
            )
            .bind(id)
            .bind(value_string(payload, "document_id").unwrap_or_default())
            .bind(value_string(payload, "tag_id").unwrap_or_default())
            .bind(value_string(payload, "space_id"))
            .bind(
                value_string(payload, "updated_at").or_else(|| value_string(payload, "created_at")),
            )
            .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        "search_index" => {
            sqlx::query(
                "INSERT INTO local_search_index (document_id, space_id, title, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(document_id) DO UPDATE SET
                    space_id = excluded.space_id,
                    title = excluded.title,
                    updated_at = excluded.updated_at,
                    payload = excluded.payload",
            )
            .bind(id)
            .bind(value_string(payload, "space_id").unwrap_or_default())
            .bind(value_string(payload, "title").unwrap_or_default())
            .bind(value_string(payload, "updated_at"))
            .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        "sync_state" => {
            sqlx::query(
                "INSERT INTO local_sync_cursors (provider, sync_state, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(provider) DO UPDATE SET
                    sync_state = excluded.sync_state,
                    updated_at = excluded.updated_at,
                    payload = excluded.payload",
            )
            .bind(provider)
            .bind(value_string(payload, "state").unwrap_or_else(|| "idle".to_string()))
            .bind(
                value_string(payload, "updated_at")
                    .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string()),
            )
            .bind(serde_json::to_string(payload).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        "queued_operations" => {
            sqlx::query(
                "INSERT INTO local_queued_operations
                    (id, entity_type, entity_id, action, queued_at, attempts, last_error, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                    entity_type = excluded.entity_type,
                    entity_id = excluded.entity_id,
                    action = excluded.action,
                    queued_at = excluded.queued_at,
                    attempts = excluded.attempts,
                    last_error = excluded.last_error,
                    payload = excluded.payload",
            )
            .bind(id)
            .bind(value_string(payload, "entity_type").unwrap_or_default())
            .bind(value_string(payload, "entity_id").unwrap_or_default())
            .bind(value_string(payload, "action").unwrap_or_default())
            .bind(value_string(payload, "queued_at").unwrap_or_default())
            .bind(value_i64(payload, "attempts").unwrap_or(0))
            .bind(value_string(payload, "last_error"))
            .bind(
                payload
                    .get("payload")
                    .map(serde_json::to_string)
                    .transpose()
                    .map_err(|e| e.to_string())?,
            )
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
        _ => {}
    }

    Ok(())
}

async fn delete_normalized_record(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    collection: &str,
    id: &str,
) -> Result<(), String> {
    let statement = match collection {
        "spaces" => "DELETE FROM local_spaces WHERE id = ?1",
        "documents" => "DELETE FROM local_documents WHERE id = ?1",
        "folders" => "DELETE FROM local_folders WHERE id = ?1",
        "tags" => "DELETE FROM local_tags WHERE id = ?1",
        "property_schemas" => "DELETE FROM local_properties WHERE id = ?1",
        "assets" => "DELETE FROM local_assets WHERE id = ?1",
        "templates" => "DELETE FROM local_templates WHERE id = ?1",
        "workflow_templates" => "DELETE FROM local_workflow_templates WHERE id = ?1",
        "document_workflows" => "DELETE FROM local_document_workflows WHERE id = ?1",
        "document_workflow_events" => "DELETE FROM local_document_workflow_events WHERE id = ?1",
        "property_values" => "DELETE FROM local_property_values WHERE id = ?1",
        "doc_tags" => "DELETE FROM local_doc_tags WHERE id = ?1",
        "search_index" => "DELETE FROM local_search_index WHERE document_id = ?1",
        "queued_operations" => "DELETE FROM local_queued_operations WHERE id = ?1",
        "sync_state" => "DELETE FROM local_sync_cursors WHERE provider = ?1",
        _ => return Ok(()),
    };

    sqlx::query(statement)
        .bind(id)
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

async fn write_normalized_snapshot_tables(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    provider: &str,
    snapshot: &Value,
) -> Result<(), String> {
    if let Some(spaces) = snapshot.get("spaces").and_then(Value::as_array) {
        for space in spaces {
            sqlx::query(
                "INSERT INTO local_spaces (id, name, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(value_string(space, "id").unwrap_or_default())
            .bind(value_string(space, "name").unwrap_or_default())
            .bind(value_string(space, "updated_at").or_else(|| value_string(space, "created_at")))
            .bind(serde_json::to_string(space).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(documents) = snapshot.get("documents").and_then(Value::as_array) {
        for document in documents {
            sqlx::query(
                "INSERT INTO local_documents (id, space_id, title, folder_id, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(value_string(document, "id").unwrap_or_default())
            .bind(value_string(document, "space_id").unwrap_or_default())
            .bind(value_string(document, "title").unwrap_or_default())
            .bind(value_string(document, "folder_id"))
            .bind(value_string(document, "updated_at"))
            .bind(serde_json::to_string(document).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(folders) = snapshot.get("folders").and_then(Value::as_array) {
        for folder in folders {
            sqlx::query(
                "INSERT INTO local_folders (id, space_id, parent_id, name, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(value_string(folder, "id").unwrap_or_default())
            .bind(value_string(folder, "space_id").unwrap_or_default())
            .bind(value_string(folder, "parent_id"))
            .bind(value_string(folder, "name").unwrap_or_default())
            .bind(value_string(folder, "updated_at"))
            .bind(serde_json::to_string(folder).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(tags) = snapshot.get("tags").and_then(Value::as_array) {
        for tag in tags {
            sqlx::query(
                "INSERT INTO local_tags (id, space_id, name, color, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(value_string(tag, "id").unwrap_or_default())
            .bind(value_string(tag, "space_id").unwrap_or_default())
            .bind(value_string(tag, "name").unwrap_or_default())
            .bind(value_string(tag, "color"))
            .bind(value_string(tag, "updated_at").or_else(|| value_string(tag, "created_at")))
            .bind(serde_json::to_string(tag).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(properties) = snapshot.get("property_schemas").and_then(Value::as_array) {
        for property in properties {
            sqlx::query(
                "INSERT INTO local_properties (id, space_id, name, property_type, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(value_string(property, "id").unwrap_or_default())
            .bind(value_string(property, "space_id").unwrap_or_default())
            .bind(value_string(property, "name").unwrap_or_default())
            .bind(value_string(property, "property_type").unwrap_or_default())
            .bind(value_string(property, "updated_at").or_else(|| value_string(property, "created_at")))
            .bind(serde_json::to_string(property).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(assets) = snapshot.get("assets").and_then(Value::as_array) {
        for asset in assets {
            sqlx::query(
                "INSERT INTO local_assets (id, space_id, document_id, name, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(value_string(asset, "id").unwrap_or_default())
            .bind(value_string(asset, "space_id"))
            .bind(value_string(asset, "document_id"))
            .bind(value_string(asset, "name").unwrap_or_default())
            .bind(value_string(asset, "updated_at").or_else(|| value_string(asset, "created_at")))
            .bind(serde_json::to_string(asset).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(templates) = snapshot.get("templates").and_then(Value::as_array) {
        for template in templates {
            sqlx::query(
                "INSERT INTO local_templates (id, space_id, title, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(value_string(template, "id").unwrap_or_default())
            .bind(value_string(template, "space_id").unwrap_or_default())
            .bind(
                value_string(template, "title")
                    .or_else(|| value_string(template, "name"))
                    .unwrap_or_default(),
            )
            .bind(
                value_string(template, "updated_at")
                    .or_else(|| value_string(template, "created_at")),
            )
            .bind(serde_json::to_string(template).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(workflow_templates) = snapshot.get("workflow_templates").and_then(Value::as_array) {
        for workflow_template in workflow_templates {
            sqlx::query(
                "INSERT INTO local_workflow_templates (id, space_id, name, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(value_string(workflow_template, "id").unwrap_or_default())
            .bind(value_string(workflow_template, "space_id").unwrap_or_default())
            .bind(value_string(workflow_template, "name").unwrap_or_default())
            .bind(
                value_string(workflow_template, "updated_at")
                    .or_else(|| value_string(workflow_template, "created_at")),
            )
            .bind(serde_json::to_string(workflow_template).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(document_workflows) = snapshot.get("document_workflows").and_then(Value::as_array) {
        for (index, document_workflow) in document_workflows.iter().enumerate() {
            sqlx::query(
                "INSERT INTO local_document_workflows
                    (id, document_id, space_id, workflow_template_id, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(record_id("document_workflows", document_workflow, index))
            .bind(value_string(document_workflow, "document_id").unwrap_or_default())
            .bind(value_string(document_workflow, "space_id"))
            .bind(value_string(document_workflow, "workflow_template_id"))
            .bind(
                value_string(document_workflow, "updated_at")
                    .or_else(|| value_string(document_workflow, "created_at")),
            )
            .bind(serde_json::to_string(document_workflow).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(document_workflow_events) = snapshot
        .get("document_workflow_events")
        .and_then(Value::as_array)
    {
        for event in document_workflow_events {
            sqlx::query(
                "INSERT INTO local_document_workflow_events
                    (id, document_id, workflow_id, created_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(value_string(event, "id").unwrap_or_default())
            .bind(value_string(event, "document_id").unwrap_or_default())
            .bind(
                value_string(event, "document_workflow_id")
                    .or_else(|| value_string(event, "workflow_id")),
            )
            .bind(value_string(event, "created_at"))
            .bind(serde_json::to_string(event).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(property_values) = snapshot.get("property_values").and_then(Value::as_array) {
        for (index, property_value) in property_values.iter().enumerate() {
            sqlx::query(
                "INSERT INTO local_property_values
                    (id, document_id, property_id, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(record_id("property_values", property_value, index))
            .bind(value_string(property_value, "document_id").unwrap_or_default())
            .bind(value_string(property_value, "property_id").unwrap_or_default())
            .bind(
                value_string(property_value, "updated_at")
                    .or_else(|| value_string(property_value, "created_at")),
            )
            .bind(serde_json::to_string(property_value).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(doc_tags) = snapshot.get("doc_tags").and_then(Value::as_array) {
        for (index, doc_tag) in doc_tags.iter().enumerate() {
            sqlx::query(
                "INSERT INTO local_doc_tags
                    (id, document_id, tag_id, space_id, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .bind(record_id("doc_tags", doc_tag, index))
            .bind(value_string(doc_tag, "document_id").unwrap_or_default())
            .bind(value_string(doc_tag, "tag_id").unwrap_or_default())
            .bind(value_string(doc_tag, "space_id"))
            .bind(
                value_string(doc_tag, "updated_at").or_else(|| value_string(doc_tag, "created_at")),
            )
            .bind(serde_json::to_string(doc_tag).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(search_entries) = snapshot.get("search_index").and_then(Value::as_array) {
        for entry in search_entries {
            sqlx::query(
                "INSERT INTO local_search_index (document_id, space_id, title, updated_at, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )
            .bind(value_string(entry, "document_id").unwrap_or_default())
            .bind(value_string(entry, "space_id").unwrap_or_default())
            .bind(value_string(entry, "title").unwrap_or_default())
            .bind(value_string(entry, "updated_at"))
            .bind(serde_json::to_string(entry).map_err(|e| e.to_string())?)
            .execute(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    if let Some(sync_state) = snapshot.get("sync_state") {
        sqlx::query(
            "INSERT INTO local_sync_cursors (provider, sync_state, updated_at, payload)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(provider)
        .bind(value_string(sync_state, "state").unwrap_or_else(|| "idle".to_string()))
        .bind(
            value_string(sync_state, "updated_at")
                .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string()),
        )
        .bind(serde_json::to_string(sync_state).map_err(|e| e.to_string())?)
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;

        if let Some(operations) = sync_state
            .get("queued_operations")
            .and_then(Value::as_array)
        {
            for (index, operation) in operations.iter().enumerate() {
                let id = value_string(operation, "id").unwrap_or_default();
                sqlx::query("INSERT INTO local_vault_records(collection,id,row_order,payload,updated_at) VALUES('queued_operations',?1,?2,?3,NULL)")
            .bind(&id).bind(index as i64).bind(operation.to_string()).execute(&mut **tx).await.map_err(|e|e.to_string())?;
                sqlx::query(
                    "INSERT INTO local_queued_operations
                        (id, entity_type, entity_id, action, queued_at, attempts, last_error, payload)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                )
                .bind(value_string(operation, "id").unwrap_or_default())
                .bind(value_string(operation, "entity_type").unwrap_or_default())
                .bind(value_string(operation, "entity_id").unwrap_or_default())
                .bind(value_string(operation, "action").unwrap_or_default())
                .bind(value_string(operation, "queued_at").unwrap_or_default())
                .bind(value_i64(operation, "attempts").unwrap_or(0))
                .bind(value_string(operation, "last_error"))
                .bind(
                    operation
                        .get("payload")
                        .map(serde_json::to_string)
                        .transpose()
                        .map_err(|e| e.to_string())?,
                )
                .execute(&mut **tx)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
    }

    write_normalized_replay_checkpoints(tx, provider, snapshot).await?;

    Ok(())
}

async fn write_normalized_replay_checkpoints(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    provider: &str,
    snapshot: &Value,
) -> Result<(), String> {
    let mut checkpoint_index = 0usize;
    for checkpoint in replay_checkpoint_values(snapshot) {
        let id = value_string_any(checkpoint, &["id"])
            .or_else(|| value_string_any(checkpoint, &["spaceId", "space_id"]))
            .unwrap_or_else(|| {
                checkpoint_index += 1;
                format!("checkpoint-{checkpoint_index}")
            });
        sqlx::query(
            "INSERT OR REPLACE INTO local_replay_checkpoints
                (id, provider, space_id, source_server_url, downloaded_by_user_id, downloaded_at, payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(id)
        .bind(provider)
        .bind(value_string_any(checkpoint, &["spaceId", "space_id"]))
        .bind(value_string_any(
            checkpoint,
            &["sourceServerUrl", "source_server_url"],
        ))
        .bind(value_string_any(
            checkpoint,
            &["downloadedByUserId", "downloaded_by_user_id"],
        ))
        .bind(value_string_any(
            checkpoint,
            &["downloadedAt", "downloaded_at", "created_at"],
        ))
        .bind(serde_json::to_string(checkpoint).map_err(|e| e.to_string())?)
        .execute(&mut **tx)
        .await
        .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn replay_checkpoint_values(snapshot: &Value) -> Vec<&Value> {
    let mut checkpoints = Vec::new();
    for key in ["recovery_checkpoints", "shared_space_download_checkpoints"] {
        if let Some(items) = snapshot.get(key).and_then(Value::as_array) {
            checkpoints.extend(items);
        }
    }
    if let Some(items) = snapshot
        .get("sync_state")
        .and_then(|value| value.get("recovery_checkpoints"))
        .and_then(Value::as_array)
    {
        checkpoints.extend(items);
    }
    checkpoints
}

async fn read_maintenance_catalogs(pool: &Pool<Sqlite>) -> Result<Option<Value>, String> {
    let mut connection=pool.acquire().await.map_err(|e|e.to_string())?;
    read_maintenance_catalogs_connection(&mut connection).await
}

async fn read_maintenance_catalogs_connection(connection: &mut sqlx::SqliteConnection) -> Result<Option<Value>, String> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT value FROM local_vault_metadata WHERE key='maintenance_target_catalogs'",
    )
    .fetch_optional(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;
    row.map(|(raw,)| {
        let value: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        if !value.is_array() {
            return Err("Maintenance target catalogs are malformed".into());
        }
        Ok(value)
    })
    .transpose()
}

async fn read_normalized_sync_state(
    pool: &Pool<Sqlite>,
    provider: &str,
) -> Result<Option<Value>, String> {
    let mut connection=pool.acquire().await.map_err(|e|e.to_string())?;
    read_normalized_sync_state_connection(&mut connection, provider).await
}

async fn read_normalized_sync_state_connection(
    connection: &mut sqlx::SqliteConnection,
    provider: &str,
) -> Result<Option<Value>, String> {
    let cursor: Option<(String, String)> =
        sqlx::query_as("SELECT sync_state, payload FROM local_sync_cursors WHERE provider = ?1")
            .bind(provider)
            .fetch_optional(&mut *connection)
            .await
            .map_err(|e| e.to_string())?;

    let operation_rows = sqlx::query(
        "SELECT id, entity_type, entity_id, action, queued_at, attempts, last_error, payload
         FROM local_queued_operations ORDER BY queued_at, id",
    )
    .fetch_all(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;
    let replay_checkpoints = read_normalized_replay_checkpoints_connection(connection, provider).await?;

    let raw_rows: Vec<(String, String, Option<i64>)> = sqlx::query_as(
        "SELECT id, payload, row_order FROM local_vault_records WHERE collection = 'queued_operations'",
    ).fetch_all(&mut *connection).await.map_err(|e| e.to_string())?;
    if cursor.is_none() && operation_rows.is_empty() && replay_checkpoints.is_empty() && raw_rows.is_empty() {
        return Ok(None);
    }

    let mut sync_state = match cursor.as_ref() {
        Some((_, payload)) => serde_json::from_str::<Value>(payload)
            .map_err(|_| "Replay cursor evidence is malformed".to_string())?
            .as_object().cloned().ok_or_else(|| "Replay cursor evidence is malformed".to_string())?,
        None => Map::new(),
    };

    if let Some((state, _)) = cursor {
        if !["idle", "syncing", "conflict", "error"].contains(&state.as_str()) {
            return Err("Replay cursor state is invalid".into());
        }
        sync_state.insert("state".to_string(), Value::String(state));
    } else {
        // Column-only legacy queue records still form a readable idle queue;
        // this compatibility state does not create replay provenance.
        sync_state.insert("state".into(), Value::String("idle".into()));
    }

    let original_operations = match sync_state.get("queued_operations") {
        Some(Value::Array(values)) => values.clone(),
        None => Vec::new(),
        _ => return Err("Replay cursor queue evidence is malformed".into()),
    };
    let mut cursor_ids = std::collections::HashSet::new();
    for raw in &original_operations {
        let id = raw.get("id").and_then(Value::as_str).filter(|id| !id.trim().is_empty())
            .ok_or_else(|| "Replay cursor operation evidence is malformed".to_string())?;
        if !cursor_ids.insert(id) { return Err("Replay operation evidence is ambiguous".into()); }
    }
    // Partial queue upserts update their bridge and normalized tuple in the same
    // transaction, but need not update the older full-snapshot cursor payload.
    let mut raw_by_id = std::collections::HashMap::new();
    let mut raw_order = std::collections::HashMap::new();
    for (id, payload, order) in raw_rows {
        let order = order.filter(|order| *order >= 0).ok_or_else(|| "Replay queue order evidence is invalid".to_string())?;
        raw_order.insert(id.clone(), order);
        let raw: Value = serde_json::from_str(&payload)
            .map_err(|_| "Replay operation evidence is malformed".to_string())?;
        if !raw.is_object() || raw.get("id").and_then(Value::as_str) != Some(id.as_str()) {
            return Err("Replay operation evidence identity differs".into());
        }
        raw_by_id.insert(id, raw);
    }
    let marker: Option<(String,)> = sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key='replay_queue_order_format'")
        .fetch_optional(&mut *connection).await.map_err(|e|e.to_string())?;
    let (positions, _) = replay_queue_order_plan(&operation_rows,&raw_by_id,&raw_order,&original_operations,marker.as_ref().map(|row|row.0.as_str()))?;
    let mut ordered_operations = operation_rows.into_iter().map(|row| {
        let id = row.get::<String, _>("id");
        let originals = original_operations.iter().filter(|raw| raw.get("id").and_then(Value::as_str) == Some(id.as_str())).collect::<Vec<_>>();
        if originals.len() > 1 {
            return Err("Replay operation evidence is ambiguous".to_string());
        }
        let order = positions[&id];
        let raw = match raw_by_id.remove(&id) { Some(raw)=>Some(raw), None=>current_replay_cursor_witness(&original_operations,&row)? };
        let action = row.get::<String, _>("action");
        let payload = row.get::<Option<String>, _>("payload").map(|value|
            serde_json::from_str::<Value>(&value).map_err(|_| "Replay operation payload is malformed".to_string())
        ).transpose()?;
        let mut operation = if let Some(raw) = raw {
            let mut value = require_replay_queue_witness(&raw, &row)?;
            // Attempts/error are the only mutable queue columns. Absence of a
            // legacy attempts field is preserved while its SQL default stays 0.
            let attempts = row.get::<i64, _>("attempts");
            if attempts < 0 { return Err("Replay attempts are invalid".into()); }
            if value.contains_key("attempts") || attempts != 0 {
                value.insert("attempts".into(), Value::Number(attempts.into()));
            }
            match row.get::<Option<String>, _>("last_error") {
                Some(error) => { value.insert("last_error".into(), Value::String(error)); }
                None => { value.remove("last_error"); }
            }
            value
        } else {
            // Old column-only records have no provider/account witness. Keep
            // their legacy shape; never manufacture current replay provenance.
            if matches!(action.as_str(), "update_identifier" | "update_maintenance") {
                return Err("Identifier recovery binding is missing".into());
            }
            let mut value = Map::new();
            for field in ["id", "entity_type", "entity_id", "action", "queued_at"] {
                value.insert(field.into(), Value::String(row.get::<String, _>(field)));
            }
            let attempts = row.get::<i64, _>("attempts");
            if attempts < 0 { return Err("Replay attempts are invalid".into()); }
            value.insert("attempts".into(), Value::Number(attempts.into()));
            if let Some(error) = row.get::<Option<String>, _>("last_error") { value.insert("last_error".into(), Value::String(error)); }
            if let Some(payload) = payload { value.insert("payload".into(), payload); }
            value
        };
        // Keep object field order irrelevant; authored payload/source strings,
        // explicit nulls and all unknown operation metadata remain unchanged.
        Ok((order, Value::Object(std::mem::take(&mut operation))))
    }).collect::<Result<Vec<_>, String>>()?;
    ordered_operations.sort_by_key(|(order, _)| *order);
    if ordered_operations.windows(2).any(|rows| rows[0].0 == rows[1].0) {
        return Err("Replay queue order evidence is ambiguous".into());
    }
    let queued_operations = ordered_operations.into_iter().map(|(_, operation)| operation).collect::<Vec<_>>();
    if !raw_by_id.is_empty() {
        return Err("Replay operation evidence has no canonical tuple".into());
    }

    sync_state.insert(
        "queued_operations".to_string(),
        Value::Array(queued_operations),
    );
    sync_state.insert(
        "recovery_checkpoints".to_string(),
        Value::Array(replay_checkpoints),
    );

    Ok(Some(Value::Object(sync_state)))
}

async fn read_normalized_replay_checkpoints(
    pool: &Pool<Sqlite>,
    provider: &str,
) -> Result<Vec<Value>, String> {
    let mut connection=pool.acquire().await.map_err(|e|e.to_string())?;
    read_normalized_replay_checkpoints_connection(&mut connection, provider).await
}

async fn read_normalized_replay_checkpoints_connection(
    connection: &mut sqlx::SqliteConnection,
    provider: &str,
) -> Result<Vec<Value>, String> {
    let rows = sqlx::query(
        "SELECT payload FROM local_replay_checkpoints
         WHERE provider = ?1
         ORDER BY downloaded_at DESC, id",
    )
    .bind(provider)
    .fetch_all(&mut *connection)
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let payload: String = row.get("payload");
            serde_json::from_str::<Value>(&payload).ok()
        })
        .collect())
}

// Storage admission only: retain complete evidence JSON without granting authority
// or normalizing its decision/source fields. The reviewed-move consumer owns policy.
fn validate_reviewed_move_records(records:&[Value])->Result<(),String> {
    let mut ids=std::collections::HashSet::new();
    for record in records {
        let id=record.get("id").and_then(Value::as_str).filter(|id|!id.trim().is_empty())
            .ok_or("Reviewed move evidence needs its original operation ID")?;
        if !record.is_object() || !ids.insert(id) {return Err("Reviewed move evidence IDs must be unique".into());}
    }
    Ok(())
}

// Storage admission only: preserve the complete request, manifest, receipt and
// binding as opaque JSON. The web consumer owns transfer-state policy.
fn validate_reviewed_draft_transfer_records(records:&[Value])->Result<(),String> {
    let mut ids=std::collections::HashSet::new();
    for record in records {
        let id=record.get("operation_id").and_then(Value::as_str).filter(|id|!id.trim().is_empty())
            .ok_or("Reviewed draft transfer evidence needs its original operation ID")?;
        if !record.is_object() || !ids.insert(id) {return Err("Reviewed draft transfer evidence IDs must be unique".into());}
    }
    Ok(())
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn value_string_any(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| value_string(value, key))
}

fn value_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

fn record_id(collection: &str, value: &Value, index: usize) -> String {
    if collection == "reviewed_draft_transfer_records" {
        if let Some(operation_id) = value.get("operation_id").and_then(Value::as_str) {
            return operation_id.to_string();
        }
    }
    if collection == "document_workflows" {
        if let Some(id) = value.get("id").and_then(Value::as_str) {
            return id.to_string();
        }
        if let Some(document_id) = value.get("document_id").and_then(Value::as_str) {
            return document_id.to_string();
        }
    }

    if collection == "property_values" {
        let document_id = value
            .get("document_id")
            .and_then(Value::as_str)
            .unwrap_or("document");
        let property_id = value
            .get("property_id")
            .and_then(Value::as_str)
            .unwrap_or("property");
        return format!("{document_id}:{property_id}");
    }

    if collection == "doc_tags" {
        let document_id = value
            .get("document_id")
            .and_then(Value::as_str)
            .unwrap_or("document");
        let tag_id = value.get("tag_id").and_then(Value::as_str).unwrap_or("tag");
        return format!("{document_id}:{tag_id}");
    }

    if collection == "search_index" {
        if let Some(document_id) = value.get("document_id").and_then(Value::as_str) {
            return document_id.to_string();
        }
    }

    value
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("{collection}-{index}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn record_id_uses_document_and_tag_for_doc_tags() {
        assert_eq!(
            record_id(
                "doc_tags",
                &json!({ "document_id": "doc-1", "tag_id": "tag-1" }),
                0,
            ),
            "doc-1:tag-1"
        );
    }

    #[test]
    fn record_id_uses_document_for_search_index() {
        assert_eq!(
            record_id("search_index", &json!({ "document_id": "doc-1" }), 0),
            "doc-1"
        );
    }

    #[test]
    fn record_id_falls_back_to_collection_index() {
        assert_eq!(
            record_id("folders", &json!({ "name": "Inbox" }), 3),
            "folders-3"
        );
    }

    async fn atomic_test_pool() -> Pool<Sqlite> {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.expect("test sqlite");
        migrate_database(&pool).await.expect("migrate sqlite");
        pool
    }

    fn property_commit_snapshot(value: &str, operation_id: &str) -> Value {
        json!({
            "schema_version": 1,
            "documents": [{"id":"doc-1","space_id":"space-1","title":"Page","content":"Body"}],
            "property_schemas": [{"id":"property-1","space_id":"space-1","name":"Field","property_type":"text"}],
            "property_values": [{"id":"value-1","document_id":"doc-1","property_id":"property-1","value":value}],
            "sync_state": {"state":"idle","queued_operations":[{"id":operation_id,"entity_type":"document_property_values","entity_id":"doc-1","action":"set","queued_at":"2026-10-07T12:00:00Z"}]}
        })
    }

    fn replay_roundtrip_snapshot() -> Value {
        let mut snapshot = property_commit_snapshot("Original", "z-content-op");
        snapshot["sync_state"]["future_state"] = json!({"checkpoint":"retained"});
        snapshot["sync_state"]["queued_operations"] = json!([
            {"id":"z-content-op","entity_type":"document","entity_id":"doc-1","action":"update_content","queued_at":"2026-10-09T10:00:00Z","replay_provider":"google-drive","server_context":{"server_base":"https://old.example.test/api/v1","account_id":"old-account","session_scope":"old-session","future_binding":true},"payload":{"content":"Original\r\nbody","source":{"published_at":null,"revision":"old"}},"future_operation":{"raw":[null,"retained"]}},
            {"id":"a-identifier-op","entity_type":"document","entity_id":"doc-1","action":"update_identifier","queued_at":"2026-10-09T10:00:00Z","replay_provider":"google-drive","payload":{"id":"doc-1","source":{"human_identifier":null}},"attempts":0},
            {"id":"m-maintenance-op","entity_type":"document","entity_id":"doc-1","action":"update_maintenance","queued_at":"2026-10-09T10:00:00Z","replay_provider":"none","payload":null,"attempts":1,"last_error":"Retained"}
        ]);
        snapshot
    }

    fn replay_file_test_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("starlane-queue-{label}-{}-{}.sqlite3", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))
    }

    async fn snapshot_tx_capture(pool: &Pool<Sqlite>) -> Vec<(String,String)> {
        let tables:Vec<(String,)> = sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").fetch_all(pool).await.unwrap();
        let mut captured=Vec::new();
        for (table,) in tables {
            let fields:Vec<(String,)> = sqlx::query_as("SELECT name FROM pragma_table_info(?1) ORDER BY cid").bind(&table).fetch_all(pool).await.unwrap();
            let quote=|value:&str| format!("\"{}\"",value.replace('"',"\"\""));
            let columns=fields.iter().map(|(name,)|quote(name)).collect::<Vec<_>>().join(",");
            let sql=format!("SELECT json_group_array(json_array({columns})) FROM (SELECT * FROM {} ORDER BY rowid)",quote(&table));
            let (raw,):(String,)=sqlx::query_as(&sql).fetch_one(pool).await.unwrap();
            captured.push((table,raw));
        }
        captured
    }

    #[tokio::test]
    async fn snapshot_tx_helper_and_later_caller_sql_failure_roll_back_all_tables() {
        for stage in ["helper","caller"] {
            let pool=atomic_test_pool().await;
            let initial=replay_roundtrip_snapshot();
            write_snapshot_to_database(&pool,"none",&initial).await.unwrap();
            // Test-only caller data proves later SQL is part of the same TX.
            sqlx::query("CREATE TABLE snapshot_tx_probe(id TEXT PRIMARY KEY,revision TEXT NOT NULL)").execute(&pool).await.unwrap();
            let before=snapshot_tx_capture(&pool).await;
            let mut candidate=replay_roundtrip_snapshot();
            candidate["documents"][0]["content"]=json!("Uncommitted candidate body");
            if stage=="helper" {
                let duplicate=candidate["sync_state"]["queued_operations"][0].clone();
                candidate["sync_state"]["queued_operations"].as_array_mut().unwrap().push(duplicate);
            }
            let mut tx=pool.begin().await.unwrap();
            let outcome=write_snapshot_in_tx(&mut tx,"none",&candidate).await;
            if stage=="helper" { assert!(outcome.is_err()); }
            else {
                let revision=outcome.unwrap();
                let (pending,):(String,)=sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key='mirror_pending'").fetch_one(&mut *tx).await.unwrap();
                assert_eq!(pending,revision,"helper returns a pending same-TX revision");
                sqlx::query("INSERT INTO snapshot_tx_probe(id,revision) VALUES('caller',?1)").bind(&revision).execute(&mut *tx).await.unwrap();
                assert!(sqlx::query("INSERT INTO snapshot_tx_probe(id,revision) VALUES('caller','deliberate duplicate failure')").execute(&mut *tx).await.is_err());
            }
            tx.rollback().await.unwrap();
            assert_eq!(snapshot_tx_capture(&pool).await,before,"{stage} failure must retain every original SQL table");
        }
    }

    #[tokio::test]
    async fn snapshot_tx_caller_commit_persists_revision_queue_and_later_sql_on_actual_reopen() {
        let path=replay_file_test_path("snapshot-caller-tx");
        let pool=open_database(&path).await.unwrap();migrate_database(&pool).await.unwrap();
        let initial=property_commit_snapshot("Old","old-operation");
        let old_revision=write_snapshot_to_database(&pool,"none",&initial).await.unwrap();
        sqlx::query("CREATE TABLE snapshot_tx_probe(id TEXT PRIMARY KEY,revision TEXT NOT NULL)").execute(&pool).await.unwrap();
        let other=open_database(&path).await.unwrap();
        let candidate=replay_roundtrip_snapshot();
        let mut tx=pool.begin().await.unwrap();
        let revision=write_snapshot_in_tx(&mut tx,"none",&candidate).await.unwrap();
        sqlx::query("INSERT INTO snapshot_tx_probe(id,revision) VALUES('caller',?1)").bind(&revision).execute(&mut *tx).await.unwrap();
        let (observed,):(String,)=sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key='canonical_revision'").fetch_one(&other).await.unwrap();
        assert_eq!(observed,old_revision,"helper must not commit independently");
        other.close().await;
        tx.commit().await.unwrap();pool.close().await;
        let pool=open_database(&path).await.unwrap();migrate_database(&pool).await.unwrap();
        let (stored,):(String,)=sqlx::query_as("SELECT revision FROM snapshot_tx_probe WHERE id='caller'").fetch_one(&pool).await.unwrap();
        assert_eq!(stored,revision);
        let markers:Vec<(String,String)>=sqlx::query_as("SELECT key,value FROM local_vault_metadata WHERE key IN ('canonical_revision','mirror_pending') ORDER BY key").fetch_all(&pool).await.unwrap();
        assert_eq!(markers,vec![("canonical_revision".into(),revision.clone()),("mirror_pending".into(),revision)]);
        assert_eq!(read_normalized_sync_state(&pool,"none").await.unwrap().unwrap()["queued_operations"],candidate["sync_state"]["queued_operations"]);
        assert_eq!(read_snapshot_bridge_collection(&pool,"documents").await.unwrap(),candidate["documents"].as_array().unwrap().clone());
        pool.close().await;std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn replay_raw_all_actions_survive_actual_sql_full_snapshot_and_reopen() {
        let path = replay_file_test_path("full-reopen");
        let pool = open_database(&path).await.unwrap();
        migrate_database(&pool).await.unwrap();
        let snapshot = replay_roundtrip_snapshot();
        write_snapshot_to_database(&pool, "none", &snapshot).await.unwrap();
        pool.close().await;
        let pool = open_database(&path).await.unwrap();
        migrate_database(&pool).await.unwrap();
        let actual = read_normalized_sync_state(&pool, "none").await.unwrap().unwrap();
        assert_eq!(actual["queued_operations"], snapshot["sync_state"]["queued_operations"]);
        assert_eq!(actual["future_state"], snapshot["sync_state"]["future_state"]);
        assert!(actual["queued_operations"][0].get("attempts").is_none());
        let raw: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM local_vault_records WHERE collection='queued_operations'").fetch_one(&pool).await.unwrap();
        assert_eq!(raw.0, 3);
        pool.close().await;
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn replay_partial_upsert_uses_current_raw_witness_and_delete_removes_both() {
        let path = replay_file_test_path("partial-reopen");
        let pool = open_database(&path).await.unwrap();
        migrate_database(&pool).await.unwrap();
        let snapshot = replay_roundtrip_snapshot();
        write_snapshot_to_database(&pool, "none", &snapshot).await.unwrap();
        pool.close().await;
        let pool = open_database(&path).await.unwrap();
        migrate_database(&pool).await.unwrap();
        let mut updated = snapshot["sync_state"]["queued_operations"][0].clone();
        updated["payload"]["content"] = json!("Legitimate updated body");
        updated["future_operation"] = json!({"raw":"new witness"});
        let mut tx = pool.begin().await.unwrap();
        upsert_snapshot_bridge_record(&mut tx, "queued_operations", "z-content-op", &updated, None).await.unwrap();
        upsert_normalized_record(&mut tx, "none", "queued_operations", "z-content-op", &updated).await.unwrap();
        tx.commit().await.unwrap();
        pool.close().await;
        let pool = open_database(&path).await.unwrap();
        migrate_database(&pool).await.unwrap();
        let actual = read_normalized_sync_state(&pool, "none").await.unwrap().unwrap();
        assert_eq!(actual["queued_operations"][0], updated);
        assert_eq!(actual["future_state"], snapshot["sync_state"]["future_state"]);
        // Cursor raw intentionally remains the older full-snapshot payload.
        let cursor: (String,) = sqlx::query_as("SELECT payload FROM local_sync_cursors WHERE provider='none'").fetch_one(&pool).await.unwrap();
        assert_eq!(serde_json::from_str::<Value>(&cursor.0).unwrap()["queued_operations"][0], snapshot["sync_state"]["queued_operations"][0]);
        let appended = json!({"id":"b-appended","entity_type":"document","entity_id":"doc-1","action":"update_content","queued_at":"2020-01-01T00:00:00Z","payload":{"content":"Appended intent"},"future_operation":{"append":true}});
        let mut tx = pool.begin().await.unwrap();
        upsert_snapshot_bridge_record(&mut tx, "queued_operations", "b-appended", &appended, None).await.unwrap();
        upsert_normalized_record(&mut tx, "none", "queued_operations", "b-appended", &appended).await.unwrap();
        tx.commit().await.unwrap();
        pool.close().await;
        let pool = open_database(&path).await.unwrap();
        migrate_database(&pool).await.unwrap();
        let actual = read_normalized_sync_state(&pool, "none").await.unwrap().unwrap();
        assert_eq!(actual["queued_operations"], json!([updated, snapshot["sync_state"]["queued_operations"][1], snapshot["sync_state"]["queued_operations"][2], appended]));
        let mut tx = pool.begin().await.unwrap();
        delete_snapshot_bridge_and_normalized_record(&mut tx, "queued_operations", "z-content-op").await.unwrap();
        tx.commit().await.unwrap();
        pool.close().await;
        let pool = open_database(&path).await.unwrap();
        migrate_database(&pool).await.unwrap();
        let actual = read_normalized_sync_state(&pool, "none").await.unwrap().unwrap();
        assert_eq!(actual["queued_operations"].as_array().unwrap().len(), 3);
        assert!(!actual["queued_operations"].as_array().unwrap().iter().any(|op| op["id"] == "z-content-op"));
        assert_eq!(actual["queued_operations"][0], snapshot["sync_state"]["queued_operations"][1]);
        assert_eq!(actual["queued_operations"][1], snapshot["sync_state"]["queued_operations"][2]);
        assert_eq!(actual["queued_operations"][2], appended);
        assert_eq!(actual["future_state"], snapshot["sync_state"]["future_state"]);
        pool.close().await;
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn replay_preupgrade_zero_positions_use_complete_cursor_or_whole_legacy_dispatch_order() {
        for kind in ["complete_cursor", "no_cursor", "changed_and_new_partial"] {
            let pool = atomic_test_pool().await;
            let snapshot = replay_roundtrip_snapshot();
            write_snapshot_to_database(&pool, "none", &snapshot).await.unwrap();
            // Exact incumbent persisted shape: baseline partial upsert wrote
            // row_order.unwrap_or(0) for EVERY current raw queue record.
            sqlx::query("DELETE FROM local_vault_metadata WHERE key='replay_queue_order_format'").execute(&pool).await.unwrap();
            sqlx::query("UPDATE local_vault_records SET row_order=0 WHERE collection='queued_operations'").execute(&pool).await.unwrap();
            let mut current = snapshot["sync_state"]["queued_operations"].as_array().unwrap().clone();
            if kind == "no_cursor" { sqlx::query("DELETE FROM local_sync_cursors").execute(&pool).await.unwrap(); }
            if kind == "changed_and_new_partial" {
                current[0]["payload"]["content"] = json!("Real earlier partial edit");
                let appended = json!({"id":"b-legacy-partial","entity_type":"document","entity_id":"doc-1","action":"update_content","queued_at":"2020-01-01T00:00:00Z","payload":null,"replay_provider":"google-drive","server_context":{"server_base":"https://old.example.test/api/v1","account_id":"old-account","session_scope":"old-session"},"future_operation":{"retained":true}});
                let mut tx = pool.begin().await.unwrap();
                sqlx::query("UPDATE local_vault_records SET payload=?1 WHERE collection='queued_operations' AND id='z-content-op'")
                    .bind(current[0].to_string()).execute(&mut *tx).await.unwrap();
                upsert_normalized_record(&mut tx,"none","queued_operations","z-content-op",&current[0]).await.unwrap();
                sqlx::query("INSERT INTO local_vault_records(collection,id,row_order,payload,updated_at) VALUES('queued_operations','b-legacy-partial',0,?1,NULL)")
                    .bind(appended.to_string()).execute(&mut *tx).await.unwrap();
                upsert_normalized_record(&mut tx,"none","queued_operations","b-legacy-partial",&appended).await.unwrap();
                tx.commit().await.unwrap(); current.push(appended);
            }
            let raw_before: Vec<(String,String,i64)> = sqlx::query_as("SELECT id,payload,row_order FROM local_vault_records WHERE collection='queued_operations' ORDER BY id").fetch_all(&pool).await.unwrap();
            let mut expected = current.clone();
            if kind != "complete_cursor" { expected.sort_by_key(|v| (v["queued_at"].as_str().unwrap().to_string(),v["id"].as_str().unwrap().to_string())); }
            assert_eq!(read_normalized_sync_state(&pool,"none").await.unwrap().unwrap()["queued_operations"],Value::Array(expected.clone()),"{kind}");
            assert_eq!(raw_before,sqlx::query_as::<_,(String,String,i64)>("SELECT id,payload,row_order FROM local_vault_records WHERE collection='queued_operations' ORDER BY id").fetch_all(&pool).await.unwrap());
            let marker: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM local_vault_metadata WHERE key='replay_queue_order_format'").fetch_one(&pool).await.unwrap();
            assert_eq!(marker.0,0,"read must not rewrite legacy disk");
            let mut changed = current.iter().find(|v|v["id"]=="z-content-op").unwrap().clone();
            changed["payload"]["content"] = json!("Current adopted partial edit");
            let mut tx = pool.begin().await.unwrap();
            upsert_snapshot_bridge_record(&mut tx,"queued_operations","z-content-op",&changed,None).await.unwrap();
            upsert_normalized_record(&mut tx,"none","queued_operations","z-content-op",&changed).await.unwrap();
            tx.commit().await.unwrap();
            *expected.iter_mut().find(|v|v["id"]=="z-content-op").unwrap()=changed;
            assert_eq!(read_normalized_sync_state(&pool,"none").await.unwrap().unwrap()["queued_operations"],Value::Array(expected));
            let marker: (String,) = sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key='replay_queue_order_format'").fetch_one(&pool).await.unwrap();
            assert_eq!(marker.0,if kind=="complete_cursor" {"1:legacy_cursor"}else{"1:legacy_dispatch"});
        }
    }

    #[tokio::test]
    async fn replay_mixed_current_and_stale_cursor_preserves_each_valid_witness_during_adoption_reopen() {
        let path=replay_file_test_path("mixed-cursor");
        let pool=open_database(&path).await.unwrap();
        migrate_database(&pool).await.unwrap();
        let mut snapshot=replay_roundtrip_snapshot();
        // Original array z,a; incumbent fallback a,z. A remains fully current,
        // while an old partial change to B exists only in normalized columns.
        let _ = snapshot["sync_state"]["queued_operations"].as_array_mut().unwrap().pop();
        snapshot["sync_state"]["queued_operations"][1]["server_context"]=json!({"server_base":"https://old.example.test/api/v1","account_id":"original-account","session_scope":"original-session"});
        snapshot["sync_state"]["queued_operations"][1]["future_operation"]=json!({"exact":"retained"});
        let mut a=snapshot["sync_state"]["queued_operations"][1].clone();
        write_snapshot_to_database(&pool,"none",&snapshot).await.unwrap();
        sqlx::query("DELETE FROM local_vault_records WHERE collection='queued_operations'").execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM local_vault_metadata WHERE key='replay_queue_order_format'").execute(&pool).await.unwrap();
        let replacement=json!({"content":"Current column-only partial body"});
        sqlx::query("UPDATE local_queued_operations SET payload=?1 WHERE id='z-content-op'").bind(replacement.to_string()).execute(&pool).await.unwrap();
        pool.close().await;
        let pool=open_database(&path).await.unwrap(); migrate_database(&pool).await.unwrap();
        let before=read_normalized_sync_state(&pool,"none").await.unwrap().unwrap()["queued_operations"].clone();
        assert_eq!(before[0],a,"current A keeps original provider/account/future metadata despite stale B");
        assert_eq!(before[1]["id"],json!("z-content-op"));
        assert_eq!(before[1]["payload"],replacement);
        assert!(before[1].get("server_context").is_none());
        assert!(before[1].get("replay_provider").is_none());
        assert!(before[1].get("future_operation").is_none());
        a["payload"]["human_identifier"]=json!("NEW-1");
        let mut tx=pool.begin().await.unwrap();
        upsert_snapshot_bridge_record(&mut tx,"queued_operations","a-identifier-op",&a,None).await.unwrap();
        upsert_normalized_record(&mut tx,"none","queued_operations","a-identifier-op",&a).await.unwrap();
        tx.commit().await.unwrap();pool.close().await;
        let pool=open_database(&path).await.unwrap();migrate_database(&pool).await.unwrap();
        let actual=read_normalized_sync_state(&pool,"none").await.unwrap().unwrap()["queued_operations"].clone();
        assert_eq!(actual,json!([a,before[1]]),"adoption keeps per-row witnesses with one whole fallback order");
        let marker:(String,)=sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key='replay_queue_order_format'").fetch_one(&pool).await.unwrap();
        assert_eq!(marker.0,"1:legacy_dispatch");
        let mut tx=pool.begin().await.unwrap();
        delete_snapshot_bridge_and_normalized_record(&mut tx,"queued_operations","z-content-op").await.unwrap();
        tx.commit().await.unwrap();pool.close().await;
        let pool=open_database(&path).await.unwrap();migrate_database(&pool).await.unwrap();
        assert_eq!(read_normalized_sync_state(&pool,"none").await.unwrap().unwrap()["queued_operations"],json!([a]));
        pool.close().await;std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn replay_current_cursor_invalid_present_metadata_never_downgrades_to_unbound() {
        for (field,value) in [("server_context",Value::Null),("replay_provider",json!("unknown")),("attempts",json!("bad")),("last_error",Value::Null),("entity_id",json!(" "))] {
            let pool=atomic_test_pool().await;
            let mut snapshot=replay_roundtrip_snapshot();
            snapshot["sync_state"]["queued_operations"].as_array_mut().unwrap().truncate(1);
            write_snapshot_to_database(&pool,"none",&snapshot).await.unwrap();
            sqlx::query("DELETE FROM local_vault_records WHERE collection='queued_operations'").execute(&pool).await.unwrap();
            sqlx::query("DELETE FROM local_vault_metadata WHERE key='replay_queue_order_format'").execute(&pool).await.unwrap();
            snapshot["sync_state"]["queued_operations"][0][field]=value;
            if field=="entity_id" { sqlx::query("UPDATE local_queued_operations SET entity_id=' ' WHERE id='z-content-op'").execute(&pool).await.unwrap(); }
            sqlx::query("UPDATE local_sync_cursors SET payload=?1 WHERE provider='none'").bind(snapshot["sync_state"].to_string()).execute(&pool).await.unwrap();
            let cursors:Vec<(String,String)>=sqlx::query_as("SELECT provider,payload FROM local_sync_cursors ORDER BY provider").fetch_all(&pool).await.unwrap();
            let columns:Vec<(String,String,String,i64,Option<String>)>=sqlx::query_as("SELECT id,entity_id,queued_at,attempts,payload FROM local_queued_operations ORDER BY id").fetch_all(&pool).await.unwrap();
            assert!(read_normalized_sync_state(&pool,"none").await.is_err(),"current {field} corruption must remain visible");
            let mut tx=pool.begin().await.unwrap();
            assert!(upsert_snapshot_bridge_record(&mut tx,"queued_operations","z-content-op",&snapshot["sync_state"]["queued_operations"][0],None).await.is_err(),"UPSERT must reject {field}");
            tx.rollback().await.unwrap();
            let mut tx=pool.begin().await.unwrap();
            assert!(delete_snapshot_bridge_and_normalized_record(&mut tx,"queued_operations","z-content-op").await.is_err(),"DELETE must reject {field}");
            tx.rollback().await.unwrap();
            assert_eq!(cursors,sqlx::query_as::<_,(String,String)>("SELECT provider,payload FROM local_sync_cursors ORDER BY provider").fetch_all(&pool).await.unwrap());
            assert_eq!(columns,sqlx::query_as::<_,(String,String,String,i64,Option<String>)>("SELECT id,entity_id,queued_at,attempts,payload FROM local_queued_operations ORDER BY id").fetch_all(&pool).await.unwrap());
            let raw_count:(i64,)=sqlx::query_as("SELECT COUNT(*) FROM local_vault_records WHERE collection='queued_operations'").fetch_one(&pool).await.unwrap();
            let marker_count:(i64,)=sqlx::query_as("SELECT COUNT(*) FROM local_vault_metadata WHERE key='replay_queue_order_format'").fetch_one(&pool).await.unwrap();
            assert_eq!(raw_count.0,0);assert_eq!(marker_count.0,0,"invalid evidence must not be adopted");
        }
    }

    #[tokio::test]
    async fn replay_legacy_delete_adopts_order_but_marked_duplicates_and_unknown_formats_reject() {
        for has_cursor in [true,false] {
            let pool=atomic_test_pool().await;
            let snapshot=replay_roundtrip_snapshot();
            write_snapshot_to_database(&pool,"none",&snapshot).await.unwrap();
            sqlx::query("DELETE FROM local_vault_metadata WHERE key='replay_queue_order_format'").execute(&pool).await.unwrap();
            sqlx::query("UPDATE local_vault_records SET row_order=0 WHERE collection='queued_operations'").execute(&pool).await.unwrap();
            if !has_cursor { sqlx::query("DELETE FROM local_sync_cursors").execute(&pool).await.unwrap(); }
            let mut expected=snapshot["sync_state"]["queued_operations"].as_array().unwrap().clone();
            if !has_cursor { expected.sort_by_key(|v| (v["queued_at"].as_str().unwrap().to_string(),v["id"].as_str().unwrap().to_string())); }
            expected.retain(|v|v["id"]!="a-identifier-op");
            sqlx::query("CREATE TRIGGER queue_delete_fail BEFORE DELETE ON local_queued_operations BEGIN SELECT RAISE(ABORT,'deliberate queue delete failure'); END").execute(&pool).await.unwrap();
            let before:Vec<(String,String,i64)>=sqlx::query_as("SELECT id,payload,row_order FROM local_vault_records WHERE collection='queued_operations' ORDER BY id").fetch_all(&pool).await.unwrap();
            let mut tx=pool.begin().await.unwrap();
            assert!(delete_snapshot_bridge_and_normalized_record(&mut tx,"queued_operations","a-identifier-op").await.is_err());
            tx.rollback().await.unwrap();
            assert_eq!(before,sqlx::query_as::<_,(String,String,i64)>("SELECT id,payload,row_order FROM local_vault_records WHERE collection='queued_operations' ORDER BY id").fetch_all(&pool).await.unwrap());
            let marker_count:(i64,)=sqlx::query_as("SELECT COUNT(*) FROM local_vault_metadata WHERE key='replay_queue_order_format'").fetch_one(&pool).await.unwrap();
            assert_eq!(marker_count.0,0,"failed delete must roll back adopted positions and marker");
            sqlx::query("DROP TRIGGER queue_delete_fail").execute(&pool).await.unwrap();
            let mut tx=pool.begin().await.unwrap();
            delete_snapshot_bridge_and_normalized_record(&mut tx,"queued_operations","a-identifier-op").await.unwrap();
            tx.commit().await.unwrap();
            assert_eq!(read_normalized_sync_state(&pool,"none").await.unwrap().unwrap()["queued_operations"],Value::Array(expected));
            let marker:(String,)=sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key='replay_queue_order_format'").fetch_one(&pool).await.unwrap();
            assert_eq!(marker.0,if has_cursor {"1:legacy_cursor"}else{"1:legacy_dispatch"});
        }
        for corruption in ["duplicate","unknown","malformed"] {
            let pool=atomic_test_pool().await;
            let snapshot=replay_roundtrip_snapshot();
            write_snapshot_to_database(&pool,"none",&snapshot).await.unwrap();
            match corruption {
                "duplicate" => { sqlx::query("UPDATE local_vault_records SET row_order=0 WHERE collection='queued_operations'").execute(&pool).await.unwrap(); },
                "unknown" => { sqlx::query("UPDATE local_vault_metadata SET value='2:future' WHERE key='replay_queue_order_format'").execute(&pool).await.unwrap(); },
                _ => { sqlx::query("UPDATE local_vault_metadata SET value='garbage' WHERE key='replay_queue_order_format'").execute(&pool).await.unwrap(); },
            }
            assert!(read_normalized_sync_state(&pool,"none").await.is_err());
            let before:Vec<(String,String,i64)>=sqlx::query_as("SELECT id,payload,row_order FROM local_vault_records WHERE collection='queued_operations' ORDER BY id").fetch_all(&pool).await.unwrap();
            let mut tx=pool.begin().await.unwrap();
            assert!(delete_snapshot_bridge_and_normalized_record(&mut tx,"queued_operations","z-content-op").await.is_err());
            tx.rollback().await.unwrap();
            let mut tx=pool.begin().await.unwrap();
            assert!(upsert_snapshot_bridge_record(&mut tx,"queued_operations","z-content-op",&snapshot["sync_state"]["queued_operations"][0],None).await.is_err());
            tx.rollback().await.unwrap();
            assert_eq!(before,sqlx::query_as::<_,(String,String,i64)>("SELECT id,payload,row_order FROM local_vault_records WHERE collection='queued_operations' ORDER BY id").fetch_all(&pool).await.unwrap());
        }
    }

    #[tokio::test]
    async fn replay_only_attempts_and_error_overlay_raw_evidence() {
        let pool = atomic_test_pool().await;
        let snapshot = replay_roundtrip_snapshot();
        write_snapshot_to_database(&pool, "none", &snapshot).await.unwrap();
        sqlx::query("UPDATE local_queued_operations SET attempts=2,last_error='Network unavailable' WHERE id='z-content-op'").execute(&pool).await.unwrap();
        sqlx::query("UPDATE local_queued_operations SET last_error=NULL WHERE id='m-maintenance-op'").execute(&pool).await.unwrap();
        let mut expected = snapshot["sync_state"]["queued_operations"].clone();
        expected[0]["attempts"] = json!(2);
        expected[0]["last_error"] = json!("Network unavailable");
        expected[2].as_object_mut().unwrap().remove("last_error");
        assert_eq!(read_normalized_sync_state(&pool, "none").await.unwrap().unwrap()["queued_operations"], expected);
    }

    #[tokio::test]
    async fn replay_cursor_legacy_fallback_requires_complete_unique_tuple() {
        let pool = atomic_test_pool().await;
        let snapshot = replay_roundtrip_snapshot();
        write_snapshot_to_database(&pool, "none", &snapshot).await.unwrap();
        sqlx::query("DELETE FROM local_vault_records WHERE collection='queued_operations'").execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM local_vault_metadata WHERE key='replay_queue_order_format'").execute(&pool).await.unwrap();
        assert_eq!(read_normalized_sync_state(&pool, "none").await.unwrap().unwrap()["queued_operations"], snapshot["sync_state"]["queued_operations"]);
        let mut ambiguous = snapshot["sync_state"].clone();
        let duplicate = ambiguous["queued_operations"][0].clone();
        ambiguous["queued_operations"].as_array_mut().unwrap().push(duplicate);
        sqlx::query("UPDATE local_sync_cursors SET payload=?1 WHERE provider='none'").bind(ambiguous.to_string()).execute(&pool).await.unwrap();
        assert!(read_normalized_sync_state(&pool, "none").await.is_err());
    }

    #[tokio::test]
    async fn replay_malformed_or_mismatched_evidence_never_reconstructs_authority() {
        for sql in [
            "UPDATE local_vault_records SET payload='invalid-json' WHERE collection='queued_operations' AND id='z-content-op'",
            "UPDATE local_queued_operations SET entity_id='different-page' WHERE id='z-content-op'",
            "UPDATE local_queued_operations SET payload='invalid-json' WHERE id='z-content-op'",
            "UPDATE local_queued_operations SET queued_at='different-time' WHERE id='z-content-op'",
            "DELETE FROM local_queued_operations WHERE id='z-content-op'",
            "UPDATE local_sync_cursors SET payload='invalid-json' WHERE provider='none'",
            "UPDATE local_vault_records SET row_order=1 WHERE collection='queued_operations' AND id='z-content-op'",
            "UPDATE local_vault_records SET payload=json_set(payload,'$.entity_id',' ') WHERE collection='queued_operations' AND id='z-content-op'; UPDATE local_queued_operations SET entity_id=' ' WHERE id='z-content-op'",
            "UPDATE local_vault_records SET payload=json_set(payload,'$.server_context.account_id',NULL) WHERE collection='queued_operations' AND id='z-content-op'",
            "UPDATE local_vault_records SET payload=json_set(payload,'$.replay_provider','unknown') WHERE collection='queued_operations' AND id='z-content-op'",
        ] {
            let pool = atomic_test_pool().await;
            write_snapshot_to_database(&pool, "none", &replay_roundtrip_snapshot()).await.unwrap();
            for statement in sql.split(';') { sqlx::query(statement).execute(&pool).await.unwrap(); }
            assert!(read_normalized_sync_state(&pool, "none").await.is_err(), "must reject {sql}");
        }
    }

    #[tokio::test]
    async fn replay_unbound_legacy_columns_never_gain_provider_or_account() {
        let pool = atomic_test_pool().await;
        sqlx::query("INSERT INTO local_queued_operations(id,entity_type,entity_id,action,queued_at,attempts,payload) VALUES('legacy','document','doc-1','update_content','old',0,'null')").execute(&pool).await.unwrap();
        let actual = read_normalized_sync_state(&pool, "none").await.unwrap().unwrap();
        assert_eq!(actual["queued_operations"][0]["payload"], Value::Null);
        assert!(actual["queued_operations"][0].get("server_context").is_none());
        assert!(actual["queued_operations"][0].get("replay_provider").is_none());
        sqlx::query("UPDATE local_queued_operations SET action='update_identifier' WHERE id='legacy'").execute(&pool).await.unwrap();
        assert!(read_normalized_sync_state(&pool, "none").await.is_err());
    }

    #[tokio::test]
    async fn atomic_snapshot_late_sql_failure_rolls_back_values_queue_and_marker() {
        let pool = atomic_test_pool().await;
        let before = property_commit_snapshot("Original", "original-operation");
        let receipt = commit_snapshot_and_mirror(&pool, "none", &before, None, || Ok(())).await.expect("initial commit");
        assert!(receipt.canonical_committed);
        assert!(!receipt.mirror_pending);
        let mut candidate = property_commit_snapshot("Changed", "duplicate-operation");
        let duplicate = candidate["sync_state"]["queued_operations"][0].clone();
        candidate["sync_state"]["queued_operations"].as_array_mut().unwrap().push(duplicate);
        let mirror_called = std::cell::Cell::new(false);
        let result = commit_snapshot_and_mirror(&pool, "none", &candidate, Some(&receipt.canonical_revision), || { mirror_called.set(true); Ok(()) }).await;
        assert!(result.is_err());
        assert!(!mirror_called.get());
        assert_eq!(read_snapshot_bridge_collection(&pool, "property_values").await.unwrap(), before["property_values"].as_array().unwrap().clone());
        let queue_ids: Vec<(String,)> = sqlx::query_as("SELECT id FROM local_queued_operations").fetch_all(&pool).await.unwrap();
        assert_eq!(queue_ids, vec![("original-operation".into(),)]);
        let pending: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM local_vault_metadata WHERE key = 'mirror_pending'").fetch_one(&pool).await.unwrap();
        assert_eq!(pending.0, 0);
    }

    #[tokio::test]
    async fn stale_snapshot_revision_rejects_before_replacing_canonical_data() {
        let pool = atomic_test_pool().await;
        let initial = property_commit_snapshot("Initial", "initial");
        let first = commit_snapshot_and_mirror(&pool, "none", &initial, None, || Ok(())).await.unwrap();
        let newer = property_commit_snapshot("Another window", "newer");
        let second = commit_snapshot_and_mirror(&pool, "none", &newer, Some(&first.canonical_revision), || Ok(())).await.unwrap();
        let stale = property_commit_snapshot("Stale edit", "stale");
        let error = commit_snapshot_and_mirror(&pool, "none", &stale, Some(&first.canonical_revision), || Ok(())).await.unwrap_err();
        assert!(error.contains("another window"));
        assert_eq!(read_snapshot_bridge_collection(&pool, "property_values").await.unwrap(), newer["property_values"].as_array().unwrap().clone());
        let revision: (String,) = sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key = 'canonical_revision'").fetch_one(&pool).await.unwrap();
        assert_eq!(revision.0, second.canonical_revision);
    }

    #[tokio::test]
    async fn older_mirror_completion_preserves_newer_pending_marker() {
        let pool = atomic_test_pool().await;
        let first = commit_snapshot_and_mirror(&pool, "none", &property_commit_snapshot("Initial", "first"), None, || Err("mirror failed".into())).await.unwrap();
        let second = commit_snapshot_and_mirror(&pool, "none", &property_commit_snapshot("Newer", "second"), Some(&first.canonical_revision), || Err("mirror failed again".into())).await.unwrap();
        let stale_mirror = finish_snapshot_mirror(&pool, &first.canonical_revision, || Ok(())).await;
        assert!(stale_mirror.mirror_pending);
        let pending: (String,) = sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key = 'mirror_pending'").fetch_one(&pool).await.unwrap();
        assert_eq!(pending.0, second.canonical_revision);
    }

    #[tokio::test]
    async fn committed_snapshot_reports_mirror_failure_and_recovers_without_replaying_mutation() {
        let path = std::env::temp_dir().join(format!("starlane-atomic-{}-{}.sqlite", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let pool = open_database(&path).await.unwrap();
        migrate_database(&pool).await.unwrap();
        let candidate = property_commit_snapshot("Committed", "one-operation");
        let receipt = commit_snapshot_and_mirror(&pool, "none", &candidate, None, || Err("Mirror disk unavailable".into())).await.expect("canonical commit succeeds");
        assert!(receipt.canonical_committed);
        assert!(receipt.mirror_pending);
        assert_eq!(receipt.mirror_error.as_deref(), Some("Mirror disk unavailable"));
        pool.close().await;
        // A restart reads canonical committed values and the persisted retry marker.
        let pool = open_database(&path).await.unwrap();
        assert_eq!(read_snapshot_bridge_collection(&pool, "property_values").await.unwrap(), candidate["property_values"].as_array().unwrap().clone());
        let pending: (String,) = sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key = 'mirror_pending'").fetch_one(&pool).await.unwrap();
        let recovered = finish_snapshot_mirror(&pool, &pending.0, || Ok(())).await;
        assert!(recovered.canonical_committed);
        assert!(!recovered.mirror_pending);
        let counts: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM local_queued_operations").fetch_one(&pool).await.unwrap();
        assert_eq!(counts.0, 1);
        let pending: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM local_vault_metadata WHERE key = 'mirror_pending'").fetch_one(&pool).await.unwrap();
        assert_eq!(pending.0, 0);
        pool.close().await;
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn maintenance_catalog_bound_queue_reopen_cas_and_clear() {
        let pool = atomic_test_pool().await;
        let mut snapshot = json!({"schema_version":1,"documents":[{"id":"page-maintenance","space_id":"space-1","title":"Page","maintenance_user_id":null,"maintenance_group_id":"team-a"}],"sync_state":{"state":"idle","queued_operations":[{"id":"maintenance-op","entity_type":"document","entity_id":"page-maintenance","action":"update_maintenance","queued_at":"2026-10-08T00:00:00Z","attempts":0,"replay_provider":"google-drive","server_context":{"server_base":"https://notes.example.test/api/v1","account_id":"account-a","session_scope":"session-a"},"payload":{"id":"page-maintenance","space_id":"space-1","maintenance":{"kind":"group","group_id":"team-a"},"source":{"page_updated_at":"2026-10-08T00:00:00Z","published_at":null,"maintenance_user_id":null,"maintenance_group_id":null}}}]}});
        snapshot["maintenance_target_catalogs"] = json!([{"server_base":"https://notes.example.test/api/v1","account_id":"account-a","space_id":"space-1","targets":{"users":[],"groups":[{"id":"team-a","name":"Team","managed_space_id":null}],"groups_available":true}}]);
        let revision = write_snapshot_with_revision(&pool, "google-drive", &snapshot, None, true)
            .await
            .unwrap();
        assert_eq!(
            read_maintenance_catalogs(&pool).await.unwrap().unwrap(),
            snapshot["maintenance_target_catalogs"]
        );
        assert_eq!(
            read_normalized_payload_collection(&pool, "local_documents", "space_id,title,id")
                .await
                .unwrap()[0],
            snapshot["documents"][0]
        );
        assert_eq!(
            read_normalized_sync_state(&pool, "google-drive")
                .await
                .unwrap()
                .unwrap()["queued_operations"][0],
            snapshot["sync_state"]["queued_operations"][0]
        );
        let mut stale = snapshot.clone();
        stale["documents"][0]["maintenance_group_id"] = json!("different");
        assert!(
            write_snapshot_with_revision(&pool, "google-drive", &stale, None, true)
                .await
                .is_err()
        );
        assert_eq!(
            read_maintenance_catalogs(&pool).await.unwrap().unwrap(),
            snapshot["maintenance_target_catalogs"]
        );
        sqlx::query("UPDATE local_queued_operations SET entity_id='different-page' WHERE id='maintenance-op'").execute(&pool).await.unwrap();
        assert!(read_normalized_sync_state(&pool, "google-drive")
            .await
            .unwrap_err()
            .contains("differs"));
        let mut empty = snapshot.clone();
        empty["maintenance_target_catalogs"] = json!([]);
        write_snapshot_with_revision(&pool, "google-drive", &empty, Some(&revision), true)
            .await
            .unwrap();
        assert_eq!(
            read_maintenance_catalogs(&pool).await.unwrap(),
            Some(json!([]))
        );
        empty
            .as_object_mut()
            .unwrap()
            .remove("maintenance_target_catalogs");
        write_snapshot_to_database(&pool, "google-drive", &empty)
            .await
            .unwrap();
        assert_eq!(read_maintenance_catalogs(&pool).await.unwrap(), None);
    }
    #[tokio::test]
    async fn identifier_payload_queue_and_cas_roundtrip_preserve_exact_known_fields() {
        let pool = atomic_test_pool().await;
        let snapshot = json!({"schema_version":1,
            "documents":[{"id":"doc-identifier","space_id":"space-1","title":"Runbook","content":"Exact body","human_identifier":"ENG-0042","archived_at":"2026-10-08T00:00:00Z"}],
            "sync_state":{"state":"idle","queued_operations":[{"id":"identifier-op","entity_type":"document","entity_id":"doc-identifier","action":"update_identifier","queued_at":"2026-10-08T00:00:00Z","attempts":0,"replay_provider":"google-drive","server_context":{"server_base":"https://notes.example.test/api/v1","account_id":"account-a","session_scope":"session-a"},"payload":{"id":"doc-identifier","space_id":"space-1","human_identifier":"ENG-0042","source":{"page_updated_at":"2026-10-08T00:00:00Z","published_at":null,"human_identifier":null}}}]}});
        let revision = write_snapshot_with_revision(&pool, "google-drive", &snapshot, None, true)
            .await
            .expect("canonical commit");
        let pages =
            read_normalized_payload_collection(&pool, "local_documents", "space_id, title, id")
                .await
                .expect("read page payload");
        assert_eq!(pages[0], snapshot["documents"][0]);
        let sync = read_normalized_sync_state(&pool, "google-drive")
            .await
            .expect("read operation payload")
            .unwrap();
        assert_eq!(
            sync["queued_operations"][0],
            snapshot["sync_state"]["queued_operations"][0]
        );
        let mut stale = snapshot.clone();
        stale["documents"][0]["human_identifier"] = json!("ENG-STALE");
        assert!(
            write_snapshot_with_revision(&pool, "google-drive", &stale, None, true)
                .await
                .is_err()
        );
        assert_eq!(
            read_normalized_payload_collection(&pool, "local_documents", "space_id, title, id")
                .await
                .unwrap()[0]["human_identifier"],
            "ENG-0042"
        );
        assert!(!revision.is_empty());
        sqlx::query("UPDATE local_queued_operations SET entity_id = 'different-page' WHERE id = 'identifier-op'").execute(&pool).await.unwrap();
        assert!(read_normalized_sync_state(&pool, "google-drive")
            .await
            .unwrap_err()
            .contains("differs"));
    }


    fn move_evidence(id:&str)->Value {
        json!({"schema_version":1,"id":id,"provider":"none","actor_id":"local-owner","moved_at":"original-stamp","intent":{"root":{"kind":"folder","id":"folder-original"},"destination":{"space_id":"destination"}},"decisions":{"tag":"preserve"},"before":{"documents":[{"id":"doc-1","space_id":"source","body":"EXACT\r\nBYTES","future":null}]},"after":{"documents":[{"id":"doc-1","space_id":"destination","body":"EXACT\r\nBYTES","future":null}]},"removed":{"property_values":[{"id":"raw-value","value":" [null,1] "}]},"future_evidence":{"raw":[null,"KEEP"]}})
    }
    fn reviewed_draft_transfer_evidence(operation_id:&str)->Value {
        json!({"schema_version":1,"operation_id":operation_id,"state":"pending","request":{"schema_version":1,"operation_id":operation_id,"future":{"raw":[null,"KEEP"]}},"exact_manifest":{"schema_version":7,"operations":[{"id":"asset-op","payload":{"data_url":"data:text/plain;base64,SGVsbG8="}}]},"prepared_at":"original-stamp","updated_at":"original-stamp"})
    }
    #[tokio::test]
    async fn reviewed_draft_transfer_full_snapshot_real_reopen_keeps_order_and_raw_metadata() {
        assert!(COLLECTIONS.contains(&"reviewed_draft_transfer_records"));
        let path=replay_file_test_path("reviewed-draft-transfer-evidence");
        let pool=open_database(&path).await.unwrap();migrate_database(&pool).await.unwrap();
        let mut snapshot=property_commit_snapshot("Unchanged","original-queued-operation");
        snapshot["reviewed_draft_transfer_records"]=json!([reviewed_draft_transfer_evidence("z-operation"),reviewed_draft_transfer_evidence("a-operation")]);
        write_snapshot_to_database(&pool,"none",&snapshot).await.unwrap();pool.close().await;
        let pool=open_database(&path).await.unwrap();migrate_database(&pool).await.unwrap();
        assert_eq!(read_snapshot_bridge_collection(&pool,"reviewed_draft_transfer_records").await.unwrap(),snapshot["reviewed_draft_transfer_records"].as_array().unwrap().clone());
        pool.close().await;std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn reviewed_move_evidence_full_snapshot_real_reopen_keeps_order_and_raw_metadata() {
        assert!(COLLECTIONS.contains(&"reviewed_move_records"));
        let path=replay_file_test_path("reviewed-move-evidence");
        let pool=open_database(&path).await.unwrap();migrate_database(&pool).await.unwrap();
        let mut snapshot=property_commit_snapshot("Unchanged","original-queued-operation");
        snapshot["reviewed_move_records"]=json!([move_evidence("z-operation"),move_evidence("a-operation")]);
        let revision=write_snapshot_to_database(&pool,"none",&snapshot).await.unwrap();pool.close().await;
        let pool=open_database(&path).await.unwrap();migrate_database(&pool).await.unwrap();
        assert_eq!(read_snapshot_bridge_collection(&pool,"reviewed_move_records").await.unwrap(),snapshot["reviewed_move_records"].as_array().unwrap().clone());
        let current:(String,)=sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key='canonical_revision'").fetch_one(&pool).await.unwrap();assert_eq!(current.0,revision);
        assert_eq!(read_normalized_sync_state(&pool,"none").await.unwrap().unwrap()["queued_operations"],snapshot["sync_state"]["queued_operations"]);
        pool.close().await;std::fs::remove_file(path).unwrap();
    }
    #[tokio::test]
    async fn reviewed_move_evidence_sql_failure_rolls_back_entire_candidate_and_revision() {
        let pool=atomic_test_pool().await;let mut snapshot=property_commit_snapshot("Original","original-queue");
        snapshot["reviewed_move_records"]=json!([move_evidence("original-evidence")]);write_snapshot_to_database(&pool,"none",&snapshot).await.unwrap();
        sqlx::query("CREATE TRIGGER reject_move_evidence BEFORE INSERT ON local_vault_records WHEN NEW.collection='reviewed_move_records' AND NEW.id='new-evidence' BEGIN SELECT RAISE(ABORT,'fixture late evidence failure'); END").execute(&pool).await.unwrap();
        let before=snapshot_tx_capture(&pool).await;
        let mut candidate=property_commit_snapshot("Changed","new-queue");candidate["reviewed_move_records"]=json!([move_evidence("original-evidence"),move_evidence("new-evidence")]);
        assert!(write_snapshot_to_database(&pool,"none",&candidate).await.is_err());assert_eq!(snapshot_tx_capture(&pool).await,before);
    }
    #[tokio::test]
    async fn reviewed_move_evidence_malformed_duplicate_and_corrupt_bridge_fail_without_dropping_history() {
        for invalid in [Value::Null,json!({}),json!([{"id":" "}]),json!([move_evidence("same"),move_evidence("same")])] {
            let pool=atomic_test_pool().await;let mut original=property_commit_snapshot("Original","original-operation");original["reviewed_move_records"]=json!([move_evidence("original")]);write_snapshot_to_database(&pool,"none",&original).await.unwrap();
            let before=snapshot_tx_capture(&pool).await;let mut candidate=original.clone();candidate["reviewed_move_records"]=invalid;
            assert!(write_snapshot_to_database(&pool,"none",&candidate).await.is_err());assert_eq!(snapshot_tx_capture(&pool).await,before);
        }
        for corruption in ["UPDATE local_vault_records SET payload='invalid-json' WHERE collection='reviewed_move_records'", "UPDATE local_vault_records SET payload=json_set(payload,'$.id','changed') WHERE collection='reviewed_move_records'"] {
            let pool=atomic_test_pool().await;let mut snapshot=property_commit_snapshot("Original","original-operation");snapshot["reviewed_move_records"]=json!([move_evidence("original")]);write_snapshot_to_database(&pool,"none",&snapshot).await.unwrap();
            sqlx::query(corruption).execute(&pool).await.unwrap();let before=snapshot_tx_capture(&pool).await;assert!(read_snapshot_bridge_collection(&pool,"reviewed_move_records").await.is_err());assert_eq!(snapshot_tx_capture(&pool).await,before);
        }
    }
    #[tokio::test]
    async fn write_snapshot_populates_normalized_tables() {
        let database_path = std::env::temp_dir().join(format!(
            "starlane-native-local-db-test-{}.sqlite",
            std::process::id()
        ));
        if database_path.exists() {
            std::fs::remove_file(&database_path).ok();
        }
        let pool = open_database(&database_path).await.expect("open database");
        migrate_database(&pool).await.expect("migrate database");
        let snapshot = json!({
            "schema_version": 1,
            "documents": [{
                "id": "doc-1",
                "space_id": "space-1",
                "title": "Runbook",
                "content": "body",
                "updated_at": "2026-07-08T12:00:00.000Z"
            }],
            "folders": [{
                "id": "folder-1",
                "space_id": "space-1",
                "name": "Ops"
            }],
            "tags": [{
                "id": "tag-1",
                "space_id": "space-1",
                "name": "Ready",
                "color": "#2563eb",
                "created_at": "2026-07-08T12:00:00.000Z"
            }],
            "property_schemas": [{
                "id": "property-1",
                "space_id": "space-1",
                "name": "Status",
                "property_type": "select"
            }],
            "spaces": [{
                "id": "space-1",
                "name": "Operations"
            }],
            "assets": [{
                "id": "asset-1",
                "space_id": "space-1",
                "document_id": "doc-1",
                "name": "diagram.png"
            }],
            "templates": [{
                "id": "template-1",
                "space_id": "space-1",
                "title": "Incident"
            }],
            "workflow_templates": [{
                "id": "workflow-template-1",
                "space_id": "space-1",
                "name": "Review"
            }],
            "document_workflows": [{
                "document_id": "doc-1",
                "space_id": "space-1",
                "workflow_template_id": "workflow-template-1"
            }],
            "document_workflow_events": [{
                "id": "workflow-event-1",
                "document_id": "doc-1",
                "document_workflow_id": "doc-1",
                "created_at": "2026-07-08T12:00:00.000Z"
            }],
            "property_values": [{
                "document_id": "doc-1",
                "property_id": "property-1",
                "value": "Ready"
            }],
            "doc_tags": [{
                "document_id": "doc-1",
                "tag_id": "tag-1",
                "space_id": "space-1"
            }],
            "search_index": [{
                "document_id": "doc-1",
                "space_id": "space-1",
                "title": "Runbook",
                "content": "body",
                "updated_at": "2026-07-08T12:00:00.000Z"
            }],
            "sync_state": {
                "state": "idle",
                "updated_at": "2026-07-08T12:00:00.000Z",
                "queued_operations": [{
                    "id": "syncop-1",
                    "entity_type": "document",
                    "entity_id": "doc-1",
                    "action": "update_content",
                    "queued_at": "2026-07-08T12:00:00.000Z",
                    "attempts": 2,
                    "last_error": "retry later",
                    "payload": { "id": "doc-1", "content": "updated" }
                }],
                "recovery_checkpoints": [{
                    "spaceId": "space-1",
                    "spaceName": "Ops",
                    "sourceServerUrl": "https://notes.example.test",
                    "downloadedByUserId": "user-1",
                    "downloadedAt": "2026-07-08T12:05:00.000Z",
                    "documentCount": 1,
                    "folderCount": 1,
                    "tagCount": 1,
                    "propertyCount": 1
                }]
            }
        });

        write_snapshot_to_database(&pool, "google-drive", &snapshot)
            .await
            .expect("write snapshot");
        sqlx::query(
            "UPDATE local_vault_records SET payload = ?1 WHERE collection = ?2 AND id = ?3",
        )
        .bind(r#"{"id":"doc-1","space_id":"space-1","title":"Stale Bridge"}"#)
        .bind("documents")
        .bind("doc-1")
        .execute(&pool)
        .await
        .expect("mark document bridge stale");
        sqlx::query(
            "UPDATE local_vault_records SET payload = ?1 WHERE collection = ?2 AND id = ?3",
        )
        .bind(r#"{"id":"property-1","space_id":"space-1","name":"Stale Property","property_type":"text"}"#)
        .bind("property_schemas")
        .bind("property-1")
        .execute(&pool)
        .await
        .expect("mark property bridge stale");
        sqlx::query(
            "UPDATE local_vault_records SET payload = ?1 WHERE collection = ?2 AND id = ?3",
        )
        .bind(r#"{"id":"asset-1","space_id":"space-1","document_id":"doc-1","name":"stale.png"}"#)
        .bind("assets")
        .bind("asset-1")
        .execute(&pool)
        .await
        .expect("mark asset bridge stale");

        let document_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM local_documents")
            .fetch_one(&pool)
            .await
            .expect("document count");
        let normalized_documents =
            read_normalized_payload_collection(&pool, "local_documents", "space_id, title, id")
                .await
                .expect("read normalized documents");
        let bridge_documents = read_snapshot_bridge_collection(&pool, "documents")
            .await
            .expect("read bridge documents");
        let normalized_properties =
            read_normalized_payload_collection(&pool, "local_properties", "space_id, name, id")
                .await
                .expect("read normalized properties");
        let normalized_assets =
            read_normalized_payload_collection(&pool, "local_assets", "space_id, name, id")
                .await
                .expect("read normalized assets");
        let normalized_templates =
            read_normalized_payload_collection(&pool, "local_templates", "space_id, title, id")
                .await
                .expect("read normalized templates");
        let normalized_document_workflows = read_normalized_payload_collection(
            &pool,
            "local_document_workflows",
            "space_id, document_id, id",
        )
        .await
        .expect("read normalized document workflows");
        let normalized_property_values = read_normalized_payload_collection(
            &pool,
            "local_property_values",
            "document_id, property_id, id",
        )
        .await
        .expect("read normalized property values");
        let normalized_doc_tags =
            read_normalized_payload_collection(&pool, "local_doc_tags", "document_id, tag_id, id")
                .await
                .expect("read normalized doc tags");
        let normalized_search_index = read_normalized_payload_collection(
            &pool,
            "local_search_index",
            "space_id, updated_at, document_id",
        )
        .await
        .expect("read normalized search index");
        let queued: (String, i64, Option<String>, String) = sqlx::query_as(
            "SELECT action, attempts, last_error, payload FROM local_queued_operations WHERE id = ?1",
        )
        .bind("syncop-1")
        .fetch_one(&pool)
        .await
        .expect("queued operation");
        let tag_name: (String,) = sqlx::query_as("SELECT name FROM local_tags WHERE id = ?1")
            .bind("tag-1")
            .fetch_one(&pool)
            .await
            .expect("tag");
        let checkpoint: (String, Option<String>, String) = sqlx::query_as(
            "SELECT space_id, source_server_url, payload
             FROM local_replay_checkpoints WHERE id = ?1",
        )
        .bind("space-1")
        .fetch_one(&pool)
        .await
        .expect("replay checkpoint");
        let sync_state = read_normalized_sync_state(&pool, "google-drive")
            .await
            .expect("read normalized sync state")
            .expect("sync state");
        let queued_operations = sync_state
            .get("queued_operations")
            .and_then(Value::as_array)
            .expect("queued operations");
        let recovery_checkpoints = sync_state
            .get("recovery_checkpoints")
            .and_then(Value::as_array)
            .expect("recovery checkpoints");

        assert_eq!(document_count.0, 1);
        assert_eq!(
            normalized_documents[0].get("title").and_then(Value::as_str),
            Some("Runbook")
        );
        assert_eq!(
            bridge_documents[0].get("title").and_then(Value::as_str),
            Some("Stale Bridge")
        );
        assert_eq!(
            normalized_properties[0].get("name").and_then(Value::as_str),
            Some("Status")
        );
        assert_eq!(
            normalized_assets[0].get("name").and_then(Value::as_str),
            Some("diagram.png")
        );
        assert_eq!(
            normalized_templates[0].get("title").and_then(Value::as_str),
            Some("Incident")
        );
        assert_eq!(
            normalized_document_workflows[0]
                .get("workflow_template_id")
                .and_then(Value::as_str),
            Some("workflow-template-1")
        );
        assert_eq!(
            normalized_property_values[0]
                .get("value")
                .and_then(Value::as_str),
            Some("Ready")
        );
        assert_eq!(
            normalized_doc_tags[0].get("tag_id").and_then(Value::as_str),
            Some("tag-1")
        );
        assert_eq!(
            normalized_search_index[0]
                .get("document_id")
                .and_then(Value::as_str),
            Some("doc-1")
        );
        assert_eq!(queued.0, "update_content");
        assert_eq!(queued.1, 2);
        assert_eq!(queued.2.as_deref(), Some("retry later"));
        assert!(queued.3.contains("updated"));
        assert_eq!(tag_name.0, "Ready");
        assert_eq!(checkpoint.0, "space-1");
        assert_eq!(checkpoint.1.as_deref(), Some("https://notes.example.test"));
        assert!(checkpoint.2.contains("documentCount"));
        assert_eq!(
            sync_state.get("state").and_then(Value::as_str),
            Some("idle")
        );
        assert_eq!(queued_operations.len(), 1);
        assert_eq!(
            queued_operations[0].get("id").and_then(Value::as_str),
            Some("syncop-1")
        );
        assert_eq!(
            queued_operations[0].get("attempts").and_then(Value::as_i64),
            Some(2)
        );
        assert_eq!(recovery_checkpoints.len(), 1);
        assert_eq!(
            recovery_checkpoints[0]
                .get("sourceServerUrl")
                .and_then(Value::as_str),
            Some("https://notes.example.test")
        );

        drop(pool);
        std::fs::remove_file(database_path).ok();
    }

    #[tokio::test]
    async fn record_mutations_update_bridge_and_normalized_tables() {
        let database_path = std::env::temp_dir().join(format!(
            "starlane-native-local-db-record-test-{}.sqlite",
            std::process::id()
        ));
        if database_path.exists() {
            std::fs::remove_file(&database_path).ok();
        }
        let pool = open_database(&database_path).await.expect("open database");
        migrate_database(&pool).await.expect("migrate database");

        let document = json!({
            "id": "doc-direct",
            "space_id": "space-1",
            "title": "Direct Write",
            "content": "body",
            "updated_at": "2026-07-08T12:00:00.000Z"
        });
        let mut tx = pool.begin().await.expect("begin tx");
        upsert_snapshot_bridge_record(&mut tx, "documents", "doc-direct", &document, Some(7))
            .await
            .expect("upsert bridge");
        upsert_normalized_record(
            &mut tx,
            "google-drive",
            "documents",
            "doc-direct",
            &document,
        )
        .await
        .expect("upsert normalized");
        tx.commit().await.expect("commit upsert");

        let bridge: (i64, String) = sqlx::query_as(
            "SELECT row_order, payload FROM local_vault_records
             WHERE collection = ?1 AND id = ?2",
        )
        .bind("documents")
        .bind("doc-direct")
        .fetch_one(&pool)
        .await
        .expect("bridge record");
        let normalized =
            read_normalized_payload_collection(&pool, "local_documents", "space_id, title, id")
                .await
                .expect("read normalized documents");

        assert_eq!(bridge.0, 7);
        assert!(bridge.1.contains("Direct Write"));
        assert_eq!(
            normalized[0].get("title").and_then(Value::as_str),
            Some("Direct Write")
        );

        let mut tx = pool.begin().await.expect("begin delete tx");
        sqlx::query("DELETE FROM local_vault_records WHERE collection = ?1 AND id = ?2")
            .bind("documents")
            .bind("doc-direct")
            .execute(&mut *tx)
            .await
            .expect("delete bridge");
        delete_normalized_record(&mut tx, "documents", "doc-direct")
            .await
            .expect("delete normalized");
        tx.commit().await.expect("commit delete");

        let bridge_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM local_vault_records
             WHERE collection = ?1 AND id = ?2",
        )
        .bind("documents")
        .bind("doc-direct")
        .fetch_one(&pool)
        .await
        .expect("bridge count");
        let normalized_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM local_documents WHERE id = ?1")
                .bind("doc-direct")
                .fetch_one(&pool)
                .await
                .expect("normalized count");

        assert_eq!(bridge_count.0, 0);
        assert_eq!(normalized_count.0, 0);

        std::fs::remove_file(database_path).ok();
    }
}

#[cfg(all(test,not(any(target_os="ios",target_os="android"))))]
mod snapshot_selection_wait_tests {
    use super::*;
    use std::fs;
    fn fixture()->PathBuf {let path=std::env::temp_dir().join(format!("starlane-selection-db-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));fs::create_dir_all(&path).unwrap();path}
    fn empty_snapshot()->Value {json!({"schema_version":1,"spaces":[],"folders":[],"documents":[],"drafts":[],"assets":[],"templates":[]})}
    #[tokio::test]
    async fn actual_snapshot_mutex_wait_detects_changed_deleted_and_represented_selection_before_path_mutation() {
        for kind in ["changed","deleted","representation","malformed"] {
            let dir=fixture();let settings=dir.join("settings.json");let original=dir.join("original");let other=dir.join("other");fs::create_dir(&original).unwrap();let legacy=original.join("starlane-vault-none.sqlite3");fs::write(&legacy,b"original database sentinel").unwrap();
            let initial=if kind=="representation" {json!({"vault_path":null})} else {json!({"vault_path":original})};fs::write(&settings,serde_json::to_vec(&initial).unwrap()).unwrap();
            let request=vault::SnapshotRequestBinding::capture_at(settings.clone(),original.clone(),"none").unwrap();let held=snapshot_commit_lock().lock().await;let (started,ready)=tokio::sync::oneshot::channel();
            let task=tokio::spawn(async move {let _=started.send(());snapshot_database_after_wait(&request).await});ready.await.unwrap();tokio::task::yield_now().await;assert!(!task.is_finished());
            if kind=="malformed" {fs::write(&settings,br#"{"vault_path":"SECRET"#).unwrap();} else {let changed=match kind {"changed"=>json!({"vault_path":other}),"deleted"=>json!({}),_=>json!({"vault_path":7})};fs::write(&settings,serde_json::to_vec(&changed).unwrap()).unwrap();}drop(held);
            let result=task.await.unwrap();assert!(result.is_err(),"{kind}");assert_eq!(fs::read(&legacy).unwrap(),b"original database sentinel");assert!(!original.join(".starlane").exists());assert!(!other.exists());fs::remove_dir_all(dir).unwrap();
        }
    }
    #[tokio::test]
    async fn unrelated_settings_after_wait_allow_actual_first_default_zero_asset_commit_and_cas() {
        let dir=fixture();let settings=dir.join("settings.json");let root=dir.join("fresh");let request=vault::SnapshotRequestBinding::capture_at(settings.clone(),root.clone(),"none").unwrap();assert!(!settings.exists());assert!(!root.exists());
        let held=snapshot_commit_lock().lock().await;let (started,ready)=tokio::sync::oneshot::channel();let task=tokio::spawn(async move {let _=started.send(());let (guard,path)=snapshot_database_after_wait(&request).await?;Ok::<_,String>((guard,path,request))});ready.await.unwrap();tokio::task::yield_now().await;assert!(!task.is_finished());
        fs::write(&settings,br#"{"theme":"dark","token":"SECRET"}"#).unwrap();drop(held);let (guard,path,request)=task.await.unwrap().unwrap();let pool=open_database(&path).await.unwrap();migrate_database(&pool).await.unwrap();let snapshot=empty_snapshot();
        let first=commit_snapshot_and_mirror(&pool,"none",&snapshot,None,||request.write_mirror(&snapshot)).await.unwrap();assert!(first.canonical_committed);assert!(!first.mirror_pending);assert!(root.join("starlane-vault-none.json").exists());
        fs::write(&settings,br#"{"theme":"light","token":"CHANGED"}"#).unwrap();request.verify_selection().unwrap();let second=commit_snapshot_and_mirror(&pool,"none",&snapshot,Some(&first.canonical_revision),||request.write_mirror(&snapshot)).await.unwrap();assert!(second.canonical_committed);assert!(!second.mirror_pending);assert_ne!(first.canonical_revision,second.canonical_revision);
        pool.close().await;drop(guard);fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn actual_canonical_commit_with_late_selection_switch_reports_pending_without_foreign_mirror() {
        let dir=fixture();let settings=dir.join("settings.json");let root=dir.join("original");let other=dir.join("foreign");fs::write(&settings,serde_json::to_vec(&json!({"vault_path":root})).unwrap()).unwrap();let request=vault::SnapshotRequestBinding::capture_at(settings.clone(),other.clone(),"none").unwrap();let (guard,path)=snapshot_database_after_wait(&request).await.unwrap();let pool=open_database(&path).await.unwrap();migrate_database(&pool).await.unwrap();let old_mirror=root.join("starlane-vault-none.json");fs::write(&old_mirror,b"old derived mirror").unwrap();let snapshot=empty_snapshot();
        let receipt=commit_snapshot_and_mirror(&pool,"none",&snapshot,None,||{fs::write(&settings,serde_json::to_vec(&json!({"vault_path":other})).unwrap()).unwrap();request.write_mirror(&snapshot)}).await.unwrap();assert!(receipt.canonical_committed);assert!(receipt.mirror_pending);assert!(!other.exists());assert_eq!(fs::read(old_mirror).unwrap(),b"old derived mirror");
        let revision:(String,)=sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key='canonical_revision'").fetch_one(&pool).await.unwrap();assert_eq!(revision.0,receipt.canonical_revision);let pending:(String,)=sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key='mirror_pending'").fetch_one(&pool).await.unwrap();assert_eq!(pending.0,receipt.canonical_revision);pool.close().await;drop(guard);fs::remove_dir_all(dir).unwrap();
    }
    fn destination(request:&vault::SnapshotRequestBinding)->NativeSnapshotExpectedDestination {
        NativeSnapshotExpectedDestination {schema_version:1,provider:"none".into(),store_kind:"sqlite".into(),database_path:request.expected_database_path().unwrap().to_string_lossy().into_owned(),supports_reviewed_move_evidence:true}
    }
    #[tokio::test]
    async fn reviewed_move_original_descriptor_rejects_new_selection_before_any_directory_or_legacy_mutation() {
        let dir=fixture();let settings=dir.join("settings.json");let a=dir.join("a");let b=dir.join("b");
        fs::write(&settings,serde_json::to_vec(&json!({"vault_path":a})).unwrap()).unwrap();
        let original=vault::SnapshotRequestBinding::capture_at(settings.clone(),a.clone(),"none").unwrap();let expected=destination(&original);
        assert!(!a.exists());assert!(!b.exists());
        fs::write(&settings,serde_json::to_vec(&json!({"vault_path":b})).unwrap()).unwrap();
        let current=vault::SnapshotRequestBinding::capture_at(settings.clone(),a.clone(),"none").unwrap();
        assert!(verify_reviewed_move_destination(&current,"none",&expected).is_err());
        assert!(!a.exists());assert!(!b.exists());
        fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn reviewed_move_original_descriptor_survives_actual_mutex_wait_only_when_selection_unchanged() {
        let dir=fixture();let settings=dir.join("settings.json");let a=dir.join("a");let b=dir.join("b");
        fs::write(&settings,serde_json::to_vec(&json!({"vault_path":a})).unwrap()).unwrap();
        let request=vault::SnapshotRequestBinding::capture_at(settings.clone(),a.clone(),"none").unwrap();let expected=destination(&request);
        verify_reviewed_move_destination(&request,"none",&expected).unwrap();
        let held=snapshot_commit_lock().lock().await;let (started,ready)=tokio::sync::oneshot::channel();
        let task=tokio::spawn(async move {let _=started.send(());let (_guard,_path)=snapshot_database_after_wait(&request).await?;verify_reviewed_move_destination(&request,"none",&expected)});
        ready.await.unwrap();tokio::task::yield_now().await;assert!(!task.is_finished());
        fs::write(&settings,serde_json::to_vec(&json!({"vault_path":b})).unwrap()).unwrap();drop(held);
        assert!(task.await.unwrap().is_err());assert!(!a.exists());assert!(!b.exists());fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn reviewed_move_final_destination_proof_rolls_back_written_snapshot_and_keeps_original_revision() {
        let dir=fixture();let settings=dir.join("settings.json");let a=dir.join("a");let b=dir.join("b");
        fs::write(&settings,serde_json::to_vec(&json!({"vault_path":a})).unwrap()).unwrap();
        let request=vault::SnapshotRequestBinding::capture_at(settings.clone(),a.clone(),"none").unwrap();let expected=destination(&request);
        let (guard,path)=snapshot_database_after_wait(&request).await.unwrap();let pool=open_database(&path).await.unwrap();migrate_database(&pool).await.unwrap();
        let initial=empty_snapshot();let revision=write_snapshot_with_revision(&pool,"none",&initial,None,true).await.unwrap();
        let before_records:Vec<(String,String,i64,String)>=sqlx::query_as("SELECT collection,id,row_order,payload FROM local_vault_records ORDER BY collection,id").fetch_all(&pool).await.unwrap();
        let before_metadata:Vec<(String,String)>=sqlx::query_as("SELECT key,value FROM local_vault_metadata ORDER BY key").fetch_all(&pool).await.unwrap();
        let mut candidate=initial.clone();candidate["spaces"]=json!([{"id":"new-space","name":"candidate"}]);candidate["reviewed_move_records"]=json!([{"id":"move","schema_version":1,"provider":"none","future":{"raw":true}}]);
        let mut reached=false;
        let result=write_snapshot_with_revision_checked(&pool,"none",&candidate,Some(&revision),true,||{
            reached=true;fs::write(&settings,serde_json::to_vec(&json!({"vault_path":b})).unwrap()).unwrap();verify_reviewed_move_destination(&request,"none",&expected)
        }).await;
        assert!(reached);assert!(result.is_err());
        assert_eq!(sqlx::query_as::<_,(String,String,i64,String)>("SELECT collection,id,row_order,payload FROM local_vault_records ORDER BY collection,id").fetch_all(&pool).await.unwrap(),before_records);
        assert_eq!(sqlx::query_as::<_,(String,String)>("SELECT key,value FROM local_vault_metadata ORDER BY key").fetch_all(&pool).await.unwrap(),before_metadata);
        let new_space:(i64,)=sqlx::query_as("SELECT COUNT(*) FROM local_spaces WHERE id='new-space'").fetch_one(&pool).await.unwrap();assert_eq!(new_space.0,0);assert!(!b.exists());
        fs::write(&settings,serde_json::to_vec(&json!({"vault_path":a})).unwrap()).unwrap();
        let next=write_snapshot_with_revision_checked(&pool,"none",&candidate,Some(&revision),true,||verify_reviewed_move_destination(&request,"none",&expected)).await.unwrap();assert_ne!(next,revision);
        pool.close().await;let reopened=open_database(&path).await.unwrap();let raw:(String,)=sqlx::query_as("SELECT payload FROM local_vault_records WHERE collection='reviewed_move_records' AND id='move'").fetch_one(&reopened).await.unwrap();assert_eq!(serde_json::from_str::<Value>(&raw.0).unwrap(),candidate["reviewed_move_records"][0]);reopened.close().await;drop(guard);fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn reviewed_move_bound_read_rejects_foreign_current_selection_without_creating_paths() {
        let dir=fixture();let settings=dir.join("settings.json");let a=dir.join("a");let b=dir.join("b");
        fs::write(&settings,serde_json::to_vec(&json!({"vault_path":a})).unwrap()).unwrap();let original=vault::SnapshotRequestBinding::capture_at(settings.clone(),a.clone(),"none").unwrap();let expected=destination(&original);
        fs::write(&settings,serde_json::to_vec(&json!({"vault_path":b})).unwrap()).unwrap();let current=vault::SnapshotRequestBinding::capture_at(settings.clone(),a.clone(),"none").unwrap();
        assert!(read_bound_snapshot_after_wait(&current,"none",&expected).await.is_err());assert!(!a.exists());assert!(!b.exists());
        fs::write(&settings,serde_json::to_vec(&json!({"vault_path":a})).unwrap()).unwrap();let absent=read_bound_snapshot_after_wait(&original,"none",&expected).await.unwrap();assert!(absent.is_none());assert!(!a.exists());fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn reviewed_move_bound_read_actual_mutex_wait_rejects_retarget_before_filesystem() {
        let dir=fixture();let settings=dir.join("settings.json");let a=dir.join("a");let b=dir.join("b");fs::write(&settings,serde_json::to_vec(&json!({"vault_path":a})).unwrap()).unwrap();let request=vault::SnapshotRequestBinding::capture_at(settings.clone(),a.clone(),"none").unwrap();let expected=destination(&request);
        let held=snapshot_commit_lock().lock().await;let(started,ready)=tokio::sync::oneshot::channel();let task=tokio::spawn(async move {let _=started.send(());read_bound_snapshot_after_wait(&request,"none",&expected).await});ready.await.unwrap();tokio::task::yield_now().await;assert!(!task.is_finished());
        fs::write(&settings,serde_json::to_vec(&json!({"vault_path":b})).unwrap()).unwrap();drop(held);assert!(task.await.unwrap().is_err());assert!(!a.exists());assert!(!b.exists());fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test]
    async fn reviewed_move_bound_read_final_sql_proof_rejects_identical_foreign_candidate_and_revision() {
        let dir=fixture();let settings=dir.join("settings.json");let a=dir.join("a");let b=dir.join("b");fs::write(&settings,serde_json::to_vec(&json!({"vault_path":a})).unwrap()).unwrap();let request=vault::SnapshotRequestBinding::capture_at(settings.clone(),a.clone(),"none").unwrap();let expected=destination(&request);
        let path=request.database_path().unwrap();let pool=open_database(&path).await.unwrap();migrate_database(&pool).await.unwrap();let mut snapshot=empty_snapshot();snapshot["reviewed_move_records"]=json!([{"id":"same-move","schema_version":1,"future":"unchanged"}]);let revision=write_snapshot_with_revision(&pool,"none",&snapshot,None,true).await.unwrap();
        fs::create_dir_all(b.join(".starlane")).unwrap();let foreign=b.join(".starlane").join(path.file_name().unwrap());sqlx::query("VACUUM INTO ?1").bind(foreign.to_string_lossy().as_ref()).execute(&pool).await.unwrap();let other=open_database(&foreign).await.unwrap();let identical=read_snapshot_from_pool(&other,"none").await.unwrap();assert_eq!(identical.as_ref().unwrap()["canonical_revision"],revision);assert_eq!(identical,read_snapshot_from_pool(&pool,"none").await.unwrap());
        let before:Vec<(String,String)>=sqlx::query_as("SELECT key,value FROM local_vault_metadata ORDER BY key").fetch_all(&pool).await.unwrap();let mut reached=false;
        let result=read_snapshot_from_pool_checked(&pool,"none",||{reached=true;fs::write(&settings,serde_json::to_vec(&json!({"vault_path":b})).unwrap()).unwrap();verify_reviewed_move_destination(&request,"none",&expected)}).await;assert!(reached);assert!(result.is_err());assert_eq!(sqlx::query_as::<_,(String,String)>("SELECT key,value FROM local_vault_metadata ORDER BY key").fetch_all(&pool).await.unwrap(),before);
        fs::write(&settings,serde_json::to_vec(&json!({"vault_path":a})).unwrap()).unwrap();let accepted=read_snapshot_from_pool_checked(&pool,"none",||verify_reviewed_move_destination(&request,"none",&expected)).await.unwrap();assert_eq!(accepted,identical);pool.close().await;other.close().await;fs::remove_dir_all(dir).unwrap();
    }

}

/// Joint archive protocol. Its durable tables are deliberately outside the
/// snapshot bridge's replaceable metadata/records and remain after later edits.
#[cfg(all(unix, not(any(target_os = "ios", target_os = "android"))))]
mod joint_archive {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    #[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(deny_unknown_fields)]
    pub struct Binding {
        pub provider: String,
        pub database_path: String,
        pub database_instance_id: String,
    }
    #[derive(Clone)]
    struct Plan {
        request: vault::SnapshotRequestBinding,
        binding: Binding,
        envelope: Value,
        stage_digest: String,
        handle: String,
        destination: PathBuf,
        revision: Option<String>,
        destination_snapshot: Option<Value>,
    }
    static PLANS: OnceLock<Mutex<HashMap<String, Plan>>> = OnceLock::new();
    fn plans() -> &'static Mutex<HashMap<String, Plan>> { PLANS.get_or_init(|| Mutex::new(HashMap::new())) }
    fn digest(value: &Value) -> Result<String, String> {
        Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value).map_err(|e| e.to_string())?)))
    }
    async fn tables(pool: &Pool<Sqlite>) -> Result<(), String> {
        for statement in [
            "CREATE TABLE IF NOT EXISTS native_archive_instance (singleton INTEGER PRIMARY KEY CHECK(singleton=1), instance_id TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS native_archive_import_intents (operation_id TEXT PRIMARY KEY NOT NULL, provider TEXT NOT NULL, instance_id TEXT NOT NULL, request_fingerprint TEXT NOT NULL, request_json TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS native_archive_import_receipts (operation_id TEXT PRIMARY KEY NOT NULL, request_fingerprint TEXT NOT NULL, result_json TEXT NOT NULL, result_digest TEXT NOT NULL)",
            "CREATE TABLE IF NOT EXISTS native_archive_import_resolutions (operation_id TEXT PRIMARY KEY NOT NULL, request_fingerprint TEXT NOT NULL, resolution_json TEXT NOT NULL, resolution_digest TEXT NOT NULL)",
            "CREATE TRIGGER IF NOT EXISTS native_archive_resolutions_no_update BEFORE UPDATE ON native_archive_import_resolutions BEGIN SELECT RAISE(ABORT,'Immutable archive resolution'); END",
            "CREATE TRIGGER IF NOT EXISTS native_archive_resolutions_no_delete BEFORE DELETE ON native_archive_import_resolutions BEGIN SELECT RAISE(ABORT,'Immutable archive resolution'); END",
            "CREATE TRIGGER IF NOT EXISTS native_archive_intents_no_update BEFORE UPDATE ON native_archive_import_intents BEGIN SELECT RAISE(ABORT,'Immutable archive intent'); END",
            "CREATE TRIGGER IF NOT EXISTS native_archive_intents_no_delete BEFORE DELETE ON native_archive_import_intents BEGIN SELECT RAISE(ABORT,'Immutable archive intent'); END",
            "CREATE TRIGGER IF NOT EXISTS native_archive_receipts_no_update BEFORE UPDATE ON native_archive_import_receipts BEGIN SELECT RAISE(ABORT,'Immutable archive receipt'); END",
            "CREATE TRIGGER IF NOT EXISTS native_archive_receipts_no_delete BEFORE DELETE ON native_archive_import_receipts BEGIN SELECT RAISE(ABORT,'Immutable archive receipt'); END",
        ] { sqlx::query(statement).execute(pool).await.map_err(|e| e.to_string())?; }
        Ok(())
    }
    async fn instance(pool: &Pool<Sqlite>) -> Result<String, String> {
        let (id,): (String,) = sqlx::query_as("SELECT instance_id FROM native_archive_instance WHERE singleton=1")
            .fetch_one(pool).await.map_err(|_| "Original archive database identity is unavailable".to_string())?;
        if id.is_empty() { return Err("Original archive database identity is invalid".into()); }
        Ok(id)
    }
    async fn revision(pool: &Pool<Sqlite>) -> Result<Option<String>, String> {
        let row: Option<(String,)> = sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key='canonical_revision'")
            .fetch_optional(pool).await.map_err(|e| e.to_string())?;
        Ok(row.map(|r| r.0))
    }
    fn verify_binding(request: &vault::SnapshotRequestBinding, binding: &Binding, provider: &str) -> Result<PathBuf, String> {
        let path = request.expected_database_path()?;
        if binding.provider != provider || binding.database_path != path.to_string_lossy() || binding.database_instance_id.is_empty() {
            return Err("Archive recovery requires its original database and provider".into());
        }
        Ok(path)
    }
    async fn existing(request: &vault::SnapshotRequestBinding, binding: &Binding, provider: &str) -> Result<Pool<Sqlite>, String> {
        let path = verify_binding(request, binding, provider)?;
        let metadata = std::fs::metadata(&path).map_err(|_| "Original archive database is unavailable".to_string())?;
        if !metadata.is_file() { return Err("Original archive database is unavailable".into()); }
        // Discovery never creates a root, DB, schema, instance or migration.
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(
            SqliteConnectOptions::new().filename(&path).create_if_missing(false).synchronous(SqliteSynchronous::Full)
        ).await.map_err(|_| "Original archive database is unavailable".to_string())?;
        if instance(&pool).await? != binding.database_instance_id { return Err("Original archive database identity changed".into()); }
        verify_binding(request, binding, provider)?;
        Ok(pool)
    }
    pub async fn plan(app: &AppHandle, provider: &str, handle: &str, original: &str) -> Result<Value, String> {
        let request = vault::capture_snapshot_request(app, provider)?;
        plan_request(request,provider,handle,original).await
    }
    async fn plan_request(request:vault::SnapshotRequestBinding,provider:&str,handle:&str,original:&str)->Result<Value,String> {
        let destination = request.archive_assets_path()?;
        // Private extraction precedes canonical DB bootstrap; no asset install.
        let stage = vault::archive_admission::plan_joint(provider, handle, original, &destination)?;
        let (_guard, path) = snapshot_database_after_wait(&request).await?;
        let pool = open_database(&path).await?;
        migrate_database(&pool).await?;
        tables(&pool).await?;
        let fresh_id = digest(&json!({"path":path,"nonce":SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos().to_string()}))?;
        sqlx::query("INSERT OR IGNORE INTO native_archive_instance(singleton,instance_id) VALUES(1,?1)")
            .bind(fresh_id).execute(&pool).await.map_err(|e|e.to_string())?;
        let binding = Binding { provider: provider.into(), database_path: path.to_string_lossy().into_owned(), database_instance_id: instance(&pool).await? };
        let revision = revision(&pool).await?;
        let snapshot = match read_snapshot_from_pool(&pool, provider).await? {Some(snapshot)=>Some(snapshot),None=>request.legacy_snapshot()?};
        let mut envelope = stage.clone();
        envelope["binding"] = serde_json::to_value(&binding).map_err(|e|e.to_string())?;
        envelope["expected_revision"] = json!(revision);
        envelope["destination_snapshot"] = json!(snapshot);
        let plan_digest = digest(&envelope)?;
        envelope["plan_digest"] = json!(plan_digest);
        request.verify_selection()?;
        vault::archive_admission::verify_joint_plan(provider, handle, stage["plan_digest"].as_str().ok_or("Stage digest missing")?, &destination, false)?;
        let saved = Plan { request, binding, envelope: envelope.clone(), stage_digest: stage["plan_digest"].as_str().unwrap().into(), handle: handle.into(), destination, revision, destination_snapshot: snapshot };
        let mut map = plans().lock().map_err(|_| "Archive plan lock failed")?;
        map.retain(|handle,_|vault::archive_admission::joint_available(handle));
        if map.len() >= 8 { return Err("Finish pending archive plans before preparing another".into()); }
        map.insert(handle.into(), saved);
        Ok(envelope)
    }
    fn queue(snapshot: &Value) -> Result<Vec<Value>, String> {
        let Some(sync) = snapshot.get("sync_state") else { return Ok(Vec::new()); };
        if !sync.is_object() { return Err("Archive replay state is malformed".into()); }
        let Some(queue) = sync.get("queued_operations") else { return Ok(Vec::new()); };
        queue.as_array().cloned().ok_or_else(|| "Archive replay queue is malformed".into())
    }
    fn utf16_byte(text: &str, offset: usize) -> Result<usize, String> {
        let mut units = 0;
        for (byte, c) in text.char_indices() {
            if units == offset { return Ok(byte); }
            units += c.len_utf16();
            if units > offset { return Err("Archive reference splits a Unicode character".into()); }
        }
        if units == offset { Ok(text.len()) } else { Err("Archive reference offset is invalid".into()) }
    }
    fn candidate(plan: &Plan, submitted: &Value, changes: &[Value]) -> Result<(), String> {
        let original: Value = serde_json::from_str(plan.envelope["original_snapshot_json"].as_str().ok_or("Original archive JSON missing")?).map_err(|e|e.to_string())?;
        let mut expected = original.clone();
        let versions = plan.envelope["versions"].as_array().ok_or("Archive version plan missing")?;
        let mut aliases = HashMap::<String, String>::new();
        for version in versions {
            let index = version["index"].as_u64().ok_or("Archive asset index missing")? as usize;
            let old = original["assets"].get(index).ok_or("Archive asset index changed")?;
            if old["id"] != version["id"] || old["source"] != "tauri" { return Err("Archive asset identity changed".into()); }
            for field in ["url", "vault_path"] {
                if let Some(alias) = old[field].as_str().filter(|s| !s.is_empty()) {
                    let replacement = version["url"].as_str().ok_or("Archive asset URL missing")?.to_string();
                    if aliases.insert(alias.into(), replacement.clone()).is_some_and(|prior|prior!=replacement) { return Err("Archive asset aliases are ambiguous".into()); }
                }
                expected["assets"][index][field] = version[field].clone();
            }
            expected["assets"][index]["size"] = version["size"].clone();
        }
        // A skipped/browser asset may have a literal alias equal to a native
        // source. Remapping that spelling would silently change its reference.
        let owned: HashSet<usize> = versions.iter().filter_map(|v| v["index"].as_u64().map(|n| n as usize)).collect();
        if let Some(assets) = original["assets"].as_array() {
            for (index, asset) in assets.iter().enumerate() {
                if owned.contains(&index) { continue; }
                for field in ["url", "vault_path"] {
                    if asset[field].as_str().is_some_and(|alias| aliases.contains_key(alias)) {
                        return Err("Archive asset aliases are ambiguous".into());
                    }
                }
            }
        }
        let mut fields = HashMap::<(String, usize, String), Vec<&Value>>::new();
        let mut affected = HashSet::new();
        for change in changes {
            let collection = change["collection"].as_str().ok_or("Reference owner missing")?;
            let field = change["field"].as_str().ok_or("Reference field missing")?;
            if !["documents","drafts","templates"].contains(&collection) || !["content","cover_image"].contains(&field) || change["offset_unit"] != "utf16_code_units" { return Err("Archive reference owner is unsupported".into()); }
            let index = change["index"].as_u64().ok_or("Reference owner index missing")? as usize;
            let row = original[collection].get(index).ok_or("Reference owner changed")?;
            affected.insert(row["id"].as_str().ok_or("Reference owner identity missing")?.to_string());
            fields.entry((collection.into(),index,field.into())).or_default().push(change);
        }
        for ((collection,index,field), mut replacements) in fields {
            let text = original[&collection][index][&field].as_str().ok_or("Original reference text missing")?;
            replacements.sort_by_key(|c|c["start"].as_u64().unwrap_or(u64::MAX));
            let mut result = String::new(); let mut previous = 0;
            for change in replacements {
                let start = utf16_byte(text,change["start"].as_u64().ok_or("Reference start invalid")? as usize)?;
                let end = utf16_byte(text,change["end"].as_u64().ok_or("Reference end invalid")? as usize)?;
                let old = change["original"].as_str().ok_or("Original reference missing")?;
                let new = change["replacement"].as_str().ok_or("Replacement reference missing")?;
                if start < previous || start >= end || text.get(start..end) != Some(old) || aliases.get(old).map(String::as_str) != Some(new) { return Err("Archive reference span changed or is ambiguous".into()); }
                result.push_str(&text[previous..start]);result.push_str(new);previous=end;
            }
            result.push_str(&text[previous..]);expected[&collection][index][&field]=json!(result);
        }
        let mut merged = Vec::new(); let mut by_id = HashMap::<String,Value>::new();
        for raw in plan.destination_snapshot.as_ref().map(queue).transpose()?.unwrap_or_default().into_iter().chain(queue(&original)?) {
            let id = raw["id"].as_str().filter(|s|!s.is_empty()).ok_or("Replay identity missing")?.to_string();
            if affected.contains(raw["entity_id"].as_str().unwrap_or("")) && (matches!(raw["action"].as_str(),Some("update_content"|"resolve_conflict")) || raw["payload"].get("content").is_some() || raw["payload"].get("body").is_some() || raw["payload"].get("cover_image").is_some()) { return Err("Queued body or source depends on remapped archive references; retain and resolve the original queue first".into()); }
            if let Some(prior)=by_id.get(&id) { if prior!=&raw {return Err("Destination and archive replay operations conflict; retain both original queues".into());} continue; }
            by_id.insert(id,raw.clone());merged.push(raw);
        }
        if !merged.is_empty() || original.get("sync_state").is_some() {
            if expected.get("sync_state").is_none() { expected["sync_state"]=json!({}); }
            expected["sync_state"]["queued_operations"]=json!(merged);
        }
        if &expected != submitted { return Err("Archive candidate differs from the exact original metadata, references or queue plan".into()); }
        Ok(())
    }
    async fn require_no_other_unresolved(tx: &mut sqlx::Transaction<'_, Sqlite>, binding: &Binding, operation: &str) -> Result<(), String> {
        let pending: Option<(String,)> = sqlx::query_as(
            "SELECT i.operation_id FROM native_archive_import_intents i
             LEFT JOIN native_archive_import_receipts r ON r.operation_id=i.operation_id AND r.request_fingerprint=i.request_fingerprint
             LEFT JOIN native_archive_import_resolutions a ON a.operation_id=i.operation_id AND a.request_fingerprint=i.request_fingerprint
             WHERE i.instance_id=?1 AND i.provider=?2 AND i.operation_id<>?3
               AND r.operation_id IS NULL AND a.operation_id IS NULL LIMIT 1"
        ).bind(&binding.database_instance_id).bind(&binding.provider).bind(operation)
            .fetch_optional(&mut **tx).await.map_err(|e| e.to_string())?;
        if pending.is_some() { return Err("A prior archive import needs original database inspection and explicit retained abandonment".into()); }
        Ok(())
    }
    async fn verify_result_rows(tx: &mut sqlx::Transaction<'_,Sqlite>, snapshot: &Value, revision: &str, binding: &Binding) -> Result<(),String> {
        let (instance,): (String,) = sqlx::query_as("SELECT instance_id FROM native_archive_instance WHERE singleton=1")
            .fetch_one(&mut **tx).await.map_err(|e| e.to_string())?;
        if instance != binding.database_instance_id { return Err("Original archive database instance changed before commit".into()); }
        for (key, field) in [("sync_state", "sync_state"), ("maintenance_target_catalogs", "maintenance_target_catalogs")] {
            let raw: Option<(String,)> = sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key=?1")
                .bind(key).fetch_optional(&mut **tx).await.map_err(|e| e.to_string())?;
            let actual = raw.map(|row| serde_json::from_str::<Value>(&row.0).map_err(|e| e.to_string())).transpose()?;
            if actual.as_ref() != snapshot.get(field) { return Err("Archive supported raw state metadata changed before commit".into()); }
        }
        let cursors: Vec<(String, String, String, String)> = sqlx::query_as("SELECT provider,sync_state,updated_at,payload FROM local_sync_cursors")
            .fetch_all(&mut **tx).await.map_err(|e| e.to_string())?;
        match snapshot.get("sync_state") {
            None if cursors.is_empty() => {},
            Some(raw) if cursors.len() == 1 => {
                let (provider, state, updated, payload) = &cursors[0];
                let expected_state = value_string(raw,"state").unwrap_or_else(|| "idle".into());
                let expected_updated = value_string(raw,"updated_at").unwrap_or_else(|| "1970-01-01T00:00:00Z".into());
                if provider != &binding.provider || state != &expected_state || updated != &expected_updated || serde_json::from_str::<Value>(payload).map_err(|e| e.to_string())? != *raw {
                    return Err("Archive sync cursor or preserved state extras changed before commit".into());
                }
            },
            _ => return Err("Archive sync cursor set changed before commit".into()),
        }

        // The supported sync-state reader also overlays normalized recovery
        // checkpoints. Bind that overlay, including raw future fields, rather
        // than only proving its cursor's original JSON.
        let mut checkpoints = HashMap::<String, Value>::new();
        let mut checkpoint_index = 0usize;
        for raw in replay_checkpoint_values(snapshot) {
            let id = value_string_any(raw, &["id"]).or_else(|| value_string_any(raw, &["spaceId", "space_id"]))
                .unwrap_or_else(|| { checkpoint_index += 1; format!("checkpoint-{checkpoint_index}") });
            checkpoints.insert(id, raw.clone()); // Same explicit last-ID-wins SQL projection.
        }
        let rows = sqlx::query("SELECT id,provider,space_id,source_server_url,downloaded_by_user_id,downloaded_at,payload FROM local_replay_checkpoints")
            .fetch_all(&mut **tx).await.map_err(|e| e.to_string())?;
        if rows.len() != checkpoints.len() { return Err("Archive recovery checkpoint set changed before commit".into()); }
        for row in rows {
            let id: String = row.get("id");
            let raw = checkpoints.remove(&id).ok_or("Archive recovery checkpoint identity changed")?;
            if row.get::<String,_>("provider") != binding.provider || serde_json::from_str::<Value>(&row.get::<String,_>("payload")).map_err(|e| e.to_string())? != raw {
                return Err("Archive recovery checkpoint raw evidence changed".into());
            }
            for (column, fields) in [
                ("space_id", &["spaceId", "space_id"][..]),
                ("source_server_url", &["sourceServerUrl", "source_server_url"][..]),
                ("downloaded_by_user_id", &["downloadedByUserId", "downloaded_by_user_id"][..]),
                ("downloaded_at", &["downloadedAt", "downloaded_at", "created_at"][..]),
            ] {
                if row.get::<Option<String>,_>(column) != value_string_any(&raw, fields) { return Err("Archive recovery checkpoint projection changed".into()); }
            }
        }
        for (key, expected) in [
            ("schema_version", LOCAL_DATABASE_SCHEMA_VERSION.to_string()),
            ("replay_queue_order_format", "1:array".to_string()),
            ("mirror_pending", revision.to_string()),
        ] {
            let actual: Option<(String,)> = sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key=?1")
                .bind(key).fetch_optional(&mut **tx).await.map_err(|e| e.to_string())?;
            if actual.map(|row| row.0).as_deref() != Some(expected.as_str()) { return Err("Archive generated result metadata changed before commit".into()); }
        }
        for collection in COLLECTIONS {
            let expected=snapshot.get(*collection).and_then(Value::as_array).cloned().unwrap_or_default();
            let raw:Vec<(String,)>=sqlx::query_as("SELECT payload FROM local_vault_records WHERE collection=?1 ORDER BY row_order").bind(collection).fetch_all(&mut **tx).await.map_err(|e|e.to_string())?;
            let actual=raw.into_iter().map(|row|serde_json::from_str::<Value>(&row.0).map_err(|e|e.to_string())).collect::<Result<Vec<_>,_>>()?;
            if actual!=expected {return Err("Archive snapshot result rows changed before commit".into());}
        }
        for (collection,table) in [("spaces","local_spaces"),("documents","local_documents"),("folders","local_folders"),("assets","local_assets"),("templates","local_templates"),("workflow_templates","local_workflow_templates"),("document_workflows","local_document_workflows"),("document_workflow_events","local_document_workflow_events"),("property_schemas","local_properties"),("property_values","local_property_values"),("tags","local_tags"),("doc_tags","local_doc_tags"),("search_index","local_search_index")] {
            let expected=snapshot.get(collection).and_then(Value::as_array).cloned().unwrap_or_default();
            let raw:Vec<(String,)>=sqlx::query_as(&format!("SELECT payload FROM {table}")).fetch_all(&mut **tx).await.map_err(|e|e.to_string())?;
            let mut actual=raw.into_iter().map(|row|serde_json::from_str::<Value>(&row.0).map_err(|e|e.to_string())).collect::<Result<Vec<_>,_>>()?;
            for item in expected {let index=actual.iter().position(|row|row==&item).ok_or("Archive normalized result changed")?;actual.remove(index);}
            if !actual.is_empty() {return Err("Archive normalized result gained unrelated rows".into());}
        }
        let expected=queue(snapshot)?;
        let raw:Vec<(String,)>=sqlx::query_as("SELECT payload FROM local_vault_records WHERE collection='queued_operations' ORDER BY row_order").fetch_all(&mut **tx).await.map_err(|e|e.to_string())?;
        let actual=raw.into_iter().map(|row|serde_json::from_str::<Value>(&row.0).map_err(|e|e.to_string())).collect::<Result<Vec<_>,_>>()?;
        if actual!=expected {return Err("Archive raw queue result changed".into());}
        let rows=sqlx::query("SELECT * FROM local_queued_operations").fetch_all(&mut **tx).await.map_err(|e|e.to_string())?;
        if rows.len()!=expected.len() {return Err("Archive normalized queue result changed".into());}
        for raw in expected {
            let row=rows.iter().find(|row|row.get::<String,_>("id")==raw["id"].as_str().unwrap_or("")).ok_or("Archive queue identity changed")?;
            require_replay_queue_witness(&raw,row)?;
            if row.get::<i64,_>("attempts")!=raw["attempts"].as_i64().unwrap_or(0) || row.get::<Option<String>,_>("last_error").as_deref()!=raw["last_error"].as_str() {return Err("Archive queue bookkeeping changed before commit".into());}
        }
        let (current,):(String,)=sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key='canonical_revision'").fetch_one(&mut **tx).await.map_err(|e|e.to_string())?;
        if current!=revision {return Err("Archive result revision changed".into());}
        Ok(())
    }
    pub async fn commit(provider: &str, handle: &str, plan_digest: &str, snapshot: Value, changes: Vec<Value>) -> Result<Value,String> {
        let plan = plans().lock().map_err(|_|"Archive plan lock failed")?.get(handle).cloned().ok_or("Original archive plan unavailable; inspect its retained database intent")?;
        if plan.binding.provider!=provider || plan.envelope["plan_digest"]!=plan_digest { return Err("Original archive plan changed".into()); }
        candidate(&plan,&snapshot,&changes)?;
        #[cfg(not(target_os="linux"))]
        if !plan.envelope["imported_provenance"].is_null(){return Err("Receipt-provenance import requires Linux destination owner".into());}
        let request = json!({"plan":plan.envelope,"candidate":snapshot,"reference_changes":changes});
        let fingerprint=digest(&request)?;
        let operation=plan.envelope["operation_id"].as_str().ok_or("Archive operation missing")?.to_string();
        let _guard=snapshot_commit_lock().lock().await;
        let pool=existing(&plan.request,&plan.binding,provider).await?;
        if let Some(result)=historical(&pool,&plan.binding,&operation,&fingerprint).await? {
            let mut result=result;
            result["current_snapshot"]=json!(read_snapshot_from_pool(&pool,provider).await?);
            plan.request.verify_selection()?;return Ok(result);
        }
        if revision(&pool).await?!=plan.revision {return Err("Original destination snapshot changed; retain the original archive plan".into());}
        let current=match read_snapshot_from_pool(&pool,provider).await? {Some(current)=>Some(current),None=>plan.request.legacy_snapshot()?};
        if current!=plan.destination_snapshot {return Err("Original destination rows or replay queue changed; retain the original archive plan".into());}
        // Fully exercise the ACTUAL snapshot SQL validator before persisting an
        // intent or installing files. Every statement is rolled back explicitly.
        let mut preflight=pool.begin().await.map_err(|e|e.to_string())?;
        let checked=match write_snapshot_in_tx(&mut preflight,provider,&snapshot).await {
            Ok(revision)=>verify_result_rows(&mut preflight,&snapshot,&revision,&plan.binding).await,
            Err(error)=>Err(error),
        };
        preflight.rollback().await.map_err(|e|e.to_string())?;
        checked?;
        vault::archive_admission::reserve_joint(provider,handle,&plan.stage_digest,&plan.destination)?;
        let outcome=async {
            vault::archive_admission::verify_joint_plan(provider,handle,&plan.stage_digest,&plan.destination,false)?;
            // Admit and persist the intent under the actual SQLite writer lock,
            // including other processes which already passed JS discovery.
            let mut admission = pool.begin().await.map_err(|e| e.to_string())?;
            writer(&mut admission, &plan.binding).await?;
            require_no_other_unresolved(&mut admission, &plan.binding, &operation).await?;
            sqlx::query("INSERT INTO native_archive_import_intents(operation_id,provider,instance_id,request_fingerprint,request_json) VALUES(?1,?2,?3,?4,?5)")
                .bind(&operation).bind(provider).bind(&plan.binding.database_instance_id).bind(&fingerprint).bind(serde_json::to_string(&request).map_err(|e|e.to_string())?)
                .execute(&mut *admission).await.map_err(|_|"Original archive intent could not be recorded; inspect its original operation".to_string())?;
            plan.request.verify_selection()?;
            admission.commit().await.map_err(|_|"Archive intent acknowledgement unknown; inspect its original database".to_string())?;
            #[cfg(test)]
            AFTER_INTENT.with(|hook| -> Result<(),String> {if let Some(callback)=hook.borrow_mut().take(){callback()?;}Ok(())})?;
            plan.request.verify_selection()?;
            let mut tx=pool.begin().await.map_err(|e|e.to_string())?;
            writer(&mut tx,&plan.binding).await?;
            require_no_other_unresolved(&mut tx, &plan.binding, &operation).await?;
            let abandoned:Option<(String,)>=sqlx::query_as("SELECT request_fingerprint FROM native_archive_import_resolutions WHERE operation_id=?1").bind(&operation).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?;
            if abandoned.is_some() {return Err("Original archive import was retained and abandoned; it cannot be dispatched again".into());}
            let current:Option<(String,)>=sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key='canonical_revision'").fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?;
            if current.map(|r|r.0)!=plan.revision {return Err("Destination changed after archive intent; unique versions and original intent retained".into());}
            let revision=write_snapshot_in_tx(&mut tx,provider,&snapshot).await?;
            #[cfg(target_os="linux")]
            let mut imported_owner=if !plan.envelope["imported_provenance"].is_null(){Some(vault::archive_admission::prepare_joint_owner(provider,handle,&plan.stage_digest,&plan.destination,plan.request.clone(),serde_json::to_value(&plan.binding).map_err(|e|e.to_string())?,&fingerprint,&revision,&snapshot)?)}else{None};
            #[cfg(target_os="linux")]
            let native_installation=imported_owner.as_mut().map(|owner|owner.seal()).transpose()?;
            #[cfg(not(target_os="linux"))]
            let native_installation:Option<Value>=None;
            let result=json!({"native_installation":native_installation,"schema_version":1,"canonical_committed":true,"operation_id":operation,"request_fingerprint":fingerprint,"binding":plan.binding,"canonical_revision":revision,"candidate":snapshot,"reference_changes":changes,"versions":plan.envelope["versions"],"imported_provenance":plan.envelope["imported_provenance"]});
            sqlx::query("INSERT INTO native_archive_import_receipts(operation_id,request_fingerprint,result_json,result_digest) VALUES(?1,?2,?3,?4)")
                .bind(&operation).bind(&fingerprint).bind(serde_json::to_string(&result).map_err(|e|e.to_string())?).bind(digest(&result)?).execute(&mut *tx).await.map_err(|e|e.to_string())?;
            verify_result_rows(&mut tx,&snapshot,&revision,&plan.binding).await?;
            let (stored,stored_digest):(String,String)=sqlx::query_as("SELECT result_json,result_digest FROM native_archive_import_receipts WHERE operation_id=?1 AND request_fingerprint=?2").bind(&operation).bind(&fingerprint).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?;
            if serde_json::from_str::<Value>(&stored).map_err(|e|e.to_string())?!=result || stored_digest!=digest(&result)? {return Err("Archive receipt changed before commit".into());}
            // Install while holding the ORIGINAL SQLite writer transaction.
            // Explicit abandonment cannot overtake this canonical commit.
            #[cfg(test)]
            AFTER_SQL.with(|hook| -> Result<(), String> { if let Some(callback) = hook.borrow_mut().take() { callback()?; } Ok(()) })?;
            plan.request.verify_selection()?;
            #[cfg(target_os="linux")]
            vault::archive_admission::install_joint_with(provider,handle,&plan.stage_digest,&plan.destination,|index|{if let Some(owner)=imported_owner.as_mut(){owner.publish(index)?;Ok(true)}else{Ok(false)}})?;
            #[cfg(not(target_os="linux"))]
            vault::archive_admission::install_joint(provider,handle,&plan.stage_digest,&plan.destination)?;
            #[cfg(target_os="linux")]
            if let Some(owner)=imported_owner.as_ref(){owner.verify_installed()?;}

            // ALL SQL awaits precede original settings/path and synchronous
            // creator/version byte proof. Never prove against another DB/root.
            plan.request.verify_selection()?;
            vault::archive_admission::verify_joint_plan(provider,handle,&plan.stage_digest,&plan.destination,true)?;
            tx.commit().await.map_err(|_|"Archive commit acknowledgement unknown; inspect the original operation in its original database".to_string())?;
            #[cfg(test)]
            if LOSE_COMMIT_REPLY.with(|flag|flag.replace(false)){return Err("Test-only lost acknowledgement after real archive commit".into());}
            let mirror=finish_snapshot_mirror(&pool,&revision,||plan.request.write_mirror(&snapshot)).await;
            let mut response=result;response["mirror_pending"]=json!(mirror.mirror_pending);
            response["current_snapshot"]=json!(read_snapshot_from_pool(&pool,provider).await?);
            plan.request.verify_selection()?;
            Ok(response)
        }.await;
        vault::archive_admission::release_joint(handle);
        // Request identity remains recoverable in its immutable SQL intent even
        // when process memory/staging descriptors vanish after an unknown commit.
        plans().lock().map_err(|_|"Archive plan lock failed")?.remove(handle);
        outcome
    }
    async fn historical(pool:&Pool<Sqlite>,binding:&Binding,operation:&str,fingerprint:&str)->Result<Option<Value>,String> {
        let row:Option<(String,String,String,String,String)>=sqlx::query_as("SELECT i.provider,i.instance_id,i.request_json,r.result_json,r.result_digest FROM native_archive_import_intents i JOIN native_archive_import_receipts r ON r.operation_id=i.operation_id AND r.request_fingerprint=i.request_fingerprint WHERE i.operation_id=?1 AND i.request_fingerprint=?2")
            .bind(operation).bind(fingerprint).fetch_optional(pool).await.map_err(|_|"Original archive history unavailable".to_string())?;
        let Some((provider,instance,request,result,sealed))=row else{return Ok(None);};
        let request:Value=serde_json::from_str(&request).map_err(|_|"Archive intent malformed")?;
        let result:Value=serde_json::from_str(&result).map_err(|_|"Archive receipt malformed")?;
        if request["plan"]["binding"]!=serde_json::to_value(binding).map_err(|e|e.to_string())? || request["plan"]["operation_id"]!=operation || result["schema_version"]!=1 || result["canonical_committed"]!=true || provider!=binding.provider || instance!=binding.database_instance_id || digest(&request)?!=fingerprint || digest(&result)?!=sealed || result["request_fingerprint"]!=fingerprint || result["operation_id"]!=operation || result["binding"]!=serde_json::to_value(binding).map_err(|e|e.to_string())? || result["candidate"]!=request["candidate"] || result["reference_changes"]!=request["reference_changes"] || result["versions"]!=request["plan"]["versions"] || result["imported_provenance"]!=request["plan"]["imported_provenance"] {return Err("Archive immutable request/result evidence changed".into());}
        Ok(Some(result))
    }
    pub async fn inspect(app:&AppHandle,provider:&str,binding:Binding,operation:&str,fingerprint:&str)->Result<Value,String> {
        let request=vault::capture_snapshot_request(app,provider)?;
        let _guard=snapshot_commit_lock().lock().await;
        let pool=existing(&request,&binding,provider).await?;
        let result=historical(&pool,&binding,operation,fingerprint).await?;
        let intent:Option<(String,)>=sqlx::query_as("SELECT request_json FROM native_archive_import_intents WHERE operation_id=?1 AND request_fingerprint=?2 AND instance_id=?3 AND provider=?4").bind(operation).bind(fingerprint).bind(&binding.database_instance_id).bind(provider).fetch_optional(&pool).await.map_err(|e|e.to_string())?;
        let intent=intent.map(|r|serde_json::from_str::<Value>(&r.0).map_err(|_|"Archive intent malformed".to_string())).transpose()?;
        if intent.as_ref().is_some_and(|value|digest(value).ok().as_deref()!=Some(fingerprint)) {return Err("Archive intent evidence changed".into());}
        let current=read_snapshot_from_pool(&pool,provider).await?;
        verify_binding(&request,&binding,provider)?;
        let retained_resolution=resolution(&pool,&binding,operation,fingerprint).await?;
        verify_binding(&request,&binding,provider)?;
        Ok(json!({"binding":binding,"operation_id":operation,"request_fingerprint":fingerprint,"intent":intent,"receipt":result,"resolution":retained_resolution,"current_snapshot":current}))
    }
    async fn writer(tx:&mut sqlx::Transaction<'_,Sqlite>,binding:&Binding)->Result<(),String> {
        // SQLite's actual write lock, shared by both commit and resolution; a
        // process-only mutex is not claimed to exclude another app process.
        let changed=sqlx::query("UPDATE native_archive_instance SET instance_id=instance_id WHERE singleton=1 AND instance_id=?1").bind(&binding.database_instance_id).execute(&mut **tx).await.map_err(|e|e.to_string())?.rows_affected();
        if changed!=1 {return Err("Original archive database identity changed".into());}Ok(())
    }
    async fn resolution(pool:&Pool<Sqlite>,binding:&Binding,operation:&str,fingerprint:&str)->Result<Option<Value>,String> {
        let row:Option<(String,String)>=sqlx::query_as("SELECT resolution_json,resolution_digest FROM native_archive_import_resolutions WHERE operation_id=?1 AND request_fingerprint=?2").bind(operation).bind(fingerprint).fetch_optional(pool).await.map_err(|e|e.to_string())?;
        let Some((raw,sealed))=row else{return Ok(None);};let value:Value=serde_json::from_str(&raw).map_err(|_|"Archive resolution malformed")?;
        if digest(&value)?!=sealed || value["binding"]!=serde_json::to_value(binding).map_err(|e|e.to_string())? || value["operation_id"]!=operation || value["request_fingerprint"]!=fingerprint || value["retained_abandoned"]!=true {return Err("Archive resolution evidence changed".into());}Ok(Some(value))
    }
    pub async fn abandon(app:&AppHandle,provider:&str,binding:Binding,operation:&str,fingerprint:&str)->Result<Value,String> {
        let request=vault::capture_snapshot_request(app,provider)?;
        abandon_request(request,provider,binding,operation,fingerprint).await
    }
    async fn abandon_request(request:vault::SnapshotRequestBinding,provider:&str,binding:Binding,operation:&str,fingerprint:&str)->Result<Value,String> {
        let _guard=snapshot_commit_lock().lock().await;
        let pool=existing(&request,&binding,provider).await?;
        if let Some(resolution)=resolution(&pool,&binding,operation,fingerprint).await? {request.verify_selection()?;return Ok(resolution);}
        let mut tx=pool.begin().await.map_err(|e|e.to_string())?;writer(&mut tx,&binding).await?;
        let intent:Option<(String,)>=sqlx::query_as("SELECT request_json FROM native_archive_import_intents WHERE operation_id=?1 AND request_fingerprint=?2 AND instance_id=?3 AND provider=?4").bind(operation).bind(fingerprint).bind(&binding.database_instance_id).bind(provider).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?;
        let raw=intent.ok_or("Original pending archive intent is unavailable")?.0;let intent:Value=serde_json::from_str(&raw).map_err(|_|"Archive intent malformed")?;
        if digest(&intent)?!=fingerprint {return Err("Original pending archive intent changed".into());}
        let committed:Option<(String,)>=sqlx::query_as("SELECT operation_id FROM native_archive_import_receipts WHERE operation_id=?1").bind(operation).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?;
        if committed.is_some(){return Err("This archive import already committed; inspect its original receipt".into());}
        let result=json!({"schema_version":1,"binding":binding,"operation_id":operation,"request_fingerprint":fingerprint,"retained_abandoned":true,"canonical_committed":false,"evidence_retained":true});
        sqlx::query("INSERT INTO native_archive_import_resolutions(operation_id,request_fingerprint,resolution_json,resolution_digest) VALUES(?1,?2,?3,?4)").bind(operation).bind(fingerprint).bind(result.to_string()).bind(digest(&result)?).execute(&mut *tx).await.map_err(|e|e.to_string())?;
        verify_binding(&request,&binding,provider)?;tx.commit().await.map_err(|_|"Archive resolution acknowledgement unknown; inspect the original operation")?;Ok(result)
    }
    pub async fn discover(app: &AppHandle, provider: &str, cursor: Option<String>, limit: u32) -> Result<Value, String> {
        let request = vault::capture_snapshot_request(app, provider)?;
        let guard = snapshot_commit_lock().lock().await;
        let path = request.expected_database_path()?;
        if !path.exists() { request.verify_selection()?; return Ok(json!({"binding":null,"operations":[],"next_cursor":null})); }
        let pool = SqlitePoolOptions::new().max_connections(1).connect_with(SqliteConnectOptions::new().filename(&path).create_if_missing(false)).await.map_err(|_|"Existing archive database is unavailable")?;
        let present: Option<(String,)> = sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' AND name='native_archive_instance'").fetch_optional(&pool).await.map_err(|e|e.to_string())?;
        if present.is_none() { request.verify_selection()?;return Ok(json!({"binding":null,"operations":[],"next_cursor":null})); }
        let binding = Binding { provider:provider.into(),database_path:path.to_string_lossy().into_owned(),database_instance_id:instance(&pool).await? };
        request.verify_selection()?;pool.close().await;drop(guard);
        // This is read-only enrollment for inventory, NEVER an acknowledgement
        // of an earlier unknown operation whose original binding is different.
        list(app,provider,binding,cursor,limit).await
    }
    pub async fn list(app:&AppHandle,provider:&str,binding:Binding,cursor:Option<String>,limit:u32)->Result<Value,String> {
        if !(1..=20).contains(&limit) {return Err("Archive inventory limit must be between 1 and 20".into());}
        let request=vault::capture_snapshot_request(app,provider)?;let _guard=snapshot_commit_lock().lock().await;
        let pool=existing(&request,&binding,provider).await?;
        if let Some(prior)=cursor.as_ref() {
            let row:Option<(String,String)>=sqlx::query_as("SELECT request_json,request_fingerprint FROM native_archive_import_intents WHERE operation_id=?1 AND provider=?2 AND instance_id=?3").bind(prior).bind(provider).bind(&binding.database_instance_id).fetch_optional(&pool).await.map_err(|e|e.to_string())?;
            let (raw,fingerprint)=row.ok_or("Archive inventory cursor is unavailable")?;
            let raw:Value=serde_json::from_str(&raw).map_err(|_|"Archive inventory cursor is malformed")?;
            if digest(&raw)?!=fingerprint {return Err("Archive inventory cursor changed".into());}
        }
        let rows:Vec<(String,String,String,Option<String>)>=sqlx::query_as("SELECT i.operation_id,i.request_fingerprint,i.request_json,r.operation_id FROM native_archive_import_intents i LEFT JOIN native_archive_import_receipts r ON r.operation_id=i.operation_id WHERE i.provider=?1 AND i.instance_id=?2 AND i.operation_id>?3 ORDER BY i.operation_id LIMIT ?4")
            .bind(provider).bind(&binding.database_instance_id).bind(cursor.unwrap_or_default()).bind(i64::from(limit)+1).fetch_all(&pool).await.map_err(|e|e.to_string())?;
        let mut result=Vec::new();for (operation,fingerprint,raw,committed) in rows.iter().take(limit as usize) {
            let intent:Value=serde_json::from_str(raw).map_err(|_|"Archive intent malformed")?;
            if digest(&intent)?!=*fingerprint {return Err("Archive inventory evidence changed".into());}
            if committed.is_some() {historical(&pool,&binding,operation,fingerprint).await?.ok_or("Archive receipt missing")?;}
            let retained=resolution(&pool,&binding,operation,fingerprint).await?;
            result.push(json!({"operation_id":operation,"request_fingerprint":fingerprint,"canonical_committed":committed.is_some(),"retained_abandoned":retained.is_some(),"intent":intent}));
        }
        verify_binding(&request,&binding,provider)?;
        let next=if rows.len()>limit as usize {result.last().and_then(|v|v["operation_id"].as_str()).map(str::to_string)}else{None};
        Ok(json!({"binding":binding,"operations":result,"next_cursor":next}))
    }
    pub async fn reset_allowed(path:&PathBuf)->Result<(),String> {
        // Retained partials and immutable download receipts must survive reset.
        // A pending operation may not yet have committed any SQL receipt.
        if let Some(parent) = path.parent() {
            let retained = parent.join("attachment-downloads");
            match std::fs::symlink_metadata(&retained) {
                Ok(metadata) => {
                    if !metadata.is_dir() || metadata.file_type().is_symlink() {
                        return Err("Native attachment retention directory requires inspection before reset".into());
                    }
                    if let Some(entry) = std::fs::read_dir(&retained).map_err(|e| e.to_string())?.next() {
                        entry.map_err(|e| e.to_string())?;
                        return Err("Native attachment history or pending download must be retained before reset".into());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        if !path.exists(){return Ok(());}
        let pool=SqlitePoolOptions::new().max_connections(1).connect_with(SqliteConnectOptions::new().filename(path).create_if_missing(false)).await.map_err(|e|e.to_string())?;
        let present:Option<(String,)>=sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' AND name='native_archive_import_intents'").fetch_optional(&pool).await.map_err(|e|e.to_string())?;
        if present.is_some(){let (count,):(i64,)=sqlx::query_as("SELECT COUNT(*) FROM native_archive_import_intents").fetch_one(&pool).await.map_err(|e|e.to_string())?;if count>0{return Err("Archive import history or pending intent belongs to this database; retain it and inspect before reset".into());}}
        let download_table: Option<(String,)> = sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' AND name='native_attachment_download_receipts'").fetch_optional(&pool).await.map_err(|e| e.to_string())?;
        if download_table.is_some() {
            let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM native_attachment_download_receipts").fetch_one(&pool).await.map_err(|e| e.to_string())?;
            if count > 0 {
                pool.close().await;
                return Err("Immutable native attachment receipts must be retained before reset".into());
            }
        }
        pool.close().await;Ok(())
    }
    #[cfg(test)]
    thread_local! {
        static AFTER_INTENT: std::cell::RefCell<Option<Box<dyn FnOnce()->Result<(),String>>>>=std::cell::RefCell::new(None);
        static LOSE_COMMIT_REPLY: std::cell::Cell<bool>=std::cell::Cell::new(false);
        static AFTER_SQL: std::cell::RefCell<Option<Box<dyn FnOnce()->Result<(),String>>>>=std::cell::RefCell::new(None);
    }
    #[cfg(test)]
    pub(crate) mod tests {
        use super::*;
        use std::fs;
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        fn root() -> PathBuf {
            let path=std::env::temp_dir().join(format!("starlane-joint-import-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
            fs::create_dir_all(&path).unwrap();path
        }
        fn empty() -> Value {json!({"schema_version":1,"spaces":[],"folders":[],"documents":[],"drafts":[],"assets":[],"templates":[],"unknown_root":{"raw":"retain"}})}
        fn zip(path:&std::path::Path,raw:&Value,bytes:Option<&[u8]>) {
            let mut archive=zip::ZipWriter::new(fs::File::create(path).unwrap());let options=SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            archive.start_file("vault.json",options).unwrap();archive.write_all(raw.to_string().as_bytes()).unwrap();
            if let Some(bytes)=bytes {archive.start_file("assets/media-image.png",options).unwrap();archive.write_all(bytes).unwrap();}
            archive.finish().unwrap().sync_all().unwrap();
        }
        async fn prepared(raw:Value,bytes:Option<&[u8]>)->(PathBuf,Plan,Value) {
            let root=root();let vault=root.join("vault");fs::create_dir(&vault).unwrap();let settings=root.join("settings.json");fs::write(&settings,json!({"vault_path":vault}).to_string()).unwrap();let source=root.join("source.zip");zip(&source,&raw,bytes);
            let request=vault::SnapshotRequestBinding::capture_at(settings,vault.clone(),"none").unwrap();let p=vault::archive_admission::prepare("none",&source,&vault.join("assets")).unwrap();let reply=plan_request(request,"none",&p.handle,&raw.to_string()).await.unwrap();let plan=plans().lock().unwrap().get(&p.handle).unwrap().clone();(root,plan,reply)
        }
        async fn inspect_plan(plan:&Plan,fingerprint:&str)->Value {
            let pool=existing(&plan.request,&plan.binding,"none").await.unwrap();let operation=plan.envelope["operation_id"].as_str().unwrap();let receipt=historical(&pool,&plan.binding,operation,fingerprint).await.unwrap();let current=read_snapshot_from_pool(&pool,"none").await.unwrap();pool.close().await;json!({"receipt":receipt,"current_snapshot":current})
        }
        pub(crate) async fn genuine_import_owner_fixture(length:usize)->(PathBuf,vault::SnapshotRequestBinding,crate::local_database::attachment_download_owner::native_archive_import::Request,Value,Value){
            use crate::local_database::attachment_download_owner::native_archive_capture::tests as capture_tests;
            let base=root();let source=base.join("source-vault");fs::create_dir(&source).unwrap();capture_tests::installed_import_owner_bytes(&source,length).await;let archive=base.join("complete.zip");capture_tests::export_original_fixture(&source,archive.clone()).await;
            let destination=base.join("new-vault");fs::create_dir(&destination).unwrap();let settings=base.join("new-settings.json");fs::write(&settings,json!({"vault_path":destination}).to_string()).unwrap();let request=vault::SnapshotRequestBinding::capture_at(settings,destination.clone(),"none").unwrap();let prepared=vault::archive_admission::prepare("none",&archive,&destination.join("assets")).unwrap();let reply=plan_request(request.clone(),"none",&prepared.handle,&prepared.snapshot_json).await.unwrap();
            let mut snapshot:Value=serde_json::from_str(&prepared.snapshot_json).unwrap();let version=&reply["versions"][0];let original=snapshot["assets"][0]["url"].as_str().unwrap().to_string();let replacement=version["url"].as_str().unwrap().to_string();let text=snapshot["documents"][0]["content"].as_str().unwrap().to_string();let byte=text.find(&original).unwrap();let start=text[..byte].encode_utf16().count();let end=start+original.encode_utf16().count();snapshot["documents"][0]["content"]=json!(text.replace(&original,&replacement));snapshot["assets"][0]["url"]=version["url"].clone();snapshot["assets"][0]["vault_path"]=version["vault_path"].clone();snapshot["assets"][0]["size"]=version["size"].clone();
            let changes=vec![json!({"collection":"documents","index":0,"field":"content","offset_unit":"utf16_code_units","start":start,"end":end,"original":original,"replacement":replacement})];let result=commit("none",&prepared.handle,reply["plan_digest"].as_str().unwrap(),snapshot,changes).await.unwrap();let asset=&result["candidate"]["assets"][0];let selected=crate::local_database::attachment_download_owner::native_archive_import::Request{operation:prepared.handle,request_fingerprint:result["request_fingerprint"].as_str().unwrap().into(),asset_id:asset["id"].as_str().unwrap().into(),index:0,space_id:asset["space_id"].as_str().unwrap().into(),document_id:asset["document_id"].as_str().unwrap().into()};(base,request,selected,capture_tests::local(),result)
        }
        pub(crate) async fn import_existing_canonical_archive(archive:&std::path::Path,destination:PathBuf,lost_ack:bool)->(vault::SnapshotRequestBinding,Value){
            fs::create_dir(&destination).unwrap();let settings=destination.with_extension("settings.json");fs::write(&settings,json!({"vault_path":destination}).to_string()).unwrap();let selection=vault::SnapshotRequestBinding::capture_at(settings,destination.clone(),"none").unwrap();let prepared=vault::archive_admission::prepare("none",archive,&destination.join("assets")).unwrap();let reply=plan_request(selection.clone(),"none",&prepared.handle,&prepared.snapshot_json).await.unwrap();let plan=plans().lock().unwrap().get(&prepared.handle).unwrap().clone();let mut snapshot:Value=serde_json::from_str(&prepared.snapshot_json).unwrap();let mut changes=vec![];
            for version in reply["versions"].as_array().unwrap(){let index=version["index"].as_u64().unwrap()as usize;let original=snapshot["assets"][index]["url"].as_str().unwrap().to_string();let replacement=version["url"].as_str().unwrap().to_string();for(collection,field)in [("documents","content"),("documents","cover_image"),("drafts","content"),("drafts","cover_image"),("templates","content")]{if let Some(rows)=snapshot[collection].as_array_mut(){for(row_index,row)in rows.iter_mut().enumerate(){if let Some(text)=row[field].as_str().map(str::to_string){if let Some(byte)=text.find(&original){let start=text[..byte].encode_utf16().count();changes.push(json!({"collection":collection,"index":row_index,"field":field,"offset_unit":"utf16_code_units","start":start,"end":start+original.encode_utf16().count(),"original":original,"replacement":replacement}));row[field]=json!(text.replacen(&original,&replacement,1));}}}}}snapshot["assets"][index]["url"]=version["url"].clone();snapshot["assets"][index]["vault_path"]=version["vault_path"].clone();snapshot["assets"][index]["size"]=version["size"].clone();}
            if lost_ack{LOSE_COMMIT_REPLY.with(|flag|flag.set(true));}
            let committed=commit("none",&prepared.handle,reply["plan_digest"].as_str().unwrap(),snapshot,changes).await;
            let result=if lost_ack{let error=committed.unwrap_err();assert!(error.contains("lost acknowledgement"),"{error}");let pool=existing(&plan.request,&plan.binding,"none").await.unwrap();let raw:String=sqlx::query_scalar("SELECT result_json FROM native_archive_import_receipts WHERE operation_id=?1").bind(&prepared.handle).fetch_one(&pool).await.unwrap();let result:Value=serde_json::from_str(&raw).unwrap();let result=historical(&pool,&plan.binding,&prepared.handle,result["request_fingerprint"].as_str().unwrap()).await.unwrap().unwrap();pool.close().await;result}else{committed.unwrap()};(selection,result)
        }
        pub(crate) async fn fail_actual_history_import_before_receipt(archive:&std::path::Path,destination:PathBuf)->(vault::SnapshotRequestBinding,String,String){
            let settings=destination.with_extension("pending-settings.json");fs::write(&settings,json!({"vault_path":destination}).to_string()).unwrap();let selection=vault::SnapshotRequestBinding::capture_at(settings,destination.clone(),"none").unwrap();let prepared=vault::archive_admission::prepare("none",archive,&destination.join("assets")).unwrap();let reply=plan_request(selection.clone(),"none",&prepared.handle,&prepared.snapshot_json).await.unwrap();let plan=plans().lock().unwrap().get(&prepared.handle).unwrap().clone();let pool=existing(&plan.request,&plan.binding,"none").await.unwrap();let revision=revision(&pool).await.unwrap().unwrap();sqlx::query("CREATE TRIGGER fixture_abort_history_receipt BEFORE INSERT ON native_archive_import_receipts BEGIN SELECT RAISE(ABORT,'fixture original history receipt interruption'); END").execute(&pool).await.unwrap();pool.close().await;let candidate:Value=serde_json::from_str(&prepared.snapshot_json).unwrap();let error=commit("none",&prepared.handle,reply["plan_digest"].as_str().unwrap(),candidate,vec![]).await.unwrap_err();assert!(error.contains("fixture original history receipt interruption"),"{error}");(selection,revision,prepared.handle)
        }
        pub(crate) async fn import_existing_owner_archive(archive:&std::path::Path,destination:PathBuf,lost_ack:bool)->(vault::SnapshotRequestBinding,crate::local_database::attachment_download_owner::native_archive_import::Request,Value){let(selection,result)=import_existing_canonical_archive(archive,destination,lost_ack).await;let index=result["candidate"]["assets"].as_array().unwrap().iter().position(|asset|!asset["server_version_provenance"].is_null()||!asset["native_local_version_provenance"].is_null()).unwrap();let asset=&result["candidate"]["assets"][index];let request=crate::local_database::attachment_download_owner::native_archive_import::Request{operation:result["operation_id"].as_str().unwrap().into(),request_fingerprint:result["request_fingerprint"].as_str().unwrap().into(),asset_id:asset["id"].as_str().unwrap().into(),index,space_id:asset["space_id"].as_str().unwrap().into(),document_id:asset["document_id"].as_str().unwrap().into()};(selection,request,result)
        }
        #[tokio::test]
        async fn actual_receipt_archive_joint_import_preserves_original_raw_evidence_without_download_authority() {
            use crate::local_database::attachment_download_owner::{native_archive_capture::tests as capture_tests};
            let _test_guard=vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
            let base=root();let source=base.join("source-vault");fs::create_dir(&source).unwrap();capture_tests::installed(&source).await;
            let archive=base.join("complete.zip");capture_tests::export_original_fixture(&source,archive.clone()).await;
            let destination=base.join("new-vault");fs::create_dir(&destination).unwrap();let settings=base.join("new-settings.json");fs::write(&settings,json!({"vault_path":destination}).to_string()).unwrap();
            let prepared=vault::archive_admission::prepare("none",&archive,&destination.join("assets")).unwrap();
            let request=vault::SnapshotRequestBinding::capture_at(settings,destination.clone(),"none").unwrap();let reply=plan_request(request,"none",&prepared.handle,&prepared.snapshot_json).await.unwrap();
            let plan=plans().lock().unwrap().get(&prepared.handle).unwrap().clone();let provenance=reply["imported_provenance"].clone();assert_eq!(provenance["authority"],"portable_provenance_only");
            let mut candidate:Value=serde_json::from_str(&prepared.snapshot_json).unwrap();let original=candidate["assets"][0].clone();let version=&reply["versions"][0];candidate["assets"][0]["url"]=version["url"].clone();candidate["assets"][0]["vault_path"]=version["vault_path"].clone();candidate["assets"][0]["size"]=version["size"].clone();
            let result=commit("none",&prepared.handle,reply["plan_digest"].as_str().unwrap(),candidate.clone(),vec![]).await.unwrap();assert_eq!(result["imported_provenance"],provenance);assert_eq!(result["candidate"]["assets"][0]["server_version_provenance"],original["server_version_provenance"]);assert_eq!(fs::read(version["vault_path"].as_str().unwrap()).unwrap(),b"exact retired original");
            let pool=existing(&plan.request,&plan.binding,"none").await.unwrap();let download_table:Option<(String,)>=sqlx::query_as("SELECT name FROM sqlite_master WHERE name='native_attachment_download_receipts'").fetch_optional(&pool).await.unwrap();assert!(download_table.is_none(),"copied provenance creates no download-serving receipt table");pool.close().await;
            let inspected=inspect_plan(&plan,result["request_fingerprint"].as_str().unwrap()).await;assert_eq!(inspected["receipt"]["imported_provenance"],provenance);assert_eq!(inspected["current_snapshot"],result["current_snapshot"]);
            let fresh=std::process::Command::new("python3").args(["-c","import sqlite3,json,sys; c=sqlite3.connect(sys.argv[1]); r=json.loads(c.execute('SELECT result_json FROM native_archive_import_receipts').fetchone()[0]); print(json.dumps(json.loads(r['imported_provenance']['raw_receipts_json']))); c.close()"]).arg(&plan.binding.database_path).output().unwrap();assert!(fresh.status.success());let original_strings:Vec<String>=serde_json::from_str(provenance["raw_receipts_json"].as_str().unwrap()).unwrap();assert_eq!(serde_json::from_slice::<Vec<String>>(&fresh.stdout).unwrap(),original_strings);
            let asset=&result["candidate"]["assets"][0];let read_request=crate::local_database::attachment_download_owner::native_archive_import::Request{operation:prepared.handle.clone(),request_fingerprint:result["request_fingerprint"].as_str().unwrap().into(),asset_id:asset["id"].as_str().unwrap().into(),index:0,space_id:asset["space_id"].as_str().unwrap().into(),document_id:asset["document_id"].as_str().unwrap().into()};
            let interactive=crate::local_database::attachment_download_owner::native_archive_import::Owner::capture(plan.request.clone(),"none",read_request,capture_tests::local()).unwrap();let missing=interactive.with_read(capture_tests::local(),|_|Ok(())).await.unwrap_err();assert!(missing.contains("exact reference missing"),"interactive imported serving still requires its current reference: {missing}");
            capture_tests::archive_owner(&destination).await.with_capture(capture_tests::local(),|capture|{assert_eq!(capture.binaries.len(),1);assert_eq!(capture.raw_imports.len(),1);Ok(())}).await.unwrap();
            fs::remove_dir_all(base).unwrap();
        }
        #[tokio::test]
        async fn actual_lost_ack_reopen_retains_full_original_receipt_and_later_b() {
            let _test_guard = vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let raw=empty();let(root,plan,reply)=prepared(raw.clone(),None).await;LOSE_COMMIT_REPLY.with(|flag|flag.set(true));assert!(commit("none",&plan.handle,reply["plan_digest"].as_str().unwrap(),raw.clone(),vec![]).await.unwrap_err().contains("lost acknowledgement"));
            let pool=existing(&plan.request,&plan.binding,"none").await.unwrap();let(op,fp,request):(String,String,String)=sqlx::query_as("SELECT operation_id,request_fingerprint,request_json FROM native_archive_import_intents").fetch_one(&pool).await.unwrap();let request:Value=serde_json::from_str(&request).unwrap();assert_eq!(request["candidate"],raw);let mut b=raw.clone();b["documents"]=json!([{"id":"B","space_id":"other","title":"Later B","content":"keep B"}]);write_snapshot_to_database(&pool,"none",&b).await.unwrap();pool.close().await;
            let inspected=inspect_plan(&plan,&fp).await;assert_eq!(inspected["receipt"]["operation_id"],op);assert_eq!(inspected["receipt"]["candidate"],raw);assert_eq!(inspected["current_snapshot"]["documents"],b["documents"]);fs::remove_dir_all(root).unwrap();
        }
        #[tokio::test]
        async fn failed_install_intent_can_be_explicitly_retained_abandoned_without_erasing_evidence() {
            let _test_guard = vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let raw=empty();let(root,plan,reply)=prepared(raw.clone(),None).await;AFTER_INTENT.with(|hook|*hook.borrow_mut()=Some(Box::new(||Err("deliberate failure after durable intent".into()))));assert!(commit("none",&plan.handle,reply["plan_digest"].as_str().unwrap(),raw.clone(),vec![]).await.is_err());
            let pool=existing(&plan.request,&plan.binding,"none").await.unwrap();let(op,fp,original):(String,String,String)=sqlx::query_as("SELECT operation_id,request_fingerprint,request_json FROM native_archive_import_intents").fetch_one(&pool).await.unwrap();assert!(read_snapshot_from_pool(&pool,"none").await.unwrap().is_none());pool.close().await;
            let resolved=abandon_request(plan.request.clone(),"none",plan.binding.clone(),&op,&fp).await.unwrap();assert_eq!(resolved["retained_abandoned"],true);assert_eq!(resolved["canonical_committed"],false);let pool=existing(&plan.request,&plan.binding,"none").await.unwrap();let(retained,):(String,)=sqlx::query_as("SELECT request_json FROM native_archive_import_intents WHERE operation_id=?1").bind(&op).fetch_one(&pool).await.unwrap();assert_eq!(retained,original);assert!(historical(&pool,&plan.binding,&op,&fp).await.unwrap().is_none());assert!(reset_allowed(&PathBuf::from(&plan.binding.database_path)).await.is_err());pool.close().await;
            assert!(commit("none",&plan.handle,reply["plan_digest"].as_str().unwrap(),raw.clone(),vec![]).await.is_err(),"the retained abandoned original handle cannot dispatch later");
            let mut next=raw.clone();next["unknown_root"]=json!({"next":"valid import"});let source=root.join("source.zip");zip(&source,&next,None);let prepared=vault::archive_admission::prepare("none",&source,&plan.destination).unwrap();let new_plan=plan_request(plan.request.clone(),"none",&prepared.handle,&next.to_string()).await.unwrap();let committed=commit("none",&prepared.handle,new_plan["plan_digest"].as_str().unwrap(),next.clone(),vec![]).await.unwrap();assert_eq!(committed["candidate"],next);let pool=existing(&plan.request,&plan.binding,"none").await.unwrap();let(count,):(i64,)=sqlx::query_as("SELECT COUNT(*) FROM native_archive_import_intents").fetch_one(&pool).await.unwrap();assert_eq!(count,2);let(retained,):(String,)=sqlx::query_as("SELECT request_json FROM native_archive_import_intents WHERE operation_id=?1").bind(&op).fetch_one(&pool).await.unwrap();assert_eq!(retained,original);pool.close().await;fs::remove_dir_all(root).unwrap();
        }
        #[tokio::test]
        async fn two_connection_writer_lock_excludes_late_abandonment_and_preserves_committed_receipt() {
            let _test_guard = vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let raw=empty();let(root,plan,reply)=prepared(raw.clone(),None).await;let result=commit("none",&plan.handle,reply["plan_digest"].as_str().unwrap(),raw,vec![]).await.unwrap();let op=plan.envelope["operation_id"].as_str().unwrap().to_string();let fp=result["request_fingerprint"].as_str().unwrap().to_string();
            let first=existing(&plan.request,&plan.binding,"none").await.unwrap();let mut held=first.begin().await.unwrap();writer(&mut held,&plan.binding).await.unwrap();let second=existing(&plan.request,&plan.binding,"none").await.unwrap();let binding=plan.binding.clone();let task=tokio::spawn(async move {let mut tx=second.begin().await.map_err(|e|e.to_string())?;writer(&mut tx,&binding).await?;tx.rollback().await.map_err(|e|e.to_string())});tokio::task::yield_now().await;assert!(!task.is_finished());held.commit().await.unwrap();assert!(task.await.unwrap().is_ok());
            assert!(abandon_request(plan.request.clone(),"none",plan.binding.clone(),&op,&fp).await.unwrap_err().contains("already committed"));first.close().await;fs::remove_dir_all(root).unwrap();
        }
        #[tokio::test]
        async fn fresh_zero_asset_joint_commit_seals_full_request_and_survives_reopen() {
            let _test_guard = vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let raw=empty();let(root,plan,reply)=prepared(raw.clone(),None).await;assert!(plan.binding.database_path.ends_with(".sqlite3"));
            let result=commit("none",&plan.handle,reply["plan_digest"].as_str().unwrap(),raw.clone(),vec![]).await.unwrap();assert_eq!(result["canonical_committed"],true);assert_eq!(result["candidate"],raw);
            let fp=result["request_fingerprint"].as_str().unwrap();let inspected=inspect_plan(&plan,fp).await;assert_eq!(inspected["receipt"]["candidate"]["unknown_root"],raw["unknown_root"]);assert_eq!(inspected["current_snapshot"]["documents"],json!([]));assert!(reset_allowed(&PathBuf::from(&plan.binding.database_path)).await.is_err());fs::remove_dir_all(root).unwrap();
        }
        #[tokio::test]
        async fn deep_page_joint_archive_lost_ack_cold_reload_and_original_receipt_preserve_edges_drafts_and_queue() {
            let _test_guard=vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
            for depth in [4usize,32,256] {
                let mut raw=empty();raw["spaces"]=json!([{"id":"s","name":"S"}]);
                raw["documents"]=json!((0..depth).map(|i|json!({"id":format!("p{i}"),"space_id":"s","title":format!("Page{i}"),"parent_id":if i==0 {Value::Null} else {json!(format!("p{}",i-1))},"content":format!("  Exact {i} Ω\r\n[asset](asset://retained)\n\n"),"future":{"raw_history":"retained"}})).collect::<Vec<_>>());
                raw["drafts"]=json!([{"id":"d","space_id":"s","document_id":format!("p{}",depth-1),"owner_id":"local-user","title":"Private saved draft","content":"draft exact\r\n"}]);
                raw["assets"]=json!([{"id":"a","name":"retained","source":"browser","size":0,"url":"asset://retained","space_id":"s","document_id":"p0","created_at":"original"}]);
                raw["sync_state"]=json!({"state":"idle","queued_operations":[{"id":"one-original-op","entity_type":"document","entity_id":"p0","action":"create","queued_at":"2026-10-15T00:00:00Z","payload":{"raw":"retained"}}]});
                let(root,plan,reply)=prepared(raw.clone(),None).await;
                LOSE_COMMIT_REPLY.with(|flag|flag.set(true));
                assert!(commit("none",&plan.handle,reply["plan_digest"].as_str().unwrap(),raw.clone(),vec![]).await.unwrap_err().contains("lost acknowledgement"));
                let pool=existing(&plan.request,&plan.binding,"none").await.unwrap();
                let(operation,fingerprint):(String,String)=sqlx::query_as("SELECT operation_id,request_fingerprint FROM native_archive_import_intents").fetch_one(&pool).await.unwrap();pool.close().await;
                // Archive plans are consumed after installation. The existing
                // original-receipt inspection acknowledges uncertain outcomes;
                // redispatching the consumed in-memory plan is not recovery.
                let cold=inspect_plan(&plan,&fingerprint).await;assert_eq!(cold["receipt"]["canonical_committed"],true);assert_eq!(cold["receipt"]["candidate"],raw);assert_eq!(cold["receipt"]["operation_id"],operation);
                let by_id=|rows:&Value|rows.as_array().unwrap().iter().map(|r|(r["id"].as_str().unwrap().to_string(),r.clone())).collect::<std::collections::BTreeMap<_,_>>();
                assert_eq!(by_id(&cold["current_snapshot"]["documents"]),by_id(&raw["documents"]));assert_eq!(cold["current_snapshot"]["drafts"],raw["drafts"]);assert_eq!(cold["current_snapshot"]["assets"],raw["assets"]);assert_eq!(cold["current_snapshot"]["sync_state"]["queued_operations"],raw["sync_state"]["queued_operations"]);
                let inspected_again=inspect_plan(&plan,&fingerprint).await;assert_eq!(inspected_again["receipt"],cold["receipt"]);assert_eq!(inspected_again["current_snapshot"],cold["current_snapshot"]);
                fs::remove_dir_all(root).unwrap();
            }
        }
        #[tokio::test]
        async fn mixed_native_genuine_archive_lost_ack_cold_receipt_and_binary_reexport_preserve_32_32_32() {
            use crate::local_database::attachment_download_owner::native_archive_capture::tests as capture_tests;
            let _serial=vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
            let base=root();let source=base.join("source-vault");fs::create_dir(&source).unwrap();
            capture_tests::installed_import_owner_bytes(&source,196608).await;
            let source_db=source.join(".starlane/starlane-notes.sqlite3");let pool=open_database(&source_db).await.unwrap();
            let mut snapshot=read_snapshot_from_pool(&pool,"none").await.unwrap().unwrap();let original_asset=snapshot["assets"][0].clone();let original_url=original_asset["url"].as_str().unwrap();let original_bytes=fs::read(original_asset["vault_path"].as_str().unwrap()).unwrap();assert_eq!(original_bytes,vec![7u8;196608]);
            let mut pages=(0..32).map(|i|json!({"id":if i==0{"prior-page".to_string()}else{format!("p{i}")},"space_id":"local-space","folder_id":"f31","title":format!("Page {i}"),"parent_id":if i==0{Value::Null}else if i==1{json!("prior-page")}else{json!(format!("p{}",i-1))},"content":format!("  Page {i} Ω\r\n[asset]({original_url})\n\n"),"future":{"retained_history_reference":"opaque, not authentic server publication history"}})).collect::<Vec<_>>();
            pages.extend((0..32).map(|i|json!({"id":format!("sibling{i}"),"space_id":"local-space","folder_id":"f31","title":format!("Sibling {i}"),"parent_id":"p31","content":format!("  Sibling {i} Ω\r\n\n")})));
            snapshot["folders"]=json!((0..32).map(|i|json!({"id":format!("f{i}"),"space_id":"local-space","name":format!("Folder{i}"),"parent_id":if i==0{Value::Null}else{json!(format!("f{}",i-1))}})).collect::<Vec<_>>());snapshot["documents"]=json!(pages);
            snapshot["drafts"]=json!([{"id":"mixed-private","document_id":"p31","space_id":"local-space","owner_id":"local-user","title":"Private saved draft","content":format!("  Private Ω\r\n[asset]({original_url})\n\n"),"future":{"retain":"private original"}}]);
            snapshot["templates"]=json!([{"id":"mixed-template","space_id":"local-space","name":"Template","content":"  Template Ω\r\n\n"}]);
            snapshot["sync_state"]=json!({"state":"idle","queued_operations":[{"id":"mixed-original-op","entity_type":"document","entity_id":"p31","action":"update_content","queued_at":"2026-10-15T12:00:00Z","payload":{"content":"exact queued content"}}]});
            let selection=vault::SnapshotRequestBinding::capture_at(source.with_extension("settings.json"),source.clone(),"none").unwrap();
            let prior=revision(&pool).await.unwrap();let mut current_revision=write_snapshot_with_revision_checked(&pool,"none",&snapshot,prior.as_deref(),true,||selection.preflight_snapshot(&snapshot)).await.unwrap();
            assert!(!finish_snapshot_mirror(&pool,&current_revision,||selection.write_mirror(&snapshot)).await.mirror_pending);
            let by_id=|rows:&Value|rows.as_array().unwrap().iter().map(|row|(row["id"].as_str().unwrap().to_string(),row.clone())).collect::<std::collections::BTreeMap<_,_>>();let original_pages=by_id(&snapshot["documents"]);
            // Exercise the real native commit/mirror protocol for target-only
            // lifecycle projections, without claiming a new lifecycle handler.
            for state in ["archived","active","trashed","active"]{
                let mut next=snapshot.clone();if state=="archived"{next["documents"][15]["archived_at"]=json!("2026-10-15T12:00:00Z");}if state=="trashed"{next["documents"][15]["deleted_at"]=json!("2026-10-15T12:01:00Z");}
                current_revision=write_snapshot_with_revision_checked(&pool,"none",&next,Some(&current_revision),true,||selection.preflight_snapshot(&next)).await.unwrap();let receipt=finish_snapshot_mirror(&pool,&current_revision,||selection.write_mirror(&next)).await;assert!(receipt.canonical_committed);assert!(!receipt.mirror_pending);
                let loaded=read_snapshot_from_pool(&pool,"none").await.unwrap().unwrap();assert_eq!(by_id(&loaded["documents"]),by_id(&next["documents"]));assert_eq!(loaded["drafts"],next["drafts"]);assert_eq!(loaded["sync_state"]["queued_operations"],next["sync_state"]["queued_operations"]);
                for(id,row)in by_id(&loaded["documents"]){if id!="p15"{assert_eq!(row,original_pages[&id]);}}
            }
            let before=read_snapshot_from_pool(&pool,"none").await.unwrap().unwrap();pool.close().await;
            let original_receipts=capture_tests::archive_owner(&source).await.with_capture(capture_tests::local(),|capture|{assert_eq!(capture.binaries.len(),1);Ok(capture.raw_receipts.clone())}).await.unwrap();assert_eq!(original_receipts.len(),1);
            let archive=base.join("mixed-original.zip");capture_tests::export_original_fixture(&source,archive.clone()).await;let archive_bytes=fs::read(&archive).unwrap();
            // The original body queue belongs to a page whose asset reference
            // would be remapped. This is a real refusal, not permission to alter
            // its immutable source/queue binding during restoration.
            let rejected=base.join("rejected-vault");fs::create_dir(&rejected).unwrap();let rejected_settings=rejected.with_extension("settings.json");fs::write(&rejected_settings,json!({"vault_path":rejected}).to_string()).unwrap();
            let rejected_selection=vault::SnapshotRequestBinding::capture_at(rejected_settings,rejected.clone(),"none").unwrap();let staged=vault::archive_admission::prepare("none",&archive,&rejected.join("assets")).unwrap();let reply=plan_request(rejected_selection.clone(),"none",&staged.handle,&staged.snapshot_json).await.unwrap();let rejected_plan=plans().lock().unwrap().get(&staged.handle).unwrap().clone();
            let mut submitted:Value=serde_json::from_str(&staged.snapshot_json).unwrap();let version=&reply["versions"][0];let mut changes=vec![];let old=submitted["assets"][0]["url"].as_str().unwrap().to_string();let replacement=version["url"].as_str().unwrap().to_string();
            for collection in ["documents","drafts"]{for(index,row)in submitted[collection].as_array_mut().unwrap().iter_mut().enumerate(){let body=row["content"].as_str().unwrap().to_string();if let Some(byte)=body.find(&old){let start=body[..byte].encode_utf16().count();changes.push(json!({"collection":collection,"index":index,"field":"content","offset_unit":"utf16_code_units","start":start,"end":start+old.encode_utf16().count(),"original":old,"replacement":replacement}));row["content"]=json!(body.replacen(&old,&replacement,1));}}}
            for field in ["url","vault_path","size"]{submitted["assets"][0][field]=version[field].clone();}
            let rejected_pool=existing(&rejected_selection,&rejected_plan.binding,"none").await.unwrap();let prior_target=read_snapshot_from_pool(&rejected_pool,"none").await.unwrap();rejected_pool.close().await;let prior_database=fs::read(&rejected_plan.binding.database_path).unwrap();
            let error=commit("none",&staged.handle,reply["plan_digest"].as_str().unwrap(),submitted,changes).await.unwrap_err();assert!(error.contains("Queued body or source depends on remapped archive references"),"{error}");assert_eq!(fs::read(&rejected_plan.binding.database_path).unwrap(),prior_database);assert!(!PathBuf::from(version["vault_path"].as_str().unwrap()).exists());
            let rejected_pool=existing(&rejected_selection,&rejected_plan.binding,"none").await.unwrap();assert_eq!(read_snapshot_from_pool(&rejected_pool,"none").await.unwrap(),prior_target);let intents:(i64,)=sqlx::query_as("SELECT COUNT(*) FROM native_archive_import_intents").fetch_one(&rejected_pool).await.unwrap();assert_eq!(intents.0,0);let receipts:(i64,)=sqlx::query_as("SELECT COUNT(*) FROM native_archive_import_receipts").fetch_one(&rejected_pool).await.unwrap();assert_eq!(receipts.0,0);rejected_pool.close().await;
            assert_eq!(fs::read(&archive).unwrap(),archive_bytes);assert_eq!(fs::read(original_asset["vault_path"].as_str().unwrap()).unwrap(),original_bytes);vault::archive_admission::cancel("none",&staged.handle).unwrap();plans().lock().unwrap().remove(&staged.handle);
            // A distinct source capture can retain an independent original body
            // queue for an unchanged sibling without weakening that refusal.
            let source_pool=open_database(&source_db).await.unwrap();let mut independent=before.clone();independent["sync_state"]["queued_operations"][0]["entity_id"]=json!("sibling31");let revision=write_snapshot_with_revision_checked(&source_pool,"none",&independent,Some(&current_revision),true,||selection.preflight_snapshot(&independent)).await.unwrap();assert!(!finish_snapshot_mirror(&source_pool,&revision,||selection.write_mirror(&independent)).await.mirror_pending);let before=read_snapshot_from_pool(&source_pool,"none").await.unwrap().unwrap();source_pool.close().await;
            let archive=base.join("mixed-independent-queue.zip");capture_tests::export_original_fixture(&source,archive.clone()).await;let archive_bytes=fs::read(&archive).unwrap();
            let destination=base.join("restored-vault");let(request,result)=import_existing_canonical_archive(&archive,destination.clone(),true).await;
            let binding:Binding=serde_json::from_value(result["binding"].clone()).unwrap();let operation=result["operation_id"].as_str().unwrap();let fingerprint=result["request_fingerprint"].as_str().unwrap();assert_eq!(result["canonical_committed"],true);let candidate=&result["candidate"];
            let new_asset=&candidate["assets"][0];assert_eq!(new_asset["id"],original_asset["id"]);assert_eq!(new_asset["space_id"],original_asset["space_id"]);assert_eq!(new_asset["document_id"],original_asset["document_id"]);assert_ne!(new_asset["vault_path"],original_asset["vault_path"]);assert_eq!(fs::read(new_asset["vault_path"].as_str().unwrap()).unwrap(),original_bytes);
            let replacement=new_asset["url"].as_str().unwrap();let mut expected=before.clone();for collection in ["documents","drafts"]{for row in expected[collection].as_array_mut().unwrap(){if let Some(body)=row["content"].as_str(){row["content"]=json!(body.replace(original_url,replacement));}}}
            for _ in 0..2{let reopened=existing(&request,&binding,"none").await.unwrap();assert_eq!(historical(&reopened,&binding,operation,fingerprint).await.unwrap().unwrap(),result);let loaded=read_snapshot_from_pool(&reopened,"none").await.unwrap().unwrap();for collection in ["documents","folders","drafts","templates"]{assert_eq!(by_id(&loaded[collection]),by_id(&expected[collection]),"{collection}");}assert_eq!(loaded["sync_state"]["queued_operations"],before["sync_state"]["queued_operations"]);let(count,):(i64,)=sqlx::query_as("SELECT COUNT(*) FROM native_archive_import_receipts").fetch_one(&reopened).await.unwrap();assert_eq!(count,1);reopened.close().await;}
            // Re-export from the restored native owner while the source path is
            // absent; preserve the original files under a held fixture name.
            let held=base.join("held-source");fs::rename(&source,&held).unwrap();let again=base.join("restored-export.zip");capture_tests::export_original_fixture(&destination,again.clone()).await;assert!(again.is_file());fs::rename(&held,&source).unwrap();
            let source_reopened=open_database(&source_db).await.unwrap();assert_eq!(read_snapshot_from_pool(&source_reopened,"none").await.unwrap().unwrap(),before);source_reopened.close().await;
            let receipts_after=capture_tests::archive_owner(&source).await.with_capture(capture_tests::local(),|capture|Ok(capture.raw_receipts.clone())).await.unwrap();assert_eq!(receipts_after,original_receipts);assert_eq!(fs::read(original_asset["vault_path"].as_str().unwrap()).unwrap(),original_bytes);assert_eq!(fs::read(archive).unwrap(),archive_bytes);
            fs::remove_dir_all(base).unwrap();
        }
        #[tokio::test]
        async fn multiasset_joint_commit_preserves_old_bytes_mixed_indices_ids_and_unicode_spans() {
            let _test_guard = vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let mut raw=empty();raw["spaces"]=json!([{"id":"space","name":"Space"}]);raw["documents"]=json!([{"id":"page","space_id":"space","title":"Title","content":"🌙 ![media](asset://localhost/original)","future":{"keep":true}}]);raw["assets"]=json!([{"id":"remote","source":"browser","name":"remote","url":"https://example.test/remote","size":1,"created_at":"raw-browser"},{"id":"media","source":"tauri","name":"image.png","url":"asset://localhost/original","vault_path":"/original","size":196608,"created_at":"raw","future":"keep"}]);
            let bytes=vec![73u8;196608];let(root,plan,reply)=prepared(raw.clone(),Some(&bytes)).await;let version=&reply["versions"][0];assert_eq!(version["index"],1);assert_eq!(version["id"],"media");
            let mut candidate=raw.clone();candidate["assets"][1]["url"]=version["url"].clone();candidate["assets"][1]["vault_path"]=version["vault_path"].clone();candidate["assets"][1]["size"]=version["size"].clone();candidate["documents"][0]["content"]=json!(format!("🌙 ![media]({})",version["url"].as_str().unwrap()));
            let change=json!({"collection":"documents","index":0,"field":"content","offset_unit":"utf16_code_units","start":12,"end":38,"original":"asset://localhost/original","replacement":version["url"]});
            // Derive the exact JS code-unit span, including the two-unit emoji.
            let text=raw["documents"][0]["content"].as_str().unwrap();let byte=text.find("asset://").unwrap();let start=text[..byte].encode_utf16().count();let mut change=change;change["start"]=json!(start);change["end"]=json!(start+"asset://localhost/original".encode_utf16().count());
            fs::create_dir_all(&plan.destination).unwrap();fs::write(plan.destination.join("media-image.png"),b"old media").unwrap();let result=commit("none",&plan.handle,reply["plan_digest"].as_str().unwrap(),candidate.clone(),vec![change]).await.unwrap();assert_eq!(result["candidate"],candidate);assert_eq!(fs::read(plan.destination.join("media-image.png")).unwrap(),b"old media");assert_eq!(fs::read(version["vault_path"].as_str().unwrap()).unwrap(),bytes);assert_eq!(result["current_snapshot"]["documents"][0]["future"],raw["documents"][0]["future"]);fs::remove_dir_all(root).unwrap();
        }
        #[tokio::test]
        async fn final_selection_drift_after_sql_retains_intent_and_rolls_back_before_install() {
            let _test_guard = vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let raw = empty();
            let (root, plan, reply) = prepared(raw.clone(), None).await;
            let settings = root.join("settings.json");
            let foreign = root.join("foreign");
            let selected = foreign.clone();
            AFTER_SQL.with(|hook| *hook.borrow_mut() = Some(Box::new(move || {
                fs::write(settings, json!({"vault_path": selected}).to_string()).map_err(|e| e.to_string())
            })));
            assert!(commit("none", &plan.handle, reply["plan_digest"].as_str().unwrap(), raw, vec![]).await.is_err());
            assert!(!foreign.exists());
            assert!(!plan.destination.exists());
            fs::write(root.join("settings.json"), json!({"vault_path": root.join("vault")}).to_string()).unwrap();
            let pool = existing(&plan.request, &plan.binding, "none").await.unwrap();
            let (intents,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM native_archive_import_intents").fetch_one(&pool).await.unwrap();
            let (receipts,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM native_archive_import_receipts").fetch_one(&pool).await.unwrap();
            assert_eq!(intents, 1);
            assert_eq!(receipts, 0);
            assert_eq!(revision(&pool).await.unwrap(), plan.revision);
            assert!(read_snapshot_from_pool(&pool, "none").await.unwrap().is_none());
            pool.close().await;
            fs::remove_dir_all(root).unwrap();
        }
        #[tokio::test]
        async fn normalized_trigger_mutation_rejects_before_durable_intent_or_asset_install() {
            let _test_guard = vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let mut raw = empty();
            raw["documents"] = json!([{"id":"doc", "space_id":"space", "title":"Original", "content":"raw body"}]);
            let (root, plan, reply) = prepared(raw.clone(), None).await;
            let pool = existing(&plan.request, &plan.binding, "none").await.unwrap();
            sqlx::query("CREATE TRIGGER alter_archive_document AFTER INSERT ON local_documents BEGIN UPDATE local_documents SET payload='{}' WHERE id=NEW.id; END").execute(&pool).await.unwrap();
            pool.close().await;
            assert!(commit("none", &plan.handle, reply["plan_digest"].as_str().unwrap(), raw, vec![]).await.is_err());
            let pool = existing(&plan.request, &plan.binding, "none").await.unwrap();
            let (intents,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM native_archive_import_intents").fetch_one(&pool).await.unwrap();
            let (documents,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM local_documents").fetch_one(&pool).await.unwrap();
            assert_eq!(intents, 0);
            assert_eq!(documents, 0);
            assert!(!plan.destination.exists());
            pool.close().await;
            vault::archive_admission::cancel("none", &plan.handle).unwrap();
            plans().lock().unwrap().remove(&plan.handle);
            fs::remove_dir_all(root).unwrap();
        }
        #[tokio::test]
        async fn two_original_plans_require_failed_intent_abandonment_before_new_import() {
            let _test_guard = vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let raw = empty();
            let (root, a, a_reply) = prepared(raw.clone(), None).await;
            let staged_b = vault::archive_admission::prepare("none", &root.join("source.zip"), &a.destination).unwrap();
            let b_reply = plan_request(a.request.clone(), "none", &staged_b.handle, &raw.to_string()).await.unwrap();
            let b = plans().lock().unwrap().get(&staged_b.handle).unwrap().clone();
            assert_eq!(a.revision, b.revision);
            AFTER_INTENT.with(|hook| *hook.borrow_mut() = Some(Box::new(|| Err("deliberate A failure after durable intent".into()))));
            assert!(commit("none", &a.handle, a_reply["plan_digest"].as_str().unwrap(), raw.clone(), vec![]).await.is_err());
            let pool = existing(&a.request, &a.binding, "none").await.unwrap();
            let before: Vec<(String, String, String)> = sqlx::query_as("SELECT operation_id,request_fingerprint,request_json FROM native_archive_import_intents ORDER BY operation_id").fetch_all(&pool).await.unwrap();
            assert_eq!(before.len(), 1);
            pool.close().await;
            let denied = commit("none", &b.handle, b_reply["plan_digest"].as_str().unwrap(), raw.clone(), vec![]).await.unwrap_err();
            assert!(denied.contains("prior archive import"));
            assert!(!a.destination.exists());
            let pool = existing(&a.request, &a.binding, "none").await.unwrap();
            assert_eq!(sqlx::query_as::<_, (String, String, String)>("SELECT operation_id,request_fingerprint,request_json FROM native_archive_import_intents ORDER BY operation_id").fetch_all(&pool).await.unwrap(), before);
            assert_eq!(revision(&pool).await.unwrap(), a.revision);
            assert!(read_snapshot_from_pool(&pool, "none").await.unwrap().is_none());
            pool.close().await;
            abandon_request(a.request.clone(), "none", a.binding.clone(), &before[0].0, &before[0].1).await.unwrap();
            let staged_c = vault::archive_admission::prepare("none", &root.join("source.zip"), &a.destination).unwrap();
            let c_reply = plan_request(a.request.clone(), "none", &staged_c.handle, &raw.to_string()).await.unwrap();
            let result = commit("none", &staged_c.handle, c_reply["plan_digest"].as_str().unwrap(), raw.clone(), vec![]).await.unwrap();
            assert_eq!(result["candidate"], raw);
            let pool = existing(&a.request, &a.binding, "none").await.unwrap();
            let retained: (String,) = sqlx::query_as("SELECT request_json FROM native_archive_import_intents WHERE operation_id=?1").bind(&before[0].0).fetch_one(&pool).await.unwrap();
            assert_eq!(retained.0, before[0].2);
            pool.close().await;
            fs::remove_dir_all(root).unwrap();
        }
        #[tokio::test]
        async fn two_connection_admission_wait_rechecks_new_durable_unresolved_intent() {
            let _test_guard = vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let raw = empty();
            let (root, plan, _) = prepared(raw.clone(), None).await;
            let first = existing(&plan.request, &plan.binding, "none").await.unwrap();
            let second = existing(&plan.request, &plan.binding, "none").await.unwrap();
            let mut held = first.begin().await.unwrap();
            writer(&mut held, &plan.binding).await.unwrap();
            require_no_other_unresolved(&mut held, &plan.binding, "A").await.unwrap();
            let binding = plan.binding.clone();
            let (started, ready) = tokio::sync::oneshot::channel();
            let waiting = tokio::spawn(async move {
                let mut tx = second.begin().await.map_err(|e| e.to_string())?;
                let _ = started.send(());
                writer(&mut tx, &binding).await?;
                let result = require_no_other_unresolved(&mut tx, &binding, "B").await;
                tx.rollback().await.map_err(|e| e.to_string())?;
                result
            });
            ready.await.unwrap();
            tokio::task::yield_now().await;
            assert!(!waiting.is_finished());
            let request = json!({"plan": plan.envelope, "candidate": raw, "reference_changes": []});
            sqlx::query("INSERT INTO native_archive_import_intents(operation_id,provider,instance_id,request_fingerprint,request_json) VALUES('A','none',?1,?2,?3)")
                .bind(&plan.binding.database_instance_id).bind(digest(&request).unwrap()).bind(request.to_string()).execute(&mut *held).await.unwrap();
            held.commit().await.unwrap();
            assert!(waiting.await.unwrap().unwrap_err().contains("prior archive import"));
            assert_eq!(revision(&first).await.unwrap(), plan.revision);
            first.close().await;
            vault::archive_admission::cancel("none", &plan.handle).unwrap();
            plans().lock().unwrap().remove(&plan.handle);
            fs::remove_dir_all(root).unwrap();
        }
        #[tokio::test]
        async fn receipt_trigger_state_cursor_and_instance_changes_roll_back_before_install() {
            let _test_guard = vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            for mutation in [
                "UPDATE local_replay_checkpoints SET payload='{}'",
                "UPDATE local_replay_checkpoints SET provider='foreign'",
                "UPDATE local_replay_checkpoints SET downloaded_at='altered'",
                "UPDATE local_sync_cursors SET payload='{}'",
                "UPDATE local_sync_cursors SET sync_state='error'",
                "UPDATE local_sync_cursors SET updated_at='altered'",
                "UPDATE local_vault_metadata SET value='{}' WHERE key='sync_state'",
                "UPDATE local_vault_metadata SET value='[]' WHERE key='maintenance_target_catalogs'",
                "UPDATE native_archive_instance SET instance_id='altered'",
                "UPDATE local_vault_metadata SET value='altered' WHERE key='mirror_pending'",
                "UPDATE local_vault_metadata SET value='altered' WHERE key='replay_queue_order_format'",
            ] {
                let mut raw = empty();
                raw["sync_state"] = json!({"state":"idle", "updated_at":"raw-time", "future_state":{"keep":"exact"}, "queued_operations":[], "recovery_checkpoints":[{"id":"checkpoint", "spaceId":"space", "downloadedAt":"raw-time", "future_checkpoint":{"keep":"raw"}}]});
                raw["maintenance_target_catalogs"] = json!([{"id":"raw-catalog", "future":{"keep":true}}]);
                let (root, plan, reply) = prepared(raw.clone(), None).await;
                let pool = existing(&plan.request, &plan.binding, "none").await.unwrap();
                sqlx::query(&format!("CREATE TRIGGER mutate_archive_result AFTER INSERT ON native_archive_import_receipts BEGIN {mutation}; END")).execute(&pool).await.unwrap();
                pool.close().await;
                assert!(commit("none", &plan.handle, reply["plan_digest"].as_str().unwrap(), raw.clone(), vec![]).await.is_err(), "{mutation}");
                let pool = existing(&plan.request, &plan.binding, "none").await.unwrap();
                assert_eq!(instance(&pool).await.unwrap(), plan.binding.database_instance_id);
                assert_eq!(revision(&pool).await.unwrap(), plan.revision);
                assert!(read_snapshot_from_pool(&pool, "none").await.unwrap().is_none());
                let (intents,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM native_archive_import_intents").fetch_one(&pool).await.unwrap();
                let (receipts,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM native_archive_import_receipts").fetch_one(&pool).await.unwrap();
                assert_eq!(intents, 1);
                assert_eq!(receipts, 0);
                assert!(!plan.destination.exists());
                pool.close().await;
                fs::remove_dir_all(root).unwrap();
            }
            let mut raw = empty();
            raw["sync_state"] = json!({"state":"idle", "updated_at":"raw-time", "future_state":{"keep":"exact"}, "queued_operations":[], "recovery_checkpoints":[{"id":"checkpoint", "spaceId":"space", "downloadedAt":"raw-time", "future_checkpoint":{"keep":"raw"}}]});
            let (root, plan, reply) = prepared(raw.clone(), None).await;
            let result = commit("none", &plan.handle, reply["plan_digest"].as_str().unwrap(), raw.clone(), vec![]).await.unwrap();
            assert_eq!(result["current_snapshot"]["sync_state"]["future_state"], raw["sync_state"]["future_state"]);
            assert_eq!(result["binding"]["database_instance_id"], plan.binding.database_instance_id);
            fs::remove_dir_all(root).unwrap();
        }
        #[tokio::test]
        async fn candidate_or_queue_mismatch_rejects_before_intent_and_any_asset_install() {
            let _test_guard = vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let raw=empty();let(root,mut plan,reply)=prepared(raw.clone(),None).await;let operation=json!({"id":"queued","entity_type":"document","entity_id":"outside","action":"update_content","queued_at":"raw","payload":{"content":"original"},"replay_provider":"none","future":{"keep":true}});plan.destination_snapshot=Some(json!({"sync_state":{"queued_operations":[operation]}}));plans().lock().unwrap().insert(plan.handle.clone(),plan.clone());
            assert!(commit("none",&plan.handle,reply["plan_digest"].as_str().unwrap(),raw.clone(),vec![]).await.is_err());let mut other=raw.clone();other["unknown_root"]=json!("changed");assert!(candidate(&plan,&other,&[]).is_err());let pool=existing(&plan.request,&plan.binding,"none").await.unwrap();let(count,):(i64,)=sqlx::query_as("SELECT COUNT(*) FROM native_archive_import_intents").fetch_one(&pool).await.unwrap();assert_eq!(count,0);assert!(!plan.destination.exists());pool.close().await;vault::archive_admission::cancel("none",&plan.handle).unwrap();plans().lock().unwrap().remove(&plan.handle);fs::remove_dir_all(root).unwrap();
        }
        #[tokio::test]
        async fn raw_queue_union_allows_exact_duplicates_and_retains_unrelated_provenance() {
            let _test_guard = vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let mut raw=empty();let operation=json!({"id":"queued","entity_type":"document","entity_id":"outside","action":"update_content","queued_at":"raw","payload":{"content":"original"},"replay_provider":"none","server_context":{"server_base":"https://original.invalid","account_id":"actor","session_scope":"sid"},"future":{"keep":true}});raw["sync_state"]=json!({"state":"idle","queued_operations":[operation.clone()]});let(root,mut plan,_)=prepared(raw.clone(),None).await;plan.destination_snapshot=Some(json!({"sync_state":{"queued_operations":[operation.clone()]}}));assert!(candidate(&plan,&raw,&[]).is_ok());let mut conflict=operation.clone();conflict["server_context"]["account_id"]=json!("foreign");plan.destination_snapshot=Some(json!({"sync_state":{"queued_operations":[conflict]}}));assert!(candidate(&plan,&raw,&[]).is_err());vault::archive_admission::cancel("none",&plan.handle).unwrap();plans().lock().unwrap().remove(&plan.handle);fs::remove_dir_all(root).unwrap();
        }
        #[tokio::test]
        async fn immutable_receipt_keeps_original_a_after_actual_later_b_and_rejects_wrong_fingerprint() {
            let _test_guard = vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let raw=empty();let(root,plan,reply)=prepared(raw.clone(),None).await;let result=commit("none",&plan.handle,reply["plan_digest"].as_str().unwrap(),raw.clone(),vec![]).await.unwrap();let fp=result["request_fingerprint"].as_str().unwrap();let pool=existing(&plan.request,&plan.binding,"none").await.unwrap();let mut b=raw.clone();b["documents"]=json!([{"id":"later","space_id":"other","title":"B","content":"later canonical B"}]);write_snapshot_to_database(&pool,"none",&b).await.unwrap();assert!(historical(&pool,&plan.binding,plan.envelope["operation_id"].as_str().unwrap(),"wrong").await.unwrap().is_none());assert!(sqlx::query("UPDATE native_archive_import_receipts SET result_json='{}'").execute(&pool).await.is_err());pool.close().await;let inspected=inspect_plan(&plan,fp).await;assert_eq!(inspected["receipt"]["candidate"],raw);assert_eq!(inspected["current_snapshot"]["documents"],b["documents"]);fs::remove_dir_all(root).unwrap();
        }
    }

}

#[tauri::command]
pub async fn plan_native_vault_archive_import(app: AppHandle, provider: String, handle: String, original_snapshot_json: String) -> Result<Value,String> {
    #[cfg(all(unix, not(any(target_os="ios",target_os="android"))))]
    { return joint_archive::plan(&app,&provider,&handle,&original_snapshot_json).await; }
    #[cfg(not(all(unix, not(any(target_os="ios",target_os="android")))))]
    { let _=(app,provider,handle,original_snapshot_json);Err("Joint archive import requires a supported updated desktop shell".into()) }
}
#[tauri::command]
pub async fn commit_native_vault_archive_import(provider: String, handle: String, plan_digest: String, candidate: Value, reference_changes: Vec<Value>) -> Result<Value,String> {
    #[cfg(all(unix, not(any(target_os="ios",target_os="android"))))]
    { return joint_archive::commit(&provider,&handle,&plan_digest,candidate,reference_changes).await; }
    #[cfg(not(all(unix, not(any(target_os="ios",target_os="android")))))]
    { let _=(provider,handle,plan_digest,candidate,reference_changes);Err("Joint archive import requires a supported updated desktop shell".into()) }
}
#[tauri::command]
pub async fn inspect_native_vault_archive_import(app: AppHandle, provider: String, binding: Value, operation_id: String, request_fingerprint: String) -> Result<Value,String> {
    #[cfg(all(unix, not(any(target_os="ios",target_os="android"))))]
    { return joint_archive::inspect(&app,&provider,serde_json::from_value(binding).map_err(|_|"Original archive binding is malformed")?,&operation_id,&request_fingerprint).await; }
    #[cfg(not(all(unix, not(any(target_os="ios",target_os="android")))))]
    { let _=(app,provider,binding,operation_id,request_fingerprint);Err("Joint archive recovery requires a supported updated desktop shell".into()) }
}
#[tauri::command]
pub async fn list_native_vault_archive_imports(app: AppHandle, provider: String, binding: Option<Value>, cursor: Option<String>, limit: u32) -> Result<Value,String> {
    #[cfg(all(unix, not(any(target_os="ios",target_os="android"))))]
    { return match binding { Some(binding) => joint_archive::list(&app,&provider,serde_json::from_value(binding).map_err(|_|"Original archive binding is malformed")?,cursor,limit).await, None => joint_archive::discover(&app,&provider,cursor,limit).await }; }
    #[cfg(not(all(unix, not(any(target_os="ios",target_os="android")))))]
    { let _=(app,provider,binding,cursor,limit);Err("Joint archive recovery requires a supported updated desktop shell".into()) }
}

#[tauri::command]
pub async fn abandon_native_vault_archive_import(app:AppHandle,provider:String,binding:Value,operation_id:String,request_fingerprint:String)->Result<Value,String> {
    #[cfg(all(unix, not(any(target_os="ios",target_os="android"))))]
    {return joint_archive::abandon(&app,&provider,serde_json::from_value(binding).map_err(|_|"Original archive binding malformed")?,&operation_id,&request_fingerprint).await;}
    #[cfg(not(all(unix, not(any(target_os="ios",target_os="android")))))]
    {let _=(app,provider,binding,operation_id,request_fingerprint);Err("Joint archive recovery requires a supported updated desktop shell".into())}
}

#[cfg(target_os = "linux")]
pub(crate) mod attachment_download_owner;

#[cfg(all(test,not(any(target_os="ios",target_os="android"))))]
mod deep_folder_admission_tests {
    use super::*;
    use std::fs;
    fn fixture()->(PathBuf,PathBuf,vault::SnapshotRequestBinding) {
        let dir=std::env::temp_dir().join(format!("starlane-deep-canonical-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir(&dir).unwrap();let root=dir.join("vault");let settings=dir.join("settings.json");fs::write(&settings,serde_json::to_vec(&json!({"vault_path":root})).unwrap()).unwrap();
        let request=vault::SnapshotRequestBinding::capture_at(settings,root.clone(),"none").unwrap();(dir,root,request)
    }
    fn candidate(depth:usize)->Value {
        json!({"schema_version":1,"spaces":[{"id":"s","name":"S"}],
            "folders":(0..depth).map(|i|json!({"id":format!("f{i}"),"space_id":"s","name":"x","parent_id":if i==0 {Value::Null} else {json!(format!("f{}",i-1))}})).collect::<Vec<_>>(),
            "documents":[{"id":"page","space_id":"s","title":"P","content":"exact body\r\nretained","future_raw":{"history":"retain"},"folder_id":format!("f{}",depth-1)}],
            "drafts":[{"id":"draft","space_id":"s","document_id":"page","title":"Saved draft","content":"draft exact body"}],"templates":[],"assets":[],
            "sync_state":{"state":"idle","queued_operations":[{"id":"one-operation","entity_type":"folder","entity_id":"f0","action":"create","queued_at":"2026-10-15T00:00:00Z"}]}})
    }
    #[tokio::test]
    async fn invalid_admission_precedes_destination_creation_and_legacy_rename() {
        for existing in [false,true] {
            let (dir,root,request)=fixture();
            if existing {fs::create_dir(&root).unwrap();fs::write(root.join("starlane-vault-none.sqlite3"),b"legacy sentinel").unwrap();}
            let mut source=candidate(4);source["folders"][0]["parent_id"]=json!("f3");
            assert!(snapshot_candidate_database_after_wait(&request,&source).await.is_err());
            assert!(!root.join(".starlane").exists());assert!(!root.join("starlane-vault-none.json").exists());
            if existing {assert_eq!(fs::read(root.join("starlane-vault-none.sqlite3")).unwrap(),b"legacy sentinel");} else {assert!(!root.exists());}
            fs::remove_dir_all(dir).unwrap();
        }
    }
    #[tokio::test]
    async fn actual_sqlite_deep_commit_cold_reload_final_admission_rollback_and_mirror_only_retry() {
        for depth in [4,32,256] {
            let (dir,root,request)=fixture();let source=candidate(depth);
            let (guard,path)=snapshot_candidate_database_after_wait(&request,&source).await.unwrap();
            let pool=open_database(&path).await.unwrap();migrate_database(&pool).await.unwrap();
            let revision=write_snapshot_with_revision_checked(&pool,"none",&source,None,true,||request.preflight_snapshot(&source)).await.unwrap();
            let first=finish_snapshot_mirror(&pool,&revision,||request.write_mirror(&source)).await;
            assert!(first.canonical_committed);assert!(!first.mirror_pending);pool.close().await;
            let pool=open_database(&path).await.unwrap();
            let loaded=read_snapshot_from_pool(&pool,"none").await.unwrap().unwrap();
            let by_id=|rows:&Value| rows.as_array().unwrap().iter().map(|row|(row["id"].as_str().unwrap().to_string(),row.clone())).collect::<std::collections::BTreeMap<_,_>>();
            assert_eq!(by_id(&loaded["folders"]),by_id(&source["folders"]));assert_eq!(loaded["documents"],source["documents"]);assert_eq!(loaded["drafts"],source["drafts"]);
            let before:Vec<(String,String)>=sqlx::query_as("SELECT key,value FROM local_vault_metadata ORDER BY key").fetch_all(&pool).await.unwrap();
            let original_json=fs::read(root.join("starlane-vault-none.json")).unwrap();
            let mut changed=source.clone();changed["documents"][0]["content"]=json!("would overwrite");
            // The production final admission callback runs after all awaited SQL,
            // before COMMIT. A destination failure rolls back rows/queue/revision.
            let result=write_snapshot_with_revision_checked(&pool,"none",&changed,Some(&revision),true,|| {
                let mut invalid=changed.clone();invalid["folders"][0]["parent_id"]=json!("missing");request.preflight_snapshot(&invalid)
            }).await;
            assert!(result.is_err());assert_eq!(read_snapshot_from_pool(&pool,"none").await.unwrap().unwrap(),loaded);
            let after:Vec<(String,String)>=sqlx::query_as("SELECT key,value FROM local_vault_metadata ORDER BY key").fetch_all(&pool).await.unwrap();assert_eq!(after,before);assert_eq!(fs::read(root.join("starlane-vault-none.json")).unwrap(),original_json);
            // A later mirror failure is a committed canonical mutation; retry the
            // mirror only, preserving the one admitted queue operation.
            let revision2=write_snapshot_with_revision_checked(&pool,"none",&changed,Some(&revision),true,||request.preflight_snapshot(&changed)).await.unwrap();
            let receipt=finish_snapshot_mirror(&pool,&revision2,||Err("simulated postcommit disk failure".into())).await;assert!(receipt.canonical_committed);assert!(receipt.mirror_pending);
            let retry=finish_snapshot_mirror(&pool,&revision2,||request.write_mirror(&changed)).await;assert!(retry.canonical_committed);assert!(!retry.mirror_pending);assert_eq!(retry.canonical_revision,revision2);
            let (queued,):(i64,)=sqlx::query_as("SELECT COUNT(*) FROM local_queued_operations").fetch_one(&pool).await.unwrap();assert_eq!(queued,1);
            assert_eq!(read_snapshot_from_pool(&pool,"none").await.unwrap().unwrap()["documents"][0]["content"],"would overwrite");
            pool.close().await;drop(guard);fs::remove_dir_all(dir).unwrap();
        }
    }
}
