use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::io::AsyncWriteExt;

use crate::auth::middleware::AdminUser;
use crate::auth::routes::AppError;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/audit-log", get(list_audit_log))
        .route("/admin/audit-log/export", get(export_csv))
        .route("/admin/storage-integrity", get(storage_integrity))
        .route("/admin/storage-health", get(storage_health))
        .route(
            "/admin/plugin-runtime-audit",
            post(record_plugin_runtime_audit),
        )
}

/// Preserve the administrator credential ceiling while resolving original
/// user/session or key/service evidence again after observations wait.
async fn current_storage_admin(state: &AppState, admin: &AdminUser) -> Result<(), AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    crate::auth::middleware::require_current_admin(&mut conn, admin).await?;
    Ok(())
}

async fn observe_storage<T>(
    state: &AppState,
    auth: &AdminUser,
    observation: impl std::future::Future<Output = Result<T, AppError>>,
) -> Result<T, AppError> {
    current_storage_admin(state, auth).await?;
    let report = observation.await?;
    current_storage_admin(state, auth).await?;
    Ok(report)
}

async fn storage_health(
    State(state): State<AppState>,
    auth: AdminUser,
    #[cfg(target_os = "linux")] historical: Option<axum::Extension<crate::storage::restoration::linux::HistoricalHealthRequest>>,
) -> Result<axum::response::Response, AppError> {
    #[cfg(target_os = "linux")]
    if let Some(axum::Extension(historical)) = historical {
        return historical.observe(&state, &auth).await;
    }
    let report = observe_storage(&state, &auth, async {
        Ok(crate::storage::health::observe(&state).await)
    }).await?;
    Ok(Json(report).into_response())
}

async fn storage_integrity(
    State(state): State<AppState>,
    auth: AdminUser,
) -> Result<impl IntoResponse, AppError> {
    let report = observe_storage(&state, &auth, async {
        crate::storage::audit_integrity(&state.db).await.map_err(AppError::internal)
    }).await?;
    Ok(Json(report))
}

#[derive(Deserialize)]
pub struct AuditLogQuery {
    pub action: Option<String>,
    pub user_id: Option<String>,
    pub target_type: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize)]
pub struct AuditLogEntry {
    pub id: String,
    pub user_id: Option<String>,
    pub user_email: Option<String>,
    pub ip_address: Option<String>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub created_at: String,
    pub actor_kind: Option<String>,
    pub actor_id: Option<String>,
}

#[derive(Deserialize)]
pub struct PluginRuntimeAuditRequest {
    pub plugin_id: String,
    pub plugin_name: Option<String>,
    pub action: String,
    pub runtime_state: String,
    pub reason: Option<String>,
    pub space_id: Option<String>,
    pub space_name: Option<String>,
}

async fn record_plugin_runtime_audit(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(req): Json<PluginRuntimeAuditRequest>,
) -> Result<impl IntoResponse, AppError> {
    let (plugin_id, action) = validate_plugin_runtime_audit_request(&req)?;

    let _ = (
        req.plugin_name,
        req.runtime_state,
        req.reason,
        req.space_id,
        req.space_name,
    );

    log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        action,
        Some("plugin_runtime"),
        Some(&plugin_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

fn validate_plugin_runtime_audit_request(
    req: &PluginRuntimeAuditRequest,
) -> Result<(String, &'static str), AppError> {
    let plugin_id = req.plugin_id.trim();
    if plugin_id.is_empty() {
        return Err(AppError::BadRequest("plugin_id is required".into()));
    }

    let action = match req.action.as_str() {
        "runtime_enable_blocked" => "plugin.runtime_enable_blocked",
        "runtime_space_enable_blocked" => "plugin.runtime_space_enable_blocked",
        _ => {
            return Err(AppError::BadRequest(
                "Unsupported plugin runtime audit action".into(),
            ));
        }
    };

    Ok((plugin_id.to_string(), action))
}

async fn list_audit_log(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<AuditLogQuery>,
) -> Result<impl IntoResponse, AppError> {
    let (limit, offset) = audit_log_pagination(query.limit, query.offset);

    let entries: Vec<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT a.id, a.user_id, u.email, a.ip_address, a.action, a.target_type, \
             a.target_id, a.created_at, a.actor_kind, a.actor_id \
             FROM audit_log a LEFT JOIN users u ON a.user_id = u.id \
             WHERE ($1 IS NULL OR a.action LIKE $2) \
               AND ($3 IS NULL OR a.user_id = $3) \
               AND ($4 IS NULL OR a.target_type = $4) \
             ORDER BY a.created_at DESC LIMIT $5 OFFSET $6",
    )
    .bind(query.action.as_deref())
    .bind(query.action.as_deref().map(|value| format!("%{value}%")))
    .bind(query.user_id.as_deref())
    .bind(query.target_type.as_deref())
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<AuditLogEntry> = entries
        .into_iter()
        .map(
            |(id, uid, email, ip, action, ttype, tid, created, actor_kind, actor_id)| {
                AuditLogEntry {
                    id,
                    user_id: uid,
                    user_email: email,
                    ip_address: ip,
                    action,
                    target_type: ttype,
                    target_id: tid,
                    created_at: created,
                    actor_kind,
                    actor_id,
                }
            },
        )
        .collect();

    Ok(Json(result))
}

async fn export_csv(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, AppError> {
    let entries: Vec<(
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(
        "SELECT a.id, u.email, a.ip_address, a.action, a.target_type, a.target_id, a.created_at, a.actor_kind, a.actor_id \
             FROM audit_log a LEFT JOIN users u ON a.user_id = u.id \
             ORDER BY a.created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let mut csv = String::from(
        "id,user_email,ip_address,action,target_type,target_id,created_at,actor_kind,actor_id\n",
    );
    for (id, email, ip, action, ttype, tid, created, actor_kind, actor_id) in entries {
        let fields = [
            id,
            email.unwrap_or_default(),
            ip.unwrap_or_default(),
            action,
            ttype.unwrap_or_default(),
            tid.unwrap_or_default(),
            created,
            actor_kind.unwrap_or_default(),
            actor_id.unwrap_or_default(),
        ];
        csv.push_str(
            &fields
                .iter()
                .map(|field| csv_field(field))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push('\n');
    }

    Ok((
        [
            ("Content-Type", "text/csv"),
            (
                "Content-Disposition",
                "attachment; filename=\"audit-log.csv\"",
            ),
        ],
        csv,
    ))
}

/// Preserve text cells exactly; simple legacy values keep their existing bytes.
fn csv_field(value: &str) -> String {
    if value.contains([',', '"', '\r', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// Captured successful management audit. Identity comes only from the current
/// administrator proof; service keys never occupy the human user foreign key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct CapturedActorAudit {
    id: String,
    user_id: Option<String>,
    ip_address: Option<String>,
    action: String,
    target_type: Option<String>,
    target_id: Option<String>,
    created_at: String,
    actor_kind: String,
    actor_id: String,
}
fn authority_actor(
    authority: &crate::auth::middleware::CurrentAdminAuthority,
) -> (&'static str, &str, Option<&str>) {
    use crate::auth::middleware::CurrentAdminAuthority;
    match authority {
        CurrentAdminAuthority::HumanInstanceAdmin { user_id } => ("user", user_id, Some(user_id)),
        CurrentAdminAuthority::ServiceAdminKey(key) => ("service_key", &key.key_id, None),
    }
}
pub(crate) async fn insert_actor_action(
    conn: &mut sqlx::AnyConnection,
    authority: &crate::auth::middleware::CurrentAdminAuthority,
    action: &str,
    target_type: &str,
    target_id: &str,
) -> Result<CapturedActorAudit, AppError> {
    let (kind, actor, user) = authority_actor(authority);
    insert_captured_actor_action(conn, kind, actor, user, action, target_type, target_id).await
}
pub(crate) async fn insert_scim_actor_action(
    conn: &mut sqlx::AnyConnection,
    credential: &crate::scim::CurrentScimCredential,
    action: &str,
    target_type: &str,
    target_id: &str,
) -> Result<CapturedActorAudit, AppError> {
    if !credential.is_current() {
        return Err(AppError::Unauthorized("SCIM credential unavailable".into()));
    }
    insert_captured_actor_action(
        conn,
        "scim_token",
        "SCIM_BEARER_TOKEN",
        None,
        action,
        target_type,
        target_id,
    )
    .await
}
/// Fixed registration action, bound to freshly verified canonical row evidence.
/// This route-origin data witness carries no administrator or HTTP capability.
pub(crate) async fn insert_registration_space_action(
    conn: &mut sqlx::AnyConnection,
    origin: &crate::auth::routes::RegistrationAuditOrigin,
) -> Result<CapturedActorAudit, AppError> {
    insert_captured_actor_action(
        conn,
        "user",
        origin.user_id(),
        Some(origin.user_id()),
        "space.personal_create",
        "space",
        origin.space_id(),
    )
    .await
}
pub(crate) async fn verify_registration_space_action(
    conn: &mut sqlx::AnyConnection,
    event: &CapturedActorAudit,
    origin: &crate::auth::routes::RegistrationAuditOrigin,
) -> Result<(), AppError> {
    if event.action != "space.personal_create"
        || event.target_type.as_deref() != Some("space")
        || event.target_id.as_deref() != Some(origin.space_id())
    {
        return Err(AppError::Conflict("Registration audit changed".into()));
    }
    verify_captured_actor_action(
        conn,
        event,
        "user",
        origin.user_id(),
        Some(origin.user_id()),
    )
    .await
}
/// Fixed birth audit for the actual newly created user, not administrator authority.
pub(crate) async fn insert_admin_birth_space_action(
    conn: &mut sqlx::AnyConnection,
    origin: &crate::auth::routes::AdminAccountBirthAuditOrigin,
) -> Result<CapturedActorAudit, AppError> {
    insert_captured_actor_action(
        conn,
        "user",
        origin.user_id(),
        Some(origin.user_id()),
        "space.personal_create",
        "space",
        origin.space_id(),
    )
    .await
}
pub(crate) async fn verify_admin_birth_space_action(
    conn: &mut sqlx::AnyConnection,
    event: &CapturedActorAudit,
    origin: &crate::auth::routes::AdminAccountBirthAuditOrigin,
) -> Result<(), AppError> {
    if event.action != "space.personal_create"
        || event.target_type.as_deref() != Some("space")
        || event.target_id.as_deref() != Some(origin.space_id())
    {
        return Err(AppError::Conflict("Account birth audit changed".into()));
    }
    verify_captured_actor_action(
        conn,
        event,
        "user",
        origin.user_id(),
        Some(origin.user_id()),
    )
    .await
}
/// Fixed CSV birth event from verified new-account rows, never an HTTP principal.
pub(crate) async fn insert_csv_birth_space_action(
    conn: &mut sqlx::AnyConnection,
    origin: &crate::provisioning::CsvAccountBirthAuditOrigin,
) -> Result<CapturedActorAudit, AppError> {
    insert_captured_actor_action(
        conn,
        "user",
        origin.user_id(),
        Some(origin.user_id()),
        "space.personal_create",
        "space",
        origin.space_id(),
    )
    .await
}
pub(crate) async fn verify_csv_birth_space_action(
    conn: &mut sqlx::AnyConnection,
    event: &CapturedActorAudit,
    origin: &crate::provisioning::CsvAccountBirthAuditOrigin,
) -> Result<(), AppError> {
    if event.action != "space.personal_create"
        || event.target_type.as_deref() != Some("space")
        || event.target_id.as_deref() != Some(origin.space_id())
    {
        return Err(AppError::Conflict("CSV birth audit changed".into()));
    }
    verify_captured_actor_action(
        conn,
        event,
        "user",
        origin.user_id(),
        Some(origin.user_id()),
    )
    .await
}
/// Existing personal-space attribution from a real provider callback's verified birth.
pub(crate) async fn insert_oauth_birth_space_action(
    conn: &mut sqlx::AnyConnection,
    origin: &crate::auth::routes::OAuthAccountBirthAuditOrigin,
) -> Result<CapturedActorAudit, AppError> {
    insert_captured_actor_action(
        conn,
        "user",
        origin.user_id(),
        Some(origin.user_id()),
        "space.personal_create",
        "space",
        origin.space_id(),
    )
    .await
}
pub(crate) async fn verify_oauth_birth_space_action(
    conn: &mut sqlx::AnyConnection,
    event: &CapturedActorAudit,
    origin: &crate::auth::routes::OAuthAccountBirthAuditOrigin,
) -> Result<(), AppError> {
    if event.action != "space.personal_create"
        || event.target_type.as_deref() != Some("space")
        || event.target_id.as_deref() != Some(origin.space_id())
    {
        return Err(AppError::Conflict("Provider birth audit changed".into()));
    }
    verify_captured_actor_action(
        conn,
        event,
        "user",
        origin.user_id(),
        Some(origin.user_id()),
    )
    .await
}

async fn insert_captured_actor_action(
    conn: &mut sqlx::AnyConnection,
    kind: &str,
    actor: &str,
    user: Option<&str>,
    action: &str,
    target_type: &str,
    target_id: &str,
) -> Result<CapturedActorAudit, AppError> {
    let mut event = CapturedActorAudit {
        id: uuid::Uuid::new_v4().to_string(),
        user_id: user.map(str::to_owned),
        ip_address: None,
        action: action.into(),
        target_type: Some(target_type.into()),
        target_id: Some(target_id.into()),
        created_at: String::new(),
        actor_kind: kind.into(),
        actor_id: actor.into(),
    };
    let created:Option<String>=sqlx::query_scalar("INSERT INTO audit_log(id,user_id,ip_address,action,target_type,target_id,actor_kind,actor_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING created_at")
        .bind(&event.id).bind(&event.user_id).bind(&event.ip_address).bind(&event.action)
        .bind(&event.target_type).bind(&event.target_id).bind(&event.actor_kind).bind(&event.actor_id)
        .fetch_optional(conn).await.map_err(AppError::internal)?;
    event.created_at = created.ok_or_else(|| AppError::Conflict("Audit event changed".into()))?;
    Ok(event)
}
pub(crate) async fn verify_actor_action(
    conn: &mut sqlx::AnyConnection,
    event: &CapturedActorAudit,
    authority: &crate::auth::middleware::CurrentAdminAuthority,
) -> Result<(), AppError> {
    let (kind, actor, user) = authority_actor(authority);
    verify_captured_actor_action(conn, event, kind, actor, user).await
}
pub(crate) async fn verify_scim_actor_action(
    conn: &mut sqlx::AnyConnection,
    event: &CapturedActorAudit,
    credential: &crate::scim::CurrentScimCredential,
) -> Result<(), AppError> {
    if !credential.is_current() {
        return Err(AppError::Unauthorized("SCIM credential unavailable".into()));
    }
    verify_captured_actor_action(conn, event, "scim_token", "SCIM_BEARER_TOKEN", None).await
}
async fn verify_captured_actor_action(
    conn: &mut sqlx::AnyConnection,
    event: &CapturedActorAudit,
    kind: &str,
    actor: &str,
    user: Option<&str>,
) -> Result<(), AppError> {
    if event.actor_kind != kind || event.actor_id != actor || event.user_id.as_deref() != user {
        return Err(AppError::Conflict("Audit actor changed".into()));
    }
    let row:Option<(String,Option<String>,Option<String>,String,Option<String>,Option<String>,String,Option<String>,Option<String>,Option<String>,Option<String>)>=sqlx::query_as("SELECT id,user_id,ip_address,action,target_type,target_id,created_at,actor_kind,actor_id,before_value,after_value FROM audit_log WHERE id=$1")
        .bind(&event.id).fetch_optional(conn).await.map_err(AppError::internal)?;
    if row
        != Some((
            event.id.clone(),
            event.user_id.clone(),
            event.ip_address.clone(),
            event.action.clone(),
            event.target_type.clone(),
            event.target_id.clone(),
            event.created_at.clone(),
            Some(event.actor_kind.clone()),
            Some(event.actor_id.clone()),
            None,
            None,
        ))
    {
        return Err(AppError::Conflict("Audit event changed".into()));
    }
    Ok(())
}
#[cfg(test)]
pub(crate) async fn test_append_actor_action(
    directory: &std::path::Path,
    event: &CapturedActorAudit,
) {
    append_lifecycle_value_in(directory, &serde_json::to_value(event).unwrap()).await;
}
/// Only after a known successful SQL commit; JSONL delivery is advisory.
pub(crate) async fn append_actor_action(event: &CapturedActorAudit) {
    if let Ok(value) = serde_json::to_value(event) {
        append_lifecycle_value(&value).await;
    }
}

/// Helper to record an audit log entry (called from other modules)
pub async fn log_action(
    db: &sqlx::AnyPool,
    user_id: Option<&str>,
    ip: Option<&str>,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
) {
    let id = uuid::Uuid::new_v4().to_string();
    let _ = sqlx::query(
        "INSERT INTO audit_log (id, user_id, ip_address, action, target_type, target_id) \
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(ip)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .execute(db)
    .await;
    append_lifecycle_event(&id, user_id, ip, action, target_type, target_id).await;
}

async fn append_lifecycle_event(
    id: &str,
    user_id: Option<&str>,
    ip: Option<&str>,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
) {
    let now = chrono::Utc::now();
    let event = json!({
        "id": id,
        "user_id": user_id,
        "ip_address": ip,
        "action": action,
        "target_type": target_type,
        "target_id": target_id,
        "created_at": now.to_rfc3339(),
    });
    append_lifecycle_value(&event).await;
}

async fn append_lifecycle_value(event: &serde_json::Value) {
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string());
    let directory = PathBuf::from(data_dir).join("logging").join("lifecycle");
    append_lifecycle_value_in(&directory, event).await;
}

async fn append_lifecycle_value_in(directory: &std::path::Path, event: &serde_json::Value) {
    if let Err(error) = tokio::fs::create_dir_all(&directory).await {
        tracing::warn!(%error, "Unable to create lifecycle log directory");
        return;
    }
    let now = chrono::Utc::now();
    let path = directory.join(format!("lifecycle-{}.jsonl", now.format("%Y-%m-%d")));
    match tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .await
    {
        Ok(mut file) => {
            if let Ok(mut line) = serde_json::to_vec(&event) {
                line.push(b'\n');
                if let Err(error) = file.write_all(&line).await {
                    tracing::warn!(%error, "Unable to append lifecycle event");
                }
            }
        }
        Err(error) => tracing::warn!(%error, "Unable to open lifecycle event log"),
    }
    prune_lifecycle_logs(&directory).await;
}

async fn prune_lifecycle_logs(directory: &std::path::Path) {
    let Ok(mut entries) = tokio::fs::read_dir(directory).await else {
        return;
    };
    let cutoff = SystemTime::now().checked_sub(Duration::from_secs(30 * 86_400));
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(metadata) = entry.metadata().await else {
            continue;
        };
        if cutoff.is_some_and(|limit| {
            metadata
                .modified()
                .ok()
                .is_some_and(|modified| modified < limit)
        }) {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
}

fn audit_log_pagination(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    (
        limit.unwrap_or(50).clamp(0, 200),
        offset.unwrap_or(0).max(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_csv_fields_preserve_quotes_commas_and_line_breaks() {
        assert_eq!(csv_field("plain actor"), "plain actor");
        assert_eq!(csv_field("key,part"), "\"key,part\"");
        assert_eq!(csv_field("key\"part"), "\"key\"\"part\"");
        assert_eq!(csv_field("key\r\npart"), "\"key\r\npart\"");
        assert_eq!(csv_field(""), "");
    }

    #[test]
    fn audit_log_pagination_matches_openapi_bounds() {
        assert_eq!(audit_log_pagination(None, None), (50, 0));
        assert_eq!(audit_log_pagination(Some(-5), Some(-10)), (0, 0));
        assert_eq!(audit_log_pagination(Some(25), Some(15)), (25, 15));
        assert_eq!(audit_log_pagination(Some(250), Some(2)), (200, 2));
    }

    #[test]
    fn plugin_runtime_audit_validation_accepts_only_blocked_runtime_actions() {
        let global_block = PluginRuntimeAuditRequest {
            plugin_id: "  com.example.plugin  ".to_string(),
            plugin_name: None,
            action: "runtime_enable_blocked".to_string(),
            runtime_state: "blocked".to_string(),
            reason: None,
            space_id: None,
            space_name: None,
        };
        assert_eq!(
            validate_plugin_runtime_audit_request(&global_block)
                .expect("global runtime block should be valid"),
            (
                "com.example.plugin".to_string(),
                "plugin.runtime_enable_blocked"
            )
        );

        let space_block = PluginRuntimeAuditRequest {
            action: "runtime_space_enable_blocked".to_string(),
            space_id: Some("space-1".to_string()),
            space_name: Some("Engineering".to_string()),
            ..global_block
        };
        assert_eq!(
            validate_plugin_runtime_audit_request(&space_block)
                .expect("space runtime block should be valid"),
            (
                "com.example.plugin".to_string(),
                "plugin.runtime_space_enable_blocked"
            )
        );

        let missing_plugin = PluginRuntimeAuditRequest {
            plugin_id: "   ".to_string(),
            ..space_block
        };
        match validate_plugin_runtime_audit_request(&missing_plugin) {
            Err(AppError::BadRequest(message)) => {
                assert_eq!(message, "plugin_id is required");
            }
            other => panic!("expected missing plugin rejection, got {other:?}"),
        }

        let unsupported_action = PluginRuntimeAuditRequest {
            plugin_id: "com.example.plugin".to_string(),
            action: "runtime_enable_allowed".to_string(),
            ..missing_plugin
        };
        match validate_plugin_runtime_audit_request(&unsupported_action) {
            Err(AppError::BadRequest(message)) => {
                assert_eq!(message, "Unsupported plugin runtime audit action");
            }
            other => panic!("expected unsupported action rejection, got {other:?}"),
        }
    }
}

/// Advisory delivery of an exact move-local event already proven in its SQL transaction.
/// This performs no SQL and grants no operation or administrator authority.
pub(crate) async fn append_committed_move_action(event: &serde_json::Value) {
    append_lifecycle_value(event).await;
}

#[cfg(test)]
mod storage_status_authority_tests;
