use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/spaces/{space_id}/tags", get(list_tags).post(create_tag))
        .route(
            "/spaces/{space_id}/tags/{tag_id}",
            axum::routing::put(update_tag).delete(delete_tag),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/tags",
            get(list_doc_tags).post(add_doc_tag),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/tags/{tag_id}",
            axum::routing::delete(remove_doc_tag),
        )
}

#[derive(Deserialize)]
pub struct CreateTagRequest {
    pub name: String,
    pub color: Option<String>,
    pub source_id: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Serialize)]
pub struct TagResponse {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: Option<String>,
    pub doc_count: Option<i64>,
}

async fn list_tags(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    require_tag_space_access(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;
    let tags: Vec<(String, String, String, String, i64)> = sqlx::query_as(
        "SELECT t.id, t.name, t.color, t.created_at, COUNT(dt.document_id) as doc_count \
         FROM tags t LEFT JOIN doc_tags dt ON t.id = dt.tag_id \
         WHERE t.space_id = ?1 GROUP BY t.id, t.name, t.color, t.created_at ORDER BY t.name",
    )
    .bind(&space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<TagResponse> = tags
        .into_iter()
        .map(|(id, name, color, created_at, count)| TagResponse {
            id,
            name,
            color,
            created_at: Some(created_at),
            doc_count: Some(count),
        })
        .collect();

    Ok(Json(result))
}

async fn create_tag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<CreateTagRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    let tag_id = super::routes::import_id_or_uuid(req.source_id.as_deref())?;
    let created_at = super::routes::import_timestamp_or_now(req.created_at.as_deref())?;
    let color = req.color.as_deref().unwrap_or("#6b7280");

    let existing_id: Option<(String,)> = sqlx::query_as("SELECT id FROM tags WHERE id = ?1")
        .bind(&tag_id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;
    if existing_id.is_some() {
        return Err(AppError::Conflict(
            "A tag with this import id already exists".into(),
        ));
    }

    sqlx::query(
        "INSERT INTO tags (id, space_id, name, color, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&tag_id)
    .bind(&space_id)
    .bind(&req.name)
    .bind(color)
    .bind(&created_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok((
        StatusCode::CREATED,
        Json(TagResponse {
            id: tag_id,
            name: req.name,
            color: color.to_string(),
            created_at: Some(created_at),
            doc_count: Some(0),
        }),
    ))
}

async fn update_tag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, tag_id)): Path<(String, String)>,
    Json(req): Json<CreateTagRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    let result = sqlx::query(
        "UPDATE tags SET name = ?1, color = COALESCE(?2, color) WHERE id = ?3 AND space_id = ?4",
    )
    .bind(&req.name)
    .bind(&req.color)
    .bind(&tag_id)
    .bind(&space_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Tag not found in this space".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_tag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, tag_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    let result = sqlx::query("DELETE FROM tags WHERE id = ?1 AND space_id = ?2")
        .bind(&tag_id)
        .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Tag not found in this space".into()));
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn list_doc_tags(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    require_tag_document_access(
        &state,
        &space_id,
        &doc_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;
    let tags: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT t.id, t.name, t.color, t.created_at FROM tags t \
         JOIN doc_tags dt ON t.id = dt.tag_id \
         WHERE dt.document_id = ?1 ORDER BY t.name",
    )
    .bind(&doc_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<TagResponse> = tags
        .into_iter()
        .map(|(id, name, color, created_at)| TagResponse {
            id,
            name,
            color,
            created_at: Some(created_at),
            doc_count: None,
        })
        .collect();

    Ok(Json(result))
}

async fn add_doc_tag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    Json(req): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    require_tag_document_access(
        &state,
        &space_id,
        &doc_id,
        &auth.claims.sub,
        &["owner", "editor"],
    )
    .await?;

    let tag_id = req["tag_id"]
        .as_str()
        .ok_or_else(|| AppError::BadRequest("tag_id required".into()))?;

    let tag_exists: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM tags WHERE id = ?1 AND space_id = ?2 LIMIT 1")
            .bind(tag_id)
            .bind(&space_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    if tag_exists.is_none() {
        return Err(AppError::NotFound("Tag not found in this space".into()));
    }

    sqlx::query(
        "INSERT INTO doc_tags (document_id, tag_id) VALUES (?1, ?2) \
         ON CONFLICT(document_id, tag_id) DO NOTHING",
    )
    .bind(&doc_id)
    .bind(tag_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(StatusCode::CREATED)
}

async fn remove_doc_tag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id, tag_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    require_tag_document_access(
        &state,
        &space_id,
        &doc_id,
        &auth.claims.sub,
        &["owner", "editor"],
    )
    .await?;

    sqlx::query(
        "DELETE FROM doc_tags WHERE document_id = ?1 AND tag_id IN (SELECT id FROM tags WHERE id = ?2 AND space_id = ?3)",
    )
    .bind(&doc_id)
    .bind(&tag_id)
    .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn require_tag_space_access(
    state: &AppState,
    space_id: &str,
    user_id: &str,
    allowed_roles: &[&str],
) -> Result<(), AppError> {
    super::routes::require_space_role(state, space_id, user_id, allowed_roles).await?;
    Ok(())
}

async fn require_tag_document_access(
    state: &AppState,
    space_id: &str,
    document_id: &str,
    user_id: &str,
    allowed_roles: &[&str],
) -> Result<(), AppError> {
    require_tag_space_access(state, space_id, user_id, allowed_roles).await?;
    let document: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM documents WHERE id = ?1 AND space_id = ?2 LIMIT 1")
            .bind(document_id)
            .bind(space_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    if document.is_none() {
        return Err(AppError::NotFound(
            "Document not found in this space".into(),
        ));
    }
    Ok(())
}
