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
        .route(
            "/spaces/{space_id}/templates",
            get(list_templates).post(create_template),
        )
        .route(
            "/spaces/{space_id}/templates/{template_id}",
            get(get_template)
                .put(update_template)
                .delete(delete_template),
        )
}

#[derive(Deserialize)]
pub struct TemplateRequest {
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub properties: Option<serde_json::Value>,
    pub tags: Option<Vec<String>>,
    pub source_id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Serialize)]
pub struct TemplateResponse {
    pub id: String,
    pub space_id: String,
    pub name: String,
    pub description: Option<String>,
    pub content: String,
    pub properties: Option<serde_json::Value>,
    pub tags: Vec<String>,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

fn validate_template(req: &TemplateRequest) -> Result<(), AppError> {
    if req.name.trim().is_empty() {
        return Err(AppError::BadRequest("Template name is required".into()));
    }
    if req.name.len() > 100 {
        return Err(AppError::BadRequest(
            "Template name must be at most 100 characters".into(),
        ));
    }
    if let Some(description) = &req.description {
        if description.len() > 1000 {
            return Err(AppError::BadRequest(
                "Template description must be at most 1000 characters".into(),
            ));
        }
    }
    if req.content.len() > 5_242_880 {
        return Err(AppError::BadRequest(
            "Template content must be at most 5 MB".into(),
        ));
    }
    Ok(())
}

fn parse_json_value(raw: Option<String>) -> Option<serde_json::Value> {
    raw.and_then(|value| serde_json::from_str(&value).ok())
}

fn parse_tags(raw: Option<String>) -> Vec<String> {
    raw.and_then(|value| serde_json::from_str(&value).ok())
        .unwrap_or_default()
}

fn row_to_template(
    row: (
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
    ),
) -> TemplateResponse {
    let (
        id,
        space_id,
        name,
        description,
        content,
        properties,
        tags,
        created_by,
        created_at,
        updated_at,
    ) = row;
    TemplateResponse {
        id,
        space_id,
        name,
        description,
        content,
        properties: parse_json_value(properties),
        tags: parse_tags(tags),
        created_by,
        created_at,
        updated_at,
    }
}

async fn list_templates(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;

    let templates = sqlx::query_as::<_, (String, String, String, Option<String>, String, Option<String>, Option<String>, String, String, String)>(
        "SELECT id, space_id, name, description, content, properties, tags, created_by, created_at, updated_at \
         FROM templates WHERE space_id = ?1 ORDER BY name",
    )
    .bind(&space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?
    .into_iter()
    .map(row_to_template)
    .collect::<Vec<_>>();

    Ok(Json(templates))
}

async fn get_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;

    let template = sqlx::query_as::<_, (String, String, String, Option<String>, String, Option<String>, Option<String>, String, String, String)>(
        "SELECT id, space_id, name, description, content, properties, tags, created_by, created_at, updated_at \
         FROM templates WHERE id = ?1 AND space_id = ?2",
    )
    .bind(&template_id)
    .bind(&space_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?
    .map(row_to_template)
    .ok_or_else(|| AppError::NotFound("Template not found".into()))?;

    Ok(Json(template))
}

async fn create_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<TemplateRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;
    validate_template(&req)?;

    let template_id = super::routes::import_id_or_uuid(req.source_id.as_deref())?;
    let created_at = super::routes::import_timestamp_or_now(req.created_at.as_deref())?;
    let updated_at =
        super::routes::import_timestamp_or_default(req.updated_at.as_deref(), &created_at)?;
    let properties = req
        .properties
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(AppError::internal)?;
    let tags = serde_json::to_string(&req.tags.unwrap_or_default()).map_err(AppError::internal)?;

    let existing_id: Option<(String,)> = sqlx::query_as("SELECT id FROM templates WHERE id = ?1")
        .bind(&template_id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;
    if existing_id.is_some() {
        return Err(AppError::Conflict(
            "A template with this import id already exists".into(),
        ));
    }

    sqlx::query(
        "INSERT INTO templates (id, space_id, name, description, content, properties, tags, created_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(&template_id)
    .bind(&space_id)
    .bind(req.name.trim())
    .bind(&req.description)
    .bind(&req.content)
    .bind(properties)
    .bind(tags)
    .bind(&auth.claims.sub)
    .bind(&created_at)
    .bind(&updated_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "template.create",
        Some("template"),
        Some(&template_id),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "id": template_id })),
    ))
}

async fn update_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id)): Path<(String, String)>,
    Json(req): Json<TemplateRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;
    validate_template(&req)?;

    let properties = req
        .properties
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(AppError::internal)?;
    let tags = serde_json::to_string(&req.tags.unwrap_or_default()).map_err(AppError::internal)?;
    let updated_at = chrono::Utc::now().to_rfc3339();

    let result = sqlx::query(
        "UPDATE templates SET name = ?1, description = ?2, content = ?3, properties = ?4, tags = ?5, \
         updated_at = ?6 WHERE id = ?7 AND space_id = ?8",
    )
    .bind(req.name.trim())
    .bind(&req.description)
    .bind(&req.content)
    .bind(properties)
    .bind(tags)
    .bind(&updated_at)
    .bind(&template_id)
    .bind(&space_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Template not found".into()));
    }

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "template.update",
        Some("template"),
        Some(&template_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    let result = sqlx::query("DELETE FROM templates WHERE id = ?1 AND space_id = ?2")
        .bind(&template_id)
        .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Template not found".into()));
    }

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "template.delete",
        Some("template"),
        Some(&template_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}
