//! Actual signed lifecycle-route writer loss and fresh-process startup/retry.
//! These fixtures claim process restart on Linux, not physical power loss.
use std::os::unix::fs::DirBuilderExt;
use super::*;
use axum::{body::{to_bytes, Body}, http::Request as HttpRequest};
use serde_json::{json, Value};
use std::{fs, io::Write, path::{Path as FsPath, PathBuf}, process::{Command, Stdio}, time::{Duration, Instant}};
use tower::ServiceExt;

const PREFIX: &str = "spaces::drafts::publication_route_restart_tests::";
const OLD_BODY: &str = "OLD READER BODY UNIQUE";
const NEXT_BODY: &str = "NEXT READER BODY UNIQUE";
const PHASES: &[&str] = &["before_prepare","after_stage","after_prepare","after_page_sql","after_audit","after_storage_receipt","after_route_receipt","after_consume","before_commit","after_commit","before_ack"];
fn committed(phase:&str)->bool {matches!(phase,"after_commit"|"before_ack")}
fn durable_json(path:&FsPath,value:&Value) {
    let mut file=fs::File::create(path).unwrap();
    file.write_all(&serde_json::to_vec(value).unwrap()).unwrap();file.sync_all().unwrap();
    fs::File::open(path.parent().unwrap()).unwrap().sync_all().unwrap();
}
fn original(root:&FsPath)->Value {serde_json::from_slice(&fs::read(root.join("original-request.json")).unwrap()).unwrap()}
async fn state(root:&FsPath)->AppState {
    // Same before-write gate as production, before SQLite opens or directory init.
    storage::restoration::bootstrap_gate(root).unwrap();storage::init_data_dir().await.unwrap();
    sqlx::any::install_default_drivers();
    let db=sqlx::any::AnyPoolOptions::new().max_connections(3).after_connect(|conn,_|Box::pin(async move {
        sqlx::query("PRAGMA journal_mode=WAL").execute(&mut *conn).await?;
        sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *conn).await?;
        sqlx::query("PRAGMA busy_timeout=5000").execute(conn).await?;Ok(())
    })).connect(&format!("sqlite://{}?mode=rwc",root.join("route.sqlite").display())).await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    let journal:String=sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&db).await.unwrap();assert_eq!(journal.to_lowercase(),"wal");
    AppState{db,jwt_secret:"publication-route-fixture-secret-32-bytes".into(),base_url:"http://fixture.invalid".into(),
        search_index:crate::search::SearchIndex::new(root.join("search")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()}
}
fn token(state:&AppState,user:&str)->String {
    let mut claims=crate::spaces::properties::tests::test_auth(user).claims;claims.sid=format!("crash-{user}");
    jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap()
}
async fn http(state:&AppState,user:Option<&str>,method:&str,url:&str,body:Option<Value>)->(StatusCode,Value) {
    let mut request=HttpRequest::builder().method(method).uri(url);
    if let Some(user)=user {request=request.header("Authorization",format!("Bearer {}",token(state,user)));}
    if body.is_some(){request=request.header("Content-Type","application/json");}
    let response=crate::api_routes().with_state(state.clone()).oneshot(request.body(body.map(|v|Body::from(v.to_string())).unwrap_or_else(Body::empty)).unwrap()).await.unwrap();
    let status=response.status();let bytes=to_bytes(response.into_body(),2_000_000).await.unwrap();
    (status,serde_json::from_slice(&bytes).unwrap_or_else(|_|json!({"unparsed":String::from_utf8_lossy(&bytes)})))
}
async fn seed(state:&AppState,root:&FsPath,legacy:bool) {
    for (user,role) in [("owner-a","owner"),("viewer-a","viewer")] {
        sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')").bind(user).bind(format!("{user}@fixture.invalid")).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,device_name) VALUES($1,$2,'fixture','2999-01-01T00:00:00Z','Browser')").bind(format!("crash-{user}")).bind(user).execute(&state.db).await.unwrap();
        if user=="owner-a" {sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('space-a','Crash fixture','crash','private','owner-a')").execute(&state.db).await.unwrap();}
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'space-a',$2,$3)").bind(format!("member-{user}")).bind(user).bind(role).execute(&state.db).await.unwrap();
    }
    let fm=super::super::documents::document_storage_frontmatter("Old title",Some("old-icon"),false,true);
    let mut path=storage::write_document("crash","page-a","Old title",&fm,OLD_BODY).await.unwrap();
    if legacy {let legacy="spaces/crash/docs/old-legacy.md";fs::rename(root.join(&path),root.join(legacy)).unwrap();path=legacy.into();}
    sqlx::query("INSERT INTO documents(id,space_id,title,icon,cover_image,content_path,created_by,last_updated_by,created_at,updated_at,published_at,public,public_slug) VALUES('page-a','space-a','Old title','old-icon','old-cover',$1,'owner-a','owner-a','source','source','source',1,'route-crash-link')").bind(&path).execute(&state.db).await.unwrap();
    sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,icon,cover_image,base_updated_at,created_at,updated_at) VALUES('draft-a','page-a','space-a','owner-a','New title',$1,'new-icon','new-cover','source','source','source')").bind(NEXT_BODY).execute(&state.db).await.unwrap();
    durable_json(&root.join("baseline.json"),&json!({"path":path,"bytes":storage::read_document(&path).await.unwrap(),"legacy":legacy}));
}
async fn counts(state:&AppState)->Value {
    let mut counts=serde_json::Map::new();
    for (label,query) in [("storage","SELECT count(*) FROM publication_receipts"),("route","SELECT count(*) FROM draft_publication_receipts"),("audit","SELECT count(*) FROM audit_log WHERE action='document.draft_publish'"),("draft","SELECT count(*) FROM document_drafts WHERE id='draft-a'")] {
        counts.insert(label.into(),json!(sqlx::query_scalar::<_,i64>(query).fetch_one(&state.db).await.unwrap()));
    }
    Value::Object(counts)
}
async fn verify_readers(state:&AppState,root:&FsPath,new:bool) {
    let base:Value=serde_json::from_slice(&fs::read(root.join("baseline.json")).unwrap()).unwrap();
    // Independent actual audience routes: space viewer has no private draft access.
    for user in [None,Some("viewer-a")] {
        let url=if user.is_none(){"/p/route-crash-link"}else{"/spaces/space-a/documents/page-a/content"};
        let (status,response)=http(state,user,"GET",url,None).await;assert_eq!(status,StatusCode::OK,"{response}");
        let expected=if new {
            let fm=super::super::documents::document_storage_frontmatter("New title",Some("new-icon"),false,true);
            format!("---\n{}\n---\n\n{}",fm.trim(),NEXT_BODY)
        } else {base["bytes"].as_str().unwrap().to_owned()};
        assert_eq!(response["content"],expected,"reader must see one complete revision");
        assert_eq!(response["title"],if new{"New title"}else{"Old title"});
    }
    let mut conn=state.db.acquire().await.unwrap();
    let row=super::publication_rows(&mut conn,"SELECT * FROM documents WHERE id='page-a'",&[]).await.unwrap().remove(0);
    assert_eq!(row["title"],if new{"New title"}else{"Old title"});assert_eq!(row["icon"],if new{"new-icon"}else{"old-icon"});assert_eq!(row["cover_image"],if new{"new-cover"}else{"old-cover"});
    assert_eq!(fs::read_to_string(root.join(base["path"].as_str().unwrap())).unwrap(),base["bytes"]);
}
#[tokio::test]
async fn publication_route_writer_child() {
    let Ok(root)=std::env::var("L3_ROUTE_ROOT") else{return};let root=PathBuf::from(root);assert_eq!(storage::data_dir(),root);
    let phase=std::env::var("L3_ROUTE_PHASE").unwrap();assert!(PHASES.contains(&phase.as_str()));
    let legacy=std::env::var_os("L3_ROUTE_LEGACY").is_some();let state=state(&root).await;seed(&state,&root,legacy).await;
    // Normal live startup precedes confirmation. Legacy case deliberately models
    // an already-serving legacy source interrupted before the upgrade restart.
    if !legacy {storage::startup::recover_and_normalize(&state.db).await.unwrap();}
    let(status,review)=http(&state,Some("owner-a"),"GET","/spaces/space-a/documents/page-a/drafts/draft-a/publish-source",None).await;assert_eq!(status,StatusCode::OK,"{review}");
    let request=json!({"schema_version":1,"operation_id":Uuid::new_v4().to_string(),"draft_source_digest":review["draft_source_digest"],"page_source_digest":review["page_source_digest"]});
    durable_json(&root.join("original-request.json"),&request);
    let marker=root.join("crash-marker.json");let chosen=phase.clone();
    let hook:Box<dyn Fn(&str,&str)+Send>=Box::new(move |point,op|{if point==chosen {durable_json(&marker,&json!({"phase":point,"operation_id":op}));std::process::exit(77);}});
    let result=if phase=="after_stage" {
        let marker=root.join("crash-marker.json");let op=request["operation_id"].as_str().unwrap().to_owned();
        let stage_hook:Box<dyn FnOnce()+Send>=Box::new(move ||{durable_json(&marker,&json!({"phase":"after_stage","operation_id":op}));std::process::exit(77);});
        storage::publication::PUBLICATION_AFTER_STAGE.scope(std::cell::RefCell::new(Some(stage_hook)),
            http(&state,Some("owner-a"),"POST","/spaces/space-a/documents/page-a/drafts/draft-a/publish",Some(request))).await
    }else{
        super::PUBLICATION_ROUTE_CRASH.scope(hook,http(&state,Some("owner-a"),"POST","/spaces/space-a/documents/page-a/drafts/draft-a/publish",Some(request))).await
    };
    panic!("selected crash point {phase} was not reached: {result:?}");
}
#[tokio::test]
async fn publication_route_restart_child() {
    let Ok(root)=std::env::var("L3_ROUTE_ROOT") else{return};let root=PathBuf::from(root);assert_eq!(storage::data_dir(),root);
    let phase=std::env::var("L3_ROUTE_PHASE").unwrap();let was_committed=committed(&phase);
    let marker:Value=serde_json::from_slice(&fs::read(root.join("crash-marker.json")).unwrap()).unwrap();let request=original(&root);
    assert_eq!(marker["phase"],phase);assert_eq!(marker["operation_id"],request["operation_id"]);
    let state=state(&root).await;
    // Actual shared production sequence includes custody, journal recovery AND
    // normalization; no copying the weaker recover-only fixture sequence.
    storage::startup::recover_and_normalize(&state.db).await.unwrap();
    let expected=if was_committed{json!({"storage":1,"route":1,"audit":1,"draft":0})}else{json!({"storage":0,"route":0,"audit":0,"draft":1})};
    assert_eq!(counts(&state).await,expected);verify_readers(&state,&root,was_committed).await;
    storage::startup::recover_and_normalize(&state.db).await.unwrap();assert_eq!(counts(&state).await,expected);verify_readers(&state,&root,was_committed).await;
    let(status,first)=http(&state,Some("owner-a"),"POST","/spaces/space-a/documents/page-a/drafts/draft-a/publish",Some(request.clone())).await;
    assert_eq!(status,StatusCode::OK,"original operation must reconcile, not silently rebase: {first}");
    assert_eq!(first["operation_id"],request["operation_id"]);assert_eq!(first["draft_source_digest"],request["draft_source_digest"]);assert_eq!(first["page_source_digest"],request["page_source_digest"]);
    assert_eq!(first["disposition"],if was_committed{"historical"}else{"applied"});
    assert_eq!(first["publication"],json!({"title":"New title","content":NEXT_BODY,"icon":"new-icon","cover_image":"new-cover"}));
    let all_once=json!({"storage":1,"route":1,"audit":1,"draft":0});assert_eq!(counts(&state).await,all_once);verify_readers(&state,&root,true).await;
    let mut historical=first.clone();historical["disposition"]=json!("historical");
    for _ in 0..2 {storage::startup::recover_and_normalize(&state.db).await.unwrap();let(status,response)=http(&state,Some("owner-a"),"POST","/spaces/space-a/documents/page-a/drafts/draft-a/publish",Some(request.clone())).await;assert_eq!(status,StatusCode::OK);assert_eq!(response,historical);assert_eq!(counts(&state).await,all_once);verify_readers(&state,&root,true).await;}
    durable_json(&root.join("restart-result.json"),&json!({"phase":phase,"operation_id":request["operation_id"],"counts":all_once,"first_disposition":first["disposition"]}));state.db.close().await;
}
fn child(root:&FsPath,name:&str,phase:&str,legacy:bool)->i32 {
    let log=root.join(format!("{name}.log"));let out=fs::File::create(&log).unwrap();
    let mut command=Command::new(std::env::current_exe().unwrap());command.args(["--exact",&format!("{PREFIX}{name}"),"--nocapture"])
        .env("DATA_DIR",root).env("ALLOW_PUBLIC_PAGES","true").env("L3_ROUTE_ROOT",root).env("L3_ROUTE_PHASE",phase)
        .stdout(Stdio::from(out.try_clone().unwrap())).stderr(Stdio::from(out));
    if legacy {command.env("L3_ROUTE_LEGACY","1");}else{command.env_remove("L3_ROUTE_LEGACY");}
    let mut process=command.spawn().unwrap();let deadline=Instant::now()+Duration::from_secs(30);
    loop {if let Some(status)=process.try_wait().unwrap(){return status.code().unwrap_or(-1);}if Instant::now()>deadline {process.kill().unwrap();let _=process.wait();panic!("child timeout {name}/{phase}; {}",log.display());}std::thread::sleep(Duration::from_millis(20));}
}
fn exercise(phase:&str,legacy:bool) {
    let root=std::env::temp_dir().join(format!("lifecycle-publication-route-{}",Uuid::new_v4()));fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    assert_eq!(child(&root,"publication_route_writer_child",phase,legacy),77,"writer did not reach checkpoint; root {}",root.display());
    let code=child(&root,"publication_route_restart_child",phase,legacy);
    assert_eq!(code,0,"restart failed {phase}: {}",fs::read_to_string(root.join("publication_route_restart_child.log")).unwrap());
    let result:Value=serde_json::from_slice(&fs::read(root.join("restart-result.json")).unwrap()).unwrap();assert_eq!(result["phase"],phase);assert_eq!(result["counts"],json!({"storage":1,"route":1,"audit":1,"draft":0}));
    fs::remove_dir_all(root).unwrap();
}
#[test]
fn actual_signed_publication_route_crash_restart_all_transaction_and_ack_boundaries() {for phase in PHASES {exercise(phase,false);}}
#[test]
fn orphan_publication_restart_preserves_original_legacy_source_before_normalization() {exercise("after_prepare",true);}


fn retained_evidence(root:&FsPath)->std::collections::BTreeMap<String,Vec<u8>> {
    fn walk(root:&FsPath,path:&FsPath,out:&mut std::collections::BTreeMap<String,Vec<u8>>) {
        if !path.exists(){return}for entry in fs::read_dir(path).unwrap(){let path=entry.unwrap().path();
            if path.is_dir(){walk(root,&path,out)}else{out.insert(path.strip_prefix(root).unwrap().to_string_lossy().into_owned(),fs::read(path).unwrap());}
        }
    }
    let mut out=std::collections::BTreeMap::new();walk(root,&root.join(".publication-journal"),&mut out);walk(root,&root.join("spaces/crash/docs/revisions"),&mut out);out
}
#[tokio::test]
async fn publication_route_refusal_child() {
    let Ok(root)=std::env::var("L3_ROUTE_ROOT") else{return};let root=PathBuf::from(root);assert_eq!(storage::data_dir(),root);
    let variant=fs::read_to_string(root.join("refusal-variant.txt")).unwrap();
    let mut request=original(&root);let state=state(&root).await;let mut actor="owner-a";
    match variant.as_str() {
        "draft_same_stamp"=>{sqlx::query("UPDATE document_drafts SET content='NEWER PRIVATE BODY' WHERE id='draft-a'").execute(&state.db).await.unwrap();},
        "page_same_stamp"=>{sqlx::query("UPDATE documents SET last_updated_by='viewer-a' WHERE id='page-a'").execute(&state.db).await.unwrap();},
        "foreign_request"=>{request["draft_source_digest"]=json!("0".repeat(64));},
        "foreign_actor"=>{actor="viewer-a";},
        "corrupt_install"=>{let op=request["operation_id"].as_str().unwrap();let path=root.join(format!("spaces/crash/docs/revisions/{op}.md"));let mut file=fs::OpenOptions::new().write(true).truncate(true).open(path).unwrap();file.write_all(b"CORRUPT RETAINED INSTALL").unwrap();file.sync_all().unwrap();},
        "revoked_authority"=>{sqlx::query("UPDATE space_members SET role='viewer' WHERE space_id='space-a' AND user_id='owner-a'").execute(&state.db).await.unwrap();},
        _=>panic!("unknown refusal case {variant}"),
    }
    let evidence=retained_evidence(&root);assert!(!evidence.is_empty());
    let expected=json!({"storage":0,"route":0,"audit":0,"draft":1});
    for _ in 0..2 {
        // Corrupt retained evidence may make startup itself refuse. It must never
        // acquire authority by repairing bytes; exact retry is still rejected.
        let _=storage::startup::recover_and_normalize(&state.db).await;
        assert_eq!(retained_evidence(&root),evidence,"startup modified retained evidence: {variant}");
        let(status,response)=http(&state,Some(actor),"POST","/spaces/space-a/documents/page-a/drafts/draft-a/publish",Some(request.clone())).await;
        assert!(status.is_client_error()||status.is_server_error(),"refused operation succeeded {variant}: {response}");
        assert_eq!(counts(&state).await,expected);assert_eq!(retained_evidence(&root),evidence,"retry restaged or rewrote retained evidence: {variant}");
        verify_readers(&state,&root,false).await;
    }
    if variant=="draft_same_stamp" {let body:String=sqlx::query_scalar("SELECT content FROM document_drafts WHERE id='draft-a'").fetch_one(&state.db).await.unwrap();assert_eq!(body,"NEWER PRIVATE BODY");}
    durable_json(&root.join("refusal-result.json"),&json!({"variant":variant,"counts":expected}));state.db.close().await;
}
#[test]
fn actual_orphan_route_retry_refuses_changed_sources_actor_request_corruption_and_authority() {
    for variant in ["draft_same_stamp","page_same_stamp","foreign_request","foreign_actor","corrupt_install","revoked_authority"] {
        let root=std::env::temp_dir().join(format!("lifecycle-publication-refusal-{}",Uuid::new_v4()));fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        assert_eq!(child(&root,"publication_route_writer_child","after_prepare",false),77,"writer did not reach checkpoint; root {}",root.display());
        fs::write(root.join("refusal-variant.txt"),variant).unwrap();
        let code=child(&root,"publication_route_refusal_child","after_prepare",false);
        assert_eq!(code,0,"refusal failed {variant}: {}",fs::read_to_string(root.join("publication_route_refusal_child.log")).unwrap());
        let result:Value=serde_json::from_slice(&fs::read(root.join("refusal-result.json")).unwrap()).unwrap();assert_eq!(result["variant"],variant);
        fs::remove_dir_all(root).unwrap();
    }
}
