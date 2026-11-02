//! Direct ordinary-user published-page viewer management, wired together with
//! published reader, source-witness and guarded-cascade adoption in spaces::router.
use super::{access::{self, PageOperation, Purpose}, published_source::{self, SourceProof}};
use crate::{auth::{middleware::{authenticate_request, ApiKeyRequestOperation, AuthError, AuthUser}, routes::AppError}, AppState};
use axum::{extract::{FromRef, FromRequestParts, Path, State}, http::request::Parts, Json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::AnyConnection;
use uuid::Uuid;

pub struct InspectionAuth(pub AuthUser);
impl<S> FromRequestParts<S> for InspectionAuth where S: Send + Sync, AppState: FromRef<S> {
    type Rejection = AuthError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        authenticate_request(parts, &AppState::from_ref(state), ApiKeyRequestOperation::Read).await.map(Self)
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action { Grant, Revoke }
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal { pub user_id: String, pub action: Action }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Confirmation { pub proposal: Proposal, pub source_digest: String, pub confirm: bool }
const GRANTS: &str = "SELECT * FROM document_viewer_grants WHERE document_id=$1 AND space_id=$2";
fn unavailable() -> AppError { AppError::NotFound("Page viewer management unavailable".into()) }
fn conflict() -> AppError { AppError::Conflict("Page viewer source changed; retain the original request and inspect current access".into()) }
async fn rows(conn: &mut AnyConnection, query: &str, args: &[&str]) -> Result<Vec<Value>, AppError> {
    super::document_deletion::rows(conn, query, args).await.map_err(AppError::internal)
}
pub(super) async fn authority(conn: &mut AnyConnection, auth: &AuthUser, space: &str, document: &str, write: bool, creating: bool) -> Result<Vec<String>, AppError> {
    if write {
        let ceiling = access::require_space_membership_management(conn, auth, space).await?;
        if creating && !ceiling.contains(&access::Permission::View) { return Err(unavailable()); }
    } else if creating {
        access::require_space_management_catalog_read(conn, auth, space).await?;
    }
    let page = access::resolve_page_access(conn, access::principal_from_auth(auth)?, space, document,
        if write { PageOperation::RestrictionApply } else { PageOperation::RestrictionPreview }, Purpose::Routine).await?;
    if !page.published { return Err(unavailable()); }
    let mut chain = vec![document.to_owned()]; chain.extend(page.ancestor_ids);
    // First viewer cut does not create an allowlist entitlement or access through
    // an independently private ancestor. Restrictions remain a separate workflow.
    if creating { for id in &chain {
        let active: Option<i64> = sqlx::query_scalar("SELECT 1 FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL")
            .bind(id).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        let policy: Option<String> = sqlx::query_scalar("SELECT mode FROM document_access_policies WHERE document_id=$1 AND space_id=$2")
            .bind(id).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        if active.is_none() || policy.as_deref().is_some_and(|mode| mode != "inherited") { return Err(unavailable()); }
    }
    }
    Ok(chain.into_iter().skip(1).collect())
}
pub(super) async fn recipient(conn: &mut AnyConnection, space: &str, user: &str, require_eligible: bool) -> Result<Value, AppError> {
    let mut selected = rows(conn, "SELECT id,display_name,email,role,is_active,account_kind FROM users WHERE id=$1", &[user]).await?;
    let value = selected.pop().ok_or_else(unavailable)?;
    if !require_eligible { return Ok(value); }
    if value["is_active"] != 1 || !(value["account_kind"] == "guest" || (value["account_kind"] == "member" && matches!(value["role"].as_str(), Some("member" | "viewer")))) || user.starts_with("service:") { return Err(unavailable()); }
    let blocked: Option<i64> = sqlx::query_scalar("SELECT 1 FROM space_user_blocks WHERE space_id=$1 AND user_id=$2")
        .bind(space).bind(user).fetch_optional(conn).await.map_err(AppError::internal)?;
    if blocked.is_some() { return Err(unavailable()); }
    Ok(value)
}
/// Reviewed moves borrow the existing account-management ceiling without
/// treating MoveContent or a method key as authority to grant page viewing.
pub(super) async fn move_space_authority(
    conn: &mut AnyConnection, auth: &AuthUser, space: &str,
    write: bool, creating: bool,
) -> Result<(), AppError> {
    if write {
        let account = access::require_space_membership_management(conn, auth, space).await?;
        if creating && !account.contains(&access::Permission::View) { return Err(unavailable()); }
    } else {
        access::require_space_management_catalog_read(conn, auth, space).await?;
    }
    Ok(())
}
async fn capture(conn: &mut AnyConnection, auth: &AuthUser, space: &str, document: &str, ancestors: &[String]) -> Result<Vec<SourceProof>, AppError> {
    let mut proof = published_source::capture_source(conn, auth, space, document, ancestors).await?;
    // The manager's existing entitled catalog permits these ordinary recipients;
    // bind their labels/kind/activity without fabricating recipient authentication.
    for (query, parameters) in [("SELECT id,display_name,email,role,is_active,account_kind FROM users".to_string(), vec![]),
        (GRANTS.to_string(), vec![document.to_string(), space.to_string()])] {
        let args: Vec<&str> = parameters.iter().map(String::as_str).collect();
        if !proof.iter().any(|p| p.query == query && p.parameters == parameters) {
            proof.push(SourceProof { rows: rows(conn, &query, &args).await?, query, parameters });
        }
    }
    for table in ["draft_birth_receipts", "draft_transfer_receipts", "draft_save_replay_receipts", "publication_receipts", "canonical_revision_receipts"] {
        let query = format!("SELECT * FROM {table} WHERE document_id=$1 AND space_id=$2");
        proof.push(SourceProof { rows: rows(conn, &query, &[document, space]).await?, query, parameters: vec![document.into(), space.into()] });
    }
    Ok(proof)
}
fn digest(auth: &AuthUser, space: &str, document: &str, proposal: &Proposal, proof: &[SourceProof]) -> Result<String, AppError> {
    let source = json!({"actor":auth.claims.sub,"space_id":space,"document_id":document,"proposal":proposal,
        "proof":proof.iter().map(|p|json!({"query":p.query,"parameters":p.parameters,"rows":p.rows})).collect::<Vec<_>>()});
    published_source::digest(&source, b"starlane-page-viewer-management-v1")
}
async fn verify(conn: &mut AnyConnection, auth: &AuthUser, space: &str, document: &str, proof: &[SourceProof], selected_recipient: Option<(&str, &Value, bool)>, write: bool) -> Result<(), AppError> {
    published_source::verify_source(conn, proof).await?;
    #[cfg(test)] after_inventory().await;
    authority(conn, auth, space, document, write, selected_recipient.is_some_and(|(_,_,eligible)|eligible)).await?;
    if let Some((user, expected, require_eligible)) = selected_recipient { if recipient(conn, space, user, require_eligible).await? != *expected { return Err(conflict()); } }
    // Reprove every captured raw page/ancestor/policy/receipt tuple after the
    // typed-policy waits; the original method credential is LAST.
    published_source::verify_source(conn, proof).await?;
    if write { access::require_current_human_preference_write(conn, auth).await?; }
    else { access::require_current_human_read(conn, auth).await?; }
    Ok(())
}
pub async fn inspect(State(state): State<AppState>, InspectionAuth(auth): InspectionAuth, Path((space, document)): Path<(String, String)>) -> Result<Json<Value>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let ancestors = authority(&mut conn, &auth, &space, &document, false, false).await?;
    let proof = capture(&mut conn, &auth, &space, &document, &ancestors).await?;
    let grants = rows(&mut conn, GRANTS, &[&document, &space]).await?;
    let mut projection = Vec::new();
    for grant in grants {
        let user = grant["user_id"].as_str().ok_or_else(conflict)?;
        let label = match recipient(&mut conn, &space, user, true).await {
            Ok(value) => Some(value), Err(AppError::NotFound(_)) => None, Err(error) => return Err(error),
        };
        projection.push(json!({"user_id":user,"revision":grant["revision"],"role":"viewer","scope":"page",
            "display_name":label.as_ref().map(|v|v["display_name"].clone()),"available":label.is_some()}));
    }
    verify(&mut conn, &auth, &space, &document, &proof, None, false).await?;
    Ok(Json(json!({"schema_version":1,"space_id":space,"document_id":document,"grants":projection,
        "scope_notice":"Published page only. No space membership, drafts, editing or access through restricted/private ancestors."})))
}
pub async fn review(State(state): State<AppState>, InspectionAuth(auth): InspectionAuth, Path((space, document)): Path<(String, String)>, Json(proposal): Json<Proposal>) -> Result<Json<Value>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let ancestors = authority(&mut conn, &auth, &space, &document, false, proposal.action == Action::Grant).await?;
    if proposal.action == Action::Revoke && !rows(&mut conn, GRANTS, &[&document,&space]).await?.iter().any(|g|g["user_id"]==proposal.user_id) { return Err(unavailable()); }
    let selected = recipient(&mut conn, &space, &proposal.user_id, proposal.action == Action::Grant).await?;
    let proof = capture(&mut conn, &auth, &space, &document, &ancestors).await?;
    let grants = proof.iter().find(|p|p.query==GRANTS).ok_or_else(conflict)?;
    let existing = grants.rows.iter().find(|v|v["user_id"]==proposal.user_id).cloned();
    if proposal.action == Action::Revoke && existing.is_none() { return Err(conflict()); }
    let source_digest = digest(&auth, &space, &document, &proposal, &proof)?;
    let visible = match recipient(&mut conn, &space, &proposal.user_id, true).await {
        Ok(_) => true, Err(AppError::NotFound(_)) => false, Err(error) => return Err(error),
    };
    verify(&mut conn, &auth, &space, &document, &proof, Some((&proposal.user_id, &selected, proposal.action == Action::Grant)), false).await?;
    Ok(Json(json!({"schema_version":1,"space_id":space,"document_id":document,"proposal":proposal,
        "source_digest":source_digest,"recipient":{"id":selected["id"],"display_name":if visible {selected["display_name"].clone()} else {Value::Null},"email":if visible {selected["email"].clone()} else {Value::Null}},"existing_grant":existing})))
}
pub async fn confirm(State(state): State<AppState>, auth: AuthUser, Path((space, document)): Path<(String, String)>, Json(request): Json<Confirmation>) -> Result<Json<Value>, AppError> {
    if !request.confirm || request.source_digest.len()!=64 || !request.source_digest.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) { return Err(conflict()); }
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space]).await?;
    let guarded = sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2").bind(&document).bind(&space).execute(&mut *tx).await.map_err(AppError::internal)?;
    if guarded.rows_affected()!=1 { return Err(unavailable()); }
    let ancestors = authority(&mut tx, &auth, &space, &document, true, request.proposal.action == Action::Grant).await?;
    if request.proposal.action == Action::Revoke && !rows(&mut tx, GRANTS, &[&document,&space]).await?.iter().any(|g|g["user_id"]==request.proposal.user_id) { return Err(unavailable()); }
    let selected = recipient(&mut tx, &space, &request.proposal.user_id, request.proposal.action == Action::Grant).await?;
    let mut proof = capture(&mut tx, &auth, &space, &document, &ancestors).await?;
    if digest(&auth, &space, &document, &request.proposal, &proof)? != request.source_digest { return Err(conflict()); }
    let original_users = proof.iter().find(|p|p.query=="SELECT id,display_name,email,role,is_active,account_kind FROM users").ok_or_else(conflict)?.rows.clone();
    let grants = proof.iter_mut().find(|p|p.query==GRANTS).ok_or_else(conflict)?;
    let previous = grants.rows.iter().find(|v|v["user_id"]==request.proposal.user_id).cloned();
    let stamp = chrono::Utc::now().to_rfc3339();
    let revision = Uuid::new_v4().to_string();
    let next = if request.proposal.action == Action::Grant {
        let id = previous.as_ref().and_then(|v|v["id"].as_str()).map(str::to_owned).unwrap_or_else(||Uuid::new_v4().to_string());
        let created = previous.as_ref().and_then(|v|v["created_at"].as_str()).unwrap_or(&stamp);
        let creator = previous.as_ref().and_then(|v|v["created_by"].as_str()).unwrap_or(&auth.claims.sub);
        let row = json!({"id":id,"document_id":document,"space_id":space,"user_id":request.proposal.user_id,"role":"viewer","scope":"page","revision":revision,"created_by":creator,"created_at":created,"updated_at":stamp});
        if let Some(old) = &previous {
            let count=sqlx::query("UPDATE document_viewer_grants SET revision=$1,updated_at=$2 WHERE id=$3 AND document_id=$4 AND space_id=$5 AND user_id=$6 AND revision=$7")
                .bind(&revision).bind(&stamp).bind(&id).bind(&document).bind(&space).bind(&request.proposal.user_id).bind(old["revision"].as_str().ok_or_else(conflict)?).execute(&mut *tx).await.map_err(AppError::internal)?;
            if count.rows_affected()!=1 { return Err(conflict()); }
        } else {
            sqlx::query("INSERT INTO document_viewer_grants(id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,'viewer','page',$5,$6,$7,$7)")
                .bind(&id).bind(&document).bind(&space).bind(&request.proposal.user_id).bind(&revision).bind(&auth.claims.sub).bind(&stamp).execute(&mut *tx).await.map_err(AppError::internal)?;
        }
        Some(row)
    } else {
        let old=previous.as_ref().ok_or_else(conflict)?;
        let count=sqlx::query("DELETE FROM document_viewer_grants WHERE id=$1 AND document_id=$2 AND space_id=$3 AND user_id=$4 AND revision=$5")
            .bind(old["id"].as_str().ok_or_else(conflict)?).bind(&document).bind(&space).bind(&request.proposal.user_id).bind(old["revision"].as_str().ok_or_else(conflict)?).execute(&mut *tx).await.map_err(AppError::internal)?;
        if count.rows_affected()!=1 { return Err(conflict()); } None
    };
    grants.rows.retain(|v|v["user_id"]!=request.proposal.user_id);
    if let Some(row)=&next { grants.rows.push(row.clone()); } grants.rows.sort_by_cached_key(Value::to_string);
    let expected_grants = grants.rows.clone();
    let recipient_query = "SELECT u.id,u.role,u.is_active,u.account_kind FROM users u WHERE EXISTS(SELECT 1 FROM document_viewer_grants g WHERE g.user_id=u.id AND g.document_id=$1 AND g.space_id=$2)";
    let mut expected_recipients: Vec<Value> = original_users.iter().filter(|u| expected_grants.iter().any(|g|g["user_id"]==u["id"]))
        .map(|u|json!({"id":u["id"],"role":u["role"],"is_active":u["is_active"],"account_kind":u["account_kind"]})).collect();
    expected_recipients.sort_by_cached_key(Value::to_string);
    for witness in &mut proof {
        if witness.query == GRANTS && witness.parameters == vec![document.clone(),space.clone()] { witness.rows = expected_grants.clone(); }
        if witness.query == recipient_query && witness.parameters == vec![document.clone(),space.clone()] { witness.rows = expected_recipients.clone(); }
    }
    let audit_id=Uuid::new_v4().to_string();
    let action=if request.proposal.action==Action::Grant {"document.page_viewer_grant"} else {"document.page_viewer_revoke"};
    let before=serde_json::to_string(&previous).map_err(AppError::internal)?;
    let after=serde_json::to_string(&next).map_err(AppError::internal)?;
    let created:String=sqlx::query_scalar("INSERT INTO audit_log(id,user_id,actor_kind,actor_id,action,target_type,target_id,before_value,after_value) VALUES($1,$2,'user',$2,$3,'document',$4,$5,$6) RETURNING created_at")
        .bind(&audit_id).bind(&auth.claims.sub).bind(action).bind(&document).bind(&before).bind(&after).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    let audit=json!({"id":audit_id,"user_id":auth.claims.sub,"actor_kind":"user","actor_id":auth.claims.sub,"action":action,"target_type":"document","target_id":document,"before_value":before,"after_value":after,"created_at":created,"ip_address":null});
    proof.push(SourceProof {query:"SELECT * FROM audit_log WHERE id=$1".into(),parameters:vec![audit_id],rows:vec![audit]});
    verify(&mut tx, &auth, &space, &document, &proof, Some((&request.proposal.user_id, &selected, request.proposal.action == Action::Grant)), true).await?;
    tx.commit().await.map_err(|_|AppError::Conflict("Page viewer acknowledgement unknown; retain the original request and inspect access without automatic retry".into()))?;
    Ok(Json(json!({"schema_version":1,"space_id":space,"document_id":document,"user_id":request.proposal.user_id,"action":request.proposal.action,"grant":next,"confirmed":true,"source_digest":request.source_digest})))
}
#[cfg(test)]
tokio::task_local! { static AFTER_INVENTORY: (std::sync::Arc<tokio::sync::Notify>,std::sync::Arc<tokio::sync::Notify>); }
#[cfg(test)]
async fn after_inventory() { if let Ok((reached,release))=AFTER_INVENTORY.try_with(|s|s.clone()) { reached.notify_one();release.notified().await; } }

/// Parent registers this router only after all first-cut readers are integrated.
pub fn router() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/spaces/{space}/documents/{document}/viewer-grants/inspect", axum::routing::post(inspect))
        .route("/spaces/{space}/documents/{document}/viewer-grants/review", axum::routing::post(review))
        .route("/spaces/{space}/documents/{document}/viewer-grants/confirm", axum::routing::post(confirm))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::{to_bytes, Body}, http::{Request, StatusCode}};
    use tower::ServiceExt;
    use sqlx::any::AnyPoolOptions;

    async fn fixture() -> AppState {
        let mut state = super::super::properties::tests::property_test_state().await;
        let path = std::env::temp_dir().join(format!("starlane-page-viewer-management-{}.sqlite",Uuid::new_v4()));
        state.db = AnyPoolOptions::new().max_connections(2).after_connect(|c,_|Box::pin(async move {
            sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *c).await?;
            sqlx::query("PRAGMA journal_mode=WAL").execute(&mut *c).await?;
            sqlx::query("PRAGMA busy_timeout=5000").execute(&mut *c).await?;Ok(())
        })).connect(&format!("sqlite://{}?mode=rwc",path.display())).await.unwrap();
        sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
        for (id,role) in [("owner-a","member"),("recipient","viewer"),("editor-a","member"),("foreign","member")] {
            sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,$3)").bind(id).bind(format!("{id}@example.test")).bind(role).execute(&state.db).await.unwrap();
        }
        for space in ["space-a","space-b"] {
            sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,$1,$1,'private','owner-a')").bind(space).execute(&state.db).await.unwrap();
        }
        for (id,role) in [("owner-a","owner"),("editor-a","editor")] {
            sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'space-a',$2,$3)").bind(format!("m-{id}")).bind(id).bind(role).execute(&state.db).await.unwrap();
        }
        for (id,space) in [("page-a","space-a"),("page-b","space-b")] {
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES($1,$2,$1,'unchanged-original-path','owner-a','owner-a','original-publication')").bind(id).bind(space).execute(&state.db).await.unwrap();
        }
        for (key,user,scope) in [("viewer-read","owner-a","read"),("viewer-write","owner-a","write"),("viewer-admin","owner-a","admin"),("viewer-editor","editor-a","admin"),("recipient-read","recipient","read")] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,$2,'Page viewer',$3,'test',$4)").bind(key).bind(user).bind(crate::auth::api_keys::hash_api_key(key)).bind(format!("[\"{scope}\"]")).execute(&state.db).await.unwrap();
        }
        let opened: String = sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name='main'").fetch_one(&state.db).await.unwrap();
        assert_eq!(std::fs::canonicalize(opened).unwrap(),std::fs::canonicalize(path).unwrap());
        state
    }
    fn request(stage: &str,key: &str,value: Value) -> Request<Body> {
        Request::builder().method("POST").uri(format!("/spaces/space-a/documents/page-a/viewer-grants/{stage}"))
            .header("X-API-Key",key).header("Content-Type","application/json").body(Body::from(value.to_string())).unwrap()
    }
    async fn auth(state: &AppState,key: &str) -> AuthUser {
        let (mut parts,_) = request("confirm",key,json!({})).into_parts();
        AuthUser::from_request_parts(&mut parts,state).await.unwrap()
    }
    async fn read_auth(state: &AppState,key: &str) -> AuthUser {
        let (mut parts,_) = request("inspect",key,json!({})).into_parts();
        InspectionAuth::from_request_parts(&mut parts,state).await.unwrap().0
    }
    async fn call(state: &AppState,stage: &str,key: &str,value: Value) -> (StatusCode,Value) {
        let response=router().with_state(state.clone()).oneshot(request(stage,key,value)).await.unwrap();
        let status=response.status();let bytes=to_bytes(response.into_body(),usize::MAX).await.unwrap();
        (status,serde_json::from_slice(&bytes).unwrap())
    }
    async fn checked(state: &AppState,action: Action) -> PageViewerRequestFixture {
        let proposal=Proposal {user_id:"recipient".into(),action};
        let a=read_auth(state,"viewer-read").await;
        let Json(value)=review(State(state.clone()),InspectionAuth(a),Path(("space-a".into(),"page-a".into())),Json(proposal.clone())).await.unwrap();
        PageViewerRequestFixture {proposal,source_digest:value["source_digest"].as_str().unwrap().into()}
    }
    struct PageViewerRequestFixture {proposal: Proposal,source_digest: String}
    impl PageViewerRequestFixture {fn request(self)->Confirmation {Confirmation{proposal:self.proposal,source_digest:self.source_digest,confirm:true}}}
    async fn snapshot(state: &AppState) -> Vec<(String,Option<String>)> {super::super::documents::tests::move_database_snapshot(state).await}
    async fn grant(state: &AppState) {
        let original=auth(state,"viewer-write").await;
        let request=checked(state,Action::Grant).await;
        confirm(State(state.clone()),original,Path(("space-a".into(),"page-a".into())),Json(request.request())).await.unwrap();
    }
    #[tokio::test]
    async fn actual_http_nonmember_review_write_only_confirmation_and_exact_audit() {
        let s=fixture().await;
        let response=super::super::routes::router().with_state(s.clone()).oneshot(Request::builder().uri("/spaces/space-a/principals").header("X-API-Key","viewer-read").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(),StatusCode::OK);
        let catalog:Value=serde_json::from_slice(&to_bytes(response.into_body(),usize::MAX).await.unwrap()).unwrap();
        assert!(catalog["users"].as_array().unwrap().iter().any(|u|u["id"]=="recipient"));
        let (status,checked)=call(&s,"review","viewer-read",json!({"user_id":"recipient","action":"grant"})).await;
        assert_eq!(status,StatusCode::OK,"{checked}");
        let (status,result)=call(&s,"confirm","viewer-write",json!({"proposal":checked["proposal"],"source_digest":checked["source_digest"],"confirm":true})).await;
        assert_eq!(status,StatusCode::OK,"{result}");assert_eq!(result["source_digest"],checked["source_digest"]);
        let members:i64=sqlx::query_scalar("SELECT COUNT(*) FROM space_members WHERE user_id='recipient'").fetch_one(&s.db).await.unwrap();assert_eq!(members,0);
        let mut conn=s.db.acquire().await.unwrap();let recipient=read_auth(&s,"recipient-read").await;
        let space_access = access::resolve_space_access(&mut conn,access::principal_from_auth(&recipient).unwrap(),"space-a",Purpose::Routine).await;
        assert!(space_access.map(|a|!a.permissions.contains(&access::Permission::View)).unwrap_or(true));
        let audit=rows(&mut conn,"SELECT * FROM audit_log WHERE action='document.page_viewer_grant'",&[]).await.unwrap();
        assert_eq!(audit.len(),1);assert_eq!(audit[0]["actor_id"],"owner-a");assert_eq!(audit[0]["actor_kind"],"user");
        let after:Value=serde_json::from_str(audit[0]["after_value"].as_str().unwrap()).unwrap();assert_eq!(after,result["grant"]);
    }
    #[tokio::test]
    async fn changed_complete_grant_source_and_foreign_scope_reject_without_mutation() {
        let s=fixture().await;let original=auth(&s,"viewer-write").await;let q=checked(&s,Action::Grant).await;
        sqlx::query("UPDATE documents SET title='new title' WHERE id='page-a'").execute(&s.db).await.unwrap();let before=snapshot(&s).await;
        assert!(confirm(State(s.clone()),original,Path(("space-a".into(),"page-a".into())),Json(q.request())).await.is_err());assert_eq!(snapshot(&s).await,before);
        let a=auth(&s,"viewer-write").await;let before=snapshot(&s).await;
        assert!(confirm(State(s.clone()),a,Path(("space-b".into(),"page-a".into())),Json(Confirmation{proposal:Proposal{user_id:"recipient".into(),action:Action::Grant},source_digest:"0".repeat(64),confirm:true})).await.is_err());assert_eq!(snapshot(&s).await,before);
    }
    #[tokio::test]
    async fn editor_read_key_blocked_and_private_ancestor_cannot_create_grant() {
        for key in ["viewer-editor","viewer-read"] {
            let s=fixture().await;let a=if key=="viewer-read" {read_auth(&s,key).await}else{auth(&s,key).await};let q=checked(&s,Action::Grant).await;let before=snapshot(&s).await;
            assert!(confirm(State(s.clone()),a,Path(("space-a".into(),"page-a".into())),Json(q.request())).await.is_err());assert_eq!(snapshot(&s).await,before);
        }
        let s=fixture().await;sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','recipient','owner-a')").execute(&s.db).await.unwrap();
        let (status,_)=call(&s,"review","viewer-read",json!({"user_id":"recipient","action":"grant"})).await;assert!(!status.is_success());
        sqlx::query("UPDATE documents SET parent_id='private-parent' WHERE id='page-a'").execute(&s.db).await.unwrap_err();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('private-parent','space-a','private','','owner-a','owner-a')").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE documents SET parent_id='private-parent' WHERE id='page-a'").execute(&s.db).await.unwrap();
        let (status,_)=call(&s,"inspect","viewer-read",json!({})).await;assert!(!status.is_success());
    }
    #[tokio::test]
    async fn audit_or_recipient_trigger_drift_rolls_back_complete_database() {
        for trigger in ["CREATE TRIGGER viewer_drift AFTER INSERT ON document_viewer_grants BEGIN UPDATE users SET is_active=0 WHERE id='recipient'; END", "CREATE TRIGGER viewer_drift AFTER INSERT ON audit_log WHEN NEW.action='document.page_viewer_grant' BEGIN UPDATE documents SET title='injected' WHERE id='page-a'; END", "CREATE TRIGGER viewer_drift BEFORE INSERT ON audit_log WHEN NEW.action='document.page_viewer_grant' BEGIN SELECT RAISE(ABORT,'audit unavailable'); END"] {
            let s=fixture().await;let a=auth(&s,"viewer-write").await;let q=checked(&s,Action::Grant).await;sqlx::query(trigger).execute(&s.db).await.unwrap();let before=snapshot(&s).await;
            assert!(confirm(State(s.clone()),a,Path(("space-a".into(),"page-a".into())),Json(q.request())).await.is_err());assert_eq!(snapshot(&s).await,before);
        }
    }
    #[tokio::test]
    async fn late_wal_recipient_label_and_ancestor_source_are_not_emitted() {
        for change in ["UPDATE users SET display_name='new private label' WHERE id='recipient'", "UPDATE documents SET title='changed source' WHERE id='page-a'", "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','recipient','owner-a')"] {
            let s=fixture().await;grant(&s).await;let a=read_auth(&s,"viewer-read").await;
            let reached=std::sync::Arc::new(tokio::sync::Notify::new());let release=std::sync::Arc::new(tokio::sync::Notify::new());let state=s.clone();let gates=(reached.clone(),release.clone());
            let task=tokio::spawn(AFTER_INVENTORY.scope(gates,async move {inspect(State(state),InspectionAuth(a),Path(("space-a".into(),"page-a".into()))).await}));
            reached.notified().await;sqlx::query(change).execute(&s.db).await.unwrap();let before=snapshot(&s).await;release.notify_one();assert!(task.await.unwrap().is_err());assert_eq!(snapshot(&s).await,before);
        }
    }
    #[tokio::test]
    async fn last_original_write_key_revoked_by_audit_trigger_rolls_back() {
        let s=fixture().await;let a=auth(&s,"viewer-write").await;let q=checked(&s,Action::Grant).await;
        sqlx::query("CREATE TRIGGER viewer_revoke AFTER INSERT ON audit_log WHEN NEW.action='document.page_viewer_grant' BEGIN DELETE FROM api_keys WHERE id='viewer-write'; END").execute(&s.db).await.unwrap();let before=snapshot(&s).await;
        assert!(confirm(State(s.clone()),a,Path(("space-a".into(),"page-a".into())),Json(q.request())).await.is_err());assert_eq!(snapshot(&s).await,before);
    }
    #[tokio::test]
    async fn inactive_and_blocked_recipient_exact_revoke_retains_other_evidence() {
        for change in ["UPDATE users SET is_active=0 WHERE id='recipient'", "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','recipient','owner-a')"] {
            let s=fixture().await;grant(&s).await;sqlx::query(change).execute(&s.db).await.unwrap();
            let (status,inspection)=call(&s,"inspect","viewer-read",json!({})).await;assert_eq!(status,StatusCode::OK);assert_eq!(inspection["grants"][0]["display_name"],Value::Null);
            let a=auth(&s,"viewer-write").await;let q=checked(&s,Action::Revoke).await;
            confirm(State(s.clone()),a,Path(("space-a".into(),"page-a".into())),Json(q.request())).await.unwrap();
            let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_viewer_grants").fetch_one(&s.db).await.unwrap();assert_eq!(count,0);
            let title:String=sqlx::query_scalar("SELECT title FROM documents WHERE id='page-a'").fetch_one(&s.db).await.unwrap();assert_eq!(title,"page-a");
        }
    }
    #[tokio::test]
    async fn actual_admin_deactivation_denies_recipient_but_manager_can_revoke() {
        let s=fixture().await;
        // The actual admin lifecycle route supports ordinary member/admin account roles.
        sqlx::query("UPDATE users SET role='member' WHERE id='recipient'").execute(&s.db).await.unwrap();
        grant(&s).await;
        sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'").execute(&s.db).await.unwrap();
        let response=crate::auth::routes::router().with_state(s.clone()).oneshot(Request::builder().method("PUT").uri("/admin/users/recipient")
            .header("X-API-Key","viewer-admin").header("Content-Type","application/json").body(Body::from(json!({"is_active":false}).to_string())).unwrap()).await.unwrap();
        let status=response.status();
        let response_body=to_bytes(response.into_body(),usize::MAX).await.unwrap();
        assert_eq!(status,StatusCode::NO_CONTENT,"actual admin lifecycle response: {}",String::from_utf8_lossy(&response_body));
        let active:i64=sqlx::query_scalar("SELECT is_active FROM users WHERE id='recipient'").fetch_one(&s.db).await.unwrap();assert_eq!(active,0);
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_viewer_grants WHERE user_id='recipient'").fetch_one(&s.db).await.unwrap();assert_eq!(count,1);
        let (mut parts,_)=request("inspect","recipient-read",json!({})).into_parts();assert!(InspectionAuth::from_request_parts(&mut parts,&s).await.is_err());
        let a=auth(&s,"viewer-write").await;let q=checked(&s,Action::Revoke).await;
        confirm(State(s.clone()),a,Path(("space-a".into(),"page-a".into())),Json(q.request())).await.unwrap();
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_viewer_grants").fetch_one(&s.db).await.unwrap();assert_eq!(count,0);
    }
    #[tokio::test]
    async fn signed_original_jwt_expires_after_audit_wait_and_rolls_back() {
        let s=fixture().await;
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('viewer-jwt','owner-a','viewer-fixture','2999-01-01T00:00:00Z')").execute(&s.db).await.unwrap();
        let now=chrono::Utc::now().timestamp();
        let claims=crate::auth::jwt::Claims{sub:"owner-a".into(),sid:"viewer-jwt".into(),email:"owner-a@example.test".into(),role:"member".into(),iat:now,exp:now+2,token_type:crate::auth::jwt::TokenType::Access};
        let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes())).unwrap();
        let (mut parts,_)=Request::builder().uri("/fixture").header("Authorization",format!("Bearer {token}")).body(Body::empty()).unwrap().into_parts();
        let a=AuthUser::from_request_parts(&mut parts,&s).await.unwrap();let q=checked(&s,Action::Grant).await;let before=snapshot(&s).await;
        let reached=std::sync::Arc::new(tokio::sync::Notify::new());let release=std::sync::Arc::new(tokio::sync::Notify::new());let state=s.clone();let gates=(reached.clone(),release.clone());
        let task=tokio::spawn(AFTER_INVENTORY.scope(gates,async move {confirm(State(state),a,Path(("space-a".into(),"page-a".into())),Json(q.request())).await}));
        reached.notified().await;tokio::time::sleep(std::time::Duration::from_secs(3)).await;release.notify_one();
        assert!(task.await.unwrap().is_err());assert_eq!(snapshot(&s).await,before);
    }

    #[tokio::test]
    async fn restricted_page_exact_revoke_keeps_current_manager_policy() {
        let s=fixture().await;grant(&s).await;
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('page-a','space-a','restricted','restricted-current','owner-a')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('owner-current','page-a','space-a','owner-a','owner-a')").execute(&s.db).await.unwrap();
        let a=auth(&s,"viewer-write").await;let q=checked(&s,Action::Revoke).await;
        confirm(State(s.clone()),a,Path(("space-a".into(),"page-a".into())),Json(q.request())).await.unwrap();
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_viewer_grants").fetch_one(&s.db).await.unwrap();assert_eq!(count,0);
        let (status,_)=call(&s,"review","viewer-read",json!({"user_id":"recipient","action":"grant"})).await;assert!(!status.is_success());
    }

    #[tokio::test]
    async fn custom_management_cannot_grant_beyond_account_view_but_can_revoke() {
        let s=fixture().await;grant(&s).await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('viewer-management-only','space-a','Manage only','[\"manage_space\"]','owner-a')").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='viewer-management-only' WHERE user_id='owner-a' AND space_id='space-a'").execute(&s.db).await.unwrap();
        let a=auth(&s,"viewer-write").await;let before=snapshot(&s).await;
        assert!(authority(&mut s.db.acquire().await.unwrap(),&a,"space-a","page-a",true,true).await.is_err());assert_eq!(snapshot(&s).await,before);
        let (status,_)=call(&s,"inspect","viewer-read",json!({})).await;assert_eq!(status,StatusCode::OK);
        let q=checked(&s,Action::Revoke).await;confirm(State(s.clone()),a,Path(("space-a".into(),"page-a".into())),Json(q.request())).await.unwrap();
        sqlx::query("UPDATE space_roles SET permissions='[\"manage_space\",\"view\"]' WHERE id='viewer-management-only'").execute(&s.db).await.unwrap();
        let a=auth(&s,"viewer-write").await;let q=checked(&s,Action::Grant).await;
        confirm(State(s.clone()),a,Path(("space-a".into(),"page-a".into())),Json(q.request())).await.unwrap();
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_viewer_grants").fetch_one(&s.db).await.unwrap();assert_eq!(count,1);
    }

}
