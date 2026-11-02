//! Candidate sibling module inside spaces/publication_snapshots.rs.
use super::*;
use crate::spaces::drafts::draft_reader_current_tests as fixture;
use crate::AppState;
use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
use tower::ServiceExt;
async fn http(s:&AppState,method:&str,path:&str,body:Option<Value>)->(StatusCode,Value){
    let mut req=Request::builder().method(method).uri(path).header("Authorization",format!("Bearer {}",fixture::token(s,"owner-1")));
    if body.is_some(){req=req.header("Content-Type","application/json")}
    let response=crate::api_routes().with_state(s.clone()).oneshot(req.body(body.map(|v|Body::from(v.to_string())).unwrap_or_else(Body::empty)).unwrap()).await.unwrap();
    let status=response.status();let bytes=to_bytes(response.into_body(),8_000_000).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap_or_else(|_|json!({"raw":String::from_utf8_lossy(&bytes)})))
}
const PUBLISH:&str="/spaces/space-1/documents/doc-1/drafts/draft-1/publish";
async fn ready()->(AppState,String,std::path::PathBuf,Value){
    let(s,slug,path)=fixture::fixture().await;
    sqlx::query("UPDATE documents SET published_at=updated_at WHERE id='doc-1'").execute(&s.db).await.unwrap();
    let(status,source)=http(&s,"GET","/spaces/space-1/documents/doc-1/drafts/draft-1/publish-source",None).await;assert_eq!(status,StatusCode::OK,"{source}");
    let request=json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"draft_source_digest":source["draft_source_digest"],"page_source_digest":source["page_source_digest"]});
    (s,slug,path,request)
}
/// Genuine old producer path: prepare base-L3 fingerprint BEFORE typed captures,
/// rollback SQL, then pass exact original request to upgraded route for resume.
async fn legacy_orphan(s:&AppState,request:&Value){
    use crate::spaces::{draft_save,draft_publication as core,document_deletion::rows};
    let auth=fixture::auth(s,"owner-1").await;let mut tx=s.db.begin().await.unwrap();
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await.unwrap();
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx,&["space-1"]).await.unwrap();
    let draft=rows(&mut tx,"SELECT * FROM document_drafts WHERE id=$1",&["draft-1"]).await.unwrap().remove(0);
    let page=rows(&mut tx,"SELECT * FROM documents WHERE id=$1",&["doc-1"]).await.unwrap().remove(0);
    let req:core::Request=serde_json::from_value(request.clone()).unwrap();
    let fingerprint=core::storage_fingerprint(&auth,"space-1","doc-1","draft-1",&req,&draft).unwrap();
    let title=draft_save::string(&draft,"title").unwrap();let icon=draft_save::optional(&draft,"icon").unwrap();
    let body=crate::template_guidance::strip_template_guidance(&draft_save::string(&draft,"content").unwrap());
    let front=crate::spaces::documents::document_storage_frontmatter(&title,icon.as_deref(),page["locked"].as_i64()==Some(1),page["public"].as_i64()==Some(1));
    crate::storage::publication::prepare_publication(&mut tx,&auth,crate::storage::publication::PublicationIntent{
        op_id:req.operation_id,space_id:"space-1".into(),document_id:"doc-1".into(),draft_id:"draft-1".into(),source_path:draft_save::string(&page,"content_path").unwrap(),source_revision:draft_save::string(&page,"updated_at").unwrap(),revision:chrono::Utc::now().to_rfc3339(),payload_fingerprint:fingerprint
    },&front,&body).await.unwrap();tx.rollback().await.unwrap();
}
#[tokio::test]
async fn authentic_preupgrade_orphan_captures_legacy_content_and_retries_without_metadata_retrofit(){
    let(s,slug,path,req)=ready().await;legacy_orphan(&s,&req).await;
    let(status,first)=http(&s,"POST",PUBLISH,Some(req.clone())).await;assert_eq!(status,StatusCode::OK,"{first}");
    let rows=read_validated(&mut *s.db.acquire().await.unwrap(),"doc-1").await.unwrap();assert_eq!(rows.len(),2);assert!(rows.iter().all(|r|r.payload.binding==Binding::LegacyContentV1));
    sqlx::query("INSERT INTO properties_schema(id,space_id,name,property_type) VALUES('late-history','space-1','Later live definition','text')").execute(&s.db).await.unwrap();
    let(status,retry)=http(&s,"POST",PUBLISH,Some(req)).await;assert_eq!(status,StatusCode::OK,"{retry}");let mut expected=first;expected["disposition"]=json!("historical");assert_eq!(retry,expected);
    assert_eq!(read_validated(&mut *s.db.acquire().await.unwrap(),"doc-1").await.unwrap(),rows);fixture::cleanup(s,&slug,&path).await;
}
#[tokio::test]
async fn authentic_legacy_baseline_coherent_portable_claim_tampering_is_not_immutable_history(){
    let(s,slug,path,req)=ready().await;legacy_orphan(&s,&req).await;
    assert_eq!(http(&s,"POST",PUBLISH,Some(req)).await.0,StatusCode::OK);
    let mut baseline=read_validated(&mut *s.db.acquire().await.unwrap(),"doc-1").await.unwrap().into_iter().find(|r|r.kind=="baseline").unwrap();
    baseline.payload.content.portable_markdown=Some("FORGED BODY".into());baseline.payload.content.extraction=Extraction::ProvenGeneratedHeader;
    sqlx::query("UPDATE document_publication_snapshots SET payload_json=$1,payload_digest=$2 WHERE id=$3").bind(serde_json::to_string(&baseline.payload).unwrap()).bind(digest(&json!(baseline.payload)).unwrap()).bind(&baseline.id).execute(&s.db).await.unwrap();
    assert!(read_validated(&mut *s.db.acquire().await.unwrap(),"doc-1").await.is_err());
    assert_ne!(http(&s,"GET","/spaces/space-1/documents/doc-1/publication-snapshots",None).await.0,StatusCode::OK);fixture::cleanup(s,&slug,&path).await;
}
#[tokio::test]
async fn actual_route_snapshot_abort_and_coherent_trigger_mutation_roll_back_entire_publication(){
    for action in ["SELECT RAISE(ABORT,'snapshot refused');","UPDATE document_publication_snapshots SET captured_by='foreign' WHERE id=NEW.id;"] {
        let(s,slug,path,req)=ready().await;
        sqlx::query(&format!("CREATE TRIGGER snapshot_route_fail AFTER INSERT ON document_publication_snapshots BEGIN {action} END")).execute(&s.db).await.unwrap();
        let before=fixture::snapshot(&s,true).await;
        let(status,response)=http(&s,"POST",PUBLISH,Some(req)).await;assert_ne!(status,StatusCode::OK,"{response}");
        assert_eq!(fixture::snapshot(&s,true).await,before,"all SQL including page/draft/audit/receipt/capture must roll back");
        let mut conn=s.db.acquire().await.unwrap();let page=crate::spaces::document_deletion::rows(&mut conn,"SELECT * FROM documents WHERE id=$1",&["doc-1"]).await.unwrap().remove(0);
        assert_eq!(crate::spaces::draft_publication::page_bytes(&page).await.unwrap().unwrap(),b"PUBLIC OLD UNIQUE");drop(conn);fixture::cleanup(s,&slug,&path).await;
    }
}
#[tokio::test]
async fn genuine_old_committed_receipt_has_no_typed_history_and_retry_never_retrofits(){
    use crate::spaces::{draft_save,draft_publication as core,document_deletion::rows};
    let(s,slug,path,request)=ready().await;legacy_orphan(&s,&request).await;
    let auth=fixture::auth(&s,"owner-1").await;let req:core::Request=serde_json::from_value(request.clone()).unwrap();
    let prepared=crate::storage::publication::retained_publication(&req.operation_id).unwrap().unwrap();
    // Execute the original successful L3 SQL+physical receipt producers, without
    // calling upgraded POST/capture or rewriting any native manifest/fingerprint.
    let mut tx=s.db.begin().await.unwrap();crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await.unwrap();crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx,&["space-1"]).await.unwrap();
    let draft=rows(&mut tx,"SELECT * FROM document_drafts WHERE id=$1",&["draft-1"]).await.unwrap().remove(0);
    sqlx::query("UPDATE documents SET title=$1,icon=$2,cover_image=$3,last_updated_by='owner-1',updated_at=$4,published_at=$4,content_path=$5 WHERE id='doc-1'").bind(draft_save::string(&draft,"title").unwrap()).bind(draft_save::optional(&draft,"icon").unwrap()).bind(draft_save::optional(&draft,"cover_image").unwrap()).bind(prepared.revision()).bind(prepared.path()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO audit_log(id,user_id,actor_kind,actor_id,action,target_type,target_id) VALUES($1,'owner-1','user','owner-1','document.draft_publish','document','doc-1')").bind(uuid::Uuid::new_v4().to_string()).execute(&mut *tx).await.unwrap();
    crate::storage::publication::record_publication(&mut tx,&auth,&prepared).await.unwrap();
    let page=rows(&mut tx,"SELECT * FROM documents WHERE id=$1",&["doc-1"]).await.unwrap().remove(0);let body=crate::template_guidance::strip_template_guidance(&draft_save::string(&draft,"content").unwrap());
    let recorded=core::record(&mut tx,&auth,"space-1","doc-1","draft-1",&req,&draft,&page,&prepared,&body).await.unwrap();
    sqlx::query("DELETE FROM document_draft_access WHERE draft_id='draft-1'").execute(&mut *tx).await.unwrap();sqlx::query("DELETE FROM document_drafts WHERE id='draft-1'").execute(&mut *tx).await.unwrap();crate::storage::publication::verify_prepared_publication(&prepared).unwrap();tx.commit().await.unwrap();
    assert!(read_validated(&mut *s.db.acquire().await.unwrap(),"doc-1").await.unwrap().is_empty());
    let(status,listing)=http(&s,"GET","/spaces/space-1/documents/doc-1/publication-snapshots",None).await;assert_eq!(status,StatusCode::OK,"{listing}");assert_eq!(listing["snapshots"],json!([]));
    let(status,retry)=http(&s,"POST",PUBLISH,Some(request)).await;assert_eq!(status,StatusCode::OK,"{retry}");let mut expected=recorded.response;expected["disposition"]=json!("historical");assert_eq!(retry,expected);
    let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_publication_snapshots").fetch_one(&s.db).await.unwrap();assert_eq!(count,0);fixture::cleanup(s,&slug,&path).await;
}
#[tokio::test]
async fn v2_historical_retry_keeps_exact_capture_after_live_schema_and_values_change(){
    let(s,slug,path,request)=ready().await;let(status,first)=http(&s,"POST",PUBLISH,Some(request.clone())).await;assert_eq!(status,StatusCode::OK,"{first}");
    let original=read_validated(&mut *s.db.acquire().await.unwrap(),"doc-1").await.unwrap();assert!(original.iter().all(|r|r.payload.binding==Binding::CaptureV2));
    sqlx::query("INSERT INTO properties_schema(id,space_id,name,property_type,options) VALUES('after-capture','space-1','Later schema','select','{invalid raw options')").execute(&s.db).await.unwrap();
    sqlx::query("INSERT INTO property_values(id,document_id,property_id,value) VALUES('after-value','doc-1','after-capture','Later raw value')").execute(&s.db).await.unwrap();
    let(status,retry)=http(&s,"POST",PUBLISH,Some(request)).await;assert_eq!(status,StatusCode::OK,"{retry}");let mut expected=first;expected["disposition"]=json!("historical");assert_eq!(retry,expected);
    assert_eq!(read_validated(&mut *s.db.acquire().await.unwrap(),"doc-1").await.unwrap(),original);fixture::cleanup(s,&slug,&path).await;
}

/// Disposable producer: real reviewed publication + snapshot recovery, then the
/// genuine stopped SQLite capture path. No receipt/custody rows are invented.
#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "requires fresh absolute DATA_DIR and new STARLANE_ENGINE_SOURCE_BUNDLE in owned fixture workspace; run alone"]
async fn required_populated54_runtime_publication_backup_producer() {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let root = crate::storage::data_dir();
    let target = std::path::PathBuf::from(std::env::var("STARLANE_ENGINE_SOURCE_BUNDLE").expect("new capture target required"));
    assert!(root.is_absolute() && target.is_absolute() && !root.exists() && !target.exists(), "Require new absolute owned roots");
    let parent = root.parent().unwrap();
    assert!(parent.is_dir(), "Owned fixture parent must already exist");
    std::fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
    crate::storage::init_data_dir().await.unwrap();
    // Existing L4 fixture chooses its live file database beneath temp_dir. Keep
    // it on the same owned filesystem, so final relocation preserves its inode.
    let old_temp = std::env::var_os("TMPDIR");
    std::env::set_var("TMPDIR", parent);
    let (state, _slug, database, request) = ready().await;
    match old_temp { Some(value) => std::env::set_var("TMPDIR", value), None => std::env::remove_var("TMPDIR") }
    assert_eq!(database.parent(), Some(parent));
    let (status, published) = http(&state, "POST", PUBLISH, Some(request.clone())).await;
    assert_eq!(status, StatusCode::OK, "{published}");
    let snapshot = format!("{}:publication", request["operation_id"].as_str().unwrap());
    let prefix = format!("/spaces/space-1/documents/doc-1/publication-snapshots/{snapshot}");
    let (status, reviewed) = http(&state, "GET", &format!("{prefix}/recovery-source"), None).await;
    assert_eq!(status, StatusCode::OK, "{reviewed}");
    let recover = json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),
        "snapshot_source_digest":reviewed["snapshot_source_digest"],"page_source_digest":reviewed["page_source_digest"]});
    let (status, recovered) = http(&state, "POST", &format!("{prefix}/recover"), Some(recover)).await;
    assert_eq!(status, StatusCode::CREATED, "{recovered}");
    for table in ["draft_publication_receipts", "document_publication_snapshots", "snapshot_recovery_receipts"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}")).fetch_one(&state.db).await.unwrap();
        assert!(count > 0, "Actual runtime did not produce {table}");
    }
    let raw: Vec<u8> = sqlx::query_scalar("SELECT raw_markdown FROM document_publication_snapshots WHERE id=$1")
        .bind(&snapshot).fetch_one(&state.db).await.unwrap();
    assert!(!raw.is_empty());
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT MAX(version) FROM _sqlx_migrations").fetch_one(&state.db).await.unwrap(), 54);
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&state.db).await.unwrap();
    state.db.close().await;
    drop(state);
    let canonical = root.join("starlane-notes.db");
    assert!(!canonical.exists());
    std::fs::rename(&database, &canonical).unwrap();
    std::fs::set_permissions(&canonical, std::fs::Permissions::from_mode(0o600)).unwrap();
    let bundle = crate::storage::restoration::linux::capture_sqlite_certified(&root, &target, "1.5.3", true).await.unwrap();
    bundle.fence().unwrap();
    assert_eq!(bundle.manifest.compatible_catalogue(), "sqlite/54");
    assert_eq!(sqlx::query_scalar::<_, Vec<u8>>("SELECT raw_markdown FROM document_publication_snapshots WHERE id=$1")
        .bind(&snapshot).fetch_one(&bundle.db).await.unwrap(), raw);
    bundle.db.close().await;
    println!("GENUINE_POPULATED54_RUNTIME_PUBLICATION_RECOVERY_CAPTURE_PASS actual_reviewed_handlers3families raw_blob_exact no_receipt_or_custody_seeding");
}
