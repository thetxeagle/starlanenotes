pub(crate) mod restoration_assets;
pub(crate) mod drafts;
use crate::spaces::document_deletion::rows as delete_rows;
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

/// Application-owned collaboration rooms; cloning AppState shares only this application.
#[derive(Clone, Default)]
pub struct WsRooms {
    rooms: DocSessions,
    drafts: drafts::Rooms,
    #[cfg(test)]
    final_read_pause: Arc<RwLock<Option<Arc<WsFinalReadPause>>>>,
    #[cfg(test)]
    final_source_pause: Arc<RwLock<Option<Arc<WsFinalReadPause>>>>,
}

#[cfg(test)]
struct WsFinalReadPause {
    reached: tokio::sync::Notify,
    release: tokio::sync::Notify,
}

#[cfg(test)]
async fn ws_before_final_read_identity(state: &AppState) {
    let pause = state.ws_rooms.final_read_pause.read().await.clone();
    if let Some(pause) = pause {
        pause.reached.notify_one();
        pause.release.notified().await;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncMessage {
    pub doc_id: String,
    pub user_id: String,
    pub msg_type: String, // "change", "cursor", "presence"
    pub payload: serde_json::Value,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/ws/sync", get(ws_handler))
        .route("/ws/drafts/v1", get(drafts::handler))
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
    #[serde(default)]
    pub draft_bindings: Option<SyncReplayDraftBindingSidecar>,
    pub operations: Vec<SyncReplayOperation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SyncReplayDraftBindingSidecar {
    pub schema_version: u64,
    pub binding_count: usize,
    pub bindings: Vec<SyncReplayDraftBinding>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SyncReplayDraftBinding {
    pub operation_id: String,
    pub operation_fingerprint: String,
    pub transfer_operation_id: String,
    pub transfer_request_fingerprint: String,
    pub local_source: crate::spaces::draft_transfer::LocalSource,
    pub target_space_id: String,
    pub target_document_id: String,
    pub target_draft_id: String,
    pub target_draft_created_at: String,
    pub target_source_digest: String,
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
    #[serde(skip_serializing_if="Option::is_none")]
    pub published_source: Option<crate::storage::canonical_rewrite::PublishedSourceEvidence>,
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

/// Unsupported guest replay stays held; historical receipts confer no new scope.
/// Service/nonhuman protocol behavior is unchanged by account classification.
async fn require_replay_member(state: &AppState, auth: &AuthUser) -> Result<(), AppError> {
    // AuthUser is original verified extractor evidence. This is classification
    // only: existing operation proofs own current JWT/key Read/Write admission.
    // Rechecking expiry here would discard legitimate failed-operation history.
    if auth.api_key_scopes.is_some()
        && auth.claims.sub.starts_with("service:")
        && auth.claims.sid == auth.claims.sub
    {
        return Ok(());
    }
    let kind: Option<String> = sqlx::query_scalar("SELECT account_kind FROM users WHERE id=$1")
        .bind(&auth.claims.sub).fetch_optional(&state.db).await.map_err(AppError::internal)?;
    if kind.as_deref() != Some("member") {
        return Err(AppError::NotFound("Replay unavailable; retain queued work for review".into()));
    }
    Ok(())
}
#[cfg(test)]
thread_local! {static GUEST_BEFORE_REPLAY_HISTORY_FINAL:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=std::cell::RefCell::new(None);}

async fn preflight_replay_manifest(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(manifest): Json<SyncReplayManifestRequest>,
) -> Result<Json<SyncReplayPreflightResponse>, AppError> {
    require_replay_member(&state,&auth).await?;
    let response=validate_replay_manifest(manifest);
    require_replay_member(&state,&auth).await?;
    Ok(Json(response))
}

async fn execute_replay_manifest(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(manifest): Json<SyncReplayManifestRequest>,
) -> Result<Json<SyncReplayExecutionResponse>, AppError> {
    require_replay_member(&state,&auth).await?;
    let preflight = validate_replay_manifest(manifest.clone());
    if !preflight.ready {
        return Err(AppError::BadRequest(format!(
            "Replay manifest is blocked: {}",
            preflight.blockers.join("; ")
        )));
    }

    // Harddelete receipts and replay history are human-owned. Service keys do
    // not become synthetic user rows merely because the HTTP method permits them.
    if manifest.operations.iter().any(|operation| {
        replay_operation_support(operation) == ReplayOperationSupport::DocumentDelete
    }) {
        let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
        crate::spaces::access::require_current_human_preference_write(&mut conn, &auth).await?;
    }

    let mut results = Vec::new();
    for operation in &manifest.operations {
        require_replay_member(&state,&auth).await?;
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
    require_replay_member(&state,&auth).await?;
    record_replay_manifest_history(&state, &auth, &manifest, &response).await?;
    require_replay_member(&state,&auth).await?;
    Ok(Json(response))
}

async fn list_replay_history(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<SyncReplayHistoryResponse>, AppError> {
    require_replay_member(&state,&auth).await?;
    let rows: Vec<(String, String, String, String, i64, i64, i64, i64, i64, i64, i64, String)> =
        sqlx::query_as(
            "SELECT id, provider, status, attempted_by, operation_count, attempted_operation_count, \
             applied_operation_count, skipped_operation_count, failed_operation_count, unsupported_operation_count, \
             recovery_checkpoint_count, created_at \
             FROM sync_replay_manifests WHERE attempted_by = $1 ORDER BY created_at DESC LIMIT 20",
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

    #[cfg(test)]
    {
        let hook=GUEST_BEFORE_REPLAY_HISTORY_FINAL.with(|slot|slot.borrow_mut().take());
        if let Some(hook)=hook {hook.await;}
    }
    require_replay_member(&state,&auth).await?;
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

    if let Some(sidecar) = &manifest.draft_bindings {
        if sidecar.schema_version != 1 {
            blockers.push("Replay draft_bindings schema_version must be 1".to_string());
        }
        if sidecar.binding_count != sidecar.bindings.len() {
            blockers.push(format!(
                "draft_bindings binding_count {} does not match {} bindings",
                sidecar.binding_count,
                sidecar.bindings.len()
            ));
        }
        let mut bound = HashSet::new();
        for binding in &sidecar.bindings {
            if !bound.insert(binding.operation_id.clone()) {
                blockers.push(format!(
                    "Duplicate replay draft binding operation id: {}",
                    binding.operation_id
                ));
                continue;
            }
            let Some(operation) = manifest
                .operations
                .iter()
                .find(|operation| operation.id == binding.operation_id)
            else {
                blockers.push(format!(
                    "Replay draft binding {} has no matching operation",
                    binding.operation_id
                ));
                continue;
            };
            if replay_operation_support(operation) != ReplayOperationSupport::AssetCreate {
                blockers.push(format!(
                    "Replay draft binding {} must reference an asset create operation",
                    binding.operation_id
                ));
            }
            match replay_body_incoming(&manifest.provider, operation) {
                Ok(incoming) if incoming.payload_fingerprint == binding.operation_fingerprint => {}
                _ => blockers.push(format!(
                    "Replay draft binding {} fingerprint differs from its original payload",
                    binding.operation_id
                )),
            }
            if uuid::Uuid::parse_str(&binding.transfer_operation_id).is_err()
                || uuid::Uuid::parse_str(&binding.target_draft_id).is_err()
                || binding.target_space_id.trim().is_empty()
                || binding.target_document_id.trim().is_empty()
                || binding.target_draft_created_at.trim().is_empty()
                || !replay_hash(&binding.transfer_request_fingerprint)
                || !replay_hash(&binding.target_source_digest)
            {
                blockers.push(format!(
                    "Replay draft binding {} has invalid receipt identity",
                    binding.operation_id
                ));
            }
        }
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
            if operation.payload.is_null()
                && replay_operation_support(operation) != ReplayOperationSupport::DocumentDelete
            {
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

fn replay_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn replay_draft_binding<'a>(
    manifest: &'a SyncReplayManifestRequest,
    operation: &SyncReplayOperation,
) -> Option<&'a SyncReplayDraftBinding> {
    manifest
        .draft_bindings
        .as_ref()?
        .bindings
        .iter()
        .find(|binding| binding.operation_id == operation.id)
}

#[cfg(target_os = "linux")]
async fn resolve_replay_draft_binding(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
    payload: &serde_json::Map<String, Value>,
    binding: &SyncReplayDraftBinding,
) -> Result<(String, String, crate::storage::attachment_upload::DraftUploadScope), String> {
    let incoming = replay_body_incoming("binding-fingerprint", operation)?;
    let local = payload
        .get("local_draft_source")
        .and_then(Value::as_object)
        .ok_or("Replay draft binding requires the original local_draft_source")?;
    let local_string = |key: &str| {
        local
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("Replay local_draft_source missing {key}"))
    };
    if local.get("schema_version") != Some(&Value::from(1))
        || local_string("space_id")? != binding.local_source.space_id
        || local_string("document_id")? != binding.local_source.document_id
        || local_string("draft_id")? != binding.local_source.draft_id
        || local_string("draft_created_at")? != binding.local_source.draft_created_at
        || payload_string(payload, "draft_id").as_deref()
            != Some(binding.local_source.draft_id.as_str())
        || incoming.payload_fingerprint != binding.operation_fingerprint
    {
        return Err("Replay draft binding differs from the original queued operation".into());
    }
    let receipt = crate::spaces::draft_transfer::receipt_for_replay_binding(
        conn,
        auth,
        &binding.transfer_operation_id,
    )
    .await
    .map_err(|error| format!("{error:?}"))?;
    if receipt.request_fingerprint != binding.transfer_request_fingerprint
        || receipt.local_source != binding.local_source
        || receipt.server_space_id != binding.target_space_id
        || receipt.server_document_id != binding.target_document_id
        || receipt.target_draft_id != binding.target_draft_id
        || receipt.target_draft_created_at != binding.target_draft_created_at
        || receipt.target_source_digest != binding.target_source_digest
        || !receipt
            .selected_operation_fingerprints
            .contains(&binding.operation_fingerprint)
    {
        return Err("Replay draft binding differs from its immutable transfer receipt".into());
    }
    Ok((
        receipt.server_space_id,
        receipt.server_document_id,
        crate::storage::attachment_upload::DraftUploadScope {
            schema_version: 2,
            draft_id: receipt.target_draft_id,
            draft_created_at: receipt.target_draft_created_at,
            copy_source: None,
        },
    ))
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

async fn execute_replay_operation(state:&AppState,auth:&AuthUser,manifest:&SyncReplayManifestRequest,operation:&SyncReplayOperation)->SyncReplayOperationResult {
    let mut result=execute_replay_operation_inner(state,auth,manifest,operation).await;
    if matches!(result.status.as_str(),"applied"|"skipped") && matches!(replay_operation_support(operation),ReplayOperationSupport::DocumentUpdateContent|ReplayOperationSupport::DocumentResolveConflict) && operation.payload.get("draft_id").is_none() {
        let evidence=async {
            let incoming=replay_body_incoming(&manifest.provider,operation)?;
            let mut conn=state.db.acquire().await.map_err(|e|e.to_string())?;
            let receipt=crate::storage::canonical_rewrite::committed_incoming(&mut conn,auth,&incoming).await.map_err(|e|format!("{e:?}"))?.ok_or_else(||"Unproven published replay acknowledgement; retain original edit".to_string())?;
            Ok::<_,String>(receipt.published_source_evidence())
        }.await;
        match evidence {Ok(value)=>result.published_source=value,Err(e)=>return replay_result(operation,"failed",e)}
    }
    result
}
async fn execute_replay_operation_inner(
    state: &AppState,
    auth: &AuthUser,
    manifest: &SyncReplayManifestRequest,
    operation: &SyncReplayOperation,
) -> SyncReplayOperationResult {
    // Validate canonical identity before any historical success shortcut. A
    // receipt or legacy ledger must never disguise malformed supplied metadata.
    if matches!(
        replay_operation_support(operation),
        ReplayOperationSupport::SpaceCreate | ReplayOperationSupport::SpaceUpdate
            | ReplayOperationSupport::DocumentCreate | ReplayOperationSupport::DocumentUpdate | ReplayOperationSupport::DocumentUpdateIdentifier | ReplayOperationSupport::DocumentUpdateMaintenance
            | ReplayOperationSupport::DocumentUpdateContent | ReplayOperationSupport::DocumentResolveConflict
            | ReplayOperationSupport::DocumentDelete | ReplayOperationSupport::TemplateCreate
            | ReplayOperationSupport::TemplateUpdate | ReplayOperationSupport::TemplateDelete
            | ReplayOperationSupport::AssetCreate | ReplayOperationSupport::AssetDelete
    ) {
        if let Some(payload) = operation.payload.as_object() {
            if let Err(error) = replay_payload_entity_id(operation, payload, "Entity") {
                return replay_result(operation, "failed", error);
            }
        }
    }
    match existing_replay_status(state, auth, manifest, operation).await {
        Ok(Some(status)) if matches!(status.as_str(), "applied" | "skipped") => {
            if replay_operation_support(operation)
                == ReplayOperationSupport::DocumentUpdateIdentifier
            {
                return match crate::spaces::page_metadata::historical(state,auth,&manifest.provider,&operation.id,&operation.payload).await {
                    Ok(Some(_))=>replay_result(operation,"skipped","Exact identifier receipt confirms recorded outcome without reapplying current metadata".into()),
                    Ok(None)=>replay_result(operation,"failed","Identifier acknowledgement requires exact receipt; retain queued request".into()),
                    Err(e)=>replay_result(operation,"failed",format!("{e:?}")),
                };
            }

            if replay_operation_support(operation)
                == ReplayOperationSupport::DocumentUpdateMaintenance
            {
                return match crate::spaces::maintenance::historical(state,auth,&manifest.provider,&operation.id,&operation.payload).await {
                    Ok(Some(_))=>replay_result(operation,"skipped","Exact maintenance receipt confirms recorded outcome without reapplying current metadata".into()),
                    Ok(None)=>replay_result(operation,"failed","Maintenance acknowledgement requires exact receipt; retain queued request".into()),
                    Err(e)=>replay_result(operation,"failed",format!("{e:?}")),
                };
            }

            if matches!(
                replay_operation_support(operation),
                ReplayOperationSupport::TemplateCreate
                    | ReplayOperationSupport::TemplateUpdate
                    | ReplayOperationSupport::TemplateDelete
            ) {
                return match replay_template_historical_receipt(state,auth,&manifest.provider,operation).await {
                    Ok(true)=>replay_result(operation,"skipped","Exact template receipt confirms original mutation without reapplying later state".into()),
                    Ok(false)=>replay_result(operation,"failed","Unproven template acknowledgement; retain queued operation for review".into()),
                    Err(error)=>replay_result(operation,"failed",error),
                };
            }
            if replay_operation_support(operation) == ReplayOperationSupport::DocumentUpdate {
                return match replay_metadata_historical_receipt(state,auth,&manifest.provider,operation).await {
                    Ok(true)=>replay_result(operation,"skipped","Exact immutable metadata receipt confirms earlier update without reapplying later state".into()),
                    Ok(false)=>replay_result(operation,"failed","Metadata acknowledgement lacks exact receipt; retain queued edit".into()),
                    Err(error)=>replay_result(operation,"failed",error),
                };
            }
            if replay_operation_support(operation) == ReplayOperationSupport::DocumentCreate {
                return match replay_create_historical_receipt(state,auth,&manifest.provider,operation).await {
                    Ok(true)=>replay_result(operation,"skipped","Exact committed creation receipt confirms original creation without reapplying later state".into()),
                    Ok(false)=>replay_result(operation,"failed","Existing creation acknowledgement has no exact receipt; retain queued creation for review".into()),
                    Err(error)=>replay_result(operation,"failed",error),
                };
            }
            if replay_operation_support(operation)==ReplayOperationSupport::DocumentDelete {
                return match replay_document_delete_historical_receipt(state,auth,&manifest.provider,operation).await {Ok(true)=>replay_result(operation,"skipped","Exact deletion receipt confirms earlier commit without reapplying current page".into()),Ok(false)=>replay_result(operation,"failed","Unproven deletion acknowledgement; retain queued deletion".into()),Err(e)=>replay_result(operation,"failed",e)};
            }
            if replay_operation_support(operation) == ReplayOperationSupport::AssetDelete {
                return match replay_asset_delete_historical_receipt(state,auth,&manifest.provider,operation).await {
                    Ok(true)=>replay_result(operation,"skipped","Exact committed asset deletion receipt confirms this operation without repeating deletion".into()),
                    Ok(false)=>replay_result(operation,"failed","Existing deletion acknowledgement has no exact receipt; retain queued deletion for review".into()),
                    Err(error)=>replay_result(operation,"failed",error),
                };
            }
            if replay_operation_support(operation) == ReplayOperationSupport::AssetCreate {
                return match replay_asset_historical_receipt_bound(state,auth,&manifest.provider,operation,replay_draft_binding(manifest,operation)).await {
                    Ok(true)=>replay_result(operation,"skipped","Exact committed asset receipt confirms this upload without reapplying references".into()),
                    Ok(false)=>replay_result(operation,"failed","Existing asset acknowledgement lacks exact immutable receipt; retain queued upload for review".into()),
                    Err(error)=>replay_result(operation,"failed",error),
                };
            }
            if matches!(
                replay_operation_support(operation),
                ReplayOperationSupport::DocumentUpdateContent
                    | ReplayOperationSupport::DocumentResolveConflict
            ) {
                let recorded_draft=match crate::spaces::draft_save::has_historical(state,auth,&manifest.provider,&operation.id).await {Ok(v)=>v,Err(e)=>return replay_result(operation,"failed",format!("{e:?}"))};
                if operation.payload.get("draft_id").is_some() || recorded_draft {
                    return match replay_body_historical_receipt(state,auth,&manifest.provider,operation).await {
                        Ok(true)=>replay_result(operation,"skipped","Exact draft-save receipt confirms original save without rewriting later edits".into()),
                        Ok(false)=>replay_result(operation,"failed","Draft save acknowledgement has no exact receipt; retain the original edit for review".into()),
                        Err(error)=>replay_result(operation,"failed",error),
                    };
                }
                return match replay_body_historical_receipt(state,auth,&manifest.provider,operation).await {
                    Ok(true)=>replay_result(operation,"skipped","Exact published receipt confirms original commit without reapplying later state".into()),
                    Ok(false)=>replay_result(operation,"failed","Unproven published acknowledgement; retain original queued edit".into()),
                    Err(error)=>replay_result(operation,"failed",error),
                };
            }
            return replay_result(
                operation,
                "skipped",
                format!("Replay operation already recorded with status {status}"),
            );
        }
        Ok(Some(status)) => {
            if replay_operation_support(operation)
                == ReplayOperationSupport::DocumentUpdateIdentifier
            {
                match crate::spaces::page_metadata::historical(
                    state,
                    auth,
                    &manifest.provider,
                    &operation.id,
                    &operation.payload,
                )
                .await
                {
                    Ok(Some(_)) => {
                        let _ = update_replay_ledger(state, &operation.id, "applied", None).await;
                        return replay_result(
                            operation,
                            "skipped",
                            "Exact identifier receipt confirms recorded outcome".into(),
                        );
                    }
                    Ok(None) => {}
                    Err(e) => return replay_result(operation, "failed", format!("{e:?}")),
                }
            }

            if replay_operation_support(operation)
                == ReplayOperationSupport::DocumentUpdateMaintenance
            {
                match crate::spaces::maintenance::historical(
                    state,
                    auth,
                    &manifest.provider,
                    &operation.id,
                    &operation.payload,
                )
                .await
                {
                    Ok(Some(_)) => {
                        let _ = update_replay_ledger(state, &operation.id, "applied", None).await;
                        return replay_result(
                            operation,
                            "skipped",
                            "Exact maintenance receipt confirms recorded outcome".into(),
                        );
                    }
                    Ok(None) => {}
                    Err(e) => return replay_result(operation, "failed", format!("{e:?}")),
                }
            }

            if replay_operation_support(operation)==ReplayOperationSupport::DocumentUpdate {
                match replay_metadata_historical_receipt(state,auth,&manifest.provider,operation).await {
                    Ok(true)=>{let _=update_replay_ledger(state,&operation.id,"applied",None).await;return replay_result(operation,"skipped","Exact metadata receipt confirms earlier commit".into());},
                    Ok(false)=>{},Err(error)=>return replay_result(operation,"failed",error),
                }
            }

            if matches!(
                replay_operation_support(operation),
                ReplayOperationSupport::TemplateCreate
                    | ReplayOperationSupport::TemplateUpdate
                    | ReplayOperationSupport::TemplateDelete
            ) {
                match replay_template_historical_receipt(state, auth, &manifest.provider, operation)
                    .await
                {
                    Ok(true) => {
                        let _ = update_replay_ledger(state, &operation.id, "applied", None).await;
                        return replay_result(
                            operation,
                            "skipped",
                            "Exact template receipt confirms earlier commit".into(),
                        );
                    }
                    Ok(false) => {}
                    Err(error) => return replay_result(operation, "failed", error),
                }
            }
            if replay_operation_support(operation) == ReplayOperationSupport::DocumentCreate {
                match replay_create_historical_receipt(state, auth, &manifest.provider, operation)
                    .await
                {
                    Ok(true) => {
                        let _ = update_replay_ledger(state, &operation.id, "applied", None).await;
                        return replay_result(
                            operation,
                            "skipped",
                            "Earlier atomic creation confirmed by exact receipt".into(),
                        );
                    }
                    Ok(false) => {}
                    Err(error) => return replay_result(operation, "failed", error),
                }
            }
            if replay_operation_support(operation)==ReplayOperationSupport::DocumentDelete {
                match replay_document_delete_historical_receipt(state,auth,&manifest.provider,operation).await {Ok(true)=>{let _=update_replay_ledger(state,&operation.id,"applied",None).await;return replay_result(operation,"skipped","Exact deletion receipt confirms earlier commit".into())},Ok(false)=>{},Err(e)=>return replay_result(operation,"failed",e)}
            }
            if replay_operation_support(operation) == ReplayOperationSupport::AssetDelete {
                match replay_asset_delete_historical_receipt(
                    state,
                    auth,
                    &manifest.provider,
                    operation,
                )
                .await
                {
                    Ok(true) => {
                        let _ = update_replay_ledger(state, &operation.id, "applied", None).await;
                        return replay_result(
                            operation,
                            "skipped",
                            "Earlier atomic deletion confirmed by exact receipt".into(),
                        );
                    }
                    Ok(false) => {}
                    Err(error) => return replay_result(operation, "failed", error),
                }
            }
            if replay_operation_support(operation) == ReplayOperationSupport::AssetCreate {
                match replay_asset_historical_receipt_bound(state, auth, &manifest.provider, operation, replay_draft_binding(manifest, operation))
                    .await
                {
                    Ok(true) => {
                        let _ = update_replay_ledger(state, &operation.id, "applied", None).await;
                        return replay_result(
                            operation,
                            "skipped",
                            "Earlier atomic asset commit confirmed by exact receipt".into(),
                        );
                    }
                    Ok(false) => {}
                    Err(error) => return replay_result(operation, "failed", error),
                }
            }
            if matches!(
                replay_operation_support(operation),
                ReplayOperationSupport::DocumentUpdateContent
                    | ReplayOperationSupport::DocumentResolveConflict
            ) {
                match replay_body_historical_receipt(state, auth, &manifest.provider, operation)
                    .await
                {
                    Ok(true) => {
                        let _ = update_replay_ledger(state, &operation.id, "applied", None).await;
                        return replay_result(
                            operation,
                            "skipped",
                            "Earlier canonical commit confirmed from its exact immutable receipt"
                                .into(),
                        );
                    }
                    Ok(false) => {}
                    Err(error) => return replay_result(operation, "failed", error),
                }
            }
            // Failed or unfinished attempts must stay actionable. A skipped response
            // is a success acknowledgement and could discard the client's recovery queue.
            let result_status = if status == "unsupported" {
                "unsupported"
            } else {
                "failed"
            };
            return replay_result(operation, result_status, format!(
                "Replay operation remains recorded with status {status}; inspect recovery history and submit a corrected operation with a new id. The original payload has been retained."
            ));
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
            replay_document_create(state, auth, &manifest.provider, operation).await
        }
        ReplayOperationSupport::DocumentUpdateIdentifier => crate::spaces::page_metadata::mutate(
            state,
            auth,
            &manifest.provider,
            &operation.id,
            &operation.payload,
        )
        .await
        .map(|r| {
            if r.historical {
                "skipped".into()
            } else {
                "applied".into()
            }
        })
        .map_err(|e| format!("{e:?}")),
        ReplayOperationSupport::DocumentUpdateMaintenance => crate::spaces::maintenance::mutate(
            state,
            auth,
            &manifest.provider,
            &operation.id,
            &operation.payload,
        )
        .await
        .map(|r| {
            if r.historical {
                "skipped".into()
            } else {
                "applied".into()
            }
        })
        .map_err(|e| format!("{e:?}")),
        ReplayOperationSupport::DocumentUpdate => {
            replay_document_update(state, auth, &manifest.provider, operation).await
        }
        ReplayOperationSupport::DocumentUpdateContent => {
            replay_document_update_content(state, auth, &manifest.provider, operation).await
        }
        ReplayOperationSupport::DocumentResolveConflict => {
            replay_document_update_content(state, auth, &manifest.provider, operation).await
        }
        ReplayOperationSupport::DocumentDelete => {
            replay_document_delete(state, auth, &manifest.provider, operation).await
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
            replay_template_crud(state, auth, &manifest.provider, operation).await
        }
        ReplayOperationSupport::TemplateUpdate => {
            replay_template_crud(state, auth, &manifest.provider, operation).await
        }
        ReplayOperationSupport::TemplateDelete => {
            replay_template_crud(state, auth, &manifest.provider, operation).await
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
        ReplayOperationSupport::AssetCreate => {
            replay_asset_create_bound(state, auth, &manifest.provider, operation, replay_draft_binding(manifest, operation)).await
        }
        ReplayOperationSupport::AssetDelete => {
            replay_asset_delete(state, auth, &manifest.provider, operation).await
        }
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
    DocumentUpdateIdentifier,
    DocumentUpdateMaintenance,
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
        ("document", "update_identifier") => ReplayOperationSupport::DocumentUpdateIdentifier,
        ("document", "update_maintenance") => ReplayOperationSupport::DocumentUpdateMaintenance,
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

    let mut tx = state.db.begin().await.map_err(|error| error.to_string())?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx)
        .await
        .map_err(|error| format!("{error:?}"))?;
    // Preserve the actual authenticated replay evidence; no service-as-human identity.
    let admin = crate::auth::middleware::AdminUser {
        claims: auth.claims.clone(),
        api_key_scopes: auth.api_key_scopes.clone(),
    };
    crate::auth::middleware::require_current_admin(&mut tx, &admin)
        .await
        .map_err(|error| format!("{error:?}"))?;
    let existing_slug: Option<(String,)> = sqlx::query_as("SELECT id FROM spaces WHERE slug = $1")
        .bind(&slug)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    if existing_slug.is_some() {
        return Err("A space with this name already exists".to_string());
    }

    let existing_id: Option<(String,)> = sqlx::query_as("SELECT id FROM spaces WHERE id = $1")
        .bind(&space_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    if existing_id.is_some() {
        return Err("A space with this import id already exists".to_string());
    }

    let created_at = replay_timestamp(payload, "created_at", None)?;
    let updated_at = replay_timestamp(payload, "updated_at", Some(&created_at))?;
    let visibility =
        payload_string(payload, "visibility").unwrap_or_else(|| "internal".to_string());
    let icon = payload_nullable_string(payload, "icon");
    let accent_color = payload_nullable_string(payload, "accent_color");

    sqlx::query(
        "INSERT INTO spaces (id, name, slug, description, icon, accent_color, visibility, created_by, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
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
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    sqlx::query(
        "INSERT INTO space_members (id, space_id, user_id, role, created_at) \
         VALUES ($1, $2, $3, 'owner', $4)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(&space_id)
    .bind(&auth.claims.sub)
    .bind(&created_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    crate::spaces::routes::ensure_managed_space_group_on_connection(
        &mut tx,
        &space_id,
        &name,
        &auth.claims.sub,
    )
    .await
    .map_err(|error| format!("{error:?}"))?;

    crate::auth::middleware::require_current_admin(&mut tx, &admin)
        .await
        .map_err(|error| format!("{error:?}"))?;
    tx.commit().await.map_err(|error| error.to_string())?;
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

    let request: crate::spaces::routes::UpdateSpaceRequest = serde_json::from_value(operation.payload.clone()).map_err(|error| format!("Invalid space update: {error}"))?;
    let name = request.name;
    if let Some(ref value) = name {
        validate_replay_name(value, "Space name", 100)?;
    }

    let description = request.description;
    if let Some(ref value) = description {
        if value.len() > 1000 {
            return Err("Space description must be at most 1000 characters".to_string());
        }
    }

    let icon = request.icon;
    let accent_color = request.accent_color;
    let visibility = request.visibility;
    let has_update = name.is_some()
        || description.is_some()
        || icon.is_some()
        || accent_color.is_some()
        || visibility.is_some();
    if !has_update {
        return Err("Space update payload must include at least one editable field".to_string());
    }

    let updated_at = replay_timestamp(payload, "updated_at", None)?;
    crate::spaces::routes::update_space_on_current_authority(
        state, auth, &space_id,
        &crate::spaces::routes::UpdateSpaceRequest {name, description, icon, accent_color, visibility},
        Some(&updated_at),
    ).await.map_err(|error| format!("{error:?}"))?;

    Ok("applied".to_string())
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ReplayCreateReceipt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    human_identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    human_identifier_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    maintenance_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    maintenance_group_id: Option<String>,
    incoming: crate::storage::canonical_rewrite::IncomingIdentity,
    actor_id: String,
    space_id: String,
    title: String,
    icon: Option<String>,
    cover_image: Option<String>,
    folder_id: Option<String>,
    parent_id: Option<String>,
    content: String,
    created_at: String,
    updated_at: String,
    stage: crate::storage::journal::StagedDocument,
}
async fn replay_create_receipt_current(
    conn: &mut sqlx::AnyConnection,
    r: &ReplayCreateReceipt,
) -> Result<bool, String> {
    let raw = serde_json::to_string(r).map_err(|e| e.to_string())?;
    let row:Option<i64>=sqlx::query_scalar("SELECT 1 FROM document_create_replay_receipts WHERE provider=$1 AND incoming_id=$2 AND actor_id=$3 AND entity_id=$4 AND action=$5 AND payload_fingerprint=$6 AND space_id=$7 AND storage_op_id=$8 AND receipt_json=$9").bind(&r.incoming.provider).bind(&r.incoming.operation_id).bind(&r.actor_id).bind(&r.incoming.entity_id).bind(&r.incoming.action).bind(&r.incoming.payload_fingerprint).bind(&r.space_id).bind(&r.stage.op_id).bind(raw).fetch_optional(&mut *conn).await.map_err(|e|e.to_string())?;
    let stage = &r.stage;
    let stored:Option<i64>=sqlx::query_scalar("SELECT 1 FROM storage_journal_receipts WHERE op_id=$1 AND space_id=$2 AND document_id=$3 AND draft_id=$4 AND content_path=$5 AND initial_revision=$6 AND sha256=$7 AND byte_length=$8 AND state IN('pending','complete')").bind(&stage.op_id).bind(&stage.space_id).bind(&stage.document_id).bind(&stage.draft_id).bind(&stage.path).bind(&stage.revision).bind(&stage.sha256).bind(stage.length).fetch_optional(conn).await.map_err(|e|e.to_string())?;
    Ok(row.is_some() && stored.is_some())
}
async fn replay_create_historical_receipt(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    operation: &SyncReplayOperation,
) -> Result<bool, String> {
    let mut conn = state.db.acquire().await.map_err(|e| e.to_string())?;
    crate::spaces::access::require_current_human_preference_write(&mut conn, auth)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let raw:Option<String>=sqlx::query_scalar("SELECT receipt_json FROM document_create_replay_receipts WHERE provider=$1 AND incoming_id=$2").bind(provider).bind(&operation.id).fetch_optional(&mut *conn).await.map_err(|e|e.to_string())?;
    let Some(raw) = raw else { return Ok(false) };
    let r: ReplayCreateReceipt = serde_json::from_str(&raw)
        .map_err(|_| "Creation receipt invalid; retain queued creation for review")?;
    if r.incoming != replay_body_incoming(provider, operation)?
        || r.actor_id != auth.claims.sub
        || r.incoming.entity_type != "document"
        || r.incoming.action != "create"
        || r.stage.document_id != operation.entity_id
        || r.stage.space_id != r.space_id
        || r.stage.revision != r.updated_at
        || r.stage.draft_id.is_empty()
    {
        return Err("Creation receipt does not match exact incoming operation; retain queued creation for review".into());
    }
    crate::spaces::access::require_current_human_preference_write(&mut conn, auth)
        .await
        .map_err(|e| format!("{e:?}"))?;
    if !replay_create_receipt_current(&mut conn, &r).await? {
        return Err(
            "Creation receipt/storage evidence changed; retain queued creation for review".into(),
        );
    }
    Ok(true)
}
async fn replay_created_rows_current(
    conn: &mut sqlx::AnyConnection,
    r: &ReplayCreateReceipt,
) -> Result<bool, String> {
    let page:Option<i64>=sqlx::query_scalar("SELECT 1 FROM documents WHERE id=$1 AND space_id=$2 AND (folder_id=$3 OR(folder_id IS NULL AND $3 IS NULL)) AND (parent_id=$4 OR(parent_id IS NULL AND $4 IS NULL)) AND title=$5 AND (icon=$6 OR(icon IS NULL AND $6 IS NULL)) AND (cover_image=$7 OR(cover_image IS NULL AND $7 IS NULL)) AND content_path=$8 AND created_by=$9 AND last_updated_by=$9 AND created_at=$10 AND updated_at=$11 AND published_at IS NULL AND archived_at IS NULL AND deleted_at IS NULL AND public=0 AND locked=0 AND locked_by IS NULL AND public_slug IS NULL AND automerge_state IS NULL AND sort_order=0 AND deleted_by IS NULL AND (human_identifier=$12 OR(human_identifier IS NULL AND $12 IS NULL)) AND (human_identifier_key=$13 OR(human_identifier_key IS NULL AND $13 IS NULL)) AND (maintenance_user_id=$14 OR(maintenance_user_id IS NULL AND $14 IS NULL)) AND (maintenance_group_id=$15 OR(maintenance_group_id IS NULL AND $15 IS NULL))").bind(&r.incoming.entity_id).bind(&r.space_id).bind(&r.folder_id).bind(&r.parent_id).bind(&r.title).bind(&r.icon).bind(&r.cover_image).bind(&r.stage.path).bind(&r.actor_id).bind(&r.created_at).bind(&r.updated_at).bind(&r.human_identifier).bind(&r.human_identifier_key).bind(&r.maintenance_user_id).bind(&r.maintenance_group_id).fetch_optional(&mut *conn).await.map_err(|e|e.to_string())?;
    let draft:Option<i64>=sqlx::query_scalar("SELECT 1 FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3 AND owner_id=$4 AND title=$5 AND (icon=$6 OR(icon IS NULL AND $6 IS NULL)) AND (cover_image=$7 OR(cover_image IS NULL AND $7 IS NULL)) AND content=$8 AND base_updated_at=$9 AND updated_at=$9 AND created_at=$10 AND previous_folder_id IS NULL AND previous_parent_id IS NULL").bind(&r.stage.draft_id).bind(&r.incoming.entity_id).bind(&r.space_id).bind(&r.actor_id).bind(&r.title).bind(&r.icon).bind(&r.cover_image).bind(&r.content).bind(&r.updated_at).bind(&r.created_at).fetch_optional(&mut *conn).await.map_err(|e|e.to_string())?;
    let subscription: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM doc_subscriptions WHERE document_id=$1 AND user_id=$2")
            .bind(&r.incoming.entity_id)
            .bind(&r.actor_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| e.to_string())?;
    let draft_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM document_drafts WHERE document_id=$1")
            .bind(&r.incoming.entity_id)
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| e.to_string())?;
    let share_count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.document_id=$1").bind(&r.incoming.entity_id).fetch_one(&mut *conn).await.map_err(|e|e.to_string())?;
    let subscription_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM doc_subscriptions WHERE document_id=$1")
            .bind(&r.incoming.entity_id)
            .fetch_one(conn)
            .await
            .map_err(|e| e.to_string())?;
    Ok(page.is_some()
        && draft.is_some()
        && subscription.is_some()
        && draft_count == 1
        && share_count == 0
        && subscription_count == 1)
}
async fn replay_document_create(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    use crate::spaces::{access, documents, folders};
    use crate::storage::journal;
    let payload = operation
        .payload
        .as_object()
        .ok_or("Document create payload must be an object")?;
    let doc_id = replay_payload_entity_id(operation, payload, "Document")?;
    let space_id = payload_string(payload, "space_id")
        .filter(|v| !v.trim().is_empty())
        .ok_or("Document payload missing space_id")?;
    let title = payload_string(payload, "title").ok_or("Document payload missing title")?;
    let content = match payload.get("content") {
        None => String::new(),
        Some(Value::String(body)) => body.clone(),
        _ => {
            return Err(
                "Supplied creation content must be a string; retain queued body for review".into(),
            )
        }
    };
    if title.len() > 500 || content.len() > 5_242_880 {
        return Err("Document title/body exceeds creation limit".into());
    }
    let optional = |name: &str| -> Result<Option<String>, String> {
        match payload.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(v)) => Ok(Some(v.clone())),
            _ => Err(format!("Document {name} must be a string or null")),
        }
    };
    let (human_identifier, human_identifier_key) =
        crate::spaces::page_metadata::normalize_identifier(
            optional("human_identifier")?.as_deref(),
        )
        .map_err(|e| format!("{e:?}"))?;
    let maintenance_user_id = optional("maintenance_user_id")?;
    let maintenance_group_id = optional("maintenance_group_id")?;
    let icon = optional("icon")?;
    let cover_image = optional("cover_image")?;
    let folder_id = optional("folder_id")?;
    let parent_id = optional("parent_id")?;
    let created_at = crate::spaces::routes::import_timestamp_or_now(
        payload_string(payload, "created_at").as_deref(),
    )
    .map_err(|e| format!("{e:?}"))?;
    let updated_at = crate::spaces::routes::import_timestamp_or_default(
        payload_string(payload, "updated_at").as_deref(),
        &created_at,
    )
    .map_err(|e| format!("{e:?}"))?;
    if replay_create_historical_receipt(state, auth, provider, operation).await? {
        return Ok("skipped".into());
    }
    let principal = access::principal_from_auth(auth).map_err(|e| format!("{e:?}"))?;
    let mut tx = state.db.begin().await.map_err(|e| e.to_string())?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx)
        .await
        .map_err(|e| format!("{e:?}"))?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space_id])
        .await
        .map_err(|e| format!("{e:?}"))?;
    documents::require_document_creation_on_connection(&mut tx, auth, &space_id)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let existing: Option<i64> = sqlx::query_scalar("SELECT 1 FROM documents WHERE id=$1")
        .bind(&doc_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    if existing.is_some() {
        return Err("Creation ID is unavailable without an exact committed incoming receipt; retain queued creation for review".into());
    }
    folders::validate_document_folder_depth_on_connection(&mut tx, &space_id, folder_id.as_deref())
        .await
        .map_err(|e| format!("{e:?}"))?;
    documents::validate_create_parent_on_connection(
        &mut tx,
        &space_id,
        parent_id.as_deref(),
        folder_id.as_deref(),
        principal,
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    documents::require_document_creation_on_connection(&mut tx, auth, &space_id)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1")
        .bind(&space_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    crate::spaces::page_metadata::ensure_identifier_available(
        &mut tx,
        &space_id,
        human_identifier_key.as_deref(),
        None,
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    let maintenance_target = crate::spaces::maintenance::validate_assignment(
        &mut tx,
        auth,
        &space_id,
        maintenance_user_id.as_deref(),
        maintenance_group_id.as_deref(),
        crate::spaces::maintenance::TargetUse::Select,
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    let frontmatter =
        documents::document_storage_frontmatter(&title, icon.as_deref(), false, false);
    let draft_id = uuid::Uuid::new_v4().to_string();
    let stage = journal::stage_document(
        &mut tx,
        &space_id,
        &doc_id,
        &draft_id,
        &updated_at,
        &slug,
        &title,
        &frontmatter,
        &content,
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    sqlx::query("INSERT INTO documents(id,space_id,folder_id,parent_id,title,icon,cover_image,content_path,created_by,last_updated_by,created_at,updated_at,human_identifier,human_identifier_key,maintenance_user_id,maintenance_group_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$9,$10,$11,$12,$13,$14,$15)").bind(&doc_id).bind(&space_id).bind(&folder_id).bind(&parent_id).bind(&title).bind(&icon).bind(&cover_image).bind(&stage.path).bind(&auth.claims.sub).bind(&created_at).bind(&updated_at).bind(&human_identifier).bind(&human_identifier_key).bind(&maintenance_user_id).bind(&maintenance_group_id).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,icon,cover_image,content,base_updated_at,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$9)").bind(&draft_id).bind(&doc_id).bind(&space_id).bind(&auth.claims.sub).bind(&title).bind(&icon).bind(&cover_image).bind(&content).bind(&updated_at).bind(&created_at).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    sqlx::query("INSERT INTO doc_subscriptions(document_id,user_id) VALUES($1,$2) ON CONFLICT(document_id,user_id) DO NOTHING").bind(&doc_id).bind(&auth.claims.sub).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    let r = ReplayCreateReceipt {
        human_identifier,
        human_identifier_key,
        maintenance_user_id,
        maintenance_group_id,
        incoming: replay_body_incoming(provider, operation)?,
        actor_id: auth.claims.sub.clone(),
        space_id: space_id.clone(),
        title,
        icon,
        cover_image,
        folder_id,
        parent_id,
        content,
        created_at,
        updated_at,
        stage,
    };
    sqlx::query("INSERT INTO document_create_replay_receipts(provider,incoming_id,actor_id,entity_id,action,payload_fingerprint,space_id,storage_op_id,receipt_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(provider).bind(&operation.id).bind(&auth.claims.sub).bind(&doc_id).bind(&operation.action).bind(&r.incoming.payload_fingerprint).bind(&space_id).bind(&r.stage.op_id).bind(serde_json::to_string(&r).map_err(|e|e.to_string())?).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    #[cfg(test)]
    replay_create_before_commit().await;
    folders::validate_document_folder_depth_on_connection(
        &mut tx,
        &space_id,
        r.folder_id.as_deref(),
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    documents::validate_create_parent_on_connection(
        &mut tx,
        &space_id,
        r.parent_id.as_deref(),
        r.folder_id.as_deref(),
        principal,
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    documents::require_document_creation_on_connection(&mut tx, auth, &space_id)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let current_slug: Option<String> = sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1")
        .bind(&space_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    if current_slug.as_ref() != Some(&slug)
        || !replay_created_rows_current(&mut tx, &r).await?
        || !replay_create_receipt_current(&mut tx, &r).await?
    {
        return Err("Created page/draft/receipt changed; retain queued creation for review".into());
    }
    if crate::spaces::maintenance::validate_assignment(
        &mut tx,
        auth,
        &space_id,
        r.maintenance_user_id.as_deref(),
        r.maintenance_group_id.as_deref(),
        crate::spaces::maintenance::TargetUse::Select,
    )
    .await
    .map_err(|e| format!("{e:?}"))?
        != maintenance_target
    {
        return Err("Maintenance target changed during creation".into());
    }
    documents::require_document_creation_on_connection(&mut tx, auth, &space_id)
        .await
        .map_err(|e| format!("{e:?}"))?;
    journal::verify_staged_files(&r.stage).map_err(|e| format!("{e:?}"))?;
    #[cfg(test)]
    let fault = REPLAY_CREATE_COMMIT_FAULT.with(|s| s.borrow_mut().take());
    #[cfg(test)]
    let committed = if fault == Some(ReplayCanonicalCommitFault::Rollback) {
        tx.rollback().await.map_err(|e| e.to_string())?;
        Err("injected creation rollback".into())
    } else {
        tx.commit().await.map_err(|e| e.to_string()).and_then(|()| {
            if fault.is_some() {
                Err("injected lost creation acknowledgement".into())
            } else {
                Ok(())
            }
        })
    };
    #[cfg(not(test))]
    let committed = tx.commit().await.map_err(|e| e.to_string());
    if let Err(error) = committed {
        #[cfg(test)]
        {
            let hook = REPLAY_CREATE_BEFORE_CONFIRM.with(|s| s.borrow_mut().take());
            if let Some(h) = hook {
                h.await
            }
        }
        match replay_create_historical_receipt(state, auth, provider, operation).await {
            Ok(true) => {}
            Ok(false) => {
                return Err(format!(
                    "Creation did not commit; retained private stage: {error}"
                ))
            }
            Err(proof) => {
                return Err(format!(
                    "Creation outcome indeterminate; retain queued work and private stage: {proof}"
                ))
            }
        }
    }
    if !journal::publish_committed(&state.db, &r.stage).await {
        tracing::warn!(document_id=%doc_id,op_id=%r.stage.op_id,"Replay creation committed; storage_pending evidence retained and owned SQL draft remains authoritative");
    }
    Ok("applied".into())
}
#[cfg(test)]
thread_local! {static REPLAY_CREATE_COMMIT_FAULT:std::cell::RefCell<Option<ReplayCanonicalCommitFault>>=const{std::cell::RefCell::new(None)};static REPLAY_CREATE_BEFORE_CONFIRM:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};static REPLAY_CREATE_BEFORE_COMMIT:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};}
#[cfg(test)]
async fn replay_create_before_commit() {
    let hook = REPLAY_CREATE_BEFORE_COMMIT.with(|s| s.borrow_mut().take());
    if let Some(h) = hook {
        h.await
    }
}

fn replay_body_incoming(
    provider: &str,
    operation: &SyncReplayOperation,
) -> Result<crate::storage::canonical_rewrite::IncomingIdentity, String> {
    use sha2::{Digest, Sha256};
    fn ordered(value: &Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort();
                let mut result = serde_json::Map::new();
                for key in keys {
                    result.insert(key.clone(), ordered(&map[key]));
                }
                Value::Object(result)
            }
            Value::Array(values) => Value::Array(values.iter().map(ordered).collect()),
            other => other.clone(),
        }
    }
    if provider.is_empty() || operation.id.is_empty() {
        return Err("Replay provider and operation identity are required".into());
    }
    let fingerprint = hex::encode(Sha256::digest(
        serde_json::to_vec(&ordered(&operation.payload)).map_err(|error| error.to_string())?,
    ));
    Ok(crate::storage::canonical_rewrite::IncomingIdentity {
        provider: provider.into(),
        operation_id: operation.id.clone(),
        entity_type: operation.entity_type.clone(),
        entity_id: operation.entity_id.clone(),
        action: operation.action.clone(),
        payload_fingerprint: fingerprint,
    })
}
async fn replay_body_historical_receipt(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    operation: &SyncReplayOperation,
) -> Result<bool, String> {
    let incoming = replay_body_incoming(provider, operation)?;
    if let Some(payload)=operation.payload.as_object() {
        if let Some(draft)=payload.get("draft_id") {
            let draft=draft.as_str().filter(|s|!s.trim().is_empty()).ok_or_else(||"Replay draft_id must be a nonempty string when supplied".to_string())?;
            let doc=replay_payload_entity_id(operation,payload,"Document")?;
            let source=crate::spaces::draft_save::required(payload.get("source_digest")).map_err(|e|format!("{e:?}"))?;
            return crate::spaces::draft_save::historical(state,auth,&incoming,&doc,draft,&source).await.map_err(|e|format!("{e:?}"));
        }
    }
    if crate::spaces::draft_save::has_historical(state,auth,provider,&operation.id).await.map_err(|e|format!("{e:?}"))? {
        return Err("source_required: Original recorded draft_id and source_digest are required to confirm this queued save".into());
    }
    let mut conn = state
        .db
        .acquire()
        .await
        .map_err(|error| error.to_string())?;
    crate::storage::canonical_rewrite::committed_incoming(&mut conn, auth, &incoming)
        .await
        .map(|receipt| receipt.is_some())
        .map_err(|error| format!("{error:?}"))
}
async fn replay_document_update_content(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    use crate::spaces::access::{self, DraftOperation, PageOperation, Purpose};
    use crate::storage::canonical_rewrite::{self, CommitEvidence, RewriteIntent, RewriteReason};
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Document update_content payload must be an object".to_string())?;
    let doc_id = replay_payload_entity_id(operation, payload, "Document")?;
    let content = payload_string(payload, "content")
        .ok_or_else(|| "Document payload missing content".to_string())?;
    if content.len() > 5_242_880 {
        return Err("Document content must be at most 5 MB".into());
    }
    let requested_draft = match payload.get("draft_id") {
        None => None,
        Some(Value::String(id)) if !id.trim().is_empty() => Some(id.clone()),
        _ => return Err("Replay draft_id must be a nonempty string when supplied".into()),
    };
    if payload
        .get("updated_at")
        .is_some_and(|value| !value.is_null() && !value.is_string())
    {
        return Err("Replay updated_at must be a timestamp string or null".into());
    }
    let updated_at = replay_timestamp(payload, "updated_at", None)?;
    let incoming = replay_body_incoming(provider, operation)?;
    if let Some(draft)=requested_draft.as_deref() {
        let source=crate::spaces::draft_save::required(payload.get("source_digest")).map_err(|e|format!("{e:?}"))?;
        if crate::spaces::draft_save::historical(state,auth,&incoming,&doc_id,draft,&source).await.map_err(|e|format!("{e:?}"))? {return Ok("applied".into());}
        let actual_space:Option<String>=sqlx::query_scalar("SELECT space_id FROM documents WHERE id=$1 AND archived_at IS NULL AND deleted_at IS NULL").bind(&doc_id).fetch_optional(&state.db).await.map_err(|e|e.to_string())?;
        let space=actual_space.ok_or_else(||"Document is unavailable on server".to_string())?;
        match payload.get("space_id") {None=>{},Some(Value::String(s)) if s==&space=>{},_=>return Err("Replay space_id no longer matches the document; retain the queued edit".into())}
        crate::spaces::draft_save::save(state,auth,&space,&doc_id,draft,&source,crate::spaces::draft_save::Patch{content:Some(content.clone()),..Default::default()},&updated_at,Some(&incoming)).await.map_err(|e|format!("{e:?}"))?;
        return Ok("applied".into());
    }
    // Bound historical evidence acknowledges an earlier commit only; no canonical
    // SQL or body is replayed after later edits, unpublication or deletion.
    if replay_body_historical_receipt(state, auth, provider, operation).await? {
        return Ok("applied".into());
    }
    let original_digest=crate::spaces::published_source::required(payload.get("published_source_digest")).map_err(|e|format!("{e:?}"))?;
    let principal = access::principal_from_auth(auth).map_err(|error| format!("{error:?}"))?;
    // Advisory scope discovery is outside the transaction: its first write is
    // the ordered space guard, followed by a fresh scoped document lock.
    let actual_space: Option<String> = sqlx::query_scalar(
        "SELECT space_id FROM documents WHERE id=$1 AND archived_at IS NULL AND deleted_at IS NULL",
    )
    .bind(&doc_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| error.to_string())?;
    let space_id = actual_space.ok_or_else(|| "Document is unavailable on server".to_string())?;
    match payload.get("space_id") {None=>{},Some(Value::String(id)) if !id.trim().is_empty() && id==&space_id=>{},Some(Value::String(_))=>return Err("Replay space_id no longer matches the document's space; review the queued edit before retrying".into()),_=>return Err("Replay space_id must be a nonempty string when supplied".into())};
    let mut tx = state.db.begin().await.map_err(|error| error.to_string())?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await.map_err(|e|format!("{e:?}"))?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space_id])
        .await
        .map_err(|error| format!("{error:?}"))?;
    let locked=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&doc_id).bind(&space_id).execute(&mut *tx).await.map_err(|error|error.to_string())?;
    if locked.rows_affected() != 1 {
        return Err("Document is unavailable in this space".into());
    }
    let initial_access=access::resolve_page_access(
        &mut tx,
        principal,
        &space_id,
        &doc_id,
        PageOperation::Edit,
        Purpose::Routine,
    )
    .await
    .map_err(|error| format!("{error:?}"))?;
    let source:(String,String,Option<String>)=sqlx::query_as("SELECT content_path,updated_at,published_at FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&doc_id).bind(&space_id).fetch_one(&mut *tx).await.map_err(|error|error.to_string())?;
    if source.2.is_none() {
        return Err("source_required: Unpublished content replay requires an explicit original draft_id and source_digest; retain the queued edit for review".into());
    }
    let original_row=crate::spaces::published_source::selected(&mut tx,&space_id,&doc_id).await.map_err(|e|format!("{e:?}"))?;
    let old_bytes=canonical_rewrite::published_source_bytes(&source.0).map_err(|e|format!("{e:?}"))?;
    if crate::spaces::published_source::digest(&original_row,&old_bytes).map_err(|e|format!("{e:?}"))?!=original_digest {return Err("source_conflict: Original published source changed; retain queued edit".into());}
    let mut expected_source=crate::spaces::published_source::capture_source(&mut tx,auth,&space_id,&doc_id,&initial_access.ancestor_ids).await.map_err(|e|format!("{e:?}"))?;
    let rewrite = canonical_rewrite::prepare_published_replay(
        &mut tx,
        auth,
        RewriteIntent {
            space_id: space_id.clone(),
            document_id: doc_id.clone(),
            source_path: source.0.clone(),
            source_revision: source.1.clone(),
            published_stamp: source.2.clone().unwrap(),
            revision: updated_at.clone(),
            source_sha256: Some({use sha2::{Digest,Sha256};hex::encode(Sha256::digest(&old_bytes))}),
            reason: RewriteReason::ReplayContent,
            incoming,
        },
        &original_digest,
        content.as_bytes(),
    )
    .await
    .map_err(|error| format!("{error:?}"))?;
    let written=sqlx::query("UPDATE documents SET content_path=$1,last_updated_by=$2,updated_at=$3 WHERE id=$4 AND space_id=$5 AND content_path=$6 AND updated_at=$7 AND published_at=$8 AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(rewrite.path()).bind(&auth.claims.sub).bind(rewrite.revision()).bind(&doc_id).bind(&space_id).bind(&source.0).bind(&source.1).bind(&source.2).execute(&mut *tx).await.map_err(|error|error.to_string())?;
    if written.rows_affected() != 1 {
        return Err("Published canonical source changed; retain the queued edit for review".into());
    }
    let expected_row=crate::spaces::published_source::expected(&original_row,rewrite.path(),&auth.claims.sub,rewrite.revision());
    crate::spaces::published_source::transition(&mut expected_source,&doc_id,&expected_row).map_err(|e|format!("{e:?}"))?;
    #[cfg(test)]
    replay_canonical_before_commit().await;
    access::resolve_page_access(
        &mut tx,
        principal,
        &space_id,
        &doc_id,
        PageOperation::Edit,
        Purpose::Routine,
    )
    .await
    .map_err(|error| format!("{error:?}"))?;
    let current:Option<(String,String,String,Option<String>)>=sqlx::query_as("SELECT content_path,updated_at,last_updated_by,published_at FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&doc_id).bind(&space_id).fetch_optional(&mut *tx).await.map_err(|error|error.to_string())?;
    if current
        != Some((
            rewrite.path().into(),
            rewrite.revision().into(),
            auth.claims.sub.clone(),
            source.2.clone(),
        ))
    {
        return Err("Published canonical source changed before commit".into());
    }
    // The primitive repeats current actor/source proof after inserting its
    // receipt, then verifies the entire manifest and exact file last. No
    // unrelated awaited proof extends that filesystem boundary before commit.
    canonical_rewrite::record_rewrite(&mut tx, auth, &rewrite)
        .await
        .map_err(|error| format!("{error:?}"))?;
    let audit_id=uuid::Uuid::new_v4().to_string();
    let audit_stamp:String=sqlx::query_scalar("INSERT INTO audit_log(id,user_id,actor_kind,actor_id,action,target_type,target_id) VALUES($1,$2,'user',$2,'document.content_update','document',$3) RETURNING created_at").bind(&audit_id).bind(&auth.claims.sub).bind(&doc_id).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?;
    let audit=serde_json::json!({"id":audit_id,"user_id":auth.claims.sub,"actor_kind":"user","actor_id":auth.claims.sub,"action":"document.content_update","target_type":"document","target_id":doc_id,"created_at":audit_stamp,"ip_address":null,"before_value":null,"after_value":null});
    if crate::spaces::document_deletion::rows(&mut tx,"SELECT * FROM audit_log WHERE id=$1",&[&audit_id]).await?!=vec![audit.clone()] {return Err("Published replay audit changed".into());}
    canonical_rewrite::verify_recorded(&mut tx,&rewrite).await.map_err(|e|format!("{e:?}"))?;
    crate::spaces::published_source::verify_source(&mut tx,&expected_source).await.map_err(|e|format!("{e:?}"))?;
    access::resolve_page_access(&mut tx,principal,&space_id,&doc_id,PageOperation::Edit,Purpose::Routine).await.map_err(|e|format!("{e:?}"))?;
    if crate::spaces::published_source::selected(&mut tx,&space_id,&doc_id).await.map_err(|e|format!("{e:?}"))?!=expected_row {return Err("Published result source changed".into());}
    #[cfg(test)]
    {let hook=REPLAY_PUBLISHED_BEFORE_IDENTITY.with(|s|s.borrow_mut().take());if let Some(h)=hook {h.await;}}
    access::require_current_human_preference_write(&mut tx,auth).await.map_err(|e|format!("{e:?}"))?;
    canonical_rewrite::verify_prepared_files(&rewrite).map_err(|e|format!("{e:?}"))?;
    #[cfg(test)]
    let fault = REPLAY_CANONICAL_COMMIT_FAULT.with(|slot| slot.borrow_mut().take());
    #[cfg(test)]
    let commit_result = if fault == Some(ReplayCanonicalCommitFault::Rollback) {
        tx.rollback().await.map_err(|error| error.to_string())?;
        Err("injected commit rollback".to_string())
    } else {
        tx.commit()
            .await
            .map_err(|error| error.to_string())
            .and_then(|()| {
                if fault.is_some() {
                    Err("injected lost commit acknowledgement".to_string())
                } else {
                    Ok(())
                }
            })
    };
    #[cfg(not(test))]
    let commit_result = tx.commit().await.map_err(|error| error.to_string());
    if let Err(error) = commit_result {
        #[cfg(test)]
        {
            let hook = REPLAY_CANONICAL_BEFORE_CONFIRM.with(|slot| slot.borrow_mut().take());
            if let Some(hook) = hook {
                hook.await;
            }
        }
        match canonical_rewrite::confirm_committed(&state.db,&rewrite).await {
            Ok(CommitEvidence::Committed)=>{},
            Ok(CommitEvidence::NotCommitted)=>return Err(format!("Canonical commit did not apply; retained private staging evidence: {error}")),
            Err(proof)=>return Err(format!("Canonical commit outcome is indeterminate; retain recovery evidence and reconcile before replaying: {proof:?}")),
        }
    }
    crate::audit::append_committed_move_action(&audit).await;
    let _ = index_source_bound_replay(state,auth,&space_id,&doc_id,&expected_row,&rewrite,&content).await;
    Ok("applied".into())
}
#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReplayCanonicalCommitFault {
    LostAck,
    Rollback,
    UnknownProof,
}
#[cfg(test)]
thread_local! {static REPLAY_PUBLISHED_BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};}
#[cfg(test)]
thread_local! {static REPLAY_CANONICAL_COMMIT_FAULT:std::cell::RefCell<Option<ReplayCanonicalCommitFault>>=const {std::cell::RefCell::new(None)};}
#[cfg(test)]
thread_local! {static REPLAY_CANONICAL_BEFORE_CONFIRM:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const {std::cell::RefCell::new(None)};}
#[cfg(test)]
thread_local! {static REPLAY_CANONICAL_BEFORE_COMMIT:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const {std::cell::RefCell::new(None)};}
#[cfg(test)]
async fn replay_canonical_before_commit() {
    let hook = REPLAY_CANONICAL_BEFORE_COMMIT.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook.await;
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, sqlx::FromRow)]
struct MetadataPage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    human_identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    human_identifier_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    maintenance_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    maintenance_group_id: Option<String>,
    id: String,
    space_id: String,
    folder_id: Option<String>,
    parent_id: Option<String>,
    title: String,
    icon: Option<String>,
    cover_image: Option<String>,
    content_path: String,
    created_by: String,
    last_updated_by: String,
    created_at: String,
    updated_at: String,
    published_at: Option<String>,
    archived_at: Option<String>,
    deleted_at: Option<String>,
    deleted_by: Option<String>,
    locked: i64,
    locked_by: Option<String>,
    public: i64,
    public_slug: Option<String>,
    sort_order: i64,
    automerge_state: Option<Vec<u8>>,
}
const METADATA_PAGE_COLUMNS:&str="id,space_id,folder_id,parent_id,title,icon,cover_image,content_path,created_by,last_updated_by,created_at,updated_at,published_at,archived_at,deleted_at,deleted_by,locked,locked_by,public,public_slug,sort_order,automerge_state,human_identifier,human_identifier_key,maintenance_user_id,maintenance_group_id";
async fn metadata_page(conn: &mut sqlx::AnyConnection, id: &str) -> Result<MetadataPage, String> {
    sqlx::query_as(&format!(
        "SELECT {METADATA_PAGE_COLUMNS} FROM documents WHERE id=$1"
    ))
    .bind(id)
    .fetch_optional(conn)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "Page unavailable; retain queued update".into())
}
async fn metadata_subtree(
    conn: &mut sqlx::AnyConnection,
    root: &MetadataPage,
) -> Result<Vec<MetadataPage>, String> {
    let mut pending = vec![root.id.clone()];
    let mut seen = std::collections::BTreeSet::new();
    let mut result = Vec::new();
    while let Some(id) = pending.pop() {
        if !seen.insert(id.clone()) {
            return Err("Page hierarchy changed or is corrupt; retain queued update".into());
        }
        let page = metadata_page(conn, &id).await?;
        if page.space_id != root.space_id || page.archived_at.is_some() || page.deleted_at.is_some()
        {
            return Err("Page hierarchy unavailable; retain queued update".into());
        }
        pending.extend(
            sqlx::query_scalar::<_, String>(
                "SELECT id FROM documents WHERE parent_id=$1 ORDER BY id",
            )
            .bind(&id)
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| e.to_string())?,
        );
        result.push(page);
    }
    result.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(result)
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, sqlx::FromRow)]
struct MetadataDraft {
    id: String,
    document_id: String,
    space_id: String,
    owner_id: String,
    title: String,
    icon: Option<String>,
    cover_image: Option<String>,
    content: String,
    base_updated_at: String,
    created_at: String,
    updated_at: String,
    previous_folder_id: Option<String>,
    previous_parent_id: Option<String>,
}
async fn metadata_draft(conn: &mut sqlx::AnyConnection, id: &str) -> Result<MetadataDraft, String> {
    sqlx::query_as("SELECT id,document_id,space_id,owner_id,title,icon,cover_image,content,base_updated_at,created_at,updated_at,previous_folder_id,previous_parent_id FROM document_drafts WHERE id=$1").bind(id).fetch_optional(conn).await.map_err(|e|e.to_string())?.ok_or_else(||"Requested draft unavailable; retain queued presentation edit".into())
}
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MetadataSource {
    page_updated_at: String,
    published_at: Option<String>,
}
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
enum MetadataDraftChoice {
    OwnLatest,
    Exact {
        id: String,
        updated_at: String,
        base_updated_at: String,
    },
}
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MetadataIntent {
    version: i64,
    fields: Vec<String>,
    source: MetadataSource,
    draft: Option<MetadataDraftChoice>,
}
fn metadata_same_revision(a: &str, b: &str) -> bool {
    match (
        chrono::DateTime::parse_from_rfc3339(a),
        chrono::DateTime::parse_from_rfc3339(b),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}
fn metadata_same_optional_revision(a: &Option<String>, b: &Option<String>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => metadata_same_revision(a, b),
        (None, None) => true,
        _ => false,
    }
}
fn metadata_nullable(
    p: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<Option<String>>, String> {
    match p.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(v)) => Ok(Some(if v.trim().is_empty() {
            None
        } else {
            Some(v.clone())
        })),
        _ => Err(format!(
            "Metadata {key} must be a string or null; retain queued edit"
        )),
    }
}
fn metadata_parse(p: &serde_json::Map<String, Value>) -> Result<Option<MetadataIntent>, String> {
    for key in ["space_id", "title", "content", "updated_at", "created_at"] {
        if p.get(key).is_some_and(|v| !v.is_string()) {
            return Err(format!("Metadata {key} must be a string when supplied"));
        }
    }
    for key in ["folder_id", "parent_id", "icon", "cover_image"] {
        metadata_nullable(p, key)?;
    }
    let Some(raw) = p.get("metadata_intent") else {
        return Ok(None);
    };
    let published = raw
        .get("source")
        .and_then(|v| v.get("published_at"))
        .ok_or("Metadata source requires explicit published_at")?;
    if !published.is_null() && !published.as_str().is_some_and(|v| !v.is_empty()) {
        return Err("Invalid metadata publication source".into());
    }
    let i: MetadataIntent = serde_json::from_value(raw.clone())
        .map_err(|e| format!("Invalid metadata intent; retain queued edit: {e}"))?;
    let mut seen = std::collections::BTreeSet::new();
    if i.version != 1 || i.fields.is_empty() || i.source.page_updated_at.is_empty() {
        return Err("Invalid metadata intent version/source/mask".into());
    }
    for field in &i.fields {
        if !matches!(
            field.as_str(),
            "title" | "icon" | "cover_image" | "folder_id" | "parent_id"
        ) || !seen.insert(field)
            || !p.contains_key(field)
        {
            return Err("Metadata intent requires unique known fields with explicit values; retain queued edit".into());
        }
    }
    if i.fields.iter().any(|f| f == "title")
        && !p
            .get("title")
            .and_then(Value::as_str)
            .is_some_and(|v| !v.trim().is_empty())
    {
        return Err("Metadata title must not be empty".into());
    }
    if i.draft.is_some()
        && !i
            .fields
            .iter()
            .any(|f| matches!(f.as_str(), "title" | "icon" | "cover_image"))
    {
        return Err("Draft context requires a presentation field".into());
    }
    if let Some(MetadataDraftChoice::Exact {
        id,
        updated_at,
        base_updated_at,
    }) = &i.draft
    {
        if id.trim().is_empty() || updated_at.is_empty() || base_updated_at.is_empty() {
            return Err("Exact draft intent requires ID and source revisions".into());
        }
    }
    Ok(Some(i))
}
/// Replace only safely recognized generated single-line presentation fields.
/// Body, line endings and all custom YAML bytes remain unchanged. Ambiguity is a
/// conflict, never a reason to normalize/discard the user's portable metadata.
fn metadata_markdown(
    bytes: &[u8],
    title: &str,
    icon: &Option<String>,
    cover: &Option<String>,
    selected: &[&str],
) -> Result<Vec<u8>, String> {
    let raw = std::str::from_utf8(bytes)
        .map_err(|_| "Canonical source is not UTF-8; retain presentation edit")?;
    let eol = if raw.starts_with("---\r\n") {
        "\r\n"
    } else if raw.starts_with("---\n") {
        "\n"
    } else {
        if raw
            .trim_start_matches('\u{feff}')
            .lines()
            .next()
            .is_some_and(|line| line.trim() == "---")
        {
            return Err("Ambiguous leading YAML header; retain queued edit".into());
        }
        let eol = if raw.contains("\r\n") { "\r\n" } else { "\n" };
        let prefix = format!(
            "---{eol}title: {}{eol}icon: {}{eol}cover_image: {}{eol}---{eol}",
            serde_json::to_string(title).unwrap(),
            serde_json::to_string(icon).unwrap(),
            serde_json::to_string(cover).unwrap()
        );
        let mut out = prefix.into_bytes();
        out.extend_from_slice(bytes);
        return Ok(out);
    };
    let rest = &raw[3 + eol.len()..];
    let boundary = format!("{eol}---{eol}");
    let end = rest
        .find(&boundary)
        .ok_or("Generated presentation header is incomplete; retain queued edit")?;
    let header = &rest[..end];
    let mapping: serde_yaml::Mapping = serde_yaml::from_str(header)
        .map_err(|_| "Ambiguous portable YAML; retain queued presentation edit")?;
    let keys = ["title", "icon", "cover_image"];
    let mut counts = [0; 3];
    let mut out = String::new();
    for line in header.split(eol) {
        let found = keys
            .iter()
            .position(|key| selected.contains(key) && line.starts_with(&format!("{key}:")));
        if let Some(n) = found {
            counts[n] += 1;
            let tail = line.split_once(':').unwrap().1.trim();
            if serde_json::from_str::<Value>(tail).map_or(true, |v| !v.is_string() && !v.is_null())
            {
                return Err("Selected presentation header is not a generated single-line field; retain queued edit".into());
            }
            let value = match n {
                0 => Value::String(title.to_owned()),
                1 => icon.clone().map(Value::String).unwrap_or(Value::Null),
                _ => cover.clone().map(Value::String).unwrap_or(Value::Null),
            };
            out.push_str(&format!(
                "{}: {}",
                keys[n],
                serde_json::to_string(&value).unwrap()
            ));
        } else {
            out.push_str(line);
        }
        out.push_str(eol);
    }
    for (n, key) in keys.iter().enumerate() {
        if !selected.contains(key) {
            continue;
        }
        if counts[n] > 1
            || counts[n] == 0 && mapping.contains_key(serde_yaml::Value::String(key.to_string()))
        {
            return Err("Ambiguous selected presentation field; retain queued edit".into());
        }
        if counts[n] == 0 {
            let value = match n {
                0 => Value::String(title.to_owned()),
                1 => icon.clone().map(Value::String).unwrap_or(Value::Null),
                _ => cover.clone().map(Value::String).unwrap_or(Value::Null),
            };
            out.push_str(&format!(
                "{key}: {}{eol}",
                serde_json::to_string(&value).unwrap()
            ));
        }
    }
    Ok(format!("---{eol}{out}---{eol}{}", &rest[end + boundary.len()..]).into_bytes())
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
struct MetadataReceipt {
    incoming: crate::storage::canonical_rewrite::IncomingIdentity,
    actor_id: String,
    space_id: String,
    before: Vec<MetadataPage>,
    after: Vec<MetadataPage>,
    draft_before: Option<MetadataDraft>,
    draft_after: Option<MetadataDraft>,
    rewrite: Option<crate::storage::canonical_rewrite::PreparedRewrite>,
}
async fn metadata_receipt(
    conn: &mut sqlx::AnyConnection,
    incoming: &crate::storage::canonical_rewrite::IncomingIdentity,
) -> Result<Option<MetadataReceipt>, String> {
    let raw:Option<(String,String,String,String,String,String)>=sqlx::query_as("SELECT actor_id,document_id,space_id,action,payload_fingerprint,receipt_json FROM document_metadata_replay_receipts WHERE provider=$1 AND incoming_id=$2").bind(&incoming.provider).bind(&incoming.operation_id).fetch_optional(conn).await.map_err(|e|e.to_string())?;
    let Some((actor, doc, space, action, hash, json)) = raw else {
        return Ok(None);
    };
    let r: MetadataReceipt =
        serde_json::from_str(&json).map_err(|_| "Corrupt metadata receipt; retain queued edit")?;
    if r.incoming != *incoming
        || r.actor_id != actor
        || r.space_id != space
        || doc != incoming.entity_id
        || action != incoming.action
        || hash != incoming.payload_fingerprint
        || incoming.entity_type != "document"
        || action != "update"
        || r.before.is_empty()
        || r.after.is_empty()
    {
        return Err("Metadata receipt identity differs; retain queued edit".into());
    }
    Ok(Some(r))
}
async fn replay_metadata_historical_receipt(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    op: &SyncReplayOperation,
) -> Result<bool, String> {
    let p = op
        .payload
        .as_object()
        .ok_or("Metadata payload must be an object")?;
    replay_payload_entity_id(op, p, "Document")?;
    metadata_parse(p)?;
    let incoming = replay_body_incoming(provider, op)?;
    let mut conn = state.db.acquire().await.map_err(|e| e.to_string())?;
    let Some(r) = metadata_receipt(&mut conn, &incoming).await? else {
        return Ok(false);
    };
    if r.actor_id != auth.claims.sub {
        return Err("Metadata receipt actor differs; retain queued edit".into());
    }
    if let Some(expected) = &r.rewrite {
        if crate::storage::canonical_rewrite::committed_incoming(&mut conn, auth, &incoming)
            .await
            .map_err(|e| format!("{e:?}"))?
            .as_ref()
            != Some(expected)
        {
            return Err("Metadata rewrite receipt differs; retain queued edit".into());
        }
    }

    if metadata_receipt(&mut conn, &incoming).await?.as_ref() != Some(&r) {
        return Err("Metadata receipt changed; retain queued edit".into());
    }
    crate::spaces::access::require_current_human_preference_write(&mut conn, auth)
        .await
        .map_err(|e| format!("{e:?}"))?;
    Ok(true)
}
#[cfg(test)]
thread_local! {
 static METADATA_BEFORE_LOCK:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};
 static METADATA_BEFORE_FINAL:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};
 static METADATA_COMMIT_FAULT:std::cell::RefCell<Option<ReplayCanonicalCommitFault>>=const{std::cell::RefCell::new(None)};
 static METADATA_BEFORE_CONFIRM:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};
}
async fn replay_document_update(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    use crate::spaces::access::{self, DraftOperation, PageOperation, Purpose};
    use crate::storage::canonical_rewrite::{self, RewriteIntent, RewriteReason};
    let p = operation
        .payload
        .as_object()
        .ok_or("Metadata payload must be an object")?;
    let id = replay_payload_entity_id(operation, p, "Document")?;
    let intent = metadata_parse(p)?;
    let incoming = replay_body_incoming(provider, operation)?;
    if replay_metadata_historical_receipt(state, auth, provider, operation).await? {
        return Ok("applied".into());
    }
    let principal = access::principal_from_auth(auth).map_err(|e| format!("{e:?}"))?;
    let space: Option<String> = sqlx::query_scalar(
        "SELECT space_id FROM documents WHERE id=$1 AND archived_at IS NULL AND deleted_at IS NULL",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| e.to_string())?;
    let space = space.ok_or("Page unavailable; retain queued metadata edit")?;
    if p.get("space_id")
        .is_some_and(|v| v.as_str() != Some(space.as_str()))
    {
        return Err("Page unavailable in supplied scope; retain queued edit".into());
    }
    let revision = replay_timestamp(p, "updated_at", None)?;
    #[cfg(test)]
    {
        let hook = METADATA_BEFORE_LOCK.with(|s| s.borrow_mut().take());
        if let Some(h) = hook {
            h.await;
        }
    }

    let mut tx = state.db.begin().await.map_err(|e| e.to_string())?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space])
        .await
        .map_err(|e| format!("{e:?}"))?;
    let lock=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&id).bind(&space).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    if lock.rows_affected() != 1 {
        return Err("Page unavailable; retain queued edit".into());
    }
    let root = metadata_page(&mut tx, &id).await?;
    if let Some(i) = &intent {
        if !metadata_same_revision(&i.source.page_updated_at, &root.updated_at)
            || !metadata_same_optional_revision(&i.source.published_at, &root.published_at)
        {
            return Err("Page source changed; review queued metadata edit".into());
        }
    }
    let includes = |key: &str| {
        intent
            .as_ref()
            .map_or(p.contains_key(key), |i| i.fields.iter().any(|f| f == key))
    };
    let title = if includes("title") {
        p.get("title")
            .and_then(Value::as_str)
            .ok_or("Metadata title must be a string")?
            .to_string()
    } else {
        root.title.clone()
    };
    if title.len() > 500 {
        return Err("Document title must be at most 500 characters".into());
    }
    let nullable = |key: &str, current: &Option<String>| -> Result<Option<String>, String> {
        Ok(if includes(key) {
            metadata_nullable(p, key)?.ok_or("Intended nullable field must be explicit")?
        } else {
            current.clone()
        })
    };
    let icon = nullable("icon", &root.icon)?;
    let cover = nullable("cover_image", &root.cover_image)?;
    let folder = nullable("folder_id", &root.folder_id)?;
    let parent = nullable("parent_id", &root.parent_id)?;
    let moving = folder != root.folder_id || parent != root.parent_id;
    let placement_intended = intent.as_ref().is_some_and(|i| {
        i.fields
            .iter()
            .any(|f| matches!(f.as_str(), "folder_id" | "parent_id"))
    });
    let require_move = moving || placement_intended;
    let presenting = title != root.title
        || icon != root.icon
        || cover != root.cover_image
        || intent.as_ref().is_some_and(|i| {
            i.fields
                .iter()
                .any(|f| matches!(f.as_str(), "title" | "icon" | "cover_image"))
        });
    let page_op = if presenting {
        PageOperation::Edit
    } else if require_move {
        PageOperation::Move
    } else {
        PageOperation::UserPagePreference
    };
    access::resolve_page_access(&mut tx, principal, &space, &id, page_op, Purpose::Routine)
        .await
        .map_err(|e| format!("{e:?}"))?;
    if require_move {
        access::resolve_page_access(
            &mut tx,
            principal,
            &space,
            &id,
            PageOperation::Move,
            Purpose::Routine,
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
    }
    let before = if moving {
        metadata_subtree(&mut tx, &root).await?
    } else {
        vec![root.clone()]
    };
    let mut expected = before.clone();
    let mut draft_before = None;
    let mut draft_after = None;
    let mut prepared = None;
    if presenting {
        let choice = intent.as_ref().and_then(|i| i.draft.as_ref());
        if root.published_at.is_none() || choice.is_some() {
            let did=match choice{Some(MetadataDraftChoice::Exact{id,..})=>id.clone(),_=>sqlx::query_scalar::<_,String>("SELECT id FROM document_drafts WHERE document_id=$1 AND space_id=$2 AND owner_id=$3 ORDER BY updated_at DESC,id DESC LIMIT 1").bind(&id).bind(&space).bind(&auth.claims.sub).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?.ok_or("No owned draft; supply exact editable draft context and retain queued presentation")?};
            if sqlx::query(
                "UPDATE document_drafts SET id=id WHERE id=$1 AND document_id=$2 AND space_id=$3",
            )
            .bind(&did)
            .bind(&id)
            .bind(&space)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected()
                != 1
            {
                return Err("Selected draft unavailable; retain queued edit".into());
            }
            access::resolve_draft_access(
                &mut tx,
                principal,
                &space,
                &id,
                &did,
                DraftOperation::Edit,
                Purpose::Routine,
            )
            .await
            .map_err(|e| format!("{e:?}"))?;
            let d = metadata_draft(&mut tx, &did).await?;
            if let Some(MetadataDraftChoice::Exact {
                updated_at,
                base_updated_at,
                ..
            }) = choice
            {
                if !metadata_same_revision(updated_at, &d.updated_at)
                    || !metadata_same_revision(base_updated_at, &d.base_updated_at)
                {
                    return Err("Selected draft source changed; retain queued edit".into());
                }
            }
            let mut after = d.clone();
            if includes("title") {
                after.title = title.clone()
            }
            if includes("icon") {
                after.icon = icon.clone()
            }
            if includes("cover_image") {
                after.cover_image = cover.clone()
            }
            after.updated_at = revision.clone();
            sqlx::query("UPDATE document_drafts SET title=$1,icon=$2,cover_image=$3,updated_at=$4 WHERE id=$5 AND document_id=$6 AND space_id=$7").bind(&after.title).bind(&after.icon).bind(&after.cover_image).bind(&revision).bind(&did).bind(&id).bind(&space).execute(&mut *tx).await.map_err(|e|e.to_string())?;
            draft_before = Some(d);
            draft_after = Some(after);
        } else {
            let bytes = crate::storage::asset_blob::read_canonical(&root.content_path)
                .map_err(|e| e.to_string())?;
            let selected: Vec<&str> = ["title", "icon", "cover_image"]
                .into_iter()
                .filter(|key| includes(key))
                .collect();
            let updated = metadata_markdown(&bytes, &title, &icon, &cover, &selected)?;
            use sha2::Digest;
            prepared = Some(
                canonical_rewrite::prepare_rewrite(
                    &mut tx,
                    auth,
                    RewriteIntent {
                        space_id: space.clone(),
                        document_id: id.clone(),
                        source_path: root.content_path.clone(),
                        source_revision: root.updated_at.clone(),
                        published_stamp: root.published_at.clone().unwrap(),
                        revision: revision.clone(),
                        source_sha256: Some(hex::encode(sha2::Sha256::digest(&bytes))),
                        reason: RewriteReason::MetadataPresentation,
                        incoming: incoming.clone(),
                    },
                    &updated,
                )
                .await
                .map_err(|e| format!("{e:?}"))?,
            );
        }
    }
    if moving {
        crate::spaces::documents::apply_replay_document_placement(
            &mut tx,
            auth,
            &space,
            &id,
            if folder != root.folder_id {
                Some(folder.clone())
            } else {
                None
            },
            if parent != root.parent_id {
                Some(parent.clone())
            } else {
                None
            },
            &revision,
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
        for page in &mut expected {
            page.folder_id = folder.clone();
            if page.id == id {
                page.parent_id = parent.clone()
            }
            page.updated_at = revision.clone();
            page.last_updated_by = auth.claims.sub.clone();
        }
    }
    let current = metadata_page(&mut tx, &id).await?;
    let target = expected.iter_mut().find(|d| d.id == id).unwrap();
    if presenting && !(root.published_at.is_some() && draft_after.is_some()) {
        target.title = title;
        target.icon = icon;
        target.cover_image = cover;
        target.updated_at = revision.clone();
        target.last_updated_by = auth.claims.sub.clone();
        if let Some(m) = &prepared {
            target.content_path = m.path().to_owned();
        }
        let n=sqlx::query("UPDATE documents SET title=$1,icon=$2,cover_image=$3,content_path=$4,updated_at=$5,last_updated_by=$6 WHERE id=$7 AND space_id=$8 AND content_path=$9 AND updated_at=$10 AND (published_at=$11 OR(published_at IS NULL AND $11 IS NULL)) AND archived_at IS NULL AND deleted_at IS NULL").bind(&target.title).bind(&target.icon).bind(&target.cover_image).bind(&target.content_path).bind(&revision).bind(&auth.claims.sub).bind(&id).bind(&space).bind(&current.content_path).bind(&current.updated_at).bind(&root.published_at).execute(&mut *tx).await.map_err(|e|e.to_string())?;
        if n.rows_affected() != 1 {
            return Err("Page source changed; retain queued edit".into());
        }
    }
    let r = MetadataReceipt {
        incoming: incoming.clone(),
        actor_id: auth.claims.sub.clone(),
        space_id: space.clone(),
        before,
        after: expected.clone(),
        draft_before,
        draft_after: draft_after.clone(),
        rewrite: prepared.clone(),
    };
    sqlx::query("INSERT INTO document_metadata_replay_receipts(provider,incoming_id,actor_id,document_id,space_id,action,payload_fingerprint,receipt_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(provider).bind(&incoming.operation_id).bind(&auth.claims.sub).bind(&id).bind(&space).bind(&incoming.action).bind(&incoming.payload_fingerprint).bind(serde_json::to_string(&r).map_err(|e|e.to_string())?).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    if let Some(m) = &prepared {
        canonical_rewrite::record_rewrite(&mut tx, auth, m)
            .await
            .map_err(|e| format!("{e:?}"))?;
    }
    if require_move {
        access::resolve_page_access(
            &mut tx,
            principal,
            &space,
            &id,
            PageOperation::Move,
            Purpose::Routine,
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
    }
    access::resolve_page_access(&mut tx, principal, &space, &id, page_op, Purpose::Routine)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let final_root = metadata_page(&mut tx, &id).await?;
    let actual = if moving {
        metadata_subtree(&mut tx, &final_root).await?
    } else {
        vec![metadata_page(&mut tx, &id).await?]
    };
    if actual != expected {
        return Err("Page/subtree source changed during metadata write; retain queued edit".into());
    }
    if let Some(d) = &draft_after {
        access::resolve_draft_access(
            &mut tx,
            principal,
            &space,
            &id,
            &d.id,
            DraftOperation::Edit,
            Purpose::Routine,
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
        if metadata_draft(&mut tx, &d.id).await? != *d {
            return Err("Selected draft changed; retain queued edit".into());
        }
    }
    if metadata_receipt(&mut tx, &incoming).await?.as_ref() != Some(&r) {
        return Err("Metadata receipt changed; retain queued edit".into());
    }
    #[cfg(test)]
    {
        let hook = METADATA_BEFORE_FINAL.with(|s| s.borrow_mut().take());
        if let Some(h) = hook {
            h.await;
        }
    }
    access::require_current_human_preference_write(&mut tx, auth)
        .await
        .map_err(|e| format!("{e:?}"))?;
    if let Some(m) = &prepared {
        canonical_rewrite::verify_prepared_files(m).map_err(|e| format!("{e:?}"))?;
    }
    #[cfg(test)]
    let fault = METADATA_COMMIT_FAULT.with(|s| s.borrow_mut().take());
    #[cfg(test)]
    let committed = if fault == Some(ReplayCanonicalCommitFault::Rollback) {
        tx.rollback().await.map_err(|e| e.to_string())?;
        Err("Injected metadata rollback".to_string())
    } else {
        tx.commit().await.map_err(|e| e.to_string()).and_then(|()| {
            if fault.is_some() {
                Err("Injected lost metadata acknowledgement".into())
            } else {
                Ok(())
            }
        })
    };
    #[cfg(not(test))]
    let committed = tx.commit().await.map_err(|e| e.to_string());
    if let Err(e) = committed {
        #[cfg(test)]
        {
            let hook = METADATA_BEFORE_CONFIRM.with(|s| s.borrow_mut().take());
            if let Some(h) = hook {
                h.await;
            }
        }

        return match replay_metadata_historical_receipt(state, auth, provider, operation).await {
            Ok(true) => Ok("applied".into()),
            Ok(false) => Err(format!(
                "Metadata commit unconfirmed; retain queued edit: {e}"
            )),
            Err(proof) => Err(format!(
                "Metadata commit indeterminate; retain queued edit: {e}; {proof}"
            )),
        };
    }
    let _ = index_replay_published_document(state, &id).await;
    Ok("applied".into())
}

// Hard deletion is a SQL operation. Retained files are deliberately not an
// erasure promise; receipt retries never repeat deletion against a reused ID.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct DocumentDeleteReceipt {
    incoming: crate::storage::canonical_rewrite::IncomingIdentity,
    actor_id: String,
    space_id: String,
    source_fingerprint: String,
    retained_paths: Vec<String>,
    created_at: String,
}
fn document_delete_unavailable() -> String {
    "Document deletion target is unavailable; retain queued deletion for review".into()
}
fn document_delete_error(error: AppError) -> String {
    match error {
        AppError::NotFound(_) | AppError::Forbidden(_) => document_delete_unavailable(),
        other => format!("{other:?}"),
    }
}
async fn deletion_receipt(
    conn: &mut sqlx::AnyConnection,
    incoming: &crate::storage::canonical_rewrite::IncomingIdentity,
) -> Result<Option<DocumentDeleteReceipt>, String> {
    let rows = delete_rows(
        conn,
        "SELECT * FROM document_delete_replay_receipts WHERE provider=$1 AND incoming_id=$2",
        &[&incoming.provider, &incoming.operation_id],
    )
    .await?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let r: DocumentDeleteReceipt = serde_json::from_str(
        row["receipt_json"]
            .as_str()
            .ok_or("Deletion receipt needs review")?,
    )
    .map_err(|_| "Deletion receipt needs review")?;
    if r.incoming != *incoming
        || incoming.entity_type != "document"
        || incoming.action != "delete"
        || r.space_id.is_empty()
        || r.source_fingerprint.len() != 64
        || row["actor_id"] != r.actor_id
        || row["entity_id"] != incoming.entity_id
        || row["action"] != incoming.action
        || row["payload_fingerprint"] != incoming.payload_fingerprint
        || row["space_id"] != r.space_id
        || row["source_fingerprint"] != r.source_fingerprint
        || row["created_at"] != r.created_at
    {
        return Err("Deletion receipt differs from original intent; retain queued deletion".into());
    }
    Ok(Some(r))
}
struct OriginalSyncDelete {
 space: String,
 target: crate::storage::attachment_erasure_plan::OriginalDeleteTarget,
}
fn document_delete_payload(operation: &SyncReplayOperation) -> Result<OriginalSyncDelete, String> {
 let review=|| "Original deletion identity unavailable; retain queued deletion for named review".to_owned();
 if uuid::Uuid::parse_str(&operation.id).map(|id|id.to_string()).ok().as_deref()!=Some(operation.id.as_str()){return Err(review())}
 let payload=operation.payload.as_object().ok_or_else(review)?;
 replay_payload_entity_id(operation,payload,"Document")?;
 if payload.keys().any(|k|!matches!(k.as_str(),"id"|"space_id"|"original_target")){return Err(review())}
 let space=payload.get("space_id").and_then(Value::as_str).filter(|v|!v.trim().is_empty()&&v.len()<=4096).ok_or_else(review)?.to_owned();
 let target=serde_json::from_value(payload.get("original_target").cloned().ok_or_else(review)?).map_err(|_|review())?;
 let target:crate::storage::attachment_erasure_plan::OriginalDeleteTarget=target;
 target.validate(false).map_err(document_delete_error)?;
 Ok(OriginalSyncDelete{space,target})
}
async fn delete_search_if_still_absent(
    state: &AppState,
    space: &str,
    id: &str,
) -> Result<(), String> {
    let mut tx = state.db.begin().await.map_err(|e| e.to_string())?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx)
        .await
        .map_err(document_delete_error)?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &[space])
        .await
        .map_err(document_delete_error)?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM documents WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    // Adopted creation shares the audience guard. Keep it held across the
    // synchronous advisory index change; a receipt never owns a recreated ID.
    if exists.is_none() {
        state.search_index.delete(id).map_err(|e| e.to_string())?;
    }
    tx.rollback().await.map_err(|e| e.to_string())?;
    Ok(())
}
async fn replay_document_delete_historical_receipt(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    op: &SyncReplayOperation,
) -> Result<bool, String> {
    let original=document_delete_payload(op)?;
    let incoming=replay_body_incoming(provider,op)?;
    let mut tx=state.db.begin().await.map_err(|e|e.to_string())?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await.map_err(document_delete_error)?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx,&[&original.space]).await.map_err(document_delete_error)?;
    crate::spaces::access::require_current_human_preference_write(&mut tx,auth).await.map_err(document_delete_error)?;
    let receipt=deletion_receipt(&mut tx,&incoming).await?;
    if receipt.as_ref().is_some_and(|r|r.actor_id!=auth.claims.sub||r.space_id!=original.space){return Err("Deletion receipt identity collision; retain queued deletion".into())}
    let committed=crate::storage::attachment_erasure_plan::retry(&mut tx,auth,&original.space,&op.entity_id,&op.id,&original.target).await.map_err(document_delete_error)?;
    if receipt.is_some()&&!committed{return Err("Legacy deletion receipt lacks original witnessed plan; retain queued deletion for review".into())}
    if deletion_receipt(&mut tx,&incoming).await?!=receipt{return Err("Deletion receipt changed; retain queued deletion".into())}
    crate::spaces::access::require_current_human_preference_write(&mut tx,auth).await.map_err(document_delete_error)?;
    tx.commit().await.map_err(|e|e.to_string())?;
    Ok(committed)
}
async fn replay_document_delete(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    use crate::spaces::access;
    let original=document_delete_payload(operation)?;
    let id=operation.entity_id.clone();
    let space=original.space.clone();
    if replay_document_delete_historical_receipt(state,auth,provider,operation).await?{return Ok("skipped".into())}
    let incoming=replay_body_incoming(provider,operation)?;
    let mut tx = state.db.begin().await.map_err(|e| e.to_string())?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx)
        .await
        .map_err(document_delete_error)?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space])
        .await
        .map_err(document_delete_error)?;
    #[cfg(test)]
    if let Some(h) = DELETE_AFTER_GUARD.with(|s| s.borrow_mut().take()) {
        h.await;
    }
    // Recheck under the actual deletion guards; a prior receipt never owns a new row.
    if crate::storage::attachment_erasure_plan::retry(&mut tx,auth,&space,&id,&operation.id,&original.target).await.map_err(document_delete_error)? {
      access::require_current_human_preference_write(&mut tx,auth).await.map_err(document_delete_error)?;
      tx.commit().await.map_err(|e|e.to_string())?;return Ok("skipped".into());
    }
    crate::storage::attachment_erasure_plan::verify_original_target(&mut tx,&space,&id,&original.target).await.map_err(document_delete_error)?;
    let prepared = crate::spaces::document_deletion::prepare(&mut tx, auth, &space, &id, None)
        .await
        .map_err(document_delete_error)?;
    let receipt = DocumentDeleteReceipt {
        incoming: incoming.clone(),
        actor_id: auth.claims.sub.clone(),
        space_id: space.clone(),
        source_fingerprint: prepared.fingerprint().to_owned(),
        retained_paths: prepared.retained_paths().to_vec(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    sqlx::query("INSERT INTO document_delete_replay_receipts(provider,incoming_id,actor_id,entity_id,action,payload_fingerprint,space_id,source_fingerprint,created_at,receipt_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(provider).bind(&operation.id).bind(&auth.claims.sub).bind(&id).bind(&operation.action).bind(&incoming.payload_fingerprint).bind(&space).bind(&receipt.source_fingerprint).bind(&receipt.created_at).bind(serde_json::to_string(&receipt).map_err(|e|e.to_string())?).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    #[cfg(test)]
    if let Some(h) = DELETE_BEFORE_SQL.with(|s| s.borrow_mut().take()) {
        h.await;
    }
    // Capture after our non-cascading provider receipt so the original reference
    // census includes it. Persist the same UUID/witness used by REST atomically.
    let plan=crate::storage::attachment_erasure_plan::prepare(&mut tx,auth,&prepared,operation.id.clone(),&original.target).await.map_err(document_delete_error)?;
    // Shared domain rechecks source after receipt INSERT waits before deletion.
    crate::spaces::document_deletion::delete_prepared(&mut tx, auth, &prepared)
        .await
        .map_err(document_delete_error)?;
    crate::storage::attachment_erasure_plan::verify(&mut tx,&plan).await.map_err(document_delete_error)?;
    // The original target no longer exists. Preserve its already consumed actual
    // source authorization through exact unchanged actor/ancestor policy, not a
    // fabricated page or an authorization lookup against the deleted row.
    if deletion_receipt(&mut tx, &incoming).await?.as_ref() != Some(&receipt) {
        return Err("Deletion receipt changed; retain queued deletion".into());
    }
    crate::spaces::document_deletion::verify_authority(&mut tx, auth, &prepared)
        .await
        .map_err(document_delete_error)?;
    #[cfg(test)]
    if let Some(h) = DELETE_BEFORE_IDENTITY.with(|s| s.borrow_mut().take()) {
        h.await;
    }
    access::require_current_human_preference_write(&mut tx, auth)
        .await
        .map_err(document_delete_error)?;
    #[cfg(test)]
    let fault = DELETE_COMMIT_FAULT.with(|s| s.borrow_mut().take());
    #[cfg(test)]
    let committed = if fault == Some(ReplayCanonicalCommitFault::Rollback) {
        tx.rollback().await.map_err(|e| e.to_string())?;
        Err("injected deletion rollback".to_owned())
    } else {
        tx.commit().await.map_err(|e| e.to_string()).and_then(|()| {
            if fault.is_some() {
                Err("injected lost deletion acknowledgement".into())
            } else {
                Ok(())
            }
        })
    };
    #[cfg(not(test))]
    let committed = tx.commit().await.map_err(|e| e.to_string());
    if let Err(error) = committed {
        #[cfg(test)]
        if let Some(h) = DELETE_BEFORE_CONFIRM.with(|s| s.borrow_mut().take()) {
            h.await;
        }
        match replay_document_delete_historical_receipt(state, auth, provider, operation).await {
            Ok(true) => {}
            Ok(false) => return Err(format!("Deletion did not commit; files retained: {error}")),
            Err(proof) => {
                return Err(format!(
                    "Deletion outcome indeterminate; retain queued deletion and files: {proof}"
                ))
            }
        }
    }
    let _ = delete_search_if_still_absent(state, &space, &id).await;
    Ok("applied".into())
}
#[cfg(test)]
thread_local! {static DELETE_AFTER_GUARD:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};static DELETE_BEFORE_SQL:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};static DELETE_BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};static DELETE_BEFORE_CONFIRM:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};static DELETE_COMMIT_FAULT:std::cell::RefCell<Option<ReplayCanonicalCommitFault>>=const{std::cell::RefCell::new(None)};}

fn folder_replay_error(error: AppError) -> String {
    match error {
        AppError::Forbidden(_) | AppError::NotFound(_) => "Folder/page target unavailable".into(),
        other => format!("{other:?}"),
    }
}
fn folder_replay_string(
    payload: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match payload.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(format!("Folder replay {key} must be a string")),
    }
}
fn folder_replay_nullable(
    payload: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match payload.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(format!("Folder replay {key} must be a string or null")),
    }
}
fn folder_replay_required(
    payload: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, String> {
    folder_replay_string(payload, key)?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("Folder replay {key} must be a nonempty string"))
}
fn folder_replay_identity(
    operation: &SyncReplayOperation,
    payload: &serde_json::Map<String, Value>,
) -> Result<String, String> {
    let id = folder_replay_string(payload, "id")?.unwrap_or_else(|| operation.entity_id.clone());
    if id.trim().is_empty() || id != operation.entity_id {
        return Err("Folder replay identity is invalid".into());
    }
    Ok(id)
}
fn folder_replay_metadata(
    payload: &serde_json::Map<String, Value>,
    id: String,
) -> Result<crate::spaces::folders::FolderReplayMetadata, String> {
    let sort_order = match payload.get("sort_order") {
        None => None,
        Some(value) => Some(
            value
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or_else(|| "Folder replay sort_order must be a 32-bit integer".to_string())?,
        ),
    };
    let created_at = folder_replay_string(payload, "created_at")?;
    let updated_at = folder_replay_string(payload, "updated_at")?;
    for value in [created_at.as_deref(), updated_at.as_deref()]
        .into_iter()
        .flatten()
    {
        crate::spaces::routes::import_timestamp_or_now(Some(value)).map_err(folder_replay_error)?;
    }
    Ok(crate::spaces::folders::FolderReplayMetadata {
        id,
        sort_order,
        created_at,
        updated_at,
    })
}
async fn replay_folder_create(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Folder payload must be an object".to_string())?;
    let id = folder_replay_identity(operation, payload)?;
    let space = folder_replay_required(payload, "space_id")?;
    let name = folder_replay_required(payload, "name")?;
    validate_replay_name(&name, "Folder name", 100)?;
    let import = folder_replay_metadata(payload, id)?;
    let req = crate::spaces::folders::CreateFolderRequest {
        name,
        parent_id: folder_replay_nullable(payload, "parent_id")?,
        icon: folder_replay_nullable(payload, "icon")?,
        source_id: None,
        created_at: import.created_at.clone(),
        updated_at: import.updated_at.clone(),
    };
    let (_, existing) =
        crate::spaces::folders::mutate_folder_create(state, auth, space, req, Some(import))
            .await
            .map_err(folder_replay_error)?;
    Ok(if existing { "skipped" } else { "applied" }.into())
}
async fn replay_folder_update(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Folder payload must be an object".to_string())?;
    let id = folder_replay_identity(operation, payload)?;
    let space = folder_replay_required(payload, "space_id")?;
    let name = folder_replay_required(payload, "name")?;
    validate_replay_name(&name, "Folder name", 100)?;
    let import = folder_replay_metadata(payload, id.clone())?;
    let req = crate::spaces::folders::UpdateFolderRequest {
        name: Some(name),
        parent_id: folder_replay_nullable(payload, "parent_id")?,
        icon: folder_replay_nullable(payload, "icon")?,
    };
    crate::spaces::folders::mutate_folder_update(state, auth, space, id, req, Some(import))
        .await
        .map_err(folder_replay_error)?;
    Ok("applied".into())
}
async fn replay_folder_delete(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Folder payload must be an object".to_string())?;
    let id = folder_replay_identity(operation, payload)?;
    let space = folder_replay_required(payload, "space_id")?;
    crate::spaces::folders::mutate_folder_delete(
        state,
        auth,
        space,
        id,
        crate::spaces::folders::FolderMutationOrigin::Replay,
    )
    .await
    .map_err(folder_replay_error)?;
    Ok("applied".into())
}

// Tag replay adapters preserve legacy string IDs. Supplied fields must have the
// declared wire type; only an absent association space permits actual-scope fallback.
fn tag_replay_error(error: AppError) -> String {
    match error {
        AppError::NotFound(_) | AppError::Forbidden(_) => "Tag/page target unavailable".into(),
        other => format!("{other:?}"),
    }
}
fn tag_replay_string(
    payload: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match payload.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(format!("Tag replay {key} must be a string when supplied")),
    }
}
fn tag_replay_required_string(
    payload: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, String> {
    tag_replay_string(payload, key)?
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("Tag replay {key} must be a nonempty string"))
}
fn tag_replay_identity(
    operation: &SyncReplayOperation,
    payload: &serde_json::Map<String, Value>,
) -> Result<String, String> {
    let id = tag_replay_string(payload, "id")?.unwrap_or_else(|| operation.entity_id.clone());
    if id.trim().is_empty() || id != operation.entity_id {
        return Err("Tag replay identity is invalid".into());
    }
    Ok(id)
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
    let id = tag_replay_identity(operation, payload)?;
    let space = tag_replay_required_string(payload, "space_id")?;
    let name = tag_replay_required_string(payload, "name")?;
    validate_replay_name(&name, "Tag name", 100)?;
    let color = tag_replay_string(payload, "color")?.unwrap_or_else(|| "#6b7280".into());
    let created = tag_replay_string(payload, "created_at")?;
    let created_at = crate::spaces::routes::import_timestamp_or_now(created.as_deref())
        .map_err(tag_replay_error)?;
    crate::spaces::tags::mutate_tag(
        state,
        auth,
        Some(&space),
        crate::spaces::tags::TagMutationOrigin::Replay,
        crate::spaces::tags::TagMutation::Create {
            id,
            name,
            color,
            created_at,
            match_existing_timestamp: created.is_some(),
        },
    )
    .await
    .map(|result| result.replay_status().to_owned())
    .map_err(tag_replay_error)
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
    let id = tag_replay_identity(operation, payload)?;
    let space = tag_replay_required_string(payload, "space_id")?;
    let name = tag_replay_required_string(payload, "name")?;
    validate_replay_name(&name, "Tag name", 100)?;
    // Preserve the existing replay wire default; REST omitting color retains it.
    let color = Some(tag_replay_string(payload, "color")?.unwrap_or_else(|| "#6b7280".into()));
    crate::spaces::tags::mutate_tag(
        state,
        auth,
        Some(&space),
        crate::spaces::tags::TagMutationOrigin::Replay,
        crate::spaces::tags::TagMutation::Update { id, name, color },
    )
    .await
    .map(|result| result.replay_status().to_owned())
    .map_err(tag_replay_error)
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
    let id = tag_replay_identity(operation, payload)?;
    let space = tag_replay_required_string(payload, "space_id")?;
    crate::spaces::tags::mutate_tag(
        state,
        auth,
        Some(&space),
        crate::spaces::tags::TagMutationOrigin::Replay,
        crate::spaces::tags::TagMutation::Delete { id },
    )
    .await
    .map(|result| result.replay_status().to_owned())
    .map_err(tag_replay_error)
}

async fn replay_document_tag_association(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
    add: bool,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Document tag payload must be an object".to_string())?;
    let document_id = tag_replay_required_string(payload, "document_id")?;
    let tag_id = tag_replay_required_string(payload, "tag_id")?;
    if operation.entity_id != format!("{document_id}:{tag_id}") {
        return Err("Document tag replay identity is invalid".into());
    }
    let space = tag_replay_string(payload, "space_id")?;
    if space.as_ref().is_some_and(|space| space.trim().is_empty()) {
        return Err("Tag replay space_id must be a nonempty string".into());
    }
    crate::spaces::tags::mutate_tag(
        state,
        auth,
        space.as_deref(),
        crate::spaces::tags::TagMutationOrigin::Replay,
        crate::spaces::tags::TagMutation::Associate {
            document_id,
            tag_id,
            add,
        },
    )
    .await
    .map(|result| result.replay_status().to_owned())
    .map_err(tag_replay_error)
}
async fn replay_document_tag_add(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    replay_document_tag_association(state, auth, operation, true).await
}
async fn replay_document_tag_remove(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    replay_document_tag_association(state, auth, operation, false).await
}

fn template_replay_entity_id(
    operation: &SyncReplayOperation,
    payload: &serde_json::Map<String, Value>,
) -> Result<String, String> {
    if let Some(value) = payload.get("id") {
        match value {
            Value::String(id) if !id.trim().is_empty() && id==&operation.entity_id=>{},
            _=>return Err("Template payload id must be a nonempty string matching the operation identity; retain queued metadata".into()),
        }
    }
    replay_payload_entity_id(operation, payload, "Template")
}
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TemplateReplayReceipt {
    incoming: crate::storage::canonical_rewrite::IncomingIdentity,
    actor_id: String,
    space_id: String,
    outcome: String,
    proof: crate::spaces::templates::TemplateCrudProof,
}
fn template_replay_error(error: AppError) -> String {
    match error {
        AppError::Forbidden(_) | AppError::NotFound(_) => {
            "Template target unavailable; retain queued operation for review".into()
        }
        other => format!("{other:?}"),
    }
}
async fn template_receipt_current(
    conn: &mut sqlx::AnyConnection,
    r: &TemplateReplayReceipt,
) -> Result<bool, String> {
    let raw = serde_json::to_string(r).map_err(|e| e.to_string())?;
    let n:Option<i64>=sqlx::query_scalar("SELECT 1 FROM template_replay_receipts WHERE provider=$1 AND incoming_id=$2 AND actor_id=$3 AND entity_id=$4 AND action=$5 AND payload_fingerprint=$6 AND space_id=$7 AND outcome=$8 AND receipt_json=$9").bind(&r.incoming.provider).bind(&r.incoming.operation_id).bind(&r.actor_id).bind(&r.incoming.entity_id).bind(&r.incoming.action).bind(&r.incoming.payload_fingerprint).bind(&r.space_id).bind(&r.outcome).bind(raw).fetch_optional(conn).await.map_err(|e|e.to_string())?;
    Ok(n.is_some())
}
async fn replay_template_historical_receipt(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    operation: &SyncReplayOperation,
) -> Result<bool, String> {
    let mut conn = state.db.acquire().await.map_err(|e| e.to_string())?;
    crate::spaces::access::require_current_human_preference_write(&mut conn, auth)
        .await
        .map_err(template_replay_error)?;
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT receipt_json FROM template_replay_receipts WHERE provider=$1 AND incoming_id=$2",
    )
    .bind(provider)
    .bind(&operation.id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| e.to_string())?;
    let Some(raw) = raw else { return Ok(false) };
    let r: TemplateReplayReceipt = serde_json::from_str(&raw)
        .map_err(|_| "Template receipt invalid; retain queued operation for review")?;
    let payload = operation
        .payload
        .as_object()
        .ok_or("Template payload must be an object")?;
    let shape = match operation.action.as_str() {
        "create" => {
            r.proof.before.is_none()
                && r.proof
                    .after
                    .as_ref()
                    .is_some_and(|v| v.created_by == auth.claims.sub)
                && r.outcome == "applied"
        }
        "update" => {
            r.proof
                .before
                .as_ref()
                .zip(r.proof.after.as_ref())
                .is_some_and(|(a, b)| a.created_by == b.created_by && a.created_at == b.created_at)
                && r.outcome == "applied"
        }
        "delete" => {
            r.proof.after.is_none()
                && ((r.proof.before.is_some() && r.outcome == "applied")
                    || (r.proof.before.is_none() && r.outcome == "skipped"))
        }
        _ => false,
    };
    let consumer_shape = match operation.action.as_str() {
        "create" => {
            r.proof.drafts_before.is_empty()
                && r.proof.revisions_before.is_empty()
                && r.proof.drafts_after.is_empty()
                && r.proof.revisions_after.is_empty()
        }
        "update" => {
            r.proof.drafts_before == r.proof.drafts_after
                && r.proof.revisions_before == r.proof.revisions_after
        }
        "delete" => {
            r.proof.drafts_after.is_empty()
                && r.proof.revisions_after.is_empty()
                && (r.proof.before.is_some()
                    || (r.proof.drafts_before.is_empty() && r.proof.revisions_before.is_empty()))
        }
        _ => false,
    };
    let row_scope = r
        .proof
        .before
        .iter()
        .chain(r.proof.after.iter())
        .all(|v| v.id == operation.entity_id && v.space_id == r.space_id);
    let consumer_scope = r
        .proof
        .drafts_before
        .iter()
        .chain(r.proof.drafts_after.iter())
        .all(|v| v.template_id == operation.entity_id && v.space_id == r.space_id)
        && r.proof
            .revisions_before
            .iter()
            .chain(r.proof.revisions_after.iter())
            .all(|v| v.template_id == operation.entity_id && v.space_id == r.space_id);
    if r.incoming != replay_body_incoming(provider, operation)?
        || r.actor_id != auth.claims.sub
        || r.incoming.entity_type != "template"
        || payload_string(payload, "space_id").as_deref() != Some(r.space_id.as_str())
        || template_replay_entity_id(operation, payload)? != operation.entity_id
        || !shape
        || !consumer_shape
        || !row_scope
        || !consumer_scope
    {
        return Err(
            "Template receipt does not match exact incoming operation; retain queue for review"
                .into(),
        );
    }
    crate::spaces::access::require_current_human_preference_write(&mut conn, auth)
        .await
        .map_err(template_replay_error)?;
    if !template_receipt_current(&mut conn, &r).await? {
        return Err("Template receipt changed; retain queue for review".into());
    }
    Ok(true)
}
async fn replay_template_crud(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    use crate::spaces::templates::{
        apply_template_crud, finish_template_crud, template_mutation_begin, TemplateCrudOrigin,
        TemplateMutationRow,
    };
    let payload = operation
        .payload
        .as_object()
        .ok_or("Template payload must be an object")?;
    let id = template_replay_entity_id(operation, payload)?;
    let space = payload_string(payload, "space_id")
        .filter(|s| !s.trim().is_empty())
        .ok_or("Template payload requires explicit space_id")?;
    let desired = if operation.action != "delete" {
        let name = payload_string(payload, "name").ok_or("Template payload missing string name")?;
        let content =
            payload_string(payload, "content").ok_or("Template payload missing string content")?;
        let description = match payload.get("description") {
            None | Some(Value::Null) => None,
            Some(Value::String(v)) => Some(v.clone()),
            _ => {
                return Err(
                    "Template description must be a string or null; retain queued metadata".into(),
                )
            }
        };
        validate_template_replay_payload(&name, description.as_deref(), &content)?;
        let created = if operation.action == "create" {
            replay_timestamp(payload, "created_at", None)?
        } else {
            String::new()
        };
        let updated = replay_timestamp(
            payload,
            "updated_at",
            if operation.action == "create" {
                Some(&created)
            } else {
                None
            },
        )?;
        Some(TemplateMutationRow {
            id: id.clone(),
            space_id: space.clone(),
            name: name.trim().into(),
            description,
            content,
            properties: payload_json_string(payload, "properties")?,
            tags: Some(payload_json_array_string(payload, "tags")?),
            created_by: auth.claims.sub.clone(),
            created_at: created,
            updated_at: updated,
        })
    } else {
        None
    };
    if replay_template_historical_receipt(state, auth, provider, operation).await? {
        return Ok("skipped".into());
    }
    // Missing delete is observable only after current scoped management authority;
    // a globally present foreign ID is rejected by the shared core.
    let mut tx = template_mutation_begin(
        state,
        auth,
        &space,
        if operation.action == "update" {
            Some(&id)
        } else {
            None
        },
    )
    .await
    .map_err(template_replay_error)?;
    if operation.action == "delete" {
        sqlx::query("UPDATE templates SET id=id WHERE id=$1 AND space_id=$2")
            .bind(&id)
            .bind(&space)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    }
    let proof = apply_template_crud(
        &mut tx,
        auth,
        &space,
        &id,
        &operation.action,
        desired,
        TemplateCrudOrigin::Replay,
    )
    .await
    .map_err(template_replay_error)?;
    let outcome = if operation.action == "delete" && proof.before.is_none() {
        "skipped"
    } else {
        "applied"
    };
    let r = TemplateReplayReceipt {
        incoming: replay_body_incoming(provider, operation)?,
        actor_id: auth.claims.sub.clone(),
        space_id: space.clone(),
        outcome: outcome.into(),
        proof,
    };
    sqlx::query("INSERT INTO template_replay_receipts(provider,incoming_id,actor_id,entity_id,action,payload_fingerprint,space_id,outcome,receipt_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(provider).bind(&operation.id).bind(&auth.claims.sub).bind(&id).bind(&operation.action).bind(&r.incoming.payload_fingerprint).bind(&space).bind(outcome).bind(serde_json::to_string(&r).map_err(|e|e.to_string())?).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    if !template_receipt_current(&mut tx, &r).await? {
        return Err("Template receipt changed during mutation".into());
    }
    finish_template_crud(&mut tx, auth, &space, &id, &r.proof)
        .await
        .map_err(template_replay_error)?;
    #[cfg(test)]
    let fault = TEMPLATE_REPLAY_COMMIT_FAULT.with(|h| h.borrow_mut().take());
    #[cfg(test)]
    let committed = if fault == Some(ReplayCanonicalCommitFault::Rollback) {
        tx.rollback().await.map_err(|e| e.to_string())?;
        Err("injected rollback".into())
    } else {
        tx.commit().await.map_err(|e| e.to_string()).and_then(|()| {
            if fault.is_some() {
                Err("injected lost acknowledgement".into())
            } else {
                Ok(())
            }
        })
    };
    #[cfg(not(test))]
    let committed = tx.commit().await.map_err(|e| e.to_string());
    if let Err(error) = committed {
        #[cfg(test)]
        {
            let hook = TEMPLATE_REPLAY_BEFORE_CONFIRM.with(|h| h.borrow_mut().take());
            if let Some(h) = hook {
                h.await
            }
        }
        return match replay_template_historical_receipt(state,auth,provider,operation).await {
            Ok(true)=>Ok(outcome.into()),Ok(false)=>Err(format!("Template commit failed; retain queued operation: {error}")),Err(_)=>Err("Template commit outcome indeterminate; retain queued operation and receipt for review".into())
        };
    }
    Ok(outcome.into())
}

#[cfg(test)]
thread_local! {static TEMPLATE_REPLAY_COMMIT_FAULT:std::cell::RefCell<Option<ReplayCanonicalCommitFault>>=const{std::cell::RefCell::new(None)};static TEMPLATE_REPLAY_BEFORE_CONFIRM:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};}

fn property_replay_error(error: crate::auth::routes::AppError) -> String {
    match error {
        crate::auth::routes::AppError::Forbidden(_)
        | crate::auth::routes::AppError::NotFound(_) => "Property/page target unavailable".into(),
        error => format!("{error:?}"),
    }
}
fn property_replay_string(
    payload: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match payload.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(format!("Property replay {key} must be a string")),
    }
}
fn property_replay_required_string(
    payload: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, String> {
    property_replay_string(payload, key)?
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| format!("Property replay {key} must be a nonempty string"))
}
fn property_replay_identity(
    operation: &SyncReplayOperation,
    payload: &serde_json::Map<String, Value>,
) -> Result<String, String> {
    let id = property_replay_string(payload, "id")?.unwrap_or_else(|| operation.entity_id.clone());
    if id.trim().is_empty() || id != operation.entity_id {
        return Err("Property replay identity is invalid".into());
    }
    Ok(id)
}
fn property_replay_sort(payload: &serde_json::Map<String, Value>) -> Result<Option<i32>, String> {
    match payload.get("sort_order") {
        None => Ok(None),
        Some(value) => value
            .as_i64()
            .and_then(|v| i32::try_from(v).ok())
            .map(Some)
            .ok_or_else(|| "Property replay sort_order must be a 32-bit integer".into()),
    }
}
fn property_replay_schema(
    payload: &serde_json::Map<String, Value>,
) -> Result<crate::spaces::properties::CreatePropertySchemaRequest, String> {
    Ok(crate::spaces::properties::CreatePropertySchemaRequest {
        name: property_replay_required_string(payload, "name")?,
        property_type: property_replay_required_string(payload, "property_type")?,
        options: payload.get("options").filter(|v| !v.is_null()).cloned(),
        source_id: None,
        created_at: None,
    })
}

async fn replay_property_schema_create(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Property schema payload must be an object".to_string())?;
    let id = property_replay_identity(operation, payload)?;
    let space = property_replay_required_string(payload, "space_id")?;
    let req = property_replay_schema(payload)?;
    let sort = property_replay_sort(payload)?;
    let created = property_replay_string(payload, "created_at")?;
    let timestamp = crate::spaces::routes::import_timestamp_or_now(created.as_deref())
        .map_err(property_replay_error)?;
    let (_, existing) = crate::spaces::properties::mutate_property_create(
        state,
        auth,
        space,
        req,
        id,
        timestamp,
        sort,
        true,
        created.is_some(),
    )
    .await
    .map_err(property_replay_error)?;
    Ok(if existing { "skipped" } else { "applied" }.into())
}
async fn replay_property_schema_update(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Property schema payload must be an object".to_string())?;
    let id = property_replay_identity(operation, payload)?;
    let space = property_replay_required_string(payload, "space_id")?;
    let req = property_replay_schema(payload)?;
    let sort = property_replay_sort(payload)?;
    crate::spaces::properties::mutate_property_update(state, auth, space, id, req, sort)
        .await
        .map_err(property_replay_error)?;
    Ok("applied".into())
}
async fn replay_property_schema_delete(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Property schema payload must be an object".to_string())?;
    let id = property_replay_identity(operation, payload)?;
    let space = property_replay_required_string(payload, "space_id")?;
    crate::spaces::properties::mutate_property_delete(state, auth, space, id)
        .await
        .map_err(property_replay_error)?;
    Ok("applied".into())
}
async fn replay_property_schema_reorder(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Property reorder payload must be an object".to_string())?;
    let space = property_replay_required_string(payload, "space_id")?;
    if operation.entity_id != space {
        return Err("Property reorder identity must match space_id".into());
    }
    let ids = payload
        .get("property_ids")
        .and_then(Value::as_array)
        .ok_or_else(|| "Property reorder property_ids must be an array".to_string())?;
    let property_ids = ids
        .iter()
        .map(|id| {
            id.as_str()
                .filter(|v| !v.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| "Property reorder IDs must be nonempty strings".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    crate::spaces::properties::mutate_property_reorder(
        state,
        auth,
        space,
        crate::spaces::properties::ReorderPropertySchemaRequest { property_ids },
    )
    .await
    .map_err(property_replay_error)?;
    Ok("applied".into())
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
    let document_id = match payload.get("document_id") {
        None => operation.entity_id.clone(),
        Some(Value::String(id)) if !id.trim().is_empty() => id.clone(),
        _ => return Err("Document property values document_id must be a nonempty string".into()),
    };
    if document_id != operation.entity_id {
        return Err(
            "Document property values payload document_id must match operation entity_id".into(),
        );
    }
    let space_id = property_replay_required_string(payload, "space_id")?;
    let values = payload
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| "Document property values payload missing values".to_string())?;
    if values.len() > 100 {
        return Err("At most 100 property values may be supplied".into());
    }
    let mut prepared = Vec::new();
    let mut seen = HashSet::new();
    for value in values {
        let entry = value
            .as_object()
            .ok_or_else(|| "Document property value entries must be objects".to_string())?;
        let property_id = payload_string(entry, "property_id")
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| "Document property value missing property_id".to_string())?;
        if !seen.insert(property_id.clone()) {
            return Err("A property may only appear once in a batch".into());
        }
        // A malformed value must never be mistaken for an intentional clear.
        let property_value = match entry.get("value") {
            Some(Value::String(value)) => Some(value.clone()),
            Some(Value::Null) => None,
            _ => return Err("Document property value must be a string or explicit null".into()),
        };
        for field in ["id", "created_at", "updated_at"] {
            if entry
                .get(field)
                .is_some_and(|value| !value.is_null() && !value.is_string())
            {
                return Err(format!(
                    "Document property value {field} must be a string or null"
                ));
            }
        }
        let value_id =
            payload_string(entry, "id").unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        if value_id.trim().is_empty() {
            return Err("Document property value id must not be empty".into());
        }
        let created_at = replay_timestamp(entry, "created_at", None)?;
        let updated_at = replay_timestamp(entry, "updated_at", Some(&created_at))?;
        prepared.push((
            property_id,
            value_id,
            property_value,
            created_at,
            updated_at,
        ));
    }
    let prepared = prepared
        .into_iter()
        .map(|(property_id, id, value, created, updated)| {
            (
                crate::spaces::properties::PropertyValueEntry {
                    source_id: None,
                    property_id,
                    value,
                    created_at: None,
                    updated_at: None,
                },
                id,
                created,
                updated,
            )
        })
        .collect();
    crate::spaces::properties::mutate_property_values(state, auth, space_id, document_id, prepared)
        .await
        .map_err(property_replay_error)?;
    Ok("applied".into())
}

// Workflow adapters only bind typed import data. Shared REST domain functions
// own transactions, scope/participation/locks and current/precommit authority.
fn workflow_replay_error(error: AppError) -> String {
    match error {
        AppError::Forbidden(_) | AppError::NotFound(_) => "Workflow/page target unavailable".into(),
        other => format!("{other:?}"),
    }
}
fn workflow_replay_string(
    payload: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match payload.get(key) {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(format!("Workflow replay {key} must be a string")),
    }
}
fn workflow_replay_nullable_string(
    payload: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, String> {
    match payload.get(key) {
        Some(Value::Null) | None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(format!("Workflow replay {key} must be a string or null")),
    }
}
fn workflow_replay_required(
    payload: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, String> {
    workflow_replay_string(payload, key)?
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| format!("Workflow replay {key} must be a nonempty string"))
}
fn workflow_replay_identity(
    operation: &SyncReplayOperation,
    payload: &serde_json::Map<String, Value>,
) -> Result<String, String> {
    let id = workflow_replay_string(payload, "id")?.unwrap_or_else(|| operation.entity_id.clone());
    if id.trim().is_empty() || id != operation.entity_id {
        return Err("Workflow replay identity is invalid".into());
    }
    Ok(id)
}
fn workflow_replay_context(
    payload: &serde_json::Map<String, Value>,
    id: String,
) -> Result<crate::spaces::workflows::WorkflowReplayImport, String> {
    Ok(crate::spaces::workflows::WorkflowReplayImport {
        id,
        template_id: workflow_replay_string(payload, "workflow_template_id")?,
        status: workflow_replay_string(payload, "status")?,
        created_by: workflow_replay_string(payload, "created_by")?,
        created_at: workflow_replay_string(payload, "created_at")?,
        updated_at: workflow_replay_string(payload, "updated_at")?,
        event: None,
    })
}
fn workflow_replay_event(
    payload: &serde_json::Map<String, Value>,
    workflow: &str,
    document: &str,
    id: Option<&str>,
) -> Result<crate::spaces::workflows::WorkflowEventImport, String> {
    for field in ["document_workflow_id", "workflow_id"] {
        if workflow_replay_string(payload, field)?.is_some_and(|value| value != workflow) {
            return Err("Workflow event source binding is invalid".into());
        }
    }
    if workflow_replay_string(payload, "document_id")?.is_some_and(|value| value != document) {
        return Err("Workflow event source binding is invalid".into());
    }
    let event_id = workflow_replay_string(payload, "id")?
        .or_else(|| id.map(str::to_owned))
        .ok_or_else(|| "Workflow replay nested event requires a stable id".to_string())?;
    if event_id.trim().is_empty() || id.is_some_and(|id| id != event_id) {
        return Err("Workflow event identity is invalid".into());
    }
    Ok(crate::spaces::workflows::WorkflowEventImport {
        id: event_id,
        action: workflow_replay_required(payload, "action")?,
        from_stage_id: workflow_replay_nullable_string(payload, "from_stage_id")?,
        to_stage_id: workflow_replay_required(payload, "to_stage_id")?,
        note: workflow_replay_nullable_string(payload, "note")?,
        created_by: workflow_replay_string(payload, "created_by")?,
        created_at: workflow_replay_string(payload, "created_at")?,
    })
}

fn workflow_replay_nested_event(
    payload: &serde_json::Map<String, Value>,
    workflow: &str,
    document: &str,
    space: &str,
) -> Result<Option<crate::spaces::workflows::WorkflowEventImport>, String> {
    match payload.get("event") {
        None => Ok(None),
        Some(Value::Object(event)) => {
            if workflow_replay_string(event, "space_id")?.is_some_and(|value| value != space) {
                return Err("Workflow event source binding is invalid".into());
            }
            workflow_replay_event(event, workflow, document, None).map(Some)
        }
        _ => Err("Workflow event must be an object when supplied".into()),
    }
}
fn workflow_replay_template_request(
    payload: &serde_json::Map<String, Value>,
    import: &crate::spaces::workflows::WorkflowReplayImport,
) -> Result<crate::spaces::workflows::WorkflowTemplateRequest, String> {
    let stages: Vec<crate::spaces::workflows::WorkflowStage> = serde_json::from_value(
        payload
            .get("stages")
            .cloned()
            .ok_or_else(|| "Workflow stages are required".to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let approvers: Option<Vec<crate::spaces::workflows::WorkflowApprover>> =
        match payload.get("approvers") {
            None | Some(Value::Null) => None,
            Some(value) => {
                Some(serde_json::from_value(value.clone()).map_err(|error| error.to_string())?)
            }
        };
    for id in stages.iter().map(|stage| stage.id.as_deref()).chain(
        approvers
            .iter()
            .flatten()
            .map(|approver| approver.id.as_deref()),
    ) {
        if !id.is_some_and(|id| !id.is_empty() && id == id.trim()) {
            return Err("Workflow replay stages and approvers require stable nonempty ids".into());
        }
    }
    Ok(crate::spaces::workflows::WorkflowTemplateRequest {
        name: workflow_replay_required(payload, "name")?,
        description: workflow_replay_nullable_string(payload, "description")?,
        stages,
        approvers,
        source_id: Some(import.id.clone()),
        created_at: import.created_at.clone(),
        updated_at: import.updated_at.clone(),
    })
}

async fn replay_workflow_template_create(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Workflow payload must be an object".to_string())?;
    let id = workflow_replay_identity(operation, payload)?;
    let space = workflow_replay_required(payload, "space_id")?;
    let import = workflow_replay_context(payload, id)?;
    let req = workflow_replay_template_request(payload, &import)?;
    let (_, existing) =
        crate::spaces::workflows::mutate_create_template(state, auth, space, req, Some(import))
            .await
            .map_err(workflow_replay_error)?;
    Ok(if existing { "skipped" } else { "applied" }.into())
}
async fn replay_workflow_template_update(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Workflow payload must be an object".to_string())?;
    let id = workflow_replay_identity(operation, payload)?;
    let space = workflow_replay_required(payload, "space_id")?;
    let import = workflow_replay_context(payload, id.clone())?;
    let req = workflow_replay_template_request(payload, &import)?;
    crate::spaces::workflows::mutate_update_template(state, auth, space, id, req, Some(import))
        .await
        .map_err(workflow_replay_error)?;
    Ok("applied".into())
}
async fn replay_workflow_template_delete(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Workflow payload must be an object".to_string())?;
    let id = workflow_replay_identity(operation, payload)?;
    let space = workflow_replay_required(payload, "space_id")?;
    crate::spaces::workflows::mutate_delete_template(
        state,
        auth,
        space,
        id.clone(),
        Some(workflow_replay_context(payload, id)?),
    )
    .await
    .map_err(workflow_replay_error)?;
    Ok("applied".into())
}
async fn replay_document_workflow_upsert(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Workflow payload must be an object".to_string())?;
    let id = workflow_replay_identity(operation, payload)?;
    let doc = workflow_replay_required(payload, "document_id")?;
    let space = workflow_replay_required(payload, "space_id")?;
    let mut import = workflow_replay_context(payload, id.clone())?;
    let template = workflow_replay_required(payload, "workflow_template_id")?;
    let stage = workflow_replay_required(payload, "current_stage_id")?;
    import.event = workflow_replay_nested_event(payload, &id, &doc, &space)?;
    let req = crate::spaces::workflows::EnableDocumentWorkflowRequest {
        template_id: template,
        stage_id: Some(stage),
        source_id: Some(id),
        event_source_id: None,
        event_created_by: None,
        event_created_at: None,
        created_at: import.created_at.clone(),
        updated_at: import.updated_at.clone(),
    };
    crate::spaces::workflows::mutate_enable_workflow(state, auth, space, doc, req, Some(import))
        .await
        .map_err(workflow_replay_error)?;
    Ok("applied".into())
}
async fn replay_document_workflow_delete(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Workflow payload must be an object".to_string())?;
    let doc = workflow_replay_string(payload, "document_id")?
        .unwrap_or_else(|| operation.entity_id.clone());
    if doc.trim().is_empty() || doc != operation.entity_id {
        return Err("Workflow delete source binding is invalid".into());
    }
    let space = workflow_replay_required(payload, "space_id")?;
    let expected = workflow_replay_string(payload, "id")?;
    crate::spaces::workflows::mutate_disable_workflow(
        state,
        auth,
        space,
        doc,
        expected,
        crate::spaces::workflows::WorkflowMutationOrigin::Replay,
    )
    .await
    .map_err(workflow_replay_error)?;
    Ok("applied".into())
}
async fn replay_document_workflow_transition(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Workflow payload must be an object".to_string())?;
    let id = workflow_replay_identity(operation, payload)?;
    let doc = workflow_replay_required(payload, "document_id")?;
    let space = workflow_replay_required(payload, "space_id")?;
    let mut import = workflow_replay_context(payload, id.clone())?;
    import.template_id = Some(workflow_replay_required(payload, "workflow_template_id")?);
    import.event = workflow_replay_nested_event(payload, &id, &doc, &space)?;
    let stage = workflow_replay_required(payload, "current_stage_id")?;
    let req = crate::spaces::workflows::TransitionDocumentWorkflowRequest {
        stage_id: stage,
        note: import.event.as_ref().and_then(|e| e.note.clone()),
    };
    crate::spaces::workflows::mutate_transition_workflow(
        state,
        auth,
        space,
        doc,
        req,
        Some(import),
    )
    .await
    .map_err(workflow_replay_error)?;
    Ok("applied".into())
}
async fn replay_document_workflow_event(
    state: &AppState,
    auth: &AuthUser,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    let payload = operation
        .payload
        .as_object()
        .ok_or_else(|| "Workflow payload must be an object".to_string())?;
    let id = workflow_replay_identity(operation, payload)?;
    let doc = workflow_replay_required(payload, "document_id")?;
    let space = workflow_replay_required(payload, "space_id")?;
    let workflow = workflow_replay_string(payload, "document_workflow_id")?
        .or(workflow_replay_string(payload, "workflow_id")?)
        .filter(|v| !v.trim().is_empty())
        .ok_or_else(|| "Workflow event source binding is required".to_string())?;
    let event = workflow_replay_event(payload, &workflow, &doc, Some(&id))?;
    if event.action != operation.action {
        return Err("Workflow event action does not match operation".into());
    }
    let import = workflow_replay_context(payload, workflow)?;
    let req = crate::spaces::workflows::RecordDocumentWorkflowEventRequest {
        action: event.action,
        note: event.note,
        source_id: Some(id),
        created_by: event.created_by,
        created_at: event.created_at,
        from_stage_id: event.from_stage_id,
        to_stage_id: Some(event.to_stage_id),
    };
    crate::spaces::workflows::mutate_record_workflow_event(
        state,
        auth,
        space,
        doc,
        req,
        Some(import),
    )
    .await
    .map_err(workflow_replay_error)?;
    Ok("applied".into())
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AssetReplayReceipt {
    pub(crate) incoming: crate::storage::canonical_rewrite::IncomingIdentity,
    pub(crate) actor_id: String,
    pub(crate) space_id: String,
    pub(crate) document_id: String,
    pub(crate) storage_path: String,
    pub(crate) filename: String,
    pub(crate) original_name: String,
    pub(crate) mime_type: String,
    pub(crate) created_at: String,
    pub(crate) byte_length: i64,
    pub(crate) binary_sha256: String,
    pub(crate) source_kind: String,
    pub(crate) source_id: Option<String>,
    pub(crate) source_revision: Option<String>,
    pub(crate) body_sha256: Option<String>,
    /// Original native admission attempt; omitted verbatim from historical receipt JSON.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) admission_attempt_id: Option<String>,
    /// Optional reviewed transfer sidecar; historical receipt bytes omit this field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) draft_binding: Option<SyncReplayDraftBinding>,
    /// New native draft scope; legacy original receipt bytes omit this field.
    #[cfg(target_os = "linux")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) draft_scope: Option<crate::storage::attachment_upload::DraftUploadScope>,
}
pub(crate) async fn asset_receipt_current(
    conn: &mut sqlx::AnyConnection,
    r: &AssetReplayReceipt,
) -> Result<bool, String> {
    let raw = serde_json::to_string(r).map_err(|e| e.to_string())?;
    let row:Option<i64>=sqlx::query_scalar("SELECT 1 FROM asset_replay_receipts WHERE provider=$1 AND incoming_id=$2 AND actor_id=$3 AND entity_type=$4 AND entity_id=$5 AND action=$6 AND payload_fingerprint=$7 AND space_id=$8 AND document_id=$9 AND storage_path=$10 AND filename=$11 AND original_name=$12 AND mime_type=$13 AND byte_length=$14 AND binary_sha256=$15 AND source_kind=$16 AND (source_id=$17 OR (source_id IS NULL AND $17 IS NULL)) AND (source_revision=$18 OR (source_revision IS NULL AND $18 IS NULL)) AND (body_sha256=$19 OR (body_sha256 IS NULL AND $19 IS NULL)) AND receipt_json=$20 AND created_at=$21")
 .bind(&r.incoming.provider).bind(&r.incoming.operation_id).bind(&r.actor_id).bind(&r.incoming.entity_type).bind(&r.incoming.entity_id).bind(&r.incoming.action).bind(&r.incoming.payload_fingerprint).bind(&r.space_id).bind(&r.document_id).bind(&r.storage_path).bind(&r.filename).bind(&r.original_name).bind(&r.mime_type).bind(r.byte_length).bind(&r.binary_sha256).bind(&r.source_kind).bind(&r.source_id).bind(&r.source_revision).bind(&r.body_sha256).bind(raw).bind(&r.created_at).fetch_optional(conn).await.map_err(|e|e.to_string())?;
    Ok(row.is_some())
}
async fn replay_asset_historical_receipt(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    op: &SyncReplayOperation,
) -> Result<bool, String> {
    replay_asset_historical_receipt_bound(state, auth, provider, op, None).await
}
async fn replay_asset_historical_receipt_bound(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    op: &SyncReplayOperation,
    binding: Option<&SyncReplayDraftBinding>,
) -> Result<bool, String> {
    let incoming = replay_body_incoming(provider, op)?;
    let mut conn = state.db.acquire().await.map_err(|e| e.to_string())?;
    let actor = crate::spaces::access::require_current_human_preference_write(&mut conn, auth)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT receipt_json FROM asset_replay_receipts WHERE provider=$1 AND incoming_id=$2",
    )
    .bind(provider)
    .bind(&op.id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| e.to_string())?;
    let Some(raw) = raw else { return Ok(false) };
    let r: AssetReplayReceipt =
        serde_json::from_str(&raw).map_err(|_| "Asset receipt requires review")?;
    if r.incoming != incoming
        || r.actor_id != actor
        || r.incoming.entity_type != "asset"
        || r.incoming.action != "create"
        || r.byte_length <= 0
        || r.byte_length > 100 * 1024 * 1024
        || r.binary_sha256.len() != 64
        || !matches!(r.source_kind.as_str(), "published" | "draft" | "unchanged")
        || r.draft_binding.as_ref() != binding
    {
        return Err("Asset receipt identity differs; retain the queued upload for review".into());
    }
    crate::spaces::access::require_current_human_preference_write(&mut conn, auth)
        .await
        .map_err(|e| format!("{e:?}"))?;
    if !asset_receipt_current(&mut conn, &r).await? {
        return Err("Asset receipt changed; retain queued upload for review".into());
    }
    #[cfg(target_os = "linux")]
    if r.draft_scope.is_some() {
        crate::spaces::asset_audiences::verify_upload_scope(&mut conn,auth,&r.space_id,&r.document_id,r.draft_scope.as_ref()).await.map_err(|e|format!("{e:?}"))?;
        crate::spaces::asset_audiences::admit_replay(&mut conn,&r,false).await.map_err(|e|format!("{e:?}"))?;
        crate::spaces::access::require_current_human_preference_write(&mut conn,auth).await.map_err(|e|format!("{e:?}"))?;
        if !asset_receipt_current(&mut conn,&r).await? {return Err("Original scoped replay receipt changed".into());}
    }
    Ok(true)
}
async fn insert_asset_receipt(
    conn: &mut sqlx::AnyConnection,
    r: &AssetReplayReceipt,
) -> Result<(), String> {
    sqlx::query("INSERT INTO asset_replay_receipts(provider,incoming_id,actor_id,entity_type,entity_id,action,payload_fingerprint,space_id,document_id,storage_path,filename,original_name,mime_type,byte_length,binary_sha256,source_kind,source_id,source_revision,body_sha256,receipt_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)")
 .bind(&r.incoming.provider).bind(&r.incoming.operation_id).bind(&r.actor_id).bind(&r.incoming.entity_type).bind(&r.incoming.entity_id).bind(&r.incoming.action).bind(&r.incoming.payload_fingerprint).bind(&r.space_id).bind(&r.document_id).bind(&r.storage_path).bind(&r.filename).bind(&r.original_name).bind(&r.mime_type).bind(r.byte_length).bind(&r.binary_sha256).bind(&r.source_kind).bind(&r.source_id).bind(&r.source_revision).bind(&r.body_sha256).bind(serde_json::to_string(r).map_err(|e|e.to_string())?).bind(&r.created_at).execute(conn).await.map_err(|e|e.to_string())?;
    Ok(())
}
pub(crate) async fn asset_association_current(
    conn: &mut sqlx::AnyConnection,
    r: &AssetReplayReceipt,
) -> Result<bool, String> {
    let row:Option<i64>=sqlx::query_scalar("SELECT 1 FROM attachments WHERE id=$1 AND document_id=$2 AND space_id=$3 AND storage_path=$4 AND filename=$5 AND original_name=$6 AND mime_type=$7 AND size_bytes=$8 AND created_by=$9 AND created_at=$10")
 .bind(&r.incoming.entity_id).bind(&r.document_id).bind(&r.space_id).bind(&r.storage_path).bind(&r.filename).bind(&r.original_name).bind(&r.mime_type).bind(r.byte_length).bind(&r.actor_id).bind(&r.created_at).fetch_optional(conn).await.map_err(|e|e.to_string())?;
    Ok(row.is_some())
}

async fn replay_asset_create(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    replay_asset_create_bound(state, auth, provider, operation, None).await
}
async fn replay_asset_create_bound(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    operation: &SyncReplayOperation,
    binding: Option<&SyncReplayDraftBinding>,
) -> Result<String, String> {
    use crate::spaces::access::{self, DraftOperation, PageOperation, Purpose};
    use crate::storage::{
        asset_blob,
        canonical_rewrite::{self, RewriteIntent, RewriteReason},
    };
    use sha2::{Digest, Sha256};
    let payload = operation
        .payload
        .as_object()
        .ok_or("Asset create payload must be an object")?;
    let asset_id = replay_payload_entity_id(operation, payload, "Asset")?;
    let original_document_id =
        payload_string(payload, "document_id").ok_or("Asset payload missing document_id")?;
    let original_space_id =
        payload_string(payload, "space_id").ok_or("Asset payload missing space_id")?;
    let original_name = payload_string(payload, "name").ok_or("Asset payload missing name")?;
    let mime_type =
        payload_string(payload, "mime_type").unwrap_or_else(|| "application/octet-stream".into());
    let data_url = payload_string(payload, "data_url")
        .or_else(|| payload_string(payload, "url").filter(|v| v.starts_with("data:")))
        .or_else(|| payload_string(payload, "source_url").filter(|v| v.starts_with("data:")))
        .ok_or("Asset replay binary data is unavailable")?;
    let requested_draft = match payload.get("draft_id") {
        None => None,
        Some(Value::String(id)) if !id.trim().is_empty() => Some(id.clone()),
        _ => return Err("Asset replay draft_id must be a nonempty exact draft ID".into()),
    };
    let bytes = decode_base64_data_url(&data_url)?;
    if bytes.is_empty() || bytes.len() > 100 * 1024 * 1024 {
        return Err("Asset replay binary data must be nonempty and at most 100 MB".into());
    }
    let incoming = replay_body_incoming(provider, operation)?;
    if replay_asset_historical_receipt_bound(state, auth, provider, operation, binding).await? {
        return Ok("applied".into());
    }
    #[cfg(not(target_os = "linux"))]
    if requested_draft.is_some() {return Err("Draft-scoped asset replay requires supported native custody; retain queued bytes".into());}
    #[cfg(not(target_os = "linux"))]
    if binding.is_some() {return Err("Reviewed transfer replay requires supported native custody; retain queued bytes".into());}
    #[cfg(target_os = "linux")]
    let (space_id, document_id, draft_scope) = {
        let mut conn=state.db.acquire().await.map_err(|e|e.to_string())?;
        if let Some(binding) = binding {
            let canonical_asset=uuid::Uuid::parse_str(&asset_id).is_ok_and(|v|v.to_string()==asset_id);
            if !canonical_asset && !crate::spaces::asset_audiences::opaque_replay_identity(&asset_id) {return Err("Bound draft replay requires a canonical asset UUID or a supported opaque replay identity; retain original queued bytes".into());}
            let (space, document, scope) = resolve_replay_draft_binding(&mut conn, auth, operation, payload, binding).await?;
            (space, document, Some(scope))
        } else {
            let mut headers=axum::http::HeaderMap::new();
            if let Some(id)=&requested_draft {
                if uuid::Uuid::parse_str(&asset_id).is_err() || uuid::Uuid::parse_str(&asset_id).is_ok_and(|v|v.to_string()!=asset_id) {return Err("Draft-scoped replay requires a canonical asset UUID; retain original queued bytes".into());}
                headers.insert("x-starlane-upload-draft",axum::http::HeaderValue::from_str(id).map_err(|_|"Invalid original replay draft identity")?);
            }
            let scope=crate::spaces::asset_audiences::capture_upload_scope(&mut conn,auth,&original_space_id,&original_document_id,&headers).await.map_err(|e|format!("{e:?}"))?;
            (original_space_id, original_document_id, scope)
        }
    };
    #[cfg(not(target_os = "linux"))]
    let (space_id, document_id) = (original_space_id, original_document_id);
    #[cfg(target_os = "linux")]
    let effective_draft = draft_scope.as_ref().map(|scope| scope.draft_id.clone()).or_else(|| requested_draft.clone());
    #[cfg(not(target_os = "linux"))]
    let effective_draft = requested_draft.clone();
    let principal = access::principal_from_auth(auth).map_err(|e| format!("{e:?}"))?;
    #[cfg(target_os = "linux")]
    let lease = {
        // Authorize before consuming quota; repeat under the mutation locks below.
        let mut conn = state.db.acquire().await.map_err(|e| e.to_string())?;
        access::resolve_page_access(&mut conn, principal, &space_id, &document_id,
            PageOperation::Edit, Purpose::Routine).await.map_err(|e| format!("{e:?}"))?;
        drop(conn);
        let limit = crate::storage::attachment_capacity::configured_limit().map_err(|e| e.to_string())?;
        let maximum = bytes.len() as u64;
        let producer = crate::storage::attachment_upload::admission::Producer::Replay {
            draft_scope:draft_scope.clone(),
            provider: provider.to_owned(), incoming_id: operation.id.clone(), actor_id: auth.claims.sub.clone(),
            space_id: space_id.clone(), document_id: document_id.clone(), asset_id: asset_id.clone(),
            payload_fingerprint: incoming.payload_fingerprint.clone(),
            binary_sha256: hex::encode(Sha256::digest(&bytes)), byte_length: maximum,
        };
        let root = crate::storage::data_dir();
        let held = tokio::task::spawn_blocking(move || crate::storage::attachment_upload::admission::WriterLease::begin(
            &root, &uuid::Uuid::new_v4().to_string(), producer, maximum))
            .await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        crate::storage::attachment_capacity::reserve_authorized(&state.db, &held, limit, auth).await.map_err(|e| e.to_string())?;
        held
    };
    let mut tx = state.db.begin().await.map_err(|e| e.to_string())?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await.map_err(|e|format!("{e:?}"))?;
    #[cfg(target_os="linux")]
    crate::spaces::asset_audiences::verify_upload_scope(&mut tx,auth,&space_id,&document_id,draft_scope.as_ref()).await.map_err(|e|format!("{e:?}"))?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space_id])
        .await
        .map_err(|e| format!("{e:?}"))?;
    let locked=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&document_id).bind(&space_id).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    if locked.rows_affected() != 1 {
        return Err("Asset target is unavailable".into());
    }
    access::resolve_page_access(
        &mut tx,
        principal,
        &space_id,
        &document_id,
        PageOperation::Edit,
        Purpose::Routine,
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM attachments WHERE id=$1")
        .bind(&asset_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    if exists.is_some() {
        return Err("Existing asset ID has no exact committed upload receipt; retain queued bytes for review".into());
    }
    let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1")
        .bind(&space_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    let source:(String,String,Option<String>,String)=sqlx::query_as("SELECT content_path,updated_at,published_at,last_updated_by FROM documents WHERE id=$1 AND space_id=$2").bind(&document_id).bind(&space_id).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?;
    let mut references: Vec<String> = ["data_url", "url", "source_url", "vault_path"]
        .into_iter()
        .filter_map(|k| payload_string(payload, k))
        .filter(|v| !v.trim().is_empty())
        .collect();
    references.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
    references.dedup();
    type DraftProof = (
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        String,
    );
    let mut draft: Option<(String, DraftProof)> = None;
    let current = if source.2.is_none() || effective_draft.is_some() {
        let id=match effective_draft.clone() {Some(id)=>id,None=>sqlx::query_scalar("SELECT id FROM document_drafts WHERE document_id=$1 AND space_id=$2 AND owner_id=$3 ORDER BY updated_at DESC,id DESC LIMIT 1").bind(&document_id).bind(&space_id).bind(&auth.claims.sub).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?.ok_or("No owned draft is available; supply the exact editable draft_id and retain the queued upload")?};
        let locked = sqlx::query(
            "UPDATE document_drafts SET id=id WHERE id=$1 AND document_id=$2 AND space_id=$3",
        )
        .bind(&id)
        .bind(&document_id)
        .bind(&space_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        if locked.rows_affected() != 1 {
            return Err("Asset target draft is unavailable".into());
        }
        access::resolve_draft_access(
            &mut tx,
            principal,
            &space_id,
            &document_id,
            &id,
            DraftOperation::Edit,
            Purpose::Routine,
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
        let row:DraftProof=sqlx::query_as("SELECT owner_id,title,icon,cover_image,base_updated_at,created_at,content,updated_at FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3").bind(&id).bind(&document_id).bind(&space_id).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?;
        let body = row.6.clone();
        draft = Some((id, row));
        body
    } else {
        if !source.0.starts_with(&format!("spaces/{slug}/docs/")) {
            return Err("Published source path differs from current scope".into());
        }
        String::from_utf8(asset_blob::read_canonical(&source.0).map_err(|e| e.to_string())?)
            .map_err(|_| "Published source is not UTF-8; retain the queued upload")?
    };
    let extension = original_name
        .rsplit_once('.')
        .map(|(_, v)| v)
        .filter(|v| !v.is_empty() && v.len() <= 16 && v.bytes().all(|b| b.is_ascii_alphanumeric()))
        .unwrap_or("bin");
    #[cfg(target_os = "linux")]
    let blob = {
        let root = crate::storage::data_dir();
        let slug = slug.clone();
        let document = document_id.clone();
        let extension = extension.to_owned();
        let held = lease.clone();
        // A cancelled async caller cannot make the capsule idle while a blocking writer still owns bytes.
        tokio::task::spawn_blocking(move || asset_blob::create_with_lease(&root, &slug, &document, &extension, &bytes, held))
            .await.map_err(|e| e.to_string())?.map_err(|e| format!("Exclusive admitted blob installation failed; retained evidence: {e}"))?
    };
    #[cfg(not(target_os = "linux"))]
    let blob = asset_blob::create(&crate::storage::data_dir(), &slug, &document_id, extension, &bytes)
        .map_err(|e| format!("Exclusive blob installation failed; retained unassociated evidence: {e}"))?;
    let filename = blob.path().rsplit('/').next().unwrap().to_string();
    let url = format!("/api/v1/files/{}", blob.path());
    let mut updated = current.clone();
    for reference in references {
        updated = updated.replace(&reference, &url);
    }
    let changed = updated != current;
    let revision = chrono::Utc::now().to_rfc3339();
    let mut expected_source = source.clone();
    let mut rewrite = None;
    let mut receipt = AssetReplayReceipt {
        #[cfg(target_os="linux")]
        draft_scope:draft_scope.clone(),
        incoming: incoming.clone(),
        actor_id: auth.claims.sub.clone(),
        space_id: space_id.clone(),
        document_id: document_id.clone(),
        storage_path: blob.path().to_string(),
        filename: filename.clone(),
        original_name: original_name.clone(),
        mime_type: mime_type.clone(),
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        byte_length: blob.length(),
        binary_sha256: blob.sha256().to_string(),
        source_kind: "unchanged".into(),
        source_id: None,
        source_revision: None,
        body_sha256: None,
        #[cfg(target_os = "linux")]
        admission_attempt_id: Some(lease.attempt_id().to_owned()),
        #[cfg(not(target_os = "linux"))]
        admission_attempt_id: None,
        draft_binding: binding.cloned(),
    };
    if changed {
        receipt.body_sha256 = Some(hex::encode(Sha256::digest(updated.as_bytes())));
        receipt.source_revision = Some(revision.clone());
        if let Some((id, row)) = &mut draft {
            receipt.source_kind = "draft".into();
            receipt.source_id = Some(id.clone());
            sqlx::query("UPDATE document_drafts SET content=$1,updated_at=$2 WHERE id=$3 AND document_id=$4 AND space_id=$5").bind(&updated).bind(&revision).bind(&*id).bind(&document_id).bind(&space_id).execute(&mut *tx).await.map_err(|e|e.to_string())?;
            row.6 = updated.clone();
            row.7 = revision.clone();
        } else {
            receipt.source_kind = "published".into();
            receipt.source_id = Some(document_id.clone());
            let intent = RewriteIntent {
                    space_id: space_id.clone(),
                    document_id: document_id.clone(),
                    source_path: source.0.clone(),
                    source_revision: source.1.clone(),
                    published_stamp: source.2.clone().unwrap(),
                    revision: revision.clone(),
                    source_sha256: Some(hex::encode(Sha256::digest(current.as_bytes()))),
                    reason: RewriteReason::AssetReferenceRewrite,
                    incoming,
                };
            #[cfg(target_os = "linux")]
            let prepared = canonical_rewrite::prepare_asset_replay_with_lease(&mut tx, auth, intent, updated.as_bytes(), &lease)
                .await.map_err(|e| format!("{e:?}"))?;
            #[cfg(not(target_os = "linux"))]
            let prepared = canonical_rewrite::prepare_rewrite(&mut tx, auth, intent, updated.as_bytes())
                .await.map_err(|e| format!("{e:?}"))?;
            let affected=sqlx::query("UPDATE documents SET content_path=$1,updated_at=$2,last_updated_by=$3 WHERE id=$4 AND space_id=$5 AND content_path=$6 AND updated_at=$7 AND published_at=$8 AND archived_at IS NULL AND deleted_at IS NULL").bind(prepared.path()).bind(&revision).bind(&auth.claims.sub).bind(&document_id).bind(&space_id).bind(&source.0).bind(&source.1).bind(&source.2).execute(&mut *tx).await.map_err(|e|e.to_string())?;
            if affected.rows_affected() != 1 {
                return Err("Published source changed; retain queued upload".into());
            }
            expected_source.0 = prepared.path().into();
            expected_source.1 = revision.clone();
            expected_source.3 = auth.claims.sub.clone();
            rewrite = Some(prepared);
        }
    }
    blob.verify().map_err(|e| e.to_string())?;
    sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(&asset_id).bind(&document_id).bind(&space_id).bind(&filename).bind(&original_name).bind(&mime_type).bind(blob.length()).bind(blob.path()).bind(&auth.claims.sub).bind(&receipt.created_at).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    insert_asset_receipt(&mut tx, &receipt).await?;
    #[cfg(target_os="linux")]
    crate::spaces::asset_audiences::admit_replay(&mut tx,&receipt,true).await.map_err(|e|format!("{e:?}"))?;
    #[cfg(not(target_os="linux"))]
    crate::spaces::asset_audiences::admit_legacy(&mut tx,&asset_id,&space_id,&document_id).await.map_err(|e|format!("{e:?}"))?;
    if let Some(prepared) = &rewrite {
        canonical_rewrite::record_rewrite(&mut tx, auth, prepared)
            .await
            .map_err(|e| format!("{e:?}"))?;
    }
    #[cfg(test)]
    replay_asset_before_commit().await;
    access::resolve_page_access(
        &mut tx,
        principal,
        &space_id,
        &document_id,
        PageOperation::Edit,
        Purpose::Routine,
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    if let Some((id, row)) = &draft {
        access::resolve_draft_access(
            &mut tx,
            principal,
            &space_id,
            &document_id,
            id,
            DraftOperation::Edit,
            Purpose::Routine,
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
        let after:Option<DraftProof>=sqlx::query_as("SELECT owner_id,title,icon,cover_image,base_updated_at,created_at,content,updated_at FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3").bind(id).bind(&document_id).bind(&space_id).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?;
        if after.as_ref() != Some(row) {
            return Err("Exact draft changed during upload; retain queued bytes".into());
        }
    }
    let current_source:Option<(String,String,Option<String>,String)>=sqlx::query_as("SELECT content_path,updated_at,published_at,last_updated_by FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&document_id).bind(&space_id).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?;
    let current_slug: Option<String> = sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1")
        .bind(&space_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    if current_source.as_ref() != Some(&expected_source)
        || current_slug.as_ref() != Some(&slug)
        || !asset_association_current(&mut tx, &receipt).await?
        || !asset_receipt_current(&mut tx, &receipt).await?
    {
        return Err("Asset association/source/receipt changed; retain queued upload".into());
    }
    if let Some(prepared) = &rewrite {
        canonical_rewrite::verify_prepared_files(prepared).map_err(|e| format!("{e:?}"))?;
    }
    #[cfg(target_os = "linux")]
    crate::storage::attachment_capacity::commit_replay_in(&mut tx, &lease, &receipt).await.map_err(|e| e.to_string())?;
    #[cfg(target_os="linux")]
    {
        crate::spaces::asset_audiences::verify_upload_scope(&mut tx,auth,&space_id,&document_id,draft_scope.as_ref()).await.map_err(|e|format!("{e:?}"))?;
        crate::spaces::asset_audiences::admit_replay(&mut tx,&receipt,false).await.map_err(|e|format!("{e:?}"))?;
        crate::spaces::asset_audiences::verify_upload_scope(&mut tx,auth,&space_id,&document_id,draft_scope.as_ref()).await.map_err(|e|format!("{e:?}"))?;
    }
    blob.verify().map_err(|e| e.to_string())?;
    #[cfg(test)]
    let fault = REPLAY_ASSET_COMMIT_FAULT.with(|s| s.borrow_mut().take());
    #[cfg(test)]
    let committed = if fault == Some(ReplayCanonicalCommitFault::Rollback) {
        tx.rollback().await.map_err(|e| e.to_string())?;
        Err("injected asset commit rollback".to_string())
    } else {
        tx.commit().await.map_err(|e| e.to_string()).and_then(|()| {
            if fault.is_some() {
                Err("injected lost asset acknowledgement".into())
            } else {
                Ok(())
            }
        })
    };
    #[cfg(not(test))]
    let committed = tx.commit().await.map_err(|e| e.to_string());
    if let Err(error) = committed {
        #[cfg(test)]
        {
            let hook = REPLAY_ASSET_BEFORE_CONFIRM.with(|s| s.borrow_mut().take());
            if let Some(h) = hook {
                h.await;
            }
        }
        match replay_asset_historical_receipt_bound(state, auth, provider, operation, binding).await {
            Ok(true) => {}
            Ok(false) => {
                return Err(format!(
                    "Asset commit did not apply; retained unassociated bytes: {error}"
                ))
            }
            Err(e) => {
                return Err(format!(
                    "Asset commit outcome is indeterminate; retain receipt/blob evidence: {e}"
                ))
            }
        }
    }
    let _ = index_replay_published_document(state, &document_id).await;
    #[cfg(target_os="linux")]
    if draft_scope.is_some() && !replay_asset_historical_receipt_bound(state,auth,provider,operation,binding).await? {return Err("Scoped replay outcome requires original receipt review".into());}
    Ok("applied".into())
}
#[cfg(test)]
thread_local! {static REPLAY_ASSET_COMMIT_FAULT:std::cell::RefCell<Option<ReplayCanonicalCommitFault>>=const{std::cell::RefCell::new(None)};}
#[cfg(test)]
thread_local! {static REPLAY_ASSET_BEFORE_CONFIRM:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};}
#[cfg(test)]
thread_local! {static REPLAY_ASSET_BEFORE_COMMIT:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};}
#[cfg(test)]
async fn replay_asset_before_commit() {
    let hook = REPLAY_ASSET_BEFORE_COMMIT.with(|s| s.borrow_mut().take());
    if let Some(h) = hook {
        h.await;
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct AssetDeleteReceipt {
    incoming: crate::storage::canonical_rewrite::IncomingIdentity,
    actor_id: String,
    space_id: String,
    document_id: String,
    outcome: String,
    association: Option<(String, String, String, i64, String, String, String)>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    retirement_receipt_sha256: Option<String>,
}
// filename, original_name, mime_type, size_bytes, storage_path, created_by, created_at.
type DeletedAssetAssociation = (String, String, String, i64, String, String, String);
async fn asset_delete_receipt_current(
    conn: &mut sqlx::AnyConnection,
    r: &AssetDeleteReceipt,
) -> Result<bool, String> {
    let json = serde_json::to_string(r).map_err(|e| e.to_string())?;
    let found:Option<i64>=sqlx::query_scalar("SELECT 1 FROM asset_delete_replay_receipts WHERE provider=$1 AND incoming_id=$2 AND actor_id=$3 AND entity_id=$4 AND action=$5 AND payload_fingerprint=$6 AND space_id=$7 AND document_id=$8 AND outcome=$9 AND receipt_json=$10")
    .bind(&r.incoming.provider).bind(&r.incoming.operation_id).bind(&r.actor_id).bind(&r.incoming.entity_id).bind(&r.incoming.action).bind(&r.incoming.payload_fingerprint).bind(&r.space_id).bind(&r.document_id).bind(&r.outcome).bind(json).fetch_optional(conn).await.map_err(|e|e.to_string())?;
    Ok(found.is_some())
}
/// Read-only original replay-retirement association; no current-user or physical grant.
pub(crate) async fn inspect_retirement_receipt(conn:&mut sqlx::AnyConnection,retired:&crate::storage::attachment_retirement::Retirement)->Result<(),String>{
    if retired.origin=="direct" {return Ok(())}
    if retired.origin!="replay" {return Err("Unknown retirement origin".into())}
    let (provider,op):(String,String)=serde_json::from_str(&retired.operation_id).map_err(|e|e.to_string())?;
    if provider.is_empty()||op.is_empty()||serde_json::to_string(&(&provider,&op)).map_err(|e|e.to_string())?!=retired.operation_id{return Err("Retirement replay identity differs".into())}
    let raw:String=sqlx::query_scalar("SELECT receipt_json FROM asset_delete_replay_receipts WHERE provider=$1 AND incoming_id=$2").bind(&provider).bind(&op).fetch_one(&mut *conn).await.map_err(|e|e.to_string())?;
    let r:AssetDeleteReceipt=serde_json::from_str(&raw).map_err(|e|e.to_string())?;
    use sha2::{Digest,Sha256};
    let sha=hex::encode(Sha256::digest(serde_json::to_vec(retired).map_err(|e|e.to_string())?));
    if r.incoming.provider!=provider||r.incoming.operation_id!=op||r.incoming.entity_type!="asset"||r.incoming.entity_id!=retired.attachment_id||r.incoming.action!="delete"||r.actor_id!=retired.actor_id||r.space_id!=retired.space_id||r.document_id!=retired.document_id||r.outcome!="deleted"||r.association.as_ref()!=Some(&retired.association)||r.retirement_receipt_sha256.as_deref()!=Some(sha.as_str())||!asset_delete_receipt_current(conn,&r).await? {return Err("Original replay retirement receipt differs".into())}
    Ok(())
}
async fn replay_asset_delete_historical_receipt(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    operation: &SyncReplayOperation,
) -> Result<bool, String> {
    let mut conn = state.db.acquire().await.map_err(|e| e.to_string())?;
    crate::spaces::access::require_current_human_preference_write(&mut conn, auth)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let raw:Option<String>=sqlx::query_scalar("SELECT receipt_json FROM asset_delete_replay_receipts WHERE provider=$1 AND incoming_id=$2").bind(provider).bind(&operation.id).fetch_optional(&mut *conn).await.map_err(|e|e.to_string())?;
    let Some(raw) = raw else { return Ok(false) };
    let r: AssetDeleteReceipt = serde_json::from_str(&raw)
        .map_err(|_| "Deletion receipt is invalid; retain queued deletion for review")?;
    if r.incoming != replay_body_incoming(provider, operation)?
        || r.actor_id != auth.claims.sub
        || r.incoming.entity_type != "asset"
        || r.incoming.action != "delete"
        || r.space_id.is_empty()
        || r.document_id.is_empty()
        || !matches!(
            (r.outcome.as_str(), r.association.is_some()),
            ("deleted", true) | ("observed_absent", false)
        )
    {
        return Err("Deletion receipt does not match exact incoming operation; retain queued deletion for review".into());
    }
    if let Some(expected)=&r.retirement_receipt_sha256 {
        use sha2::{Digest,Sha256};
        let retired=crate::storage::attachment_retirement::read(&mut conn,&r.incoming.entity_id).await?.ok_or("Retirement evidence absent; retain queued deletion for review")?;
        let actual=hex::encode(Sha256::digest(serde_json::to_vec(&retired).map_err(|e|e.to_string())?));
        if &actual!=expected || (r.outcome=="deleted" && (r.association.as_ref()!=Some(&retired.association) || retired.actor_id!=r.actor_id || retired.space_id!=r.space_id || retired.document_id!=r.document_id || retired.origin!="replay" || retired.operation_id!=serde_json::to_string(&(&r.incoming.provider,&r.incoming.operation_id)).map_err(|e|e.to_string())?)) {
            return Err("Retirement receipt differs; retain queued deletion for review".into());
        }
    }
    crate::spaces::access::require_current_human_preference_write(&mut conn, auth)
        .await
        .map_err(|e| format!("{e:?}"))?;
    if !asset_delete_receipt_current(&mut conn, &r).await? {
        return Err("Deletion receipt changed; retain queued deletion for review".into());
    }
    Ok(true)
}
async fn replay_asset_delete(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    operation: &SyncReplayOperation,
) -> Result<String, String> {
    use crate::spaces::access::{self, PageOperation, Purpose};
    let payload = operation.payload.as_object();
    let asset_id = match payload {
        Some(p) => replay_payload_entity_id(operation, p, "Asset")?,
        None => operation.entity_id.clone(),
    };
    let context = |name: &str| -> Result<Option<String>, String> {
        match payload.and_then(|p| p.get(name)) {
            None => Ok(None),
            Some(Value::String(v)) if !v.trim().is_empty() => Ok(Some(v.clone())),
            _ => Err("Asset deletion context must contain exact nonempty IDs".into()),
        }
    };
    let requested_space = context("space_id")?;
    let requested_doc = context("document_id")?;
    let requested_draft = context("draft_id")?;
    let explicit_context = requested_space.is_some() && requested_doc.is_some();
    if replay_asset_delete_historical_receipt(state, auth, provider, operation).await? {
        return Ok("skipped".into());
    }
    let incoming = replay_body_incoming(provider, operation)?;
    let routed: Option<(String, String)> =
        sqlx::query_as("SELECT space_id,document_id FROM attachments WHERE NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=attachments.id) AND id=$1")
            .bind(&asset_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| e.to_string())?;
    let (space_id,document_id)=match routed {
        Some((space,doc))=>{if requested_space.as_ref().is_some_and(|v|v!=&space)||requested_doc.as_ref().is_some_and(|v|v!=&doc){return Err("Asset deletion target is unavailable; retain queued deletion for review".into())}(space,doc)},
        None=>match (requested_space,requested_doc){(Some(space),Some(doc))=>(space,doc),_=>return Err("Asset deletion requires exact editable page context or a committed deletion receipt; retain queued deletion for review".into())},
    };
    #[cfg(not(target_os="linux"))]
    if requested_draft.is_some(){return Err("Draft-scoped asset deletion requires supported native custody; retain queued deletion".into());}
    #[cfg(target_os="linux")]
    let draft_scope={
        let mut headers=axum::http::HeaderMap::new();
        if let Some(id)=&requested_draft {headers.insert("x-starlane-upload-draft",axum::http::HeaderValue::from_str(id).map_err(|_|"Invalid original deletion draft identity")?);}
        let mut conn=state.db.acquire().await.map_err(|e|e.to_string())?;
        crate::spaces::asset_audiences::capture_upload_scope(&mut conn,auth,&space_id,&document_id,&headers).await.map_err(|e|format!("{e:?}"))?
    };
    let principal = access::principal_from_auth(auth).map_err(|e| format!("{e:?}"))?;
    let mut tx = state.db.begin().await.map_err(|e| e.to_string())?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await.map_err(|e|format!("{e:?}"))?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space_id])
        .await
        .map_err(|e| format!("{e:?}"))?;
    let locked=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&document_id).bind(&space_id).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    if locked.rows_affected() != 1 {
        return Err(
            "Asset deletion target is unavailable; retain queued deletion for review".into(),
        );
    }
    access::resolve_page_access(
        &mut tx,
        principal,
        &space_id,
        &document_id,
        PageOperation::Edit,
        Purpose::Routine,
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    let source:(String,String,Option<String>,String)=sqlx::query_as("SELECT content_path,updated_at,published_at,last_updated_by FROM documents WHERE id=$1 AND space_id=$2").bind(&document_id).bind(&space_id).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?;
    let locked =
        sqlx::query("UPDATE attachments SET id=id WHERE id=$1 AND document_id=$2 AND space_id=$3 AND NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=attachments.id)")
            .bind(&asset_id)
            .bind(&document_id)
            .bind(&space_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
    access::resolve_page_access(
        &mut tx,
        principal,
        &space_id,
        &document_id,
        PageOperation::Edit,
        Purpose::Routine,
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    let association:Option<DeletedAssetAssociation>=sqlx::query_as("SELECT filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at FROM attachments WHERE NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=attachments.id) AND id=$1 AND document_id=$2 AND space_id=$3").bind(&asset_id).bind(&document_id).bind(&space_id).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?;
    if (locked.rows_affected() == 1) != association.is_some() {
        return Err("Asset association changed; retain queued deletion for review".into());
    }
    if association.is_none() {
        if !explicit_context {
            return Err("Absent asset requires explicit editable page context; retain queued deletion for review".into());
        }
        let retained:Option<(String,String)>=sqlx::query_as("SELECT space_id,document_id FROM attachments WHERE id=$1").bind(&asset_id).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?;
        if retained.as_ref().is_some_and(|(s,d)|s!=&space_id||d!=&document_id) {
            return Err("Asset deletion target is unavailable; retain queued deletion for review".into());
        }
        if retained.is_some() && crate::storage::attachment_retirement::read(&mut tx,&asset_id).await?.is_none() {
            return Err("Active asset deletion source differs; retain queued deletion for review".into());
        }
    }
    let has_association:bool=sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachments WHERE id=$1 AND document_id=$2 AND space_id=$3").bind(&asset_id).bind(&document_id).bind(&space_id).fetch_one(&mut *tx).await.map_err(|e|e.to_string())?==1;
    #[cfg(target_os="linux")]
    if has_association {crate::spaces::asset_audiences::require_change(&mut tx,auth,&space_id,&document_id,&asset_id,draft_scope.as_ref()).await.map_err(|e|format!("{e:?}"))?;}
    #[cfg(not(target_os="linux"))]
    if has_association {
        let kind:Option<String>=sqlx::query_scalar("SELECT kind FROM attachment_audience_origins WHERE asset_id=$1").bind(&asset_id).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?;
        if kind.as_deref()!=Some("legacy_page_v1"){return Err("Private asset deletion requires supported native draft custody".into())}
    }
    let retirement_before=crate::storage::attachment_retirement::read(&mut tx,&asset_id).await?;
    let expected_retirement = if let Some(row) = &association {
        let retirement_operation=serde_json::to_string(&(provider,&operation.id)).map_err(|e|e.to_string())?;
        Some(crate::storage::attachment_retirement::retire(&mut tx,&asset_id,&auth.claims.sub,&space_id,&document_id,"replay",&retirement_operation,row).await?)
    } else {retirement_before.clone()};
    let outcome = if association.is_some() {
        "deleted"
    } else {
        "observed_absent"
    };
    let receipt = AssetDeleteReceipt {
        incoming,
        actor_id: auth.claims.sub.clone(),
        space_id: space_id.clone(),
        document_id: document_id.clone(),
        outcome: outcome.into(),
        association,
        retirement_receipt_sha256: expected_retirement.as_ref().map(|r|{use sha2::{Digest,Sha256};serde_json::to_vec(r).map(|raw|hex::encode(Sha256::digest(raw)))}).transpose().map_err(|e|e.to_string())?,
    };
    sqlx::query("INSERT INTO asset_delete_replay_receipts(provider,incoming_id,actor_id,entity_id,action,payload_fingerprint,space_id,document_id,outcome,receipt_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(provider).bind(&operation.id).bind(&auth.claims.sub).bind(&asset_id).bind(&operation.action).bind(&receipt.incoming.payload_fingerprint).bind(&space_id).bind(&document_id).bind(outcome).bind(serde_json::to_string(&receipt).map_err(|e|e.to_string())?).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    #[cfg(test)]
    replay_asset_delete_before_commit().await;
    access::resolve_page_access(
        &mut tx,
        principal,
        &space_id,
        &document_id,
        PageOperation::Edit,
        Purpose::Routine,
    )
    .await
    .map_err(|e| format!("{e:?}"))?;
    let current:Option<(String,String,Option<String>,String)>=sqlx::query_as("SELECT content_path,updated_at,published_at,last_updated_by FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&document_id).bind(&space_id).fetch_optional(&mut *tx).await.map_err(|e|e.to_string())?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM attachments WHERE NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=attachments.id) AND id=$1")
        .bind(&asset_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    let retirement_after=crate::storage::attachment_retirement::read(&mut tx,&asset_id).await?;
    if retirement_after!=expected_retirement {return Err("Retirement source changed; retain queued deletion for review".into());}
    if receipt.association.is_some() && !crate::storage::attachment_retirement::matches_staged_association(&mut tx,expected_retirement.as_ref().ok_or("Retirement absent")?).await? {return Err("Retirement association changed; retain queued deletion for review".into());}
    if current.as_ref() != Some(&source)
        || exists.is_some()
        || !asset_delete_receipt_current(&mut tx, &receipt).await?
    {
        return Err(
            "Deletion source/absence/receipt changed; retain queued deletion for review".into(),
        );
    }
    #[cfg(target_os="linux")]
    if has_association {crate::spaces::asset_audiences::require_change(&mut tx,auth,&space_id,&document_id,&asset_id,draft_scope.as_ref()).await.map_err(|e|format!("{e:?}"))?;}
    #[cfg(target_os="linux")]
    crate::spaces::asset_audiences::verify_upload_scope(&mut tx,auth,&space_id,&document_id,draft_scope.as_ref()).await.map_err(|e|format!("{e:?}"))?;
    #[cfg(test)]
    let fault = REPLAY_ASSET_DELETE_COMMIT_FAULT.with(|s| s.borrow_mut().take());
    #[cfg(test)]
    let committed = if fault == Some(ReplayCanonicalCommitFault::Rollback) {
        tx.rollback().await.map_err(|e| e.to_string())?;
        Err("injected deletion rollback".into())
    } else {
        tx.commit().await.map_err(|e| e.to_string()).and_then(|()| {
            if fault.is_some() {
                Err("injected lost deletion acknowledgement".into())
            } else {
                Ok(())
            }
        })
    };
    #[cfg(not(test))]
    let committed = tx.commit().await.map_err(|e| e.to_string());
    if let Err(error) = committed {
        #[cfg(test)]
        {
            let hook = REPLAY_ASSET_DELETE_BEFORE_CONFIRM.with(|s| s.borrow_mut().take());
            if let Some(h) = hook {
                h.await
            }
        }
        match replay_asset_delete_historical_receipt(state, auth, provider, operation).await {
            Ok(true) => {}
            Ok(false) => return Err(format!("Deletion did not commit; blob retained: {error}")),
            Err(proof) => {
                return Err(format!(
                    "Deletion outcome is indeterminate; retain queued deletion and blob: {proof}"
                ))
            }
        }
    }
    Ok(if outcome == "deleted" {
        "applied"
    } else {
        "skipped"
    }
    .into())
}
#[cfg(test)]
thread_local! {static REPLAY_ASSET_DELETE_COMMIT_FAULT:std::cell::RefCell<Option<ReplayCanonicalCommitFault>>=const{std::cell::RefCell::new(None)};static REPLAY_ASSET_DELETE_BEFORE_CONFIRM:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};static REPLAY_ASSET_DELETE_BEFORE_COMMIT:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};}
#[cfg(test)]
async fn replay_asset_delete_before_commit() {
    let hook = REPLAY_ASSET_DELETE_BEFORE_COMMIT.with(|s| s.borrow_mut().take());
    if let Some(h) = hook {
        h.await
    }
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

/// Replay indexes canonical content only while the current page is published and active.
/// Hold the document row through the snapshot/index update so lifecycle mutations cannot
/// expose a private revision via a delayed metadata or asset-reference replay.
// Delayed advisory indexing cannot replace newer/recreated state or borrow a
// mutable/private fallback. Failures never change canonical acknowledgement.
async fn index_source_bound_replay(state:&AppState,auth:&AuthUser,space:&str,doc:&str,expected:&Value,rewrite:&crate::storage::canonical_rewrite::PreparedRewrite,body:&str)->Result<(),String> {
    use crate::spaces::access::{self,PageOperation,Purpose};
    let mut tx=state.db.begin().await.map_err(|e|e.to_string())?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await.map_err(|e|format!("{e:?}"))?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx,&[space]).await.map_err(|e|format!("{e:?}"))?;
    sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2").bind(doc).bind(space).execute(&mut *tx).await.map_err(|e|e.to_string())?;
    crate::storage::canonical_rewrite::verify_recorded(&mut tx,rewrite).await.map_err(|e|format!("{e:?}"))?;
    access::resolve_page_access(&mut tx,access::principal_from_auth(auth).map_err(|e|format!("{e:?}"))?,space,doc,PageOperation::Edit,Purpose::Routine).await.map_err(|e|format!("{e:?}"))?;
    if crate::spaces::published_source::selected(&mut tx,space,doc).await.map_err(|e|format!("{e:?}"))?!=*expected {return Ok(());}
    access::require_current_human_preference_write(&mut tx,auth).await.map_err(|e|format!("{e:?}"))?;
    crate::storage::canonical_rewrite::verify_prepared_files(rewrite).map_err(|e|format!("{e:?}"))?;
    state.search_index.upsert(crate::search::index::DocEntry{doc_id:doc.into(),space_id:space.into(),title:expected["title"].as_str().ok_or("Published title unavailable")?.into(),body:body.into(),tags:String::new()}).map_err(|e|e.to_string())?;
    tx.rollback().await.map_err(|e|e.to_string())?;Ok(())
}
async fn index_replay_published_document(
    state: &AppState,
    document_id: &str,
) -> Result<(), String> {
    let mut tx = state.db.begin().await.map_err(|error| error.to_string())?;
    sqlx::query("UPDATE documents SET title = title WHERE id = $1")
        .bind(document_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    let published: Option<(String, String, String)> = sqlx::query_as("SELECT content_path, title, space_id FROM documents WHERE id = $1 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(document_id).fetch_optional(&mut *tx).await.map_err(|error| error.to_string())?;
    if let Some((path, title, space_id)) = published {
        match crate::storage::read_document(&path).await {
            Ok(body) => state
                .search_index
                .upsert(crate::search::index::DocEntry {
                    doc_id: document_id.to_string(),
                    space_id,
                    title,
                    body,
                    tags: String::new(),
                })
                .map_err(|error| error.to_string())?,
            Err(_) => state
                .search_index
                .delete(document_id)
                .map_err(|error| error.to_string())?,
        }
    } else {
        // Clearing an old entry also repairs previously indexed private/archived content.
        state
            .search_index
            .delete(document_id)
            .map_err(|error| error.to_string())?;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
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

async fn existing_replay_status(
    state: &AppState,
    auth: &AuthUser,
    manifest: &SyncReplayManifestRequest,
    operation: &SyncReplayOperation,
) -> Result<Option<String>, String> {
    let existing: Option<(String,String,String,String,String,String,Option<String>)> = sqlx::query_as(
        "SELECT attempted_by,provider,entity_type,entity_id,action,status,payload FROM sync_replay_operations WHERE operation_id = $1",
    ).bind(&operation.id).fetch_optional(&state.db).await.map_err(|error|error.to_string())?;
    let Some((actor, provider, entity_type, entity_id, action, status, payload)) = existing else {
        return Ok(None);
    };
    // Successful replay acknowledgment is bound to the recorded intent and actor,
    // not just a globally unique caller-supplied ID. Never disclose foreign status.
    let payload_matches = payload
        .as_deref()
        .and_then(|stored| serde_json::from_str::<Value>(stored).ok())
        .is_some_and(|stored| stored == operation.payload);
    if actor != auth.claims.sub
        || provider != manifest.provider
        || entity_type != operation.entity_type
        || entity_id != operation.entity_id
        || action != operation.action
        || !payload_matches
    {
        return Err(
            "Replay operation identity collision; submit a corrected operation with a new id"
                .into(),
        );
    }
    Ok(Some(status))
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
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
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
        "UPDATE sync_replay_operations SET status = $1, completed_at = $2, error = $3 WHERE operation_id = $4",
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
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
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
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
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
             FROM sync_replay_checkpoints WHERE manifest_id = $1 ORDER BY created_at DESC, id DESC",
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


fn payload_string(payload: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    payload.get(key).and_then(Value::as_str).map(str::to_string)
}

fn replay_payload_entity_id(
    operation: &SyncReplayOperation,
    payload: &serde_json::Map<String, Value>,
    label: &str,
) -> Result<String, String> {
    let payload_id = match payload.get("id") {
        None => operation.entity_id.clone(),
        Some(Value::String(id)) => id.clone(),
        Some(_) => return Err(format!("{label} replay payload id must be a string when supplied")),
    };
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
        published_source: None,
        operation_id: operation.id.clone(),
        entity_type: operation.entity_type.clone(),
        action: operation.action.clone(),
        status: status.to_string(),
        message,
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    let sessions = state.ws_rooms.rooms.clone();
    ws.on_upgrade(move |socket| handle_socket(socket, state, sessions))
}

async fn handle_socket(mut socket: WebSocket, state: AppState, sessions: DocSessions) {
    let auth = match socket.recv().await {
        Some(Ok(Message::Text(token))) => {
            match jwt::validate_token(&token, &state.jwt_secret, jwt::TokenType::Access) {
                Ok(claims) => match validate_ws_session(&state, &claims).await {
                    Ok(auth) => auth,
                    Err(()) => {
                        send_ws_unavailable(&mut socket).await;
                        return;
                    }
                },
                Err(_) => {
                    send_ws_unavailable(&mut socket).await;
                    return;
                }
            }
        }
        _ => return,
    };
    let user_id = auth.claims.sub.clone();
    if socket
        .send(Message::Text(
            serde_json::json!({"status":"authenticated", "user_id":user_id})
                .to_string()
                .into(),
        ))
        .await
        .is_err()
    {
        return;
    }
    let mut subscribed_doc: Option<String> = None;
    let mut rx: Option<broadcast::Receiver<SyncMessage>> = None;
    let mut revalidate = tokio::time::interval(std::time::Duration::from_secs(5));
    revalidate.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = revalidate.tick() => {
                let allowed = if let Some(doc_id) = subscribed_doc.as_deref() {
                    authorize_ws_page(&state, &auth, doc_id, false).await.map(|_| ())
                } else { authorize_ws_connection(&state, &auth, None).await };
                if allowed.is_err() { send_ws_unavailable(&mut socket).await; break; }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(sync_msg) = serde_json::from_str::<SyncMessage>(&text) {
                            match sync_msg.msg_type.as_str() {
                                "subscribe" => {
                                    let doc_id = sync_msg.doc_id;
                                    let _initial_state = match load_doc_state(&state, &auth, &doc_id).await {
                                        Ok(data) => data,
                                        Err(()) => { send_ws_unavailable(&mut socket).await; break; }
                                    };
                                    let mut sessions_w = sessions.write().await;
                                    let sender = sessions_w.entry(doc_id.clone()).or_insert_with(|| broadcast::channel(256).0);
                                    rx = Some(sender.subscribe());
                                    if let Some(previous) = subscribed_doc.as_ref().filter(|previous| *previous != &doc_id) {
                                        if sessions_w.get(previous).is_some_and(|sender| sender.receiver_count()==0) { sessions_w.remove(previous); }
                                    }
                                    subscribed_doc = Some(doc_id.clone());
                                    drop(sessions_w);
                                    // Recheck immediately before emitting the canonical state.
                                    let state_data = match load_doc_state(&state, &auth, &doc_id).await {
                                        Ok(data) => data, Err(()) => { send_ws_unavailable(&mut socket).await; break; }
                                    };
                                    if socket.send(Message::Binary(state_data.into())).await.is_err() { break; }
                                }
                                "change" | "cursor" | "presence" => {
                                    let frame = match prepare_ws_frame(&state, &auth, subscribed_doc.as_deref(), sync_msg).await {
                                        Ok(frame) => frame,
                                        Err(()) => { send_ws_unavailable(&mut socket).await; break; }
                                    };
                                    let sessions_r = sessions.read().await;
                                    if authorize_ws_page(&state, &auth, &frame.doc_id, false).await.is_err() { send_ws_unavailable(&mut socket).await; break; }
                                    if let Some(sender) = sessions_r.get(&frame.doc_id) { let _ = sender.send(frame); }
                                }
                                _ => {
                                    if authorize_ws_connection(&state, &auth, subscribed_doc.as_deref()).await.is_err() { send_ws_unavailable(&mut socket).await; break; }
                                }
                            }
                        } else if authorize_ws_connection(&state, &auth, subscribed_doc.as_deref()).await.is_err() {
                            send_ws_unavailable(&mut socket).await; break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {
                        if authorize_ws_connection(&state, &auth, subscribed_doc.as_deref()).await.is_err() { send_ws_unavailable(&mut socket).await; break; }
                    }
                    Some(Err(_)) => break,
                }
            }
            broadcast_msg = async {
                if let Some(ref mut receiver) = rx { receiver.recv().await.ok() }
                else { std::future::pending::<Option<SyncMessage>>().await }
            } => {
                if let Some(msg) = broadcast_msg {
                    if authorize_ws_outgoing(&state, &auth, subscribed_doc.as_deref(), &msg).await.is_err() {
                        send_ws_unavailable(&mut socket).await; break;
                    }
                    if msg.user_id != user_id {
                        if socket.send(Message::Text(serde_json::to_string(&msg).unwrap_or_default().into())).await.is_err() { break; }
                    }
                }
            }
        }
    }
    drop(rx);
    if let Some(doc_id) = subscribed_doc {
        let mut sessions_w = sessions.write().await;
        if sessions_w
            .get(&doc_id)
            .is_some_and(|sender| sender.receiver_count() == 0)
        {
            sessions_w.remove(&doc_id);
        }
    }
    let _ = socket.send(Message::Close(None)).await;
}

async fn send_ws_unavailable(socket: &mut WebSocket) {
    let _ = socket
        .send(Message::Text(
            serde_json::json!({"error":"unavailable"})
                .to_string()
                .into(),
        ))
        .await;
}

async fn validate_ws_session(state: &AppState, claims: &jwt::Claims) -> Result<AuthUser, ()> {
    if claims.exp <= chrono::Utc::now().timestamp() || claims.token_type != jwt::TokenType::Access {
        return Err(());
    }
    let claims = crate::auth::middleware::validate_bearer_session(state, claims.clone())
        .await
        .map_err(|_| ())?;
    Ok(AuthUser {
        claims,
        api_key_scopes: None,
    })
}

async fn authorize_ws_connection(
    state: &AppState,
    auth: &AuthUser,
    subscribed_doc: Option<&str>,
) -> Result<(), ()> {
    if let Some(doc_id) = subscribed_doc {
        authorize_ws_page(state, auth, doc_id, false)
            .await
            .map(|_| ())
    } else {
        ws_identity(state, auth).await
    }
}

/// Guest page viewing does not enroll unsupported canonical collaboration.
async fn ws_current_member(conn: &mut sqlx::AnyConnection, auth: &AuthUser) -> Result<(), ()> {
    let kind: Option<String> = sqlx::query_scalar("SELECT account_kind FROM users WHERE id=$1 AND is_active=1")
        .bind(&auth.claims.sub).fetch_optional(conn).await.map_err(|_| ())?;
    if kind.as_deref() != Some("member") { return Err(()); }
    Ok(())
}

async fn ws_identity(state: &AppState, auth: &AuthUser) -> Result<(), ()> {
    let mut conn = state.db.acquire().await.map_err(|_| ())?;
    ws_current_member(&mut conn, auth).await?;
    crate::spaces::access::require_current_human_read(&mut conn, auth)
        .await
        .map_err(|_| ())?;
    Ok(())
}

async fn ws_page_on_connection(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    document: &str,
    edit: bool,
) -> Result<(), ()> {
    use crate::spaces::access::{principal_from_auth, resolve_page_access, PageOperation, Purpose};
    let principal = principal_from_auth(auth).map_err(|_| ())?;
    resolve_page_access(
        conn,
        principal,
        space,
        document,
        PageOperation::View,
        Purpose::Routine,
    )
    .await
    .map_err(|_| ())?;
    if edit {
        resolve_page_access(
            conn,
            principal,
            space,
            document,
            PageOperation::Edit,
            Purpose::Routine,
        )
        .await
        .map_err(|_| ())?;
    }
    // View through a readable private draft never permits canonical state delivery.
    let active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(document).bind(space).fetch_one(&mut *conn).await.map_err(|_| ())?;
    if active != 1 {
        return Err(());
    }
    ws_current_member(conn, auth).await?;
    crate::spaces::access::require_current_human_read(conn, auth)
        .await
        .map_err(|_| ())?;
    Ok(())
}

/// Canonical sync has no draft identity; private SQL drafts stay separate.
async fn authorize_ws_page(
    state: &AppState,
    auth: &AuthUser,
    doc_id: &str,
    edit: bool,
) -> Result<String, ()> {
    let mut conn = state.db.acquire().await.map_err(|_| ())?;
    let space: String = sqlx::query_scalar("SELECT space_id FROM documents WHERE id=$1")
        .bind(doc_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|_| ())?
        .ok_or(())?;
    ws_page_on_connection(&mut conn, auth, &space, doc_id, edit).await?;
    Ok(space)
}

async fn authorize_ws_outgoing(
    state: &AppState,
    auth: &AuthUser,
    subscribed_doc: Option<&str>,
    frame: &SyncMessage,
) -> Result<(), ()> {
    if subscribed_doc != Some(frame.doc_id.as_str()) {
        return Err(());
    }
    authorize_ws_page(state, auth, &frame.doc_id, false)
        .await
        .map(|_| ())
}

/// Only successful committed changes produce a broadcastable frame, with a trusted actor.
async fn prepare_ws_frame(
    state: &AppState,
    auth: &AuthUser,
    subscribed_doc: Option<&str>,
    mut frame: SyncMessage,
) -> Result<SyncMessage, ()> {
    if subscribed_doc != Some(frame.doc_id.as_str()) {
        return Err(());
    }
    match frame.msg_type.as_str() {
        "change" => {
            let data = serde_json::to_vec(&frame.payload).map_err(|_| ())?;
            store_change(state, &frame.doc_id, auth, &data).await?;
        }
        "cursor" | "presence" => {
            authorize_ws_page(state, auth, &frame.doc_id, false).await?;
        }
        _ => return Err(()),
    }
    frame.user_id = auth.claims.sub.clone();
    Ok(frame)
}

async fn load_doc_state(state: &AppState, auth: &AuthUser, doc_id: &str) -> Result<Vec<u8>, ()> {
    let mut conn = state.db.acquire().await.map_err(|_| ())?;
    let source: Option<(String, String, String, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT space_id,published_at,updated_at,automerge_state FROM documents WHERE id=$1 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL"
    ).bind(doc_id).fetch_optional(&mut *conn).await.map_err(|_| ())?;
    let source = source.ok_or(())?;
    ws_page_on_connection(&mut conn, auth, &source.0, doc_id, false).await?;
    #[cfg(test)]
    ws_before_final_read_identity(state).await;
    // Row equality and identity alone do not retain the original page authority.
    ws_page_on_connection(&mut conn, auth, &source.0, doc_id, false).await?;
    let current: Option<(String, String, String, Option<Vec<u8>>)> = sqlx::query_as(
        "SELECT space_id,published_at,updated_at,automerge_state FROM documents WHERE id=$1 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL"
    ).bind(doc_id).fetch_optional(&mut *conn).await.map_err(|_| ())?;
    if current.as_ref() != Some(&source) {
        return Err(());
    }
    #[cfg(test)]
    {let pause=state.ws_rooms.final_source_pause.read().await.clone();if let Some(pause)=pause {pause.reached.notify_one();pause.release.notified().await;}}
    crate::spaces::access::require_current_human_member_read(&mut conn, auth)
        .await
        .map_err(|_| ())?;
    Ok(source.3.unwrap_or_default())
}

/// Full canonical row evidence, excluding only the two fields this operation replaces.
async fn ws_unchanged_page_source(
    conn: &mut sqlx::AnyConnection,
    document: &str,
    space: &str,
) -> Result<Value, ()> {
    use sqlx::Row;
    let row = sqlx::query("SELECT * FROM documents WHERE id=$1 AND space_id=$2")
        .bind(document)
        .bind(space)
        .fetch_optional(conn)
        .await
        .map_err(|_| ())?
        .ok_or(())?;
    let mut fields = serde_json::Map::new();
    for column in [
        "id",
        "space_id",
        "folder_id",
        "parent_id",
        "title",
        "icon",
        "cover_image",
        "content_path",
        "locked_by",
        "public_slug",
        "created_by",
        "last_updated_by",
        "created_at",
        "published_at",
        "archived_at",
        "deleted_at",
        "deleted_by",
    ] {
        fields.insert(
            column.into(),
            serde_json::to_value(row.try_get::<Option<String>, _>(column).map_err(|_| ())?)
                .map_err(|_| ())?,
        );
    }
    for column in ["locked", "public", "sort_order"] {
        fields.insert(
            column.into(),
            Value::from(row.try_get::<i64, _>(column).map_err(|_| ())?),
        );
    }
    Ok(Value::Object(fields))
}

async fn store_change(
    state: &AppState,
    doc_id: &str,
    auth: &AuthUser,
    change_data: &[u8],
) -> Result<(), ()> {
    use crate::spaces::{access, mutation_guards};
    let space: String = sqlx::query_scalar("SELECT space_id FROM documents WHERE id=$1")
        .bind(doc_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| ())?
        .ok_or(())?;
    let mut tx = state.db.begin().await.map_err(|_| ())?;
    mutation_guards::lock_spaces_for_mutation(&mut tx, &[space.as_str()])
        .await
        .map_err(|_| ())?;
    mutation_guards::require_current_page_edit(
        &mut tx,
        access::principal_from_auth(auth).map_err(|_| ())?,
        &space,
        doc_id,
    )
    .await
    .map_err(|_| ())?;
    ws_page_on_connection(&mut tx, auth, &space, doc_id, true).await?;
    let publication: String = sqlx::query_scalar("SELECT published_at FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(doc_id).bind(&space).fetch_optional(&mut *tx).await.map_err(|_| ())?.ok_or(())?;
    let unchanged = ws_unchanged_page_source(&mut tx, doc_id, &space).await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM doc_versions WHERE document_id=$1")
        .bind(doc_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| ())?;
    let max: Option<i64> =
        sqlx::query_scalar("SELECT MAX(version_number) FROM doc_versions WHERE document_id=$1")
            .bind(doc_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| ())?;
    let version = max.unwrap_or(0) + 1;
    let version_id = uuid::Uuid::new_v4().to_string();
    let timestamp = chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO doc_versions(id,document_id,version_number,change_set,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(&version_id).bind(doc_id).bind(version).bind(change_data).bind(&auth.claims.sub).bind(&timestamp).execute(&mut *tx).await.map_err(|_| ())?;
    let updated = sqlx::query("UPDATE documents SET automerge_state=$1,updated_at=$2 WHERE id=$3 AND space_id=$4 AND published_at=$5 AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(change_data).bind(&timestamp).bind(doc_id).bind(&space).bind(&publication).execute(&mut *tx).await.map_err(|_| ())?;
    if updated.rows_affected() != 1 {
        return Err(());
    }
    // Repeat typed/current-kind authority after writes and triggers, before final canonical evidence.
    ws_page_on_connection(&mut tx, auth, &space, doc_id, true).await?;
    let exact: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents d JOIN doc_versions v ON v.document_id=d.id WHERE d.id=$1 AND d.space_id=$2 AND d.published_at=$3 AND d.archived_at IS NULL AND d.deleted_at IS NULL AND d.automerge_state=$4 AND d.updated_at=$5 AND v.id=$6 AND v.version_number=$7 AND v.change_set=$4 AND v.created_by=$8 AND v.created_at=$5 AND v.lines_added=0 AND v.lines_removed=0")
        .bind(doc_id).bind(&space).bind(&publication).bind(change_data).bind(&timestamp).bind(&version_id).bind(version).bind(&auth.claims.sub).fetch_one(&mut *tx).await.map_err(|_| ())?;
    if exact != 1 {
        return Err(());
    }
    let totals: (i64, Option<i64>) = sqlx::query_as(
        "SELECT COUNT(*),MAX(version_number) FROM doc_versions WHERE document_id=$1",
    )
    .bind(doc_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| ())?;
    if totals != (count + 1, Some(version))
        || ws_unchanged_page_source(&mut tx, doc_id, &space).await? != unchanged
    {
        return Err(());
    }
    access::require_current_human_preference_write(&mut tx, auth)
        .await
        .map_err(|_| ())?;
    tx.commit().await.map_err(|_| ())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ReplayCanonicalCommitFault, DELETE_BEFORE_CONFIRM, DELETE_BEFORE_IDENTITY,
        DELETE_COMMIT_FAULT,
    };
    use super::{
        execute_replay_manifest, replay_body_incoming, replay_operation_support,
        replay_payload_entity_id, validate_replay_manifest, AssetReplayReceipt,
        ReplayOperationSupport, SyncReplayContract, SyncReplayDraftBinding,
        SyncReplayDraftBindingSidecar, SyncReplayManifestRequest, SyncReplayOperation,
    };
    use crate::auth::jwt::{Claims, TokenType};
    use crate::auth::middleware::AuthUser;
    use crate::AppState;
    use axum::{
        body::{to_bytes, Body},
        extract::State,
        http::{Request, StatusCode},
        Json,
    };
    use serde_json::{json, Value};
    use sqlx::any::AnyPoolOptions;
    use sqlx::AnyPool;
    use tower::ServiceExt;

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

    async fn authenticated_json(
        state: &AppState,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let token = crate::auth::jwt::create_token_pair_for_session(
            "user-1",
            "user@example.test",
            "member",
            &state.jwt_secret,
            Some("test-session"),
            30,
        )
        .unwrap()
        .access_token;
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("Authorization", format!("Bearer {token}"));
        if body.is_some() {
            request = request.header("Content-Type", "application/json");
        }
        let response = crate::api_routes()
            .with_state(state.clone())
            .oneshot(
                request
                    .body(body.map_or_else(Body::empty, |value| Body::from(value.to_string())))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 8_000_000).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes)
                .unwrap_or_else(|_| json!({"raw": String::from_utf8_lossy(&bytes)})),
        )
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
            ws_rooms: crate::sync::WsRooms::default(),
        }
    }

    async fn create_replay_fixture_tables(db: &AnyPool) {
        for statement in [
            "CREATE TABLE users (
                id TEXT PRIMARY KEY NOT NULL,
                email TEXT NOT NULL UNIQUE,
                display_name TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'member',
                account_kind TEXT NOT NULL DEFAULT 'member' CHECK(account_kind IN ('member','guest')),
                is_active INTEGER NOT NULL DEFAULT 1
            )",
            "CREATE TABLE spaces (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                slug TEXT NOT NULL UNIQUE,
                description TEXT,
                icon TEXT,
                accent_color TEXT,
                visibility TEXT NOT NULL DEFAULT 'private',
                trash_retention_days INTEGER NOT NULL DEFAULT 30,
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
            "CREATE TABLE groups (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL UNIQUE,
                description TEXT,
                scim_managed INTEGER NOT NULL DEFAULT 0,
                managed_space_id TEXT UNIQUE,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            "CREATE TABLE group_members (
                id TEXT PRIMARY KEY NOT NULL,
                group_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                UNIQUE(group_id, user_id)
            )",
            "CREATE TABLE space_group_permissions (
                id TEXT PRIMARY KEY NOT NULL,
                space_id TEXT NOT NULL,
                group_id TEXT NOT NULL,
                role TEXT NOT NULL DEFAULT 'viewer',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(space_id, group_id)
            )",
            "CREATE TABLE space_user_blocks (
                space_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                created_by TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (space_id, user_id)
            )",
            "CREATE TABLE space_roles (
                id TEXT PRIMARY KEY NOT NULL,
                space_id TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                permissions TEXT NOT NULL,
                created_by TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            "CREATE TABLE global_roles (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL UNIQUE,
                description TEXT NOT NULL DEFAULT '',
                permissions TEXT NOT NULL DEFAULT '[]',
                built_in INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            "CREATE TABLE group_global_roles (
                group_id TEXT NOT NULL,
                role_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                PRIMARY KEY (group_id, role_id)
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
                locked_by TEXT,
                published_at TEXT,
                archived_at TEXT,
                deleted_at TEXT
            )",
            "CREATE TABLE document_drafts (
                id TEXT PRIMARY KEY NOT NULL,
                document_id TEXT NOT NULL,
                space_id TEXT NOT NULL,
                owner_id TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                icon TEXT,
                cover_image TEXT,
                content TEXT NOT NULL DEFAULT '',
                base_updated_at TEXT NOT NULL DEFAULT '',
                created_at TEXT NOT NULL DEFAULT 'old',
                updated_at TEXT NOT NULL DEFAULT 'old'
            )",
            "CREATE TABLE document_draft_access (draft_id TEXT NOT NULL, user_id TEXT NOT NULL, role TEXT NOT NULL)",
            "CREATE TABLE doc_subscriptions (document_id TEXT NOT NULL, user_id TEXT NOT NULL, PRIMARY KEY(document_id, user_id))",
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
        sqlx::query("INSERT INTO users (id, email, display_name, role) VALUES ($1, $2, $3, $4)")
            .bind("user-1")
            .bind("user@example.test")
            .bind("Replay User")
            .bind("member")
            .execute(db)
            .await
            .expect("insert test user");
        sqlx::query(
            "INSERT INTO spaces (id, name, slug, description, icon, accent_color, visibility, created_by, created_at, updated_at)
             VALUES ($1, $2, $3, NULL, NULL, NULL, 'private', $4, $5, $5)",
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
             VALUES ($1, $2, $3, $4, $5)",
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
             VALUES ($1, $2, $3, $4, NULL, $5, $6, $6)",
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
            "INSERT INTO documents (id, space_id, folder_id, title, icon, content_path, created_by, last_updated_by, created_at, updated_at, published_at)
             VALUES ($1, $2, $3, $4, NULL, $5, $6, $6, $7, $7, $7)",
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
            draft_bindings: None,
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
            draft_bindings: None,
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
                        "options": ["Ready"],
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
            draft_bindings: None,
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
    fn replay_preflight_seals_optional_draft_bindings_to_original_asset_payloads() {
        let operation = replay_operation(
            "syncop-bound-asset",
            "2026-10-19T12:00:00Z",
            "asset",
            "11111111-1111-4111-8111-111111111111",
            "create",
            json!({
                "id": "11111111-1111-4111-8111-111111111111",
                "space_id": "local-space",
                "document_id": "local-document",
                "draft_id": "local-draft",
                "name": "Original.png",
                "data_url": "data:image/png;base64,QQ==",
                "local_draft_source": {
                    "schema_version": 1,
                    "space_id": "local-space",
                    "document_id": "local-document",
                    "draft_id": "local-draft",
                    "draft_created_at": "local-created"
                }
            }),
        );
        let fingerprint = replay_body_incoming("self-hosted", &operation)
            .unwrap()
            .payload_fingerprint;
        let binding = SyncReplayDraftBinding {
            operation_id: operation.id.clone(),
            operation_fingerprint: fingerprint,
            transfer_operation_id: "22222222-2222-4222-8222-222222222222".into(),
            transfer_request_fingerprint: "a".repeat(64),
            local_source: crate::spaces::draft_transfer::LocalSource {
                provider: "native-local".into(),
                root_identity: "root-a".into(),
                space_id: "local-space".into(),
                document_id: "local-document".into(),
                draft_id: "local-draft".into(),
                draft_created_at: "local-created".into(),
                snapshot_digest: "b".repeat(64),
            },
            target_space_id: "space-1".into(),
            target_document_id: "doc-1".into(),
            target_draft_id: "33333333-3333-4333-8333-333333333333".into(),
            target_draft_created_at: "target-created".into(),
            target_source_digest: "c".repeat(64),
        };
        let mut manifest = single_operation_manifest(operation);
        manifest.draft_bindings = Some(SyncReplayDraftBindingSidecar {
            schema_version: 1,
            binding_count: 1,
            bindings: vec![binding.clone()],
        });
        assert!(validate_replay_manifest(manifest.clone()).ready);

        manifest.draft_bindings.as_mut().unwrap().bindings[0].operation_fingerprint =
            "d".repeat(64);
        let changed = validate_replay_manifest(manifest);
        assert!(!changed.ready);
        assert!(changed
            .blockers
            .iter()
            .any(|blocker| blocker.contains("fingerprint differs")));

        let mut duplicate = single_operation_manifest(replay_operation(
            "syncop-bound-asset",
            "2026-10-19T12:00:00Z",
            "asset",
            "11111111-1111-4111-8111-111111111111",
            "create",
            json!({
                "id": "11111111-1111-4111-8111-111111111111",
                "space_id": "local-space",
                "document_id": "local-document",
                "draft_id": "local-draft",
                "name": "Original.png",
                "data_url": "data:image/png;base64,QQ==",
                "local_draft_source": {
                    "schema_version": 1,
                    "space_id": "local-space",
                    "document_id": "local-document",
                    "draft_id": "local-draft",
                    "draft_created_at": "local-created"
                }
            }),
        ));
        duplicate.draft_bindings = Some(SyncReplayDraftBindingSidecar {
            schema_version: 1,
            binding_count: 2,
            bindings: vec![binding.clone(), binding],
        });
        let duplicate = validate_replay_manifest(duplicate);
        assert!(!duplicate.ready);
        assert!(duplicate
            .blockers
            .iter()
            .any(|blocker| blocker.contains("Duplicate replay draft binding")));
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
            draft_bindings: None,
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
            draft_bindings: None,
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
            draft_bindings: None,
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
            draft_bindings: None,
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
            draft_bindings: None,
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
    fn replay_payload_entity_id_preserves_absent_legacy_ids_and_rejects_wrong_types() {
        let mut op = operation("identity", "2026-07-08T12:00:00Z", "available");
        op.entity_id = "legacy:stable-id".into();
        assert_eq!(replay_payload_entity_id(&op, json!({}).as_object().unwrap(), "Document").unwrap(), op.entity_id);
        assert_eq!(replay_payload_entity_id(&op, json!({"id": op.entity_id}).as_object().unwrap(), "Document").unwrap(), op.entity_id);
        for value in [Value::Null, json!(42), json!(true), json!([]), json!({})] {
            let error = replay_payload_entity_id(&op, json!({"id": value}).as_object().unwrap(), "Document").unwrap_err();
            assert!(error.contains("must be a string when supplied"));
        }
        for value in ["", "  ", "other-id"] {
            assert!(replay_payload_entity_id(&op, json!({"id": value}).as_object().unwrap(), "Document").is_err());
        }
    }

    #[tokio::test]
    async fn maintenance_replay_create_import_and_legacy_update_preserve_pair() {
        let _data_root=crate::storage::fixture_root::FixtureRoot::private();
        let state = property_replay_test_state().await;
        crate::storage::init_data_dir().await.unwrap();
        crate::storage::init_space_dirs("notes").await.unwrap();
        let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id='space-1'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        crate::storage::init_space_dirs(&slug).await.unwrap();
        let auth = folder_dispatch_auth(&state).await;
        let create = replay_operation(
            "maintenance-import",
            "2000-01-01T00:00:00Z",
            "document",
            "maintenance-import-page",
            "create",
            serde_json::json!({"id":"maintenance-import-page","space_id":"space-1","title":"Imported","content":"Private body","maintenance_user_id":"other-user","created_at":"2000-01-01T00:00:00Z","updated_at":"2000-01-01T00:00:00Z"}),
        );
        let m = single_operation_manifest(create.clone());
        assert_eq!(
            super::execute_replay_operation(&state, &auth, &m, &create)
                .await
                .status,
            "applied"
        );
        let page = super::metadata_page(
            &mut *state.db.acquire().await.unwrap(),
            "maintenance-import-page",
        )
        .await
        .unwrap();
        assert_eq!(page.maintenance_user_id.as_deref(), Some("other-user"));
        assert_eq!(
            super::execute_replay_operation(&state, &auth, &m, &create)
                .await
                .status,
            "skipped"
        );
        let update = replay_operation(
            "maintenance-legacy-update",
            "2000-01-01T00:00:00Z",
            "document",
            "maintenance-import-page",
            "update",
            serde_json::json!({"id":"maintenance-import-page","space_id":"space-1","title":"Updated"}),
        );
        assert!(
            super::replay_document_update(&state, &auth, "self-hosted", &update)
                .await
                .is_ok()
        );
        let page = super::metadata_page(
            &mut *state.db.acquire().await.unwrap(),
            "maintenance-import-page",
        )
        .await
        .unwrap();
        assert_eq!(page.maintenance_user_id.as_deref(), Some("other-user"));
    }
    #[tokio::test]
    async fn maintenance_replay_receipt_bridge_preserves_legacy_metadata_and_failed_work() {
        let s = property_replay_test_state().await;
        let a = folder_dispatch_auth(&s).await;
        sqlx::query("UPDATE documents SET updated_at='2000-01-01T00:00:00Z',published_at='2000-01-01T00:00:00Z' WHERE id='doc-1'").execute(&s.db).await.unwrap();
        let payload = serde_json::json!({"id":"doc-1","space_id":"space-1","maintenance":{"kind":"user","user_id":"other-user"},"source":{"page_updated_at":"2000-01-01T00:00:00Z","published_at":"2000-01-01T00:00:00Z","maintenance_user_id":null,"maintenance_group_id":null}});
        let op = replay_operation(
            "maintenance-replay",
            "2000-01-01T00:00:00Z",
            "document",
            "doc-1",
            "update_maintenance",
            payload.clone(),
        );
        let manifest = single_operation_manifest(op.clone());
        let first = super::execute_replay_operation(&s, &a, &manifest, &op).await;
        assert_eq!(first.status, "applied");
        let retry = super::execute_replay_operation(&s, &a, &manifest, &op).await;
        assert_eq!(retry.status, "skipped");
        let mut bad = payload;
        bad["maintenance"] = serde_json::json!({"kind":"user","user_id":"missing"});
        let op = replay_operation(
            "bad-maintenance",
            "2000-01-01T00:00:00Z",
            "document",
            "doc-1",
            "update_maintenance",
            bad.clone(),
        );
        let manifest = single_operation_manifest(op.clone());
        assert_eq!(
            super::execute_replay_operation(&s, &a, &manifest, &op)
                .await
                .status,
            "failed"
        );
        let stored:(String,String)=sqlx::query_as("SELECT status,payload FROM sync_replay_operations WHERE operation_id='bad-maintenance'").fetch_one(&s.db).await.unwrap();
        assert_eq!(stored.0, "failed");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&stored.1).unwrap(),
            bad
        );
        let current = super::metadata_page(&mut *s.db.acquire().await.unwrap(), "doc-1")
            .await
            .unwrap();
        assert_eq!(current.maintenance_user_id.as_deref(), Some("other-user"));
    }
    #[tokio::test]
    async fn replay_supplied_malformed_ids_cannot_mutate_or_ack_canonical_targets() {
        use crate::spaces::documents::tests::move_database_snapshot;
        let state = property_replay_test_state().await;
        let (created,deleted):(String,Option<String>)=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE id='doc-1'").fetch_one(&state.db).await.unwrap();let original=json!({"created_at":created,"deleted_at":deleted});
        let before = move_database_snapshot(&state).await;
        for value in [Value::Null, json!(42), json!(true), json!([]), json!({})] {
            for (entity, action, target) in [
                ("space", "create", "new-space"), ("space", "update", "space-1"),
                ("document", "create", "new-document"), ("document", "update", "doc-1"),
                ("document", "update_content", "doc-1"), ("document", "delete", "doc-1"),
                ("asset", "create", "new-asset"), ("asset", "delete", "missing-asset"),
            ] {
                let op = replay_operation(if entity=="document"&&action=="delete"{"cccccccc-cccc-4ccc-8ccc-cccccccccccc"}else{"malformed-id"}, "2026-07-08T12:00:00Z", entity, target, action,
                    json!({"original_target":original.clone(),"id":value,"space_id":"space-1","document_id":"doc-1","name":"Changed","title":"Changed","content":"Changed","filename":"changed.txt","content_base64":"YQ=="}));
                let result = match (entity, action) {
                    ("space", "create") => super::replay_space_create(&state, &admin_auth_user(), &op).await,
                    ("space", "update") => super::replay_space_update(&state, &admin_auth_user(), &op).await,
                    ("document", "create") => super::replay_document_create(&state, &auth_user(), "self-hosted", &op).await,
                    ("document", "update") => super::replay_document_update(&state, &auth_user(), "self-hosted", &op).await,
                    ("document", "update_content") => super::replay_document_update_content(&state, &auth_user(), "self-hosted", &op).await,
                    ("document", "delete") => super::replay_document_delete(&state, &auth_user(), "local", &op).await,
                    ("asset", "create") => super::replay_asset_create(&state, &auth_user(), "self-hosted", &op).await,
                    ("asset", "delete") => super::replay_asset_delete(&state, &auth_user(), "self-hosted", &op).await,
                    _ => unreachable!(),
                };
                assert!(result.unwrap_err().contains("must be a string when supplied"), "{entity}/{action}: {value}");
                assert_eq!(move_database_snapshot(&state).await, before, "{entity}/{action}: {value}");
            }
        }
        let op = replay_operation("malformed-transport", "2026-07-08T12:00:00Z", "document", "doc-1", "update", json!({"id":null,"space_id":"space-1","title":"Changed"}));
        let response = execute_replay_manifest(State(state.clone()), auth_user(), Json(single_operation_manifest(op))).await.unwrap().0;
        assert_eq!(response.failed_operation_count, 1);
        assert_eq!(response.applied_operation_count + response.skipped_operation_count, 0);
        assert!(response.results[0].message.contains("must be a string when supplied"));
        let domain = |snapshot: Vec<(String, Option<String>)>| snapshot.into_iter().filter(|(table,_)| !table.starts_with("sync_replay_")).collect::<Vec<_>>();
        assert_eq!(domain(move_database_snapshot(&state).await), domain(before));
    }

    #[tokio::test]
    async fn replay_supplied_identity_is_checked_before_legacy_success_acknowledgments() {
        use crate::spaces::documents::tests::move_database_snapshot;
        let state = property_replay_test_state().await;
        for status in ["applied", "skipped"] {
            for (index, (entity, action)) in [
                ("space", "create"), ("space", "update"),
                ("document", "create"), ("document", "update"),
                ("document", "update_content"), ("document", "resolve_conflict:keep-local"),
                ("document", "delete"), ("template", "create"),
                ("template", "update"), ("template", "delete"),
                ("asset", "create"), ("asset", "delete"),
            ].into_iter().enumerate() {
                let op = replay_operation(&format!("historical-{status}-{index}"), "2026-07-08T12:00:00Z", entity, "legacy-id", action, json!({"id":null,"space_id":"space-1"}));
                let manifest = single_operation_manifest(op.clone());
                super::insert_replay_ledger(&state, &auth_user(), &manifest, &op, status, None).await.unwrap();
                let before = move_database_snapshot(&state).await;
                let result = super::execute_replay_operation(&state, &auth_user(), &manifest, &op).await;
                assert_eq!(result.status, "failed", "{entity}/{action}/{status}");
                assert!(result.message.contains("must be a string when supplied"));
                assert_eq!(move_database_snapshot(&state).await, before);
            }
        }
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
        let state = property_replay_test_state().await;
        crate::storage::init_space_dirs("replay-space")
            .await
            .expect("space dirs");
        seed_document(&state.db, "doc-resolve-conflict", None, "Conflict Doc").await;
        tokio::fs::write(
            crate::storage::data_dir().join("spaces/replay-space/docs/doc-resolve-conflict.md"),
            "Original conflict body",
        )
        .await
        .unwrap();

        let original_row=crate::spaces::published_source::selected(&mut state.db.acquire().await.unwrap(),"space-1","doc-resolve-conflict").await.unwrap();
        let original_digest=crate::spaces::published_source::digest(&original_row,b"Original conflict body").unwrap();
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
                "published_source_digest":original_digest,
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
            "SELECT content_path, last_updated_by, updated_at FROM documents WHERE id = $1",
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
        let state = property_replay_test_state().await;
        sqlx::query("UPDATE users SET role='admin' WHERE id='user-1'")
            .execute(&state.db)
            .await
            .unwrap();
        let claims = Claims {
            sid: "test-session".into(),
            ..admin_auth_user().claims
        };
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let request = axum::http::Request::builder()
            .method("POST")
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();
        let actual = <AuthUser as axum::extract::FromRequestParts<AppState>>::from_request_parts(
            &mut parts, &state,
        )
        .await
        .unwrap();

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

        let response =
            execute_replay_manifest(State(state.clone()), AuthUser {
                claims: actual.claims.clone(),
                api_key_scopes: actual.api_key_scopes.clone(),
            }, Json(manifest.clone()))
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
             FROM spaces WHERE id = $1",
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
            sqlx::query_as("SELECT role FROM space_members WHERE space_id = $1 AND user_id = $2")
                .bind("space-created")
                .bind("user-1")
                .fetch_one(&state.db)
                .await
                .expect("owner membership");
        assert_eq!(owner_membership.0, "owner");

        let second_response = execute_replay_manifest(State(state), actual, Json(manifest))
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

        let space_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM spaces WHERE id = $1")
            .bind("space-member-created")
            .fetch_one(&state.db)
            .await
            .expect("space count");
        assert_eq!(space_count.0, 0);
    }

    #[tokio::test]
    async fn replay_execution_applies_space_update() {
        let state = property_replay_test_state().await;

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
            execute_replay_manifest(State(state.clone()), folder_dispatch_auth(&state).await, Json(manifest.clone()))
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
            "SELECT name, slug, description, icon, accent_color, visibility, updated_at FROM spaces WHERE id = $1",
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

        let second_response = execute_replay_manifest(State(state.clone()), folder_dispatch_auth(&state).await, Json(manifest))
            .await
            .expect("execute replay again")
            .0;
        assert_eq!(second_response.status, "completed");
        assert_eq!(second_response.applied_operation_count, 0);
        assert_eq!(second_response.skipped_operation_count, 1);
    }

    // Exercise the real upgrade and framed socket path without another client dependency.
    async fn ws_test_server(
        state: AppState,
    ) -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, super::router().with_state(state))
                .await
                .unwrap();
        });
        (address, task)
    }

    async fn ws_client_send(stream: &mut tokio::net::TcpStream, text: &str) {
        use tokio::io::AsyncWriteExt;
        let bytes = text.as_bytes();
        let mut frame = vec![0x81];
        if bytes.len() < 126 {
            frame.push(0x80 | bytes.len() as u8);
        } else {
            frame.push(0xfe);
            frame.extend_from_slice(&(bytes.len() as u16).to_be_bytes());
        }
        let mask = [31, 57, 93, 11];
        frame.extend_from_slice(&mask);
        frame.extend(bytes.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        stream.write_all(&frame).await.unwrap();
    }

    async fn ws_client_read(stream: &mut tokio::net::TcpStream) -> (u8, Vec<u8>) {
        use tokio::io::AsyncReadExt;
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            let mut header = [0; 2];
            stream.read_exact(&mut header).await.unwrap();
            assert_eq!(header[1] & 0x80, 0);
            let mut length = (header[1] & 0x7f) as usize;
            if length == 126 {
                let mut n = [0; 2];
                stream.read_exact(&mut n).await.unwrap();
                length = u16::from_be_bytes(n) as usize;
            }
            if length == 127 {
                let mut n = [0; 8];
                stream.read_exact(&mut n).await.unwrap();
                length = u64::from_be_bytes(n) as usize;
            }
            assert!(length < 1_000_000);
            let mut body = vec![0; length];
            stream.read_exact(&mut body).await.unwrap();
            (header[0] & 15, body)
        })
        .await
        .expect("websocket response")
    }

    async fn ws_client(
        address: std::net::SocketAddr,
        state: &AppState,
        user: &str,
        sid: &str,
    ) -> tokio::net::TcpStream {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
        socket.write_all(format!("GET /ws/sync HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n").as_bytes()).await.unwrap();
        let mut response = Vec::new();
        while !response.ends_with(b"\r\n\r\n") {
            let mut b = [0];
            socket.read_exact(&mut b).await.unwrap();
            response.push(b[0]);
            assert!(response.len() < 8192);
        }
        assert!(String::from_utf8(response)
            .unwrap()
            .starts_with("HTTP/1.1 101"));
        let token = crate::auth::jwt::create_token_pair_for_session(
            user,
            "ws@test",
            "member",
            &state.jwt_secret,
            Some(sid),
            30,
        )
        .unwrap()
        .access_token;
        ws_client_send(&mut socket, &token).await;
        let response: Value = serde_json::from_slice(&ws_client_read(&mut socket).await.1).unwrap();
        assert_eq!(response["status"], "authenticated");
        ws_client_send(
            &mut socket,
            &serde_json::to_string(&websocket_frame("subscribe", "ws-page")).unwrap(),
        )
        .await;
        let initial = ws_client_read(&mut socket).await;
        assert_eq!(initial.0, 2);
        assert_eq!(initial.1, b"original snapshot");
        socket
    }

    async fn ws_two_user_state() -> AppState {
        let state = websocket_test_state().await;
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('other-ws','space-1','other-user','editor')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,session_kind) VALUES('other-session','other-user','other-ws','2999-01-01T00:00:00Z','browser')").execute(&state.db).await.unwrap();
        state
    }

    #[tokio::test]
    async fn websocket_real_shared_room_delivers_committed_trusted_frames_then_denies_revoked_recipient(
    ) {
        for revoke in [
            "INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('ws-page','space-1','restricted','revoked','user-1'); INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('ws-allowed','ws-page','space-1','user-1','user-1')",
            "DELETE FROM space_members WHERE user_id='other-user'",
            "DELETE FROM sessions WHERE id='other-session'",
        ] {
            let state=ws_two_user_state().await;let (address,server)=ws_test_server(state.clone()).await;
            let mut sender=ws_client(address,&state,"user-1","test-session").await;
            let mut recipient=ws_client(address,&state,"other-user","other-session").await;
            ws_client_send(&mut sender,&serde_json::to_string(&websocket_frame("change","ws-page")).unwrap()).await;
            let delivered=ws_client_read(&mut recipient).await;assert_eq!(delivered.0,1);
            let frame:Value=serde_json::from_slice(&delivered.1).unwrap();assert_eq!(frame["user_id"],"user-1");assert_eq!(frame["type"],Value::Null);assert_eq!(frame["msg_type"],"change");assert_eq!(websocket_snapshot(&state).await.2,1);
            for query in revoke.split("; ") {sqlx::query(query).execute(&state.db).await.unwrap();}
            ws_client_send(&mut sender,&serde_json::to_string(&websocket_frame("presence","ws-page")).unwrap()).await;
            let refused:Value=serde_json::from_slice(&ws_client_read(&mut recipient).await.1).unwrap();assert_eq!(refused,json!({"error":"unavailable"}));
            drop(sender);drop(recipient);server.abort();let _=server.await;
        }
    }

    #[tokio::test]
    async fn websocket_real_failed_store_sends_no_broadcast_and_preserves_canonical_state() {
        let state = ws_two_user_state().await;
        let (address, server) = ws_test_server(state.clone()).await;
        let mut sender = ws_client(address, &state, "user-1", "test-session").await;
        let mut recipient = ws_client(address, &state, "other-user", "other-session").await;
        let before = websocket_snapshot(&state).await;
        sqlx::query("CREATE TRIGGER ws_fail BEFORE INSERT ON doc_versions BEGIN SELECT RAISE(ABORT,'fail'); END").execute(&state.db).await.unwrap();
        ws_client_send(
            &mut sender,
            &serde_json::to_string(&websocket_frame("change", "ws-page")).unwrap(),
        )
        .await;
        let refused: Value = serde_json::from_slice(&ws_client_read(&mut sender).await.1).unwrap();
        assert_eq!(refused, json!({"error":"unavailable"}));
        use tokio::io::AsyncReadExt;
        let mut header = [0; 2];
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(200),
            recipient.read_exact(&mut header)
        )
        .await
        .is_err());
        assert_eq!(websocket_snapshot(&state).await, before);
        drop(sender);
        drop(recipient);
        server.abort();
        let _ = server.await;
    }

    #[tokio::test]
    async fn websocket_different_app_states_do_not_share_rooms() {
        let state = ws_two_user_state().await;
        let mut other = state.clone();
        other.ws_rooms = super::WsRooms::default();
        let (address, server) = ws_test_server(state.clone()).await;
        let (foreign_address, foreign_server) = ws_test_server(other.clone()).await;
        let mut sender = ws_client(address, &state, "user-1", "test-session").await;
        let mut recipient = ws_client(foreign_address, &other, "other-user", "other-session").await;
        ws_client_send(
            &mut sender,
            &serde_json::to_string(&websocket_frame("presence", "ws-page")).unwrap(),
        )
        .await;
        use tokio::io::AsyncReadExt;
        let mut header = [0; 2];
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(200),
            recipient.read_exact(&mut header)
        )
        .await
        .is_err());
        drop(sender);
        drop(recipient);
        server.abort();
        foreign_server.abort();
        let _ = server.await;
        let _ = foreign_server.await;
    }

    #[tokio::test]
    async fn websocket_typed_ancestor_restrictions_and_private_participation_apply_to_all_directions(
    ) {
        let state = websocket_test_state().await;
        let auth = auth_user();
        let before = websocket_snapshot(&state).await;
        seed_document(&state.db, "ws-parent", None, "Private ancestor").await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='ws-parent'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET parent_id='ws-parent' WHERE id='ws-page'")
            .execute(&state.db)
            .await
            .unwrap();
        for admin in [false, true] {
            if admin {
                sqlx::query("UPDATE users SET role='admin' WHERE id='user-1'")
                    .execute(&state.db)
                    .await
                    .unwrap();
            }
            assert!(super::load_doc_state(&state, &auth, "ws-page")
                .await
                .is_err());
            assert!(super::prepare_ws_frame(
                &state,
                &auth,
                Some("ws-page"),
                websocket_frame("change", "ws-page")
            )
            .await
            .is_err());
            assert!(super::authorize_ws_outgoing(
                &state,
                &auth,
                Some("ws-page"),
                &websocket_frame("presence", "ws-page")
            )
            .await
            .is_err());
            assert_eq!(websocket_snapshot(&state).await, before);
        }
        sqlx::query("UPDATE documents SET published_at='parent-published' WHERE id='ws-parent'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('ws-parent','space-1','restricted','empty','user-1')").execute(&state.db).await.unwrap();
        assert!(super::load_doc_state(&state, &auth, "ws-page")
            .await
            .is_err());
        sqlx::query("INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('parent-allowed','ws-parent','space-1','user-1','user-1')").execute(&state.db).await.unwrap();
        assert!(super::load_doc_state(&state, &auth, "ws-page")
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn guest_replay_entry_and_late_history_response_retain_original_protocol_records() {
        let state=property_replay_test_state().await;
        let auth=folder_dispatch_auth(&state).await;
        let manifest=single_operation_manifest(replay_operation("guest-original-folder","2026-07-08T12:00:00Z","folder","guest-folder","create",json!({"id":"guest-folder","space_id":"space-1","name":"Original","created_at":"2026-07-08T12:00:00Z","updated_at":"2026-07-08T12:00:00Z"})));
        let response=execute_replay_manifest(State(state.clone()),AuthUser { claims: auth.claims.clone(), api_key_scopes: auth.api_key_scopes.clone() },Json(manifest.clone())).await.unwrap().0;
        assert_eq!(response.applied_operation_count,1);
        assert_eq!(super::list_replay_history(State(state.clone()),AuthUser { claims: auth.claims.clone(), api_key_scopes: auth.api_key_scopes.clone() }).await.unwrap().0.manifests.len(),1);
        let before=crate::spaces::documents::tests::move_database_snapshot(&state).await;
        let writer=state.db.clone();super::GUEST_BEFORE_REPLAY_HISTORY_FINAL.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {
            sqlx::query("UPDATE users SET account_kind='guest' WHERE id='user-1'").execute(&writer).await.unwrap();
        })));
        assert!(super::list_replay_history(State(state.clone()),AuthUser { claims: auth.claims.clone(), api_key_scopes: auth.api_key_scopes.clone() }).await.is_err());
        assert!(super::preflight_replay_manifest(State(state.clone()),AuthUser { claims: auth.claims.clone(), api_key_scopes: auth.api_key_scopes.clone() },Json(manifest.clone())).await.is_err());
        assert!(execute_replay_manifest(State(state.clone()),auth,Json(manifest)).await.is_err());
        let after=crate::spaces::documents::tests::move_database_snapshot(&state).await;
        assert_eq!(before.into_iter().filter(|(t,_)|t!="users").collect::<Vec<_>>(),after.into_iter().filter(|(t,_)|t!="users").collect::<Vec<_>>());
    }

    async fn guest_socket_wal_state() -> (AppState, std::path::PathBuf) {
        let mut state = websocket_test_state().await;
        let path = std::env::temp_dir().join(format!("guest-ws-{}.db", uuid::Uuid::new_v4()));
        let db = AnyPoolOptions::new().max_connections(3).after_connect(|conn,_| Box::pin(async move {
            sqlx::query("PRAGMA journal_mode=WAL").execute(&mut *conn).await?;
            sqlx::query("PRAGMA busy_timeout=5000").execute(conn).await?; Ok(())
        })).connect(&format!("sqlite://{}?mode=rwc",path.display())).await.unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap(); seed_replay_fixture_data(&db).await;
        for sql in [
            "INSERT INTO users(id,email,display_name,role) VALUES('other-user','other@test','Other','member')",
            "INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,session_kind) VALUES('test-session','user-1','guest-wal','2999-01-01T00:00:00Z','browser')",
            "INSERT INTO space_members(id,space_id,user_id,role) VALUES('other-ws','space-1','other-user','editor')",
            "INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,session_kind) VALUES('other-session','other-user','other-ws','2999-01-01T00:00:00Z','browser')",
        ] { sqlx::query(sql).execute(&db).await.unwrap(); }
        seed_document(&db,"ws-page",None,"Page").await;
        sqlx::query("UPDATE documents SET automerge_state=$1 WHERE id='ws-page'").bind(b"original snapshot".as_slice()).execute(&db).await.unwrap();
        state.db=db; (state,path)
    }
    #[tokio::test]
    async fn guest_socket_real_join_late_wal_classification_sends_no_canonical_state() {
        let (state,path)=guest_socket_wal_state().await;
        let before=websocket_snapshot(&state).await;
        let (address,server)=ws_test_server(state.clone()).await;
        let mut socket=ws_client(address,&state,"user-1","test-session").await;
        let pause=std::sync::Arc::new(super::WsFinalReadPause{reached:tokio::sync::Notify::new(),release:tokio::sync::Notify::new()});
        *state.ws_rooms.final_read_pause.write().await=Some(pause.clone());
        ws_client_send(&mut socket,&serde_json::to_string(&websocket_frame("subscribe","ws-page")).unwrap()).await;
        tokio::time::timeout(std::time::Duration::from_secs(2),pause.reached.notified()).await.unwrap();
        sqlx::query("UPDATE users SET account_kind='guest' WHERE id='user-1'").execute(&state.db).await.unwrap();
        pause.release.notify_one();
        assert_eq!(serde_json::from_slice::<Value>(&ws_client_read(&mut socket).await.1).unwrap(),json!({"error":"unavailable"}));
        assert_eq!(websocket_snapshot(&state).await,before);
        drop(socket);server.abort();let _=server.await;state.db.close().await;let _=std::fs::remove_file(path);
    }
    #[tokio::test]
    async fn guest_socket_real_final_source_wait_classification_does_not_return_bytes() {
        let (state,path)=guest_socket_wal_state().await;let before=websocket_snapshot(&state).await;
        let (address,server)=ws_test_server(state.clone()).await;
        let mut socket=ws_client(address,&state,"user-1","test-session").await;
        let pause=std::sync::Arc::new(super::WsFinalReadPause{reached:tokio::sync::Notify::new(),release:tokio::sync::Notify::new()});
        *state.ws_rooms.final_source_pause.write().await=Some(pause.clone());
        ws_client_send(&mut socket,&serde_json::to_string(&websocket_frame("subscribe","ws-page")).unwrap()).await;
        tokio::time::timeout(std::time::Duration::from_secs(2),pause.reached.notified()).await.unwrap();
        sqlx::query("UPDATE users SET account_kind='guest' WHERE id='user-1'").execute(&state.db).await.unwrap();pause.release.notify_one();
        assert_eq!(serde_json::from_slice::<Value>(&ws_client_read(&mut socket).await.1).unwrap(),json!({"error":"unavailable"}));
        assert_eq!(websocket_snapshot(&state).await,before);
        drop(socket);server.abort();let _=server.await;state.db.close().await;let _=std::fs::remove_file(path);
    }
    #[tokio::test]
    async fn guest_socket_real_shared_room_output_and_idle_deny_classified_member() {
        for idle in [false,true] {
            let (state,path)=guest_socket_wal_state().await;let before=websocket_snapshot(&state).await;
            let (address,server)=ws_test_server(state.clone()).await;
            let mut sender=ws_client(address,&state,"user-1","test-session").await;
            let mut recipient=ws_client(address,&state,"other-user","other-session").await;
            sqlx::query("UPDATE users SET account_kind='guest' WHERE id='other-user'").execute(&state.db).await.unwrap();
            if !idle { ws_client_send(&mut sender,&serde_json::to_string(&websocket_frame("presence","ws-page")).unwrap()).await; }
            assert_eq!(serde_json::from_slice::<Value>(&ws_client_read(&mut recipient).await.1).unwrap(),json!({"error":"unavailable"}));
            assert_eq!(websocket_snapshot(&state).await,before);
            drop(sender);drop(recipient);server.abort();let _=server.await;state.db.close().await;let _=std::fs::remove_file(path);
        }
    }
    #[tokio::test]
    async fn guest_socket_real_late_input_trigger_rolls_back_canonical_change_and_history() {
        let (state,path)=guest_socket_wal_state().await;let before=websocket_snapshot(&state).await;
        let (address,server)=ws_test_server(state.clone()).await;
        let mut sender=ws_client(address,&state,"user-1","test-session").await;
        sqlx::query("CREATE TRIGGER guest_input AFTER INSERT ON doc_versions BEGIN UPDATE users SET account_kind='guest' WHERE id='user-1'; END").execute(&state.db).await.unwrap();
        ws_client_send(&mut sender,&serde_json::to_string(&websocket_frame("change","ws-page")).unwrap()).await;
        assert_eq!(serde_json::from_slice::<Value>(&ws_client_read(&mut sender).await.1).unwrap(),json!({"error":"unavailable"}));
        assert_eq!(websocket_snapshot(&state).await,before);
        let kind:String=sqlx::query_scalar("SELECT account_kind FROM users WHERE id='user-1'").fetch_one(&state.db).await.unwrap();assert_eq!(kind,"member");
        drop(sender);server.abort();let _=server.await;state.db.close().await;let _=std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn websocket_last_canonical_read_rechecks_original_jwt_expiry() {
        let state = websocket_test_state().await;
        let mut claims = auth_user().claims;
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let verified =
            crate::auth::jwt::validate_token(&token, &state.jwt_secret, TokenType::Access).unwrap();
        let auth = super::validate_ws_session(&state, &verified).await.unwrap();
        let pause = std::sync::Arc::new(super::WsFinalReadPause {
            reached: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
        });
        *state.ws_rooms.final_read_pause.write().await = Some(pause.clone());
        let reader_state = state.clone();
        let expiry = verified.exp;
        let reader =
            tokio::spawn(
                async move { super::load_doc_state(&reader_state, &auth, "ws-page").await },
            );
        tokio::time::timeout(std::time::Duration::from_secs(2), pause.reached.notified())
            .await
            .unwrap();
        while chrono::Utc::now().timestamp() <= expiry {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        pause.release.notify_one();
        assert!(reader.await.unwrap().is_err());
        let future_session: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sessions WHERE id='test-session' AND expires_at>$1",
        )
        .bind(chrono::Utc::now().to_rfc3339())
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(future_session, 1);
        assert_eq!(websocket_snapshot(&state).await.0, b"original snapshot");
    }

    #[tokio::test]
    async fn websocket_space_guard_wait_observes_external_session_revocation() {
        let mut state = websocket_test_state().await;
        let path =
            std::env::temp_dir().join(format!("starlane-ws-wal-{}.sqlite", uuid::Uuid::new_v4()));
        let uri = format!("sqlite://{}?mode=rwc", path.display());
        let db = AnyPoolOptions::new()
            .max_connections(3)
            .connect(&uri)
            .await
            .unwrap();
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&db)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        seed_replay_fixture_data(&db).await;
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,session_kind) VALUES('test-session','user-1','wal-session','2999-01-01T00:00:00Z','browser')").execute(&db).await.unwrap();
        seed_document(&db, "ws-page", None, "Page").await;
        sqlx::query("UPDATE documents SET automerge_state=$1 WHERE id='ws-page'")
            .bind(b"original snapshot".as_slice())
            .execute(&db)
            .await
            .unwrap();
        state.db = db;
        let before = websocket_snapshot(&state).await;
        let mut blocker = state.db.begin().await.unwrap();
        crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut blocker, &["space-1"])
            .await
            .unwrap();
        let waiting_state = state.clone();
        let waiting = tokio::spawn(async move {
            super::store_change(&waiting_state, "ws-page", &auth_user(), b"blocked update").await
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            !waiting.is_finished(),
            "mutation must wait for held writer guard"
        );
        sqlx::query("DELETE FROM sessions WHERE id='test-session'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        blocker.commit().await.unwrap();
        assert!(waiting.await.unwrap().is_err());
        assert_eq!(websocket_snapshot(&state).await, before);
        state.db.close().await;
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn websocket_late_policy_and_source_mutation_roll_back_versions_and_state() {
        for action in [
            "DELETE FROM sessions WHERE id='test-session';",
            "UPDATE documents SET published_at=NULL WHERE id='ws-page';",
            "UPDATE documents SET locked=1,locked_by='other-user' WHERE id='ws-page';",
            "INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('ws-page','space-1','restricted','late','user-1');",
            "UPDATE doc_versions SET created_by='other-user' WHERE id=NEW.id;",
            "UPDATE doc_versions SET created_at='tampered' WHERE id=NEW.id;",
            "INSERT INTO doc_versions(id,document_id,version_number,change_set,created_by) VALUES('rogue-version',NEW.document_id,999,X'00','user-1');",
            "UPDATE documents SET content_path='unexpected-path',title='changed' WHERE id='ws-page';",
        ] {
            let state=websocket_test_state().await;let before=websocket_snapshot(&state).await;
            sqlx::query(&format!("CREATE TRIGGER ws_late AFTER INSERT ON doc_versions BEGIN {action} END")).execute(&state.db).await.unwrap();
            assert!(super::prepare_ws_frame(&state,&auth_user(),Some("ws-page"),websocket_frame("change","ws-page")).await.is_err());
            assert_eq!(websocket_snapshot(&state).await,before);
            let sessions:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE id='test-session'").fetch_one(&state.db).await.unwrap();assert_eq!(sessions,1);
        }
    }

    async fn websocket_test_state() -> AppState {
        let state = property_replay_test_state().await;
        // property replay fixture now uses real migrations, including these tables.
        sqlx::query("UPDATE sessions SET session_kind='browser',expires_at='2099-01-01T00:00:00Z',last_seen_at='old' WHERE id='test-session'").execute(&state.db).await.unwrap();
        seed_document(&state.db, "ws-page", None, "Published page").await;
        sqlx::query("UPDATE documents SET automerge_state = $1 WHERE id='ws-page'")
            .bind(b"original snapshot".as_slice())
            .execute(&state.db)
            .await
            .unwrap();
        state
    }

    fn websocket_frame(kind: &str, doc: &str) -> super::SyncMessage {
        super::SyncMessage {
            doc_id: doc.into(),
            user_id: "spoofed-other-user".into(),
            msg_type: kind.into(),
            payload: json!({"change":"new snapshot"}),
        }
    }

    async fn websocket_snapshot(state: &AppState) -> (Vec<u8>, String, i64) {
        let (body, date): (Vec<u8>, String) =
            sqlx::query_as("SELECT automerge_state,updated_at FROM documents WHERE id='ws-page'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let versions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM doc_versions")
            .fetch_one(&state.db)
            .await
            .unwrap();
        (body, date, versions)
    }

    #[tokio::test]
    async fn websocket_revalidates_expiry_session_and_active_account_on_every_operation() {
        for revoke in [
            "DELETE FROM sessions",
            "UPDATE sessions SET expires_at='2000-01-01T00:00:00Z'",
            "UPDATE users SET is_active=0",
        ] {
            let state = websocket_test_state().await;
            let claims = auth_user();
            assert_eq!(
                super::load_doc_state(&state, &claims, "ws-page")
                    .await
                    .unwrap(),
                b"original snapshot"
            );
            let before = websocket_snapshot(&state).await;
            sqlx::query(revoke).execute(&state.db).await.unwrap();
            assert!(super::validate_ws_session(&state, &claims.claims)
                .await
                .is_err());
            assert!(super::load_doc_state(&state, &claims, "ws-page")
                .await
                .is_err());
            for kind in ["change", "presence", "cursor"] {
                assert!(super::prepare_ws_frame(
                    &state,
                    &claims,
                    Some("ws-page"),
                    websocket_frame(kind, "ws-page")
                )
                .await
                .is_err());
            }
            assert!(super::authorize_ws_outgoing(
                &state,
                &claims,
                Some("ws-page"),
                &websocket_frame("presence", "ws-page")
            )
            .await
            .is_err());
            assert_eq!(websocket_snapshot(&state).await, before);
        }
        let state = websocket_test_state().await;
        let mut claims = auth_user().claims;
        claims.exp = chrono::Utc::now().timestamp();
        assert!(super::validate_ws_session(&state, &claims).await.is_err());
        claims.exp += 900;
        claims.token_type = TokenType::Refresh;
        assert!(super::validate_ws_session(&state, &claims).await.is_err());
    }

    #[tokio::test]
    async fn websocket_canonical_pages_reject_private_lifecycle_foreign_and_blocked_access() {
        let state = websocket_test_state().await;
        let claims = auth_user();
        let before = websocket_snapshot(&state).await;
        for lifecycle in ["published_at=NULL", "archived_at='old'", "deleted_at='old'"] {
            sqlx::query(&format!(
                "UPDATE documents SET {lifecycle} WHERE id='ws-page'"
            ))
            .execute(&state.db)
            .await
            .unwrap();
            assert!(super::load_doc_state(&state, &claims, "ws-page")
                .await
                .is_err());
            assert!(super::prepare_ws_frame(
                &state,
                &claims,
                Some("ws-page"),
                websocket_frame("change", "ws-page")
            )
            .await
            .is_err());
            assert!(super::authorize_ws_outgoing(
                &state,
                &claims,
                Some("ws-page"),
                &websocket_frame("presence", "ws-page")
            )
            .await
            .is_err());
            assert_eq!(websocket_snapshot(&state).await, before);
            sqlx::query("UPDATE documents SET published_at='old',archived_at=NULL,deleted_at=NULL WHERE id='ws-page'").execute(&state.db).await.unwrap();
        }
        sqlx::query("INSERT INTO spaces (id,name,slug,created_by,created_at,updated_at,visibility) VALUES ('ws-foreign','Foreign','foreign','user-1','old','old','private')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET space_id='ws-foreign' WHERE id='ws-page'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::load_doc_state(&state, &claims, "ws-page")
            .await
            .is_err());
        assert!(super::load_doc_state(&state, &claims, "nonexistent")
            .await
            .is_err());
        sqlx::query("UPDATE documents SET space_id='space-1' WHERE id='ws-page'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO space_user_blocks VALUES ('space-1','user-1','user-1','old')")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::load_doc_state(&state, &claims, "ws-page")
            .await
            .is_err());
        assert!(super::prepare_ws_frame(
            &state,
            &claims,
            Some("ws-page"),
            websocket_frame("change", "ws-page")
        )
        .await
        .is_err());
        assert_eq!(websocket_snapshot(&state).await, before);
    }

    #[tokio::test]
    async fn websocket_group_and_custom_permission_revocation_are_effective_immediately() {
        let state = websocket_test_state().await;
        let claims = auth_user();
        sqlx::query("DELETE FROM space_members")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups (id,name,created_at,updated_at) VALUES ('ws-group','WS group','old','old')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_members VALUES ('ws-member','ws-group','user-1','old')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO space_group_permissions VALUES ('ws-permission','space-1','ws-group','viewer','old','old')").execute(&state.db).await.unwrap();
        assert!(super::load_doc_state(&state, &claims, "ws-page")
            .await
            .is_ok());
        assert!(super::prepare_ws_frame(
            &state,
            &claims,
            Some("ws-page"),
            websocket_frame("change", "ws-page")
        )
        .await
        .is_err());
        sqlx::query("DELETE FROM group_members")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::load_doc_state(&state, &claims, "ws-page")
            .await
            .is_err());
        assert!(super::authorize_ws_outgoing(
            &state,
            &claims,
            Some("ws-page"),
            &websocket_frame("presence", "ws-page")
        )
        .await
        .is_err());
        sqlx::query("INSERT INTO space_roles VALUES ('custom:ws','space-1','Custom','','[\"view\",\"edit_content\"]','user-1','old','old')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_members VALUES ('ws-custom-member','space-1','user-1','custom:ws','old')").execute(&state.db).await.unwrap();
        assert!(super::authorize_ws_page(&state, &claims, "ws-page", true)
            .await
            .is_ok());
        sqlx::query("UPDATE space_roles SET permissions='[\"view\"]' WHERE id='custom:ws'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::authorize_ws_page(&state, &claims, "ws-page", true)
            .await
            .is_err());
        assert!(super::load_doc_state(&state, &claims, "ws-page")
            .await
            .is_ok());
        // Legacy/custom records may lack View despite carrying EditContent.
        sqlx::query("UPDATE space_roles SET permissions='[\"edit_content\"]' WHERE id='custom:ws'")
            .execute(&state.db).await.unwrap();
        assert!(super::authorize_ws_page(&state, &claims, "ws-page", true).await.is_err());
        assert!(super::load_doc_state(&state, &claims, "ws-page").await.is_err());
        assert_eq!(websocket_snapshot(&state).await.2, 0);
    }

    #[tokio::test]
    async fn websocket_protocol_binds_subscription_actor_view_and_edit_lock() {
        let state = websocket_test_state().await;
        let claims = auth_user();
        let before = websocket_snapshot(&state).await;
        for kind in ["presence", "cursor", "change"] {
            assert!(super::prepare_ws_frame(
                &state,
                &claims,
                None,
                websocket_frame(kind, "ws-page")
            )
            .await
            .is_err());
            assert!(super::prepare_ws_frame(
                &state,
                &claims,
                Some("ws-page"),
                websocket_frame(kind, "other-page")
            )
            .await
            .is_err());
        }
        assert!(super::authorize_ws_outgoing(
            &state,
            &claims,
            Some("ws-page"),
            &websocket_frame("presence", "other-page")
        )
        .await
        .is_err());
        sqlx::query("UPDATE space_members SET role='viewer'")
            .execute(&state.db)
            .await
            .unwrap();
        for kind in ["presence", "cursor"] {
            let frame = super::prepare_ws_frame(
                &state,
                &claims,
                Some("ws-page"),
                websocket_frame(kind, "ws-page"),
            )
            .await
            .unwrap();
            assert_eq!(frame.user_id, "user-1");
            assert_eq!(frame.doc_id, "ws-page");
        }
        assert!(super::prepare_ws_frame(
            &state,
            &claims,
            Some("ws-page"),
            websocket_frame("change", "ws-page")
        )
        .await
        .is_err());
        assert_eq!(websocket_snapshot(&state).await, before);
        sqlx::query("UPDATE space_members SET role='editor'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET locked=1,locked_by='other-user' WHERE id='ws-page'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::load_doc_state(&state, &claims, "ws-page")
            .await
            .is_ok());
        assert!(super::prepare_ws_frame(
            &state,
            &claims,
            Some("ws-page"),
            websocket_frame("change", "ws-page")
        )
        .await
        .is_err());
        assert_eq!(websocket_snapshot(&state).await, before);
        sqlx::query("UPDATE documents SET locked_by='user-1' WHERE id='ws-page'")
            .execute(&state.db)
            .await
            .unwrap();
        for _ in 0..2 {
            let frame = super::prepare_ws_frame(
                &state,
                &claims,
                Some("ws-page"),
                websocket_frame("change", "ws-page"),
            )
            .await
            .unwrap();
            assert_eq!(frame.user_id, "user-1");
        }
        let versions: Vec<(i64, String)> = sqlx::query_as(
            "SELECT version_number,created_by FROM doc_versions ORDER BY version_number",
        )
        .fetch_all(&state.db)
        .await
        .unwrap();
        assert_eq!(versions, vec![(1, "user-1".into()), (2, "user-1".into())]);
        assert_eq!(
            websocket_snapshot(&state).await.0,
            serde_json::to_vec(&websocket_frame("change", "ws-page").payload).unwrap()
        );
    }

    #[tokio::test]
    async fn websocket_failed_store_rolls_back_version_and_snapshot_without_broadcastable_frame() {
        let state = websocket_test_state().await;
        let claims = auth_user();
        let before = websocket_snapshot(&state).await;
        for trigger in [
            "CREATE TRIGGER ws_failure BEFORE INSERT ON doc_versions BEGIN SELECT RAISE(ABORT,'version fails'); END",
            "CREATE TRIGGER ws_failure BEFORE UPDATE OF automerge_state ON documents BEGIN SELECT RAISE(ABORT,'snapshot fails'); END",
            "CREATE TRIGGER ws_failure BEFORE UPDATE OF automerge_state ON documents BEGIN SELECT RAISE(IGNORE); END",
        ] {
            sqlx::query(trigger).execute(&state.db).await.unwrap();
            assert!(super::prepare_ws_frame(&state,&claims,Some("ws-page"),websocket_frame("change","ws-page")).await.is_err());
            assert_eq!(websocket_snapshot(&state).await,before);
            sqlx::query("DROP TRIGGER ws_failure").execute(&state.db).await.unwrap();
        }
        assert!(super::prepare_ws_frame(
            &state,
            &claims,
            Some("ws-page"),
            websocket_frame("change", "ws-page")
        )
        .await
        .is_ok());
        assert_eq!(websocket_snapshot(&state).await.2, 1);
    }

    async fn property_replay_test_state() -> AppState {
        let mut state = replay_test_state().await;
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        seed_replay_fixture_data(&db).await;
        sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES('other-user','other@test','Other','member')").execute(&db).await.unwrap();
        sqlx::query("INSERT INTO spaces(id,name,slug,created_by) VALUES('space-2','Foreign','property-foreign','user-1')").execute(&db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('test-session','user-1','metadata-test','2999-01-01T00:00:00Z')").execute(&db).await.unwrap();
        state.db = db;
        for statement in [
            "INSERT INTO properties_schema (id, space_id, name, property_type, options, sort_order, created_at) VALUES ('a-text', 'space-1', 'Text', 'text', NULL, 1, 'old'), ('z-number', 'space-1', 'Number', 'number', NULL, 2, 'old'), ('choice', 'space-1', 'Choice', 'select', '[\"Ready\",\"Done\"]', 3, 'old'), ('foreign', 'space-2', 'Foreign', 'text', NULL, 1, 'old'), ('relation', 'space-1', 'Related', 'relation', NULL, 4, 'old')",
        ] {
            sqlx::query(statement).execute(&state.db).await.unwrap();
        }
        seed_document(&state.db, "doc-1", None, "Source").await;
        seed_document(&state.db, "target", None, "Target").await;
        sqlx::query("UPDATE documents SET published_at = 'old'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO property_values (id, document_id, property_id, value, created_at, updated_at) VALUES ('original-value', 'doc-1', 'a-text', 'Original', 'original-created', 'original-updated'), ('original-choice', 'doc-1', 'choice', 'Ready', 'original-created', 'original-updated')").execute(&state.db).await.unwrap();
        state
    }

    fn property_values_operation(id: &str, values: serde_json::Value) -> SyncReplayOperation {
        replay_operation(
            id,
            "2026-10-07T12:00:00Z",
            "document_property_values",
            "doc-1",
            "set",
            json!({"document_id":"doc-1", "space_id":"space-1", "values": values}),
        )
    }

    #[tokio::test]
    async fn replay_properties_validate_schema_and_preserve_import_identity() {
        let state = property_replay_test_state().await;
        let invalid = replay_operation(
            "invalid-schema",
            "2026-10-07T12:00:00Z",
            "property_schema",
            "imported",
            "create",
            json!({"id":"imported", "space_id":"space-1", "name":"Imported", "property_type":"select", "options":["Same", "Same"]}),
        );
        let response = execute_replay_manifest(
            State(state.clone()),
            auth_user(),
            Json(single_operation_manifest(invalid)),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(response.failed_operation_count, 1);
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM properties_schema WHERE id = 'imported'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(count, 0);
        let valid = replay_operation(
            "valid-schema",
            "2026-10-07T12:00:00Z",
            "property_schema",
            "imported",
            "create",
            json!({"id":"imported", "space_id":"space-1", "name":"Imported", "property_type":"select", "options":["Ready"], "sort_order":7, "created_at":"2026-10-01T12:00:00Z"}),
        );
        let manifest = single_operation_manifest(valid);
        let response =
            execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest.clone()))
                .await
                .unwrap()
                .0;
        assert_eq!(response.applied_operation_count, 1);
        let (id, sort, created): (String, i64, String) = sqlx::query_as(
            "SELECT id, sort_order, created_at FROM properties_schema WHERE id = 'imported'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            (id, sort, created),
            ("imported".into(), 7, "2026-10-01T12:00:00+00:00".into())
        );
        let response = execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest))
            .await
            .unwrap()
            .0;
        assert_eq!(response.skipped_operation_count, 1);
        let foreign = replay_operation(
            "foreign-schema-create",
            "2026-10-07T12:00:00Z",
            "property_schema",
            "foreign",
            "create",
            json!({"id":"foreign", "space_id":"space-1", "name":"Foreign", "property_type":"text"}),
        );
        assert!(
            super::replay_property_schema_create(&state, &auth_user(), &foreign)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn replay_properties_reject_foreign_and_incompatible_schema_updates_without_data_loss() {
        let state = property_replay_test_state().await;
        for (id, property, kind, options) in [
            ("foreign-update", "foreign", "text", json!(null)),
            ("missing-update", "missing", "text", json!(null)),
            ("type-update", "a-text", "number", json!(null)),
            ("option-update", "choice", "select", json!(["Done"])),
        ] {
            let op = replay_operation(
                id,
                "2026-10-07T12:00:00Z",
                "property_schema",
                property,
                "update",
                json!({"id":property, "space_id":"space-1", "name":"Changed", "property_type":kind, "options":options, "sort_order":99}),
            );
            let response = execute_replay_manifest(
                State(state.clone()),
                auth_user(),
                Json(single_operation_manifest(op)),
            )
            .await
            .unwrap()
            .0;
            assert_eq!(response.failed_operation_count, 1, "{id}");
            assert_eq!(response.applied_operation_count, 0);
        }
        let schema: (String, String, i64) = sqlx::query_as(
            "SELECT name, property_type, sort_order FROM properties_schema WHERE id = 'a-text'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(schema, ("Text".into(), "text".into(), 1));
        let choice: (String, String) = sqlx::query_as("SELECT options, value FROM properties_schema JOIN property_values ON property_values.property_id = properties_schema.id WHERE properties_schema.id = 'choice'").fetch_one(&state.db).await.unwrap();
        assert_eq!(choice, ("[\"Ready\",\"Done\"]".into(), "Ready".into()));
        let old: String =
            sqlx::query_scalar("SELECT value FROM property_values WHERE id = 'original-value'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(old, "Original");
    }

    #[tokio::test]
    async fn replay_properties_reject_invalid_batches_and_retain_failed_payload_for_recovery() {
        let state = property_replay_test_state().await;
        let first = json!({"property_id":"a-text", "value":"Should not persist"});
        for invalid in [
            json!({"property_id":"foreign", "value":"secret"}),
            json!({"property_id":"z-number", "value":"invalid number"}),
            json!({"property_id":"z-number", "value":12}),
            json!({"property_id":"a-text", "value":false}),
            json!({"property_id":"z-number", "value":{}}),
            json!({"property_id":"z-number"}),
            json!({"property_id":"z-number", "value":"1", "updated_at":false}),
            json!({"property_id":"a-text", "value":"Duplicate"}),
        ] {
            let op = property_values_operation("invalid-batch", json!([first.clone(), invalid]));
            assert!(
                super::replay_document_property_values_set(&state, &auth_user(), &op)
                    .await
                    .is_err()
            );
            let old: String =
                sqlx::query_scalar("SELECT value FROM property_values WHERE id = 'original-value'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(old, "Original");
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM property_values")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(count, 2);
        }
        let op = property_values_operation(
            "failed-recovery",
            json!([first, {"property_id":"z-number", "value":12}]),
        );
        let payload = op.payload.clone();
        let response = execute_replay_manifest(
            State(state.clone()),
            auth_user(),
            Json(single_operation_manifest(op)),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(response.status, "partial");
        assert_eq!(response.failed_operation_count, 1);
        let (status, error, stored): (String, String, String) = sqlx::query_as("SELECT status, error, payload FROM sync_replay_operations WHERE operation_id = 'failed-recovery'").fetch_one(&state.db).await.unwrap();
        assert_eq!(status, "failed");
        assert!(error.contains("string or explicit null"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&stored).unwrap(),
            payload
        );
    }

    #[tokio::test]
    async fn replay_properties_failed_and_pending_retries_never_acknowledge_success() {
        let state = property_replay_test_state().await;
        let failed = property_values_operation(
            "retry-validation",
            json!([
                {"property_id":"a-text", "value":"Must not persist"},
                {"property_id":"z-number", "value":12}
            ]),
        );
        let original_payload = failed.payload.clone();
        let manifest = single_operation_manifest(failed);
        for _ in 0..2 {
            let response =
                execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest.clone()))
                    .await
                    .unwrap()
                    .0;
            assert_eq!(response.status, "partial");
            assert_eq!(response.failed_operation_count, 1);
            assert_eq!(response.skipped_operation_count, 0);
        }
        let mut corrected = manifest.clone();
        corrected.operations[0].payload["values"][1]["value"] = json!("12");
        let response =
            execute_replay_manifest(State(state.clone()), auth_user(), Json(corrected.clone()))
                .await
                .unwrap()
                .0;
        assert_eq!(response.results[0].status, "failed");
        assert!(response.results[0].message.contains("new id"));
        let (status, payload): (String, String) = sqlx::query_as("SELECT status, payload FROM sync_replay_operations WHERE operation_id = 'retry-validation'").fetch_one(&state.db).await.unwrap();
        assert_eq!(status, "failed");
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&payload).unwrap(),
            original_payload
        );
        sqlx::query("UPDATE sync_replay_operations SET status = 'pending' WHERE operation_id = 'retry-validation'").execute(&state.db).await.unwrap();
        let response = execute_replay_manifest(State(state.clone()), auth_user(), Json(corrected))
            .await
            .unwrap()
            .0;
        assert_eq!(response.results[0].status, "failed");
        assert_eq!(response.skipped_operation_count, 0);
        let status: String = sqlx::query_scalar(
            "SELECT status FROM sync_replay_operations WHERE operation_id = 'retry-validation'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(status, "pending");
        let value: String =
            sqlx::query_scalar("SELECT value FROM property_values WHERE id = 'original-value'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(value, "Original");
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM property_values")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn replay_properties_roll_back_late_write_errors_and_preserve_valid_ids_timestamps() {
        let state = property_replay_test_state().await;
        let collision = property_values_operation(
            "collision",
            json!([
                {"property_id":"a-text", "value":"Must roll back"},
                {"property_id":"z-number", "value":"12", "id":"original-value"}
            ]),
        );
        assert!(
            super::replay_document_property_values_set(&state, &auth_user(), &collision)
                .await
                .is_err()
        );
        let old: String =
            sqlx::query_scalar("SELECT value FROM property_values WHERE id = 'original-value'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(old, "Original");
        let valid = property_values_operation(
            "valid-values",
            json!([
                {"property_id":"z-number", "value":".5", "id":"legacy:value-number", "created_at":"2026-10-01T12:00:00Z", "updated_at":"2026-10-07T12:00:00Z"},
                {"property_id":"a-text", "value":null, "id":"replacement-id", "created_at":"2026-10-01T12:00:00Z", "updated_at":"2026-10-07T12:00:00Z"}
            ]),
        );
        let manifest = single_operation_manifest(valid);
        let response =
            execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest.clone()))
                .await
                .unwrap()
                .0;
        assert_eq!(response.applied_operation_count, 1);
        let number: (String, String, String, String) = sqlx::query_as("SELECT id, value, created_at, updated_at FROM property_values WHERE property_id = 'z-number'").fetch_one(&state.db).await.unwrap();
        assert_eq!(
            number,
            (
                "legacy:value-number".into(),
                ".5".into(),
                "2026-10-01T12:00:00+00:00".into(),
                "2026-10-07T12:00:00+00:00".into()
            )
        );
        let text: (String, Option<String>, String) = sqlx::query_as(
            "SELECT id, value, created_at FROM property_values WHERE property_id = 'a-text'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            text,
            ("original-value".into(), None, "original-created".into())
        );
        let response = execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest))
            .await
            .unwrap()
            .0;
        assert_eq!(response.skipped_operation_count, 1);
    }

    #[tokio::test]
    async fn replay_properties_preserve_legacy_document_identity_fallback_and_reject_malformed_ids()
    {
        let state = property_replay_test_state().await;
        let mut legacy = property_values_operation(
            "legacy-values",
            json!([{"property_id":"a-text", "value":"Legacy update"}]),
        );
        legacy
            .payload
            .as_object_mut()
            .unwrap()
            .remove("document_id");
        assert_eq!(
            super::replay_document_property_values_set(&state, &auth_user(), &legacy)
                .await
                .unwrap(),
            "applied"
        );
        for malformed in [
            json!(null),
            json!(false),
            json!(""),
            json!("another-document"),
        ] {
            legacy
                .payload
                .as_object_mut()
                .unwrap()
                .insert("document_id".into(), malformed);
            assert!(
                super::replay_document_property_values_set(&state, &auth_user(), &legacy)
                    .await
                    .is_err()
            );
        }
        let value: String =
            sqlx::query_scalar("SELECT value FROM property_values WHERE id = 'original-value'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(value, "Legacy update");
    }

    #[tokio::test]
    async fn replay_properties_allow_compatible_schema_edits_and_empty_type_changes() {
        let state = property_replay_test_state().await;
        let compatible = replay_operation(
            "compatible-schema",
            "2026-10-07T12:00:00Z",
            "property_schema",
            "choice",
            "update",
            json!({"id":"choice", "space_id":"space-1", "name":"Review status", "property_type":"select", "options":["Ready", "Done", "Paused"], "sort_order":8}),
        );
        assert_eq!(
            super::replay_property_schema_update(&state, &auth_user(), &compatible)
                .await
                .unwrap(),
            "applied"
        );
        let schema: (String, String, i64, String) = sqlx::query_as("SELECT name, property_type, sort_order, created_at FROM properties_schema WHERE id = 'choice'").fetch_one(&state.db).await.unwrap();
        assert_eq!(
            schema,
            ("Review status".into(), "select".into(), 8, "old".into())
        );
        let conversion = replay_operation(
            "empty-conversion",
            "2026-10-07T12:00:00Z",
            "property_schema",
            "z-number",
            "update",
            json!({"id":"z-number", "space_id":"space-1", "name":"Empty text", "property_type":"text"}),
        );
        assert_eq!(
            super::replay_property_schema_update(&state, &auth_user(), &conversion)
                .await
                .unwrap(),
            "applied"
        );
        let schema: (String, i64) = sqlx::query_as(
            "SELECT property_type, sort_order FROM properties_schema WHERE id = 'z-number'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(schema, ("text".into(), 2));
    }

    #[tokio::test]
    async fn replay_properties_enforce_relation_scope_lifecycle_and_draft_access() {
        let state = property_replay_test_state().await;
        for change in [
            "space_id = 'space-2'",
            "deleted_at = 'old'",
            "archived_at = 'old'",
            "published_at = NULL",
        ] {
            sqlx::query("UPDATE documents SET space_id = 'space-1', published_at = 'old', deleted_at = NULL, archived_at = NULL WHERE id = 'target'").execute(&state.db).await.unwrap();
            sqlx::query(&format!(
                "UPDATE documents SET {change} WHERE id = 'target'"
            ))
            .execute(&state.db)
            .await
            .unwrap();
            let op = property_values_operation(
                "bad-relation",
                json!([{"property_id":"a-text", "value":"Do not persist"}, {"property_id":"relation", "value":"target"}]),
            );
            assert!(
                super::replay_document_property_values_set(&state, &auth_user(), &op)
                    .await
                    .is_err()
            );
            let old: String =
                sqlx::query_scalar("SELECT value FROM property_values WHERE id = 'original-value'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(old, "Original");
        }
        sqlx::query("INSERT INTO document_drafts (id, document_id, space_id, owner_id, title, base_updated_at) VALUES ('draft-target', 'target', 'space-1', 'other-user','Target','base')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access (draft_id, user_id, role, granted_by) VALUES ('draft-target', 'user-1', 'viewer', \'user-1\')").execute(&state.db).await.unwrap();
        let op = property_values_operation(
            "valid-relation",
            json!([{"property_id":"relation", "value":"target"}]),
        );
        assert_eq!(
            super::replay_document_property_values_set(&state, &auth_user(), &op)
                .await
                .unwrap(),
            "applied"
        );
    }

    #[tokio::test]
    async fn replay_offline_creation_keeps_owned_private_drafts_and_authorized_relations() {
        let mut state = property_replay_test_state().await;
        let slug = format!("offline-relation-{}", uuid::Uuid::new_v4());
        sqlx::query("UPDATE spaces SET slug = $1 WHERE id = 'space-1'")
            .bind(&slug)
            .execute(&state.db)
            .await
            .unwrap();
        let index_path =
            std::env::temp_dir().join(format!("offline-draft-index-{}", uuid::Uuid::new_v4()));
        state.search_index = crate::search::SearchIndex::new(index_path.clone()).unwrap();
        let operations = vec![
            replay_operation(
                "offline-create-a",
                "2026-10-07T12:00:00Z",
                "document",
                "offline-a",
                "create",
                json!({"id":"offline-a", "space_id":"space-1", "title":"Unpublished Secret A", "content":"# Secret A\nRaw author content", "icon":"lucide:file-text", "created_at":"2026-10-01T12:00:00Z", "updated_at":"2026-10-07T12:00:00Z"}),
            ),
            replay_operation(
                "offline-create-b",
                "2026-10-07T12:00:01Z",
                "document",
                "offline-b",
                "create",
                json!({"id":"offline-b", "space_id":"space-1", "title":"Unpublished Secret B", "content":"# Secret B\nPrivate target content", "icon":"lucide:book", "cover_image":"https://example.test/cover.png", "created_at":"2026-10-01T12:00:00Z", "updated_at":"2026-10-07T12:00:00Z"}),
            ),
            replay_operation(
                "offline-create-relation-schema",
                "2026-10-07T12:00:02Z",
                "property_schema",
                "offline-related",
                "create",
                json!({"id":"offline-related", "space_id":"space-1", "name":"Offline relation", "property_type":"relation"}),
            ),
            replay_operation(
                "offline-create-relation-value",
                "2026-10-07T12:00:03Z",
                "document_property_values",
                "offline-a",
                "set",
                json!({"document_id":"offline-a", "space_id":"space-1", "values":[{"id":"offline-relation-value", "property_id":"offline-related", "value":"offline-b"}]}),
            ),
        ];
        let mut manifest = single_operation_manifest(operations[0].clone());
        manifest.operation_count = operations.len();
        manifest.operations = operations;
        let response =
            execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest.clone()))
                .await
                .unwrap()
                .0;
        assert_eq!(response.status, "completed");
        assert_eq!(response.applied_operation_count, 4);
        let published: Vec<Option<String>> = sqlx::query_scalar(
            "SELECT published_at FROM documents WHERE id IN ('offline-a', 'offline-b')",
        )
        .fetch_all(&state.db)
        .await
        .unwrap();
        assert_eq!(published, vec![None, None]);
        let draft: (String, String, Option<String>, Option<String>, String, String, String, String) = sqlx::query_as("SELECT owner_id, title, icon, cover_image, content, base_updated_at, created_at, updated_at FROM document_drafts WHERE document_id = 'offline-b'").fetch_one(&state.db).await.unwrap();
        assert_eq!(
            draft,
            (
                "user-1".into(),
                "Unpublished Secret B".into(),
                Some("lucide:book".into()),
                Some("https://example.test/cover.png".into()),
                "# Secret B\nPrivate target content".into(),
                "2026-10-07T12:00:00+00:00".into(),
                "2026-10-01T12:00:00+00:00".into(),
                "2026-10-07T12:00:00+00:00".into()
            )
        );
        // Open a fresh reader so this assertion cannot pass merely because an async
        // existing index reader has not yet observed newly indexed secret content.
        let index = tantivy::Index::open_in_dir(&index_path).unwrap();
        let reader = index.reader().unwrap();
        assert_eq!(reader.searcher().num_docs(), 0);
        sqlx::query("INSERT INTO users (id, email, display_name, role) VALUES ('editor-2', 'editor2@example.test', 'Other editor', 'member')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('metadata-editor-2','editor-2','metadata-test','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_members (id, space_id, user_id, role, created_at) VALUES ('other-editor', 'space-1', 'editor-2', 'editor', 'old')").execute(&state.db).await.unwrap();
        let mut other_editor = auth_user();
        other_editor.claims.sub = "editor-2".into();
        other_editor.claims.sid = "metadata-editor-2".into();
        let relation = replay_operation(
            "other-editor-relation",
            "2026-10-07T12:00:04Z",
            "document_property_values",
            "offline-a",
            "set",
            json!({"document_id":"offline-a", "space_id":"space-1", "values":[{"property_id":"a-text", "value":"Should not persist"}, {"property_id":"offline-related", "value":"offline-b"}]}),
        );
        assert!(
            super::replay_document_property_values_set(&state, &other_editor, &relation)
                .await
                .is_err()
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM property_values WHERE document_id = 'offline-a' AND property_id = 'a-text'").fetch_one(&state.db).await.unwrap();
        assert_eq!(count, 0);
        sqlx::query("INSERT INTO document_draft_access (draft_id, user_id, role, granted_by) SELECT id, \'editor-2\', \'viewer\', \'user-1\' FROM document_drafts WHERE document_id = 'offline-b'").execute(&state.db).await.unwrap();
        // Reading the relation target does not authorize editing its private source.
        assert!(super::replay_document_property_values_set(&state, &other_editor, &relation).await.is_err());
        sqlx::query("INSERT INTO document_draft_access (draft_id, user_id, role, granted_by) SELECT id, \'editor-2\', \'editor\', \'user-1\' FROM document_drafts WHERE document_id = 'offline-a'").execute(&state.db).await.unwrap();
        assert_eq!(
            super::replay_document_property_values_set(&state, &other_editor, &relation)
                .await
                .unwrap(),
            "applied"
        );
        let response = execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest))
            .await
            .unwrap()
            .0;
        assert_eq!(response.skipped_operation_count, 4);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM document_drafts WHERE document_id IN ('offline-a', 'offline-b')",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(count, 2);
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn replay_offline_creation_rolls_back_page_when_initial_draft_fails() {
        let state = property_replay_test_state().await;
        let slug = format!("offline-draft-rollback-{}", uuid::Uuid::new_v4());
        sqlx::query("UPDATE spaces SET slug = $1 WHERE id = 'space-1'")
            .bind(&slug)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER fail_initial_draft BEFORE INSERT ON document_drafts BEGIN SELECT RAISE(ABORT, 'forced draft failure'); END").execute(&state.db).await.unwrap();
        let op = replay_operation(
            "offline-create-failure",
            "2026-10-07T12:00:00Z",
            "document",
            "failed-page",
            "create",
            json!({"id":"failed-page", "space_id":"space-1", "title":"Should roll back", "content":"Recoverable queued content"}),
        );
        assert!(super::replay_document_create(&state, &auth_user(), "self-hosted", &op)
            .await
            .is_err());
        let page_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE id = 'failed-page'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let draft_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM document_drafts WHERE document_id = 'failed-page'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        let subscription_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM doc_subscriptions WHERE document_id = 'failed-page'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!((page_count, draft_count, subscription_count), (0, 0, 0));
        assert!(!crate::storage::space_dir(&slug).exists(), "Failed creation must not install a canonical mirror");
    }

    #[tokio::test]
    async fn replay_create_then_edit_keeps_saved_body_in_authorized_private_draft() {
        let mut state = property_replay_test_state().await;
        let slug = format!("private-replay-edit-{}", uuid::Uuid::new_v4());
        sqlx::query("UPDATE spaces SET slug = $1 WHERE id = 'space-1'")
            .bind(&slug)
            .execute(&state.db)
            .await
            .unwrap();
        let index_path =
            std::env::temp_dir().join(format!("private-replay-index-{}", uuid::Uuid::new_v4()));
        state.search_index = crate::search::SearchIndex::new(index_path.clone()).unwrap();
        let created = replay_operation(
            "private-create",
            "2026-10-07T12:00:00Z",
            "document",
            "private-page",
            "create",
            json!({"id":"private-page", "space_id":"space-1", "title":"Secret private page", "content":"Initial raw body", "updated_at":"2026-10-07T12:00:00Z"}),
        );
        let response = execute_replay_manifest(
            State(state.clone()),
            auth_user(),
            Json(single_operation_manifest(created)),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(response.applied_operation_count, 1);
        let (path, shell_updated): (String, String) = sqlx::query_as(
            "SELECT content_path, updated_at FROM documents WHERE id = 'private-page'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        let shell = crate::storage::read_document(&path).await.unwrap();
        let draft_id: String =
            sqlx::query_scalar("SELECT id FROM document_drafts WHERE document_id = 'private-page'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let mut edit = replay_operation(
            "private-edit",
            "2026-10-07T12:10:00Z",
            "document",
            "private-page",
            "update_content",
            json!({"id":"private-page", "space_id":"space-1", "content":"Latest secret draft body", "updated_at":"2026-10-07T12:10:00Z"}),
        );
        edit.payload["draft_id"]=json!(draft_id);
        edit.payload["source_digest"]=json!(crate::spaces::draft_save::digest(&crate::spaces::draft_save::selected(&mut state.db.acquire().await.unwrap(),"space-1","private-page",&draft_id).await.unwrap()).unwrap());
        let response = execute_replay_manifest(
            State(state.clone()),
            auth_user(),
            Json(single_operation_manifest(edit.clone())),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(response.applied_operation_count, 1);
        let draft: (String, String, String) = sqlx::query_as(
            "SELECT content, updated_at, base_updated_at FROM document_drafts WHERE id = $1",
        )
        .bind(&draft_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            draft,
            (
                "Latest secret draft body".into(),
                "2026-10-07T12:10:00+00:00".into(),
                shell_updated.clone()
            )
        );
        let page: (Option<String>, String) = sqlx::query_as(
            "SELECT published_at, updated_at FROM documents WHERE id = 'private-page'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(page, (None, shell_updated));
        assert_eq!(crate::storage::read_document(&path).await.unwrap(), shell);
        let index = tantivy::Index::open_in_dir(&index_path).unwrap();
        assert_eq!(index.reader().unwrap().searcher().num_docs(), 0);
        sqlx::query("INSERT INTO users (id, email, display_name, role) VALUES ('editor-2', 'other@example.test', 'Other editor', 'member')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('metadata-editor-2','editor-2','metadata-test','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_members (id, space_id, user_id, role, created_at) VALUES ('other-editor', 'space-1', 'editor-2', 'editor', 'old')").execute(&state.db).await.unwrap();
        let mut other = auth_user();
        other.claims.sub = "editor-2".into();
        other.claims.sid = "metadata-editor-2".into();
        let mut denied = edit.clone();
        denied.id = "private-denied-edit".into();
        denied.payload["content"] = json!("Unauthorized saved content must remain recoverable");
        let response = execute_replay_manifest(
            State(state.clone()),
            other,
            Json(single_operation_manifest(denied.clone())),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(response.failed_operation_count, 1);
        let payload: String = sqlx::query_scalar(
            "SELECT payload FROM sync_replay_operations WHERE operation_id = 'private-denied-edit'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&payload).unwrap(),
            denied.payload
        );
        assert_eq!(crate::storage::read_document(&path).await.unwrap(), shell);
        let unchanged: String =
            sqlx::query_scalar("SELECT content FROM document_drafts WHERE id = $1")
                .bind(&draft_id)
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(unchanged, "Latest secret draft body");
        let mut explicit = edit.clone();
        explicit.payload["draft_id"] = json!(draft_id);
        sqlx::query("INSERT INTO document_draft_access (draft_id, user_id, role, granted_by) VALUES ($1, 'editor-2', 'viewer', \'user-1\')").bind(&draft_id).execute(&state.db).await.unwrap();
        let mut other = auth_user();
        other.claims.sub = "editor-2".into();
        other.claims.sid = "metadata-editor-2".into();
        assert!(
            super::replay_document_update_content(&state, &other, "self-hosted", &explicit)
                .await
                .is_err()
        );
        sqlx::query("UPDATE document_draft_access SET role = 'editor' WHERE draft_id = $1")
            .bind(&draft_id)
            .execute(&state.db)
            .await
            .unwrap();
        // A shared editor must identify the draft explicitly, never auto-select it.
        assert!(
            super::replay_document_update_content(&state, &other, "self-hosted", &edit)
                .await
                .is_err()
        );
        explicit.payload["content"] = json!("Explicit shared draft edit");
        explicit.id="shared-source-bound-save".into();
        explicit.payload["source_digest"]=json!(crate::spaces::draft_save::digest(&crate::spaces::draft_save::selected(&mut state.db.acquire().await.unwrap(),"space-1","private-page",&draft_id).await.unwrap()).unwrap());
        assert_eq!(
            super::replay_document_update_content(&state, &other, "self-hosted", &explicit)
                .await
                .unwrap(),
            "applied"
        );
        sqlx::query("INSERT INTO document_drafts (id, document_id, space_id, owner_id, title, content, base_updated_at) VALUES ('draft-from-another-page', 'target', 'space-1', 'user-1', 'Target', 'Other draft untouched', 'base')").execute(&state.db).await.unwrap();
        explicit.payload["draft_id"] = json!("draft-from-another-page");
        assert!(super::replay_document_update_content(
            &state,
            &auth_user(),
            "self-hosted",
            &explicit
        )
        .await
        .is_err());
        let foreign: String = sqlx::query_scalar(
            "SELECT content FROM document_drafts WHERE id = 'draft-from-another-page'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(foreign, "Other draft untouched");
        assert_eq!(crate::storage::read_document(&path).await.unwrap(), shell);
        assert_eq!(index.reader().unwrap().searcher().num_docs(), 0);
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn replay_private_edit_requires_original_explicit_draft_and_rejects_stale_scope() {
        let state = property_replay_test_state().await;
        let slug = format!("private-replay-scope-{}", uuid::Uuid::new_v4());
        sqlx::query("UPDATE spaces SET slug = $1 WHERE id = 'space-1'")
            .bind(&slug)
            .execute(&state.db)
            .await
            .unwrap();
        let created = replay_operation(
            "private-scope-create",
            "2026-10-07T12:00:00Z",
            "document",
            "private-page",
            "create",
            json!({"id":"private-page", "space_id":"space-1", "title":"Private", "content":"Initial raw body", "updated_at":"2026-10-07T12:00:00Z"}),
        );
        super::replay_document_create(&state, &auth_user(), "self-hosted", &created)
            .await
            .unwrap();
        for id in ["a-owned", "z-owned"] {
            sqlx::query("INSERT INTO document_drafts (id, document_id, space_id, owner_id, content, base_updated_at, updated_at, title) VALUES ($1, 'private-page', 'space-1', 'user-1', 'Old draft', 'original-base', '2026-10-07T12:01:00Z', 'Private')").bind(id).execute(&state.db).await.unwrap();
        }
        let mut edit = replay_operation(
            "private-scope-edit",
            "2026-10-07T12:10:00Z",
            "document",
            "private-page",
            "update_content",
            json!({"id":"private-page", "space_id":"space-1", "content":"Deterministic edit", "updated_at":"2026-10-07T12:10:00Z"}),
        );
        assert!(super::replay_document_update_content(&state,&auth_user(),"self-hosted",&edit).await.unwrap_err().contains("source_required"));
        edit.payload["draft_id"]=json!("z-owned");
        edit.payload["source_digest"]=json!(crate::spaces::draft_save::digest(&crate::spaces::draft_save::selected(&mut state.db.acquire().await.unwrap(),"space-1","private-page","z-owned").await.unwrap()).unwrap());
        super::replay_document_update_content(&state,&auth_user(),"self-hosted",&edit).await.unwrap();
        let drafts: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, content FROM document_drafts WHERE id IN ('a-owned','z-owned') ORDER BY id",
        )
        .fetch_all(&state.db)
        .await
        .unwrap();
        assert_eq!(
            drafts,
            vec![
                ("a-owned".into(), "Old draft".into()),
                ("z-owned".into(), "Deterministic edit".into())
            ]
        );
        for bad_scope in [json!(null), json!(false), json!("space-2")] {
            edit.payload["space_id"] = bad_scope;
            assert!(super::replay_document_update_content(
                &state,
                &auth_user(),
                "self-hosted",
                &edit
            )
            .await
            .is_err());
        }
        edit.payload["space_id"] = json!("space-1");
        edit.id="new-source-lifecycle-intent".into();
        edit.payload["source_digest"]=json!(crate::spaces::draft_save::digest(&crate::spaces::draft_save::selected(&mut state.db.acquire().await.unwrap(),"space-1","private-page","z-owned").await.unwrap()).unwrap());
        for lifecycle in ["archived_at", "deleted_at"] {
            sqlx::query(&format!(
                "UPDATE documents SET {lifecycle} = 'old' WHERE id = 'private-page'"
            ))
            .execute(&state.db)
            .await
            .unwrap();
            assert!(super::replay_document_update_content(
                &state,
                &auth_user(),
                "self-hosted",
                &edit
            )
            .await
            .is_err());
            sqlx::query(&format!(
                "UPDATE documents SET {lifecycle} = NULL WHERE id = 'private-page'"
            ))
            .execute(&state.db)
            .await
            .unwrap();
        }
        sqlx::query("UPDATE spaces SET name='Other',slug='other' WHERE id='space-2'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO space_members (id, space_id, user_id, role, created_at) VALUES ('second-membership', 'space-2', 'user-1', 'editor', 'old')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET space_id = 'space-2' WHERE id = 'private-page'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE document_drafts SET space_id = 'space-2' WHERE document_id = 'private-page'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        // The actor can edit both spaces, but their queued old scope is stale.
        assert!(
            super::replay_document_update_content(&state, &auth_user(), "self-hosted", &edit)
                .await
                .is_err()
        );
        let content: String =
            sqlx::query_scalar("SELECT content FROM document_drafts WHERE id = 'z-owned'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(content, "Deterministic edit");
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn replay_metadata_indexes_only_active_published_pages() {
        let mut state = property_replay_test_state().await;
        let slug = format!("private-replay-index-{}", uuid::Uuid::new_v4());
        sqlx::query("UPDATE spaces SET slug = $1 WHERE id = 'space-1'")
            .bind(&slug)
            .execute(&state.db)
            .await
            .unwrap();
        let index_path =
            std::env::temp_dir().join(format!("replay-index-guard-{}", uuid::Uuid::new_v4()));
        state.search_index = crate::search::SearchIndex::new(index_path.clone()).unwrap();
        let count = || {
            tantivy::Index::open_in_dir(&index_path)
                .unwrap()
                .reader()
                .unwrap()
                .searcher()
                .num_docs()
        };
        let create = replay_operation(
            "index-private-create",
            "2026-10-07T12:00:00Z",
            "document",
            "private-index-page",
            "create",
            json!({"id":"private-index-page", "space_id":"space-1", "title":"Private title", "content":"Secret body private-asset-ref published-asset-ref"}),
        );
        super::replay_document_create(&state, &auth_user(), "self-hosted", &create)
            .await
            .unwrap();
        assert_eq!(count(), 0);
        // Repair a stale entry left by an older consumer when metadata is replayed.
        state
            .search_index
            .upsert(crate::search::index::DocEntry {
                doc_id: "private-index-page".into(),
                space_id: "space-1".into(),
                title: "Stale private title".into(),
                body: "Stale secret body".into(),
                tags: String::new(),
            })
            .unwrap();
        assert_eq!(count(), 1);
        let metadata = replay_operation(
            "index-private-metadata",
            "2026-10-07T12:01:00Z",
            "document",
            "private-index-page",
            "update",
            json!({"id":"private-index-page", "space_id":"space-1", "title":"Renamed private title"}),
        );
        super::replay_document_update(&state, &auth_user(), "self-hosted", &metadata)
            .await
            .unwrap();
        assert_eq!(count(), 0);
        sqlx::query("UPDATE documents SET published_at = '2026-10-07T12:02:00Z' WHERE id = 'private-index-page'").execute(&state.db).await.unwrap();
        super::replay_document_update(&state, &auth_user(), "self-hosted", &metadata)
            .await
            .unwrap();
        // Exact historical replay acknowledges without reapplying or reindexing
        // a different later publication. Independent index repair stays advisory.
        assert_eq!(count(), 0);
        super::index_replay_published_document(&state,"private-index-page").await.unwrap();
        assert_eq!(count(), 1);
        for lifecycle in [
            "published_at = NULL",
            "archived_at = '2026-10-07T12:03:00Z'",
            "deleted_at = '2026-10-07T12:03:00Z'",
        ] {
            sqlx::query("UPDATE documents SET published_at = '2026-10-07T12:02:00Z', archived_at = NULL, deleted_at = NULL WHERE id = 'private-index-page'").execute(&state.db).await.unwrap();
            super::index_replay_published_document(&state, "private-index-page")
                .await
                .unwrap();
            assert_eq!(count(), 1);
            sqlx::query(&format!(
                "UPDATE documents SET {lifecycle} WHERE id = 'private-index-page'"
            ))
            .execute(&state.db)
            .await
            .unwrap();
            super::index_replay_published_document(&state, "private-index-page")
                .await
                .unwrap();
            assert_eq!(count(), 0);
        }
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn replay_property_core_preserves_legacy_import_and_rejects_false_identity_receipts() {
        let state = crate::spaces::properties::tests::property_test_state().await;
        let payload = json!({"id":"legacy:property","space_id":"space-a","name":"  Legacy name  ","property_type":"select","options":["A","B"],"sort_order":17,"created_at":"2026-10-01T00:00:00Z"});
        for (id, expected) in [
            ("property-import", "applied"),
            ("property-import-repeated", "skipped"),
        ] {
            assert_eq!(
                run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        id,
                        "property_schema",
                        "legacy:property",
                        "create",
                        payload.clone()
                    )
                )
                .await
                .status,
                expected
            );
        }
        for (index, (key, value)) in [
            ("name", json!("Different")),
            ("options", json!(["A"])),
            ("created_at", json!("2026-10-02T00:00:00Z")),
            ("sort_order", json!(18)),
        ]
        .into_iter()
        .enumerate()
        {
            let mut changed = payload.clone();
            changed[key] = value;
            assert_eq!(
                run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        &format!("property-conflict-{index}"),
                        "property_schema",
                        "legacy:property",
                        "create",
                        changed.clone()
                    )
                )
                .await
                .status,
                "failed"
            );
            let stored: String = sqlx::query_scalar(
                "SELECT payload FROM sync_replay_operations WHERE operation_id=$1",
            )
            .bind(format!("property-conflict-{index}"))
            .fetch_one(&state.db)
            .await
            .unwrap();
            assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(), changed);
        }
        let row: (String, i32, String) = sqlx::query_as(
            "SELECT name,sort_order,created_at FROM properties_schema WHERE id='legacy:property'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            row,
            (
                "  Legacy name  ".into(),
                17,
                "2026-10-01T00:00:00+00:00".into()
            )
        );
        let op = tag_replay_operation(
            "property-value-import",
            "document_property_values",
            "target-a",
            "set",
            json!({"space_id":"space-a","values":[{"id":"legacy:value","property_id":"legacy:property","value":"A","created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-02T00:00:00Z"}]}),
        );
        assert_eq!(
            run_tag_operation(&state, "editor-a", op).await.status,
            "applied"
        );
        let row:(String,String,String)=sqlx::query_as("SELECT id,created_at,updated_at FROM property_values WHERE property_id='legacy:property'").fetch_one(&state.db).await.unwrap();
        assert_eq!(
            row,
            (
                "legacy:value".into(),
                "2026-10-01T00:00:00+00:00".into(),
                "2026-10-02T00:00:00+00:00".into()
            )
        );
    }

    #[tokio::test]
    async fn replay_property_core_reorder_is_atomic_and_delete_preserves_scope_and_owner_policy() {
        let state = crate::spaces::properties::tests::property_test_state().await;
        let before: Vec<(String, i32)> =
            sqlx::query_as("SELECT id,sort_order FROM properties_schema ORDER BY id")
                .fetch_all(&state.db)
                .await
                .unwrap();
        for (i, payload) in [
            json!({"space_id":"space-a","property_ids":["select-a","foreign"]}),
            json!({"space_id":"space-a","property_ids":["select-a","select-a"]}),
            json!({"space_id":"space-a","property_ids":["select-a",false]}),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        &format!("property-reorder-invalid-{i}"),
                        "property_schema",
                        "space-a",
                        "reorder",
                        payload
                    )
                )
                .await
                .status,
                "failed"
            );
        }
        assert_eq!(
            sqlx::query_as::<_, (String, i32)>(
                "SELECT id,sort_order FROM properties_schema ORDER BY id"
            )
            .fetch_all(&state.db)
            .await
            .unwrap(),
            before
        );
        assert_eq!(
            run_tag_operation(
                &state,
                "viewer-a",
                tag_replay_operation(
                    "property-viewer-reorder",
                    "property_schema",
                    "space-a",
                    "reorder",
                    json!({"space_id":"space-a","property_ids":["select-a"]})
                )
            )
            .await
            .status,
            "failed"
        );
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "property-editor-delete",
                    "property_schema",
                    "number-a",
                    "delete",
                    json!({"space_id":"space-a"})
                )
            )
            .await
            .status,
            "failed"
        );
        let mut messages = Vec::new();
        for id in ["missing", "foreign"] {
            messages.push(
                run_tag_operation(
                    &state,
                    "owner-a",
                    tag_replay_operation(
                        &format!("property-delete-{id}"),
                        "property_schema",
                        id,
                        "delete",
                        json!({"space_id":"space-a"}),
                    ),
                )
                .await
                .message,
            );
        }
        assert_eq!(messages[0], messages[1]);
        sqlx::query("INSERT INTO property_values(id,document_id,property_id,value,created_at,updated_at) VALUES('corrupt-consumer','target-b','number-a','1','old','old')").execute(&state.db).await.unwrap();
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "property-foreign-cascade",
                    "property_schema",
                    "number-a",
                    "delete",
                    json!({"space_id":"space-a"})
                )
            )
            .await
            .status,
            "failed"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM property_values WHERE id='corrupt-consumer'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn replay_property_core_requires_private_participation_and_active_unlocked_pages() {
        let state = crate::spaces::properties::tests::property_test_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at,updated_at) VALUES('property-draft','target-a','space-a','owner-a','Private','','old','old','old')").execute(&state.db).await.unwrap();
        let payload =
            json!({"space_id":"space-a","values":[{"property_id":"number-a","value":"3"}]});
        for (id, share, expected) in [
            ("property-private", None, "failed"),
            ("property-shared-viewer", Some("viewer"), "failed"),
            ("property-shared-editor", Some("editor"), "applied"),
        ] {
            if let Some(role) = share {
                sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by,created_at) VALUES('property-draft','editor-a',$1,'owner-a','old') ON CONFLICT(draft_id,user_id) DO UPDATE SET role=$1").bind(role).execute(&state.db).await.unwrap();
            }
            assert_eq!(
                run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        id,
                        "document_property_values",
                        "target-a",
                        "set",
                        payload.clone()
                    )
                )
                .await
                .status,
                expected
            );
        }
        for (i,change) in ["UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'","UPDATE documents SET locked=0,archived_at='old' WHERE id='target-a'","UPDATE documents SET archived_at=NULL,deleted_at='old' WHERE id='target-a'","UPDATE documents SET deleted_at=NULL WHERE id='target-a'; UPDATE users SET is_active=0 WHERE id='editor-a'"].into_iter().enumerate() {
            sqlx::raw_sql(change).execute(&state.db).await.unwrap();
            let mut changed=payload.clone();changed["values"][0]["value"]=json!("99");
            assert_eq!(run_tag_operation(&state,"editor-a",tag_replay_operation(&format!("property-lifecycle-{i}"),"document_property_values","target-a","set",changed)).await.status,"failed");
        }
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT value FROM property_values WHERE document_id='target-a' AND property_id='number-a'").fetch_one(&state.db).await.unwrap(),"3");
    }

    #[tokio::test]
    async fn replay_property_core_rolls_back_late_write_revocation_and_keeps_failed_payload() {
        let state = crate::spaces::properties::tests::property_test_state().await;
        sqlx::query("CREATE TRIGGER property_replay_late_revoke AFTER INSERT ON property_values BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END").execute(&state.db).await.unwrap();
        let payload =
            json!({"space_id":"space-a","values":[{"property_id":"number-a","value":"5"}]});
        let op = tag_replay_operation(
            "property-late-revoke",
            "document_property_values",
            "target-a",
            "set",
            payload.clone(),
        );
        assert_eq!(
            run_tag_operation(&state, "editor-a", op.clone())
                .await
                .status,
            "failed"
        );
        assert_eq!(
            run_tag_operation(&state, "editor-a", op).await.status,
            "failed"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM property_values")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT is_active FROM users WHERE id='editor-a'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
        let row:(String,String)=sqlx::query_as("SELECT status,payload FROM sync_replay_operations WHERE operation_id='property-late-revoke'").fetch_one(&state.db).await.unwrap();
        assert_eq!(row.0, "failed");
        assert_eq!(serde_json::from_str::<Value>(&row.1).unwrap(), payload);
    }

    #[tokio::test]
    async fn replay_property_core_rejects_malformed_schema_identity_and_sort_before_writes() {
        let state = crate::spaces::properties::tests::property_test_state().await;
        let base =
            json!({"id":"new-property","space_id":"space-a","name":"New","property_type":"text"});
        for (i, (field, value)) in [
            ("id", Value::Null),
            ("created_at", json!(9)),
            ("space_id", Value::Null),
            ("sort_order", json!(2147483648_i64)),
            ("sort_order", json!(1.5)),
        ]
        .into_iter()
        .enumerate()
        {
            let mut payload = base.clone();
            payload[field] = value;
            assert_eq!(
                run_tag_operation(
                    &state,
                    "owner-a",
                    tag_replay_operation(
                        &format!("property-malformed-{i}"),
                        "property_schema",
                        "new-property",
                        "create",
                        payload
                    )
                )
                .await
                .status,
                "failed"
            );
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM properties_schema WHERE id='new-property'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn replay_property_core_rechecks_waited_policy_and_custom_edit_permission() {
        let state = crate::spaces::properties::tests::property_test_state().await;
        sqlx::query("INSERT INTO space_roles (id,space_id,name,permissions,created_by) VALUES ('property-custom','space-a','Custom','[\"edit_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='property-custom' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        let payload =
            json!({"space_id":"space-a","values":[{"property_id":"number-a","value":"7"}]});
        let initial = tag_replay_operation(
            "property-custom-value",
            "document_property_values",
            "target-a",
            "set",
            payload.clone(),
        );
        assert_eq!(
            run_tag_operation(&state, "editor-a", initial).await.status,
            "applied"
        );
        let mut blocker = state.db.begin().await.unwrap();
        crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut blocker, &["space-a"])
            .await
            .unwrap();
        sqlx::query("UPDATE space_members SET role='viewer' WHERE user_id='editor-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let cloned = state.clone();
        let mut changed = payload;
        changed["values"][0]["value"] = json!("88");
        let queued = tag_replay_operation(
            "property-waited-revocation",
            "document_property_values",
            "target-a",
            "set",
            changed,
        );
        let mut task = tokio::spawn(async move {
            super::replay_document_property_values_set(
                &cloned,
                &crate::spaces::properties::tests::test_auth("editor-a"),
                &queued,
            )
            .await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut task)
                .await
                .is_err()
        );
        blocker.commit().await.unwrap();
        assert_eq!(
            task.await.unwrap().unwrap_err(),
            "Property/page target unavailable"
        );
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT value FROM property_values WHERE document_id='target-a' AND property_id='number-a'").fetch_one(&state.db).await.unwrap(),"7");
    }

    #[tokio::test]
    async fn replay_property_core_schema_final_gate_rolls_back_update_and_delete_cascades() {
        let state = crate::spaces::properties::tests::property_test_state().await;
        sqlx::query("CREATE TRIGGER property_replay_update_revoke AFTER UPDATE OF name ON properties_schema WHEN NEW.name='Changed' BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END").execute(&state.db).await.unwrap();
        assert_eq!(run_tag_operation(&state,"editor-a",tag_replay_operation("property-schema-late-revoke","property_schema","number-a","update",json!({"space_id":"space-a","name":"Changed","property_type":"number","sort_order":17}))).await.status,"failed");
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT name FROM properties_schema WHERE id='number-a'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            "Count"
        );
        sqlx::query("INSERT INTO property_values(id,document_id,property_id,value,created_at,updated_at) VALUES('delete-value','target-a','number-a','1','old','old')").execute(&state.db).await.unwrap();
        sqlx::query("CREATE TRIGGER property_replay_delete_revoke AFTER DELETE ON properties_schema BEGIN UPDATE users SET is_active=0 WHERE id='owner-a'; END").execute(&state.db).await.unwrap();
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "property-schema-delete-late-revoke",
                    "property_schema",
                    "number-a",
                    "delete",
                    json!({"space_id":"space-a"})
                )
            )
            .await
            .status,
            "failed"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM properties_schema WHERE id='number-a'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM property_values WHERE id='delete-value'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT is_active FROM users WHERE id='owner-a'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
    }

    fn workflow_import_payload(id: &str, document: &str) -> Value {
        json!({"id":id,"document_id":document,"space_id":"space-a","workflow_template_id":"template-a","current_stage_id":"review","status":"active","created_by":"owner-a","created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-02T00:00:00Z","event":{"id":format!("{id}:enable"),"document_workflow_id":id,"document_id":document,"to_stage_id":"review","action":"enable","created_by":"owner-a","created_at":"2026-10-01T00:00:00Z"}})
    }
    fn workflow_template_payload(id: &str) -> Value {
        json!({"id":id,"space_id":"space-a","name":"Review","description":null,"stages":[{"id":"review","name":"Review","sort_order":0},{"id":"done","name":"Done","sort_order":1}],"approvers":[],"created_by":"owner-a","created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-02T00:00:00Z"})
    }
    async fn workflow_session_test_state() -> AppState {
        let state = crate::spaces::properties::tests::property_test_state().await;
        for user in ["owner-a", "editor-a", "viewer-a"] {
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES ($1,$2,'test','2099-01-01T00:00:00Z')").bind(format!("workflow-session-{user}")).bind(user).execute(&state.db).await.unwrap();
        }

        state
    }
    async fn workflow_replay_test_state() -> AppState {
        let state = workflow_session_test_state().await;
        let payload = workflow_template_payload("template-a");
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "workflow-fixture-template",
                    "workflow_template",
                    "template-a",
                    "create",
                    payload
                )
            )
            .await
            .status,
            "applied"
        );
        state
    }
    #[tokio::test]
    async fn replay_workflow_core_preserves_legacy_identity_times_and_historical_authors() {
        let state = workflow_replay_test_state().await;
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "workflow-template-repeat",
                    "workflow_template",
                    "template-a",
                    "create",
                    workflow_template_payload("template-a")
                )
            )
            .await
            .status,
            "skipped"
        );
        let payload = workflow_import_payload("legacy:workflow", "target-a");
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-import",
                    "document_workflow",
                    "legacy:workflow",
                    "create",
                    payload.clone()
                )
            )
            .await
            .status,
            "applied"
        );
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-import-again",
                    "document_workflow",
                    "legacy:workflow",
                    "update",
                    payload
                )
            )
            .await
            .status,
            "applied"
        );
        let row:(String,String,String,String)=sqlx::query_as("SELECT id,created_by,created_at,updated_at FROM document_workflows WHERE document_id='target-a'").fetch_one(&state.db).await.unwrap();
        assert_eq!(
            row,
            (
                "legacy:workflow".into(),
                "owner-a".into(),
                "2026-10-01T00:00:00+00:00".into(),
                "2026-10-02T00:00:00+00:00".into()
            )
        );
        let rows: Vec<(String, String, String)> =
            sqlx::query_as("SELECT id,created_by,created_at FROM document_workflow_events")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(
            rows,
            vec![(
                "legacy:workflow:enable".into(),
                "owner-a".into(),
                "2026-10-01T00:00:00+00:00".into()
            )]
        );
        let mut changed = workflow_template_payload("template-a");
        changed["name"] = json!("Wrong");
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "workflow-template-collision",
                    "workflow_template",
                    "template-a",
                    "create",
                    changed
                )
            )
            .await
            .status,
            "failed"
        );
    }
    #[tokio::test]
    async fn replay_workflow_core_binds_nested_identity_and_rejects_malformed_fields_without_state()
    {
        let state = workflow_replay_test_state().await;
        for (index, (field, value)) in [
            ("document_id", json!("target-b")),
            ("document_workflow_id", json!("foreign-workflow")),
            ("created_by", json!("missing-user")),
            ("created_at", json!(false)),
        ]
        .into_iter()
        .enumerate()
        {
            let mut payload = workflow_import_payload("workflow-a", "target-a");
            payload["event"][field] = value;
            assert_eq!(
                run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        &format!("workflow-forged-{index}"),
                        "document_workflow",
                        "workflow-a",
                        "create",
                        payload
                    )
                )
                .await
                .status,
                "failed"
            );
        }
        for (index, (field, value)) in [
            ("id", Value::Null),
            ("space_id", json!(42)),
            ("event", Value::Null),
            ("created_by", json!(false)),
            ("created_at", json!(false)),
            ("status", json!(false)),
        ]
        .into_iter()
        .enumerate()
        {
            let mut payload = workflow_import_payload("workflow-a", "target-a");
            payload[field] = value;
            assert_eq!(
                run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        &format!("workflow-malformed-{index}"),
                        "document_workflow",
                        "workflow-a",
                        "create",
                        payload
                    )
                )
                .await
                .status,
                "failed"
            );
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_workflows")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_workflow_events")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn replay_workflow_core_rolls_back_state_when_history_or_final_actor_check_fails() {
        let state = workflow_replay_test_state().await;
        sqlx::query("CREATE TRIGGER fail_replay_workflow_event BEFORE INSERT ON document_workflow_events BEGIN SELECT RAISE(ABORT,'forced history failure'); END").execute(&state.db).await.unwrap();
        let payload = workflow_import_payload("workflow-a", "target-a");
        let op = tag_replay_operation(
            "workflow-history-failure",
            "document_workflow",
            "workflow-a",
            "create",
            payload.clone(),
        );
        assert_eq!(
            run_tag_operation(&state, "editor-a", op.clone())
                .await
                .status,
            "failed"
        );
        assert_eq!(
            run_tag_operation(&state, "editor-a", op).await.status,
            "failed"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_workflows")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
        let raw:String=sqlx::query_scalar("SELECT payload FROM sync_replay_operations WHERE operation_id='workflow-history-failure'").fetch_one(&state.db).await.unwrap();
        assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), payload);
        sqlx::query("DROP TRIGGER fail_replay_workflow_event")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER revoke_replay_workflow_actor AFTER INSERT ON document_workflow_events BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END").execute(&state.db).await.unwrap();
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-late-revoke",
                    "document_workflow",
                    "workflow-a",
                    "create",
                    payload
                )
            )
            .await
            .status,
            "failed"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_workflows")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_workflow_events")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT is_active FROM users WHERE id='editor-a'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
    }
    #[tokio::test]
    async fn replay_workflow_core_requires_private_editor_share_and_preserves_lock_lifecycle() {
        let state = workflow_replay_test_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at,updated_at) VALUES('workflow-private','target-a','space-a','owner-a','Private','','old','old','old')").execute(&state.db).await.unwrap();
        let payload = workflow_import_payload("workflow-a", "target-a");
        for (id, share, expected) in [
            ("workflow-private-unshared", None, "failed"),
            ("workflow-private-viewer", Some("viewer"), "failed"),
            ("workflow-private-editor", Some("editor"), "applied"),
        ] {
            if let Some(role) = share {
                sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('workflow-private','editor-a',$1,'owner-a') ON CONFLICT(draft_id,user_id) DO UPDATE SET role=$1").bind(role).execute(&state.db).await.unwrap();
            }
            assert_eq!(
                run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        id,
                        "document_workflow",
                        "workflow-a",
                        "create",
                        payload.clone()
                    )
                )
                .await
                .status,
                expected
            );
        }
        for (index, statement) in [
            "UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'",
            "UPDATE documents SET locked=0,archived_at='old' WHERE id='target-a'",
            "UPDATE documents SET archived_at=NULL,deleted_at='old' WHERE id='target-a'",
        ]
        .into_iter()
        .enumerate()
        {
            sqlx::query(statement).execute(&state.db).await.unwrap();
            assert_eq!(
                run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        &format!("workflow-private-blocked-{index}"),
                        "document_workflow",
                        "target-a",
                        "delete",
                        json!({"document_id":"target-a","id":"workflow-a","space_id":"space-a"})
                    )
                )
                .await
                .status,
                "failed"
            );
        }
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_workflows")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
    }
    #[tokio::test]
    async fn replay_workflow_core_transition_enforces_current_approver_and_bound_template() {
        let state = workflow_replay_test_state().await;
        let enable = workflow_import_payload("workflow-a", "target-a");
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-approver-enable",
                    "document_workflow",
                    "workflow-a",
                    "create",
                    enable.clone()
                )
            )
            .await
            .status,
            "applied"
        );
        sqlx::query("UPDATE workflow_templates SET approvers='[{\"id\":\"owner-rule\",\"approver_type\":\"role\",\"approver_id\":\"owner\",\"stage_id\":\"done\"}]' WHERE id='template-a'").execute(&state.db).await.unwrap();
        let mut transition = enable;
        transition["current_stage_id"] = json!("done");
        transition["updated_at"] = json!("2026-10-03T00:00:00Z");
        transition["event"] = json!({"id":"legacy:transition","document_workflow_id":"workflow-a","document_id":"target-a","from_stage_id":"review","to_stage_id":"done","action":"transition","created_by":"owner-a","created_at":"2026-10-03T00:00:00Z"});
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-approver-denied",
                    "document_workflow",
                    "workflow-a",
                    "transition",
                    transition.clone()
                )
            )
            .await
            .status,
            "failed"
        );
        let mut foreign = transition.clone();
        foreign["workflow_template_id"] = json!("foreign-template");
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "workflow-bound-template",
                    "document_workflow",
                    "workflow-a",
                    "transition",
                    foreign
                )
            )
            .await
            .status,
            "failed"
        );
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "workflow-approver-accepted",
                    "document_workflow",
                    "workflow-a",
                    "transition",
                    transition.clone()
                )
            )
            .await
            .status,
            "applied"
        );
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "workflow-transition-repeat",
                    "document_workflow",
                    "workflow-a",
                    "transition",
                    transition
                )
            )
            .await
            .status,
            "applied"
        );
        let row:(String,String,String)=sqlx::query_as("SELECT current_stage_id,status,updated_at FROM document_workflows WHERE id='workflow-a'").fetch_one(&state.db).await.unwrap();
        assert_eq!(
            row,
            (
                "done".into(),
                "completed".into(),
                "2026-10-03T00:00:00+00:00".into()
            )
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM document_workflow_events WHERE id='legacy:transition'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            1
        );
        let mut update = workflow_template_payload("template-a");
        update["stages"] = json!([{"id":"fresh","name":"Fresh","sort_order":0},{"id":"done","name":"Done","sort_order":1}]);
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "workflow-history-stage-removal",
                    "workflow_template",
                    "template-a",
                    "update",
                    update
                )
            )
            .await
            .status,
            "failed"
        );
    }
    #[tokio::test]
    async fn replay_workflow_core_standalone_history_preserves_raw_note_and_rejects_invalid_stage_ids(
    ) {
        let state = workflow_replay_test_state().await;
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-note-enable",
                    "document_workflow",
                    "workflow-a",
                    "create",
                    workflow_import_payload("workflow-a", "target-a")
                )
            )
            .await
            .status,
            "applied"
        );
        let event = json!({"id":"legacy:note","document_workflow_id":"workflow-a","document_id":"target-a","space_id":"space-a","from_stage_id":"review","to_stage_id":"review","action":"delegate","note":"  Preserve raw historical note  ","created_by":"owner-a","created_at":"2026-10-04T00:00:00Z"});
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-raw-note",
                    "document_workflow_event",
                    "legacy:note",
                    "delegate",
                    event.clone()
                )
            )
            .await
            .status,
            "applied"
        );
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-raw-note-repeat",
                    "document_workflow_event",
                    "legacy:note",
                    "delegate",
                    event.clone()
                )
            )
            .await
            .status,
            "applied"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT note FROM document_workflow_events WHERE id='legacy:note'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            "  Preserve raw historical note  "
        );
        let mut malformed = event;
        malformed["id"] = json!("bad-stage-event");
        malformed["to_stage_id"] = json!(" review ");
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-raw-stage-invalid",
                    "document_workflow_event",
                    "bad-stage-event",
                    "delegate",
                    malformed
                )
            )
            .await
            .status,
            "failed"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM document_workflow_events WHERE id='bad-stage-event'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn replay_workflow_core_requires_stable_nested_ids_and_retains_rejected_imports() {
        let state = workflow_replay_test_state().await;
        for (index, missing) in [
            None,
            Some(Value::Null),
            Some(json!("")),
            Some(json!(" stage ")),
        ]
        .into_iter()
        .enumerate()
        {
            let mut template = workflow_template_payload("new-template");
            match missing {
                None => {
                    template["stages"][0].as_object_mut().unwrap().remove("id");
                }
                Some(value) => template["stages"][0]["id"] = value,
            }
            assert_eq!(
                run_tag_operation(
                    &state,
                    "owner-a",
                    tag_replay_operation(
                        &format!("workflow-no-stage-id-{index}"),
                        "workflow_template",
                        "new-template",
                        "create",
                        template
                    )
                )
                .await
                .status,
                "failed"
            );
        }
        for (index, approver) in [
            json!({"approver_type":"role","approver_id":"owner"}),
            json!({"id":null,"approver_type":"role","approver_id":"owner"}),
            json!({"id":"","approver_type":"role","approver_id":"owner"}),
        ]
        .into_iter()
        .enumerate()
        {
            let mut template = workflow_template_payload("new-template");
            template["approvers"] = json!([approver]);
            assert_eq!(
                run_tag_operation(
                    &state,
                    "owner-a",
                    tag_replay_operation(
                        &format!("workflow-no-approver-id-{index}"),
                        "workflow_template",
                        "new-template",
                        "create",
                        template
                    )
                )
                .await
                .status,
                "failed"
            );
        }
        let mut duplicate = workflow_template_payload("new-template");
        duplicate["stages"][1]["id"] = json!("review");
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "workflow-duplicate-stage-id",
                    "workflow_template",
                    "new-template",
                    "create",
                    duplicate
                )
            )
            .await
            .status,
            "failed"
        );
        let mut missing_event = workflow_import_payload("workflow-a", "target-a");
        missing_event["event"].as_object_mut().unwrap().remove("id");
        for id in ["workflow-no-event-id", "workflow-no-event-id-again"] {
            assert_eq!(
                run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        id,
                        "document_workflow",
                        "workflow-a",
                        "create",
                        missing_event.clone()
                    )
                )
                .await
                .status,
                "failed"
            );
        }
        let raw: String = sqlx::query_scalar(
            "SELECT payload FROM sync_replay_operations WHERE operation_id='workflow-no-event-id'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(), missing_event);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM workflow_templates WHERE id='new-template'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_workflows")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_workflow_events")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn replay_workflow_core_imports_suppress_rest_side_effects_but_rest_keeps_notifications_and_audit(
    ) {
        let state = workflow_replay_test_state().await;
        sqlx::query("UPDATE workflow_templates SET approvers='[{\"id\":\"notify-owner\",\"approver_type\":\"user\",\"approver_id\":\"owner-a\",\"stage_id\":\"review\"}]' WHERE id='template-a'").execute(&state.db).await.unwrap();
        let before: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM notifications),(SELECT COUNT(*) FROM audit_log)",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        let op = tag_replay_operation(
            "workflow-no-notify",
            "document_workflow",
            "workflow-a",
            "create",
            workflow_import_payload("workflow-a", "target-a"),
        );
        assert_eq!(
            run_tag_operation(&state, "editor-a", op.clone())
                .await
                .status,
            "applied"
        );
        assert_eq!(
            run_tag_operation(&state, "editor-a", op).await.status,
            "skipped"
        );
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-no-notify-new-id",
                    "document_workflow",
                    "workflow-a",
                    "update",
                    workflow_import_payload("workflow-a", "target-a")
                )
            )
            .await
            .status,
            "applied"
        );
        let event = json!({"id":"no-notify-event","document_workflow_id":"workflow-a","document_id":"target-a","space_id":"space-a","to_stage_id":"review","action":"delegate","note":"Historical handoff","created_by":"owner-a","created_at":"2026-10-03T00:00:00Z"});
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-no-handoff-notify",
                    "document_workflow_event",
                    "no-notify-event",
                    "delegate",
                    event
                )
            )
            .await
            .status,
            "applied"
        );
        let after: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM notifications),(SELECT COUNT(*) FROM audit_log)",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(before, after);
        crate::spaces::workflows::mutate_enable_workflow(
            &state,
            &{
                let mut auth = crate::spaces::properties::tests::test_auth("editor-a");
                auth.claims.sid = "workflow-session-editor-a".into();
                auth
            },
            "space-a".into(),
            "target-a".into(),
            crate::spaces::workflows::EnableDocumentWorkflowRequest {
                template_id: "template-a".into(),
                stage_id: Some("review".into()),
                source_id: None,
                event_source_id: None,
                event_created_by: None,
                event_created_at: None,
                created_at: None,
                updated_at: None,
            },
            None,
        )
        .await
        .unwrap();
        let rest: (i64, i64) = sqlx::query_as(
            "SELECT (SELECT COUNT(*) FROM notifications),(SELECT COUNT(*) FROM audit_log)",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert!(rest.0 > after.0);
        assert!(rest.1 > after.1);
    }

    #[tokio::test]
    async fn replay_workflow_core_missing_template_cannot_cascade_corrupt_history() {
        let state = workflow_replay_test_state().await;
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-orphan-enable",
                    "document_workflow",
                    "workflow-a",
                    "create",
                    workflow_import_payload("workflow-a", "target-a")
                )
            )
            .await
            .status,
            "applied"
        );
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE document_workflows SET workflow_template_id='missing-template' WHERE id='workflow-a'").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE document_workflow_events SET document_id='target-b' WHERE document_workflow_id='workflow-a'").execute(&state.db).await.unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-orphan-disable",
                    "document_workflow",
                    "target-a",
                    "delete",
                    json!({"document_id":"target-a","space_id":"space-a"})
                )
            )
            .await
            .status,
            "failed"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_workflows")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_workflow_events")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
    }
    #[tokio::test]
    async fn replay_workflow_core_preserves_raw_legacy_top_level_bindings() {
        let state = workflow_session_test_state().await;
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "workflow-raw-template",
                    "workflow_template",
                    " legacy-template ",
                    "create",
                    workflow_template_payload(" legacy-template ")
                )
            )
            .await
            .status,
            "applied"
        );
        let mut payload = workflow_import_payload(" legacy-workflow ", "target-a");
        payload["workflow_template_id"] = json!(" legacy-template ");
        payload["event"]["id"] = json!(" legacy-event ");
        for id in ["workflow-raw-binding", "workflow-raw-binding-again"] {
            assert_eq!(
                run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        id,
                        "document_workflow",
                        " legacy-workflow ",
                        "create",
                        payload.clone()
                    )
                )
                .await
                .status,
                "applied"
            );
        }
        let row: (String, String) = sqlx::query_as(
            "SELECT id,workflow_template_id FROM document_workflows WHERE document_id='target-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            row,
            (" legacy-workflow ".into(), " legacy-template ".into())
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM document_workflow_events WHERE id=' legacy-event '"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            1
        );
        payload["current_stage_id"] = json!(" review ");
        payload.as_object_mut().unwrap().remove("event");
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-raw-stage-not-remapped",
                    "document_workflow",
                    " legacy-workflow ",
                    "update",
                    payload
                )
            )
            .await
            .status,
            "failed"
        );
    }
    #[tokio::test]
    async fn replay_workflow_core_event_identity_collision_and_delete_cascade_are_scoped() {
        let state = workflow_replay_test_state().await;
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-events-enable",
                    "document_workflow",
                    "workflow-a",
                    "create",
                    workflow_import_payload("workflow-a", "target-a")
                )
            )
            .await
            .status,
            "applied"
        );
        let event = json!({"id":"workflow-a:enable","document_workflow_id":"workflow-a","document_id":"target-a","space_id":"space-a","to_stage_id":"done","action":"enable","created_by":"owner-a","created_at":"2026-10-01T00:00:00Z"});
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-event-collision",
                    "document_workflow_event",
                    "workflow-a:enable",
                    "enable",
                    event
                )
            )
            .await
            .status,
            "failed"
        );
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-template-editor-delete",
                    "workflow_template",
                    "template-a",
                    "delete",
                    json!({"space_id":"space-a"})
                )
            )
            .await
            .status,
            "failed"
        );
        sqlx::query("UPDATE document_workflow_events SET document_id='target-b' WHERE document_workflow_id='workflow-a'").execute(&state.db).await.unwrap();
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "workflow-template-corrupt-cascade",
                    "workflow_template",
                    "template-a",
                    "delete",
                    json!({"space_id":"space-a"})
                )
            )
            .await
            .status,
            "failed"
        );
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "workflow-corrupt-disable",
                    "document_workflow",
                    "target-a",
                    "delete",
                    json!({"space_id":"space-a","document_id":"target-a","id":"workflow-a"})
                )
            )
            .await
            .status,
            "failed"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_workflow_events")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
    }

    async fn folder_replay_test_state() -> AppState {
        let state = crate::spaces::properties::tests::property_test_state().await;
        for (id, space, parent) in [
            ("folder-root", "space-a", None),
            ("folder-child", "space-a", Some("folder-root")),
            ("folder-leaf", "space-a", Some("folder-child")),
            ("folder-target", "space-a", None),
            ("folder-foreign", "space-b", None),
        ] {
            sqlx::query("INSERT INTO folders(id,space_id,parent_id,name,sort_order,created_at,updated_at) VALUES ($1,$2,$3,$1,9,'2026-10-01T00:00:00Z','2026-10-01T00:00:00Z')").bind(id).bind(space).bind(parent).execute(&state.db).await.unwrap();
        }
        sqlx::query("UPDATE documents SET folder_id='folder-child' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        state
    }
    async fn folder_replay_domain(state: &AppState) -> Vec<(String, Option<String>)> {
        crate::spaces::documents::tests::move_database_snapshot(state)
            .await
            .into_iter()
            .filter(|(table, _)| !table.starts_with("sync_replay_"))
            .collect()
    }
    fn folder_payload(id: &str, parent: Option<&str>) -> Value {
        json!({"id":id,"space_id":"space-a","name":" Raw name ","parent_id":parent,"icon":null,"sort_order":27,"created_at":"2026-10-02T00:00:00Z","updated_at":"2026-10-03T00:00:00Z"})
    }
    #[tokio::test]
    async fn replay_folder_core_preserves_legacy_ids_times_sort_and_exact_import_retry() {
        let state = folder_replay_test_state().await;
        let payload = folder_payload(" legacy:folder ", Some("folder-target"));
        let before_audit: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
            .fetch_one(&state.db)
            .await
            .unwrap();
        for (operation, status) in [
            ("folder-import-first", "applied"),
            ("folder-import-repeat", "skipped"),
        ] {
            let result = run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    operation,
                    "folder",
                    " legacy:folder ",
                    "create",
                    payload.clone(),
                ),
            )
            .await;
            assert_eq!(result.status, status, "{}", result.message);
        }
        let row:(String,String,i32,String,String)=sqlx::query_as("SELECT id,name,sort_order,created_at,updated_at FROM folders WHERE id=' legacy:folder '").fetch_one(&state.db).await.unwrap();
        assert_eq!(
            row,
            (
                " legacy:folder ".into(),
                " Raw name ".into(),
                27,
                "2026-10-02T00:00:00+00:00".into(),
                "2026-10-03T00:00:00+00:00".into()
            )
        );
        let before = folder_replay_domain(&state).await;
        let mut changed = payload.clone();
        changed["name"] = json!("Changed");
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "folder-import-conflict",
                    "folder",
                    " legacy:folder ",
                    "create",
                    changed
                )
            )
            .await
            .status,
            "failed"
        );
        assert_eq!(folder_replay_domain(&state).await, before);
        let mut update = payload;
        update["parent_id"] = Value::Null;
        update["name"] = json!("Renamed");
        update["sort_order"] = json!(31);
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "folder-import-update",
                    "folder",
                    " legacy:folder ",
                    "update",
                    update
                )
            )
            .await
            .status,
            "applied"
        );
        let row: (Option<String>, Option<String>, i32) = sqlx::query_as(
            "SELECT parent_id,icon,sort_order FROM folders WHERE id=' legacy:folder '",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(row, (None, None, 31));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_log")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            before_audit
        );
    }
    #[tokio::test]
    async fn replay_folder_core_rejects_forged_scope_types_identity_and_hierarchy_without_writes() {
        for (i, (field, value)) in [
            ("id", json!("other")),
            ("space_id", json!("space-b")),
            ("parent_id", json!("folder-foreign")),
            ("parent_id", json!(false)),
            ("icon", json!(7)),
            ("sort_order", json!(3.5)),
            ("created_at", json!(false)),
            ("updated_at", json!("bad")),
            ("name", json!("")),
        ]
        .into_iter()
        .enumerate()
        {
            let state = folder_replay_test_state().await;
            let before = folder_replay_domain(&state).await;
            let mut payload = folder_payload("new-folder", None);
            payload[field] = value;
            assert_eq!(
                run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        &format!("folder-invalid-{i}"),
                        "folder",
                        "new-folder",
                        "create",
                        payload
                    )
                )
                .await
                .status,
                "failed"
            );
            assert_eq!(folder_replay_domain(&state).await, before);
        }
        let state = folder_replay_test_state().await;
        for (action, parent) in [("update", Some("folder-leaf"))] {
            let id = if action == "create" {
                "new-folder"
            } else {
                "folder-root"
            };
            let before = folder_replay_domain(&state).await;
            assert_eq!(
                run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        &format!("folder-depth-{action}"),
                        "folder",
                        id,
                        action,
                        folder_payload(id, parent)
                    )
                )
                .await
                .status,
                "failed"
            );
            assert_eq!(folder_replay_domain(&state).await, before);
        }
        for id in ["folder-foreign", "missing"] {
            let before = folder_replay_domain(&state).await;
            for action in ["update", "delete"] {
                let result = run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        &format!("folder-scope-{id}-{action}"),
                        "folder",
                        id,
                        action,
                        folder_payload(id, None),
                    ),
                )
                .await;
                assert_eq!(result.status, "failed");
                assert_eq!(result.message, "Folder/page target unavailable");
            }
            assert_eq!(folder_replay_domain(&state).await, before);
        }
    }
    #[tokio::test]
    async fn replay_folder_core_reparent_and_delete_check_actual_private_locked_lifecycle_pages() {
        for bad in [
            "UPDATE documents SET published_at=NULL WHERE id='target-a'",
            "UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'",
            "UPDATE documents SET archived_at='old' WHERE id='target-a'",
            "UPDATE documents SET deleted_at='old' WHERE id='target-a'",
            "UPDATE users SET is_active=0 WHERE id='editor-a'",
        ] {
            for action in ["update", "delete"] {
                let state = folder_replay_test_state().await;
                sqlx::query(bad).execute(&state.db).await.unwrap();
                let before = folder_replay_domain(&state).await;
                let result = run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        &format!("folder-page-{action}"),
                        "folder",
                        "folder-child",
                        action,
                        folder_payload("folder-child", None),
                    ),
                )
                .await;
                assert_eq!(result.status, "failed");
                assert_eq!(folder_replay_domain(&state).await, before);
            }
        }
        let state = folder_replay_test_state().await;
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES ('folder-draft','target-a','space-a','editor-a','Private','Keep draft','old')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "folder-private-owner-update",
                    "folder",
                    "folder-child",
                    "update",
                    folder_payload("folder-child", None)
                )
            )
            .await
            .status,
            "applied"
        );
        let doc_before: (String, String) =
            sqlx::query_as("SELECT created_by,content_path FROM documents WHERE id='target-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let op = tag_replay_operation(
            "folder-private-owner-delete",
            "folder",
            "folder-child",
            "delete",
            json!({"id":"folder-child","space_id":"space-a"}),
        );
        assert_eq!(
            run_tag_operation(&state, "editor-a", op.clone())
                .await
                .status,
            "applied"
        );
        let receipt = run_tag_operation(&state, "editor-a", op).await;
        assert_eq!(receipt.status, "skipped");
        assert_eq!(
            sqlx::query_as::<_, (String, String)>(
                "SELECT created_by,content_path FROM documents WHERE id='target-a'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            doc_before
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM document_drafts WHERE id='folder-draft'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT folder_id FROM documents WHERE id='target-a'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            None
        );
        let unknown = run_tag_operation(
            &state,
            "editor-a",
            tag_replay_operation(
                "folder-delete-new-unknown",
                "folder",
                "folder-child",
                "delete",
                json!({"id":"folder-child","space_id":"space-a"}),
            ),
        )
        .await;
        assert_eq!(unknown.status, "failed");
        assert_eq!(unknown.message, "Folder/page target unavailable");
    }
    #[tokio::test]
    async fn replay_folder_core_last_write_rechecks_and_rolls_back_metadata_cascade_and_ledger() {
        for (action,trigger) in [
            ("create","CREATE TRIGGER folder_late AFTER INSERT ON folders BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END"),
            ("update","CREATE TRIGGER folder_late AFTER UPDATE OF name ON folders BEGIN UPDATE space_members SET role='viewer' WHERE user_id='editor-a'; END"),
            ("update","CREATE TRIGGER folder_late AFTER UPDATE OF name ON folders BEGIN UPDATE folders SET parent_id='folder-target' WHERE id=NEW.id; END"),
            ("delete","CREATE TRIGGER folder_late AFTER DELETE ON folders BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END"),
            ("delete","CREATE TRIGGER folder_late AFTER UPDATE OF folder_id ON documents WHEN NEW.folder_id IS NULL BEGIN UPDATE documents SET folder_id='folder-target' WHERE id=NEW.id; END")
        ] {
            let state=folder_replay_test_state().await;sqlx::query(trigger).execute(&state.db).await.unwrap();let before=folder_replay_domain(&state).await;
            let id=if action=="create" { "new-folder" } else { "folder-child" };
            let result=run_tag_operation(&state,"editor-a",tag_replay_operation("folder-late","folder",id,action,folder_payload(id,None))).await;
            assert_eq!(result.status,"failed");assert_eq!(folder_replay_domain(&state).await,before);
            let status:String=sqlx::query_scalar("SELECT status FROM sync_replay_operations WHERE operation_id='folder-late'").fetch_one(&state.db).await.unwrap();assert_eq!(status,"failed");
        }
    }
    #[tokio::test]
    async fn replay_folder_core_import_retry_rechecks_metadata_after_tuple_lock() {
        for change in [
            "name='Changed'",
            "icon='Changed'",
            "parent_id='folder-root'",
            "sort_order=28",
            "created_at='2026-10-04T00:00:00Z'",
            "updated_at='2026-10-04T00:00:00Z'",
        ] {
            let state = folder_replay_test_state().await;
            let payload = folder_payload("folder-retry", None);
            assert_eq!(
                run_tag_operation(
                    &state,
                    "editor-a",
                    tag_replay_operation(
                        "folder-retry-first",
                        "folder",
                        "folder-retry",
                        "create",
                        payload.clone()
                    )
                )
                .await
                .status,
                "applied"
            );
            sqlx::query(&format!("CREATE TRIGGER folder_retry_wait AFTER UPDATE OF id ON folders WHEN NEW.id='folder-retry' BEGIN UPDATE folders SET {change} WHERE id=NEW.id; END")).execute(&state.db).await.unwrap();
            let before = folder_replay_domain(&state).await;
            let result = run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "folder-retry-after-wait",
                    "folder",
                    "folder-retry",
                    "create",
                    payload,
                ),
            )
            .await;
            assert_eq!(result.status, "failed", "{change}");
            assert_eq!(folder_replay_domain(&state).await, before);
        }
    }

    #[tokio::test]
    async fn replay_folder_core_waited_operation_rechecks_current_graph_and_actor() {
        for action in ["create", "update", "delete"] {
            for sql in [
                "UPDATE space_members SET role='viewer' WHERE user_id='editor-a'",
                "UPDATE folders SET space_id='space-b' WHERE id='folder-target'",
                "UPDATE folders SET parent_id='folder-child' WHERE id='folder-foreign'",
            ] {
                if action == "delete" && sql.contains("id='folder-target'")
                    || action == "create" && sql.contains("id='folder-foreign'")
                {
                    continue;
                }
                let state = folder_replay_test_state().await;
                let mut blocker = state.db.begin().await.unwrap();
                crate::spaces::mutation_guards::lock_spaces_for_mutation(
                    &mut blocker,
                    &["space-a"],
                )
                .await
                .unwrap();
                let worker = state.clone();
                let id = if action == "create" {
                    "new-folder"
                } else {
                    "folder-child"
                };
                let op = tag_replay_operation(
                    "folder-wait",
                    "folder",
                    id,
                    action,
                    folder_payload(id, Some("folder-target")),
                );
                let task =
                    tokio::spawn(async move { run_tag_operation(&worker, "editor-a", op).await });
                tokio::task::yield_now().await;
                sqlx::query(sql).execute(&mut *blocker).await.unwrap();
                blocker.commit().await.unwrap();
                let before = folder_replay_domain(&state).await;
                assert_eq!(task.await.unwrap().status, "failed", "{action}: {sql}");
                assert_eq!(folder_replay_domain(&state).await, before);
            }
        }
    }

    // Tag mutation tests exercise the actual SQLite migrations and a one-connection
    // pool, catching both schema drift and pooled helper calls under transactions.
    async fn tag_replay_test_state() -> AppState {
        let state = crate::spaces::properties::tests::property_test_state().await;
        sqlx::query("INSERT INTO tags(id,space_id,name,color,created_at) VALUES ('tag-a','space-a','A','#111111','2026-10-01T00:00:00Z'),('tag-b','space-b','B','#222222','2026-10-01T00:00:00Z')").execute(&state.db).await.unwrap();
        state
    }
    fn tag_replay_operation(
        id: &str,
        kind: &str,
        entity: &str,
        action: &str,
        payload: Value,
    ) -> SyncReplayOperation {
        replay_operation(id, "2026-10-07T12:00:00Z", kind, entity, action, payload)
    }
    async fn run_tag_operation(
        state: &AppState,
        actor: &str,
        op: SyncReplayOperation,
    ) -> super::SyncReplayOperationResult {
        let manifest = single_operation_manifest(op.clone());
        let mut auth = crate::spaces::properties::tests::test_auth(actor);
        if matches!(
            op.entity_type.as_str(),
            "workflow_template" | "document_workflow" | "document_workflow_event"
        ) {
            auth.claims.sid = format!("workflow-session-{actor}");
        }
        super::execute_replay_operation(state, &auth, &manifest, &op).await
    }

    #[tokio::test]
    async fn replay_metadata_uses_current_key_ceiling_and_retains_failed_payloads() {
        let state = tag_replay_test_state().await;
        let auth =
            crate::spaces::properties::tests::metadata_key(&state, "owner-a", r#"["write"]"#).await;
        for (id, kind, target, payload) in [
            (
                "key-tag-write",
                "tag",
                "key-tag",
                json!({"id":"key-tag","space_id":"space-a","name":"Key tag","color":"#abc123"}),
            ),
            (
                "key-property-write",
                "property_schema",
                "key-prop",
                json!({"id":"key-prop","space_id":"space-a","name":"Key property","property_type":"text","sort_order":1}),
            ),
        ] {
            let op = tag_replay_operation(id, kind, target, "create", payload);
            assert_eq!(
                super::execute_replay_operation(
                    &state,
                    &auth,
                    &single_operation_manifest(op.clone()),
                    &op
                )
                .await
                .status,
                "applied"
            );
        }
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]'")
            .execute(&state.db)
            .await
            .unwrap();
        for (id, kind, target, payload) in [
            (
                "key-tag-denied",
                "tag",
                "denied-tag",
                json!({"id":"denied-tag","space_id":"space-a","name":"Denied tag","color":"#abc123"}),
            ),
            (
                "key-property-denied",
                "property_schema",
                "denied-prop",
                json!({"id":"denied-prop","space_id":"space-a","name":"Denied property","property_type":"text","sort_order":1}),
            ),
        ] {
            let op = tag_replay_operation(id, kind, target, "create", payload);
            let result = super::execute_replay_operation(
                &state,
                &auth,
                &single_operation_manifest(op.clone()),
                &op,
            )
            .await;
            assert_eq!(result.status, "failed");
            let stored: String = sqlx::query_scalar(
                "SELECT payload FROM sync_replay_operations WHERE operation_id=$1",
            )
            .bind(id)
            .fetch_one(&state.db)
            .await
            .unwrap();
            assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(), op.payload);
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE id='denied-tag'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM properties_schema WHERE id='denied-prop'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn replay_tags_reject_forged_scope_relationship_and_malformed_fields_without_writes() {
        let state = tag_replay_test_state().await;
        sqlx::query("INSERT INTO doc_tags(document_id,tag_id) VALUES ('target-b','tag-b')")
            .execute(&state.db)
            .await
            .unwrap();
        let mut cases = vec![
            (
                "document_tag",
                "target-b:tag-b",
                "remove",
                json!({"document_id":"target-b","tag_id":"tag-b","space_id":"space-a"}),
            ),
            (
                "document_tag",
                "target-a:tag-b",
                "add",
                json!({"document_id":"target-a","tag_id":"tag-b","space_id":"space-a"}),
            ),
            (
                "document_tag",
                "target-a:tag-b",
                "remove",
                json!({"document_id":"target-a","tag_id":"tag-b","space_id":"space-a"}),
            ),
            (
                "document_tag",
                "different:tag-a",
                "add",
                json!({"document_id":"target-a","tag_id":"tag-a"}),
            ),
            (
                "tag",
                "tag-b",
                "create",
                json!({"id":"tag-b","space_id":"space-a","name":"B"}),
            ),
            (
                "tag",
                "tag-b",
                "update",
                json!({"id":"tag-b","space_id":"space-a","name":"Changed"}),
            ),
            (
                "tag",
                "tag-b",
                "delete",
                json!({"id":"tag-b","space_id":"space-a"}),
            ),
            (
                "tag",
                "new",
                "create",
                json!({"id":5,"space_id":"space-a","name":"New"}),
            ),
            (
                "tag",
                "new",
                "create",
                json!({"id":"new","space_id":"space-a","name":"New","color":false}),
            ),
            (
                "tag",
                "new",
                "create",
                json!({"id":"new","space_id":"space-a","name":"New","created_at":42}),
            ),
        ];
        for space in [Value::Null, json!(42), json!(""), json!(" ")] {
            cases.push((
                "document_tag",
                "target-a:tag-a",
                "add",
                json!({"document_id":"target-a","tag_id":"tag-a","space_id":space}),
            ));
        }
        for (index, (kind, entity, action, payload)) in cases.into_iter().enumerate() {
            let id = format!("malicious-tag-{index}");
            let result = run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(&id, kind, entity, action, payload.clone()),
            )
            .await;
            assert_eq!(result.status, "failed", "{id}");
            let saved: (String, String) = sqlx::query_as(
                "SELECT status,payload FROM sync_replay_operations WHERE operation_id=$1",
            )
            .bind(&id)
            .fetch_one(&state.db)
            .await
            .unwrap();
            assert_eq!(saved.0, "failed");
            assert_eq!(serde_json::from_str::<Value>(&saved.1).unwrap(), payload);
        }
        let tags: Vec<(String, String, String)> =
            sqlx::query_as("SELECT id,name,color FROM tags ORDER BY id")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(
            tags,
            vec![
                ("tag-a".into(), "A".into(), "#111111".into()),
                ("tag-b".into(), "B".into(), "#222222".into())
            ]
        );
        let pairs: Vec<(String, String)> =
            sqlx::query_as("SELECT document_id,tag_id FROM doc_tags")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(pairs, vec![("target-b".into(), "tag-b".into())]);
    }

    #[tokio::test]
    async fn replay_tags_preserve_import_identity_and_repeated_legitimate_operations() {
        let state = tag_replay_test_state().await;
        let create = json!({"id":"legacy-tag","space_id":"space-a","name":"Imported","color":"#abcdef","created_at":"2026-09-01T12:00:00Z"});
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation("tag-create", "tag", "legacy-tag", "create", create.clone())
            )
            .await
            .status,
            "applied"
        );
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation("tag-create-new-op", "tag", "legacy-tag", "create", create)
            )
            .await
            .status,
            "skipped"
        );
        let row: (String, String, String) =
            sqlx::query_as("SELECT id,color,created_at FROM tags WHERE id='legacy-tag'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(
            row,
            (
                "legacy-tag".into(),
                "#abcdef".into(),
                "2026-09-01T12:00:00+00:00".into()
            )
        );
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "tag-name-collision",
                    "tag",
                    "another-id",
                    "create",
                    json!({"id":"another-id","space_id":"space-a","name":"imported"})
                )
            )
            .await
            .status,
            "failed"
        );
        let relationship = json!({"document_id":"target-a","tag_id":"legacy-tag"}); // Actual old-client shape, no space hint.
        for (id, action, expected) in [
            ("tag-add", "add", "applied"),
            ("tag-add-again", "add", "skipped"),
            ("tag-remove", "remove", "applied"),
            ("tag-remove-again", "remove", "skipped"),
        ] {
            assert_eq!(
                run_tag_operation(
                    &state,
                    "owner-a",
                    tag_replay_operation(
                        id,
                        "document_tag",
                        "target-a:legacy-tag",
                        action,
                        relationship.clone()
                    )
                )
                .await
                .status,
                expected
            );
        }
        for (id, expected) in [("tag-delete", "applied"), ("tag-delete-again", "failed")] {
            assert_eq!(
                run_tag_operation(
                    &state,
                    "owner-a",
                    tag_replay_operation(
                        id,
                        "tag",
                        "legacy-tag",
                        "delete",
                        json!({"id":"legacy-tag","space_id":"space-a"})
                    )
                )
                .await
                .status,
                expected
            );
        }
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "tag-delete",
                    "tag",
                    "legacy-tag",
                    "delete",
                    json!({"id":"legacy-tag","space_id":"space-a"})
                )
            )
            .await
            .status,
            "skipped"
        );
    }

    #[tokio::test]
    async fn replay_tags_enforce_private_draft_participation_locks_lifecycle_and_current_actor() {
        let state = tag_replay_test_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES ('draft','target-a','space-a','editor-a','Private','old')").execute(&state.db).await.unwrap();
        let payload = json!({"document_id":"target-a","tag_id":"tag-a","space_id":"space-a"});
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "owner-private",
                    "document_tag",
                    "target-a:tag-a",
                    "add",
                    payload.clone()
                )
            )
            .await
            .status,
            "failed"
        );
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ('draft','owner-a','viewer','editor-a')").execute(&state.db).await.unwrap();
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "shared-viewer",
                    "document_tag",
                    "target-a:tag-a",
                    "add",
                    payload.clone()
                )
            )
            .await
            .status,
            "failed"
        );
        sqlx::query("UPDATE document_draft_access SET role='editor'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "shared-editor",
                    "document_tag",
                    "target-a:tag-a",
                    "add",
                    payload.clone()
                )
            )
            .await
            .status,
            "applied"
        );
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "owned-private",
                    "document_tag",
                    "target-a:tag-a",
                    "remove",
                    payload.clone()
                )
            )
            .await
            .status,
            "applied"
        );
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES ('space-a','editor-a','owner-a')").execute(&state.db).await.unwrap();
        assert_eq!(
            run_tag_operation(
                &state,
                "editor-a",
                tag_replay_operation(
                    "blocked-private-editor",
                    "document_tag",
                    "target-a:tag-a",
                    "add",
                    payload.clone()
                )
            )
            .await
            .status,
            "failed"
        );
        sqlx::query("DELETE FROM space_user_blocks WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        for (i,change) in ["UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'","UPDATE documents SET locked=0,archived_at='old' WHERE id='target-a'","UPDATE documents SET archived_at=NULL,deleted_at='old' WHERE id='target-a'","UPDATE documents SET deleted_at=NULL WHERE id='target-a'; UPDATE users SET is_active=0 WHERE id='editor-a'"].into_iter().enumerate() {
            sqlx::raw_sql(change).execute(&state.db).await.unwrap();
            assert_eq!(run_tag_operation(&state,"editor-a",tag_replay_operation(&format!("blocked-tag-{i}"),"document_tag","target-a:tag-a","add",payload.clone())).await.status,"failed");
        }
        let pairs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM doc_tags")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(pairs, 0);
    }

    #[tokio::test]
    async fn replay_tags_recheck_after_wait_and_roll_back_late_write_revocation_and_foreign_cascade(
    ) {
        let state = tag_replay_test_state().await;
        let mut blocker = state.db.begin().await.unwrap();
        crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut blocker, &["space-a"])
            .await
            .unwrap();
        sqlx::query("UPDATE space_members SET role='viewer' WHERE user_id='editor-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let queued_state = state.clone();
        let mut queued = tokio::spawn(async move {
            run_tag_operation(
                &queued_state,
                "editor-a",
                tag_replay_operation(
                    "waited-tag",
                    "document_tag",
                    "target-a:tag-a",
                    "add",
                    json!({"document_id":"target-a","tag_id":"tag-a","space_id":"space-a"}),
                ),
            )
            .await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut queued)
                .await
                .is_err()
        );
        blocker.commit().await.unwrap();
        assert_eq!(queued.await.unwrap().status, "failed");
        sqlx::query("CREATE TRIGGER revoke_tag_actor AFTER INSERT ON doc_tags BEGIN UPDATE users SET is_active=0 WHERE id='owner-a'; END").execute(&state.db).await.unwrap();
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "late-tag",
                    "document_tag",
                    "target-a:tag-a",
                    "add",
                    json!({"document_id":"target-a","tag_id":"tag-a","space_id":"space-a"})
                )
            )
            .await
            .status,
            "failed"
        );
        let active: i64 = sqlx::query_scalar("SELECT is_active FROM users WHERE id='owner-a'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(active, 1);
        let pairs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM doc_tags")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(pairs, 0);
        sqlx::query("DROP TRIGGER revoke_tag_actor")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO doc_tags(document_id,tag_id) VALUES ('target-b','tag-a')")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "foreign-cascade",
                    "tag",
                    "tag-a",
                    "delete",
                    json!({"id":"tag-a","space_id":"space-a"})
                )
            )
            .await
            .status,
            "failed"
        );
        let pairs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM doc_tags")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(pairs, 1);
    }

    #[tokio::test]
    async fn replay_ledger_acknowledgments_bind_actor_provider_identity_action_and_json_payload() {
        let state = tag_replay_test_state().await;
        let original = tag_replay_operation(
            "bound-tag-op",
            "tag",
            "ledger-tag",
            "create",
            json!({"id":"ledger-tag","space_id":"space-a","name":"Ledger"}),
        );
        let manifest = single_operation_manifest(original.clone());
        let auth = crate::spaces::properties::tests::test_auth("owner-a");
        assert_eq!(
            super::execute_replay_operation(&state, &auth, &manifest, &original)
                .await
                .status,
            "applied"
        );
        // Intentionally different serialized key order/whitespace is the same JSON intent.
        sqlx::query(
            "UPDATE sync_replay_operations SET payload=$1 WHERE operation_id='bound-tag-op'",
        )
        .bind("{ \"space_id\":\"space-a\", \"name\":\"Ledger\", \"id\":\"ledger-tag\" }")
        .execute(&state.db)
        .await
        .unwrap();
        assert_eq!(
            super::execute_replay_operation(&state, &auth, &manifest, &original)
                .await
                .status,
            "skipped"
        );
        let before:(String,String,String)=sqlx::query_as("SELECT status,attempted_by,payload FROM sync_replay_operations WHERE operation_id='bound-tag-op'").fetch_one(&state.db).await.unwrap();
        for mismatch in [
            "actor",
            "provider",
            "entity_type",
            "entity_id",
            "action",
            "payload",
        ] {
            let mut op = original.clone();
            let mut request = manifest.clone();
            let mut actor = crate::spaces::properties::tests::test_auth("owner-a");
            match mismatch {
                "actor" => actor.claims.sub = "editor-a".into(),
                "provider" => request.provider = "different".into(),
                "entity_type" => op.entity_type = "property_schema".into(),
                "entity_id" => op.entity_id = "other".into(),
                "action" => op.action = "update".into(),
                _ => op.payload["name"] = json!("Changed"),
            }
            let response = super::execute_replay_operation(&state, &actor, &request, &op).await;
            assert_eq!(response.status, "failed", "{mismatch}");
            assert_eq!(
                response.message,
                "Replay operation identity collision; submit a corrected operation with a new id"
            );
            let after:(String,String,String)=sqlx::query_as("SELECT status,attempted_by,payload FROM sync_replay_operations WHERE operation_id='bound-tag-op'").fetch_one(&state.db).await.unwrap();
            assert_eq!(after, before);
        }
        for corrupt in [Some("{broken"), None] {
            sqlx::query(
                "UPDATE sync_replay_operations SET payload=$1 WHERE operation_id='bound-tag-op'",
            )
            .bind(corrupt)
            .execute(&state.db)
            .await
            .unwrap();
            let response =
                super::execute_replay_operation(&state, &auth, &manifest, &original).await;
            assert_eq!(response.status, "failed");
            assert_eq!(
                response.message,
                "Replay operation identity collision; submit a corrected operation with a new id"
            );
        }
        let name: String = sqlx::query_scalar("SELECT name FROM tags WHERE id='ledger-tag'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(name, "Ledger");
    }

    #[tokio::test]
    async fn replay_tags_same_id_skip_requires_matching_requested_metadata_and_preserves_legacy_names(
    ) {
        let state = tag_replay_test_state().await;
        for (index, change) in [
            ("name", json!("Changed")),
            ("color", json!("#999999")),
            ("created_at", json!("2026-10-02T00:00:00Z")),
        ]
        .into_iter()
        .enumerate()
        {
            let mut payload = json!({"id":"tag-a","space_id":"space-a","name":"A","color":"#111111","created_at":"2026-10-01T00:00:00Z"});
            payload[change.0] = change.1;
            assert_eq!(
                run_tag_operation(
                    &state,
                    "owner-a",
                    tag_replay_operation(
                        &format!("metadata-collision-{index}"),
                        "tag",
                        "tag-a",
                        "create",
                        payload
                    )
                )
                .await
                .status,
                "failed"
            );
        }
        // No supplied timestamp must not compare a generated current time to the old record.
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "same-id-no-time",
                    "tag",
                    "tag-a",
                    "create",
                    json!({"id":"tag-a","space_id":"space-a","name":"A","color":"#111111"})
                )
            )
            .await
            .status,
            "skipped"
        );
        assert_eq!(run_tag_operation(&state,"owner-a",tag_replay_operation("same-id-time","tag","tag-a","create",json!({"id":"tag-a","space_id":"space-a","name":"A","color":"#111111","created_at":"2026-10-01T00:00:00+00:00"}))).await.status,"skipped");
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "raw-name-create",
                    "tag",
                    "raw-name",
                    "create",
                    json!({"id":"raw-name","space_id":"space-a","name":" Legacy "})
                )
            )
            .await
            .status,
            "applied"
        );
        let name: String = sqlx::query_scalar("SELECT name FROM tags WHERE id='raw-name'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(name, " Legacy ");
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "raw-name-update",
                    "tag",
                    "raw-name",
                    "update",
                    json!({"id":"raw-name","space_id":"space-a","name":" Updated legacy "})
                )
            )
            .await
            .status,
            "applied"
        );
        let name: String = sqlx::query_scalar("SELECT name FROM tags WHERE id='raw-name'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(name, " Updated legacy ");
        let row: (String, String, String) =
            sqlx::query_as("SELECT name,color,created_at FROM tags WHERE id='tag-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(
            row,
            ("A".into(), "#111111".into(), "2026-10-01T00:00:00Z".into())
        );
    }

    #[tokio::test]
    async fn replay_tags_orphan_consumers_block_delete_and_late_delete_revocation_rolls_back_cascade(
    ) {
        let state = tag_replay_test_state().await;
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO doc_tags(document_id,tag_id) VALUES ('missing-page','tag-a')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "orphan-tag-delete",
                    "tag",
                    "tag-a",
                    "delete",
                    json!({"id":"tag-a","space_id":"space-a"})
                )
            )
            .await
            .status,
            "failed"
        );
        sqlx::query("DELETE FROM doc_tags WHERE document_id='missing-page'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO doc_tags(document_id,tag_id) VALUES ('target-a','tag-a')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER late_tag_delete AFTER DELETE ON tags BEGIN UPDATE users SET is_active=0 WHERE id='owner-a'; END").execute(&state.db).await.unwrap();
        assert_eq!(
            run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    "late-tag-delete",
                    "tag",
                    "tag-a",
                    "delete",
                    json!({"id":"tag-a","space_id":"space-a"})
                )
            )
            .await
            .status,
            "failed"
        );
        let tag: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE id='tag-a'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(tag, 1);
        let pair: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM doc_tags WHERE document_id='target-a' AND tag_id='tag-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(pair, 1);
        let active: i64 = sqlx::query_scalar("SELECT is_active FROM users WHERE id='owner-a'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(active, 1);
    }

    #[tokio::test]
    async fn replay_tag_core_waits_then_rechecks_page_scope_and_current_session() {
        for revoke_session in [false, true] {
            let mut state = tag_replay_test_state().await;
            let path = std::env::temp_dir().join(format!(
                "starlane-tag-guard-wait-{}.sqlite",
                uuid::Uuid::new_v4()
            ));
            // Use an actual migrated on-disk database with two connections. Both
            // connections wait on the write guard rather than failing immediately.
            state.db = AnyPoolOptions::new()
                .max_connections(2)
                .after_connect(|conn, _| {
                    Box::pin(async move {
                        sqlx::query("PRAGMA busy_timeout = 5000")
                            .execute(conn)
                            .await?;
                        Ok(())
                    })
                })
                .connect(&format!("sqlite://{}?mode=rwc", path.display()))
                .await
                .unwrap();
            sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
            assert!(path.exists(), "SQLite file fixture was not created");
            for query in [
            "INSERT INTO users(id,email,display_name,role) VALUES ('owner-a','owner@example.test','Owner','member')",
            "INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('metadata-owner-a','owner-a','metadata-test','2999-01-01T00:00:00Z')",
            "INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES ('space-a','A','space-a','private','owner-a'),('space-b','B','space-b','private','owner-a')",
            "INSERT INTO space_members(id,space_id,user_id,role) VALUES ('member-a','space-a','owner-a','owner')",
            "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES ('target-a','space-a','A','','owner-a','owner-a','old')",
            "INSERT INTO tags(id,space_id,name) VALUES ('tag-a','space-a','A')",
        ] { sqlx::query(query).execute(&state.db).await.unwrap(); }
            let mut holder = state.db.begin().await.unwrap();
            crate::spaces::mutation_guards::lock_spaces_for_mutation(
                &mut holder,
                &["space-a", "space-b"],
            )
            .await
            .unwrap();
            sqlx::query(if revoke_session {
                "DELETE FROM sessions WHERE id='metadata-owner-a'"
            } else {
                "UPDATE documents SET space_id='space-b' WHERE id='target-a'"
            })
            .execute(&mut *holder)
            .await
            .unwrap();
            let queued_state = state.clone();
            // Calling the domain core directly makes this a guard wait, rather than a
            // pending replay-ledger INSERT wait; both connections are available.
            let mut queued = tokio::spawn(async move {
                crate::spaces::tags::mutate_tag(
                    &queued_state,
                    &crate::spaces::properties::tests::test_auth("owner-a"),
                    Some("space-a"),
                    crate::spaces::tags::TagMutationOrigin::Replay,
                    crate::spaces::tags::TagMutation::Associate {
                        document_id: "target-a".into(),
                        tag_id: "tag-a".into(),
                        add: true,
                    },
                )
                .await
                .map(|_| ())
            });
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(30), &mut queued)
                    .await
                    .is_err()
            );
            holder.commit().await.unwrap();
            assert!(matches!(
                queued.await.unwrap(),
                Err(crate::auth::routes::AppError::NotFound(_)
                    | crate::auth::routes::AppError::Unauthorized(_))
            ));
            let pairs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM doc_tags")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(pairs, 0);
            state.db.close().await;
            std::fs::remove_file(path).unwrap();
        }
    }

    #[tokio::test]
    async fn replay_tag_association_conceals_absent_foreign_and_private_targets_uniformly() {
        let state = tag_replay_test_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES ('hidden','target-a','space-a','editor-a','Private','old')").execute(&state.db).await.unwrap();
        for (index, document) in ["missing-page", "target-b", "target-a"]
            .into_iter()
            .enumerate()
        {
            let payload = json!({"document_id":document,"tag_id":"tag-a"});
            let id = format!("unavailable-association-{index}");
            let result = run_tag_operation(
                &state,
                "owner-a",
                tag_replay_operation(
                    &id,
                    "document_tag",
                    &format!("{document}:tag-a"),
                    "add",
                    payload.clone(),
                ),
            )
            .await;
            assert_eq!(result.status, "failed");
            assert_eq!(result.message, "Tag/page target unavailable");
            let saved: (String, String) = sqlx::query_as(
                "SELECT status,payload FROM sync_replay_operations WHERE operation_id=$1",
            )
            .bind(id)
            .fetch_one(&state.db)
            .await
            .unwrap();
            assert_eq!(saved.0, "failed");
            assert_eq!(serde_json::from_str::<Value>(&saved.1).unwrap(), payload);
        }
        let pairs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM doc_tags")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(pairs, 0);
    }

    #[tokio::test]
    async fn replay_execution_applies_portable_sql_and_ledger_skips() {
        let state = property_replay_test_state().await;
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
            sqlx::query_as("SELECT name, sort_order FROM folders WHERE id = $1")
                .bind("folder-1")
                .fetch_one(&state.db)
                .await
                .expect("folder replay row");
        let tag: (String, String) = sqlx::query_as("SELECT name, color FROM tags WHERE id = $1")
            .bind("tag-1")
            .fetch_one(&state.db)
            .await
            .expect("tag replay row");
        let property: (String, String, i64) = sqlx::query_as(
            "SELECT name, property_type, sort_order FROM properties_schema WHERE id = $1",
        )
        .bind("property-1")
        .fetch_one(&state.db)
        .await
        .expect("property replay row");
        let ledger_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sync_replay_operations WHERE status = $1")
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
    async fn replay_identifier_rest_lost_ack_and_ledger_retry_never_reapply_newer_metadata() {
        let state = property_replay_test_state().await;
        sqlx::query("UPDATE documents SET updated_at='2000-01-01T00:00:00Z',published_at='2000-01-01T00:00:00Z' WHERE id='doc-1'").execute(&state.db).await.unwrap();
        let auth = folder_dispatch_auth(&state).await;
        let payload = json!({"id":"doc-1","space_id":"space-1","human_identifier":"Original-1","source":{"page_updated_at":"2000-01-01T00:00:00Z","published_at":"2000-01-01T00:00:00+00:00","human_identifier":null}});
        crate::spaces::page_metadata::mutate(
            &state,
            &auth,
            "self-hosted",
            "identifier-lost-ack",
            &payload,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE documents SET human_identifier='Later-2',human_identifier_key='LATER-2',updated_at='2002-01-01T00:00:00Z' WHERE id='doc-1'").execute(&state.db).await.unwrap();
        let op = replay_operation(
            "identifier-lost-ack",
            "2000-01-01T00:00:00Z",
            "document",
            "doc-1",
            "update_identifier",
            payload.clone(),
        );
        let manifest = single_operation_manifest(op.clone());
        for _ in 0..2 {
            let result = super::execute_replay_operation(&state, &auth, &manifest, &op).await;
            assert_eq!(result.status, "skipped", "{:?}", result);
            let row: (String, String) = sqlx::query_as(
                "SELECT human_identifier,updated_at FROM documents WHERE id='doc-1'",
            )
            .fetch_one(&state.db)
            .await
            .unwrap();
            assert_eq!(row, ("Later-2".into(), "2002-01-01T00:00:00Z".into()));
        }
        let mut collision = op.clone();
        collision.payload["human_identifier"] = json!("Forged");
        let before = crate::spaces::documents::tests::move_database_snapshot(&state).await;
        assert_eq!(
            super::execute_replay_operation(&state, &auth, &manifest, &collision)
                .await
                .status,
            "failed"
        );
        assert_eq!(
            crate::spaces::documents::tests::move_database_snapshot(&state).await,
            before
        );
    }
    #[tokio::test]
    async fn replay_identifier_unproven_success_and_malformed_intent_retain_failed_payload() {
        let state = property_replay_test_state().await;
        let auth = folder_dispatch_auth(&state).await;
        let payload = json!({"id":"doc-1","space_id":"space-1","human_identifier":null,"source":{"page_updated_at":"2000-01-01T00:00:00Z","published_at":null,"human_identifier":null}});
        let op = replay_operation(
            "identifier-invalid",
            "2000-01-01T00:00:00Z",
            "document",
            "doc-1",
            "update_identifier",
            payload.clone(),
        );
        let manifest = single_operation_manifest(op.clone());
        let before: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT human_identifier,human_identifier_key FROM documents WHERE id='doc-1'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        for _ in 0..2 {
            assert_eq!(
                super::execute_replay_operation(&state, &auth, &manifest, &op)
                    .await
                    .status,
                "failed"
            );
        }
        let saved:(String,String)=sqlx::query_as("SELECT status,payload FROM sync_replay_operations WHERE operation_id='identifier-invalid'").fetch_one(&state.db).await.unwrap();
        assert_eq!(saved.0, "failed");
        assert_eq!(serde_json::from_str::<Value>(&saved.1).unwrap(), payload);
        sqlx::query("UPDATE sync_replay_operations SET status='applied' WHERE operation_id='identifier-invalid'").execute(&state.db).await.unwrap();
        assert_eq!(
            super::execute_replay_operation(&state, &auth, &manifest, &op)
                .await
                .status,
            "failed"
        );
        let after: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT human_identifier,human_identifier_key FROM documents WHERE id='doc-1'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(after, before);
    }

    #[tokio::test]
    async fn replay_identifier_create_import_and_legacy_update_preserve_authoritative_reference() {
        let _data_root=crate::storage::fixture_root::FixtureRoot::private();
        let state = property_replay_test_state().await;
        let auth = folder_dispatch_auth(&state).await;
        crate::storage::init_data_dir().await.unwrap();
        let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id='space-1'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        crate::storage::init_space_dirs(&slug).await.unwrap();
        let id = format!("legacy-import-reference-{}", uuid::Uuid::new_v4());
        let operation = replay_operation(
            "identifier-create",
            "2000-01-01T00:00:00Z",
            "document",
            &id,
            "create",
            json!({"id":id,"space_id":"space-1","title":"Imported","content":"PRIVATE RAW BODY","human_identifier":" Replay-3 ","created_at":"2000-01-01T00:00:00Z","updated_at":"2001-01-01T00:00:00Z"}),
        );
        super::replay_document_create(&state, &auth, "self-hosted", &operation)
            .await
            .unwrap();
        let pair: (String, String) = sqlx::query_as(
            "SELECT human_identifier,human_identifier_key FROM documents WHERE id=$1",
        )
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(pair, ("Replay-3".into(), "REPLAY-3".into()));
        super::replay_document_create(&state, &auth, "self-hosted", &operation)
            .await
            .unwrap();
        let mut conflict = operation.clone();
        conflict.id = "identifier-create-collision".into();
        conflict.entity_id = "colliding-reference".into();
        conflict.payload["id"] = json!("colliding-reference");
        conflict.payload["human_identifier"] = json!("replay-3");
        let before = crate::spaces::documents::tests::move_database_snapshot(&state).await;
        assert!(
            super::replay_document_create(&state, &auth, "self-hosted", &conflict)
                .await
                .is_err()
        );
        assert_eq!(
            crate::spaces::documents::tests::move_database_snapshot(&state).await,
            before
        );
        let (state, _) = metadata_fixture(false).await;
        let auth = folder_dispatch_auth(&state).await;
        sqlx::query("UPDATE documents SET human_identifier='Keep-1',human_identifier_key='KEEP-1' WHERE id='doc-1'").execute(&state.db).await.unwrap();
        let legacy = replay_operation(
            "legacy-identifier-preservation",
            "2026-10-08T12:00:00Z",
            "document",
            "doc-1",
            "update",
            json!({"id":"doc-1","space_id":"space-1","title":"Canonical","human_identifier":null,"updated_at":"2026-10-08T12:00:00Z"}),
        );
        super::replay_document_update(&state, &auth, "self-hosted", &legacy)
            .await
            .unwrap();
        let pair: (String, String) = sqlx::query_as(
            "SELECT human_identifier,human_identifier_key FROM documents WHERE id='doc-1'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(pair, ("Keep-1".into(), "KEEP-1".into()));
    }

    async fn folder_dispatch_auth(state: &AppState) -> AuthUser {
        let claims = auth_user().claims;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let request = axum::http::Request::builder()
            .method("POST")
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();
        <AuthUser as axum::extract::FromRequestParts<AppState>>::from_request_parts(
            &mut parts, state,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn replay_execution_preserves_folder_create_at_depth_four() {
        let state = property_replay_test_state().await;
        let actual = folder_dispatch_auth(&state).await;
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

        let response = execute_replay_manifest(State(state.clone()), actual, Json(manifest))
            .await
            .expect("execute replay")
            .0;

        assert_eq!(response.status, "completed");
        assert_eq!(response.failed_operation_count, 0);
        assert_eq!(response.results[0].status, "applied");

        let folder_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM folders WHERE id = $1")
            .bind("folder-too-deep")
            .fetch_one(&state.db)
            .await
            .expect("folder count");
        assert_eq!(folder_count.0, 1);
        let parent: String = sqlx::query_scalar("SELECT parent_id FROM folders WHERE id=$1")
            .bind("folder-too-deep").fetch_one(&state.db).await.unwrap();
        assert_eq!(parent, "folder-grandchild");
    }

    #[tokio::test]
    async fn replay_execution_applies_document_update() {
        let (state,path) = metadata_fixture(false).await;
        seed_folder(&state.db, "folder-target", None, "Target", 1).await;
        seed_document(&state.db, "doc-update", None, "Original").await;
        sqlx::query("UPDATE documents SET content_path=$1 WHERE id='doc-update'").bind(&path).execute(&state.db).await.unwrap();
        tokio::fs::write(crate::storage::data_dir().join(&path),b"---\ntitle: \"Original\"\nicon: \"\"\n---\nBody").await.unwrap();

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
            "SELECT title, folder_id, last_updated_by, icon, cover_image FROM documents WHERE id = $1",
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
        let (state, existing_path) = canonical_replay_fixture(false).await;
        seed_document(&state.db, "doc-delete", None, "Delete Me").await;
        let target_file = crate::storage::data_dir().join(&existing_path)
            .parent().unwrap().join("doc-delete.md");
        let retained_path = target_file.strip_prefix(crate::storage::data_dir())
            .unwrap().to_string_lossy().into_owned();
        tokio::fs::write(&target_file,b"TARGET DELETE RETAINED RAW\r\n").await.unwrap();
        sqlx::query("UPDATE documents SET content_path=$1 WHERE id='doc-delete'")
            .bind(&retained_path).execute(&state.db).await.unwrap();


        let (created,deleted):(String,Option<String>)=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE id='doc-delete'").fetch_one(&state.db).await.unwrap();let original=json!({"created_at":created,"deleted_at":deleted});
        let manifest = single_operation_manifest(replay_operation(
            "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            "2026-07-08T12:04:00.000Z",
            "document",
            "doc-delete",
            "delete",
            json!({
                "id": "doc-delete",
                "space_id": "space-1",
                "original_target":original
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

        let document_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents WHERE id = $1")
            .bind("doc-delete")
            .fetch_one(&state.db)
            .await
            .expect("document count");
        assert_eq!(document_count.0, 0);

        assert_eq!(tokio::fs::read(crate::storage::data_dir().join(&retained_path)).await.unwrap(), b"TARGET DELETE RETAINED RAW\r\n");
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_delete_replay_receipts WHERE incoming_id='dddddddd-dddd-4ddd-8ddd-dddddddddddd'").fetch_one(&state.db).await.unwrap();
        assert_eq!(count,1);
        let second_response = execute_replay_manifest(State(state), auth_user(), Json(manifest))
            .await
            .expect("execute replay again")
            .0;
        assert_eq!(second_response.status, "completed");
        assert_eq!(second_response.applied_operation_count, 0);
        assert_eq!(second_response.skipped_operation_count, 1);
        let retained_file=crate::storage::data_dir().join(&retained_path);
        assert_eq!(tokio::fs::read(&retained_file).await.unwrap(),b"TARGET DELETE RETAINED RAW\r\n");
        tokio::fs::remove_dir_all(retained_file.parent().unwrap().parent().unwrap()).await.unwrap();
    }

    #[tokio::test]
    async fn replay_execution_applies_asset_delete() {
        let (state, _) = canonical_replay_fixture(false).await;
        sqlx::query(
            "INSERT INTO attachments (id, document_id, space_id, filename, original_name, mime_type, size_bytes, storage_path, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind("asset-delete")
        .bind("doc-1")
        .bind("space-1")
        .bind("asset-delete.txt")
        .bind("asset-delete.txt")
        .bind("text/plain")
        .bind(5_i64)
        .bind("spaces/replay-space/attachments/doc-1/asset-delete.txt")
        .bind("user-1")
        .execute(&state.db)
        .await
        .expect("insert replay test attachment");
        let mut conn=state.db.acquire().await.unwrap();crate::spaces::asset_audiences::admit_legacy(&mut conn,"asset-delete","space-1","doc-1").await.unwrap();drop(conn);

        let manifest = single_operation_manifest(replay_operation(
            "syncop-asset-delete",
            "2026-07-08T12:04:00.000Z",
            "asset",
            "asset-delete",
            "delete",
            json!({
                "id": "asset-delete",
                "space_id": "space-1",
                "document_id": "doc-1"
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
            sqlx::query_as("SELECT COUNT(*) FROM attachments WHERE id = $1")
                .bind("asset-delete")
                .fetch_one(&state.db)
                .await
                .expect("attachment count");
        assert_eq!(attachment_count.0, 1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM attachment_retirements WHERE attachment_id='asset-delete'").fetch_one(&state.db).await.unwrap(),1);

        let second_response = execute_replay_manifest(State(state), auth_user(), Json(manifest))
            .await
            .expect("execute replay again")
            .0;
        assert_eq!(second_response.status, "completed");
        assert_eq!(second_response.applied_operation_count, 0);
        assert_eq!(second_response.skipped_operation_count, 1);
    }

    #[tokio::test]
    async fn replay_execution_preserves_document_create_in_deep_folder() {
        let (state, _) = canonical_replay_fixture(false).await;
        seed_three_level_folder_tree(&state.db).await;
        seed_folder(
            &state.db,
            "folder-too-deep",
            Some("folder-grandchild"),
            "Too Deep",
            1,
        )
        .await;
        let actual = folder_dispatch_auth(&state).await;
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

        let response = execute_replay_manifest(State(state.clone()), actual, Json(manifest))
            .await
            .expect("execute replay")
            .0;

        assert_eq!(response.status, "completed", "{response:?}");
        assert_eq!(response.failed_operation_count, 0);
        assert_eq!(response.results[0].status, "applied");

        let document_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents WHERE id = $1")
            .bind("doc-too-deep")
            .fetch_one(&state.db)
            .await
            .expect("document count");
        assert_eq!(document_count.0, 1);
        let parent: String = sqlx::query_scalar("SELECT folder_id FROM documents WHERE id=$1")
            .bind("doc-too-deep").fetch_one(&state.db).await.unwrap();
        assert_eq!(parent, "folder-too-deep");
    }

    #[tokio::test]
    async fn replay_execution_blocks_folder_update_cycle() {
        let state = property_replay_test_state().await;
        let actual = folder_dispatch_auth(&state).await;
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

        let response = execute_replay_manifest(State(state.clone()), actual, Json(manifest))
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
            sqlx::query_as("SELECT parent_id FROM folders WHERE id = $1")
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
            sqlx::query_as("SELECT COUNT(*) FROM sync_replay_operations WHERE status = $1")
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
    async fn canonical_replay_fixture(disk: bool) -> (AppState, String) {
        let mut state = property_replay_test_state().await;
        if disk {
            let db_path = std::env::temp_dir()
                .join(format!("replay-canonical-{}.sqlite", uuid::Uuid::new_v4()));
            state.db = AnyPoolOptions::new()
                .max_connections(2)
                .after_connect(|conn, _| {
                    Box::pin(async move {
                        sqlx::query("PRAGMA busy_timeout=5000")
                            .execute(&mut *conn)
                            .await?;
                        sqlx::query("PRAGMA journal_mode=WAL").execute(conn).await?;
                        Ok(())
                    })
                })
                .connect(&format!("sqlite://{}?mode=rwc", db_path.display()))
                .await
                .unwrap();
            sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
            seed_replay_fixture_data(&state.db).await;
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('test-session','user-1','canonical','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
            seed_document(&state.db, "doc-1", None, "Canonical").await;
        }
        let slug = format!("canonical-replay-{}", uuid::Uuid::new_v4());
        let path = format!("spaces/{slug}/docs/original.md");
        sqlx::query("UPDATE spaces SET slug=$1 WHERE id='space-1'")
            .bind(&slug)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET content_path=$1,updated_at='source-revision',published_at='source-publication' WHERE id='doc-1'").bind(&path).execute(&state.db).await.unwrap();
        tokio::fs::create_dir_all(crate::storage::data_dir().join(format!("spaces/{slug}/docs")))
            .await
            .unwrap();
        tokio::fs::write(
            crate::storage::data_dir().join(&path),
            b"PUBLIC OLD RAW\r\n",
        )
        .await
        .unwrap();
        (state, path)
    }
    fn canonical_replay_operation(id: &str, body: &str) -> SyncReplayOperation {
        replay_operation(
            id,
            "2026-10-08T12:00:00Z",
            "document",
            "doc-1",
            "update_content",
            json!({"id":"doc-1","space_id":"space-1","content":body,"updated_at":"2026-10-08T12:00:00Z"}),
        )
    }
    async fn source_bound_operation(state:&AppState,id:&str,body:&str)->SyncReplayOperation {
        let row=crate::spaces::published_source::selected(&mut state.db.acquire().await.unwrap(),"space-1","doc-1").await.unwrap();
        let old=crate::storage::canonical_rewrite::published_source_bytes(row["content_path"].as_str().unwrap()).unwrap();
        let mut op=canonical_replay_operation(id,body);op.payload["published_source_digest"]=json!(crate::spaces::published_source::digest(&row,&old).unwrap());op
    }
    #[tokio::test]
    async fn published_original_source_stale_missing_and_malformed_fail_before_storage() {
        let (state,old)=canonical_replay_fixture(false).await;
        let original=source_bound_operation(&state,"original","NEW").await;
        // A real newer metadata change, SAME clock, must invalidate full row evidence.
        sqlx::query("UPDATE documents SET title='Later title' WHERE id='doc-1'").execute(&state.db).await.unwrap();
        let before=crate::spaces::documents::tests::move_database_snapshot(&state).await;
        for value in [None,Some(json!(null)),Some(json!("not-a-source")),Some(original.payload["published_source_digest"].clone())] {
            let mut op=original.clone();match value {None=>{op.payload.as_object_mut().unwrap().remove("published_source_digest");},Some(v)=>op.payload["published_source_digest"]=v};
            assert!(super::replay_document_update_content(&state,&auth_user(),"actual-provider",&op).await.is_err());
            assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&state).await,before);
            assert_eq!(tokio::fs::read(crate::storage::data_dir().join(&old)).await.unwrap(),b"PUBLIC OLD RAW\r\n");
            assert!(!crate::storage::data_dir().join(&old).parent().unwrap().join("revisions").exists());
        }
    }
    #[tokio::test]
    async fn published_result_full_raw_same_stamp_trigger_rolls_back() {
        let (state,old)=canonical_replay_fixture(false).await;
        let op=source_bound_operation(&state,"trigger","NEW").await;
        sqlx::query("CREATE TRIGGER published_full_result AFTER UPDATE OF content_path ON documents BEGIN UPDATE documents SET title='CORRUPTED SAME STAMP' WHERE id=NEW.id; END").execute(&state.db).await.unwrap();
        let before=crate::spaces::documents::tests::move_database_snapshot(&state).await;
        assert!(super::replay_document_update_content(&state,&auth_user(),"p",&op).await.is_err());
        assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&state).await,before);
        assert_eq!(tokio::fs::read(crate::storage::data_dir().join(old)).await.unwrap(),b"PUBLIC OLD RAW\r\n");
    }
    #[tokio::test]
    async fn published_result_ack_exact_request_and_historical_never_reapply() {
        let (state,_)=canonical_replay_fixture(false).await;
        let op=source_bound_operation(&state,"ack","NEW exact 🦅\r\n").await;let m=single_operation_manifest(op.clone());
        let first=super::execute_replay_operation(&state,&auth_user(),&m,&op).await;assert_eq!(first.status,"applied","{:?}",first);
        let evidence=first.published_source.unwrap();assert_eq!(evidence.original_source_digest,op.payload["published_source_digest"].as_str().unwrap());assert_eq!(evidence.payload_fingerprint,super::replay_body_incoming(&m.provider,&op).unwrap().payload_fingerprint);
        let row=crate::spaces::published_source::selected(&mut state.db.acquire().await.unwrap(),"space-1","doc-1").await.unwrap();assert_eq!(evidence.published_source_digest,crate::spaces::published_source::digest(&row,op.payload["content"].as_str().unwrap().as_bytes()).unwrap());
        sqlx::query("UPDATE documents SET title='Later',published_at=NULL WHERE id='doc-1'").execute(&state.db).await.unwrap();let before=crate::spaces::documents::tests::move_database_snapshot(&state).await;
        let retry=super::execute_replay_operation(&state,&auth_user(),&m,&op).await;assert_eq!(retry.status,"skipped");assert_eq!(retry.published_source,Some(evidence));assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&state).await,before);
        let mut forged=op.clone();forged.payload["content"]=json!("Forged");assert_eq!(super::execute_replay_operation(&state,&auth_user(),&m,&forged).await.status,"failed");assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&state).await,before);
    }
    #[tokio::test]
    async fn published_final_original_jwt_expires_after_receipt_proof_rolls_back() {
        let (s,old)=canonical_replay_fixture(false).await;
        let op=source_bound_operation(&s,"expiry-after-receipt","NEW").await;
        let mut claims=auth_user().claims;claims.exp=chrono::Utc::now().timestamp()+2;
        let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes())).unwrap();
        let (mut parts,_)=axum::http::Request::builder().method("POST").header("Authorization",format!("Bearer {token}")).body(()).unwrap().into_parts();
        let auth=<AuthUser as axum::extract::FromRequestParts<AppState>>::from_request_parts(&mut parts,&s).await.unwrap();
        let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;
        super::REPLAY_PUBLISHED_BEFORE_IDENTITY.with(|h|*h.borrow_mut()=Some(Box::pin(async {tokio::time::sleep(std::time::Duration::from_secs(3)).await;})));
        assert!(super::replay_document_update_content(&s,&auth,"actual-provider",&op).await.is_err());
        assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before);
        assert_eq!(tokio::fs::read(crate::storage::data_dir().join(old)).await.unwrap(),b"PUBLIC OLD RAW\r\n");
        // Persistent session remains long-lived; JWT deadline is the failing proof.
        crate::spaces::access::require_current_human_preference_write(&mut s.db.acquire().await.unwrap(),&auth_user()).await.unwrap();
    }
    #[tokio::test]
    async fn published_legacy_applied_ledger_has_no_original_receipt_ack() {
        let (s,_)=canonical_replay_fixture(false).await;let op=canonical_replay_operation("unproven-applied","NEW");let m=single_operation_manifest(op.clone());
        super::insert_replay_ledger(&s,&auth_user(),&m,&op,"applied",None).await.unwrap();let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;
        let r=super::execute_replay_operation(&s,&auth_user(),&m,&op).await;assert_eq!(r.status,"failed");assert!(r.published_source.is_none());assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before);
    }
    #[tokio::test]
    async fn published_original_source_real_wal_guard_wait_rejects_newer_row() {
        let (state,old)=canonical_replay_fixture(true).await;let op=source_bound_operation(&state,"wait-source","DENIED").await;
        let mut hold=state.db.begin().await.unwrap();crate::spaces::mutation_guards::lock_audience_for_mutation(&mut hold).await.unwrap();crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut hold,&["space-1"]).await.unwrap();
        sqlx::query("UPDATE documents SET title='Committed newer same clock' WHERE id='doc-1'").execute(&mut *hold).await.unwrap();
        let worker=state.clone();let task=tokio::spawn(async move {super::replay_document_update_content(&worker,&auth_user(),"actual-provider",&op).await});
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;assert!(!task.is_finished());hold.commit().await.unwrap();
        let before=crate::spaces::documents::tests::move_database_snapshot(&state).await;assert!(task.await.unwrap().unwrap_err().contains("source_conflict"));assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&state).await,before);
        crate::spaces::access::require_current_human_preference_write(&mut state.db.acquire().await.unwrap(),&auth_user()).await.unwrap();assert_eq!(tokio::fs::read(crate::storage::data_dir().join(old)).await.unwrap(),b"PUBLIC OLD RAW\r\n");
    }
    #[tokio::test]
    async fn published_result_sql_labels_cannot_replace_sealed_ack_evidence() {
        for field in ["result_source_digest","original_source_digest"] {
            let (s,_)=canonical_replay_fixture(false).await;let op=source_bound_operation(&s,"seal","NEW").await;let m=single_operation_manifest(op.clone());
            assert_eq!(super::execute_replay_operation(&s,&auth_user(),&m,&op).await.status,"applied");
            let raw:String=sqlx::query_scalar("SELECT manifest_json FROM canonical_revision_receipts").fetch_one(&s.db).await.unwrap();let mut altered:Value=serde_json::from_str(&raw).unwrap();altered[field]=json!("f".repeat(64));
            sqlx::query("UPDATE canonical_revision_receipts SET manifest_json=$1").bind(altered.to_string()).execute(&s.db).await.unwrap();let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;
            let result=super::execute_replay_operation(&s,&auth_user(),&m,&op).await;assert_eq!(result.status,"failed");assert!(result.published_source.is_none());assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before);
        }
    }
    async fn canonical_replay_source(state: &AppState) -> (String, String, Option<String>) {
        sqlx::query_as(
            "SELECT content_path,updated_at,published_at FROM documents WHERE id='doc-1'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap()
    }
    #[tokio::test]
    async fn canonical_replay_preserves_raw_bytes_publication_and_historical_identity() {
        let (state, old) = canonical_replay_fixture(false).await;
        let raw = "---\r\ntitle: Raw\r\n---\r\nUNICODE 🦅 body\r\n";
        let op = source_bound_operation(&state," legacy:body operation ", raw).await;
        assert_eq!(
            super::replay_document_update_content(&state, &auth_user(), "actual-provider", &op)
                .await
                .unwrap(),
            "applied"
        );
        let first = canonical_replay_source(&state).await;
        assert_ne!(first.0, old);
        assert!(first.0.contains("/docs/revisions/"));
        assert_eq!(first.2.as_deref(), Some("source-publication"));
        assert_eq!(
            tokio::fs::read(crate::storage::data_dir().join(&first.0))
                .await
                .unwrap(),
            raw.as_bytes()
        );
        assert_eq!(
            tokio::fs::read(crate::storage::data_dir().join(&old))
                .await
                .unwrap(),
            b"PUBLIC OLD RAW\r\n"
        );
        let receipt: (String, String, String) =
            sqlx::query_as("SELECT provider,incoming_id,action FROM canonical_revision_receipts")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(
            receipt,
            (
                "actual-provider".into(),
                op.id.clone(),
                "update_content".into()
            )
        );
        let mut newer = source_bound_operation(&state,"newer", "Later body").await;
        newer.payload["updated_at"] = json!("2026-10-08T12:01:00Z");
        super::replay_document_update_content(&state, &auth_user(), "actual-provider", &newer)
            .await
            .unwrap();
        let last = canonical_replay_source(&state).await;
        super::replay_document_update_content(&state, &auth_user(), "actual-provider", &op)
            .await
            .unwrap();
        assert_eq!(canonical_replay_source(&state).await, last);
        let mut collision = op.clone();
        collision.payload["content"] = json!("Collision");
        assert!(super::replay_document_update_content(
            &state,
            &auth_user(),
            "actual-provider",
            &collision
        )
        .await
        .is_err());
        assert_eq!(canonical_replay_source(&state).await, last);
        sqlx::query("DELETE FROM sessions")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::replay_document_update_content(
            &state,
            &auth_user(),
            "actual-provider",
            &op
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn canonical_replay_late_policy_or_source_changes_rollback_pointer_and_receipt() {
        for mutation in ["DELETE FROM sessions WHERE id='test-session'","UPDATE users SET is_active=0 WHERE id='user-1'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','user-1','user-1')","UPDATE documents SET locked=1,locked_by='other-user' WHERE id='doc-1'","UPDATE documents SET updated_at='trigger-revision' WHERE id='doc-1'","UPDATE documents SET published_at=NULL WHERE id='doc-1'"] {
            let (state,old)=canonical_replay_fixture(false).await;
            let before=canonical_replay_source(&state).await;
            sqlx::query(&format!("CREATE TRIGGER late_canonical AFTER INSERT ON canonical_revision_receipts BEGIN {mutation}; END")).execute(&state.db).await.unwrap();
            assert!(super::replay_document_update_content(&state,&auth_user(),"self-hosted",&source_bound_operation(&state,"late","NEW").await).await.is_err(),"{mutation}");
            assert_eq!(canonical_replay_source(&state).await,before);
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM canonical_revision_receipts").fetch_one(&state.db).await.unwrap();assert_eq!(count,0);
            assert_eq!(tokio::fs::read(crate::storage::data_dir().join(old)).await.unwrap(),b"PUBLIC OLD RAW\r\n");
        }
    }
    #[tokio::test]
    async fn canonical_replay_current_write_key_scope_and_late_downgrade() {
        let (state, _) = canonical_replay_fixture(false).await;
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('canonical-key','user-1','Test','hash','prefix','[\"write\"]')").execute(&state.db).await.unwrap();
        let mut auth = auth_user();
        auth.claims.sid = "api-key:canonical-key".into();
        auth.api_key_scopes = Some(vec!["write".into()]);
        super::replay_document_update_content(
            &state,
            &auth,
            "self-hosted",
            &source_bound_operation(&state,"write-key", "FIRST").await,
        )
        .await
        .unwrap();
        let before = canonical_replay_source(&state).await;
        sqlx::query("CREATE TRIGGER late_key AFTER INSERT ON canonical_revision_receipts BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='canonical-key'; END").execute(&state.db).await.unwrap();
        assert!(super::replay_document_update_content(
            &state,
            &auth,
            "self-hosted",
            &source_bound_operation(&state,"late-key", "SECOND").await
        )
        .await
        .is_err());
        assert_eq!(canonical_replay_source(&state).await, before);
        sqlx::query("DROP TRIGGER late_key")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='canonical-key'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::replay_document_update_content(
            &state,
            &auth,
            "self-hosted",
            &source_bound_operation(&state,"read-key", "DENIED").await
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn canonical_replay_disk_wal_reader_sees_old_pointer_until_atomic_commit() {
        let (state, old) = canonical_replay_fixture(true).await;
        let observer = state.clone();
        let old_observed = old.clone();
        super::REPLAY_CANONICAL_BEFORE_COMMIT.with(|slot| {
            *slot.borrow_mut() = Some(Box::pin(async move {
                let source = canonical_replay_source(&observer).await;
                assert_eq!(source.0, old_observed);
                assert_eq!(
                    tokio::fs::read(crate::storage::data_dir().join(&source.0))
                        .await
                        .unwrap(),
                    b"PUBLIC OLD RAW\r\n"
                );
                let receipts: i64 =
                    sqlx::query_scalar("SELECT count(*) FROM canonical_revision_receipts")
                        .fetch_one(&observer.db)
                        .await
                        .unwrap();
                assert_eq!(receipts, 0);
            }))
        });
        super::replay_document_update_content(
            &state,
            &auth_user(),
            "self-hosted",
            &source_bound_operation(&state,"wal", "NEW EXACT").await,
        )
        .await
        .unwrap();
        let after = canonical_replay_source(&state).await;
        assert_ne!(after.0, old);
        assert_eq!(
            tokio::fs::read(crate::storage::data_dir().join(after.0))
                .await
                .unwrap(),
            b"NEW EXACT"
        );
    }
    #[tokio::test]
    async fn canonical_replay_commit_faults_use_receipts_without_reapplication() {
        use super::ReplayCanonicalCommitFault;
        for fault in [
            ReplayCanonicalCommitFault::LostAck,
            ReplayCanonicalCommitFault::Rollback,
            ReplayCanonicalCommitFault::UnknownProof,
        ] {
            let (state, _) = canonical_replay_fixture(false).await;
            let before = canonical_replay_source(&state).await;
            let op = source_bound_operation(&state,"commit-fault", "NEW").await;
            super::REPLAY_CANONICAL_COMMIT_FAULT.with(|slot| *slot.borrow_mut() = Some(fault));
            if fault == ReplayCanonicalCommitFault::UnknownProof {
                let unavailable = state.clone();
                super::REPLAY_CANONICAL_BEFORE_CONFIRM.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {
                    sqlx::query("ALTER TABLE canonical_revision_receipts RENAME TO unavailable_canonical_receipts").execute(&unavailable.db).await.unwrap();
                })));
            }
            let result =
                super::replay_document_update_content(&state, &auth_user(), "self-hosted", &op)
                    .await;
            match fault {
                ReplayCanonicalCommitFault::LostAck => assert_eq!(result.unwrap(), "applied"),
                ReplayCanonicalCommitFault::Rollback => {
                    assert!(result.unwrap_err().contains("did not apply"));
                    assert_eq!(canonical_replay_source(&state).await, before);
                    continue;
                }
                ReplayCanonicalCommitFault::UnknownProof => {
                    assert!(result.unwrap_err().contains("indeterminate"));
                    sqlx::query("ALTER TABLE unavailable_canonical_receipts RENAME TO canonical_revision_receipts").execute(&state.db).await.unwrap();
                }
            }
            let committed = canonical_replay_source(&state).await;
            super::replay_document_update_content(&state, &auth_user(), "self-hosted", &op)
                .await
                .unwrap();
            assert_eq!(canonical_replay_source(&state).await, committed);
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM canonical_revision_receipts")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(count, 1);
        }
    }
    #[tokio::test]
    async fn canonical_replay_pending_ledger_reconciles_exact_historical_receipt() {
        let (state, _) = canonical_replay_fixture(false).await;
        let op = source_bound_operation(&state,"historical-pending", "NEW").await;
        super::replay_document_update_content(&state, &auth_user(), "self-hosted", &op)
            .await
            .unwrap();
        let manifest = single_operation_manifest(op.clone());
        super::insert_replay_ledger(&state, &auth_user(), &manifest, &op, "pending", None)
            .await
            .unwrap();
        let response =
            execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest.clone()))
                .await
                .unwrap()
                .0;
        assert_eq!(response.skipped_operation_count, 1);
        assert_eq!(response.failed_operation_count, 0);
        let mut collision = manifest;
        collision.operations[0].payload["content"] = json!("BAD");
        let response = execute_replay_manifest(State(state), auth_user(), Json(collision))
            .await
            .unwrap()
            .0;
        assert_eq!(response.failed_operation_count, 1);
    }

    #[tokio::test]
    async fn canonical_replay_http_write_only_key_and_read_only_denial() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;
        let (state, _) = canonical_replay_fixture(false).await;
        let raw = "canonical-http-key";
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('canonical-http','user-1','HTTP',$1,'prefix','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key(raw)).execute(&state.db).await.unwrap();
        let app = super::api_router().with_state(state.clone());
        let original_http=source_bound_operation(&state,"original-http","HTTP RAW").await.payload["published_source_digest"].clone();
        let request = |id: &str| {
            Request::builder().method("POST").uri("/sync/replay").header("X-API-Key",raw).header("content-type","application/json").body(Body::from({
            let c=contract();let mut op=canonical_replay_operation(id,"HTTP RAW");op.payload["published_source_digest"]=original_http.clone();
            json!({"schema_version":1,"provider":"self-hosted","operation_count":1,"recovery_checkpoint_count":0,"recovery_checkpoints":[],"replay_contract":{"schema_version":c.schema_version,"operation_order":c.operation_order,"idempotency_key":c.idempotency_key,"transport":c.transport,"server_backends":c.server_backends,"required_payload_status":c.required_payload_status,"conflict_policy":c.conflict_policy,"web_canonical_store":c.web_canonical_store},"operations":[{"id":op.id,"entity_type":op.entity_type,"entity_id":op.entity_id,"action":op.action,"queued_at":op.queued_at,"payload_status":op.payload_status,"payload":op.payload}]}).to_string()
        })).unwrap()
        };
        let response = app.clone().oneshot(request("http-write")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["applied_operation_count"], 1);
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='canonical-http'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            app.oneshot(request("http-read-denied"))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    async fn canonical_replay_draft_exact_share_revocation_rolls_back_without_publication() {
        let (state, old) = canonical_replay_fixture(false).await;
        sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES('shared-editor','share@test','Share','member')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('shared-session','shared-editor','shared','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('shared-member','space-1','shared-editor','editor')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='doc-1'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('shared-exact','doc-1','space-1','user-1','Private','OLD DRAFT','source-revision')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('shared-exact','shared-editor','editor','user-1')").execute(&state.db).await.unwrap();
        let mut auth = auth_user();
        auth.claims.sub = "shared-editor".into();
        auth.claims.sid = "shared-session".into();
        let mut op = canonical_replay_operation("shared-draft", "NEW DRAFT");
        op.payload["draft_id"] = json!("shared-exact");
        op.payload["source_digest"]=json!(crate::spaces::draft_save::digest(&crate::spaces::draft_save::selected(&mut state.db.acquire().await.unwrap(),"space-1","doc-1","shared-exact").await.unwrap()).unwrap());
        let before = canonical_replay_source(&state).await;
        sqlx::query("CREATE TRIGGER late_draft_share AFTER UPDATE OF content ON document_drafts BEGIN DELETE FROM document_draft_access WHERE draft_id='shared-exact'; END").execute(&state.db).await.unwrap();
        assert!(
            super::replay_document_update_content(&state, &auth, "self-hosted", &op)
                .await
                .is_err()
        );
        let body: String =
            sqlx::query_scalar("SELECT content FROM document_drafts WHERE id='shared-exact'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(body, "OLD DRAFT");
        assert_eq!(canonical_replay_source(&state).await, before);
        sqlx::query("DROP TRIGGER late_draft_share")
            .execute(&state.db)
            .await
            .unwrap();
        super::replay_document_update_content(&state, &auth, "self-hosted", &op)
            .await
            .unwrap();
        assert_eq!(canonical_replay_source(&state).await, before);
        assert_eq!(
            tokio::fs::read(crate::storage::data_dir().join(old))
                .await
                .unwrap(),
            b"PUBLIC OLD RAW\r\n"
        );
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM canonical_revision_receipts")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
    #[tokio::test]
    async fn canonical_replay_guard_wait_observes_external_committed_session_revocation() {
        let (state, old) = canonical_replay_fixture(true).await;
        let before = canonical_replay_source(&state).await;
        let mut holder = state.db.begin().await.unwrap();
        crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut holder, &["space-1"])
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE id='test-session'")
            .execute(&mut *holder)
            .await
            .unwrap();
        let worker_state = state.clone();
        let task = tokio::spawn(async move {
            super::replay_document_update_content(
                &worker_state,
                &auth_user(),
                "self-hosted",
                &source_bound_operation(&worker_state,"wait-revoke", "DENIED").await,
            )
            .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        assert!(
            !task.is_finished(),
            "worker must wait on current space write guard"
        );
        holder.commit().await.unwrap();
        assert!(task.await.unwrap().is_err());
        assert_eq!(canonical_replay_source(&state).await, before);
        assert_eq!(
            tokio::fs::read(crate::storage::data_dir().join(old))
                .await
                .unwrap(),
            b"PUBLIC OLD RAW\r\n"
        );
    }

    #[tokio::test]
    async fn canonical_replay_existing_resolve_actions_bind_exact_receipt_without_publish_alias() {
        for action in [
            "resolve_conflict:merge",
            "resolve_conflict:keep-local",
            "resolve_conflict:keep-remote",
        ] {
            let (state, _) = canonical_replay_fixture(false).await;
            let mut op = source_bound_operation(&state,"resolve-exact", "RAW RESOLVED").await;
            op.action = action.into();
            let response = execute_replay_manifest(
                State(state.clone()),
                auth_user(),
                Json(single_operation_manifest(op)),
            )
            .await
            .unwrap()
            .0;
            assert_eq!(
                response.applied_operation_count, 1,
                "{action}: {:?}",
                response.results
            );
            let recorded: String =
                sqlx::query_scalar("SELECT action FROM canonical_revision_receipts")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(recorded, action);
        }
        let (state, _) = canonical_replay_fixture(false).await;
        let before = canonical_replay_source(&state).await;
        let mut op = source_bound_operation(&state,"no-publish-alias", "DO NOT PROMOTE").await;
        op.action = "publish".into();
        assert_eq!(
            replay_operation_support(&op),
            ReplayOperationSupport::Unsupported
        );
        let response = execute_replay_manifest(
            State(state.clone()),
            auth_user(),
            Json(single_operation_manifest(op)),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(response.unsupported_operation_count, 1);
        assert_eq!(canonical_replay_source(&state).await, before);
    }
    fn asset_create_operation(id: &str, entity: &str) -> SyncReplayOperation {
        replay_operation(
            id,
            "2026-10-08T12:00:00Z",
            "asset",
            entity,
            "create",
            json!({"id":entity,"document_id":"doc-1","space_id":"space-1","name":"Original File.png","mime_type":"image/png","data_url":"data:image/png;base64,QVNTRVQgQllURVM=","source_url":"vault-image"}),
        )
    }
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn reviewed_transfer_sidecar_binds_asset_replay_without_mutating_original_payload() {
        let (state, _) = canonical_replay_fixture(false).await;
        let asset_id = "asset-import-11111111-1111-4111-8111-111111111111";
        let unreferenced_asset_id = "asset-import-22222222-2222-4222-8222-222222222222";
        let mut operation = asset_create_operation("syncop-bound-transfer", asset_id);
        operation.payload["space_id"] = json!("local-space");
        operation.payload["document_id"] = json!("local-document");
        operation.payload["draft_id"] = json!("local-draft");
        operation.payload["local_draft_source"] = json!({
            "schema_version": 1,
            "space_id": "local-space",
            "document_id": "local-document",
            "draft_id": "local-draft",
            "draft_created_at": "local-created"
        });
        let mut unreferenced_operation =
            asset_create_operation("syncop-bound-unreferenced", unreferenced_asset_id);
        unreferenced_operation.payload["space_id"] = json!("local-space");
        unreferenced_operation.payload["document_id"] = json!("local-document");
        unreferenced_operation.payload["draft_id"] = json!("local-draft");
        unreferenced_operation.payload["data_url"] =
            json!("data:image/png;base64,VU5SRUZFUkVOQ0VE");
        unreferenced_operation.payload["local_draft_source"] =
            operation.payload["local_draft_source"].clone();
        let original_payload = operation.payload.clone();
        let fingerprint = replay_body_incoming("actual-provider", &operation)
            .unwrap()
            .payload_fingerprint;
        let unreferenced_fingerprint =
            replay_body_incoming("actual-provider", &unreferenced_operation)
                .unwrap()
                .payload_fingerprint;
        let reviewed = crate::spaces::draft_transfer::ReviewedDraft {
            title: "Reviewed local draft".into(),
            icon: None,
            cover_image: None,
            content: format!("Reviewed body with [original pin](starlane-attachment://{asset_id})"),
        };
        let snapshot_digest = crate::spaces::draft_save::digest(&json!({
            "title": reviewed.title,
            "icon": reviewed.icon,
            "cover_image": reviewed.cover_image,
            "content": reviewed.content,
        }))
        .unwrap();
        let local_source = crate::spaces::draft_transfer::LocalSource {
            provider: "native-local".into(),
            root_identity: "root-a".into(),
            space_id: "local-space".into(),
            document_id: "local-document".into(),
            draft_id: "local-draft".into(),
            draft_created_at: "local-created".into(),
            snapshot_digest,
        };
        let page = crate::spaces::published_source::selected(
            &mut state.db.acquire().await.unwrap(),
            "space-1",
            "doc-1",
        )
        .await
        .unwrap();
        let published_bytes = crate::storage::canonical_rewrite::published_source_bytes(
            page["content_path"].as_str().unwrap(),
        )
        .unwrap();
        let transfer = crate::spaces::draft_transfer::create(
            &state,
            &auth_user(),
            "space-1",
            "doc-1",
            crate::spaces::draft_transfer::Request {
                schema_version: 1,
                operation_id: "22222222-2222-4222-8222-222222222222".into(),
                local_source: local_source.clone(),
                server_source: crate::spaces::draft_transfer::ServerSource::PublishedPage {
                    page_updated_at: page["updated_at"].as_str().unwrap().into(),
                    source_digest: crate::spaces::published_source::digest(
                        &page,
                        &published_bytes,
                    )
                    .unwrap(),
                },
                reviewed_draft: reviewed,
                selected_operation_fingerprints: vec![
                    fingerprint.clone(),
                    unreferenced_fingerprint.clone(),
                ],
            },
        )
        .await
        .unwrap();
        let binding = SyncReplayDraftBinding {
            operation_id: operation.id.clone(),
            operation_fingerprint: fingerprint,
            transfer_operation_id: transfer.operation_id.clone(),
            transfer_request_fingerprint: transfer.request_fingerprint.clone(),
            local_source: local_source.clone(),
            target_space_id: transfer.server_space_id.clone(),
            target_document_id: transfer.server_document_id.clone(),
            target_draft_id: transfer.target_draft_id.clone(),
            target_draft_created_at: transfer.target_draft_created_at.clone(),
            target_source_digest: transfer.target_source_digest.clone(),
        };
        let unreferenced_binding = SyncReplayDraftBinding {
            operation_id: unreferenced_operation.id.clone(),
            operation_fingerprint: unreferenced_fingerprint,
            transfer_operation_id: transfer.operation_id.clone(),
            transfer_request_fingerprint: transfer.request_fingerprint.clone(),
            local_source: local_source.clone(),
            target_space_id: transfer.server_space_id.clone(),
            target_document_id: transfer.server_document_id.clone(),
            target_draft_id: transfer.target_draft_id.clone(),
            target_draft_created_at: transfer.target_draft_created_at.clone(),
            target_source_digest: transfer.target_source_digest.clone(),
        };
        let mut manifest = single_operation_manifest(operation.clone());
        manifest.provider = "actual-provider".into();
        manifest.operation_count = 2;
        manifest.operations.push(unreferenced_operation.clone());
        manifest.draft_bindings = Some(SyncReplayDraftBindingSidecar {
            schema_version: 1,
            binding_count: 2,
            bindings: vec![binding.clone(), unreferenced_binding.clone()],
        });

        let first = execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest.clone()))
            .await
            .unwrap()
            .0;
        assert_eq!(first.applied_operation_count, 2);
        assert_eq!(operation.payload, original_payload);
        let target = crate::spaces::draft_save::selected(
            &mut state.db.acquire().await.unwrap(),
            "space-1",
            "doc-1",
            &transfer.target_draft_id,
        )
        .await
        .unwrap();
        assert_eq!(
            target["content"].as_str().unwrap(),
            format!("Reviewed body with [original pin](starlane-attachment://{asset_id})")
        );
        let mut redirected = manifest.clone();
        let redirected_draft = uuid::Uuid::new_v4().to_string();
        for redirected_binding in &mut redirected.draft_bindings.as_mut().unwrap().bindings {
            redirected_binding.target_draft_id = redirected_draft.clone();
        }
        let rejected = execute_replay_manifest(
            State(state.clone()),
            auth_user(),
            Json(redirected),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(rejected.failed_operation_count, 2);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM attachments")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            crate::spaces::draft_save::selected(
                &mut state.db.acquire().await.unwrap(),
                "space-1",
                "doc-1",
                &transfer.target_draft_id,
            )
            .await
            .unwrap(),
            target
        );
        let raw: String = sqlx::query_scalar(
            "SELECT receipt_json FROM asset_replay_receipts WHERE provider='actual-provider' AND incoming_id=$1",
        )
        .bind(&operation.id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        let receipt: AssetReplayReceipt = serde_json::from_str(&raw).unwrap();
        assert_eq!(receipt.draft_binding.as_ref(), Some(&binding));
        assert_eq!(receipt.space_id, "space-1");
        assert_eq!(receipt.document_id, "doc-1");
        assert_eq!(receipt.draft_scope.unwrap().draft_id, transfer.target_draft_id);
        let unreferenced_raw: String = sqlx::query_scalar(
            "SELECT receipt_json FROM asset_replay_receipts WHERE provider='actual-provider' AND incoming_id=$1",
        )
        .bind(&unreferenced_operation.id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        let unreferenced_receipt: AssetReplayReceipt =
            serde_json::from_str(&unreferenced_raw).unwrap();
        assert_eq!(
            unreferenced_receipt.draft_binding.as_ref(),
            Some(&unreferenced_binding)
        );
        let proof = crate::spaces::asset_audiences::read(
            &mut state.db.acquire().await.unwrap(),
            &auth_user(),
            "space-1",
            "doc-1",
            asset_id,
        )
        .await
        .unwrap();
        assert_eq!(proof.expected_binary().map(|pair| pair.0), Some(11));
        let unreferenced_proof = crate::spaces::asset_audiences::read(
            &mut state.db.acquire().await.unwrap(),
            &auth_user(),
            "space-1",
            "doc-1",
            unreferenced_asset_id,
        )
        .await
        .unwrap();
        assert_eq!(
            unreferenced_proof.expected_binary().map(|pair| pair.0),
            Some(12)
        );
        let prepared = crate::spaces::asset_audiences::prepare_publication(
            &mut state.db.acquire().await.unwrap(),
            &auth_user(),
            "space-1",
            "doc-1",
            &transfer.target_draft_id,
            target["content"].as_str().unwrap(),
            None,
        )
        .await
        .unwrap();
        assert_eq!(prepared, std::collections::BTreeSet::from([asset_id.into()]));

        let retry = execute_replay_manifest(
            State(state.clone()),
            auth_user(),
            Json(manifest.clone()),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(retry.skipped_operation_count, 2);

        let mut unbound_manifest = manifest.clone();
        unbound_manifest.draft_bindings = None;
        let unbound = execute_replay_manifest(
            State(state.clone()),
            auth_user(),
            Json(unbound_manifest),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(unbound.failed_operation_count, 2);

        let draft_path = format!(
            "/spaces/space-1/documents/doc-1/drafts/{}",
            transfer.target_draft_id
        );
        let (status, publish_source) = authenticated_json(
            &state,
            "GET",
            &format!("{draft_path}/publish-source"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{publish_source}");
        let publish_operation_id = uuid::Uuid::new_v4().to_string();
        let publish_request = json!({
            "schema_version": 1,
            "operation_id": publish_operation_id,
            "draft_source_digest": publish_source["draft_source_digest"],
            "page_source_digest": publish_source["page_source_digest"],
        });
        let (status, published) = authenticated_json(
            &state,
            "POST",
            &format!("{draft_path}/publish"),
            Some(publish_request),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{published}");
        let reviewed_snapshot = format!("{publish_operation_id}:publication");
        let published_referenced: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attachment_audience_bindings WHERE asset_id=$1 AND binding_kind='publication' AND target_id=$2",
        )
        .bind(asset_id)
        .bind(&reviewed_snapshot)
        .fetch_one(&state.db)
        .await
        .unwrap();
        let published_unreferenced: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attachment_audience_bindings WHERE asset_id=$1 AND binding_kind='publication'",
        )
        .bind(unreferenced_asset_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(published_referenced, 1);
        assert_eq!(published_unreferenced, 0);

        let current_base: String =
            sqlx::query_scalar("SELECT updated_at FROM documents WHERE id='doc-1'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        sqlx::query(
            "INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('later-publication','doc-1','space-1','user-1','Later publication','Later public body',$1)",
        )
        .bind(current_base)
        .execute(&state.db)
        .await
        .unwrap();
        let later_path = "/spaces/space-1/documents/doc-1/drafts/later-publication";
        let (status, later_source) = authenticated_json(
            &state,
            "GET",
            &format!("{later_path}/publish-source"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{later_source}");
        let (status, later_published) = authenticated_json(
            &state,
            "POST",
            &format!("{later_path}/publish"),
            Some(json!({
                "schema_version": 1,
                "operation_id": uuid::Uuid::new_v4().to_string(),
                "draft_source_digest": later_source["draft_source_digest"],
                "page_source_digest": later_source["page_source_digest"],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{later_published}");

        let recovery_path = format!(
            "/spaces/space-1/documents/doc-1/publication-snapshots/{reviewed_snapshot}"
        );
        let (status, recovery_source) = authenticated_json(
            &state,
            "GET",
            &format!("{recovery_path}/recovery-source"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{recovery_source}");
        let (status, recovered) = authenticated_json(
            &state,
            "POST",
            &format!("{recovery_path}/recover"),
            Some(json!({
                "schema_version": 1,
                "operation_id": uuid::Uuid::new_v4().to_string(),
                "snapshot_source_digest": recovery_source["snapshot_source_digest"],
                "page_source_digest": recovery_source["page_source_digest"],
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{recovered}");
        let recovered_draft_id = recovered["draft"]["id"].as_str().unwrap();
        let recovered_draft = crate::spaces::draft_save::selected(
            &mut state.db.acquire().await.unwrap(),
            "space-1",
            "doc-1",
            recovered_draft_id,
        )
        .await
        .unwrap();
        assert_eq!(
            recovered_draft["content"].as_str().unwrap(),
            format!("Reviewed body with [original pin](starlane-attachment://{asset_id})")
        );
        let recovered_referenced: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attachment_audience_bindings WHERE asset_id=$1 AND binding_kind='draft' AND target_id=$2",
        )
        .bind(asset_id)
        .bind(recovered_draft_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        let recovered_unreferenced: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM attachment_audience_bindings WHERE asset_id=$1 AND binding_kind='draft' AND target_id=$2",
        )
        .bind(unreferenced_asset_id)
        .bind(recovered_draft_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(recovered_referenced, 1);
        assert_eq!(recovered_unreferenced, 0);
        let mut unbound_opaque = asset_create_operation(
            "syncop-unbound-opaque",
            "asset-import-44444444-4444-4444-8444-444444444444",
        );
        unbound_opaque.payload["draft_id"] = json!(transfer.target_draft_id);
        assert!(
            super::replay_asset_create(
                &state,
                &auth_user(),
                "actual-provider",
                &unbound_opaque,
            )
            .await
            .is_err(),
            "unbound draft replay must retain its canonical UUID contract"
        );
    }
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn restoration_asset_classifier_real_producer_current_history_and_bytes() {
        let (state, _) = canonical_replay_fixture(false).await;
        let op = asset_create_operation("classified-asset", "classified-asset");
        super::replay_asset_create(&state, &auth_user(), "actual-provider", &op)
            .await
            .unwrap();
        let root = crate::storage::data_dir();
        let mut conn = state.db.acquire().await.unwrap();
        let changes_before: i64 = sqlx::query_scalar("SELECT total_changes()")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        let rows = super::restoration_assets::classify_at(&root, &mut conn)
            .await
            .unwrap();
        let changes_after: i64 = sqlx::query_scalar("SELECT total_changes()")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(changes_before, changes_after);
        let linked_root =
            std::env::temp_dir().join(format!("classified-root-link-{}", uuid::Uuid::new_v4()));
        std::os::unix::fs::symlink(&root, &linked_root).unwrap();
        assert!(
            super::restoration_assets::classify_at(&linked_root, &mut conn)
                .await
                .is_err()
        );
        std::fs::remove_file(linked_root).unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].current_attachment);
        assert!(!rows[0].requires_canonical_evidence);
        assert_eq!(rows[0].receipt.incoming.operation_id, op.id);
        let path = root.join(&rows[0].receipt.storage_path);
        let original = std::fs::read(&path).unwrap();
        assert_eq!(original, b"ASSET BYTES");
        let raw = rows[0].raw_receipt.clone();
        let before: Vec<(String, String)> = sqlx::query_as("SELECT id,role FROM users ORDER BY id")
            .fetch_all(&mut *conn)
            .await
            .unwrap();
        for bytes in [
            b"X".as_slice(),
            b"ASSET BYTEZ".as_slice(),
            b"ASSET BYTES EXTRA".as_slice(),
        ] {
            std::fs::write(&path, bytes).unwrap();
            assert!(super::restoration_assets::classify_at(&root, &mut conn)
                .await
                .is_err());
        }
        std::fs::write(&path, &original).unwrap();
        let missing = path.with_extension("held");
        std::fs::rename(&path, &missing).unwrap();
        assert!(super::restoration_assets::classify_at(&root, &mut conn)
            .await
            .is_err());
        std::os::unix::fs::symlink(&missing, &path).unwrap();
        assert!(super::restoration_assets::classify_at(&root, &mut conn)
            .await
            .is_err());
        std::fs::remove_file(&path).unwrap();
        std::fs::rename(&missing, &path).unwrap();
        for column in [
            "provider",
            "incoming_id",
            "actor_id",
            "entity_type",
            "entity_id",
            "action",
            "payload_fingerprint",
            "space_id",
            "document_id",
            "storage_path",
            "filename",
            "original_name",
            "mime_type",
            "binary_sha256",
            "source_kind",
            "created_at",
        ] {
            sqlx::query(&format!(
                "UPDATE asset_replay_receipts SET {column}='changed'"
            ))
            .execute(&mut *conn)
            .await
            .unwrap();
            assert!(
                super::restoration_assets::classify_at(&root, &mut conn)
                    .await
                    .is_err(),
                "{column}"
            );
            let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let restore = match column {
                "provider" => v["incoming"]["provider"].as_str().unwrap(),
                "incoming_id" => v["incoming"]["operation_id"].as_str().unwrap(),
                "entity_type" => v["incoming"]["entity_type"].as_str().unwrap(),
                "entity_id" => v["incoming"]["entity_id"].as_str().unwrap(),
                "action" => v["incoming"]["action"].as_str().unwrap(),
                "payload_fingerprint" => v["incoming"]["payload_fingerprint"].as_str().unwrap(),
                _ => v[column].as_str().unwrap(),
            };
            sqlx::query(&format!("UPDATE asset_replay_receipts SET {column}=$1"))
                .bind(restore)
                .execute(&mut *conn)
                .await
                .unwrap();
        }
        for column in ["source_id", "source_revision", "body_sha256"] {
            sqlx::query(&format!(
                "UPDATE asset_replay_receipts SET {column}='changed'"
            ))
            .execute(&mut *conn)
            .await
            .unwrap();
            assert!(
                super::restoration_assets::classify_at(&root, &mut conn)
                    .await
                    .is_err(),
                "{column}"
            );
            sqlx::query(&format!("UPDATE asset_replay_receipts SET {column}=NULL"))
                .execute(&mut *conn)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE asset_replay_receipts SET byte_length=999")
            .execute(&mut *conn)
            .await
            .unwrap();
        assert!(super::restoration_assets::classify_at(&root, &mut conn)
            .await
            .is_err());
        sqlx::query("UPDATE asset_replay_receipts SET byte_length=$1")
            .bind(original.len() as i64)
            .execute(&mut *conn)
            .await
            .unwrap();
        for changed in [
            raw.replacen("{", "{\"unknown\":1,", 1),
            raw.replacen("\"incoming\":{", "\"incoming\":{\"unknown\":1,", 1),
            raw.replacen(
                "\"actor_id\":",
                "\"actor_id\":\"duplicate\",\"actor_id\":",
                1,
            ),
        ] {
            sqlx::query("UPDATE asset_replay_receipts SET receipt_json=$1")
                .bind(changed)
                .execute(&mut *conn)
                .await
                .unwrap();
            assert!(super::restoration_assets::classify_at(&root, &mut conn)
                .await
                .is_err());
        }
        sqlx::query("UPDATE asset_replay_receipts SET receipt_json=$1")
            .bind(&raw)
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("DELETE FROM attachments")
            .execute(&mut *conn)
            .await
            .unwrap();
        std::fs::remove_file(&path).unwrap();
        let rows = super::restoration_assets::classify_at(&root, &mut conn)
            .await
            .unwrap();
        assert!(!rows[0].current_attachment);
        assert_eq!(rows[0].raw_receipt, raw);
        let after: Vec<(String, String)> = sqlx::query_as("SELECT id,role FROM users ORDER BY id")
            .fetch_all(&mut *conn)
            .await
            .unwrap();
        assert_eq!(before, after);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn restoration_asset_classifier_real_published_source_dependency() {
        let (state, old) = canonical_replay_fixture(false).await;
        tokio::fs::write(
            crate::storage::data_dir().join(old),
            "![image](vault-image)",
        )
        .await
        .unwrap();
        let op = asset_create_operation("classified-published", "classified-published");
        super::replay_asset_create(&state, &auth_user(), "actual-provider", &op)
            .await
            .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let rows = super::restoration_assets::classify_at(&crate::storage::data_dir(), &mut conn)
            .await
            .unwrap();
        assert!(rows[0].current_attachment);
        assert!(rows[0].requires_canonical_evidence);
        assert_eq!(rows[0].receipt.source_kind, "published");
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM canonical_revision_receipts")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(n, 1);
        let op_id: String = sqlx::query_scalar(
            "SELECT op_id FROM canonical_revision_receipts WHERE provider=$1 AND incoming_id=$2",
        )
        .bind("actual-provider")
        .bind(&op.id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        let root = crate::storage::data_dir();
        let inspected =
            crate::storage::canonical_rewrite::inspect_restoration(&root, &mut conn, &op_id)
                .await
                .unwrap();
        assert!(inspected.matches_asset_receipt(&rows[0].receipt));
        for field in [
            "provider",
            "incoming_id",
            "entity_type",
            "entity_id",
            "action",
            "fingerprint",
            "actor",
            "space",
            "document",
            "source_kind",
            "source_id",
            "source_revision",
            "body_sha",
        ] {
            let mut changed = rows[0].receipt.clone();
            match field {
                "provider" => changed.incoming.provider = "changed".into(),
                "incoming_id" => changed.incoming.operation_id = "changed".into(),
                "entity_type" => changed.incoming.entity_type = "changed".into(),
                "entity_id" => changed.incoming.entity_id = "changed".into(),
                "action" => changed.incoming.action = "changed".into(),
                "fingerprint" => changed.incoming.payload_fingerprint = "0".repeat(64),
                "actor" => changed.actor_id = "changed".into(),
                "space" => changed.space_id = "changed".into(),
                "document" => changed.document_id = "changed".into(),
                "source_kind" => changed.source_kind = "unchanged".into(),
                "source_id" => changed.source_id = Some("changed".into()),
                "source_revision" => changed.source_revision = Some("changed".into()),
                "body_sha" => changed.body_sha256 = Some("0".repeat(64)),
                _ => unreachable!(),
            }
            assert!(!inspected.matches_asset_receipt(&changed), "{field}");
        }
        sqlx::query("UPDATE documents SET updated_at='legitimate-later-revision'")
            .execute(&mut *conn)
            .await
            .unwrap();
        let historical =
            crate::storage::canonical_rewrite::inspect_restoration(&root, &mut conn, &op_id)
                .await
                .unwrap();
        assert!(historical.matches_asset_receipt(&rows[0].receipt));
        let target: String = sqlx::query_scalar(
            "SELECT content_path FROM canonical_revision_receipts WHERE op_id=$1",
        )
        .bind(&op_id)
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        std::fs::remove_file(root.join(target)).unwrap();
        assert!(
            crate::storage::canonical_rewrite::inspect_restoration(&root, &mut conn, &op_id)
                .await
                .is_err()
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn restoration_asset_classifier_real_draft_source_and_metadata_history() {
        let (state, _) = canonical_replay_fixture(false).await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='doc-1'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES('classified-draft','doc-1','space-1','user-1','A','A vault-image','base','same')").execute(&state.db).await.unwrap();
        let op = asset_create_operation("classified-draft-asset", "classified-draft-asset");
        super::replay_asset_create(&state, &auth_user(), "actual-provider", &op)
            .await
            .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let root = crate::storage::data_dir();
        let rows = super::restoration_assets::classify_at(&root, &mut conn)
            .await
            .unwrap();
        assert!(rows[0].current_attachment);
        assert!(!rows[0].requires_canonical_evidence);
        assert_eq!(rows[0].receipt.source_kind, "draft");
        assert_eq!(
            rows[0].receipt.source_id.as_deref(),
            Some("classified-draft")
        );
        sqlx::query("UPDATE attachments SET original_name='Renamed after upload'")
            .execute(&mut *conn)
            .await
            .unwrap();
        let rows = super::restoration_assets::classify_at(&root, &mut conn)
            .await
            .unwrap();
        assert!(!rows[0].current_attachment);
        sqlx::query("UPDATE attachments SET original_name='Original File.png'")
            .execute(&mut *conn)
            .await
            .unwrap();
        sqlx::query("UPDATE spaces SET slug='different-current-slug' WHERE id='space-1'")
            .execute(&mut *conn)
            .await
            .unwrap();
        assert!(super::restoration_assets::classify_at(&root, &mut conn)
            .await
            .is_err());
    }
    #[tokio::test]
    async fn asset_replay_published_reference_and_receipt_are_atomic_raw_and_historical() {
        let (mut state, old) = canonical_replay_fixture(false).await;
        let asset_index =
            std::env::temp_dir().join(format!("asset-index-{}", uuid::Uuid::new_v4()));
        state.search_index = crate::search::SearchIndex::new(asset_index.clone()).unwrap();
        let raw = "---\r\ncustom: vault-image\r\n---\r\n![raw](vault-image)\r\n";
        tokio::fs::write(crate::storage::data_dir().join(&old), raw)
            .await
            .unwrap();
        let op = asset_create_operation(" legacy:asset op ", "raw:asset/id");
        super::replay_asset_create(&state, &auth_user(), "actual-provider", &op)
            .await
            .unwrap();
        let association: (String, String, String, i64) = sqlx::query_as(
            "SELECT storage_path,filename,original_name,size_bytes FROM attachments",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(association.2, "Original File.png");
        assert_eq!(association.3, 11);
        assert!(uuid::Uuid::parse_str(association.1.trim_end_matches(".png")).is_ok());
        assert_eq!(
            tokio::fs::read(crate::storage::data_dir().join(&association.0))
                .await
                .unwrap(),
            b"ASSET BYTES"
        );
        let new = canonical_replay_source(&state).await;
        assert_ne!(old, new.0);
        assert_eq!(new.2.as_deref(), Some("source-publication"));
        let content = tokio::fs::read_to_string(crate::storage::data_dir().join(&new.0))
            .await
            .unwrap();
        assert_eq!(
            content,
            raw.replace("vault-image", &format!("/api/v1/files/{}", association.0))
        );
        assert_eq!(
            tokio::fs::read_to_string(crate::storage::data_dir().join(&old))
                .await
                .unwrap(),
            raw
        );
        assert_eq!(
            tantivy::Index::open_in_dir(&asset_index)
                .unwrap()
                .reader()
                .unwrap()
                .searcher()
                .num_docs(),
            1
        );
        let receipt: (String, String, String) =
            sqlx::query_as("SELECT provider,incoming_id,entity_id FROM asset_replay_receipts")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(
            receipt,
            (
                "actual-provider".into(),
                op.id.clone(),
                "raw:asset/id".into()
            )
        );
        let later = source_bound_operation(&state,"later-after-asset", "LATER BODY").await;
        super::replay_document_update_content(&state, &auth_user(), "actual-provider", &later)
            .await
            .unwrap();
        let latest = canonical_replay_source(&state).await;
        super::replay_asset_create(&state, &auth_user(), "actual-provider", &op)
            .await
            .unwrap();
        assert_eq!(canonical_replay_source(&state).await, latest);
        let mut collision = op;
        collision.payload["data_url"] = json!("data:image/png;base64,Q09MTElTSU9O");
        assert!(
            super::replay_asset_create(&state, &auth_user(), "actual-provider", &collision)
                .await
                .is_err()
        );
        assert_eq!(canonical_replay_source(&state).await, latest);
    }
    #[cfg(target_os="linux")]
    #[tokio::test]
    async fn asset_replay_v2_keeps_exact_draft_origin_and_receipt_without_body_cas() {
        let (state,old)=canonical_replay_fixture(false).await;
        let draft="11111111-1111-4111-8111-111111111111";
        let asset="33333333-3333-4333-8333-333333333333";
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at) VALUES($1,'doc-1','space-1','user-1','Private','Original private body','source-revision','original-draft-created')").bind(draft).execute(&state.db).await.unwrap();
        let mut op=asset_create_operation("scoped-replay-op",asset);op.payload["draft_id"]=json!(draft);
        super::replay_asset_create(&state,&auth_user(),"actual-provider",&op).await.unwrap();
        let raw:String=sqlx::query_scalar("SELECT receipt_json FROM asset_replay_receipts WHERE incoming_id='scoped-replay-op'").fetch_one(&state.db).await.unwrap();
        let receipt:super::AssetReplayReceipt=serde_json::from_str(&raw).unwrap();
        let scope=receipt.draft_scope.as_ref().unwrap();assert_eq!(scope.draft_id,draft);assert_eq!(scope.draft_created_at,"original-draft-created");
        assert_eq!(receipt.source_kind,"unchanged");assert_eq!(serde_json::to_string(&receipt).unwrap(),raw);
        let origin:(String,String,String,String)=sqlx::query_as("SELECT kind,draft_id,draft_created_at,operation_id FROM attachment_audience_origins WHERE asset_id=$1").bind(asset).fetch_one(&state.db).await.unwrap();
        assert_eq!(origin,("draft_replay_v2".into(),draft.into(),"original-draft-created".into(),"scoped-replay-op".into()));
        let mut conn=state.db.acquire().await.unwrap();crate::spaces::asset_audiences::read(&mut conn,&auth_user(),"space-1","doc-1",asset).await.unwrap();drop(conn);
        // Typing does not invalidate a completed upload's original scope/receipt.
        sqlx::query("UPDATE document_drafts SET content='Later private typing',updated_at='later-body' WHERE id=$1").bind(draft).execute(&state.db).await.unwrap();
        super::replay_asset_create(&state,&auth_user(),"actual-provider",&op).await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT content FROM document_drafts WHERE id=$1").bind(draft).fetch_one(&state.db).await.unwrap(),"Later private typing");
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT receipt_json FROM asset_replay_receipts WHERE incoming_id='scoped-replay-op'").fetch_one(&state.db).await.unwrap(),raw);
        assert_eq!(tokio::fs::read(crate::storage::data_dir().join(old)).await.unwrap(),b"PUBLIC OLD RAW\r\n");
        sqlx::query("INSERT INTO users(id,email,display_name) VALUES('another-actor','lock-owner@example.invalid','Other lock owner')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET locked=1,locked_by='another-actor' WHERE id='doc-1'").execute(&state.db).await.unwrap();
        assert!(super::replay_asset_create(&state,&auth_user(),"actual-provider",&op).await.is_err(),"v2 receipt acknowledgement still requires current exact draft edit authority");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachments WHERE id=$1").bind(asset).fetch_one(&state.db).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT receipt_json FROM asset_replay_receipts WHERE incoming_id='scoped-replay-op'").fetch_one(&state.db).await.unwrap(),raw);
    }
    #[cfg(target_os="linux")]
    #[tokio::test]
    async fn asset_replay_v2_scoped_http_download_refuses_same_length_source_tamper() {
        use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
        use tower::ServiceExt;
        let (state,_)=canonical_replay_fixture(false).await;
        let draft="77777777-7777-4777-8777-777777777777";let asset="88888888-8888-4888-8888-888888888888";
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at) VALUES($1,'doc-1','space-1','user-1','Private','PRIVATE SOURCE','source-revision','original-created')")
            .bind(draft).execute(&state.db).await.unwrap();
        let mut upload=asset_create_operation("scoped-http-original-binary",asset);upload.payload["draft_id"]=json!(draft);
        super::replay_asset_create(&state,&auth_user(),"actual-provider",&upload).await.unwrap();
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_version_records WHERE version_id=$1").bind(asset).fetch_one(&state.db).await.unwrap(),0,"Replay must not acquire invented immutable version history");
        let path:String=sqlx::query_scalar("SELECT storage_path FROM attachments WHERE id=$1").bind(asset).fetch_one(&state.db).await.unwrap();
        let full=crate::storage::data_dir().join(&path);let original=std::fs::read(&full).unwrap();assert!(!original.is_empty());
        let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&auth_user().claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();
        let app=crate::spaces::attachments::router().with_state(state.clone());
        let request=||Request::builder().uri(format!("/files/{path}")).header("Authorization",format!("Bearer {token}")).body(Body::empty()).unwrap();
        let response=app.clone().oneshot(request()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
        assert_eq!(to_bytes(response.into_body(),original.len()+1).await.unwrap().as_ref(),original);
        let review_path=format!("/spaces/space-1/documents/doc-1/attachment-copy-source/{asset}");
        let review_request=||Request::builder().uri(&review_path).header("Authorization",format!("Bearer {token}")).body(Body::empty()).unwrap();
        let reviewed=app.clone().oneshot(review_request()).await.unwrap();assert_eq!(reviewed.status(),StatusCode::OK);
        let reviewed:Value=serde_json::from_slice(&to_bytes(reviewed.into_body(),65536).await.unwrap()).unwrap();
        assert_eq!(reviewed["asset_id"],asset);
        let mut changed=original.clone();changed[0]^=1;std::fs::write(&full,&changed).unwrap();
        let response=app.clone().oneshot(request()).await.unwrap();
        assert_eq!(response.status(),StatusCode::NOT_FOUND,"Scoped Replay's original receipt hash must fence actual served bytes even without VersionRecord");
        assert_eq!(app.clone().oneshot(review_request()).await.unwrap().status(),StatusCode::NOT_FOUND,"Unregistered review must honor scoped Replay's original bytes");
        let copy=json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"source_document_id":"doc-1","version_id":asset,"expected_legacy_source":reviewed});
        let response=app.oneshot(Request::builder().method("POST").uri(format!("/spaces/space-1/documents/doc-1/drafts/{draft}/attachments/copy")).header("Authorization",format!("Bearer {token}")).header("Content-Type","application/json").body(Body::from(serde_json::to_vec(&copy).unwrap())).unwrap()).await.unwrap();
        assert_eq!(response.status(),StatusCode::NOT_FOUND,"Original reviewed copy must not adopt tainted Replay bytes");
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachments").fetch_one(&state.db).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_upload_receipts").fetch_one(&state.db).await.unwrap(),0);
        assert_eq!(std::fs::read(full).unwrap(),changed,"A rejected read must never rewrite or repair original source bytes");
    }
    #[cfg(target_os="linux")]
    #[tokio::test]
    async fn asset_replay_v2_late_draft_incarnation_change_rolls_back_all_audience_rows() {
        let (state,old)=canonical_replay_fixture(false).await;
        let draft="11111111-1111-4111-8111-111111111111";
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at) VALUES($1,'doc-1','space-1','user-1','Private','vault-image original','source-revision','original-created')").bind(draft).execute(&state.db).await.unwrap();
        let before=canonical_replay_source(&state).await;
        sqlx::query("CREATE TRIGGER changed_draft_identity AFTER INSERT ON asset_replay_receipts BEGIN UPDATE document_drafts SET created_at='substituted-incarnation'; END").execute(&state.db).await.unwrap();
        let mut op=asset_create_operation("late-scope-op","44444444-4444-4444-8444-444444444444");op.payload["draft_id"]=json!(draft);
        assert!(super::replay_asset_create(&state,&auth_user(),"actual-provider",&op).await.is_err());
        assert_eq!(canonical_replay_source(&state).await,before);
        for table in ["attachments","asset_replay_receipts","attachment_audience_origins","attachment_audience_bindings"] {assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT count(*) FROM {table}")).fetch_one(&state.db).await.unwrap(),0,"{table}");}
        assert_eq!(sqlx::query_as::<_,(String,String)>("SELECT content,created_at FROM document_drafts WHERE id=$1").bind(draft).fetch_one(&state.db).await.unwrap(),("vault-image original".into(),"original-created".into()));
        assert_eq!(tokio::fs::read(crate::storage::data_dir().join(old)).await.unwrap(),b"PUBLIC OLD RAW\r\n");
    }
    #[tokio::test]
    async fn asset_replay_no_reference_upload_has_receipt_without_canonical_transition() {
        let (state, _) = canonical_replay_fixture(false).await;
        let before = canonical_replay_source(&state).await;
        let op = asset_create_operation("pool-upload", "pool-asset");
        super::replay_asset_create(&state, &auth_user(), "self-hosted", &op)
            .await
            .unwrap();
        assert_eq!(canonical_replay_source(&state).await, before);
        let kinds: String = sqlx::query_scalar("SELECT source_kind FROM asset_replay_receipts")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(kinds, "unchanged");
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM canonical_revision_receipts")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let retry = asset_create_operation("unproven-new-operation", "pool-asset");
        assert!(
            super::replay_asset_create(&state, &auth_user(), "self-hosted", &retry)
                .await
                .unwrap_err()
                .contains("no exact committed")
        );
    }
    #[tokio::test]
    async fn asset_replay_late_receipt_policy_association_and_source_failures_roll_back_together() {
        for statement in ["DELETE FROM sessions WHERE id='test-session'","UPDATE users SET is_active=0 WHERE id='user-1'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','user-1','user-1')","UPDATE documents SET locked=1,locked_by='other-user' WHERE id='doc-1'","UPDATE documents SET published_at=NULL WHERE id='doc-1'","UPDATE documents SET updated_at='late' WHERE id='doc-1'","UPDATE attachments SET original_name='changed'","UPDATE attachments SET created_at='changed'","UPDATE asset_replay_receipts SET binary_sha256='changed'"] {
            let (state,old)=canonical_replay_fixture(false).await;tokio::fs::write(crate::storage::data_dir().join(&old),b"vault-image OLD").await.unwrap();let before=canonical_replay_source(&state).await;
            sqlx::query(&format!("CREATE TRIGGER late_asset AFTER INSERT ON asset_replay_receipts BEGIN {statement}; END")).execute(&state.db).await.unwrap();
            assert!(super::replay_asset_create(&state,&auth_user(),"self-hosted",&asset_create_operation("late-asset","asset")).await.is_err(),"{statement}");
            assert_eq!(canonical_replay_source(&state).await,before);
            for table in ["attachments","asset_replay_receipts","canonical_revision_receipts"] {let n:i64=sqlx::query_scalar(&format!("SELECT count(*) FROM {table}")).fetch_one(&state.db).await.unwrap();assert_eq!(n,0,"{statement} {table}");}
            assert_eq!(tokio::fs::read(crate::storage::data_dir().join(old)).await.unwrap(),b"vault-image OLD");
        }
    }
    #[tokio::test]
    async fn asset_replay_private_draft_selection_and_exact_share_rollback_never_touch_shell() {
        let (state, old) = canonical_replay_fixture(false).await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='doc-1'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES('11111111-1111-4111-8111-111111111111','doc-1','space-1','user-1','A','A vault-image','base','same'),('z-owned','doc-1','space-1','user-1','Z','Z vault-image','base','same')").execute(&state.db).await.unwrap();
        super::replay_asset_create(
            &state,
            &auth_user(),
            "self-hosted",
            &asset_create_operation("own-asset", "own-asset"),
        )
        .await
        .unwrap();
        let drafts: Vec<(String, String)> =
            sqlx::query_as("SELECT id,content FROM document_drafts ORDER BY id")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(drafts[0].1, "A vault-image");
        assert!(drafts[1].1.starts_with("Z /api/v1/files/"));
        sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES('share-user','share@test','Share','member')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('share-session','share-user','share','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('share-member','space-1','share-user','editor')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('11111111-1111-4111-8111-111111111111','share-user','editor','user-1')").execute(&state.db).await.unwrap();
        let mut shared = auth_user();
        shared.claims.sub = "share-user".into();
        shared.claims.sid = "share-session".into();
        let mut op = asset_create_operation("share-upload", "22222222-2222-4222-8222-222222222222");
        assert!(
            super::replay_asset_create(&state, &shared, "self-hosted", &op)
                .await
                .unwrap_err()
                .contains("exact editable draft_id")
        );
        op.payload["draft_id"] = json!("11111111-1111-4111-8111-111111111111");
        sqlx::query("CREATE TRIGGER revoke_share AFTER INSERT ON asset_replay_receipts BEGIN DELETE FROM document_draft_access WHERE draft_id='11111111-1111-4111-8111-111111111111'; END").execute(&state.db).await.unwrap();
        assert!(
            super::replay_asset_create(&state, &shared, "self-hosted", &op)
                .await
                .is_err()
        );
        sqlx::query("DROP TRIGGER revoke_share")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE document_draft_access SET role='viewer'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(
            super::replay_asset_create(&state, &shared, "self-hosted", &op)
                .await
                .is_err()
        );
        sqlx::query("UPDATE document_draft_access SET role='editor'")
            .execute(&state.db)
            .await
            .unwrap();
        super::replay_asset_create(&state, &shared, "self-hosted", &op)
            .await
            .unwrap();
        assert_eq!(
            tokio::fs::read(crate::storage::data_dir().join(old))
                .await
                .unwrap(),
            b"PUBLIC OLD RAW\r\n"
        );
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM canonical_revision_receipts")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
    #[tokio::test]
    async fn asset_replay_disk_wal_reader_never_sees_association_or_body_before_commit() {
        let (state, old) = canonical_replay_fixture(true).await;
        tokio::fs::write(crate::storage::data_dir().join(&old), b"OLD vault-image")
            .await
            .unwrap();
        let observer = state.clone();
        let previous = old.clone();
        super::REPLAY_ASSET_BEFORE_COMMIT.with(|slot| {
            *slot.borrow_mut() = Some(Box::pin(async move {
                let source = canonical_replay_source(&observer).await;
                assert_eq!(source.0, previous);
                assert_eq!(
                    tokio::fs::read(crate::storage::data_dir().join(source.0))
                        .await
                        .unwrap(),
                    b"OLD vault-image"
                );
                for table in [
                    "attachments",
                    "asset_replay_receipts",
                    "canonical_revision_receipts",
                ] {
                    let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
                        .fetch_one(&observer.db)
                        .await
                        .unwrap();
                    assert_eq!(n, 0);
                }
            }))
        });
        super::replay_asset_create(
            &state,
            &auth_user(),
            "self-hosted",
            &asset_create_operation("wal-asset", "wal-asset"),
        )
        .await
        .unwrap();
        let current = canonical_replay_source(&state).await;
        assert_ne!(current.0, old);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn asset_replay_commit_ack_and_unknown_proof_never_duplicate_blob_or_rewrite() {
        use super::ReplayCanonicalCommitFault;
        for fault in [
            ReplayCanonicalCommitFault::LostAck,
            ReplayCanonicalCommitFault::Rollback,
            ReplayCanonicalCommitFault::UnknownProof,
        ] {
            let (state, old) = canonical_replay_fixture(false).await;
            tokio::fs::write(crate::storage::data_dir().join(&old), b"vault-image BEFORE")
                .await
                .unwrap();
            let before = canonical_replay_source(&state).await;
            let op = asset_create_operation("asset-ack", "asset-ack");
            super::REPLAY_ASSET_COMMIT_FAULT.with(|s| *s.borrow_mut() = Some(fault));
            if fault == ReplayCanonicalCommitFault::UnknownProof {
                let unavailable = state.clone();
                super::REPLAY_ASSET_BEFORE_CONFIRM.with(|s|*s.borrow_mut()=Some(Box::pin(async move {sqlx::query("ALTER TABLE asset_replay_receipts RENAME TO unavailable_asset_receipts").execute(&unavailable.db).await.unwrap();})));
            }
            let result = super::replay_asset_create(&state, &auth_user(), "self-hosted", &op).await;
            match fault {
                ReplayCanonicalCommitFault::LostAck => {
                    result.unwrap();
                }
                ReplayCanonicalCommitFault::Rollback => {
                    assert!(result.unwrap_err().contains("did not apply"));
                    assert_eq!(canonical_replay_source(&state).await, before);
                    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments")
                        .fetch_one(&state.db)
                        .await
                        .unwrap();
                    assert_eq!(n, 0);
                    continue;
                }
                ReplayCanonicalCommitFault::UnknownProof => {
                    assert!(result.unwrap_err().contains("indeterminate"));
                    sqlx::query(
                        "ALTER TABLE unavailable_asset_receipts RENAME TO asset_replay_receipts",
                    )
                    .execute(&state.db)
                    .await
                    .unwrap();
                }
            }
            let committed = canonical_replay_source(&state).await;
            let manifest = single_operation_manifest(op.clone());
            super::insert_replay_ledger(&state, &auth_user(), &manifest, &op, "pending", None)
                .await
                .unwrap();
            let result = execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest))
                .await
                .unwrap()
                .0;
            assert_eq!(result.skipped_operation_count, 1);
            assert_eq!(result.failed_operation_count, 0);
            assert_eq!(canonical_replay_source(&state).await, committed);
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(n, 1);
        }
    }
    #[tokio::test]
    async fn asset_replay_http_current_write_key_read_denial_and_late_downgrade() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;
        let (state, _) = canonical_replay_fixture(false).await;
        let raw = "asset-http-key";
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('asset-http','user-1','HTTP',$1,'prefix','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key(raw)).execute(&state.db).await.unwrap();
        let app = super::api_router().with_state(state.clone());
        let request = |id: &str| {
            let c = contract();
            let op = asset_create_operation(id, id);
            Request::builder().method("POST").uri("/sync/replay").header("X-API-Key",raw).header("content-type","application/json").body(Body::from(json!({"schema_version":1,"provider":"self-hosted","operation_count":1,"replay_contract":{"schema_version":c.schema_version,"operation_order":c.operation_order,"idempotency_key":c.idempotency_key,"transport":c.transport,"server_backends":c.server_backends,"required_payload_status":c.required_payload_status,"conflict_policy":c.conflict_policy,"web_canonical_store":c.web_canonical_store},"operations":[{"id":op.id,"entity_type":op.entity_type,"entity_id":op.entity_id,"action":op.action,"queued_at":op.queued_at,"payload_status":op.payload_status,"payload":op.payload}]}).to_string())).unwrap()
        };
        let response = app.clone().oneshot(request("http-asset")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["applied_operation_count"], 1);
        sqlx::query("CREATE TRIGGER late_asset_key AFTER INSERT ON asset_replay_receipts BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='asset-http'; END").execute(&state.db).await.unwrap();
        let response = app.clone().oneshot(request("http-late-key")).await.unwrap();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["failed_operation_count"], 1);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 1);
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='asset-http'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            app.oneshot(request("http-read-denied"))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    async fn asset_replay_unproven_legacy_skip_and_current_source_scope_never_acknowledge() {
        let (state, _) = canonical_replay_fixture(false).await;
        let op = asset_create_operation("old-skip", "asset-old");
        let manifest = single_operation_manifest(op.clone());
        super::insert_replay_ledger(&state, &auth_user(), &manifest, &op, "skipped", None)
            .await
            .unwrap();
        let result = execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest))
            .await
            .unwrap()
            .0;
        assert_eq!(result.failed_operation_count, 1);
        assert_eq!(result.skipped_operation_count, 0);
        for change in [
            "UPDATE documents SET archived_at='old' WHERE id='doc-1'",
            "UPDATE documents SET deleted_at='old' WHERE id='doc-1'",
            "UPDATE documents SET locked=1,locked_by='other-user' WHERE id='doc-1'",
            "UPDATE documents SET space_id='space-2' WHERE id='doc-1'",
        ] {
            let (state, _) = canonical_replay_fixture(false).await;
            sqlx::query(change).execute(&state.db).await.unwrap();
            assert!(super::replay_asset_create(
                &state,
                &auth_user(),
                "self-hosted",
                &asset_create_operation("scope-denied", "scope-asset")
            )
            .await
            .is_err());
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(n, 0);
        }
    }

    #[tokio::test]
    async fn asset_replay_late_canonical_receipt_association_and_held_blob_drift_abort() {
        for statement in [
            "UPDATE attachments SET storage_path='foreign/elsewhere'",
            "UPDATE asset_replay_receipts SET source_kind='corrupt'",
        ] {
            let (state, old) = canonical_replay_fixture(false).await;
            tokio::fs::write(crate::storage::data_dir().join(&old), b"vault-image BEFORE")
                .await
                .unwrap();
            let before = canonical_replay_source(&state).await;
            sqlx::query(&format!("CREATE TRIGGER canonical_asset_drift AFTER INSERT ON canonical_revision_receipts BEGIN {statement}; END")).execute(&state.db).await.unwrap();
            assert!(super::replay_asset_create(
                &state,
                &auth_user(),
                "self-hosted",
                &asset_create_operation("late-canonical-association", "asset")
            )
            .await
            .is_err());
            assert_eq!(canonical_replay_source(&state).await, before);
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(n, 0);
        }
        let (state, old) = canonical_replay_fixture(false).await;
        let before = canonical_replay_source(&state).await;
        let parent = crate::storage::data_dir()
            .join(old.strip_suffix("docs/original.md").unwrap())
            .join("attachments/doc-1");
        super::REPLAY_ASSET_BEFORE_COMMIT.with(|s| {
            *s.borrow_mut() = Some(Box::pin(async move {
                let mut entries = tokio::fs::read_dir(parent).await.unwrap();
                let path = entries.next_entry().await.unwrap().unwrap().path();
                tokio::fs::write(path, b"BLOB DRIFT").await.unwrap();
            }))
        });
        assert!(super::replay_asset_create(
            &state,
            &auth_user(),
            "self-hosted",
            &asset_create_operation("late-blob", "asset")
        )
        .await
        .is_err());
        assert_eq!(canonical_replay_source(&state).await, before);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
    #[tokio::test]
    async fn asset_replay_guard_wait_rechecks_external_current_session_before_install() {
        let (state, _) = canonical_replay_fixture(true).await;
        let before = canonical_replay_source(&state).await;
        let mut holder = state.db.begin().await.unwrap();
        crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut holder, &["space-1"])
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE id='test-session'")
            .execute(&mut *holder)
            .await
            .unwrap();
        let worker = state.clone();
        let task = tokio::spawn(async move {
            super::replay_asset_create(
                &worker,
                &auth_user(),
                "self-hosted",
                &asset_create_operation("wait-asset", "asset"),
            )
            .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        assert!(!task.is_finished());
        holder.commit().await.unwrap();
        assert!(task.await.unwrap().is_err());
        assert_eq!(canonical_replay_source(&state).await, before);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
    fn asset_delete_operation(id: &str, entity: &str) -> SyncReplayOperation {
        replay_operation(
            id,
            "2026-10-08T12:00:00Z",
            "asset",
            entity,
            "delete",
            json!({"id":entity,"space_id":"space-1","document_id":"doc-1"}),
        )
    }
    async fn seed_delete_asset(state: &AppState, id: &str) -> String {
        let path = format!("asset-delete-evidence-{}", uuid::Uuid::new_v4());
        tokio::fs::write(crate::storage::data_dir().join(&path), b"RETAINED BLOB")
            .await
            .unwrap();
        sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES($1,'doc-1','space-1','internal.bin','Original.bin','application/octet-stream',13,$2,'user-1')").bind(id).bind(&path).execute(&state.db).await.unwrap();
        let mut conn=state.db.acquire().await.unwrap();crate::spaces::asset_audiences::admit_legacy(&mut conn,id,"space-1","doc-1").await.unwrap();
        path
    }
    #[cfg(target_os="linux")]
    #[tokio::test]
    async fn asset_delete_v2_requires_exact_private_draft_scope_and_retains_original_bytes() {
        let (state,_)=canonical_replay_fixture(false).await;
        let draft="11111111-1111-4111-8111-111111111111";let asset="55555555-5555-4555-8555-555555555555";
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at) VALUES($1,'doc-1','space-1','user-1','Private','PRIVATE ORIGINAL','source-revision','original-created')").bind(draft).execute(&state.db).await.unwrap();
        let mut upload=asset_create_operation("private-delete-source",asset);upload.payload["draft_id"]=json!(draft);
        super::replay_asset_create(&state,&auth_user(),"actual-provider",&upload).await.unwrap();
        let path:String=sqlx::query_scalar("SELECT storage_path FROM attachments WHERE id=$1").bind(asset).fetch_one(&state.db).await.unwrap();let bytes=std::fs::read(crate::storage::data_dir().join(&path)).unwrap();
        sqlx::query("INSERT INTO users(id,email,display_name) VALUES('page-only','page-only@example.invalid','Page editor')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('page-only-session','page-only','fixture','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('page-only-member','space-1','page-only','editor')").execute(&state.db).await.unwrap();
        let mut page_editor=auth_user();page_editor.claims.sub="page-only".into();page_editor.claims.sid="page-only-session".into();page_editor.claims.email="page-only@example.invalid".into();
        let mut deletion=asset_delete_operation("private-delete-operation",asset);
        assert!(super::replay_asset_delete(&state,&page_editor,"actual-provider",&deletion).await.is_err());
        deletion.payload["draft_id"]=json!(draft);
        assert!(super::replay_asset_delete(&state,&page_editor,"actual-provider",&deletion).await.is_err());
        for table in ["attachment_retirements","asset_delete_replay_receipts"] {assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT count(*) FROM {table}")).fetch_one(&state.db).await.unwrap(),0);}
        assert_eq!(std::fs::read(crate::storage::data_dir().join(&path)).unwrap(),bytes);
        assert_eq!(super::replay_asset_delete(&state,&auth_user(),"actual-provider",&deletion).await.unwrap(),"applied");
        assert_eq!(std::fs::read(crate::storage::data_dir().join(&path)).unwrap(),bytes);
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT content FROM document_drafts WHERE id=$1").bind(draft).fetch_one(&state.db).await.unwrap(),"PRIVATE ORIGINAL");
        let receipt:String=sqlx::query_scalar("SELECT receipt_json FROM asset_delete_replay_receipts").fetch_one(&state.db).await.unwrap();
        assert_eq!(super::replay_asset_delete(&state,&auth_user(),"actual-provider",&deletion).await.unwrap(),"skipped");
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT receipt_json FROM asset_delete_replay_receipts").fetch_one(&state.db).await.unwrap(),receipt);
    }
    #[cfg(target_os="linux")]
    #[tokio::test]
    async fn asset_delete_v2_late_draft_incarnation_change_rolls_back_retirement() {
        let (state,_)=canonical_replay_fixture(false).await;
        let draft="11111111-1111-4111-8111-111111111111";let asset="66666666-6666-4666-8666-666666666666";
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at) VALUES($1,'doc-1','space-1','user-1','Private','PRIVATE ORIGINAL','source-revision','original-created')").bind(draft).execute(&state.db).await.unwrap();
        let mut upload=asset_create_operation("late-private-delete-source",asset);upload.payload["draft_id"]=json!(draft);super::replay_asset_create(&state,&auth_user(),"actual-provider",&upload).await.unwrap();
        let path:String=sqlx::query_scalar("SELECT storage_path FROM attachments WHERE id=$1").bind(asset).fetch_one(&state.db).await.unwrap();let bytes=std::fs::read(crate::storage::data_dir().join(&path)).unwrap();
        sqlx::query("CREATE TRIGGER change_deleting_draft AFTER INSERT ON asset_delete_replay_receipts BEGIN UPDATE document_drafts SET created_at='changed-incarnation'; END").execute(&state.db).await.unwrap();
        let mut deletion=asset_delete_operation("late-private-delete",asset);deletion.payload["draft_id"]=json!(draft);
        assert!(super::replay_asset_delete(&state,&auth_user(),"actual-provider",&deletion).await.is_err());
        for table in ["attachment_retirements","asset_delete_replay_receipts"] {assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT count(*) FROM {table}")).fetch_one(&state.db).await.unwrap(),0);}
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT created_at FROM document_drafts WHERE id=$1").bind(draft).fetch_one(&state.db).await.unwrap(),"original-created");assert_eq!(std::fs::read(crate::storage::data_dir().join(&path)).unwrap(),bytes);
    }
    #[tokio::test]
    async fn asset_delete_retains_association_and_historical_receipt_never_resurrects_retired_id() {
        let (state, _) = canonical_replay_fixture(false).await;
        let source = canonical_replay_source(&state).await;
        let path = seed_delete_asset(&state, "raw:asset/id").await;
        let op = asset_delete_operation(" legacy:delete ", "raw:asset/id");
        assert_eq!(
            super::replay_asset_delete(&state, &auth_user(), "actual-provider", &op)
                .await
                .unwrap(),
            "applied"
        );
        assert_eq!(
            tokio::fs::read(crate::storage::data_dir().join(&path))
                .await
                .unwrap(),
            b"RETAINED BLOB"
        );
        assert_eq!(canonical_replay_source(&state).await, source);
        let retirement_before=crate::storage::attachment_retirement::read(&mut *state.db.acquire().await.unwrap(),"raw:asset/id").await.unwrap().unwrap();
        let receipt_before:String=sqlx::query_scalar("SELECT receipt_json FROM asset_delete_replay_receipts").fetch_one(&state.db).await.unwrap();
        assert!(sqlx::query("INSERT INTO attachments SELECT * FROM attachments WHERE id='raw:asset/id'").execute(&state.db).await.is_err(),"retired identity cannot be reused");
        assert_eq!(
            super::replay_asset_delete(&state, &auth_user(), "actual-provider", &op)
                .await
                .unwrap(),
            "skipped"
        );
        let current: String =
            sqlx::query_scalar("SELECT storage_path FROM attachments WHERE id='raw:asset/id'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(current, path);
        assert_eq!(crate::storage::attachment_retirement::read(&mut *state.db.acquire().await.unwrap(),"raw:asset/id").await.unwrap().unwrap(),retirement_before);
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT receipt_json FROM asset_delete_replay_receipts").fetch_one(&state.db).await.unwrap(),receipt_before);
        let mut changed = op.clone();
        changed.payload["other"] = json!(true);
        assert!(
            super::replay_asset_delete(&state, &auth_user(), "actual-provider", &changed)
                .await
                .is_err()
        );
        sqlx::query("DELETE FROM sessions")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(
            super::replay_asset_delete(&state, &auth_user(), "actual-provider", &op)
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn asset_delete_scoped_absence_no_context_foreign_and_invalid_context() {
        let (state, _) = canonical_replay_fixture(false).await;
        let op = asset_delete_operation("absent", "missing");
        assert_eq!(
            super::replay_asset_delete(&state, &auth_user(), "actual", &op)
                .await
                .unwrap(),
            "skipped"
        );
        let outcome: String =
            sqlx::query_scalar("SELECT outcome FROM asset_delete_replay_receipts")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(outcome, "observed_absent");
        for payload in [
            json!({"id":"missing"}),
            json!({"id":"missing","space_id":null,"document_id":"doc-1"}),
            json!({"id":"missing","space_id":"space-foreign","document_id":"doc-1"}),
        ] {
            let mut bad = asset_delete_operation(&uuid::Uuid::new_v4().to_string(), "missing");
            bad.payload = payload;
            assert!(
                super::replay_asset_delete(&state, &auth_user(), "actual", &bad)
                    .await
                    .is_err()
            );
        }
        seed_delete_asset(&state, "existing").await;
        let mut bad = asset_delete_operation("foreign", "existing");
        bad.payload["document_id"] = json!("wrong");
        assert!(
            super::replay_asset_delete(&state, &auth_user(), "actual", &bad)
                .await
                .is_err()
        );
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 1);
    }
    #[tokio::test]
    async fn asset_delete_late_policy_source_receipt_changes_rollback_and_never_unlink() {
        for statement in ["DELETE FROM sessions WHERE id='test-session'","UPDATE users SET is_active=0 WHERE id='user-1'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','user-1','user-1')","UPDATE documents SET locked=1,locked_by='other-user' WHERE id='doc-1'","UPDATE documents SET archived_at='late' WHERE id='doc-1'","UPDATE documents SET updated_at='late' WHERE id='doc-1'","UPDATE asset_delete_replay_receipts SET outcome='changed'"]{
            let (state,_)=canonical_replay_fixture(false).await;let path=seed_delete_asset(&state,"asset").await;let before=canonical_replay_source(&state).await;
            sqlx::query(&format!("CREATE TRIGGER late_delete AFTER INSERT ON asset_delete_replay_receipts BEGIN {statement}; END")).execute(&state.db).await.unwrap();
            assert!(super::replay_asset_delete(&state,&auth_user(),"actual",&asset_delete_operation("late","asset")).await.is_err(),"{statement}");
            let n:i64=sqlx::query_scalar("SELECT count(*) FROM attachments").fetch_one(&state.db).await.unwrap();assert_eq!(n,1);let n:i64=sqlx::query_scalar("SELECT count(*) FROM asset_delete_replay_receipts").fetch_one(&state.db).await.unwrap();assert_eq!(n,0);assert_eq!(canonical_replay_source(&state).await,before);assert_eq!(tokio::fs::read(crate::storage::data_dir().join(path)).await.unwrap(),b"RETAINED BLOB");
        }
    }
    #[tokio::test]
    async fn asset_delete_unknown_commit_exact_proof_and_rollback() {
        for fault in [
            super::ReplayCanonicalCommitFault::Rollback,
            super::ReplayCanonicalCommitFault::LostAck,
        ] {
            let (state, _) = canonical_replay_fixture(false).await;
            let path = seed_delete_asset(&state, "asset").await;
            super::REPLAY_ASSET_DELETE_COMMIT_FAULT.with(|s| *s.borrow_mut() = Some(fault));
            let result = super::replay_asset_delete(
                &state,
                &auth_user(),
                "actual",
                &asset_delete_operation("uncertain", "asset"),
            )
            .await;
            assert_eq!(
                result.is_ok(),
                fault != super::ReplayCanonicalCommitFault::Rollback
            );
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(n,1,"both rollback and committed retirement retain original association");
            let retired:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_retirements").fetch_one(&state.db).await.unwrap();
            assert_eq!(retired,if fault==super::ReplayCanonicalCommitFault::Rollback{0}else{1});
            let receipts:i64=sqlx::query_scalar("SELECT count(*) FROM asset_delete_replay_receipts").fetch_one(&state.db).await.unwrap();
            assert_eq!(receipts,retired,"retirement and acknowledgment commit atomically");
            assert_eq!(
                tokio::fs::read(crate::storage::data_dir().join(path))
                    .await
                    .unwrap(),
                b"RETAINED BLOB"
            );
        }
    }
    #[tokio::test]
    async fn asset_delete_wal_before_commit_retains_visible_association_and_bytes() {
        let (state, _) = canonical_replay_fixture(true).await;
        let path = seed_delete_asset(&state, "asset").await;
        let observer = state.clone();
        let evidence = path.clone();
        super::REPLAY_ASSET_DELETE_BEFORE_COMMIT.with(|s| {
            *s.borrow_mut() = Some(Box::pin(async move {
                let n: i64 =
                    sqlx::query_scalar("SELECT count(*) FROM attachments WHERE id='asset'")
                        .fetch_one(&observer.db)
                        .await
                        .unwrap();
                assert_eq!(n, 1);
                let n: i64 =
                    sqlx::query_scalar("SELECT count(*) FROM asset_delete_replay_receipts")
                        .fetch_one(&observer.db)
                        .await
                        .unwrap();
                assert_eq!(n, 0);
                assert_eq!(
                    tokio::fs::read(crate::storage::data_dir().join(evidence))
                        .await
                        .unwrap(),
                    b"RETAINED BLOB"
                );
            }))
        });
        super::replay_asset_delete(
            &state,
            &auth_user(),
            "actual",
            &asset_delete_operation("wal", "asset"),
        )
        .await
        .unwrap();
        assert_eq!(
            tokio::fs::read(crate::storage::data_dir().join(path))
                .await
                .unwrap(),
            b"RETAINED BLOB"
        );
    }

    #[tokio::test]
    async fn asset_delete_guard_wait_observes_external_session_revocation() {
        let (state, _) = canonical_replay_fixture(true).await;
        let path = seed_delete_asset(&state, "asset").await;
        let mut holder = state.db.begin().await.unwrap();
        crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut holder, &["space-1"])
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE id='test-session'")
            .execute(&mut *holder)
            .await
            .unwrap();
        let worker = state.clone();
        let task = tokio::spawn(async move {
            super::replay_asset_delete(
                &worker,
                &auth_user(),
                "actual",
                &asset_delete_operation("wait", "asset"),
            )
            .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        assert!(!task.is_finished());
        holder.commit().await.unwrap();
        assert!(task.await.unwrap().is_err());
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            tokio::fs::read(crate::storage::data_dir().join(path))
                .await
                .unwrap(),
            b"RETAINED BLOB"
        );
    }
    #[tokio::test]
    async fn asset_delete_private_and_lock_policy_is_actual_page_edit() {
        let (state, _) = canonical_replay_fixture(false).await;
        seed_delete_asset(&state, "asset").await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='doc-1'")
            .execute(&state.db)
            .await
            .unwrap();
        let op = asset_delete_operation("private", "asset");
        assert!(
            super::replay_asset_delete(&state, &auth_user(), "actual", &op)
                .await
                .is_err(),
            "creator cannot bypass exact draft participation"
        );
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('owned','doc-1','space-1','user-1','Draft','UNCHANGED','old')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET locked=1,locked_by='other-user' WHERE id='doc-1'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(
            super::replay_asset_delete(&state, &auth_user(), "actual", &op)
                .await
                .is_err()
        );
        sqlx::query("UPDATE documents SET locked=0,locked_by=NULL WHERE id='doc-1'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            super::replay_asset_delete(&state, &auth_user(), "actual", &op)
                .await
                .unwrap(),
            "applied"
        );
        let body: String =
            sqlx::query_scalar("SELECT content FROM document_drafts WHERE id='owned'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(body, "UNCHANGED");
    }
    #[tokio::test]
    async fn asset_delete_unproven_legacy_ledger_and_postlock_foreign_association_failclosed() {
        let (state, _) = canonical_replay_fixture(false).await;
        let op = asset_delete_operation("legacy", "absent");
        let manifest = single_operation_manifest(op.clone());
        super::insert_replay_ledger(&state, &auth_user(), &manifest, &op, "pending", None)
            .await
            .unwrap();
        super::update_replay_ledger(&state, &op.id, "skipped", None)
            .await
            .unwrap();
        let response = execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest))
            .await
            .unwrap()
            .0;
        assert_eq!(response.failed_operation_count, 1);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM asset_delete_replay_receipts")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 0);
        seed_delete_asset(&state, "asset").await;
        sqlx::query("CREATE TRIGGER move_on_lock AFTER UPDATE OF id ON attachments BEGIN UPDATE attachments SET document_id='missing-page' WHERE id=NEW.id; END").execute(&state.db).await.unwrap();
        assert!(super::replay_asset_delete(
            &state,
            &auth_user(),
            "actual",
            &asset_delete_operation("moved", "asset")
        )
        .await
        .is_err());
        let doc: String =
            sqlx::query_scalar("SELECT document_id FROM attachments WHERE id='asset'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(doc, "doc-1");
    }
    #[tokio::test]
    async fn asset_delete_http_write_key_read_deny_and_late_downgrade() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;
        let (state, _) = canonical_replay_fixture(false).await;
        seed_delete_asset(&state, "first").await;
        seed_delete_asset(&state, "second").await;
        let raw = "asset-delete-key";
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('delete-key','user-1','Delete',$1,'prefix','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key(raw)).execute(&state.db).await.unwrap();
        let app = super::api_router().with_state(state.clone());
        let request = |id: &str, asset: &str| {
            let op = asset_delete_operation(id, asset);
            let c = contract();
            let manifest = json!({"schema_version":1,"provider":"self-hosted","operation_count":1,"replay_contract":{"schema_version":c.schema_version,"operation_order":c.operation_order,"idempotency_key":c.idempotency_key,"transport":c.transport,"server_backends":c.server_backends,"required_payload_status":c.required_payload_status,"conflict_policy":c.conflict_policy,"web_canonical_store":c.web_canonical_store},"operations":[{"id":op.id,"entity_type":op.entity_type,"entity_id":op.entity_id,"action":op.action,"queued_at":op.queued_at,"payload_status":op.payload_status,"payload":op.payload}]});
            Request::builder()
                .method("POST")
                .uri("/sync/replay")
                .header("X-API-Key", raw)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&manifest).unwrap()))
                .unwrap()
        };
        let response = app
            .clone()
            .oneshot(request("http-first", "first"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["applied_operation_count"], 1);
        sqlx::query("CREATE TRIGGER late_delete_key AFTER INSERT ON asset_delete_replay_receipts BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='delete-key'; END").execute(&state.db).await.unwrap();
        let response = app
            .clone()
            .oneshot(request("http-second", "second"))
            .await
            .unwrap();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["failed_operation_count"], 1);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments WHERE id='second'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 1);
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='delete-key'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            app.oneshot(request("http-read", "second"))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn asset_delete_indeterminate_receipt_read_never_claims_success_and_retry_heals() {
        let (state, _) = canonical_replay_fixture(false).await;
        let path = seed_delete_asset(&state, "asset").await;
        let op = asset_delete_operation("uncertain-proof", "asset");
        let broken = state.clone();
        super::REPLAY_ASSET_DELETE_COMMIT_FAULT
            .with(|s| *s.borrow_mut() = Some(super::ReplayCanonicalCommitFault::UnknownProof));
        super::REPLAY_ASSET_DELETE_BEFORE_CONFIRM.with(|s|*s.borrow_mut()=Some(Box::pin(async move{sqlx::query("ALTER TABLE asset_delete_replay_receipts RENAME TO unavailable_delete_receipts").execute(&broken.db).await.unwrap();})));
        assert!(
            super::replay_asset_delete(&state, &auth_user(), "actual", &op)
                .await
                .unwrap_err()
                .contains("indeterminate")
        );
        assert_eq!(
            tokio::fs::read(crate::storage::data_dir().join(path))
                .await
                .unwrap(),
            b"RETAINED BLOB"
        );
        sqlx::query(
            "ALTER TABLE unavailable_delete_receipts RENAME TO asset_delete_replay_receipts",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert_eq!(
            super::replay_asset_delete(&state, &auth_user(), "actual", &op)
                .await
                .unwrap(),
            "skipped"
        );
    }
    fn incoming_create_operation(id: &str, entity: &str) -> SyncReplayOperation {
        replay_operation(
            id,
            "2026-10-08T12:00:00Z",
            "document",
            entity,
            "create",
            json!({"id":entity,"space_id":"space-1","title":"Imported private title","content":"---\r\ncustom: original\r\n---\r\nRAW PRIVATE BODY\r\n","icon":"lucide:file","cover_image":"https://example.test/cover","created_at":"2026-10-01T12:00:00Z","updated_at":"2026-10-08T12:00:00Z"}),
        )
    }
    #[tokio::test]
    async fn replay_create_preserves_raw_identity_metadata_owned_body_and_historical_state() {
        let (state, _) = canonical_replay_fixture(false).await;
        let op = incoming_create_operation(" legacy: incoming ", " raw:legacy/id ");
        let before = canonical_replay_source(&state).await;
        assert_eq!(
            super::replay_document_create(&state, &auth_user(), "actual-provider", &op)
                .await
                .unwrap(),
            "applied"
        );
        let row:(String,Option<String>,Option<String>,Option<String>,String,String,String,String)=sqlx::query_as("SELECT title,icon,cover_image,published_at,created_by,created_at,updated_at,content_path FROM documents WHERE id=$1").bind(&op.entity_id).fetch_one(&state.db).await.unwrap();
        assert_eq!(row.0, "Imported private title");
        assert_eq!(row.1.as_deref(), Some("lucide:file"));
        assert_eq!(row.2.as_deref(), Some("https://example.test/cover"));
        assert_eq!(row.3, None);
        assert_eq!(row.4, "user-1");
        assert_eq!(row.5, "2026-10-01T12:00:00+00:00");
        assert_eq!(row.6, "2026-10-08T12:00:00+00:00");
        let draft: (String, String, Option<String>) = sqlx::query_as(
            "SELECT owner_id,content,cover_image FROM document_drafts WHERE document_id=$1",
        )
        .bind(&op.entity_id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(draft.0, "user-1");
        assert_eq!(draft.1, op.payload["content"].as_str().unwrap());
        assert_eq!(draft.2, row.2);
        assert_eq!(canonical_replay_source(&state).await, before);
        sqlx::query("UPDATE document_drafts SET content='LATER EDIT' WHERE document_id=$1")
            .bind(&op.entity_id)
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            super::replay_document_create(&state, &auth_user(), "actual-provider", &op)
                .await
                .unwrap(),
            "skipped"
        );
        let body: String =
            sqlx::query_scalar("SELECT content FROM document_drafts WHERE document_id=$1")
                .bind(&op.entity_id)
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(body, "LATER EDIT");
        sqlx::query("DELETE FROM documents WHERE id=$1")
            .bind(&op.entity_id)
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            super::replay_document_create(&state, &auth_user(), "actual-provider", &op)
                .await
                .unwrap(),
            "skipped"
        );
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM documents WHERE id=$1")
            .bind(&op.entity_id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 0);
        let mut collision = op.clone();
        collision.payload["title"] = json!("Changed");
        assert!(
            super::replay_document_create(&state, &auth_user(), "actual-provider", &collision)
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn replay_create_exact_create_content_not_edit_and_current_role_union() {
        let (state, _) = canonical_replay_fixture(false).await;
        sqlx::query("UPDATE spaces SET created_by='other-user' WHERE id='space-1'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('custom:create','space-1','Creator','[\"create_content\"]','user-1')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='custom:create' WHERE user_id='user-1'")
            .execute(&state.db)
            .await
            .unwrap();
        let mut op = incoming_create_operation("create-only", "create-only");
        op.payload.as_object_mut().unwrap().remove("content");
        super::replay_document_create(&state, &auth_user(), "actual", &op)
            .await
            .unwrap();
        let body: String = sqlx::query_scalar(
            "SELECT content FROM document_drafts WHERE document_id='create-only'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(body, "");
        // Preserve existing local EditContent -> CreateContent compatibility; granular global Edit alone must not grant creation.
        for q in ["UPDATE space_roles SET permissions='[\"view\",\"comment\"]' WHERE id='custom:create'", "INSERT INTO groups(id,name) VALUES('create-group','Creators')","INSERT INTO group_members(id,group_id,user_id) VALUES('create-member','create-group','user-1')","UPDATE global_roles SET permissions='[\"edit_content\"]' WHERE id='contributor'","INSERT INTO group_global_roles(group_id,role_id) VALUES('create-group','contributor')"]{sqlx::query(q).execute(&state.db).await.unwrap();}
        assert!(super::replay_document_create(
            &state,
            &auth_user(),
            "actual",
            &incoming_create_operation("edit-only", "edit-only")
        )
        .await
        .is_err());
        sqlx::query(
            "UPDATE global_roles SET permissions='[\"create_content\"]' WHERE id='contributor'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        super::replay_document_create(
            &state,
            &auth_user(),
            "actual",
            &incoming_create_operation("union", "union"),
        )
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn replay_create_parent_current_private_lock_folder_and_cycle_rules() {
        let (state, _) = canonical_replay_fixture(false).await;
        let mut op = incoming_create_operation("child", "child");
        op.payload["parent_id"] = json!("doc-1");
        super::replay_document_create(&state, &auth_user(), "actual", &op)
            .await
            .unwrap();
        let parent: String = sqlx::query_scalar("SELECT parent_id FROM documents WHERE id='child'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(parent, "doc-1");
        for q in [
            "UPDATE documents SET locked=1,locked_by='other-user' WHERE id='doc-1'",
            "UPDATE documents SET locked=0,locked_by=NULL,published_at=NULL WHERE id='doc-1'",
            "UPDATE documents SET published_at='published',parent_id='doc-1' WHERE id='doc-1'",
        ] {
            sqlx::query(q).execute(&state.db).await.unwrap();
            let mut denied = incoming_create_operation(
                &uuid::Uuid::new_v4().to_string(),
                &uuid::Uuid::new_v4().to_string(),
            );
            denied.payload["parent_id"] = json!("doc-1");
            assert!(
                super::replay_document_create(&state, &auth_user(), "actual", &denied)
                    .await
                    .is_err()
            );
        }
    }
    #[tokio::test]
    async fn replay_deep_page_parent_chain_preserves_exact_retry_and_current_ancestor_admission() {
        let (state, _) = canonical_replay_fixture(false).await;
        let mut parent = "doc-1".to_owned();
        for depth in 1..=32 {
            let id = format!("replay-deep-{depth}");
            let mut operation = incoming_create_operation(&id, &id);
            operation.payload["parent_id"] = json!(parent);
            assert_eq!(super::replay_document_create(&state, &auth_user(), "actual", &operation).await.unwrap(), "applied");
            let saved: Option<String> = sqlx::query_scalar("SELECT parent_id FROM documents WHERE id=$1").bind(&id).fetch_one(&state.db).await.unwrap();
            assert_eq!(saved.as_deref(), Some(parent.as_str()));
            assert_eq!(super::replay_document_create(&state, &auth_user(), "actual", &operation).await.unwrap(), "skipped");
            parent = id;
        }
        sqlx::query("UPDATE documents SET locked=1,locked_by='other-user' WHERE id='doc-1'").execute(&state.db).await.unwrap();
        let mut denied = incoming_create_operation("deep-denied", "deep-denied");
        denied.payload["parent_id"] = json!(parent);
        assert!(super::replay_document_create(&state, &auth_user(), "actual", &denied).await.is_err());
        let missing: i64 = sqlx::query_scalar("SELECT count(*) FROM documents WHERE id='deep-denied'").fetch_one(&state.db).await.unwrap();
        assert_eq!(missing, 0);
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM document_create_replay_receipts").fetch_one(&state.db).await.unwrap();
        assert_eq!(receipts, 32);
    }
    #[tokio::test]
    async fn replay_create_late_row_policy_and_receipt_changes_rollback_all_canonical_rows() {
        for statement in ["DELETE FROM sessions WHERE id='test-session'","UPDATE users SET is_active=0 WHERE id='user-1'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','user-1','user-1')","UPDATE documents SET title='changed' WHERE id='new-page'","UPDATE document_drafts SET title='changed',content='changed' WHERE document_id='new-page'","DELETE FROM doc_subscriptions WHERE document_id='new-page'","UPDATE document_create_replay_receipts SET action='changed'","UPDATE storage_journal_receipts SET sha256='changed'"]{
            let (state,old)=canonical_replay_fixture(false).await;sqlx::query(&format!("CREATE TRIGGER late_create AFTER INSERT ON document_create_replay_receipts BEGIN {statement}; END")).execute(&state.db).await.unwrap();assert!(super::replay_document_create(&state,&auth_user(),"actual",&incoming_create_operation("late","new-page")).await.is_err(),"{statement}");for table in ["document_create_replay_receipts","storage_journal_receipts"]{let n:i64=sqlx::query_scalar(&format!("SELECT count(*) FROM {table}")).fetch_one(&state.db).await.unwrap();assert_eq!(n,0,"{statement}");}let n:i64=sqlx::query_scalar("SELECT count(*) FROM documents WHERE id='new-page'").fetch_one(&state.db).await.unwrap();assert_eq!(n,0);assert_eq!(tokio::fs::read(crate::storage::data_dir().join(old)).await.unwrap(),b"PUBLIC OLD RAW\r\n");
        }
    }
    #[tokio::test]
    async fn replay_create_lost_ack_and_indeterminate_proof_never_duplicate_owned_draft() {
        for fault in [
            super::ReplayCanonicalCommitFault::Rollback,
            super::ReplayCanonicalCommitFault::LostAck,
            super::ReplayCanonicalCommitFault::UnknownProof,
        ] {
            let (state, _) = canonical_replay_fixture(false).await;
            let op = incoming_create_operation("uncertain", "new-page");
            super::REPLAY_CREATE_COMMIT_FAULT.with(|s| *s.borrow_mut() = Some(fault));
            if fault == super::ReplayCanonicalCommitFault::UnknownProof {
                let broken = state.clone();
                super::REPLAY_CREATE_BEFORE_CONFIRM.with(|s|*s.borrow_mut()=Some(Box::pin(async move{sqlx::query("ALTER TABLE document_create_replay_receipts RENAME TO unavailable_create_receipts").execute(&broken.db).await.unwrap();})));
            }
            let result = super::replay_document_create(&state, &auth_user(), "actual", &op).await;
            assert_eq!(
                result.is_ok(),
                fault == super::ReplayCanonicalCommitFault::LostAck
            );
            let n: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM document_drafts WHERE document_id='new-page'",
            )
            .fetch_one(&state.db)
            .await
            .unwrap();
            assert_eq!(
                n,
                if fault == super::ReplayCanonicalCommitFault::Rollback {
                    0
                } else {
                    1
                }
            );
            if fault == super::ReplayCanonicalCommitFault::UnknownProof {
                assert!(result.unwrap_err().contains("indeterminate"));
                sqlx::query("ALTER TABLE unavailable_create_receipts RENAME TO document_create_replay_receipts").execute(&state.db).await.unwrap();
                assert_eq!(
                    super::replay_document_create(&state, &auth_user(), "actual", &op)
                        .await
                        .unwrap(),
                    "skipped"
                );
            }
        }
    }
    #[tokio::test]
    async fn replay_create_wal_visibility_and_guardwait_current_session() {
        let (state, _) = canonical_replay_fixture(true).await;
        let observer = state.clone();
        super::REPLAY_CREATE_BEFORE_COMMIT.with(|s| {
            *s.borrow_mut() = Some(Box::pin(async move {
                for table in [
                    "document_create_replay_receipts",
                    "storage_journal_receipts",
                ] {
                    let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
                        .fetch_one(&observer.db)
                        .await
                        .unwrap();
                    assert_eq!(n, 0);
                }
                let n: i64 =
                    sqlx::query_scalar("SELECT count(*) FROM documents WHERE id='wal-new'")
                        .fetch_one(&observer.db)
                        .await
                        .unwrap();
                assert_eq!(n, 0);
            }))
        });
        super::replay_document_create(
            &state,
            &auth_user(),
            "actual",
            &incoming_create_operation("wal", "wal-new"),
        )
        .await
        .unwrap();
        let mut holder = state.db.begin().await.unwrap();
        crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut holder, &["space-1"])
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE id='test-session'")
            .execute(&mut *holder)
            .await
            .unwrap();
        let worker = state.clone();
        let task = tokio::spawn(async move {
            super::replay_document_create(
                &worker,
                &auth_user(),
                "actual",
                &incoming_create_operation("wait", "wait-new"),
            )
            .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        assert!(!task.is_finished());
        holder.commit().await.unwrap();
        assert!(task.await.unwrap().is_err());
    }
    #[tokio::test]
    async fn replay_create_filename_collision_commits_sql_without_overwriting_and_retains_pending_stage(
    ) {
        let (state, _) = canonical_replay_fixture(false).await;
        let one = incoming_create_operation("first", "same:id");
        let two = incoming_create_operation("second", "same/id");
        super::replay_document_create(&state, &auth_user(), "actual", &one)
            .await
            .unwrap();
        let path: String =
            sqlx::query_scalar("SELECT content_path FROM documents WHERE id='same:id'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let before = tokio::fs::read(crate::storage::data_dir().join(&path))
            .await
            .unwrap();
        let mut two = two;
        two.payload["content"] = json!("SECOND BODY");
        assert_eq!(
            super::replay_document_create(&state, &auth_user(), "actual", &two)
                .await
                .unwrap(),
            "applied"
        );
        assert_eq!(
            tokio::fs::read(crate::storage::data_dir().join(&path))
                .await
                .unwrap(),
            before
        );
        let state_receipt: String = sqlx::query_scalar(
            "SELECT state FROM storage_journal_receipts WHERE document_id='same/id'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(state_receipt, "pending");
        let body: String =
            sqlx::query_scalar("SELECT content FROM document_drafts WHERE document_id='same/id'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(body, "SECOND BODY");
    }

    #[tokio::test]
    async fn replay_create_http_write_key_read_deny_late_downgrade_and_unproven_ids() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;
        let (state, _) = canonical_replay_fixture(false).await;
        let raw = "replay-create-key";
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('create-key','user-1','Create',$1,'prefix','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key(raw)).execute(&state.db).await.unwrap();
        let app = super::api_router().with_state(state.clone());
        let request = |id: &str| {
            let op = incoming_create_operation(id, id);
            let c = contract();
            let manifest = json!({"schema_version":1,"provider":"self-hosted","operation_count":1,"replay_contract":{"schema_version":c.schema_version,"operation_order":c.operation_order,"idempotency_key":c.idempotency_key,"transport":c.transport,"server_backends":c.server_backends,"required_payload_status":c.required_payload_status,"conflict_policy":c.conflict_policy,"web_canonical_store":c.web_canonical_store},"operations":[{"id":op.id,"entity_type":op.entity_type,"entity_id":op.entity_id,"action":op.action,"queued_at":op.queued_at,"payload_status":op.payload_status,"payload":op.payload}]});
            Request::builder()
                .method("POST")
                .uri("/sync/replay")
                .header("X-API-Key", raw)
                .header("content-type", "application/json")
                .body(Body::from(manifest.to_string()))
                .unwrap()
        };
        let response = app.clone().oneshot(request("http-first")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["applied_operation_count"], 1);
        sqlx::query("CREATE TRIGGER late_create_key AFTER INSERT ON document_create_replay_receipts BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='create-key'; END").execute(&state.db).await.unwrap();
        let response = app.clone().oneshot(request("http-late")).await.unwrap();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["failed_operation_count"], 1);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM documents WHERE id='http-late'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 0);
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='create-key'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            app.oneshot(request("http-read")).await.unwrap().status(),
            StatusCode::FORBIDDEN
        );
        assert!(super::replay_document_create(
            &state,
            &auth_user(),
            "actual",
            &incoming_create_operation("collision", "doc-1")
        )
        .await
        .is_err());
        let op = incoming_create_operation("legacy-skip", "unproven");
        let manifest = single_operation_manifest(op.clone());
        super::insert_replay_ledger(&state, &auth_user(), &manifest, &op, "skipped", None)
            .await
            .unwrap();
        let response = execute_replay_manifest(State(state.clone()), auth_user(), Json(manifest))
            .await
            .unwrap()
            .0;
        assert_eq!(response.failed_operation_count, 1);
    }
    #[tokio::test]
    async fn replay_create_failed_draft_keeps_only_private_stage_and_never_installs_canonical_file()
    {
        let (state, _) = canonical_replay_fixture(false).await;
        let stage = std::sync::Arc::new(std::sync::Mutex::new(None));
        let captured = stage.clone();
        crate::storage::journal::test_stage_callback(move |m| {
            *captured.lock().unwrap() = Some(m.clone())
        });
        sqlx::query("CREATE TRIGGER fail_new_draft BEFORE INSERT ON document_drafts BEGIN SELECT RAISE(ABORT,'fail draft'); END").execute(&state.db).await.unwrap();
        assert!(super::replay_document_create(
            &state,
            &auth_user(),
            "actual",
            &incoming_create_operation("failure", "failure")
        )
        .await
        .is_err());
        let manifest = stage.lock().unwrap().clone().unwrap();
        assert!(!crate::storage::data_dir().join(&manifest.path).exists());
        let private = crate::storage::data_dir()
            .join(".storage-journal")
            .join(&manifest.op_id)
            .join("payload");
        assert!(private.exists());
        for table in [
            "storage_journal_receipts",
            "document_create_replay_receipts",
        ] {
            let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(n, 0);
        }
    }

    #[tokio::test]
    async fn replay_create_expired_jwt_inactive_actor_and_real_service_principal_deny_before_stage()
    {
        let (state, _) = canonical_replay_fixture(false).await;
        let op = incoming_create_operation("proof", "proof");
        let mut expired = auth_user();
        expired.claims.exp = 0;
        assert!(
            super::replay_document_create(&state, &expired, "actual", &op)
                .await
                .is_err()
        );
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('replay-create-service',NULL,'Service','hash','prefix',1,'[\"admin\"]')").execute(&state.db).await.unwrap();
        let mut service = auth_user();
        service.claims.sub = "service:replay-create-service".into();
        service.claims.sid = service.claims.sub.clone();
        service.api_key_scopes = Some(vec!["admin".into()]);
        assert!(
            super::replay_document_create(&state, &service, "actual", &op)
                .await
                .is_err()
        );
        sqlx::query("UPDATE users SET is_active=0 WHERE id='user-1'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(
            super::replay_document_create(&state, &auth_user(), "actual", &op)
                .await
                .is_err()
        );
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM storage_journal_receipts")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
    #[tokio::test]
    async fn replay_create_present_nonstring_body_fails_before_staging_while_absent_remains_empty()
    {
        for body in [
            Value::Null,
            json!(42),
            json!({"body":"must retain"}),
            json!(["must retain"]),
        ] {
            let (state, _) = canonical_replay_fixture(false).await;
            let mut op = incoming_create_operation("malformed-body", "malformed-body");
            op.payload["content"] = body;
            let stage = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let observed = stage.clone();
            crate::storage::journal::test_stage_callback(move |_| {
                observed.store(true, std::sync::atomic::Ordering::SeqCst);
            });
            let error = super::replay_document_create(&state, &auth_user(), "actual", &op)
                .await
                .unwrap_err();
            assert!(error.contains("must be a string"));
            assert!(!stage.load(std::sync::atomic::Ordering::SeqCst));
            for table in [
                "storage_journal_receipts",
                "document_create_replay_receipts",
            ] {
                let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
                assert_eq!(n, 0);
            }
            let n: i64 =
                sqlx::query_scalar("SELECT count(*) FROM documents WHERE id='malformed-body'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(n, 0);
        }
    }
    #[tokio::test]
    async fn replay_create_initial_default_page_and_draft_fields_reject_late_mutation() {
        for statement in ["UPDATE documents SET locked_by='other-user' WHERE id='new-page'","UPDATE documents SET public_slug='unexpected-link' WHERE id='new-page'","UPDATE documents SET automerge_state=X'0102' WHERE id='new-page'","UPDATE documents SET sort_order=7 WHERE id='new-page'","UPDATE documents SET deleted_by='other-user' WHERE id='new-page'","UPDATE document_drafts SET previous_folder_id='unexpected' WHERE document_id='new-page'","UPDATE document_drafts SET previous_parent_id='unexpected' WHERE document_id='new-page'"] {
            let (state,_)=canonical_replay_fixture(false).await;sqlx::query(&format!("CREATE TRIGGER late_defaults AFTER INSERT ON document_create_replay_receipts BEGIN {statement}; END")).execute(&state.db).await.unwrap();assert!(super::replay_document_create(&state,&auth_user(),"actual",&incoming_create_operation("default-drift","new-page")).await.is_err(),"{statement}");let n:i64=sqlx::query_scalar("SELECT count(*) FROM documents WHERE id='new-page'").fetch_one(&state.db).await.unwrap();assert_eq!(n,0);let n:i64=sqlx::query_scalar("SELECT count(*) FROM document_create_replay_receipts").fetch_one(&state.db).await.unwrap();assert_eq!(n,0);
        }
    }

    #[tokio::test]
    async fn replay_create_final_stage_corruption_manifest_and_symlink_rollback_sql_retain_evidence(
    ) {
        for kind in ["payload", "manifest", "symlink"] {
            let (state, old) = canonical_replay_fixture(false).await;
            let captured = std::sync::Arc::new(std::sync::Mutex::new(None));
            let capture = captured.clone();
            crate::storage::journal::test_stage_callback(move |m| {
                *capture.lock().unwrap() = Some(m.clone())
            });
            let evidence = captured.clone();
            let variant = kind.to_owned();
            let outside = std::env::temp_dir()
                .join(format!("replay-create-outside-{}", uuid::Uuid::new_v4()));
            std::fs::write(&outside, b"UNOWNED").unwrap();
            let external = outside.clone();
            super::REPLAY_CREATE_BEFORE_COMMIT.with(|h| {
                *h.borrow_mut() = Some(Box::pin(async move {
                    let m = evidence.lock().unwrap().clone().unwrap();
                    let dir = crate::storage::data_dir()
                        .join(".storage-journal")
                        .join(&m.op_id);
                    match variant.as_str() {
                        "payload" => std::fs::write(dir.join("payload"), b"CORRUPTED").unwrap(),
                        "manifest" => {
                            let mut raw: Value = serde_json::from_slice(
                                &std::fs::read(dir.join("manifest.json")).unwrap(),
                            )
                            .unwrap();
                            raw["draft_id"] = json!("misbound-draft");
                            std::fs::write(
                                dir.join("manifest.json"),
                                serde_json::to_vec(&raw).unwrap(),
                            )
                            .unwrap();
                        }
                        _ => {
                            std::fs::remove_file(dir.join("payload")).unwrap();
                            #[cfg(target_os = "linux")]
                            std::os::unix::fs::symlink(&external, dir.join("payload")).unwrap();
                        }
                    }
                }))
            });
            assert!(
                super::replay_document_create(
                    &state,
                    &auth_user(),
                    "actual",
                    &incoming_create_operation("corrupt-stage", "corrupt-stage")
                )
                .await
                .is_err(),
                "{kind}"
            );
            let n: i64 =
                sqlx::query_scalar("SELECT count(*) FROM documents WHERE id='corrupt-stage'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(n, 0);
            for table in [
                "storage_journal_receipts",
                "document_create_replay_receipts",
            ] {
                let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
                assert_eq!(n, 0);
            }
            let m = captured.lock().unwrap().clone().unwrap();
            assert!(crate::storage::data_dir()
                .join(".storage-journal")
                .join(&m.op_id)
                .exists());
            assert!(!crate::storage::data_dir().join(&m.path).exists());
            assert_eq!(std::fs::read(&outside).unwrap(), b"UNOWNED");
            assert_eq!(
                std::fs::read(crate::storage::data_dir().join(old)).unwrap(),
                b"PUBLIC OLD RAW\r\n"
            );
            std::fs::remove_file(outside).unwrap();
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn replay_create_postchecksum_same_bytes_payload_exchange_rejects_detached_inode() {
        let (state, _) = canonical_replay_fixture(false).await;
        let captured = std::sync::Arc::new(std::sync::Mutex::new(None));
        let store = captured.clone();
        crate::storage::journal::test_stage_callback(move |m| {
            *store.lock().unwrap() = Some(m.clone())
        });
        let exchange = captured.clone();
        crate::storage::journal::test_stage_verify_callback(move || {
            let m = exchange.lock().unwrap().clone().unwrap();
            let dir = crate::storage::data_dir()
                .join(".storage-journal")
                .join(&m.op_id);
            let bytes = std::fs::read(dir.join("payload")).unwrap();
            std::fs::rename(dir.join("payload"), dir.join("detached-evidence")).unwrap();
            std::fs::write(dir.join("payload"), bytes).unwrap();
        });
        assert!(super::replay_document_create(
            &state,
            &auth_user(),
            "actual",
            &incoming_create_operation("detached", "detached")
        )
        .await
        .is_err());
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM documents WHERE id='detached'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 0);
        let m = captured.lock().unwrap().clone().unwrap();
        assert!(crate::storage::data_dir()
            .join(".storage-journal")
            .join(m.op_id)
            .join("detached-evidence")
            .exists());
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM document_create_replay_receipts")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn replay_create_extra_draft_share_or_subscription_rolls_back_private_initial_audience() {
        for statement in ["INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('extra-draft','new-page','space-1','other-user','Unexpected','EXTRA PRIVATE BODY','base')", "INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) SELECT id,'other-user','viewer','user-1' FROM document_drafts WHERE document_id='new-page'", "INSERT INTO doc_subscriptions(document_id,user_id) VALUES('new-page','other-user')"] {
            let (state,_)=canonical_replay_fixture(false).await;sqlx::query(&format!("CREATE TRIGGER late_initial_audience AFTER INSERT ON document_create_replay_receipts BEGIN {statement}; END")).execute(&state.db).await.unwrap();assert!(super::replay_document_create(&state,&auth_user(),"actual",&incoming_create_operation("extra-audience","new-page")).await.is_err(),"{statement}");let n:i64=sqlx::query_scalar("SELECT count(*) FROM documents WHERE id='new-page'").fetch_one(&state.db).await.unwrap();assert_eq!(n,0);let n:i64=sqlx::query_scalar("SELECT count(*) FROM document_create_replay_receipts").fetch_one(&state.db).await.unwrap();assert_eq!(n,0);let n:i64=sqlx::query_scalar("SELECT count(*) FROM document_draft_access").fetch_one(&state.db).await.unwrap();assert_eq!(n,0);
        }
    }    async fn metadata_fixture(disk: bool) -> (AppState, String) {
        let (state, path) = canonical_replay_fixture(disk).await;
        sqlx::query("UPDATE documents SET title='Canonical',updated_at='2026-10-08T10:00:00+00:00',published_at='2026-10-08T09:00:00+00:00' WHERE id='doc-1'").execute(&state.db).await.unwrap();
        tokio::fs::write(crate::storage::data_dir().join(&path),"---\r\ntitle: \"Canonical\"\r\nicon: \"\"\r\nlocked: false\r\npublic: false\r\ncustom: |\r\n  preserve raw quoted \"data\"\r\n# exact comment\r\n---\r\n\r\nBODY raw 🦅\r\n".as_bytes()).await.unwrap();
        (state, path)
    }
    fn metadata_operation(id: &str, fields: Value, values: Value) -> SyncReplayOperation {
        let mut p = json!({"id":"doc-1","space_id":"space-1","updated_at":"2026-10-08T11:00:00.000Z","metadata_intent":{"version":1,"fields":fields,"source":{"page_updated_at":"2026-10-08T10:00:00.000Z","published_at":"2026-10-08T09:00:00Z"}}});
        p.as_object_mut()
            .unwrap()
            .extend(values.as_object().unwrap().clone());
        replay_operation(id, "2026-10-08T11:00:00Z", "document", "doc-1", "update", p)
    }
    #[test]
    fn metadata_header_preserves_body_custom_yaml_crlf_and_rejects_ambiguity() {
        let raw=b"---\r\ntitle: \"Old\"\r\nicon: \"file\"\r\ncustom: |\r\n  title: body remains\r\n# c\r\n---\r\nRAW\r\n";
        let out = super::metadata_markdown(
            raw,
            "New \"quoted\"",
            &None,
            &Some("raw:cover".into()),
            &["title", "icon", "cover_image"],
        )
        .unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("title: \"New \\\"quoted\\\"\"\r\n"));
        assert!(out.ends_with("custom: |\r\n  title: body remains\r\n# c\r\ncover_image: \"raw:cover\"\r\n---\r\nRAW\r\n"));
        for raw in [
            "---\ntitle: [\nicon: \"\"\n---\nBODY",
            "---\ntitle: \"Old\"\ntitle: \"Other\"\nicon: \"\"\n---\nBODY",
            "---\ntitle: |\n  Old\nicon: \"\"\n---\nBODY",
            "---\n\"title\": \"Old\"\nicon: \"\"\n---\nBODY",
        ] {
            assert!(
                super::metadata_markdown(
                    raw.as_bytes(),
                    "New",
                    &None,
                    &None,
                    &["title", "icon", "cover_image"]
                )
                .is_err(),
                "{raw}"
            );
        }
        let raw = "Headerless 🦅\r\nRAW exact";
        let out = super::metadata_markdown(raw.as_bytes(), "Header new", &None, &None, &["title"])
            .unwrap();
        assert!(out.ends_with(raw.as_bytes()));
        assert!(out.starts_with(b"---\r\ntitle: \"Header new\"\r\n"));
        let raw=b"---\r\ntitle: \"Old\\u0020title\"\r\nicon:   \"file\"\r\ncover_image: |\r\n  retain multiline cover\r\ncustom: true\r\n---\r\nBODY\r\n";
        let title = super::metadata_markdown(raw, "New", &None, &None, &["title"]).unwrap();
        assert_eq!(title,b"---\r\ntitle: \"New\"\r\nicon:   \"file\"\r\ncover_image: |\r\n  retain multiline cover\r\ncustom: true\r\n---\r\nBODY\r\n");
        let icon =
            super::metadata_markdown(raw, "SQL title", &Some("new".into()), &None, &["icon"])
                .unwrap();
        assert!(icon.starts_with(b"---\r\ntitle: \"Old\\u0020title\"\r\nicon: \"new\"\r\n"));
        assert!(icon.ends_with(
            b"cover_image: |\r\n  retain multiline cover\r\ncustom: true\r\n---\r\nBODY\r\n"
        ));
        assert!(super::metadata_same_revision(
            "2026-10-08T10:00:00.000Z",
            "2026-10-08T10:00:00+00:00"
        ));
        assert!(!super::metadata_same_revision(
            "2026-10-08T10:00:00.001Z",
            "2026-10-08T10:00:00Z"
        ));
    }
    #[tokio::test]
    async fn metadata_published_raw_presentation_receipt_never_reapplies_later_state() {
        let (state, old) = metadata_fixture(false).await;
        let raw = tokio::fs::read(crate::storage::data_dir().join(&old))
            .await
            .unwrap();
        let op = metadata_operation(
            " raw legacy metadata ID ",
            json!(["title", "icon", "cover_image"]),
            json!({"title":"New \"quoted\"","icon":null,"cover_image":"cover raw"}),
        );
        super::replay_document_update(&state, &auth_user(), "actual-provider", &op)
            .await
            .unwrap();
        let first = canonical_replay_source(&state).await;
        assert_ne!(first.0, old);
        assert_eq!(first.2.as_deref(), Some("2026-10-08T09:00:00+00:00"));
        assert_eq!(
            tokio::fs::read(crate::storage::data_dir().join(&old))
                .await
                .unwrap(),
            raw
        );
        let body = tokio::fs::read_to_string(crate::storage::data_dir().join(&first.0))
            .await
            .unwrap();
        assert!(body.ends_with("BODY raw 🦅\r\n"));
        assert!(body.contains("# exact comment\r\n"));
        sqlx::query(
            "UPDATE documents SET title='Later',updated_at='later',parent_id=NULL WHERE id='doc-1'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        super::replay_document_update(&state, &auth_user(), "actual-provider", &op)
            .await
            .unwrap();
        let title: String = sqlx::query_scalar("SELECT title FROM documents WHERE id='doc-1'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(title, "Later");
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM document_metadata_replay_receipts")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(n, 1);
        let mut collision = op.clone();
        collision.payload["title"] = json!("Collision");
        assert!(
            super::replay_document_update(&state, &auth_user(), "actual-provider", &collision)
                .await
                .is_err()
        );
        assert!(
            super::replay_document_update(&state, &auth_user(), "foreign-provider", &op)
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn metadata_headerless_and_malformed_canonical_preserve_queued_body_evidence() {
        for raw in [
            "Legacy body 🦅\r\nexact bytes",
            "---\ntitle: [\nicon: \"\"\n---\nSECRET RAW",
            "---\ntitle: \"Old\"\nunterminated",
        ] {
            let (s, old) = metadata_fixture(false).await;
            tokio::fs::write(crate::storage::data_dir().join(&old), raw.as_bytes())
                .await
                .unwrap();
            let op = metadata_operation(
                "header-case",
                json!(["title"]),
                json!({"title":"Header new"}),
            );
            let result = execute_replay_manifest(
                State(s.clone()),
                auth_user(),
                Json(single_operation_manifest(op)),
            )
            .await
            .unwrap()
            .0;
            assert_eq!(
                tokio::fs::read(crate::storage::data_dir().join(&old))
                    .await
                    .unwrap(),
                raw.as_bytes()
            );
            let receipts: i64 =
                sqlx::query_scalar("SELECT count(*) FROM document_metadata_replay_receipts")
                    .fetch_one(&s.db)
                    .await
                    .unwrap();
            if raw.starts_with("---") {
                assert_eq!(result.failed_operation_count, 1);
                assert_eq!(receipts, 0);
                assert_eq!(canonical_replay_source(&s).await.0, old);
                let ledger: String = sqlx::query_scalar(
                    "SELECT status FROM sync_replay_operations WHERE operation_id='header-case'",
                )
                .fetch_one(&s.db)
                .await
                .unwrap();
                assert_eq!(ledger, "failed");
            } else {
                assert_eq!(result.applied_operation_count, 1);
                assert_eq!(receipts, 1);
                let next = canonical_replay_source(&s).await.0;
                assert_ne!(next, old);
                let new = tokio::fs::read(crate::storage::data_dir().join(next))
                    .await
                    .unwrap();
                assert!(new.ends_with(raw.as_bytes()));
            }
        }
    }
    #[tokio::test]
    async fn metadata_placement_root_clear_parent_and_subtree_with_move_only_permission() {
        let (state, old) = metadata_fixture(false).await;
        seed_folder(&state.db, "folder-dest", None, "Dest", 0).await;
        seed_document(&state.db, "child", None, "Child").await;
        sqlx::query("UPDATE documents SET parent_id='doc-1' WHERE id='child'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('move-only','space-1','Mover','[\"view\",\"move_content\"]','user-1')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='move-only' WHERE user_id='user-1' AND space_id='space-1'").execute(&state.db).await.unwrap();
        let op = metadata_operation(
            "move-only",
            json!(["folder_id", "parent_id"]),
            json!({"folder_id":"folder-dest","parent_id":null}),
        );
        super::replay_document_update(&state, &auth_user(), "actual-provider", &op)
            .await
            .unwrap();
        let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT id,folder_id,parent_id FROM documents WHERE id IN('doc-1','child') ORDER BY id",
        )
        .fetch_all(&state.db)
        .await
        .unwrap();
        assert_eq!(
            rows,
            vec![
                (
                    "child".into(),
                    Some("folder-dest".into()),
                    Some("doc-1".into())
                ),
                ("doc-1".into(), Some("folder-dest".into()), None)
            ]
        );
        assert_eq!(canonical_replay_source(&state).await.0, old);
        let mut op = metadata_operation("edit-denied", json!(["title"]), json!({"title":"Denied"}));
        op.payload["metadata_intent"]["source"]["page_updated_at"] = json!("2026-10-08T11:00:00Z");
        assert!(
            super::replay_document_update(&state, &auth_user(), "actual-provider", &op)
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn metadata_private_owned_and_exact_draft_preserve_body_base_and_published_projection() {
        let (state, _) = metadata_fixture(false).await;
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES('owned','doc-1','space-1','user-1','Draft','RAW DRAFT','base','2026-10-08T10:01:00Z')").execute(&state.db).await.unwrap();
        let mut op = metadata_operation(
            "exact-draft",
            json!(["title", "icon"]),
            json!({"title":"Draft new","icon":null}),
        );
        op.payload["metadata_intent"]["draft"] = json!({"mode":"exact","id":"owned","updated_at":"2026-10-08T10:01:00+00:00","base_updated_at":"base"});
        super::replay_document_update(&state, &auth_user(), "actual-provider", &op)
            .await
            .unwrap();
        let title: String = sqlx::query_scalar("SELECT title FROM documents WHERE id='doc-1'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(title, "Canonical");
        let d = super::metadata_draft(&mut *state.db.acquire().await.unwrap(), "owned")
            .await
            .unwrap();
        assert_eq!(
            (
                d.title.as_str(),
                d.content.as_str(),
                d.base_updated_at.as_str()
            ),
            ("Draft new", "RAW DRAFT", "base")
        );
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='doc-1'")
            .execute(&state.db)
            .await
            .unwrap();
        let mut op = metadata_operation(
            "own-private",
            json!(["cover_image"]),
            json!({"cover_image":"Private cover"}),
        );
        op.payload["metadata_intent"]["source"]["published_at"] = Value::Null;
        super::replay_document_update(&state, &auth_user(), "actual-provider", &op)
            .await
            .unwrap();
        assert_eq!(
            super::metadata_draft(&mut *state.db.acquire().await.unwrap(), "owned")
                .await
                .unwrap()
                .content,
            "RAW DRAFT"
        );
        let mut op = metadata_operation("misbound-draft", json!(["title"]), json!({"title":"Bad"}));
        op.payload["metadata_intent"]["source"] =
            json!({"page_updated_at":"2026-10-08T11:00:00Z","published_at":null});
        op.payload["metadata_intent"]["draft"] =
            json!({"mode":"exact","id":"missing","updated_at":"old","base_updated_at":"base"});
        assert!(
            super::replay_document_update(&state, &auth_user(), "actual-provider", &op)
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn metadata_exact_shared_draft_editor_only_and_late_body_tamper_roll_back() {
        for role in ["viewer", "editor"] {
            let (s, old) = metadata_fixture(false).await;
            sqlx::query("UPDATE documents SET published_at=NULL WHERE id='doc-1'")
                .execute(&s.db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES('shared-metadata','doc-1','space-1','other-user','Shared title','SHARED EXACT BODY','base','2026-10-08T10:01:00Z')").execute(&s.db).await.unwrap();
            sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('shared-metadata','user-1',$1,'other-user')").bind(role).execute(&s.db).await.unwrap();
            let mut op = metadata_operation(
                "shared-metadata",
                json!(["title"]),
                json!({"title":"Edited shared"}),
            );
            op.payload["metadata_intent"]["source"]["published_at"] = Value::Null;
            op.payload["metadata_intent"]["draft"] = json!({"mode":"exact","id":"shared-metadata","updated_at":"2026-10-08T10:01:00Z","base_updated_at":"base"});
            let result = super::replay_document_update(&s, &auth_user(), "p", &op).await;
            let d = super::metadata_draft(&mut *s.db.acquire().await.unwrap(), "shared-metadata")
                .await
                .unwrap();
            assert_eq!(d.content, "SHARED EXACT BODY");
            assert_eq!(d.owner_id, "other-user");
            assert_eq!(d.base_updated_at, "base");
            if role == "viewer" {
                assert!(result.is_err());
                assert_eq!(d.title, "Shared title")
            } else {
                result.unwrap();
                assert_eq!(d.title, "Edited shared")
            }
            assert_eq!(canonical_replay_source(&s).await.0, old);
        }
        let (s, old) = metadata_fixture(false).await;
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES('owned-late','doc-1','space-1','user-1','Original draft','BODY ORIGINAL','base','2026-10-08T10:01:00Z')").execute(&s.db).await.unwrap();
        sqlx::query("CREATE TRIGGER metadata_draft_tamper AFTER INSERT ON document_metadata_replay_receipts BEGIN UPDATE document_drafts SET content='ROGUE BODY',title='Rogue' WHERE id='owned-late'; END").execute(&s.db).await.unwrap();
        let mut op = metadata_operation(
            "draft-tamper",
            json!(["title"]),
            json!({"title":"Intended"}),
        );
        op.payload["metadata_intent"]["draft"] = json!({"mode":"exact","id":"owned-late","updated_at":"2026-10-08T10:01:00Z","base_updated_at":"base"});
        assert!(super::replay_document_update(&s, &auth_user(), "p", &op)
            .await
            .is_err());
        let d = super::metadata_draft(&mut *s.db.acquire().await.unwrap(), "owned-late")
            .await
            .unwrap();
        assert_eq!(
            (d.title.as_str(), d.content.as_str()),
            ("Original draft", "BODY ORIGINAL")
        );
        assert_eq!(canonical_replay_source(&s).await.0, old);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM document_metadata_replay_receipts")
            .fetch_one(&s.db)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
    #[tokio::test]
    async fn metadata_strict_payload_and_late_source_authority_receipt_roll_back() {
        for bad in [
            json!({"title":null}),
            json!({"parent_id":3}),
            json!({"updated_at":5}),
            json!({"content":null}),
            json!({"space_id":null}),
        ] {
            let (s, old) = metadata_fixture(false).await;
            let mut op = metadata_operation("bad", json!(["title"]), json!({"title":"Safe"}));
            op.payload
                .as_object_mut()
                .unwrap()
                .extend(bad.as_object().unwrap().clone());
            assert!(super::replay_document_update(&s, &auth_user(), "p", &op)
                .await
                .is_err());
            assert_eq!(canonical_replay_source(&s).await.0, old);
            let n: i64 =
                sqlx::query_scalar("SELECT count(*) FROM document_metadata_replay_receipts")
                    .fetch_one(&s.db)
                    .await
                    .unwrap();
            assert_eq!(n, 0);
        }
        for statement in ["UPDATE users SET is_active=0 WHERE id='user-1'","DELETE FROM sessions WHERE id='test-session'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','user-1','user-1')","UPDATE documents SET title='Rogue' WHERE id='doc-1'","UPDATE documents SET locked=1,locked_by='other-user' WHERE id='doc-1'","UPDATE document_metadata_replay_receipts SET payload_fingerprint='rogue'"]{
        let(s,old)=metadata_fixture(false).await;sqlx::query(&format!("CREATE TRIGGER late_metadata AFTER INSERT ON document_metadata_replay_receipts BEGIN {statement}; END")).execute(&s.db).await.unwrap();let op=metadata_operation("late",json!(["title"]),json!({"title":"Safe"}));assert!(super::replay_document_update(&s,&auth_user(),"p",&op).await.is_err(),"{statement}");assert_eq!(canonical_replay_source(&s).await.0,old);let title:String=sqlx::query_scalar("SELECT title FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();assert_eq!(title,"Canonical");let n:i64=sqlx::query_scalar("SELECT count(*) FROM canonical_revision_receipts").fetch_one(&s.db).await.unwrap();assert_eq!(n,0);
      }
    }
    #[tokio::test]
    async fn metadata_real_http_write_key_presentation_only_and_read_denial() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;
        let (s, _) = metadata_fixture(false).await;
        sqlx::query("INSERT INTO global_roles(id,name,permissions) VALUES('present-only','Present','[\"view\",\"edit_content\"]')").execute(&s.db).await.unwrap();
        for q in ["DELETE FROM space_members WHERE user_id='user-1' AND space_id='space-1'","INSERT INTO groups(id,name) VALUES('metadata-group','Metadata')","INSERT INTO group_members(id,group_id,user_id) VALUES('metadata-member','metadata-group','user-1')","INSERT INTO group_global_roles(group_id,role_id) VALUES('metadata-group','present-only')"]{sqlx::query(q).execute(&s.db).await.unwrap();}
        let key = "metadata-real-write-key";
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('metadata-key','user-1','Metadata',$1,'prefix','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key(key)).execute(&s.db).await.unwrap();
        let app = super::api_router().with_state(s.clone());
        let request = |id: &str| {
            let op = metadata_operation(id, json!(["title"]), json!({"title":"HTTP presentation"}));
            Request::builder().method("POST").uri("/sync/replay").header("X-API-Key",key).header("content-type","application/json").body(Body::from({let c=contract();json!({"schema_version":1,"provider":"actual-provider","operation_count":1,"recovery_checkpoint_count":0,"recovery_checkpoints":[],"replay_contract":{"schema_version":c.schema_version,"operation_order":c.operation_order,"idempotency_key":c.idempotency_key,"transport":c.transport,"server_backends":c.server_backends,"required_payload_status":c.required_payload_status,"conflict_policy":c.conflict_policy,"web_canonical_store":c.web_canonical_store},"operations":[{"id":op.id,"entity_type":op.entity_type,"entity_id":op.entity_id,"action":op.action,"queued_at":op.queued_at,"payload_status":op.payload_status,"payload":op.payload}]}).to_string()})).unwrap()
        };
        let r = app
            .clone()
            .oneshot(request("write-positive"))
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        let b: Value = serde_json::from_slice(
            &axum::body::to_bytes(r.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(b["applied_operation_count"], 1, "{b}");
        seed_folder(&s.db, "mixed-folder", None, "Mixed", 0).await;
        let mut mixed = metadata_operation(
            "mixed-denied",
            json!(["title", "folder_id"]),
            json!({"title":"Mixed blocked","folder_id":"mixed-folder"}),
        );
        mixed.payload["metadata_intent"]["source"]["page_updated_at"] =
            json!("2026-10-08T11:00:00Z");
        assert!(
            super::replay_document_update(&s, &folder_dispatch_auth(&s).await, "p", &mixed)
                .await
                .is_err()
        );
        let title: String = sqlx::query_scalar("SELECT title FROM documents WHERE id='doc-1'")
            .fetch_one(&s.db)
            .await
            .unwrap();
        assert_eq!(title, "HTTP presentation");
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='metadata-key'")
            .execute(&s.db)
            .await
            .unwrap();
        assert_eq!(
            app.oneshot(request("read-negative"))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    async fn metadata_explicit_unchanged_placement_requires_actual_move_write_and_lock_policy() {
        for scenario in ["view-only", "locked", "move"] {
            let (s, old) = metadata_fixture(false).await;
            let permissions = if scenario == "view-only" {
                "[\"view\"]"
            } else {
                "[\"view\",\"move_content\"]"
            };
            sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('metadata-placement-role','space-1','Placement',$1,'user-1')").bind(permissions).execute(&s.db).await.unwrap();
            sqlx::query("UPDATE space_members SET role='metadata-placement-role' WHERE user_id='user-1' AND space_id='space-1'").execute(&s.db).await.unwrap();
            if scenario == "locked" {
                sqlx::query(
                    "UPDATE documents SET locked=1,locked_by='other-user' WHERE id='doc-1'",
                )
                .execute(&s.db)
                .await
                .unwrap();
            }
            let key = "metadata-placement-write";
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('metadata-placement-key','user-1','Placement',$1,'prefix','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key(key)).execute(&s.db).await.unwrap();
            let request = axum::http::Request::builder()
                .method("POST")
                .header("X-API-Key", key)
                .body(())
                .unwrap();
            let (mut parts, _) = request.into_parts();
            let auth = <AuthUser as axum::extract::FromRequestParts<AppState>>::from_request_parts(
                &mut parts, &s,
            )
            .await
            .unwrap();
            let before = crate::spaces::documents::tests::move_database_snapshot(&s).await;
            let op = metadata_operation(
                "unchanged-placement",
                json!(["folder_id", "parent_id"]),
                json!({"folder_id":null,"parent_id":null}),
            );
            let result = super::replay_document_update(&s, &auth, "actual-provider", &op).await;
            if scenario == "move" {
                result.unwrap();
                assert_eq!(canonical_replay_source(&s).await.0, old);
                let receipt: super::MetadataReceipt = serde_json::from_str(
                    &sqlx::query_scalar::<_, String>(
                        "SELECT receipt_json FROM document_metadata_replay_receipts",
                    )
                    .fetch_one(&s.db)
                    .await
                    .unwrap(),
                )
                .unwrap();
                assert_eq!(receipt.before, receipt.after);
                assert!(receipt.rewrite.is_none());
            } else {
                assert!(result.is_err(), "{scenario}");
                assert_eq!(
                    crate::spaces::documents::tests::move_database_snapshot(&s).await,
                    before,
                    "{scenario}"
                );
            }
        }
    }
    #[tokio::test]
    async fn metadata_wal_old_path_before_commit_and_current_credentials_after_wait() {
        for deny in [false, true] {
            let (s, old) = metadata_fixture(true).await;
            let db = s.db.clone();
            let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
            if deny {
                super::METADATA_BEFORE_LOCK.with(|hook| {
                    *hook.borrow_mut() = Some(Box::pin(async move {
                        sqlx::query("DELETE FROM sessions WHERE id='test-session'")
                            .execute(&db)
                            .await
                            .unwrap();
                    }))
                });
            } else {
                let seen = seen.clone();
                super::METADATA_BEFORE_FINAL.with(|hook| {
                    *hook.borrow_mut() = Some(Box::pin(async move {
                        let row: (String, String) = sqlx::query_as(
                            "SELECT content_path,title FROM documents WHERE id='doc-1'",
                        )
                        .fetch_one(&db)
                        .await
                        .unwrap();
                        *seen.lock().unwrap() = Some(row);
                    }))
                });
            }
            let op = metadata_operation("wal", json!(["title"]), json!({"title":"New visible"}));
            let result = super::replay_document_update(&s, &auth_user(), "p", &op).await;
            if deny {
                assert!(result.is_err());
                assert_eq!(canonical_replay_source(&s).await.0, old)
            } else {
                result.unwrap();
                assert_eq!(
                    seen.lock().unwrap().as_ref(),
                    Some(&(old.clone(), "Canonical".to_string()))
                );
                assert_ne!(canonical_replay_source(&s).await.0, old);
            }
        }
    }
    #[tokio::test]
    async fn metadata_lost_ack_rollback_and_unknown_proof_retain_truthful_receipts() {
        for fault in [
            super::ReplayCanonicalCommitFault::LostAck,
            super::ReplayCanonicalCommitFault::Rollback,
            super::ReplayCanonicalCommitFault::UnknownProof,
        ] {
            let (s, old) = metadata_fixture(false).await;
            super::METADATA_COMMIT_FAULT.with(|f| *f.borrow_mut() = Some(fault));
            if fault == super::ReplayCanonicalCommitFault::UnknownProof {
                let db = s.db.clone();
                super::METADATA_BEFORE_CONFIRM.with(|h| {
                    *h.borrow_mut() = Some(Box::pin(async move {
                        sqlx::query(
                            "UPDATE document_metadata_replay_receipts SET receipt_json='broken'",
                        )
                        .execute(&db)
                        .await
                        .unwrap();
                    }))
                });
            }
            let op = metadata_operation("fault", json!(["title"]), json!({"title":"Fault new"}));
            let r = super::replay_document_update(&s, &auth_user(), "p", &op).await;
            match fault {
                super::ReplayCanonicalCommitFault::LostAck => assert_eq!(r.unwrap(), "applied"),
                super::ReplayCanonicalCommitFault::Rollback => {
                    assert!(r.is_err());
                    assert_eq!(canonical_replay_source(&s).await.0, old)
                }
                super::ReplayCanonicalCommitFault::UnknownProof => {
                    assert!(r.unwrap_err().contains("indeterminate"));
                    assert_ne!(canonical_replay_source(&s).await.0, old)
                }
            }
        }
        let (s, _) = metadata_fixture(false).await;
        let op = metadata_operation("unproven", json!(["title"]), json!({"title":"Lost"}));
        let manifest = single_operation_manifest(op.clone());
        super::insert_replay_ledger(&s, &auth_user(), &manifest, &op, "applied", None)
            .await
            .unwrap();
        let response = execute_replay_manifest(State(s.clone()), auth_user(), Json(manifest))
            .await
            .unwrap()
            .0;
        assert_eq!(response.failed_operation_count, 1);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM document_metadata_replay_receipts")
            .fetch_one(&s.db)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
    #[tokio::test]
    async fn metadata_final_source_file_and_expired_original_jwt_fail_without_sql_commit() {
        let (s, old) = metadata_fixture(false).await;
        let root = crate::storage::data_dir();
        super::METADATA_BEFORE_FINAL.with(|h| {
            *h.borrow_mut() = Some(Box::pin(async move {
                let dirs = std::fs::read_dir(root.join(".canonical-rewrite-journal")).unwrap();
                for entry in dirs {
                    let path = entry.unwrap().path().join("manifest.json");
                    if let Ok(raw) = std::fs::read(&path) {
                        if let Ok(v) = serde_json::from_slice::<Value>(&raw) {
                            if v["incoming"]["operation_id"] == "file-drift" {
                                let path = v["path"].as_str().unwrap();
                                std::fs::write(root.join(path), b"Rogue bytes").unwrap();
                            }
                        }
                    }
                }
            }))
        });
        let op = metadata_operation("file-drift", json!(["title"]), json!({"title":"Changed"}));
        assert!(super::replay_document_update(&s, &auth_user(), "p", &op)
            .await
            .is_err());
        assert_eq!(canonical_replay_source(&s).await.0, old);
        let (s, old) = metadata_fixture(false).await;
        let mut claims = auth_user().claims;
        claims.exp = (chrono::Utc::now() + chrono::Duration::seconds(2)).timestamp();
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes()),
        )
        .unwrap();
        let request = axum::http::Request::builder()
            .method("POST")
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();
        let auth = <AuthUser as axum::extract::FromRequestParts<AppState>>::from_request_parts(
            &mut parts, &s,
        )
        .await
        .unwrap();
        super::METADATA_BEFORE_FINAL.with(|h| {
            *h.borrow_mut() = Some(Box::pin(async move {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }))
        });
        let op = metadata_operation("expired", json!(["title"]), json!({"title":"Changed"}));
        assert!(super::replay_document_update(&s, &auth, "p", &op)
            .await
            .is_err());
        assert_eq!(canonical_replay_source(&s).await.0, old);
    }
    #[test]
    fn document_delete_preflight_accepts_only_consumed_legacy_empty_delete_payload() {
        let op = document_delete_operation("empty-harddelete", Value::Null);
        assert!(super::validate_replay_manifest(single_operation_manifest(op.clone())).ready);
        for action in ["create", "update", "update_content"] {
            let mut other = op.clone();
            other.action = action.into();
            assert!(!super::validate_replay_manifest(single_operation_manifest(other)).ready);
        }
        let mut missing = op;
        missing.payload_status = "missing".into();
        assert!(!super::validate_replay_manifest(single_operation_manifest(missing)).ready);
    }
    #[tokio::test]
    async fn document_delete_actual_cascades_consume_private_drafts_shares_binary_history_and_drawing_rows_only(
    ) {
        let (state, slug, _) = document_delete_fixture(false).await;
        let drawing = format!("spaces/{slug}/docs/retained.excalidraw");
        tokio::fs::write(
            crate::storage::data_dir().join(&drawing),
            b"RETAINED DRAWING",
        )
        .await
        .unwrap();
        for sql in [
            "INSERT INTO doc_versions(id,document_id,version_number,change_set,created_by) VALUES('delete-history','target-a',1,X'000102','owner-a')",
            "INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('delete-draft','target-a','space-a','editor-a','Other private draft','PRIVATE SQL BODY','old')",
            "INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('delete-draft','viewer-a','viewer','editor-a')",
            "INSERT INTO comments(id,document_id,user_id,content) VALUES('delete-comment','target-a','owner-a','comment')",
            "INSERT INTO comments(id,document_id,parent_id,user_id,content) VALUES('delete-reply','target-a','delete-comment','editor-a','reply')",
            "INSERT INTO reactions(id,document_id,user_id,emoji) VALUES('delete-reaction','target-a','viewer-a','star')",
            "INSERT INTO doc_subscriptions(document_id,user_id) VALUES('target-a','viewer-a')",
        ] {sqlx::query(sql).execute(&state.db).await.unwrap();}
        sqlx::query("INSERT INTO excalidraw_files(id,document_id,space_id,filename,storage_path,created_by) VALUES('delete-drawing','target-a','space-a','drawing',$1,'editor-a')").bind(&drawing).execute(&state.db).await.unwrap();
        let op = witnessed_delete_operation(&state).await;
        assert_eq!(
            super::replay_document_delete(
                &state,
                &crate::spaces::properties::tests::test_auth("owner-a"),
                "actual",
                &op
            )
            .await
            .unwrap(),
            "applied"
        );
        for table in [
            "doc_versions",
            "document_drafts",
            "document_draft_access",
            "comments",
            "reactions",
            "doc_subscriptions",
            "excalidraw_files",
        ] {
            let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(count, 0, "{table}");
        }
        assert_eq!(
            tokio::fs::read(crate::storage::data_dir().join(&drawing))
                .await
                .unwrap(),
            b"RETAINED DRAWING"
        );
        let receipt: String =
            sqlx::query_scalar("SELECT receipt_json FROM document_delete_replay_receipts")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert!(!receipt.contains("PRIVATE SQL BODY"));
        assert!(!receipt.contains("Other private draft"));
        delete_cleanup(&slug).await;
    }
    #[tokio::test]
    async fn document_delete_lost_ack_recreated_id_keeps_actual_new_search_entry() {
        let (state, slug, _) = document_delete_fixture(false).await;
        let op = witnessed_delete_operation(&state).await;
        DELETE_COMMIT_FAULT.with(|s| *s.borrow_mut() = Some(ReplayCanonicalCommitFault::LostAck));
        let later = state.clone();
        DELETE_BEFORE_CONFIRM.with(|s|*s.borrow_mut()=Some(Box::pin(async move {
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','Recreatedsearch','','owner-a','owner-a','later')").execute(&later.db).await.unwrap();
            later.search_index.upsert(crate::search::index::DocEntry{doc_id:"target-a".into(),space_id:"space-a".into(),title:"Recreatedsearch".into(),body:"Later body".into(),tags:"".into()}).unwrap();
        })));
        let refusal = super::replay_document_delete(
            &state, &crate::spaces::properties::tests::test_auth("owner-a"), "actual", &op
        ).await.unwrap_err();
        assert!(refusal.contains("indeterminate") && refusal.contains("retain queued deletion"), "{refusal}");
        let hits = state
            .search_index
            .snapshot("Recreatedsearch")
            .unwrap()
            .page(0, 10)
            .unwrap();
        assert!(hits
            .iter()
            .any(|h| h.doc_id == "target-a" && h.title == "Recreatedsearch"));
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT title FROM documents WHERE id='target-a'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            "Recreatedsearch"
        );
        delete_cleanup(&slug).await;
    }
    #[tokio::test]
    async fn document_delete_nonobject_payloads_never_receive_historical_ack() {
        let (state, slug, _) = document_delete_fixture(false).await;
        let auth = crate::spaces::properties::tests::test_auth("owner-a");
        for payload in [json!(false), json!(42), json!("legacy"), json!([])] {
            let op = document_delete_operation(&uuid::Uuid::new_v4().to_string(), payload);
            let before = delete_domain_snapshot(&state).await;
            assert!(super::replay_document_delete(&state, &auth, "actual", &op)
                .await
                .is_err());
            assert!(
                super::replay_document_delete_historical_receipt(&state, &auth, "actual", &op)
                    .await
                    .is_err()
            );
            assert_eq!(delete_domain_snapshot(&state).await, before);
        }
        delete_cleanup(&slug).await;
    }
    async fn document_delete_fixture(wal: bool) -> (AppState, String, String) {
        let mut state = crate::spaces::properties::tests::property_test_state().await;
        if wal {
            let file = format!(
                "/tmp/starlane-document-delete-{}.sqlite",
                uuid::Uuid::new_v4()
            );
            state.db = sqlx::any::AnyPoolOptions::new()
                .max_connections(2)
                .after_connect(|c, _| {
                    Box::pin(async move {
                        sqlx::query("PRAGMA foreign_keys=ON")
                            .execute(&mut *c)
                            .await?;
                        sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;
                        Ok(())
                    })
                })
                .connect(&format!("sqlite://{file}?mode=rwc"))
                .await
                .unwrap();
            sqlx::query("PRAGMA journal_mode=WAL")
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
            for user in ["owner-a", "editor-a", "viewer-a"] {
                sqlx::query(
                    "INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')",
                )
                .bind(user)
                .bind(format!("{user}@example.test"))
                .execute(&state.db)
                .await
                .unwrap();
                sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'test','2999-01-01T00:00:00Z')").bind(format!("metadata-{user}")).bind(user).execute(&state.db).await.unwrap();
            }
            for space in ["space-a", "space-b"] {
                sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,$1,$1,'private','owner-a')").bind(space).execute(&state.db).await.unwrap();
            }
            for (user, role) in [
                ("owner-a", "owner"),
                ("editor-a", "editor"),
                ("viewer-a", "viewer"),
            ] {
                sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'space-a',$2,$3)").bind(format!("member-{user}")).bind(user).bind(role).execute(&state.db).await.unwrap();
            }
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','Original','','owner-a','owner-a','published')").execute(&state.db).await.unwrap();
        }
        let slug = format!("delete-retained-{}", uuid::Uuid::new_v4());
        sqlx::query("UPDATE spaces SET slug=$1 WHERE id='space-a'")
            .bind(&slug)
            .execute(&state.db)
            .await
            .unwrap();
        let path = format!("spaces/{slug}/docs/original.md");
        tokio::fs::create_dir_all(crate::storage::space_dir(&slug).join("docs"))
            .await
            .unwrap();
        tokio::fs::write(
            crate::storage::data_dir().join(&path),
            b"RETAINED EXACT ORIGINAL BODY",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE documents SET content_path=$1 WHERE id='target-a'")
            .bind(&path)
            .execute(&state.db)
            .await
            .unwrap();
        let blob = format!("spaces/{slug}/docs/retained-asset.bin");
        tokio::fs::write(crate::storage::data_dir().join(&blob), b"RETAINED BLOB")
            .await
            .unwrap();
        sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES('delete-asset','target-a','space-a','blob','blob','application/octet-stream',13,$1,'owner-a')").bind(&blob).execute(&state.db).await.unwrap();
        (state, slug, path)
    }
    fn document_delete_operation(id: &str, payload: Value) -> SyncReplayOperation {
        replay_operation(
            id,
            "2026-10-08T00:00:00Z",
            "document",
            "target-a",
            "delete",
            payload,
        )
    }
    async fn witnessed_delete_operation(state:&AppState)->SyncReplayOperation {
      let row:(String,Option<String>)=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE id='target-a'").fetch_one(&state.db).await.unwrap();
      document_delete_operation(&uuid::Uuid::new_v4().to_string(),json!({"id":"target-a","space_id":"space-a","original_target":{"created_at":row.0,"deleted_at":row.1}}))
    }
    #[tokio::test]
    async fn witnessed_sync_delete_commits_original_outbox_and_exact_retry() {
      let(state,slug,_)=document_delete_fixture(false).await;
      let auth=crate::spaces::properties::tests::test_auth("owner-a");let op=witnessed_delete_operation(&state).await;
      assert_eq!(super::replay_document_delete(&state,&auth,"actual",&op).await.unwrap(),"applied");
      let raw:String=sqlx::query_scalar("SELECT plan_json FROM attachment_erasure_outbox WHERE operation_id=$1").bind(&op.id).fetch_one(&state.db).await.unwrap();
      assert_eq!(serde_json::from_str::<Value>(&raw).unwrap()["original_target"],op.payload["original_target"]);
      assert_eq!(super::replay_document_delete(&state,&auth,"actual",&op).await.unwrap(),"skipped");
      assert_eq!(super::replay_document_delete(&state,&auth,"other-provider",&op).await.unwrap(),"skipped");
      let mut changed=op.clone();changed.payload["original_target"]["created_at"]=json!("changed");
      assert!(super::replay_document_delete(&state,&auth,"actual",&changed).await.is_err());
      sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('target-a','space-a','NEW','','owner-a','owner-a')").execute(&state.db).await.unwrap();
      assert!(super::replay_document_delete(&state,&auth,"actual",&op).await.is_err());
      assert_eq!(sqlx::query_scalar::<_,String>("SELECT plan_json FROM attachment_erasure_outbox WHERE operation_id=$1").bind(&op.id).fetch_one(&state.db).await.unwrap(),raw);
      delete_cleanup(&slug).await;
    }
    #[tokio::test]
    async fn witnessed_sync_delete_legacy_changed_target_and_viewer_preserve_source() {
      let(state,slug,_)=document_delete_fixture(false).await;let auth=crate::spaces::properties::tests::test_auth("owner-a");
      let op=witnessed_delete_operation(&state).await;let before=delete_domain_snapshot(&state).await;
      let mut legacy=op.clone();legacy.payload=Value::Null;
      assert!(super::replay_document_delete(&state,&auth,"actual",&legacy).await.is_err());
      let mut stale=op.clone();stale.payload["original_target"]["deleted_at"]=json!("different-cycle");
      assert!(super::replay_document_delete(&state,&auth,"actual",&stale).await.is_err());
      assert!(super::replay_document_delete(&state,&crate::spaces::properties::tests::test_auth("viewer-a"),"actual",&op).await.is_err());
      assert_eq!(delete_domain_snapshot(&state).await,before);delete_cleanup(&slug).await;
    }
    #[tokio::test]
    async fn witnessed_sync_delete_lost_ack_confirms_only_original_absent_target() {
      let(state,slug,_)=document_delete_fixture(false).await;let auth=crate::spaces::properties::tests::test_auth("owner-a");let op=witnessed_delete_operation(&state).await;
      DELETE_COMMIT_FAULT.with(|s|*s.borrow_mut()=Some(ReplayCanonicalCommitFault::LostAck));
      assert_eq!(super::replay_document_delete(&state,&auth,"actual",&op).await.unwrap(),"applied");
      assert_eq!(super::replay_document_delete(&state,&auth,"actual",&op).await.unwrap(),"skipped");delete_cleanup(&slug).await;
    }
    async fn delete_cleanup(slug: &str) {
        tokio::fs::remove_dir_all(crate::storage::space_dir(slug))
            .await
            .unwrap();
    }
    async fn delete_domain_snapshot(state: &AppState) -> Vec<(String, Option<String>)> {
        crate::spaces::documents::tests::move_database_snapshot(state)
            .await
            .into_iter()
            .filter(|(table, _)| !table.starts_with("sync_replay_"))
            .collect()
    }
    #[tokio::test]
    async fn document_delete_lifecycle_locked_targets_retain_files_and_receipt_retry_keeps_recreated_id(
    ) {
        for lifecycle in ["active", "archive", "trash"] {
            let (state, slug, path) = document_delete_fixture(false).await;
            sqlx::query("UPDATE documents SET locked=1,locked_by='viewer-a',archived_at=$1,deleted_at=$2 WHERE id='target-a'").bind(if lifecycle=="archive"{Some("old")}else{None}).bind(if lifecycle=="trash"{Some("old")}else{None}).execute(&state.db).await.unwrap();
            let op = witnessed_delete_operation(&state).await;
            let auth = crate::spaces::properties::tests::test_auth("owner-a");
            assert_eq!(
                super::replay_document_delete(&state, &auth, "actual", &op)
                    .await
                    .unwrap(),
                "applied"
            );
            assert_eq!(
                tokio::fs::read(crate::storage::data_dir().join(&path))
                    .await
                    .unwrap(),
                b"RETAINED EXACT ORIGINAL BODY"
            );
            assert_eq!(
                tokio::fs::read(crate::storage::space_dir(&slug).join("docs/retained-asset.bin"))
                    .await
                    .unwrap(),
                b"RETAINED BLOB"
            );
            let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attachments")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(n, 0);
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','RECREATED LATER','later','owner-a','owner-a','new')").execute(&state.db).await.unwrap();
            let before = delete_domain_snapshot(&state).await;
            let refusal = super::replay_document_delete(&state, &auth, "actual", &op).await.unwrap_err();
            assert!(refusal.contains("review"), "Recreated identifier must require named review: {refusal}");
            assert_eq!(delete_domain_snapshot(&state).await, before);
            let mut changed = op.clone();
            changed.payload = json!({"space_id":"space-a"});
            assert!(
                super::replay_document_delete(&state, &auth, "actual", &changed)
                    .await
                    .is_err()
            );
            delete_cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn document_delete_scope_children_and_corrupt_foreign_consumers_fail_whole_state() {
        for scenario in [
            "foreign-space",
            "malformed-space",
            "child",
            "foreign-asset",
            "foreign-comment",
            "foreign-property",
        ] {
            let (state, slug, _) = document_delete_fixture(false).await;
            let mut op = witnessed_delete_operation(&state).await;
            match scenario {
                "foreign-space" => op.payload["space_id"] = json!("space-b"),
                "malformed-space" => op.payload["space_id"] = json!(null),
                "child" => {
                    sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,parent_id,published_at) VALUES('dependent','space-a','Child','','owner-a','owner-a','target-a','old')").execute(&state.db).await.unwrap();
                }
                "foreign-asset" => {
                    sqlx::query("UPDATE attachments SET space_id='space-b'")
                        .execute(&state.db)
                        .await
                        .unwrap();
                }
                "foreign-property" => {
                    sqlx::query("INSERT INTO property_values(id,document_id,property_id,value) VALUES('corrupt','target-a','foreign','value')").execute(&state.db).await.unwrap();
                }
                _ => {
                    sqlx::query("INSERT INTO comments(id,document_id,user_id,content) VALUES('parent-comment','target-a','owner-a','source'),('foreign-comment','target-b','owner-a','foreign')").execute(&state.db).await.unwrap();
                    sqlx::query(
                        "UPDATE comments SET parent_id='parent-comment' WHERE id='foreign-comment'",
                    )
                    .execute(&state.db)
                    .await
                    .unwrap();
                }
            }
            let before = delete_domain_snapshot(&state).await;
            let files = crate::spaces::documents::tests::move_file_snapshot(&slug);
            assert!(
                super::replay_document_delete(
                    &state,
                    &crate::spaces::properties::tests::test_auth("owner-a"),
                    "actual",
                    &op
                )
                .await
                .is_err(),
                "{scenario}"
            );
            assert_eq!(delete_domain_snapshot(&state).await, before, "{scenario}");
            assert_eq!(
                crate::spaces::documents::tests::move_file_snapshot(&slug),
                files
            );
            delete_cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn document_delete_current_private_participation_and_exact_permission_not_edit_proxy() {
        let (state, slug, _) = document_delete_fixture(false).await;
        sqlx::query("UPDATE documents SET published_at=NULL,deleted_at='old' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('delete-private','target-a','space-a','editor-a','Only editor','PRIVATE','old')").execute(&state.db).await.unwrap();
        let op = witnessed_delete_operation(&state).await;
        let before = delete_domain_snapshot(&state).await;
        assert!(super::replay_document_delete(
            &state,
            &crate::spaces::properties::tests::test_auth("owner-a"),
            "actual",
            &op
        )
        .await
        .is_err());
        assert_eq!(delete_domain_snapshot(&state).await, before);
        sqlx::query("UPDATE document_drafts SET owner_id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        // A global granular role can authorize deletion without EditContent.
        sqlx::query("DELETE FROM space_members WHERE user_id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES('delete-group','Delete group')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('delete-member','delete-group','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO global_roles(id,name,permissions) VALUES('delete-only','Delete only','[\"delete_content\"]')").execute(&state.db).await.unwrap();
        sqlx::query(
            "INSERT INTO group_global_roles(group_id,role_id) VALUES('delete-group','delete-only')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let write =
            crate::spaces::properties::tests::metadata_key(&state, "owner-a", r#"["write"]"#).await;
        assert_eq!(
            super::replay_document_delete(&state, &write, "actual", &op)
                .await
                .unwrap(),
            "applied"
        );
        delete_cleanup(&slug).await;
    }
    #[tokio::test]
    async fn document_delete_receipt_and_delete_triggers_rollback_policy_source_and_cascades() {
        for (timing,sql) in [
            ("receipt","DELETE FROM sessions WHERE id='metadata-owner-a'"),
            ("receipt","UPDATE documents SET title='DRIFT' WHERE id='target-a'"),
            ("receipt","UPDATE document_delete_replay_receipts SET source_fingerprint='corrupt'"),
            ("receipt","INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,parent_id) VALUES('late-child','space-a','child','','owner-a','owner-a','target-a')"),
            ("delete","UPDATE users SET is_active=0 WHERE id='owner-a'"),
            ("delete","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','owner-a')"),
            ("delete","INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('late-page','space-a','changed','','owner-a','owner-a')"),
        ] {
            let (state,slug,_)=document_delete_fixture(false).await;
            let op=witnessed_delete_operation(&state).await;
            let event=if timing=="receipt"{"INSERT ON document_delete_replay_receipts"}else{"DELETE ON documents"};
            sqlx::query(&format!("CREATE TRIGGER delete_drift AFTER {event} BEGIN {sql}; END")).execute(&state.db).await.unwrap();
            let before=delete_domain_snapshot(&state).await;let files=crate::spaces::documents::tests::move_file_snapshot(&slug);
            assert!(super::replay_document_delete(&state,&crate::spaces::properties::tests::test_auth("owner-a"),"actual",&op).await.is_err(),"{timing}/{sql}");
            assert_eq!(delete_domain_snapshot(&state).await,before);assert_eq!(crate::spaces::documents::tests::move_file_snapshot(&slug),files);
            delete_cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn document_delete_unknown_commit_rollback_receipt_and_unavailable_keep_files() {
        for fault in [
            ReplayCanonicalCommitFault::Rollback,
            ReplayCanonicalCommitFault::LostAck,
            ReplayCanonicalCommitFault::UnknownProof,
        ] {
            let (state, slug, path) = document_delete_fixture(false).await;
            let op = witnessed_delete_operation(&state).await;
            DELETE_COMMIT_FAULT.with(|s| *s.borrow_mut() = Some(fault));
            if fault == ReplayCanonicalCommitFault::UnknownProof {
                let db = state.db.clone();
                DELETE_BEFORE_CONFIRM.with(|s| {
                    *s.borrow_mut() = Some(Box::pin(async move {
                        db.close().await;
                    }))
                });
            }
            let result = super::replay_document_delete(
                &state,
                &crate::spaces::properties::tests::test_auth("owner-a"),
                "actual",
                &op,
            )
            .await;
            assert_eq!(result.is_ok(), fault == ReplayCanonicalCommitFault::LostAck);
            assert_eq!(
                tokio::fs::read(crate::storage::data_dir().join(&path))
                    .await
                    .unwrap(),
                b"RETAINED EXACT ORIGINAL BODY"
            );
            if fault == ReplayCanonicalCommitFault::Rollback {
                let n: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE id='target-a'")
                        .fetch_one(&state.db)
                        .await
                        .unwrap();
                assert_eq!(n, 1);
                let n: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM document_delete_replay_receipts")
                        .fetch_one(&state.db)
                        .await
                        .unwrap();
                assert_eq!(n, 0);
            }
            delete_cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn document_delete_guard_wait_uses_external_wal_current_actor_and_space() {
        for mutation in [
            "UPDATE users SET is_active=0 WHERE id='owner-a'",
            "UPDATE documents SET space_id='space-b' WHERE id='target-a'",
        ] {
            let (state, slug, _) = document_delete_fixture(true).await;
            let op = witnessed_delete_operation(&state).await;
            let mut hold = state.db.begin().await.unwrap();
            crate::spaces::mutation_guards::lock_audience_for_mutation(&mut hold)
                .await
                .unwrap();
            let cloned = state.clone();
            let task = tokio::spawn(async move {
                super::replay_document_delete(
                    &cloned,
                    &crate::spaces::properties::tests::test_auth("owner-a"),
                    "actual",
                    &op,
                )
                .await
            });
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!task.is_finished());
            sqlx::query(mutation).execute(&mut *hold).await.unwrap();
            hold.commit().await.unwrap();
            assert!(task.await.unwrap().is_err());
            let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE id='target-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(n, 1);
            delete_cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn document_delete_legacy_ledger_never_acknowledges_unproven_absence_or_recreated_page() {
        let (state, slug, _) = document_delete_fixture(false).await;
        let op = document_delete_operation(&uuid::Uuid::new_v4().to_string(), Value::Null);
        let mut manifest = single_operation_manifest(op.clone());
        manifest.provider = "actual".into();
        let auth = crate::spaces::properties::tests::test_auth("owner-a");
        super::insert_replay_ledger(&state, &auth, &manifest, &op, "applied", None)
            .await
            .unwrap();
        assert_eq!(
            super::execute_replay_operation(&state, &auth, &manifest, &op)
                .await
                .status,
            "failed"
        );
        let fresh = witnessed_delete_operation(&state).await;
        let mut m = single_operation_manifest(fresh.clone());
        m.provider = "actual".into();
        assert_eq!(
            super::execute_replay_operation(&state, &auth, &m, &fresh)
                .await
                .status,
            "applied"
        );
        super::update_replay_ledger(&state, &fresh.id, "pending", None)
            .await
            .unwrap();
        assert_eq!(
            super::execute_replay_operation(&state, &auth, &m, &fresh)
                .await
                .status,
            "skipped"
        );
        let mut unknown = fresh.clone();
        unknown.id = uuid::Uuid::new_v4().to_string();
        assert!(
            super::replay_document_delete(&state, &auth, "actual", &unknown)
                .await
                .is_err()
        );
        delete_cleanup(&slug).await;
    }

    #[tokio::test]
    async fn document_delete_final_original_jwt_expiration_rolls_back_after_all_source_proofs() {
        let (state, slug, _) = document_delete_fixture(false).await;
        let mut auth = crate::spaces::properties::tests::test_auth("owner-a");
        let op = witnessed_delete_operation(&state).await;
        auth.claims.exp = chrono::Utc::now().timestamp() + 2;
        let deadline = auth.claims.exp;
        DELETE_BEFORE_IDENTITY.with(|s| {
            *s.borrow_mut() = Some(Box::pin(async move {
                while chrono::Utc::now().timestamp() <= deadline {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            }))
        });
        let before = delete_domain_snapshot(&state).await;
        let files = crate::spaces::documents::tests::move_file_snapshot(&slug);
        assert!(super::replay_document_delete(
            &state,
            &auth,
            "actual",
            &op
        )
        .await
        .is_err());
        assert_eq!(delete_domain_snapshot(&state).await, before);
        assert_eq!(
            crate::spaces::documents::tests::move_file_snapshot(&slug),
            files
        );
        delete_cleanup(&slug).await;
    }
    #[tokio::test]
    async fn document_delete_actual_http_write_key_read_denial_and_late_key_rollback() {
        use axum::{
            body::Body,
            http::{Request, StatusCode},
        };
        use tower::ServiceExt;
        for scenario in [
            "write",
            "read",
            "late",
            "edit-only",
            "service",
            "mixed-service",
        ] {
            let (state, slug, _) = document_delete_fixture(false).await;
            let raw = "actual-document-delete-key";
            let scope = if scenario == "read" {
                r#"["read"]"#
            } else {
                r#"["write"]"#
            };
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('delete-http-key','owner-a','Delete',$1,'prefix',$2)").bind(crate::auth::api_keys::hash_api_key(raw)).bind(scope).execute(&state.db).await.unwrap();
            if scenario == "late" {
                sqlx::query("CREATE TRIGGER delete_key_drift AFTER INSERT ON document_delete_replay_receipts BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='delete-http-key'; END").execute(&state.db).await.unwrap();
            }
            if scenario == "edit-only" {
                sqlx::query("DELETE FROM space_members WHERE user_id='owner-a'")
                    .execute(&state.db)
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO groups(id,name) VALUES('only-editor','Editor')")
                    .execute(&state.db)
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('only-editor-member','only-editor','owner-a')").execute(&state.db).await.unwrap();
                sqlx::query("INSERT INTO global_roles(id,name,permissions) VALUES('only-edit-role','Edit only','[\"edit_content\"]')").execute(&state.db).await.unwrap();
                sqlx::query("INSERT INTO group_global_roles(group_id,role_id) VALUES('only-editor','only-edit-role')").execute(&state.db).await.unwrap();
            }
            if scenario.ends_with("service") {
                sqlx::query("UPDATE api_keys SET is_service_account=1,scopes='[\"admin\"]' WHERE id='delete-http-key'").execute(&state.db).await.unwrap();
            }
            let op = witnessed_delete_operation(&state).await;
            let c = contract();
            let mut manifest = json!({"schema_version":1,"provider":"actual","operation_count":1,"replay_contract":{"schema_version":c.schema_version,"operation_order":c.operation_order,"idempotency_key":c.idempotency_key,"transport":c.transport,"server_backends":c.server_backends,"required_payload_status":c.required_payload_status,"conflict_policy":c.conflict_policy,"web_canonical_store":c.web_canonical_store},"operations":[{"id":op.id,"entity_type":op.entity_type,"entity_id":op.entity_id,"action":op.action,"queued_at":op.queued_at,"payload_status":op.payload_status,"payload":op.payload}]});
            if scenario == "mixed-service" {
                let mut additional = manifest["operations"][0].clone();
                additional["id"] = json!("zz-mixed");
                additional["entity_type"] = json!("tag");
                additional["entity_id"] = json!("new-mixed-tag");
                additional["action"] = json!("create");
                additional["payload"] =
                    json!({"id":"new-mixed-tag","space_id":"space-a","name":"Must not create"});
                manifest["operations"]
                    .as_array_mut()
                    .unwrap()
                    .push(additional);
                manifest["operation_count"] = json!(2);
            }
            let before = delete_domain_snapshot(&state).await;
            let request = Request::builder()
                .method("POST")
                .uri("/sync/replay")
                .header("X-API-Key", raw)
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&manifest).unwrap()))
                .unwrap();
            let response = super::api_router()
                .with_state(state.clone())
                .oneshot(request)
                .await
                .unwrap();
            if scenario == "read" {
                assert_eq!(response.status(), StatusCode::FORBIDDEN);
            } else if scenario.ends_with("service") {
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            } else {
                assert_eq!(response.status(), StatusCode::OK);
                let body: Value = serde_json::from_slice(
                    &axum::body::to_bytes(response.into_body(), usize::MAX)
                        .await
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(
                    body[if scenario == "write" {
                        "applied_operation_count"
                    } else {
                        "failed_operation_count"
                    }],
                    1,
                    "{scenario}: {body}"
                );
            }
            if scenario.ends_with("service") {
                sqlx::query("UPDATE api_keys SET last_used_at=NULL WHERE id='delete-http-key'")
                    .execute(&state.db)
                    .await
                    .unwrap();
                assert_eq!(delete_domain_snapshot(&state).await, before);
            }
            let n: i64 = sqlx::query_scalar("SELECT count(*) FROM documents WHERE id='target-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(n, if scenario == "write" { 0 } else { 1 });
            assert_eq!(
                tokio::fs::read(crate::storage::space_dir(&slug).join("docs/retained-asset.bin"))
                    .await
                    .unwrap(),
                b"RETAINED BLOB"
            );
            if scenario == "late" {
                let scopes: String =
                    sqlx::query_scalar("SELECT scopes FROM api_keys WHERE id='delete-http-key'")
                        .fetch_one(&state.db)
                        .await
                        .unwrap();
                assert_eq!(scopes, r#"["write"]"#);
            }
            delete_cleanup(&slug).await;
        }
    }
    async fn draft_save_incoming_fixture()->(AppState,String,AuthUser,SyncReplayOperation) {
        use axum::extract::FromRequestParts;
        let(state,old)=canonical_replay_fixture(true).await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='doc-1'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at,updated_at) VALUES('save-draft','doc-1','space-1','user-1','Original draft','Original body','source-revision','created','initial')").execute(&state.db).await.unwrap();
        let raw=crate::spaces::draft_save::selected(&mut state.db.acquire().await.unwrap(),"space-1","doc-1","save-draft").await.unwrap();
        let mut op=canonical_replay_operation("draft-incoming","FIRST SAVE");op.payload["draft_id"]=json!("save-draft");op.payload["source_digest"]=json!(crate::spaces::draft_save::digest(&raw).unwrap());
        let claims=auth_user().claims;let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();
        let(mut parts,_)=axum::http::Request::builder().method("POST").header("Authorization",format!("Bearer {token}")).body(axum::body::Body::empty()).unwrap().into_parts();
        let auth=AuthUser::from_request_parts(&mut parts,&state).await.unwrap();
        (state,old,auth,op)
    }
    #[tokio::test]
    async fn draft_save_exact_incoming_receipt_ack_never_replays_newer_deleted_or_recreated_draft() {
        let(s,old,a,op)=draft_save_incoming_fixture().await;
        super::replay_document_update_content(&s,&a,"actual-provider",&op).await.unwrap();
        sqlx::query("UPDATE document_drafts SET content='NEWER PRIVATE',title='Newer title',updated_at='later' WHERE id='save-draft'").execute(&s.db).await.unwrap();
        let before=delete_domain_snapshot(&s).await;assert!(super::replay_body_historical_receipt(&s,&a,"actual-provider",&op).await.unwrap());assert_eq!(super::replay_document_update_content(&s,&a,"actual-provider",&op).await.unwrap(),"applied");assert_eq!(delete_domain_snapshot(&s).await,before);
        sqlx::query("DELETE FROM document_drafts WHERE id='save-draft'").execute(&s.db).await.unwrap();let before=delete_domain_snapshot(&s).await;assert!(super::replay_body_historical_receipt(&s,&a,"actual-provider",&op).await.unwrap());assert_eq!(delete_domain_snapshot(&s).await,before);
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('save-draft','doc-1','space-1','user-1','Recreated','RECREATED BYTES','newbase')").execute(&s.db).await.unwrap();let before=delete_domain_snapshot(&s).await;assert!(super::replay_body_historical_receipt(&s,&a,"actual-provider",&op).await.unwrap());assert_eq!(delete_domain_snapshot(&s).await,before);
        assert_eq!(tokio::fs::read(crate::storage::data_dir().join(&old)).await.unwrap(),b"PUBLIC OLD RAW\r\n");
        let slug:String=sqlx::query_scalar("SELECT slug FROM spaces WHERE id='space-1'").fetch_one(&s.db).await.unwrap();s.db.close().await;tokio::fs::remove_dir_all(crate::storage::space_dir(&slug)).await.unwrap();
    }
    #[tokio::test]
    async fn draft_save_payload_provider_actor_or_missing_original_cannot_borrow_applied_ledger_ack() {
        let(s,old,a,op)=draft_save_incoming_fixture().await;
        let manifest=single_operation_manifest(op.clone());super::insert_replay_ledger(&s,&a,&manifest,&op,"pending",None).await.unwrap();
        super::replay_document_update_content(&s,&a,"self-hosted",&op).await.unwrap();super::update_replay_ledger(&s,&op.id,"applied",None).await.unwrap();let before=delete_domain_snapshot(&s).await;
        for variant in ["content","digest","missing-draft","provider"] {
            let mut changed=op.clone();let mut manifest=single_operation_manifest(changed.clone());
            match variant {"content"=>changed.payload["content"]=json!("wrong"),"digest"=>changed.payload["source_digest"]=json!("f".repeat(64)),"missing-draft"=>{changed.payload.as_object_mut().unwrap().remove("draft_id");},"provider"=>manifest.provider="another-provider".into(),_=>unreachable!()};
            let result=super::execute_replay_operation(&s,&a,&manifest,&changed).await;assert_eq!(result.status,"failed","{variant}");assert_eq!(delete_domain_snapshot(&s).await,before);
        }
        let mut changed=op.clone();changed.id="unproven-legacy".into();changed.payload.as_object_mut().unwrap().remove("source_digest");let m=single_operation_manifest(changed.clone());super::insert_replay_ledger(&s,&a,&m,&changed,"applied",None).await.unwrap();assert_eq!(super::execute_replay_operation(&s,&a,&m,&changed).await.status,"failed");
        sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES('receipt-other','receipt-other@test','Other','member')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('receipt-other-session','receipt-other','hash','2999-01-01T00:00:00Z')").execute(&s.db).await.unwrap();
        let mut claims=auth_user().claims;claims.sub="receipt-other".into();claims.sid="receipt-other-session".into();
        let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes())).unwrap();
        let(mut parts,_)=axum::http::Request::builder().method("POST").header("Authorization",format!("Bearer {token}")).body(axum::body::Body::empty()).unwrap().into_parts();
        use axum::extract::FromRequestParts;
        let other=AuthUser::from_request_parts(&mut parts,&s).await.unwrap();
        crate::spaces::access::require_current_human_preference_write(&mut s.db.acquire().await.unwrap(),&other).await.unwrap();
        assert!(super::replay_body_historical_receipt(&s,&other,"self-hosted",&op).await.is_err());
        assert_eq!(tokio::fs::read(crate::storage::data_dir().join(old)).await.unwrap(),b"PUBLIC OLD RAW\r\n");
    }
    #[tokio::test]
    async fn draft_save_committed_receipt_confirms_pending_ledger_after_lost_completion_only() {
        let(s,_old,a,op)=draft_save_incoming_fixture().await;let m=single_operation_manifest(op.clone());
        sqlx::query("CREATE TRIGGER ledger_lost_completion BEFORE UPDATE OF status ON sync_replay_operations WHEN NEW.status='applied' BEGIN SELECT RAISE(ABORT,'completion unavailable'); END").execute(&s.db).await.unwrap();
        assert_eq!(super::execute_replay_operation(&s,&a,&m,&op).await.status,"applied");
        let status:String=sqlx::query_scalar("SELECT status FROM sync_replay_operations WHERE operation_id=$1").bind(&op.id).fetch_one(&s.db).await.unwrap();assert_eq!(status,"pending");
        sqlx::query("UPDATE document_drafts SET content='NEWER AFTER UNKNOWN',updated_at='newer' WHERE id='save-draft'").execute(&s.db).await.unwrap();let before=delete_domain_snapshot(&s).await;
        sqlx::query("DROP TRIGGER ledger_lost_completion").execute(&s.db).await.unwrap();assert_eq!(super::execute_replay_operation(&s,&a,&m,&op).await.status,"skipped");assert_eq!(delete_domain_snapshot(&s).await,before);
    }

}

#[cfg(test)]
mod template_replay_atomic_tests {
    use super::*;
    use crate::spaces::routes::catalog_tests::{auth, catalog_state, key};
    use serde_json::json;
    fn op(id: &str, entity: &str, action: &str, payload: Value) -> SyncReplayOperation {
        SyncReplayOperation {
            id: id.into(),
            entity_type: "template".into(),
            entity_id: entity.into(),
            action: action.into(),
            payload,
            queued_at: "2026-10-07T00:00:00Z".into(),
            payload_status: "available".into(),
        }
    }
    fn payload(id: &str) -> Value {
        json!({"id":id,"space_id":"space-a","name":"  Replay  ","content":"RAW\r\nBODY","description":"raw description","properties":[1,{"x":null}],"tags":["a",7,null,{"legacy":true}],"created_at":"2026-10-01T01:02:03Z","updated_at":"2026-10-02T01:02:03Z","created_by":"viewer-a"})
    }
    async fn apply(s: &AppState, a: &AuthUser, o: &SyncReplayOperation) -> Result<String, String> {
        replay_template_crud(s, a, "real-provider", o).await
    }
    async fn snapshot(s: &AppState) -> Vec<(String, Option<String>)> {
        sqlx::query_as("SELECT 'template:'||id,name||'|'||space_id||'|'||content||'|'||COALESCE(description,'<null>')||'|'||COALESCE(properties,'<null>')||'|'||COALESCE(tags,'<null>')||'|'||created_by||'|'||created_at||'|'||updated_at FROM templates UNION ALL SELECT 'draft:'||id,snapshot FROM template_drafts UNION ALL SELECT 'revision:'||id,snapshot FROM template_revisions UNION ALL SELECT 'receipt:'||incoming_id,receipt_json FROM template_replay_receipts ORDER BY 1").fetch_all(&s.db).await.unwrap()
    }
    #[tokio::test]
    async fn template_replay_raw_metadata_ids_timestamps_and_update_attribution_survive() {
        let s = catalog_state(false).await;
        let a = auth("editor-a");
        let id = "legacy:/ raw-template";
        let c = op("raw-create", id, "create", payload(id));
        assert_eq!(apply(&s, &a, &c).await.unwrap(), "applied");
        let row:(String,String,String,String,String,String,String)=sqlx::query_as("SELECT name,content,properties,tags,created_by,created_at,updated_at FROM templates WHERE id=$1").bind(id).fetch_one(&s.db).await.unwrap();
        assert_eq!(
            row,
            (
                "Replay".into(),
                "RAW\r\nBODY".into(),
                "[1,{\"x\":null}]".into(),
                "[\"a\",7,null,{\"legacy\":true}]".into(),
                "editor-a".into(),
                "2026-10-01T01:02:03+00:00".into(),
                "2026-10-02T01:02:03+00:00".into()
            )
        );
        let mut v = payload(id);
        v["content"] = json!("UPDATED");
        v["created_at"] = json!("ignored invalid creation stamp");
        v["properties"] = Value::Null;
        v["tags"] = Value::Null;
        v.as_object_mut().unwrap().remove("description");
        let u = op("raw-update", id, "update", v);
        apply(&s, &auth("owner-a"), &u).await.unwrap();
        let r: (Option<String>, Option<String>, String, String, String) = sqlx::query_as(
            "SELECT description,properties,tags,created_by,created_at FROM templates WHERE id=$1",
        )
        .bind(id)
        .fetch_one(&s.db)
        .await
        .unwrap();
        assert_eq!(r, (None, None, "[]".into(), "editor-a".into(), row.5));
        let no_times = op(
            "raw-default",
            "raw-default",
            "create",
            json!({"space_id":"space-a","name":"D","content":""}),
        );
        apply(&s, &a, &no_times).await.unwrap();
        let (c, u): (String, String) =
            sqlx::query_as("SELECT created_at,updated_at FROM templates WHERE id='raw-default'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(c, u);
    }
    #[tokio::test]
    async fn template_replay_malformed_description_body_and_tags_fail_without_domain_writes() {
        for (field, value) in [
            ("description", json!(7)),
            ("description", json!({})),
            ("content", Value::Null),
            ("content", json!(false)),
            ("name", json!([])),
            ("tags", json!({"not":"array"})),
        ] {
            let s = catalog_state(false).await;
            let before = snapshot(&s).await;
            let mut v = payload("bad");
            v[field] = value;
            assert!(apply(&s, &auth("editor-a"), &op("bad", "bad", "create", v))
                .await
                .is_err());
            assert_eq!(snapshot(&s).await, before);
        }
    }
    #[tokio::test]
    async fn template_replay_foreign_ids_and_scoped_absent_delete_never_false_ack() {
        let s = catalog_state(false).await;
        let a = auth("editor-a");
        for action in ["create", "update", "delete"] {
            let mut v = payload("foreign-template");
            v["space_id"] = json!("space-a");
            let before = snapshot(&s).await;
            let e = apply(
                &s,
                &a,
                &op(&format!("foreign-{action}"), "foreign-template", action, v),
            )
            .await
            .unwrap_err();
            assert!(e.contains("unavailable"));
            assert_eq!(snapshot(&s).await, before);
        }
        for v in [json!({}), json!({"space_id":""})] {
            assert!(apply(&s, &a, &op("no-context", "absent", "delete", v))
                .await
                .is_err());
        }
        let absent = op("absent", "absent", "delete", json!({"space_id":"space-a"}));
        assert!(apply(&s, &auth("viewer-a"), &absent).await.is_err());
        assert_eq!(apply(&s, &a, &absent).await.unwrap(), "skipped");
        let d = op(
            "delete-existing",
            "template-a",
            "delete",
            json!({"space_id":"space-a"}),
        );
        assert_eq!(apply(&s, &a, &d).await.unwrap(), "applied");
    }
    #[tokio::test]
    async fn template_replay_actual_key_role_union_and_service_policy() {
        let s = catalog_state(false).await;
        let c = op(
            "key-create",
            "key-template",
            "create",
            payload("key-template"),
        );
        let read = key(&s, "replay-read", Some("editor-a"), "[\"read\"]").await;
        assert!(apply(&s, &read, &c).await.is_err());
        let service = key(&s, "replay-service", None, "[\"admin\"]").await;
        assert!(apply(&s, &service, &c).await.is_err());
        let write = key(&s, "replay-write", Some("editor-a"), "[\"write\"]").await;
        assert_eq!(apply(&s, &write, &c).await.unwrap(), "applied");
        sqlx::query("DELETE FROM space_members WHERE user_id='editor-a'")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES('replay-group','Replay')")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('replay-member','replay-group','editor-a')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO global_roles(id,name,permissions) VALUES('replay-role','Replay','[\"edit_content\"]')").execute(&s.db).await.unwrap();
        sqlx::query(
            "INSERT INTO group_global_roles(group_id,role_id) VALUES('replay-group','replay-role')",
        )
        .execute(&s.db)
        .await
        .unwrap();
        let u = op(
            "key-update",
            "key-template",
            "update",
            payload("key-template"),
        );
        assert!(apply(&s, &write, &u).await.is_err());
        sqlx::query(
            "UPDATE global_roles SET permissions='[\"manage_templates\"]' WHERE id='replay-role'",
        )
        .execute(&s.db)
        .await
        .unwrap();
        assert_eq!(apply(&s, &write, &u).await.unwrap(), "applied");
        sqlx::query(
            "UPDATE api_keys SET expires_at='2000-01-01T00:00:00Z' WHERE id='replay-write'",
        )
        .execute(&s.db)
        .await
        .unwrap();
        assert!(apply(
            &s,
            &write,
            &op(
                "expired",
                "key-template",
                "delete",
                json!({"space_id":"space-a"})
            )
        )
        .await
        .is_err());
    }
    async fn seed_draft(s: &AppState, space: &str) {
        sqlx::query("INSERT INTO template_drafts(id,template_id,space_id,owner_id,snapshot) VALUES('replay-draft','template-a',$1,'owner-a','RAW SNAPSHOT')").bind(space).execute(&s.db).await.unwrap();
    }
    #[tokio::test]
    async fn template_replay_update_preserves_history_and_delete_rejects_foreign_consumers() {
        let s = catalog_state(false).await;
        seed_draft(&s, "space-a").await;
        let revisions: Vec<(String, String)> = sqlx::query_as(
            "SELECT id,snapshot FROM template_revisions WHERE template_id='template-a' ORDER BY id",
        )
        .fetch_all(&s.db)
        .await
        .unwrap();
        apply(
            &s,
            &auth("editor-a"),
            &op(
                "update-history",
                "template-a",
                "update",
                payload("template-a"),
            ),
        )
        .await
        .unwrap();
        assert_eq!(sqlx::query_as::<_,(String,String)>("SELECT id,snapshot FROM template_revisions WHERE template_id='template-a' ORDER BY id").fetch_all(&s.db).await.unwrap(),revisions);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT snapshot FROM template_drafts WHERE id='replay-draft'"
            )
            .fetch_one(&s.db)
            .await
            .unwrap(),
            "RAW SNAPSHOT"
        );
        sqlx::query("UPDATE template_drafts SET space_id='space-b' WHERE id='replay-draft'")
            .execute(&s.db)
            .await
            .unwrap();
        let before = snapshot(&s).await;
        for action in ["update", "delete"] {
            assert!(apply(
                &s,
                &auth("editor-a"),
                &op(
                    &format!("corrupt-{action}"),
                    "template-a",
                    action,
                    payload("template-a")
                )
            )
            .await
            .is_err());
            assert_eq!(snapshot(&s).await, before);
        }
        sqlx::query("UPDATE template_drafts SET space_id='space-a' WHERE id='replay-draft'")
            .execute(&s.db)
            .await
            .unwrap();
        apply(
            &s,
            &auth("editor-a"),
            &op(
                "delete-history",
                "template-a",
                "delete",
                json!({"space_id":"space-a"}),
            ),
        )
        .await
        .unwrap();
        let n:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM template_drafts WHERE template_id='template-a')+(SELECT COUNT(*) FROM template_revisions WHERE template_id='template-a')").fetch_one(&s.db).await.unwrap();
        assert_eq!(n, 0);
    }
    #[tokio::test]
    async fn template_replay_last_sql_policy_source_and_receipt_waits_roll_back() {
        for sql in ["DELETE FROM sessions WHERE user_id='editor-a'", "UPDATE templates SET content='TAMPER' WHERE id='template-a'", "UPDATE template_replay_receipts SET receipt_json='{}' WHERE incoming_id='late'", "INSERT INTO template_drafts(id,template_id,space_id,owner_id,snapshot) VALUES('unexpected','template-a','space-b','owner-a','FOREIGN')"] {
            let s=catalog_state(false).await;let before=snapshot(&s).await;let sessions:Vec<String>=sqlx::query_scalar("SELECT id FROM sessions ORDER BY id").fetch_all(&s.db).await.unwrap();
            sqlx::query(&format!("CREATE TRIGGER late_template AFTER INSERT ON template_replay_receipts BEGIN {sql}; END")).execute(&s.db).await.unwrap();
            assert!(apply(&s,&auth("editor-a"),&op("late","template-a","update",payload("template-a"))).await.is_err());assert_eq!(snapshot(&s).await,before);assert_eq!(sqlx::query_scalar::<_,String>("SELECT id FROM sessions ORDER BY id").fetch_all(&s.db).await.unwrap(),sessions);
        }
    }
    #[tokio::test]
    async fn template_replay_wal_space_wait_uses_current_credential_and_canonical_source() {
        let s = catalog_state(true).await;
        let mut blocker = s.db.begin().await.unwrap();
        sqlx::query("UPDATE spaces SET id=id WHERE id='space-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE user_id='editor-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let copy = s.clone();
        let task = tokio::spawn(async move {
            apply(
                &copy,
                &auth("editor-a"),
                &op("wait", "template-a", "update", payload("template-a")),
            )
            .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!task.is_finished());
        blocker.commit().await.unwrap();
        assert!(task.await.unwrap().is_err());
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT content FROM templates WHERE id='template-a'")
                .fetch_one(&s.db)
                .await
                .unwrap(),
            "TEMPLATE SECRET"
        );
        let s = catalog_state(true).await;
        let mut blocker = s.db.begin().await.unwrap();
        sqlx::query("UPDATE spaces SET id=id WHERE id='space-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        sqlx::query("UPDATE templates SET created_by='viewer-a',created_at='newer-source' WHERE id='template-a'").execute(&mut *blocker).await.unwrap();
        let copy = s.clone();
        let task = tokio::spawn(async move {
            apply(
                &copy,
                &auth("editor-a"),
                &op("current", "template-a", "update", payload("template-a")),
            )
            .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!task.is_finished());
        blocker.commit().await.unwrap();
        task.await.unwrap().unwrap();
        let r: (String, String) =
            sqlx::query_as("SELECT created_by,created_at FROM templates WHERE id='template-a'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(r, ("viewer-a".into(), "newer-source".into()));
    }
    #[tokio::test]
    async fn template_replay_historical_receipts_never_reapply_later_metadata_or_reused_ids() {
        let s = catalog_state(false).await;
        let a = auth("editor-a");
        let c = op("history-create", "history", "create", payload("history"));
        apply(&s, &a, &c).await.unwrap();
        sqlx::query("UPDATE templates SET content='LATER' WHERE id='history'")
            .execute(&s.db)
            .await
            .unwrap();
        assert_eq!(apply(&s, &a, &c).await.unwrap(), "skipped");
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT content FROM templates WHERE id='history'")
                .fetch_one(&s.db)
                .await
                .unwrap(),
            "LATER"
        );
        let d = op(
            "history-delete",
            "history",
            "delete",
            json!({"space_id":"space-a"}),
        );
        apply(&s, &a, &d).await.unwrap();
        sqlx::query("INSERT INTO templates(id,space_id,name,content,created_by) VALUES('history','space-b','Reused','KEEP','owner-a')").execute(&s.db).await.unwrap();
        assert_eq!(apply(&s, &a, &d).await.unwrap(), "skipped");
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT content FROM templates WHERE id='history'")
                .fetch_one(&s.db)
                .await
                .unwrap(),
            "KEEP"
        );
        assert!(replay_template_crud(&s, &a, "other-provider", &c)
            .await
            .is_err());
        let mut changed = c.clone();
        changed.payload["content"] = json!("CHANGED");
        assert!(apply(&s, &a, &changed).await.is_err());
        assert!(apply(&s, &auth("owner-a"), &c).await.is_err());
    }
    #[tokio::test]
    async fn template_replay_receipt_insert_failure_rolls_back_entire_template_change() {
        let s = catalog_state(false).await;
        let before = snapshot(&s).await;
        sqlx::query("CREATE TRIGGER receipt_failure BEFORE INSERT ON template_replay_receipts BEGIN SELECT RAISE(ABORT,'injected receipt failure'); END").execute(&s.db).await.unwrap();
        assert!(apply(
            &s,
            &auth("editor-a"),
            &op("fail-receipt", "new", "create", payload("new"))
        )
        .await
        .is_err());
        assert_eq!(snapshot(&s).await, before);
    }

    fn manifest(o: SyncReplayOperation) -> SyncReplayManifestRequest {
        SyncReplayManifestRequest {
            schema_version: 1,
            provider: "real-provider".into(),
            replay_contract: SyncReplayContract {
                schema_version: 1,
                operation_order: "queued_at_ascending".into(),
                idempotency_key: "operation.id".into(),
                transport: "server-api".into(),
                server_backends: vec!["sqlite".into(), "postgresql".into()],
                required_payload_status: "available".into(),
                conflict_policy: "server_validates_version_and_permissions".into(),
                web_canonical_store: "server-api".into(),
            },
            operation_count: 1,
            recovery_checkpoint_count: 0,
            recovery_checkpoints: vec![],
            draft_bindings: None,
            operations: vec![o],
        }
    }
    #[tokio::test]
    async fn template_replay_generic_ledger_ack_requires_atomic_receipt_and_pending_heals() {
        let s = catalog_state(false).await;
        let a = auth("editor-a");
        let c = op(
            "ledger-create",
            "ledger-template",
            "create",
            payload("ledger-template"),
        );
        let m = manifest(c.clone());
        assert_eq!(
            execute_replay_operation(&s, &a, &m, &c).await.status,
            "applied"
        );
        sqlx::query("UPDATE templates SET content='LATER' WHERE id='ledger-template'")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE sync_replay_operations SET status='pending' WHERE operation_id='ledger-create'",
        )
        .execute(&s.db)
        .await
        .unwrap();
        assert_eq!(
            execute_replay_operation(&s, &a, &m, &c).await.status,
            "skipped"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT content FROM templates WHERE id='ledger-template'"
            )
            .fetch_one(&s.db)
            .await
            .unwrap(),
            "LATER"
        );
        let fake = op(
            "legacy-unproven",
            "template-a",
            "update",
            payload("template-a"),
        );
        let fm = manifest(fake.clone());
        insert_replay_ledger(&s, &a, &fm, &fake, "applied", None)
            .await
            .unwrap();
        let before = snapshot(&s).await;
        assert_eq!(
            execute_replay_operation(&s, &a, &fm, &fake).await.status,
            "failed"
        );
        assert_eq!(snapshot(&s).await, before);
        let mut collision = c.clone();
        collision.payload["content"] = json!("COLLISION");
        assert_eq!(
            execute_replay_operation(&s, &a, &m, &collision)
                .await
                .status,
            "failed"
        );
    }
    #[tokio::test]
    async fn template_replay_lost_ack_rollback_and_unavailable_commit_proof_are_truthful() {
        for fault in [
            ReplayCanonicalCommitFault::Rollback,
            ReplayCanonicalCommitFault::LostAck,
            ReplayCanonicalCommitFault::UnknownProof,
        ] {
            let s = catalog_state(false).await;
            let c = op("uncertain", "uncertain", "create", payload("uncertain"));
            TEMPLATE_REPLAY_COMMIT_FAULT.with(|h| *h.borrow_mut() = Some(fault));
            if fault == ReplayCanonicalCommitFault::UnknownProof {
                let copy = s.clone();
                TEMPLATE_REPLAY_BEFORE_CONFIRM.with(|h|*h.borrow_mut()=Some(Box::pin(async move{sqlx::query("ALTER TABLE template_replay_receipts RENAME TO unavailable_template_receipts").execute(&copy.db).await.unwrap();})));
            }
            let result = apply(&s, &auth("editor-a"), &c).await;
            assert_eq!(result.is_ok(), fault == ReplayCanonicalCommitFault::LostAck);
            let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM templates WHERE id='uncertain'")
                .fetch_one(&s.db)
                .await
                .unwrap();
            assert_eq!(
                n,
                if fault == ReplayCanonicalCommitFault::Rollback {
                    0
                } else {
                    1
                }
            );
            if fault == ReplayCanonicalCommitFault::UnknownProof {
                assert!(result.unwrap_err().contains("indeterminate"));
                sqlx::query(
                    "ALTER TABLE unavailable_template_receipts RENAME TO template_replay_receipts",
                )
                .execute(&s.db)
                .await
                .unwrap();
                assert_eq!(apply(&s, &auth("editor-a"), &c).await.unwrap(), "skipped");
            }
        }
    }
    #[tokio::test]
    async fn template_replay_http_actual_write_only_key_succeeds_read_key_denies() {
        use tower::ServiceExt;
        let s = catalog_state(false).await;
        for (id, scope, expected) in [
            ("replay-http-write", "write", axum::http::StatusCode::OK),
            (
                "replay-http-read",
                "read",
                axum::http::StatusCode::FORBIDDEN,
            ),
        ] {
            let _ = key(&s, id, Some("editor-a"), &format!("[\"{scope}\"]")).await;
            sqlx::query("UPDATE api_keys SET key_hash=$1 WHERE id=$2")
                .bind(crate::auth::api_keys::hash_api_key(id))
                .bind(id)
                .execute(&s.db)
                .await
                .unwrap();
            let body = json!({"schema_version":1,"provider":"real-provider","replay_contract":{"schema_version":1,"operation_order":"queued_at_ascending","idempotency_key":"operation.id","transport":"server-api","server_backends":["sqlite","postgresql"],"required_payload_status":"available","conflict_policy":"server_validates_version_and_permissions","web_canonical_store":"server-api"},"operation_count":1,"operations":[{"id":id,"entity_type":"template","entity_id":id,"action":"create","queued_at":"2026-10-07T00:00:00Z","payload_status":"available","payload":payload(id)}]});
            let request = axum::http::Request::builder()
                .method("POST")
                .uri("/sync/replay")
                .header("X-API-Key", id)
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap();
            let r = api_router()
                .with_state(s.clone())
                .oneshot(request)
                .await
                .unwrap();
            assert_eq!(r.status(), expected);
            if expected == axum::http::StatusCode::OK {
                let b = axum::body::to_bytes(r.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let v: Value = serde_json::from_slice(&b).unwrap();
                assert_eq!(v["applied_operation_count"], 1);
            }
        }
    }

    #[tokio::test]
    async fn template_replay_supplied_identity_is_strict_without_breaking_absent_legacy_ids() {
        for value in [
            Value::Null,
            json!(7),
            json!({}),
            json!(""),
            json!("  "),
            json!("different"),
        ] {
            let s = catalog_state(false).await;
            let before = snapshot(&s).await;
            let mut v = payload("strict-id");
            v["id"] = value;
            assert!(apply(
                &s,
                &auth("editor-a"),
                &op("strict-id", "strict-id", "create", v)
            )
            .await
            .is_err());
            assert_eq!(snapshot(&s).await, before);
        }
        let s = catalog_state(false).await;
        let id = "raw:/ absent-id";
        let mut v = payload(id);
        v.as_object_mut().unwrap().remove("id");
        assert_eq!(
            apply(
                &s,
                &auth("editor-a"),
                &op("absent-payload-id", id, "create", v)
            )
            .await
            .unwrap(),
            "applied"
        );
        let c = op("absent-payload-id", id, "create", {
            let mut v = payload(id);
            v.as_object_mut().unwrap().remove("id");
            v
        });
        assert_eq!(apply(&s, &auth("editor-a"), &c).await.unwrap(), "skipped");
    }
}

#[cfg(test)]
mod space_creation_topology_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use crate::spaces::routes::space_creation_fence_tests::{admin, fail_insert, setup};
    fn operation(space: &str, name: &str) -> SyncReplayOperation {
        serde_json::from_value(serde_json::json!({"id":"creation-topology-op","entity_type":"space","entity_id":space,"action":"create","queued_at":"2026-07-08T12:04:00Z","payload_status":"available","payload":{"id":space,"name":name,"visibility":"private","created_at":"2026-07-08T12:03:00Z","updated_at":"2026-07-08T12:04:00Z"}})).unwrap()
    }
    #[tokio::test]
    async fn space_creation_replay_each_constituent_failure_rolls_back_entire_topology() {
        for table in [
            "spaces",
            "space_members",
            "groups",
            "group_members",
            "space_group_permissions",
        ] {
            let state = setup(false).await;
            let proof = admin(&state).await;
            let auth = AuthUser {
                claims: proof.claims,
                api_key_scopes: proof.api_key_scopes,
            };
            fail_insert(&state, table).await;
            let before = move_database_snapshot(&state).await;
            let id = uuid::Uuid::new_v4().to_string();
            let name = format!("Replay {id}");
            let slug = crate::storage::sanitize_slug(&name);
            assert!(
                replay_space_create(&state, &auth, &operation(&id, &name))
                    .await
                    .is_err(),
                "{table}"
            );
            assert_eq!(move_database_snapshot(&state).await, before, "{table}");
            assert!(!crate::storage::space_dir(&slug).exists());
        }
    }
    #[tokio::test]
    async fn space_creation_replay_import_and_exact_existing_receipt_retry_contract_preserved() {
        let state = setup(false).await;
        let proof = admin(&state).await;
        let auth = AuthUser {
            claims: proof.claims,
            api_key_scopes: proof.api_key_scopes,
        };
        let id = uuid::Uuid::new_v4().to_string();
        let name = format!("Historical {id}");
        let op = operation(&id, &name);
        let manifest = SyncReplayManifestRequest {
            schema_version: 1,
            provider: "self-hosted".into(),
            operation_count: 1,
            recovery_checkpoint_count: 0,
            recovery_checkpoints: vec![],
            draft_bindings: None,
            operations: vec![op.clone()],
            replay_contract: SyncReplayContract {
                schema_version: 1,
                operation_order: "queued_at_ascending".into(),
                idempotency_key: "operation.id".into(),
                transport: "server-api".into(),
                server_backends: vec!["sqlite".into(), "postgresql".into()],
                required_payload_status: "available".into(),
                conflict_policy: "server_validates_version_and_permissions".into(),
                web_canonical_store: "server-api".into(),
            },
        };
        // This uses the actual dispatcher/ledger path rather than only calling the family SQL.
        let first = execute_replay_operation(&state, &auth, &manifest, &op).await;
        assert_eq!(first.status, "applied");
        let before = move_database_snapshot(&state).await;
        let retry = execute_replay_operation(&state, &auth, &manifest, &op).await;
        assert_eq!(retry.status, "skipped");
        assert_eq!(move_database_snapshot(&state).await, before);
        assert_eq!(
            sqlx::query_as::<_, (String, String, String)>(
                "SELECT id,created_at,updated_at FROM spaces WHERE id=$1"
            )
            .bind(&id)
            .fetch_one(&state.db)
            .await
            .unwrap(),
            (
                id.clone(),
                "2026-07-08T12:03:00+00:00".into(),
                "2026-07-08T12:04:00+00:00".into()
            )
        );
        tokio::fs::remove_dir_all(crate::storage::space_dir(&crate::storage::sanitize_slug(
            &name,
        )))
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn space_creation_replay_last_constituent_credential_revocation_rolls_back() {
        let state = setup(false).await;
        let proof = admin(&state).await;
        let auth = AuthUser {
            claims: proof.claims,
            api_key_scopes: proof.api_key_scopes,
        };
        sqlx::query("CREATE TRIGGER replay_late_constructor AFTER INSERT ON space_group_permissions BEGIN DELETE FROM sessions WHERE user_id='owner-a'; END").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        let id = uuid::Uuid::new_v4().to_string();
        assert!(
            replay_space_create(&state, &auth, &operation(&id, &format!("Replay late {id}")))
                .await
                .is_err()
        );
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn space_creation_replay_signed_admin_claim_cannot_override_current_non_admin_account() {
        let state = setup(false).await;
        let proof = admin(&state).await;
        let auth = AuthUser {
            claims: proof.claims,
            api_key_scopes: proof.api_key_scopes,
        };
        assert_eq!(auth.claims.role, "admin");
        sqlx::query("UPDATE users SET role='user' WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        let id = uuid::Uuid::new_v4().to_string();
        let name = format!("Denied forged role {id}");
        assert!(replay_space_create(&state, &auth, &operation(&id, &name))
            .await
            .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
        assert!(!crate::storage::space_dir(&crate::storage::sanitize_slug(&name)).exists());
    }
}

#[cfg(test)]
mod space_update_current_replay_tests {
    use super::*;
    #[tokio::test]
    async fn replay_space_update_uses_actual_shared_core_and_preserves_import_time() {
        let state=crate::spaces::routes::space_update_current_tests::state(false).await;
        let auth=crate::spaces::routes::space_update_current_tests::auth(&state).await;
        let operation:SyncReplayOperation=serde_json::from_value(serde_json::json!({"id":"space-update-operation","entity_type":"space","entity_id":"space-a","action":"update","queued_at":"2000-01-01T00:00:00Z","payload_status":"ready","payload":{"id":"space-a","name":"Imported name","description":"Imported description","visibility":"internal","updated_at":"2000-01-01T00:00:00Z"}})).unwrap();
        assert_eq!(replay_space_update(&state,&auth,&operation).await.unwrap(),"applied");
        let row:(String,String,String)=sqlx::query_as("SELECT name,visibility,updated_at FROM spaces WHERE id='space-a'").fetch_one(&state.db).await.unwrap();
        assert_eq!(row,("Imported name".into(),"internal".into(),"2000-01-01T00:00:00+00:00".into()));
        let before=crate::spaces::documents::tests::move_database_snapshot(&state).await;
        let invalid:SyncReplayOperation=serde_json::from_value(serde_json::json!({"id":"invalid-space-update","entity_type":"space","entity_id":"space-a","action":"update","queued_at":"2000-01-01T00:00:00Z","payload_status":"ready","payload":{"name":"Must not save","visibility":"invalid"}})).unwrap();
        assert!(replay_space_update(&state,&auth,&invalid).await.is_err());
        assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&state).await,before);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod replay_capacity_tests;

#[cfg(all(test,target_os="linux"))]
#[path="replay_http_kill_tests.rs"]
mod replay_http_kill_tests;
