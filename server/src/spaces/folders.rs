use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::AnyPool;

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::AppState;

pub(crate) const MAX_FOLDER_DEPTH: i32 = 3;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/spaces/{space_id}/folders",
            get(list_folders).post(create_folder),
        )
        .route(
            "/spaces/{space_id}/folders/{folder_id}",
            get(get_folder).put(update_folder).delete(delete_folder),
        )
        .route(
            "/spaces/{space_id}/folders/{folder_id}/move",
            post(move_folder),
        )
        .route(
            "/spaces/{space_id}/folders/{folder_id}/reorder",
            axum::routing::put(reorder_folder),
        )
}

// ============================================================
// Types
// ============================================================

#[derive(Deserialize)]
pub struct CreateFolderRequest {
    pub name: String,
    pub parent_id: Option<String>,
    pub icon: Option<String>,
    pub source_id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateFolderRequest {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub parent_id: Option<String>, // Move folder to new parent
}

#[derive(Deserialize)]
pub struct ReorderRequest {
    pub sort_order: i32,
}

#[derive(Deserialize)]
pub struct MoveFolderRequest {
    pub target_space_id: String,
    pub parent_id: Option<String>,
}

#[derive(Serialize)]
pub struct FolderResponse {
    pub id: String,
    pub space_id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub icon: Option<String>,
    pub sort_order: i32,
    pub created_at: String,
    pub updated_at: String,
}

async fn folder_parent_id(
    db: &AnyPool,
    space_id: &str,
    folder_id: &str,
) -> Result<Option<String>, AppError> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT parent_id FROM folders WHERE id = ?1 AND space_id = ?2")
            .bind(folder_id)
            .bind(space_id)
            .fetch_optional(db)
            .await
            .map_err(AppError::internal)?;

    row.map(|(parent_id,)| parent_id)
        .ok_or_else(|| AppError::BadRequest("Folder not found".into()))
}

async fn folder_depth(db: &AnyPool, space_id: &str, folder_id: &str) -> Result<i32, AppError> {
    let mut depth = 1;
    let mut current_id = folder_id.to_string();
    let mut seen = std::collections::HashSet::new();

    loop {
        if !seen.insert(current_id.clone()) {
            return Err(AppError::BadRequest(
                "Folder hierarchy contains a cycle".into(),
            ));
        }

        match folder_parent_id(db, space_id, &current_id).await? {
            Some(parent_id) => {
                depth += 1;
                current_id = parent_id;
            }
            None => return Ok(depth),
        }
    }
}

pub(crate) async fn folder_subtree_depth(
    db: &AnyPool,
    space_id: &str,
    folder_id: &str,
) -> Result<i32, AppError> {
    let mut max_depth = 1;
    let mut stack = vec![(folder_id.to_string(), 1)];
    let mut seen = std::collections::HashSet::new();

    while let Some((current_id, depth)) = stack.pop() {
        if !seen.insert(current_id.clone()) {
            return Err(AppError::BadRequest(
                "Folder hierarchy contains a cycle".into(),
            ));
        }
        max_depth = max_depth.max(depth);

        let children: Vec<(String,)> =
            sqlx::query_as("SELECT id FROM folders WHERE space_id = ?1 AND parent_id = ?2")
                .bind(space_id)
                .bind(&current_id)
                .fetch_all(db)
                .await
                .map_err(AppError::internal)?;

        for (child_id,) in children {
            stack.push((child_id, depth + 1));
        }
    }

    Ok(max_depth)
}

async fn collect_folder_subtree_ids(
    db: &AnyPool,
    space_id: &str,
    folder_id: &str,
) -> Result<Vec<String>, AppError> {
    folder_parent_id(db, space_id, folder_id).await?;
    let mut ids = Vec::new();
    let mut stack = vec![folder_id.to_string()];
    let mut seen = std::collections::HashSet::new();

    while let Some(current_id) = stack.pop() {
        if !seen.insert(current_id.clone()) {
            return Err(AppError::BadRequest(
                "Folder hierarchy contains a cycle".into(),
            ));
        }
        ids.push(current_id.clone());

        let children: Vec<(String,)> =
            sqlx::query_as("SELECT id FROM folders WHERE space_id = ?1 AND parent_id = ?2")
                .bind(space_id)
                .bind(&current_id)
                .fetch_all(db)
                .await
                .map_err(AppError::internal)?;

        for (child_id,) in children {
            stack.push(child_id);
        }
    }

    Ok(ids)
}

pub(crate) async fn document_ids_in_folders(
    db: &AnyPool,
    space_id: &str,
    folder_ids: &[String],
) -> Result<Vec<String>, AppError> {
    let mut ids = Vec::new();
    for folder_id in folder_ids {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT id FROM documents WHERE space_id = ?1 AND folder_id = ?2")
                .bind(space_id)
                .bind(folder_id)
                .fetch_all(db)
                .await
                .map_err(AppError::internal)?;
        ids.extend(rows.into_iter().map(|(id,)| id));
    }
    Ok(ids)
}

async fn validate_folder_move_destination(
    db: &AnyPool,
    source_space_id: &str,
    target_space_id: &str,
    folder_id: &str,
    parent_id: Option<&str>,
) -> Result<(), AppError> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };

    if source_space_id == target_space_id {
        return validate_moved_folder_parent(db, source_space_id, folder_id, parent_id).await;
    }

    let parent_depth = folder_depth(db, target_space_id, parent_id).await?;
    let subtree_depth = folder_subtree_depth(db, source_space_id, folder_id).await?;
    if parent_depth + subtree_depth > MAX_FOLDER_DEPTH {
        return Err(AppError::BadRequest(format!(
            "Folders can be nested up to {MAX_FOLDER_DEPTH} levels"
        )));
    }
    Ok(())
}

pub(crate) async fn validate_document_folder_depth(
    db: &AnyPool,
    space_id: &str,
    folder_id: Option<&str>,
) -> Result<(), AppError> {
    let Some(folder_id) = folder_id else {
        return Ok(());
    };
    if folder_depth(db, space_id, folder_id).await? > MAX_FOLDER_DEPTH {
        return Err(AppError::BadRequest(format!(
            "Pages can be nested up to {MAX_FOLDER_DEPTH} folder levels"
        )));
    }
    Ok(())
}

async fn validate_new_folder_parent(
    db: &AnyPool,
    space_id: &str,
    parent_id: Option<&str>,
) -> Result<(), AppError> {
    let Some(parent_id) = parent_id else {
        return Ok(());
    };
    let parent_depth = folder_depth(db, space_id, parent_id).await?;
    if parent_depth + 1 > MAX_FOLDER_DEPTH {
        return Err(AppError::BadRequest(format!(
            "Folders can be nested up to {MAX_FOLDER_DEPTH} levels"
        )));
    }
    Ok(())
}

async fn validate_moved_folder_parent(
    db: &AnyPool,
    space_id: &str,
    folder_id: &str,
    parent_id: &str,
) -> Result<(), AppError> {
    if folder_id == parent_id {
        return Err(AppError::BadRequest(
            "Folder cannot be moved inside itself".into(),
        ));
    }

    let mut current_parent = Some(parent_id.to_string());
    let mut seen = std::collections::HashSet::new();
    while let Some(current_id) = current_parent {
        if current_id == folder_id {
            return Err(AppError::BadRequest(
                "Folder cannot be moved inside one of its descendants".into(),
            ));
        }
        if !seen.insert(current_id.clone()) {
            return Err(AppError::BadRequest(
                "Folder hierarchy contains a cycle".into(),
            ));
        }
        current_parent = folder_parent_id(db, space_id, &current_id).await?;
    }

    let parent_depth = folder_depth(db, space_id, parent_id).await?;
    let subtree_depth = folder_subtree_depth(db, space_id, folder_id).await?;
    if parent_depth + subtree_depth > MAX_FOLDER_DEPTH {
        return Err(AppError::BadRequest(format!(
            "Folders can be nested up to {MAX_FOLDER_DEPTH} levels"
        )));
    }
    Ok(())
}

// ============================================================
// Handlers
// ============================================================

/// GET /api/v1/spaces/{space_id}/folders
async fn list_folders(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let folders: Vec<(
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        i32,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, space_id, parent_id, name, icon, sort_order, created_at, updated_at \
             FROM folders WHERE space_id = ?1 ORDER BY sort_order, name",
    )
    .bind(&space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<FolderResponse> = folders
        .into_iter()
        .map(
            |(id, sid, pid, name, icon, sort, created, updated)| FolderResponse {
                id,
                space_id: sid,
                parent_id: pid,
                name,
                icon,
                sort_order: sort,
                created_at: created,
                updated_at: updated,
            },
        )
        .collect();

    Ok(Json(result))
}

/// POST /api/v1/spaces/{space_id}/folders
async fn create_folder(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<CreateFolderRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    if req.name.is_empty() {
        return Err(AppError::BadRequest("Folder name is required".into()));
    }

    if req.name.len() > 100 {
        return Err(AppError::BadRequest(
            "Folder name must be at most 100 characters".into(),
        ));
    }

    let folder_id = super::routes::import_id_or_uuid(req.source_id.as_deref())?;
    let created_at = super::routes::import_timestamp_or_now(req.created_at.as_deref())?;
    let updated_at =
        super::routes::import_timestamp_or_default(req.updated_at.as_deref(), &created_at)?;

    let existing_id: Option<(String,)> = sqlx::query_as("SELECT id FROM folders WHERE id = ?1")
        .bind(&folder_id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;
    if existing_id.is_some() {
        return Err(AppError::Conflict(
            "A folder with this import id already exists".into(),
        ));
    }

    validate_new_folder_parent(&state.db, &space_id, req.parent_id.as_deref()).await?;

    // Get next sort order
    let max_sort: (Option<i32>,) = sqlx::query_as(
        "SELECT MAX(sort_order) FROM folders WHERE space_id = ?1 AND parent_id IS ?2",
    )
    .bind(&space_id)
    .bind(&req.parent_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::internal)?;

    let sort_order = max_sort.0.unwrap_or(0) + 1;

    sqlx::query(
        "INSERT INTO folders (id, space_id, parent_id, name, icon, sort_order, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&folder_id)
    .bind(&space_id)
    .bind(&req.parent_id)
    .bind(&req.name)
    .bind(&req.icon)
    .bind(sort_order)
    .bind(&created_at)
    .bind(&updated_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "folder.create",
        Some("folder"),
        Some(&folder_id),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(FolderResponse {
            id: folder_id,
            space_id,
            parent_id: req.parent_id,
            name: req.name,
            icon: req.icon,
            sort_order,
            created_at,
            updated_at,
        }),
    ))
}

/// GET /api/v1/spaces/{space_id}/folders/{folder_id}
async fn get_folder(
    State(state): State<AppState>,
    _auth: AuthUser,
    Path((_space_id, folder_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let folder: Option<(
        String,
        String,
        Option<String>,
        String,
        Option<String>,
        i32,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, space_id, parent_id, name, icon, sort_order, created_at, updated_at \
             FROM folders WHERE id = ?1",
    )
    .bind(&folder_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    let (id, sid, pid, name, icon, sort, created, updated) =
        folder.ok_or_else(|| AppError::BadRequest("Folder not found".into()))?;

    Ok(Json(FolderResponse {
        id,
        space_id: sid,
        parent_id: pid,
        name,
        icon,
        sort_order: sort,
        created_at: created,
        updated_at: updated,
    }))
}

/// PUT /api/v1/spaces/{space_id}/folders/{folder_id}
async fn update_folder(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, folder_id)): Path<(String, String)>,
    Json(req): Json<UpdateFolderRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    if let Some(ref name) = req.name {
        if name.len() > 100 {
            return Err(AppError::BadRequest(
                "Folder name must be at most 100 characters".into(),
            ));
        }
    }

    let updated_at = chrono::Utc::now().to_rfc3339();

    if let Some(ref name) = req.name {
        sqlx::query("UPDATE folders SET name = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(name)
            .bind(&updated_at)
            .bind(&folder_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    if let Some(ref icon) = req.icon {
        sqlx::query("UPDATE folders SET icon = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(icon)
            .bind(&updated_at)
            .bind(&folder_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    if let Some(ref parent_id) = req.parent_id {
        validate_moved_folder_parent(&state.db, &space_id, &folder_id, parent_id).await?;
        sqlx::query("UPDATE folders SET parent_id = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(parent_id)
            .bind(&updated_at)
            .bind(&folder_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "folder.update",
        Some("folder"),
        Some(&folder_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/spaces/{space_id}/folders/{folder_id}/move
async fn move_folder(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, folder_id)): Path<(String, String)>,
    Json(req): Json<MoveFolderRequest>,
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

    validate_folder_move_destination(
        &state.db,
        &space_id,
        &req.target_space_id,
        &folder_id,
        req.parent_id.as_deref(),
    )
    .await?;

    let folder_ids = collect_folder_subtree_ids(&state.db, &space_id, &folder_id).await?;
    let document_ids = document_ids_in_folders(&state.db, &space_id, &folder_ids).await?;
    let updated_at = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "UPDATE folders SET space_id = ?1, updated_at = ?2 WHERE space_id = ?3 AND id = ?4",
    )
    .bind(&req.target_space_id)
    .bind(&updated_at)
    .bind(&space_id)
    .bind(&folder_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    sqlx::query("UPDATE folders SET parent_id = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(&req.parent_id)
        .bind(&updated_at)
        .bind(&folder_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    for child_folder_id in folder_ids.iter().filter(|id| *id != &folder_id) {
        sqlx::query("UPDATE folders SET space_id = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(&req.target_space_id)
            .bind(&updated_at)
            .bind(child_folder_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    for doc_id in &document_ids {
        sqlx::query(
            "UPDATE documents SET space_id = ?1, last_updated_by = ?2, updated_at = ?3 \
             WHERE id = ?4",
        )
        .bind(&req.target_space_id)
        .bind(&auth.claims.sub)
        .bind(&updated_at)
        .bind(doc_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    }

    if req.target_space_id != space_id {
        super::documents::cleanup_moved_document_metadata(
            &state.db,
            &req.target_space_id,
            &document_ids,
        )
        .await?;
        super::documents::update_moved_document_space_records(
            &state.db,
            &req.target_space_id,
            &document_ids,
        )
        .await?;
        super::documents::reindex_documents(&state, &document_ids).await;
    }

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "folder.move",
        Some("folder"),
        Some(&folder_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::{
        validate_document_folder_depth, validate_moved_folder_parent, validate_new_folder_parent,
    };
    use sqlx::any::AnyPoolOptions;
    use sqlx::AnyPool;

    async fn hierarchy_pool() -> AnyPool {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");

        sqlx::query(
            "CREATE TABLE folders (
                id TEXT PRIMARY KEY NOT NULL,
                space_id TEXT NOT NULL,
                parent_id TEXT,
                name TEXT NOT NULL,
                icon TEXT,
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&db)
        .await
        .expect("create folders table");

        for (id, parent_id) in [
            ("root", None),
            ("child", Some("root")),
            ("grandchild", Some("child")),
            ("root-b", None),
            ("branch-parent", Some("root-b")),
            ("branch-child", Some("branch-parent")),
        ] {
            sqlx::query(
                "INSERT INTO folders (id, space_id, parent_id, name, created_at, updated_at)
                 VALUES (?1, 'space-1', ?2, ?1, '2026-07-10T00:00:00Z', '2026-07-10T00:00:00Z')",
            )
            .bind(id)
            .bind(parent_id)
            .execute(&db)
            .await
            .expect("insert folder");
        }

        db
    }

    #[tokio::test]
    async fn validates_new_folder_depth_limit() {
        let db = hierarchy_pool().await;

        validate_new_folder_parent(&db, "space-1", None)
            .await
            .expect("root folder is allowed");
        validate_new_folder_parent(&db, "space-1", Some("root"))
            .await
            .expect("second-level folder is allowed");
        validate_new_folder_parent(&db, "space-1", Some("child"))
            .await
            .expect("third-level folder is allowed");

        let error = validate_new_folder_parent(&db, "space-1", Some("grandchild"))
            .await
            .expect_err("fourth-level folder is blocked");
        assert!(format!("{error:?}").contains("Folders can be nested up to 3 levels"));
    }

    #[tokio::test]
    async fn validates_folder_move_depth_and_cycles() {
        let db = hierarchy_pool().await;

        validate_moved_folder_parent(&db, "space-1", "branch-child", "root")
            .await
            .expect("moving a leaf under a root stays within the cap");

        let depth_error = validate_moved_folder_parent(&db, "space-1", "branch-parent", "child")
            .await
            .expect_err("moving a subtree under level two would exceed the cap");
        assert!(format!("{depth_error:?}").contains("Folders can be nested up to 3 levels"));

        let cycle_error = validate_moved_folder_parent(&db, "space-1", "root", "grandchild")
            .await
            .expect_err("moving into a descendant is blocked");
        assert!(format!("{cycle_error:?}")
            .contains("Folder cannot be moved inside one of its descendants"));
    }

    #[tokio::test]
    async fn validates_document_folder_depth_limit() {
        let db = hierarchy_pool().await;

        validate_document_folder_depth(&db, "space-1", Some("grandchild"))
            .await
            .expect("pages can live at the third folder level");

        sqlx::query(
            "INSERT INTO folders (id, space_id, parent_id, name, created_at, updated_at)
             VALUES ('too-deep', 'space-1', 'grandchild', 'too-deep', '2026-07-10T00:00:00Z', '2026-07-10T00:00:00Z')",
        )
        .execute(&db)
        .await
        .expect("insert corrupted deep folder");

        let error = validate_document_folder_depth(&db, "space-1", Some("too-deep"))
            .await
            .expect_err("pages cannot be placed beyond the third folder level");
        assert!(format!("{error:?}").contains("Pages can be nested up to 3 folder levels"));
    }
}

/// DELETE /api/v1/spaces/{space_id}/folders/{folder_id}
async fn delete_folder(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, folder_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    // CASCADE will handle child folders and documents
    sqlx::query("DELETE FROM folders WHERE id = ?1")
        .bind(&folder_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "folder.delete",
        Some("folder"),
        Some(&folder_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// PUT /api/v1/spaces/{space_id}/folders/{folder_id}/reorder
async fn reorder_folder(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, folder_id)): Path<(String, String)>,
    Json(req): Json<ReorderRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    let updated_at = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE folders SET sort_order = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(req.sort_order)
        .bind(&updated_at)
        .bind(&folder_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(StatusCode::NO_CONTENT)
}
