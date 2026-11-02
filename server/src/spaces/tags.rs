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

fn map_tag_database_error(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(database_error) = &error {
        if database_error
            .message()
            .contains("UNIQUE constraint failed: tags.space_id, tags.name")
        {
            return AppError::Conflict("A tag with this name already exists in this space".into());
        }
    }
    AppError::internal(error)
}

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
    Ok(Json(read_tag_catalog(&state, &auth, &space_id).await?))
}
pub(crate) async fn read_tag_catalog(
    state: &AppState,
    auth: &AuthUser,
    space_id: &str,
) -> Result<Vec<TagResponse>, AppError> {
    metadata_actor(auth)?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_metadata_space_view(&mut conn, auth, space_id).await?;
    let tags: Vec<(String, String, String, String)> =
        sqlx::query_as("SELECT id,name,color,created_at FROM tags WHERE space_id=$1 ORDER BY name")
            .bind(space_id)
            .fetch_all(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    let associations:Vec<(String,String)>=sqlx::query_as("SELECT dt.tag_id,dt.document_id FROM doc_tags dt JOIN tags t ON t.id=dt.tag_id JOIN documents d ON d.id=dt.document_id WHERE t.space_id=$1 AND d.space_id=$1").bind(space_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    #[cfg(test)]
    super::properties::tests::pause_metadata_read().await;
    let mut visible = std::collections::BTreeSet::new();
    let pages: std::collections::BTreeSet<_> =
        associations.iter().map(|(_, doc)| doc.clone()).collect();
    for page in pages {
        match require_metadata_page_view(&mut conn, auth, space_id, &page).await {
            Ok(()) => {
                visible.insert(page);
            }
            Err(AppError::NotFound(_) | AppError::Forbidden(_)) => {}
            Err(e) => return Err(e),
        }
    }
    // Counts may not reveal inaccessible, archived, deleted or corrupt foreign pages.
    let associations:Vec<(String,String)>=sqlx::query_as("SELECT dt.tag_id,dt.document_id FROM doc_tags dt JOIN tags t ON t.id=dt.tag_id JOIN documents d ON d.id=dt.document_id WHERE t.space_id=$1 AND d.space_id=$1").bind(space_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let mut current = std::collections::BTreeSet::new();
    for page in visible {
        match require_metadata_page_view(&mut conn, auth, space_id, &page).await {
            Ok(()) => {
                current.insert(page);
            }
            Err(AppError::NotFound(_) | AppError::Forbidden(_)) => {}
            Err(e) => return Err(e),
        }
    }
    let result: Vec<TagResponse> = tags
        .into_iter()
        .map(|(id, name, color, created_at)| {
            let count = associations
                .iter()
                .filter(|(tag, page)| tag == &id && current.contains(page))
                .count() as i64;
            TagResponse {
                id,
                name,
                color,
                created_at: Some(created_at),
                doc_count: Some(count),
            }
        })
        .collect();
    require_metadata_space_view(&mut conn, auth, space_id).await?;
    Ok(result)
}

/// Transport adapters prepare identity/timestamps; this core owns all tag writes
/// and applies the same actor, scope, page and transaction policy to REST/replay.
#[derive(Clone, Copy)]
pub(crate) enum TagMutationOrigin {
    Rest,
    Replay,
}

pub(crate) enum TagMutation {
    Create {
        id: String,
        name: String,
        color: String,
        created_at: String,
        match_existing_timestamp: bool,
    },
    Update {
        id: String,
        name: String,
        color: Option<String>,
    },
    Delete {
        id: String,
    },
    Associate {
        document_id: String,
        tag_id: String,
        add: bool,
    },
}

pub(crate) enum TagMutationResult {
    Created(TagResponse),
    Existing(TagResponse),
    Applied,
    Unchanged,
}

impl TagMutationResult {
    pub(crate) fn replay_status(&self) -> &'static str {
        match self {
            Self::Created(_) | Self::Applied => "applied",
            Self::Existing(_) | Self::Unchanged => "skipped",
        }
    }
}

type StoredTag = (String, String, String, String, String);
fn stored_tag_response((id, _, name, color, created_at): StoredTag) -> TagResponse {
    TagResponse {
        id,
        name,
        color,
        created_at: Some(created_at),
        doc_count: Some(0),
    }
}

fn metadata_actor(auth: &AuthUser) -> Result<&str, AppError> {
    if matches!(
        super::access::principal_from_auth(auth)?,
        super::access::Principal::ServiceKey { .. }
    ) {
        return Err(AppError::Unauthorized(
            "Metadata principal unavailable".into(),
        ));
    }
    Ok(auth.claims.sub.as_str())
}
async fn require_metadata_space_mutation(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    owner_only: bool,
) -> Result<(), AppError> {
    let access = super::access::resolve_space_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        super::access::Purpose::Routine,
    )
    .await?;
    if !access.allows(super::access::Permission::EditContent)
        || (owner_only && !access.allows(super::access::Permission::ManageSpace))
    {
        return Err(AppError::NotFound("Metadata target unavailable".into()));
    }
    Ok(())
}
async fn require_metadata_page_mutation(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    page: &str,
) -> Result<(), AppError> {
    require_metadata_space_mutation(conn, auth, space, false).await?;
    let locked=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(page).bind(space).execute(&mut *conn).await.map_err(AppError::internal)?;
    if locked.rows_affected() != 1 {
        return Err(AppError::NotFound("Document unavailable".into()));
    }
    super::access::resolve_page_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        page,
        super::access::PageOperation::Edit,
        super::access::Purpose::Routine,
    )
    .await?;
    Ok(())
}

pub(crate) async fn mutate_tag(
    state: &AppState,
    auth: &AuthUser,
    requested_space: Option<&str>,
    origin: TagMutationOrigin,
    mutation: TagMutation,
) -> Result<TagMutationResult, AppError> {
    metadata_actor(auth)?;
    let document_id = match &mutation {
        TagMutation::Associate { document_id, .. } => Some(document_id.as_str()),
        _ => None,
    };
    // Older queued associations do not carry a space. This is only a binding hint:
    // after waiting for the space guard, the page gate resolves that scope again.
    let space_id = match requested_space {
        Some(id) if !id.trim().is_empty() => id.to_owned(),
        Some(_) => return Err(AppError::BadRequest("Space ID is required".into())),
        None => {
            let doc =
                document_id.ok_or_else(|| AppError::BadRequest("Space ID is required".into()))?;
            sqlx::query_scalar::<_, String>("SELECT space_id FROM documents WHERE id = $1")
                .bind(doc)
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?
                .ok_or_else(|| AppError::NotFound("Document unavailable".into()))?
        }
    };
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    if let Some(doc) = document_id {
        require_metadata_page_mutation(&mut tx, auth, &space_id, doc).await?;
    } else {
        require_metadata_space_mutation(&mut tx, auth, &space_id, false).await?;
    }
    let outcome = match &mutation {
        TagMutation::Create {
            id,
            name,
            color,
            created_at,
            match_existing_timestamp,
        } => {
            let name = if matches!(origin, TagMutationOrigin::Rest) {
                name.trim()
            } else {
                name.as_str()
            };
            if id.trim().is_empty() || name.trim().is_empty() {
                return Err(AppError::BadRequest(
                    "Tag identity and name are required".into(),
                ));
            }
            let existing: Option<StoredTag> =
                sqlx::query_as("SELECT id,space_id,name,color,created_at FROM tags WHERE id = $1")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(AppError::internal)?;
            if let Some(existing) = existing {
                if existing.1 != space_id {
                    return Err(AppError::NotFound("Tag not found in this space".into()));
                }
                if matches!(origin, TagMutationOrigin::Rest) {
                    return Err(AppError::Conflict(
                        "A tag with this import id already exists".into(),
                    ));
                }
                let timestamp_matches = existing.4 == *created_at
                    || chrono::DateTime::parse_from_rfc3339(&existing.4)
                        .ok()
                        .zip(chrono::DateTime::parse_from_rfc3339(created_at).ok())
                        .is_some_and(|(left, right)| left == right);
                if existing.2 != name
                    || existing.3 != *color
                    || (*match_existing_timestamp && !timestamp_matches)
                {
                    return Err(AppError::Conflict("Tag import identity already has different metadata; review the retained replay payload".into()));
                }
                TagMutationResult::Existing(stored_tag_response(existing))
            } else {
                let duplicate: Option<StoredTag> = sqlx::query_as("SELECT id,space_id,name,color,created_at FROM tags WHERE space_id = $1 AND lower(name) = lower($2) LIMIT 1")
                    .bind(&space_id).bind(name).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
                if let Some(existing) = duplicate {
                    // Replay cannot remap future queued references to another tag ID.
                    if matches!(origin, TagMutationOrigin::Replay) {
                        return Err(AppError::Conflict("Tag name is already bound to a different ID; review the retained replay payload".into()));
                    }
                    TagMutationResult::Existing(stored_tag_response(existing))
                } else {
                    let row = sqlx::query("INSERT INTO tags (id,space_id,name,color,created_at) VALUES ($1,$2,$3,$4,$5)")
                        .bind(id).bind(&space_id).bind(name).bind(color).bind(created_at).execute(&mut *tx).await.map_err(map_tag_database_error)?;
                    if row.rows_affected() != 1 {
                        return Err(AppError::Conflict("Metadata write did not apply".into()));
                    }
                    TagMutationResult::Created(TagResponse {
                        id: id.clone(),
                        name: name.to_owned(),
                        color: color.clone(),
                        created_at: Some(created_at.clone()),
                        doc_count: Some(0),
                    })
                }
            }
        }
        TagMutation::Update { id, name, color } => {
            let name = if matches!(origin, TagMutationOrigin::Rest) {
                name.trim()
            } else {
                name.as_str()
            };
            if id.trim().is_empty() || name.trim().is_empty() {
                return Err(AppError::BadRequest(
                    "Tag identity and name are required".into(),
                ));
            }
            let scoped = sqlx::query("UPDATE tags SET name = name WHERE id = $1 AND space_id = $2")
                .bind(id)
                .bind(&space_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
            if scoped.rows_affected() != 1 {
                return Err(AppError::NotFound("Tag not found in this space".into()));
            }
            require_metadata_space_mutation(&mut tx, auth, &space_id, false).await?;
            let duplicate: Option<i64> = sqlx::query_scalar("SELECT 1 FROM tags WHERE space_id = $1 AND lower(name) = lower($2) AND id <> $3 LIMIT 1")
                .bind(&space_id).bind(name).bind(id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
            if duplicate.is_some() {
                return Err(AppError::Conflict(
                    "A tag with this name already exists in this space".into(),
                ));
            }
            let row = sqlx::query("UPDATE tags SET name = $1,color = COALESCE($2,color) WHERE id = $3 AND space_id = $4")
                .bind(name).bind(color).bind(id).bind(&space_id).execute(&mut *tx).await.map_err(map_tag_database_error)?;
            if row.rows_affected() != 1 {
                return Err(AppError::NotFound("Tag not found in this space".into()));
            }
            TagMutationResult::Applied
        }
        TagMutation::Delete { id } => {
            if id.trim().is_empty() {
                return Err(AppError::BadRequest("Tag identity is required".into()));
            }
            let scoped = sqlx::query("UPDATE tags SET name = name WHERE id = $1 AND space_id = $2")
                .bind(id)
                .bind(&space_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
            require_metadata_space_mutation(&mut tx, auth, &space_id, false).await?;
            if scoped.rows_affected() != 1 {
                // No receipt/tombstone proves an unseen target was deleted. Missing
                // and foreign tags must remain indistinguishable and actionable.
                return Err(AppError::NotFound("Tag not found in this space".into()));
            } else {
                let foreign: Option<i64> = sqlx::query_scalar("SELECT 1 FROM doc_tags dt LEFT JOIN documents d ON d.id = dt.document_id WHERE dt.tag_id = $1 AND (d.id IS NULL OR d.space_id <> $2) LIMIT 1")
                    .bind(id).bind(&space_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
                if foreign.is_some() {
                    return Err(AppError::Conflict(
                        "Tag has associations outside this space; review them before deleting"
                            .into(),
                    ));
                }
                let row = sqlx::query("DELETE FROM tags WHERE id = $1 AND space_id = $2")
                    .bind(id)
                    .bind(&space_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::internal)?;
                if row.rows_affected() != 1 {
                    return Err(AppError::NotFound("Tag not found in this space".into()));
                }
                TagMutationResult::Applied
            }
        }
        TagMutation::Associate {
            document_id,
            tag_id,
            add,
        } => {
            if document_id.trim().is_empty() || tag_id.trim().is_empty() {
                return Err(AppError::BadRequest(
                    "Document and tag IDs are required".into(),
                ));
            }
            let scoped = sqlx::query("UPDATE tags SET name = name WHERE id = $1 AND space_id = $2")
                .bind(tag_id)
                .bind(&space_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
            if scoped.rows_affected() != 1 {
                return Err(AppError::NotFound("Tag not found in this space".into()));
            }
            require_metadata_page_mutation(&mut tx, auth, &space_id, document_id).await?;
            let row = if *add {
                sqlx::query("INSERT INTO doc_tags(document_id,tag_id) VALUES ($1,$2) ON CONFLICT(document_id,tag_id) DO NOTHING")
                    .bind(document_id).bind(tag_id).execute(&mut *tx).await.map_err(AppError::internal)?
            } else {
                sqlx::query("DELETE FROM doc_tags WHERE document_id = $1 AND tag_id = $2")
                    .bind(document_id)
                    .bind(tag_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::internal)?
            };
            match row.rows_affected() {
                0 => TagMutationResult::Unchanged,
                1 => TagMutationResult::Applied,
                _ => return Err(AppError::Conflict("Metadata write did not apply".into())),
            }
        }
    };
    // Canonical INSERT/DELETE/UPDATE can also wait on unadopted tuple/FK writers.
    // A final denial rolls back metadata and cascades; global revocation stability
    // still requires all audience writers to participate in the space protocol.
    if let Some(doc) = document_id {
        require_metadata_page_mutation(&mut tx, auth, &space_id, doc).await?;
    } else {
        require_metadata_space_mutation(&mut tx, auth, &space_id, false).await?;
    }
    tx.commit().await.map_err(AppError::internal)?;
    Ok(outcome)
}

async fn create_tag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<CreateTagRequest>,
) -> Result<impl IntoResponse, AppError> {
    let id = super::routes::import_id_or_uuid(req.source_id.as_deref())?;
    let created_at = super::routes::import_timestamp_or_now(req.created_at.as_deref())?;
    let result = mutate_tag(
        &state,
        &auth,
        Some(&space_id),
        TagMutationOrigin::Rest,
        TagMutation::Create {
            id,
            name: req.name,
            color: req.color.unwrap_or_else(|| "#6b7280".into()),
            created_at,
            match_existing_timestamp: false,
        },
    )
    .await?;
    match result {
        TagMutationResult::Created(tag) => Ok((StatusCode::CREATED, Json(tag))),
        TagMutationResult::Existing(tag) => Ok((StatusCode::OK, Json(tag))),
        _ => Err(AppError::Internal("Unexpected tag create result".into())),
    }
}

async fn update_tag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, tag_id)): Path<(String, String)>,
    Json(req): Json<CreateTagRequest>,
) -> Result<impl IntoResponse, AppError> {
    mutate_tag(
        &state,
        &auth,
        Some(&space_id),
        TagMutationOrigin::Rest,
        TagMutation::Update {
            id: tag_id,
            name: req.name,
            color: req.color,
        },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_tag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, tag_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    mutate_tag(
        &state,
        &auth,
        Some(&space_id),
        TagMutationOrigin::Rest,
        TagMutation::Delete { id: tag_id },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_doc_tags(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    metadata_actor(&auth)?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_metadata_page_view(&mut conn, &auth, &space_id, &doc_id).await?;
    let tags:Vec<(String,String,String,String)>=sqlx::query_as("SELECT t.id,t.name,t.color,t.created_at FROM tags t JOIN doc_tags dt ON t.id=dt.tag_id WHERE dt.document_id=$1 AND t.space_id=$2 ORDER BY t.name").bind(&doc_id).bind(&space_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    #[cfg(test)]
    super::properties::tests::pause_metadata_read().await;
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
    require_metadata_page_view(&mut conn, &auth, &space_id, &doc_id).await?;
    Ok(Json(result))
}

async fn add_doc_tag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    Json(req): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let tag_id = req["tag_id"]
        .as_str()
        .ok_or_else(|| AppError::BadRequest("tag_id required".into()))?
        .to_owned();
    mutate_tag(
        &state,
        &auth,
        Some(&space_id),
        TagMutationOrigin::Rest,
        TagMutation::Associate {
            document_id: doc_id,
            tag_id,
            add: true,
        },
    )
    .await?;
    Ok(StatusCode::CREATED)
}

async fn remove_doc_tag(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id, tag_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    mutate_tag(
        &state,
        &auth,
        Some(&space_id),
        TagMutationOrigin::Rest,
        TagMutation::Associate {
            document_id: doc_id,
            tag_id,
            add: false,
        },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn require_metadata_space_view(
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
    if !access.allows(super::access::Permission::View) {
        return Err(AppError::NotFound("Metadata target unavailable".into()));
    }
    Ok(())
}
async fn require_metadata_page_view(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    page: &str,
) -> Result<(), AppError> {
    super::access::resolve_page_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        page,
        super::access::PageOperation::View,
        super::access::Purpose::Routine,
    )
    .await?;
    Ok(())
}


#[cfg(test)]
pub(crate) async fn test_list_tags(
    state: &AppState,
    auth: AuthUser,
) -> Result<axum::response::Response, AppError> {
    list_tags(State(state.clone()), auth, Path("space-a".into()))
        .await
        .map(IntoResponse::into_response)
}
#[cfg(test)]
pub(crate) async fn test_list_doc_tags(
    state: &AppState,
    auth: AuthUser,
) -> Result<axum::response::Response, AppError> {
    list_doc_tags(
        State(state.clone()),
        auth,
        Path(("space-a".into(), "target-a".into())),
    )
    .await
    .map(IntoResponse::into_response)
}

#[cfg(test)]
mod tests {
    use super::super::properties::tests::{property_test_state, test_auth};
    use super::*;

    #[tokio::test]
    async fn tags_recheck_page_scope_privacy_lifecycle_and_valid_idempotence() {
        let state = property_test_state().await;
        sqlx::query("INSERT INTO tags(id,space_id,name) VALUES ('tag-a','space-a','A'),('tag-b','space-b','B')").execute(&state.db).await.unwrap();
        for (actor, doc, tag) in [
            ("viewer-a", "target-a", "tag-a"),
            ("owner-a", "target-b", "tag-a"),
            ("owner-a", "target-a", "tag-b"),
        ] {
            assert!(add_doc_tag(
                State(state.clone()),
                test_auth(actor),
                Path(("space-a".into(), doc.into())),
                Json(serde_json::json!({"tag_id":tag}))
            )
            .await
            .is_err());
            assert!(remove_doc_tag(
                State(state.clone()),
                test_auth(actor),
                Path(("space-a".into(), doc.into(), tag.into()))
            )
            .await
            .is_err());
        }
        for change in ["UPDATE documents SET published_at=NULL WHERE id='target-a'","UPDATE documents SET published_at='old',archived_at='old' WHERE id='target-a'","UPDATE documents SET archived_at=NULL,deleted_at='old' WHERE id='target-a'","UPDATE documents SET deleted_at=NULL,locked=1,locked_by='editor-a' WHERE id='target-a'"] {
            sqlx::query(change).execute(&state.db).await.unwrap();
            assert!(add_doc_tag(State(state.clone()),test_auth("owner-a"),Path(("space-a".into(),"target-a".into())),Json(serde_json::json!({"tag_id":"tag-a"}))).await.is_err());
        }
        sqlx::query("UPDATE documents SET locked=0 WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        for _ in 0..2 {
            assert!(add_doc_tag(
                State(state.clone()),
                test_auth("editor-a"),
                Path(("space-a".into(), "target-a".into())),
                Json(serde_json::json!({"tag_id":"tag-a"}))
            )
            .await
            .is_ok());
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM doc_tags")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 1);
        for _ in 0..2 {
            assert!(remove_doc_tag(
                State(state.clone()),
                test_auth("editor-a"),
                Path(("space-a".into(), "target-a".into(), "tag-a".into()))
            )
            .await
            .is_ok());
        }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM doc_tags")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn tag_definition_delete_preserves_foreign_association_and_inactive_actor_cannot_create()
    {
        let state = property_test_state().await;
        sqlx::query("INSERT INTO tags(id,space_id,name) VALUES ('tag-a','space-a','A')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO doc_tags(document_id,tag_id) VALUES ('target-b','tag-a')")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(matches!(
            delete_tag(
                State(state.clone()),
                test_auth("owner-a"),
                Path(("space-a".into(), "tag-a".into()))
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM doc_tags")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 1);
        sqlx::query("UPDATE users SET is_active=0 WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(matches!(
            create_tag(
                State(state.clone()),
                test_auth("owner-a"),
                Path("space-a".into()),
                Json(CreateTagRequest {
                    name: "New".into(),
                    color: None,
                    source_id: None,
                    created_at: None
                })
            )
            .await,
            Err(AppError::Unauthorized(_))
        ));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tags")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }
    #[tokio::test]
    async fn rest_tag_adapter_preserves_name_reuse_color_and_current_custom_edit_permission() {
        let state = property_test_state().await;
        sqlx::query("INSERT INTO tags(id,space_id,name,color) VALUES ('existing','space-a','Existing','#123456')").execute(&state.db).await.unwrap();
        let response = create_tag(
            State(state.clone()),
            test_auth("owner-a"),
            Path("space-a".into()),
            Json(CreateTagRequest {
                name: " existing ".into(),
                color: Some("#999999".into()),
                source_id: None,
                created_at: None,
            }),
        )
        .await
        .unwrap()
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 10000)
            .await
            .unwrap();
        let tag: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(tag["id"], "existing");
        assert_eq!(tag["color"], "#123456");
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES ('custom-edit','space-a','Custom','[\"edit_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='custom-edit' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(update_tag(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "existing".into())),
            Json(CreateTagRequest {
                name: " Updated ".into(),
                color: None,
                source_id: None,
                created_at: None
            })
        )
        .await
        .is_ok());
        let row: (String, String) =
            sqlx::query_as("SELECT name,color FROM tags WHERE id='existing'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(row, ("Updated".into(), "#123456".into()));
        assert!(create_tag(
            State(state.clone()),
            test_auth("viewer-a"),
            Path("space-a".into()),
            Json(CreateTagRequest {
                name: "Denied".into(),
                color: None,
                source_id: None,
                created_at: None
            })
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn tags_current_key_scope_and_final_session_checks_preserve_writes() {
        let state = property_test_state().await;
        let auth =
            super::super::properties::tests::metadata_key(&state, "owner-a", r#"["write"]"#).await;
        let request = |name: &str| CreateTagRequest {
            name: name.into(),
            color: None,
            source_id: None,
            created_at: None,
        };
        assert!(create_tag(
            State(state.clone()),
            super::super::properties::tests::copy_auth(&auth),
            Path("space-a".into()),
            Json(request("Writable"))
        )
        .await
        .is_ok());
        assert!(list_tags(
            State(state.clone()),
            super::super::properties::tests::copy_auth(&auth),
            Path("space-a".into())
        )
        .await
        .is_err());
        let id: String = sqlx::query_scalar("SELECT id FROM tags WHERE name='Writable'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert!(add_doc_tag(
            State(state.clone()),
            super::super::properties::tests::copy_auth(&auth),
            Path(("space-a".into(), "target-a".into())),
            Json(serde_json::json!({"tag_id":id}))
        )
        .await
        .is_ok());
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(list_doc_tags(
            State(state.clone()),
            super::super::properties::tests::copy_auth(&auth),
            Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_ok());
        assert!(create_tag(
            State(state.clone()),
            super::super::properties::tests::copy_auth(&auth),
            Path("space-a".into()),
            Json(request("Denied"))
        )
        .await
        .is_err());
        sqlx::query("CREATE TRIGGER revoke_metadata_session AFTER INSERT ON tags BEGIN DELETE FROM sessions WHERE id='metadata-owner-a'; END").execute(&state.db).await.unwrap();
        assert!(create_tag(
            State(state.clone()),
            test_auth("owner-a"),
            Path("space-a".into()),
            Json(request("Rollback"))
        )
        .await
        .is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE name='Rollback'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE id='metadata-owner-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn tag_counts_hide_private_lifecycle_and_foreign_associations() {
        let state = property_test_state().await;
        sqlx::query("INSERT INTO tags(id,space_id,name,color) VALUES('visible-tag','space-a','Visible','#fff'),('foreign-tag','space-b','Hidden','#000')").execute(&state.db).await.unwrap();
        for (id, published, archived, deleted) in [
            ("private", None, None, None),
            ("archived", Some("old"), Some("old"), None),
            ("deleted", Some("old"), None, Some("old")),
        ] {
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at,archived_at,deleted_at) VALUES($1,'space-a',$1,'','owner-a','owner-a',$2,$3,$4)").bind(id).bind(published).bind(archived).bind(deleted).execute(&state.db).await.unwrap();
        }
        for page in ["target-a", "private", "archived", "deleted", "target-b"] {
            sqlx::query("INSERT INTO doc_tags(document_id,tag_id) VALUES($1,'visible-tag')")
                .bind(page)
                .execute(&state.db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO doc_tags(document_id,tag_id) VALUES('target-a','foreign-tag')")
            .execute(&state.db)
            .await
            .unwrap();
        let response = list_tags(
            State(state.clone()),
            test_auth("owner-a"),
            Path("space-a".into()),
        )
        .await
        .unwrap()
        .into_response();
        let bytes = axum::body::to_bytes(response.into_body(), 10000)
            .await
            .unwrap();
        let tags: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(tags[0]["doc_count"], 1);
        let response = list_doc_tags(
            State(state.clone()),
            test_auth("owner-a"),
            Path(("space-a".into(), "target-a".into())),
        )
        .await
        .unwrap()
        .into_response();
        let bytes = axum::body::to_bytes(response.into_body(), 10000)
            .await
            .unwrap();
        let tags: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(tags.as_array().unwrap().len(), 1);
        assert_eq!(tags[0]["id"], "visible-tag");
    }
}
