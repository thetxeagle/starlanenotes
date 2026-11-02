//! Full upgrade and actual HTTP page/attachment lifecycle accounting controls.
use crate::AppState;
use serde_json::{json,Value};
use sqlx::any::AnyPoolOptions;
async fn charge(state:&AppState)->i64{
 let mut tx=state.db.begin().await.unwrap();super::serialize(&mut tx).await.unwrap();
 let charge=super::retained::charge(&mut tx,true).await.unwrap();let bytes=charge.bytes;
 charge.fence_async().await.unwrap();tx.rollback().await.unwrap();
 charge.fence_async().await.unwrap();charge.release_async().await.unwrap();bytes
}
async fn publish_reviewed(client:&reqwest::Client,base:&str,token:&str,draft:&str){
 let endpoint=format!("{base}/spaces/space-1/documents/direct-doc/drafts/{draft}");
 let response=client.get(format!("{endpoint}/publish-source")).bearer_auth(token).send().await.unwrap();
 let status=response.status();let proof:Value=response.json().await.unwrap();assert_eq!(status,200,"publication review: {proof}");
 assert_eq!(proof["schema_version"],1);assert_eq!(proof["space_id"],"space-1");
 assert_eq!(proof["document_id"],"direct-doc");assert_eq!(proof["draft_id"],draft);
 assert_eq!(proof["draft"]["content"],"PUBLISHED ORIGINAL");
 let intent=json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),
  "draft_source_digest":proof["draft_source_digest"].as_str().expect("reviewed draft digest"),
  "page_source_digest":proof["page_source_digest"].as_str().expect("reviewed page digest")});
 let response=client.post(format!("{endpoint}/publish")).bearer_auth(token).json(&intent).send().await.unwrap();
 let status=response.status();let raw=response.text().await.unwrap();assert_eq!(status,200,"publish status={status} body={raw}");
 let published:Value=serde_json::from_str(&raw).unwrap();
 for field in ["schema_version","operation_id","draft_source_digest","page_source_digest"]{
  assert_eq!(published[field],intent[field],"publication must acknowledge the reviewed original intent");
 }
 assert_eq!(published["draft_id"],draft);assert_eq!(published["disposition"],"applied");
 assert_eq!(published["publication"],proof["draft"]);
}
async fn expect_no_content(response:reqwest::Response,phase:&str){
 let status=response.status();let body=response.text().await.unwrap();
 assert_eq!(status,204,"lifecycle phase={phase} status={status} body={body}");
}
async fn post_file(client:&reqwest::Client,url:&str,token:&str,op:&str,bytes:&[u8],replacement:Option<(&str,i64)>)->reqwest::Response{
 let boundary=format!("retained-{}",uuid::Uuid::new_v4().simple());let mut body=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"Same Name.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();body.extend_from_slice(bytes);body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
 let mut request=client.post(url).bearer_auth(token).header("x-starlane-upload-operation",op).header("content-type",format!("multipart/form-data; boundary={boundary}")).body(body);
 if let Some((id,revision))=replacement{request=request.header("x-starlane-attachment-version",id).header("x-starlane-attachment-revision",revision.to_string());}request.send().await.unwrap()
}
#[tokio::test(flavor="current_thread")]
#[ignore="owns isolated upgrade database/root and real ephemeral TCP; engines run separately"]
async fn retained_bytes_upgrade_actual_page_delete_history_and_refusal(){
 sqlx::any::install_default_drivers();let root=std::path::PathBuf::from(std::env::var("DATA_DIR").expect("owned DATA_DIR"));std::fs::create_dir_all(&root).unwrap();let pg=std::env::var("REPLAY_CAPACITY_TEST_PG_URL").ok();let schema=format!("retained_bytes_{}",uuid::Uuid::new_v4().simple());
 let admin=if let Some(url)=&pg{let p=AnyPoolOptions::new().max_connections(2).connect(url).await.unwrap();sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&p).await.unwrap();Some(p)}else{None};
 let db=if let Some(url)=&pg{let owned=schema.clone();AnyPoolOptions::new().max_connections(5).after_connect(move|conn,_|{let sql=format!("SET search_path TO {owned}");Box::pin(async move{sqlx::query(&sql).execute(conn).await?;Ok(())})}).connect(url).await.unwrap()}else{AnyPoolOptions::new().max_connections(5).after_connect(|conn,_|Box::pin(async move{sqlx::query("PRAGMA busy_timeout=10000").execute(&mut *conn).await?;sqlx::query("PRAGMA journal_mode=WAL").execute(conn).await?;Ok(())})).connect(&format!("sqlite://{}?mode=rwc",root.join("fixture.sqlite").display())).await.unwrap()};
 let mut before=if pg.is_some(){sqlx::migrate!("./migrations-postgres")}else{sqlx::migrate!("./migrations")};let future=if pg.is_some(){5}else{51};before.migrations=std::borrow::Cow::Owned(before.migrations.iter().filter(|m|m.version<future).cloned().collect());before.run(&db).await.unwrap();
 for sql in ["INSERT INTO users(id,email,display_name,role) VALUES('user-1','retained@test.invalid','Retained','member')","INSERT INTO spaces(id,name,slug,created_by) VALUES('space-1','Retained','retained-slug','user-1')","INSERT INTO space_members(id,space_id,user_id,role) VALUES('member-1','space-1','user-1','owner')","INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('session-1','user-1','fixture','2999-01-01T00:00:00Z')","INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at,published_at) VALUES('legacy-doc','space-1','Legacy','spaces/retained-slug/docs/legacy.md','user-1','user-1','source-revision','source-publication')","INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at) VALUES('legacy-asset','legacy-doc','space-1','legacy.bin','Legacy.bin','application/octet-stream',3,'spaces/retained-slug/attachments/legacy-doc/legacy.bin','user-1','2026-10-11T00:00:00Z')"]{sqlx::query(sql).execute(&db).await.unwrap();}
 std::fs::create_dir_all(root.join("spaces/retained-slug/attachments/legacy-doc")).unwrap();std::fs::create_dir_all(root.join("spaces/retained-slug/docs")).unwrap();std::fs::write(root.join("spaces/retained-slug/attachments/legacy-doc/legacy.bin"),b"OLD").unwrap();for name in ["legacy"]{std::fs::write(root.join(format!("spaces/retained-slug/docs/{name}.md")),b"PUBLISHED ORIGINAL").unwrap();}
 if pg.is_some(){sqlx::migrate!("./migrations-postgres").run(&db).await.unwrap();}else{sqlx::migrate!("./migrations").run(&db).await.unwrap();}
 let state=AppState{db,jwt_secret:"owned-retained-fixture".into(),base_url:"http://localhost".into(),search_index:crate::search::SearchIndex::new(root.join("index")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};assert_eq!(charge(&state).await,3);
 let token=crate::auth::jwt::create_token_pair_for_session("user-1","retained@test.invalid","member",&state.jwt_secret,Some("session-1"),30).unwrap().access_token;
 let router=crate::spaces::attachments::router().merge(crate::spaces::moves::router()).merge(crate::spaces::documents::router()).merge(crate::spaces::drafts::router()).merge(crate::storage::attachment_versions::router()).merge(crate::sync::api_router()).with_state(state.clone());let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();let server=tokio::spawn(async move{axum::serve(listener,router).await.unwrap()});let base=format!("http://{address}");let client=reqwest::Client::new();std::env::set_var("MAX_UPLOAD_SIZE_MB","1");std::env::set_var("ATTACHMENT_STORAGE_QUOTA_BYTES","2097152");
 let response=client.post(format!("{base}/spaces/space-1/documents")).bearer_auth(&token).json(&json!({"source_id":"direct-doc","title":"Direct","content":"PUBLISHED ORIGINAL","property_values":[]})).send().await.unwrap();let status=response.status();let created:Value=response.json().await.unwrap();assert!(status.is_success(),"{created}");publish_reviewed(&client,&base,&token,created["draft_id"].as_str().unwrap()).await;
 let upload=format!("{base}/spaces/space-1/documents/direct-doc/attachments");let first_op=uuid::Uuid::new_v4().to_string();let response=post_file(&client,&upload,&token,&first_op,b"ASSET BYTES",None).await;let status=response.status();let first:Value=response.json().await.unwrap();assert_eq!(status,201,"{first}");assert_eq!(charge(&state).await,14);
 let first_id=first["id"].as_str().unwrap();let versions=format!("{base}/spaces/space-1/documents/direct-doc/attachment-families/{first_id}/versions");let next_op=uuid::Uuid::new_v4().to_string();let response=post_file(&client,&versions,&token,&next_op,b"NEWER SAME NAME",Some((first_id,1))).await;let status=response.status();let second:Value=response.json().await.unwrap();assert_eq!(status,201,"{second}");assert_eq!(charge(&state).await,29);
 let retire=format!("{upload}/{first_id}");assert_eq!(client.delete(&retire).bearer_auth(&token).send().await.unwrap().status(),204);assert_eq!(charge(&state).await,29);let history:Value=client.get(&versions).bearer_auth(&token).send().await.unwrap().json().await.unwrap();assert_eq!(history["total"],2);assert_eq!(history["retained_versions"].as_object().unwrap().len(),1);
 std::env::set_var("ATTACHMENT_STORAGE_QUOTA_BYTES","29");let retry=post_file(&client,&versions,&token,&next_op,b"NEWER SAME NAME",Some((first_id,1))).await;assert_eq!(retry.status(),201);
 sqlx::query("INSERT INTO spaces(id,name,slug,created_by) VALUES('space-2','Destination','destination-slug','user-1')").execute(&state.db).await.unwrap();sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('member-2','space-2','user-1','owner')").execute(&state.db).await.unwrap();
 let response=client.post(format!("{base}/spaces/space-1/moves/preview")).bearer_auth(&token).json(&json!({"root":{"kind":"document","id":"direct-doc"},"destination":{"space_id":"space-2","folder_id":null,"parent_id":null}})).send().await.unwrap();let status=response.status();let preview:Value=response.json().await.unwrap();assert_eq!(status,200,"{preview}");assert_eq!(preview["commit_available"],true,"{preview}");
 let response=client.post(format!("{base}/moves/previews/{}/commit",preview["preview_id"].as_str().unwrap())).bearer_auth(&token).json(&json!({"fingerprint":preview["fingerprint"],"decisions":[],"public_links":"disable","confirm_audience_change":true,"confirm_draft_space_change":true})).send().await.unwrap();let status=response.status();let moved:Value=response.json().await.unwrap();assert_eq!(status,200,"{moved}");assert_eq!(charge(&state).await,55,"original 26-byte direct versions plus authenticated distinct 26-byte moved copies and legacy 3-byte blob");
 for (space,doc) in [("space-1","legacy-doc"),("space-2","direct-doc")]{
  let page=format!("{base}/spaces/{space}/documents/{doc}");
  expect_no_content(client.delete(&page).bearer_auth(&token).send().await.unwrap(),&format!("{space}/{doc}:first-trash")).await;
  assert_eq!(charge(&state).await,55);
  expect_no_content(client.post(format!("{page}/trash")).bearer_auth(&token).send().await.unwrap(),&format!("{space}/{doc}:restore")).await;
  assert_eq!(charge(&state).await,55);
  expect_no_content(client.delete(&page).bearer_auth(&token).send().await.unwrap(),&format!("{space}/{doc}:second-trash")).await;
  assert_eq!(charge(&state).await,55);
  // Review the current incarnation and second trash cycle through the same
  // authenticated lifecycle metadata route used by a real client.
  let response=client.get(format!("{base}/spaces/{space}/documents/trash")).bearer_auth(&token).send().await.unwrap();
  let status=response.status();let raw=response.text().await.unwrap();
  assert_eq!(status,200,"lifecycle phase={space}/{doc}:review-trash status={status} body={raw}");
  let reviewed:Value=serde_json::from_str(&raw).unwrap();
  let selected=reviewed.as_array().expect("trash metadata array").iter().find(|row|row["id"]==doc).expect("original trashed page in reviewed list");
  assert_eq!(selected["space_id"],space);
  assert!(selected["deleted_at"].as_str().is_some_and(|value|!value.is_empty()));
  let intent=json!({"original_target":{"created_at":selected["created_at"].as_str().expect("reviewed creation stamp"),"deleted_at":selected["deleted_at"]}});
  let operation=uuid::Uuid::new_v4().to_string();
  let response=client.delete(format!("{page}/trash")).bearer_auth(&token)
   .header("x-starlane-delete-operation",&operation).json(&intent).send().await.unwrap();
  let acknowledged=response.headers().get("x-starlane-delete-operation").and_then(|value|value.to_str().ok()).map(str::to_owned);
  expect_no_content(response,&format!("{space}/{doc}:permanent-delete")).await;
  assert_eq!(acknowledged.as_deref(),Some(operation.as_str()),"lifecycle phase={space}/{doc}:permanent-delete acknowledgement");
  assert_eq!(charge(&state).await,55);
 }
 let live:i64=sqlx::query_scalar("SELECT count(*) FROM attachments").fetch_one(&state.db).await.unwrap();assert_eq!(live,0);let ledger:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_retained_byte_ledger").fetch_one(&state.db).await.unwrap();assert_eq!(ledger,3);
 // A fresh remaining page makes quota refusal independent of deleted-page authorization.
 sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at,published_at) VALUES('new-doc','space-1','New','spaces/retained-slug/docs/new.md','user-1','user-1','source-revision','source-publication')").execute(&state.db).await.unwrap();std::fs::write(root.join("spaces/retained-slug/docs/new.md"),b"NEW PAGE").unwrap();std::env::set_var("ATTACHMENT_STORAGE_QUOTA_BYTES",(1048576+54).to_string());let denied=post_file(&client,&format!("{base}/spaces/space-1/documents/new-doc/attachments"),&token,&uuid::Uuid::new_v4().to_string(),b"NEW",None).await;assert_eq!(denied.status(),409);assert!(denied.text().await.unwrap().contains("quota exhausted"));assert_eq!(charge(&state).await,55);
 // Unattributed pre-upgrade retained bytes refuse configured quota; no invented provenance.
 std::fs::write(root.join("spaces/retained-slug/attachments/legacy-doc/orphan.bin"),b"UNATTRIBUTED").unwrap();let mut tx=state.db.begin().await.unwrap();super::serialize(&mut tx).await.unwrap();assert!(super::retained::charge(&mut tx,true).await.err().unwrap().to_string().contains("Unattributed"));tx.rollback().await.unwrap();std::fs::remove_file(root.join("spaces/retained-slug/attachments/legacy-doc/orphan.bin")).unwrap();
 sqlx::query("UPDATE attachment_retained_byte_ledger SET byte_length=4 WHERE asset_id='legacy-asset'").execute(&state.db).await.unwrap();let mut tx=state.db.begin().await.unwrap();super::serialize(&mut tx).await.unwrap();assert!(super::retained::charge(&mut tx,true).await.is_err());tx.rollback().await.unwrap();sqlx::query("UPDATE attachment_retained_byte_ledger SET byte_length=3 WHERE asset_id='legacy-asset'").execute(&state.db).await.unwrap();
 // Immutable original proof loss/hash contradiction cannot become a lower charge.
 sqlx::query("CREATE TABLE fixture_ledger_snapshot AS SELECT * FROM attachment_retained_byte_ledger").execute(&state.db).await.unwrap();
 sqlx::query("DELETE FROM attachment_retained_byte_ledger WHERE asset_id=$1").bind(first_id).execute(&state.db).await.unwrap();let mut tx=state.db.begin().await.unwrap();super::serialize(&mut tx).await.unwrap();assert!(super::retained::charge(&mut tx,true).await.is_err());tx.rollback().await.unwrap();sqlx::query("INSERT INTO attachment_retained_byte_ledger SELECT * FROM fixture_ledger_snapshot WHERE asset_id=$1").bind(first_id).execute(&state.db).await.unwrap();
 sqlx::query("UPDATE attachment_retained_byte_ledger SET original_receipt_sha256=$1 WHERE asset_id=$2").bind("0".repeat(64)).bind(first_id).execute(&state.db).await.unwrap();let mut tx=state.db.begin().await.unwrap();super::serialize(&mut tx).await.unwrap();assert!(super::retained::charge(&mut tx,true).await.is_err());tx.rollback().await.unwrap();sqlx::query("UPDATE attachment_retained_byte_ledger SET original_receipt_sha256=(SELECT original_receipt_sha256 FROM fixture_ledger_snapshot WHERE asset_id=$1) WHERE asset_id=$1").bind(first_id).execute(&state.db).await.unwrap();sqlx::query("DROP TABLE fixture_ledger_snapshot").execute(&state.db).await.unwrap();assert_eq!(charge(&state).await,55);
 println!("PASS engine={} upgrade_seed=true original_migrations_byteexact=true actual_tcp=true legacy_retained=3 history_retired_retained=26 page_cascade_charge=55 authenticated_distinct_copy_extra_charge=26 fresh_upload_quota_refused=true unattributed_refused=true contradictory_refused=true missing_ledger_refused=true raw_receipt_hash_refused=true",if pg.is_some(){"postgresql"}else{"sqlite"});server.abort();let _=server.await;state.db.close().await;if let Some(admin)=admin{sqlx::query(&format!("DROP SCHEMA {schema} CASCADE")).execute(&admin).await.unwrap();admin.close().await;}
}
