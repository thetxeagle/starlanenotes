//! Required disposable engines only; no webhook transport or listener is started.
use super::{matching_webhooks,webhook_event_matches,dispatch_event,PUBLIC_PAGE_PUBLISHED_EVENT};
use sqlx::{AnyPool,Connection};
use std::{path::{Path,PathBuf},collections::BTreeSet};
use serde_json::{json,Value};
use axum::{body::Body,http::{Request,StatusCode}};
use tower::ServiceExt;
const JWT:&str="owned-audit-webhook-fixture-secret";
fn owned()->PathBuf{PathBuf::from(std::env::var("STARLANE_AUDIT_WEBHOOK_WORKSPACE").expect("required owned workspace"))}
async fn pool(url:&str,schema:&str)->AnyPool{
 sqlx::any::install_default_drivers();let engine=crate::db::DatabaseEngine::from_url(url).unwrap();let schema=schema.to_owned();
 sqlx::any::AnyPoolOptions::new().max_connections(2).after_connect(move|c,_|{let schema=schema.clone();Box::pin(async move{
  match engine{crate::db::DatabaseEngine::Postgres=>{sqlx::query(&format!("SET search_path TO {schema}")).execute(c).await?;},crate::db::DatabaseEngine::Sqlite=>{sqlx::query("PRAGMA foreign_keys=ON").execute(c).await?;}}Ok(())
 })}).connect(url).await.unwrap()
}
fn ids(rows:&[(String,String,Option<String>,String)])->BTreeSet<String>{rows.iter().map(|r|r.0.clone()).collect()}
fn expected(names:&[&str])->BTreeSet<String>{names.iter().map(|s|s.to_string()).collect()}
async fn exercise(url:&str,schema:&str,root:&Path){
 let db=pool(url,schema).await;crate::db::migrate_selected(&db,crate::db::DatabaseEngine::from_url(url).unwrap()).await.unwrap();
 sqlx::query("INSERT INTO users(id,email,display_name,role,account_kind) VALUES('aw-owner','aw@fixture.invalid','Owned member','member','member')").execute(&db).await.unwrap();
 sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,session_kind) VALUES('aw-session','aw-owner','not-a-token','2999-01-01T00:00:00Z','browser')").execute(&db).await.unwrap();
 for space in ["aw-a","aw-b"]{sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,$1,$1,'private','aw-owner')").bind(space).execute(&db).await.unwrap();}
 crate::audit::log_action(&db,Some("aw-owner"),Some("192.0.2.4"),"fixture.actual",Some("document"),Some("stable-id")).await;
 crate::audit::log_action(&db,None,None,"fixture.optional",None,None).await;
 let records:Vec<(String,Option<String>,Option<String>,String,Option<String>,Option<String>,String)>=sqlx::query_as("SELECT id,user_id,ip_address,action,target_type,target_id,created_at FROM audit_log ORDER BY action").fetch_all(&db).await.unwrap();
 assert_eq!(records.len(),2,"actual log_action must persist both records, not swallow PG syntax error");
 let required=&records[0];assert_eq!((&required.1,&required.2,required.3.as_str(),&required.4,&required.5),(&Some("aw-owner".into()),&Some("192.0.2.4".into()),"fixture.actual",&Some("document".into()),&Some("stable-id".into())));
 let optional=&records[1];assert_eq!((&optional.1,&optional.2,optional.3.as_str(),&optional.4,&optional.5),(&None,&None,"fixture.optional",&None,&None));
 for row in &records{uuid::Uuid::parse_str(&row.0).unwrap();chrono::DateTime::parse_from_rfc3339(&row.6).unwrap();}
 let mut logged=Vec::<Value>::new();for entry in std::fs::read_dir(root.join("logging/lifecycle")).unwrap(){let entry=entry.unwrap();assert_eq!(entry.path().extension().unwrap(),"jsonl");for line in std::fs::read_to_string(entry.path()).unwrap().lines(){logged.push(serde_json::from_str(line).unwrap());}}
 assert_eq!(logged.len(),2);for row in &records{let e=logged.iter().find(|v|v["id"]==row.0).unwrap();assert_eq!(e["action"],row.3);assert_eq!(e["user_id"],json!(row.1));assert_eq!(e["ip_address"],json!(row.2));assert_eq!(e["target_type"],json!(row.4));assert_eq!(e["target_id"],json!(row.5));}
 for(id,space,active,events,secret)in [
  ("global",None,1,"[\"public_page.published\"]",None),("scoped",Some("aw-a"),1,"[\"public_page.published\"]",Some("owned-not-credential")),
  ("unrelated",Some("aw-b"),1,"[\"public_page.published\"]",None),("inactive",Some("aw-a"),0,"[\"public_page.published\"]",None),
  ("inactive-global",None,0,"[\"public_page.published\"]",None),("malformed",Some("aw-a"),1,"{}",None),("not-subscribed",Some("aw-a"),1,"[\" public_page.published \" ]",None)]{
  sqlx::query("INSERT INTO webhooks(id,space_id,name,url,secret_hash,events,is_active,created_by) VALUES($1,$2,$1,'http://never-contact.invalid',$3,$4,$5,'aw-owner')").bind(id).bind(space).bind(secret).bind(events).bind(active as i64).execute(&db).await.unwrap();
 }
 let scoped=matching_webhooks(&db,Some("aw-a")).await.unwrap();assert_eq!(ids(&scoped),expected(&["global","scoped","malformed","not-subscribed"]));
 assert_eq!(scoped.iter().find(|v|v.0=="scoped").unwrap().2.as_deref(),Some("owned-not-credential"));assert!(scoped.iter().find(|v|v.0=="global").unwrap().2.is_none());
 let selected:BTreeSet<String>=scoped.iter().filter(|v|webhook_event_matches(&serde_json::from_str::<Vec<String>>(&v.3).unwrap_or_default(),PUBLIC_PAGE_PUBLISHED_EVENT)).map(|v|v.0.clone()).collect();assert_eq!(selected,expected(&["global","scoped"]));
 assert_eq!(ids(&matching_webhooks(&db,Some("aw-b")).await.unwrap()),expected(&["global","unrelated"]));
 assert_eq!(ids(&matching_webhooks(&db,None).await.unwrap()),expected(&["global"]));
 assert_eq!(ids(&matching_webhooks(&db,Some("missing-space")).await.unwrap()),expected(&["global"]));
 // Legacy private events remain denied even when old records survive migration.
 dispatch_event(&db,"fixture.not-subscribed",&json!({"owned":"no delivery"}),Some("aw-a")).await;
 dispatch_event(&db,"fixture.not-subscribed",&json!({"owned":"no delivery"}),None).await;
 let deliveries:i64=sqlx::query_scalar("SELECT count(*) FROM webhook_deliveries").fetch_one(&db).await.unwrap();assert_eq!(deliveries,0);
 let state=crate::AppState{db:db.clone(),jwt_secret:JWT.into(),base_url:"http://owned.invalid".into(),search_index:crate::search::SearchIndex::new(root.join("search_index")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};
 for token in [None,Some(crate::auth::jwt::create_token_pair_for_session("aw-owner","aw@fixture.invalid","member",JWT,Some("aw-session"),30).unwrap().access_token)]{
  let expected=StatusCode::UNAUTHORIZED;let mut request=Request::builder().uri("/webhooks");if let Some(token)=token{request=request.header("Authorization",format!("Bearer {token}"));}
  let response=crate::api_routes().with_state(state.clone()).oneshot(request.body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),expected);
 }
 let after:Vec<(String,Option<String>,Option<String>,String,Option<String>,Option<String>,String)>=sqlx::query_as("SELECT id,user_id,ip_address,action,target_type,target_id,created_at FROM audit_log ORDER BY action").fetch_all(&db).await.unwrap();assert_eq!(after,records);
 assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM webhooks").fetch_one(&db).await.unwrap(),7);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM webhook_deliveries").fetch_one(&db).await.unwrap(),0);
 println!("AUDIT_WEBHOOK_ACTUAL_PASS audit2+matching nullable/scoped/global/inactive/event denied+actual member401 anonymous401; no deliveries/listeners");
 db.close().await;drop(state);
}
#[tokio::test]
#[ignore="requires owned isolated PostgreSQL fixture controls"]
async fn required_postgres_audit_webhook(){
 let root=crate::storage::data_dir();assert!(root.starts_with(owned()));std::fs::create_dir_all(&root).unwrap();let url=std::env::var("STARLANE_AUDIT_WEBHOOK_PG_URL").expect("required owned PG URL");assert!(url.starts_with("postgres://"));sqlx::any::install_default_drivers();let schema=format!("aw_{}",uuid::Uuid::new_v4().simple());let mut c=sqlx::AnyConnection::connect(&url).await.unwrap();sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&mut c).await.unwrap();c.close().await.unwrap();exercise(&url,&schema,&root).await;
}
#[tokio::test]
#[ignore="requires owned isolated SQLite fixture controls"]
async fn required_sqlite_audit_webhook(){let root=crate::storage::data_dir();assert!(root.starts_with(owned()));std::fs::create_dir_all(&root).unwrap();exercise(&format!("sqlite://{}?mode=rwc",root.join("starlane-notes.db").display()),"",&root).await;}
