//! Explicit source-held physical preservation. No current-reference or cleanup authority.
use super::*;
use crate::storage::restoration::{historical::{ApplicationProjection,HistoricalCompositePreservationEvidence,HistoricalReferenceSemantics,PURPOSE},linux::{HistoricalCompletedPhysical,PriorCompletedCaptureEvidence}};
pub(crate) struct HistoricalCompositeSource {
 root:PathBuf,_guard:std::sync::Arc<crate::storage::native_family_fence::Creation>,
 physical:HistoricalCompletedPhysical,prior:PriorCompletedCaptureEvidence,
 native:Vec<HeldNativeBasis>,projection:String,capsules:std::collections::BTreeSet<String>,stages:std::collections::BTreeSet<String>,
}
fn namespace_names(root:&Path,namespace:&str)->anyhow::Result<std::collections::BTreeSet<String>>{
 let root=stage::root(root)?;let dir=match stage::open(&root,namespace,libc::O_RDONLY|libc::O_DIRECTORY){Ok(f)=>f,Err(e)if e.kind()==std::io::ErrorKind::NotFound=>return Ok(Default::default()),Err(e)=>return Err(e.into())};use std::os::fd::AsRawFd;let mut names=std::collections::BTreeSet::new();
 for e in std::fs::read_dir(format!("/proc/self/fd/{}",dir.as_raw_fd()))?{ensure!(names.len()<512,"Historical namespace exceeds bounded v1 census");let name=e?.file_name().into_string().map_err(|_|anyhow::anyhow!("Historical namespace nonUTF8"))?;ensure!(uuid::Uuid::parse_str(&name).is_ok_and(|u|u.to_string()==name),"Historical namespace malformed attempt");names.insert(name);}Ok(names)
}
impl HistoricalCompositeSource {
 pub(crate) async fn hold_in(root:&Path,tx:&mut sqlx::Transaction<'_,sqlx::Any>)->anyhow::Result<Self>{
  let guard=std::sync::Arc::new(crate::storage::native_family_fence::Creation::exclusive(root)?);
  let physical=HistoricalCompletedPhysical::hold_in(root,&mut **tx,guard.clone()).await?;let prior=physical.evidence()?;let old=physical.completed_attempts();let capsules=namespace_names(root,NS)?;
  ensure!(old.is_subset(&capsules),"Historical original completed capsule missing");
  let all:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id LIMIT 513").fetch_all(&mut **tx).await?;
  ensure!(all.len()<=512&&all.iter().cloned().collect::<std::collections::BTreeSet<_>>()==capsules,"Historical complete SQL/capsule inventory differs");
  let mut native=Vec::new();
  for attempt in capsules.iter().filter(|id|!old.contains(*id)) {
   let lease=WriterLease::open_idle(root,attempt)?;lease.verify()?;lease.matches_root(root)?;ensure!(matches!(lease.producer(),Producer::Direct{..}),"Historical v1 excludes Replay original scope");
   let row=super::super::capture::reservation_in(&mut **tx,attempt).await?;CompositeOriginalSource::bind_row(&lease,&row)?;
   let class=match row.state.as_str(){
    "reserved"=>NativeClass::Reserved{supported_original_scope:Some(lease.capture_supported_scope(tx).await?.context("Historical v1 requires fully captured Direct partial stage")?)},
    "committed"=>{let Producer::Direct{request}=lease.producer()else{unreachable!()};let raw:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1 AND attempt_id=$2").bind(&request.operation_id).bind(attempt).fetch_one(&mut **tx).await?;let receipt:Receipt=serde_json::from_str(&raw)?;ensure!(receipt.intent.request==*request&&receipt.intent.attempt_id==*attempt&&row.committed_asset_id.as_deref()==Some(receipt.intent.asset_id.as_str())&&row.committed_bytes==Some(receipt.intent.byte_length),"Historical original committed winning receipt/row differs");ensure!(crate::storage::attachment_upload::verify(root,&receipt,false)?,"Historical original committed bytes unavailable");crate::storage::attachment_versions::validate_composite_preserved_singleton_in(&mut **tx,&receipt).await.map_err(anyhow::Error::msg)?;NativeClass::CommittedDirect{receipt_json:raw}},
    "disposed"=>{let raw=row.disposition_json.clone().context("Historical original disposed raw plan absent")?;ensure!(row.disposition_sha256.as_deref()==Some(digest(raw.as_bytes()).as_str()),"Historical original disposed digest differs");NativeClass::DisposedDirect{original_intent_json:raw}},
    _=>anyhow::bail!("Historical native unsupported state")};
   let claim_json=std::str::from_utf8(lease.original_claim())?.to_owned();let records=lease.capture_record_fence()?;native.push(HeldNativeBasis{absence:None,lease,records,basis:NativeBasis{row,claim_json,class}});
  }
  ensure!(!native.is_empty(),"Historical v1 requires genuine NEW Direct union");
  let mut expected_stages=old.clone();for n in &native {if n.lease.stage_directory()?.is_some(){expected_stages.insert(n.lease.attempt_id().to_owned());}}
  let stages=namespace_names(root,stage::NAMESPACE)?;ensure!(stages==expected_stages,"Historical full stage custody differs");
  let projection=physical.application_projection_in(&mut **tx).await?;
  let held=Self{root:root.to_owned(),_guard:guard,physical,prior,native,projection,capsules,stages};held.recheck_in(&mut **tx).await?;Ok(held)
 }
 pub(crate) fn reference_semantics(&self)->HistoricalReferenceSemantics{HistoricalReferenceSemantics::NotEstablished}
 pub(crate) async fn recheck_in(&self,conn:&mut sqlx::AnyConnection)->anyhow::Result<()> {
  self.physical.recheck_in(conn).await?;ensure!(self.physical.evidence()?==self.prior,"Historical original completed lineage changed");
  ensure!(namespace_names(&self.root,NS)?==self.capsules&&namespace_names(&self.root,stage::NAMESPACE)?==self.stages,"Historical complete native namespace changed");
  let all:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id LIMIT 513").fetch_all(&mut *conn).await?;ensure!(all.len()==self.capsules.len()&&all.iter().all(|id|self.capsules.contains(id)),"Historical complete reservation inventory changed");
  for n in &self.native {n.lease.verify()?;n.lease.matches_root(&self.root)?;n.records.verify()?;ensure!(super::super::capture::reservation_in(conn,n.lease.attempt_id()).await?==n.basis.row,"Historical full native SQL row changed");CompositeOriginalSource::bind_row(&n.lease,&n.basis.row)?;if let NativeClass::Reserved{supported_original_scope:Some(scope)}=&n.basis.class{n.lease.verify_captured_scope(scope)?;}if let NativeClass::CommittedDirect{receipt_json}=&n.basis.class{let receipt:Receipt=serde_json::from_str(receipt_json)?;ensure!(crate::storage::attachment_upload::verify(&self.root,&receipt,false)?,"Historical original committed bytes changed");crate::storage::attachment_versions::validate_composite_preserved_singleton_in(conn,&receipt).await.map_err(anyhow::Error::msg)?;}}
  let scopes=self.native_scopes()?;
  for scope in &scopes{let mut raws=std::collections::BTreeMap::new();for b in &scope.files{let n=self.native.iter().find(|n|n.lease.attempt_id()==scope.reservation.operation_id).context("Historical original scope not held")?;let f=scoped_file(&n.lease.0.root,&b.role.relative(n.lease.attempt_id())?)?;use std::os::unix::fs::FileExt;let mut raw=vec![0;b.length as usize];f.read_exact_at(&mut raw,0)?;raws.insert(b.role.clone(),raw);}let record=validate_composite_native_records(scope,&raws)?;self.physical.check_native_component_in(&record,conn).await?;}
  ensure!(self.physical.application_projection_in(conn).await?==self.projection,"Historical full application/opaque projection changed");self.physical.recheck_in(conn).await?;Ok(())
 }
 fn native_scopes(&self)->anyhow::Result<Vec<crate::storage::restoration::composite::NativeScope>>{
  use crate::storage::restoration::composite as C;use std::os::unix::fs::MetadataExt;let mut result=Vec::new();
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
   ensure!(scope.directories.iter().find(|d|d.role==C::DirectoryRole::DataRoot).map(|d|&d.captured)==Some(&self.prior.source_root),"Composite original source root association differs");result.push(scope);
  }

  Ok(result)
 }
 pub(crate) async fn capture_evidence_in(&self,conn:&mut sqlx::AnyConnection)->anyhow::Result<HistoricalCompositePreservationEvidence>{
  self.recheck_in(conn).await?;let e=HistoricalCompositePreservationEvidence{version:1,purpose:PURPOSE.into(),prior:self.prior.clone(),native:self.native_scopes()?,application_projection:ApplicationProjection{version:1,sha256:self.projection.clone(),reference_semantics:self.reference_semantics()}};e.validate_shape()?;self.recheck_in(conn).await?;Ok(e)
 }
}
