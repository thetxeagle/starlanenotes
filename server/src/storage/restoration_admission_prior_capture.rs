// Separate source exclusion/capture token. Never constructs or rebinds WriterLease.
pub(crate) use super::admission::PriorRestoredCaptureEvidence;
struct PreservedAdmissionCapsule {
 attempt:String,directory:File,stamp:PhysicalStamp,
 records:std::collections::BTreeMap<String,(File,PhysicalStamp,Vec<u8>)>,
}
/// Holds all reservation capsules; unsupported scopes remain preservation-only evidence.
pub(crate) struct PriorRestoredCaptureLeases {
 active:Arc<Active>,namespace:(File,PhysicalStamp),rows:Vec<admission::CapturedReservation>,
 history:(Vec<HistorySqlRow>,Vec<(i64,String)>),
 authorities:Vec<AdmissionRoleAuthority>,capsules:Vec<PreservedAdmissionCapsule>,
 role_directories:std::collections::BTreeMap<(String,admission::DirectoryRole),PhysicalStamp>,
 role_files:std::collections::BTreeMap<(String,admission::FileRole),PhysicalStamp>,
 replay_reference_inventory:Option<SqliteCaptureInventory>,
}
fn prior_capsule_names(directory:&File)->io::Result<std::collections::BTreeSet<String>> {
 std::fs::read_dir(format!("/proc/self/fd/{}",directory.as_raw_fd()))?.map(|e|e?.file_name().into_string().map_err(|_|fail("NonUTF8 prior capsule record"))).collect()
}
async fn prior_capture_rows(conn:&mut sqlx::AnyConnection)->io::Result<Vec<admission::CapturedReservation>> {
 let rows=sqlx::query("SELECT operation_id,request_sha256,admission_claim_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,disposition_json,disposition_sha256 FROM attachment_capacity_reservations ORDER BY operation_id").fetch_all(&mut *conn).await.map_err(|_|fail("Prior source reservation inventory unavailable"))?;
 rows.iter().map(|r|Ok(admission::CapturedReservation{operation_id:r.try_get(0).map_err(|_|fail("Invalid prior operation"))?,request_sha256:r.try_get(1).map_err(|_|fail("Invalid prior request"))?,admission_claim_sha256:r.try_get(2).map_err(|_|fail("Invalid prior claim"))?,maximum_bytes:r.try_get(3).map_err(|_|fail("Invalid prior maximum"))?,state:r.try_get(4).map_err(|_|fail("Invalid prior state"))?,committed_asset_id:r.try_get(5).map_err(|_|fail("Invalid prior committed asset"))?,committed_bytes:r.try_get(6).map_err(|_|fail("Invalid prior committed bytes"))?,disposition_json:r.try_get(7).map_err(|_|fail("Invalid prior disposition"))?,disposition_sha256:r.try_get(8).map_err(|_|fail("Invalid prior disposition digest"))?})).collect()
}
impl PriorRestoredCaptureLeases {
 pub(crate) async fn hold(path:&Path,conn:&mut sqlx::AnyConnection,expected:&[admission::CapturedReservation])->io::Result<Self> {
  let (_,active)=registered(path)?;let active=active.ok_or_else(||fail("Prior capture lacks registered finalized source"))?;active.check()?;
  if active.ledger.version!=2{return Err(fail("Prior capture requires finalized Ledger2; legacy capture unchanged"));}
  let mut rows=expected.to_vec();rows.sort_by(|a,b|a.operation_id.cmp(&b.operation_id));if rows.windows(2).any(|r|r[0].operation_id==r[1].operation_id)||prior_capture_rows(conn).await?!=rows{return Err(fail("Prior capture requires complete exact reservation inventory"));}
  let history=history_rows(conn).await?;
  let history_source=if active.engine.is_some(){HistorySource::new_with_check(&active.root,None,engine_transfer::engine_root_fence)?}else{HistorySource::new(&active.root,None)?};
  let closure=history_closure(history_source,identity(active.root.fd())?,Some((&history.0,&history.1)))?;if closure.current_id!=active.ledger.generation_id{return Err(fail("Prior capture current finalized source differs"));}closure.source.fence()?;
  let ns=active.root.directory(".attachment-upload-admission")?;let ns_stamp=physical_stamp(&ns)?;if ns_stamp.mode&0o777!=0o700||ns.metadata()?.uid()!=unsafe{libc::geteuid()}{return Err(fail("Prior admission namespace is not private owned directory"));}let names=prior_capsule_names(&ns)?;if names!=rows.iter().map(|r|r.operation_id.clone()).collect(){return Err(fail("Prior capsule inventory differs from complete reservations"));}
  let mut authorities=Vec::new();let mut capsules=Vec::new();
  for row in &rows {
   if !valid_id(&row.operation_id){return Err(fail("Invalid prior capture operation"));}
   let relative=format!(".attachment-upload-admission/{}",row.operation_id);let claim_path=format!("{relative}/claim.json");let raw=read_ledger(&active.root.leaf(&claim_path)?)?;row.matches_claim(&raw)?;
   let sealed=active.ledger.admission.as_ref().and_then(|a|a.scopes.iter().find(|s|s.attempt_id==row.operation_id));
   let directory=if row.state=="reserved"&&sealed.is_some(){let authority=acquire_admission_authority_in(path,conn,&row.operation_id,&raw).await?;let d=authority.directories[&admission::DirectoryRole::AdmissionCapsule].try_clone()?;authorities.push(authority);d}else{let d=active.root.directory(&relative)?;if unsafe{libc::flock(d.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}<0{return Err(io::Error::last_os_error());}d};
   let stamp=physical_stamp(&directory)?;if stamp.mode&0o777!=0o700||directory.metadata()?.uid()!=unsafe{libc::geteuid()}{return Err(fail("Prior capsule is not private owned directory"));}
   let mut records=std::collections::BTreeMap::new();for name in prior_capsule_names(&directory)? {
    if !["claim.json","stage-origin.json","replay-blob-origin.json","replay-rewrite-plan.json","replay-rewrite-origin.json","disposal-intent.json","disposal-tombstone.json","replay-disposal-intent.json","replay-disposal-tombstone.json"].contains(&name.as_str()){return Err(fail("Unsupported prior capsule record class; preserve charge without transport"));}
    let file=active.root.leaf(&format!("{relative}/{name}"))?;let before=physical_stamp(&file)?;if before.length>8*1024*1024||before.links!=1||before.mode&0o777!=0o600||file.metadata()?.uid()!=unsafe{libc::geteuid()}{return Err(fail("Prior capsule record bound/private topology differs"));}
    let bytes=read_ledger(&file)?;if physical_stamp(&file)?!=before{return Err(fail("Prior capsule record raced"));}
    crate::storage::attachment_upload::admission::validate_preserved_capsule_record(&name,&bytes,&raw)?;
    records.insert(name,(file,before,bytes));
   }
   if records.get("claim.json").is_none_or(|r|r.2!=raw){return Err(fail("Prior capsule original claim changed"));}
   capsules.push(PreservedAdmissionCapsule{attempt:row.operation_id.clone(),directory,stamp,records});
  }
  let mut role_directories=std::collections::BTreeMap::new();let mut role_files=std::collections::BTreeMap::new();
  for authority in &authorities {
   for (role,held) in &authority.directories {let stamp=physical_stamp(held)?;if stamp.mode&0o777!=0o700||held.metadata()?.uid()!=unsafe{libc::geteuid()}{return Err(fail("Prior finalized role directory is not private owned data"));}role_directories.insert((authority.scope.attempt_id.clone(),role.clone()),stamp);}
   for (role,held) in &authority.files {let stamp=physical_stamp(held)?;if stamp.links!=1||stamp.mode&0o777!=0o600||held.metadata()?.uid()!=unsafe{libc::geteuid()}{return Err(fail("Prior finalized role file is not private exclusive data"));}role_files.insert((authority.scope.attempt_id.clone(),role.clone()),stamp);}
  }
  let mut has_replay=false;for authority in &authorities {
   let claim:serde_json::Value=serde_json::from_str(&authority.scope.original_claim_json).map_err(|_|fail("Malformed finalized prior producer"))?;
   let producer:crate::storage::attachment_upload::admission::Producer=serde_json::from_value(claim.get("producer").ok_or_else(||fail("Missing finalized prior producer"))?.clone()).map_err(|_|fail("Invalid finalized prior producer"))?;
   has_replay|=matches!(producer,crate::storage::attachment_upload::admission::Producer::Replay{..});
  }
  let replay_reference_inventory=if has_replay{Some(sqlite_capture_inventory(&active.root)?)}else{None};
  let held=Self{active,namespace:(ns,ns_stamp),rows,history,authorities,capsules,role_directories,role_files,replay_reference_inventory};held.recheck_in(conn).await?;Ok(held)
 }
 pub(crate) fn fence(&self)->io::Result<()> {
  self.active.check()?;if let Some(inventory)=&self.replay_reference_inventory{let check=if self.active.engine.is_some(){engine_transfer::engine_root_fence}else{Root::check};inventory.fence_source(&self.active.root,check)?;}for authority in &self.authorities{authority.check()?;
   for (role,held) in &authority.directories {if physical_stamp(held)?!=self.role_directories[&(authority.scope.attempt_id.clone(),role.clone())]{return Err(fail("Prior finalized role directory stamp changed"));}}
   for (role,held) in &authority.files {if physical_stamp(held)?!=self.role_files[&(authority.scope.attempt_id.clone(),role.clone())]{return Err(fail("Prior finalized role exclusive file stamp changed"));}}
  }
  let ns=self.active.root.directory(".attachment-upload-admission")?;if physical_stamp(&ns)?!=self.namespace.1||physical_stamp(&self.namespace.0)?!=self.namespace.1{return Err(fail("Prior complete capsule namespace changed"));}if prior_capsule_names(&ns)?!=self.rows.iter().map(|r|r.operation_id.clone()).collect(){return Err(fail("Prior capsule inventory changed"));}
  for capsule in &self.capsules {
   let base=format!(".attachment-upload-admission/{}",capsule.attempt);let fresh=self.active.root.directory(&base)?;
   if physical_stamp(&fresh)?!=capsule.stamp||physical_stamp(&capsule.directory)?!=capsule.stamp||prior_capsule_names(&fresh)?!=capsule.records.keys().cloned().collect(){return Err(fail("Prior capsule directory/inventory changed"));}
   for (name,(held,stamp,raw)) in &capsule.records {let fresh=self.active.root.leaf(&format!("{base}/{name}"))?;if physical_stamp(held)?!=*stamp||physical_stamp(&fresh)?!=*stamp||read_ledger(held)?!=*raw||read_ledger(&fresh)?!=*raw{return Err(fail("Prior capsule exact original record changed"));}}
  }self.active.check()
 }
 pub(crate) async fn recheck_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()> {
  self.fence()?;if prior_capture_rows(conn).await?!=self.rows||history_rows(conn).await?!=self.history{return Err(fail("Prior complete source SQL/generation changed"));}
  for authority in &self.authorities{authority.check_in(conn).await?;}
  if let Some(held)=&self.replay_reference_inventory {
   use crate::storage::attachment_upload::admission::{Producer,validate_captured_replay_custody};
   use std::os::unix::fs::FileExt;
   let inventory=held.files.keys().cloned().collect();
   let read_bound=|path:&str,bound:u64|->io::Result<Vec<u8>> {
    let(file,stamp,sha)=held.files.get(path).ok_or_else(||fail("Prior Replay protected source reference absent"))?;
    if stamp.length>bound||physical_stamp(file)?!=*stamp{return Err(fail("Prior Replay source bound/physical proof differs"));}
    let mut raw=vec![0;stamp.length as usize];let mut offset=0;while offset<raw.len(){let n=file.read_at(&mut raw[offset..],offset as u64)?;if n==0{return Err(fail("Prior Replay source reference truncated"));}offset+=n;}
    if digest(&raw)!=*sha||physical_stamp(file)?!=*stamp{return Err(fail("Prior Replay source reference changed"));}Ok(raw)
   };
   let read=|path:&str|read_bound(path,6_000_000);
   for authority in &self.authorities {
    let claim:serde_json::Value=serde_json::from_str(&authority.scope.original_claim_json).map_err(|_|fail("Malformed finalized prior Replay producer"))?;
    let producer:Producer=serde_json::from_value(claim.get("producer").ok_or_else(||fail("Missing finalized prior producer"))?.clone()).map_err(|_|fail("Invalid finalized prior producer"))?;
    if matches!(producer,Producer::Replay{..}) {
     let capsule_prefix=format!(".attachment-upload-admission/{}/",authority.scope.attempt_id);let stage_prefix=format!(".attachment-upload-journal/{}/",authority.scope.attempt_id);
     let rewrite_prefixes=authority.scope.directories.iter().filter_map(|b|match &b.role{admission::DirectoryRole::CanonicalRewriteDirectory{operation_id}=>Some(format!(".canonical-rewrite-journal/{operation_id}/")),_=>None}).collect::<Vec<_>>();
     let expected=authority.scope.files.iter().filter(|b|!matches!(&b.role,admission::FileRole::ReplayBlob{..})).map(|b|b.role.relative(&authority.scope.attempt_id)).collect::<io::Result<std::collections::BTreeSet<_>>>()?;
     let observed=held.files.keys().filter(|p|p.starts_with(&capsule_prefix)||p.starts_with(&stage_prefix)||rewrite_prefixes.iter().any(|prefix|p.starts_with(prefix))).cloned().collect::<std::collections::BTreeSet<_>>();
     if expected!=observed{return Err(fail("Prior finalized Replay role directory inventory changed; no unsupported installation or added record transport"));}
     let mut raws=std::collections::BTreeMap::new();for binding in &authority.scope.files {let path=binding.role.relative(&authority.scope.attempt_id)?;let raw=read_bound(&path,64*1024*1024)?;if raw.len()as u64!=binding.length||digest(&raw)!=binding.sha256{return Err(fail("Prior finalized Replay role bytes differ"));}raws.insert(binding.role.clone(),raw);}
     let proof=validate_captured_replay_custody(&authority.scope,&raws)?;proof.validate_references(conn,&inventory,&read).await.map_err(|e|io::Error::other(format!("Prior Replay source reference refusal: {e:#}")))?;
    }
   }
  }
  self.fence()
 }
 /// Separate evidence type: never serialized as native version2 or installed automatically.
 pub(crate) fn lineage_evidence(&self)->io::Result<PriorRestoredCaptureEvidence> {
  self.fence()?;let scopes=self.authorities.iter().map(|a|{let s=&a.scope;admission::AdmissionCaptureScope{namespace:s.namespace.clone(),attempt_id:s.attempt_id.clone(),original_claim_json:s.original_claim_json.clone(),admission_claim_sha256:s.admission_claim_sha256.clone(),request_sha256:s.request_sha256.clone(),maximum_bytes:s.maximum_bytes,reservation:s.reservation.clone(),directories:s.directories.iter().map(|b|admission::CaptureDirectoryBinding{role:b.role.clone(),original:b.original.clone(),captured:b.adopted.clone()}).collect(),files:s.files.iter().map(|b|admission::CaptureFileBinding{role:b.role.clone(),original:b.original.clone(),captured:b.adopted.clone(),length:b.length,sha256:b.sha256.clone()}).collect()}}).collect();
  let root=identity(self.active.root.fd())?;Ok(PriorRestoredCaptureEvidence{version:3,generation_id:self.active.ledger.generation_id.clone(),ledger_sha256:self.active.ledger_sha256.clone(),source_root:admission::Identity{device:root.0,inode:root.1},scopes})
 }
}

// Cold source constructor authenticates captured predecessor closure and exact source SQL.
pub(super) async fn prior_capture_bundle_roles(bundle:&crate::storage::backup_inventory::ValidatedBundle,conn:&mut sqlx::AnyConnection,evidence:&PriorRestoredCaptureEvidence)->io::Result<admission::AdmissionOwnerEnvelope> {
 bundle.fence()?;
 if !bundle.is_qualified_capacity_capture(){return Err(fail("Prior admission requires qualified capacity SQLite format4/profile51"));}
 if evidence.version!=3||evidence.scopes.is_empty()||evidence.source_root!=(admission::Identity{device:bundle.manifest.source_custody.root.device,inode:bundle.manifest.source_custody.root.inode}) {return Err(fail("Prior transport lacks exact source generation/root or roles"));}
 let history=inspect_history(bundle,conn).await?.ok_or_else(||fail("Prior transport has no authenticated captured lineage"))?;
 if history.current_id!=evidence.generation_id{return Err(fail("Prior transport current generation differs"));}
 let generation=&history.generations[&history.current_id];if digest(&generation.raw)!=evidence.ledger_sha256||generation.envelope.as_ref().is_none_or(|e|e.version!=3)||generation.ledger.version!=2{return Err(fail("Prior transport finalized Ledger2 bytes differ"));}
 let original=generation.ledger.admission.as_ref().ok_or_else(||fail("Prior finalized admission roles absent"))?;
 let mut scopes=Vec::new();let mut seen=std::collections::BTreeSet::new();
 for capture in &evidence.scopes {
  if !seen.insert(capture.attempt_id.clone()){return Err(fail("Duplicate prior capture attempt"));}
  let old=original.scopes.iter().find(|s|s.attempt_id==capture.attempt_id).ok_or_else(||fail("Prior capture scope absent from finalized source"))?;
  let expected=admission::AdmissionCaptureScope{namespace:old.namespace.clone(),attempt_id:old.attempt_id.clone(),original_claim_json:old.original_claim_json.clone(),admission_claim_sha256:old.admission_claim_sha256.clone(),request_sha256:old.request_sha256.clone(),maximum_bytes:old.maximum_bytes,reservation:old.reservation.clone(),directories:old.directories.iter().map(|b|admission::CaptureDirectoryBinding{role:b.role.clone(),original:b.original.clone(),captured:b.adopted.clone()}).collect(),files:old.files.iter().map(|b|admission::CaptureFileBinding{role:b.role.clone(),original:b.original.clone(),captured:b.adopted.clone(),length:b.length,sha256:b.sha256.clone()}).collect()};
  if *capture!=expected||!matches!(old.reservation,admission::ReservationState::Reserved){return Err(fail("Prior transport scope differs from exact finalized original/adopted roles"));}
  let mut scope=old.clone();for b in &mut scope.directories{b.captured=b.adopted.clone();}for b in &mut scope.files{b.captured=b.adopted.clone();}scopes.push(scope);
 }
 let scopes=admission::AdmissionOwnerEnvelope{version:2,scopes};scopes.validate(&evidence.source_root)?;bundle.fence()?;Ok(scopes)
}

/// Explicit finalized-role class selection, never a fallback after native failure.
pub(crate) fn has_finalized_admission_capture_source(path:&Path)->io::Result<bool> {
 let (_,active)=registered(path)?;let Some(active)=active else{return Ok(false)};active.check()?;
 Ok(active.ledger.version==2&&active.ledger.admission.is_some())
}
