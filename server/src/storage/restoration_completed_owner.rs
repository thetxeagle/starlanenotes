// Completed absence certifier and finalized reader; included only in linux.
struct CertifiedCompletedDestination {
 roles:completed::CompletedOwnerEnvelope,
 source:crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::ClassifiedCompletedBundle,
}
async fn completed_certify_destination(bundle:&crate::storage::backup_inventory::ValidatedBundle,d:&PlannedDestination,conn:&mut sqlx::AnyConnection)->io::Result<Option<CertifiedCompletedDestination>> {
 if bundle.manifest.completed_absence_capture.is_none()&&bundle.manifest.prior_completed_absence_capture.is_none()&&bundle.manifest.mixed_completed_absence_capture.is_none(){return Ok(None);}
 completed_certify_destination_component(bundle,d,conn).await
}
async fn completed_certify_destination_component(bundle:&crate::storage::backup_inventory::ValidatedBundle,d:&PlannedDestination,conn:&mut sqlx::AnyConnection)->io::Result<Option<CertifiedCompletedDestination>> {
 bundle.fence()?;d.fence()?;
 let path=bundle.data_path().parent().ok_or_else(||fail("Completed source path absent"))?.to_owned();
 let independently_held=std::sync::Arc::new(crate::storage::backup_inventory::verify(&path).await.map_err(|_|fail("Completed held source verification failed"))?);
 if independently_held.raw_manifest!=bundle.raw_manifest||independently_held.copy_overlap_identities()?!=bundle.copy_overlap_identities()?{return Err(fail("Completed held bundle source differs"));}
 let source=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::ClassifiedCompletedBundle::from_bundle(independently_held,conn).await.map_err(|_|fail("Completed source typed/custody/SQL proof invalid"))?;
 let mut scopes=Vec::new();
 for original in &source.source_envelope().scopes {
  let attempt=&original.reservation.operation_id;let mut directories=Vec::new();let mut files=Vec::new();
  for binding in &original.directories {
   let id=if let Some(path)=binding.role.relative(attempt).map_err(|_|fail("Invalid completed source directory"))?{
    let(held,stamp)=d.directories.get(&path).ok_or_else(||fail("Completed certified target directory absent"))?;
    if identity(held)?!=(stamp.device,stamp.inode)||held.metadata()?.mode()&0o777!=0o700{return Err(fail("Completed target directory changed"));}
    admission::Identity{device:stamp.device,inode:stamp.inode}
   }else{admission::Identity{device:d.root_stamp.device,inode:d.root_stamp.inode}};
   directories.push(completed::CompletedOwnedDirectory{role:binding.role.clone(),adopted:id});
  }
  for binding in &original.files {
   let path=binding.role.relative(attempt).map_err(|_|fail("Invalid completed target file"))?;
   let held=d.leaves.get(&path).ok_or_else(||fail("Completed certified target record absent"))?;
   if held.bytes.size_bytes!=binding.length||held.bytes.sha256!=binding.sha256||identity(&held.held)?!=(held.stamp.device,held.stamp.inode){return Err(fail("Completed target record changed"));}
   files.push(completed::CompletedOwnedFile{role:binding.role.clone(),adopted:admission::Identity{device:held.stamp.device,inode:held.stamp.inode}});
  }
  let raw=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::original_record_metadata(original).map_err(|_|fail("Completed original record metadata malformed"))?;
  let receipt=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::validate_original_records(original,&raw).map_err(|_|fail("Completed original receipt malformed"))?;
  let stage_payload=format!(".attachment-upload-journal/{attempt}/payload.part");
  for path in [&receipt.intent.storage_path,&stage_payload]{
   if d.leaves.contains_key(path)||d.directories.contains_key(path){return Err(fail("Completed target erased path resurrected"));}
   let(parent,name)=path.rsplit_once('/').ok_or_else(||fail("Completed erased target parent invalid"))?;
   let(held,_)=d.directories.get(parent).ok_or_else(||fail("Completed erased target parent absent"))?;
   if exists(held,name)?{return Err(fail("Completed erased target leaf exists"));}
  }
  scopes.push(completed::CompletedOwnerScope{original:original.clone(),directories,files,source_basis:if source.uses_source_basis(){Some(source.source_basis(original).map_err(|_|fail("Completed current source basis invalid"))?)}else{None}});
 }
 let roles=completed::CompletedOwnerEnvelope{version:if source.uses_source_basis(){2}else{1},scopes};roles.validate(&admission::Identity{device:d.root_stamp.device,inode:d.root_stamp.inode})?;
 source.fence_in(conn).await.map_err(|_|fail("Completed source changed during certification"))?;bundle.fence()?;d.fence()?;
 Ok(Some(CertifiedCompletedDestination{roles,source}))
}

/// Accounting-only finalized absence. No writer, serving or deletion authority.
pub(crate) struct CompletedAbsenceAuthority {
 active:Arc<Active>,roles:completed::CompletedOwnerEnvelope,
 native:Option<CompositeNativeAuthority>,
 family:Arc<crate::storage::native_family_fence::Creation>,
 directories:std::collections::BTreeMap<String,File>,files:std::collections::BTreeMap<String,File>,
 receipts:std::collections::BTreeMap<String,crate::storage::attachment_upload::Receipt>,
 immutable_stamps:std::collections::BTreeMap<String,PhysicalStamp>,
}
impl CompletedAbsenceAuthority {
 pub(crate) fn check(&self)->io::Result<()> {
  use crate::storage::attachment_upload::admission::committed_erasure::accounting::capture as capture;
  self.active.check()?;if let Some(native)=&self.native{native.check()?;}
  if identity(self.family.root())?!=identity(self.active.root.fd())?{return Err(fail("Completed held family root differs"));}
  for scope in &self.roles.scopes {
   let attempt=&scope.original.reservation.operation_id;
   for b in &scope.directories {
    let relative=b.role.relative(attempt).map_err(|_|fail("Completed directory role invalid"))?;
    let key=relative.clone().unwrap_or_default();let held=&self.directories[&key];
    let fresh=if let Some(path)=relative{self.active.root.directory(&path)?}else{self.active.root.fd().try_clone()?};
    if identity(held)?!=(b.adopted.device,b.adopted.inode)||identity(&fresh)?!=identity(held)?||fresh.metadata()?.mode()&0o777!=0o700{return Err(fail("Completed adopted directory changed"));}
   }
   for role in [capture::CompletedDirectoryRole::AdmissionCapsule,capture::CompletedDirectoryRole::DirectStageDirectory]{let path=role.relative(attempt).map_err(|_|fail("Completed immutable directory role invalid"))?.ok_or_else(||fail("Completed immutable directory absent"))?;let held=&self.directories[&path];let fresh=self.active.root.directory(&path)?;if physical_stamp(held)?!=self.immutable_stamps[&path]||physical_stamp(&fresh)?!=self.immutable_stamps[&path]{return Err(fail("Held completed immutable directory stamp changed"));}}
   for b in &scope.files {
    let path=b.role.relative(attempt).map_err(|_|fail("Completed file role invalid"))?;
    let held=&self.files[&path];let fresh=self.active.root.leaf(&path)?;
    if physical_stamp(held)?!=self.immutable_stamps[&path]||physical_stamp(&fresh)?!=self.immutable_stamps[&path]{return Err(fail("Held completed immutable file stamp changed"));}
    let original=scope.original.files.iter().find(|s|s.role==b.role).ok_or_else(||fail("Completed original file binding absent"))?;
    if identity(held)?!=(b.adopted.device,b.adopted.inode)||identity(&fresh)?!=identity(held)?||fresh.metadata()?.mode()&0o777!=0o600||fresh.metadata()?.uid()!=unsafe{libc::geteuid()}||fresh.metadata()?.nlink()!=1||held.metadata()?.nlink()!=1||hash_file(held,original.length)?!=original.sha256||hash_file(&fresh,original.length)?!=original.sha256{return Err(fail("Completed adopted file changed"));}
   }
   for role in [capture::CompletedDirectoryRole::AdmissionCapsule,capture::CompletedDirectoryRole::DirectStageDirectory] {
    let path=role.relative(attempt).map_err(|_|fail("Completed directory membership invalid"))?.ok_or_else(||fail("Completed record parent absent"))?;
    let names:std::collections::BTreeSet<String>=std::fs::read_dir(format!("/proc/self/fd/{}",self.directories[&path].as_raw_fd()))?.map(|e|e?.file_name().into_string().map_err(|_|fail("Completed record name invalid"))).collect::<io::Result<_>>()?;
    let expected:std::collections::BTreeSet<String>=scope.files.iter().filter_map(|b|b.role.relative(attempt).ok()).filter_map(|p|p.rsplit_once('/').filter(|(parent,_)|*parent==path).map(|(_,n)|n.to_owned())).collect();
    if names!=expected{return Err(fail("Completed record directory membership differs"));}
   }
   let receipt=&self.receipts[&scope.original.disposition["asset_id"]];
   for erased in [&receipt.intent.storage_path,&format!(".attachment-upload-journal/{attempt}/payload.part")] {
    let(parent,name)=erased.rsplit_once('/').ok_or_else(||fail("Completed erased path malformed"))?;
    let held=self.directories.get(parent).ok_or_else(||fail("Completed erased parent not held"))?;
    if exists(held,name)?{return Err(fail("Completed erased path resurrected"));}
   }
  }self.active.check()
 }
 pub(crate) fn originals(&self)->&std::collections::BTreeMap<String,crate::storage::attachment_upload::Receipt>{&self.receipts}
 pub(crate) async fn check_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()> {
  self.check_subset_in(conn).await?;
  let actual:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id").fetch_all(&mut *conn).await.map_err(|_|fail("Completed reservation census unavailable"))?;
  let mut expected:Vec<String>=self.roles.scopes.iter().map(|s|s.original.reservation.operation_id.clone()).collect();if let Some(native)=&self.native{expected.extend(native.attempts());}expected.sort();
  if actual!=expected{return Err(fail("Mixed completed reservation classes unsupported"));}
  let namespace=self.directories.get(".attachment-upload-admission").ok_or_else(||fail("Completed namespace absent"))?;
  let names:std::collections::BTreeSet<String>=std::fs::read_dir(format!("/proc/self/fd/{}",namespace.as_raw_fd()))?.map(|e|e?.file_name().into_string().map_err(|_|fail("Completed capsule name invalid"))).collect::<io::Result<_>>()?;
  if names!=expected.into_iter().collect(){return Err(fail("Completed full native capsule census differs"));}self.check()
 }
 pub(crate) fn covers_completed_attempt(&self,attempt:&str)->bool{self.roles.scopes.iter().any(|scope|scope.original.reservation.operation_id==attempt)}
 pub(crate) async fn check_subset_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()> {
  self.check()?;let(rows,pointer)=history_rows(conn).await?;
  let source=if self.active.engine.is_some(){HistorySource::new_with_check(&self.active.root,None,engine_transfer::engine_root_fence)?}else{HistorySource::new(&self.active.root,None)?};
  let closure=history_closure(source,identity(self.active.root.fd())?,Some((&rows,&pointer)))?;
  if closure.current_id!=self.active.ledger.generation_id||digest(&closure.generations[&closure.current_id].raw)!=self.active.ledger_sha256{return Err(fail("Completed finalized generation SQL differs"));}
  closure.source.fence()?;
  let envelope=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope{version:1,source_kind:"original_native_direct_v1".into(),scopes:self.roles.scopes.iter().map(|s|s.original.clone()).collect()};
  crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::check_completed_sql_subset(&envelope,&self.receipts,conn).await.map_err(|_|fail("Completed finalized full SQL inventory differs"))?;
  crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::check_cold_references(&self.active.root.absolute,&envelope,&self.receipts,conn).await.map_err(|_|fail("Completed restored current protected references differ"))?;
  if let Some(native)=&self.native{native.check_subset_in(conn).await?;}
  if history_rows(conn).await?!=(rows,pointer){return Err(fail("Completed generation SQL changed"));}self.check()
 }
}
pub(crate) async fn acquire_completed_absence_in(path:&Path,conn:&mut sqlx::AnyConnection)->io::Result<Option<CompletedAbsenceAuthority>> {
 let(_,active)=registered(path)?;if active.as_ref().is_none_or(|a|a.ledger.completed_absence.is_none()&&a.ledger.composite.is_none()){return Ok(None);}
 let family=Arc::new(crate::storage::native_family_fence::Creation::exclusive(path)?);
 let proof=acquire_completed_absence_subset_in(path,conn,family).await?;
 if let Some(proof)=&proof{proof.check_in(conn).await?;}Ok(proof)
}
// No caller-supplied rows can construct this token: it is rooted in registered
// original Active and exact immutable Ledger3 roles, under one shared exclusion.
pub(crate) async fn acquire_completed_absence_subset_in(path:&Path,conn:&mut sqlx::AnyConnection,family:Arc<crate::storage::native_family_fence::Creation>)->io::Result<Option<CompletedAbsenceAuthority>> {
 use crate::storage::attachment_upload::admission::committed_erasure::accounting::capture as capture;
 let(_,active)=registered(path)?;let Some(active)=active else{return Ok(None)};
 let Some(roles)=active.ledger.completed_absence.clone().or_else(||active.ledger.composite.as_ref().map(|c|c.completed.clone())) else{return Ok(None)};
 if !matches!(active.ledger.version,3|4)||active.ledger.admission.is_some(){return Err(fail("Completed mixed ledger ownership unsupported"));}
 roles.validate(&admission::Identity{device:active.ledger.root_device,inode:active.ledger.root_inode})?;
 if identity(family.root())?!=identity(active.root.fd())?{return Err(fail("Completed shared family root differs"));}
 let mut directories=std::collections::BTreeMap::new();let mut files=std::collections::BTreeMap::new();let mut receipts=std::collections::BTreeMap::new();
 for scope in &roles.scopes {
  let attempt=&scope.original.reservation.operation_id;
  for b in &scope.directories {let p=b.role.relative(attempt).map_err(|_|fail("Completed role invalid"))?;let key=p.clone().unwrap_or_default();if !directories.contains_key(&key){let f=if let Some(p)=p{active.root.directory(&p)?}else{active.root.fd().try_clone()?};if matches!(b.role,capture::CompletedDirectoryRole::AdmissionCapsule)&&unsafe{libc::flock(f.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}<0{return Err(io::Error::last_os_error());}directories.insert(key,f);}}
  for b in &scope.files{let p=b.role.relative(attempt).map_err(|_|fail("Completed file role invalid"))?;files.insert(p.clone(),active.root.leaf(&p)?);}
  let raw=capture::original_record_metadata(&scope.original).map_err(|_|fail("Completed original metadata invalid"))?;
  let receipt=capture::validate_original_records(&scope.original,&raw).map_err(|_|fail("Completed original receipt invalid"))?;receipts.insert(receipt.intent.asset_id.clone(),receipt);
 }
 let mut immutable_stamps=std::collections::BTreeMap::new();for(path,file)in &files{immutable_stamps.insert(path.clone(),physical_stamp(file)?);}
 for scope in &roles.scopes{for role in [capture::CompletedDirectoryRole::AdmissionCapsule,capture::CompletedDirectoryRole::DirectStageDirectory]{let path=role.relative(&scope.original.reservation.operation_id).map_err(|_|fail("Completed immutable role invalid"))?.ok_or_else(||fail("Completed immutable directory absent"))?;immutable_stamps.insert(path.clone(),physical_stamp(&directories[&path])?);}}
 let native=if active.ledger.version==4{Some(CompositeNativeAuthority::hold(active.clone(),conn).await?)}else{None};
 let proof=CompletedAbsenceAuthority{active,roles,native,family,directories,files,receipts,immutable_stamps};proof.check_subset_in(conn).await?;Ok(Some(proof))
}
