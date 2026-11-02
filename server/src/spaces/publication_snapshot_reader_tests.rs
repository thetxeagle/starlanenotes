use super::*;
use super::super::drafts::draft_reader_current_tests as fixture;
use axum::{body::to_bytes,http::{Request,StatusCode}};
use tower::ServiceExt;
async fn http(s:&AppState,user:&str,method:&str,path:&str,body:Option<Value>)->axum::response::Response{
    let mut req=Request::builder().method(method).uri(path).header("Authorization",format!("Bearer {}",fixture::token(s,user)));
    if body.is_some(){req=req.header("Content-Type","application/json")}
    let req=req.body(body.map(|v|Body::from(v.to_string())).unwrap_or_else(Body::empty)).unwrap();
    if path.contains("publication-snapshots"){router().with_state(s.clone()).oneshot(req).await.unwrap()}else{crate::api_routes().with_state(s.clone()).oneshot(req).await.unwrap()}
}
async fn json_response(r:axum::response::Response)->(StatusCode,Value){let status=r.status();(status,serde_json::from_slice(&to_bytes(r.into_body(),8_000_000).await.unwrap()).unwrap())}
async fn setup()->(AppState,String,std::path::PathBuf,String,String){
    let(s,slug,path)=fixture::fixture().await;
    sqlx::query("PRAGMA journal_mode=WAL").execute(&s.db).await.unwrap();
    let journal:String=sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&s.db).await.unwrap();assert_eq!(journal.to_lowercase(),"wal");
    sqlx::query("UPDATE documents SET published_at=updated_at WHERE id='doc-1'").execute(&s.db).await.unwrap();
    sqlx::query("INSERT INTO properties_schema(id,space_id,name,property_type,options,sort_order) VALUES('linked-schema','space-1','Observed schema','select','{invalid raw options',0),('unrelated-schema','space-1','SECRET UNRELATED SCHEMA','text',NULL,1)").execute(&s.db).await.unwrap();
    sqlx::query("INSERT INTO property_values(id,document_id,property_id,value) VALUES('observed-value','doc-1','linked-schema','EXACT RAW VALUE')").execute(&s.db).await.unwrap();
    sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES('observed-asset','doc-1','space-1','INTERNAL FILENAME','safe.png','image/png',7,'SECRET INTERNAL STORAGE PATH','owner-1')").execute(&s.db).await.unwrap();
        {let mut legacy=s.db.acquire().await.unwrap();crate::spaces::asset_audiences::admit_legacy(&mut legacy,"observed-asset","space-1","doc-1").await.unwrap();}
    sqlx::query("INSERT INTO doc_versions(id,document_id,version_number,created_by,change_set) VALUES('legacy-unchanged','doc-1',1,'owner-1',X'010203')").execute(&s.db).await.unwrap();
    let(status,review)=json_response(http(&s,"owner-1","GET","/spaces/space-1/documents/doc-1/drafts/draft-1/publish-source",None).await).await;assert_eq!(status,StatusCode::OK,"{review}");
    let op=uuid::Uuid::new_v4().to_string();let req=json!({"schema_version":1,"operation_id":op,"draft_source_digest":review["draft_source_digest"],"page_source_digest":review["page_source_digest"]});
    let(status,response)=json_response(http(&s,"owner-1","POST","/spaces/space-1/documents/doc-1/drafts/draft-1/publish",Some(req)).await).await;assert_eq!(status,StatusCode::OK,"{response}");
    (s,slug,path,format!("{op}:publication"),format!("{op}:baseline"))
}
fn url(id:Option<&str>)->String{let mut out="/spaces/space-1/documents/doc-1/publication-snapshots".to_string();if let Some(id)=id{out.push('/');out.push_str(id)}out}
#[tokio::test]
async fn actual_signed_history_current_edit_productsafety_raw_baseline_and_legacy_unchanged(){
    let(s,slug,path,id,baseline)=setup().await;let before=fixture::snapshot(&s,true).await;
    let response=http(&s,"editor-1","GET",&url(None),None).await;assert_eq!(response.headers()[header::CACHE_CONTROL],"no-store");let(status,list)=json_response(response).await;assert_eq!(status,StatusCode::OK,"{list}");assert_eq!(list["space_id"],"space-1");assert_eq!(list["document_id"],"doc-1");assert_eq!(list["snapshots"].as_array().unwrap().len(),2);
    let response=http(&s,"editor-1","GET",&url(Some(&id)),None).await;assert_eq!(response.headers()[header::CACHE_CONTROL],"no-store");let(status,detail)=json_response(response).await;assert_eq!(status,StatusCode::OK,"{detail}");let row=&detail["snapshot"];
    assert_eq!(row["content"]["portable_markdown"],"Original body");assert_eq!(row["comparisons"]["properties_schema"].as_array().unwrap().len(),1);
    assert_eq!(row["comparisons"]["properties_schema"][0]["options_raw"],"{invalid raw options");assert_eq!(row["comparisons"]["property_values"][0]["value"],"EXACT RAW VALUE");
    let rendered=detail.to_string();for forbidden in ["SECRET UNRELATED SCHEMA","SECRET INTERNAL STORAGE PATH","INTERNAL FILENAME","baseline_source_page","actor_kind","request_json","storage_manifest_json","change_set","locked_by","space_members"]{assert!(!rendered.contains(forbidden),"leaked {forbidden}")}
    for target in [&id,&baseline]{let response=http(&s,"editor-1","GET",&format!("{}/raw",url(Some(target))),None).await;assert_eq!(response.status(),StatusCode::OK);assert_eq!(response.headers()[header::CACHE_CONTROL],"no-store");assert_eq!(response.headers()[header::CONTENT_TYPE],"application/octet-stream");assert_eq!(response.headers()[header::CONTENT_DISPOSITION],"attachment; filename=\"publication-snapshot.md\"");assert_eq!(response.headers()["X-Starlane-Snapshot-Id"],target.as_str());let length=response.headers()[header::CONTENT_LENGTH].to_str().unwrap().parse::<usize>().unwrap();let raw=to_bytes(response.into_body(),8_000_000).await.unwrap();assert_eq!(raw.len(),length);if target==&baseline{assert_eq!(raw.as_ref(),b"PUBLIC OLD UNIQUE")}else{assert!(String::from_utf8(raw.to_vec()).unwrap().ends_with("Original body"))}}
    assert_eq!(fixture::snapshot(&s,true).await,before);fixture::cleanup(s,&slug,&path).await;
}
#[tokio::test]
async fn actual_signed_history_custom_edit_without_view_positive_viewers_foreign_ids_and_read_only_keys_denied(){
    let(s,slug,path,id,_)=setup().await;
    sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('history-editor','space-1','Write only','[\"edit_content\"]','owner-1')").execute(&s.db).await.unwrap();
    sqlx::query("UPDATE space_members SET role='history-editor' WHERE user_id='editor-1'").execute(&s.db).await.unwrap();
    for target in [url(None),url(Some(&id)),format!("{}/raw",url(Some(&id)))]{let r=http(&s,"editor-1","GET",&target,None).await;assert_eq!(r.status(),StatusCode::OK,"custom ordinary Edit must work");assert_ne!(http(&s,"viewer-1","GET",&target,None).await.status(),StatusCode::OK)}
    sqlx::query("INSERT INTO document_viewer_grants(id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at) VALUES('history-viewer','doc-1','space-1','viewer-1','viewer','page','original','owner-1','created','updated')").execute(&s.db).await.unwrap();
    sqlx::query("DELETE FROM space_members WHERE user_id='viewer-1'").execute(&s.db).await.unwrap();
    for guest in [false,true]{
        if guest {sqlx::query("UPDATE users SET account_kind='guest' WHERE id='viewer-1'").execute(&s.db).await.unwrap();}
        assert_eq!(http(&s,"viewer-1","GET","/spaces/space-1/documents/doc-1/content",None).await.status(),StatusCode::OK,"direct viewer has current published read");
        for target in [url(None),url(Some(&id)),format!("{}/raw",url(Some(&id)))]{assert_ne!(http(&s,"viewer-1","GET",&target,None).await.status(),StatusCode::OK,"published direct/guest View cannot inherit old history");}
    }
    for target in [url(Some("missing")),url(Some(&id)).replace("space-1","foreign"),url(Some(&id)).replace("doc-1","foreign")]{assert_ne!(http(&s,"editor-1","GET",&target,None).await.status(),StatusCode::OK)}
    for(scopes,label,expected)in [("[\"read\"]","history-read",false),("[\"read\",\"write\"]","history-write",true)]{
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'editor-1',$1,$2,'test',$3)").bind(label).bind(crate::auth::api_keys::hash_api_key(label)).bind(scopes).execute(&s.db).await.unwrap();
        let r=router().with_state(s.clone()).oneshot(Request::builder().uri(url(Some(&id))).header("X-API-Key",label).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(r.status()==StatusCode::OK,expected);
    }
    fixture::cleanup(s,&slug,&path).await;
}
#[tokio::test]
async fn disk_wal_snapshot_and_current_policy_drift_after_capture_rejects_without_reader_writes(){
    for mutation in ["UPDATE document_publication_snapshots SET payload_json='{}' WHERE kind='publication'","UPDATE properties_schema SET options='{new invalid raw' WHERE id='linked-schema'","UPDATE users SET is_active=0 WHERE id='editor-1'","DELETE FROM sessions WHERE id='publication-editor-1'","UPDATE space_members SET role='viewer' WHERE user_id='editor-1'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','editor-1','owner-1')","UPDATE documents SET archived_at='later' WHERE id='doc-1'","INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('doc-1','space-1','restricted','changed','owner-1')"]{
        let(s,slug,path,id,_)=setup().await;let db=s.db.clone();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let flag=reached.clone();
        let response=BEFORE_INVENTORY.scope(std::cell::RefCell::new(Some(Box::pin(async move{sqlx::query(mutation).execute(&db).await.unwrap();flag.store(true,std::sync::atomic::Ordering::SeqCst);}))),http(&s,"editor-1","GET",&url(Some(&id)),None)).await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert_ne!(response.status(),StatusCode::OK,"{mutation}");
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_publication_snapshots").fetch_one(&s.db).await.unwrap();assert_eq!(count,2);fixture::cleanup(s,&slug,&path).await;
    }
}
#[tokio::test]
async fn disk_wal_final_reproof_denies_selected_corruption_and_live_identity_resource_loss(){
    for mutation in ["UPDATE document_publication_snapshots SET raw_sha256='00' WHERE kind='baseline'","DELETE FROM sessions WHERE id='publication-editor-1'","UPDATE documents SET deleted_at='later' WHERE id='doc-1'"]{
        let(s,slug,path,id,_)=setup().await;let db=s.db.clone();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let flag=reached.clone();
        let response=AFTER_INVENTORY.scope(std::cell::RefCell::new(Some(Box::pin(async move{sqlx::query(mutation).execute(&db).await.unwrap();flag.store(true,std::sync::atomic::Ordering::SeqCst);}))),http(&s,"editor-1","GET",&format!("{}/raw",url(Some(&id))),None)).await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert_ne!(response.status(),StatusCode::OK,"{mutation}");fixture::cleanup(s,&slug,&path).await;
    }
}

#[tokio::test]
async fn captured_history_survives_old_canonical_rewrite_and_missing_installed_serving_path(){
    let(s,slug,path,id,baseline)=setup().await;
    let stored=core::read_validated(&mut *s.db.acquire().await.unwrap(),"doc-1").await.unwrap();
    let result=stored.iter().find(|r|r.id==id).unwrap().raw.clone();let original=stored.iter().find(|r|r.id==baseline).unwrap().raw.clone();
    let current:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();
    // DB history is the download source; serving files are not snapshot custody.
    let baseline_path=stored.iter().find(|r|r.id==baseline).unwrap().payload.baseline_source_page.as_ref().unwrap()["content_path"].as_str().unwrap();
    std::fs::write(crate::storage::data_dir().join(baseline_path),b"REWRITTEN OLD SERVING FILE").unwrap();
    std::fs::remove_file(crate::storage::data_dir().join(current)).unwrap();
    for (target,expected) in [(&id,&result),(&baseline,&original)]{
        let response=http(&s,"editor-1","GET",&format!("{}/raw",url(Some(target))),None).await;assert_eq!(response.status(),StatusCode::OK,"history must not reopen deleted original serving path");assert_eq!(to_bytes(response.into_body(),8_000_000).await.unwrap().as_ref(),expected.as_slice());
    }
    fixture::cleanup(s,&slug,&path).await;
}

#[tokio::test]
async fn actual_signed_reader_missing_full_v2_group_and_coherent_row_id_rename_are_never_empty_or_repaired(){
    for mutation in ["DELETE FROM document_publication_snapshots","UPDATE document_publication_snapshots SET id='renamed-coherent-payload' WHERE kind='publication'"]{
        let(s,slug,path,id,_)=setup().await;
        sqlx::query(mutation).execute(&s.db).await.unwrap();let before=fixture::snapshot(&s,true).await;
        for target in [url(None),url(Some(&id)),format!("{}/raw",url(Some(&id))),url(Some("renamed-coherent-payload"))]{
            let response=http(&s,"editor-1","GET",&target,None).await;assert_ne!(response.status(),StatusCode::OK,"missing or rebound v2 group cannot be served: {mutation}");
        }
        assert_eq!(fixture::snapshot(&s,true).await,before,"GET must not repair missing/rebound groups");fixture::cleanup(s,&slug,&path).await;
    }
}
