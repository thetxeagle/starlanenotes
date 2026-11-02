// Genuine replay producer on original49/3 SQLx catalogs, then additive50/4 upgrade.
async fn registry_applied(db:&AnyPool)->Vec<(i64,Vec<u8>)> {
    sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version").fetch_all(db).await.unwrap()
}
fn registry_native_bytes(root:&Path)->Value {
    use sha2::{Digest,Sha256};
    fn walk(root:&Path,path:&Path,out:&mut serde_json::Map<String,Value>) {
        if !path.exists(){return;}
        for e in std::fs::read_dir(path).unwrap(){let p=e.unwrap().path();let m=std::fs::symlink_metadata(&p).unwrap();assert!(!m.file_type().is_symlink());if m.is_dir(){walk(root,&p,out);}else{assert!(m.is_file());out.insert(p.strip_prefix(root).unwrap().to_str().unwrap().into(),json!(hex::encode(Sha256::digest(std::fs::read(p).unwrap()))));}}
    }
    let mut out=serde_json::Map::new();
    for p in ["spaces",".storage-journal",".publication-journal",".canonical-rewrite-journal",".relocation-journal",".attachment-upload-journal"] {walk(root,&root.join(p),&mut out);}
    Value::Object(out)
}
async fn registry_legacy_association(db:&AnyPool)->Value {
    let mut c=db.acquire().await.unwrap();json!(crate::spaces::document_deletion::rows(&mut c,"SELECT * FROM attachments ORDER BY id",&[]).await.unwrap())
}
async fn registry_upgrade(engine:&str) {
    let root=crate::storage::data_dir();assert!(root.is_absolute()&&root.starts_with(work()));std::fs::create_dir_all(&root).unwrap();
    let (url,schema)=if engine=="postgres" {
        let u=std::env::var("STARLANE_UPLOAD_ENGINE_PG_URL").expect("owned disposable PG URL required");sqlx::any::install_default_drivers();let schema=format!("registry_upgrade_{}",uuid::Uuid::new_v4().simple());let mut c=sqlx::AnyConnection::connect(&u).await.unwrap();sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&mut c).await.unwrap();c.close().await.unwrap();(u,schema)
    }else{assert_eq!(engine,"sqlite");(format!("sqlite://{}?mode=rwc",root.join("starlane-notes.db").display()),String::new())};
    let kind=crate::db::DatabaseEngine::from_url(&url).unwrap();let db=real::pool(&url,&schema).await;
    let latest=crate::db::selected_migrator(kind);let prior_max=if kind==crate::db::DatabaseEngine::Sqlite{49}else{3};
    let prior=sqlx::migrate::Migrator{migrations:std::borrow::Cow::Owned(latest.iter().filter(|m|m.version<=prior_max).cloned().collect()),..sqlx::migrate::Migrator::DEFAULT};
    prior.run(&db).await.unwrap();let prior_history=registry_applied(&db).await;assert_eq!(prior_history.len(),prior_max as usize);
    real::seed(&db).await;let s=real::state(db,&root);let owner=real::login(&s,"upload-owner").await;
    let born=real::ok(&s,Some(&owner),"POST","/spaces/upload-space/documents",json!({"source_id":"upload-page","title":"Genuine old-catalog page","content":"Initial","property_values":[]})).await;
    let draft=born["draft_id"].as_str().expect("real initial draft");let path=format!("/spaces/upload-space/documents/upload-page/drafts/{draft}");
    let selected=real::ok(&s,Some(&owner),"GET",&path,Value::Null).await;
    real::ok(&s,Some(&owner),"PUT",&path,json!({"source_digest":selected["source_digest"],"content":"Original old-catalog publication [legacy](vault://registry-upgrade-original)"})).await;
    real::ok(&s,Some(&owner),"POST",&format!("{path}/publish"),json!({})).await;
    let id=uuid::Uuid::new_v4().to_string();let op=uuid::Uuid::new_v4().to_string();
    let mut replay=retirement_manifest(&op,&id);replay["operations"][0]["action"]=json!("create");replay["operations"][0]["payload"]=json!({"id":id,"space_id":"upload-space","document_id":"upload-page","name":"old-original.bin","mime_type":"application/octet-stream","data_url":"data:application/octet-stream;base64,AAECA/8=","vault_path":"vault://registry-upgrade-original"});
    let result=real::ok(&s,Some(&owner),"POST","/sync/replay",replay.clone()).await;assert_eq!(result["applied_operation_count"],1,"actual original-catalog replay: {result}");
    let association=registry_legacy_association(&s.db).await;assert_eq!(association.as_array().unwrap().len(),1);
    let receipt:String=sqlx::query_scalar("SELECT receipt_json FROM asset_replay_receipts WHERE incoming_id=$1").bind(&op).fetch_one(&s.db).await.unwrap();
    let bytes=registry_native_bytes(&root);
    crate::db::migrate_selected(&s.db,kind).await.unwrap();let after=registry_applied(&s.db).await;assert_eq!(&after[..prior_history.len()],prior_history.as_slice());assert_eq!(after.len(),(prior_max+1) as usize);
    for table in ["attachment_families","attachment_version_records","attachment_version_changes","attachment_retirements"] {assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT COUNT(*) FROM {table}")).fetch_one(&s.db).await.unwrap(),0,"old receipt is not registry backfill");}
    assert_eq!(registry_legacy_association(&s.db).await,association);assert_eq!(registry_native_bytes(&root),bytes);
    let path:String=sqlx::query_scalar("SELECT storage_path FROM attachments WHERE id=$1").bind(&id).fetch_one(&s.db).await.unwrap();let body_url=format!("/api/v1/files/{path}");
    assert_eq!(get(&s,&owner,&body_url).await,(StatusCode::OK,vec![0,1,2,3,255]));
    let h=format!("/spaces/upload-space/documents/upload-page/attachment-families/{id}/versions");assert_ne!(get(&s,&owner,&h).await.0,StatusCode::OK,"legacy has no fabricated history");
    assert_eq!(real::ok(&s,Some(&owner),"POST","/sync/replay",replay).await["skipped_operation_count"],1);
    let uri=format!("/spaces/upload-space/documents/upload-page/attachments/{id}");assert_eq!(real::call(&s,Some(&owner),"DELETE",&uri,Value::Null).await.0,StatusCode::NO_CONTENT);
    assert_ne!(get(&s,&owner,&body_url).await.0,StatusCode::OK);
    let retained=real::ok(&s,Some(&owner),"GET","/spaces/upload-space/documents/upload-page/attachment-retirements",Value::Null).await;assert_eq!(retained.as_array().unwrap().len(),1);let mut c=s.db.acquire().await.unwrap();let inspection=crate::storage::attachment_versions::inspect_registry_snapshot(&mut c).await.unwrap();assert!(inspection.versions().is_empty());assert_eq!(inspection.retirements().len(),1);drop(c);assert_eq!(retained[0]["association"][1],"old-original.bin");
    assert_eq!(registry_legacy_association(&s.db).await,association);assert_eq!(registry_native_bytes(&root),bytes);
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT receipt_json FROM asset_replay_receipts WHERE incoming_id=$1").bind(&op).fetch_one(&s.db).await.unwrap(),receipt);
    let expected=json!({"url":url,"schema":schema,"owner":owner,"asset":id,"operation":op,"body_url":body_url,"association":association,"receipt":receipt,"bytes":bytes,"retired":retained});
    let file=work().join("upgrade-expected.json");std::fs::write(&file,expected.to_string()).unwrap();use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&file,std::fs::Permissions::from_mode(0o600)).unwrap();s.db.close().await;drop(s);
    let child=std::process::Command::new(std::env::current_exe().unwrap()).args(["attachment_version_move_fixture::fresh_registry_upgrade_child","--ignored","--exact","--nocapture"]).output().unwrap();assert!(child.status.success(),"fresh upgrade child: {} {}",String::from_utf8_lossy(&child.stdout),String::from_utf8_lossy(&child.stderr));assert!(String::from_utf8_lossy(&child.stdout).contains("REGISTRY_UPGRADE_GENUINE_LEGACY_FRESH_RETIREMENT_RAW_RECEIPT_BYTES_PASS"));println!("{}",String::from_utf8_lossy(&child.stdout));println!("REGISTRY_UPGRADE_GENUINE_ORIGINAL_CATALOG_SQLX_HISTORY_NO_BACKFILL_PASS");
}
#[tokio::test]#[ignore="requires private original49 catalog producer/current50 upgrade and fresh process"]async fn required_registry_upgrade_sqlite(){registry_upgrade("sqlite").await;}
#[tokio::test]#[ignore="requires disposable owned original3 catalog producer/current4 upgrade and fresh process"]async fn required_registry_upgrade_postgres(){registry_upgrade("postgres").await;}
#[tokio::test]#[ignore="only genuine required upgrade parent supplies source evidence"]async fn fresh_registry_upgrade_child(){
    let e:Value=serde_json::from_slice(&std::fs::read(work().join("upgrade-expected.json")).unwrap()).unwrap();let root=crate::storage::data_dir();assert!(root.is_absolute()&&root.starts_with(work()));
    let db=real::pool(e["url"].as_str().unwrap(),e["schema"].as_str().unwrap()).await;crate::storage::restoration::bootstrap_gate(&root).unwrap();crate::storage::restoration::activate_in(&root,&db).await.unwrap();crate::storage::relocation::recover(&db).await.unwrap();crate::storage::journal::recover(&db).await.unwrap();crate::storage::canonical_rewrite::recover(&db).await.unwrap();crate::storage::publication::recover(&db).await.unwrap();crate::storage::attachment_upload::recover_in(&root,&db).await.unwrap();let s=real::state(db,&root);let owner=e["owner"].as_str().unwrap();
    assert_eq!(registry_legacy_association(&s.db).await,e["association"]);assert_eq!(registry_native_bytes(&root),e["bytes"]);assert_eq!(sqlx::query_scalar::<_,String>("SELECT receipt_json FROM asset_replay_receipts WHERE incoming_id=$1").bind(e["operation"].as_str().unwrap()).fetch_one(&s.db).await.unwrap(),e["receipt"]);
    assert_ne!(get(&s,owner,e["body_url"].as_str().unwrap()).await.0,StatusCode::OK);assert_eq!(real::ok(&s,Some(owner),"GET","/spaces/upload-space/documents/upload-page/attachment-retirements",Value::Null).await,e["retired"]);
    assert!(!real::call(&s,None,"GET","/spaces/upload-space/documents/upload-page/attachment-retirements",Value::Null).await.0.is_success());s.db.close().await;println!("REGISTRY_UPGRADE_GENUINE_LEGACY_FRESH_RETIREMENT_RAW_RECEIPT_BYTES_PASS");
}
