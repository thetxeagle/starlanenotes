use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::middleware::{AdminUser, AuthUser};
use crate::auth::routes::AppError;
use crate::storage;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // Spaces
        .route("/spaces", get(list_spaces).post(create_space))
        .route(
            "/spaces/{space_id}",
            get(get_space)
                .put(update_space)
                .delete(delete_space),
        )
        .route("/spaces/{space_id}/members", get(list_members).post(add_member))
        .route(
            "/spaces/{space_id}/members/{user_id}",
            axum::routing::put(update_member_role)
                .delete(remove_member),
        )
        .route("/spaces/{space_id}/groups", get(list_space_groups).post(grant_group_to_space))
        .route(
            "/spaces/{space_id}/groups/{group_id}",
            axum::routing::put(update_space_group_role)
                .delete(remove_group_from_space),
        )
        .route("/admin/groups", get(list_groups).post(create_group))
        .route("/admin/groups/{group_id}/members", get(list_group_members).post(add_group_member))
        .route(
            "/admin/groups/{group_id}/members/{user_id}",
            axum::routing::delete(remove_group_member),
        )
        // Folders (nested under spaces)
        .merge(super::folders::router())
        // Documents (nested under spaces)
        .merge(super::documents::router())
}

// ============================================================
// Types
// ============================================================

#[derive(Deserialize)]
pub struct CreateSpaceRequest {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    pub visibility: Option<String>,
    pub source_id: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateSpaceRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    pub visibility: Option<String>,
}

#[derive(Serialize)]
pub struct SpaceResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub accent_color: Option<String>,
    pub visibility: String,
    pub member_role: Option<String>,
    pub created_by: String,
    pub owner_username: String,
    pub owner_display_name: String,
    pub created_at: String,
    pub updated_at: String,
}

pub(crate) fn import_id_or_uuid(source_id: Option<&str>) -> Result<String, AppError> {
    let Some(source_id) = source_id else {
        return Ok(Uuid::new_v4().to_string());
    };
    let trimmed = source_id.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(
            "Import source id must not be empty".into(),
        ));
    }
    if trimmed.len() > 128 {
        return Err(AppError::BadRequest(
            "Import source id must be at most 128 characters".into(),
        ));
    }
    if !trimmed
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AppError::BadRequest(
            "Import source id may only contain letters, numbers, hyphens, and underscores".into(),
        ));
    }
    Ok(trimmed.to_string())
}

pub(crate) fn import_timestamp_or_now(value: Option<&str>) -> Result<String, AppError> {
    import_timestamp_or_default(value, &chrono::Utc::now().to_rfc3339())
}

pub(crate) fn import_timestamp_or_default(
    value: Option<&str>,
    default: &str,
) -> Result<String, AppError> {
    let Some(value) = value else {
        return Ok(default.to_string());
    };
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&chrono::Utc).to_rfc3339())
        .map_err(|_| AppError::BadRequest("Import timestamp must be RFC3339".into()))
}

#[derive(Deserialize)]
pub struct AddMemberRequest {
    pub user_id: String,
    pub role: Option<String>,
}

#[derive(Serialize)]
pub struct MemberResponse {
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub role: String,
    pub joined_at: String,
}

#[derive(Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct AddGroupMemberRequest {
    pub user_id: String,
}

#[derive(Deserialize)]
pub struct SpaceGroupRequest {
    pub group_id: String,
    pub role: Option<String>,
}

#[derive(Serialize)]
pub struct GroupResponse {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub member_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct GroupMemberResponse {
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Serialize)]
pub struct SpaceGroupResponse {
    pub group_id: String,
    pub name: String,
    pub description: Option<String>,
    pub role: String,
    pub member_count: i64,
    pub granted_at: String,
}

// ============================================================
// Space Handlers
// ============================================================

/// GET /api/v1/spaces
async fn list_spaces(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let spaces: Vec<(String, String, String, Option<String>, Option<String>, Option<String>, String, Option<String>, Option<String>, String, String, String, String, String)> =
        sqlx::query_as(
            "SELECT s.id, s.name, s.slug, s.description, s.icon, s.accent_color, s.visibility, \
             sm.role, \
             (SELECT sgp.role FROM space_group_permissions sgp \
              JOIN group_members gm ON gm.group_id = sgp.group_id \
              WHERE sgp.space_id = s.id AND gm.user_id = ?1 \
              ORDER BY CASE sgp.role WHEN 'owner' THEN 1 WHEN 'editor' THEN 2 ELSE 3 END LIMIT 1), \
             s.created_by, COALESCE(NULLIF(owner.username, ''), owner.email), owner.display_name, s.created_at, s.updated_at \
             FROM spaces s \
             JOIN users owner ON owner.id = s.created_by \
             LEFT JOIN space_members sm ON s.id = sm.space_id AND sm.user_id = ?1 \
             WHERE sm.user_id IS NOT NULL \
                OR EXISTS (SELECT 1 FROM space_group_permissions sgp JOIN group_members gm ON gm.group_id = sgp.group_id WHERE sgp.space_id = s.id AND gm.user_id = ?1) \
                OR s.visibility IN ('internal', 'public') \
             ORDER BY s.name",
        )
        .bind(&auth.claims.sub)
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?;

    let result: Vec<SpaceResponse> = spaces
        .into_iter()
        .map(
            |(
                id,
                name,
                slug,
                desc,
                icon,
                color,
                vis,
                direct_role,
                group_role,
                created_by,
                owner_username,
                owner_display_name,
                created,
                updated,
            )| SpaceResponse {
                id,
                name,
                slug,
                description: desc,
                icon,
                accent_color: color,
                visibility: vis,
                member_role: direct_role.or(group_role),
                created_by,
                owner_username,
                owner_display_name,
                created_at: created,
                updated_at: updated,
            },
        )
        .collect();

    Ok(Json(result))
}

async fn current_user_display_name(state: &AppState, user_id: &str) -> Result<String, AppError> {
    let display_name: Option<(String,)> =
        sqlx::query_as("SELECT display_name FROM users WHERE id = ?1")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    display_name
        .map(|(display_name,)| display_name)
        .ok_or_else(|| AppError::BadRequest("Space owner was not found".into()))
}

async fn current_user_owner_username(state: &AppState, user_id: &str) -> Result<String, AppError> {
    let email: Option<(String,)> =
        sqlx::query_as("SELECT COALESCE(NULLIF(username, ''), email) FROM users WHERE id = ?1")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    email
        .map(|row| row.0)
        .ok_or_else(|| AppError::BadRequest("Space owner not found".into()))
}

/// POST /api/v1/spaces
async fn create_space(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(req): Json<CreateSpaceRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.name.is_empty() {
        return Err(AppError::BadRequest("Name is required".into()));
    }

    if req.name.len() > 100 {
        return Err(AppError::BadRequest(
            "Space name must be at most 100 characters".into(),
        ));
    }

    if let Some(ref desc) = req.description {
        if desc.len() > 1000 {
            return Err(AppError::BadRequest(
                "Space description must be at most 1000 characters".into(),
            ));
        }
    }

    let slug = storage::sanitize_slug(&req.name);
    if slug.is_empty() {
        return Err(AppError::BadRequest(
            "Name must contain alphanumeric characters".into(),
        ));
    }

    // Check slug uniqueness
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM spaces WHERE slug = ?1")
        .bind(&slug)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;

    if existing.is_some() {
        return Err(AppError::Conflict(
            "A space with this name already exists".into(),
        ));
    }

    let space_id = import_id_or_uuid(req.source_id.as_deref())?;
    let created_at = import_timestamp_or_now(req.created_at.as_deref())?;
    let updated_at = import_timestamp_or_default(req.updated_at.as_deref(), &created_at)?;
    let visibility = req.visibility.as_deref().unwrap_or("private");

    let existing_id: Option<(String,)> = sqlx::query_as("SELECT id FROM spaces WHERE id = ?1")
        .bind(&space_id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;
    if existing_id.is_some() {
        return Err(AppError::Conflict(
            "A space with this import id already exists".into(),
        ));
    }

    sqlx::query(
        "INSERT INTO spaces (id, name, slug, description, icon, accent_color, visibility, created_by, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(&space_id)
    .bind(&req.name)
    .bind(&slug)
    .bind(&req.description)
    .bind(&req.icon)
    .bind(&req.accent_color)
    .bind(visibility)
    .bind(&admin.claims.sub)
    .bind(&created_at)
    .bind(&updated_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    // Add creator as owner
    let member_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO space_members (id, space_id, user_id, role) VALUES (?1, ?2, ?3, 'owner')",
    )
    .bind(&member_id)
    .bind(&space_id)
    .bind(&admin.claims.sub)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    // Create directory structure on disk
    storage::init_space_dirs(&slug)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create space directory: {}", e)))?;

    // --- Wire: Webhook + Audit ---
    crate::webhooks::dispatch::dispatch_event(
        &state.db, "space.created",
        &serde_json::json!({ "space_id": space_id, "name": req.name, "created_by": admin.claims.sub }),
        Some(&space_id),
    ).await;
    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "space.create",
        Some("space"),
        Some(&space_id),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(SpaceResponse {
            id: space_id,
            name: req.name,
            slug,
            description: req.description,
            icon: req.icon,
            accent_color: req.accent_color,
            visibility: visibility.to_string(),
            member_role: Some("owner".to_string()),
            created_by: admin.claims.sub.clone(),
            owner_username: current_user_owner_username(&state, &admin.claims.sub).await?,
            owner_display_name: current_user_display_name(&state, &admin.claims.sub).await?,
            created_at,
            updated_at,
        }),
    ))
}

/// GET /api/v1/spaces/{space_id}
async fn get_space(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let space: Option<(
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT s.id, s.name, s.slug, s.description, s.icon, s.accent_color, s.visibility, \
             s.created_by, owner.email, owner.display_name, s.created_at, s.updated_at \
             FROM spaces s JOIN users owner ON owner.id = s.created_by WHERE s.id = ?1",
    )
    .bind(&space_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    let (
        id,
        name,
        slug,
        desc,
        icon,
        color,
        vis,
        created_by,
        owner_username,
        owner_display_name,
        created,
        updated,
    ) = space.ok_or_else(|| AppError::BadRequest("Space not found".into()))?;

    // Get member role
    let member_role: Option<(String,)> =
        sqlx::query_as("SELECT role FROM space_members WHERE space_id = ?1 AND user_id = ?2")
            .bind(&space_id)
            .bind(&auth.claims.sub)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    if vis == "private" && member_role.is_none() {
        require_space_role(
            &state,
            &space_id,
            &auth.claims.sub,
            &["owner", "editor", "viewer"],
        )
        .await?;
    }

    Ok(Json(SpaceResponse {
        id,
        name,
        slug,
        description: desc,
        icon,
        accent_color: color,
        visibility: vis,
        member_role: member_role.map(|r| r.0),
        created_by,
        owner_username,
        owner_display_name,
        created_at: created,
        updated_at: updated,
    }))
}

/// PUT /api/v1/spaces/{space_id}
async fn update_space(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<UpdateSpaceRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Verify user is owner or admin
    require_space_role(&state, &space_id, &auth.claims.sub, &["owner"]).await?;

    if let Some(ref name) = req.name {
        if name.len() > 100 {
            return Err(AppError::BadRequest(
                "Space name must be at most 100 characters".into(),
            ));
        }
    }

    if let Some(ref desc) = req.description {
        if desc.len() > 1000 {
            return Err(AppError::BadRequest(
                "Space description must be at most 1000 characters".into(),
            ));
        }
    }

    let updated_at = chrono::Utc::now().to_rfc3339();

    if let Some(ref name) = req.name {
        sqlx::query("UPDATE spaces SET name = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(name)
            .bind(&updated_at)
            .bind(&space_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    if let Some(ref desc) = req.description {
        sqlx::query("UPDATE spaces SET description = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(desc)
            .bind(&updated_at)
            .bind(&space_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    if let Some(ref icon) = req.icon {
        sqlx::query("UPDATE spaces SET icon = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(icon)
            .bind(&updated_at)
            .bind(&space_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    if let Some(ref color) = req.accent_color {
        sqlx::query("UPDATE spaces SET accent_color = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(color)
            .bind(&updated_at)
            .bind(&space_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    if let Some(ref vis) = req.visibility {
        sqlx::query("UPDATE spaces SET visibility = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(vis)
            .bind(&updated_at)
            .bind(&space_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    crate::webhooks::dispatch::dispatch_event(
        &state.db,
        "space.updated",
        &serde_json::json!({ "space_id": space_id, "updated_by": auth.claims.sub }),
        Some(&space_id),
    )
    .await;
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "space.update",
        Some("space"),
        Some(&space_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/v1/spaces/{space_id}
async fn delete_space(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    require_space_role(&state, &space_id, &auth.claims.sub, &["owner"]).await?;

    // Get slug for disk cleanup
    let slug: (String,) = sqlx::query_as("SELECT slug FROM spaces WHERE id = ?1")
        .bind(&space_id)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::internal)?;

    // Delete from DB (cascades to members, folders, documents, etc.)
    sqlx::query("DELETE FROM spaces WHERE id = ?1")
        .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    // Delete from disk
    storage::delete_space_dir(&slug.0)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to delete space directory: {}", e)))?;

    crate::webhooks::dispatch::dispatch_event(
        &state.db,
        "space.deleted",
        &serde_json::json!({ "space_id": space_id, "deleted_by": auth.claims.sub }),
        None,
    )
    .await;
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "space.delete",
        Some("space"),
        Some(&space_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================
// Member Handlers
// ============================================================

/// GET /api/v1/spaces/{space_id}/members
async fn list_members(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    require_space_role(
        &state,
        &space_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;

    let members: Vec<(String, String, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT u.id, u.email, u.display_name, u.avatar_url, sm.role, sm.created_at \
         FROM space_members sm \
         JOIN users u ON sm.user_id = u.id \
         WHERE sm.space_id = ?1 \
         ORDER BY sm.role, u.display_name",
    )
    .bind(&space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<MemberResponse> = members
        .into_iter()
        .map(|(uid, email, name, avatar, role, joined)| MemberResponse {
            user_id: uid,
            email,
            display_name: name,
            avatar_url: avatar,
            role,
            joined_at: joined,
        })
        .collect();

    Ok(Json(result))
}

/// POST /api/v1/spaces/{space_id}/members
async fn add_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<AddMemberRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"]).await?;

    let role = req.role.as_deref().unwrap_or("editor");
    let member_id = Uuid::new_v4().to_string();

    sqlx::query("INSERT INTO space_members (id, space_id, user_id, role) VALUES (?1, ?2, ?3, ?4)")
        .bind(&member_id)
        .bind(&space_id)
        .bind(&req.user_id)
        .bind(role)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "space.member_add",
        Some("space"),
        Some(&space_id),
    )
    .await;

    Ok(StatusCode::CREATED)
}

/// PUT /api/v1/spaces/{space_id}/members/{user_id}
async fn update_member_role(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, user_id)): Path<(String, String)>,
    Json(req): Json<AddMemberRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_space_role(&state, &space_id, &auth.claims.sub, &["owner"]).await?;

    let role = req.role.as_deref().unwrap_or("editor");

    sqlx::query("UPDATE space_members SET role = ?1 WHERE space_id = ?2 AND user_id = ?3")
        .bind(role)
        .bind(&space_id)
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "space.member_role_update",
        Some("space"),
        Some(&space_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/v1/spaces/{space_id}/members/{user_id}
async fn remove_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, user_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    require_space_role(&state, &space_id, &auth.claims.sub, &["owner"]).await?;

    sqlx::query("DELETE FROM space_members WHERE space_id = ?1 AND user_id = ?2")
        .bind(&space_id)
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "space.member_remove",
        Some("space"),
        Some(&space_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================
// Group Handlers
// ============================================================

/// GET /api/v1/admin/groups
async fn list_groups(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, AppError> {
    let groups: Vec<(String, String, Option<String>, i64, String, String)> = sqlx::query_as(
        "SELECT g.id, g.name, g.description, COUNT(gm.user_id) AS member_count, g.created_at, g.updated_at \
         FROM groups g \
         LEFT JOIN group_members gm ON gm.group_id = g.id \
         GROUP BY g.id, g.name, g.description, g.created_at, g.updated_at \
         ORDER BY g.name",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<GroupResponse> = groups
        .into_iter()
        .map(
            |(id, name, description, member_count, created_at, updated_at)| GroupResponse {
                id,
                name,
                description,
                member_count,
                created_at,
                updated_at,
            },
        )
        .collect();

    Ok(Json(result))
}

/// POST /api/v1/admin/groups
async fn create_group(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(req): Json<CreateGroupRequest>,
) -> Result<impl IntoResponse, AppError> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("Group name is required".into()));
    }
    if name.len() > 100 {
        return Err(AppError::BadRequest(
            "Group name must be at most 100 characters".into(),
        ));
    }

    let group_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO groups (id, name, description) VALUES (?1, ?2, ?3)")
        .bind(&group_id)
        .bind(name)
        .bind(&req.description)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "group.create",
        Some("group"),
        Some(&group_id),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(GroupResponse {
            id: group_id,
            name: name.to_string(),
            description: req.description,
            member_count: 0,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }),
    ))
}

/// GET /api/v1/admin/groups/{group_id}/members
async fn list_group_members(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(group_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let members: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT u.id, u.email, u.display_name, u.role, gm.created_at \
         FROM group_members gm \
         JOIN users u ON u.id = gm.user_id \
         WHERE gm.group_id = ?1 \
         ORDER BY u.display_name",
    )
    .bind(&group_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<GroupMemberResponse> = members
        .into_iter()
        .map(
            |(user_id, email, display_name, role, joined_at)| GroupMemberResponse {
                user_id,
                email,
                display_name,
                role,
                joined_at,
            },
        )
        .collect();

    Ok(Json(result))
}

/// POST /api/v1/admin/groups/{group_id}/members
async fn add_group_member(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(group_id): Path<String>,
    Json(req): Json<AddGroupMemberRequest>,
) -> Result<impl IntoResponse, AppError> {
    let member_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO group_members (id, group_id, user_id) VALUES (?1, ?2, ?3) \
         ON CONFLICT(group_id, user_id) DO NOTHING",
    )
    .bind(&member_id)
    .bind(&group_id)
    .bind(&req.user_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "group.member_add",
        Some("group"),
        Some(&group_id),
    )
    .await;

    Ok(StatusCode::CREATED)
}

/// DELETE /api/v1/admin/groups/{group_id}/members/{user_id}
async fn remove_group_member(
    State(state): State<AppState>,
    admin: AdminUser,
    Path((group_id, user_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    sqlx::query("DELETE FROM group_members WHERE group_id = ?1 AND user_id = ?2")
        .bind(&group_id)
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "group.member_remove",
        Some("group"),
        Some(&group_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/spaces/{space_id}/groups
async fn list_space_groups(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    require_space_role(&state, &space_id, &auth.claims.sub, &["owner"]).await?;

    let groups: Vec<(String, String, Option<String>, String, i64, String)> = sqlx::query_as(
        "SELECT g.id, g.name, g.description, sgp.role, COUNT(gm.user_id) AS member_count, sgp.created_at \
         FROM space_group_permissions sgp \
         JOIN groups g ON g.id = sgp.group_id \
         LEFT JOIN group_members gm ON gm.group_id = g.id \
         WHERE sgp.space_id = ?1 \
         GROUP BY g.id, g.name, g.description, sgp.role, sgp.created_at \
         ORDER BY g.name",
    )
    .bind(&space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<SpaceGroupResponse> = groups
        .into_iter()
        .map(
            |(group_id, name, description, role, member_count, granted_at)| SpaceGroupResponse {
                group_id,
                name,
                description,
                role,
                member_count,
                granted_at,
            },
        )
        .collect();

    Ok(Json(result))
}

/// POST /api/v1/spaces/{space_id}/groups
async fn grant_group_to_space(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<SpaceGroupRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_space_role(&state, &space_id, &auth.claims.sub, &["owner"]).await?;

    let role = validate_space_role(req.role.as_deref().unwrap_or("viewer"))?;
    let grant_id = Uuid::new_v4().to_string();
    let updated_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO space_group_permissions (id, space_id, group_id, role) VALUES (?1, ?2, ?3, ?4) \
         ON CONFLICT(space_id, group_id) DO UPDATE SET role = excluded.role, updated_at = ?5",
    )
    .bind(&grant_id)
    .bind(&space_id)
    .bind(&req.group_id)
    .bind(role)
    .bind(&updated_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "space.group_grant",
        Some("space"),
        Some(&space_id),
    )
    .await;

    Ok(StatusCode::CREATED)
}

/// PUT /api/v1/spaces/{space_id}/groups/{group_id}
async fn update_space_group_role(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, group_id)): Path<(String, String)>,
    Json(req): Json<SpaceGroupRequest>,
) -> Result<impl IntoResponse, AppError> {
    require_space_role(&state, &space_id, &auth.claims.sub, &["owner"]).await?;

    let role = validate_space_role(req.role.as_deref().unwrap_or("viewer"))?;
    let updated_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE space_group_permissions SET role = ?1, updated_at = ?2 \
         WHERE space_id = ?3 AND group_id = ?4",
    )
    .bind(role)
    .bind(&updated_at)
    .bind(&space_id)
    .bind(&group_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "space.group_role_update",
        Some("space"),
        Some(&space_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/v1/spaces/{space_id}/groups/{group_id}
async fn remove_group_from_space(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, group_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    require_space_role(&state, &space_id, &auth.claims.sub, &["owner"]).await?;

    sqlx::query("DELETE FROM space_group_permissions WHERE space_id = ?1 AND group_id = ?2")
        .bind(&space_id)
        .bind(&group_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "space.group_remove",
        Some("space"),
        Some(&space_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================
// Auth Helpers
// ============================================================

fn validate_space_role(role: &str) -> Result<&str, AppError> {
    match role {
        "owner" | "editor" | "viewer" => Ok(role),
        _ => Err(AppError::BadRequest("Invalid space role".into())),
    }
}

fn role_allows(role: &str, allowed_roles: &[&str]) -> bool {
    allowed_roles.contains(&role)
}

/// Verify the user has one of the required roles in a space (or is admin)
pub async fn require_space_role(
    state: &AppState,
    space_id: &str,
    user_id: &str,
    allowed_roles: &[&str],
) -> Result<String, AppError> {
    // Check if user is system admin
    let user_role: Option<(String,)> = sqlx::query_as("SELECT role FROM users WHERE id = ?1")
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;

    if let Some((role,)) = &user_role {
        if role == "admin" {
            return Ok("admin".to_string());
        }
    }

    // Check space membership
    let member: Option<(String,)> =
        sqlx::query_as("SELECT role FROM space_members WHERE space_id = ?1 AND user_id = ?2")
            .bind(space_id)
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    match member {
        Some((role,)) if role_allows(&role, allowed_roles) => Ok(role),
        Some(_) => Err(AppError::Forbidden("Insufficient permissions".into())),
        None => {
            let group_role: Option<(String,)> = sqlx::query_as(
                "SELECT sgp.role FROM space_group_permissions sgp \
                 JOIN group_members gm ON gm.group_id = sgp.group_id \
                 WHERE sgp.space_id = ?1 AND gm.user_id = ?2 \
                 ORDER BY CASE sgp.role WHEN 'owner' THEN 1 WHEN 'editor' THEN 2 ELSE 3 END LIMIT 1",
            )
            .bind(space_id)
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

            match group_role {
                Some((role,)) if role_allows(&role, allowed_roles) => Ok(role),
                Some(_) => Err(AppError::Forbidden("Insufficient permissions".into())),
                None => Err(AppError::Forbidden("Not a member of this space".into())),
            }
        }
    }
}
