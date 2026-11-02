//! Actual draft birth/resume. SQL-only output; no mutable private mirror writes.
//! A receipt is retry identity, never draft/page authorization or a latest baseline.
use super::{access::{self,PageOperation,DraftOperation,DraftParticipation,Purpose},draft_save,published_source};
use crate::{AppState,auth::{middleware::AuthUser,routes::AppError}};
use serde_json::{json,Value};
use sqlx::AnyConnection;
use uuid::Uuid;

#[derive(Clone,Default,serde::Deserialize,serde::Serialize)]
pub(crate) struct Request {
    pub force_new: Option<bool>,
    pub operation_id: Option<String>,
    pub source_draft_id: Option<String>,
    pub source_digest: Option<Value>,
}
pub(crate) struct Outcome {pub saved:draft_save::Saved,pub created:bool,pub operation_id:Option<String>}
fn conflict()->AppError {AppError::Conflict("source_conflict: Draft birth or access changed; retain the original request for review".into())}
fn required()->AppError {AppError::Conflict("source_required: Private fork requires the selected editable draft and its original source_digest; forced birth requires an operation_id".into())}
async fn rows(c:&mut AnyConnection,q:&str,args:&[&str])->Result<Vec<Value>,AppError> {super::document_deletion::rows(c,q,args).await.map_err(AppError::internal)}
fn request_hash(r:&Request)->Result<String,AppError> {draft_save::digest(&serde_json::to_value(r).map_err(AppError::internal)?)}
fn request_id(r:&Request)->Result<Option<&str>,AppError> {
    match r.operation_id.as_deref() {Some(id) if Uuid::parse_str(id).is_ok()=>Ok(Some(id)),None if !r.force_new.unwrap_or(false)=>Ok(None),_=>Err(required())}
}
fn expected_receipt(actor:&str,id:&str,space:&str,doc:&str,hash:&str,row:&Value)->Result<Value,AppError> {
    Ok(json!({"actor_id":actor,"operation_id":id,"space_id":space,"document_id":doc,"request_fingerprint":hash,"draft_id":draft_save::string(row,"id")?,"result_digest":draft_save::digest(row)?,"result_json":serde_json::to_string(row).map_err(AppError::internal)?}))
}
async fn saved(c:&mut AnyConnection,row:Value,page:&Value)->Result<draft_save::Saved,AppError> {
    let owner=draft_save::string(&row,"owner_id")?;
    let names=rows(c,"SELECT id,display_name FROM users WHERE id=$1",&[&owner]).await?;
    if names.len()!=1 {return Err(conflict());}
    Ok(draft_save::Saved{document_title:if page["published_at"].is_null(){draft_save::string(&row,"title")?}else{draft_save::string(page,"title")?},owner_name:draft_save::string(&names[0],"display_name")?,row,participation:DraftParticipation::Owner})
}
/// Historical ACK still requires current original authorization and exact original
/// result. A later save/deletion/recreated ID never becomes this receipt's result.
async fn historical(c:&mut AnyConnection,auth:&AuthUser,space:&str,doc:&str,r:&Request)->Result<Option<Outcome>,AppError> {
    let Some(id)=request_id(r)? else{return Ok(None)};
    access::require_current_human_preference_write(c,auth).await?;
    let receipt=rows(c,"SELECT * FROM draft_birth_receipts WHERE actor_id=$1 AND operation_id=$2",&[&auth.claims.sub,id]).await?;
    if receipt.is_empty(){return Ok(None)}
    if receipt.len()!=1 {return Err(conflict());}
    let raw=&receipt[0];let row:Value=serde_json::from_str(&draft_save::string(raw,"result_json")?).map_err(|_|conflict())?;
    let hash=request_hash(r)?;
    if *raw!=expected_receipt(&auth.claims.sub,id,space,doc,&hash,&row)? || row["owner_id"]!=auth.claims.sub || row["document_id"]!=doc || row["space_id"]!=space {return Err(conflict());}
    let draft=draft_save::string(&row,"id")?;
    let principal=access::principal_from_auth(auth)?;
    let a=access::resolve_draft_access(c,principal,space,doc,&draft,DraftOperation::Edit,Purpose::Routine).await?;
    if a.participation!=DraftParticipation::Owner{return Err(conflict());}
    let mut proof=published_source::capture_source(c,auth,space,doc,&a.page.ancestor_ids).await?;
    proof.push(published_source::SourceProof{query:"SELECT id,display_name FROM users WHERE id=$1".into(),parameters:vec![auth.claims.sub.clone()],rows:rows(c,"SELECT id,display_name FROM users WHERE id=$1",&[&auth.claims.sub]).await?});
    let page=proof[0].rows.iter().find(|p|p["id"]==doc).ok_or_else(conflict)?;
    let result=saved(c,row.clone(),page).await?;
    published_source::verify_source(c,&proof).await?;
    access::resolve_draft_access(c,principal,space,doc,&draft,DraftOperation::Edit,Purpose::Routine).await?;
    if draft_save::selected(c,space,doc,&draft).await?!=row || rows(c,"SELECT * FROM draft_birth_receipts WHERE actor_id=$1 AND operation_id=$2",&[&auth.claims.sub,id]).await?!=receipt {return Err(conflict());}
    access::require_current_human_preference_write(c,auth).await?;
    Ok(Some(Outcome{saved:result,created:true,operation_id:Some(id.to_owned())}))
}
pub(crate) async fn create(state:&AppState,auth:&AuthUser,space:&str,doc:&str,r:Request)->Result<Outcome,AppError> {
    let operation=request_id(&r)?.map(str::to_owned);
    let mut tx=state.db.begin().await.map_err(AppError::internal)?;
    #[cfg(test)] before_lock().await;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx,&[space]).await?;
    if let Some(result)=historical(&mut tx,auth,space,doc,&r).await? {tx.rollback().await.map_err(AppError::internal)?;return Ok(result);}
    let n=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(doc).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    if n.rows_affected()!=1{return Err(AppError::NotFound("Document unavailable".into()));}
    sqlx::query("UPDATE document_drafts SET id=id WHERE document_id=$1 AND space_id=$2").bind(doc).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    sqlx::query("UPDATE document_draft_access SET draft_id=draft_id WHERE draft_id IN(SELECT id FROM document_drafts WHERE document_id=$1 AND space_id=$2)").bind(doc).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    let principal=access::principal_from_auth(auth)?;
    access::require_current_human_preference_write(&mut tx,auth).await?;
    // This proof precedes ownership insertion: birth cannot manufacture its own
    // original private participation to bypass PageEdit.
    let authority=access::resolve_page_access(&mut tx,principal,space,doc,PageOperation::Edit,Purpose::Routine).await?;
    let mut proof=published_source::capture_source(&mut tx,auth,space,doc,&authority.ancestor_ids).await?;
    let page=proof[0].rows.iter().find(|p|p["id"]==doc).ok_or_else(conflict)?.clone();
    let owner_names=rows(&mut tx,"SELECT id,display_name FROM users WHERE id=$1",&[&auth.claims.sub]).await?;
    proof.push(published_source::SourceProof{query:"SELECT id,display_name FROM users WHERE id=$1".into(),parameters:vec![auth.claims.sub.clone()],rows:owner_names});
    if !r.force_new.unwrap_or(false) {
        let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM document_drafts WHERE document_id=$1 AND space_id=$2 AND owner_id=$3 ORDER BY updated_at DESC,id ASC LIMIT 1").bind(doc).bind(space).bind(&auth.claims.sub).fetch_all(&mut *tx).await.map_err(AppError::internal)?;
        if let Some(id)=ids.first() {
            let a=access::resolve_draft_access(&mut tx,principal,space,doc,id,DraftOperation::Edit,Purpose::Routine).await?;
            if a.participation!=DraftParticipation::Owner{return Err(conflict());}
            let row=draft_save::selected(&mut tx,space,doc,id).await?;let result=saved(&mut tx,row.clone(),&page).await?;
            published_source::verify_source(&mut tx,&proof).await?;
            access::resolve_draft_access(&mut tx,principal,space,doc,id,DraftOperation::Edit,Purpose::Routine).await?;
            if draft_save::selected(&mut tx,space,doc,id).await?!=row{return Err(conflict());}
            access::require_current_human_preference_write(&mut tx,auth).await?;
            tx.rollback().await.map_err(AppError::internal)?;
            return Ok(Outcome{saved:result,created:false,operation_id:None});
        }
    }
    let mut private_source=None;
    let mut published_bytes=None;
    let (title,icon,cover,content,base,folder,parent)=if page["published_at"].is_null() {
        let source_id=r.source_draft_id.as_deref().filter(|s|!s.is_empty()).ok_or_else(required)?;
        let source=draft_save::required(r.source_digest.as_ref())?;
        let a=access::resolve_draft_access(&mut tx,principal,space,doc,source_id,DraftOperation::Edit,Purpose::Routine).await?;
        if !matches!(a.participation,DraftParticipation::Owner|DraftParticipation::SharedEditor){return Err(conflict());}
        let row=draft_save::selected(&mut tx,space,doc,source_id).await?;
        if draft_save::digest(&row)?!=source{return Err(conflict());}
        private_source=Some((source_id.to_owned(),row.clone(),a.participation));
        (draft_save::string(&row,"title")?,draft_save::optional(&row,"icon")?,draft_save::optional(&row,"cover_image")?,draft_save::string(&row,"content")?,draft_save::string(&row,"base_updated_at")?,draft_save::optional(&row,"previous_folder_id")?,draft_save::optional(&row,"previous_parent_id")?)
    }else {
        if r.source_draft_id.is_some()||r.source_digest.is_some(){return Err(conflict());}
        let path=draft_save::string(&page,"content_path")?;
        let slug:String=sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1").bind(space).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
        if (path.is_empty() || path.contains(char::from(92)) || !std::path::Path::new(&path).components().all(|c|matches!(c,std::path::Component::Normal(_))))||!path.starts_with(&format!("spaces/{slug}/docs/")){return Err(conflict());}
        let body=crate::storage::read_document(&path).await.map_err(AppError::internal)?;
        published_bytes=Some((path,body.clone()));
        (draft_save::string(&page,"title")?,draft_save::optional(&page,"icon")?,draft_save::optional(&page,"cover_image")?,body,draft_save::string(&page,"updated_at")?,draft_save::optional(&page,"folder_id")?,draft_save::optional(&page,"parent_id")?)
    };
    if title.trim().is_empty()||title.len()>500||content.len()>5_242_880 {return Err(AppError::BadRequest("Draft source exceeds supported title/content bounds".into()));}
    let id=Uuid::new_v4().to_string();let stamp=chrono::Utc::now().to_rfc3339();
    let raw=sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,icon,cover_image,content,base_updated_at,created_at,updated_at,previous_folder_id,previous_parent_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10,$11,$12) RETURNING *").bind(&id).bind(doc).bind(space).bind(&auth.claims.sub).bind(&title).bind(&icon).bind(&cover).bind(&content).bind(&base).bind(&stamp).bind(&folder).bind(&parent).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    let result_row=draft_save::joined_source(&raw)?;
    let expected=json!({"id":id,"document_id":doc,"space_id":space,"owner_id":auth.claims.sub,"title":title,"icon":icon,"cover_image":cover,"content":content,"base_updated_at":base,"created_at":stamp,"updated_at":stamp,"previous_folder_id":folder,"previous_parent_id":parent});
    if result_row!=expected{return Err(conflict());}
    let rows_proof=proof.iter_mut().find(|p|p.query=="SELECT * FROM document_drafts WHERE document_id=$1"&&p.parameters==vec![doc.to_owned()]).ok_or_else(conflict)?;
    rows_proof.rows.push(result_row.clone());rows_proof.rows.sort_by_cached_key(Value::to_string);
    let audit_id=Uuid::new_v4().to_string();
    let audit_stamp:String=sqlx::query_scalar("INSERT INTO audit_log(id,user_id,actor_kind,actor_id,action,target_type,target_id) VALUES($1,$2,'user',$2,'document.draft_create','document_draft',$3) RETURNING created_at").bind(&audit_id).bind(&auth.claims.sub).bind(&id).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    let audit=json!({"id":audit_id,"user_id":auth.claims.sub,"ip_address":null,"action":"document.draft_create","target_type":"document_draft","target_id":id,"created_at":audit_stamp,"actor_kind":"user","actor_id":auth.claims.sub,"before_value":null,"after_value":null});
    let asset_witness_operation=if operation.is_some(){operation.clone()}else if super::asset_audiences::needs_copy_witness(&mut tx,doc,&result_row).await?{Some(id.clone())}else{None};
    let receipt=if let Some(operation)=asset_witness_operation.as_deref() {
        let hash=request_hash(&r)?;let receipt=expected_receipt(&auth.claims.sub,operation,space,doc,&hash,&result_row)?;
        sqlx::query("INSERT INTO draft_birth_receipts(actor_id,operation_id,space_id,document_id,request_fingerprint,draft_id,result_digest,result_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(&auth.claims.sub).bind(operation).bind(space).bind(doc).bind(hash).bind(&id).bind(draft_save::digest(&result_row)?).bind(serde_json::to_string(&result_row).map_err(AppError::internal)?).execute(&mut *tx).await.map_err(AppError::internal)?;
        Some(receipt)
    }else{None};
    super::asset_audiences::inherit_draft(&mut tx,auth,space,doc,&result_row,operation.as_deref().unwrap_or(&id),private_source.as_ref().map(|s|s.0.as_str()),None).await?;
    let asset_proofs=super::asset_audiences::audience_proofs(&mut tx,doc).await?;
    let result=saved(&mut tx,result_row.clone(),&page).await?;
    #[cfg(test)] after_writes().await;
    published_source::verify_source(&mut tx,&asset_proofs).await?;
    if rows(&mut tx,"SELECT * FROM audit_log WHERE id=$1",&[&audit_id]).await?!=vec![audit.clone()]{return Err(conflict());}
    if let (Some(operation),Some(expected))=(asset_witness_operation.as_deref(),receipt) {if rows(&mut tx,"SELECT * FROM draft_birth_receipts WHERE actor_id=$1 AND operation_id=$2",&[&auth.claims.sub,operation]).await?!=vec![expected]{return Err(conflict());}}
    published_source::verify_source(&mut tx,&proof).await?;
    access::resolve_page_access(&mut tx,principal,space,doc,PageOperation::Edit,Purpose::Routine).await?;
    if let Some((source,row,participation))=private_source {
        let final_source=access::resolve_draft_access(&mut tx,principal,space,doc,&source,DraftOperation::Edit,Purpose::Routine).await?;
        if final_source.participation!=participation||draft_save::selected(&mut tx,space,doc,&source).await?!=row{return Err(conflict());}
    }
    let a=access::resolve_draft_access(&mut tx,principal,space,doc,&id,DraftOperation::Edit,Purpose::Routine).await?;
    if a.participation!=DraftParticipation::Owner||draft_save::selected(&mut tx,space,doc,&id).await?!=result_row{return Err(conflict());}
    #[cfg(test)] before_identity().await;
    access::require_current_human_preference_write(&mut tx,auth).await?;
    // Synchronous original-path point proof after LAST SQL/credential await.
    // Existing published-writer adoption/full storage custody remains distinct.
    if let Some((path,body))=published_bytes {if std::fs::read_to_string(crate::storage::data_dir().join(path)).map_err(AppError::internal)?!=body{return Err(conflict());}}
    tx.commit().await.map_err(|_|AppError::Conflict("Draft birth outcome is unknown; retain this original operation_id and source for exact retry".into()))?;
    crate::audit::append_committed_move_action(&audit).await;
    #[cfg(test)] if unknown_ack(){return Err(AppError::Conflict("Draft birth outcome is unknown; retain this original operation_id and source for exact retry".into()));}
    Ok(Outcome{saved:result,created:true,operation_id:operation})
}
#[cfg(test)] tokio::task_local! {
    pub(crate) static BEFORE_LOCK:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    pub(crate) static AFTER_WRITES:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    pub(crate) static BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    pub(crate) static UNKNOWN_ACK:bool;
}
#[cfg(test)] async fn before_lock(){if let Ok(Some(h))=BEFORE_LOCK.try_with(|s|s.borrow_mut().take()){h.await}}
#[cfg(test)] async fn after_writes(){if let Ok(Some(h))=AFTER_WRITES.try_with(|s|s.borrow_mut().take()){h.await}}
#[cfg(test)] async fn before_identity(){if let Ok(Some(h))=BEFORE_IDENTITY.try_with(|s|s.borrow_mut().take()){h.await}}
#[cfg(test)] fn unknown_ack()->bool {UNKNOWN_ACK.try_with(|b|*b).unwrap_or(false)}
