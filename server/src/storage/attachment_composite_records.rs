// Pure typed cold-record validation. This does not construct any native lease,
// runtime owner, serving permission, deletion action or released-charge token.
pub(crate) struct CompositeRecordEvidence {
 scope:crate::storage::restoration::composite::NativeScope,
 kind:CompositeRecordKind,
}
enum CompositeRecordKind {Reserved,Committed(super::Receipt),DisposedDirect(Disposal),DisposedReplay(ReplayDisposal,Option<ReplayBlobOrigin>)}
pub(crate) fn validate_composite_native_records(scope:&crate::storage::restoration::composite::NativeScope,raws:&std::collections::BTreeMap<crate::storage::restoration::composite::FileRole,Vec<u8>>)->io::Result<CompositeRecordEvidence>{
 use crate::storage::restoration::composite::{DirectoryRole as D,FileRole as F,CapsuleRecord as C,StageRecord as S,NativeClass as K};
 scope.validate_shape()?;let attempt=&scope.reservation.operation_id;
 let claim:Claim=decode(scope.claim_json.as_bytes())?;validate(&claim.producer,claim.maximum_bytes)?;
 let claim_sha=digest(scope.claim_json.as_bytes());let producer_sha=digest(&json(&claim.producer)?);
 if claim.version!=1||claim.attempt_id!=*attempt||claim.producer_sha256!=producer_sha||producer_sha!=scope.reservation.request_sha256||claim_sha!=scope.reservation.admission_claim_sha256||claim.maximum_bytes as i64!=scope.reservation.maximum_bytes{return Err(fail("Composite original producer/claim differs"));}
 let get=|role:F|->io::Result<&[u8]>{raws.get(&role).map(Vec::as_slice).ok_or_else(||fail("Composite typed original record absent"))};
 if raws.keys().cloned().collect::<std::collections::BTreeSet<_>>()!=scope.files.iter().map(|b|b.role.clone()).collect(){return Err(fail("Composite record inventory omitted or substituted"));}
 for b in &scope.files{let raw=get(b.role.clone())?;if raw.len()as u64!=b.length||digest(raw)!=b.sha256{return Err(fail("Composite original record length/hash differs"));}if let F::CapsuleRecord{..}=&b.role{let relative=b.role.relative(attempt)?;let name=relative.rsplit('/').next().ok_or_else(||fail("Composite capsule role absent"))?;validate_preserved_capsule_record(name,raw,scope.claim_json.as_bytes())?;}}
 let directory=|role:&D|->io::Result<Identity>{let b=scope.directories.iter().find(|b|&b.role==role).ok_or_else(||fail("Composite original directory role absent"))?;Ok(Identity{device:b.original.device,inode:b.original.inode})};
 if get(F::CapsuleRecord{name:C::Claim})?!=scope.claim_json.as_bytes()||directory(&D::DataRoot)?!=claim.root||directory(&D::AdmissionNamespace)?!=claim.namespace||directory(&D::AdmissionCapsule)?!=claim.capsule{return Err(fail("Composite original claim/root/capsule roles differ"));}
 let mut expected_dirs=std::collections::BTreeSet::from([D::DataRoot,D::AdmissionNamespace,D::AdmissionCapsule]);let mut expected_files=std::collections::BTreeSet::from([F::CapsuleRecord{name:C::Claim}]);
 let stage=match &claim.producer {
  Producer::Direct{request}=>if let Some(raw)=raws.get(&F::CapsuleRecord{name:C::StageOrigin}){
   let origin:StageOrigin=decode(raw)?;if origin.version!=1||origin.claim_sha256!=claim_sha||directory(&D::DirectStageNamespace)?!=origin.namespace||directory(&D::DirectStageDirectory)?!=origin.directory{return Err(fail("Composite Direct original stage roles differ"));}
   let rr=get(F::DirectStageRecord{name:S::Reservation})?;let rq=get(F::DirectStageRecord{name:S::Request})?;let reservation:stage::Reservation=decode(rr)?;
   if digest(rr)!=origin.reservation_sha256||digest(rq)!=origin.request_sha256||rq!=json(request)?.as_slice()||reservation.version!=1||reservation.operation_id!=*attempt||reservation.actor_id!=request.actor_id||reservation.space_id!=request.space_id||reservation.document_id!=request.document_id||reservation.original_name!=request.original_name||reservation.mime_type!=request.mime_type||reservation.maximum_bytes!=claim.maximum_bytes{return Err(fail("Composite Direct original request/reservation differs"));}
   expected_dirs.extend([D::DirectStageNamespace,D::DirectStageDirectory]);expected_files.extend([F::CapsuleRecord{name:C::StageOrigin},F::DirectStageRecord{name:S::Reservation},F::DirectStageRecord{name:S::Request}]);
   for name in [S::Seal,S::Intent,S::InstallProof]{let role=F::DirectStageRecord{name};if raws.contains_key(&role){expected_files.insert(role);}}
   if let Some(raw)=raws.get(&F::DirectStageRecord{name:S::Seal}){let seal:stage::Seal=decode(raw)?;if seal.version!=1||seal.reservation_sha256!=origin.reservation_sha256||seal.length>claim.maximum_bytes||!valid_hash(&seal.payload_sha256){return Err(fail("Composite Direct original seal differs"));}}
   let intent=raws.get(&F::DirectStageRecord{name:S::Intent});let proof=raws.get(&F::DirectStageRecord{name:S::InstallProof});match(intent,proof){(Some(raw),Some(proof))=>{let intent:super::Intent=decode(raw)?;super::validate(&intent)?;let proof:super::InstallProof=decode(proof)?;if intent.attempt_id!=*attempt||intent.request!=*request||proof.version!=1||proof.intent_sha256!=digest(raw)||(proof.original_device,proof.original_inode)!=(origin.payload.device,origin.payload.inode){return Err(fail("Composite Direct original intent/install proof differs"));}},(None,None)=>{},_=>return Err(fail("Composite Direct incomplete prepared proof unsupported"))};Some(origin)
  }else{None},
  Producer::Replay{..}=>None,
 };
 let kind=match &scope.class{
  K::Reserved{original_scope}=>{if let Some(original)=original_scope {if original.original_claim_json!=scope.claim_json{return Err(fail("Composite pending original role claim differs"));}}CompositeRecordKind::Reserved},
  K::CommittedDirect{receipt_json}=>{let Producer::Direct{request}=&claim.producer else{return Err(fail("Composite committed Direct producer differs"))};let receipt:super::Receipt=decode(receipt_json.as_bytes())?;super::validate(&receipt.intent)?;let raw=get(F::DirectStageRecord{name:S::Intent})?;let proof=get(F::DirectStageRecord{name:S::InstallProof})?;
   if receipt.intent.request!=*request||receipt.intent.attempt_id!=*attempt||receipt.intent_sha256!=digest(raw)||receipt.install_proof_sha256!=digest(proof)||json(&receipt.intent)?.as_slice()!=raw||scope.reservation.committed_asset_id.as_deref()!=Some(receipt.intent.asset_id.as_str())||scope.reservation.committed_bytes!=Some(receipt.intent.byte_length)||stage.is_none(){return Err(fail("Composite original committed raw receipt differs"));}CompositeRecordKind::Committed(receipt)},
  K::DisposedDirect{intent_json}=>{if !matches!(claim.producer,Producer::Direct{..}){return Err(fail("Composite disposed Direct producer differs"));}let plan:Disposal=decode(intent_json.as_bytes())?;
   if plan.version!=1||plan.attempt_id!=*attempt||plan.claim_json!=scope.claim_json||plan.admission_claim_sha256!=claim_sha||plan.request_sha256!=producer_sha||plan.maximum_bytes!=claim.maximum_bytes as i64||plan.payload_length>claim.maximum_bytes||get(F::CapsuleRecord{name:C::DisposalIntent})?!=intent_json.as_bytes()||get(F::CapsuleRecord{name:C::DisposalTombstone})?!=intent_json.as_bytes(){return Err(fail("Composite original Direct disposition binding differs"));}
   match(&stage,&plan.stage_origin_json){(Some(origin),Some(raw))=>{if get(F::CapsuleRecord{name:C::StageOrigin})?!=raw.as_bytes()||plan.payload.as_ref()!=Some(&origin.payload)||plan.payload_sha256.as_ref().is_none_or(|sha|!valid_hash(sha)){return Err(fail("Composite disposed Direct original payload evidence differs"));}},(None,None)=>{if plan.payload.is_some()||plan.payload_length!=0||plan.payload_sha256.is_some()||!plan.envelopes.is_empty()||plan.installed_path.is_some(){return Err(fail("Composite Direct before-stage disposition differs"));}},_=>return Err(fail("Composite disposed Direct stage origin omitted"))}
   for(name,role)in[("reservation.json",S::Reservation),("request.json",S::Request),("seal.json",S::Seal),("intent.json",S::Intent),("install-proof.json",S::InstallProof)]{if raws.get(&F::DirectStageRecord{name:role}).map(Vec::as_slice)!=plan.envelopes.get(name).map(String::as_bytes){return Err(fail("Composite disposed Direct original stage envelope changed"));}}
   if let Some(path)=&plan.installed_path{let original:super::Intent=decode(plan.envelopes.get("intent.json").ok_or_else(||fail("Composite installed disposition lacks original intent"))?.as_bytes())?;if &original.storage_path!=path{return Err(fail("Composite disposed Direct canonical path differs"));}}
   expected_files.extend([F::CapsuleRecord{name:C::DisposalIntent},F::CapsuleRecord{name:C::DisposalTombstone}]);CompositeRecordKind::DisposedDirect(plan)},
  K::DisposedReplay{intent_json}=>{let Producer::Replay{binary_sha256,byte_length,..}=&claim.producer else{return Err(fail("Composite disposed Replay producer differs"))};let plan:ReplayDisposal=decode(intent_json.as_bytes())?;
   if plan.version!=1||plan.attempt_id!=*attempt||plan.claim_json!=scope.claim_json||plan.admission_claim_sha256!=claim_sha||plan.request_sha256!=producer_sha||plan.maximum_bytes!=claim.maximum_bytes as i64||plan.payload_length>claim.maximum_bytes||get(F::CapsuleRecord{name:C::ReplayDisposalIntent})?!=intent_json.as_bytes()||get(F::CapsuleRecord{name:C::ReplayDisposalTombstone})?!=intent_json.as_bytes(){return Err(fail("Composite original Replay disposition binding differs"));}
   if plan.rewrite_plan_json.is_some()||plan.rewrite_manifest_json.is_some()||plan.rewrite_install_proof_json.is_some()||plan.rewrite_incomplete_disposition_json.is_some(){return Err(fail("Composite disposed Replay rewrite needs complete distinct typed proof"));}
   let origin=if let Some(raw)=&plan.origin_json {if get(F::CapsuleRecord{name:C::ReplayBlobOrigin})?!=raw.as_bytes(){return Err(fail("Composite disposed Replay origin record differs"));}let o:ReplayBlobOrigin=decode(raw.as_bytes())?;let parts=o.path.split('/').collect::<Vec<_>>();let Producer::Replay{document_id,..}=&claim.producer else{unreachable!()};let parent=o.path.rsplit_once('/').ok_or_else(||fail("Composite Replay origin parent absent"))?.0.to_owned();
    if o.version!=1||o.claim_sha256!=claim_sha||o.expected_length!=*byte_length||o.expected_sha256!=*binary_sha256||parts.len()!=5||parts[0]!="spaces"||parts[2]!="attachments"||parts[3]!=document_id||directory(&D::ReplayBlobParent{path:parent.clone()})?!=o.parent||plan.payload.as_ref()!=Some(&o.payload)||plan.payload_sha256.as_ref().is_none_or(|sha|!valid_hash(sha)){return Err(fail("Composite disposed Replay original blob proof differs"));}expected_dirs.insert(D::ReplayBlobParent{path:parent});expected_files.insert(F::CapsuleRecord{name:C::ReplayBlobOrigin});Some(o)
   }else{if plan.payload.is_some()||plan.payload_length!=0||plan.payload_sha256.is_some(){return Err(fail("Composite Replay before-blob disposition differs"));}None};
   expected_files.extend([F::CapsuleRecord{name:C::ReplayDisposalIntent},F::CapsuleRecord{name:C::ReplayDisposalTombstone}]);CompositeRecordKind::DisposedReplay(plan,origin)},
 };
 if matches!(&claim.producer,Producer::Replay{..})&&matches!(&scope.class,K::Reserved{..}){return Err(fail("Composite pending Replay needs its complete typed original role consumer"));}
 if scope.directories.iter().map(|b|b.role.clone()).collect::<std::collections::BTreeSet<_>>()!=expected_dirs||raws.keys().cloned().collect::<std::collections::BTreeSet<_>>()!=expected_files{return Err(fail("Composite original native record/directory census omitted or substituted"));}
 Ok(CompositeRecordEvidence{scope:scope.clone(),kind})
}
impl CompositeRecordEvidence {
 pub(crate) async fn validate_snapshot_row_and_paths_in(&self,conn:&mut sqlx::AnyConnection,inventory:&std::collections::BTreeSet<String>)->anyhow::Result<()> {
  let row=committed_erasure::accounting::capture::reservation_in(conn,&self.scope.reservation.operation_id).await?;anyhow::ensure!(row==self.scope.reservation,"Composite snapshot complete reservation differs");
  match &self.kind {
   CompositeRecordKind::Reserved=>{},
   CompositeRecordKind::Committed(receipt)=>{let raw:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1 AND attempt_id=$2").bind(&receipt.intent.request.operation_id).bind(&receipt.intent.attempt_id).fetch_one(&mut *conn).await?;let crate::storage::restoration::composite::NativeClass::CommittedDirect{receipt_json}=&self.scope.class else{anyhow::bail!("Composite committed source class changed")};anyhow::ensure!(&raw==receipt_json&&inventory.contains(&receipt.intent.storage_path),"Composite preserved winning receipt/path differs");},
   CompositeRecordKind::DisposedDirect(plan)=>{for query in ["SELECT COUNT(*) FROM attachment_upload_receipts WHERE attempt_id=$1","SELECT COUNT(*) FROM attachment_version_records WHERE upload_attempt_id=$1"]{let count:i64=sqlx::query_scalar(query).bind(&plan.attempt_id).fetch_one(&mut *conn).await?;anyhow::ensure!(count==0,"Composite disposed attempt has winner");}anyhow::ensure!(!inventory.contains(&format!(".attachment-upload-journal/{}/payload.part",plan.attempt_id))&&plan.installed_path.as_ref().is_none_or(|p|!inventory.contains(p)),"Composite disposed Direct payload resurrected");},
   CompositeRecordKind::DisposedReplay(plan,origin)=>{for query in ["SELECT COUNT(*) FROM attachment_upload_receipts WHERE attempt_id=$1","SELECT COUNT(*) FROM attachment_version_records WHERE upload_attempt_id=$1"]{let count:i64=sqlx::query_scalar(query).bind(&plan.attempt_id).fetch_one(&mut *conn).await?;anyhow::ensure!(count==0,"Composite disposed Replay attempt has winner");}if let Some(origin)=origin{anyhow::ensure!(!inventory.contains(&origin.path),"Composite disposed Replay blob resurrected");}},
  }Ok(())
 }
}

impl CompositeRecordEvidence {
 /// Read-only complete copied inventory absence fence. A caller-supplied list of
 /// paths/flags cannot substitute the held qualified bundle and original custody.
 /// Still not a finalized owner, and never releases a charge by itself.
 pub(crate) async fn validate_disposed_bundle_in(&self,bundle:&crate::storage::backup_inventory::ValidatedBundle,conn:&mut sqlx::AnyConnection)->anyhow::Result<()> {
  bundle.fence()?;anyhow::ensure!(bundle.is_qualified_completed_capture(),"Composite disposed snapshot requires exact53 qualified profile");
  let(raw,replay,payload,length,sha)=match &self.kind {
   CompositeRecordKind::DisposedDirect(p)=>(self.scope.reservation.disposition_json.as_deref().ok_or_else(||fail("Composite original Direct raw absence absent"))?,false,p.payload.as_ref(),p.payload_length,p.payload_sha256.as_deref()),
   CompositeRecordKind::DisposedReplay(p,_)=>(self.scope.reservation.disposition_json.as_deref().ok_or_else(||fail("Composite original Replay raw absence absent"))?,true,p.payload.as_ref(),p.payload_length,p.payload_sha256.as_deref()),
   _=>anyhow::bail!("Composite disposed snapshot validator cannot classify another native class"),
  };
  let paths=bundle.manifest.files.0.keys().cloned().collect();self.validate_snapshot_row_and_paths_in(conn,&paths).await?;
  let designation=bundle.metadata_designation()?;
  for(path,bytes)in &bundle.manifest.files.0 {
   if path=="starlane-notes.db"{continue;} // Derived snapshot, not captured payload.
   let source=bundle.manifest.source_custody.files.0.get(path).ok_or_else(||fail("Composite disposed all-file custody omitted"))?;
   if let Some(payload)=payload {anyhow::ensure!((source.device,source.inode)!=(payload.device,payload.inode),"Composite disposed original payload inode resurrected under alias");}
   let held=bundle.open_owned(path)?;
   let informational=designation.as_ref().map(|d|d.matches_held_marker(path,&held)).transpose()?.unwrap_or(false);
   if !informational {anyhow::ensure!(!sha.is_some_and(|sha|bytes.size_bytes==length&&bytes.sha256==sha),"Composite disposed independent payload bytes resurrected under alias");}
  }
  committed_erasure::accounting::composite::check_disposed_snapshot_references_in(&bundle.data_path(),conn,raw,replay).await?;
  if let Some(d)=&designation{d.fence()?;}bundle.fence()?;Ok(())
 }
}

impl CompositeRecordEvidence {
 pub(crate) async fn validate_committed_bundle_in(&self,bundle:&crate::storage::backup_inventory::ValidatedBundle,conn:&mut sqlx::AnyConnection)->anyhow::Result<()> {
  bundle.fence()?;let CompositeRecordKind::Committed(receipt)=&self.kind else{anyhow::bail!("Composite committed validator cannot classify another class")};
  let paths=bundle.manifest.files.0.keys().cloned().collect();self.validate_snapshot_row_and_paths_in(conn,&paths).await?;
  let origin_binding=self.scope.files.iter().find(|b|b.role==crate::storage::restoration::composite::FileRole::CapsuleRecord{name:crate::storage::restoration::composite::CapsuleRecord::StageOrigin}).ok_or_else(||fail("Composite committed original stage role absent"))?;
  let origin_path=origin_binding.role.relative(&receipt.intent.attempt_id)?;let mut file=bundle.open_owned(&origin_path)?;let mut raw=Vec::new();use std::io::Read;file.read_to_end(&mut raw)?;let origin:StageOrigin=decode(&raw)?;
  for path in [&receipt.intent.storage_path,&format!(".attachment-upload-journal/{}/payload.part",receipt.intent.attempt_id)] {
   let bytes=bundle.manifest.files.0.get(path).ok_or_else(||fail("Composite preserved original payload path absent"))?;let id=bundle.manifest.source_custody.files.0.get(path).ok_or_else(||fail("Composite preserved original payload identity absent"))?;
   let expected=if bundle.manifest.composite_admission_capture.as_ref().is_some_and(|e|e.prior_scope(&self.scope).is_some()){
    let proof=self.scope.files.iter().find(|b|b.role==crate::storage::restoration::composite::FileRole::DirectStageRecord{name:crate::storage::restoration::composite::StageRecord::InstallProof}).ok_or_else(||fail("Repeated committed original proof role absent"))?;
    crate::storage::restoration::linux::composite_prior_committed_identity_in(bundle,conn,&receipt.intent.attempt_id,path,(origin.payload.device,origin.payload.inode),receipt.intent.byte_length as u64,&receipt.intent.binary_sha256,&proof.sha256).await?
   }else{(origin.payload.device,origin.payload.inode)};
   anyhow::ensure!((id.device,id.inode)==expected&&bytes.size_bytes==receipt.intent.byte_length as u64&&bytes.sha256==receipt.intent.binary_sha256,"Composite preserved payload custody/bytes differ");
  }
  crate::storage::attachment_versions::validate_composite_preserved_singleton_in(conn,receipt).await.map_err(anyhow::Error::msg)?;bundle.fence()?;Ok(())
 }
}

impl CompositeRecordEvidence {
 // Only callers retaining opaque actual Active + full target census may use
 // these observations as a component of finalized accounting. No grant here.
 pub(crate) fn exclude_disposed_payload_file(&self,relative:&str,file:&std::fs::File,designation:Option<&crate::storage::data_root_designation::Designation>)->io::Result<()> {
  use std::os::unix::fs::MetadataExt;
  let(payload,length,sha)=match &self.kind {CompositeRecordKind::DisposedDirect(p)=>(p.payload.as_ref(),p.payload_length,p.payload_sha256.as_deref()),CompositeRecordKind::DisposedReplay(p,_)=>(p.payload.as_ref(),p.payload_length,p.payload_sha256.as_deref()),_=>return Ok(())};
  let m=file.metadata()?;if payload.is_some_and(|p|(m.dev(),m.ino())==(p.device,p.inode)){return Err(fail("Composite finalized original payload inode resurrected"));}
  let informational=designation.map(|d|d.matches_held_marker(relative,file)).transpose()?.unwrap_or(false);
  if !informational&&m.len()==length {if let Some(sha)=sha {if super::hash(file,length)?==sha{return Err(fail("Composite finalized independent payload copy resurrected"));}}}Ok(())
 }
 pub(crate) async fn validate_finalized_native_in(&self,root:&std::path::Path,conn:&mut sqlx::AnyConnection,inventory:&std::collections::BTreeSet<String>)->anyhow::Result<()> {
  self.validate_snapshot_row_and_paths_in(conn,inventory).await?;
  match &self.kind {
   CompositeRecordKind::Reserved=>{for query in ["SELECT COUNT(*) FROM attachment_upload_receipts WHERE attempt_id=$1","SELECT COUNT(*) FROM attachment_version_records WHERE upload_attempt_id=$1"]{let n:i64=sqlx::query_scalar(query).bind(&self.scope.reservation.operation_id).fetch_one(&mut *conn).await?;anyhow::ensure!(n==0,"Composite finalized pending attempt has winner");}},
   CompositeRecordKind::Committed(receipt)=>{super::verify(root,receipt,false)?;crate::storage::attachment_versions::validate_composite_preserved_singleton_in(conn,receipt).await.map_err(anyhow::Error::msg)?;},
   CompositeRecordKind::DisposedDirect(_)=>committed_erasure::accounting::composite::check_disposed_snapshot_references_in(root,conn,self.scope.reservation.disposition_json.as_deref().ok_or_else(||fail("Composite original Direct plan absent"))?,false).await?,
   CompositeRecordKind::DisposedReplay(_,_)=>committed_erasure::accounting::composite::check_disposed_snapshot_references_in(root,conn,self.scope.reservation.disposition_json.as_deref().ok_or_else(||fail("Composite original Replay plan absent"))?,true).await?,
  }Ok(())
 }
}

// Pure observations used only under the PG consumer's opaque held complete
// snapshot. These checks do not certify a caller-provided inventory or grant
// ownership, serving, cleanup or released accounting by themselves.
impl CompositeRecordEvidence {
 pub(crate) fn check_captured_disposed_file(&self,path:&str,source_device:u64,source_inode:u64,file:&std::fs::File,designation:Option<&crate::storage::data_root_designation::Designation>)->io::Result<()> {
  let payload=match &self.kind{CompositeRecordKind::DisposedDirect(p)=>p.payload.as_ref(),CompositeRecordKind::DisposedReplay(p,_)=>p.payload.as_ref(),_=>return Ok(())};
  if payload.is_some_and(|p|(p.device,p.inode)==(source_device,source_inode)){return Err(fail("Composite captured original erased inode retained"));}self.exclude_disposed_payload_file(path,file,designation)
 }
 pub(crate) async fn check_captured_committed_in(&self,conn:&mut sqlx::AnyConnection,stage_origin_raw:&[u8],files:&std::collections::BTreeMap<String,(u64,u64,u64,String)>)->anyhow::Result<()> {
  let CompositeRecordKind::Committed(receipt)=&self.kind else{anyhow::bail!("Composite committed source observation wrong class")};
  let b=self.scope.files.iter().find(|b|b.role==crate::storage::restoration::composite::FileRole::CapsuleRecord{name:crate::storage::restoration::composite::CapsuleRecord::StageOrigin}).ok_or_else(||fail("Composite captured stage origin absent"))?;
  anyhow::ensure!(stage_origin_raw.len()as u64==b.length&&digest(stage_origin_raw)==b.sha256,"Composite original stage origin differs");let origin:StageOrigin=decode(stage_origin_raw)?;
  for path in [&receipt.intent.storage_path,&format!(".attachment-upload-journal/{}/payload.part",receipt.intent.attempt_id)]{let (device,inode,length,sha)=files.get(path).ok_or_else(||fail("Composite captured committed file absent"))?;anyhow::ensure!((*device,*inode)==(origin.payload.device,origin.payload.inode)&&*length==receipt.intent.byte_length as u64&&sha==&receipt.intent.binary_sha256,"Composite captured original committed bytes/custody differ");}
  crate::storage::attachment_versions::validate_composite_preserved_singleton_in(conn,receipt).await.map_err(anyhow::Error::msg)
 }
}

// Purpose-private physical fact only: never current-reference or cleanup authority.
impl CompositeRecordEvidence {
 pub(crate) async fn validate_historical_disposed_bundle_in(&self,bundle:&crate::storage::backup_inventory::ValidatedBundle,conn:&mut sqlx::AnyConnection)->anyhow::Result<()> {
  let evidence=bundle.manifest.historical_composite_preservation_capture.as_ref().ok_or_else(||fail("Historical physical disposed purpose absent"))?;evidence.validate_shape()?;anyhow::ensure!(evidence.native.iter().any(|scope|scope==&self.scope),"Historical physical disposed scope is not in complete source envelope");
  bundle.fence()?;anyhow::ensure!(bundle.is_qualified_completed_capture(),"Composite disposed snapshot requires exact53 qualified profile");
  let(_raw,_replay,payload,length,sha)=match &self.kind {
   CompositeRecordKind::DisposedDirect(p)=>(self.scope.reservation.disposition_json.as_deref().ok_or_else(||fail("Composite original Direct raw absence absent"))?,false,p.payload.as_ref(),p.payload_length,p.payload_sha256.as_deref()),
   _=>anyhow::bail!("Composite disposed snapshot validator cannot classify another native class"),
  };
  let paths=bundle.manifest.files.0.keys().cloned().collect();self.validate_snapshot_row_and_paths_in(conn,&paths).await?;
  let designation=bundle.metadata_designation()?;
  for(path,bytes)in &bundle.manifest.files.0 {
   if path=="starlane-notes.db"{continue;} // Derived snapshot, not captured payload.
   let source=bundle.manifest.source_custody.files.0.get(path).ok_or_else(||fail("Composite disposed all-file custody omitted"))?;
   if let Some(payload)=payload {anyhow::ensure!((source.device,source.inode)!=(payload.device,payload.inode),"Composite disposed original payload inode resurrected under alias");}
   let held=bundle.open_owned(path)?;
   let informational=designation.as_ref().map(|d|d.matches_held_marker(path,&held)).transpose()?.unwrap_or(false);
   if !informational {anyhow::ensure!(!sha.is_some_and(|sha|bytes.size_bytes==length&&bytes.sha256==sha),"Composite disposed independent payload bytes resurrected under alias");}
  }
  if let Some(d)=&designation{d.fence()?;}bundle.fence()?;Ok(())
 }
}
