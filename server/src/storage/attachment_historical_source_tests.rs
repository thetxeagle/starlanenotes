//! Source-only mechanism gate. No cold/accounting/application success claim.
use super::*;
async fn opaque_ws(pool:&sqlx::AnyPool,base:&str,token:&str,document:&str){
 use tokio::io::{AsyncWriteExt,AsyncReadExt};let address=base.strip_suffix("/api/v1").unwrap().strip_prefix("http://").unwrap();let mut socket=tokio::net::TcpStream::connect(address).await.unwrap();socket.write_all(format!("GET /ws/sync HTTP/1.1\r\nHost: {address}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n").as_bytes()).await.unwrap();let mut header=Vec::new();while !header.ends_with(b"\r\n\r\n"){assert!(header.len()<8192);header.push(socket.read_u8().await.unwrap());}assert!(header.starts_with(b"HTTP/1.1 101"));
 async fn send(s:&mut tokio::net::TcpStream,text:&str){use tokio::io::AsyncWriteExt;let b=text.as_bytes();assert!(b.len()<=8192);let mut f=vec![0x81];if b.len()<126{f.push(0x80|b.len()as u8)}else{f.push(0x80|126);f.extend_from_slice(&(b.len()as u16).to_be_bytes())}let mask=[17,39,71,103];f.extend_from_slice(&mask);f.extend(b.iter().enumerate().map(|(i,b)|b^mask[i%4]));s.write_all(&f).await.unwrap();}
 async fn receive(s:&mut tokio::net::TcpStream)->(u8,Vec<u8>){use tokio::io::{AsyncReadExt,AsyncWriteExt};loop{let op=s.read_u8().await.unwrap();assert!(op&0x80!=0&&op&15!=8,"Owned WS closed or fragmented before expected JSON");let size=s.read_u8().await.unwrap();assert_eq!(size&0x80,0);let n=match size&127{126=>s.read_u16().await.unwrap()as usize,127=>s.read_u64().await.unwrap()as usize,n=>n as usize};assert!(n<=65536);let mut b=vec![0;n];s.read_exact(&mut b).await.unwrap();match op&15{9=>{assert!(n<=125);let mask=[17,39,71,103];let mut frame=vec![0x8a,0x80|n as u8];frame.extend_from_slice(&mask);frame.extend(b.iter().enumerate().map(|(i,b)|b^mask[i%4]));s.write_all(&frame).await.unwrap();},10=>{},1|2=>return (op&15,b),_=>panic!("Unexpected registered WS opcode")}}}

 send(&mut socket,token).await;let(op,auth)=receive(&mut socket).await;assert_eq!(op,1,"Authentication must return registered JSON text");let auth:Value=serde_json::from_slice(&auth).unwrap();assert_eq!(auth["status"],"authenticated");let original_state:Option<Vec<u8>>=sqlx::query_scalar("SELECT automerge_state FROM documents WHERE id=$1").bind(document).fetch_one(pool).await.unwrap();send(&mut socket,&json!({"msg_type":"subscribe","doc_id":document,"user_id":"untrusted-attribution","payload":{}}).to_string()).await;let(op,snapshot)=receive(&mut socket).await;assert_eq!(op,2,"Subscription must return the registered raw binary state");assert_eq!(snapshot,original_state.unwrap_or_default(),"Actual subscribe snapshot must equal source raw bytes");let opaque=json!({"retained_opaque_history":[17,23,255]});send(&mut socket,&json!({"msg_type":"change","doc_id":document,"user_id":"untrusted-attribution","payload":opaque}).to_string()).await;
 let mut seen=false;for _ in 0..500{let found:Vec<Vec<u8>>=sqlx::query_scalar("SELECT change_set FROM doc_versions WHERE document_id=$1").bind(document).fetch_all(pool).await.unwrap();if found.iter().any(|b|serde_json::from_slice::<Value>(b).ok().as_ref()==Some(&opaque)){seen=true;break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}assert!(seen,"Registered WebSocket failed to retain original opaque bytes");socket.shutdown().await.unwrap();
}
pub(super) async fn actual(root:&Path,pool:&sqlx::AnyPool,client:&reqwest::Client,base:&str,owner:&str,document:&str,pending:&str,body:&[u8]){
 let before=rows(root,pool).await;let before_records=records(root);tokio::time::timeout(std::time::Duration::from_secs(10),opaque_ws(pool,base,owner,document)).await.expect("Actual registered opaque WS source timed out");let raw=std::fs::read(root.join(".attachment-upload-admission").join(pending).join("claim.json")).unwrap();let claim:Value=serde_json::from_slice(&raw).unwrap();let response=client.post(format!("{base}/spaces/space/documents/{document}/attachment-upload-attempts/{pending}/cleanup")).bearer_auth(owner).json(&json!({"producer_sha256":claim["producer_sha256"],"claim_sha256":storage::attachment_upload::digest(&raw)})).send().await.unwrap();let status=response.status();let response:Value=response.json().await.unwrap();assert!(status==200&&response["outcome"]=="retained_review","Opaque cleanup must retain current unknown references status={status} body={response}");assert_eq!(std::fs::read(root.join(".attachment-upload-journal").join(pending).join("payload.part")).unwrap(),body);assert_eq!(rows(root,pool).await,before);assert_eq!(records(root),before_records);
 let mut tx=pool.begin().await.unwrap();let strict=storage::attachment_upload::admission::committed_erasure::accounting::composite::CompositeOriginalSource::hold_in(root,&mut tx).await;assert!(strict.as_ref().is_err_and(|e|format!("{e:#}").contains("Completed restored current protected references differ")),"Default source must retain exact current-reference refusal");drop(strict);tx.rollback().await.unwrap();
 let mut tx=pool.begin().await.unwrap();let held=storage::restoration::linux::HistoricalCompositeSource::hold_in(root,&mut tx).await.unwrap();held.recheck_in(&mut tx).await.unwrap();let evidence=held.capture_evidence_in(&mut tx).await.unwrap();evidence.validate_shape().unwrap();assert_eq!(evidence.native.len(),3);assert_eq!(held.reference_semantics(),storage::restoration::historical::HistoricalReferenceSemantics::NotEstablished);let pending_scope=evidence.native.iter().find(|s|s.reservation.operation_id==pending).unwrap();assert_eq!(pending_scope.reservation.maximum_bytes,MAXIMUM);assert_eq!(pending_scope.reservation.state,"reserved");assert_eq!(pending_scope.claim_json.as_bytes(),raw);if let Ok(control)=std::env::var("HISTORICAL_COMPOSITE_SOURCE_CONTROL"){
  use std::os::unix::fs::OpenOptionsExt;
  let erased=&evidence.prior.roles.scopes[0];let erased_asset=&erased.original.disposition["asset_id"];
  match control.as_str(){
   "foreign_connected_sql"=>{
    let url=if std::env::var("REPLAY_CAPACITY_TEST_PG_URL").is_ok(){std::env::var("REPLAY_CAPACITY_TEST_PG_URL").unwrap()}else{"sqlite::memory:".into()};
    let foreign=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&url).await.unwrap();let mut conn=foreign.acquire().await.unwrap();
    assert!(held.recheck_in(&mut conn).await.is_err(),"Historical source accepted foreign connected SQL target");drop(conn);foreign.close().await;drop(held);tx.rollback().await.unwrap();assert_eq!(rows(root,pool).await,before);assert_eq!(records(root),before_records);println!("HISTORICAL_SOURCE_CONTROL_foreign_connected_sql_REFUSAL_PASS; source_unchanged_no_cold_grant");return;
   },
   "wrong_root"=>{
    let wrong=root.parent().unwrap().join(format!("historical-wrong-root-{}",uuid::Uuid::new_v4()));std::os::unix::fs::DirBuilderExt::mode(&mut std::fs::DirBuilder::new(),0o700).create(&wrong).unwrap();
    assert!(storage::restoration::linux::HistoricalCompositeSource::hold_in(&wrong,&mut tx).await.is_err(),"Historical source accepted an unregistered wrong root");held.recheck_in(&mut tx).await.unwrap();drop(held);tx.rollback().await.unwrap();assert_eq!(rows(root,pool).await,before);assert_eq!(records(root),before_records);println!("HISTORICAL_SOURCE_CONTROL_wrong_root_REFUSAL_PASS; source_unchanged_no_cold_grant");return;
   },
   "foreign_equal_claim_inode"=>{let original=root.join(".attachment-upload-admission").join(pending).join("claim.json");let parent=root.parent().unwrap();let saved=parent.join("historical-original-claim-evidence.json");std::fs::rename(&original,&saved).unwrap();let mut f=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&original).unwrap();std::io::Write::write_all(&mut f,&raw).unwrap();f.sync_all().unwrap();},
   "recreated_erased_payload"=>{let metadata=storage::attachment_upload::admission::committed_erasure::accounting::capture::original_record_metadata(&erased.original).unwrap();let receipt=storage::attachment_upload::admission::committed_erasure::accounting::capture::validate_original_records(&erased.original,&metadata).unwrap();let path=root.join(&receipt.intent.storage_path);let mut f=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path).unwrap();std::io::Write::write_all(&mut f,b"foreign resurrected payload").unwrap();f.sync_all().unwrap();},
   "new_live_association"=>{sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at) SELECT $1,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at FROM attachments ORDER BY id LIMIT 1").bind(erased_asset).execute(&mut *tx).await.unwrap();},
   "unknown_application_table"=>{sqlx::query("CREATE TABLE historical_unprojected_application(value TEXT)").execute(&mut *tx).await.unwrap();},
   _=>panic!("Unknown explicit historical source control"),
  }
  assert!(held.recheck_in(&mut tx).await.is_err(),"Historical source accepted deliberate custody/SQL substitution: {control}");
  drop(held);tx.rollback().await.unwrap();println!("HISTORICAL_SOURCE_CONTROL_{control}_REFUSAL_PASS; preserved_disposable_mutation_no_cold_grant");return;
 }
 let encoded=serde_json::to_vec(&evidence).unwrap();let decoded:storage::restoration::historical::HistoricalCompositePreservationEvidence=serde_json::from_slice(&encoded).unwrap();assert_eq!(decoded,evidence);held.recheck_in(&mut tx).await.unwrap();drop(held);tx.rollback().await.unwrap();assert_eq!(rows(root,pool).await,before);assert_eq!(records(root),before_records);assert_eq!(std::fs::read(root.join(".attachment-upload-journal").join(pending).join("payload.part")).unwrap(),body);println!("ACTUAL_SOURCE_ONLY_HISTORICAL_LEDGER3_NEW_DIRECT_OPAQUE_WS_DEFAULT_REFUSAL_FULLROWS_PENDING_MAX_PRESERVATION_PASS; cold_and_accounting_unqualified");
}

pub(super) async fn cold_after_stop(root:&Path){
 assert!(!root.join(".pg-restoration-custody").exists(),"PG historical producer remains independently owned/unqualified");
 let output=root.with_file_name(format!("historical-standard-backup-{}",uuid::Uuid::new_v4()));
 let bundle=if std::env::var("HISTORICAL_READONLY_ADMISSION_TEST").as_deref()==Ok("true"){storage::restoration::linux::historical_readonly_tests::capture_original_observed(root,&output).await}else{storage::restoration::linux::capture_sqlite_historical_preservation(root,&output,"1.5.3",true).await.unwrap()};
 assert!(bundle.manifest.historical_composite_preservation_capture.is_some());
 assert!(storage::restoration_plan::plan(&bundle).await.is_err(),"Unqualified historical destination fell back to generic roles");
 let saved=root.with_file_name(format!("historical-original-source-kept-{}",uuid::Uuid::new_v4()));std::fs::rename(root,&saved).unwrap();assert!(!root.exists());
 storage::restoration::linux::inspect_historical_snapshot(&bundle).await.unwrap();
 let plan=storage::restoration_plan::plan_historical_preservation(&bundle).await.unwrap();
 let destination=output.with_file_name(format!("historical-certified-target-{}",uuid::Uuid::new_v4()));
 let copy=storage::restoration::reserve_and_copy(&bundle,&destination).unwrap();
 let target=storage::restoration::materialize_groups(copy,&bundle,plan).unwrap();
 storage::restoration::linux::inspect_historical_certified_target(&bundle,&target).await.unwrap();
 let sealed=storage::restoration::seal_publication(&bundle,target).await.unwrap();
 let committed=storage::restoration::commit_publication(sealed).await.unwrap();
 let ready=storage::restoration::retire_publication(committed).unwrap();ready.fence().unwrap();
 assert!(ready.path().join(".restoration-custody/current.json").is_file());
 let pool=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=rw",ready.path().join("starlane-notes.db").display())).await.unwrap();
 let mut conn=pool.acquire().await.unwrap();sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await.unwrap();
 let held=storage::restoration::linux::HistoricalPublishedObservation::hold_original_published(&ready,&mut conn).await.unwrap();
 let summary=held.summary().unwrap();assert_eq!(summary.completed_scopes,3);assert_eq!(summary.new_committed,1);assert_eq!(summary.new_pending,1);assert_eq!(summary.new_disposed,1);assert_eq!(summary.retained_pending_maximum,1024*1024);assert_eq!(summary.reference_semantics,storage::restoration::historical::HistoricalReferenceSemantics::NotEstablished);
 held.recheck_in(&mut conn).await.unwrap();
 let code=r#"import sqlite3,sys
c=sqlite3.connect(sys.argv[1],timeout=0.05)
try:
 c.execute('BEGIN IMMEDIATE')
 c.execute('UPDATE _sqlx_migrations SET execution_time=execution_time+1 WHERE version=1')
 c.commit()
 print('ACTUAL_TARGET_FOREIGN_PROCESS_COMMIT_ADMITTED')
 sys.exit(42)
except sqlite3.OperationalError as e:
 if 'locked' not in str(e).lower():raise
 c.rollback()
 print('ACTUAL_TARGET_FOREIGN_PROCESS_COMMIT_BLOCKED')
 sys.exit(10)
"#;
 let run=||std::process::Command::new("/usr/bin/python3").args(["-I","-c",code]).arg(ready.path().join("starlane-notes.db")).output().unwrap();
 let blocked=run();assert_eq!(blocked.status.code(),Some(10),"Original observer lost caller reservation: {} {}",String::from_utf8_lossy(&blocked.stdout),String::from_utf8_lossy(&blocked.stderr));
 held.recheck_in(&mut conn).await.unwrap();drop(held);sqlx::query("ROLLBACK").execute(&mut *conn).await.unwrap();drop(conn);pool.close().await;ready.fence().unwrap();
 let admitted=run();assert_eq!(admitted.status.code(),Some(42),"Released actual target writer remained blocked: {} {}",String::from_utf8_lossy(&admitted.stdout),String::from_utf8_lossy(&admitted.stderr));
 // Build a NEW genuine destination; the deliberately changed release-control
 // target is never reused as fresh-owner evidence.
 let fresh=output.with_file_name(format!("historical-independent-fresh-{}",uuid::Uuid::new_v4()));
 let plan=storage::restoration_plan::plan_historical_preservation(&bundle).await.unwrap();let copy=storage::restoration::reserve_and_copy(&bundle,&fresh).unwrap();let target=storage::restoration::materialize_groups(copy,&bundle,plan).unwrap();let sealed=storage::restoration::seal_publication(&bundle,target).await.unwrap();let committed=storage::restoration::commit_publication(sealed).await.unwrap();let ready_fresh=storage::restoration::retire_publication(committed).unwrap();ready_fresh.fence().unwrap();drop(ready_fresh);
 let child=std::process::Command::new(std::env::current_exe().unwrap()).env("HISTORICAL_FINALIZED_FRESH_ROOT",&fresh).env("DATA_DIR",&fresh).args(["actual_fresh_historical_physical_observation","--ignored","--nocapture","--test-threads=1"]).output().unwrap();
 let child_log=fresh.with_file_name(format!("historical-fresh-child-{}.log",uuid::Uuid::new_v4()));{use std::io::Write;use std::os::unix::fs::OpenOptionsExt;let mut f=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(child_log).unwrap();f.write_all(&child.stdout).unwrap();f.write_all(&child.stderr).unwrap();}
 assert!(String::from_utf8_lossy(&child.stdout).contains("ACTUAL_FRESH_HISTORICAL_CONSERVATIVE_LOGICAL_")&&String::from_utf8_lossy(&child.stdout).contains("ACTUAL_HISTORICAL_CONSERVATIVE_USAGE_RELEASED_FOREIGN_WRITER_COMMIT_PASS"),"Actual conservative accounting/foreign writer scope omitted: {} {}",String::from_utf8_lossy(&child.stdout),String::from_utf8_lossy(&child.stderr));println!("{}",String::from_utf8_lossy(&child.stdout));
 assert!(child.status.success()&&String::from_utf8_lossy(&child.stdout).contains("ACTUAL_INDEPENDENT_FRESH_REGISTERED_LEDGER5_FULL_SOURCE_CUSTODY_ROWS_ORIGINAL81_PROJECTION_NATIVE_PHYSICAL_OBSERVATION_PENDING_MAX_COPIED_WRITER_REFUSAL_PASS"),"Independent fresh historical owner failed: {} {}",String::from_utf8_lossy(&child.stdout),String::from_utf8_lossy(&child.stderr));
 for control in ["old_row","current_row","projection","foreign_equal_claim","recreated_payload","new_association","copied_root","foreign_connection","usage_missing_row","usage_extra_row","usage_state","usage_maximum"]{
  let target_path=output.with_file_name(format!("historical-fresh-control-{control}-{}",uuid::Uuid::new_v4()));let plan=storage::restoration_plan::plan_historical_preservation(&bundle).await.unwrap();let copy=storage::restoration::reserve_and_copy(&bundle,&target_path).unwrap();let d=storage::restoration::materialize_groups(copy,&bundle,plan).unwrap();let sealed=storage::restoration::seal_publication(&bundle,d).await.unwrap();let committed=storage::restoration::commit_publication(sealed).await.unwrap();let ready=storage::restoration::retire_publication(committed).unwrap();ready.fence().unwrap();drop(ready);
  let child=std::process::Command::new(std::env::current_exe().unwrap()).env("HISTORICAL_FINALIZED_FRESH_ROOT",&target_path).env("DATA_DIR",&target_path).env("HISTORICAL_FINALIZED_FRESH_CONTROL",control).args(["actual_fresh_historical_physical_observation","--ignored","--nocapture","--test-threads=1"]).output().unwrap();
  let log=target_path.with_file_name(format!("historical-fresh-{control}-{}.log",uuid::Uuid::new_v4()));{use std::io::Write;use std::os::unix::fs::OpenOptionsExt;let mut f=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(log).unwrap();f.write_all(&child.stdout).unwrap();f.write_all(&child.stderr).unwrap();}
  assert!(child.status.success()&&String::from_utf8_lossy(&child.stdout).contains(&format!("HISTORICAL_FRESH_CONTROL_{control}_REFUSAL_ALL_OPAQUE_SQL_BYTES_NATIVE_RECORDS_AND_SOURCE_FOOTPRINT_UNCHANGED_PASS")),"Fresh control {control} failed: {} {}",String::from_utf8_lossy(&child.stdout),String::from_utf8_lossy(&child.stderr));println!("ACTUAL_INDEPENDENT_HISTORICAL_FRESH_{control}_REFUSAL_PASS");
 }
 if std::env::var("HISTORICAL_READONLY_ADMISSION_TEST").as_deref()==Ok("true"){
  let target=output.with_file_name(format!("historical-independent-readonly-{}",uuid::Uuid::new_v4()));let plan=storage::restoration_plan::plan_historical_preservation(&bundle).await.unwrap();let copy=storage::restoration::reserve_and_copy(&bundle,&target).unwrap();let d=storage::restoration::materialize_groups(copy,&bundle,plan).unwrap();let sealed=storage::restoration::seal_publication(&bundle,d).await.unwrap();let committed=storage::restoration::commit_publication(sealed).await.unwrap();let ready=storage::restoration::retire_publication(committed).unwrap();storage::restoration::linux::historical_readonly_tests::actual_target_admission(ready).await;
  let target=output.with_file_name(format!("historical-readonly-foreign-pool-{}",uuid::Uuid::new_v4()));let plan=storage::restoration_plan::plan_historical_preservation(&bundle).await.unwrap();let copy=storage::restoration::reserve_and_copy(&bundle,&target).unwrap();let d=storage::restoration::materialize_groups(copy,&bundle,plan).unwrap();let sealed=storage::restoration::seal_publication(&bundle,d).await.unwrap();let committed=storage::restoration::commit_publication(sealed).await.unwrap();let ready=storage::restoration::retire_publication(committed).unwrap();storage::restoration::linux::historical_readonly_tests::foreign_original_pool_refusal(ready).await;
 }
 println!("ACTUAL_HISTORICAL_STANDARD_SOURCE_ABSENT_NEW_LEDGER5_SEPARATE_FRESH_ACTIVATION_ORIGINAL81_PENDING_MAX_PHYSICAL_OBSERVATION_PASS; accounting_pg_normal_application_unqualified");

 println!("ACTUAL_HISTORICAL_ORIGINAL_OBJECT_RETAINED_MAIN_FD_REAL_RESERVATION_FOREIGN_PROCESS_BLOCKED_THEN_RELEASED_PASS; disposable_target_intentionally_changed_after_observation");

 println!("ACTUAL_HISTORICAL_ORIGINAL_PUBLISHED_OBJECT_SAME_MAIN_FULLROWS_FULL_PROJECTION_NATIVE_CENSUS_PENDING_MAX_PRESERVED_OBSERVATION_PASS; independent_fresh_reader_accounting_unqualified");

 println!("ACTUAL_HISTORICAL_SOURCE_ABSENT_DISTINCT_LEDGER5_VFS_COMMIT_READBACK_RETIRE_PHYSICAL_ONLY_PASS; finalized_reader_accounting_startup_unqualified");

 println!("ACTUAL_HISTORICAL_SOURCE_ABSENT_PLANNED_DESTINATION_CERTIFIED_PHYSICAL_ROLES_PASS; ledger5_accounting_startup_unqualified");

 println!("ACTUAL_EXPLICIT_SOURCE1_HISTORICAL_SQLITE_PRODUCER_SOURCE_ABSENT_INDEPENDENT_COLD_PHYSICAL_CLASSIFICATION_PASS; destination_and_accounting_unqualified");
}

#[tokio::test(flavor="current_thread")]
#[ignore="owned genuine historical fresh process only"]
async fn actual_fresh_historical_physical_observation(){
 let root=PathBuf::from(std::env::var("HISTORICAL_FINALIZED_FRESH_ROOT").expect("Actual owned fresh target required"));
 storage::restoration::bootstrap_gate(&root).unwrap();sqlx::any::install_default_drivers();
 let pool=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=rw",root.join("starlane-notes.db").display())).await.unwrap();
 storage::restoration::activate_in(&root,&pool).await.unwrap();
 let attempt:String=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations WHERE state='reserved'").fetch_one(&pool).await.unwrap();
 let raw=std::fs::read(root.join(".attachment-upload-admission").join(&attempt).join("claim.json")).unwrap();let body=std::fs::read(root.join(".attachment-upload-journal").join(&attempt).join("payload.part")).unwrap();
 if let Ok(control)=std::env::var("HISTORICAL_FINALIZED_FRESH_CONTROL"){
  use std::os::unix::fs::{OpenOptionsExt,DirBuilderExt};use std::io::Write;
  let ledger:Value=serde_json::from_slice(&std::fs::read(root.join(".restoration-custody/current.json")).unwrap()).unwrap();let roles=&ledger["runtime"]["historical_preservation"];let gid=ledger["runtime"]["generation_id"].as_str().unwrap();let oldid=roles["source"]["prior"]["generation_id"].as_str().unwrap();
  match control.as_str(){
   "old_row"=>{sqlx::query("UPDATE storage_restore_generations SET created_at=created_at||'-foreign' WHERE generation_id=$1").bind(oldid).execute(&pool).await.unwrap();},
   "current_row"=>{sqlx::query("UPDATE storage_restore_generations SET created_at=created_at||'-foreign' WHERE generation_id=$1").bind(gid).execute(&pool).await.unwrap();},
   "projection"=>{sqlx::query("UPDATE documents SET title=title||'-foreign'").execute(&pool).await.unwrap();},
   "foreign_equal_claim"=>{let path=root.join(".attachment-upload-admission").join(&attempt).join("claim.json");let kept=root.with_file_name(format!("historical-original-claim-kept-{}",uuid::Uuid::new_v4()));std::fs::rename(&path,&kept).unwrap();let mut f=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(path).unwrap();f.write_all(&raw).unwrap();f.sync_all().unwrap();},
   "recreated_payload"=>{let mut selected=None;for scope in roles["completed"]["material"]["scopes"].as_array().unwrap(){let old=scope["original"]["reservation"]["operation_id"].as_str().unwrap();let intent:storage::attachment_upload::Intent=serde_json::from_slice(&std::fs::read(root.join(".attachment-upload-journal").join(old).join("intent.json")).unwrap()).unwrap();if intent.binary_sha256==storage::attachment_upload::digest(b"ORIGINAL NATIVE WINNER"){selected=Some(intent);break;}}let intent=selected.expect("Genuine original fixture body required");let path=root.join(intent.storage_path);assert!(!path.exists());let mut f=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(path).unwrap();f.write_all(b"ORIGINAL NATIVE WINNER").unwrap();f.sync_all().unwrap();},
   "new_association"=>{let erased=roles["completed"]["material"]["scopes"][0]["original"]["disposition"]["asset_id"].as_str().unwrap();sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at) SELECT $1,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at FROM attachments ORDER BY id LIMIT 1").bind(erased).execute(&pool).await.unwrap();},
   "usage_missing_row"=>{sqlx::query("DELETE FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&attempt).execute(&pool).await.unwrap();},
   "usage_extra_row"=>{sqlx::query("INSERT INTO attachment_capacity_reservations(operation_id,request_sha256,admission_claim_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,disposition_json,disposition_sha256) SELECT $1,request_sha256,admission_claim_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,disposition_json,disposition_sha256 FROM attachment_capacity_reservations WHERE operation_id=$2").bind(uuid::Uuid::new_v4().to_string()).bind(&attempt).execute(&pool).await.unwrap();},
   "usage_state"=>{sqlx::query("UPDATE attachment_capacity_reservations SET state='committed',committed_asset_id='foreign-accounting-asset',committed_bytes=0 WHERE operation_id=$1").bind(&attempt).execute(&pool).await.unwrap();},
   "usage_maximum"=>{sqlx::query("UPDATE attachment_capacity_reservations SET maximum_bytes=maximum_bytes+1 WHERE operation_id=$1").bind(&attempt).execute(&pool).await.unwrap();},
   "copied_root"|"foreign_connection"=>{},_=>panic!("Unknown bounded fresh control"),
  }
  let before_sql={let mut c=pool.acquire().await.unwrap();storage::restoration::linux::engine_transfer::historical_application_projection_in(&mut c,None).await.unwrap()};pool.close().await;
  let before=historical_test_footprint(&root);
  if control=="copied_root"{let copied=root.with_file_name(format!("historical-copied-unadmitted-{}",uuid::Uuid::new_v4()));fn copy(a:&Path,b:&Path){use std::os::unix::fs::DirBuilderExt;std::fs::DirBuilder::new().mode(0o700).create(b).unwrap();for e in std::fs::read_dir(a).unwrap(){let e=e.unwrap();let p=e.path();let n=b.join(e.file_name());if e.file_type().unwrap().is_dir(){copy(&p,&n)}else{std::fs::copy(&p,&n).unwrap();}}}copy(&root,&copied);assert!(storage::restoration::bootstrap_gate(&copied).is_err());let e=storage::restoration::linux::inspect_finalized_historical_sqlite_usage(&copied,true).await.err().expect("Copied root cannot invent activation");assert_eq!(e.kind(),std::io::ErrorKind::InvalidData);}
  else if control=="foreign_connection"{let other=root.with_file_name(format!("historical-foreign-connected-{}.sqlite",uuid::Uuid::new_v4()));std::fs::copy(root.join("starlane-notes.db"),&other).unwrap();let e=storage::restoration::linux::inspect_finalized_historical_foreign_usage_for_test(&root,&other).await.err().expect("Equal-row different SQLite file cannot provide source custody");assert_eq!(e.kind(),std::io::ErrorKind::InvalidData);assert!(e.to_string().contains("Qualified original connected SQLite target differs"),"Wrong refusal class: {e}");}
  else{let e=storage::restoration::linux::inspect_finalized_historical_sqlite_usage(&root,true).await.err().expect("Controlled altered source cannot gain historical observation");assert_eq!(e.kind(),std::io::ErrorKind::InvalidData,"Unexpected incidental I/O failure: {e}");}
  tokio::task::yield_now().await;assert_eq!(historical_test_footprint(&root),before,"Refusal changed original controlled bytes/inodes/stamps");
  let p=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=ro",root.join("starlane-notes.db").display())).await.unwrap();let after_sql={let mut c=p.acquire().await.unwrap();storage::restoration::linux::engine_transfer::historical_application_projection_in(&mut c,None).await.unwrap()};p.close().await;assert_eq!(after_sql,before_sql);assert_eq!(std::fs::read(root.join(".attachment-upload-admission").join(&attempt).join("claim.json")).unwrap(),raw);assert_eq!(std::fs::read(root.join(".attachment-upload-journal").join(&attempt).join("payload.part")).unwrap(),body);
  println!("HISTORICAL_FRESH_CONTROL_{control}_REFUSAL_ALL_OPAQUE_SQL_BYTES_NATIVE_RECORDS_AND_SOURCE_FOOTPRINT_UNCHANGED_PASS");return;
 }
 let expected_logical:i64=sqlx::query_scalar("SELECT CAST(COALESCE(SUM(byte_length),0) AS BIGINT) FROM attachment_retained_byte_ledger").fetch_one(&pool).await.unwrap();assert!(expected_logical>0);
 let expected_pending:i64=sqlx::query_scalar("SELECT CAST(COALESCE(SUM(maximum_bytes),0) AS BIGINT) FROM attachment_capacity_reservations WHERE state='reserved'").fetch_one(&pool).await.unwrap();assert_eq!(expected_pending,1048576);
 let before_sql={let mut c=pool.acquire().await.unwrap();storage::restoration::linux::engine_transfer::historical_application_projection_in(&mut c,None).await.unwrap()};
 pool.close().await;
 let before_physical=historical_test_footprint(&root);
 let usage=storage::restoration::linux::inspect_finalized_historical_sqlite_usage(&root,true).await.unwrap();
 assert_eq!(usage.immutable_logical_charge_bytes,expected_logical);assert_eq!(usage.pending_maximum_bytes,expected_pending);assert_eq!(usage.conservative_total_bytes,expected_logical.checked_add(expected_pending).unwrap());assert_eq!(usage.reference_semantics,storage::restoration::historical::HistoricalReferenceSemantics::NotEstablished);
 assert_eq!(historical_test_footprint(&root),before_physical);
 let p=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=ro",root.join("starlane-notes.db").display())).await.unwrap();let after_sql={let mut c=p.acquire().await.unwrap();storage::restoration::linux::engine_transfer::historical_application_projection_in(&mut c,None).await.unwrap()};p.close().await;assert_eq!(after_sql,before_sql);
 println!("ACTUAL_FRESH_HISTORICAL_CONSERVATIVE_LOGICAL_{}_PENDING_{}_TOTAL_{}_FULLROWS_NATIVE_UNCHANGED_PASS",usage.immutable_logical_charge_bytes,usage.pending_maximum_bytes,usage.conservative_total_bytes);
 let wire:Value=serde_json::from_slice(&std::fs::read(root.join(".restoration-custody/current.json")).unwrap()).unwrap();let owner=wire["runtime"]["historical_preservation"].clone();let json=serde_json::to_string(&owner).unwrap();
 let duplicate=json.replacen('{',"{\"version\":1,",1);assert!(serde_json::from_str::<storage::restoration::historical::HistoricalOwnerEnvelope>(&duplicate).is_err());
 let mut unknown=owner.clone();unknown["foreign_custody"]=json!(true);assert!(serde_json::from_value::<storage::restoration::historical::HistoricalOwnerEnvelope>(unknown).is_err());
 let committed=owner["native"].as_array().unwrap().iter().position(|s|s["role"]=="committed_preservation").unwrap();let mut null=owner.clone();null["native"][committed]["payloads"]=Value::Null;assert!(serde_json::from_value::<storage::restoration::historical::HistoricalOwnerEnvelope>(null).is_err());
 let root_id={use std::os::unix::fs::MetadataExt;let m=std::fs::metadata(&root).unwrap();storage::restoration::admission::Identity{device:m.dev(),inode:m.ino()}};
 let mut missing=owner.clone();missing["native"][committed]["payloads"]=json!([]);assert!(serde_json::from_value::<storage::restoration::historical::HistoricalOwnerEnvelope>(missing).unwrap().validate(&root_id).is_err());
 let mut wrong=owner.clone();wrong["native"][committed]["role"]=json!("retained_pending");assert!(serde_json::from_value::<storage::restoration::historical::HistoricalOwnerEnvelope>(wrong).unwrap().validate(&root_id).is_err());
 let mut null=wire["runtime"].clone();null["historical_preservation"]=Value::Null;assert!(serde_json::from_value::<storage::restoration::Ledger>(null).is_err());
 println!("ACTUAL_HISTORICAL_TYPED_FIELD_STRICT_NULL_DUPLICATE_UNKNOWN_ROLE_PAYLOAD_CONTROLS_PASS; decoded_wire_not_authority");
 let s=storage::restoration::linux::inspect_finalized_historical_sqlite(&root,true).await.unwrap();assert_eq!((s.completed_scopes,s.new_committed,s.new_pending,s.new_disposed),(3,1,1,1));assert_eq!(s.retained_pending_maximum,1024*1024);assert_eq!(s.reference_semantics,storage::restoration::historical::HistoricalReferenceSemantics::NotEstablished);
 assert_eq!(std::fs::read(root.join(".attachment-upload-admission").join(&attempt).join("claim.json")).unwrap(),raw);assert_eq!(std::fs::read(root.join(".attachment-upload-journal").join(&attempt).join("payload.part")).unwrap(),body);
 let refused=storage::attachment_upload::admission::WriterLease::open_idle(&root,&attempt);assert!(refused.as_ref().is_err_and(|e|format!("{e:#}").contains("Admission native custody replaced")),"Copied old original capsule must refuse native writer based on original custody, not active exclusion");
 let code="import sqlite3,sys; c=sqlite3.connect(sys.argv[1],timeout=0.05); c.execute('BEGIN IMMEDIATE'); c.execute('UPDATE _sqlx_migrations SET execution_time=execution_time+1 WHERE version=1'); c.commit(); print('ACTUAL_HISTORICAL_USAGE_RELEASED_FOREIGN_WRITER_COMMITTED'); sys.exit(42)";
 let released=std::process::Command::new("/usr/bin/python3").args(["-I","-c",code]).arg(root.join("starlane-notes.db")).output().unwrap();assert_eq!(released.status.code(),Some(42),"Usage did not release actual writer exclusion: {}",String::from_utf8_lossy(&released.stderr));
 println!("ACTUAL_HISTORICAL_CONSERVATIVE_USAGE_RELEASED_FOREIGN_WRITER_COMMIT_PASS; disposable_target_deliberately_changed_after_all_observations");
 println!("ACTUAL_INDEPENDENT_FRESH_REGISTERED_LEDGER5_FULL_SOURCE_CUSTODY_ROWS_ORIGINAL81_PROJECTION_NATIVE_PHYSICAL_OBSERVATION_PENDING_MAX_COPIED_WRITER_REFUSAL_PASS; reference_semantics_not_established_conservative_only_no_quota_release");
}

fn historical_test_footprint(root:&Path)->Value{
 use std::os::unix::fs::MetadataExt;fn walk(root:&Path,p:&Path,out:&mut serde_json::Map<String,Value>){for e in std::fs::read_dir(p).unwrap(){let e=e.unwrap();let path=e.path();let m=std::fs::symlink_metadata(&path).unwrap();let rel=path.strip_prefix(root).unwrap().to_str().unwrap().to_owned();let sha=if m.is_file(){storage::attachment_upload::digest(&std::fs::read(&path).unwrap())}else{String::new()};out.insert(rel,json!([m.dev(),m.ino(),m.len(),m.nlink(),m.mode(),m.mtime(),m.mtime_nsec(),m.ctime(),m.ctime_nsec(),sha]));if m.is_dir(){walk(root,&path,out)}}}let mut out=serde_json::Map::new();let m=std::fs::symlink_metadata(root).unwrap();out.insert(String::new(),json!([m.dev(),m.ino(),m.len(),m.nlink(),m.mode(),m.mtime(),m.mtime_nsec(),m.ctime(),m.ctime_nsec()]));walk(root,root,&mut out);Value::Object(out)
}
