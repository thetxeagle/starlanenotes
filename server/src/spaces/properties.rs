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
use crate::storage;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // Property schema (space-level)
        .route(
            "/spaces/{space_id}/properties",
            get(list_property_schema).post(create_property_schema),
        )
        .route(
            "/spaces/{space_id}/properties/reorder",
            axum::routing::put(reorder_property_schema),
        )
        .route(
            "/spaces/{space_id}/properties/{property_id}",
            axum::routing::put(update_property_schema).delete(delete_property_schema),
        )
        // Property values (document-level)
        .route(
            "/spaces/{space_id}/documents/{doc_id}/properties",
            get(list_property_values).put(set_property_values),
        )
        // Backlinks
        .route(
            "/spaces/{space_id}/documents/{doc_id}/backlinks",
            get(list_backlinks),
        )
}

#[derive(Deserialize)]
pub struct CreatePropertySchemaRequest {
    pub name: String,
    pub property_type: String,
    pub options: Option<serde_json::Value>,
    pub source_id: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Deserialize)]
pub struct ReorderPropertySchemaRequest {
    pub property_ids: Vec<String>,
}

#[derive(Serialize)]
pub struct PropertySchemaResponse {
    pub id: String,
    pub name: String,
    pub property_type: String,
    pub options: Option<serde_json::Value>,
    pub sort_order: i32,
}

#[derive(Serialize)]
pub struct PropertyValueResponse {
    pub property_id: String,
    pub property_name: String,
    pub property_type: String,
    pub options: Option<serde_json::Value>,
    pub value: Option<String>,
}

#[derive(Deserialize)]
pub struct SetPropertyValuesRequest {
    pub values: Vec<PropertyValueEntry>,
}

#[derive(Deserialize)]
pub struct PropertyValueEntry {
    pub source_id: Option<String>,
    pub property_id: String,
    pub value: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Serialize)]
pub struct BacklinkResponse {
    pub document_id: String,
    pub title: String,
    pub space_id: String,
    pub space_name: String,
}

async fn list_property_schema(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    require_property_space_access(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;
    let schemas: Vec<(String, String, String, Option<String>, i32)> = sqlx::query_as(
        "SELECT id, name, property_type, options, sort_order FROM properties_schema \
         WHERE space_id = ?1 ORDER BY sort_order",
    )
    .bind(&space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<PropertySchemaResponse> = schemas
        .into_iter()
        .map(|(id, name, ptype, opts, sort)| PropertySchemaResponse {
            id,
            name,
            property_type: ptype,
            options: opts.and_then(|o| serde_json::from_str(&o).ok()),
            sort_order: sort,
        })
        .collect();

    Ok(Json(result))
}

async fn create_property_schema(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<CreatePropertySchemaRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

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

    if !valid_types.contains(&req.property_type.as_str()) {
        return Err(AppError::BadRequest(format!(
            "Invalid property type: {}",
            req.property_type
        )));
    }

    let prop_id = super::routes::import_id_or_uuid(req.source_id.as_deref())?;
    let created_at = super::routes::import_timestamp_or_now(req.created_at.as_deref())?;
    let options_json = req.options.as_ref().map(|o| o.to_string());

    let existing_id: Option<(String,)> =
        sqlx::query_as("SELECT id FROM properties_schema WHERE id = ?1")
            .bind(&prop_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    if existing_id.is_some() {
        return Err(AppError::Conflict(
            "A property with this import id already exists".into(),
        ));
    }

    let max_sort: (Option<i32>,) =
        sqlx::query_as("SELECT MAX(sort_order) FROM properties_schema WHERE space_id = ?1")
            .bind(&space_id)
            .fetch_one(&state.db)
            .await
            .map_err(AppError::internal)?;

    let sort_order = max_sort.0.unwrap_or(0) + 1;

    sqlx::query(
        "INSERT INTO properties_schema (id, space_id, name, property_type, options, sort_order, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )
    .bind(&prop_id)
    .bind(&space_id)
    .bind(&req.name)
    .bind(&req.property_type)
    .bind(&options_json)
    .bind(sort_order)
    .bind(&created_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok((
        StatusCode::CREATED,
        Json(PropertySchemaResponse {
            id: prop_id,
            name: req.name,
            property_type: req.property_type,
            options: req.options,
            sort_order,
        }),
    ))
}

async fn update_property_schema(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, property_id)): Path<(String, String)>,
    Json(req): Json<CreatePropertySchemaRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    let options_json = req.options.map(|o| o.to_string());

    let result = sqlx::query(
        "UPDATE properties_schema SET name = ?1, property_type = ?2, options = ?3 \
         WHERE id = ?4 AND space_id = ?5",
    )
    .bind(&req.name)
    .bind(&req.property_type)
    .bind(&options_json)
    .bind(&property_id)
    .bind(&space_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "Property not found in this space".into(),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn reorder_property_schema(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<ReorderPropertySchemaRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;

    for (index, property_id) in req.property_ids.iter().enumerate() {
        sqlx::query("UPDATE properties_schema SET sort_order = ?1 WHERE id = ?2 AND space_id = ?3")
            .bind((index + 1) as i32)
            .bind(property_id)
            .bind(&space_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_property_schema(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, property_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner"]).await?;

    let result = sqlx::query("DELETE FROM properties_schema WHERE id = ?1 AND space_id = ?2")
        .bind(&property_id)
        .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "Property not found in this space".into(),
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn list_property_values(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    require_property_document_access(
        &state,
        &space_id,
        &doc_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;
    let values: Vec<(String, String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT ps.id, ps.name, ps.property_type, ps.options, pv.value \
         FROM properties_schema ps \
         LEFT JOIN property_values pv ON ps.id = pv.property_id AND pv.document_id = ?1 \
         WHERE ps.space_id = (SELECT space_id FROM documents WHERE id = ?1) \
         ORDER BY ps.sort_order",
    )
    .bind(&doc_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<PropertyValueResponse> = values
        .into_iter()
        .map(|(pid, name, ptype, options, value)| PropertyValueResponse {
            property_id: pid,
            property_name: name,
            property_type: ptype,
            options: options.and_then(|raw| serde_json::from_str(&raw).ok()),
            value,
        })
        .collect();

    Ok(Json(result))
}

async fn set_property_values(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    Json(req): Json<SetPropertyValuesRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_property_document_access(
        &state,
        &space_id,
        &doc_id,
        &auth.claims.sub,
        &["owner", "editor"],
    )
    .await?;

    for entry in req.values {
        let id = super::routes::import_id_or_uuid(entry.source_id.as_deref())?;
        let created_at = super::routes::import_timestamp_or_now(entry.created_at.as_deref())?;
        let updated_at =
            super::routes::import_timestamp_or_default(entry.updated_at.as_deref(), &created_at)?;
        sqlx::query(
            "INSERT INTO property_values (id, document_id, property_id, value, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6) \
             ON CONFLICT(document_id, property_id) \
             DO UPDATE SET value = ?4, updated_at = ?6",
        )
        .bind(&id)
        .bind(&doc_id)
        .bind(&entry.property_id)
        .bind(&entry.value)
        .bind(&created_at)
        .bind(&updated_at)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn require_property_space_access(
    state: &AppState,
    space_id: &str,
    user_id: &str,
    allowed_roles: &[&str],
) -> Result<(), AppError> {
    super::routes::require_space_role(state, space_id, user_id, allowed_roles).await?;
    Ok(())
}

async fn require_property_document_access(
    state: &AppState,
    space_id: &str,
    document_id: &str,
    user_id: &str,
    allowed_roles: &[&str],
) -> Result<(), AppError> {
    require_property_space_access(state, space_id, user_id, allowed_roles).await?;
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

async fn list_backlinks(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    // Search for documents that contain wiki links to this document
    // This is a simple text search — the CRDT/search index will make this faster later
    let doc_title: Option<(String,)> = sqlx::query_as("SELECT title FROM documents WHERE id = ?1")
        .bind(&doc_id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;

    let title = match doc_title {
        Some((t,)) => t,
        None => return Ok(Json(vec![])),
    };

    // Find docs that may reference this title via [[Title]] wiki links.
    let linking_docs: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT d.id, d.title, d.space_id, s.name, d.content_path \
         FROM documents d JOIN spaces s ON d.space_id = s.id \
         WHERE d.id != ?1 AND d.space_id = ?2",
    )
    .bind(&doc_id)
    .bind(&space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let mut result: Vec<BacklinkResponse> = Vec::new();
    for (document_id, title_value, source_space_id, source_space_name, content_path) in linking_docs
    {
        let content = storage::read_document(&content_path)
            .await
            .unwrap_or_default();
        if !contains_backlink_reference(&content, &title, &doc_id) {
            continue;
        }
        result.push(BacklinkResponse {
            document_id,
            title: title_value,
            space_id: source_space_id,
            space_name: source_space_name,
        });
    }

    Ok(Json(result))
}

fn contains_backlink_reference(content: &str, target_title: &str, target_doc_id: &str) -> bool {
    let normalized_target = normalize_title(target_title);
    let has_wiki_title = extract_wiki_links(content)
        .iter()
        .any(|link| normalize_title(link) == normalized_target);

    if has_wiki_title {
        return true;
    }

    extract_doc_id_links(content)
        .iter()
        .any(|doc_id| doc_id == target_doc_id)
}

fn normalize_title(title: &str) -> String {
    title.trim().to_lowercase()
}

fn extract_wiki_links(content: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut offset = 0;

    while let Some(start_rel) = content[offset..].find("[[") {
        let start = offset + start_rel + 2;
        let Some(end_rel) = content[start..].find("]]") else {
            break;
        };
        let end = start + end_rel;
        let raw = &content[start..end];
        let title = raw.split('|').next().unwrap_or("").trim();
        if !title.is_empty() {
            links.push(title.to_string());
        }
        offset = end + 2;
    }

    links
}

fn extract_doc_id_links(content: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut offset = 0;

    while let Some(start_rel) = content[offset..].find("/docs/") {
        let start = offset + start_rel + "/docs/".len();
        let tail = &content[start..];
        let len = tail
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .count();
        if len > 0 {
            ids.push(tail[..len].to_string());
        }
        offset = start;
    }

    ids
}

#[cfg(test)]
mod tests {
    use super::{contains_backlink_reference, extract_doc_id_links, extract_wiki_links};

    #[test]
    fn extracts_wiki_links_with_alias_support() {
        let content = "Refs: [[Roadmap]], [[Design Notes|design]], [[  Spec ]]";
        assert_eq!(
            extract_wiki_links(content),
            vec!["Roadmap", "Design Notes", "Spec"]
        );
    }

    #[test]
    fn matches_titles_case_insensitively() {
        let content = "Refs: [[RoadMap|alias]] and [[Other]]";
        assert!(contains_backlink_reference(
            content,
            " roadmap ",
            "doc-1234"
        ));
        assert!(!contains_backlink_reference(content, "Missing", "doc-1234"));
    }

    #[test]
    fn detects_doc_id_links() {
        let content = "Open /spaces/a/docs/doc-1234 and also /docs/other_doc";
        assert_eq!(extract_doc_id_links(content), vec!["doc-1234", "other_doc"]);
        assert!(contains_backlink_reference(content, "No Match", "doc-1234"));
    }
}
