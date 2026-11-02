//! Required real first-user/native/config producers on both migrated engines.
use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
use serde_json::{json,Value};use sqlx::{AnyPool,Connection,Column,Row};use tower::ServiceExt;
use std::{path::{Path,PathBuf},collections::BTreeMap,io::Read};use sha2::{Digest,Sha256};
const JWT:&str="owned-password-fixture-secret";const PASSWORD:&str="owned-fixture-password-not-a-real-account";
fn workspace()->PathBuf{PathBuf::from(std::env::var("STARLANE_PASSKEY_CRYPTO_WORKSPACE").expect("required owned cryptographic passkey workspace"))}
async fn pool(url:&str,schema:&str)->AnyPool{sqlx::any::install_default_drivers();let engine=crate::db::DatabaseEngine::from_url(url).unwrap();let schema=schema.to_owned();sqlx::any::AnyPoolOptions::new().max_connections(3).after_connect(move|c,_|{let schema=schema.clone();Box::pin(async move{match engine{crate::db::DatabaseEngine::Postgres=>{sqlx::query(&format!("SET search_path TO {schema}")).execute(c).await?;},crate::db::DatabaseEngine::Sqlite=>{sqlx::query("PRAGMA foreign_keys=ON").execute(&mut*c).await?;sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;}}Ok(())})}).connect(url).await.unwrap()}
fn state(db:AnyPool,root:&Path)->crate::AppState{crate::AppState{db,jwt_secret:JWT.into(),base_url:"https://passkey.owned.invalid".into(),search_index:crate::search::SearchIndex::new(root.join("search_index")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()}}

async fn call(s:&crate::AppState,token:Option<&str>,method:&str,uri:&str,body:Value)->(StatusCode,Value){println!("CRYPTO_STEP {method} {uri}");let mut q=Request::builder().method(method).uri(uri).header("Content-Type","application/json");if let Some(t)=token{q=q.header("Authorization",format!("Bearer {t}"));}let mut req=q.body(Body::from(body.to_string())).unwrap();req.extensions_mut().insert(axum::extract::ConnectInfo("127.0.0.1:43123".parse::<std::net::SocketAddr>().unwrap()));let r=crate::api_routes().with_state(s.clone()).oneshot(req).await.unwrap();let code=r.status();let bytes=to_bytes(r.into_body(),4*1024*1024).await.unwrap();(code,if bytes.is_empty(){Value::Null}else{serde_json::from_slice(&bytes).unwrap_or(json!({"non_json":true}))})}
async fn ok(s:&crate::AppState,token:Option<&str>,method:&str,uri:&str,body:Value)->Value{let(code,v)=call(s,token,method,uri,body).await;assert!(code.is_success(),"actual {method} {uri} status={code}; response suppressed to protect credentials");v}
fn access(v:&Value)->&str{v["access_token"].as_str().unwrap()}fn refresh(v:&Value)->&str{v["refresh_token"].as_str().unwrap()}fn sid(v:&Value)->String{crate::auth::jwt::validate_token(access(v),JWT,crate::auth::jwt::TokenType::Access).unwrap().sid}
async fn snapshot(db:&AnyPool,tables:&[&str])->BTreeMap<String,Vec<Value>>{let mut out=BTreeMap::new();for table in tables{let mut values=Vec::new();for row in sqlx::query(&format!("SELECT * FROM {table}")).fetch_all(db).await.unwrap(){let mut cells=BTreeMap::new();for col in row.columns(){let n=col.name();let v=if let Ok(v)=row.try_get::<Option<String>,_>(n){v.map(|v|json!(["text",v])).unwrap_or(json!(["null"]))}else if let Ok(v)=row.try_get::<Option<i64>,_>(n){v.map(|v|json!(["integer",v.to_string()])).unwrap_or(json!(["null"]))}else if let Ok(v)=row.try_get::<Option<Vec<u8>>,_>(n){v.map(|v|json!(["blob",hex::encode(v)])).unwrap_or(json!(["null"]))}else{panic!("unsupported cell {table}.{n}")};cells.insert(n.to_string(),v);}values.push(serde_json::to_value(cells).unwrap());}values.sort_by_cached_key(Value::to_string);out.insert(table.to_string(),values);}out}
fn login_body(email:&str,password:&str)->Value{json!({"email":email,"password":password,"device_name":"Owned browser","session_kind":"browser","client_platform":"web"})}
fn bytes(root:&Path)->BTreeMap<String,String>{fn walk(root:&Path,p:&Path,out:&mut BTreeMap<String,String>){let m=std::fs::symlink_metadata(p).unwrap();assert!(!m.file_type().is_symlink());if m.is_dir(){for e in std::fs::read_dir(p).unwrap(){walk(root,&e.unwrap().path(),out);}}else{assert!(m.is_file());let mut f=std::fs::File::open(p).unwrap();let mut h=Sha256::new();let mut b=[0u8;65536];loop{let n=f.read(&mut b).unwrap();if n==0{break}h.update(&b[..n]);}out.insert(p.strip_prefix(root).unwrap().to_string_lossy().into(),hex::encode(h.finalize()));}}let mut out=BTreeMap::new();for n in["spaces",".storage-journal",".publication-journal",".canonical-rewrite-journal",".move-relocation-journal"]{let p=root.join(n);if p.exists(){walk(root,&p,&mut out);}}out}





const EMAIL:&str="crypto-owner@owned.invalid";
const STABLE:&[&str]=&["users","spaces","space_members","documents","document_drafts","storage_journal_receipts","system_config"];
async fn software(kind:&str,options:Value)->Value{
 let dir=workspace().join("ceremonies");std::fs::create_dir_all(&dir).unwrap();let id=uuid::Uuid::new_v4().to_string();let request=dir.join(format!("request-{id}.json"));let response=dir.join(format!("response-{id}.json"));std::fs::write(request,json!({"id":id,"kind":kind,"options":options}).to_string()).unwrap();
 let raw=tokio::time::timeout(std::time::Duration::from_secs(45),async{loop{if response.is_file(){break std::fs::read(&response).unwrap();}tokio::time::sleep(std::time::Duration::from_millis(25)).await;}}).await.expect("required real software-authenticator response");assert!(raw.len()<65536);let v:Value=serde_json::from_slice(&raw).unwrap();assert_eq!(v["id"],id);assert!(v["credential"].is_object(),"genuine browser ceremony failed; no accepted credential substitution");println!("CRYPTO_REAL_BROWSER_PROOF {kind}");v["credential"].clone()
}
async fn authenticate(s:&crate::AppState)->Value{
 let start=ok(s,None,"POST","/auth/passkeys/login/start",json!({"email":EMAIL})).await;let credential=software("get",start["public_key"].clone()).await;
 ok(s,None,"POST","/auth/passkeys/login/finish",json!({"challenge_id":start["challenge_id"],"credential":credential,"device_name":"Owned virtual authenticator","session_kind":"browser","client_platform":"web"})).await
}
const ATOMIC_TABLES:&[&str]=&["passkeys","sessions","passkey_challenges"];
async fn fault(db:&AnyPool,engine:&str,kind:&str){
 let (table,timing,body)=match kind{
 "enrollment-failure"=>("passkeys","BEFORE","failure"),
 "login-failure"=>("sessions","BEFORE","failure"),
 "enrollment-drift"=>("passkeys","AFTER","UPDATE passkeys SET name='foreign drift' WHERE id=NEW.id;"),
 "login-drift"=>("sessions","AFTER","UPDATE passkeys SET sign_count=999 WHERE user_id=NEW.user_id;"),
 _=>panic!("unknown actual database fault")};
 if engine=="postgres"{let body=if body=="failure"{"RAISE EXCEPTION 'owned atomicity failure';"}else{body};sqlx::query(&format!("CREATE FUNCTION owned_atomic_fault() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN {body} RETURN NEW; END $$")).execute(db).await.unwrap();sqlx::query(&format!("CREATE TRIGGER owned_atomic_fault {timing} INSERT ON {table} FOR EACH ROW EXECUTE FUNCTION owned_atomic_fault()")).execute(db).await.unwrap();}
 else {let body=if body=="failure"{"SELECT RAISE(ABORT,'owned atomicity failure');"}else{body};sqlx::query(&format!("CREATE TRIGGER owned_atomic_fault {timing} INSERT ON {table} BEGIN {body} END")).execute(db).await.unwrap();}
}
async fn clear_fault(db:&AnyPool,engine:&str,table:&str){
 if engine=="postgres"{sqlx::query(&format!("DROP TRIGGER owned_atomic_fault ON {table}")).execute(db).await.unwrap();sqlx::query("DROP FUNCTION owned_atomic_fault()").execute(db).await.unwrap();}
 else{sqlx::query("DROP TRIGGER owned_atomic_fault").execute(db).await.unwrap();}
}
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
async fn signed_request(s:&crate::AppState)->Value{let start=ok(s,None,"POST","/auth/passkeys/login/start",json!({"email":EMAIL})).await;let credential=software("get",start["public_key"].clone()).await;json!({"challenge_id":start["challenge_id"],"credential":credential,"device_name":"Atomic authenticator","session_kind":"browser","client_platform":"web"})}

async fn run(engine:&str){
 let case=std::env::var("STARLANE_PASSKEY_ATOMIC_CASE").expect("required atomicity scenario");assert!(matches!(case.as_str(),"enrollment-failure"|"login-failure"|"enrollment-drift"|"login-drift"|"deletion"|"concurrency"|"counter-order"|"counter-legacy"|"inactive-account"));
 let root=crate::storage::data_dir();assert!(root.starts_with(workspace()));let url=if engine=="postgres"{std::env::var("STARLANE_PASSKEY_CRYPTO_PG_URL").expect("required owned crypto PG URL")}else{format!("sqlite://{}?mode=rwc",root.join("starlane-notes.db").display())};std::fs::create_dir_all(&root).unwrap();
 let schema=if engine=="postgres"{sqlx::any::install_default_drivers();let n=format!("crypto_{}",uuid::Uuid::new_v4().simple());let mut c=sqlx::AnyConnection::connect(&url).await.unwrap();sqlx::query(&format!("CREATE SCHEMA {n}")).execute(&mut c).await.unwrap();c.close().await.unwrap();n}else{String::new()};std::fs::write(workspace().join("schema.txt"),&schema).unwrap();let db=pool(&url,&schema).await;crate::db::migrate_selected(&db,crate::db::DatabaseEngine::from_url(&url).unwrap()).await.unwrap();let s=state(db,&root);
 let signup=ok(&s,None,"POST","/auth/register",json!({"email":EMAIL,"display_name":"Crypto owner","password":PASSWORD})).await;let login=ok(&s,None,"POST","/auth/login",login_body(EMAIL,PASSWORD)).await;assert_eq!(signup["user"]["id"],login["user"]["id"]);let uid=login["user"]["id"].as_str().unwrap();let space:String=sqlx::query_scalar("SELECT id FROM spaces WHERE created_by=$1").bind(uid).fetch_one(&s.db).await.unwrap();let page=ok(&s,Some(access(&login)),"POST",&format!("/spaces/{space}/documents"),json!({"source_id":"crypto-page","title":"Crypto page","content":"EXACT_CRYPTO_PAGE Ω","property_values":[]})).await;ok(&s,Some(access(&login)),"POST",&format!("/spaces/{space}/documents/crypto-page/drafts/{}/publish",page["draft_id"].as_str().unwrap()),json!({})).await;
 let outsider=if case=="deletion" {
 ok(&s,Some(access(&login)),"POST","/admin/config",json!({"key":"allow_public_signups","value":"true"})).await;
 ok(&s,None,"POST","/auth/register",json!({"email":"other@atomic.owned.invalid","display_name":"Other actual owner","password":PASSWORD})).await;
 Some(ok(&s,None,"POST","/auth/login",login_body("other@atomic.owned.invalid",PASSWORD)).await)
 }else{None};
 let source=snapshot(&s.db,STABLE).await;let files=bytes(&root);let start=ok(&s,Some(access(&login)),"POST","/auth/passkeys/register/start",json!({"name":"Real virtual CTAP2"})).await;let credential=software("create",start["public_key"].clone()).await;assert_eq!(credential["type"],"public-key");let registration_body=json!({"challenge_id":start["challenge_id"],"name":"Real virtual CTAP2","credential":credential});
 if case.starts_with("enrollment-") {
 fault(&s.db,engine,&case).await;let before=snapshot(&s.db,ATOMIC_TABLES).await;let begin=chrono::Utc::now();
 let(code,_)=call(&s,Some(access(&login)),"POST","/auth/passkeys/register/finish",registration_body.clone()).await;let end=chrono::Utc::now();
 assert_eq!(code,if case=="enrollment-failure"{StatusCode::INTERNAL_SERVER_ERROR}else{StatusCode::CONFLICT});session_touch(&before,&snapshot(&s.db,ATOMIC_TABLES).await,&sid(&login),begin,end);assert_eq!(files,bytes(&root));
 clear_fault(&s.db,engine,"passkeys").await;println!("ATOMIC_GENUINE_ENROLLMENT_ROLLBACK_PASS");
 }
 let registered=ok(&s,Some(access(&login)),"POST","/auth/passkeys/register/finish",registration_body.clone()).await;
 if case.starts_with("enrollment-"){let before=snapshot(&s.db,ATOMIC_TABLES).await;let begin=chrono::Utc::now();assert_eq!(call(&s,Some(access(&login)),"POST","/auth/passkeys/register/finish",registration_body).await.0,StatusCode::BAD_REQUEST);session_touch(&before,&snapshot(&s.db,ATOMIC_TABLES).await,&sid(&login),begin,chrono::Utc::now());}
 let id=registered["id"].as_str().unwrap();let row:(String,String,String)=sqlx::query_as("SELECT user_id,credential_id,public_key FROM passkeys WHERE id=$1").bind(id).fetch_one(&s.db).await.unwrap();assert_eq!(row.0,uid);let _:webauthn_rs::prelude::Passkey=serde_json::from_str(&row.2).unwrap();let list=ok(&s,Some(access(&login)),"GET","/auth/passkeys",Value::Null).await;assert_eq!(list.as_array().unwrap().len(),1);assert_eq!(list[0]["id"],id);
 if case.starts_with("login-") {
 let request=signed_request(&s).await;fault(&s.db,engine,&case).await;let before=snapshot(&s.db,ATOMIC_TABLES).await;
 assert_eq!(call(&s,None,"POST","/auth/passkeys/login/finish",request.clone()).await.0,if case=="login-failure"{StatusCode::INTERNAL_SERVER_ERROR}else{StatusCode::CONFLICT});
 assert!(before==snapshot(&s.db,ATOMIC_TABLES).await,"entire challenge/credential/session transaction must roll back");assert_eq!(files,bytes(&root));clear_fault(&s.db,engine,"sessions").await;
 let accepted=ok(&s,None,"POST","/auth/passkeys/login/finish",request.clone()).await;
 assert_eq!(accepted["user"]["id"],uid);let committed=snapshot(&s.db,ATOMIC_TABLES).await;
 assert_eq!(call(&s,None,"POST","/auth/passkeys/login/finish",request).await.0,StatusCode::BAD_REQUEST);assert!(committed==snapshot(&s.db,ATOMIC_TABLES).await);println!("ATOMIC_GENUINE_LOGIN_ROLLBACK_LOST_ACK_PASS");
}
if case=="concurrency" {
 let request=signed_request(&s).await;let sessions:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sessions").fetch_one(&s.db).await.unwrap();
 let(left,right)=tokio::join!(call(&s,None,"POST","/auth/passkeys/login/finish",request.clone()),call(&s,None,"POST","/auth/passkeys/login/finish",request));
 assert!((left.0.is_success()&&right.0==StatusCode::BAD_REQUEST)||(right.0.is_success()&&left.0==StatusCode::BAD_REQUEST));
 assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM sessions").fetch_one(&s.db).await.unwrap(),sessions+1);assert_eq!(files,bytes(&root));println!("ATOMIC_GENUINE_CONCURRENT_SINGLE_CONSUMER_PASS");
}
if case=="counter-legacy" {
 let early=signed_request(&s).await;let pending=signed_request(&s).await;let newest=signed_request(&s).await;
 assert_eq!(crate::auth::routes::legacy_counter_produce(s.clone(),newest).await,StatusCode::OK);
 assert_eq!(crate::auth::routes::legacy_counter_produce(s.clone(),early).await,StatusCode::OK);
 let (sql,raw):(i64,String)=sqlx::query_as("SELECT sign_count,public_key FROM passkeys WHERE id=$1").bind(id).fetch_one(&s.db).await.unwrap();
 let v:Value=serde_json::from_str(&raw).unwrap();let native=v["cred"]["counter"].as_i64().unwrap();assert!(native>sql);println!("LEGACY_AUTHENTIC_SQL_NATIVE_MISMATCH sql={sql} native={native}");
 let before=snapshot(&s.db,&["passkeys","sessions"]).await;let(code,_)=call(&s,None,"POST","/auth/passkeys/login/finish",pending).await;println!("LEGACY_CANDIDATE_PENDING_STALE_STATUS {code}");assert_eq!(code,StatusCode::UNAUTHORIZED);assert!(before==snapshot(&s.db,&["passkeys","sessions"]).await);
 println!("LEGACY_AUTHENTIC_STALE_PENDING_REFUSED");
}
if case=="counter-order" {
 let earlier=signed_request(&s).await;let later=signed_request(&s).await;
 let n:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sessions").fetch_one(&s.db).await.unwrap();ok(&s,None,"POST","/auth/passkeys/login/finish",later).await;
 let expected=snapshot(&s.db,&["passkeys","sessions"]).await;
 assert_eq!(call(&s,None,"POST","/auth/passkeys/login/finish",earlier).await.0,StatusCode::UNAUTHORIZED);
 assert!(expected==snapshot(&s.db,&["passkeys","sessions"]).await);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM sessions").fetch_one(&s.db).await.unwrap(),n+1);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM passkey_challenges").fetch_one(&s.db).await.unwrap(),0);println!("ATOMIC_GENUINE_STALE_COUNTER_REFUSAL_PASS");
}
if case=="inactive-account" {
 let request=signed_request(&s).await;sqlx::query("UPDATE users SET is_active=0 WHERE id=$1").bind(uid).execute(&s.db).await.unwrap();let before=snapshot(&s.db,ATOMIC_TABLES).await;
 assert_eq!(call(&s,None,"POST","/auth/passkeys/login/finish",request.clone()).await.0,StatusCode::UNAUTHORIZED);assert!(before==snapshot(&s.db,ATOMIC_TABLES).await);assert_eq!(files,bytes(&root));
 sqlx::query("UPDATE users SET is_active=1 WHERE id=$1").bind(uid).execute(&s.db).await.unwrap();ok(&s,None,"POST","/auth/passkeys/login/finish",request).await;println!("ATOMIC_GENUINE_CURRENT_ACCOUNT_DENIAL_PASS");
}
if case=="deletion" {
 let other=outsider.as_ref().unwrap();let before=snapshot(&s.db,ATOMIC_TABLES).await;let begin=chrono::Utc::now();assert_eq!(call(&s,Some(access(other)),"DELETE",&format!("/auth/passkeys/{id}"),Value::Null).await.0,StatusCode::NOT_FOUND);session_touch(&before,&snapshot(&s.db,ATOMIC_TABLES).await,&sid(other),begin,chrono::Utc::now());
 let stale=signed_request(&s).await;let fresh_owner=ok(&s,None,"POST","/auth/login",login_body(EMAIL,PASSWORD)).await;
 ok(&s,Some(access(&login)),"POST","/auth/logout",Value::Null).await;let before=snapshot(&s.db,ATOMIC_TABLES).await;assert_eq!(call(&s,Some(access(&login)),"DELETE",&format!("/auth/passkeys/{id}"),Value::Null).await.0,StatusCode::UNAUTHORIZED);assert!(before==snapshot(&s.db,ATOMIC_TABLES).await);
 ok(&s,Some(access(&fresh_owner)),"DELETE",&format!("/auth/passkeys/{id}"),Value::Null).await;assert!(snapshot(&s.db,&["passkeys"]).await["passkeys"].is_empty());
 let preserved=snapshot(&s.db,&["passkeys","sessions"]).await;assert_eq!(call(&s,None,"POST","/auth/passkeys/login/finish",stale).await.0,StatusCode::UNAUTHORIZED);assert!(preserved==snapshot(&s.db,&["passkeys","sessions"]).await);assert_eq!(files,bytes(&root));
 let start=ok(&s,Some(access(&fresh_owner)),"POST","/auth/passkeys/register/start",json!({"name":"Replacement actual key"})).await;let credential=software("create",start["public_key"].clone()).await;ok(&s,Some(access(&fresh_owner)),"POST","/auth/passkeys/register/finish",json!({"challenge_id":start["challenge_id"],"name":"Replacement actual key","credential":credential})).await;println!("ATOMIC_GENUINE_OWNER_CURRENT_SESSION_DELETION_PASS");
}
let options=ok(&s,None,"POST","/auth/sign-in-options",json!({"email":EMAIL})).await;assert_eq!(options,json!({"password_available":true,"passkey_available":true}));let sessions:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id=$1").bind(uid).fetch_one(&s.db).await.unwrap();let denial_before=snapshot(&s.db,&["passkeys","sessions","passkey_challenges"]).await;let refused_start=ok(&s,None,"POST","/auth/passkeys/login/start",json!({"email":EMAIL})).await;let mut tampered=software("get",refused_start["public_key"].clone()).await;use base64::Engine as _;let mut signature=base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(tampered["response"]["signature"].as_str().unwrap()).unwrap();assert!(!signature.is_empty());signature[0]^=1;tampered["response"]["signature"]=json!(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature));let denied_body=json!({"challenge_id":refused_start["challenge_id"],"credential":tampered,"device_name":"Rejected real signature","session_kind":"browser","client_platform":"web"});let(code,_)=call(&s,None,"POST","/auth/passkeys/login/finish",denied_body.clone()).await;assert_eq!(code,StatusCode::UNAUTHORIZED);assert_eq!(snapshot(&s.db,&["passkeys","sessions","passkey_challenges"]).await,denial_before);let(code,_)=call(&s,None,"POST","/auth/passkeys/login/finish",denied_body).await;assert_eq!(code,StatusCode::BAD_REQUEST);assert_eq!(snapshot(&s.db,&["passkeys","sessions","passkey_challenges"]).await,denial_before);assert_eq!(files,bytes(&root));println!("CRYPTO_REAL_SIGNATURE_DENIAL_PRESERVES_STATE");let signed=authenticate(&s).await;assert_eq!(signed["user"]["id"],uid);assert_eq!(signed["user"]["account_kind"],"member");let session:(String,String,String,String)=sqlx::query_as("SELECT user_id,device_name,session_kind,client_platform FROM sessions WHERE id=$1").bind(sid(&signed)).fetch_one(&s.db).await.unwrap();assert_eq!(session,(uid.into(),"Owned virtual authenticator".into(),"browser".into(),"web".into()));assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM sessions WHERE user_id=$1").bind(uid).fetch_one(&s.db).await.unwrap(),sessions+1);ok(&s,Some(access(&signed)),"GET","/auth/me",Value::Null).await;assert!(source==snapshot(&s.db,STABLE).await);assert_eq!(files,bytes(&root));println!("CRYPTO_REAL_ENROLLMENT_LOGIN_PASS {engine}");
 let e=json!({"login":signed,"space":space,"stable":source,"files":files,"passkeys":snapshot(&s.db,&["passkeys"]).await});std::fs::write(workspace().join("expected.json"),e.to_string()).unwrap();s.db.close().await;drop(s);let child=std::process::Command::new(std::env::current_exe().unwrap()).args(["postgres_passkey_atomic_fixture::fresh_crypto_child","--ignored","--exact","--nocapture"]).env("STARLANE_PASSKEY_CRYPTO_CHILD_URL",url).env("STARLANE_PASSKEY_CRYPTO_SCHEMA",schema).output().unwrap();assert!(child.status.success(),"{}\n{}",String::from_utf8_lossy(&child.stdout),String::from_utf8_lossy(&child.stderr));println!("CRYPTO_FRESH_CHILD {}",String::from_utf8_lossy(&child.stdout));
}
#[tokio::test]#[ignore="required real software authenticator orchestration"]async fn required_postgres_flow(){run("postgres").await;}
#[tokio::test]#[ignore="required real software authenticator orchestration"]async fn required_sqlite_flow(){run("sqlite").await;}
#[tokio::test]#[ignore="only genuine parent launches"]async fn fresh_crypto_child(){let url=std::env::var("STARLANE_PASSKEY_CRYPTO_CHILD_URL").expect("required crypto child URL");let schema=std::env::var("STARLANE_PASSKEY_CRYPTO_SCHEMA").expect("required parent schema");let root=crate::storage::data_dir();assert!(root.starts_with(workspace()));let e:Value=serde_json::from_slice(&std::fs::read(workspace().join("expected.json")).unwrap()).unwrap();let s=state(pool(&url,&schema).await,&root);assert_eq!(serde_json::to_value(snapshot(&s.db,STABLE).await).unwrap(),e["stable"]);assert_eq!(serde_json::to_value(snapshot(&s.db,&["passkeys"]).await).unwrap(),e["passkeys"]);crate::storage::restoration::bootstrap_gate(&root).unwrap();crate::storage::restoration::activate_in(&root,&s.db).await.unwrap();crate::storage::relocation::recover(&s.db).await.unwrap();crate::storage::journal::recover(&s.db).await.unwrap();crate::storage::canonical_rewrite::recover(&s.db).await.unwrap();crate::storage::publication::recover(&s.db).await.unwrap();crate::storage::attachment_upload::recover_in(&root,&s.db).await.unwrap();ok(&s,Some(access(&e["login"])),"GET","/auth/me",Value::Null).await;let a=authenticate(&s).await;assert_eq!(a["user"]["id"],e["login"]["user"]["id"]);let content=ok(&s,Some(access(&a)),"GET",&format!("/spaces/{}/documents/crypto-page/content",e["space"].as_str().unwrap()),Value::Null).await;assert!(content["content"].as_str().unwrap().contains("EXACT_CRYPTO_PAGE Ω"));assert_eq!(serde_json::to_value(snapshot(&s.db,STABLE).await).unwrap(),e["stable"]);assert_eq!(serde_json::to_value(bytes(&root)).unwrap(),e["files"]);s.db.close().await;drop(s);println!("CRYPTO_REAL_FRESH_RESTART_PASS");}
