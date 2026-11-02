//! Include at crate root under cfg(test), in a disposable carrier only.
//! No fabricated journal proofs/receipts or restoration envelopes.
use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
use serde_json::{json,Value};
use sqlx::AnyPool;
use std::{path::{Path,PathBuf},collections::BTreeSet};
use tower::ServiceExt;

const SPACE:&str="restore-space";
const PAGE:&str="restore-page";
const SECRET:&str="restore-secret";
const NEIGHBOR:&str="restore-neighbor";
const UNPUBLISHED:&str="restore-unpublished";
const PRIVATE:&str="PRIVATE_RESTORE_DRAFT_NEVER_PUBLISHED";
const SECRET_BODY:&str="RESTOREMARKER SECRET_RESTORE_BODY";
const BINARY:&[u8]=b"\x00RESTORED\xff\x80\r\nATTACHMENT\x00EXACT";
const JWT:&str="owned-disposable-application-acceptance-jwt-secret";

fn workspace()->PathBuf {PathBuf::from(std::env::var("STARLANE_RESTORATION_APP_WORKSPACE").unwrap())}
async fn pool(root:&Path)->AnyPool {
    sqlx::any::install_default_drivers();
    sqlx::any::AnyPoolOptions::new().max_connections(2).after_connect(|c,_|Box::pin(async move {
        sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *c).await?;
        sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;Ok(())
    })).connect(&format!("sqlite://{}?mode=rwc",root.join("starlane-notes.db").display())).await.unwrap()
}
fn state(db:AnyPool,root:&Path)->crate::AppState {
    crate::AppState {db,jwt_secret:JWT.into(),base_url:"http://disposable.invalid".into(),
        search_index:crate::search::SearchIndex::new(root.join("search_index")).unwrap(),
        oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()}
}
fn bearer(actor:&str)->String {
    crate::auth::jwt::create_token_pair_for_session(actor,&format!("{actor}@acceptance.invalid"),
        if actor=="admin" {"admin"} else if actor=="guest" {"viewer"} else {"member"},
        JWT,Some(&format!("acceptance-{actor}")),30).unwrap().access_token
}
async fn raw(s:&crate::AppState,actor:Option<&str>,method:&str,uri:&str,content_type:&str,bytes:Vec<u8>)->(StatusCode,Vec<u8>) {
    let mut r=Request::builder().method(method).uri(uri).header("Content-Type",content_type);
    if let Some(actor)=actor {r=r.header("Authorization",format!("Bearer {}",bearer(actor)));}
    let response=crate::api_routes().with_state(s.clone()).oneshot(r.body(Body::from(bytes)).unwrap()).await.unwrap();
    let code=response.status();let bytes=to_bytes(response.into_body(),8*1024*1024).await.unwrap().to_vec();(code,bytes)
}
async fn call(s:&crate::AppState,actor:Option<&str>,method:&str,uri:&str,value:Value)->(StatusCode,Vec<u8>) {
    raw(s,actor,method,uri,"application/json",serde_json::to_vec(&value).unwrap()).await
}
async fn ok(s:&crate::AppState,actor:&str,method:&str,uri:&str,value:Value)->Value {
    let (code,bytes)=call(s,Some(actor),method,uri,value).await;
    assert!(code.is_success(),"{actor} {method} {uri}: {code} {}",String::from_utf8_lossy(&bytes));
    if bytes.is_empty(){Value::Null}else{serde_json::from_slice(&bytes).unwrap()}
}
async fn denied(s:&crate::AppState,actor:Option<&str>,uri:&str)->StatusCode {
    let (code,bytes)=call(s,actor,"GET",uri,Value::Null).await;
    assert!(matches!(code,StatusCode::UNAUTHORIZED|StatusCode::FORBIDDEN|StatusCode::NOT_FOUND),"Unexpected denial {code} at {uri}");
    for secret in [PRIVATE,SECRET_BODY] {assert!(!String::from_utf8_lossy(&bytes).contains(secret));}
    assert_ne!(bytes,BINARY);code
}
fn page_uri(id:&str)->String {format!("/spaces/{SPACE}/documents/{id}")}
async fn create_page(s:&crate::AppState,id:&str,body:&str,publish:bool)->Value {
    let created=ok(s,"owner","POST",&format!("/spaces/{SPACE}/documents"),json!({"source_id":id,"title":id,"content":body,"human_identifier":if id==PAGE {Some("RESTORE-042")}else{None},"property_values":[]})).await;
    assert_eq!(created["id"],id);assert!(!created.get("storage_pending").is_some_and(|v|v==true));
    if publish {
        let draft=created["draft_id"].as_str().unwrap();
        ok(s,"owner","POST",&format!("{}/drafts/{draft}/publish",page_uri(id)),json!({})).await;
    }
    created
}
async fn viewer_grant(s:&crate::AppState,user:&str,action:&str) {
    let proposal=json!({"user_id":user,"action":action});let prefix=format!("{}/viewer-grants",page_uri(PAGE));
    let review=ok(s,"owner","POST",&format!("{prefix}/review"),proposal.clone()).await;
    let confirmed=ok(s,"owner","POST",&format!("{prefix}/confirm"),json!({"proposal":proposal,"source_digest":review["source_digest"],"confirm":true})).await;
    assert_eq!(confirmed["confirmed"],true);
}
async fn source(root:&Path) {
    let db=pool(root).await;sqlx::migrate!("./migrations").run(&db).await.unwrap();
    for actor in ["owner","editor","spaceviewer","directviewer","guest","outsider","admin"] {
        sqlx::query("INSERT INTO users(id,email,display_name,role,account_kind) VALUES($1,$2,$1,$3,$4)")
            .bind(actor).bind(format!("{actor}@acceptance.invalid")).bind(if actor=="admin" {"admin"}else{"member"})
            .bind(if actor=="guest" {"guest"}else{"member"}).execute(&db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,session_kind) VALUES($1,$2,'fixture-only','2999-01-01T00:00:00Z','browser')")
            .bind(format!("acceptance-{actor}")).bind(actor).execute(&db).await.unwrap();
    }
    sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,'Restore acceptance','restore-acceptance','private','owner')").bind(SPACE).execute(&db).await.unwrap();
    for (actor,role) in [("owner","owner"),("editor","editor"),("spaceviewer","viewer")] {
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,$2,$3,$4)").bind(format!("membership-{actor}")).bind(SPACE).bind(actor).bind(role).execute(&db).await.unwrap();
    }
    let s=state(db,root);
    let property=ok(&s,"owner","POST",&format!("/spaces/{SPACE}/properties"),json!({"source_id":"restore-property","name":"Recovery proof","property_type":"text"})).await;
    let main=create_page(&s,PAGE,"RESTOREMARKER PUBLIC_RESTORE_BODY",true).await;
    create_page(&s,SECRET,SECRET_BODY,true).await;
    create_page(&s,NEIGHBOR,"NEIGHBOR_RESTORE_BODY",true).await;
    create_page(&s,UNPUBLISHED,"UNPUBLISHED_RESTORE_BODY",false).await;
    ok(&s,"owner","PUT",&format!("{}/properties",page_uri(PAGE)),json!({"values":[{"property_id":property["id"],"value":"same value after restoration"}]})).await;
    let restricted=ok(&s,"owner","POST",&format!("{}/restrictions/preview",page_uri(SECRET)),json!({"mode":"restricted","allowlist":{"users":["owner"],"groups":[]}})).await;
    ok(&s,"owner","POST",&format!("{}/restrictions/apply",page_uri(SECRET)),json!({"preview_token":restricted["preview_token"],"confirm":true})).await;
    // Upload through Multipart extractor and real application storage, not copied metadata.
    let mut multipart=b"--acceptance-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"owned.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec();multipart.extend_from_slice(BINARY);multipart.extend_from_slice(b"\r\n--acceptance-boundary--\r\n");
    let (code,bytes)=raw(&s,Some("owner"),"POST",&format!("{}/attachments",page_uri(PAGE)),"multipart/form-data; boundary=acceptance-boundary",multipart).await;
    assert_eq!(code,StatusCode::CREATED,"{}",String::from_utf8_lossy(&bytes));let asset:Value=serde_json::from_slice(&bytes).unwrap();
    let asset_uri=asset["url"].as_str().unwrap().strip_prefix("/api/v1").unwrap().to_string();
    let draft=ok(&s,"owner","POST",&format!("{}/drafts",page_uri(PAGE)),json!({})).await;let draft_id=draft["id"].as_str().unwrap();
    let body=format!("RESTOREMARKER PUBLIC_RESTORE_BODY\n[Owned attachment](/api/v1{asset_uri})\n");
    ok(&s,"owner","PUT",&format!("{}/drafts/{draft_id}",page_uri(PAGE)),json!({"source_digest":draft["source_digest"],"content":body})).await;
    ok(&s,"owner","POST",&format!("{}/drafts/{draft_id}/publish",page_uri(PAGE)),json!({})).await;
    let private=ok(&s,"owner","POST",&format!("{}/drafts",page_uri(PAGE)),json!({})).await;let private_id=private["id"].as_str().unwrap();
    ok(&s,"owner","PUT",&format!("{}/drafts/{private_id}",page_uri(PAGE)),json!({"source_digest":private["source_digest"],"content":PRIVATE})).await;
    ok(&s,"owner","PUT",&format!("{}/drafts/{private_id}/access/editor",page_uri(PAGE)),json!({"role":"editor"})).await;
    viewer_grant(&s,"directviewer","grant").await;viewer_grant(&s,"guest","grant").await;
    let link=ok(&s,"owner","POST",&format!("{}/publish",page_uri(PAGE)),json!({})).await;
    let content=ok(&s,"owner","GET",&format!("{}/content",page_uri(PAGE)),Value::Null).await;
    assert!(content["content"].as_str().unwrap().contains(&body));
    // Normalize only through actual ordinary startup recovery BEFORE stopped-writer capture.
    // Publication route installs immutable bytes before SQL; recovery completes its SQL
    // verified_at acknowledgment. Fresh recovery later intentionally refreshes it.
    crate::storage::relocation::recover(&s.db).await.unwrap();
    crate::storage::journal::recover(&s.db).await.unwrap();
    crate::storage::canonical_rewrite::recover(&s.db).await.unwrap();
    crate::storage::publication::recover(&s.db).await.unwrap();
    let receipt_count:i64=sqlx::query_scalar("SELECT count(*) FROM storage_journal_receipts WHERE state='complete'").fetch_one(&s.db).await.unwrap();assert!(receipt_count>=4);
    let publications:i64=sqlx::query_scalar("SELECT count(*) FROM publication_receipts").fetch_one(&s.db).await.unwrap();assert!(publications>=4);
    let evidence=json!({"page_id":main["id"],"space_id":SPACE,"property_id":property["id"],"private_draft_id":private_id,"asset":asset,"asset_uri":asset_uri,"public_slug":link["public_slug"],"published_content":content["content"],"storage_receipts":receipt_count,"publication_receipts":publications});
    std::fs::write(workspace().join("application-evidence.json"),serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();s.db.close().await;
}
async fn restore() {
    let w=workspace();let b=crate::storage::backup_inventory::verify(&w.join("bundle")).await.unwrap();
    let plan=crate::storage::restoration_plan::plan(&b).await.unwrap();
    let copied=crate::storage::restoration::reserve_and_copy(&b,&w.join("destination")).unwrap();
    let planned=crate::storage::restoration::materialize_groups(copied,&b,plan).unwrap();
    let sealed=crate::storage::restoration::seal_publication(&b,planned).await.unwrap();
    let committed=crate::storage::restoration::commit_publication(sealed).await.unwrap();
    let ready=crate::storage::restoration::retire_publication(committed).unwrap();
    assert_eq!(ready.path(),w.join("destination"));assert!(!ready.path().join(".starlane-restore-incomplete").exists());
    b.fence().unwrap();b.db.close().await;
}
async fn startup(root:&Path,db:&AnyPool) {
    crate::storage::restoration::bootstrap_gate(root).unwrap();
    crate::storage::restoration::activate_in(root,db).await.unwrap();
    crate::storage::relocation::recover(db).await.unwrap();
    crate::storage::journal::recover(db).await.unwrap();
    crate::storage::canonical_rewrite::recover(db).await.unwrap();
    let before:Vec<(String,String,Option<String>)>=sqlx::query_as("SELECT op_id,manifest_json,verified_at FROM publication_receipts ORDER BY op_id").fetch_all(db).await.unwrap();
    let recovery_started=chrono::Utc::now();
    crate::storage::publication::recover(db).await.unwrap();
    let after:Vec<(String,String,Option<String>)>=sqlx::query_as("SELECT op_id,manifest_json,verified_at FROM publication_receipts ORDER BY op_id").fetch_all(db).await.unwrap();
    let recovery_finished=chrono::Utc::now();
    assert_eq!(before.len(),4);assert_eq!(after.len(),before.len());
    let mut acknowledgments=Vec::new();
    for (old,new) in before.iter().zip(&after) {
        assert_eq!((&old.0,&old.1),(&new.0,&new.1));
        let time=chrono::DateTime::parse_from_rfc3339(new.2.as_deref().unwrap()).unwrap().with_timezone(&chrono::Utc);
        assert!(time>=recovery_started && time<=recovery_finished);
        acknowledgments.push(json!({"op_id":old.0,"manifest_json":old.1,"before_verified_at":old.2,"after_verified_at":new.2}));
    }
    if std::env::var("STARLANE_RESTORATION_APP_MODE").unwrap()=="startup" {
        std::fs::write(workspace().join("publication-recovery-audit.json"),serde_json::to_vec_pretty(&json!({"actual_operation":"publication::recover","started":recovery_started.to_rfc3339(),"finished":recovery_finished.to_rfc3339(),"receipts":acknowledgments})).unwrap()).unwrap();
    }
}
async fn audience(root:&Path) {
    let db=pool(root).await;startup(root,&db).await;let s=state(db,root);
    let e:Value=serde_json::from_slice(&std::fs::read(workspace().join("application-evidence.json")).unwrap()).unwrap();
    let content_uri=format!("{}/content",page_uri(PAGE));let asset_uri=e["asset_uri"].as_str().unwrap();let private_uri=format!("{}/drafts/{}",page_uri(PAGE),e["private_draft_id"].as_str().unwrap());
    for actor in ["owner","editor","spaceviewer","directviewer","guest"] {
        let content=ok(&s,actor,"GET",&content_uri,Value::Null).await;assert_eq!(content["id"],PAGE);assert_eq!(content["content"],e["published_content"]);
        let (code,bytes)=call(&s,Some(actor),"GET",asset_uri,Value::Null).await;assert_eq!(code,StatusCode::OK);assert_eq!(bytes,BINARY);
        if matches!(actor,"owner"|"editor") {let draft=ok(&s,actor,"GET",&private_uri,Value::Null).await;assert_eq!(draft["id"],e["private_draft_id"]);assert_eq!(draft["content"],PRIVATE);} else {denied(&s,Some(actor),&private_uri).await;}
    }
    for actor in ["editor","spaceviewer","directviewer","guest","outsider"] {denied(&s,Some(actor),&format!("{}/content",page_uri(SECRET))).await;}
    for actor in ["directviewer","guest","outsider"] {denied(&s,Some(actor),&format!("{}/content",page_uri(NEIGHBOR))).await;denied(&s,Some(actor),&format!("{}/content",page_uri(UNPUBLISHED))).await;}
    assert!(ok(&s,"owner","GET",&format!("{}/content",page_uri(SECRET)),Value::Null).await["content"].as_str().unwrap().contains(SECRET_BODY));
    denied(&s,Some("outsider"),&content_uri).await;assert_eq!(denied(&s,None,&content_uri).await,StatusCode::UNAUTHORIZED);assert_eq!(denied(&s,None,asset_uri).await,StatusCode::UNAUTHORIZED);
    let public_uri=format!("/p/{}",e["public_slug"].as_str().unwrap());let (code,bytes)=call(&s,None,"GET",&public_uri,Value::Null).await;assert_eq!(code,StatusCode::OK);let public:Value=serde_json::from_slice(&bytes).unwrap();assert_eq!(public["content"],e["published_content"]);
    denied(&s,None,"/p/unknown-not-a-public-link").await;
    let properties=ok(&s,"owner","GET",&format!("{}/properties",page_uri(PAGE)),Value::Null).await;assert!(properties.as_array().unwrap().iter().any(|r|r["property_id"]==e["property_id"]&&r["value"]=="same value after restoration"));
    // Derived rebuild is actual admin route; authorization evidence is actual search route.
    let rebuilt=ok(&s,"admin","POST","/admin/search/reindex",json!({})).await;assert_eq!(rebuilt["documents_indexed"],3);
    for actor in ["owner","editor","spaceviewer","directviewer","guest","outsider"] {
        let results=ok(&s,actor,"GET","/search?q=RESTOREMARKER",Value::Null).await;
        let ids:BTreeSet<_>=results.as_array().unwrap().iter().map(|r|r["document_id"].as_str().unwrap()).collect();
        let expected:BTreeSet<_>=if actor=="owner" {[PAGE,SECRET].into_iter().collect()}else if actor=="outsider" {BTreeSet::new()}else{[PAGE].into_iter().collect()};assert_eq!(ids,expected,"search audience {actor}");
        assert!(results.as_array().unwrap().iter().all(|r|r["is_draft"]==false));
        let private_results=ok(&s,actor,"GET","/search?q=PRIVATE_RESTORE_DRAFT_NEVER_PUBLISHED&is_draft=true",Value::Null).await;
        let found=private_results.as_array().unwrap().iter().any(|r|r["draft_id"]==e["private_draft_id"]);
        assert_eq!(found,matches!(actor,"owner"|"editor"),"private draft search audience {actor}");
    }
    viewer_grant(&s,"directviewer","revoke").await;denied(&s,Some("directviewer"),&content_uri).await;denied(&s,Some("directviewer"),asset_uri).await;
    let results=ok(&s,"directviewer","GET","/search?q=RESTOREMARKER",Value::Null).await;assert!(results.as_array().unwrap().is_empty());
    sqlx::query("DELETE FROM sessions WHERE id='acceptance-guest'").execute(&s.db).await.unwrap();assert_eq!(denied(&s,Some("guest"),&content_uri).await,StatusCode::UNAUTHORIZED);assert_eq!(denied(&s,Some("guest"),asset_uri).await,StatusCode::UNAUTHORIZED);
    ok(&s,"owner","POST",&format!("{}/unpublish",page_uri(PAGE)),json!({})).await;denied(&s,None,&public_uri).await;
    crate::storage::restoration::fence(root).unwrap();s.db.close().await;
    std::fs::write(workspace().join("application-acceptance.json"),serde_json::to_vec_pretty(&json!({"status":"passed","audiences":["owner","editor","spaceviewer","directviewer","guest","outsider","anonymous"],"attachment_exact_bytes":true,"revocations":["direct_page_grant","guest_session","public_link"],"actual_search_route_after_rebuild":true,"listener_or_browser_test":false})).unwrap()).unwrap();
}
#[tokio::test]
#[ignore = "explicit disposable application acceptance subprocess"]
async fn application_acceptance_child() {
    let mode=std::env::var("STARLANE_RESTORATION_APP_MODE").expect("explicit application acceptance mode required");
    std::env::var("DATA_DIR").expect("explicit disposable DATA_DIR required");
    let w=workspace();let root=crate::storage::data_dir();assert!(root.starts_with(&w));assert!(matches!(mode.as_str(),"source"|"restore"|"startup"|"audience"));
    match mode.as_str() {"source"=>source(&root).await,"restore"=>restore().await,"startup"=>{let db=pool(&root).await;startup(&root,&db).await;db.close().await;},"audience"=>audience(&root).await,_=>unreachable!()}
}
