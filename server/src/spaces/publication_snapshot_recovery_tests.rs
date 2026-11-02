// Candidate sibling module in the new snapshot-recovery backend module.
// Uses only actual signed API routes and existing L4 file-backed fixture.
use crate::{AppState,spaces::drafts::draft_reader_current_tests as fixture};
use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
use serde_json::{json,Value};
use tower::ServiceExt;
const OLD_BODY:&str="---\nsynopsis: historical text\nunknown: exact value\ncustom_css: |\n  .section { color: inherit; }\n---\n\nOlder portable body  \n\n";
async fn http(s:&AppState,user:&str,method:&str,path:&str,body:Option<Value>)->(StatusCode,Value){
    let mut req=Request::builder().method(method).uri(path).header("Authorization",format!("Bearer {}",fixture::token(s,user)));
    if body.is_some(){req=req.header("Content-Type","application/json")}
    let response=crate::api_routes().with_state(s.clone()).oneshot(req.body(body.map(|v|Body::from(v.to_string())).unwrap_or_else(Body::empty)).unwrap()).await.unwrap();
    let status=response.status();let bytes=to_bytes(response.into_body(),8_000_000).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap_or_else(|_|json!({"raw":String::from_utf8_lossy(&bytes)})))
}
async fn publish(s:&AppState,draft:&str)->String{
    let path=format!("/spaces/space-1/documents/doc-1/drafts/{draft}");
    let(status,source)=http(s,"owner-1","GET",&format!("{path}/publish-source"),None).await;assert_eq!(status,StatusCode::OK,"{source}");
    let op=uuid::Uuid::new_v4().to_string();let req=json!({"schema_version":1,"operation_id":op,"draft_source_digest":source["draft_source_digest"],"page_source_digest":source["page_source_digest"]});
    let(status,response)=http(s,"owner-1","POST",&format!("{path}/publish"),Some(req)).await;assert_eq!(status,StatusCode::OK,"{response}");format!("{op}:publication")
}
struct Case {s:AppState,slug:String,path:std::path::PathBuf,snapshot:String}
impl Case {
    fn source(&self)->String{format!("/spaces/space-1/documents/doc-1/publication-snapshots/{}/recovery-source",self.snapshot)}
    fn recover(&self)->String{format!("/spaces/space-1/documents/doc-1/publication-snapshots/{}/recover",self.snapshot)}
    async fn review(&self,user:&str)->Value{let(status,source)=http(&self.s,user,"GET",&self.source(),None).await;assert_eq!(status,StatusCode::OK,"{source}");json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"snapshot_source_digest":source["snapshot_source_digest"],"page_source_digest":source["page_source_digest"]})}
    async fn cleanup(self){fixture::cleanup(self.s,&self.slug,&self.path).await}
}
async fn setup()->Case{
    let(s,slug,path)=fixture::fixture().await;sqlx::query("PRAGMA journal_mode=WAL").execute(&s.db).await.unwrap();
    let journal:String=sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&s.db).await.unwrap();assert_eq!(journal.to_lowercase(),"wal");
    sqlx::query("UPDATE documents SET published_at=updated_at WHERE id='doc-1'").execute(&s.db).await.unwrap();
    sqlx::query("UPDATE document_drafts SET title='Historical recovered title',content=$1,icon='compass',cover_image='https://invalid.example/historical-cover.png' WHERE id='draft-1'").bind(OLD_BODY).execute(&s.db).await.unwrap();
    let snapshot=publish(&s,"draft-1").await;
    let base:String=sqlx::query_scalar("SELECT updated_at FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();
    sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,icon,cover_image) VALUES('newer-publication','doc-1','space-1','owner-1','Current published title','CURRENT PUBLISHED BODY',$1,'file-text',NULL)").bind(base).execute(&s.db).await.unwrap();publish(&s,"newer-publication").await;
    sqlx::query("INSERT INTO folders(id,space_id,name) VALUES('recovery-current-folder','space-1','Current folder')").execute(&s.db).await.unwrap();sqlx::query("UPDATE documents SET folder_id='recovery-current-folder' WHERE id='doc-1'").execute(&s.db).await.unwrap();
    sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('untouched-editor-work','doc-1','space-1','editor-1','Existing private work','DO NOT OVERWRITE','earlier-base')").execute(&s.db).await.unwrap();
    sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('untouched-editor-work','owner-1','editor','editor-1')").execute(&s.db).await.unwrap();
    sqlx::query("INSERT INTO properties_schema(id,space_id,name,property_type,options) VALUES('current-property','space-1','Current property','select','{raw unsupported options')").execute(&s.db).await.unwrap();sqlx::query("INSERT INTO property_values(id,document_id,property_id,value) VALUES('current-value','doc-1','current-property','CURRENT RAW VALUE')").execute(&s.db).await.unwrap();
    sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES('current-asset','doc-1','space-1','current.png','current.png','image/png',7,'spaces/unused-current-asset','owner-1')").execute(&s.db).await.unwrap();
        {let mut legacy=s.db.acquire().await.unwrap();crate::spaces::asset_audiences::admit_legacy(&mut legacy,"current-asset","space-1","doc-1").await.unwrap();}
    sqlx::query("INSERT INTO doc_versions(id,document_id,version_number,created_by,change_set) VALUES('untouched-legacy','doc-1',1,'owner-1',X'010203')").execute(&s.db).await.unwrap();Case{s,slug,path,snapshot}
}
#[tokio::test]
async fn older_capture_creates_exact_actor_owned_draft_without_touching_current_domain(){
    let c=setup().await;let request=c.review("editor-1").await;let before=fixture::snapshot(&c.s,true).await;
    let mut conn=c.s.db.acquire().await.unwrap();let page=crate::spaces::document_deletion::rows(&mut conn,"SELECT * FROM documents WHERE id=$1",&["doc-1"]).await.unwrap().remove(0);let bytes=crate::spaces::draft_publication::page_bytes(&page).await.unwrap();drop(conn);
    let(status,result)=http(&c.s,"editor-1","POST",&c.recover(),Some(request.clone())).await;assert_eq!(status,StatusCode::CREATED,"{result}");assert_eq!(result["disposition"],"applied");
    let id=result["draft"]["id"].as_str().unwrap();assert_ne!(id,"untouched-editor-work");
    let mut conn=c.s.db.acquire().await.unwrap();let draft=crate::spaces::draft_save::selected(&mut conn,"space-1","doc-1",id).await.unwrap();
    for(key,value)in [("title",json!("Historical recovered title")),("content",json!(OLD_BODY)),("icon",json!("compass")),("cover_image",json!("https://invalid.example/historical-cover.png")),("owner_id",json!("editor-1")),("base_updated_at",page["updated_at"].clone()),("previous_folder_id",json!("recovery-current-folder")),("previous_parent_id",page["parent_id"].clone())]{assert_eq!(draft[key],value,"{key}")}
    let shares:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_draft_access WHERE draft_id=$1").bind(id).fetch_one(&mut *conn).await.unwrap();assert_eq!(shares,0);
    let current=crate::spaces::document_deletion::rows(&mut conn,"SELECT * FROM documents WHERE id=$1",&["doc-1"]).await.unwrap().remove(0);assert_eq!(current,page);assert_eq!(crate::spaces::draft_publication::page_bytes(&current).await.unwrap(),bytes);drop(conn);
    let after=fixture::snapshot(&c.s,true).await;
    let old_drafts=&before.iter().find(|(t,_)|t=="document_drafts").unwrap().1;
    let drafts=&after.iter().find(|(t,_)|t=="document_drafts").unwrap().1;
    assert_eq!(drafts.iter().filter(|r|r["id"]!=id).cloned().collect::<Vec<_>>(),*old_drafts,"every other draft remains exact");
    let old_audit=&before.iter().find(|(t,_)|t=="audit_log").unwrap().1;
    let audit=&after.iter().find(|(t,_)|t=="audit_log").unwrap().1;
    let added=audit.iter().filter(|r|!old_audit.contains(r)).collect::<Vec<_>>();assert_eq!(added.len(),1);assert_eq!(added[0]["action"],"document.snapshot_recover");assert_eq!(added[0]["user_id"],"editor-1");assert_eq!(added[0]["actor_id"],"editor-1");assert_eq!(added[0]["target_id"],id);
    for(table,rows)in before{if ["document_drafts","snapshot_recovery_receipts","audit_log"].contains(&table.as_str()){continue}assert_eq!(after.iter().find(|(t,_)|t==&table).unwrap().1,rows,"unrelated {table}");}
    let(status,retry)=http(&c.s,"editor-1","POST",&c.recover(),Some(request)).await;assert_eq!(status,StatusCode::OK,"{retry}");let mut historical=result;historical["disposition"]=json!("historical");assert_eq!(retry,historical);assert_eq!(fixture::snapshot(&c.s,true).await,after);c.cleanup().await;
}
#[tokio::test]
async fn concurrent_original_requests_and_lost_ack_reconcile_one_exact_private_result(){
    let c=setup().await;let req=c.review("editor-1").await;let url=c.recover();
    let barrier=std::sync::Arc::new(tokio::sync::Barrier::new(2));let left=barrier.clone();let right=barrier.clone();
    let(a,b)=tokio::join!(
        super::BEFORE_LOCK.scope(std::cell::RefCell::new(Some(Box::pin(async move{left.wait().await;}))),http(&c.s,"editor-1","POST",&url,Some(req.clone()))),
        super::BEFORE_LOCK.scope(std::cell::RefCell::new(Some(Box::pin(async move{right.wait().await;}))),http(&c.s,"editor-1","POST",&url,Some(req.clone())))
    );
    let statuses=[a.0,b.0];assert!(statuses.contains(&StatusCode::CREATED));assert!(statuses.contains(&StatusCode::OK));assert_eq!(a.1["draft"],b.1["draft"]);
    // First outcome intentionally not adopted; the original serialized intent is retried.
    let(status,retry)=http(&c.s,"editor-1","POST",&url,Some(req)).await;assert_eq!(status,StatusCode::OK,"{retry}");assert_eq!(retry["disposition"],"historical");assert_eq!(retry["draft"],a.1["draft"]);
    let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM snapshot_recovery_receipts").fetch_one(&c.s.db).await.unwrap();assert_eq!(count,1);
    let audit:i64=sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE action='document.snapshot_recover'").fetch_one(&c.s.db).await.unwrap();assert_eq!(audit,1);c.cleanup().await;
}
#[tokio::test]
async fn changed_or_deleted_result_never_rebases_or_creates_a_replacement(){
    for mutation in ["UPDATE document_drafts SET content='LATER WORK' WHERE id=$1","DELETE FROM document_drafts WHERE id=$1"]{
        let c=setup().await;let req=c.review("editor-1").await;let(status,result)=http(&c.s,"editor-1","POST",&c.recover(),Some(req.clone())).await;assert_eq!(status,StatusCode::CREATED,"{result}");let id=result["draft"]["id"].as_str().unwrap();
        sqlx::query(mutation).bind(id).execute(&c.s.db).await.unwrap();let before=fixture::snapshot(&c.s,true).await;
        let(status,response)=http(&c.s,"editor-1","POST",&c.recover(),Some(req)).await;assert_ne!(status,StatusCode::OK,"{response}");assert_ne!(status,StatusCode::CREATED,"{response}");assert_eq!(fixture::snapshot(&c.s,true).await,before);c.cleanup().await;
    }
}
#[tokio::test]
async fn recovery_sql_triggers_cannot_mutate_existing_work_metadata_or_owned_outcome(){
    for trigger in [
        "CREATE TRIGGER recover_poison AFTER UPDATE OF id ON documents BEGIN UPDATE document_drafts SET content='POISON OTHER WORK' WHERE id='untouched-editor-work'; END",
        "CREATE TRIGGER recover_poison AFTER INSERT ON document_drafts BEGIN SELECT RAISE(ABORT,'recovery insertion refused'); END",
        "CREATE TRIGGER recover_poison AFTER INSERT ON document_drafts BEGIN UPDATE document_drafts SET owner_id='owner-1' WHERE id=NEW.id; END",
        "CREATE TRIGGER recover_poison AFTER INSERT ON document_drafts BEGIN UPDATE document_drafts SET content='POISON OTHER WORK' WHERE id='untouched-editor-work'; END",
        "CREATE TRIGGER recover_poison AFTER INSERT ON document_drafts BEGIN UPDATE property_values SET value='POISON PROPERTY' WHERE id='current-value'; END",
        "CREATE TRIGGER recover_poison AFTER INSERT ON document_drafts BEGIN UPDATE properties_schema SET options='POISON SCHEMA' WHERE id='current-property'; END",
        "CREATE TRIGGER recover_poison AFTER INSERT ON document_drafts BEGIN INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES(NEW.id,'owner-1','editor','editor-1'); END",
        "CREATE TRIGGER recover_poison AFTER INSERT ON snapshot_recovery_receipts BEGIN UPDATE snapshot_recovery_receipts SET evidence_digest='00' WHERE operation_id=NEW.operation_id; END",
        "CREATE TRIGGER recover_poison AFTER INSERT ON audit_log WHEN NEW.action='document.snapshot_recover' BEGIN UPDATE audit_log SET actor_id='owner-1' WHERE id=NEW.id; END"
    ] {
        let c=setup().await;let req=c.review("editor-1").await;sqlx::query(trigger).execute(&c.s.db).await.unwrap();let before=fixture::snapshot(&c.s,true).await;
        let(status,result)=http(&c.s,"editor-1","POST",&c.recover(),Some(req)).await;assert_ne!(status,StatusCode::CREATED,"{trigger}: {result}");assert_ne!(status,StatusCode::OK,"{trigger}: {result}");assert_eq!(fixture::snapshot(&c.s,true).await,before,"rollback all SQL: {trigger}");c.cleanup().await;
    }
}
#[tokio::test]
async fn wal_late_current_source_lock_or_edit_revocation_is_rechecked_after_guard_wait(){
    for mutation in ["UPDATE documents SET title='SAME STAMP NEW TITLE' WHERE id='doc-1'","UPDATE documents SET locked=1,locked_by='owner-1' WHERE id='doc-1'","UPDATE space_members SET role='viewer' WHERE user_id='editor-1'","DELETE FROM sessions WHERE id='publication-editor-1'","UPDATE users SET is_active=0 WHERE id='editor-1'"]{
        let c=setup().await;let req=c.review("editor-1").await;let db=c.s.db.clone();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let flag=reached.clone();
        let(status,result)=super::BEFORE_LOCK.scope(std::cell::RefCell::new(Some(Box::pin(async move {sqlx::query(mutation).execute(&db).await.unwrap();flag.store(true,std::sync::atomic::Ordering::SeqCst);}))),http(&c.s,"editor-1","POST",&c.recover(),Some(req))).await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert_ne!(status,StatusCode::CREATED,"{mutation}: {result}");assert_ne!(status,StatusCode::OK,"{mutation}: {result}");let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM snapshot_recovery_receipts").fetch_one(&c.s.db).await.unwrap();assert_eq!(count,0);let drafts:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_drafts").fetch_one(&c.s.db).await.unwrap();assert_eq!(drafts,1);c.cleanup().await;
    }
}
#[tokio::test]
async fn explicit_new_review_may_create_a_second_copy_but_original_request_cannot_change_actor_or_inputs(){
    let c=setup().await;let request=c.review("editor-1").await;let(status,first)=http(&c.s,"editor-1","POST",&c.recover(),Some(request.clone())).await;assert_eq!(status,StatusCode::CREATED,"{first}");let before=fixture::snapshot(&c.s,true).await;
    for (actor,mutated) in [("owner-1",request.clone()),("editor-1",{let mut r=request.clone();r["page_source_digest"]=json!("0".repeat(64));r}), ("editor-1",{let mut r=request.clone();r["content"]=json!("CLIENT SUPPLIED BODY");r})]{let(status,result)=http(&c.s,actor,"POST",&c.recover(),Some(mutated)).await;assert_ne!(status,StatusCode::CREATED,"{result}");assert_ne!(status,StatusCode::OK,"{result}");assert_eq!(fixture::snapshot(&c.s,true).await,before);}
    let fresh=c.review("editor-1").await;let(status,second)=http(&c.s,"editor-1","POST",&c.recover(),Some(fresh)).await;assert_eq!(status,StatusCode::CREATED,"{second}");assert_ne!(first["draft"]["id"],second["draft"]["id"]);assert_eq!(first["draft"]["content"],second["draft"]["content"]);c.cleanup().await;
}
#[tokio::test]
async fn raw_only_keeps_download_but_never_guesses_an_editor_source(){
    let mut c=setup().await;c.snapshot=c.snapshot.strip_suffix(":publication").unwrap().to_owned()+":baseline";
    let(status,source)=http(&c.s,"editor-1","GET",&c.source(),None).await;assert_eq!(status,StatusCode::OK,"{source}");assert_eq!(source["recoverable"],false);assert_eq!(source["reason"],"raw_only");assert!(source["draft"].is_null());
    let req=json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"snapshot_source_digest":source["snapshot_source_digest"],"page_source_digest":source["page_source_digest"]});let before=fixture::snapshot(&c.s,true).await;let(status,result)=http(&c.s,"editor-1","POST",&c.recover(),Some(req)).await;assert_ne!(status,StatusCode::CREATED,"{result}");assert_ne!(status,StatusCode::OK,"{result}");assert_eq!(fixture::snapshot(&c.s,true).await,before);
    let raw=c.source().replace("/recovery-source","/raw");assert_eq!(http(&c.s,"editor-1","GET",&raw,None).await.0,StatusCode::OK);c.cleanup().await;
}
#[tokio::test]
async fn deleted_original_receipt_does_not_authorize_rebirth_even_if_target_is_gone(){
    for delete_target in [false,true]{
        let c=setup().await;let req=c.review("editor-1").await;let(status,result)=http(&c.s,"editor-1","POST",&c.recover(),Some(req.clone())).await;assert_eq!(status,StatusCode::CREATED,"{result}");
        sqlx::query("DELETE FROM snapshot_recovery_receipts WHERE operation_id=$1").bind(req["operation_id"].as_str().unwrap()).execute(&c.s.db).await.unwrap();
        if delete_target{sqlx::query("DELETE FROM document_drafts WHERE id=$1").bind(result["draft"]["id"].as_str().unwrap()).execute(&c.s.db).await.unwrap();}
        let before=fixture::snapshot(&c.s,true).await;let(status,response)=http(&c.s,"editor-1","POST",&c.recover(),Some(req)).await;assert_ne!(status,StatusCode::CREATED,"{response}");assert_ne!(status,StatusCode::OK,"{response}");assert_eq!(fixture::snapshot(&c.s,true).await,before,"missing original evidence is held, never rebuilt");c.cleanup().await;
    }
}
#[cfg(target_os="linux")]
include!("snapshot_recovery_restart_tests.rs");
#[tokio::test]
async fn actual_postcommit_unknown_ack_retries_exact_original_receipt_without_another_copy(){
    let c=setup().await;let req=c.review("editor-1").await;let(status,response)=super::UNKNOWN_ACK.scope(true,http(&c.s,"editor-1","POST",&c.recover(),Some(req.clone()))).await;assert_eq!(status,StatusCode::CONFLICT,"{response}");
    let before=fixture::snapshot(&c.s,true).await;let receipts:i64=sqlx::query_scalar("SELECT COUNT(*) FROM snapshot_recovery_receipts").fetch_one(&c.s.db).await.unwrap();assert_eq!(receipts,1);
    let(status,result)=http(&c.s,"editor-1","POST",&c.recover(),Some(req)).await;assert_eq!(status,StatusCode::OK,"{result}");assert_eq!(result["disposition"],"historical");assert_eq!(fixture::snapshot(&c.s,true).await,before);c.cleanup().await;
}

// Candidate to append INSIDE publication_snapshot_recovery::tests after current frozen run.
#[tokio::test]
async fn first_stale_review_is_explicitly_rejected_before_birth_and_can_be_reviewed_again(){
    let c=setup().await;let original=c.review("editor-1").await;
    sqlx::query("UPDATE documents SET title='Current source changed without timestamp' WHERE id='doc-1'").execute(&c.s.db).await.unwrap();
    let before=fixture::snapshot(&c.s,true).await;
    let(status,response)=http(&c.s,"editor-1","POST",&c.recover(),Some(original.clone())).await;
    assert_eq!(status,StatusCode::CONFLICT,"{response}");assert!(response.to_string().contains("recovery_rejected:"),"{response}");
    assert_eq!(fixture::snapshot(&c.s,true).await,before,"known fresh rejection makes no domain writes");
    let next=c.review("editor-1").await;assert_ne!(next["operation_id"],original["operation_id"]);
    let(status,result)=http(&c.s,"editor-1","POST",&c.recover(),Some(next)).await;assert_eq!(status,StatusCode::CREATED,"{result}");
    let before=fixture::snapshot(&c.s,true).await;
    sqlx::query("UPDATE document_drafts SET content='Target later changed' WHERE id=$1").bind(result["draft"]["id"].as_str().unwrap()).execute(&c.s.db).await.unwrap();
    let damaged=fixture::snapshot(&c.s,true).await;assert_ne!(before,damaged);
    // Historical damaged outcome remains held, never classified as a fresh safe rejection.
    let request=json!({"schema_version":1,"operation_id":result["operation_id"],"snapshot_source_digest":result["snapshot_source_digest"],"page_source_digest":result["page_source_digest"]});
    let(status,response)=http(&c.s,"editor-1","POST",&c.recover(),Some(request)).await;assert_eq!(status,StatusCode::CONFLICT,"{response}");assert!(!response.to_string().contains("recovery_rejected:"),"{response}");assert_eq!(fixture::snapshot(&c.s,true).await,damaged);c.cleanup().await;
}
