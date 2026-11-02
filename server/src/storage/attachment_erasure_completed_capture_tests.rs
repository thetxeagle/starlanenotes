 use super::*;use crate::storage::attachment_upload::admission::{WriterLease,Producer};use axum::{body::Body,extract::FromRequestParts,http::Request};use crate::{AppState,storage};use serde_json::{json,Value};use sqlx::any::AnyPoolOptions;
 #[tokio::test(flavor="current_thread")]
 #[ignore="requires private disposable dual-engine fixture and owns actual TCP"]
 async fn actual_foreign_space_draft_writer_waits_across_guarded_scan(){actual_completed_fixture(false,false).await}
 #[tokio::test]
 #[ignore="isolated actual standard completed source capture"]
 async fn actual_standard_sqlite_completed_capture(){actual_completed_fixture(true,false).await}
 #[tokio::test]
 #[ignore="requires a closed actual standard completed restore and own fresh process"]
 async fn actual_fresh_completed_usage(){
  sqlx::any::install_default_drivers();let root=storage::data_dir();
  let db=AnyPoolOptions::new().max_connections(2).connect(&format!("sqlite://{}?mode=rw",root.join("starlane-notes.db").display())).await.unwrap();
  storage::restoration::activate_in(&root,&db).await.unwrap();
  let usage=storage::attachment_capacity::usage_report(&db).await.unwrap();
  assert_eq!(usage.retained_bytes,0);assert_eq!(usage.logical_retained_bytes,0);assert_eq!(usage.pending_bytes,0);
  db.close().await;println!("ACTUAL_FRESH_COMPLETED_RESTORED_USAGE_REPORT_ZERO_PASS");
 }
 #[tokio::test]
 #[ignore="isolated actual multi-owner completed standard capture and cold accounting"]
 async fn actual_standard_sqlite_two_completed_capture(){actual_completed_fixture(true,true).await}
 async fn actual_completed_fixture(completed_capture_only:bool,two_completed:bool){
 sqlx::any::install_default_drivers();let root=storage::data_dir();std::fs::create_dir_all(&root).unwrap();let pg=std::env::var("REPLAY_CAPACITY_TEST_PG_URL").ok();let schema=format!("erase_fence_{}",uuid::Uuid::new_v4().simple());
 let admin=if let Some(url)=&pg{let p=AnyPoolOptions::new().max_connections(2).connect(url).await.unwrap();sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&p).await.unwrap();Some(p)}else{None};
 let db=if let Some(url)=&pg{let owned=schema.clone();AnyPoolOptions::new().max_connections(5).after_connect(move|conn,_|{let sql=format!("SET search_path TO {owned}");Box::pin(async move{sqlx::query(&sql).execute(conn).await?;Ok(())})}).connect(url).await.unwrap()}else{AnyPoolOptions::new().max_connections(5).after_connect(|conn,_|Box::pin(async move{sqlx::query("PRAGMA busy_timeout=10000").execute(&mut *conn).await?;sqlx::query("PRAGMA journal_mode=WAL").execute(conn).await?;Ok(())})).connect(&format!("sqlite://{}?mode=rwc",root.join(if completed_capture_only{"starlane-notes.db"}else{"fixture.sqlite"}).display())).await.unwrap()};
 if pg.is_some(){sqlx::migrate!("./migrations-postgres").run(&db).await.unwrap()}else{sqlx::migrate!("./migrations").run(&db).await.unwrap()}
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
 let mut completed=false;for _ in 0..3 {let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();let one=fenced.prepare_one(&owner_auth,&deletion,&original).await.unwrap();completed=one.execute().unwrap();fenced.tx.rollback().await.unwrap();if completed{break}}
 assert!(completed);assert!(!root.join(&original.intent.storage_path).exists());
 let raw_after_erasure:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&upload_operation).fetch_one(&state.db).await.unwrap();assert_eq!(raw_after_erasure,raw_before);
 FencedTransaction::begin(&state.db).await.unwrap().finalize_existing(&owner_auth,&deletion,&original).await.unwrap();
 if completed_capture_only&&two_completed {
  let page_id="completed-second-page";
  let response=client.post(format!("{base}/spaces/space/documents")).bearer_auth(&token).json(&json!({"source_id":page_id,"title":"Completed Second","content":"SECOND ORIGINAL","property_values":[]})).send().await.unwrap();let status=response.status();let page:Value=response.json().await.unwrap();assert_eq!(status,201,"{page}");
  let response=client.post(format!("{base}/spaces/space/documents/{page_id}/drafts/{}/publish",page["draft_id"].as_str().unwrap())).bearer_auth(&token).json(&json!({})).send().await.unwrap();assert!(response.status().is_success());
  let second_op=uuid::Uuid::new_v4().to_string();let second_boundary="completed-second-original-boundary";
  let second_body=format!("--{second_boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"Second.bin\"\r\nContent-Type: application/octet-stream\r\n\r\nSECOND ORIGINAL NATIVE WINNER\r\n--{second_boundary}--\r\n");
  let response=client.post(format!("{base}/spaces/space/documents/{page_id}/attachments")).bearer_auth(&token).header("x-starlane-upload-operation",&second_op).header("content-type",format!("multipart/form-data; boundary={second_boundary}")).body(second_body).send().await.unwrap();let status=response.status();let uploaded:Value=response.json().await.unwrap();assert_eq!(status,201,"{uploaded}");
  let mut c=state.db.acquire().await.unwrap();let second=storage::attachment_upload::receipt(&mut c,&second_op).await.unwrap().unwrap();drop(c);
  let response=client.delete(format!("{base}/spaces/space/documents/{page_id}")).bearer_auth(&token).send().await.unwrap();assert_eq!(response.status(),204);
  let witness:(String,Option<String>)=sqlx::query_as("SELECT created_at,deleted_at FROM documents WHERE id=$1").bind(page_id).fetch_one(&state.db).await.unwrap();let second_delete=uuid::Uuid::new_v4().to_string();
  let response=client.delete(format!("{base}/spaces/space/documents/{page_id}/trash")).bearer_auth(&token).header("x-starlane-delete-operation",&second_delete).json(&json!({"original_target":{"created_at":witness.0,"deleted_at":witness.1}})).send().await.unwrap();assert_eq!(response.status(),204);
  let mut completed=false;for _ in 0..3{let mut fenced=FencedTransaction::begin(&state.db).await.unwrap();let one=fenced.prepare_one(&owner_auth,&second_delete,&second).await.unwrap();completed=one.execute().unwrap();fenced.tx.rollback().await.unwrap();if completed{break}}assert!(completed);
  FencedTransaction::begin(&state.db).await.unwrap().finalize_existing(&owner_auth,&second_delete,&second).await.unwrap();
  assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_erasure_dispositions").fetch_one(&state.db).await.unwrap(),2);
 }
 if completed_capture_only {
  assert!(pg.is_none(),"SQLite standard producer fixture requires SQLite");
  server.abort();let _=server.await;state.db.close().await;
  use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&root,std::fs::Permissions::from_mode(0o700)).unwrap();std::fs::set_permissions(root.join("starlane-notes.db"),std::fs::Permissions::from_mode(0o600)).unwrap();
  let target=root.with_file_name(format!("completed-standard-backup-{}",uuid::Uuid::new_v4().simple()));
  let bundle=storage::restoration::linux::capture_sqlite_certified(&root,&target,"1.5.3",true).await.unwrap();
  assert_eq!(bundle.manifest.completed_absence_capture.as_ref().unwrap().scopes.len(),if two_completed{2}else{1});
  let mut conn=bundle.db.acquire().await.unwrap();
  assert_eq!(bundle.manifest.compatible_catalogue(),"sqlite/53","Capture report must identify the actual certified catalogue");
  let proof=super::super::accounting::capture::ClassifiedCompletedBundle::from_bundle(bundle.clone(),&mut conn).await.unwrap();proof.fence_in(&mut conn).await.unwrap();
  assert!(WriterLease::open_idle(&bundle.data_path(),&original.intent.attempt_id).is_err(),"Cold copied original must not rebind native writer");
  // Genuine standard producer -> certified destination -> finalized accounting.
  let restored=root.with_file_name(format!("completed-finalized-restored-{}",uuid::Uuid::new_v4().simple()));
  storage::backup_restore::run(&target,&restored,"1.5.3").await.unwrap();
  let restored_db=AnyPoolOptions::new().max_connections(2).connect(&format!("sqlite://{}?mode=rw",restored.join("starlane-notes.db").display())).await.unwrap();
  storage::restoration::activate_in(&restored,&restored_db).await.unwrap();
  let mut restored_tx=restored_db.begin().await.unwrap();
  super::super::accounting::lock_global_in(&mut restored_tx).await.unwrap();
  let paths=storage::attachment_capacity::retained_paths_in(&mut restored_tx).await.unwrap();
  let held=super::super::accounting::capture_in_at(&restored,&mut restored_tx,paths.clone(),&[]).await.unwrap();
  assert!(held.matches(&original));held.fence().unwrap();assert_eq!(storage::attachment_capacity::test_completed_logical_charge_in(&mut restored_tx,held).await.unwrap(),0,"Finalized held absence must exclude only the exact immutable original");
  assert!(WriterLease::open_idle(&restored,&original.intent.attempt_id).is_err(),"Finalized erased original never rebinds WriterLease");
  let clean_restored=root.with_file_name(format!("completed-clean-restored-{}",uuid::Uuid::new_v4().simple()));
  storage::backup_restore::run(&target,&clean_restored,"1.5.3").await.unwrap();
  println!("ACTUAL_COMPLETED_CLEAN_RESTORED_ROOT={}",clean_restored.display());
  // An external native alias cannot change the immutable record link census.
  let alias=restored.with_file_name(format!("completed-external-record-alias-{}",uuid::Uuid::new_v4().simple()));
  let original_claim=restored.join(format!(".attachment-upload-admission/{}/claim.json",original.intent.attempt_id));
  std::fs::hard_link(&original_claim,&alias).unwrap();
  assert!(super::super::accounting::capture_in_at(&restored,&mut restored_tx,paths.clone(),&[]).await.is_err(),"External record alias granted completed accounting");
  std::fs::remove_file(&alias).unwrap();
  // A fresh equal-byte foreign record cannot acquire the sealed adopted role.
  let claim_path=restored.join(format!(".attachment-upload-admission/{}/claim.json",original.intent.attempt_id));
  let saved=restored.with_file_name(format!("completed-preserved-claim-{}",uuid::Uuid::new_v4().simple()));std::fs::rename(&claim_path,&saved).unwrap();
  let bytes=std::fs::read(&saved).unwrap();use std::os::unix::fs::OpenOptionsExt;use std::io::Write;
  let mut foreign=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&claim_path).unwrap();foreign.write_all(&bytes).unwrap();foreign.sync_all().unwrap();drop(foreign);
  assert!(super::super::accounting::capture_in_at(&restored,&mut restored_tx,paths,&[]).await.is_err(),"Equal-byte foreign restored capsule granted accounting");
  restored_tx.rollback().await.unwrap();restored_db.close().await;
  println!("ACTUAL_STANDARD_SQLITE_COMPLETED_FINALIZED_ACCOUNTING_FOREIGN_INODE_REFUSAL_PASS");
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
