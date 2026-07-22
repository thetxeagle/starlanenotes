use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{AnyPool, Row};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::search::index::DocEntry;
use crate::storage;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/spaces/{space_id}/documents",
            get(list_documents).post(create_document),
        )
        .route(
            "/spaces/{space_id}/documents/starred",
            get(list_starred_documents),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/star",
            axum::routing::post(star_document).delete(unstar_document),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}",
            get(get_document)
                .put(update_document)
                .delete(delete_document),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/move",
            post(move_document),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/content",
            get(get_content).put(update_content),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/lock",
            axum::routing::post(lock_document),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/unlock",
            axum::routing::post(unlock_document),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/publish",
            axum::routing::post(publish_document),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/unpublish",
            axum::routing::post(unpublish_document),
        )
        // Public pages (no auth)
        .route("/p/{slug}", get(get_public_page))
}

// ============================================================
// Types
// ============================================================

#[derive(Deserialize)]
pub struct CreateDocumentRequest {
    pub title: String,
    pub folder_id: Option<String>,
    pub parent_id: Option<String>,
    pub icon: Option<String>,
    pub content: Option<String>,
    pub template_id: Option<String>,
    pub source_id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

struct TemplateMetadata {
    tags: Vec<(String, String)>,
    property_values: Vec<(String, String)>,
}

#[derive(Deserialize)]
pub struct UpdateDocumentRequest {
    pub title: Option<String>,
    pub folder_id: Option<String>,
    pub parent_id: Option<String>,
    pub icon: Option<String>,
    pub cover_image: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateContentRequest {
    pub content: String,
}

#[derive(Deserialize)]
pub struct MoveDocumentRequest {
    pub target_space_id: String,
    pub folder_id: Option<String>,
    pub parent_id: Option<String>,
}

#[derive(Serialize)]
pub struct DocumentResponse {
    pub id: String,
    pub space_id: String,
    pub folder_id: Option<String>,
    pub parent_id: Option<String>,
    pub title: String,
    pub icon: Option<String>,
    pub cover_image: Option<String>,
    pub locked: bool,
    pub locked_by: Option<String>,
    pub public: bool,
    pub public_slug: Option<String>,
    pub created_by: String,
    pub last_updated_by: String,
    pub created_at: String,
    pub updated_at: String,
    pub workflow_template_id: Option<String>,
    pub workflow_template_name: Option<String>,
    pub workflow_stage_id: Option<String>,
    pub workflow_stage_name: Option<String>,
    pub workflow_status: Option<String>,
}

#[derive(Serialize)]
pub struct DocumentContentResponse {
    pub id: String,
    pub title: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct StarredDocumentResponse {
    pub id: String,
    pub space_id: String,
    pub space_name: String,
    pub title: String,
    pub icon: Option<String>,
    pub updated_at: String,
    pub starred_at: String,
}

#[derive(Deserialize)]
pub struct ListDocumentsQuery {
    pub folder_id: Option<String>,
    pub include_nested: Option<bool>,
    pub workflow_status: Option<String>,
    pub workflow_stage_id: Option<String>,
    pub workflow_template_id: Option<String>,
}

#[derive(Serialize)]
pub struct PublicPageResponse {
    pub title: String,
    pub icon: Option<String>,
    pub cover_image: Option<String>,
    pub content: String,
    pub space_name: String,
    pub space_accent_color: Option<String>,
    pub author: String,
    pub updated_at: String,
}

// ============================================================
// Handlers
// ============================================================

/// GET /api/v1/spaces/{space_id}/documents/starred
async fn list_starred_documents(
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

    let rows = sqlx::query(
        "SELECT d.id, d.space_id, s.name AS space_name, d.title, d.icon, d.updated_at, sd.starred_at \
         FROM starred_documents sd \
         JOIN documents d ON d.id = sd.document_id \
         JOIN spaces s ON s.id = d.space_id \
         WHERE sd.user_id = ?1 AND d.space_id = ?2 \
         ORDER BY sd.starred_at DESC",
    )
    .bind(&auth.claims.sub)
    .bind(&space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result = rows
        .into_iter()
        .map(|row| {
            Ok(StarredDocumentResponse {
                id: row.try_get("id")?,
                space_id: row.try_get("space_id")?,
                space_name: row.try_get("space_name")?,
                title: row.try_get("title")?,
                icon: row.try_get("icon")?,
                updated_at: row.try_get("updated_at")?,
                starred_at: row.try_get("starred_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(AppError::internal)?;

    Ok(Json(result))
}

/// POST /api/v1/spaces/{space_id}/documents/{doc_id}/star
async fn star_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;

    let exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM documents WHERE id = ?1 AND space_id = ?2")
            .bind(&doc_id)
            .bind(&space_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    if exists.is_none() {
        return Err(AppError::NotFound("Document not found".into()));
    }

    let starred_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO starred_documents (user_id, document_id, starred_at) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT(user_id, document_id) DO UPDATE SET starred_at = excluded.starred_at",
    )
    .bind(&auth.claims.sub)
    .bind(&doc_id)
    .bind(&starred_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/v1/spaces/{space_id}/documents/{doc_id}/star
async fn unstar_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;

    sqlx::query(
        "DELETE FROM starred_documents \
         WHERE user_id = ?1 \
           AND document_id IN (SELECT id FROM documents WHERE id = ?2 AND space_id = ?3)",
    )
    .bind(&auth.claims.sub)
    .bind(&doc_id)
    .bind(&space_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/spaces/{space_id}/documents
async fn list_documents(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Query(query): Query<ListDocumentsQuery>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;

    if let Some(status) = query.workflow_status.as_deref() {
        if !matches!(status, "active" | "completed" | "cancelled") {
            return Err(AppError::BadRequest(
                "Invalid workflow status filter".into(),
            ));
        }
    }

    let rows = if query.include_nested.unwrap_or(false) {
        sqlx::query(
            "SELECT id, space_id, folder_id, parent_id, title, icon, cover_image, locked, locked_by, \
             public, public_slug, created_by, last_updated_by, created_at, updated_at, \
             workflow_template_id, workflow_template_name, workflow_stage_id, workflow_status, workflow_stages \
             FROM ( \
                 SELECT d.id, d.space_id, d.folder_id, d.parent_id, d.title, d.icon, d.cover_image, d.locked, d.locked_by, \
                        d.public, d.public_slug, d.created_by, d.last_updated_by, d.created_at, d.updated_at, \
                        dw.workflow_template_id, wt.name AS workflow_template_name, \
                        dw.current_stage_id AS workflow_stage_id, dw.status AS workflow_status, wt.stages AS workflow_stages, \
                        d.updated_at AS sort_updated_at \
                 FROM documents d \
                 LEFT JOIN document_workflows dw ON dw.document_id = d.id \
                 LEFT JOIN workflow_templates wt ON wt.id = dw.workflow_template_id AND wt.space_id = d.space_id \
                 WHERE d.space_id = ?1 \
                   AND (?2 IS NULL OR dw.status = ?2) \
                   AND (?3 IS NULL OR dw.current_stage_id = ?3) \
                   AND (?4 IS NULL OR dw.workflow_template_id = ?4) \
             ) \
             ORDER BY sort_updated_at DESC, title",
        )
        .bind(&space_id)
        .bind(query.workflow_status.as_deref())
        .bind(query.workflow_stage_id.as_deref())
        .bind(query.workflow_template_id.as_deref())
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?
    } else if let Some(ref folder_id) = query.folder_id {
        sqlx::query(
            "SELECT id, space_id, folder_id, parent_id, title, icon, cover_image, locked, locked_by, \
             public, public_slug, created_by, last_updated_by, created_at, updated_at, \
             workflow_template_id, workflow_template_name, workflow_stage_id, workflow_status, workflow_stages \
             FROM ( \
                 SELECT d.id, d.space_id, d.folder_id, d.parent_id, d.title, d.icon, d.cover_image, d.locked, d.locked_by, \
                        d.public, d.public_slug, d.created_by, d.last_updated_by, d.created_at, d.updated_at, \
                        dw.workflow_template_id, wt.name AS workflow_template_name, \
                        dw.current_stage_id AS workflow_stage_id, dw.status AS workflow_status, wt.stages AS workflow_stages, \
                        d.sort_order \
                 FROM documents d \
                 LEFT JOIN document_workflows dw ON dw.document_id = d.id \
                 LEFT JOIN workflow_templates wt ON wt.id = dw.workflow_template_id AND wt.space_id = d.space_id \
                 WHERE d.space_id = ?1 AND d.folder_id = ?2 \
                   AND (?3 IS NULL OR dw.status = ?3) \
                   AND (?4 IS NULL OR dw.current_stage_id = ?4) \
                   AND (?5 IS NULL OR dw.workflow_template_id = ?5) \
             ) \
             ORDER BY sort_order, title",
        )
        .bind(&space_id)
        .bind(folder_id)
        .bind(query.workflow_status.as_deref())
        .bind(query.workflow_stage_id.as_deref())
        .bind(query.workflow_template_id.as_deref())
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?
    } else {
        sqlx::query(
            "SELECT id, space_id, folder_id, parent_id, title, icon, cover_image, locked, locked_by, \
             public, public_slug, created_by, last_updated_by, created_at, updated_at, \
             workflow_template_id, workflow_template_name, workflow_stage_id, workflow_status, workflow_stages \
             FROM ( \
                 SELECT d.id, d.space_id, d.folder_id, d.parent_id, d.title, d.icon, d.cover_image, d.locked, d.locked_by, \
                        d.public, d.public_slug, d.created_by, d.last_updated_by, d.created_at, d.updated_at, \
                        dw.workflow_template_id, wt.name AS workflow_template_name, \
                        dw.current_stage_id AS workflow_stage_id, dw.status AS workflow_status, wt.stages AS workflow_stages, \
                        d.sort_order \
                 FROM documents d \
                 LEFT JOIN document_workflows dw ON dw.document_id = d.id \
                 LEFT JOIN workflow_templates wt ON wt.id = dw.workflow_template_id AND wt.space_id = d.space_id \
                 WHERE d.space_id = ?1 AND d.folder_id IS NULL \
                   AND (?2 IS NULL OR dw.status = ?2) \
                   AND (?3 IS NULL OR dw.current_stage_id = ?3) \
                   AND (?4 IS NULL OR dw.workflow_template_id = ?4) \
             ) \
             ORDER BY sort_order, title",
        )
        .bind(&space_id)
        .bind(query.workflow_status.as_deref())
        .bind(query.workflow_stage_id.as_deref())
        .bind(query.workflow_template_id.as_deref())
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?
    };

    let result: Vec<DocumentResponse> = rows
        .into_iter()
        .map(|row| {
            let workflow_stage_id = row.try_get::<Option<String>, _>("workflow_stage_id")?;
            let workflow_stages = row.try_get::<Option<String>, _>("workflow_stages")?;
            let workflow_stage_name = workflow_stage_id.as_deref().and_then(|stage_id| {
                workflow_stages
                    .as_deref()
                    .and_then(|raw| {
                        serde_json::from_str::<Vec<crate::spaces::workflows::WorkflowStage>>(raw)
                            .ok()
                    })
                    .and_then(|stages| {
                        stages
                            .into_iter()
                            .find(|stage| stage.id.as_deref() == Some(stage_id))
                            .map(|stage| stage.name)
                    })
            });

            Ok(DocumentResponse {
                id: row.try_get("id")?,
                space_id: row.try_get("space_id")?,
                folder_id: row.try_get("folder_id")?,
                parent_id: row.try_get("parent_id")?,
                title: row.try_get("title")?,
                icon: row.try_get("icon")?,
                cover_image: row.try_get("cover_image")?,
                locked: row.try_get::<i32, _>("locked")? != 0,
                locked_by: row.try_get("locked_by")?,
                public: row.try_get::<i32, _>("public")? != 0,
                public_slug: row.try_get("public_slug")?,
                created_by: row.try_get("created_by")?,
                last_updated_by: row.try_get("last_updated_by")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                workflow_template_id: row.try_get("workflow_template_id")?,
                workflow_template_name: row.try_get("workflow_template_name")?,
                workflow_stage_id,
                workflow_stage_name,
                workflow_status: row.try_get("workflow_status")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(AppError::internal)?;

    Ok(Json(result))
}

/// POST /api/v1/spaces/{space_id}/documents
async fn create_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<CreateDocumentRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    if req.title.len() > 500 {
        return Err(AppError::BadRequest(
            "Document title must be at most 500 characters".into(),
        ));
    }

    if let Some(ref content) = req.content {
        if content.len() > 5_242_880 {
            return Err(AppError::BadRequest(
                "Document content must be at most 5 MB".into(),
            ));
        }
    }

    let doc_id = super::routes::import_id_or_uuid(req.source_id.as_deref())?;
    let created_at = super::routes::import_timestamp_or_now(req.created_at.as_deref())?;
    let updated_at =
        super::routes::import_timestamp_or_default(req.updated_at.as_deref(), &created_at)?;
    let content = req.content.as_deref().unwrap_or("");

    let existing_id: Option<(String,)> = sqlx::query_as("SELECT id FROM documents WHERE id = ?1")
        .bind(&doc_id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;
    if existing_id.is_some() {
        return Err(AppError::Conflict(
            "A document with this import id already exists".into(),
        ));
    }

    super::folders::validate_document_folder_depth(&state.db, &space_id, req.folder_id.as_deref())
        .await?;
    validate_document_parent(
        &state,
        &space_id,
        req.parent_id.as_deref(),
        req.folder_id.as_deref(),
        None,
    )
    .await?;

    let template_metadata = match req.template_id.as_deref() {
        Some(template_id) => resolve_template_metadata(&state, &space_id, template_id).await?,
        None => TemplateMetadata {
            tags: Vec::new(),
            property_values: Vec::new(),
        },
    };

    // Get space slug for file path
    let slug: (String,) = sqlx::query_as("SELECT slug FROM spaces WHERE id = ?1")
        .bind(&space_id)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::internal)?;

    // Build frontmatter
    let frontmatter = format!(
        "title: \"{}\"\nicon: \"{}\"\nlocked: false\npublic: false",
        req.title,
        req.icon.as_deref().unwrap_or("")
    );

    // Write to disk
    let content_path = storage::write_document(&slug.0, &doc_id, &frontmatter, content)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to write document: {}", e)))?;

    // Keep the document and its template metadata together in one database transaction.
    let mut transaction = state.db.begin().await.map_err(AppError::internal)?;
    sqlx::query(
        "INSERT INTO documents (id, space_id, folder_id, parent_id, title, icon, content_path, created_by, last_updated_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10)",
    )
    .bind(&doc_id)
    .bind(&space_id)
    .bind(&req.folder_id)
    .bind(&req.parent_id)
    .bind(&req.title)
    .bind(&req.icon)
    .bind(&content_path)
    .bind(&auth.claims.sub)
    .bind(&created_at)
    .bind(&updated_at)
    .execute(&mut *transaction)
    .await
    .map_err(AppError::internal)?;

    // Auto-subscribe creator
    sqlx::query(
        "INSERT INTO doc_subscriptions (document_id, user_id) VALUES (?1, ?2) \
         ON CONFLICT(document_id, user_id) DO NOTHING",
    )
    .bind(&doc_id)
    .bind(&auth.claims.sub)
    .execute(&mut *transaction)
    .await
    .map_err(AppError::internal)?;

    for (tag_id, _) in &template_metadata.tags {
        sqlx::query(
            "INSERT INTO doc_tags (document_id, tag_id) VALUES (?1, ?2) \
             ON CONFLICT(document_id, tag_id) DO NOTHING",
        )
        .bind(&doc_id)
        .bind(tag_id)
        .execute(&mut *transaction)
        .await
        .map_err(AppError::internal)?;
    }

    for (property_id, value) in &template_metadata.property_values {
        sqlx::query(
            "INSERT INTO property_values (id, document_id, property_id, value, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&doc_id)
        .bind(property_id)
        .bind(value)
        .bind(&created_at)
        .bind(&updated_at)
        .execute(&mut *transaction)
        .await
        .map_err(AppError::internal)?;
    }

    transaction.commit().await.map_err(AppError::internal)?;

    // --- Wire: Search index ---
    if let Err(e) = state.search_index.upsert(DocEntry {
        doc_id: doc_id.clone(),
        space_id: space_id.clone(),
        title: req.title.clone(),
        body: content.to_string(),
        tags: template_metadata
            .tags
            .iter()
            .map(|(_, name)| name.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    }) {
        tracing::warn!("Failed to index document {}: {}", doc_id, e);
    }

    // --- Wire: Webhook dispatch ---
    crate::webhooks::dispatch::dispatch_event(
        &state.db,
        "document.created",
        &serde_json::json!({
            "document_id": doc_id,
            "space_id": space_id,
            "title": req.title,
            "created_by": auth.claims.sub,
        }),
        Some(&space_id),
    )
    .await;

    // --- Wire: Audit log ---
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "document.create",
        Some("document"),
        Some(&doc_id),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(DocumentResponse {
            id: doc_id,
            space_id,
            folder_id: req.folder_id,
            parent_id: req.parent_id,
            title: req.title,
            icon: req.icon,
            cover_image: None,
            locked: false,
            locked_by: None,
            public: false,
            public_slug: None,
            created_by: auth.claims.sub.clone(),
            last_updated_by: auth.claims.sub,
            created_at,
            updated_at,
            workflow_template_id: None,
            workflow_template_name: None,
            workflow_stage_id: None,
            workflow_stage_name: None,
            workflow_status: None,
        }),
    ))
}

async fn resolve_template_metadata(
    state: &AppState,
    space_id: &str,
    template_id: &str,
) -> Result<TemplateMetadata, AppError> {
    let template: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT properties, tags FROM templates WHERE id = ?1 AND space_id = ?2")
            .bind(template_id)
            .bind(space_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    let (properties, tags) =
        template.ok_or_else(|| AppError::NotFound("Template not found".into()))?;

    let tag_names = tags
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default();
    let mut resolved_tags = Vec::new();
    for name in tag_names {
        let name = name.trim();
        if name.is_empty()
            || resolved_tags
                .iter()
                .any(|(_, existing): &(String, String)| existing.eq_ignore_ascii_case(name))
        {
            continue;
        }
        if let Some((id, stored_name)) = sqlx::query_as::<_, (String, String)>(
            "SELECT id, name FROM tags WHERE space_id = ?1 AND lower(name) = lower(?2) ORDER BY name LIMIT 1",
        )
        .bind(space_id)
        .bind(name)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?
        {
            resolved_tags.push((id, stored_name));
        }
    }

    let property_object = properties
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    let mut property_values = Vec::new();
    for (name, value) in property_object {
        let Some(value) = template_property_value(&value) else {
            continue;
        };
        if let Some((id,)) = sqlx::query_as::<_, (String,)>(
            "SELECT id FROM properties_schema WHERE space_id = ?1 AND lower(name) = lower(?2) ORDER BY name LIMIT 1",
        )
        .bind(space_id)
        .bind(&name)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?
        {
            property_values.push((id, value));
        }
    }

    Ok(TemplateMetadata {
        tags: resolved_tags,
        property_values,
    })
}

fn template_property_value(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
            serde_json::to_string(value).ok()
        }
    }
}

/// GET /api/v1/spaces/{space_id}/documents/{doc_id}
async fn get_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;

    let doc = fetch_document(&state, &doc_id).await?;
    Ok(Json(doc))
}

/// PUT /api/v1/spaces/{space_id}/documents/{doc_id}
async fn update_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    Json(req): Json<UpdateDocumentRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    if let Some(ref title) = req.title {
        if title.len() > 500 {
            return Err(AppError::BadRequest(
                "Document title must be at most 500 characters".into(),
            ));
        }
    }

    check_not_locked(&state, &doc_id, &auth.claims.sub).await?;
    let updated_at = chrono::Utc::now().to_rfc3339();

    if let Some(ref title) = req.title {
        sqlx::query(
            "UPDATE documents SET title = ?1, last_updated_by = ?2, \
             updated_at = ?3 WHERE id = ?4",
        )
        .bind(title)
        .bind(&auth.claims.sub)
        .bind(&updated_at)
        .bind(&doc_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    }

    if let Some(ref folder_id) = req.folder_id {
        super::folders::validate_document_folder_depth(&state.db, &space_id, Some(folder_id))
            .await?;
        sqlx::query("UPDATE documents SET folder_id = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(folder_id)
            .bind(&updated_at)
            .bind(&doc_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    if let Some(ref parent_id) = req.parent_id {
        let current_folder: Option<(Option<String>,)> =
            sqlx::query_as("SELECT folder_id FROM documents WHERE id = ?1 AND space_id = ?2")
                .bind(&doc_id)
                .bind(&space_id)
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?;
        let folder_id = req
            .folder_id
            .clone()
            .or_else(|| current_folder.and_then(|row| row.0));
        validate_document_parent(
            &state,
            &space_id,
            Some(parent_id),
            folder_id.as_deref(),
            Some(&doc_id),
        )
        .await?;
        sqlx::query("UPDATE documents SET parent_id = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(parent_id)
            .bind(&updated_at)
            .bind(&doc_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    if let Some(ref icon) = req.icon {
        sqlx::query("UPDATE documents SET icon = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(icon)
            .bind(&updated_at)
            .bind(&doc_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    if let Some(ref cover) = req.cover_image {
        sqlx::query("UPDATE documents SET cover_image = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(cover)
            .bind(&updated_at)
            .bind(&doc_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    // --- Wire: Re-index if title changed ---
    if req.title.is_some() {
        // Read current content for re-indexing
        if let Ok((content_path, title)) = sqlx::query_as::<_, (String, String)>(
            "SELECT content_path, title FROM documents WHERE id = ?1",
        )
        .bind(&doc_id)
        .fetch_one(&state.db)
        .await
        {
            let body = storage::read_document(&content_path)
                .await
                .unwrap_or_default();
            if let Err(e) = state.search_index.upsert(DocEntry {
                doc_id: doc_id.clone(),
                space_id: space_id.clone(),
                title,
                body,
                tags: String::new(),
            }) {
                tracing::warn!("Failed to re-index document {}: {}", doc_id, e);
            }
        }
    }

    // --- Wire: Webhook dispatch ---
    crate::webhooks::dispatch::dispatch_event(
        &state.db,
        "document.updated",
        &serde_json::json!({
            "document_id": doc_id,
            "space_id": space_id,
            "updated_by": auth.claims.sub,
        }),
        Some(&space_id),
    )
    .await;

    // --- Wire: Audit log ---
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "document.update",
        Some("document"),
        Some(&doc_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/v1/spaces/{space_id}/documents/{doc_id}
async fn delete_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    // Get content path for disk cleanup
    let path: Option<(String,)> =
        sqlx::query_as("SELECT content_path FROM documents WHERE id = ?1")
            .bind(&doc_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    if let Some((content_path,)) = path {
        storage::delete_document(&content_path).await.ok();
    }

    sqlx::query("DELETE FROM documents WHERE id = ?1")
        .bind(&doc_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    // --- Wire: Remove from search index ---
    if let Err(e) = state.search_index.delete(&doc_id) {
        tracing::warn!("Failed to remove document {} from index: {}", doc_id, e);
    }

    // --- Wire: Webhook dispatch ---
    crate::webhooks::dispatch::dispatch_event(
        &state.db,
        "document.deleted",
        &serde_json::json!({
            "document_id": doc_id,
            "space_id": space_id,
            "deleted_by": auth.claims.sub,
        }),
        Some(&space_id),
    )
    .await;

    // --- Wire: Audit log ---
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "document.delete",
        Some("document"),
        Some(&doc_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/spaces/{space_id}/documents/{doc_id}/move
async fn move_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    Json(req): Json<MoveDocumentRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;
    super::routes::require_space_role(
        &state,
        &req.target_space_id,
        &auth.claims.sub,
        &["owner", "editor"],
    )
    .await?;

    check_not_locked(&state, &doc_id, &auth.claims.sub).await?;
    validate_document_move_destination(
        &state,
        &space_id,
        &req.target_space_id,
        &doc_id,
        req.folder_id.as_deref(),
        req.parent_id.as_deref(),
    )
    .await?;

    let document_ids = collect_document_subtree_ids(&state, &space_id, &doc_id).await?;
    let updated_at = chrono::Utc::now().to_rfc3339();

    for moved_doc_id in &document_ids {
        sqlx::query(
            "UPDATE documents SET space_id = ?1, folder_id = ?2, last_updated_by = ?3, \
             updated_at = ?4 WHERE id = ?5",
        )
        .bind(&req.target_space_id)
        .bind(&req.folder_id)
        .bind(&auth.claims.sub)
        .bind(&updated_at)
        .bind(moved_doc_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    }

    sqlx::query("UPDATE documents SET parent_id = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(&req.parent_id)
        .bind(&updated_at)
        .bind(&doc_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    if req.target_space_id != space_id {
        cleanup_moved_document_metadata(&state.db, &req.target_space_id, &document_ids).await?;
        update_moved_document_space_records(&state.db, &req.target_space_id, &document_ids).await?;
    }
    reindex_documents(&state, &document_ids).await;

    crate::webhooks::dispatch::dispatch_event(
        &state.db,
        "document.updated",
        &serde_json::json!({
            "document_id": doc_id,
            "space_id": req.target_space_id,
            "previous_space_id": space_id,
            "updated_by": auth.claims.sub,
        }),
        Some(&req.target_space_id),
    )
    .await;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "document.move",
        Some("document"),
        Some(&doc_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/spaces/{space_id}/documents/{doc_id}/content
async fn get_content(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;

    let doc: (String, String, String) =
        sqlx::query_as("SELECT id, title, content_path FROM documents WHERE id = ?1")
            .bind(&doc_id)
            .fetch_one(&state.db)
            .await
            .map_err(AppError::internal)?;

    let content = storage::read_document(&doc.2)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read document: {}", e)))?;

    Ok(Json(DocumentContentResponse {
        id: doc.0,
        title: doc.1,
        content,
    }))
}

/// PUT /api/v1/spaces/{space_id}/documents/{doc_id}/content
async fn update_content(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    Json(req): Json<UpdateContentRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    if req.content.len() > 5_242_880 {
        return Err(AppError::BadRequest(
            "Document content must be at most 5 MB".into(),
        ));
    }

    check_not_locked(&state, &doc_id, &auth.claims.sub).await?;

    let doc: (String, String) =
        sqlx::query_as("SELECT content_path, title FROM documents WHERE id = ?1")
            .bind(&doc_id)
            .fetch_one(&state.db)
            .await
            .map_err(AppError::internal)?;

    // Write to disk
    let full_path = storage::data_dir().join(&doc.0);
    tokio::fs::write(&full_path, &req.content)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to write document: {}", e)))?;

    // Update timestamp
    let updated_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE documents SET last_updated_by = ?1, \
         updated_at = ?2 WHERE id = ?3",
    )
    .bind(&auth.claims.sub)
    .bind(&updated_at)
    .bind(&doc_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    // --- Wire: Re-index with new content ---
    if let Err(e) = state.search_index.upsert(DocEntry {
        doc_id: doc_id.clone(),
        space_id: space_id.clone(),
        title: doc.1,
        body: req.content,
        tags: String::new(),
    }) {
        tracing::warn!("Failed to re-index document {}: {}", doc_id, e);
    }

    // --- Wire: Webhook dispatch ---
    crate::webhooks::dispatch::dispatch_event(
        &state.db,
        "document.content_updated",
        &serde_json::json!({
            "document_id": doc_id,
            "space_id": space_id,
            "updated_by": auth.claims.sub,
        }),
        Some(&space_id),
    )
    .await;

    // --- Wire: Audit log ---
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "document.content_update",
        Some("document"),
        Some(&doc_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/spaces/{space_id}/documents/{doc_id}/lock
async fn lock_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner"]).await?;

    let updated_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE documents SET locked = 1, locked_by = ?1, \
         updated_at = ?2 WHERE id = ?3",
    )
    .bind(&auth.claims.sub)
    .bind(&updated_at)
    .bind(&doc_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "document.lock",
        Some("document"),
        Some(&doc_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/spaces/{space_id}/documents/{doc_id}/unlock
async fn unlock_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner"]).await?;

    let updated_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE documents SET locked = 0, locked_by = NULL, \
         updated_at = ?1 WHERE id = ?2",
    )
    .bind(&updated_at)
    .bind(&doc_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "document.unlock",
        Some("document"),
        Some(&doc_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/spaces/{space_id}/documents/{doc_id}/publish
async fn publish_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let allowed = std::env::var("ALLOW_PUBLIC_PAGES")
        .map(|v| v != "false")
        .unwrap_or(true);

    if !allowed {
        return Err(AppError::BadRequest("Public pages are disabled".into()));
    }

    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    let slug = Uuid::new_v4().to_string().replace('-', "")[..16].to_string();

    let updated_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE documents SET public = 1, public_slug = ?1, \
         updated_at = ?2 WHERE id = ?3",
    )
    .bind(&slug)
    .bind(&updated_at)
    .bind(&doc_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "document.publish",
        Some("document"),
        Some(&doc_id),
    )
    .await;

    Ok(Json(serde_json::json!({ "public_slug": slug })))
}

/// POST /api/v1/spaces/{space_id}/documents/{doc_id}/unpublish
async fn unpublish_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    let updated_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE documents SET public = 0, public_slug = NULL, \
         updated_at = ?1 WHERE id = ?2",
    )
    .bind(&updated_at)
    .bind(&doc_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "document.unpublish",
        Some("document"),
        Some(&doc_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/p/{slug} (public -- no auth)
async fn get_public_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let doc: Option<(
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT d.title, d.icon, d.cover_image, d.content_path, d.updated_at, u.display_name \
         FROM documents d \
         JOIN users u ON d.created_by = u.id \
         WHERE d.public_slug = ?1 AND d.public = 1",
    )
    .bind(&slug)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    let (title, icon, cover, content_path, updated_at, author) =
        doc.ok_or_else(|| AppError::BadRequest("Page not found".into()))?;

    let content = storage::read_document(&content_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read document: {}", e)))?;

    let space: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT s.name, s.accent_color FROM spaces s \
         JOIN documents d ON d.space_id = s.id \
         WHERE d.public_slug = ?1",
    )
    .bind(&slug)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    let (space_name, space_accent) = space.unwrap_or(("".to_string(), None));

    Ok(Json(PublicPageResponse {
        title,
        icon,
        cover_image: cover,
        content,
        space_name,
        space_accent_color: space_accent,
        author,
        updated_at,
    }))
}

// ============================================================
// Helpers
// ============================================================

async fn fetch_document(state: &AppState, doc_id: &str) -> Result<DocumentResponse, AppError> {
    let doc: Option<(
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        i32,
        Option<String>,
        i32,
        Option<String>,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, space_id, folder_id, parent_id, title, icon, cover_image, locked, locked_by, \
             public, public_slug, created_by, last_updated_by, created_at, updated_at \
             FROM documents WHERE id = ?1",
    )
    .bind(doc_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    let (
        id,
        sid,
        fid,
        parent_id,
        title,
        icon,
        cover,
        locked,
        locked_by,
        public,
        slug,
        created_by,
        updated_by,
        created,
        updated,
    ) = doc.ok_or_else(|| AppError::BadRequest("Document not found".into()))?;

    Ok(DocumentResponse {
        id,
        space_id: sid,
        folder_id: fid,
        parent_id,
        title,
        icon,
        cover_image: cover,
        locked: locked != 0,
        locked_by,
        public: public != 0,
        public_slug: slug,
        created_by,
        last_updated_by: updated_by,
        created_at: created,
        updated_at: updated,
        workflow_template_id: None,
        workflow_template_name: None,
        workflow_stage_id: None,
        workflow_stage_name: None,
        workflow_status: None,
    })
}

async fn validate_document_parent(
    state: &AppState,
    space_id: &str,
    parent_id: Option<&str>,
    folder_id: Option<&str>,
    moving_document_id: Option<&str>,
) -> Result<(), AppError> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };

    if moving_document_id == Some(parent_id) {
        return Err(AppError::BadRequest(
            "A page cannot be its own parent".into(),
        ));
    }

    let parent: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT folder_id, parent_id FROM documents WHERE id = ?1 AND space_id = ?2",
    )
    .bind(parent_id)
    .bind(space_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;
    let Some((parent_folder_id, mut ancestor_id)) = parent else {
        return Err(AppError::BadRequest(
            "Parent page not found in this space".into(),
        ));
    };

    if parent_folder_id.as_deref() != folder_id {
        return Err(AppError::BadRequest(
            "Child pages must stay in the same folder as their parent".into(),
        ));
    }

    let mut depth = 1;
    while let Some(current_id) = ancestor_id {
        if moving_document_id == Some(current_id.as_str()) {
            return Err(AppError::BadRequest(
                "A page cannot be moved inside its descendant".into(),
            ));
        }
        depth += 1;
        if depth >= 3 {
            return Err(AppError::BadRequest(
                "Pages can be nested up to 3 levels".into(),
            ));
        }
        ancestor_id = sqlx::query_as::<_, (Option<String>,)>(
            "SELECT parent_id FROM documents WHERE id = ?1 AND space_id = ?2",
        )
        .bind(&current_id)
        .bind(space_id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?
        .and_then(|row| row.0);
    }

    Ok(())
}

async fn document_subtree_depth(
    state: &AppState,
    space_id: &str,
    doc_id: &str,
) -> Result<i32, AppError> {
    let mut max_depth = 1;
    let mut stack = vec![(doc_id.to_string(), 1)];
    let mut seen = std::collections::HashSet::new();

    while let Some((current_id, depth)) = stack.pop() {
        if !seen.insert(current_id.clone()) {
            return Err(AppError::BadRequest(
                "Page hierarchy contains a cycle".into(),
            ));
        }
        max_depth = max_depth.max(depth);

        let children: Vec<(String,)> =
            sqlx::query_as("SELECT id FROM documents WHERE space_id = ?1 AND parent_id = ?2")
                .bind(space_id)
                .bind(&current_id)
                .fetch_all(&state.db)
                .await
                .map_err(AppError::internal)?;

        for (child_id,) in children {
            stack.push((child_id, depth + 1));
        }
    }

    Ok(max_depth)
}

async fn collect_document_subtree_ids(
    state: &AppState,
    space_id: &str,
    doc_id: &str,
) -> Result<Vec<String>, AppError> {
    fetch_document(state, doc_id).await?;
    let mut ids = Vec::new();
    let mut stack = vec![doc_id.to_string()];
    let mut seen = std::collections::HashSet::new();

    while let Some(current_id) = stack.pop() {
        if !seen.insert(current_id.clone()) {
            return Err(AppError::BadRequest(
                "Page hierarchy contains a cycle".into(),
            ));
        }
        ids.push(current_id.clone());

        let children: Vec<(String,)> =
            sqlx::query_as("SELECT id FROM documents WHERE space_id = ?1 AND parent_id = ?2")
                .bind(space_id)
                .bind(&current_id)
                .fetch_all(&state.db)
                .await
                .map_err(AppError::internal)?;

        for (child_id,) in children {
            stack.push(child_id);
        }
    }

    Ok(ids)
}

async fn validate_document_move_destination(
    state: &AppState,
    source_space_id: &str,
    target_space_id: &str,
    doc_id: &str,
    folder_id: Option<&str>,
    parent_id: Option<&str>,
) -> Result<(), AppError> {
    let source_doc: Option<(String,)> =
        sqlx::query_as("SELECT id FROM documents WHERE id = ?1 AND space_id = ?2")
            .bind(doc_id)
            .bind(source_space_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    if source_doc.is_none() {
        return Err(AppError::BadRequest("Document not found".into()));
    }

    super::folders::validate_document_folder_depth(&state.db, target_space_id, folder_id).await?;
    validate_document_parent(state, target_space_id, parent_id, folder_id, Some(doc_id)).await?;

    if let Some(parent_id) = parent_id {
        let parent_depth = document_depth(state, target_space_id, parent_id).await?;
        let subtree_depth = document_subtree_depth(state, source_space_id, doc_id).await?;
        if parent_depth + subtree_depth > 3 {
            return Err(AppError::BadRequest(
                "Pages can be nested up to 3 levels".into(),
            ));
        }
    }

    Ok(())
}

async fn document_depth(state: &AppState, space_id: &str, doc_id: &str) -> Result<i32, AppError> {
    let mut depth = 1;
    let mut current_id = doc_id.to_string();
    let mut seen = std::collections::HashSet::new();

    loop {
        if !seen.insert(current_id.clone()) {
            return Err(AppError::BadRequest(
                "Page hierarchy contains a cycle".into(),
            ));
        }

        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT parent_id FROM documents WHERE id = ?1 AND space_id = ?2")
                .bind(&current_id)
                .bind(space_id)
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?;
        let Some((parent_id,)) = row else {
            return Err(AppError::BadRequest(
                "Parent page not found in this space".into(),
            ));
        };
        match parent_id {
            Some(parent_id) => {
                depth += 1;
                current_id = parent_id;
            }
            None => return Ok(depth),
        }
    }
}

pub(crate) async fn cleanup_moved_document_metadata(
    db: &AnyPool,
    target_space_id: &str,
    document_ids: &[String],
) -> Result<(), AppError> {
    for doc_id in document_ids {
        sqlx::query(
            "DELETE FROM doc_tags WHERE document_id = ?1 \
             AND tag_id NOT IN (SELECT id FROM tags WHERE space_id = ?2)",
        )
        .bind(doc_id)
        .bind(target_space_id)
        .execute(db)
        .await
        .map_err(AppError::internal)?;

        sqlx::query(
            "DELETE FROM property_values WHERE document_id = ?1 \
             AND property_id NOT IN (SELECT id FROM properties_schema WHERE space_id = ?2)",
        )
        .bind(doc_id)
        .bind(target_space_id)
        .execute(db)
        .await
        .map_err(AppError::internal)?;

        sqlx::query(
            "DELETE FROM document_workflows WHERE document_id = ?1 \
             AND workflow_template_id NOT IN (SELECT id FROM workflow_templates WHERE space_id = ?2)",
        )
        .bind(doc_id)
        .bind(target_space_id)
        .execute(db)
        .await
        .map_err(AppError::internal)?;
    }
    Ok(())
}

pub(crate) async fn update_moved_document_space_records(
    db: &AnyPool,
    target_space_id: &str,
    document_ids: &[String],
) -> Result<(), AppError> {
    for doc_id in document_ids {
        sqlx::query("UPDATE attachments SET space_id = ?1 WHERE document_id = ?2")
            .bind(target_space_id)
            .bind(doc_id)
            .execute(db)
            .await
            .map_err(AppError::internal)?;
        sqlx::query("UPDATE excalidraw_files SET space_id = ?1 WHERE document_id = ?2")
            .bind(target_space_id)
            .bind(doc_id)
            .execute(db)
            .await
            .map_err(AppError::internal)?;
    }
    Ok(())
}

pub(crate) async fn reindex_documents(state: &AppState, document_ids: &[String]) {
    for doc_id in document_ids {
        let Ok((space_id, content_path, title)) = sqlx::query_as::<_, (String, String, String)>(
            "SELECT space_id, content_path, title FROM documents WHERE id = ?1",
        )
        .bind(doc_id)
        .fetch_one(&state.db)
        .await
        else {
            continue;
        };
        let body = storage::read_document(&content_path)
            .await
            .unwrap_or_default();
        if let Err(e) = state.search_index.upsert(DocEntry {
            doc_id: doc_id.clone(),
            space_id,
            title,
            body,
            tags: String::new(),
        }) {
            tracing::warn!("Failed to re-index document {}: {}", doc_id, e);
        }
    }
}

async fn check_not_locked(state: &AppState, doc_id: &str, user_id: &str) -> Result<(), AppError> {
    let locked: Option<(i32, Option<String>)> =
        sqlx::query_as("SELECT locked, locked_by FROM documents WHERE id = ?1")
            .bind(doc_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    if let Some((1, Some(locked_by))) = locked {
        if locked_by != user_id {
            return Err(AppError::Conflict(
                "Document is locked by another user".into(),
            ));
        }
    }

    Ok(())
}
