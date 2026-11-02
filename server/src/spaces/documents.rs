use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{Any, AnyPool, Row, Transaction};
use std::collections::{BTreeSet, HashMap};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::search::index::DocEntry;
use crate::storage;
use crate::AppState;

const DEFAULT_NEW_DOCUMENT_CONTENT: &str = "";

pub(crate) fn document_storage_frontmatter(
    title: &str,
    icon: Option<&str>,
    locked: bool,
    public: bool,
) -> String {
    format!(
        "title: {}\nicon: {}\nlocked: {locked}\npublic: {public}",
        serde_json::to_string(title).unwrap_or_else(|_| "\"Untitled\"".to_string()),
        serde_json::to_string(icon.unwrap_or("")).unwrap_or_else(|_| "\"\"".to_string()),
    )
}

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
            "/spaces/{space_id}/documents/{doc_id}/duplicate",
            post(duplicate_document),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/trash",
            post(restore_document).delete(permanently_delete_document_with_headers),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/permanent-deletions/{operation_id}/cleanup",
            post(cleanup_permanent_document_attachments),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/archive",
            post(archive_document).delete(unarchive_document),
        )
        .route(
            "/spaces/{space_id}/documents/trash",
            get(list_trashed_documents),
        )
        .route(
            "/spaces/{space_id}/documents/archive",
            get(list_archived_documents),
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
    pub human_identifier: Option<String>,
    pub maintenance_user_id: Option<String>,
    pub maintenance_group_id: Option<String>,
    pub title: String,
    pub folder_id: Option<String>,
    pub parent_id: Option<String>,
    pub icon: Option<String>,
    pub cover_image: Option<String>,
    pub content: Option<String>,
    pub template_id: Option<String>,
    #[serde(default)]
    pub property_values: Vec<CreatePropertyValue>,
    pub source_id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Deserialize)]
pub struct CreatePropertyValue {
    pub property_id: String,
    pub value: String,
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
pub struct DuplicateDocumentRequest {
    pub title: Option<String>,
    pub draft_id: Option<String>,
}

#[derive(Deserialize)]
pub struct MoveDocumentRequest {
    pub target_space_id: String,
    pub folder_id: Option<String>,
    pub parent_id: Option<String>,
}

pub(crate) struct SpaceStorageRelocation {
    document_paths: HashMap<String, String>,
    attachment_paths: HashMap<String, String>,
    pub(crate) staged_files: Vec<storage::StagedFileCopy>,
}

#[derive(Serialize)]
pub struct DocumentResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_access_source: Option<String>,
    pub human_identifier: Option<String>,
    pub maintenance_user_id: Option<String>,
    pub maintenance_group_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identifier_edit_allowed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maintenance_edit_allowed: Option<bool>,
    /// Current ordinary Edit authority, never a lease or permission inferred from lock ownership.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_edit_allowed: Option<bool>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    pub updated_at: String,
    pub published_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<String>,
    pub workflow_template_id: Option<String>,
    pub workflow_template_name: Option<String>,
    pub workflow_stage_id: Option<String>,
    pub workflow_stage_name: Option<String>,
    pub workflow_status: Option<String>,
}

#[derive(Serialize)]
pub struct PublishedDocumentMetadata {
    pub title: String,
    pub icon: Option<String>,
    pub cover_image: Option<String>,
}

#[derive(Serialize)]
pub struct DocumentContentResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_metadata: Option<PublishedDocumentMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_source_digest: Option<String>,
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

#[derive(Clone, PartialEq, Eq, sqlx::FromRow)]
struct StarredSource {
    id: String,
    space_id: String,
    space_name: String,
    title: String,
    icon: Option<String>,
    updated_at: String,
    starred_at: String,
    published_at: Option<String>,
}
async fn starred_source(
    conn: &mut sqlx::AnyConnection,
    actor: &str,
    space: &str,
    id: &str,
) -> Result<Option<StarredSource>, AppError> {
    sqlx::query_as("SELECT d.id,d.space_id,s.name AS space_name,d.title,d.icon,d.updated_at,sd.starred_at,d.published_at FROM starred_documents sd JOIN documents d ON d.id=sd.document_id JOIN spaces s ON s.id=d.space_id WHERE sd.user_id=$1 AND d.space_id=$2 AND d.id=$3 AND d.archived_at IS NULL AND d.deleted_at IS NULL")
        .bind(actor).bind(space).bind(id).fetch_optional(conn).await.map_err(AppError::internal)
}
async fn require_star_operation(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    operation: super::access::PageOperation,
) -> Result<(), AppError> {
    super::access::resolve_page_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        id,
        operation,
        super::access::Purpose::Routine,
    )
    .await?;
    Ok(())
}
#[cfg(test)]
thread_local! {
    static STARS_BEFORE_FINAL:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=std::cell::RefCell::new(None);
}
#[cfg(test)]
async fn pause_stars_final() {
    let hook = STARS_BEFORE_FINAL.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook.await;
    }
}
/// GET /api/v1/spaces/{space_id}/documents/starred
async fn list_starred_documents(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let actor = require_document_discovery_space_view(&mut conn, &auth, &space_id)
        .await?
        .actor_id;
    let ids:Vec<String>=sqlx::query_scalar("SELECT d.id FROM starred_documents sd JOIN documents d ON d.id=sd.document_id WHERE sd.user_id=$1 AND d.space_id=$2 AND d.archived_at IS NULL AND d.deleted_at IS NULL ORDER BY sd.starred_at DESC")
        .bind(&actor).bind(&space_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let mut candidates = Vec::new();
    for id in ids {
        match require_star_operation(
            &mut conn,
            &auth,
            &space_id,
            &id,
            super::access::PageOperation::View,
        )
        .await
        {
            Ok(()) => {}
            Err(AppError::NotFound(_)) => continue,
            Err(e) => return Err(e),
        }
        let Some(source) = starred_source(&mut conn, &actor, &space_id, &id).await? else {
            continue;
        };
        let draft = if source.published_at.is_none() {
            match readable_discovery_draft(&mut conn, &auth, &space_id, &id).await {
                Ok(d) => Some(d),
                Err(AppError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            }
        } else {
            None
        };
        candidates.push((source, draft));
    }
    #[cfg(test)]
    pause_stars_final().await;
    // Empty lists also need fresh credentials. Per-page/source proofs follow
    // this global wait; no unrelated credential wait follows the returned rows.
    require_document_discovery_space_view(&mut conn, &auth, &space_id).await?;
    let mut result = Vec::new();
    for (source, draft) in candidates {
        match require_star_operation(
            &mut conn,
            &auth,
            &space_id,
            &source.id,
            super::access::PageOperation::View,
        )
        .await
        {
            Ok(()) => {}
            Err(AppError::NotFound(_)) => continue,
            Err(e) => return Err(e),
        }
        let (title, icon) = if let Some(draft) = draft {
            match require_discovery_draft_view(&mut conn, &auth, &space_id, &source.id, &draft.id)
                .await
            {
                Ok(()) => {}
                Err(AppError::NotFound(_)) => continue,
                Err(e) => return Err(e),
            }
            let current:Option<(String,Option<String>,String)>=sqlx::query_as("SELECT title,icon,content FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3")
                .bind(&draft.id).bind(&source.id).bind(&space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
            if current != Some((draft.title.clone(), draft.icon.clone(), draft.content)) {
                continue;
            }
            (draft.title, draft.icon)
        } else {
            (source.title.clone(), source.icon.clone())
        };
        if starred_source(&mut conn, &actor, &space_id, &source.id)
            .await?
            .as_ref()
            != Some(&source)
        {
            continue;
        }
        result.push(StarredDocumentResponse {
            id: source.id,
            space_id: source.space_id,
            space_name: source.space_name,
            title,
            icon,
            updated_at: source.updated_at,
            starred_at: source.starred_at,
        });
    }
    Ok(Json(result))
}
/// POST /api/v1/spaces/{space_id}/documents/{doc_id}/star
async fn star_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    let row=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&doc_id).bind(&space_id).execute(&mut *tx).await.map_err(AppError::internal)?;
    if row.rows_affected() != 1 {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    require_star_operation(
        &mut tx,
        &auth,
        &space_id,
        &doc_id,
        super::access::PageOperation::UserPagePreference,
    )
    .await?;
    sqlx::query("INSERT INTO starred_documents(user_id,document_id,starred_at) VALUES($1,$2,$3) ON CONFLICT(user_id,document_id) DO UPDATE SET starred_at=excluded.starred_at")
        .bind(&auth.claims.sub).bind(&doc_id).bind(chrono::Utc::now().to_rfc3339()).execute(&mut *tx).await.map_err(AppError::internal)?;
    require_star_operation(
        &mut tx,
        &auth,
        &space_id,
        &doc_id,
        super::access::PageOperation::UserPagePreference,
    )
    .await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}
/// DELETE /api/v1/spaces/{space_id}/documents/{doc_id}/star
async fn unstar_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    // Missing/inaccessible targets share 204. These locks establish only scope;
    // erasing one's own preference does not require page visibility or revive it.
    sqlx::query("UPDATE spaces SET id=id WHERE id=$1")
        .bind(&space_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    let actor = super::access::require_current_human_preference_write(&mut tx, &auth).await?;
    sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2")
        .bind(&doc_id)
        .bind(&space_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    sqlx::query("DELETE FROM starred_documents WHERE user_id=$1 AND document_id IN(SELECT id FROM documents WHERE id=$2 AND space_id=$3)")
        .bind(&actor).bind(&doc_id).bind(&space_id).execute(&mut *tx).await.map_err(AppError::internal)?;
    super::access::require_current_human_preference_write(&mut tx, &auth).await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn require_document_metadata_view(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space_id: &str,
    document_id: &str,
) -> Result<super::access::PageAccess, AppError> {
    super::access::resolve_page_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space_id,
        document_id,
        super::access::PageOperation::InspectMetadata,
        super::access::Purpose::Routine,
    )
    .await
}
pub(crate) struct ReadableDiscoveryDraft {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) icon: Option<String>,
    pub(crate) cover_image: Option<String>,
    pub(crate) content: String,
}
pub(crate) async fn require_discovery_draft_view(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space_id: &str,
    document_id: &str,
    draft_id: &str,
) -> Result<(), AppError> {
    super::access::resolve_draft_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space_id,
        document_id,
        draft_id,
        super::access::DraftOperation::View,
        super::access::Purpose::Routine,
    )
    .await?;
    Ok(())
}
pub(crate) async fn readable_discovery_draft(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space_id: &str,
    document_id: &str,
) -> Result<ReadableDiscoveryDraft, AppError> {
    // Attribution on the page is never draft participation. Select only a real
    // owned or explicitly shared draft, then validate that exact identity.
    let actor = require_document_discovery_space_view(conn, auth, space_id)
        .await?
        .actor_id;
    let id: Option<String> = sqlx::query_scalar("SELECT dd.id FROM document_drafts dd WHERE dd.document_id=$1 AND dd.space_id=$2 AND (dd.owner_id=$3 OR EXISTS(SELECT 1 FROM document_draft_access dda WHERE dda.draft_id=dd.id AND dda.user_id=$3 AND dda.role IN ('viewer','editor'))) ORDER BY CASE WHEN dd.owner_id=$3 THEN 0 ELSE 1 END,dd.updated_at DESC,dd.id ASC LIMIT 1").bind(document_id).bind(space_id).bind(&actor).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let id = id.ok_or_else(|| AppError::NotFound("Target unavailable".into()))?;
    require_discovery_draft_view(conn, auth, space_id, document_id, &id).await?;
    let row: Option<(String,Option<String>,Option<String>,String)> = sqlx::query_as("SELECT title,icon,cover_image,content FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3").bind(&id).bind(document_id).bind(space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let (title, icon, cover_image, content) =
        row.ok_or_else(|| AppError::NotFound("Target unavailable".into()))?;
    Ok(ReadableDiscoveryDraft {
        id,
        title,
        icon,
        cover_image,
        content,
    })
}
pub(crate) fn discovery_draft_content(draft: &ReadableDiscoveryDraft) -> Result<String, AppError> {
    let normalized = draft.content.replace("\r\n", "\n");
    let mut portable = serde_yaml::Mapping::new();
    let mut body = normalized.as_str();
    if let Some(rest) = normalized.strip_prefix("---\n") {
        if let Some((header, remainder)) = rest.split_once("\n---\n") {
            if header.trim().is_empty() {
                return Ok(normalized);
            }
            let Ok(mapping) = serde_yaml::from_str::<serde_yaml::Mapping>(header) else {
                // Existing authorized drafts must remain readable and repairable.
                // Raw portable fields never replace the typed access proof or
                // authoritative JSON presentation/identity fields.
                return Ok(normalized);
            };
            portable = mapping;
            body = remainder.trim_start_matches('\n');
        }
    }
    // The exact readable draft supplies presentation. Portable content may retain
    // custom fields but cannot override canonical page identity or lifecycle.
    let reserved = [
        "id",
        "space_id",
        "document_id",
        "draft_id",
        "owner_id",
        "title",
        "icon",
        "cover_image",
        "locked",
        "locked_by",
        "public",
        "public_slug",
        "published_at",
        "archived_at",
        "deleted_at",
        "deleted_by",
        "created_by",
        "last_updated_by",
        "created_at",
        "updated_at",
        "content_path",
        "folder_id",
        "parent_id",
        "previous_folder_id",
        "previous_parent_id",
        "sort_order",
        "starlane_lifecycle",
    ];
    portable.retain(|key, _| {
        !key.as_str()
            .is_some_and(|key| reserved.contains(&key.to_ascii_lowercase().as_str()))
    });
    portable.insert(
        serde_yaml::Value::String("title".into()),
        serde_yaml::Value::String(draft.title.clone()),
    );
    portable.insert(
        serde_yaml::Value::String("icon".into()),
        serde_yaml::Value::String(draft.icon.clone().unwrap_or_default()),
    );
    portable.insert(
        serde_yaml::Value::String("cover_image".into()),
        serde_yaml::Value::String(draft.cover_image.clone().unwrap_or_default()),
    );
    portable.insert(
        serde_yaml::Value::String("locked".into()),
        serde_yaml::Value::Bool(false),
    );
    portable.insert(
        serde_yaml::Value::String("public".into()),
        serde_yaml::Value::Bool(false),
    );
    let header = serde_yaml::to_string(&portable).map_err(AppError::internal)?;
    Ok(format!("---\n{}---\n\n{}", header, body))
}
pub(crate) async fn require_current_document_read_source(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    document: &str,
    published: &str,
    path: &str,
    updated: &str,
) -> Result<(), AppError> {
    let current:Option<(String,String,String)>=sqlx::query_as("SELECT published_at,content_path,updated_at FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL").bind(document).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if !current.is_some_and(|(value, current_path, current_updated)| {
        value == published && current_path == path && current_updated == updated
    }) {
        return Err(AppError::Conflict(
            "Page changed while being read; retry".into(),
        ));
    }
    Ok(())
}

async fn require_document_discovery_space_view(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space_id: &str,
) -> Result<super::access::SpaceAccess, AppError> {
    let access = super::access::resolve_space_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space_id,
        super::access::Purpose::Routine,
    )
    .await?;
    if !access.allows(super::access::Permission::View) {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    Ok(access)
}
async fn require_document_discovery_page_view(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space_id: &str,
    document_id: &str,
) -> Result<(), AppError> {
    super::access::resolve_page_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space_id,
        document_id,
        super::access::PageOperation::View,
        super::access::Purpose::Routine,
    )
    .await?;
    Ok(())
}

/// GET /api/v1/spaces/{space_id}/documents
async fn list_documents(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Query(query): Query<ListDocumentsQuery>,
) -> Result<impl IntoResponse, AppError> {
    // Listing is read-only. Retention expiry is not authority to delete pages
    // during a GET; automated purge awaits a guarded lifecycle worker.
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let access = require_document_discovery_space_view(&mut conn, &auth, &space_id).await?;

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
             public, public_slug, created_by, last_updated_by, created_at, updated_at, published_at, human_identifier, maintenance_user_id, maintenance_group_id, \
             draft_id, workflow_template_id, workflow_template_name, workflow_stage_id, workflow_status, workflow_stages \
             FROM ( \
                 SELECT d.id, d.space_id, d.folder_id, d.parent_id, d.title, d.icon, d.cover_image, d.locked, d.locked_by, \
                        d.public, d.public_slug, d.created_by, d.last_updated_by, d.created_at, d.updated_at, d.published_at, d.human_identifier, d.maintenance_user_id, d.maintenance_group_id, \
                        CASE WHEN d.published_at IS NULL THEN ( \
                            SELECT dd.id FROM document_drafts dd \
                            LEFT JOIN document_draft_access dda ON dda.draft_id = dd.id AND dda.user_id = $5 \
                            WHERE dd.document_id = d.id AND dd.space_id = d.space_id \
                              AND (dd.owner_id = $5 OR (dda.user_id = $5 AND dda.role IN ('viewer','editor'))) \
                            ORDER BY CASE WHEN dd.owner_id = $5 THEN 0 ELSE 1 END, dd.updated_at DESC, dd.id ASC \
                            LIMIT 1 \
                        ) ELSE NULL END AS draft_id, \
                        dw.workflow_template_id, wt.name AS workflow_template_name, \
                        dw.current_stage_id AS workflow_stage_id, dw.status AS workflow_status, wt.stages AS workflow_stages, \
                        d.updated_at AS sort_updated_at \
                 FROM documents d \
                 LEFT JOIN document_workflows dw ON dw.document_id = d.id \
                 LEFT JOIN workflow_templates wt ON wt.id = dw.workflow_template_id AND wt.space_id = d.space_id \
                 WHERE d.space_id = $1 AND d.archived_at IS NULL AND d.deleted_at IS NULL \
                   AND (d.published_at IS NOT NULL OR EXISTS ( \
                       SELECT 1 FROM document_drafts dd \
                       LEFT JOIN document_draft_access dda ON dda.draft_id = dd.id AND dda.user_id = $5 \
                       WHERE dd.document_id = d.id AND dd.space_id = d.space_id \
                         AND (dd.owner_id = $5 OR (dda.user_id = $5 AND dda.role IN ('viewer','editor'))) \
                   )) \
                   AND ($2 IS NULL OR dw.status = $2) \
                   AND ($3 IS NULL OR dw.current_stage_id = $3) \
                   AND ($4 IS NULL OR dw.workflow_template_id = $4) \
             ) \
             ORDER BY sort_updated_at DESC, title",
        )
        .bind(&space_id)
        .bind(query.workflow_status.as_deref())
        .bind(query.workflow_stage_id.as_deref())
        .bind(query.workflow_template_id.as_deref())
        .bind(&access.actor_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::internal)?
    } else if let Some(ref folder_id) = query.folder_id {
        sqlx::query(
            "SELECT id, space_id, folder_id, parent_id, title, icon, cover_image, locked, locked_by, \
             public, public_slug, created_by, last_updated_by, created_at, updated_at, published_at, human_identifier, maintenance_user_id, maintenance_group_id, \
             draft_id, workflow_template_id, workflow_template_name, workflow_stage_id, workflow_status, workflow_stages \
             FROM ( \
                 SELECT d.id, d.space_id, d.folder_id, d.parent_id, d.title, d.icon, d.cover_image, d.locked, d.locked_by, \
                        d.public, d.public_slug, d.created_by, d.last_updated_by, d.created_at, d.updated_at, d.published_at, d.human_identifier, d.maintenance_user_id, d.maintenance_group_id, \
                        CASE WHEN d.published_at IS NULL THEN ( \
                            SELECT dd.id FROM document_drafts dd \
                            LEFT JOIN document_draft_access dda ON dda.draft_id = dd.id AND dda.user_id = $6 \
                            WHERE dd.document_id = d.id AND dd.space_id = d.space_id \
                              AND (dd.owner_id = $6 OR (dda.user_id = $6 AND dda.role IN ('viewer','editor'))) \
                            ORDER BY CASE WHEN dd.owner_id = $6 THEN 0 ELSE 1 END, dd.updated_at DESC, dd.id ASC \
                            LIMIT 1 \
                        ) ELSE NULL END AS draft_id, \
                        dw.workflow_template_id, wt.name AS workflow_template_name, \
                        dw.current_stage_id AS workflow_stage_id, dw.status AS workflow_status, wt.stages AS workflow_stages, \
                        d.sort_order \
                 FROM documents d \
                 LEFT JOIN document_workflows dw ON dw.document_id = d.id \
                 LEFT JOIN workflow_templates wt ON wt.id = dw.workflow_template_id AND wt.space_id = d.space_id \
                 WHERE d.space_id = $1 AND d.folder_id = $2 AND d.archived_at IS NULL AND d.deleted_at IS NULL \
                   AND (d.published_at IS NOT NULL OR EXISTS ( \
                       SELECT 1 FROM document_drafts dd \
                       LEFT JOIN document_draft_access dda ON dda.draft_id = dd.id AND dda.user_id = $6 \
                       WHERE dd.document_id = d.id AND dd.space_id = d.space_id \
                         AND (dd.owner_id = $6 OR (dda.user_id = $6 AND dda.role IN ('viewer','editor'))) \
                   )) \
                   AND ($3 IS NULL OR dw.status = $3) \
                   AND ($4 IS NULL OR dw.current_stage_id = $4) \
                   AND ($5 IS NULL OR dw.workflow_template_id = $5) \
             ) \
             ORDER BY sort_order, title",
        )
        .bind(&space_id)
        .bind(folder_id)
        .bind(query.workflow_status.as_deref())
        .bind(query.workflow_stage_id.as_deref())
        .bind(query.workflow_template_id.as_deref())
        .bind(&access.actor_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::internal)?
    } else {
        sqlx::query(
            "SELECT id, space_id, folder_id, parent_id, title, icon, cover_image, locked, locked_by, \
             public, public_slug, created_by, last_updated_by, created_at, updated_at, published_at, human_identifier, maintenance_user_id, maintenance_group_id, \
             draft_id, workflow_template_id, workflow_template_name, workflow_stage_id, workflow_status, workflow_stages \
             FROM ( \
                 SELECT d.id, d.space_id, d.folder_id, d.parent_id, d.title, d.icon, d.cover_image, d.locked, d.locked_by, \
                        d.public, d.public_slug, d.created_by, d.last_updated_by, d.created_at, d.updated_at, d.published_at, d.human_identifier, d.maintenance_user_id, d.maintenance_group_id, \
                        CASE WHEN d.published_at IS NULL THEN ( \
                            SELECT dd.id FROM document_drafts dd \
                            LEFT JOIN document_draft_access dda ON dda.draft_id = dd.id AND dda.user_id = $5 \
                            WHERE dd.document_id = d.id AND dd.space_id = d.space_id \
                              AND (dd.owner_id = $5 OR (dda.user_id = $5 AND dda.role IN ('viewer','editor'))) \
                            ORDER BY CASE WHEN dd.owner_id = $5 THEN 0 ELSE 1 END, dd.updated_at DESC, dd.id ASC \
                            LIMIT 1 \
                        ) ELSE NULL END AS draft_id, \
                        dw.workflow_template_id, wt.name AS workflow_template_name, \
                        dw.current_stage_id AS workflow_stage_id, dw.status AS workflow_status, wt.stages AS workflow_stages, \
                        d.sort_order \
                 FROM documents d \
                 LEFT JOIN document_workflows dw ON dw.document_id = d.id \
                 LEFT JOIN workflow_templates wt ON wt.id = dw.workflow_template_id AND wt.space_id = d.space_id \
                 WHERE d.space_id = $1 AND d.folder_id IS NULL AND d.archived_at IS NULL AND d.deleted_at IS NULL \
                   AND (d.published_at IS NOT NULL OR EXISTS ( \
                       SELECT 1 FROM document_drafts dd \
                       LEFT JOIN document_draft_access dda ON dda.draft_id = dd.id AND dda.user_id = $5 \
                       WHERE dd.document_id = d.id AND dd.space_id = d.space_id \
                         AND (dd.owner_id = $5 OR (dda.user_id = $5 AND dda.role IN ('viewer','editor'))) \
                   )) \
                   AND ($2 IS NULL OR dw.status = $2) \
                   AND ($3 IS NULL OR dw.current_stage_id = $3) \
                   AND ($4 IS NULL OR dw.workflow_template_id = $4) \
             ) \
             ORDER BY sort_order, title",
        )
        .bind(&space_id)
        .bind(query.workflow_status.as_deref())
        .bind(query.workflow_stage_id.as_deref())
        .bind(query.workflow_template_id.as_deref())
        .bind(&access.actor_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::internal)?
    };

    let initial_ids=rows.iter().map(|row|row.try_get::<String,_>("id")).collect::<Result<BTreeSet<_>,_>>().map_err(AppError::internal)?;
    let initial_access=super::access::resolve_page_batch(&mut conn,super::access::principal_from_auth(&auth)?,&space_id,&initial_ids,super::access::PageOperation::View).await?;
    let mut visible_rows = Vec::new();
    for row in rows {
        let id: String = row.try_get("id").map_err(AppError::internal)?;
        match initial_access.require(&id) {
            Ok(()) => visible_rows.push(row),
            Err(AppError::NotFound(_)) => {}
            Err(error) => return Err(error),
        }
    }
    let result: Vec<DocumentResponse> = visible_rows
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
        page_access_source: None,
                human_identifier: row.try_get("human_identifier")?,
                maintenance_user_id: row.try_get("maintenance_user_id")?,
                maintenance_group_id: row.try_get("maintenance_group_id")?,
                identifier_edit_allowed: None,
                maintenance_edit_allowed: None,
                page_edit_allowed: None,
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
                deleted_at: None,
                updated_at: row.try_get("updated_at")?,
                published_at: row.try_get("published_at")?,
                draft_id: row.try_get("draft_id")?,
                workflow_template_id: row.try_get("workflow_template_id")?,
                workflow_template_name: row.try_get("workflow_template_name")?,
                workflow_stage_id,
                workflow_stage_name,
                workflow_status: row.try_get("workflow_status")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(AppError::internal)?;

    let final_ids=result.iter().map(|page|page.id.clone()).collect();
    let final_access=super::access::resolve_page_batch(&mut conn,super::access::principal_from_auth(&auth)?,&space_id,&final_ids,super::access::PageOperation::View).await?;
    let mut current_result = Vec::new();
    for document in result {
        match final_access.require(&document.id)
        {
            Ok(()) => {
                if let Some(draft_id) = document.draft_id.as_deref() {
                    match final_access.require_draft_view(&mut conn,&document.id,draft_id).await
                    {
                        Ok(()) => {}
                        Err(AppError::NotFound(_)) => continue,
                        Err(error) => return Err(error),
                    }
                }
                let current:Option<(Option<String>,Option<String>,Option<String>,Option<String>)>=sqlx::query_as("SELECT published_at,human_identifier,maintenance_user_id,maintenance_group_id FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&document.id).bind(&space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
                if current
                    != Some((
                        document.published_at.clone(),
                        document.human_identifier.clone(),
                        document.maintenance_user_id.clone(),
                        document.maintenance_group_id.clone(),
                    ))
                {
                    continue;
                }
                current_result.push(document);
            }
            Err(AppError::NotFound(_)) => {}
            Err(error) => return Err(error),
        }
    }
    // No phase-local admission survives the selected-draft/metadata SQL above.
    let emit_ids=current_result.iter().map(|page|page.id.clone()).collect();
    let emit_access=super::access::resolve_page_batch(&mut conn,super::access::principal_from_auth(&auth)?,&space_id,&emit_ids,super::access::PageOperation::View).await?;
    let mut emitted=Vec::new();
    for page in current_result {
        match emit_access.require(&page.id) {
            Ok(())=>emitted.push(page), Err(AppError::NotFound(_))=>{}, Err(error)=>return Err(error),
        }
    }
    require_document_discovery_space_view(&mut conn, &auth, &space_id).await?;
    Ok(Json(emitted))
}

pub(crate) async fn require_document_creation_on_connection(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space_id: &str,
) -> Result<(), AppError> {
    let access = super::access::resolve_space_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space_id,
        super::access::Purpose::Routine,
    )
    .await?;
    if !access.allows(super::access::Permission::CreateContent) {
        return Err(AppError::NotFound(
            "Creation destination unavailable".into(),
        ));
    }
    Ok(())
}

/// POST /api/v1/spaces/{space_id}/documents
async fn create_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<CreateDocumentRequest>,
) -> Result<impl IntoResponse, AppError> {
    let (human_identifier, human_identifier_key) =
        super::page_metadata::normalize_identifier(req.human_identifier.as_deref())?;
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

    let principal = super::access::principal_from_auth(&auth)?;
    let doc_id = super::routes::import_id_or_uuid(req.source_id.as_deref())?;
    let created_at = super::routes::import_timestamp_or_now(req.created_at.as_deref())?;
    let updated_at =
        super::routes::import_timestamp_or_default(req.updated_at.as_deref(), &created_at)?;
    let content = req
        .content
        .as_deref()
        .unwrap_or(DEFAULT_NEW_DOCUMENT_CONTENT);

    let mut transaction = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut transaction).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut transaction, &[&space_id]).await?;
    require_document_creation_on_connection(&mut transaction, &auth, &space_id).await?;

    let existing_id: Option<(String,)> = sqlx::query_as("SELECT id FROM documents WHERE id = $1")
        .bind(&doc_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(AppError::internal)?;
    if existing_id.is_some() {
        return Err(AppError::Conflict(
            "A document with this import id already exists".into(),
        ));
    }

    super::folders::validate_document_folder_depth_on_connection(
        &mut transaction,
        &space_id,
        req.folder_id.as_deref(),
    )
    .await?;
    let mut template_metadata = match req.template_id.as_deref() {
        Some(template_id) => {
            resolve_template_metadata_on_connection(&mut transaction, &space_id, template_id)
                .await?
        }
        None => TemplateMetadata {
            tags: Vec::new(),
            property_values: Vec::new(),
        },
    };

    apply_create_property_values_on_connection(
        &mut transaction,
        &space_id,
        &req.property_values,
        &mut template_metadata,
    )
    .await?;

    // Get space slug for file path
    let slug: (String,) = sqlx::query_as("SELECT slug FROM spaces WHERE id = $1")
        .bind(&space_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(AppError::internal)?;

    let maintenance_target = super::maintenance::validate_assignment(
        &mut transaction,
        &auth,
        &space_id,
        req.maintenance_user_id.as_deref(),
        req.maintenance_group_id.as_deref(),
        super::maintenance::TargetUse::Select,
    )
    .await?;
    // Build frontmatter
    super::page_metadata::ensure_identifier_available(
        &mut transaction,
        &space_id,
        human_identifier_key.as_deref(),
        None,
    )
    .await?;
    let frontmatter = document_storage_frontmatter(&req.title, req.icon.as_deref(), false, false);

    // Lock and validate the final template/default + explicit property batch before
    // any filesystem write. Keep those locks through document/metadata insertion.
    template_metadata
        .property_values
        .sort_by(|a, b| a.0.cmp(&b.0));
    let assignments: Vec<(&str, Option<&str>)> = template_metadata
        .property_values
        .iter()
        .map(|(property_id, value)| (property_id.as_str(), Some(value.as_str())))
        .collect();
    let relation_principal = principal;
    let relation_targets = super::property_validation::validate_assignment_batch(
        &mut transaction,
        &space_id,
        relation_principal,
        &assignments,
    )
    .await?;
    validate_create_parent_on_connection(
        &mut transaction,
        &space_id,
        req.parent_id.as_deref(),
        req.folder_id.as_deref(),
        principal,
    )
    .await?;
    require_document_creation_on_connection(&mut transaction, &auth, &space_id).await?;
    for (tag_id, _) in &template_metadata.tags {
        let tagged = sqlx::query("UPDATE tags SET id=id WHERE id=$1 AND space_id=$2")
            .bind(tag_id)
            .bind(&space_id)
            .execute(&mut *transaction)
            .await
            .map_err(AppError::internal)?;
        if tagged.rows_affected() != 1 {
            return Err(AppError::BadRequest(
                "Template tag binding is unavailable".into(),
            ));
        }
    }
    require_document_creation_on_connection(&mut transaction, &auth, &space_id).await?;
    // Private durable stage + receipt in this guarded canonical transaction.
    let draft_id = Uuid::new_v4().to_string();
    let storage_stage = storage::journal::stage_document(
        &mut transaction,
        &space_id,
        &doc_id,
        &draft_id,
        &updated_at,
        &slug.0,
        &req.title,
        &frontmatter,
        content,
    )
    .await?;
    let content_path = storage_stage.path.clone();

    // Keep the document and its template metadata together in one database transaction.
    sqlx::query(
        "INSERT INTO documents (id, space_id, folder_id, parent_id, title, icon, cover_image, content_path, created_by, last_updated_by, created_at, updated_at, human_identifier, human_identifier_key, maintenance_user_id, maintenance_group_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9, $10, $11, $12, $13, $14, $15)",
    )
    .bind(&doc_id)
    .bind(&space_id)
    .bind(&req.folder_id)
    .bind(&req.parent_id)
    .bind(&req.title)
    .bind(&req.icon)
    .bind(&req.cover_image)
    .bind(&content_path)
    .bind(&auth.claims.sub)
    .bind(&created_at)
    .bind(&updated_at)
    .bind(&human_identifier)
    .bind(&human_identifier_key)
    .bind(&req.maintenance_user_id)
    .bind(&req.maintenance_group_id)
    .execute(&mut *transaction)
    .await
    .map_err(AppError::internal)?;

    sqlx::query(
        "INSERT INTO document_drafts \
         (id, document_id, space_id, owner_id, title, icon, content, base_updated_at, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(&draft_id)
    .bind(&doc_id)
    .bind(&space_id)
    .bind(&auth.claims.sub)
    .bind(&req.title)
    .bind(&req.icon)
    .bind(content)
    .bind(&updated_at)
    .bind(&created_at)
    .bind(&updated_at)
    .execute(&mut *transaction)
    .await
    .map_err(AppError::internal)?;

    // Auto-subscribe creator
    sqlx::query(
        "INSERT INTO doc_subscriptions (document_id, user_id) VALUES ($1, $2) \
         ON CONFLICT(document_id, user_id) DO NOTHING",
    )
    .bind(&doc_id)
    .bind(&auth.claims.sub)
    .execute(&mut *transaction)
    .await
    .map_err(AppError::internal)?;

    for (tag_id, _) in &template_metadata.tags {
        sqlx::query(
            "INSERT INTO doc_tags (document_id, tag_id) VALUES ($1, $2) \
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
             VALUES ($1, $2, $3, $4, $5, $6)",
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

    let stored_placement: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT folder_id,parent_id FROM documents WHERE id=$1 AND space_id=$2")
            .bind(&doc_id)
            .bind(&space_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(AppError::internal)?;
    if stored_placement != Some((req.folder_id.clone(), req.parent_id.clone())) {
        return Err(AppError::Conflict(
            "Created page placement changed; retry creation".into(),
        ));
    }
    // Canonical inserts may have waited on legacy writers. Recheck current
    // placement and authority on this same connection before committing.
    super::folders::validate_document_folder_depth_on_connection(
        &mut transaction,
        &space_id,
        req.folder_id.as_deref(),
    )
    .await?;
    validate_create_parent_on_connection(
        &mut transaction,
        &space_id,
        req.parent_id.as_deref(),
        req.folder_id.as_deref(),
        principal,
    )
    .await?;

    for (tag_id, _) in &template_metadata.tags {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM tags WHERE id=$1 AND space_id=$2")
                .bind(tag_id)
                .bind(&space_id)
                .fetch_optional(&mut *transaction)
                .await
                .map_err(AppError::internal)?;
        if exists.is_none() {
            return Err(AppError::BadRequest(
                "Template tag binding is unavailable".into(),
            ));
        }
    }
    require_document_creation_on_connection(&mut transaction, &auth, &space_id).await?;
    let inserted_page: Option<i64> = sqlx::query_scalar("SELECT 1 FROM documents WHERE id=$1 AND space_id=$2 AND (folder_id=$3 OR (folder_id IS NULL AND $3 IS NULL)) AND (parent_id=$4 OR (parent_id IS NULL AND $4 IS NULL)) AND archived_at IS NULL AND deleted_at IS NULL AND published_at IS NULL AND public=0 AND locked=0 AND created_by=$5 AND last_updated_by=$5 AND content_path=$6 AND updated_at=$7 AND title=$8 AND (icon=$9 OR (icon IS NULL AND $9 IS NULL)) AND (cover_image=$10 OR (cover_image IS NULL AND $10 IS NULL)) AND created_at=$11 AND (human_identifier=$12 OR(human_identifier IS NULL AND $12 IS NULL)) AND (human_identifier_key=$13 OR(human_identifier_key IS NULL AND $13 IS NULL)) AND (maintenance_user_id=$14 OR(maintenance_user_id IS NULL AND $14 IS NULL)) AND (maintenance_group_id=$15 OR(maintenance_group_id IS NULL AND $15 IS NULL))")
        .bind(&doc_id).bind(&space_id).bind(&req.folder_id).bind(&req.parent_id).bind(&auth.claims.sub).bind(&content_path).bind(&updated_at).bind(&req.title).bind(&req.icon).bind(&req.cover_image).bind(&created_at).bind(&human_identifier).bind(&human_identifier_key).bind(&req.maintenance_user_id).bind(&req.maintenance_group_id).fetch_optional(&mut *transaction).await.map_err(AppError::internal)?;
    let inserted_draft: Option<i64> = sqlx::query_scalar("SELECT 1 FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3 AND owner_id=$4 AND base_updated_at=$5 AND title=$6 AND (icon=$7 OR (icon IS NULL AND $7 IS NULL)) AND content=$8 AND created_at=$9 AND updated_at=$10")
        .bind(&draft_id).bind(&doc_id).bind(&space_id).bind(&auth.claims.sub).bind(&updated_at).bind(&req.title).bind(&req.icon).bind(content).bind(&created_at).bind(&updated_at).fetch_optional(&mut *transaction).await.map_err(AppError::internal)?;
    if inserted_page.is_none() || inserted_draft.is_none() {
        return Err(AppError::Conflict(
            "Created page or initial draft changed; retry creation".into(),
        ));
    }
    super::property_validation::revalidate_relation_targets(
        &mut transaction,
        &space_id,
        relation_principal,
        &relation_targets,
    )
    .await?;
    if super::maintenance::validate_assignment(
        &mut transaction,
        &auth,
        &space_id,
        req.maintenance_user_id.as_deref(),
        req.maintenance_group_id.as_deref(),
        super::maintenance::TargetUse::Select,
    )
    .await?
        != maintenance_target
    {
        return Err(AppError::Conflict(
            "Maintenance target changed during creation".into(),
        ));
    }
    // Initial save baseline comes from the actual newly-created raw row in this
    // creation transaction, never a later GET or a caller-manufactured default.
    let draft_source_digest=super::draft_save::digest(&super::draft_save::selected(&mut transaction,&space_id,&doc_id,&draft_id).await?)?;
    require_document_creation_on_connection(&mut transaction, &auth, &space_id).await?;
    if let Err(commit_error) = transaction.commit().await {
        if !storage::journal::confirm_commit(&state.db, &storage_stage)
            .await
            .unwrap_or(false)
        {
            return Err(AppError::internal(commit_error));
        }
    }
    let storage_complete = storage::journal::publish_committed(&state.db, &storage_stage).await;

    tracing::info!(
        action = "document.created",
        space_id = %space_id,
        document_id = %doc_id,
        draft_id = %draft_id,
        user_id = %auth.claims.sub,
        title = %req.title,
        folder_id = ?req.folder_id,
        parent_id = ?req.parent_id,
        "Created document"
    );

    // --- Wire: Webhook dispatch ---
    crate::webhooks::dispatch::dispatch_event(
        &state.db,
        "document.draft_created",
        &serde_json::json!({
            "document_id": doc_id,
            "draft_id": draft_id,
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
        "document.draft_create",
        Some("document_draft"),
        Some(&draft_id),
    )
    .await;

    let response = DocumentResponse {
        page_access_source: None,
        human_identifier,
        maintenance_user_id: req.maintenance_user_id,
        maintenance_group_id: req.maintenance_group_id,
        identifier_edit_allowed: None,
        maintenance_edit_allowed: None,
        page_edit_allowed: None,
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
        deleted_at: None,
        updated_at,
        published_at: None,
        draft_id: Some(draft_id),
        workflow_template_id: None,
        workflow_template_name: None,
        workflow_stage_id: None,
        workflow_stage_name: None,
        workflow_status: None,
    };
    let mut response = serde_json::to_value(response).map_err(AppError::internal)?;
    response["draft_source_digest"]=serde_json::json!(draft_source_digest);
    if !storage_complete {
        response["storage_pending"] = serde_json::json!(true);
        response["op_id"] = serde_json::json!(storage_stage.op_id);
    }
    Ok((
        if storage_complete {
            StatusCode::CREATED
        } else {
            StatusCode::ACCEPTED
        },
        Json(response),
    ))
}

#[cfg(test)]
async fn apply_create_property_values(
    state: &AppState,
    space_id: &str,
    assignments: &[CreatePropertyValue],
    template_metadata: &mut TemplateMetadata,
) -> Result<(), AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    apply_create_property_values_on_connection(&mut conn, space_id, assignments, template_metadata)
        .await
}

async fn apply_create_property_values_on_connection(
    conn: &mut sqlx::AnyConnection,
    space_id: &str,
    assignments: &[CreatePropertyValue],
    template_metadata: &mut TemplateMetadata,
) -> Result<(), AppError> {
    if assignments.len() > 100 {
        return Err(AppError::BadRequest(
            "At most 100 property values may be supplied".into(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for assignment in assignments {
        if !seen.insert(&assignment.property_id) {
            return Err(AppError::BadRequest(
                "A property may only appear once in a batch".into(),
            ));
        }
        if assignment.value.len() > 100_000 {
            return Err(AppError::BadRequest("Property value is too long".into()));
        }
        let schema: Option<(String,)> =
            sqlx::query_as("SELECT id FROM properties_schema WHERE id = $1 AND space_id = $2")
                .bind(&assignment.property_id)
                .bind(&space_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(AppError::internal)?;
        if schema.is_none() {
            return Err(AppError::BadRequest(
                "Property binding is not in this space".into(),
            ));
        }
        template_metadata
            .property_values
            .retain(|(id, _)| id != &assignment.property_id);
        template_metadata
            .property_values
            .push((assignment.property_id.clone(), assignment.value.clone()));
    }

    Ok(())
}

async fn resolve_template_metadata_on_connection(
    conn: &mut sqlx::AnyConnection,
    space_id: &str,
    template_id: &str,
) -> Result<TemplateMetadata, AppError> {
    let locked = sqlx::query("UPDATE templates SET id=id WHERE id=$1 AND space_id=$2")
        .bind(template_id)
        .bind(space_id)
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if locked.rows_affected() != 1 {
        return Err(AppError::NotFound("Template not found".into()));
    }
    let template: Option<(Option<String>, Option<String>)> =
        sqlx::query_as("SELECT properties, tags FROM templates WHERE id = $1 AND space_id = $2")
            .bind(template_id)
            .bind(space_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    let (properties, tags) =
        template.ok_or_else(|| AppError::NotFound("Template not found".into()))?;

    let tag_names = match tags {
        Some(raw) => serde_json::from_str::<Vec<String>>(&raw)
            .map_err(|_| AppError::BadRequest("Template tags are malformed".into()))?,
        None => Vec::new(),
    };
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
            "SELECT id, name FROM tags WHERE space_id = $1 AND lower(name) = lower($2) ORDER BY name LIMIT 1",
        )
        .bind(space_id)
        .bind(name)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?
        {
            resolved_tags.push((id, stored_name));
        }
    }

    let property_object = match properties {
        Some(raw) => serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&raw)
            .map_err(|_| AppError::BadRequest("Template properties are malformed".into()))?,
        None => serde_json::Map::new(),
    };
    let mut property_values = Vec::new();
    for (name, value) in property_object {
        if name.starts_with("__starlane_") {
            continue;
        }
        let Some(value) = template_property_value(&value) else {
            continue;
        };
        if let Some((id,)) = sqlx::query_as::<_, (String,)>(
            "SELECT id FROM properties_schema WHERE space_id = $1 AND lower(name) = lower($2) ORDER BY name LIMIT 1",
        )
        .bind(space_id)
        .bind(&name)
        .fetch_optional(&mut *conn)
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

#[cfg(test)]
tokio::task_local! {
    pub(crate) static IDENTIFIER_GET_AFTER_CAPABILITY: (sqlx::AnyPool, &'static str);
    pub(crate) static PAGE_EDIT_GET_AFTER_INVENTORY: (sqlx::AnyPool, &'static str);
}

/// Project current ordinary human Edit under the exact existing lock/restriction
/// resolver. A matching locked_by is insufficient without this authority.
async fn current_page_edit_allowed(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    document: &str,
) -> Result<bool, AppError> {
    let result = async {
        super::access::require_current_human_preference_write(conn, auth).await?;
        super::access::resolve_page_access(
            conn, super::access::principal_from_auth(auth)?, space, document,
            super::access::PageOperation::Edit, super::access::Purpose::Routine,
        ).await?;
        Ok::<(), AppError>(())
    }.await;
    match result {
        Ok(()) => Ok(true),
        Err(AppError::NotFound(_)) | Err(AppError::Forbidden(_)) | Err(AppError::Conflict(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

/// GET /api/v1/spaces/{space_id}/documents/{doc_id}
async fn get_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let active_published: Option<i64> = sqlx::query_scalar("SELECT 1 FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(&doc_id).bind(&space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if active_published.is_some() {
        let page=super::shared_pages::published_view(&mut conn,&auth,&space_id,&doc_id).await?;
        if page.direct_viewer_grant.is_some() {
            let original=super::published_source::selected(&mut conn,&space_id,&doc_id).await?;
            let proof=super::published_source::capture_source(&mut conn,&auth,&space_id,&doc_id,&page.ancestor_ids).await?;
            let mut doc=fetch_document_on_connection(&mut conn,&doc_id,Some(&space_id)).await?;
            #[cfg(test)]
            super::shared_pages::reader_wait().await;
            // Routing IDs and published presentation only. A direct page grant
            // does not expose placement, recipients, editor catalogs or drafts.
            doc.folder_id=None;doc.parent_id=None;doc.maintenance_user_id=None;doc.maintenance_group_id=None;
            doc.locked_by=None;doc.created_by=String::new();doc.last_updated_by=String::new();
            doc.page_access_source=Some("direct_page_grant".into());
            doc.identifier_edit_allowed=Some(false);doc.maintenance_edit_allowed=Some(false);doc.page_edit_allowed=Some(false);
            if super::published_source::selected(&mut conn,&space_id,&doc_id).await?!=original {return Err(super::published_source::conflict());}
            super::published_source::verify_source(&mut conn,&proof).await?;
            if super::shared_pages::published_view(&mut conn,&auth,&space_id,&doc_id).await?.direct_viewer_grant!=page.direct_viewer_grant {return Err(super::published_source::conflict());}
            super::access::require_current_human_read(&mut conn,&auth).await?;
            return Ok(([(axum::http::header::CACHE_CONTROL,"no-store")],Json(doc)));
        }
    }
    let metadata_access = require_document_metadata_view(&mut conn, &auth, &space_id, &doc_id).await?;
    let metadata_source = super::published_source::capture_source(&mut conn, &auth, &space_id, &doc_id, &metadata_access.ancestor_ids).await?;
    let mut doc = fetch_document_on_connection(&mut conn, &doc_id, Some(&space_id)).await?;
    let archived: Option<String> =
        sqlx::query_scalar("SELECT archived_at FROM documents WHERE id=$1 AND space_id=$2")
            .bind(&doc_id)
            .bind(&space_id)
            .fetch_one(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    let mut selected_draft = None;
    let mut archived_selected_draft = None;
    if doc.published_at.is_none() && archived.is_some() {
        let draft = readable_lifecycle_metadata_draft(&mut conn, &auth, &space_id, &doc_id, false)
            .await?
            .ok_or_else(|| AppError::NotFound("Target unavailable".into()))?;
        doc.title = draft.title.clone();
        doc.icon = draft.icon.clone();
        doc.cover_image = draft.cover_image.clone();
        doc.draft_id = Some(draft.id.clone());
        archived_selected_draft = Some(draft);
    }
    if doc.published_at.is_none() && archived.is_none() {
        let draft = readable_discovery_draft(&mut conn, &auth, &space_id, &doc_id).await?;
        doc.title = draft.title.clone();
        doc.icon = draft.icon.clone();
        doc.cover_image = draft.cover_image.clone();
        doc.draft_id = Some(draft.id.clone());
        require_discovery_draft_view(&mut conn, &auth, &space_id, &doc_id, &draft.id).await?;
        selected_draft = Some(draft);
    }
    require_document_metadata_view(&mut conn, &auth, &space_id, &doc_id).await?;
    doc.page_edit_allowed = Some(current_page_edit_allowed(&mut conn, &auth, &space_id, &doc_id).await?);
    doc.identifier_edit_allowed =
        Some(super::page_metadata::edit_allowed(&mut conn, &auth, &space_id, &doc_id).await?);
    doc.maintenance_edit_allowed =
        Some(super::maintenance::edit_allowed(&mut conn, &auth, &space_id, &doc_id).await?);
    #[cfg(test)]
    if let Ok((db, statement)) = IDENTIFIER_GET_AFTER_CAPABILITY.try_with(Clone::clone) {
        sqlx::query(statement)
            .execute(&db)
            .await
            .map_err(AppError::internal)?;
    }
    let current: Option<(Option<String>,Option<String>,String,Option<String>,i32,Option<String>,Option<String>,Option<String>)> = sqlx::query_as("SELECT published_at,archived_at,updated_at,human_identifier,locked,locked_by,maintenance_user_id,maintenance_group_id FROM documents WHERE id=$1 AND space_id=$2 AND deleted_at IS NULL").bind(&doc_id).bind(&space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if current
        != Some((
            doc.published_at.clone(),
            archived,
            doc.updated_at.clone(),
            doc.human_identifier.clone(),
            i32::from(doc.locked),
            doc.locked_by.clone(),
            doc.maintenance_user_id.clone(),
            doc.maintenance_group_id.clone(),
        ))
    {
        return Err(AppError::Conflict(
            "Page changed while being read; retry".into(),
        ));
    }
    require_document_metadata_view(&mut conn, &auth, &space_id, &doc_id).await?;
    if let Some(draft) = selected_draft {
        require_current_content_draft_source(&mut conn, &auth, &space_id, &doc_id, &draft).await?;
    }
    if let Some(draft) = archived_selected_draft {
        require_current_lifecycle_draft_metadata(
            &mut conn, &auth, &space_id, &doc_id, &draft, false,
        )
        .await?;
    }
    super::published_source::verify_source(&mut conn, &metadata_source).await?;
    #[cfg(test)]
    if let Ok((db, statement)) = PAGE_EDIT_GET_AFTER_INVENTORY.try_with(Clone::clone) {
        sqlx::query(statement).execute(&db).await.map_err(AppError::internal)?;
    }
    if doc.page_edit_allowed != Some(current_page_edit_allowed(&mut conn, &auth, &space_id, &doc_id).await?) {
        return Err(super::published_source::conflict());
    }
    super::access::require_current_human_read(&mut conn, &auth).await?;
    Ok(([(axum::http::header::CACHE_CONTROL,"no-store")],Json(doc)))
}

fn placement_unavailable() -> AppError {
    AppError::NotFound("Page unavailable".into())
}
async fn require_scoped_page_operation(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    doc: &str,
    operation: super::access::PageOperation,
) -> Result<(), AppError> {
    super::access::resolve_page_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        doc,
        operation,
        super::access::Purpose::Routine,
    )
    .await?;
    Ok(())
}
async fn begin_scoped_page_mutation<'a>(
    state: &'a AppState,
    auth: &AuthUser,
    space: &str,
    doc: &str,
    operation: super::access::PageOperation,
) -> Result<sqlx::Transaction<'a, sqlx::Any>, AppError> {
    super::access::principal_from_auth(auth)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[space]).await?;
    require_scoped_page_operation(&mut tx, auth, space, doc, operation).await?;
    let row=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND deleted_at IS NULL AND archived_at IS NULL")
        .bind(doc).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    if row.rows_affected() != 1 {
        return Err(placement_unavailable());
    }
    require_scoped_page_operation(&mut tx, auth, space, doc, operation).await?;
    Ok(tx)
}
#[derive(Clone, Debug, Eq, PartialEq)]
struct PlacementPage {
    id: String,
    folder_id: Option<String>,
    parent_id: Option<String>,
}
async fn placement_page(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    id: &str,
) -> Result<PlacementPage, AppError> {
    let row:Option<(Option<String>,Option<String>)>=sqlx::query_as("SELECT folder_id,parent_id FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(id).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let (folder_id, parent_id) = row.ok_or_else(placement_unavailable)?;
    Ok(PlacementPage {
        id: id.into(),
        folder_id,
        parent_id,
    })
}
async fn collect_placement_subtree(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    root: &str,
) -> Result<Vec<PlacementPage>, AppError> {
    collect_placement_subtree_with_budget(conn, space, root, &mut super::access::PageTraversalBudget::default()).await
}
async fn collect_placement_subtree_with_budget(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    root: &str,
    budget: &mut super::access::PageTraversalBudget,
) -> Result<Vec<PlacementPage>, AppError> {
    let mut pages = std::collections::BTreeMap::new();
    let mut pending = std::collections::VecDeque::from([root.to_owned()]);
    let mut discovered = BTreeSet::from([root.to_owned()]);
    budget.visit(root)?;
    while let Some(id) = pending.pop_front() {
        let page = placement_page(conn, space, &id).await?;
        let limit = super::access::MAX_PAGE_OPERATION_NODES.saturating_sub(budget.len()) + 1;
        let children: Vec<(String, String)> = sqlx::query_as(
            "SELECT id,space_id FROM documents WHERE parent_id=$1 ORDER BY id LIMIT $2",
        ).bind(&id).bind(limit as i64).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
        for (child, child_space) in children {
            if child_space != space { return Err(AppError::Conflict("Page hierarchy unavailable for mutation".into())); }
            if !discovered.insert(child.clone()) { return Err(AppError::Conflict("Page hierarchy contains a cycle".into())); }
            budget.visit(&child)?;
            pending.push_back(child);
        }
        pages.insert(id, page);
    }
    Ok(pages.into_values().collect())
}
async fn placement_ancestors_with_budget(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    parent: Option<&str>,
    folder: Option<&str>,
    subtree: &BTreeSet<String>,
    budget: &mut super::access::PageTraversalBudget,
) -> Result<Vec<PlacementPage>, AppError> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    let mut next = parent.map(str::to_owned);
    while let Some(id) = next {
        if subtree.contains(&id) || !seen.insert(id.clone()) {
            return Err(AppError::BadRequest("A page cannot be moved inside itself or its descendant".into()));
        }
        budget.visit(&id)?;
        let page = placement_page(conn, space, &id).await?;
        if page.folder_id.as_deref() != folder {
            return Err(AppError::Conflict("Child pages must stay in the same folder as their parent".into()));
        }
        next = page.parent_id.clone(); result.push(page);
    }
    Ok(result)
}
async fn placement_ancestors(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    parent: Option<&str>,
    folder: Option<&str>,
    subtree: &BTreeSet<String>,
) -> Result<Vec<PlacementPage>, AppError> {
    let mut budget=super::access::PageTraversalBudget::default();
    for id in subtree { budget.visit(id)?; }
    placement_ancestors_with_budget(conn,space,parent,folder,subtree,&mut budget).await
}
/// Validate connectedness once, rather than walking every page back to the root.
fn validate_placement_graph(root: &str, pages: &[PlacementPage]) -> Result<(),AppError> {
    let root_page=pages.iter().find(|page|page.id==root).ok_or_else(placement_unavailable)?;
    let mut children=std::collections::BTreeMap::<&str,Vec<&str>>::new();
    for page in pages {
        if page.folder_id!=root_page.folder_id {return Err(AppError::Conflict("Page subtree has inconsistent folder placement".into()));}
        if page.id!=root {children.entry(page.parent_id.as_deref().ok_or_else(||AppError::Conflict("Page subtree is disconnected".into()))?).or_default().push(&page.id);}
    }
    let mut seen=BTreeSet::new();let mut pending=vec![root];
    while let Some(id)=pending.pop() {
        if !seen.insert(id) {return Err(AppError::Conflict("Page hierarchy contains a cycle".into()));}
        if let Some(children)=children.get(id) {pending.extend(children.iter().copied());}
    }
    if seen.len()!=pages.len() {return Err(AppError::Conflict("Page subtree is disconnected".into()));}
    Ok(())
}
async fn lock_scoped_placement_rows(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    ids: &std::collections::BTreeSet<String>,
) -> Result<(), AppError> {
    for id in ids {
        let locked=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL")
            .bind(id).bind(space).execute(&mut *conn).await.map_err(AppError::internal)?;
        if locked.rows_affected() != 1 {
            return Err(placement_unavailable());
        }
    }
    Ok(())
}
async fn placement_folder_chain(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    folder: Option<&str>,
) -> Result<Vec<(String, Option<String>)>, AppError> {
    let mut next = folder.map(str::to_owned);
    let mut seen = std::collections::BTreeSet::new();
    let mut result = Vec::new();
    while let Some(id) = next {
        if !seen.insert(id.clone()) {
            return Err(AppError::Conflict(
                "Folder hierarchy contains a cycle".into(),
            ));
        }
        let row: Option<(Option<String>,)> =
            sqlx::query_as("SELECT parent_id FROM folders WHERE id=$1 AND space_id=$2")
                .bind(&id)
                .bind(space)
                .fetch_optional(&mut *conn)
                .await
                .map_err(AppError::internal)?;
        let parent = row
            .ok_or_else(|| AppError::NotFound("Folder unavailable".into()))?
            .0;
        next = parent.clone();
        result.push((id, parent));
    }
    Ok(result)
}
async fn lock_placement_folders(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    folder: Option<&str>,
) -> Result<Vec<(String, Option<String>)>, AppError> {
    let before = placement_folder_chain(conn, space, folder).await?;
    let ids = before
        .iter()
        .map(|row| row.0.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    for id in ids {
        let updated = sqlx::query("UPDATE folders SET id=id WHERE id=$1 AND space_id=$2")
            .bind(id)
            .bind(space)
            .execute(&mut *conn)
            .await
            .map_err(AppError::internal)?;
        if updated.rows_affected() != 1 {
            return Err(AppError::NotFound("Folder unavailable".into()));
        }
    }
    if placement_folder_chain(conn, space, folder).await? != before {
        return Err(AppError::Conflict("Folder hierarchy changed; retry".into()));
    }
    Ok(before)
}
async fn validate_placement_candidate(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    root: &str,
    pages: &[PlacementPage],
    ancestors: &[PlacementPage],
    folder: Option<&str>,
    parent: Option<&str>,
) -> Result<(), AppError> {
    validate_placement_graph(root,pages)?;
    let mut targets=BTreeSet::new();
    for page in pages {targets.insert(page.id.clone());}
    for ancestor in ancestors {targets.insert(ancestor.id.clone());}
    // Explicit targets preserve the old strong Move checks. Source ancestors
    // remain weak ancestors unless they are also closure/destination targets.
    super::folders::validate_document_folder_depth_on_connection(conn, space, folder).await?;
    let subtree = pages.iter().map(|page| page.id.clone()).collect();
    if placement_ancestors(conn, space, parent, folder, &subtree).await? != ancestors {
        return Err(AppError::Conflict("Parent hierarchy changed; retry".into()));
    }
    super::access::resolve_page_batch(conn,super::access::principal_from_auth(auth)?,space,&targets,super::access::PageOperation::Move).await?.require_all()?;
    // Last authority resolution follows the preceding hierarchy/folder reads.
    require_scoped_page_operation(conn, auth, space, root, super::access::PageOperation::Move)
        .await?;
    Ok(())
}

/// PUT /api/v1/spaces/{space_id}/documents/{doc_id}
#[derive(Clone)]
enum PlacementChange {
    Unchanged,
    Set(Option<String>),
}
impl PlacementChange {
    fn resolve(&self, current: &Option<String>) -> Option<String> {
        match self {
            Self::Unchanged => current.clone(),
            Self::Set(value) => value.clone(),
        }
    }
}
#[derive(Clone, Copy)]
enum PlacementActorStamp {
    Preserve,
    CurrentActor,
}
impl PlacementActorStamp {
    fn actor<'a>(self, auth: &'a AuthUser) -> Option<&'a str> {
        match self {
            Self::Preserve => None,
            Self::CurrentActor => Some(&auth.claims.sub),
        }
    }
}
async fn apply_document_placement(
    tx: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space_id: &str,
    doc_id: &str,
    folder_change: PlacementChange,
    parent_change: PlacementChange,
    stamp: PlacementActorStamp,
) -> Result<Vec<String>, AppError> {
    apply_document_placement_at(
        tx,
        auth,
        space_id,
        doc_id,
        folder_change,
        parent_change,
        stamp,
        None,
    )
    .await
}
pub(crate) async fn apply_replay_document_placement(
    tx: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    doc: &str,
    folder: Option<Option<String>>,
    parent: Option<Option<String>>,
    revision: &str,
) -> Result<Vec<String>, AppError> {
    apply_document_placement_at(
        tx,
        auth,
        space,
        doc,
        folder
            .map(PlacementChange::Set)
            .unwrap_or(PlacementChange::Unchanged),
        parent
            .map(PlacementChange::Set)
            .unwrap_or(PlacementChange::Unchanged),
        PlacementActorStamp::CurrentActor,
        Some(revision),
    )
    .await
}
async fn apply_document_placement_at(
    tx: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space_id: &str,
    doc_id: &str,
    folder_change: PlacementChange,
    parent_change: PlacementChange,
    stamp: PlacementActorStamp,
    imported_revision: Option<&str>,
) -> Result<Vec<String>, AppError> {
    let mut page_budget=super::access::PageTraversalBudget::default();
    let before = collect_placement_subtree_with_budget(&mut *tx, space_id, doc_id, &mut page_budget).await?;
    let root = before
        .iter()
        .find(|page| page.id == doc_id)
        .ok_or_else(placement_unavailable)?;
    let folder = folder_change.resolve(&root.folder_id);
    let parent = parent_change.resolve(&root.parent_id);
    let closure=before.iter().map(|page|page.id.clone()).collect::<BTreeSet<_>>();
    let source_ancestors=placement_ancestors_with_budget(&mut *tx,space_id,root.parent_id.as_deref(),root.folder_id.as_deref(),&closure,&mut page_budget).await?;
    let folder_ancestors = lock_placement_folders(&mut *tx, space_id, folder.as_deref()).await?;
    let mut lock_ids = before
        .iter()
        .map(|page| page.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let ancestors = placement_ancestors_with_budget(
        &mut *tx,
        space_id,
        parent.as_deref(),
        folder.as_deref(),
        &lock_ids,
        &mut page_budget,
    )
    .await?;
    lock_ids.extend(ancestors.iter().map(|page| page.id.clone()));
    lock_ids.extend(source_ancestors.iter().map(|page|page.id.clone()));
    lock_scoped_placement_rows(&mut *tx, space_id, &lock_ids).await?;
    if placement_ancestors_with_budget(&mut *tx,space_id,root.parent_id.as_deref(),root.folder_id.as_deref(),&closure,&mut page_budget).await?!=source_ancestors {
        return Err(AppError::Conflict("Source parent hierarchy changed; retry".into()));
    }
    if collect_placement_subtree(&mut *tx, space_id, doc_id).await? != before {
        return Err(AppError::Conflict("Page hierarchy changed; retry".into()));
    }
    validate_placement_candidate(
        &mut *tx,
        auth,
        space_id,
        doc_id,
        &before,
        &ancestors,
        folder.as_deref(),
        parent.as_deref(),
    )
    .await?;
    let now = imported_revision
        .map(str::to_owned)
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    if !matches!(folder_change, PlacementChange::Unchanged)
        || !matches!(parent_change, PlacementChange::Unchanged)
    {
        for page in &before {
            let parent_value = if page.id == doc_id {
                parent.as_deref()
            } else {
                page.parent_id.as_deref()
            };
            let changed=sqlx::query("UPDATE documents SET folder_id=$1,parent_id=$2,updated_at=$3,last_updated_by=CASE WHEN $6 IS NULL THEN last_updated_by ELSE $6 END WHERE id=$4 AND space_id=$5 AND archived_at IS NULL AND deleted_at IS NULL")
                .bind(&folder).bind(parent_value).bind(&now).bind(&page.id).bind(space_id).bind(stamp.actor(auth)).execute(&mut *tx).await.map_err(AppError::internal)?;
            if changed.rows_affected() != 1 {
                return Err(placement_unavailable());
            }
        }
    }
    let expected = before
        .iter()
        .cloned()
        .map(|mut page| {
            page.folder_id = folder.clone();
            if page.id == doc_id {
                page.parent_id = parent.clone();
            }
            page
        })
        .collect::<Vec<_>>();
    if collect_placement_subtree(&mut *tx, space_id, doc_id).await? != expected {
        return Err(AppError::Conflict(
            "Page placement changed during update; retry".into(),
        ));
    }
    validate_placement_candidate(
        &mut *tx,
        auth,
        space_id,
        doc_id,
        &expected,
        &ancestors,
        folder.as_deref(),
        parent.as_deref(),
    )
    .await?;
    if placement_folder_chain(&mut *tx, space_id, folder.as_deref()).await? != folder_ancestors {
        return Err(AppError::Conflict("Folder hierarchy changed; retry".into()));
    }
    require_scoped_page_operation(
        &mut *tx,
        auth,
        space_id,
        doc_id,
        super::access::PageOperation::Move,
    )
    .await?;
    Ok(expected.into_iter().map(|page| page.id).collect())
}

async fn update_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    Json(req): Json<UpdateDocumentRequest>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = begin_scoped_page_mutation(
        &state,
        &auth,
        &space_id,
        &doc_id,
        super::access::PageOperation::Move,
    )
    .await?;
    if req.title.is_some() || req.icon.is_some() || req.cover_image.is_some() {
        return Err(AppError::Conflict(
            "Page content and presentation must be changed through a draft".into(),
        ));
    }
    apply_document_placement(
        &mut tx,
        &auth,
        &space_id,
        &doc_id,
        req.folder_id
            .clone()
            .map(|id| PlacementChange::Set(Some(id)))
            .unwrap_or(PlacementChange::Unchanged),
        req.parent_id
            .clone()
            .map(|id| PlacementChange::Set(Some(id)))
            .unwrap_or(PlacementChange::Unchanged),
        PlacementActorStamp::Preserve,
    )
    .await?;
    tx.commit().await.map_err(AppError::internal)?;

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

    tracing::info!(
        action = "document.saved",
        space_id = %space_id,
        document_id = %doc_id,
        user_id = %auth.claims.sub,
        title_changed = req.title.is_some(),
        metadata_changed = req.title.is_some() || req.folder_id.is_some() || req.parent_id.is_some() || req.icon.is_some() || req.cover_image.is_some(),
        "Saved document"
    );

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/v1/spaces/{space_id}/documents/{doc_id}
async fn delete_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let committed = super::lifecycle::trash(&state, &auth, &space_id, &doc_id).await?;
    if let Err(error) = super::lifecycle::remove_index(&state, &auth, &committed).await {
        tracing::warn!(document_id=%doc_id,?error,"Trash index removal deferred");
    }
    // Derived lifecycle copies are deferred; preserve all existing bytes.
    crate::webhooks::dispatch::dispatch_event(
        &state.db,
        "document.trashed",
        &committed.webhook,
        Some(&space_id),
    )
    .await;
    crate::audit::append_committed_move_action(&committed.audit).await;
    tracing::info!(action = "document.trashed", space_id = %space_id, document_id = %doc_id, user_id = %auth.claims.sub, "Moved document to trash");

    Ok(StatusCode::NO_CONTENT)
}

async fn list_trashed_documents(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    list_lifecycle_documents(&state, &auth, &space_id, true).await
}

/// Copy is a source read and a destination creation. It never inherits the
/// source's edit lock, public link, metadata associations, or draft participants.
async fn require_duplicate_destination(
    conn: &mut sqlx::AnyConnection,
    principal: super::access::Principal<'_>,
    space_id: &str,
    folder_id: Option<&str>,
    parent_id: Option<&str>,
) -> Result<(), AppError> {
    let access = super::access::resolve_space_access(
        conn,
        principal,
        space_id,
        super::access::Purpose::Routine,
    )
    .await?;
    if !access.allows(super::access::Permission::CreateContent) {
        return Err(AppError::NotFound("Copy destination unavailable".into()));
    }
    super::folders::validate_document_folder_depth_on_connection(conn, space_id, folder_id).await?;
    validate_create_parent_with_reserved_nodes(conn, space_id, parent_id, folder_id, principal,2).await?;
    // The dedicated creation-parent checks can wait on ancestor row locks.
    let access = super::access::resolve_space_access(
        conn,
        principal,
        space_id,
        super::access::Purpose::Routine,
    )
    .await?;
    if !access.allows(super::access::Permission::CreateContent) {
        return Err(AppError::NotFound("Copy destination unavailable".into()));
    }
    Ok(())
}

async fn duplicate_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    Json(req): Json<DuplicateDocumentRequest>,
) -> Result<impl IntoResponse, AppError> {
    let principal = super::access::principal_from_auth(&auth)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    let source = sqlx::query("UPDATE documents SET title=title WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(&doc_id).bind(&space_id).execute(&mut *tx).await.map_err(AppError::internal)?;
    if source.rows_affected() != 1 {
        return Err(AppError::NotFound("Copy source unavailable".into()));
    }
    let page = super::access::resolve_page_access(
        &mut tx,
        principal,
        &space_id,
        &doc_id,
        super::access::PageOperation::View,
        super::access::Purpose::Routine,
    )
    .await?;
    let (mut title,mut icon,mut cover,content_path,folder_id,parent_id):(String,Option<String>,Option<String>,String,Option<String>,Option<String>)=sqlx::query_as("SELECT title,icon,cover_image,content_path,folder_id,parent_id FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(&doc_id).bind(&space_id).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    require_duplicate_destination(
        &mut tx,
        principal,
        &space_id,
        folder_id.as_deref(),
        parent_id.as_deref(),
    )
    .await?;
    let selected_draft=match req.draft_id {
        Some(id) if !id.trim().is_empty()=>Some(id),
        Some(_)=>return Err(AppError::BadRequest("Choose a draft to duplicate".into())),
        None=>sqlx::query_scalar::<_,String>("SELECT id FROM document_drafts WHERE document_id=$1 AND space_id=$2 AND owner_id=$3 ORDER BY updated_at DESC,id ASC LIMIT 1")
            .bind(&doc_id).bind(&space_id).bind(&auth.claims.sub).fetch_optional(&mut *tx).await.map_err(AppError::internal)?,
    };
    let content = if let Some(draft_id) = &selected_draft {
        super::access::resolve_draft_access(
            &mut tx,
            principal,
            &space_id,
            &doc_id,
            draft_id,
            super::access::DraftOperation::View,
            super::access::Purpose::Routine,
        )
        .await?;
        let locked = sqlx::query(
            "UPDATE document_drafts SET id=id WHERE id=$1 AND document_id=$2 AND space_id=$3",
        )
        .bind(draft_id)
        .bind(&doc_id)
        .bind(&space_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        if locked.rows_affected() != 1 {
            return Err(AppError::NotFound("Copy draft unavailable".into()));
        }
        super::access::resolve_draft_access(
            &mut tx,
            principal,
            &space_id,
            &doc_id,
            draft_id,
            super::access::DraftOperation::View,
            super::access::Purpose::Routine,
        )
        .await?;
        let (draft_title,draft_icon,draft_cover,body):(String,Option<String>,Option<String>,String)=sqlx::query_as("SELECT title,icon,cover_image,content FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3")
            .bind(draft_id).bind(&doc_id).bind(&space_id).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
        title = draft_title;
        icon = draft_icon;
        cover = draft_cover;
        body
    } else if page.published {
        storage::read_document(&content_path)
            .await
            .map_err(|error| AppError::Internal(format!("Failed to read copy source: {error}")))?
    } else {
        return Err(AppError::BadRequest(
            "Choose an accessible draft_id to duplicate this private page".into(),
        ));
    };
    let title = req
        .title
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("{title} (Copy)"));
    let new_id = Uuid::new_v4().to_string();
    let draft_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1")
        .bind(&space_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    // Revalidate after all possible row waits, before any storage side effects.
    let current_source = super::access::resolve_page_access(
        &mut tx,
        principal,
        &space_id,
        &doc_id,
        super::access::PageOperation::View,
        super::access::Purpose::Routine,
    )
    .await?;
    if selected_draft.is_none() && !current_source.published {
        return Err(AppError::Conflict(
            "Published copy source changed; select a draft and retry".into(),
        ));
    }
    if let Some(selected) = &selected_draft {
        super::access::resolve_draft_access(
            &mut tx,
            principal,
            &space_id,
            &doc_id,
            selected,
            super::access::DraftOperation::View,
            super::access::Purpose::Routine,
        )
        .await?;
    }
    require_duplicate_destination(
        &mut tx,
        principal,
        &space_id,
        folder_id.as_deref(),
        parent_id.as_deref(),
    )
    .await?;
    let frontmatter = document_storage_frontmatter(&title, icon.as_deref(), false, false);
    let storage_stage = storage::journal::stage_document(
        &mut tx,
        &space_id,
        &new_id,
        &draft_id,
        &now,
        &slug,
        &title,
        &frontmatter,
        &content,
    )
    .await?;
    let new_path = storage_stage.path.clone();
    let staged:Result<(),AppError>=async {
        let row=sqlx::query("INSERT INTO documents(id,space_id,folder_id,parent_id,title,icon,cover_image,content_path,created_by,last_updated_by,created_at,updated_at,human_identifier,human_identifier_key) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$9,$10,$10,NULL,NULL)")
            .bind(&new_id).bind(&space_id).bind(&folder_id).bind(&parent_id).bind(&title).bind(&icon).bind(&cover).bind(&new_path).bind(&auth.claims.sub).bind(&now).execute(&mut *tx).await.map_err(AppError::internal)?;
        if row.rows_affected()!=1 {return Err(AppError::Conflict("Copy page write did not apply".into()));}
        let row=sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,icon,cover_image,content,base_updated_at,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$9,$9)")
            .bind(&draft_id).bind(&new_id).bind(&space_id).bind(&auth.claims.sub).bind(&title).bind(&icon).bind(&cover).bind(&content).bind(&now).execute(&mut *tx).await.map_err(AppError::internal)?;
        if row.rows_affected()!=1 {return Err(AppError::Conflict("Copy draft write did not apply".into()));}
        let created:Option<(Option<String>,Option<String>)>=sqlx::query_as("SELECT folder_id,parent_id FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL AND published_at IS NULL AND public=0 AND locked=0 AND created_by=$3 AND human_identifier IS NULL AND human_identifier_key IS NULL AND maintenance_user_id IS NULL AND maintenance_group_id IS NULL")
            .bind(&new_id).bind(&space_id).bind(&auth.claims.sub).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
        if created!=Some((folder_id.clone(),parent_id.clone())) {return Err(AppError::Conflict("Copy placement or lifecycle changed; retry duplication".into()));}
        let draft:Option<String>=sqlx::query_scalar("SELECT id FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3 AND owner_id=$4")
            .bind(&draft_id).bind(&new_id).bind(&space_id).bind(&auth.claims.sub).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
        if draft.is_none() {return Err(AppError::Conflict("Copy draft ownership changed; retry duplication".into()));}
        let placement:Option<(Option<String>,Option<String>)>=sqlx::query_as("SELECT folder_id,parent_id FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL")
            .bind(&doc_id).bind(&space_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
        if placement!=Some((folder_id.clone(),parent_id.clone())) {return Err(AppError::Conflict("Copy source placement changed; retry duplication".into()));}
        let current_source=super::access::resolve_page_access(&mut tx,principal,&space_id,&doc_id,super::access::PageOperation::View,super::access::Purpose::Routine).await?;
    if selected_draft.is_none() && !current_source.published {return Err(AppError::Conflict("Published copy source changed; select a draft and retry".into()));}
        if let Some(selected)=&selected_draft {super::access::resolve_draft_access(&mut tx,principal,&space_id,&doc_id,selected,super::access::DraftOperation::View,super::access::Purpose::Routine).await?;}
        require_duplicate_destination(&mut tx,principal,&space_id,folder_id.as_deref(),parent_id.as_deref()).await?;
        Ok(())
    }.await;
    if let Err(error) = staged {
        match tx.rollback().await {
            Ok(()) => {
                tracing::info!(op_id=%storage_stage.op_id,"Duplicate rollback confirmed; private stage will be reconciled under space guard");
            }
            Err(rollback) => {
                tracing::warn!(%rollback,document_id=%new_id,"Duplicate rollback outcome uncertain; retain generated file for recovery")
            }
        }
        return Err(error);
    }
    // An uncertain commit retains stage+manifest until guarded receipt recovery.
    if let Err(commit_error) = tx.commit().await {
        if !storage::journal::confirm_commit(&state.db, &storage_stage)
            .await
            .unwrap_or(false)
        {
            return Err(AppError::internal(commit_error));
        }
    }
    let storage_complete = storage::journal::publish_committed(&state.db, &storage_stage).await;
    tracing::info!(action="document.duplicated",space_id=%space_id,source_document_id=%doc_id,document_id=%new_id,draft_id=%draft_id,user_id=%auth.claims.sub,"Duplicated document");
    Ok((
        if storage_complete {
            StatusCode::CREATED
        } else {
            StatusCode::ACCEPTED
        },
        Json(if storage_complete {
            serde_json::json!({"id":new_id,"draft_id":draft_id,"title":title})
        } else {
            serde_json::json!({"id":new_id,"draft_id":draft_id,"title":title,"storage_pending":true,"op_id":storage_stage.op_id})
        }),
    ))
}

async fn list_archived_documents(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    list_lifecycle_documents(&state, &auth, &space_id, false).await
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LifecycleDraftMetadata {
    id: String,
    owner_id: String,
    title: String,
    icon: Option<String>,
    cover_image: Option<String>,
    created_at: String,
    updated_at: String,
    base_updated_at: String,
}
type LifecycleDraftMetadataRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
);
impl From<LifecycleDraftMetadataRow> for LifecycleDraftMetadata {
    fn from(
        (id,owner_id,title,icon,cover_image,created_at,updated_at,base_updated_at):LifecycleDraftMetadataRow,
    ) -> Self {
        Self {
            id,
            owner_id,
            title,
            icon,
            cover_image,
            created_at,
            updated_at,
            base_updated_at,
        }
    }
}
async fn readable_lifecycle_metadata_draft(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    page: &str,
    trashed: bool,
) -> Result<Option<LifecycleDraftMetadata>, AppError> {
    let (operation, draft_operation) = lifecycle_metadata_operations(trashed);
    let access = super::access::resolve_page_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        page,
        operation,
        super::access::Purpose::Routine,
    )
    .await?;
    let row:Option<LifecycleDraftMetadataRow>=sqlx::query_as("SELECT dd.id,dd.owner_id,dd.title,dd.icon,dd.cover_image,dd.created_at,dd.updated_at,dd.base_updated_at FROM document_drafts dd JOIN documents d ON d.id=dd.document_id AND d.space_id=dd.space_id WHERE dd.document_id=$1 AND dd.space_id=$2 AND d.published_at IS NULL AND (dd.owner_id=$3 OR EXISTS(SELECT 1 FROM document_draft_access a WHERE a.draft_id=dd.id AND a.user_id=$3 AND a.role IN ('viewer','editor'))) ORDER BY CASE WHEN dd.owner_id=$3 THEN 0 ELSE 1 END,dd.updated_at DESC,dd.id ASC LIMIT 1")
        .bind(page).bind(space).bind(&access.space.actor_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let Some(row) = row else { return Ok(None) };
    let metadata = LifecycleDraftMetadata::from(row);
    let draft = super::access::resolve_draft_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        page,
        &metadata.id,
        draft_operation,
        super::access::Purpose::Routine,
    )
    .await?;
    if draft.page.published {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    Ok(Some(metadata))
}
async fn require_current_lifecycle_draft_metadata(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    page: &str,
    selected: &LifecycleDraftMetadata,
    trashed: bool,
) -> Result<(), AppError> {
    let current:Option<LifecycleDraftMetadataRow>=sqlx::query_as("SELECT dd.id,dd.owner_id,dd.title,dd.icon,dd.cover_image,dd.created_at,dd.updated_at,dd.base_updated_at FROM document_drafts dd JOIN documents d ON d.id=dd.document_id AND d.space_id=dd.space_id WHERE dd.id=$1 AND dd.document_id=$2 AND dd.space_id=$3 AND d.published_at IS NULL")
        .bind(&selected.id).bind(page).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if current.map(LifecycleDraftMetadata::from).as_ref() != Some(selected) {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    // The exact metadata tuple is compared before the final current selected
    // participation/ancestor proof. Another readable draft cannot substitute.
    let draft = super::access::resolve_draft_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        page,
        &selected.id,
        lifecycle_metadata_operations(trashed).1,
        super::access::Purpose::Routine,
    )
    .await?;
    if draft.page.published {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    Ok(())
}

fn lifecycle_metadata_operations(
    trashed: bool,
) -> (super::access::PageOperation, super::access::DraftOperation) {
    if trashed {
        (
            super::access::PageOperation::InspectTrashedMetadata,
            super::access::DraftOperation::InspectTrashedMetadata,
        )
    } else {
        (
            super::access::PageOperation::InspectArchivedMetadata,
            super::access::DraftOperation::InspectArchivedMetadata,
        )
    }
}

async fn lifecycle_projection(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    trashed: bool,
) -> Result<(Vec<DocumentResponse>, Vec<String>), AppError> {
    require_document_discovery_space_view(conn, auth, space).await?;
    let predicate = if trashed {
        "deleted_at IS NOT NULL"
    } else {
        "archived_at IS NOT NULL AND deleted_at IS NULL"
    };
    let ids: Vec<String> = sqlx::query_scalar(&format!(
        "SELECT id FROM documents WHERE space_id=$1 AND {predicate} ORDER BY updated_at DESC"
    ))
    .bind(space)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let (operation, _) = lifecycle_metadata_operations(trashed);
    let mut output = vec![];
    let mut chain = std::collections::BTreeSet::new();
    for id in ids {
        let access = match super::access::resolve_page_access(
            conn,
            super::access::principal_from_auth(auth)?,
            space,
            &id,
            operation,
            super::access::Purpose::Routine,
        )
        .await
        {
            Ok(access) => access,
            Err(AppError::NotFound(_)) => continue,
            Err(error) => return Err(error),
        };
        let mut doc = fetch_document_on_connection(conn, &id, Some(space)).await?;
        if doc.published_at.is_none() {
            let draft =
                match readable_lifecycle_metadata_draft(conn, auth, space, &id, trashed).await {
                    Ok(Some(draft)) => draft,
                    Ok(None) | Err(AppError::NotFound(_)) => continue,
                    Err(error) => return Err(error),
                };
            doc.title = draft.title;
            doc.icon = draft.icon;
            doc.cover_image = draft.cover_image;
            doc.draft_id = Some(draft.id);
        }
        chain.insert(id);
        chain.extend(access.ancestor_ids);
        output.push(doc);
    }
    Ok((output, chain.into_iter().collect()))
}

/// Internal equality witnesses, never response data or an authorization policy.
async fn lifecycle_metadata_source(
    conn: &mut sqlx::AnyConnection,
    actor: &str,
    space: &str,
    chain: &[String],
    trashed: bool,
) -> Result<serde_json::Value, AppError> {
    use super::maintenance::rows;
    let mut value = serde_json::Map::new();
    for (name,query,args) in [
        ("account","SELECT id,role,is_active,account_kind FROM users WHERE id=$1",vec![actor]),
        ("space","SELECT * FROM spaces WHERE id=$1",vec![space]),
        ("direct","SELECT * FROM space_members WHERE space_id=$1 AND user_id=$2 ORDER BY id",vec![space,actor]),
        ("block","SELECT * FROM space_user_blocks WHERE space_id=$1 AND user_id=$2",vec![space,actor]),
        ("memberships","SELECT * FROM group_members WHERE user_id=$1 ORDER BY group_id",vec![actor]),
        ("group_roles","SELECT p.* FROM space_group_permissions p JOIN group_members m ON m.group_id=p.group_id WHERE p.space_id=$1 AND m.user_id=$2 ORDER BY p.id",vec![space,actor]),
        ("global_roles","SELECT p.* FROM group_global_roles p JOIN group_members m ON m.group_id=p.group_id WHERE m.user_id=$1 ORDER BY p.group_id,p.role_id",vec![actor]),
        ("space_definitions","SELECT * FROM space_roles WHERE space_id=$1 ORDER BY id",vec![space]),
        ("global_definitions","SELECT r.* FROM global_roles r WHERE EXISTS(SELECT 1 FROM group_global_roles p JOIN group_members m ON m.group_id=p.group_id WHERE p.role_id=r.id AND m.user_id=$1) ORDER BY r.id",vec![actor]),
    ] { value.insert(name.into(),serde_json::json!(rows(conn,query,&args).await?)); }
    let predicate = if trashed {
        "deleted_at IS NOT NULL"
    } else {
        "archived_at IS NOT NULL AND deleted_at IS NULL"
    };
    value.insert("listing_ids".into(),serde_json::json!(rows(conn,&format!("SELECT id,updated_at FROM documents WHERE space_id=$1 AND {predicate} ORDER BY updated_at DESC"),&[space]).await?));
    let mut pages = vec![];
    for id in chain {
        pages.push(serde_json::json!({
            "page":rows(conn,"SELECT * FROM documents WHERE id=$1 AND space_id=$2",&[id,space]).await?,
            "policy":rows(conn,"SELECT * FROM document_access_policies WHERE document_id=$1 ORDER BY document_id",&[id]).await?,
            "allowlist":rows(conn,"SELECT * FROM document_access_allowlist WHERE document_id=$1 ORDER BY id",&[id]).await?,
            "allowlist_targets":rows(conn,"SELECT a.id,u.id AS user_id,g.id AS group_id FROM document_access_allowlist a LEFT JOIN users u ON u.id=a.user_id LEFT JOIN groups g ON g.id=a.group_id WHERE a.document_id=$1 ORDER BY a.id",&[id]).await?,
            "drafts":rows(conn,"SELECT * FROM document_drafts WHERE document_id=$1 ORDER BY id",&[id]).await?,
            "shares":rows(conn,"SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.document_id=$1 AND a.user_id=$2 ORDER BY a.draft_id",&[id,actor]).await?,
        }));
    }
    let grant_proofs=super::page_viewer_grants::capture_space_dependencies(conn,space).await?;
    value.insert("viewer_grants".into(),grant_proofs);
    value.insert("pages".into(), serde_json::json!(pages));
    Ok(serde_json::Value::Object(value))
}

async fn list_lifecycle_documents(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    trashed: bool,
) -> Result<Json<Vec<DocumentResponse>>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let (output, chain) = lifecycle_projection(&mut conn, auth, space, trashed).await?;
    let before =
        lifecycle_metadata_source(&mut conn, &auth.claims.sub, space, &chain, trashed).await?;
    #[cfg(test)]
    lifecycle_metadata_after_capture().await;
    let (current, current_chain) = lifecycle_projection(&mut conn, auth, space, trashed).await?;
    if serde_json::to_value(&output).map_err(AppError::internal)?
        != serde_json::to_value(&current).map_err(AppError::internal)?
        || chain != current_chain
    {
        return Err(AppError::NotFound(
            "Lifecycle metadata unavailable; refresh".into(),
        ));
    }
    #[cfg(test)]
    lifecycle_metadata_after_projection().await;
    if lifecycle_metadata_source(&mut conn, &auth.claims.sub, space, &chain, trashed).await?
        != before
    {
        return Err(AppError::NotFound(
            "Lifecycle metadata unavailable; refresh".into(),
        ));
    }
    // Inventory SQL can wait. Current target/ancestor and exact selected-draft
    // authorization follows every comparison; original credential proof is LAST.
    #[cfg(test)]
    lifecycle_metadata_after_source().await;
    require_document_discovery_space_view(&mut conn, auth, space).await?;
    let (operation, draft_operation) = lifecycle_metadata_operations(trashed);
    for doc in &output {
        let current = super::access::resolve_page_access(
            &mut conn,
            super::access::principal_from_auth(auth)?,
            space,
            &doc.id,
            operation,
            super::access::Purpose::Routine,
        )
        .await?;
        let mut expected_ancestors = Vec::new();
        let mut parent = doc.parent_id.clone();
        while let Some(id) = parent {
            let source = before["pages"]
                .as_array()
                .and_then(|pages| pages.iter().find(|p| p["page"][0]["id"] == id))
                .ok_or_else(|| {
                    AppError::NotFound("Lifecycle metadata unavailable; refresh".into())
                })?;
            expected_ancestors.push(id);
            parent = source["page"][0]["parent_id"].as_str().map(str::to_owned);
            if expected_ancestors.len() > chain.len() {
                return Err(AppError::NotFound(
                    "Lifecycle metadata unavailable; refresh".into(),
                ));
            }
        }
        if current.ancestor_ids != expected_ancestors
            || current.published != doc.published_at.is_some()
        {
            return Err(AppError::NotFound(
                "Lifecycle metadata unavailable; refresh".into(),
            ));
        }
        if let Some(draft_id) = &doc.draft_id {
            super::access::resolve_draft_access(
                &mut conn,
                super::access::principal_from_auth(auth)?,
                space,
                &doc.id,
                draft_id,
                draft_operation,
                super::access::Purpose::Routine,
            )
            .await?;
        }
    }
    #[cfg(test)]
    lifecycle_metadata_before_identity().await;
    super::access::require_current_human_read(&mut conn, auth).await?;
    Ok(Json(output))
}

#[cfg(test)]
tokio::task_local! {
    static LIFECYCLE_METADATA_AFTER_CAPTURE: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    static LIFECYCLE_METADATA_AFTER_PROJECTION: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    static LIFECYCLE_METADATA_AFTER_SOURCE: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    static LIFECYCLE_METADATA_BEFORE_IDENTITY: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
}
#[cfg(test)]
async fn lifecycle_metadata_after_capture() {
    if let Ok(Some(h)) = LIFECYCLE_METADATA_AFTER_CAPTURE.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}
#[cfg(test)]
async fn lifecycle_metadata_after_projection() {
    if let Ok(Some(h)) = LIFECYCLE_METADATA_AFTER_PROJECTION.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}
#[cfg(test)]
async fn lifecycle_metadata_after_source() {
    if let Ok(Some(h)) = LIFECYCLE_METADATA_AFTER_SOURCE.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}
#[cfg(test)]
async fn lifecycle_metadata_before_identity() {
    if let Ok(Some(h)) = LIFECYCLE_METADATA_BEFORE_IDENTITY.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}

async fn mirror_lifecycle_document(state: &AppState, doc_id: &str, lifecycle: &str) {
    let row: Option<(String, String, String, String, Option<String>, i32, i32)> = sqlx::query_as(
        "SELECT d.space_id, s.slug, d.title, d.content_path, d.icon, d.locked, d.public
         FROM documents d JOIN spaces s ON s.id = d.space_id WHERE d.id = $1",
    )
    .bind(doc_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    let Some((space_id, slug, title, content_path, icon, locked, public)) = row else {
        return;
    };
    let Ok(content) = storage::read_document(&content_path).await else {
        return;
    };
    let frontmatter = format!(
        "starlane_lifecycle: {lifecycle}\ndocument_id: {doc_id}\nspace_id: {space_id}\nupdated_at: {}\n{}",
        chrono::Utc::now().to_rfc3339(),
        document_storage_frontmatter(&title, icon.as_deref(), locked != 0, public != 0)
    );
    if let Err(error) =
        storage::write_lifecycle_document(&slug, lifecycle, doc_id, &title, &frontmatter, &content)
            .await
    {
        tracing::warn!(document_id = %doc_id, %error, "Failed to mirror lifecycle document");
    }
}

async fn remove_lifecycle_mirror(state: &AppState, doc_id: &str, lifecycle: &str) {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT s.slug, d.title FROM documents d JOIN spaces s ON s.id = d.space_id WHERE d.id = $1",
    )
    .bind(doc_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    if let Some((slug, title)) = row {
        let _ = storage::delete_lifecycle_document(&slug, lifecycle, doc_id, &title).await;
    }
}

async fn restore_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let committed = super::lifecycle::restore(&state, &auth, &space_id, &doc_id).await?;
    // Unsafe canonical reindex reads and lifecycle-mirror deletion await safe repair.
    crate::webhooks::dispatch::dispatch_event(
        &state.db,
        "document.restored",
        &committed.webhook,
        Some(&space_id),
    )
    .await;
    crate::audit::append_committed_move_action(&committed.audit).await;
    tracing::info!(action="document.restored",space_id=%space_id,document_id=%doc_id,user_id=%auth.claims.sub,"Restored document from trash");
    Ok(StatusCode::NO_CONTENT)
}

async fn archive_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let committed = super::lifecycle::archive(&state, &auth, &space_id, &doc_id).await?;
    if let Err(error) = super::lifecycle::remove_index(&state, &auth, &committed).await {
        tracing::warn!(document_id=%doc_id,?error,"Archive index removal deferred");
    }
    // Unsafe derived mirrors are deferred; all prior bytes remain retained.
    crate::webhooks::dispatch::dispatch_event(
        &state.db,
        "document.archived",
        &committed.webhook,
        Some(&space_id),
    )
    .await;
    crate::audit::append_committed_move_action(&committed.audit).await;
    tracing::info!(action="document.archived",space_id=%space_id,document_id=%doc_id,user_id=%auth.claims.sub,"Archived document");
    Ok(StatusCode::NO_CONTENT)
}

async fn unarchive_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let committed = super::lifecycle::unarchive(&state, &auth, &space_id, &doc_id).await?;
    // Reindexing and mirror removal await a safe derived-storage repair protocol.
    // Never read unpublished/reused canonical bytes or unlink old evidence here.
    crate::webhooks::dispatch::dispatch_event(
        &state.db,
        "document.unarchived",
        &committed.webhook,
        Some(&space_id),
    )
    .await;
    crate::audit::append_committed_move_action(&committed.audit).await;
    tracing::info!(action="document.unarchived",space_id=%space_id,document_id=%doc_id,user_id=%auth.claims.sub,"Unarchived document");
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
async fn permanently_delete_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<axum::response::Response, AppError> {
    let row:(String,Option<String>)=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE id=$1 AND space_id=$2").bind(&doc_id).bind(&space_id).fetch_one(&state.db).await.map_err(AppError::internal)?;
    permanently_delete_document_operation(state,auth,space_id,doc_id,uuid::Uuid::new_v4().to_string(),storage::attachment_erasure_plan::OriginalDeleteTarget{created_at:row.0,deleted_at:row.1}).await
}
async fn permanently_delete_document_with_headers(State(state):State<AppState>,auth:AuthUser,Path((space_id,doc_id)):Path<(String,String)>,headers:axum::http::HeaderMap,payload:Result<Json<storage::attachment_erasure_plan::OriginalDeleteRequest>,axum::extract::rejection::JsonRejection>)->Result<axum::response::Response,AppError>{
 let Json(payload)=payload.map_err(|_|storage::attachment_erasure_plan::target_review())?;payload.original_target.validate(true)?;
 let operation=storage::attachment_erasure_plan::operation(&headers)?.ok_or_else(storage::attachment_erasure_plan::target_review)?;permanently_delete_document_operation(state,auth,space_id,doc_id,operation,payload.original_target).await
}
async fn permanently_delete_document_operation(state:AppState,auth:AuthUser,space_id:String,doc_id:String,operation:String,original_target:storage::attachment_erasure_plan::OriginalDeleteTarget)->Result<axum::response::Response,AppError>{
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    if storage::attachment_erasure_plan::retry(&mut tx,&auth,&space_id,&doc_id,&operation,&original_target).await?{super::access::require_current_human_preference_write(&mut tx,&auth).await?;tx.commit().await.map_err(AppError::internal)?;return Ok(([("x-starlane-delete-operation",operation.as_str())],StatusCode::NO_CONTENT).into_response());}
    storage::attachment_erasure_plan::verify_original_target(&mut tx,&space_id,&doc_id,&original_target).await?;
    #[cfg(test)]
    rest_delete_after_guard().await;
    let prepared = super::document_deletion::prepare(
        &mut tx,
        &auth,
        &space_id,
        &doc_id,
        Some(super::access::PageOperation::DeleteTrashed),
    )
    .await?;
    let plan=storage::attachment_erasure_plan::prepare(&mut tx,&auth,&prepared,operation,&original_target).await?;
    super::document_deletion::delete_prepared(&mut tx, &auth, &prepared).await?;
    storage::attachment_erasure_plan::verify(&mut tx,&plan).await?;
    super::document_deletion::verify_authority(&mut tx, &auth, &prepared).await?;
    #[cfg(test)]
    rest_delete_before_identity().await;
    super::access::require_current_human_preference_write(&mut tx, &auth).await?;
    tx.commit().await.map_err(AppError::internal)?;
    // Physical cleanup is deferred. A retained blob has no live page/asset association.
    tracing::info!(action = "document.deleted", space_id = %space_id, document_id = %doc_id, user_id = %auth.claims.sub, permanent = true, "Permanently deleted document");
    Ok(([("x-starlane-delete-operation",plan.operation_id.as_str())],StatusCode::NO_CONTENT).into_response())
}

async fn reindex_document(state: &AppState, doc_id: &str) {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT space_id, title, content_path, COALESCE(public_slug, '') FROM documents WHERE id = $1 AND deleted_at IS NULL AND archived_at IS NULL",
    )
    .bind(doc_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    if let Some((space_id, title, content_path, _)) = row {
        let body = storage::read_document(&content_path)
            .await
            .unwrap_or_default();
        if let Err(error) = state.search_index.upsert(DocEntry {
            doc_id: doc_id.to_string(),
            space_id,
            title,
            body,
            tags: String::new(),
        }) {
            tracing::warn!("Failed to re-index restored document {}: {}", doc_id, error);
        }
    }
}

/// POST /api/v1/spaces/{space_id}/documents/{doc_id}/move
async fn move_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    Json(req): Json<MoveDocumentRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.target_space_id != space_id {
        // Review-only cross-space intent: resolve actual source/destination before
        // the safety barrier, without mutation locks, writes or storage work.
        let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
        require_scoped_page_operation(
            &mut conn,
            &auth,
            &space_id,
            &doc_id,
            super::access::PageOperation::Move,
        )
        .await?;
        let target = super::access::resolve_space_access(
            &mut conn,
            super::access::principal_from_auth(&auth)?,
            &req.target_space_id,
            super::access::Purpose::Routine,
        )
        .await?;
        if !target.allows(super::access::Permission::MoveContent) {
            return Err(placement_unavailable());
        }
        placement_folder_chain(&mut conn, &req.target_space_id, req.folder_id.as_deref()).await?;
        let ancestors = placement_ancestors(
            &mut conn,
            &req.target_space_id,
            req.parent_id.as_deref(),
            req.folder_id.as_deref(),
            &std::collections::BTreeSet::new(),
        )
        .await?;
        if !ancestors.is_empty() {
            let targets=ancestors.iter().map(|page|page.id.clone()).collect();
            super::access::resolve_page_batch(&mut conn,super::access::principal_from_auth(&auth)?,&req.target_space_id,&targets,super::access::PageOperation::Move).await?.require_all()?;
        }
        require_scoped_page_operation(
            &mut conn,
            &auth,
            &space_id,
            &doc_id,
            super::access::PageOperation::Move,
        )
        .await?;
        let current_target = super::access::resolve_space_access(
            &mut conn,
            super::access::principal_from_auth(&auth)?,
            &req.target_space_id,
            super::access::Purpose::Routine,
        )
        .await?;
        if !current_target.allows(super::access::Permission::MoveContent) {
            return Err(placement_unavailable());
        }
        reject_unreviewed_cross_space_move(&space_id, &req.target_space_id)?;
    }
    let mut tx = begin_scoped_page_mutation(
        &state,
        &auth,
        &space_id,
        &doc_id,
        super::access::PageOperation::Move,
    )
    .await?;
    let document_ids = apply_document_placement(
        &mut tx,
        &auth,
        &space_id,
        &doc_id,
        PlacementChange::Set(req.folder_id),
        PlacementChange::Set(req.parent_id),
        PlacementActorStamp::CurrentActor,
    )
    .await?;
    tx.commit().await.map_err(AppError::internal)?;
    // Same-space placement leaves indexed title/body/space/tags unchanged.
    // Avoid indexing unpublished canonical shells as a hierarchy side effect.

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

    tracing::info!(action = "document.moved", space_id = %space_id, target_space_id = %req.target_space_id, document_id = %doc_id, moved_document_count = document_ids.len(), user_id = %auth.claims.sub, "Moved document");

    Ok(StatusCode::NO_CONTENT)
}

pub(crate) fn reject_unreviewed_cross_space_move(
    source: &str,
    target: &str,
) -> Result<(), AppError> {
    if source != target {
        return Err(AppError::Conflict(
            "Cross-space moves require a reviewed metadata and access preview before relocation. Keep this item in its current space until reviewed moves are available.".into(),
        ));
    }
    Ok(())
}

pub(crate) async fn require_move_target_space(
    state: &AppState,
    space_id: &str,
    user_id: &str,
) -> Result<(), AppError> {
    // Missing and inaccessible destinations share the same response. Admin bypass in the
    // role resolver does not establish that a requested destination actually exists.
    match super::routes::require_space_role(state, space_id, user_id, &["owner", "editor"]).await {
        Ok(_) => {}
        Err(AppError::Forbidden(_)) | Err(AppError::NotFound(_)) => {
            return Err(AppError::NotFound("Move destination not found".into()))
        }
        Err(error) => return Err(error),
    }
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM spaces WHERE id = $1")
        .bind(space_id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;
    if exists.is_none() {
        return Err(AppError::NotFound("Move destination not found".into()));
    }
    Ok(())
}

pub(crate) async fn prepare_space_storage_relocation(
    db: &AnyPool,
    source_space_id: &str,
    target_space_id: &str,
    document_ids: &[String],
) -> Result<SpaceStorageRelocation, AppError> {
    let source_slug: (String,) = sqlx::query_as("SELECT slug FROM spaces WHERE id = $1")
        .bind(source_space_id)
        .fetch_one(db)
        .await
        .map_err(AppError::internal)?;
    let target_slug: (String,) = sqlx::query_as("SELECT slug FROM spaces WHERE id = $1")
        .bind(target_space_id)
        .fetch_one(db)
        .await
        .map_err(AppError::internal)?;
    let mut relocation = SpaceStorageRelocation {
        document_paths: HashMap::new(),
        attachment_paths: HashMap::new(),
        staged_files: Vec::new(),
    };

    if let Err(error) = populate_space_storage_relocation(
        db,
        source_space_id,
        &source_slug.0,
        &target_slug.0,
        document_ids,
        &mut relocation,
    )
    .await
    {
        storage::rollback_staged_file_copies(&relocation.staged_files).await;
        return Err(error);
    }
    Ok(relocation)
}

async fn populate_space_storage_relocation(
    db: &AnyPool,
    source_space_id: &str,
    source_slug: &str,
    target_slug: &str,
    document_ids: &[String],
    relocation: &mut SpaceStorageRelocation,
) -> Result<(), AppError> {
    for document_id in document_ids {
        let (title, content_path, archived_at, deleted_at): (
            String,
            String,
            Option<String>,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT title, content_path, archived_at, deleted_at FROM documents WHERE id = $1 AND space_id = $2",
        )
        .bind(document_id)
        .bind(source_space_id)
        .fetch_one(db)
        .await
        .map_err(AppError::internal)?;
        let filename = storage::document_filename(&title, document_id);
        let next_path = format!("spaces/{target_slug}/docs/{filename}");
        stage_relocation_file(relocation, &content_path, &next_path, true).await?;
        relocation
            .document_paths
            .insert(document_id.clone(), next_path);

        let lifecycle = if deleted_at.is_some() {
            Some("trashed")
        } else if archived_at.is_some() {
            Some("archived")
        } else {
            None
        };
        if let Some(lifecycle) = lifecycle {
            let source = format!("spaces/{source_slug}/{lifecycle}/{filename}");
            let destination = format!("spaces/{target_slug}/{lifecycle}/{filename}");
            stage_relocation_file(relocation, &source, &destination, false).await?;
        }

        let drafts: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, title FROM document_drafts WHERE document_id = $1 AND space_id = $2",
        )
        .bind(document_id)
        .bind(source_space_id)
        .fetch_all(db)
        .await
        .map_err(AppError::internal)?;
        for (draft_id, draft_title) in drafts {
            let filename = storage::document_filename(&draft_title, &draft_id);
            let source = format!("spaces/{source_slug}/drafts/{filename}");
            let destination = format!("spaces/{target_slug}/drafts/{filename}");
            stage_relocation_file(relocation, &source, &destination, false).await?;
        }

        let attachments: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, storage_path FROM attachments WHERE document_id = $1 AND space_id = $2",
        )
        .bind(document_id)
        .bind(source_space_id)
        .fetch_all(db)
        .await
        .map_err(AppError::internal)?;
        for (attachment_id, storage_path) in attachments {
            let filename = std::path::Path::new(&storage_path)
                .file_name()
                .and_then(|value| value.to_str())
                .ok_or_else(|| {
                    AppError::Internal("Attachment storage path has no filename".into())
                })?;
            let destination = format!("spaces/{target_slug}/attachments/{document_id}/{filename}");
            stage_relocation_file(relocation, &storage_path, &destination, true).await?;
            relocation
                .attachment_paths
                .insert(attachment_id, destination);
        }
    }
    Ok(())
}

async fn stage_relocation_file(
    relocation: &mut SpaceStorageRelocation,
    source: &str,
    destination: &str,
    required: bool,
) -> Result<(), AppError> {
    match storage::stage_owned_file_copy(source, destination, required).await {
        Ok(Some(copy)) => relocation.staged_files.push(copy),
        Ok(None) => {}
        Err(error) => {
            return Err(AppError::Internal(format!(
                "Failed to stage moved page storage: {error}"
            )));
        }
    }
    Ok(())
}

pub(crate) async fn update_relocated_storage_records(
    transaction: &mut Transaction<'_, Any>,
    target_space_id: &str,
    relocation: &SpaceStorageRelocation,
) -> Result<(), AppError> {
    for (document_id, content_path) in &relocation.document_paths {
        sqlx::query("UPDATE documents SET content_path = $1 WHERE id = $2")
            .bind(content_path)
            .bind(document_id)
            .execute(&mut **transaction)
            .await
            .map_err(AppError::internal)?;
        sqlx::query("UPDATE document_drafts SET space_id = $1 WHERE document_id = $2")
            .bind(target_space_id)
            .bind(document_id)
            .execute(&mut **transaction)
            .await
            .map_err(AppError::internal)?;
    }
    for (attachment_id, storage_path) in &relocation.attachment_paths {
        sqlx::query("UPDATE attachments SET space_id = $1, storage_path = $2 WHERE id = $3")
            .bind(target_space_id)
            .bind(storage_path)
            .bind(attachment_id)
            .execute(&mut **transaction)
            .await
            .map_err(AppError::internal)?;
    }
    Ok(())
}

#[cfg(test)]
tokio::task_local! {
    static CONTENT_FINAL_SOURCE_CHANGE: String;
}

pub(crate) async fn require_current_content_draft_source(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    doc: &str,
    draft: &ReadableDiscoveryDraft,
) -> Result<(), AppError> {
    let row: Option<(String, Option<String>, Option<String>, String)> = sqlx::query_as(
        "SELECT dd.title,dd.icon,dd.cover_image,dd.content FROM document_drafts dd \
         JOIN documents d ON d.id=dd.document_id AND d.space_id=dd.space_id \
         WHERE dd.id=$1 AND dd.document_id=$2 AND dd.space_id=$3 AND d.published_at IS NULL",
    )
    .bind(&draft.id)
    .bind(doc)
    .bind(space)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    if row.as_ref() != Some(&(draft.title.clone(), draft.icon.clone(), draft.cover_image.clone(), draft.content.clone())) {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    // Another readable draft cannot authorize the body selected earlier.
    require_discovery_draft_view(conn, auth, space, doc, &draft.id).await
}

/// GET /api/v1/spaces/{space_id}/documents/{doc_id}/content
async fn get_content(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;

    // Capture presentation, placement and source in ONE raw row; never attach
    // a later row digest to an earlier title/body selection.
    let raw=super::document_deletion::rows(&mut conn,"SELECT * FROM documents WHERE id=$1 AND space_id=$2",&[&doc_id,&space_id]).await.map_err(AppError::internal)?;
    if raw.len()!=1 {return Err(AppError::NotFound("Target unavailable".into()));}
    let row=&raw[0];
    let doc=(super::draft_save::string(row,"id")?,super::draft_save::string(row,"title")?,super::draft_save::string(row,"content_path")?,super::draft_save::optional(row,"published_at")?,super::draft_save::string(row,"updated_at")?);
    if doc.3.is_some() {super::shared_pages::published_view(&mut conn,&auth,&space_id,&doc_id).await?;} else {require_document_discovery_page_view(&mut conn,&auth,&space_id,&doc_id).await?;}
    let mut selected_draft = None;
    let published_row=if doc.3.is_some() {Some(row.clone())}else{None};
    let published_proofs=if published_row.is_some() {
        let page=super::access::resolve_page_access(&mut conn,super::access::principal_from_auth(&auth)?,&space_id,&doc_id,super::access::PageOperation::GrantedPublishedView,super::access::Purpose::Routine).await?;
        Some((super::published_source::capture_source(&mut conn,&auth,&space_id,&doc_id,&page.ancestor_ids).await?,page.direct_viewer_grant))
    }else{None};
    let (title, content) = if doc.3.is_none() {
        let draft = readable_discovery_draft(&mut conn, &auth, &space_id, &doc_id).await?;
        let content = discovery_draft_content(&draft)?;
        require_discovery_draft_view(&mut conn, &auth, &space_id, &doc_id, &draft.id).await?;
        let title = draft.title.clone();
        selected_draft = Some(draft);
        (title, content)
    } else {
        let content = storage::read_document(&doc.2)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to read document: {}", e)))?;
        require_current_document_read_source(
            &mut conn,
            &space_id,
            &doc_id,
            doc.3.as_deref().unwrap(),
            &doc.2,
            &doc.4,
        )
        .await?;
        (doc.1, content)
    };

    if published_row.is_some() {super::shared_pages::published_view(&mut conn,&auth,&space_id,&doc_id).await?;} else {require_document_discovery_page_view(&mut conn,&auth,&space_id,&doc_id).await?;}
    #[cfg(test)]
    super::shared_pages::reader_wait().await;
    #[cfg(test)]
    if let Ok(change) = CONTENT_FINAL_SOURCE_CHANGE.try_with(Clone::clone) {
        sqlx::query(&change).execute(&mut *conn).await.map_err(AppError::internal)?;
    }
    if let Some(draft) = selected_draft.as_ref() {
        require_current_content_draft_source(&mut conn, &auth, &space_id, &doc_id, draft).await?;
    }
    if let Some(published) = doc.3.as_deref() {
        require_current_document_read_source(
            &mut conn, &space_id, &doc_id, published, &doc.2, &doc.4,
        )
        .await?;
    }

    let published_source_digest=if let Some(row)=published_row.as_ref() {
        if super::published_source::selected(&mut conn,&space_id,&doc_id).await?!=*row {return Err(super::published_source::conflict());}
        // Re-read the original path, never pair a later row with earlier body.
        if storage::read_document(&doc.2).await.map_err(AppError::internal)?!=content {return Err(super::published_source::conflict());}
        Some(super::published_source::digest(row,content.as_bytes())?)
    }else{None};
    if let Some((proofs,_))=published_proofs.as_ref() {super::published_source::verify_source(&mut conn,proofs).await?;}
    if published_row.is_some() {super::shared_pages::published_view(&mut conn,&auth,&space_id,&doc_id).await?;} else {require_document_discovery_page_view(&mut conn,&auth,&space_id,&doc_id).await?;}
    if let Some(row)=published_row.as_ref() {if super::published_source::selected(&mut conn,&space_id,&doc_id).await?!=*row {return Err(super::published_source::conflict());}}
    if let Some((_,grant))=published_proofs.as_ref() {if super::shared_pages::published_view(&mut conn,&auth,&space_id,&doc_id).await?.direct_viewer_grant!=*grant {return Err(super::published_source::conflict());}}
    super::access::require_current_human_read(&mut conn,&auth).await?;
    if published_row.is_some() && std::fs::read(storage::data_dir().join(&doc.2)).map_err(AppError::internal)?!=content.as_bytes() {return Err(super::published_source::conflict());}
    let published_metadata = published_row.as_ref().map(|row| -> Result<PublishedDocumentMetadata, AppError> {
        Ok(PublishedDocumentMetadata {
            title: super::draft_save::string(row, "title")?,
            icon: super::draft_save::optional(row, "icon")?,
            cover_image: super::draft_save::optional(row, "cover_image")?,
        })
    }).transpose()?;
    Ok(Json(DocumentContentResponse {
        published_metadata,
        published_source_digest,
        id: doc.0,
        title,
        content,
    }))
}

/// PUT /api/v1/spaces/{space_id}/documents/{doc_id}/content
async fn update_content(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, _doc_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    Err(AppError::Conflict(
        "Page content must be changed through a draft".into(),
    ))
}

/// POST /api/v1/spaces/{space_id}/documents/{doc_id}/lock
async fn lock_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    set_document_lock(&state, &auth, &space_id, &doc_id, true).await?;
    Ok(StatusCode::NO_CONTENT)
}
/// POST /api/v1/spaces/{space_id}/documents/{doc_id}/unlock
async fn unlock_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    set_document_lock(&state, &auth, &space_id, &doc_id, false).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn set_document_lock(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    doc: &str,
    locked: bool,
) -> Result<(), AppError> {
    let operation = super::access::PageOperation::ManageLock;
    let mut tx = begin_scoped_page_mutation(state, auth, space, doc, operation).await?;
    let before:(i64,Option<String>)=sqlx::query_as("SELECT locked,locked_by FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(doc).bind(space).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    let changed = (before.0 != 0) != locked;
    if changed {
        let updated=sqlx::query("UPDATE documents SET locked=$1,locked_by=$2,updated_at=$3 WHERE id=$4 AND space_id=$5 AND archived_at IS NULL AND deleted_at IS NULL")
            .bind(if locked {1_i64}else{0_i64}).bind(if locked {Some(auth.claims.sub.as_str())}else{None}).bind(chrono::Utc::now().to_rfc3339()).bind(doc).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
        if updated.rows_affected() != 1 {
            return Err(placement_unavailable());
        }
    }
    let expected = if changed {
        (
            if locked { 1_i64 } else { 0_i64 },
            if locked {
                Some(auth.claims.sub.clone())
            } else {
                None
            },
        )
    } else {
        before
    };
    let actual:Option<(i64,Option<String>)>=sqlx::query_as("SELECT locked,locked_by FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(doc).bind(space).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    if actual != Some(expected) {
        return Err(AppError::Conflict("Page lock state changed; retry".into()));
    }
    require_scoped_page_operation(&mut tx, auth, space, doc, operation).await?;
    tx.commit().await.map_err(AppError::internal)?;
    if changed {
        if let Err(error) =
            notify_document_lock_subscribers(state, space, doc, &auth.claims.sub, locked).await
        {
            tracing::warn!(?error,"Committed page lock notification failed");
        }
    }
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        if locked {
            "document.lock"
        } else {
            "document.unlock"
        },
        Some("document"),
        Some(doc),
    )
    .await;
    Ok(())
}

#[derive(Clone, PartialEq, Eq, sqlx::FromRow)]
struct PublicLinkSource {
    folder_id: Option<String>,
    parent_id: Option<String>,
    title: String,
    icon: Option<String>,
    cover_image: Option<String>,
    locked: i64,
    locked_by: Option<String>,
    public: i64,
    public_slug: Option<String>,
    content_path: String,
    updated_at: String,
    published_at: Option<String>,
    archived_at: Option<String>,
    created_by: String,
    last_updated_by: String,
}
async fn public_link_source(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    doc: &str,
) -> Result<PublicLinkSource, AppError> {
    sqlx::query_as("SELECT folder_id,parent_id,title,icon,cover_image,locked,locked_by,public,public_slug,content_path,updated_at,published_at,archived_at,created_by,last_updated_by FROM documents WHERE id=$1 AND space_id=$2 AND deleted_at IS NULL")
        .bind(doc).bind(space).fetch_optional(conn).await.map_err(AppError::internal)?.ok_or_else(||AppError::NotFound("Target unavailable".into()))
}

async fn require_public_link_operation(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    doc: &str,
    enable: bool,
) -> Result<super::access::PageAccess, AppError> {
    super::access::resolve_page_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        doc,
        if enable {
            super::access::PageOperation::PublicLinkEnable
        } else {
            super::access::PageOperation::PublicLinkDisable
        },
        super::access::Purpose::Routine,
    )
    .await
}
#[derive(Clone)]
struct PublicLinkProof {
    query: String,
    parameters: Vec<String>,
    rows: Vec<serde_json::Value>,
}
async fn public_link_raw(
    conn: &mut sqlx::AnyConnection,
    query: &str,
    args: &[&str],
) -> Result<Vec<serde_json::Value>, AppError> {
    super::document_deletion::rows(conn, query, args)
        .await
        .map_err(AppError::internal)
}
async fn public_link_witness(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    doc: &str,
    ancestors: &[String],
) -> Result<Vec<PublicLinkProof>, AppError> {
    let mut specs: Vec<(String, Vec<String>)> = vec![
        (
            "SELECT * FROM documents WHERE space_id=$1".into(),
            vec![space.into()],
        ),
        ("SELECT id,space_id,parent_id FROM documents".into(), vec![]),
        (
            "SELECT id,role,is_active,account_kind FROM users WHERE id=$1".into(),
            vec![auth.claims.sub.clone()],
        ),
    ];
    for table in [
        "spaces",
        "space_members",
        "space_group_permissions",
        "space_user_blocks",
        "space_roles",
    ] {
        specs.push((
            format!(
                "SELECT * FROM {table} WHERE {}=$1",
                if table == "spaces" { "id" } else { "space_id" }
            ),
            vec![space.into()],
        ));
    }
    for table in [
        "groups",
        "group_members",
        "group_global_roles",
        "global_roles",
    ] {
        specs.push((format!("SELECT * FROM {table}"), vec![]));
    }
    for id in std::iter::once(doc).chain(ancestors.iter().map(String::as_str)) {
        for table in [
            "document_drafts",
            "document_access_policies",
            "document_access_allowlist",
        ] {
            specs.push((
                format!("SELECT * FROM {table} WHERE document_id=$1"),
                vec![id.into()],
            ));
        }
        specs.push(("SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.document_id=$1".into(),vec![id.into()]));
    }
    let mut result = Vec::new();
    for (query, parameters) in specs {
        let args: Vec<&str> = parameters.iter().map(String::as_str).collect();
        let rows = public_link_raw(conn, &query, &args).await?;
        result.push(PublicLinkProof {
            query,
            parameters,
            rows,
        });
    }
    Ok(result)
}
fn public_link_changed() -> AppError {
    AppError::Conflict("Public link source changed; retry".into())
}

async fn change_public_link(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    doc: &str,
    enable: bool,
) -> Result<Option<String>, AppError> {
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[space]).await?;
    let locked=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND deleted_at IS NULL AND (archived_at IS NULL OR $3=1)").bind(doc).bind(space).bind(if enable{0_i64}else{1_i64}).execute(&mut *tx).await.map_err(AppError::internal)?;
    if locked.rows_affected() != 1 {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    let authority = require_public_link_operation(&mut tx, auth, space, doc, enable).await?;
    let source = public_link_source(&mut tx, space, doc).await?;
    if enable && source.published_at.is_none() {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    if enable && !public_pages_enabled() {
        return Err(AppError::BadRequest("Public pages are disabled".into()));
    }
    let mut expected =
        public_link_witness(&mut tx, auth, space, doc, &authority.ancestor_ids).await?;
    let slug = enable.then(|| Uuid::new_v4().to_string().replace('-', "")[..16].to_string());
    let updated = chrono::Utc::now().to_rfc3339();
    let page = expected[0]
        .rows
        .iter_mut()
        .find(|p| p["id"] == doc)
        .ok_or_else(|| AppError::NotFound("Target unavailable".into()))?;
    page["public"] = serde_json::json!(if enable { 1 } else { 0 });
    page["public_slug"] = serde_json::json!(slug);
    page["updated_at"] = serde_json::json!(updated);
    expected[0]
        .rows
        .sort_by_cached_key(serde_json::Value::to_string);
    let applied=sqlx::query("UPDATE documents SET public=$1,public_slug=$2,updated_at=$3 WHERE id=$4 AND space_id=$5 AND deleted_at IS NULL AND (archived_at IS NULL OR $6=1) AND ($6=1 OR published_at IS NOT NULL)").bind(if enable{1_i64}else{0_i64}).bind(&slug).bind(&updated).bind(doc).bind(space).bind(if enable{0_i64}else{1_i64}).execute(&mut *tx).await.map_err(AppError::internal)?;
    if applied.rows_affected() != 1 {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    // A fixed route action attributed to the actual authorized human, not a synthetic capability.
    let action = if enable {
        "document.public_link_enable"
    } else {
        "document.public_link_disable"
    };
    let audit_id = Uuid::new_v4().to_string();
    let created_at:String=sqlx::query_scalar("INSERT INTO audit_log(id,user_id,action,target_type,target_id) VALUES($1,$2,$3,'document',$4) RETURNING created_at").bind(&audit_id).bind(&auth.claims.sub).bind(action).bind(doc).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    let audit = serde_json::json!({"id":audit_id,"user_id":auth.claims.sub,"ip_address":null,"action":action,"target_type":"document","target_id":doc,"created_at":created_at});
    let stored =
        public_link_raw(&mut tx, "SELECT * FROM audit_log WHERE id=$1", &[&audit_id]).await?;
    let expected_audit = serde_json::json!({"id":audit_id,"user_id":auth.claims.sub,"ip_address":null,"action":action,"target_type":"document","target_id":doc,"created_at":created_at,"actor_kind":null,"actor_id":null,"before_value":null,"after_value":null});
    if stored != vec![expected_audit] {
        return Err(public_link_changed());
    }
    let mut expected_source = source;
    expected_source.public = if enable { 1 } else { 0 };
    expected_source.public_slug = slug.clone();
    expected_source.updated_at = updated;
    if public_link_source(&mut tx, space, doc).await? != expected_source {
        return Err(public_link_changed());
    }
    for proof in expected {
        let args: Vec<&str> = proof.parameters.iter().map(String::as_str).collect();
        if public_link_raw(&mut tx, &proof.query, &args).await? != proof.rows {
            return Err(public_link_changed());
        }
    }
    // All source/audit SQL precedes final real link-policy and original credential proof.
    require_public_link_operation(&mut tx, auth, space, doc, enable).await?;
    if enable && !public_pages_enabled() {
        return Err(AppError::BadRequest("Public pages are disabled".into()));
    }
    #[cfg(test)]
    if let Ok(Some(hook)) = PUBLIC_LINK_BEFORE_IDENTITY.try_with(|slot| slot.borrow_mut().take()) {
        hook.await
    }
    super::access::require_current_human_preference_write(&mut tx, auth).await?;
    // Pure deployment policy is also rechecked after the final credential SQL wait.
    if enable && !public_pages_enabled() {
        return Err(AppError::BadRequest("Public pages are disabled".into()));
    }
    #[cfg(test)]
    if PUBLIC_LINK_ROLLBACK.try_with(|v| *v).unwrap_or(false) {
        tx.rollback().await.map_err(AppError::internal)?;
        return Err(AppError::Internal(
            "Public link commit did not succeed".into(),
        ));
    }
    tx.commit().await.map_err(AppError::internal)?;
    crate::audit::append_committed_move_action(&audit).await;
    Ok(slug)
}
#[cfg(test)]
tokio::task_local! {
 static PUBLIC_LINK_BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
 static PUBLIC_LINK_ROLLBACK:bool;
}
/// POST /api/v1/spaces/{space_id}/documents/{doc_id}/publish
async fn publish_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space, doc)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let slug = change_public_link(&state, &auth, &space, &doc, true)
        .await?
        .ok_or_else(|| AppError::Internal("Enabled link omitted slug".into()))?;
    tracing::info!(action="document.public_link_enabled",space_id=%space,document_id=%doc,user_id=%auth.claims.sub,public_slug=%slug,"Enabled public document link");
    Ok(Json(serde_json::json!({"public_slug":slug})))
}
/// POST /api/v1/spaces/{space_id}/documents/{doc_id}/unpublish
async fn unpublish_document(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space, doc)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    change_public_link(&state, &auth, &space, &doc, false).await?;
    tracing::info!(action="document.public_link_disabled",space_id=%space,document_id=%doc,user_id=%auth.claims.sub,"Disabled public document link");
    Ok(StatusCode::NO_CONTENT)
}

// Visibility governs authenticated discovery, not explicit anonymous page links.
// Private spaces may intentionally share a page; changing visibility does not revoke it.
// The deployment switch and document lifecycle are checked on every anonymous read.
pub(crate) fn public_pages_enabled() -> bool {
    public_pages_enabled_value(std::env::var("ALLOW_PUBLIC_PAGES").ok().as_deref())
}

fn public_pages_enabled_value(value: Option<&str>) -> bool {
    value.is_none_or(|value| !value.trim().eq_ignore_ascii_case("false"))
}

/// GET /api/v1/p/{slug} (public -- no auth)
async fn get_public_page(
    State(state): State<AppState>,
    Path(slug): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let response = read_public_page(&state, &slug, public_pages_enabled()).await?;
    if !public_pages_enabled() {
        return Err(AppError::NotFound("Page not found".into()));
    }
    Ok(response)
}

#[cfg(test)]
thread_local! {
    static PUBLIC_READ_AFTER_IO: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>>> = std::cell::RefCell::new(None);
}

#[derive(PartialEq, Eq, sqlx::FromRow)]
struct PublicReadSource {
    maintenance_user_id: Option<String>,
    maintenance_group_id: Option<String>,
    id: String,
    space_id: String,
    title: String,
    icon: Option<String>,
    cover_image: Option<String>,
    content_path: String,
    updated_at: String,
    published_at: String,
    author: String,
    space_name: String,
    space_accent_color: Option<String>,
}
async fn public_read_source(
    conn: &mut sqlx::AnyConnection,
    slug: &str,
) -> Result<Option<PublicReadSource>, AppError> {
    sqlx::query_as(
        "SELECT d.maintenance_user_id, d.maintenance_group_id, d.id, d.space_id, d.title, d.icon, d.cover_image, d.content_path, \
         d.updated_at, d.published_at, u.display_name AS author, \
         s.name AS space_name, s.accent_color AS space_accent_color \
         FROM documents d JOIN users u ON d.created_by = u.id \
         JOIN spaces s ON d.space_id = s.id \
         WHERE d.public_slug = $1 AND d.public = 1 AND d.published_at IS NOT NULL \
         AND d.archived_at IS NULL AND d.deleted_at IS NULL",
    )
    .bind(slug)
    .fetch_optional(conn)
    .await
    .map_err(AppError::internal)
}
async fn read_public_page(
    state: &AppState,
    slug: &str,
    enabled: bool,
) -> Result<Json<PublicPageResponse>, AppError> {
    if !enabled {
        return Err(AppError::NotFound("Page not found".into()));
    }
    // Autocommit reads must observe revocation committed while file I/O waits.
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let source = public_read_source(&mut conn, slug)
        .await?
        .ok_or_else(|| AppError::NotFound("Page not found".into()))?;
    super::access::require_anonymous_page_chain(&mut conn, &source.space_id, &source.id)
        .await
        .map_err(|_| AppError::NotFound("Page not found".into()))?;
    let content = storage::read_document(&source.content_path)
        .await
        .map_err(|_| AppError::NotFound("Page not found".into()))?;
    #[cfg(test)]
    {
        let hook = PUBLIC_READ_AFTER_IO.with(|slot| slot.borrow_mut().take());
        if let Some(hook) = hook { hook.await; }
    }
    super::access::require_anonymous_page_chain(&mut conn, &source.space_id, &source.id)
        .await
        .map_err(|_| AppError::NotFound("Page not found".into()))?;
    if public_read_source(&mut conn, slug).await?.as_ref() != Some(&source) {
        return Err(AppError::NotFound("Page not found".into()));
    }
    // No later query/I/O follows this link/lifecycle/source proof. This does not
    // attest immutable bytes while legacy writers overwrite files before SQL.
    Ok(Json(PublicPageResponse {
        title: source.title,
        icon: source.icon,
        cover_image: source.cover_image,
        content,
        space_name: source.space_name,
        space_accent_color: source.space_accent_color,
        author: source.author,
        updated_at: source.updated_at,
    }))
}

// ============================================================
// Helpers
// ============================================================

async fn fetch_document(state: &AppState, doc_id: &str) -> Result<DocumentResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    fetch_document_on_connection(&mut conn, doc_id, None).await
}
async fn fetch_document_on_connection(
    conn: &mut sqlx::AnyConnection,
    doc_id: &str,
    space_id: Option<&str>,
) -> Result<DocumentResponse, AppError> {
    let row = sqlx::query("SELECT * FROM documents WHERE id=$1 AND ($2 IS NULL OR space_id=$2)")
        .bind(doc_id)
        .bind(space_id)
        .fetch_optional(conn)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::BadRequest("Document not found".into()))?;
    Ok(DocumentResponse {
        page_access_source: None,
        id: row.try_get("id").map_err(AppError::internal)?,
        space_id: row.try_get("space_id").map_err(AppError::internal)?,
        folder_id: row.try_get("folder_id").map_err(AppError::internal)?,
        parent_id: row.try_get("parent_id").map_err(AppError::internal)?,
        title: row.try_get("title").map_err(AppError::internal)?,
        icon: row.try_get("icon").map_err(AppError::internal)?,
        cover_image: row.try_get("cover_image").map_err(AppError::internal)?,
        locked_by: row.try_get("locked_by").map_err(AppError::internal)?,
        public_slug: row.try_get("public_slug").map_err(AppError::internal)?,
        created_by: row.try_get("created_by").map_err(AppError::internal)?,
        last_updated_by: row.try_get("last_updated_by").map_err(AppError::internal)?,
        created_at: row.try_get("created_at").map_err(AppError::internal)?,
        deleted_at: row.try_get("deleted_at").map_err(AppError::internal)?,
        updated_at: row.try_get("updated_at").map_err(AppError::internal)?,
        published_at: row.try_get("published_at").map_err(AppError::internal)?,
        maintenance_user_id: row
            .try_get("maintenance_user_id")
            .map_err(AppError::internal)?,
        maintenance_group_id: row
            .try_get("maintenance_group_id")
            .map_err(AppError::internal)?,
        human_identifier: row
            .try_get("human_identifier")
            .map_err(AppError::internal)?,
        locked: row
            .try_get::<i32, _>("locked")
            .map_err(AppError::internal)?
            != 0,
        public: row
            .try_get::<i32, _>("public")
            .map_err(AppError::internal)?
            != 0,
        identifier_edit_allowed: None,
        maintenance_edit_allowed: None,
        page_edit_allowed: None,
        draft_id: None,
        workflow_template_id: None,
        workflow_template_name: None,
        workflow_stage_id: None,
        workflow_stage_name: None,
        workflow_status: None,
    })
}

/// Creation placement is resolved on the guarded transaction, including every
/// ancestor. Attribution alone never grants private-page participation.
pub(crate) async fn validate_create_parent_on_connection(
    conn: &mut sqlx::AnyConnection,
    space_id: &str,
    parent_id: Option<&str>,
    folder_id: Option<&str>,
    principal: super::access::Principal<'_>,
) -> Result<(), AppError> {
    validate_create_parent_with_reserved_nodes(conn,space_id,parent_id,folder_id,principal,1).await
}
async fn validate_create_parent_with_reserved_nodes(
    conn:&mut sqlx::AnyConnection,
    space_id:&str,
    parent_id:Option<&str>,
    folder_id:Option<&str>,
    principal:super::access::Principal<'_>,
    reserved_nodes:usize,
)->Result<(),AppError> {
    let mut next = parent_id.map(str::to_owned);
    let mut ancestors = std::collections::BTreeMap::new();
    while let Some(id) = next {
        if ancestors.contains_key(&id) {
            return Err(AppError::BadRequest(
                "Page hierarchy contains a cycle".into(),
            ));
        }
        if ancestors.len() >= super::access::MAX_PAGE_OPERATION_NODES.saturating_sub(reserved_nodes) {
            return Err(AppError::BadRequest("Page operation exceeds the 10000-node traversal budget".into()));
        }
        let parent: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT folder_id, parent_id FROM documents WHERE id = $1 AND space_id = $2 AND deleted_at IS NULL AND archived_at IS NULL",
        ).bind(&id).bind(space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        let (parent_folder, ancestor) =
            parent.ok_or_else(|| AppError::NotFound("Parent page unavailable".into()))?;
        if parent_folder.as_deref() != folder_id {
            return Err(AppError::BadRequest(
                "Child pages must stay in the same folder as their parent".into(),
            ));
        }
        ancestors.insert(id, (parent_folder, ancestor.clone()));
        next = ancestor;
    }
    for id in ancestors.keys() {
        let locked = sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(id).bind(space_id).execute(&mut *conn).await.map_err(AppError::internal)?;
        if locked.rows_affected() != 1 {
            return Err(AppError::NotFound("Parent page unavailable".into()));
        }
        let actual: Option<(Option<String>, Option<String>)> = sqlx::query_as("SELECT folder_id,parent_id FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(id).bind(space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        if actual.as_ref() != ancestors.get(id) {
            return Err(AppError::Conflict(
                "Parent placement changed; retry creation".into(),
            ));
        }
    }
    // A legacy hierarchy writer may have completed while a page lock waited.
    // Reject any changed edge rather than acquiring a new ancestor out of order.
    for (id, expected) in &ancestors {
        let actual: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT folder_id,parent_id FROM documents WHERE id=$1 AND space_id=$2 AND deleted_at IS NULL AND archived_at IS NULL",
        ).bind(id).bind(space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        if actual.as_ref() != Some(expected) {
            return Err(AppError::Conflict(
                "Parent placement changed; retry creation".into(),
            ));
        }
    }
    let targets=ancestors.keys().cloned().collect();
    if !ancestors.is_empty() {
        super::access::resolve_page_batch(conn,principal,space_id,&targets,super::access::PageOperation::Edit).await?.require_all()?;
    }
    Ok(())
}

pub(crate) async fn cleanup_moved_document_metadata(
    db: &AnyPool,
    target_space_id: &str,
    document_ids: &[String],
) -> Result<(), AppError> {
    for doc_id in document_ids {
        sqlx::query(
            "DELETE FROM doc_tags WHERE document_id = $1 \
             AND tag_id NOT IN (SELECT id FROM tags WHERE space_id = $2)",
        )
        .bind(doc_id)
        .bind(target_space_id)
        .execute(db)
        .await
        .map_err(AppError::internal)?;

        sqlx::query(
            "DELETE FROM property_values WHERE document_id = $1 \
             AND property_id NOT IN (SELECT id FROM properties_schema WHERE space_id = $2)",
        )
        .bind(doc_id)
        .bind(target_space_id)
        .execute(db)
        .await
        .map_err(AppError::internal)?;

        sqlx::query(
            "DELETE FROM document_workflows WHERE document_id = $1 \
             AND workflow_template_id NOT IN (SELECT id FROM workflow_templates WHERE space_id = $2)",
        )
        .bind(doc_id)
        .bind(target_space_id)
        .execute(db)
        .await
        .map_err(AppError::internal)?;
    }
    Ok(())
}

pub(crate) async fn reindex_documents(state: &AppState, document_ids: &[String]) {
    for doc_id in document_ids {
        let Ok((space_id, content_path, title)) = sqlx::query_as::<_, (String, String, String)>(
            "SELECT space_id, content_path, title FROM documents WHERE id = $1",
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

pub(crate) async fn check_not_locked(
    state: &AppState,
    doc_id: &str,
    user_id: &str,
) -> Result<(), AppError> {
    let locked: Option<(i32, Option<String>)> =
        sqlx::query_as("SELECT locked, locked_by FROM documents WHERE id = $1")
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

async fn notify_document_lock_subscribers(
    state: &AppState,
    space_id: &str,
    document_id: &str,
    actor_id: &str,
    locked: bool,
) -> Result<(), AppError> {
    let actor_name: Option<String> = sqlx::query_scalar(
        "SELECT COALESCE(NULLIF(display_name,''),email) FROM users WHERE id=$1 AND is_active=1",
    )
    .bind(actor_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;
    let Some(actor_name) = actor_name else {
        return Ok(());
    };

    let subscriber_ids: Vec<(String,)> = sqlx::query_as(
        "SELECT ds.user_id FROM doc_subscriptions ds \
         JOIN users u ON u.id = ds.user_id \
         WHERE ds.document_id = $1 AND ds.user_id != $2 AND u.is_active = 1",
    )
    .bind(document_id)
    .bind(actor_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    for (recipient_id,) in subscriber_ids {
        let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
        let Some((_, document_title)) =
            readable_notification_page_title(&mut conn, space_id, document_id, &recipient_id)
                .await?
        else {
            continue;
        };
        drop(conn);
        let (notification_type, title, body) =
            document_lock_notification_copy(&document_title, &actor_name, locked);
        crate::notifications::notify(
            &state.db,
            &recipient_id,
            notification_type,
            &title,
            Some(&body),
            Some(actor_id),
            Some("document"),
            Some(document_id),
        )
        .await;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) type NotificationTitleReadHook =
    Box<dyn FnOnce() -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> + Send>;

/// Trusted delivery recipient, distinct from HTTP authentication. Returns only an
/// actual readable title and the same current typed permission evidence.
pub(crate) async fn readable_notification_page_title(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    doc: &str,
    user: &str,
) -> Result<Option<(super::access::SpaceAccess, String)>, AppError> {
    readable_notification_page_title_inner(
        conn,
        space,
        doc,
        user,
        #[cfg(test)]
        None,
        #[cfg(test)]
        None,
    )
    .await
}

pub(crate) async fn readable_notification_page_title_inner(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    doc: &str,
    user: &str,
    #[cfg(test)] after_load: Option<NotificationTitleReadHook>,
    #[cfg(test)] mut after_source: Option<NotificationTitleReadHook>,
) -> Result<Option<(super::access::SpaceAccess, String)>, AppError> {
    use super::access::{
        resolve_draft_access, resolve_page_access, DraftOperation, PageOperation, Principal,
        Purpose,
    };
    let principal = Principal::User {
        user_id: user,
        session_id: None,
        token_expires_at: None,
    };
    let initial = match resolve_page_access(
        conn,
        principal,
        space,
        doc,
        PageOperation::View,
        Purpose::Routine,
    )
    .await
    {
        Ok(page) => page,
        Err(AppError::NotFound(_)) | Err(AppError::Unauthorized(_)) => return Ok(None),
        Err(error) => return Err(error),
    };
    let source:Option<(Option<String>,String,String)>=sqlx::query_as("SELECT published_at,title,updated_at FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(doc).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let Some(source) = source else {
        return Ok(None);
    };
    if initial.published != source.0.is_some() {
        return Ok(None);
    }
    let draft: Option<(String, String, String, String)> = if initial.published {
        None
    } else {
        sqlx::query_as("SELECT d.id,d.title,d.updated_at,d.owner_id FROM document_drafts d WHERE d.document_id=$1 AND d.space_id=$2 AND (d.owner_id=$3 OR EXISTS(SELECT 1 FROM document_draft_access a WHERE a.draft_id=d.id AND a.user_id=$3 AND a.role IN ('viewer','editor'))) ORDER BY CASE WHEN d.owner_id=$3 THEN 0 ELSE 1 END,d.updated_at DESC,d.id ASC LIMIT 1")
            .bind(doc).bind(space).bind(user).fetch_optional(&mut *conn).await.map_err(AppError::internal)?
    };
    if !initial.published && draft.is_none() {
        return Ok(None);
    }
    if let Some(draft) = draft.as_ref() {
        match resolve_draft_access(
            conn,
            principal,
            space,
            doc,
            &draft.0,
            DraftOperation::View,
            Purpose::Routine,
        )
        .await
        {
            Ok(_) => {}
            Err(AppError::NotFound(_)) | Err(AppError::Unauthorized(_)) => return Ok(None),
            Err(error) => return Err(error),
        }
    }
    #[cfg(test)]
    if let Some(hook) = after_load {
        hook().await;
    }
    let current = match resolve_page_access(
        conn,
        principal,
        space,
        doc,
        PageOperation::View,
        Purpose::Routine,
    )
    .await
    {
        Ok(page) => page,
        Err(AppError::NotFound(_)) | Err(AppError::Unauthorized(_)) => return Ok(None),
        Err(error) => return Err(error),
    };
    if current.published != initial.published {
        return Ok(None);
    }
    let now:Option<(Option<String>,String,String)>=sqlx::query_as("SELECT published_at,title,updated_at FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(doc).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if now.as_ref() != Some(&source) {
        return Ok(None);
    }
    let title = if let Some(draft) = draft.as_ref() {
        let now:Option<(String,String,String,String)>=sqlx::query_as("SELECT id,title,updated_at,owner_id FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3")
            .bind(&draft.0).bind(doc).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        if now.as_ref() != Some(&draft) {
            return Ok(None);
        }
        // LAST exact participation check: another still-readable draft cannot
        // authorize the title of this selected draft after its share is revoked.
        match resolve_draft_access(
            conn,
            principal,
            space,
            doc,
            &draft.0,
            DraftOperation::View,
            Purpose::Routine,
        )
        .await
        {
            Ok(_) => {}
            Err(AppError::NotFound(_)) | Err(AppError::Unauthorized(_)) => return Ok(None),
            Err(error) => return Err(error),
        }
        draft.1.clone()
    } else {
        source.1.clone()
    };
    #[cfg(test)]
    if let Some(hook) = after_source.take() {
        hook().await;
    }
    // LAST statement binds current activity and the exact acknowledged source.
    // Typed PageView/DraftView remain the authority; this predicate grants none.
    let exact: i64 = if let Some(draft) = draft.as_ref() {
        sqlx::query_scalar("SELECT COUNT(*) FROM users u JOIN documents d ON d.id=$2 AND d.space_id=$3 JOIN document_drafts r ON r.id=$7 AND r.document_id=d.id AND r.space_id=d.space_id WHERE u.id=$1 AND u.is_active=1 AND d.archived_at IS NULL AND d.deleted_at IS NULL AND d.published_at IS NULL AND d.title=$4 AND d.updated_at=$5 AND r.title=$6 AND r.updated_at=$8 AND r.owner_id=$9 AND (r.owner_id=u.id OR EXISTS(SELECT 1 FROM document_draft_access a WHERE a.draft_id=r.id AND a.user_id=u.id AND a.role IN ('viewer','editor')))")
            .bind(user).bind(doc).bind(space).bind(&source.1).bind(&source.2).bind(&draft.1).bind(&draft.0).bind(&draft.2).bind(&draft.3).fetch_one(conn).await.map_err(AppError::internal)?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM users u JOIN documents d ON d.id=$2 AND d.space_id=$3 WHERE u.id=$1 AND u.is_active=1 AND d.archived_at IS NULL AND d.deleted_at IS NULL AND d.published_at=$4 AND d.title=$5 AND d.updated_at=$6")
            .bind(user).bind(doc).bind(space).bind(&source.0).bind(&source.1).bind(&source.2).fetch_one(conn).await.map_err(AppError::internal)?
    };
    if exact != 1 {
        return Ok(None);
    }
    Ok(Some((current.space, title)))
}

fn document_lock_notification_copy(
    document_title: &str,
    actor_name: &str,
    locked: bool,
) -> (&'static str, String, String) {
    if locked {
        (
            "doc_locked",
            format!("Document locked: {document_title}"),
            format!("{actor_name} locked {document_title}."),
        )
    } else {
        (
            "doc_unlocked",
            format!("Document unlocked: {document_title}"),
            format!("{actor_name} unlocked {document_title}."),
        )
    }
}

pub(crate) async fn notify_new_document_mentions(
    state: &AppState,
    space_id: &str,
    document_id: &str,
    actor_id: &str,
    document_title: &str,
    mentioned_user_ids: &BTreeSet<String>,
) -> Result<(), sqlx::Error> {
    if mentioned_user_ids.is_empty() {
        return Ok(());
    }

    let eligible_user_ids: BTreeSet<String> = sqlx::query_as(
        "SELECT u.id FROM users u \
         LEFT JOIN space_members sm ON sm.user_id = u.id AND sm.space_id = $1 \
         WHERE u.is_active = 1 AND (u.role = 'admin' OR sm.user_id IS NOT NULL)",
    )
    .bind(space_id)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|(user_id,): (String,)| user_id)
    .collect();

    for recipient_id in mentioned_user_ids
        .iter()
        .filter(|recipient_id| recipient_id.as_str() != actor_id)
        .filter(|recipient_id| eligible_user_ids.contains(*recipient_id))
    {
        crate::notifications::notify(
            &state.db,
            recipient_id,
            "mention",
            &format!("You were mentioned on {document_title}"),
            Some(&format!("Someone mentioned you on {document_title}.")),
            Some(actor_id),
            Some("document"),
            Some(document_id),
        )
        .await;
    }

    Ok(())
}

pub(crate) fn portable_mention_user_ids(content: &str) -> BTreeSet<String> {
    let mut user_ids = BTreeSet::new();
    let mut remaining = content;

    while let Some(start) = remaining.find("@[") {
        let after_open = &remaining[start + 2..];
        let Some(display_end) = after_open.find("](") else {
            break;
        };
        let after_display = &after_open[display_end + 2..];
        let Some(id_end) = after_display.find(')') else {
            break;
        };
        let candidate = &after_display[..id_end];
        if (1..=128).contains(&candidate.len())
            && candidate
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            user_ids.insert(candidate.to_string());
        }
        remaining = &after_display[id_end + 1..];
    }

    user_ids
}

pub(crate) fn newly_added_portable_mention_user_ids(
    previous_content: &str,
    current_content: &str,
) -> BTreeSet<String> {
    let previous_ids = portable_mention_user_ids(previous_content);
    portable_mention_user_ids(current_content)
        .difference(&previous_ids)
        .cloned()
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{
        get_document, get_content, readable_discovery_draft, require_document_discovery_page_view, require_discovery_draft_view,
        document_lock_notification_copy, document_storage_frontmatter, duplicate_document, AppError,
        list_documents, list_starred_documents, newly_added_portable_mention_user_ids,
        portable_mention_user_ids, star_document, DuplicateDocumentRequest, ListDocumentsQuery,
        DEFAULT_NEW_DOCUMENT_CONTENT,
    };
    use crate::auth::jwt::{Claims, TokenType};
    use crate::auth::middleware::AuthUser;
    use crate::AppState;
    use axum::extract::{Path, Query, State};
    use axum::response::IntoResponse;
    use axum::Json;
    use sqlx::any::AnyPoolOptions;

    async fn placement_state() -> AppState {
        let state = discovery_test_state().await;
        for folder in ["placement-one", "placement-two"] {
            sqlx::query("INSERT INTO folders(id,space_id,name) VALUES($1,'space-a',$1)").bind(folder).execute(&state.db).await.unwrap();
        }
        sqlx::query("UPDATE documents SET folder_id='placement-one' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        state
    }
    async fn placement_update(
        state: &AppState,
        user: &str,
        doc: &str,
        payload: serde_json::Value,
    ) -> Result<(), AppError> {
        super::update_document(
            State(state.clone()),
            discovery_auth(user),
            Path(("space-a".into(), doc.into())),
            Json(serde_json::from_value(payload).unwrap()),
        )
        .await
        .map(|_| ())
    }
    async fn placement_child(state: &AppState, id: &str, parent: &str) {
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,folder_id,parent_id,created_by,last_updated_by,published_at) VALUES($1,'space-a',$1,'','placement-one',$2,'owner-a','owner-a','published')").bind(id).bind(parent).execute(&state.db).await.unwrap();
    }
    #[tokio::test]
    async fn placement_wrong_space_missing_and_lock_routes_leave_entire_database_and_files_unchanged(
    ) {
        let state = placement_state().await;
        let slug = format!("placement-{}", uuid::Uuid::new_v4());
        let path =
            crate::storage::write_document(&slug, "target-a", "Existing", "", "Keep saved work")
                .await
                .unwrap();
        sqlx::query("UPDATE documents SET content_path=$1 WHERE id='target-a'")
            .bind(path)
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        let files = move_file_snapshot(&slug);
        for doc in ["target-b", "missing"] {
            for payload in [
                serde_json::json!({"folder_id":"placement-two"}),
                serde_json::json!({"parent_id":"target-a"}),
                serde_json::json!({"title":"No canonical edit"}),
            ] {
                assert!(matches!(
                    placement_update(&state, "editor-a", doc, payload).await,
                    Err(AppError::NotFound(_))
                ));
            }
            for locked in [true, false] {
                assert!(matches!(
                    super::set_document_lock(
                        &state,
                        &discovery_auth("owner-a"),
                        "space-a",
                        doc,
                        locked
                    )
                    .await,
                    Err(AppError::NotFound(_))
                ));
            }
        }
        assert_eq!(move_database_snapshot(&state).await, before);
        assert_eq!(move_file_snapshot(&slug), files);
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn placement_invalid_parent_is_atomic_and_folder_only_keeps_parent_consistency() {
        let state = placement_state().await;
        placement_child(&state, "placement-child", "target-a").await;
        let before = move_database_snapshot(&state).await;
        for parent in ["target-a", "placement-child", "target-b", "missing-parent"] {
            assert!(placement_update(
                &state,
                "editor-a",
                "target-a",
                serde_json::json!({"folder_id":"placement-two","parent_id":parent})
            )
            .await
            .is_err());
            assert_eq!(move_database_snapshot(&state).await, before);
        }
        assert!(matches!(
            placement_update(
                &state,
                "editor-a",
                "placement-child",
                serde_json::json!({"folder_id":"placement-two"})
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn placement_folder_change_propagates_entire_subtree_and_null_means_unchanged() {
        let state = placement_state().await;
        placement_child(&state, "placement-child", "target-a").await;
        placement_child(&state, "placement-grandchild", "placement-child").await;
        placement_update(
            &state,
            "editor-a",
            "target-a",
            serde_json::json!({"folder_id":"placement-two"}),
        )
        .await
        .unwrap();
        let rows: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT id,folder_id,parent_id FROM documents WHERE space_id='space-a' ORDER BY id",
        )
        .fetch_all(&state.db)
        .await
        .unwrap();
        assert_eq!(rows.len(), 3);
        assert!(rows
            .iter()
            .all(|row| row.1.as_deref() == Some("placement-two")));
        placement_update(
            &state,
            "editor-a",
            "placement-child",
            serde_json::json!({"folder_id":null,"parent_id":null}),
        )
        .await
        .unwrap();
        let after: Vec<(String, Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT id,folder_id,parent_id FROM documents WHERE space_id='space-a' ORDER BY id",
        )
        .fetch_all(&state.db)
        .await
        .unwrap();
        assert_eq!(after, rows);
    }
    #[tokio::test]
    async fn placement_accepts_deep_subtrees_and_hidden_or_inactive_descendants_fail_closed() {
        let state = placement_state().await;
        placement_child(&state, "placement-child", "target-a").await;
        placement_child(&state, "placement-grandchild", "placement-child").await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,folder_id,created_by,last_updated_by,published_at) VALUES('destination','space-a','Destination','','placement-one','owner-a','owner-a','published')").execute(&state.db).await.unwrap();
        placement_update(&state,"editor-a","target-a",serde_json::json!({"parent_id":"destination"})).await.unwrap();
        let parent:String=sqlx::query_scalar("SELECT parent_id FROM documents WHERE id='target-a'").fetch_one(&state.db).await.unwrap();
        assert_eq!(parent,"destination");
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='placement-grandchild'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(placement_update(
            &state,
            "editor-a",
            "target-a",
            serde_json::json!({"folder_id":"placement-two"})
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
        sqlx::query("UPDATE documents SET published_at='published',archived_at='archived' WHERE id='placement-grandchild'").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(placement_update(
            &state,
            "editor-a",
            "target-a",
            serde_json::json!({"folder_id":"placement-two"})
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn placement_foreign_child_edges_and_inconsistent_subtree_folders_are_not_repaired_silently(
    ) {
        let state = placement_state().await;
        sqlx::query("UPDATE documents SET parent_id='target-a' WHERE id='target-b'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(matches!(
            placement_update(
                &state,
                "editor-a",
                "target-a",
                serde_json::json!({"folder_id":"placement-two"})
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        assert_eq!(move_database_snapshot(&state).await, before);
        sqlx::query("UPDATE documents SET parent_id=NULL WHERE id='target-b'")
            .execute(&state.db)
            .await
            .unwrap();
        placement_child(&state, "placement-child", "target-a").await;
        sqlx::query("UPDATE documents SET folder_id='placement-two' WHERE id='placement-child'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(placement_update(
            &state,
            "editor-a",
            "target-a",
            serde_json::json!({"folder_id":"placement-two"})
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn placement_late_sql_failure_and_current_actor_revocation_roll_back_all_rows() {
        for trigger in ["CREATE TRIGGER placement_failure BEFORE UPDATE OF folder_id ON documents WHEN NEW.id='target-a' BEGIN SELECT RAISE(ABORT,'last-write'); END","CREATE TRIGGER placement_failure AFTER UPDATE OF folder_id ON documents WHEN NEW.id='target-a' BEGIN DELETE FROM sessions WHERE user_id='editor-a'; END"] {
            let state=placement_state().await;placement_child(&state,"placement-child","target-a").await;
            sqlx::query(trigger).execute(&state.db).await.unwrap();let before=move_database_snapshot(&state).await;
            assert!(placement_update(&state,"editor-a","target-a",serde_json::json!({"folder_id":"placement-two"})).await.is_err());assert_eq!(move_database_snapshot(&state).await,before);
        }
    }
    #[tokio::test]
    async fn placement_current_private_editor_and_lock_owner_are_required() {
        let state = placement_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('placement-draft','target-a','space-a','owner-a','Private','old')").execute(&state.db).await.unwrap();
        assert!(placement_update(
            &state,
            "editor-a",
            "target-a",
            serde_json::json!({"folder_id":"placement-two"})
        )
        .await
        .is_err());
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('placement-draft','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        assert!(placement_update(
            &state,
            "editor-a",
            "target-a",
            serde_json::json!({"folder_id":"placement-two"})
        )
        .await
        .is_err());
        sqlx::query("UPDATE document_draft_access SET role='editor'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(matches!(
            placement_update(
                &state,
                "editor-a",
                "target-a",
                serde_json::json!({"folder_id":"placement-two"})
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        sqlx::query("UPDATE documents SET locked_by='editor-a' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        placement_update(
            &state,
            "editor-a",
            "target-a",
            serde_json::json!({"folder_id":"placement-two"}),
        )
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn placement_move_only_capability_preserves_key_write_ceiling_without_edit_or_read() {
        let state = placement_state().await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('placement-mover','space-a','Mover','[\"move_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='placement-mover' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('placement-key','editor-a','Mover','hash','prefix','[\"write\"]')").execute(&state.db).await.unwrap();
        let key_auth = || {
            let mut a = discovery_auth("editor-a");
            a.claims.sid = "api-key:placement-key".into();
            a.api_key_scopes = Some(vec!["write".into()]);
            a
        };
        super::update_document(
            State(state.clone()),
            key_auth(),
            Path(("space-a".into(), "target-a".into())),
            Json(serde_json::from_value(serde_json::json!({"folder_id":"placement-two"})).unwrap()),
        )
        .await
        .unwrap();
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::update_document(
            State(state),
            key_auth(),
            Path(("space-a".into(), "target-a".into())),
            Json(serde_json::from_value(serde_json::json!({"folder_id":"placement-one"})).unwrap())
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn placement_owner_lock_management_ignores_existing_lock_but_requires_private_editor() {
        let state = placement_state().await;
        sqlx::query("UPDATE documents SET locked=1,locked_by='editor-a' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        super::set_document_lock(
            &state,
            &discovery_auth("owner-a"),
            "space-a",
            "target-a",
            false,
        )
        .await
        .unwrap();
        assert!(super::set_document_lock(
            &state,
            &discovery_auth("editor-a"),
            "space-a",
            "target-a",
            true
        )
        .await
        .is_err());
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('lock-draft','target-a','space-a','editor-a','Private','old')").execute(&state.db).await.unwrap();
        assert!(super::set_document_lock(
            &state,
            &discovery_auth("owner-a"),
            "space-a",
            "target-a",
            true
        )
        .await
        .is_err());
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('lock-draft','owner-a','viewer','editor-a')").execute(&state.db).await.unwrap();
        assert!(super::set_document_lock(
            &state,
            &discovery_auth("owner-a"),
            "space-a",
            "target-a",
            true
        )
        .await
        .is_err());
        sqlx::query("UPDATE document_draft_access SET role='editor'")
            .execute(&state.db)
            .await
            .unwrap();
        super::set_document_lock(
            &state,
            &discovery_auth("owner-a"),
            "space-a",
            "target-a",
            true,
        )
        .await
        .unwrap();
        super::set_document_lock(
            &state,
            &discovery_auth("owner-a"),
            "space-a",
            "target-a",
            true,
        )
        .await
        .unwrap();
        super::set_document_lock(
            &state,
            &discovery_auth("owner-a"),
            "space-a",
            "target-a",
            false,
        )
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn placement_lock_late_credential_revocation_rolls_back_lock_and_audit() {
        let state = placement_state().await;
        sqlx::query("CREATE TRIGGER lock_revoke AFTER UPDATE OF locked ON documents BEGIN DELETE FROM sessions WHERE user_id='owner-a'; END").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(matches!(
            super::set_document_lock(
                &state,
                &discovery_auth("owner-a"),
                "space-a",
                "target-a",
                true
            )
            .await,
            Err(AppError::Unauthorized(_))
        ));
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn placement_private_lock_notifications_use_exact_recipient_draft_title() {
        let state = placement_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL,title='CANONICAL UNRECOGNIZED' WHERE id='target-a'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('lock-owner-draft','target-a','space-a','owner-a','Readable shared title','old')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('lock-owner-draft','viewer-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        for user in ["viewer-a", "editor-a"] {
            sqlx::query("INSERT INTO doc_subscriptions(document_id,user_id) VALUES('target-a',$1)")
                .bind(user)
                .execute(&state.db)
                .await
                .unwrap();
        }
        super::set_document_lock(
            &state,
            &discovery_auth("owner-a"),
            "space-a",
            "target-a",
            true,
        )
        .await
        .unwrap();
        let rows: Vec<(String, String, String)> =
            sqlx::query_as("SELECT user_id,title,body FROM notifications")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "viewer-a");
        assert!(rows[0].1.contains("Readable shared title"));
        assert!(!rows[0].1.contains("CANONICAL"));
        assert!(!rows[0].2.contains("CANONICAL"));
    }

    #[tokio::test]
    async fn placement_http_write_only_key_can_move_and_manage_lock_but_read_only_cannot() {
        use tower::ServiceExt;
        let state = placement_state().await;
        let key = "placement-http-key";
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('placement-http','owner-a','Write',$1,'prefix','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key(key)).execute(&state.db).await.unwrap();
        let app = super::router().with_state(state.clone());
        let request = |method: &str, path: &str, body: &str| {
            axum::http::Request::builder()
                .method(method)
                .uri(path)
                .header("X-API-Key", key)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap()
        };
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "PUT",
                    "/spaces/space-a/documents/target-a",
                    r#"{"folder_id":"placement-two"}"#
                ))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::NO_CONTENT
        );
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    "/spaces/space-a/documents/target-a/lock",
                    ""
                ))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::NO_CONTENT
        );
        assert_eq!(
            app.clone()
                .oneshot(request(
                    "POST",
                    "/spaces/space-a/documents/target-a/unlock",
                    ""
                ))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::NO_CONTENT
        );
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='placement-http'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            app.oneshot(request(
                "POST",
                "/spaces/space-a/documents/target-a/lock",
                ""
            ))
            .await
            .unwrap()
            .status(),
            axum::http::StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    async fn placement_lifecycle_block_and_after_wait_account_revocation_remain_denied() {
        let state = placement_state().await;
        for field in ["archived_at", "deleted_at"] {
            sqlx::query(&format!(
                "UPDATE documents SET {field}='inactive' WHERE id='target-a'"
            ))
            .execute(&state.db)
            .await
            .unwrap();
            assert!(matches!(
                placement_update(
                    &state,
                    "owner-a",
                    "target-a",
                    serde_json::json!({"folder_id":"placement-two"})
                )
                .await,
                Err(AppError::NotFound(_))
            ));
            assert!(super::set_document_lock(
                &state,
                &discovery_auth("owner-a"),
                "space-a",
                "target-a",
                true
            )
            .await
            .is_err());
            sqlx::query(&format!(
                "UPDATE documents SET {field}=NULL WHERE id='target-a'"
            ))
            .execute(&state.db)
            .await
            .unwrap();
        }
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','editor-a')").execute(&state.db).await.unwrap();
        assert!(super::set_document_lock(
            &state,
            &discovery_auth("owner-a"),
            "space-a",
            "target-a",
            true
        )
        .await
        .is_err());
        sqlx::query("DELETE FROM space_user_blocks")
            .execute(&state.db)
            .await
            .unwrap();
        let mut held = state.db.acquire().await.unwrap();
        let worker = state.clone();
        let pending = tokio::spawn(async move {
            super::set_document_lock(
                &worker,
                &discovery_auth("owner-a"),
                "space-a",
                "target-a",
                true,
            )
            .await
        });
        tokio::task::yield_now().await;
        sqlx::query("DELETE FROM sessions WHERE user_id='owner-a'")
            .execute(&mut *held)
            .await
            .unwrap();
        drop(held);
        assert!(matches!(
            pending.await.unwrap(),
            Err(AppError::Unauthorized(_))
        ));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT locked FROM documents WHERE id='target-a'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn placement_expired_bearer_and_late_write_key_scope_changes_are_rejected() {
        let state = placement_state().await;
        let mut expired = discovery_auth("owner-a");
        expired.claims.exp = chrono::Utc::now().timestamp();
        assert!(matches!(
            super::set_document_lock(&state, &expired, "space-a", "target-a", true).await,
            Err(AppError::Unauthorized(_))
        ));
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('late-lock-key','owner-a','Write','hash','prefix','[\"write\"]')").execute(&state.db).await.unwrap();
        let mut key = discovery_auth("owner-a");
        key.claims.sid = "api-key:late-lock-key".into();
        key.api_key_scopes = Some(vec!["write".into()]);
        sqlx::query("CREATE TRIGGER lock_scope_revoke AFTER UPDATE OF locked ON documents BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='late-lock-key'; END").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(
            super::set_document_lock(&state, &key, "space-a", "target-a", true)
                .await
                .is_err()
        );
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    fn discovery_auth(user: &str) -> AuthUser {
        let mut auth = test_auth(user);
        auth.claims.sid = format!("discovery-session-{user}");
        auth
    }
    async fn discovery_test_state() -> AppState {
        let state = super::super::properties::tests::property_test_state().await;
        for user in ["owner-a", "editor-a", "viewer-a"] {
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES ($1,$2,'test','2099-01-01T00:00:00Z')").bind(format!("discovery-session-{user}")).bind(user).execute(&state.db).await.unwrap();
        }
        state
    }
    async fn discovery_list(
        state: &AppState,
        auth: AuthUser,
        nested: bool,
    ) -> Result<serde_json::Value, crate::auth::routes::AppError> {
        let mut query = root_document_query();
        query.0.include_nested = Some(nested);
        let response = list_documents(State(state.clone()), auth, Path("space-a".into()), query)
            .await?
            .into_response();
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        Ok(serde_json::from_slice(&body).unwrap())
    }
    async fn discovery_response(response: impl IntoResponse) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_response().into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
    #[test]
    fn discovery_draft_frontmatter_preserves_custom_fields_without_reserved_overrides() {
        let draft=super::ReadableDiscoveryDraft {id:"draft".into(),title:"Actual title".into(),icon:Some("Actual icon".into()),cover_image:Some("Actual cover".into()),content:"---\r\n\"title\": |\r\n  Forged title\r\n'icon': Forged icon\r\ncover_image: Forged cover\r\nlocked: true\r\npublic: true\r\npublished_at: forged\r\nspace_id: foreign\r\nsynopsis: |\r\n  Keep synopsis\r\npage_width: wide\r\nguidance: Keep guidance\r\ncustom_field:\r\n  nested: Keep nested\r\n---\r\n\r\nBody\r\n<!-- starlane:custom-css -->\r\np { color: blue; }\r\n<!-- /starlane:custom-css -->".into()};
        let content = super::discovery_draft_content(&draft).unwrap();
        let rest = content.strip_prefix("---\n").unwrap();
        let (header, body) = rest.split_once("\n---\n").unwrap();
        let metadata: serde_yaml::Value = serde_yaml::from_str(header).unwrap();
        assert_eq!(metadata["title"].as_str(), Some("Actual title"));
        assert_eq!(metadata["icon"].as_str(), Some("Actual icon"));
        assert_eq!(metadata["cover_image"].as_str(), Some("Actual cover"));
        assert_eq!(metadata["locked"].as_bool(), Some(false));
        assert_eq!(metadata["public"].as_bool(), Some(false));
        assert!(metadata.get("published_at").is_none());
        assert!(metadata.get("space_id").is_none());
        assert_eq!(metadata["page_width"].as_str(), Some("wide"));
        assert_eq!(
            metadata["custom_field"]["nested"].as_str(),
            Some("Keep nested")
        );
        assert!(metadata["synopsis"]
            .as_str()
            .unwrap()
            .contains("Keep synopsis"));
        assert_eq!(metadata["guidance"].as_str(), Some("Keep guidance"));
        assert!(body.contains("Body\n<!-- starlane:custom-css -->\np { color: blue; }"));
        assert_eq!(content.lines().filter(|line| *line == "---").count(), 2);
        assert!(draft.content.contains("Forged title"));
    }
    #[tokio::test]
    async fn discovery_canonical_read_source_rejects_republication_path_and_revision_changes() {
        let state = discovery_test_state().await;
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES ('own-proof','target-a','space-a','viewer-a','Own','Own body','base')").execute(&state.db).await.unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let source: (String, String, String) = sqlx::query_as(
            "SELECT published_at,content_path,updated_at FROM documents WHERE id='target-a'",
        )
        .fetch_one(&mut *conn)
        .await
        .unwrap();
        for statement in [
            "UPDATE documents SET published_at=NULL WHERE id='target-a'",
            "UPDATE documents SET content_path='new-source' WHERE id='target-a'",
            "UPDATE documents SET updated_at='new-revision' WHERE id='target-a'",
        ] {
            sqlx::query("UPDATE documents SET published_at=$1,content_path=$2,updated_at=$3 WHERE id='target-a'").bind(&source.0).bind(&source.1).bind(&source.2).execute(&mut *conn).await.unwrap();
            sqlx::query(statement).execute(&mut *conn).await.unwrap();
            require_document_discovery_page_view(
                &mut conn,
                &discovery_auth("viewer-a"),
                "space-a",
                "target-a",
            )
            .await
            .unwrap();
            assert!(super::require_current_document_read_source(
                &mut conn, "space-a", "target-a", &source.0, &source.1, &source.2
            )
            .await
            .is_err());
        }
    }

    #[tokio::test]
    async fn discovery_unpublished_body_is_exact_readable_draft_not_canonical_or_unshared_draft() {
        let state = discovery_test_state().await;
        let slug = format!("discovery-{}", uuid::Uuid::new_v4());
        let path = crate::storage::write_document(
            &slug,
            "target-a",
            "Canonical secret",
            "",
            "CANONICAL PRIVATE SECRET",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL,content_path=$1,title='Canonical secret' WHERE id='target-a'").bind(path).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,icon,cover_image,content,base_updated_at,updated_at) VALUES ('unshared-draft','target-a','space-a','owner-a','Unshared secret',NULL,NULL,'UNSHARED PRIVATE SECRET','base','2099-01-01'),('shared-z','target-a','space-a','owner-a','Shared Z','Z','z-cover','SHARED Z BODY','base','2026-01-01'),('shared-a','target-a','space-a','owner-a','Shared A','A','a-cover','---\nsynopsis: Portable\n---\n\nSHARED A BODY','base','2026-01-01')").execute(&state.db).await.unwrap();
        for id in ["shared-z", "shared-a"] {
            sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ($1,'viewer-a','viewer','owner-a')").bind(id).execute(&state.db).await.unwrap();
        }
        let response = get_content(
            State(state.clone()),
            discovery_auth("viewer-a"),
            Path(("space-a".into(), "target-a".into())),
        )
        .await
        .unwrap();
        let body = discovery_response(response).await;
        assert_eq!(body["title"], "Shared A");
        let content = body["content"].as_str().unwrap();
        assert!(content.contains("SHARED A BODY"));
        assert!(content.contains("synopsis: Portable"));
        assert!(content.contains("icon: A"));
        assert!(content.contains("cover_image: a-cover"));
        for hidden in [
            "CANONICAL PRIVATE SECRET",
            "UNSHARED PRIVATE SECRET",
            "SHARED Z BODY",
        ] {
            assert!(!content.contains(hidden));
        }
        let metadata = discovery_response(
            get_document(
                State(state.clone()),
                discovery_auth("viewer-a"),
                Path(("space-a".into(), "target-a".into())),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(metadata["draft_id"], "shared-a");
        assert_eq!(metadata["title"], "Shared A");
        assert_eq!(metadata["icon"], "A");
        assert_eq!(metadata["cover_image"], "a-cover");
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES ('own-draft','target-a','space-a','viewer-a','Own title','OWN BODY','base','2000-01-01')").execute(&state.db).await.unwrap();
        let own = discovery_response(
            get_content(
                State(state.clone()),
                discovery_auth("viewer-a"),
                Path(("space-a".into(), "target-a".into())),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(own["title"], "Own title");
        assert!(own["content"].as_str().unwrap().contains("OWN BODY"));
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn discovery_list_projects_only_recognized_exact_draft_shares_with_stable_ties() {
        let state = discovery_test_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        for (id, updated) in [
            ("draft-a", "2026-01-01"),
            ("draft-b", "2026-01-01"),
            ("z-invalid", "2099-01-01"),
        ] {
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES ($1,'target-a','space-a','owner-a',$1,'private','base',$2)").bind(id).bind(updated).execute(&state.db).await.unwrap();
        }
        for id in ["draft-a", "draft-b"] {
            sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ($1,'viewer-a','viewer','owner-a')").bind(id).execute(&state.db).await.unwrap();
        }
        sqlx::query("PRAGMA ignore_check_constraints=ON")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ('z-invalid','viewer-a','unrecognized','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("PRAGMA ignore_check_constraints=OFF")
            .execute(&state.db)
            .await
            .unwrap();
        let listed = discovery_list(&state, discovery_auth("viewer-a"), true)
            .await
            .unwrap();
        assert_eq!(listed[0]["draft_id"], "draft-a");
        let auth = discovery_auth("viewer-a");
        let mut conn = state.db.acquire().await.unwrap();
        sqlx::query("DELETE FROM document_draft_access WHERE draft_id='draft-a'")
            .execute(&mut *conn)
            .await
            .unwrap();
        require_document_discovery_page_view(&mut conn, &auth, "space-a", "target-a")
            .await
            .unwrap();
        assert!(require_discovery_draft_view(
            &mut conn,
            &auth,
            "space-a",
            "target-a",
            listed[0]["draft_id"].as_str().unwrap()
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn discovery_malformed_scalar_and_empty_draft_headers_remain_readable_without_canonical_fallback(
    ) {
        for owned in [false, true] {
            for header in ["foo: [", "scalar", ""] {
                let state = discovery_test_state().await;
                sqlx::query("UPDATE documents SET published_at=NULL,content_path='missing-canonical-secret',title='Canonical secret' WHERE id='target-a'").execute(&state.db).await.unwrap();
                let content = format!("---\r\n{header}\r\n---\r\n\r\nEXACT AUTHORIZED BODY");
                let owner = if owned { "viewer-a" } else { "owner-a" };
                sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,icon,cover_image,content,base_updated_at) VALUES ('repair-draft','target-a','space-a',$1,'Actual draft title','Actual icon','Actual cover',$2,'base')").bind(owner).bind(&content).execute(&state.db).await.unwrap();
                if !owned {
                    sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ('repair-draft','viewer-a','editor','owner-a')").execute(&state.db).await.unwrap();
                }
                let body = discovery_response(
                    get_content(
                        State(state.clone()),
                        discovery_auth("viewer-a"),
                        Path(("space-a".into(), "target-a".into())),
                    )
                    .await
                    .unwrap(),
                )
                .await;
                assert_eq!(body["title"], "Actual draft title");
                assert!(body.get("published_metadata").is_none());
                assert_eq!(body["content"], content.replace("\r\n", "\n"));
                let metadata = discovery_response(
                    get_document(
                        State(state.clone()),
                        discovery_auth("viewer-a"),
                        Path(("space-a".into(), "target-a".into())),
                    )
                    .await
                    .unwrap(),
                )
                .await;
                assert_eq!(metadata["title"], "Actual draft title");
                assert_eq!(metadata["icon"], "Actual icon");
                assert_eq!(metadata["cover_image"], "Actual cover");
                let stored: String = sqlx::query_scalar(
                    "SELECT content FROM document_drafts WHERE id='repair-draft'",
                )
                .fetch_one(&state.db)
                .await
                .unwrap();
                assert_eq!(stored, content);
            }
        }
    }

    #[tokio::test]
    async fn content_published_source_same_row_exact_body_and_late_same_stamp_rejection() {
        let state=discovery_test_state().await;let slug=format!("source-get-{}",uuid::Uuid::new_v4());let path=format!("spaces/{slug}/docs/body.md");
        tokio::fs::create_dir_all(crate::storage::data_dir().join(format!("spaces/{slug}/docs"))).await.unwrap();let body="---\r\ncustom: keep\r\n---\r\nExact 🦅\r\n";tokio::fs::write(crate::storage::data_dir().join(&path),body).await.unwrap();
        sqlx::query("UPDATE documents SET content_path=$1,published_at='published',updated_at='same-clock',icon='Published icon',cover_image='starlane-attachment://11111111-1111-4111-8111-111111111111' WHERE id='target-a'").bind(&path).execute(&state.db).await.unwrap();
        let raw=super::super::published_source::selected(&mut state.db.acquire().await.unwrap(),"space-a","target-a").await.unwrap();
        let response=discovery_response(get_content(State(state.clone()),discovery_auth("viewer-a"),Path(("space-a".into(),"target-a".into()))).await.unwrap()).await;
        assert_eq!(response["content"],body);assert_eq!(response["title"],raw["title"]);assert_eq!(response["published_source_digest"],super::super::published_source::digest(&raw,body.as_bytes()).unwrap());
        assert_eq!(response["published_metadata"],serde_json::json!({"title":raw["title"],"icon":"Published icon","cover_image":"starlane-attachment://11111111-1111-4111-8111-111111111111"}));
        let changed_cover=super::CONTENT_FINAL_SOURCE_CHANGE.scope("UPDATE documents SET cover_image='Later same clock cover' WHERE id='target-a'".into(),get_content(State(state.clone()),discovery_auth("viewer-a"),Path(("space-a".into(),"target-a".into())))).await;
        assert!(changed_cover.is_err());
        let result=super::CONTENT_FINAL_SOURCE_CHANGE.scope("UPDATE documents SET title='Later same stamp' WHERE id='target-a'".into(),get_content(State(state.clone()),discovery_auth("viewer-a"),Path(("space-a".into(),"target-a".into())))).await;assert!(result.is_err());
        assert_eq!(tokio::fs::read(crate::storage::data_dir().join(&path)).await.unwrap(),body.as_bytes());tokio::fs::remove_dir_all(crate::storage::data_dir().join(format!("spaces/{slug}"))).await.unwrap();
    }
    #[tokio::test]
    async fn content_final_selected_draft_proof_rejects_late_revocation_and_source_changes() {
        use axum::extract::FromRequestParts;
        for change in [
            "DELETE FROM document_draft_access WHERE draft_id='draft-a'",
            "UPDATE document_drafts SET title='Changed' WHERE id='draft-a'",
            "UPDATE document_drafts SET content='Changed body' WHERE id='draft-a'",
            "UPDATE documents SET published_at='now' WHERE id='target-a'",
            "DELETE FROM document_drafts WHERE id='draft-a'",
            "DELETE FROM sessions WHERE user_id='viewer-a'",
        ] {
            let state = discovery_test_state().await;
            sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'").execute(&state.db).await.unwrap();
            for id in ["draft-a", "draft-b"] {
                sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES ($1,'target-a','space-a','owner-a',$1,$1,'base')").bind(id).execute(&state.db).await.unwrap();
                sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ($1,'viewer-a','viewer','owner-a')").bind(id).execute(&state.db).await.unwrap();
            }
            let token = jsonwebtoken::encode(&jsonwebtoken::Header::default(), &discovery_auth("viewer-a").claims, &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();
            let (mut parts, _) = axum::http::Request::builder().method("GET").header("Authorization", format!("Bearer {token}")).body(()).unwrap().into_parts();
            let auth = AuthUser::from_request_parts(&mut parts, &state).await.unwrap();
            let result = super::CONTENT_FINAL_SOURCE_CHANGE.scope(change.into(), get_content(State(state.clone()), auth, Path(("space-a".into(), "target-a".into())))).await;
            assert!(result.is_err(), "late source change returned selected body: {change}");
            if !change.contains("sessions") {
                // Draft B still gives page visibility; it must not authorize draft A's saved body.
                let mut conn = state.db.acquire().await.unwrap();
                require_document_discovery_page_view(&mut conn, &discovery_auth("viewer-a"), "space-a", "target-a").await.unwrap();
            }
        }
    }

    #[tokio::test]
    async fn discovery_exact_selected_draft_revocation_cannot_fall_back_to_other_participation() {
        let state = discovery_test_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        for id in ["draft-a", "draft-b"] {
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES ($1,'target-a','space-a','owner-a',$1,$1,'base')").bind(id).execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ($1,'viewer-a','viewer','owner-a')").bind(id).execute(&state.db).await.unwrap();
        }
        let auth = discovery_auth("viewer-a");
        let mut conn = state.db.acquire().await.unwrap();
        let draft = readable_discovery_draft(&mut conn, &auth, "space-a", "target-a")
            .await
            .unwrap();
        assert_eq!(draft.id, "draft-a");
        sqlx::query("DELETE FROM document_draft_access WHERE draft_id='draft-a'")
            .execute(&mut *conn)
            .await
            .unwrap();
        require_document_discovery_page_view(&mut conn, &auth, "space-a", "target-a")
            .await
            .unwrap();
        assert!(
            require_discovery_draft_view(&mut conn, &auth, "space-a", "target-a", &draft.id)
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn discovery_archived_metadata_preserves_view_without_body_or_private_admin_shortcut() {
        let state = discovery_test_state().await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at,archived_at) VALUES ('archived-parent','space-a','Archived ancestor','','owner-a','owner-a','published','2099-01-01')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET archived_at='2099-01-01',parent_id='archived-parent',locked=1,locked_by='owner-a' WHERE id='target-a'").execute(&state.db).await.unwrap();
        let metadata = discovery_response(
            get_document(
                State(state.clone()),
                discovery_auth("viewer-a"),
                Path(("space-a".into(), "target-a".into())),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(metadata["id"], "target-a");
        assert!(get_content(
            State(state.clone()),
            discovery_auth("viewer-a"),
            Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_err());
        assert_eq!(
            discovery_list(&state, discovery_auth("viewer-a"), true)
                .await
                .unwrap(),
            serde_json::json!([])
        );
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='archived-parent'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE users SET role='admin' WHERE id='viewer-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(get_document(
            State(state.clone()),
            discovery_auth("viewer-a"),
            Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_err());
        sqlx::query("UPDATE documents SET published_at='published',deleted_at='2099-01-01' WHERE id='archived-parent'").execute(&state.db).await.unwrap();
        assert!(get_document(
            State(state.clone()),
            discovery_auth("owner-a"),
            Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn discovery_published_content_remains_viewable_when_locked() {
        let state = discovery_test_state().await;
        let slug = format!("discovery-{}", uuid::Uuid::new_v4());
        let path =
            crate::storage::write_document(&slug, "target-a", "Published", "", "PUBLISHED BODY")
                .await
                .unwrap();
        sqlx::query(
            "UPDATE documents SET content_path=$1,locked=1,locked_by='owner-a' WHERE id='target-a'",
        )
        .bind(path)
        .execute(&state.db)
        .await
        .unwrap();
        let body = discovery_response(
            get_content(
                State(state.clone()),
                discovery_auth("viewer-a"),
                Path(("space-a".into(), "target-a".into())),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(body["content"], "PUBLISHED BODY");
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn discovery_list_hides_private_ancestors_and_inactive_rows_but_not_locked_pages() {
        let state = discovery_test_state().await;
        // Model legacy unresolved attribution without treating labels as authority.
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET locked=1, locked_by='owner-a',created_by='unresolved-attribution',last_updated_by='unresolved-attribution' WHERE id='target-a'").execute(&state.db).await.unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,parent_id,title,content_path,created_by,last_updated_by,published_at) VALUES ('private-parent','space-a',NULL,'Hidden title','','owner-a','owner-a',NULL),('published-child','space-a','private-parent','Hidden child','','owner-a','owner-a','published'),('archived-page','space-a',NULL,'Archive title','','owner-a','owner-a','published'),('trashed-page','space-a',NULL,'Trash title','','owner-a','owner-a','published')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET archived_at='2099-01-01' WHERE id='archived-page'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET deleted_at='2099-01-01' WHERE id='trashed-page'")
            .execute(&state.db)
            .await
            .unwrap();
        let visible = discovery_list(&state, discovery_auth("viewer-a"), true)
            .await
            .unwrap();
        assert_eq!(visible.as_array().unwrap().len(), 1);
        assert_eq!(visible[0]["id"], "target-a");
        assert_eq!(visible[0]["locked"], true);
        assert_eq!(visible[0]["created_by"], "unresolved-attribution");
        assert!(!visible.to_string().contains("Hidden"));
        for id in [
            "published-child",
            "archived-page",
            "trashed-page",
            "target-b",
            "missing",
        ] {
            assert!(get_content(
                State(state.clone()),
                discovery_auth("viewer-a"),
                Path(("space-a".into(), id.into()))
            )
            .await
            .is_err());
        }
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES ('parent-draft','private-parent','space-a','owner-a','Private','private','base')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ('parent-draft','viewer-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        let shared = discovery_list(&state, discovery_auth("viewer-a"), true)
            .await
            .unwrap();
        assert_eq!(shared.as_array().unwrap().len(), 3);
        assert!(shared
            .as_array()
            .unwrap()
            .iter()
            .any(|doc| doc["id"] == "published-child"));
    }
    #[tokio::test]
    async fn discovery_global_group_view_is_permission_based_and_blocked_accounts_are_hidden() {
        let state = discovery_test_state().await;
        sqlx::query("DELETE FROM space_members WHERE user_id='viewer-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES ('discovery-group','Discovery')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES ('discovery-member','discovery-group','viewer-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO global_roles(id,name,permissions) VALUES ('opaque-view','Opaque','[\"view\"]')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_global_roles(group_id,role_id) VALUES ('discovery-group','opaque-view')").execute(&state.db).await.unwrap();
        let visible = discovery_list(&state, discovery_auth("viewer-a"), false)
            .await
            .unwrap();
        assert_eq!(visible[0]["id"], "target-a");
        let mut conn = state.db.acquire().await.unwrap();
        let access = super::super::access::resolve_space_access(
            &mut conn,
            super::super::access::principal_from_auth(&discovery_auth("viewer-a")).unwrap(),
            "space-a",
            super::super::access::Purpose::Routine,
        )
        .await
        .unwrap();
        assert!(access.roles.iter().any(|role| matches!(&role.source, super::super::access::RoleSource::GlobalGroup { group_id } if group_id=="discovery-group")));
        drop(conn);
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES ('space-a','viewer-a','owner-a')").execute(&state.db).await.unwrap();
        assert!(discovery_list(&state, discovery_auth("viewer-a"), true)
            .await
            .is_err());
    }
    #[tokio::test]
    async fn discovery_rechecks_real_sessions_current_keys_and_service_principals() {
        let state = discovery_test_state().await;
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES ('discovery-key','viewer-a','Discovery','hash','dk','[\"read\"]')").execute(&state.db).await.unwrap();
        let key_auth = |scope: &str| {
            let mut auth = discovery_auth("viewer-a");
            auth.claims.sid = "api-key:discovery-key".into();
            auth.api_key_scopes = Some(vec![scope.into()]);
            auth
        };
        assert!(discovery_list(&state, key_auth("read"), true).await.is_ok());
        sqlx::query("UPDATE api_keys SET scopes='[\"write\"]' WHERE id='discovery-key'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(discovery_list(&state, key_auth("read"), true)
            .await
            .is_err());
        assert!(get_content(
            State(state.clone()),
            key_auth("write"),
            Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_err());
        sqlx::query("DELETE FROM api_keys WHERE id='discovery-key'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(discovery_list(&state, key_auth("read"), true)
            .await
            .is_err());
        sqlx::query("DELETE FROM sessions WHERE user_id='viewer-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(discovery_list(&state, discovery_auth("viewer-a"), true)
            .await
            .is_err());
        sqlx::query("UPDATE users SET is_active=0 WHERE id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(discovery_list(&state, discovery_auth("editor-a"), true)
            .await
            .is_err());
        let mut service = discovery_auth("owner-a");
        service.claims.sid = "service:discovery-key".into();
        service.claims.sub = "service:discovery-key".into();
        service.api_key_scopes = Some(vec!["admin".into()]);
        assert!(discovery_list(&state, service, true).await.is_err());
    }

    async fn document_list_test_state() -> AppState {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        for statement in [
            "CREATE TABLE users (account_kind TEXT NOT NULL DEFAULT 'member',id TEXT PRIMARY KEY, role TEXT NOT NULL)",
            "CREATE TABLE spaces (id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT 'Test Space', slug TEXT NOT NULL DEFAULT 'test-space', visibility TEXT NOT NULL, trash_retention_days INTEGER NOT NULL DEFAULT 30)",
            "CREATE TABLE space_members (space_id TEXT NOT NULL, user_id TEXT NOT NULL, role TEXT NOT NULL)",
            "CREATE TABLE groups (id TEXT PRIMARY KEY)",
            "CREATE TABLE group_members (group_id TEXT NOT NULL, user_id TEXT NOT NULL)",
            "CREATE TABLE space_group_permissions (space_id TEXT NOT NULL, group_id TEXT NOT NULL, role TEXT NOT NULL)",
            "CREATE TABLE space_user_blocks (space_id TEXT NOT NULL, user_id TEXT NOT NULL)",
            "CREATE TABLE space_roles (id TEXT NOT NULL, space_id TEXT NOT NULL, permissions TEXT NOT NULL)",
            "CREATE TABLE global_roles (id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', permissions TEXT NOT NULL DEFAULT '[]', built_in INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE group_global_roles (group_id TEXT NOT NULL, role_id TEXT NOT NULL, PRIMARY KEY(group_id, role_id))",
            "CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                folder_id TEXT,
                parent_id TEXT,
                title TEXT NOT NULL,
                icon TEXT,
                cover_image TEXT,
                content_path TEXT NOT NULL DEFAULT '',
                locked INTEGER NOT NULL DEFAULT 0,
                locked_by TEXT,
                public INTEGER NOT NULL DEFAULT 0,
                public_slug TEXT,
                created_by TEXT NOT NULL,
                last_updated_by TEXT NOT NULL,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                published_at TEXT,
                archived_at TEXT,
                deleted_at TEXT,
                deleted_by TEXT
            )",
            "CREATE TABLE document_drafts (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                space_id TEXT NOT NULL,
                owner_id TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                base_updated_at TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
            "CREATE TABLE document_draft_access (
                draft_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                role TEXT NOT NULL
            )",
            "CREATE TABLE starred_documents (
                user_id TEXT NOT NULL,
                document_id TEXT NOT NULL,
                starred_at TEXT NOT NULL,
                PRIMARY KEY(user_id, document_id)
            )",
            "CREATE TABLE document_workflows (
                document_id TEXT PRIMARY KEY,
                workflow_template_id TEXT NOT NULL,
                current_stage_id TEXT NOT NULL,
                status TEXT NOT NULL
            )",
            "CREATE TABLE workflow_templates (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                name TEXT NOT NULL,
                stages TEXT NOT NULL
            )",
        ] {
            sqlx::query(statement)
                .execute(&db)
                .await
                .expect("create document list fixture table");
        }
        let index_path = std::env::temp_dir().join(format!(
            "starlane-notes-document-list-test-{}",
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

    pub(crate) fn test_auth(user_id: &str) -> AuthUser {
        AuthUser {
            claims: Claims {
                sub: user_id.to_string(),
                sid: "session-1".to_string(),
                email: format!("{user_id}@example.test"),
                role: "member".to_string(),
                exp: i64::MAX,
                iat: 0,
                token_type: TokenType::Access,
            },
            api_key_scopes: None,
        }
    }

    fn root_document_query() -> Query<ListDocumentsQuery> {
        Query(ListDocumentsQuery {
            folder_id: None,
            include_nested: None,
            workflow_status: None,
            workflow_stage_id: None,
            workflow_template_id: None,
        })
    }

    pub(super) fn public_link_auth(user: &str) -> AuthUser {
        let mut auth = test_auth(user);
        auth.claims.sid = format!("public-link-{user}");
        auth
    }
    pub(super) async fn public_link_test_state() -> AppState {
        let state = super::super::properties::tests::property_test_state().await;
        for sql in [
            "INSERT INTO users(id,email,display_name,role) VALUES('reader-a','reader@example.test','Reader','member')",
            "UPDATE users SET display_name='Test author' WHERE id='editor-a'",
            "INSERT INTO space_members(id,space_id,user_id,role) VALUES('public-reader','space-a','reader-a','viewer')",
            "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,created_at,updated_at,published_at,public,public_slug) VALUES('page-a','space-a','Published A','','editor-a','editor-a','old','old','old',1,'link-a'),('page-b','space-b','Private B','','editor-a','editor-a','old','old','old',1,'link-b')",
        ] {sqlx::query(sql).execute(&state.db).await.unwrap();}
        for actor in ["editor-a", "reader-a", "owner-a"] {
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'test','2999-01-01T00:00:00Z')").bind(format!("public-link-{actor}")).bind(actor).execute(&state.db).await.unwrap();
        }
        state
    }

    pub(crate) async fn move_guard_fixture() -> (AppState, String, String) {
        let state = document_list_test_state().await;
        let source_slug = format!("move-source-{}", uuid::Uuid::new_v4());
        let target_slug = format!("move-target-{}", uuid::Uuid::new_v4());
        for statement in [
            "CREATE TABLE folders (id TEXT PRIMARY KEY, space_id TEXT, parent_id TEXT, name TEXT, icon TEXT, sort_order INTEGER DEFAULT 0, created_at TEXT, updated_at TEXT)",
            "CREATE TABLE properties_schema (id TEXT PRIMARY KEY, space_id TEXT, name TEXT, property_type TEXT, options TEXT)",
            "CREATE TABLE property_values (id TEXT PRIMARY KEY, document_id TEXT, property_id TEXT, value TEXT)",
            "CREATE TABLE tags (id TEXT PRIMARY KEY, space_id TEXT, name TEXT)",
            "CREATE TABLE doc_tags (document_id TEXT, tag_id TEXT)",
            "CREATE TABLE doc_versions (id TEXT PRIMARY KEY, document_id TEXT, version_number INTEGER, change_set BLOB, lines_added INTEGER DEFAULT 0, lines_removed INTEGER DEFAULT 0, created_by TEXT, created_at TEXT)",
            "CREATE TABLE attachments (id TEXT PRIMARY KEY, document_id TEXT, space_id TEXT, storage_path TEXT)",
            "CREATE TABLE document_workflow_events (id TEXT PRIMARY KEY, document_workflow_id TEXT, document_id TEXT, from_stage_id TEXT, to_stage_id TEXT, action TEXT, note TEXT, created_by TEXT, created_at TEXT)",
            "CREATE TABLE audit_log (id TEXT PRIMARY KEY, user_id TEXT, api_key_id TEXT, action TEXT, target_type TEXT, target_id TEXT, ip_address TEXT, created_at TEXT)",
            "INSERT INTO users(id,role) VALUES ('mover', 'member'), ('admin-mover', 'admin')",
            "INSERT INTO space_members VALUES ('move-source', 'mover', 'editor'), ('move-target', 'mover', 'editor')",
            "INSERT INTO folders VALUES ('source-folder', 'move-source', NULL, 'Source folder', 'folder', 4, 'old', 'old'), ('child-folder', 'move-source', 'source-folder', 'Child folder', NULL, 5, 'old', 'old'), ('other-folder', 'move-source', NULL, 'Other folder', NULL, 6, 'old', 'old'), ('target-folder', 'move-target', NULL, 'Target', NULL, 0, 'old', 'old')",
            "INSERT INTO properties_schema VALUES ('source-property','move-source','Score','number',NULL)",
            "INSERT INTO property_values VALUES ('source-value','move-page','source-property','42')",
            "INSERT INTO tags VALUES ('source-tag','move-source','Important')",
            "INSERT INTO doc_tags VALUES ('move-page','source-tag')",
            "INSERT INTO workflow_templates VALUES ('source-workflow','move-source','Review','[]')",
            "INSERT INTO document_workflows VALUES ('move-page','source-workflow','review','active')",
            "INSERT INTO document_workflow_events VALUES ('history-event','source-instance','move-page',NULL,'review','enable','Preserved history','mover','old')",
            "INSERT INTO starred_documents VALUES ('mover','move-page','old')",
            "INSERT INTO document_drafts VALUES ('move-draft','move-page','move-source','mover','Saved draft','Private saved body','old','old','old')",
            "INSERT INTO document_draft_access VALUES ('move-draft','collaborator','editor')",
        ] { sqlx::query(statement).execute(&state.db).await.unwrap(); }
        for (id, slug) in [("move-source", &source_slug), ("move-target", &target_slug)] {
            sqlx::query("INSERT INTO spaces (id,slug,visibility) VALUES ($1,$2,'private')")
                .bind(id)
                .bind(slug)
                .execute(&state.db)
                .await
                .unwrap();
        }
        for (id, parent, folder) in [
            ("move-page", None, "source-folder"),
            ("move-child", Some("move-page"), "child-folder"),
        ] {
            let path = crate::storage::write_document(
                &source_slug,
                id,
                id,
                "title: Preserved",
                "Preserved canonical body",
            )
            .await
            .unwrap();
            sqlx::query("INSERT INTO documents (id,space_id,folder_id,parent_id,title,content_path,created_by,last_updated_by,created_at,updated_at,published_at,public,public_slug) VALUES ($1,'move-source',$2,$3,$1,$4,'mover','mover','old','old','old',1,$1)")
                .bind(id).bind(folder).bind(parent).bind(path).execute(&state.db).await.unwrap();
        }
        let version_path = crate::storage::space_dir(&source_slug).join("history/version.md");
        tokio::fs::create_dir_all(version_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&version_path, b"Historical body")
            .await
            .unwrap();
        let attachment_path = crate::storage::space_dir(&source_slug).join("attachments/file.bin");
        tokio::fs::create_dir_all(attachment_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&attachment_path, b"Attachment bytes")
            .await
            .unwrap();
        sqlx::query("INSERT INTO doc_versions (id, document_id, version_number, change_set, created_by, created_at) VALUES ('version','move-page',1,$1,'mover','old')")
            .bind(b"Historical CRDT bytes".as_slice())
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO attachments VALUES ('attachment','move-page','move-source',$1)")
            .bind(attachment_path.to_string_lossy().as_ref())
            .execute(&state.db)
            .await
            .unwrap();
        (state, source_slug, target_slug)
    }

    pub(crate) async fn move_database_snapshot(state: &AppState) -> Vec<(String, Option<String>)> {
        use sqlx::Row;
        let tables: Vec<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
                .fetch_all(&state.db)
                .await
                .unwrap();
        let mut snapshot = Vec::new();
        for table in tables {
            let columns = sqlx::query(&format!("PRAGMA table_info({table})"))
                .fetch_all(&state.db)
                .await
                .unwrap();
            let expression = columns
                .iter()
                .map(|column| format!("quote(\"{}\")", column.get::<String, _>("name")))
                .collect::<Vec<_>>()
                .join(" || '|' || ");
            let rows: Option<String> = sqlx::query_scalar(&format!("SELECT group_concat({expression}, char(10)) FROM (SELECT * FROM \"{table}\" ORDER BY rowid)")).fetch_one(&state.db).await.unwrap();
            snapshot.push((table, rows));
        }
        snapshot
    }

    pub(crate) fn move_file_snapshot(slug: &str) -> Vec<(std::path::PathBuf, Vec<u8>)> {
        fn collect(
            root: &std::path::Path,
            current: &std::path::Path,
            files: &mut Vec<(std::path::PathBuf, Vec<u8>)>,
        ) {
            for entry in std::fs::read_dir(current).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    collect(root, &path, files);
                } else {
                    files.push((
                        path.strip_prefix(root).unwrap().to_path_buf(),
                        std::fs::read(path).unwrap(),
                    ));
                }
            }
        }
        let root = crate::storage::space_dir(slug);
        let mut files = Vec::new();
        collect(&root, &root, &mut files);
        files.sort();
        files
    }

    async fn placement_move(
        state: &AppState,
        user: &str,
        doc: &str,
        target: &str,
        folder: Option<&str>,
        parent: Option<&str>,
    ) -> Result<(), AppError> {
        super::move_document(
            State(state.clone()),
            discovery_auth(user),
            Path(("space-a".into(), doc.into())),
            Json(super::MoveDocumentRequest {
                target_space_id: target.into(),
                folder_id: folder.map(str::to_owned),
                parent_id: parent.map(str::to_owned),
            }),
        )
        .await
        .map(|_| ())
    }
    #[tokio::test]
    async fn legacy_cross_space_document_moves_preserve_every_record_and_file() {
        let state = placement_state().await;
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('move-other','space-b','owner-a','editor')").execute(&state.db).await.unwrap();
        let slug = format!("cross-move-{}", uuid::Uuid::new_v4());
        let path = crate::storage::write_document(&slug, "target-a", "Original", "", "Keep bytes")
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET content_path=$1 WHERE id='target-a'")
            .bind(path)
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        let files = move_file_snapshot(&slug);
        let error = placement_move(&state, "owner-a", "target-a", "space-b", None, None)
            .await
            .unwrap_err();
        assert!(
            matches!(error,AppError::Conflict(message) if message.contains("metadata and access preview"))
        );
        assert_eq!(move_database_snapshot(&state).await, before);
        assert_eq!(move_file_snapshot(&slug), files);
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn document_move_resolves_source_and_conceals_missing_or_inaccessible_targets() {
        let state = placement_state().await;
        let before = move_database_snapshot(&state).await;
        for (doc, target, folder, parent) in [
            ("missing", "space-a", None, None),
            ("target-b", "space-a", None, None),
            ("target-a", "missing", None, None),
            ("target-a", "space-b", None, None),
            ("target-a", "space-a", Some("missing"), None),
            ("target-a", "space-a", None, Some("target-b")),
        ] {
            assert!(matches!(
                placement_move(&state, "owner-a", doc, target, folder, parent).await,
                Err(AppError::NotFound(_))
            ));
            assert_eq!(move_database_snapshot(&state).await, before);
        }
    }
    #[tokio::test]
    async fn same_space_document_move_keeps_subtree_identity_metadata_and_files() {
        let state = placement_state().await;
        placement_child(&state, "move-child", "target-a").await;
        let slug = format!("same-move-{}", uuid::Uuid::new_v4());
        let path = crate::storage::write_document(&slug, "target-a", "Original", "", "Keep bytes")
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET content_path=$1 WHERE id='target-a'")
            .bind(path)
            .execute(&state.db)
            .await
            .unwrap();
        let files = move_file_snapshot(&slug);
        placement_move(
            &state,
            "editor-a",
            "target-a",
            "space-a",
            Some("placement-two"),
            None,
        )
        .await
        .unwrap();
        let rows:Vec<(String,Option<String>,Option<String>,String)>=sqlx::query_as("SELECT id,folder_id,parent_id,last_updated_by FROM documents WHERE id IN ('target-a','move-child') ORDER BY id").fetch_all(&state.db).await.unwrap();
        assert_eq!(
            rows,
            vec![
                (
                    "move-child".into(),
                    Some("placement-two".into()),
                    Some("target-a".into()),
                    "editor-a".into()
                ),
                (
                    "target-a".into(),
                    Some("placement-two".into()),
                    None,
                    "editor-a".into()
                )
            ]
        );
        placement_move(&state, "editor-a", "target-a", "space-a", None, None)
            .await
            .unwrap();
        let folders: Vec<Option<String>> = sqlx::query_scalar(
            "SELECT folder_id FROM documents WHERE id IN ('target-a','move-child')",
        )
        .fetch_all(&state.db)
        .await
        .unwrap();
        assert_eq!(folders, vec![None, None]);
        assert_eq!(move_file_snapshot(&slug), files);
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn placement_move_private_editor_lock_and_late_authority_rollback() {
        let state = placement_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('move-private','target-a','space-a','owner-a','Private','secret','base')").execute(&state.db).await.unwrap();
        for grant in [None, Some("viewer")] {
            if let Some(grant) = grant {
                sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('move-private','editor-a',$1,'owner-a')").bind(grant).execute(&state.db).await.unwrap();
            }
            let before = move_database_snapshot(&state).await;
            assert!(
                placement_move(&state, "editor-a", "target-a", "space-a", None, None)
                    .await
                    .is_err()
            );
            assert_eq!(move_database_snapshot(&state).await, before);
        }
        sqlx::query("UPDATE document_draft_access SET role='editor' WHERE draft_id='move-private'")
            .execute(&state.db)
            .await
            .unwrap();
        placement_move(&state, "editor-a", "target-a", "space-a", None, None)
            .await
            .unwrap();
        assert!(state
            .search_index
            .search("secret", None, 10)
            .unwrap()
            .is_empty());
        sqlx::query("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(matches!(
            placement_move(
                &state,
                "editor-a",
                "target-a",
                "space-a",
                Some("placement-two"),
                None
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        assert_eq!(move_database_snapshot(&state).await, before);
        sqlx::query("UPDATE documents SET locked=0,locked_by=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER move_revoke AFTER UPDATE OF folder_id ON documents WHEN NEW.id='target-a' BEGIN DELETE FROM sessions WHERE id='discovery-session-editor-a'; END").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(matches!(
            placement_move(
                &state,
                "editor-a",
                "target-a",
                "space-a",
                Some("placement-two"),
                None
            )
            .await,
            Err(AppError::Unauthorized(_))
        ));
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn placement_move_and_put_keep_distinct_none_and_actor_stamp_semantics() {
        let state = placement_state().await;
        placement_child(&state, "move-child", "target-a").await;
        placement_update(
            &state,
            "editor-a",
            "move-child",
            serde_json::json!({"folder_id":null,"parent_id":null}),
        )
        .await
        .unwrap();
        let row: (Option<String>, Option<String>, String) = sqlx::query_as(
            "SELECT folder_id,parent_id,last_updated_by FROM documents WHERE id='move-child'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(
            row,
            (
                Some("placement-one".into()),
                Some("target-a".into()),
                "owner-a".into()
            )
        );
        placement_move(&state, "editor-a", "move-child", "space-a", None, None)
            .await
            .unwrap();
        let row: (Option<String>, Option<String>, String) = sqlx::query_as(
            "SELECT folder_id,parent_id,last_updated_by FROM documents WHERE id='move-child'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(row, (None, None, "editor-a".into()));
    }
    #[tokio::test]
    async fn placement_move_invalid_parent_and_hidden_subtree_are_atomic() {
        let state = placement_state().await;
        placement_child(&state, "move-child", "target-a").await;
        for parent in ["missing", "target-a", "move-child", "target-b"] {
            let before = move_database_snapshot(&state).await;
            assert!(placement_move(
                &state,
                "editor-a",
                "target-a",
                "space-a",
                Some("placement-two"),
                Some(parent)
            )
            .await
            .is_err());
            assert_eq!(move_database_snapshot(&state).await, before);
        }
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='move-child'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(
            placement_move(&state, "editor-a", "target-a", "space-a", None, None)
                .await
                .is_err()
        );
        assert_eq!(move_database_snapshot(&state).await, before);
    }

    #[tokio::test]
    async fn public_links_typed_private_ancestors_and_archived_lock_exception() {
        let state = public_link_test_state().await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('private-link-parent','space-a','Private','','owner-a','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('parent-draft','private-link-parent','space-a','owner-a','Parent','secret','base')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET parent_id='private-link-parent',locked=1,locked_by='owner-a' WHERE id='page-a'").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(super::change_public_link(
            &state,
            &public_link_auth("editor-a"),
            "space-a",
            "page-a",
            true
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('parent-draft','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        super::change_public_link(
            &state,
            &public_link_auth("editor-a"),
            "space-a",
            "page-a",
            true,
        )
        .await
        .unwrap();
        sqlx::query(
            "UPDATE documents SET archived_at='old' WHERE id IN ('page-a','private-link-parent')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(super::change_public_link(
            &state,
            &public_link_auth("editor-a"),
            "space-a",
            "page-a",
            true
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
        super::change_public_link(
            &state,
            &public_link_auth("editor-a"),
            "space-a",
            "page-a",
            false,
        )
        .await
        .unwrap();
        let locked: i64 = sqlx::query_scalar("SELECT locked FROM documents WHERE id='page-a'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(locked, 1);
        sqlx::query("UPDATE documents SET published_at=NULL,archived_at=NULL,parent_id=NULL WHERE id='page-a'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('private-link','page-a','space-a','owner-a','Private','secret','base')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('private-link','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(super::change_public_link(
            &state,
            &public_link_auth("editor-a"),
            "space-a",
            "page-a",
            false
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
        sqlx::query("UPDATE document_draft_access SET role='editor' WHERE draft_id='private-link'")
            .execute(&state.db)
            .await
            .unwrap();
        super::change_public_link(
            &state,
            &public_link_auth("editor-a"),
            "space-a",
            "page-a",
            false,
        )
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn public_links_late_current_identity_and_source_changes_roll_back_every_table() {
        for change in [
            "DELETE FROM sessions WHERE id='public-link-editor-a'",
            "UPDATE users SET is_active=0 WHERE id='editor-a'",
            "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','editor-a','owner-a')",
            "UPDATE documents SET published_at=NULL WHERE id='page-a'",
            "UPDATE documents SET public_slug='replacement' WHERE id='page-a'",
            "UPDATE documents SET content_path='changed' WHERE id='page-a'",
        ] {
            let state=public_link_test_state().await;
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('own-link-draft','page-a','space-a','editor-a','Own','secret','base')").execute(&state.db).await.unwrap();
            sqlx::query(&format!("CREATE TRIGGER late_public_link AFTER UPDATE OF public ON documents WHEN NEW.id='page-a' BEGIN {change}; END")).execute(&state.db).await.unwrap();
            let before=move_database_snapshot(&state).await;
            assert!(super::change_public_link(&state,&public_link_auth("editor-a"),"space-a","page-a",true).await.is_err(),"{change}");
            assert_eq!(move_database_snapshot(&state).await,before,"{change}");
        }
    }
    #[tokio::test]
    async fn public_links_real_http_keys_require_write_ceiling_and_actual_edit_content() {
        use tower::ServiceExt;
        let state = public_link_test_state().await;
        for (id, actor, scope) in [
            ("link-read", "editor-a", "[\"read\"]"),
            ("link-write", "editor-a", "[\"write\"]"),
            ("link-viewer-write", "reader-a", "[\"write\"]"),
        ] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,$2,$1,$3,'test',$4)").bind(id).bind(actor).bind(crate::auth::api_keys::hash_api_key(id)).bind(scope).execute(&state.db).await.unwrap();
        }
        for (key, action, success) in [
            ("link-read", "publish", false),
            ("link-viewer-write", "publish", false),
            ("link-write", "publish", true),
            ("link-read", "unpublish", false),
            ("link-write", "unpublish", true),
        ] {
            let response = super::router()
                .with_state(state.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri(format!("/spaces/space-a/documents/page-a/{action}"))
                        .header("X-API-Key", key)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status().is_success(),
                success,
                "{key} {action}:{}",
                response.status()
            );
        }
    }
    #[tokio::test]
    async fn public_links_observe_external_wal_revocation_after_space_lock_wait() {
        for variant in ["session", "block", "key", "group"] {
            let mut state = public_link_test_state().await;
            let file = std::env::temp_dir().join(format!("link-wal-{}.db", uuid::Uuid::new_v4()));
            state.db = sqlx::any::AnyPoolOptions::new()
                .max_connections(2)
                .after_connect(|conn, _| {
                    Box::pin(async move {
                        sqlx::query("PRAGMA journal_mode=WAL")
                            .execute(&mut *conn)
                            .await?;
                        sqlx::query("PRAGMA busy_timeout=5000")
                            .execute(conn)
                            .await?;
                        Ok(())
                    })
                })
                .connect(&format!("sqlite://{}?mode=rwc", file.display()))
                .await
                .unwrap();
            sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
            for sql in [
                "INSERT INTO users(id,email,display_name,role) VALUES('editor-a','editor@example.test','Editor','member')",
                "INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('public-link-editor-a','editor-a','test','2999-01-01T00:00:00Z')",
                "INSERT INTO spaces(id,name,slug,created_by) VALUES('space-a','A','a','editor-a')",
                "INSERT INTO space_members(id,space_id,user_id,role) VALUES('editor','space-a','editor-a','editor')",
                "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('page-a','space-a','Page','','editor-a','editor-a','old')",
            ] {sqlx::query(sql).execute(&state.db).await.unwrap();}
            let mut auth = public_link_auth("editor-a");
            if variant == "key" {
                sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('wait-key','editor-a','Test','hash','test','[\"write\"]')").execute(&state.db).await.unwrap();
                auth.claims.sid = "api-key:wait-key".into();
                auth.api_key_scopes = Some(vec!["write".into()]);
            }
            if variant == "group" {
                for sql in ["DELETE FROM space_members","INSERT INTO groups(id,name) VALUES('writers','Writers')","INSERT INTO group_members(id,group_id,user_id) VALUES('member','writers','editor-a')","INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('custom:link','space-a','Writer','[\"edit_content\"]','editor-a')","INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('grant','space-a','writers','custom:link')"] {sqlx::query(sql).execute(&state.db).await.unwrap();}
            }
            let mut blocker = state.db.begin().await.unwrap();
            sqlx::query("UPDATE spaces SET id=id WHERE id='space-a'")
                .execute(&mut *blocker)
                .await
                .unwrap();
            let change=match variant {"session"=>"DELETE FROM sessions WHERE user_id='editor-a'","block"=>"INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','editor-a','editor-a')","key"=>"UPDATE api_keys SET scopes='[\"read\"]' WHERE id='wait-key'",_=>"DELETE FROM space_group_permissions"};
            sqlx::query(change).execute(&mut *blocker).await.unwrap();
            let copied = state.clone();
            let mut pending = tokio::spawn(async move {
                super::change_public_link(&copied, &auth, "space-a", "page-a", true).await
            });
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(30), &mut pending)
                    .await
                    .is_err()
            );
            blocker.commit().await.unwrap();
            assert!(pending.await.unwrap().is_err(), "{variant}");
            let public: i64 = sqlx::query_scalar("SELECT public FROM documents WHERE id='page-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(public, 0);
            let audit: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(audit, 0);
            state.db.close().await;
            std::fs::remove_file(file).unwrap();
        }
    }

    #[tokio::test]
    async fn public_links_reject_cross_space_ids_without_mutations_or_audit_events() {
        let state = public_link_test_state().await;
        assert!(matches!(
            super::publish_document(
                State(state.clone()),
                public_link_auth("editor-a"),
                Path(("space-a".into(), "page-b".into()))
            )
            .await,
            Err(crate::auth::routes::AppError::NotFound(_))
        ));
        assert!(matches!(
            super::unpublish_document(
                State(state.clone()),
                public_link_auth("editor-a"),
                Path(("space-a".into(), "page-b".into()))
            )
            .await,
            Err(crate::auth::routes::AppError::NotFound(_))
        ));
        assert!(super::publish_document(
            State(state.clone()),
            public_link_auth("editor-a"),
            Path(("space-a".into(), "missing".into()))
        )
        .await
        .is_err());
        let unchanged: (i64, String, String) = sqlx::query_as(
            "SELECT public, public_slug, updated_at FROM documents WHERE id = 'page-b'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(unchanged, (1, "link-b".into(), "old".into()));
        let audit_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(audit_count, 0);
    }

    #[tokio::test]
    async fn public_links_respect_effective_editor_access_and_lifecycle() {
        let state = public_link_test_state().await;
        assert!(matches!(
            super::unpublish_document(
                State(state.clone()),
                public_link_auth("reader-a"),
                Path(("space-a".into(), "page-a".into()))
            )
            .await,
            Err(crate::auth::routes::AppError::NotFound(_))
        ));
        sqlx::query(
            "INSERT INTO space_user_blocks (space_id, user_id, created_by) VALUES ('space-a', 'editor-a','owner-a')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert!(matches!(
            super::publish_document(
                State(state.clone()),
                public_link_auth("editor-a"),
                Path(("space-a".into(), "page-a".into()))
            )
            .await,
            Err(crate::auth::routes::AppError::NotFound(_))
        ));
        sqlx::query("DELETE FROM space_user_blocks")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET archived_at = 'old' WHERE id = 'page-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::publish_document(
            State(state.clone()),
            public_link_auth("editor-a"),
            Path(("space-a".into(), "page-a".into()))
        )
        .await
        .is_err());
        let audit_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(audit_count, 0);
        // Revocation remains available for archived pages, even when enable is forbidden.
        super::unpublish_document(
            State(state.clone()),
            public_link_auth("editor-a"),
            Path(("space-a".into(), "page-a".into())),
        )
        .await
        .unwrap();
        let action: String = sqlx::query_scalar("SELECT action FROM audit_log")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(action, "document.public_link_disable");
        let public: i64 = sqlx::query_scalar("SELECT public FROM documents WHERE id = 'page-a'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(public, 0);
    }

    #[tokio::test]
    async fn public_reader_rechecks_link_lifecycle_and_exact_source_after_io_wal() {
        for change in ["public=0", "published_at=NULL", "archived_at='changed'", "deleted_at='changed'", "public_slug='replacement'", "content_path='replacement.md'", "updated_at='changed'"] {
            let mut state = super::super::properties::tests::property_test_state().await;
            let db_path = std::env::temp_dir().join(format!("public-read-{}.sqlite", uuid::Uuid::new_v4()));
            std::fs::File::create(&db_path).unwrap();
            let url = format!("sqlite://{}", db_path.display());
            state.db = sqlx::any::AnyPoolOptions::new().max_connections(2).connect(&url).await.unwrap();
            sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
            sqlx::query("PRAGMA journal_mode=WAL").execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO users(id,email,password_hash,display_name,role) VALUES('public-owner','public@example.test','hash','Public author','member')").execute(&state.db).await.unwrap();
            let slug = format!("public-read-{}", uuid::Uuid::new_v4());
            sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('public-space','Shared space',$1,'private','public-owner')").bind(&slug).execute(&state.db).await.unwrap();
            let path = crate::storage::write_document(&slug,"public-page","Allowed","","Allowed published body").await.unwrap();
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at,published_at,public,public_slug) VALUES('public-page','public-space','Allowed',$1,'public-owner','public-owner','old','old',1,'public-token')").bind(&path).execute(&state.db).await.unwrap();
            let other = sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
            let query = format!("UPDATE documents SET {change} WHERE id='public-page'");
            super::PUBLIC_READ_AFTER_IO.with(|slot| *slot.borrow_mut()=Some(Box::pin(async move {
                sqlx::query(&query).execute(&other).await.unwrap();
                other.close().await;
            })));
            let result = super::read_public_page(&state,"public-token",true).await;
            state.db.close().await;
            tokio::fs::remove_dir_all(crate::storage::space_dir(&slug)).await.unwrap();
            for suffix in ["", "-wal", "-shm"] { let _=std::fs::remove_file(format!("{}{suffix}",db_path.display())); }
            assert!(matches!(result,Err(crate::auth::routes::AppError::NotFound(_))),"stale anonymous source emitted after {change}");
        }
    }

    #[tokio::test]
    async fn public_links_check_global_switch_and_all_revocation_states_before_storage() {
        let state = public_link_test_state().await;
        assert!(matches!(
            super::read_public_page(&state, "link-a", false).await,
            Err(crate::auth::routes::AppError::NotFound(_))
        ));
        // Missing storage returns the same unavailable response without disclosing a disk path.
        assert!(matches!(
            super::read_public_page(&state, "link-a", true).await,
            Err(crate::auth::routes::AppError::NotFound(_))
        ));
        let slug = format!("public-revocation-qa-{}", uuid::Uuid::new_v4());
        let content_path = crate::storage::write_document(
            &slug,
            "page-a",
            "Published A",
            "title: Published A",
            "Published content",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE documents SET content_path = $1 WHERE id = 'page-a'")
            .bind(&content_path)
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::read_public_page(&state, "link-a", true)
            .await
            .is_ok());
        assert!(matches!(
            super::read_public_page(&state, "link-a", false).await,
            Err(crate::auth::routes::AppError::NotFound(_))
        ));
        for revoked in [
            "public = 0",
            "published_at = NULL",
            "archived_at = 'old'",
            "deleted_at = 'old'",
        ] {
            sqlx::query("UPDATE documents SET public = 1, published_at = 'old', archived_at = NULL, deleted_at = NULL WHERE id = 'page-a'").execute(&state.db).await.unwrap();
            sqlx::query(&format!(
                "UPDATE documents SET {revoked} WHERE id = 'page-a'"
            ))
            .execute(&state.db)
            .await
            .unwrap();
            assert!(matches!(
                super::read_public_page(&state, "link-a", true).await,
                Err(crate::auth::routes::AppError::NotFound(_))
            ));
        }
        crate::storage::delete_document(&content_path)
            .await
            .unwrap();
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn public_links_read_only_published_content_in_every_space_visibility() {
        let state = public_link_test_state().await;
        let slug = format!("public-link-qa-{}", uuid::Uuid::new_v4());
        let content_path = crate::storage::write_document(
            &slug,
            "page-a",
            "Published A",
            "title: Published A",
            "Published content only",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE documents SET content_path = $1 WHERE id = 'page-a'")
            .bind(&content_path)
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts (id, document_id, space_id, owner_id, title, content, base_updated_at, created_at, updated_at) VALUES ('draft-a', 'page-a', 'space-a', 'editor-a', 'SECRET draft title', 'SECRET draft content', 'old', 'old', 'old')").execute(&state.db).await.unwrap();
        for visibility in ["public", "internal", "private"] {
            sqlx::query("UPDATE spaces SET visibility = $1 WHERE id = 'space-a'")
                .bind(visibility)
                .execute(&state.db)
                .await
                .unwrap();
            let response = super::read_public_page(&state, "link-a", true)
                .await
                .unwrap()
                .0;
            assert_eq!(response.title, "Published A");
            assert!(response.content.contains("Published content only"));
            assert!(!response.content.contains("SECRET"));
            assert_eq!(response.author, "Test author");
        }
        crate::storage::delete_document(&content_path)
            .await
            .unwrap();
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn public_links_do_not_grant_anonymous_uploaded_asset_access() {
        use tower::ServiceExt;
        let state = public_link_test_state().await;
        let response = crate::spaces::attachments::router()
            .with_state(state)
            .oneshot(
                axum::http::Request::builder()
                    .uri("/files/spaces/test/attachments/page-a/image.png")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn public_links_parse_global_switch_without_mutating_process_environment() {
        assert!(super::public_pages_enabled_value(None));
        assert!(super::public_pages_enabled_value(Some("true")));
        assert!(!super::public_pages_enabled_value(Some("false")));
        assert!(!super::public_pages_enabled_value(Some(" FALSE ")));
    }

    #[tokio::test]
    async fn public_links_require_published_revision_and_support_custom_edit_permission() {
        let state = public_link_test_state().await;
        sqlx::query("UPDATE documents SET published_at = NULL WHERE id = 'page-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(matches!(
            super::publish_document(
                State(state.clone()),
                public_link_auth("editor-a"),
                Path(("space-a".into(), "page-a".into()))
            )
            .await,
            Err(crate::auth::routes::AppError::NotFound(_))
        ));
        let audit_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(audit_count, 0);
        sqlx::query("UPDATE documents SET published_at = 'old' WHERE id = 'page-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO space_roles (id, space_id, name, permissions,created_by) VALUES ('custom:writer', 'space-a', 'Writer', '[\"view\",\"edit_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role = 'custom:writer' WHERE user_id = 'editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        super::publish_document(
            State(state.clone()),
            public_link_auth("editor-a"),
            Path(("space-a".into(), "page-a".into())),
        )
        .await
        .unwrap();
        let (action, target): (String, String) =
            sqlx::query_as("SELECT action, target_id FROM audit_log")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(action, "document.public_link_enable");
        assert_eq!(target, "page-a");
        let (public, slug): (i64, String) =
            sqlx::query_as("SELECT public, public_slug FROM documents WHERE id = 'page-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(public, 1);
        assert_ne!(slug, "link-a");
    }

    #[test]
    fn blank_documents_start_with_an_open_writing_canvas() {
        assert_eq!(DEFAULT_NEW_DOCUMENT_CONTENT, "");
    }

    #[test]
    fn document_storage_frontmatter_quotes_user_metadata_safely() {
        assert_eq!(
            document_storage_frontmatter(
                "Eagle's \"Launch\" Plan",
                Some("lucide:file-text"),
                false,
                true,
            ),
            "title: \"Eagle's \\\"Launch\\\" Plan\"\nicon: \"lucide:file-text\"\nlocked: false\npublic: true"
        );
    }

    fn duplicate_auth(user: &str) -> AuthUser {
        let mut auth = crate::spaces::properties::tests::test_auth(user);
        auth.claims.sid = format!("duplicate-{user}");
        auth
    }
    async fn duplicate_state() -> (AppState, String, String) {
        let state = crate::spaces::properties::tests::property_test_state().await;
        let slug = format!("duplicate-qa-{}", uuid::Uuid::new_v4());
        sqlx::query("UPDATE spaces SET slug=$1 WHERE id='space-a'")
            .bind(&slug)
            .execute(&state.db)
            .await
            .unwrap();
        for user in ["owner-a", "editor-a", "viewer-a"] {
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'hash','2999-01-01T00:00:00Z')").bind(format!("duplicate-{user}")).bind(user).execute(&state.db).await.unwrap();
        }
        let path = crate::storage::write_document(
            &slug,
            "target-a",
            "Source",
            "title: Source\nportable_field: preserved",
            "# Published\n[[Same title|stable-page-id]]",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE documents SET title='Source',icon='lucide:file-text',cover_image='cover',content_path=$1 WHERE id='target-a'").bind(&path).execute(&state.db).await.unwrap();
        (state, slug, path)
    }
    async fn duplicate_add_draft(state: &AppState, id: &str, owner: &str, content: &str) {
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,icon,cover_image,content,base_updated_at,updated_at) VALUES($1,'target-a','space-a',$2,$1,'draft-icon','draft-cover',$3,'base','same-time')").bind(id).bind(owner).bind(content).execute(&state.db).await.unwrap();
    }
    async fn duplicate_call(
        state: &AppState,
        user: &str,
        draft: Option<&str>,
    ) -> Result<axum::response::Response, AppError> {
        duplicate_document(
            State(state.clone()),
            duplicate_auth(user),
            Path(("space-a".into(), "target-a".into())),
            Json(DuplicateDocumentRequest {
                title: None,
                draft_id: draft.map(str::to_owned),
            }),
        )
        .await
        .map(IntoResponse::into_response)
    }
    async fn duplicate_assert_unchanged(state: &AppState, slug: &str, path: &str) {
        let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM documents),(SELECT count(*) FROM document_drafts WHERE document_id<>'target-a')").fetch_one(&state.db).await.unwrap();
        assert_eq!(counts, (2, 0));
        let entries = std::fs::read_dir(crate::storage::docs_dir(slug))
            .unwrap()
            .count();
        assert_eq!(entries, 1);
        assert!(crate::storage::read_document(path)
            .await
            .unwrap()
            .contains("# Published"));
    }

    #[tokio::test]
    async fn duplicate_oversized_legacy_draft_preserves_source_without_new_receipt() {
        let (state, slug, _) = duplicate_state().await;
        let content = "x".repeat(6_000_001);
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('large-draft','target-a','space-a','editor-a','Large',$1,'base')").bind(&content).execute(&state.db).await.unwrap();
        assert!(matches!(
            duplicate_call(&state, "editor-a", None).await,
            Err(crate::auth::routes::AppError::BadRequest(_))
        ));
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM documents")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 2);
        let receipts: i64 = sqlx::query_scalar("SELECT count(*) FROM storage_journal_receipts")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(receipts, 0);
        let stored: String =
            sqlx::query_scalar("SELECT content FROM document_drafts WHERE id='large-draft'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(stored, content);
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn duplicate_publication_collision_returns_committed_pending_copy_and_draft() {
        let (state, slug, _) = duplicate_state().await;
        crate::storage::journal::test_stage_callback(|m| {
            let path = crate::storage::data_dir().join(&m.path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"Unowned collision").unwrap();
        });
        let response = duplicate_call(&state, "editor-a", None).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::ACCEPTED);
        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(json["storage_pending"], true);
        let id = json["id"].as_str().unwrap();
        assert!(json["op_id"].as_str().is_some());
        let (path,content):(String,String)=sqlx::query_as("SELECT d.content_path,dd.content FROM documents d JOIN document_drafts dd ON dd.document_id=d.id WHERE d.id=$1 AND dd.id=$2").bind(id).bind(json["draft_id"].as_str().unwrap()).fetch_one(&state.db).await.unwrap();
        assert!(content.contains("# Published"));
        let readable = discovery_response(
            get_content(State(state.clone()), duplicate_auth("editor-a"), Path(("space-a".into(), id.into())))
                .await.unwrap(),
        ).await;
        assert!(readable["content"].as_str().unwrap().contains("# Published"));
        assert!(!readable["content"].as_str().unwrap().contains("Unowned collision"));
        assert_eq!(
            crate::storage::read_document(&path).await.unwrap(),
            "Unowned collision"
        );
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn duplicate_preserves_nested_location_and_portable_content() {
        let (state, slug, path) = duplicate_state().await;
        sqlx::query("INSERT INTO folders(id,space_id,name) VALUES('folder-a','space-a','Folder')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,folder_id,title,content_path,created_by,last_updated_by,published_at) VALUES('parent-a','space-a','folder-a','Parent','','owner-a','owner-a','published')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET folder_id='folder-a',parent_id='parent-a',public=1 WHERE id='target-a'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO property_values(id,document_id,property_id,value) VALUES('source-value','target-a','number-a','42')").execute(&state.db).await.unwrap();
        assert_eq!(
            duplicate_call(&state, "editor-a", None)
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::CREATED
        );
        let copy:(String,String,String,String,Option<String>,Option<String>,Option<String>,Option<String>,i64,i64,Option<String>)=sqlx::query_as("SELECT id,title,content_path,created_by,folder_id,parent_id,icon,cover_image,public,locked,published_at FROM documents WHERE id NOT IN ('target-a','target-b','parent-a')").fetch_one(&state.db).await.unwrap();
        assert!(uuid::Uuid::parse_str(&copy.0).is_ok());
        assert_eq!(copy.1, "Source (Copy)");
        assert_eq!(copy.3, "editor-a");
        assert_eq!(copy.4.as_deref(), Some("folder-a"));
        assert_eq!(copy.5.as_deref(), Some("parent-a"));
        assert_eq!(copy.6.as_deref(), Some("lucide:file-text"));
        assert_eq!(copy.7.as_deref(), Some("cover"));
        assert_eq!((copy.8, copy.9, copy.10), (0, 0, None));
        let copied = crate::storage::read_document(&copy.2).await.unwrap();
        assert!(copied.contains("portable_field: preserved"));
        assert!(copied.contains("[[Same title|stable-page-id]]"));
        let draft: (String, String, String) =
            sqlx::query_as("SELECT id,owner_id,content FROM document_drafts WHERE document_id=$1")
                .bind(&copy.0)
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert!(uuid::Uuid::parse_str(&draft.0).is_ok());
        assert_eq!(draft.1, "editor-a");
        assert_eq!(draft.2, crate::storage::read_document(&path).await.unwrap());
        let copied_values: i64 =
            sqlx::query_scalar("SELECT count(*) FROM property_values WHERE document_id=$1")
                .bind(&copy.0)
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(copied_values, 0);
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn duplicate_private_source_requires_exact_accessible_draft_selection() {
        let (state, slug, path) = duplicate_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL,content_path='missing-private-canonical' WHERE id='target-a'").execute(&state.db).await.unwrap();
        duplicate_add_draft(&state, "private-owner", "owner-a", "owner-secret").await;
        assert!(duplicate_call(&state, "editor-a", None).await.is_err());
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('private-owner','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        assert!(
            matches!(duplicate_call(&state,"editor-a",None).await,Err(AppError::BadRequest(message)) if message.contains("draft_id"))
        );
        assert!(duplicate_call(&state, "editor-a", Some("missing"))
            .await
            .is_err());
        duplicate_assert_unchanged(&state, &slug, &path).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('custom:copy','space-a','Copy','[\"view\",\"create_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='custom:copy' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(duplicate_call(&state, "editor-a", Some("private-owner"))
            .await
            .is_ok());
        let body: String =
            sqlx::query_scalar("SELECT content FROM document_drafts WHERE document_id<>'target-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(body, "owner-secret");
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn duplicate_prefers_deterministic_owned_draft_and_ignores_source_read_lock() {
        let (state, slug, _) = duplicate_state().await;
        duplicate_add_draft(&state, "owned-z", "editor-a", "z-body").await;
        duplicate_add_draft(&state, "owned-a", "editor-a", "a-body").await;
        sqlx::query("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(duplicate_call(&state, "editor-a", None).await.is_ok());
        assert!(duplicate_call(&state, "editor-a", Some("owned-z"))
            .await
            .is_ok());
        let bodies: Vec<String> = sqlx::query_scalar(
            "SELECT content FROM document_drafts WHERE document_id<>'target-a' ORDER BY content",
        )
        .fetch_all(&state.db)
        .await
        .unwrap();
        assert_eq!(bodies, vec!["a-body", "z-body"]);
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn duplicate_denies_lifecycle_actor_and_destination_without_storage_writes() {
        for sql in [
            "UPDATE documents SET archived_at='archive' WHERE id='target-a'",
            "UPDATE documents SET deleted_at='trash' WHERE id='target-a'",
            "UPDATE users SET is_active=0 WHERE id='editor-a'",
            "DELETE FROM sessions WHERE user_id='editor-a'",
            "UPDATE space_members SET role='viewer' WHERE user_id='editor-a'",
            "UPDATE documents SET folder_id='foreign-folder' WHERE id='target-a'",
        ] {
            let (state, slug, path) = duplicate_state().await;
            sqlx::query("INSERT INTO folders(id,space_id,name) VALUES('foreign-folder','space-b','Foreign')").execute(&state.db).await.unwrap();
            sqlx::query(sql).execute(&state.db).await.unwrap();
            assert!(
                duplicate_call(&state, "editor-a", None).await.is_err(),
                "{sql}"
            );
            duplicate_assert_unchanged(&state, &slug, &path).await;
            tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
                .await
                .unwrap();
        }
    }
    #[tokio::test]
    async fn duplicate_cleans_generated_file_on_sql_failure_and_late_revocation() {
        for trigger in ["CREATE TRIGGER deny_copy BEFORE INSERT ON document_drafts BEGIN SELECT RAISE(ABORT,'forced draft failure'); END", "CREATE TRIGGER deactivate_copy AFTER INSERT ON document_drafts WHEN NEW.document_id<>'target-a' BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END"] {
            let (state,slug,path)=duplicate_state().await;sqlx::query(trigger).execute(&state.db).await.unwrap();assert!(duplicate_call(&state,"editor-a",None).await.is_err());duplicate_assert_unchanged(&state,&slug,&path).await;
            let active:i64=sqlx::query_scalar("SELECT is_active FROM users WHERE id='editor-a'").fetch_one(&state.db).await.unwrap();assert_eq!(active,1);tokio::fs::remove_dir_all(crate::storage::space_dir(&slug)).await.unwrap();
        }
    }
    #[tokio::test]
    async fn duplicate_rechecks_current_session_after_waiting_for_transaction() {
        let (state, slug, path) = duplicate_state().await;
        let mut blocker = state.db.begin().await.unwrap();
        sqlx::query("DELETE FROM sessions WHERE user_id='editor-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let copied = state.clone();
        let mut pending =
            tokio::spawn(async move { duplicate_call(&copied, "editor-a", None).await });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut pending)
                .await
                .is_err()
        );
        blocker.commit().await.unwrap();
        assert!(pending.await.unwrap().is_err());
        duplicate_assert_unchanged(&state, &slug, &path).await;
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn duplicate_current_key_scopes_limit_source_and_creation() {
        let (state, slug, path) = duplicate_state().await;
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('copy-key','editor-a','Copy key','hash','prefix',0,'[\"read\"]')").execute(&state.db).await.unwrap();
        let auth = || {
            let mut auth = duplicate_auth("editor-a");
            auth.claims.sid = "api-key:copy-key".into();
            auth.api_key_scopes = Some(vec!["admin".into()]);
            auth
        };
        let request = || {
            Json(DuplicateDocumentRequest {
                title: None,
                draft_id: None,
            })
        };
        assert!(duplicate_document(
            State(state.clone()),
            auth(),
            Path(("space-a".into(), "target-a".into())),
            request()
        )
        .await
        .is_err());
        duplicate_assert_unchanged(&state, &slug, &path).await;
        sqlx::query("UPDATE api_keys SET scopes='[\"read\",\"write\"]' WHERE id='copy-key'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(duplicate_document(
            State(state.clone()),
            auth(),
            Path(("space-a".into(), "target-a".into())),
            request()
        )
        .await
        .is_ok());
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn duplicate_rolls_back_if_canonical_source_becomes_private_after_copy_write() {
        let (state, slug, path) = duplicate_state().await;
        duplicate_add_draft(&state, "shared-owner", "owner-a", "different-private-body").await;
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('shared-owner','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("CREATE TRIGGER privatize_source AFTER INSERT ON document_drafts WHEN NEW.document_id<>'target-a' BEGIN UPDATE documents SET published_at=NULL WHERE id='target-a'; END").execute(&state.db).await.unwrap();
        assert!(matches!(
            duplicate_call(&state, "editor-a", None).await,
            Err(AppError::Conflict(_))
        ));
        duplicate_assert_unchanged(&state, &slug, &path).await;
        let published: Option<String> =
            sqlx::query_scalar("SELECT published_at FROM documents WHERE id='target-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert!(published.is_some());
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn unpublished_document_list_visibility_follows_private_draft_access() {
        let state = discovery_test_state().await;
        sqlx::query("DELETE FROM documents")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO documents (
                id, space_id, title, content_path, created_by, last_updated_by, created_at, updated_at
             ) VALUES (
                'doc-1', 'space-a', 'Hidden shell', '', 'owner-a', 'owner-a',
                '2026-07-27T10:00:00Z', '2026-07-27T10:00:00Z'
             )",
        )
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO document_drafts (
                id, document_id, space_id, owner_id, title, content, base_updated_at,
                created_at, updated_at
             ) VALUES (
                'draft-1', 'doc-1', 'space-a', 'owner-a', 'Durable draft', 'private',
                '2026-07-27T10:00:00Z', '2026-07-27T10:00:00Z', '2026-07-27T10:00:00Z'
             )",
        )
        .execute(&state.db)
        .await
        .unwrap();

        let owner_response = list_documents(
            State(state.clone()),
            discovery_auth("owner-a"),
            Path("space-a".to_string()),
            root_document_query(),
        )
        .await
        .unwrap()
        .into_response();
        let owner_body = axum::body::to_bytes(owner_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let owner_documents: serde_json::Value = serde_json::from_slice(&owner_body).unwrap();
        assert_eq!(owner_documents[0]["id"], "doc-1");
        assert_eq!(owner_documents[0]["draft_id"], "draft-1");
        assert!(owner_documents[0]["published_at"].is_null());

        let hidden_response = list_documents(
            State(state.clone()),
            discovery_auth("viewer-a"),
            Path("space-a".to_string()),
            root_document_query(),
        )
        .await
        .unwrap()
        .into_response();
        let hidden_body = axum::body::to_bytes(hidden_response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&hidden_body).unwrap(),
            serde_json::json!([])
        );

        sqlx::query(
            "INSERT INTO document_draft_access (draft_id, user_id, role, granted_by) \
             VALUES ('draft-1', 'viewer-a', 'viewer', 'owner-a')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let shared_response = list_documents(
            State(state),
            discovery_auth("viewer-a"),
            Path("space-a".to_string()),
            root_document_query(),
        )
        .await
        .unwrap()
        .into_response();
        let shared_body = axum::body::to_bytes(shared_response.into_body(), usize::MAX)
            .await
            .unwrap();
        let shared_documents: serde_json::Value = serde_json::from_slice(&shared_body).unwrap();
        assert_eq!(shared_documents[0]["draft_id"], "draft-1");
    }

    async fn stars_json(state: &AppState, actor: &str) -> Result<serde_json::Value, AppError> {
        Ok(discovery_response(
            list_starred_documents(
                State(state.clone()),
                discovery_auth(actor),
                Path("space-a".into()),
            )
            .await?,
        )
        .await)
    }
    async fn seed_stars_private(state: &AppState) {
        for sql in [
            "UPDATE documents SET published_at=NULL,title='UNSHARED CANONICAL',icon='canonical' WHERE id='target-a'",
            "INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,icon,content,base_updated_at,updated_at) VALUES('selected','target-a','space-a','owner-a','Shared title','shared','shared secret','base','2999')",
            "INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('selected','viewer-a','viewer','owner-a')",
        ] {sqlx::query(sql).execute(&state.db).await.unwrap();}
    }
    #[tokio::test]
    async fn starred_unpublished_documents_follow_private_draft_access() {
        let state = discovery_test_state().await;
        seed_stars_private(&state).await;
        star_document(
            State(state.clone()),
            discovery_auth("viewer-a"),
            Path(("space-a".into(), "target-a".into())),
        )
        .await
        .unwrap();
        let list = stars_json(&state, "viewer-a").await.unwrap();
        assert_eq!(list[0]["title"], "Shared title");
        assert_eq!(list[0]["icon"], "shared");
        assert!(!list.to_string().contains("UNSHARED CANONICAL"));
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('private-parent','space-a','Parent','','owner-a','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET parent_id='private-parent' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            stars_json(&state, "viewer-a").await.unwrap(),
            serde_json::json!([])
        );
        assert!(star_document(
            State(state.clone()),
            discovery_auth("viewer-a"),
            Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_err());
        sqlx::query("UPDATE documents SET parent_id=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        for life in ["archived_at", "deleted_at"] {
            sqlx::query(&format!(
                "UPDATE documents SET {life}='old' WHERE id='target-a'"
            ))
            .execute(&state.db)
            .await
            .unwrap();
            assert_eq!(
                stars_json(&state, "viewer-a").await.unwrap(),
                serde_json::json!([])
            );
            assert!(star_document(
                State(state.clone()),
                discovery_auth("viewer-a"),
                Path(("space-a".into(), "target-a".into()))
            )
            .await
            .is_err());
            sqlx::query(&format!(
                "UPDATE documents SET {life}=NULL WHERE id='target-a'"
            ))
            .execute(&state.db)
            .await
            .unwrap();
        }
        sqlx::query("PRAGMA ignore_check_constraints=ON")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE document_draft_access SET role='unknown' WHERE draft_id='selected'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(matches!(
            stars_json(&state, "viewer-a").await,
            Err(AppError::Conflict(_))
        ));
    }
    #[tokio::test]
    async fn stars_scoped_cleanup_and_late_denial_preserve_full_database() {
        let state = discovery_test_state().await;
        star_document(
            State(state.clone()),
            discovery_auth("viewer-a"),
            Path(("space-a".into(), "target-a".into())),
        )
        .await
        .unwrap();
        sqlx::query("INSERT INTO starred_documents(user_id,document_id) VALUES('viewer-a','target-b'),('owner-a','target-a')").execute(&state.db).await.unwrap();
        for (space, doc) in [
            ("space-a", "target-b"),
            ("missing", "target-a"),
            ("space-a", "missing"),
        ] {
            let before = move_database_snapshot(&state).await;
            super::unstar_document(
                State(state.clone()),
                discovery_auth("viewer-a"),
                Path((space.into(), doc.into())),
            )
            .await
            .unwrap();
            assert_eq!(move_database_snapshot(&state).await, before);
        }
        sqlx::query("UPDATE documents SET deleted_at='old' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')").execute(&state.db).await.unwrap();
        super::unstar_document(
            State(state.clone()),
            discovery_auth("viewer-a"),
            Path(("space-a".into(), "target-a".into())),
        )
        .await
        .unwrap();
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM starred_documents WHERE document_id='target-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(count, 1);
        sqlx::query("DELETE FROM space_user_blocks")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET deleted_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER revoke_star AFTER INSERT ON starred_documents BEGIN DELETE FROM sessions WHERE id='discovery-session-viewer-a'; END").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(star_document(
            State(state.clone()),
            discovery_auth("viewer-a"),
            Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
        sqlx::query("DROP TRIGGER revoke_star")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER revoke_unstar AFTER DELETE ON starred_documents BEGIN UPDATE users SET is_active=0 WHERE id=OLD.user_id; END").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(super::unstar_document(
            State(state.clone()),
            discovery_auth("viewer-a"),
            Path(("space-b".into(), "target-b".into()))
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn stars_real_http_keys_keep_read_and_write_ceilings_without_edit_permission() {
        use tower::ServiceExt;
        let state = discovery_test_state().await;
        seed_stars_private(&state).await;
        for (id, scopes) in [("read", "[\"read\"]"), ("write", "[\"write\"]")] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'viewer-a',$1,$2,'test',$3)").bind(id).bind(crate::auth::api_keys::hash_api_key(id)).bind(scopes).execute(&state.db).await.unwrap();
        }
        for (method, key, path, success) in [
            (
                "POST",
                "write",
                "/spaces/space-a/documents/target-a/star",
                true,
            ),
            ("GET", "read", "/spaces/space-a/documents/starred", true),
            ("GET", "write", "/spaces/space-a/documents/starred", false),
            (
                "POST",
                "read",
                "/spaces/space-a/documents/target-a/star",
                false,
            ),
            (
                "DELETE",
                "read",
                "/spaces/space-a/documents/target-a/star",
                false,
            ),
            (
                "DELETE",
                "write",
                "/spaces/space-a/documents/target-a/star",
                true,
            ),
        ] {
            let response = super::router()
                .with_state(state.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .method(method)
                        .uri(path)
                        .header("X-API-Key", key)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                response.status().is_success(),
                success,
                "{method} {key} {path}: {}",
                response.status()
            );
        }
    }
    #[tokio::test]
    async fn stars_final_read_observes_external_wal_revocation_and_exact_source_loss() {
        for variant in ["session", "block", "unpublish", "share"] {
            let mut state = discovery_test_state().await;
            let file = std::env::temp_dir().join(format!("stars-wal-{}.db", uuid::Uuid::new_v4()));
            state.db = sqlx::any::AnyPoolOptions::new()
                .max_connections(2)
                .after_connect(|conn, _| {
                    Box::pin(async move {
                        sqlx::query("PRAGMA journal_mode=WAL")
                            .execute(&mut *conn)
                            .await?;
                        sqlx::query("PRAGMA busy_timeout=5000")
                            .execute(conn)
                            .await?;
                        Ok(())
                    })
                })
                .connect(&format!("sqlite://{}?mode=rwc", file.display()))
                .await
                .unwrap();
            sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
            for sql in [
                "INSERT INTO users(id,email,display_name,role) VALUES('owner-a','owner@example.test','Owner','member'),('viewer-a','viewer@example.test','Viewer','member'),('editor-a','editor@example.test','Editor','member')",
                "INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('discovery-session-viewer-a','viewer-a','test','2999-01-01T00:00:00Z')",
                "INSERT INTO spaces(id,name,slug,created_by) VALUES('space-a','Space','space-a','owner-a')",
                "INSERT INTO space_members(id,space_id,user_id,role) VALUES('viewer','space-a','viewer-a','viewer')",
                "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','UNSHARED CANONICAL','','owner-a','owner-a','old')",
                "INSERT INTO starred_documents(user_id,document_id) VALUES('viewer-a','target-a')",
            ] {sqlx::query(sql).execute(&state.db).await.unwrap();}
            if variant == "share" {
                seed_stars_private(&state).await;
            }
            if variant == "share" || variant == "unpublish" {
                sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES('other','target-a','space-a','editor-a','Other','other secret','base','2000')").execute(&state.db).await.unwrap();
                sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('other','viewer-a','viewer','owner-a')").execute(&state.db).await.unwrap();
            }
            let change=match variant {
                "session"=>"DELETE FROM sessions WHERE user_id='viewer-a'",
                "block"=>"INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')",
                "share"=>"DELETE FROM document_draft_access WHERE draft_id='selected' AND user_id='viewer-a'",
                _=>"UPDATE documents SET published_at=NULL WHERE id='target-a'",
            };
            let db = state.db.clone();
            super::STARS_BEFORE_FINAL.with(|slot| {
                *slot.borrow_mut() = Some(Box::pin(async move {
                    sqlx::query(change).execute(&db).await.unwrap();
                }))
            });
            let result = stars_json(&state, "viewer-a").await;
            if variant == "session" || variant == "block" {
                assert!(result.is_err());
            } else {
                assert_eq!(result.unwrap(), serde_json::json!([]), "{variant}");
                let mut conn = state.db.acquire().await.unwrap();
                super::require_star_operation(
                    &mut conn,
                    &discovery_auth("viewer-a"),
                    "space-a",
                    "target-a",
                    super::super::access::PageOperation::View,
                )
                .await
                .unwrap();
            }
            state.db.close().await;
            std::fs::remove_file(file).unwrap();
        }
    }

    async fn creation_test_state() -> (AppState, String) {
        let state = crate::spaces::properties::tests::property_test_state().await;
        let slug = format!("create-qa-{}", uuid::Uuid::new_v4());
        sqlx::query("UPDATE spaces SET slug=$1 WHERE id='space-a'")
            .bind(&slug)
            .execute(&state.db)
            .await
            .unwrap();
        (state, slug)
    }

    async fn attempt_creation(
        state: &AppState,
        payload: serde_json::Value,
    ) -> Result<axum::response::Response, crate::auth::routes::AppError> {
        let request: super::CreateDocumentRequest = serde_json::from_value(payload).unwrap();
        super::create_document(
            State(state.clone()),
            crate::spaces::properties::tests::test_auth("editor-a"),
            Path("space-a".into()),
            Json(request),
        )
        .await
        .map(IntoResponse::into_response)
    }

    #[tokio::test]
    async fn page_creation_collision_returns_committed_pending_ids_without_overwrite() {
        let (state, slug) = creation_test_state().await;
        let id = uuid::Uuid::new_v4().to_string();
        let dir = crate::storage::docs_dir(&slug);
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join(crate::storage::document_filename("Pending", &id));
        tokio::fs::write(&path, b"Unowned existing file")
            .await
            .unwrap();
        let response = attempt_creation(
            &state,
            serde_json::json!({"title":"Pending","source_id":id,"content":"Saved private draft"}),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::ACCEPTED);
        let json: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(json["id"], id);
        assert_eq!(json["storage_pending"], true);
        assert!(json["op_id"].as_str().is_some());
        let content: String = sqlx::query_scalar(
            "SELECT content FROM document_drafts WHERE id=$1 AND document_id=$2",
        )
        .bind(json["draft_id"].as_str().unwrap())
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(content, "Saved private draft");
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('journal-content-editor','editor-a','hash','2999-01-01T00:00:00Z')")
            .execute(&state.db).await.unwrap();
        let mut auth = test_auth("editor-a");
        auth.claims.sid = "journal-content-editor".into();
        let readable = discovery_response(
            get_content(State(state.clone()), auth, Path(("space-a".into(), id.clone())))
                .await.unwrap(),
        ).await;
        assert!(readable["content"].as_str().unwrap().contains("Saved private draft"));
        assert!(!readable["content"].as_str().unwrap().contains("Unowned existing file"));
        assert_eq!(
            tokio::fs::read(&path).await.unwrap(),
            b"Unowned existing file"
        );
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn page_creation_validates_template_and_explicit_values_before_storage() {
        let (state, slug) = creation_test_state().await;
        sqlx::query("INSERT INTO templates(id,space_id,name,content,properties,tags,created_by) VALUES ('invalid-template','space-a','Invalid','','{\"Count\":\"not-a-number\"}','[]','owner-a')").execute(&state.db).await.unwrap();
        for payload in [
            serde_json::json!({"title":"Invalid","property_values":[{"property_id":"number-a","value":"bad"}]}),
            serde_json::json!({"title":"Invalid","property_values":[{"property_id":"foreign","value":"secret"}]}),
            serde_json::json!({"title":"Invalid","template_id":"invalid-template"}),
            serde_json::json!({"title":"Invalid","property_values":[{"property_id":"number-a","value":"1"},{"property_id":"number-a","value":"2"}]}),
        ] {
            assert!(matches!(
                attempt_creation(&state, payload).await,
                Err(crate::auth::routes::AppError::BadRequest(_))
            ));
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(count, 2);
            assert!(
                !crate::storage::space_dir(&slug).exists(),
                "invalid metadata must not create storage"
            );
        }
    }

    #[tokio::test]
    async fn page_creation_rejects_corrupt_template_json_before_storage() {
        for (properties, tags) in [("{", "[]"), ("[]", "[]"), ("{}", "{"), ("{}", "[1]")] {
            let (state, slug) = creation_test_state().await;
            sqlx::query("INSERT INTO templates(id,space_id,name,content,properties,tags,created_by) VALUES ('bad','space-a','Bad','',$1,$2,'owner-a')").bind(properties).bind(tags).execute(&state.db).await.unwrap();
            assert!(matches!(
                attempt_creation(
                    &state,
                    serde_json::json!({"title":"Invalid","template_id":"bad"})
                )
                .await,
                Err(crate::auth::routes::AppError::BadRequest(_))
            ));
            assert!(!crate::storage::space_dir(&slug).exists());
        }
    }

    #[tokio::test]
    async fn page_creation_rejects_foreign_inactive_private_locked_and_cyclic_parents() {
        for case in [
            "foreign",
            "archived",
            "deleted",
            "private",
            "locked",
            "cycle",
            "foreign-ancestor",
            "folder-mismatch",
        ] {
            let (state, slug) = creation_test_state().await;
            let parent = if case == "foreign" {
                "target-b"
            } else {
                "target-a"
            };
            match case {
                "archived" => {
                    sqlx::query("UPDATE documents SET archived_at='now' WHERE id='target-a'")
                        .execute(&state.db)
                        .await
                        .unwrap();
                }
                "deleted" => {
                    sqlx::query("UPDATE documents SET deleted_at='now' WHERE id='target-a'")
                        .execute(&state.db)
                        .await
                        .unwrap();
                }
                "private" => {
                    sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
                        .execute(&state.db)
                        .await
                        .unwrap();
                }
                "locked" => {
                    sqlx::query(
                        "UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'",
                    )
                    .execute(&state.db)
                    .await
                    .unwrap();
                }
                "cycle" => {
                    sqlx::query("UPDATE documents SET parent_id='target-a' WHERE id='target-a'")
                        .execute(&state.db)
                        .await
                        .unwrap();
                }
                "foreign-ancestor" => {
                    sqlx::query("UPDATE documents SET parent_id='target-b' WHERE id='target-a'")
                        .execute(&state.db)
                        .await
                        .unwrap();
                }
                "folder-mismatch" => {
                    sqlx::query("INSERT INTO folders(id,space_id,name) VALUES ('folder','space-a','Folder')").execute(&state.db).await.unwrap();
                    sqlx::query("UPDATE documents SET folder_id='folder' WHERE id='target-a'")
                        .execute(&state.db)
                        .await
                        .unwrap();
                }
                _ => {}
            }
            assert!(
                attempt_creation(
                    &state,
                    serde_json::json!({"title":"Child","parent_id":parent})
                )
                .await
                .is_err(),
                "{case}"
            );
            assert!(!crate::storage::space_dir(&slug).exists(), "{case}");
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_drafts")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(count, 0, "{case}");
        }
    }

    #[tokio::test]
    async fn page_creation_preserves_custom_editor_template_overrides_and_owned_draft() {
        let (state, slug) = creation_test_state().await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES ('custom:create','space-a','Creator','[\"view\",\"edit_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='custom:create' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO tags(id,space_id,name) VALUES ('tag','space-a','Release')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO templates(id,space_id,name,content,properties,tags,created_by) VALUES ('good','space-a','Good','','{\"Count\":1,\"Unbound\":\"ignored\",\"__starlane_test\":true}','[\"release\"]','owner-a')").execute(&state.db).await.unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        let response = tokio::time::timeout(std::time::Duration::from_secs(3), attempt_creation(&state, serde_json::json!({"title":"Child","parent_id":"target-a","template_id":"good","source_id":id,"content":"Body","property_values":[{"property_id":"number-a","value":"2"}]}))).await.expect("single-connection creation must not acquire another pool connection").unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::CREATED);
        let row: (String, String, Option<String>, String, String) = sqlx::query_as("SELECT d.id,d.created_by,d.published_at,v.owner_id,v.content FROM documents d JOIN document_drafts v ON v.document_id=d.id WHERE d.id=$1").bind(&id).fetch_one(&state.db).await.unwrap();
        assert_eq!(
            row,
            (
                id.clone(),
                "editor-a".into(),
                None,
                "editor-a".into(),
                "Body".into()
            )
        );
        let value: String =
            sqlx::query_scalar("SELECT value FROM property_values WHERE document_id=$1")
                .bind(&id)
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(value, "2");
        let tag: String = sqlx::query_scalar("SELECT tag_id FROM doc_tags WHERE document_id=$1")
            .bind(&id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(tag, "tag");
        let path: String = sqlx::query_scalar("SELECT content_path FROM documents WHERE id=$1")
            .bind(&id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert!(crate::storage::data_dir().join(&path).exists());
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn page_creation_rechecks_parent_edges_after_document_lock() {
        let (state, slug) = creation_test_state().await;
        sqlx::query("CREATE TRIGGER retarget_parent AFTER UPDATE OF id ON documents WHEN NEW.id='target-a' BEGIN UPDATE documents SET parent_id='target-b' WHERE id='target-a'; END").execute(&state.db).await.unwrap();
        assert!(matches!(
            attempt_creation(
                &state,
                serde_json::json!({"title":"Child","parent_id":"target-a"})
            )
            .await,
            Err(crate::auth::routes::AppError::Conflict(_))
        ));
        let parent: Option<String> =
            sqlx::query_scalar("SELECT parent_id FROM documents WHERE id='target-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(parent, None);
        assert!(!crate::storage::space_dir(&slug).exists());
    }

    #[tokio::test]
    async fn private_parent_requires_explicit_editor_participation() {
        let (state, _) = creation_test_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL,locked=1,locked_by='editor-a' WHERE id='target-a'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES ('parent-draft','target-a','space-a','owner-a','Private','base')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ('parent-draft','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        let mut tx = state.db.begin().await.unwrap();
        super::super::mutation_guards::lock_spaces_for_mutation(&mut tx, &["space-a"])
            .await
            .unwrap();
        assert!(matches!(
            super::validate_create_parent_on_connection(
                &mut tx,
                "space-a",
                Some("target-a"),
                None,
                super::super::access::principal_from_auth(
                    &crate::spaces::properties::tests::test_auth("editor-a")
                )
                .unwrap()
            )
            .await,
            Err(crate::auth::routes::AppError::NotFound(_))
        ));
        tx.rollback().await.unwrap();
        sqlx::query("UPDATE document_draft_access SET role='editor' WHERE draft_id='parent-draft'")
            .execute(&state.db)
            .await
            .unwrap();
        let mut tx = state.db.begin().await.unwrap();
        super::super::mutation_guards::lock_spaces_for_mutation(&mut tx, &["space-a"])
            .await
            .unwrap();
        super::validate_create_parent_on_connection(
            &mut tx,
            "space-a",
            Some("target-a"),
            None,
            super::super::access::principal_from_auth(
                &crate::spaces::properties::tests::test_auth("editor-a"),
            )
            .unwrap(),
        )
        .await
        .unwrap();
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn page_creation_rejects_insert_retargeting_and_rolls_back() {
        let (state, slug) = creation_test_state().await;
        sqlx::query("CREATE TRIGGER retarget_created AFTER INSERT ON documents BEGIN UPDATE documents SET parent_id='target-b' WHERE id=NEW.id; END").execute(&state.db).await.unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        assert!(matches!(
            attempt_creation(
                &state,
                serde_json::json!({"title":"Rollback","source_id":id})
            )
            .await,
            Err(crate::auth::routes::AppError::Conflict(_))
        ));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE id=$1")
            .bind(&id)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
        assert!(
            !crate::storage::space_dir(&slug).exists(),
            "failed canonical insert must not publish storage"
        );
    }

    #[tokio::test]
    async fn page_creation_final_authority_denial_rolls_back_all_database_rows() {
        let (state, slug) = creation_test_state().await;
        sqlx::query("CREATE TRIGGER revoke_at_create AFTER INSERT ON documents BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END").execute(&state.db).await.unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        assert!(matches!(attempt_creation(&state, serde_json::json!({"title":"Rollback","source_id":id,"property_values":[{"property_id":"number-a","value":"2"}]})).await, Err(crate::auth::routes::AppError::Unauthorized(_))));
        for table in [
            "documents",
            "document_drafts",
            "doc_subscriptions",
            "property_values",
        ] {
            let column = if table == "documents" {
                "id"
            } else {
                "document_id"
            };
            let count: i64 =
                sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE {column}=$1"))
                    .bind(&id)
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(count, 0, "{table}");
        }
        let active: i64 = sqlx::query_scalar("SELECT is_active FROM users WHERE id='editor-a'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(active, 1);
        assert!(
            !crate::storage::space_dir(&slug).exists(),
            "late denial leaves only a private journal stage, no canonical file"
        );
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM storage_journal_receipts WHERE document_id=$1",
        )
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(receipts, 0);
    }

    #[tokio::test]
    async fn template_field_assignments_override_defaults_and_reject_foreign_schema() {
        let state = document_list_test_state().await;
        sqlx::query(
            "CREATE TABLE properties_schema (id TEXT PRIMARY KEY, space_id TEXT, name TEXT)",
        )
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO properties_schema (id, space_id, name) VALUES ('chef', 'space-1', 'Chef'), ('foreign', 'space-2', 'Secret')")
            .execute(&state.db).await.unwrap();
        let mut metadata = super::TemplateMetadata {
            tags: Vec::new(),
            property_values: vec![("chef".into(), "Default".into())],
        };
        super::apply_create_property_values(
            &state,
            "space-1",
            &[super::CreatePropertyValue {
                property_id: "chef".into(),
                value: "Eagle".into(),
            }],
            &mut metadata,
        )
        .await
        .unwrap();
        assert_eq!(
            metadata.property_values,
            vec![("chef".to_string(), "Eagle".to_string())]
        );
        let failure = super::apply_create_property_values(
            &state,
            "space-1",
            &[super::CreatePropertyValue {
                property_id: "foreign".into(),
                value: "Secret".into(),
            }],
            &mut metadata,
        )
        .await;
        assert!(matches!(
            failure,
            Err(crate::auth::routes::AppError::BadRequest(_))
        ));
        assert_eq!(metadata.property_values.len(), 1);
    }

    #[test]
    fn document_lock_and_unlock_notifications_use_their_distinct_preference_types() {
        assert_eq!(
            document_lock_notification_copy("Launch plan", "Eagle", true),
            (
                "doc_locked",
                "Document locked: Launch plan".to_string(),
                "Eagle locked Launch plan.".to_string(),
            ),
        );
        assert_eq!(
            document_lock_notification_copy("Launch plan", "Eagle", false),
            (
                "doc_unlocked",
                "Document unlocked: Launch plan".to_string(),
                "Eagle unlocked Launch plan.".to_string(),
            ),
        );
    }

    #[test]
    fn portable_document_mentions_extract_only_bounded_member_ids() {
        assert_eq!(
            portable_mention_user_ids(
                "Ask @[Eagle](user-1), @[Simmons](user_2), then @[Eagle](user-1).",
            ),
            ["user-1".to_string(), "user_2".to_string()]
                .into_iter()
                .collect(),
        );
        assert!(portable_mention_user_ids("@[No id]() @[Unsafe](user/path)").is_empty());
    }

    #[test]
    fn unchanged_mentions_do_not_retrigger_on_document_autosave() {
        assert_eq!(
            newly_added_portable_mention_user_ids(
                "Ask @[Eagle](user-1).",
                "Ask @[Eagle](user-1) and @[Simmons](user-2).",
            ),
            ["user-2".to_string()].into_iter().collect(),
        );
        assert!(newly_added_portable_mention_user_ids(
            "Ask @[Eagle](user-1).",
            "Ask @[Eagle](user-1).",
        )
        .is_empty());
    }
    #[tokio::test]
    async fn creation_relation_target_final_authority_rolls_back_page_and_metadata() {
        let (state, slug) = creation_test_state().await;
        for q in ["UPDATE documents SET published_at=NULL WHERE id='target-a'", "INSERT INTO document_drafts(id,space_id,document_id,owner_id,title,base_updated_at) VALUES('relation-create-draft','space-a','target-a','owner-a','Private','old')", "INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('relation-create-draft','editor-a','viewer','owner-a')", "CREATE TRIGGER revoke_creation_relation AFTER INSERT ON property_values BEGIN DELETE FROM document_draft_access WHERE draft_id='relation-create-draft'; END"] {sqlx::query(q).execute(&state.db).await.unwrap();}
        let id = uuid::Uuid::new_v4().to_string();
        let request:super::CreateDocumentRequest=serde_json::from_value(serde_json::json!({"title":"Relation rollback","source_id":id,"property_values":[{"property_id":"relation-a","value":"target-a"}]})).unwrap();
        let result = super::create_document(
            State(state.clone()),
            crate::spaces::properties::tests::test_auth("editor-a"),
            Path("space-a".into()),
            Json(request),
        )
        .await;
        assert!(
            matches!(result, Err(crate::auth::routes::AppError::BadRequest(_))),
            "creation must fail on revoked target authority"
        );
        for (table, column) in [
            ("documents", "id"),
            ("document_drafts", "document_id"),
            ("property_values", "document_id"),
        ] {
            let count: i64 =
                sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE {column}=$1"))
                    .bind(&id)
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(count, 0);
        }
        let grants: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM document_draft_access WHERE draft_id='relation-create-draft'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(grants, 1);
        assert!(!crate::storage::docs_dir(&slug)
            .join(crate::storage::document_filename("Relation rollback", &id))
            .exists());
    }
    async fn create_as(
        state: &AppState,
        auth: &AuthUser,
        payload: serde_json::Value,
    ) -> Result<axum::response::Response, crate::auth::routes::AppError> {
        super::create_document(
            State(state.clone()),
            crate::spaces::properties::tests::copy_auth(auth),
            Path("space-a".into()),
            Json(serde_json::from_value::<super::CreateDocumentRequest>(payload).unwrap()),
        )
        .await
        .map(IntoResponse::into_response)
    }
    async fn creation_only_role(state: &AppState, permissions: &str) {
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('custom:creation','space-a','Creation',$1,'owner-a')").bind(permissions).execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='custom:creation' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn rest_creation_exact_create_permission_not_edit_and_actual_role_union() {
        let (state, slug) = creation_test_state().await;
        creation_only_role(&state, r#"["create_content"]"#).await;
        let auth = crate::spaces::properties::tests::test_auth("editor-a");
        for properties in [
            serde_json::json!([]),
            serde_json::json!([{"property_id":"number-a","value":"12"}]),
        ] {
            let id = uuid::Uuid::new_v4().to_string();
            let response=create_as(&state,&auth,serde_json::json!({"title":"Create only","source_id":id,"property_values":properties})).await.unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::CREATED);
            let owner: String =
                sqlx::query_scalar("SELECT owner_id FROM document_drafts WHERE document_id=$1")
                    .bind(&id)
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(owner, "editor-a");
        }
        // Local legacy EditContent expands to creation. Granular GLOBAL roles do not.
        for q in ["UPDATE space_roles SET permissions='[\"view\",\"comment\"]' WHERE id='custom:creation'", "INSERT INTO groups(id,name) VALUES('creation-group','Creation')", "INSERT INTO group_members(id,group_id,user_id) VALUES('creation-member','creation-group','editor-a')", "UPDATE global_roles SET permissions='[\"edit_content\"]' WHERE id='contributor'", "INSERT INTO group_global_roles(group_id,role_id) VALUES('creation-group','contributor')"] {sqlx::query(q).execute(&state.db).await.unwrap();}
        assert!(matches!(
            create_as(&state, &auth, serde_json::json!({"title":"Edit only"})).await,
            Err(crate::auth::routes::AppError::NotFound(_))
        ));
        sqlx::query("UPDATE global_roles SET permissions='[\"view\"]' WHERE id='admin'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE group_global_roles SET role_id='admin' WHERE group_id='creation-group'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let mut stale_admin = crate::spaces::properties::tests::copy_auth(&auth);
        stale_admin.claims.role = "admin".into();
        assert!(create_as(
            &state,
            &stale_admin,
            serde_json::json!({"title":"Admin label is not authority"})
        )
        .await
        .is_err());
        sqlx::query(
            "UPDATE group_global_roles SET role_id='contributor' WHERE group_id='creation-group'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query(
            "UPDATE global_roles SET permissions='[\"create_content\"]' WHERE id='contributor'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert!(
            create_as(&state, &auth, serde_json::json!({"title":"Union"}))
                .await
                .is_ok()
        );
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn rest_creation_http_write_key_and_late_current_key_scope_are_exact() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let (state, slug) = creation_test_state().await;
        let raw = "creation-http-key";
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('creation-key','editor-a','Create',$1,'prefix','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key(raw)).execute(&state.db).await.unwrap();
        let app = super::router().with_state(state.clone());
        let request = || {
            Request::builder()
                .method("POST")
                .uri("/spaces/space-a/documents")
                .header("X-API-Key", raw)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"HTTP create"}"#))
                .unwrap()
        };
        assert_eq!(
            app.clone().oneshot(request()).await.unwrap().status(),
            axum::http::StatusCode::CREATED
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
            .fetch_one(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='creation-key'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            app.clone().oneshot(request()).await.unwrap().status(),
            axum::http::StatusCode::FORBIDDEN
        );
        sqlx::query("UPDATE api_keys SET scopes='[\"write\"]' WHERE id='creation-key'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER creation_key_downgrade AFTER INSERT ON document_drafts BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='creation-key'; END").execute(&state.db).await.unwrap();
        assert!(!app.oneshot(request()).await.unwrap().status().is_success());
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(after, count);
        let scopes: String =
            sqlx::query_scalar("SELECT scopes FROM api_keys WHERE id='creation-key'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(scopes, r#"["write"]"#);
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn rest_creation_late_session_and_initial_draft_ownership_changes_roll_back() {
        for change in [
            "DELETE FROM sessions WHERE id='metadata-editor-a'",
            "UPDATE document_drafts SET owner_id='owner-a' WHERE id=NEW.id",
            "UPDATE documents SET public=1 WHERE id=NEW.document_id",
            "UPDATE document_drafts SET title='Changed initial title',content='Changed private body' WHERE id=NEW.id",
        ] {
            let (state, slug) = creation_test_state().await;
            sqlx::query(&format!(
                "CREATE TRIGGER creation_late AFTER INSERT ON document_drafts BEGIN {change}; END"
            ))
            .execute(&state.db)
            .await
            .unwrap();
            let id = uuid::Uuid::new_v4().to_string();
            assert!(create_as(
                &state,
                &crate::spaces::properties::tests::test_auth("editor-a"),
                serde_json::json!({"title":"Rollback","source_id":id})
            )
            .await
            .is_err());
            for (table, column) in [
                ("documents", "id"),
                ("document_drafts", "document_id"),
                ("storage_journal_receipts", "document_id"),
            ] {
                let count: i64 =
                    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE {column}=$1"))
                        .bind(&id)
                        .fetch_one(&state.db)
                        .await
                        .unwrap();
                assert_eq!(count, 0);
            }
            let session: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE id='metadata-editor-a'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(session, 1);
            assert!(!crate::storage::docs_dir(&slug)
                .join(crate::storage::document_filename("Rollback", &id))
                .exists());
        }
    }
    #[tokio::test]
    async fn rest_creation_current_session_token_activity_and_service_are_not_fabricated() {
        for change in [
            "DELETE FROM sessions WHERE id='metadata-editor-a'",
            "UPDATE users SET is_active=0 WHERE id='editor-a'",
            "UPDATE sessions SET expires_at='2000-01-01T00:00:00Z' WHERE id='metadata-editor-a'",
        ] {
            let (state, slug) = creation_test_state().await;
            sqlx::query(change).execute(&state.db).await.unwrap();
            assert!(create_as(
                &state,
                &crate::spaces::properties::tests::test_auth("editor-a"),
                serde_json::json!({"title":"Denied"})
            )
            .await
            .is_err());
            assert!(!crate::storage::space_dir(&slug).exists());
        }
        let (state, slug) = creation_test_state().await;
        let mut auth = crate::spaces::properties::tests::test_auth("editor-a");
        auth.claims.exp = 0;
        assert!(
            create_as(&state, &auth, serde_json::json!({"title":"Expired"}))
                .await
                .is_err()
        );
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('creation-service',NULL,'Service','hash','prefix',1,'[\"admin\"]')").execute(&state.db).await.unwrap();
        auth.claims.exp = i64::MAX;
        auth.claims.sub = "service:creation-service".into();
        auth.claims.sid = "service:creation-service".into();
        auth.api_key_scopes = Some(vec!["admin".into()]);
        assert!(
            create_as(&state, &auth, serde_json::json!({"title":"Service"}))
                .await
                .is_err()
        );
        assert!(!crate::storage::space_dir(&slug).exists());
    }
    #[tokio::test]
    async fn rest_creation_wait_observes_real_wal_committed_credential_revocation() {
        let (mut state, _) = creation_test_state().await;
        let file =
            std::env::temp_dir().join(format!("creation-wal-{}.sqlite", uuid::Uuid::new_v4()));
        state.db = sqlx::any::AnyPoolOptions::new()
            .max_connections(2)
            .after_connect(|conn, _| {
                Box::pin(async move {
                    sqlx::query("PRAGMA journal_mode=WAL")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA busy_timeout=5000")
                        .execute(conn)
                        .await?;
                    Ok(())
                })
            })
            .connect(&format!("sqlite://{}?mode=rwc", file.display()))
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
        for q in ["INSERT INTO users(id,email,display_name,role) VALUES('editor-a','editor@create.test','Editor','member')", "INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('metadata-editor-a','editor-a','test','2999-01-01T00:00:00Z')", "INSERT INTO spaces(id,name,slug,created_by) VALUES('space-a','A','creation-wal','editor-a')", "INSERT INTO space_members(id,space_id,user_id,role) VALUES('editor','space-a','editor-a','editor')"]{sqlx::query(q).execute(&state.db).await.unwrap();}
        let mut blocker = state.db.begin().await.unwrap();
        super::super::mutation_guards::lock_spaces_for_mutation(&mut blocker, &["space-a"])
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE id='metadata-editor-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let queued_state = state.clone();
        let mut queued = tokio::spawn(async move {
            create_as(
                &queued_state,
                &crate::spaces::properties::tests::test_auth("editor-a"),
                serde_json::json!({"title":"Wait"}),
            )
            .await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut queued)
                .await
                .is_err()
        );
        blocker.commit().await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(5), queued)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
        state.db.close().await;
        std::fs::remove_file(file).unwrap();
    }

    #[tokio::test]
    async fn duplicate_parent_typed_edit_preserves_private_editor_and_content_lock_contract() {
        let (state, slug, path) = duplicate_state().await;
        for q in ["INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('copy-parent','space-a','Parent','','owner-a','owner-a')", "UPDATE documents SET parent_id='copy-parent' WHERE id='target-a'", "INSERT INTO document_drafts(id,space_id,document_id,owner_id,title,base_updated_at) VALUES('copy-parent-draft','space-a','copy-parent','owner-a','Private','old')", "INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('copy-parent-draft','editor-a','viewer','owner-a')"]{sqlx::query(q).execute(&state.db).await.unwrap();}
        let copy = || super::DuplicateDocumentRequest {
            title: None,
            draft_id: None,
        };
        assert!(super::duplicate_document(
            State(state.clone()),
            duplicate_auth("editor-a"),
            Path(("space-a".into(), "target-a".into())),
            Json(copy())
        )
        .await
        .is_err());
        sqlx::query(
            "UPDATE document_draft_access SET role='editor' WHERE draft_id='copy-parent-draft'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert!(super::duplicate_document(
            State(state.clone()),
            duplicate_auth("editor-a"),
            Path(("space-a".into(), "target-a".into())),
            Json(copy())
        )
        .await
        .is_ok());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
            .fetch_one(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='copy-parent'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(matches!(
            super::duplicate_document(
                State(state.clone()),
                duplicate_auth("editor-a"),
                Path(("space-a".into(), "target-a".into())),
                Json(copy())
            )
            .await,
            Err(crate::auth::routes::AppError::Conflict(_))
        ));
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(after, count);
        assert!(crate::storage::data_dir().join(path).exists());
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod anonymous_restriction_tests {
    use super::*;
    use crate::spaces::routes::catalog_tests::catalog_state;
    async fn fixture(wal: bool) -> (AppState, String) {
        let state = catalog_state(wal).await;
        let slug = format!("restriction-public-{}", uuid::Uuid::new_v4());
        sqlx::query("UPDATE spaces SET slug=$1 WHERE id='space-a'")
            .bind(&slug)
            .execute(&state.db)
            .await
            .unwrap();
        let path = storage::write_document(&slug, "anonymous-page", "Public", "", "ANONYMOUS BODY")
            .await
            .unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,parent_id,title,content_path,created_by,last_updated_by,published_at,public,public_slug) VALUES('anonymous-parent','space-a',NULL,'Private parent','','owner-a','owner-a',NULL,0,NULL),('anonymous-page','space-a','anonymous-parent','Public',$1,'owner-a','owner-a','old',1,'restriction-public-link')").bind(&path).execute(&state.db).await.unwrap();
        (state, slug)
    }
    #[tokio::test]
    async fn anonymous_restriction_inherited_links_keep_private_parent_contract_but_any_restriction_denies(
    ) {
        for node in ["anonymous-parent", "anonymous-page"] {
            let (state, slug) = fixture(false).await;
            let response = read_public_page(&state, "restriction-public-link", true)
                .await
                .unwrap();
            assert!(response.0.content.contains("ANONYMOUS BODY"));
            sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES($1,'space-a','restricted','revision','owner-a')").bind(node).execute(&state.db).await.unwrap();
            // Even a human-owner allowlist never authorizes anonymous reads.
            sqlx::query("INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('owner-allowed',$1,'space-a','owner-a','owner-a')").bind(node).execute(&state.db).await.unwrap();
            assert!(matches!(
                read_public_page(&state, "restriction-public-link", true).await,
                Err(AppError::NotFound(_))
            ));
            tokio::fs::remove_dir_all(storage::space_dir(&slug))
                .await
                .unwrap();
        }
    }
    #[tokio::test]
    async fn anonymous_restriction_final_gate_observes_external_wal_policy_after_file_io() {
        for node in ["anonymous-parent", "anonymous-page"] {
            let (state, slug) = fixture(true).await;
            let db = state.db.clone();
            PUBLIC_READ_AFTER_IO.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {
                sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES($1,'space-a','restricted','late-revision','owner-a')").bind(node).execute(&db).await.unwrap();
            })));
            assert!(matches!(
                read_public_page(&state, "restriction-public-link", true).await,
                Err(AppError::NotFound(_))
            ));
            let mode: String = sqlx::query_scalar(
                "SELECT mode FROM document_access_policies WHERE document_id=$1",
            )
            .bind(node)
            .fetch_one(&state.db)
            .await
            .unwrap();
            assert_eq!(mode, "restricted");
            tokio::fs::remove_dir_all(storage::space_dir(&slug))
                .await
                .unwrap();
        }
    }
}

#[cfg(test)]
mod identifier_get_current_tests {
    use super::*;
    use axum::extract::FromRequestParts;
    #[tokio::test]
    async fn identifier_get_final_selected_draft_and_policy_are_current_after_capability_wal() {
        for statement in [
            "DELETE FROM document_draft_access WHERE draft_id='identifier-selected' AND user_id='editor-a'",
            "UPDATE document_drafts SET title='New selected title' WHERE id='identifier-selected'",
            "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','editor-a','owner-a')",
            "INSERT INTO document_access_policies(document_id,space_id,mode,created_by,revision) VALUES('identifier-parent','space-a','restricted','owner-a','late')",
            "DELETE FROM sessions WHERE id='metadata-editor-a'",
        ] {
            let state=crate::spaces::routes::space_creation_fence_tests::setup(true).await;
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('identifier-parent','space-a','Published parent','','owner-a','owner-a','2000-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO documents(id,space_id,parent_id,title,content_path,created_by,last_updated_by,human_identifier,human_identifier_key) VALUES('identifier-private','space-a','identifier-parent','SECRET canonical','','owner-a','owner-a','Private-1','PRIVATE-1')").execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES('identifier-selected','identifier-private','space-a','owner-a','Selected visible','Selected body','old','2026-01-01'),('identifier-alternate','identifier-private','space-a','owner-a','Alternate visible','Other body','old','2000-01-01')").execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('identifier-selected','editor-a','viewer','owner-a'),('identifier-alternate','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
            let mut actor=tests::test_auth("editor-a");actor.claims.sid="metadata-editor-a".into();let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&actor.claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();
            let req=axum::http::Request::builder().header("Authorization",format!("Bearer {token}")).body(()).unwrap();let(mut parts,_)=req.into_parts();let actor=AuthUser::from_request_parts(&mut parts,&state).await.unwrap();
            let selected=readable_discovery_draft(&mut *state.db.acquire().await.unwrap(),&actor,"space-a","identifier-private").await.unwrap();assert_eq!(selected.id,"identifier-selected");
            let outcome=IDENTIFIER_GET_AFTER_CAPABILITY.scope((state.db.clone(),statement),get_document(State(state.clone()),actor,Path(("space-a".into(),"identifier-private".into())))).await;assert!(outcome.is_err(),"{statement}");
            let proof = if statement.starts_with("DELETE FROM document_draft_access") {"SELECT COUNT(*) FROM document_draft_access WHERE draft_id='identifier-selected' AND user_id='editor-a'"} else if statement.starts_with("UPDATE document_drafts") {"SELECT COUNT(*) FROM document_drafts WHERE id='identifier-selected' AND title='New selected title'"} else if statement.starts_with("INSERT INTO space_user_blocks") {"SELECT COUNT(*) FROM space_user_blocks WHERE space_id='space-a' AND user_id='editor-a'"} else if statement.starts_with("INSERT INTO document_access_policies") {"SELECT COUNT(*) FROM document_access_policies WHERE document_id='identifier-parent'"} else {"SELECT COUNT(*) FROM sessions WHERE id='metadata-editor-a'"};
            let actual:i64=sqlx::query_scalar(proof).fetch_one(&state.db).await.unwrap();assert_eq!(actual,if statement.starts_with("DELETE") {0}else{1},"external hook must execute: {statement}");
            if statement.starts_with("DELETE FROM document_draft_access") || statement.starts_with("UPDATE document_drafts") {let req=axum::http::Request::builder().header("Authorization",format!("Bearer {token}")).body(()).unwrap();let(mut parts,_)=req.into_parts();let actor=AuthUser::from_request_parts(&mut parts,&state).await.unwrap();require_document_discovery_page_view(&mut *state.db.acquire().await.unwrap(),&actor,"space-a","identifier-private").await.unwrap();}
        }
    }
}

#[cfg(test)]
mod lifecycle_listing_read_only_tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    async fn fixture() -> (AppState, String, String, String) {
        let state = crate::spaces::properties::tests::property_test_state().await;
        let suffix = uuid::Uuid::new_v4();
        let own_slug = format!("lifecycle-read-own-{suffix}");
        let hidden_slug = format!("lifecycle-read-hidden-{suffix}");
        for (space, slug) in [("space-a", &own_slug), ("space-b", &hidden_slug)] {
            sqlx::query("UPDATE spaces SET slug=$1,trash_retention_days=1 WHERE id=$2")
                .bind(slug)
                .bind(space)
                .execute(&state.db)
                .await
                .unwrap();
            tokio::fs::create_dir_all(storage::space_dir(slug).join("docs"))
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at,deleted_at,deleted_by) VALUES('expired-own','space-a','Expired own',$1,'owner-a','owner-a','2000-01-01T00:00:00Z','2000-01-02T00:00:00Z','owner-a')")
            .bind(format!("spaces/{own_slug}/docs/expired-own.md")).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at,archived_at) VALUES('archived-own','space-a','Archived own',$1,'owner-a','owner-a','2000-01-01T00:00:00Z','2000-01-02T00:00:00Z')")
            .bind(format!("spaces/{own_slug}/docs/archived-own.md")).execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET title='SECRET foreign victim',content_path=$1,deleted_at='2000-01-02T00:00:00Z',deleted_by='owner-a' WHERE id='target-b'")
            .bind(format!("spaces/{hidden_slug}/docs/target-b.md")).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('target-b','space-b','restricted','hidden-revision','owner-a')")
            .execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('hidden-owner','target-b','space-b','owner-a','owner-a')")
            .execute(&state.db).await.unwrap();
        for (slug, name, content) in [
            (&own_slug, "expired-own.md", "EXPIRED OWN BODY"),
            (&own_slug, "archived-own.md", "ARCHIVED BODY"),
            (&hidden_slug, "target-b.md", "SECRET FOREIGN BODY"),
        ] {
            tokio::fs::write(storage::space_dir(slug).join("docs").join(name), content)
                .await
                .unwrap();
        }
        let raw = format!("lifecycle-read-key-{suffix}");
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('lifecycle-read-key','viewer-a','Read listings',$1,'test','[\"read\"]')")
            .bind(crate::auth::api_keys::hash_api_key(&raw)).execute(&state.db).await.unwrap();
        (state, own_slug, hidden_slug, raw)
    }

    #[tokio::test]
    async fn listing_gets_do_not_purge_expired_pages_or_write_lifecycle_mirrors() {
        let (state, own_slug, hidden_slug, raw) = fixture().await;
        let app = super::router().with_state(state.clone());
        let before = tests::move_database_snapshot(&state).await;
        let own_files = tests::move_file_snapshot(&own_slug);
        let hidden_files = tests::move_file_snapshot(&hidden_slug);
        for (path, expected, excluded) in [
            ("/spaces/space-a/documents", "target-a", "expired-own"),
            (
                "/spaces/space-a/documents/trash",
                "expired-own",
                "archived-own",
            ),
            (
                "/spaces/space-a/documents/archive",
                "archived-own",
                "expired-own",
            ),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .header("X-API-Key", &raw)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "{path}");
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let rows = body.as_array().expect("existing array response");
            assert!(
                rows.iter().any(|row| row["id"] == expected),
                "{path}: {body}"
            );
            assert!(!rows
                .iter()
                .any(|row| row["id"] == excluded || row["id"] == "target-b"));
            assert!(!String::from_utf8_lossy(&bytes).contains("SECRET"));
            // Only the existing API-key extractor's last-used bookkeeping is
            // exempt. Every security field, row and other table remains bound.
            sqlx::query("UPDATE api_keys SET last_used_at=NULL WHERE id='lifecycle-read-key'")
                .execute(&state.db)
                .await
                .unwrap();
            assert_eq!(
                tests::move_database_snapshot(&state).await,
                before,
                "{path}"
            );
            assert_eq!(tests::move_file_snapshot(&own_slug), own_files, "{path}");
            assert_eq!(
                tests::move_file_snapshot(&hidden_slug),
                hidden_files,
                "{path}"
            );
        }
        tokio::fs::remove_dir_all(storage::space_dir(&own_slug))
            .await
            .unwrap();
        tokio::fs::remove_dir_all(storage::space_dir(&hidden_slug))
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod lifecycle_metadata_current_tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    pub(super) async fn setup(wal: bool, trash: bool) -> AppState {
        let state = crate::spaces::routes::space_creation_fence_tests::setup(wal).await;
        let field = if trash { "deleted_at" } else { "archived_at" };
        for (id, published) in [
            ("life-published", Some("2026-01-01T00:00:00Z")),
            ("life-owned", None),
            ("life-shared", None),
            ("life-hidden", None),
            ("life-restricted", Some("2026-01-01T00:00:00Z")),
        ] {
            sqlx::query(&format!("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at,created_at,{field}) VALUES($1,'space-a',$2,'NO BODY READ','owner-a','owner-a',$3,'2001-02-03T04:05:06.123456789+00:00','2000-01-01T00:00:00Z')"))
                .bind(id).bind(if published.is_some(){id}else{"SECRET CANONICAL"}).bind(published).execute(&state.db).await.unwrap();
        }
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('life-parent','space-a','SECRET ANCESTOR','','owner-a','owner-a','2026-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET parent_id='life-parent' WHERE id='life-restricted'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('life-parent','space-a','restricted','restricted','owner-a')").execute(&state.db).await.unwrap();
        for (draft, page, owner, title, updated) in [
            (
                "life-own-draft",
                "life-owned",
                "viewer-a",
                "OWN LABEL",
                "2026-01-01",
            ),
            (
                "life-selected",
                "life-shared",
                "owner-a",
                "SHARED LABEL",
                "2026-01-01",
            ),
            (
                "life-alternate",
                "life-shared",
                "owner-a",
                "ALTERNATE LABEL",
                "2000-01-01",
            ),
            (
                "life-hidden-draft",
                "life-hidden",
                "owner-a",
                "SECRET OTHER DRAFT",
                "2026-01-01",
            ),
        ] {
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,icon,cover_image,content,base_updated_at,updated_at) VALUES($1,$2,'space-a',$3,$4,'authorized-icon','authorized-cover','SECRET BODY','base',$5)")
                .bind(draft).bind(page).bind(owner).bind(title).bind(updated).execute(&state.db).await.unwrap();
        }
        for draft in ["life-selected", "life-alternate"] {
            sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES($1,'viewer-a','viewer','owner-a')").bind(draft).execute(&state.db).await.unwrap();
        }
        for (raw, scopes, service) in [
            ("life-read", "[\"read\"]", false),
            ("life-write", "[\"write\"]", false),
            ("life-service", "[\"admin\"]", true),
        ] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes,is_service_account) VALUES($1,$2,$1,$3,'test',$4,$5)").bind(raw).bind(if service{None}else{Some("viewer-a")}).bind(crate::auth::api_keys::hash_api_key(raw)).bind(scopes).bind(i64::from(service)).execute(&state.db).await.unwrap();
        }
        state
    }
    fn request(trash: bool, key: &str) -> Request<Body> {
        Request::builder()
            .uri(if trash {
                "/spaces/space-a/documents/trash"
            } else {
                "/spaces/space-a/documents/archive"
            })
            .header("X-API-Key", key)
            .body(Body::empty())
            .unwrap()
    }
    async fn actor(state: &AppState) -> AuthUser {
        use axum::extract::FromRequestParts;
        let (mut parts, _) = request(false, "life-read").into_parts();
        AuthUser::from_request_parts(&mut parts, state)
            .await
            .unwrap()
    }
    #[tokio::test]
    async fn actual_http_lifecycle_catalog_filters_private_ancestors_and_preserves_read_only_rows()
    {
        for trash in [false, true] {
            let state = setup(false, trash).await;
            let before = tests::move_database_snapshot(&state).await;
            let app = super::router().with_state(state.clone());
            let res = app
                .clone()
                .oneshot(request(trash, "life-read"))
                .await
                .unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
            let text = String::from_utf8(bytes.to_vec()).unwrap();
            assert!(!text.contains("SECRET"), "{text}");
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let array = json.as_array().unwrap();
            assert_eq!(array.len(), 3);
            for (id, title) in [
                ("life-published", "life-published"),
                ("life-owned", "OWN LABEL"),
                ("life-shared", "SHARED LABEL"),
            ] {
                let doc = array.iter().find(|d| d["id"] == id).unwrap();
                assert_eq!(doc["title"], title);
                let original:(String,Option<String>)=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE id=$1 AND space_id='space-a'").bind(id).fetch_one(&state.db).await.unwrap();
                assert_eq!(doc["created_at"], original.0, "draft labels must not replace original incarnation");
                if trash { assert_eq!(doc["deleted_at"], original.1.unwrap(), "exact original Trash cycle"); }
                else { assert!(doc.get("deleted_at").is_none(), "active/archive response stays unchanged"); }
                if id != "life-published" {
                    assert_eq!(doc["icon"], "authorized-icon");
                    assert_eq!(doc["cover_image"], "authorized-cover");
                }
            }
            for key in ["life-write", "life-service"] {
                assert!(!app
                    .clone()
                    .oneshot(request(trash, key))
                    .await
                    .unwrap()
                    .status()
                    .is_success());
            }
            sqlx::query("UPDATE api_keys SET last_used_at=NULL WHERE id IN ('life-read','life-write','life-service')").execute(&state.db).await.unwrap();
            assert_eq!(tests::move_database_snapshot(&state).await, before);
        }
    }
    #[tokio::test]
    async fn lifecycle_metadata_modes_never_authorize_active_or_wrong_state_pages_or_body_view() {
        let state = setup(false, false).await;
        let auth = actor(&state).await;
        let mut conn = state.db.acquire().await.unwrap();
        let principal = super::super::access::principal_from_auth(&auth).unwrap();
        use super::super::access::{resolve_page_access, PageOperation, Purpose};
        assert!(resolve_page_access(
            &mut conn,
            principal,
            "space-a",
            "life-published",
            PageOperation::InspectArchivedMetadata,
            Purpose::Routine
        )
        .await
        .is_ok());
        for (id, op) in [
            ("target-a", PageOperation::InspectArchivedMetadata),
            ("life-published", PageOperation::InspectTrashedMetadata),
            ("life-published", PageOperation::View),
            ("life-published", PageOperation::Edit),
        ] {
            assert!(
                resolve_page_access(&mut conn, principal, "space-a", id, op, Purpose::Routine)
                    .await
                    .is_err()
            );
        }
    }
    #[tokio::test]
    async fn genuine_wal_lifecycle_source_share_and_final_policy_changes_reject_stale_metadata() {
        for trash in [false, true] {
            for sql in [
                "DELETE FROM document_draft_access WHERE draft_id='life-selected' AND user_id='viewer-a'",
                "UPDATE document_drafts SET title='CHANGED LABEL' WHERE id='life-selected'",
                "UPDATE documents SET title='CHANGED PUBLISHED' WHERE id='life-published'",
                "UPDATE documents SET created_at='changed-incarnation' WHERE id='life-published'",
                "UPDATE documents SET deleted_at='changed-trash-cycle' WHERE id='life-published' AND deleted_at IS NOT NULL",
                "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')",
                "INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('life-published','space-a','restricted','late','owner-a')",
            ] {
                if !trash && sql.contains("changed-trash-cycle") { continue; }
                let state=setup(true,trash).await;let auth=actor(&state).await;let db=state.db.clone();
                let outcome=LIFECYCLE_METADATA_AFTER_PROJECTION.scope(std::cell::RefCell::new(Some(Box::pin(async move {sqlx::query(sql).execute(&db).await.unwrap();}))),list_lifecycle_documents(&state,&auth,"space-a",trash)).await;
                assert!(outcome.is_err(),"{trash} {sql}");
                if sql.starts_with("DELETE FROM document_draft_access") {let a=super::super::access::resolve_page_access(&mut *state.db.acquire().await.unwrap(),super::super::access::principal_from_auth(&auth).unwrap(),"space-a","life-shared",lifecycle_metadata_operations(trash).0,super::super::access::Purpose::Routine).await;assert!(a.is_ok(),"alternate draft preserves page access");}
            }
        }
    }
    #[tokio::test]
    async fn lifecycle_empty_catalog_still_rechecks_current_identity_after_all_sql() {
        let state = setup(true, false).await;
        sqlx::query("UPDATE documents SET archived_at=NULL")
            .execute(&state.db)
            .await
            .unwrap();
        let auth = actor(&state).await;
        let db = state.db.clone();
        let result = LIFECYCLE_METADATA_BEFORE_IDENTITY
            .scope(
                std::cell::RefCell::new(Some(Box::pin(async move {
                    sqlx::query("DELETE FROM api_keys WHERE id='life-read'")
                        .execute(&db)
                        .await
                        .unwrap();
                }))),
                list_lifecycle_documents(&state, &auth, "space-a", false),
            )
            .await;
        assert!(matches!(result, Err(AppError::Unauthorized(_))));
    }
    #[tokio::test]
    async fn genuine_wal_after_inventory_current_page_and_exact_selected_draft_proofs_are_last() {
        for trash in [false, true] {
            for sql in [
                "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')",
                "DELETE FROM document_draft_access WHERE draft_id='life-selected' AND user_id='viewer-a'",
                "INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('life-published','space-a','restricted','late-inventory','owner-a')",
            ] {
                let state=setup(true,trash).await;let auth=actor(&state).await;let db=state.db.clone();
                let outcome=LIFECYCLE_METADATA_AFTER_SOURCE.scope(std::cell::RefCell::new(Some(Box::pin(async move {sqlx::query(sql).execute(&db).await.unwrap();}))),list_lifecycle_documents(&state,&auth,"space-a",trash)).await;
                assert!(outcome.is_err(),"after inventory: {trash} {sql}");
                if sql.starts_with("DELETE") {
                    assert!(super::super::access::resolve_page_access(&mut *state.db.acquire().await.unwrap(),super::super::access::principal_from_auth(&auth).unwrap(),"space-a","life-shared",lifecycle_metadata_operations(trash).0,super::super::access::Purpose::Routine).await.is_ok(),"alternate draft must keep page View while selected proof denies");
                }
            }
        }
    }

    #[tokio::test]
    async fn custom_view_only_role_can_read_lifecycle_metadata_without_mutation_authority() {
        for trash in [false, true] {
            let state = setup(false, trash).await;
            sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('life-view-only','space-a','Reader','[\"view\"]','owner-a')").execute(&state.db).await.unwrap();
            sqlx::query("UPDATE space_members SET role='life-view-only' WHERE space_id='space-a' AND user_id='viewer-a'").execute(&state.db).await.unwrap();
            let app = super::router().with_state(state.clone());
            assert_eq!(
                app.oneshot(request(trash, "life-read"))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::OK
            );
            let auth = actor(&state).await;
            let mut conn = state.db.acquire().await.unwrap();
            let access = super::super::access::resolve_page_access(
                &mut conn,
                super::super::access::principal_from_auth(&auth).unwrap(),
                "space-a",
                "life-published",
                lifecycle_metadata_operations(trash).0,
                super::super::access::Purpose::Routine,
            )
            .await
            .unwrap();
            assert!(!access
                .space
                .allows(super::super::access::Permission::EditContent));
        }
    }
    #[tokio::test]
    async fn genuine_jwt_expiry_after_all_inventory_and_policy_queries_denies_response() {
        use axum::extract::FromRequestParts;
        let state = setup(false, false).await;
        let mut claims = tests::test_auth("viewer-a").claims;
        claims.sid = "metadata-viewer-a".into();
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let req = Request::builder()
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let auth = AuthUser::from_request_parts(&mut parts, &state)
            .await
            .unwrap();
        sqlx::query(
            "UPDATE sessions SET expires_at='2999-01-01T00:00:00Z' WHERE id='metadata-viewer-a'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let before = tests::move_database_snapshot(&state).await;
        let result = LIFECYCLE_METADATA_BEFORE_IDENTITY
            .scope(
                std::cell::RefCell::new(Some(Box::pin(async {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }))),
                list_lifecycle_documents(&state, &auth, "space-a", false),
            )
            .await;
        assert!(matches!(result, Err(AppError::Unauthorized(_))));
        assert_eq!(tests::move_database_snapshot(&state).await, before);
    }
}

#[cfg(test)]
mod archived_private_get_current_tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        extract::FromRequestParts,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    fn request(page: &str, body: bool) -> Request<Body> {
        Request::builder()
            .uri(format!(
                "/spaces/space-a/documents/{page}{}",
                if body { "/content" } else { "" }
            ))
            .header("X-API-Key", "life-read")
            .body(Body::empty())
            .unwrap()
    }
    #[tokio::test]
    async fn actual_http_archived_private_get_uses_authorized_metadata_and_never_canonical_labels_or_body(
    ) {
        let state = super::lifecycle_metadata_current_tests::setup(false, false).await;
        let app = super::router().with_state(state.clone());
        let before = tests::move_database_snapshot(&state).await;
        for (page, title, draft) in [
            ("life-owned", "OWN LABEL", "life-own-draft"),
            ("life-shared", "SHARED LABEL", "life-selected"),
        ] {
            let response = app.clone().oneshot(request(page, false)).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(json["title"], title);
            assert_eq!(json["icon"], "authorized-icon");
            assert_eq!(json["cover_image"], "authorized-cover");
            assert_eq!(json["draft_id"], draft);
            assert_eq!(json["identifier_edit_allowed"], false);
            assert_eq!(json["maintenance_edit_allowed"], false);
            assert!(!String::from_utf8_lossy(&bytes).contains("SECRET"));
            assert_eq!(
                app.clone()
                    .oneshot(request(page, true))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::NOT_FOUND,
                "metadata access must not authorize archived body"
            );
        }
        assert_eq!(
            app.oneshot(request("life-hidden", false))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        sqlx::query("UPDATE api_keys SET last_used_at=NULL WHERE id='life-read'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(tests::move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn actual_http_wal_archived_selected_share_and_label_reproof_follow_capability_sql() {
        for sql in [
            "DELETE FROM document_draft_access WHERE draft_id='life-selected' AND user_id='viewer-a'",
            "UPDATE document_drafts SET title='CHANGED SHARED TITLE' WHERE id='life-selected'",
            "UPDATE document_drafts SET icon='changed-icon',cover_image='changed-cover' WHERE id='life-selected'",
            "UPDATE documents SET published_at='2026-01-01T00:00:00Z' WHERE id='life-shared'",
            "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')",
        ] {
            let state=super::lifecycle_metadata_current_tests::setup(true,false).await;let app=super::router().with_state(state.clone());
            let response=IDENTIFIER_GET_AFTER_CAPABILITY.scope((state.db.clone(),sql),app.oneshot(request("life-shared",false))).await.unwrap();
            assert!(!response.status().is_success(),"{sql}");let bytes=to_bytes(response.into_body(),usize::MAX).await.unwrap();assert!(!String::from_utf8_lossy(&bytes).contains("SECRET CANONICAL"));
            if sql.starts_with("DELETE FROM document_draft_access") {
                let remaining:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_draft_access WHERE draft_id='life-selected' AND user_id='viewer-a'").fetch_one(&state.db).await.unwrap();assert_eq!(remaining,0,"external WAL hook ran");
                let(mut parts,_)=request("life-shared",false).into_parts();let auth=AuthUser::from_request_parts(&mut parts,&state).await.unwrap();
                assert!(super::super::access::resolve_page_access(&mut *state.db.acquire().await.unwrap(),super::super::access::principal_from_auth(&auth).unwrap(),"space-a","life-shared",super::super::access::PageOperation::InspectArchivedMetadata,super::super::access::Purpose::Routine).await.is_ok(),"alternate readable draft keeps page metadata access");
            }
        }
    }
}

#[cfg(test)]
tokio::task_local! {
 static REST_DELETE_AFTER_GUARD:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
 static REST_DELETE_BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
}
#[cfg(test)]
async fn rest_delete_after_guard() {
    if let Ok(Some(h)) = REST_DELETE_AFTER_GUARD.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}
#[cfg(test)]
async fn rest_delete_before_identity() {
    if let Ok(Some(h)) = REST_DELETE_BEFORE_IDENTITY.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}
#[cfg(test)]
mod rest_permanent_delete_current_tests {
    use super::*;
    use serde_json::{json,Value};
    use axum::{
        body::Body,
        extract::FromRequestParts,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    pub(super) async fn fixture(wal: bool) -> (AppState, String, String) {
        let state = crate::spaces::routes::space_creation_fence_tests::setup(wal).await;
        // Real FKs on every fixture connection, including the WAL writer.
        let mut first = state.db.acquire().await.unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&mut *first)
            .await
            .unwrap();
        if wal {
            let mut second = state.db.acquire().await.unwrap();
            sqlx::query("PRAGMA foreign_keys=ON")
                .execute(&mut *second)
                .await
                .unwrap();
        }
        drop(first);
        sqlx::query("UPDATE users SET role='member' WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        let slug = format!("rest-delete-{}", uuid::Uuid::new_v4());
        let path = format!("spaces/{slug}/docs/original.md");
        sqlx::query("UPDATE spaces SET slug=$1 WHERE id='space-a'")
            .bind(&slug)
            .execute(&state.db)
            .await
            .unwrap();
        tokio::fs::create_dir_all(storage::space_dir(&slug).join("docs"))
            .await
            .unwrap();
        tokio::fs::write(storage::data_dir().join(&path), b"RETAINED EXACT BODY")
            .await
            .unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at,deleted_at,locked,locked_by) VALUES('rest-delete','space-a','Original',$1,'owner-a','owner-a','published','2000',1,'owner-a'),('rest-active','space-a','Active','','owner-a','owner-a','published',NULL,0,NULL),('rest-foreign','space-b','Foreign','','owner-a','owner-a','published','2000',0,NULL)").bind(&path).execute(&state.db).await.unwrap();
        let blob = format!("spaces/{slug}/docs/blob.bin");
        tokio::fs::write(storage::data_dir().join(&blob), b"RETAINED BLOB")
            .await
            .unwrap();
        sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES('rest-blob','rest-delete','space-a','blob','blob','application/octet-stream',13,$1,'owner-a')").bind(blob).execute(&state.db).await.unwrap();
        for (key, scope, user) in [
            ("rest-write", "write", "editor-a"),
            ("rest-read", "read", "editor-a"),
            ("rest-viewer-write", "write", "viewer-a"),
        ] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,$2,$1,$3,'test',$4)").bind(key).bind(user).bind(crate::auth::api_keys::hash_api_key(key)).bind(format!("[\"{scope}\"]")).execute(&state.db).await.unwrap();
        }
        (state, slug, path)
    }
    fn request(page: &str, key: &str) -> Request<Body> {
        Request::builder()
            .method("DELETE")
            .uri(format!("/spaces/space-a/documents/{page}/trash"))
            .header("X-API-Key", key)
            .body(Body::empty())
            .unwrap()
    }
    async fn auth(state: &AppState) -> AuthUser {
        let (mut p, _) = request("rest-delete", "rest-write").into_parts();
        AuthUser::from_request_parts(&mut p, state).await.unwrap()
    }
    async fn cleanup(slug: &str) {
        let _ = tokio::fs::remove_dir_all(storage::space_dir(slug)).await;
    }
    fn selected_request(page:&str,key:&str,operation:&str,target:&Value)->Request<Body>{Request::builder().method("DELETE").uri(format!("/spaces/space-a/documents/{page}/trash")).header("X-API-Key",key).header("x-starlane-delete-operation",operation).header("content-type","application/json").body(Body::from(json!({"original_target":target}).to_string())).unwrap()}
    async fn selected_target(state:&AppState,page:&str)->Value{let (created,deleted):(String,Option<String>)=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE id=$1").bind(page).fetch_one(&state.db).await.unwrap();json!({"created_at":created,"deleted_at":deleted})}
    #[tokio::test]
    async fn actual_http_rest_permanent_delete_is_scoped_trashed_and_retains_unassociated_bytes() {
        let (state, slug, path) = fixture(false).await;
        let original=selected_target(&state,"rest-delete").await;let active=selected_target(&state,"rest-active").await;let foreign=selected_target(&state,"rest-foreign").await;let operation="aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
        let app = router().with_state(state.clone());
        assert_eq!(
            app.clone()
                .oneshot(selected_request("rest-delete","rest-write",operation,&original))
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM documents WHERE id='rest-delete'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments WHERE id='rest-blob'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM document_delete_replay_receipts")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0,
            "REST never manufactures replay receipts"
        );
        assert_eq!(
            tokio::fs::read(storage::data_dir().join(&path))
                .await
                .unwrap(),
            b"RETAINED EXACT BODY"
        );
        assert_eq!(
            tokio::fs::read(storage::space_dir(&slug).join("docs/blob.bin"))
                .await
                .unwrap(),
            b"RETAINED BLOB"
        );
        let retry=app.clone().oneshot(selected_request("rest-delete","rest-write",operation,&original)).await.unwrap();assert_eq!(retry.status(),StatusCode::NO_CONTENT);assert_eq!(retry.headers()["x-starlane-delete-operation"],operation);
        for (page,target) in [("missing",&original),("rest-active",&active),("rest-foreign",&foreign)] {
            assert_eq!(
                app.clone()
                    .oneshot(selected_request(page,"rest-write",&uuid::Uuid::new_v4().to_string(),target))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::CONFLICT
            );
        }
        let body = Request::builder()
            .uri("/spaces/space-a/documents/rest-delete/content")
            .header("X-API-Key", "rest-read")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            app.oneshot(body).await.unwrap().status(),
            StatusCode::NOT_FOUND
        );
        cleanup(&slug).await;
    }
    #[tokio::test]
    async fn actual_http_rest_permanent_delete_read_viewer_and_child_denials_preserve_rows_files() {
        for kind in ["read", "viewer", "child", "foreign"] {
            let (state, slug, path) = fixture(false).await;
            if kind == "child" {
                sqlx::query("UPDATE documents SET parent_id='rest-delete' WHERE id='rest-active'")
                    .execute(&state.db)
                    .await
                    .unwrap();
            }
            if kind == "foreign" {
                sqlx::query("UPDATE attachments SET space_id='space-b' WHERE id='rest-blob'")
                    .execute(&state.db)
                    .await
                    .unwrap();
            }
            let before = tests::move_database_snapshot(&state).await;
            let key = match kind {
                "read" => "rest-read",
                "viewer" => "rest-viewer-write",
                _ => "rest-write",
            };
            let response = router()
                .with_state(state.clone())
                .oneshot(request("rest-delete", key))
                .await
                .unwrap();
            assert!(!response.status().is_success(), "{kind}");
            if kind == "child" {
                assert_eq!(response.status(), StatusCode::CONFLICT);
            }
            sqlx::query("UPDATE api_keys SET last_used_at=NULL WHERE id=$1")
                .bind(key)
                .execute(&state.db)
                .await
                .unwrap();
            assert_eq!(
                tests::move_database_snapshot(&state).await,
                before,
                "{kind}"
            );
            assert_eq!(
                tokio::fs::read(storage::data_dir().join(path))
                    .await
                    .unwrap(),
                b"RETAINED EXACT BODY"
            );
            cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn rest_permanent_delete_exact_delete_custom_role_and_private_editor_are_required() {
        let (state, slug, _) = fixture(false).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('rest-delete-only','space-a','Delete only','[\"delete_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='rest-delete-only' WHERE space_id='space-a' AND user_id='editor-a'").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='rest-delete'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('rest-private','rest-delete','space-a','owner-a','Private','base')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('rest-private','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        let original=selected_target(&state,"rest-delete").await;let operation="bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
        let a = auth(&state).await;
        let before = tests::move_database_snapshot(&state).await;
        assert!(permanently_delete_document_operation(state.clone(),a,"space-a".into(),"rest-delete".into(),operation.into(),serde_json::from_value(original.clone()).unwrap())
        .await
        .is_err());
        assert_eq!(tests::move_database_snapshot(&state).await, before);
        sqlx::query("UPDATE document_draft_access SET role='editor' WHERE draft_id='rest-private'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            router()
                .with_state(state.clone())
                .oneshot(selected_request("rest-delete","rest-write",operation,&original))
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT
        );
        cleanup(&slug).await;
    }
    #[tokio::test]
    async fn rest_permanent_delete_late_delete_trigger_drift_rolls_back_every_cascade() {
        for change in [
            "UPDATE users SET is_active=0 WHERE id='editor-a'",
            "UPDATE spaces SET visibility='public' WHERE id='space-a'",
            "UPDATE documents SET title='Unexpected' WHERE id='rest-active'",
        ] {
            let (state, slug, path) = fixture(false).await;
            sqlx::query(&format!("CREATE TRIGGER rest_delete_drift AFTER DELETE ON documents WHEN OLD.id='rest-delete' BEGIN {change}; END")).execute(&state.db).await.unwrap();
            let a = auth(&state).await;
            let before = tests::move_database_snapshot(&state).await;
            assert!(permanently_delete_document(
                State(state.clone()),
                a,
                Path(("space-a".into(), "rest-delete".into()))
            )
            .await
            .is_err());
            assert_eq!(
                tests::move_database_snapshot(&state).await,
                before,
                "{change}"
            );
            assert_eq!(
                tokio::fs::read(storage::data_dir().join(path))
                    .await
                    .unwrap(),
                b"RETAINED EXACT BODY"
            );
            cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn rest_permanent_delete_wal_guard_wait_rechecks_original_actor_and_source() {
        for change in [
            "UPDATE api_keys SET scopes='[\"read\"]' WHERE id='rest-write'",
            "UPDATE documents SET space_id='space-b' WHERE id='rest-delete'",
        ] {
            let (state, slug, path) = fixture(true).await;
            let a = auth(&state).await;
            let mut hold = state.db.begin().await.unwrap();
            super::super::mutation_guards::lock_audience_for_mutation(&mut hold)
                .await
                .unwrap();
            let cloned = state.clone();
            let task = tokio::spawn(async move {
                permanently_delete_document(
                    State(cloned),
                    a,
                    Path(("space-a".into(), "rest-delete".into())),
                )
                .await
            });
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!task.is_finished());
            sqlx::query(change).execute(&mut *hold).await.unwrap();
            hold.commit().await.unwrap();
            let before = tests::move_database_snapshot(&state).await;
            assert!(task.await.unwrap().is_err());
            assert_eq!(tests::move_database_snapshot(&state).await, before);
            assert_eq!(
                tokio::fs::read(storage::data_dir().join(path))
                    .await
                    .unwrap(),
                b"RETAINED EXACT BODY"
            );
            cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn rest_permanent_delete_last_original_jwt_expiry_rolls_back_after_source_reads() {
        let (state, slug, _) = fixture(false).await;
        let mut claims = crate::spaces::properties::tests::test_auth("editor-a").claims;
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let req = Request::builder()
            .method("DELETE")
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let a = AuthUser::from_request_parts(&mut parts, &state)
            .await
            .unwrap();
        let before = tests::move_database_snapshot(&state).await;
        let outcome = REST_DELETE_BEFORE_IDENTITY
            .scope(
                std::cell::RefCell::new(Some(Box::pin(async {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await
                }))),
                permanently_delete_document(
                    State(state.clone()),
                    a,
                    Path(("space-a".into(), "rest-delete".into())),
                ),
            )
            .await;
        assert!(matches!(outcome, Err(AppError::Unauthorized(_))));
        assert_eq!(tests::move_database_snapshot(&state).await, before);
        cleanup(&slug).await;
    }
}

#[cfg(test)]
mod soft_trash_current_tests {
    use super::*;
    use axum::{
        body::Body,
        extract::FromRequestParts,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    async fn fixture(wal: bool) -> (AppState, String, String) {
        let (state, slug, path) = rest_permanent_delete_current_tests::fixture(wal).await;
        sqlx::query("UPDATE documents SET deleted_at=NULL,deleted_by=NULL WHERE id='rest-delete'")
            .execute(&state.db)
            .await
            .unwrap();
        (state, slug, path)
    }
    fn request(page: &str, key: &str) -> Request<Body> {
        Request::builder()
            .method("DELETE")
            .uri(format!("/spaces/space-a/documents/{page}"))
            .header("X-API-Key", key)
            .body(Body::empty())
            .unwrap()
    }
    async fn auth(state: &AppState) -> AuthUser {
        let (mut p, _) = request("rest-delete", "rest-write").into_parts();
        AuthUser::from_request_parts(&mut p, state).await.unwrap()
    }
    async fn cleanup(slug: &str) {
        let _ = tokio::fs::remove_dir_all(storage::space_dir(slug)).await;
    }
    async fn retained(slug: &str, path: &str) {
        assert_eq!(
            tokio::fs::read(storage::data_dir().join(path))
                .await
                .unwrap(),
            b"RETAINED EXACT BODY"
        );
        assert_eq!(
            tokio::fs::read(storage::space_dir(slug).join("docs/blob.bin"))
                .await
                .unwrap(),
            b"RETAINED BLOB"
        );
    }
    #[tokio::test]
    async fn soft_trash_http_active_and_archived_preserve_children_associations_and_all_bytes() {
        for archived in [false, true] {
            let (state, slug, path) = fixture(false).await;
            sqlx::query("UPDATE documents SET parent_id='rest-delete' WHERE id='rest-active'")
                .execute(&state.db)
                .await
                .unwrap();
            if archived {
                sqlx::query("UPDATE documents SET archived_at='2001' WHERE id='rest-delete'")
                    .execute(&state.db)
                    .await
                    .unwrap();
            }
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('trash-draft','rest-delete','space-a','owner-a','Working','OWN BODY','base')").execute(&state.db).await.unwrap();
            let before = super::super::document_deletion::rows(
                &mut *state.db.acquire().await.unwrap(),
                "SELECT * FROM documents WHERE id='rest-delete'",
                &[],
            )
            .await
            .unwrap();
            let mirror = storage::space_dir(&slug).join("trash");
            tokio::fs::create_dir_all(&mirror).await.unwrap();
            tokio::fs::write(mirror.join("existing.md"), b"EXISTING MIRROR")
                .await
                .unwrap();
            let response = router()
                .with_state(state.clone())
                .oneshot(request("rest-delete", "rest-write"))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::NO_CONTENT);
            let after = super::super::document_deletion::rows(
                &mut *state.db.acquire().await.unwrap(),
                "SELECT * FROM documents WHERE id='rest-delete'",
                &[],
            )
            .await
            .unwrap();
            let mut expected = before[0].clone();
            for field in ["updated_at", "deleted_at", "deleted_by", "archived_at"] {
                expected[field] = after[0][field].clone();
            }
            assert_eq!(after, vec![expected]);
            assert_eq!(after[0]["deleted_by"], "editor-a");
            assert!(after[0]["deleted_at"].is_string());
            assert!(after[0]["archived_at"].is_null());
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    "SELECT parent_id FROM documents WHERE id='rest-active'"
                )
                .fetch_one(&state.db)
                .await
                .unwrap(),
                "rest-delete"
            );
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    "SELECT content FROM document_drafts WHERE id='trash-draft'"
                )
                .fetch_one(&state.db)
                .await
                .unwrap(),
                "OWN BODY"
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM attachments WHERE id='rest-blob'"
                )
                .fetch_one(&state.db)
                .await
                .unwrap(),
                1
            );
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM audit_log WHERE action='document.delete' AND user_id='editor-a' AND target_id='rest-delete'").fetch_one(&state.db).await.unwrap(),1);
            assert_eq!(
                tokio::fs::read(mirror.join("existing.md")).await.unwrap(),
                b"EXISTING MIRROR"
            );
            retained(&slug, &path).await;
            for page in ["rest-delete", "rest-foreign", "missing"] {
                assert_eq!(
                    router()
                        .with_state(state.clone())
                        .oneshot(request(page, "rest-write"))
                        .await
                        .unwrap()
                        .status(),
                    StatusCode::NOT_FOUND
                );
            }
            cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn soft_trash_http_actual_scope_custom_delete_and_private_editor() {
        for key in ["rest-read", "rest-viewer-write"] {
            let (state, slug, path) = fixture(false).await;
            let before = tests::move_database_snapshot(&state).await;
            let response = router()
                .with_state(state.clone())
                .oneshot(request("rest-delete", key))
                .await
                .unwrap();
            assert!(!response.status().is_success());
            sqlx::query("UPDATE api_keys SET last_used_at=NULL WHERE id=$1")
                .bind(key)
                .execute(&state.db)
                .await
                .unwrap();
            assert_eq!(tests::move_database_snapshot(&state).await, before);
            retained(&slug, &path).await;
            cleanup(&slug).await;
        }
        let (state, slug, path) = fixture(false).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('trash-delete-only','space-a','Delete only','[\"delete_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='trash-delete-only' WHERE space_id='space-a' AND user_id='editor-a'").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='rest-delete'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('trash-private','rest-delete','space-a','owner-a','Private','base')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('trash-private','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        let a = auth(&state).await;
        let before = tests::move_database_snapshot(&state).await;
        assert!(delete_document(
            State(state.clone()),
            a,
            Path(("space-a".into(), "rest-delete".into()))
        )
        .await
        .is_err());
        assert_eq!(tests::move_database_snapshot(&state).await, before);
        sqlx::query(
            "UPDATE document_draft_access SET role='editor' WHERE draft_id='trash-private'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert_eq!(
            router()
                .with_state(state.clone())
                .oneshot(request("rest-delete", "rest-write"))
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT
        );
        retained(&slug, &path).await;
        cleanup(&slug).await;
    }
    #[tokio::test]
    async fn soft_trash_late_row_and_audit_trigger_drift_roll_back_everything() {
        for trigger in [
   "CREATE TRIGGER trash_drift AFTER UPDATE OF deleted_at ON documents WHEN NEW.id='rest-delete' BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END",
   "CREATE TRIGGER trash_drift AFTER UPDATE OF deleted_at ON documents WHEN NEW.id='rest-delete' BEGIN UPDATE documents SET title='Injected' WHERE id='rest-active'; END",
   "CREATE TRIGGER trash_drift AFTER INSERT ON audit_log WHEN NEW.action='document.delete' BEGIN UPDATE spaces SET visibility='public' WHERE id='space-a'; END",
   "CREATE TRIGGER trash_drift AFTER INSERT ON audit_log WHEN NEW.action='document.delete' BEGIN UPDATE audit_log SET after_value='injected' WHERE id=NEW.id; END",
   "CREATE TRIGGER trash_drift BEFORE INSERT ON audit_log WHEN NEW.action='document.delete' BEGIN SELECT RAISE(ABORT,'audit denied'); END"]{
   let(state,slug,path)=fixture(false).await;sqlx::query(trigger).execute(&state.db).await.unwrap();let a=auth(&state).await;let before=tests::move_database_snapshot(&state).await;assert!(delete_document(State(state.clone()),a,Path(("space-a".into(),"rest-delete".into()))).await.is_err(),"{trigger}");assert_eq!(tests::move_database_snapshot(&state).await,before,"{trigger}");retained(&slug,&path).await;cleanup(&slug).await;
  }
    }
    #[tokio::test]
    async fn soft_trash_wal_fence_wait_rechecks_original_key_and_page() {
        for change in [
            "UPDATE api_keys SET scopes='[\"read\"]' WHERE id='rest-write'",
            "UPDATE documents SET space_id='space-b' WHERE id='rest-delete'",
        ] {
            let (state, slug, path) = fixture(true).await;
            let a = auth(&state).await;
            let mut hold = state.db.begin().await.unwrap();
            super::super::mutation_guards::lock_audience_for_mutation(&mut hold)
                .await
                .unwrap();
            let cloned = state.clone();
            let task = tokio::spawn(async move {
                delete_document(
                    State(cloned),
                    a,
                    Path(("space-a".into(), "rest-delete".into())),
                )
                .await
            });
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!task.is_finished());
            sqlx::query(change).execute(&mut *hold).await.unwrap();
            hold.commit().await.unwrap();
            let before = tests::move_database_snapshot(&state).await;
            assert!(task.await.unwrap().is_err());
            assert_eq!(tests::move_database_snapshot(&state).await, before);
            retained(&slug, &path).await;
            cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn soft_trash_last_original_jwt_expiry_rolls_back_audit_and_transition() {
        let (state, slug, path) = fixture(false).await;
        let mut claims = crate::spaces::properties::tests::test_auth("editor-a").claims;
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let req = Request::builder()
            .method("DELETE")
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let a = AuthUser::from_request_parts(&mut parts, &state)
            .await
            .unwrap();
        let before = tests::move_database_snapshot(&state).await;
        let outcome = super::super::lifecycle::TRASH_BEFORE_IDENTITY
            .scope(
                std::cell::RefCell::new(Some(Box::pin(async {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await
                }))),
                delete_document(
                    State(state.clone()),
                    a,
                    Path(("space-a".into(), "rest-delete".into())),
                ),
            )
            .await;
        assert!(matches!(outcome, Err(AppError::Unauthorized(_))));
        assert_eq!(tests::move_database_snapshot(&state).await, before);
        retained(&slug, &path).await;
        cleanup(&slug).await;
    }
    #[tokio::test]
    async fn soft_trash_confirmed_rollback_returns_no_committed_outcome_and_keeps_bytes() {
        let (state, slug, path) = fixture(false).await;
        let a = auth(&state).await;
        let before = tests::move_database_snapshot(&state).await;
        let result = super::super::lifecycle::TRASH_ROLLBACK
            .scope(
                true,
                delete_document(
                    State(state.clone()),
                    a,
                    Path(("space-a".into(), "rest-delete".into())),
                ),
            )
            .await;
        assert!(result.is_err());
        assert_eq!(tests::move_database_snapshot(&state).await, before);
        retained(&slug, &path).await;
        cleanup(&slug).await;
    }
}

#[cfg(test)]
mod public_link_atomic_audit_tests {
    use super::*;
    use axum::{
        body::Body,
        extract::FromRequestParts,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    async fn state() -> AppState {
        tests::public_link_test_state().await
    }
    fn request(page: &str, key: &str, enable: bool) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(format!(
                "/spaces/space-a/documents/{page}/{}",
                if enable { "publish" } else { "unpublish" }
            ))
            .header("X-API-Key", key)
            .body(Body::empty())
            .unwrap()
    }
    async fn key(state: &AppState, id: &str, scope: &str) {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'editor-a',$1,$2,'test',$3)").bind(id).bind(crate::auth::api_keys::hash_api_key(id)).bind(format!("[\"{scope}\"]")).execute(&state.db).await.unwrap();
    }
    #[tokio::test]
    async fn public_link_atomic_http_audit_preserves_fixed_actions_and_body_lock_exception() {
        let state = state().await;
        key(&state, "audit-link-write", "write").await;
        sqlx::query("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='page-a'")
            .execute(&state.db)
            .await
            .unwrap();
        for enable in [true, false] {
            let response = router()
                .with_state(state.clone())
                .oneshot(request("page-a", "audit-link-write", enable))
                .await
                .unwrap();
            assert!(response.status().is_success(), "{:?}", response.status());
            let action = if enable {
                "document.public_link_enable"
            } else {
                "document.public_link_disable"
            };
            let row:(String,String,String,Option<String>,Option<String>)=sqlx::query_as("SELECT user_id,target_type,target_id,before_value,after_value FROM audit_log WHERE action=$1").bind(action).fetch_one(&state.db).await.unwrap();
            assert_eq!(
                row,
                (
                    "editor-a".into(),
                    "document".into(),
                    "page-a".into(),
                    None,
                    None
                )
            );
        }
        let baseline_used: Option<String> =
            sqlx::query_scalar("SELECT last_used_at FROM api_keys WHERE id='audit-link-write'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let before = tests::move_database_snapshot(&state).await;
        for page in ["page-b", "missing"] {
            assert_eq!(
                router()
                    .with_state(state.clone())
                    .oneshot(request(page, "audit-link-write", true))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::NOT_FOUND
            );
        }
        // Restore only known extractor bookkeeping, retaining every key/security/domain column.
        sqlx::query("UPDATE api_keys SET last_used_at=$1 WHERE id='audit-link-write'")
            .bind(baseline_used)
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(tests::move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn public_link_audit_failure_or_late_source_tamper_rolls_back_full_database() {
        for change in [
   "CREATE TRIGGER link_audit BEFORE INSERT ON audit_log WHEN NEW.action LIKE 'document.public_link_%' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END",
   "CREATE TRIGGER link_audit AFTER INSERT ON audit_log WHEN NEW.action LIKE 'document.public_link_%' BEGIN UPDATE audit_log SET before_value='injected' WHERE id=NEW.id; END",
   "CREATE TRIGGER link_audit AFTER INSERT ON audit_log WHEN NEW.action LIKE 'document.public_link_%' BEGIN DELETE FROM sessions WHERE id='public-link-editor-a'; END",
   "CREATE TRIGGER link_audit AFTER INSERT ON audit_log WHEN NEW.action LIKE 'document.public_link_%' BEGIN UPDATE documents SET title='Unexpected' WHERE id='page-a'; END",
   "CREATE TRIGGER link_audit AFTER INSERT ON audit_log WHEN NEW.action LIKE 'document.public_link_%' BEGIN UPDATE spaces SET visibility='public' WHERE id='space-a'; END",
   "CREATE TRIGGER link_audit AFTER INSERT ON audit_log WHEN NEW.action LIKE 'document.public_link_%' BEGIN UPDATE document_draft_access SET role='viewer' WHERE draft_id='link-parent-draft'; END"]{
   for enable in [true,false]{
    let state=state().await;
    sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('link-private-parent','space-a','Private parent','','owner-a','owner-a')").execute(&state.db).await.unwrap();
    sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('link-parent-draft','link-private-parent','space-a','owner-a','Private','base')").execute(&state.db).await.unwrap();
    sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('link-parent-draft','editor-a','editor','owner-a')").execute(&state.db).await.unwrap();
    sqlx::query("UPDATE documents SET parent_id='link-private-parent' WHERE id='page-a'").execute(&state.db).await.unwrap();
    sqlx::query(change).execute(&state.db).await.unwrap();let before=tests::move_database_snapshot(&state).await;
    assert!(change_public_link(&state,&tests::public_link_auth("editor-a"),"space-a","page-a",enable).await.is_err(),"{enable} {change}");assert_eq!(tests::move_database_snapshot(&state).await,before,"{enable} {change}");
   }
  }
    }
    #[tokio::test]
    async fn public_link_last_original_jwt_expiry_rolls_back_canonical_and_audit() {
        let state = state().await;
        let mut claims = tests::public_link_auth("editor-a").claims;
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let req = Request::builder()
            .method("POST")
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let auth = AuthUser::from_request_parts(&mut parts, &state)
            .await
            .unwrap();
        let before = tests::move_database_snapshot(&state).await;
        let result = PUBLIC_LINK_BEFORE_IDENTITY
            .scope(
                std::cell::RefCell::new(Some(Box::pin(async {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await
                }))),
                change_public_link(&state, &auth, "space-a", "page-a", true),
            )
            .await;
        assert!(matches!(result, Err(AppError::Unauthorized(_))));
        assert_eq!(tests::move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn public_link_confirmed_rollback_returns_no_slug_or_durable_audit() {
        let state = state().await;
        let before = tests::move_database_snapshot(&state).await;
        let result = PUBLIC_LINK_ROLLBACK
            .scope(
                true,
                change_public_link(
                    &state,
                    &tests::public_link_auth("editor-a"),
                    "space-a",
                    "page-a",
                    true,
                ),
            )
            .await;
        assert!(result.is_err());
        assert_eq!(tests::move_database_snapshot(&state).await, before);
    }
}

#[cfg(test)]
mod archive_transition_current_tests {
    use super::*;
    use axum::{
        body::Body,
        extract::FromRequestParts,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    async fn setup(wal: bool) -> (AppState, String, String) {
        let (state, slug, path) = rest_permanent_delete_current_tests::fixture(wal).await;
        sqlx::query("UPDATE documents SET deleted_at=NULL,deleted_by=NULL,public=1,public_slug='archive-link',human_identifier='ARCHIVE.1',human_identifier_key='ARCHIVE.1',maintenance_user_id='editor-a' WHERE id='rest-delete'").execute(&state.db).await.unwrap();
        (state, slug, path)
    }
    fn request(page: &str, key: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(format!("/spaces/space-a/documents/{page}/archive"))
            .header("X-API-Key", key)
            .body(Body::empty())
            .unwrap()
    }
    async fn auth(state: &AppState) -> AuthUser {
        let (mut parts, _) = request("rest-delete", "rest-write").into_parts();
        AuthUser::from_request_parts(&mut parts, state)
            .await
            .unwrap()
    }
    async fn cleanup(slug: &str) {
        let _ = tokio::fs::remove_dir_all(storage::space_dir(slug)).await;
    }
    async fn retained(slug: &str, path: &str) {
        assert_eq!(
            tokio::fs::read(storage::data_dir().join(path))
                .await
                .unwrap(),
            b"RETAINED EXACT BODY"
        );
        assert_eq!(
            tokio::fs::read(storage::space_dir(slug).join("docs/blob.bin"))
                .await
                .unwrap(),
            b"RETAINED BLOB"
        );
    }
    #[tokio::test]
    async fn archive_http_refreshes_existing_stamp_and_preserves_children_metadata_flags_and_bytes()
    {
        let (state, slug, path) = setup(false).await;
        sqlx::query("UPDATE documents SET parent_id='rest-delete' WHERE id='rest-active'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('archive-own','rest-delete','space-a','owner-a','Working','OWN BODY','base')").execute(&state.db).await.unwrap();
        let mirror = storage::space_dir(&slug).join("archived");
        tokio::fs::create_dir_all(&mirror).await.unwrap();
        tokio::fs::write(mirror.join("existing.md"), b"PREEXISTING MIRROR")
            .await
            .unwrap();
        let mut previous = None;
        for _ in 0..2 {
            let before = super::super::document_deletion::rows(
                &mut *state.db.acquire().await.unwrap(),
                "SELECT * FROM documents WHERE id='rest-delete'",
                &[],
            )
            .await
            .unwrap();
            assert_eq!(
                router()
                    .with_state(state.clone())
                    .oneshot(request("rest-delete", "rest-write"))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::NO_CONTENT
            );
            let after = super::super::document_deletion::rows(
                &mut *state.db.acquire().await.unwrap(),
                "SELECT * FROM documents WHERE id='rest-delete'",
                &[],
            )
            .await
            .unwrap();
            let mut expected = before[0].clone();
            for field in ["archived_at", "updated_at"] {
                expected[field] = after[0][field].clone();
            }
            assert_eq!(after, vec![expected]);
            assert_eq!(after[0]["archived_at"], after[0]["updated_at"]);
            assert!(after[0]["archived_at"].is_string());
            if let Some(old) = previous {
                assert_ne!(old, after[0]["archived_at"]);
            }
            previous = Some(after[0]["archived_at"].clone());
            assert!(after[0]["deleted_at"].is_null());
            assert_eq!(after[0]["public"], 1);
            assert_eq!(after[0]["public_slug"], "archive-link");
            assert_eq!(after[0]["maintenance_user_id"], "editor-a");
        }
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT parent_id FROM documents WHERE id='rest-active'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            "rest-delete"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT content FROM document_drafts WHERE id='archive-own'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            "OWN BODY"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments WHERE id='rest-blob'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM audit_log WHERE action='document.archive' AND target_id='rest-delete' AND user_id='editor-a'").fetch_one(&state.db).await.unwrap(),2);
        assert_eq!(
            tokio::fs::read(mirror.join("existing.md")).await.unwrap(),
            b"PREEXISTING MIRROR"
        );
        retained(&slug, &path).await;
        cleanup(&slug).await;
    }
    #[tokio::test]
    async fn archive_actual_method_and_archive_permission_private_editor_are_required() {
        for mode in [
            "read",
            "viewer",
            "delete-only",
            "private-viewer",
            "foreign",
            "trashed",
        ] {
            let (state, slug, path) = setup(false).await;
            let key = match mode {
                "read" => "rest-read",
                "viewer" => "rest-viewer-write",
                _ => "rest-write",
            };
            if mode == "delete-only" {
                sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('delete-not-archive','space-a','Delete','[\"delete_content\"]','owner-a')").execute(&state.db).await.unwrap();
                sqlx::query("UPDATE space_members SET role='delete-not-archive' WHERE space_id='space-a' AND user_id='editor-a'").execute(&state.db).await.unwrap();
            }
            if mode == "private-viewer" {
                sqlx::query("UPDATE documents SET published_at=NULL WHERE id='rest-delete'")
                    .execute(&state.db)
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('archive-private','rest-delete','space-a','owner-a','Private','base')").execute(&state.db).await.unwrap();
                sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('archive-private','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
            }
            if mode == "trashed" {
                sqlx::query("UPDATE documents SET deleted_at='old' WHERE id='rest-delete'")
                    .execute(&state.db)
                    .await
                    .unwrap();
            }
            let before = tests::move_database_snapshot(&state).await;
            let page = if mode == "foreign" {
                "rest-foreign"
            } else {
                "rest-delete"
            };
            let response = router()
                .with_state(state.clone())
                .oneshot(request(page, key))
                .await
                .unwrap();
            assert!(!response.status().is_success(), "{mode}");
            sqlx::query("UPDATE api_keys SET last_used_at=NULL WHERE id=$1")
                .bind(key)
                .execute(&state.db)
                .await
                .unwrap();
            assert_eq!(
                tests::move_database_snapshot(&state).await,
                before,
                "{mode}"
            );
            retained(&slug, &path).await;
            cleanup(&slug).await;
        }
        let (state, slug, path) = setup(false).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('archive-only','space-a','Archive','[\"archive_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='archive-only' WHERE space_id='space-a' AND user_id='editor-a'").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='rest-delete'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('archive-private','rest-delete','space-a','owner-a','Private','base')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('archive-private','editor-a','editor','owner-a')").execute(&state.db).await.unwrap();
        assert_eq!(
            router()
                .with_state(state.clone())
                .oneshot(request("rest-delete", "rest-write"))
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT
        );
        retained(&slug, &path).await;
        cleanup(&slug).await;
    }
    #[tokio::test]
    async fn archive_late_canonical_and_audit_source_drift_roll_back_all_rows_and_bytes() {
        for trigger in [
            "CREATE TRIGGER archive_drift AFTER UPDATE OF archived_at ON documents WHEN NEW.id='rest-delete' BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END",
            "CREATE TRIGGER archive_drift AFTER UPDATE OF archived_at ON documents WHEN NEW.id='rest-delete' BEGIN UPDATE documents SET public_slug='Unexpected' WHERE id='rest-delete'; END",
            "CREATE TRIGGER archive_drift AFTER INSERT ON audit_log WHEN NEW.action='document.archive' BEGIN UPDATE spaces SET visibility='public' WHERE id='space-a'; END",
            "CREATE TRIGGER archive_drift AFTER INSERT ON audit_log WHEN NEW.action='document.archive' BEGIN UPDATE documents SET title='Injected' WHERE id='rest-active'; END",
            "CREATE TRIGGER archive_drift AFTER INSERT ON audit_log WHEN NEW.action='document.archive' BEGIN UPDATE audit_log SET after_value='Injected' WHERE id=NEW.id; END",
            "CREATE TRIGGER archive_drift BEFORE INSERT ON audit_log WHEN NEW.action='document.archive' BEGIN SELECT RAISE(ABORT,'audit denied'); END"] {
            let(state,slug,path)=setup(false).await;sqlx::query(trigger).execute(&state.db).await.unwrap();let a=auth(&state).await;let before=tests::move_database_snapshot(&state).await;
            assert!(archive_document(State(state.clone()),a,Path(("space-a".into(),"rest-delete".into()))).await.is_err(),"{trigger}");assert_eq!(tests::move_database_snapshot(&state).await,before,"{trigger}");retained(&slug,&path).await;cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn archive_wal_audience_wait_rechecks_original_key_and_actual_source() {
        for change in [
            "UPDATE api_keys SET scopes='[\"read\"]' WHERE id='rest-write'",
            "UPDATE documents SET space_id='space-b' WHERE id='rest-delete'",
        ] {
            let (state, slug, path) = setup(true).await;
            let a = auth(&state).await;
            let mut hold = state.db.begin().await.unwrap();
            super::super::mutation_guards::lock_audience_for_mutation(&mut hold)
                .await
                .unwrap();
            let copied = state.clone();
            let task = tokio::spawn(async move {
                archive_document(
                    State(copied),
                    a,
                    Path(("space-a".into(), "rest-delete".into())),
                )
                .await
            });
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!task.is_finished());
            sqlx::query(change).execute(&mut *hold).await.unwrap();
            hold.commit().await.unwrap();
            let before = tests::move_database_snapshot(&state).await;
            assert!(task.await.unwrap().is_err());
            assert_eq!(tests::move_database_snapshot(&state).await, before);
            retained(&slug, &path).await;
            cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn archive_last_original_jwt_expiry_rolls_back_transition_and_audit_after_real_proofs() {
        let (state, slug, path) = setup(false).await;
        let mut claims = crate::spaces::properties::tests::test_auth("editor-a").claims;
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let req = Request::builder()
            .method("POST")
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let a = AuthUser::from_request_parts(&mut parts, &state)
            .await
            .unwrap();
        let before = tests::move_database_snapshot(&state).await;
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen = reached.clone();
        let result = super::super::lifecycle::ARCHIVE_BEFORE_IDENTITY
            .scope(
                std::cell::RefCell::new(Some(Box::pin(async move {
                    seen.store(true, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await
                }))),
                archive_document(
                    State(state.clone()),
                    a,
                    Path(("space-a".into(), "rest-delete".into())),
                ),
            )
            .await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));
        assert!(matches!(result, Err(AppError::Unauthorized(_))));
        assert_eq!(tests::move_database_snapshot(&state).await, before);
        retained(&slug, &path).await;
        cleanup(&slug).await;
    }
    #[tokio::test]
    async fn archive_confirmed_rollback_and_restored_stamp_do_not_emit_committed_effects() {
        let (state, slug, path) = setup(false).await;
        let a = auth(&state).await;
        let before = tests::move_database_snapshot(&state).await;
        assert!(super::super::lifecycle::ARCHIVE_ROLLBACK
            .scope(
                true,
                archive_document(
                    State(state.clone()),
                    a,
                    Path(("space-a".into(), "rest-delete".into()))
                )
            )
            .await
            .is_err());
        assert_eq!(tests::move_database_snapshot(&state).await, before);
        let a = auth(&state).await;
        let committed = super::super::lifecycle::archive(&state, &a, "space-a", "rest-delete")
            .await
            .unwrap();
        sqlx::query(
            "UPDATE documents SET archived_at=NULL,updated_at='restored' WHERE id='rest-delete'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        state
            .search_index
            .upsert(crate::search::index::DocEntry {
                doc_id: "rest-delete".into(),
                space_id: "space-a".into(),
                title: "Restoredmarker".into(),
                body: "Fresh restored body".into(),
                tags: "".into(),
            })
            .unwrap();
        let mut visible = false;
        for _ in 0..20 {
            if !state
                .search_index
                .search("Restoredmarker", Some("space-a"), 10)
                .unwrap()
                .is_empty()
            {
                visible = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert!(
            visible,
            "restored index entry must be visible before stale advisory removal"
        );
        assert!(
            super::super::lifecycle::remove_index(&state, &a, &committed)
                .await
                .is_err()
        );
        assert_eq!(
            state
                .search_index
                .search("Restoredmarker", Some("space-a"), 10)
                .unwrap()
                .len(),
            1
        );
        retained(&slug, &path).await;
        cleanup(&slug).await;
    }
}

#[cfg(test)]
mod unarchive_transition_current_tests {
    use super::*;
    use axum::{
        body::Body,
        extract::FromRequestParts,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    async fn setup(wal: bool) -> (AppState, String, String) {
        let (state, slug, path) = rest_permanent_delete_current_tests::fixture(wal).await;
        sqlx::query("UPDATE documents SET deleted_at=NULL,deleted_by=NULL,archived_at='old-archive',public=1,public_slug='archive-link',human_identifier='ARCHIVE.1',human_identifier_key='ARCHIVE.1',maintenance_user_id='editor-a' WHERE id='rest-delete'").execute(&state.db).await.unwrap();
        (state, slug, path)
    }
    fn request(page: &str, key: &str) -> Request<Body> {
        Request::builder()
            .method("DELETE")
            .uri(format!("/spaces/space-a/documents/{page}/archive"))
            .header("X-API-Key", key)
            .body(Body::empty())
            .unwrap()
    }
    async fn auth(state: &AppState) -> AuthUser {
        let (mut parts, _) = request("rest-delete", "rest-write").into_parts();
        AuthUser::from_request_parts(&mut parts, state)
            .await
            .unwrap()
    }
    async fn cleanup(slug: &str) {
        let _ = tokio::fs::remove_dir_all(storage::space_dir(slug)).await;
    }
    async fn retained(slug: &str, path: &str) {
        assert_eq!(
            tokio::fs::read(storage::data_dir().join(path))
                .await
                .unwrap(),
            b"RETAINED EXACT BODY"
        );
        assert_eq!(
            tokio::fs::read(storage::space_dir(slug).join("docs/blob.bin"))
                .await
                .unwrap(),
            b"RETAINED BLOB"
        );
    }

    #[tokio::test]
    async fn unarchive_http_exact_active_outcome_preserves_children_flags_and_all_bytes() {
        let (state, slug, path) = setup(false).await;
        sqlx::query("UPDATE documents SET parent_id='rest-delete' WHERE id='rest-active'")
            .execute(&state.db)
            .await
            .unwrap();
        let mirror = storage::space_dir(&slug).join("archived/existing.md");
        tokio::fs::create_dir_all(mirror.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&mirror, b"PREEXISTING MIRROR")
            .await
            .unwrap();
        let before = super::super::document_deletion::rows(
            &mut *state.db.acquire().await.unwrap(),
            "SELECT * FROM documents WHERE id='rest-delete'",
            &[],
        )
        .await
        .unwrap();
        assert_eq!(
            router()
                .with_state(state.clone())
                .oneshot(request("rest-delete", "rest-write"))
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT
        );
        let after = super::super::document_deletion::rows(
            &mut *state.db.acquire().await.unwrap(),
            "SELECT * FROM documents WHERE id='rest-delete'",
            &[],
        )
        .await
        .unwrap();
        let mut expected = before[0].clone();
        expected["archived_at"] = serde_json::Value::Null;
        expected["updated_at"] = after[0]["updated_at"].clone();
        assert_eq!(after, vec![expected]);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT parent_id FROM documents WHERE id='rest-active'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            "rest-delete"
        );
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM audit_log WHERE action='document.unarchive' AND user_id='editor-a' AND target_id='rest-delete'").fetch_one(&state.db).await.unwrap(),1);
        assert_eq!(
            tokio::fs::read(&mirror).await.unwrap(),
            b"PREEXISTING MIRROR"
        );
        retained(&slug, &path).await;
        let a = auth(&state).await;
        let before = tests::move_database_snapshot(&state).await;
        assert!(matches!(
            unarchive_document(
                State(state.clone()),
                a,
                Path(("space-a".into(), "rest-delete".into()))
            )
            .await,
            Err(AppError::NotFound(_))
        ));
        assert_eq!(tests::move_database_snapshot(&state).await, before);
        cleanup(&slug).await;
    }
    #[tokio::test]
    async fn unarchive_confirmed_rollback_retains_source_audit_and_bytes() {
        let (state, slug, path) = setup(false).await;
        let a = auth(&state).await;
        let before = tests::move_database_snapshot(&state).await;
        assert!(super::super::lifecycle::UNARCHIVE_ROLLBACK
            .scope(
                true,
                unarchive_document(
                    State(state.clone()),
                    a,
                    Path(("space-a".into(), "rest-delete".into()))
                )
            )
            .await
            .is_err());
        assert_eq!(tests::move_database_snapshot(&state).await, before);
        retained(&slug, &path).await;
        cleanup(&slug).await;
    }
    #[tokio::test]
    async fn unarchive_actual_method_and_unarchive_permission_private_editor_are_required() {
        for mode in [
            "read",
            "viewer",
            "delete-only",
            "private-viewer",
            "foreign",
            "trashed",
            "active",
        ] {
            let (state, slug, path) = setup(false).await;
            let key = match mode {
                "read" => "rest-read",
                "viewer" => "rest-viewer-write",
                _ => "rest-write",
            };
            if mode == "delete-only" {
                sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('delete-not-archive','space-a','Delete','[\"delete_content\"]','owner-a')").execute(&state.db).await.unwrap();
                sqlx::query("UPDATE space_members SET role='delete-not-archive' WHERE space_id='space-a' AND user_id='editor-a'").execute(&state.db).await.unwrap();
            }
            if mode == "private-viewer" {
                sqlx::query("UPDATE documents SET published_at=NULL WHERE id='rest-delete'")
                    .execute(&state.db)
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('archive-private','rest-delete','space-a','owner-a','Private','base')").execute(&state.db).await.unwrap();
                sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('archive-private','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
            }
            if mode == "active" {
                sqlx::query("UPDATE documents SET archived_at=NULL WHERE id='rest-delete'")
                    .execute(&state.db)
                    .await
                    .unwrap();
            }
            if mode == "trashed" {
                sqlx::query("UPDATE documents SET deleted_at='old' WHERE id='rest-delete'")
                    .execute(&state.db)
                    .await
                    .unwrap();
            }
            let before = tests::move_database_snapshot(&state).await;
            let page = if mode == "foreign" {
                "rest-foreign"
            } else {
                "rest-delete"
            };
            let response = router()
                .with_state(state.clone())
                .oneshot(request(page, key))
                .await
                .unwrap();
            assert!(!response.status().is_success(), "{mode}");
            sqlx::query("UPDATE api_keys SET last_used_at=NULL WHERE id=$1")
                .bind(key)
                .execute(&state.db)
                .await
                .unwrap();
            assert_eq!(
                tests::move_database_snapshot(&state).await,
                before,
                "{mode}"
            );
            retained(&slug, &path).await;
            cleanup(&slug).await;
        }
        let (state, slug, path) = setup(false).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('archive-only','space-a','Archive','[\"archive_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='archive-only' WHERE space_id='space-a' AND user_id='editor-a'").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='rest-delete'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('archive-private','rest-delete','space-a','owner-a','Private','base')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('archive-private','editor-a','editor','owner-a')").execute(&state.db).await.unwrap();
        assert_eq!(
            router()
                .with_state(state.clone())
                .oneshot(request("rest-delete", "rest-write"))
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT
        );
        retained(&slug, &path).await;
        cleanup(&slug).await;
    }
    #[tokio::test]
    async fn unarchive_late_canonical_and_audit_source_drift_roll_back_all_rows_and_bytes() {
        for trigger in [
            "CREATE TRIGGER unarchive_drift AFTER UPDATE OF archived_at ON documents WHEN NEW.id='rest-delete' BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END",
            "CREATE TRIGGER unarchive_drift AFTER UPDATE OF archived_at ON documents WHEN NEW.id='rest-delete' BEGIN UPDATE documents SET public_slug='Unexpected' WHERE id='rest-delete'; END",
            "CREATE TRIGGER unarchive_drift AFTER INSERT ON audit_log WHEN NEW.action='document.unarchive' BEGIN UPDATE spaces SET visibility='public' WHERE id='space-a'; END",
            "CREATE TRIGGER unarchive_drift AFTER INSERT ON audit_log WHEN NEW.action='document.unarchive' BEGIN UPDATE documents SET title='Injected' WHERE id='rest-active'; END",
            "CREATE TRIGGER unarchive_drift AFTER INSERT ON audit_log WHEN NEW.action='document.unarchive' BEGIN UPDATE audit_log SET after_value='Injected' WHERE id=NEW.id; END",
            "CREATE TRIGGER unarchive_drift BEFORE INSERT ON audit_log WHEN NEW.action='document.unarchive' BEGIN SELECT RAISE(ABORT,'audit denied'); END"] {
            let(state,slug,path)=setup(false).await;sqlx::query(trigger).execute(&state.db).await.unwrap();let a=auth(&state).await;let before=tests::move_database_snapshot(&state).await;
            assert!(unarchive_document(State(state.clone()),a,Path(("space-a".into(),"rest-delete".into()))).await.is_err(),"{trigger}");assert_eq!(tests::move_database_snapshot(&state).await,before,"{trigger}");retained(&slug,&path).await;cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn unarchive_wal_audience_wait_rechecks_original_key_and_actual_source() {
        for change in [
            "UPDATE api_keys SET scopes='[\"read\"]' WHERE id='rest-write'",
            "UPDATE documents SET space_id='space-b' WHERE id='rest-delete'",
        ] {
            let (state, slug, path) = setup(true).await;
            let a = auth(&state).await;
            let mut hold = state.db.begin().await.unwrap();
            super::super::mutation_guards::lock_audience_for_mutation(&mut hold)
                .await
                .unwrap();
            let copied = state.clone();
            let task = tokio::spawn(async move {
                unarchive_document(
                    State(copied),
                    a,
                    Path(("space-a".into(), "rest-delete".into())),
                )
                .await
            });
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!task.is_finished());
            sqlx::query(change).execute(&mut *hold).await.unwrap();
            hold.commit().await.unwrap();
            let before = tests::move_database_snapshot(&state).await;
            assert!(task.await.unwrap().is_err());
            assert_eq!(tests::move_database_snapshot(&state).await, before);
            retained(&slug, &path).await;
            cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn unarchive_last_original_jwt_expiry_rolls_back_transition_and_audit_after_real_proofs()
    {
        let (state, slug, path) = setup(false).await;
        let mut claims = crate::spaces::properties::tests::test_auth("editor-a").claims;
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let req = Request::builder()
            .method("DELETE")
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let a = AuthUser::from_request_parts(&mut parts, &state)
            .await
            .unwrap();
        let before = tests::move_database_snapshot(&state).await;
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen = reached.clone();
        let result = super::super::lifecycle::UNARCHIVE_BEFORE_IDENTITY
            .scope(
                std::cell::RefCell::new(Some(Box::pin(async move {
                    seen.store(true, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await
                }))),
                unarchive_document(
                    State(state.clone()),
                    a,
                    Path(("space-a".into(), "rest-delete".into())),
                ),
            )
            .await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));
        assert!(matches!(result, Err(AppError::Unauthorized(_))));
        assert_eq!(tests::move_database_snapshot(&state).await, before);
        retained(&slug, &path).await;
        cleanup(&slug).await;
    }
}

#[cfg(test)]
mod restore_transition_current_tests {
    use super::*;
    use axum::{
        body::Body,
        extract::FromRequestParts,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    async fn setup(wal: bool) -> (AppState, String, String) {
        let (state, slug, path) = rest_permanent_delete_current_tests::fixture(wal).await;
        sqlx::query("UPDATE documents SET deleted_at='old-trash',deleted_by='owner-a',archived_at=NULL,public=1,public_slug='archive-link',human_identifier='ARCHIVE.1',human_identifier_key='ARCHIVE.1',maintenance_user_id='editor-a' WHERE id='rest-delete'").execute(&state.db).await.unwrap();
        (state, slug, path)
    }
    fn request(page: &str, key: &str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(format!("/spaces/space-a/documents/{page}/trash"))
            .header("X-API-Key", key)
            .body(Body::empty())
            .unwrap()
    }
    async fn auth(state: &AppState) -> AuthUser {
        let (mut parts, _) = request("rest-delete", "rest-write").into_parts();
        AuthUser::from_request_parts(&mut parts, state)
            .await
            .unwrap()
    }
    async fn cleanup(slug: &str) {
        let _ = tokio::fs::remove_dir_all(storage::space_dir(slug)).await;
    }
    async fn retained(slug: &str, path: &str) {
        assert_eq!(
            tokio::fs::read(storage::data_dir().join(path))
                .await
                .unwrap(),
            b"RETAINED EXACT BODY"
        );
        assert_eq!(
            tokio::fs::read(storage::space_dir(slug).join("docs/blob.bin"))
                .await
                .unwrap(),
            b"RETAINED BLOB"
        );
    }

    #[tokio::test]
    async fn restore_http_exact_active_outcome_preserves_children_flags_and_all_bytes() {
        let (state, slug, path) = setup(false).await;
        sqlx::query("UPDATE documents SET parent_id='rest-delete' WHERE id='rest-active'")
            .execute(&state.db)
            .await
            .unwrap();
        let mirror = storage::space_dir(&slug).join("archived/existing.md");
        tokio::fs::create_dir_all(mirror.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&mirror, b"PREEXISTING MIRROR")
            .await
            .unwrap();
        let before = super::super::document_deletion::rows(
            &mut *state.db.acquire().await.unwrap(),
            "SELECT * FROM documents WHERE id='rest-delete'",
            &[],
        )
        .await
        .unwrap();
        assert_eq!(
            router()
                .with_state(state.clone())
                .oneshot(request("rest-delete", "rest-write"))
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT
        );
        let after = super::super::document_deletion::rows(
            &mut *state.db.acquire().await.unwrap(),
            "SELECT * FROM documents WHERE id='rest-delete'",
            &[],
        )
        .await
        .unwrap();
        let mut expected = before[0].clone();
        expected["deleted_at"] = serde_json::Value::Null;
        expected["deleted_by"] = serde_json::Value::Null;
        expected["updated_at"] = after[0]["updated_at"].clone();
        assert_eq!(after, vec![expected]);
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT parent_id FROM documents WHERE id='rest-active'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            "rest-delete"
        );
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM audit_log WHERE action='document.restore' AND user_id='editor-a' AND target_id='rest-delete'").fetch_one(&state.db).await.unwrap(),1);
        assert_eq!(
            tokio::fs::read(&mirror).await.unwrap(),
            b"PREEXISTING MIRROR"
        );
        retained(&slug, &path).await;
        let a = auth(&state).await;
        let before = tests::move_database_snapshot(&state).await;
        assert!(matches!(
            restore_document(
                State(state.clone()),
                a,
                Path(("space-a".into(), "rest-delete".into()))
            )
            .await,
            Err(AppError::NotFound(_))
        ));
        assert_eq!(tests::move_database_snapshot(&state).await, before);
        cleanup(&slug).await;
    }
    #[tokio::test]
    async fn restore_confirmed_rollback_retains_source_audit_and_bytes() {
        let (state, slug, path) = setup(false).await;
        let a = auth(&state).await;
        let before = tests::move_database_snapshot(&state).await;
        assert!(super::super::lifecycle::RESTORE_ROLLBACK
            .scope(
                true,
                restore_document(
                    State(state.clone()),
                    a,
                    Path(("space-a".into(), "rest-delete".into()))
                )
            )
            .await
            .is_err());
        assert_eq!(tests::move_database_snapshot(&state).await, before);
        retained(&slug, &path).await;
        cleanup(&slug).await;
    }
    #[tokio::test]
    async fn restore_actual_method_and_restore_permission_private_editor_are_required() {
        for mode in [
            "read",
            "viewer",
            "archive-only",
            "private-viewer",
            "foreign",
            "archived-only",
            "active",
        ] {
            let (state, slug, path) = setup(false).await;
            let key = match mode {
                "read" => "rest-read",
                "viewer" => "rest-viewer-write",
                _ => "rest-write",
            };
            if mode == "archive-only" {
                sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('archive-not-delete','space-a','Delete','[\"archive_content\"]','owner-a')").execute(&state.db).await.unwrap();
                sqlx::query("UPDATE space_members SET role='archive-not-delete' WHERE space_id='space-a' AND user_id='editor-a'").execute(&state.db).await.unwrap();
            }
            if mode == "private-viewer" {
                sqlx::query("UPDATE documents SET published_at=NULL WHERE id='rest-delete'")
                    .execute(&state.db)
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('archive-private','rest-delete','space-a','owner-a','Private','base')").execute(&state.db).await.unwrap();
                sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('archive-private','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
            }
            if mode == "active" {
                sqlx::query(
                    "UPDATE documents SET deleted_at=NULL,deleted_by=NULL WHERE id='rest-delete'",
                )
                .execute(&state.db)
                .await
                .unwrap();
            }
            if mode == "archived-only" {
                sqlx::query("UPDATE documents SET deleted_at=NULL,deleted_by=NULL,archived_at='old-archive' WHERE id='rest-delete'")
                    .execute(&state.db)
                    .await
                    .unwrap();
            }
            let before = tests::move_database_snapshot(&state).await;
            let page = if mode == "foreign" {
                "rest-foreign"
            } else {
                "rest-delete"
            };
            let response = router()
                .with_state(state.clone())
                .oneshot(request(page, key))
                .await
                .unwrap();
            assert!(!response.status().is_success(), "{mode}");
            sqlx::query("UPDATE api_keys SET last_used_at=NULL WHERE id=$1")
                .bind(key)
                .execute(&state.db)
                .await
                .unwrap();
            assert_eq!(
                tests::move_database_snapshot(&state).await,
                before,
                "{mode}"
            );
            retained(&slug, &path).await;
            cleanup(&slug).await;
        }
        let (state, slug, path) = setup(false).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('delete-only','space-a','Delete','[\"delete_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='delete-only' WHERE space_id='space-a' AND user_id='editor-a'").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='rest-delete'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('archive-private','rest-delete','space-a','owner-a','Private','base')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('archive-private','editor-a','editor','owner-a')").execute(&state.db).await.unwrap();
        assert_eq!(
            router()
                .with_state(state.clone())
                .oneshot(request("rest-delete", "rest-write"))
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT
        );
        retained(&slug, &path).await;
        cleanup(&slug).await;
    }
    #[tokio::test]
    async fn restore_late_canonical_and_audit_source_drift_roll_back_all_rows_and_bytes() {
        for trigger in [
            "CREATE TRIGGER restore_drift AFTER UPDATE OF deleted_at ON documents WHEN NEW.id='rest-delete' BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END",
            "CREATE TRIGGER restore_drift AFTER UPDATE OF deleted_at ON documents WHEN NEW.id='rest-delete' BEGIN UPDATE documents SET public_slug='Unexpected' WHERE id='rest-delete'; END",
            "CREATE TRIGGER restore_drift AFTER INSERT ON audit_log WHEN NEW.action='document.restore' BEGIN UPDATE spaces SET visibility='public' WHERE id='space-a'; END",
            "CREATE TRIGGER restore_drift AFTER INSERT ON audit_log WHEN NEW.action='document.restore' BEGIN UPDATE documents SET title='Injected' WHERE id='rest-active'; END",
            "CREATE TRIGGER restore_drift AFTER INSERT ON audit_log WHEN NEW.action='document.restore' BEGIN UPDATE audit_log SET after_value='Injected' WHERE id=NEW.id; END",
            "CREATE TRIGGER restore_drift BEFORE INSERT ON audit_log WHEN NEW.action='document.restore' BEGIN SELECT RAISE(ABORT,'audit denied'); END"] {
            let(state,slug,path)=setup(false).await;sqlx::query(trigger).execute(&state.db).await.unwrap();let a=auth(&state).await;let before=tests::move_database_snapshot(&state).await;
            assert!(restore_document(State(state.clone()),a,Path(("space-a".into(),"rest-delete".into()))).await.is_err(),"{trigger}");assert_eq!(tests::move_database_snapshot(&state).await,before,"{trigger}");retained(&slug,&path).await;cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn restore_wal_audience_wait_rechecks_original_key_and_actual_source() {
        for change in [
            "UPDATE api_keys SET scopes='[\"read\"]' WHERE id='rest-write'",
            "UPDATE documents SET space_id='space-b' WHERE id='rest-delete'",
        ] {
            let (state, slug, path) = setup(true).await;
            let a = auth(&state).await;
            let mut hold = state.db.begin().await.unwrap();
            super::super::mutation_guards::lock_audience_for_mutation(&mut hold)
                .await
                .unwrap();
            let copied = state.clone();
            let task = tokio::spawn(async move {
                restore_document(
                    State(copied),
                    a,
                    Path(("space-a".into(), "rest-delete".into())),
                )
                .await
            });
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!task.is_finished());
            sqlx::query(change).execute(&mut *hold).await.unwrap();
            hold.commit().await.unwrap();
            let before = tests::move_database_snapshot(&state).await;
            assert!(task.await.unwrap().is_err());
            assert_eq!(tests::move_database_snapshot(&state).await, before);
            retained(&slug, &path).await;
            cleanup(&slug).await;
        }
    }
    #[tokio::test]
    async fn restore_last_original_jwt_expiry_rolls_back_transition_and_audit_after_real_proofs() {
        let (state, slug, path) = setup(false).await;
        let mut claims = crate::spaces::properties::tests::test_auth("editor-a").claims;
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let req = Request::builder()
            .method("POST")
            .header("Authorization", format!("Bearer {token}"))
            .body(Body::empty())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let a = AuthUser::from_request_parts(&mut parts, &state)
            .await
            .unwrap();
        let before = tests::move_database_snapshot(&state).await;
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let seen = reached.clone();
        let result = super::super::lifecycle::RESTORE_BEFORE_IDENTITY
            .scope(
                std::cell::RefCell::new(Some(Box::pin(async move {
                    seen.store(true, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await
                }))),
                restore_document(
                    State(state.clone()),
                    a,
                    Path(("space-a".into(), "rest-delete".into())),
                ),
            )
            .await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));
        assert!(matches!(result, Err(AppError::Unauthorized(_))));
        assert_eq!(tests::move_database_snapshot(&state).await, before);
        retained(&slug, &path).await;
        cleanup(&slug).await;
    }

    #[tokio::test]
    async fn restore_anomalous_archived_trash_retains_archive_stamp_and_denies_body() {
        let (state, slug, path) = setup(false).await;
        sqlx::query("UPDATE documents SET archived_at='RETAINED ARCHIVE' WHERE id='rest-delete'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            router()
                .with_state(state.clone())
                .oneshot(request("rest-delete", "rest-write"))
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT
        );
        let row: (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT deleted_at,deleted_by,archived_at FROM documents WHERE id='rest-delete'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(row, (None, None, Some("RETAINED ARCHIVE".into())));
        let req = Request::builder()
            .method("GET")
            .uri("/spaces/space-a/documents/rest-delete/content")
            .header("X-API-Key", "rest-read")
            .body(Body::empty())
            .unwrap();
        assert_eq!(
            router()
                .with_state(state.clone())
                .oneshot(req)
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        retained(&slug, &path).await;
        cleanup(&slug).await;
    }
}

#[cfg(all(test,target_os="linux"))]
#[path="permanent_erasure_plan_tests.rs"]
mod permanent_erasure_plan_tests;

async fn cleanup_permanent_document_attachments(State(state):State<AppState>,auth:AuthUser,Path((space,document,operation)):Path<(String,String,String)>,Json(request):Json<storage::attachment_erasure_plan::OriginalDeleteRequest>)->Result<Json<storage::attachment_upload::admission::committed_erasure::CleanupOutcome>,AppError>{
 storage::attachment_upload::admission::committed_erasure::cleanup(&state.db,&auth,&space,&document,&operation,&request.original_target).await.map(Json)
}
#[cfg(test)]
#[path = "page_edit_capability_tests.rs"]
mod page_edit_capability_tests;
