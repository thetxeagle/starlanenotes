//! Disposable actual-route fixture. Required selectors fail closed; child is ignored.
use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
use serde_json::{json,Value};
use sqlx::{AnyPool,Connection};
use std::{path::{Path,PathBuf},collections::BTreeMap,io::Read};
use sha2::{Digest,Sha256};
use tower::ServiceExt;
const JWT:&str="private-disposable-pg-runtime-fixture-secret";
const SPACE:&str="pg-runtime-space";
const PAGE:&str="pg-runtime-page";
const BODY:&str="PG_RUNTIME_EXACT_BODY\nUnicode Ω café\n[stable reference](starlane://pg-runtime-page)\n";
fn workspace()->PathBuf {PathBuf::from(std::env::var("STARLANE_PG_RUNTIME_WORKSPACE").expect("required disposable workspace"))}
async fn pool(url:&str,schema:&str)->AnyPool {
 sqlx::any::install_default_drivers();let engine=crate::db::DatabaseEngine::from_url(url).unwrap();
 let schema=schema.to_owned();
 sqlx::any::AnyPoolOptions::new().max_connections(2).after_connect(move|c,_|{let schema=schema.clone();Box::pin(async move{
  match engine {crate::db::DatabaseEngine::Postgres=>{sqlx::query(&format!("SET search_path TO {schema}")).execute(c).await?;},crate::db::DatabaseEngine::Sqlite=>{sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *c).await?;sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;}} Ok(())
 })}).connect(url).await.unwrap()
}
fn state(db:AnyPool,root:&Path)->crate::AppState {crate::AppState {db,jwt_secret:JWT.into(),base_url:"http://owned.invalid".into(),search_index:crate::search::SearchIndex::new(root.join("search_index")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()}}
fn bearer()->String {crate::auth::jwt::create_token_pair_for_session("runtime-owner","runtime-owner@fixture.invalid","admin",JWT,Some("runtime-session"),30).unwrap().access_token}
async fn call_token(s:&crate::AppState,method:&str,uri:&str,v:Value,token:&str)->(StatusCode,Value){
 let r=Request::builder().method(method).uri(uri).header("Content-Type","application/json").header("Authorization",format!("Bearer {token}")).body(Body::from(serde_json::to_vec(&v).unwrap())).unwrap();
 let r=crate::api_routes().with_state(s.clone()).oneshot(r).await.unwrap();let code=r.status();let bytes=to_bytes(r.into_body(),8*1024*1024).await.unwrap();let v=serde_json::from_slice(&bytes).unwrap_or_else(|_|json!({"raw":String::from_utf8_lossy(&bytes)}));(code,v)
}
async fn call(s:&crate::AppState,method:&str,uri:&str,v:Value)->(StatusCode,Value){call_token(s,method,uri,v,&bearer()).await}
async fn ok(s:&crate::AppState,method:&str,uri:&str,v:Value)->Value {let(c,v)=call(s,method,uri,v).await;assert!(c.is_success(),"{method} {uri}: {c} {v}");v}
async fn recover(db:&AnyPool,root:&Path) {crate::storage::restoration::bootstrap_gate(root).unwrap();crate::storage::restoration::activate_in(root,db).await.unwrap();crate::storage::relocation::recover(db).await.unwrap();crate::storage::journal::recover(db).await.unwrap();crate::storage::canonical_rewrite::recover(db).await.unwrap();crate::storage::publication::recover(db).await.unwrap();}
fn essential_bytes(root:&Path)->BTreeMap<String,String>{
 fn walk(root:&Path,path:&Path,out:&mut BTreeMap<String,String>){
  let meta=std::fs::symlink_metadata(path).unwrap();assert!(!meta.file_type().is_symlink());
  if meta.is_dir(){for entry in std::fs::read_dir(path).unwrap(){walk(root,&entry.unwrap().path(),out);}}
  else {assert!(meta.is_file());let mut file=std::fs::File::open(path).unwrap();let mut h=Sha256::new();let mut buf=[0u8;65536];loop{let n=file.read(&mut buf).unwrap();if n==0{break;}h.update(&buf[..n]);}out.insert(path.strip_prefix(root).unwrap().to_string_lossy().into_owned(),hex::encode(h.finalize()));}
 }
 let mut out=BTreeMap::new();for name in ["spaces",".storage-journal",".publication-journal",".canonical-rewrite-journal",".move-relocation-journal"]{let path=root.join(name);if path.exists(){walk(root,&path,&mut out);}}out
}
async fn essential_sql(db:&AnyPool)->BTreeMap<String,Vec<Value>>{
 use sqlx::{Column,Row};let mut out=BTreeMap::new();
 for table in ["documents","document_drafts","storage_journal_receipts","publication_receipts"]{
  let mut rows=Vec::new();for row in sqlx::query(&format!("SELECT * FROM {table}")).fetch_all(db).await.unwrap(){
   let mut cells=BTreeMap::new();for col in row.columns(){let name=col.name();let v=if let Ok(v)=row.try_get::<Option<String>,_>(name){v.map(|v|json!(["text",v])).unwrap_or(json!(["null"]))}else if let Ok(v)=row.try_get::<Option<i64>,_>(name){v.map(|v|json!(["integer",v.to_string()])).unwrap_or(json!(["null"]))}else if let Ok(v)=row.try_get::<Option<Vec<u8>>,_>(name){v.map(|v|json!(["blob",hex::encode(v)])).unwrap_or(json!(["null"]))}else{panic!("unsupported fixture SQL cell {table}.{name}");};cells.insert(name.to_owned(),v);}
   rows.push(serde_json::to_value(cells).unwrap());
  }rows.sort_by_cached_key(Value::to_string);out.insert(table.to_owned(),rows);
 }out
}
async fn current_denials(url:&str,schema:&str,root:&Path){
 let s=state(pool(url,schema).await,root);let bytes=essential_bytes(root);
 let logical=essential_sql(&s.db).await;
 async fn denied(s:&crate::AppState,token:&str,root:&Path,bytes:&BTreeMap<String,String>,logical:&BTreeMap<String,Vec<Value>>){
  let(code,body)=call_token(s,"GET","/auth/me",Value::Null,token).await;assert_eq!(code,StatusCode::UNAUTHORIZED,"{body}");assert!(!body.to_string().contains(BODY));assert_eq!(essential_bytes(root),*bytes);
  assert_eq!(essential_sql(&s.db).await,*logical);
 }
 sqlx::query("UPDATE sessions SET expires_at='2000-01-01T00:00:00Z' WHERE id='runtime-session'").execute(&s.db).await.unwrap();denied(&s,&bearer(),root,&bytes,&logical).await;
 sqlx::query("UPDATE sessions SET expires_at='2999-01-01T00:00:00Z' WHERE id='runtime-session'").execute(&s.db).await.unwrap();
 sqlx::query("UPDATE users SET is_active=0 WHERE id='runtime-owner'").execute(&s.db).await.unwrap();denied(&s,&bearer(),root,&bytes,&logical).await;
 sqlx::query("UPDATE users SET is_active=1 WHERE id='runtime-owner'").execute(&s.db).await.unwrap();
 sqlx::query("INSERT INTO users(id,email,display_name,role,account_kind) VALUES('runtime-other','runtime-other@fixture.invalid','Other user','member','member')").execute(&s.db).await.unwrap();
 let mismatch=crate::auth::jwt::create_token_pair_for_session("runtime-other","runtime-other@fixture.invalid","member",JWT,Some("runtime-session"),30).unwrap().access_token;denied(&s,&mismatch,root,&bytes,&logical).await;
 sqlx::query("DELETE FROM sessions WHERE id='runtime-session'").execute(&s.db).await.unwrap();denied(&s,&bearer(),root,&bytes,&logical).await;
 s.db.close().await;drop(s);println!("CURRENT_SESSION_DENIALS_PASS expired/inactive/sid-sub-mismatch/revoked exact401; essential bytes and journal SQL unchanged");
}
async fn workflow(url:&str,schema:&str,root:&Path){
 let db=pool(url,schema).await;crate::db::migrate_selected(&db,crate::db::DatabaseEngine::from_url(url).unwrap()).await.unwrap();
 sqlx::query("INSERT INTO users(id,email,display_name,role,account_kind) VALUES('runtime-owner','runtime-owner@fixture.invalid','Runtime owner','admin','member')").execute(&db).await.unwrap();
 sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,session_kind) VALUES('runtime-session','runtime-owner','not-a-credential','2999-01-01T00:00:00Z','browser')").execute(&db).await.unwrap();
 sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,'Runtime fixture','runtime-fixture','private','runtime-owner')").bind(SPACE).execute(&db).await.unwrap();
 sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('runtime-membership',$1,'runtime-owner','owner')").bind(SPACE).execute(&db).await.unwrap();
 let s=state(db,root);let me=ok(&s,"GET","/auth/me",Value::Null).await;assert_eq!(me["id"],"runtime-owner");
 let ai_config=ok(&s,"GET","/ai/config",Value::Null).await;assert_eq!(ai_config["enabled"],false);
 let providers=ok(&s,"GET","/ai/providers",Value::Null).await;assert_eq!(providers.as_array().unwrap().len(),4);
 ok(&s,"PUT","/ai/providers/ollama",json!({"base_url":"http://ollama.invalid:11434","model":"fixture-model"})).await;
 ok(&s,"PUT","/ai/settings",json!({"enabled":true,"default_provider":"ollama"})).await;
 let configured=ok(&s,"GET","/ai/config",Value::Null).await;assert_eq!(configured["enabled"],true);assert_eq!(configured["default_provider"],"ollama");
 ok(&s,"DELETE","/ai/providers/ollama",Value::Null).await;
 println!("AI_CONFIG_PROVIDER_SQL_PASS");
 let prefix=format!("/spaces/{SPACE}/documents");let page=ok(&s,"POST",&prefix,json!({"source_id":PAGE,"title":"Runtime page","content":"INITIAL_PRIVATE_NATIVE_BODY","property_values":[]})).await;
 assert_eq!(page["id"],PAGE);let draft_id=page["draft_id"].as_str().unwrap();let uri=format!("{prefix}/{PAGE}/drafts/{draft_id}");
 let draft=ok(&s,"GET",&uri,Value::Null).await;assert_eq!(draft["id"],draft_id);let original=draft["source_digest"].as_str().unwrap().to_string();assert_eq!(original.len(),64);
 ok(&s,"PUT",&uri,json!({"source_digest":original,"content":BODY})).await;
 let saved=ok(&s,"GET",&uri,Value::Null).await;assert_eq!(saved["content"],BODY);assert_ne!(saved["source_digest"],original);
 let(code,_)=call(&s,"PUT",&uri,json!({"source_digest":original,"content":"STALE_REJECTED_BODY"})).await;assert_eq!(code,StatusCode::CONFLICT);
 assert_eq!(ok(&s,"GET",&uri,Value::Null).await["content"],BODY);
 ok(&s,"POST",&format!("{uri}/publish"),json!({})).await;let published=ok(&s,"GET",&format!("{prefix}/{PAGE}/content"),Value::Null).await;assert_eq!(published["id"],PAGE);assert!(published["content"].as_str().unwrap().contains(BODY));
 let receipts:i64=sqlx::query_scalar("SELECT count(*) FROM storage_journal_receipts WHERE state='complete'").fetch_one(&s.db).await.unwrap();assert_eq!(receipts,1);
 let publications:i64=sqlx::query_scalar("SELECT count(*) FROM publication_receipts").fetch_one(&s.db).await.unwrap();assert_eq!(publications,1);
 std::fs::write(workspace().join("expected.json"),serde_json::to_vec(&json!({"published":published,"draft_id":draft_id})).unwrap()).unwrap();s.db.close().await;drop(s);
 let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["postgres_runtime_fixture::fresh_restart_child","--ignored","--exact","--nocapture"]).env("STARLANE_PG_RUNTIME_CHILD_URL",url).env("STARLANE_PG_RUNTIME_SCHEMA",schema).env("DATA_DIR",root).output().unwrap();
 assert!(output.status.success(),"fresh-process actual recovery failed:\n{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));println!("FRESH_RESTART_CHILD_PASS {}",String::from_utf8_lossy(&output.stdout));
 current_denials(url,schema,root).await;
}
#[tokio::test]
#[ignore="requires explicit owned disposable PostgreSQL orchestration"]
async fn required_postgres_current_jwt_native_page_save_publish_restart(){
 let url=std::env::var("STARLANE_PG_RUNTIME_URL").expect("required owned disposable PostgreSQL fixture URL");assert!(url.starts_with("postgres://"));
 let root=crate::storage::data_dir();assert!(root.starts_with(workspace()));std::fs::create_dir_all(&root).unwrap();
 sqlx::any::install_default_drivers();let schema=format!("runtime_{}",uuid::Uuid::new_v4().simple());let mut c=sqlx::AnyConnection::connect(&url).await.unwrap();sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&mut c).await.unwrap();c.close().await.unwrap();
 workflow(&url,&schema,&root).await;
}
#[tokio::test]
#[ignore="requires explicit owned disposable workspace orchestration"]
async fn required_sqlite_current_jwt_native_page_save_publish_restart(){
 let root=crate::storage::data_dir();assert!(root.starts_with(workspace()));std::fs::create_dir_all(&root).unwrap();workflow(&format!("sqlite://{}?mode=rwc",root.join("starlane-notes.db").display()),"",&root).await;
}
#[tokio::test]
#[ignore="only required parent fixture launches fresh process with genuine database/journals"]
async fn fresh_restart_child(){
 let url=std::env::var("STARLANE_PG_RUNTIME_CHILD_URL").expect("required actual parent fixture URL");let schema=std::env::var("STARLANE_PG_RUNTIME_SCHEMA").expect("required actual parent schema");let root=crate::storage::data_dir();assert!(root.starts_with(workspace()));let db=pool(&url,&schema).await;
 crate::db::migrate_selected(&db,crate::db::DatabaseEngine::from_url(&url).unwrap()).await.unwrap();recover(&db,&root).await;let s=state(db,&root);
 let expected:Value=serde_json::from_slice(&std::fs::read(workspace().join("expected.json")).unwrap()).unwrap();let content=ok(&s,"GET",&format!("/spaces/{SPACE}/documents/{PAGE}/content"),Value::Null).await;assert_eq!(content,expected["published"]);
 let remaining:i64=sqlx::query_scalar("SELECT count(*) FROM document_drafts WHERE id=$1").bind(expected["draft_id"].as_str().unwrap()).fetch_one(&s.db).await.unwrap();assert_eq!(remaining,0);
 let completed:i64=sqlx::query_scalar("SELECT count(*) FROM publication_receipts WHERE verified_at IS NOT NULL").fetch_one(&s.db).await.unwrap();assert_eq!(completed,1);s.db.close().await;
}
