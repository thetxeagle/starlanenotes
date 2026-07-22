use axum::{
    extract::{
        ws::{Message, WebSocket},
        State, WebSocketUpgrade,
    },
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::auth::jwt;
use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::AppState;

/// Active document sessions — tracks connected clients per document
type DocSessions = Arc<RwLock<HashMap<String, broadcast::Sender<SyncMessage>>>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncMessage {
    pub doc_id: String,
    pub user_id: String,
    pub msg_type: String, // "change", "cursor", "presence"
    pub payload: serde_json::Value,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/ws/sync", get(ws_handler))
}

pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/sync/replay/preflight", post(preflight_replay_manifest))
        .route("/sync/replay", post(execute_replay_manifest))
        .route("/sync/replay/history", get(list_replay_history))
}

#[derive(Clone, Debug, Deserialize)]
pub struct SyncReplayManifestRequest {
    pub schema_version: u64,
    pub provider: String,
    pub replay_contract: SyncReplayContract,
    pub operation_count: usize,
    #[serde(default)]
    pub recovery_checkpoint_count: usize,
    #[serde(default)]
    pub recovery_checkpoints: Vec<Value>,
    pub operations: Vec<SyncReplayOperation>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SyncReplayContract {
    pub schema_version: u64,
    pub operation_order: String,
    pub idempotency_key: String,
    pub transport: String,
    pub server_backends: Vec<String>,
    pub required_payload_status: String,
    pub conflict_policy: String,
    pub web_canonical_store: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SyncReplayOperation {
    pub id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub action: String,
    pub queued_at: String,
    pub payload_status: String,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct SyncReplayPreflightResponse {
    pub status: String,
    pub provider: String,
    pub ready: bool,
    pub operation_count: usize,
    pub ready_operation_count: usize,
    pub blocked_operation_count: usize,
    pub recovery_checkpoint_count: usize,
    pub target_backends: Vec<String>,
    pub blockers: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SyncReplayExecutionResponse {
    pub status: String,
    pub attempted_operation_count: usize,
    pub applied_operation_count: usize,
    pub skipped_operation_count: usize,
    pub failed_operation_count: usize,
    pub unsupported_operation_count: usize,
    pub results: Vec<SyncReplayOperationResult>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SyncReplayOperationResult {
    pub operation_id: String,
    pub entity_type: String,
    pub action: String,
    pub status: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SyncReplayHistoryResponse {
    pub manifests: Vec<SyncReplayHistoryManifest>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SyncReplayHistoryManifest {
    pub id: String,
    pub provider: String,
    pub status: String,
    pub attempted_by: String,
    pub operation_count: i64,
    pub attempted_operation_count: i64,
    pub applied_operation_count: i64,
    pub skipped_operation_count: i64,
    pub failed_operation_count: i64,
    pub unsupported_operation_count: i64,
    pub recovery_checkpoint_count: i64,
    pub created_at: String,
    pub checkpoints: Vec<SyncReplayHistoryCheckpoint>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SyncReplayHistoryCheckpoint {
    pub id: String,
    pub space_id: Option<String>,
    pub source_server_url: Option<String>,
    pub downloaded_by_user_id: Option<String>,
    pub checkpoint: Value,
    pub created_at: String,
}

async fn preflight_replay_manifest(
    _auth: AuthUser,
    Json(manifest): Json<SyncReplayManifestRequest>,
) -> Result<Json<SyncReplayPreflightResponse>, AppError> {
    Ok(Json(validate_replay_manifest(manifest)))
}

async fn execute_replay_manifest(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(manifest): Json<SyncReplayManifestRequest>,
) -> Result<Json<SyncReplayExecutionResponse>, AppError> {
    let preflight = validate_replay_manifest(manifest.clone());
    if !preflight.ready {
        return Err(AppError::BadRequest(format!(
            "Replay manifest is blocked: {}",
            preflight.blockers.join("; ")
        )));
    }

    let mut results = Vec::new();
    for operation in &manifest.operations {
        results.push(execute_replay_operation(&state, &auth, &manifest, operation).await);
    }

    let applied_operation_count = results
        .iter()
        .filter(|result| result.status == "applied")
        .count();
    let skipped_operation_count = results
        .iter()
        .filter(|result| result.status == "skipped")
        .count();
    let failed_operation_count = results
        .iter()
        .filter(|result| result.status == "failed")
        .count();
    let unsupported_operation_count = results
        .iter()
        .filter(|result| result.status == "unsupported")
        .count();

    let response = SyncReplayExecutionResponse {
        status: if failed_operation_count == 0 && unsupported_operation_count == 0 {
            "completed"
        } else {
            "partial"
        }
        .to_string(),
        attempted_operation_count: results.len(),
        applied_operation_count,
        skipped_operation_count,
        failed_operation_count,
        unsupported_operation_count,
        results,
    };
    record_replay_manifest_history(&state, &auth, &manifest, &response).await?;

    Ok(Json(response))
}

async fn list_replay_history(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<SyncReplayHistoryResponse>, AppError> {
    let rows: Vec<(String, String, String, String, i64, i64, i64, i64, i64, i64, i64, String)> =
        sqlx::query_as(
            "SELECT id, provider, status, attempted_by, operation_count, attempted_operation_count, \
             applied_operation_count, skipped_operation_count, failed_operation_count, unsupported_operation_count, \
             recovery_checkpoint_count, created_at \
             FROM sync_replay_manifests WHERE attempted_by = ?1 ORDER BY created_at DESC LIMIT 20",
        )
        .bind(&auth.claims.sub)
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?;

    let mut manifests = Vec::new();
    for row in rows {
        let checkpoints = fetch_replay_history_checkpoints(&state, &row.0).await?;
        manifests.push(SyncReplayHistoryManifest {
            id: row.0,
            provider: row.1,
            status: row.2,
            attempted_by: row.3,
            operation_count: row.4,
            attempted_operation_count: row.5,
            applied_operation_count: row.6,
            skipped_operation_count: row.7,
            failed_operation_count: row.8,
            unsupported_operation_count: row.9,
            recovery_checkpoint_count: row.10,
            created_at: row.11,
            checkpoints,
        });
    }

    Ok(Json(SyncReplayHistoryResponse { manifests }))
}

fn validate_replay_manifest(manifest: SyncReplayManifestRequest) -> SyncReplayPreflightResponse {
    let mut blockers = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut blocked_operation_ids = HashSet::new();

    if manifest.schema_version != 1 {
        blockers.push("Replay manifest schema_version must be 1".to_string());
    }
    if manifest.replay_contract.schema_version != 1 {
        blockers.push("Replay contract schema_version must be 1".to_string());
    }
    if manifest.replay_contract.operation_order != "queued_at_ascending" {
        blockers.push("Replay contract must use queued_at_ascending operation order".to_string());
    }
    if manifest.replay_contract.idempotency_key != "operation.id" {
        blockers.push("Replay contract must use operation.id as the idempotency key".to_string());
    }
    if manifest.replay_contract.transport != "server-api" {
        blockers.push("Replay contract must use server-api transport".to_string());
    }
    if manifest.replay_contract.required_payload_status != "available" {
        blockers.push("Replay contract must require available payloads".to_string());
    }
    if manifest.replay_contract.conflict_policy != "server_validates_version_and_permissions" {
        blockers.push(
            "Replay contract must use server_validates_version_and_permissions conflict policy"
                .to_string(),
        );
    }
    if manifest.replay_contract.web_canonical_store != "server-api" {
        blockers.push("Replay contract must keep web canonical storage on server-api".to_string());
    }
    if !manifest
        .replay_contract
        .server_backends
        .iter()
        .any(|backend| backend == "sqlite")
        || !manifest
            .replay_contract
            .server_backends
            .iter()
            .any(|backend| backend == "postgresql")
    {
        blockers.push(
            "Replay contract must target both SQLite and PostgreSQL server backends".to_string(),
        );
    }
    if manifest.operation_count != manifest.operations.len() {
        blockers.push(format!(
            "operation_count {} does not match {} operations",
            manifest.operation_count,
            manifest.operations.len()
        ));
    }
    if manifest.recovery_checkpoint_count != manifest.recovery_checkpoints.len() {
        blockers.push(format!(
            "recovery_checkpoint_count {} does not match {} checkpoints",
            manifest.recovery_checkpoint_count,
            manifest.recovery_checkpoints.len()
        ));
    }

    let mut previous_sort_key: Option<(String, String)> = None;
    for operation in &manifest.operations {
        if operation.id.trim().is_empty() {
            blockers.push("Replay operation id must not be empty".to_string());
        } else if !seen_ids.insert(operation.id.clone()) {
            blockers.push(format!("Duplicate replay operation id: {}", operation.id));
        }
        if operation.entity_type.trim().is_empty() {
            blockers.push(format!(
                "Replay operation {} has empty entity_type",
                operation.id
            ));
        }
        if operation.entity_id.trim().is_empty() {
            blockers.push(format!(
                "Replay operation {} has empty entity_id",
                operation.id
            ));
        }
        if operation.action.trim().is_empty() {
            blockers.push(format!(
                "Replay operation {} has empty action",
                operation.id
            ));
        }
        if operation.queued_at.trim().is_empty() {
            blockers.push(format!(
                "Replay operation {} has empty queued_at",
                operation.id
            ));
        }
        if operation.payload_status == manifest.replay_contract.required_payload_status {
            if operation.payload.is_null() {
                blocked_operation_ids.insert(operation.id.clone());
                blockers.push(format!(
                    "Replay operation {} has available status but null payload",
                    operation.id
                ));
            }
        } else {
            blocked_operation_ids.insert(operation.id.clone());
            blockers.push(format!(
                "Replay operation {} is missing payload",
                operation.id
            ));
        }

        let sort_key = (operation.queued_at.clone(), operation.id.clone());
        if previous_sort_key
            .as_ref()
            .is_some_and(|previous| previous > &sort_key)
        {
            blockers.push(
                "Replay operations must be sorted by queued_at ascending, then operation id"
                    .to_string(),
            );
        }
        previous_sort_key = Some(sort_key);
    }

    validate_replay_dependency_order(
        &manifest.operations,
        &mut blockers,
        &mut blocked_operation_ids,
    );

    let blocked_operation_count = blocked_operation_ids.len();
    let ready_operation_count = manifest
        .operations
        .len()
        .saturating_sub(blocked_operation_count);
    let ready = blockers.is_empty();

    SyncReplayPreflightResponse {
        status: if ready { "ready" } else { "blocked" }.to_string(),
        provider: manifest.provider,
        ready,
        operation_count: manifest.operations.len(),
        ready_operation_count,
        blocked_operation_count,
        recovery_checkpoint_count: manifest.recovery_checkpoint_count,
        target_backends: manifest.replay_contract.server_backends,
        blockers,
    }
}

fn validate_replay_dependency_order(
    operations: &[SyncReplayOperation],
    blockers: &mut Vec<String>,
    blocked_operation_ids: &mut HashSet<String>,
) {
    let create_indexes = replay_create_indexes(operations);

    for (index, operation) in operations.iter().enumerate() {
        if operation.action != "create" {
            add_later_create_dependency_blocker(
                blockers,
                blocked_operation_ids,
                &create_indexes,
                operation,
                index,
                &operation.entity_type,
                &operation.entity_id,
            );
        }

        let Some(payload) = operation.payload.as_object() else {
            continue;
        };

        match (operation.entity_type.as_str(), operation.action.as_str()) {
            ("document", "create" | "update_content") => {
                if let Some(folder_id) = payload_string(payload, "folder_id") {
                    add_later_create_dependency_blocker(
                        blockers,
                        blocked_operation_ids,
                        &create_indexes,
                        operation,
                        index,
                        "folder",
                        &folder_id,
                    );
                }
            }
            ("folder", "create" | "update") => {
                if let Some(parent_id) = payload_string(payload, "parent_id") {
                    add_later_create_dependency_blocker(
                        blockers,
                        blocked_operation_ids,
                        &create_indexes,
                        operation,
                        index,
                        "folder",
                        &parent_id,
                    );
                }
            }
            ("document_tag", "add" | "remove") => {
                if let Some(document_id) = payload_string(payload, "document_id") {
                    add_later_create_dependency_blocker(
                        blockers,
                        blocked_operation_ids,
                        &create_indexes,
                        operation,
                        index,
                        "document",
                        &document_id,
                    );
                }
                if let Some(tag_id) = payload_string(payload, "tag_id") {
                    add_later_create_dependency_blocker(
                        blockers,
                        blocked_operation_ids,
                        &create_indexes,
                        operation,
                        index,
                        "tag",
                        &tag_id,
                    );
                }
            }
            ("property_schema", "reorder") => {
                if let Some(property_ids) = payload.get("property_ids").and_then(Value::as_array) {
                    for property_id in property_ids.iter().filter_map(Value::as_str) {
                        add_later_create_dependency_blocker(
                            blockers,
                            blocked_operation_ids,
                            &create_indexes,
                            operation,
                            index,
                            "property_schema",
                            property_id,
                        );
                    }
                }
            }
            ("document_property_values", "set") => {
                let document_id = payload_string(payload, "document_id")
                    .unwrap_or_else(|| operation.entity_id.clone());
                add_later_create_dependency_blocker(
                    blockers,
                    blocked_operation_ids,
                    &create_indexes,
                    operation,
                    index,
                    "document",
                    &document_id,
                );
                if let Some(values) = payload.get("values").and_then(Value::as_array) {
                    for value in values.iter().filter_map(Value::as_object) {
                        if let Some(property_id) = payload_string(value, "property_id") {
                            add_later_create_dependency_blocker(
                                blockers,
                                blocked_operation_ids,
                                &create_indexes,
                                operation,
                                index,
                                "property_schema",
                                &property_id,
                            );
                        }
                    }
                }
            }
            ("document_workflow", "create" | "update" | "transition" | "delete") => {
                let document_id = payload_string(payload, "document_id")
                    .unwrap_or_else(|| operation.entity_id.clone());
                add_later_create_dependency_blocker(
                    blockers,
                    blocked_operation_ids,
                    &create_indexes,
                    operation,
                    index,
                    "document",
                    &document_id,
                );
                if let Some(template_id) = payload_string(payload, "workflow_template_id") {
                    add_later_create_dependency_blocker(
                        blockers,
                        blocked_operation_ids,
                        &create_indexes,
                        operation,
                        index,
                        "workflow_template",
                        &template_id,
                    );
                }
            }
            (
                "document_workflow_event",
                "transition" | "enable" | "disable" | "delegate" | "escalate",
            ) => {
                if let Some(document_id) = payload_string(payload, "document_id") {
                    add_later_create_dependency_blocker(
                        blockers,
                        blocked_operation_ids,
                        &create_indexes,
                        operation,
                        index,
                        "document",
                        &document_id,
                    );
                }
                if let Some(workflow_id) = payload_string(payload, "document_workflow_id")
                    .or_else(|| payload_string(payload, "workflow_id"))
                {
                    add_later_create_dependency_blocker(
                        blockers,
                        blocked_operation_ids,
                        &create_indexes,
                        operation,
                        index,
                        "document_workflow",
                        &workflow_id,
                    );
                }
            }
            ("asset", "create" | "delete") => {
                if let Some(document_id) = payload_string(payload, "document_id") {
                    add_later_create_dependency_blocker(
                        blockers,
                        blocked_operation_ids,
                        &create_indexes,
                        operation,
                        index,
                        "document",
                        &document_id,
                    );
                }
            }
            _ => {}
        }
    }
}

fn replay_create_indexes(operations: &[SyncReplayOperation]) -> HashMap<(String, String), usize> {
    operations
        .iter()
        .enumerate()
        .filter(|(_, operation)| operation.action == "create")
        .map(|(index, operation)| {
            (
                (operation.entity_type.clone(), operation.entity_id.clone()),
                index,
            )
        })
        .collect()
}

fn add_later_create_dependency_blocker(
    blockers: &mut Vec<String>,
    blocked_operation_ids: &mut HashSet<String>,
    create_indexes: &HashMap<(String, String), usize>,
    operation: &SyncReplayOperation,
    operation_index: usize,
    dependency_type: &str,
    dependency_id: &str,
) {
    if dependency_id.trim().is_empty() {
        return;
    }

    let dependency_key = (dependency_type.to_string(), dependency_id.to_string());
    let Some(dependency_index) = create_indexes.get(&dependency_key) else {
        return;
    };
    if *dependency_index <= operation_index {
        return;
    }

    blocked_operation_ids.insert(operation.id.clone());
    blockers.push(format!(
        "Replay operation {} depends on later {} create {}",
        operation.id, dependency_type, dependency_id
    ));
}

async fn execute_replay_operation(
    state: &AppState,
    auth: &AuthUser,
    manifest: &SyncReplayManifestRequest,
    operation: &SyncReplayOperation,
) -> SyncReplayOperationResult {
    match existing_replay_status(state, &operation.id).await {
        Ok(Some(status)) => {
            return replay_result(
                operation,
                "skipped",
                format!("Replay operation already recorded with status {status}"),
            );
        }
        Ok(None) => {}
        Err(error) => return replay_result(operation, "failed", error),
    }

    if let Err(error) =
        insert_replay_ledger(state, auth, manifest, operation, "pending", None).await
    {
        return replay_result(operation, "failed", error);
    }

    let result = match replay_operation_support(operation) {
        ReplayOperationSupport::SpaceCreate => replay_space_create(state, auth, operation).await,
        ReplayOperationSupport::SpaceUpdate => replay_space_update(state, auth, operation).await,
        ReplayOperationSupport::DocumentCreate => {
            replay_document_create(state, auth, operation).await
        }
        ReplayOperationSupport::DocumentUpdate => {
            replay_document_update(state, auth, operation).await
        }
        ReplayOperationSupport::DocumentUpdateContent => {
            replay_document_update_content(state, auth, operation).await
        }
        ReplayOperationSupport::DocumentResolveConflict => {
            replay_document_update_content(state, auth, operation).await
        }
        ReplayOperationSupport::DocumentDelete => {
            replay_document_delete(state, auth, operation).await
        }
        ReplayOperationSupport::FolderCreate => replay_folder_create(state, auth, operation).await,
        ReplayOperationSupport::FolderUpdate => replay_folder_update(state, auth, operation).await,
        ReplayOperationSupport::FolderDelete => replay_folder_delete(state, auth, operation).await,
        ReplayOperationSupport::TagCreate => replay_tag_create(state, auth, operation).await,
        ReplayOperationSupport::TagUpdate => replay_tag_update(state, auth, operation).await,
        ReplayOperationSupport::TagDelete => replay_tag_delete(state, auth, operation).await,
        ReplayOperationSupport::DocumentTagAdd => {
            replay_document_tag_add(state, auth, operation).await
        }
        ReplayOperationSupport::DocumentTagRemove => {
            replay_document_tag_remove(state, auth, operation).await
        }
        ReplayOperationSupport::TemplateCreate => {
            replay_template_create(state, auth, operation).await
        }
        ReplayOperationSupport::TemplateUpdate => {
            replay_template_update(state, auth, operation).await
        }
        ReplayOperationSupport::TemplateDelete => {
            replay_template_delete(state, auth, operation).await
        }
        ReplayOperationSupport::PropertySchemaCreate => {
            replay_property_schema_create(state, auth, operation).await
        }
        ReplayOperationSupport::PropertySchemaUpdate => {
            replay_property_schema_update(state, auth, operation).await
        }
        ReplayOperationSupport::PropertySchemaDelete => {
            replay_property_schema_delete(state, auth, operation).await
        }
        ReplayOperationSupport::PropertySchemaReorder => {
            replay_property_schema_reorder(state, auth, operation).await
        }
        ReplayOperationSupport::DocumentPropertyValuesSet => {
            replay_document_property_values_set(state, auth, operation).await
        }
        ReplayOperationSupport::WorkflowTemplateCreate => {
            replay_workflow_template_create(state, auth, operation).await
        }
        ReplayOperationSupport::WorkflowTemplateUpdate => {
            replay_workflow_template_update(state, auth, operation).await
        }
        ReplayOperationSupport::WorkflowTemplateDelete => {
            replay_workflow_template_delete(state, auth, operation).await
        }
        ReplayOperationSupport::DocumentWorkflowCreate
        | ReplayOperationSupport::DocumentWorkflowUpdate => {
            replay_document_workflow_upsert(state, auth, operation).await
        }
        ReplayOperationSupport::DocumentWorkflowDelete => {
            replay_document_workflow_delete(state, auth, operation).await
        }
        ReplayOperationSupport::DocumentWorkflowTransition => {
            replay_document_workflow_transition(state, auth, operation).await
        }
        ReplayOperationSupport::DocumentWorkflowEvent => {
            replay_document_workflow_event(state, auth, operation).await
        }
        ReplayOperationSupport::AssetCreate => replay_asset_create(state, auth, operation).await,
        ReplayOperationSupport::AssetDelete => replay_asset_delete(state, auth, operation).await,
        ReplayOperationSupport::Unsupported => Err(format!(
            "Unsupported replay operation {}:{}",
            operation.entity_type, operation.action
        )),
    };

    match result {
        Ok(status) => {
            let message = if status == "skipped" {
                "Replay operation was already reflected on the server".to_string()
            } else {
                "Replay operation applied".to_string()
            };
            let _ = update_replay_ledger(state, &operation.id, &status, None).await;
            replay_result(operation, &status, message)
        }
        Err(error) if error.starts_with("Unsupported replay operation") => {
            let _ = update_replay_ledger(state, &operation.id, "unsupported", Some(&error)).await;
            replay_result(operation, "unsupported", error)
        }
        Err(error) => {
            let _ = update_replay_ledger(state, &operation.id, "failed", Some(&error)).await;
            replay_result(operation, "failed", error)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplayOperationSupport {
    SpaceCreate,
    SpaceUpdate,
    DocumentCreate,
    DocumentUpdate,
    DocumentUpdateContent,
    DocumentResolveConflict,
    DocumentDelete,
    FolderCreate,
    FolderUpdate,
    FolderDelete,
    TagCreate,
    TagUpdate,
    TagDelete,
    DocumentTagAdd,
    DocumentTagRemove,
    TemplateCreate,
    TemplateUpdate,
    TemplateDelete,
    PropertySchemaCreate,
    PropertySchemaUpdate,
    PropertySchemaDelete,
    PropertySchemaReorder,
    DocumentPropertyValuesSet,
    WorkflowTemplateCreate,
    WorkflowTemplateUpdate,
    WorkflowTemplateDelete,
    DocumentWorkflowCreate,
    DocumentWorkflowUpdate,
    DocumentWorkflowDelete,
    DocumentWorkflowTransition,
    DocumentWorkflowEvent,
    AssetCreate,
    AssetDelete,
    Unsupported,
}

fn replay_operation_support(operation: &SyncReplayOperation) -> ReplayOperationSupport {
    match (operation.entity_type.as_str(), operation.action.as_str()) {
        ("space", "create") => ReplayOperationSupport::SpaceCreate,
        ("space", "update") => ReplayOperationSupport::SpaceUpdate,
        ("document", "create") => ReplayOperationSupport::DocumentCreate,
        ("document", "update") => ReplayOperationSupport::DocumentUpdate,
        ("document", "update_content") => ReplayOperationSupport::DocumentUpdateContent,
        ("document", action) if replay_conflict_resolution_strategy(action).is_some() => {
            ReplayOperationSupport::DocumentResolveConflict
        }
        ("document", "delete") => ReplayOperationSupport::DocumentDelete,
        ("folder", "create") => ReplayOperationSupport::FolderCreate,
        ("folder", "update") => ReplayOperationSupport::FolderUpdate,
        ("folder", "delete") => ReplayOperationSupport::FolderDelete,
        ("tag", "create") => ReplayOperationSupport::TagCreate,
        ("tag", "update") => ReplayOperationSupport::TagUpdate,
        ("tag", "delete") => ReplayOperationSupport::TagDelete,
        ("document_tag", "add") => ReplayOperationSupport::DocumentTagAdd,
        ("document_tag", "remove") => ReplayOperationSupport::DocumentTagRemove,
        ("template", "create") => ReplayOperationSupport::TemplateCreate,
        ("template", "update") => ReplayOperationSupport::TemplateUpdate,
        ("template", "delete") => ReplayOperationSupport::TemplateDelete,
        ("property_schema", "create") => ReplayOperationSupport::PropertySchemaCreate,
        ("property_schema", "update") => ReplayOperationSupport::PropertySchemaUpdate,
        ("property_schema", "delete") => ReplayOperationSupport::PropertySchemaDelete,
        ("property_schema", "reorder") => ReplayOperationSupport::PropertySchemaReorder,
        ("document_property_values", "set") => ReplayOperationSupport::DocumentPropertyValuesSet,
        ("workflow_template", "create") => ReplayOperationSupport::WorkflowTemplateCreate,
        ("workflow_template", "update") => ReplayOperationSupport::WorkflowTemplateUpdate,
        ("workflow_template", "delete") => ReplayOperationSupport::WorkflowTemplateDelete,
        ("document_workflow", "create") => ReplayOperationSupport::DocumentWorkflowCreate,
        ("document_workflow", "update") => ReplayOperationSupport::DocumentWorkflowUpdate,
        ("document_workflow", "delete") => ReplayOperationSupport::DocumentWorkflowDelete,
        ("document_workflow", "transition") => ReplayOperationSupport::DocumentWorkflowTransition,
        (
            "document_workflow_event",
            "transition" | "enable" | "disable" | "delegate" | "escalate",
        ) => ReplayOperationSupport::DocumentWorkflowEvent,
        ("asset", "create") => ReplayOperationSupport::AssetCreate,
        ("asset", "delete") => ReplayOperationSupport::AssetDelete,
        _ => ReplayOperationSupport::Unsupported,
    }
}

fn replay_conflict_resolution_strategy(action: &str) -> Option<&str> {
    action
        .strip_prefix("resolve_conflict:")
        .filter(|strategy| matches!(*strategy, "merge" | "keep-local" | "keep-remote"))
}

async fn replay_space_create(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    require_replay_admin(auth)?;

    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Space create payload must be an object".to_string())?;
    let space_id = replay_payload_entity_id(operation, payload, "Space")?;
    crate::spaces::routes::import_id_or_uuid(Some(&space_id))
        .map_err(|error| format!("{error:?}"))?;

    let name =
        payload_string(payload, "name").ok_or_else(|| "Space payload missing name".to_string())?;
    validate_replay_name(&name, "Space name", 100)?;

    let description = payload_nullable_string(payload, "description");
    if let Some(ref value) = description {
        if value.len() > 1000 {
            return Err("Space description must be at most 1000 characters".to_string());
        }
    }

    let slug = crate::storage::sanitize_slug(&name);
    if slug.is_empty() {
        return Err("Name must contain alphanumeric characters".to_string());
    }

    let existing_slug: Option<(String,)> = sqlx::query_as("SELECT id FROM spaces WHERE slug = ?1")
        .bind(&slug)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| error.to_string())?;
    if existing_slug.is_some() {
        return Err("A space with this name already exists".to_string());
    }

    let existing_id: Option<(String,)> = sqlx::query_as("SELECT id FROM spaces WHERE id = ?1")
        .bind(&space_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| error.to_string())?;
    if existing_id.is_some() {
        return Err("A space with this import id already exists".to_string());
    }

    let created_at = replay_timestamp(payload, "created_at", None)?;
    let updated_at = replay_timestamp(payload, "updated_at", Some(&created_at))?;
    let visibility = payload_string(payload, "visibility").unwrap_or_else(|| "private".to_string());
    let icon = payload_nullable_string(payload, "icon");
    let accent_color = payload_nullable_string(payload, "accent_color");

    sqlx::query(
        "INSERT INTO spaces (id, name, slug, description, icon, accent_color, visibility, created_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(&space_id)
    .bind(&name)
    .bind(&slug)
    .bind(&description)
    .bind(&icon)
    .bind(&accent_color)
    .bind(&visibility)
    .bind(&auth.claims.sub)
    .bind(&created_at)
    .bind(&updated_at)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    sqlx::query(
        "INSERT INTO space_members (id, space_id, user_id, role, created_at) \
         VALUES (?1, ?2, ?3, 'owner', ?4)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&space_id)
    .bind(&auth.claims.sub)
    .bind(&created_at)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    crate::storage::init_space_dirs(&slug)
        .await
        .map_err(|error| format!("Failed to create space directory: {error}"))?;

    Ok("applied".to_string())
}

async fn replay_space_update(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Space update payload must be an object".to_string())?;
    let space_id = replay_payload_entity_id(operation, payload, "Space")?;

    require_replay_owner(state, auth, &space_id).await?;

    let name = payload_string(payload, "name");
    if let Some(ref value) = name {
        validate_replay_name(value, "Space name", 100)?;
    }

    let description = payload_nullable_string(payload, "description");
    if let Some(ref value) = description {
        if value.len() > 1000 {
            return Err("Space description must be at most 1000 characters".to_string());
        }
    }

    let icon = payload_nullable_string(payload, "icon");
    let accent_color = payload_nullable_string(payload, "accent_color");
    let visibility = payload_nullable_string(payload, "visibility");
    let has_update = name.is_some()
        || description.is_some()
        || icon.is_some()
        || accent_color.is_some()
        || visibility.is_some();
    if !has_update {
        return Err("Space update payload must include at least one editable field".to_string());
    }

    let updated_at = replay_timestamp(payload, "updated_at", None)?;
    let result = sqlx::query(
        "UPDATE spaces SET \
         name = COALESCE(?1, name), \
         description = COALESCE(?2, description), \
         icon = COALESCE(?3, icon), \
         accent_color = COALESCE(?4, accent_color), \
         visibility = COALESCE(?5, visibility), \
         updated_at = ?6 \
         WHERE id = ?7",
    )
    .bind(&name)
    .bind(&description)
    .bind(&icon)
    .bind(&accent_color)
    .bind(&visibility)
    .bind(&updated_at)
    .bind(&space_id)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    if result.rows_affected() == 0 {
        return Err("Space not found".to_string());
    }

    Ok("applied".to_string())
}

async fn replay_document_create(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Document create payload must be an object".to_string())?;
    let doc_id = replay_payload_entity_id(operation, payload, "Document")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Document payload missing space_id".to_string())?;
    let title = payload_string(payload, "title")
        .ok_or_else(|| "Document payload missing title".to_string())?;
    let content = payload_string(payload, "content").unwrap_or_default();
    if title.len() > 500 {
        return Err("Document title must be at most 500 characters".to_string());
    }
    if content.len() > 5_242_880 {
        return Err("Document content must be at most 5 MB".to_string());
    }

    crate::spaces::routes::require_space_role(
        state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor"],
    )
    .await
    .map_err(|error| format!("{error:?}"))?;

    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM documents WHERE id = ?1")
        .bind(&doc_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| error.to_string())?;
    if existing.is_some() {
        return Ok("skipped".to_string());
    }

    let slug: (String,) = sqlx::query_as("SELECT slug FROM spaces WHERE id = ?1")
        .bind(&space_id)
        .fetch_one(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    let icon = payload_string(payload, "icon");
    let folder_id = payload_string(payload, "folder_id");
    validate_replay_document_folder_depth(state, &space_id, folder_id.as_deref()).await?;
    let created_at = crate::spaces::routes::import_timestamp_or_now(
        payload_string(payload, "created_at").as_deref(),
    )
    .map_err(|error| format!("{error:?}"))?;
    let updated_at = crate::spaces::routes::import_timestamp_or_default(
        payload_string(payload, "updated_at").as_deref(),
        &created_at,
    )
    .map_err(|error| format!("{error:?}"))?;
    let frontmatter = format!(
        "title: \"{}\"\nicon: \"{}\"\nlocked: false\npublic: false",
        title,
        icon.as_deref().unwrap_or("")
    );
    let content_path = crate::storage::write_document(&slug.0, &doc_id, &frontmatter, &content)
        .await
        .map_err(|error| format!("Failed to write document: {error}"))?;

    sqlx::query(
        "INSERT INTO documents (id, space_id, folder_id, title, icon, content_path, created_by, last_updated_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8, ?9)",
    )
    .bind(&doc_id)
    .bind(&space_id)
    .bind(&folder_id)
    .bind(&title)
    .bind(&icon)
    .bind(&content_path)
    .bind(&auth.claims.sub)
    .bind(&created_at)
    .bind(&updated_at)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    subscribe_user_to_document(state, &doc_id, &auth.claims.sub).await?;
    let _ = state.search_index.upsert(crate::search::index::DocEntry {
        doc_id: doc_id.clone(),
        space_id: space_id.clone(),
        title,
        body: content,
        tags: String::new(),
    });

    Ok("applied".to_string())
}

async fn replay_document_update_content(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Document update_content payload must be an object".to_string())?;
    let doc_id = replay_payload_entity_id(operation, payload, "Document")?;
    let content = payload_string(payload, "content")
        .ok_or_else(|| "Document payload missing content".to_string())?;
    if content.len() > 5_242_880 {
        return Err("Document content must be at most 5 MB".to_string());
    }

    let doc: (String, String, String, i64, Option<String>) = sqlx::query_as(
        "SELECT content_path, title, space_id, locked, locked_by FROM documents WHERE id = ?1",
    )
    .bind(&doc_id)
    .fetch_one(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    crate::spaces::routes::require_space_role(
        state,
        &doc.2,
        &auth.claims.sub,
        &["owner", "editor"],
    )
    .await
    .map_err(|error| format!("{error:?}"))?;
    if doc.3 != 0 && doc.4.as_deref() != Some(auth.claims.sub.as_str()) {
        return Err("Document is locked by another user".to_string());
    }

    let full_path = crate::storage::data_dir().join(&doc.0);
    tokio::fs::write(&full_path, &content)
        .await
        .map_err(|error| format!("Failed to write document: {error}"))?;

    let updated_at = crate::spaces::routes::import_timestamp_or_now(
        payload_string(payload, "updated_at").as_deref(),
    )
    .map_err(|error| format!("{error:?}"))?;
    sqlx::query("UPDATE documents SET last_updated_by = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(&auth.claims.sub)
        .bind(&updated_at)
        .bind(&doc_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    let _ = state.search_index.upsert(crate::search::index::DocEntry {
        doc_id,
        space_id: doc.2,
        title: doc.1,
        body: content,
        tags: String::new(),
    });

    Ok("applied".to_string())
}

async fn replay_document_update(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Document update payload must be an object".to_string())?;
    let doc_id = replay_payload_entity_id(operation, payload, "Document")?;

    let doc: (
        String,
        String,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = sqlx::query_as(
        "SELECT content_path, title, space_id, locked, locked_by, folder_id, icon, cover_image \
         FROM documents WHERE id = ?1",
    )
    .bind(&doc_id)
    .fetch_one(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    if let Some(payload_space_id) = payload_string(payload, "space_id") {
        if payload_space_id != doc.2 {
            return Err("Document update payload space does not match server document".to_string());
        }
    }

    require_replay_editor(state, auth, &doc.2).await?;
    if doc.3 != 0 && doc.4.as_deref() != Some(auth.claims.sub.as_str()) {
        return Err("Document is locked by another user".to_string());
    }

    let title = payload_string(payload, "title").unwrap_or(doc.1);
    if title.len() > 500 {
        return Err("Document title must be at most 500 characters".to_string());
    }

    let folder_id = match payload.get("folder_id") {
        Some(Value::Null) => None,
        Some(Value::String(value)) if value.trim().is_empty() => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => return Err("Document update payload folder is invalid".to_string()),
        None => doc.5,
    };
    validate_replay_document_folder_depth(state, &doc.2, folder_id.as_deref()).await?;

    let icon = match payload.get("icon") {
        Some(Value::Null) => None,
        Some(Value::String(value)) if value.trim().is_empty() => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => return Err("Document update payload icon is invalid".to_string()),
        None => doc.6,
    };
    let cover_image = match payload.get("cover_image") {
        Some(Value::Null) => None,
        Some(Value::String(value)) if value.trim().is_empty() => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => return Err("Document update payload cover image is invalid".to_string()),
        None => doc.7,
    };
    let updated_at = crate::spaces::routes::import_timestamp_or_now(
        payload_string(payload, "updated_at").as_deref(),
    )
    .map_err(|error| format!("{error:?}"))?;

    sqlx::query(
        "UPDATE documents SET title = ?1, folder_id = ?2, icon = ?3, cover_image = ?4, \
         last_updated_by = ?5, updated_at = ?6 WHERE id = ?7",
    )
    .bind(&title)
    .bind(&folder_id)
    .bind(&icon)
    .bind(&cover_image)
    .bind(&auth.claims.sub)
    .bind(&updated_at)
    .bind(&doc_id)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    let body = crate::storage::read_document(&doc.0)
        .await
        .unwrap_or_default();
    let _ = state.search_index.upsert(crate::search::index::DocEntry {
        doc_id,
        space_id: doc.2,
        title,
        body,
        tags: String::new(),
    });

    Ok("applied".to_string())
}

async fn replay_document_delete(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation.payload.as_object();
    let doc_id = match payload {
        Some(payload) => replay_payload_entity_id(operation, payload, "Document")?,
        None => operation.entity_id.clone(),
    };

    let existing: Option<(String, String)> =
        sqlx::query_as("SELECT content_path, space_id FROM documents WHERE id = ?1")
            .bind(&doc_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;

    let Some((content_path, space_id)) = existing else {
        return Ok("skipped".to_string());
    };

    if let Some(payload) = payload {
        if let Some(payload_space_id) = payload_string(payload, "space_id") {
            if payload_space_id != space_id {
                return Err(
                    "Document delete payload space does not match server document".to_string(),
                );
            }
        }
    }

    require_replay_editor(state, auth, &space_id).await?;

    let full_path = crate::storage::data_dir().join(&content_path);
    tokio::fs::remove_file(full_path).await.ok();

    sqlx::query("DELETE FROM documents WHERE id = ?1")
        .bind(&doc_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    let _ = state.search_index.delete(&doc_id);

    Ok("applied".to_string())
}

async fn replay_folder_create(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Folder create payload must be an object".to_string())?;
    let folder_id = replay_payload_entity_id(operation, payload, "Folder")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Folder payload missing space_id".to_string())?;
    let name =
        payload_string(payload, "name").ok_or_else(|| "Folder payload missing name".to_string())?;
    validate_replay_name(&name, "Folder name", 100)?;

    require_replay_editor(state, auth, &space_id).await?;
    if replay_record_exists(state, "folders", &folder_id).await? {
        return Ok("skipped".to_string());
    }

    let parent_id = payload_string(payload, "parent_id");
    validate_replay_new_folder_parent(state, &space_id, parent_id.as_deref()).await?;
    let icon = payload_string(payload, "icon");
    let created_at = replay_timestamp(payload, "created_at", None)?;
    let updated_at = replay_timestamp(payload, "updated_at", Some(&created_at))?;
    let sort_order = next_folder_sort_order(state, &space_id, parent_id.as_deref()).await?;

    sqlx::query(
        "INSERT INTO folders (id, space_id, parent_id, name, icon, sort_order, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&folder_id)
    .bind(&space_id)
    .bind(&parent_id)
    .bind(&name)
    .bind(&icon)
    .bind(sort_order)
    .bind(&created_at)
    .bind(&updated_at)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_folder_update(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Folder update payload must be an object".to_string())?;
    let folder_id = replay_payload_entity_id(operation, payload, "Folder")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Folder payload missing space_id".to_string())?;
    let name =
        payload_string(payload, "name").ok_or_else(|| "Folder payload missing name".to_string())?;
    validate_replay_name(&name, "Folder name", 100)?;

    require_replay_editor(state, auth, &space_id).await?;
    if !replay_record_exists(state, "folders", &folder_id).await? {
        return Err("Folder not found on server".to_string());
    }

    let parent_id = payload_string(payload, "parent_id");
    validate_replay_moved_folder_parent(state, &space_id, &folder_id, parent_id.as_deref()).await?;
    let icon = payload_string(payload, "icon");
    let updated_at = replay_timestamp(payload, "updated_at", None)?;
    sqlx::query(
        "UPDATE folders SET name = ?1, parent_id = ?2, icon = ?3, updated_at = ?4 WHERE id = ?5 AND space_id = ?6",
    )
    .bind(&name)
    .bind(&parent_id)
    .bind(&icon)
    .bind(&updated_at)
    .bind(&folder_id)
    .bind(&space_id)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_folder_delete(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Folder delete payload must be an object".to_string())?;
    let folder_id = replay_payload_entity_id(operation, payload, "Folder")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Folder payload missing space_id".to_string())?;

    require_replay_editor(state, auth, &space_id).await?;
    if !replay_record_exists(state, "folders", &folder_id).await? {
        return Ok("skipped".to_string());
    }

    sqlx::query("DELETE FROM folders WHERE id = ?1 AND space_id = ?2")
        .bind(&folder_id)
        .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_tag_create(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Tag create payload must be an object".to_string())?;
    let tag_id = replay_payload_entity_id(operation, payload, "Tag")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Tag payload missing space_id".to_string())?;
    let name =
        payload_string(payload, "name").ok_or_else(|| "Tag payload missing name".to_string())?;
    validate_replay_name(&name, "Tag name", 100)?;

    require_replay_editor(state, auth, &space_id).await?;
    if replay_record_exists(state, "tags", &tag_id).await? {
        return Ok("skipped".to_string());
    }

    let color = payload_string(payload, "color").unwrap_or_else(|| "#6b7280".to_string());
    let created_at = replay_timestamp(payload, "created_at", None)?;
    sqlx::query(
        "INSERT INTO tags (id, space_id, name, color, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&tag_id)
    .bind(&space_id)
    .bind(&name)
    .bind(&color)
    .bind(&created_at)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_tag_update(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Tag update payload must be an object".to_string())?;
    let tag_id = replay_payload_entity_id(operation, payload, "Tag")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Tag payload missing space_id".to_string())?;
    let name =
        payload_string(payload, "name").ok_or_else(|| "Tag payload missing name".to_string())?;
    validate_replay_name(&name, "Tag name", 100)?;

    require_replay_editor(state, auth, &space_id).await?;
    if !replay_record_exists(state, "tags", &tag_id).await? {
        return Err("Tag not found on server".to_string());
    }

    let color = payload_string(payload, "color").unwrap_or_else(|| "#6b7280".to_string());
    sqlx::query("UPDATE tags SET name = ?1, color = ?2 WHERE id = ?3 AND space_id = ?4")
        .bind(&name)
        .bind(&color)
        .bind(&tag_id)
        .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_tag_delete(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Tag delete payload must be an object".to_string())?;
    let tag_id = replay_payload_entity_id(operation, payload, "Tag")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Tag payload missing space_id".to_string())?;

    require_replay_editor(state, auth, &space_id).await?;
    if !replay_record_exists(state, "tags", &tag_id).await? {
        return Ok("skipped".to_string());
    }

    sqlx::query("DELETE FROM tags WHERE id = ?1 AND space_id = ?2")
        .bind(&tag_id)
        .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_document_tag_add(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Document tag add payload must be an object".to_string())?;
    let document_id = payload_string(payload, "document_id")
        .ok_or_else(|| "Document tag payload missing document_id".to_string())?;
    let tag_id = payload_string(payload, "tag_id")
        .ok_or_else(|| "Document tag payload missing tag_id".to_string())?;
    let doc: (String,) = sqlx::query_as("SELECT space_id FROM documents WHERE id = ?1")
        .bind(&document_id)
        .fetch_one(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    require_replay_editor(state, auth, &doc.0).await?;
    let tag: Option<(String,)> =
        sqlx::query_as("SELECT id FROM tags WHERE id = ?1 AND space_id = ?2")
            .bind(&tag_id)
            .bind(&doc.0)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    if tag.is_none() {
        return Err("Tag not found on server".to_string());
    }

    let existing: Option<(String,)> =
        sqlx::query_as("SELECT document_id FROM doc_tags WHERE document_id = ?1 AND tag_id = ?2")
            .bind(&document_id)
            .bind(&tag_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    if existing.is_some() {
        return Ok("skipped".to_string());
    }

    sqlx::query("INSERT INTO doc_tags (document_id, tag_id) VALUES (?1, ?2)")
        .bind(&document_id)
        .bind(&tag_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_document_tag_remove(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Document tag remove payload must be an object".to_string())?;
    let document_id = payload_string(payload, "document_id")
        .ok_or_else(|| "Document tag payload missing document_id".to_string())?;
    let tag_id = payload_string(payload, "tag_id")
        .ok_or_else(|| "Document tag payload missing tag_id".to_string())?;
    let space_id = match payload_string(payload, "space_id") {
        Some(space_id) => space_id,
        None => {
            let doc: (String,) = sqlx::query_as("SELECT space_id FROM documents WHERE id = ?1")
                .bind(&document_id)
                .fetch_one(&state.db)
                .await
                .map_err(|error| error.to_string())?;
            doc.0
        }
    };

    require_replay_editor(state, auth, &space_id).await?;
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT document_id FROM doc_tags WHERE document_id = ?1 AND tag_id = ?2")
            .bind(&document_id)
            .bind(&tag_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    if existing.is_none() {
        return Ok("skipped".to_string());
    }

    sqlx::query("DELETE FROM doc_tags WHERE document_id = ?1 AND tag_id = ?2")
        .bind(&document_id)
        .bind(&tag_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_template_create(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Template create payload must be an object".to_string())?;
    let template_id = replay_payload_entity_id(operation, payload, "Template")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Template payload missing space_id".to_string())?;
    let name = payload_string(payload, "name")
        .ok_or_else(|| "Template payload missing name".to_string())?;
    let content = payload_string(payload, "content")
        .ok_or_else(|| "Template payload missing content".to_string())?;
    validate_template_replay_payload(
        &name,
        payload_string(payload, "description").as_deref(),
        &content,
    )?;

    require_replay_editor(state, auth, &space_id).await?;
    if replay_record_exists(state, "templates", &template_id).await? {
        return Ok("skipped".to_string());
    }

    let properties = payload_json_string(payload, "properties")?;
    let tags = payload_json_array_string(payload, "tags")?;
    let created_at = replay_timestamp(payload, "created_at", None)?;
    let updated_at = replay_timestamp(payload, "updated_at", Some(&created_at))?;
    sqlx::query(
        "INSERT INTO templates (id, space_id, name, description, content, properties, tags, created_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(&template_id)
    .bind(&space_id)
    .bind(name.trim())
    .bind(payload_string(payload, "description"))
    .bind(&content)
    .bind(properties)
    .bind(tags)
    .bind(&auth.claims.sub)
    .bind(&created_at)
    .bind(&updated_at)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_template_update(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Template update payload must be an object".to_string())?;
    let template_id = replay_payload_entity_id(operation, payload, "Template")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Template payload missing space_id".to_string())?;
    let name = payload_string(payload, "name")
        .ok_or_else(|| "Template payload missing name".to_string())?;
    let content = payload_string(payload, "content")
        .ok_or_else(|| "Template payload missing content".to_string())?;
    validate_template_replay_payload(
        &name,
        payload_string(payload, "description").as_deref(),
        &content,
    )?;

    require_replay_editor(state, auth, &space_id).await?;
    if !replay_record_exists(state, "templates", &template_id).await? {
        return Err("Template not found on server".to_string());
    }

    let properties = payload_json_string(payload, "properties")?;
    let tags = payload_json_array_string(payload, "tags")?;
    let updated_at = replay_timestamp(payload, "updated_at", None)?;
    sqlx::query(
        "UPDATE templates SET name = ?1, description = ?2, content = ?3, properties = ?4, tags = ?5, updated_at = ?6 \
         WHERE id = ?7 AND space_id = ?8",
    )
    .bind(name.trim())
    .bind(payload_string(payload, "description"))
    .bind(&content)
    .bind(properties)
    .bind(tags)
    .bind(&updated_at)
    .bind(&template_id)
    .bind(&space_id)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_template_delete(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Template delete payload must be an object".to_string())?;
    let template_id = replay_payload_entity_id(operation, payload, "Template")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Template payload missing space_id".to_string())?;

    require_replay_editor(state, auth, &space_id).await?;
    if !replay_record_exists(state, "templates", &template_id).await? {
        return Ok("skipped".to_string());
    }

    sqlx::query("DELETE FROM templates WHERE id = ?1 AND space_id = ?2")
        .bind(&template_id)
        .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_property_schema_create(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Property schema create payload must be an object".to_string())?;
    let property_id = replay_payload_entity_id(operation, payload, "Property schema")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Property schema payload missing space_id".to_string())?;
    let name = payload_string(payload, "name")
        .ok_or_else(|| "Property schema payload missing name".to_string())?;
    let property_type = payload_string(payload, "property_type")
        .ok_or_else(|| "Property schema payload missing property_type".to_string())?;
    validate_property_schema_replay_payload(&name, &property_type)?;

    require_replay_editor(state, auth, &space_id).await?;
    if replay_record_exists(state, "properties_schema", &property_id).await? {
        return Ok("skipped".to_string());
    }

    let options = payload_json_string(payload, "options")?;
    let sort_order = payload_i64(payload, "sort_order")
        .map(|value| value as i32)
        .unwrap_or(next_property_sort_order(state, &space_id).await?);
    let created_at = replay_timestamp(payload, "created_at", None)?;
    sqlx::query(
        "INSERT INTO properties_schema (id, space_id, name, property_type, options, sort_order, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&property_id)
    .bind(&space_id)
    .bind(&name)
    .bind(&property_type)
    .bind(options)
    .bind(sort_order)
    .bind(&created_at)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_property_schema_update(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Property schema update payload must be an object".to_string())?;
    let property_id = replay_payload_entity_id(operation, payload, "Property schema")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Property schema payload missing space_id".to_string())?;
    let name = payload_string(payload, "name")
        .ok_or_else(|| "Property schema payload missing name".to_string())?;
    let property_type = payload_string(payload, "property_type")
        .ok_or_else(|| "Property schema payload missing property_type".to_string())?;
    validate_property_schema_replay_payload(&name, &property_type)?;

    require_replay_editor(state, auth, &space_id).await?;
    if !replay_record_exists(state, "properties_schema", &property_id).await? {
        return Err("Property schema not found on server".to_string());
    }

    let options = payload_json_string(payload, "options")?;
    let sort_order = payload_i64(payload, "sort_order").map(|value| value as i32);
    if let Some(sort_order) = sort_order {
        sqlx::query(
            "UPDATE properties_schema SET name = ?1, property_type = ?2, options = ?3, sort_order = ?4 \
             WHERE id = ?5 AND space_id = ?6",
        )
        .bind(&name)
        .bind(&property_type)
        .bind(options)
        .bind(sort_order)
        .bind(&property_id)
        .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;
    } else {
        sqlx::query(
            "UPDATE properties_schema SET name = ?1, property_type = ?2, options = ?3 \
             WHERE id = ?4 AND space_id = ?5",
        )
        .bind(&name)
        .bind(&property_type)
        .bind(options)
        .bind(&property_id)
        .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;
    }

    Ok("applied".to_string())
}

async fn replay_property_schema_delete(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Property schema delete payload must be an object".to_string())?;
    let property_id = replay_payload_entity_id(operation, payload, "Property schema")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Property schema payload missing space_id".to_string())?;

    require_replay_owner(state, auth, &space_id).await?;
    if !replay_record_exists(state, "properties_schema", &property_id).await? {
        return Ok("skipped".to_string());
    }

    sqlx::query("DELETE FROM properties_schema WHERE id = ?1 AND space_id = ?2")
        .bind(&property_id)
        .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_property_schema_reorder(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Property schema reorder payload must be an object".to_string())?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Property schema reorder payload missing space_id".to_string())?;
    let property_ids = payload
        .get("property_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| "Property schema reorder payload missing property_ids".to_string())?;

    require_replay_editor(state, auth, &space_id).await?;
    for (index, property_id) in property_ids.iter().enumerate() {
        let property_id = property_id
            .as_str()
            .ok_or_else(|| "Property schema reorder property_ids must be strings".to_string())?;
        sqlx::query("UPDATE properties_schema SET sort_order = ?1 WHERE id = ?2 AND space_id = ?3")
            .bind((index + 1) as i32)
            .bind(property_id)
            .bind(&space_id)
            .execute(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    }

    Ok("applied".to_string())
}

async fn replay_document_property_values_set(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Document property values payload must be an object".to_string())?;
    let document_id =
        payload_string(payload, "document_id").unwrap_or_else(|| operation.entity_id.clone());
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Document property values payload missing space_id".to_string())?;
    let values = payload
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| "Document property values payload missing values".to_string())?;

    require_replay_editor(state, auth, &space_id).await?;
    ensure_document_in_space(state, &document_id, &space_id).await?;

    for value in values {
        let value_payload = value
            .as_object()
            .ok_or_else(|| "Document property value entries must be objects".to_string())?;
        let property_id = payload_string(value_payload, "property_id")
            .ok_or_else(|| "Document property value missing property_id".to_string())?;
        ensure_property_in_space(state, &property_id, &space_id).await?;
        let value_id =
            payload_string(value_payload, "id").unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let property_value = payload_nullable_string(value_payload, "value");
        let created_at = replay_timestamp(value_payload, "created_at", None)?;
        let updated_at = replay_timestamp(value_payload, "updated_at", Some(&created_at))?;
        upsert_property_value(
            state,
            &value_id,
            &document_id,
            &property_id,
            property_value.as_deref(),
            &created_at,
            &updated_at,
        )
        .await?;
    }

    Ok("applied".to_string())
}

async fn replay_workflow_template_create(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Workflow template create payload must be an object".to_string())?;
    let template_id = replay_payload_entity_id(operation, payload, "Workflow template")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Workflow template payload missing space_id".to_string())?;
    let name = payload_string(payload, "name")
        .ok_or_else(|| "Workflow template payload missing name".to_string())?;
    let description = payload_string(payload, "description");
    let (stages, approvers) = normalize_workflow_template_replay_payload(
        &name,
        description.as_deref(),
        payload.get("stages"),
        payload.get("approvers"),
    )?;

    require_replay_owner(state, auth, &space_id).await?;
    if replay_record_exists(state, "workflow_templates", &template_id).await? {
        return Ok("skipped".to_string());
    }

    let created_at = replay_timestamp(payload, "created_at", None)?;
    let updated_at = replay_timestamp(payload, "updated_at", Some(&created_at))?;
    sqlx::query(
        "INSERT INTO workflow_templates (id, space_id, name, description, stages, approvers, created_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(&template_id)
    .bind(&space_id)
    .bind(name.trim())
    .bind(description)
    .bind(serde_json::to_string(&stages).map_err(|error| error.to_string())?)
    .bind(serde_json::to_string(&approvers).map_err(|error| error.to_string())?)
    .bind(&auth.claims.sub)
    .bind(&created_at)
    .bind(&updated_at)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_workflow_template_update(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Workflow template update payload must be an object".to_string())?;
    let template_id = replay_payload_entity_id(operation, payload, "Workflow template")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Workflow template payload missing space_id".to_string())?;
    let name = payload_string(payload, "name")
        .ok_or_else(|| "Workflow template payload missing name".to_string())?;
    let description = payload_string(payload, "description");
    let (stages, approvers) = normalize_workflow_template_replay_payload(
        &name,
        description.as_deref(),
        payload.get("stages"),
        payload.get("approvers"),
    )?;

    require_replay_owner(state, auth, &space_id).await?;
    if !replay_record_exists(state, "workflow_templates", &template_id).await? {
        return Err("Workflow template not found on server".to_string());
    }

    let updated_at = replay_timestamp(payload, "updated_at", None)?;
    sqlx::query(
        "UPDATE workflow_templates SET name = ?1, description = ?2, stages = ?3, approvers = ?4, updated_at = ?5 \
         WHERE id = ?6 AND space_id = ?7",
    )
    .bind(name.trim())
    .bind(description)
    .bind(serde_json::to_string(&stages).map_err(|error| error.to_string())?)
    .bind(serde_json::to_string(&approvers).map_err(|error| error.to_string())?)
    .bind(&updated_at)
    .bind(&template_id)
    .bind(&space_id)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_workflow_template_delete(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Workflow template delete payload must be an object".to_string())?;
    let template_id = replay_payload_entity_id(operation, payload, "Workflow template")?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Workflow template payload missing space_id".to_string())?;

    require_replay_owner(state, auth, &space_id).await?;
    if !replay_record_exists(state, "workflow_templates", &template_id).await? {
        return Ok("skipped".to_string());
    }

    sqlx::query("DELETE FROM workflow_templates WHERE id = ?1 AND space_id = ?2")
        .bind(&template_id)
        .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_document_workflow_upsert(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Document workflow payload must be an object".to_string())?;
    let workflow_id = payload_string(payload, "id").unwrap_or_else(|| operation.entity_id.clone());
    let document_id = payload_string(payload, "document_id")
        .ok_or_else(|| "Document workflow payload missing document_id".to_string())?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Document workflow payload missing space_id".to_string())?;
    let template_id = payload_string(payload, "workflow_template_id")
        .ok_or_else(|| "Document workflow payload missing workflow_template_id".to_string())?;
    let stage_id = payload_string(payload, "current_stage_id")
        .ok_or_else(|| "Document workflow payload missing current_stage_id".to_string())?;
    let status = payload_string(payload, "status").unwrap_or_else(|| "active".to_string());
    validate_document_workflow_status(&status)?;

    require_replay_editor(state, auth, &space_id).await?;
    ensure_document_in_space(state, &document_id, &space_id).await?;
    ensure_workflow_template_stage_in_space(state, &template_id, &stage_id, &space_id).await?;

    let created_at = replay_timestamp(payload, "created_at", None)?;
    let updated_at = replay_timestamp(payload, "updated_at", Some(&created_at))?;
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM document_workflows WHERE document_id = ?1")
            .bind(&document_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;

    if existing.is_some() {
        sqlx::query(
            "UPDATE document_workflows SET workflow_template_id = ?1, current_stage_id = ?2, status = ?3, updated_at = ?4 \
             WHERE document_id = ?5",
        )
        .bind(&template_id)
        .bind(&stage_id)
        .bind(&status)
        .bind(&updated_at)
        .bind(&document_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;
    } else {
        sqlx::query(
            "INSERT INTO document_workflows (id, document_id, workflow_template_id, current_stage_id, status, created_by, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(&workflow_id)
        .bind(&document_id)
        .bind(&template_id)
        .bind(&stage_id)
        .bind(&status)
        .bind(&auth.claims.sub)
        .bind(&created_at)
        .bind(&updated_at)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;
    }

    if let Some(event) = payload.get("event").and_then(Value::as_object) {
        replay_document_workflow_event_insert(state, auth, event, &workflow_id, &document_id)
            .await?;
    }

    Ok("applied".to_string())
}

async fn replay_document_workflow_delete(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Document workflow delete payload must be an object".to_string())?;
    let document_id =
        payload_string(payload, "document_id").unwrap_or_else(|| operation.entity_id.clone());
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Document workflow payload missing space_id".to_string())?;

    require_replay_editor(state, auth, &space_id).await?;
    ensure_document_in_space(state, &document_id, &space_id).await?;
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM document_workflows WHERE document_id = ?1")
            .bind(&document_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    if existing.is_none() {
        return Ok("skipped".to_string());
    }

    sqlx::query("DELETE FROM document_workflows WHERE document_id = ?1")
        .bind(&document_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn replay_document_workflow_transition(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Document workflow transition payload must be an object".to_string())?;
    let workflow_id = payload_string(payload, "id").unwrap_or_else(|| operation.entity_id.clone());
    let document_id = payload_string(payload, "document_id")
        .ok_or_else(|| "Document workflow transition payload missing document_id".to_string())?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Document workflow transition payload missing space_id".to_string())?;
    let template_id = payload_string(payload, "workflow_template_id").ok_or_else(|| {
        "Document workflow transition payload missing workflow_template_id".to_string()
    })?;
    let stage_id = payload_string(payload, "current_stage_id").ok_or_else(|| {
        "Document workflow transition payload missing current_stage_id".to_string()
    })?;

    require_replay_editor(state, auth, &space_id).await?;
    ensure_document_in_space(state, &document_id, &space_id).await?;
    ensure_workflow_template_stage_in_space(state, &template_id, &stage_id, &space_id).await?;

    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM document_workflows WHERE id = ?1 AND document_id = ?2")
            .bind(&workflow_id)
            .bind(&document_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    if existing.is_none() {
        return Err("Document workflow not found on server".to_string());
    }

    let status =
        if workflow_template_stage_is_final(state, &template_id, &stage_id, &space_id).await? {
            "completed"
        } else {
            "active"
        };
    let updated_at = replay_timestamp(payload, "updated_at", None)?;
    sqlx::query(
        "UPDATE document_workflows SET current_stage_id = ?1, status = ?2, updated_at = ?3 \
         WHERE id = ?4 AND document_id = ?5",
    )
    .bind(&stage_id)
    .bind(status)
    .bind(&updated_at)
    .bind(&workflow_id)
    .bind(&document_id)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    if let Some(event) = payload.get("event").and_then(Value::as_object) {
        replay_document_workflow_event_insert(state, auth, event, &workflow_id, &document_id)
            .await?;
    }

    Ok("applied".to_string())
}

async fn replay_document_workflow_event(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Document workflow event payload must be an object".to_string())?;
    let document_id = payload_string(payload, "document_id")
        .ok_or_else(|| "Document workflow event payload missing document_id".to_string())?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Document workflow event payload missing space_id".to_string())?;

    require_replay_editor(state, auth, &space_id).await?;
    ensure_document_in_space(state, &document_id, &space_id).await?;

    let workflow_id = payload_string(payload, "document_workflow_id")
        .or_else(|| payload_string(payload, "workflow_id"))
        .ok_or_else(|| {
            "Document workflow event payload missing document_workflow_id".to_string()
        })?;
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM document_workflows WHERE id = ?1 AND document_id = ?2")
            .bind(&workflow_id)
            .bind(&document_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    if existing.is_none() {
        return Err("Document workflow not found on server".to_string());
    }

    replay_document_workflow_event_insert(state, auth, payload, &workflow_id, &document_id).await?;
    Ok("applied".to_string())
}

async fn replay_asset_create(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Asset create payload must be an object".to_string())?;
    let asset_id = replay_payload_entity_id(operation, payload, "Asset")?;
    let document_id = payload_string(payload, "document_id")
        .ok_or_else(|| "Asset payload missing document_id".to_string())?;
    let space_id = payload_string(payload, "space_id")
        .ok_or_else(|| "Asset payload missing space_id".to_string())?;
    let original_name =
        payload_string(payload, "name").ok_or_else(|| "Asset payload missing name".to_string())?;
    let mime_type = payload_string(payload, "mime_type")
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let data_url = payload_string(payload, "data_url")
        .or_else(|| payload_string(payload, "url").filter(|value| value.starts_with("data:")))
        .or_else(|| {
            payload_string(payload, "source_url").filter(|value| value.starts_with("data:"))
        })
        .ok_or_else(|| "Asset replay binary data is unavailable".to_string())?;

    require_replay_editor(state, auth, &space_id).await?;
    ensure_document_in_space(state, &document_id, &space_id).await?;

    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM attachments WHERE id = ?1")
        .bind(&asset_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|error| error.to_string())?;
    if existing.is_some() {
        return Ok("skipped".to_string());
    }

    let bytes = decode_base64_data_url(&data_url)?;
    if bytes.is_empty() {
        return Err("Asset replay binary data is empty".to_string());
    }
    if bytes.len() > 100 * 1024 * 1024 {
        return Err("Asset replay binary data exceeds 100 MB".to_string());
    }

    let slug: (String,) = sqlx::query_as("SELECT slug FROM spaces WHERE id = ?1")
        .bind(&space_id)
        .fetch_one(&state.db)
        .await
        .map_err(|error| error.to_string())?;
    let filename = replay_asset_filename(&asset_id, &original_name);
    let dir = crate::storage::attachments_dir(&slug.0).join(&document_id);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|error| format!("Failed to create attachment directory: {error}"))?;
    let file_path = dir.join(&filename);
    tokio::fs::write(&file_path, &bytes)
        .await
        .map_err(|error| format!("Failed to write attachment: {error}"))?;

    let storage_path = format!("spaces/{}/attachments/{}/{}", slug.0, document_id, filename);
    sqlx::query(
        "INSERT INTO attachments (id, document_id, space_id, filename, original_name, mime_type, size_bytes, storage_path, created_by) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(&asset_id)
    .bind(&document_id)
    .bind(&space_id)
    .bind(&filename)
    .bind(&original_name)
    .bind(&mime_type)
    .bind(bytes.len() as i64)
    .bind(&storage_path)
    .bind(&auth.claims.sub)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    rewrite_document_asset_references(state, auth, payload, &document_id, &storage_path).await?;

    Ok("applied".to_string())
}

async fn replay_asset_delete(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation.payload.as_object();
    let asset_id = match payload {
        Some(payload) => replay_payload_entity_id(operation, payload, "Asset")?,
        None => operation.entity_id.clone(),
    };

    let existing: Option<(String, String, String)> =
        sqlx::query_as("SELECT document_id, space_id, storage_path FROM attachments WHERE id = ?1")
            .bind(&asset_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;

    let Some((document_id, space_id, storage_path)) = existing else {
        return Ok("skipped".to_string());
    };

    if let Some(payload) = payload {
        if let Some(payload_document_id) = payload_string(payload, "document_id") {
            if payload_document_id != document_id {
                return Err(
                    "Asset delete payload document does not match server attachment".to_string(),
                );
            }
        }
        if let Some(payload_space_id) = payload_string(payload, "space_id") {
            if payload_space_id != space_id {
                return Err(
                    "Asset delete payload space does not match server attachment".to_string(),
                );
            }
        }
    }

    require_replay_editor(state, auth, &space_id).await?;
    ensure_document_in_space(state, &document_id, &space_id).await?;

    let full_path = crate::storage::data_dir().join(&storage_path);
    tokio::fs::remove_file(full_path).await.ok();

    sqlx::query("DELETE FROM attachments WHERE id = ?1")
        .bind(&asset_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    Ok("applied".to_string())
}

async fn require_replay_editor(
    state: &AppState,
    auth: &AuthUser,
    space_id: &str,
) -> Result<(), String> {
    crate::spaces::routes::require_space_role(
        state,
        space_id,
        &auth.claims.sub,
        &["owner", "editor"],
    )
    .await
    .map(|_| ())
    .map_err(|error| format!("{error:?}"))
}

async fn require_replay_owner(
    state: &AppState,
    auth: &AuthUser,
    space_id: &str,
) -> Result<(), String> {
    crate::spaces::routes::require_space_role(state, space_id, &auth.claims.sub, &["owner"])
        .await
        .map(|_| ())
        .map_err(|error| format!("{error:?}"))
}

fn require_replay_admin(auth: &AuthUser) -> Result<(), String> {
    if auth.claims.role != "admin" {
        return Err("Replay operation requires admin role".to_string());
    }
    if auth
        .api_key_scopes
        .as_ref()
        .is_some_and(|scopes| !scopes.iter().any(|scope| scope == "admin"))
    {
        return Err("Replay operation requires admin API key scope".to_string());
    }
    Ok(())
}

fn validate_replay_name(name: &str, label: &str, max_len: usize) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err(format!("{label} is required"));
    }
    if name.len() > max_len {
        return Err(format!("{label} must be at most {max_len} characters"));
    }
    Ok(())
}

fn validate_property_schema_replay_payload(name: &str, property_type: &str) -> Result<(), String> {
    validate_replay_name(name, "Property name", 100)?;
    let valid_types = [
        "text",
        "long_text",
        "number",
        "date",
        "datetime",
        "checkbox",
        "select",
        "multi_select",
        "url",
        "user",
        "relation",
    ];
    if !valid_types.contains(&property_type) {
        return Err(format!("Invalid property type: {property_type}"));
    }
    Ok(())
}

fn validate_template_replay_payload(
    name: &str,
    description: Option<&str>,
    content: &str,
) -> Result<(), String> {
    validate_replay_name(name, "Template name", 100)?;
    if description.is_some_and(|value| value.len() > 1000) {
        return Err("Template description must be at most 1000 characters".to_string());
    }
    if content.len() > 5_242_880 {
        return Err("Template content must be at most 5 MB".to_string());
    }
    Ok(())
}

fn normalize_workflow_template_replay_payload(
    name: &str,
    description: Option<&str>,
    stages: Option<&Value>,
    approvers: Option<&Value>,
) -> Result<
    (
        Vec<crate::spaces::workflows::WorkflowStage>,
        Vec<crate::spaces::workflows::WorkflowApprover>,
    ),
    String,
> {
    validate_replay_name(name, "Workflow template name", 100)?;
    if description.is_some_and(|value| value.len() > 1000) {
        return Err("Workflow template description must be at most 1000 characters".to_string());
    }

    let stages_value =
        stages.ok_or_else(|| "Workflow template payload missing stages".to_string())?;
    let mut stages = serde_json::from_value::<Vec<crate::spaces::workflows::WorkflowStage>>(
        stages_value.clone(),
    )
    .map_err(|error| error.to_string())?
    .into_iter()
    .enumerate()
    .filter_map(|(index, stage)| {
        let stage_name = stage.name.trim();
        if stage_name.is_empty() {
            return None;
        }
        Some(crate::spaces::workflows::WorkflowStage {
            id: Some(
                stage
                    .id
                    .as_deref()
                    .filter(|id| !id.trim().is_empty())
                    .map(str::trim)
                    .map(str::to_string)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            ),
            name: stage_name.to_string(),
            sort_order: Some(stage.sort_order.unwrap_or(index as i64)),
        })
    })
    .collect::<Vec<_>>();

    if stages.len() < 2 {
        return Err("Workflow templates require at least two stages".to_string());
    }
    if stages.len() > 12 {
        return Err("Workflow templates can have at most 12 stages".to_string());
    }
    stages.sort_by_key(|stage| stage.sort_order.unwrap_or(0));

    let stage_ids = stages
        .iter()
        .filter_map(|stage| stage.id.as_deref())
        .collect::<HashSet<_>>();
    let approvers = match approvers {
        Some(Value::Null) | None => Vec::new(),
        Some(value) => {
            serde_json::from_value::<Vec<crate::spaces::workflows::WorkflowApprover>>(value.clone())
                .map_err(|error| error.to_string())?
        }
    }
    .into_iter()
    .map(|approver| {
        let approver_id = approver.approver_id.trim().to_string();
        if approver_id.is_empty() {
            return Err("Workflow approver is required".to_string());
        }
        if approver.approver_type != "user" && approver.approver_type != "role" {
            return Err("Workflow approver type must be user or role".to_string());
        }
        if let Some(stage_id) = approver.stage_id.as_deref() {
            if !stage_ids.contains(stage_id) {
                return Err("Workflow approver stage must exist".to_string());
            }
        }
        Ok(crate::spaces::workflows::WorkflowApprover {
            id: Some(
                approver
                    .id
                    .as_deref()
                    .filter(|id| !id.trim().is_empty())
                    .map(str::trim)
                    .map(str::to_string)
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            ),
            approver_type: approver.approver_type,
            approver_id,
            stage_id: approver.stage_id,
        })
    })
    .collect::<Result<Vec<_>, String>>()?;

    Ok((stages, approvers))
}

fn validate_document_workflow_status(status: &str) -> Result<(), String> {
    match status {
        "active" | "completed" | "cancelled" => Ok(()),
        _ => Err("Document workflow status is invalid".to_string()),
    }
}

fn decode_base64_data_url(data_url: &str) -> Result<Vec<u8>, String> {
    let rest = data_url
        .strip_prefix("data:")
        .ok_or_else(|| "Asset replay payload must be a data URL".to_string())?;
    let (header, payload) = rest
        .split_once(',')
        .ok_or_else(|| "Asset replay data URL is invalid".to_string())?;
    if !header.split(';').any(|part| part == "base64") {
        return Err("Asset replay data URL must be base64 encoded".to_string());
    }
    general_purpose::STANDARD
        .decode(payload)
        .map_err(|error| format!("Asset replay data URL is invalid: {error}"))
}

fn replay_asset_filename(asset_id: &str, original_name: &str) -> String {
    let sanitized_name = sanitize_replay_filename(original_name);
    let extension = sanitized_name
        .rsplit_once('.')
        .map(|(_, extension)| sanitize_replay_filename(extension))
        .filter(|extension| !extension.is_empty())
        .unwrap_or_else(|| "bin".to_string());
    format!("{}.{}", sanitize_replay_filename(asset_id), extension)
}

fn sanitize_replay_filename(value: &str) -> String {
    let sanitized = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
        .collect::<String>();
    if sanitized.is_empty() {
        "asset".to_string()
    } else {
        sanitized
    }
}

async fn rewrite_document_asset_references(
    state: &AppState,
    auth: &AuthUser,
    payload: &serde_json::Map<String, Value>,
    document_id: &str,
    storage_path: &str,
) -> Result<(), String> {
    let references = ["data_url", "url", "source_url", "vault_path"]
        .into_iter()
        .filter_map(|key| payload_string(payload, key))
        .filter(|value| !value.trim().is_empty())
        .collect::<HashSet<_>>();
    if references.is_empty() {
        return Ok(());
    }

    let document: Option<(String, String, String)> =
        sqlx::query_as("SELECT content_path, title, space_id FROM documents WHERE id = ?1")
            .bind(document_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    let Some((content_path, title, space_id)) = document else {
        return Ok(());
    };
    let full_path = crate::storage::data_dir().join(&content_path);
    let current = tokio::fs::read_to_string(&full_path)
        .await
        .map_err(|error| format!("Failed to read document for asset rewrite: {error}"))?;
    let server_url = format!("/api/v1/files/{storage_path}");
    let mut updated = current.clone();
    for reference in references {
        updated = updated.replace(&reference, &server_url);
    }
    if updated == current {
        return Ok(());
    }

    tokio::fs::write(&full_path, &updated)
        .await
        .map_err(|error| format!("Failed to rewrite document asset references: {error}"))?;
    let updated_at = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE documents SET last_updated_by = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(&auth.claims.sub)
        .bind(&updated_at)
        .bind(document_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    let _ = state.search_index.upsert(crate::search::index::DocEntry {
        doc_id: document_id.to_string(),
        space_id,
        title,
        body: updated,
        tags: String::new(),
    });

    Ok(())
}

fn payload_json_string(
    payload: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match payload.get(key) {
        Some(value) if !value.is_null() => serde_json::to_string(value)
            .map(Some)
            .map_err(|error| error.to_string()),
        _ => Ok(None),
    }
}

fn payload_json_array_string(
    payload: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, String> {
    match payload.get(key) {
        Some(value @ Value::Array(_)) => {
            serde_json::to_string(value).map_err(|error| error.to_string())
        }
        Some(Value::Null) | None => Ok("[]".to_string()),
        _ => Err(format!("{key} must be an array")),
    }
}

fn payload_i64(payload: &serde_json::Map<String, Value>, key: &str) -> Option<i64> {
    payload.get(key).and_then(Value::as_i64)
}

fn payload_nullable_string(payload: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    match payload.get(key) {
        Some(Value::String(value)) => Some(value.clone()),
        _ => None,
    }
}

fn replay_timestamp(
    payload: &serde_json::Map<String, Value>,
    key: &str,
    default: Option<&str>,
) -> Result<String, String> {
    match payload_string(payload, key) {
        Some(value) => crate::spaces::routes::import_timestamp_or_default(
            Some(&value),
            default.unwrap_or(&value),
        )
        .map_err(|error| format!("{error:?}")),
        None => match default {
            Some(value) => Ok(value.to_string()),
            None => crate::spaces::routes::import_timestamp_or_now(None)
                .map_err(|error| format!("{error:?}")),
        },
    }
}

async fn replay_record_exists(state: &AppState, table: &str, id: &str) -> Result<bool, String> {
    let query = match table {
        "folders" => "SELECT id FROM folders WHERE id = ?1",
        "tags" => "SELECT id FROM tags WHERE id = ?1",
        "templates" => "SELECT id FROM templates WHERE id = ?1",
        "properties_schema" => "SELECT id FROM properties_schema WHERE id = ?1",
        "workflow_templates" => "SELECT id FROM workflow_templates WHERE id = ?1",
        _ => return Err("Unsupported replay existence table".to_string()),
    };
    sqlx::query_as::<_, (String,)>(query)
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map(|row| row.is_some())
        .map_err(|error| error.to_string())
}

async fn next_property_sort_order(state: &AppState, space_id: &str) -> Result<i32, String> {
    let max_sort: (Option<i32>,) =
        sqlx::query_as("SELECT MAX(sort_order) FROM properties_schema WHERE space_id = ?1")
            .bind(space_id)
            .fetch_one(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    Ok(max_sort.0.unwrap_or(0) + 1)
}

async fn ensure_document_in_space(
    state: &AppState,
    document_id: &str,
    space_id: &str,
) -> Result<(), String> {
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM documents WHERE id = ?1 AND space_id = ?2")
            .bind(document_id)
            .bind(space_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    if existing.is_none() {
        return Err("Document not found on server".to_string());
    }
    Ok(())
}

async fn ensure_property_in_space(
    state: &AppState,
    property_id: &str,
    space_id: &str,
) -> Result<(), String> {
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM properties_schema WHERE id = ?1 AND space_id = ?2")
            .bind(property_id)
            .bind(space_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    if existing.is_none() {
        return Err("Property schema not found on server".to_string());
    }
    Ok(())
}

async fn ensure_workflow_template_stage_in_space(
    state: &AppState,
    template_id: &str,
    stage_id: &str,
    space_id: &str,
) -> Result<(), String> {
    let template: Option<(String,)> =
        sqlx::query_as("SELECT stages FROM workflow_templates WHERE id = ?1 AND space_id = ?2")
            .bind(template_id)
            .bind(space_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    let stages = template
        .ok_or_else(|| "Workflow template not found on server".to_string())
        .and_then(|row| {
            serde_json::from_str::<Vec<crate::spaces::workflows::WorkflowStage>>(&row.0)
                .map_err(|error| error.to_string())
        })?;
    if !stages
        .iter()
        .any(|stage| stage.id.as_deref() == Some(stage_id))
    {
        return Err("Workflow stage not found on server".to_string());
    }
    Ok(())
}

async fn workflow_template_stage_is_final(
    state: &AppState,
    template_id: &str,
    stage_id: &str,
    space_id: &str,
) -> Result<bool, String> {
    let template: Option<(String,)> =
        sqlx::query_as("SELECT stages FROM workflow_templates WHERE id = ?1 AND space_id = ?2")
            .bind(template_id)
            .bind(space_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    let mut stages = template
        .ok_or_else(|| "Workflow template not found on server".to_string())
        .and_then(|row| {
            serde_json::from_str::<Vec<crate::spaces::workflows::WorkflowStage>>(&row.0)
                .map_err(|error| error.to_string())
        })?;
    stages.sort_by_key(|stage| stage.sort_order.unwrap_or(0));
    Ok(stages
        .last()
        .and_then(|stage| stage.id.as_deref())
        .is_some_and(|id| id == stage_id))
}

async fn replay_document_workflow_event_insert(
    state: &AppState,
    auth: &AuthUser,
    payload: &serde_json::Map<String, Value>,
    fallback_workflow_id: &str,
    fallback_document_id: &str,
) -> Result<(), String> {
    let event_id =
        payload_string(payload, "id").unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM document_workflow_events WHERE id = ?1")
            .bind(&event_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    if existing.is_some() {
        return Ok(());
    }

    let action = payload_string(payload, "action").unwrap_or_else(|| "enable".to_string());
    if !matches!(
        action.as_str(),
        "transition" | "enable" | "disable" | "delegate" | "escalate"
    ) {
        return Err("Workflow event action is not supported".to_string());
    }
    let to_stage_id = payload_string(payload, "to_stage_id")
        .ok_or_else(|| "Workflow event payload missing to_stage_id".to_string())?;
    let created_at = replay_timestamp(payload, "created_at", None)?;
    let created_by =
        payload_string(payload, "created_by").unwrap_or_else(|| auth.claims.sub.clone());
    sqlx::query(
        "INSERT INTO document_workflow_events (id, document_workflow_id, document_id, from_stage_id, to_stage_id, action, note, created_by, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(&event_id)
    .bind(payload_string(payload, "document_workflow_id").unwrap_or_else(|| fallback_workflow_id.to_string()))
    .bind(payload_string(payload, "document_id").unwrap_or_else(|| fallback_document_id.to_string()))
    .bind(payload_string(payload, "from_stage_id"))
    .bind(&to_stage_id)
    .bind(&action)
    .bind(payload_string(payload, "note"))
    .bind(&created_by)
    .bind(&created_at)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    Ok(())
}

async fn upsert_property_value(
    state: &AppState,
    value_id: &str,
    document_id: &str,
    property_id: &str,
    value: Option<&str>,
    created_at: &str,
    updated_at: &str,
) -> Result<(), String> {
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM property_values WHERE document_id = ?1 AND property_id = ?2",
    )
    .bind(document_id)
    .bind(property_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| error.to_string())?;

    if existing.is_some() {
        sqlx::query(
            "UPDATE property_values SET value = ?1, updated_at = ?2 WHERE document_id = ?3 AND property_id = ?4",
        )
        .bind(value)
        .bind(updated_at)
        .bind(document_id)
        .bind(property_id)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;
    } else {
        sqlx::query(
            "INSERT INTO property_values (id, document_id, property_id, value, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(value_id)
        .bind(document_id)
        .bind(property_id)
        .bind(value)
        .bind(created_at)
        .bind(updated_at)
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;
    }

    Ok(())
}

async fn replay_folder_parent_id(
    state: &AppState,
    space_id: &str,
    folder_id: &str,
) -> Result<Option<String>, String> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT parent_id FROM folders WHERE id = ?1 AND space_id = ?2")
            .bind(folder_id)
            .bind(space_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?;

    row.map(|(parent_id,)| parent_id)
        .ok_or_else(|| "Folder not found on server".to_string())
}

async fn replay_folder_depth(
    state: &AppState,
    space_id: &str,
    folder_id: &str,
) -> Result<i32, String> {
    let mut depth = 1;
    let mut current_id = folder_id.to_string();
    let mut seen = HashSet::new();

    loop {
        if !seen.insert(current_id.clone()) {
            return Err("Folder hierarchy contains a cycle".to_string());
        }

        match replay_folder_parent_id(state, space_id, &current_id).await? {
            Some(parent_id) => {
                depth += 1;
                current_id = parent_id;
            }
            None => return Ok(depth),
        }
    }
}

async fn replay_folder_subtree_depth(
    state: &AppState,
    space_id: &str,
    folder_id: &str,
) -> Result<i32, String> {
    let mut max_depth = 1;
    let mut stack = vec![(folder_id.to_string(), 1)];
    let mut seen = HashSet::new();

    while let Some((current_id, depth)) = stack.pop() {
        if !seen.insert(current_id.clone()) {
            return Err("Folder hierarchy contains a cycle".to_string());
        }
        max_depth = max_depth.max(depth);

        let children: Vec<(String,)> =
            sqlx::query_as("SELECT id FROM folders WHERE space_id = ?1 AND parent_id = ?2")
                .bind(space_id)
                .bind(&current_id)
                .fetch_all(&state.db)
                .await
                .map_err(|error| error.to_string())?;

        for (child_id,) in children {
            stack.push((child_id, depth + 1));
        }
    }

    Ok(max_depth)
}

async fn validate_replay_document_folder_depth(
    state: &AppState,
    space_id: &str,
    folder_id: Option<&str>,
) -> Result<(), String> {
    let Some(folder_id) = folder_id else {
        return Ok(());
    };
    if replay_folder_depth(state, space_id, folder_id).await?
        > crate::spaces::folders::MAX_FOLDER_DEPTH
    {
        return Err(format!(
            "Pages can be nested up to {} folder levels",
            crate::spaces::folders::MAX_FOLDER_DEPTH
        ));
    }
    Ok(())
}

async fn validate_replay_new_folder_parent(
    state: &AppState,
    space_id: &str,
    parent_id: Option<&str>,
) -> Result<(), String> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };
    let parent_depth = replay_folder_depth(state, space_id, parent_id).await?;
    if parent_depth + 1 > crate::spaces::folders::MAX_FOLDER_DEPTH {
        return Err(format!(
            "Folders can be nested up to {} levels",
            crate::spaces::folders::MAX_FOLDER_DEPTH
        ));
    }
    Ok(())
}

async fn validate_replay_moved_folder_parent(
    state: &AppState,
    space_id: &str,
    folder_id: &str,
    parent_id: Option<&str>,
) -> Result<(), String> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };
    if folder_id == parent_id {
        return Err("Folder cannot be moved inside itself".to_string());
    }

    let mut current_parent = Some(parent_id.to_string());
    let mut seen = HashSet::new();
    while let Some(current_id) = current_parent {
        if current_id == folder_id {
            return Err("Folder cannot be moved inside one of its descendants".to_string());
        }
        if !seen.insert(current_id.clone()) {
            return Err("Folder hierarchy contains a cycle".to_string());
        }
        current_parent = replay_folder_parent_id(state, space_id, &current_id).await?;
    }

    let parent_depth = replay_folder_depth(state, space_id, parent_id).await?;
    let subtree_depth = replay_folder_subtree_depth(state, space_id, folder_id).await?;
    if parent_depth + subtree_depth > crate::spaces::folders::MAX_FOLDER_DEPTH {
        return Err(format!(
            "Folders can be nested up to {} levels",
            crate::spaces::folders::MAX_FOLDER_DEPTH
        ));
    }
    Ok(())
}

async fn next_folder_sort_order(
    state: &AppState,
    space_id: &str,
    parent_id: Option<&str>,
) -> Result<i32, String> {
    let max_sort: (Option<i32>,) = if let Some(parent_id) = parent_id {
        sqlx::query_as("SELECT MAX(sort_order) FROM folders WHERE space_id = ?1 AND parent_id = ?2")
            .bind(space_id)
            .bind(parent_id)
            .fetch_one(&state.db)
            .await
            .map_err(|error| error.to_string())?
    } else {
        sqlx::query_as(
            "SELECT MAX(sort_order) FROM folders WHERE space_id = ?1 AND parent_id IS NULL",
        )
        .bind(space_id)
        .fetch_one(&state.db)
        .await
        .map_err(|error| error.to_string())?
    };
    Ok(max_sort.0.unwrap_or(0) + 1)
}

async fn existing_replay_status(
    state: &AppState,
    operation_id: &str,
) -> Result<Option<String>, String> {
    sqlx::query_as("SELECT status FROM sync_replay_operations WHERE operation_id = ?1")
        .bind(operation_id)
        .fetch_optional(&state.db)
        .await
        .map(|row: Option<(String,)>| row.map(|item| item.0))
        .map_err(|error| error.to_string())
}

async fn insert_replay_ledger(
    state: &AppState,
    auth: &AuthUser,
    manifest: &SyncReplayManifestRequest,
    operation: &SyncReplayOperation,
    status: &str,
    error: Option<&str>,
) -> Result<(), String> {
    let attempted_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO sync_replay_operations \
         (operation_id, provider, entity_type, entity_id, action, status, attempted_by, attempted_at, error, payload) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(&operation.id)
    .bind(&manifest.provider)
    .bind(&operation.entity_type)
    .bind(&operation.entity_id)
    .bind(&operation.action)
    .bind(status)
    .bind(&auth.claims.sub)
    .bind(attempted_at)
    .bind(error)
    .bind(serde_json::to_string(&operation.payload).map_err(|err| err.to_string())?)
    .execute(&state.db)
    .await
    .map(|_| ())
    .map_err(|err| err.to_string())
}

async fn update_replay_ledger(
    state: &AppState,
    operation_id: &str,
    status: &str,
    error: Option<&str>,
) -> Result<(), String> {
    let completed_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE sync_replay_operations SET status = ?1, completed_at = ?2, error = ?3 WHERE operation_id = ?4",
    )
    .bind(status)
    .bind(completed_at)
    .bind(error)
    .bind(operation_id)
    .execute(&state.db)
    .await
    .map(|_| ())
    .map_err(|err| err.to_string())
}

async fn record_replay_manifest_history(
    state: &AppState,
    auth: &AuthUser,
    manifest: &SyncReplayManifestRequest,
    response: &SyncReplayExecutionResponse,
) -> Result<(), AppError> {
    let manifest_id = uuid::Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO sync_replay_manifests \
         (id, provider, status, attempted_by, operation_count, attempted_operation_count, applied_operation_count, \
          skipped_operation_count, failed_operation_count, unsupported_operation_count, recovery_checkpoint_count, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )
    .bind(&manifest_id)
    .bind(&manifest.provider)
    .bind(&response.status)
    .bind(&auth.claims.sub)
    .bind(manifest.operation_count as i64)
    .bind(response.attempted_operation_count as i64)
    .bind(response.applied_operation_count as i64)
    .bind(response.skipped_operation_count as i64)
    .bind(response.failed_operation_count as i64)
    .bind(response.unsupported_operation_count as i64)
    .bind(manifest.recovery_checkpoint_count as i64)
    .bind(&created_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    for checkpoint in &manifest.recovery_checkpoints {
        let checkpoint_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO sync_replay_checkpoints \
             (id, manifest_id, provider, attempted_by, space_id, source_server_url, downloaded_by_user_id, checkpoint_json, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(&checkpoint_id)
        .bind(&manifest_id)
        .bind(&manifest.provider)
        .bind(&auth.claims.sub)
        .bind(checkpoint_string(checkpoint, "spaceId").or_else(|| checkpoint_string(checkpoint, "space_id")))
        .bind(
            checkpoint_string(checkpoint, "sourceServerUrl")
                .or_else(|| checkpoint_string(checkpoint, "source_server_url")),
        )
        .bind(
            checkpoint_string(checkpoint, "downloadedByUserId")
                .or_else(|| checkpoint_string(checkpoint, "downloaded_by_user_id")),
        )
        .bind(serde_json::to_string(checkpoint).map_err(AppError::internal)?)
        .bind(&created_at)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    }

    Ok(())
}

async fn fetch_replay_history_checkpoints(
    state: &AppState,
    manifest_id: &str,
) -> Result<Vec<SyncReplayHistoryCheckpoint>, AppError> {
    let rows: Vec<(String, Option<String>, Option<String>, Option<String>, String, String)> =
        sqlx::query_as(
            "SELECT id, space_id, source_server_url, downloaded_by_user_id, checkpoint_json, created_at \
             FROM sync_replay_checkpoints WHERE manifest_id = ?1 ORDER BY created_at DESC, id DESC",
        )
        .bind(manifest_id)
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(rows
        .into_iter()
        .map(
            |(
                id,
                space_id,
                source_server_url,
                downloaded_by_user_id,
                checkpoint_json,
                created_at,
            )| {
                SyncReplayHistoryCheckpoint {
                    id,
                    space_id,
                    source_server_url,
                    downloaded_by_user_id,
                    checkpoint: serde_json::from_str(&checkpoint_json).unwrap_or(Value::Null),
                    created_at,
                }
            },
        )
        .collect())
}

fn checkpoint_string(checkpoint: &Value, key: &str) -> Option<String> {
    checkpoint
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

async fn subscribe_user_to_document(
    state: &AppState,
    document_id: &str,
    user_id: &str,
) -> Result<(), String> {
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT document_id FROM doc_subscriptions WHERE document_id = ?1 AND user_id = ?2",
    )
    .bind(document_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| error.to_string())?;
    if existing.is_none() {
        sqlx::query("INSERT INTO doc_subscriptions (document_id, user_id) VALUES (?1, ?2)")
            .bind(document_id)
            .bind(user_id)
            .execute(&state.db)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn payload_string(payload: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    payload.get(key).and_then(Value::as_str).map(str::to_string)
}

fn replay_payload_entity_id(
    operation: &SyncReplayOperation,
    payload: &serde_json::Map<String, Value>,
    label: &str,
) -> Result<String, String> {
    let payload_id = payload_string(payload, "id").unwrap_or_else(|| operation.entity_id.clone());
    if payload_id.trim().is_empty() {
        return Err(format!("{label} replay payload missing id"));
    }
    if payload_id != operation.entity_id {
        return Err(format!(
            "{label} replay payload id {payload_id} does not match operation entity_id {}",
            operation.entity_id
        ));
    }
    Ok(payload_id)
}

fn replay_result(
    operation: &SyncReplayOperation,
    status: &str,
    message: String,
) -> SyncReplayOperationResult {
    SyncReplayOperationResult {
        operation_id: operation.id.clone(),
        entity_type: operation.entity_type.clone(),
        action: operation.action.clone(),
        status: status.to_string(),
        message,
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    let sessions: DocSessions = Arc::new(RwLock::new(HashMap::new()));
    ws.on_upgrade(move |socket| handle_socket(socket, state, sessions))
}

async fn handle_socket(mut socket: WebSocket, state: AppState, sessions: DocSessions) {
    // Step 1: Authenticate — first message must be a JWT token
    let user_id = match socket.recv().await {
        Some(Ok(Message::Text(token))) => {
            match jwt::validate_token(&token, &state.jwt_secret, jwt::TokenType::Access) {
                Ok(claims) => claims.sub,
                Err(_) => {
                    let _ = socket
                        .send(Message::Text(
                            serde_json::json!({"error": "invalid_token"})
                                .to_string()
                                .into(),
                        ))
                        .await;
                    return;
                }
            }
        }
        _ => return,
    };

    // Send auth success
    let _ = socket
        .send(Message::Text(
            serde_json::json!({"status": "authenticated", "user_id": &user_id})
                .to_string()
                .into(),
        ))
        .await;

    // Step 2: Listen for subscribe/change messages
    let mut subscribed_doc: Option<String> = None;
    let mut rx: Option<broadcast::Receiver<SyncMessage>> = None;

    loop {
        tokio::select! {
            // Receive from client
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(sync_msg) = serde_json::from_str::<SyncMessage>(&text) {
                            match sync_msg.msg_type.as_str() {
                                "subscribe" => {
                                    // Subscribe to document changes
                                    let doc_id = sync_msg.doc_id.clone();

                                    let mut sessions_w = sessions.write().await;
                                    let sender = sessions_w
                                        .entry(doc_id.clone())
                                        .or_insert_with(|| broadcast::channel(256).0);

                                    rx = Some(sender.subscribe());
                                    subscribed_doc = Some(doc_id.clone());

                                    // Send current document state
                                    if let Ok(state_data) = load_doc_state(&state, &doc_id).await {
                                        let _ = socket.send(Message::Binary(state_data.into())).await;
                                    }
                                }
                                "change" => {
                                    // Client sent a change — merge and broadcast
                                    if let Some(ref doc_id) = subscribed_doc {
                                        // Store the change in doc_versions
                                        let change_data = serde_json::to_vec(&sync_msg.payload).unwrap_or_default();
                                        store_change(&state, doc_id, &user_id, &change_data).await;

                                        // Broadcast to other clients
                                        let sessions_r = sessions.read().await;
                                        if let Some(sender) = sessions_r.get(doc_id) {
                                            let _ = sender.send(sync_msg);
                                        }
                                    }
                                }
                                "cursor" | "presence" => {
                                    // Forward cursor/presence updates to other clients
                                    if let Some(ref doc_id) = subscribed_doc {
                                        let sessions_r = sessions.read().await;
                                        if let Some(sender) = sessions_r.get(doc_id) {
                                            let _ = sender.send(sync_msg);
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }

            // Receive broadcasts from other clients
            broadcast_msg = async {
                if let Some(ref mut receiver) = rx {
                    receiver.recv().await.ok()
                } else {
                    // No subscription yet — wait forever
                    std::future::pending::<Option<SyncMessage>>().await
                }
            } => {
                if let Some(msg) = broadcast_msg {
                    // Don't echo back to sender
                    if msg.user_id != user_id {
                        let json = serde_json::to_string(&msg).unwrap_or_default();
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }

    // Cleanup: remove from sessions if no other subscribers
    if let Some(doc_id) = subscribed_doc {
        let sessions_r = sessions.read().await;
        if let Some(sender) = sessions_r.get(&doc_id) {
            if sender.receiver_count() == 0 {
                drop(sessions_r);
                sessions.write().await.remove(&doc_id);
            }
        }
    }
}

/// Load the current automerge state for a document
async fn load_doc_state(state: &AppState, doc_id: &str) -> Result<Vec<u8>, anyhow::Error> {
    let result: Option<(Option<Vec<u8>>,)> =
        sqlx::query_as("SELECT automerge_state FROM documents WHERE id = ?1")
            .bind(doc_id)
            .fetch_optional(&state.db)
            .await?;

    Ok(result.and_then(|r| r.0).unwrap_or_default())
}

/// Store a change set and update the document's automerge state
async fn store_change(state: &AppState, doc_id: &str, user_id: &str, change_data: &[u8]) {
    let version_id = uuid::Uuid::new_v4().to_string();

    // Get next version number
    let max: (Option<i64>,) =
        sqlx::query_as("SELECT MAX(version_number) FROM doc_versions WHERE document_id = ?1")
            .bind(doc_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or((None,));

    let version_number = max.0.unwrap_or(0) + 1;

    // Store the change set
    let _ = sqlx::query(
        "INSERT INTO doc_versions (id, document_id, version_number, change_set, created_by) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&version_id)
    .bind(doc_id)
    .bind(version_number)
    .bind(change_data)
    .bind(user_id)
    .execute(&state.db)
    .await;

    // Update the document's automerge_state snapshot
    // In a full implementation, this would merge the change into the existing state
    let updated_at = chrono::Utc::now().to_rfc3339();
    let _ = sqlx::query("UPDATE documents SET automerge_state = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(change_data)
        .bind(&updated_at)
        .bind(doc_id)
        .execute(&state.db)
        .await;
}

#[cfg(test)]
mod tests {
    use super::{
        execute_replay_manifest, replay_operation_support, replay_payload_entity_id,
        validate_replay_manifest, ReplayOperationSupport, SyncReplayContract,
        SyncReplayManifestRequest, SyncReplayOperation,
    };
    use crate::auth::jwt::{Claims, TokenType};
    use crate::auth::middleware::AuthUser;
    use crate::AppState;
    use axum::{extract::State, Json};
    use serde_json::json;
    use sqlx::any::AnyPoolOptions;
    use sqlx::AnyPool;

    fn contract() -> SyncReplayContract {
        SyncReplayContract {
            schema_version: 1,
            operation_order: "queued_at_ascending".to_string(),
            idempotency_key: "operation.id".to_string(),
            transport: "server-api".to_string(),
            server_backends: vec!["sqlite".to_string(), "postgresql".to_string()],
            required_payload_status: "available".to_string(),
            conflict_policy: "server_validates_version_and_permissions".to_string(),
            web_canonical_store: "server-api".to_string(),
        }
    }

    fn auth_user() -> AuthUser {
        AuthUser {
            claims: Claims {
                sub: "user-1".to_string(),
                sid: "test-session".to_string(),
                email: "user@example.test".to_string(),
                role: "member".to_string(),
                exp: 4_102_444_800,
                iat: 1_783_542_000,
                token_type: TokenType::Access,
            },
            api_key_scopes: None,
        }
    }

    fn admin_auth_user() -> AuthUser {
        AuthUser {
            claims: Claims {
                sub: "user-1".to_string(),
                sid: "test-admin-session".to_string(),
                email: "user@example.test".to_string(),
                role: "admin".to_string(),
                exp: 4_102_444_800,
                iat: 1_783_542_000,
                token_type: TokenType::Access,
            },
            api_key_scopes: None,
        }
    }

    fn operation(id: &str, queued_at: &str, payload_status: &str) -> SyncReplayOperation {
        SyncReplayOperation {
            id: id.to_string(),
            entity_type: "document".to_string(),
            entity_id: "doc-1".to_string(),
            action: "update_content".to_string(),
            queued_at: queued_at.to_string(),
            payload_status: payload_status.to_string(),
            payload: json!({ "id": "doc-1", "content": "updated" }),
        }
    }

    fn replay_operation(
        id: &str,
        queued_at: &str,
        entity_type: &str,
        entity_id: &str,
        action: &str,
        payload: serde_json::Value,
    ) -> SyncReplayOperation {
        SyncReplayOperation {
            id: id.to_string(),
            entity_type: entity_type.to_string(),
            entity_id: entity_id.to_string(),
            action: action.to_string(),
            queued_at: queued_at.to_string(),
            payload_status: "available".to_string(),
            payload,
        }
    }

    async fn replay_test_state() -> AppState {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");

        replay_test_state_with_pool(db).await
    }

    async fn replay_test_state_with_pool(db: AnyPool) -> AppState {
        create_replay_fixture_tables(&db).await;
        seed_replay_fixture_data(&db).await;

        let index_path = std::env::temp_dir().join(format!(
            "starlane-notes-sync-replay-test-{}",
            uuid::Uuid::new_v4()
        ));
        AppState {
            db,
            jwt_secret: "test-secret-with-enough-length-for-jwt".to_string(),
            base_url: "http://localhost:3000".to_string(),
            search_index: crate::search::SearchIndex::new(index_path).expect("search index"),
            oauth_cache: crate::auth::routes::OAuthStateCache::new(),
        }
    }

    async fn create_replay_fixture_tables(db: &AnyPool) {
        for statement in [
            "CREATE TABLE users (
                id TEXT PRIMARY KEY NOT NULL,
                email TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'member'
            )",
            "CREATE TABLE spaces (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                description TEXT,
                icon TEXT,
                accent_color TEXT,
                visibility TEXT NOT NULL DEFAULT 'private',
                created_by TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            "CREATE TABLE space_members (
                id TEXT PRIMARY KEY NOT NULL,
                space_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'editor',
                created_at TEXT NOT NULL,
                UNIQUE(space_id, user_id)
            )",
            "CREATE TABLE folders (
                id TEXT PRIMARY KEY NOT NULL,
                space_id TEXT NOT NULL,
                parent_id TEXT,
                name TEXT NOT NULL,
                icon TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            "CREATE TABLE documents (
                id TEXT PRIMARY KEY NOT NULL,
                space_id TEXT NOT NULL,
                folder_id TEXT,
                title TEXT NOT NULL,
                icon TEXT,
                cover_image TEXT,
                content_path TEXT NOT NULL,
                created_by TEXT NOT NULL,
                last_updated_by TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                locked INTEGER NOT NULL DEFAULT 0,
                locked_by TEXT
            )",
            "CREATE TABLE attachments (
                id TEXT PRIMARY KEY NOT NULL,
                document_id TEXT NOT NULL,
                space_id TEXT NOT NULL,
                filename TEXT NOT NULL,
                original_name TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                storage_path TEXT NOT NULL,
                created_by TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT '2026-07-08T12:00:00Z'
            )",
            "CREATE TABLE tags (
                id TEXT PRIMARY KEY NOT NULL,
                space_id TEXT NOT NULL,
                name TEXT NOT NULL,
                color TEXT NOT NULL DEFAULT '#6b7280',
                created_at TEXT NOT NULL,
                UNIQUE(space_id, name)
            )",
            "CREATE TABLE properties_schema (
                id TEXT PRIMARY KEY NOT NULL,
                space_id TEXT NOT NULL,
                name TEXT NOT NULL,
                property_type TEXT NOT NULL,
                options TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                UNIQUE(space_id, name)
            )",
            "CREATE TABLE sync_replay_operations (
                operation_id TEXT PRIMARY KEY NOT NULL,
                provider TEXT NOT NULL,
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                action TEXT NOT NULL,
                status TEXT NOT NULL,
                attempted_by TEXT NOT NULL,
                attempted_at TEXT NOT NULL,
                completed_at TEXT,
                error TEXT,
                payload TEXT
            )",
            "CREATE TABLE sync_replay_manifests (
                id TEXT PRIMARY KEY NOT NULL,
                provider TEXT NOT NULL,
                status TEXT NOT NULL,
                attempted_by TEXT NOT NULL,
                operation_count INTEGER NOT NULL,
                attempted_operation_count INTEGER NOT NULL,
                applied_operation_count INTEGER NOT NULL,
                skipped_operation_count INTEGER NOT NULL,
                failed_operation_count INTEGER NOT NULL,
                unsupported_operation_count INTEGER NOT NULL,
                recovery_checkpoint_count INTEGER NOT NULL,
                created_at TEXT NOT NULL
            )",
            "CREATE TABLE sync_replay_checkpoints (
                id TEXT PRIMARY KEY NOT NULL,
                manifest_id TEXT NOT NULL,
                provider TEXT NOT NULL,
                attempted_by TEXT NOT NULL,
                space_id TEXT,
                source_server_url TEXT,
                downloaded_by_user_id TEXT,
                checkpoint_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            )",
        ] {
            sqlx::query(statement)
                .execute(db)
                .await
                .expect("create replay test table");
        }
    }

    async fn seed_replay_fixture_data(db: &AnyPool) {
        sqlx::query("INSERT INTO users (id, email, display_name, role) VALUES (?1, ?2, ?3, ?4)")
            .bind("user-1")
            .bind("user@example.test")
            .bind("Replay User")
            .bind("member")
            .execute(db)
            .await
            .expect("insert test user");
        sqlx::query(
            "INSERT INTO spaces (id, name, slug, description, icon, accent_color, visibility, created_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, NULL, NULL, NULL, 'private', ?4, ?5, ?5)",
        )
        .bind("space-1")
        .bind("Replay Space")
        .bind("replay-space")
        .bind("user-1")
        .bind("2026-07-08T12:00:00Z")
        .execute(db)
        .await
        .expect("insert test space");
        sqlx::query(
            "INSERT INTO space_members (id, space_id, user_id, role, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind("membership-1")
        .bind("space-1")
        .bind("user-1")
        .bind("owner")
        .bind("2026-07-08T12:00:00Z")
        .execute(db)
        .await
        .expect("insert test membership");
    }

    async fn seed_folder(
        db: &AnyPool,
        id: &str,
        parent_id: Option<&str>,
        name: &str,
        sort_order: i64,
    ) {
        sqlx::query(
            "INSERT INTO folders (id, space_id, parent_id, name, icon, sort_order, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?6)",
        )
        .bind(id)
        .bind("space-1")
        .bind(parent_id)
        .bind(name)
        .bind(sort_order)
        .bind("2026-07-08T12:00:00Z")
        .execute(db)
        .await
        .expect("insert replay test folder");
    }

    async fn seed_three_level_folder_tree(db: &AnyPool) {
        seed_folder(db, "folder-root", None, "Root", 1).await;
        seed_folder(db, "folder-child", Some("folder-root"), "Child", 1).await;
        seed_folder(
            db,
            "folder-grandchild",
            Some("folder-child"),
            "Grandchild",
            1,
        )
        .await;
    }

    async fn seed_document(db: &AnyPool, id: &str, folder_id: Option<&str>, title: &str) {
        sqlx::query(
            "INSERT INTO documents (id, space_id, folder_id, title, icon, content_path, created_by, last_updated_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?6, ?7, ?7)",
        )
        .bind(id)
        .bind("space-1")
        .bind(folder_id)
        .bind(title)
        .bind(format!("spaces/replay-space/docs/{id}.md"))
        .bind("user-1")
        .bind("2026-07-08T12:00:00Z")
        .execute(db)
        .await
        .expect("insert replay test document");
    }

    fn single_operation_manifest(operation: SyncReplayOperation) -> SyncReplayManifestRequest {
        SyncReplayManifestRequest {
            schema_version: 1,
            provider: "self-hosted".to_string(),
            replay_contract: contract(),
            operation_count: 1,
            recovery_checkpoint_count: 0,
            recovery_checkpoints: Vec::new(),
            operations: vec![operation],
        }
    }

    fn replay_sql_manifest() -> SyncReplayManifestRequest {
        SyncReplayManifestRequest {
            schema_version: 1,
            provider: "self-hosted".to_string(),
            replay_contract: contract(),
            operation_count: 4,
            recovery_checkpoint_count: 1,
            recovery_checkpoints: vec![json!({
                "spaceId": "space-1",
                "sourceServerUrl": "https://notes.example.test",
                "downloadedByUserId": "user-1"
            })],
            operations: vec![
                replay_operation(
                    "syncop-folder-create",
                    "2026-07-08T12:00:00.000Z",
                    "folder",
                    "folder-1",
                    "create",
                    json!({
                        "id": "folder-1",
                        "space_id": "space-1",
                        "name": "Imported",
                        "created_at": "2026-07-08T12:00:00Z",
                        "updated_at": "2026-07-08T12:00:00Z"
                    }),
                ),
                replay_operation(
                    "syncop-tag-create",
                    "2026-07-08T12:01:00.000Z",
                    "tag",
                    "tag-1",
                    "create",
                    json!({
                        "id": "tag-1",
                        "space_id": "space-1",
                        "name": "Synced",
                        "color": "#2563eb",
                        "created_at": "2026-07-08T12:01:00Z"
                    }),
                ),
                replay_operation(
                    "syncop-property-create",
                    "2026-07-08T12:02:00.000Z",
                    "property_schema",
                    "property-1",
                    "create",
                    json!({
                        "id": "property-1",
                        "space_id": "space-1",
                        "name": "Status",
                        "property_type": "select",
                        "options": [{ "id": "ready", "label": "Ready" }],
                        "sort_order": 4,
                        "created_at": "2026-07-08T12:02:00Z"
                    }),
                ),
                replay_operation(
                    "syncop-property-reorder",
                    "2026-07-08T12:03:00.000Z",
                    "property_schema",
                    "space-1",
                    "reorder",
                    json!({
                        "space_id": "space-1",
                        "property_ids": ["property-1"]
                    }),
                ),
            ],
        }
    }

    #[test]
    fn replay_preflight_accepts_ready_manifest() {
        let response = validate_replay_manifest(SyncReplayManifestRequest {
            schema_version: 1,
            provider: "none".to_string(),
            replay_contract: contract(),
            operation_count: 2,
            recovery_checkpoint_count: 1,
            recovery_checkpoints: vec![json!({
                "spaceId": "space-1",
                "sourceServerUrl": "https://notes.example.test",
                "downloadedByUserId": "user-1"
            })],
            operations: vec![
                operation("syncop-a", "2026-07-08T12:00:00.000Z", "available"),
                operation("syncop-b", "2026-07-08T12:01:00.000Z", "available"),
            ],
        });

        assert!(response.ready);
        assert_eq!(response.status, "ready");
        assert_eq!(response.ready_operation_count, 2);
        assert!(response.blockers.is_empty());
    }

    #[test]
    fn replay_preflight_blocks_missing_payloads_and_unsorted_operations() {
        let response = validate_replay_manifest(SyncReplayManifestRequest {
            schema_version: 1,
            provider: "none".to_string(),
            replay_contract: contract(),
            operation_count: 2,
            recovery_checkpoint_count: 0,
            recovery_checkpoints: Vec::new(),
            operations: vec![
                operation("syncop-b", "2026-07-08T12:01:00.000Z", "available"),
                operation("syncop-a", "2026-07-08T12:00:00.000Z", "missing"),
            ],
        });

        assert!(!response.ready);
        assert_eq!(response.status, "blocked");
        assert_eq!(response.ready_operation_count, 1);
        assert_eq!(response.blocked_operation_count, 1);
        assert!(response
            .blockers
            .iter()
            .any(|blocker| blocker.contains("missing payload")));
        assert!(response
            .blockers
            .iter()
            .any(|blocker| blocker.contains("sorted by queued_at")));
    }

    #[test]
    fn replay_preflight_blocks_duplicate_operation_ids() {
        let response = validate_replay_manifest(SyncReplayManifestRequest {
            schema_version: 1,
            provider: "none".to_string(),
            replay_contract: contract(),
            operation_count: 2,
            recovery_checkpoint_count: 0,
            recovery_checkpoints: Vec::new(),
            operations: vec![
                operation("syncop-a", "2026-07-08T12:00:00.000Z", "available"),
                operation("syncop-a", "2026-07-08T12:01:00.000Z", "available"),
            ],
        });

        assert!(!response.ready);
        assert!(response
            .blockers
            .iter()
            .any(|blocker| blocker.contains("Duplicate replay operation id")));
    }

    #[test]
    fn replay_preflight_blocks_dependencies_created_later_in_manifest() {
        let response = validate_replay_manifest(SyncReplayManifestRequest {
            schema_version: 1,
            provider: "none".to_string(),
            replay_contract: contract(),
            operation_count: 3,
            recovery_checkpoint_count: 0,
            recovery_checkpoints: Vec::new(),
            operations: vec![
                replay_operation(
                    "syncop-document-tag-add",
                    "2026-07-08T12:00:00.000Z",
                    "document_tag",
                    "doc-1:tag-1",
                    "add",
                    json!({
                        "document_id": "doc-1",
                        "tag_id": "tag-1"
                    }),
                ),
                replay_operation(
                    "syncop-document-create",
                    "2026-07-08T12:01:00.000Z",
                    "document",
                    "doc-1",
                    "create",
                    json!({
                        "id": "doc-1",
                        "space_id": "space-1",
                        "title": "Dependency target"
                    }),
                ),
                replay_operation(
                    "syncop-tag-create",
                    "2026-07-08T12:02:00.000Z",
                    "tag",
                    "tag-1",
                    "create",
                    json!({
                        "id": "tag-1",
                        "space_id": "space-1",
                        "name": "Dependency",
                        "color": "#2563eb"
                    }),
                ),
            ],
        });

        assert!(!response.ready);
        assert_eq!(response.blocked_operation_count, 1);
        assert_eq!(response.ready_operation_count, 2);
        assert!(response.blockers.iter().any(|blocker| {
            blocker.contains(
                "Replay operation syncop-document-tag-add depends on later document create doc-1",
            )
        }));
        assert!(response.blockers.iter().any(|blocker| {
            blocker.contains(
                "Replay operation syncop-document-tag-add depends on later tag create tag-1",
            )
        }));
    }

    #[test]
    fn replay_preflight_blocks_asset_delete_dependencies_created_later_in_manifest() {
        let response = validate_replay_manifest(SyncReplayManifestRequest {
            schema_version: 1,
            provider: "none".to_string(),
            replay_contract: contract(),
            operation_count: 2,
            recovery_checkpoint_count: 0,
            recovery_checkpoints: Vec::new(),
            operations: vec![
                replay_operation(
                    "syncop-asset-delete",
                    "2026-07-08T12:00:00.000Z",
                    "asset",
                    "asset-1",
                    "delete",
                    json!({
                        "id": "asset-1",
                        "space_id": "space-1",
                        "document_id": "doc-asset-target"
                    }),
                ),
                replay_operation(
                    "syncop-document-create",
                    "2026-07-08T12:01:00.000Z",
                    "document",
                    "doc-asset-target",
                    "create",
                    json!({
                        "id": "doc-asset-target",
                        "space_id": "space-1",
                        "title": "Asset target"
                    }),
                ),
            ],
        });

        assert!(!response.ready);
        assert_eq!(response.blocked_operation_count, 1);
        assert_eq!(response.ready_operation_count, 1);
        assert!(response.blockers.iter().any(|blocker| {
            blocker.contains(
                "Replay operation syncop-asset-delete depends on later document create doc-asset-target",
            )
        }));
    }

    #[test]
    fn replay_preflight_accepts_dependencies_created_earlier_in_manifest() {
        let response = validate_replay_manifest(SyncReplayManifestRequest {
            schema_version: 1,
            provider: "none".to_string(),
            replay_contract: contract(),
            operation_count: 3,
            recovery_checkpoint_count: 0,
            recovery_checkpoints: Vec::new(),
            operations: vec![
                replay_operation(
                    "syncop-document-create",
                    "2026-07-08T12:00:00.000Z",
                    "document",
                    "doc-1",
                    "create",
                    json!({
                        "id": "doc-1",
                        "space_id": "space-1",
                        "title": "Dependency target"
                    }),
                ),
                replay_operation(
                    "syncop-tag-create",
                    "2026-07-08T12:01:00.000Z",
                    "tag",
                    "tag-1",
                    "create",
                    json!({
                        "id": "tag-1",
                        "space_id": "space-1",
                        "name": "Dependency",
                        "color": "#2563eb"
                    }),
                ),
                replay_operation(
                    "syncop-document-tag-add",
                    "2026-07-08T12:02:00.000Z",
                    "document_tag",
                    "doc-1:tag-1",
                    "add",
                    json!({
                        "document_id": "doc-1",
                        "tag_id": "tag-1"
                    }),
                ),
            ],
        });

        assert!(response.ready);
        assert_eq!(response.blocked_operation_count, 0);
        assert_eq!(response.ready_operation_count, 3);
    }

    #[test]
    fn replay_payload_entity_id_rejects_mismatched_payload_identity() {
        let operation = operation("syncop-a", "2026-07-08T12:00:00.000Z", "available");
        let payload = json!({ "id": "doc-2" });
        let payload = payload.as_object().expect("payload object");

        let error =
            replay_payload_entity_id(&operation, payload, "Document").expect_err("mismatch");
        assert!(error.contains("does not match operation entity_id doc-1"));
    }

    #[test]
    fn replay_support_covers_document_folder_and_tag_operations() {
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "space".to_string(),
                entity_id: "space-created".to_string(),
                action: "create".to_string(),
                ..operation(
                    "syncop-space-create",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::SpaceCreate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "space".to_string(),
                entity_id: "space-1".to_string(),
                action: "update".to_string(),
                ..operation(
                    "syncop-space-update",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::SpaceUpdate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                action: "create".to_string(),
                ..operation("syncop-create", "2026-07-08T12:00:00.000Z", "available")
            }),
            ReplayOperationSupport::DocumentCreate
        );
        assert_eq!(
            replay_operation_support(&operation(
                "syncop-update",
                "2026-07-08T12:00:00.000Z",
                "available",
            )),
            ReplayOperationSupport::DocumentUpdateContent
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                action: "update".to_string(),
                ..operation(
                    "syncop-document-update",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::DocumentUpdate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                action: "resolve_conflict:merge".to_string(),
                ..operation(
                    "syncop-document-resolve-conflict",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::DocumentResolveConflict
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                action: "resolve_conflict:unknown".to_string(),
                ..operation(
                    "syncop-document-resolve-conflict-unknown",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::Unsupported
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                action: "delete".to_string(),
                ..operation(
                    "syncop-document-delete",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::DocumentDelete
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "folder".to_string(),
                action: "create".to_string(),
                ..operation(
                    "syncop-folder-create",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::FolderCreate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "folder".to_string(),
                action: "delete".to_string(),
                ..operation(
                    "syncop-folder-delete",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::FolderDelete
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "folder".to_string(),
                action: "update".to_string(),
                ..operation(
                    "syncop-folder-update",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::FolderUpdate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "tag".to_string(),
                action: "create".to_string(),
                ..operation("syncop-tag-create", "2026-07-08T12:00:00.000Z", "available")
            }),
            ReplayOperationSupport::TagCreate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "tag".to_string(),
                action: "delete".to_string(),
                ..operation("syncop-tag-delete", "2026-07-08T12:00:00.000Z", "available")
            }),
            ReplayOperationSupport::TagDelete
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "tag".to_string(),
                action: "update".to_string(),
                ..operation("syncop-tag-update", "2026-07-08T12:00:00.000Z", "available")
            }),
            ReplayOperationSupport::TagUpdate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "document_tag".to_string(),
                action: "remove".to_string(),
                ..operation(
                    "syncop-doc-tag-remove",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::DocumentTagRemove
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "document_tag".to_string(),
                action: "add".to_string(),
                ..operation(
                    "syncop-doc-tag-add",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::DocumentTagAdd
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "template".to_string(),
                action: "create".to_string(),
                ..operation(
                    "syncop-template-create",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::TemplateCreate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "template".to_string(),
                action: "update".to_string(),
                ..operation(
                    "syncop-template-update",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::TemplateUpdate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "template".to_string(),
                action: "delete".to_string(),
                ..operation(
                    "syncop-template-delete",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::TemplateDelete
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "property_schema".to_string(),
                action: "create".to_string(),
                ..operation(
                    "syncop-property-create",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::PropertySchemaCreate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "property_schema".to_string(),
                action: "update".to_string(),
                ..operation(
                    "syncop-property-update",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::PropertySchemaUpdate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "property_schema".to_string(),
                action: "delete".to_string(),
                ..operation(
                    "syncop-property-delete",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::PropertySchemaDelete
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "property_schema".to_string(),
                action: "reorder".to_string(),
                ..operation(
                    "syncop-property-reorder",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::PropertySchemaReorder
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "document_property_values".to_string(),
                action: "set".to_string(),
                ..operation(
                    "syncop-property-values",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::DocumentPropertyValuesSet
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "workflow_template".to_string(),
                action: "create".to_string(),
                ..operation(
                    "syncop-workflow-template-create",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::WorkflowTemplateCreate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "workflow_template".to_string(),
                action: "update".to_string(),
                ..operation(
                    "syncop-workflow-template-update",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::WorkflowTemplateUpdate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "workflow_template".to_string(),
                action: "delete".to_string(),
                ..operation(
                    "syncop-workflow-template-delete",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::WorkflowTemplateDelete
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "document_workflow".to_string(),
                action: "create".to_string(),
                ..operation(
                    "syncop-document-workflow-create",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::DocumentWorkflowCreate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "document_workflow".to_string(),
                action: "update".to_string(),
                ..operation(
                    "syncop-document-workflow-update",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::DocumentWorkflowUpdate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "document_workflow".to_string(),
                action: "delete".to_string(),
                ..operation(
                    "syncop-document-workflow-delete",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::DocumentWorkflowDelete
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "document_workflow".to_string(),
                action: "transition".to_string(),
                ..operation(
                    "syncop-document-workflow-transition",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::DocumentWorkflowTransition
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "document_workflow_event".to_string(),
                action: "delegate".to_string(),
                ..operation(
                    "syncop-document-workflow-event",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::DocumentWorkflowEvent
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "asset".to_string(),
                action: "create".to_string(),
                ..operation(
                    "syncop-asset-create",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::AssetCreate
        );
        assert_eq!(
            replay_operation_support(&SyncReplayOperation {
                entity_type: "asset".to_string(),
                action: "delete".to_string(),
                ..operation(
                    "syncop-asset-delete",
                    "2026-07-08T12:00:00.000Z",
                    "available"
                )
            }),
            ReplayOperationSupport::AssetDelete
        );
    }

    #[tokio::test]
    async fn replay_execution_applies_document_conflict_resolution_content() {
        let state = replay_test_state().await;
        crate::storage::init_space_dirs("replay-space")
            .await
            .expect("space dirs");
        seed_document(&state.db, "doc-resolve-conflict", None, "Conflict Doc").await;

        let manifest = single_operation_manifest(replay_operation(
            "syncop-document-resolve-conflict",
            "2026-07-08T12:04:00.000Z",
            "document",
            "doc-resolve-conflict",
            "resolve_conflict:merge",
            json!({
                "id": "doc-resolve-conflict",
                "space_id": "space-1",
                "content": "Resolved conflict body",
                "updated_at": "2026-07-08T12:04:00Z"
            }),
        ));

        let response =
            execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest.clone()))
                .await
                .expect("execute replay")
                .0;

        assert_eq!(response.status, "completed");
        assert_eq!(response.applied_operation_count, 1);
        assert_eq!(response.results[0].status, "applied");

        let document: (String, String, String) = sqlx::query_as(
            "SELECT content_path, last_updated_by, updated_at FROM documents WHERE id = ?1",
        )
        .bind("doc-resolve-conflict")
        .fetch_one(&state.db)
        .await
        .expect("document conflict resolution update");
        assert_eq!(document.1, "user-1");
        assert_eq!(document.2, "2026-07-08T12:04:00+00:00");

        let body = tokio::fs::read_to_string(crate::storage::data_dir().join(document.0))
            .await
            .expect("resolved document body");
        assert_eq!(body, "Resolved conflict body");

        let second_response = execute_replay_manifest(State(state), auth_user(), Json(manifest))
            .await
            .expect("execute replay again")
            .0;
        assert_eq!(second_response.status, "completed");
        assert_eq!(second_response.applied_operation_count, 0);
        assert_eq!(second_response.skipped_operation_count, 1);
    }

    #[tokio::test]
    async fn replay_execution_applies_space_create() {
        let state = replay_test_state().await;

        let manifest = single_operation_manifest(replay_operation(
            "syncop-space-create",
            "2026-07-08T12:04:00.000Z",
            "space",
            "space-created",
            "create",
            json!({
                "id": "space-created",
                "name": "Created Space",
                "description": "Created through replay",
                "icon": "rocket",
                "accent_color": "#2563eb",
                "visibility": "private",
                "created_at": "2026-07-08T12:03:00Z",
                "updated_at": "2026-07-08T12:04:00Z"
            }),
        ));

        let response = execute_replay_manifest(
            State(state.clone()),
            admin_auth_user(),
            Json(manifest.clone()),
        )
        .await
        .expect("execute replay")
        .0;

        assert_eq!(response.status, "completed");
        assert_eq!(response.applied_operation_count, 1);
        assert_eq!(response.results[0].status, "applied");

        let space: (
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            String,
            String,
            String,
        ) = sqlx::query_as(
            "SELECT name, slug, description, icon, accent_color, visibility, created_by, created_at, updated_at \
             FROM spaces WHERE id = ?1",
        )
        .bind("space-created")
        .fetch_one(&state.db)
        .await
        .expect("created space");
        assert_eq!(
            space,
            (
                "Created Space".to_string(),
                "createdspace".to_string(),
                Some("Created through replay".to_string()),
                Some("rocket".to_string()),
                Some("#2563eb".to_string()),
                "private".to_string(),
                "user-1".to_string(),
                "2026-07-08T12:03:00+00:00".to_string(),
                "2026-07-08T12:04:00+00:00".to_string(),
            )
        );

        let owner_membership: (String,) =
            sqlx::query_as("SELECT role FROM space_members WHERE space_id = ?1 AND user_id = ?2")
                .bind("space-created")
                .bind("user-1")
                .fetch_one(&state.db)
                .await
                .expect("owner membership");
        assert_eq!(owner_membership.0, "owner");

        let second_response =
            execute_replay_manifest(State(state), admin_auth_user(), Json(manifest))
                .await
                .expect("execute replay again")
                .0;
        assert_eq!(second_response.status, "completed");
        assert_eq!(second_response.applied_operation_count, 0);
        assert_eq!(second_response.skipped_operation_count, 1);
    }

    #[tokio::test]
    async fn replay_execution_blocks_space_create_for_non_admin() {
        let state = replay_test_state().await;

        let manifest = single_operation_manifest(replay_operation(
            "syncop-space-create-member",
            "2026-07-08T12:04:00.000Z",
            "space",
            "space-member-created",
            "create",
            json!({
                "id": "space-member-created",
                "name": "Member Created Space",
                "created_at": "2026-07-08T12:03:00Z"
            }),
        ));

        let response = execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest))
            .await
            .expect("execute replay")
            .0;

        assert_eq!(response.status, "partial");
        assert_eq!(response.failed_operation_count, 1);
        assert_eq!(response.results[0].status, "failed");
        assert!(response.results[0]
            .message
            .contains("Replay operation requires admin role"));

        let space_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM spaces WHERE id = ?1")
            .bind("space-member-created")
            .fetch_one(&state.db)
            .await
            .expect("space count");
        assert_eq!(space_count.0, 0);
    }

    #[tokio::test]
    async fn replay_execution_applies_space_update() {
        let state = replay_test_state().await;

        let manifest = single_operation_manifest(replay_operation(
            "syncop-space-update",
            "2026-07-08T12:04:00.000Z",
            "space",
            "space-1",
            "update",
            json!({
                "id": "space-1",
                "name": "Renamed Replay Space",
                "description": "Replay metadata",
                "icon": "orbital",
                "accent_color": "#2563eb",
                "visibility": "private",
                "updated_at": "2026-07-08T12:04:00Z"
            }),
        ));

        let response =
            execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest.clone()))
                .await
                .expect("execute replay")
                .0;

        assert_eq!(response.status, "completed");
        assert_eq!(response.applied_operation_count, 1);
        assert_eq!(response.results[0].status, "applied");

        let space: (
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            String,
        ) = sqlx::query_as(
            "SELECT name, slug, description, icon, accent_color, visibility, updated_at FROM spaces WHERE id = ?1",
        )
        .bind("space-1")
        .fetch_one(&state.db)
        .await
        .expect("space update");
        assert_eq!(
            space,
            (
                "Renamed Replay Space".to_string(),
                "replay-space".to_string(),
                Some("Replay metadata".to_string()),
                Some("orbital".to_string()),
                Some("#2563eb".to_string()),
                "private".to_string(),
                "2026-07-08T12:04:00+00:00".to_string(),
            )
        );

        let second_response = execute_replay_manifest(State(state), auth_user(), Json(manifest))
            .await
            .expect("execute replay again")
            .0;
        assert_eq!(second_response.status, "completed");
        assert_eq!(second_response.applied_operation_count, 0);
        assert_eq!(second_response.skipped_operation_count, 1);
    }

    #[tokio::test]
    async fn replay_execution_applies_portable_sql_and_ledger_skips() {
        let state = replay_test_state().await;
        let manifest = replay_sql_manifest();

        let first_response =
            execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest.clone()))
                .await
                .expect("execute replay")
                .0;
        assert_eq!(first_response.status, "completed");
        assert_eq!(first_response.applied_operation_count, 4);
        assert_eq!(first_response.skipped_operation_count, 0);

        let folder: (String, i64) =
            sqlx::query_as("SELECT name, sort_order FROM folders WHERE id = ?1")
                .bind("folder-1")
                .fetch_one(&state.db)
                .await
                .expect("folder replay row");
        let tag: (String, String) = sqlx::query_as("SELECT name, color FROM tags WHERE id = ?1")
            .bind("tag-1")
            .fetch_one(&state.db)
            .await
            .expect("tag replay row");
        let property: (String, String, i64) = sqlx::query_as(
            "SELECT name, property_type, sort_order FROM properties_schema WHERE id = ?1",
        )
        .bind("property-1")
        .fetch_one(&state.db)
        .await
        .expect("property replay row");
        let ledger_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sync_replay_operations WHERE status = ?1")
                .bind("applied")
                .fetch_one(&state.db)
                .await
                .expect("ledger count");
        let checkpoint_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sync_replay_checkpoints")
                .fetch_one(&state.db)
                .await
                .expect("checkpoint count");

        assert_eq!(folder, ("Imported".to_string(), 1));
        assert_eq!(tag, ("Synced".to_string(), "#2563eb".to_string()));
        assert_eq!(property, ("Status".to_string(), "select".to_string(), 1));
        assert_eq!(ledger_count.0, 4);
        assert_eq!(checkpoint_count.0, 1);

        let second_response =
            execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest))
                .await
                .expect("execute replay again")
                .0;
        assert_eq!(second_response.status, "completed");
        assert_eq!(second_response.applied_operation_count, 0);
        assert_eq!(second_response.skipped_operation_count, 4);
    }

    #[tokio::test]
    async fn replay_execution_blocks_folder_create_beyond_depth_limit() {
        let state = replay_test_state().await;
        seed_three_level_folder_tree(&state.db).await;
        let manifest = single_operation_manifest(replay_operation(
            "syncop-folder-too-deep",
            "2026-07-08T12:04:00.000Z",
            "folder",
            "folder-too-deep",
            "create",
            json!({
                "id": "folder-too-deep",
                "space_id": "space-1",
                "parent_id": "folder-grandchild",
                "name": "Too Deep",
                "created_at": "2026-07-08T12:04:00Z",
                "updated_at": "2026-07-08T12:04:00Z"
            }),
        ));

        let response = execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest))
            .await
            .expect("execute replay")
            .0;

        assert_eq!(response.status, "partial");
        assert_eq!(response.failed_operation_count, 1);
        assert_eq!(response.results[0].status, "failed");
        assert!(response.results[0]
            .message
            .contains("Folders can be nested up to 3 levels"));

        let folder_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM folders WHERE id = ?1")
            .bind("folder-too-deep")
            .fetch_one(&state.db)
            .await
            .expect("folder count");
        assert_eq!(folder_count.0, 0);
    }

    #[tokio::test]
    async fn replay_execution_applies_document_update() {
        let state = replay_test_state().await;
        seed_folder(&state.db, "folder-target", None, "Target", 1).await;
        seed_document(&state.db, "doc-update", None, "Original").await;

        let manifest = single_operation_manifest(replay_operation(
            "syncop-document-update",
            "2026-07-08T12:04:00.000Z",
            "document",
            "doc-update",
            "update",
            json!({
                "id": "doc-update",
                "space_id": "space-1",
                "folder_id": "folder-target",
                "title": "Updated Title",
                "icon": "file-text",
                "cover_image": "starlane://asset/cover",
                "updated_at": "2026-07-08T12:04:00Z"
            }),
        ));

        let response =
            execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest.clone()))
                .await
                .expect("execute replay")
                .0;

        assert_eq!(response.status, "completed");
        assert_eq!(response.applied_operation_count, 1);
        assert_eq!(response.results[0].status, "applied");

        let document: (String, Option<String>, String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT title, folder_id, last_updated_by, icon, cover_image FROM documents WHERE id = ?1",
        )
        .bind("doc-update")
        .fetch_one(&state.db)
        .await
        .expect("document update");
        assert_eq!(
            document,
            (
                "Updated Title".to_string(),
                Some("folder-target".to_string()),
                "user-1".to_string(),
                Some("file-text".to_string()),
                Some("starlane://asset/cover".to_string()),
            )
        );

        let second_response = execute_replay_manifest(State(state), auth_user(), Json(manifest))
            .await
            .expect("execute replay again")
            .0;
        assert_eq!(second_response.status, "completed");
        assert_eq!(second_response.applied_operation_count, 0);
        assert_eq!(second_response.skipped_operation_count, 1);
    }

    #[tokio::test]
    async fn replay_execution_applies_document_delete() {
        let state = replay_test_state().await;
        seed_document(&state.db, "doc-delete", None, "Delete Me").await;

        let manifest = single_operation_manifest(replay_operation(
            "syncop-document-delete",
            "2026-07-08T12:04:00.000Z",
            "document",
            "doc-delete",
            "delete",
            json!({
                "id": "doc-delete",
                "space_id": "space-1"
            }),
        ));

        let response =
            execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest.clone()))
                .await
                .expect("execute replay")
                .0;

        assert_eq!(response.status, "completed");
        assert_eq!(response.applied_operation_count, 1);
        assert_eq!(response.results[0].status, "applied");

        let document_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents WHERE id = ?1")
            .bind("doc-delete")
            .fetch_one(&state.db)
            .await
            .expect("document count");
        assert_eq!(document_count.0, 0);

        let second_response = execute_replay_manifest(State(state), auth_user(), Json(manifest))
            .await
            .expect("execute replay again")
            .0;
        assert_eq!(second_response.status, "completed");
        assert_eq!(second_response.applied_operation_count, 0);
        assert_eq!(second_response.skipped_operation_count, 1);
    }

    #[tokio::test]
    async fn replay_execution_applies_asset_delete() {
        let state = replay_test_state().await;
        seed_document(&state.db, "doc-asset-delete", None, "Asset Delete").await;
        sqlx::query(
            "INSERT INTO attachments (id, document_id, space_id, filename, original_name, mime_type, size_bytes, storage_path, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind("asset-delete")
        .bind("doc-asset-delete")
        .bind("space-1")
        .bind("asset-delete.txt")
        .bind("asset-delete.txt")
        .bind("text/plain")
        .bind(5_i64)
        .bind("spaces/replay-space/attachments/doc-asset-delete/asset-delete.txt")
        .bind("user-1")
        .execute(&state.db)
        .await
        .expect("insert replay test attachment");

        let manifest = single_operation_manifest(replay_operation(
            "syncop-asset-delete",
            "2026-07-08T12:04:00.000Z",
            "asset",
            "asset-delete",
            "delete",
            json!({
                "id": "asset-delete",
                "space_id": "space-1",
                "document_id": "doc-asset-delete"
            }),
        ));

        let response =
            execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest.clone()))
                .await
                .expect("execute replay")
                .0;

        assert_eq!(response.status, "completed");
        assert_eq!(response.applied_operation_count, 1);
        assert_eq!(response.results[0].status, "applied");

        let attachment_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM attachments WHERE id = ?1")
                .bind("asset-delete")
                .fetch_one(&state.db)
                .await
                .expect("attachment count");
        assert_eq!(attachment_count.0, 0);

        let second_response = execute_replay_manifest(State(state), auth_user(), Json(manifest))
            .await
            .expect("execute replay again")
            .0;
        assert_eq!(second_response.status, "completed");
        assert_eq!(second_response.applied_operation_count, 0);
        assert_eq!(second_response.skipped_operation_count, 1);
    }

    #[tokio::test]
    async fn replay_execution_blocks_document_create_in_too_deep_folder() {
        let state = replay_test_state().await;
        seed_three_level_folder_tree(&state.db).await;
        seed_folder(
            &state.db,
            "folder-too-deep",
            Some("folder-grandchild"),
            "Too Deep",
            1,
        )
        .await;
        let manifest = single_operation_manifest(replay_operation(
            "syncop-document-too-deep",
            "2026-07-08T12:05:00.000Z",
            "document",
            "doc-too-deep",
            "create",
            json!({
                "id": "doc-too-deep",
                "space_id": "space-1",
                "folder_id": "folder-too-deep",
                "title": "Too Deep",
                "content": "nested too deeply",
                "created_at": "2026-07-08T12:05:00Z",
                "updated_at": "2026-07-08T12:05:00Z"
            }),
        ));

        let response = execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest))
            .await
            .expect("execute replay")
            .0;

        assert_eq!(response.status, "partial");
        assert_eq!(response.failed_operation_count, 1);
        assert_eq!(response.results[0].status, "failed");
        assert!(response.results[0]
            .message
            .contains("Pages can be nested up to 3 folder levels"));

        let document_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents WHERE id = ?1")
            .bind("doc-too-deep")
            .fetch_one(&state.db)
            .await
            .expect("document count");
        assert_eq!(document_count.0, 0);
    }

    #[tokio::test]
    async fn replay_execution_blocks_folder_update_cycle() {
        let state = replay_test_state().await;
        seed_three_level_folder_tree(&state.db).await;
        let manifest = single_operation_manifest(replay_operation(
            "syncop-folder-cycle",
            "2026-07-08T12:06:00.000Z",
            "folder",
            "folder-root",
            "update",
            json!({
                "id": "folder-root",
                "space_id": "space-1",
                "parent_id": "folder-grandchild",
                "name": "Root",
                "updated_at": "2026-07-08T12:06:00Z"
            }),
        ));

        let response = execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest))
            .await
            .expect("execute replay")
            .0;

        assert_eq!(response.status, "partial");
        assert_eq!(response.failed_operation_count, 1);
        assert_eq!(response.results[0].status, "failed");
        assert!(response.results[0]
            .message
            .contains("Folder cannot be moved inside one of its descendants"));

        let parent: (Option<String>,) =
            sqlx::query_as("SELECT parent_id FROM folders WHERE id = ?1")
                .bind("folder-root")
                .fetch_one(&state.db)
                .await
                .expect("root parent");
        assert_eq!(parent.0, None);
    }

    #[tokio::test]
    async fn replay_execution_can_run_against_postgres_when_configured() {
        let Ok(postgres_url) = std::env::var("STARLANE_SYNC_REPLAY_POSTGRES_URL") else {
            eprintln!(
                "skipping Postgres replay integration test; set STARLANE_SYNC_REPLAY_POSTGRES_URL"
            );
            return;
        };
        if postgres_url.trim().is_empty() {
            eprintln!(
                "skipping Postgres replay integration test; STARLANE_SYNC_REPLAY_POSTGRES_URL is empty"
            );
            return;
        }

        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect(&postgres_url)
            .await
            .expect("connect postgres replay test database");
        let schema = format!("sync_replay_test_{}", uuid::Uuid::new_v4().simple());
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&db)
            .await
            .expect("create postgres replay test schema");
        sqlx::query(&format!("SET search_path TO {schema}"))
            .execute(&db)
            .await
            .expect("set postgres replay test search path");

        let state = replay_test_state_with_pool(db.clone()).await;
        let manifest = replay_sql_manifest();
        let response = execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest))
            .await
            .expect("execute postgres replay")
            .0;

        assert_eq!(response.status, "completed");
        assert_eq!(response.applied_operation_count, 4);
        assert_eq!(response.failed_operation_count, 0);

        let ledger_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sync_replay_operations WHERE status = ?1")
                .bind("applied")
                .fetch_one(&state.db)
                .await
                .expect("postgres replay ledger count");
        assert_eq!(ledger_count.0, 4);

        sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
            .execute(&state.db)
            .await
            .expect("drop postgres replay test schema");
    }
}
