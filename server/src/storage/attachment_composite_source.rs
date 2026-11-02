//! Original-source basis only for a future distinct composite transport.
//! This module emits no manifest field, destination owner, WriterLease or disposal grant.
use super::*;
use crate::storage::restoration::{admission::CapturedReservation,linux::PriorCompletedCaptureEvidence};
use std::path::{Path,PathBuf};

#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize)]
#[serde(tag="class",rename_all="snake_case")]
enum NativeClass {
 Reserved{supported_original_scope:Option<crate::storage::restoration::admission::AdmissionOwnerScope>},
 CommittedDirect{receipt_json:String},
 DisposedDirect{original_intent_json:String},
 DisposedReplay{original_intent_json:String},
}
#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize)]
struct NativeBasis {row:CapturedReservation,claim_json:String,class:NativeClass}
struct HeldNativeBasis {absence:Option<coordinator::DisposedPendingSource>,lease:WriterLease,records:crate::storage::attachment_upload::admission::CapturedCapsuleRecords,basis:NativeBasis}
/// Evidence is deliberately not Deserialize: this source prototype has no cold constructor.
#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize)]
struct CompositeBasis {version:u32,prior:PriorCompletedCaptureEvidence,prior_native:Option<Vec<crate::storage::restoration::composite::NativeOwnerScope>>,native:Vec<NativeBasis>}
/// Retains one original complete roster; never reopens old capsules as native writers.
pub(crate) struct CompositeOriginalSource {root:PathBuf,held:CompletedAccounting,basis:CompositeBasis,native:Vec<HeldNativeBasis>}
impl CompositeOriginalSource {
 pub(crate) async fn hold_in(root:&Path,tx:&mut sqlx::Transaction<'_,sqlx::Any>)->anyhow::Result<Self>{
  let paths=crate::storage::attachment_capacity::retained_paths_in(&mut **tx).await?;
  let held=super::capture_in_at(root,&mut **tx,paths,&[]).await?;
  let roster=held.roster.as_ref().context("Composite requires original complete held roster")?;
  let q=roster.qualified().context("Composite requires original registered qualified source")?;
  let prior=q.prior_completed_evidence()?;prior.validate_shape()?;let prior_native=q.prior_composite_native()?;
  // This first basis does not silently cover additional native completed erasures.
  let old_assets=prior.roles.scopes.iter().map(|s|s.original.disposition["asset_id"].clone()).collect::<std::collections::BTreeSet<_>>();
  let actual:Vec<String>=sqlx::query_scalar("SELECT asset_id FROM attachment_erasure_dispositions ORDER BY asset_id LIMIT 513").fetch_all(&mut **tx).await?;
  ensure!(actual.len()<=512&&actual.iter().cloned().collect::<std::collections::BTreeSet<_>>()==old_assets,"Additional completed scopes require separate composite proof");
  let old_attempts=prior.roles.scopes.iter().map(|s|s.original.reservation.operation_id.clone()).chain(prior_native.iter().flatten().map(|s|s.original.reservation.operation_id.clone())).collect::<std::collections::BTreeSet<_>>();
  let all:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id LIMIT 513").fetch_all(&mut **tx).await?;
  ensure!(all.len()<=512&&old_attempts.iter().all(|id|all.contains(id)),"Composite reservation inventory exceeds bound or loses old scope");
  let mut native=Vec::new();
  for attempt in all.iter().filter(|id|!old_attempts.contains(*id)) {
   let lease=roster.lease(attempt)?;let row=super::capture::reservation_in(&mut **tx,attempt).await?;Self::bind_row(&lease,&row)?;
   let class=match row.state.as_str(){
    "reserved"=>NativeClass::Reserved{supported_original_scope:lease.capture_supported_scope(tx).await?},
    "committed"=>{let Producer::Direct{request}=lease.producer()else{anyhow::bail!("Composite committed Replay requires distinct winning receipt proof")};
     let raw:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1 AND attempt_id=$2").bind(&request.operation_id).bind(attempt).fetch_one(&mut **tx).await?;
     let receipt:Receipt=serde_json::from_str(&raw)?;ensure!(receipt.intent.request==*request&&receipt.intent.attempt_id==*attempt&&row.committed_asset_id.as_deref()==Some(receipt.intent.asset_id.as_str())&&row.committed_bytes==Some(receipt.intent.byte_length),"Composite native winning receipt differs");NativeClass::CommittedDirect{receipt_json:raw}},
    "disposed"=>{Self::strict_disposed_in(&lease,&row,&mut **tx).await?;let raw=row.disposition_json.clone().context("Composite disposition absent")?;match lease.producer(){Producer::Direct{..}=>NativeClass::DisposedDirect{original_intent_json:raw},Producer::Replay{..}=>NativeClass::DisposedReplay{original_intent_json:raw}}},
    _=>anyhow::bail!("Composite unknown native admission state"),
   };
   let absence=if row.state=="disposed"{let paths=crate::storage::attachment_capacity::retained_paths_in(&mut **tx).await?;Some(coordinator::capture_disposed_pending_source_in(&lease,&mut **tx,paths,roster).await?)}else{None};let claim_json=std::str::from_utf8(lease.original_claim())?.to_owned();let records=lease.capture_record_fence()?;
   native.push(HeldNativeBasis{absence,lease,records,basis:NativeBasis{row,claim_json,class}});
  }
  ensure!(!native.is_empty()||prior_native.is_some(),"Composite basis requires native or retained finalized scopes");
  let basis=CompositeBasis{version:if prior_native.is_some(){2}else{1},prior,prior_native,native:native.iter().map(|h|h.basis.clone()).collect()};
  ensure!(serde_json::to_vec(&basis)?.len()<=8*1024*1024,"Composite source evidence exceeds bound");
  let result=Self{root:root.to_owned(),held,basis,native};result.recheck_in(&mut **tx).await?;Ok(result)
 }
 fn bind_row(lease:&WriterLease,row:&CapturedReservation)->anyhow::Result<()>{
  lease.verify()?;ensure!(row.operation_id==lease.attempt_id()&&row.request_sha256==lease.producer_sha256()&&row.admission_claim_sha256==lease.claim_sha256()&&row.maximum_bytes==lease.maximum_bytes()as i64,"Composite original typed claim/reservation differs");
  match row.state.as_str(){"reserved"=>ensure!(row.committed_asset_id.is_none()&&row.committed_bytes.is_none()&&row.disposition_json.is_none()&&row.disposition_sha256.is_none(),"Composite reserved shape differs"),"committed"=>ensure!(row.committed_asset_id.is_some()&&row.committed_bytes.is_some_and(|n|n>=0&&n<=row.maximum_bytes)&&row.disposition_json.is_none()&&row.disposition_sha256.is_none(),"Composite committed shape differs"),"disposed"=>ensure!(row.committed_asset_id.is_none()&&row.committed_bytes.is_none()&&row.disposition_json.is_some()&&row.disposition_sha256.is_some(),"Composite disposed shape differs"),_=>anyhow::bail!("Composite unknown reservation state")};Ok(())
 }
 async fn strict_disposed_in(lease:&WriterLease,row:&CapturedReservation,conn:&mut sqlx::AnyConnection)->anyhow::Result<()> {
  let raw=row.disposition_json.as_deref().context("Composite disposed raw intent absent")?;let sha=row.disposition_sha256.as_deref().context("Composite disposed hash absent")?;
  ensure!(raw.len()<=8*1024*1024,"Composite disposal record exceeds bound");lease.observe_native_disposed_in(conn,raw,sha).await?;
  // Bind every original plan scalar in addition to the existing idle native
  // tombstone/path absence/protected-reference observation.
  let (version,attempt,claim,request,claim_sha,maximum,payload_len)=match lease.producer(){
   Producer::Direct{..}=>{let p:Disposal=decode(raw.as_bytes())?;(p.version,p.attempt_id,p.claim_json,p.request_sha256,p.admission_claim_sha256,p.maximum_bytes,p.payload_length)},
   Producer::Replay{..}=>{let p:ReplayDisposal=decode(raw.as_bytes())?;(p.version,p.attempt_id,p.claim_json,p.request_sha256,p.admission_claim_sha256,p.maximum_bytes,p.payload_length)},
  };
  ensure!(version==1&&attempt==lease.attempt_id()&&claim.as_bytes()==lease.original_claim()&&request==lease.producer_sha256()&&claim_sha==lease.claim_sha256()&&maximum==lease.maximum_bytes()as i64&&payload_len<=lease.maximum_bytes(),"Composite original disposal plan/capsule binding differs");Ok(())
 }
 pub(crate) async fn recheck_in(&self,conn:&mut sqlx::AnyConnection)->anyhow::Result<()> {
  self.held.fence_restored_in(&self.root,conn).await?;
  let roster=self.held.roster.as_ref().context("Composite retained roster absent")?;let q=roster.qualified().context("Composite original source absent")?;
  ensure!(q.prior_composite_native()?==self.basis.prior_native,"Composite original finalized native lineage changed");
  ensure!(q.prior_completed_evidence()?==self.basis.prior,"Composite original generation/old roles changed");
  let all:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id LIMIT 513").fetch_all(&mut *conn).await?;
  let expected=self.basis.prior.roles.scopes.iter().map(|s|s.original.reservation.operation_id.clone()).chain(self.basis.prior_native.iter().flatten().map(|s|s.original.reservation.operation_id.clone())).chain(self.native.iter().map(|h|h.basis.row.operation_id.clone())).collect::<std::collections::BTreeSet<_>>();
  ensure!(all.len()==expected.len()&&all.iter().all(|id|expected.contains(id)),"Composite complete reservation source changed");
  for native in &self.native {if let Some(absence)=&native.absence{absence.recheck_in(conn).await?;}native.lease.verify()?;native.records.verify()?;ensure!(super::capture::reservation_in(conn,native.lease.attempt_id()).await?==native.basis.row,"Composite exact native reservation changed");Self::bind_row(&native.lease,&native.basis.row)?;
   match &native.basis.class {NativeClass::DisposedDirect{..}|NativeClass::DisposedReplay{..}=>Self::strict_disposed_in(&native.lease,&native.basis.row,conn).await?,NativeClass::CommittedDirect{receipt_json}=>{let Producer::Direct{request}=native.lease.producer()else{anyhow::bail!("Composite producer changed")};let raw:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1 AND attempt_id=$2").bind(&request.operation_id).bind(native.lease.attempt_id()).fetch_one(&mut *conn).await?;ensure!(&raw==receipt_json,"Composite raw winning receipt changed");},NativeClass::Reserved{..}=>{}}
  }
  roster.fence_in(conn).await?;self.held.fence()?;Ok(())
 }
 #[cfg(test)] pub(crate) fn test_counts(&self)->(usize,usize,usize,usize,usize){let mut c=(self.basis.prior.roles.scopes.len(),0,0,0,0);for h in &self.native{match h.basis.class{NativeClass::Reserved{..}=>c.1+=1,NativeClass::CommittedDirect{..}=>c.2+=1,NativeClass::DisposedDirect{..}=>c.3+=1,NativeClass::DisposedReplay{..}=>c.4+=1}}c}
}

impl CompositeOriginalSource {
 /// Typed bytes/roles from the very held original source; this remains evidence
 /// only. No caller JSON or copied old capsule can enter this constructor.
 pub(crate) async fn original_native_scopes_in(&self,conn:&mut sqlx::AnyConnection)->anyhow::Result<Vec<crate::storage::restoration::composite::NativeScope>> {
  use crate::storage::restoration::composite as C;
  use std::os::unix::fs::MetadataExt;
  self.recheck_in(conn).await?;let mut result=self.basis.prior_native.iter().flatten().map(|s|s.original.clone()).collect::<Vec<_>>();
  for native in &self.native {
   let lease=&native.lease;let mut directories=Vec::new();let mut files=Vec::new();
   let mut add_dir=|role:C::DirectoryRole,f:&std::fs::File|->anyhow::Result<()>{let m=f.metadata()?;ensure!(m.is_dir()&&m.mode()&0o777==0o700&&m.uid()==unsafe{libc::geteuid()},"Composite source directory not private/original");let id=crate::storage::restoration::admission::Identity{device:m.dev(),inode:m.ino()};directories.push(C::SourceDirectory{role,original:id.clone(),captured:id});Ok(())};
   add_dir(C::DirectoryRole::DataRoot,&lease.0.root)?;add_dir(C::DirectoryRole::AdmissionNamespace,&lease.0.namespace)?;add_dir(C::DirectoryRole::AdmissionCapsule,&lease.0.directory)?;
   let mut add_file=|role:C::FileRole,f:&std::fs::File|->anyhow::Result<()>{let m=f.metadata()?;ensure!(m.is_file()&&m.mode()&0o777==0o600&&m.uid()==unsafe{libc::geteuid()}&&m.nlink()==1&&m.len()<=8*1024*1024,"Composite source metadata not private/exclusive/bounded");let id=crate::storage::restoration::admission::Identity{device:m.dev(),inode:m.ino()};let sha=crate::storage::attachment_upload::hash(f,m.len())?;files.push(C::SourceFile{role,original:id.clone(),captured:id,length:m.len(),sha256:sha});Ok(())};
   for(name,(f,_,_,_))in &native.records.records {
    let name=match name.as_str(){"claim.json"=>C::CapsuleRecord::Claim,"stage-origin.json"=>C::CapsuleRecord::StageOrigin,"disposal-intent.json"=>C::CapsuleRecord::DisposalIntent,"disposal-tombstone.json"=>C::CapsuleRecord::DisposalTombstone,"replay-blob-origin.json"=>C::CapsuleRecord::ReplayBlobOrigin,"replay-rewrite-plan.json"=>C::CapsuleRecord::ReplayRewritePlan,"replay-rewrite-origin.json"=>C::CapsuleRecord::ReplayRewriteOrigin,"replay-disposal-intent.json"=>C::CapsuleRecord::ReplayDisposalIntent,"replay-disposal-tombstone.json"=>C::CapsuleRecord::ReplayDisposalTombstone,_=>anyhow::bail!("Composite native capsule record role unsupported")};add_file(C::FileRole::CapsuleRecord{name},f)?;
   }
   match lease.producer(){
    Producer::Direct{..}=>{if native.basis.row.state!="disposed"{lease.plan_direct()?;}if let Some((ns,dir))=lease.stage_directory()?{let raw=lease.stage_origin()?.context("Composite Direct stage origin absent")?;lease.verify_origin(&ns,&dir,&raw)?;add_dir(C::DirectoryRole::DirectStageNamespace,&ns)?;add_dir(C::DirectoryRole::DirectStageDirectory,&dir)?;
     for(name,role)in[("reservation.json",C::StageRecord::Reservation),("request.json",C::StageRecord::Request),("seal.json",C::StageRecord::Seal),("intent.json",C::StageRecord::Intent),("install-proof.json",C::StageRecord::InstallProof)]{if let Some(f)=optional_file(&dir,name)?{add_file(C::FileRole::DirectStageRecord{name:role},&f)?;}}
    }},
    Producer::Replay{..}=>{if let Some(raw)=lease.replay_origin()?{let(o,parent,_)=lease.verify_replay_origin(&raw)?;let relative=o.path.rsplit_once('/').context("Composite Replay parent absent")?.0.to_owned();add_dir(C::DirectoryRole::ReplayBlobParent{path:relative},&parent)?;}
     // A separate typed canonical-rewrite record constructor is required before
     // emitting rewrite-bearing disposed scopes. Never omit those files.
     ensure!(lease.replay_plan()?.is_none(),"Composite Replay rewrite roles require explicit typed capture");
    },
   }
   let class=match &native.basis.class {NativeClass::Reserved{supported_original_scope}=>C::NativeClass::Reserved{original_scope:supported_original_scope.clone()},NativeClass::CommittedDirect{receipt_json}=>C::NativeClass::CommittedDirect{receipt_json:receipt_json.clone()},NativeClass::DisposedDirect{original_intent_json}=>C::NativeClass::DisposedDirect{intent_json:original_intent_json.clone()},NativeClass::DisposedReplay{original_intent_json}=>C::NativeClass::DisposedReplay{intent_json:original_intent_json.clone()}};
   let scope=C::NativeScope{reservation:native.basis.row.clone(),claim_json:native.basis.claim_json.clone(),class,directories,files};scope.validate_shape()?;let mut raw_records=std::collections::BTreeMap::new();for binding in &scope.files{let relative=binding.role.relative(lease.attempt_id())?;let f=scoped_file(&lease.0.root,&relative)?;use std::os::unix::fs::FileExt;let mut raw=vec![0;binding.length as usize];f.read_exact_at(&mut raw,0)?;raw_records.insert(binding.role.clone(),raw);}validate_composite_native_records(&scope,&raw_records)?;
   ensure!(scope.directories.iter().find(|d|d.role==C::DirectoryRole::DataRoot).map(|d|&d.captured)==Some(&self.basis.prior.source_root),"Composite original source root association differs");result.push(scope);
  }
  self.recheck_in(conn).await?;Ok(result)
 }
}

impl CompositeOriginalSource {
 pub(crate) async fn capture_evidence_in(&self,conn:&mut sqlx::AnyConnection)->anyhow::Result<crate::storage::restoration::composite::CompositeCaptureEvidence>{
  self.recheck_in(conn).await?;
  let evidence=crate::storage::restoration::composite::CompositeCaptureEvidence{version:self.basis.version,prior:self.basis.prior.clone(),native:self.original_native_scopes_in(conn).await?,prior_native:self.basis.prior_native.clone()};
  evidence.validate_shape()?;
  let expected=self.basis.prior.roles.scopes.iter().map(|s|s.original.reservation.operation_id.clone()).chain(self.basis.prior_native.iter().flatten().map(|s|s.original.reservation.operation_id.clone())).chain(self.native.iter().map(|s|s.basis.row.operation_id.clone())).collect();
  // The roster's original custody fence already requires the exact full namespace.
  evidence.check_complete_reservation_census_in(conn,&expected).await?;
  self.recheck_in(conn).await?;Ok(evidence)
 }
}

// Read-only typed snapshot references, exposed through the accounting child so
// the private coordinator remains private to committed-erasure implementation.
pub(crate) async fn check_disposed_snapshot_references_in(root:&Path,conn:&mut sqlx::AnyConnection,raw:&str,replay:bool)->anyhow::Result<()>{
 coordinator::check_disposed_snapshot_references_in(root,conn,raw,replay).await
}

// Separate historical physical-only purpose; default current-reference capture stays strict.
#[path="attachment_historical_source.rs"]
pub(crate) mod historical;
