//! Typed access resolver. Page reads, duplication, workflow, comments and page notifications use this module; broader adoption
//! remains in progress. Callers must
//! re-resolve after blocking locks and before their final mutation/commit.
//! Restriction allowlists intersect page/ancestor access. A dormant named direct
//! viewer read foundation exists; production route activation and guests remain held.
use crate::auth::routes::AppError;
pub use super::page_viewer_grants::DirectViewerGrant;
use serde::{Deserialize, Serialize};
use sqlx::AnyConnection;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    View,
    Comment,
    CreateContent,
    EditContent,
    MoveContent,
    DuplicateContent,
    ManageFolders,
    ManageTemplates,
    ArchiveContent,
    DeleteContent,
    ManageMembers,
    ManageSpace,
}
const ALL: [Permission; 12] = [
    Permission::View,
    Permission::Comment,
    Permission::CreateContent,
    Permission::EditContent,
    Permission::MoveContent,
    Permission::DuplicateContent,
    Permission::ManageFolders,
    Permission::ManageTemplates,
    Permission::ArchiveContent,
    Permission::DeleteContent,
    Permission::ManageMembers,
    Permission::ManageSpace,
];

/// Session-less users are already authenticated background actors, not anonymous
/// callers. HTTP bearer adapters MUST supply the actual session ID and JWT expiry;
/// session expiry alone cannot keep an expired JWT authorized during a wait.
/// Service keys have a distinct current-key capability, but remain denied by
/// page/space resolution until explicit operation/attribution adapters exist.
#[derive(Clone, Copy, Debug)]
pub enum Principal<'a> {
    User {
        user_id: &'a str,
        session_id: Option<&'a str>,
        token_expires_at: Option<i64>,
    },
    UserKey {
        key_id: &'a str,
        user_id: &'a str,
        token_expires_at: Option<i64>,
    },
    ServiceKey {
        key_id: &'a str,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ApiKeyScope {
    Read,
    Write,
    Admin,
}
#[derive(Clone, Debug)]
pub struct UserKeyCapability {
    pub key_id: String,
    pub user_id: String,
    pub scopes: BTreeSet<ApiKeyScope>,
}
/// Current service-key proof only. It confers no user identity, instance-admin
/// status, private participation, page grants, or storage/audit attribution.
/// A future explicit operation adapter must consume this distinct capability.
#[derive(Clone, Debug)]
pub struct ServiceKeyCapability {
    pub key_id: String,
    pub scopes: BTreeSet<ApiKeyScope>,
}
#[derive(Clone, Debug)]
pub enum CurrentKeyCapability {
    User(UserKeyCapability),
    Service(ServiceKeyCapability),
}
fn key_unavailable() -> AppError {
    AppError::Unauthorized("API key unavailable".into())
}
/// AuthUser is produced by the real signature/key verifier. This adapter carries
/// identity evidence, not its possibly stale role/scope labels. Resolution still
/// checks current session/key/account records on the caller's connection.
pub fn principal_from_auth(
    auth: &crate::auth::middleware::AuthUser,
) -> Result<Principal<'_>, AppError> {
    if auth.claims.exp <= chrono::Utc::now().timestamp()
        || !matches!(auth.claims.token_type, crate::auth::jwt::TokenType::Access)
    {
        return Err(AppError::Unauthorized("Token unavailable".into()));
    }
    if auth.api_key_scopes.is_none() {
        if auth.claims.sid.is_empty() || auth.claims.sub.starts_with("service:") {
            return Err(key_unavailable());
        }
        return Ok(Principal::User {
            user_id: &auth.claims.sub,
            session_id: Some(&auth.claims.sid),
            token_expires_at: Some(auth.claims.exp),
        });
    }
    if let Some(key_id) = auth
        .claims
        .sid
        .strip_prefix("service:")
        .filter(|id| !id.is_empty())
    {
        if auth.claims.sub != format!("service:{key_id}") {
            return Err(key_unavailable());
        }
        return Ok(Principal::ServiceKey { key_id });
    }
    let key_id = auth
        .claims
        .sid
        .strip_prefix("api-key:")
        .filter(|id| !id.is_empty())
        .ok_or_else(key_unavailable)?;
    if auth.claims.sub.starts_with("service:") {
        return Err(key_unavailable());
    }
    Ok(Principal::UserKey {
        key_id,
        user_id: &auth.claims.sub,
        token_expires_at: Some(auth.claims.exp),
    })
}
/// Does not authenticate an arbitrary key ID: only callers with already verified
/// AuthUser key evidence may invoke this revalidation helper. No secrets are read.
pub async fn resolve_current_key(
    conn: &mut AnyConnection,
    key_id: &str,
) -> Result<CurrentKeyCapability, AppError> {
    let row: Option<(Option<String>, i64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT user_id,is_service_account,expires_at,scopes FROM api_keys WHERE id=$1",
    )
    .bind(key_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let (owner, service, expiry, raw) = row.ok_or_else(key_unavailable)?;
    if let Some(expiry) = expiry {
        if !chrono::DateTime::parse_from_rfc3339(&expiry)
            .ok()
            .is_some_and(|time| time > chrono::Utc::now())
        {
            return Err(key_unavailable());
        }
    }
    if service != 0 && raw.is_none() {
        return Err(key_unavailable());
    }
    let scopes: BTreeSet<_> = crate::auth::api_keys::parse_scope_json(raw.as_deref())
        .into_iter()
        .map(|scope| match scope.as_str() {
            "read" => Ok(ApiKeyScope::Read),
            "write" => Ok(ApiKeyScope::Write),
            "admin" => Ok(ApiKeyScope::Admin),
            _ => Err(key_unavailable()),
        })
        .collect::<Result<_, _>>()?;
    if scopes.is_empty() {
        return Err(key_unavailable());
    }
    if service != 0 {
        if !scopes.contains(&ApiKeyScope::Admin) {
            return Err(key_unavailable());
        }
        return Ok(CurrentKeyCapability::Service(ServiceKeyCapability {
            key_id: key_id.into(),
            scopes,
        }));
    }
    let user_id = owner.ok_or_else(key_unavailable)?;
    let active: Option<i64> = sqlx::query_scalar("SELECT 1 FROM users WHERE id=$1 AND is_active=1")
        .bind(&user_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if active.is_none() {
        return Err(key_unavailable());
    }
    Ok(CurrentKeyCapability::User(UserKeyCapability {
        key_id: key_id.into(),
        user_id,
        scopes,
    }))
}
fn key_allows(scopes: &BTreeSet<ApiKeyScope>, permission: Permission) -> bool {
    scopes.contains(&ApiKeyScope::Admin)
        || scopes.contains(&if permission == Permission::View {
            ApiKeyScope::Read
        } else {
            ApiKeyScope::Write
        })
}

#[derive(Clone, Copy, Debug)]
pub enum Purpose<'a> {
    Routine,
    AdminRecovery { reason: &'a str },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoleSource {
    Direct,
    SpaceGroup { group_id: String },
    GlobalGroup { group_id: String },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssignedRole {
    pub id: String,
    pub source: RoleSource,
}
/// This is a required audit intent, NOT an audit receipt. An adapter MUST persist
/// an auditable recovery record before exposing data or taking recovery actions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryAuditRequired {
    pub actor_id: String,
    pub space_id: String,
    pub reason: String,
}
#[derive(Clone, Debug)]
pub struct SpaceAccess {
    pub actor_id: String,
    pub space_id: String,
    pub instance_admin: bool,
    pub roles: Vec<AssignedRole>,
    pub permissions: BTreeSet<Permission>,
    pub recovery_audit_required: Option<RecoveryAuditRequired>,
}
impl SpaceAccess {
    pub fn allows(&self, permission: Permission) -> bool {
        self.permissions.contains(&permission)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageOperation {
    /// Relation target: account View with a mutation-method key ceiling.
    ReferenceFromMutation,
    /// Owner draft metadata only: account View, explicit lifecycle inspection.
    DraftOwnerRead,
    /// Same owner metadata with the original Write/Admin method ceiling.
    DraftOwnerMutation,
    /// Read-only move inventory requested through a write-method credential.
    InspectMovePreviewFromMutation,
    /// Own notification acknowledgment: account View, write-method key ceiling.
    AcknowledgeNotification,
    /// Own page preference: account View with a write-method key ceiling.
    UserPagePreference,
    View,
    /// Dormant page-only published read foundation. No production adapter selects
    /// this operation until management/discovery/read witnesses adopt grants.
    /// Unlike View this may admit a direct grant, never a draft or lifecycle read.
    GrantedPublishedView,
    /// Metadata inspection only: archived chains remain subject to normal View
    /// and private participation. This does not authorize any body read.
    InspectMetadata,
    /// Archived listing metadata only; never body or mutation authority.
    InspectArchivedMetadata,
    /// Trash listing metadata only; never body or mutation authority.
    InspectTrashedMetadata,
    /// Canonical deletion only: exact target lifecycle, private editor proof,
    /// and Write/DeleteContent ceiling. Existing body locks do not bar deletion.
    DeleteActive,
    DeleteArchived,
    DeleteTrashed,
    /// Soft trash of active or archived source; current DeleteContent/private editor.
    /// Body locks do not prohibit this lifecycle metadata transition.
    Trash,
    /// Archive metadata transition: active or already archived nondeleted target,
    /// actual ArchiveContent/private editor; body locks do not bar it.
    Archive,
    /// Unarchive requires an archived source and proves the actual active outcome.
    UnarchiveArchived,
    UnarchiveActive,
    /// Trash restoration uses existing DeleteContent and preserves archived_at.
    RestoreTrashed,
    RestoreNotDeleted,
    /// Restriction management is account ManageSpace with private editor participation.
    RestrictionPreview,
    RestrictionApply,
    Comment,
    Edit,
    /// Link management changes no body and deliberately ignores content locks.
    PublicLinkEnable,
    /// Revocation also permits archived target/ancestor chains; never deleted.
    PublicLinkDisable,
    /// Same-space page hierarchy mutation; exact MoveContent capability.
    Move,
    /// Owner-equivalent lock management; existing locks do not bar unlocking.
    ManageLock,
}
#[cfg(test)]
tokio::task_local! { static DIRECT_VIEWER_FINAL_GATE: (std::sync::Arc<tokio::sync::Notify>, std::sync::Arc<tokio::sync::Notify>); }
#[derive(Clone, Debug)]
pub struct PageAccess {
    pub space: SpaceAccess,
    pub document_id: String,
    pub ancestor_ids: Vec<String>,
    pub published: bool,
    pub locked_by: Option<String>,
    /// Separate page capability: never union a grant into SpaceAccess.
    pub page_permissions: BTreeSet<Permission>,
    pub direct_viewer_grant: Option<DirectViewerGrant>,
}
impl PageAccess {
    pub fn allows_page(&self, permission: Permission) -> bool {
        self.page_permissions.contains(&permission)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DraftOperation {
    View,
    Edit,
    InspectArchivedMetadata,
    InspectTrashedMetadata,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DraftParticipation {
    Owner,
    SharedViewer,
    SharedEditor,
    RecoveryAuditRequired,
}
#[derive(Clone, Debug)]
pub struct DraftAccess {
    pub page: PageAccess,
    pub draft_id: String,
    pub participation: DraftParticipation,
}
fn unavailable() -> AppError {
    AppError::NotFound("Target unavailable".into())
}
fn invalid_policy() -> AppError {
    AppError::Conflict("Access policy is invalid".into())
}
fn parse_permissions(raw: &str) -> Result<BTreeSet<Permission>, AppError> {
    serde_json::from_str::<Vec<Permission>>(raw)
        .map(|v| v.into_iter().collect())
        .map_err(|_| invalid_policy())
}
fn built_in(role: &str) -> Option<BTreeSet<Permission>> {
    let tokens: &[Permission] = match role {
        "owner" | "space_admin" | "admin" => &ALL,
        "editor" | "contributor" => &ALL[..10],
        "commenter" => &ALL[..2],
        "viewer" => &ALL[..1],
        _ => return None,
    };
    Some(tokens.iter().copied().collect())
}
pub(crate) async fn role_permissions(
    conn: &mut AnyConnection,
    space_id: &str,
    role: &AssignedRole,
) -> Result<BTreeSet<Permission>, AppError> {
    let raw: Option<String> = match &role.source {
        RoleSource::GlobalGroup { .. } => {
            sqlx::query_scalar("SELECT permissions FROM global_roles WHERE id=$1")
                .bind(&role.id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(AppError::internal)?
        }
        _ => {
            if let Some(tokens) = built_in(&role.id) {
                return Ok(tokens);
            }
            sqlx::query_scalar("SELECT permissions FROM space_roles WHERE id=$1 AND space_id=$2")
                .bind(&role.id)
                .bind(space_id)
                .fetch_optional(&mut *conn)
                .await
                .map_err(AppError::internal)?
        }
    };
    let raw = raw.ok_or_else(invalid_policy)?;
    if matches!(role.source, RoleSource::GlobalGroup { .. }) {
        parse_permissions(&raw)
    } else {
        space_custom_role_permissions(&raw)
    }
}
/// The same legacy space policy expansion serves current role resolution and
/// requested custom-role grant ceilings. Global JSON must remain granular.
pub(crate) fn space_custom_role_permissions(raw: &str) -> Result<BTreeSet<Permission>, AppError> {
    let mut permissions = parse_permissions(raw)?;
    if permissions.contains(&Permission::EditContent) {
        permissions.extend(ALL[2..10].iter().copied());
    }
    Ok(permissions)
}

struct CurrentHumanIdentity {
    actor_id: String,
    role: String,
    account_kind: String,
    key_scopes: Option<BTreeSet<ApiKeyScope>>,
}
async fn current_human_identity(
    conn: &mut AnyConnection,
    principal: Principal<'_>,
) -> Result<CurrentHumanIdentity, AppError> {
    let mut key_scopes = None;
    let (actor, session, token_expiry) = match principal {
        Principal::User {
            user_id,
            session_id,
            token_expires_at,
        } if !user_id.starts_with("service:") => (user_id, session_id, token_expires_at),
        Principal::UserKey {
            key_id,
            user_id,
            token_expires_at,
        } if !user_id.starts_with("service:") => {
            let CurrentKeyCapability::User(key) = resolve_current_key(conn, key_id).await? else {
                return Err(key_unavailable());
            };
            if key.user_id != user_id {
                return Err(key_unavailable());
            }
            key_scopes = Some(key.scopes);
            (user_id, None, token_expires_at)
        }
        _ => return Err(AppError::Unauthorized("Principal unavailable".into())),
    };
    if token_expiry.is_some_and(|expiry| expiry <= chrono::Utc::now().timestamp()) {
        return Err(AppError::Unauthorized("Token unavailable".into()));
    }
    let account: Option<(String, i64, String)> =
        sqlx::query_as("SELECT role,is_active,account_kind FROM users WHERE id=$1")
            .bind(actor)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    let Some((role, 1, account_kind)) = account else {
        return Err(AppError::Unauthorized("Account unavailable".into()));
    };
    if !matches!(account_kind.as_str(), "member" | "guest") { return Err(invalid_policy()); }
    if let Some(session_id) = session {
        let expiry: Option<String> =
            sqlx::query_scalar("SELECT expires_at FROM sessions WHERE id=$1 AND user_id=$2")
                .bind(session_id)
                .bind(actor)
                .fetch_optional(&mut *conn)
                .await
                .map_err(AppError::internal)?;
        if !expiry
            .as_deref()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .is_some_and(|time| time > chrono::Utc::now())
        {
            return Err(AppError::Unauthorized("Session unavailable".into()));
        }
    }
    Ok(CurrentHumanIdentity {
        actor_id: actor.into(),
        role,
        account_kind,
        key_scopes,
    })
}
/// Current credential identity only, preserving the caller's existing scope policy.
/// Authentication credential transactions supply their own operation policy.
pub(crate) async fn require_current_human_identity(
    conn: &mut AnyConnection,
    auth: &crate::auth::middleware::AuthUser,
) -> Result<String, AppError> {
    Ok(current_human_identity(conn, principal_from_auth(auth)?).await?.actor_id)
}

/// Current credential proof only; this grants no space/page/private participation.
/// Empty/global read adapters still need this proof before and after their work.
pub(crate) async fn require_current_human_read(
    conn: &mut AnyConnection,
    auth: &crate::auth::middleware::AuthUser,
) -> Result<String, AppError> {
    let current = current_human_identity(conn, principal_from_auth(auth)?).await?;
    if current.key_scopes.as_ref().is_some_and(|scopes| {
        !scopes.contains(&ApiKeyScope::Read) && !scopes.contains(&ApiKeyScope::Admin)
    }) {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    Ok(current.actor_id)
}

/// Global human catalogs require a current ordinary member, not a page grant.
pub(crate) async fn require_current_human_member_read(
    conn: &mut AnyConnection,
    auth: &crate::auth::middleware::AuthUser,
) -> Result<String, AppError> {
    let current = current_human_identity(conn, principal_from_auth(auth)?).await?;
    if current.account_kind != "member"
        || current.key_scopes.as_ref().is_some_and(|scopes| {
            !scopes.contains(&ApiKeyScope::Read) && !scopes.contains(&ApiKeyScope::Admin)
        })
    {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    Ok(current.actor_id)
}

/// Current instance-admin identity only, with the actual Admin key ceiling.
/// Global role labels/permission unions never manufacture this authority.
pub(crate) async fn require_current_human_instance_admin(
    conn: &mut AnyConnection,
    auth: &crate::auth::middleware::AuthUser,
) -> Result<String, AppError> {
    let current = current_human_identity(conn, principal_from_auth(auth)?).await?;
    if current.role != "admin" || current.account_kind != "member"
        || current
            .key_scopes
            .as_ref()
            .is_some_and(|scopes| !scopes.contains(&ApiKeyScope::Admin))
    {
        return Err(AppError::Unauthorized(
            "Admin credential unavailable".into(),
        ));
    }
    Ok(current.actor_id)
}

/// Identity-only proof for erasing an actor's own preference. This grants no
/// space/page visibility, private participation, or content mutation permission.
pub(crate) async fn require_current_human_preference_write(
    conn: &mut AnyConnection,
    auth: &crate::auth::middleware::AuthUser,
) -> Result<String, AppError> {
    let current = current_human_identity(conn, principal_from_auth(auth)?).await?;
    if current.key_scopes.as_ref().is_some_and(|scopes| {
        !scopes.contains(&ApiKeyScope::Write) && !scopes.contains(&ApiKeyScope::Admin)
    }) {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    Ok(current.actor_id)
}

/// Evaluates current identity, scope, blocks and additive explicit permission
/// tokens on one connection. Role labels never stand in for permissions or for
/// instance administrator identity. Recovery returns an audit obligation only.
pub async fn resolve_space_access(
    conn: &mut AnyConnection,
    principal: Principal<'_>,
    space_id: &str,
    purpose: Purpose<'_>,
) -> Result<SpaceAccess, AppError> {
    resolve_space_access_for_page_operation(
        conn,
        principal,
        space_id,
        purpose,
        None,
        SpaceCredentialMode::Ordinary,
    )
    .await
}
#[derive(Clone, Copy)]
enum SpaceCredentialMode {
    Ordinary,
    ManagementCatalogRead,
    MembershipManagement,
    TemplateDraftRead,
    MovePreviewRead,
    MovePreviewWrite,
    MaintenanceCatalogRead,
    MaintenanceCatalogWrite,
}
/// Preview only: account MoveContent+View with the request-method ceiling.
/// This Result never exposes a capability usable to commit a relocation.
#[derive(Clone, Copy, Debug)]
pub(crate) enum MovePreviewCredential {
    Read,
    Write,
}
pub(crate) async fn require_move_preview_space(
    conn: &mut AnyConnection,
    auth: &crate::auth::middleware::AuthUser,
    space_id: &str,
    credential: MovePreviewCredential,
) -> Result<(), AppError> {
    let mode = match credential {
        MovePreviewCredential::Read => SpaceCredentialMode::MovePreviewRead,
        MovePreviewCredential::Write => SpaceCredentialMode::MovePreviewWrite,
    };
    let access = resolve_space_access_for_page_operation(
        conn,
        principal_from_auth(auth)?,
        space_id,
        Purpose::Routine,
        None,
        mode,
    )
    .await?;
    if !access.allows(Permission::MoveContent) || !access.allows(Permission::View) {
        return Err(unavailable());
    }
    Ok(())
}
/// Read a definition used in a preview under its actual request-method ceiling.
pub(crate) async fn require_move_preview_metadata_space(
    conn: &mut AnyConnection,
    auth: &crate::auth::middleware::AuthUser,
    space_id: &str,
    credential: MovePreviewCredential,
) -> Result<(), AppError> {
    let operation = match credential {
        MovePreviewCredential::Read => None,
        MovePreviewCredential::Write => Some(PageOperation::InspectMovePreviewFromMutation),
    };
    let current = resolve_space_access_for_page_operation(
        conn,
        principal_from_auth(auth)?,
        space_id,
        Purpose::Routine,
        operation,
        SpaceCredentialMode::Ordinary,
    )
    .await?;
    if !current.allows(Permission::View) {
        return Err(unavailable());
    }
    Ok(())
}
/// Read an existing management catalog with account View+ManageSpace and the
/// actual credential's Read/Admin ceiling. This returns no mutation capability.
pub(crate) async fn require_space_management_catalog_read(
    conn: &mut AnyConnection,
    auth: &crate::auth::middleware::AuthUser,
    space_id: &str,
) -> Result<(), AppError> {
    let access = resolve_space_access_for_page_operation(
        conn,
        principal_from_auth(auth)?,
        space_id,
        Purpose::Routine,
        None,
        SpaceCredentialMode::ManagementCatalogRead,
    )
    .await?;
    if !access.allows(Permission::View) || !access.allows(Permission::ManageSpace) {
        return Err(unavailable());
    }
    Ok(())
}
/// Target inspection only: retain the account management-catalog scope with the
/// actual request-method key ceiling. This returns no management capability.
#[derive(Clone, Copy)]
pub(crate) enum MaintenanceTargetCredential {
    Read,
    Write,
}
pub(crate) async fn maintenance_groups_available(
    conn: &mut AnyConnection,
    auth: &crate::auth::middleware::AuthUser,
    space: &str,
    credential: MaintenanceTargetCredential,
) -> Result<bool, AppError> {
    let mode = match credential {
        MaintenanceTargetCredential::Read => SpaceCredentialMode::MaintenanceCatalogRead,
        MaintenanceTargetCredential::Write => SpaceCredentialMode::MaintenanceCatalogWrite,
    };
    let current = match resolve_space_access_for_page_operation(
        conn,
        principal_from_auth(auth)?,
        space,
        Purpose::Routine,
        None,
        mode,
    )
    .await
    {
        Ok(current) => current,
        Err(AppError::NotFound(_)) => return Ok(false),
        Err(e) => return Err(e),
    };
    Ok(current.allows(Permission::View) && current.allows(Permission::ManageSpace))
}
/// Direct membership management only: current real write credential and account
/// ManageSpace, returning the account permission union as the grant ceiling.
/// Generic space/page resolution and key capability intersections are unchanged.
pub(crate) async fn require_space_membership_management(
    conn: &mut AnyConnection,
    auth: &crate::auth::middleware::AuthUser,
    space_id: &str,
) -> Result<BTreeSet<Permission>, AppError> {
    let access = resolve_space_access_for_page_operation(
        conn,
        principal_from_auth(auth)?,
        space_id,
        Purpose::Routine,
        None,
        SpaceCredentialMode::MembershipManagement,
    )
    .await?;
    if !access.allows(Permission::ManageSpace) {
        return Err(if access.allows(Permission::View) {
            AppError::Forbidden("Insufficient role".into())
        } else {
            unavailable()
        });
    }
    Ok(access.permissions)
}

/// Shared template drafts retain account ManageTemplates participation; unlike
/// document drafts this does not invent an owner/share ACL or require View.
pub(crate) async fn require_template_draft_read(
    conn: &mut AnyConnection,
    auth: &crate::auth::middleware::AuthUser,
    space_id: &str,
) -> Result<(), AppError> {
    let access = resolve_space_access_for_page_operation(
        conn,
        principal_from_auth(auth)?,
        space_id,
        Purpose::Routine,
        None,
        SpaceCredentialMode::TemplateDraftRead,
    )
    .await?;
    if !access.allows(Permission::ManageTemplates) {
        return Err(unavailable());
    }
    Ok(())
}

pub(crate) async fn space_membership_role_permissions(
    conn: &mut AnyConnection,
    space_id: &str,
    role: &str,
) -> Result<BTreeSet<Permission>, AppError> {
    if !matches!(role, "space_admin" | "editor" | "commenter" | "viewer") {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM space_roles WHERE space_id=$1 AND id=$2")
                .bind(space_id)
                .bind(role)
                .fetch_optional(&mut *conn)
                .await
                .map_err(AppError::internal)?;
        if exists.is_none() {
            return Err(AppError::BadRequest("Invalid space role".into()));
        }
    }
    role_permissions(
        conn,
        space_id,
        &AssignedRole {
            id: role.into(),
            source: RoleSource::Direct,
        },
    )
    .await
}

async fn resolve_space_access_for_page_operation(
    conn: &mut AnyConnection,
    principal: Principal<'_>,
    space_id: &str,
    purpose: Purpose<'_>,
    operation: Option<PageOperation>,
    credential_mode: SpaceCredentialMode,
) -> Result<SpaceAccess, AppError> {
    let current = current_human_identity(conn, principal).await?;
    calculate_space_access(
        conn, &current.actor_id, current.role, current.key_scopes,
        space_id, purpose, operation, credential_mode,
    ).await
}

/// Shared canonical account permission calculation. This does not authenticate
/// a recipient or confer actor authority; only actual principal callers do that.
async fn calculate_space_access(
    conn: &mut AnyConnection,
    actor: &str,
    role: String,
    key_scopes: Option<BTreeSet<ApiKeyScope>>,
    space_id: &str,
    purpose: Purpose<'_>,
    operation: Option<PageOperation>,
    credential_mode: SpaceCredentialMode,
) -> Result<SpaceAccess, AppError> {
    if matches!(credential_mode, SpaceCredentialMode::MembershipManagement)
        && key_scopes.as_ref().is_some_and(|scopes| {
            !scopes.contains(&ApiKeyScope::Write) && !scopes.contains(&ApiKeyScope::Admin)
        })
    {
        return Err(unavailable());
    }
    let visibility: Option<String> =
        sqlx::query_scalar("SELECT visibility FROM spaces WHERE id=$1")
            .bind(space_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    let visibility = visibility.ok_or_else(unavailable)?;
    let account_kind: String = sqlx::query_scalar("SELECT account_kind FROM users WHERE id=$1")
        .bind(actor).fetch_one(&mut *conn).await.map_err(AppError::internal)?;
    if !matches!(account_kind.as_str(), "member" | "guest") { return Err(invalid_policy()); }
    let guest = account_kind == "guest";
    let instance_admin = role == "admin" && !guest;
    let recovery_audit_required = match purpose {
        Purpose::Routine => None,
        Purpose::AdminRecovery { reason }
            if instance_admin
                && !reason.trim().is_empty()
                && key_scopes
                    .as_ref()
                    .is_none_or(|scopes| scopes.contains(&ApiKeyScope::Admin)) =>
        {
            Some(RecoveryAuditRequired {
                actor_id: actor.into(),
                space_id: space_id.into(),
                reason: reason.into(),
            })
        }
        Purpose::AdminRecovery { .. } => return Err(unavailable()),
    };
    let blocked: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM space_user_blocks WHERE space_id=$1 AND user_id=$2")
            .bind(space_id)
            .bind(actor)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    if blocked.is_some() && recovery_audit_required.is_none() {
        return Err(unavailable());
    }
    let mut roles = Vec::new();
    let direct: Option<String> =
        sqlx::query_scalar("SELECT role FROM space_members WHERE space_id=$1 AND user_id=$2")
            .bind(space_id)
            .bind(actor)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    if let Some(id) = direct {
        roles.push(AssignedRole {
            id,
            source: RoleSource::Direct,
        });
    }
    let groups: Vec<(String,String)> = sqlx::query_as("SELECT sgp.group_id,sgp.role FROM space_group_permissions sgp JOIN group_members gm ON gm.group_id=sgp.group_id WHERE sgp.space_id=$1 AND gm.user_id=$2 ORDER BY sgp.group_id,sgp.role").bind(space_id).bind(actor).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    for (group_id, id) in groups {
        roles.push(AssignedRole {
            id,
            source: RoleSource::SpaceGroup { group_id },
        });
    }
    let globals: Vec<(String,String)> = sqlx::query_as("SELECT gr.group_id,gr.role_id FROM group_global_roles gr JOIN group_members gm ON gm.group_id=gr.group_id WHERE gm.user_id=$1 ORDER BY gr.group_id,gr.role_id").bind(actor).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    for (group_id, id) in globals {
        roles.push(AssignedRole {
            id,
            source: RoleSource::GlobalGroup { group_id },
        });
    }
    if guest { roles.clear(); }
    let mut permissions = BTreeSet::new();
    for role in &roles {
        permissions.extend(role_permissions(conn, space_id, role).await?);
    }
    if !guest && matches!(visibility.as_str(), "internal" | "public") {
        permissions.insert(Permission::View);
    }
    if instance_admin {
        permissions.extend(ALL);
    }
    if let Some(scopes) = &key_scopes {
        if !matches!(credential_mode, SpaceCredentialMode::MembershipManagement) {
            permissions.retain(|permission| {
                if matches!(permission, Permission::View | Permission::ManageSpace)
                    && matches!(
                        credential_mode,
                        SpaceCredentialMode::MaintenanceCatalogRead
                            | SpaceCredentialMode::MaintenanceCatalogWrite
                    )
                {
                    scopes.contains(&ApiKeyScope::Admin)
                        || scopes.contains(&match credential_mode {
                            SpaceCredentialMode::MaintenanceCatalogRead => ApiKeyScope::Read,
                            _ => ApiKeyScope::Write,
                        })
                } else if matches!(permission, Permission::View | Permission::MoveContent)
                    && matches!(
                        credential_mode,
                        SpaceCredentialMode::MovePreviewRead
                            | SpaceCredentialMode::MovePreviewWrite
                    )
                {
                    scopes.contains(&ApiKeyScope::Admin)
                        || scopes.contains(&match credential_mode {
                            SpaceCredentialMode::MovePreviewRead => ApiKeyScope::Read,
                            _ => ApiKeyScope::Write,
                        })
                } else if (*permission == Permission::ManageSpace
                    && matches!(credential_mode, SpaceCredentialMode::ManagementCatalogRead))
                    || (*permission == Permission::ManageTemplates
                        && matches!(credential_mode, SpaceCredentialMode::TemplateDraftRead))
                {
                    scopes.contains(&ApiKeyScope::Read) || scopes.contains(&ApiKeyScope::Admin)
                } else if *permission == Permission::View
                    && matches!(
                        operation,
                        Some(
                            PageOperation::AcknowledgeNotification
                                | PageOperation::UserPagePreference
                                | PageOperation::ReferenceFromMutation
                                | PageOperation::InspectMovePreviewFromMutation
                                | PageOperation::DraftOwnerMutation
                        )
                    )
                {
                    scopes.contains(&ApiKeyScope::Write) || scopes.contains(&ApiKeyScope::Admin)
                } else {
                    key_allows(scopes, *permission)
                }
            });
        }
    }
    Ok(SpaceAccess {
        actor_id: actor.into(),
        space_id: space_id.into(),
        instance_admin,
        roles,
        permissions,
        recovery_audit_required,
    })
}
async fn draft_participation(
    conn: &mut AnyConnection,
    space_id: &str,
    document_id: &str,
    draft_id: Option<&str>,
    actor: &str,
    edit: bool,
) -> Result<Option<DraftParticipation>, AppError> {
    let rows:Vec<(String,Option<String>)>=sqlx::query_as("SELECT d.owner_id,a.role FROM document_drafts d LEFT JOIN document_draft_access a ON a.draft_id=d.id AND a.user_id=$1 WHERE d.document_id=$2 AND d.space_id=$3 AND ($4 IS NULL OR d.id=$4) ORDER BY d.id")
        .bind(actor).bind(document_id).bind(space_id).bind(draft_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    for (owner, role) in rows {
        if owner == actor {
            return Ok(Some(DraftParticipation::Owner));
        }
        match role.as_deref() {
            Some("editor") => return Ok(Some(DraftParticipation::SharedEditor)),
            Some("viewer") if !edit => return Ok(Some(DraftParticipation::SharedViewer)),
            Some("viewer") | None => {}
            _ => return Err(invalid_policy()),
        }
    }
    Ok(None)
}
/// Restriction rows are not additive grants and never change space authority.
/// Missing policy means inherited; empty restricted policy denies routine access.
/// All rows are validated even when the actor matches another allowlist entry.
async fn require_page_restriction(
    conn: &mut AnyConnection,
    space: &str,
    document: &str,
    actor: Option<&str>,
    recovery: bool,
) -> Result<(), AppError> {
    let policy: Option<(String, String, String)> = sqlx::query_as(
        "SELECT space_id,mode,revision FROM document_access_policies WHERE document_id=$1",
    )
    .bind(document)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let entries:Vec<(String,Option<String>,Option<String>)>=sqlx::query_as("SELECT space_id,user_id,group_id FROM document_access_allowlist WHERE document_id=$1 ORDER BY id").bind(document).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let Some((policy_space, mode, revision)) = policy else {
        return if entries.is_empty() {
            Ok(())
        } else {
            Err(invalid_policy())
        };
    };
    if policy_space != space
        || revision.trim().is_empty()
        || !matches!(mode.as_str(), "inherited" | "restricted")
    {
        return Err(invalid_policy());
    }
    let mut matched = false;
    for (entry_space, user, group) in entries {
        if entry_space != space {
            return Err(invalid_policy());
        }
        match (user, group) {
            (Some(user), None) => {
                let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM users WHERE id=$1")
                    .bind(&user)
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(AppError::internal)?;
                if exists.is_none() {
                    return Err(invalid_policy());
                }
                matched |= actor == Some(user.as_str());
            }
            (None, Some(group)) => {
                let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM groups WHERE id=$1")
                    .bind(&group)
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(AppError::internal)?;
                if exists.is_none() {
                    return Err(invalid_policy());
                }
                if let Some(actor) = actor {
                    let member: Option<i64> = sqlx::query_scalar(
                        "SELECT 1 FROM group_members WHERE group_id=$1 AND user_id=$2",
                    )
                    .bind(&group)
                    .bind(actor)
                    .fetch_optional(&mut *conn)
                    .await
                    .map_err(AppError::internal)?;
                    matched |= member.is_some();
                }
            }
            _ => return Err(invalid_policy()),
        }
    }
    if mode == "restricted" && !matched && !recovery {
        return Err(unavailable());
    }
    Ok(())
}
/// Anonymous links are separate from human allowlists. Until a reviewed explicit
/// link confirmation API exists, any restricted node/ancestor bars anonymous I/O.
/// Unrestricted private ancestors retain existing explicit-link semantics.
pub(crate) async fn require_anonymous_page_chain(
    conn: &mut AnyConnection,
    space: &str,
    document: &str,
) -> Result<(), AppError> {
    let mut next = Some(document.to_owned());
    let mut seen = BTreeSet::new();
    while let Some(id) = next {
        if !seen.insert(id.clone()) {
            return Err(invalid_policy());
        }
        let row:Option<(Option<String>,)>=sqlx::query_as("SELECT parent_id FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&id).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        let (parent,) = row.ok_or_else(unavailable)?;
        require_page_restriction(conn, space, &id, None, false).await?;
        next = parent;
    }
    Ok(())
}

/// Resource admission for one page closure and its source/destination chains.
/// This is not a workspace-size cap or a product nesting-depth policy.
pub(crate) const MAX_PAGE_OPERATION_NODES: usize = 10_000;
#[derive(Default)]
pub(crate) struct PageTraversalBudget(BTreeSet<String>);
impl PageTraversalBudget {
    pub(crate) fn visit(&mut self, id: &str) -> Result<(), AppError> {
        self.0.insert(id.to_owned());
        if self.0.len() > MAX_PAGE_OPERATION_NODES {
            return Err(AppError::BadRequest("Page operation exceeds the 10000-node traversal budget".into()));
        }
        Ok(())
    }
    pub(crate) fn len(&self) -> usize { self.0.len() }
}
fn page_permission(operation: PageOperation) -> Permission {
match operation {
        PageOperation::View
        | PageOperation::GrantedPublishedView
        | PageOperation::InspectMetadata
        | PageOperation::InspectArchivedMetadata
        | PageOperation::InspectTrashedMetadata
        | PageOperation::AcknowledgeNotification
        | PageOperation::UserPagePreference
        | PageOperation::ReferenceFromMutation
        | PageOperation::InspectMovePreviewFromMutation
        | PageOperation::DraftOwnerRead
        | PageOperation::DraftOwnerMutation => Permission::View,
        PageOperation::Comment => Permission::Comment,
        PageOperation::Edit
        | PageOperation::PublicLinkEnable
        | PageOperation::PublicLinkDisable => Permission::EditContent,
        PageOperation::DeleteActive
        | PageOperation::DeleteArchived
        | PageOperation::DeleteTrashed
        | PageOperation::Trash
        | PageOperation::RestoreTrashed
        | PageOperation::RestoreNotDeleted => Permission::DeleteContent,
        PageOperation::Archive
        | PageOperation::UnarchiveArchived
        | PageOperation::UnarchiveActive => Permission::ArchiveContent,
        PageOperation::Move => Permission::MoveContent,
        PageOperation::ManageLock
        | PageOperation::RestrictionPreview
        | PageOperation::RestrictionApply => Permission::ManageSpace,
    }
}
fn page_editor_participation(operation: PageOperation) -> bool {
matches!(
        operation,
        PageOperation::Edit
            | PageOperation::Move
            | PageOperation::ManageLock
            | PageOperation::RestrictionPreview
            | PageOperation::RestrictionApply
            | PageOperation::PublicLinkEnable
            | PageOperation::PublicLinkDisable
            | PageOperation::DeleteActive
            | PageOperation::DeleteArchived
            | PageOperation::DeleteTrashed
            | PageOperation::Trash
            | PageOperation::Archive
            | PageOperation::UnarchiveArchived
            | PageOperation::UnarchiveActive
            | PageOperation::RestoreTrashed
            | PageOperation::RestoreNotDeleted
    )
}
#[derive(Clone,PartialEq,Eq)]
struct PageAccessNode {
    parent: Option<String>,
    published: Option<String>,
    locked: i64,
    locked_by: Option<String>,
}
async fn read_page_access_node(
    conn: &mut AnyConnection,
    space_id: &str,
    id: &str,
    document_id: &str,
    operation: PageOperation,
) -> Result<Option<PageAccessNode>, AppError> {
        let row:Option<(Option<String>,Option<String>,i64,Option<String>)>=sqlx::query_as("SELECT parent_id,published_at,locked,locked_by FROM documents WHERE id=$1 AND space_id=$2 AND (deleted_at IS NULL OR $4=1) AND (archived_at IS NULL OR $3=1) AND ($5=0 OR (archived_at IS NOT NULL AND deleted_at IS NULL)) AND ($6=0 OR deleted_at IS NOT NULL) AND ($7=0 OR (deleted_at IS NULL AND archived_at IS NULL)) AND ($8=0 OR deleted_at IS NULL)").bind(&id).bind(space_id).bind(if matches!(operation, PageOperation::DraftOwnerRead | PageOperation::DraftOwnerMutation | PageOperation::InspectMetadata | PageOperation::PublicLinkDisable | PageOperation::InspectArchivedMetadata | PageOperation::InspectTrashedMetadata | PageOperation::DeleteActive | PageOperation::DeleteArchived | PageOperation::DeleteTrashed | PageOperation::Trash | PageOperation::Archive | PageOperation::UnarchiveArchived | PageOperation::UnarchiveActive | PageOperation::RestoreTrashed | PageOperation::RestoreNotDeleted) {1_i64} else {0_i64}).bind(if matches!(operation, PageOperation::DraftOwnerRead | PageOperation::DraftOwnerMutation | PageOperation::InspectTrashedMetadata | PageOperation::DeleteActive | PageOperation::DeleteArchived | PageOperation::DeleteTrashed | PageOperation::Trash | PageOperation::Archive | PageOperation::UnarchiveArchived | PageOperation::UnarchiveActive | PageOperation::RestoreTrashed | PageOperation::RestoreNotDeleted) {1_i64} else {0_i64}).bind(if id == document_id && matches!(operation, PageOperation::InspectArchivedMetadata | PageOperation::DeleteArchived | PageOperation::UnarchiveArchived) {1_i64} else {0_i64}).bind(if id == document_id && matches!(operation, PageOperation::InspectTrashedMetadata | PageOperation::DeleteTrashed | PageOperation::RestoreTrashed) {1_i64} else {0_i64}).bind(if id == document_id && matches!(operation, PageOperation::DeleteActive | PageOperation::UnarchiveActive) {1_i64} else {0_i64}).bind(if id == document_id && matches!(operation, PageOperation::Trash | PageOperation::Archive | PageOperation::UnarchiveArchived | PageOperation::UnarchiveActive | PageOperation::RestoreNotDeleted) {1_i64} else {0_i64}).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    Ok(row.map(|(parent,published,locked,locked_by)| PageAccessNode {parent,published,locked,locked_by}))
}
/// Shared by single-page and phase-local batch resolution. Ancestors retain
/// their existing weaker participation/lock semantics; only explicit targets
/// require the operation's editor participation and own-page lock admission.
async fn require_page_access_node(
    conn: &mut AnyConnection,
    space: &SpaceAccess,
    space_id: &str,
    id: &str,
    operation: PageOperation,
    node: &PageAccessNode,
    target: bool,
) -> Result<(), AppError> {
        require_page_restriction(
            conn,
            space_id,
            &id,
            Some(&space.actor_id),
            space.recovery_audit_required.is_some(),
        )
        .await?;
        if space.recovery_audit_required.is_none() {
            if node.published.is_none()
                && draft_participation(
                    conn,
                    space_id,
                    &id,
                    None,
                    &space.actor_id,
                    page_editor_participation(operation) && target,
                )
                .await?
                .is_none()
            {
                return Err(unavailable());
            }
            if matches!(operation, PageOperation::Edit | PageOperation::Move)
                && target
                && node.locked != 0
                && node.locked_by.as_deref() != Some(space.actor_id.as_str())
            {
                return Err(AppError::Conflict(
                    "Document is locked by another user".into(),
                ));
            }
        }
    Ok(())
}

#[derive(Clone)]
enum PageBatchDecision { Allowed, Unavailable, Conflict(String) }
impl PageBatchDecision {
    fn from_result(result: Result<(),AppError>) -> Result<Self,AppError> {
        match result {
            Ok(()) => Ok(Self::Allowed),
            Err(AppError::NotFound(_)) => Ok(Self::Unavailable),
            Err(AppError::Conflict(message)) => Ok(Self::Conflict(message)),
            Err(other) => Err(other),
        }
    }
    fn require(&self) -> Result<(),AppError> {
        match self { Self::Allowed => Ok(()), Self::Unavailable => Err(unavailable()), Self::Conflict(message) => Err(AppError::Conflict(message.clone())) }
    }
}
/// Non-authoritative, phase-local result. It reveals no ancestor identifiers or
/// metadata and is discarded before subsequent locks/source/CAS waits.
pub(crate) struct PageAccessBatch<'a> { targets: std::collections::BTreeMap<String,PageBatchDecision>, nodes: std::collections::BTreeMap<String,Option<PageAccessNode>>, principal: Principal<'a>, space: SpaceAccess, operation: PageOperation }
impl PageAccessBatch<'_> {
    pub(crate) fn require(&self, id: &str) -> Result<(),AppError> {
        self.targets.get(id).ok_or_else(unavailable)?.require()
    }
    pub(crate) fn authorized_nodes(&self) -> BTreeSet<String> {
        let mut result=BTreeSet::new();
        for (target,decision) in &self.targets {
            if !matches!(decision,PageBatchDecision::Allowed) {continue;}
            let mut current=Some(target.as_str());
            while let Some(id)=current {
                if !result.insert(id.to_owned()) {break;}
                current=self.nodes.get(id).and_then(Option::as_ref).and_then(|node|node.parent.as_deref());
            }
        }
        result
    }
    pub(crate) async fn require_draft_view(&self,conn:&mut AnyConnection,document:&str,draft:&str)->Result<(),AppError> {
        if self.operation!=PageOperation::View {return Err(unavailable());}
        self.require(document)?;
        require_selected_draft_participation(conn,&self.space,document,draft,false).await?;
        let current=resolve_space_access_for_page_operation(conn,self.principal,&self.space.space_id,Purpose::Routine,Some(PageOperation::View),SpaceCredentialMode::Ordinary).await?;
        if current.actor_id!=self.space.actor_id || !current.allows(Permission::View) {return Err(unavailable());}
        Ok(())
    }
    pub(crate) fn require_all(&self) -> Result<(),AppError> {
        for decision in self.targets.values() { decision.require()?; }
        Ok(())
    }
}
#[cfg(test)]
thread_local! {static PAGE_BATCH_FINAL_SQL:std::cell::RefCell<Option<&'static str>>=const{std::cell::RefCell::new(None)};}
/// Resolve one fresh current-active phase using the same node policy as the
/// single resolver. No direct grants, lifecycle exceptions or recovery purpose
/// are admitted here. Unrelated read-catalog roots do not share an operation cap.
pub(crate) async fn resolve_page_batch<'a>(
    conn: &mut AnyConnection,
    principal: Principal<'a>,
    space_id: &str,
    targets: &BTreeSet<String>,
    operation: PageOperation,
) -> Result<PageAccessBatch<'a>,AppError> {
    if !matches!(operation,PageOperation::View|PageOperation::Edit|PageOperation::Move|PageOperation::InspectMovePreviewFromMutation) {
        return Err(AppError::BadRequest("Unsupported page batch operation".into()));
    }
    let space = resolve_space_access_for_page_operation(conn,principal,space_id,Purpose::Routine,Some(operation),SpaceCredentialMode::Ordinary).await?;
    if !space.allows(page_permission(operation)) { return Err(unavailable()); }
    let bounded_union = matches!(operation,PageOperation::Edit|PageOperation::Move);
    let mut budget=PageTraversalBudget::default();
    let mut nodes=std::collections::BTreeMap::<String,Option<PageAccessNode>>::new();
    for target in targets {
        let mut current=Some(target.clone());
        let mut path=BTreeSet::new();
        while let Some(id)=current {
            if nodes.contains_key(&id) { break; }
            if !path.insert(id.clone()) { return Err(invalid_policy()); }
            if bounded_union && path.len()>MAX_PAGE_OPERATION_NODES { return Err(AppError::BadRequest("Page operation exceeds the 10000-node traversal budget".into())); }
            if bounded_union { budget.visit(&id)?; }
            let node=read_page_access_node(conn,space_id,&id,&id,operation).await?;
            current=node.as_ref().and_then(|n|n.parent.clone());
            nodes.insert(id,node);
        }
    }
    let mut weak=std::collections::BTreeMap::new();
    let mut direct=std::collections::BTreeMap::new();
    for (id,node) in &nodes {
        let decision=if let Some(node)=node {
            PageBatchDecision::from_result(require_page_access_node(conn,&space,space_id,id,operation,node,false).await)?
        } else { PageBatchDecision::Unavailable };
        weak.insert(id.clone(),decision);
        if targets.contains(id) {
            let decision=if let Some(node)=node {
                PageBatchDecision::from_result(require_page_access_node(conn,&space,space_id,id,operation,node,true).await)?
            } else { PageBatchDecision::Unavailable };
            direct.insert(id.clone(),decision);
        }
    }
    let mut inherited=std::collections::BTreeMap::<String,(PageBatchDecision,usize)>::new();
    for start in nodes.keys() {
        if inherited.contains_key(start) { continue; }
        let mut path=Vec::new();let mut seen=BTreeSet::new();let mut current=Some(start.clone());
        let mut base=PageBatchDecision::Allowed;let mut depth=0;
        while let Some(id)=current {
            if let Some((decision,known))=inherited.get(&id) {base=decision.clone();depth=*known;break;}
            if !seen.insert(id.clone()) { base=PageBatchDecision::Conflict("Access policy is invalid".into());break; }
            current=nodes.get(&id).and_then(Option::as_ref).and_then(|node|node.parent.clone());path.push(id);
        }
        for id in path.into_iter().rev() {
            depth+=1;
            if bounded_union && depth>MAX_PAGE_OPERATION_NODES {return Err(AppError::BadRequest("Page operation exceeds the 10000-node traversal budget".into()));}
            let own=weak.get(&id).cloned().unwrap_or(PageBatchDecision::Unavailable);
            if !matches!(own,PageBatchDecision::Allowed) {base=own;}
            inherited.insert(id,(base.clone(),depth));
        }
    }
    let mut result=std::collections::BTreeMap::new();
    for target in targets {
        let mut decision=direct.remove(target).unwrap_or(PageBatchDecision::Unavailable);
        if matches!(decision,PageBatchDecision::Allowed) {
            if let Some(parent)=nodes.get(target).and_then(Option::as_ref).and_then(|n|n.parent.as_ref()) {
                decision=inherited.get(parent).map(|v|v.0.clone()).unwrap_or(PageBatchDecision::Unavailable);
            }
        }
        result.insert(target.clone(),decision);
    }
    #[cfg(test)]
    if let Some(statement)=PAGE_BATCH_FINAL_SQL.with(|slot|slot.borrow_mut().take()) {
        sqlx::query(statement).execute(&mut *conn).await.map_err(AppError::internal)?;
    }
    // A page/edge may have changed while another node's policy SQL waited.
    // Check retained topology/current lifecycle without repeating ancestor walks.
    for (id,expected) in &nodes {
        if read_page_access_node(conn,space_id,id,id,operation).await? != *expected {
            return Err(unavailable());
        }
    }
    // Do not let a credential which expired/revoked during node SQL authorize
    // the phase. Later locks/source waits require a wholly new batch.
    let current=resolve_space_access_for_page_operation(conn,principal,space_id,Purpose::Routine,Some(operation),SpaceCredentialMode::Ordinary).await?;
    if current.actor_id!=space.actor_id || !current.allows(page_permission(operation)) || current.permissions!=space.permissions {return Err(unavailable());}
    Ok(PageAccessBatch {targets:result,nodes,principal,space,operation})
}

/// Ordinary operations require an active page and active ancestor chain.
/// Explicit lifecycle metadata operations admit only their target state and
/// inspect inactive ancestors solely for policy; no title/body is read here.
/// Draft shares and restrictions apply independently at every ancestor.
/// Attribution and routine instance-admin authority do not bypass either.
pub async fn resolve_page_access(
    conn: &mut AnyConnection,
    principal: Principal<'_>,
    space_id: &str,
    document_id: &str,
    operation: PageOperation,
    purpose: Purpose<'_>,
) -> Result<PageAccess, AppError> {
    let space = resolve_space_access_for_page_operation(
        conn,
        principal,
        space_id,
        purpose,
        Some(operation),
        if operation == PageOperation::RestrictionPreview {
            SpaceCredentialMode::ManagementCatalogRead
        } else {
            SpaceCredentialMode::Ordinary
        },
    )
    .await?;
    let permission = page_permission(operation);
    let direct_viewer_grant = if !space.allows(permission) && operation == PageOperation::GrantedPublishedView && matches!(purpose, Purpose::Routine) {
        let identity = current_human_identity(conn, principal).await?;
        if !(identity.account_kind == "guest" || matches!(identity.role.as_str(), "member" | "viewer")) || identity.key_scopes.as_ref().is_some_and(|scopes| !scopes.contains(&ApiKeyScope::Read) && !scopes.contains(&ApiKeyScope::Admin)) {
            return Err(unavailable());
        }
        super::page_viewer_grants::read_current_direct_viewer_grant(conn, space_id, document_id, &space.actor_id).await?
    } else { None };
    if !space.allows(permission) && direct_viewer_grant.is_none() {
        return Err(unavailable());
    }
    let mut next = Some(document_id.to_owned());
    let mut seen = BTreeSet::new();
    let mut ancestor_ids = Vec::new();
    let mut page_published = false;
    let mut page_locked_by = None;
    while let Some(id) = next {
        if !seen.insert(id.clone()) {
            return Err(invalid_policy());
        }
        let node = read_page_access_node(conn, space_id, &id, document_id, operation)
            .await?.ok_or_else(unavailable)?;
        require_page_access_node(conn, &space, space_id, &id, operation, &node, id == document_id).await?;
        let PageAccessNode { parent, published, locked, locked_by } = node;
        if id == document_id {
            page_published = published.is_some();
            page_locked_by = if locked != 0 { locked_by } else { None };
        } else {
            ancestor_ids.push(id.clone());
        }
        next = parent;
    }
    if operation == PageOperation::GrantedPublishedView && !page_published { return Err(unavailable()); }
    if let Some(original) = &direct_viewer_grant {
        #[cfg(test)]
        if let Ok((reached, release)) = DIRECT_VIEWER_FINAL_GATE.try_with(|pair| pair.clone()) { reached.notify_one(); release.notified().await; }
        // New grant dependencies are read again after every ancestor/draft wait.
        // Block/space admission is independently repeated, and the original
        // human credential is LAST. Downstream readers still need raw source
        // witnesses and their own final admission before route activation.
        resolve_space_access_for_page_operation(conn, principal, space_id, purpose, Some(operation), SpaceCredentialMode::Ordinary).await?;
        let current = super::page_viewer_grants::read_current_direct_viewer_grant(conn, space_id, document_id, &space.actor_id).await?;
        if current.as_ref() != Some(original) { return Err(unavailable()); }
        let identity = current_human_identity(conn, principal).await?;
        if identity.actor_id != space.actor_id || !(identity.account_kind == "guest" || matches!(identity.role.as_str(), "member" | "viewer")) || identity.key_scopes.as_ref().is_some_and(|scopes| !scopes.contains(&ApiKeyScope::Read) && !scopes.contains(&ApiKeyScope::Admin)) { return Err(unavailable()); }
    }
    let mut page_permissions = space.permissions.clone();
    if direct_viewer_grant.is_some() { page_permissions.insert(Permission::View); }
    Ok(PageAccess {
        space,
        document_id: document_id.into(),
        ancestor_ids,
        published: page_published,
        locked_by: page_locked_by,
        page_permissions,
        direct_viewer_grant,
    })
}
/// Checks the exact requested draft, not any other draft on the same page.
/// Publish ownership is an independent future adapter check; this API does not
/// authorize publication just because it authorizes shared draft editing.
async fn require_selected_draft_participation(conn:&mut AnyConnection,space:&SpaceAccess,document_id:&str,draft_id:&str,edit:bool)->Result<DraftParticipation,AppError> {
    let scoped: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3",
    )
    .bind(draft_id)
    .bind(document_id)
    .bind(space.space_id.as_str())
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    if scoped.is_none() {
        return Err(unavailable());
    }
    let participation = if space.recovery_audit_required.is_some() {
        DraftParticipation::RecoveryAuditRequired
    } else {
        draft_participation(
            conn,
            space.space_id.as_str(),
            document_id,
            Some(draft_id),
            &space.actor_id,
            edit,
        )
        .await?
        .ok_or_else(unavailable)?
    };
    Ok(participation)
}

pub async fn resolve_draft_access(
    conn: &mut AnyConnection,
    principal: Principal<'_>,
    space_id: &str,
    document_id: &str,
    draft_id: &str,
    operation: DraftOperation,
    purpose: Purpose<'_>,
) -> Result<DraftAccess, AppError> {
    let page = resolve_page_access(
        conn,
        principal,
        space_id,
        document_id,
        match operation {
            DraftOperation::Edit => PageOperation::Edit,
            DraftOperation::View => PageOperation::View,
            DraftOperation::InspectArchivedMetadata => PageOperation::InspectArchivedMetadata,
            DraftOperation::InspectTrashedMetadata => PageOperation::InspectTrashedMetadata,
        },
        purpose,
    )
    .await?;
    let participation=require_selected_draft_participation(conn,&page.space,document_id,draft_id,operation==DraftOperation::Edit).await?;
    Ok(DraftAccess {
        page,
        draft_id: draft_id.into(),
        participation,
    })
}

/// Resolves discussion authority for one exact active draft. Discussion is
/// deliberately independent from the content lock: the caller must retain the
/// page's current Comment permission and explicit draft participation, but
/// does not gain EditContent (or Comment) merely by sharing the draft.
pub async fn resolve_draft_comment_access(
    conn: &mut AnyConnection,
    principal: Principal<'_>,
    space_id: &str,
    document_id: &str,
    draft_id: &str,
    purpose: Purpose<'_>,
) -> Result<DraftAccess, AppError> {
    let page = resolve_page_access(
        conn,
        principal,
        space_id,
        document_id,
        PageOperation::Comment,
        purpose,
    )
    .await?;
    let participation = require_selected_draft_participation(
        conn,
        &page.space,
        document_id,
        draft_id,
        false,
    )
    .await?;
    Ok(DraftAccess {
        page,
        draft_id: draft_id.into(),
        participation,
    })
}

/// A method-aware owner operation, never recovery attribution or draft editing.
#[derive(Clone, Copy)]
pub(crate) enum DraftOwnerCredential { Read, Write }

pub(crate) struct OwnedDraftAccess {
    pub page: PageAccess,
    actor: String,
    space: String,
    document: String,
    draft: String,
}

pub(crate) async fn resolve_owned_draft_access(
    conn: &mut AnyConnection,
    auth: &crate::auth::middleware::AuthUser,
    space: &str,
    document: &str,
    draft: &str,
    credential: DraftOwnerCredential,
) -> Result<OwnedDraftAccess, AppError> {
    match credential {
        DraftOwnerCredential::Read => require_current_human_read(conn, auth).await?,
        DraftOwnerCredential::Write => require_current_human_preference_write(conn, auth).await?,
    };
    let operation = match credential {
        DraftOwnerCredential::Read => PageOperation::DraftOwnerRead,
        DraftOwnerCredential::Write => PageOperation::DraftOwnerMutation,
    };
    let page = resolve_page_access(
        conn, principal_from_auth(auth)?, space, document, operation, Purpose::Routine,
    ).await?;
    let owner: Option<String> = sqlx::query_scalar(
        "SELECT owner_id FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3",
    ).bind(draft).bind(document).bind(space).fetch_optional(&mut *conn)
        .await.map_err(AppError::internal)?;
    if owner.as_deref() != Some(auth.claims.sub.as_str()) {
        return Err(unavailable());
    }
    Ok(OwnedDraftAccess {
        page, actor: auth.claims.sub.clone(), space: space.into(),
        document: document.into(), draft: draft.into(),
    })
}

/// Data eligibility for the prospective selected draft share. No Principal,
/// AuthUser, token or session is manufactured for a recipient. Its account View
/// remains independent of this new draft participation; private ancestors cannot
/// be opened by a child draft share. This result grants no mutation authority.
pub(crate) async fn inspect_draft_share_recipient(
    conn: &mut AnyConnection,
    space: &str,
    document: &str,
    draft: &str,
    recipient: &str,
) -> Result<(), AppError> {
    let role: Option<String> = sqlx::query_scalar(
        "SELECT role FROM users WHERE id=$1 AND is_active=1",
    ).bind(recipient).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let role = role.ok_or_else(unavailable)?;
    let account = calculate_space_access(
        conn, recipient, role, None, space, Purpose::Routine, None,
        SpaceCredentialMode::Ordinary,
    ).await?;
    if !account.allows(Permission::View) { return Err(unavailable()); }
    let scoped: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3",
    ).bind(draft).bind(document).bind(space).fetch_optional(&mut *conn)
        .await.map_err(AppError::internal)?;
    if scoped.is_none() { return Err(unavailable()); }
    let mut next = Some(document.to_owned());
    let mut seen = BTreeSet::new();
    while let Some(id) = next {
        if !seen.insert(id.clone()) { return Err(invalid_policy()); }
        let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT parent_id,published_at FROM documents WHERE id=$1 AND space_id=$2",
        ).bind(&id).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        let (parent, published) = row.ok_or_else(unavailable)?;
        require_page_restriction(conn, space, &id, Some(recipient), false).await?;
        if id != document && published.is_none()
            && draft_participation(conn, space, &id, None, recipient, false).await?.is_none()
        { return Err(unavailable()); }
        // Only this selected target gains prospective viewer/editor participation.
        next = parent;
    }
    Ok(())
}

/// Post-delete proof consumes the unforgeable original owner-operation witness.
/// Only the removed target draft participation is intentionally unavailable.
/// Current target restrictions, every private ancestor, exact ancestry and actual
/// original account View/Write remain mandatory. Caller verifies full expected
/// raw target/source/cascade inventory before invoking this narrow final proof.
pub(crate) async fn verify_owned_draft_removed(
    conn: &mut AnyConnection,
    auth: &crate::auth::middleware::AuthUser,
    original: &OwnedDraftAccess,
) -> Result<(), AppError> {
    if original.actor != auth.claims.sub { return Err(unavailable()); }
    let present: Option<i64> = sqlx::query_scalar("SELECT 1 FROM document_drafts WHERE id=$1")
        .bind(&original.draft).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if present.is_some() { return Err(unavailable()); }
    let account = resolve_space_access_for_page_operation(
        conn, principal_from_auth(auth)?, &original.space, Purpose::Routine,
        Some(PageOperation::DraftOwnerMutation), SpaceCredentialMode::Ordinary,
    ).await?;
    if !account.allows(Permission::View) { return Err(unavailable()); }
    let mut next = Some(original.document.clone());
    let mut seen = BTreeSet::new();
    let mut ancestors = Vec::new();
    while let Some(id) = next {
        if !seen.insert(id.clone()) { return Err(invalid_policy()); }
        let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
            "SELECT parent_id,published_at FROM documents WHERE id=$1 AND space_id=$2",
        ).bind(&id).bind(&original.space).fetch_optional(&mut *conn)
            .await.map_err(AppError::internal)?;
        let (parent, published) = row.ok_or_else(unavailable)?;
        require_page_restriction(conn, &original.space, &id, Some(&original.actor), false).await?;
        if id == original.document {
            if published.is_some() != original.page.published { return Err(unavailable()); }
        } else {
            ancestors.push(id.clone());
            if published.is_none() && draft_participation(
                conn, &original.space, &id, None, &original.actor, false,
            ).await?.is_none() { return Err(unavailable()); }
        }
        next = parent;
    }
    if ancestors != original.page.ancestor_ids { return Err(unavailable()); }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{any::AnyPoolOptions, AnyPool};
    async fn fixture() -> AnyPool {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        for (id, role) in [
            ("owner", "member"),
            ("actor", "member"),
            ("admin", "admin"),
            ("other", "member"),
        ] {
            sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES ($1,$2,$1,$3)")
                .bind(id)
                .bind(format!("{id}@example.test"))
                .bind(role)
                .execute(&db)
                .await
                .unwrap();
        }
        for (id, visibility) in [
            ("space", "private"),
            ("public-space", "public"),
            ("foreign", "private"),
        ] {
            sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES ($1,$1,$1,$2,'owner')").bind(id).bind(visibility).execute(&db).await.unwrap();
        }
        for id in ["owner", "actor", "other"] {
            sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES ($1,'space',$1,'editor')").bind(id).execute(&db).await.unwrap();
        }
        for (id, space, parent, published) in [
            ("parent", "space", None, Some("2026-01-01")),
            ("page", "space", Some("parent"), Some("2026-01-01")),
            ("foreign-page", "foreign", None, Some("2026-01-01")),
        ] {
            sqlx::query("INSERT INTO documents(id,space_id,parent_id,title,content_path,created_by,last_updated_by,published_at) VALUES ($1,$2,$3,'Private title','','owner','owner',$4)").bind(id).bind(space).bind(parent).bind(published).execute(&db).await.unwrap();
        }
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES ('draft','page','space','owner','Private','old')").execute(&db).await.unwrap();
        db
    }
    fn access_result(result:Result<(),AppError>)->String {
        match result {Ok(())=>"allowed".into(),Err(error)=>format!("{error:?}")}
    }
    async fn batch_matches_single(db:&AnyPool,principal:Principal<'_>,targets:&BTreeSet<String>,operation:PageOperation) {
        let mut expected=std::collections::BTreeMap::new();
        for id in targets {expected.insert(id.clone(),access_result(resolve_page_access(&mut db.acquire().await.unwrap(),principal,"space",id,operation,Purpose::Routine).await.map(|_|())));}
        let result=resolve_page_batch(&mut db.acquire().await.unwrap(),principal,"space",targets,operation).await;
        for id in targets {
            let actual=match &result {Ok(batch)=>access_result(batch.require(id)),Err(error)=>format!("{error:?}")};
            assert_eq!(actual,expected[id],"target={id} operation={operation:?}");
        }
    }
    #[test]
    fn page_operation_budget_counts_distinct_union_and_accepts_exact_boundary() {
        let mut budget=PageTraversalBudget::default();
        for n in 0..MAX_PAGE_OPERATION_NODES {budget.visit(&format!("page-{n}")).unwrap();}
        for n in 0..MAX_PAGE_OPERATION_NODES {budget.visit(&format!("page-{n}")).unwrap();}
        assert_eq!(budget.len(),MAX_PAGE_OPERATION_NODES);
        assert!(matches!(budget.visit("one-new-identity"),Err(AppError::BadRequest(_))));
    }
    #[tokio::test]
    async fn deep_page_read_batch_retains_existing_valid_10001_chain_without_mutation_admission() {
        let db=fixture().await;
        let mut tx=db.begin().await.unwrap();
        for n in 0..10_001 {
            let id=format!("legacy-chain-{n:05}");
            let parent=(n>0).then(||format!("legacy-chain-{:05}",n-1));
            sqlx::query("INSERT INTO documents(id,space_id,parent_id,title,content_path,created_by,last_updated_by,published_at) VALUES($1,'space',$2,'Existing retained page','','owner','owner','published')").bind(id).bind(parent).execute(&mut *tx).await.unwrap();
        }
        tx.commit().await.unwrap();
        let targets=BTreeSet::from(["legacy-chain-10000".to_owned()]);
        for operation in [PageOperation::View,PageOperation::InspectMovePreviewFromMutation] {
            let batch=resolve_page_batch(&mut db.acquire().await.unwrap(),user("owner"),"space",&targets,operation).await.unwrap();
            batch.require_all().unwrap();assert_eq!(batch.authorized_nodes().len(),10_001);
        }
        assert!(matches!(resolve_page_batch(&mut db.acquire().await.unwrap(),user("owner"),"space",&targets,PageOperation::Move).await,Err(AppError::BadRequest(_))));
    }
    #[tokio::test]
    async fn deep_page_batch_matches_single_roles_private_targets_ancestor_locks_and_policies() {
        let db=fixture().await;
        let targets=BTreeSet::from(["parent".to_owned(),"page".to_owned(),"foreign-page".to_owned(),"missing".to_owned()]);
        for actor in ["owner","actor","admin","other"] {
            for operation in [PageOperation::View,PageOperation::Edit,PageOperation::Move,PageOperation::InspectMovePreviewFromMutation] {
                batch_matches_single(&db,user(actor),&targets,operation).await;
            }
        }
        for statement in [
            "UPDATE documents SET published_at=NULL WHERE id='page'",
            "INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('draft','actor','viewer','owner')",
            "UPDATE document_draft_access SET role='editor'",
            "UPDATE documents SET locked=1,locked_by='other' WHERE id='parent'",
            "UPDATE documents SET locked=1,locked_by='other' WHERE id='page'",
            "UPDATE documents SET locked=0,locked_by=NULL",
            "UPDATE documents SET published_at=NULL WHERE id='parent'",
            "INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('parent-draft','parent','space','owner','Parent','old')",
            "INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('parent-draft','actor','viewer','owner')",
            "INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('parent','space','restricted','batch-policy','owner')",
        ] {
            sqlx::query(statement).execute(&db).await.unwrap();
            for operation in [PageOperation::View,PageOperation::Edit,PageOperation::Move,PageOperation::InspectMovePreviewFromMutation] {
                batch_matches_single(&db,user("actor"),&targets,operation).await;
            }
        }
    }
    #[tokio::test]
    async fn deep_page_batch_preserves_custom_roles_key_ceilings_and_fresh_original_credentials() {
        let db=fixture().await;let targets=BTreeSet::from(["page".to_owned()]);
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('custom:batch','space','Batch','[\"view\",\"move_content\"]','owner')").execute(&db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='custom:batch' WHERE user_id='actor'").execute(&db).await.unwrap();
        for operation in [PageOperation::View,PageOperation::Edit,PageOperation::Move,PageOperation::InspectMovePreviewFromMutation] {batch_matches_single(&db,user("actor"),&targets,operation).await;}
        for (id,scope) in [("batch-read","read"),("batch-write","write"),("batch-admin","admin")] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'owner',$1,$1,'batch',$2)").bind(id).bind(format!("[\"{scope}\"]")).execute(&db).await.unwrap();
            let key=Principal::UserKey{key_id:id,user_id:"owner",token_expires_at:None};
            for operation in [PageOperation::View,PageOperation::Edit,PageOperation::Move,PageOperation::InspectMovePreviewFromMutation] {batch_matches_single(&db,key,&targets,operation).await;}
        }
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('batch-session','owner','fictional','2999-01-01T00:00:00Z')").execute(&db).await.unwrap();
        let original=Principal::User{user_id:"owner",session_id:Some("batch-session"),token_expires_at:Some(chrono::Utc::now().timestamp()+3600)};
        PAGE_BATCH_FINAL_SQL.with(|slot|*slot.borrow_mut()=Some("DELETE FROM sessions WHERE id='batch-session'"));
        assert!(matches!(resolve_page_batch(&mut db.acquire().await.unwrap(),original,"space",&targets,PageOperation::View).await,Err(AppError::Unauthorized(_))));
        batch_matches_single(&db,original,&targets,PageOperation::View).await;
        PAGE_BATCH_FINAL_SQL.with(|slot|*slot.borrow_mut()=Some("UPDATE documents SET space_id='foreign' WHERE id='page'"));
        assert!(matches!(resolve_page_batch(&mut db.acquire().await.unwrap(),user("owner"),"space",&targets,PageOperation::View).await,Err(AppError::NotFound(_))));
        batch_matches_single(&db,user("owner"),&targets,PageOperation::View).await;
    }
    fn user(id: &str) -> Principal<'_> {
        Principal::User {
            user_id: id,
            session_id: None,
            token_expires_at: None,
        }
    }
    async fn space(db: &AnyPool, id: &str) -> Result<SpaceAccess, AppError> {
        resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user(id),
            "space",
            Purpose::Routine,
        )
        .await
    }
    async fn page(db: &AnyPool, id: &str, op: PageOperation) -> Result<PageAccess, AppError> {
        resolve_page_access(
            &mut db.acquire().await.unwrap(),
            user(id),
            "space",
            "page",
            op,
            Purpose::Routine,
        )
        .await
    }
    async fn group(db: &AnyPool, global: &str) {
        sqlx::query("INSERT INTO groups(id,name) VALUES ('g','Group')")
            .execute(db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES ('gm','g','actor')")
            .execute(db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_global_roles(group_id,role_id) VALUES ('g',$1)")
            .bind(global)
            .execute(db)
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn archived_inspect_metadata_retains_view_policy_without_body_or_mutation_authority() {
        let db = fixture().await;
        sqlx::query("UPDATE documents SET archived_at='2026-01-01' WHERE id IN ('page','parent')")
            .execute(&db)
            .await
            .unwrap();
        page(&db, "actor", PageOperation::InspectMetadata)
            .await
            .unwrap();
        for operation in [
            PageOperation::View,
            PageOperation::Edit,
            PageOperation::Comment,
        ] {
            assert!(page(&db, "actor", operation).await.is_err());
        }
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='parent'")
            .execute(&db)
            .await
            .unwrap();
        assert!(page(&db, "actor", PageOperation::InspectMetadata)
            .await
            .is_err());
        assert!(page(&db, "admin", PageOperation::InspectMetadata)
            .await
            .is_err());
        sqlx::query("UPDATE documents SET published_at='published',deleted_at='2026-01-01' WHERE id='parent'").execute(&db).await.unwrap();
        assert!(page(&db, "owner", PageOperation::InspectMetadata)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn mixed_roles_union_legacy_permissions_without_global_expansion() {
        let db = fixture().await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES ('custom:edit','space','Editor','[\"view\",\"edit_content\"]','owner')").execute(&db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='custom:edit' WHERE user_id='actor'")
            .execute(&db)
            .await
            .unwrap();
        group(&db, "viewer").await;
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES ('sg','space','g','commenter')").execute(&db).await.unwrap();
        let access = space(&db, "actor").await.unwrap();
        for p in [
            Permission::View,
            Permission::Comment,
            Permission::EditContent,
            Permission::ManageFolders,
            Permission::ManageTemplates,
        ] {
            assert!(access.allows(p));
        }
        assert!(!access.allows(Permission::ManageSpace));
        assert!(!access.instance_admin);
        assert_eq!(access.roles.len(), 3);
        assert!(matches!(access.roles[0].source, RoleSource::Direct));
        sqlx::query("DELETE FROM space_members WHERE user_id='actor'")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("DELETE FROM space_group_permissions")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("UPDATE group_global_roles SET role_id='contributor'")
            .execute(&db)
            .await
            .unwrap();
        let global = space(&db, "actor").await.unwrap();
        assert!(global.allows(Permission::EditContent));
        for p in [
            Permission::ManageFolders,
            Permission::ManageTemplates,
            Permission::DeleteContent,
        ] {
            assert!(!global.allows(p));
        }
        sqlx::query("UPDATE group_global_roles SET role_id='admin'")
            .execute(&db)
            .await
            .unwrap();
        let admin = space(&db, "actor").await.unwrap();
        assert!(!admin.instance_admin);
        assert!(!admin.allows(Permission::ManageSpace));
        assert!(admin.allows(Permission::ManageMembers));
    }
    #[tokio::test]
    async fn active_account_actual_scope_and_current_session_are_required() {
        let db = fixture().await;
        assert!(matches!(
            space(&db, "missing").await,
            Err(AppError::Unauthorized(_))
        ));
        sqlx::query("UPDATE users SET is_active=0 WHERE id='admin'")
            .execute(&db)
            .await
            .unwrap();
        assert!(matches!(
            space(&db, "admin").await,
            Err(AppError::Unauthorized(_))
        ));
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user("actor"),
            "missing",
            Purpose::Routine
        )
        .await
        .is_err());
        sqlx::query("UPDATE users SET is_active=1 WHERE id='admin'")
            .execute(&db)
            .await
            .unwrap();
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user("admin"),
            "missing",
            Purpose::AdminRecovery { reason: "Recover" }
        )
        .await
        .is_err());
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user("missing"),
            "public-space",
            Purpose::Routine
        )
        .await
        .is_err());
        let principal = Principal::User {
            user_id: "actor",
            session_id: Some("session"),
            token_expires_at: Some(i64::MAX),
        };
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            principal,
            "space",
            Purpose::Routine
        )
        .await
        .is_err());
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES ('session','actor','test','2999-01-01T00:00:00Z')").execute(&db).await.unwrap();
        resolve_space_access(
            &mut db.acquire().await.unwrap(),
            principal,
            "space",
            Purpose::Routine,
        )
        .await
        .unwrap();
        let expired_token = Principal::User {
            user_id: "actor",
            session_id: Some("session"),
            token_expires_at: Some(1),
        };
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            expired_token,
            "space",
            Purpose::Routine
        )
        .await
        .is_err());
        sqlx::query("UPDATE sessions SET expires_at='2000-01-01T00:00:00Z'")
            .execute(&db)
            .await
            .unwrap();
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            principal,
            "space",
            Purpose::Routine
        )
        .await
        .is_err());
        sqlx::query("UPDATE sessions SET expires_at='malformed'")
            .execute(&db)
            .await
            .unwrap();
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            principal,
            "space",
            Purpose::Routine
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn routine_blocks_and_private_draft_rules_apply_even_to_instance_admin() {
        let db = fixture().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='page'")
            .execute(&db)
            .await
            .unwrap();
        assert!(page(&db, "actor", PageOperation::View).await.is_err());
        assert!(page(&db, "admin", PageOperation::View).await.is_err());
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ('draft','actor','viewer','owner')").execute(&db).await.unwrap();
        page(&db, "actor", PageOperation::View).await.unwrap();
        assert!(page(&db, "actor", PageOperation::Edit).await.is_err());
        sqlx::query("UPDATE document_draft_access SET role='editor'")
            .execute(&db)
            .await
            .unwrap();
        page(&db, "actor", PageOperation::Edit).await.unwrap();
        page(&db, "owner", PageOperation::Edit).await.unwrap();
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES ('space','admin','owner')").execute(&db).await.unwrap();
        assert!(space(&db, "admin").await.is_err());
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES ('space','actor','owner')").execute(&db).await.unwrap();
        assert!(page(&db, "actor", PageOperation::View).await.is_err());
        let recovery = resolve_draft_access(
            &mut db.acquire().await.unwrap(),
            user("admin"),
            "space",
            "page",
            "draft",
            DraftOperation::View,
            Purpose::AdminRecovery {
                reason: "Recover lost owner access",
            },
        )
        .await
        .unwrap();
        assert_eq!(
            recovery.participation,
            DraftParticipation::RecoveryAuditRequired
        );
        assert!(recovery.page.space.recovery_audit_required.is_some());
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user("actor"),
            "space",
            Purpose::AdminRecovery { reason: "Recovery" }
        )
        .await
        .is_err());
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user("admin"),
            "space",
            Purpose::AdminRecovery { reason: " " }
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn exact_draft_identity_lifecycle_locks_and_ancestor_fail_closed() {
        let db = fixture().await;
        assert!(resolve_draft_access(
            &mut db.acquire().await.unwrap(),
            user("owner"),
            "space",
            "page",
            "missing",
            DraftOperation::Edit,
            Purpose::Routine
        )
        .await
        .is_err());
        assert!(resolve_draft_access(
            &mut db.acquire().await.unwrap(),
            user("owner"),
            "foreign",
            "foreign-page",
            "draft",
            DraftOperation::Edit,
            Purpose::Routine
        )
        .await
        .is_err());
        sqlx::query("UPDATE documents SET locked=1,locked_by='other' WHERE id='page'")
            .execute(&db)
            .await
            .unwrap();
        assert!(page(&db, "owner", PageOperation::Edit).await.is_err());
        page(&db, "owner", PageOperation::View).await.unwrap();
        sqlx::query("UPDATE documents SET locked=0 WHERE id='page'")
            .execute(&db)
            .await
            .unwrap();
        for query in [
            "UPDATE documents SET deleted_at='old' WHERE id='parent'",
            "UPDATE documents SET archived_at='old' WHERE id='parent'",
            "UPDATE documents SET parent_id='foreign-page' WHERE id='page'",
            "UPDATE documents SET parent_id='page' WHERE id='parent'",
        ] {
            let mut tx = db.begin().await.unwrap();
            sqlx::query(query).execute(&mut *tx).await.unwrap();
            assert!(resolve_page_access(
                &mut tx,
                user("owner"),
                "space",
                "page",
                PageOperation::View,
                Purpose::Routine
            )
            .await
            .is_err());
            tx.rollback().await.unwrap();
        }
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='parent'")
            .execute(&db)
            .await
            .unwrap();
        assert!(page(&db, "actor", PageOperation::View).await.is_err());
    }
    #[tokio::test]
    async fn corrupt_global_and_custom_permission_json_never_becomes_authority() {
        let db = fixture().await;
        group(&db, "contributor").await;
        sqlx::query("UPDATE global_roles SET permissions='[\"edit_content\",\"invented\"]' WHERE id='contributor'").execute(&db).await.unwrap();
        assert!(space(&db, "actor").await.is_err());
        sqlx::query("DELETE FROM group_global_roles")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES ('custom:bad','space','Invalid','not-json','owner')").execute(&db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='custom:bad' WHERE user_id='actor'")
            .execute(&db)
            .await
            .unwrap();
        assert!(space(&db, "actor").await.is_err());
    }
    #[tokio::test]
    async fn service_principals_fail_closed_and_resolver_does_not_mutate_database() {
        let db = fixture().await;
        let before: i64 = sqlx::query_scalar("SELECT total_changes()")
            .fetch_one(&db)
            .await
            .unwrap();
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            Principal::ServiceKey { key_id: "key" },
            "space",
            Purpose::Routine
        )
        .await
        .is_err());
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user("service:key"),
            "public-space",
            Purpose::Routine
        )
        .await
        .is_err());
        page(&db, "actor", PageOperation::View).await.unwrap();
        let after: i64 = sqlx::query_scalar("SELECT total_changes()")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(before, after);
    }
    #[tokio::test]
    async fn caller_transaction_observes_uncommitted_revocation_without_pool_reentry() {
        let db = fixture().await;
        let mut tx = db.begin().await.unwrap();
        resolve_page_access(
            &mut tx,
            user("actor"),
            "space",
            "page",
            PageOperation::Edit,
            Purpose::Routine,
        )
        .await
        .unwrap();
        sqlx::query("UPDATE users SET is_active=0 WHERE id='actor'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(matches!(
            resolve_space_access(&mut tx, user("actor"), "space", Purpose::Routine).await,
            Err(AppError::Unauthorized(_))
        ));
        tx.rollback().await.unwrap();
        page(&db, "actor", PageOperation::Edit).await.unwrap();
    }

    #[tokio::test]
    async fn ancestor_view_access_does_not_inherit_content_locks_or_draft_edit_requirements() {
        let db = fixture().await;
        sqlx::query("UPDATE documents SET locked=1,locked_by='other' WHERE id='parent'")
            .execute(&db)
            .await
            .unwrap();
        page(&db, "actor", PageOperation::Edit).await.unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id IN ('page','parent')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES ('parent-draft','parent','space','owner','Private parent','old')").execute(&db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ('draft','actor','editor','owner'),('parent-draft','actor','viewer','owner')").execute(&db).await.unwrap();
        page(&db, "actor", PageOperation::Edit).await.unwrap();
        sqlx::query("DELETE FROM document_draft_access WHERE draft_id='parent-draft'")
            .execute(&db)
            .await
            .unwrap();
        assert!(page(&db, "actor", PageOperation::Edit).await.is_err());
    }
    async fn key(db: &AnyPool, owner: Option<&str>, service: bool, scopes: Option<&str>) {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES ('key',$1,'Test key','test-hash','test',$2,$3)").bind(owner).bind(if service {1} else {0}).bind(scopes).execute(db).await.unwrap();
    }
    fn user_key(id: &str) -> Principal<'_> {
        Principal::UserKey {
            key_id: "key",
            user_id: id,
            token_expires_at: Some(i64::MAX),
        }
    }
    #[tokio::test]
    async fn current_user_key_scopes_are_ceilings_not_instance_admin_or_role_grants() {
        let db = fixture().await;
        key(&db, Some("actor"), false, Some("[\"read\"]")).await;
        let access = resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user_key("actor"),
            "space",
            Purpose::Routine,
        )
        .await
        .unwrap();
        assert!(access.allows(Permission::View));
        assert!(!access.allows(Permission::EditContent));
        assert!(!access.instance_admin);
        sqlx::query("UPDATE api_keys SET scopes='[\"write\"]'")
            .execute(&db)
            .await
            .unwrap();
        let access = resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user_key("actor"),
            "space",
            Purpose::Routine,
        )
        .await
        .unwrap();
        assert!(!access.allows(Permission::View));
        assert!(access.allows(Permission::EditContent));
        sqlx::query("UPDATE api_keys SET scopes='[\"admin\"]'")
            .execute(&db)
            .await
            .unwrap();
        let access = resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user_key("actor"),
            "space",
            Purpose::Routine,
        )
        .await
        .unwrap();
        assert!(!access.instance_admin);
        assert!(!access.allows(Permission::ManageSpace));
        sqlx::query("UPDATE api_keys SET scopes=NULL")
            .execute(&db)
            .await
            .unwrap();
        let access = resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user_key("actor"),
            "space",
            Purpose::Routine,
        )
        .await
        .unwrap();
        assert!(access.allows(Permission::View));
        assert!(access.allows(Permission::EditContent));
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='page'")
            .execute(&db)
            .await
            .unwrap();
        assert!(resolve_draft_access(
            &mut db.acquire().await.unwrap(),
            user_key("actor"),
            "space",
            "page",
            "draft",
            DraftOperation::View,
            Purpose::Routine
        )
        .await
        .is_err());
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ('draft','actor','editor','owner')").execute(&db).await.unwrap();
        resolve_draft_access(
            &mut db.acquire().await.unwrap(),
            user_key("actor"),
            "space",
            "page",
            "draft",
            DraftOperation::Edit,
            Purpose::Routine,
        )
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn current_key_owner_expiry_activity_scope_and_deletion_are_revalidated() {
        let db = fixture().await;
        key(&db, Some("actor"), false, Some("[\"read\",\"write\"]")).await;
        for statement in [
            "UPDATE api_keys SET user_id='other'",
            "UPDATE users SET is_active=0 WHERE id='actor'",
            "UPDATE api_keys SET expires_at='2000-01-01T00:00:00Z'",
            "UPDATE api_keys SET expires_at='malformed'",
            "UPDATE api_keys SET scopes='[\"admin\",\"unknown\"]'",
            "UPDATE api_keys SET scopes='{'",
            "DELETE FROM api_keys",
        ] {
            let mut tx = db.begin().await.unwrap();
            sqlx::query(statement).execute(&mut *tx).await.unwrap();
            assert!(
                resolve_space_access(&mut tx, user_key("actor"), "space", Purpose::Routine)
                    .await
                    .is_err(),
                "{statement}"
            );
            tx.rollback().await.unwrap();
        }
        let expired = Principal::UserKey {
            key_id: "key",
            user_id: "actor",
            token_expires_at: Some(1),
        };
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            expired,
            "space",
            Purpose::Routine
        )
        .await
        .is_err());
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user_key("missing"),
            "space",
            Purpose::Routine
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn service_capability_is_current_explicit_and_never_a_human_or_private_participant() {
        let db = fixture().await;
        key(&db, None, true, Some("[\"admin\"]")).await;
        let CurrentKeyCapability::Service(cap) =
            resolve_current_key(&mut db.acquire().await.unwrap(), "key")
                .await
                .unwrap()
        else {
            panic!("service became user")
        };
        assert_eq!(cap.key_id, "key");
        assert!(cap.scopes.contains(&ApiKeyScope::Admin));
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            Principal::ServiceKey { key_id: "key" },
            "space",
            Purpose::Routine
        )
        .await
        .is_err());
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user_key("actor"),
            "space",
            Purpose::Routine
        )
        .await
        .is_err());
        for statement in [
            "UPDATE api_keys SET scopes=NULL",
            "UPDATE api_keys SET scopes='[\"read\",\"write\"]'",
            "UPDATE api_keys SET scopes='[\"admin\",\"unknown\"]'",
            "UPDATE api_keys SET scopes='null'",
            "UPDATE api_keys SET expires_at='2000-01-01T00:00:00Z'",
            "DELETE FROM api_keys",
        ] {
            let mut tx = db.begin().await.unwrap();
            sqlx::query(statement).execute(&mut *tx).await.unwrap();
            assert!(
                resolve_current_key(&mut tx, "key").await.is_err(),
                "{statement}"
            );
            tx.rollback().await.unwrap();
        }
    }
    #[tokio::test]
    async fn bearer_adapter_preserves_real_session_and_token_expiry_and_rejects_bad_key_identity() {
        let db = fixture().await;
        let mut auth = crate::spaces::properties::tests::test_auth("actor");
        auth.claims.sid = "session".into();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES ('session','actor','test','2999-01-01T00:00:00Z')").execute(&db).await.unwrap();
        let principal = principal_from_auth(&auth).unwrap();
        resolve_space_access(
            &mut db.acquire().await.unwrap(),
            principal,
            "space",
            Purpose::Routine,
        )
        .await
        .unwrap();
        auth.claims.exp = 1;
        assert!(principal_from_auth(&auth).is_err());
        auth.claims.exp = i64::MAX;
        auth.api_key_scopes = Some(vec!["admin".into()]);
        assert!(principal_from_auth(&auth).is_err());
        auth.claims.sid = "api-key:key".into();
        key(&db, Some("actor"), false, Some("[\"read\"]")).await;
        let principal = principal_from_auth(&auth).unwrap();
        let access = resolve_space_access(
            &mut db.acquire().await.unwrap(),
            principal,
            "space",
            Purpose::Routine,
        )
        .await
        .unwrap();
        assert!(!access.instance_admin);
        assert!(!access.allows(Permission::EditContent));
        auth.claims.sid = "service:key".into();
        assert!(principal_from_auth(&auth).is_err());
        auth.claims.sub = "service:key".into();
        assert!(matches!(
            principal_from_auth(&auth).unwrap(),
            Principal::ServiceKey { .. }
        ));
    }
    #[tokio::test]
    async fn user_key_recovery_requires_current_admin_scope_and_actual_instance_admin() {
        let db = fixture().await;
        key(&db, Some("admin"), false, Some("[\"read\"]")).await;
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user_key("admin"),
            "space",
            Purpose::AdminRecovery { reason: "Recover" }
        )
        .await
        .is_err());
        sqlx::query("UPDATE api_keys SET scopes='[\"admin\"]'")
            .execute(&db)
            .await
            .unwrap();
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user_key("admin"),
            "space",
            Purpose::AdminRecovery { reason: "Recover" }
        )
        .await
        .unwrap()
        .recovery_audit_required
        .is_some());
        sqlx::query("UPDATE users SET role='member' WHERE id='admin'")
            .execute(&db)
            .await
            .unwrap();
        assert!(resolve_space_access(
            &mut db.acquire().await.unwrap(),
            user_key("admin"),
            "space",
            Purpose::AdminRecovery { reason: "Recover" }
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn acknowledgment_notification_keeps_view_permission_and_actual_write_key_ceiling() {
        let db = fixture().await;
        key(&db, Some("actor"), false, Some("[\"write\"]")).await;
        let database = &db;
        let ack = move |scope| async move {
            resolve_page_access(
                &mut database.acquire().await.unwrap(),
                user_key("actor"),
                "space",
                "page",
                scope,
                Purpose::Routine,
            )
            .await
        };
        assert!(ack(PageOperation::AcknowledgeNotification).await.is_ok());
        assert!(ack(PageOperation::View).await.is_err());
        assert!(ack(PageOperation::InspectMetadata).await.is_err());
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]'")
            .execute(&db)
            .await
            .unwrap();
        assert!(ack(PageOperation::AcknowledgeNotification).await.is_err());
        assert!(ack(PageOperation::View).await.is_ok());
        for scope in ["{", "null", "[\"write\",\"unknown\"]"] {
            sqlx::query("UPDATE api_keys SET scopes=$1")
                .bind(scope)
                .execute(&db)
                .await
                .unwrap();
            assert!(ack(PageOperation::AcknowledgeNotification).await.is_err());
        }
        sqlx::query("UPDATE api_keys SET scopes='[\"write\"]'")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='page'")
            .execute(&db)
            .await
            .unwrap();
        assert!(ack(PageOperation::AcknowledgeNotification).await.is_err());
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('draft','actor','viewer','owner')").execute(&db).await.unwrap();
        assert!(ack(PageOperation::AcknowledgeNotification).await.is_ok());
        sqlx::query("DELETE FROM document_draft_access")
            .execute(&db)
            .await
            .unwrap();
        assert!(ack(PageOperation::AcknowledgeNotification).await.is_err());
        sqlx::query("UPDATE documents SET published_at='published' WHERE id='page'")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("UPDATE api_keys SET user_id='other'")
            .execute(&db)
            .await
            .unwrap();
        assert!(ack(PageOperation::AcknowledgeNotification).await.is_err());
        sqlx::query("UPDATE api_keys SET user_id='actor',expires_at='2000-01-01T00:00:00Z'")
            .execute(&db)
            .await
            .unwrap();
        assert!(ack(PageOperation::AcknowledgeNotification).await.is_err());
        sqlx::query("UPDATE api_keys SET expires_at=NULL")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("UPDATE users SET is_active=0 WHERE id='actor'")
            .execute(&db)
            .await
            .unwrap();
        assert!(ack(PageOperation::AcknowledgeNotification).await.is_err());
        sqlx::query("UPDATE users SET is_active=1 WHERE id='actor'")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("DELETE FROM api_keys")
            .execute(&db)
            .await
            .unwrap();
        assert!(ack(PageOperation::AcknowledgeNotification).await.is_err());
    }
}

#[cfg(test)]
mod restriction_tests {
    use super::*;
    use crate::spaces::routes::catalog_tests::{auth, catalog_state, key};
    use crate::{auth::middleware::AuthUser, AppState};
    async fn fixture(wal: bool) -> AppState {
        let state = catalog_state(wal).await;
        sqlx::query("INSERT INTO documents(id,space_id,parent_id,title,content_path,created_by,last_updated_by,published_at) VALUES('restriction-parent','space-a',NULL,'Parent','','owner-a','owner-a','old'),('restriction-page','space-a','restriction-parent','Page','','owner-a','owner-a','old')").execute(&state.db).await.unwrap();
        state
    }
    async fn policy(state: &AppState, page: &str, mode: &str) {
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES($1,'space-a',$2,'revision-1','owner-a')").bind(page).bind(mode).execute(&state.db).await.unwrap();
    }
    async fn allow(state: &AppState, page: &str, user: Option<&str>, group: Option<&str>) {
        sqlx::query("INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,group_id,created_by) VALUES($1,$2,'space-a',$3,$4,'owner-a')").bind(uuid::Uuid::new_v4().to_string()).bind(page).bind(user).bind(group).execute(&state.db).await.unwrap();
    }
    async fn page(
        state: &AppState,
        auth: &AuthUser,
        op: PageOperation,
    ) -> Result<PageAccess, AppError> {
        resolve_page_access(
            &mut state.db.acquire().await.unwrap(),
            principal_from_auth(auth)?,
            "space-a",
            "restriction-page",
            op,
            Purpose::Routine,
        )
        .await
    }
    #[tokio::test]
    async fn restriction_inherited_empty_user_and_group_intersect_every_ancestor() {
        let state = fixture(false).await;
        let actor = auth("viewer-a");
        page(&state, &actor, PageOperation::View).await.unwrap();
        policy(&state, "restriction-page", "inherited").await;
        page(&state, &actor, PageOperation::View).await.unwrap();
        sqlx::query("UPDATE document_access_policies SET mode='restricted'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(page(&state, &actor, PageOperation::View).await.is_err());
        allow(&state, "restriction-page", Some("viewer-a"), None).await;
        page(&state, &actor, PageOperation::View).await.unwrap();
        policy(&state, "restriction-parent", "restricted").await;
        assert!(page(&state, &actor, PageOperation::View).await.is_err());
        sqlx::query("INSERT INTO groups(id,name) VALUES('restriction-group','Audience'); INSERT INTO group_members(id,group_id,user_id) VALUES('restriction-member','restriction-group','viewer-a')").execute(&state.db).await.unwrap();
        allow(
            &state,
            "restriction-parent",
            None,
            Some("restriction-group"),
        )
        .await;
        page(&state, &actor, PageOperation::View).await.unwrap();
        sqlx::query("DELETE FROM group_members WHERE id='restriction-member'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(page(&state, &actor, PageOperation::View).await.is_err());
    }
    #[tokio::test]
    async fn restriction_creator_admin_and_recovery_remain_distinct() {
        let state = fixture(false).await;
        policy(&state, "restriction-parent", "restricted").await;
        assert!(page(&state, &auth("owner-a"), PageOperation::View)
            .await
            .is_err());
        sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(page(&state, &auth("owner-a"), PageOperation::View)
            .await
            .is_err());
        let actor = auth("owner-a");
        let recovered = resolve_page_access(
            &mut state.db.acquire().await.unwrap(),
            principal_from_auth(&actor).unwrap(),
            "space-a",
            "restriction-page",
            PageOperation::View,
            Purpose::AdminRecovery {
                reason: "explicit recovery",
            },
        )
        .await
        .unwrap();
        assert!(recovered.space.recovery_audit_required.is_some());
        // Existing explicit recovery keeps its typed audit obligation even for private ancestry; no routine read route receives it.
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='restriction-parent'")
            .execute(&state.db)
            .await
            .unwrap();
        let private_recovery = resolve_page_access(
            &mut state.db.acquire().await.unwrap(),
            principal_from_auth(&actor).unwrap(),
            "space-a",
            "restriction-page",
            PageOperation::View,
            Purpose::AdminRecovery {
                reason: "explicit recovery",
            },
        )
        .await
        .unwrap();
        assert!(private_recovery.space.recovery_audit_required.is_some());
        assert!(page(&state, &actor, PageOperation::View).await.is_err());
    }
    #[tokio::test]
    async fn restriction_allowlist_never_grants_space_private_or_block_authority() {
        let state = fixture(false).await;
        policy(&state, "restriction-page", "restricted").await;
        allow(&state, "restriction-page", Some("viewer-a"), None).await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='restriction-page'; INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('restriction-draft','restriction-page','space-a','owner-a','Draft','PRIVATE','old')").execute(&state.db).await.unwrap();
        let actor = auth("viewer-a");
        assert!(page(&state, &actor, PageOperation::View).await.is_err());
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('restriction-draft','viewer-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        page(&state, &actor, PageOperation::View).await.unwrap();
        assert!(page(&state, &actor, PageOperation::Edit).await.is_err());
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')").execute(&state.db).await.unwrap();
        assert!(page(&state, &actor, PageOperation::View).await.is_err());
    }
    #[tokio::test]
    async fn restriction_current_keys_sessions_and_lock_contract_unchanged() {
        let state = fixture(false).await;
        policy(&state, "restriction-page", "restricted").await;
        allow(&state, "restriction-page", Some("editor-a"), None).await;
        let write = key(&state, "restriction-write", Some("editor-a"), "[\"write\"]").await;
        page(&state, &write, PageOperation::Edit).await.unwrap();
        page(&state, &write, PageOperation::ReferenceFromMutation)
            .await
            .unwrap();
        assert!(page(&state, &write, PageOperation::View).await.is_err());
        let read = key(&state, "restriction-read", Some("editor-a"), "[\"read\"]").await;
        page(&state, &read, PageOperation::View).await.unwrap();
        assert!(page(&state, &read, PageOperation::Edit).await.is_err());
        sqlx::query(
            "UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='restriction-page'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        page(&state, &read, PageOperation::View).await.unwrap();
        assert!(page(&state, &write, PageOperation::Edit).await.is_err());
        let service = key(&state, "restriction-service", None, "[\"admin\"]").await;
        assert!(page(&state, &service, PageOperation::View).await.is_err());
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='restriction-write'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(page(&state, &write, PageOperation::ReferenceFromMutation)
            .await
            .is_err());
        sqlx::query("DELETE FROM sessions WHERE id='metadata-editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(page(&state, &auth("editor-a"), PageOperation::View)
            .await
            .is_err());
        sqlx::query("UPDATE users SET is_active=0 WHERE id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(page(&state, &read, PageOperation::View).await.is_err());
    }
    #[tokio::test]
    async fn restriction_corrupt_policy_allowlist_and_hierarchy_fail_closed() {
        for change in [
            "UPDATE document_access_policies SET space_id='space-b'",
            "UPDATE document_access_policies SET revision=''",
            "UPDATE document_access_policies SET mode='unknown'",
            "UPDATE document_access_allowlist SET space_id='space-b'",
            "UPDATE document_access_allowlist SET user_id='missing-user'",
            "UPDATE document_access_allowlist SET user_id=NULL,group_id='missing-group'",
            "UPDATE document_access_allowlist SET user_id=NULL,group_id=NULL",
            "DELETE FROM document_access_policies",
            "UPDATE documents SET parent_id='restriction-page' WHERE id='restriction-parent'",
            "UPDATE documents SET space_id='space-b' WHERE id='restriction-parent'",
        ] {
            let state = fixture(false).await;
            policy(&state, "restriction-page", "restricted").await;
            allow(&state, "restriction-page", Some("viewer-a"), None).await;
            let mut conn = state.db.acquire().await.unwrap();
            sqlx::query("PRAGMA foreign_keys=OFF")
                .execute(&mut *conn)
                .await
                .unwrap();
            sqlx::query("PRAGMA ignore_check_constraints=ON")
                .execute(&mut *conn)
                .await
                .unwrap();
            sqlx::query(change).execute(&mut *conn).await.unwrap();
            sqlx::query("PRAGMA ignore_check_constraints=OFF")
                .execute(&mut *conn)
                .await
                .unwrap();
            sqlx::query("PRAGMA foreign_keys=ON")
                .execute(&mut *conn)
                .await
                .unwrap();
            drop(conn);
            assert!(
                page(&state, &auth("viewer-a"), PageOperation::View)
                    .await
                    .is_err(),
                "{change}"
            );
        }
    }
    #[tokio::test]
    async fn restriction_database_xor_and_scoped_binding_constraints() {
        let state = fixture(false).await;
        policy(&state, "restriction-page", "restricted").await;
        for sql in [
            "INSERT INTO document_access_allowlist(id,document_id,space_id,created_by) VALUES('bad','restriction-page','space-a','owner-a')",
            "INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('bad','restriction-page','space-b','viewer-a','owner-a')",
            "INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('bad','restriction-page','space-a','missing','owner-a')",
        ] { assert!(sqlx::query(sql).execute(&state.db).await.is_err(),"{sql}"); }
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_access_allowlist")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
    #[tokio::test]
    async fn restriction_autocommit_observes_external_wal_group_policy_and_session_revocation() {
        for change in ["DELETE FROM document_access_allowlist", "UPDATE document_access_policies SET mode='restricted' WHERE document_id='restriction-parent'", "DELETE FROM sessions WHERE id='metadata-viewer-a'"] {
            let state=fixture(true).await;
            policy(&state,"restriction-page","restricted").await;
            policy(&state,"restriction-parent","inherited").await;
            allow(&state,"restriction-page",Some("viewer-a"),None).await;
            let actor=auth("viewer-a");
            let mut conn=state.db.acquire().await.unwrap();
            resolve_page_access(&mut conn,principal_from_auth(&actor).unwrap(),"space-a","restriction-page",PageOperation::View,Purpose::Routine).await.unwrap();
            sqlx::query(change).execute(&state.db).await.unwrap();
            assert!(resolve_page_access(&mut conn,principal_from_auth(&actor).unwrap(),"space-a","restriction-page",PageOperation::View,Purpose::Routine).await.is_err(),"{change}");
        }
    }
}

#[cfg(test)]
mod page_delete_authority_tests {
    use super::*;
    use crate::auth::middleware::AuthUser;
    use axum::{extract::FromRequestParts, http::Request};
    async fn fixture(scope: &str) -> (crate::AppState, AuthUser) {
        let state = crate::spaces::properties::tests::property_test_state().await;
        let scopes = serde_json::to_string(&vec![scope]).unwrap();
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('typed-delete-key','editor-a','Delete proof',$1,'test',$2)")
            .bind(crate::auth::api_keys::hash_api_key("typed-delete-key")).bind(scopes).execute(&state.db).await.unwrap();
        let req = Request::builder()
            .method(if scope == "read" { "GET" } else { "DELETE" })
            .header("X-API-Key", "typed-delete-key")
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let auth = AuthUser::from_request_parts(&mut parts, &state)
            .await
            .unwrap();
        (state, auth)
    }
    #[tokio::test]
    async fn actual_write_delete_scope_enforces_exact_state_and_preserves_body_lock_exception() {
        for (archive, trash, operation) in [
            (None, None, PageOperation::DeleteActive),
            (Some("2000"), None, PageOperation::DeleteArchived),
            (None, Some("2000"), PageOperation::DeleteTrashed),
        ] {
            let (state, auth) = fixture("write").await;
            sqlx::query("UPDATE documents SET archived_at=$1,deleted_at=$2,locked=1,locked_by='owner-a' WHERE id='target-a'").bind(archive).bind(trash).execute(&state.db).await.unwrap();
            let mut conn = state.db.acquire().await.unwrap();
            let principal = principal_from_auth(&auth).unwrap();
            assert!(resolve_page_access(
                &mut conn,
                principal,
                "space-a",
                "target-a",
                operation,
                Purpose::Routine
            )
            .await
            .is_ok());
            for wrong in [
                PageOperation::DeleteActive,
                PageOperation::DeleteArchived,
                PageOperation::DeleteTrashed,
            ] {
                if wrong != operation {
                    assert!(resolve_page_access(
                        &mut conn,
                        principal,
                        "space-a",
                        "target-a",
                        wrong,
                        Purpose::Routine
                    )
                    .await
                    .is_err());
                }
            }
        }
        let (state, auth) = fixture("read").await;
        assert!(resolve_page_access(
            &mut *state.db.acquire().await.unwrap(),
            principal_from_auth(&auth).unwrap(),
            "space-a",
            "target-a",
            PageOperation::DeleteActive,
            Purpose::Routine
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn exact_delete_permission_not_edit_and_private_editor_participation_are_required() {
        let (state, auth) = fixture("write").await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('delete-only','space-a','Delete only','[\"delete_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='delete-only' WHERE space_id='space-a' AND user_id='editor-a'").execute(&state.db).await.unwrap();
        let principal = principal_from_auth(&auth).unwrap();
        assert!(resolve_page_access(
            &mut *state.db.acquire().await.unwrap(),
            principal,
            "space-a",
            "target-a",
            PageOperation::DeleteActive,
            Purpose::Routine
        )
        .await
        .is_ok());
        assert!(resolve_page_access(
            &mut *state.db.acquire().await.unwrap(),
            principal,
            "space-a",
            "target-a",
            PageOperation::Edit,
            Purpose::Routine
        )
        .await
        .is_err());
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('delete-private','target-a','space-a','owner-a','PRIVATE','base')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('delete-private','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        assert!(resolve_page_access(
            &mut *state.db.acquire().await.unwrap(),
            principal,
            "space-a",
            "target-a",
            PageOperation::DeleteActive,
            Purpose::Routine
        )
        .await
        .is_err());
        sqlx::query("UPDATE document_draft_access SET role='editor' WHERE draft_id='delete-private' AND user_id='editor-a'").execute(&state.db).await.unwrap();
        assert!(resolve_page_access(
            &mut *state.db.acquire().await.unwrap(),
            principal,
            "space-a",
            "target-a",
            PageOperation::DeleteActive,
            Purpose::Routine
        )
        .await
        .is_ok());
    }
    #[tokio::test]
    async fn inactive_ancestor_delete_traversal_still_enforces_current_restrictions_and_blocks() {
        let (state, auth) = fixture("write").await;
        let principal = principal_from_auth(&auth).unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at,deleted_at) VALUES('delete-parent','space-a','Private parent','','owner-a','owner-a','published','2000')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET parent_id='delete-parent' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(resolve_page_access(
            &mut *state.db.acquire().await.unwrap(),
            principal,
            "space-a",
            "target-a",
            PageOperation::DeleteActive,
            Purpose::Routine
        )
        .await
        .is_ok());
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('delete-parent','space-a','restricted','delete-revision','owner-a')").execute(&state.db).await.unwrap();
        assert!(resolve_page_access(
            &mut *state.db.acquire().await.unwrap(),
            principal,
            "space-a",
            "target-a",
            PageOperation::DeleteActive,
            Purpose::Routine
        )
        .await
        .is_err());
        sqlx::query("INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('delete-allow','delete-parent','space-a','editor-a','owner-a')").execute(&state.db).await.unwrap();
        assert!(resolve_page_access(
            &mut *state.db.acquire().await.unwrap(),
            principal,
            "space-a",
            "target-a",
            PageOperation::DeleteActive,
            Purpose::Routine
        )
        .await
        .is_ok());
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','editor-a','owner-a')").execute(&state.db).await.unwrap();
        assert!(resolve_page_access(
            &mut *state.db.acquire().await.unwrap(),
            principal,
            "space-a",
            "target-a",
            PageOperation::DeleteActive,
            Purpose::Routine
        )
        .await
        .is_err());
    }
}

#[cfg(test)]
mod direct_viewer_foundation_tests {
    use super::*;
    use sqlx::{any::AnyPoolOptions, AnyPool};
    use std::sync::Arc;
    use tokio::sync::Notify;

    async fn fixture(wal: bool) -> AnyPool {
        sqlx::any::install_default_drivers();
        let url = if wal { format!("sqlite:///tmp/starlane-direct-viewer-{}.db?mode=rwc", uuid::Uuid::new_v4()) } else { "sqlite::memory:".into() };
        let pool = AnyPoolOptions::new().max_connections(if wal { 3 } else { 1 }).after_connect(|conn, _| Box::pin(async move {
            sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *conn).await?;
            sqlx::query("PRAGMA busy_timeout=5000").execute(&mut *conn).await?;
            Ok(())
        })).connect(&url).await.unwrap();
        sqlx::query("PRAGMA foreign_keys=ON").execute(&pool).await.unwrap();
        if wal { sqlx::query("PRAGMA journal_mode=WAL").execute(&pool).await.unwrap(); }
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        for id in ["owner", "reader", "member"] {
            sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')").bind(id).bind(format!("{id}@example.test")).execute(&pool).await.unwrap();
        }
        sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('space','Space','space','private','owner'),('foreign','Foreign','foreign','private','owner')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('owner-member','space','owner','owner'),('member-row','space','member','viewer')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,parent_id,title,content_path,created_by,last_updated_by,published_at) VALUES('parent','space',NULL,'Parent','','owner','owner','published'),('page','space','parent','Page','','owner','owner','published')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO document_viewer_grants(id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at) VALUES('grant','page','space','reader','viewer','page','revision-1','owner','created','updated')").execute(&pool).await.unwrap();
        pool
    }
    fn reader() -> Principal<'static> { Principal::User { user_id: "reader", session_id: None, token_expires_at: None } }
    async fn resolve(pool: &AnyPool, operation: PageOperation) -> Result<PageAccess,AppError> {
        resolve_page_access(&mut pool.acquire().await.unwrap(),reader(),"space","page",operation,Purpose::Routine).await
    }
    #[tokio::test]
    async fn direct_viewer_named_read_never_unions_space_or_activates_existing_routes() {
        let pool=fixture(false).await;
        let access=resolve(&pool,PageOperation::GrantedPublishedView).await.unwrap();
        assert!(access.allows_page(Permission::View));
        assert!(!access.allows_page(Permission::Comment));
        assert!(access.space.permissions.is_empty()); assert!(access.space.roles.is_empty());
        assert_eq!(access.direct_viewer_grant.unwrap().revision,"revision-1");
        for operation in [PageOperation::View,PageOperation::InspectMetadata,PageOperation::InspectArchivedMetadata,PageOperation::InspectTrashedMetadata,PageOperation::DraftOwnerRead,PageOperation::DraftOwnerMutation,PageOperation::AcknowledgeNotification,PageOperation::UserPagePreference,PageOperation::ReferenceFromMutation,PageOperation::InspectMovePreviewFromMutation,PageOperation::Comment,PageOperation::Edit,PageOperation::Move,PageOperation::Trash,PageOperation::PublicLinkEnable,PageOperation::ManageLock,PageOperation::RestrictionPreview] { assert!(resolve(&pool,operation).await.is_err(),"{operation:?}"); }
        assert!(resolve_page_access(&mut pool.acquire().await.unwrap(),reader(),"space","parent",PageOperation::GrantedPublishedView,Purpose::Routine).await.is_err());
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM space_members WHERE user_id='reader'").fetch_one(&pool).await.unwrap();assert_eq!(count,0);
    }
    #[tokio::test]
    async fn direct_viewer_requires_active_published_target_without_lifecycle_exceptions() {
        let pool=fixture(false).await;
        for statement in ["UPDATE documents SET published_at=NULL WHERE id='page'","UPDATE documents SET published_at='published',archived_at='archived' WHERE id='page'","UPDATE documents SET archived_at=NULL,deleted_at='deleted' WHERE id='page'"] {
            sqlx::query(statement).execute(&pool).await.unwrap();assert!(resolve(&pool,PageOperation::GrantedPublishedView).await.is_err());
        }
    }
    #[tokio::test]
    async fn direct_viewer_intersects_restrictions_and_private_ancestor_participation() {
        let pool=fixture(false).await;
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('parent','space','restricted','r','owner')").execute(&pool).await.unwrap();
        assert!(resolve(&pool,PageOperation::GrantedPublishedView).await.is_err());
        sqlx::query("INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('allow','parent','space','reader','owner')").execute(&pool).await.unwrap();
        assert!(resolve(&pool,PageOperation::GrantedPublishedView).await.is_ok());
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='parent'").execute(&pool).await.unwrap();
        assert!(resolve(&pool,PageOperation::GrantedPublishedView).await.is_err());
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('private-parent','parent','space','owner','Private','old')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('private-parent','reader','viewer','owner')").execute(&pool).await.unwrap();
        assert!(resolve(&pool,PageOperation::GrantedPublishedView).await.is_ok());
        assert!(resolve_draft_access(&mut pool.acquire().await.unwrap(),reader(),"space","parent","private-parent",DraftOperation::View,Purpose::Routine).await.is_err());
    }
    #[tokio::test]
    async fn direct_viewer_block_and_current_account_override_grant() {
        let pool=fixture(false).await;
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space','reader','owner')").execute(&pool).await.unwrap();
        assert!(resolve(&pool,PageOperation::GrantedPublishedView).await.is_err());
        sqlx::query("DELETE FROM space_user_blocks WHERE user_id='reader'").execute(&pool).await.unwrap();
        sqlx::query("UPDATE users SET is_active=0 WHERE id='reader'").execute(&pool).await.unwrap();
        assert!(resolve(&pool,PageOperation::GrantedPublishedView).await.is_err());
    }
    #[tokio::test]
    async fn direct_viewer_personal_key_requires_read_ceiling_and_current_key() {
        let pool=fixture(false).await;
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('viewer-key','reader','Key','hash','test',0,'[\"write\"]')").execute(&pool).await.unwrap();
        let principal=Principal::UserKey { key_id:"viewer-key",user_id:"reader",token_expires_at:None };
        assert!(resolve_page_access(&mut pool.acquire().await.unwrap(),principal,"space","page",PageOperation::GrantedPublishedView,Purpose::Routine).await.is_err());
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='viewer-key'").execute(&pool).await.unwrap();
        assert!(resolve_page_access(&mut pool.acquire().await.unwrap(),principal,"space","page",PageOperation::GrantedPublishedView,Purpose::Routine).await.is_ok());
        sqlx::query("DELETE FROM api_keys WHERE id='viewer-key'").execute(&pool).await.unwrap();
        assert!(resolve_page_access(&mut pool.acquire().await.unwrap(),principal,"space","page",PageOperation::GrantedPublishedView,Purpose::Routine).await.is_err());
    }
    #[tokio::test]
    async fn direct_viewer_existing_member_inheritance_stays_exact() {
        let pool=fixture(false).await;let member=Principal::User { user_id:"member",session_id:None,token_expires_at:None };
        let access=resolve_page_access(&mut pool.acquire().await.unwrap(),member,"space","page",PageOperation::View,Purpose::Routine).await.unwrap();
        assert_eq!(access.page_permissions,access.space.permissions);assert!(access.direct_viewer_grant.is_none());assert!(access.space.allows(Permission::View));
    }
    #[tokio::test]
    async fn direct_viewer_migration_checks_scope_role_binding_and_recipient_uniqueness() {
        let pool=fixture(false).await;
        for (id,space,user,role,scope,revision) in [("bad-role","space","member","editor","page","r"),("bad-scope","space","member","viewer","subtree","r"),("bad-space","foreign","member","viewer","page","r"),("bad-revision","space","member","viewer","page"," "),("duplicate","space","reader","viewer","page","r")] {
            let result=sqlx::query("INSERT INTO document_viewer_grants(id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at) VALUES($1,'page',$2,$3,$4,$5,$6,'owner','created','updated')").bind(id).bind(space).bind(user).bind(role).bind(scope).bind(revision).execute(&pool).await;assert!(result.is_err(),"{id}");
        }
        assert!(sqlx::query("DELETE FROM documents WHERE id='page'").execute(&pool).await.is_err());
    }
    #[tokio::test]
    async fn direct_viewer_revoke_during_final_boundary_rejects_real_wal_read() {
        let pool=fixture(true).await;let reached=Arc::new(Notify::new());let release=Arc::new(Notify::new());
        let child=pool.clone();let gate=(reached.clone(),release.clone());
        let waiting=tokio::spawn(async move { DIRECT_VIEWER_FINAL_GATE.scope(gate,async move { resolve(&child,PageOperation::GrantedPublishedView).await }).await });
        tokio::time::timeout(std::time::Duration::from_secs(5), reached.notified()).await.unwrap();
        sqlx::query("DELETE FROM document_viewer_grants WHERE id='grant'").execute(&pool).await.unwrap();
        release.notify_one();assert!(waiting.await.unwrap().is_err());
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM document_viewer_grants").fetch_one(&pool).await.unwrap();assert_eq!(count,0);
    }
    #[tokio::test]
    async fn direct_viewer_unknown_account_kind_is_not_guest_activation() {
        let pool=fixture(false).await;
        sqlx::query("UPDATE users SET role='guest' WHERE id='reader'").execute(&pool).await.unwrap();
        assert!(resolve(&pool,PageOperation::GrantedPublishedView).await.is_err());
    }
    #[tokio::test]
    async fn direct_viewer_revoked_original_session_after_ancestor_wait_is_not_admitted() {
        let pool=fixture(true).await;
        let expiry=(chrono::Utc::now()+chrono::Duration::hours(1)).to_rfc3339();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('reader-session','reader','fictional',$1)").bind(expiry).execute(&pool).await.unwrap();
        let reached=Arc::new(Notify::new());let release=Arc::new(Notify::new());let child=pool.clone();let gate=(reached.clone(),release.clone());
        let waiting=tokio::spawn(async move { DIRECT_VIEWER_FINAL_GATE.scope(gate,async move {
            resolve_page_access(&mut child.acquire().await.unwrap(),Principal::User { user_id:"reader",session_id:Some("reader-session"),token_expires_at:Some(chrono::Utc::now().timestamp()+3600) },"space","page",PageOperation::GrantedPublishedView,Purpose::Routine).await
        }).await });
        tokio::time::timeout(std::time::Duration::from_secs(5), reached.notified()).await.unwrap();sqlx::query("DELETE FROM sessions WHERE id='reader-session'").execute(&pool).await.unwrap();release.notify_one();assert!(waiting.await.unwrap().is_err());
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM document_viewer_grants WHERE id='grant'").fetch_one(&pool).await.unwrap();assert_eq!(count,1);
    }

}
