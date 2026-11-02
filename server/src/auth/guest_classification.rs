//! Explicit one-way classification of an existing ordinary account. This does
//! not delete memberships/resources, invite accounts or grant page authority.
use axum::{extract::{FromRef, FromRequestParts, Path, State}, http::request::Parts, routing::post, Json, Router};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::AnyConnection;
use super::{middleware::{authenticate_request, AdminUser, ApiKeyRequestOperation, AuthError, AuthUser}, routes::AppError};
use crate::{spaces::{access, document_deletion, mutation_guards}, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/users/{user_id}/guest/review", post(review))
        .route("/admin/users/{user_id}/guest/confirm", post(confirm))
}
struct ReviewAdmin(AuthUser);
impl<S> FromRequestParts<S> for ReviewAdmin where S: Send + Sync, AppState: FromRef<S> {
    type Rejection = AuthError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = AppState::from_ref(state);
        authenticate_request(parts, &state, ApiKeyRequestOperation::Read).await.map(Self)
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewRequest {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Confirmation { source_digest: String, confirm: bool }
#[derive(Clone, PartialEq)]
struct Source { query: String, arguments: Vec<String>, rows: Vec<Value> }
fn unavailable() -> AppError { AppError::NotFound("Guest classification unavailable".into()) }
fn changed() -> AppError { AppError::Conflict("Guest classification source changed; retain the original request and review current account state".into()) }
async fn rows(conn: &mut AnyConnection, query: &str, arguments: &[&str]) -> Result<Vec<Value>, AppError> {
    document_deletion::rows(conn, query, arguments).await.map_err(AppError::internal)
}
async fn administrator(conn: &mut AnyConnection, auth: &AuthUser) -> Result<(), AppError> {
    access::require_current_human_instance_admin(conn, auth).await?;
    access::principal_from_auth(auth)?;
    Ok(())
}
async fn selected(conn: &mut AnyConnection, auth: &AuthUser, id: &str) -> Result<Value, AppError> {
    if id == auth.claims.sub || id.starts_with("service:") { return Err(unavailable()); }
    let value = rows(conn, "SELECT * FROM users WHERE id=$1", &[id]).await?.into_iter().next().ok_or_else(unavailable)?;
    if value["is_active"] != 1 || value["account_kind"] != "member"
        || !matches!(value["role"].as_str(), Some("member" | "viewer")) { return Err(unavailable()); }
    Ok(value)
}
async fn capture(conn: &mut AnyConnection, actor: &str, target: &str) -> Result<Vec<Source>, AppError> {
    let mut result = Vec::new();
    // Account/policy/ownership rows remain byte-exact. Rolling session renewal
    // and key last_used are not authority sources; exact expiry is checked LAST.
    for (query, arguments) in [
        ("SELECT * FROM users WHERE id=$1 OR id=$2".to_owned(), vec![actor.to_owned(),target.to_owned()]),
        ("SELECT id,user_id FROM sessions WHERE user_id=$1 OR user_id=$2".to_owned(), vec![actor.to_owned(),target.to_owned()]),
        ("SELECT id,user_id,is_service_account,key_hash,scopes,expires_at FROM api_keys WHERE user_id=$1 OR user_id=$2".to_owned(), vec![actor.to_owned(),target.to_owned()]),
    ] {
        let args: Vec<&str> = arguments.iter().map(String::as_str).collect();
        result.push(Source {rows: rows(conn,&query,&args).await?,query,arguments});
    }
    for table in ["spaces","space_members","space_roles","groups","group_members","space_group_permissions","group_global_roles","global_roles","space_user_blocks","document_viewer_grants","document_access_policies","document_access_allowlist","documents","document_drafts","document_draft_access","templates","template_drafts","template_revisions","folders","tags","doc_tags","properties_schema","property_values","workflow_templates","document_workflows","document_workflow_events","doc_versions","attachments","excalidraw_files","comments","reactions","starred_documents","doc_subscriptions","draft_birth_receipts","draft_transfer_receipts","draft_save_replay_receipts","publication_receipts","canonical_revision_receipts"] {
        let query = format!("SELECT * FROM {table}");
        result.push(Source {rows:rows(conn,&query,&[]).await?,query,arguments:Vec::new()});
    }
    Ok(result)
}
async fn verify(conn: &mut AnyConnection, expected: &[Source]) -> Result<(), AppError> {
    for proof in expected {
        let args: Vec<&str> = proof.arguments.iter().map(String::as_str).collect();
        if rows(conn,&proof.query,&args).await? != proof.rows { return Err(changed()); }
    }
    Ok(())
}
fn expiry() -> i64 { (chrono::Utc::now().timestamp() / 600 + 1) * 600 }
fn digest(state: &AppState, auth: &AuthUser, target: &str, proof: &[Source], expiry: i64) -> Result<String, AppError> {
    let source: Vec<Value> = proof.iter().map(|p|json!({"query":p.query,"arguments":p.arguments,"rows":p.rows})).collect();
    let value=json!({"schema_version":1,"actor":auth.claims.sub,"session":auth.claims.sid,"token_expiry":auth.api_key_scopes.is_none().then_some(auth.claims.exp),"key_scopes":auth.api_key_scopes,"target":target,"expires_at":expiry,"source":source});
    let mut mac=Hmac::<Sha256>::new_from_slice(state.jwt_secret.as_bytes()).map_err(AppError::internal)?;
    mac.update(b"guest-classification-v1\0");mac.update(&serde_json::to_vec(&value).map_err(AppError::internal)?);
    Ok(hex::encode(mac.finalize().into_bytes()))
}
async fn review(State(state): State<AppState>, ReviewAdmin(auth): ReviewAdmin, Path(id): Path<String>, Json(_request): Json<ReviewRequest>) -> Result<Json<Value>, AppError> {
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    administrator(&mut conn,&auth).await?;
    let target=selected(&mut conn,&auth,&id).await?;
    let proof=capture(&mut conn,&auth.claims.sub,&id).await?;
    let expires=expiry();let source_digest=digest(&state,&auth,&id,&proof,expires)?;
    #[cfg(test)]
    after_review_capture().await;
    administrator(&mut conn,&auth).await?;
    verify(&mut conn,&proof).await?;
    if rows(&mut conn,"SELECT * FROM users WHERE id=$1",&[&id]).await? != vec![target.clone()] {return Err(changed());}
    access::require_current_human_read(&mut conn,&auth).await?;
    administrator(&mut conn,&auth).await?;
    Ok(Json(json!({"schema_version":1,"target":{"id":id,"display_name":target["display_name"],"account_kind":"member"},"source_digest":source_digest,"expires_at":chrono::DateTime::from_timestamp(expires,0).ok_or_else(changed)?.to_rfc3339()})))
}
async fn confirm(State(state): State<AppState>, admin: AdminUser, Path(id): Path<String>, Json(request): Json<Confirmation>) -> Result<Json<Value>, AppError> {
    if !request.confirm || request.source_digest.len()!=64 || !request.source_digest.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)) { return Err(changed()); }
    let auth=AuthUser {claims:admin.claims,api_key_scopes:admin.api_key_scopes};
    let mut tx=state.db.begin().await.map_err(AppError::internal)?;
    mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    let spaces: Vec<String> = sqlx::query_scalar("SELECT id FROM spaces ORDER BY id").fetch_all(&mut *tx).await.map_err(AppError::internal)?;
    mutation_guards::lock_spaces_for_mutation(&mut tx,&spaces.iter().map(String::as_str).collect::<Vec<_>>()).await?;
    administrator(&mut tx,&auth).await?;
    let original=selected(&mut tx,&auth,&id).await?;
    let mut proof=capture(&mut tx,&auth.claims.sub,&id).await?;
    if digest(&state,&auth,&id,&proof,expiry())? != request.source_digest { return Err(changed()); }
    let stamp=chrono::Utc::now().to_rfc3339();
    let count=sqlx::query("UPDATE users SET account_kind='guest',updated_at=$1 WHERE id=$2 AND account_kind='member' AND is_active=1 AND role=$3 AND updated_at=$4")
        .bind(&stamp).bind(&id).bind(original["role"].as_str().ok_or_else(changed)?).bind(original["updated_at"].as_str().ok_or_else(changed)?)
        .execute(&mut *tx).await.map_err(AppError::internal)?;
    if count.rows_affected()!=1 {return Err(changed());}
    let mut expected=original.clone();expected["account_kind"]=json!("guest");expected["updated_at"]=json!(stamp);
    for row in &mut proof[0].rows {if row["id"]==id {*row=expected.clone();}}
    proof[0].rows.sort_by_cached_key(Value::to_string);
    let audit_id=uuid::Uuid::new_v4().to_string();
    let audit_value = |row: &Value| json!({"id": row["id"], "account_kind": row["account_kind"], "role": row["role"], "is_active": row["is_active"]}).to_string();
    let before=audit_value(&original);let after=audit_value(&expected);
    let created:String=sqlx::query_scalar("INSERT INTO audit_log(id,user_id,actor_kind,actor_id,action,target_type,target_id,before_value,after_value) VALUES($1,$2,'user',$2,'user.classify_guest','user',$3,$4,$5) RETURNING created_at")
        .bind(&audit_id).bind(&auth.claims.sub).bind(&id).bind(&before).bind(&after).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    let audit=json!({"id":audit_id,"user_id":auth.claims.sub,"actor_kind":"user","actor_id":auth.claims.sub,"action":"user.classify_guest","target_type":"user","target_id":id,"before_value":before,"after_value":after,"created_at":created,"ip_address":null});
    administrator(&mut tx,&auth).await?;
    verify(&mut tx,&proof).await?;
    if rows(&mut tx,"SELECT * FROM audit_log WHERE id=$1",&[&audit_id]).await?!=vec![audit] {return Err(changed());}
    administrator(&mut tx,&auth).await?;
    tx.commit().await.map_err(|_|AppError::Conflict("Guest classification acknowledgement unknown; retain the original request and inspect current account state without automatic retry".into()))?;
    Ok(Json(json!({"confirmed":true,"user_id":id,"account_kind":"guest"})))
}

#[cfg(test)]
tokio::task_local! {
    static REVIEW_CAPTURE_HOOK: std::sync::Arc<dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send>> + Send + Sync>;
}
#[cfg(test)]
async fn after_review_capture() {
    if let Ok(callback) = REVIEW_CAPTURE_HOOK.try_with(Clone::clone) { callback().await; }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::{to_bytes, Body}, http::{Request, StatusCode}};
    use tower::ServiceExt;
    use crate::spaces::{access::{PageOperation, Purpose}, properties::tests::test_auth, documents::tests::move_database_snapshot};

    async fn fixture(wal: bool) -> AppState {
        let mut state = crate::spaces::properties::tests::property_test_state().await;
        if wal {
            state.db.close().await;
            let path = std::env::temp_dir().join(format!("guest-classification-{}.sqlite", uuid::Uuid::new_v4()));
            state.db = sqlx::any::AnyPoolOptions::new().max_connections(3)
                .after_connect(|conn, _| Box::pin(async move {
                    sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *conn).await?;
                    sqlx::query("PRAGMA journal_mode=WAL").execute(&mut *conn).await?;
                    Ok(())
                }))
                .connect(&format!("sqlite://{}?mode=rwc", path.display())).await.unwrap();
            sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
            for id in ["owner-a", "viewer-a", "editor-a"] {
                sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')")
                    .bind(id).bind(format!("{id}@example.test")).execute(&state.db).await.unwrap();
                sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'fixture','2999-01-01T00:00:00Z')")
                    .bind(format!("metadata-{id}")).bind(id).execute(&state.db).await.unwrap();
            }
            sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('space-a','Space','space-a','private','owner-a')").execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('owner-member','space-a','owner-a','owner'),('viewer-member','space-a','viewer-a','editor')").execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','Target','original-path','owner-a','owner-a','published')").execute(&state.db).await.unwrap();
            let opened: String = sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name='main'").fetch_one(&state.db).await.unwrap();
            assert_eq!(std::fs::canonicalize(opened).unwrap(), std::fs::canonicalize(&path).unwrap());
            assert!(std::fs::metadata(path).unwrap().len() > 0);
        }
        sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'").execute(&state.db).await.unwrap();
        for (id,user,scopes) in [("qa-admin","owner-a","[\"admin\"]"),("qa-read","viewer-a","[\"read\"]"),("qa-guest-admin","viewer-a","[\"admin\"]")] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,$2,'Guest QA',$3,'qa',$4)")
                .bind(id).bind(user).bind(crate::auth::api_keys::hash_api_key(id)).bind(scopes).execute(&state.db).await.unwrap();
        }
        sqlx::query("INSERT INTO document_viewer_grants(id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at) VALUES('guest-grant','target-a','space-a','viewer-a','viewer','page','original','owner-a','created','updated')").execute(&state.db).await.unwrap();
        state
    }
    fn request(stage: &str, target: &str, key: &str, value: Value) -> Request<Body> {
        Request::builder().method("POST").uri(format!("/admin/users/{target}/guest/{stage}"))
            .header("X-API-Key",key).header("Content-Type","application/json")
            .body(Body::from(value.to_string())).unwrap()
    }
    async fn call(state: &AppState, stage: &str, target: &str, key: &str, value: Value) -> (StatusCode, Value) {
        let response = router().with_state(state.clone()).oneshot(request(stage,target,key,value)).await.unwrap();
        let status=response.status();let bytes=to_bytes(response.into_body(),usize::MAX).await.unwrap();
        (status,serde_json::from_slice(&bytes).unwrap())
    }
    async fn classify(state: &AppState) {
        let (status,review)=call(state,"review","viewer-a","qa-admin",json!({})).await;
        assert_eq!(status,StatusCode::OK,"{review}");
        let (status,result)=call(state,"confirm","viewer-a","qa-admin",json!({"source_digest":review["source_digest"],"confirm":true})).await;
        assert_eq!(status,StatusCode::OK,"{result}");assert_eq!(result["account_kind"],"guest");
    }
    #[tokio::test]
    async fn guest_classification_http_retains_memberships_resources_and_exact_audit() {
        let state=fixture(false).await;
        sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES('legacy-asset','target-a','space-a','asset.png','original.png','image/png',68,'original-asset-path','viewer-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('legacy-draft','target-a','space-a','viewer-a','Original private title','Original private body','original')").execute(&state.db).await.unwrap();
        let mut conn=state.db.acquire().await.unwrap();
        let assets=rows(&mut conn,"SELECT * FROM attachments",&[]).await.unwrap();
        let drafts=rows(&mut conn,"SELECT * FROM document_drafts",&[]).await.unwrap();
        let resources=rows(&mut conn,"SELECT * FROM documents",&[]).await.unwrap();
        let memberships=rows(&mut conn,"SELECT * FROM space_members",&[]).await.unwrap();
        let grants=rows(&mut conn,"SELECT * FROM document_viewer_grants",&[]).await.unwrap();drop(conn);
        classify(&state).await;
        let mut conn=state.db.acquire().await.unwrap();
        assert_eq!(rows(&mut conn,"SELECT * FROM documents",&[]).await.unwrap(),resources);
        assert_eq!(rows(&mut conn,"SELECT * FROM attachments",&[]).await.unwrap(),assets);
        assert_eq!(rows(&mut conn,"SELECT * FROM document_drafts",&[]).await.unwrap(),drafts);
        assert_eq!(rows(&mut conn,"SELECT * FROM space_members",&[]).await.unwrap(),memberships);
        assert_eq!(rows(&mut conn,"SELECT * FROM document_viewer_grants",&[]).await.unwrap(),grants);
        let audit=rows(&mut conn,"SELECT * FROM audit_log WHERE action='user.classify_guest'",&[]).await.unwrap();
        assert_eq!(audit.len(),1);assert_eq!(audit[0]["user_id"],"owner-a");
        let after:Value=serde_json::from_str(audit[0]["after_value"].as_str().unwrap()).unwrap();
        assert_eq!(after,json!({"id":"viewer-a","account_kind":"guest","role":"member","is_active":1}));
        assert!(!audit[0].to_string().contains("password_hash"));
    }
    #[tokio::test]
    async fn guest_current_kind_suppresses_inherited_space_but_keeps_only_direct_published_view() {
        let state=fixture(false).await;
        for query in [
            "INSERT INTO groups(id,name) VALUES('legacy-group','Legacy group')",
            "INSERT INTO group_members(id,group_id,user_id) VALUES('legacy-member','legacy-group','viewer-a')",
            "INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('legacy-space-role','space-a','legacy-group','owner')",
            "INSERT INTO group_global_roles(group_id,role_id) VALUES('legacy-group','owner')",
            "UPDATE spaces SET visibility='internal' WHERE id='space-a'",
        ] { sqlx::query(query).execute(&state.db).await.unwrap(); }
        classify(&state).await;
        let auth=test_auth("viewer-a");let mut conn=state.db.acquire().await.unwrap();
        let principal=access::principal_from_auth(&auth).unwrap();
        let space=access::resolve_space_access(&mut conn,principal,"space-a",Purpose::Routine).await.unwrap();
        assert!(space.permissions.is_empty());
        assert!(access::resolve_page_access(&mut conn,principal,"space-a","target-a",PageOperation::GrantedPublishedView,Purpose::Routine).await.is_ok());
        for operation in [PageOperation::View,PageOperation::Comment,PageOperation::Edit] {
            assert!(access::resolve_page_access(&mut conn,principal,"space-a","target-a",operation,Purpose::Routine).await.is_err());
        }
        assert!(access::require_current_human_member_read(&mut conn,&auth).await.is_err());
    }
    #[tokio::test]
    async fn guest_admin_key_and_claim_cannot_manufacture_current_admin() {
        let state=fixture(false).await;classify(&state).await;
        sqlx::query("UPDATE users SET role='admin' WHERE id='viewer-a'").execute(&state.db).await.unwrap();
        let (status,_)=call(&state,"review","editor-a","qa-guest-admin",json!({})).await;
        assert_ne!(status,StatusCode::OK);
        let auth=test_auth("viewer-a");let mut conn=state.db.acquire().await.unwrap();
        assert!(access::require_current_human_instance_admin(&mut conn,&auth).await.is_err());
        let space=access::resolve_space_access(&mut conn,access::principal_from_auth(&auth).unwrap(),"space-a",Purpose::Routine).await.unwrap();
        assert!(space.permissions.is_empty());
    }
    #[tokio::test]
    async fn guest_classification_changed_source_and_audit_fault_roll_back_complete_database() {
        for fault in 0..3 {
            let state=fixture(false).await;
            let (status,review)=call(&state,"review","viewer-a","qa-admin",json!({})).await;assert_eq!(status,StatusCode::OK);
            if fault == 1 {sqlx::query("CREATE TRIGGER guest_audit_fail AFTER INSERT ON audit_log WHEN NEW.action='user.classify_guest' BEGIN SELECT RAISE(ABORT,'guest audit fault'); END").execute(&state.db).await.unwrap();}
            else if fault == 2 {sqlx::query("CREATE TRIGGER guest_admin_drift AFTER INSERT ON audit_log WHEN NEW.action='user.classify_guest' BEGIN UPDATE users SET account_kind='guest' WHERE id='owner-a'; END").execute(&state.db).await.unwrap();}
            else {sqlx::query("UPDATE users SET display_name='Changed' WHERE id='viewer-a'").execute(&state.db).await.unwrap();}
            // Extractor bookkeeping precedes the complete rollback baseline.
            let (mut parts,_)=request("confirm","viewer-a","qa-admin",json!({})).into_parts();
            let admin=AdminUser::from_request_parts(&mut parts,&state).await.unwrap();
            let before=move_database_snapshot(&state).await;
            assert!(confirm(State(state.clone()),admin,Path("viewer-a".into()),Json(Confirmation{source_digest:review["source_digest"].as_str().unwrap().into(),confirm:true})).await.is_err());
            assert_eq!(move_database_snapshot(&state).await,before);
        }
    }
    #[tokio::test]
    async fn guest_review_wal_late_target_or_original_admin_kind_change_is_denied() {
        for id in ["viewer-a","owner-a"] {
            let state=fixture(true).await;
            let copy=state.clone();let id=id.to_owned();
            let hook:std::sync::Arc<dyn Fn()->std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>+Send+Sync>=std::sync::Arc::new(move||{let state=copy.clone();let id=id.clone();Box::pin(async move {sqlx::query("UPDATE users SET account_kind='guest' WHERE id=$1").bind(id).execute(&state.db).await.unwrap();})});
            let (status,_)=REVIEW_CAPTURE_HOOK.scope(hook,call(&state,"review","viewer-a","qa-admin",json!({}))).await;
            assert_ne!(status,StatusCode::OK);
            let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE action='user.classify_guest'").fetch_one(&state.db).await.unwrap();assert_eq!(count,0);
        }
    }
    #[tokio::test]
    async fn guest_real_bearer_claims_and_global_directory_observe_database_kind() {
        let state=fixture(false).await;
        let list = |key: &str| Request::builder().method("GET").uri("/users/directory")
            .header("X-API-Key",key).body(Body::empty()).unwrap();
        let ordinary=super::super::routes::router().with_state(state.clone()).oneshot(list("qa-read")).await.unwrap();
        assert_eq!(ordinary.status(),StatusCode::OK);
        classify(&state).await;
        let denied=super::super::routes::router().with_state(state.clone()).oneshot(list("qa-read")).await.unwrap();
        assert_ne!(denied.status(),StatusCode::OK);
        sqlx::query("UPDATE users SET role='admin' WHERE id='viewer-a'").execute(&state.db).await.unwrap();
        let tokens=super::super::jwt::create_token_pair_for_session("viewer-a","viewer-a@example.test","admin",&state.jwt_secret,Some("metadata-viewer-a"),30).unwrap();
        let (mut parts,_)=Request::builder().method("GET").uri("/auth/me").header("Authorization",format!("Bearer {}",tokens.access_token)).body(Body::empty()).unwrap().into_parts();
        let auth=AuthUser::from_request_parts(&mut parts,&state).await.unwrap();
        assert_eq!(auth.claims.role,"viewer");
        let mut conn=state.db.acquire().await.unwrap();
        assert!(access::require_current_human_instance_admin(&mut conn,&auth).await.is_err());
    }
    #[tokio::test]
    async fn guest_classification_schema_default_invalid_kind_and_ineligible_targets_are_safe() {
        let state=fixture(false).await;
        let kinds:Vec<String>=sqlx::query_scalar("SELECT account_kind FROM users ORDER BY id").fetch_all(&state.db).await.unwrap();
        assert!(kinds.iter().all(|kind|kind=="member"));
        assert!(sqlx::query("UPDATE users SET account_kind='other' WHERE id='viewer-a'").execute(&state.db).await.is_err());
        for target in ["owner-a","missing"] {
            let (status,_)=call(&state,"review",target,"qa-admin",json!({})).await;assert_ne!(status,StatusCode::OK);
        }
        sqlx::query("UPDATE users SET is_active=0 WHERE id='viewer-a'").execute(&state.db).await.unwrap();
        let (status,_)=call(&state,"review","viewer-a","qa-admin",json!({})).await;assert_ne!(status,StatusCode::OK);
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE action='user.classify_guest'").fetch_one(&state.db).await.unwrap();assert_eq!(count,0);
    }
    #[tokio::test]
    async fn guest_admin_reactivation_keeps_kind_and_denies_promotion_without_personal_birth() {
        let state=fixture(false).await;classify(&state).await;
        let original_spaces:Vec<String>=sqlx::query_scalar("SELECT id FROM spaces ORDER BY id").fetch_all(&state.db).await.unwrap();
        for active in [false,true] {
            let response=super::super::routes::router().with_state(state.clone()).oneshot(Request::builder().method("PUT").uri("/admin/users/viewer-a").header("X-API-Key","qa-admin").header("Content-Type","application/json").body(Body::from(json!({"is_active":active}).to_string())).unwrap()).await.unwrap();
            assert_eq!(response.status(),StatusCode::NO_CONTENT);
            let kind:String=sqlx::query_scalar("SELECT account_kind FROM users WHERE id='viewer-a'").fetch_one(&state.db).await.unwrap();assert_eq!(kind,"guest");
        }
        let response=super::super::routes::router().with_state(state.clone()).oneshot(Request::builder().method("PUT").uri("/admin/users/viewer-a").header("X-API-Key","qa-admin").header("Content-Type","application/json").body(Body::from(json!({"role":"admin"}).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(),StatusCode::BAD_REQUEST);
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT id FROM spaces ORDER BY id").fetch_all(&state.db).await.unwrap(),original_spaces);
        assert!(super::super::routes::create_personal_space(&state,"viewer-a","Guest").await.is_err());
    }

    #[tokio::test]
    async fn guest_review_original_signed_token_expiry_after_source_wait_is_denied() {
        let state=fixture(true).await;
        let mut claims=test_auth("owner-a").claims;claims.role="admin".into();
        claims.exp=chrono::Utc::now().timestamp()+2;claims.iat=chrono::Utc::now().timestamp();
        let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();
        let deadline=claims.exp;
        let hook:std::sync::Arc<dyn Fn()->std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>+Send+Sync>=std::sync::Arc::new(move||Box::pin(async move {
            while chrono::Utc::now().timestamp()<=deadline {tokio::time::sleep(std::time::Duration::from_millis(25)).await;}
        }));
        let request=Request::builder().method("POST").uri("/admin/users/viewer-a/guest/review").header("Authorization",format!("Bearer {token}")).header("Content-Type","application/json").body(Body::from("{}")).unwrap();
        let response=REVIEW_CAPTURE_HOOK.scope(hook,router().with_state(state.clone()).oneshot(request)).await.unwrap();
        assert_ne!(response.status(),StatusCode::OK);
        let kind:String=sqlx::query_scalar("SELECT account_kind FROM users WHERE id='viewer-a'").fetch_one(&state.db).await.unwrap();assert_eq!(kind,"member");
    }

    #[tokio::test]
    async fn guest_manager_catalog_can_review_and_confirm_only_page_viewer_grant() {
        let state=fixture(false).await;classify(&state).await;
        sqlx::query("DELETE FROM document_viewer_grants WHERE id='guest-grant'").execute(&state.db).await.unwrap();
        let catalog=crate::spaces::routes::router().with_state(state.clone()).oneshot(Request::builder().method("GET").uri("/spaces/space-a/principals").header("X-API-Key","qa-admin").body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(catalog.status(),StatusCode::OK);
        let catalog:Value=serde_json::from_slice(&to_bytes(catalog.into_body(),usize::MAX).await.unwrap()).unwrap();
        assert!(catalog["users"].as_array().unwrap().iter().any(|u|u["id"]=="viewer-a"));
        let proposal=json!({"user_id":"viewer-a","action":"grant"});
        let request=|stage:&str,value:Value|Request::builder().method("POST").uri(format!("/spaces/space-a/documents/target-a/viewer-grants/{stage}")).header("X-API-Key","qa-admin").header("Content-Type","application/json").body(Body::from(value.to_string())).unwrap();
        let response=crate::spaces::page_viewer_grant_management::router().with_state(state.clone()).oneshot(request("review",proposal.clone())).await.unwrap();
        assert_eq!(response.status(),StatusCode::OK);
        let review:Value=serde_json::from_slice(&to_bytes(response.into_body(),usize::MAX).await.unwrap()).unwrap();
        let response=crate::spaces::page_viewer_grant_management::router().with_state(state.clone()).oneshot(request("confirm",json!({"proposal":proposal,"source_digest":review["source_digest"],"confirm":true}))).await.unwrap();
        assert_eq!(response.status(),StatusCode::OK);
        let stored:(String,String)=sqlx::query_as("SELECT role,scope FROM document_viewer_grants WHERE user_id='viewer-a'").fetch_one(&state.db).await.unwrap();assert_eq!(stored,("viewer".into(),"page".into()));
        let auth=test_auth("viewer-a");let mut conn=state.db.acquire().await.unwrap();
        assert!(access::resolve_page_access(&mut conn,access::principal_from_auth(&auth).unwrap(),"space-a","target-a",PageOperation::GrantedPublishedView,Purpose::Routine).await.is_ok());
        assert!(access::resolve_page_access(&mut conn,access::principal_from_auth(&auth).unwrap(),"space-a","target-a",PageOperation::Edit,Purpose::Routine).await.is_err());
    }

    #[tokio::test]
    async fn guest_member_read_helper_uses_one_current_kind_and_key_ceiling() {
        let state = fixture(false).await;
        let (mut parts, _) = Request::builder().method("GET").uri("/users/directory")
            .header("X-API-Key", "qa-read").body(Body::empty()).unwrap().into_parts();
        let auth = authenticate_request(&mut parts, &state, ApiKeyRequestOperation::Read).await.unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        assert_eq!(access::require_current_human_member_read(&mut conn, &auth).await.unwrap(), "viewer-a");
        sqlx::query("UPDATE api_keys SET scopes='[\"write\"]' WHERE id='qa-read'").execute(&mut *conn).await.unwrap();
        assert!(access::require_current_human_member_read(&mut conn, &auth).await.is_err());
        sqlx::query("UPDATE api_keys SET scopes='[\"admin\"]' WHERE id='qa-read'").execute(&mut *conn).await.unwrap();
        assert_eq!(access::require_current_human_member_read(&mut conn, &auth).await.unwrap(), "viewer-a");
        sqlx::query("UPDATE users SET account_kind='guest' WHERE id='viewer-a'").execute(&mut *conn).await.unwrap();
        assert!(access::require_current_human_member_read(&mut conn, &auth).await.is_err());
    }

}
