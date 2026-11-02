// Finalized completed source capture. Original records remain immutable; no writer constructor.
#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PriorCompletedCaptureEvidence {
 pub version:u32,pub generation_id:String,pub ledger_sha256:String,
 pub source_root:admission::Identity,pub roles:completed::CompletedOwnerEnvelope,
}
impl PriorCompletedCaptureEvidence {
 pub(crate) fn validate_shape(&self)->io::Result<()> {
  if self.version!=1||!valid_id(&self.generation_id)||!valid_hash(&self.ledger_sha256){return Err(fail("Malformed prior completed capture evidence"));}
  self.roles.validate(&self.source_root)
 }
}
pub(crate) struct FinalizedCompletedCapture {
 active:Arc<Active>,accounting:crate::storage::attachment_upload::admission::committed_erasure::accounting::CompletedAccounting,
 history:(Vec<HistorySqlRow>,Vec<(i64,String)>),roles:completed::CompletedOwnerEnvelope,
 inventory:SqliteCaptureInventory,
 designation:Option<crate::storage::data_root_designation::Designation>,
}
impl FinalizedCompletedCapture {
 pub(crate) async fn hold(path:&Path,conn:&mut sqlx::AnyConnection)->io::Result<Self> {
  let(_,active)=registered(path)?;let active=active.ok_or_else(||fail("Prior completed capture lacks original registered source"))?;active.check()?;
  if active.ledger.version!=3||active.ledger.admission.is_some(){return Err(fail("Prior completed capture requires exclusive Ledger3"));}
  if let Some(engine)=&active.engine {match &**engine {
   EngineAuthority::PgRestore(p)=>p.verify_any_connection(conn).await.map_err(|_|fail("Prior completed original connected PG target differs"))?,
   EngineAuthority::Transfer(_)=>return Err(fail("Prior completed capture requires finalized restored engine authority")),
  }}
  let roles=active.ledger.completed_absence.clone().ok_or_else(||fail("Prior completed finalized roles absent"))?;
  roles.validate(&admission::Identity{device:active.ledger.root_device,inode:active.ledger.root_inode})?;
  let history=history_rows(conn).await?;
  let paths=crate::storage::attachment_capacity::retained_paths_in(conn).await.map_err(|_|fail("Prior completed retained path evidence invalid"))?;
  // One held accounting owner performs global references and acquires all capsule
  // exclusions exactly once. A serialized Ledger3 never substitutes this owner.
  let accounting=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture_in_at(path,conn,paths,&[]).await.map_err(|_|fail("Prior completed source current absence/reference proof invalid"))?;
  accounting.require_restored_only_in(conn).await.map_err(|_|fail("Mixed old/new capsule/reservation source requires distinct transport evidence"))?;
  if accounting.ids().count()!=roles.scopes.len(){return Err(fail("Mixed old/new completed source requires distinct transport evidence"));}
  for scope in &roles.scopes {
   let raw=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::original_record_metadata(&scope.original).map_err(|_|fail("Prior completed original metadata invalid"))?;
   let receipt=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::validate_original_records(&scope.original,&raw).map_err(|_|fail("Prior completed original proof invalid"))?;
   if !accounting.matches(&receipt){return Err(fail("Prior completed held original differs"));}
  }
  let designation=crate::storage::data_root_designation::Designation::capture_held(path,active.root.fd())?;
  let inventory=sqlite_capture_inventory(&active.root)?;
  let proof=Self{active,accounting,history,roles,inventory,designation};proof.recheck_in(conn).await?;Ok(proof)
 }
 pub(crate) fn fence(&self)->io::Result<()> {
  self.active.check()?;self.accounting.fence().map_err(|_|fail("Prior completed held absence changed"))?;
  let check=if self.active.engine.is_some(){engine_transfer::engine_root_fence}else{Root::check};self.inventory.fence_source(&self.active.root,check)?;
  let envelope=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope{version:1,source_kind:"original_native_direct_v1".into(),scopes:self.roles.scopes.iter().map(|s|s.original.clone()).collect()};
  crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::exclude_completed_payload_inventory(&envelope,self.inventory.files.iter().map(|(path,(held,stamp,sha))|(stamp.device,stamp.inode,stamp.length,sha.as_str(),self.designation.as_ref().map(|token|(token,path.as_str(),held))))).map_err(|_|fail("Prior completed full source payload absence differs"))?;self.active.check()
 }
 pub(crate) async fn recheck_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()> {
  self.fence()?;
  if let Some(engine)=&self.active.engine {match &**engine {
   EngineAuthority::PgRestore(p)=>p.verify_any_connection(conn).await.map_err(|_|fail("Prior completed original connected PG target differs"))?,
   EngineAuthority::Transfer(_)=>return Err(fail("Prior completed capture requires finalized restored engine authority")),
  }}
  if history_rows(conn).await?!=self.history{return Err(fail("Prior completed source generation SQL changed"));}
  self.accounting.fence_restored_in(&self.active.root.absolute,conn).await.map_err(|_|fail("Prior completed full SQL/reference proof changed"))?;
  self.fence()
 }
 pub(crate) fn evidence(&self)->io::Result<PriorCompletedCaptureEvidence> {
  self.fence()?;let root=identity(self.active.root.fd())?;
  let evidence=PriorCompletedCaptureEvidence{version:1,generation_id:self.active.ledger.generation_id.clone(),ledger_sha256:self.active.ledger_sha256.clone(),source_root:admission::Identity{device:root.0,inode:root.1},roles:self.roles.clone()};evidence.validate_shape()?;Ok(evidence)
 }
}
pub(crate) fn has_finalized_completed_capture_source(path:&Path)->io::Result<bool> {
 let(_,active)=registered(path)?;let Some(active)=active else{return Ok(false)};active.check()?;Ok(active.ledger.version==3&&active.ledger.completed_absence.is_some())
}
pub(crate) async fn prior_completed_bundle_roles(bundle:&crate::storage::backup_inventory::ValidatedBundle,conn:&mut sqlx::AnyConnection,evidence:&PriorCompletedCaptureEvidence)->io::Result<completed::CompletedOwnerEnvelope> {
 bundle.fence()?;evidence.validate_shape()?;
 if !bundle.is_qualified_completed_capture()||evidence.source_root!=(admission::Identity{device:bundle.manifest.source_custody.root.device,inode:bundle.manifest.source_custody.root.inode}){return Err(fail("Prior completed captured profile/root differs"));}
 let history=inspect_history(bundle,conn).await?.ok_or_else(||fail("Prior completed captured finalized history absent"))?;
 if history.current_id!=evidence.generation_id{return Err(fail("Prior completed current captured generation differs"));}
 let generation=&history.generations[&history.current_id];
 if digest(&generation.raw)!=evidence.ledger_sha256||generation.ledger.version!=3||generation.ledger.admission.is_some()||generation.ledger.completed_absence.as_ref()!=Some(&evidence.roles){return Err(fail("Prior completed canonical Ledger3 roles differ"));}
 bundle.fence()?;Ok(evidence.roles.clone())
}

// Evidence only: constructors must retain the original old-scope/native roster.
#[derive(Clone,Debug,PartialEq,Eq,serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MixedCompletedCaptureEvidence{pub version:u32,pub prior:PriorCompletedCaptureEvidence,pub native:crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope}
impl MixedCompletedCaptureEvidence{
 pub(crate) fn validate_shape(&self)->io::Result<()> {
  if self.version!=1{return Err(fail("Unsupported mixed completed capture evidence"));}self.prior.validate_shape()?;self.native.validate_shape().map_err(|_|fail("Malformed mixed native completion proof"))?;
  let old_assets=self.prior.roles.scopes.iter().map(|s|s.original.disposition["asset_id"].clone()).collect::<std::collections::BTreeSet<_>>();let old_attempts=self.prior.roles.scopes.iter().map(|s|s.original.reservation.operation_id.clone()).collect::<std::collections::BTreeSet<_>>();
  if self.native.scopes.iter().any(|s|old_assets.contains(&s.disposition["asset_id"])||old_attempts.contains(&s.reservation.operation_id))||self.native.scopes.len()+old_assets.len()>512||serde_json::to_vec(self).map_err(|_|fail("Mixed capture evidence encoding invalid"))?.len()>8*1024*1024{return Err(fail("Mixed capture scopes duplicate or exceed bound"));}
  for scope in &self.native.scopes{let root=scope.directories.iter().find(|s|matches!(s.role,crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedDirectoryRole::DataRoot)).ok_or_else(||fail("Mixed native original root absent"))?;if root.original!=self.prior.source_root||root.captured!=self.prior.source_root{return Err(fail("Mixed native proof is not original current source root"));}}Ok(())
 }
 pub(crate) fn envelope(&self)->io::Result<crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope>{self.validate_shape()?;let mut scopes=self.prior.roles.scopes.iter().map(|s|s.original.clone()).chain(self.native.scopes.iter().cloned()).collect::<Vec<_>>();scopes.sort_by(|a,b|a.disposition["asset_id"].cmp(&b.disposition["asset_id"]));let e=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope{version:1,source_kind:"original_native_direct_v1".into(),scopes};e.validate_shape().map_err(|_|fail("Mixed completion union malformed"))?;Ok(e)}
}

// Source-held mixed proof. Its prior subproof never constructs a WriterLease.
pub(crate) struct MixedCompletedCapture {
 active:Arc<Active>,native:crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::NativeCompletedCapture,
 evidence:MixedCompletedCaptureEvidence,history:(Vec<HistorySqlRow>,Vec<(i64,String)>),inventory:SqliteCaptureInventory,
 designation:Option<crate::storage::data_root_designation::Designation>,
}
impl MixedCompletedCapture {
 pub(crate) async fn hold(path:&Path,conn:&mut sqlx::AnyConnection)->io::Result<Self>{
  let(_,active)=registered(path)?;let active=active.ok_or_else(||fail("Mixed completed capture lacks original registered source"))?;active.check()?;
  if active.ledger.version!=3||active.ledger.admission.is_some()||active.ledger.completed_absence.is_none(){return Err(fail("Mixed capture requires original exclusive completed Ledger3"));}
  if let Some(engine)=&active.engine{match &**engine{EngineAuthority::PgRestore(p)=>p.verify_any_connection(conn).await.map_err(|_|fail("Mixed original connected PG target differs"))?,EngineAuthority::Transfer(_)=>return Err(fail("Mixed completion requires finalized restored engine authority")),}}
  let history=history_rows(conn).await?;let paths=crate::storage::attachment_capacity::retained_paths_in(conn).await.map_err(|_|fail("Mixed retained path evidence invalid"))?;
  let native=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::NativeCompletedCapture::capture_native_additions_in(path,conn,paths).await.map_err(|_|fail("Mixed held native additions/current references invalid"))?;
  let evidence=native.mixed_evidence_in(conn).await.map_err(|_|fail("Mixed source evidence unavailable"))?;
  if evidence.prior.generation_id!=active.ledger.generation_id||evidence.prior.ledger_sha256!=active.ledger_sha256||active.ledger.completed_absence.as_ref()!=Some(&evidence.prior.roles){return Err(fail("Mixed prior original Active differs"));}
  let designation=crate::storage::data_root_designation::Designation::capture_held(path,active.root.fd())?;let inventory=sqlite_capture_inventory(&active.root)?;
  let proof=Self{active,native,evidence,history,inventory,designation};proof.recheck_in(conn).await?;Ok(proof)
 }
 pub(crate) fn fence(&self)->io::Result<()>{
  self.active.check()?;self.native.fence().map_err(|_|fail("Mixed original native custody changed"))?;
  let check=if self.active.engine.is_some(){engine_transfer::engine_root_fence}else{Root::check};self.inventory.fence_source(&self.active.root,check)?;
  let envelope=self.evidence.envelope()?;
  crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::exclude_completed_payload_inventory(&envelope,self.inventory.files.iter().map(|(path,(held,stamp,sha))|(stamp.device,stamp.inode,stamp.length,sha.as_str(),self.designation.as_ref().map(|token|(token,path.as_str(),held))))).map_err(|_|fail("Mixed complete source erased payload census differs"))?;
  self.active.check()
 }
 pub(crate) async fn recheck_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()>{
  self.fence()?;if let Some(engine)=&self.active.engine{match &**engine{EngineAuthority::PgRestore(p)=>p.verify_any_connection(conn).await.map_err(|_|fail("Mixed original connected PG target differs"))?,EngineAuthority::Transfer(_)=>return Err(fail("Mixed completion requires finalized engine authority")),}}
  if history_rows(conn).await?!=self.history{return Err(fail("Mixed source finalized generation changed"));}
  if self.native.mixed_evidence_in(conn).await.map_err(|_|fail("Mixed complete SQL/roster/references changed"))?!=self.evidence{return Err(fail("Mixed evidence changed"));}self.fence()
 }
 pub(crate) fn evidence(&self)->io::Result<MixedCompletedCaptureEvidence>{self.fence()?;self.evidence.validate_shape()?;Ok(self.evidence.clone())}
}
// Explicit current SQL selection. Unsupported additional reservation classes still
// fail in hold; errors never fall back to the old-only producer.
pub(crate) async fn has_mixed_completed_capture_source(path:&Path,conn:&mut sqlx::AnyConnection)->io::Result<bool>{
 let(_,active)=registered(path)?;let Some(active)=active else{return Ok(false)};active.check()?;
 let Some(roles)=&active.ledger.completed_absence else{return Ok(false)};
 if active.ledger.version!=3||active.ledger.admission.is_some(){return Err(fail("Mixed source class requires exclusive Ledger3"));}
 let actual:Vec<String>=sqlx::query_scalar("SELECT asset_id FROM attachment_erasure_dispositions ORDER BY asset_id LIMIT 513").fetch_all(&mut *conn).await.map_err(|_|fail("Mixed source completed SQL inventory unavailable"))?;
 let old=roles.scopes.iter().map(|s|s.original.disposition["asset_id"].clone()).collect::<std::collections::BTreeSet<_>>();
 if actual.len()>512||!old.iter().all(|id|actual.contains(id)){return Err(fail("Mixed source completed roster incomplete"));}active.check()?;Ok(actual.iter().any(|id|!old.contains(id)))
}

// Repeated composite evidence is an explicit new class, never a legacy Ledger3 reinterpretation.
pub(crate) async fn prior_composite_completed_bundle_roles(bundle:&crate::storage::backup_inventory::ValidatedBundle,conn:&mut sqlx::AnyConnection,evidence:&composite::CompositeCaptureEvidence)->io::Result<completed::CompletedOwnerEnvelope>{
 bundle.fence()?;evidence.validate_shape()?;if evidence.version!=2{return Err(fail("Repeated composite source requires evidence v2"));}
 let e=&evidence.prior;if !bundle.is_qualified_completed_capture()||e.source_root!=(admission::Identity{device:bundle.manifest.source_custody.root.device,inode:bundle.manifest.source_custody.root.inode}){return Err(fail("Repeated composite captured profile/root differs"));}
 let history=inspect_history(bundle,conn).await?.ok_or_else(||fail("Repeated composite finalized history absent"))?;if history.current_id!=e.generation_id{return Err(fail("Repeated composite captured generation differs"));}let generation=&history.generations[&history.current_id];let old=generation.ledger.composite.as_ref().ok_or_else(||fail("Repeated composite canonical Ledger4 roles absent"))?;
 if digest(&generation.raw)!=e.ledger_sha256||generation.ledger.version!=4||generation.ledger.admission.is_some()||old.completed!=e.roles||evidence.prior_native.as_ref()!=Some(&old.native){return Err(fail("Repeated composite canonical original/current roles differ"));}old.validate(&e.source_root)?;bundle.fence()?;Ok(e.roles.clone())
}

pub(crate) async fn composite_prior_committed_identity_in(bundle:&crate::storage::backup_inventory::ValidatedBundle,conn:&mut sqlx::AnyConnection,attempt:&str,path:&str,original:(u64,u64),length:u64,sha:&str,proof_sha:&str)->io::Result<(u64,u64)>{
 let e=bundle.manifest.composite_admission_capture.as_ref().ok_or_else(||fail("Repeated committed capture absent"))?;prior_composite_completed_bundle_roles(bundle,conn,e).await?;
 if !e.prior_native.as_ref().is_some_and(|all|all.iter().any(|n|n.original.reservation.operation_id==attempt&&matches!(n.original.class,composite::NativeClass::CommittedDirect{..}))){return Err(fail("Repeated committed scope not in authenticated old census"));}
 let history=inspect_history(bundle,conn).await?.ok_or_else(||fail("Repeated committed original history absent"))?;let ledger=&history.generations[&history.current_id].ledger;
 let scope=ledger.scopes.iter().find(|s|s.namespace==".attachment-upload-journal"&&s.op_id==attempt&&s.evidence_sha256==proof_sha).ok_or_else(||fail("Repeated committed original upload scope absent"))?;
 let binding=scope.bindings.iter().find(|b|b.relative==path).ok_or_else(||fail("Repeated committed typed path binding absent"))?;
 if (binding.original_device,binding.original_inode)!=original||binding.length!=length||binding.sha256!=sha{return Err(fail("Repeated committed original proof bytes/path differ"));}bundle.fence()?;Ok((binding.adopted_device,binding.adopted_inode))
}
