use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
        .route(
            "/spaces/{space_id}/templates/{template_id}/drafts",
            post(create_template_draft),
        )
        .route(
            "/spaces/{space_id}/templates/{template_id}/drafts/{draft_id}",
            get(get_template_draft)
                .put(update_template_draft)
                .delete(release_template_draft),
        )
        .route(
            "/spaces/{space_id}/templates/{template_id}/drafts/{draft_id}/publish",
            post(publish_template_draft),
        )
        .route(
            "/spaces/{space_id}/templates/{template_id}/revisions",
            get(list_template_revisions),
        )
        .route(
            "/spaces/{space_id}/templates/{template_id}/revisions/{revision_id}/revert",
            post(revert_template_revision),
        )
}

#[derive(Deserialize, Serialize, Clone)]
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
struct TemplateDraftResponse {
    id: String,
    template_id: String,
    space_id: String,
    owner_id: String,
    snapshot: TemplateRequest,
    created_at: String,
    updated_at: String,
    last_autosaved_at: Option<String>,
    lock_owner_id: Option<String>,
    lock_expires_at: Option<String>,
}

#[derive(Serialize)]
struct TemplateRevisionResponse {
    id: String,
    template_id: String,
    revision_number: i64,
    snapshot: TemplateRequest,
    created_by: String,
    created_at: String,
}

fn draft_response(
    row: (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    ),
) -> TemplateDraftResponse {
    let (
        id,
        template_id,
        space_id,
        owner_id,
        snapshot,
        created_at,
        updated_at,
        last_autosaved_at,
        lock_owner_id,
        lock_expires_at,
    ) = row;
    let parsed = serde_json::from_str::<TemplateRequest>(&snapshot).unwrap_or(TemplateRequest {
        name: "Untitled template".into(),
        description: None,
        content: String::new(),
        properties: None,
        tags: None,
        source_id: None,
        created_at: None,
        updated_at: None,
    });
    TemplateDraftResponse {
        id,
        template_id,
        space_id,
        owner_id,
        snapshot: parsed,
        created_at,
        updated_at,
        last_autosaved_at,
        lock_owner_id,
        lock_expires_at,
    }
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

type CatalogTemplateRaw = (
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
);
type CatalogRevisionRaw = (String, i64, String, String, String);
async fn catalog_template_raw(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    id: &str,
) -> Result<Option<CatalogTemplateRaw>, AppError> {
    sqlx::query_as("SELECT id,space_id,name,description,content,properties,tags,created_by,created_at,updated_at FROM templates WHERE id=$1 AND space_id=$2")
        .bind(id).bind(space).fetch_optional(conn).await.map_err(AppError::internal)
}
async fn require_catalog_template(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    id: &str,
) -> Result<(), AppError> {
    if catalog_template_raw(conn, space, id).await?.is_none() {
        return Err(template_unavailable());
    }
    Ok(())
}
async fn catalog_revisions_raw(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    template: &str,
) -> Result<Vec<CatalogRevisionRaw>, AppError> {
    sqlx::query_as("SELECT r.id,r.revision_number,r.snapshot,r.created_by,r.created_at FROM template_revisions r JOIN templates t ON t.id=r.template_id AND t.space_id=r.space_id WHERE r.template_id=$1 AND r.space_id=$2 ORDER BY r.revision_number DESC LIMIT 10")
        .bind(template).bind(space).fetch_all(conn).await.map_err(AppError::internal)
}
async fn list_templates(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    super::routes::require_catalog_space_view(&mut conn, &auth, &space_id).await?;
    let templates = sqlx::query_as::<_, (String, String, String, Option<String>, String, Option<String>, Option<String>, String, String, String)>(
        "SELECT id, space_id, name, description, content, properties, tags, created_by, created_at, updated_at \
         FROM templates WHERE space_id = $1 ORDER BY name",
    )
    .bind(&space_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    #[cfg(test)]
    super::routes::catalog_after_load().await;
    super::routes::require_catalog_space_view(&mut conn, &auth, &space_id).await?;
    #[cfg(test)]
    super::routes::catalog_after_space_view().await;
    for row in &templates {
        if catalog_template_raw(&mut conn, &space_id, &row.0)
            .await?
            .as_ref()
            != Some(row)
        {
            return Err(template_unavailable());
        }
    }
    Ok(Json(
        templates
            .into_iter()
            .map(row_to_template)
            .collect::<Vec<_>>(),
    ))
}

async fn get_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    super::routes::require_catalog_space_view(&mut conn, &auth, &space_id).await?;
    require_catalog_template(&mut conn, &space_id, &template_id).await?;
    let template = sqlx::query_as::<_, (String, String, String, Option<String>, String, Option<String>, Option<String>, String, String, String)>(
        "SELECT id, space_id, name, description, content, properties, tags, created_by, created_at, updated_at \
         FROM templates WHERE id = $1 AND space_id = $2",
    )
    .bind(&template_id)
    .bind(&space_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?
    .ok_or_else(|| AppError::NotFound("Template not found".into()))?;

    #[cfg(test)]
    super::routes::catalog_after_load().await;
    super::routes::require_catalog_space_view(&mut conn, &auth, &space_id).await?;
    #[cfg(test)]
    super::routes::catalog_after_space_view().await;
    if catalog_template_raw(&mut conn, &space_id, &template_id)
        .await?
        .as_ref()
        != Some(&template)
    {
        return Err(template_unavailable());
    }
    Ok(Json(row_to_template(template)))
}

#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TemplateMutationRow {
    pub(crate) id: String,
    pub(crate) space_id: String,
    pub(crate) name: String,
    pub(crate) description: Option<String>,
    pub(crate) content: String,
    pub(crate) properties: Option<String>,
    pub(crate) tags: Option<String>,
    pub(crate) created_by: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}
#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TemplateMutationDraft {
    pub(crate) id: String,
    pub(crate) template_id: String,
    pub(crate) space_id: String,
    pub(crate) owner_id: String,
    pub(crate) snapshot: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) last_autosaved_at: Option<String>,
    pub(crate) lock_owner_id: Option<String>,
    pub(crate) lock_expires_at: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TemplateMutationRevision {
    pub(crate) id: String,
    pub(crate) template_id: String,
    pub(crate) space_id: String,
    pub(crate) revision_number: i64,
    pub(crate) snapshot: String,
    pub(crate) created_by: String,
    pub(crate) created_at: String,
}
fn template_unavailable() -> AppError {
    AppError::NotFound("Template unavailable".into())
}
fn template_changed() -> AppError {
    AppError::Conflict("Template source changed during mutation".into())
}
pub(crate) async fn template_write_access(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
) -> Result<(), AppError> {
    let access = super::access::resolve_space_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        super::access::Purpose::Routine,
    )
    .await?;
    if !access.allows(super::access::Permission::ManageTemplates) {
        return Err(template_unavailable());
    }
    Ok(())
}
pub(crate) async fn template_mutation_begin(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    template: Option<&str>,
) -> Result<sqlx::Transaction<'static, sqlx::Any>, AppError> {
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[space]).await?;
    if let Some(id) = template {
        if sqlx::query("UPDATE templates SET id=id WHERE id=$1 AND space_id=$2")
            .bind(id)
            .bind(space)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?
            .rows_affected()
            != 1
        {
            return Err(template_unavailable());
        }
    }
    template_write_access(&mut tx, auth, space).await?;
    Ok(tx)
}
async fn mutation_template(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    id: &str,
) -> Result<Option<TemplateMutationRow>, AppError> {
    sqlx::query_as("SELECT id,space_id,name,description,content,properties,tags,created_by,created_at,updated_at FROM templates WHERE id=$1 AND space_id=$2").bind(id).bind(space).fetch_optional(conn).await.map_err(AppError::internal)
}
async fn mutation_draft(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    template: &str,
    id: &str,
) -> Result<Option<TemplateMutationDraft>, AppError> {
    sqlx::query_as("SELECT d.id,d.template_id,d.space_id,d.owner_id,d.snapshot,d.created_at,d.updated_at,d.last_autosaved_at,d.lock_owner_id,d.lock_expires_at FROM template_drafts d JOIN templates t ON t.id=d.template_id AND t.space_id=d.space_id WHERE d.id=$1 AND d.template_id=$2 AND d.space_id=$3").bind(id).bind(template).bind(space).fetch_optional(conn).await.map_err(AppError::internal)
}
async fn lock_template_draft(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    template: &str,
    id: &str,
) -> Result<Option<TemplateMutationDraft>, AppError> {
    sqlx::query("UPDATE template_drafts SET id=id WHERE id=$1 AND template_id=$2 AND space_id=$3")
        .bind(id)
        .bind(template)
        .bind(space)
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    mutation_draft(conn, space, template, id).await
}
async fn mutation_revisions(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    template: &str,
) -> Result<Vec<TemplateMutationRevision>, AppError> {
    sqlx::query_as("SELECT id,template_id,space_id,revision_number,snapshot,created_by,created_at FROM template_revisions WHERE template_id=$1 AND space_id=$2 ORDER BY revision_number,id").bind(template).bind(space).fetch_all(conn).await.map_err(AppError::internal)
}
async fn lock_template_consumers(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    template: &str,
) -> Result<(), AppError> {
    // The schema has independent space/template FKs. Never silently cascade or
    // retain history belonging to a different scope.
    let invalid:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM template_drafts WHERE template_id=$1 AND space_id<>$2)+(SELECT COUNT(*) FROM template_revisions WHERE template_id=$1 AND space_id<>$2)").bind(template).bind(space).fetch_one(&mut *conn).await.map_err(AppError::internal)?;
    if invalid != 0 {
        return Err(template_changed());
    }
    let drafts: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM template_drafts WHERE template_id=$1 AND space_id=$2 ORDER BY id",
    )
    .bind(template)
    .bind(space)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    for id in drafts {
        sqlx::query(
            "UPDATE template_drafts SET id=id WHERE id=$1 AND template_id=$2 AND space_id=$3",
        )
        .bind(id)
        .bind(template)
        .bind(space)
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    }
    let revisions: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM template_revisions WHERE template_id=$1 AND space_id=$2 ORDER BY id",
    )
    .bind(template)
    .bind(space)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    for id in revisions {
        sqlx::query(
            "UPDATE template_revisions SET id=id WHERE id=$1 AND template_id=$2 AND space_id=$3",
        )
        .bind(id)
        .bind(template)
        .bind(space)
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    }
    let invalid:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM template_drafts WHERE template_id=$1 AND space_id<>$2)+(SELECT COUNT(*) FROM template_revisions WHERE template_id=$1 AND space_id<>$2)").bind(template).bind(space).fetch_one(conn).await.map_err(AppError::internal)?;
    if invalid != 0 {
        return Err(template_changed());
    }
    Ok(())
}
async fn put_mutation_template(
    conn: &mut sqlx::AnyConnection,
    row: &TemplateMutationRow,
) -> Result<(), AppError> {
    let n=sqlx::query("UPDATE templates SET name=$1,description=$2,content=$3,properties=$4,tags=$5,updated_at=$6 WHERE id=$7 AND space_id=$8").bind(&row.name).bind(&row.description).bind(&row.content).bind(&row.properties).bind(&row.tags).bind(&row.updated_at).bind(&row.id).bind(&row.space_id).execute(conn).await.map_err(AppError::internal)?.rows_affected();
    if n != 1 {
        return Err(template_changed());
    }
    Ok(())
}
fn apply_template_request(
    row: &mut TemplateMutationRow,
    req: &TemplateRequest,
    now: &str,
) -> Result<(), AppError> {
    row.name = req.name.trim().into();
    row.description = req.description.clone();
    row.content = req.content.clone();
    row.properties = req
        .properties
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(AppError::internal)?;
    row.tags = Some(
        serde_json::to_string(&req.tags.clone().unwrap_or_default()).map_err(AppError::internal)?,
    );
    row.updated_at = now.into();
    Ok(())
}
fn parse_mutation_snapshot(raw: &str) -> Result<TemplateRequest, AppError> {
    serde_json::from_str(raw)
        .map_err(|_| AppError::BadRequest("Template snapshot is malformed".into()))
}
fn check_draft_lock(draft: &TemplateMutationDraft, actor: &str, now: &str) -> Result<(), AppError> {
    if let (Some(owner), Some(expires)) = (&draft.lock_owner_id, &draft.lock_expires_at) {
        if owner != actor && expires.as_str() > now {
            return Err(AppError::Conflict(
                "This template draft is locked by another user".into(),
            ));
        }
    }
    Ok(())
}
fn mutation_draft_response(row: TemplateMutationDraft) -> TemplateDraftResponse {
    draft_response((
        row.id,
        row.template_id,
        row.space_id,
        row.owner_id,
        row.snapshot,
        row.created_at,
        row.updated_at,
        row.last_autosaved_at,
        row.lock_owner_id,
        row.lock_expires_at,
    ))
}
async fn finish_template_mutation(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    expected: Option<&TemplateMutationRow>,
    draft: Option<(&str, Option<&TemplateMutationDraft>)>,
    revisions: Option<&[TemplateMutationRevision]>,
) -> Result<(), AppError> {
    template_write_access(conn, auth, space).await?;
    if mutation_template(conn, space, id).await?.as_ref() != expected {
        return Err(template_changed());
    }
    if let Some((draft_id, expected)) = draft {
        if mutation_draft(conn, space, id, draft_id).await?.as_ref() != expected {
            return Err(template_changed());
        }
    }
    if let Some(expected) = revisions {
        if mutation_revisions(conn, space, id).await?.as_slice() != expected {
            return Err(template_changed());
        }
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TemplateCrudProof {
    pub(crate) before: Option<TemplateMutationRow>,
    pub(crate) after: Option<TemplateMutationRow>,
    pub(crate) drafts_before: Vec<TemplateMutationDraft>,
    pub(crate) revisions_before: Vec<TemplateMutationRevision>,
    pub(crate) drafts_after: Vec<TemplateMutationDraft>,
    pub(crate) revisions_after: Vec<TemplateMutationRevision>,
}
async fn mutation_drafts(
    conn: &mut sqlx::AnyConnection,
    id: &str,
) -> Result<Vec<TemplateMutationDraft>, AppError> {
    sqlx::query_as("SELECT id,template_id,space_id,owner_id,snapshot,created_at,updated_at,last_autosaved_at,lock_owner_id,lock_expires_at FROM template_drafts WHERE template_id=$1 ORDER BY id").bind(id).fetch_all(conn).await.map_err(AppError::internal)
}
#[derive(Clone, Copy)]
pub(crate) enum TemplateCrudOrigin {
    Rest,
    Replay,
}
// Normalization belongs to the caller: REST and legacy replay deliberately have
// different ID, timestamp and raw JSON contracts. Policy and SQL are shared.
pub(crate) async fn apply_template_crud(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    action: &str,
    desired: Option<TemplateMutationRow>,
    origin: TemplateCrudOrigin,
) -> Result<TemplateCrudProof, AppError> {
    let mut before = mutation_template(conn, space, id).await?;
    let global: Option<i64> = sqlx::query_scalar("SELECT 1 FROM templates WHERE id=$1")
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if (action == "create" && global.is_some())
        || (global.is_some() && before.is_none())
        || (action == "update" && before.is_none())
    {
        return Err(template_unavailable());
    }
    if before.is_some() {
        lock_template_consumers(conn, space, id).await?;
    }
    template_write_access(conn, auth, space).await?;
    before = mutation_template(conn, space, id).await?;
    let drafts_before = mutation_drafts(conn, id).await?;
    let revisions_before = mutation_revisions(conn, space, id).await?;
    if action == "create" && (!drafts_before.is_empty() || !revisions_before.is_empty()) {
        return Err(template_changed());
    }
    let after = match action {
        "create" => {
            let row = desired.ok_or_else(template_changed)?;
            if row.id != id || row.space_id != space || row.created_by != auth.claims.sub {
                return Err(template_changed());
            }
            let n=sqlx::query("INSERT INTO templates(id,space_id,name,description,content,properties,tags,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(&row.id).bind(&row.space_id).bind(&row.name).bind(&row.description).bind(&row.content).bind(&row.properties).bind(&row.tags).bind(&row.created_by).bind(&row.created_at).bind(&row.updated_at).execute(&mut *conn).await.map_err(AppError::internal)?.rows_affected();
            if n != 1 {
                return Err(template_changed());
            }
            Some(row)
        }
        "update" => {
            let mut row = desired.ok_or_else(template_changed)?;
            let original = before.as_ref().ok_or_else(template_unavailable)?;
            if matches!(origin, TemplateCrudOrigin::Rest) {
                row.updated_at = Utc::now().to_rfc3339();
            }
            row.id = original.id.clone();
            row.space_id = original.space_id.clone();
            row.created_by = original.created_by.clone();
            row.created_at = original.created_at.clone();
            put_mutation_template(conn, &row).await?;
            Some(row)
        }
        "delete" => {
            if before.is_some() {
                let n = sqlx::query("DELETE FROM templates WHERE id=$1 AND space_id=$2")
                    .bind(id)
                    .bind(space)
                    .execute(&mut *conn)
                    .await
                    .map_err(AppError::internal)?
                    .rows_affected();
                if n != 1 {
                    return Err(template_changed());
                }
            }
            None
        }
        _ => return Err(template_changed()),
    };
    let (drafts_after, revisions_after) = if action == "delete" {
        (Vec::new(), Vec::new())
    } else {
        (drafts_before.clone(), revisions_before.clone())
    };
    Ok(TemplateCrudProof {
        before,
        after,
        drafts_before,
        revisions_before,
        drafts_after,
        revisions_after,
    })
}
pub(crate) async fn finish_template_crud(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    proof: &TemplateCrudProof,
) -> Result<(), AppError> {
    template_write_access(conn, auth, space).await?;
    if mutation_template(conn, space, id).await? != proof.after
        || mutation_drafts(conn, id).await? != proof.drafts_after
        || mutation_revisions(conn, space, id).await? != proof.revisions_after
    {
        return Err(template_changed());
    }
    let foreign: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM template_revisions WHERE template_id=$1 AND space_id<>$2",
    )
    .bind(id)
    .bind(space)
    .fetch_one(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    if foreign != 0 {
        return Err(template_changed());
    }
    let global: Option<String> = sqlx::query_scalar("SELECT space_id FROM templates WHERE id=$1")
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(AppError::internal)?;
    if global.as_deref() != proof.after.as_ref().map(|r| r.space_id.as_str()) {
        return Err(template_changed());
    }
    Ok(())
}

async fn insert_template_revision(
    conn: &mut sqlx::AnyConnection,
    row: &TemplateMutationRevision,
) -> Result<(), AppError> {
    let n=sqlx::query("INSERT INTO template_revisions(id,template_id,space_id,revision_number,snapshot,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(&row.id).bind(&row.template_id).bind(&row.space_id).bind(row.revision_number).bind(&row.snapshot).bind(&row.created_by).bind(&row.created_at).execute(conn).await.map_err(AppError::internal)?.rows_affected();
    if n != 1 {
        return Err(template_changed());
    }
    Ok(())
}
async fn template_mutation_audit(state: &AppState, auth: &AuthUser, action: &str, id: &str) {
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        action,
        Some("template"),
        Some(id),
    )
    .await;
}

async fn create_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<TemplateRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate_template(&req)?;
    let id = super::routes::import_id_or_uuid(req.source_id.as_deref())?;
    let created = super::routes::import_timestamp_or_now(req.created_at.as_deref())?;
    let updated = super::routes::import_timestamp_or_default(req.updated_at.as_deref(), &created)?;
    let mut tx = template_mutation_begin(&state, &auth, &space_id, None).await?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM templates WHERE id=$1")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if exists.is_some() {
        return Err(AppError::Conflict(
            "A template with this import id already exists".into(),
        ));
    }
    let mut row = TemplateMutationRow {
        id: id.clone(),
        space_id: space_id.clone(),
        name: String::new(),
        description: None,
        content: String::new(),
        properties: None,
        tags: None,
        created_by: auth.claims.sub.clone(),
        created_at: created,
        updated_at: updated.clone(),
    };
    apply_template_request(&mut row, &req, &updated)?;
    let proof = apply_template_crud(
        &mut tx,
        &auth,
        &space_id,
        &id,
        "create",
        Some(row),
        TemplateCrudOrigin::Rest,
    )
    .await?;
    finish_template_crud(&mut tx, &auth, &space_id, &id, &proof).await?;
    tx.commit().await.map_err(AppError::internal)?;
    template_mutation_audit(&state, &auth, "template.create", &id).await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({"id":id}))))
}

async fn update_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id)): Path<(String, String)>,
    Json(req): Json<TemplateRequest>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!(action="template.update_attempt",space_id=%space_id,template_id=%template_id,user_id=%auth.claims.sub,content_bytes=req.content.len(),"Template update requested");
    // The core owns its transaction. An error returns only after it is dropped;
    // the deliberate legacy failure audit is outside the rolled-back domain TX.
    if let Err(error) = update_template_core(&state, &auth, &space_id, &template_id, &req).await {
        if matches!(&error, AppError::Internal(_)) {
            tracing::error!(action="template.update_failed",space_id=%space_id,template_id=%template_id,user_id=%auth.claims.sub,error=?error,"Template update database write failed");
        } else {
            tracing::warn!(action="template.update_failed",space_id=%space_id,template_id=%template_id,user_id=%auth.claims.sub,error=?error,"Template update failed");
        }
        template_mutation_audit(&state, &auth, "template.update_failed", &template_id).await;
        return Err(error);
    }
    tracing::info!(action="template.update_succeeded",space_id=%space_id,template_id=%template_id,user_id=%auth.claims.sub,content_bytes=req.content.len(),"Template update persisted");
    Ok(StatusCode::NO_CONTENT)
}
async fn update_template_core(
    state: &AppState,
    auth: &AuthUser,
    space_id: &str,
    template_id: &str,
    req: &TemplateRequest,
) -> Result<(), AppError> {
    validate_template(&req)?;
    let mut tx = template_mutation_begin(&state, &auth, &space_id, Some(&template_id)).await?;
    let mut row = mutation_template(&mut tx, space_id, template_id)
        .await?
        .ok_or_else(template_unavailable)?;
    apply_template_request(&mut row, req, &Utc::now().to_rfc3339())?;
    let proof = apply_template_crud(
        &mut tx,
        auth,
        space_id,
        template_id,
        "update",
        Some(row),
        TemplateCrudOrigin::Rest,
    )
    .await?;
    finish_template_crud(&mut tx, auth, space_id, template_id, &proof).await?;
    tx.commit().await.map_err(AppError::internal)?;
    template_mutation_audit(&state, &auth, "template.update", &template_id).await;
    Ok(())
}

async fn create_template_draft(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = template_mutation_begin(&state, &auth, &space_id, Some(&template_id)).await?;
    lock_template_consumers(&mut tx, &space_id, &template_id).await?;
    template_write_access(&mut tx, &auth, &space_id).await?;
    let template = mutation_template(&mut tx, &space_id, &template_id)
        .await?
        .ok_or_else(template_unavailable)?;
    let existing: Option<String> =
        sqlx::query_scalar("SELECT id FROM template_drafts WHERE template_id=$1 AND space_id=$2")
            .bind(&template_id)
            .bind(&space_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    let now = Utc::now();
    let now_text = now.to_rfc3339();
    let expires = (now + Duration::hours(1)).to_rfc3339();
    let row = if let Some(id) = existing {
        let mut draft = mutation_draft(&mut tx, &space_id, &template_id, &id)
            .await?
            .ok_or_else(template_unavailable)?;
        check_draft_lock(&draft, &auth.claims.sub, &now_text)?;
        draft.lock_owner_id = Some(auth.claims.sub.clone());
        draft.lock_expires_at = Some(expires);
        draft.updated_at = now_text;
        let n=sqlx::query("UPDATE template_drafts SET lock_owner_id=$1,lock_expires_at=$2,updated_at=$3 WHERE id=$4 AND template_id=$5 AND space_id=$6").bind(&draft.lock_owner_id).bind(&draft.lock_expires_at).bind(&draft.updated_at).bind(&id).bind(&template_id).bind(&space_id).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected();
        if n != 1 {
            return Err(template_changed());
        }
        draft
    } else {
        let properties = template
            .properties
            .as_deref()
            .map(serde_json::from_str::<serde_json::Value>)
            .transpose()
            .map_err(|_| AppError::BadRequest("Template properties are malformed".into()))?;
        let tags = template
            .tags
            .as_deref()
            .map(serde_json::from_str::<Vec<String>>)
            .transpose()
            .map_err(|_| AppError::BadRequest("Template tags are malformed".into()))?
            .unwrap_or_default();
        let req = TemplateRequest {
            name: template.name.clone(),
            description: template.description.clone(),
            content: template.content.clone(),
            properties,
            tags: Some(tags),
            source_id: None,
            created_at: None,
            updated_at: None,
        };
        let draft = TemplateMutationDraft {
            id: Uuid::new_v4().to_string(),
            template_id: template_id.clone(),
            space_id: space_id.clone(),
            owner_id: auth.claims.sub.clone(),
            snapshot: serde_json::to_string(&req).map_err(AppError::internal)?,
            created_at: now_text.clone(),
            updated_at: now_text,
            last_autosaved_at: None,
            lock_owner_id: Some(auth.claims.sub.clone()),
            lock_expires_at: Some(expires),
        };
        let n=sqlx::query("INSERT INTO template_drafts(id,template_id,space_id,owner_id,snapshot,created_at,updated_at,last_autosaved_at,lock_owner_id,lock_expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(&draft.id).bind(&draft.template_id).bind(&draft.space_id).bind(&draft.owner_id).bind(&draft.snapshot).bind(&draft.created_at).bind(&draft.updated_at).bind(&draft.last_autosaved_at).bind(&draft.lock_owner_id).bind(&draft.lock_expires_at).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected();
        if n != 1 {
            return Err(template_changed());
        }
        draft
    };
    finish_template_mutation(
        &mut tx,
        &auth,
        &space_id,
        &template_id,
        Some(&template),
        Some((&row.id, Some(&row))),
        None,
    )
    .await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(Json(mutation_draft_response(row)))
}

async fn update_template_draft(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id, draft_id)): Path<(String, String, String)>,
    Json(req): Json<TemplateRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate_template(&req)?;
    let mut tx = template_mutation_begin(&state, &auth, &space_id, Some(&template_id)).await?;
    lock_template_consumers(&mut tx, &space_id, &template_id).await?;
    template_write_access(&mut tx, &auth, &space_id).await?;
    let mut row = lock_template_draft(&mut tx, &space_id, &template_id, &draft_id)
        .await?
        .ok_or_else(template_unavailable)?;
    let template = mutation_template(&mut tx, &space_id, &template_id)
        .await?
        .ok_or_else(template_unavailable)?;
    let now = Utc::now();
    let text = now.to_rfc3339();
    check_draft_lock(&row, &auth.claims.sub, &text)?;
    row.snapshot = serde_json::to_string(&req).map_err(AppError::internal)?;
    row.updated_at = text.clone();
    row.last_autosaved_at = Some(text);
    row.lock_owner_id = Some(auth.claims.sub.clone());
    row.lock_expires_at = Some((now + Duration::hours(1)).to_rfc3339());
    let n=sqlx::query("UPDATE template_drafts SET snapshot=$1,updated_at=$2,last_autosaved_at=$3,lock_owner_id=$4,lock_expires_at=$5 WHERE id=$6 AND template_id=$7 AND space_id=$8").bind(&row.snapshot).bind(&row.updated_at).bind(&row.last_autosaved_at).bind(&row.lock_owner_id).bind(&row.lock_expires_at).bind(&draft_id).bind(&template_id).bind(&space_id).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected();
    if n != 1 {
        return Err(template_changed());
    }
    finish_template_mutation(
        &mut tx,
        &auth,
        &space_id,
        &template_id,
        Some(&template),
        Some((&draft_id, Some(&row))),
        None,
    )
    .await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(Json(mutation_draft_response(row)))
}

// Compare raw storage, including the complete scoped parent, before projection.
// Snapshot parsing remains the existing response contract.
#[derive(sqlx::FromRow, PartialEq, Eq)]
struct ReadableTemplateDraft {
    id: String,
    template_id: String,
    space_id: String,
    owner_id: String,
    snapshot: String,
    created_at: String,
    updated_at: String,
    last_autosaved_at: Option<String>,
    lock_owner_id: Option<String>,
    lock_expires_at: Option<String>,
    parent_name: String,
    parent_description: Option<String>,
    parent_content: String,
    parent_properties: Option<String>,
    parent_tags: Option<String>,
    parent_created_by: String,
    parent_created_at: String,
    parent_updated_at: String,
}
async fn readable_template_draft(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    template: &str,
    draft: &str,
) -> Result<ReadableTemplateDraft, AppError> {
    sqlx::query_as("SELECT d.id,d.template_id,d.space_id,d.owner_id,d.snapshot,d.created_at,d.updated_at,d.last_autosaved_at,d.lock_owner_id,d.lock_expires_at,t.name AS parent_name,t.description AS parent_description,t.content AS parent_content,t.properties AS parent_properties,t.tags AS parent_tags,t.created_by AS parent_created_by,t.created_at AS parent_created_at,t.updated_at AS parent_updated_at FROM template_drafts d JOIN templates t ON t.id=d.template_id AND t.space_id=d.space_id WHERE d.id=$1 AND d.template_id=$2 AND d.space_id=$3")
        .bind(draft).bind(template).bind(space).fetch_optional(conn).await
        .map_err(AppError::internal)?.ok_or_else(|| AppError::NotFound("Template draft not found".into()))
}
async fn get_template_draft(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id, draft_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    super::access::require_template_draft_read(&mut conn, &auth, &space_id).await?;
    let row = readable_template_draft(&mut conn, &space_id, &template_id, &draft_id).await?;
    #[cfg(test)]
    super::routes::catalog_after_load().await;
    super::access::require_template_draft_read(&mut conn, &auth, &space_id).await?;
    #[cfg(test)]
    super::routes::catalog_after_space_view().await;
    if readable_template_draft(&mut conn, &space_id, &template_id, &draft_id).await? != row {
        return Err(AppError::NotFound("Template draft not found".into()));
    }
    Ok(Json(draft_response((
        row.id,
        row.template_id,
        row.space_id,
        row.owner_id,
        row.snapshot,
        row.created_at,
        row.updated_at,
        row.last_autosaved_at,
        row.lock_owner_id,
        row.lock_expires_at,
    ))))
}

async fn publish_template_draft(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id, draft_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = template_mutation_begin(&state, &auth, &space_id, Some(&template_id)).await?;
    lock_template_consumers(&mut tx, &space_id, &template_id).await?;
    template_write_access(&mut tx, &auth, &space_id).await?;
    let draft = mutation_draft(&mut tx, &space_id, &template_id, &draft_id)
        .await?
        .ok_or_else(template_unavailable)?;
    if draft.owner_id != auth.claims.sub {
        return Err(AppError::Forbidden(
            "Only the template draft owner can publish it".into(),
        ));
    }
    // Publishing intentionally retains the existing owner-only rule; another
    // editor's content lock does not transfer draft ownership or publish rights.
    let req = parse_mutation_snapshot(&draft.snapshot)?;
    validate_template(&req)?;
    let mut row = mutation_template(&mut tx, &space_id, &template_id)
        .await?
        .ok_or_else(template_unavailable)?;
    let now = Utc::now().to_rfc3339();
    apply_template_request(&mut row, &req, &now)?;
    let mut revisions = mutation_revisions(&mut tx, &space_id, &template_id).await?;
    let next = revisions
        .iter()
        .map(|r| r.revision_number)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(template_changed)?;
    let revision = TemplateMutationRevision {
        id: Uuid::new_v4().to_string(),
        template_id: template_id.clone(),
        space_id: space_id.clone(),
        revision_number: next,
        snapshot: serde_json::to_string(&req).map_err(AppError::internal)?,
        created_by: auth.claims.sub.clone(),
        created_at: now,
    };
    put_mutation_template(&mut tx, &row).await?;
    insert_template_revision(&mut tx, &revision).await?;
    // History/canonical writes may invoke database consumers. Publication must
    // still own the exact locked draft it is about to consume.
    if mutation_draft(&mut tx, &space_id, &template_id, &draft_id)
        .await?
        .as_ref()
        != Some(&draft)
    {
        return Err(template_changed());
    }
    let n =
        sqlx::query("DELETE FROM template_drafts WHERE id=$1 AND template_id=$2 AND space_id=$3")
            .bind(&draft_id)
            .bind(&template_id)
            .bind(&space_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?
            .rows_affected();
    if n != 1 {
        return Err(template_changed());
    }
    sqlx::query("DELETE FROM template_revisions WHERE template_id=$1 AND space_id=$2 AND revision_number<=$3").bind(&template_id).bind(&space_id).bind(next-10).execute(&mut *tx).await.map_err(AppError::internal)?;
    revisions.push(revision);
    revisions.retain(|r| r.revision_number > next - 10);
    finish_template_mutation(
        &mut tx,
        &auth,
        &space_id,
        &template_id,
        Some(&row),
        Some((&draft_id, None)),
        Some(&revisions),
    )
    .await?;
    tx.commit().await.map_err(AppError::internal)?;
    tracing::info!(action="template.published",space_id=%space_id,template_id=%template_id,draft_id=%draft_id,revision=next,user_id=%auth.claims.sub,"Template draft published");
    Ok(StatusCode::NO_CONTENT)
}

async fn release_template_draft(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id, draft_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = template_mutation_begin(&state, &auth, &space_id, None).await?;
    let n = sqlx::query("UPDATE templates SET id=id WHERE id=$1 AND space_id=$2")
        .bind(&template_id)
        .bind(&space_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?
        .rows_affected();
    // Existing release is a scoped, metadata-free no-op for absent targets.
    if n == 0 {
        template_write_access(&mut tx, &auth, &space_id).await?;
        tx.commit().await.map_err(AppError::internal)?;
        return Ok(StatusCode::NO_CONTENT);
    }
    lock_template_consumers(&mut tx, &space_id, &template_id).await?;
    template_write_access(&mut tx, &auth, &space_id).await?;
    let template = mutation_template(&mut tx, &space_id, &template_id)
        .await?
        .ok_or_else(template_unavailable)?;
    let mut draft = lock_template_draft(&mut tx, &space_id, &template_id, &draft_id).await?;
    if let Some(row) = &mut draft {
        row.lock_owner_id = None;
        row.lock_expires_at = None;
        let n=sqlx::query("UPDATE template_drafts SET lock_owner_id=NULL,lock_expires_at=NULL WHERE id=$1 AND template_id=$2 AND space_id=$3").bind(&draft_id).bind(&template_id).bind(&space_id).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected();
        if n != 1 {
            return Err(template_changed());
        }
    }
    finish_template_mutation(
        &mut tx,
        &auth,
        &space_id,
        &template_id,
        Some(&template),
        Some((&draft_id, draft.as_ref())),
        None,
    )
    .await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_template_revisions(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    super::routes::require_catalog_space_view(&mut conn, &auth, &space_id).await?;
    require_catalog_template(&mut conn, &space_id, &template_id).await?;
    let parent = catalog_template_raw(&mut conn, &space_id, &template_id)
        .await?
        .ok_or_else(template_unavailable)?;
    let rows = catalog_revisions_raw(&mut conn, &space_id, &template_id).await?;
    #[cfg(test)]
    super::routes::catalog_after_load().await;
    super::routes::require_catalog_space_view(&mut conn, &auth, &space_id).await?;
    #[cfg(test)]
    super::routes::catalog_after_space_view().await;
    if catalog_template_raw(&mut conn, &space_id, &template_id)
        .await?
        .as_ref()
        != Some(&parent)
        || catalog_revisions_raw(&mut conn, &space_id, &template_id).await? != rows
    {
        return Err(template_unavailable());
    }
    let revisions = rows
        .into_iter()
        .filter_map(|(id, revision_number, snapshot, created_by, created_at)| {
            serde_json::from_str::<TemplateRequest>(&snapshot)
                .ok()
                .map(|snapshot| TemplateRevisionResponse {
                    id,
                    template_id: template_id.clone(),
                    revision_number,
                    snapshot,
                    created_by,
                    created_at,
                })
        })
        .collect::<Vec<_>>();
    Ok(Json(revisions))
}

async fn revert_template_revision(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id, revision_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = template_mutation_begin(&state, &auth, &space_id, Some(&template_id)).await?;
    lock_template_consumers(&mut tx, &space_id, &template_id).await?;
    template_write_access(&mut tx, &auth, &space_id).await?;
    let mut revisions = mutation_revisions(&mut tx, &space_id, &template_id).await?;
    let source = revisions
        .iter()
        .find(|r| r.id == revision_id)
        .ok_or_else(|| AppError::NotFound("Template revision not found".into()))?;
    let req = parse_mutation_snapshot(&source.snapshot)?;
    validate_template(&req)?;
    let mut row = mutation_template(&mut tx, &space_id, &template_id)
        .await?
        .ok_or_else(template_unavailable)?;
    let now = Utc::now().to_rfc3339();
    apply_template_request(&mut row, &req, &now)?;
    let next = revisions
        .iter()
        .map(|r| r.revision_number)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(template_changed)?;
    let revision = TemplateMutationRevision {
        id: Uuid::new_v4().to_string(),
        template_id: template_id.clone(),
        space_id: space_id.clone(),
        revision_number: next,
        snapshot: serde_json::to_string(&req).map_err(AppError::internal)?,
        created_by: auth.claims.sub.clone(),
        created_at: now,
    };
    put_mutation_template(&mut tx, &row).await?;
    insert_template_revision(&mut tx, &revision).await?;
    revisions.push(revision);
    finish_template_mutation(
        &mut tx,
        &auth,
        &space_id,
        &template_id,
        Some(&row),
        None,
        Some(&revisions),
    )
    .await?;
    tx.commit().await.map_err(AppError::internal)?;
    tracing::info!(action="template.reverted",space_id=%space_id,template_id=%template_id,revision_id=%revision_id,user_id=%auth.claims.sub,"Template revision reverted");
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, template_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = template_mutation_begin(&state, &auth, &space_id, Some(&template_id)).await?;
    let proof = apply_template_crud(
        &mut tx,
        &auth,
        &space_id,
        &template_id,
        "delete",
        None,
        TemplateCrudOrigin::Rest,
    )
    .await?;
    finish_template_crud(&mut tx, &auth, &space_id, &template_id, &proof).await?;
    tx.commit().await.map_err(AppError::internal)?;
    template_mutation_audit(&state, &auth, "template.delete", &template_id).await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod catalog_read_tests {
    use super::super::routes::catalog_tests::{
        auth, body, catalog_state, copy_auth, key, pause, pause_final,
    };
    use super::*;
    #[tokio::test]
    async fn catalog_templates_preserve_wire_snapshots_order_and_revision_limit() {
        let state = catalog_state(false).await;
        let read = key(&state, "template-read", Some("viewer-a"), "[\"read\"]").await;
        let list = body(
            list_templates(
                State(state.clone()),
                copy_auth(&read),
                Path("space-a".into()),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["content"], "TEMPLATE SECRET");
        assert_eq!(list[0]["tags"], serde_json::json!([]));
        let one = body(
            get_template(
                State(state.clone()),
                copy_auth(&read),
                Path(("space-a".into(), "template-a".into())),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(one["id"], "template-a");
        let revisions = body(
            list_template_revisions(
                State(state.clone()),
                copy_auth(&read),
                Path(("space-a".into(), "template-a".into())),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(revisions.as_array().unwrap().len(), 10);
        assert_eq!(revisions[0]["revision_number"], 13);
        assert_eq!(revisions[9]["revision_number"], 4);
        assert_eq!(revisions[0]["snapshot"]["content"], "SNAPSHOT SECRET");
        for id in ["missing-template", "foreign-template"] {
            assert!(matches!(
                get_template(
                    State(state.clone()),
                    copy_auth(&read),
                    Path(("space-a".into(), id.into()))
                )
                .await,
                Err(AppError::NotFound(_))
            ));
            assert!(matches!(
                list_template_revisions(
                    State(state.clone()),
                    copy_auth(&read),
                    Path(("space-a".into(), id.into()))
                )
                .await,
                Err(AppError::NotFound(_))
            ));
        }
        sqlx::query("UPDATE api_keys SET scopes='[\"write\"]' WHERE id='template-read'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(
            list_templates(State(state.clone()), read, Path("space-a".into()))
                .await
                .is_err()
        );
        let service = key(&state, "template-service", None, "[\"admin\"]").await;
        assert!(matches!(
            list_templates(State(state), service, Path("space-a".into())).await,
            Err(AppError::Unauthorized(_))
        ));
    }
    #[tokio::test]
    async fn catalog_template_revisions_require_actual_scoped_canonical_parent() {
        let state = catalog_state(false).await;
        sqlx::query("UPDATE templates SET space_id='space-b' WHERE id='template-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(matches!(
            list_template_revisions(
                State(state.clone()),
                auth("viewer-a"),
                Path(("space-a".into(), "template-a".into()))
            )
            .await,
            Err(AppError::NotFound(_))
        ));
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("DELETE FROM templates WHERE id='template-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&state.db)
            .await
            .unwrap();
        let retained: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM template_revisions WHERE template_id='template-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(retained, 13);
        assert!(matches!(
            list_template_revisions(
                State(state),
                auth("viewer-a"),
                Path(("space-a".into(), "template-a".into()))
            )
            .await,
            Err(AppError::NotFound(_))
        ));
    }
    #[tokio::test]
    async fn catalog_template_reads_recheck_external_wal_scope_visibility_and_credentials() {
        for sql in ["DELETE FROM sessions WHERE id='metadata-viewer-a'","UPDATE templates SET space_id='space-b' WHERE id='template-a'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')"] {
            let state=catalog_state(true).await;pause(&state,sql);assert!(get_template(State(state.clone()),auth("viewer-a"),Path(("space-a".into(),"template-a".into()))).await.is_err());
        }
        let state = catalog_state(true).await;
        pause(
            &state,
            "UPDATE templates SET space_id='space-b' WHERE id='template-a'",
        );
        assert!(list_template_revisions(
            State(state),
            auth("viewer-a"),
            Path(("space-a".into(), "template-a".into()))
        )
        .await
        .is_err());
        let state = catalog_state(true).await;
        pause(&state, "DELETE FROM sessions WHERE id='metadata-viewer-a'");
        assert!(
            list_templates(State(state), auth("viewer-a"), Path("space-a".into()))
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn catalog_template_and_revision_bindings_are_last_after_space_policy_wait() {
        for (kind, sql) in [
            (
                "get",
                "UPDATE templates SET space_id='space-b' WHERE id='template-a'",
            ),
            ("list", "DELETE FROM templates WHERE id='template-a'"),
            (
                "revisions",
                "UPDATE template_revisions SET space_id='space-b' WHERE id='revision-13'",
            ),
            (
                "revisions",
                "DELETE FROM template_revisions WHERE id='revision-13'",
            ),
        ] {
            let state = catalog_state(true).await;
            pause_final(&state, sql);
            let denied = match kind {
                "get" => get_template(
                    State(state.clone()),
                    auth("viewer-a"),
                    Path(("space-a".into(), "template-a".into())),
                )
                .await
                .is_err(),
                "list" => list_templates(
                    State(state.clone()),
                    auth("viewer-a"),
                    Path("space-a".into()),
                )
                .await
                .is_err(),
                _ => list_template_revisions(
                    State(state.clone()),
                    auth("viewer-a"),
                    Path(("space-a".into(), "template-a".into())),
                )
                .await
                .is_err(),
            };
            assert!(denied, "{kind}: {sql}");
        }
    }
    #[tokio::test]
    async fn catalog_full_source_preserves_malformed_fallback_without_writes() {
        let state = catalog_state(false).await;
        sqlx::query(
            "UPDATE templates SET properties='malformed',tags='malformed' WHERE id='template-a'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query("UPDATE template_revisions SET snapshot='malformed' WHERE id='revision-13'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = crate::spaces::documents::tests::move_database_snapshot(&state).await;
        let one = body(
            get_template(
                State(state.clone()),
                auth("viewer-a"),
                Path(("space-a".into(), "template-a".into())),
            )
            .await
            .unwrap(),
        )
        .await;
        assert!(one["properties"].is_null());
        assert_eq!(one["tags"], serde_json::json!([]));
        let list = body(
            list_templates(
                State(state.clone()),
                auth("viewer-a"),
                Path("space-a".into()),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(list[0], one);
        let revisions = body(
            list_template_revisions(
                State(state.clone()),
                auth("viewer-a"),
                Path(("space-a".into(), "template-a".into())),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(revisions.as_array().unwrap().len(), 9);
        assert_eq!(revisions[0]["revision_number"], 12);
        assert_eq!(revisions[8]["revision_number"], 4);
        assert_eq!(
            crate::spaces::documents::tests::move_database_snapshot(&state).await,
            before
        );
    }
    #[tokio::test]
    async fn catalog_full_source_wal_template_bytes_are_last() {
        let mut stale = Vec::new();
        for sql in [
            "UPDATE templates SET content='NEW BODY' WHERE id='template-a'",
            "UPDATE templates SET properties='{\"changed\":true}' WHERE id='template-a'",
            "UPDATE templates SET tags='[\"changed\"]' WHERE id='template-a'",
            "UPDATE templates SET created_by='editor-a' WHERE id='template-a'",
            "UPDATE templates SET name='Changed',description='Changed' WHERE id='template-a'",
            "UPDATE templates SET created_at='2099-01-01',updated_at='2099-01-02' WHERE id='template-a'",
        ] {
            for kind in ["get", "list"] {
                let state=catalog_state(true).await;pause_final(&state,sql);
                let denied=if kind=="get" {get_template(State(state.clone()),auth("viewer-a"),Path(("space-a".into(),"template-a".into()))).await.is_err()} else {list_templates(State(state.clone()),auth("viewer-a"),Path("space-a".into())).await.is_err()};
                if !denied {stale.push(format!("{kind}: {sql}"));}
                assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM templates WHERE id='template-a' AND space_id='space-a'").fetch_one(&state.db).await.unwrap(),1);
            }
        }
        assert!(stale.is_empty(), "stale same-ID source emitted: {stale:?}");
    }
    #[tokio::test]
    async fn catalog_full_source_wal_revision_bytes_and_parent_are_last() {
        let mut stale = Vec::new();
        for sql in [
            "UPDATE template_revisions SET snapshot='{\"name\":\"Changed\",\"content\":\"NEW\"}' WHERE id='revision-13'",
            "UPDATE template_revisions SET snapshot='malformed' WHERE id='revision-13'",
            "UPDATE template_revisions SET created_by='editor-a' WHERE id='revision-13'",
            "UPDATE template_revisions SET revision_number=99 WHERE id='revision-13'",
            "UPDATE template_revisions SET created_at='2099-01-01' WHERE id='revision-13'",
            "UPDATE templates SET content='NEW PARENT' WHERE id='template-a'",
        ] {
            let state=catalog_state(true).await;pause_final(&state,sql);
            if list_template_revisions(State(state.clone()),auth("viewer-a"),Path(("space-a".into(),"template-a".into()))).await.is_ok() {stale.push(sql);}
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM template_revisions WHERE id='revision-13' AND template_id='template-a' AND space_id='space-a'").fetch_one(&state.db).await.unwrap(),1);
        }
        assert!(
            stale.is_empty(),
            "stale same-ID revision emitted: {stale:?}"
        );
    }
}

#[cfg(test)]
mod template_draft_read_tests {
    use super::super::routes::catalog_tests::{
        auth, body, catalog_state, copy_auth, key, pause, pause_final,
    };
    use super::*;
    async fn seed(state: &AppState) {
        sqlx::query("INSERT INTO template_drafts(id,template_id,space_id,owner_id,snapshot,lock_owner_id,lock_expires_at) VALUES('shared-template-draft','template-a','space-a','owner-a',$1,'owner-a','2999-01-01T00:00:00Z')")
            .bind(r#"{"name":"Shared raw draft","content":"PRIVATE TEMPLATE BODY","description":null,"properties":{"x":"y"},"tags":["legacy"],"source_id":"raw-source","created_at":"raw-created","updated_at":"raw-updated"}"#)
            .execute(&state.db).await.unwrap();
    }
    async fn get(state: &AppState, actor: AuthUser) -> Result<impl IntoResponse, AppError> {
        get_template_draft(
            State(state.clone()),
            actor,
            Path((
                "space-a".into(),
                "template-a".into(),
                "shared-template-draft".into(),
            )),
        )
        .await
    }
    #[tokio::test]
    async fn template_draft_reads_keep_shared_editor_lock_and_wire_contract() {
        let state = catalog_state(false).await;
        seed(&state).await;
        let row = body(get(&state, auth("editor-a")).await.unwrap()).await;
        assert_eq!(row["snapshot"]["content"], "PRIVATE TEMPLATE BODY");
        assert_eq!(row["owner_id"], "owner-a");
        assert_eq!(row["lock_owner_id"], "owner-a");
        assert_eq!(row["snapshot"]["source_id"], "raw-source");
        assert!(get(&state, auth("viewer-a")).await.is_err());
        let read = key(&state, "draft-read", Some("editor-a"), "[\"read\"]").await;
        assert!(get(&state, copy_auth(&read)).await.is_ok());
        let write = key(&state, "draft-write", Some("editor-a"), "[\"write\"]").await;
        assert!(get(&state, write).await.is_err());
        let service = key(&state, "draft-service", None, "[\"admin\"]").await;
        assert!(get(&state, service).await.is_err());
    }
    #[tokio::test]
    async fn template_draft_read_http_method_scopes_remain_exact() {
        use tower::ServiceExt;
        let state = catalog_state(false).await;
        seed(&state).await;
        for (name, scope, expected) in [
            ("http-draft-read", "read", StatusCode::OK),
            ("http-draft-write", "write", StatusCode::FORBIDDEN),
        ] {
            let _ = key(&state, name, Some("editor-a"), &format!("[\"{scope}\"]")).await;
            sqlx::query("UPDATE api_keys SET key_hash=$1 WHERE id=$2")
                .bind(crate::auth::api_keys::hash_api_key(name))
                .bind(name)
                .execute(&state.db)
                .await
                .unwrap();
            let response = router()
                .with_state(state.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .uri("/spaces/space-a/templates/template-a/drafts/shared-template-draft")
                        .header("X-API-Key", name)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{scope}");
        }
    }
    #[tokio::test]
    async fn template_draft_reads_use_exact_global_and_legacy_permissions_without_view() {
        let state = catalog_state(false).await;
        seed(&state).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('edit-only','space-a','Edit only','[\"edit_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='edit-only' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(get(&state, auth("editor-a")).await.is_ok());
        sqlx::query("DELETE FROM space_members WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES('draft-global','Draft global')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('draft-gm','draft-global','editor-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO global_roles(id,name,permissions,built_in) VALUES('draft-gr','Draft role','[\"edit_content\"]',0)").execute(&state.db).await.unwrap();
        sqlx::query(
            "INSERT INTO group_global_roles(group_id,role_id) VALUES('draft-global','draft-gr')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert!(get(&state, auth("editor-a")).await.is_err());
        sqlx::query(
            "UPDATE global_roles SET permissions='[\"manage_templates\"]' WHERE id='draft-gr'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert!(get(&state, auth("editor-a")).await.is_ok());
        sqlx::query("UPDATE global_roles SET permissions='[\"unknown\"]' WHERE id='draft-gr'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(get(&state, auth("editor-a")).await.is_err());
    }
    #[tokio::test]
    async fn template_draft_reads_require_exact_scoped_parent() {
        let state = catalog_state(false).await;
        seed(&state).await;
        for path in [("space-b", "template-a"), ("space-a", "foreign-template")] {
            assert!(get_template_draft(
                State(state.clone()),
                auth("owner-a"),
                Path((path.0.into(), path.1.into(), "shared-template-draft".into()))
            )
            .await
            .is_err());
        }
        sqlx::query("UPDATE template_drafts SET template_id='foreign-template' WHERE id='shared-template-draft'").execute(&state.db).await.unwrap();
        assert!(get_template_draft(
            State(state.clone()),
            auth("owner-a"),
            Path((
                "space-a".into(),
                "foreign-template".into(),
                "shared-template-draft".into()
            ))
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn template_draft_read_wal_current_credentials_and_policy_after_load() {
        for sql in ["DELETE FROM sessions WHERE user_id='editor-a'","UPDATE users SET is_active=0 WHERE id='editor-a'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','editor-a','owner-a')","UPDATE space_members SET role='viewer' WHERE user_id='editor-a'"] {
            let state=catalog_state(true).await;seed(&state).await;pause(&state,sql);
            assert!(get(&state,auth("editor-a")).await.is_err(),"{sql}");
        }
        let state = catalog_state(true).await;
        seed(&state).await;
        let read = key(&state, "draft-late-read", Some("editor-a"), "[\"read\"]").await;
        pause(
            &state,
            "UPDATE api_keys SET scopes='[\"write\"]' WHERE id='draft-late-read'",
        );
        assert!(get(&state, read).await.is_err());
        let state = catalog_state(false).await;
        seed(&state).await;
        let mut expired = auth("editor-a");
        expired.claims.exp = 1;
        assert!(get(&state, expired).await.is_err());
    }
    #[tokio::test]
    async fn template_draft_read_wal_final_raw_snapshot_and_parent_drift() {
        for sql in ["UPDATE template_drafts SET snapshot='{\"name\":\"Replacement\",\"content\":\"NEW\"}' WHERE id='shared-template-draft'","UPDATE template_drafts SET owner_id='viewer-a' WHERE id='shared-template-draft'","UPDATE template_drafts SET lock_owner_id='editor-a' WHERE id='shared-template-draft'","UPDATE templates SET content='replacement' WHERE id='template-a'","UPDATE templates SET space_id='space-b' WHERE id='template-a'"] {
            let state=catalog_state(true).await;seed(&state).await;pause_final(&state,sql);
            assert!(get(&state,auth("editor-a")).await.is_err(),"{sql}");
        }
    }
}

#[cfg(test)]
async fn template_mutation_domain_snapshot(state: &AppState) -> Vec<(String, Option<String>)> {
    let mut rows = crate::spaces::documents::tests::move_database_snapshot(state).await;
    rows.retain(|(table, _)| table != "audit_log");
    let audit:Vec<(String,Option<String>,Option<String>,String,Option<String>,Option<String>,Option<String>,Option<String>,String)>=sqlx::query_as("SELECT id,user_id,ip_address,action,target_type,target_id,before_value,after_value,created_at FROM audit_log WHERE action<>'template.update_failed' ORDER BY rowid").fetch_all(&state.db).await.unwrap();
    rows.push((
        "audit_nonfailure".into(),
        Some(serde_json::to_string(&audit).unwrap()),
    ));
    rows
}

#[cfg(test)]
mod template_mutation_tests {
    use super::super::routes::catalog_tests::{auth, body, catalog_state, key};
    use super::*;
    fn req(content: &str) -> TemplateRequest {
        TemplateRequest {
            name: "  Shared template  ".into(),
            description: None,
            content: content.into(),
            properties: Some(serde_json::json!({"Unbound":{"raw":true},"__starlane_meta":[1,2]})),
            tags: Some(vec!["Unbound name".into()]),
            source_id: Some("raw-import-metadata".into()),
            created_at: Some("raw-created".into()),
            updated_at: Some("raw-updated".into()),
        }
    }
    async fn seed(state: &AppState, lock: bool) {
        sqlx::query("INSERT INTO template_drafts(id,template_id,space_id,owner_id,snapshot,lock_owner_id,lock_expires_at) VALUES('draft','template-a','space-a','owner-a',$1,$2,$3)").bind(serde_json::to_string(&req("DRAFT BODY")).unwrap()).bind(lock.then_some("owner-a")).bind(lock.then_some("2999-01-01T00:00:00Z")).execute(&state.db).await.unwrap();
    }
    fn draft_path() -> Path<(String, String, String)> {
        Path(("space-a".into(), "template-a".into(), "draft".into()))
    }
    fn template_path() -> Path<(String, String)> {
        Path(("space-a".into(), "template-a".into()))
    }
    async fn snapshot(state: &AppState) -> Vec<(String, Option<String>)> {
        template_mutation_domain_snapshot(state).await
    }
    #[tokio::test]
    async fn template_writers_shared_save_release_owner_publish_and_raw_defaults() {
        let state = catalog_state(false).await;
        seed(&state, true).await;
        assert!(update_template_draft(
            State(state.clone()),
            auth("editor-a"),
            draft_path(),
            Json(req("BLOCKED"))
        )
        .await
        .is_err());
        release_template_draft(State(state.clone()), auth("editor-a"), draft_path())
            .await
            .unwrap();
        let response = body(
            update_template_draft(
                State(state.clone()),
                auth("editor-a"),
                draft_path(),
                Json(req("NEW SHARED BODY")),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(response["owner_id"], "owner-a");
        assert_eq!(response["snapshot"]["name"], "  Shared template  ");
        assert_eq!(response["snapshot"]["source_id"], "raw-import-metadata");
        assert!(
            publish_template_draft(State(state.clone()), auth("editor-a"), draft_path())
                .await
                .is_err()
        );
        publish_template_draft(State(state.clone()), auth("owner-a"), draft_path())
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT content FROM templates WHERE id='template-a'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            "NEW SHARED BODY"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM template_revisions WHERE template_id='template-a'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            10
        );
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT MAX(revision_number) FROM template_revisions WHERE template_id='template-a'").fetch_one(&state.db).await.unwrap(),14);
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT name FROM templates WHERE id='template-a'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            "Shared template"
        );
    }
    #[tokio::test]
    async fn template_writers_revert_keeps_history_and_delete_cascades_exact_scope() {
        let state = catalog_state(false).await;
        seed(&state, false).await;
        revert_template_revision(
            State(state.clone()),
            auth("editor-a"),
            Path(("space-a".into(), "template-a".into(), "revision-1".into())),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM template_revisions WHERE template_id='template-a'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            14
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT content FROM templates WHERE id='template-a'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            "SNAPSHOT SECRET"
        );
        delete_template(State(state.clone()), auth("editor-a"), template_path())
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM template_drafts")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM template_revisions")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
        assert!(sqlx::query_scalar::<_, String>(
            "SELECT id FROM templates WHERE id='foreign-template'"
        )
        .fetch_one(&state.db)
        .await
        .is_ok());
    }
    #[tokio::test]
    async fn template_writers_canonical_null_import_and_missing_release_contract() {
        let state = catalog_state(false).await;
        let id = Uuid::new_v4().to_string();
        let mut request = req("CREATE");
        request.source_id = Some(id.clone());
        request.created_at = Some("2026-01-01T00:00:00Z".into());
        request.updated_at = Some("2026-01-02T00:00:00Z".into());
        request.properties = None;
        request.tags = None;
        let response = create_template(
            State(state.clone()),
            auth("editor-a"),
            Path("space-a".into()),
            Json(request),
        )
        .await
        .unwrap()
        .into_response();
        assert_eq!(response.status(), StatusCode::CREATED);
        let row = sqlx::query_as::<_, (Option<String>, String, String, String)>(
            "SELECT properties,tags,created_at,updated_at FROM templates WHERE id=$1",
        )
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            row,
            (
                None,
                "[]".into(),
                "2026-01-01T00:00:00+00:00".into(),
                "2026-01-02T00:00:00+00:00".into()
            )
        );
        release_template_draft(
            State(state.clone()),
            auth("editor-a"),
            Path((
                "space-a".into(),
                "absent-template".into(),
                "absent-draft".into(),
            )),
        )
        .await
        .unwrap();
        let mut update = req("UPDATED");
        update.properties = None;
        update.tags = None;
        update_template(
            State(state.clone()),
            auth("editor-a"),
            template_path(),
            Json(update),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_as::<_, (Option<String>, String)>(
                "SELECT properties,tags FROM templates WHERE id='template-a'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            (None, "[]".into())
        );
    }
    #[tokio::test]
    async fn template_writers_malformed_source_and_foreign_consumers_roll_back() {
        for raw in ["broken", "{\"name\":1}"] {
            let state = catalog_state(false).await;
            seed(&state, false).await;
            sqlx::query("UPDATE template_drafts SET snapshot=$1 WHERE id='draft'")
                .bind(raw)
                .execute(&state.db)
                .await
                .unwrap();
            let before = snapshot(&state).await;
            assert!(
                publish_template_draft(State(state.clone()), auth("owner-a"), draft_path())
                    .await
                    .is_err()
            );
            assert_eq!(snapshot(&state).await, before);
        }
        for field in ["properties", "tags"] {
            let state = catalog_state(false).await;
            sqlx::query(&format!(
                "UPDATE templates SET {field}='broken' WHERE id='template-a'"
            ))
            .execute(&state.db)
            .await
            .unwrap();
            let before = snapshot(&state).await;
            assert!(
                create_template_draft(State(state.clone()), auth("editor-a"), template_path())
                    .await
                    .is_err()
            );
            assert_eq!(snapshot(&state).await, before);
        }
        for kind in ["draft", "revision"] {
            let state = catalog_state(false).await;
            seed(&state, false).await;
            let sql = if kind == "draft" {
                "UPDATE template_drafts SET space_id='space-b' WHERE id='draft'"
            } else {
                "UPDATE template_revisions SET space_id='space-b' WHERE id='revision-1'"
            };
            sqlx::query(sql).execute(&state.db).await.unwrap();
            let before = snapshot(&state).await;
            assert!(
                delete_template(State(state.clone()), auth("editor-a"), template_path())
                    .await
                    .is_err()
            );
            assert_eq!(snapshot(&state).await, before);
        }
    }
    #[tokio::test]
    async fn template_writers_final_sql_policy_and_source_drift_roll_back() {
        for trigger in [
            "UPDATE users SET is_active=0 WHERE id='editor-a'",
            "UPDATE templates SET content='REDIRECTED' WHERE id=NEW.id",
            "UPDATE templates SET space_id='space-b' WHERE id=NEW.id",
        ] {
            let state = catalog_state(false).await;
            sqlx::query(&format!("CREATE TRIGGER template_late AFTER UPDATE OF content ON templates WHEN NEW.content='ATTEMPT' BEGIN {trigger}; END")).execute(&state.db).await.unwrap();
            let before = snapshot(&state).await;
            assert!(update_template(
                State(state.clone()),
                auth("editor-a"),
                template_path(),
                Json(req("ATTEMPT"))
            )
            .await
            .is_err());
            assert_eq!(snapshot(&state).await, before);
        }
        for trigger in [
            "UPDATE users SET is_active=0 WHERE id='owner-a'",
            "UPDATE template_revisions SET snapshot='replacement' WHERE id=NEW.id",
            "UPDATE template_drafts SET owner_id='editor-a' WHERE id='draft'",
            "UPDATE template_drafts SET snapshot='{\"name\":\"Newer\",\"content\":\"NEWER\"}' WHERE id='draft'",
        ] {
            let state = catalog_state(false).await;
            seed(&state, false).await;
            sqlx::query(&format!("CREATE TRIGGER revision_late AFTER INSERT ON template_revisions BEGIN {trigger}; END")).execute(&state.db).await.unwrap();
            let before = snapshot(&state).await;
            assert!(
                publish_template_draft(State(state.clone()), auth("owner-a"), draft_path())
                    .await
                    .is_err()
            );
            assert_eq!(snapshot(&state).await, before);
        }
        let state = catalog_state(false).await;
        seed(&state, false).await;
        sqlx::query("CREATE TRIGGER draft_late AFTER UPDATE OF snapshot ON template_drafts BEGIN UPDATE template_drafts SET owner_id='editor-a' WHERE id=NEW.id; END").execute(&state.db).await.unwrap();
        let before = snapshot(&state).await;
        assert!(update_template_draft(
            State(state.clone()),
            auth("editor-a"),
            draft_path(),
            Json(req("ATTEMPT"))
        )
        .await
        .is_err());
        assert_eq!(snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn template_writers_actual_keys_and_global_permission_granularity() {
        let state = catalog_state(false).await;
        let read = key(&state, "writer-read", Some("editor-a"), "[\"read\"]").await;
        let before = snapshot(&state).await;
        assert!(update_template(
            State(state.clone()),
            read,
            template_path(),
            Json(req("DENIED"))
        )
        .await
        .is_err());
        assert_eq!(snapshot(&state).await, before);
        let write = key(&state, "writer-write", Some("editor-a"), "[\"write\"]").await;
        update_template(
            State(state.clone()),
            write,
            template_path(),
            Json(req("WRITE")),
        )
        .await
        .unwrap();
        let service = key(&state, "writer-service", None, "[\"admin\"]").await;
        assert!(
            delete_template(State(state.clone()), service, template_path())
                .await
                .is_err()
        );
        sqlx::query("DELETE FROM space_members WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES('writer-group','Writer')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('writer-member','writer-group','editor-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO global_roles(id,name,permissions) VALUES('writer-role','Writer role','[\"edit_content\"]')").execute(&state.db).await.unwrap();
        sqlx::query(
            "INSERT INTO group_global_roles(group_id,role_id) VALUES('writer-group','writer-role')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert!(update_template(
            State(state.clone()),
            auth("editor-a"),
            template_path(),
            Json(req("DENIED"))
        )
        .await
        .is_err());
        sqlx::query(
            "UPDATE global_roles SET permissions='[\"manage_templates\"]' WHERE id='writer-role'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        update_template(
            State(state.clone()),
            auth("editor-a"),
            template_path(),
            Json(req("GRANULAR")),
        )
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn template_writers_wal_wait_rechecks_current_session_and_publishes_newer_saved_body() {
        let state = catalog_state(true).await;
        seed(&state, false).await;
        let mut blocker = state.db.begin().await.unwrap();
        sqlx::query("UPDATE spaces SET id=id WHERE id='space-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let next = serde_json::to_string(&req("NEWER SAVE BEFORE PUBLISH LOCK")).unwrap();
        sqlx::query("UPDATE template_drafts SET snapshot=$1 WHERE id='draft'")
            .bind(next)
            .execute(&mut *blocker)
            .await
            .unwrap();
        let copied = state.clone();
        let task = tokio::spawn(async move {
            publish_template_draft(State(copied), auth("owner-a"), draft_path())
                .await
                .map(|_| ())
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!task.is_finished());
        blocker.commit().await.unwrap();
        task.await.unwrap().unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT content FROM templates WHERE id='template-a'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            "NEWER SAVE BEFORE PUBLISH LOCK"
        );
        let state = catalog_state(true).await;
        let mut blocker = state.db.begin().await.unwrap();
        sqlx::query("UPDATE spaces SET id=id WHERE id='space-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE user_id='editor-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let copied = state.clone();
        let task = tokio::spawn(async move {
            update_template(
                State(copied),
                auth("editor-a"),
                template_path(),
                Json(req("DENIED")),
            )
            .await
            .map(|_| ())
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!task.is_finished());
        blocker.commit().await.unwrap();
        assert!(task.await.unwrap().is_err());
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT content FROM templates WHERE id='template-a'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            "TEMPLATE SECRET"
        );
    }
    #[tokio::test]
    async fn template_writers_wal_expired_lock_acquisition_serializes_and_preserves_owner() {
        let state = catalog_state(true).await;
        seed(&state, false).await;
        let left = state.clone();
        let right = state.clone();
        let first = tokio::spawn(async move {
            create_template_draft(State(left), auth("owner-a"), template_path())
                .await
                .map(|_| ())
        });
        let second = tokio::spawn(async move {
            create_template_draft(State(right), auth("editor-a"), template_path())
                .await
                .map(|_| ())
        });
        let (a, b) = tokio::join!(first, second);
        assert_ne!(a.unwrap().is_ok(), b.unwrap().is_ok());
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT owner_id FROM template_drafts WHERE id='draft'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            "owner-a"
        );
    }
    #[tokio::test]
    async fn template_writers_wal_parallel_reverts_allocate_distinct_revisions() {
        let state = catalog_state(true).await;
        let left = state.clone();
        let right = state.clone();
        let a = tokio::spawn(async move {
            revert_template_revision(
                State(left),
                auth("owner-a"),
                Path(("space-a".into(), "template-a".into(), "revision-1".into())),
            )
            .await
            .map(|_| ())
        });
        let b = tokio::spawn(async move {
            revert_template_revision(
                State(right),
                auth("editor-a"),
                Path(("space-a".into(), "template-a".into(), "revision-2".into())),
            )
            .await
            .map(|_| ())
        });
        a.await.unwrap().unwrap();
        b.await.unwrap().unwrap();
        let rows:Vec<i64>=sqlx::query_scalar("SELECT revision_number FROM template_revisions WHERE revision_number>13 ORDER BY revision_number").fetch_all(&state.db).await.unwrap();
        assert_eq!(rows, vec![14, 15]);
    }
}

#[cfg(test)]
mod template_mutation_http_tests {
    use super::super::routes::catalog_tests::{catalog_state, key};
    use super::*;
    #[tokio::test]
    async fn template_mutation_http_current_write_read_and_service_ceiling() {
        use tower::ServiceExt;
        let state = catalog_state(false).await;
        for (id, owner, scope, expected) in [
            (
                "http-write",
                Some("editor-a"),
                "write",
                StatusCode::NO_CONTENT,
            ),
            ("http-read", Some("editor-a"), "read", StatusCode::FORBIDDEN),
            ("http-service", None, "admin", StatusCode::UNAUTHORIZED),
        ] {
            let _ = key(&state, id, owner, &format!("[\"{scope}\"]")).await;
            sqlx::query("UPDATE api_keys SET key_hash=$1,is_service_account=$2 WHERE id=$3")
                .bind(crate::auth::api_keys::hash_api_key(id))
                .bind(i64::from(owner.is_none()))
                .bind(id)
                .execute(&state.db)
                .await
                .unwrap();
            let request = axum::http::Request::builder()
                .method("PUT")
                .uri("/spaces/space-a/templates/template-a")
                .header("X-API-Key", id)
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(
                    r#"{"name":"HTTP","content":"NEW","properties":null,"tags":null}"#,
                ))
                .unwrap();
            let response = router()
                .with_state(state.clone())
                .oneshot(request)
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{id}");
        }
    }
}

#[cfg(test)]
mod template_mutation_boundary_tests {
    use super::super::routes::catalog_tests::{auth, catalog_state, key};
    use super::*;
    async fn domain_snapshot(state: &AppState) -> Vec<(String, Option<String>)> {
        template_mutation_domain_snapshot(state).await
    }
    async fn seed(state: &AppState) {
        let snapshot = serde_json::json!({"name":"Draft","content":"DRAFT"}).to_string();
        sqlx::query("INSERT INTO template_drafts(id,template_id,space_id,owner_id,snapshot,lock_owner_id,lock_expires_at) VALUES('boundary-draft','template-a','space-a','editor-a',$1,NULL,NULL)").bind(snapshot).execute(&state.db).await.unwrap();
    }
    fn req() -> TemplateRequest {
        TemplateRequest {
            name: "Boundary".into(),
            description: None,
            content: "ATTEMPT".into(),
            properties: None,
            tags: None,
            source_id: Some(Uuid::new_v4().to_string()),
            created_at: None,
            updated_at: None,
        }
    }
    #[tokio::test]
    async fn template_every_writer_late_credential_revocation_rolls_back_all_sql() {
        for kind in [
            "create", "update", "acquire", "save", "publish", "release", "revert", "delete",
        ] {
            let state = catalog_state(false).await;
            seed(&state).await;
            let event = match kind {
                "create" => "AFTER INSERT ON templates",
                "update" => "AFTER UPDATE OF content ON templates",
                "acquire" | "release" => "AFTER UPDATE OF lock_owner_id ON template_drafts",
                "save" => "AFTER UPDATE OF snapshot ON template_drafts",
                "publish" | "revert" => "AFTER INSERT ON template_revisions",
                _ => "AFTER DELETE ON templates",
            };
            sqlx::query(&format!("CREATE TRIGGER credential_late {event} BEGIN DELETE FROM sessions WHERE user_id='editor-a'; END")).execute(&state.db).await.unwrap();
            let before = domain_snapshot(&state).await;
            let actor = auth("editor-a");
            let path = Path(("space-a".into(), "template-a".into()));
            let draft = Path((
                "space-a".into(),
                "template-a".into(),
                "boundary-draft".into(),
            ));
            let denied = match kind {
                "create" => create_template(
                    State(state.clone()),
                    actor,
                    Path("space-a".into()),
                    Json(req()),
                )
                .await
                .is_err(),
                "update" => update_template(State(state.clone()), actor, path, Json(req()))
                    .await
                    .is_err(),
                "acquire" => create_template_draft(State(state.clone()), actor, path)
                    .await
                    .is_err(),
                "save" => update_template_draft(State(state.clone()), actor, draft, Json(req()))
                    .await
                    .is_err(),
                "publish" => publish_template_draft(State(state.clone()), actor, draft)
                    .await
                    .is_err(),
                "release" => release_template_draft(State(state.clone()), actor, draft)
                    .await
                    .is_err(),
                "revert" => revert_template_revision(
                    State(state.clone()),
                    actor,
                    Path(("space-a".into(), "template-a".into(), "revision-1".into())),
                )
                .await
                .is_err(),
                _ => delete_template(State(state.clone()), actor, path)
                    .await
                    .is_err(),
            };
            assert!(denied, "{kind}");
            assert_eq!(domain_snapshot(&state).await, before, "{kind}");
            let events: Vec<(String, String, String, String)> = sqlx::query_as(
                "SELECT action,user_id,target_type,target_id FROM audit_log ORDER BY rowid",
            )
            .fetch_all(&state.db)
            .await
            .unwrap();
            assert_eq!(
                events,
                if kind == "update" {
                    vec![(
                        "template.update_failed".into(),
                        "editor-a".into(),
                        "template".into(),
                        "template-a".into(),
                    )]
                } else {
                    Vec::new()
                },
                "{kind}"
            );
        }
    }
    #[tokio::test]
    async fn template_write_current_jwt_key_and_legacy_custom_role_checks() {
        let state = catalog_state(false).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('legacy-writer','space-a','Legacy writer','[\"edit_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='legacy-writer' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        update_template(
            State(state.clone()),
            auth("editor-a"),
            Path(("space-a".into(), "template-a".into())),
            Json(req()),
        )
        .await
        .unwrap();
        let mut expired = auth("editor-a");
        expired.claims.exp = 1;
        assert!(update_template(
            State(state.clone()),
            expired,
            Path(("space-a".into(), "template-a".into())),
            Json(req())
        )
        .await
        .is_err());
        let current = key(&state, "boundary-key", Some("editor-a"), "[\"write\"]").await;
        sqlx::query("CREATE TRIGGER late_key AFTER UPDATE OF content ON templates BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='boundary-key'; END").execute(&state.db).await.unwrap();
        let before = domain_snapshot(&state).await;
        assert!(update_template(
            State(state.clone()),
            current,
            Path(("space-a".into(), "template-a".into())),
            Json(req())
        )
        .await
        .is_err());
        assert_eq!(domain_snapshot(&state).await, before);
    }
}
