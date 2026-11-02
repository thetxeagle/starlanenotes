//! Private guarded reference exclusion. Historical raw receipts remain provenance.
use super::*;
use crate::storage::{attachment_upload::Receipt,attachment_erasure_plan::Plan};

fn review()->AppError{AppError::Conflict("Retained attachment references require review".into())}
fn source_unreferenced(source:&str,receipt:&Receipt)->Result<(),AppError>{
 crate::storage::attachment_erasure_source::exclude(source,&receipt.intent.asset_id,&receipt.intent.storage_path).map_err(|_|review())
}
pub(crate) struct ReferenceProof{pub(crate) plan:Plan,pub(crate) plan_raw:String,pub(crate) receipt:Receipt,pub(crate) paths:std::collections::BTreeMap<String,(String,i64)>,pub(crate) finalized_journals:std::collections::BTreeSet<String>}
pub(super) async fn no_protected_references(held:&mut FencedTransaction,auth:&crate::auth::middleware::AuthUser,operation:&str,receipt:&Receipt)->Result<ReferenceProof,AppError>{
 let paths=crate::storage::attachment_capacity::retained_paths_in(&mut held.tx).await.map_err(|_|review())?;
 prove_in(&mut held.tx,Some(&auth.claims.sub),operation,receipt,paths,false).await
}
pub(crate) async fn prove_in(conn:&mut sqlx::AnyConnection,actor:Option<&str>,operation:&str,receipt:&Receipt,paths:std::collections::BTreeMap<String,(String,i64)>,strict:bool)->Result<ReferenceProof,AppError>{prove_in_at(&crate::storage::data_dir(),conn,actor,operation,receipt,paths,strict).await}
pub(crate) async fn prove_in_at(root:&std::path::Path,conn:&mut sqlx::AnyConnection,actor:Option<&str>,operation:&str,receipt:&Receipt,paths:std::collections::BTreeMap<String,(String,i64)>,strict:bool)->Result<ReferenceProof,AppError>{
 let plan=crate::storage::attachment_erasure_plan::load_for_physical_review(conn,operation).await?.ok_or_else(review)?;
 if plan.format_version!=2||!plan.original_target.matches_source(&plan.source,&plan.document_id){return Err(review())}
 if actor.is_some_and(|actor|plan.actor_id!=actor)||plan.space_id!=receipt.intent.request.space_id||plan.document_id!=receipt.intent.request.document_id{return Err(review())}
 if strict{crate::storage::attachment_erasure_plan::verify(conn,&plan).await?;}else{crate::storage::attachment_erasure_plan::verify_historical_subset(conn,&plan).await?;}
 if sqlx::query_scalar::<_,i64>("SELECT 1 FROM documents WHERE id=$1").bind(&plan.document_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?.is_some(){return Err(review())}
 let captured=plan.source["domain"]["attachments"].as_array().ok_or_else(review)?;
 let matches:Vec<_>=captured.iter().filter(|row|row["id"]==receipt.intent.asset_id).collect();
 let current_origin=matches.len()==1&&matches[0]["document_id"]==plan.document_id&&matches[0]["space_id"]==plan.space_id&&matches[0]["storage_path"]==receipt.intent.storage_path&&matches[0]["size_bytes"].as_i64()==Some(receipt.intent.byte_length);
 let original_versions:Vec<_>=plan.references.get("attachment_version_records").ok_or_else(review)?.iter().filter(|row|row["version_id"]==receipt.intent.asset_id).collect();
 let retained_origin=original_versions.len()==1&&original_versions[0]["original_document_id"]==plan.document_id&&original_versions[0]["original_space_id"]==plan.space_id&&original_versions[0]["original_storage_path"]==receipt.intent.storage_path&&original_versions[0]["byte_length"].as_i64()==Some(receipt.intent.byte_length)&&original_versions[0]["upload_operation_id"]==receipt.intent.request.operation_id&&original_versions[0]["upload_attempt_id"]==receipt.intent.attempt_id&&original_versions[0]["binary_sha256"]==receipt.intent.binary_sha256&&original_versions[0]["provenance"]=="direct_upload_v1";
 if !current_origin&&!retained_origin{return Err(review())}
 if crate::storage::attachment_upload::receipt(&mut *conn,&receipt.intent.request.operation_id).await.map_err(|_|review())?.as_ref()!=Some(receipt){return Err(review())}
 // Full logical immutable receipt/registry/ledger consistency remains mandatory,
 // even without quota. This observation itself does not reduce accounting charge.

 for (table,column) in [("attachments","id"),("move_asset_path_aliases","asset_id")] {
  let q=format!("SELECT 1 FROM {table} WHERE {column}=$1 LIMIT 1");if sqlx::query_scalar::<_,i64>(&q).bind(&receipt.intent.asset_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?.is_some(){return Err(review())}
 }
 for row in plan.references.get("asset_replay_receipts").ok_or_else(review)? {let raw=row["receipt_json"].as_str().ok_or_else(review)?;let replay:crate::sync::AssetReplayReceipt=serde_json::from_str(raw).map_err(|_|review())?;if replay.incoming.entity_id==receipt.intent.asset_id{return Err(review())}}
 let history:(i64,i64)=sqlx::query_as("SELECT count(*),COALESCE(SUM(length(change_set)),0) FROM doc_versions").fetch_one(&mut *conn).await.map_err(AppError::internal)?;
 // The retained CRDT changes are opaque to this bounded adapter. No substring
 // shortcut can establish they do not reference the selected asset.
 if history.0>512||history.1!=0{return Err(review())}
 // Registered body-bearing consumers also retain references outside the page
 // Markdown: comments, reusable templates, property values and workflow notes.
 let mut total=0usize;
 for table in ["comments","templates","template_drafts","template_revisions","property_values","properties_schema","document_workflow_events","document_workflows","workflow_templates"] {
  let count:i64=sqlx::query_scalar(&format!("SELECT count(*) FROM {table}")).fetch_one(&mut *conn).await.map_err(AppError::internal)?;if count>512{return Err(review())}
  let rows=crate::spaces::document_deletion::rows(&mut *conn,&format!("SELECT * FROM {table} LIMIT 513"),&[]).await.map_err(|_|review())?;
  for row in rows {let raw=serde_json::to_string(&row).map_err(AppError::internal)?;total=total.checked_add(raw.len()).ok_or_else(review)?;if total>8*1024*1024{return Err(review())}source_unreferenced(&raw,receipt)?;}
 }
 let drawings:i64=sqlx::query_scalar("SELECT count(*) FROM excalidraw_files").fetch_one(&mut *conn).await.map_err(AppError::internal)?;
 if drawings!=0{return Err(review())} // Opaque retained drawing payloads are unsupported.
 let(count,size):(i64,i64)=sqlx::query_as("SELECT count(*),COALESCE(SUM(length(content)+COALESCE(length(cover_image),0)),0) FROM document_drafts").fetch_one(&mut *conn).await.map_err(AppError::internal)?;if count>512||size>8*1024*1024{return Err(review())}
 let drafts:Vec<(String,Option<String>)>=sqlx::query_as("SELECT content,cover_image FROM document_drafts LIMIT 513").fetch_all(&mut *conn).await.map_err(AppError::internal)?;if drafts.len()>512{return Err(review())}
 for(content,cover)in drafts{if let Some(cover)=cover{total=total.checked_add(cover.len()).ok_or_else(review)?;if total>8*1024*1024{return Err(review())}source_unreferenced(&cover,receipt)?;}total=total.checked_add(content.len()).ok_or_else(review)?;if total>8*1024*1024{return Err(review())}source_unreferenced(&content,receipt)?;}
 let(count,size):(i64,i64)=sqlx::query_as("SELECT count(*),COALESCE(SUM(COALESCE(length(cover_image),0)),0) FROM documents").fetch_one(&mut *conn).await.map_err(AppError::internal)?;if count>512||size>8*1024*1024{return Err(review())}
 let documents:Vec<(String,String,Option<String>)>=sqlx::query_as("SELECT id,content_path,cover_image FROM documents ORDER BY id LIMIT 513").fetch_all(&mut *conn).await.map_err(AppError::internal)?;if documents.len()>512{return Err(review())}
 for(_,path,cover)in documents {if let Some(cover)=cover{total=total.checked_add(cover.len()).ok_or_else(review)?;if total>8*1024*1024{return Err(review())}source_unreferenced(&cover,receipt)?;}let bytes=crate::storage::journal_fs::source_bytes(root,&path).map_err(|_|review())?;total=total.checked_add(bytes.len()).ok_or_else(review)?;if total>8*1024*1024{return Err(review())}let source=std::str::from_utf8(&bytes).map_err(|_|review())?;source_unreferenced(source,receipt)?;}
 let mut finalized_journals=std::collections::BTreeSet::new();
 // Completed historical receipts are provenance, not current source grants.
 // Skip their staged sources only after exact existing inspection proves that
 // retry cannot introduce a new publication. Pending/uncommitted operations
 // remain fully inspected by the native family census.
 let rows=crate::spaces::document_deletion::rows(&mut *conn,"SELECT * FROM storage_journal_receipts LIMIT 513",&[]).await.map_err(|_|review())?;if rows.len()>512{return Err(review())}for row in rows{
  let op=row["op_id"].as_str().ok_or_else(review)?;
  let inspected=crate::storage::journal::inspect_restoration(root,&mut *conn,op).await.map_err(|_|review())?;
  if inspected.class==crate::storage::journal::StorageOperationClass::CommittedComplete{finalized_journals.insert(format!(".storage-journal/{op}/"));}
 }
 let rows=crate::spaces::document_deletion::rows(&mut *conn,"SELECT * FROM publication_receipts LIMIT 513",&[]).await.map_err(|_|review())?;if rows.len()>512{return Err(review())}for row in rows{
  let op=row["op_id"].as_str().ok_or_else(review)?;
  let inspected=crate::storage::publication::inspect_restoration(root,&mut *conn,op).await.map_err(|_|review())?;
  if matches!(inspected.class,crate::storage::publication::RestorationOperationClass::CommittedHistorical|crate::storage::publication::RestorationOperationClass::CommittedCurrent){finalized_journals.insert(format!(".publication-journal/{op}/"));}
 }
 let plan_raw=serde_json::to_string(&plan).map_err(AppError::internal)?;Ok(ReferenceProof{plan,plan_raw,receipt:receipt.clone(),paths,finalized_journals})
}
