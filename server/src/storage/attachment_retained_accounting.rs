//! Conservative retained-byte accounting. This module grants no file mutation or serving authority.
use anyhow::{ensure, Context};
use serde::{Deserialize, Serialize};
use sha2::{Digest,Sha256};
use sqlx::{AnyConnection,Row};
use std::collections::{BTreeMap,BTreeSet};
const LIMIT:usize=8192;
#[derive(Debug,Clone,PartialEq,Eq,Serialize,Deserialize)]
struct Metadata {bytes:i64,filename:String,name:String,mime:String,uploader:String,created:String}
#[derive(Clone)]
struct Asset {metadata:Metadata,paths:BTreeSet<String>}
fn valid_path(path:&str)->bool { let p:Vec<_>=path.split('/').collect();p.len()==5&&p[0]=="spaces"&&p[2]=="attachments"&&p.iter().all(|s|!s.is_empty()&&*s!="."&&*s!=".."&&!s.contains('\\')&&!s.contains('\0')) }
fn digest(raw:&str)->String {hex::encode(Sha256::digest(raw.as_bytes()))}
fn add(assets:&mut BTreeMap<String,Asset>,id:String,m:Metadata,path:String)->anyhow::Result<()> {
 ensure!(!id.is_empty()&&m.bytes>=0&&valid_path(&path),"Invalid retained accounting identity");
 ensure!(assets.len()<LIMIT||assets.contains_key(&id),"Retained accounting asset bound exceeded");
 if let Some(old)=assets.get_mut(&id){ensure!(old.metadata==m,"Contradictory retained immutable metadata");old.paths.insert(path);}
 else{assets.insert(id,Asset{metadata:m,paths:BTreeSet::from([path])});}Ok(())
}
async fn rows(conn:&mut AnyConnection,table:&str)->anyhow::Result<Vec<sqlx::any::AnyRow>> {
 let rows=sqlx::query(&format!("SELECT * FROM {table} LIMIT {}",LIMIT+1)).fetch_all(conn).await?;ensure!(rows.len()<=LIMIT,"Retained accounting evidence bound exceeded");Ok(rows)
}
fn direct(r:&crate::storage::attachment_upload::Receipt)->(String,Metadata,String){let i=&r.intent;let q=&i.request;(i.asset_id.clone(),Metadata{bytes:i.byte_length,filename:i.filename.clone(),name:q.original_name.clone(),mime:q.mime_type.clone(),uploader:q.actor_id.clone(),created:i.created_at.clone()},i.storage_path.clone())}
fn replay(r:&crate::sync::AssetReplayReceipt)->(String,Metadata,String){(r.incoming.entity_id.clone(),Metadata{bytes:r.byte_length,filename:r.filename.clone(),name:r.original_name.clone(),mime:r.mime_type.clone(),uploader:r.actor_id.clone(),created:r.created_at.clone()},r.storage_path.clone())}
async fn persist(conn:&mut AnyConnection,kind:&str,parts:(String,Metadata,String),raw:String)->anyhow::Result<()> {
 let (id,m,path)=parts;
 let sha=digest(&raw);
 sqlx::query("INSERT INTO attachment_retained_byte_ledger(asset_id,byte_length,filename,original_name,mime_type,uploader_id,original_created_at,original_storage_path,evidence_kind,original_receipt_json,original_receipt_sha256) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) ON CONFLICT(asset_id) DO NOTHING")
 .bind(&id).bind(m.bytes).bind(&m.filename).bind(&m.name).bind(&m.mime).bind(&m.uploader).bind(&m.created).bind(&path).bind(kind).bind(&raw).bind(&sha).execute(&mut *conn).await?;
 let row=sqlx::query("SELECT * FROM attachment_retained_byte_ledger WHERE asset_id=$1").bind(&id).fetch_one(conn).await?;
 ensure!(ledger_metadata(&row)?==m,"Accounting winner immutable metadata differs");
 let prior_kind:String=row.try_get("evidence_kind")?;
 if prior_kind!="legacy_upgrade" {ensure!(prior_kind==kind&&row.try_get::<String,_>("original_storage_path")?==path&&row.try_get::<String,_>("original_receipt_json")?==raw&&row.try_get::<String,_>("original_receipt_sha256")?==sha,"Accounting winner original receipt differs");}
 else{ensure!(row.try_get::<Option<String>,_>("original_receipt_json")?.is_none()&&row.try_get::<Option<String>,_>("original_receipt_sha256")?.is_none(),"Legacy accounting record has fabricated receipt");}
 Ok(())
}
fn ledger_metadata(row:&sqlx::any::AnyRow)->anyhow::Result<Metadata>{Ok(Metadata{bytes:row.try_get("byte_length")?,filename:row.try_get("filename")?,name:row.try_get("original_name")?,mime:row.try_get("mime_type")?,uploader:row.try_get("uploader_id")?,created:row.try_get("original_created_at")?})}
pub(super) async fn persist_direct(conn:&mut AnyConnection,r:&crate::storage::attachment_upload::Receipt)->anyhow::Result<()> {let raw=serde_json::to_string(r)?;persist(conn,"direct",direct(r),raw).await}
pub(super) async fn persist_replay(conn:&mut AnyConnection,r:&crate::sync::AssetReplayReceipt)->anyhow::Result<()> {let raw=serde_json::to_string(r)?;persist(conn,"replay",replay(r),raw).await}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClaimIdentity{device:u64,inode:u64}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AccountingClaim{version:u8,attempt_id:String,producer:crate::storage::attachment_upload::admission::Producer,producer_sha256:String,maximum_bytes:u64,root:ClaimIdentity,namespace:ClaimIdentity,capsule:ClaimIdentity}
// Values remain movable; only the exact owned evidence is shared with blocking
// fence workers. Sharing never reopens, clones authority or renews a generation.
pub(super) struct Charge {
 pub paths:BTreeMap<String,(String,i64)>, pub bytes:i64, pub logical_bytes:i64,
 guards:ChargeGuardOwner,
}
struct ChargeEvidence {
 root:std::path::PathBuf,
 inventory:Option<inventory::PhysicalInventory>,
 claims:Vec<crate::storage::attachment_upload::HeldVersionPath>,
 completed:Option<crate::storage::attachment_upload::admission::committed_erasure::accounting::CompletedAccounting>,
 cancelled:std::sync::atomic::AtomicBool,
}
impl ChargeEvidence {
 fn cancel(&self) {
  self.cancelled.store(true,std::sync::atomic::Ordering::SeqCst);
  if let Some(inventory)=&self.inventory {inventory.cancel();}
 }
 fn current(&self)->anyhow::Result<()> {
  ensure!(!self.cancelled.load(std::sync::atomic::Ordering::SeqCst),"Retained accounting observation was cancelled");
  ensure!(crate::storage::data_dir()==self.root,"Retained accounting root selection changed");Ok(())
 }
 fn fence(&self)->anyhow::Result<()> {self.fence_observed(||Ok(()))}
 fn fence_observed(&self,after_proofs:impl FnOnce()->anyhow::Result<()>)->anyhow::Result<()> {
  self.current()?;
  if let Some(inventory)=&self.inventory {inventory.fence()?;}
  for claim in &self.claims {self.current()?;claim.fence(&self.root)?;}
  if let Some(proof)=&self.completed {self.current()?;proof.fence()?;}
  after_proofs()?;
  if let Some(inventory)=&self.inventory {inventory.fence()?;}
  self.current()
 }
 fn release_blocking(mut self) {
  self.cancel();
  if let Some(inventory)=self.inventory.take() {inventory.release_blocking();}
 }
}
fn release_charge_evidence(evidence:std::sync::Arc<ChargeEvidence>) {
 // Final filesystem owners must not join a custody thread on an async executor.
 if let Ok(runtime)=tokio::runtime::Handle::try_current() {
  runtime.spawn_blocking(move||drop(evidence));
 } else {drop(evidence);}
}
struct ChargeGuardOwner {evidence:Option<std::sync::Arc<ChargeEvidence>>}
impl ChargeGuardOwner {
 fn held(&self)->&std::sync::Arc<ChargeEvidence> {self.evidence.as_ref().expect("retained evidence owner is live")}
}
impl Drop for ChargeGuardOwner {
 fn drop(&mut self) {if let Some(evidence)=self.evidence.take(){evidence.cancel();release_charge_evidence(evidence);}}
}
struct ChargeFenceAwait {evidence:Option<std::sync::Arc<ChargeEvidence>>,disarmed:bool}
impl Drop for ChargeFenceAwait {
 fn drop(&mut self) {
  if let Some(evidence)=self.evidence.take(){if !self.disarmed{evidence.cancel();}release_charge_evidence(evidence);}
 }
}
impl Charge {
 #[cfg(test)]
 pub(super) fn test_fence_after_proofs(&self,phase:impl FnOnce()->anyhow::Result<()>)->anyhow::Result<()> {self.guards.held().fence_observed(phase)}
 pub fn fence(&self)->anyhow::Result<()> {self.guards.held().fence()}
 pub async fn fence_async(&self)->anyhow::Result<()> {
  let mut awaiting=ChargeFenceAwait{evidence:Some(self.guards.held().clone()),disarmed:false};
  let evidence=self.guards.held().clone();
  tokio::task::spawn_blocking(move||evidence.fence()).await??;
  self.guards.held().current()?;
  awaiting.disarmed=true;
  // The borrowed Charge still owns an Arc here, so dropping this successful
  // await reference cannot destroy evidence or defer a native lock release.
  awaiting.evidence.take();Ok(())
 }
 pub async fn release_async(mut self)->anyhow::Result<()> {
  if let Some(evidence)=self.guards.evidence.take() {
   evidence.cancel();
   tokio::task::spawn_blocking(move|| {
    match std::sync::Arc::try_unwrap(evidence) {
     Ok(evidence)=>evidence.release_blocking(),
     Err(evidence)=>drop(evidence), // An abandoned fence still owns its canceled generation.
    }
   }).await?;
  }
  Ok(())
 }
}
pub(super) async fn charge(conn:&mut AnyConnection,quota:bool)->anyhow::Result<Charge>{charge_inner(conn,quota,None).await}
pub(super) async fn charge_proved(conn:&mut AnyConnection,quota:bool,completed:crate::storage::attachment_upload::admission::committed_erasure::accounting::CompletedAccounting)->anyhow::Result<Charge>{charge_inner(conn,quota,Some(completed)).await}
async fn charge_inner(conn:&mut AnyConnection,quota:bool,completed:Option<crate::storage::attachment_upload::admission::committed_erasure::accounting::CompletedAccounting>)->anyhow::Result<Charge>{
 let root=crate::storage::data_dir();
 let mut assets=BTreeMap::new();let mut ledger=BTreeSet::new();let mut claims=Vec::new();let mut replay_receipts=BTreeMap::new();let mut direct_kinds=BTreeSet::new();
 for row in rows(conn,"attachment_retained_byte_ledger").await? {
  let id:String=row.try_get("asset_id")?;let kind:String=row.try_get("evidence_kind")?;let raw:Option<String>=row.try_get("original_receipt_json")?;let sha:Option<String>=row.try_get("original_receipt_sha256")?;
  let m=ledger_metadata(&row)?;let path:String=row.try_get("original_storage_path")?;
  match kind.as_str(){"legacy_upgrade"=>ensure!(raw.is_none()&&sha.is_none(),"Legacy ledger has fabricated receipt"),"direct"=>{direct_kinds.insert(id.clone());let raw=raw.context("Direct ledger receipt missing")?;ensure!(sha.as_deref()==Some(digest(&raw).as_str()),"Ledger receipt hash differs");let r:crate::storage::attachment_upload::Receipt=serde_json::from_str(&raw)?;ensure!(crate::storage::attachment_upload::receipt(conn,&r.intent.request.operation_id).await.map_err(anyhow::Error::msg)?.as_ref()==Some(&r),"Original direct ledger receipt missing or differs");ensure!(direct(&r)==(id.clone(),m.clone(),path.clone()),"Ledger original direct identity differs");},"replay"=>{let raw=raw.context("Replay ledger receipt missing")?;ensure!(sha.as_deref()==Some(digest(&raw).as_str()),"Ledger replay receipt hash differs");let r:crate::sync::AssetReplayReceipt=serde_json::from_str(&raw)?;ensure!(crate::sync::asset_receipt_current(conn,&r).await.map_err(anyhow::Error::msg)?,"Original replay ledger receipt missing or differs");ensure!(replay(&r)==(id.clone(),m.clone(),path.clone()),"Ledger original replay identity differs");},_=>anyhow::bail!("Unknown ledger evidence kind")}
  ledger.insert(id.clone());add(&mut assets,id,m,path)?;
 }
 let original_paths:BTreeMap<_,_>=assets.iter().map(|(id,a)|(id.clone(),a.paths.clone())).collect();let mut current_paths=Vec::new();let mut move_paths=BTreeMap::<String,BTreeSet<String>>::new();
 for row in rows(conn,"attachments").await? {let id:String=row.try_get("id")?;ensure!(!quota||ledger.contains(&id),"Current legacy attachment lacks migration accounting record");current_paths.push((id.clone(),row.try_get::<String,_>("storage_path")?));add(&mut assets,id,Metadata{bytes:row.try_get("size_bytes")?,filename:row.try_get("filename")?,name:row.try_get("original_name")?,mime:row.try_get("mime_type")?,uploader:row.try_get("created_by")?,created:row.try_get("created_at")?},row.try_get("storage_path")?)?;}
 let mut receipts=BTreeMap::new();let mut hashes=BTreeMap::new();
 for row in rows(conn,"attachment_upload_receipts").await? {let operation:String=row.try_get("operation_id")?;let r=crate::storage::attachment_upload::receipt(conn,&operation).await.map_err(anyhow::Error::msg)?.context("Direct retained receipt disappeared")?;let raw:String=row.try_get("receipt_json")?;ensure!(serde_json::to_string(&r)?==raw,"Original receipt encoding differs");if quota&&!completed.as_ref().is_some_and(|proof|proof.matches(&r)){crate::storage::attachment_upload::verify(&root,&r,true)?;}
 let parts=direct(&r);hashes.insert(parts.0.clone(),r.intent.binary_sha256.clone());ensure!(!quota||ledger.contains(&parts.0),"Original upload lacks accounting record");ensure!(receipts.insert(parts.0.clone(),(r.clone(),raw)).is_none(),"Multiple upload receipts claim one immutable asset");add(&mut assets,parts.0,parts.1,parts.2)?;}
 for row in rows(conn,"asset_replay_receipts").await? {let raw:String=row.try_get("receipt_json")?;let r:crate::sync::AssetReplayReceipt=serde_json::from_str(&raw)?;ensure!(crate::sync::asset_receipt_current(conn,&r).await.map_err(anyhow::Error::msg)?,"Original replay receipt differs");let p=replay(&r);ensure!(replay_receipts.insert(p.0.clone(),r.clone()).is_none(),"Multiple replay receipts claim one immutable asset");ensure!(hashes.insert(p.0.clone(),r.binary_sha256.clone()).is_none(),"Multiple producer receipts claim one immutable asset");ensure!(!quota||ledger.contains(&p.0),"Retained replay lacks accounting record");add(&mut assets,p.0,p.1,p.2)?;}
 for row in rows(conn,"attachment_version_records").await? {let id:String=row.try_get("version_id")?;let (r,raw)=receipts.get(&id).context("Immutable version original receipt missing")?;let i=&r.intent;let q=&i.request;
  ensure!(row.try_get::<String,_>("provenance")?=="direct_upload_v1"&&row.try_get::<String,_>("upload_operation_id")?==q.operation_id&&row.try_get::<String,_>("upload_attempt_id")?==i.attempt_id&&row.try_get::<String,_>("binary_sha256")?==i.binary_sha256&&row.try_get::<String,_>("raw_receipt_sha256")?==digest(raw)&&row.try_get::<String,_>("intent_sha256")?==r.intent_sha256&&row.try_get::<String,_>("install_proof_sha256")?==r.install_proof_sha256,"Immutable version provenance differs");
  add(&mut assets,id,Metadata{bytes:row.try_get("byte_length")?,filename:row.try_get("original_filename")?,name:row.try_get("original_name")?,mime:row.try_get("mime_type")?,uploader:row.try_get("uploader_id")?,created:row.try_get("original_created_at")?},row.try_get("original_storage_path")?)?;
 }
 // Validate registry sequence and full replacement receipt even when the live page has been cascaded.
 let version_rows=rows(conn,"attachment_version_records").await?;let mut numbered=BTreeMap::new();let version_ids:BTreeSet<String>=version_rows.iter().map(|r|r.try_get("version_id")).collect::<Result<_,_>>()?;ensure!(direct_kinds.is_subset(&version_ids),"New direct ledger missing immutable registry evidence");
 for row in &version_rows {let id:String=row.try_get("version_id")?;let family:String=row.try_get("logical_attachment_id")?;let number:i64=row.try_get("version_number")?;let(r,_)=receipts.get(&id).context("Registry original upload missing")?;ensure!(number>0&&row.try_get::<String,_>("original_space_id")?==r.intent.request.space_id&&row.try_get::<String,_>("original_document_id")?==r.intent.request.document_id,"Registry original scope/ordinal differs");ensure!(numbered.insert((family,number),id).is_none(),"Registry ordinal ambiguous");}
 let mut expected_changes=BTreeMap::new();let mut seen=0usize;
 for head in rows(conn,"attachment_families").await? {let family:String=head.try_get("id")?;let revision:i64=head.try_get("revision")?;ensure!(revision>0&&revision<=LIMIT as i64&&head.try_get::<String,_>("state")?=="active","Invalid retained registry head");let space:String=head.try_get("space_id")?;let doc:String=head.try_get("document_id")?;let created:String=head.try_get("created_at")?;
  for ordinal in 1..=revision {let id=numbered.get(&(family.clone(),ordinal)).context("Retained registry sequence missing")?;let(r,raw)=receipts.get(id).context("Retained registry original receipt missing")?;ensure!(r.intent.request.document_id==doc,"Retained registry document differs");if ordinal==1 {ensure!(id==&family&&r.intent.request.space_id==space&&r.intent.created_at==created,"Retained family original singleton differs");}else {let previous=numbered.get(&(family.clone(),ordinal-1)).context("Retained predecessor missing")?;expected_changes.insert(r.intent.request.operation_id.clone(),(family.clone(),space.clone(),doc.clone(),created.clone(),ordinal,previous.clone(),id.clone(),digest(raw)));}seen+=1;}
  ensure!(numbered.get(&(family,revision)).map(String::as_str)==Some(head.try_get::<String,_>("current_version_id")?.as_str()),"Retained registry head version differs");
 }
 ensure!(seen==version_rows.len(),"Orphan retained registry version");let changes=rows(conn,"attachment_version_changes").await?;ensure!(changes.len()==expected_changes.len(),"Missing/extra retained replacement receipt");
 for row in changes {let op:String=row.try_get("operation_id")?;let(family,space,doc,created,ordinal,previous,id,raw_sha)=expected_changes.get(&op).context("Retained replacement operation unproved")?;let raw:String=row.try_get("receipt_json")?;let c:crate::storage::attachment_versions::ReplacementChange=serde_json::from_str(&raw)?;let(r,_) =receipts.get(id).context("Retained replacement receipt missing")?;let i=&r.intent;let q=&i.request;
  ensure!(serde_json::to_string(&c)?==raw&&c.expected.id==*family&&c.expected.space_id==*space&&c.expected.document_id==*doc&&c.expected.created_at==*created&&c.expected.state=="active"&&c.expected.revision==ordinal-1&&c.expected.current_version_id==*previous&&c.version.version_id==*id&&c.version.logical_attachment_id==*family&&c.version.version_number==*ordinal&&c.version.original_space_id==q.space_id&&c.version.original_document_id==q.document_id&&c.version.uploader_id==q.actor_id&&c.version.original_name==q.original_name&&c.version.mime_type==q.mime_type&&c.version.byte_length==i.byte_length&&c.version.binary_sha256==i.binary_sha256&&c.version.original_created_at==i.created_at&&c.version.original_storage_path==i.storage_path&&c.version.original_filename==i.filename&&c.version.upload_operation_id==op&&c.version.upload_attempt_id==i.attempt_id&&c.version.raw_receipt_sha256==*raw_sha&&c.version.intent_sha256==r.intent_sha256&&c.version.install_proof_sha256==r.install_proof_sha256&&c.version.provenance=="direct_upload_v1","Retained replacement full receipt differs");
  ensure!(row.try_get::<String,_>("family_id")?==*family&&row.try_get::<String,_>("expected_version_id")?==*previous&&row.try_get::<i64,_>("expected_revision")?==ordinal-1&&row.try_get::<String,_>("new_version_id")?==*id&&row.try_get::<i64,_>("applied_revision")?==*ordinal&&row.try_get::<String,_>("actor_id")?==q.actor_id&&row.try_get::<String,_>("origin")?==c.origin&&row.try_get::<String,_>("space_id")?==q.space_id&&row.try_get::<String,_>("document_id")?==q.document_id&&row.try_get::<String,_>("upload_receipt_sha256")?==*raw_sha&&row.try_get::<String,_>("changed_at")?==i.created_at,"Retained replacement scalar differs");
 }
 for row in rows(conn,"attachment_retirements").await? {let id:String=row.try_get("attachment_id")?;let r=crate::storage::attachment_retirement::read(conn,&id).await.map_err(anyhow::Error::msg)?.context("Retirement receipt disappeared")?;crate::sync::inspect_retirement_receipt(conn,&r).await.map_err(anyhow::Error::msg)?;let a=&r.association;add(&mut assets,id,Metadata{bytes:a.3,filename:a.0.clone(),name:a.1.clone(),mime:a.2.clone(),uploader:a.5.clone(),created:a.6.clone()},a.4.clone())?;}
 for row in rows(conn,"attachment_capacity_reservations").await? {let attempt:String=row.try_get("operation_id")?;let maximum:i64=row.try_get("maximum_bytes")?;let request_sha:String=row.try_get("request_sha256")?;let claim_sha:String=row.try_get("admission_claim_sha256")?;let state:String=row.try_get("state")?;ensure!(maximum>=0,"Negative retained reservation maximum");
  if quota {ensure!(uuid::Uuid::parse_str(&attempt)?.to_string()==attempt,"Invalid retained attempt identity");let held=crate::storage::attachment_upload::HeldVersionPath::open(&root,&format!(".attachment-upload-admission/{attempt}/claim.json"))?;let raw=held.bounded_bytes(16384)?;let claim:AccountingClaim=serde_json::from_slice(&raw)?;ensure!(claim.version==1&&claim.attempt_id==attempt&&claim.maximum_bytes==maximum as u64&&claim.producer_sha256==request_sha&&hex::encode(Sha256::digest(serde_json::to_vec(&claim.producer)?))==request_sha&&hex::encode(Sha256::digest(&raw))==claim_sha&&claim.root.inode>0&&claim.namespace.inode>0&&claim.capsule.inode>0,"Retained original admission claim differs");let _devices=(claim.root.device,claim.namespace.device,claim.capsule.device);
   if state=="committed" {let id:String=row.try_get("committed_asset_id")?;let bytes:i64=row.try_get("committed_bytes")?;ensure!(bytes<=maximum&&assets.get(&id).is_some_and(|a|a.metadata.bytes==bytes),"Committed reservation accounting winner missing or differs");match claim.producer{crate::storage::attachment_upload::admission::Producer::Direct{request}=>{let(r,_)=receipts.get(&id).context("Committed direct original receipt missing")?;ensure!(r.intent.request==request&&r.intent.attempt_id==attempt&&r.intent.byte_length==bytes,"Committed direct original admission differs");},crate::storage::attachment_upload::admission::Producer::Replay{provider,incoming_id,actor_id,space_id,document_id,asset_id,payload_fingerprint,binary_sha256,byte_length,draft_scope}=>{let r=replay_receipts.get(&id).context("Committed replay original receipt missing")?;ensure!(r.draft_scope==draft_scope&&r.incoming.provider==provider&&r.incoming.operation_id==incoming_id&&r.actor_id==actor_id&&r.space_id==space_id&&r.document_id==document_id&&r.incoming.entity_id==asset_id&&r.incoming.payload_fingerprint==payload_fingerprint&&r.binary_sha256==binary_sha256&&r.byte_length as u64==byte_length&&r.admission_attempt_id.as_deref()==Some(attempt.as_str()),"Committed replay original admission differs");}}}
   else if state=="disposed" {let raw:String=row.try_get("disposition_json")?;ensure!(row.try_get::<String,_>("disposition_sha256")?==digest(&raw),"Disposed original disposition hash differs");let v:serde_json::Value=serde_json::from_str(&raw)?;ensure!(v["attempt_id"]==attempt&&v["request_sha256"]==request_sha&&v["admission_claim_sha256"]==claim_sha&&v["maximum_bytes"].as_i64()==Some(maximum),"Disposed original reservation differs");}
   else{ensure!(state=="reserved","Unknown retained capacity state");}claims.push(held);
  }
 }
 if quota {
 // Aliases are accepted only from the exact committed move, sealed native receipt and escrow.
 for row in rows(conn,"move_asset_path_aliases").await? {let kind:String=row.try_get("asset_kind")?;if kind!="attachment"{continue}let id:String=row.try_get("asset_id")?;let old:String=row.try_get("old_path")?;let op:String=row.try_get("op_id")?;let m=crate::storage::relocation::logical_receipt(conn,&op).await.map_err(|e|anyhow::anyhow!("{e:?}"))?.context("Accounting move receipt missing")?;crate::storage::relocation::verify_escrow(conn,&m).await.map_err(|e|anyhow::anyhow!("{e:?}"))?;crate::storage::relocation::verify_sealed_receipt(&root,&m).map_err(|e|anyhow::anyhow!("{e:?}"))?;
  let manifest:crate::storage::relocation::PreparedRelocation=serde_json::from_str(m["manifest_json"].as_str().context("Move manifest missing")?)?;ensure!(crate::storage::relocation::valid_manifest(&manifest),"Invalid accounting relocation manifest");let matching:Vec<_>=manifest.files.iter().filter(|f|f.kind=="attachment"&&f.subject_id==id&&f.source==old).collect();ensure!(matching.len()==1,"Accounting alias owner ambiguous");let f=matching[0];let asset=assets.get_mut(&id).context("Alias retained accounting owner missing")?;ensure!(f.length==asset.metadata.bytes as u64&&hashes.get(&id).is_none_or(|hash|hash==&f.sha256),"Alias original length/hash differs");crate::storage::relocation::verify_attachment_pair(&root,&manifest,f).map_err(|e|anyhow::anyhow!("{e:?}"))?;asset.paths.insert(f.source.clone());asset.paths.insert(f.destination.clone());move_paths.entry(id).or_default().extend([f.source.clone(),f.destination.clone()]);
 }
 }
 if quota {for(id,path)in current_paths {ensure!(original_paths.get(&id).is_some_and(|p|p.contains(&path))||move_paths.get(&id).is_some_and(|p|p.contains(&path)),"Current attachment path lacks original accounting or authenticated move evidence");}}
 if let Some(proof)=&completed{proof.fence()?;for id in proof.ids(){ensure!(assets.remove(id).is_some(),"Completed original missing immutable accounting evidence");}}
 let mut bytes=0i64;for asset in assets.values(){bytes=bytes.checked_add(asset.metadata.bytes).context("Retained accounting overflow")?;}
 let logical_bytes=bytes;let inventory=if quota {let (held,charged)=inventory::PhysicalInventory::capture(root.clone(),assets.clone()).await?;bytes=charged;Some(held)}else{None};
 let mut paths=BTreeMap::new();for(id,a)in &assets{for path in &a.paths{if let Some(prior)=paths.insert(path.clone(),(id.clone(),a.metadata.bytes)){ensure!(prior==(id.clone(),a.metadata.bytes),"Ambiguous retained path evidence");}}}
 Ok(Charge{paths,bytes,logical_bytes,guards:ChargeGuardOwner{evidence:Some(std::sync::Arc::new(ChargeEvidence{root,inventory,claims,completed,cancelled:std::sync::atomic::AtomicBool::new(false)}))}})
}
#[path="attachment_retained_inventory.rs"]
mod inventory;
