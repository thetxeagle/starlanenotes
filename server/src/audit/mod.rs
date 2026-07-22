use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::auth::middleware::AdminUser;
use crate::auth::routes::AppError;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/audit-log", get(list_audit_log))
        .route("/admin/audit-log/export", get(export_csv))
        .route(
            "/admin/plugin-runtime-audit",
            post(record_plugin_runtime_audit),
        )
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
    )> = sqlx::query_as(
        "SELECT a.id, a.user_id, u.email, a.ip_address, a.action, a.target_type, \
             a.target_id, a.created_at \
             FROM audit_log a LEFT JOIN users u ON a.user_id = u.id \
             ORDER BY a.created_at DESC LIMIT ?1 OFFSET ?2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<AuditLogEntry> = entries
        .into_iter()
        .map(
            |(id, uid, email, ip, action, ttype, tid, created)| AuditLogEntry {
                id,
                user_id: uid,
                user_email: email,
                ip_address: ip,
                action,
                target_type: ttype,
                target_id: tid,
                created_at: created,
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
    )> = sqlx::query_as(
        "SELECT a.id, u.email, a.ip_address, a.action, a.target_type, a.target_id, a.created_at \
             FROM audit_log a LEFT JOIN users u ON a.user_id = u.id \
             ORDER BY a.created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let mut csv =
        String::from("id,user_email,ip_address,action,target_type,target_id,created_at\n");
    for (id, email, ip, action, ttype, tid, created) in entries {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            id,
            email.unwrap_or_default(),
            ip.unwrap_or_default(),
            action,
            ttype.unwrap_or_default(),
            tid.unwrap_or_default(),
            created,
        ));
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
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&id)
    .bind(user_id)
    .bind(ip)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .execute(db)
    .await;
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
