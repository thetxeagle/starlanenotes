// Genuine current API retirement acceptance, colocated with the original upload/move producer.
async fn retirement_rows(db:&AnyPool)->Value {
    let mut c=db.acquire().await.unwrap();
    json!(crate::spaces::document_deletion::rows(&mut c,"SELECT * FROM attachment_retirements ORDER BY attachment_id",&[]).await.unwrap())
}
async fn retirement_original_before_move(s:&crate::AppState,owner:&str,viewer:&str,original:&Value,engine:crate::db::DatabaseEngine) {
    let raw=immutable_sql(&s.db).await;let files=evidence(&crate::storage::data_dir());
    let uri=format!("/spaces/upload-space/documents/upload-page/attachments/{}",original["id"].as_str().unwrap());
    assert!(!real::call(s,Some(viewer),"DELETE",&uri,Value::Null).await.0.is_success());
    assert_eq!(retirement_rows(&s.db).await,json!([]));
    assert_eq!(immutable_sql(&s.db).await,raw);assert_eq!(evidence(&crate::storage::data_dir()),files);
    retirement_insert_failure(s,true,engine).await;
    assert_eq!(real::call(s,Some(owner),"DELETE",&uri,Value::Null).await.0,StatusCode::CONFLICT);
    assert_eq!(retirement_rows(&s.db).await,json!([]));
    assert_eq!(immutable_sql(&s.db).await,raw);assert_eq!(evidence(&crate::storage::data_dir()),files);
    retirement_insert_failure(s,false,engine).await;
    let (a,b)=tokio::join!(real::call(s,Some(owner),"DELETE",&uri,Value::Null),real::call(s,Some(owner),"DELETE",&uri,Value::Null));
    assert!((a.0==StatusCode::NO_CONTENT&&b.0==StatusCode::NOT_FOUND)||(b.0==StatusCode::NO_CONTENT&&a.0==StatusCode::NOT_FOUND),"real concurrent direct responses: {a:?} {b:?}");
    assert_eq!(immutable_sql(&s.db).await,raw);assert_eq!(evidence(&crate::storage::data_dir()),files);
    assert_ne!(get(s,owner,original["url"].as_str().unwrap()).await.0,StatusCode::OK);
    let listed=real::ok(s,Some(owner),"GET","/spaces/upload-space/documents/upload-page/attachments",Value::Null).await;
    assert!(!listed.as_array().unwrap().iter().any(|r|r["id"]==original["id"]));
    let retained=real::ok(s,Some(viewer),"GET","/spaces/upload-space/documents/upload-page/attachment-retirements",Value::Null).await;
    assert_eq!(retained.as_array().unwrap().len(),1);assert_eq!(retained[0]["attachment_id"],original["id"]);
    assert_eq!(retained[0]["association"][1],original["original_name"]);
    println!("RETIREMENT_REAL_DIRECT_PRESERVES_ASSOCIATION_AND_RECEIPTS_BEFORE_MOVE_PASS");
}
fn retirement_manifest(id:&str,asset:&str)->Value {
    json!({"schema_version":1,"provider":"self-hosted","operation_count":1,"recovery_checkpoint_count":0,"recovery_checkpoints":[],"replay_contract":{"schema_version":1,"operation_order":"queued_at_ascending","idempotency_key":"operation.id","transport":"server-api","server_backends":["sqlite","postgresql"],"required_payload_status":"available","conflict_policy":"server_validates_version_and_permissions","web_canonical_store":"server-api"},"operations":[{"id":id,"entity_type":"asset","entity_id":asset,"action":"delete","queued_at":"2026-10-10T00:00:00Z","payload_status":"available","payload":{"id":asset,"space_id":"upload-destination","document_id":"upload-page"}}]})
}
async fn retirement_after_move(s:&crate::AppState,expected:&Value) {
    let engine=crate::db::DatabaseEngine::from_url(expected["url"].as_str().unwrap()).unwrap();
    let owner=expected["owner"].as_str().unwrap();let viewer=expected["viewer"].as_str().unwrap();
    let original=expected["original"]["id"].as_str().unwrap();let asset=expected["replacement"]["version"]["version_id"].as_str().unwrap();
    let uri=format!("/spaces/upload-destination/documents/upload-page/attachments/{original}");
    assert_eq!(real::call(s,Some(owner),"DELETE",&uri,Value::Null).await.0,StatusCode::NOT_FOUND);
    assert_eq!(retirement_rows(&s.db).await,expected["original_retirement"]);
    let metadata=real::ok(s,Some(owner),"GET","/spaces/upload-destination/documents/upload-page/attachment-retirements",Value::Null).await;
    assert_eq!(metadata[0]["space_id"],"upload-space");
    assert!(!real::call(s,Some(viewer),"GET","/spaces/upload-destination/documents/upload-page/attachment-retirements",Value::Null).await.0.is_success());
    let operation=uuid::Uuid::new_v4().to_string();let manifest=retirement_manifest(&operation,asset);
    retirement_insert_failure(s,true,engine).await;
    let protected=retirement_rows(&s.db).await;
    let failed=real::ok(s,Some(owner),"POST","/sync/replay",manifest.clone()).await;
    assert_eq!(failed["failed_operation_count"],1,"actual constituent failure: {failed}");
    assert_eq!(retirement_rows(&s.db).await,protected);
    assert_eq!(immutable_sql(&s.db).await,expected["immutable_sql"]);assert_eq!(evidence(&crate::storage::data_dir()),expected["evidence"]);
    let receipt_count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM asset_delete_replay_receipts WHERE provider='self-hosted' AND incoming_id=$1").bind(&operation).fetch_one(&s.db).await.unwrap();assert_eq!(receipt_count,0);
    assert_eq!(get(s,owner,expected["replacement_url"].as_str().unwrap()).await,(StatusCode::OK,REPLACED.to_vec()));
    retirement_insert_failure(s,false,engine).await;
    let retained_failure=real::ok(s,Some(owner),"POST","/sync/replay",manifest.clone()).await;
    assert_eq!(retained_failure["failed_operation_count"],1,"failed payload retains explicit original operation; no silent retry");
    assert_eq!(retirement_rows(&s.db).await,protected);
    let manifest=retirement_manifest(&uuid::Uuid::new_v4().to_string(),asset);
    let version_body=format!("/spaces/upload-destination/documents/upload-page/attachment-families/{original}/versions/{asset}/file");
    let state=s.clone();let token=owner.to_owned();let original_operation=manifest.clone();
    let denied=crate::storage::attachment_versions::HISTORY_DOWNLOAD_BEFORE_FINAL.scope(std::cell::RefCell::new(Some(Box::pin(async move {
        let first=real::ok(&state,Some(&token),"POST","/sync/replay",original_operation).await;
        assert_eq!(first["applied_operation_count"],1,"actual concurrent retirement replay: {first}");
    }))),get(s,owner,&version_body)).await;
    assert_ne!(denied.0,StatusCode::OK,"history reader cannot emit stale retirement metadata after waiting");
    assert_eq!(get(s,owner,&version_body).await,(StatusCode::OK,REPLACED.to_vec()));
    let original_body=format!("/spaces/upload-destination/documents/upload-page/attachment-families/{original}/versions/{original}/file");
    assert_eq!(get(s,owner,&original_body).await,(StatusCode::OK,ORIGINAL.to_vec()));
    let pin=format!("/spaces/upload-destination/documents/upload-page/attachment-versions/{original}");
    let exact=real::ok(s,Some(owner),"GET",&pin,Value::Null).await;assert_eq!(exact["logical_attachment_id"],original);assert_eq!(exact["version"]["version_id"],original);assert_eq!(exact["version"]["original_space_id"],"upload-space");assert_eq!(exact["current_space_id"],"upload-destination");assert_eq!(exact["retirement"]["state"],"retained");
    assert_ne!(get(s,viewer,&pin).await.0,StatusCode::OK);assert!(!real::call(s,None,"GET",&pin,Value::Null).await.0.is_success());assert_ne!(get(s,owner,&format!("/spaces/upload-space/documents/upload-page/attachment-versions/{original}")).await.0,StatusCode::OK);

    assert_ne!(get(s,viewer,&original_body).await.0,StatusCode::OK,"revoked old-space audience cannot read retained moved bytes");
    assert!(!real::call(s,None,"GET",&original_body,Value::Null).await.0.is_success());
    assert_ne!(get(s,owner,&format!("/spaces/upload-space/documents/upload-page/attachment-families/{original}/versions/{original}/file")).await.0,StatusCode::OK);
    println!("RETAINED_HISTORY_BODY_CURRENT_MOVED_AUTHORITY_EXACT_BYTES_CONCURRENT_RETIREMENT_FENCE_PASS");
    assert_eq!(immutable_sql(&s.db).await,expected["immutable_sql"]);assert_eq!(evidence(&crate::storage::data_dir()),expected["evidence"]);
    let retired=retirement_rows(&s.db).await;let second=real::ok(s,Some(owner),"POST","/sync/replay",manifest.clone()).await;
    assert_eq!(second["skipped_operation_count"],1,"actual duplicate retirement: {second}");
    assert_eq!(retirement_rows(&s.db).await,retired);
    for url in [expected["original"]["url"].as_str().unwrap(),expected["replacement_url"].as_str().unwrap()] {assert_ne!(get(s,owner,url).await.0,StatusCode::OK);}
    let h=format!("/spaces/upload-destination/documents/upload-page/attachment-families/{original}/versions");
    let history=real::ok(s,Some(owner),"GET",&h,Value::Null).await;
    assert_eq!(history["total"],2);assert_eq!(history["retained_versions"].as_object().unwrap().len(),2);
    assert_eq!(history["versions"][1],expected["replacement"]["version"]);
    let mut changed=manifest.clone();changed["operations"][0]["payload"]["document_id"]=json!("different-page");
    let before=retirement_rows(&s.db).await;
    let reply=real::call(s,Some(owner),"POST","/sync/replay",changed).await;
    assert!(reply.1["applied_operation_count"]!=1,"changed incoming must not retire anything: {reply:?}");
    assert_eq!(retirement_rows(&s.db).await,before);
    assert_eq!(immutable_sql(&s.db).await,expected["immutable_sql"]);assert_eq!(evidence(&crate::storage::data_dir()),expected["evidence"]);
    let mut conn=s.db.acquire().await.unwrap();
    let snapshot=crate::storage::attachment_versions::inspect_registry_snapshot(&mut conn).await.unwrap();
    assert_eq!(snapshot.families().len(),1);assert_eq!(snapshot.versions().len(),2);assert_eq!(snapshot.changes().len(),1);assert_eq!(snapshot.retirements().len(),2);
    for v in snapshot.versions(){assert_eq!(snapshot.current_associations()[&v.version_id].space_id,"upload-destination");assert_eq!(v.original_space_id,"upload-space");}
    let op=snapshot.changes()[0].0.clone();let raw:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_version_changes WHERE operation_id=$1").bind(&op).fetch_one(&mut *conn).await.unwrap();
    sqlx::query("UPDATE attachment_version_changes SET receipt_json='{}' WHERE operation_id=$1").bind(&op).execute(&mut *conn).await.unwrap();
    assert!(crate::storage::attachment_versions::inspect_registry_snapshot(&mut conn).await.is_err(),"malformed original change cannot become restoration metadata authority");
    sqlx::query("UPDATE attachment_version_changes SET receipt_json=$1 WHERE operation_id=$2").bind(&raw).bind(&op).execute(&mut *conn).await.unwrap();
    let after=crate::storage::attachment_versions::inspect_registry_snapshot(&mut conn).await.unwrap();assert_eq!(after.raw_rows(),snapshot.raw_rows());
    println!("REGISTRY_READONLY_ORIGINAL_RECEIPT_CURRENT_MOVED_ASSOCIATION_CENSUS_PASS");
    println!("RETIREMENT_REAL_CURRENT_AUTHORITY_MOVE_DIRECT_DUPLICATE_REPLAY_HISTORY_RAW_BYTES_PASS");
}

async fn retirement_insert_failure(s:&crate::AppState,enabled:bool,engine:crate::db::DatabaseEngine) {
    if engine==crate::db::DatabaseEngine::Sqlite {
        sqlx::raw_sql(if enabled {"CREATE TRIGGER owned_retirement_failure BEFORE INSERT ON attachment_retirements BEGIN SELECT RAISE(ABORT,'owned constituent retirement failure'); END"} else {"DROP TRIGGER owned_retirement_failure"}).execute(&s.db).await.unwrap();
    } else if enabled {
        sqlx::raw_sql("CREATE FUNCTION owned_retirement_failure_fn() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'owned constituent retirement failure'; END $$; CREATE TRIGGER owned_retirement_failure BEFORE INSERT ON attachment_retirements FOR EACH ROW EXECUTE FUNCTION owned_retirement_failure_fn();").execute(&s.db).await.unwrap();
    } else {
        sqlx::raw_sql("DROP TRIGGER owned_retirement_failure ON attachment_retirements; DROP FUNCTION owned_retirement_failure_fn();").execute(&s.db).await.unwrap();
    }
}

async fn post_move_replacement_attempt(s:&crate::AppState,owner:&str,logical:&str,current:&str,op:&str)->(StatusCode,Value){
    let mut bytes=b"--owned\r\nContent-Disposition: form-data; name=\"file\"; filename=\"post-move.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec();bytes.extend_from_slice(b"GENUINE NEW DESTINATION VERSION\0");bytes.extend_from_slice(b"\r\n--owned--\r\n");
    let request=Request::builder().method("POST").uri(format!("/spaces/upload-destination/documents/upload-page/attachment-families/{logical}/versions")).header("authorization",format!("Bearer {owner}")).header("content-type","multipart/form-data; boundary=owned").header("x-starlane-upload-operation",op).header("x-starlane-attachment-version",current).header("x-starlane-attachment-revision","2").body(Body::from(bytes)).unwrap();
    let response=app(s).oneshot(request).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),100000).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap())
}

async fn second_move_retry(s:&crate::AppState,owner:&str,logical:&str,old:&str,op:&str,payload:&[u8])->(StatusCode,Value){
    let mut bytes=b"--owned\r\nContent-Disposition: form-data; name=\"file\"; filename=\"post-move.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec();bytes.extend_from_slice(payload);bytes.extend_from_slice(b"\r\n--owned--\r\n");
    let r=Request::builder().method("POST").uri(format!("/spaces/upload-second-destination/documents/upload-page/attachment-families/{logical}/versions")).header("authorization",format!("Bearer {owner}")).header("content-type","multipart/form-data; boundary=owned").header("x-starlane-upload-operation",op).header("x-starlane-attachment-version",old).header("x-starlane-attachment-revision","2").body(Body::from(bytes)).unwrap();let response=app(s).oneshot(r).await.unwrap();let status=response.status();let bytes=to_bytes(response.into_body(),100000).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap())
}

async fn post_move_replacement_journey(state:crate::AppState,expected:&Value){
    let s=&state;
    let owner=expected["owner"].as_str().unwrap();let viewer=expected["viewer"].as_str().unwrap();let logical=expected["original"]["id"].as_str().unwrap();let old=expected["replacement"]["version"]["version_id"].as_str().unwrap();let op=uuid::Uuid::new_v4().to_string();
    let prior=immutable_sql(&s.db).await;let files=evidence(&crate::storage::data_dir());
    assert!(!post_move_replacement_attempt(s,viewer,logical,old,&uuid::Uuid::new_v4().to_string()).await.0.is_success());assert_eq!(immutable_sql(&s.db).await,prior);assert_eq!(evidence(&crate::storage::data_dir()),files);
    let first=post_move_replacement_attempt(s,owner,logical,old,&op).await;assert_eq!(first.0,StatusCode::CREATED,"genuine post-move replacement: {first:?}");assert_eq!(first.1["expected"]["space_id"],"upload-space");assert_eq!(first.1["version"]["original_space_id"],"upload-destination");assert_eq!(first.1["version"]["version_number"],3);
    let immutable=immutable_sql(&s.db).await;for table in ["attachment_upload_receipts","attachment_version_records","attachment_version_changes"] {for row in prior[table].as_array().unwrap(){assert!(immutable[table].as_array().unwrap().contains(row),"old immutable origin/proof row cannot be rewritten");}}
    let duplicate=post_move_replacement_attempt(s,owner,logical,old,&op).await;assert_eq!(duplicate,first);assert_eq!(immutable_sql(&s.db).await,immutable);
    let listed=real::ok(s,Some(owner),"GET","/spaces/upload-destination/documents/upload-page/attachments",Value::Null).await;for a in listed.as_array().unwrap(){assert_eq!(a["logical_attachment_id"],logical,"family comes from validated registry, not body URL");}
    let current=first.1["version"]["version_id"].as_str().unwrap();let body=format!("/spaces/upload-destination/documents/upload-page/attachment-families/{logical}/versions/{current}/file");assert_eq!(get(s,owner,&body).await,(StatusCode::OK,b"GENUINE NEW DESTINATION VERSION\0".to_vec()));assert_ne!(get(s,viewer,&body).await.0,StatusCode::OK);
    let history=real::ok(s,Some(owner),"GET",&format!("/spaces/upload-destination/documents/upload-page/attachment-families/{logical}/versions"),Value::Null).await;assert_eq!(history["total"],3);assert_eq!(history["versions"][0]["original_space_id"],"upload-space");assert_eq!(history["versions"][2]["original_space_id"],"upload-destination");
    let mut c=s.db.acquire().await.unwrap();let inspection=crate::storage::attachment_versions::inspect_registry_snapshot(&mut c).await.unwrap();assert_eq!(inspection.versions().len(),3);assert_eq!(inspection.changes().len(),2);for v in inspection.versions(){assert_eq!(inspection.current_associations()[&v.version_id].space_id,"upload-destination");}drop(c);
    let queued=retirement_manifest(&uuid::Uuid::new_v4().to_string(),current);let applied=real::ok(s,Some(owner),"POST","/sync/replay",queued.clone()).await;assert_eq!(applied["applied_operation_count"],1);let before=retirement_rows(&s.db).await;let duplicate=real::ok(s,Some(owner),"POST","/sync/replay",queued).await;assert_eq!(duplicate["skipped_operation_count"],1);assert_eq!(retirement_rows(&s.db).await,before);assert_eq!(get(s,owner,&body).await,(StatusCode::OK,b"GENUINE NEW DESTINATION VERSION\0".to_vec()));
    let before_second=immutable_sql(&s.db).await;
    let preview=real::ok(s,Some(owner),"POST","/spaces/upload-destination/moves/preview",json!({"root":{"kind":"document","id":"upload-page"},"destination":{"space_id":"upload-second-destination","folder_id":null,"parent_id":null}})).await;
    assert_eq!(preview["blockers"],json!([]));assert_eq!(preview["commit_available"],true);
    real::ok(s,Some(owner),"POST",&format!("/moves/previews/{}/commit",preview["preview_id"].as_str().unwrap()),json!({"fingerprint":preview["fingerprint"],"decisions":[],"public_links":"disable","confirm_audience_change":true,"confirm_draft_space_change":true})).await;
    assert_eq!(immutable_sql(&s.db).await,before_second,"second move preserves immutable origin rows");
    for id in [logical,old,current] {
        let pin=format!("/spaces/upload-second-destination/documents/upload-page/attachment-versions/{id}");
        let metadata=real::ok(s,Some(owner),"GET",&pin,Value::Null).await;assert_eq!(metadata["logical_attachment_id"],logical);assert_eq!(metadata["version"]["version_id"],id);assert_eq!(metadata["current_space_id"],"upload-second-destination");assert_ne!(get(s,viewer,&pin).await.0,StatusCode::OK);
        let historical=format!("/spaces/upload-second-destination/documents/upload-page/attachment-families/{logical}/versions/{id}/file");let expected_bytes=if id==logical {ORIGINAL.to_vec()} else if id==old {REPLACED.to_vec()} else {b"GENUINE NEW DESTINATION VERSION\0".to_vec()};assert_eq!(get(s,owner,&historical).await,(StatusCode::OK,expected_bytes));
    }
    let mut bytes=b"--owned\r\nContent-Disposition: form-data; name=\"file\"; filename=\"post-move.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec();bytes.extend_from_slice(b"GENUINE NEW DESTINATION VERSION\0");bytes.extend_from_slice(b"\r\n--owned--\r\n");
    let retry_request=Request::builder().method("POST").uri(format!("/spaces/upload-second-destination/documents/upload-page/attachment-families/{logical}/versions")).header("authorization",format!("Bearer {owner}")).header("content-type","multipart/form-data; boundary=owned").header("x-starlane-upload-operation",&op).header("x-starlane-attachment-version",old).header("x-starlane-attachment-revision","2").body(Body::from(bytes)).unwrap();
    let snapshot=immutable_sql(&s.db).await;let native=evidence(&crate::storage::data_dir());let response=app(s).oneshot(retry_request).await.unwrap();let status=response.status();let payload=to_bytes(response.into_body(),100000).await.unwrap();println!("SECOND_MOVE_ORIGINAL_OPERATION_RETRY_CURRENT_ASSOCIATION status={status} body={}",String::from_utf8_lossy(&payload));assert_eq!(status,StatusCode::CREATED,"completed retry uses validated current relocation, never changes original receipt");assert_eq!(serde_json::from_slice::<Value>(&payload).unwrap(),first.1);assert_eq!(immutable_sql(&s.db).await,snapshot);assert_eq!(evidence(&crate::storage::data_dir()),native);
    assert_eq!(second_move_retry(s,owner,logical,old,&op,b"different request bytes").await.0,StatusCode::CONFLICT);
    assert_eq!(immutable_sql(&s.db).await,snapshot);assert_eq!(evidence(&crate::storage::data_dir()),native);
    let (one,two)=tokio::join!(second_move_retry(s,owner,logical,old,&op,b"GENUINE NEW DESTINATION VERSION\0"),second_move_retry(s,owner,logical,old,&op,b"GENUINE NEW DESTINATION VERSION\0"));assert_eq!(one,(StatusCode::CREATED,first.1.clone()));assert_eq!(two,one);
    assert_eq!(immutable_sql(&s.db).await,snapshot);assert_eq!(evidence(&crate::storage::data_dir()),native);
    let mut c=s.db.acquire().await.unwrap();let registry=crate::storage::attachment_versions::inspect_registry_snapshot(&mut c).await.unwrap();assert_eq!(registry.versions().len(),3);for v in registry.versions(){assert_eq!(registry.current_associations()[&v.version_id].space_id,"upload-second-destination");}drop(c);
    let body=format!("/spaces/upload-second-destination/documents/upload-page/attachment-families/{logical}/versions/{current}/file");
    println!("SECOND_MOVE_ALL_IMMUTABLE_PINS_CURRENT_AUTHORITY_EXACT_BYTES_COMPLETED_RETRY201_CONCURRENT_IDENTICAL_TAMPER409_NO_WRITE_PASS");
    let expected=json!({"source":expected,"immutable":immutable_sql(&s.db).await,"retirements":retirement_rows(&s.db).await,"version":first.1["version"],"body":body,"current_target":"upload-second-destination"});
    let file=work().join("postmove-expected.json");std::fs::write(&file,expected.to_string()).unwrap();use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&file,std::fs::Permissions::from_mode(0o600)).unwrap();
    s.db.close().await;drop(state);
    let child=std::process::Command::new(std::env::current_exe().unwrap()).args(["attachment_version_move_fixture::fresh_postmove_version_child","--ignored","--exact","--nocapture"]).output().unwrap();
    std::fs::write(work().join("postmove-fresh-child.log"),[child.stdout.as_slice(),child.stderr.as_slice()].concat()).unwrap();assert!(child.status.success(),"new-origin fresh recovery: {} {}",String::from_utf8_lossy(&child.stdout),String::from_utf8_lossy(&child.stderr));assert!(String::from_utf8_lossy(&child.stdout).contains("POSTMOVE_NEW_ORIGIN_NATIVE_CURRENT_HISTORY_RETIREMENT_FRESH_RECOVERY_PASS"));
    println!("POSTMOVE_NEW_ORIGIN_NATIVE_CURRENT_HISTORY_RETIREMENT_FRESH_RECOVERY_PASS");
    println!("POST_MOVE_REPLACEMENT_REAL_NEW_ORIGIN_OLD_RECEIPTS_CURRENT_HISTORY_CENSUS_EXACT_RETRY_REPLAY_PASS");
}

#[tokio::test]#[ignore="genuine postmove producer supplies private exact source evidence"]
async fn fresh_postmove_version_child(){
    let e:Value=serde_json::from_slice(&std::fs::read(work().join("postmove-expected.json")).unwrap()).unwrap();let source=&e["source"];let root=crate::storage::data_dir();assert!(root.is_absolute()&&root.starts_with(work()));
    let db=real::pool(source["url"].as_str().unwrap(),source["schema"].as_str().unwrap()).await;
    crate::storage::restoration::bootstrap_gate(&root).unwrap();crate::storage::restoration::activate_in(&root,&db).await.unwrap();crate::storage::relocation::recover(&db).await.unwrap();crate::storage::journal::recover(&db).await.unwrap();crate::storage::canonical_rewrite::recover(&db).await.unwrap();crate::storage::publication::recover(&db).await.unwrap();crate::storage::attachment_upload::recover_in(&root,&db).await.unwrap();let s=real::state(db,&root);
    assert_eq!(immutable_sql(&s.db).await,e["immutable"]);assert_eq!(retirement_rows(&s.db).await,e["retirements"]);
    let owner=source["owner"].as_str().unwrap();assert_eq!(get(&s,owner,e["body"].as_str().unwrap()).await,(StatusCode::OK,b"GENUINE NEW DESTINATION VERSION\0".to_vec()));assert_ne!(get(&s,source["viewer"].as_str().unwrap(),e["body"].as_str().unwrap()).await.0,StatusCode::OK);
    let mut c=s.db.acquire().await.unwrap();let registry=crate::storage::attachment_versions::inspect_registry_snapshot(&mut c).await.unwrap();assert_eq!(registry.versions().len(),3);assert_eq!(registry.versions()[2].original_space_id,"upload-destination");assert_eq!(registry.current_associations()[e["version"]["version_id"].as_str().unwrap()].space_id,e["current_target"].as_str().unwrap());drop(c);s.db.close().await;
    println!("POSTMOVE_NEW_ORIGIN_NATIVE_CURRENT_HISTORY_RETIREMENT_FRESH_RECOVERY_PASS");
}
