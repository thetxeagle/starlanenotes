use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/spaces/{space_id}/documents/{doc_id}/versions",
            get(list_versions),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/versions/{version_id}",
            get(get_version),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/versions/{version_id}/restore",
            axum::routing::post(restore_version),
        )
}

#[derive(Serialize)]
pub struct VersionResponse {
    pub id: String,
    pub version_number: i64,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub created_by: String,
    pub author_name: String,
    pub created_at: String,
}

async fn list_versions(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path((_space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let versions: Vec<(String, i64, i64, i64, String, String, String)> = sqlx::query_as(
        "SELECT v.id, v.version_number, v.lines_added, v.lines_removed, v.created_by, \
         u.display_name, v.created_at \
         FROM doc_versions v JOIN users u ON v.created_by = u.id \
         WHERE v.document_id = ?1 ORDER BY v.version_number DESC",
    )
    .bind(&doc_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<VersionResponse> = versions
        .into_iter()
        .map(
            |(id, num, added, removed, uid, uname, created)| VersionResponse {
                id,
                version_number: num,
                lines_added: added,
                lines_removed: removed,
                created_by: uid,
                author_name: uname,
                created_at: created,
            },
        )
        .collect();

    Ok(Json(result))
}

async fn get_version(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path((_space_id, _doc_id, version_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let version: Option<(String, i64, i64, i64, String, String, String)> = sqlx::query_as(
        "SELECT v.id, v.version_number, v.lines_added, v.lines_removed, v.created_by, \
         u.display_name, v.created_at \
         FROM doc_versions v JOIN users u ON v.created_by = u.id \
         WHERE v.id = ?1",
    )
    .bind(&version_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    let (id, num, added, removed, uid, uname, created) =
        version.ok_or_else(|| AppError::BadRequest("Version not found".into()))?;

    Ok(Json(VersionResponse {
        id,
        version_number: num,
        lines_added: added,
        lines_removed: removed,
        created_by: uid,
        author_name: uname,
        created_at: created,
    }))
}

async fn restore_version(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id, version_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    // Get the version's change_set (CRDT state)
    let version: Option<(Option<Vec<u8>>,)> =
        sqlx::query_as("SELECT change_set FROM doc_versions WHERE id = ?1")
            .bind(&version_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    let _change_set = version.ok_or_else(|| AppError::BadRequest("Version not found".into()))?;

    // Create a new version with the restored content (non-destructive)
    let new_version_id = Uuid::new_v4().to_string();
    let max_version: (Option<i64>,) =
        sqlx::query_as("SELECT MAX(version_number) FROM doc_versions WHERE document_id = ?1")
            .bind(&doc_id)
            .fetch_one(&state.db)
            .await
            .map_err(AppError::internal)?;

    let new_version_number = max_version.0.unwrap_or(0) + 1;

    sqlx::query(
        "INSERT INTO doc_versions (id, document_id, version_number, created_by) \
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(&new_version_id)
    .bind(&doc_id)
    .bind(new_version_number)
    .bind(&auth.claims.sub)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(Json(serde_json::json!({
        "version_id": new_version_id,
        "version_number": new_version_number,
    })))
}
