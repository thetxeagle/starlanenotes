use std::os::unix::fs::DirBuilderExt;
// include!("snapshot_recovery_restart_tests.rs") inside recovery tests on Linux.
// Process loss only, not physical power-loss certification.
fn recovery_durable(path:&std::path::Path,value:&Value){
    use std::io::Write;
    let mut file=std::fs::File::create(path).unwrap();file.write_all(&serde_json::to_vec(value).unwrap()).unwrap();file.sync_all().unwrap();std::fs::File::open(path.parent().unwrap()).unwrap().sync_all().unwrap();
}
async fn recovery_reopen(root:&std::path::Path,meta:&Value)->AppState{
    crate::storage::restoration::bootstrap_gate(root).unwrap();crate::storage::init_data_dir().await.unwrap();sqlx::any::install_default_drivers();
    let db=sqlx::any::AnyPoolOptions::new().max_connections(3).after_connect(|conn,_|Box::pin(async move{sqlx::query("PRAGMA journal_mode=WAL").execute(&mut *conn).await?;sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *conn).await?;sqlx::query("PRAGMA busy_timeout=5000").execute(conn).await?;Ok(())})).connect(&format!("sqlite://{}?mode=rwc",meta["database"].as_str().unwrap())).await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    AppState{db,jwt_secret:meta["fixture_jwt_secret"].as_str().unwrap().into(),base_url:"http://fixture.invalid".into(),search_index:crate::search::SearchIndex::new(root.join("restart-search")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()}
}
async fn recovery_restart_counts(s:&AppState)->Value{
    let mut out=serde_json::Map::new();for(label,query)in [("receipt","SELECT COUNT(*) FROM snapshot_recovery_receipts"),("audit","SELECT COUNT(*) FROM audit_log WHERE action='document.snapshot_recover'"),("draft","SELECT COUNT(*) FROM document_drafts")]{out.insert(label.into(),json!(sqlx::query_scalar::<_,i64>(query).fetch_one(&s.db).await.unwrap()));}Value::Object(out)
}
#[tokio::test]
async fn snapshot_recovery_writer_child(){
    let Ok(root)=std::env::var("L5_RECOVERY_ROOT")else{return};let root=std::path::PathBuf::from(root);assert_eq!(crate::storage::data_dir(),root);let phase=std::env::var("L5_RECOVERY_PHASE").unwrap();assert!(["before_commit","after_commit"].contains(&phase.as_str()));
    crate::storage::restoration::bootstrap_gate(&root).unwrap();crate::storage::init_data_dir().await.unwrap();let c=setup().await;let req=c.review("editor-1").await;
    let mut conn=c.s.db.acquire().await.unwrap();let page=crate::spaces::document_deletion::rows(&mut conn,"SELECT * FROM documents WHERE id=$1",&["doc-1"]).await.unwrap().remove(0);let bytes=crate::spaces::draft_publication::page_bytes(&page).await.unwrap();drop(conn);
    recovery_durable(&root.join("original.json"),&json!({"request":req,"database":c.path,"fixture_jwt_secret":c.s.jwt_secret,"snapshot":c.snapshot,"slug":c.slug,"page":page,"page_bytes":bytes}));
    let marker=root.join("writer-reached.json");let chosen=phase.clone();let op=req["operation_id"].clone();let hook:Box<dyn FnOnce()+Send>=Box::new(move||{recovery_durable(&marker,&json!({"phase":chosen,"operation_id":op}));std::process::exit(77)});
    let outcome=if phase=="before_commit"{super::BEFORE_COMMIT.scope(std::cell::RefCell::new(Some(hook)),http(&c.s,"editor-1","POST",&c.recover(),Some(req))).await}else{super::AFTER_COMMIT.scope(std::cell::RefCell::new(Some(hook)),http(&c.s,"editor-1","POST",&c.recover(),Some(req))).await};
    panic!("writer did not reach requested {phase} checkpoint: {outcome:?}");
}
#[tokio::test]
async fn snapshot_recovery_restart_child(){
    let Ok(root)=std::env::var("L5_RECOVERY_ROOT")else{return};let root=std::path::PathBuf::from(root);assert_eq!(crate::storage::data_dir(),root);let phase=std::env::var("L5_RECOVERY_PHASE").unwrap();let meta:Value=serde_json::from_slice(&std::fs::read(root.join("original.json")).unwrap()).unwrap();let marker:Value=serde_json::from_slice(&std::fs::read(root.join("writer-reached.json")).unwrap()).unwrap();assert_eq!(marker["phase"],phase);assert_eq!(marker["operation_id"],meta["request"]["operation_id"]);
    let s=recovery_reopen(&root,&meta).await;crate::storage::startup::recover_and_normalize(&s.db).await.unwrap();let committed=phase=="after_commit";
    assert_eq!(recovery_restart_counts(&s).await,if committed{json!({"receipt":1,"audit":1,"draft":2})}else{json!({"receipt":0,"audit":0,"draft":1})});
    let url=format!("/spaces/space-1/documents/doc-1/publication-snapshots/{}/recover",meta["snapshot"].as_str().unwrap());let req=meta["request"].clone();
    let(status,first)=http(&s,"editor-1","POST",&url,Some(req.clone())).await;assert_eq!(status,if committed{StatusCode::OK}else{StatusCode::CREATED},"{first}");assert_eq!(first["operation_id"],req["operation_id"]);assert_eq!(first["disposition"],if committed{"historical"}else{"applied"});assert_eq!(first["draft"]["title"],"Historical recovered title");assert_eq!(first["draft"]["content"],OLD_BODY);assert_eq!(first["draft"]["icon"],"compass");assert_eq!(first["draft"]["cover_image"],"https://invalid.example/historical-cover.png");assert_eq!(first["draft"]["base_updated_at"],meta["page"]["updated_at"]);
    let mut expected=first.clone();expected["disposition"]=json!("historical");for _ in 0..2{crate::storage::startup::recover_and_normalize(&s.db).await.unwrap();let(status,result)=http(&s,"editor-1","POST",&url,Some(req.clone())).await;assert_eq!(status,StatusCode::OK,"{result}");assert_eq!(result,expected);assert_eq!(recovery_restart_counts(&s).await,json!({"receipt":1,"audit":1,"draft":2}));}
    let mut conn=s.db.acquire().await.unwrap();let page=crate::spaces::document_deletion::rows(&mut conn,"SELECT * FROM documents WHERE id=$1",&["doc-1"]).await.unwrap().remove(0);assert_eq!(page,meta["page"]);let bytes=crate::spaces::draft_publication::page_bytes(&page).await.unwrap();assert_eq!(json!(bytes),meta["page_bytes"]);drop(conn);
    recovery_durable(&root.join("restart-result.json"),&json!({"phase":phase,"operation_id":req["operation_id"],"draft_id":first["draft"]["id"],"counts":recovery_restart_counts(&s).await}));s.db.close().await;
}
fn recovery_child(root:&std::path::Path,name:&str,phase:&str)->i32{
    let log=root.join(format!("{name}.log"));let out=std::fs::File::create(&log).unwrap();let mut child=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact",&format!("spaces::publication_snapshot_recovery::tests::{name}"),"--nocapture"]).env("DATA_DIR",root).env("TMPDIR",root.join("scratch")).env("L5_RECOVERY_ROOT",root).env("L5_RECOVERY_PHASE",phase).stdout(std::process::Stdio::from(out.try_clone().unwrap())).stderr(std::process::Stdio::from(out)).spawn().unwrap();let deadline=std::time::Instant::now()+std::time::Duration::from_secs(45);
    loop{if let Some(status)=child.try_wait().unwrap(){return status.code().unwrap_or(-1)}if std::time::Instant::now()>deadline{child.kill().unwrap();let _=child.wait();panic!("recovery child timeout: {}",log.display())}std::thread::sleep(std::time::Duration::from_millis(20));}
}
#[test]
fn actual_recovery_process_loss_before_and_after_commit_retries_original_operation_once(){
    for phase in ["before_commit","after_commit"]{let root=std::env::temp_dir().join(format!("snapshot-recovery-process-{}",uuid::Uuid::new_v4()));std::fs::DirBuilder::new().mode(0o700).create(&root).unwrap();std::fs::create_dir(root.join("scratch")).unwrap();
        assert_eq!(recovery_child(&root,"snapshot_recovery_writer_child",phase),77,"writer failed; {}",std::fs::read_to_string(root.join("snapshot_recovery_writer_child.log")).unwrap());assert_eq!(recovery_child(&root,"snapshot_recovery_restart_child",phase),0,"restart failed; {}",std::fs::read_to_string(root.join("snapshot_recovery_restart_child.log")).unwrap());let result:Value=serde_json::from_slice(&std::fs::read(root.join("restart-result.json")).unwrap()).unwrap();assert_eq!(result["phase"],phase);assert_eq!(result["counts"],json!({"receipt":1,"audit":1,"draft":2}));std::fs::remove_dir_all(root).unwrap();
    }
}
