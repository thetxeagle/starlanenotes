//! Held completed absence for accounting only. SQL rows and serialized metadata
//! never grant physical cleanup, serving, writer rebinding or cold certification.
use super::*;
use anyhow::{ensure,Context};
use sqlx::{AnyConnection,Row};
use std::collections::BTreeMap;
pub(crate) struct CompletedAccounting{roster:Option<census::NativeRoster>,originals:BTreeMap<String,Receipt>,observations:Vec<ExistingCompletedNative>,restored:Option<std::sync::Arc<crate::storage::restoration::CompletedAbsenceAuthority>>}
impl CompletedAccounting{
 pub(crate) async fn require_restored_only_in(&self,conn:&mut AnyConnection)->anyhow::Result<()> {self.restored.as_ref().context("Restored-only capture owner absent")?.check_in(conn).await?;self.fence()}
 pub(crate) async fn fence_restored_in(&self,root:&std::path::Path,conn:&mut AnyConnection)->anyhow::Result<()> {
  let restored=self.restored.as_ref().context("Finalized completed capture requires retained restored owner")?;
  restored.check_subset_in(conn).await?;if let Some(roster)=&self.roster{roster.fence_in(conn).await?;}let paths=crate::storage::attachment_capacity::retained_paths_in(conn).await?;
  for (asset,receipt) in restored.originals() {
   let (op,expected):(String,String)=sqlx::query_as("SELECT deletion_operation_id,plan_sha256 FROM attachment_erasure_dispositions WHERE asset_id=$1").bind(asset).fetch_one(&mut *conn).await?;
   let reference=coordinator::references::prove_in_at(root,conn,None,&op,receipt,paths.clone(),false).await.map_err(|e|anyhow::anyhow!("{e:?}"))?;
   ensure!(digest(reference.plan_raw.as_bytes())==expected,"Finalized completed capture current reference plan differs");
  }
  restored.check_subset_in(conn).await?;self.fence()
 }
 pub(crate) fn matches(&self,r:&Receipt)->bool{self.originals.get(&r.intent.asset_id)==Some(r)}
 pub(crate) fn ids(&self)->impl Iterator<Item=&String>{self.originals.keys()}
 pub(crate) fn fence(&self)->anyhow::Result<()>{if let Some(restored)=&self.restored{restored.check()?;}if let Some(roster)=&self.roster{roster.fence()?;}for held in &self.observations{held.fence()?;}Ok(())}
}
fn same_material(original:&str,fresh:&str)->anyhow::Result<()>{
 ensure!(original.len()<=65536,"Completed material exceeds bound");let mut old:serde_json::Value=serde_json::from_str(original)?;let mut now:serde_json::Value=serde_json::from_str(fresh)?;
 ensure!(serde_json::to_string(&old)?==original,"Completed material encoding differs");
 let observed=old["family_observation_sha256"].as_str().context("Completed observation hash absent")?;ensure!(observed.len()==64&&observed.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)),"Completed observation hash malformed");
 old.as_object_mut().context("Completed material not object")?.remove("family_observation_sha256");now.as_object_mut().context("Fresh material not object")?.remove("family_observation_sha256");ensure!(old==now,"Completed original owned path material differs");Ok(())
}
pub(crate) async fn capture_in(conn:&mut AnyConnection,paths:BTreeMap<String,(String,i64)>,borrowed:&[WriterLease])->anyhow::Result<CompletedAccounting>{capture_in_at(&crate::storage::data_dir(),conn,paths,borrowed).await}
pub(crate) async fn capture_in_at(root:&std::path::Path,conn:&mut AnyConnection,paths:BTreeMap<String,(String,i64)>,borrowed:&[WriterLease])->anyhow::Result<CompletedAccounting>{
 let(count,bytes):(i64,i64)=sqlx::query_as("SELECT count(*),COALESCE(SUM(length(native_intent_json)+length(owned_path_set_json)),0) FROM attachment_erasure_dispositions").fetch_one(&mut *conn).await?;ensure!(count<=512&&bytes<=8*1024*1024,"Completed disposition inventory exceeds bound");
 let rows=sqlx::query("SELECT * FROM attachment_erasure_dispositions ORDER BY asset_id LIMIT 513").fetch_all(&mut *conn).await?;ensure!(rows.len()<=512,"Completed disposition count differs");
 let mut result=CompletedAccounting{roster:None,originals:BTreeMap::new(),observations:vec![],restored:None};if rows.is_empty(){return Ok(result)}
 let qualified=crate::storage::restoration::qualified_erasure_source_in(root,conn).await?;
 let restored=if let Some(old)=qualified.as_ref().and_then(|q|q.completed()){Some(old)}else{crate::storage::restoration::acquire_completed_absence_in(root,conn).await?.map(std::sync::Arc::new)};
 if let Some(restored)=restored {
  for (asset,receipt) in restored.originals() {
   let scope=rows.iter().find(|row|row.try_get::<String,_>("asset_id").ok().as_ref()==Some(asset)).context("Restored completed disposition absent")?;
   let op:String=scope.try_get("deletion_operation_id")?;
   let reference=coordinator::references::prove_in_at(root,conn,None,&op,receipt,paths.clone(),false).await.map_err(|e|anyhow::anyhow!("{e:?}"))?;
   ensure!(digest(reference.plan_raw.as_bytes())==scope.try_get::<String,_>("plan_sha256")?,"Restored completed current reference plan differs");
   result.originals.insert(asset.clone(),receipt.clone());
  }
  restored.check_subset_in(conn).await?;result.restored=Some(restored);result.fence()?;
 }
 let roster=census::NativeRoster::capture_qualified(root,borrowed,qualified.clone())?;roster.fence_in(conn).await?;
 for row in rows{
  if result.originals.contains_key(&row.try_get::<String,_>("asset_id")?){continue;}
  let op:String=row.try_get("deletion_operation_id")?;let asset:String=row.try_get("asset_id")?;let actor:String=row.try_get("actor_id")?;let raw:String=row.try_get("native_intent_json")?;ensure!(raw.len()<=16384,"Completed native intent exceeds bound");let intent:Intent=serde_json::from_str(&raw)?;let receipt:Receipt=serde_json::from_str(&intent.receipt_json)?;
  ensure!(asset==receipt.intent.asset_id,"Completed asset differs from original receipt");
  let reference=coordinator::references::prove_in_at(root,conn,None,&op,&receipt,paths.clone(),false).await.map_err(|e|anyhow::anyhow!("{e:?}"))?;
  let plan_sha=digest(reference.plan_raw.as_bytes());let lease=roster.lease(&receipt.intent.attempt_id)?;
  let owner=OwnedErasure::open_with_lease(lease,&receipt,&op,&plan_sha,&reference.plan.actor_id,false)?;
  let census=census::FamilyCensus::capture_roster_qualified(&owner,Some(paths.clone()),reference.finalized_journals,&roster,qualified.clone())?;
  census.fence_in(conn).await?;let material=owner.completed_path_material(&census)?;
  let original_paths:String=row.try_get("owned_path_set_json")?;same_material(&original_paths,&material)?;
  let expected=[("asset_id",receipt.intent.asset_id.clone()),("actor_id",reference.plan.actor_id.clone()),("space_id",reference.plan.space_id.clone()),("document_id",reference.plan.document_id.clone()),("producer_kind","original_direct_v1".into()),("original_attempt_id",receipt.intent.attempt_id.clone()),("plan_sha256",plan_sha),("original_receipt_sha256",owner.intent.receipt_sha256.clone()),("admission_claim_sha256",owner.lease.claim_sha256()),("native_intent_json",String::from_utf8(owner.raw.clone())?),("native_intent_sha256",digest(&owner.raw)),("native_tombstone_json",String::from_utf8(owner.raw.clone())?),("native_tombstone_sha256",digest(&owner.raw)),("owned_path_set_sha256",digest(original_paths.as_bytes())),("disposition","owned_paths_erased_v1".into())];
  for(name,value)in expected{ensure!(row.try_get::<String,_>(name)?==value,"Completed disposition scalar/raw binding differs: {name}");}
  ensure!(actor==reference.plan.actor_id,"Completed actor differs");let completed:String=row.try_get("completed_at")?;let completed=chrono::DateTime::parse_from_rfc3339(&completed)?;ensure!(completed>=chrono::DateTime::parse_from_rfc3339(&reference.plan.created_at)?&&completed<=chrono::Utc::now(),"Completed disposition time differs");
  ensure!(result.originals.insert(asset,receipt).is_none(),"Duplicate completed original");result.observations.push(ExistingCompletedNative{owner,census,material});
 }
 result.roster=Some(roster);result.fence()?;Ok(result)
}

pub(crate) async fn lock_global_in(conn:&mut AnyConnection)->anyhow::Result<()>{coordinator::lock_every_space(conn).await.map_err(|e|anyhow::anyhow!("{e:?}"))?;Ok(())}
pub(crate) struct WriterAuthority<'a>{auth:&'a crate::auth::middleware::AuthUser,expiry:Option<chrono::DateTime<chrono::FixedOffset>>}
impl WriterAuthority<'_>{pub(crate) fn check_now(&self)->anyhow::Result<()>{crate::spaces::access::principal_from_auth(self.auth).map_err(|e|anyhow::anyhow!("{e:?}"))?;ensure!(self.expiry.is_none_or(|expiry|expiry>chrono::Utc::now()),"Current writer credential expired");Ok(())}}
pub(crate) async fn current_writer_in<'a>(conn:&mut AnyConnection,auth:&'a crate::auth::middleware::AuthUser,lease:&WriterLease)->anyhow::Result<WriterAuthority<'a>>{
 use crate::spaces::access;lease.verify()?;
 let(actor,space,document)=match lease.producer(){Producer::Direct{request}=>(request.actor_id.as_str(),request.space_id.as_str(),request.document_id.as_str()),Producer::Replay{actor_id,space_id,document_id,..}=>(actor_id.as_str(),space_id.as_str(),document_id.as_str())};
 ensure!(auth.claims.sub==actor,"Exact current producer principal required");
 let principal=access::principal_from_auth(auth).map_err(|e|anyhow::anyhow!("{e:?}"))?;
 // Current policy supplies operation/attribution only for humans and personal
 // user keys. Service keys remain denied by normal page resolution.
 let expiry=match principal{
  access::Principal::User{..}=>{
   let user=sqlx::query("UPDATE users SET id=id WHERE id=$1").bind(actor).execute(&mut *conn).await?;let session=sqlx::query("UPDATE sessions SET id=id WHERE id=$1 AND user_id=$2").bind(&auth.claims.sid).bind(actor).execute(&mut *conn).await?;ensure!(user.rows_affected()==1&&session.rows_affected()==1,"Current writer rows unavailable");
   access::require_current_human_preference_write(&mut *conn,auth).await.map_err(|e|anyhow::anyhow!("{e:?}"))?;
   let expiry:String=sqlx::query_scalar("SELECT expires_at FROM sessions WHERE id=$1 AND user_id=$2").bind(&auth.claims.sid).bind(actor).fetch_one(&mut *conn).await?;Some(chrono::DateTime::parse_from_rfc3339(&expiry)?)
  },
  access::Principal::UserKey{key_id,user_id,..}=>{
   ensure!(user_id==actor,"Original key producer owner differs");let user=sqlx::query("UPDATE users SET id=id WHERE id=$1").bind(actor).execute(&mut *conn).await?;let key=sqlx::query("UPDATE api_keys SET id=id WHERE id=$1 AND user_id=$2 AND is_service_account=0").bind(key_id).bind(actor).execute(&mut *conn).await?;ensure!(user.rows_affected()==1&&key.rows_affected()==1,"Current personal key rows unavailable");
   let capability=access::resolve_current_key(&mut *conn,key_id).await.map_err(|e|anyhow::anyhow!("{e:?}"))?;let access::CurrentKeyCapability::User(current)=capability else{anyhow::bail!("Original personal key changed class")};ensure!(current.user_id==actor&&(current.scopes.contains(&access::ApiKeyScope::Write)||current.scopes.contains(&access::ApiKeyScope::Admin)),"Current personal key write ceiling unavailable");
   let expiry:Option<String>=sqlx::query_scalar("SELECT expires_at FROM api_keys WHERE id=$1 AND user_id=$2 AND is_service_account=0").bind(key_id).bind(actor).fetch_one(&mut *conn).await?;expiry.as_deref().map(chrono::DateTime::parse_from_rfc3339).transpose()?
  },
  access::Principal::ServiceKey{..}=>anyhow::bail!("Current service key page attribution unsupported"),
 };
 access::resolve_page_access(&mut *conn,principal,space,document,access::PageOperation::Edit,access::Purpose::Routine).await.map_err(|e|anyhow::anyhow!("{e:?}"))?;
 let proof=WriterAuthority{auth,expiry};proof.check_now()?;Ok(proof)
}

// Recovery-owned sealed source transport seam.
#[path="attachment_completed_capture.rs"]
pub(crate) mod capture;

// Source-only composite prototype; no cold or manifest integration.
#[path="attachment_composite_source.rs"]
pub(crate) mod composite;
