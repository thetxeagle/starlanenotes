use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::auth::middleware::{AdminUser, AuthUser};
use crate::auth::routes::AppError;
use crate::storage;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // Spaces
        .route("/spaces", get(list_spaces).post(create_space))
        .route("/spaces/role-definitions", get(list_space_role_definitions))
        .route(
            "/spaces/{space_id}",
            get(get_space)
                .put(update_space)
                .delete(delete_space),
        )
        .route(
            "/spaces/{space_id}/retention",
            get(get_space_retention).put(update_space_retention),
        )
        .route("/spaces/{space_id}/members", get(list_members).post(add_member))
        .route("/spaces/{space_id}/access-users", get(list_effective_space_users))
        .route("/spaces/{space_id}/principals", get(list_space_principals))
        .route(
            "/spaces/{space_id}/members/{user_id}",
            axum::routing::put(update_member_role)
                .delete(remove_member),
        )
        .route(
            "/spaces/{space_id}/blocks/{user_id}",
            axum::routing::put(block_space_user)
                .delete(unblock_space_user),
        )
        .route(
            "/spaces/{space_id}/roles",
            get(list_space_roles).post(create_space_role),
        )
        .route(
            "/spaces/{space_id}/roles/{role_id}",
            axum::routing::put(update_space_role)
                .delete(delete_space_role),
        )
        .route("/spaces/{space_id}/groups", get(list_space_groups).post(grant_group_to_space))
        .route(
            "/spaces/{space_id}/groups/{group_id}",
            axum::routing::put(update_space_group_role)
                .delete(remove_group_from_space),
        )
        .route(
            "/spaces/{space_id}/groups/{group_id}/members",
            get(list_space_group_members).post(add_space_group_member),
        )
        .route(
            "/spaces/{space_id}/groups/{group_id}/members/{user_id}",
            axum::routing::delete(remove_space_group_member),
        )
        .route("/admin/groups", get(list_groups).post(create_group))
        .route("/admin/roles", get(list_global_roles))
        .route(
            "/admin/groups/{group_id}",
            axum::routing::put(update_group).delete(delete_group),
        )
        .route("/admin/groups/{group_id}/spaces", get(list_group_space_grants))
        .route("/admin/groups/{group_id}/members", get(list_group_members).post(add_group_member))
        .route(
            "/admin/groups/{group_id}/members/{user_id}",
            axum::routing::delete(remove_group_member),
        )
        .route(
            "/admin/groups/{group_id}/roles",
            get(list_group_global_roles).post(assign_group_global_role),
        )
        .route(
            "/admin/groups/{group_id}/roles/{role_id}",
            axum::routing::delete(remove_group_global_role),
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
pub struct SpaceRetentionResponse {
    pub trash_retention_days: i32,
}

#[derive(Deserialize)]
pub struct UpdateSpaceRetentionRequest {
    pub trash_retention_days: i32,
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

#[derive(Serialize)]
pub struct EffectiveAccessSource {
    pub kind: &'static str,
    pub id: String,
    pub name: String,
    pub role: String,
}

#[derive(Serialize)]
pub struct EffectiveSpaceUserResponse {
    pub user_id: String,
    pub email: String,
    pub display_name: String,
    pub effective_role: Option<String>,
    pub direct_role: Option<String>,
    pub blocked: bool,
    pub sources: Vec<EffectiveAccessSource>,
}

#[derive(Deserialize)]
pub struct SaveSpaceRoleRequest {
    pub name: String,
    pub description: Option<String>,
    pub permissions: Vec<SpacePermission>,
}

#[derive(Serialize)]
pub struct StoredSpaceRoleDefinition {
    pub id: String,
    pub name: String,
    pub description: String,
    pub permissions: Vec<SpacePermission>,
    pub built_in: bool,
}

#[derive(Deserialize)]
pub struct CreateGroupRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateGroupRequest {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Deserialize)]
pub struct AddGroupMemberRequest {
    pub user_id: String,
}

#[derive(Deserialize)]
pub struct AssignGroupRoleRequest {
    pub role_id: String,
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
    pub space_grant_count: i64,
    pub scim_managed: bool,
    pub managed_space_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct GroupSpaceGrantResponse {
    pub space_id: String,
    pub name: String,
    pub slug: String,
    pub role: String,
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
pub struct GlobalRoleResponse {
    pub id: String,
    pub name: String,
    pub description: String,
    pub permissions: Vec<String>,
    pub built_in: bool,
}

#[derive(Serialize)]
pub struct SpaceGroupResponse {
    pub group_id: String,
    pub name: String,
    pub description: Option<String>,
    pub role: String,
    pub member_count: i64,
    pub granted_at: String,
    pub managed: bool,
}

#[derive(Serialize)]
pub struct SpacePrincipalUser {
    pub id: String,
    pub email: String,
    pub display_name: String,
}

#[derive(Serialize)]
pub struct SpacePrincipalGroup {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub member_count: i64,
    pub managed_space_id: Option<String>,
}

#[derive(Serialize)]
pub struct SpacePrincipalCatalog {
    pub users: Vec<SpacePrincipalUser>,
    pub groups: Vec<SpacePrincipalGroup>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpacePermission {
    View,
    Comment,
    EditContent,
    ManageMembers,
    ManageSpace,
}

#[derive(Serialize)]
pub struct SpaceRoleDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub permissions: &'static [SpacePermission],
}

const VIEWER_PERMISSIONS: &[SpacePermission] = &[SpacePermission::View];
const COMMENTER_PERMISSIONS: &[SpacePermission] =
    &[SpacePermission::View, SpacePermission::Comment];
const EDITOR_PERMISSIONS: &[SpacePermission] = &[
    SpacePermission::View,
    SpacePermission::Comment,
    SpacePermission::EditContent,
];
const SPACE_ADMIN_PERMISSIONS: &[SpacePermission] = &[
    SpacePermission::View,
    SpacePermission::Comment,
    SpacePermission::EditContent,
    SpacePermission::ManageMembers,
    SpacePermission::ManageSpace,
];

pub const SPACE_ROLE_DEFINITIONS: &[SpaceRoleDefinition] = &[
    SpaceRoleDefinition {
        id: "space_admin",
        name: "Space admin",
        description: "Manage space settings, access, and content.",
        permissions: SPACE_ADMIN_PERMISSIONS,
    },
    SpaceRoleDefinition {
        id: "editor",
        name: "Editor",
        description: "Create, edit, organize, and comment on content.",
        permissions: EDITOR_PERMISSIONS,
    },
    SpaceRoleDefinition {
        id: "commenter",
        name: "Commenter",
        description: "View content and participate in comments.",
        permissions: COMMENTER_PERMISSIONS,
    },
    SpaceRoleDefinition {
        id: "viewer",
        name: "Viewer",
        description: "View space content without making changes.",
        permissions: VIEWER_PERMISSIONS,
    },
];

async fn list_space_role_definitions(_auth: AuthUser) -> Result<impl IntoResponse, AppError> {
    Ok(Json(SPACE_ROLE_DEFINITIONS))
}

fn built_in_space_roles() -> Vec<StoredSpaceRoleDefinition> {
    SPACE_ROLE_DEFINITIONS
        .iter()
        .map(|definition| StoredSpaceRoleDefinition {
            id: definition.id.to_string(),
            name: definition.name.to_string(),
            description: definition.description.to_string(),
            permissions: definition.permissions.to_vec(),
            built_in: true,
        })
        .collect()
}

async fn list_space_roles(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_space_groups_read(&mut conn, &auth, &space_id).await?;
    let stored: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT id, name, description, permissions FROM space_roles \
         WHERE space_id = $1 ORDER BY lower(name)",
    )
    .bind(&space_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let mut roles = built_in_space_roles();
    for (id, name, description, permissions) in stored {
        roles.push(StoredSpaceRoleDefinition {
            id,
            name,
            description,
            permissions: serde_json::from_str(&permissions).map_err(AppError::internal)?,
            built_in: false,
        });
    }
    #[cfg(test)]
    catalog_after_load().await;
    require_space_groups_read(&mut conn, &auth, &space_id).await?;
    super::access::require_current_human_read(&mut conn, &auth).await?;
    Ok(Json(roles))
}

fn validate_role_definition(req: &SaveSpaceRoleRequest) -> Result<(), AppError> {
    let name = req.name.trim();
    if name.is_empty() || name.len() > 80 {
        return Err(AppError::BadRequest(
            "Role name must be between 1 and 80 characters".into(),
        ));
    }
    if req.description.as_deref().unwrap_or_default().len() > 240 {
        return Err(AppError::BadRequest(
            "Role description must be at most 240 characters".into(),
        ));
    }
    if !req.permissions.contains(&SpacePermission::View) {
        return Err(AppError::BadRequest(
            "Space roles must include view permission".into(),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CustomSpaceRoleSource {
    role: Option<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )>,
    members: Vec<(String, String, String, String, String)>,
    grants: Vec<(String, String, String, String, String, String)>,
}
async fn custom_space_role_source(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    role: &str,
) -> Result<CustomSpaceRoleSource, AppError> {
    let row=sqlx::query_as("SELECT id,space_id,name,description,permissions,created_by,created_at,updated_at FROM space_roles WHERE id=$1 AND space_id=$2").bind(role).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let members:Vec<(String,String,String,String,String)>=sqlx::query_as("SELECT id,space_id,user_id,role,created_at FROM space_members WHERE role=$1 AND (space_id=$2 OR EXISTS(SELECT 1 FROM space_roles WHERE id=$1 AND space_id=$2)) ORDER BY id").bind(role).bind(space).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let grants:Vec<(String,String,String,String,String,String)>=sqlx::query_as("SELECT id,space_id,group_id,role,created_at,updated_at FROM space_group_permissions WHERE role=$1 AND (space_id=$2 OR EXISTS(SELECT 1 FROM space_roles WHERE id=$1 AND space_id=$2)) ORDER BY id").bind(role).bind(space).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    if members.iter().any(|m| m.1 != space) || grants.iter().any(|g| g.1 != space) {
        return Err(AppError::Conflict("Space role source unavailable".into()));
    }
    if row.is_none() && (!members.is_empty() || !grants.is_empty()) {
        return Err(AppError::Conflict("Space role source unavailable".into()));
    }
    let orphan_consumers:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM space_members m LEFT JOIN users u ON u.id=m.user_id WHERE m.space_id=$1 AND m.role=$2 AND u.id IS NULL) + (SELECT COUNT(*) FROM space_group_permissions p LEFT JOIN groups g ON g.id=p.group_id WHERE p.space_id=$1 AND p.role=$2 AND g.id IS NULL)").bind(space).bind(role).fetch_one(&mut *conn).await.map_err(AppError::internal)?;
    if orphan_consumers != 0 {
        return Err(AppError::Conflict("Space role source unavailable".into()));
    }
    Ok(CustomSpaceRoleSource {
        role: row,
        members,
        grants,
    })
}
enum CustomSpaceRoleChange<'a> {
    Create(&'a SaveSpaceRoleRequest),
    Update(&'a SaveSpaceRoleRequest),
    Delete,
}
impl CustomSpaceRoleChange<'_> {
    fn request(&self) -> Option<&SaveSpaceRoleRequest> {
        match self {
            Self::Create(r) | Self::Update(r) => Some(r),
            Self::Delete => None,
        }
    }
}
#[cfg(test)]
static CUSTOM_ROLE_FINAL_EXPIRY: std::sync::Mutex<Option<i64>> = std::sync::Mutex::new(None);
#[cfg(test)]
async fn custom_role_final_credential_pause(role: &str) {
    if role != "custom:expiry" {
        return;
    }
    let expiry = CUSTOM_ROLE_FINAL_EXPIRY.lock().unwrap().take();
    if let Some(expiry) = expiry {
        while chrono::Utc::now().timestamp() <= expiry {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }
}
async fn mutate_custom_space_role(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    role: &str,
    change: CustomSpaceRoleChange<'_>,
) -> Result<String, AppError> {
    use super::{access, mutation_guards};
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    mutation_guards::lock_spaces_for_mutation(&mut tx, &[space])
        .await
        .map_err(|e| match e {
            AppError::NotFound(_) => AppError::NotFound("Target unavailable".into()),
            v => v,
        })?;
    access::require_space_membership_management(&mut tx, auth, space).await?;
    if let Some(req) = change.request() {
        validate_role_definition(req)?;
    }
    sqlx::query("UPDATE space_roles SET id=id WHERE id=$1 AND space_id=$2")
        .bind(role)
        .bind(space)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    let source = custom_space_role_source(&mut tx, space, role).await?;
    if !matches!(change, CustomSpaceRoleChange::Create(_)) && source.role.is_none() {
        return Err(AppError::NotFound("Space role not found".into()));
    }
    if matches!(change, CustomSpaceRoleChange::Create(_)) && source.role.is_some() {
        return Err(AppError::Conflict("Space role already exists".into()));
    }
    if let Some(r) = &source.role {
        access::space_custom_role_permissions(&r.4)?;
    }
    for m in &source.members {
        sqlx::query("UPDATE space_members SET id=id WHERE id=$1 AND space_id=$2 AND role=$3")
            .bind(&m.0)
            .bind(space)
            .bind(role)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    }
    for g in &source.grants {
        sqlx::query(
            "UPDATE space_group_permissions SET id=id WHERE id=$1 AND space_id=$2 AND role=$3",
        )
        .bind(&g.0)
        .bind(space)
        .bind(role)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    }
    let mut expected = custom_space_role_source(&mut tx, space, role).await?;
    if expected != source {
        return Err(AppError::Conflict("Space role source changed".into()));
    }
    if matches!(change, CustomSpaceRoleChange::Delete)
        && (!source.members.is_empty() || !source.grants.is_empty())
    {
        return Err(AppError::Conflict(
            "Reassign users and groups before deleting this role".into(),
        ));
    }
    let raw = change
        .request()
        .map(|r| serde_json::to_string(&r.permissions).map_err(AppError::internal))
        .transpose()?;
    let requested = raw
        .as_deref()
        .map(access::space_custom_role_permissions)
        .transpose()?;
    let mut actor_roles:Vec<String>=sqlx::query_scalar("SELECT role FROM space_members WHERE space_id=$1 AND user_id=$2 UNION SELECT p.role FROM space_group_permissions p JOIN group_members m ON m.group_id=p.group_id WHERE p.space_id=$1 AND m.user_id=$2").bind(space).bind(&auth.claims.sub).fetch_all(&mut *tx).await.map_err(AppError::internal)?;
    let affects_actor = actor_roles.iter().any(|r| r == role);
    actor_roles.push(role.into());
    actor_roles.sort();
    actor_roles.dedup();
    let mut actor_expected =
        audience_policy_snapshot(&mut tx, space, &auth.claims.sub, &actor_roles).await?;
    let ceiling = access::require_space_membership_management(&mut tx, auth, space).await?;
    if requested.as_ref().is_some_and(|r| !r.is_subset(&ceiling)) {
        return Err(AppError::Forbidden(
            "Cannot grant permissions outside your space authority".into(),
        ));
    }
    let now = chrono::Utc::now().to_rfc3339();
    match &change {
        CustomSpaceRoleChange::Create(req) => {
            let raw = raw.as_ref().unwrap();
            let result=sqlx::query("INSERT INTO space_roles(id,space_id,name,description,permissions,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$7)").bind(role).bind(space).bind(req.name.trim()).bind(req.description.as_deref().unwrap_or_default().trim()).bind(raw).bind(&auth.claims.sub).bind(&now).execute(&mut *tx).await.map_err(AppError::internal)?;
            if result.rows_affected() != 1 {
                return Err(AppError::Conflict("Space role target changed".into()));
            }
            expected.role = Some((
                role.into(),
                space.into(),
                req.name.trim().into(),
                req.description.as_deref().unwrap_or_default().trim().into(),
                raw.clone(),
                auth.claims.sub.clone(),
                now.clone(),
                now,
            ));
        }
        CustomSpaceRoleChange::Update(req) => {
            let raw = raw.as_ref().unwrap();
            let result=sqlx::query("UPDATE space_roles SET name=$1,description=$2,permissions=$3,updated_at=$4 WHERE id=$5 AND space_id=$6").bind(req.name.trim()).bind(req.description.as_deref().unwrap_or_default().trim()).bind(raw).bind(&now).bind(role).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
            if result.rows_affected() != 1 {
                return Err(AppError::Conflict("Space role target changed".into()));
            }
            let expected = expected.role.as_mut().unwrap();
            expected.2 = req.name.trim().into();
            expected.3 = req.description.as_deref().unwrap_or_default().trim().into();
            expected.4 = raw.clone();
            expected.7 = now;
        }
        CustomSpaceRoleChange::Delete => {
            let result = sqlx::query("DELETE FROM space_roles WHERE id=$1 AND space_id=$2")
                .bind(role)
                .bind(space)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
            if result.rows_affected() != 1 {
                return Err(AppError::Conflict("Space role target changed".into()));
            }
            expected.role = None;
        }
    }
    for definition in &mut actor_expected.role_definitions {
        if definition.0 == role {
            definition.1 = expected.role.as_ref().map(|r| r.4.clone());
        }
    }
    if affects_actor {
        access::require_current_human_preference_write(&mut tx, auth).await?;
    } else {
        let current = access::require_space_membership_management(&mut tx, auth, space).await?;
        if requested.as_ref().is_some_and(|r| !r.is_subset(&current)) {
            return Err(AppError::Forbidden(
                "Cannot grant permissions outside your space authority".into(),
            ));
        }
    }
    if custom_space_role_source(&mut tx, space, role).await? != expected {
        return Err(AppError::Conflict("Space role source changed".into()));
    }
    if audience_policy_snapshot(&mut tx, space, &auth.claims.sub, &actor_roles).await?
        != actor_expected
    {
        return Err(AppError::Conflict(
            "Space access changed during role mutation".into(),
        ));
    }
    #[cfg(test)]
    custom_role_final_credential_pause(role).await;
    // Final real identity/method proof follows every source query. Exact
    // intentional self-loss does not manufacture fresh management authority.
    access::require_current_human_preference_write(&mut tx, auth).await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(role.into())
}

async fn create_space_role(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<SaveSpaceRoleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let id = mutate_custom_space_role(
        &state,
        &auth,
        &space_id,
        &format!("custom:{}", Uuid::new_v4()),
        CustomSpaceRoleChange::Create(&req),
    )
    .await?;
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "space.role_create",
        Some("space"),
        Some(&space_id),
    )
    .await;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

async fn update_space_role(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, role_id)): Path<(String, String)>,
    Json(req): Json<SaveSpaceRoleRequest>,
) -> Result<impl IntoResponse, AppError> {
    mutate_custom_space_role(
        &state,
        &auth,
        &space_id,
        &role_id,
        CustomSpaceRoleChange::Update(&req),
    )
    .await?;
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "space.role_update",
        Some("space"),
        Some(&space_id),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_space_role(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, role_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    mutate_custom_space_role(
        &state,
        &auth,
        &space_id,
        &role_id,
        CustomSpaceRoleChange::Delete,
    )
    .await?;
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "space.role_delete",
        Some("space"),
        Some(&space_id),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn ensure_managed_space_group(
    state: &AppState,
    space_id: &str,
    space_name: &str,
    creator_id: &str,
) -> Result<String, AppError> {
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    let group =
        ensure_managed_space_group_on_connection(&mut tx, space_id, space_name, creator_id).await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(group)
}
/// Caller owns a fence-first transaction. Never start/commit or re-enter the pool here.
/// Data-only prepared insert evidence; it grants no actor authority.
pub(crate) struct PreparedManagedSpaceGroup {
    pub(crate) id: String,
    expected: Option<PreparedManagedGroupRows>,
}
struct PreparedManagedGroupRows {
    group: (
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        i64,
        Option<String>,
    ),
    member: (String, String, String, String),
    grant: (String, String, String, String, String, String),
}
impl PreparedManagedSpaceGroup {
    pub(crate) async fn verify_new(&self, conn: &mut sqlx::AnyConnection) -> Result<(), AppError> {
        let expected = self
            .expected
            .as_ref()
            .ok_or_else(|| AppError::Conflict("New personal group already exists".into()))?;
        let group:Option<(String,String,Option<String>,String,String,Option<String>,i64,Option<String>)>=sqlx::query_as("SELECT id,name,description,created_at,updated_at,external_id,scim_managed,managed_space_id FROM groups WHERE id=$1").bind(&self.id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        let members:Vec<(String,String,String,String)>=sqlx::query_as("SELECT id,group_id,user_id,created_at FROM group_members WHERE group_id=$1 ORDER BY id").bind(&self.id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
        let grants:Vec<(String,String,String,String,String,String)>=sqlx::query_as("SELECT id,space_id,group_id,role,created_at,updated_at FROM space_group_permissions WHERE group_id=$1 ORDER BY id").bind(&self.id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
        let extras:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM group_global_roles WHERE group_id=$1)+(SELECT COUNT(*) FROM document_access_allowlist WHERE group_id=$1)").bind(&self.id).fetch_one(conn).await.map_err(AppError::internal)?;
        if group != Some(expected.group.clone())
            || members != vec![expected.member.clone()]
            || grants != vec![expected.grant.clone()]
            || extras != 0
        {
            return Err(AppError::Conflict("Prepared personal group changed".into()));
        }
        Ok(())
    }
}
pub(crate) async fn ensure_managed_space_group_on_connection(
    conn: &mut sqlx::AnyConnection,
    space_id: &str,
    space_name: &str,
    creator_id: &str,
) -> Result<String, AppError> {
    Ok(
        prepare_managed_space_group_on_connection(conn, space_id, space_name, creator_id)
            .await?
            .id,
    )
}
pub(crate) async fn prepare_managed_space_group_on_connection(
    conn: &mut sqlx::AnyConnection,
    space_id: &str,
    space_name: &str,
    creator_id: &str,
) -> Result<PreparedManagedSpaceGroup, AppError> {
    #[cfg(test)]
    managed_group_creation_pause().await;
    let existing: Option<String> =
        sqlx::query_scalar("SELECT id FROM groups WHERE managed_space_id = $1")
            .bind(space_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    if let Some(group_id) = existing {
        return Ok(PreparedManagedSpaceGroup {
            id: group_id,
            expected: None,
        });
    }

    let group_id = Uuid::new_v4().to_string();
    let preferred_name = format!("{space_name} members");
    let name_in_use: Option<String> = sqlx::query_scalar("SELECT id FROM groups WHERE name = $1")
        .bind(&preferred_name)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    let group_name = if name_in_use.is_some() {
        format!("{preferred_name} · {}", &space_id[..space_id.len().min(8)])
    } else {
        preferred_name
    };
    let description = format!("Dedicated collaboration group for {space_name}.");
    let now = chrono::Utc::now().to_rfc3339();
    let member_id = Uuid::new_v4().to_string();
    let grant_id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO groups (id, name, description, managed_space_id, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $5)",
    )
    .bind(&group_id)
    .bind(&group_name)
    .bind(&description)
    .bind(space_id)
    .bind(&now)
    .execute(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    sqlx::query(
        "INSERT INTO group_members (id, group_id, user_id, created_at) VALUES ($1, $2, $3, $4) \
         ON CONFLICT(group_id, user_id) DO NOTHING",
    )
    .bind(&member_id)
    .bind(&group_id)
    .bind(creator_id)
    .bind(&now)
    .execute(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    sqlx::query(
        "INSERT INTO space_group_permissions (id, space_id, group_id, role, created_at, updated_at) \
         VALUES ($1, $2, $3, 'owner', $4, $4) \
         ON CONFLICT(space_id, group_id) DO NOTHING",
    )
    .bind(&grant_id)
    .bind(space_id)
    .bind(&group_id)
    .bind(&now)
    .execute(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    // Dedicated groups own this space through the scoped grant above. Global
    // permissions require an explicit assignment; existing groups/grants remain
    // untouched by the early return and are never repaired or removed here.

    Ok(PreparedManagedSpaceGroup {
        id: group_id.clone(),
        expected: Some(PreparedManagedGroupRows {
            group: (
                group_id.clone(),
                group_name,
                Some(description),
                now.clone(),
                now.clone(),
                None,
                0,
                Some(space_id.into()),
            ),
            member: (member_id, group_id.clone(), creator_id.into(), now.clone()),
            grant: (
                grant_id,
                space_id.into(),
                group_id,
                "owner".into(),
                now.clone(),
                now,
            ),
        }),
    })
}

// ============================================================
// Space Handlers
// ============================================================

/// GET /api/v1/spaces
/// Space catalogs carry actual credential evidence; display role labels never authorize.
pub(crate) async fn require_catalog_space_view(
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
#[cfg(test)]
thread_local! { static CATALOG_AFTER_LOAD: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>> = std::cell::RefCell::new(None); }
#[cfg(test)]
pub(crate) async fn catalog_after_load() {
    let hook = CATALOG_AFTER_LOAD.with(|s| s.borrow_mut().take());
    if let Some(hook) = hook {
        hook.await;
    }
}

#[cfg(test)]
thread_local! {static CATALOG_AFTER_SPACE_VIEW: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>> = std::cell::RefCell::new(None);}
#[cfg(test)]
pub(crate) async fn catalog_after_space_view() {
    let hook = CATALOG_AFTER_SPACE_VIEW.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook.await;
    }
}

async fn list_spaces(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    super::access::require_current_human_read(&mut conn, &auth).await?;
    let spaces: Vec<(String, String, String, Option<String>, Option<String>, Option<String>, String, Option<String>, Option<String>, String, String, String, String, String)> =
        sqlx::query_as(
            "SELECT s.id, s.name, s.slug, s.description, s.icon, s.accent_color, s.visibility, \
             sm.role, \
             (SELECT sgp.role FROM space_group_permissions sgp \
              JOIN group_members gm ON gm.group_id = sgp.group_id \
              WHERE sgp.space_id = s.id AND gm.user_id = $1 \
              ORDER BY CASE sgp.role \
                WHEN 'owner' THEN 1 \
                WHEN 'space_admin' THEN 2 \
                WHEN 'editor' THEN 3 \
                WHEN 'commenter' THEN 4 \
                ELSE 5 END LIMIT 1), \
             s.created_by, COALESCE(NULLIF(owner.username, ''), owner.email, ''), COALESCE(owner.display_name, 'Unknown user'), s.created_at, s.updated_at \
             FROM spaces s \
             LEFT JOIN users owner ON owner.id = s.created_by \
             LEFT JOIN space_members sm ON s.id = sm.space_id AND sm.user_id = $1 \
             ORDER BY s.name",
        )
        .bind(&auth.claims.sub)
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::internal)?;

    #[cfg(test)]
    catalog_after_load().await;
    super::access::require_current_human_read(&mut conn, &auth).await?;
    let mut visible = Vec::new();
    for row in spaces {
        match require_catalog_space_view(&mut conn, &auth, &row.0).await {
            Ok(_) => visible.push(row),
            Err(AppError::NotFound(_)) => {}
            Err(error) => return Err(error),
        }
    }
    let result: Vec<SpaceResponse> = visible
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
                member_role: strongest_role(direct_role, group_role),
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
        sqlx::query_as("SELECT display_name FROM users WHERE id = $1")
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
        sqlx::query_as("SELECT COALESCE(NULLIF(username, ''), email) FROM users WHERE id = $1")
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

    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    crate::auth::middleware::require_current_admin(&mut tx, &admin).await?;
    // Check slug uniqueness
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM spaces WHERE slug = $1")
        .bind(&slug)
        .fetch_optional(&mut *tx)
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
    let visibility = req.visibility.as_deref().unwrap_or("internal");
    validate_space_visibility(visibility)?;

    let existing_id: Option<(String,)> = sqlx::query_as("SELECT id FROM spaces WHERE id = $1")
        .bind(&space_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if existing_id.is_some() {
        return Err(AppError::Conflict(
            "A space with this import id already exists".into(),
        ));
    }

    sqlx::query(
        "INSERT INTO spaces (id, name, slug, description, icon, accent_color, visibility, created_by, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
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
    .execute(&mut *tx)
    .await
    .map_err(AppError::internal)?;

    ensure_managed_space_group_on_connection(&mut tx, &space_id, &req.name, &admin.claims.sub)
        .await?;

    // Add creator as owner
    let member_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO space_members (id, space_id, user_id, role) VALUES ($1, $2, $3, 'owner')",
    )
    .bind(&member_id)
    .bind(&space_id)
    .bind(&admin.claims.sub)
    .execute(&mut *tx)
    .await
    .map_err(AppError::internal)?;

    crate::auth::middleware::require_current_admin(&mut tx, &admin).await?;
    tx.commit().await.map_err(AppError::internal)?;
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
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_catalog_space_view(&mut conn, &auth, &space_id).await?;
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
             s.created_by, COALESCE(owner.email, ''), COALESCE(owner.display_name, 'Unknown user'), s.created_at, s.updated_at \
             FROM spaces s LEFT JOIN users owner ON owner.id = s.created_by WHERE s.id = $1",
    )
    .bind(&space_id)
    .fetch_optional(&mut *conn)
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
    ) = space.ok_or_else(|| AppError::NotFound("Target unavailable".into()))?;

    #[cfg(test)]
    catalog_after_load().await;
    let access = require_catalog_space_view(&mut conn, &auth, &space_id).await?;
    // Compatibility display only; the additive typed permission union authorized above.
    let effective_role = if access.instance_admin {
        "admin".to_string()
    } else {
        access
            .roles
            .iter()
            .max_by_key(|role| role_rank(&role.id))
            .map(|role| role.id.clone())
            .unwrap_or_else(|| "viewer".into())
    };

    Ok(Json(SpaceResponse {
        id,
        name,
        slug,
        description: desc,
        icon,
        accent_color: color,
        visibility: vis,
        member_role: Some(effective_role),
        created_by,
        owner_username,
        owner_display_name,
        created_at: created,
        updated_at: updated,
    }))
}

type SpaceUpdateSource = (String, String, String, Option<String>, Option<String>, Option<String>, String, String, String, String, i32);

async fn space_update_source(conn: &mut sqlx::AnyConnection, space: &str) -> Result<SpaceUpdateSource, AppError> {
    sqlx::query_as("SELECT id,name,slug,description,icon,accent_color,visibility,created_by,created_at,updated_at,COALESCE(trash_retention_days,30) FROM spaces WHERE id=$1")
        .bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound("Target unavailable".into()))
}

pub(crate) async fn update_space_on_current_authority(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    req: &UpdateSpaceRequest,
    imported_updated_at: Option<&str>,
) -> Result<(), AppError> {
    // Validate the entire requested update before the first canonical write.
    if req.name.as_ref().is_some_and(|v| v.len() > 100) {
        return Err(AppError::BadRequest("Space name must be at most 100 characters".into()));
    }
    if req.description.as_ref().is_some_and(|v| v.len() > 1000) {
        return Err(AppError::BadRequest("Space description must be at most 1000 characters".into()));
    }
    if let Some(visibility) = req.visibility.as_deref() {
        validate_space_visibility(visibility)?;
    }
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[space]).await?;
    // This adapter requires actual account ManageSpace and a current human Write/Admin key ceiling.
    super::access::require_space_membership_management(&mut tx, auth, space).await?;
    let mut expected = space_update_source(&mut tx, space).await?;
    let changed = req.name.is_some() || req.description.is_some() || req.icon.is_some() || req.accent_color.is_some() || req.visibility.is_some();
    if changed {
        if let Some(v) = &req.name { expected.1 = v.clone(); }
        if let Some(v) = &req.description { expected.3 = Some(v.clone()); }
        if let Some(v) = &req.icon { expected.4 = Some(v.clone()); }
        if let Some(v) = &req.accent_color { expected.5 = Some(v.clone()); }
        if let Some(v) = &req.visibility { expected.6 = v.clone(); }
        expected.9 = imported_updated_at.map(str::to_owned).unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
        if sqlx::query("UPDATE spaces SET name=$1,description=$2,icon=$3,accent_color=$4,visibility=$5,updated_at=$6 WHERE id=$7")
            .bind(&expected.1).bind(&expected.3).bind(&expected.4).bind(&expected.5).bind(&expected.6).bind(&expected.9).bind(space)
            .execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected() != 1 {
            return Err(AppError::Conflict("Space source changed".into()));
        }
    }
    if space_update_source(&mut tx, space).await? != expected {
        return Err(AppError::Conflict("Space source changed".into()));
    }
    super::access::require_space_membership_management(&mut tx, auth, space).await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(())
}

/// PUT /api/v1/spaces/{space_id}
async fn update_space(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<UpdateSpaceRequest>,
) -> Result<impl IntoResponse, AppError> {
    update_space_on_current_authority(&state, &auth, &space_id, &req, None).await?;

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

/// GET /api/v1/spaces/{space_id}/retention
async fn get_space_retention(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_catalog_space_view(&mut conn, &auth, &space_id).await?;
    let source = space_update_source(&mut conn, &space_id).await?;
    #[cfg(test)]
    catalog_after_load().await;
    require_catalog_space_view(&mut conn, &auth, &space_id).await?;
    let current = space_update_source(&mut conn, &space_id).await?;
    if current != source {
        return Err(AppError::Conflict("Space source changed".into()));
    }
    super::access::require_current_human_read(&mut conn, &auth).await?;
    let trash_retention_days = source.10;
    Ok(Json(SpaceRetentionResponse {
        trash_retention_days,
    }))
}

/// PUT /api/v1/spaces/{space_id}/retention
async fn update_space_retention(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<UpdateSpaceRetentionRequest>,
) -> Result<impl IntoResponse, AppError> {
    if !(30..=120).contains(&req.trash_retention_days) {
        return Err(AppError::BadRequest(
            "Trash retention must be between 30 and 120 days".into(),
        ));
    }
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    super::access::require_space_membership_management(&mut tx, &auth, &space_id).await?;
    let mut expected = space_update_source(&mut tx, &space_id).await?;
    expected.10 = req.trash_retention_days;
    expected.9 = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query("UPDATE spaces SET trash_retention_days=$1,updated_at=$2 WHERE id=$3")
        .bind(expected.10).bind(&expected.9).bind(&space_id)
        .execute(&mut *tx).await.map_err(AppError::internal)?;
    if result.rows_affected() != 1 || space_update_source(&mut tx, &space_id).await? != expected {
        return Err(AppError::Conflict("Space source changed".into()));
    }
    super::access::require_space_membership_management(&mut tx, &auth, &space_id).await?;
    tx.commit().await.map_err(AppError::internal)?;
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "space.retention_update",
        Some("space"),
        Some(&space_id),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/v1/spaces/{space_id}
async fn delete_space(
    State(state): State<AppState>, auth: AuthUser, Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    super::space_deletion::delete(&state, &auth, &space_id).await?;
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
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_catalog_space_view(&mut conn, &auth, &space_id).await?;

    let members: Vec<(String, String, String, Option<String>, String, String)> = sqlx::query_as(
        "SELECT u.id, u.email, u.display_name, u.avatar_url, sm.role, sm.created_at \
         FROM space_members sm \
         JOIN users u ON sm.user_id = u.id \
         WHERE sm.space_id = $1 \
         ORDER BY sm.role, u.display_name",
    )
    .bind(&space_id)
    .fetch_all(&mut *conn)
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

    #[cfg(test)]
    catalog_after_load().await;
    require_catalog_space_view(&mut conn, &auth, &space_id).await?;
    super::access::require_current_human_read(&mut conn, &auth).await?;
    Ok(Json(result))
}

/// GET /api/v1/spaces/{space_id}/access-users
async fn list_effective_space_users(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_space_groups_read(&mut conn, &auth, &space_id).await?;

    let rows: Vec<(String, String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT u.id, u.email, u.display_name, 'direct', u.id, 'Direct assignment', sm.role \
         FROM space_members sm JOIN users u ON u.id = sm.user_id \
         WHERE sm.space_id = $1 \
         UNION ALL \
         SELECT u.id, u.email, u.display_name, 'group', g.id, g.name, sgp.role \
         FROM space_group_permissions sgp \
         JOIN groups g ON g.id = sgp.group_id \
         JOIN group_members gm ON gm.group_id = g.id \
         JOIN users u ON u.id = gm.user_id \
         WHERE sgp.space_id = $1 \
         ORDER BY 3, 4, 6",
    )
    .bind(&space_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let blocked_ids: Vec<String> =
        sqlx::query_scalar("SELECT user_id FROM space_user_blocks WHERE space_id = $1")
            .bind(&space_id)
            .fetch_all(&mut *conn)
            .await
            .map_err(AppError::internal)?;

    let mut users: BTreeMap<String, EffectiveSpaceUserResponse> = BTreeMap::new();
    for (user_id, email, display_name, kind, source_id, source_name, role) in rows {
        let candidate_rank = role_rank_on_connection(&mut conn, &space_id, &role).await?;
        let current_rank = if let Some(current_role) = users
            .get(&user_id)
            .and_then(|user| user.effective_role.as_deref())
        {
            role_rank_on_connection(&mut conn, &space_id, current_role).await?
        } else {
            0
        };
        let user = users
            .entry(user_id.clone())
            .or_insert_with(|| EffectiveSpaceUserResponse {
                user_id,
                email,
                display_name,
                effective_role: None,
                direct_role: None,
                blocked: false,
                sources: Vec::new(),
            });
        if kind == "direct" {
            user.direct_role = Some(role.clone());
        }
        if user
            .effective_role
            .as_ref()
            .is_none_or(|_| candidate_rank > current_rank)
        {
            user.effective_role = Some(role.clone());
        }
        user.sources.push(EffectiveAccessSource {
            kind: if kind == "direct" { "direct" } else { "group" },
            id: source_id,
            name: source_name,
            role,
        });
    }
    for blocked_id in blocked_ids {
        if let Some(user) = users.get_mut(&blocked_id) {
            user.blocked = true;
            user.effective_role = None;
        }
    }

    let mut result = users.into_values().collect::<Vec<_>>();
    result.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
    });
    #[cfg(test)]
    catalog_after_load().await;
    require_space_groups_read(&mut conn, &auth, &space_id).await?;
    super::access::require_current_human_read(&mut conn, &auth).await?;
    Ok(Json(result))
}

#[derive(Clone)]
enum DirectAudienceChange {
    Add { user_id: String, role: String },
    Update { user_id: String, role: String },
    Remove { user_id: String },
    Block { user_id: String },
    Unblock { user_id: String },
}
impl DirectAudienceChange {
    fn target(&self) -> &str {
        match self {
            Self::Add { user_id, .. }
            | Self::Update { user_id, .. }
            | Self::Remove { user_id }
            | Self::Block { user_id }
            | Self::Unblock { user_id } => user_id,
        }
    }
    fn role(&self) -> Option<&str> {
        match self {
            Self::Add { role, .. } | Self::Update { role, .. } => Some(role),
            _ => None,
        }
    }
    fn audit(&self) -> &'static str {
        match self {
            Self::Add { .. } => "space.member_add",
            Self::Update { .. } => "space.member_role_update",
            Self::Remove { .. } => "space.member_remove",
            Self::Block { .. } => "space.user_block",
            Self::Unblock { .. } => "space.user_unblock",
        }
    }
}

/// Only current authority dependencies, never session/key middleware bookkeeping.
/// The role ID list is fixed by initial approval plus the intended new role, so
/// removing a direct role does not accidentally remove its policy evidence.
#[derive(Clone, Debug, PartialEq, Eq)]
struct AudiencePolicySnapshot {
    actor: (String, i64),
    space: (String, String),
    direct: Option<(String, String, String)>,
    block: Option<(String, String)>,
    groups: Vec<String>,
    grants: Vec<(String, String)>,
    globals: Vec<(String, String, Option<String>)>,
    role_definitions: Vec<(String, Option<String>)>,
}
async fn audience_policy_snapshot(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    actor: &str,
    roles: &[String],
) -> Result<AudiencePolicySnapshot, AppError> {
    let actor_row = sqlx::query_as("SELECT role,is_active FROM users WHERE id=$1")
        .bind(actor)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound("Target unavailable".into()))?;
    let space_row = sqlx::query_as("SELECT visibility,created_by FROM spaces WHERE id=$1")
        .bind(space)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound("Target unavailable".into()))?;
    let direct = sqlx::query_as(
        "SELECT id,role,created_at FROM space_members WHERE space_id=$1 AND user_id=$2",
    )
    .bind(space)
    .bind(actor)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let block = sqlx::query_as(
        "SELECT created_by,created_at FROM space_user_blocks WHERE space_id=$1 AND user_id=$2",
    )
    .bind(space)
    .bind(actor)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let groups =
        sqlx::query_scalar("SELECT group_id FROM group_members WHERE user_id=$1 ORDER BY group_id")
            .bind(actor)
            .fetch_all(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    let grants = sqlx::query_as("SELECT sgp.group_id,sgp.role FROM space_group_permissions sgp JOIN group_members gm ON gm.group_id=sgp.group_id WHERE sgp.space_id=$1 AND gm.user_id=$2 ORDER BY sgp.group_id,sgp.role").bind(space).bind(actor).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let globals = sqlx::query_as("SELECT gr.group_id,gr.role_id,r.permissions FROM group_global_roles gr JOIN group_members gm ON gm.group_id=gr.group_id LEFT JOIN global_roles r ON r.id=gr.role_id WHERE gm.user_id=$1 ORDER BY gr.group_id,gr.role_id").bind(actor).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let mut role_definitions = Vec::new();
    for role in roles {
        let raw =
            sqlx::query_scalar("SELECT permissions FROM space_roles WHERE space_id=$1 AND id=$2")
                .bind(space)
                .bind(role)
                .fetch_optional(&mut *conn)
                .await
                .map_err(AppError::internal)?;
        role_definitions.push((role.clone(), raw));
    }
    Ok(AudiencePolicySnapshot {
        actor: actor_row,
        space: space_row,
        direct,
        block,
        groups,
        grants,
        globals,
        role_definitions,
    })
}

async fn direct_audience_target(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    user: &str,
) -> Result<(Option<(String, String, String)>, Option<(String, String)>), AppError> {
    let direct = sqlx::query_as(
        "SELECT id,role,created_at FROM space_members WHERE space_id=$1 AND user_id=$2",
    )
    .bind(space)
    .bind(user)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let block = sqlx::query_as(
        "SELECT created_by,created_at FROM space_user_blocks WHERE space_id=$1 AND user_id=$2",
    )
    .bind(space)
    .bind(user)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    Ok((direct, block))
}
async fn apply_direct_audience_change(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    change: &DirectAudienceChange,
) -> Result<(), AppError> {
    use super::access;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[space])
        .await
        .map_err(|error| match error {
            AppError::NotFound(_) => AppError::NotFound("Target unavailable".into()),
            other => other,
        })?;
    // Lock only scoped target associations, never unrelated/global user rows.
    sqlx::query("UPDATE space_members SET role=role WHERE space_id=$1 AND user_id=$2")
        .bind(space)
        .bind(change.target())
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    sqlx::query(
        "UPDATE space_user_blocks SET created_at=created_at WHERE space_id=$1 AND user_id=$2",
    )
    .bind(space)
    .bind(change.target())
    .execute(&mut *tx)
    .await
    .map_err(AppError::internal)?;
    let ceiling = access::require_space_membership_management(&mut tx, auth, space).await?;
    if let Some(role) = change.role() {
        sqlx::query("UPDATE space_roles SET permissions=permissions WHERE space_id=$1 AND id=$2")
            .bind(space)
            .bind(role)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
        let requested = access::space_membership_role_permissions(&mut tx, space, role).await?;
        if !requested.is_subset(&ceiling) {
            return Err(AppError::Forbidden(
                "Cannot grant permissions outside your space authority".into(),
            ));
        }
    }
    let creator: String = sqlx::query_scalar("SELECT created_by FROM spaces WHERE id=$1")
        .bind(space)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if matches!(change, DirectAudienceChange::Block { .. }) && change.target() == creator {
        return Err(AppError::Conflict(
            "The space owner cannot be blocked".into(),
        ));
    }
    let mut expected_target = direct_audience_target(&mut tx, space, change.target()).await?;
    let self_change = change.target() == auth.claims.sub;
    let mut role_ids:Vec<String>=sqlx::query_scalar("SELECT role FROM space_members WHERE space_id=$1 AND user_id=$2 UNION SELECT sgp.role FROM space_group_permissions sgp JOIN group_members gm ON gm.group_id=sgp.group_id WHERE sgp.space_id=$1 AND gm.user_id=$2").bind(space).bind(&auth.claims.sub).fetch_all(&mut *tx).await.map_err(AppError::internal)?;
    if let Some(role) = change.role() {
        role_ids.push(role.into());
    }
    role_ids.sort();
    role_ids.dedup();
    let initial = if self_change {
        Some(audience_policy_snapshot(&mut tx, space, &auth.claims.sub, &role_ids).await?)
    } else {
        None
    };
    // Approval is current after target/definition waits. For self-loss this is
    // immutable approval, not an invented post-mutation administrator principal.
    let ceiling = access::require_space_membership_management(&mut tx, auth, space).await?;
    if let Some(role) = change.role() {
        if !access::space_membership_role_permissions(&mut tx, space, role)
            .await?
            .is_subset(&ceiling)
        {
            return Err(AppError::Forbidden(
                "Cannot grant permissions outside your space authority".into(),
            ));
        }
    }
    if let Some(initial) = &initial {
        if audience_policy_snapshot(&mut tx, space, &auth.claims.sub, &role_ids).await? != *initial
        {
            return Err(AppError::Conflict(
                "Space access changed during approval".into(),
            ));
        }
    }
    let id = Uuid::new_v4().to_string();
    let created = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let blocked_at = chrono::Utc::now().to_rfc3339();
    let mut expected = initial.clone();
    match change {
        DirectAudienceChange::Add { user_id, role } => {
            let affected=sqlx::query("INSERT INTO space_members(id,space_id,user_id,role,created_at) VALUES($1,$2,$3,$4,$5)").bind(&id).bind(space).bind(user_id).bind(role).bind(&created).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected();
            if affected != 1 {
                return Err(AppError::Conflict(
                    "Membership target changed during write".into(),
                ));
            }
            expected_target.0 = Some((id.clone(), role.clone(), created.clone()));
            if let Some(e) = &mut expected {
                e.direct = Some((id, role.clone(), created.clone()));
            }
        }
        DirectAudienceChange::Update { user_id, role } => {
            let affected =
                sqlx::query("UPDATE space_members SET role=$1 WHERE space_id=$2 AND user_id=$3")
                    .bind(role)
                    .bind(space)
                    .bind(user_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::internal)?
                    .rows_affected();
            if affected != u64::from(expected_target.0.is_some()) {
                return Err(AppError::Conflict(
                    "Membership target changed during write".into(),
                ));
            }
            if let Some(d) = &mut expected_target.0 {
                d.1 = role.clone();
            }
            if let Some(e) = &mut expected {
                if let Some(d) = &mut e.direct {
                    d.1 = role.clone();
                }
            }
        }
        DirectAudienceChange::Remove { user_id } => {
            let affected =
                sqlx::query("DELETE FROM space_members WHERE space_id=$1 AND user_id=$2")
                    .bind(space)
                    .bind(user_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::internal)?
                    .rows_affected();
            if affected != u64::from(expected_target.0.is_some()) {
                return Err(AppError::Conflict(
                    "Membership target changed during write".into(),
                ));
            }
            expected_target.0 = None;
            if let Some(e) = &mut expected {
                e.direct = None;
            }
        }
        DirectAudienceChange::Block { user_id } => {
            let affected=sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by,created_at) VALUES($1,$2,$3,$4) ON CONFLICT(space_id,user_id) DO UPDATE SET created_by=excluded.created_by,created_at=excluded.created_at").bind(space).bind(user_id).bind(&auth.claims.sub).bind(&blocked_at).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected();
            if affected != 1 {
                return Err(AppError::Conflict(
                    "Membership target changed during write".into(),
                ));
            }
            expected_target.1 = Some((auth.claims.sub.clone(), blocked_at.clone()));
            if let Some(e) = &mut expected {
                e.block = Some((auth.claims.sub.clone(), blocked_at.clone()));
            }
        }
        DirectAudienceChange::Unblock { user_id } => {
            let affected =
                sqlx::query("DELETE FROM space_user_blocks WHERE space_id=$1 AND user_id=$2")
                    .bind(space)
                    .bind(user_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::internal)?
                    .rows_affected();
            if affected != u64::from(expected_target.1.is_some()) {
                return Err(AppError::Conflict(
                    "Membership target changed during write".into(),
                ));
            }
            expected_target.1 = None;
            if let Some(e) = &mut expected {
                e.block = None;
            }
        }
    }
    if matches!(change, DirectAudienceChange::Block { .. }) {
        let current_creator: String =
            sqlx::query_scalar("SELECT created_by FROM spaces WHERE id=$1")
                .bind(space)
                .fetch_one(&mut *tx)
                .await
                .map_err(AppError::internal)?;
        if current_creator == change.target() {
            return Err(AppError::Conflict(
                "The space owner cannot be blocked".into(),
            ));
        }
    }
    if let Some(expected) = expected {
        // Preserve intentional self-demotion/removal/block, but reject ANY other
        // dependency drift and revalidate the actual active credential after SQL.
        access::require_current_human_preference_write(&mut tx, auth).await?;
        if audience_policy_snapshot(&mut tx, space, &auth.claims.sub, &role_ids).await? != expected
        {
            return Err(AppError::Conflict(
                "Space access changed during self mutation".into(),
            ));
        }
        access::require_current_human_preference_write(&mut tx, auth).await?;
    } else {
        if let Some(role) = change.role() {
            let requested = access::space_membership_role_permissions(&mut tx, space, role).await?;
            let current = access::require_space_membership_management(&mut tx, auth, space).await?;
            if !requested.is_subset(&current) {
                return Err(AppError::Forbidden(
                    "Cannot grant permissions outside your space authority".into(),
                ));
            }
        } else {
            access::require_space_membership_management(&mut tx, auth, space).await?;
        }
    }
    if direct_audience_target(&mut tx, space, change.target()).await? != expected_target {
        return Err(AppError::Conflict(
            "Membership target changed during mutation".into(),
        ));
    }
    tx.commit().await.map_err(AppError::internal)?;
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        change.audit(),
        Some("space"),
        Some(space),
    )
    .await;
    Ok(())
}

async fn block_space_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, user_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    apply_direct_audience_change(
        &state,
        &auth,
        &space_id,
        &DirectAudienceChange::Block { user_id },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unblock_space_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, user_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    apply_direct_audience_change(
        &state,
        &auth,
        &space_id,
        &DirectAudienceChange::Unblock { user_id },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_space_principals(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_space_groups_read(&mut conn, &auth, &space_id).await?;

    let users: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id, email, display_name FROM users WHERE is_active = 1 ORDER BY display_name",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let groups: Vec<(String, String, Option<String>, i64, Option<String>)> = sqlx::query_as(
        "SELECT g.id, g.name, g.description, COUNT(gm.user_id), g.managed_space_id \
         FROM groups g LEFT JOIN group_members gm ON gm.group_id = g.id \
         WHERE g.scim_managed = 0 AND (g.managed_space_id IS NULL OR g.managed_space_id = $1) \
         GROUP BY g.id, g.name, g.description, g.managed_space_id \
         ORDER BY CASE WHEN g.managed_space_id = $1 THEN 0 ELSE 1 END, g.name",
    )
    .bind(&space_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    #[cfg(test)]
    catalog_after_load().await;
    require_space_groups_read(&mut conn, &auth, &space_id).await?;
    super::access::require_current_human_read(&mut conn, &auth).await?;
    Ok(Json(SpacePrincipalCatalog {
        users: users
            .into_iter()
            .map(|(id, email, display_name)| SpacePrincipalUser {
                id,
                email,
                display_name,
            })
            .collect(),
        groups: groups
            .into_iter()
            .map(
                |(id, name, description, member_count, managed_space_id)| SpacePrincipalGroup {
                    id,
                    name,
                    description,
                    member_count,
                    managed_space_id,
                },
            )
            .collect(),
    }))
}

/// Session-less real recipient eligibility, not an HTTP or mutation approval.
/// Delivery remains advisory; this is not a fence against every audience writer.
async fn direct_invite_recipient_visible(state: &AppState, space: &str, user: &str) -> bool {
    let Ok(mut conn) = state.db.acquire().await else {
        return false;
    };
    super::access::resolve_space_access(
        &mut conn,
        super::access::Principal::User {
            user_id: user,
            session_id: None,
            token_expires_at: None,
        },
        space,
        super::access::Purpose::Routine,
    )
    .await
    .is_ok_and(|a| a.allows(super::access::Permission::View))
}

/// POST /api/v1/spaces/{space_id}/members
async fn add_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<AddMemberRequest>,
) -> Result<impl IntoResponse, AppError> {
    let role = req.role.as_deref().unwrap_or("editor");
    apply_direct_audience_change(
        &state,
        &auth,
        &space_id,
        &DirectAudienceChange::Add {
            user_id: req.user_id.clone(),
            role: role.into(),
        },
    )
    .await?;

    // Direct membership is an invitation event: keep the recipient's durable
    // inbox in step with their newly granted space access. The caller already
    // sees the completed action, so do not create a self-notification.
    if req.user_id != auth.claims.sub
        && direct_invite_recipient_visible(&state, &space_id, &req.user_id).await
    {
        let invitation_context: Option<(String, String)> = sqlx::query_as(
            "SELECT s.name, COALESCE(NULLIF(u.display_name, ''), u.email) \
             FROM spaces s JOIN users u ON u.id = $1 WHERE s.id = $2",
        )
        .bind(&auth.claims.sub)
        .bind(&space_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();

        if let Some((space_name, actor_name)) = invitation_context {
            if !direct_invite_recipient_visible(&state, &space_id, &req.user_id).await {
                return Ok(StatusCode::CREATED);
            }
            let (title, body) = space_invite_notification_copy(&space_name, &actor_name, role);
            crate::notifications::notify(
                &state.db,
                &req.user_id,
                "space_invite",
                &title,
                Some(&body),
                Some(&auth.claims.sub),
                Some("space"),
                Some(&space_id),
            )
            .await;
        }
    }

    Ok(StatusCode::CREATED)
}

/// PUT /api/v1/spaces/{space_id}/members/{user_id}
async fn update_member_role(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, user_id)): Path<(String, String)>,
    Json(req): Json<AddMemberRequest>,
) -> Result<impl IntoResponse, AppError> {
    apply_direct_audience_change(
        &state,
        &auth,
        &space_id,
        &DirectAudienceChange::Update {
            user_id,
            role: req.role.unwrap_or_else(|| "editor".into()),
        },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/v1/spaces/{space_id}/members/{user_id}
async fn remove_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, user_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    apply_direct_audience_change(
        &state,
        &auth,
        &space_id,
        &DirectAudienceChange::Remove { user_id },
    )
    .await?;
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
    let groups: Vec<(String, String, Option<String>, i64, i64, i64, Option<String>, String, String)> = sqlx::query_as(
        "SELECT g.id, g.name, g.description, \
             (SELECT COUNT(*) FROM group_members gm WHERE gm.group_id = g.id) AS member_count, \
             (SELECT COUNT(*) FROM space_group_permissions sgp WHERE sgp.group_id = g.id) AS space_grant_count, \
             g.scim_managed, g.managed_space_id, g.created_at, g.updated_at \
         FROM groups g \
         ORDER BY g.name",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<GroupResponse> = groups
        .into_iter()
        .map(
            |(
                id,
                name,
                description,
                member_count,
                space_grant_count,
                scim_managed,
                managed_space_id,
                created_at,
                updated_at,
            )| GroupResponse {
                id,
                name,
                description,
                member_count,
                space_grant_count,
                scim_managed: scim_managed != 0,
                managed_space_id,
                created_at,
                updated_at,
            },
        )
        .collect();

    Ok(Json(result))
}

/// GET /api/v1/admin/roles
async fn list_global_roles(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, AppError> {
    let roles: Vec<(String, String, String, String, i64)> = sqlx::query_as(
        "SELECT id, name, description, permissions, built_in FROM global_roles ORDER BY built_in DESC, lower(name)",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;
    let result = roles
        .into_iter()
        .map(
            |(id, name, description, permissions, built_in)| GlobalRoleResponse {
                id,
                name,
                description,
                permissions: serde_json::from_str(&permissions).unwrap_or_default(),
                built_in: built_in != 0,
            },
        )
        .collect::<Vec<_>>();
    Ok(Json(result))
}

/// GET /api/v1/admin/groups/{group_id}/roles
async fn list_group_global_roles(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(group_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let roles: Vec<(String, String, String, String, i64)> = sqlx::query_as(
        "SELECT r.id, r.name, r.description, r.permissions, r.built_in
         FROM group_global_roles gr JOIN global_roles r ON r.id = gr.role_id
         WHERE gr.group_id = $1 ORDER BY lower(r.name)",
    )
    .bind(&group_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;
    Ok(Json(
        roles
            .into_iter()
            .map(
                |(id, name, description, permissions, built_in)| GlobalRoleResponse {
                    id,
                    name,
                    description,
                    permissions: serde_json::from_str(&permissions).unwrap_or_default(),
                    built_in: built_in != 0,
                },
            )
            .collect::<Vec<_>>(),
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GlobalRoleMutationSource {
    spaces: Vec<String>,
    group: (
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        i64,
        Option<String>,
    ),
    role: (String, String, String, String, i64, String, String),
    linked_role_definitions: Vec<(String, String, String, String, i64, String, String)>,
    members: Vec<(String, String, String, String)>,
    grants: Vec<(String, String, String, String, String, String)>,
    roles: Vec<(String, String, String)>,
}

async fn global_role_mutation_source(
    conn: &mut sqlx::AnyConnection,
    group_id: &str,
    role_id: &str,
) -> Result<GlobalRoleMutationSource, AppError> {
    let spaces = sqlx::query_scalar("SELECT id FROM spaces ORDER BY id")
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    let group = sqlx::query_as("SELECT id,name,description,created_at,updated_at,external_id,scim_managed,managed_space_id FROM groups WHERE id=$1")
        .bind(group_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound("Group not found".into()))?;
    let role = sqlx::query_as("SELECT id,name,description,permissions,built_in,created_at,updated_at FROM global_roles WHERE id=$1")
        .bind(role_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?
        .ok_or_else(|| AppError::NotFound("Global role not found".into()))?;
    let linked_role_definitions = sqlx::query_as("SELECT r.id,r.name,r.description,r.permissions,r.built_in,r.created_at,r.updated_at FROM global_roles r WHERE r.id=$2 OR EXISTS(SELECT 1 FROM group_global_roles g WHERE g.group_id=$1 AND g.role_id=r.id) ORDER BY r.id")
        .bind(group_id).bind(role_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let orphan:Option<i64>=sqlx::query_scalar("SELECT 1 FROM group_global_roles g LEFT JOIN global_roles r ON r.id=g.role_id WHERE g.group_id=$1 AND r.id IS NULL LIMIT 1")
        .bind(group_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if orphan.is_some() {
        return Err(AppError::Conflict("Audience source is inconsistent".into()));
    }
    let members = sqlx::query_as(
        "SELECT id,group_id,user_id,created_at FROM group_members WHERE group_id=$1 ORDER BY id",
    )
    .bind(group_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let grants = sqlx::query_as("SELECT id,space_id,group_id,role,created_at,updated_at FROM space_group_permissions WHERE group_id=$1 ORDER BY id")
        .bind(group_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let roles = sqlx::query_as("SELECT group_id,role_id,created_at FROM group_global_roles WHERE group_id=$1 ORDER BY role_id")
        .bind(group_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    Ok(GlobalRoleMutationSource {
        spaces,
        group,
        role,
        linked_role_definitions,
        members,
        grants,
        roles,
    })
}

/// Only these two admin routes adopt the topology fence in this slice.
/// Other topology writers must join before claiming protection from phantoms.
async fn mutate_group_global_role(
    state: &AppState,
    admin: &AdminUser,
    group_id: &str,
    role_id: &str,
    assign: bool,
) -> Result<crate::audit::CapturedActorAudit, AppError> {
    use super::mutation_guards::{lock_audience_for_mutation, lock_spaces_for_mutation};
    use crate::auth::middleware::require_current_admin;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_audience_for_mutation(&mut tx).await?;
    let spaces: Vec<String> = sqlx::query_scalar("SELECT id FROM spaces ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    lock_spaces_for_mutation(
        &mut tx,
        &spaces.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await?;
    require_current_admin(&mut tx, admin).await?;
    let group = sqlx::query("UPDATE groups SET id=id WHERE id=$1")
        .bind(group_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if group.rows_affected() != 1 {
        return Err(AppError::NotFound(
            if assign {
                "Group not found"
            } else {
                "Group role assignment not found"
            }
            .into(),
        ));
    }
    let role_ids:Vec<String>=sqlx::query_scalar("SELECT r.id FROM global_roles r WHERE r.id=$2 OR EXISTS(SELECT 1 FROM group_global_roles g WHERE g.group_id=$1 AND g.role_id=r.id) ORDER BY r.id")
        .bind(group_id).bind(role_id).fetch_all(&mut *tx).await.map_err(AppError::internal)?;
    if !role_ids.iter().any(|id| id == role_id) {
        return Err(AppError::NotFound(
            if assign {
                "Global role not found"
            } else {
                "Group role assignment not found"
            }
            .into(),
        ));
    }
    for id in role_ids {
        let role = sqlx::query("UPDATE global_roles SET id=id WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
        if role.rows_affected() != 1 {
            return Err(AppError::Conflict("Audience source changed".into()));
        }
    }
    // Lock each existing association in deterministic family/identity order.
    for (select, update) in [
        (
            "SELECT id FROM group_members WHERE group_id=$1 ORDER BY id",
            "UPDATE group_members SET id=id WHERE group_id=$1 AND id=$2",
        ),
        (
            "SELECT id FROM space_group_permissions WHERE group_id=$1 ORDER BY id",
            "UPDATE space_group_permissions SET id=id WHERE group_id=$1 AND id=$2",
        ),
        (
            "SELECT role_id FROM group_global_roles WHERE group_id=$1 ORDER BY role_id",
            "UPDATE group_global_roles SET group_id=group_id WHERE group_id=$1 AND role_id=$2",
        ),
    ] {
        let ids: Vec<String> = sqlx::query_scalar(select)
            .bind(group_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(AppError::internal)?;
        for id in ids {
            let rows = sqlx::query(update)
                .bind(group_id)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
            if rows.rows_affected() != 1 {
                return Err(AppError::Conflict("Audience source changed".into()));
            }
        }
    }
    require_current_admin(&mut tx, admin).await?;
    let mut expected = global_role_mutation_source(&mut tx, group_id, role_id).await?;
    if expected.spaces != spaces {
        return Err(AppError::Conflict("Audience source changed".into()));
    }
    if !assign && expected.group.7.is_some() && role_id == "owner" {
        return Err(AppError::Forbidden(
            "A space's managed group must retain the Owner role".into(),
        ));
    }
    if assign {
        let created:Option<String>=sqlx::query_scalar("INSERT INTO group_global_roles(group_id,role_id) VALUES($1,$2) ON CONFLICT(group_id,role_id) DO NOTHING RETURNING created_at")
            .bind(group_id).bind(role_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
        if let Some(created) = created {
            expected
                .roles
                .push((group_id.into(), role_id.into(), created));
            expected.roles.sort_by(|a, b| a.1.cmp(&b.1));
        }
    } else {
        let result = sqlx::query("DELETE FROM group_global_roles WHERE group_id=$1 AND role_id=$2")
            .bind(group_id)
            .bind(role_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
        if result.rows_affected() != 1 {
            return Err(AppError::NotFound("Group role assignment not found".into()));
        }
        expected.roles.retain(|r| r.1 != role_id);
    }
    let authority = require_current_admin(&mut tx, admin).await?;
    let audit = crate::audit::insert_actor_action(
        &mut tx,
        &authority,
        if assign {
            "group.role_assign"
        } else {
            "group.role_remove"
        },
        "group",
        group_id,
    )
    .await?;
    // Audit SQL can itself wait or trigger policy/source changes. All final
    // proofs follow it, and denial rolls back both the grant and its audit.
    let authority = require_current_admin(&mut tx, admin).await?;
    if global_role_mutation_source(&mut tx, group_id, role_id).await? != expected {
        return Err(AppError::Conflict("Audience source changed".into()));
    }
    crate::audit::verify_actor_action(&mut tx, &audit, &authority).await?;
    #[cfg(test)]
    after_audience_equality_proofs().await;
    require_current_admin(&mut tx, admin).await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(audit)
}

/// POST /api/v1/admin/groups/{group_id}/roles
async fn assign_group_global_role(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(group_id): Path<String>,
    Json(req): Json<AssignGroupRoleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let audit = mutate_group_global_role(&state, &admin, &group_id, &req.role_id, true).await?;
    crate::audit::append_actor_action(&audit).await;
    Ok(StatusCode::CREATED)
}

/// DELETE /api/v1/admin/groups/{group_id}/roles/{role_id}
async fn remove_group_global_role(
    State(state): State<AppState>,
    admin: AdminUser,
    Path((group_id, role_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let audit = mutate_group_global_role(&state, &admin, &group_id, &role_id, false).await?;
    crate::audit::append_actor_action(&audit).await;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/admin/groups
type GroupCrudRow = (
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    i64,
    Option<String>,
);
type GroupCrudPolicy = (String, String, String, String, String, String, String);
type GroupCrudAllowlist = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
);
type GroupCrudPage = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);
#[derive(Default)]
struct GroupCrudDependencies {
    maintenance: Vec<(String, String)>,
    global: Vec<String>,
    local: Vec<(String, String)>,
    policies: Vec<(String, String)>,
}
#[derive(PartialEq, Debug)]
struct GroupCrudSource {
    maintenance: Vec<serde_json::Value>,
    spaces: Vec<String>,
    group: Option<GroupCrudRow>,
    members: Vec<(String, String, String, String)>,
    grants: Vec<(String, String, String, String, String, String)>,
    roles: Vec<(String, String, String)>,
    global: Vec<(String, String, String, String, i64, String, String)>,
    local: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )>,
    policies: Vec<GroupCrudPolicy>,
    allowlist: Vec<GroupCrudAllowlist>,
    pages: Vec<GroupCrudPage>,
}
async fn group_crud_dependencies(
    conn: &mut sqlx::AnyConnection,
    group: &str,
) -> Result<GroupCrudDependencies, AppError> {
    Ok(GroupCrudDependencies {
        maintenance: sqlx::query_as("SELECT id,space_id FROM documents WHERE maintenance_group_id=$1 ORDER BY id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?,
        global: sqlx::query_scalar("SELECT role_id FROM group_global_roles WHERE group_id=$1 ORDER BY role_id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?,
        local: sqlx::query_as("SELECT DISTINCT p.space_id,p.role FROM space_group_permissions p JOIN space_roles r ON r.space_id=p.space_id AND r.id=p.role WHERE p.group_id=$1 ORDER BY p.space_id,p.role").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?,
        policies: sqlx::query_as("SELECT DISTINCT document_id,space_id FROM document_access_allowlist WHERE group_id=$1 ORDER BY document_id,space_id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?,
    })
}
fn group_crud_drift() -> AppError {
    AppError::Conflict("Audience source changed".into())
}
async fn group_crud_source(
    conn: &mut sqlx::AnyConnection,
    group: &str,
    deps: &GroupCrudDependencies,
) -> Result<GroupCrudSource, AppError> {
    let spaces = sqlx::query_scalar("SELECT id FROM spaces ORDER BY id")
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    let group: Option<GroupCrudRow> = sqlx::query_as("SELECT id,name,description,created_at,updated_at,external_id,scim_managed,managed_space_id FROM groups WHERE id=$1").bind(group).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let mut global = Vec::new();
    for id in &deps.global {
        global.push(sqlx::query_as("SELECT id,name,description,permissions,built_in,created_at,updated_at FROM global_roles WHERE id=$1").bind(id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?.ok_or_else(group_crud_drift)?);
    }
    let mut local = Vec::new();
    for (space, id) in &deps.local {
        local.push(sqlx::query_as("SELECT id,space_id,name,description,permissions,created_by,created_at,updated_at FROM space_roles WHERE space_id=$1 AND id=$2").bind(space).bind(id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?.ok_or_else(group_crud_drift)?);
    }
    let mut policies = Vec::new();
    let mut pages = Vec::new();
    let mut allowlist = Vec::new();
    for (doc, space) in &deps.policies {
        policies.push(sqlx::query_as("SELECT document_id,space_id,mode,revision,created_by,created_at,updated_at FROM document_access_policies WHERE document_id=$1 AND space_id=$2").bind(doc).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?.ok_or_else(group_crud_drift)?);
        pages.push(sqlx::query_as("SELECT id,space_id,parent_id,folder_id,published_at,archived_at,deleted_at,locked_by,created_by FROM documents WHERE id=$1 AND space_id=$2").bind(doc).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?.ok_or_else(group_crud_drift)?);
        let rows: Vec<GroupCrudAllowlist> = sqlx::query_as("SELECT id,document_id,space_id,user_id,group_id,created_by,created_at FROM document_access_allowlist WHERE document_id=$1 ORDER BY id").bind(doc).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
        if rows
            .iter()
            .any(|r| &r.2 != space || r.3.is_some() == r.4.is_some())
        {
            return Err(group_crud_drift());
        }
        allowlist.extend(rows);
    }
    let mut maintenance = Vec::new();
    for (id, space) in &deps.maintenance {
        let mut records = crate::spaces::maintenance::rows(
            conn,
            "SELECT * FROM documents WHERE id=$1 AND space_id=$2",
            &[id, space],
        )
        .await
        .map_err(|e| e)?;
        if records.len() != 1 {
            return Err(group_crud_drift());
        }
        maintenance.append(&mut records);
    }
    Ok(GroupCrudSource {
        maintenance,
        spaces,
        group,
        members: Vec::new(),
        grants: Vec::new(),
        roles: Vec::new(),
        global,
        local,
        policies,
        allowlist,
        pages,
    })
}
// Keep link queries keyed to the requested ID even after deletion, so a trigger
// cannot conceal a recreated association behind an absent canonical group.
async fn full_group_crud_source(
    conn: &mut sqlx::AnyConnection,
    group: &str,
    deps: &GroupCrudDependencies,
) -> Result<GroupCrudSource, AppError> {
    let mut source = group_crud_source(conn, group, deps).await?;
    source.members = sqlx::query_as(
        "SELECT id,group_id,user_id,created_at FROM group_members WHERE group_id=$1 ORDER BY id",
    )
    .bind(group)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    source.grants=sqlx::query_as("SELECT id,space_id,group_id,role,created_at,updated_at FROM space_group_permissions WHERE group_id=$1 ORDER BY id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    source.roles=sqlx::query_as("SELECT group_id,role_id,created_at FROM group_global_roles WHERE group_id=$1 ORDER BY role_id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let corrupt:Option<i64>=sqlx::query_scalar("SELECT 1 FROM group_members m LEFT JOIN users u ON u.id=m.user_id WHERE m.group_id=$1 AND u.id IS NULL UNION ALL SELECT 1 FROM space_group_permissions p LEFT JOIN spaces s ON s.id=p.space_id LEFT JOIN space_roles r ON r.id=p.role AND r.space_id=p.space_id WHERE p.group_id=$1 AND (s.id IS NULL OR (p.role NOT IN ('owner','space_admin','admin','editor','contributor','commenter','viewer') AND r.id IS NULL)) LIMIT 1").bind(group).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if corrupt.is_some() {
        return Err(group_crud_drift());
    }
    let actual_maintenance: Vec<(String, String)> = sqlx::query_as(
        "SELECT id,space_id FROM documents WHERE maintenance_group_id=$1 ORDER BY id",
    )
    .bind(group)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    if actual_maintenance
        .iter()
        .any(|p| !deps.maintenance.contains(p))
    {
        return Err(group_crud_drift());
    }
    let actual_policies:Vec<(String,String)>=sqlx::query_as("SELECT DISTINCT document_id,space_id FROM document_access_allowlist WHERE group_id=$1 ORDER BY document_id,space_id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    if actual_policies.iter().any(|p| !deps.policies.contains(p)) {
        return Err(group_crud_drift());
    }
    Ok(source)
}
enum GroupCrudChange<'a> {
    Create {
        name: &'a str,
        description: &'a Option<String>,
    },
    Update {
        name: &'a str,
        description: &'a Option<String>,
    },
    Delete,
}
async fn mutate_admin_group(
    state: &AppState,
    admin: &AdminUser,
    group: &str,
    change: GroupCrudChange<'_>,
) -> Result<Option<GroupResponse>, AppError> {
    use super::mutation_guards::{lock_audience_for_mutation, lock_spaces_for_mutation};
    use crate::auth::middleware::require_current_admin;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_audience_for_mutation(&mut tx).await?;
    let spaces: Vec<String> = sqlx::query_scalar("SELECT id FROM spaces ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    lock_spaces_for_mutation(
        &mut tx,
        &spaces.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await?;
    require_current_admin(&mut tx, admin).await?;
    let create = matches!(change, GroupCrudChange::Create { .. });
    let locked = sqlx::query("UPDATE groups SET id=id WHERE id=$1")
        .bind(group)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if (!create && locked.rows_affected() != 1) || (create && locked.rows_affected() != 0) {
        return Err(if create {
            group_crud_drift()
        } else {
            AppError::NotFound("Group not found".into())
        });
    }
    let deps = group_crud_dependencies(&mut tx, group).await?;
    for id in &deps.global {
        if sqlx::query("UPDATE global_roles SET id=id WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?
            .rows_affected()
            != 1
        {
            return Err(group_crud_drift());
        }
    }
    for (space, id) in &deps.local {
        if sqlx::query("UPDATE space_roles SET permissions=permissions WHERE space_id=$1 AND id=$2")
            .bind(space)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?
            .rows_affected()
            != 1
        {
            return Err(group_crud_drift());
        }
    }
    for (select, update) in [
        (
            "SELECT id FROM group_members WHERE group_id=$1 ORDER BY id",
            "UPDATE group_members SET id=id WHERE group_id=$1 AND id=$2",
        ),
        (
            "SELECT id FROM space_group_permissions WHERE group_id=$1 ORDER BY id",
            "UPDATE space_group_permissions SET id=id WHERE group_id=$1 AND id=$2",
        ),
        (
            "SELECT role_id FROM group_global_roles WHERE group_id=$1 ORDER BY role_id",
            "UPDATE group_global_roles SET group_id=group_id WHERE group_id=$1 AND role_id=$2",
        ),
    ] {
        let ids: Vec<String> = sqlx::query_scalar(select)
            .bind(group)
            .fetch_all(&mut *tx)
            .await
            .map_err(AppError::internal)?;
        for id in ids {
            if sqlx::query(update)
                .bind(group)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?
                .rows_affected()
                != 1
            {
                return Err(group_crud_drift());
            }
        }
    }
    for (doc, space) in &deps.maintenance {
        if sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2")
            .bind(doc)
            .bind(space)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?
            .rows_affected()
            != 1
        {
            return Err(group_crud_drift());
        }
    }
    for (doc, space) in &deps.policies {
        if sqlx::query("UPDATE documents SET title=title WHERE id=$1 AND space_id=$2")
            .bind(doc)
            .bind(space)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?
            .rows_affected()
            != 1
        {
            return Err(group_crud_drift());
        }
        if sqlx::query("UPDATE document_access_policies SET revision=revision WHERE document_id=$1 AND space_id=$2").bind(doc).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected()!=1 {return Err(group_crud_drift());}
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM document_access_allowlist WHERE document_id=$1 ORDER BY id",
        )
        .bind(doc)
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        for id in ids {
            if sqlx::query(
                "UPDATE document_access_allowlist SET id=id WHERE id=$1 AND document_id=$2",
            )
            .bind(id)
            .bind(doc)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?
            .rows_affected()
                != 1
            {
                return Err(group_crud_drift());
            }
        }
    }
    require_current_admin(&mut tx, admin).await?;
    let mut expected = full_group_crud_source(&mut tx, group, &deps).await?;
    if expected.spaces != spaces {
        return Err(group_crud_drift());
    }
    if create {
        if expected.group.is_some()
            || !expected.members.is_empty()
            || !expected.grants.is_empty()
            || !expected.roles.is_empty()
            || !deps.policies.is_empty()
        {
            return Err(group_crud_drift());
        }
    } else {
        let row = expected.group.as_ref().ok_or_else(group_crud_drift)?;
        if row.6 != 0 {
            return Err(AppError::Conflict(
                "This group is managed by SCIM. Update it through the identity provider.".into(),
            ));
        }
        if row.7.is_some() {
            return Err(AppError::Conflict("A space's dedicated group is managed from that space and cannot be renamed, detached, or deleted.".into()));
        }
    }
    let action = match change {
        GroupCrudChange::Create { name, description } => {
            // Choose the canonical timestamps only after all consumer locks.
            let time = chrono::Utc::now().to_rfc3339();
            if sqlx::query("INSERT INTO groups(id,name,description,created_at,updated_at) VALUES($1,$2,$3,$4,$4)").bind(group).bind(name).bind(description).bind(&time).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected()!=1 {return Err(group_crud_drift());}
            expected.group = Some((
                group.into(),
                name.into(),
                description.clone(),
                time.clone(),
                time,
                None,
                0,
                None,
            ));
            "group.create"
        }
        GroupCrudChange::Update { name, description } => {
            let time = chrono::Utc::now().to_rfc3339();
            if sqlx::query("UPDATE groups SET name=$1,description=$2,updated_at=$3 WHERE id=$4")
                .bind(name)
                .bind(description)
                .bind(&time)
                .bind(group)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?
                .rows_affected()
                != 1
            {
                return Err(group_crud_drift());
            }
            let row = expected.group.as_mut().ok_or_else(group_crud_drift)?;
            row.1 = name.into();
            row.2 = description.clone();
            row.4 = time;
            "group.update"
        }
        GroupCrudChange::Delete => {
            for (sql, count) in [
                (
                    "DELETE FROM space_group_permissions WHERE group_id=$1",
                    expected.grants.len(),
                ),
                (
                    "DELETE FROM group_members WHERE group_id=$1",
                    expected.members.len(),
                ),
            ] {
                if sqlx::query(sql)
                    .bind(group)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::internal)?
                    .rows_affected()
                    != count as u64
                {
                    return Err(group_crud_drift());
                }
            }
            if sqlx::query("DELETE FROM groups WHERE id=$1")
                .bind(group)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?
                .rows_affected()
                != 1
            {
                return Err(group_crud_drift());
            }
            for page in &mut expected.maintenance {
                page["maintenance_group_id"] = serde_json::Value::Null;
            }
            expected.group = None;
            expected.members.clear();
            expected.grants.clear();
            expected.roles.clear();
            expected
                .allowlist
                .retain(|row| row.4.as_deref() != Some(group));
            "group.delete"
        }
    };
    let authority = require_current_admin(&mut tx, admin).await?;
    let event =
        crate::audit::insert_actor_action(&mut tx, &authority, action, "group", group).await?;
    let authority = require_current_admin(&mut tx, admin).await?;
    if full_group_crud_source(&mut tx, group, &deps).await? != expected {
        return Err(group_crud_drift());
    }
    crate::audit::verify_actor_action(&mut tx, &event, &authority).await?;
    #[cfg(test)]
    after_audience_equality_proofs().await;
    require_current_admin(&mut tx, admin).await?;
    let response = expected.group.map(|row| GroupResponse {
        id: row.0,
        name: row.1,
        description: row.2,
        member_count: 0,
        space_grant_count: 0,
        scim_managed: false,
        managed_space_id: None,
        created_at: row.3,
        updated_at: row.4,
    });
    tx.commit().await.map_err(AppError::internal)?;
    crate::audit::append_actor_action(&event).await;
    Ok(response)
}

fn validate_admin_group_name(name: &str) -> Result<&str, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("Group name is required".into()));
    }
    if name.len() > 100 {
        return Err(AppError::BadRequest(
            "Group name must be at most 100 characters".into(),
        ));
    }
    Ok(name)
}
async fn create_group(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(req): Json<CreateGroupRequest>,
) -> Result<impl IntoResponse, AppError> {
    let name = validate_admin_group_name(&req.name)?;
    let id = Uuid::new_v4().to_string();
    let response = mutate_admin_group(
        &state,
        &admin,
        &id,
        GroupCrudChange::Create {
            name,
            description: &req.description,
        },
    )
    .await?
    .ok_or_else(group_crud_drift)?;
    Ok((StatusCode::CREATED, Json(response)))
}
/// PUT /api/v1/admin/groups/{group_id}
async fn update_group(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(group): Path<String>,
    Json(req): Json<UpdateGroupRequest>,
) -> Result<impl IntoResponse, AppError> {
    let name = validate_admin_group_name(&req.name)?;
    mutate_admin_group(
        &state,
        &admin,
        &group,
        GroupCrudChange::Update {
            name,
            description: &req.description,
        },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
/// DELETE /api/v1/admin/groups/{group_id}
async fn delete_group(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(group): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    mutate_admin_group(&state, &admin, &group, GroupCrudChange::Delete).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/admin/groups/{group_id}/spaces
async fn list_group_space_grants(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(group_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let grants: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT s.id, s.name, s.slug, sgp.role \
         FROM space_group_permissions sgp \
         JOIN spaces s ON s.id = sgp.space_id \
         WHERE sgp.group_id = $1 \
         ORDER BY s.name",
    )
    .bind(&group_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(Json(
        grants
            .into_iter()
            .map(|(space_id, name, slug, role)| GroupSpaceGrantResponse {
                space_id,
                name,
                slug,
                role,
            })
            .collect::<Vec<_>>(),
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
         WHERE gm.group_id = $1 \
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct AdminGroupMemberSource {
    spaces: Vec<String>,
    group: (
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        i64,
        Option<String>,
    ),
    global_definitions: Vec<(String, String, String, String, i64, String, String)>,
    local_definitions: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )>,
    grants: Vec<(String, String, String, String, String, String)>,
    roles: Vec<(String, String, String)>,
    members: Vec<(String, String, String, String)>,
}
async fn admin_group_member_source(
    conn: &mut sqlx::AnyConnection,
    group: &str,
) -> Result<AdminGroupMemberSource, AppError> {
    let spaces = sqlx::query_scalar("SELECT id FROM spaces ORDER BY id")
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    let group_row=sqlx::query_as("SELECT id,name,description,created_at,updated_at,external_id,scim_managed,managed_space_id FROM groups WHERE id=$1").bind(group).fetch_optional(&mut *conn).await.map_err(AppError::internal)?.ok_or_else(||AppError::NotFound("Group not found".into()))?;
    let global_definitions=sqlx::query_as("SELECT r.id,r.name,r.description,r.permissions,r.built_in,r.created_at,r.updated_at FROM global_roles r WHERE EXISTS(SELECT 1 FROM group_global_roles g WHERE g.group_id=$1 AND g.role_id=r.id) ORDER BY r.id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let local_definitions=sqlx::query_as("SELECT r.id,r.space_id,r.name,r.description,r.permissions,r.created_by,r.created_at,r.updated_at FROM space_roles r WHERE EXISTS(SELECT 1 FROM space_group_permissions g WHERE g.group_id=$1 AND g.space_id=r.space_id AND g.role=r.id) ORDER BY r.space_id,r.id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let orphan:Option<i64>=sqlx::query_scalar("SELECT 1 FROM group_global_roles g LEFT JOIN global_roles r ON r.id=g.role_id WHERE g.group_id=$1 AND r.id IS NULL LIMIT 1").bind(group).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let bad_grant:Option<i64>=sqlx::query_scalar("SELECT 1 FROM space_group_permissions p LEFT JOIN spaces s ON s.id=p.space_id LEFT JOIN space_roles r ON r.id=p.role AND r.space_id=p.space_id WHERE p.group_id=$1 AND (s.id IS NULL OR (p.role NOT IN ('owner','space_admin','admin','editor','contributor','commenter','viewer') AND r.id IS NULL)) LIMIT 1").bind(group).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if orphan.is_some() || bad_grant.is_some() {
        return Err(AppError::Conflict("Audience source is inconsistent".into()));
    }
    let grants=sqlx::query_as("SELECT id,space_id,group_id,role,created_at,updated_at FROM space_group_permissions WHERE group_id=$1 ORDER BY id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let roles=sqlx::query_as("SELECT group_id,role_id,created_at FROM group_global_roles WHERE group_id=$1 ORDER BY role_id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let members = sqlx::query_as(
        "SELECT id,group_id,user_id,created_at FROM group_members WHERE group_id=$1 ORDER BY id",
    )
    .bind(group)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    Ok(AdminGroupMemberSource {
        spaces,
        group: group_row,
        global_definitions,
        local_definitions,
        grants,
        roles,
        members,
    })
}
/// Broad group membership requires real instance/service-admin authority, never
/// a URL-space manager, creator attribution, or a strongest global role label.
async fn mutate_admin_group_member(
    state: &AppState,
    admin: &AdminUser,
    group: &str,
    user: &str,
    add: bool,
) -> Result<(), AppError> {
    use super::mutation_guards::{lock_audience_for_mutation, lock_spaces_for_mutation};
    use crate::auth::middleware::require_current_admin;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_audience_for_mutation(&mut tx).await?;
    let spaces: Vec<String> = sqlx::query_scalar("SELECT id FROM spaces ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    lock_spaces_for_mutation(
        &mut tx,
        &spaces.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await?;
    require_current_admin(&mut tx, admin).await?;
    let rows = sqlx::query("UPDATE groups SET id=id WHERE id=$1")
        .bind(group)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if rows.rows_affected() != 1 {
        return Err(AppError::NotFound("Group not found".into()));
    }
    let global_ids:Vec<String>=sqlx::query_scalar("SELECT r.id FROM global_roles r WHERE EXISTS(SELECT 1 FROM group_global_roles g WHERE g.group_id=$1 AND g.role_id=r.id) ORDER BY r.id").bind(group).fetch_all(&mut *tx).await.map_err(AppError::internal)?;
    for id in global_ids {
        let rows = sqlx::query("UPDATE global_roles SET id=id WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
        if rows.rows_affected() != 1 {
            return Err(AppError::Conflict("Audience source changed".into()));
        }
    }
    let local_ids:Vec<(String,String)>=sqlx::query_as("SELECT r.space_id,r.id FROM space_roles r WHERE EXISTS(SELECT 1 FROM space_group_permissions p WHERE p.group_id=$1 AND p.space_id=r.space_id AND p.role=r.id) ORDER BY r.space_id,r.id").bind(group).fetch_all(&mut *tx).await.map_err(AppError::internal)?;
    for (space, id) in local_ids {
        let rows = sqlx::query(
            "UPDATE space_roles SET permissions=permissions WHERE space_id=$1 AND id=$2",
        )
        .bind(space)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        if rows.rows_affected() != 1 {
            return Err(AppError::Conflict("Audience source changed".into()));
        }
    }
    for (select, update) in [
        (
            "SELECT id FROM group_members WHERE group_id=$1 ORDER BY id",
            "UPDATE group_members SET id=id WHERE group_id=$1 AND id=$2",
        ),
        (
            "SELECT id FROM space_group_permissions WHERE group_id=$1 ORDER BY id",
            "UPDATE space_group_permissions SET id=id WHERE group_id=$1 AND id=$2",
        ),
        (
            "SELECT role_id FROM group_global_roles WHERE group_id=$1 ORDER BY role_id",
            "UPDATE group_global_roles SET group_id=group_id WHERE group_id=$1 AND role_id=$2",
        ),
    ] {
        let ids: Vec<String> = sqlx::query_scalar(select)
            .bind(group)
            .fetch_all(&mut *tx)
            .await
            .map_err(AppError::internal)?;
        for id in ids {
            let rows = sqlx::query(update)
                .bind(group)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
            if rows.rows_affected() != 1 {
                return Err(AppError::Conflict("Audience source changed".into()));
            }
        }
    }
    let authority = require_current_admin(&mut tx, admin).await?;
    let mut expected = admin_group_member_source(&mut tx, group).await?;
    if expected.spaces != spaces {
        return Err(AppError::Conflict("Audience source changed".into()));
    }
    if expected.group.6 != 0 {
        return Err(AppError::Conflict(
            "This group is managed by SCIM. Update it through the identity provider.".into(),
        ));
    }
    let target = expected.members.iter().find(|m| m.2 == user).cloned();
    if add {
        let inserted:Option<(String,String)>=sqlx::query_as("INSERT INTO group_members(id,group_id,user_id) VALUES($1,$2,$3) ON CONFLICT(group_id,user_id) DO NOTHING RETURNING id,created_at").bind(Uuid::new_v4().to_string()).bind(group).bind(user).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
        match (target.is_some(), inserted) {
            (false, Some((id, time))) => {
                expected.members.push((id, group.into(), user.into(), time));
                expected.members.sort_by(|a, b| a.0.cmp(&b.0));
            }
            (true, None) => {}
            _ => return Err(AppError::Conflict("Group membership target changed".into())),
        }
    } else {
        let rows = sqlx::query("DELETE FROM group_members WHERE group_id=$1 AND user_id=$2")
            .bind(group)
            .bind(user)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
        if rows.rows_affected() != u64::from(target.is_some()) {
            return Err(AppError::Conflict("Group membership target changed".into()));
        }
        expected.members.retain(|m| m.2 != user);
    }
    let event = crate::audit::insert_actor_action(
        &mut tx,
        &authority,
        if add {
            "group.member_add"
        } else {
            "group.member_remove"
        },
        "group",
        group,
    )
    .await?;
    let current = require_current_admin(&mut tx, admin).await?;
    if admin_group_member_source(&mut tx, group).await? != expected {
        return Err(AppError::Conflict("Audience source changed".into()));
    }
    crate::audit::verify_actor_action(&mut tx, &event, &current).await?;
    #[cfg(test)]
    after_audience_equality_proofs().await;
    require_current_admin(&mut tx, admin).await?;
    tx.commit().await.map_err(AppError::internal)?;
    crate::audit::append_actor_action(&event).await;
    Ok(())
}
/// POST /api/v1/admin/groups/{group_id}/members
async fn add_group_member(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(group_id): Path<String>,
    Json(req): Json<AddGroupMemberRequest>,
) -> Result<impl IntoResponse, AppError> {
    mutate_admin_group_member(&state, &admin, &group_id, &req.user_id, true).await?;
    Ok(StatusCode::CREATED)
}
/// DELETE /api/v1/admin/groups/{group_id}/members/{user_id}
async fn remove_group_member(
    State(state): State<AppState>,
    admin: AdminUser,
    Path((group_id, user_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    mutate_admin_group_member(&state, &admin, &group_id, &user_id, false).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/spaces/{space_id}/groups
/// A management catalog is read-only: absent managed groups are not recreated here.
async fn require_space_groups_read(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
) -> Result<(), AppError> {
    super::access::require_space_management_catalog_read(conn, auth, space).await
}
type SpaceGroupCatalogRow = (
    String,
    String,
    Option<String>,
    String,
    i64,
    String,
    Option<String>,
);
async fn space_group_catalog_rows(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    group: Option<&str>,
) -> Result<Vec<SpaceGroupCatalogRow>, AppError> {
    sqlx::query_as("SELECT g.id,g.name,g.description,sgp.role,COUNT(gm.user_id),sgp.created_at,g.managed_space_id FROM space_group_permissions sgp JOIN groups g ON g.id=sgp.group_id LEFT JOIN group_members gm ON gm.group_id=g.id WHERE sgp.space_id=$1 AND ($2 IS NULL OR g.id=$2) GROUP BY g.id,g.name,g.description,sgp.role,sgp.created_at,g.managed_space_id ORDER BY CASE WHEN g.managed_space_id IS NOT NULL THEN 0 ELSE 1 END,g.name,g.id")
        .bind(space).bind(group).fetch_all(conn).await.map_err(AppError::internal)
}
async fn list_space_groups(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Autocommit reads observe external WAL changes after awaits; no stale read TX.
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_space_groups_read(&mut conn, &auth, &space_id).await?;
    let rows = space_group_catalog_rows(&mut conn, &space_id, None).await?;
    #[cfg(test)]
    catalog_after_load().await;
    require_space_groups_read(&mut conn, &auth, &space_id).await?;
    #[cfg(test)]
    catalog_after_space_view().await;
    require_space_groups_read(&mut conn, &auth, &space_id).await?;
    let mut result = Vec::new();
    for row in rows {
        // Final exact grant/group projection: moved, deleted or changed rows never
        // emit their earlier names, role or member count.
        let current = space_group_catalog_rows(&mut conn, &space_id, Some(&row.0)).await?;
        if current.first() != Some(&row) {
            continue;
        }
        let (group_id, name, description, role, member_count, granted_at, managed_space_id) = row;
        result.push(SpaceGroupResponse {
            group_id,
            name,
            description,
            role,
            member_count,
            granted_at,
            managed: managed_space_id.as_deref() == Some(space_id.as_str()),
        });
    }
    Ok(Json(result))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ScopedGroupGrantSource {
    group: Option<(
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        i64,
        Option<String>,
    )>,
    grants: Vec<(String, String, String, String, String, String)>,
    members: Vec<(String, String, String, String)>,
    globals: Vec<(String, String, String)>,
    global_definitions: Vec<(String, String, String, String, i64, String, String)>,
    local_definitions: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )>,
}
async fn scoped_group_grant_source(
    conn: &mut sqlx::AnyConnection,
    group: &str,
    space: &str,
    requested: Option<&str>,
) -> Result<ScopedGroupGrantSource, AppError> {
    let group_row=sqlx::query_as("SELECT id,name,description,created_at,updated_at,external_id,scim_managed,managed_space_id FROM groups WHERE id=$1").bind(group).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let grants:Vec<(String,String,String,String,String,String)>=sqlx::query_as("SELECT id,space_id,group_id,role,created_at,updated_at FROM space_group_permissions WHERE group_id=$1 ORDER BY id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let members: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT id,group_id,user_id,created_at FROM group_members WHERE group_id=$1 ORDER BY id",
    )
    .bind(group)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let globals:Vec<(String,String,String)>=sqlx::query_as("SELECT group_id,role_id,created_at FROM group_global_roles WHERE group_id=$1 ORDER BY role_id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let global_definitions:Vec<(String,String,String,String,i64,String,String)>=sqlx::query_as("SELECT r.id,r.name,r.description,r.permissions,r.built_in,r.created_at,r.updated_at FROM global_roles r WHERE EXISTS(SELECT 1 FROM group_global_roles g WHERE g.group_id=$1 AND g.role_id=r.id) ORDER BY r.id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let local_definitions:Vec<(String,String,String,String,String,String,String,String)>=sqlx::query_as("SELECT r.id,r.space_id,r.name,r.description,r.permissions,r.created_by,r.created_at,r.updated_at FROM space_roles r WHERE EXISTS(SELECT 1 FROM space_group_permissions g WHERE g.group_id=$1 AND g.space_id=r.space_id AND g.role=r.id) OR (r.space_id=$2 AND r.id=$3) ORDER BY r.space_id,r.id").bind(group).bind(space).bind(requested).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    if group_row.is_none() && (!grants.is_empty() || !members.is_empty() || !globals.is_empty()) {
        return Err(AppError::Conflict("Group grant source unavailable".into()));
    }
    if globals
        .iter()
        .any(|g| !global_definitions.iter().any(|r| r.0 == g.1))
        || grants.iter().any(|g| {
            !matches!(
                g.3.as_str(),
                "owner"
                    | "admin"
                    | "space_admin"
                    | "editor"
                    | "contributor"
                    | "commenter"
                    | "viewer"
            ) && !local_definitions.iter().any(|r| r.0 == g.3 && r.1 == g.1)
        })
    {
        return Err(AppError::Conflict("Group grant source unavailable".into()));
    }
    Ok(ScopedGroupGrantSource {
        group: group_row,
        grants,
        members,
        globals,
        global_definitions,
        local_definitions,
    })
}
#[derive(Clone, Copy)]
enum ScopedGroupGrantChange<'a> {
    Upsert(&'a str),
    Update(&'a str),
    Remove,
}
impl ScopedGroupGrantChange<'_> {
    fn role(&self) -> Option<&str> {
        match self {
            Self::Upsert(r) | Self::Update(r) => Some(r),
            Self::Remove => None,
        }
    }
}
async fn mutate_scoped_group_grant(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    group: &str,
    change: ScopedGroupGrantChange<'_>,
) -> Result<(), AppError> {
    use super::{access, mutation_guards};
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    mutation_guards::lock_spaces_for_mutation(&mut tx, &[space])
        .await
        .map_err(|e| match e {
            AppError::NotFound(_) => AppError::NotFound("Target unavailable".into()),
            other => other,
        })?;
    access::require_space_membership_management(&mut tx, auth, space).await?;
    sqlx::query("UPDATE groups SET id=id WHERE id=$1")
        .bind(group)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    let initial = scoped_group_grant_source(&mut tx, group, space, change.role()).await?;
    // Definition locks precede association locks, consistently with global and
    // confined-member writers. Other spaces are evidence, not extra authority.
    for r in &initial.global_definitions {
        sqlx::query("UPDATE global_roles SET id=id WHERE id=$1")
            .bind(&r.0)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    }
    for r in &initial.local_definitions {
        sqlx::query("UPDATE space_roles SET id=id WHERE space_id=$1 AND id=$2")
            .bind(&r.1)
            .bind(&r.0)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    }
    for r in &initial.members {
        sqlx::query("UPDATE group_members SET id=id WHERE group_id=$1 AND id=$2")
            .bind(group)
            .bind(&r.0)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    }
    for r in &initial.grants {
        sqlx::query("UPDATE space_group_permissions SET id=id WHERE group_id=$1 AND id=$2")
            .bind(group)
            .bind(&r.0)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    }
    for r in &initial.globals {
        sqlx::query(
            "UPDATE group_global_roles SET role_id=role_id WHERE group_id=$1 AND role_id=$2",
        )
        .bind(group)
        .bind(&r.1)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    }
    let mut expected = scoped_group_grant_source(&mut tx, group, space, change.role()).await?;
    if expected != initial {
        return Err(AppError::Conflict("Group grant source changed".into()));
    }
    if matches!(change, ScopedGroupGrantChange::Remove)
        && expected.group.as_ref().and_then(|g| g.7.as_deref()) == Some(space)
    {
        return Err(AppError::Conflict(
            "The dedicated space group cannot be detached. Change its role or membership instead."
                .into(),
        ));
    }
    let ceiling = access::require_space_membership_management(&mut tx, auth, space).await?;
    if let Some(role) = change.role() {
        if !access::space_membership_role_permissions(&mut tx, space, role)
            .await?
            .is_subset(&ceiling)
        {
            return Err(AppError::Forbidden(
                "Cannot grant permissions outside your space authority".into(),
            ));
        }
    }
    let actor_member = expected.members.iter().any(|m| m.2 == auth.claims.sub);
    let mut roles:Vec<String>=sqlx::query_scalar("SELECT role FROM space_members WHERE space_id=$1 AND user_id=$2 UNION SELECT p.role FROM space_group_permissions p JOIN group_members m ON m.group_id=p.group_id WHERE p.space_id=$1 AND m.user_id=$2").bind(space).bind(&auth.claims.sub).fetch_all(&mut *tx).await.map_err(AppError::internal)?;
    if let Some(role) = change.role() {
        roles.push(role.into());
    }
    roles.sort();
    roles.dedup();
    let mut actor_expected =
        audience_policy_snapshot(&mut tx, space, &auth.claims.sub, &roles).await?;
    let old = expected.grants.iter().find(|g| g.1 == space).cloned();
    // The real account/key approval remains current immediately before SQL.
    let ceiling = access::require_space_membership_management(&mut tx, auth, space).await?;
    if let Some(role) = change.role() {
        if !access::space_membership_role_permissions(&mut tx, space, role)
            .await?
            .is_subset(&ceiling)
        {
            return Err(AppError::Forbidden(
                "Cannot grant permissions outside your space authority".into(),
            ));
        }
    }
    match change {
        ScopedGroupGrantChange::Upsert(role) => {
            let now = chrono::Utc::now().to_rfc3339();
            let inserted:(String,String,String)=sqlx::query_as("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES($1,$2,$3,$4) ON CONFLICT(space_id,group_id) DO UPDATE SET role=excluded.role,updated_at=$5 RETURNING id,created_at,updated_at").bind(Uuid::new_v4().to_string()).bind(space).bind(group).bind(role).bind(&now).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
            if let Some(old) = &old {
                if old.0 != inserted.0 || old.4 != inserted.1 {
                    return Err(AppError::Conflict("Group grant target changed".into()));
                }
                let g = expected.grants.iter_mut().find(|g| g.1 == space).unwrap();
                g.3 = role.into();
                g.5 = now;
            } else {
                let stamp = inserted.2;
                expected.grants.push((
                    inserted.0,
                    space.into(),
                    group.into(),
                    role.into(),
                    inserted.1,
                    stamp,
                ));
                expected.grants.sort_by(|a, b| a.0.cmp(&b.0));
            }
        }
        ScopedGroupGrantChange::Update(role) => {
            let now = chrono::Utc::now().to_rfc3339();
            let rows=sqlx::query("UPDATE space_group_permissions SET role=$1,updated_at=$2 WHERE space_id=$3 AND group_id=$4").bind(role).bind(&now).bind(space).bind(group).execute(&mut *tx).await.map_err(AppError::internal)?;
            if rows.rows_affected() != u64::from(old.is_some()) {
                return Err(AppError::Conflict("Group grant target changed".into()));
            }
            if let Some(g) = expected.grants.iter_mut().find(|g| g.1 == space) {
                g.3 = role.into();
                g.5 = now;
            }
        }
        ScopedGroupGrantChange::Remove => {
            let rows = sqlx::query(
                "DELETE FROM space_group_permissions WHERE space_id=$1 AND group_id=$2",
            )
            .bind(space)
            .bind(group)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
            if rows.rows_affected() != u64::from(old.is_some()) {
                return Err(AppError::Conflict("Group grant target changed".into()));
            }
            expected.grants.retain(|g| g.1 != space);
        }
    }
    if actor_member {
        actor_expected.grants.retain(|g| g.0 != group);
        if let Some(g) = expected.grants.iter().find(|g| g.1 == space) {
            actor_expected.grants.push((group.into(), g.3.clone()));
            actor_expected.grants.sort();
        }
    }
    if actor_member {
        access::require_current_human_preference_write(&mut tx, auth).await?;
    } else {
        let current = access::require_space_membership_management(&mut tx, auth, space).await?;
        if let Some(role) = change.role() {
            if !access::space_membership_role_permissions(&mut tx, space, role)
                .await?
                .is_subset(&current)
            {
                return Err(AppError::Forbidden(
                    "Cannot grant permissions outside your space authority".into(),
                ));
            }
        }
    }
    // Preserve definition evidence even when a grant is removed/replaced: the
    // expected full raw tuple must not lose the old role policy from its proof.
    let current = scoped_group_grant_source(&mut tx, group, space, change.role()).await?;
    let mut expected_projection = expected.clone();
    expected_projection.local_definitions.retain(|r| {
        current
            .local_definitions
            .iter()
            .any(|v| v.0 == r.0 && v.1 == r.1)
    });
    if current != expected_projection {
        return Err(AppError::Conflict("Group grant source changed".into()));
    }
    for r in &expected.local_definitions {
        let row:Option<(String,String,String,String,String,String,String,String)>=sqlx::query_as("SELECT id,space_id,name,description,permissions,created_by,created_at,updated_at FROM space_roles WHERE id=$1 AND space_id=$2").bind(&r.0).bind(&r.1).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
        if row.as_ref() != Some(r) {
            return Err(AppError::Conflict("Group grant role changed".into()));
        }
    }
    if audience_policy_snapshot(&mut tx, space, &auth.claims.sub, &roles).await? != actor_expected {
        return Err(AppError::Conflict(
            "Space access changed during approval".into(),
        ));
    }
    // Reprove the real write credential after all source/policy reads.
    // Intentional self-loss does not renew a ManageSpace capability.
    #[cfg(test)]
    after_audience_equality_proofs().await;
    access::require_current_human_preference_write(&mut tx, auth).await?;
    // No new global membership or service-as-human authority is created here.
    tx.commit().await.map_err(AppError::internal)?;
    Ok(())
}

/// POST /api/v1/spaces/{space_id}/groups
async fn grant_group_to_space(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<SpaceGroupRequest>,
) -> Result<impl IntoResponse, AppError> {
    mutate_scoped_group_grant(
        &state,
        &auth,
        &space_id,
        &req.group_id,
        ScopedGroupGrantChange::Upsert(req.role.as_deref().unwrap_or("viewer")),
    )
    .await?;

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
    mutate_scoped_group_grant(
        &state,
        &auth,
        &space_id,
        &group_id,
        ScopedGroupGrantChange::Update(req.role.as_deref().unwrap_or("viewer")),
    )
    .await?;

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
    mutate_scoped_group_grant(
        &state,
        &auth,
        &space_id,
        &group_id,
        ScopedGroupGrantChange::Remove,
    )
    .await?;

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

async fn require_group_granted_to_space_on_connection(
    conn: &mut sqlx::AnyConnection,
    space_id: &str,
    group_id: &str,
) -> Result<(), AppError> {
    let exists: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM space_group_permissions WHERE space_id = $1 AND group_id = $2",
    )
    .bind(space_id)
    .bind(group_id)
    .fetch_optional(conn)
    .await
    .map_err(AppError::internal)?;
    if exists.is_none() {
        return Err(AppError::NotFound(
            "Group is not assigned to this space".into(),
        ));
    }
    Ok(())
}

async fn list_space_group_members(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, group_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_space_groups_read(&mut conn, &auth, &space_id).await?;
    require_group_granted_to_space_on_connection(&mut conn, &space_id, &group_id).await?;

    let members: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT u.id, u.email, u.display_name, u.role, gm.created_at \
         FROM group_members gm JOIN users u ON u.id = gm.user_id \
         WHERE gm.group_id = $1 ORDER BY u.display_name",
    )
    .bind(&group_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    #[cfg(test)]
    catalog_after_load().await;
    require_space_groups_read(&mut conn, &auth, &space_id).await?;
    require_group_granted_to_space_on_connection(&mut conn, &space_id, &group_id).await?;
    super::access::require_current_human_read(&mut conn, &auth).await?;
    Ok(Json(
        members
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
            .collect::<Vec<_>>(),
    ))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SoleSpaceGroupSource {
    group: (
        String,
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        i64,
        Option<String>,
    ),
    grants: Vec<(String, String, String, String, String, String)>,
    globals: Vec<(String, String, String)>,
    members: Vec<(String, String, String, String)>,
    role_policy: Option<String>,
}
async fn sole_space_group_source(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    group: &str,
) -> Result<SoleSpaceGroupSource, AppError> {
    let row=sqlx::query_as("SELECT g.id,g.name,g.description,g.created_at,g.updated_at,g.external_id,g.scim_managed,g.managed_space_id FROM groups g WHERE g.id=$1 AND EXISTS(SELECT 1 FROM space_group_permissions p WHERE p.group_id=g.id AND p.space_id=$2)")
        .bind(group).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?.ok_or_else(||AppError::NotFound("Target unavailable".into()))?;
    let grants:Vec<(String,String,String,String,String,String)>=sqlx::query_as("SELECT id,space_id,group_id,role,created_at,updated_at FROM space_group_permissions WHERE group_id=$1 ORDER BY id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let globals=sqlx::query_as("SELECT group_id,role_id,created_at FROM group_global_roles WHERE group_id=$1 ORDER BY role_id").bind(group).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let members = sqlx::query_as(
        "SELECT id,group_id,user_id,created_at FROM group_members WHERE group_id=$1 ORDER BY id",
    )
    .bind(group)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let role_policy = if grants.len() == 1 {
        sqlx::query_scalar("SELECT permissions FROM space_roles WHERE space_id=$1 AND id=$2")
            .bind(space)
            .bind(&grants[0].3)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?
    } else {
        None
    };
    Ok(SoleSpaceGroupSource {
        group: row,
        grants,
        globals,
        members,
        role_policy,
    })
}
fn validate_sole_space_group(source: &SoleSpaceGroupSource, space: &str) -> Result<(), AppError> {
    if source
        .group
        .7
        .as_deref()
        .is_some_and(|bound| bound != space)
    {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    if source.group.6 != 0 {
        return Err(AppError::Conflict(
            "This group is managed by SCIM. Update it through the identity provider.".into(),
        ));
    }
    if !source.globals.is_empty() || source.grants.len() != 1 || source.grants[0].1 != space {
        return Err(AppError::Conflict(
            "Group membership affects additional access. Use an instance-admin group endpoint."
                .into(),
        ));
    }
    Ok(())
}
async fn group_grant_permission_ceiling(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    role: &str,
) -> Result<std::collections::BTreeSet<super::access::Permission>, AppError> {
    // Evaluate existing legacy group Owner/Admin aliases, never grant a NEW
    // direct Owner role or rewrite the stored association's role.
    super::access::space_membership_role_permissions(
        conn,
        space,
        if matches!(role, "owner" | "admin") {
            "space_admin"
        } else {
            role
        },
    )
    .await
}
async fn mutate_sole_space_group_member(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    group: &str,
    user: &str,
    add: bool,
) -> Result<(), AppError> {
    use super::{access, mutation_guards};
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    mutation_guards::lock_spaces_for_mutation(&mut tx, &[space])
        .await
        .map_err(|error| match error {
            AppError::NotFound(_) => AppError::NotFound("Target unavailable".into()),
            other => other,
        })?;
    access::require_space_membership_management(&mut tx, auth, space).await?;
    let rows=sqlx::query("UPDATE groups SET id=id WHERE id=$1 AND EXISTS(SELECT 1 FROM space_group_permissions p WHERE p.group_id=groups.id AND p.space_id=$2)").bind(group).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    if rows.rows_affected() != 1 {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    let scope = sole_space_group_source(&mut tx, space, group).await?;
    validate_sole_space_group(&scope, space)?;
    let role = scope.grants[0].3.clone();
    sqlx::query("UPDATE space_roles SET permissions=permissions WHERE space_id=$1 AND id=$2")
        .bind(space)
        .bind(&role)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    for (select, update) in [
        (
            "SELECT id FROM group_members WHERE group_id=$1 ORDER BY id",
            "UPDATE group_members SET id=id WHERE group_id=$1 AND id=$2",
        ),
        (
            "SELECT id FROM space_group_permissions WHERE group_id=$1 ORDER BY id",
            "UPDATE space_group_permissions SET id=id WHERE group_id=$1 AND id=$2",
        ),
    ] {
        let ids: Vec<String> = sqlx::query_scalar(select)
            .bind(group)
            .fetch_all(&mut *tx)
            .await
            .map_err(AppError::internal)?;
        for id in ids {
            let rows = sqlx::query(update)
                .bind(group)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
            if rows.rows_affected() != 1 {
                return Err(AppError::Conflict("Group membership source changed".into()));
            }
        }
    }
    let mut expected = sole_space_group_source(&mut tx, space, group).await?;
    if expected != scope {
        return Err(AppError::Conflict("Group membership source changed".into()));
    }
    validate_sole_space_group(&expected, space)?;
    let ceiling = access::require_space_membership_management(&mut tx, auth, space).await?;
    if add
        && !group_grant_permission_ceiling(&mut tx, space, &role)
            .await?
            .is_subset(&ceiling)
    {
        return Err(AppError::Forbidden(
            "Cannot grant permissions outside your space authority".into(),
        ));
    }
    let self_remove = !add && user == auth.claims.sub;
    let mut actor_roles:Vec<String>=sqlx::query_scalar("SELECT role FROM space_members WHERE space_id=$1 AND user_id=$2 UNION SELECT p.role FROM space_group_permissions p JOIN group_members m ON m.group_id=p.group_id WHERE p.space_id=$1 AND m.user_id=$2").bind(space).bind(&auth.claims.sub).fetch_all(&mut *tx).await.map_err(AppError::internal)?;
    actor_roles.push(role.clone());
    actor_roles.sort();
    actor_roles.dedup();
    let mut actor_expected = if self_remove {
        Some(audience_policy_snapshot(&mut tx, space, &auth.claims.sub, &actor_roles).await?)
    } else {
        None
    };
    // Approval is current after all blocking locks, before canonical membership.
    let ceiling = access::require_space_membership_management(&mut tx, auth, space).await?;
    if add
        && !group_grant_permission_ceiling(&mut tx, space, &role)
            .await?
            .is_subset(&ceiling)
    {
        return Err(AppError::Forbidden(
            "Cannot grant permissions outside your space authority".into(),
        ));
    }
    let target = expected.members.iter().find(|m| m.2 == user).cloned();
    if add {
        let inserted:Option<(String,String)>=sqlx::query_as("INSERT INTO group_members(id,group_id,user_id) VALUES($1,$2,$3) ON CONFLICT(group_id,user_id) DO NOTHING RETURNING id,created_at").bind(Uuid::new_v4().to_string()).bind(group).bind(user).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
        match (target.is_some(), inserted) {
            (false, Some((id, created))) => {
                expected
                    .members
                    .push((id, group.into(), user.into(), created));
                expected.members.sort_by(|a, b| a.0.cmp(&b.0));
            }
            (true, None) => {}
            _ => return Err(AppError::Conflict("Group membership target changed".into())),
        }
    } else {
        let rows = sqlx::query("DELETE FROM group_members WHERE group_id=$1 AND user_id=$2")
            .bind(group)
            .bind(user)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
        if rows.rows_affected() != u64::from(target.is_some()) {
            return Err(AppError::Conflict("Group membership target changed".into()));
        }
        expected.members.retain(|m| m.2 != user);
        if let Some(actor) = &mut actor_expected {
            actor.groups.retain(|id| id != group);
            actor.grants.retain(|grant| grant.0 != group);
        }
    }
    if let Some(actor) = actor_expected {
        // Only the precise intended self-removal is allowed to lose authority.
        // Real credential/current account remains mandatory; no fake admin.
        access::require_current_human_preference_write(&mut tx, auth).await?;
        if audience_policy_snapshot(&mut tx, space, &auth.claims.sub, &actor_roles).await? != actor
        {
            return Err(AppError::Conflict(
                "Space access changed during self mutation".into(),
            ));
        }
        access::require_current_human_preference_write(&mut tx, auth).await?;
    } else {
        let current = access::require_space_membership_management(&mut tx, auth, space).await?;
        if add
            && !group_grant_permission_ceiling(&mut tx, space, &role)
                .await?
                .is_subset(&current)
        {
            return Err(AppError::Forbidden(
                "Cannot grant permissions outside your space authority".into(),
            ));
        }
    }
    let final_source = sole_space_group_source(&mut tx, space, group).await?;
    validate_sole_space_group(&final_source, space)?;
    if final_source != expected {
        return Err(AppError::Conflict("Group membership source changed".into()));
    }
    tx.commit().await.map_err(AppError::internal)?;
    Ok(())
}
async fn add_space_group_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, group_id)): Path<(String, String)>,
    Json(req): Json<AddGroupMemberRequest>,
) -> Result<impl IntoResponse, AppError> {
    mutate_sole_space_group_member(&state, &auth, &space_id, &group_id, &req.user_id, true).await?;
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "space.group_member_add",
        Some("space"),
        Some(&space_id),
    )
    .await;
    Ok(StatusCode::CREATED)
}
async fn remove_space_group_member(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, group_id, user_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    mutate_sole_space_group_member(&state, &auth, &space_id, &group_id, &user_id, false).await?;
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "space.group_member_remove",
        Some("space"),
        Some(&space_id),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

// ============================================================
// Auth Helpers
// ============================================================

fn validate_space_visibility(visibility: &str) -> Result<&str, AppError> {
    match visibility {
        "private" | "internal" | "public" => Ok(visibility),
        _ => Err(AppError::BadRequest("Invalid space visibility".into())),
    }
}

fn space_invite_notification_copy(
    space_name: &str,
    actor_name: &str,
    role: &str,
) -> (String, String) {
    let article = if role
        .chars()
        .next()
        .is_some_and(|character| matches!(character, 'a' | 'e' | 'i' | 'o' | 'u'))
    {
        "an"
    } else {
        "a"
    };
    (
        format!("You were added to {space_name}"),
        format!("{actor_name} added you as {article} {role}."),
    )
}

fn role_allows(role: &str, allowed_roles: &[&str]) -> bool {
    allowed_roles.iter().any(|required| match *required {
        "owner" => matches!(role, "owner" | "space_admin" | "admin"),
        "editor" => matches!(
            role,
            "owner" | "space_admin" | "admin" | "editor" | "contributor"
        ),
        "commenter" => matches!(
            role,
            "owner" | "space_admin" | "admin" | "editor" | "contributor" | "commenter"
        ),
        "viewer" => matches!(
            role,
            "owner" | "space_admin" | "admin" | "editor" | "contributor" | "commenter" | "viewer"
        ),
        other => role == other,
    })
}

fn role_rank(role: &str) -> u8 {
    match role {
        "owner" => 6,
        "space_admin" | "admin" => 5,
        "editor" => 4,
        "contributor" => 3,
        "commenter" => 2,
        "viewer" => 1,
        _ => 0,
    }
}

async fn custom_role_permissions(
    conn: &mut sqlx::AnyConnection,
    space_id: &str,
    role: &str,
) -> Result<Option<Vec<SpacePermission>>, AppError> {
    let permissions: Option<String> =
        sqlx::query_scalar("SELECT permissions FROM space_roles WHERE space_id = $1 AND id = $2")
            .bind(space_id)
            .bind(role)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    permissions
        .map(|value| serde_json::from_str(&value).map_err(AppError::internal))
        .transpose()
}

fn required_permission(allowed_roles: &[&str]) -> Option<SpacePermission> {
    if allowed_roles.contains(&"owner") {
        Some(SpacePermission::ManageSpace)
    } else if allowed_roles.contains(&"editor") {
        Some(SpacePermission::EditContent)
    } else if allowed_roles.contains(&"commenter") {
        Some(SpacePermission::Comment)
    } else if allowed_roles.contains(&"viewer") {
        Some(SpacePermission::View)
    } else {
        None
    }
}

async fn role_allows_in_space(
    conn: &mut sqlx::AnyConnection,
    space_id: &str,
    role: &str,
    allowed_roles: &[&str],
) -> Result<bool, AppError> {
    if role_allows(role, allowed_roles) {
        return Ok(true);
    }
    let Some(required) = required_permission(allowed_roles) else {
        return Ok(false);
    };
    Ok(custom_role_permissions(conn, space_id, role)
        .await?
        .is_some_and(|permissions| permissions.contains(&required)))
}


async fn role_rank_on_connection(
    conn: &mut sqlx::AnyConnection,
    space_id: &str,
    role: &str,
) -> Result<u8, AppError> {
    let built_in_rank = role_rank(role);
    if built_in_rank > 0 {
        return Ok(built_in_rank);
    }
    Ok(custom_role_permissions(conn, space_id, role)
        .await?
        .map_or(0, |permissions| permissions.len().min(4) as u8))
}

fn strongest_role(first: Option<String>, second: Option<String>) -> Option<String> {
    match (first, second) {
        (Some(first), Some(second)) if role_rank(&second) > role_rank(&first) => Some(second),
        (Some(first), _) => Some(first),
        (None, second) => second,
    }
}

/// Verify the user has one of the required roles in a space (or is admin)
pub async fn require_space_role(
    state: &AppState,
    space_id: &str,
    user_id: &str,
    allowed_roles: &[&str],
) -> Result<String, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_space_role_on_connection(&mut conn, space_id, user_id, allowed_roles).await
}

/// Evaluate the existing space policy on the caller's transaction connection.
/// Mutation callers use this after acquiring ordered space guards, before scoped writes.
/// It deliberately shares the same policy as the pooled entry point.
pub async fn require_space_role_on_connection(
    conn: &mut sqlx::AnyConnection,
    space_id: &str,
    user_id: &str,
    allowed_roles: &[&str],
) -> Result<String, AppError> {
    // Check if user is system admin
    let user_role: Option<(String,)> = sqlx::query_as("SELECT role FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?;

    if let Some((role,)) = &user_role {
        if role == "admin" {
            return Ok("admin".to_string());
        }
    }

    let blocked: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM space_user_blocks WHERE space_id = $1 AND user_id = $2")
            .bind(space_id)
            .bind(user_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    if blocked.is_some() {
        return Err(AppError::Forbidden(
            "Your access to this space has been blocked".into(),
        ));
    }

    let direct_role: Option<String> =
        sqlx::query_scalar("SELECT role FROM space_members WHERE space_id = $1 AND user_id = $2")
            .bind(space_id)
            .bind(user_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    let group_roles: Vec<String> = sqlx::query_scalar(
        "SELECT sgp.role FROM space_group_permissions sgp \
         JOIN group_members gm ON gm.group_id = sgp.group_id \
         WHERE sgp.space_id = $1 AND gm.user_id = $2",
    )
    .bind(space_id)
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let global_group_roles: Vec<String> = sqlx::query_scalar(
        "SELECT r.id FROM group_global_roles ggr
         JOIN global_roles r ON r.id = ggr.role_id
         JOIN group_members gm ON gm.group_id = ggr.group_id
         WHERE gm.user_id = $1",
    )
    .bind(user_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    let mut assigned_roles = group_roles;
    assigned_roles.extend(global_group_roles);
    if let Some(role) = direct_role {
        assigned_roles.push(role);
    }
    if !assigned_roles.is_empty() {
        let mut strongest: Option<(String, u8)> = None;
        let mut allowed = false;
        for role in assigned_roles {
            let rank = role_rank_on_connection(conn, space_id, &role).await?;
            if strongest
                .as_ref()
                .is_none_or(|(_, current_rank)| rank > *current_rank)
            {
                strongest = Some((role.clone(), rank));
            }
            if role_allows_in_space(conn, space_id, &role, allowed_roles).await? {
                allowed = true;
            }
        }
        return if allowed {
            Ok(strongest
                .map(|(role, _)| role)
                .unwrap_or_else(|| "viewer".into()))
        } else {
            Err(AppError::Forbidden("Insufficient permissions".into()))
        };
    }

    if allowed_roles.contains(&"viewer") {
        let visibility: Option<String> =
            sqlx::query_scalar("SELECT visibility FROM spaces WHERE id = $1")
                .bind(space_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(AppError::internal)?;
        return match visibility.as_deref() {
            Some("internal" | "public") => Ok("viewer".to_string()),
            Some(_) => Err(AppError::Forbidden("Not a member of this space".into())),
            None => Err(AppError::NotFound("Space not found".into())),
        };
    }

    Err(AppError::Forbidden("Not a member of this space".into()))
}

#[cfg(test)]
mod tests {
    use super::{
        require_space_role, require_space_role_on_connection, role_allows, space_invite_notification_copy, strongest_role,
        validate_space_visibility,
    };
    use crate::auth::routes::AppError;
    use crate::AppState;
    use sqlx::any::AnyPoolOptions;

    async fn access_test_state() -> AppState {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        for statement in [
            "CREATE TABLE users (account_kind TEXT NOT NULL DEFAULT 'member',id TEXT PRIMARY KEY, role TEXT NOT NULL)",
            "CREATE TABLE spaces (id TEXT PRIMARY KEY, visibility TEXT NOT NULL, trash_retention_days INTEGER NOT NULL DEFAULT 30)",
            "CREATE TABLE space_members (space_id TEXT NOT NULL, user_id TEXT NOT NULL, role TEXT NOT NULL)",
            "CREATE TABLE groups (id TEXT PRIMARY KEY)",
            "CREATE TABLE group_members (group_id TEXT NOT NULL, user_id TEXT NOT NULL)",
            "CREATE TABLE space_group_permissions (space_id TEXT NOT NULL, group_id TEXT NOT NULL, role TEXT NOT NULL)",
            "CREATE TABLE space_user_blocks (space_id TEXT NOT NULL, user_id TEXT NOT NULL)",
            "CREATE TABLE space_roles (id TEXT NOT NULL, space_id TEXT NOT NULL, permissions TEXT NOT NULL)",
            "CREATE TABLE global_roles (id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', permissions TEXT NOT NULL DEFAULT '[]', built_in INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE group_global_roles (group_id TEXT NOT NULL, role_id TEXT NOT NULL, PRIMARY KEY(group_id, role_id))",
        ] {
            sqlx::query(statement)
                .execute(&db)
                .await
                .expect("create access fixture table");
        }
        sqlx::query("INSERT INTO global_roles (id, name) VALUES ('owner', 'Owner')")
            .execute(&db)
            .await
            .expect("seed access fixture role");
        let index_path = std::env::temp_dir().join(format!(
            "starlane-notes-space-access-test-{}",
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

    #[test]
    fn space_invites_use_a_clear_recipient_focused_copy_contract() {
        assert_eq!(
            space_invite_notification_copy("Design", "Eagle", "editor"),
            (
                "You were added to Design".to_string(),
                "Eagle added you as an editor.".to_string(),
            ),
        );
    }

    #[test]
    fn built_in_space_roles_enforce_the_permission_ladder() {
        assert!(role_allows("space_admin", &["owner"]));
        assert!(role_allows("editor", &["editor"]));
        assert!(role_allows("commenter", &["commenter"]));
        assert!(role_allows("commenter", &["viewer"]));
        assert!(!role_allows("viewer", &["commenter"]));
        assert!(!role_allows("editor", &["owner"]));
    }

    #[test]
    fn space_visibility_accepts_only_supported_access_modes() {
        assert!(validate_space_visibility("private").is_ok());
        assert!(validate_space_visibility("internal").is_ok());
        assert!(validate_space_visibility("public").is_ok());
        assert!(validate_space_visibility("secret").is_err());
    }

    #[test]
    fn direct_and_group_assignments_resolve_to_the_strongest_role() {
        assert_eq!(
            strongest_role(Some("viewer".into()), Some("editor".into())),
            Some("editor".into())
        );
        assert_eq!(
            strongest_role(Some("space_admin".into()), Some("commenter".into())),
            Some("space_admin".into())
        );
    }

    #[tokio::test]
    async fn explicit_space_block_overrides_group_and_visibility_access() {
        let state = access_test_state().await;
        sqlx::query("INSERT INTO users (id, role) VALUES ('user-1', 'member')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO spaces (id, visibility) VALUES ('space-1', 'public')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups (id) VALUES ('group-1')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members (group_id, user_id) VALUES ('group-1', 'user-1')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO space_group_permissions (space_id, group_id, role) \
             VALUES ('space-1', 'group-1', 'editor')",
        )
        .execute(&state.db)
        .await
        .unwrap();

        assert_eq!(
            require_space_role(&state, "space-1", "user-1", &["viewer"])
                .await
                .unwrap(),
            "editor"
        );
        sqlx::query(
            "INSERT INTO space_user_blocks (space_id, user_id) VALUES ('space-1', 'user-1')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert!(matches!(
            require_space_role(&state, "space-1", "user-1", &["viewer"]).await,
            Err(AppError::Forbidden(_))
        ));
    }

    #[tokio::test]
    async fn custom_space_roles_enforce_their_explicit_permissions() {
        let state = access_test_state().await;
        sqlx::query("INSERT INTO users (id, role) VALUES ('user-1', 'member')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO spaces (id, visibility) VALUES ('space-1', 'private')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO space_roles (id, space_id, permissions) \
             VALUES ('custom:reviewer', 'space-1', '[\"view\",\"comment\"]')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO space_members (space_id, user_id, role) \
             VALUES ('space-1', 'user-1', 'custom:reviewer')",
        )
        .execute(&state.db)
        .await
        .unwrap();

        assert!(
            require_space_role(&state, "space-1", "user-1", &["commenter"])
                .await
                .is_ok()
        );
        assert!(matches!(
            require_space_role(&state, "space-1", "user-1", &["editor"]).await,
            Err(AppError::Forbidden(_))
        ));
    }

    #[tokio::test]
    async fn transactional_space_policy_sees_uncommitted_revocation_without_pool_reentry() {
        let state = access_test_state().await;
        for statement in [
            "INSERT INTO users (id, role) VALUES ('member', 'member')",
            "INSERT INTO spaces (id, visibility) VALUES ('space', 'public')",
            "INSERT INTO space_members (space_id, user_id, role) VALUES ('space', 'member', 'editor')",
        ] {
            sqlx::query(statement).execute(&state.db).await.unwrap();
        }
        let pooled = require_space_role(&state, "space", "member", &["editor"]).await.unwrap();
        let mut tx = state.db.begin().await.unwrap();
        assert_eq!(require_space_role_on_connection(&mut tx, "space", "member", &["editor"]).await.unwrap(), pooled);
        sqlx::query("INSERT INTO space_user_blocks (space_id, user_id) VALUES ('space', 'member')")
            .execute(&mut *tx).await.unwrap();
        assert!(matches!(require_space_role_on_connection(&mut tx, "space", "member", &["viewer"]).await, Err(AppError::Forbidden(_))));
        tx.rollback().await.unwrap();
        assert_eq!(require_space_role(&state, "space", "member", &["editor"]).await.unwrap(), pooled);
    }

    #[tokio::test]
    async fn transactional_space_policy_keeps_custom_and_group_permissions_additive() {
        let state = access_test_state().await;
        for statement in [
            "INSERT INTO users (id, role) VALUES ('member', 'member')",
            "INSERT INTO spaces (id, visibility) VALUES ('space', 'private')",
            "INSERT INTO space_roles (id, space_id, permissions) VALUES ('custom:edit', 'space', '[\"view\",\"edit_content\"]')",
            "INSERT INTO space_members (space_id, user_id, role) VALUES ('space', 'member', 'custom:edit')",
            "INSERT INTO groups (id) VALUES ('group')",
            "INSERT INTO group_members (group_id, user_id) VALUES ('group', 'member')",
            "INSERT INTO space_group_permissions (space_id, group_id, role) VALUES ('space', 'group', 'commenter')",
        ] {
            sqlx::query(statement).execute(&state.db).await.unwrap();
        }
        let mut tx = state.db.begin().await.unwrap();
        for required in ["viewer", "commenter", "editor"] {
            assert!(require_space_role_on_connection(&mut tx, "space", "member", &[required]).await.is_ok());
        }
        assert!(matches!(require_space_role_on_connection(&mut tx, "space", "member", &["owner"]).await, Err(AppError::Forbidden(_))));
        tx.rollback().await.unwrap();
        assert!(require_space_role(&state, "space", "member", &["editor"]).await.is_ok());
    }
}
#[cfg(test)]
pub(crate) mod catalog_tests {
    use super::*;
    use axum::{
        extract::{Path, State},
        response::IntoResponse,
    };
    use serde_json::{json, Value};
    pub(crate) async fn catalog_state(wal: bool) -> AppState {
        let mut state = super::super::properties::tests::property_test_state().await;
        let options = sqlx::any::AnyPoolOptions::new().max_connections(if wal { 2 } else { 1 });
        state.db = if wal {
            let path = std::env::temp_dir().join(format!("catalog-wal-{}.db", Uuid::new_v4()));
            options
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
                .connect(&format!("sqlite://{}?mode=rwc", path.display()))
                .await
                .unwrap()
        } else {
            options.connect("sqlite::memory:").await.unwrap()
        };
        sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
        for actor in ["owner-a", "viewer-a", "editor-a"] {
            sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')")
                .bind(actor)
                .bind(format!("{actor}@catalog.test"))
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'test','2999-01-01T00:00:00Z')").bind(format!("metadata-{actor}")).bind(actor).execute(&state.db).await.unwrap();
        }
        for space in ["space-a", "space-b"] {
            sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,$1,$1,'private','owner-a')").bind(space).execute(&state.db).await.unwrap();
        }
        for (actor, role) in [
            ("owner-a", "owner"),
            ("viewer-a", "viewer"),
            ("editor-a", "editor"),
        ] {
            sqlx::query(
                "INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'space-a',$2,$3)",
            )
            .bind(format!("member-{actor}"))
            .bind(actor)
            .bind(role)
            .execute(&state.db)
            .await
            .unwrap();
        }
        for (id, space, parent) in [
            ("folder-a", "space-a", None),
            ("child-a", "space-a", Some("folder-a")),
            ("foreign-folder", "space-b", None),
        ] {
            sqlx::query("INSERT INTO folders(id,space_id,parent_id,name) VALUES($1,$2,$3,$1)")
                .bind(id)
                .bind(space)
                .bind(parent)
                .execute(&state.db)
                .await
                .unwrap();
        }
        for (id, space) in [("template-a", "space-a"), ("foreign-template", "space-b")] {
            sqlx::query("INSERT INTO templates(id,space_id,name,content,created_by) VALUES($1,$2,$1,'TEMPLATE SECRET','owner-a')").bind(id).bind(space).execute(&state.db).await.unwrap();
        }
        let snapshot = json!({"name":"Historical","content":"SNAPSHOT SECRET"}).to_string();
        for number in 1..=13 {
            sqlx::query("INSERT INTO template_revisions(id,template_id,space_id,revision_number,snapshot,created_by) VALUES($1,'template-a','space-a',$2,$3,'owner-a')").bind(format!("revision-{number}")).bind(number).bind(&snapshot).execute(&state.db).await.unwrap();
        }
        state
    }
    pub(crate) fn auth(actor: &str) -> AuthUser {
        super::super::properties::tests::test_auth(actor)
    }
    pub(crate) fn copy_auth(auth: &AuthUser) -> AuthUser {
        AuthUser {
            claims: auth.claims.clone(),
            api_key_scopes: auth.api_key_scopes.clone(),
        }
    }
    pub(crate) async fn key(
        state: &AppState,
        id: &str,
        actor: Option<&str>,
        scopes: &str,
    ) -> AuthUser {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,$2,'Catalog key',$1,'catalog',$3)").bind(id).bind(actor).bind(scopes).execute(&state.db).await.unwrap();
        let mut a = auth(actor.unwrap_or("service"));
        a.claims.sub = actor
            .map(str::to_owned)
            .unwrap_or_else(|| format!("service:{id}"));
        a.claims.sid = format!(
            "{}:{id}",
            if actor.is_some() {
                "api-key"
            } else {
                "service"
            }
        );
        a.api_key_scopes = Some(vec!["admin".into()]);
        a
    }
    pub(crate) async fn body(response: impl IntoResponse) -> Value {
        let bytes = axum::body::to_bytes(response.into_response().into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
    pub(crate) fn pause(state: &AppState, sql: &'static str) {
        let db = state.db.clone();
        CATALOG_AFTER_LOAD.with(|slot| {
            *slot.borrow_mut() = Some(Box::pin(async move {
                sqlx::query(sql).execute(&db).await.unwrap();
            }))
        });
    }
    pub(crate) fn pause_final(state: &AppState, sql: &'static str) {
        let db = state.db.clone();
        CATALOG_AFTER_SPACE_VIEW.with(|slot| {
            *slot.borrow_mut() = Some(Box::pin(async move {
                sqlx::query(sql).execute(&db).await.unwrap();
            }))
        });
    }
    #[tokio::test]
    async fn catalog_space_discovery_uses_actual_permission_union_not_group_presence_or_labels() {
        let state = catalog_state(false).await;
        sqlx::query("DELETE FROM space_members WHERE user_id='viewer-a'")
            .execute(&state.db)
            .await
            .unwrap();
        for sql in [
            "INSERT INTO groups(id,name) VALUES('catalog-group','Catalog')",
            "INSERT INTO group_members(id,group_id,user_id) VALUES('catalog-member','catalog-group','viewer-a')",
            "INSERT INTO global_roles(id,name,permissions) VALUES('admin','Fake ADMIN','[]') ON CONFLICT(id) DO UPDATE SET permissions='[]'",
            "INSERT INTO group_global_roles(group_id,role_id) VALUES('catalog-group','admin')",
        ] {sqlx::query(sql).execute(&state.db).await.unwrap();}
        assert_eq!(
            body(
                list_spaces(State(state.clone()), auth("viewer-a"))
                    .await
                    .unwrap()
            )
            .await,
            json!([])
        );
        assert!(matches!(
            get_space(
                State(state.clone()),
                auth("viewer-a"),
                Path("space-a".into())
            )
            .await,
            Err(AppError::NotFound(_))
        ));
        sqlx::query("UPDATE global_roles SET permissions='[\"view\"]' WHERE id='admin'")
            .execute(&state.db)
            .await
            .unwrap();
        let spaces = body(
            list_spaces(State(state.clone()), auth("viewer-a"))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(spaces.as_array().unwrap().len(), 2);
        assert!(get_space(
            State(state.clone()),
            auth("viewer-a"),
            Path("space-b".into())
        )
        .await
        .is_ok());
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('local-no-view','space-a','No View','[]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('direct-custom','space-a','viewer-a','local-no-view')").execute(&state.db).await.unwrap();
        assert!(
            get_space(
                State(state.clone()),
                auth("viewer-a"),
                Path("space-a".into())
            )
            .await
            .is_ok(),
            "Actual global View survives additive local assignment"
        );
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')").execute(&state.db).await.unwrap();
        let spaces = body(
            list_spaces(State(state.clone()), auth("viewer-a"))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(spaces.as_array().unwrap().len(), 1);
        assert_eq!(spaces[0]["id"], "space-b");
        for id in ["space-a", "missing"] {
            assert!(matches!(
                get_space(State(state.clone()), auth("viewer-a"), Path(id.into())).await,
                Err(AppError::NotFound(_))
            ));
        }
    }
    #[tokio::test]
    async fn catalog_spaces_keep_dangling_creator_metadata_and_real_key_ceilings() {
        let state = catalog_state(false).await;
        sqlx::query("PRAGMA foreign_keys=OFF")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE spaces SET created_by='missing-creator' WHERE id='space-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&state.db)
            .await
            .unwrap();
        let list = body(
            list_spaces(State(state.clone()), auth("viewer-a"))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["owner_display_name"], "Unknown user");
        assert!(get_space(
            State(state.clone()),
            auth("viewer-a"),
            Path("space-a".into())
        )
        .await
        .is_ok());
        let read = key(&state, "catalog-read", Some("viewer-a"), "[\"read\"]").await;
        assert!(get_space(
            State(state.clone()),
            copy_auth(&read),
            Path("space-a".into())
        )
        .await
        .is_ok());
        let write = key(&state, "catalog-write", Some("viewer-a"), "[\"write\"]").await;
        assert!(
            get_space(State(state.clone()), write, Path("space-a".into()))
                .await
                .is_err()
        );
        let service = key(&state, "catalog-service", None, "[\"admin\"]").await;
        assert!(matches!(
            list_spaces(State(state.clone()), service).await,
            Err(AppError::Unauthorized(_))
        ));
        sqlx::query(
            "UPDATE api_keys SET expires_at='2000-01-01T00:00:00Z' WHERE id='catalog-read'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert!(
            get_space(State(state.clone()), read, Path("space-a".into()))
                .await
                .is_err()
        );
        sqlx::query("UPDATE users SET is_active=0 WHERE id='viewer-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(matches!(
            list_spaces(State(state), auth("viewer-a")).await,
            Err(AppError::Unauthorized(_))
        ));
    }
    #[tokio::test]
    async fn catalog_spaces_final_read_rechecks_external_wal_revocation_and_blocks() {
        for sql in ["DELETE FROM sessions WHERE id='metadata-viewer-a'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')"] {
            let state=catalog_state(true).await;pause(&state,sql);
            assert!(get_space(State(state.clone()),auth("viewer-a"),Path("space-a".into())).await.is_err());
        }
        let state = catalog_state(true).await;
        sqlx::query("DELETE FROM space_members WHERE user_id='viewer-a'")
            .execute(&state.db)
            .await
            .unwrap();
        pause(&state, "DELETE FROM sessions WHERE id='metadata-viewer-a'");
        assert!(
            matches!(
                list_spaces(State(state), auth("viewer-a")).await,
                Err(AppError::Unauthorized(_))
            ),
            "Empty/hidden catalog still checks current credential"
        );
    }
    async fn seed_group_catalog(state: &AppState, managed: bool) {
        sqlx::query("INSERT INTO groups(id,name,description,managed_space_id) VALUES('catalog-group','Group','Description',$1)")
            .bind(if managed {Some("space-a")} else {None}).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('catalog-grant','space-a','catalog-group','viewer')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('catalog-group-member','catalog-group','editor-a')").execute(&state.db).await.unwrap();
    }
    async fn groups_snapshot(state: &AppState) -> Vec<(String, Option<String>)> {
        use sqlx::Row;
        let mut rows = super::super::documents::tests::move_database_snapshot(state).await;
        // Initial HTTP authentication updates only these existing bookkeeping fields.
        for table in ["api_keys", "sessions"] {
            let columns = sqlx::query(&format!("PRAGMA table_info({table})"))
                .fetch_all(&state.db)
                .await
                .unwrap();
            let expression = columns
                .iter()
                .map(|c| c.get::<String, _>("name"))
                .filter(|name| {
                    !(table == "api_keys" && name == "last_used_at")
                        && !(table == "sessions"
                            && matches!(name.as_str(), "last_seen_at" | "expires_at"))
                })
                .map(|name| format!("quote(\"{name}\")"))
                .collect::<Vec<_>>()
                .join(" || '|' || ");
            let canonical:Option<String>=sqlx::query_scalar(&format!("SELECT group_concat({expression},char(10)) FROM (SELECT * FROM {table} ORDER BY rowid)")).fetch_one(&state.db).await.unwrap();
            for (name, value) in &mut rows {
                if name == table {
                    *value = canonical.clone();
                }
            }
        }
        rows
    }
    #[tokio::test]
    async fn group_catalog_http_read_keys_never_repair_missing_managed_group() {
        use tower::ServiceExt;
        let state = catalog_state(false).await;
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('read-catalog','owner-a','Read',$1,'read','[\"read\"]')")
            .bind(crate::auth::api_keys::hash_api_key("read-catalog")).execute(&state.db).await.unwrap();
        let before = groups_snapshot(&state).await;
        let response = super::router()
            .with_state(state.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri("/spaces/space-a/groups")
                    .header("X-API-Key", "read-catalog")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(body(response).await, json!([]));
        assert_eq!(
            groups_snapshot(&state).await,
            before,
            "all canonical tables unchanged, excluding intentional key usage timestamp"
        );
        let mut current = auth("owner-a");
        current.claims.sid = "api-key:read-catalog".into();
        current.api_key_scopes = Some(vec!["read".into()]);
        let mut conn = state.db.acquire().await.unwrap();
        let ordinary = super::super::access::resolve_space_access(
            &mut conn,
            super::super::access::principal_from_auth(&current).unwrap(),
            "space-a",
            super::super::access::Purpose::Routine,
        )
        .await
        .unwrap();
        assert!(
            !ordinary.allows(super::super::access::Permission::ManageSpace),
            "ordinary resolver still forbids Read key management mutations"
        );
        super::super::access::require_space_management_catalog_read(&mut conn, &current, "space-a")
            .await
            .unwrap();
        drop(conn);
        let denied = super::router()
            .with_state(state.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/spaces/space-a/groups")
                    .header("X-API-Key", "read-catalog")
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"group_id":"absent","role":"viewer"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), axum::http::StatusCode::FORBIDDEN);
        assert_eq!(groups_snapshot(&state).await, before);
        for (key, scopes, expected) in [
            (
                "write-catalog",
                r#"["write"]"#,
                axum::http::StatusCode::FORBIDDEN,
            ),
            ("admin-catalog", r#"["admin"]"#, axum::http::StatusCode::OK),
        ] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'owner-a',$1,$2,'test',$3)").bind(key).bind(crate::auth::api_keys::hash_api_key(key)).bind(scopes).execute(&state.db).await.unwrap();
            let before = groups_snapshot(&state).await;
            let response = super::router()
                .with_state(state.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .uri("/spaces/space-a/groups")
                        .header("X-API-Key", key)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            assert_eq!(groups_snapshot(&state).await, before);
        }
        let used: Option<String> =
            sqlx::query_scalar("SELECT last_used_at FROM api_keys WHERE id='read-catalog'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert!(used.is_some());
        let before = groups_snapshot(&state).await;
        let token = crate::auth::jwt::create_token_pair_for_session(
            "owner-a",
            "owner-a@catalog.test",
            "member",
            &state.jwt_secret,
            Some("metadata-owner-a"),
            30,
        )
        .unwrap();
        let response = super::router()
            .with_state(state.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri("/spaces/space-a/groups")
                    .header("Authorization", format!("Bearer {}", token.access_token))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        assert_eq!(
            groups_snapshot(&state).await,
            before,
            "HTTP bearer GET changes no canonical row, excluding initial session heartbeat"
        );
        let before = super::super::documents::tests::move_database_snapshot(&state).await;
        super::list_space_groups(
            State(state.clone()),
            auth("owner-a"),
            Path("space-a".into()),
        )
        .await
        .unwrap();
        assert_eq!(
            super::super::documents::tests::move_database_snapshot(&state).await,
            before,
            "empty handler performs zero writes including auth tables"
        );
    }
    #[tokio::test]
    async fn group_catalog_preserves_existing_managed_and_reusable_wire_fields() {
        for managed in [false, true] {
            let state = catalog_state(false).await;
            seed_group_catalog(&state, managed).await;
            let before = super::super::documents::tests::move_database_snapshot(&state).await;
            let rows = body(
                super::list_space_groups(
                    State(state.clone()),
                    auth("owner-a"),
                    Path("space-a".into()),
                )
                .await
                .unwrap(),
            )
            .await;
            assert_eq!(rows[0]["group_id"], "catalog-group");
            assert_eq!(rows[0]["name"], "Group");
            assert_eq!(rows[0]["description"], "Description");
            assert_eq!(rows[0]["role"], "viewer");
            assert_eq!(rows[0]["member_count"], 1);
            assert_eq!(rows[0]["managed"], managed);
            assert!(rows[0]["granted_at"].is_string());
            assert_eq!(
                super::super::documents::tests::move_database_snapshot(&state).await,
                before
            );
        }
    }
    #[tokio::test]
    async fn group_catalog_keeps_management_union_and_current_credential_scope() {
        let state = catalog_state(false).await;
        seed_group_catalog(&state, false).await;
        for actor in ["viewer-a", "editor-a"] {
            assert!(super::list_space_groups(
                State(state.clone()),
                auth(actor),
                Path("space-a".into())
            )
            .await
            .is_err());
        }
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('catalog-custom','space-a','Manage','[\"manage_space\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query(
            "UPDATE space_group_permissions SET role='catalog-custom' WHERE id='catalog-grant'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('catalog-viewer','catalog-group','viewer-a')").execute(&state.db).await.unwrap();
        assert!(
            super::list_space_groups(
                State(state.clone()),
                auth("viewer-a"),
                Path("space-a".into())
            )
            .await
            .is_ok(),
            "direct View plus scoped custom ManageSpace union"
        );
        let write = key(&state, "catalog-write", Some("owner-a"), "[\"write\"]").await;
        assert!(
            super::list_space_groups(State(state.clone()), write, Path("space-a".into()))
                .await
                .is_err()
        );
        assert!(super::list_space_groups(
            State(state.clone()),
            auth("viewer-a"),
            Path("space-b".into())
        )
        .await
        .is_err());
        assert!(super::list_space_groups(
            State(state.clone()),
            auth("owner-a"),
            Path("absent".into())
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn group_catalog_empty_and_populated_reads_observe_external_wal_revocation() {
        for seeded in [false, true] {
            for sql in ["DELETE FROM sessions WHERE id='metadata-owner-a'","UPDATE users SET is_active=0 WHERE id='owner-a'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','owner-a')","DELETE FROM space_members WHERE user_id='owner-a'"] {
            let state=catalog_state(true).await;if seeded {seed_group_catalog(&state,false).await;}pause(&state,sql);
            assert!(super::list_space_groups(State(state.clone()),auth("owner-a"),Path("space-a".into())).await.is_err());
        }
        }
    }
    #[tokio::test]
    async fn group_catalog_final_exact_bindings_omit_changed_external_wal_rows() {
        for sql in [
            "UPDATE space_group_permissions SET space_id='space-b' WHERE id='catalog-grant'",
            "DELETE FROM groups WHERE id='catalog-group'",
            "UPDATE groups SET name='New name' WHERE id='catalog-group'",
            "UPDATE space_group_permissions SET role='editor' WHERE id='catalog-grant'",
            "DELETE FROM group_members WHERE id='catalog-group-member'",
        ] {
            let state = catalog_state(true).await;
            seed_group_catalog(&state, false).await;
            pause_final(&state, sql);
            assert_eq!(
                body(
                    super::list_space_groups(
                        State(state.clone()),
                        auth("owner-a"),
                        Path("space-a".into())
                    )
                    .await
                    .unwrap()
                )
                .await,
                json!([])
            );
        }
    }
}
#[cfg(test)]
mod direct_audience_tests {
    use super::catalog_tests::{auth, catalog_state};
    use super::*;
    async fn apply(
        state: &AppState,
        actor: &str,
        change: DirectAudienceChange,
    ) -> Result<(), AppError> {
        apply_direct_audience_change(state, &auth(actor), "space-a", &change).await
    }
    async fn role(state: &AppState, actor: &str) -> Option<String> {
        sqlx::query_scalar("SELECT role FROM space_members WHERE space_id='space-a' AND user_id=$1")
            .bind(actor)
            .fetch_optional(&state.db)
            .await
            .unwrap()
    }
    async fn custom(state: &AppState, id: &str, permissions: &str) {
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES($1,'space-a',$1,$2,'owner-a')").bind(id).bind(permissions).execute(&state.db).await.unwrap();
    }
    async fn target(state: &AppState) {
        sqlx::query("INSERT INTO users(id,email,display_name,role,is_active) VALUES('target','target@test','Target','member',0)").execute(&state.db).await.unwrap();
    }
    #[tokio::test]
    async fn direct_audience_write_key_grants_inactive_target_and_preserves_http_contracts() {
        use tower::ServiceExt;
        let state = catalog_state(false).await;
        target(&state).await;
        for (id, scopes) in [("aud-write", "[\"write\"]"), ("aud-read", "[\"read\"]")] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'owner-a',$1,$2,$1,$3)").bind(id).bind(crate::auth::api_keys::hash_api_key(id)).bind(scopes).execute(&state.db).await.unwrap();
        }
        async fn request(
            state: &AppState,
            key: &str,
            method: &str,
            path: &str,
            body: &str,
        ) -> axum::http::StatusCode {
            super::router()
                .with_state(state.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .method(method)
                        .uri(path)
                        .header("X-API-Key", key)
                        .header("Content-Type", "application/json")
                        .body(axum::body::Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
        }
        let viewer = auth("viewer-a");
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &viewer.claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let denied = super::router()
            .with_state(state.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method("POST")
                    .uri("/spaces/space-a/members")
                    .header("Authorization", format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(
                        r#"{"user_id":"target","role":"viewer"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            denied.status(),
            StatusCode::FORBIDDEN,
            "current ordinary viewer keeps legacy permission-denial status"
        );
        sqlx::query("DELETE FROM space_members WHERE space_id='space-a' AND user_id='viewer-a'")
            .execute(&state.db)
            .await
            .unwrap();
        let mut concealed_bodies = Vec::new();
        for space in ["space-a", "missing-space"] {
            let hidden = super::router()
                .with_state(state.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri(format!("/spaces/{space}/members"))
                        .header("Authorization", format!("Bearer {token}"))
                        .header("Content-Type", "application/json")
                        .body(axum::body::Body::from(
                            r#"{"user_id":"target","role":"viewer"}"#,
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                hidden.status(),
                StatusCode::NOT_FOUND,
                "hidden existing and missing space are concealed equally"
            );
            concealed_bodies.push(
                axum::body::to_bytes(hidden.into_body(), usize::MAX)
                    .await
                    .unwrap(),
            );
        }
        assert_eq!(
            concealed_bodies[0], concealed_bodies[1],
            "error body must not disclose private space existence"
        );

        assert_eq!(
            request(
                &state,
                "aud-read",
                "POST",
                "/spaces/space-a/members",
                r#"{"user_id":"target"}"#
            )
            .await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            request(
                &state,
                "aud-write",
                "POST",
                "/spaces/space-a/members",
                r#"{"user_id":"target","role":"viewer"}"#
            )
            .await,
            StatusCode::CREATED
        );
        assert_eq!(role(&state, "target").await.as_deref(), Some("viewer"));
        assert_eq!(
            request(
                &state,
                "aud-write",
                "PUT",
                "/spaces/space-a/members/target",
                r#"{"user_id":"ignored-body-target"}"#
            )
            .await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(role(&state, "target").await.as_deref(), Some("editor"));
        assert_eq!(
            request(
                &state,
                "aud-write",
                "DELETE",
                "/spaces/space-a/members/target",
                ""
            )
            .await,
            StatusCode::NO_CONTENT
        );
        assert!(role(&state, "target").await.is_none());
        assert_eq!(
            request(
                &state,
                "aud-write",
                "PUT",
                "/spaces/space-a/blocks/target",
                ""
            )
            .await,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            request(
                &state,
                "aud-write",
                "DELETE",
                "/spaces/space-a/blocks/target",
                ""
            )
            .await,
            StatusCode::NO_CONTENT
        );
    }
    #[tokio::test]
    async fn direct_audience_custom_manager_cannot_escalate_but_exact_union_and_legacy_expansion_work(
    ) {
        let state = catalog_state(false).await;
        target(&state).await;
        custom(&state, "manager", r#"["manage_space"]"#).await;
        sqlx::query("UPDATE space_members SET role='manager' WHERE user_id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        for requested in ["viewer", "editor", "space_admin"] {
            assert!(matches!(
                apply(
                    &state,
                    "owner-a",
                    DirectAudienceChange::Add {
                        user_id: "target".into(),
                        role: requested.into()
                    }
                )
                .await,
                Err(AppError::Forbidden(_))
            ));
        }
        assert!(role(&state, "target").await.is_none());
        sqlx::query("INSERT INTO groups(id,name) VALUES('union','Union')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('union-owner','union','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('union-grant','space-a','union','viewer')").execute(&state.db).await.unwrap();
        apply(
            &state,
            "owner-a",
            DirectAudienceChange::Add {
                user_id: "target".into(),
                role: "viewer".into(),
            },
        )
        .await
        .unwrap();
        custom(&state, "legacy-edit", r#"["edit_content"]"#).await;
        sqlx::query("INSERT INTO global_roles(id,name,permissions) VALUES('granular-global','Granular','[\"edit_content\"]')").execute(&state.db).await.unwrap();
        sqlx::query(
            "INSERT INTO group_global_roles(group_id,role_id) VALUES('union','granular-global')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert!(
            matches!(
                apply(
                    &state,
                    "owner-a",
                    DirectAudienceChange::Update {
                        user_id: "target".into(),
                        role: "legacy-edit".into()
                    }
                )
                .await,
                Err(AppError::Forbidden(_))
            ),
            "global Edit is granular, never expanded"
        );
        sqlx::query("UPDATE space_group_permissions SET role='legacy-edit' WHERE id='union-grant'")
            .execute(&state.db)
            .await
            .unwrap();
        apply(
            &state,
            "owner-a",
            DirectAudienceChange::Update {
                user_id: "target".into(),
                role: "legacy-edit".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(role(&state, "target").await.as_deref(), Some("legacy-edit"));
    }
    #[tokio::test]
    async fn direct_audience_intentional_self_loss_and_creator_block_rule_preserved() {
        for change in [
            DirectAudienceChange::Remove {
                user_id: "owner-a".into(),
            },
            DirectAudienceChange::Update {
                user_id: "owner-a".into(),
                role: "viewer".into(),
            },
        ] {
            let state = catalog_state(false).await;
            apply(&state, "owner-a", change).await.unwrap();
            assert_ne!(role(&state, "owner-a").await.as_deref(), Some("owner"));
        }
        let state = catalog_state(false).await;
        assert!(matches!(
            apply(
                &state,
                "owner-a",
                DirectAudienceChange::Block {
                    user_id: "owner-a".into()
                }
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        sqlx::query("UPDATE space_members SET role='space_admin' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        apply(
            &state,
            "editor-a",
            DirectAudienceChange::Block {
                user_id: "editor-a".into(),
            },
        )
        .await
        .unwrap();
        assert!(sqlx::query_scalar::<_, i64>(
            "SELECT 1 FROM space_user_blocks WHERE user_id='editor-a'"
        )
        .fetch_optional(&state.db)
        .await
        .unwrap()
        .is_some());
    }
    #[tokio::test]
    async fn direct_audience_self_loss_denies_unrelated_policy_and_credential_trigger_drift() {
        for sql in [
            "DELETE FROM sessions WHERE user_id='owner-a'",
            "UPDATE users SET is_active=0 WHERE id='owner-a'",
            "UPDATE spaces SET visibility='public' WHERE id='space-a'",
        ] {
            let state = catalog_state(false).await;
            sqlx::query(&format!("CREATE TRIGGER audience_drift AFTER DELETE ON space_members WHEN OLD.user_id='owner-a' BEGIN {sql}; END")).execute(&state.db).await.unwrap();
            assert!(apply(
                &state,
                "owner-a",
                DirectAudienceChange::Remove {
                    user_id: "owner-a".into()
                }
            )
            .await
            .is_err());
            assert_eq!(role(&state, "owner-a").await.as_deref(), Some("owner"));
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM audit_log WHERE action='space.member_remove'"
                )
                .fetch_one(&state.db)
                .await
                .unwrap(),
                0
            );
        }
    }
    #[tokio::test]
    async fn direct_audience_late_target_write_revocation_rolls_back_canonical_and_trigger() {
        let state = catalog_state(false).await;
        sqlx::query("CREATE TRIGGER audience_late AFTER UPDATE OF role ON space_members WHEN NEW.user_id='viewer-a' AND NEW.role='editor' BEGIN INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','owner-a'); END").execute(&state.db).await.unwrap();
        assert!(apply(
            &state,
            "owner-a",
            DirectAudienceChange::Update {
                user_id: "viewer-a".into(),
                role: "editor".into()
            }
        )
        .await
        .is_err());
        assert_eq!(role(&state, "viewer-a").await.as_deref(), Some("viewer"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM space_user_blocks")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn direct_audience_remove_keeps_group_provenance_and_missing_cleanup_noops() {
        let state = catalog_state(false).await;
        sqlx::query("INSERT INTO groups(id,name) VALUES('retained','Retained')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('retained-member','retained','editor-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('retained-grant','space-a','retained','editor')").execute(&state.db).await.unwrap();
        apply(
            &state,
            "owner-a",
            DirectAudienceChange::Remove {
                user_id: "editor-a".into(),
            },
        )
        .await
        .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let a = super::super::access::resolve_space_access(
            &mut conn,
            super::super::access::principal_from_auth(&auth("editor-a")).unwrap(),
            "space-a",
            super::super::access::Purpose::Routine,
        )
        .await
        .unwrap();
        assert!(a.allows(super::super::access::Permission::EditContent));
        assert!(a.roles.iter().any(|r| matches!(
            r.source,
            super::super::access::RoleSource::SpaceGroup { .. }
        )));
        drop(conn);
        for c in [
            DirectAudienceChange::Remove {
                user_id: "absent".into(),
            },
            DirectAudienceChange::Unblock {
                user_id: "absent".into(),
            },
            DirectAudienceChange::Update {
                user_id: "absent".into(),
                role: "viewer".into(),
            },
        ] {
            apply(&state, "owner-a", c).await.unwrap();
        }
    }
    #[tokio::test]
    async fn direct_audience_rejects_foreign_role_viewer_inactive_actor_and_duplicate_atomically() {
        let state = catalog_state(false).await;
        target(&state).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('foreign-role','space-b','Foreign','[\"view\"]','owner-a')").execute(&state.db).await.unwrap();
        assert!(matches!(
            apply(
                &state,
                "owner-a",
                DirectAudienceChange::Add {
                    user_id: "target".into(),
                    role: "foreign-role".into()
                }
            )
            .await,
            Err(AppError::BadRequest(_))
        ));
        assert!(apply(
            &state,
            "viewer-a",
            DirectAudienceChange::Remove {
                user_id: "editor-a".into()
            }
        )
        .await
        .is_err());
        assert!(apply(
            &state,
            "owner-a",
            DirectAudienceChange::Add {
                user_id: "viewer-a".into(),
                role: "editor".into()
            }
        )
        .await
        .is_err());
        assert_eq!(role(&state, "viewer-a").await.as_deref(), Some("viewer"));
        sqlx::query("UPDATE users SET is_active=0 WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(matches!(
            apply(
                &state,
                "owner-a",
                DirectAudienceChange::Remove {
                    user_id: "editor-a".into()
                }
            )
            .await,
            Err(AppError::Unauthorized(_))
        ));
        assert!(role(&state, "target").await.is_none());
    }
    #[tokio::test]
    async fn direct_audience_rechecks_real_session_after_space_lock_wait() {
        let state = catalog_state(true).await;
        let mut blocker = state.db.begin().await.unwrap();
        sqlx::query("UPDATE spaces SET name=name WHERE id='space-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let task_state = state.clone();
        let task = tokio::spawn(async move {
            apply(
                &task_state,
                "owner-a",
                DirectAudienceChange::Remove {
                    user_id: "editor-a".into(),
                },
            )
            .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            !task.is_finished(),
            "request actually waits behind existing writer"
        );
        sqlx::query("DELETE FROM sessions WHERE user_id='owner-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        blocker.commit().await.unwrap();
        assert!(matches!(
            task.await.unwrap(),
            Err(AppError::Unauthorized(_))
        ));
        assert_eq!(role(&state, "editor-a").await.as_deref(), Some("editor"));
    }
    #[tokio::test]
    async fn direct_audience_final_exact_target_and_self_role_definition_drift_roll_back() {
        let state = catalog_state(false).await;
        sqlx::query("CREATE TRIGGER corrupt_target AFTER UPDATE OF role ON space_members WHEN NEW.user_id='viewer-a' AND NEW.role='editor' BEGIN UPDATE space_members SET role='viewer' WHERE id=NEW.id; END").execute(&state.db).await.unwrap();
        assert!(matches!(
            apply(
                &state,
                "owner-a",
                DirectAudienceChange::Update {
                    user_id: "viewer-a".into(),
                    role: "editor".into()
                }
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        assert_eq!(role(&state, "viewer-a").await.as_deref(), Some("viewer"));
        let state = catalog_state(false).await;
        custom(&state, "self-manager", r#"["manage_space","view"]"#).await;
        sqlx::query("UPDATE space_members SET role='self-manager' WHERE user_id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER self_role_drift AFTER UPDATE OF role ON space_members WHEN NEW.user_id='owner-a' AND NEW.role='viewer' BEGIN UPDATE space_roles SET permissions='[\"view\"]' WHERE id='self-manager'; END").execute(&state.db).await.unwrap();
        assert!(matches!(
            apply(
                &state,
                "owner-a",
                DirectAudienceChange::Update {
                    user_id: "owner-a".into(),
                    role: "viewer".into()
                }
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        assert_eq!(
            role(&state, "owner-a").await.as_deref(),
            Some("self-manager")
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT permissions FROM space_roles WHERE id='self-manager'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            r#"["manage_space","view"]"#
        );
    }
    #[tokio::test]
    async fn direct_audience_actual_bearer_expiry_and_late_key_revocation_deny() {
        let state = catalog_state(false).await;
        let mut expired = auth("owner-a");
        expired.claims.exp = 1;
        assert!(matches!(
            apply_direct_audience_change(
                &state,
                &expired,
                "space-a",
                &DirectAudienceChange::Remove {
                    user_id: "editor-a".into()
                }
            )
            .await,
            Err(AppError::Unauthorized(_))
        ));
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('late-audience','owner-a','Late','hash','late','[\"write\"]')").execute(&state.db).await.unwrap();
        let mut key = auth("owner-a");
        key.claims.sid = "api-key:late-audience".into();
        key.api_key_scopes = Some(vec!["write".into()]);
        sqlx::query("CREATE TRIGGER revoke_audience_key AFTER DELETE ON space_members WHEN OLD.user_id='editor-a' BEGIN DELETE FROM api_keys WHERE id='late-audience'; END").execute(&state.db).await.unwrap();
        assert!(apply_direct_audience_change(
            &state,
            &key,
            "space-a",
            &DirectAudienceChange::Remove {
                user_id: "editor-a".into()
            }
        )
        .await
        .is_err());
        assert_eq!(role(&state, "editor-a").await.as_deref(), Some("editor"));
        assert!(
            sqlx::query_scalar::<_, i64>("SELECT 1 FROM api_keys WHERE id='late-audience'")
                .fetch_optional(&state.db)
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn direct_audience_creator_changed_by_block_trigger_rolls_back() {
        let state = catalog_state(false).await;
        sqlx::query("CREATE TRIGGER changed_creator AFTER INSERT ON space_user_blocks WHEN NEW.user_id='editor-a' BEGIN UPDATE spaces SET created_by='editor-a' WHERE id='space-a'; END").execute(&state.db).await.unwrap();
        assert!(matches!(
            apply(
                &state,
                "owner-a",
                DirectAudienceChange::Block {
                    user_id: "editor-a".into()
                }
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        assert_eq!(
            sqlx::query_scalar::<_, String>("SELECT created_by FROM spaces WHERE id='space-a'")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            "owner-a"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM space_user_blocks")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn direct_audience_invitation_only_for_current_visible_active_recipient() {
        let state = catalog_state(false).await;
        target(&state).await;
        add_member(
            State(state.clone()),
            auth("owner-a"),
            Path("space-a".into()),
            Json(AddMemberRequest {
                user_id: "target".into(),
                role: Some("viewer".into()),
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM notifications WHERE user_id='target'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            0
        );
        apply(
            &state,
            "owner-a",
            DirectAudienceChange::Remove {
                user_id: "target".into(),
            },
        )
        .await
        .unwrap();
        sqlx::query("UPDATE users SET is_active=1 WHERE id='target'")
            .execute(&state.db)
            .await
            .unwrap();
        apply(
            &state,
            "owner-a",
            DirectAudienceChange::Block {
                user_id: "target".into(),
            },
        )
        .await
        .unwrap();
        add_member(
            State(state.clone()),
            auth("owner-a"),
            Path("space-a".into()),
            Json(AddMemberRequest {
                user_id: "target".into(),
                role: Some("viewer".into()),
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM notifications WHERE user_id='target'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            0
        );
        apply(
            &state,
            "owner-a",
            DirectAudienceChange::Remove {
                user_id: "target".into(),
            },
        )
        .await
        .unwrap();
        apply(
            &state,
            "owner-a",
            DirectAudienceChange::Unblock {
                user_id: "target".into(),
            },
        )
        .await
        .unwrap();
        add_member(
            State(state.clone()),
            auth("owner-a"),
            Path("space-a".into()),
            Json(AddMemberRequest {
                user_id: "target".into(),
                role: Some("viewer".into()),
            }),
        )
        .await
        .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM notifications WHERE user_id='target' AND type='space_invite'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            1
        );
    }
}
#[cfg(test)]
mod managed_group_provisioning_tests {
    use super::super::access;
    use super::catalog_tests::{auth, catalog_state};
    use super::*;
    async fn scoped(state: &AppState, space: &str, actor: &str) {
        let group: String = sqlx::query_scalar("SELECT id FROM groups WHERE managed_space_id=$1")
            .bind(space)
            .fetch_one(&state.db)
            .await
            .unwrap();
        let global: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM group_global_roles WHERE group_id=$1")
                .bind(&group)
                .fetch_one(&state.db)
                .await
                .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let principal = || access::Principal::User {
            user_id: actor,
            session_id: None,
            token_expires_at: None,
        };
        let own =
            access::resolve_space_access(&mut conn, principal(), space, access::Purpose::Routine)
                .await
                .unwrap();
        assert!(
            own.allows(access::Permission::View) && own.allows(access::Permission::ManageSpace)
        );
        let foreign = access::resolve_space_access(
            &mut conn,
            principal(),
            "space-b",
            access::Purpose::Routine,
        )
        .await
        .unwrap();
        assert!(
            !foreign.allows(access::Permission::View)
                && !foreign.allows(access::Permission::ManageSpace),
            "ordinary dedicated-group member acquired unrelated private-space permissions: {:?}",
            foreign.permissions
        );
        assert_eq!(
            global, 0,
            "new dedicated group must not automatically grant global permissions"
        );
        assert!(own.roles.iter().any(
            |r| matches!(&r.source,access::RoleSource::SpaceGroup{group_id} if *group_id==group)
                && r.id == "owner"
        ));
    }
    #[tokio::test]
    async fn managed_provisioning_new_group_is_scoped_without_global_owner() {
        let state = catalog_state(false).await;
        sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('new-scoped','New scoped','new-scoped','private','editor-a')").execute(&state.db).await.unwrap();
        let group = ensure_managed_space_group(&state, "new-scoped", "New scoped", "editor-a")
            .await
            .unwrap();
        let creator: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM group_members WHERE group_id=$1 AND user_id='editor-a'",
        )
        .bind(&group)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(creator, 1);
        scoped(&state, "new-scoped", "editor-a").await;
        let repeated =
            ensure_managed_space_group(&state, "new-scoped", "Changed input name", "viewer-a")
                .await
                .unwrap();
        assert_eq!(group, repeated);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM group_members WHERE group_id=$1")
                .bind(&group)
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
    }
    #[tokio::test]
    async fn managed_provisioning_ordinary_personal_space_retains_only_own_scope() {
        let state = catalog_state(false).await;
        crate::auth::routes::create_personal_space(
            &state,
            "viewer-a",
            &format!("Personal {}", Uuid::new_v4()),
        )
        .await
        .unwrap();
        let space: String = sqlx::query_scalar("SELECT id FROM spaces WHERE created_by='viewer-a'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT role FROM space_members WHERE space_id=$1 AND user_id='viewer-a'"
            )
            .bind(&space)
            .fetch_one(&state.db)
            .await
            .unwrap(),
            "owner"
        );
        scoped(&state, &space, "viewer-a").await;
    }
    async fn managed_provisioning_http_branch(replay: bool) {
        use tower::ServiceExt;
        let state = catalog_state(false).await;
        sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        let actor = auth("owner-a");
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &actor.claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let space = Uuid::new_v4().to_string();
        let name = format!("Scoped {}", Uuid::new_v4());
        let request = if replay {
            serde_json::json!({"schema_version":1,"provider":"self-hosted","operation_count":1,"replay_contract":{"schema_version":1,"operation_order":"queued_at_ascending","idempotency_key":"operation.id","transport":"server-api","server_backends":["sqlite","postgresql"],"required_payload_status":"available","conflict_policy":"server_validates_version_and_permissions","web_canonical_store":"server-api"},"operations":[{"id":"managed-import-op","entity_type":"space","entity_id":space,"action":"create","queued_at":"2026-07-08T12:04:00Z","payload_status":"available","payload":{"id":space,"name":name,"visibility":"private","created_at":"2026-07-08T12:03:00Z","updated_at":"2026-07-08T12:04:00Z"}}]})
        } else {
            serde_json::json!({"source_id":space,"name":name,"visibility":"private","created_at":"2026-07-08T12:03:00Z","updated_at":"2026-07-08T12:04:00Z"})
        };
        let http = axum::http::Request::builder()
            .method("POST")
            .uri(if replay { "/sync/replay" } else { "/spaces" })
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(request.to_string()))
            .unwrap();
        let response = if replay {
            crate::sync::api_router()
                .with_state(state.clone())
                .oneshot(http)
                .await
                .unwrap()
        } else {
            super::router()
                .with_state(state.clone())
                .oneshot(http)
                .await
                .unwrap()
        };
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(
            status.is_success(),
            "{status}: {}",
            String::from_utf8_lossy(&bytes)
        );
        if replay {
            let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(body["applied_operation_count"], 1, "{body}");
        }
        assert_eq!(
            sqlx::query_as::<_, (String, String, String)>(
                "SELECT id,created_at,updated_at FROM spaces WHERE id=$1"
            )
            .bind(&space)
            .fetch_one(&state.db)
            .await
            .unwrap(),
            (
                space.clone(),
                "2026-07-08T12:03:00+00:00".into(),
                "2026-07-08T12:04:00+00:00".into()
            )
        );
        sqlx::query("UPDATE users SET role='member' WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        scoped(&state, &space, "owner-a").await;
    }
    #[tokio::test]
    async fn managed_provisioning_rest_preserves_identity_without_global_grant() {
        managed_provisioning_http_branch(false).await;
    }
    #[tokio::test]
    async fn managed_provisioning_replay_preserves_identity_without_global_grant() {
        managed_provisioning_http_branch(true).await;
    }
    #[tokio::test]
    async fn managed_provisioning_existing_explicit_global_roles_are_unchanged() {
        let state = catalog_state(false).await;
        sqlx::query("INSERT INTO groups(id,name,managed_space_id) VALUES('existing-managed','Existing','space-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('existing-member','existing-managed','editor-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_global_roles(group_id,role_id,created_at) VALUES('existing-managed','owner','historical-owner'),('existing-managed','viewer','explicit-viewer')").execute(&state.db).await.unwrap();
        let before = crate::spaces::documents::tests::move_database_snapshot(&state).await;
        assert_eq!(
            ensure_managed_space_group(&state, "space-a", "Ignored change", "viewer-a")
                .await
                .unwrap(),
            "existing-managed"
        );
        assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&state).await,before,"existing managed group early return must not repair, remove or reassign explicit historical grants");
    }
}

#[cfg(test)]
mod global_role_fence_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use axum::extract::FromRequestParts;
    async fn setup(wal: bool) -> AppState {
        let state = catalog_tests::catalog_state(wal).await;
        sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES('global-test','Global test')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('global-member','global-test','editor-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('global-grant','space-a','global-test','viewer')").execute(&state.db).await.unwrap();
        state
    }
    async fn admin(state: &AppState, service: bool) -> AdminUser {
        let mut request = axum::http::Request::builder()
            .uri("/admin/groups/global-test/roles")
            .method("POST");
        if service {
            sqlx::query("INSERT INTO api_keys(id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('fence-service','Fence', $1,'test',1,'[\"admin\"]') ON CONFLICT(id) DO NOTHING").bind(crate::auth::api_keys::hash_api_key("fence-service")).execute(&state.db).await.unwrap();
            request = request.header("X-API-Key", "fence-service");
        } else {
            let token = jsonwebtoken::encode(
                &jsonwebtoken::Header::default(),
                &catalog_tests::auth("owner-a").claims,
                &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
            )
            .unwrap();
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let (mut parts, _) = request.body(()).unwrap().into_parts();
        AdminUser::from_request_parts(&mut parts, state)
            .await
            .unwrap()
    }
    #[tokio::test]
    async fn audience_fence_seed_preserves_rows_and_missing_fails_caller_rollback() {
        let state = setup(false).await;
        let before = move_database_snapshot(&state).await;
        let mut tx = state.db.begin().await.unwrap();
        super::super::mutation_guards::lock_audience_for_mutation(&mut tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(move_database_snapshot(&state).await, before);
        sqlx::query("DELETE FROM audience_mutation_fence")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        let mut tx = state.db.begin().await.unwrap();
        sqlx::query("UPDATE groups SET name='Tentative' WHERE id='global-test'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            super::super::mutation_guards::lock_audience_for_mutation(&mut tx)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn global_role_fence_preserves_assign_noop_remove_and_distinct_service() {
        for service in [false, true] {
            let state = setup(false).await;
            let proof = admin(&state, service).await;
            if service {
                let mut conn = state.db.acquire().await.unwrap();
                assert!(matches!(
                    crate::auth::middleware::require_current_admin(&mut conn, &proof)
                        .await
                        .unwrap(),
                    crate::auth::middleware::CurrentAdminAuthority::ServiceAdminKey(_)
                ));
            }
            for _ in 0..2 {
                let response = assign_group_global_role(
                    State(state.clone()),
                    admin(&state, service).await,
                    Path("global-test".into()),
                    Json(AssignGroupRoleRequest {
                        role_id: "contributor".into(),
                    }),
                )
                .await
                .unwrap()
                .into_response();
                assert_eq!(response.status(), StatusCode::CREATED);
            }
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM group_global_roles WHERE group_id='global-test'",
            )
            .fetch_one(&state.db)
            .await
            .unwrap();
            assert_eq!(count, 1);
            let response = remove_group_global_role(
                State(state.clone()),
                admin(&state, service).await,
                Path(("global-test".into(), "contributor".into())),
            )
            .await
            .unwrap()
            .into_response();
            assert_eq!(response.status(), StatusCode::NO_CONTENT);
            assert!(matches!(
                mutate_group_global_role(&state, &proof, "global-test", "contributor", false).await,
                Err(AppError::NotFound(_))
            ));
        }
    }
    #[tokio::test]
    async fn global_role_fence_missing_managed_owner_and_scim_contracts() {
        let state = setup(false).await;
        let proof = admin(&state, false).await;
        let before = move_database_snapshot(&state).await;
        for (group, role, assign) in [
            ("missing", "owner", true),
            ("global-test", "missing", true),
            ("missing", "owner", false),
            ("global-test", "viewer", false),
        ] {
            assert!(matches!(
                mutate_group_global_role(&state, &proof, group, role, assign).await,
                Err(AppError::NotFound(_))
            ));
            assert_eq!(move_database_snapshot(&state).await, before);
        }
        sqlx::query(
            "UPDATE groups SET managed_space_id='space-a',scim_managed=1 WHERE id='global-test'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        mutate_group_global_role(&state, &proof, "global-test", "owner", true)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(matches!(
            mutate_group_global_role(&state, &proof, "global-test", "owner", false).await,
            Err(AppError::Forbidden(_))
        ));
        assert_eq!(move_database_snapshot(&state).await, before);
        mutate_group_global_role(&state, &proof, "global-test", "viewer", true)
            .await
            .unwrap();
        mutate_group_global_role(&state, &proof, "global-test", "viewer", false)
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn global_role_fence_late_source_or_credential_drift_rolls_back_every_table() {
        for assign in [true, false] {
            for sql in [
            "UPDATE users SET role='member' WHERE id='owner-a';",
            "DELETE FROM sessions WHERE user_id='owner-a';",
            "UPDATE groups SET scim_managed=1 WHERE id='global-test';",
            "UPDATE global_roles SET permissions='[]' WHERE id='contributor';",
            "UPDATE group_members SET user_id='viewer-a' WHERE id='global-member';",
            "UPDATE space_group_permissions SET role='editor' WHERE id='global-grant';",
            "INSERT INTO group_global_roles(group_id,role_id) VALUES('global-test','viewer');",
            "INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('late-space','Late','late-space','private','owner-a');",
        ] {
            let state=setup(false).await;let proof=admin(&state,false).await;if !assign {mutate_group_global_role(&state,&proof,"global-test","contributor",true).await.unwrap();}
            let trigger=format!("CREATE TRIGGER late_audience AFTER {} ON group_global_roles WHEN {}.group_id='global-test' AND {}.role_id='contributor' BEGIN {} END",if assign {"INSERT"} else {"DELETE"},if assign {"NEW"} else {"OLD"},if assign {"NEW"} else {"OLD"},sql);sqlx::query(&trigger).execute(&state.db).await.unwrap();let before=move_database_snapshot(&state).await;
            assert!(mutate_group_global_role(&state,&proof,"global-test","contributor",assign).await.is_err(),"{assign}: {sql}");assert_eq!(move_database_snapshot(&state).await,before,"{assign}: {sql}");
        }
        }
    }
    #[tokio::test]
    async fn global_role_fence_late_service_scope_downgrade_rolls_back() {
        let state = setup(false).await;
        let proof = admin(&state, true).await;
        sqlx::query("CREATE TRIGGER revoke_service AFTER INSERT ON group_global_roles BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='fence-service'; END").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(
            mutate_group_global_role(&state, &proof, "global-test", "contributor", true)
                .await
                .is_err()
        );
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn global_role_fence_true_wal_wait_rechecks_actual_admin() {
        for service in [false, true] {
            let state = setup(true).await;
            let proof = admin(&state, service).await;
            let mut blocker = state.db.begin().await.unwrap();
            super::super::mutation_guards::lock_audience_for_mutation(&mut blocker)
                .await
                .unwrap();
            sqlx::query(if service {
                "UPDATE api_keys SET scopes='[\"read\"]' WHERE id='fence-service'"
            } else {
                "UPDATE users SET role='member' WHERE id='owner-a'"
            })
            .execute(&mut *blocker)
            .await
            .unwrap();
            let copy = state.clone();
            let task = tokio::spawn(async move {
                mutate_group_global_role(&copy, &proof, "global-test", "contributor", true).await
            });
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!task.is_finished());
            blocker.commit().await.unwrap();
            assert!(task.await.unwrap().is_err());
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM group_global_roles WHERE group_id='global-test'",
            )
            .fetch_one(&state.db)
            .await
            .unwrap();
            assert_eq!(count, 0);
        }
    }
    #[tokio::test]
    async fn global_role_fence_serializes_two_adopted_mutations_without_lost_assignments() {
        let state = setup(true).await;
        let first = admin(&state, false).await;
        let second = admin(&state, false).await;
        let mut blocker = state.db.begin().await.unwrap();
        super::super::mutation_guards::lock_audience_for_mutation(&mut blocker)
            .await
            .unwrap();
        let copy = state.clone();
        let task = tokio::spawn(async move {
            mutate_group_global_role(&copy, &first, "global-test", "contributor", true).await
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!task.is_finished());
        blocker.commit().await.unwrap();
        task.await.unwrap().unwrap();
        mutate_group_global_role(&state, &second, "global-test", "viewer", true)
            .await
            .unwrap();
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT role_id FROM group_global_roles WHERE group_id='global-test' ORDER BY role_id",
        )
        .fetch_all(&state.db)
        .await
        .unwrap();
        assert_eq!(rows, ["contributor", "viewer"]);
    }
    #[tokio::test]
    async fn global_role_fence_preserves_other_linked_role_policy_snapshot() {
        let state = setup(false).await;
        let proof = admin(&state, false).await;
        mutate_group_global_role(&state, &proof, "global-test", "viewer", true)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER drift_other_role AFTER INSERT ON group_global_roles WHEN NEW.role_id='contributor' BEGIN UPDATE global_roles SET permissions='[]' WHERE id='viewer'; END").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(
            mutate_group_global_role(&state, &proof, "global-test", "contributor", true)
                .await
                .is_err()
        );
        assert_eq!(move_database_snapshot(&state).await, before);
    }

    #[tokio::test]
    async fn actor_audit_real_service_and_human_rows_jsonl_and_deleted_key_provenance() {
        for service in [false, true] {
            let state = setup(false).await;
            let proof = admin(&state, service).await;
            let event = mutate_group_global_role(&state, &proof, "global-test", "viewer", true)
                .await
                .unwrap();
            let row: (
                Option<String>,
                Option<String>,
                Option<String>,
                String,
                Option<String>,
                Option<String>,
            ) = sqlx::query_as(
                "SELECT user_id,actor_kind,actor_id,action,target_type,target_id FROM audit_log",
            )
            .fetch_one(&state.db)
            .await
            .unwrap();
            assert_eq!(
                row,
                (
                    if service {
                        None
                    } else {
                        Some("owner-a".into())
                    },
                    Some(if service { "service_key" } else { "user" }.into()),
                    Some(if service { "fence-service" } else { "owner-a" }.into()),
                    "group.role_assign".into(),
                    Some("group".into()),
                    Some("global-test".into())
                )
            );
            let directory = std::env::temp_dir().join(format!("actor-audit-{}", Uuid::new_v4()));
            crate::audit::test_append_actor_action(&directory, &event).await;
            let file = std::fs::read_dir(&directory)
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            let value: serde_json::Value =
                serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
            assert_eq!(value["actor_kind"], row.1.clone().unwrap());
            assert_eq!(value["actor_id"], row.2.clone().unwrap());
            assert_eq!(value["user_id"], serde_json::to_value(&row.0).unwrap());
            assert!(!value.to_string().contains("key_hash"));
            if service {
                sqlx::query("DELETE FROM api_keys WHERE id='fence-service'")
                    .execute(&state.db)
                    .await
                    .unwrap();
                let retained: (Option<String>, Option<String>) =
                    sqlx::query_as("SELECT actor_kind,actor_id FROM audit_log")
                        .fetch_one(&state.db)
                        .await
                        .unwrap();
                assert_eq!(retained, (row.1, row.2));
            }
        }
    }
    #[tokio::test]
    async fn actor_audit_insert_failure_rolls_back_grant_and_noop() {
        for (existing, assign) in [(false, true), (true, true), (true, false)] {
            let state = setup(false).await;
            let proof = admin(&state, true).await;
            if existing {
                mutate_group_global_role(&state, &proof, "global-test", "viewer", true)
                    .await
                    .unwrap();
            }
            sqlx::query("CREATE TRIGGER reject_actor_audit BEFORE INSERT ON audit_log BEGIN SELECT RAISE(ABORT,'audit unavailable'); END").execute(&state.db).await.unwrap();
            let before = move_database_snapshot(&state).await;
            assert!(
                mutate_group_global_role(&state, &proof, "global-test", "viewer", assign)
                    .await
                    .is_err()
            );
            assert_eq!(move_database_snapshot(&state).await, before);
        }
    }
    #[tokio::test]
    async fn actor_audit_late_policy_credential_and_event_drift_roll_back() {
        for sql in [
            "DELETE FROM api_keys WHERE id='fence-service';",
            "UPDATE groups SET name='changed' WHERE id='global-test';",
            "UPDATE audit_log SET actor_id='different';",
            "DELETE FROM audit_log;",
            "UPDATE audit_log SET before_value='{}';",
            "UPDATE audit_log SET after_value='{}';",
        ] {
            let state = setup(false).await;
            let proof = admin(&state, true).await;
            sqlx::query(&format!(
                "CREATE TRIGGER change_actor_audit AFTER INSERT ON audit_log BEGIN {sql} END"
            ))
            .execute(&state.db)
            .await
            .unwrap();
            let before = move_database_snapshot(&state).await;
            assert!(
                mutate_group_global_role(&state, &proof, "global-test", "viewer", true)
                    .await
                    .is_err(),
                "{sql}"
            );
            assert_eq!(move_database_snapshot(&state).await, before, "{sql}");
        }
        let state = setup(false).await;
        let proof = admin(&state, false).await;
        sqlx::query("CREATE TRIGGER revoke_actor_audit AFTER INSERT ON audit_log BEGIN DELETE FROM sessions WHERE user_id='owner-a'; END").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(
            mutate_group_global_role(&state, &proof, "global-test", "viewer", true)
                .await
                .is_err()
        );
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn actor_audit_successful_noop_and_remove_audit_once_denied_paths_never_audit() {
        let state = setup(false).await;
        let proof = admin(&state, true).await;
        for _ in 0..2 {
            mutate_group_global_role(&state, &proof, "global-test", "viewer", true)
                .await
                .unwrap();
        }
        mutate_group_global_role(&state, &proof, "global-test", "viewer", false)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(matches!(
            mutate_group_global_role(&state, &proof, "global-test", "viewer", false).await,
            Err(AppError::NotFound(_))
        ));
        assert_eq!(move_database_snapshot(&state).await, before);
        let actions: Vec<String> =
            sqlx::query_scalar("SELECT action FROM audit_log ORDER BY created_at,id")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(
            actions
                .iter()
                .filter(|s| s.as_str() == "group.role_assign")
                .count(),
            2
        );
        assert_eq!(
            actions
                .iter()
                .filter(|s| s.as_str() == "group.role_remove")
                .count(),
            1
        );
        sqlx::query("DELETE FROM api_keys WHERE id='fence-service'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(
            mutate_group_global_role(&state, &proof, "global-test", "viewer", true)
                .await
                .is_err()
        );
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn actor_audit_json_csv_legacy_null_and_actual_identity_projection() {
        use tower::ServiceExt;
        let state = setup(false).await;
        let proof = admin(&state, true).await;
        mutate_group_global_role(&state, &proof, "global-test", "viewer", true)
            .await
            .unwrap();
        sqlx::query("INSERT INTO audit_log(id,user_id,action) VALUES('legacy-audit','owner-a','legacy.action')").execute(&state.db).await.unwrap();
        let raw_id = "key,\"line\nnext";
        sqlx::query("INSERT INTO api_keys(id,name,key_hash,key_prefix,is_service_account,scopes) VALUES($1,'CSV',$2,'test',1,'[\"admin\"]')").bind(&raw_id).bind(crate::auth::api_keys::hash_api_key("csv-service")).execute(&state.db).await.unwrap();
        let request = axum::http::Request::builder()
            .method("POST")
            .header("X-API-Key", "csv-service")
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();
        let raw_proof = AdminUser::from_request_parts(&mut parts, &state)
            .await
            .unwrap();
        mutate_group_global_role(&state, &raw_proof, "global-test", "viewer", true)
            .await
            .unwrap();
        let app = crate::audit::router().with_state(state.clone());
        let request = |path: &str| {
            axum::http::Request::builder()
                .uri(path)
                .header("X-API-Key", "fence-service")
                .body(axum::body::Body::empty())
                .unwrap()
        };
        let response = app
            .clone()
            .oneshot(request("/admin/audit-log"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let value: serde_json::Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let rows = value.as_array().unwrap();
        let legacy = rows.iter().find(|r| r["id"] == "legacy-audit").unwrap();
        assert!(legacy["actor_kind"].is_null() && legacy["actor_id"].is_null());
        assert_eq!(legacy["user_id"], "owner-a");
        assert!(rows.iter().any(|r| r["actor_id"] == raw_id));
        let current = rows
            .iter()
            .find(|r| r["action"] == "group.role_assign" && r["actor_id"] == "fence-service")
            .unwrap();
        assert_eq!(current["actor_id"], "fence-service");
        assert!(current["user_id"].is_null() && current["user_email"].is_null());
        let response = app
            .oneshot(request("/admin/audit-log/export"))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let csv = String::from_utf8(
            axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(csv.starts_with("id,user_email,ip_address,action,target_type,target_id,created_at,actor_kind,actor_id\n"));
        assert!(csv
            .lines()
            .any(|r| r.ends_with(",service_key,fence-service")));
        assert!(csv
            .lines()
            .any(|r| r.starts_with("legacy-audit,") && r.ends_with(",,")));
        assert!(csv.contains(",service_key,\"key,\"\"line\nnext\"\n"));
    }
}

#[cfg(test)]
mod sole_space_group_member_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use axum::extract::FromRequestParts;
    use tower::ServiceExt;
    async fn setup(wal: bool) -> AppState {
        let state = catalog_tests::catalog_state(wal).await;
        sqlx::query("INSERT INTO groups(id,name) VALUES('sole-group','Sole group')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('sole-grant','space-a','sole-group','editor')").execute(&state.db).await.unwrap();
        state
    }
    async fn proof(state: &AppState, actor: &str, key: Option<&str>) -> AuthUser {
        let mut request = axum::http::Request::builder()
            .method("POST")
            .uri("/spaces/space-a/groups/sole-group/members");
        if let Some(key) = key {
            request = request.header("X-API-Key", key);
        } else {
            let token = jsonwebtoken::encode(
                &jsonwebtoken::Header::default(),
                &catalog_tests::auth(actor).claims,
                &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
            )
            .unwrap();
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let (mut parts, _) = request.body(()).unwrap().into_parts();
        AuthUser::from_request_parts(&mut parts, state)
            .await
            .unwrap()
    }
    async fn http(state: &AppState, method: &str, key: &str, body: &str) -> axum::http::StatusCode {
        let request = axum::http::Request::builder()
            .method(method)
            .uri(if method == "POST" {
                "/spaces/space-a/groups/sole-group/members"
            } else {
                "/spaces/space-a/groups/sole-group/members/viewer-a"
            })
            .header("X-API-Key", key)
            .header("Content-Type", "application/json")
            .body(axum::body::Body::from(body.to_owned()))
            .unwrap();
        router()
            .with_state(state.clone())
            .oneshot(request)
            .await
            .unwrap()
            .status()
    }
    #[tokio::test]
    async fn sole_group_manual_null_and_managed_url_preserve_idempotent_status_inactive_targets_and_creator(
    ) {
        for managed in [false, true] {
            let state = setup(false).await;
            if managed {
                sqlx::query("UPDATE groups SET managed_space_id='space-a' WHERE id='sole-group'")
                    .execute(&state.db)
                    .await
                    .unwrap();
            }
            sqlx::query("UPDATE users SET is_active=0 WHERE id='viewer-a'")
                .execute(&state.db)
                .await
                .unwrap();
            for _ in 0..2 {
                let response = add_space_group_member(
                    State(state.clone()),
                    proof(&state, "owner-a", None).await,
                    Path(("space-a".into(), "sole-group".into())),
                    Json(AddGroupMemberRequest {
                        user_id: "viewer-a".into(),
                    }),
                )
                .await
                .unwrap()
                .into_response();
                assert_eq!(response.status(), StatusCode::CREATED);
            }
            for _ in 0..2 {
                let response = remove_space_group_member(
                    State(state.clone()),
                    proof(&state, "owner-a", None).await,
                    Path(("space-a".into(), "sole-group".into(), "viewer-a".into())),
                )
                .await
                .unwrap()
                .into_response();
                assert_eq!(response.status(), StatusCode::NO_CONTENT);
            }
            let creator:(String,String)=sqlx::query_as("SELECT s.created_by,m.role FROM spaces s JOIN space_members m ON m.space_id=s.id AND m.user_id=s.created_by WHERE s.id='space-a'").fetch_one(&state.db).await.unwrap();
            assert_eq!(creator, ("owner-a".into(), "owner".into()));
        }
    }
    #[tokio::test]
    async fn sole_group_broad_fanout_concealed_missing_and_scim_preserved() {
        let mut messages = Vec::new();
        for sql in ["INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('other-grant','space-b','sole-group','viewer')","INSERT INTO group_global_roles(group_id,role_id) VALUES('sole-group','viewer')"] {let state=setup(false).await;let auth=proof(&state,"owner-a",None).await;sqlx::query(sql).execute(&state.db).await.unwrap();let before=move_database_snapshot(&state).await;let error=mutate_sole_space_group_member(&state,&auth,"space-a","sole-group","viewer-a",true).await.err().unwrap();assert!(matches!(error,AppError::Conflict(_)));if let AppError::Conflict(message)=error {messages.push(message);} else {panic!("expected concealed fanout conflict");}assert_eq!(move_database_snapshot(&state).await,before);}
        assert!(messages.iter().all(|message|message=="Group membership affects additional access. Use an instance-admin group endpoint."));
        for sql in [
            "UPDATE groups SET managed_space_id='space-b' WHERE id='sole-group'",
            "DELETE FROM space_group_permissions WHERE group_id='sole-group'",
        ] {
            let state = setup(false).await;
            let auth = proof(&state, "owner-a", None).await;
            sqlx::query(sql).execute(&state.db).await.unwrap();
            let before = move_database_snapshot(&state).await;
            assert!(matches!(
                mutate_sole_space_group_member(
                    &state,
                    &auth,
                    "space-a",
                    "sole-group",
                    "viewer-a",
                    true
                )
                .await,
                Err(AppError::NotFound(_))
            ));
            assert_eq!(move_database_snapshot(&state).await, before);
        }
        let state = setup(false).await;
        let auth = proof(&state, "owner-a", None).await;
        sqlx::query("UPDATE groups SET scim_managed=1 WHERE id='sole-group'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(
            matches!(mutate_sole_space_group_member(&state,&auth,"space-a","sole-group","viewer-a",true).await,Err(AppError::Conflict(ref s)) if s.contains("identity provider"))
        );
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn sole_group_actual_http_read_write_keys_and_current_manage_only_ceiling() {
        let state = setup(false).await;
        for (key, scopes) in [("sole-write", "[\"write\"]"), ("sole-read", "[\"read\"]")] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'owner-a',$1,$2,'test',$3)").bind(key).bind(crate::auth::api_keys::hash_api_key(key)).bind(scopes).execute(&state.db).await.unwrap();
        }
        sqlx::query("INSERT INTO api_keys(id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('sole-service','Service',$1,'test',1,'[\"admin\"]')").bind(crate::auth::api_keys::hash_api_key("sole-service")).execute(&state.db).await.unwrap();
        assert_eq!(
            http(&state, "POST", "sole-service", "{\"user_id\":\"viewer-a\"}").await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            http(&state, "POST", "sole-write", "{\"user_id\":\"viewer-a\"}").await,
            StatusCode::CREATED
        );
        assert_eq!(
            http(&state, "DELETE", "sole-write", "").await,
            StatusCode::NO_CONTENT
        );
        let before = move_database_snapshot(&state).await;
        assert_eq!(
            http(&state, "POST", "sole-read", "{\"user_id\":\"viewer-a\"}").await,
            StatusCode::FORBIDDEN
        );
        assert_eq!(move_database_snapshot(&state).await, before);
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('manage-only','space-a','Manage only','[\"manage_space\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='manage-only' WHERE user_id='owner-a' AND space_id='space-a'").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_group_permissions SET role='owner' WHERE id='sole-grant'")
            .execute(&state.db)
            .await
            .unwrap();
        let auth = proof(&state, "owner-a", None).await;
        let before = move_database_snapshot(&state).await;
        assert!(matches!(
            mutate_sole_space_group_member(
                &state,
                &auth,
                "space-a",
                "sole-group",
                "viewer-a",
                true
            )
            .await,
            Err(AppError::Forbidden(_))
        ));
        assert_eq!(move_database_snapshot(&state).await, before);
        sqlx::query(
            "UPDATE space_members SET role='owner' WHERE user_id='owner-a' AND space_id='space-a'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        mutate_sole_space_group_member(
            &state,
            &proof(&state, "owner-a", None).await,
            "space-a",
            "sole-group",
            "viewer-a",
            true,
        )
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn sole_group_custom_role_expansion_and_group_union_permission_ceiling() {
        let state = setup(false).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('scoped-editor','space-a','Scoped editor','[\"edit_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('scoped-manage','space-a','Scoped manager','[\"manage_space\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='scoped-manage' WHERE user_id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES('actor-group','Actor group')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('actor-group-member','actor-group','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('actor-group-grant','space-a','actor-group','scoped-editor')").execute(&state.db).await.unwrap();
        sqlx::query(
            "UPDATE space_group_permissions SET role='scoped-editor' WHERE id='sole-grant'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        mutate_sole_space_group_member(
            &state,
            &proof(&state, "owner-a", None).await,
            "space-a",
            "sole-group",
            "viewer-a",
            true,
        )
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn sole_group_intentional_self_loss_succeeds_but_unrelated_policy_drift_rolls_back() {
        for drift in [false, true] {
            let state = setup(false).await;
            sqlx::query("UPDATE space_group_permissions SET role='owner' WHERE id='sole-grant'")
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('sole-actor','sole-group','owner-a')").execute(&state.db).await.unwrap();
            sqlx::query("DELETE FROM space_members WHERE space_id='space-a' AND user_id='owner-a'")
                .execute(&state.db)
                .await
                .unwrap();
            let auth = proof(&state, "owner-a", None).await;
            if drift {
                sqlx::query("CREATE TRIGGER late_self_delete AFTER DELETE ON group_members BEGIN INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','viewer-a'); END").execute(&state.db).await.unwrap();
            }
            let before = move_database_snapshot(&state).await;
            let result = mutate_sole_space_group_member(
                &state,
                &auth,
                "space-a",
                "sole-group",
                "owner-a",
                false,
            )
            .await;
            if drift {
                assert!(result.is_err());
                assert_eq!(move_database_snapshot(&state).await, before);
            } else {
                result.unwrap();
                let mut conn = state.db.acquire().await.unwrap();
                assert!(super::super::access::require_space_membership_management(
                    &mut conn, &auth, "space-a"
                )
                .await
                .is_err());
            }
        }
    }
    #[tokio::test]
    async fn sole_group_late_sources_and_credentials_roll_back_all_domain_and_audits() {
        for add in [true, false] {
            for sql in ["DELETE FROM sessions WHERE user_id='owner-a';","UPDATE users SET is_active=0 WHERE id='owner-a';","UPDATE groups SET scim_managed=1 WHERE id='sole-group';","UPDATE space_group_permissions SET role='owner' WHERE id='sole-grant';","INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('late-other','space-b','sole-group','viewer');","INSERT INTO group_global_roles(group_id,role_id) VALUES('sole-group','viewer');","UPDATE group_members SET user_id='owner-a' WHERE id='retained-member';"] {let state=setup(false).await;let auth=proof(&state,"owner-a",None).await;sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('retained-member','sole-group','editor-a')").execute(&state.db).await.unwrap();if !add {mutate_sole_space_group_member(&state,&auth,"space-a","sole-group","viewer-a",true).await.unwrap();}let trigger=format!("CREATE TRIGGER late_sole AFTER {} ON group_members WHEN {}.group_id='sole-group' BEGIN {} END",if add {"INSERT"} else {"DELETE"},if add {"NEW"} else {"OLD"},sql);sqlx::query(&trigger).execute(&state.db).await.unwrap();let before=move_database_snapshot(&state).await;let result=mutate_sole_space_group_member(&state,&auth,"space-a","sole-group","viewer-a",add).await;assert!(result.is_err(),"{add}: {sql}");assert_eq!(move_database_snapshot(&state).await,before,"{add}: {sql}");}
        }
    }
    #[tokio::test]
    async fn sole_group_true_wal_fence_wait_rechecks_session_and_complete_scope() {
        for sql in ["DELETE FROM sessions WHERE user_id='owner-a'","INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('wait-other','space-b','sole-group','viewer')"] {let state=setup(true).await;let auth=proof(&state,"owner-a",None).await;let mut blocker=state.db.begin().await.unwrap();super::super::mutation_guards::lock_audience_for_mutation(&mut blocker).await.unwrap();sqlx::query(sql).execute(&mut *blocker).await.unwrap();let copy=state.clone();let task=tokio::spawn(async move {mutate_sole_space_group_member(&copy,&auth,"space-a","sole-group","viewer-a",true).await});tokio::time::sleep(std::time::Duration::from_millis(100)).await;assert!(!task.is_finished());blocker.commit().await.unwrap();assert!(task.await.unwrap().is_err());let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id='sole-group'").fetch_one(&state.db).await.unwrap();assert_eq!(count,0);}
    }
    #[tokio::test]
    async fn sole_group_late_current_write_key_and_custom_role_policy_rollback() {
        for key_case in [false, true] {
            let state = setup(false).await;
            sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('mutable-role','space-a','Mutable','[\"edit_content\"]','owner-a')").execute(&state.db).await.unwrap();
            sqlx::query(
                "UPDATE space_group_permissions SET role='mutable-role' WHERE id='sole-grant'",
            )
            .execute(&state.db)
            .await
            .unwrap();
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('mutable-key','owner-a','Mutable',$1,'test','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key("mutable-key")).execute(&state.db).await.unwrap();
            let auth = proof(&state, "owner-a", Some("mutable-key")).await;
            sqlx::query(if key_case {"CREATE TRIGGER mutate_key AFTER INSERT ON group_members BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='mutable-key'; END"} else {"CREATE TRIGGER mutate_role AFTER INSERT ON group_members BEGIN UPDATE space_roles SET permissions='[]' WHERE id='mutable-role'; END"}).execute(&state.db).await.unwrap();
            let before = move_database_snapshot(&state).await;
            assert!(mutate_sole_space_group_member(
                &state,
                &auth,
                "space-a",
                "sole-group",
                "viewer-a",
                true
            )
            .await
            .is_err());
            assert_eq!(move_database_snapshot(&state).await, before);
        }
    }
}

#[cfg(test)]
mod admin_group_member_fence_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use axum::extract::FromRequestParts;
    use tower::ServiceExt;
    // The real extractor may update only last_used_at before an Admin scope
    // rejection. Domain/authority/audit rows and every other key field are fixed.
    async fn http_denial_snapshot(state: &AppState) -> Vec<(String, Option<String>)> {
        use sqlx::Row;
        let mut rows = move_database_snapshot(state).await;
        rows.retain(|(table, _)| table != "api_keys");
        let columns = sqlx::query("PRAGMA table_info(api_keys)")
            .fetch_all(&state.db)
            .await
            .unwrap();
        let expression = columns
            .iter()
            .map(|row| row.get::<String, _>("name"))
            .filter(|name| name != "last_used_at")
            .map(|name| format!("quote(\"{}\")", name))
            .collect::<Vec<_>>()
            .join(" || '|' || ");
        let keys:Option<String>=sqlx::query_scalar(&format!("SELECT group_concat({expression},char(10)) FROM (SELECT * FROM api_keys ORDER BY rowid)")).fetch_one(&state.db).await.unwrap();
        rows.push(("api_keys_except_usage".into(), keys));
        rows
    }
    async fn setup(wal: bool) -> AppState {
        let state = catalog_tests::catalog_state(wal).await;
        sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO groups(id,name) VALUES('admin-member-group','Admin member group')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('fanout-custom','space-b','Fanout custom','[\"edit_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('fanout-a','space-a','admin-member-group','owner'),('fanout-b','space-b','admin-member-group','fanout-custom')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_global_roles(group_id,role_id) VALUES('admin-member-group','contributor')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('retained-admin-member','admin-member-group','editor-a')").execute(&state.db).await.unwrap();
        state
    }
    async fn key(state: &AppState, id: &str, service: bool, scopes: &str) {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES($1,$2,$1,$3,'test',$4,$5)").bind(id).bind((!service).then_some("owner-a")).bind(crate::auth::api_keys::hash_api_key(id)).bind(i64::from(service)).bind(scopes).execute(&state.db).await.unwrap();
    }
    fn bearer(state: &AppState) -> String {
        jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &catalog_tests::auth("owner-a").claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap()
    }
    async fn proof(state: &AppState, key: Option<&str>) -> AdminUser {
        let mut request = axum::http::Request::builder()
            .method("POST")
            .uri("/admin/groups/admin-member-group/members");
        if let Some(key) = key {
            request = request.header("X-API-Key", key);
        } else {
            request = request.header("Authorization", format!("Bearer {}", bearer(state)));
        }
        let (mut parts, _) = request.body(()).unwrap().into_parts();
        AdminUser::from_request_parts(&mut parts, state)
            .await
            .unwrap()
    }
    async fn http(state: &AppState, method: &str, key: Option<&str>) -> StatusCode {
        let mut request = axum::http::Request::builder()
            .method(method)
            .uri(if method == "POST" {
                "/admin/groups/admin-member-group/members"
            } else {
                "/admin/groups/admin-member-group/members/viewer-a"
            })
            .header("Content-Type", "application/json");
        if let Some(key) = key {
            request = request.header("X-API-Key", key);
        } else {
            request = request.header("Authorization", format!("Bearer {}", bearer(state)));
        }
        let request = request
            .body(axum::body::Body::from(if method == "POST" {
                "{\"user_id\":\"viewer-a\"}"
            } else {
                ""
            }))
            .unwrap();
        router()
            .with_state(state.clone())
            .oneshot(request)
            .await
            .unwrap()
            .status()
    }
    #[tokio::test]
    async fn admin_member_real_http_bearer_user_and_service_admin_scope_compatibility() {
        let state = setup(false).await;
        key(&state, "human-member-admin", false, "[\"admin\"]").await;
        key(&state, "service-member-admin", true, "[\"admin\"]").await;
        key(&state, "human-member-write", false, "[\"write\"]").await;
        key(&state, "human-member-read", false, "[\"read\"]").await;
        for key in [
            None,
            Some("human-member-admin"),
            Some("service-member-admin"),
        ] {
            assert_eq!(http(&state, "POST", key).await, StatusCode::CREATED);
            assert_eq!(http(&state, "DELETE", key).await, StatusCode::NO_CONTENT);
        }
        for key in [Some("human-member-read"), Some("human-member-write")] {
            let before = http_denial_snapshot(&state).await;
            assert_eq!(http(&state, "POST", key).await, StatusCode::FORBIDDEN);
            assert_eq!(http(&state, "DELETE", key).await, StatusCode::FORBIDDEN);
            assert_eq!(http_denial_snapshot(&state).await, before);
        }
    }
    #[tokio::test]
    async fn admin_member_noop_id_time_inactive_dedicated_and_absent_remove_preserved() {
        let state = setup(false).await;
        let admin = proof(&state, None).await;
        sqlx::query("UPDATE groups SET managed_space_id='space-a' WHERE id='admin-member-group'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE users SET is_active=0 WHERE id='viewer-a'")
            .execute(&state.db)
            .await
            .unwrap();
        let source = admin_group_member_source(
            &mut *state.db.acquire().await.unwrap(),
            "admin-member-group",
        )
        .await
        .unwrap();
        mutate_admin_group_member(&state, &admin, "admin-member-group", "viewer-a", true)
            .await
            .unwrap();
        let row:(String,String)=sqlx::query_as("SELECT id,created_at FROM group_members WHERE group_id='admin-member-group' AND user_id='viewer-a'").fetch_one(&state.db).await.unwrap();
        mutate_admin_group_member(&state, &admin, "admin-member-group", "viewer-a", true)
            .await
            .unwrap();
        let repeated:(String,String)=sqlx::query_as("SELECT id,created_at FROM group_members WHERE group_id='admin-member-group' AND user_id='viewer-a'").fetch_one(&state.db).await.unwrap();
        assert_eq!(row, repeated);
        mutate_admin_group_member(&state, &admin, "admin-member-group", "viewer-a", false)
            .await
            .unwrap();
        mutate_admin_group_member(&state, &admin, "admin-member-group", "viewer-a", false)
            .await
            .unwrap();
        mutate_admin_group_member(&state, &admin, "admin-member-group", "unknown-user", false)
            .await
            .unwrap();
        assert_eq!(
            admin_group_member_source(
                &mut *state.db.acquire().await.unwrap(),
                "admin-member-group"
            )
            .await
            .unwrap(),
            source
        );
        let owner:(String,String)=sqlx::query_as("SELECT s.created_by,m.role FROM spaces s JOIN space_members m ON m.space_id=s.id AND m.user_id=s.created_by WHERE s.id='space-a'").fetch_one(&state.db).await.unwrap();
        assert_eq!(owner, ("owner-a".into(), "owner".into()));
    }
    #[tokio::test]
    async fn admin_member_missing_group_and_scim_fail_without_domain_or_audit_changes() {
        let state = setup(false).await;
        let admin = proof(&state, None).await;
        let before = move_database_snapshot(&state).await;
        for add in [true, false] {
            assert!(matches!(
                mutate_admin_group_member(&state, &admin, "missing-group", "viewer-a", add).await,
                Err(AppError::NotFound(_))
            ));
            assert_eq!(move_database_snapshot(&state).await, before);
        }
        sqlx::query("UPDATE groups SET scim_managed=1 WHERE id='admin-member-group'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        for add in [true, false] {
            assert!(
                matches!(mutate_admin_group_member(&state,&admin,"admin-member-group","viewer-a",add).await,Err(AppError::Conflict(ref text)) if text=="This group is managed by SCIM. Update it through the identity provider.")
            );
            assert_eq!(move_database_snapshot(&state).await, before);
        }
    }
    #[tokio::test]
    async fn admin_member_self_remove_keeps_actual_instance_admin_not_global_role_label() {
        let state = setup(false).await;
        let admin = proof(&state, None).await;
        mutate_admin_group_member(&state, &admin, "admin-member-group", "owner-a", true)
            .await
            .unwrap();
        mutate_admin_group_member(&state, &admin, "admin-member-group", "owner-a", false)
            .await
            .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        assert!(matches!(
            crate::auth::middleware::require_current_admin(&mut conn, &admin)
                .await
                .unwrap(),
            crate::auth::middleware::CurrentAdminAuthority::HumanInstanceAdmin { .. }
        ));
    }
    #[tokio::test]
    async fn admin_member_late_domain_credential_and_audit_drift_full_rollback() {
        for add in [true, false] {
            for sql in [
            "UPDATE users SET role='member' WHERE id='owner-a';",
            "DELETE FROM sessions WHERE user_id='owner-a';",
            "UPDATE groups SET scim_managed=1 WHERE id='admin-member-group';",
            "UPDATE global_roles SET permissions='[]' WHERE id='contributor';",
            "UPDATE space_roles SET permissions='[]',updated_at='late-policy' WHERE id='fanout-custom';",
            "UPDATE space_group_permissions SET role='viewer' WHERE id='fanout-b';",
            "INSERT INTO group_global_roles(group_id,role_id) VALUES('admin-member-group','viewer');",
            "UPDATE group_members SET user_id='owner-a' WHERE id='retained-admin-member';",
            "INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('new-audience-space','New audience','new-audience-space','private','owner-a');",
        ] {let state=setup(false).await;let admin=proof(&state,None).await;if !add {mutate_admin_group_member(&state,&admin,"admin-member-group","viewer-a",true).await.unwrap();}let trigger=format!("CREATE TRIGGER late_admin_member AFTER {} ON group_members WHEN {}.user_id='viewer-a' BEGIN {} END",if add {"INSERT"} else {"DELETE"},if add {"NEW"} else {"OLD"},sql);sqlx::query(&trigger).execute(&state.db).await.unwrap();let before=move_database_snapshot(&state).await;assert!(mutate_admin_group_member(&state,&admin,"admin-member-group","viewer-a",add).await.is_err(),"{add}: {sql}");assert_eq!(move_database_snapshot(&state).await,before,"{add}: {sql}");}
        }
    }
    #[tokio::test]
    async fn admin_member_true_wal_fence_wait_current_key_session_account_and_latest_scope() {
        for mode in [
            "bearer-session",
            "bearer-account",
            "human-key",
            "service-key",
        ] {
            let state = setup(true).await;
            let admin = match mode {
                "human-key" => {
                    key(&state, "wait-member-key", false, "[\"admin\"]").await;
                    proof(&state, Some("wait-member-key")).await
                }
                "service-key" => {
                    key(&state, "wait-member-key", true, "[\"admin\"]").await;
                    proof(&state, Some("wait-member-key")).await
                }
                _ => proof(&state, None).await,
            };
            let mut blocker = state.db.begin().await.unwrap();
            super::super::mutation_guards::lock_audience_for_mutation(&mut blocker)
                .await
                .unwrap();
            sqlx::query(match mode {
                "bearer-session" => "DELETE FROM sessions WHERE user_id='owner-a'",
                "bearer-account" => "UPDATE users SET is_active=0 WHERE id='owner-a'",
                _ => "UPDATE api_keys SET scopes='[\"read\"]' WHERE id='wait-member-key'",
            })
            .execute(&mut *blocker)
            .await
            .unwrap();
            let copy = state.clone();
            let task = tokio::spawn(async move {
                mutate_admin_group_member(&copy, &admin, "admin-member-group", "viewer-a", true)
                    .await
            });
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!task.is_finished());
            blocker.commit().await.unwrap();
            assert!(task.await.unwrap().is_err());
            let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id='admin-member-group' AND user_id='viewer-a'").fetch_one(&state.db).await.unwrap();
            assert_eq!(count, 0);
        }
    }
    #[tokio::test]
    async fn admin_member_original_jwt_expiry_after_real_fence_wait_denied() {
        let state = setup(true).await;
        let mut claims = catalog_tests::auth("owner-a").claims;
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let request = axum::http::Request::builder()
            .method("POST")
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();
        let admin = AdminUser::from_request_parts(&mut parts, &state)
            .await
            .unwrap();
        let mut blocker = state.db.begin().await.unwrap();
        super::super::mutation_guards::lock_audience_for_mutation(&mut blocker)
            .await
            .unwrap();
        let copy = state.clone();
        let task = tokio::spawn(async move {
            mutate_admin_group_member(&copy, &admin, "admin-member-group", "viewer-a", true).await
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!task.is_finished());
        while chrono::Utc::now().timestamp() <= claims.exp {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        blocker.commit().await.unwrap();
        assert!(task.await.unwrap().is_err());
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM group_members WHERE group_id='admin-member-group' AND user_id='viewer-a'").fetch_one(&state.db).await.unwrap();
        assert_eq!(count, 0);
    }
    #[tokio::test]
    async fn admin_member_same_tx_audit_tamper_failure_and_trigger_policy_drift_rollback() {
        for sql in [
            "UPDATE audit_log SET target_id='foreign-group' WHERE id=NEW.id;",
            "UPDATE audit_log SET action='different-action' WHERE id=NEW.id;",
            "UPDATE users SET role='member' WHERE id='owner-a';",
            "UPDATE space_group_permissions SET role='viewer' WHERE id='fanout-a';",
            "SELECT RAISE(ABORT,'audit insert refused');",
        ] {
            let state = setup(false).await;
            let admin = proof(&state, None).await;
            sqlx::query(&format!("CREATE TRIGGER late_member_audit AFTER INSERT ON audit_log WHEN NEW.action='group.member_add' BEGIN {sql} END")).execute(&state.db).await.unwrap();
            let before = move_database_snapshot(&state).await;
            assert!(
                mutate_admin_group_member(&state, &admin, "admin-member-group", "viewer-a", true)
                    .await
                    .is_err(),
                "{sql}"
            );
            assert_eq!(move_database_snapshot(&state).await, before, "{sql}");
        }
    }
    #[tokio::test]
    async fn admin_member_service_audit_actual_key_without_human_fk_and_late_revoke() {
        let state = setup(false).await;
        key(&state, "member-service", true, "[\"admin\"]").await;
        let admin = proof(&state, Some("member-service")).await;
        mutate_admin_group_member(&state, &admin, "admin-member-group", "viewer-a", true)
            .await
            .unwrap();
        let (actor,user):(String,Option<String>)=sqlx::query_as("SELECT actor_id,user_id FROM audit_log WHERE action='group.member_add' ORDER BY created_at DESC LIMIT 1").fetch_one(&state.db).await.unwrap();
        assert_eq!(actor, "member-service");
        assert_eq!(user, None);
        sqlx::query("CREATE TRIGGER late_member_service_audit AFTER INSERT ON audit_log WHEN NEW.action='group.member_remove' BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='member-service'; END").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(
            mutate_admin_group_member(&state, &admin, "admin-member-group", "viewer-a", false)
                .await
                .is_err()
        );
        assert_eq!(move_database_snapshot(&state).await, before);
    }
}

#[cfg(test)]
mod scoped_group_grant_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use axum::extract::FromRequestParts;
    use tower::ServiceExt;
    async fn setup(wal: bool) -> AppState {
        let state = catalog_tests::catalog_state(wal).await;
        sqlx::query("INSERT INTO groups(id,name) VALUES('grant-group','Grant group')")
            .execute(&state.db)
            .await
            .unwrap();
        state
    }
    async fn proof(state: &AppState, actor: &str, key: Option<&str>) -> AuthUser {
        let mut req = axum::http::Request::builder()
            .method("POST")
            .uri("/spaces/space-a/groups");
        if let Some(k) = key {
            req = req.header("X-API-Key", k);
        } else {
            let token = jsonwebtoken::encode(
                &jsonwebtoken::Header::default(),
                &catalog_tests::auth(actor).claims,
                &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
            )
            .unwrap();
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        let (mut parts, _) = req.body(()).unwrap().into_parts();
        AuthUser::from_request_parts(&mut parts, state)
            .await
            .unwrap()
    }
    async fn key(state: &AppState, id: &str, scopes: &str) {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'owner-a',$1,$2,'test',$3)").bind(id).bind(crate::auth::api_keys::hash_api_key(id)).bind(scopes).execute(&state.db).await.unwrap();
    }
    async fn grant(state: &AppState, auth: &AuthUser, role: &str) {
        mutate_scoped_group_grant(
            state,
            auth,
            "space-a",
            "grant-group",
            ScopedGroupGrantChange::Upsert(role),
        )
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn scoped_grant_http_keys_and_path_body_compatibility() {
        let state = setup(false).await;
        key(&state, "grant-write", "[\"write\"]").await;
        key(&state, "grant-read", "[\"read\"]").await;
        let router = super::router().with_state(state.clone());
        for (method, path, k, body, status) in [
            (
                "POST",
                "/spaces/space-a/groups",
                "grant-read",
                "{\"group_id\":\"grant-group\",\"role\":\"viewer\"}",
                StatusCode::FORBIDDEN,
            ),
            (
                "POST",
                "/spaces/space-a/groups",
                "grant-write",
                "{\"group_id\":\"grant-group\"}",
                StatusCode::CREATED,
            ),
            (
                "PUT",
                "/spaces/space-a/groups/grant-group",
                "grant-write",
                "{\"group_id\":\"ignored-foreign\",\"role\":\"editor\"}",
                StatusCode::NO_CONTENT,
            ),
            (
                "DELETE",
                "/spaces/space-a/groups/grant-group",
                "grant-write",
                "",
                StatusCode::NO_CONTENT,
            ),
        ] {
            let response = router
                .clone()
                .oneshot(
                    axum::http::Request::builder()
                        .method(method)
                        .uri(path)
                        .header("X-API-Key", k)
                        .header("Content-Type", "application/json")
                        .body(axum::body::Body::from(body))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), status, "{method}");
        }
        let n: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM space_group_permissions WHERE group_id='grant-group'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(n, 0);
    }
    #[tokio::test]
    async fn scoped_grant_upsert_noop_raw_group_semantics_preserved() {
        let state = setup(false).await;
        let auth = proof(&state, "owner-a", None).await;
        sqlx::query(
            "UPDATE groups SET managed_space_id='space-b',scim_managed=1 WHERE id='grant-group'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('grant-inactive','grant-group','viewer-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE users SET is_active=0 WHERE id='viewer-a'")
            .execute(&state.db)
            .await
            .unwrap();
        // Historical Contributor is supported by existing account resolution,
        // but is not a newly assignable request role.
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role,created_at,updated_at) VALUES('historical-contributor','space-b','grant-group','contributor','old-created','old-updated')").execute(&state.db).await.unwrap();
        let contributor_before:(String,String,String,String,String,String)=sqlx::query_as("SELECT id,space_id,group_id,role,created_at,updated_at FROM space_group_permissions WHERE id='historical-contributor'").fetch_one(&state.db).await.unwrap();
        grant(&state, &auth, "viewer").await;
        let before: (String, String) = sqlx::query_as(
            "SELECT id,created_at FROM space_group_permissions WHERE group_id='grant-group' AND space_id='space-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        grant(&state, &auth, "editor").await;
        let after: (String, String) = sqlx::query_as(
            "SELECT id,created_at FROM space_group_permissions WHERE group_id='grant-group' AND space_id='space-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(before, after);
        mutate_scoped_group_grant(
            &state,
            &auth,
            "space-a",
            "unknown",
            ScopedGroupGrantChange::Update("viewer"),
        )
        .await
        .unwrap();
        mutate_scoped_group_grant(
            &state,
            &auth,
            "space-a",
            "unknown",
            ScopedGroupGrantChange::Remove,
        )
        .await
        .unwrap();
        mutate_scoped_group_grant(
            &state,
            &auth,
            "space-a",
            "grant-group",
            ScopedGroupGrantChange::Remove,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE groups SET managed_space_id='space-a' WHERE id='grant-group'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(matches!(
            mutate_scoped_group_grant(
                &state,
                &auth,
                "space-a",
                "grant-group",
                ScopedGroupGrantChange::Remove
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        assert_eq!(move_database_snapshot(&state).await, before);
        let contributor_after:(String,String,String,String,String,String)=sqlx::query_as("SELECT id,space_id,group_id,role,created_at,updated_at FROM space_group_permissions WHERE id='historical-contributor'").fetch_one(&state.db).await.unwrap();
        assert_eq!(contributor_after, contributor_before);
        let before = move_database_snapshot(&state).await;
        assert!(mutate_scoped_group_grant(
            &state,
            &auth,
            "space-a",
            "grant-group",
            ScopedGroupGrantChange::Upsert("contributor")
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn scoped_grant_current_account_union_ceiling_and_role_scope() {
        let state = setup(false).await;
        let auth = proof(&state, "editor-a", None).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('manage-only','space-a','Manage','[\"manage_space\"]','owner-a'),('grant-edit','space-a','Edit','[\"edit_content\"]','owner-a'),('foreign-role','space-b','Foreign','[\"view\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='manage-only' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        for r in ["grant-edit", "foreign-role", "owner"] {
            assert!(mutate_scoped_group_grant(
                &state,
                &auth,
                "space-a",
                "grant-group",
                ScopedGroupGrantChange::Upsert(r)
            )
            .await
            .is_err());
            assert_eq!(move_database_snapshot(&state).await, before);
        }
        sqlx::query("INSERT INTO groups(id,name) VALUES('actor-union','Actor union')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('union-member','actor-union','editor-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('union-permission','space-a','actor-union','grant-edit')").execute(&state.db).await.unwrap();
        grant(&state, &auth, "grant-edit").await;
    }
    #[tokio::test]
    async fn scoped_grant_intentional_self_loss_and_unrelated_drift() {
        for remove in [false, true] {
            for drift in [false, true] {
                let state = setup(false).await;
                let owner = proof(&state, "owner-a", None).await;
                grant(&state, &owner, "space_admin").await;
                sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('actor-member','grant-group','editor-a')").execute(&state.db).await.unwrap();
                sqlx::query("DELETE FROM space_members WHERE user_id='editor-a'")
                    .execute(&state.db)
                    .await
                    .unwrap();
                let auth = proof(&state, "editor-a", None).await;
                if drift {
                    sqlx::query(if remove{"CREATE TRIGGER grant_drift AFTER DELETE ON space_group_permissions BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END"}else{"CREATE TRIGGER grant_drift AFTER UPDATE OF role ON space_group_permissions BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END"}).execute(&state.db).await.unwrap();
                }
                let before = move_database_snapshot(&state).await;
                let result = mutate_scoped_group_grant(
                    &state,
                    &auth,
                    "space-a",
                    "grant-group",
                    if remove {
                        ScopedGroupGrantChange::Remove
                    } else {
                        ScopedGroupGrantChange::Update("viewer")
                    },
                )
                .await;
                if drift {
                    assert!(result.is_err());
                    assert_eq!(move_database_snapshot(&state).await, before);
                } else {
                    result.unwrap();
                    let mut conn = state.db.acquire().await.unwrap();
                    assert!(super::super::access::require_space_membership_management(
                        &mut conn, &auth, "space-a"
                    )
                    .await
                    .is_err());
                }
            }
        }
    }
    #[tokio::test]
    async fn scoped_grant_late_full_source_and_credentials_rollback() {
        for mode in 0..3 {
            for sql in ["DELETE FROM sessions WHERE user_id='owner-a';","UPDATE users SET is_active=0 WHERE id='owner-a';","UPDATE groups SET description='drift' WHERE id='grant-group';","INSERT INTO group_members(id,group_id,user_id) VALUES('late-member','grant-group','viewer-a');","INSERT INTO group_global_roles(group_id,role_id) VALUES('grant-group','viewer');","INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('other-grant','space-b','grant-group','viewer');"]{let state=setup(false).await;let auth=proof(&state,"owner-a",None).await;if mode!=0 {grant(&state,&auth,"viewer").await;}let event=match mode{0=>"INSERT",1=>"UPDATE OF role",_=>"DELETE"};sqlx::query(&format!("CREATE TRIGGER late_grant AFTER {event} ON space_group_permissions BEGIN {sql} END")).execute(&state.db).await.unwrap();let before=move_database_snapshot(&state).await;let result=mutate_scoped_group_grant(&state,&auth,"space-a","grant-group",match mode{0=>ScopedGroupGrantChange::Upsert("editor"),1=>ScopedGroupGrantChange::Update("editor"),_=>ScopedGroupGrantChange::Remove}).await;assert!(result.is_err(),"{mode}: {sql}");assert_eq!(move_database_snapshot(&state).await,before,"{mode}: {sql}");}
        }
    }
    #[tokio::test]
    async fn scoped_grant_late_definition_and_write_key_rollback() {
        for key_case in [false, true] {
            let state = setup(false).await;
            key(&state, "mutable-grant", "[\"write\"]").await;
            let auth = proof(&state, "owner-a", Some("mutable-grant")).await;
            sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('grant-custom','space-a','Custom','[\"edit_content\"]','owner-a')").execute(&state.db).await.unwrap();
            sqlx::query(if key_case{"CREATE TRIGGER late_key AFTER INSERT ON space_group_permissions BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='mutable-grant'; END"}else{"CREATE TRIGGER late_role AFTER INSERT ON space_group_permissions BEGIN UPDATE space_roles SET permissions='[\"manage_space\"]' WHERE id='grant-custom'; END"}).execute(&state.db).await.unwrap();
            let before = move_database_snapshot(&state).await;
            assert!(mutate_scoped_group_grant(
                &state,
                &auth,
                "space-a",
                "grant-group",
                ScopedGroupGrantChange::Upsert("grant-custom")
            )
            .await
            .is_err());
            assert_eq!(move_database_snapshot(&state).await, before);
        }
    }
    #[tokio::test]
    async fn scoped_grant_real_wal_wait_rechecks_credentials_and_current_group() {
        for sql in ["DELETE FROM sessions WHERE user_id='owner-a'","UPDATE users SET is_active=0 WHERE id='owner-a'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','viewer-a')"]{let state=setup(true).await;let auth=proof(&state,"owner-a",None).await;let mut blocker=state.db.begin().await.unwrap();super::super::mutation_guards::lock_audience_for_mutation(&mut blocker).await.unwrap();sqlx::query(sql).execute(&mut *blocker).await.unwrap();let copy=state.clone();let task=tokio::spawn(async move{mutate_scoped_group_grant(&copy,&auth,"space-a","grant-group",ScopedGroupGrantChange::Upsert("editor")).await});tokio::time::sleep(std::time::Duration::from_millis(100)).await;assert!(!task.is_finished());blocker.commit().await.unwrap();let before=move_database_snapshot(&state).await;assert!(task.await.unwrap().is_err());assert_eq!(move_database_snapshot(&state).await,before);}
    }
    #[tokio::test]
    async fn scoped_grant_orphan_group_associations_fail_without_repair_or_mutation() {
        for association in ["grant", "member", "global"] {
            for remove in [false, true] {
                let state = setup(false).await;
                let auth = proof(&state, "owner-a", None).await;
                match association {
                    "grant" => grant(&state, &auth, "viewer").await,
                    "member" => {
                        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('orphan-member','grant-group','viewer-a')").execute(&state.db).await.unwrap();
                    }
                    "global" => {
                        sqlx::query("INSERT INTO group_global_roles(group_id,role_id) VALUES('grant-group','viewer')").execute(&state.db).await.unwrap();
                    }
                    _ => unreachable!(),
                }
                // Actual migrated schema, with only legacy corruption seeding
                // temporarily bypassing its FK enforcement on this one connection.
                let mut conn = state.db.acquire().await.unwrap();
                sqlx::query("PRAGMA foreign_keys=OFF")
                    .execute(&mut *conn)
                    .await
                    .unwrap();
                sqlx::query("DELETE FROM groups WHERE id='grant-group'")
                    .execute(&mut *conn)
                    .await
                    .unwrap();
                sqlx::query("PRAGMA foreign_keys=ON")
                    .execute(&mut *conn)
                    .await
                    .unwrap();
                drop(conn);
                let before = move_database_snapshot(&state).await;
                let error = mutate_scoped_group_grant(
                    &state,
                    &auth,
                    "space-a",
                    "grant-group",
                    if remove {
                        ScopedGroupGrantChange::Remove
                    } else {
                        ScopedGroupGrantChange::Update("editor")
                    },
                )
                .await
                .unwrap_err();
                assert!(
                    matches!(error, AppError::Conflict(_)),
                    "{association}/{remove}: {error:?}"
                );
                assert_eq!(
                    move_database_snapshot(&state).await,
                    before,
                    "{association}/{remove}"
                );
            }
        }
    }
}

#[cfg(test)]
mod custom_space_role_mutation_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use axum::extract::FromRequestParts;
    use serde_json::Value;
    use tower::ServiceExt;
    async fn setup(wal: bool) -> AppState {
        catalog_tests::catalog_state(wal).await
    }
    async fn proof(state: &AppState, actor: &str, key: Option<&str>) -> AuthUser {
        let mut req = axum::http::Request::builder().method("POST");
        if let Some(k) = key {
            req = req.header("X-API-Key", k);
        } else {
            let token = jsonwebtoken::encode(
                &jsonwebtoken::Header::default(),
                &catalog_tests::auth(actor).claims,
                &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
            )
            .unwrap();
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        let (mut parts, _) = req.body(()).unwrap().into_parts();
        AuthUser::from_request_parts(&mut parts, state)
            .await
            .unwrap()
    }
    fn req(perms: Vec<SpacePermission>) -> SaveSpaceRoleRequest {
        SaveSpaceRoleRequest {
            name: "  Valid Role  ".into(),
            description: None,
            permissions: perms,
        }
    }
    async fn create(state: &AppState, auth: &AuthUser) {
        mutate_custom_space_role(
            state,
            auth,
            "space-a",
            "custom:target",
            CustomSpaceRoleChange::Create(&req(vec![
                SpacePermission::View,
                SpacePermission::Comment,
            ])),
        )
        .await
        .unwrap();
    }
    async fn key(state: &AppState, id: &str, scopes: &str) {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'owner-a',$1,$2,'test',$3)").bind(id).bind(crate::auth::api_keys::hash_api_key(id)).bind(scopes).execute(&state.db).await.unwrap();
    }
    #[tokio::test]
    async fn custom_role_real_http_write_only_and_wire_semantics() {
        let state = setup(false).await;
        key(&state, "role-write", "[\"write\"]").await;
        key(&state, "role-read", "[\"read\"]").await;
        let app = super::router().with_state(state.clone());
        for k in ["role-read", "role-write"] {
            let response=app.clone().oneshot(axum::http::Request::builder().method("POST").uri("/spaces/space-a/roles").header("X-API-Key",k).header("Content-Type","application/json").body(axum::body::Body::from("{\"name\":\"  HTTP role  \",\"description\":null,\"permissions\":[\"view\",\"comment\"]}")).unwrap()).await.unwrap();
            if k == "role-read" {
                assert_eq!(response.status(), StatusCode::FORBIDDEN);
                continue;
            }
            assert_eq!(response.status(), StatusCode::CREATED);
            let payload: Value = serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap(),
            )
            .unwrap();
            let id = payload["id"].as_str().unwrap();
            assert!(id.starts_with("custom:"));
            Uuid::parse_str(id.strip_prefix("custom:").unwrap()).unwrap();
            let row: (String, String, String) =
                sqlx::query_as("SELECT name,description,permissions FROM space_roles WHERE id=$1")
                    .bind(id)
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(
                row,
                (
                    "HTTP role".into(),
                    "".into(),
                    "[\"view\",\"comment\"]".into()
                )
            );
            for method in ["PUT", "DELETE"] {
                let r=app.clone().oneshot(axum::http::Request::builder().method(method).uri(format!("/spaces/space-a/roles/{id}")).header("X-API-Key","role-write").header("Content-Type","application/json").body(axum::body::Body::from(if method=="PUT"{"{\"name\":\"Changed\",\"description\":\" trim \",\"permissions\":[\"view\"]}"}else{""})).unwrap()).await.unwrap();
                assert_eq!(r.status(), StatusCode::NO_CONTENT);
            }
        }
    }
    #[tokio::test]
    async fn custom_role_validation_and_scoped_missing_preserve_database() {
        let state = setup(false).await;
        let auth = proof(&state, "owner-a", None).await;
        let before = move_database_snapshot(&state).await;
        for invalid in [
            SaveSpaceRoleRequest {
                name: " ".into(),
                description: None,
                permissions: vec![SpacePermission::View],
            },
            req(vec![SpacePermission::ManageSpace]),
            SaveSpaceRoleRequest {
                name: "valid".into(),
                description: Some("x".repeat(241)),
                permissions: vec![SpacePermission::View],
            },
        ] {
            assert!(matches!(
                mutate_custom_space_role(
                    &state,
                    &auth,
                    "space-a",
                    "custom:target",
                    CustomSpaceRoleChange::Create(&invalid)
                )
                .await,
                Err(AppError::BadRequest(_))
            ));
            assert_eq!(move_database_snapshot(&state).await, before);
        }
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('foreign-role','space-b','Foreign','[\"view\"]','owner-a')").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        for action in [
            CustomSpaceRoleChange::Update(&req(vec![SpacePermission::View])),
            CustomSpaceRoleChange::Delete,
        ] {
            assert!(matches!(
                mutate_custom_space_role(&state, &auth, "space-a", "foreign-role", action).await,
                Err(AppError::NotFound(_))
            ));
            assert_eq!(move_database_snapshot(&state).await, before);
        }
    }
    #[tokio::test]
    async fn custom_role_legacy_expanded_ceiling_keeps_global_permissions_granular() {
        let state = setup(false).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('manager-role','space-a','Manager','[\"view\",\"manage_space\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='manager-role' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES('role-union','Role union')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('role-union-member','role-union','editor-a')").execute(&state.db).await.unwrap();
        sqlx::query(
            "INSERT INTO group_global_roles(group_id,role_id) VALUES('role-union','contributor')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let auth = proof(&state, "editor-a", None).await;
        let request = req(vec![SpacePermission::View, SpacePermission::EditContent]);
        let before = move_database_snapshot(&state).await;
        assert!(matches!(
            mutate_custom_space_role(
                &state,
                &auth,
                "space-a",
                "custom:target",
                CustomSpaceRoleChange::Create(&request)
            )
            .await,
            Err(AppError::Forbidden(_))
        ));
        assert_eq!(move_database_snapshot(&state).await, before);
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('role-union-space','space-a','role-union','editor')").execute(&state.db).await.unwrap();
        mutate_custom_space_role(
            &state,
            &auth,
            "space-a",
            "custom:target",
            CustomSpaceRoleChange::Create(&request),
        )
        .await
        .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let permissions = super::super::access::space_membership_role_permissions(
            &mut conn,
            "space-a",
            "custom:target",
        )
        .await
        .unwrap();
        assert!(permissions.contains(&super::super::access::Permission::ManageFolders));
        assert!(permissions.contains(&super::super::access::Permission::DeleteContent));
    }
    #[tokio::test]
    async fn custom_role_intentional_self_policy_loss_is_exact() {
        for drift in [false, true] {
            let state = setup(false).await;
            sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('custom:target','space-a','Self manager','[\"view\",\"manage_space\"]','owner-a')").execute(&state.db).await.unwrap();
            sqlx::query("UPDATE space_members SET role='custom:target' WHERE user_id='editor-a'")
                .execute(&state.db)
                .await
                .unwrap();
            let auth = proof(&state, "editor-a", None).await;
            if drift {
                sqlx::query("CREATE TRIGGER role_self_drift AFTER UPDATE OF permissions ON space_roles BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END").execute(&state.db).await.unwrap();
            }
            let before = move_database_snapshot(&state).await;
            let result = mutate_custom_space_role(
                &state,
                &auth,
                "space-a",
                "custom:target",
                CustomSpaceRoleChange::Update(&req(vec![SpacePermission::View])),
            )
            .await;
            if drift {
                assert!(result.is_err());
                assert_eq!(move_database_snapshot(&state).await, before);
            } else {
                result.unwrap();
                let mut conn = state.db.acquire().await.unwrap();
                assert!(super::super::access::require_space_membership_management(
                    &mut conn, &auth, "space-a"
                )
                .await
                .is_err());
            }
        }
    }
    #[tokio::test]
    async fn custom_role_assigned_delete_and_corrupt_source_fail_unchanged() {
        for group in [false, true] {
            let state = setup(false).await;
            let auth = proof(&state, "owner-a", None).await;
            create(&state, &auth).await;
            if group {
                sqlx::query("INSERT INTO groups(id,name) VALUES('role-consumer','Role consumer')")
                    .execute(&state.db)
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('role-grant','space-a','role-consumer','custom:target')").execute(&state.db).await.unwrap();
            } else {
                sqlx::query(
                    "UPDATE space_members SET role='custom:target' WHERE user_id='viewer-a'",
                )
                .execute(&state.db)
                .await
                .unwrap();
            }
            let before = move_database_snapshot(&state).await;
            let e = mutate_custom_space_role(
                &state,
                &auth,
                "space-a",
                "custom:target",
                CustomSpaceRoleChange::Delete,
            )
            .await
            .unwrap_err();
            assert!(matches!(e, AppError::Conflict(_)));
            assert_eq!(move_database_snapshot(&state).await, before);
        }
        let state = setup(false).await;
        let auth = proof(&state, "owner-a", None).await;
        create(&state, &auth).await;
        sqlx::query("UPDATE space_roles SET permissions='corrupt' WHERE id='custom:target'")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(mutate_custom_space_role(
            &state,
            &auth,
            "space-a",
            "custom:target",
            CustomSpaceRoleChange::Update(&req(vec![SpacePermission::View]))
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn custom_role_late_source_account_and_key_rollbacks() {
        for mode in 0..3 {
            for sql in [
                "UPDATE users SET is_active=0 WHERE id='owner-a';",
                "DELETE FROM sessions WHERE user_id='owner-a';",
                "UPDATE api_keys SET scopes='[\"read\"]' WHERE id='role-mutable';",
                "UPDATE space_members SET role='custom:target' WHERE user_id='viewer-a';",
                "UPDATE space_roles SET name='Tampered' WHERE id='custom:target';",
            ] {
                if mode == 2 && sql.contains("UPDATE space_roles SET name") {
                    continue;
                }
                let state = setup(false).await;
                key(&state, "role-mutable", "[\"write\"]").await;
                let auth = proof(
                    &state,
                    "owner-a",
                    if sql.contains("DELETE FROM sessions") {
                        None
                    } else {
                        Some("role-mutable")
                    },
                )
                .await;
                if mode != 0 {
                    create(&state, &auth).await;
                }
                let event = match mode {
                    0 => "INSERT",
                    1 => "UPDATE OF permissions",
                    _ => "DELETE",
                };
                sqlx::query(&format!(
                    "CREATE TRIGGER late_role AFTER {event} ON space_roles BEGIN {sql} END"
                ))
                .execute(&state.db)
                .await
                .unwrap();
                let before = move_database_snapshot(&state).await;
                let request = req(vec![SpacePermission::View]);
                let change = match mode {
                    0 => CustomSpaceRoleChange::Create(&request),
                    1 => CustomSpaceRoleChange::Update(&request),
                    _ => CustomSpaceRoleChange::Delete,
                };
                assert!(
                    mutate_custom_space_role(&state, &auth, "space-a", "custom:target", change)
                        .await
                        .is_err(),
                    "{mode}: {sql}"
                );
                assert_eq!(
                    move_database_snapshot(&state).await,
                    before,
                    "{mode}: {sql}"
                );
            }
        }
    }
    #[tokio::test]
    async fn custom_role_real_wal_credential_wait_rechecks() {
        for sql in [
            "DELETE FROM sessions WHERE user_id='owner-a'",
            "UPDATE users SET is_active=0 WHERE id='owner-a'",
        ] {
            let state = setup(true).await;
            let auth = proof(&state, "owner-a", None).await;
            let mut blocker = state.db.begin().await.unwrap();
            super::super::mutation_guards::lock_audience_for_mutation(&mut blocker)
                .await
                .unwrap();
            sqlx::query(sql).execute(&mut *blocker).await.unwrap();
            let copy = state.clone();
            let task = tokio::spawn(async move {
                mutate_custom_space_role(
                    &copy,
                    &auth,
                    "space-a",
                    "custom:target",
                    CustomSpaceRoleChange::Create(&req(vec![SpacePermission::View])),
                )
                .await
            });
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!task.is_finished());
            blocker.commit().await.unwrap();
            let before = move_database_snapshot(&state).await;
            assert!(task.await.unwrap().is_err());
            assert_eq!(move_database_snapshot(&state).await, before);
        }
    }

    #[tokio::test]
    async fn custom_role_delete_serializes_actual_direct_assignment() {
        let state = setup(true).await;
        let auth = proof(&state, "owner-a", None).await;
        create(&state, &auth).await;
        let delete_actor = proof(&state, "owner-a", None).await;
        let assign_actor = proof(&state, "owner-a", None).await;
        let mut blocker = state.db.begin().await.unwrap();
        super::super::mutation_guards::lock_audience_for_mutation(&mut blocker)
            .await
            .unwrap();
        let deleting = state.clone();
        let deleting_task = tokio::spawn(async move {
            mutate_custom_space_role(
                &deleting,
                &delete_actor,
                "space-a",
                "custom:target",
                CustomSpaceRoleChange::Delete,
            )
            .await
        });
        let assigning = state.clone();
        let assigning_task = tokio::spawn(async move {
            apply_direct_audience_change(
                &assigning,
                &assign_actor,
                "space-a",
                &DirectAudienceChange::Update {
                    user_id: "viewer-a".into(),
                    role: "custom:target".into(),
                },
            )
            .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!deleting_task.is_finished());
        assert!(!assigning_task.is_finished());
        blocker.commit().await.unwrap();
        let deleted = deleting_task.await.unwrap();
        let assigned = assigning_task.await.unwrap();
        let role: String =
            sqlx::query_scalar("SELECT role FROM space_members WHERE user_id='viewer-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM space_roles WHERE id='custom:target'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        // Both real cores run. Either serialized ordering is valid; a successful
        // assignment can never coexist with a successfully deleted definition.
        match (deleted, assigned) {
            (Ok(_), Err(_)) => {
                assert_eq!(exists, 0);
                assert_eq!(role, "viewer");
            }
            (Err(AppError::Conflict(_)), Ok(_)) => {
                assert_eq!(exists, 1);
                assert_eq!(role, "custom:target");
            }
            other => panic!("invalid serialized outcome: {other:?}"),
        }
    }
    #[tokio::test]
    async fn custom_role_actual_jwt_expiry_after_final_source_rolls_back() {
        let state = setup(true).await;
        let mut claims = catalog_tests::auth("owner-a").claims;
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let request = axum::http::Request::builder()
            .method("POST")
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();
        let auth = AuthUser::from_request_parts(&mut parts, &state)
            .await
            .unwrap();
        // Middleware legitimately refreshes the existing session. Pin its
        // current DB expiry far beyond the shorter minted JWT for this fixture.
        sqlx::query("UPDATE sessions SET expires_at='2999-01-01T00:00:00Z' WHERE id=$1")
            .bind(&claims.sid)
            .execute(&state.db)
            .await
            .unwrap();
        *CUSTOM_ROLE_FINAL_EXPIRY.lock().unwrap() = Some(claims.exp);
        let before = move_database_snapshot(&state).await;
        assert!(mutate_custom_space_role(
            &state,
            &auth,
            "space-a",
            "custom:expiry",
            CustomSpaceRoleChange::Create(&req(vec![SpacePermission::View]))
        )
        .await
        .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
        let expiry: String = sqlx::query_scalar("SELECT expires_at FROM sessions WHERE id=$1")
            .bind(&claims.sid)
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(expiry, "2999-01-01T00:00:00Z");
        assert!(CUSTOM_ROLE_FINAL_EXPIRY.lock().unwrap().is_none());
    }
    #[tokio::test]
    async fn custom_role_missing_canonical_consumers_fail_without_repair() {
        for group_case in [false, true] {
            let state = setup(false).await;
            let auth = proof(&state, "owner-a", None).await;
            create(&state, &auth).await;
            if group_case {
                sqlx::query(
                    "INSERT INTO groups(id,name) VALUES('orphan-role-group','Orphan group')",
                )
                .execute(&state.db)
                .await
                .unwrap();
                sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('orphan-role-member','orphan-role-group','viewer-a')").execute(&state.db).await.unwrap();
                sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('orphan-role-grant','space-a','orphan-role-group','custom:target')").execute(&state.db).await.unwrap();
            } else {
                sqlx::query(
                    "UPDATE space_members SET role='custom:target' WHERE user_id='viewer-a'",
                )
                .execute(&state.db)
                .await
                .unwrap();
            }
            let mut conn = state.db.acquire().await.unwrap();
            sqlx::query("PRAGMA foreign_keys=OFF")
                .execute(&mut *conn)
                .await
                .unwrap();
            sqlx::query(if group_case {
                "DELETE FROM groups WHERE id='orphan-role-group'"
            } else {
                "DELETE FROM users WHERE id='viewer-a'"
            })
            .execute(&mut *conn)
            .await
            .unwrap();
            sqlx::query("PRAGMA foreign_keys=ON")
                .execute(&mut *conn)
                .await
                .unwrap();
            drop(conn);
            let before = move_database_snapshot(&state).await;
            assert!(matches!(
                mutate_custom_space_role(
                    &state,
                    &auth,
                    "space-a",
                    "custom:target",
                    CustomSpaceRoleChange::Update(&req(vec![SpacePermission::View]))
                )
                .await,
                Err(AppError::Conflict(_))
            ));
            assert_eq!(move_database_snapshot(&state).await, before);
        }

        // Existing inactive people and real SCIM groups remain valid consumers;
        // identity integrity does not invent eligibility or provider restrictions.
        let state = setup(false).await;
        let auth = proof(&state, "owner-a", None).await;
        create(&state, &auth).await;
        sqlx::query("UPDATE users SET is_active=0 WHERE id='viewer-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE space_members SET role='custom:target' WHERE user_id='viewer-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO groups(id,name,scim_managed) VALUES('actual-scim-role','Actual SCIM',1)",
        )
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('actual-scim-grant','space-a','actual-scim-role','custom:target')").execute(&state.db).await.unwrap();
        mutate_custom_space_role(
            &state,
            &auth,
            "space-a",
            "custom:target",
            CustomSpaceRoleChange::Update(&req(vec![SpacePermission::View])),
        )
        .await
        .unwrap();
        let inactive: i64 = sqlx::query_scalar("SELECT is_active FROM users WHERE id='viewer-a'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(inactive, 0);
        let managed: i64 =
            sqlx::query_scalar("SELECT scim_managed FROM groups WHERE id='actual-scim-role'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(managed, 1);
        let assigned:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM space_members WHERE user_id='viewer-a' AND role='custom:target') + (SELECT COUNT(*) FROM space_group_permissions WHERE id='actual-scim-grant' AND role='custom:target')").fetch_one(&state.db).await.unwrap();
        assert_eq!(assigned, 2);
    }
}

#[cfg(test)]
thread_local! {static AUDIENCE_FINAL_EXPIRY:std::cell::Cell<Option<i64>>=const{std::cell::Cell::new(None)};}
#[cfg(test)]
async fn after_audience_equality_proofs() {
    if let Some(deadline) = AUDIENCE_FINAL_EXPIRY.with(|hook| hook.take()) {
        while chrono::Utc::now().timestamp() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }
}
#[cfg(test)]
mod audience_final_credential_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use axum::extract::FromRequestParts;
    async fn setup(admin: bool) -> AppState {
        let state = catalog_tests::catalog_state(false).await;
        if admin {
            sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'")
                .execute(&state.db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO groups(id,name) VALUES('final-proof-group','Final proof group')")
            .execute(&state.db)
            .await
            .unwrap();
        state
    }
    fn request(state: &AppState) -> axum::http::Request<()> {
        let mut claims = catalog_tests::auth("owner-a").claims;
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        axum::http::Request::builder()
            .method("POST")
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap()
    }
    async fn admin(state: &AppState) -> AdminUser {
        let (mut parts, _) = request(state).into_parts();
        AdminUser::from_request_parts(&mut parts, state)
            .await
            .unwrap()
    }
    async fn human(state: &AppState) -> AuthUser {
        let (mut parts, _) = request(state).into_parts();
        AuthUser::from_request_parts(&mut parts, state)
            .await
            .unwrap()
    }
    #[tokio::test]
    async fn global_role_final_source_and_audit_reads_require_unexpired_actual_jwt() {
        for assign in [true, false] {
            let state = setup(true).await;
            if !assign {
                sqlx::query("INSERT INTO group_global_roles(group_id,role_id) VALUES('final-proof-group','viewer')").execute(&state.db).await.unwrap();
            }
            let proof = admin(&state).await;
            let before = move_database_snapshot(&state).await;
            AUDIENCE_FINAL_EXPIRY.with(|hook| hook.set(Some(proof.claims.exp)));
            let result =
                mutate_group_global_role(&state, &proof, "final-proof-group", "viewer", assign)
                    .await;
            assert!(
                AUDIENCE_FINAL_EXPIRY.with(|hook| hook.get()).is_none(),
                "the final proof boundary was reached"
            );
            assert!(
                matches!(result, Err(AppError::Unauthorized(_))),
                "assign={assign}: expired JWT must not commit"
            );
            assert_eq!(
                move_database_snapshot(&state).await,
                before,
                "domain and exact audit rollback"
            );
        }
    }
    #[tokio::test]
    async fn admin_member_final_source_and_audit_reads_require_unexpired_actual_jwt() {
        for add in [true, false] {
            let state = setup(true).await;
            if !add {
                sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('final-proof-member','final-proof-group','viewer-a')").execute(&state.db).await.unwrap();
            }
            let proof = admin(&state).await;
            let before = move_database_snapshot(&state).await;
            AUDIENCE_FINAL_EXPIRY.with(|hook| hook.set(Some(proof.claims.exp)));
            let result =
                mutate_admin_group_member(&state, &proof, "final-proof-group", "viewer-a", add)
                    .await;
            assert!(
                AUDIENCE_FINAL_EXPIRY.with(|hook| hook.get()).is_none(),
                "the final proof boundary was reached"
            );
            assert!(
                matches!(result, Err(AppError::Unauthorized(_))),
                "add={add}: expired JWT must not commit"
            );
            assert_eq!(
                move_database_snapshot(&state).await,
                before,
                "domain and exact audit rollback"
            );
        }
    }
    #[tokio::test]
    async fn scoped_grant_final_source_and_self_policy_reads_require_unexpired_actual_jwt() {
        for kind in [0, 1, 2] {
            let state = setup(false).await;
            sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('self-proof-member','final-proof-group','owner-a')").execute(&state.db).await.unwrap();
            if kind != 0 {
                sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('final-proof-grant','space-a','final-proof-group','owner')").execute(&state.db).await.unwrap();
            }
            let proof = human(&state).await;
            let before = move_database_snapshot(&state).await;
            AUDIENCE_FINAL_EXPIRY.with(|hook| hook.set(Some(proof.claims.exp)));
            let change = match kind {
                0 => ScopedGroupGrantChange::Upsert("viewer"),
                1 => ScopedGroupGrantChange::Update("viewer"),
                _ => ScopedGroupGrantChange::Remove,
            };
            let result =
                mutate_scoped_group_grant(&state, &proof, "space-a", "final-proof-group", change)
                    .await;
            assert!(
                AUDIENCE_FINAL_EXPIRY.with(|hook| hook.get()).is_none(),
                "the final proof boundary was reached"
            );
            assert!(
                matches!(result, Err(AppError::Unauthorized(_))),
                "kind={kind}: expired JWT must not commit"
            );
            assert_eq!(
                move_database_snapshot(&state).await,
                before,
                "grant and expected self-policy rollback"
            );
        }
    }
}

#[cfg(test)]
pub(crate) mod space_creation_fence_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use axum::extract::FromRequestParts;
    pub(crate) async fn setup(wal: bool) -> AppState {
        let state = catalog_tests::catalog_state(wal).await;
        sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        state
    }
    pub(crate) async fn admin(state: &AppState) -> AdminUser {
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &catalog_tests::auth("owner-a").claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let request = axum::http::Request::builder()
            .method("POST")
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();
        AdminUser::from_request_parts(&mut parts, state)
            .await
            .unwrap()
    }
    pub(crate) async fn fail_insert(state: &AppState, table: &str) {
        sqlx::query(&format!("CREATE TRIGGER fail_space_constituent AFTER INSERT ON {table} BEGIN SELECT RAISE(ABORT,'space constituent failed'); END")).execute(&state.db).await.unwrap();
    }
    #[tokio::test]
    async fn space_creation_rest_each_constituent_failure_rolls_back_entire_topology() {
        for table in [
            "spaces",
            "space_members",
            "groups",
            "group_members",
            "space_group_permissions",
        ] {
            let state = setup(false).await;
            let admin = admin(&state).await;
            fail_insert(&state, table).await;
            let before = move_database_snapshot(&state).await;
            let id = Uuid::new_v4().to_string();
            let name = format!("Atomic {id}");
            let slug = storage::sanitize_slug(&name);
            let req: CreateSpaceRequest = serde_json::from_value(
                serde_json::json!({"source_id":id,"name":name,"visibility":"private"}),
            )
            .unwrap();
            assert!(
                create_space(State(state.clone()), admin, Json(req))
                    .await
                    .is_err(),
                "{table}"
            );
            assert_eq!(move_database_snapshot(&state).await, before, "{table}");
            assert!(
                !storage::space_dir(&slug).exists(),
                "no directories before SQL commit"
            );
        }
    }
    #[tokio::test]
    async fn space_creation_personal_each_constituent_failure_rolls_back_entire_topology() {
        for table in [
            "spaces",
            "space_members",
            "groups",
            "group_members",
            "space_group_permissions",
        ] {
            let state = setup(false).await;
            fail_insert(&state, table).await;
            let before = move_database_snapshot(&state).await;
            let display = format!("Personal {}", Uuid::new_v4());
            let slug = storage::sanitize_slug(&format!("{display}'s Space").replace(' ', "-"));
            assert!(
                crate::auth::routes::create_personal_space(&state, "viewer-a", &display)
                    .await
                    .is_err(),
                "{table}"
            );
            assert_eq!(move_database_snapshot(&state).await, before, "{table}");
            assert!(!storage::space_dir(&slug).exists());
        }
    }
    #[tokio::test]
    async fn space_creation_personal_internal_and_inactive_recipient_contract_unchanged() {
        for active in [0, 1] {
            let state = setup(false).await;
            sqlx::query("UPDATE users SET is_active=$1 WHERE id='viewer-a'")
                .bind(active)
                .execute(&state.db)
                .await
                .unwrap();
            let display = format!("Recipient {}", Uuid::new_v4());
            crate::auth::routes::create_personal_space(&state, "viewer-a", &display)
                .await
                .unwrap();
            let (id, slug, visibility): (String, String, String) =
                sqlx::query_as("SELECT id,slug,visibility FROM spaces WHERE created_by='viewer-a'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(visibility, "internal");
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    "SELECT role FROM space_members WHERE space_id=$1 AND user_id='viewer-a'"
                )
                .bind(&id)
                .fetch_one(&state.db)
                .await
                .unwrap(),
                "owner"
            );
            let group: String =
                sqlx::query_scalar("SELECT id FROM groups WHERE managed_space_id=$1")
                    .bind(&id)
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM group_global_roles WHERE group_id=$1"
                )
                .bind(&group)
                .fetch_one(&state.db)
                .await
                .unwrap(),
                0
            );
            assert_eq!(
                sqlx::query_scalar::<_, String>(
                    "SELECT role FROM space_group_permissions WHERE group_id=$1 AND space_id=$2"
                )
                .bind(&group)
                .bind(&id)
                .fetch_one(&state.db)
                .await
                .unwrap(),
                "owner"
            );
            tokio::fs::remove_dir_all(storage::space_dir(&slug))
                .await
                .unwrap();
        }
    }
    #[tokio::test]
    async fn space_creation_missing_fence_fails_before_space_or_group_insert() {
        let state = setup(false).await;
        sqlx::query("DELETE FROM audience_mutation_fence")
            .execute(&state.db)
            .await
            .unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(
            crate::auth::routes::create_personal_space(&state, "viewer-a", "Missing fence")
                .await
                .is_err()
        );
        assert_eq!(move_database_snapshot(&state).await, before);
        assert!(
            ensure_managed_space_group(&state, "space-a", "A", "owner-a")
                .await
                .is_err()
        );
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn space_creation_true_wal_waits_before_first_insert_and_rechecks_rest_identity() {
        let state = setup(true).await;
        let admin = admin(&state).await;
        let mut blocker = state.db.begin().await.unwrap();
        super::super::mutation_guards::lock_audience_for_mutation(&mut blocker)
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE user_id='owner-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let copy = state.clone();
        let id = Uuid::new_v4().to_string();
        let wanted = id.clone();
        let req: CreateSpaceRequest =
            serde_json::from_value(serde_json::json!({"source_id":id,"name":format!("Wait {id}")}))
                .unwrap();
        let task = tokio::spawn(async move {
            create_space(State(copy), admin, Json(req))
                .await
                .map(|_| ())
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!task.is_finished());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM spaces WHERE id=$1")
                .bind(&wanted)
                .fetch_one(&mut *blocker)
                .await
                .unwrap(),
            0
        );
        blocker.commit().await.unwrap();
        let before = move_database_snapshot(&state).await;
        assert!(task.await.unwrap().is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn space_creation_rest_last_constituent_credential_revocation_rolls_back() {
        let state = setup(false).await;
        let proof = admin(&state).await;
        sqlx::query("CREATE TRIGGER late_constructor AFTER INSERT ON space_group_permissions BEGIN DELETE FROM sessions WHERE user_id='owner-a'; END").execute(&state.db).await.unwrap();
        let before = move_database_snapshot(&state).await;
        let req: CreateSpaceRequest =
            serde_json::from_value(serde_json::json!({"name":format!("Late {}",Uuid::new_v4())}))
                .unwrap();
        assert!(matches!(
            create_space(State(state.clone()), proof, Json(req)).await,
            Err(AppError::Unauthorized(_))
        ));
        assert_eq!(move_database_snapshot(&state).await, before);
    }
    #[tokio::test]
    async fn space_creation_service_authority_never_fabricates_a_human_creator() {
        let state = setup(false).await;
        sqlx::query("INSERT INTO api_keys(id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('constructor-service','Constructor', $1,'test',1,'[\"admin\"]')").bind(crate::auth::api_keys::hash_api_key("constructor-service")).execute(&state.db).await.unwrap();
        let request = axum::http::Request::builder()
            .method("POST")
            .header("X-API-Key", "constructor-service")
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();
        let proof = AdminUser::from_request_parts(&mut parts, &state)
            .await
            .unwrap();
        assert_eq!(proof.claims.sub, "service:constructor-service");
        let before = move_database_snapshot(&state).await;
        let req: CreateSpaceRequest = serde_json::from_value(
            serde_json::json!({"name":format!("Service {}",Uuid::new_v4())}),
        )
        .unwrap();
        // The existing creator-user FK contract rejects this route; no fake
        // user row or partial ownership namespace is manufactured for a key.
        assert!(create_space(State(state.clone()), proof, Json(req))
            .await
            .is_err());
        assert_eq!(move_database_snapshot(&state).await, before);
    }
}

#[cfg(test)]
thread_local! {static MANAGED_GROUP_CREATION_PAUSE:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};}
#[cfg(test)]
async fn managed_group_creation_pause() {
    if let Some(hook) = MANAGED_GROUP_CREATION_PAUSE.with(|hook| hook.borrow_mut().take()) {
        hook.await;
    }
}
#[cfg(test)]
mod space_creation_broad_writer_order_test {
    use super::*;
    #[tokio::test]
    async fn space_creation_true_wal_broad_writer_waits_then_guards_committed_new_space() {
        let state = space_creation_fence_tests::setup(true).await;
        let proof = space_creation_fence_tests::admin(&state).await;
        sqlx::query("INSERT INTO groups(id,name) VALUES('creation-broad-group','Broad group')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("CREATE TABLE observed_new_space_guard(space_id TEXT)")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER observe_new_space_guard AFTER UPDATE OF id ON spaces WHEN NEW.created_by='viewer-a' BEGIN INSERT INTO observed_new_space_guard VALUES(NEW.id); END").execute(&state.db).await.unwrap();
        let entered = std::sync::Arc::new(tokio::sync::Notify::new());
        let release = std::sync::Arc::new(tokio::sync::Notify::new());
        let entered_hook = entered.clone();
        let release_hook = release.clone();
        let copy = state.clone();
        let display = format!("Ordered {}", Uuid::new_v4());
        let creator = tokio::spawn(async move {
            MANAGED_GROUP_CREATION_PAUSE.with(|slot| {
                *slot.borrow_mut() = Some(Box::pin(async move {
                    entered_hook.notify_one();
                    release_hook.notified().await;
                }))
            });
            crate::auth::routes::create_personal_space(&copy, "viewer-a", &display).await
        });
        entered.notified().await;
        let copy = state.clone();
        let broad = tokio::spawn(async move {
            mutate_group_global_role(&copy, &proof, "creation-broad-group", "viewer", true)
                .await
                .map(|_| ())
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            !broad.is_finished(),
            "broad writer must wait on creation's fence"
        );
        release.notify_one();
        creator.await.unwrap().unwrap();
        broad.await.unwrap().unwrap();
        let (id, slug): (String, String) =
            sqlx::query_as("SELECT id,slug FROM spaces WHERE created_by='viewer-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let guarded: Vec<String> =
            sqlx::query_scalar("SELECT space_id FROM observed_new_space_guard")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(guarded, vec![id]);
        tokio::fs::remove_dir_all(storage::space_dir(&slug))
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod admin_group_crud_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use axum::extract::FromRequestParts;
    use serde_json::Value;
    use tower::ServiceExt;
    async fn setup(wal: bool) -> AppState {
        let s = space_creation_fence_tests::setup(wal).await;
        for sql in [
            "INSERT INTO groups(id,name,description) VALUES('crud-group','CRUD group','Original'),('other-crud-group','Other CRUD group',NULL)",
            "INSERT INTO group_members(id,group_id,user_id) VALUES('crud-member','crud-group','viewer-a')",
            "INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('crud-custom','space-b','CRUD custom','[\"view\"]','owner-a')",
            "INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('crud-grant-a','space-a','crud-group','viewer'),('crud-grant-b','space-b','crud-group','crud-custom')",
            "INSERT INTO group_global_roles(group_id,role_id) VALUES('crud-group','contributor')",
            "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('crud-page','space-a','Page','space-a/docs/crud-page.md','owner-a','owner-a')",
            "INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('crud-page','space-a','restricted','policy-v1','owner-a')",
            "INSERT INTO document_access_allowlist(id,document_id,space_id,group_id,created_by) VALUES('crud-access','crud-page','space-a','crud-group','owner-a'),('other-crud-access','crud-page','space-a','other-crud-group','owner-a')",
            "INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('user-crud-access','crud-page','space-a','editor-a','owner-a')",
        ] {sqlx::query(sql).execute(&s.db).await.unwrap();}
        s
    }
    async fn proof(s: &AppState, key: Option<&str>, expires: Option<i64>) -> AdminUser {
        let mut req = axum::http::Request::builder().method("POST");
        if let Some(key) = key {
            req = req.header("X-API-Key", key);
        } else {
            let mut claims = catalog_tests::auth("owner-a").claims;
            if let Some(expires) = expires {
                claims.exp = expires;
            }
            let token = jsonwebtoken::encode(
                &jsonwebtoken::Header::default(),
                &claims,
                &jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes()),
            )
            .unwrap();
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        let (mut parts, _) = req.body(()).unwrap().into_parts();
        AdminUser::from_request_parts(&mut parts, s).await.unwrap()
    }
    async fn key(s: &AppState, id: &str, service: bool, scopes: &str) {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES($1,$2,$1,$3,'test',$4,$5)").bind(id).bind((!service).then_some("owner-a")).bind(crate::auth::api_keys::hash_api_key(id)).bind(i64::from(service)).bind(scopes).execute(&s.db).await.unwrap();
    }
    fn change(mode: usize) -> GroupCrudChange<'static> {
        static DESCRIPTION: Option<String> = None;
        match mode {
            0 => GroupCrudChange::Create {
                name: "New CRUD group",
                description: &DESCRIPTION,
            },
            1 => GroupCrudChange::Update {
                name: "Renamed CRUD group",
                description: &DESCRIPTION,
            },
            _ => GroupCrudChange::Delete,
        }
    }
    fn target(mode: usize) -> &'static str {
        if mode == 0 {
            "created-crud-group"
        } else {
            "crud-group"
        }
    }

    #[tokio::test]
    async fn maintenance_group_delete_fixed_page_source_allows_setnull_and_rejects_tamper() {
        for tamper in [false, true] {
            let s = setup(false).await;
            let admin = proof(&s, None, None).await;
            sqlx::query(
                "UPDATE documents SET maintenance_group_id='crud-group' WHERE id='crud-page'",
            )
            .execute(&s.db)
            .await
            .unwrap();
            if tamper {
                sqlx::query("CREATE TRIGGER maintenance_delete_drift AFTER DELETE ON groups WHEN OLD.id='crud-group' BEGIN UPDATE documents SET title='unexpected' WHERE id='crud-page'; END").execute(&s.db).await.unwrap();
            }
            let before = move_database_snapshot(&s).await;
            let stamp: String =
                sqlx::query_scalar("SELECT updated_at FROM documents WHERE id='crud-page'")
                    .fetch_one(&s.db)
                    .await
                    .unwrap();
            let result =
                mutate_admin_group(&s, &admin, "crud-group", GroupCrudChange::Delete).await;
            if tamper {
                assert!(result.is_err());
                assert_eq!(before, move_database_snapshot(&s).await);
            } else {
                result.unwrap();
                let row: (Option<String>, String) = sqlx::query_as(
                    "SELECT maintenance_group_id,updated_at FROM documents WHERE id='crud-page'",
                )
                .fetch_one(&s.db)
                .await
                .unwrap();
                assert_eq!(row, (None, stamp));
            }
        }
    }
    #[tokio::test]
    async fn admin_group_crud_wire_and_full_cascade_preserve_other_policy_and_definitions() {
        let s = setup(false).await;
        let p = proof(&s, None, None).await;
        let created = mutate_admin_group(&s, &p, target(0), change(0))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(created.name, "New CRUD group");
        assert_eq!(created.member_count, 0);
        assert_eq!(created.space_grant_count, 0);
        assert_eq!(created.created_at, created.updated_at);
        let deps = group_crud_dependencies(&mut *s.db.acquire().await.unwrap(), "crud-group")
            .await
            .unwrap();
        let before =
            full_group_crud_source(&mut *s.db.acquire().await.unwrap(), "crud-group", &deps)
                .await
                .unwrap();
        mutate_admin_group(&s, &p, target(1), change(1))
            .await
            .unwrap();
        let updated =
            full_group_crud_source(&mut *s.db.acquire().await.unwrap(), "crud-group", &deps)
                .await
                .unwrap();
        assert_eq!(updated.members, before.members);
        assert_eq!(updated.roles, before.roles);
        assert_eq!(updated.grants, before.grants);
        assert_eq!(updated.allowlist, before.allowlist);
        mutate_admin_group(&s, &p, target(2), change(2))
            .await
            .unwrap();
        let deleted =
            full_group_crud_source(&mut *s.db.acquire().await.unwrap(), "crud-group", &deps)
                .await
                .unwrap();
        assert!(deleted.group.is_none());
        assert!(deleted.members.is_empty());
        assert!(deleted.roles.is_empty());
        assert!(deleted.grants.is_empty());
        assert_eq!(deleted.global, before.global);
        assert_eq!(deleted.local, before.local);
        assert_eq!(deleted.policies, before.policies);
        assert_eq!(deleted.pages, before.pages);
        assert_eq!(
            deleted
                .allowlist
                .iter()
                .map(|r| r.0.as_str())
                .collect::<Vec<_>>(),
            vec!["other-crud-access", "user-crud-access"]
        );
        let audits:Vec<(String,String,String,Option<String>)>=sqlx::query_as("SELECT action,actor_kind,actor_id,user_id FROM audit_log WHERE action IN ('group.create','group.update','group.delete') ORDER BY action").fetch_all(&s.db).await.unwrap();
        assert_eq!(audits.len(), 3);
        assert!(audits
            .iter()
            .all(|r| r.1 == "user" && r.2 == "owner-a" && r.3.as_deref() == Some("owner-a")));
    }
    #[tokio::test]
    async fn admin_group_crud_scim_dedicated_missing_and_name_contracts_remain() {
        assert!(matches!(
            validate_admin_group_name("  "),
            Err(AppError::BadRequest(_))
        ));
        assert!(validate_admin_group_name(&"x".repeat(101)).is_err());
        assert_eq!(validate_admin_group_name("  Name  ").unwrap(), "Name");
        for (field, value) in [("scim_managed", "1"), ("managed_space_id", "'space-a'")] {
            let s = setup(false).await;
            let p = proof(&s, None, None).await;
            sqlx::query(&format!(
                "UPDATE groups SET {field}={value} WHERE id='crud-group'"
            ))
            .execute(&s.db)
            .await
            .unwrap();
            let before = move_database_snapshot(&s).await;
            for mode in [1, 2] {
                assert!(matches!(
                    mutate_admin_group(&s, &p, target(mode), change(mode)).await,
                    Err(AppError::Conflict(_))
                ));
                assert_eq!(move_database_snapshot(&s).await, before);
            }
        }
        let s = setup(false).await;
        let p = proof(&s, None, None).await;
        let before = move_database_snapshot(&s).await;
        for mode in [1, 2] {
            assert!(matches!(
                mutate_admin_group(&s, &p, "missing", change(mode)).await,
                Err(AppError::NotFound(_))
            ));
            assert_eq!(move_database_snapshot(&s).await, before);
        }
        assert!(mutate_admin_group(&s, &p, "crud-group", change(0))
            .await
            .is_err());
        assert_eq!(move_database_snapshot(&s).await, before);
    }
    #[tokio::test]
    async fn admin_group_crud_actual_http_human_service_and_method_scopes() {
        for credential in ["human-admin", "service-admin", "read", "write"] {
            let s = setup(false).await;
            key(
                &s,
                credential,
                credential == "service-admin",
                match credential {
                    "read" => "[\"read\"]",
                    "write" => "[\"write\"]",
                    _ => "[\"admin\"]",
                },
            )
            .await;
            let response=router().with_state(s.clone()).oneshot(axum::http::Request::builder().method("POST").uri("/admin/groups").header("X-API-Key",credential).header("Content-Type","application/json").body(axum::body::Body::from("{\"name\":\" Created HTTP group \",\"description\":\"HTTP description\"}")).unwrap()).await.unwrap();
            if credential == "read" || credential == "write" {
                assert_eq!(response.status(), StatusCode::FORBIDDEN);
                continue;
            }
            assert_eq!(response.status(), StatusCode::CREATED);
            let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            let id = body["id"].as_str().unwrap();
            assert_eq!(body["name"], "Created HTTP group");
            assert!(Uuid::parse_str(id).is_ok());
            for method in ["PUT", "DELETE"] {
                let req = axum::http::Request::builder()
                    .method(method)
                    .uri(format!("/admin/groups/{id}"))
                    .header("X-API-Key", credential)
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(if method == "PUT" {
                        "{\"name\":\" Renamed HTTP group \"}"
                    } else {
                        ""
                    }))
                    .unwrap();
                assert_eq!(
                    router()
                        .with_state(s.clone())
                        .oneshot(req)
                        .await
                        .unwrap()
                        .status(),
                    StatusCode::NO_CONTENT
                );
            }
            let rows:Vec<(String,String,Option<String>)>=sqlx::query_as("SELECT actor_kind,actor_id,user_id FROM audit_log WHERE action IN ('group.create','group.update','group.delete')").fetch_all(&s.db).await.unwrap();
            assert_eq!(rows.len(), 3);
            for (kind, actor, user) in rows {
                if credential == "service-admin" {
                    assert_eq!(
                        (kind, actor, user),
                        ("service_key".into(), credential.into(), None)
                    );
                } else {
                    assert_eq!(
                        (kind, actor, user),
                        ("user".into(), "owner-a".into(), Some("owner-a".into()))
                    );
                }
            }
        }
    }
    #[tokio::test]
    async fn admin_group_crud_late_credential_and_source_trigger_full_rollback() {
        for mode in 0..3 {
            for sql in ["DELETE FROM sessions WHERE user_id='owner-a';","UPDATE users SET role='member' WHERE id='owner-a';","UPDATE global_roles SET permissions='[]' WHERE id='contributor';","UPDATE document_access_policies SET revision='late' WHERE document_id='crud-page';","INSERT INTO group_members(id,group_id,user_id) VALUES('late-member','crud-group','editor-a');"] {
                // New unassociated groups have no pre-existing definition/policy dependency.
                if mode==0 && (sql.contains("global_roles") || sql.contains("document_access_policies") || sql.contains("crud-group")) {continue;}
                let s=setup(false).await;let p=proof(&s,None,None).await;
                sqlx::query(&format!("CREATE TRIGGER late_crud AFTER {} ON groups BEGIN {sql} END",match mode {0=>"INSERT",1=>"UPDATE OF name",_=>"DELETE"})).execute(&s.db).await.unwrap();
                let before=move_database_snapshot(&s).await;assert!(mutate_admin_group(&s,&p,target(mode),change(mode)).await.is_err(),"{mode}: {sql}");assert_eq!(move_database_snapshot(&s).await,before,"{mode}: {sql}");
            }
        }
    }
    #[tokio::test]
    async fn admin_group_crud_audit_failure_tamper_and_source_wait_rollback() {
        for mode in 0..3 {
            for sql in [
                "SELECT RAISE(ABORT,'audit failure');",
                "UPDATE audit_log SET after_value='tamper' WHERE id=NEW.id;",
                "DELETE FROM sessions WHERE user_id='owner-a';",
                "UPDATE space_group_permissions SET role='owner' WHERE id='crud-grant-a';",
                "UPDATE global_roles SET name='Audit-mutated role' WHERE id='contributor';",
                "UPDATE document_access_policies SET revision='audit-mutated-policy' WHERE document_id='crud-page';",
            ] {
                if (mode == 0 && (sql.contains("space_group_permissions") || sql.contains("global_roles") || sql.contains("document_access_policies"))) || (mode == 2 && sql.contains("space_group_permissions")) {
                    continue;
                }
                let s = setup(false).await;
                let p = proof(&s, None, None).await;
                sqlx::query(&format!(
                    "CREATE TRIGGER late_crud_audit AFTER INSERT ON audit_log BEGIN {sql} END"
                ))
                .execute(&s.db)
                .await
                .unwrap();
                let before = move_database_snapshot(&s).await;
                assert!(
                    mutate_admin_group(&s, &p, target(mode), change(mode))
                        .await
                        .is_err(),
                    "{mode}: {sql}"
                );
                assert_eq!(move_database_snapshot(&s).await, before);
            }
        }
    }
    #[tokio::test]
    async fn admin_group_crud_real_wal_fence_wait_rechecks_session_and_service_key() {
        for mode in 0..3 {
            for service in [false, true] {
                let s = setup(true).await;
                if service {
                    key(&s, "crud-wait-service", true, "[\"admin\"]").await;
                }
                let p = proof(&s, service.then_some("crud-wait-service"), None).await;
                let mut blocker = s.db.begin().await.unwrap();
                super::super::mutation_guards::lock_audience_for_mutation(&mut blocker)
                    .await
                    .unwrap();
                sqlx::query(if service {
                    "UPDATE api_keys SET scopes='[\"read\"]' WHERE id='crud-wait-service'"
                } else {
                    "DELETE FROM sessions WHERE user_id='owner-a'"
                })
                .execute(&mut *blocker)
                .await
                .unwrap();
                let c = s.clone();
                let task = tokio::spawn(async move {
                    mutate_admin_group(&c, &p, target(mode), change(mode))
                        .await
                        .map(|_| ())
                });
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                assert!(!task.is_finished());
                blocker.commit().await.unwrap();
                let before = move_database_snapshot(&s).await;
                assert!(task.await.unwrap().is_err());
                assert_eq!(move_database_snapshot(&s).await, before);
            }
        }
    }
    #[tokio::test]
    async fn admin_group_crud_final_read_proofs_require_unexpired_original_jwt() {
        for mode in 0..3 {
            let s = setup(false).await;
            let expires = chrono::Utc::now().timestamp() + 2;
            let p = proof(&s, None, Some(expires)).await;
            let before = move_database_snapshot(&s).await;
            AUDIENCE_FINAL_EXPIRY.with(|h| h.set(Some(expires)));
            let result = mutate_admin_group(&s, &p, target(mode), change(mode)).await;
            assert!(AUDIENCE_FINAL_EXPIRY.with(|h| h.get()).is_none());
            assert!(matches!(result, Err(AppError::Unauthorized(_))));
            assert_eq!(move_database_snapshot(&s).await, before);
        }
    }
    #[tokio::test]
    async fn admin_group_crud_orphan_and_wrong_scope_bindings_are_retained_on_denial() {
        for sql in [
            "UPDATE document_access_allowlist SET space_id='space-b' WHERE id='crud-access'",
            "DELETE FROM users WHERE id='viewer-a'",
            "DELETE FROM global_roles WHERE id='contributor'",
        ] {
            let s = setup(false).await;
            let p = proof(&s, None, None).await;
            sqlx::query("PRAGMA foreign_keys=OFF")
                .execute(&s.db)
                .await
                .unwrap();
            sqlx::query(sql).execute(&s.db).await.unwrap();
            sqlx::query("PRAGMA foreign_keys=ON")
                .execute(&s.db)
                .await
                .unwrap();
            let before = move_database_snapshot(&s).await;
            assert!(
                mutate_admin_group(&s, &p, "crud-group", change(2))
                    .await
                    .is_err(),
                "{sql}"
            );
            assert_eq!(move_database_snapshot(&s).await, before);
        }
    }
}

#[cfg(test)]
pub(crate) mod space_update_current_tests {
    use super::*;
    use axum::extract::FromRequestParts;
    pub(crate) async fn state(wal: bool) -> AppState { space_creation_fence_tests::setup(wal).await }
    pub(crate) async fn auth(state: &AppState) -> AuthUser {
        let token = jsonwebtoken::encode(&jsonwebtoken::Header::default(), &catalog_tests::auth("owner-a").claims, &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();
        let (mut parts, _) = axum::http::Request::builder().method("PUT").header("Authorization", format!("Bearer {token}")).body(()).unwrap().into_parts();
        AuthUser::from_request_parts(&mut parts, state).await.unwrap()
    }
    fn request(visibility: &str) -> UpdateSpaceRequest {
        UpdateSpaceRequest { name: Some("Changed name".into()), description: Some("Changed description".into()), icon: Some("Changed icon".into()), accent_color: Some("blue".into()), visibility: Some(visibility.into()) }
    }
    #[tokio::test]
    async fn space_update_invalid_final_field_and_late_source_changes_preserve_all_fields() {
        let state = state(false).await;
        let auth = auth(&state).await;
        let before = crate::spaces::documents::tests::move_database_snapshot(&state).await;
        assert!(update_space(State(state.clone()), auth, Path("space-a".into()), Json(request("invalid"))).await.is_err());
        assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&state).await, before);
        for action in ["DELETE FROM sessions WHERE user_id='owner-a';", "UPDATE spaces SET slug='mutated-source' WHERE id='space-a';"] {
            let state = self::state(false).await;
            let auth = self::auth(&state).await;
            sqlx::query(&format!("CREATE TRIGGER late_space_update AFTER UPDATE OF name ON spaces BEGIN {action} END")).execute(&state.db).await.unwrap();
            let before = crate::spaces::documents::tests::move_database_snapshot(&state).await;
            assert!(update_space_on_current_authority(&state, &auth, "space-a", &request("private"), None).await.is_err());
            assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&state).await, before);
        }
    }
    #[tokio::test]
    async fn space_update_actual_keys_scope_and_exact_import_fields() {
        for scope in ["read", "write", "admin"] {
            let state = state(false).await;
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'owner-a',$1,$2,'test',$3)").bind(scope).bind(crate::auth::api_keys::hash_api_key(scope)).bind(format!("[\"{scope}\"]")).execute(&state.db).await.unwrap();
            let (mut parts, _) = axum::http::Request::builder().method(if scope == "read" { "GET" } else { "PUT" }).header("X-API-Key",scope).body(()).unwrap().into_parts();
            let auth = AuthUser::from_request_parts(&mut parts,&state).await.unwrap();
            let before = space_update_source(&mut *state.db.acquire().await.unwrap(),"space-a").await.unwrap();
            let result = update_space_on_current_authority(&state,&auth,"space-a",&request("public"),Some("2000-01-01T00:00:00Z")).await;
            let after = space_update_source(&mut *state.db.acquire().await.unwrap(),"space-a").await.unwrap();
            if scope=="read" { assert!(result.is_err()); assert_eq!(after,before); }
            else { result.unwrap(); assert_eq!(after.1,"Changed name"); assert_eq!(after.6,"public"); assert_eq!(after.9,"2000-01-01T00:00:00Z"); assert_eq!((after.0,after.2,after.7,after.8,after.10),(before.0,before.2,before.7,before.8,before.10)); }
        }
    }
    #[tokio::test]
    async fn space_update_real_wal_wait_rechecks_actual_session_before_write() {
        let state = state(true).await;
        let auth = auth(&state).await;
        let mut blocker = state.db.begin().await.unwrap();
        super::super::mutation_guards::lock_audience_for_mutation(&mut blocker).await.unwrap();
        sqlx::query("DELETE FROM sessions WHERE user_id='owner-a'").execute(&mut *blocker).await.unwrap();
        let other = state.clone();
        let task=tokio::spawn(async move {update_space_on_current_authority(&other,&auth,"space-a",&request("private"),None).await});
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!task.is_finished());
        blocker.commit().await.unwrap();
        let before = crate::spaces::documents::tests::move_database_snapshot(&state).await;
        assert!(task.await.unwrap().is_err());
        assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&state).await,before);
    }
}

#[cfg(test)]
mod retention_current_tests {
    use super::*;
    use axum::extract::FromRequestParts;
    #[tokio::test]
    async fn retention_current_bounds_source_and_late_identity_rollback() {
        for action in [None, Some("UPDATE spaces SET slug='wrong' WHERE id='space-a';"), Some("DELETE FROM sessions WHERE user_id='owner-a';")] {
            let state=space_update_current_tests::state(false).await;
            let before=space_update_source(&mut *state.db.acquire().await.unwrap(),"space-a").await.unwrap();
            if let Some(action)=action { sqlx::query(&format!("CREATE TRIGGER late_retention AFTER UPDATE OF trash_retention_days ON spaces BEGIN {action} END")).execute(&state.db).await.unwrap(); }
            let result=update_space_retention(State(state.clone()),space_update_current_tests::auth(&state).await,Path("space-a".into()),Json(UpdateSpaceRetentionRequest{trash_retention_days:60})).await;
            let after=space_update_source(&mut *state.db.acquire().await.unwrap(),"space-a").await.unwrap();
            if action.is_some() {assert!(result.is_err());assert_eq!(after,before);} else {result.unwrap();let mut expected=before;expected.9=after.9.clone();expected.10=60;assert_eq!(after,expected);get_space_retention(State(state.clone()),space_update_current_tests::auth(&state).await,Path("space-a".into())).await.unwrap();}
        }
        for days in [29,121] {
            let state=space_update_current_tests::state(false).await;
            let before=space_update_source(&mut *state.db.acquire().await.unwrap(),"space-a").await.unwrap();
            assert!(update_space_retention(State(state.clone()),space_update_current_tests::auth(&state).await,Path("space-a".into()),Json(UpdateSpaceRetentionRequest{trash_retention_days:days})).await.is_err());
            assert_eq!(space_update_source(&mut *state.db.acquire().await.unwrap(),"space-a").await.unwrap(),before);
        }
    }
    #[tokio::test]
    async fn retention_current_actual_key_read_write_ceiling_and_revocation() {
        for scope in ["read","write","admin"] {
            let state=space_update_current_tests::state(false).await;
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'owner-a',$1,$2,'test',$3)").bind(scope).bind(crate::auth::api_keys::hash_api_key(scope)).bind(format!("[\"{scope}\"]")).execute(&state.db).await.unwrap();
            for read in [true,false] {
                let (mut parts,_)=axum::http::Request::builder().method(if scope=="write" {"PUT"} else {"GET"}).header("X-API-Key",scope).body(()).unwrap().into_parts();
                let auth=AuthUser::from_request_parts(&mut parts,&state).await.unwrap();
                let result=if read {get_space_retention(State(state.clone()),auth,Path("space-a".into())).await.map(|_|())} else {update_space_retention(State(state.clone()),auth,Path("space-a".into()),Json(UpdateSpaceRetentionRequest{trash_retention_days:45})).await.map(|_|())};
                assert_eq!(result.is_ok(),if read {scope!="write"} else {scope!="read"});
            }
            let (mut parts,_)=axum::http::Request::builder().method(if scope=="write" {"PUT"} else {"GET"}).header("X-API-Key",scope).body(()).unwrap().into_parts();
            let auth=AuthUser::from_request_parts(&mut parts,&state).await.unwrap();
            sqlx::query("DELETE FROM api_keys WHERE id=$1").bind(scope).execute(&state.db).await.unwrap();
            assert!(get_space_retention(State(state.clone()),auth,Path("space-a".into())).await.is_err());
        }
    }
    #[tokio::test]
    async fn retention_waiting_writer_rechecks_real_wal_session() {
        let state=space_update_current_tests::state(true).await;
        let auth=space_update_current_tests::auth(&state).await;
        let mut blocker=state.db.begin().await.unwrap();
        super::super::mutation_guards::lock_audience_for_mutation(&mut blocker).await.unwrap();
        sqlx::query("DELETE FROM sessions WHERE user_id='owner-a'").execute(&mut *blocker).await.unwrap();
        let other=state.clone();let task=tokio::spawn(async move {update_space_retention(State(other),auth,Path("space-a".into()),Json(UpdateSpaceRetentionRequest{trash_retention_days:60})).await.map(|_|())});
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;assert!(!task.is_finished());
        blocker.commit().await.unwrap();let before=space_update_source(&mut *state.db.acquire().await.unwrap(),"space-a").await.unwrap();
        assert!(task.await.unwrap().is_err());assert_eq!(space_update_source(&mut *state.db.acquire().await.unwrap(),"space-a").await.unwrap(),before);
    }
}

#[cfg(test)]
mod retention_read_wal_tests {
    use super::*;
    #[tokio::test]
    async fn retention_read_afterload_real_wal_revocation_is_current() {
        for action in ["DELETE FROM sessions WHERE user_id='owner-a'", "UPDATE users SET is_active=0 WHERE id='owner-a'", "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','owner-a')"] {
            let state=space_update_current_tests::state(true).await;
            let auth=space_update_current_tests::auth(&state).await;
            let other=state.clone();let action=action.to_owned();
            CATALOG_AFTER_LOAD.with(|cell| *cell.borrow_mut()=Some(Box::pin(async move {sqlx::query(&action).execute(&other.db).await.unwrap();})));
            assert!(get_space_retention(State(state.clone()),auth,Path("space-a".into())).await.is_err());
        }
    }
}

#[cfg(test)]
mod audience_catalog_current_tests {
    use super::*;
    use axum::extract::FromRequestParts;
    async fn fixture(wal:bool)->AppState {
        let state=space_update_current_tests::state(wal).await;
        sqlx::query("INSERT INTO groups(id,name) VALUES('read-group','Readable group')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('read-member','read-group','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('read-grant','space-a','read-group','viewer')").execute(&state.db).await.unwrap();state
    }
    async fn invoke(state:AppState,auth:AuthUser,kind:usize)->Result<axum::response::Response,AppError>{
        match kind {
            0=>list_space_roles(State(state),auth,Path("space-a".into())).await.map(IntoResponse::into_response),
            1=>list_members(State(state),auth,Path("space-a".into())).await.map(IntoResponse::into_response),
            2=>list_effective_space_users(State(state),auth,Path("space-a".into())).await.map(IntoResponse::into_response),
            3=>list_space_principals(State(state),auth,Path("space-a".into())).await.map(IntoResponse::into_response),
            _=>list_space_group_members(State(state),auth,Path(("space-a".into(),"read-group".into()))).await.map(IntoResponse::into_response),
        }
    }
    #[tokio::test]
    async fn audience_catalog_actual_keys_method_ceiling_and_no_read_writes() {
        for kind in 0..5 {for scope in ["read","write","admin"] {
            let state=fixture(false).await;
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'owner-a',$1,$2,'test',$3)").bind(scope).bind(crate::auth::api_keys::hash_api_key(scope)).bind(format!("[\"{scope}\"]")).execute(&state.db).await.unwrap();
            let (mut parts,_)=axum::http::Request::builder().method(if scope=="write"{"PUT"}else{"GET"}).header("X-API-Key",scope).body(()).unwrap().into_parts();
            let auth=AuthUser::from_request_parts(&mut parts,&state).await.unwrap();
            let before=crate::spaces::documents::tests::move_database_snapshot(&state).await;
            let result=invoke(state.clone(),auth,kind).await;
            assert_eq!(result.is_ok(),scope!="write","kind={kind},scope={scope}");
            if let Ok(response)=result {assert_eq!(response.status(),StatusCode::OK);}
            assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&state).await,before);
        }}
    }
    #[tokio::test]
    async fn audience_catalog_afterload_real_wal_revocations_are_current() {
        for kind in 0..5 {for action in ["DELETE FROM sessions WHERE user_id='owner-a'","UPDATE users SET is_active=0 WHERE id='owner-a'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','owner-a')"] {
            let state=fixture(true).await;let auth=space_update_current_tests::auth(&state).await;
            let other=state.clone();let action=action.to_owned();
            CATALOG_AFTER_LOAD.with(|cell|*cell.borrow_mut()=Some(Box::pin(async move {sqlx::query(&action).execute(&other.db).await.unwrap();})));
            assert!(invoke(state,auth,kind).await.is_err(),"kind={kind}");
        }}
    }
    #[tokio::test]
    async fn audience_catalog_management_union_and_view_only_members_split() {
        for management in [false,true] {for kind in 0..5 {
            let state=fixture(false).await;
            sqlx::query("UPDATE users SET role='member' WHERE id='owner-a'").execute(&state.db).await.unwrap();
            sqlx::query("DELETE FROM group_members WHERE user_id='owner-a'").execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('catalog-custom','space-a','Catalog', $1,'owner-a')").bind(if management{"[\"view\",\"manage_space\"]"}else{"[\"view\"]"}).execute(&state.db).await.unwrap();
            sqlx::query("UPDATE space_members SET role='catalog-custom' WHERE space_id='space-a' AND user_id='owner-a'").execute(&state.db).await.unwrap();
            let auth=space_update_current_tests::auth(&state).await;
            assert_eq!(invoke(state,auth,kind).await.is_ok(),management||kind==1,"kind={kind},management={management}");
        }}
    }
    #[tokio::test]
    async fn audience_catalog_group_grant_late_removal_conceals_member_directory() {
        let state=fixture(true).await;let auth=space_update_current_tests::auth(&state).await;
        let other=state.clone();CATALOG_AFTER_LOAD.with(|cell|*cell.borrow_mut()=Some(Box::pin(async move {sqlx::query("DELETE FROM space_group_permissions WHERE id='read-grant'").execute(&other.db).await.unwrap();})));
        assert!(invoke(state,auth,4).await.is_err());
    }
}
