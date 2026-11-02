//! Caller-specific, read-only access evidence. This is not a grant, audience
//! enumeration, publisher authorization, or a promise of continuous access.
use super::{
    access::{
        self, DraftOperation, DraftParticipation, PageOperation, Permission, Purpose, RoleSource,
    },
    maintenance::rows,
};
use crate::{
    auth::{
        middleware::{authenticate_request, ApiKeyRequestOperation, AuthError, AuthUser},
        routes::AppError,
    },
    AppState,
};
use axum::{
    extract::{FromRef, FromRequestParts, Path, State},
    http::request::Parts,
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::AnyConnection;
use std::collections::{BTreeMap, BTreeSet};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/spaces/{space_id}/documents/{doc_id}/access", get(inspect))
        .route(
            "/spaces/{space_id}/documents/{doc_id}/access/inspect",
            axum::routing::post(inspect),
        )
}
/// Both transports inspect only. Preserve original key/session evidence and the
/// Read ceiling even on POST; never manufacture a human or remove key scopes.
struct InspectionAuth(AuthUser);
impl<S> FromRequestParts<S> for InspectionAuth
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = AuthError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        authenticate_request(
            parts,
            &AppState::from_ref(state),
            ApiKeyRequestOperation::Read,
        )
        .await
        .map(Self)
    }
}
fn unavailable() -> AppError {
    AppError::NotFound("Page access unavailable".into())
}
async fn page(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    op: PageOperation,
) -> Result<access::PageAccess, AppError> {
    access::resolve_page_access(
        conn,
        access::principal_from_auth(auth)?,
        space,
        id,
        op,
        Purpose::Routine,
    )
    .await
}
fn expected_denial(error: &AppError) -> bool {
    matches!(error, AppError::NotFound(_) | AppError::Forbidden(_))
        || matches!(error, AppError::Conflict(message) if message == "Document is locked by another user")
}
async fn allowed(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    op: PageOperation,
) -> Result<bool, AppError> {
    match page(conn, auth, space, id, op).await {
        Ok(_) => Ok(true),
        Err(e) if expected_denial(&e) => Ok(false),
        Err(e) => Err(e),
    }
}
async fn selected(
    conn: &mut AnyConnection,
    actor: &str,
    space: &str,
    id: &str,
) -> Result<Option<Value>, AppError> {
    let mut candidates = rows(conn,"SELECT d.* FROM document_drafts d WHERE d.space_id=$1 AND d.document_id=$2 AND (d.owner_id=$3 OR EXISTS(SELECT 1 FROM document_draft_access a WHERE a.draft_id=d.id AND a.user_id=$3 AND a.role IN ('viewer','editor'))) ORDER BY CASE WHEN d.owner_id=$3 THEN 0 ELSE 1 END,d.updated_at DESC,d.id ASC LIMIT 1",&[space,id,actor]).await?;
    Ok(candidates.pop())
}
async fn draft_view(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    draft: &str,
    op: DraftOperation,
) -> Result<access::DraftAccess, AppError> {
    access::resolve_draft_access(
        conn,
        access::principal_from_auth(auth)?,
        space,
        id,
        draft,
        op,
        Purpose::Routine,
    )
    .await
}
/// Preserve inherited/private View behavior; only its ordinary denial may
/// select the new exact active-published direct viewer admission.
async fn report_page(conn:&mut AnyConnection,auth:&AuthUser,space:&str,id:&str)->Result<access::PageAccess,AppError> {
    match page(conn,auth,space,id,PageOperation::View).await {
        Ok(value)=>Ok(value),
        Err(error) if expected_denial(&error)=>page(conn,auth,space,id,PageOperation::GrantedPublishedView).await,
        Err(error)=>Err(error),
    }
}
async fn report(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    public_enabled: bool,
) -> Result<(Value, Vec<String>, Option<String>), AppError> {
    let current = report_page(conn, auth, space, id).await?;
    let grant_only=current.direct_viewer_grant.is_some();
    let mut chain = vec![id.to_owned()];
    chain.extend(current.ancestor_ids.clone());
    let mut sources: BTreeMap<&str, BTreeSet<Permission>> = BTreeMap::new();
    for role in &current.space.roles {
        let kind = match &role.source {
            RoleSource::Direct => "direct_space_role",
            RoleSource::SpaceGroup { .. } => "space_group_role",
            RoleSource::GlobalGroup { .. } => "global_group_role",
        };
        let permissions = access::role_permissions(conn, space, role).await?;
        sources.entry(kind).or_default().extend(
            permissions
                .intersection(&current.space.permissions)
                .copied(),
        );
    }
    let visibility: String = sqlx::query_scalar("SELECT visibility FROM spaces WHERE id=$1")
        .bind(space)
        .fetch_one(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if matches!(visibility.as_str(), "internal" | "public")
        && current.space.allows(Permission::View)
    {
        sources
            .entry(if visibility == "internal" {
                "internal_space_fallback"
            } else {
                "public_space_fallback"
            })
            .or_default()
            .insert(Permission::View);
    }
    if current.space.instance_admin {
        sources
            .entry("instance_admin")
            .or_default()
            .extend(current.space.permissions.iter().copied());
    }
    let mut capabilities = serde_json::Map::new();
    capabilities.insert("view".into(), json!(true));
    for (name, op) in [
        ("comment", PageOperation::Comment),
        ("edit", PageOperation::Edit),
        ("move", PageOperation::Move),
        ("manage_lock", PageOperation::ManageLock),
        ("enable_public_link", PageOperation::PublicLinkEnable),
        ("disable_public_link", PageOperation::PublicLinkDisable),
    ] {
        let mut value = if grant_only {false} else {allowed(conn, auth, space, id, op).await?};
        if name == "enable_public_link" {
            value &= current.published && public_enabled;
        }
        capabilities.insert(name.into(), json!(value));
    }
    let mut local = "inherited";
    let mut ancestor_restricted = false;
    for (index, node) in chain.iter().enumerate() {
        let mode: Option<String> = sqlx::query_scalar(
            "SELECT mode FROM document_access_policies WHERE document_id=$1 AND space_id=$2",
        )
        .bind(node)
        .bind(space)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?;
        if mode.as_deref() == Some("restricted") {
            if index == 0 {
                local = "restricted"
            } else {
                ancestor_restricted = true
            }
        }
    }
    let state: (i64, Option<String>) =
        sqlx::query_as("SELECT public,public_slug FROM documents WHERE id=$1 AND space_id=$2")
            .bind(id)
            .bind(space)
            .fetch_one(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    let configured = state.0 != 0;
    let reason = if !configured {
        Some("not_enabled")
    } else if state.1.as_ref().is_none_or(|s| s.is_empty()) {
        Some("missing_link")
    } else if !public_enabled {
        Some("deployment_disabled")
    } else if !current.published {
        Some("unpublished")
    } else {
        match access::require_anonymous_page_chain(conn, space, id).await {
            Ok(()) => None,
            Err(e) if expected_denial(&e) => Some("restricted"),
            Err(e) => return Err(e),
        }
    };
    let chosen = if grant_only {None} else {selected(conn, &current.space.actor_id, space, id).await?};
    let mut draft = json!({"scope":"none","view":false,"edit":false});
    let draft_id = chosen
        .as_ref()
        .and_then(|r| r["id"].as_str())
        .map(str::to_owned);
    if let Some(draft_id) = &draft_id {
        let access = draft_view(conn, auth, space, id, draft_id, DraftOperation::View).await?;
        let scope = match access.participation {
            DraftParticipation::Owner => "owned",
            DraftParticipation::SharedViewer => "shared_viewer",
            DraftParticipation::SharedEditor => "shared_editor",
            DraftParticipation::RecoveryAuditRequired => return Err(unavailable()),
        };
        let edit = match draft_view(conn, auth, space, id, draft_id, DraftOperation::Edit).await {
            Ok(_) => true,
            Err(e) if expected_denial(&e) => false,
            Err(e) => return Err(e),
        };
        draft = json!({"scope":scope,"view":true,"edit":edit});
    }
    // Feature availability is separate from management authority. Active
    // published admission includes the complete current ancestor boundary.
    let people_grants_available = current.published
        && allowed(conn, auth, space, id, PageOperation::GrantedPublishedView).await?;
    Ok((
        json!({"schema_version":1,"space_id":space,"page_id":id,"capabilities":capabilities,"page_permissions":current.page_permissions,"page_permission_sources":current.direct_viewer_grant.as_ref().map(|g|vec![json!({"kind":"direct_page_viewer","grant_id":g.id,"revision":g.revision,"permissions":[Permission::View]})]).unwrap_or_default(),"space_permission_sources":sources.into_iter().filter(|(_,p)|!p.is_empty()).map(|(kind,permissions)|json!({"kind":kind,"permissions":permissions})).collect::<Vec<_>>(),"restrictions":{"local":local,"ancestor_restricted":ancestor_restricted},"public_link":{"configured":configured,"policy_allows_anonymous":reason.is_none(),"reason":reason},"selected_private_draft":draft,"sharing":{"people_grants_available":people_grants_available,"guests_available":people_grants_available}}),
        chain,
        draft_id,
    ))
}
/// Relevant raw evidence remains internal, never a cached permission capability.
async fn source(
    conn: &mut AnyConnection,
    actor: &str,
    space: &str,
    chain: &[String],
) -> Result<Value, AppError> {
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
    ] { value.insert(name.into(),json!(rows(conn,query,&args).await?)); }
    value.insert("viewer_grants".into(),super::page_viewer_grants::capture_space_dependencies(conn,space).await?);
    let mut pages = vec![];
    for node in chain {
        pages.push(json!({"page":rows(conn,"SELECT * FROM documents WHERE id=$1 AND space_id=$2",&[node,space]).await?,"policy":rows(conn,"SELECT * FROM document_access_policies WHERE document_id=$1 ORDER BY document_id",&[node]).await?,"allowlist":rows(conn,"SELECT * FROM document_access_allowlist WHERE document_id=$1 ORDER BY id",&[node]).await?,"allowlist_targets":rows(conn,"SELECT a.id,u.id AS user_id,g.id AS group_id FROM document_access_allowlist a LEFT JOIN users u ON u.id=a.user_id LEFT JOIN groups g ON g.id=a.group_id WHERE a.document_id=$1 ORDER BY a.id",&[node]).await?,"drafts":rows(conn,"SELECT * FROM document_drafts WHERE document_id=$1 ORDER BY id",&[node]).await?,"own_shares":rows(conn,"SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.document_id=$1 AND a.user_id=$2 ORDER BY a.draft_id",&[node,actor]).await?}));
    }
    value.insert("pages".into(), json!(pages));
    Ok(Value::Object(value))
}
async fn inspect(
    State(state): State<AppState>,
    InspectionAuth(auth): InspectionAuth,
    Path((space, id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let enabled = super::documents::public_pages_enabled();
    inspect_on(&mut conn, &auth, &space, &id, enabled)
        .await
        .map(Json)
}
async fn inspect_on(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    enabled: bool,
) -> Result<Value, AppError> {
    let (mut output, chain, draft) = report(conn, auth, space, id, enabled).await?;
    let before = source(conn, &auth.claims.sub, space, &chain).await?;
    #[cfg(test)]
    after_report().await;
    let (final_output, final_chain, final_draft) = report(conn, auth, space, id, enabled).await?;
    if output != final_output || chain != final_chain || draft != final_draft {
        return Err(unavailable());
    }
    #[cfg(test)]
    after_capabilities().await;
    if let Some(draft) = &draft {
        draft_view(conn, auth, space, id, draft, DraftOperation::View).await?;
    }
    #[cfg(test)]
    after_selected_draft().await;
    if source(conn, &auth.claims.sub, space, &chain).await? != before {
        return Err(unavailable());
    }
    if super::documents::public_pages_enabled() != enabled {
        return Err(unavailable());
    }
    output["checked_at"] = json!(chrono::Utc::now().to_rfc3339());
    #[cfg(test)]
    before_identity().await;
    if source(conn,&auth.claims.sub,space,&chain).await? != before {return Err(unavailable());}
    report_page(conn,auth,space,id).await?;
    if super::documents::public_pages_enabled()!=enabled {return Err(unavailable());}
    access::require_current_human_read(conn, auth).await?;
    Ok(output)
}
#[cfg(test)]
tokio::task_local! {static AFTER_REPORT:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;static AFTER_CAPABILITIES:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;static AFTER_SELECTED_DRAFT:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;static BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;}
#[cfg(test)]
async fn after_report() {
    if let Ok(Some(h)) = AFTER_REPORT.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}
#[cfg(test)]
async fn after_capabilities() {
    if let Ok(Some(h)) = AFTER_CAPABILITIES.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}
#[cfg(test)]
async fn after_selected_draft() {
    if let Ok(Some(h)) = AFTER_SELECTED_DRAFT.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}
#[cfg(test)]
async fn before_identity() {
    if let Ok(Some(h)) = BEFORE_IDENTITY.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    pub(super) async fn setup(wal: bool) -> AppState {
        let s = crate::spaces::routes::space_creation_fence_tests::setup(wal).await;
        sqlx::query("UPDATE users SET role='member' WHERE id='owner-a'")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at,public,public_slug) VALUES('page','space-a','SECRET CANONICAL TITLE','unread-file','owner-a','owner-a','2026-10-08T00:00:00Z',1,'SECRET-LINK')").execute(&s.db).await.unwrap();
        s
    }
    fn token(s: &AppState, user: &str, exp: Option<i64>) -> String {
        let mut a = crate::spaces::documents::tests::test_auth(user);
        a.claims.sid = format!("metadata-{user}");
        if let Some(e) = exp {
            a.claims.exp = e;
        }
        jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &a.claims,
            &jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes()),
        )
        .unwrap()
    }
    pub(super) async fn auth(s: &AppState, user: &str) -> AuthUser {
        let request = Request::builder()
            .header("Authorization", format!("Bearer {}", token(s, user, None)))
            .body(())
            .unwrap();
        let (mut p, _) = request.into_parts();
        InspectionAuth::from_request_parts(&mut p, s)
            .await
            .unwrap()
            .0
    }
    pub(super) async fn read(s: &AppState, a: &AuthUser) -> Result<Value, AppError> {
        let mut c = s.db.acquire().await.unwrap();
        inspect_on(
            &mut c,
            a,
            "space-a",
            "page",
            super::super::documents::public_pages_enabled(),
        )
        .await
    }
    async fn snapshot(s: &AppState) -> Vec<(String, Option<String>)> {
        crate::spaces::documents::tests::move_database_snapshot(s).await
    }
    pub(super) async fn key(s: &AppState, id: &str, scopes: &str, service: bool) {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes,is_service_account) VALUES($1,'owner-a',$1,$2,'test',$3,$4)").bind(id).bind(crate::auth::api_keys::hash_api_key(id)).bind(scopes).bind(i64::from(service)).execute(&s.db).await.unwrap();
    }
    pub(super) async fn http(s: &AppState, method: &str, credential: &str) -> (StatusCode, Value) {
        let path = if method == "GET" {
            "/spaces/space-a/documents/page/access"
        } else {
            "/spaces/space-a/documents/page/access/inspect"
        };
        let r = router()
            .with_state(s.clone())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("X-API-Key", credential)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = r.status();
        let v =
            serde_json::from_slice(&to_bytes(r.into_body(), usize::MAX).await.unwrap()).unwrap();
        (status, v)
    }
    #[tokio::test]
    async fn real_http_read_credentials_inspect_without_write_capability_or_domain_writes() {
        let s = setup(false).await;
        for (id, scopes, service) in [
            ("read", "[\"read\"]", false),
            ("write", "[\"write\"]", false),
            ("admin", "[\"admin\"]", false),
            ("service", "[\"admin\"]", true),
        ] {
            key(&s, id, scopes, service).await;
        }
        let key_bookkeeping: Vec<(String, Option<String>)> =
            sqlx::query_as("SELECT id,last_used_at FROM api_keys ORDER BY id")
                .fetch_all(&s.db)
                .await
                .unwrap();
        let session_bookkeeping: Vec<(String, Option<String>, String)> =
            sqlx::query_as("SELECT id,last_seen_at,expires_at FROM sessions ORDER BY id")
                .fetch_all(&s.db)
                .await
                .unwrap();
        let before = snapshot(&s).await;
        for method in ["GET", "POST"] {
            let (status, v) = http(&s, method, "read").await;
            assert_eq!(status, StatusCode::OK, "{v}");
            assert_eq!(v["capabilities"]["view"], true);
            assert_eq!(v["capabilities"]["edit"], false);
            assert_eq!(v["capabilities"]["manage_lock"], false);
            assert!(v["checked_at"].as_str().is_some());
            assert_eq!(
                v["space_permission_sources"][0]["permissions"],
                json!(["view"])
            );
            let (status, v) = http(&s, method, "admin").await;
            assert_eq!(status, StatusCode::OK, "{v}");
            assert_eq!(v["capabilities"]["edit"], true);
            assert_eq!(http(&s, method, "write").await.0, StatusCode::FORBIDDEN);
            assert!(!http(&s, method, "service").await.0.is_success());
        }
        // Restore ONLY normal extractor bookkeeping, retaining every identity,
        // scope, secret hash, session binding and all other domain columns.
        for (id, used) in key_bookkeeping {
            sqlx::query("UPDATE api_keys SET last_used_at=$1 WHERE id=$2")
                .bind(used)
                .bind(id)
                .execute(&s.db)
                .await
                .unwrap();
        }
        for (id, seen, expiry) in session_bookkeeping {
            sqlx::query("UPDATE sessions SET last_seen_at=$1,expires_at=$2 WHERE id=$3")
                .bind(seen)
                .bind(expiry)
                .bind(id)
                .execute(&s.db)
                .await
                .unwrap();
        }
        assert_eq!(snapshot(&s).await, before);
        let keys: Vec<(String, String, Option<String>, i64)> =
            sqlx::query_as("SELECT id,user_id,scopes,is_service_account FROM api_keys ORDER BY id")
                .fetch_all(&s.db)
                .await
                .unwrap();
        assert_eq!(keys.len(), 4);
        assert!(keys
            .iter()
            .any(|r| r.0 == "read" && r.2.as_deref() == Some("[\"read\"]") && r.3 == 0));
    }
    #[tokio::test]
    async fn typed_custom_role_provenance_locks_and_public_policy_are_separate() {
        let s = setup(false).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('move-only','space-a','SECRET ROLE','[\"view\",\"move_content\"]','owner-a')").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='move-only' WHERE user_id='editor-a'")
            .execute(&s.db)
            .await
            .unwrap();
        let a = auth(&s, "editor-a").await;
        let before = snapshot(&s).await;
        let v = read(&s, &a).await.unwrap();
        assert_eq!(v["capabilities"]["move"], true);
        assert_eq!(v["capabilities"]["edit"], false);
        assert_eq!(v["capabilities"]["comment"], false);
        assert!(!v.to_string().contains("SECRET"));
        assert_eq!(
            v["space_permission_sources"][0],
            json!({"kind":"direct_space_role","permissions":["view","move_content"]})
        );
        assert_eq!(
            v["public_link"]["policy_allows_anonymous"], true,
            "private space alone does not revoke an intentional link"
        );
        assert_eq!(snapshot(&s).await, before);
        sqlx::query("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='page'")
            .execute(&s.db)
            .await
            .unwrap();
        let v = read(&s, &a).await.unwrap();
        assert_eq!(v["capabilities"]["move"], false);
        assert_eq!(v["capabilities"]["view"], true);
        let mut c = s.db.acquire().await.unwrap();
        let (v, _, _) = report(&mut c, &a, "space-a", "page", false).await.unwrap();
        assert_eq!(v["public_link"]["reason"], "deployment_disabled");
        assert_eq!(v["capabilities"]["enable_public_link"], false);
        drop(c);
        sqlx::query("UPDATE space_roles SET permissions='not-json' WHERE id='move-only'")
            .execute(&s.db)
            .await
            .unwrap();
        assert!(matches!(read(&s, &a).await, Err(AppError::Conflict(_))));
    }
    pub(super) async fn drafts(s: &AppState) {
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='page'")
            .execute(&s.db)
            .await
            .unwrap();
        for (id, stamp) in [
            ("selected-secret-draft", "2026-10-08T02:00:00Z"),
            ("alternate-draft", "2026-10-08T01:00:00Z"),
        ] {
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES($1,'page','space-a','owner-a','SECRET DRAFT TITLE','SECRET BODY','base',$2)").bind(id).bind(stamp).execute(&s.db).await.unwrap();
            sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES($1,'editor-a','viewer','owner-a')").bind(id).execute(&s.db).await.unwrap();
        }
    }
    #[tokio::test]
    async fn private_draft_selected_participation_is_exact_and_never_publisher_authority() {
        let s = setup(false).await;
        drafts(&s).await;
        let a = auth(&s, "editor-a").await;
        let v = read(&s, &a).await.unwrap();
        assert_eq!(
            v["selected_private_draft"],
            json!({"scope":"shared_viewer","view":true,"edit":false})
        );
        assert_eq!(v["public_link"]["reason"], "unpublished");
        assert_eq!(v["capabilities"]["enable_public_link"], false);
        assert!(!v.to_string().contains("SECRET"));
        assert!(v["capabilities"].get("publish").is_none());
        sqlx::query("UPDATE document_draft_access SET role='editor' WHERE draft_id='selected-secret-draft' AND user_id='editor-a'").execute(&s.db).await.unwrap();
        let v = read(&s, &a).await.unwrap();
        assert_eq!(v["selected_private_draft"]["scope"], "shared_editor");
        assert_eq!(v["selected_private_draft"]["edit"], true);
        let viewer = auth(&s, "viewer-a").await;
        assert!(read(&s, &viewer).await.is_err());
        sqlx::query("UPDATE users SET role='admin' WHERE id='viewer-a'")
            .execute(&s.db)
            .await
            .unwrap();
        assert!(
            read(&s, &viewer).await.is_err(),
            "routine instance admin is not draft participation"
        );
    }
    #[tokio::test]
    async fn ancestors_blocks_lifecycle_and_restrictions_conceal_unavailable_pages() {
        for variant in [
            "block",
            "archive",
            "trash",
            "private_parent",
            "restricted_parent",
            "cycle",
        ] {
            let s = setup(false).await;
            let a = auth(&s, "editor-a").await;
            match variant {
                "block" => {
                    sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','editor-a','owner-a')").execute(&s.db).await.unwrap();
                }
                "archive" => {
                    sqlx::query("UPDATE documents SET archived_at='now' WHERE id='page'")
                        .execute(&s.db)
                        .await
                        .unwrap();
                }
                "trash" => {
                    sqlx::query("UPDATE documents SET deleted_at='now' WHERE id='page'")
                        .execute(&s.db)
                        .await
                        .unwrap();
                }
                "cycle" => {
                    sqlx::query("UPDATE documents SET parent_id='page' WHERE id='page'")
                        .execute(&s.db)
                        .await
                        .unwrap();
                }
                _ => {
                    sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('SECRET-parent','space-a','SECRET ANCESTOR','','owner-a','owner-a',$1)").bind(if variant=="private_parent"{None}else{Some("published")}).execute(&s.db).await.unwrap();
                    sqlx::query("UPDATE documents SET parent_id='SECRET-parent' WHERE id='page'")
                        .execute(&s.db)
                        .await
                        .unwrap();
                    if variant == "restricted_parent" {
                        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('SECRET-parent','space-a','restricted','1','owner-a')").execute(&s.db).await.unwrap();
                    }
                }
            }
            let before = snapshot(&s).await;
            assert!(read(&s, &a).await.is_err(), "{variant}");
            assert_eq!(snapshot(&s).await, before);
        }
    }
    #[tokio::test]
    async fn genuine_wal_late_source_and_selected_share_loss_reject_saved_report() {
        for variant in [
            "role",
            "lock",
            "policy",
            "parent",
            "selected_share",
            "draft_body",
            "public",
        ] {
            let s = setup(true).await;
            if matches!(variant, "selected_share" | "draft_body") {
                drafts(&s).await;
            }
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('foreign-page','space-b','SECRET FOREIGN','','owner-a','owner-a','published')").execute(&s.db).await.unwrap();
            let a = auth(&s, "editor-a").await;
            let db = s.db.clone();
            let sql=match variant {
                "role"=>"UPDATE space_members SET role='viewer' WHERE user_id='editor-a'",
                "lock"=>"UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='page'",
                "policy"=>"INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('page','space-a','inherited','new-source','owner-a')",
                "parent"=>"UPDATE documents SET parent_id='foreign-page' WHERE id='page'",
                "selected_share"=>"DELETE FROM document_draft_access WHERE draft_id='selected-secret-draft' AND user_id='editor-a'",
                "draft_body"=>"UPDATE document_drafts SET content='changed exact selected source' WHERE id='selected-secret-draft'",
                _=>"UPDATE documents SET public_slug=NULL WHERE id='page'"};
            let result = AFTER_CAPABILITIES
                .scope(
                    std::cell::RefCell::new(Some(Box::pin(async move {
                        sqlx::query(sql).execute(&db).await.unwrap();
                    }))),
                    read(&s, &a),
                )
                .await;
            assert!(result.is_err(), "{variant}");
            if variant == "selected_share" {
                let mut c = s.db.acquire().await.unwrap();
                page(&mut c, &a, "space-a", "page", PageOperation::View)
                    .await
                    .unwrap();
            }
        }
    }
    #[tokio::test]
    async fn provenance_distinguishes_scope_global_labels_fallbacks_and_actual_admin() {
        let s = setup(false).await;
        sqlx::query("INSERT INTO groups(id,name) VALUES('SECRET-group','SECRET GROUP NAME')")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('group-member','SECRET-group','viewer-a')").execute(&s.db).await.unwrap();
        sqlx::query(
            "INSERT INTO group_global_roles(group_id,role_id) VALUES('SECRET-group','admin')",
        )
        .execute(&s.db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('scoped-grant','space-a','SECRET-group','commenter')").execute(&s.db).await.unwrap();
        let a = auth(&s, "viewer-a").await;
        let v = read(&s, &a).await.unwrap();
        assert_eq!(v["capabilities"]["edit"], true);
        assert_eq!(
            v["capabilities"]["manage_lock"], false,
            "global admin role is not actual instance admin or ManageSpace"
        );
        assert!(v["space_permission_sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["kind"] == "space_group_role"
                && p["permissions"] == json!(["view", "comment"])));
        assert!(!v.to_string().contains("SECRET"));
        assert!(!v["space_permission_sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["kind"] == "instance_admin"));
        sqlx::query("DELETE FROM group_members WHERE user_id='viewer-a'")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("DELETE FROM space_members WHERE user_id='viewer-a'")
            .execute(&s.db)
            .await
            .unwrap();
        for (visibility, kind) in [
            ("internal", "internal_space_fallback"),
            ("public", "public_space_fallback"),
        ] {
            sqlx::query("UPDATE spaces SET visibility=$1 WHERE id='space-a'")
                .bind(visibility)
                .execute(&s.db)
                .await
                .unwrap();
            let v = read(&s, &a).await.unwrap();
            assert_eq!(
                v["space_permission_sources"],
                json!([{"kind":kind,"permissions":["view"]}])
            );
            assert_eq!(v["capabilities"]["edit"], false);
        }
        sqlx::query("UPDATE users SET role='admin' WHERE id='viewer-a'")
            .execute(&s.db)
            .await
            .unwrap();
        let v = read(&s, &a).await.unwrap();
        assert!(v["space_permission_sources"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["kind"] == "instance_admin"));
        assert_eq!(v["capabilities"]["manage_lock"], true);
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('page','space-a','restricted','restricted-source','owner-a')").execute(&s.db).await.unwrap();
        assert!(
            read(&s, &a).await.is_err(),
            "routine admin still cannot bypass restrictions"
        );
        sqlx::query("INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('allow-actor','page','space-a','viewer-a','owner-a')").execute(&s.db).await.unwrap();
        let v = read(&s, &a).await.unwrap();
        assert_eq!(v["restrictions"]["local"], "restricted");
        assert_eq!(v["public_link"]["reason"], "restricted");
        assert_eq!(v["public_link"]["policy_allows_anonymous"], false);
    }
    #[tokio::test]
    async fn connection_wait_rechecks_the_actual_previously_authenticated_session() {
        let s = setup(false).await;
        let a = auth(&s, "editor-a").await;
        let mut blocker = s.db.acquire().await.unwrap();
        let copy = s.clone();
        let mut task = tokio::spawn(async move {
            inspect(
                State(copy),
                InspectionAuth(a),
                Path(("space-a".into(), "page".into())),
            )
            .await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut task)
                .await
                .is_err()
        );
        sqlx::query("DELETE FROM sessions WHERE user_id='editor-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        drop(blocker);
        assert!(matches!(
            tokio::time::timeout(std::time::Duration::from_secs(5), task)
                .await
                .unwrap()
                .unwrap(),
            Err(AppError::Unauthorized(_))
        ));
    }

    #[tokio::test]
    async fn current_credentials_after_report_queries_and_original_jwt_expiry_deny() {
        for variant in ["session", "inactive", "block", "key"] {
            let s = setup(true).await;
            key(&s, "read", "[\"read\"]", false).await;
            let a = if variant == "key" {
                let request = Request::builder()
                    .header("X-API-Key", "read")
                    .body(())
                    .unwrap();
                let (mut p, _) = request.into_parts();
                InspectionAuth::from_request_parts(&mut p, &s)
                    .await
                    .unwrap()
                    .0
            } else {
                auth(&s, "editor-a").await
            };
            let db = s.db.clone();
            let sql=match variant {"session"=>"DELETE FROM sessions WHERE user_id='editor-a'","inactive"=>"UPDATE users SET is_active=0 WHERE id='editor-a'","block"=>"INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','editor-a','owner-a')",_=>"DELETE FROM api_keys WHERE id='read'"};
            let result = AFTER_REPORT
                .scope(
                    std::cell::RefCell::new(Some(Box::pin(async move {
                        sqlx::query(sql).execute(&db).await.unwrap();
                    }))),
                    read(&s, &a),
                )
                .await;
            assert!(result.is_err(), "{variant}");
        }
        let s = setup(false).await;
        let expires = chrono::Utc::now().timestamp() + 2;
        let t = token(&s, "editor-a", Some(expires));
        let r = Request::builder()
            .header("Authorization", format!("Bearer {t}"))
            .body(())
            .unwrap();
        let (mut p, _) = r.into_parts();
        let a = InspectionAuth::from_request_parts(&mut p, &s)
            .await
            .unwrap()
            .0;
        let before = snapshot(&s).await;
        let result = BEFORE_IDENTITY
            .scope(
                std::cell::RefCell::new(Some(Box::pin(async move {
                    while chrono::Utc::now().timestamp() < expires {
                        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                    }
                }))),
                read(&s, &a),
            )
            .await;
        assert!(matches!(result, Err(AppError::Unauthorized(_))));
        assert_eq!(snapshot(&s).await, before);
    }
    async fn direct_viewer(s:&AppState)->AuthUser {
        sqlx::query("DELETE FROM space_members WHERE user_id='viewer-a'").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE users SET role='member' WHERE id='viewer-a'").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO document_viewer_grants(id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at) VALUES('page-viewer','page','space-a','viewer-a','viewer','page','original','owner-a','created','updated')").execute(&s.db).await.unwrap();
        auth(s,"viewer-a").await
    }
    #[tokio::test]
    async fn direct_viewer_access_report_has_separate_read_only_provenance_no_space_or_draft_union() {
        let s=setup(false).await;let a=direct_viewer(&s).await;
        let report=read(&s,&a).await.unwrap();
        assert_eq!(report["capabilities"]["view"],true);
        assert_eq!(report["sharing"],json!({"people_grants_available":true,"guests_available":true}));
        for capability in ["comment","edit","move","manage_lock","enable_public_link","disable_public_link"] {assert_eq!(report["capabilities"][capability],false);}
        assert_eq!(report["space_permission_sources"],json!([]));
        assert_eq!(report["page_permission_sources"],json!([{"kind":"direct_page_viewer","grant_id":"page-viewer","revision":"original","permissions":["view"]}]));
        assert_eq!(report["selected_private_draft"],json!({"scope":"none","view":false,"edit":false}));
        assert!(!report.to_string().contains("SECRET CANONICAL TITLE"));assert!(!report.to_string().contains("SECRET-LINK"));
    }
    #[tokio::test]
    async fn people_grant_feature_availability_is_active_published_not_manage_authority() {
        let s=setup(false).await;let a=auth(&s,"viewer-a").await;
        let report=read(&s,&a).await.unwrap();
        assert_eq!(report["sharing"],json!({"people_grants_available":true,"guests_available":true}));
        assert_eq!(report["capabilities"]["manage_lock"],false);
        drafts(&s).await;
        let owner=auth(&s,"owner-a").await;
        assert_eq!(read(&s,&owner).await.unwrap()["sharing"],json!({"people_grants_available":false,"guests_available":false}));
        sqlx::query("UPDATE documents SET published_at='published' WHERE id='page'").execute(&s.db).await.unwrap();
        assert_eq!(read(&s,&a).await.unwrap()["sharing"]["people_grants_available"],true);
    }
    #[tokio::test]
    async fn direct_viewer_access_report_final_wal_revoke_is_not_identity_only() {
        let s=setup(true).await;let a=direct_viewer(&s).await;let db=s.db.clone();
        let result=BEFORE_IDENTITY.scope(std::cell::RefCell::new(Some(Box::pin(async move {
            sqlx::query("DELETE FROM document_viewer_grants WHERE id='page-viewer'").execute(&db).await.unwrap();
        }) as std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>)),read(&s,&a)).await;
        assert!(result.is_err());
    }

}

#[cfg(test)]
mod selected_boundary_test {
    use super::*;
    #[tokio::test]
    async fn exact_selected_editor_demotion_after_final_view_is_rejected() {
        let s = tests::setup(true).await;
        tests::drafts(&s).await;
        sqlx::query("UPDATE document_draft_access SET role='editor' WHERE draft_id='selected-secret-draft' AND user_id='editor-a'").execute(&s.db).await.unwrap();
        let a = tests::auth(&s, "editor-a").await;
        let db = s.db.clone();
        let result=AFTER_SELECTED_DRAFT.scope(std::cell::RefCell::new(Some(Box::pin(async move{sqlx::query("UPDATE document_draft_access SET role='viewer' WHERE draft_id='selected-secret-draft' AND user_id='editor-a'").execute(&db).await.unwrap();}))),tests::read(&s,&a)).await;
        assert!(result.is_err());
        let mut c = s.db.acquire().await.unwrap();
        draft_view(
            &mut c,
            &a,
            "space-a",
            "page",
            "selected-secret-draft",
            DraftOperation::View,
        )
        .await
        .unwrap();
    }
}

#[cfg(test)]
mod deployment_test {
    use super::*;
    #[test]
    fn real_deployment_disabled_policy_is_isolated_from_other_tests() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "spaces::page_access::deployment_test::deployment_disabled_http_child",
                "--nocapture",
            ])
            .env("ALLOW_PUBLIC_PAGES", "false")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    #[tokio::test]
    #[ignore = "isolated configured-public-policy child invoked by parent test"]
    async fn deployment_disabled_http_child() {
        assert_eq!(std::env::var("ALLOW_PUBLIC_PAGES").unwrap(), "false");
        let s = tests::setup(false).await;
        tests::key(&s, "read", "[\"read\"]", false).await;
        tests::key(&s, "admin", "[\"admin\"]", false).await;
        for method in ["GET", "POST"] {
            let (status, v) = tests::http(&s, method, "read").await;
            assert_eq!(status, axum::http::StatusCode::OK);
            assert_eq!(v["public_link"]["configured"], true);
            assert_eq!(v["public_link"]["policy_allows_anonymous"], false);
            assert_eq!(v["public_link"]["reason"], "deployment_disabled");
            let (_, v) = tests::http(&s, method, "admin").await;
            assert_eq!(v["capabilities"]["enable_public_link"], false);
            assert_eq!(v["capabilities"]["disable_public_link"], true);
        }
    }

}
