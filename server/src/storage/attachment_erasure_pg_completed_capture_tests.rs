 use super::*;use crate::storage::attachment_upload::admission::{WriterLease,Producer};use axum::{body::Body,extract::FromRequestParts,http::Request};use crate::{AppState,storage};use serde_json::{json,Value};use sqlx::any::AnyPoolOptions;use sqlx::Connection;
 #[tokio::test(flavor="current_thread")]
 #[ignore="requires private disposable dual-engine fixture and owns actual TCP"]
 async fn actual_foreign_space_draft_writer_waits_across_guarded_scan(){actual_completed_fixture(false).await}
 #[tokio::test]
 #[ignore="isolated actual standard completed source capture"]
 async fn actual_standard_pg_completed_capture(){actual_completed_fixture(true).await}
 #[tokio::test]
 #[ignore="fresh process on authentic closed completed PG target"]
 async fn actual_fresh_pg_completed_usage(){sqlx::any::install_default_drivers();let root=storage::data_dir();let url=std::env::var("REPLAY_CAPACITY_TEST_PG_URL").unwrap();let serving=storage::restoration::linux::open_restored_pg_serving(&root,&url).await.unwrap();let pool=storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&serving);let report=storage::attachment_capacity::usage_report(&pool).await.unwrap();assert_eq!((report.retained_bytes,report.logical_retained_bytes,report.pending_bytes),(0,0,0));if std::env::var("MIXED_COMPLETED_TRANSPORT_TEST").as_deref()==Ok("true"){super::restored_native_tests::actual_fresh_erased_read_probes(&root,&pool).await;}pool.close().await;println!("ACTUAL_FRESH_PG_COMPLETED_USAGE_ZERO_WITH_FINALIZED_ABSENCE_PASS");}
 async fn actual_completed_fixture(completed_capture_only:bool){
 let _=tracing_subscriber::fmt().with_max_level(tracing::Level::ERROR).with_test_writer().try_init();sqlx::any::install_default_drivers();let root=storage::data_dir();let pg=Some(std::env::var("REPLAY_CAPACITY_TEST_PG_URL").expect("Protected owned PG fixture required"));let schema=String::new();let admin:Option<sqlx::AnyPool>=None;
 assert!(completed_capture_only);assert!(!root.exists(),"Fresh native qualified source only");
 let seed=root.with_file_name(format!("completed-pg-seed-{}",uuid::Uuid::new_v4().simple()));std::fs::create_dir_all(&seed).unwrap();use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&seed,std::fs::Permissions::from_mode(0o700)).unwrap();
 let seed_db=AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=rwc",seed.join("starlane-notes.db").display())).await.unwrap();sqlx::migrate!("./migrations").run(&seed_db).await.unwrap();seed_db.close().await;std::fs::set_permissions(seed.join("starlane-notes.db"),std::fs::Permissions::from_mode(0o600)).unwrap();
 let seed_bundle=root.with_file_name(format!("completed-pg-seed-bundle-{}",uuid::Uuid::new_v4().simple()));let held=storage::restoration::linux::capture_sqlite_certified(&seed,&seed_bundle,"1.5.3",true).await.unwrap();held.db.close().await;drop(held);
 let transfer=root.with_file_name(format!("completed-pg-transfer-{}",uuid::Uuid::new_v4().simple()));storage::backup_transfer::run(&seed_bundle,&root,&transfer,"1.5.3",pg.as_ref().unwrap()).await.unwrap();
 let qualified=storage::restoration::linux::open_engine_serving(&root,pg.as_ref().unwrap()).await.unwrap();let db=storage::restoration::linux::engine_transfer::pg_completed_test_native_pool(&qualified);
 if std::env::var("QUALIFIED_ERASURE_CUSTODY_CONTROL").as_deref()==Ok("foreign_equal_bytes") {
  let mut tx=db.begin().await.unwrap();
  let proof=storage::restoration::qualified_erasure_source_in(&root,&mut tx).await.unwrap().unwrap();proof.fence_in(&mut tx).await.unwrap();
  let unregistered=root.with_file_name(format!("unregistered-erasure-root-{}",uuid::Uuid::new_v4().simple()));std::fs::create_dir(&unregistered).unwrap();std::fs::set_permissions(&unregistered,std::fs::Permissions::from_mode(0o700)).unwrap();
  assert!(storage::restoration::qualified_erasure_source_in(&unregistered,&mut tx).await.unwrap().is_none(),"Unregistered root minted qualified custody");
  let mut foreign=sqlx::AnyConnection::connect(&format!("sqlite://{}?mode=rw",seed.join("starlane-notes.db").display())).await.unwrap();
  assert!(proof.fence_in(&mut foreign).await.is_err(),"Different connected engine admitted original source proof");foreign.close().await.unwrap();
  let marker=root.join(".engine-transfer-custody/current.json");let raw=std::fs::read(&marker).unwrap();let kept=marker.with_file_name("control-original-held.json");std::fs::rename(&marker,&kept).unwrap();std::fs::write(&marker,&raw).unwrap();std::fs::set_permissions(&marker,std::fs::Permissions::from_mode(0o600)).unwrap();
  assert!(proof.fence().is_err(),"Equal-byte foreign original control inode admitted");assert!(storage::restoration::qualified_erasure_source_in(&root,&mut tx).await.is_err(),"Foreign current custody minted replacement token");
  let foreign_kept=marker.with_file_name("control-foreign-held.json");std::fs::rename(&marker,&foreign_kept).unwrap();std::fs::rename(&kept,&marker).unwrap();assert!(proof.fence().is_err(),"Rename-back stamp change laundered original custody");
  tx.rollback().await.unwrap();db.close().await;drop(qualified);println!("ACTUAL_QUALIFIED_ORIGINAL_SOURCE_UNREGISTERED_FOREIGN_ENGINE_EQUAL_BYTE_INODE_RENAMEBACK_REFUSALS_PASS");return;
 }

 for sql in ["INSERT INTO users(id,email,display_name,role) VALUES('owner','owner@test.invalid','Owner','member'),('viewer','viewer@test.invalid','Viewer','member')","INSERT INTO spaces(id,name,slug,created_by) VALUES('space','Outbox','outbox','owner')","INSERT INTO space_members(id,space_id,user_id,role) VALUES('owner-member','space','owner','owner'),('viewer-member','space','viewer','viewer')","INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('owner-session','owner','owner-refresh','2999-01-01T00:00:00Z'),('viewer-session','viewer','viewer-refresh','2999-01-01T00:00:00Z')"]{sqlx::query(sql).execute(&db).await.unwrap();}
 for sql in ["INSERT INTO spaces(id,name,slug,created_by) VALUES('foreign-space','Foreign','foreign','owner')","INSERT INTO space_members(id,space_id,user_id,role) VALUES('foreign-owner','foreign-space','owner','owner')"]{sqlx::query(sql).execute(&db).await.unwrap();}
 let state=AppState{db,jwt_secret:"owned-erasure-fixture".into(),base_url:"http://localhost".into(),search_index:crate::search::SearchIndex::new(root.join(if completed_capture_only{"search_index"}else{"index"})).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};
 let token=crate::auth::jwt::create_token_pair_for_session("owner","owner@test.invalid","member",&state.jwt_secret,Some("owner-session"),30).unwrap().access_token;let viewer=crate::auth::jwt::create_token_pair_for_session("viewer","viewer@test.invalid","member",&state.jwt_secret,Some("viewer-session"),30).unwrap().access_token;
 let app=crate::spaces::documents::router().merge(crate::spaces::drafts::router()).merge(crate::spaces::attachments::router()).merge(crate::storage::attachment_versions::router()).merge(crate::auth::routes::router()).with_state(state.clone());let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap()});let base=format!("http://{address}");let client=reqwest::Client::new();std::env::remove_var("ATTACHMENT_STORAGE_QUOTA_BYTES");std::env::set_var("MAX_UPLOAD_SIZE_MB","1");

 let request=Request::builder().uri("/private-proof").header("Authorization",format!("Bearer {token}")).body(Body::empty()).unwrap();let(mut parts,_)=request.into_parts();let owner_auth=crate::auth::middleware::AuthUser::from_request_parts(&mut parts,&state).await.unwrap();
 let request=Request::builder().uri("/private-proof").header("Authorization",format!("Bearer {viewer}")).body(Body::empty()).unwrap();let(mut parts,_)=request.into_parts();let viewer_auth=crate::auth::middleware::AuthUser::from_request_parts(&mut parts,&state).await.unwrap();
 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();fenced.original_human(&owner_auth,"owner","space").await.unwrap().check_now().unwrap();
  assert!(fenced.original_human(&owner_auth,"foreign-actor","space").await.is_err());assert!(fenced.original_human(&viewer_auth,"viewer","space").await.is_err());fenced.tx.rollback().await.unwrap();}
 // Original session expires while no topology/session writer can change the held
 // transaction; time itself must be checked again at the native mutation boundary.
 sqlx::query("UPDATE sessions SET expires_at=$1 WHERE id='owner-session'").bind((chrono::Utc::now()+chrono::Duration::milliseconds(300)).to_rfc3339()).execute(&state.db).await.unwrap();
 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();let proof=fenced.original_human(&owner_auth,"owner","space").await.unwrap();tokio::time::sleep(std::time::Duration::from_millis(350)).await;assert!(proof.check_now().is_err());drop(proof);fenced.tx.rollback().await.unwrap();}
 sqlx::query("UPDATE sessions SET expires_at='2999-01-01T00:00:00Z' WHERE id='owner-session'").execute(&state.db).await.unwrap();
 sqlx::query("UPDATE users SET is_active=0 WHERE id='owner'").execute(&state.db).await.unwrap();
 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();assert!(fenced.original_human(&owner_auth,"owner","space").await.is_err());fenced.tx.rollback().await.unwrap();}
 sqlx::query("UPDATE users SET is_active=1 WHERE id='owner'").execute(&state.db).await.unwrap();
 // A real logout runs on a separate HTTP worker/connection. It cannot delete
 // the original session while the private current-human transaction holds it.
 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();let proof=fenced.original_human(&owner_auth,"owner","space").await.unwrap();
  let client2=client.clone();let base2=base.clone();let token2=token.clone();let logout=tokio::spawn(async move{client2.post(format!("{base2}/auth/logout")).bearer_auth(token2).send().await.unwrap()});
  tokio::time::sleep(std::time::Duration::from_millis(150)).await;assert!(!logout.is_finished(),"Original-session logout crossed held current-human row fence");proof.check_now().unwrap();drop(proof);fenced.tx.rollback().await.unwrap();assert_eq!(logout.await.unwrap().status(),204);}
 // Logout-before authority admission must refuse the old extracted credential,
 // even though its original signed JWT has not yet expired.
 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();assert!(fenced.original_human(&owner_auth,"owner","space").await.is_err());fenced.tx.rollback().await.unwrap();}
 sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('owner-session','owner','owner-refresh','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
 let response=client.post(format!("{base}/spaces/space/documents")).bearer_auth(&token).json(&json!({"source_id":"winner","title":"Winner","content":"ORIGINAL WINNER","property_values":[]})).send().await.unwrap();let status=response.status();let page:Value=response.json().await.unwrap();assert_eq!(status,201,"{page}");
 let response=client.post(format!("{base}/spaces/space/documents/winner/drafts/{}/publish",page["draft_id"].as_str().unwrap())).bearer_auth(&token).json(&json!({})).send().await.unwrap();assert!(response.status().is_success());
 let upload_operation=uuid::Uuid::new_v4().to_string();let boundary="original-erasure-winner-boundary";
 let body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"Original.bin\"\r\nContent-Type: application/octet-stream\r\n\r\nORIGINAL NATIVE WINNER\r\n--{boundary}--\r\n");
 let response=client.post(format!("{base}/spaces/space/documents/winner/attachments")).bearer_auth(&token).header("x-starlane-upload-operation",&upload_operation).header("content-type",format!("multipart/form-data; boundary={boundary}")).body(body).send().await.unwrap();let status=response.status();let uploaded:Value=response.json().await.unwrap();assert_eq!(status,201,"{uploaded}");let asset=uploaded["id"].as_str().unwrap().to_string();
 let mut conn=state.db.acquire().await.unwrap();let original=crate::storage::attachment_upload::receipt(&mut conn,&upload_operation).await.unwrap().unwrap();drop(conn);
 let raw_before:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&upload_operation).fetch_one(&state.db).await.unwrap();
 assert!(crate::storage::attachment_upload::admission::dispose_direct(&state.db,&root,&original.intent.attempt_id).await.is_err(),"Reserved disposer accepted real production committed winner");
 crate::storage::attachment_upload::verify(&root,&original,false).unwrap();
 let raw_after:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&upload_operation).fetch_one(&state.db).await.unwrap();assert_eq!(raw_after,raw_before);
 let reservation:String=sqlx::query_scalar("SELECT state FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&original.intent.attempt_id).fetch_one(&state.db).await.unwrap();assert_eq!(reservation,"committed");
 assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_version_records WHERE version_id=$1").bind(&asset).fetch_one(&state.db).await.unwrap(),1);
 let selected=client.get(format!("{base}/spaces/space/documents/winner/attachment-versions/{asset}")).bearer_auth(&token).send().await.unwrap();assert_eq!(selected.status(),200);

 let families=std::env::var("COMPLETED_PG_FAMILIES_TEST").as_deref()==Ok("replacement_retired_multi");let mut all_originals=vec![original.clone()];
 if families {
  let replacement_operation=uuid::Uuid::new_v4().to_string();let replacement_body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"Replacement.bin\"\r\nContent-Type: application/octet-stream\r\n\r\nREPLACEMENT NATIVE WINNER\r\n--{boundary}--\r\n");
  let r=client.post(format!("{base}/spaces/space/documents/winner/attachment-families/{asset}/versions")).bearer_auth(&token).header("x-starlane-upload-operation",&replacement_operation).header("x-starlane-attachment-version",&asset).header("x-starlane-attachment-revision","1").header("content-type",format!("multipart/form-data; boundary={boundary}")).body(replacement_body).send().await.unwrap();let status=r.status();let value:Value=r.json().await.unwrap();assert_eq!(status,201,"{value}");
  let mut conn=state.db.acquire().await.unwrap();let replacement=storage::attachment_upload::receipt(&mut conn,&replacement_operation).await.unwrap().unwrap();drop(conn);
  assert_eq!(client.delete(format!("{base}/spaces/space/documents/winner/attachments/{}",replacement.intent.asset_id)).bearer_auth(&token).send().await.unwrap().status(),204);all_originals.push(replacement);
  let other_operation=uuid::Uuid::new_v4().to_string();let other_body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"Other.bin\"\r\nContent-Type: application/octet-stream\r\n\r\nOTHER ORIGINAL NATIVE WINNER\r\n--{boundary}--\r\n");
  let r=client.post(format!("{base}/spaces/space/documents/winner/attachments")).bearer_auth(&token).header("x-starlane-upload-operation",&other_operation).header("content-type",format!("multipart/form-data; boundary={boundary}")).body(other_body).send().await.unwrap();let status=r.status();let value:Value=r.json().await.unwrap();assert_eq!(status,201,"{value}");
  let mut conn=state.db.acquire().await.unwrap();let other=storage::attachment_upload::receipt(&mut conn,&other_operation).await.unwrap().unwrap();drop(conn);assert_eq!(client.delete(format!("{base}/spaces/space/documents/winner/attachments/{}",other.intent.asset_id)).bearer_auth(&token).send().await.unwrap().status(),204);all_originals.push(other);
 }
 let response=client.post(format!("{base}/spaces/foreign-space/documents")).bearer_auth(&token).json(&json!({"source_id":"foreign-reference","title":"Foreign","content":"","property_values":[]})).send().await.unwrap();
 let status=response.status();let page:Value=response.json().await.unwrap();assert_eq!(status,201,"{page}");let draft=page["draft_id"].as_str().unwrap();
 let endpoint=format!("{base}/spaces/foreign-space/documents/foreign-reference/drafts/{draft}");
 let current:Value=client.get(&endpoint).bearer_auth(&token).send().await.unwrap().json().await.unwrap();
 let protected=format!("[file:Original.bin](starlane-attachment://{asset})");
 let mut held=state.db.begin().await.unwrap();let scopes=lock_every_space(&mut held).await.unwrap();assert_eq!(scopes,vec!["foreign-space","space"]);
 let before:String=sqlx::query_scalar("SELECT content FROM document_drafts WHERE id=$1").bind(draft).fetch_one(&mut *held).await.unwrap();assert!(!before.contains(&asset));
 let client2=client.clone();let endpoint2=endpoint.clone();let token2=token.clone();let content2=protected.clone();
 let writer=tokio::spawn(async move{client2.put(endpoint2).bearer_auth(token2).json(&json!({"source_digest":current["source_digest"],"content":content2})).send().await.unwrap()});
 tokio::time::sleep(std::time::Duration::from_millis(150)).await;assert!(!writer.is_finished(),"Foreign-space draft writer crossed held global fence");
 let during:String=sqlx::query_scalar("SELECT content FROM document_drafts WHERE id=$1").bind(draft).fetch_one(&mut *held).await.unwrap();assert_eq!(during,before);
 // This proves the actual writer exclusion seam only. No physical mutation or
 // current-human erasure grant is implied by this generated possible reference.
 held.rollback().await.unwrap();let response=writer.await.unwrap();let status=response.status();let result:Value=response.json().await.unwrap();assert_eq!(status,200,"{result}");
 let after:String=sqlx::query_scalar("SELECT content FROM document_drafts WHERE id=$1").bind(draft).fetch_one(&state.db).await.unwrap();assert_eq!(after,protected);
 // Publication is a distinct real writer: it must also remain behind the
 // same global fence, rather than infer coverage from successful draft saving.
 let mut held=state.db.begin().await.unwrap();lock_every_space(&mut held).await.unwrap();
 let client2=client.clone();let endpoint2=format!("{endpoint}/publish");let token2=token.clone();
 let publisher=tokio::spawn(async move{client2.post(endpoint2).bearer_auth(token2).json(&json!({})).send().await.unwrap()});
 tokio::time::sleep(std::time::Duration::from_millis(150)).await;assert!(!publisher.is_finished(),"Foreign-space publication crossed held global fence");
 held.rollback().await.unwrap();let response=publisher.await.unwrap();let status=response.status();let result:Value=response.json().await.unwrap();assert_eq!(status,200,"{result}");
 let deletion=uuid::Uuid::new_v4().to_string();
 assert!(client.delete(format!("{base}/spaces/space/documents/winner")).bearer_auth(&token).send().await.unwrap().status().is_success());
 let witness:(String,Option<String>)=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE id='winner'").fetch_one(&state.db).await.unwrap();
 let response=client.delete(format!("{base}/spaces/space/documents/winner/trash")).bearer_auth(&token).header("x-starlane-delete-operation",&deletion).json(&json!({"original_target":{"created_at":witness.0,"deleted_at":witness.1}})).send().await.unwrap();assert_eq!(response.status(),204);
 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();assert!(fenced.prepare_one(&owner_auth,&deletion,&original).await.is_err(),"Protected foreign published pin allowed erasure");fenced.tx.rollback().await.unwrap();}
 assert!(root.join(&original.intent.storage_path).exists());
 assert!(client.delete(format!("{base}/spaces/foreign-space/documents/foreign-reference")).bearer_auth(&token).send().await.unwrap().status().is_success());
 let foreign_witness:(String,Option<String>)=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE id='foreign-reference'").fetch_one(&state.db).await.unwrap();
 assert_eq!(client.delete(format!("{base}/spaces/foreign-space/documents/foreign-reference/trash")).bearer_auth(&token).header("x-starlane-delete-operation",uuid::Uuid::new_v4().to_string()).json(&json!({"original_target":{"created_at":foreign_witness.0,"deleted_at":foreign_witness.1}})).send().await.unwrap().status(),204);
 // Legacy format1 custody cannot stand in for the format2 original incarnation/trash witness.
 let plan_original:String=sqlx::query_scalar("SELECT plan_json FROM attachment_erasure_outbox WHERE operation_id=$1").bind(&deletion).fetch_one(&state.db).await.unwrap();
 let mut legacy:Value=serde_json::from_str(&plan_original).unwrap();legacy["format_version"]=json!(1);legacy.as_object_mut().unwrap().remove("original_target");let legacy_raw=serde_json::to_string(&legacy).unwrap();
 sqlx::query("UPDATE attachment_erasure_outbox SET plan_json=$1,plan_sha256=$2 WHERE operation_id=$3").bind(&legacy_raw).bind(crate::storage::attachment_upload::digest(legacy_raw.as_bytes())).bind(&deletion).execute(&state.db).await.unwrap();
 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();assert!(fenced.prepare_one(&owner_auth,&deletion,&original).await.is_err(),"Legacy witnessless plan authorized physical erasure");fenced.tx.rollback().await.unwrap();}
 sqlx::query("UPDATE attachment_erasure_outbox SET plan_json=$1,plan_sha256=$2 WHERE operation_id=$3").bind(&plan_original).bind(crate::storage::attachment_upload::digest(plan_original.as_bytes())).bind(&deletion).execute(&state.db).await.unwrap();
 assert!(FencedTransaction::begin(&state.db).await.unwrap().finalize_existing(&owner_auth,&deletion,&original).await.is_err(),"Incomplete physical paths created disposition");
 let count:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_erasure_dispositions").fetch_one(&state.db).await.unwrap();assert_eq!(count,0);

 let cleanup_endpoint=format!("{base}/spaces/space/documents/winner/permanent-deletions/{deletion}/cleanup");
 let cleanup_payload=json!({"original_target":serde_json::from_str::<Value>(&plan_original).unwrap()["original_target"]});
 let denied=client.post(&cleanup_endpoint).bearer_auth(&viewer).json(&cleanup_payload).send().await.unwrap();assert_eq!(denied.status(),401,"Observer cannot invoke original cleanup");
 let first=client.post(&cleanup_endpoint).bearer_auth(&token).json(&cleanup_payload).send().await.unwrap();let status=first.status();let outcome:Value=first.json().await.unwrap();assert_eq!(status,200,"{outcome}");assert_eq!(outcome["outcome"],"physically_completed","{outcome}");assert!(outcome["assets"].as_array().unwrap().iter().all(|a|a["outcome"]=="physically_completed"));
 assert!(!root.join(&original.intent.storage_path).exists());
 let raw_after_erasure:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&upload_operation).fetch_one(&state.db).await.unwrap();assert_eq!(raw_after_erasure,raw_before);
 let disposition_before:String=sqlx::query_scalar("SELECT completed_at FROM attachment_erasure_dispositions WHERE asset_id=$1").bind(&asset).fetch_one(&state.db).await.unwrap();
 let repeat=client.post(&cleanup_endpoint).bearer_auth(&token).json(&cleanup_payload).send().await.unwrap();let status=repeat.status();let repeat:Value=repeat.json().await.unwrap();assert_eq!(status,200,"{repeat}");assert_eq!(repeat["outcome"],"physically_completed");
 let disposition_after:String=sqlx::query_scalar("SELECT completed_at FROM attachment_erasure_dispositions WHERE asset_id=$1").bind(&asset).fetch_one(&state.db).await.unwrap();assert_eq!(disposition_before,disposition_after,"Endpoint retry rewrote original completion");
 // Full response is re-observed against original current human after actual logout.
 let(arrived,release)=super::cleanup_consumer::pause_final_observation(&deletion);let c=client.clone();let ep=cleanup_endpoint.clone();let tk=token.clone();let body=cleanup_payload.clone();let pending=tokio::spawn(async move{c.post(ep).bearer_auth(tk).json(&body).send().await.unwrap()});arrived.await.unwrap();
 assert_eq!(client.post(format!("{base}/auth/logout")).bearer_auth(&token).send().await.unwrap().status(),204);release.send(()).unwrap();assert_eq!(pending.await.unwrap().status(),401,"Completed observation disclosed after original logout");
 sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('owner-session','owner','owner-refresh','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
 let(arrived,release)=super::cleanup_consumer::pause_final_observation(&deletion);let c=client.clone();let ep=cleanup_endpoint.clone();let tk=token.clone();let body=cleanup_payload.clone();let pending=tokio::spawn(async move{c.post(ep).bearer_auth(tk).json(&body).send().await.unwrap()});arrived.await.unwrap();
 sqlx::query("UPDATE sessions SET expires_at='2000-01-01T00:00:00Z' WHERE id='owner-session'").execute(&state.db).await.unwrap();release.send(()).unwrap();assert_eq!(pending.await.unwrap().status(),401,"Completed observation disclosed after original session expiry");sqlx::query("UPDATE sessions SET expires_at='2999-01-01T00:00:00Z' WHERE id='owner-session'").execute(&state.db).await.unwrap();
 println!("ACTUAL_QUALIFIED_PG_NORMAL_CLEANUP_ENDPOINT_ORIGINAL_OBSERVER_EXACT_RETRY_DELAYED_LOGOUT_EXPIRY_PASS");
 if completed_capture_only && pg.is_some() {
  server.abort();let _=server.await;state.db.close().await;drop(qualified);
  let serving=storage::restoration::linux::open_engine_serving(&root,pg.as_ref().unwrap()).await.unwrap();
  let target=root.with_file_name(format!("completed-pg-backup-{}",uuid::Uuid::new_v4().simple()));
  let backup=storage::restoration::linux::engine_transfer::capture_owned_postgres(&serving,&target,true).await.unwrap();backup.fence().unwrap();
  let manifest:Value=serde_json::from_slice(&std::fs::read(target.join("manifest.json")).unwrap()).unwrap();assert_eq!(manifest["completed_absence_capture"]["scopes"].as_array().unwrap().len(),all_originals.len());
  storage::restoration::linux::engine_transfer::pg_completed_test_registry_controls(&target,&storage::restoration::linux::engine_transfer::pg_completed_test_native_pool(&serving)).await.unwrap();
  storage::restoration::linux::engine_transfer::pg_completed_test_native_pool(&serving).close().await;drop(serving);let kept=root.with_file_name(format!("completed-pg-original-source-kept-{}",uuid::Uuid::new_v4().simple()));std::fs::rename(&root,&kept).unwrap();assert!(!root.exists());
  let restored=root.with_file_name(format!("completed-pg-finalized-restored-{}",uuid::Uuid::new_v4().simple()));
  storage::restoration::linux::engine_transfer::restore_qualified_postgres(&target,&restored,pg.as_ref().unwrap(),true).await.unwrap();
  let restored_serving=storage::restoration::linux::open_restored_pg_serving(&restored,pg.as_ref().unwrap()).await.unwrap();
  let mut tx=storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&restored_serving).begin().await.unwrap();super::super::accounting::lock_global_in(&mut tx).await.unwrap();
  let paths=storage::attachment_capacity::retained_paths_in(&mut tx).await.unwrap();let absence=super::super::accounting::capture_in_at(&restored,&mut tx,paths,&[]).await.unwrap();assert!(all_originals.iter().all(|r|absence.matches(r)));absence.fence().unwrap();
  assert_eq!(storage::attachment_capacity::test_completed_logical_charge_in(&mut tx,absence).await.unwrap(),0);assert!(WriterLease::open_idle(&restored,&original.intent.attempt_id).is_err());
  assert!(all_originals.iter().all(|r|!restored.join(&r.intent.storage_path).exists()));
  tx.rollback().await.unwrap();
  if std::env::var("RESTORED_NATIVE_MIXED_TEST").as_deref()==Ok("true") {let pool=storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&restored_serving);let native=super::restored_native_tests::actual_lifecycle(&restored,&pool,&all_originals).await;pool.close().await;drop(restored_serving);
  if std::env::var("PENDING_COMPATIBILITY_CAPTURE_TEST").as_deref()==Ok("true"){
   let source=storage::restoration::linux::open_restored_pg_serving(&restored,pg.as_ref().unwrap()).await.unwrap();let target=restored.with_file_name(format!("pending-disposed-pg-backup-{}",uuid::Uuid::new_v4().simple()));let result=storage::restoration::linux::engine_transfer::capture_restored_postgres(&source,&target,true).await;
   if std::env::var("COMPOSITE_ADMISSION_SOURCE_TEST").as_deref()==Ok("true"){
    let backup=result.unwrap();backup.fence().unwrap();let manifest:Value=serde_json::from_slice(&std::fs::read(target.join("manifest.json")).unwrap()).unwrap();let evidence=serde_json::from_value::<storage::restoration::composite::CompositeCaptureEvidence>(manifest["composite_admission_capture"].clone()).unwrap();evidence.validate_shape().unwrap();assert_eq!(evidence.native.len(),4);assert_eq!(evidence.prior.roles.scopes.len(),3);
    let pool=storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&source);let expected=super::restored_native_tests::compatibility_attachment_rows(&restored,&pool).await;pool.close().await;drop(source);drop(backup);
    let kept=restored.with_file_name(format!("composite-original-pg-kept-{}",uuid::Uuid::new_v4().simple()));std::fs::rename(&restored,&kept).unwrap();assert!(!restored.exists());let cold=restored.with_file_name(format!("composite-ledger4-pg-cold-{}",uuid::Uuid::new_v4().simple()));storage::restoration::linux::engine_transfer::restore_qualified_postgres(&target,&cold,pg.as_ref().unwrap(),true).await.unwrap();std::env::set_var("DATA_DIR",&cold);let fresh=storage::restoration::linux::open_restored_pg_serving(&cold,pg.as_ref().unwrap()).await.unwrap();let pool=storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&fresh);let material:Value=serde_json::from_slice(&std::fs::read(cold.parent().unwrap().join("pending-compatibility-retained-material.json")).unwrap()).unwrap();super::restored_native_tests::actual_composite_cold_cycles(&cold,&pool,&expected,&material).await;pool.close().await;drop(fresh);println!("ACTUAL_STANDARD_PG_COMPOSITE_SOURCE_ABSENT_LEDGER4_COLD_FULL_ROWS_NATIVE_ROSTER_AND_FRESH_ACCOUNTING_PASS");println!("ACTUAL_COMPOSITE_PG_BACKUP={}",target.display());println!("ACTUAL_COMPOSITE_PG_COLD={}",cold.display());return;
   }else{assert!(result.is_err(),"Unexpected unqualified composite capture success");println!("ACTUAL_STANDARD_PENDING_DISPOSED_COMPOSITE_REFUSAL={}",result.err().unwrap());storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&source).close().await;}
  }
  if std::env::var("MIXED_COMPLETED_TRANSPORT_TEST").as_deref()==Ok("true"){
   let mut originals=all_originals.clone();originals.extend(native);let mut current=restored;
   let mut original_scope_bytes:Option<Vec<Value>>=None;
   for cycle in 0..2{
    std::env::set_var("DATA_DIR",&current);let current_serving=storage::restoration::linux::open_restored_pg_serving(&current,pg.as_ref().unwrap()).await.unwrap();
    let target=current.with_file_name(format!("mixed-completed-pg-backup-{}",uuid::Uuid::new_v4().simple()));let backup=storage::restoration::linux::engine_transfer::capture_restored_postgres(&current_serving,&target,true).await.unwrap();backup.fence().unwrap();
    let manifest:Value=serde_json::from_slice(&std::fs::read(target.join("manifest.json")).unwrap()).unwrap();assert!(manifest.get("completed_absence_capture").is_none()&&manifest.get("admission_owner_capture").is_none()&&manifest.get("prior_admission_owner_capture").is_none());
    if cycle==0{let mixed=serde_json::from_value::<storage::restoration::MixedCompletedCaptureEvidence>(manifest["mixed_completed_absence_capture"].clone()).expect("Actual root PG writer must select explicit mixed evidence");mixed.validate_shape().unwrap();assert_eq!(mixed.prior.roles.scopes.len(),all_originals.len());assert_eq!(mixed.native.scopes.len(),originals.len()-all_originals.len());let envelope=mixed.envelope().unwrap();original_scope_bytes=Some(envelope.scopes.iter().map(|s|serde_json::to_value(s).unwrap()).collect());assert!(manifest.get("prior_completed_absence_capture").is_none());}
    else{let prior=serde_json::from_value::<storage::restoration::PriorCompletedCaptureEvidence>(manifest["prior_completed_absence_capture"].clone()).unwrap();prior.validate_shape().unwrap();assert_eq!(prior.roles.version,2);assert_eq!(prior.roles.scopes.iter().map(|s|serde_json::to_value(&s.original).unwrap()).collect::<Vec<_>>(),*original_scope_bytes.as_ref().unwrap());assert!(manifest.get("mixed_completed_absence_capture").is_none());}
    if cycle==0&&std::env::var("MIXED_COMPLETED_COLD_CONTROLS").as_deref()==Ok("true"){super::restored_native_tests::actual_mixed_cold_controls(&target,pg.as_deref()).await;}
    storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&current_serving).close().await;drop(current_serving);drop(backup);
    let kept=current.with_file_name(format!("mixed-completed-pg-source-kept-{}",uuid::Uuid::new_v4().simple()));std::fs::rename(&current,&kept).unwrap();assert!(!current.exists());
    let next=current.with_file_name(format!("mixed-completed-pg-cold-{}",uuid::Uuid::new_v4().simple()));storage::restoration::linux::engine_transfer::restore_qualified_postgres(&target,&next,pg.as_ref().unwrap(),true).await.unwrap();std::env::set_var("DATA_DIR",&next);
    let serving=storage::restoration::linux::open_restored_pg_serving(&next,pg.as_ref().unwrap()).await.unwrap();let pool=storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&serving);let report=storage::attachment_capacity::usage_report(&pool).await.unwrap();assert_eq!((report.retained_bytes,report.logical_retained_bytes,report.pending_bytes),(0,0,0));
    let mut tx=pool.begin().await.unwrap();let paths=storage::attachment_capacity::retained_paths_in(&mut tx).await.unwrap();let owner=super::super::accounting::capture_in_at(&next,&mut tx,paths,&[]).await.unwrap();assert!(originals.iter().all(|r|owner.matches(r)&&!next.join(&r.intent.storage_path).exists()&&WriterLease::open_idle(&next,&r.intent.attempt_id).is_err()));owner.fence().unwrap();drop(owner);tx.rollback().await.unwrap();
    let mut tx=pool.begin().await.unwrap();let held=storage::restoration::FinalizedCompletedCapture::hold(&next,&mut tx).await.unwrap();let e=held.evidence().unwrap();assert_eq!(e.roles.version,2);let raw=serde_json::to_value(&e.roles).unwrap();for mode in ["missing","null","legacy_relabel","source_root","source_role","adopted_substitution"]{let mut bad=raw.clone();match mode{"missing"=>{bad["scopes"][0].as_object_mut().unwrap().remove("source_basis");},"null"=>bad["scopes"][0]["source_basis"]=Value::Null,"legacy_relabel"=>bad["version"]=json!(1),"source_root"=>bad["scopes"][0]["source_basis"]["root"]["inode"]=json!(0),"source_role"=>{bad["scopes"][0]["source_basis"]["directories"].as_array_mut().unwrap().pop();},"adopted_substitution"=>bad["scopes"][0]["directories"][0]["adopted"]["inode"]=json!(0),_=>unreachable!()};let parsed=serde_json::from_value::<storage::restoration::completed::CompletedOwnerEnvelope>(bad);assert!(parsed.is_err()||parsed.unwrap().validate(&e.source_root).is_err(),"V2 {mode} minted complete PG owner roles");}held.recheck_in(&mut tx).await.unwrap();drop(held);tx.rollback().await.unwrap();pool.close().await;drop(serving);
    let fresh=std::process::Command::new(std::env::current_exe().unwrap()).env("DATA_DIR",&next).args(["storage::attachment_upload::admission::committed_erasure::coordinator::pg_completed_capture_tests::actual_fresh_pg_completed_usage","--exact","--ignored","--nocapture","--test-threads=1"]).output().unwrap();assert!(fresh.status.success(),"Fresh mixed PG accounting failed");assert!(!String::from_utf8_lossy(&fresh.stdout).contains("running 0 tests"));current=next;
   }
   if std::env::var("MIXED_COMPLETED_THIRD_NATIVE_CYCLE").as_deref()==Ok("true"){
    std::env::set_var("RESTORED_NATIVE_DOCUMENT_ID","restored-native-owner-v2-new");std::env::set_var("DATA_DIR",&current);let serving=storage::restoration::linux::open_restored_pg_serving(&current,pg.as_ref().unwrap()).await.unwrap();let pool=storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&serving);let new_native=super::restored_native_tests::actual_lifecycle(&current,&pool,&originals).await;pool.close().await;drop(serving);originals.extend(new_native);
    let serving=storage::restoration::linux::open_restored_pg_serving(&current,pg.as_ref().unwrap()).await.unwrap();let target=current.with_file_name(format!("mixed-owner-v2-new-native-pg-backup-{}",uuid::Uuid::new_v4().simple()));let backup=storage::restoration::linux::engine_transfer::capture_restored_postgres(&serving,&target,true).await.unwrap();backup.fence().unwrap();let manifest:Value=serde_json::from_slice(&std::fs::read(target.join("manifest.json")).unwrap()).unwrap();let mixed=serde_json::from_value::<storage::restoration::MixedCompletedCaptureEvidence>(manifest["mixed_completed_absence_capture"].clone()).unwrap();mixed.validate_shape().unwrap();assert_eq!(mixed.prior.roles.version,2);assert_eq!((mixed.prior.roles.scopes.len(),mixed.native.scopes.len()),(5,2));storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&serving).close().await;drop(serving);drop(backup);
    let kept=current.with_file_name(format!("mixed-v2-new-native-pg-source-kept-{}",uuid::Uuid::new_v4().simple()));std::fs::rename(&current,&kept).unwrap();assert!(!current.exists());let next=current.with_file_name(format!("mixed-v2-new-native-pg-cold-{}",uuid::Uuid::new_v4().simple()));storage::restoration::linux::engine_transfer::restore_qualified_postgres(&target,&next,pg.as_ref().unwrap(),true).await.unwrap();std::env::set_var("DATA_DIR",&next);
    let serving=storage::restoration::linux::open_restored_pg_serving(&next,pg.as_ref().unwrap()).await.unwrap();let pool=storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&serving);let report=storage::attachment_capacity::usage_report(&pool).await.unwrap();assert_eq!((report.retained_bytes,report.logical_retained_bytes,report.pending_bytes),(0,0,0));let mut tx=pool.begin().await.unwrap();let paths=storage::attachment_capacity::retained_paths_in(&mut tx).await.unwrap();let owner=super::super::accounting::capture_in_at(&next,&mut tx,paths,&[]).await.unwrap();assert!(originals.iter().all(|r|owner.matches(r)&&!next.join(&r.intent.storage_path).exists()&&WriterLease::open_idle(&next,&r.intent.attempt_id).is_err()));owner.fence().unwrap();drop(owner);tx.rollback().await.unwrap();pool.close().await;drop(serving);
    let fresh=std::process::Command::new(std::env::current_exe().unwrap()).env("DATA_DIR",&next).args(["storage::attachment_upload::admission::committed_erasure::coordinator::pg_completed_capture_tests::actual_fresh_pg_completed_usage","--exact","--ignored","--nocapture","--test-threads=1"]).output().unwrap();assert!(fresh.status.success()&&String::from_utf8_lossy(&fresh.stdout).contains("ACTUAL_FRESH_MIXED_CURRENT_AUTH_PRIVATE_PIN_AND_FILE_ERASED_ORIGINAL_HTTP_REFUSALS_PASS"),"Fresh third mixed PG lifecycle reader/accounting failed");current=next;println!("ACTUAL_PG_OWNER_V2_NEW_NATIVE_LIFECYCLE_THIRD_MIXED_SOURCE_ABSENT_SEVEN_ORIGINALS_FRESH_ACCOUNTING_HTTP_REFUSALS_PASS");
   }
   println!("ACTUAL_MIXED_COMPLETED_PG_CLEAN_ROOT={}",current.display());println!("ACTUAL_MIXED_COMPLETED_PG_REAL_WRITER_TWO_SOURCE_ABSENT_CYCLES_V2_FRESH_ACCOUNTING_PASS");
  }return;}
  if std::env::var("PRIOR_COMPLETED_PG_TRANSPORT_TEST").as_deref()==Ok("true") {
   let original_scopes=manifest["completed_absence_capture"]["scopes"].clone();let mut current=restored;let mut current_serving=restored_serving;
   for _ in 0..2 {
    let backup_path=current.with_file_name(format!("prior-completed-pg-backup-{}",uuid::Uuid::new_v4().simple()));
    let prior=storage::restoration::linux::engine_transfer::capture_restored_postgres(&current_serving,&backup_path,true).await.unwrap();prior.fence().unwrap();let evidence:Value=serde_json::from_slice(&std::fs::read(backup_path.join("manifest.json")).unwrap()).unwrap();
    assert!(evidence.get("completed_absence_capture").is_none()&&evidence.get("admission_owner_capture").is_none()&&evidence.get("prior_admission_owner_capture").is_none());let roles=evidence["prior_completed_absence_capture"]["roles"]["scopes"].as_array().expect("Root-owned prior writer must emit exact finalized roles");assert_eq!(roles.iter().map(|v|v["original"].clone()).collect::<Vec<_>>(),*original_scopes.as_array().unwrap(),"Prior writer changed immutable original scopes");
    let pool=storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&current_serving);storage::restoration::linux::engine_transfer::pg_completed_test_registry_controls(&backup_path,&pool).await.unwrap();pool.close().await;drop(current_serving);
    let kept=current.with_file_name(format!("prior-completed-pg-original-kept-{}",uuid::Uuid::new_v4().simple()));std::fs::rename(&current,&kept).unwrap();assert!(!current.exists());
    let next=current.with_file_name(format!("prior-completed-pg-cold-{}",uuid::Uuid::new_v4().simple()));storage::restoration::linux::engine_transfer::restore_qualified_postgres(&backup_path,&next,pg.as_ref().unwrap(),true).await.unwrap();current_serving=storage::restoration::linux::open_restored_pg_serving(&next,pg.as_ref().unwrap()).await.unwrap();
    let pool=storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&current_serving);let mut tx=pool.begin().await.unwrap();let paths=storage::attachment_capacity::retained_paths_in(&mut tx).await.unwrap();let absence=super::super::accounting::capture_in_at(&next,&mut tx,paths,&[]).await.unwrap();assert!(all_originals.iter().all(|r|absence.matches(r)));assert_eq!(storage::attachment_capacity::test_completed_logical_charge_in(&mut tx,absence).await.unwrap(),0);assert!(all_originals.iter().all(|r|!next.join(&r.intent.storage_path).exists()&&WriterLease::open_idle(&next,&r.intent.attempt_id).is_err()));tx.rollback().await.unwrap();current=next;
   }
   storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&current_serving).close().await;drop(current_serving);println!("ACTUAL_PRIOR_COMPLETED_PG_CLEAN_RESTORED_ROOT={}",current.display());println!("ACTUAL_PRIOR_COMPLETED_PG_REAL_WRITER_REPEATED_TWO_SOURCE_ABSENT_COLD_ORIGINAL3_ACCOUNTING_NO_WRITER_GRANT_PASS");return;
  }
  if std::env::var("PRIOR_COMPLETED_PG_SOURCE_CONTROLS").as_deref()==Ok("true") {
   let pool=storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&restored_serving);let mut tx=pool.begin().await.unwrap();
   let held=storage::restoration::FinalizedCompletedCapture::hold(&restored,&mut tx).await.unwrap();held.recheck_in(&mut tx).await.unwrap();let evidence=held.evidence().unwrap();
   assert_eq!(evidence.roles.scopes.len(),all_originals.len());assert!(evidence.roles.scopes.iter().all(|s|all_originals.iter().any(|r|r.intent.attempt_id==s.original.reservation.operation_id)));
   let unknown=restored.with_file_name(format!("unregistered-prior-pg-completed-{}",uuid::Uuid::new_v4().simple()));std::fs::create_dir(&unknown).unwrap();assert!(storage::restoration::FinalizedCompletedCapture::hold(&unknown,&mut tx).await.is_err());
   use sqlx::Connection;let mut foreign=sqlx::AnyConnection::connect(pg.as_ref().unwrap()).await.unwrap();let failure=held.recheck_in(&mut foreign).await.err().expect("Foreign connected PG target must refuse");assert_eq!(failure.to_string(),"Prior completed original connected PG target differs");foreign.close().await.unwrap();
   let selected=&evidence.roles.scopes[0];let path=selected.original.files[0].role.relative(&selected.original.reservation.operation_id).unwrap();let file=restored.join(path);let bytes=std::fs::read(&file).unwrap();let original_file=restored.with_file_name(format!("original-prior-pg-record-{}",uuid::Uuid::new_v4().simple()));std::fs::rename(&file,&original_file).unwrap();std::fs::write(&file,&bytes).unwrap();std::fs::set_permissions(&file,std::fs::Permissions::from_mode(0o600)).unwrap();assert!(held.recheck_in(&mut tx).await.is_err(),"Equal-byte foreign prior PG record admitted");
   let foreign_file=restored.with_file_name(format!("foreign-prior-pg-record-{}",uuid::Uuid::new_v4().simple()));std::fs::rename(&file,&foreign_file).unwrap();std::fs::rename(&original_file,&file).unwrap();assert!(held.recheck_in(&mut tx).await.is_err(),"Rename-back prior PG record admitted");drop(held);tx.rollback().await.unwrap();pool.close().await;drop(restored_serving);
   println!("ACTUAL_PRIOR_COMPLETED_PG_FINALIZED_CONSTRUCTOR_FULL_ORIGINALS_FOREIGN_CONNECTED_SQL_UNREGISTERED_EQUALBYTE_RENAMEBACK_REFUSALS_PASS");return;
  }
  storage::restoration::linux::engine_transfer::pg_completed_test_restored_pool(&restored_serving).close().await;drop(restored_serving);
  println!("ACTUAL_COMPLETED_PG_CLEAN_RESTORED_ROOT={}",restored.display());
  backup.fence().unwrap();println!("ACTUAL_ORIGINAL_NATIVE_PG_UPLOAD_DELETION_STANDARD_CAPTURE_SOURCE_ABSENT_COLD_RESTORE_FINALIZED_ACCOUNTING_PASS");return;
 }
 if completed_capture_only {
  assert!(pg.is_none(),"SQLite standard producer fixture requires SQLite");
  server.abort();let _=server.await;state.db.close().await;
  use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&root,std::fs::Permissions::from_mode(0o700)).unwrap();std::fs::set_permissions(root.join("starlane-notes.db"),std::fs::Permissions::from_mode(0o600)).unwrap();
  let target=root.with_file_name(format!("completed-standard-backup-{}",uuid::Uuid::new_v4().simple()));
  let bundle=storage::restoration::linux::capture_sqlite_certified(&root,&target,"1.5.3",true).await.unwrap();
  assert_eq!(bundle.manifest.completed_absence_capture.as_ref().unwrap().scopes.len(),1);
  let mut conn=bundle.db.acquire().await.unwrap();
  let proof=super::super::accounting::capture::ClassifiedCompletedBundle::from_bundle(bundle.clone(),&mut conn).await.unwrap();proof.fence_in(&mut conn).await.unwrap();
  assert!(WriterLease::open_idle(&bundle.data_path(),&original.intent.attempt_id).is_err(),"Cold copied original must not rebind native writer");
  use sqlx::Connection;
  let mut mutator=sqlx::AnyConnection::connect(&format!("sqlite://{}?mode=rw",target.join("data/starlane-notes.db").display())).await.unwrap();
  sqlx::query("UPDATE attachment_erasure_dispositions SET completed_at=$1 WHERE asset_id=$2").bind(chrono::Utc::now().to_rfc3339()).bind(&asset).execute(&mut mutator).await.unwrap();mutator.close().await.unwrap();
  assert!(proof.fence_in(&mut conn).await.is_err(),"Cold exact SQL timestamp substitution admitted");
  drop(proof);drop(conn);bundle.db.close().await;
  println!("ACTUAL_STANDARD_SQLITE_COMPLETED_CAPTURE_COLD_CLASSIFICATION_NO_WRITER_GRANT_PASS");return;
 }
 let disposition_before:String=sqlx::query_scalar("SELECT owned_path_set_json FROM attachment_erasure_dispositions WHERE asset_id=$1").bind(&asset).fetch_one(&state.db).await.unwrap();
 FencedTransaction::begin(&state.db).await.unwrap().finalize_existing(&owner_auth,&deletion,&original).await.unwrap();
 let disposition_after:String=sqlx::query_scalar("SELECT owned_path_set_json FROM attachment_erasure_dispositions WHERE asset_id=$1").bind(&asset).fetch_one(&state.db).await.unwrap();assert_eq!(disposition_before,disposition_after,"Exact retry rewrote immutable completion material");
 assert!(FencedTransaction::begin(&state.db).await.unwrap().finalize_existing(&viewer_auth,&deletion,&original).await.is_err());
 let retained:i64=sqlx::query_scalar("SELECT byte_length FROM attachment_retained_byte_ledger WHERE asset_id=$1").bind(&asset).fetch_one(&state.db).await.unwrap();assert_eq!(retained,original.intent.byte_length,"Private native cleanup silently removed ledger charge");

 // Completed immutable SQL is insufficient without current native absence.
 let usage=storage::attachment_capacity::usage_report(&state.db).await.unwrap();assert_eq!(usage.retained_bytes,0);assert_eq!(usage.pending_bytes,0);
 std::fs::write(root.join(&original.intent.storage_path),b"FOREIGN RESURRECTED OBJECT").unwrap();assert!(storage::attachment_capacity::usage_report(&state.db).await.is_err());assert!(root.join(&original.intent.storage_path).exists());std::fs::remove_file(root.join(&original.intent.storage_path)).unwrap();
 let claim_before:String=sqlx::query_scalar("SELECT admission_claim_sha256 FROM attachment_erasure_dispositions WHERE asset_id=$1").bind(&asset).fetch_one(&state.db).await.unwrap();sqlx::query("UPDATE attachment_erasure_dispositions SET admission_claim_sha256=$1 WHERE asset_id=$2").bind("0".repeat(64)).bind(&asset).execute(&state.db).await.unwrap();assert!(storage::attachment_capacity::usage_report(&state.db).await.is_err());sqlx::query("UPDATE attachment_erasure_dispositions SET admission_claim_sha256=$1 WHERE asset_id=$2").bind(&claim_before).bind(&asset).execute(&state.db).await.unwrap();
 let response=client.post(format!("{base}/spaces/space/documents")).bearer_auth(&token).json(&json!({"source_id":"accounting-next","title":"Next","content":"NEXT ORIGINAL","property_values":[]})).send().await.unwrap();let status=response.status();let next:Value=response.json().await.unwrap();assert_eq!(status,201,"{next}");
 let next_draft=next["draft_id"].as_str().unwrap();let next_endpoint=format!("{base}/spaces/space/documents/accounting-next/drafts/{next_draft}");
 let current:Value=client.get(&next_endpoint).bearer_auth(&token).send().await.unwrap().json().await.unwrap();let response=client.put(&next_endpoint).bearer_auth(&token).json(&json!({"content":protected,"source_digest":current["source_digest"]})).send().await.unwrap();assert!(response.status().is_success());assert!(storage::attachment_capacity::usage_report(&state.db).await.is_err(),"Fresh protected reference allowed charge subtraction");
 let current:Value=client.get(&next_endpoint).bearer_auth(&token).send().await.unwrap().json().await.unwrap();let response=client.put(&next_endpoint).bearer_auth(&token).json(&json!({"content":"NEXT ORIGINAL","source_digest":current["source_digest"]})).send().await.unwrap();assert!(response.status().is_success());
 let request=super::super::UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"actual-fixture".into(),actor_id:"owner".into(),space_id:"space".into(),document_id:"accounting-next".into(),original_name:"proof.bin".into(),mime_type:"application/octet-stream".into()};
 let caller=WriterLease::begin(&root,&uuid::Uuid::new_v4().to_string(),Producer::Direct{request:request.clone()},64).unwrap();
 assert!(storage::attachment_capacity::reserve_authorized(&state.db,&caller,Some(64),&viewer_auth).await.is_err());
 sqlx::query("UPDATE users SET is_active=0 WHERE id='owner'").execute(&state.db).await.unwrap();assert!(storage::attachment_capacity::reserve_authorized(&state.db,&caller,Some(64),&owner_auth).await.is_err());sqlx::query("UPDATE users SET is_active=1 WHERE id='owner'").execute(&state.db).await.unwrap();
 sqlx::query("UPDATE sessions SET expires_at='2000-01-01T00:00:00Z' WHERE id='owner-session'").execute(&state.db).await.unwrap();assert!(storage::attachment_capacity::reserve_authorized(&state.db,&caller,Some(64),&owner_auth).await.is_err());sqlx::query("UPDATE sessions SET expires_at='2999-01-01T00:00:00Z' WHERE id='owner-session'").execute(&state.db).await.unwrap();
 let mut foreign_request=request.clone();foreign_request.operation_id=uuid::Uuid::new_v4().to_string();let foreign=WriterLease::begin(&root,&uuid::Uuid::new_v4().to_string(),Producer::Direct{request:foreign_request},64).unwrap();assert!(storage::attachment_capacity::reserve_authorized(&state.db,&caller,Some(64),&owner_auth).await.is_err(),"Foreign active capsule was arbitrarily excluded");assert!(storage::attachment_capacity::usage_report(&state.db).await.is_err());drop(foreign);drop(caller);
 std::env::set_var("ATTACHMENT_STORAGE_QUOTA_BYTES","1048576");
 let next_operation=uuid::Uuid::new_v4().to_string();let body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"Next.bin\"\r\nContent-Type: application/octet-stream\r\n\r\nSECOND DIFFERENT BODY\r\n--{boundary}--\r\n");
 let response=client.post(format!("{base}/spaces/space/documents/accounting-next/attachments")).bearer_auth(&token).header("x-starlane-upload-operation",&next_operation).header("content-type",format!("multipart/form-data; boundary={boundary}")).body(body).send().await.unwrap();let status=response.status();let next_uploaded:Value=response.json().await.unwrap();assert_eq!(status,201,"{next_uploaded}");
 let next_original={let mut conn=state.db.acquire().await.unwrap();storage::attachment_upload::receipt(&mut conn,&next_operation).await.unwrap().unwrap()};let next_raw=serde_json::to_string(&next_original).unwrap();
 let usage=storage::attachment_capacity::usage_report(&state.db).await.unwrap();assert_eq!(usage.retained_bytes,next_original.intent.byte_length);assert_eq!(usage.pending_bytes,0);
 assert!(client.delete(format!("{base}/spaces/space/documents/accounting-next")).bearer_auth(&token).send().await.unwrap().status().is_success());let target:(String,Option<String>)=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE id='accounting-next'").fetch_one(&state.db).await.unwrap();let next_deletion=uuid::Uuid::new_v4().to_string();assert_eq!(client.delete(format!("{base}/spaces/space/documents/accounting-next/trash")).bearer_auth(&token).header("x-starlane-delete-operation",&next_deletion).json(&json!({"original_target":{"created_at":target.0,"deleted_at":target.1}})).send().await.unwrap().status(),204);
 for _ in 0..3{let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();let done=fenced.prepare_one(&owner_auth,&next_deletion,&next_original).await.unwrap().execute().unwrap();fenced.tx.rollback().await.unwrap();if done{break}}
 FencedTransaction::begin(&state.db).await.unwrap().finalize_existing(&owner_auth,&next_deletion,&next_original).await.unwrap();
 let usage=storage::attachment_capacity::usage_report(&state.db).await.unwrap();assert_eq!(usage.retained_bytes,0,"Multiple completed originals could not share one held native roster");assert_eq!(usage.pending_bytes,0);
 let next_raw_after:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&next_operation).fetch_one(&state.db).await.unwrap();assert_eq!(next_raw,next_raw_after);
 {
  let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();
  let paths=storage::attachment_capacity::retained_paths_in(&mut fenced.tx).await.unwrap();
  let captured=super::super::accounting::capture::NativeCompletedCapture::capture_in(&root,&mut fenced.tx,paths,&[]).await.unwrap();
  let envelope=captured.envelope_in(&mut fenced.tx).await.unwrap();assert_eq!(envelope.scopes.len(),2,"Both completed owners must share one held roster");
  assert!(WriterLease::open_idle(&root,&original.intent.attempt_id).is_err(),"Capture lost original capsule exclusion");
  for scope in &envelope.scopes {assert_eq!(scope.files.len(),9);assert_eq!(scope.directories.len(),6);assert_eq!(scope.disposition.len(),18);}
  for scope in &envelope.scopes {
   use super::super::accounting::capture::{validate_original_records,CompletedFileRole};
   let attempt=&scope.reservation.operation_id;
   let raws=scope.files.iter().map(|b|(b.role.clone(),std::fs::read(root.join(b.role.relative(attempt).unwrap())).unwrap())).collect::<std::collections::BTreeMap<_,_>>();
   validate_original_records(scope,&raws).unwrap();
   let mut changed=scope.clone();changed.disposition.insert("actor_id".into(),"foreign".into());assert!(validate_original_records(&changed,&raws).is_err());
   let mut changed=scope.clone();changed.reservation.committed_bytes=Some(0);assert!(validate_original_records(&changed,&raws).is_err());
   let mut changed=scope.clone();changed.directories[0].original.inode+=1;changed.directories[0].captured=changed.directories[0].original.clone();assert!(validate_original_records(&changed,&raws).is_err());
   let mut changed=scope.clone();changed.files.pop();assert!(validate_original_records(&changed,&raws).is_err());
   let mut changed=raws.clone();changed.get_mut(&CompletedFileRole::CommittedTombstone).unwrap().push(b' ');assert!(validate_original_records(scope,&changed).is_err());
  }
  sqlx::query("UPDATE attachment_erasure_dispositions SET completed_at=$1 WHERE asset_id=$2").bind(chrono::Utc::now().to_rfc3339()).bind(&asset).execute(&mut *fenced.tx).await.unwrap();
  assert!(captured.fence_in(&mut fenced.tx).await.is_err(),"Changed exact completed SQL admitted source transport");
  drop(captured);fenced.tx.rollback().await.unwrap();
  println!("COMPLETED_ABSENCE_SOURCE_NATIVE_TWO_OWNER_RAWSQL_DIRECTORY_FILE_CUSTODY_ACTIVE_EXCLUSION_TAMPER_REFUSAL_PASS");
 }
 std::env::remove_var("ATTACHMENT_STORAGE_QUOTA_BYTES");
 println!("ACTUAL_COMPLETED_ACCOUNTING_BRIDGE_PASS engine={} two_completed_originals=true owned_active_writer_registered_upload=true native_resurrection_refused=true sql_tamper_refused=true fresh_reference_refused=true foreign_active_capsule_refused=true current_writer_authority_refused=true immutable_raw_receipts=true",if pg.is_some(){"postgresql"}else{"sqlite"});
 println!("ACTUAL_FOREIGN_SPACE_DRAFT_WRITER_FENCE_ONLY_PASS engine={} unchanged_while_held=true committed_after_release=true real_registered_winner=true reserved_disposer_refused_committed=true original_current_human_controls=true session_expiry_recheck=true actual_logout_waits=true foreign_publication_waits=true",if pg.is_some(){"postgresql"}else{"sqlite"});
 server.abort();let _=server.await;state.db.close().await;if let Some(admin)=admin{sqlx::query(&format!("DROP SCHEMA {schema} CASCADE")).execute(&admin).await.unwrap();admin.close().await;}
 }
