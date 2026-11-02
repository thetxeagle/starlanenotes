//! Reviewed immutable history creates only a new private draft. Original operation
//! receipts acknowledge exact results; they never rebase or publish a page.
use crate::{AppState,auth::{middleware::AuthUser,routes::AppError}};
use super::{access,draft_save,draft_publication as publication,publication_snapshots as core,published_source};
use axum::{extract::{Path,State},routing::{get,post},http::{header,StatusCode},Json,Router};
use serde::{Serialize,Deserialize};
use serde_json::{json,Value};
use sqlx::AnyConnection;

#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {pub schema_version:u32,pub operation_id:String,pub snapshot_source_digest:String,pub page_source_digest:String}
pub fn router()->Router<AppState>{Router::new()
 .route("/spaces/{space_id}/documents/{doc_id}/publication-snapshots/{snapshot_id}/recovery-source",get(source))
 .route("/spaces/{space_id}/documents/{doc_id}/publication-snapshots/{snapshot_id}/recover",post(recover))}
fn conflict()->AppError{AppError::Conflict("source_conflict: Snapshot recovery source or retained receipt changed; retain the original operation for review".into())}
async fn rows(c:&mut AnyConnection,q:&str,args:&[&str])->Result<Vec<Value>,AppError>{super::document_deletion::rows(c,q,args).await.map_err(AppError::internal)}
async fn authority(c:&mut AnyConnection,a:&AuthUser,s:&str,d:&str)->Result<access::PageAccess,AppError>{
 access::require_current_human_preference_write(c,a).await?;
 access::resolve_page_access(c,access::principal_from_auth(a)?,s,d,access::PageOperation::Edit,access::Purpose::Routine).await
}
fn snapshot_proof(r:&core::Stored)->Value{json!({"domain":"starlane-snapshot-recovery-source-v1","id":r.id,"payload":r.payload,"kind":r.kind,"operation":r.operation,"published_at":r.published_at,"captured_at":r.captured_at,"captured_by":r.captured_by,"raw_sha256":r.payload.content.raw_sha256,"raw_byte_length":r.raw.len()})}
fn reason(r:&core::Stored)->Option<&'static str>{let c=&r.payload.content;
 if c.portable_markdown.is_none(){Some("raw_only")}else if c.title.trim().is_empty()||c.title.len()>500{Some("title_bounds")}else if c.portable_markdown.as_ref().unwrap().len()>5_242_880{Some("content_bounds")}else{None}}
fn projection(r:&core::Stored)->Value{let c=&r.payload.content;json!({"title":c.title,"content":c.portable_markdown,"icon":c.icon,"cover_image":c.cover_reference})}
async fn capture(c:&mut AnyConnection,a:&AuthUser,s:&str,d:&str)->Result<Vec<published_source::SourceProof>,AppError>{
 let access=authority(c,a,s,d).await?;let mut p=published_source::capture_source(c,a,s,d,&access.ancestor_ids).await?;
 for (q,arg) in [("SELECT * FROM properties_schema WHERE space_id=$1",s),("SELECT * FROM document_publication_snapshots WHERE document_id=$1",d),("SELECT * FROM draft_publication_receipts WHERE document_id=$1",d),("SELECT id,display_name FROM users WHERE id=$1",&a.claims.sub),("SELECT * FROM snapshot_recovery_receipts WHERE document_id=$1",d)]{
 p.push(published_source::SourceProof{query:q.into(),parameters:vec![arg.into()],rows:rows(c,q,&[arg]).await?});}Ok(p)
}
fn physical(p:&[published_source::SourceProof])->Result<(),AppError>{
 for proof in p.iter().filter(|p|p.query=="SELECT * FROM draft_publication_receipts WHERE document_id=$1") {for r in &proof.rows{
 let m:crate::storage::publication::PreparedPublication=serde_json::from_str(&draft_save::string(r,"storage_manifest_json")?).map_err(|_|conflict())?;
 crate::storage::publication::verify_snapshot_manifest(&m)?;}}Ok(())
}
fn page(p:&[published_source::SourceProof],d:&str)->Result<Value,AppError>{p[0].rows.iter().find(|r|r["id"]==d).cloned().ok_or_else(conflict)}
async fn selected(c:&mut AnyConnection,d:&str,id:&str)->Result<core::Stored,AppError>{core::read_validated(c,d).await?.into_iter().find(|r|r.id==id).ok_or_else(||AppError::NotFound("Publication snapshot unavailable".into()))}
async fn final_check(c:&mut AnyConnection,a:&AuthUser,s:&str,d:&str,p:&[published_source::SourceProof])->Result<(),AppError>{
 published_source::verify_source(c,p).await?;authority(c,a,s,d).await?;
 access::require_current_human_preference_write(c,a).await?;physical(p)
}
async fn source(State(s):State<AppState>,a:AuthUser,Path((space,doc,id)):Path<(String,String,String)>)->Result<([(header::HeaderName,&'static str);1],Json<Value>),AppError>{
 let mut c=s.db.acquire().await.map_err(AppError::internal)?;let p=capture(&mut c,&a,&space,&doc).await?;
 let page=page(&p,&doc)?;let bytes=publication::page_bytes(&page).await?;let r=selected(&mut c,&doc,&id).await?;
 let why=reason(&r);let response=json!({"schema_version":1,"space_id":space,"document_id":doc,"snapshot_id":id,"snapshot_digest":core::digest(&json!(r.payload))?,"snapshot_source_digest":draft_save::digest(&snapshot_proof(&r))?,"page_source_digest":publication::page_digest(&page,bytes.as_deref())?,"page_published":!page["published_at"].is_null(),"page_updated_at":page["updated_at"],"recoverable":why.is_none(),"reason":why,"extraction":r.payload.content.extraction,"draft":if why.is_none(){projection(&r)}else{Value::Null}});
 final_check(&mut c,&a,&space,&doc,&p).await?;publication::verify_page_bytes(&page,&bytes)?;
 Ok(([(header::CACHE_CONTROL,"no-store")],Json(response)))
}
fn validate(r:&Request)->Result<(),AppError>{
 let valid=uuid::Uuid::parse_str(&r.operation_id).map(|u|u.to_string()==r.operation_id).unwrap_or(false);
 if r.schema_version!=1||!valid{return Err(AppError::Conflict("source_required: Recovery requires schema_version 1 and the original canonical operation UUID".into()))}
 for d in [&r.snapshot_source_digest,&r.page_source_digest]{draft_save::required(Some(&json!(d)))?;}Ok(())
}
fn receipt(actor:&str,space:&str,doc:&str,id:&str,r:&Request,e:&Value)->Result<Value,AppError>{Ok(json!({"operation_id":r.operation_id,"actor_id":actor,"space_id":space,"document_id":doc,"snapshot_id":id,"request_json":serde_json::to_string(r).map_err(AppError::internal)?,"evidence_json":serde_json::to_string(e).map_err(AppError::internal)?,"evidence_digest":draft_save::digest(e)?}))}
async fn response(a:&AuthUser,s:&str,d:&str,id:&str,r:&Request,row:Value,page:&Value,name:&str)->Result<Value,AppError>{
 let result=super::drafts::response_from_saved(draft_save::Saved{row:row.clone(),owner_name:name.into(),document_title:if page["published_at"].is_null(){draft_save::string(&row,"title")?}else{draft_save::string(page,"title")?},participation:access::DraftParticipation::Owner},&a.claims.sub)?;
 Ok(json!({"schema_version":1,"operation_id":r.operation_id,"space_id":s,"document_id":d,"snapshot_id":id,"snapshot_source_digest":r.snapshot_source_digest,"page_source_digest":r.page_source_digest,"draft":result,"disposition":"applied"}))
}
async fn recover(State(s):State<AppState>,a:AuthUser,Path((space,doc,id)):Path<(String,String,String)>,Json(r):Json<Request>)->Result<(StatusCode,[(header::HeaderName,&'static str);1],Json<Value>),AppError>{
 validate(&r)?;let mut tx=s.db.begin().await.map_err(AppError::internal)?;
 #[cfg(test)] before_lock().await;
 super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;super::mutation_guards::lock_spaces_for_mutation(&mut tx,&[&space]).await?;
 let mut p=capture(&mut tx,&a,&space,&doc).await?;
 let n=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&doc).bind(&space).execute(&mut *tx).await.map_err(AppError::internal)?;
 if n.rows_affected()!=1{return Err(AppError::NotFound("Document unavailable".into()))}
 published_source::verify_source(&mut tx,&p).await?;let page=page(&p,&doc)?;
 let snapshot=selected(&mut tx,&doc,&id).await?;let snap=snapshot_proof(&snapshot);
 let records=rows(&mut tx,"SELECT * FROM snapshot_recovery_receipts WHERE operation_id=$1",&[&r.operation_id]).await?;
 if !records.is_empty(){
  if records.len()!=1{return Err(conflict())}let e:Value=serde_json::from_str(&draft_save::string(&records[0],"evidence_json")?).map_err(|_|conflict())?;
  if records[0]!=receipt(&a.claims.sub,&space,&doc,&id,&r,&e)?||e["snapshot"]!=snap||draft_save::digest(&snap)?!=r.snapshot_source_digest{return Err(conflict())}
  let row=e["result"].clone();let draft=draft_save::string(&row,"id")?;
  if row["owner_id"]!=a.claims.sub||row["space_id"]!=space||row["document_id"]!=doc||draft_save::selected(&mut tx,&space,&doc,&draft).await?!=row{return Err(conflict())}
  let old_page=&e["page"];let bytes:Option<Vec<u8>>=serde_json::from_value(e["page_bytes"].clone()).map_err(|_|conflict())?;
  if publication::page_digest(old_page,bytes.as_deref())?!=r.page_source_digest{return Err(conflict())}
  let expected=expected_draft(&a,&space,&doc,&draft,&snapshot,old_page,&draft_save::string(&row,"created_at")?)?;
  if expected!=row{return Err(conflict())}
  let out=response(&a,&space,&doc,&id,&r,row,old_page,e["owner_name"].as_str().ok_or_else(conflict)?).await?;
  if e["response"]!=out{return Err(conflict())}
  let audit=&e["audit"];let audit_id=draft_save::string(audit,"id")?;
  if audit_id!=r.operation_id||audit["user_id"]!=a.claims.sub||audit["actor_id"]!=a.claims.sub||audit["actor_kind"]!="user"||audit["action"]!="document.snapshot_recover"||audit["target_type"]!="document_draft"||audit["target_id"]!=draft||rows(&mut tx,"SELECT * FROM audit_log WHERE id=$1",&[&audit_id]).await?!=vec![audit.clone()]{return Err(conflict())}
  if rows(&mut tx,"SELECT * FROM snapshot_recovery_receipts WHERE operation_id=$1",&[&r.operation_id]).await?!=records{return Err(conflict())}
  final_check(&mut tx,&a,&space,&doc,&p).await?;
  tx.rollback().await.map_err(AppError::internal)?;let mut out=out;out["disposition"]=json!("historical");return Ok((StatusCode::OK,[(header::CACHE_CONTROL,"no-store")],Json(out)))
 }
 // A retained operation audit proves a missing receipt is damaged evidence,
 // never permission to create a second copy (even if its draft was deleted).
 if !rows(&mut tx,"SELECT * FROM audit_log WHERE id=$1",&[&r.operation_id]).await?.is_empty(){return Err(conflict())}
 if let Some(why)=reason(&snapshot){return Err(AppError::BadRequest(format!("Snapshot cannot become an editor draft: {why}")))}
 let bytes=publication::page_bytes(&page).await?;
 if draft_save::digest(&snap)?!=r.snapshot_source_digest||publication::page_digest(&page,bytes.as_deref())?!=r.page_source_digest{return Err(AppError::Conflict("recovery_rejected: Reviewed snapshot or current page changed; review again before creating a new private draft".into()))}
 let draft=uuid::Uuid::new_v4().to_string();let stamp=chrono::Utc::now().to_rfc3339();let expected=expected_draft(&a,&space,&doc,&draft,&snapshot,&page,&stamp)?;
 let raw=sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,icon,cover_image,content,base_updated_at,created_at,updated_at,previous_folder_id,previous_parent_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10,$11,$12) RETURNING *")
 .bind(&draft).bind(&doc).bind(&space).bind(&a.claims.sub).bind(draft_save::string(&expected,"title")?).bind(draft_save::optional(&expected,"icon")?).bind(draft_save::optional(&expected,"cover_image")?).bind(draft_save::string(&expected,"content")?).bind(draft_save::string(&expected,"base_updated_at")?).bind(&stamp).bind(draft_save::optional(&expected,"previous_folder_id")?).bind(draft_save::optional(&expected,"previous_parent_id")?).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
 let result=draft_save::joined_source(&raw)?;if result!=expected{return Err(conflict())}
 let dp=p.iter_mut().find(|p|p.query=="SELECT * FROM document_drafts WHERE document_id=$1"&&p.parameters==vec![doc.clone()]).ok_or_else(conflict)?;dp.rows.push(result.clone());dp.rows.sort_by_cached_key(Value::to_string);
 let names=rows(&mut tx,"SELECT id,display_name FROM users WHERE id=$1",&[&a.claims.sub]).await?;if names.len()!=1{return Err(conflict())}let name=draft_save::string(&names[0],"display_name")?;
 let out=response(&a,&space,&doc,&id,&r,result.clone(),&page,&name).await?;
 let audit_id=r.operation_id.clone();let audit_stamp:String=sqlx::query_scalar("INSERT INTO audit_log(id,user_id,actor_kind,actor_id,action,target_type,target_id) VALUES($1,$2,'user',$2,'document.snapshot_recover','document_draft',$3) RETURNING created_at").bind(&audit_id).bind(&a.claims.sub).bind(&draft).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
 let audit=json!({"id":audit_id,"user_id":a.claims.sub,"ip_address":null,"action":"document.snapshot_recover","target_type":"document_draft","target_id":draft,"created_at":audit_stamp,"actor_kind":"user","actor_id":a.claims.sub,"before_value":null,"after_value":null});
 let e=json!({"snapshot":snap,"page":page,"page_bytes":bytes,"result":result,"owner_name":name,"response":out,"audit":audit});let rec=receipt(&a.claims.sub,&space,&doc,&id,&r,&e)?;
 sqlx::query("INSERT INTO snapshot_recovery_receipts(operation_id,actor_id,space_id,document_id,snapshot_id,request_json,evidence_json,evidence_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(&r.operation_id).bind(&a.claims.sub).bind(&space).bind(&doc).bind(&id).bind(draft_save::string(&rec,"request_json")?).bind(draft_save::string(&rec,"evidence_json")?).bind(draft_save::string(&rec,"evidence_digest")?).execute(&mut *tx).await.map_err(AppError::internal)?;
 super::asset_audiences::inherit_draft(&mut tx,&a,&space,&doc,&result,&r.operation_id,None,Some(&id)).await?;
 let asset_proofs=super::asset_audiences::audience_proofs(&mut tx,&doc).await?;
 let rp=p.iter_mut().find(|p|p.query=="SELECT * FROM snapshot_recovery_receipts WHERE document_id=$1").ok_or_else(conflict)?;rp.rows.push(rec.clone());rp.rows.sort_by_cached_key(Value::to_string);
 #[cfg(test)] after_writes().await;
 published_source::verify_source(&mut tx,&asset_proofs).await?;
 if rows(&mut tx,"SELECT * FROM audit_log WHERE id=$1",&[&audit_id]).await?!=vec![audit.clone()]||rows(&mut tx,"SELECT * FROM snapshot_recovery_receipts WHERE operation_id=$1",&[&r.operation_id]).await?!=vec![rec]||draft_save::selected(&mut tx,&space,&doc,&draft).await?!=result{return Err(conflict())}
 final_check(&mut tx,&a,&space,&doc,&p).await?;publication::verify_page_bytes(&page,&bytes)?;
 #[cfg(test)] before_commit();
 tx.commit().await.map_err(|_|AppError::Conflict("Snapshot recovery outcome is unknown; retain the exact original operation for retry".into()))?;
 #[cfg(test)] after_commit();
 crate::audit::append_committed_move_action(&audit).await;
 #[cfg(test)] if UNKNOWN_ACK.try_with(|v|*v).unwrap_or(false){return Err(AppError::Conflict("Snapshot recovery outcome is unknown; retain the exact original operation for retry".into()))}
 Ok((StatusCode::CREATED,[(header::CACHE_CONTROL,"no-store")],Json(out)))
}
fn expected_draft(a:&AuthUser,s:&str,d:&str,id:&str,r:&core::Stored,page:&Value,stamp:&str)->Result<Value,AppError>{let c=&r.payload.content;Ok(json!({"id":id,"document_id":d,"space_id":s,"owner_id":a.claims.sub,"title":c.title,"icon":c.icon,"cover_image":c.cover_reference,"content":c.portable_markdown.as_ref().ok_or_else(conflict)?,"base_updated_at":draft_save::string(page,"updated_at")?,"created_at":stamp,"updated_at":stamp,"previous_folder_id":draft_save::optional(page,"folder_id")?,"previous_parent_id":draft_save::optional(page,"parent_id")?}))}
#[cfg(test)] tokio::task_local!{
 pub(crate) static BEFORE_LOCK:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
 pub(crate) static AFTER_WRITES:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
 pub(crate) static BEFORE_COMMIT:std::cell::RefCell<Option<Box<dyn FnOnce()+Send>>>;
 pub(crate) static AFTER_COMMIT:std::cell::RefCell<Option<Box<dyn FnOnce()+Send>>>;
 pub(crate) static UNKNOWN_ACK:bool;
}
#[cfg(test)] async fn before_lock(){if let Ok(Some(h))=BEFORE_LOCK.try_with(|s|s.borrow_mut().take()){h.await}}
#[cfg(test)] async fn after_writes(){if let Ok(Some(h))=AFTER_WRITES.try_with(|s|s.borrow_mut().take()){h.await}}
#[cfg(test)] fn before_commit(){if let Ok(Some(h))=BEFORE_COMMIT.try_with(|s|s.borrow_mut().take()){h()}}
#[cfg(test)] fn after_commit(){if let Ok(Some(h))=AFTER_COMMIT.try_with(|s|s.borrow_mut().take()){h()}}

#[cfg(test)] #[path="publication_snapshot_recovery_tests.rs"] mod tests;
