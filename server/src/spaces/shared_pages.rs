//! Actor-owned direct-page discovery. This router remains unwired until the
//! manager, source inventories and cascade writers are adopted together.
use super::{access, published_source};
use crate::{auth::{middleware::{authenticate_request, ApiKeyRequestOperation, AuthError, AuthUser}, routes::AppError}, AppState};
use axum::{extract::{FromRef, FromRequestParts, State}, http::request::Parts, routing::post, Json, Router};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::AnyConnection;

pub fn router() -> Router<AppState> { Router::new().route("/shared-pages/inspect", post(inspect)) }
struct ReadAuth(AuthUser);
impl<S: Send + Sync> FromRequestParts<S> for ReadAuth where AppState: FromRef<S> {
    type Rejection = AuthError;
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        authenticate_request(parts, &AppState::from_ref(state), ApiKeyRequestOperation::Read).await.map(Self)
    }
}
/// Published GET adapters select this named operation; drafts and lifecycle
/// callers continue to use their existing operations.
pub(crate) async fn published_view(conn: &mut AnyConnection, auth: &AuthUser, space: &str, id: &str) -> Result<access::PageAccess, AppError> {
    access::resolve_page_access(conn, access::principal_from_auth(auth)?, space, id, access::PageOperation::GrantedPublishedView, access::Purpose::Routine).await
}
/// Page-owned assets retain ordinary unpublished-page behavior, without making
/// a grant an authority for an unpublished page.
pub(crate) async fn asset_view(conn: &mut AnyConnection, auth: &AuthUser, space: &str, id: &str) -> Result<access::PageAccess, AppError> {
    let published: Option<i64> = sqlx::query_scalar("SELECT 1 FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND deleted_at IS NULL AND archived_at IS NULL").bind(id).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    access::resolve_page_access(conn, access::principal_from_auth(auth)?, space, id, if published.is_some() {access::PageOperation::GrantedPublishedView} else {access::PageOperation::View}, access::Purpose::Routine).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request { cursor: Option<String> }
#[derive(Clone, Debug, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct SharedPage {
    pub id: String, pub space_id: String, pub title: String,
    pub published_at: String, pub updated_at: String,
    pub human_identifier: Option<String>, pub icon: Option<String>,
}
#[derive(Serialize)]
struct Response { pages: Vec<SharedPage>, next_cursor: Option<String> }
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor { kind: String, actor: String, identity: String, bearer_exp: Option<i64>, scopes: Option<Vec<String>>, after: String, exp: i64 }
fn cursor_error() -> AppError { AppError::BadRequest("Shared pages cursor expired or belongs to another selection; refresh Shared with me".into()) }
fn cursor_mac(state: &AppState) -> Hmac<Sha256> {
    let mut derived = Hmac::<Sha256>::new_from_slice(state.jwt_secret.as_bytes()).expect("HMAC accepts any key length");
    derived.update(b"starlane-shared-pages-cursor-key-v1\0");
    Hmac::<Sha256>::new_from_slice(&derived.finalize().into_bytes()).expect("HMAC accepts any key length")
}
fn encode_cursor(state: &AppState, auth: &AuthUser, after: &str) -> Result<String, AppError> {
    let cursor = Cursor {kind:"starlane.shared-pages.cursor.v1".into(),actor:auth.claims.sub.clone(),identity:auth.claims.sid.clone(),bearer_exp:auth.api_key_scopes.is_none().then_some(auth.claims.exp),scopes:auth.api_key_scopes.clone(),after:after.into(),exp:chrono::Utc::now().timestamp()+600};
    let bytes = serde_json::to_vec(&cursor).map_err(AppError::internal)?;
    let mut mac = cursor_mac(state); mac.update(&bytes);
    Ok(format!("{}.{}",hex::encode(&bytes),hex::encode(mac.finalize().into_bytes())))
}
fn decode_cursor(state: &AppState, auth: &AuthUser, token: &str) -> Result<String, AppError> {
    if token.len()>8192 {return Err(cursor_error());}
    let (payload,signature)=token.split_once('.').ok_or_else(cursor_error)?;
    let bytes=hex::decode(payload).map_err(|_|cursor_error())?;
    let signature=hex::decode(signature).map_err(|_|cursor_error())?;
    let mut mac=cursor_mac(state);mac.update(&bytes);mac.verify_slice(&signature).map_err(|_|cursor_error())?;
    let cursor:Cursor=serde_json::from_slice(&bytes).map_err(|_|cursor_error())?;
    if cursor.kind!="starlane.shared-pages.cursor.v1" || cursor.actor!=auth.claims.sub || cursor.identity!=auth.claims.sid || cursor.bearer_exp!=auth.api_key_scopes.is_none().then_some(auth.claims.exp) || cursor.scopes!=auth.api_key_scopes || cursor.exp<=chrono::Utc::now().timestamp() || cursor.after.is_empty() || cursor.after.len()>256 {return Err(cursor_error());}
    Ok(cursor.after)
}
async fn selected(conn: &mut AnyConnection, space: &str, id: &str) -> Result<Option<SharedPage>, AppError> {
    sqlx::query_as("SELECT id,space_id,title,published_at,updated_at,human_identifier,icon FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL").bind(id).bind(space).fetch_optional(conn).await.map_err(AppError::internal)
}
async fn inspect(State(state): State<AppState>, ReadAuth(auth): ReadAuth, Json(request): Json<Request>) -> Result<Json<Response>, AppError> {
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    access::require_current_human_read(&mut conn,&auth).await?;
    let mut after=request.cursor.as_deref().map(|c|decode_cursor(&state,&auth,c)).transpose()?.unwrap_or_default();
    let mut found=Vec::new();
    loop {
        let targets:Vec<(String,String)>=sqlx::query_as("SELECT document_id,space_id FROM document_viewer_grants WHERE user_id=$1 AND document_id>$2 ORDER BY document_id LIMIT 100").bind(&auth.claims.sub).bind(&after).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
        let count=targets.len();
        for (id,space) in targets {
            after=id.clone();
            let page=match published_view(&mut conn,&auth,&space,&id).await {
                Ok(page)=>page,
                Err(AppError::Forbidden(_)|AppError::NotFound(_))=>continue,
                Err(e)=>return Err(e),
            };
            // Catalog includes only actual actor grants, even if inherited View
            // would independently admit this target after a concurrent revoke.
            let Some(grant)=super::page_viewer_grants::read_current_direct_viewer_grant(&mut conn,&space,&id,&auth.claims.sub).await? else {continue};
            #[cfg(test)]
            if let Some(wait)=DISCOVERY_CAPTURE_WAIT.with(|slot|slot.borrow_mut().take()) {wait.await;}
            let Some(row)=selected(&mut conn,&space,&id).await? else {continue};
            let proof=published_source::capture_source(&mut conn,&auth,&space,&id,&page.ancestor_ids).await?;
            found.push((row,proof,grant));
            if found.len()==21 {break;}
        }
        if found.len()==21 || count<100 {break;}
    }
    #[cfg(test)]
    reader_wait().await;
    for (row,proof,grant) in &found {
        if selected(&mut conn,&row.space_id,&row.id).await?.as_ref()!=Some(row) {return Err(published_source::conflict());}
        published_source::verify_source(&mut conn,proof).await?;
        if super::page_viewer_grants::read_current_direct_viewer_grant(&mut conn,&row.space_id,&row.id,&auth.claims.sub).await?.as_ref()!=Some(grant) {return Err(published_source::conflict());}
        #[cfg(test)]
        policy_wait(&row.id).await;
        published_view(&mut conn,&auth,&row.space_id,&row.id).await?;
    }
    // Later page-policy checks can wait after an earlier row was proved.
    // Repeat ALL emitted row/source/original-grant evidence after those waits.
    for (row,proof,grant) in &found {
        if selected(&mut conn,&row.space_id,&row.id).await?.as_ref()!=Some(row) {return Err(published_source::conflict());}
        published_source::verify_source(&mut conn,proof).await?;
        if super::page_viewer_grants::read_current_direct_viewer_grant(&mut conn,&row.space_id,&row.id,&auth.claims.sub).await?.as_ref()!=Some(grant) {return Err(published_source::conflict());}
    }
    access::require_current_human_read(&mut conn,&auth).await?;
    // Cursor contains only the last RETURNED page identity, never an inaccessible
    // skipped target, a hidden count or an authority-bearing policy payload.
    let next_cursor=if found.len()>20 {Some(encode_cursor(&state,&auth,&found[19].0.id)?)} else {None};
    Ok(Json(Response {pages:found.into_iter().take(20).map(|(row,_,_)|row).collect(),next_cursor}))
}
#[cfg(test)]
thread_local! { static READER_WAIT:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const {std::cell::RefCell::new(None)}; }

#[cfg(test)]
thread_local! {static DISCOVERY_CAPTURE_WAIT:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};}
#[cfg(test)]
thread_local! {static POLICY_WAIT:std::cell::RefCell<Option<(String,std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>)>>=const{std::cell::RefCell::new(None)};}
#[cfg(test)]
pub(crate) async fn policy_wait(id:&str) {
    let wait=POLICY_WAIT.with(|slot| {
        let mut slot=slot.borrow_mut();
        if slot.as_ref().is_some_and(|(target,_)|target==id) {slot.take().map(|(_,wait)|wait)}else{None}
    });
    if let Some(wait)=wait {wait.await;}
}
#[cfg(test)]
pub(crate) async fn reader_wait() {
    let wait=READER_WAIT.with(|slot|slot.borrow_mut().take());
    if let Some(wait)=wait {wait.await;}
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
    use serde_json::{json,Value};
    use tower::ServiceExt;
    const BODY:&str="PUBLISHED VIEWER NEBULA EXACT";
    struct Fixture { state:AppState, body_path:std::path::PathBuf, asset_path:std::path::PathBuf, relative_asset:String, db_path:Option<String> }
    impl Fixture {
        async fn close(self) {
            self.state.db.close().await;
            let _=tokio::fs::remove_file(self.body_path).await;
            let _=tokio::fs::remove_file(self.asset_path).await;
            if let Some(path)=self.db_path {for suffix in ["","-wal","-shm"] {let _=tokio::fs::remove_file(format!("{path}{suffix}")).await;}}
        }
    }
    async fn fixture(wal:bool)->Fixture {
        let mut state=crate::spaces::properties::tests::property_test_state().await;
        let db_path=if wal {
            let path=format!("/tmp/page-viewer-readers-{}.sqlite",uuid::Uuid::new_v4());
            state.db.close().await;
            state.db=sqlx::any::AnyPoolOptions::new().max_connections(2)
                .after_connect(|conn,_|Box::pin(async move {
                    sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *conn).await?;
                    sqlx::query("PRAGMA busy_timeout=5000").execute(&mut *conn).await?;
                    Ok(())
                }))
                .connect(&format!("sqlite://{path}?mode=rwc")).await.unwrap();
            sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
            assert!(std::fs::metadata(&path).unwrap().len()>0);
            let mode:String=sqlx::query_scalar("PRAGMA journal_mode=WAL").fetch_one(&state.db).await.unwrap();
            assert_eq!(mode.to_ascii_lowercase(),"wal");
            for actor in ["owner-a","viewer-a","editor-a"] {
                sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')").bind(actor).bind(format!("{actor}@viewer.test")).execute(&state.db).await.unwrap();
                sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'fixture','2999-01-01T00:00:00Z')").bind(format!("metadata-{actor}")).bind(actor).execute(&state.db).await.unwrap();
            }
            for space in ["space-a","space-b"] {sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,$1,$1,'private','owner-a')").bind(space).execute(&state.db).await.unwrap();}
            for (actor,role) in [("owner-a","owner"),("viewer-a","viewer"),("editor-a","editor")] {sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'space-a',$2,$3)").bind(format!("member-{actor}")).bind(actor).bind(role).execute(&state.db).await.unwrap();}
            for (id,space) in [("target-a","space-a"),("target-b","space-b")] {sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES($1,$2,$1,'','owner-a','owner-a','published')").bind(id).bind(space).execute(&state.db).await.unwrap();}
            Some(path)
        } else {None};
        sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES('reader-a','reader@viewer.test','Reader','member')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('viewer-reader-a','reader-a','fixture','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
        let slug=format!("viewer-readers-{}",uuid::Uuid::new_v4());
        sqlx::query("UPDATE spaces SET name='HIDDEN SPACE LABEL',slug=$1 WHERE id='space-a'").bind(&slug).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO folders(id,space_id,name) VALUES('hidden-folder','space-a','HIDDEN FOLDER LABEL')").execute(&state.db).await.unwrap();
        let body_path=std::env::temp_dir().join(format!("viewer-body-{}.md",uuid::Uuid::new_v4()));
        tokio::fs::write(&body_path,BODY).await.unwrap();
        sqlx::query("UPDATE documents SET title='Visible published page',human_identifier='PAGE-042',human_identifier_key='PAGE-042',folder_id='hidden-folder',content_path=$1 WHERE id='target-a'").bind(body_path.to_string_lossy().as_ref()).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('hidden-draft','target-a','space-a','owner-a','HIDDEN DRAFT TITLE','HIDDEN DRAFT BODY','old')").execute(&state.db).await.unwrap();
        grant(&state,"target-a").await;
        let relative_asset=format!("spaces/{slug}/attachments/target-a/owned.bin");
        let asset_path=crate::storage::data_dir().join(&relative_asset);
        tokio::fs::create_dir_all(asset_path.parent().unwrap()).await.unwrap();
        tokio::fs::write(&asset_path,b"PAGE OWNED ASSET EXACT").await.unwrap();
        sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES('viewer-asset','target-a','space-a','owned.bin','owned.bin','application/octet-stream',22,$1,'owner-a')").bind(&relative_asset).execute(&state.db).await.unwrap();
        {let mut legacy=state.db.acquire().await.unwrap();crate::spaces::asset_audiences::admit_legacy(&mut legacy,"viewer-asset","space-a","target-a").await.unwrap();}
        state.search_index.upsert(crate::search::index::DocEntry{doc_id:"target-a".into(),space_id:"space-a".into(),title:"Visible published page".into(),body:BODY.into(),tags:String::new()}).unwrap();
        Fixture {state,body_path,asset_path,relative_asset,db_path}
    }
    async fn grant(state:&AppState,id:&str) {
        sqlx::query("INSERT INTO document_viewer_grants(id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at) VALUES($1,$2,'space-a','reader-a','viewer','page','revision-1','owner-a','2026-10-09','2026-10-09')").bind(format!("grant-{id}")).bind(id).execute(&state.db).await.unwrap();
    }
    fn token(state:&AppState,actor:&str)->String {
        let claims=crate::auth::jwt::Claims{sub:actor.into(),sid:if actor=="reader-a" {"viewer-reader-a".into()}else{format!("metadata-{actor}")},email:format!("{actor}@viewer.test"),role:"member".into(),iat:chrono::Utc::now().timestamp(),exp:chrono::Utc::now().timestamp()+900,token_type:crate::auth::jwt::TokenType::Access};
        jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap()
    }
    async fn call(state:&AppState,actor:&str,method:&str,uri:&str,body:Value)->(StatusCode,Vec<u8>) {
        call_with_token(state,&token(state,actor),method,uri,body).await
    }
    async fn call_with_token(state:&AppState,bearer:&str,method:&str,uri:&str,body:Value)->(StatusCode,Vec<u8>) {
        let app=if uri.starts_with("/shared-pages/") {super::router()}else{crate::api_routes()};
        let response=app.with_state(state.clone()).oneshot(Request::builder().method(method).uri(uri).header("Authorization",format!("Bearer {bearer}")).header("Content-Type","application/json").body(Body::from(body.to_string())).unwrap()).await.unwrap();
        (response.status(),to_bytes(response.into_body(),usize::MAX).await.unwrap().to_vec())
    }
    fn value(bytes:&[u8])->Value {serde_json::from_slice(bytes).unwrap()}
    async fn retained(state:&AppState)->Vec<Vec<Value>> {
        let mut conn=state.db.acquire().await.unwrap();let mut all=vec![];
        for query in ["SELECT * FROM documents","SELECT * FROM document_drafts","SELECT * FROM document_viewer_grants","SELECT * FROM attachments","SELECT * FROM audit_log"] {
            all.push(super::super::document_deletion::rows(&mut conn,query,&[]).await.unwrap());
        }all
    }
    #[tokio::test]
    async fn signed_nonmember_reads_only_published_metadata_body_asset_discovery_and_search() {
        let f=fixture(false).await;let before=retained(&f.state).await;
        let (status,bytes)=call(&f.state,"reader-a","GET","/spaces/space-a/documents/target-a",Value::Null).await;
        assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&bytes));let meta=value(&bytes);
        assert_eq!(meta["page_access_source"],"direct_page_grant");assert_eq!(meta["title"],"Visible published page");assert!(meta["folder_id"].is_null());assert!(meta["parent_id"].is_null());assert!(meta["draft_id"].is_null());
        let (status,bytes)=call(&f.state,"reader-a","GET","/spaces/space-a/documents/target-a/content",Value::Null).await;assert_eq!(status,StatusCode::OK);assert_eq!(value(&bytes)["content"],BODY);assert_eq!(value(&bytes)["published_source_digest"].as_str().unwrap().len(),64);
        let (status,bytes)=call(&f.state,"reader-a","GET",&format!("/files/{}",f.relative_asset),Value::Null).await;assert_eq!(status,StatusCode::OK);assert_eq!(bytes,b"PAGE OWNED ASSET EXACT");
        let (status,bytes)=call(&f.state,"reader-a","GET","/spaces/space-a/documents/target-a/attachments",Value::Null).await;assert_eq!(status,StatusCode::OK);assert_eq!(value(&bytes).as_array().unwrap().len(),1);
        let (status,bytes)=call(&f.state,"reader-a","POST","/shared-pages/inspect",json!({})).await;assert_eq!(status,StatusCode::OK);assert_eq!(value(&bytes)["pages"].as_array().unwrap().len(),1);assert!(value(&bytes)["next_cursor"].is_null());
        let (status,bytes)=call(&f.state,"reader-a","GET","/search?q=nebula",Value::Null).await;assert_eq!(status,StatusCode::OK);let result=value(&bytes);assert_eq!(result.as_array().unwrap().len(),1);assert_eq!(result[0]["space_name"],"");assert!(result[0]["folder_id"].is_null());assert_eq!(result[0]["author"],"");
        for uri in ["/spaces/space-a","/spaces/space-a/documents","/spaces/space-a/attachments","/spaces/space-a/documents/target-a/drafts"] {assert_ne!(call(&f.state,"reader-a","GET",uri,Value::Null).await.0,StatusCode::OK,"{uri}");}
        assert_eq!(retained(&f.state).await,before);assert_eq!(tokio::fs::read(&f.body_path).await.unwrap(),BODY.as_bytes());f.close().await;
    }
    #[tokio::test]
    async fn granted_search_rejects_hidden_filter_oracles_and_dataview_while_inheritance_keeps_labels() {
        let f=fixture(false).await;
        for uri in ["/search?q=space_id%3Aspace-a","/search?q=nebula&folder=hidden-folder","/search?q=nebula&user=owner-a","/search?q=nebula&has_property=false","/search?q=nebula&tag=absent","/dataview?q=NOT%20folder%3Ahidden-folder"] {
            let (status,bytes)=call(&f.state,"reader-a","GET",uri,Value::Null).await;assert_eq!(status,StatusCode::OK,"{uri}");assert_eq!(value(&bytes),json!([]),"{uri}");
        }
        assert_ne!(call(&f.state,"reader-a","GET","/search?q=nebula&space_id=space-a",Value::Null).await.0,StatusCode::OK);
        sqlx::query("INSERT INTO tags(id,space_id,name) VALUES('hidden-tag','space-a','HIDDEN_TAG_ORACLE')").execute(&f.state.db).await.unwrap();
        sqlx::query("INSERT INTO doc_tags(document_id,tag_id) VALUES('target-a','hidden-tag')").execute(&f.state.db).await.unwrap();
        f.state.search_index.upsert(crate::search::index::DocEntry{doc_id:"target-a".into(),space_id:"space-a".into(),title:"Visible published page".into(),body:BODY.into(),tags:"HIDDEN_TAG_ORACLE".into()}).unwrap();
        let (status,bytes)=call(&f.state,"reader-a","GET","/search?q=HIDDEN_TAG_ORACLE",Value::Null).await;assert_eq!(status,StatusCode::OK);assert_eq!(value(&bytes),json!([]));

        let (status,bytes)=call(&f.state,"viewer-a","GET","/search?q=nebula",Value::Null).await;assert_eq!(status,StatusCode::OK);assert_eq!(value(&bytes)[0]["space_name"],"HIDDEN SPACE LABEL");assert_eq!(value(&bytes)[0]["folder_name"],"HIDDEN FOLDER LABEL");
        let (status,bytes)=call(&f.state,"viewer-a","GET","/spaces/space-a/documents/target-a",Value::Null).await;assert_eq!(status,StatusCode::OK);assert!(value(&bytes).get("page_access_source").is_none());f.close().await;
    }
    #[tokio::test]
    async fn actual_grant_does_not_open_unpublished_lifecycle_parent_or_blocked_and_restricted_pages() {
        for change in ["UPDATE documents SET published_at=NULL WHERE id='target-a'","UPDATE documents SET archived_at='archived' WHERE id='target-a'","UPDATE documents SET deleted_at='deleted' WHERE id='target-a'","UPDATE users SET is_active=0 WHERE id='reader-a'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','reader-a','owner-a')","INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('target-a','space-a','restricted','restricted-1','owner-a')"] {
            let f=fixture(false).await;sqlx::query(change).execute(&f.state.db).await.unwrap();let before=retained(&f.state).await;
            assert_ne!(call(&f.state,"reader-a","GET","/spaces/space-a/documents/target-a/content",Value::Null).await.0,StatusCode::OK,"{change}");
            let (status,bytes)=call(&f.state,"reader-a","POST","/shared-pages/inspect",json!({})).await;if status==StatusCode::OK {assert_eq!(value(&bytes)["pages"],json!([]));}
            assert_eq!(retained(&f.state).await,before);f.close().await;
        }
        let f=fixture(false).await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('hidden-parent','space-a','HIDDEN PARENT','','owner-a','owner-a',NULL)").execute(&f.state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('parent-draft','hidden-parent','space-a','owner-a','HIDDEN PARENT','old')").execute(&f.state.db).await.unwrap();
        sqlx::query("UPDATE documents SET parent_id='hidden-parent' WHERE id='target-a'").execute(&f.state.db).await.unwrap();
        assert_ne!(call(&f.state,"reader-a","GET","/spaces/space-a/documents/target-a/content",Value::Null).await.0,StatusCode::OK);
        assert_ne!(call(&f.state,"reader-a","GET","/spaces/space-a/documents/hidden-parent",Value::Null).await.0,StatusCode::OK);f.close().await;
    }
    #[tokio::test]
    async fn real_read_api_key_can_post_discovery_but_write_only_and_service_keys_cannot() {
        let f=fixture(false).await;
        for (id,scopes,service,expected) in [("read","[\"read\"]",0,true),("write","[\"write\"]",0,false),("service","[\"read\"]",1,false)] {
            let raw=format!("viewer-key-{id}");
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES($1,'reader-a',$1,$2,'test',$3,$4)").bind(id).bind(crate::auth::api_keys::hash_api_key(&raw)).bind(service).bind(scopes).execute(&f.state.db).await.unwrap();
            let response=super::router().with_state(f.state.clone()).oneshot(Request::builder().method("POST").uri("/shared-pages/inspect").header("X-API-Key",raw).header("Content-Type","application/json").body(Body::from("{}")).unwrap()).await.unwrap();
            assert_eq!(response.status()==StatusCode::OK,expected,"{id}");
        }f.close().await;
    }
    #[tokio::test]
    async fn visible_keyset_cursor_has_no_skipped_hidden_identity_or_total_and_is_credential_bound() {
        let f=fixture(false).await;
        for i in 0..24 {
            let id=format!("page-{i:02}");sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES($1,'space-a',$1,'','owner-a','owner-a','published')").bind(&id).execute(&f.state.db).await.unwrap();grant(&f.state,&id).await;
        }
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('page-00','space-a','restricted','hidden','owner-a')").execute(&f.state.db).await.unwrap();
        let bearer=token(&f.state,"reader-a");
        let (status,bytes)=call_with_token(&f.state,&bearer,"POST","/shared-pages/inspect",json!({})).await;assert_eq!(status,StatusCode::OK);let first=value(&bytes);assert_eq!(first["pages"].as_array().unwrap().len(),20);assert!(!String::from_utf8_lossy(&bytes).contains("page-00"));assert!(first.get("total").is_none());
        let cursor=first["next_cursor"].as_str().unwrap();let cursor_json:Value=serde_json::from_slice(&hex::decode(cursor.split_once('.').unwrap().0).unwrap()).unwrap();assert_eq!(cursor_json["after"],first["pages"][19]["id"]);
        let (status,bytes)=call_with_token(&f.state,&bearer,"POST","/shared-pages/inspect",json!({"cursor":cursor})).await;assert_eq!(status,StatusCode::OK);let second=value(&bytes);assert_eq!(second["pages"].as_array().unwrap().len(),4);assert!(second["next_cursor"].is_null());
        assert_ne!(call(&f.state,"viewer-a","POST","/shared-pages/inspect",json!({"cursor":cursor})).await.0,StatusCode::OK);
        let mut changed=crate::auth::jwt::validate_token(&bearer,&f.state.jwt_secret,crate::auth::jwt::TokenType::Access).unwrap();
        changed.exp+=1;
        let changed_bearer=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&changed,&jsonwebtoken::EncodingKey::from_secret(f.state.jwt_secret.as_bytes())).unwrap();
        assert_ne!(changed_bearer,bearer);
        assert_eq!(call_with_token(&f.state,&changed_bearer,"POST","/shared-pages/inspect",json!({"cursor":cursor})).await.0,StatusCode::BAD_REQUEST);
        f.close().await;
    }
    #[tokio::test]
    async fn reached_disk_wal_grant_revoke_after_io_rejects_all_published_reader_projections() {
        for (method,uri) in [("GET","/spaces/space-a/documents/target-a"),("GET","/spaces/space-a/documents/target-a/content"),("GET","ASSET"),("GET","/spaces/space-a/documents/target-a/attachments"),("GET","/search?q=nebula"),("POST","/shared-pages/inspect")] {
            let f=fixture(true).await;let db=f.state.db.clone();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let flag=reached.clone();
            READER_WAIT.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {let mut external=db.acquire().await.unwrap();sqlx::query("DELETE FROM document_viewer_grants WHERE document_id='target-a'").execute(&mut *external).await.unwrap();flag.store(true,std::sync::atomic::Ordering::SeqCst);})));
            let uri=if uri=="ASSET" {format!("/files/{}",f.relative_asset)}else{uri.into()};let (status,bytes)=call(&f.state,"reader-a",method,&uri,json!({})).await;assert!(reached.load(std::sync::atomic::Ordering::SeqCst),"{uri}");
            if uri.starts_with("/search") {assert_eq!(status,StatusCode::OK);assert_eq!(value(&bytes),json!([]));}else{assert_ne!(status,StatusCode::OK,"{uri}");}
            assert_eq!(tokio::fs::read(&f.body_path).await.unwrap(),BODY.as_bytes());assert_eq!(tokio::fs::read(&f.asset_path).await.unwrap(),b"PAGE OWNED ASSET EXACT");f.close().await;
        }
    }
    #[tokio::test]
    async fn reached_wal_ancestor_policy_block_activity_and_original_session_reproof_are_current() {
        for change in ["INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','reader-a','owner-a')","UPDATE users SET is_active=0 WHERE id='reader-a'","UPDATE sessions SET expires_at='2000-01-01T00:00:00Z' WHERE id='viewer-reader-a'","UPDATE documents SET title='LATER SECRET LABEL' WHERE id='target-a'","INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('target-a','space-a','restricted','late','owner-a')"] {
            let f=fixture(true).await;let db=f.state.db.clone();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let flag=reached.clone();let change=change.to_owned();
            READER_WAIT.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {sqlx::query(&change).execute(&db).await.unwrap();flag.store(true,std::sync::atomic::Ordering::SeqCst);})));
            let (status,bytes)=call(&f.state,"reader-a","GET","/spaces/space-a/documents/target-a/content",Value::Null).await;assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert_ne!(status,StatusCode::OK);assert!(!String::from_utf8_lossy(&bytes).contains("LATER SECRET LABEL"));assert_eq!(tokio::fs::read(&f.body_path).await.unwrap(),BODY.as_bytes());f.close().await;
        }
    }
    #[tokio::test]
    async fn inherited_actor_catalog_pins_full_grant_even_when_revoked_before_source_capture() {
        let f=fixture(true).await;
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('reader-inherited','space-a','reader-a','viewer')").execute(&f.state.db).await.unwrap();
        let db=f.state.db.clone();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let flag=reached.clone();
        DISCOVERY_CAPTURE_WAIT.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {sqlx::query("DELETE FROM document_viewer_grants WHERE document_id='target-a'").execute(&db).await.unwrap();flag.store(true,std::sync::atomic::Ordering::SeqCst);})));
        let (status,bytes)=call(&f.state,"reader-a","POST","/shared-pages/inspect",json!({})).await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert_ne!(status,StatusCode::OK);assert!(!String::from_utf8_lossy(&bytes).contains("Visible published page"));
        assert_eq!(call(&f.state,"reader-a","GET","/spaces/space-a/documents/target-a/content",Value::Null).await.0,StatusCode::OK);
        let (status,bytes)=call(&f.state,"reader-a","POST","/shared-pages/inspect",json!({})).await;assert_eq!(status,StatusCode::OK);assert_eq!(value(&bytes)["pages"],json!([]));f.close().await;
    }
    #[tokio::test]
    async fn served_published_file_bytes_are_reproved_after_reached_io_wait() {
        let f=fixture(true).await;let path=f.body_path.clone();
        READER_WAIT.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {tokio::fs::write(path,"LATER FILE SECRET").await.unwrap();})));
        let (status,bytes)=call(&f.state,"reader-a","GET","/spaces/space-a/documents/target-a/content",Value::Null).await;assert_ne!(status,StatusCode::OK);assert!(!String::from_utf8_lossy(&bytes).contains("LATER FILE SECRET"));
        tokio::fs::write(&f.body_path,BODY).await.unwrap();
        let path=f.asset_path.clone();READER_WAIT.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {tokio::fs::write(path,"LATER ASSET SECRET").await.unwrap();})));
        let (status,bytes)=call(&f.state,"reader-a","GET",&format!("/files/{}",f.relative_asset),Value::Null).await;assert_ne!(status,StatusCode::OK);assert!(!String::from_utf8_lossy(&bytes).contains("LATER ASSET SECRET"));f.close().await;
    }

    #[tokio::test]
    async fn child_page_grant_does_not_open_parent_and_late_parent_restriction_overrides_child() {
        let f=fixture(true).await;
        sqlx::query("INSERT INTO documents(id,space_id,folder_id,title,content_path,created_by,last_updated_by,published_at) VALUES('ancestor-a','space-a','hidden-folder','HIDDEN ANCESTOR','','owner-a','owner-a','published')").execute(&f.state.db).await.unwrap();
        sqlx::query("UPDATE documents SET parent_id='ancestor-a' WHERE id='target-a'").execute(&f.state.db).await.unwrap();
        assert_eq!(call(&f.state,"reader-a","GET","/spaces/space-a/documents/target-a/content",Value::Null).await.0,StatusCode::OK);
        assert_ne!(call(&f.state,"reader-a","GET","/spaces/space-a/documents/ancestor-a/content",Value::Null).await.0,StatusCode::OK);
        let db=f.state.db.clone();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let flag=reached.clone();
        READER_WAIT.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('ancestor-a','space-a','restricted','late-parent','owner-a')").execute(&db).await.unwrap();flag.store(true,std::sync::atomic::Ordering::SeqCst);})));
        let (status,bytes)=call(&f.state,"reader-a","GET","/spaces/space-a/documents/target-a/content",Value::Null).await;assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert_ne!(status,StatusCode::OK);assert!(!String::from_utf8_lossy(&bytes).contains("HIDDEN ANCESTOR"));f.close().await;
    }
    #[tokio::test]
    async fn reached_original_read_key_revocation_denies_discovery_without_catalog_leak() {
        let f=fixture(true).await;let raw="original-viewer-key";
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('original-viewer-key','reader-a','Read',$1,'test',0,'[\"read\"]')").bind(crate::auth::api_keys::hash_api_key(raw)).execute(&f.state.db).await.unwrap();
        let db=f.state.db.clone();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let flag=reached.clone();
        READER_WAIT.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {sqlx::query("DELETE FROM api_keys WHERE id='original-viewer-key'").execute(&db).await.unwrap();flag.store(true,std::sync::atomic::Ordering::SeqCst);})));
        let response=super::router().with_state(f.state.clone()).oneshot(Request::builder().method("POST").uri("/shared-pages/inspect").header("X-API-Key",raw).header("Content-Type","application/json").body(Body::from("{}")).unwrap()).await.unwrap();
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert_ne!(response.status(),StatusCode::OK);
        let bytes=to_bytes(response.into_body(),usize::MAX).await.unwrap();assert!(!String::from_utf8_lossy(&bytes).contains("Visible published page"));f.close().await;
    }

    #[tokio::test]
    async fn final_multirow_discovery_and_search_sweep_rejects_earlier_page_change_during_later_policy() {
        for uri in ["/shared-pages/inspect","/search"] {
            for change in ["DELETE FROM document_viewer_grants WHERE document_id='target-a'","UPDATE documents SET title='LATE EARLIER TITLE' WHERE id='target-a'","INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('target-a','space-a','restricted','late-earlier','owner-a')"] {
                let f=fixture(true).await;
                sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at,updated_at) VALUES('target-z','space-a','Later published page',$1,'owner-a','owner-a','published','2000-01-01')").bind(f.body_path.to_string_lossy().as_ref()).execute(&f.state.db).await.unwrap();grant(&f.state,"target-z").await;
                f.state.search_index.upsert(crate::search::index::DocEntry {doc_id:"target-z".into(),space_id:"space-a".into(),title:"Later published page".into(),body:BODY.into(),tags:String::new()}).unwrap();
                let db=f.state.db.clone();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let flag=reached.clone();let change=change.to_owned();
                POLICY_WAIT.with(|slot|*slot.borrow_mut()=Some(("target-z".into(),Box::pin(async move {sqlx::query(&change).execute(&db).await.unwrap();flag.store(true,std::sync::atomic::Ordering::SeqCst);})))) ;
                let (status,bytes)=call(&f.state,"reader-a",if uri.starts_with("/shared") {"POST"}else{"GET"},uri,json!({})).await;
                assert!(reached.load(std::sync::atomic::Ordering::SeqCst),"{uri}");
                if uri.starts_with("/shared") {assert_ne!(status,StatusCode::OK);}else{assert_eq!(status,StatusCode::OK);assert!(!value(&bytes).as_array().unwrap().iter().any(|page|page["document_id"]=="target-a"));}
                assert!(!String::from_utf8_lossy(&bytes).contains("LATE EARLIER TITLE"));assert_eq!(tokio::fs::read(&f.body_path).await.unwrap(),BODY.as_bytes());f.close().await;
            }
        }
    }

}
