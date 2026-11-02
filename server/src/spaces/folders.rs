use super::access;
use super::mutation_guards::{lock_spaces_for_mutation, require_current_page_edit};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{AnyConnection, AnyPool};

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::AppState;

/// Resource admission, not a user-visible nesting limit. Count unique folders in
/// the participating ancestor chains and subtree before changing topology.
pub(crate) const MAX_FOLDER_OPERATION_NODES: usize = 10_000;

type FolderNodes = std::collections::HashSet<String>;

fn admit_folder_node(nodes: &mut FolderNodes, id: &str) -> Result<(), AppError> {
    nodes.insert(id.to_owned());
    if nodes.len() > MAX_FOLDER_OPERATION_NODES {
        return Err(AppError::BadRequest(format!(
            "Folder operation exceeds the {MAX_FOLDER_OPERATION_NODES}-node traversal budget"
        )));
    }
    Ok(())
}

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

async fn validate_folder_move_destination(
    db: &AnyPool,
    source_space_id: &str,
    target_space_id: &str,
    folder_id: &str,
    parent_id: Option<&str>,
) -> Result<(), AppError> {
    let mut conn = db.acquire().await.map_err(AppError::internal)?;
    validate_folder_move_destination_on_connection(
        &mut conn,
        source_space_id,
        target_space_id,
        folder_id,
        parent_id,
    )
    .await
}

pub(crate) async fn validate_document_folder_depth(
    db: &AnyPool,
    space_id: &str,
    folder_id: Option<&str>,
) -> Result<(), AppError> {
    let mut conn = db.acquire().await.map_err(AppError::internal)?;
    validate_document_folder_depth_on_connection(&mut conn, space_id, folder_id).await
}

async fn validate_new_folder_parent(
    db: &AnyPool,
    space_id: &str,
    parent_id: Option<&str>,
) -> Result<(), AppError> {
    let mut conn = db.acquire().await.map_err(AppError::internal)?;
    validate_new_folder_parent_on_connection(&mut conn, space_id, parent_id).await
}

async fn validate_moved_folder_parent(
    db: &AnyPool,
    space_id: &str,
    folder_id: &str,
    parent_id: &str,
) -> Result<(), AppError> {
    let mut conn = db.acquire().await.map_err(AppError::internal)?;
    validate_moved_folder_parent_on_connection(&mut conn, space_id, folder_id, parent_id).await
}

async fn folder_parent_id_on_connection(
    conn: &mut AnyConnection,
    space_id: &str,
    folder_id: &str,
) -> Result<Option<String>, AppError> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT parent_id FROM folders WHERE id = $1 AND space_id = $2")
            .bind(folder_id)
            .bind(space_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;

    row.map(|(parent_id,)| parent_id)
        .ok_or_else(|| AppError::BadRequest("Folder not found".into()))
}

async fn folder_ancestry_on_connection(
    conn: &mut AnyConnection,
    space_id: &str,
    folder_id: &str,
    participating: &mut FolderNodes,
    excluded: Option<&str>,
) -> Result<i32, AppError> {
    let mut current = Some(folder_id.to_owned());
    let mut seen = FolderNodes::new();
    while let Some(id) = current {
        if excluded == Some(id.as_str()) {
            return Err(AppError::BadRequest(
                "Folder cannot be moved inside one of its descendants".into(),
            ));
        }
        if !seen.insert(id.clone()) {
            return Err(AppError::BadRequest("Folder hierarchy contains a cycle".into()));
        }
        admit_folder_node(participating, &id)?;
        current = folder_parent_id_on_connection(conn, space_id, &id).await?;
    }
    Ok(seen.len() as i32)
}

async fn folder_depth_on_connection(
    conn: &mut AnyConnection,
    space_id: &str,
    folder_id: &str,
) -> Result<i32, AppError> {
    folder_ancestry_on_connection(conn, space_id, folder_id, &mut FolderNodes::new(), None).await
}

/// Each indexed child lookup is bounded too: a broad parent cannot materialize
/// an unbounded result before the node admission check. Foreign children remain
/// visible to validation instead of being silently filtered out.
async fn folder_subtree_on_connection(
    conn: &mut AnyConnection,
    space_id: &str,
    folder_id: &str,
    participating: &mut FolderNodes,
) -> Result<Vec<(String, Option<String>)>, AppError> {
    let parent = folder_parent_id_on_connection(conn, space_id, folder_id).await?;
    admit_folder_node(participating, folder_id)?;
    let mut queue = std::collections::VecDeque::from([(folder_id.to_owned(), parent)]);
    let mut seen = FolderNodes::from([folder_id.to_owned()]);
    let mut rows = Vec::new();
    while let Some((id, parent)) = queue.pop_front() {
        let limit = MAX_FOLDER_OPERATION_NODES.saturating_sub(participating.len()) + 1;
        let children: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, space_id FROM folders WHERE parent_id = $1 ORDER BY id LIMIT $2",
        ).bind(&id).bind(limit as i64).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
        for (child, space) in children {
            if space != space_id {
                return Err(AppError::Conflict("Folder hierarchy has an inconsistent space association".into()));
            }
            if !seen.insert(child.clone()) {
                return Err(AppError::BadRequest("Folder hierarchy contains a cycle".into()));
            }
            admit_folder_node(participating, &child)?;
            queue.push_back((child, Some(id.clone())));
        }
        rows.push((id, parent));
    }
    Ok(rows)
}

async fn collect_folder_subtree_ids_on_connection(
    conn: &mut AnyConnection,
    space_id: &str,
    folder_id: &str,
) -> Result<Vec<String>, AppError> {
    let mut nodes = FolderNodes::new();
    folder_ancestry_on_connection(conn, space_id, folder_id, &mut nodes, None).await?;
    Ok(folder_subtree_on_connection(conn, space_id, folder_id, &mut nodes).await?
        .into_iter().map(|(id, _)| id).collect())
}

async fn validate_folder_move_destination_on_connection(
    conn: &mut AnyConnection,
    source_space_id: &str,
    target_space_id: &str,
    folder_id: &str,
    parent_id: Option<&str>,
) -> Result<(), AppError> {
    if source_space_id == target_space_id && parent_id == Some(folder_id) {
        return Err(AppError::BadRequest("Folder cannot be moved inside itself".into()));
    }
    let mut nodes = FolderNodes::new();
    folder_ancestry_on_connection(conn, source_space_id, folder_id, &mut nodes, None).await?;
    if let Some(parent) = parent_id {
        folder_ancestry_on_connection(conn, target_space_id, parent, &mut nodes,
            (source_space_id == target_space_id).then_some(folder_id)).await?;
    }
    folder_subtree_on_connection(conn, source_space_id, folder_id, &mut nodes).await?;
    Ok(())
}

/// Historical name retained for callers; only folder graph validity/resource
/// admission is checked here. Child-page parent_id policy has its own owner.
pub(crate) async fn validate_document_folder_depth_on_connection(
    conn: &mut AnyConnection,
    space_id: &str,
    folder_id: Option<&str>,
) -> Result<(), AppError> {
    if let Some(id) = folder_id { folder_depth_on_connection(conn, space_id, id).await?; }
    Ok(())
}

async fn validate_new_folder_parent_on_connection(
    conn: &mut AnyConnection,
    space_id: &str,
    parent_id: Option<&str>,
) -> Result<(), AppError> {
    if let Some(parent) = parent_id {
        let depth = folder_depth_on_connection(conn, space_id, parent).await? as usize;
        if depth >= MAX_FOLDER_OPERATION_NODES {
            return Err(AppError::BadRequest(format!(
                "Folder operation exceeds the {MAX_FOLDER_OPERATION_NODES}-node traversal budget"
            )));
        }
    }
    Ok(())
}

async fn validate_moved_folder_parent_on_connection(
    conn: &mut AnyConnection,
    space_id: &str,
    folder_id: &str,
    parent_id: &str,
) -> Result<(), AppError> {
    validate_folder_move_destination_on_connection(conn, space_id, space_id, folder_id, Some(parent_id)).await
}

/// Exact folder topology operation, including current credential ceiling. No page View grant is implied.
async fn require_folder_space_mutation(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
) -> Result<(), AppError> {
    let current = access::resolve_space_access(
        conn,
        access::principal_from_auth(auth)?,
        space,
        access::Purpose::Routine,
    )
    .await?;
    if !current.allows(access::Permission::ManageFolders) {
        return Err(AppError::NotFound("Folder unavailable".into()));
    }
    Ok(())
}

async fn require_folder_mutation_on_connection(
    conn: &mut AnyConnection,
    space_id: &str,
    folder_id: &str,
    auth: &AuthUser,
) -> Result<(), AppError> {
    let locked = sqlx::query("UPDATE folders SET id = id WHERE id = $1 AND space_id = $2")
        .bind(folder_id)
        .bind(space_id)
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if locked.rows_affected() != 1 {
        return Err(AppError::NotFound("Folder not found".into()));
    }
    match require_folder_space_mutation(conn, auth, space_id).await {
        Ok(_) => Ok(()),
        Err(AppError::Forbidden(_)) | Err(AppError::NotFound(_)) => {
            Err(AppError::NotFound("Folder not found".into()))
        }
        Err(error) => Err(error),
    }
}

async fn validate_folder_structure_on_connection(
    conn: &mut AnyConnection,
    space_id: &str,
    folder_id: &str,
) -> Result<(), AppError> {
    let mut nodes = FolderNodes::new();
    folder_ancestry_on_connection(conn, space_id, folder_id, &mut nodes, None).await?;
    folder_subtree_on_connection(conn, space_id, folder_id, &mut nodes).await?;
    Ok(())
}

async fn require_folder_parent_identity_on_connection(
    conn: &mut AnyConnection,
    space_id: &str,
    folder_id: &str,
    expected_parent_id: Option<&str>,
) -> Result<(), AppError> {
    let actual_parent_id = folder_parent_id_on_connection(conn, space_id, folder_id).await?;
    if actual_parent_id.as_deref() != expected_parent_id {
        return Err(AppError::Conflict(
            "Folder placement changed; retry mutation".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, PartialEq, Eq)]
struct FolderMutationInventory {
    folders: Vec<(String, Option<String>)>,
    documents: Vec<(String, String)>,
}

async fn folder_mutation_inventory(
    conn: &mut AnyConnection,
    space_id: &str,
    folder_id: &str,
) -> Result<FolderMutationInventory, AppError> {
    validate_folder_structure_on_connection(conn, space_id, folder_id).await?;
    let mut folder_ids =
        collect_folder_subtree_ids_on_connection(conn, space_id, folder_id).await?;
    folder_ids.sort();
    let mut folders = Vec::new();
    let mut documents = Vec::new();
    for id in folder_ids {
        let parent = folder_parent_id_on_connection(conn, space_id, &id).await?;
        folders.push((id.clone(), parent));
        let refs: Vec<(String, String)> =
            sqlx::query_as("SELECT id, space_id FROM documents WHERE folder_id = $1 ORDER BY id")
                .bind(&id)
                .fetch_all(&mut *conn)
                .await
                .map_err(AppError::internal)?;
        for (document_id, document_space) in refs {
            if document_space != space_id {
                return Err(AppError::Conflict(
                    "Folder has an inconsistent page space association".into(),
                ));
            }
            documents.push((document_id, id.clone()));
        }
    }
    documents.sort();
    Ok(FolderMutationInventory { folders, documents })
}

async fn require_folder_inventory_mutation(
    conn: &mut AnyConnection,
    space_id: &str,
    inventory: &FolderMutationInventory,
    auth: &AuthUser,
) -> Result<(), AppError> {
    for (folder_id, _) in &inventory.folders {
        require_folder_mutation_on_connection(conn, space_id, folder_id, auth).await?;
    }
    for (document_id, _) in &inventory.documents {
        require_current_page_edit(
            conn,
            access::principal_from_auth(auth)?,
            space_id,
            document_id,
        )
        .await?;
    }
    require_folder_space_mutation(conn, auth, space_id).await?;
    Ok(())
}

async fn require_same_folder_inventory(
    conn: &mut AnyConnection,
    space_id: &str,
    folder_id: &str,
    expected: &FolderMutationInventory,
) -> Result<(), AppError> {
    if folder_mutation_inventory(conn, space_id, folder_id).await? != *expected {
        return Err(AppError::Conflict(
            "Folder subtree placement changed; retry mutation".into(),
        ));
    }
    Ok(())
}

// ============================================================
// Handlers
// ============================================================

/// GET /api/v1/spaces/{space_id}/folders
async fn require_catalog_folder_chain(
    conn: &mut AnyConnection,
    space: &str,
    id: &str,
    expected_parent: &Option<String>,
) -> Result<(), AppError> {
    let current: Option<(Option<String>,)> =
        sqlx::query_as("SELECT parent_id FROM folders WHERE id=$1 AND space_id=$2")
            .bind(id)
            .bind(space)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    if current.as_ref().map(|row| &row.0) != Some(expected_parent) {
        return Err(AppError::NotFound("Folder unavailable".into()));
    }
    match folder_depth_on_connection(conn, space, id).await {
        Ok(_) => Ok(()),
        Err(AppError::BadRequest(_)) | Err(AppError::Conflict(_)) | Err(AppError::NotFound(_)) => {
            Err(AppError::NotFound("Folder unavailable".into()))
        }
        Err(error) => Err(error),
    }
}
/// Catalogs may contain arbitrarily many unrelated roots. Validate each
/// participating ancestry once; the operation budget applies to a chain, not
/// the total size of a user's workspace.
fn validate_folder_catalog(rows: &[(String, Option<String>)]) -> Result<(), AppError> {
    let parents: std::collections::HashMap<&str, Option<&str>> = rows.iter()
        .map(|(id, parent)| (id.as_str(), parent.as_deref())).collect();
    let mut depths: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for id in parents.keys().copied() {
        if depths.contains_key(id) { continue; }
        let mut path = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut current = Some(id);
        let mut depth = 0;
        while let Some(node) = current {
            if let Some(known) = depths.get(node) { depth = *known; break; }
            if !seen.insert(node) || path.len() >= MAX_FOLDER_OPERATION_NODES {
                return Err(AppError::NotFound("Folder unavailable".into()));
            }
            let parent = parents.get(node).ok_or_else(|| AppError::NotFound("Folder unavailable".into()))?;
            path.push(node);
            current = *parent;
        }
        for node in path.into_iter().rev() {
            depth += 1;
            if depth > MAX_FOLDER_OPERATION_NODES {
                return Err(AppError::NotFound("Folder unavailable".into()));
            }
            depths.insert(node, depth);
        }
    }
    Ok(())
}

async fn list_folders(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    super::routes::require_catalog_space_view(&mut conn, &auth, &space_id).await?;
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
             FROM folders WHERE space_id = $1 ORDER BY sort_order, name",
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
    // Load actual current parent state after any DB wait/hook. Checking an
    // indexed map avoids requerying every ancestor for every catalog row.
    let current: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT id, parent_id FROM folders WHERE space_id=$1",
    ).bind(&space_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let parents: std::collections::HashMap<_, _> = current.iter()
        .map(|(id, parent)| (id.as_str(), parent)).collect();
    for row in &folders {
        if parents.get(row.0.as_str()).copied() != Some(&row.2) {
            return Err(AppError::NotFound("Folder unavailable".into()));
        }
    }
    validate_folder_catalog(&current)?;
    super::routes::require_catalog_space_view(&mut conn, &auth, &space_id).await?;
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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FolderMutationOrigin {
    Rest,
    Replay,
}
#[derive(Clone)]
pub(crate) struct FolderReplayMetadata {
    pub id: String,
    pub sort_order: Option<i32>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}
fn folder_same_timestamp(a: &str, b: &str) -> bool {
    chrono::DateTime::parse_from_rfc3339(a)
        .ok()
        .zip(chrono::DateTime::parse_from_rfc3339(b).ok())
        .is_some_and(|(a, b)| a == b)
}

/// POST /api/v1/spaces/{space_id}/folders
async fn create_folder(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<CreateFolderRequest>,
) -> Result<impl IntoResponse, AppError> {
    let (folder, _) = mutate_folder_create(&state, &auth, space_id, req, None).await?;
    Ok((StatusCode::CREATED, Json(folder)))
}
pub(crate) async fn mutate_folder_create(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    req: CreateFolderRequest,
    replay: Option<FolderReplayMetadata>,
) -> Result<(FolderResponse, bool), AppError> {
    let user_id = auth.claims.sub.as_str();
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_folder_space_mutation(&mut tx, auth, &space_id).await?;

    if req.name.is_empty() {
        return Err(AppError::BadRequest("Folder name is required".into()));
    }

    if req.name.len() > 100 {
        return Err(AppError::BadRequest(
            "Folder name must be at most 100 characters".into(),
        ));
    }

    let folder_id = match &replay {
        Some(import) => import.id.clone(),
        None => super::routes::import_id_or_uuid(req.source_id.as_deref())?,
    };
    let created_at = super::routes::import_timestamp_or_now(req.created_at.as_deref())?;
    let updated_at =
        super::routes::import_timestamp_or_default(req.updated_at.as_deref(), &created_at)?;

    let existing_id: Option<(String,)> =
        sqlx::query_as("SELECT space_id FROM folders WHERE id = $1")
            .bind(&folder_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    if let Some((scope,)) = existing_id {
        if let Some(import) = &replay {
            if scope != space_id {
                return Err(AppError::Conflict(
                    "Folder import identity conflicts with canonical metadata".into(),
                ));
            }
            // The scoped tuple lock can wait; compare only the canonical metadata
            // observed after that wait and its current actor check.
            require_folder_mutation_on_connection(&mut tx, &space_id, &folder_id, auth).await?;
            let (parent,name,icon,sort,created,updated): (Option<String>,String,Option<String>,i32,String,String) = sqlx::query_as("SELECT parent_id,name,icon,sort_order,created_at,updated_at FROM folders WHERE id=$1 AND space_id=$2")
                .bind(&folder_id).bind(&space_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?
                .ok_or_else(|| AppError::NotFound("Folder not found".into()))?;
            if parent == req.parent_id
                && name == req.name
                && icon == req.icon
                && import.sort_order.is_none_or(|value| value == sort)
                && import
                    .created_at
                    .as_deref()
                    .is_none_or(|value| folder_same_timestamp(value, &created))
                && import
                    .updated_at
                    .as_deref()
                    .is_none_or(|value| folder_same_timestamp(value, &updated))
            {
                validate_folder_structure_on_connection(&mut tx, &space_id, &folder_id).await?;
                require_folder_space_mutation(&mut tx, auth, &space_id).await?;
                access::require_current_human_preference_write(&mut tx, auth).await?;
                tx.commit().await.map_err(AppError::internal)?;
                return Ok((
                    FolderResponse {
                        id: folder_id,
                        space_id,
                        parent_id: parent,
                        name,
                        icon,
                        sort_order: sort,
                        created_at: created,
                        updated_at: updated,
                    },
                    true,
                ));
            }
        }
        return Err(AppError::Conflict(
            "Folder import identity conflicts with canonical metadata".into(),
        ));
    }

    validate_new_folder_parent_on_connection(&mut tx, &space_id, req.parent_id.as_deref()).await?;

    // Get next sort order
    let max_sort: (Option<i32>,) = sqlx::query_as(
        "SELECT MAX(sort_order) FROM folders WHERE space_id = $1 AND (parent_id = $2 OR (parent_id IS NULL AND $2 IS NULL))",
    )
    .bind(&space_id)
    .bind(&req.parent_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::internal)?;

    let sort_order = match replay.as_ref().and_then(|import| import.sort_order) {
        Some(sort) => sort,
        None => max_sort
            .0
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| AppError::Conflict("Folder sort order is exhausted".into()))?,
    };

    let inserted = sqlx::query(
        "INSERT INTO folders (id, space_id, parent_id, name, icon, sort_order, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(&folder_id)
    .bind(&space_id)
    .bind(&req.parent_id)
    .bind(&req.name)
    .bind(&req.icon)
    .bind(sort_order)
    .bind(&created_at)
    .bind(&updated_at)
    .execute(&mut *tx)
    .await
    .map_err(AppError::internal)?;

    if inserted.rows_affected() != 1 {
        return Err(AppError::Conflict("Folder creation did not persist".into()));
    }
    require_folder_mutation_on_connection(&mut tx, &space_id, &folder_id, auth).await?;
    validate_folder_structure_on_connection(&mut tx, &space_id, &folder_id).await?;
    require_folder_parent_identity_on_connection(
        &mut tx,
        &space_id,
        &folder_id,
        req.parent_id.as_deref(),
    )
    .await?;
    require_folder_space_mutation(&mut tx, auth, &space_id).await?;
    access::require_current_human_preference_write(&mut tx, auth).await?;
    tx.commit().await.map_err(AppError::internal)?;
    if replay.is_none() {
        crate::audit::log_action(
            &state.db,
            Some(user_id),
            None,
            "folder.create",
            Some("folder"),
            Some(&folder_id),
        )
        .await;
    }
    Ok((
        FolderResponse {
            id: folder_id,
            space_id,
            parent_id: req.parent_id,
            name: req.name,
            icon: req.icon,
            sort_order,
            created_at,
            updated_at,
        },
        false,
    ))
}

/// GET /api/v1/spaces/{space_id}/folders/{folder_id}
async fn get_folder(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, folder_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    super::routes::require_catalog_space_view(&mut conn, &auth, &space_id).await?;
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
             FROM folders WHERE id = $1 AND space_id = $2",
    )
    .bind(&folder_id)
    .bind(&space_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    let (id, sid, pid, name, icon, sort, created, updated) =
        folder.ok_or_else(|| AppError::NotFound("Folder not found".into()))?;

    #[cfg(test)]
    super::routes::catalog_after_load().await;
    super::routes::require_catalog_space_view(&mut conn, &auth, &space_id).await?;
    #[cfg(test)]
    super::routes::catalog_after_space_view().await;
    require_catalog_folder_chain(&mut conn, &space_id, &id, &pid).await?;
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
    mutate_folder_update(&state, &auth, space_id, folder_id, req, None).await?;
    Ok(StatusCode::NO_CONTENT)
}
pub(crate) async fn mutate_folder_update(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    folder_id: String,
    req: UpdateFolderRequest,
    replay: Option<FolderReplayMetadata>,
) -> Result<(), AppError> {
    let user_id = auth.claims.sub.as_str();
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_folder_mutation_on_connection(&mut tx, &space_id, &folder_id, auth).await?;
    let current_parent_id = folder_parent_id_on_connection(&mut tx, &space_id, &folder_id).await?;
    validate_folder_structure_on_connection(&mut tx, &space_id, &folder_id).await?;

    if let Some(ref name) = req.name {
        if name.len() > 100 {
            return Err(AppError::BadRequest(
                "Folder name must be at most 100 characters".into(),
            ));
        }
    }

    let expected_parent_id = if replay.is_some() {
        req.parent_id.clone()
    } else {
        req.parent_id.clone().or(current_parent_id.clone())
    };
    let reparenting = expected_parent_id != current_parent_id;
    let inventory = if reparenting {
        let inventory = folder_mutation_inventory(&mut tx, &space_id, &folder_id).await?;
        require_folder_inventory_mutation(&mut tx, &space_id, &inventory, auth).await?;
        require_same_folder_inventory(&mut tx, &space_id, &folder_id, &inventory).await?;
        validate_folder_move_destination_on_connection(
            &mut tx,
            &space_id,
            &space_id,
            &folder_id,
            expected_parent_id.as_deref(),
        )
        .await?;
        Some(inventory)
    } else {
        None
    };
    if replay.is_some() || req.name.is_some() || req.icon.is_some() || req.parent_id.is_some() {
        let updated_at = super::routes::import_timestamp_or_now(
            replay
                .as_ref()
                .and_then(|import| import.updated_at.as_deref()),
        )?;
        let changed = if let Some(import) = &replay {
            sqlx::query("UPDATE folders SET name=$1,icon=$2,parent_id=$3,sort_order=COALESCE($4,sort_order),updated_at=$5 WHERE id=$6 AND space_id=$7")
                .bind(&req.name).bind(&req.icon).bind(&expected_parent_id).bind(import.sort_order).bind(&updated_at).bind(&folder_id).bind(&space_id).execute(&mut *tx).await
        } else {
            sqlx::query("UPDATE folders SET name=COALESCE($1,name),icon=COALESCE($2,icon),parent_id=COALESCE($3,parent_id),updated_at=$4 WHERE id=$5 AND space_id=$6")
                .bind(&req.name).bind(&req.icon).bind(&req.parent_id).bind(&updated_at).bind(&folder_id).bind(&space_id).execute(&mut *tx).await
        }.map_err(AppError::internal)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::NotFound("Folder not found".into()));
        }
    }
    if let Some(mut inventory) = inventory {
        require_folder_inventory_mutation(&mut tx, &space_id, &inventory, auth).await?;
        for (id, parent) in &mut inventory.folders {
            if id == &folder_id {
                *parent = expected_parent_id.clone();
            }
        }
        require_same_folder_inventory(&mut tx, &space_id, &folder_id, &inventory).await?;
    }

    require_folder_mutation_on_connection(&mut tx, &space_id, &folder_id, auth).await?;
    validate_folder_structure_on_connection(&mut tx, &space_id, &folder_id).await?;
    require_folder_parent_identity_on_connection(
        &mut tx,
        &space_id,
        &folder_id,
        expected_parent_id.as_deref(),
    )
    .await?;
    require_folder_space_mutation(&mut tx, auth, &space_id).await?;
    access::require_current_human_preference_write(&mut tx, auth).await?;
    tx.commit().await.map_err(AppError::internal)?;
    if replay.is_none() {
        crate::audit::log_action(
            &state.db,
            Some(user_id),
            None,
            "folder.update",
            Some("folder"),
            Some(&folder_id),
        )
        .await;
    }
    Ok(())
}

/// POST /api/v1/spaces/{space_id}/folders/{folder_id}/move
async fn move_folder(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, folder_id)): Path<(String, String)>,
    Json(req): Json<MoveFolderRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.target_space_id != space_id {
        let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
        require_folder_space_mutation(&mut conn, &auth, &space_id).await?;
        drop(conn);
        let source: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM folders WHERE id = $1 AND space_id = $2")
                .bind(&folder_id)
                .bind(&space_id)
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?;
        if source.is_none() {
            return Err(AppError::NotFound("Folder not found".into()));
        }
        let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
        require_folder_space_mutation(&mut conn, &auth, &req.target_space_id)
            .await
            .map_err(|error| match error {
                AppError::NotFound(_) | AppError::Forbidden(_) => {
                    AppError::NotFound("Move destination not found".into())
                }
                other => other,
            })?;
        drop(conn);
        if let Some(parent_id) = req.parent_id.as_deref() {
            let exists: Option<i64> =
                sqlx::query_scalar("SELECT 1 FROM folders WHERE id = $1 AND space_id = $2")
                    .bind(parent_id)
                    .bind(&req.target_space_id)
                    .fetch_optional(&state.db)
                    .await
                    .map_err(AppError::internal)?;
            if exists.is_none() {
                return Err(AppError::NotFound("Move destination not found".into()));
            }
        }

        validate_folder_move_destination(
            &state.db,
            &space_id,
            &req.target_space_id,
            &folder_id,
            req.parent_id.as_deref(),
        )
        .await?;

        super::documents::reject_unreviewed_cross_space_move(&space_id, &req.target_space_id)?;

        return Err(AppError::Conflict(
            "Cross-space folder moves require a metadata and access preview".into(),
        ));
    }
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_folder_mutation_on_connection(&mut tx, &space_id, &folder_id, &auth).await?;
    if let Some(parent_id) = req.parent_id.as_deref() {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM folders WHERE id = $1 AND space_id = $2")
                .bind(parent_id)
                .bind(&space_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(AppError::internal)?;
        if exists.is_none() {
            return Err(AppError::NotFound("Move destination not found".into()));
        }
    }
    let inventory = folder_mutation_inventory(&mut tx, &space_id, &folder_id).await?;
    require_folder_inventory_mutation(&mut tx, &space_id, &inventory, &auth).await?;
    require_same_folder_inventory(&mut tx, &space_id, &folder_id, &inventory).await?;
    validate_folder_move_destination_on_connection(
        &mut tx,
        &space_id,
        &space_id,
        &folder_id,
        req.parent_id.as_deref(),
    )
    .await?;
    let updated_at = chrono::Utc::now().to_rfc3339();
    for (id, _) in &inventory.folders {
        let changed = if id == &folder_id {
            sqlx::query(
                "UPDATE folders SET parent_id=$1, updated_at=$2 WHERE id=$3 AND space_id=$4",
            )
            .bind(&req.parent_id)
            .bind(&updated_at)
            .bind(id)
            .bind(&space_id)
            .execute(&mut *tx)
            .await
        } else {
            sqlx::query("UPDATE folders SET updated_at=$1 WHERE id=$2 AND space_id=$3")
                .bind(&updated_at)
                .bind(id)
                .bind(&space_id)
                .execute(&mut *tx)
                .await
        }
        .map_err(AppError::internal)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::Conflict(
                "Folder subtree changed; retry mutation".into(),
            ));
        }
    }
    for (document_id, old_folder) in &inventory.documents {
        let changed = sqlx::query("UPDATE documents SET last_updated_by=$1, updated_at=$2 WHERE id=$3 AND space_id=$4 AND folder_id=$5 AND deleted_at IS NULL AND archived_at IS NULL")
            .bind(&auth.claims.sub).bind(&updated_at).bind(document_id).bind(&space_id).bind(old_folder).execute(&mut *tx).await.map_err(AppError::internal)?;
        if changed.rows_affected() != 1 {
            return Err(AppError::Conflict(
                "Folder page association changed; retry mutation".into(),
            ));
        }
    }
    require_folder_inventory_mutation(&mut tx, &space_id, &inventory, &auth).await?;
    let mut expected = inventory.clone();
    for (id, parent) in &mut expected.folders {
        if id == &folder_id {
            *parent = req.parent_id.clone();
        }
    }
    require_same_folder_inventory(&mut tx, &space_id, &folder_id, &expected).await?;
    require_folder_parent_identity_on_connection(
        &mut tx,
        &space_id,
        &folder_id,
        req.parent_id.as_deref(),
    )
    .await?;
    require_folder_space_mutation(&mut tx, &auth, &space_id).await?;
    access::require_current_human_preference_write(&mut tx, &auth).await?;
    tx.commit().await.map_err(AppError::internal)?;
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

    use super::super::documents::tests::move_database_snapshot;
    use super::super::properties::tests::{property_test_state, test_auth};
    use crate::{auth::routes::AppError, AppState};
    use axum::{
        extract::{Path, State},
        response::IntoResponse,
        Json,
    };
    use std::time::Duration;

    async fn mutation_fixture() -> AppState {
        let state = property_test_state().await;
        for (id, space, parent) in [
            ("root", "space-a", None),
            ("child", "space-a", Some("root")),
            ("grandchild", "space-a", Some("child")),
            ("root-b", "space-a", None),
            ("leaf", "space-a", Some("root-b")),
            ("foreign", "space-b", None),
        ] {
            sqlx::query("INSERT INTO folders (id,space_id,parent_id,name,sort_order,created_at,updated_at) VALUES ($1,$2,$3,$1,4,'old','old')").bind(id).bind(space).bind(parent).execute(&state.db).await.unwrap();
        }
        sqlx::query("UPDATE documents SET folder_id='child' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO property_values (id,document_id,property_id,value) VALUES ('retained-value','target-a','number-a','42')").execute(&state.db).await.unwrap();
        state
    }

    fn create_request(parent: Option<&str>, id: Option<&str>) -> super::CreateFolderRequest {
        super::CreateFolderRequest {
            name: "Created".into(),
            parent_id: parent.map(str::to_owned),
            icon: Some("folder".into()),
            source_id: id.map(str::to_owned),
            created_at: None,
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn folder_routes_reject_foreign_ids_and_denied_access_without_mutation() {
        let state = mutation_fixture().await;
        let before = move_database_snapshot(&state).await;
        for (user, folder) in [
            ("editor-a", "foreign"),
            ("editor-a", "missing"),
            ("viewer-a", "root"),
        ] {
            for result in [
                super::update_folder(
                    State(state.clone()),
                    test_auth(user),
                    Path(("space-a".into(), folder.into())),
                    Json(super::UpdateFolderRequest {
                        name: Some("Overwrite".into()),
                        icon: None,
                        parent_id: None,
                    }),
                )
                .await
                .map(IntoResponse::into_response),
                super::reorder_folder(
                    State(state.clone()),
                    test_auth(user),
                    Path(("space-a".into(), folder.into())),
                    Json(super::ReorderRequest { sort_order: 99 }),
                )
                .await
                .map(IntoResponse::into_response),
            ] {
                assert_eq!(
                    result.err().unwrap().into_response().status(),
                    axum::http::StatusCode::NOT_FOUND
                );
            }
        }
        assert_eq!(move_database_snapshot(&state).await, before);
    }

    #[tokio::test]
    async fn folder_patch_validates_all_fields_before_writing_and_keeps_authorized_routes() {
        let state = mutation_fixture().await;
        let before = move_database_snapshot(&state).await;
        assert!(super::update_folder(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "leaf".into())),
            Json(super::UpdateFolderRequest {
                name: Some("Must not persist".into()),
                icon: Some("must-not-persist".into()),
                parent_id: Some("foreign".into())
            })
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
        let retained: Vec<_> = before
            .into_iter()
            .filter(|(table, _)| table != "folders" && table != "audit_log")
            .collect();
        super::update_folder(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "leaf".into())),
            Json(super::UpdateFolderRequest {
                name: Some("Renamed".into()),
                icon: Some("new-icon".into()),
                parent_id: Some("root".into()),
            }),
        )
        .await
        .unwrap();
        super::reorder_folder(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "leaf".into())),
            Json(super::ReorderRequest { sort_order: 12 }),
        )
        .await
        .unwrap();
        let row: (String, String, String, i64) =
            sqlx::query_as("SELECT name,icon,parent_id,sort_order FROM folders WHERE id='leaf'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(
            row,
            ("Renamed".into(), "new-icon".into(), "root".into(), 12)
        );
        super::update_folder(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "leaf".into())),
            Json(super::UpdateFolderRequest {
                name: None,
                icon: None,
                parent_id: None,
            }),
        )
        .await
        .unwrap();
        let parent: String = sqlx::query_scalar("SELECT parent_id FROM folders WHERE id='leaf'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(parent, "root");
        let after: Vec<_> = move_database_snapshot(&state)
            .await
            .into_iter()
            .filter(|(table, _)| table != "folders" && table != "audit_log")
            .collect();
        assert_eq!(after, retained);
    }

    #[tokio::test]
    async fn folder_create_preserves_deep_parent_and_rejects_global_import_collision() {
        let state = mutation_fixture().await;
        let root_id = uuid::Uuid::new_v4().to_string();
        super::create_folder(
            State(state.clone()),
            test_auth("owner-a"),
            Path("space-a".into()),
            Json(create_request(None, Some(&root_id))),
        )
        .await
        .unwrap();
        let leaf_id = uuid::Uuid::new_v4().to_string();
        super::create_folder(
            State(state.clone()),
            test_auth("editor-a"),
            Path("space-a".into()),
            Json(create_request(Some("child"), Some(&leaf_id))),
        )
        .await
        .unwrap();
        let foreign_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO folders (id,space_id,name) VALUES ($1,'space-b','Foreign')")
            .bind(&foreign_id)
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        for id in [&root_id, &foreign_id] {
            assert!(matches!(
                super::create_folder(
                    State(state.clone()),
                    test_auth("editor-a"),
                    Path("space-a".into()),
                    Json(create_request(None, Some(id)))
                )
                .await,
                Err(AppError::Conflict(_))
            ));
        }
        super::create_folder(State(state.clone()), test_auth("editor-a"), Path("space-a".into()),
            Json(create_request(Some("grandchild"), None))).await.unwrap();
        let before = move_database_snapshot(&state).await;
        for parent in ["foreign", "missing"] {
            assert!(super::create_folder(
                State(state.clone()),
                test_auth("editor-a"),
                Path("space-a".into()),
                Json(create_request(Some(parent), None))
            )
            .await
            .is_err());
        }
        assert_eq!(move_database_snapshot(&state).await, before);
    }

    #[tokio::test]
    async fn folder_mutations_reject_cycles_foreign_edges_and_missing_ancestors() {
        for corruption in ["UPDATE folders SET parent_id='child' WHERE id='root'", "UPDATE folders SET parent_id='foreign' WHERE id='root'", "INSERT INTO folders (id,space_id,parent_id,name) VALUES ('foreign-child','space-b','root','Secret')"] {
            let state=mutation_fixture().await; sqlx::query(corruption).execute(&state.db).await.unwrap(); let before=move_database_snapshot(&state).await;
            assert!(super::update_folder(State(state.clone()),test_auth("editor-a"),Path(("space-a".into(),"root".into())),Json(super::UpdateFolderRequest {name:Some("Overwrite".into()),icon:None,parent_id:None})).await.is_err());
            assert!(super::reorder_folder(State(state.clone()),test_auth("editor-a"),Path(("space-a".into(),"root".into())),Json(super::ReorderRequest {sort_order:99})).await.is_err());
            assert_eq!(move_database_snapshot(&state).await,before);
        }
        let state = mutation_fixture().await;
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE folders SET parent_id='missing' WHERE id='root'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(super::reorder_folder(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "root".into())),
            Json(super::ReorderRequest { sort_order: 99 })
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
    }

    #[tokio::test]
    async fn folder_updates_reject_self_and_descendant_atomically() {
        let state = mutation_fixture().await;
        let before = move_database_snapshot(&state).await;
        for (folder, parent) in [
            ("root", "root"),
            ("root", "grandchild"),
        ] {
            assert!(super::update_folder(
                State(state.clone()),
                test_auth("editor-a"),
                Path(("space-a".into(), folder.into())),
                Json(super::UpdateFolderRequest {
                    name: Some("No partial rename".into()),
                    icon: Some("No partial icon".into()),
                    parent_id: Some(parent.into())
                })
            )
            .await
            .is_err());
            assert_eq!(move_database_snapshot(&state).await, before);
        }
    }

    #[tokio::test]
    async fn final_folder_write_deactivation_or_revocation_rolls_back_every_row_and_audit() {
        for trigger in ["CREATE TRIGGER change_authority AFTER UPDATE OF name ON folders WHEN NEW.id='leaf' BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END", "CREATE TRIGGER change_authority AFTER UPDATE OF sort_order ON folders WHEN NEW.id='leaf' BEGIN UPDATE space_members SET role='viewer' WHERE user_id='editor-a'; END"] {
            let state=mutation_fixture().await; sqlx::query(trigger).execute(&state.db).await.unwrap(); let before=move_database_snapshot(&state).await;
            let rejected=if trigger.contains("OF name") {super::update_folder(State(state.clone()),test_auth("editor-a"),Path(("space-a".into(),"leaf".into())),Json(super::UpdateFolderRequest {name:Some("Changed".into()),icon:None,parent_id:None})).await.is_err()} else {super::reorder_folder(State(state.clone()),test_auth("editor-a"),Path(("space-a".into(),"leaf".into())),Json(super::ReorderRequest {sort_order:99})).await.is_err()};
            assert!(rejected); assert_eq!(move_database_snapshot(&state).await,before);
        }
    }

    #[tokio::test]
    async fn final_create_hierarchy_change_and_insert_failure_roll_back() {
        for trigger in ["CREATE TRIGGER fail_create AFTER INSERT ON folders BEGIN UPDATE folders SET parent_id='grandchild' WHERE id=NEW.id; END", "CREATE TRIGGER fail_create BEFORE INSERT ON folders BEGIN SELECT RAISE(ABORT,'injected folder persistence failure'); END"] {
            let state=mutation_fixture().await; sqlx::query(trigger).execute(&state.db).await.unwrap(); let before=move_database_snapshot(&state).await;
            assert!(super::create_folder(State(state.clone()),test_auth("editor-a"),Path("space-a".into()),Json(create_request(None,None))).await.is_err()); assert_eq!(move_database_snapshot(&state).await,before);
        }
    }

    #[tokio::test]
    async fn waited_create_rechecks_current_graph_and_revoked_authority() {
        for graph_change in [false, true] {
            let state = mutation_fixture().await;
            let mut blocker = state.db.begin().await.unwrap();
            super::lock_spaces_for_mutation(&mut blocker, &["space-a"])
                .await
                .unwrap();
            let worker_state = state.clone();
            let worker = tokio::spawn(async move {
                super::create_folder(
                    State(worker_state),
                    test_auth("editor-a"),
                    Path("space-a".into()),
                    Json(create_request(Some("child"), None)),
                )
                .await
                .is_err()
            });
            tokio::task::yield_now().await;
            assert!(!worker.is_finished());
            if graph_change {
                sqlx::query("UPDATE folders SET parent_id='grandchild' WHERE id='root'")
                    .execute(&mut *blocker)
                    .await
                    .unwrap();
            } else {
                sqlx::query("DELETE FROM space_members WHERE user_id='editor-a'")
                    .execute(&mut *blocker)
                    .await
                    .unwrap();
            }
            blocker.commit().await.unwrap();
            let before = move_database_snapshot(&state).await;
            assert!(tokio::time::timeout(Duration::from_secs(5), worker)
                .await
                .unwrap()
                .unwrap());
            assert_eq!(move_database_snapshot(&state).await, before);
        }
    }

    #[tokio::test]
    async fn legacy_space_edit_expands_folder_management_but_manage_space_alone_does_not() {
        let state = mutation_fixture().await;
        for (role, permission) in [
            ("custom-edit", "edit_content"),
            ("custom-manage", "manage_space"),
        ] {
            sqlx::query("INSERT INTO space_roles (id,space_id,name,permissions,created_by) VALUES ($1,'space-a',$1,$2,'owner-a')").bind(role).bind(format!("[\"{permission}\"]")).execute(&state.db).await.unwrap();
        }
        sqlx::query("UPDATE space_members SET role='custom-manage' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(super::reorder_folder(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "leaf".into())),
            Json(super::ReorderRequest { sort_order: 99 })
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
        sqlx::query("UPDATE space_members SET role='custom-edit' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        super::reorder_folder(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "leaf".into())),
            Json(super::ReorderRequest { sort_order: 99 }),
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn connection_depth_helper_works_inside_one_connection_transaction() {
        let state = mutation_fixture().await;
        let mut tx = state.db.begin().await.unwrap();
        super::lock_spaces_for_mutation(&mut tx, &["space-a"])
            .await
            .unwrap();
        super::validate_document_folder_depth_on_connection(&mut tx, "space-a", None)
            .await
            .unwrap();
        super::validate_document_folder_depth_on_connection(&mut tx, "space-a", Some("grandchild"))
            .await
            .unwrap();
        assert!(super::validate_document_folder_depth_on_connection(
            &mut tx,
            "space-a",
            Some("foreign")
        )
        .await
        .is_err());
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn create_rejects_valid_same_space_parent_redirection_without_persisting_any_rows() {
        let state = mutation_fixture().await;
        sqlx::query("CREATE TRIGGER redirect_created_folder AFTER INSERT ON folders BEGIN UPDATE folders SET parent_id='root-b' WHERE id=NEW.id; END").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        let result = super::create_folder(
            State(state.clone()),
            test_auth("editor-a"),
            Path("space-a".into()),
            Json(create_request(None, None)),
        )
        .await;
        assert!(
            matches!(result, Err(AppError::Conflict(message)) if message.contains("placement changed"))
        );
        assert_eq!(move_database_snapshot(&state).await, before);
    }

    #[tokio::test]
    async fn update_parent_redirection_rolls_back_with_explicit_and_null_parent_intent() {
        for requested_parent in [Some("root"), None] {
            let state = mutation_fixture().await;
            let redirected_parent = if requested_parent.is_some() {
                "root-b"
            } else {
                "root"
            };
            let trigger = format!("CREATE TRIGGER redirect_updated_folder AFTER UPDATE OF name ON folders WHEN NEW.id='leaf' BEGIN UPDATE folders SET parent_id='{redirected_parent}' WHERE id=NEW.id; END");
            sqlx::query(&trigger).execute(&state.db).await.unwrap();
            let before = move_database_snapshot(&state).await;
            let result = super::update_folder(
                State(state.clone()),
                test_auth("editor-a"),
                Path(("space-a".into(), "leaf".into())),
                Json(super::UpdateFolderRequest {
                    name: Some("Must roll back".into()),
                    icon: Some("also roll back".into()),
                    parent_id: requested_parent.map(str::to_owned),
                }),
            )
            .await;
            assert!(
                matches!(result, Err(AppError::Conflict(message)) if message.contains("placement changed"))
            );
            assert_eq!(move_database_snapshot(&state).await, before);
        }
    }

    #[tokio::test]
    async fn reorder_rejects_valid_same_space_reparenting_without_persisting_sort_order() {
        let state = mutation_fixture().await;
        sqlx::query("CREATE TRIGGER redirect_reordered_folder AFTER UPDATE OF sort_order ON folders WHEN NEW.id='leaf' BEGIN UPDATE folders SET parent_id='root' WHERE id=NEW.id; END").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        let result = super::reorder_folder(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "leaf".into())),
            Json(super::ReorderRequest { sort_order: 99 }),
        )
        .await;
        assert!(
            matches!(result, Err(AppError::Conflict(message)) if message.contains("placement changed"))
        );
        assert_eq!(move_database_snapshot(&state).await, before);
    }

    fn same_space_move(parent: Option<&str>) -> super::MoveFolderRequest {
        super::MoveFolderRequest {
            target_space_id: "space-a".into(),
            parent_id: parent.map(str::to_owned),
        }
    }

    async fn attempt_folder_subtree(state: &AppState, deleting: bool) -> bool {
        if deleting {
            super::delete_folder(
                State(state.clone()),
                test_auth("editor-a"),
                Path(("space-a".into(), "child".into())),
            )
            .await
            .is_err()
        } else {
            super::move_folder(
                State(state.clone()),
                test_auth("editor-a"),
                Path(("space-a".into(), "child".into())),
                Json(same_space_move(None)),
            )
            .await
            .is_err()
        }
    }

    async fn document_snapshot_except(state: &AppState, excluded: &[&str]) -> Option<String> {
        use sqlx::Row;
        let columns = sqlx::query("PRAGMA table_info(documents)")
            .fetch_all(&state.db)
            .await
            .unwrap();
        let expression = columns
            .iter()
            .map(|row| row.get::<String, _>("name"))
            .filter(|name| !excluded.contains(&name.as_str()))
            .map(|name| format!("quote(\"{name}\")"))
            .collect::<Vec<_>>()
            .join(" || '|' || ");
        sqlx::query_scalar(&format!(
            "SELECT group_concat({expression},char(10)) FROM (SELECT * FROM documents ORDER BY id)"
        ))
        .fetch_one(&state.db)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn same_space_move_and_delete_preserve_pages_drafts_metadata_history_links_and_files() {
        use super::super::documents::tests::move_file_snapshot;
        let state = mutation_fixture().await;
        let slug = format!("folder-preserve-{}", uuid::Uuid::new_v4());
        sqlx::query("UPDATE spaces SET slug=$1 WHERE id='space-a'")
            .bind(&slug)
            .execute(&state.db)
            .await
            .unwrap();
        let path = crate::storage::write_document(
            &slug,
            "target-a",
            "Preserved",
            "title: Preserved",
            "Canonical saved body",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE documents SET content_path=$1, public=1, public_slug='retained-link' WHERE id='target-a'").bind(path).execute(&state.db).await.unwrap();
        for sql in [
            "INSERT INTO tags (id,space_id,name) VALUES ('retained-tag','space-a','Retained')",
            "INSERT INTO doc_tags VALUES ('target-a','retained-tag')",
            "INSERT INTO document_drafts (id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES ('retained-draft','target-a','space-a','editor-a','Draft','Saved private body','old')",
            "INSERT INTO document_draft_access (draft_id,user_id,role,granted_by) VALUES ('retained-draft','viewer-a','editor','editor-a')",
            "INSERT INTO workflow_templates (id,space_id,name,stages,approvers,created_by) VALUES ('retained-template','space-a','Retained','[]','[]','owner-a')",
            "INSERT INTO document_workflows (id,document_id,workflow_template_id,current_stage_id,status,created_by) VALUES ('retained-workflow','target-a','retained-template','review','active','owner-a')",
            "INSERT INTO document_workflow_events (id,document_workflow_id,document_id,to_stage_id,action,created_by) VALUES ('retained-event','retained-workflow','target-a','review','enable','owner-a')",
            "INSERT INTO doc_versions (id,document_id,version_number,change_set,created_by) VALUES ('retained-version','target-a',1,X'0102','owner-a')",
            "INSERT INTO attachments (id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES ('retained-asset','target-a','space-a','file.bin','file.bin','application/octet-stream',4,'attachments/file.bin','owner-a')",
        ] {sqlx::query(sql).execute(&state.db).await.unwrap();}
        let attachment = crate::storage::space_dir(&slug).join("attachments/file.bin");
        tokio::fs::create_dir_all(attachment.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(attachment, b"data").await.unwrap();
        let version = crate::storage::space_dir(&slug).join("history/version.md");
        tokio::fs::create_dir_all(version.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(version, b"Historical saved body")
            .await
            .unwrap();
        let files = move_file_snapshot(&slug);
        let retained: Vec<_> = move_database_snapshot(&state)
            .await
            .into_iter()
            .filter(|(table, _)| !matches!(table.as_str(), "folders" | "documents" | "audit_log"))
            .collect();
        let docs_before =
            document_snapshot_except(&state, &["updated_at", "last_updated_by"]).await;
        for parent in [Some("root-b"), None] {
            super::move_folder(
                State(state.clone()),
                test_auth("editor-a"),
                Path(("space-a".into(), "child".into())),
                Json(same_space_move(parent)),
            )
            .await
            .unwrap();
            let actual: Option<String> =
                sqlx::query_scalar("SELECT parent_id FROM folders WHERE id='child'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(actual.as_deref(), parent);
        }
        assert_eq!(
            document_snapshot_except(&state, &["updated_at", "last_updated_by"]).await,
            docs_before
        );
        let before_delete = document_snapshot_except(&state, &["folder_id"]).await;
        super::delete_folder(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "child".into())),
        )
        .await
        .unwrap();
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM folders WHERE id IN ('child','grandchild')")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(count, 0);
        let folder: Option<String> =
            sqlx::query_scalar("SELECT folder_id FROM documents WHERE id='target-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(folder, None);
        assert_eq!(
            document_snapshot_except(&state, &["folder_id"]).await,
            before_delete
        );
        let after: Vec<_> = move_database_snapshot(&state)
            .await
            .into_iter()
            .filter(|(table, _)| !matches!(table.as_str(), "folders" | "documents" | "audit_log"))
            .collect();
        assert_eq!(after, retained);
        assert_eq!(move_file_snapshot(&slug), files);
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn subtree_operations_reject_private_locked_inactive_and_lifecycle_pages() {
        for sql in [
            "UPDATE documents SET published_at=NULL WHERE id='target-a'",
            "UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'",
            "UPDATE documents SET archived_at='old' WHERE id='target-a'",
            "UPDATE documents SET deleted_at='old' WHERE id='target-a'",
            "UPDATE users SET is_active=0 WHERE id='editor-a'",
            "DELETE FROM space_members WHERE user_id='editor-a'",
        ] {
            for deleting in [false, true] {
                let state = mutation_fixture().await;
                sqlx::query(sql).execute(&state.db).await.unwrap();
                let before = move_database_snapshot(&state).await;
                assert!(attempt_folder_subtree(&state, deleting).await);
                assert_eq!(move_database_snapshot(&state).await, before);
            }
        }
    }

    #[tokio::test]
    async fn subtree_operations_allow_owned_or_editor_shared_private_drafts() {
        for owner in ["editor-a", "owner-a"] {
            let state = mutation_fixture().await;
            sqlx::query("UPDATE documents SET published_at=NULL,locked=1,locked_by='editor-a' WHERE id='target-a'").execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO document_drafts (id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES ('private-draft','target-a','space-a',$1,'Private','Saved','old')").bind(owner).execute(&state.db).await.unwrap();
            if owner == "owner-a" {
                sqlx::query("INSERT INTO document_draft_access (draft_id,user_id,role,granted_by) VALUES ('private-draft','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
                let before = move_database_snapshot(&state).await;
                assert!(attempt_folder_subtree(&state, false).await);
                assert!(attempt_folder_subtree(&state, true).await);
                assert_eq!(move_database_snapshot(&state).await, before);
                sqlx::query(
                    "UPDATE document_draft_access SET role='editor' WHERE draft_id='private-draft'",
                )
                .execute(&state.db)
                .await
                .unwrap();
            }
            assert!(!attempt_folder_subtree(&state, false).await);
            assert!(!attempt_folder_subtree(&state, true).await);
            let count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM document_drafts WHERE id='private-draft'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(count, 1);
        }
    }

    #[tokio::test]
    async fn subtree_operations_reject_foreign_and_orphan_references_without_cascade_loss() {
        for sql in ["INSERT INTO folders (id,space_id,parent_id,name) VALUES ('foreign-child','space-b','child','Secret')","UPDATE documents SET folder_id='child' WHERE id='target-b'","UPDATE folders SET parent_id='missing-parent' WHERE id='child'","UPDATE folders SET space_id='missing-space' WHERE id='grandchild'","UPDATE documents SET space_id='missing-space' WHERE id='target-a'"] {
            for deleting in [false,true] {
                let state=mutation_fixture().await; sqlx::query("PRAGMA foreign_keys=OFF").execute(&state.db).await.unwrap(); sqlx::query(sql).execute(&state.db).await.unwrap(); sqlx::query("PRAGMA foreign_keys=ON").execute(&state.db).await.unwrap(); let before=move_database_snapshot(&state).await;
                assert!(attempt_folder_subtree(&state,deleting).await); assert_eq!(move_database_snapshot(&state).await,before);
            }
        }
    }

    #[tokio::test]
    async fn root_folder_move_rejects_cycles() {
        for sql in ["UPDATE folders SET parent_id='grandchild' WHERE id='child'"] {
            let state=mutation_fixture().await; sqlx::query(sql).execute(&state.db).await.unwrap(); let before=move_database_snapshot(&state).await;
            assert!(attempt_folder_subtree(&state,false).await); assert_eq!(move_database_snapshot(&state).await,before);
        }
        let state = mutation_fixture().await;
        let before = move_database_snapshot(&state).await;
        assert!(super::move_folder(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "child".into())),
            Json(same_space_move(Some("grandchild")))
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
    }

    #[tokio::test]
    async fn subtree_last_write_failure_or_authority_change_rolls_back_sql_and_cascade() {
        for deleting in [false, true] {
            let triggers = if deleting {
                vec!["CREATE TRIGGER fail_subtree AFTER DELETE ON folders WHEN OLD.id='child' BEGIN SELECT RAISE(ABORT,'injected cascade failure'); END","CREATE TRIGGER fail_subtree AFTER DELETE ON folders WHEN OLD.id='child' BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END","CREATE TRIGGER fail_subtree AFTER DELETE ON folders WHEN OLD.id='child' BEGIN UPDATE space_members SET role='viewer' WHERE user_id='editor-a'; END"]
            } else {
                vec!["CREATE TRIGGER fail_subtree BEFORE UPDATE OF updated_at ON documents WHEN NEW.id='target-a' BEGIN SELECT RAISE(ABORT,'injected page persistence failure'); END","CREATE TRIGGER fail_subtree AFTER UPDATE OF updated_at ON documents WHEN NEW.id='target-a' BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END","CREATE TRIGGER fail_subtree AFTER UPDATE OF updated_at ON documents WHEN NEW.id='target-a' BEGIN UPDATE space_members SET role='viewer' WHERE user_id='editor-a'; END"]
            };
            for trigger in triggers {
                let state = mutation_fixture().await;
                sqlx::query(trigger).execute(&state.db).await.unwrap();
                let before = move_database_snapshot(&state).await;
                assert!(attempt_folder_subtree(&state, deleting).await);
                assert_eq!(move_database_snapshot(&state).await, before);
            }
        }
    }

    #[tokio::test]
    async fn subtree_final_placement_redirection_rolls_back_move_and_deletion() {
        for deleting in [false, true] {
            let state = mutation_fixture().await;
            let trigger = if deleting {
                "CREATE TRIGGER redirect_page AFTER UPDATE OF folder_id ON documents WHEN NEW.id='target-a' AND NEW.folder_id IS NULL BEGIN UPDATE documents SET folder_id='root-b' WHERE id=NEW.id; END"
            } else {
                "CREATE TRIGGER redirect_page AFTER UPDATE OF updated_at ON documents WHEN NEW.id='target-a' BEGIN UPDATE documents SET folder_id='root-b' WHERE id=NEW.id; END"
            };
            sqlx::query(trigger).execute(&state.db).await.unwrap();
            let before = move_database_snapshot(&state).await;
            assert!(attempt_folder_subtree(&state, deleting).await);
            assert_eq!(move_database_snapshot(&state).await, before);
        }
    }

    #[tokio::test]
    async fn waiting_subtree_operation_rechecks_actual_scope_policy_and_page_lifecycle() {
        for deleting in [false, true] {
            for sql in [
                "UPDATE documents SET archived_at='old' WHERE id='target-a'",
                "UPDATE folders SET space_id='space-b' WHERE id='child'",
                "DELETE FROM space_members WHERE user_id='editor-a'",
            ] {
                let state = mutation_fixture().await;
                let mut blocker = state.db.begin().await.unwrap();
                super::lock_spaces_for_mutation(&mut blocker, &["space-a"])
                    .await
                    .unwrap();
                let worker_state = state.clone();
                let worker =
                    tokio::spawn(
                        async move { attempt_folder_subtree(&worker_state, deleting).await },
                    );
                tokio::task::yield_now().await;
                assert!(!worker.is_finished());
                sqlx::query(sql).execute(&mut *blocker).await.unwrap();
                blocker.commit().await.unwrap();
                let before = move_database_snapshot(&state).await;
                assert!(tokio::time::timeout(Duration::from_secs(5), worker)
                    .await
                    .unwrap()
                    .unwrap());
                assert_eq!(move_database_snapshot(&state).await, before);
            }
        }
    }

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
                 VALUES ($1, 'space-1', $2, $1, '2026-07-10T00:00:00Z', '2026-07-10T00:00:00Z')",
            )
            .bind(id)
            .bind(parent_id)
            .execute(&db)
            .await
            .expect("insert folder");
        }

        db
    }

    async fn legacy_move_fixture() -> (
        AppState,
        String,
        String,
        crate::auth::middleware::AuthUser,
        crate::auth::middleware::AuthUser,
    ) {
        let state = mutation_fixture().await;
        for (actor, role) in [("mover", "member"), ("admin-mover", "admin")] {
            sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,$3)")
                .bind(actor)
                .bind(format!("{actor}@folder.test"))
                .bind(role)
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'test','2999-01-01T00:00:00Z')").bind(format!("metadata-{actor}")).bind(actor).execute(&state.db).await.unwrap();
        }
        let source_slug = format!("legacy-folder-source-{}", uuid::Uuid::new_v4());
        let target_slug = format!("legacy-folder-target-{}", uuid::Uuid::new_v4());
        for (id, slug) in [("move-source", &source_slug), ("move-target", &target_slug)] {
            sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,$1,$2,'private','mover')").bind(id).bind(slug).execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO space_members(id,space_id,user_id,role,created_at) VALUES($1,$2,'mover','editor','old')").bind(format!("legacy-mover-{id}")).bind(id).execute(&state.db).await.unwrap();
        }
        for (id, space, parent) in [
            ("source-folder", "move-source", None),
            ("child-folder", "move-source", Some("source-folder")),
            ("target-folder", "move-target", None),
        ] {
            sqlx::query("INSERT INTO folders(id,space_id,parent_id,name) VALUES($1,$2,$3,$1)")
                .bind(id)
                .bind(space)
                .bind(parent)
                .execute(&state.db)
                .await
                .unwrap();
        }
        crate::storage::write_document(
            &source_slug,
            "legacy-folder-body",
            "Retained",
            "",
            "RETAINED BYTES",
        )
        .await
        .unwrap();
        let mover = super::current_folder_mutation_tests::proof(&state, "mover").await;
        let admin = super::current_folder_mutation_tests::proof(&state, "admin-mover").await;
        (state, source_slug, target_slug, mover, admin)
    }

    #[tokio::test]
    async fn legacy_cross_space_folder_moves_preserve_every_record_and_file() {
        use super::super::documents::tests::{move_database_snapshot, move_file_snapshot};
        use axum::{
            extract::{Path, State},
            response::IntoResponse,
            Json,
        };
        let (state, source_slug, target_slug, mover, _) = legacy_move_fixture().await;
        let before = move_database_snapshot(&state).await;
        let files = move_file_snapshot(&source_slug);
        for id in ["source-folder", "child-folder"] {
            let error = super::move_folder(
                State(state.clone()),
                super::current_folder_mutation_tests::copied(&mover),
                Path(("move-source".into(), id.into())),
                Json(super::MoveFolderRequest {
                    target_space_id: "move-target".into(),
                    parent_id: Some("target-folder".into()),
                }),
            )
            .await
            .err()
            .unwrap();
            assert!(
                matches!(&error,crate::auth::routes::AppError::Conflict(message) if message.contains("metadata and access preview"))
            );
            assert_eq!(
                error.into_response().status(),
                axum::http::StatusCode::CONFLICT
            );
        }
        assert_eq!(move_database_snapshot(&state).await, before);
        assert_eq!(move_file_snapshot(&source_slug), files);
        assert!(!crate::storage::space_dir(&target_slug).exists());
        tokio::fs::remove_dir_all(crate::storage::space_dir(&source_slug))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn folder_move_resolves_source_and_conceals_missing_or_inaccessible_targets() {
        use super::super::documents::tests::move_database_snapshot;
        use axum::{
            extract::{Path, State},
            response::IntoResponse,
            Json,
        };
        let (state, source_slug, _, mover, admin) = legacy_move_fixture().await;
        let before = move_database_snapshot(&state).await;
        for (user, source, id, target, parent) in [
            ("mover", "move-source", "missing", "move-target", None),
            ("mover", "move-target", "source-folder", "move-target", None),
            ("mover", "move-source", "source-folder", "missing", None),
            (
                "admin-mover",
                "move-source",
                "source-folder",
                "missing",
                None,
            ),
            (
                "mover",
                "move-source",
                "source-folder",
                "move-target",
                Some("missing"),
            ),
        ] {
            let error = super::move_folder(
                State(state.clone()),
                super::current_folder_mutation_tests::copied(if user == "mover" {
                    &mover
                } else {
                    &admin
                }),
                Path((source.into(), id.into())),
                Json(super::MoveFolderRequest {
                    target_space_id: target.into(),
                    parent_id: parent.map(str::to_string),
                }),
            )
            .await
            .err()
            .unwrap();
            assert_eq!(
                error.into_response().status(),
                axum::http::StatusCode::NOT_FOUND
            );
        }
        sqlx::query("DELETE FROM space_members WHERE space_id = 'move-target'")
            .execute(&state.db)
            .await
            .unwrap();
        let error = super::move_folder(
            State(state.clone()),
            super::current_folder_mutation_tests::copied(&mover),
            Path(("move-source".into(), "source-folder".into())),
            Json(super::MoveFolderRequest {
                target_space_id: "move-target".into(),
                parent_id: None,
            }),
        )
        .await
        .err()
        .unwrap();
        assert!(
            matches!(error,crate::auth::routes::AppError::NotFound(message) if message == "Move destination not found")
        );
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role,created_at) VALUES ('legacy-mover-move-target','move-target','mover','editor','old')")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(move_database_snapshot(&state).await, before);
        tokio::fs::remove_dir_all(crate::storage::space_dir(&source_slug))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn validates_new_folder_parent_beyond_three_levels() {
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

        validate_new_folder_parent(&db, "space-1", Some("grandchild"))
            .await.expect("fourth-level folder is allowed");
    }

    #[tokio::test]
    async fn validates_deep_folder_move_and_rejects_cycles() {
        let db = hierarchy_pool().await;

        validate_moved_folder_parent(&db, "space-1", "branch-child", "root")
            .await
            .expect("moving a leaf under a root stays within the cap");

        validate_moved_folder_parent(&db, "space-1", "branch-parent", "child")
            .await.expect("moving a subtree beyond three levels is allowed");

        let cycle_error = validate_moved_folder_parent(&db, "space-1", "root", "grandchild")
            .await
            .expect_err("moving into a descendant is blocked");
        assert!(format!("{cycle_error:?}")
            .contains("Folder cannot be moved inside one of its descendants"));
    }

    #[tokio::test]
    async fn validates_deep_document_folder_placement() {
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
        .expect("insert valid deep folder");

        validate_document_folder_depth(&db, "space-1", Some("too-deep"))
            .await.expect("pages can occupy a deep folder without changing page parent policy");
    }
}

/// DELETE /api/v1/spaces/{space_id}/folders/{folder_id}
async fn delete_folder(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, folder_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    mutate_folder_delete(
        &state,
        &auth,
        space_id,
        folder_id,
        FolderMutationOrigin::Rest,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
pub(crate) async fn mutate_folder_delete(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    folder_id: String,
    origin: FolderMutationOrigin,
) -> Result<(), AppError> {
    let user_id = auth.claims.sub.as_str();
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_folder_mutation_on_connection(&mut tx, &space_id, &folder_id, auth).await?;
    let inventory = folder_mutation_inventory(&mut tx, &space_id, &folder_id).await?;
    require_folder_inventory_mutation(&mut tx, &space_id, &inventory, auth).await?;
    require_same_folder_inventory(&mut tx, &space_id, &folder_id, &inventory).await?;
    // Existing FK semantics: child folders cascade; pages survive with folder_id=NULL.
    let deleted = sqlx::query("DELETE FROM folders WHERE id=$1 AND space_id=$2")
        .bind(&folder_id)
        .bind(&space_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if deleted.rows_affected() != 1 {
        return Err(AppError::NotFound("Folder not found".into()));
    }
    for (id, _) in &inventory.folders {
        let remaining: Option<i64> = sqlx::query_scalar("SELECT 1 FROM folders WHERE id=$1")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::internal)?;
        if remaining.is_some() {
            return Err(AppError::Conflict(
                "Folder deletion did not preserve its scoped intent".into(),
            ));
        }
    }
    for (document_id, _) in &inventory.documents {
        require_current_page_edit(
            &mut tx,
            access::principal_from_auth(auth)?,
            &space_id,
            document_id,
        )
        .await?;
        let actual: Option<(Option<String>,)> =
            sqlx::query_as("SELECT folder_id FROM documents WHERE id=$1 AND space_id=$2")
                .bind(document_id)
                .bind(&space_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(AppError::internal)?;
        if actual != Some((None,)) {
            return Err(AppError::Conflict(
                "Folder page placement changed; retry deletion".into(),
            ));
        }
    }
    require_folder_space_mutation(&mut tx, auth, &space_id).await?;
    access::require_current_human_preference_write(&mut tx, auth).await?;
    tx.commit().await.map_err(AppError::internal)?;
    if origin == FolderMutationOrigin::Rest {
        crate::audit::log_action(
            &state.db,
            Some(user_id),
            None,
            "folder.delete",
            Some("folder"),
            Some(&folder_id),
        )
        .await;
    }
    Ok(())
}

/// PUT /api/v1/spaces/{space_id}/folders/{folder_id}/reorder
async fn reorder_folder(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, folder_id)): Path<(String, String)>,
    Json(req): Json<ReorderRequest>,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_folder_mutation_on_connection(&mut tx, &space_id, &folder_id, &auth).await?;
    let current_parent_id = folder_parent_id_on_connection(&mut tx, &space_id, &folder_id).await?;
    validate_folder_structure_on_connection(&mut tx, &space_id, &folder_id).await?;

    let updated_at = chrono::Utc::now().to_rfc3339();
    let changed = sqlx::query(
        "UPDATE folders SET sort_order = $1, updated_at = $2 WHERE id = $3 AND space_id = $4",
    )
    .bind(req.sort_order)
    .bind(&updated_at)
    .bind(&folder_id)
    .bind(&space_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::internal)?;
    if changed.rows_affected() != 1 {
        return Err(AppError::NotFound("Folder not found".into()));
    }

    require_folder_mutation_on_connection(&mut tx, &space_id, &folder_id, &auth).await?;
    validate_folder_structure_on_connection(&mut tx, &space_id, &folder_id).await?;
    require_folder_parent_identity_on_connection(
        &mut tx,
        &space_id,
        &folder_id,
        current_parent_id.as_deref(),
    )
    .await?;
    require_folder_space_mutation(&mut tx, &auth, &space_id).await?;
    access::require_current_human_preference_write(&mut tx, &auth).await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod catalog_read_tests {
    use super::super::routes::catalog_tests::{
        auth, body, catalog_state, copy_auth, key, pause, pause_final,
    };
    use super::*;
    #[tokio::test]
    async fn catalog_folders_preserve_space_catalog_visibility_and_reject_corrupt_chains() {
        let state = catalog_state(false).await;
        sqlx::query("INSERT INTO documents(id,space_id,folder_id,title,content_path,created_by,last_updated_by) VALUES('private-page','space-a','folder-a','PRIVATE PAGE','','owner-a','owner-a')").execute(&state.db).await.unwrap();
        let visible = body(
            list_folders(
                State(state.clone()),
                auth("viewer-a"),
                Path("space-a".into()),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(visible.as_array().unwrap().len(), 2);
        assert!(!visible.to_string().contains("PRIVATE PAGE"));
        assert!(get_folder(
            State(state.clone()),
            auth("viewer-a"),
            Path(("space-a".into(), "folder-a".into()))
        )
        .await
        .is_ok());
        for parent in ["foreign-folder", "missing-folder", "child-a"] {
            sqlx::query("PRAGMA foreign_keys=OFF")
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::query("UPDATE folders SET parent_id=$1 WHERE id='folder-a'")
                .bind(parent)
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::query("PRAGMA foreign_keys=ON")
                .execute(&state.db)
                .await
                .unwrap();
            assert!(
                matches!(
                    list_folders(
                        State(state.clone()),
                        auth("viewer-a"),
                        Path("space-a".into())
                    )
                    .await,
                    Err(AppError::NotFound(_))
                ),
                "{parent}"
            );
            assert!(
                matches!(
                    get_folder(
                        State(state.clone()),
                        auth("viewer-a"),
                        Path(("space-a".into(), "folder-a".into()))
                    )
                    .await,
                    Err(AppError::NotFound(_))
                ),
                "{parent}"
            );
        }
    }
    #[tokio::test]
    async fn catalog_folder_reads_use_current_keys_and_uniform_scoped_targets() {
        let state = catalog_state(false).await;
        let read = key(&state, "folder-read", Some("viewer-a"), "[\"read\"]").await;
        assert!(list_folders(
            State(state.clone()),
            copy_auth(&read),
            Path("space-a".into())
        )
        .await
        .is_ok());
        assert!(get_folder(
            State(state.clone()),
            copy_auth(&read),
            Path(("space-a".into(), "folder-a".into()))
        )
        .await
        .is_ok());
        for id in ["missing-folder", "foreign-folder"] {
            assert!(matches!(
                get_folder(
                    State(state.clone()),
                    copy_auth(&read),
                    Path(("space-a".into(), id.into()))
                )
                .await,
                Err(AppError::NotFound(_))
            ));
        }
        sqlx::query("UPDATE api_keys SET scopes='[\"write\"]' WHERE id='folder-read'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(
            list_folders(State(state.clone()), read, Path("space-a".into()))
                .await
                .is_err()
        );
        let service = key(&state, "folder-service", None, "[\"admin\"]").await;
        assert!(matches!(
            list_folders(State(state), service, Path("space-a".into())).await,
            Err(AppError::Unauthorized(_))
        ));
    }
    #[tokio::test]
    async fn catalog_folder_reads_recheck_external_wal_credentials_and_hierarchy() {
        for sql in [
            "DELETE FROM sessions WHERE id='metadata-viewer-a'",
            "UPDATE folders SET parent_id='foreign-folder' WHERE id='folder-a'",
        ] {
            let state = catalog_state(true).await;
            pause(&state, sql);
            assert!(get_folder(
                State(state.clone()),
                auth("viewer-a"),
                Path(("space-a".into(), "folder-a".into()))
            )
            .await
            .is_err());
        }
        let state = catalog_state(true).await;
        pause(&state,"INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')");
        assert!(
            list_folders(State(state), auth("viewer-a"), Path("space-a".into()))
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn catalog_folder_scope_and_chain_are_last_after_space_policy_wait() {
        for (get, sql) in [
            (
                true,
                "UPDATE folders SET space_id='space-b' WHERE id='folder-a'",
            ),
            (
                false,
                "UPDATE folders SET parent_id='foreign-folder' WHERE id='folder-a'",
            ),
        ] {
            let state = catalog_state(true).await;
            pause_final(&state, sql);
            let denied = if get {
                get_folder(
                    State(state.clone()),
                    auth("viewer-a"),
                    Path(("space-a".into(), "folder-a".into())),
                )
                .await
                .is_err()
            } else {
                list_folders(
                    State(state.clone()),
                    auth("viewer-a"),
                    Path("space-a".into()),
                )
                .await
                .is_err()
            };
            assert!(
                denied,
                "Externally committed hierarchy change must not emit stale folder"
            );
        }
    }
}

#[cfg(test)]
mod current_folder_mutation_tests {
    use super::super::documents::tests::move_database_snapshot;
    use super::super::routes::catalog_tests::{auth, catalog_state};
    use super::*;
    use axum::extract::FromRequestParts;
    use tower::ServiceExt;
    async fn setup(wal: bool) -> AppState {
        let state = catalog_state(wal).await;
        sqlx::query("INSERT INTO folders(id,space_id,name) VALUES('destination-folder','space-a','Destination')").execute(&state.db).await.unwrap();
        state
    }
    pub(super) async fn proof(state: &AppState, actor: &str) -> AuthUser {
        let claims = auth(actor).claims;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let req = axum::http::Request::builder()
            .method("POST")
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        AuthUser::from_request_parts(&mut parts, state)
            .await
            .unwrap()
    }
    pub(super) fn copied(a: &AuthUser) -> AuthUser {
        AuthUser {
            claims: a.claims.clone(),
            api_key_scopes: a.api_key_scopes.clone(),
        }
    }
    async fn write(state: &AppState, a: &AuthUser, mode: usize) -> Result<(), AppError> {
        match mode {
            0 => mutate_folder_create(
                state,
                a,
                "space-a".into(),
                CreateFolderRequest {
                    name: "New".into(),
                    parent_id: None,
                    icon: None,
                    source_id: None,
                    created_at: None,
                    updated_at: None,
                },
                None,
            )
            .await
            .map(|_| ()),
            1 => {
                mutate_folder_update(
                    state,
                    a,
                    "space-a".into(),
                    "folder-a".into(),
                    UpdateFolderRequest {
                        name: Some("Renamed".into()),
                        icon: None,
                        parent_id: None,
                    },
                    None,
                )
                .await
            }
            2 => {
                mutate_folder_delete(
                    state,
                    a,
                    "space-a".into(),
                    "folder-a".into(),
                    FolderMutationOrigin::Rest,
                )
                .await
            }
            3 => move_folder(
                State(state.clone()),
                copied(a),
                Path(("space-a".into(), "folder-a".into())),
                Json(MoveFolderRequest {
                    target_space_id: "space-a".into(),
                    parent_id: Some("destination-folder".into()),
                }),
            )
            .await
            .map(|_| ()),
            _ => reorder_folder(
                State(state.clone()),
                copied(a),
                Path(("space-a".into(), "folder-a".into())),
                Json(ReorderRequest { sort_order: 15 }),
            )
            .await
            .map(|_| ()),
        }
    }
    #[tokio::test]
    async fn current_folder_manage_folders_is_exact_and_global_page_contributor_cannot_manage() {
        for mode in 0..5 {
            let state = setup(false).await;
            sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('folder-only','space-a','Folder only','[\"manage_folders\"]','owner-a');UPDATE space_members SET role='folder-only' WHERE user_id='editor-a'").execute(&state.db).await.unwrap();
            let a = proof(&state, "editor-a").await;
            write(&state, &a, mode).await.unwrap();
            let state = setup(false).await;
            sqlx::query("DELETE FROM space_members WHERE user_id='editor-a';INSERT INTO groups(id,name) VALUES('contributor-group','Contributor');INSERT INTO group_members(id,group_id,user_id) VALUES('contributor-member','contributor-group','editor-a');INSERT INTO group_global_roles(group_id,role_id) VALUES('contributor-group','contributor')").execute(&state.db).await.unwrap();
            let a = proof(&state, "editor-a").await;
            let before = move_database_snapshot(&state).await;
            assert!(write(&state, &a, mode).await.is_err());
            assert_eq!(move_database_snapshot(&state).await, before);
        }
    }
    #[tokio::test]
    async fn current_folder_http_write_only_and_read_service_keys_keep_real_method_proof() {
        for (scope, actor, status) in [
            ("write", Some("editor-a"), StatusCode::CREATED),
            ("read", Some("editor-a"), StatusCode::FORBIDDEN),
            ("admin", None, StatusCode::UNAUTHORIZED),
        ] {
            let state = setup(false).await;
            let raw = format!("folder-key-{}", uuid::Uuid::new_v4());
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes,is_service_account) VALUES($1,$2,'Folder key',$3,'test',$4,$5)").bind(&raw).bind(actor).bind(crate::auth::api_keys::hash_api_key(&raw)).bind(format!("[\"{scope}\"]")).bind(if actor.is_none(){1_i64}else{0_i64}).execute(&state.db).await.unwrap();
            let req = axum::http::Request::builder()
                .method("POST")
                .uri("/spaces/space-a/folders")
                .header("X-API-Key", raw)
                .header("content-type", "application/json")
                .body(axum::body::Body::from("{\"name\":\"HTTP folder\"}"))
                .unwrap();
            let result = router()
                .with_state(state.clone())
                .oneshot(req)
                .await
                .unwrap();
            assert_eq!(result.status(), status, "{scope}/{actor:?}");
            let count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM folders WHERE name='HTTP folder'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(count, if status == StatusCode::CREATED { 1 } else { 0 });
        }
    }
    async fn affected_page(state: &AppState) {
        sqlx::query("INSERT INTO documents(id,space_id,folder_id,title,content_path,created_by,last_updated_by,published_at) VALUES('folder-parent-page','space-a',NULL,'Parent','','owner-a','owner-a','old'),('folder-affected-page','space-a','child-a','Affected','','owner-a','owner-a','old');UPDATE documents SET parent_id='folder-parent-page' WHERE id='folder-affected-page'").execute(&state.db).await.unwrap();
    }
    #[tokio::test]
    async fn current_folder_affected_page_restrictions_private_ancestors_and_locks_deny_whole_topology(
    ) {
        for mode in [2, 3] {
            for condition in [0, 1, 2] {
                let state = setup(false).await;
                affected_page(&state).await;
                sqlx::query(match condition{
    0=>"INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('folder-parent-page','space-a','restricted','rev','owner-a')",
    1=>"UPDATE documents SET published_at=NULL WHERE id='folder-parent-page'",
    _=>"UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='folder-affected-page'",
   }).execute(&state.db).await.unwrap();
                let a = proof(&state, "editor-a").await;
                let before = move_database_snapshot(&state).await;
                assert!(write(&state, &a, mode).await.is_err());
                assert_eq!(move_database_snapshot(&state).await, before);
            }
        }
    }
    #[tokio::test]
    async fn current_folder_page_edit_is_required_for_topology_but_parent_viewer_participation_is_sufficient(
    ) {
        for mode in [2, 3] {
            let state = setup(false).await;
            affected_page(&state).await;
            sqlx::query("UPDATE documents SET published_at=NULL WHERE id='folder-parent-page';INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('folder-parent-draft','folder-parent-page','space-a','owner-a','Parent','PRIVATE','old');INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('folder-parent-draft','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
            let a = proof(&state, "editor-a").await;
            write(&state, &a, mode).await.unwrap();
            let state = setup(false).await;
            affected_page(&state).await;
            sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('folder-only','space-a','Folder only','[\"manage_folders\"]','owner-a');UPDATE space_members SET role='folder-only' WHERE user_id='editor-a'").execute(&state.db).await.unwrap();
            let a = proof(&state, "editor-a").await;
            let before = move_database_snapshot(&state).await;
            assert!(write(&state, &a, mode).await.is_err());
            assert_eq!(move_database_snapshot(&state).await, before);
        }
    }
    #[tokio::test]
    async fn current_folder_after_last_write_session_and_parent_restriction_changes_roll_back() {
        for mode in [0, 1, 2, 3, 4] {
            let state = setup(false).await;
            let a = proof(&state, "editor-a").await;
            let event = if mode == 0 {
                "INSERT"
            } else if mode == 2 {
                "DELETE"
            } else {
                "UPDATE"
            };
            sqlx::query(&format!("CREATE TRIGGER folder_late_credential AFTER {event} ON folders BEGIN DELETE FROM sessions WHERE id='metadata-editor-a'; END")).execute(&state.db).await.unwrap();
            let before = move_database_snapshot(&state).await;
            assert!(write(&state, &a, mode).await.is_err());
            assert_eq!(move_database_snapshot(&state).await, before);
        }
        let state = setup(false).await;
        affected_page(&state).await;
        let a = proof(&state, "editor-a").await;
        sqlx::query("CREATE TRIGGER folder_late_restriction AFTER UPDATE OF updated_at ON documents WHEN NEW.id='folder-affected-page' BEGIN INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('folder-parent-page','space-a','restricted','late','owner-a'); END").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(write(&state, &a, 3).await.is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn current_folder_real_wal_guard_wait_rechecks_actual_session() {
        let state = setup(true).await;
        let a = proof(&state, "editor-a").await;
        let mut blocker = state.db.begin().await.unwrap();
        lock_spaces_for_mutation(&mut blocker, &["space-a"])
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE id='metadata-editor-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let copy = state.clone();
        let task = tokio::spawn(async move { write(&copy, &a, 0).await });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!task.is_finished());
        blocker.commit().await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(task.await.unwrap().is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
    }
}

#[cfg(test)]
mod folder_budget_tests {
    use super::*;
    #[test]
    fn catalog_preserves_deep_chains_and_more_than_budget_unrelated_roots() {
        let mut rows = (0..=MAX_FOLDER_OPERATION_NODES).map(|i|(format!("root-{i}"),None)).collect::<Vec<_>>();
        for i in 0..256 { rows.push((format!("chain-{i}"), if i == 0 {None} else {Some(format!("chain-{}",i-1))})); }
        validate_folder_catalog(&rows).unwrap();
        rows.push(("orphan".into(),Some("foreign-or-missing".into())));
        assert!(validate_folder_catalog(&rows).is_err());
        rows.pop();
        rows.push(("cycle-a".into(),Some("cycle-b".into())));
        rows.push(("cycle-b".into(),Some("cycle-a".into())));
        assert!(validate_folder_catalog(&rows).is_err());
    }
    #[test]
    fn catalog_ancestry_budget_counts_memoized_parent_chains() {
        let mut rows = (0..MAX_FOLDER_OPERATION_NODES).map(|i|(format!("chain-{i}"), if i == 0 {None} else {Some(format!("chain-{}",i-1))})).collect::<Vec<_>>();
        validate_folder_catalog(&rows).unwrap();
        rows.push(("too-many".into(),Some(format!("chain-{}",MAX_FOLDER_OPERATION_NODES-1))));
        assert!(validate_folder_catalog(&rows).is_err());
    }
    #[test]
    fn unique_folder_budget_accepts_boundary_and_refuses_next_node() {
        let mut nodes = FolderNodes::new();
        for i in 0..MAX_FOLDER_OPERATION_NODES { admit_folder_node(&mut nodes, &format!("folder-{i}")).unwrap(); }
        for i in 0..MAX_FOLDER_OPERATION_NODES { admit_folder_node(&mut nodes, &format!("folder-{i}")).unwrap(); }
        assert_eq!(nodes.len(), MAX_FOLDER_OPERATION_NODES);
        assert!(admit_folder_node(&mut nodes, "one-too-many").is_err());
    }
}
