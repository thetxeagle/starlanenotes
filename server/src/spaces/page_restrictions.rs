//! Confirmed restriction management only. Allowlists intersect authority; no grants or guests.
use super::access::{self, PageOperation, Purpose};
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
    routing::post,
    Json, Router,
};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::AnyConnection;
use std::collections::BTreeSet;
const KIND: &str = "starlane.restriction.preview.v1";
const NOTICE:&str="This setting applies to this page and all descendants. Other restrictions, space blocks and private draft access still apply. Descendant public links can be suspended. Only currently readable pages are listed.";
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/spaces/{space}/documents/{document}/restrictions/inspect",
            post(inspect),
        )
        .route(
            "/spaces/{space}/documents/{document}/restrictions/preview",
            post(preview),
        )
        .route(
            "/spaces/{space}/documents/{document}/restrictions/apply",
            post(apply),
        )
}
struct ReadAuth(AuthUser);
impl<S> FromRequestParts<S> for ReadAuth
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
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Allowlist {
    users: Vec<String>,
    groups: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Proposal {
    mode: String,
    allowlist: Allowlist,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Confirmation {
    preview_token: String,
    confirm: bool,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Credential {
    actor: String,
    identity: String,
    bearer_exp: Option<i64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewToken {
    kind: String,
    space: String,
    document: String,
    credential: Credential,
    proposal: Proposal,
    source_seal: String,
    iat: i64,
    exp: i64,
}
fn changed() -> AppError {
    AppError::Conflict(
        "Restriction preview is stale or unavailable; inspect again before confirming".into(),
    )
}
fn unavailable() -> AppError {
    AppError::NotFound("Restriction target unavailable".into())
}
fn invalid() -> AppError {
    AppError::BadRequest("Restriction proposal must use inherited or restricted mode and unique exact user/group identifiers".into())
}
fn normalize(mut p: Proposal) -> Result<Proposal, AppError> {
    if !matches!(p.mode.as_str(), "inherited" | "restricted")
        || p.allowlist.users.len() + p.allowlist.groups.len() > 100
    {
        return Err(invalid());
    }
    for ids in [&mut p.allowlist.users, &mut p.allowlist.groups] {
        if ids
            .iter()
            .any(|id| id.is_empty() || id.trim() != id || id.len() > 256)
        {
            return Err(invalid());
        }
        ids.sort();
        if ids.windows(2).any(|v| v[0] == v[1]) {
            return Err(invalid());
        }
    }
    if p.mode == "inherited" && (!p.allowlist.users.is_empty() || !p.allowlist.groups.is_empty()) {
        return Err(invalid());
    }
    Ok(p)
}
fn credential(auth: &AuthUser) -> Result<Credential, AppError> {
    access::principal_from_auth(auth)?;
    Ok(Credential {
        actor: auth.claims.sub.clone(),
        identity: auth.claims.sid.clone(),
        bearer_exp: if auth.api_key_scopes.is_none() {
            Some(auth.claims.exp)
        } else {
            None
        },
    })
}
fn keyed(secret: &str, domain: &[u8], bytes: &[u8]) -> Vec<u8> {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key length");
    mac.update(domain);
    mac.update(bytes);
    mac.finalize().into_bytes().to_vec()
}
fn token_key(state: &AppState) -> Vec<u8> {
    keyed(
        &state.jwt_secret,
        b"restriction-preview-signing-key-v1",
        b"",
    )
}
fn seal(state: &AppState, value: &Value) -> Result<String, AppError> {
    Ok(hex::encode(keyed(
        &state.jwt_secret,
        b"restriction-preview-source-v1",
        &serde_json::to_vec(value).map_err(AppError::internal)?,
    )))
}
async fn rows(conn: &mut AnyConnection, q: &str, args: &[&str]) -> Result<Vec<Value>, AppError> {
    super::document_deletion::rows(conn, q, args)
        .await
        .map_err(AppError::internal)
}
async fn source(conn: &mut AnyConnection, auth: &AuthUser, space: &str) -> Result<Value, AppError> {
    let actor = &auth.claims.sub;
    let mut out = serde_json::Map::new();
    for (name,q,args) in [
 ("pages","SELECT * FROM documents WHERE space_id=$1 ORDER BY id",vec![space]),
 ("graph","SELECT id,space_id,parent_id FROM documents ORDER BY id",vec![]),
 ("space","SELECT * FROM spaces WHERE id=$1",vec![space]),
 ("users","SELECT id,role,is_active,display_name FROM users ORDER BY id",vec![]),
 ("direct","SELECT * FROM space_members WHERE space_id=$1 ORDER BY id",vec![space]),
 ("blocks","SELECT * FROM space_user_blocks WHERE space_id=$1 ORDER BY user_id",vec![space]),
 ("space_roles","SELECT * FROM space_roles WHERE space_id=$1 ORDER BY id",vec![space]),
 ("groups","SELECT * FROM groups ORDER BY id",vec![]),
 ("members","SELECT * FROM group_members ORDER BY id",vec![]),
 ("group_grants","SELECT * FROM space_group_permissions ORDER BY id",vec![]),
 ("global_grants","SELECT * FROM group_global_roles ORDER BY group_id,role_id",vec![]),
 ("global_roles","SELECT * FROM global_roles ORDER BY id",vec![]),
 ("policies","SELECT * FROM document_access_policies WHERE space_id=$1 ORDER BY document_id",vec![space]),
 ("allowlists","SELECT * FROM document_access_allowlist WHERE space_id=$1 ORDER BY id",vec![space]),
 ("drafts","SELECT * FROM document_drafts WHERE space_id=$1 ORDER BY id",vec![space]),
 ("shares","SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.space_id=$1 ORDER BY a.draft_id,a.user_id",vec![space]),
 ("keys","SELECT id,user_id,is_service_account,scopes,expires_at FROM api_keys WHERE user_id=$1 ORDER BY id",vec![actor.as_str()]),
 ("session","SELECT id,user_id,device_name,session_kind,client_platform,ip_address,user_agent,created_at,refresh_token_hash FROM sessions WHERE id=$1 AND user_id=$2",vec![auth.claims.sid.as_str(),actor.as_str()]),
 ]{out.insert(name.into(),json!(rows(conn,q,&args).await?));}
    out.insert("viewer_grants".into(),super::page_viewer_grants::capture_space_dependencies(conn,space).await?);
    out.insert(
        "public_enabled".into(),
        json!(super::documents::public_pages_enabled()),
    );
    Ok(Value::Object(out))
}
fn array<'a>(s: &'a Value, k: &str) -> Result<&'a Vec<Value>, AppError> {
    s[k].as_array().ok_or_else(changed)
}
fn page_ids(s: &Value, root: &str, space: &str) -> Result<Vec<String>, AppError> {
    let pages = array(s, "pages")?;
    if !pages.iter().any(|p| p["id"] == root) {
        return Err(unavailable());
    }
    let mut result = BTreeSet::from([root.to_string()]);
    loop {
        let mut added = false;
        for p in array(s, "graph")? {
            if let Some(parent) = p["parent_id"].as_str() {
                if result.contains(parent) {
                    if p["space_id"] != space {
                        return Err(changed());
                    }
                    let id = p["id"].as_str().ok_or_else(changed)?;
                    if id == root {
                        return Err(changed());
                    }
                    added |= result.insert(id.into());
                }
            }
        }
        if !added {
            break;
        }
    }
    Ok(result.into_iter().collect())
}
async fn policy(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    op: PageOperation,
) -> Result<(), AppError> {
    access::resolve_page_access(
        conn,
        access::principal_from_auth(auth)?,
        space,
        id,
        op,
        Purpose::Routine,
    )
    .await
    .map(|_| ())
}
fn validate_targets(s: &Value, p: &Proposal, space: &str, actor: &str) -> Result<(), AppError> {
    for id in &p.allowlist.users {
        if !array(s, "users")?
            .iter()
            .any(|u| u["id"] == *id && u["is_active"] == 1)
            || array(s, "blocks")?.iter().any(|b| b["user_id"] == *id)
            || (id != actor
                && !array(s, "direct")?.iter().any(|m| m["user_id"] == *id)
                && !array(s, "members")?.iter().any(|m| {
                    m["user_id"] == *id
                        && array(s, "group_grants").is_ok_and(|grants| {
                            grants
                                .iter()
                                .any(|g| g["space_id"] == space && g["group_id"] == m["group_id"])
                        })
                        && array(s, "groups").is_ok_and(|groups| {
                            groups.iter().any(|g| {
                                g["id"] == m["group_id"]
                                    && (g["managed_space_id"].is_null()
                                        || g["managed_space_id"] == space)
                            })
                        })
                }))
        {
            return Err(unavailable());
        }
    }
    for id in &p.allowlist.groups {
        let group = array(s, "groups")?
            .iter()
            .find(|g| g["id"] == *id)
            .ok_or_else(unavailable)?;
        if !group["managed_space_id"].is_null() && group["managed_space_id"] != space {
            return Err(unavailable());
        }
        if group["managed_space_id"] != space
            && !array(s, "group_grants")?
                .iter()
                .any(|g| g["group_id"] == *id && g["space_id"] == space)
        {
            return Err(unavailable());
        }
    }
    if p.mode == "restricted"
        && !p.allowlist.users.iter().any(|u| u == actor)
        && !array(s, "members")?.iter().any(|m| {
            m["user_id"] == actor && p.allowlist.groups.iter().any(|g| m["group_id"] == *g)
        })
    {
        return Err(AppError::Conflict("This change would remove your own page access; keep yourself in the allowlist before confirming".into()));
    }
    Ok(())
}
fn anonymous_after(
    s: &Value,
    id: &str,
    root: &str,
    p: Option<&Proposal>,
) -> Result<bool, AppError> {
    let pages = array(s, "pages")?;
    let mut next = Some(id.to_string());
    let mut seen = BTreeSet::new();
    let mut first = true;
    while let Some(id) = next {
        if !seen.insert(id.clone()) {
            return Err(changed());
        }
        let row = pages.iter().find(|x| x["id"] == id).ok_or_else(changed)?;
        if !row["deleted_at"].is_null() || !row["archived_at"].is_null() {
            return Ok(false);
        }
        if first {
            first = false;
            if row["published_at"].is_null()
                || row["public"] != 1
                || row["public_slug"].as_str().is_none_or(|v| v.is_empty())
                || s["public_enabled"] != true
            {
                return Ok(false);
            }
        }
        let mode = if id == root {
            p.map(|x| x.mode.as_str())
        } else {
            None
        }
        .or_else(|| {
            s["policies"]
                .as_array()?
                .iter()
                .find(|x| x["document_id"] == id)
                .and_then(|x| x["mode"].as_str())
        })
        .unwrap_or("inherited");
        if mode == "restricted" {
            return Ok(false);
        }
        next = row["parent_id"].as_str().map(str::to_string);
    }
    Ok(true)
}
async fn impact(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    root: &str,
    p: &Proposal,
    s: &Value,
) -> Result<Vec<Value>, AppError> {
    let mut visible = vec![];
    for id in page_ids(s, root, space)? {
        let row = array(s, "pages")?
            .iter()
            .find(|v| v["id"] == id)
            .ok_or_else(changed)?;
        if rows(
            conn,
            "SELECT * FROM documents WHERE id=$1 AND space_id=$2",
            &[&id, space],
        )
        .await?
            != vec![row.clone()]
        {
            return Err(changed());
        }
        let op = if !row["deleted_at"].is_null() {
            PageOperation::InspectTrashedMetadata
        } else if !row["archived_at"].is_null() {
            PageOperation::InspectArchivedMetadata
        } else {
            PageOperation::View
        };
        match policy(conn,auth,space,&id,op).await{Ok(())=>visible.push(json!({"document_id":id,"public_link_configured":row["public"]==1&&!row["public_slug"].is_null(),"public_link_policy_before":anonymous_after(s,&id,root,None)?,"public_link_policy_after":anonymous_after(s,&id,root,Some(p))?})),Err(AppError::NotFound(_))=>{},Err(e)=>return Err(e)}
    }
    Ok(visible)
}
// Eligible catalog entries use the mutation validator, not a second role policy.
fn eligible_recipient(s: &Value, space: &str, actor: &str, id: &str, group: bool) -> Result<bool, AppError> {
    let mut users = vec![actor.to_string()];
    if !group && id != actor { users.push(id.to_string()); }
    let proposal = Proposal {
        mode: "restricted".into(),
        allowlist: Allowlist { users, groups: if group { vec![id.to_string()] } else { vec![] } },
    };
    match validate_targets(s, &proposal, space, actor) {
        Ok(()) => Ok(true),
        Err(AppError::NotFound(_)) => Ok(false),
        Err(e) => Err(e),
    }
}
fn inspection_projection(s: &Value, space: &str, id: &str, actor: &str) -> Result<Value, AppError> {
    let mut users = vec![];
    let mut groups = vec![];
    for user in array(s, "users")? {
        let uid = user["id"].as_str().ok_or_else(changed)?;
        if eligible_recipient(s, space, actor, uid, false)? {
            users.push(json!({"id":uid,"display_name":user["display_name"]}));
        }
    }
    for group in array(s, "groups")? {
        let gid = group["id"].as_str().ok_or_else(changed)?;
        if eligible_recipient(s, space, actor, gid, true)? {
            groups.push(json!({"id":gid,"name":group["name"]}));
        }
    }
    let local = array(s, "policies")?.iter().find(|p| p["document_id"] == id);
    let mut selected_users = vec![];
    let mut selected_groups = vec![];
    for entry in array(s, "allowlists")?.iter().filter(|p| p["document_id"] == id) {
        match (entry["user_id"].as_str(), entry["group_id"].as_str()) {
            (Some(uid), None) => selected_users.push(json!({"id":uid,"available":users.iter().any(|u| u["id"] == uid)})),
            (None, Some(gid)) => selected_groups.push(json!({"id":gid,"available":groups.iter().any(|g| g["id"] == gid)})),
            _ => return Err(changed()),
        }
    }
    for list in [&mut users, &mut groups, &mut selected_users, &mut selected_groups] {
        list.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    }
    Ok(json!({
        "policy": {"mode":local.map(|p| p["mode"].clone()).unwrap_or(json!("inherited")),
            "revision":local.map(|p| p["revision"].clone()).unwrap_or(Value::Null),
            "allowlist":{"users":selected_users,"groups":selected_groups}},
        "eligible_recipients":{"users":users,"groups":groups}
    }))
}
async fn inspection_can_apply(conn: &mut AnyConnection, auth: &AuthUser, space: &str, id: &str) -> Result<bool, AppError> {
    match policy(conn, auth, space, id, PageOperation::RestrictionApply).await {
        Ok(()) => Ok(true),
        Err(AppError::NotFound(_)) | Err(AppError::Forbidden(_)) => Ok(false),
        Err(e) => Err(e),
    }
}
async fn inspect(
    State(state): State<AppState>,
    ReadAuth(auth): ReadAuth,
    Path((space, id)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    policy(&mut conn, &auth, &space, &id, PageOperation::RestrictionPreview).await?;
    let captured = source(&mut conn, &auth, &space).await?;
    let mut response = inspection_projection(&captured, &space, &id, &auth.claims.sub)?;
    let can_apply = inspection_can_apply(&mut conn, &auth, &space, &id).await?;
    #[cfg(test)]
    inspect_after_capability().await;
    if source(&mut conn, &auth, &space).await? != captured { return Err(changed()); }
    #[cfg(test)]
    inspect_after_source().await;
    policy(&mut conn, &auth, &space, &id, PageOperation::RestrictionPreview).await?;
    if inspection_can_apply(&mut conn, &auth, &space, &id).await? != can_apply { return Err(changed()); }
    // Rebind all projected catalog/local-policy fields after the final capability waits.
    let final_source = source(&mut conn, &auth, &space).await?;
    if final_source != captured || inspection_projection(&final_source, &space, &id, &auth.claims.sub)? != response { return Err(changed()); }
    #[cfg(test)]
    inspect_before_identity().await;
    access::require_current_human_read(&mut conn, &auth).await?;
    if Some(super::documents::public_pages_enabled()) != captured["public_enabled"].as_bool() { return Err(changed()); }
    let out = response.as_object_mut().ok_or_else(changed)?;
    out.insert("schema_version".into(), json!(1));
    out.insert("space_id".into(), json!(space));
    out.insert("document_id".into(), json!(id));
    out.insert("checked_at".into(), json!(chrono::Utc::now().to_rfc3339()));
    out.insert("can_apply".into(), json!(can_apply));
    Ok(Json(response))
}
#[cfg(test)]
tokio::task_local! {
    static INSPECT_AFTER_CAPABILITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    static INSPECT_AFTER_SOURCE:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    static INSPECT_BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
}
#[cfg(test)]
async fn inspect_after_capability() { if let Ok(Some(h)) = INSPECT_AFTER_CAPABILITY.try_with(|s| s.borrow_mut().take()) { h.await; } }
#[cfg(test)]
async fn inspect_after_source() { if let Ok(Some(h)) = INSPECT_AFTER_SOURCE.try_with(|s| s.borrow_mut().take()) { h.await; } }
#[cfg(test)]
async fn inspect_before_identity() { if let Ok(Some(h)) = INSPECT_BEFORE_IDENTITY.try_with(|s| s.borrow_mut().take()) { h.await; } }
async fn preview(
    State(state): State<AppState>,
    ReadAuth(auth): ReadAuth,
    Path((space, id)): Path<(String, String)>,
    Json(p): Json<Proposal>,
) -> Result<Json<Value>, AppError> {
    let p = normalize(p)?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    policy(
        &mut conn,
        &auth,
        &space,
        &id,
        PageOperation::RestrictionPreview,
    )
    .await?;
    let before = source(&mut conn, &auth, &space).await?;
    validate_targets(&before, &p, &space, &auth.claims.sub)?;
    let visible = impact(&mut conn, &auth, &space, &id, &p, &before).await?;
    #[cfg(test)]
    after_preview().await;
    let current = source(&mut conn, &auth, &space).await?;
    if current != before {
        return Err(changed());
    }
    policy(
        &mut conn,
        &auth,
        &space,
        &id,
        PageOperation::RestrictionPreview,
    )
    .await?;
    // Visibility is rechecked after inventory waits; selected rows contain no draft labels.
    if source(&mut conn, &auth, &space).await? != current {
        return Err(changed());
    }
    #[cfg(test)]
    after_final_source().await;
    let final_visible = impact(&mut conn, &auth, &space, &id, &p, &current).await?;
    if final_visible != visible {
        return Err(changed());
    }
    policy(
        &mut conn,
        &auth,
        &space,
        &id,
        PageOperation::RestrictionPreview,
    )
    .await?;
    access::require_current_human_read(&mut conn, &auth).await?;
    if Some(super::documents::public_pages_enabled()) != current["public_enabled"].as_bool() {
        return Err(changed());
    }
    let now = chrono::Utc::now().timestamp();
    let claims = PreviewToken {
        kind: KIND.into(),
        space: space.clone(),
        document: id.clone(),
        credential: credential(&auth)?,
        proposal: p,
        source_seal: seal(&state, &current)?,
        iat: now,
        exp: now + 600,
    };
    let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
    header.kid = Some(KIND.into());
    let token = jsonwebtoken::encode(
        &header,
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(&token_key(&state)),
    )
    .map_err(AppError::internal)?;
    Ok(Json(
        json!({"schema_version":1,"space_id":space,"document_id":id,"preview_token":token,"expires_at":chrono::DateTime::from_timestamp(now+600,0).unwrap().to_rfc3339(),"scope_notice":NOTICE,"readable_pages":visible}),
    ))
}
fn decode(
    state: &AppState,
    c: &Confirmation,
    auth: &AuthUser,
    space: &str,
    id: &str,
) -> Result<PreviewToken, AppError> {
    if !c.confirm || c.preview_token.len() > 65536 {
        return Err(invalid());
    }
    let h = jsonwebtoken::decode_header(&c.preview_token).map_err(|_| changed())?;
    if h.alg != jsonwebtoken::Algorithm::HS256 || h.kid.as_deref() != Some(KIND) {
        return Err(changed());
    }
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.leeway = 0;
    let t = jsonwebtoken::decode::<PreviewToken>(
        &c.preview_token,
        &jsonwebtoken::DecodingKey::from_secret(&token_key(state)),
        &validation,
    )
    .map_err(|_| changed())?
    .claims;
    if t.kind != KIND
        || t.space != space
        || t.document != id
        || t.credential != credential(auth)?
        || t.iat > chrono::Utc::now().timestamp()
        || t.exp - t.iat != 600
        || normalize(t.proposal.clone())? != t.proposal
    {
        return Err(changed());
    }
    Ok(t)
}
async fn apply(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space, id)): Path<(String, String)>,
    Json(c): Json<Confirmation>,
) -> Result<Json<Value>, AppError> {
    let token = decode(&state, &c, &auth, &space, &id)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space]).await?;
    #[cfg(test)]
    after_guard().await;
    policy(&mut tx, &auth, &space, &id, PageOperation::RestrictionApply).await?;
    let before = source(&mut tx, &auth, &space).await?;
    if seal(&state, &before)? != token.source_seal {
        return Err(changed());
    }
    validate_targets(&before, &token.proposal, &space, &auth.claims.sub)?;
    for page in page_ids(&before, &id, &space)? {
        sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2")
            .bind(page)
            .bind(&space)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    }
    // Canonical target/group/participation rows follow the ordered space fence.
    for group in &token.proposal.allowlist.groups {
        sqlx::query("UPDATE groups SET id=id WHERE id=$1")
            .bind(group)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
        sqlx::query("UPDATE group_members SET id=id WHERE group_id=$1")
            .bind(group)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    }
    for user in &token.proposal.allowlist.users {
        sqlx::query("UPDATE users SET id=id WHERE id=$1")
            .bind(user)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    }
    sqlx::query("UPDATE document_drafts SET id=id WHERE space_id=$1")
        .bind(&space)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    sqlx::query("UPDATE document_draft_access SET draft_id=draft_id WHERE draft_id IN(SELECT id FROM document_drafts WHERE space_id=$1)").bind(&space).execute(&mut *tx).await.map_err(AppError::internal)?;
    sqlx::query("UPDATE document_access_allowlist SET id=id WHERE document_id=$1 AND space_id=$2")
        .bind(&id)
        .bind(&space)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    sqlx::query("UPDATE document_access_policies SET revision=revision WHERE document_id=$1 AND space_id=$2").bind(&id).bind(&space).execute(&mut *tx).await.map_err(AppError::internal)?;
    if source(&mut tx, &auth, &space).await? != before {
        return Err(changed());
    }
    policy(&mut tx, &auth, &space, &id, PageOperation::RestrictionApply).await?;
    let mut expected = before.clone();
    let revision = uuid::Uuid::new_v4().to_string();
    let stamp = chrono::Utc::now().to_rfc3339();
    let existing = array(&before, "policies")?
        .iter()
        .find(|r| r["document_id"] == id);
    let mut new=existing.cloned().unwrap_or_else(||json!({"document_id":id,"space_id":space,"created_by":auth.claims.sub,"created_at":stamp}));
    new["mode"] = json!(token.proposal.mode);
    new["revision"] = json!(revision);
    new["updated_at"] = json!(stamp);
    expected["policies"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["document_id"] != id);
    expected["policies"]
        .as_array_mut()
        .unwrap()
        .push(new.clone());
    expected["policies"]
        .as_array_mut()
        .unwrap()
        .sort_by_cached_key(Value::to_string);
    sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$6) ON CONFLICT(document_id) DO UPDATE SET mode=excluded.mode,revision=excluded.revision,updated_at=excluded.updated_at").bind(&id).bind(&space).bind(&token.proposal.mode).bind(&revision).bind(&auth.claims.sub).bind(&stamp).execute(&mut *tx).await.map_err(AppError::internal)?;
    sqlx::query("DELETE FROM document_access_allowlist WHERE document_id=$1 AND space_id=$2")
        .bind(&id)
        .bind(&space)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    expected["allowlists"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["document_id"] != id);
    for (user, group) in token
        .proposal
        .allowlist
        .users
        .iter()
        .map(|u| (Some(u.as_str()), None))
        .chain(
            token
                .proposal
                .allowlist
                .groups
                .iter()
                .map(|g| (None, Some(g.as_str()))),
        )
    {
        let entry = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,group_id,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(&entry).bind(&id).bind(&space).bind(user).bind(group).bind(&auth.claims.sub).bind(&stamp).execute(&mut *tx).await.map_err(AppError::internal)?;
        expected["allowlists"].as_array_mut().unwrap().push(json!({"id":entry,"document_id":id,"space_id":space,"user_id":user,"group_id":group,"created_by":auth.claims.sub,"created_at":stamp}));
    }
    expected["allowlists"]
        .as_array_mut()
        .unwrap()
        .sort_by_cached_key(Value::to_string);
    let audit_id = uuid::Uuid::new_v4().to_string();
    let audit_time:String=sqlx::query_scalar("INSERT INTO audit_log(id,user_id,action,target_type,target_id) VALUES($1,$2,'document.restrictions_update','document',$3) RETURNING created_at").bind(&audit_id).bind(&auth.claims.sub).bind(&id).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    let audit = json!({"id":audit_id,"user_id":auth.claims.sub,"action":"document.restrictions_update","target_type":"document","target_id":id,"created_at":audit_time,"ip_address":null,"before_value":null,"after_value":null,"actor_kind":null,"actor_id":null});
    if rows(&mut tx, "SELECT * FROM audit_log WHERE id=$1", &[&audit_id]).await?
        != vec![audit.clone()]
    {
        return Err(changed());
    }
    if source(&mut tx, &auth, &space).await? != expected {
        return Err(changed());
    }
    policy(&mut tx, &auth, &space, &id, PageOperation::RestrictionApply).await?;
    #[cfg(test)]
    before_identity().await;
    access::require_current_human_preference_write(&mut tx, &auth).await?;
    if token.exp <= chrono::Utc::now().timestamp()
        || Some(super::documents::public_pages_enabled()) != expected["public_enabled"].as_bool()
    {
        return Err(changed());
    }
    tx.commit().await.map_err(AppError::internal)?;
    crate::audit::append_committed_move_action(&audit).await;
    Ok(Json(
        json!({"schema_version":1,"space_id":space,"document_id":id,"revision":revision,"mode":token.proposal.mode,"scope_notice":NOTICE}),
    ))
}
#[cfg(test)]
tokio::task_local! {static AFTER_PREVIEW:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;static AFTER_GUARD:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;static AFTER_FINAL_SOURCE:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;static BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;}
#[cfg(test)]
async fn after_preview() {
    if let Ok(Some(h)) = AFTER_PREVIEW.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}
#[cfg(test)]
async fn after_guard() {
    if let Ok(Some(h)) = AFTER_GUARD.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}
#[cfg(test)]
async fn after_final_source() {
    if let Ok(Some(h)) = AFTER_FINAL_SOURCE.try_with(|s| s.borrow_mut().take()) {
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
        extract::FromRequestParts,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;
    pub(super) async fn fixture(wal: bool) -> AppState {
        let mut state = crate::spaces::properties::tests::property_test_state().await;
        if wal {
            let path = format!("/tmp/restriction-{}.sqlite", uuid::Uuid::new_v4());
            state.db = sqlx::any::AnyPoolOptions::new()
                .max_connections(2)
                .after_connect(|c, _| {
                    Box::pin(async move {
                        sqlx::query("PRAGMA foreign_keys=ON")
                            .execute(&mut *c)
                            .await?;
                        sqlx::query("PRAGMA journal_mode=WAL")
                            .execute(&mut *c)
                            .await?;
                        sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;
                        Ok(())
                    })
                })
                .connect(&format!("sqlite://{path}?mode=rwc"))
                .await
                .unwrap();
            sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
            for id in ["owner-a", "viewer-a", "editor-a"] {
                sqlx::query(
                    "INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')",
                )
                .bind(id)
                .bind(format!("{id}@example.test"))
                .execute(&state.db)
                .await
                .unwrap();
                sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'metadata-test','2999-01-01T00:00:00Z')").bind(format!("metadata-{id}")).bind(id).execute(&state.db).await.unwrap();
            }
            for space in ["space-a", "space-b"] {
                sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,$1,$1,'private','owner-a')").bind(space).execute(&state.db).await.unwrap();
            }
            for (id, role) in [
                ("owner-a", "owner"),
                ("viewer-a", "viewer"),
                ("editor-a", "editor"),
            ] {
                sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'space-a',$2,$3)").bind(format!("member-{id}")).bind(id).bind(role).execute(&state.db).await.unwrap();
            }
            for (id, space) in [("target-a", "space-a"), ("target-b", "space-b")] {
                sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES($1,$2,$1,'','owner-a','owner-a','published')").bind(id).bind(space).execute(&state.db).await.unwrap();
            }
        }
        for (id, scope, actor) in [
            ("restrict-admin", "admin", "owner-a"),
            ("restrict-read", "read", "owner-a"),
            ("restrict-write", "write", "owner-a"),
            ("restrict-editor", "admin", "editor-a"),
        ] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,$2,'Restriction',$3,'test',$4)").bind(id).bind(actor).bind(crate::auth::api_keys::hash_api_key(id)).bind(format!("[\"{scope}\"]")).execute(&state.db).await.unwrap();
        }
        sqlx::query("INSERT INTO documents(id,space_id,parent_id,title,content_path,created_by,last_updated_by,published_at,public,public_slug) VALUES('child','space-a','target-a','READABLE CHILD','','owner-a','owner-a','published',1,'child-link'),('hidden-child','space-a','target-a','SECRET CHILD TITLE','','owner-a','owner-a',NULL,0,NULL)").execute(&state.db).await.unwrap();
        state
    }
    pub(super) fn proposal() -> Proposal {
        Proposal {
            mode: "restricted".into(),
            allowlist: Allowlist {
                users: vec!["owner-a".into()],
                groups: vec![],
            },
        }
    }
    fn req(stage: &str, key: &str, body: Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri(format!(
                "/spaces/space-a/documents/target-a/restrictions/{stage}"
            ))
            .header("X-API-Key", key)
            .header("Content-Type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    }
    pub(super) async fn call(
        state: &AppState,
        stage: &str,
        key: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let r = router()
            .with_state(state.clone())
            .oneshot(req(stage, key, body))
            .await
            .unwrap();
        let status = r.status();
        let bytes = to_bytes(r.into_body(), usize::MAX).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
    pub(super) async fn token(state: &AppState) -> String {
        let (status, v) = call(state, "preview", "restrict-admin", json!(proposal())).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        v["preview_token"].as_str().unwrap().into()
    }
    pub(super) async fn auth(state: &AppState, key: &str) -> AuthUser {
        let (mut parts, _) = req("apply", key, json!({})).into_parts();
        AuthUser::from_request_parts(&mut parts, state)
            .await
            .unwrap()
    }
    async fn snapshot(state: &AppState) -> Vec<(String, Option<String>)> {
        crate::spaces::documents::tests::move_database_snapshot(state).await
    }
    #[tokio::test]
    async fn restriction_http_read_preview_is_no_write_and_method_scopes_are_real() {
        let s = fixture(false).await;
        let before = snapshot(&s).await;
        let (status, v) = call(&s, "preview", "restrict-read", json!(proposal())).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert!(!v.to_string().contains("hidden-child"));
        assert!(!v.to_string().contains("SECRET"));
        assert_eq!(
            call(&s, "preview", "restrict-write", json!(proposal()))
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call(
                &s,
                "apply",
                "restrict-read",
                json!({"preview_token":v["preview_token"],"confirm":true})
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        sqlx::query("UPDATE api_keys SET last_used_at=NULL")
            .execute(&s.db)
            .await
            .unwrap();
        assert_eq!(snapshot(&s).await, before);
    }
    #[tokio::test]
    async fn restriction_http_apply_revision_replay_and_auth_token_domain_are_bound() {
        let s = fixture(false).await;
        let t = token(&s).await;
        assert!(crate::auth::jwt::validate_token(
            &t,
            &s.jwt_secret,
            crate::auth::jwt::TokenType::Access
        )
        .is_err());
        let (status, v) = call(
            &s,
            "apply",
            "restrict-admin",
            json!({"preview_token":t,"confirm":true}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let revision: String = sqlx::query_scalar(
            "SELECT revision FROM document_access_policies WHERE document_id='target-a'",
        )
        .fetch_one(&s.db)
        .await
        .unwrap();
        assert_eq!(v["revision"], revision);
        assert_eq!(
            call(
                &s,
                "apply",
                "restrict-admin",
                json!({"preview_token":t,"confirm":true})
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM audit_log WHERE action='document.restrictions_update'"
            )
            .fetch_one(&s.db)
            .await
            .unwrap(),
            1
        );
        let mut c = s.db.acquire().await.unwrap();
        assert!(
            access::require_anonymous_page_chain(&mut c, "space-a", "child")
                .await
                .is_err()
        );
        assert!(policy(
            &mut c,
            &crate::spaces::properties::tests::test_auth("viewer-a"),
            "space-a",
            "child",
            PageOperation::View
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn restriction_strict_inputs_self_loss_foreign_targets_and_nonmanager_fail_without_writes(
    ) {
        let s = fixture(false).await;
        for body in [
            json!({"mode":"restricted","allowlist":{"users":[],"groups":[]}}),
            json!({"mode":"inherited","allowlist":{"users":["owner-a"],"groups":[]}}),
            json!({"mode":"restricted","allowlist":{"users":["owner-a","owner-a"],"groups":[]}}),
            json!({"mode":"restricted","allowlist":{"users":["owner-a","missing"],"groups":[]}}),
            json!({"mode":"restricted","allowlist":{"users":["owner-a"],"groups":["missing"]}}),
            json!({"mode":"restricted","allowlist":{"users":["owner-a"],"groups":[]},"extra":true}),
        ] {
            let a = auth(&s, "restrict-admin").await;
            let before = snapshot(&s).await;
            let previous: Option<String> =
                sqlx::query_scalar("SELECT last_used_at FROM api_keys WHERE id='restrict-admin'")
                    .fetch_one(&s.db)
                    .await
                    .unwrap();
            let (status, _) = call(&s, "preview", "restrict-admin", body).await;
            assert!(!status.is_success());
            sqlx::query("UPDATE api_keys SET last_used_at=$1 WHERE id='restrict-admin'")
                .bind(previous)
                .execute(&s.db)
                .await
                .unwrap();
            drop(a);
            assert_eq!(snapshot(&s).await, before);
        }
        assert_eq!(
            call(&s, "preview", "restrict-editor", json!(proposal()))
                .await
                .0,
            StatusCode::NOT_FOUND
        );
    }
    #[tokio::test]
    async fn restriction_custom_manage_only_and_private_editor_are_not_edit_role_proxies() {
        let s = fixture(false).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('manager-only','space-a','Manager','[\"manage_space\"]','owner-a')").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='manager-only' WHERE user_id='owner-a'")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL,locked=1,locked_by='editor-a' WHERE id='target-a'").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('private-root','target-a','space-a','editor-a','SECRET','base')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('private-root','owner-a','viewer','editor-a')").execute(&s.db).await.unwrap();
        assert_eq!(
            call(&s, "preview", "restrict-admin", json!(proposal()))
                .await
                .0,
            StatusCode::NOT_FOUND
        );
        sqlx::query("UPDATE document_draft_access SET role='editor' WHERE draft_id='private-root' AND user_id='owner-a'").execute(&s.db).await.unwrap();
        let t = token(&s).await;
        assert_eq!(
            call(
                &s,
                "apply",
                "restrict-admin",
                json!({"preview_token":t,"confirm":true})
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    #[tokio::test]
    async fn restriction_real_wal_preview_late_block_and_source_change_are_current() {
        for sql in ["INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','editor-a')","UPDATE documents SET parent_id=NULL WHERE id='child'"]{let s=fixture(true).await;let a=auth(&s,"restrict-admin").await;let copied=s.clone();let result=AFTER_PREVIEW.scope(std::cell::RefCell::new(Some(Box::pin(async move{sqlx::query(sql).execute(&copied.db).await.unwrap();}))),preview(State(s.clone()),ReadAuth(a),Path(("space-a".into(),"target-a".into())),Json(proposal()))).await;assert!(result.is_err());}
    }
    #[tokio::test]
    async fn restriction_real_wal_apply_guard_wait_rechecks_original_key() {
        let s = fixture(true).await;
        let t = token(&s).await;
        let a = auth(&s, "restrict-admin").await;
        let mut hold = s.db.begin().await.unwrap();
        super::super::mutation_guards::lock_audience_for_mutation(&mut hold)
            .await
            .unwrap();
        let c = s.clone();
        let task = tokio::spawn(async move {
            apply(
                State(c),
                a,
                Path(("space-a".into(), "target-a".into())),
                Json(Confirmation {
                    preview_token: t,
                    confirm: true,
                }),
            )
            .await
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!task.is_finished());
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='restrict-admin'")
            .execute(&mut *hold)
            .await
            .unwrap();
        hold.commit().await.unwrap();
        let before = snapshot(&s).await;
        assert!(task.await.unwrap().is_err());
        assert_eq!(snapshot(&s).await, before);
    }
    #[tokio::test]
    async fn restriction_audit_or_source_trigger_tamper_rolls_back_whole_database() {
        for trigger in ["CREATE TRIGGER restriction_drift AFTER INSERT ON document_access_allowlist BEGIN UPDATE documents SET public_slug='Injected' WHERE id='child'; END","CREATE TRIGGER restriction_drift AFTER INSERT ON audit_log WHEN NEW.action='document.restrictions_update' BEGIN UPDATE audit_log SET before_value='Injected' WHERE id=NEW.id; END","CREATE TRIGGER restriction_drift BEFORE INSERT ON audit_log WHEN NEW.action='document.restrictions_update' BEGIN SELECT RAISE(ABORT,'audit denied'); END","CREATE TRIGGER restriction_drift AFTER INSERT ON document_access_policies BEGIN UPDATE users SET is_active=0 WHERE id='owner-a'; END"]{let s=fixture(false).await;let t=token(&s).await;sqlx::query(trigger).execute(&s.db).await.unwrap();let a=auth(&s,"restrict-admin").await;let before=snapshot(&s).await;assert!(apply(State(s.clone()),a,Path(("space-a".into(),"target-a".into())),Json(Confirmation{preview_token:t,confirm:true})).await.is_err());assert_eq!(snapshot(&s).await,before);}
    }
    #[tokio::test]
    async fn restriction_last_original_bearer_expiry_after_actual_sql_rolls_back() {
        let s = fixture(false).await;
        let mut claims = crate::spaces::properties::tests::test_auth("owner-a").claims;
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let raw = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes()),
        )
        .unwrap();
        async fn bearer(s: &AppState, t: &str) -> AuthUser {
            let (mut parts, _) = Request::builder()
                .method("POST")
                .header("Authorization", format!("Bearer {t}"))
                .body(Body::empty())
                .unwrap()
                .into_parts();
            AuthUser::from_request_parts(&mut parts, s).await.unwrap()
        }
        let a = bearer(&s, &raw).await;
        let v = preview(
            State(s.clone()),
            ReadAuth(a),
            Path(("space-a".into(), "target-a".into())),
            Json(proposal()),
        )
        .await
        .unwrap()
        .0;
        let a = bearer(&s, &raw).await;
        let before = snapshot(&s).await;
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let copied = reached.clone();
        let result = BEFORE_IDENTITY
            .scope(
                std::cell::RefCell::new(Some(Box::pin(async move {
                    copied.store(true, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }))),
                apply(
                    State(s.clone()),
                    a,
                    Path(("space-a".into(), "target-a".into())),
                    Json(Confirmation {
                        preview_token: v["preview_token"].as_str().unwrap().into(),
                        confirm: true,
                    }),
                ),
            )
            .await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));
        assert!(matches!(result, Err(AppError::Unauthorized(_))));
        assert_eq!(snapshot(&s).await, before);
    }
}
#[cfg(test)]
mod dependency_tests {
    use super::*;
    #[tokio::test]
    async fn restriction_ancestor_allowlist_and_spaceblock_never_gain_manager_bypass() {
        let s = crate::spaces::properties::tests::property_test_state().await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('parent','space-a','PRIVATE POLICY','','owner-a','owner-a','published'); UPDATE documents SET parent_id='parent' WHERE id='target-a'").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('parent','space-a','restricted','r','editor-a')").execute(&s.db).await.unwrap();
        let a = crate::spaces::properties::tests::test_auth("owner-a");
        let mut c = s.db.acquire().await.unwrap();
        assert!(policy(
            &mut c,
            &a,
            "space-a",
            "target-a",
            PageOperation::RestrictionApply
        )
        .await
        .is_err());
        sqlx::query("INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('entry','parent','space-a','owner-a','editor-a')").execute(&mut *c).await.unwrap();
        assert!(policy(
            &mut c,
            &a,
            "space-a",
            "target-a",
            PageOperation::RestrictionApply
        )
        .await
        .is_ok());
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','editor-a')").execute(&mut *c).await.unwrap();
        assert!(policy(
            &mut c,
            &a,
            "space-a",
            "target-a",
            PageOperation::RestrictionApply
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn restriction_group_scope_selection_is_not_a_grant() {
        let s = crate::spaces::properties::tests::property_test_state().await;
        sqlx::query("INSERT INTO groups(id,name,managed_space_id) VALUES('local','Local','space-a'),('foreign','SECRET','space-b'); INSERT INTO group_members(id,group_id,user_id) VALUES('membership','local','owner-a')").execute(&s.db).await.unwrap();
        let a = crate::spaces::properties::tests::test_auth("owner-a");
        let mut c = s.db.acquire().await.unwrap();
        let before = source(&mut c, &a, "space-a").await.unwrap();
        let p = Proposal {
            mode: "restricted".into(),
            allowlist: Allowlist {
                users: vec![],
                groups: vec!["local".into()],
            },
        };
        assert!(validate_targets(&before, &p, "space-a", "owner-a").is_ok());
        let p = Proposal {
            mode: "restricted".into(),
            allowlist: Allowlist {
                users: vec!["owner-a".into()],
                groups: vec!["foreign".into()],
            },
        };
        assert!(matches!(
            validate_targets(&before, &p, "space-a", "owner-a"),
            Err(AppError::NotFound(_))
        ));
        assert_eq!(
            normalize(Proposal {
                mode: "inherited".into(),
                allowlist: Allowlist {
                    users: vec![],
                    groups: vec![]
                }
            })
            .unwrap()
            .mode,
            "inherited"
        );
    }
}
#[cfg(test)]
mod final_boundary_tests {
    use super::*;
    #[tokio::test]
    async fn restriction_preview_actual_wal_after_inventory_current_manager_is_final() {
        let s = super::tests::fixture(true).await;
        let a = super::tests::auth(&s, "restrict-admin").await;
        let copied = s.clone();
        let result=AFTER_FINAL_SOURCE.scope(std::cell::RefCell::new(Some(Box::pin(async move{sqlx::query("UPDATE space_members SET role='viewer' WHERE user_id='owner-a' AND space_id='space-a'").execute(&copied.db).await.unwrap();}))),preview(State(s.clone()),ReadAuth(a),Path(("space-a".into(),"target-a".into())),Json(super::tests::proposal()))).await;
        assert!(result.is_err());
        let mut c = s.db.acquire().await.unwrap();
        assert!(policy(
            &mut c,
            &crate::spaces::properties::tests::test_auth("owner-a"),
            "space-a",
            "target-a",
            PageOperation::View
        )
        .await
        .is_ok());
    }
}

#[cfg(test)]
mod source_contract_tests {
    use super::*;
    #[tokio::test]
    async fn restriction_existing_multirow_policy_order_is_preserved() {
        let s = super::tests::fixture(false).await;
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by,created_at,updated_at) VALUES('child','space-a','restricted','child-revision','owner-a','2099','2099'),('hidden-child','space-a','restricted','hidden-revision','owner-a','1999','1999'); INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by,created_at) VALUES('a-entry','child','space-a','owner-a','owner-a','2099'),('z-entry','hidden-child','space-a','owner-a','owner-a','1999')").execute(&s.db).await.unwrap();
        let a = super::tests::auth(&s, "restrict-admin").await;
        let mut c = s.db.acquire().await.unwrap();
        let before = source(&mut c, &a, "space-a").await.unwrap();
        drop(c);
        let t = super::tests::token(&s).await;
        let (status, value) = super::tests::call(
            &s,
            "apply",
            "restrict-admin",
            json!({"preview_token":t,"confirm":true}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK, "{value}");
        let mut c = s.db.acquire().await.unwrap();
        let after = source(&mut c, &a, "space-a").await.unwrap();
        for key in ["policies", "allowlists"] {
            let old: Vec<_> = array(&before, key)
                .unwrap()
                .iter()
                .filter(|r| r["document_id"] != "target-a")
                .cloned()
                .collect();
            let current: Vec<_> = array(&after, key)
                .unwrap()
                .iter()
                .filter(|r| r["document_id"] != "target-a")
                .cloned()
                .collect();
            assert_eq!(old, current, "{key}");
        }
    }
    #[tokio::test]
    async fn restriction_people_are_scoped_active_participants_not_global_directory_oracles() {
        let s = super::tests::fixture(false).await;
        sqlx::query("INSERT INTO users(id,email,display_name,password_hash,role,is_active) VALUES('outsider','outsider@example.test','Outsider','x','user',1),('inactive','inactive@example.test','Inactive','x','user',0); INSERT INTO space_members(id,space_id,user_id,role) VALUES('inactive-member','space-a','inactive','viewer'); INSERT INTO groups(id,name) VALUES('scoped-people','Scoped'); INSERT INTO users(id,email,display_name,password_hash,role,is_active) VALUES('group-person','group@example.test','Group Person','x','user',1); INSERT INTO group_members(id,group_id,user_id) VALUES('group-person-member','scoped-people','group-person'); INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('group-person-grant','space-a','scoped-people','viewer')").execute(&s.db).await.unwrap();
        for id in ["outsider", "missing", "inactive"] {
            let p = json!({"mode":"restricted","allowlist":{"users":["owner-a",id],"groups":[]}});
            let (status, v) = super::tests::call(&s, "preview", "restrict-admin", p).await;
            assert_eq!(status, axum::http::StatusCode::NOT_FOUND, "{id} {v}");
        }
        for id in ["viewer-a", "group-person"] {
            let p = json!({"mode":"restricted","allowlist":{"users":["owner-a",id],"groups":[]}});
            assert_eq!(
                super::tests::call(&s, "preview", "restrict-admin", p)
                    .await
                    .0,
                axum::http::StatusCode::OK
            );
        }
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')").execute(&s.db).await.unwrap();
        assert_eq!(super::tests::call(&s,"preview","restrict-admin",json!({"mode":"restricted","allowlist":{"users":["owner-a","viewer-a"],"groups":[]}})).await.0,axum::http::StatusCode::NOT_FOUND);
    }
    #[tokio::test]
    async fn restriction_confirmation_other_key_expiry_and_postpreview_source_drift_reject() {
        for scenario in ["key", "expiry", "group", "ancestor", "subtree"] {
            let s = super::tests::fixture(false).await;
            let t = super::tests::token(&s).await;
            if scenario == "key" {
                sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('other-admin','owner-a','Other',$1,'test','[\"admin\"]')").bind(crate::auth::api_keys::hash_api_key("other-admin")).execute(&s.db).await.unwrap();
                let (status, _) = super::tests::call(
                    &s,
                    "apply",
                    "other-admin",
                    json!({"preview_token":t,"confirm":true}),
                )
                .await;
                assert_eq!(status, axum::http::StatusCode::CONFLICT);
                assert_eq!(
                    super::tests::call(
                        &s,
                        "apply",
                        "restrict-editor",
                        json!({"preview_token":t,"confirm":true})
                    )
                    .await
                    .0,
                    axum::http::StatusCode::CONFLICT
                );
                continue;
            }
            let mut t = t;
            match scenario {
                "expiry" => {
                    let a = super::tests::auth(&s, "restrict-admin").await;
                    let mut claims = decode(
                        &s,
                        &Confirmation {
                            preview_token: t,
                            confirm: true,
                        },
                        &a,
                        "space-a",
                        "target-a",
                    )
                    .unwrap();
                    claims.exp = chrono::Utc::now().timestamp() - 1;
                    claims.iat = claims.exp - 600;
                    let mut h = jsonwebtoken::Header::default();
                    h.kid = Some(KIND.into());
                    t = jsonwebtoken::encode(
                        &h,
                        &claims,
                        &jsonwebtoken::EncodingKey::from_secret(&token_key(&s)),
                    )
                    .unwrap();
                }
                "group" => {
                    sqlx::query("INSERT INTO groups(id,name) VALUES('new-group','New'); INSERT INTO group_members(id,group_id,user_id) VALUES('new-membership','new-group','owner-a')").execute(&s.db).await.unwrap();
                }
                "ancestor" => {
                    sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('new-parent','space-a','Parent','','owner-a','owner-a','published'); UPDATE documents SET parent_id='new-parent' WHERE id='target-a'").execute(&s.db).await.unwrap();
                }
                "subtree" => {
                    sqlx::query("UPDATE documents SET parent_id=NULL WHERE id='child'")
                        .execute(&s.db)
                        .await
                        .unwrap();
                }
                _ => unreachable!(),
            }
            let a = super::tests::auth(&s, "restrict-admin").await;
            let before = crate::spaces::documents::tests::move_database_snapshot(&s).await;
            assert!(
                apply(
                    State(s.clone()),
                    a,
                    Path(("space-a".into(), "target-a".into())),
                    Json(Confirmation {
                        preview_token: t,
                        confirm: true
                    })
                )
                .await
                .is_err(),
                "{scenario}"
            );
            assert_eq!(
                crate::spaces::documents::tests::move_database_snapshot(&s).await,
                before,
                "{scenario}"
            );
        }
    }
    #[test]
    fn restriction_final_public_config_change_runs_in_isolated_process() {
        let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--ignored","--exact","spaces::page_restrictions::source_contract_tests::restriction_public_config_change_child","--nocapture"]).env("ALLOW_PUBLIC_PAGES","true").output().unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[tokio::test]
    #[ignore = "isolated global configuration child invoked by parent test"]
    async fn restriction_public_config_change_child() {
        assert_eq!(std::env::var("ALLOW_PUBLIC_PAGES").unwrap(), "true");
        let s = super::tests::fixture(false).await;
        use tower::ServiceExt;
        let relative = format!(
            "spaces/restriction-public-{}/docs/child.md",
            uuid::Uuid::new_v4()
        );
        let file = crate::storage::data_dir().join(&relative);
        tokio::fs::create_dir_all(file.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&file, b"PUBLIC READER CONTROL BODY")
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET content_path=$1 WHERE id='child'")
            .bind(&relative)
            .execute(&s.db)
            .await
            .unwrap();
        async fn anonymous(s: &AppState) -> axum::http::StatusCode {
            super::super::documents::router()
                .with_state(s.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .uri("/p/child-link")
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap()
                .status()
        }
        assert_eq!(anonymous(&s).await, axum::http::StatusCode::OK);
        let token = super::tests::token(&s).await;
        assert_eq!(
            super::tests::call(
                &s,
                "apply",
                "restrict-admin",
                json!({"preview_token":token,"confirm":true})
            )
            .await
            .0,
            axum::http::StatusCode::OK
        );
        assert_eq!(anonymous(&s).await, axum::http::StatusCode::NOT_FOUND);
        let configured: (i64, String) =
            sqlx::query_as("SELECT public,public_slug FROM documents WHERE id='child'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(configured, (1, "child-link".into()));
        assert_eq!(
            tokio::fs::read(&file).await.unwrap(),
            b"PUBLIC READER CONTROL BODY"
        );
        let (status, preview_response) = super::tests::call(
            &s,
            "preview",
            "restrict-admin",
            json!({"mode":"inherited","allowlist":{"users":[],"groups":[]}}),
        )
        .await;
        assert_eq!(status, axum::http::StatusCode::OK);
        assert_eq!(
            super::tests::call(
                &s,
                "apply",
                "restrict-admin",
                json!({"preview_token":preview_response["preview_token"],"confirm":true})
            )
            .await
            .0,
            axum::http::StatusCode::OK
        );
        assert_eq!(anonymous(&s).await, axum::http::StatusCode::OK);
        tokio::fs::remove_file(file).await.unwrap();
        let a = super::tests::auth(&s, "restrict-admin").await;
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = reached.clone();
        let result = AFTER_FINAL_SOURCE
            .scope(
                std::cell::RefCell::new(Some(Box::pin(async move {
                    flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    std::env::set_var("ALLOW_PUBLIC_PAGES", "false");
                }))),
                preview(
                    State(s),
                    ReadAuth(a),
                    Path(("space-a".into(), "target-a".into())),
                    Json(super::tests::proposal()),
                ),
            )
            .await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));
        assert!(matches!(result, Err(AppError::Conflict(_))));
    }
}

#[cfg(test)]
mod inspection_tests {
    use super::*;
    use axum::{body::{to_bytes, Body}, http::{Request, StatusCode}, extract::FromRequestParts};
    use tower::ServiceExt;
    async fn http(s: &AppState, key: &str) -> (StatusCode, Value) {
        let r = router().with_state(s.clone()).oneshot(Request::builder().method("POST")
            .uri("/spaces/space-a/documents/target-a/restrictions/inspect")
            .header("X-API-Key",key).body(Body::empty()).unwrap()).await.unwrap();
        let status=r.status();
        let bytes=to_bytes(r.into_body(),usize::MAX).await.unwrap();
        (status,serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }
    async fn read_auth(s:&AppState,key:&str)->AuthUser {
        let (mut parts,_)=Request::builder().method("POST").header("X-API-Key",key).body(Body::empty()).unwrap().into_parts();
        ReadAuth::from_request_parts(&mut parts,s).await.unwrap().0
    }
    async fn snapshot(s:&AppState)->Vec<(String,Option<String>)> {
        crate::spaces::documents::tests::move_database_snapshot(s).await
    }
    #[tokio::test]
    async fn restriction_inspect_http_no_body_real_scopes_and_no_domain_writes() {
        let s=super::tests::fixture(false).await;
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes,is_service_account) VALUES('inspect-service','owner-a','Service',$1,'test','[\"admin\"]',1)")
            .bind(crate::auth::api_keys::hash_api_key("inspect-service")).execute(&s.db).await.unwrap();
        let before=snapshot(&s).await;
        for (key,can_apply) in [("restrict-read",false),("restrict-admin",true)] {
            let (status,v)=http(&s,key).await;
            assert_eq!(status,StatusCode::OK,"{v}");
            assert_eq!(v["can_apply"],can_apply);
            assert_eq!(v["schema_version"],1);
            assert_eq!(v["space_id"],"space-a"); assert_eq!(v["document_id"],"target-a");
            assert_eq!(v["policy"],json!({"mode":"inherited","revision":null,"allowlist":{"users":[],"groups":[]}}));
            assert!(chrono::DateTime::parse_from_rfc3339(v["checked_at"].as_str().unwrap()).is_ok());
            assert_eq!(v.as_object().unwrap().len(),7);
            assert!(!v.to_string().contains("hidden-child"));
            assert!(!v.to_string().contains("@"));
        }
        assert_eq!(http(&s,"restrict-write").await.0,StatusCode::FORBIDDEN);
        assert!(!http(&s,"inspect-service").await.0.is_success());
        // Only the real extractor's existing last_used bookkeeping may change.
        sqlx::query("UPDATE api_keys SET last_used_at=NULL").execute(&s.db).await.unwrap();
        assert_eq!(snapshot(&s).await,before);
    }
    #[tokio::test]
    async fn restriction_inspect_manage_only_private_editor_and_no_members_proxy() {
        let s=super::tests::fixture(false).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('inspect-manager','space-a','Manager','[\"manage_space\"]','owner-a'),('inspect-members','space-a','Members','[\"manage_members\"]','owner-a'); UPDATE space_members SET role='inspect-manager' WHERE user_id='owner-a'; UPDATE documents SET published_at=NULL,locked=1,locked_by='editor-a' WHERE id='target-a'; INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('inspect-private','target-a','space-a','editor-a','SECRET PRIVATE TITLE','base'); INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('inspect-private','owner-a','viewer','editor-a')").execute(&s.db).await.unwrap();
        assert_eq!(http(&s,"restrict-admin").await.0,StatusCode::NOT_FOUND);
        sqlx::query("UPDATE document_draft_access SET role='editor' WHERE draft_id='inspect-private' AND user_id='owner-a'").execute(&s.db).await.unwrap();
        let (status,v)=http(&s,"restrict-admin").await;
        assert_eq!(status,StatusCode::OK,"{v}"); assert_eq!(v["can_apply"],true);
        assert!(!v.to_string().contains("SECRET"));
        let a=read_auth(&s,"restrict-admin").await;
        let mut c=s.db.acquire().await.unwrap();
        assert!(policy(&mut c,&a,"space-a","target-a",PageOperation::View).await.is_err()); drop(c);
        sqlx::query("UPDATE space_members SET role='inspect-members' WHERE user_id='owner-a'").execute(&s.db).await.unwrap();
        assert_eq!(http(&s,"restrict-admin").await.0,StatusCode::NOT_FOUND);
    }
    #[tokio::test]
    async fn restriction_inspect_scoped_catalog_and_unavailable_local_ids_without_labels() {
        let s=super::tests::fixture(false).await;
        sqlx::query("INSERT INTO users(id,email,display_name,role,is_active) VALUES('inspect-inactive','inactive@example.test','SECRET INACTIVE LABEL','member',0),('inspect-outsider','outsider@example.test','SECRET OUTSIDER LABEL','member',1),('inspect-group-user','group@example.test','Group User','member',1); INSERT INTO space_members(id,space_id,user_id,role) VALUES('inspect-inactive-member','space-a','inspect-inactive','viewer'); INSERT INTO groups(id,name,managed_space_id,scim_managed) VALUES('inspect-scim','SCIM Scoped',NULL,1),('inspect-foreign','SECRET FOREIGN GROUP','space-b',0),('inspect-ungranted','SECRET UNGRANTED GROUP',NULL,0); INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('inspect-scim-grant','space-a','inspect-scim','viewer'); INSERT INTO group_members(id,group_id,user_id) VALUES('inspect-group-member','inspect-scim','inspect-group-user'); INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a'); INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('target-a','space-a','restricted','inspect-revision','owner-a'); INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,group_id,created_by) VALUES('inspect-owner-selection','target-a','space-a','owner-a',NULL,'owner-a'),('inspect-inactive-selection','target-a','space-a','inspect-inactive',NULL,'owner-a'),('inspect-block-selection','target-a','space-a','viewer-a',NULL,'owner-a'),('inspect-foreign-selection','target-a','space-a',NULL,'inspect-foreign','owner-a')").execute(&s.db).await.unwrap();
        let (status,v)=http(&s,"restrict-admin").await; assert_eq!(status,StatusCode::OK,"{v}");
        assert_eq!(v["policy"]["revision"],"inspect-revision");
        assert_eq!(v["policy"]["allowlist"]["users"],json!([{ "id":"inspect-inactive","available":false},{"id":"owner-a","available":true},{"id":"viewer-a","available":false}]));
        assert_eq!(v["policy"]["allowlist"]["groups"],json!([{ "id":"inspect-foreign","available":false}]));
        assert_eq!(v["eligible_recipients"]["groups"],json!([{ "id":"inspect-scim","name":"SCIM Scoped"}]));
        assert!(v["eligible_recipients"]["users"].as_array().unwrap().iter().any(|u|u["id"]=="inspect-group-user"));
        assert!(!v.to_string().contains("SECRET")); assert!(!v.to_string().contains("inspect-outsider"));
        assert!(!v.to_string().contains("inspect-ungranted"));
    }
    #[tokio::test]
    async fn restriction_inspect_ancestor_and_block_override_manager() {
        let s=super::tests::fixture(false).await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('inspect-parent','space-a','SECRET ANCESTOR','','owner-a','owner-a','published'); UPDATE documents SET parent_id='inspect-parent' WHERE id='target-a'; INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('inspect-parent','space-a','restricted','parent-revision','owner-a'); INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('inspect-parent-allow','inspect-parent','space-a','editor-a','owner-a')").execute(&s.db).await.unwrap();
        assert_eq!(http(&s,"restrict-admin").await.0,StatusCode::NOT_FOUND);
        sqlx::query("UPDATE document_access_allowlist SET user_id='owner-a' WHERE id='inspect-parent-allow'").execute(&s.db).await.unwrap();
        let (status,v)=http(&s,"restrict-admin").await; assert_eq!(status,StatusCode::OK,"{v}");
        assert_eq!(v["policy"]["revision"],Value::Null); assert!(!v.to_string().contains("inspect-parent"));
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','editor-a')").execute(&s.db).await.unwrap();
        assert_eq!(http(&s,"restrict-admin").await.0,StatusCode::NOT_FOUND);
    }
    #[tokio::test]
    async fn restriction_inspect_real_wal_inventory_drift_is_not_a_cached_projection() {
        for q in [
            "UPDATE users SET display_name='CHANGED ELIGIBLE LABEL' WHERE id='viewer-a'",
            "UPDATE space_members SET role='viewer' WHERE user_id='owner-a' AND space_id='space-a'",
            "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')",
        ] {
            let s=super::tests::fixture(true).await;
            let a=read_auth(&s,"restrict-admin").await; let copy=s.clone();
            let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)); let mark=reached.clone();
            let result=INSPECT_AFTER_CAPABILITY.scope(std::cell::RefCell::new(Some(Box::pin(async move {
                mark.store(true,std::sync::atomic::Ordering::SeqCst);
                sqlx::query(q).execute(&copy.db).await.unwrap();
            }))),inspect(State(s.clone()),ReadAuth(a),Path(("space-a".into(),"target-a".into())))).await;
            assert!(reached.load(std::sync::atomic::Ordering::SeqCst)); assert!(matches!(result,Err(AppError::Conflict(_))));
            assert_eq!(http(&s,"restrict-read").await.0,if q.contains("role="){StatusCode::NOT_FOUND}else{StatusCode::OK});
        }
    }
    #[tokio::test]
    async fn restriction_inspect_real_wal_final_manager_and_exact_private_editor_are_current() {
        for q in [
            "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','editor-a')",
            "UPDATE document_draft_access SET role='viewer' WHERE draft_id='inspect-private' AND user_id='owner-a'",
        ] {
            let s=super::tests::fixture(true).await;
            sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'; INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('inspect-private','target-a','space-a','editor-a','SECRET','base'); INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('inspect-private','owner-a','editor','editor-a')").execute(&s.db).await.unwrap();
            let a=read_auth(&s,"restrict-admin").await; let copy=s.clone();
            let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)); let mark=reached.clone();
            let result=INSPECT_AFTER_SOURCE.scope(std::cell::RefCell::new(Some(Box::pin(async move {
                mark.store(true,std::sync::atomic::Ordering::SeqCst);
                sqlx::query(q).execute(&copy.db).await.unwrap();
            }))),inspect(State(s.clone()),ReadAuth(a),Path(("space-a".into(),"target-a".into())))).await;
            assert!(reached.load(std::sync::atomic::Ordering::SeqCst)); assert!(matches!(result,Err(AppError::NotFound(_))));
            if q.contains("role=") {
                let a=read_auth(&s,"restrict-admin").await; let mut c=s.db.acquire().await.unwrap();
                assert!(policy(&mut c,&a,"space-a","target-a",PageOperation::View).await.is_ok());
            }
        }
    }
    #[tokio::test]
    async fn restriction_inspect_last_original_bearer_expiry_after_policy_has_no_writes() {
        let s=super::tests::fixture(false).await;
        let mut claims=crate::spaces::properties::tests::test_auth("owner-a").claims;
        claims.exp=chrono::Utc::now().timestamp()+2;
        let raw=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes())).unwrap();
        let (mut parts,_)=Request::builder().method("POST").header("Authorization",format!("Bearer {raw}")).body(Body::empty()).unwrap().into_parts();
        let a=ReadAuth::from_request_parts(&mut parts,&s).await.unwrap().0;
        let before=snapshot(&s).await;
        let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)); let mark=reached.clone();
        let result=INSPECT_BEFORE_IDENTITY.scope(std::cell::RefCell::new(Some(Box::pin(async move {
            mark.store(true,std::sync::atomic::Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }))),inspect(State(s.clone()),ReadAuth(a),Path(("space-a".into(),"target-a".into())))).await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst)); assert!(matches!(result,Err(AppError::Unauthorized(_))));
        assert_eq!(snapshot(&s).await,before);
    }
    #[tokio::test]
    async fn restriction_inspect_real_wal_late_key_downgrade_keeps_read_but_rejects_hint() {
        let s=super::tests::fixture(true).await;
        let a=read_auth(&s,"restrict-admin").await; let copy=s.clone();
        let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)); let mark=reached.clone();
        let result=INSPECT_AFTER_SOURCE.scope(std::cell::RefCell::new(Some(Box::pin(async move {
            mark.store(true,std::sync::atomic::Ordering::SeqCst);
            sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='restrict-admin'").execute(&copy.db).await.unwrap();
        }))),inspect(State(s.clone()),ReadAuth(a),Path(("space-a".into(),"target-a".into())))).await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst)); assert!(matches!(result,Err(AppError::Conflict(_))));
        let (status,v)=http(&s,"restrict-admin").await;
        assert_eq!(status,StatusCode::OK,"{v}"); assert_eq!(v["can_apply"],false);
    }
    #[tokio::test]
    async fn restriction_inspect_last_original_key_revocation_has_no_handler_writes() {
        let s=super::tests::fixture(true).await;
        let a=read_auth(&s,"restrict-admin").await; let copy=s.clone();
        let expected=std::sync::Arc::new(std::sync::Mutex::new(None)); let saved=expected.clone();
        let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)); let mark=reached.clone();
        let result=INSPECT_BEFORE_IDENTITY.scope(std::cell::RefCell::new(Some(Box::pin(async move {
            mark.store(true,std::sync::atomic::Ordering::SeqCst);
            sqlx::query("DELETE FROM api_keys WHERE id='restrict-admin'").execute(&copy.db).await.unwrap();
            let after=snapshot(&copy).await; *saved.lock().unwrap()=Some(after);
        }))),inspect(State(s.clone()),ReadAuth(a),Path(("space-a".into(),"target-a".into())))).await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst)); assert!(matches!(result,Err(AppError::Unauthorized(_))));
        assert_eq!(snapshot(&s).await,expected.lock().unwrap().clone().unwrap());
    }
    #[test]
    fn restriction_inspect_final_public_config_is_checked_in_isolated_child() {
        let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--ignored","--exact","spaces::page_restrictions::inspection_tests::restriction_inspect_config_child","--nocapture"]).env("ALLOW_PUBLIC_PAGES","true").output().unwrap();
        assert!(output.status.success(),"{}{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
    }
    #[tokio::test]
    #[ignore="isolated global configuration child invoked only by parent test"]
    async fn restriction_inspect_config_child() {
        assert!(super::super::documents::public_pages_enabled());
        let s=super::tests::fixture(false).await; let a=read_auth(&s,"restrict-admin").await;
        let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)); let mark=reached.clone();
        let result=INSPECT_BEFORE_IDENTITY.scope(std::cell::RefCell::new(Some(Box::pin(async move {
            mark.store(true,std::sync::atomic::Ordering::SeqCst);
            std::env::set_var("ALLOW_PUBLIC_PAGES","false");
        }))),inspect(State(s.clone()),ReadAuth(a),Path(("space-a".into(),"target-a".into())))).await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst)); assert!(matches!(result,Err(AppError::Conflict(_))));
        assert_eq!(http(&s,"restrict-read").await.0,StatusCode::OK);
    }

}
