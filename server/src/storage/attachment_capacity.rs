//! Persistent admission bound to the original durable typed producer capsule.
//! Count retained attachment rows once and outstanding maximum reservations.
//! Failed/interrupted stages retain their reservation until an owned cleanup
//! protocol proves disposal; elapsed time never authorizes release.
#[path="attachment_retained_accounting.rs"]
mod retained;
use anyhow::{ensure, Context};
use sqlx::{Any, AnyPool, Transaction};
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Admission { Reserved, Committed { asset_id:String, bytes:i64 } }
fn request_valid(id:&str,sha:&str,maximum:i64)->anyhow::Result<()> {
 let parsed=uuid::Uuid::parse_str(id)?;
 ensure!(parsed.to_string()==id && maximum>=0,"Invalid capacity operation");
 ensure!(sha.len()==64 && sha.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)),"Invalid request digest");
 Ok(())
}
async fn serialize(tx:&mut Transaction<'_,Any>)->anyhow::Result<()> {
 let changed=sqlx::query("UPDATE attachment_capacity_lock SET revision=revision+1 WHERE id=1 AND revision<9223372036854775807")
 .execute(&mut **tx).await?.rows_affected();
 ensure!(changed==1,"Capacity guard missing or exhausted"); Ok(())
}
pub(crate) async fn reserve(pool:&AnyPool,lease:&super::attachment_upload::admission::WriterLease,limit:Option<i64>)->anyhow::Result<Admission>{reserve_inner(pool,lease,limit,None).await}
pub(crate) async fn reserve_authorized(pool:&AnyPool,lease:&super::attachment_upload::admission::WriterLease,limit:Option<i64>,auth:&crate::auth::middleware::AuthUser)->anyhow::Result<Admission>{reserve_inner(pool,lease,limit,Some(auth)).await}
async fn reserve_inner(pool:&AnyPool,lease:&super::attachment_upload::admission::WriterLease,limit:Option<i64>,auth:Option<&crate::auth::middleware::AuthUser>)->anyhow::Result<Admission>{
 reserve_observed(pool,lease,limit,auth,|_|Ok(())).await
}
async fn capture_admission_charge(tx:&mut Transaction<'_,Any>,lease:&super::attachment_upload::admission::WriterLease,physical:bool,use_completed:bool)->anyhow::Result<retained::Charge>{
 use super::attachment_upload::admission::committed_erasure::accounting;
 if use_completed {
  let paths=retained::charge(tx,false).await?.paths;
  let completed=accounting::capture_in(tx,paths,&[lease.clone()]).await?;
  retained::charge_proved(tx,physical,completed).await
 }else{retained::charge(tx,physical).await}
}
// Private injectable phase observer permits real SQL-wait regressions. Production
// calls provide only the no-op closure above; no runtime configuration exposes it.
async fn reserve_observed(pool:&AnyPool,lease:&super::attachment_upload::admission::WriterLease,limit:Option<i64>,auth:Option<&crate::auth::middleware::AuthUser>,mut checkpoint:impl FnMut(&str)->anyhow::Result<()>)->anyhow::Result<Admission>{
 use super::attachment_upload::admission::committed_erasure::accounting;
 lease.verify()?; let id=lease.attempt_id(); let sha=lease.producer_sha256(); let claim=lease.claim_sha256(); let maximum=i64::try_from(lease.maximum_bytes())?;
 request_valid(id,sha,maximum)?;
 ensure!(limit.is_none_or(|n|n>=0),"Negative capacity limit");
 let mut tx=pool.begin().await?;accounting::lock_global_in(&mut tx).await?;
 let completed_count:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_erasure_dispositions").fetch_one(&mut *tx).await?;
 let use_completed=completed_count>0&&limit.is_some();
 let authority=if use_completed{let auth=auth.context("Completed accounting admission requires current producer authority")?;Some(accounting::current_writer_in(&mut tx,auth,lease).await?)}else{None};
 serialize(&mut tx).await?;
 let prior:Option<(String,i64,String,Option<String>,Option<i64>,String)>=sqlx::query_as("SELECT request_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,admission_claim_sha256 FROM attachment_capacity_reservations WHERE operation_id=$1")
 .bind(id).fetch_optional(&mut *tx).await?;
 if let Some((old,bytes,state,asset,committed,old_claim))=prior {
  ensure!(old==sha && bytes==maximum && old_claim==claim,"Capacity operation request differs");
  let result=match state.as_str(){"reserved"=>Admission::Reserved,"committed"=>Admission::Committed{asset_id:asset.context("Committed capacity asset missing")?,bytes:committed.context("Committed capacity bytes missing")?},"disposed"=>anyhow::bail!("Original capacity attempt disposed; cannot resurrect"),_=>anyhow::bail!("Unknown capacity reservation state")};
  // A retained uncommitted reservation is not proof that its previous physical
  // observation survived the commit wait. Revalidate the original attempt, but
  // do not reserve its maximum twice or reapply a subsequently reduced quota.
  let observed=if state=="reserved"&&limit.is_some(){Some(capture_admission_charge(&mut tx,lease,true,use_completed).await?)}else{None};
  lease.verify()?;if let Some(held)=&observed{held.fence_async().await?;}if let Some(proof)=&authority{proof.check_now()?;}
  checkpoint("before-retry-rollback")?;
  tx.rollback().await?;
  checkpoint("after-retry-rollback")?;
  lease.verify()?;if let Some(held)=&observed{held.fence_async().await?;}if let Some(proof)=&authority{proof.check_now()?;}
  if let Some(held)=observed{held.release_async().await?;}
  lease.verify()?;if let Some(proof)=&authority{proof.check_now()?;}return Ok(result);
 }
 let retained=capture_admission_charge(&mut tx,lease,limit.is_some(),use_completed).await?;
 let used=retained.bytes;
 let pending:i64=sqlx::query_scalar("SELECT CAST(COALESCE(SUM(maximum_bytes),0) AS BIGINT) FROM attachment_capacity_reservations WHERE state='reserved'").fetch_one(&mut *tx).await?;
 let needed=used.checked_add(pending).and_then(|n|n.checked_add(maximum)).context("Capacity usage overflow")?;
 ensure!(limit.is_none_or(|n|needed<=n),"Attachment storage quota exhausted");
 sqlx::query("INSERT INTO attachment_capacity_reservations(operation_id,request_sha256,maximum_bytes,state,admission_claim_sha256) VALUES($1,$2,$3,'reserved',$4)")
 .bind(id).bind(sha).bind(maximum).bind(&claim).execute(&mut *tx).await?;lease.verify()?;retained.fence_async().await?;if let Some(proof)=&authority{proof.check_now()?;}
 checkpoint("before-commit")?;
 tx.commit().await?;
 checkpoint("after-commit")?;
 // Failure after the real commit retains the reservation and capsule. It is
 // unconfirmed admission, never a fabricated rollback or a committed attachment.
 lease.verify()?;retained.fence_async().await?;if let Some(proof)=&authority{proof.check_now()?;}
 retained.release_async().await?;
 lease.verify()?;if let Some(proof)=&authority{proof.check_now()?;}Ok(Admission::Reserved)
}
/// Must share the transaction that publishes the exact attachment and receipt.
/// This primitive does not decide file ownership or page authorization.
pub(crate) fn request_digest(request:&super::attachment_upload::UploadRequest)->anyhow::Result<String> {
 use sha2::{Digest,Sha256};
 Ok(hex::encode(Sha256::digest(serde_json::to_vec(request)?)))
}
pub(crate) async fn commit_direct_in(tx:&mut Transaction<'_,Any>,lease:&super::attachment_upload::admission::WriterLease,receipt:&super::attachment_upload::Receipt)->anyhow::Result<()> {
 lease.verify()?; let id=lease.attempt_id();let sha=lease.producer_sha256();let claim=lease.claim_sha256();
 let intent=&receipt.intent;let asset=&intent.asset_id;let bytes=intent.byte_length;
 request_valid(id,sha,bytes)?;
 ensure!(id==intent.attempt_id && lease.producer()==&super::attachment_upload::admission::Producer::Direct{request:intent.request.clone()},"Capacity winner attempt or request differs");
 serialize(tx).await?;
 let exact=super::attachment_upload::receipt(&mut **tx,&intent.request.operation_id).await.map_err(anyhow::Error::msg)?;
 ensure!(exact.as_ref()==Some(receipt),"Capacity winning upload receipt missing or differs");
 ensure!(super::attachment_upload::association(&mut **tx,receipt).await.map_err(anyhow::Error::msg)?,"Capacity winning attachment association differs");
 retained::persist_direct(&mut **tx,receipt).await?;
 let row:(String,i64,String,Option<String>,Option<i64>,String)=sqlx::query_as("SELECT request_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,admission_claim_sha256 FROM attachment_capacity_reservations WHERE operation_id=$1")
 .bind(id).fetch_one(&mut **tx).await?;
 ensure!(row.0==sha && row.5==claim && bytes<=row.1,"Capacity commit differs from reservation");
 if row.2=="committed" {ensure!(row.3.as_deref()==Some(asset.as_str())&&row.4==Some(bytes),"Capacity completion differs");return Ok(());}
 ensure!(row.2=="reserved","Unknown capacity state");
 let changed=sqlx::query("UPDATE attachment_capacity_reservations SET state='committed',committed_asset_id=$1,committed_bytes=$2 WHERE operation_id=$3 AND request_sha256=$4 AND state='reserved'")
 .bind(asset).bind(bytes).bind(id).bind(sha).execute(&mut **tx).await?.rows_affected();ensure!(changed==1,"Capacity reservation changed");lease.verify()?;Ok(())
}

// Current route and both-engine admission tests are added after lease hooks are frozen.

/// Explicit global managed attachment-byte quota; absence preserves unlimited admission.
pub(crate) fn configured_limit()->anyhow::Result<Option<i64>> {
 match std::env::var("ATTACHMENT_STORAGE_QUOTA_BYTES") {
  Err(std::env::VarError::NotPresent)=>Ok(None),
  Ok(value)=>{ensure!(!value.is_empty() && value.bytes().all(|b|b.is_ascii_digit()),"Invalid attachment storage quota configuration");Ok(Some(value.parse::<i64>().context("Attachment storage quota configuration overflow")?))},
  Err(_)=>anyhow::bail!("Invalid attachment storage quota configuration")
 }
}

/// Shares the winning replay attachment/full receipt transaction, with the original typed capsule held.
pub(crate) async fn commit_replay_in(tx:&mut Transaction<'_,Any>,lease:&super::attachment_upload::admission::WriterLease,receipt:&crate::sync::AssetReplayReceipt)->anyhow::Result<()> {
 use super::attachment_upload::admission::Producer;
 lease.verify()?;
 let id=lease.attempt_id();let sha=lease.producer_sha256();let claim=lease.claim_sha256();let bytes=receipt.byte_length;
 request_valid(id,sha,bytes)?;
 ensure!(receipt.admission_attempt_id.as_deref()==Some(id),"Replay capacity winning attempt differs");
 let expected=Producer::Replay {draft_scope:receipt.draft_scope.clone(),provider:receipt.incoming.provider.clone(),incoming_id:receipt.incoming.operation_id.clone(),actor_id:receipt.actor_id.clone(),
  space_id:receipt.space_id.clone(),document_id:receipt.document_id.clone(),asset_id:receipt.incoming.entity_id.clone(),payload_fingerprint:receipt.incoming.payload_fingerprint.clone(),
  binary_sha256:receipt.binary_sha256.clone(),byte_length:u64::try_from(bytes)?};
 ensure!(lease.producer()==&expected && receipt.incoming.entity_type=="asset" && receipt.incoming.action=="create","Replay capacity winning typed producer differs");
 serialize(tx).await?;
 ensure!(crate::sync::asset_receipt_current(&mut **tx,receipt).await.map_err(anyhow::Error::msg)?,"Replay capacity winning full receipt differs");
 ensure!(crate::sync::asset_association_current(&mut **tx,receipt).await.map_err(anyhow::Error::msg)?,"Replay capacity winning attachment association differs");
 retained::persist_replay(&mut **tx,receipt).await?;
 let row:(String,i64,String,Option<String>,Option<i64>,String)=sqlx::query_as("SELECT request_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,admission_claim_sha256 FROM attachment_capacity_reservations WHERE operation_id=$1")
  .bind(id).fetch_one(&mut **tx).await?;
 ensure!(row.0==sha && row.5==claim && bytes<=row.1,"Replay capacity commit differs from reservation");
 let asset=&receipt.incoming.entity_id;
 if row.2=="committed" {ensure!(row.3.as_deref()==Some(asset.as_str())&&row.4==Some(bytes),"Replay capacity completion differs");lease.verify()?;return Ok(());}
 ensure!(row.2=="reserved","Replay capacity state is not reserved");
 let changed=sqlx::query("UPDATE attachment_capacity_reservations SET state='committed',committed_asset_id=$1,committed_bytes=$2 WHERE operation_id=$3 AND request_sha256=$4 AND admission_claim_sha256=$5 AND state='reserved'")
  .bind(asset).bind(bytes).bind(id).bind(sha).bind(claim).execute(&mut **tx).await?.rows_affected();
 ensure!(changed==1,"Replay capacity reservation changed");lease.verify()?;Ok(())
}

#[cfg(all(test,target_os="linux"))]
#[path="attachment_retained_tests.rs"]
mod retained_tests;

// Logical accounting consistency observation only; grants no file mutation or subtraction.
pub(crate) async fn verify_retained_logical_in(conn:&mut sqlx::AnyConnection)->anyhow::Result<i64>{Ok(retained::charge(conn,false).await?.bytes)}

/// Guarded, checked attribution evidence only; never serving or unlink authority.
pub(crate) async fn retained_paths_in(conn:&mut sqlx::AnyConnection)->anyhow::Result<std::collections::BTreeMap<String,(String,i64)>>{Ok(retained::charge(conn,false).await?.paths)}

#[derive(Debug)]
pub(crate) struct UsageReport{pub retained_bytes:i64,pub logical_retained_bytes:i64,pub pending_bytes:i64}
pub(crate) async fn usage_report(pool:&AnyPool)->anyhow::Result<UsageReport>{
 use super::attachment_upload::admission::committed_erasure::accounting;
 let mut tx=pool.begin().await?;accounting::lock_global_in(&mut tx).await?;serialize(&mut tx).await?;
 let paths=retained::charge(&mut tx,false).await?.paths;let completed=accounting::capture_in(&mut tx,paths,&[]).await?;
 let retained=retained::charge_proved(&mut tx,true,completed).await?;
 let pending:i64=sqlx::query_scalar("SELECT CAST(COALESCE(SUM(maximum_bytes),0) AS BIGINT) FROM attachment_capacity_reservations WHERE state='reserved'").fetch_one(&mut *tx).await?;
 retained.fence_async().await?;let report=UsageReport{retained_bytes:retained.bytes,logical_retained_bytes:retained.logical_bytes,pending_bytes:pending};tx.rollback().await?;retained.fence_async().await?;retained.release_async().await?;Ok(report)
}

#[cfg(test)]
pub(crate) async fn test_completed_logical_charge_in(conn:&mut sqlx::AnyConnection,completed:super::attachment_upload::admission::committed_erasure::accounting::CompletedAccounting)->anyhow::Result<i64>{let result=retained::charge_proved(conn,false,completed).await?;result.fence_async().await?;let bytes=result.bytes;result.release_async().await?;Ok(bytes)}

#[cfg(all(test,target_os="linux"))]
#[path="attachment_capacity_custody_tests.rs"]
mod custody_tests;
