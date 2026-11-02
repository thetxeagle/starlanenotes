//! Isolated real route / original lease tests. Run each engine in a separate process.
use super::*;
use sqlx::any::AnyPoolOptions;
use serde_json::json;
use tower::ServiceExt;

fn operation(id: &str, asset: &str, source: &str) -> Value {
 json!({"id":id,"entity_type":"asset","entity_id":asset,"action":"create","queued_at":"2026-10-08T12:00:00Z","payload_status":"available","payload":{"id":asset,"document_id":"doc-1","space_id":"space-1","name":"Original File.png","mime_type":"image/png","data_url":"data:image/png;base64,QVNTRVQgQllURVM=","source_url":source}})
}
fn manifest(op: Value) -> Value {
 json!({"schema_version":1,"provider":"self-hosted","operation_count":1,"recovery_checkpoint_count":0,"recovery_checkpoints":[],"operations":[op],"replay_contract":{"schema_version":1,"operation_order":"queued_at_ascending","idempotency_key":"operation.id","transport":"server-api","server_backends":["sqlite","postgresql"],"required_payload_status":"available","conflict_policy":"server_validates_version_and_permissions","web_canonical_store":"server-api"}})
}
async fn count(state: &AppState, table: &str) -> i64 {
 sqlx::query_scalar(&format!("SELECT count(*) FROM {table}")).fetch_one(&state.db).await.unwrap()
}
#[tokio::test(flavor="current_thread")]
#[ignore = "owns a disposable database/root and real ephemeral TCP listener; run engines separately"]
async fn replay_capacity_actual_tcp_and_cancelled_axum_future() {
 sqlx::any::install_default_drivers();
 let root = std::path::PathBuf::from(std::env::var("DATA_DIR").expect("owned DATA_DIR"));
 std::fs::create_dir_all(&root).unwrap();
 let pg = std::env::var("REPLAY_CAPACITY_TEST_PG_URL").ok();
 let schema = format!("replay_capacity_{}",uuid::Uuid::new_v4().simple());
 let admin = if let Some(url)=&pg { let pool=AnyPoolOptions::new().max_connections(2).connect(url).await.unwrap();sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&pool).await.unwrap();Some(pool)} else {None};
 let db = if let Some(url)=&pg {
  let owned_schema=schema.clone();
  AnyPoolOptions::new().max_connections(5).after_connect(move |conn,_| {let sql=format!("SET search_path TO {owned_schema}");Box::pin(async move{sqlx::query(&sql).execute(conn).await?;Ok(())})}).connect(url).await.unwrap()
 } else {
  AnyPoolOptions::new().max_connections(5).after_connect(|conn,_|Box::pin(async move{sqlx::query("PRAGMA busy_timeout=10000").execute(&mut *conn).await?;sqlx::query("PRAGMA journal_mode=WAL").execute(conn).await?;Ok(())})).connect(&format!("sqlite://{}?mode=rwc",root.join("fixture.sqlite").display())).await.unwrap()
 };
 if pg.is_some(){sqlx::migrate!("./migrations-postgres").run(&db).await.unwrap();}else{sqlx::migrate!("./migrations").run(&db).await.unwrap();}
 for sql in [
  "INSERT INTO users(id,email,display_name,role) VALUES('user-1','replay@test.invalid','Replay','member')",
  "INSERT INTO spaces(id,name,slug,created_by) VALUES('space-1','Replay','replay-slug','user-1')",
  "INSERT INTO space_members(id,space_id,user_id,role) VALUES('member-1','space-1','user-1','owner')",
  "INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('session-1','user-1','fixture','2999-01-01T00:00:00Z')",
  "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at,published_at) VALUES('doc-1','space-1','Replay','spaces/replay-slug/docs/original.md','user-1','user-1','source-revision','source-publication')"
 ]{sqlx::query(sql).execute(&db).await.unwrap();}
 std::fs::create_dir_all(root.join("spaces/replay-slug/docs")).unwrap();
 std::fs::write(root.join("spaces/replay-slug/docs/original.md"),b"![image](vault-image)").unwrap();
 let state=AppState { db, jwt_secret:"owned-fixture-secret".into(),base_url:"http://localhost".into(),search_index:crate::search::SearchIndex::new(root.join("index")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:WsRooms::default() };
 let token=crate::auth::jwt::create_token_pair_for_session("user-1","replay@test.invalid","member",&state.jwt_secret,Some("session-1"),30).unwrap().access_token;
 let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();
 let router=api_router().with_state(state.clone());let serving=router.clone();
 let server=tokio::spawn(async move{axum::serve(listener,serving).await.unwrap()});
 let client=reqwest::Client::new();let endpoint=format!("http://{address}/sync/replay");
 let original=operation("original-op","original-asset","vault-image");
 std::env::set_var("ATTACHMENT_STORAGE_QUOTA_BYTES","22");
 let response=client.post(&endpoint).bearer_auth(&token).json(&manifest(original.clone())).send().await.unwrap();assert_eq!(response.status(),200);let body:Value=response.json().await.unwrap();assert_eq!(body["applied_operation_count"],1,"{body}");
 let raw:String=sqlx::query_scalar("SELECT receipt_json FROM asset_replay_receipts").fetch_one(&state.db).await.unwrap();let receipt:AssetReplayReceipt=serde_json::from_str(&raw).unwrap();let attempt=receipt.admission_attempt_id.clone().unwrap();
 assert_eq!(std::fs::read(root.join(&receipt.storage_path)).unwrap(),b"ASSET BYTES");
 let reservation:(String,String,i64)=sqlx::query_as("SELECT state,committed_asset_id,committed_bytes FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&attempt).fetch_one(&state.db).await.unwrap();assert_eq!(reservation,("committed".into(),"original-asset".into(),11));
 let idle=crate::storage::attachment_upload::admission::WriterLease::open_idle(&root,&attempt).unwrap();assert_eq!(idle.maximum_bytes(),11);drop(idle);
 let capsule_count=std::fs::read_dir(root.join(".attachment-upload-admission")).unwrap().count();
 std::env::set_var("ATTACHMENT_STORAGE_QUOTA_BYTES","11");
 let retry:Value=client.post(&endpoint).bearer_auth(&token).json(&manifest(original.clone())).send().await.unwrap().json().await.unwrap();assert_eq!(retry["failed_operation_count"],0,"{retry}");assert_eq!(count(&state,"attachment_capacity_reservations").await,1);assert_eq!(std::fs::read_dir(root.join(".attachment-upload-admission")).unwrap().count(),capsule_count);
 let denied:Value=client.post(&endpoint).bearer_auth(&token).json(&manifest(operation("denied-op","denied-asset","vault-image"))).send().await.unwrap().json().await.unwrap();assert_eq!(denied["failed_operation_count"],1,"{denied}");assert_eq!(count(&state,"attachment_capacity_reservations").await,1);assert_eq!(count(&state,"attachments").await,1);assert_eq!(count(&state,"asset_replay_receipts").await,1);
 // Explicitly exercise the historical branch, rather than only the outer replay-operation skip.
 let claims=crate::auth::jwt::validate_token(&token,&state.jwt_secret,crate::auth::jwt::TokenType::Access).unwrap();let auth=AuthUser{claims,api_key_scopes:None};let op:SyncReplayOperation=serde_json::from_value(original).unwrap();replay_asset_create(&state,&auth,"self-hosted",&op).await.unwrap();assert_eq!(count(&state,"attachment_capacity_reservations").await,1);
 // Real cancellation of the actual Axum service future. TCP disconnect alone is not cancellation authority.
 std::env::set_var("ATTACHMENT_STORAGE_QUOTA_BYTES","22");
 let (started_tx,started_rx)=tokio::sync::oneshot::channel();
 REPLAY_ASSET_BEFORE_COMMIT.with(|s|*s.borrow_mut()=Some(Box::pin(async move{started_tx.send(()).unwrap();std::future::pending::<()>().await})));
 let request=axum::http::Request::builder().method("POST").uri("/sync/replay").header("authorization",format!("Bearer {token}")).header("content-type","application/json").body(axum::body::Body::from(serde_json::to_vec(&manifest(operation("cancel-op","cancel-asset",&format!("/api/v1/files/{}",receipt.storage_path)))).unwrap())).unwrap();
 let pending=tokio::spawn(async move{router.oneshot(request).await.unwrap()});
 tokio::time::timeout(std::time::Duration::from_secs(20),started_rx).await.unwrap().unwrap();
 let cancelled:String=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations WHERE state='reserved'").fetch_one(&state.db).await.unwrap();
 assert!(crate::storage::attachment_upload::admission::WriterLease::open_idle(&root,&cancelled).is_err());
 pending.abort();assert!(pending.await.unwrap_err().is_cancelled());
 let disposition=crate::storage::attachment_upload::admission::dispose_replay(&state.db,&root,&cancelled).await.unwrap();assert!(!disposition.is_empty());
 let status:String=sqlx::query_scalar("SELECT state FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&cancelled).fetch_one(&state.db).await.unwrap();assert_eq!(status,"disposed");assert_eq!(count(&state,"attachments").await,1);assert_eq!(count(&state,"asset_replay_receipts").await,1);
 server.abort();let _=server.await;state.db.close().await;
 if let Some(admin)=admin{sqlx::query(&format!("DROP SCHEMA {schema} CASCADE")).execute(&admin).await.unwrap();admin.close().await;}
 println!("PASS engine={} actual_tcp=true exact_committed_bytes=11 retry_reservations=0 quota_new_reservations=0 cancelled_axum_future_disposed=true",if pg.is_some(){"postgresql"}else{"sqlite"});
}

#[test]
fn legacy_asset_receipt_missing_attempt_reserializes_byte_exact() {
 let raw=r#"{"incoming":{"provider":"self-hosted","operation_id":"original","entity_type":"asset","entity_id":"asset","action":"create","payload_fingerprint":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"actor_id":"user","space_id":"space","document_id":"doc","storage_path":"spaces/slug/attachments/doc/asset.png","filename":"asset.png","original_name":"Original.png","mime_type":"image/png","created_at":"2026-10-11T00:00:00Z","byte_length":11,"binary_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","source_kind":"unchanged","source_id":null,"source_revision":null,"body_sha256":null}"#;
 let receipt:AssetReplayReceipt=serde_json::from_str(raw).unwrap();assert!(receipt.admission_attempt_id.is_none());assert_eq!(serde_json::to_string(&receipt).unwrap(),raw);
}
