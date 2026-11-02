#[cfg(test)]
mod sql_controls {
 use super::*;use sqlx::Connection;
 async fn pool()->(sqlx::AnyPool,crate::db::DatabaseEngine,String) {
  let url=std::env::var("STARLANE_CAPACITY_TEST_URL").expect("owned fixture URL required");sqlx::any::install_default_drivers();
  let engine=crate::db::DatabaseEngine::from_url(&url).unwrap();let schema=format!("disposal_{}",uuid::Uuid::new_v4().simple());
  if engine==crate::db::DatabaseEngine::Postgres {let mut c=sqlx::PgConnection::connect(&url).await.unwrap();sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&mut c).await.unwrap();c.close().await.unwrap();}
  let child_schema=schema.clone();let p=sqlx::any::AnyPoolOptions::new().max_connections(3).after_connect(move|c,_| {let schema=schema.clone();Box::pin(async move {
   if engine==crate::db::DatabaseEngine::Postgres {sqlx::query(&format!("SET search_path TO {schema}")).execute(c).await?;}else{sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;}Ok(())
  })}).connect(&url).await.unwrap();crate::db::selected_migrator(engine).run(&p).await.unwrap();
  match sqlx::query_as::<_,(i64,i64)>("SELECT id,revision FROM attachment_capacity_lock").fetch_all(&p).await {
   Ok(rows)=>{assert_eq!(rows,vec![(1,1)],"Future quota fixture requires exact migration baseline");assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM attachment_capacity_reservations").fetch_one(&p).await.unwrap(),0);},
   Err(_)=>{sqlx::raw_sql(include_str!("attachment_capacity_schema.sql")).execute(&p).await.unwrap();}
  }
  sqlx::query("INSERT INTO users(id,email,display_name) VALUES('cleanup-owner','cleanup@example.test','Cleanup')").execute(&p).await.unwrap();
  sqlx::query("INSERT INTO spaces(id,name,slug,created_by) VALUES('cleanup-space','Cleanup','cleanup','cleanup-owner')").execute(&p).await.unwrap();
  sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('cleanup-page','cleanup-space','Cleanup','fixture.md','cleanup-owner','cleanup-owner')").execute(&p).await.unwrap();(p,engine,child_schema)
 }
 fn root()->PathBuf {let p=std::env::temp_dir().join(format!("disposal-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&p).unwrap();p}
 fn request()->UploadRequest {UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"direct".into(),actor_id:"cleanup-owner".into(),space_id:"cleanup-space".into(),document_id:"cleanup-page".into(),original_name:"partial.bin".into(),mime_type:"application/octet-stream".into()}}
 async fn charged(pool:&sqlx::AnyPool,id:&str)->String {sqlx::query_scalar("SELECT state FROM attachment_capacity_reservations WHERE operation_id=$1").bind(id).fetch_one(pool).await.unwrap()}
 #[tokio::test]
 #[ignore="requires private disposable SQLite or PostgreSQL fixture"]
 async fn required_direct_disposal_engine() {
  use crate::storage::{attachment_upload as upload,attachment_capacity as capacity};let (pool,engine,_schema)=pool().await;let root=root();
  let req=request();let id=uuid::Uuid::new_v4().to_string();let lease=WriterLease::begin(&root,&id,Producer::Direct{request:req.clone()},64).unwrap();capacity::reserve(&pool,&lease,Some(64)).await.unwrap();
  let mut stage=upload::Ingest::reserve_with_lease(&root,req.clone(),64,lease.clone()).unwrap();stage.append(b"partial bytes").unwrap();
  assert!(dispose_direct(&pool,&root,&id).await.is_err());assert_eq!(charged(&pool,&id).await,"reserved");drop(stage);drop(lease);
  let payload=root.join(stage::NAMESPACE).join(&id).join("payload.part");let original=std::fs::read(&payload).unwrap();
  let held=WriterLease::open_idle(&root,&id).unwrap();let plan=held.plan_direct().unwrap();let raw=json(&plan).unwrap();stage::write_envelope(&held.0.directory,"disposal-intent.json",&raw).unwrap();drop(held);
  assert_eq!(charged(&pool,&id).await,"reserved");let sha=dispose_direct(&pool,&root,&id).await.unwrap();assert!(!payload.exists());assert_eq!(charged(&pool,&id).await,"disposed");
  assert_eq!(sha,dispose_direct(&pool,&root,&id).await.unwrap());assert_eq!(original,b"partial bytes");
  let idle=WriterLease::open_idle(&root,&id).unwrap();assert!(capacity::reserve(&pool,&idle,None).await.is_err());drop(idle);
  // Equal bytes on a foreign inode cannot become owned disposal evidence.
  let req2=request();let id2=uuid::Uuid::new_v4().to_string();let l=WriterLease::begin(&root,&id2,Producer::Direct{request:req2.clone()},64).unwrap();capacity::reserve(&pool,&l,None).await.unwrap();
  let mut s=upload::Ingest::reserve_with_lease(&root,req2,64,l.clone()).unwrap();s.append(b"foreign inode").unwrap();drop(s);drop(l);
  let p=root.join(stage::NAMESPACE).join(&id2).join("payload.part");let retained=p.with_extension("retained");std::fs::rename(&p,&retained).unwrap();std::fs::copy(&retained,&p).unwrap();
  assert!(dispose_direct(&pool,&root,&id2).await.is_err());assert_eq!(charged(&pool,&id2).await,"reserved");assert!(p.exists()&&retained.exists());
  // Installed but rolled-back publication is disposed only with original proof and no SQL authority.
  let winner_req=request();let winner=uuid::Uuid::new_v4().to_string();let w=WriterLease::begin(&root,&winner,Producer::Direct{request:winner_req.clone()},64).unwrap();capacity::reserve(&pool,&w,None).await.unwrap();
  let mut s=upload::Ingest::reserve_with_lease(&root,winner_req.clone(),64,w.clone()).unwrap();s.append(b"winning bytes").unwrap();let receipt=s.finish().unwrap().prepare("cleanup","bin").unwrap().install().unwrap();
  let i=&receipt.intent;let q=&i.request;let mut tx=pool.begin().await.unwrap();
  sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
   .bind(&i.asset_id).bind(&q.document_id).bind(&q.space_id).bind(&i.filename).bind(&q.original_name).bind(&q.mime_type).bind(i.byte_length).bind(&i.storage_path).bind(&q.actor_id).bind(&i.created_at).execute(&mut *tx).await.unwrap();
  upload::insert(&mut tx,&receipt).await.unwrap();capacity::commit_direct_in(&mut tx,&w,&receipt).await.unwrap();tx.commit().await.unwrap();drop(w);
  let original_receipt:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&winner_req.operation_id).fetch_one(&pool).await.unwrap();
  assert!(dispose_direct(&pool,&root,&winner).await.is_err());upload::verify(&root,&receipt,false).unwrap();assert_eq!(charged(&pool,&winner).await,"committed");
  let loser=uuid::Uuid::new_v4().to_string();let l=WriterLease::begin(&root,&loser,Producer::Direct{request:winner_req},64).unwrap();capacity::reserve(&pool,&l,None).await.unwrap();
  let mut s=upload::Ingest::reserve_with_lease(&root,q.clone(),64,l.clone()).unwrap();s.append(b"winning bytes").unwrap();s.finish().unwrap().prepare("cleanup","bin").unwrap().install().unwrap();drop(l);
  assert_eq!(charged(&pool,&loser).await,"reserved");dispose_direct(&pool,&root,&loser).await.unwrap();assert_eq!(charged(&pool,&loser).await,"disposed");
  let unchanged:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&q.operation_id).fetch_one(&pool).await.unwrap();assert_eq!(unchanged,original_receipt);upload::verify(&root,&receipt,false).unwrap();
  let replay=uuid::Uuid::new_v4().to_string();let l=WriterLease::begin(&root,&replay,Producer::Replay{draft_scope:None,provider:"web".into(),incoming_id:"incoming".into(),actor_id:"cleanup-owner".into(),space_id:"cleanup-space".into(),document_id:"cleanup-page".into(),asset_id:uuid::Uuid::new_v4().to_string(),payload_fingerprint:"a".repeat(64),binary_sha256:"b".repeat(64),byte_length:4},4).unwrap();capacity::reserve(&pool,&l,None).await.unwrap();drop(l);
  assert!(dispose_direct(&pool,&root,&replay).await.is_err());assert_eq!(charged(&pool,&replay).await,"reserved");
  pool.close().await;println!("DIRECT_DISPOSAL_NATIVE_CUSTODY_ACTIVE_FOREIGN_INODE_LOSER_WINNER_REPLAY_REFUSAL_PASS {:?}",engine);
 }
 #[tokio::test]
 #[ignore="child of owned disposal process-kill test"]
 async fn required_disposal_crash_child() {
  sqlx::any::install_default_drivers();let url=std::env::var("STARLANE_CAPACITY_TEST_URL").unwrap();let schema=std::env::var("STARLANE_DISPOSAL_TEST_SCHEMA").unwrap();
  assert!(schema.bytes().all(|b|b.is_ascii_alphanumeric()||b==b'_'));
  let pg=url.starts_with("postgres");
  let pool=sqlx::any::AnyPoolOptions::new().max_connections(1).after_connect(move|c,_|{let schema=schema.clone();Box::pin(async move{
    if pg {sqlx::query(&format!("SET search_path TO {schema}")).execute(c).await?;}else {sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;}Ok(())
  })}).connect(&url).await.unwrap();
  let root=PathBuf::from(std::env::var("STARLANE_DISPOSAL_TEST_ROOT").unwrap());let attempt=std::env::var("STARLANE_DISPOSAL_TEST_ATTEMPT").unwrap();
  match std::env::var("STARLANE_DISPOSAL_TEST_ADAPTER").as_deref() {
   Ok("replay")=>{dispose_replay(&pool,&root,&attempt).await.unwrap();},
   Ok("replay-rewrite")=>{replay_rewrite_child(&pool,&root,&attempt).await;},
   Ok("replay-blob")=>{let lease=WriterLease::open_idle(&root,&attempt).unwrap();crate::storage::asset_blob::create_with_lease(&root,"cleanup","cleanup-page","bin",b"REPLAY BINARY",lease).unwrap();},
   _=>{dispose_direct(&pool,&root,&attempt).await.unwrap();}
  }
  panic!("expected SIGKILL checkpoint, reached completion");
 }
 #[tokio::test]
 #[ignore="requires private disposable SQLite or PostgreSQL fixture"]
 async fn required_disposal_crash_windows() {
  use crate::storage::{attachment_upload as upload,attachment_capacity as capacity};let (pool,engine,schema)=pool().await;
  for point in ["intent-durable","canonical-unlinked","payload-unlinked","tombstone-durable","sql-cas-uncommitted"] {
   let root=root();let req=request();let attempt=uuid::Uuid::new_v4().to_string();let lease=WriterLease::begin(&root,&attempt,Producer::Direct{request:req.clone()},64).unwrap();
   capacity::reserve(&pool,&lease,None).await.unwrap();let mut s=upload::Ingest::reserve_with_lease(&root,req,64,lease.clone()).unwrap();s.append(b"crash owned payload").unwrap();
   let receipt=s.finish().unwrap().prepare("cleanup","bin").unwrap().install().unwrap();drop(lease);
   let mut child=std::process::Command::new(std::env::current_exe().unwrap()).args(["--ignored","--exact","storage::attachment_upload::admission::sql_controls::required_disposal_crash_child","--nocapture","--test-threads=1"])
    .env("STARLANE_DISPOSAL_TEST_SCHEMA",&schema).env("STARLANE_DISPOSAL_TEST_ROOT",&root).env("STARLANE_DISPOSAL_TEST_ATTEMPT",&attempt).env("STARLANE_DISPOSAL_TEST_CHECKPOINT",point)
    .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
   let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
   while !root.join("checkpoint-ready").exists() {
    if let Some(status)=child.try_wait().unwrap(){panic!("checkpoint child exited early {point}: {status}");}
    if std::time::Instant::now()>deadline{let _=child.kill();panic!("checkpoint timeout {point}");}
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
   }
   assert!(WriterLease::open_idle(&root,&attempt).is_err());child.kill().unwrap();child.wait().unwrap();
   assert_eq!(charged(&pool,&attempt).await,"reserved");
   let digest=dispose_direct(&pool,&root,&attempt).await.unwrap();assert_eq!(charged(&pool,&attempt).await,"disposed");
   assert!(!root.join(&receipt.intent.storage_path).exists());assert!(!root.join(stage::NAMESPACE).join(&attempt).join("payload.part").exists());
   assert_eq!(digest,dispose_direct(&pool,&root,&attempt).await.unwrap());
   assert!(root.join(stage::NAMESPACE).join(&attempt).join("install-proof.json").exists());
   println!("ACTUAL_SIGKILL_DISPOSAL_CHECKPOINT_PASS {:?} {}",engine,point);
  }
  pool.close().await;
 }
 #[tokio::test]
 #[ignore="requires private disposable SQLite or PostgreSQL fixture"]
 async fn required_replay_disposal_engine() {
  use crate::storage::{asset_blob,attachment_capacity as capacity,canonical_rewrite::{self,IncomingIdentity,RewriteIntent,RewriteReason}};
  let (pool,engine,_schema)=pool().await;let root=root();std::fs::create_dir_all(root.join("spaces/cleanup/docs")).unwrap();
  let original=b"ORIGINAL PAGE";std::fs::write(root.join("spaces/cleanup/docs/original.md"),original).unwrap();
  sqlx::query("UPDATE documents SET content_path='spaces/cleanup/docs/original.md',updated_at='source',published_at='published' WHERE id='cleanup-page'").execute(&pool).await.unwrap();
  let produce=|incoming:&str|Producer::Replay{draft_scope:None,provider:"private-provider".into(),incoming_id:incoming.into(),actor_id:"cleanup-owner".into(),space_id:"cleanup-space".into(),document_id:"cleanup-page".into(),asset_id:"opaque-client-asset".into(),payload_fingerprint:"a".repeat(64),binary_sha256:digest(b"REPLAY BINARY"),byte_length:13};
  // Durable capsule with no staged blob is a zero-byte admission interruption.
  let id=uuid::Uuid::new_v4().to_string();let l=WriterLease::begin(&root,&id,produce("zero-admission"),64).unwrap();capacity::reserve(&pool,&l,None).await.unwrap();drop(l);
  dispose_replay(&pool,&root,&id).await.unwrap();assert_eq!(charged(&pool,&id).await,"disposed");
  let id=uuid::Uuid::new_v4().to_string();let l=WriterLease::begin(&root,&id,produce("opaque-incoming"),64).unwrap();capacity::reserve(&pool,&l,None).await.unwrap();
  let blob=asset_blob::create_with_lease(&root,"cleanup","cleanup-page","bin",b"REPLAY BINARY",l.clone()).unwrap();let path=blob.path().to_owned();
  assert!(dispose_replay(&pool,&root,&id).await.is_err());assert_eq!(charged(&pool,&id).await,"reserved");drop(blob);drop(l);
  let sha=dispose_replay(&pool,&root,&id).await.unwrap();assert!(!root.join(&path).exists());assert_eq!(charged(&pool,&id).await,"disposed");assert_eq!(sha,dispose_replay(&pool,&root,&id).await.unwrap());
  // Matching bytes at a substituted native inode cannot become owned replay disposal.
  let id=uuid::Uuid::new_v4().to_string();let l=WriterLease::begin(&root,&id,produce("foreign-attempt"),64).unwrap();capacity::reserve(&pool,&l,None).await.unwrap();
  let blob=asset_blob::create_with_lease(&root,"cleanup","cleanup-page","bin",b"REPLAY BINARY",l.clone()).unwrap();let p=root.join(blob.path());drop(blob);drop(l);
  let retained=p.with_extension("original");std::fs::rename(&p,&retained).unwrap();std::fs::copy(&retained,&p).unwrap();assert!(dispose_replay(&pool,&root,&id).await.is_err());assert_eq!(charged(&pool,&id).await,"reserved");assert!(p.exists()&&retained.exists());
  // A real old receipt has no synthesized attempt link; a new loser cannot alter its bytes.
  let old_blob=asset_blob::create(&root,"cleanup","cleanup-page","bin",b"REPLAY BINARY").unwrap();
  let old_value=serde_json::json!({"incoming":{"provider":"private-provider","operation_id":"legacy-incoming","entity_type":"asset","entity_id":"opaque-client-asset","action":"create","payload_fingerprint":"a".repeat(64)},
   "actor_id":"cleanup-owner","space_id":"cleanup-space","document_id":"cleanup-page","storage_path":old_blob.path(),"filename":old_blob.path().rsplit('/').next().unwrap(),"original_name":"legacy.bin","mime_type":"application/octet-stream","created_at":"2026-10-11T00:00:00Z",
   "byte_length":13,"binary_sha256":digest(b"REPLAY BINARY"),"source_kind":"unchanged","source_id":null,"source_revision":null,"body_sha256":null});
  let old:crate::sync::AssetReplayReceipt=serde_json::from_value(old_value).unwrap();let old_raw=serde_json::to_string(&old).unwrap();
  sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at) VALUES('opaque-client-asset','cleanup-page','cleanup-space',$1,'legacy.bin','application/octet-stream',13,$2,'cleanup-owner',$3)")
   .bind(&old.filename).bind(&old.storage_path).bind(&old.created_at).execute(&pool).await.unwrap();
  sqlx::query("INSERT INTO asset_replay_receipts(provider,incoming_id,actor_id,entity_type,entity_id,action,payload_fingerprint,space_id,document_id,storage_path,filename,original_name,mime_type,byte_length,binary_sha256,source_kind,source_id,source_revision,body_sha256,receipt_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)")
   .bind(&old.incoming.provider).bind(&old.incoming.operation_id).bind(&old.actor_id).bind(&old.incoming.entity_type).bind(&old.incoming.entity_id).bind(&old.incoming.action).bind(&old.incoming.payload_fingerprint)
   .bind(&old.space_id).bind(&old.document_id).bind(&old.storage_path).bind(&old.filename).bind(&old.original_name).bind(&old.mime_type).bind(old.byte_length).bind(&old.binary_sha256).bind(&old.source_kind)
   .bind(&old.source_id).bind(&old.source_revision).bind(&old.body_sha256).bind(&old_raw).bind(&old.created_at).execute(&pool).await.unwrap();
  let losing=uuid::Uuid::new_v4().to_string();let l=WriterLease::begin(&root,&losing,produce("legacy-incoming"),64).unwrap();capacity::reserve(&pool,&l,None).await.unwrap();
  let new_blob=asset_blob::create_with_lease(&root,"cleanup","cleanup-page","bin",b"REPLAY BINARY",l.clone()).unwrap();let new_path=new_blob.path().to_owned();drop(new_blob);drop(l);
  assert_eq!(charged(&pool,&losing).await,"reserved");dispose_replay(&pool,&root,&losing).await.unwrap();assert_eq!(charged(&pool,&losing).await,"disposed");assert!(!root.join(new_path).exists());old_blob.verify().unwrap();
  let unchanged:String=sqlx::query_scalar("SELECT receipt_json FROM asset_replay_receipts WHERE provider='private-provider' AND incoming_id='legacy-incoming'").fetch_one(&pool).await.unwrap();assert_eq!(unchanged,old_raw);assert!(!unchanged.contains("admission_attempt"));
  // Fully prepared uncommitted protected rewrite remains byteexact, never becomes serving authority.
  sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('cleanup-member','cleanup-space','cleanup-owner','owner')").execute(&pool).await.unwrap();
  sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('cleanup-session','cleanup-owner','test','2999-01-01T00:00:00Z')").execute(&pool).await.unwrap();
  let auth=crate::auth::middleware::AuthUser{claims:crate::auth::jwt::Claims{sub:"cleanup-owner".into(),sid:"cleanup-session".into(),email:"cleanup@example.test".into(),role:"member".into(),exp:4102444800,iat:0,token_type:crate::auth::jwt::TokenType::Access},api_key_scopes:None};
  let id=uuid::Uuid::new_v4().to_string();let l=WriterLease::begin(&root,&id,produce("rewrite-attempt"),64).unwrap();capacity::reserve(&pool,&l,None).await.unwrap();
  let blob=asset_blob::create_with_lease(&root,"cleanup","cleanup-page","bin",b"REPLAY BINARY",l.clone()).unwrap();let blob_path=blob.path().to_owned();let result=format!("ORIGINAL PAGE [file](/api/v1/files/{blob_path})");
  let intent=RewriteIntent{space_id:"cleanup-space".into(),document_id:"cleanup-page".into(),source_path:"spaces/cleanup/docs/original.md".into(),source_revision:"source".into(),published_stamp:"published".into(),revision:"new".into(),source_sha256:Some(digest(original)),reason:RewriteReason::AssetReferenceRewrite,
   incoming:IncomingIdentity{provider:"private-provider".into(),operation_id:"rewrite-attempt".into(),entity_type:"asset".into(),entity_id:"opaque-client-asset".into(),action:"create".into(),payload_fingerprint:"a".repeat(64)}};
  let mut tx=pool.begin().await.unwrap();crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx,&["cleanup-space"]).await.unwrap();
  sqlx::query("UPDATE documents SET id=id WHERE id='cleanup-page'").execute(&mut *tx).await.unwrap();
  std::env::set_var("DATA_DIR",&root);let rewrite=canonical_rewrite::prepare_asset_replay_with_lease(&mut tx,&auth,intent,result.as_bytes(),&l).await.unwrap();tx.rollback().await.unwrap();
  let manifest_path=root.join(".canonical-rewrite-journal").join(rewrite.op_id()).join("manifest.json");let proof_path=manifest_path.with_file_name("install-proof.json");let manifest_before=std::fs::read(&manifest_path).unwrap();let proof_before=std::fs::read(&proof_path).unwrap();drop(blob);drop(l);
  dispose_replay(&pool,&root,&id).await.unwrap();assert_eq!(charged(&pool,&id).await,"disposed");assert!(!root.join(&blob_path).exists());
  assert_eq!(std::fs::read(&manifest_path).unwrap(),manifest_before);assert_eq!(std::fs::read(&proof_path).unwrap(),proof_before);assert_eq!(std::fs::read(root.join(rewrite.path())).unwrap(),result.as_bytes());
  assert_eq!(canonical_rewrite::confirm_committed(&pool,&rewrite).await.unwrap(),canonical_rewrite::CommitEvidence::NotCommitted);
  assert_eq!(std::fs::read(root.join("spaces/cleanup/docs/original.md")).unwrap(),original);
  let current:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='cleanup-page'").fetch_one(&pool).await.unwrap();assert_eq!(current,"spaces/cleanup/docs/original.md");
  let negative=uuid::Uuid::new_v4().to_string();let l=WriterLease::begin(&root,&negative,produce("negative-rewrite"),64).unwrap();capacity::reserve(&pool,&l,None).await.unwrap();
  let b=asset_blob::create_with_lease(&root,"cleanup","cleanup-page","bin",b"REPLAY BINARY",l.clone()).unwrap();let bpath=b.path().to_owned();let body=format!("ORIGINAL PAGE [foreign](/api/v1/files/{bpath})");
  let ni=RewriteIntent{space_id:"cleanup-space".into(),document_id:"cleanup-page".into(),source_path:"spaces/cleanup/docs/original.md".into(),source_revision:"source".into(),published_stamp:"published".into(),revision:"negative".into(),source_sha256:Some(digest(original)),reason:RewriteReason::AssetReferenceRewrite,
   incoming:IncomingIdentity{provider:"private-provider".into(),operation_id:"negative-rewrite".into(),entity_type:"asset".into(),entity_id:"opaque-client-asset".into(),action:"create".into(),payload_fingerprint:"a".repeat(64)}};
  let mut tx=pool.begin().await.unwrap();crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx,&["cleanup-space"]).await.unwrap();sqlx::query("UPDATE documents SET id=id WHERE id='cleanup-page'").execute(&mut *tx).await.unwrap();
  let nr=canonical_rewrite::prepare_asset_replay_with_lease(&mut tx,&auth,ni,body.as_bytes(),&l).await.unwrap();tx.rollback().await.unwrap();drop(b);drop(l);
  let mp=root.join(".canonical-rewrite-journal").join(nr.op_id()).join("manifest.json");let raw=std::fs::read(&mp).unwrap();let mut changed:serde_json::Value=serde_json::from_slice(&raw).unwrap();changed["actor_id"]=serde_json::json!("foreign-actor");std::fs::write(&mp,serde_json::to_vec(&changed).unwrap()).unwrap();
  assert!(dispose_replay(&pool,&root,&negative).await.is_err());assert_eq!(charged(&pool,&negative).await,"reserved");assert!(root.join(&bpath).exists());
  std::fs::write(&mp,&raw).unwrap();let staged=mp.with_file_name("payload");let held_stage=mp.with_file_name("original-payload-held");std::fs::rename(&staged,&held_stage).unwrap();std::fs::copy(&held_stage,&staged).unwrap();assert!(dispose_replay(&pool,&root,&negative).await.is_err());assert_eq!(charged(&pool,&negative).await,"reserved");std::fs::remove_file(&staged).unwrap();std::fs::rename(&held_stage,&staged).unwrap();
  let rp=root.join(nr.path());let original_result=rp.with_extension("original");std::fs::rename(&rp,&original_result).unwrap();std::fs::copy(&original_result,&rp).unwrap();
  assert!(dispose_replay(&pool,&root,&negative).await.is_err());assert_eq!(charged(&pool,&negative).await,"reserved");assert!(rp.exists()&&original_result.exists()&&root.join(&bpath).exists());
  pool.close().await;println!("REPLAY_DISPOSAL_ORIGINAL_CAPSULE_NATIVE_BLOB_AND_PROTECTED_ABORTED_REWRITE_PASS {:?}",engine);
 }
 #[tokio::test]
 #[ignore="requires private disposable SQLite or PostgreSQL fixture"]
 async fn required_replay_disposal_crash_windows() {
  use crate::storage::{asset_blob,attachment_capacity as capacity};let (pool,engine,schema)=pool().await;
  for point in ["replay-blob-origin-durable","replay-blob-partial","replay-intent-durable","replay-payload-unlinked","replay-tombstone-durable","replay-sql-cas-uncommitted"] {
   let root=root();std::fs::write(root.join("fixture.md"),b"ORIGINAL PAGE").unwrap();let attempt=uuid::Uuid::new_v4().to_string();
   let p=Producer::Replay{draft_scope:None,provider:"private-provider".into(),incoming_id:format!("kill-{point}"),actor_id:"cleanup-owner".into(),space_id:"cleanup-space".into(),document_id:"cleanup-page".into(),asset_id:"opaque-asset".into(),payload_fingerprint:"a".repeat(64),binary_sha256:digest(b"REPLAY BINARY"),byte_length:13};
   let lease=WriterLease::begin(&root,&attempt,p,64).unwrap();capacity::reserve(&pool,&lease,None).await.unwrap();
   let adapter=if point.starts_with("replay-blob-") {"replay-blob"}else{
    let blob=asset_blob::create_with_lease(&root,"cleanup","cleanup-page","bin",b"REPLAY BINARY",lease.clone()).unwrap();drop(blob);"replay"};drop(lease);
   let mut child=std::process::Command::new(std::env::current_exe().unwrap()).args(["--ignored","--exact","storage::attachment_upload::admission::sql_controls::required_disposal_crash_child","--nocapture","--test-threads=1"])
    .env("STARLANE_DISPOSAL_TEST_SCHEMA",&schema).env("STARLANE_DISPOSAL_TEST_ROOT",&root).env("STARLANE_DISPOSAL_TEST_ATTEMPT",&attempt).env("STARLANE_DISPOSAL_TEST_CHECKPOINT",point).env("STARLANE_DISPOSAL_TEST_ADAPTER",adapter)
    .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
   let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
   while !root.join("checkpoint-ready").exists() {if let Some(status)=child.try_wait().unwrap(){panic!("replay checkpoint exited early {point}: {status}");}if std::time::Instant::now()>deadline{let _=child.kill();panic!("replay checkpoint timeout {point}");}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
   assert!(WriterLease::open_idle(&root,&attempt).is_err());child.kill().unwrap();child.wait().unwrap();assert_eq!(charged(&pool,&attempt).await,"reserved");
   let sha=dispose_replay(&pool,&root,&attempt).await.unwrap();assert_eq!(charged(&pool,&attempt).await,"disposed");assert_eq!(sha,dispose_replay(&pool,&root,&attempt).await.unwrap());
   let l=WriterLease::open_idle(&root,&attempt).unwrap();let (_,raw)=bytes(&l.0.directory,"replay-disposal-tombstone.json").unwrap();let plan:ReplayDisposal=decode(&raw).unwrap();let origin:ReplayBlobOrigin=decode(plan.origin_json.unwrap().as_bytes()).unwrap();assert!(!root.join(origin.path).exists());
   assert_eq!(std::fs::read(root.join("fixture.md")).unwrap(),b"ORIGINAL PAGE");println!("ACTUAL_SIGKILL_REPLAY_CHECKPOINT_PASS {:?} {}",engine,point);
  }pool.close().await;
 }

 async fn replay_rewrite_child(pool:&sqlx::AnyPool,root:&Path,attempt:&str) {
  use crate::storage::canonical_rewrite::{self,IncomingIdentity,RewriteIntent,RewriteReason};
  let l=WriterLease::open_idle(root,attempt).unwrap();let Producer::Replay{provider,incoming_id,actor_id,space_id,document_id,asset_id,payload_fingerprint,..}=l.producer() else{panic!("wrong producer")};
  let (_,raw)=bytes(&l.0.directory,"replay-blob-origin.json").unwrap();let origin:ReplayBlobOrigin=decode(&raw).unwrap();let result=format!("ORIGINAL PAGE [file](/api/v1/files/{})",origin.path);
  let auth=crate::auth::middleware::AuthUser{claims:crate::auth::jwt::Claims{sub:actor_id.clone(),sid:"cleanup-session".into(),email:"cleanup@example.test".into(),role:"member".into(),exp:4102444800,iat:0,token_type:crate::auth::jwt::TokenType::Access},api_key_scopes:None};
  let intent=RewriteIntent{space_id:space_id.clone(),document_id:document_id.clone(),source_path:"spaces/cleanup/docs/original.md".into(),source_revision:"source".into(),published_stamp:"published".into(),revision:"new".into(),source_sha256:Some(digest(b"ORIGINAL PAGE")),reason:RewriteReason::AssetReferenceRewrite,
   incoming:IncomingIdentity{provider:provider.clone(),operation_id:incoming_id.clone(),entity_type:"asset".into(),entity_id:asset_id.clone(),action:"create".into(),payload_fingerprint:payload_fingerprint.clone()}};
  let mut tx=pool.begin().await.unwrap();crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx,&[space_id.as_str()]).await.unwrap();
  sqlx::query("UPDATE documents SET id=id WHERE id=$1").bind(document_id).execute(&mut *tx).await.unwrap();std::env::set_var("DATA_DIR",root);
  canonical_rewrite::prepare_asset_replay_with_lease(&mut tx,&auth,intent,result.as_bytes(),&l).await.unwrap();panic!("expected rewrite checkpoint kill");
 }
 #[tokio::test]
 #[ignore="requires private owned two-engine fixture"]
 async fn required_incomplete_replay_rewrite_crash_windows() {
  use crate::storage::{asset_blob,attachment_capacity as capacity};let(pool,engine,schema)=pool().await;
  sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('cleanup-member','cleanup-space','cleanup-owner','owner')").execute(&pool).await.unwrap();
  sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('cleanup-session','cleanup-owner','test','2999-01-01T00:00:00Z')").execute(&pool).await.unwrap();
  for point in ["replay-rewrite-origin-durable","replay-rewrite-partial","replay-rewrite-manifest-durable"] {
   let root=root();std::fs::create_dir_all(root.join("spaces/cleanup/docs")).unwrap();std::fs::write(root.join("spaces/cleanup/docs/original.md"),b"ORIGINAL PAGE").unwrap();
   sqlx::query("UPDATE documents SET content_path='spaces/cleanup/docs/original.md',updated_at='source',published_at='published' WHERE id='cleanup-page'").execute(&pool).await.unwrap();
   let attempt=uuid::Uuid::new_v4().to_string();let incoming=format!("incomplete-{point}");let l=WriterLease::begin(&root,&attempt,Producer::Replay{draft_scope:None,provider:"private-provider".into(),incoming_id:incoming,actor_id:"cleanup-owner".into(),space_id:"cleanup-space".into(),document_id:"cleanup-page".into(),asset_id:"opaque-client-asset".into(),payload_fingerprint:"a".repeat(64),binary_sha256:digest(b"REPLAY BINARY"),byte_length:13},64).unwrap();
   capacity::reserve(&pool,&l,None).await.unwrap();let blob=asset_blob::create_with_lease(&root,"cleanup","cleanup-page","bin",b"REPLAY BINARY",l.clone()).unwrap();let blob_path=blob.path().to_owned();drop(blob);drop(l);
   let mut child=std::process::Command::new(std::env::current_exe().unwrap()).args(["--ignored","--exact","storage::attachment_upload::admission::sql_controls::required_disposal_crash_child","--nocapture","--test-threads=1"])
    .env("STARLANE_DISPOSAL_TEST_SCHEMA",&schema).env("STARLANE_DISPOSAL_TEST_ROOT",&root).env("STARLANE_DISPOSAL_TEST_ATTEMPT",&attempt).env("STARLANE_DISPOSAL_TEST_ADAPTER","replay-rewrite").env("STARLANE_DISPOSAL_TEST_CHECKPOINT",point).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
   let deadline=std::time::Instant::now()+std::time::Duration::from_secs(20);while !root.join("checkpoint-ready").exists(){if let Some(code)=child.try_wait().unwrap(){panic!("rewrite child exited {point}: {code}");}if std::time::Instant::now()>deadline{child.kill().unwrap();panic!("rewrite child timeout {point}");}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
   assert!(WriterLease::open_idle(&root,&attempt).is_err());child.kill().unwrap();child.wait().unwrap();assert_eq!(charged(&pool,&attempt).await,"reserved");
   let l=WriterLease::open_idle(&root,&attempt).unwrap();let (_,raw)=bytes(&l.0.directory,"replay-rewrite-origin.json").unwrap();let origin:ReplayRewriteOrigin=decode(&raw).unwrap();let dir=root.join(".canonical-rewrite-journal").join(&origin.op_id);let payload=dir.join("payload");let original=std::fs::read(&payload).unwrap();let manifest=std::fs::read(dir.join("manifest.json")).ok();drop(l);
   std::fs::rename(&payload,dir.join("original-held")).unwrap();std::fs::write(&payload,&original).unwrap();std::fs::set_permissions(&payload,std::fs::metadata(dir.join("original-held")).unwrap().permissions()).unwrap();assert!(dispose_replay(&pool,&root,&attempt).await.is_err());assert_eq!(charged(&pool,&attempt).await,"reserved");std::fs::remove_file(&payload).unwrap();std::fs::rename(dir.join("original-held"),&payload).unwrap();
   let namespace=root.join(".canonical-rewrite-journal");let held_namespace=root.join(".canonical-rewrite-journal-held");std::fs::rename(&namespace,&held_namespace).unwrap();std::fs::create_dir(&namespace).unwrap();std::fs::set_permissions(&namespace,std::fs::metadata(&held_namespace).unwrap().permissions()).unwrap();std::fs::rename(held_namespace.join(&origin.op_id),namespace.join(&origin.op_id)).unwrap();
   assert!(dispose_replay(&pool,&root,&attempt).await.is_err());assert_eq!(charged(&pool,&attempt).await,"reserved");std::fs::rename(namespace.join(&origin.op_id),held_namespace.join(&origin.op_id)).unwrap();std::fs::remove_dir(namespace).unwrap();std::fs::rename(held_namespace,root.join(".canonical-rewrite-journal")).unwrap();
   let origin_path=root.join(".attachment-upload-admission").join(&attempt).join("replay-rewrite-origin.json");std::fs::rename(&origin_path,origin_path.with_extension("held")).unwrap();assert!(dispose_replay(&pool,&root,&attempt).await.is_err());assert_eq!(charged(&pool,&attempt).await,"reserved");std::fs::rename(origin_path.with_extension("held"),&origin_path).unwrap();
   let sha=dispose_replay(&pool,&root,&attempt).await.unwrap();assert_eq!(charged(&pool,&attempt).await,"disposed");assert_eq!(sha,dispose_replay(&pool,&root,&attempt).await.unwrap());assert!(!root.join(&blob_path).exists());assert_eq!(std::fs::read(payload).unwrap(),original);assert_eq!(std::fs::read(dir.join("manifest.json")).ok(),manifest);assert!(!dir.join("install-proof.json").exists());assert_eq!(std::fs::read(root.join("spaces/cleanup/docs/original.md")).unwrap(),b"ORIGINAL PAGE");
   println!("ACTUAL_SIGKILL_INCOMPLETE_REPLAY_ORIGINAL_STAGE_FOREIGN_INODE_MISSING_CUSTODY_REFUSAL_PASS {:?} {point}",engine);
  }pool.close().await;
 }

 #[tokio::test]
 #[ignore="requires private disposable SQLite or PostgreSQL fixture"]
 async fn required_classified_admission_source_engine() {
  use crate::storage::{attachment_upload as upload,attachment_capacity as capacity,restoration::admission::{CapturedReservation,capture::NativeCaptureLeases}};
  let (pool,engine,_)=pool().await;let root=root();use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&root,std::fs::Permissions::from_mode(0o700)).unwrap();
  std::fs::write(root.join("fixture.md"),b"Original body without blob references").unwrap();std::fs::set_permissions(root.join("fixture.md"),std::fs::Permissions::from_mode(0o600)).unwrap();
  let direct=uuid::Uuid::new_v4().to_string();let request=request();let lease=WriterLease::begin(&root,&direct,Producer::Direct{request:request.clone()},64).unwrap();capacity::reserve(&pool,&lease,None).await.unwrap();
  let mut ingest=upload::Ingest::reserve_with_lease(&root,request,64,lease.clone()).unwrap();ingest.append(b"partial direct").unwrap();drop(ingest);drop(lease);
  let replay=uuid::Uuid::new_v4().to_string();let binary=b"REPLAY BINARY";
  let producer=Producer::Replay{draft_scope:None,provider:"private-provider".into(),incoming_id:"opaque-incoming".into(),actor_id:"cleanup-owner".into(),space_id:"cleanup-space".into(),document_id:"cleanup-page".into(),asset_id:"opaque-client-asset".into(),payload_fingerprint:"a".repeat(64),binary_sha256:digest(binary),byte_length:binary.len() as u64};
  let lease=WriterLease::begin(&root,&replay,producer,64).unwrap();capacity::reserve(&pool,&lease,None).await.unwrap();let blob=crate::storage::asset_blob::create_with_lease(&root,"cleanup","cleanup-page","bin",binary,lease.clone()).unwrap();drop(blob);drop(lease);
  let mut tx=pool.begin().await.unwrap();let rows=sqlx::query("SELECT operation_id,request_sha256,admission_claim_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,disposition_json,disposition_sha256 FROM attachment_capacity_reservations ORDER BY operation_id").fetch_all(&mut *tx).await.unwrap();use sqlx::Row;
  let expected:Vec<CapturedReservation>=rows.into_iter().map(|r|CapturedReservation{operation_id:r.get(0),request_sha256:r.get(1),admission_claim_sha256:r.get(2),maximum_bytes:r.get(3),state:r.get(4),committed_asset_id:r.get(5),committed_bytes:r.get(6),disposition_json:r.get(7),disposition_sha256:r.get(8)}).collect();
  let capture=NativeCaptureLeases::hold(&root,&mut *tx,&expected).await.unwrap();let classified=capture.classify(&mut tx).await.unwrap();classified.fence().unwrap();assert_eq!(classified.source_roles().scopes.len(),2);
  assert!(WriterLease::open_idle(&root,&direct).is_err());assert!(WriterLease::open_idle(&root,&replay).is_err());
  let payload=root.join(stage::NAMESPACE).join(&direct).join("payload.part");let retained=payload.with_extension("original");std::fs::rename(&payload,&retained).unwrap();std::fs::copy(&retained,&payload).unwrap();std::fs::set_permissions(&payload,std::fs::Permissions::from_mode(0o600)).unwrap();assert!(classified.fence().is_err(),"equal-byte foreign source payload must refuse");
  std::fs::remove_file(&payload).unwrap();std::fs::rename(&retained,&payload).unwrap();classified.fence().unwrap();
  let origin=root.join(NS).join(&direct).join("stage-origin.json");let kept=origin.with_extension("original");std::fs::rename(&origin,&kept).unwrap();std::fs::copy(&kept,&origin).unwrap();std::fs::set_permissions(&origin,std::fs::Permissions::from_mode(0o600)).unwrap();assert!(classified.fence().is_err(),"equal-byte foreign raw origin must refuse");
  drop(classified);drop(capture);tx.rollback().await.unwrap();pool.close().await;
  println!("CLASSIFIED_ADMISSION_NATIVE_DIRECT_REPLAY_FULL_SQL_ORIGINAL_ROLES_FOREIGN_PAYLOAD_ORIGIN_REFUSAL_PASS {:?}",engine);
 }
 #[tokio::test]
 #[ignore="requires private disposable SQLite or PostgreSQL fixture"]
 async fn required_supported_capture_retains_all_engine() {
  use crate::storage::{attachment_upload as upload,attachment_capacity as capacity,restoration::admission};
  use std::os::unix::fs::PermissionsExt;
  let (pool,engine,_schema)=pool().await;let root=root();std::fs::set_permissions(&root,std::fs::Permissions::from_mode(0o700)).unwrap();
  let mut attempts=Vec::new();let mut rows=Vec::new();
  for n in 0..3 {
   let attempt=uuid::Uuid::new_v4().to_string();let req=request();let writer=WriterLease::begin(&root,&attempt,Producer::Direct{request:req.clone()},64).unwrap();capacity::reserve(&pool,&writer,None).await.unwrap();
   if n==0 {let mut ingest=upload::Ingest::reserve_with_lease(&root,req,64,writer.clone()).unwrap();ingest.append(b"original supported stage").unwrap();drop(ingest);}
   if n==1 {sqlx::query("UPDATE attachment_capacity_reservations SET state='committed',committed_asset_id='retained-opaque-asset',committed_bytes=7 WHERE operation_id=$1").bind(&attempt).execute(&pool).await.unwrap();}
   rows.push(admission::CapturedReservation{operation_id:attempt.clone(),request_sha256:writer.producer_sha256().to_owned(),admission_claim_sha256:writer.claim_sha256(),maximum_bytes:64,state:if n==1{"committed"}else{"reserved"}.into(),committed_asset_id:if n==1{Some("retained-opaque-asset".into())}else{None},committed_bytes:if n==1{Some(7)}else{None},disposition_json:None,disposition_sha256:None});drop(writer);attempts.push(attempt);
  }
  let mut tx=pool.begin().await.unwrap();let leases=admission::capture::NativeCaptureLeases::hold(&root,&mut *tx,&rows).await.unwrap();
  let classified=leases.classify_supported(&mut tx).await.unwrap();let envelope=classified.capture_envelope().unwrap();assert_eq!(envelope.scopes.len(),1);assert_eq!(envelope.scopes[0].attempt_id,attempts[0]);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM attachment_capacity_reservations").fetch_one(&mut *tx).await.unwrap(),3);
  for attempt in &attempts {assert!(WriterLease::open_idle(&root,attempt).is_err(),"ALL omitted and supported source capsules retain kernel exclusion");}
  leases.recheck(&mut *tx).await.unwrap();classified.fence().unwrap();
  let terminal=root.join(format!(".attachment-upload-admission/{}/stage-origin.json",attempts[1]));std::fs::write(&terminal,b"malformed original record").unwrap();std::fs::set_permissions(&terminal,std::fs::Permissions::from_mode(0o600)).unwrap();assert!(leases.classify_supported(&mut tx).await.is_err(),"unsupported terminal cannot hide malformed proof");assert!(classified.fence().is_err());
  drop(classified);drop(leases);tx.rollback().await.unwrap();let states:Vec<String>=sqlx::query_scalar("SELECT state FROM attachment_capacity_reservations ORDER BY operation_id").fetch_all(&pool).await.unwrap();assert_eq!(states.iter().filter(|s|s.as_str()=="reserved").count(),2);assert_eq!(states.iter().filter(|s|s.as_str()=="committed").count(),1);pool.close().await;
  println!("ADMISSION_SUPPORTED_SUBSET_ALL_NATIVE_LEASES_FULL_SQL_TERMINAL_PREORIGIN_CHARGED_MALFORMED_REFUSAL_PASS {engine:?}");
 }

 #[tokio::test]
 #[ignore="requires owned disposable SQLite/PostgreSQL fixture"]
 async fn required_cold_replay_original_protected_engine() {
  use crate::storage::{attachment_capacity as capacity,restoration::admission::{CapturedReservation,capture::NativeCaptureLeases},canonical_rewrite::{RewriteIntent,RewriteReason,IncomingIdentity}};
  use std::os::unix::fs::{PermissionsExt,DirBuilderExt,OpenOptionsExt};use std::io::Write;use sqlx::Row;
  let (pool,engine,_)=pool().await;let root=root();std::fs::set_permissions(&root,std::fs::Permissions::from_mode(0o700)).unwrap();let source_path="spaces/cleanup/docs/source.md";std::fs::DirBuilder::new().recursive(true).mode(0o700).create(root.join("spaces/cleanup/docs")).unwrap();let source_bytes=b"Original published canonical body";std::fs::write(root.join(source_path),source_bytes).unwrap();std::fs::set_permissions(root.join(source_path),std::fs::Permissions::from_mode(0o600)).unwrap();sqlx::query("UPDATE documents SET content_path=$1,updated_at='source-revision',published_at='published-stamp' WHERE id='cleanup-page'").bind(source_path).execute(&pool).await.unwrap();
  let attempt=uuid::Uuid::new_v4().to_string();let binary=b"ORIGINAL REPLAY BLOB";let producer=Producer::Replay{draft_scope:None,provider:"opaque-provider".into(),incoming_id:"opaque-incoming".into(),actor_id:"cleanup-owner".into(),space_id:"cleanup-space".into(),document_id:"cleanup-page".into(),asset_id:"opaque-native-asset".into(),payload_fingerprint:"a".repeat(64),binary_sha256:digest(binary),byte_length:binary.len() as u64};let lease=WriterLease::begin(&root,&attempt,producer,64).unwrap();capacity::reserve(&pool,&lease,None).await.unwrap();let blob=crate::storage::asset_blob::create_with_lease(&root,"cleanup","cleanup-page","bin",binary,lease.clone()).unwrap();let blob_path=blob.path().to_owned();drop(blob);
  let op=uuid::Uuid::new_v4().to_string();let result=format!("Original with /api/v1/files/{blob_path}");let i=RewriteIntent{space_id:"cleanup-space".into(),document_id:"cleanup-page".into(),source_path:source_path.into(),source_revision:"source-revision".into(),published_stamp:"published-stamp".into(),revision:"result-revision".into(),source_sha256:Some(digest(source_bytes)),reason:RewriteReason::AssetReferenceRewrite,incoming:IncomingIdentity{provider:"opaque-provider".into(),operation_id:"opaque-incoming".into(),entity_type:"asset".into(),entity_id:"opaque-native-asset".into(),action:"create".into(),payload_fingerprint:"a".repeat(64)}};lease.bind_replay_rewrite_plan(&root,&op,"cleanup-owner",&i,result.as_bytes()).unwrap();
  let journal=root.join(format!(".canonical-rewrite-journal/{op}"));std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&journal).unwrap();let mut payload=std::fs::OpenOptions::new().read(true).write(true).create_new(true).mode(0o600).open(journal.join("payload")).unwrap();let parent=std::fs::File::open(&journal).unwrap();let manifest=serde_json::json!({"version":3,"op_id":op,"space_id":"cleanup-space","document_id":"cleanup-page","path":format!("spaces/cleanup/docs/revisions/{op}.md"),"revision":"result-revision","sha256":digest(result.as_bytes()),"length":result.len(),"kind":"canonical-rewrite","actor_id":"cleanup-owner","source_path":source_path,"source_revision":"source-revision","published_stamp":"published-stamp","source_sha256":digest(source_bytes),"reason":"AssetReferenceRewrite","incoming":i.incoming});let raw_manifest=serde_json::to_vec(&manifest).unwrap();let descriptor:crate::storage::journal_fs::FileDescriptor=serde_json::from_value(manifest.clone()).unwrap();lease.bind_replay_rewrite_original(&root,&descriptor,&raw_manifest,&parent,&payload).unwrap();payload.write_all(b"Original with ").unwrap();payload.sync_all().unwrap();drop(payload);drop(parent);drop(lease);
  let mut tx=pool.begin().await.unwrap();let r=sqlx::query("SELECT operation_id,request_sha256,admission_claim_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,disposition_json,disposition_sha256 FROM attachment_capacity_reservations").fetch_one(&mut *tx).await.unwrap();let expected=CapturedReservation{operation_id:r.get(0),request_sha256:r.get(1),admission_claim_sha256:r.get(2),maximum_bytes:r.get(3),state:r.get(4),committed_asset_id:r.get(5),committed_bytes:r.get(6),disposition_json:r.get(7),disposition_sha256:r.get(8)};let capture=NativeCaptureLeases::hold(&root,&mut *tx,&[expected]).await.unwrap();let classified=capture.classify(&mut tx).await.unwrap();let scope=classified.source_roles().scopes[0].clone();let raws=scope.files.iter().map(|b|(b.role.clone(),std::fs::read(root.join(b.role.relative(&attempt).unwrap())).unwrap())).collect::<std::collections::BTreeMap<_,_>>();let proof=validate_captured_replay_custody(&scope,&raws).unwrap();
  let read=|path:&str|->io::Result<Vec<u8>>{std::fs::read(root.join(path))};let mut inventory=scope.files.iter().map(|b|b.role.relative(&attempt).unwrap()).collect::<std::collections::BTreeSet<_>>();inventory.insert(source_path.into());proof.validate_references(&mut *tx,&inventory,&read).await.unwrap();classified.fence().unwrap();
  let mut wrong=raws.clone();let role=crate::storage::restoration::admission::FileRole::CapsuleRecord{name:"replay-blob-origin.json".into()};let mut origin:serde_json::Value=serde_json::from_slice(&wrong[&role]).unwrap();origin["payload"]["inode"]=serde_json::json!(999);wrong.insert(role,serde_json::to_vec(&origin).unwrap());assert!(validate_captured_replay_custody(&scope,&wrong).is_err(),"Selfconsistent foreign original blob origin refuses");
  let mut wrong=raws.clone();let role=crate::storage::restoration::admission::FileRole::CapsuleRecord{name:"replay-rewrite-origin.json".into()};let mut origin:serde_json::Value=serde_json::from_slice(&wrong[&role]).unwrap();origin["parent"]["inode"]=serde_json::json!(999);wrong.insert(role,serde_json::to_vec(&origin).unwrap());assert!(validate_captured_replay_custody(&scope,&wrong).is_err(),"Foreign original incomplete rewrite directory refuses");
  sqlx::query("INSERT INTO attachment_retained_byte_ledger(asset_id,byte_length,filename,original_name,mime_type,uploader_id,original_created_at,original_storage_path,evidence_kind) VALUES('retained-other',19,'blob.bin','original.bin','application/octet-stream','cleanup-owner','original-stamp',$1,'legacy_upgrade')").bind(&blob_path).execute(&mut *tx).await.unwrap();assert!(proof.validate_references(&mut *tx,&inventory,&read).await.is_err(),"Retained accounting observation protects candidate blob");sqlx::query("DELETE FROM attachment_retained_byte_ledger WHERE asset_id='retained-other'").execute(&mut *tx).await.unwrap();
  std::fs::write(root.join(source_path),format!("Current source /api/v1/files/{blob_path}")).unwrap();assert!(proof.validate_references(&mut *tx,&inventory,&read).await.is_err(),"Current canonical references refuse");std::fs::write(root.join(source_path),source_bytes).unwrap();
  let other=uuid::Uuid::new_v4().to_string();let other_path=format!(".canonical-rewrite-journal/{other}/payload");std::fs::DirBuilder::new().recursive(true).mode(0o700).create(root.join(format!(".canonical-rewrite-journal/{other}"))).unwrap();std::fs::write(root.join(&other_path),format!("Protected source /api/v1/files/{blob_path}")).unwrap();inventory.insert(other_path.clone());assert!(proof.validate_references(&mut *tx,&inventory,&read).await.is_err(),"Other protected canonical stage refuses");inventory.remove(&other_path);std::fs::remove_dir_all(root.join(format!(".canonical-rewrite-journal/{other}"))).unwrap();
  sqlx::query("UPDATE documents SET updated_at='changed-source' WHERE id='cleanup-page'").execute(&mut *tx).await.unwrap();assert!(proof.validate_references(&mut *tx,&inventory,&read).await.is_err(),"Original canonical source SQL substitution refuses");sqlx::query("UPDATE documents SET updated_at='source-revision' WHERE id='cleanup-page'").execute(&mut *tx).await.unwrap();proof.validate_references(&mut *tx,&inventory,&read).await.unwrap();
  assert_eq!(sqlx::query_scalar::<_,String>("SELECT state FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&attempt).fetch_one(&mut *tx).await.unwrap(),"reserved");drop(classified);drop(capture);tx.rollback().await.unwrap();pool.close().await;
  println!("COLD_REPLAY_ORIGINAL_BLOB_INCOMPLETE_REWRITE_PROTECTED_CANONICAL_SQL_RETAINED_BYTES_FOREIGN_ORIGIN_REFUSAL_PASS {engine:?}");
 }

}
