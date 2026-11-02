//! Bounded explicit cleanup consumer. Metadata DELETE is a separate operation.
use super::*;
use crate::storage::{attachment_erasure_plan::{Plan,OriginalDeleteTarget},attachment_upload::Receipt};
use serde::Serialize;
use std::collections::BTreeMap;
#[derive(Serialize)]
pub(crate) struct CleanupOutcome{operation_id:String,metadata_deleted:bool,outcome:&'static str,reason:Option<&'static str>,assets:Vec<AssetOutcome>}
#[derive(Serialize)]
struct AssetOutcome{asset_id:String,outcome:&'static str,reason:Option<&'static str>}
fn retained(id:String,reason:&'static str)->AssetOutcome{AssetOutcome{asset_id:id,outcome:"retained_review",reason:Some(reason)}}
async fn original_plan(held:&mut FencedTransaction,auth:&crate::auth::middleware::AuthUser,space:&str,document:&str,operation:&str,target:&OriginalDeleteTarget)->Result<Plan,AppError>{
 let parsed=uuid::Uuid::parse_str(operation).map_err(|_|crate::storage::attachment_erasure_plan::target_review())?;
 if parsed.to_string()!=operation{return Err(crate::storage::attachment_erasure_plan::target_review())}
 let plan=crate::storage::attachment_erasure_plan::load_for_physical_review(&mut held.tx,operation).await?.ok_or_else(||AppError::NotFound("Original deletion unavailable".into()))?;
 if plan.space_id!=space||plan.document_id!=document||plan.original_target!=*target{return Err(crate::storage::attachment_erasure_plan::target_review())}
 held.original_human(auth,&plan.actor_id,space).await?.check_now()?;
 if sqlx::query_scalar::<_,i64>("SELECT 1 FROM documents WHERE id=$1").bind(document).fetch_optional(&mut *held.tx).await.map_err(AppError::internal)?.is_some(){return Err(crate::storage::attachment_erasure_plan::target_review())}
 crate::storage::attachment_erasure_plan::verify_historical_subset(&mut held.tx,&plan).await?;Ok(plan)
}
fn original_assets(plan:&Plan)->Result<BTreeMap<String,Option<Receipt>>,AppError>{
 let mut assets=BTreeMap::new();
 for row in plan.source["domain"]["attachments"].as_array().ok_or_else(crate::storage::attachment_erasure_plan::target_review)?{let id=row["id"].as_str().ok_or_else(crate::storage::attachment_erasure_plan::target_review)?;assets.insert(id.to_owned(),None);}
 for row in plan.references.get("attachment_version_records").ok_or_else(crate::storage::attachment_erasure_plan::target_review)?{
  if row["original_document_id"]==plan.document_id&&row["original_space_id"]==plan.space_id{let id=row["version_id"].as_str().ok_or_else(crate::storage::attachment_erasure_plan::target_review)?;assets.entry(id.into()).or_insert(None);}
 }
 if assets.len()>32{return Err(AppError::Conflict("Cleanup asset count requires retained review".into()))}
 for row in plan.references.get("attachment_upload_receipts").ok_or_else(crate::storage::attachment_erasure_plan::target_review)?{
  let raw=row["receipt_json"].as_str().ok_or_else(crate::storage::attachment_erasure_plan::target_review)?;
  if let Ok(receipt)=serde_json::from_str::<Receipt>(raw){if let Some(original)=assets.get_mut(&receipt.intent.asset_id){if original.is_some()||receipt.intent.request.document_id!=plan.document_id||receipt.intent.request.space_id!=plan.space_id{return Err(crate::storage::attachment_erasure_plan::target_review())}*original=Some(receipt);}}
 }Ok(assets)
}
fn authority_error(error:&AppError)->bool{matches!(error,AppError::Unauthorized(_)|AppError::Forbidden(_)|AppError::NotFound(_))}
pub(crate) async fn cleanup(pool:&sqlx::AnyPool,auth:&crate::auth::middleware::AuthUser,space:&str,document:&str,operation:&str,target:&OriginalDeleteTarget)->Result<CleanupOutcome,AppError>{
 // Validate the original scope before reporting any retained asset information.
 let mut held=FencedTransaction::begin(pool).await?;let plan=original_plan(&mut held,auth,space,document,operation,target).await?;let originals=original_assets(&plan)?;held.tx.rollback().await.map_err(AppError::internal)?;
 let mut outcomes=vec![];let mut budget=8usize;
 for(id,receipt)in originals{let Some(receipt)=receipt else{outcomes.push(retained(id,"original_native_custody_unavailable"));continue};let mut outcome=None;
  while budget>0{let mut held=FencedTransaction::begin(pool).await?;original_plan(&mut held,auth,space,document,operation,target).await?;
   let one=match held.prepare_one(auth,operation,&receipt).await{Ok(one)=>one,Err(error)=>{if authority_error(&error){return Err(error)}held.tx.rollback().await.map_err(AppError::internal)?;outcome=Some(retained(id.clone(),"ownership_or_retained_references_require_review"));break}};
   if one.step==Step::Complete{let result=one.execute();held.tx.rollback().await.map_err(AppError::internal)?;if let Err(error)=result{if authority_error(&error){return Err(error)}outcome=Some(retained(id.clone(),"interrupted_completion_requires_retry"));break}
    let held=FencedTransaction::begin(pool).await?;match held.finalize_existing(auth,operation,&receipt).await{Ok(())=>outcome=Some(AssetOutcome{asset_id:id.clone(),outcome:"physically_completed",reason:None}),Err(error)=>{if authority_error(&error){return Err(error)}outcome=Some(retained(id.clone(),"completion_binding_requires_review"))}}break
   }
   budget-=1;let result=one.execute();held.tx.rollback().await.map_err(AppError::internal)?;if let Err(error)=result{if authority_error(&error){return Err(error)}outcome=Some(retained(id.clone(),"interrupted_owned_cleanup_requires_retry"));break}
  }
  outcomes.push(outcome.unwrap_or(AssetOutcome{asset_id:id,outcome:"pending",reason:Some("bounded_retry_with_original_operation")}));
 }
 #[cfg(test)]
 final_observation_test_gate(operation).await;
 // Every result, including an empty retained roster, is observed under fresh
 // original-operation and current-human locks after all per-asset transactions.
 let mut final_held=FencedTransaction::begin(pool).await?;
 let final_plan=original_plan(&mut final_held,auth,space,document,operation,target).await?;
 let expiry={let human=final_held.original_human(auth,&final_plan.actor_id,space).await?;human.check_now()?;human.session_expiry};
 let outcome=if outcomes.is_empty(){"retained_review"}else if outcomes.iter().any(|a|a.outcome=="retained_review"){"retained_review"}else if outcomes.iter().any(|a|a.outcome=="pending"){"pending"}else{"physically_completed"};
 let response=CleanupOutcome{operation_id:operation.into(),metadata_deleted:true,outcome,reason:if outcomes.is_empty(){Some("no_supported_owned_original_assets")}else{None},assets:outcomes};
 // The response observation linearizes while original account/session and all
 // scoped policy writers are excluded; this read observation commits no writes.
 final_held.original_human(auth,&final_plan.actor_id,space).await?.check_now()?;
 final_held.tx.rollback().await.map_err(AppError::internal)?;
 crate::spaces::access::principal_from_auth(auth)?;
 if expiry<=chrono::Utc::now(){return Err(AppError::Unauthorized("Original session expired before cleanup response".into()))}
 Ok(response)
}

#[cfg(test)]
type ObservationGate=(tokio::sync::oneshot::Sender<()>,tokio::sync::oneshot::Receiver<()>);
#[cfg(test)]
fn observation_gates()->&'static std::sync::Mutex<BTreeMap<String,ObservationGate>>{
 static GATES:std::sync::OnceLock<std::sync::Mutex<BTreeMap<String,ObservationGate>>>=std::sync::OnceLock::new();
 GATES.get_or_init(||std::sync::Mutex::new(BTreeMap::new()))
}
#[cfg(test)]
pub(super) fn pause_final_observation(operation:&str)->(tokio::sync::oneshot::Receiver<()>,tokio::sync::oneshot::Sender<()>){
 let(arrived_tx,arrived_rx)=tokio::sync::oneshot::channel();let(release_tx,release_rx)=tokio::sync::oneshot::channel();
 assert!(observation_gates().lock().unwrap().insert(operation.into(),(arrived_tx,release_rx)).is_none());(arrived_rx,release_tx)
}
#[cfg(test)]
async fn final_observation_test_gate(operation:&str){
 let gate=observation_gates().lock().unwrap().remove(operation);
 if let Some((arrived,release))=gate{let _=arrived.send(());release.await.expect("final-observation fixture release");}
}
