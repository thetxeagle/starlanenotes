//! Required real first-user/native/config producers on both migrated engines.
use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
use serde_json::{json,Value};use sqlx::{AnyPool,Connection,Column,Row};use tower::ServiceExt;
use std::{path::{Path,PathBuf},collections::BTreeMap,io::Read};use sha2::{Digest,Sha256};
const JWT:&str="owned-password-fixture-secret";const PASSWORD:&str="owned-fixture-password-not-a-real-account";
fn workspace()->PathBuf{PathBuf::from(std::env::var("STARLANE_ADMIN_RESET_WORKSPACE").expect("required owned cryptographic passkey workspace"))}
async fn pool(url:&str,schema:&str)->AnyPool{sqlx::any::install_default_drivers();let engine=crate::db::DatabaseEngine::from_url(url).unwrap();let schema=schema.to_owned();sqlx::any::AnyPoolOptions::new().max_connections(3).after_connect(move|c,_|{let schema=schema.clone();Box::pin(async move{match engine{crate::db::DatabaseEngine::Postgres=>{sqlx::query(&format!("SET search_path TO {schema}")).execute(c).await?;},crate::db::DatabaseEngine::Sqlite=>{sqlx::query("PRAGMA foreign_keys=ON").execute(&mut*c).await?;sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;}}Ok(())})}).connect(url).await.unwrap()}
fn state(db:AnyPool,root:&Path)->crate::AppState{crate::AppState{db,jwt_secret:JWT.into(),base_url:"http://reset.owned.invalid".into(),search_index:crate::search::SearchIndex::new(root.join("search_index")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()}}

async fn call(s:&crate::AppState,token:Option<&str>,method:&str,uri:&str,body:Value)->(StatusCode,Value){println!("ADMIN_RESET_STEP {method} {uri}");let mut q=Request::builder().method(method).uri(uri).header("Content-Type","application/json");if let Some(t)=token{q=q.header("Authorization",format!("Bearer {t}"));}let mut req=q.body(Body::from(body.to_string())).unwrap();req.extensions_mut().insert(axum::extract::ConnectInfo("127.0.0.1:43123".parse::<std::net::SocketAddr>().unwrap()));let r=crate::api_routes().with_state(s.clone()).oneshot(req).await.unwrap();let code=r.status();let bytes=to_bytes(r.into_body(),4*1024*1024).await.unwrap();(code,if bytes.is_empty(){Value::Null}else{serde_json::from_slice(&bytes).unwrap_or(json!({"non_json":true}))})}
async fn ok(s:&crate::AppState,token:Option<&str>,method:&str,uri:&str,body:Value)->Value{let(code,v)=call(s,token,method,uri,body).await;assert!(code.is_success(),"actual {method} {uri} status={code}; response suppressed to protect credentials");v}
fn access(v:&Value)->&str{v["access_token"].as_str().unwrap()}fn refresh(v:&Value)->&str{v["refresh_token"].as_str().unwrap()}fn sid(v:&Value)->String{crate::auth::jwt::validate_token(access(v),JWT,crate::auth::jwt::TokenType::Access).unwrap().sid}
async fn snapshot(db:&AnyPool,tables:&[&str])->BTreeMap<String,Vec<Value>>{let mut out=BTreeMap::new();for table in tables{let mut values=Vec::new();for row in sqlx::query(&format!("SELECT * FROM {table}")).fetch_all(db).await.unwrap(){let mut cells=BTreeMap::new();for col in row.columns(){let n=col.name();let v=if let Ok(v)=row.try_get::<Option<String>,_>(n){v.map(|v|json!(["text",v])).unwrap_or(json!(["null"]))}else if let Ok(v)=row.try_get::<Option<i64>,_>(n){v.map(|v|json!(["integer",v.to_string()])).unwrap_or(json!(["null"]))}else if let Ok(v)=row.try_get::<Option<Vec<u8>>,_>(n){v.map(|v|json!(["blob",hex::encode(v)])).unwrap_or(json!(["null"]))}else{panic!("unsupported cell {table}.{n}")};cells.insert(n.to_string(),v);}values.push(serde_json::to_value(cells).unwrap());}values.sort_by_cached_key(Value::to_string);out.insert(table.to_string(),values);}out}
fn login_body(email:&str,password:&str)->Value{json!({"email":email,"password":password,"device_name":"Owned browser","session_kind":"browser","client_platform":"web"})}
fn bytes(root:&Path)->BTreeMap<String,String>{fn walk(root:&Path,p:&Path,out:&mut BTreeMap<String,String>){let m=std::fs::symlink_metadata(p).unwrap();assert!(!m.file_type().is_symlink());if m.is_dir(){for e in std::fs::read_dir(p).unwrap(){walk(root,&e.unwrap().path(),out);}}else{assert!(m.is_file());let mut f=std::fs::File::open(p).unwrap();let mut h=Sha256::new();let mut b=[0u8;65536];loop{let n=f.read(&mut b).unwrap();if n==0{break}h.update(&b[..n]);}out.insert(p.strip_prefix(root).unwrap().to_string_lossy().into(),hex::encode(h.finalize()));}}let mut out=BTreeMap::new();for n in["spaces",".storage-journal",".publication-journal",".canonical-rewrite-journal",".move-relocation-journal"]{let p=root.join(n);if p.exists(){walk(root,&p,&mut out);}}out}





const EMAIL:&str="crypto-owner@owned.invalid";
fn session_touch(before:&BTreeMap<String,Vec<Value>>,after:&BTreeMap<String,Vec<Value>>,sid:&str,start:chrono::DateTime<chrono::Utc>,end:chrono::DateTime<chrono::Utc>){
 assert_eq!(before.keys().collect::<Vec<_>>(),after.keys().collect::<Vec<_>>());
 let mut old=before.clone();let mut new=after.clone();old.get_mut("sessions").unwrap().sort_by_cached_key(|r|r["id"].to_string());new.get_mut("sessions").unwrap().sort_by_cached_key(|r|r["id"].to_string());
 let mut found=0;
 for(row_before,row_after)in old.get_mut("sessions").unwrap().iter_mut().zip(new.get_mut("sessions").unwrap().iter_mut()){
  if row_before["id"][1]==sid {assert_eq!(row_after["id"][1],sid);found+=1;
   let last=chrono::DateTime::parse_from_rfc3339(row_after["last_seen_at"][1].as_str().unwrap()).unwrap();let expires=chrono::DateTime::parse_from_rfc3339(row_after["expires_at"][1].as_str().unwrap()).unwrap();
   assert!(last>=start&&last<=end);assert!(expires>=start+chrono::Duration::days(30)&&expires<=end+chrono::Duration::days(30));
   row_before.as_object_mut().unwrap().remove("last_seen_at");row_before.as_object_mut().unwrap().remove("expires_at");row_after.as_object_mut().unwrap().remove("last_seen_at");row_after.as_object_mut().unwrap().remove("expires_at");
  }
 }
 assert_eq!(found,1);assert!(old==new,"only the actual current JWT session middleware timestamp touch is permitted");
 println!("ATOMIC_EXACT_CURRENT_SESSION_TOUCH_AUDITED");
}

async fn run(engine:&str){
 let case=std::env::var("STARLANE_ADMIN_RESET_CASE").expect("required reset case");assert!(matches!(case.as_str(),"member"|"self"|"delete-failure"|"readback-drift"|"stale-admin"));
 let work=workspace();let root=crate::storage::data_dir();assert!(root.starts_with(&work));
 let url=if engine=="postgres"{std::env::var("STARLANE_ADMIN_RESET_PG_URL").expect("required private PostgreSQL URL")}else{format!("sqlite://{}?mode=rwc",root.join("starlane-notes.db").display())};std::fs::create_dir_all(&root).unwrap();
 let schema=if engine=="postgres"{sqlx::any::install_default_drivers();let n=format!("reset_{}",uuid::Uuid::new_v4().simple());let mut c=sqlx::AnyConnection::connect(&url).await.unwrap();sqlx::query(&format!("CREATE SCHEMA {n}")).execute(&mut c).await.unwrap();c.close().await.unwrap();n}else{String::new()};std::fs::write(work.join("schema.txt"),&schema).unwrap();
 let db=pool(&url,&schema).await;crate::db::migrate_selected(&db,crate::db::DatabaseEngine::from_url(&url).unwrap()).await.unwrap();let s=state(db,&root);
 let admin=ok(&s,None,"POST","/auth/register",json!({"email":"admin@reset.invalid","display_name":"Reset admin","password":PASSWORD})).await;
 let born=ok(&s,Some(access(&admin)),"POST","/admin/users",json!({"email":"member@reset.invalid","display_name":"Reset member","role":"member"})).await;
 let member=ok(&s,None,"POST","/auth/login",login_body("member@reset.invalid",born["temporary_password"].as_str().unwrap())).await;
 let old=if case=="self"{admin.clone()}else{member.clone()};let uid=old["user"]["id"].as_str().unwrap();let email=old["user"]["email"].as_str().unwrap();
 let space:String=sqlx::query_scalar("SELECT id FROM spaces WHERE created_by=$1").bind(uid).fetch_one(&s.db).await.unwrap();
 let page=ok(&s,Some(access(&old)),"POST",&format!("/spaces/{space}/documents"),json!({"source_id":"reset-page","title":"Reset page","content":"EXACT_RESET_PAGE Ω","property_values":[]})).await;
 ok(&s,Some(access(&old)),"POST",&format!("/spaces/{space}/documents/reset-page/drafts/{}/publish",page["draft_id"].as_str().unwrap()),json!({})).await;
 let files=bytes(&root);let path=format!("/admin/users/{uid}/password-reset");let mut caller=admin.clone();
 if case=="delete-failure"||case=="readback-drift"{
  assert_eq!(uuid::Uuid::parse_str(uid).unwrap().to_string(),uid);
  let (table,event,body)=if case=="delete-failure"{("sessions","BEFORE DELETE",if engine=="postgres"{"RAISE EXCEPTION 'owned deletion failure';".to_string()}else{"SELECT RAISE(ABORT,'owned deletion failure');".to_string()})}else{("users","AFTER UPDATE OF password_hash",format!("UPDATE users SET display_name='unexpected drift' WHERE id='{uid}';"))};
  if engine=="postgres"{sqlx::query(&format!("CREATE FUNCTION owned_reset_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN {body} RETURN NEW; END $$")).execute(&s.db).await.unwrap();sqlx::query(&format!("CREATE TRIGGER owned_reset_fault {event} ON {table} FOR EACH ROW EXECUTE FUNCTION owned_reset_fault()")).execute(&s.db).await.unwrap();}
  else{let pred=if table=="sessions"{format!("OLD.user_id='{uid}'")}else{format!("NEW.id='{uid}'")};sqlx::query(&format!("CREATE TRIGGER owned_reset_fault {event} ON {table} WHEN {pred} BEGIN {body} END")).execute(&s.db).await.unwrap();}
  let before=snapshot(&s.db,&["users","sessions"]).await;let start=chrono::Utc::now();let(code,_)=call(&s,Some(access(&caller)),"POST",&path,Value::Null).await;assert_eq!(code,if case=="delete-failure"{StatusCode::INTERNAL_SERVER_ERROR}else{StatusCode::CONFLICT});session_touch(&before,&snapshot(&s.db,&["users","sessions"]).await,&sid(&caller),start,chrono::Utc::now());assert_eq!(files,bytes(&root));
  sqlx::query(&format!("DROP TRIGGER owned_reset_fault{}",if engine=="postgres"{format!(" ON {table}")}else{String::new()})).execute(&s.db).await.unwrap();if engine=="postgres"{sqlx::query("DROP FUNCTION owned_reset_fault()").execute(&s.db).await.unwrap();}
 }
 if case=="stale-admin"{
  ok(&s,Some(access(&caller)),"POST","/auth/logout",json!({})).await;let before=snapshot(&s.db,&["users","sessions"]).await;assert_eq!(call(&s,Some(access(&caller)),"POST",&path,Value::Null).await.0,StatusCode::UNAUTHORIZED);assert!(before==snapshot(&s.db,&["users","sessions"]).await);caller=ok(&s,None,"POST","/auth/login",login_body("admin@reset.invalid",PASSWORD)).await;
 }
 let reset=ok(&s,Some(access(&caller)),"POST",&path,Value::Null).await;assert_eq!(reset["email"],email);let password=reset["temporary_password"].as_str().unwrap();assert_eq!(password.len(),20);
 assert_eq!(call(&s,Some(access(&old)),"GET","/auth/me",Value::Null).await.0,StatusCode::UNAUTHORIZED);
 assert_eq!(call(&s,None,"POST","/auth/refresh",json!({"refresh_token":refresh(&old)})).await.0,StatusCode::UNAUTHORIZED);
 let signed=ok(&s,None,"POST","/auth/login",login_body(email,password)).await;assert_eq!(signed["user"]["id"],uid);
 let other=if case=="self"{&member}else{&caller};assert_eq!(call(&s,Some(access(other)),"GET","/auth/me",Value::Null).await.0,StatusCode::OK);
 assert!(ok(&s,Some(access(&signed)),"GET",&format!("/spaces/{space}/documents/reset-page/content"),Value::Null).await["content"].as_str().unwrap().contains("EXACT_RESET_PAGE Ω"));assert_eq!(files,bytes(&root));
 let expected=json!({"email":email,"password":password,"old":old,"space":space,"files":files});std::fs::write(work.join("expected.json"),expected.to_string()).unwrap();s.db.close().await;drop(s);
 let child=std::process::Command::new(std::env::current_exe().unwrap()).args(["postgres_admin_reset_fixture::fresh_reset_child","--ignored","--exact","--nocapture"]).env("STARLANE_ADMIN_RESET_CHILD_URL",url).env("STARLANE_ADMIN_RESET_SCHEMA",schema).output().unwrap();assert!(child.status.success(),"{}",String::from_utf8_lossy(&child.stdout));println!("RESET_FRESH_CHILD {}",String::from_utf8_lossy(&child.stdout));println!("RESET_ACTUAL_PARENT_PASS");
}
#[tokio::test]#[ignore="explicit disposable both-engine fixture"]async fn required_postgres_flow(){run("postgres").await;}
#[tokio::test]#[ignore="explicit disposable both-engine fixture"]async fn required_sqlite_flow(){run("sqlite").await;}
#[tokio::test]#[ignore="only genuine parent launches"]async fn fresh_reset_child(){
 let url=std::env::var("STARLANE_ADMIN_RESET_CHILD_URL").expect("required parent URL");let schema=std::env::var("STARLANE_ADMIN_RESET_SCHEMA").expect("required parent schema");let root=crate::storage::data_dir();assert!(root.starts_with(workspace()));let e:Value=serde_json::from_slice(&std::fs::read(workspace().join("expected.json")).unwrap()).unwrap();let s=state(pool(&url,&schema).await,&root);
 crate::storage::restoration::bootstrap_gate(&root).unwrap();crate::storage::restoration::activate_in(&root,&s.db).await.unwrap();crate::storage::relocation::recover(&s.db).await.unwrap();crate::storage::journal::recover(&s.db).await.unwrap();crate::storage::canonical_rewrite::recover(&s.db).await.unwrap();crate::storage::publication::recover(&s.db).await.unwrap();crate::storage::attachment_upload::recover_in(&root,&s.db).await.unwrap();
 assert_eq!(call(&s,Some(access(&e["old"])),"GET","/auth/me",Value::Null).await.0,StatusCode::UNAUTHORIZED);let signed=ok(&s,None,"POST","/auth/login",login_body(e["email"].as_str().unwrap(),e["password"].as_str().unwrap())).await;
 assert!(ok(&s,Some(access(&signed)),"GET",&format!("/spaces/{}/documents/reset-page/content",e["space"].as_str().unwrap()),Value::Null).await["content"].as_str().unwrap().contains("EXACT_RESET_PAGE Ω"));assert_eq!(serde_json::to_value(bytes(&root)).unwrap(),e["files"]);s.db.close().await;drop(s);println!("RESET_ACTUAL_FRESH_RESTART_PASS");
}
