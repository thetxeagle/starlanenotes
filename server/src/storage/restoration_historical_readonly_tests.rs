use super::*;
pub(super) const SECRET:&str="owned-original-historical-source-admin-proof-secret-20261015";
pub(super) static OTHER_SESSION:std::sync::OnceLock<String>=std::sync::OnceLock::new();
pub(super) static TOKEN:std::sync::OnceLock<String>=std::sync::OnceLock::new();
pub(super) static ADMIN:std::sync::OnceLock<VerificationHumanCredential>=std::sync::OnceLock::new();
pub(crate) async fn prepare_original_admin(root:&Path,pool:&sqlx::AnyPool){
 let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(pool).await.unwrap();assert_eq!(count,0,"First real registration must precede seeded fixture actors");
 let state=crate::AppState{db:pool.clone(),jwt_secret:SECRET.into(),base_url:"http://localhost".into(),search_index:crate::search::SearchIndex::new(std::env::temp_dir().join(format!("historical-admin-index-{}",uuid::Uuid::new_v4()))).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};
 let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let base=format!("http://{}",listener.local_addr().unwrap());let app=crate::registered_application(state,&base).unwrap();let server=tokio::spawn(async move{axum::serve(listener,app.into_make_service_with_connect_info::<std::net::SocketAddr>()).await.unwrap()});
 let email=format!("historical-{}@test.invalid",uuid::Uuid::new_v4());
 let response=reqwest::Client::new().post(format!("{base}/api/v1/auth/register")).json(&serde_json::json!({"email":email,"password":"OwnedHistoricalPassword42!","display_name":"Original historical administrator"})).send().await.unwrap();let status=response.status();let data:serde_json::Value=response.json().await.unwrap();assert!(status.is_success(),"Original registration refused:{status} error={}",data.get("message").and_then(|v|v.as_str()).unwrap_or("unavailable"));let data=if std::env::var("HISTORICAL_READ_MATRIX_FIXTURE_TEST").as_deref()==Ok("true"){let response=reqwest::Client::new().post(format!("{base}/api/v1/auth/login")).json(&serde_json::json!({"email":email,"password":"OwnedHistoricalPassword42!"})).send().await.unwrap();let status=response.status();let data:serde_json::Value=response.json().await.unwrap();assert!(status.is_success(),"Actual registered producer login refused status={status}");println!("ACTUAL_HISTORICAL_FIXTURE_REGISTERED_PRODUCER_REAL_LOGIN_PASS");data}else{data};if std::env::var("HISTORICAL_KNOWN_USAGE_OBSERVER_TEST").as_deref()==Ok("true") {
 let response=reqwest::Client::new().post(format!("{base}/api/v1/auth/login")).json(&serde_json::json!({"email":email,"password":"OwnedHistoricalPassword42!"})).send().await.unwrap();assert!(response.status().is_success(),"Genuine same-user second login refused");let other:serde_json::Value=response.json().await.unwrap();let second=other["access_token"].as_str().unwrap();let first=crate::auth::jwt::validate_token(data["access_token"].as_str().unwrap(),SECRET,crate::auth::jwt::TokenType::Access).unwrap();let claims=crate::auth::jwt::validate_token(second,SECRET,crate::auth::jwt::TokenType::Access).unwrap();assert_eq!(first.sub,claims.sub);assert_ne!(first.sid,claims.sid);assert!(OTHER_SESSION.set(second.to_owned()).is_ok());println!("ACTUAL_GENUINE_SAME_USER_DISTINCT_HTTP_LOGIN_SESSION_PRE_SOURCE_PASS");
 }
 let token=data["access_token"].as_str().unwrap();assert!(TOKEN.set(token.to_owned()).is_ok());let credential=VerificationHumanCredential::supplied(token,SECRET).unwrap();{let mut c=pool.acquire().await.unwrap();verification_source_human(&mut c,credential.admin()).await.unwrap();}assert!(ADMIN.set(credential).is_ok());server.abort();let _=server.await;assert!(root.exists());println!("ACTUAL_ORIGINAL_HISTORICAL_SOURCE_FIRST_HTTP_ADMIN_REGISTRATION_CURRENT_SESSION_PASS");
}
pub(super) fn writer(path:&Path)->std::process::Output{let code=r#"import sqlite3,sys
c=sqlite3.connect(sys.argv[1],timeout=0.05)
try:
 c.execute('BEGIN IMMEDIATE')
 c.execute('UPDATE _sqlx_migrations SET execution_time=execution_time+1 WHERE version=1')
 c.commit()
 sys.exit(42)
except sqlite3.OperationalError as e:
 if 'locked' not in str(e).lower():raise
 c.rollback()
 sys.exit(10)
"#;std::process::Command::new("/usr/bin/python3").args(["-I","-c",code]).arg(path.join("starlane-notes.db")).output().unwrap()}
pub(crate) async fn capture_original_observed(root:&Path,out:&Path)->Arc<crate::storage::backup_inventory::ValidatedBundle>{
 if std::env::var("HISTORICAL_NORMAL_OPERATOR_TEST").as_deref()==Ok("true"){historical_normal_operator_tests::actual_normal_operator(root).await.unwrap();}
 let credential=ADMIN.get().unwrap();let bundle=capture_sqlite_historical_for_observation(root,out,"1.5.3",true,credential.admin(),|mut held,bundle|Box::pin(async move{held.fence().await?;let original=held.current_human().await?;assert!(!original.is_empty());let before=engine_transfer::historical_application_projection_in(held.original.conn,None).await?;assert_eq!(writer(held.path()).status.code(),Some(10),"Original SOURCE1 writer exclusion failed during historical continuation");held.fence().await?;assert_eq!(engine_transfer::historical_application_projection_in(held.original.conn,None).await?,before);if std::env::var("HISTORICAL_READONLY_CHILD_TEST").as_deref()==Ok("true"){historical_readonly_child_tests::actual_registered_child(&mut held,bundle.clone()).await?;}if std::env::var("HISTORICAL_NORMAL_OPERATOR_TEST").as_deref()==Ok("true"){historical_normal_operator_tests::actual_valid_role_controls(&mut held,bundle.clone()).await?;}println!("ACTUAL_HISTORICAL_OBSERVED_SOURCE_ORIGINAL_HTTP_ADMIN_SOURCE1_FULL512_CALLBACK_HOLD_PASS");Ok(bundle)})).await.unwrap();
 if std::env::var("HISTORICAL_NORMAL_OPERATOR_TEST").as_deref()==Ok("true"){historical_normal_operator_tests::actual_current_logout_refusal(root,&bundle).await.unwrap();}
 bundle
}
pub(crate) async fn actual_target_admission(ready:BootReadyDestination){
 let path=ready.path().to_owned();let old=std::env::var_os("DATA_DIR");std::env::set_var("DATA_DIR",&path);std::env::set_var("DATABASE_URL",format!("sqlite://{}?mode=rw",path.join("starlane-notes.db").display()));
 let(pool,engine)=crate::db::init_pool().await.unwrap();assert_eq!(engine,crate::db::DatabaseEngine::Sqlite);activate_in(&path,&pool).await.unwrap();
 let before={let mut c=pool.acquire().await.unwrap();engine_transfer::historical_application_projection_in(&mut c,None).await.unwrap()};
 let target=HistoricalReadonlySelectedTarget::enter(ready,&pool).await.unwrap();assert!(pool.is_closed());let physical=target.observe().await.unwrap();
 let late_pool=target.reader.pool.clone();let state=target.reader.vfs.as_ref().unwrap().state().clone();
 {let mut c=target.reader.pool.acquire().await.unwrap();let mut tx={use sqlx::Connection;c.begin_with("BEGIN IMMEDIATE").await.unwrap()};physical.recheck_in(&mut tx).await.unwrap();let usage=HistoricalUsageObservation::hold_in(&physical,&mut tx).await.unwrap();assert_eq!(usage.facts.conservative_total_bytes,1048660);let denied=sqlx::query("UPDATE documents SET title=title||'-forbidden'").execute(&mut *tx).await.err().expect("Readonly VFS admitted a write");assert_eq!(denied.as_database_error().and_then(|e|e.code()).as_deref(),Some("8"),"Actual write must refuse SQLITE_READONLY, not incidental locking: {denied}");usage.recheck_in(&mut tx).await.unwrap();assert_eq!(writer(&path).status.code(),Some(10),"Readonly selected target lost independent511 exclusion");assert_eq!(engine_transfer::historical_application_projection_in(&mut tx,None).await.unwrap(),before);tx.rollback().await.unwrap();}
 let raw_vfs=&*target.reader.vfs.as_ref().unwrap().vfs as *const libsqlite3_sys::sqlite3_vfs as usize;let native_size=target.reader.vfs.as_ref().unwrap().vfs.szOsFile;let main_name=CString::new(state.main_name.clone()).unwrap();
 let outstanding=target.reader.pool.acquire().await.unwrap();let closing=target.retire(physical);tokio::pin!(closing);
 let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
 while !late_pool.is_closed(){tokio::select!{_=&mut closing=>panic!("Actual readonly reader retired before outstanding native reader drained"),_=tokio::time::sleep(std::time::Duration::from_millis(10))=>assert!(std::time::Instant::now()<deadline,"Actual retirement did not reach native pool closure")}}
 assert!(!state.bindings.lock().unwrap().is_empty(),"Outstanding actual native reader disappeared before release");assert_eq!(writer(&path).status.code(),Some(10));
 drop(outstanding);let retired=closing.await.unwrap();retired.fence().unwrap();assert!(late_pool.is_closed());
 let mut file=vec![0u64;(native_size as usize+7)/8];let mut flags=0;let late=unsafe{historical_readonly_reader_open(raw_vfs as *mut _,main_name.as_ptr(),file.as_mut_ptr().cast(),libsqlite3_sys::SQLITE_OPEN_READONLY|libsqlite3_sys::SQLITE_OPEN_MAIN_DB,&mut flags)};assert_eq!(late,libsqlite3_sys::SQLITE_CANTOPEN,"Actual retired VFS accepted late raw admission");assert_eq!(writer(&path).status.code(),Some(10),"Retained retired physical owner lost its actual exclusion before output");retired.fence().unwrap();drop(retired);drop(state);drop(late_pool);drop(pool);
 assert_eq!(writer(&path).status.code(),Some(42),"Retired last owner failed to release independent target exclusion");if let Some(v)=old{std::env::set_var("DATA_DIR",v)}else{std::env::remove_var("DATA_DIR")}std::env::remove_var("DATABASE_URL");
 println!("ACTUAL_HISTORICAL_INDEPENDENT_READONLY_VFS_SQL_BEGIN_IMMEDIATE_WRITE_DENIAL_CURRENT81_USAGE_FULL512_SOURCE_SEPARATE511_TARGET_CALLBACK_DRAIN_RETIRED_HOLD_THEN_RELEASE_PASS; target_deliberately_changed_after_final_observation_no_application_record");
}

fn footprint(path:&Path)->std::collections::BTreeMap<String,(PhysicalStamp,String)>{
 fn walk(root:&Path,path:&Path,out:&mut std::collections::BTreeMap<String,(PhysicalStamp,String)>){let f=File::open(path).unwrap();let stamp=physical_stamp(&f).unwrap();let relative=path.strip_prefix(root).unwrap().to_string_lossy().to_string();let hash=if f.metadata().unwrap().is_file(){planned_hash(&f).unwrap().sha256}else{String::new()};out.insert(relative,(stamp,hash));if f.metadata().unwrap().is_dir(){for e in std::fs::read_dir(path).unwrap(){walk(root,&e.unwrap().path(),out);}}}
 let mut out=std::collections::BTreeMap::new();walk(path,path,&mut out);out
}
pub(crate) async fn foreign_original_pool_refusal(ready:BootReadyDestination){
 let path=ready.path().to_owned();let old=std::env::var_os("DATA_DIR");std::env::set_var("DATA_DIR",&path);std::env::set_var("DATABASE_URL",format!("sqlite://{}?mode=rw",path.join("starlane-notes.db").display()));
 let(pool,_)=crate::db::init_pool().await.unwrap();activate_in(&path,&pool).await.unwrap();let before=footprint(&path);let before_sql={let mut c=pool.acquire().await.unwrap();engine_transfer::historical_application_projection_in(&mut c,None).await.unwrap()};
 let foreign=sqlx::AnyPool::connect("sqlite::memory:").await.unwrap();let result=HistoricalReadonlyAdmission::select_original(ready,&foreign).await;let error=result.err().expect("Caller foreign pool cannot invent initialized original target admission");assert!(format!("{error:#}").contains("Actual initialized target pool provenance absent"));
 let after_sql={let mut c=pool.acquire().await.unwrap();engine_transfer::historical_application_projection_in(&mut c,None).await.unwrap()};assert_eq!(before_sql,after_sql);pool.close().await;foreign.close().await;assert_eq!(footprint(&path),before);if let Some(v)=old{std::env::set_var("DATA_DIR",v)}else{std::env::remove_var("DATA_DIR")}std::env::remove_var("DATABASE_URL");
 println!("ACTUAL_HISTORICAL_READONLY_ORIGINAL_PUBLISHER_TARGET_FOREIGN_INITIALIZED_POOL_REFUSAL_FULLSQL_NATIVE_UNCHANGED_PASS");
}

pub(crate) mod fixture_enrichment { include!("restoration_historical_fixture_enrichment_tests.rs"); }
