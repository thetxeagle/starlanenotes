// Purpose-private original Ledger3 physical material. Never a current-reference token.
// No CompletedAbsenceAuthority is constructed before its strict checks.
pub(crate) struct HistoricalCompletedPhysical {
 active:Arc<Active>,roles:completed::CompletedOwnerEnvelope,
 family:Arc<crate::storage::native_family_fence::Creation>,
 directories:std::collections::BTreeMap<String,File>,files:std::collections::BTreeMap<String,File>,
 receipts:std::collections::BTreeMap<String,crate::storage::attachment_upload::Receipt>,
 immutable_stamps:std::collections::BTreeMap<String,PhysicalStamp>,inventory:HistoricalSourceInventory,
}
impl HistoricalCompletedPhysical {
 pub(crate) async fn hold_in(path:&Path,conn:&mut sqlx::AnyConnection,family:Arc<crate::storage::native_family_fence::Creation>)->io::Result<Self>{
  use crate::storage::attachment_upload::admission::committed_erasure::accounting::capture as capture;
  let(_,active)=registered(path)?;let active=active.ok_or_else(||fail("Historical preservation requires original registered Active"))?;
  if active.ledger.version!=3||active.ledger.admission.is_some()||active.ledger.composite.is_some(){return Err(fail("Historical v1 requires exact original Ledger3 completed source"));}
  let roles=active.ledger.completed_absence.clone().ok_or_else(||fail("Historical original completed roles absent"))?;
  roles.validate(&admission::Identity{device:active.ledger.root_device,inode:active.ledger.root_inode})?;
  if identity(family.root())?!=identity(active.root.fd())?{return Err(fail("Historical original family/root differs"));}
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

  let inventory=HistoricalSourceInventory::capture(&active)?;let proof=Self{active,roles,family,directories,files,receipts,immutable_stamps,inventory};proof.recheck_in(conn).await?;Ok(proof)
 }
 pub(crate) fn check(&self)->io::Result<()> {
  use crate::storage::attachment_upload::admission::committed_erasure::accounting::capture as capture;
  self.active.check()?;self.inventory.fence(&self.active)?;
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

 pub(crate) async fn recheck_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()> {
  self.check()?;
  match self.active.engine.as_deref(){Some(EngineAuthority::PgRestore(p))=>p.verify_any_connection(conn).await.map_err(|_|fail("Historical actual PG source differs"))?,Some(EngineAuthority::Transfer(_))=>return Err(fail("Historical v1 excludes Transfer source")),None=>{qualified_sqlite_connection(&self.active.root,conn).await?;if qualified_sqlite_path_identity(&self.active.root)?!=identity(self.active.sqlite_main.as_ref().ok_or_else(||fail("Historical retained original main FD absent"))?)?{return Err(fail("Historical actual SQLite source main differs"));}}}
  let(rows,pointer)=history_rows(conn).await?;
  let source=HistorySource::new_with_check(&self.active.root,None,if self.active.engine.is_some(){engine_transfer::engine_root_fence}else{Root::check})?;
  let closure=history_closure(source,identity(self.active.root.fd())?,Some((&rows,&pointer)))?;
  if closure.current_id!=self.active.ledger.generation_id||digest(&closure.generations[&closure.current_id].raw)!=self.active.ledger_sha256{return Err(fail("Historical original source generation differs"));}closure.source.fence()?;
  use crate::storage::attachment_upload::admission::committed_erasure::accounting::capture as capture;
  let envelope=capture::CompletedAbsenceEnvelope{version:1,source_kind:"original_native_direct_v1".into(),scopes:self.roles.scopes.iter().map(|s|s.original.clone()).collect()};
  // Exact immutable SQL and physical fact, deliberately not current-reference proof.
  capture::check_completed_sql_subset(&envelope,&self.receipts,conn).await.map_err(|_|fail("Historical full immutable completed SQL differs"))?;
  let actual:Vec<String>=sqlx::query_scalar("SELECT asset_id FROM attachment_erasure_dispositions ORDER BY asset_id LIMIT 513").fetch_all(&mut *conn).await.map_err(|_|fail("Historical disposition census unavailable"))?;
  if actual.len()!=self.receipts.len()||actual.iter().any(|a|!self.receipts.contains_key(a)){return Err(fail("Historical source has unsupported additional completed owners"));}
  for asset in self.receipts.keys(){let n:i64=sqlx::query_scalar("SELECT count(*) FROM attachments WHERE id=$1").bind(asset).fetch_one(&mut *conn).await.map_err(|_|fail("Historical current association unavailable"))?;if n!=0{return Err(fail("Historical erased original has new live association"));}}
  let inventory=&self.inventory;
  let designation=crate::storage::data_root_designation::Designation::capture_held(&self.active.root.absolute,self.active.root.fd())?;
  for(path,file,stamp)in &inventory.files {
   let sha=if self.roles.scopes.iter().any(|s|s.original.reservation.committed_bytes==Some(stamp.length as i64)){hash_file(file,stamp.length)?}else{String::new()};
   capture::exclude_completed_payload_inventory(&envelope,std::iter::once((stamp.device,stamp.inode,stamp.length,sha.as_str(),designation.as_ref().map(|d|(d,path.as_str(),file))))).map_err(|_|fail("Historical erased payload inode/bytes resurrected"))?;
  }
  inventory.fence(&self.active)?;if let Some(d)=&designation{d.fence()?;}
  if history_rows(conn).await?!=(rows,pointer){return Err(fail("Historical original generation changed"));}self.check()
 }
 pub(crate) fn evidence(&self)->io::Result<PriorCompletedCaptureEvidence>{self.check()?;let root=identity(self.active.root.fd())?;let e=PriorCompletedCaptureEvidence{version:1,generation_id:self.active.ledger.generation_id.clone(),ledger_sha256:self.active.ledger_sha256.clone(),source_root:admission::Identity{device:root.0,inode:root.1},roles:self.roles.clone()};e.validate_shape()?;self.check()?;Ok(e)}
 pub(crate) async fn application_projection_in(&self,conn:&mut sqlx::AnyConnection)->anyhow::Result<String>{
  self.recheck_in(conn).await?;
  let engine=match self.active.engine.as_deref(){Some(EngineAuthority::PgRestore(p))=>Some(p.as_ref()),Some(EngineAuthority::Transfer(_))=>return Err(anyhow::anyhow!("Historical v1 excludes Transfer projection")),None=>None};
  let result=engine_transfer::historical_application_projection_in(conn,engine).await?;self.recheck_in(conn).await?;Ok(result)
 }
 pub(crate) fn completed_attempts(&self)->std::collections::BTreeSet<String>{self.roles.scopes.iter().map(|s|s.original.reservation.operation_id.clone()).collect()}
 pub(crate) async fn check_native_component_in(&self,record:&crate::storage::attachment_upload::admission::CompositeRecordEvidence,conn:&mut sqlx::AnyConnection)->io::Result<()> {
  self.recheck_in(conn).await?;let inventory=&self.inventory;let paths=inventory.files.iter().map(|(p,_,_)|p.clone()).collect();record.validate_snapshot_row_and_paths_in(conn,&paths).await.map_err(|_|fail("Historical native full SQL/path differs"))?;
  let designation=crate::storage::data_root_designation::Designation::capture_held(&self.active.root.absolute,self.active.root.fd())?;
  for(path,file,_)in &inventory.files{record.exclude_disposed_payload_file(path,file,designation.as_ref())?;}
  inventory.fence(&self.active)?;if let Some(d)=&designation{d.fence()?;}self.recheck_in(conn).await
 }
}
