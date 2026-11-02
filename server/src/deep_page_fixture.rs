//! Opt-in real migrated SQLite/PostgreSQL page routes, synthetic owned data only.
use axum::{body::{to_bytes, Body}, http::{Request, StatusCode}};
use serde_json::{json, Value};
use sqlx::{AnyPool, Connection};
use tower::ServiceExt;

const SPACE: &str = "deep-page-source";
const TARGET: &str = "deep-page-target";
const JWT: &str = "fictional-deep-page-fixture-secret";

fn token(actor: &str) -> String {
    crate::auth::jwt::create_token_pair_for_session(actor, &format!("{actor}@deep.invalid"),
        "member", JWT, Some(&format!("deep-{actor}")), 30).unwrap().access_token
}
async fn request(s: &crate::AppState, actor: &str, method: &str, url: &str, body: Value) -> (StatusCode, Value) {
    let response = crate::api_routes().with_state(s.clone()).oneshot(Request::builder()
        .method(method).uri(url).header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token(actor)))
        .body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 16 * 1024 * 1024).await.unwrap();
    let value = if bytes.is_empty() { Value::Null } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({"unparsed_error":String::from_utf8_lossy(&bytes)}))
    };
    (status, value)
}
async fn call(s: &crate::AppState, actor: &str, method: &str, url: &str, body: Value, expected: StatusCode) -> Value {
    let (status, result) = request(s, actor, method, url, body).await;
    assert_eq!(status, expected, "{actor} {method} {url}: {result}"); result
}
async fn domain(s: &crate::AppState) -> Value {
    let mut conn = s.db.acquire().await.unwrap();
    let mut result = serde_json::Map::new();
    for table in ["folders", "users", "space_members", "documents", "document_drafts", "document_publication_snapshots", "draft_publication_receipts", "snapshot_recovery_receipts", "audit_log"] {
        let rows = crate::spaces::document_deletion::rows(&mut conn, &format!("SELECT * FROM {table}"), &[]).await.unwrap();
        let mut rows = rows; rows.sort_by_cached_key(Value::to_string);
        result.insert(table.into(), json!(rows));
    }
    Value::Object(result)
}
async fn setup(postgres: bool) -> (crate::AppState, String) {
    let owned = std::path::PathBuf::from(std::env::var("STARLANE_CORE_WORKSPACE").expect("required owned workspace"));
    let root = crate::storage::data_dir();
    assert!(owned.is_absolute() && root.starts_with(&owned) && root != owned);
    assert!(!root.exists(), "fixture requires a fresh DATA_DIR");
    std::fs::create_dir_all(&root).unwrap();
    sqlx::any::install_default_drivers();
    let schema = format!("deep_page_{}", uuid::Uuid::new_v4().simple());
    let db: AnyPool = if postgres {
        let url = std::env::var("STARLANE_CORE_PG_URL").expect("required owned PG URL");
        assert!(url.starts_with("postgres://"));
        let mut control = sqlx::AnyConnection::connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&mut control).await.unwrap();
        control.close().await.unwrap();
        let selected = schema.clone();
        sqlx::any::AnyPoolOptions::new().max_connections(4).after_connect(move |c,_| {
            let name=selected.clone(); Box::pin(async move {sqlx::query(&format!("SET search_path TO {name}")).execute(c).await?;Ok(())})
        }).connect(&url).await.unwrap()
    } else {
        sqlx::any::AnyPoolOptions::new().max_connections(4).after_connect(|c,_| Box::pin(async move {
            sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *c).await?;
            sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?; Ok(())
        })).connect(&format!("sqlite://{}?mode=rwc",root.join("deep-pages.sqlite").display())).await.unwrap()
    };
    crate::db::migrate_selected(&db, if postgres {crate::db::DatabaseEngine::Postgres} else {crate::db::DatabaseEngine::Sqlite}).await.unwrap();
    for actor in ["owner","editor","viewer"] {
        sqlx::query("INSERT INTO users(id,email,display_name,role,account_kind) VALUES($1,$2,$1,'member','member')").bind(actor).bind(format!("{actor}@deep.invalid")).execute(&db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,session_kind) VALUES($1,$2,'fictional','2999-01-01T00:00:00Z','browser')").bind(format!("deep-{actor}")).bind(actor).execute(&db).await.unwrap();
    }
    for space in [SPACE,TARGET] {
        sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,$1,$1,'private','owner')").bind(space).execute(&db).await.unwrap();
        for (actor,role) in [("owner","owner"),("editor","editor"),("viewer","viewer")] {
            sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,$2,$3,$4)").bind(format!("deep-{space}-{actor}")).bind(space).bind(actor).bind(role).execute(&db).await.unwrap();
        }
    }
    let s=crate::AppState{db,jwt_secret:JWT.into(),base_url:"http://deep.invalid".into(),search_index:crate::search::SearchIndex::new(root.join("search-index")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};
    (s,schema)
}
fn page(space:&str,id:&str)->String{format!("/spaces/{space}/documents/{id}")}
async fn create(s:&crate::AppState,id:&str,parent:Option<&str>)->String {create_in(s,SPACE,id,parent).await}
async fn create_in(s:&crate::AppState,space:&str,id:&str,parent:Option<&str>)->String {
 let p=call(s,"owner","POST",&format!("/spaces/{space}/documents"),json!({"source_id":id,"title":id,"parent_id":parent,"content":format!("Owned exact body {id}\r\nΩ")} ),StatusCode::CREATED).await;
 assert_eq!(p["id"],id);p["draft_id"].as_str().unwrap().into()
}
async fn move_page(s:&crate::AppState,id:&str,parent:Option<&str>){call(s,"owner","POST",&format!("{}/move",page(SPACE,id)),json!({"target_space_id":SPACE,"parent_id":parent,"folder_id":null}),StatusCode::NO_CONTENT).await;}
fn replay(id:&str,parent:&str)->Value {
 json!({"schema_version":1,"provider":"self-hosted","operation_count":1,"recovery_checkpoint_count":0,"recovery_checkpoints":[],"replay_contract":{"schema_version":1,"operation_order":"queued_at_ascending","idempotency_key":"operation.id","transport":"server-api","server_backends":["sqlite","postgresql"],"required_payload_status":"available","conflict_policy":"server_validates_version_and_permissions","web_canonical_store":"server-api"},"operations":[{"id":format!("deep-page-replay-{id}"),"entity_type":"document","entity_id":id,"action":"create","queued_at":"2026-10-15T12:00:00Z","payload_status":"available","payload":{"id":id,"space_id":SPACE,"title":id,"content":"Replay original body Ω\r\n","parent_id":parent,"folder_id":null,"created_at":"2026-10-15T12:00:00Z","updated_at":"2026-10-15T12:00:00Z"}}]})
}
async fn denied_unchanged(s:&crate::AppState,actor:&str,method:&str,url:&str,body:Value){let prior=domain(s).await;let(code,result)=request(s,actor,method,url,body).await;assert!(code.is_client_error(),"unexpected {code}: {result}");assert_eq!(domain(s).await,prior,"denied operation changed retained domain");}
async fn stale_draft_unchanged(s:&crate::AppState,url:&str,body:Value) {
 let mut before=domain(s).await;
 let prior_audit=before.as_object_mut().unwrap().remove("audit_log").unwrap();
 call(s,"owner","PUT",url,body,StatusCode::CONFLICT).await;
 let mut after=domain(s).await;
 let current_audit=after.as_object_mut().unwrap().remove("audit_log").unwrap();
 assert_eq!(after,before,"stale draft CAS changed canonical rows");
 let previous=prior_audit.as_array().unwrap();let current=current_audit.as_array().unwrap();
 assert!(previous.iter().all(|row|current.contains(row)),"stale refusal removed an existing audit");
 let added=current.iter().filter(|row|!previous.contains(row)).collect::<Vec<_>>();
 assert_eq!(added.len(),1,"stale refusal must retain exactly one failure audit");
 assert_eq!(added[0]["action"],"document.draft_update_failed");
}
async fn matrix(postgres:bool){
 let(s,schema)=setup(postgres).await;
 let mut last_draft=String::new();
 for depth in 1..=256 {
  let id=format!("deep-{depth}");let parent=(depth>1).then(||format!("deep-{}",depth-1));
  let draft=create(&s,&id,parent.as_deref()).await;last_draft=draft.clone();
  if [4,32,256].contains(&depth) {
   let current=call(&s,"owner","GET",&page(SPACE,&id),Value::Null,StatusCode::OK).await;assert_eq!(current["parent_id"],json!(parent));
   let base=format!("{}/drafts/{draft}",page(SPACE,&id));let source=call(&s,"owner","GET",&format!("{base}/save-source"),Value::Null,StatusCode::OK).await;
   let save=json!({"source_digest":source["draft_source_digest"],"title":format!("Renamed {depth}"),"content":format!("Saved exact body {depth}\r\nΩ")});
   call(&s,"owner","PUT",&base,save.clone(),StatusCode::OK).await;
   stale_draft_unchanged(&s,&base,save).await;
   let copy=call(&s,"owner","POST",&format!("{}/duplicate",page(SPACE,&id)),json!({"draft_id":draft,"title":format!("Copy {depth}")}),StatusCode::CREATED).await;
   let copied=copy["id"].as_str().unwrap();let shown=call(&s,"owner","GET",&page(SPACE,copied),Value::Null,StatusCode::OK).await;assert_eq!(shown["parent_id"],json!(parent));
   move_page(&s,copied,Some(&id)).await;assert_eq!(call(&s,"owner","GET",&page(SPACE,copied),Value::Null,StatusCode::OK).await["parent_id"],id);
   println!("DEEP_PAGE_ACTUAL_ROUTE postgres={postgres} depth={depth} get/create/draft_rename/stale_CAS/duplicate/reparent PASS");
  }
 }
 let catalog=call(&s,"owner","GET",&format!("/spaces/{SPACE}/documents"),Value::Null,StatusCode::OK).await;assert_eq!(catalog.as_array().unwrap().len(),259);
 let imported=replay("deep-replayed","deep-256");let first=call(&s,"owner","POST","/sync/replay",imported.clone(),StatusCode::OK).await;assert_eq!(first["applied_operation_count"],1,"{first}");
 let prior=domain(&s).await;let retry=call(&s,"owner","POST","/sync/replay",imported,StatusCode::OK).await;assert_eq!(retry["skipped_operation_count"],1,"{retry}");assert_eq!(domain(&s).await,prior);
 // Actual publication/history retry at depth256, without producing fake receipts.
 let base=format!("{}/drafts/{last_draft}",page(SPACE,"deep-256"));let src=call(&s,"owner","GET",&format!("{base}/publish-source"),Value::Null,StatusCode::OK).await;
 let intent=json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"draft_source_digest":src["draft_source_digest"],"page_source_digest":src["page_source_digest"]});
 call(&s,"owner","POST",&format!("{base}/publish"),intent.clone(),StatusCode::OK).await;let prior=domain(&s).await;
 call(&s,"owner","POST",&format!("{base}/publish"),intent,StatusCode::OK).await;assert_eq!(domain(&s).await,prior);
 let history=call(&s,"owner","GET",&format!("{}/publication-snapshots",page(SPACE,"deep-256")),Value::Null,StatusCode::OK).await;assert_eq!(history["snapshots"].as_array().unwrap().len(),1);
 // Cycle, missing, foreign and revoked original credentials leave retained rows exact.
 for parent in ["deep-256","missing"] {denied_unchanged(&s,"owner","POST",&format!("{}/move",page(SPACE,"deep-1")),json!({"target_space_id":SPACE,"parent_id":parent})).await;}
 call(&s,"owner","POST",&format!("/spaces/{TARGET}/documents"),json!({"source_id":"foreign","title":"Foreign owned","content":"Foreign body"}),StatusCode::CREATED).await;
 denied_unchanged(&s,"owner","POST",&format!("/spaces/{SPACE}/documents"),json!({"title":"Foreign child","parent_id":"foreign"})).await;
 denied_unchanged(&s,"editor","POST",&format!("/spaces/{SPACE}/documents"),json!({"title":"Private child","parent_id":"deep-32"})).await;
 sqlx::query("UPDATE space_members SET role='viewer' WHERE space_id=$1 AND user_id='owner'").bind(SPACE).execute(&s.db).await.unwrap();
 denied_unchanged(&s,"owner","POST",&format!("/spaces/{SPACE}/documents"),json!({"title":"Revoked child","parent_id":"deep-256"})).await;
 sqlx::query("UPDATE space_members SET role='owner' WHERE space_id=$1 AND user_id='owner'").bind(SPACE).execute(&s.db).await.unwrap();
 sqlx::query("UPDATE documents SET locked=1,locked_by='editor' WHERE id='deep-32'").execute(&s.db).await.unwrap();
 denied_unchanged(&s,"owner","POST",&format!("/spaces/{SPACE}/documents"),json!({"title":"Locked ancestor child","parent_id":"deep-256"})).await;
 sqlx::query("UPDATE documents SET locked=0,locked_by=NULL WHERE id='deep-32'").execute(&s.db).await.unwrap();
 // Final-write revocation is induced inside the actual placement transaction.
 if postgres {sqlx::query("CREATE FUNCTION deep_page_late() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE space_members SET role='viewer' WHERE user_id='owner'; RETURN NEW; END $$").execute(&s.db).await.unwrap();sqlx::query("CREATE TRIGGER deep_page_late AFTER UPDATE OF parent_id ON documents FOR EACH ROW EXECUTE FUNCTION deep_page_late()").execute(&s.db).await.unwrap();}
 else {sqlx::query("CREATE TRIGGER deep_page_late AFTER UPDATE OF parent_id ON documents BEGIN UPDATE space_members SET role='viewer' WHERE user_id='owner'; END").execute(&s.db).await.unwrap();}
 denied_unchanged(&s,"owner","POST",&format!("{}/move",page(SPACE,"deep-replayed")),json!({"target_space_id":SPACE,"parent_id":"deep-4"})).await;
 sqlx::query(if postgres{"DROP TRIGGER deep_page_late ON documents"}else{"DROP TRIGGER deep_page_late"}).execute(&s.db).await.unwrap();
 // Real broad subtree, reviewed move under a deep destination, exact retry.
 let broad_draft=create_in(&s,TARGET,"broad-root",None).await;for n in 0..32{create_in(&s,TARGET,&format!("broad-{n}"),Some("broad-root")).await;}
 let p=call(&s,"owner","POST",&format!("/spaces/{TARGET}/moves/preview"),json!({"root":{"kind":"document","id":"broad-root"},"destination":{"space_id":SPACE,"folder_id":null,"parent_id":"deep-256"}}),StatusCode::OK).await;
 assert_eq!(p["blockers"],json!([]),"{p}");assert_eq!(p["commit_available"],true,"{p}");
 let stale_commit=json!({"fingerprint":p["fingerprint"],"decisions":[],"public_links":"preserve","confirm_audience_change":true,"confirm_draft_space_change":true});
 let broad_base=format!("{}/drafts/{broad_draft}",page(TARGET,"broad-root"));
 let source=call(&s,"owner","GET",&format!("{broad_base}/save-source"),Value::Null,StatusCode::OK).await;
 call(&s,"owner","PUT",&broad_base,json!({"source_digest":source["draft_source_digest"],"content":"Changed after exact reviewed preview"}),StatusCode::OK).await;
 denied_unchanged(&s,"owner","POST",&format!("/moves/previews/{}/commit",p["preview_id"].as_str().unwrap()),stale_commit).await;
 let p=call(&s,"owner","POST",&format!("/spaces/{TARGET}/moves/preview"),json!({"root":{"kind":"document","id":"broad-root"},"destination":{"space_id":SPACE,"folder_id":null,"parent_id":"deep-256"}}),StatusCode::OK).await;
 assert_eq!(p["blockers"],json!([]),"{p}");assert_eq!(p["commit_available"],true,"{p}");
 let commit=json!({"fingerprint":p["fingerprint"],"decisions":[],"public_links":"preserve","confirm_audience_change":true,"confirm_draft_space_change":true});let url=format!("/moves/previews/{}/commit",p["preview_id"].as_str().unwrap());
 let result=call(&s,"owner","POST",&url,commit.clone(),StatusCode::OK).await;assert_eq!(result["committed"],true);let prior=domain(&s).await;call(&s,"owner","POST",&url,commit,StatusCode::OK).await;assert_eq!(domain(&s).await,prior);
 assert_eq!(call(&s,"owner","GET",&page(SPACE,"broad-root"),Value::Null,StatusCode::OK).await["parent_id"],"deep-256");
 // Synthetic topology-only broad over-budget input. The refused writer is real.
 create(&s,"budget-root",None).await;let mut tx=s.db.begin().await.unwrap();
 for chunk in 0..100 {let values=(0..100).map(|n|format!("(${},$1,$2,${},'','owner','owner')",n+3,n+3)).collect::<Vec<_>>().join(",");let sql=format!("INSERT INTO documents(id,space_id,parent_id,title,content_path,created_by,last_updated_by) VALUES {values}");let mut query=sqlx::query(&sql).bind(SPACE).bind("budget-root");for n in 0..100{query=query.bind(format!("budget-{}",chunk*100+n));}query.execute(&mut *tx).await.unwrap();}tx.commit().await.unwrap();
 let before=domain(&s).await;let(code,error)=request(&s,"owner","POST",&format!("{}/move",page(SPACE,"budget-root")),json!({"target_space_id":SPACE,"parent_id":"deep-4"})).await;assert_eq!(code,StatusCode::BAD_REQUEST,"{error}");assert!(error.to_string().contains("10000-node"),"{error}");assert_eq!(domain(&s).await,before);
 println!("DEEP_PAGE_MATRIX_PASS postgres={postgres} depth=4/32/256 broad=32 replay/exactretry draft_CAS duplicate/reparent publish/history/retry reviewed_stalesource/commit/retry cycle/foreign/missing/private/revoked/lock/finalwrite/budget exact_rollback schema={schema}");s.db.close().await;
}
#[tokio::test]
#[ignore="requires owned STARLANE_CORE_WORKSPACE and fresh DATA_DIR; run alone"]
async fn required_sqlite_deep_page_runtime_matrix(){matrix(false).await}
#[tokio::test]
#[ignore="requires owned STARLANE_CORE_WORKSPACE, STARLANE_CORE_PG_URL and fresh DATA_DIR; run alone"]
async fn required_postgres_deep_page_runtime_matrix(){matrix(true).await}
