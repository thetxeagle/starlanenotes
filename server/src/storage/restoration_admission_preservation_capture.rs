// Preservation-only source certificate: no admission scope conversion or WriterLease API.
// Separate Ledger1 semantics leave every serving/disposal constructor unchanged.
enum PreservedAdmissionNamespace { Present(File,PhysicalStamp), Absent(PhysicalStamp) }
pub(crate) struct FinalizedAdmissionPreservationLeases {
 active:Arc<Active>,namespace:PreservedAdmissionNamespace,rows:Vec<admission::CapturedReservation>,
 history:(Vec<HistorySqlRow>,Vec<(i64,String)>),capsules:Vec<PreservedAdmissionCapsule>,
}
impl FinalizedAdmissionPreservationLeases {
 pub(crate) async fn hold(path:&Path,conn:&mut sqlx::AnyConnection,expected:&[admission::CapturedReservation])->io::Result<Self> {
  let (_,active)=registered(path)?;let active=active.ok_or_else(||fail("Finalized preservation lacks registered finalized source"))?;active.check()?;
  if active.ledger.version!=1||active.ledger.admission.is_some()||!active.version2{return Err(fail("Preservation requires authenticated finalized Ledger1 without admission roles"));}
  let mut rows=expected.to_vec();rows.sort_by(|a,b|a.operation_id.cmp(&b.operation_id));if rows.windows(2).any(|r|r[0].operation_id==r[1].operation_id)||prior_capture_rows(conn).await?!=rows{return Err(fail("Finalized preservation requires complete exact reservation inventory"));}
  let history=history_rows(conn).await?;
  let history_source=if active.engine.is_some(){HistorySource::new_with_check(&active.root,None,engine_transfer::engine_root_fence)?}else{HistorySource::new(&active.root,None)?};
  let closure=history_closure(history_source,identity(active.root.fd())?,Some((&history.0,&history.1)))?;if closure.current_id!=active.ledger.generation_id{return Err(fail("Finalized preservation current finalized source differs"));}if closure.generations[&closure.current_id].envelope.as_ref().is_none_or(|e|e.version!=3||e.finalization.is_none()){return Err(fail("Preservation requires authenticated finalized generation"));}closure.source.fence()?;
  // Genuine finalized zero-reservation sources need not have ever created a
  // native namespace. Absence is a private retained observation, never a role.
  let empty_root_stamp=physical_stamp(active.root.fd())?;
  if !exists(active.root.fd(),".attachment-upload-admission")? {
   if !rows.is_empty(){return Err(fail("Preserved reservations have no native namespace"));}
   let held=Self{active,namespace:PreservedAdmissionNamespace::Absent(empty_root_stamp),rows,history,capsules:Vec::new()};held.recheck_in(conn).await?;return Ok(held);
  }
  let ns=active.root.directory(".attachment-upload-admission")?;let ns_stamp=physical_stamp(&ns)?;if ns_stamp.mode&0o777!=0o700||ns.metadata()?.uid()!=unsafe{libc::geteuid()}{return Err(fail("Prior admission namespace is not private owned directory"));}let names=prior_capsule_names(&ns)?;if names!=rows.iter().map(|r|r.operation_id.clone()).collect(){return Err(fail("Preserved capsule inventory differs from complete reservations"));}
  let mut capsules=Vec::new();
  for row in &rows {
   if !valid_id(&row.operation_id){return Err(fail("Invalid prior capture operation"));}
   let relative=format!(".attachment-upload-admission/{}",row.operation_id);let claim_path=format!("{relative}/claim.json");let raw=read_ledger(&active.root.leaf(&claim_path)?)?;row.matches_claim(&raw)?;
   // A preservation hold excludes writers only; it authenticates no original inode ownership.
   let directory=active.root.directory(&relative)?;
   if unsafe{libc::flock(directory.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}<0{return Err(io::Error::last_os_error());}
   let stamp=physical_stamp(&directory)?;if stamp.mode&0o777!=0o700||directory.metadata()?.uid()!=unsafe{libc::geteuid()}{return Err(fail("Preserved capsule is not private owned directory"));}
   let mut records=std::collections::BTreeMap::new();for name in prior_capsule_names(&directory)? {
    let file=active.root.leaf(&format!("{relative}/{name}"))?;let before=physical_stamp(&file)?;if before.length>8*1024*1024||before.links!=1||before.mode&0o777!=0o600||file.metadata()?.uid()!=unsafe{libc::geteuid()}{return Err(fail("Preserved capsule record bound/private topology differs"));}
    let bytes=read_ledger(&file)?;if physical_stamp(&file)?!=before{return Err(fail("Preserved capsule record raced"));}
    if ["claim.json","stage-origin.json","replay-blob-origin.json","replay-rewrite-plan.json","replay-rewrite-origin.json","disposal-intent.json","disposal-tombstone.json","replay-disposal-intent.json","replay-disposal-tombstone.json"].contains(&name.as_str()) {
     crate::storage::attachment_upload::admission::validate_preserved_capsule_record(&name,&bytes,&raw)?;
    } // Unknown bounded private records remain opaque immutable evidence, never ownership proof.
    records.insert(name,(file,before,bytes));
   }
   if records.get("claim.json").is_none_or(|r|r.2!=raw){return Err(fail("Preserved capsule original claim changed"));}
   capsules.push(PreservedAdmissionCapsule{attempt:row.operation_id.clone(),directory,stamp,records});
  }
  let held=Self{active,namespace:PreservedAdmissionNamespace::Present(ns,ns_stamp),rows,history,capsules};held.recheck_in(conn).await?;Ok(held)
 }
 pub(crate) fn fence(&self)->io::Result<()> {
  self.active.check()?;
  match &self.namespace {
   PreservedAdmissionNamespace::Present(held,stamp)=>{let ns=self.active.root.directory(".attachment-upload-admission")?;if physical_stamp(&ns)?!=*stamp||physical_stamp(held)?!=*stamp{return Err(fail("Preserved complete capsule namespace changed"));}if prior_capsule_names(&ns)?!=self.rows.iter().map(|r|r.operation_id.clone()).collect(){return Err(fail("Preserved capsule inventory changed"));}},
   PreservedAdmissionNamespace::Absent(stamp)=>{if !self.rows.is_empty()||!self.capsules.is_empty()||physical_stamp(self.active.root.fd())?!=*stamp||exists(self.active.root.fd(),".attachment-upload-admission")?{return Err(fail("Preserved empty native namespace/root changed"));}}
  }
  for capsule in &self.capsules {
   let base=format!(".attachment-upload-admission/{}",capsule.attempt);let fresh=self.active.root.directory(&base)?;
   if physical_stamp(&fresh)?!=capsule.stamp||physical_stamp(&capsule.directory)?!=capsule.stamp||prior_capsule_names(&fresh)?!=capsule.records.keys().cloned().collect(){return Err(fail("Preserved capsule directory/inventory changed"));}
   for (name,(held,stamp,raw)) in &capsule.records {let fresh=self.active.root.leaf(&format!("{base}/{name}"))?;if physical_stamp(held)?!=*stamp||physical_stamp(&fresh)?!=*stamp||read_ledger(held)?!=*raw||read_ledger(&fresh)?!=*raw{return Err(fail("Preserved capsule exact original record changed"));}}
  }self.active.check()
 }
 pub(crate) async fn recheck_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()> {
  self.fence()?;if prior_capture_rows(conn).await?!=self.rows||history_rows(conn).await?!=self.history{return Err(fail("Preserved complete source SQL/generation changed"));}
  self.fence()
 }
}

/// Explicit class selection only. Never use preservation as a fallback after native failure.
pub(crate) fn has_finalized_admission_preservation_source(path:&Path)->io::Result<bool> {
 let (_,active)=registered(path)?;
 let Some(active)=active else{return Ok(false)};active.check()?;
 let current=history_parse(&read_ledger(&active.file)?)?;
 Ok(active.ledger.version==1&&active.ledger.admission.is_none()&&current.envelope.as_ref().is_some_and(|e|e.version==3&&e.finalization.is_some()))
}
