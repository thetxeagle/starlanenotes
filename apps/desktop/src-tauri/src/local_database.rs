use serde::Serialize;
use serde_json::{json, Map, Value};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Row, Sqlite};
use std::path::PathBuf;
use tauri::AppHandle;

use crate::vault;

const LOCAL_DATABASE_SCHEMA_VERSION: u64 = 1;
const COLLECTIONS: &[&str] = &[
    "spaces",
    "folders",
    "documents",
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
];

#[derive(Debug, Clone, Serialize)]
pub struct NativeLocalDatabaseDescriptor {
    schema_version: u64,
    provider: String,
    store_kind: String,
    database_path: String,
    supports_snapshot_bridge: bool,
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
        supports_operation_queue: true,
        replay_backends: vec!["sqlite".to_string(), "postgresql".to_string()],
    })
}

#[tauri::command]
pub async fn read_native_local_database_snapshot(
    app: AppHandle,
    provider: String,
) -> Result<Option<Value>, String> {
    let database_path = database_path(&app, &provider)?;
    if !database_path.exists() {
        return vault::read_vault_snapshot(app, provider);
    }

    let pool = open_database(&database_path).await?;
    migrate_database(&pool).await?;

    let has_snapshot: Option<(String,)> =
        sqlx::query_as("SELECT value FROM local_vault_metadata WHERE key = 'schema_version'")
            .fetch_optional(&pool)
            .await
            .map_err(|e| e.to_string())?;
    if has_snapshot.is_none() {
        return vault::read_vault_snapshot(app, provider);
    }

    let mut snapshot = Map::new();
    snapshot.insert(
        "schema_version".to_string(),
        json!(LOCAL_DATABASE_SCHEMA_VERSION),
    );

    for collection in COLLECTIONS {
        let values = match *collection {
            "spaces" => {
                read_normalized_payload_collection(&pool, "local_spaces", "name, id").await?
            }
            "documents" => {
                read_normalized_payload_collection(
                    &pool,
                    "local_documents",
                    "space_id, folder_id, title, id",
                )
                .await?
            }
            "folders" => {
                read_normalized_payload_collection(
                    &pool,
                    "local_folders",
                    "space_id, parent_id, name, id",
                )
                .await?
            }
            "tags" => {
                read_normalized_payload_collection(&pool, "local_tags", "space_id, name, id")
                    .await?
            }
            "property_schemas" => {
                read_normalized_payload_collection(&pool, "local_properties", "space_id, name, id")
                    .await?
            }
            "assets" => {
                read_normalized_payload_collection(
                    &pool,
                    "local_assets",
                    "space_id, document_id, name, id",
                )
                .await?
            }
            "templates" => {
                read_normalized_payload_collection(&pool, "local_templates", "space_id, title, id")
                    .await?
            }
            "workflow_templates" => {
                read_normalized_payload_collection(
                    &pool,
                    "local_workflow_templates",
                    "space_id, name, id",
                )
                .await?
            }
            "document_workflows" => {
                read_normalized_payload_collection(
                    &pool,
                    "local_document_workflows",
                    "space_id, document_id, id",
                )
                .await?
            }
            "document_workflow_events" => {
                read_normalized_payload_collection(
                    &pool,
                    "local_document_workflow_events",
                    "document_id, created_at, id",
                )
                .await?
            }
            "property_values" => {
                read_normalized_payload_collection(
                    &pool,
                    "local_property_values",
                    "document_id, property_id, id",
                )
                .await?
            }
            "doc_tags" => {
                read_normalized_payload_collection(
                    &pool,
                    "local_doc_tags",
                    "document_id, tag_id, id",
                )
                .await?
            }
            "search_index" => {
                read_normalized_payload_collection(
                    &pool,
                    "local_search_index",
                    "space_id, updated_at, document_id",
                )
                .await?
            }
            _ => Vec::new(),
        };
        let values = if values.is_empty() {
            read_snapshot_bridge_collection(&pool, collection).await?
        } else {
            values
        };
        snapshot.insert((*collection).to_string(), Value::Array(values));
    }

    if let Some(sync_state) = read_normalized_sync_state(&pool, &provider).await? {
        snapshot.insert("sync_state".to_string(), sync_state);
    } else if let Some((raw_sync_state,)) = sqlx::query_as::<_, (String,)>(
        "SELECT value FROM local_vault_metadata WHERE key = 'sync_state'",
    )
    .fetch_optional(&pool)
    .await
    .map_err(|e| e.to_string())?
    {
        if let Ok(value) = serde_json::from_str::<Value>(&raw_sync_state) {
            snapshot.insert("sync_state".to_string(), value);
        }
    }

    Ok(Some(Value::Object(snapshot)))
}

async fn read_snapshot_bridge_collection(
    pool: &Pool<Sqlite>,
    collection: &str,
) -> Result<Vec<Value>, String> {
    let records = sqlx::query(
        "SELECT payload FROM local_vault_records WHERE collection = ?1 ORDER BY row_order, id",
    )
    .bind(collection)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

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
    let rows = sqlx::query(&format!("SELECT payload FROM {table} ORDER BY {order_by}"))
        .fetch_all(pool)
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

#[tauri::command]
pub async fn write_native_local_database_snapshot(
    app: AppHandle,
    provider: String,
    snapshot: Value,
) -> Result<(), String> {
    let database_path = database_path(&app, &provider)?;
    let pool = open_database(&database_path).await?;
    migrate_database(&pool).await?;
    write_snapshot_to_database(&pool, &provider, &snapshot).await?;

    // Keep the portable JSON snapshot and Markdown mirror current until SQLite becomes the only
    // native runtime store.
    vault::write_vault_snapshot(app, provider, snapshot)
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
    tx.commit().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn delete_native_local_database_record(
    app: AppHandle,
    provider: String,
    collection: String,
    id: String,
) -> Result<(), String> {
    let database_path = database_path(&app, &provider)?;
    let pool = open_database(&database_path).await?;
    migrate_database(&pool).await?;

    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM local_vault_records WHERE collection = ?1 AND id = ?2")
        .bind(&collection)
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    delete_normalized_record(&mut tx, &collection, &id).await?;
    tx.commit().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reset_native_local_database(app: AppHandle, provider: String) -> Result<(), String> {
    let database_path = database_path(&app, &provider)?;
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
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS local_vault_metadata (
            key TEXT PRIMARY KEY NOT NULL,
            value TEXT NOT NULL
        )",
    )
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
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
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

async fn write_snapshot_to_database(
    pool: &Pool<Sqlite>,
    provider: &str,
    snapshot: &Value,
) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    sqlx::query("DELETE FROM local_vault_metadata")
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM local_vault_records")
        .execute(&mut *tx)
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
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }

    sqlx::query("INSERT INTO local_vault_metadata (key, value) VALUES ('schema_version', ?1)")
        .bind(LOCAL_DATABASE_SCHEMA_VERSION.to_string())
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(sync_state) = snapshot.get("sync_state") {
        sqlx::query("INSERT INTO local_vault_metadata (key, value) VALUES ('sync_state', ?1)")
            .bind(serde_json::to_string(sync_state).map_err(|e| e.to_string())?)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
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
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    write_normalized_snapshot_tables(&mut tx, provider, snapshot).await?;

    tx.commit().await.map_err(|e| e.to_string())
}

async fn upsert_snapshot_bridge_record(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    collection: &str,
    id: &str,
    payload: &Value,
    row_order: Option<i64>,
) -> Result<(), String> {
    let updated_at = payload
        .get("updated_at")
        .or_else(|| payload.get("created_at"))
        .and_then(Value::as_str)
        .map(str::to_string);
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
            for operation in operations {
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

async fn read_normalized_sync_state(
    pool: &Pool<Sqlite>,
    provider: &str,
) -> Result<Option<Value>, String> {
    let cursor: Option<(String, String)> =
        sqlx::query_as("SELECT sync_state, payload FROM local_sync_cursors WHERE provider = ?1")
            .bind(provider)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?;

    let operation_rows = sqlx::query(
        "SELECT id, entity_type, entity_id, action, queued_at, attempts, last_error, payload
         FROM local_queued_operations ORDER BY queued_at, id",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let replay_checkpoints = read_normalized_replay_checkpoints(pool, provider).await?;

    if cursor.is_none() && operation_rows.is_empty() && replay_checkpoints.is_empty() {
        return Ok(None);
    }

    let mut sync_state = cursor
        .as_ref()
        .and_then(|(_, payload)| serde_json::from_str::<Value>(payload).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    if let Some((state, _)) = cursor {
        sync_state.insert("state".to_string(), Value::String(state));
    }

    let queued_operations = operation_rows
        .into_iter()
        .map(|row| {
            let mut operation = Map::new();
            operation.insert("id".to_string(), Value::String(row.get::<String, _>("id")));
            operation.insert(
                "entity_type".to_string(),
                Value::String(row.get::<String, _>("entity_type")),
            );
            operation.insert(
                "entity_id".to_string(),
                Value::String(row.get::<String, _>("entity_id")),
            );
            operation.insert(
                "action".to_string(),
                Value::String(row.get::<String, _>("action")),
            );
            operation.insert(
                "queued_at".to_string(),
                Value::String(row.get::<String, _>("queued_at")),
            );
            operation.insert(
                "attempts".to_string(),
                Value::Number(serde_json::Number::from(row.get::<i64, _>("attempts"))),
            );
            if let Some(last_error) = row.get::<Option<String>, _>("last_error") {
                operation.insert("last_error".to_string(), Value::String(last_error));
            }
            if let Some(payload) = row.get::<Option<String>, _>("payload") {
                if let Ok(value) = serde_json::from_str::<Value>(&payload) {
                    operation.insert("payload".to_string(), value);
                }
            }
            Value::Object(operation)
        })
        .collect::<Vec<_>>();

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
    let rows = sqlx::query(
        "SELECT payload FROM local_replay_checkpoints
         WHERE provider = ?1
         ORDER BY downloaded_at DESC, id",
    )
    .bind(provider)
    .fetch_all(pool)
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
