// Accounting-only finalized native component. No WriterLease is reconstructed.
struct CompositeNativeAuthority {
 active:Arc<Active>,roles:composite::CompositeOwnerEnvelope,
 directories:std::collections::BTreeMap<String,File>,files:std::collections::BTreeMap<String,File>,
 immutable:std::collections::BTreeMap<String,PhysicalStamp>,
 records:Vec<crate::storage::attachment_upload::admission::CompositeRecordEvidence>,
}
impl CompositeNativeAuthority {
 async fn hold(active:Arc<Active>,conn:&mut sqlx::AnyConnection)->io::Result<Self>{
  use std::collections::BTreeMap;use std::io::Read;
  active.check()?;let roles=active.ledger.composite.clone().ok_or_else(||fail("Finalized composite roles absent"))?;roles.validate(&admission::Identity{device:active.ledger.root_device,inode:active.ledger.root_inode})?;
  if roles.version==1&&roles.native.iter().any(|s|!matches!(s.source_basis,composite::NativeSourceBasis::Original)){return Err(fail("Prior composite native lineage needs its separate retained source constructor"));}
  if active.ledger.version!=4||active.ledger.admission.is_some()||active.ledger.completed_absence.is_some(){return Err(fail("Finalized composite canonical class differs"));}
  match active.engine.as_deref(){Some(EngineAuthority::PgRestore(p))=>p.verify_any_connection(conn).await.map_err(|_|fail("Composite original PG target differs"))?,Some(EngineAuthority::Transfer(_))=>return Err(fail("Composite finalized Transfer class unsupported")),None=>{qualified_sqlite_connection(&active.root,conn).await?;if qualified_sqlite_path_identity(&active.root)?!=identity(active.sqlite_main.as_ref().ok_or_else(||fail("Composite original SQLite main FD absent"))?)?{return Err(fail("Composite original main identity differs"));}}}
  let mut directories=BTreeMap::new();let mut files=BTreeMap::new();let mut immutable=BTreeMap::new();let mut records=Vec::new();
  for scope in &roles.native {
   let attempt=&scope.original.reservation.operation_id;
   for b in &scope.directories {let path=b.role.relative(attempt)?.unwrap_or_default();if !directories.contains_key(&path){let held=if path.is_empty(){active.root.fd().try_clone()?}else{active.root.directory(&path)?};if identity(&held)?!=(b.adopted.device,b.adopted.inode){return Err(fail("Composite adopted directory identity differs"));}if matches!(b.role,composite::DirectoryRole::AdmissionCapsule){if unsafe{libc::flock(held.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}<0{return Err(io::Error::last_os_error());}}if matches!(b.role,composite::DirectoryRole::AdmissionCapsule|composite::DirectoryRole::DirectStageDirectory){immutable.insert(path.clone(),physical_stamp(&held)?);}directories.insert(path,held);}}
   let mut raws=BTreeMap::new();
   for b in &scope.files {let path=b.role.relative(attempt)?;let original=scope.original.files.iter().find(|v|v.role==b.role).ok_or_else(||fail("Composite original metadata role absent"))?;let held=active.root.leaf(&path)?;let stamp=physical_stamp(&held)?;if identity(&held)?!=(b.adopted.device,b.adopted.inode)||stamp.length!=original.length||stamp.links!=1||stamp.mode&0o777!=0o600||hash_file(&held,original.length)?!=original.sha256{return Err(fail("Composite adopted immutable metadata differs"));}let mut raw=Vec::new();held.try_clone()?.read_to_end(&mut raw)?;raws.insert(b.role.clone(),raw);immutable.insert(path.clone(),stamp);files.insert(path,held);}
   let record=crate::storage::attachment_upload::admission::validate_composite_native_records(&scope.original,&raws)?;
   if let Some(pending)=&scope.pending {
    let mut raw=std::collections::BTreeMap::new();
    for b in &pending.files {let path=b.role.relative(attempt)?;let held=active.root.leaf(&path)?;let stamp=physical_stamp(&held)?;if identity(&held)?!=(b.adopted.device,b.adopted.inode)||stamp.length!=b.length||stamp.mode&0o777!=0o600||stamp.links!=1||hash_file(&held,b.length)?!=b.sha256{return Err(fail("Composite finalized pending role bytes/identity differ"));}let stream=matches!(&b.role,admission::FileRole::DirectStage{name}if name=="payload.part");if stream{crate::storage::attachment_upload::admission::verify_captured_direct_payload(&held,b.length,&b.sha256,pending.maximum_bytes)?;raw.insert(b.role.clone(),Vec::new());}else{let mut bytes=Vec::new();held.try_clone()?.read_to_end(&mut bytes)?;raw.insert(b.role.clone(),bytes);}immutable.insert(path.clone(),stamp);files.insert(path,held);}
    crate::storage::attachment_upload::admission::validate_captured_partial_direct(pending,&raw)?;
   }
   if let composite::NativeClass::CommittedDirect{receipt_json}=&scope.original.class {let receipt:crate::storage::attachment_upload::Receipt=serde_json::from_str(receipt_json).map_err(|_|fail("Composite finalized committed receipt malformed"))?;for path in [receipt.intent.storage_path,format!(".attachment-upload-journal/{attempt}/payload.part")]{let held=active.root.leaf(&path)?;immutable.insert(path.clone(),physical_stamp(&held)?);files.insert(path,held);}}
   records.push(record);
  }
  let proof=Self{active,roles,directories,files,immutable,records};proof.check_subset_in(conn).await?;Ok(proof)
 }
 fn attempts(&self)->std::collections::BTreeSet<String>{self.roles.native.iter().map(|s|s.original.reservation.operation_id.clone()).collect()}
 fn check(&self)->io::Result<()> {
  self.active.check()?;
  for scope in &self.roles.native {let attempt=&scope.original.reservation.operation_id;for b in &scope.directories {let path=b.role.relative(attempt)?.unwrap_or_default();let held=&self.directories[&path];let fresh=if path.is_empty(){self.active.root.fd().try_clone()?}else{self.active.root.directory(&path)?};if identity(held)?!=(b.adopted.device,b.adopted.inode)||identity(&fresh)?!=identity(held)?{return Err(fail("Composite finalized directory replaced"));}}}
  for(path,stamp)in &self.immutable {let held=self.files.get(path).or_else(||self.directories.get(path)).ok_or_else(||fail("Composite original immutable FD missing"))?;let fresh=if self.files.contains_key(path){self.active.root.leaf(path)?}else{self.active.root.directory(path)?};if physical_stamp(held)?!=*stamp||physical_stamp(&fresh)?!=*stamp{return Err(fail("Composite immutable adopted record/directory stamp changed"));}}
  self.active.check()
 }
 async fn check_subset_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()> {
  self.check()?;
  let expected=self.roles.completed.scopes.iter().map(|s|s.original.reservation.operation_id.clone()).chain(self.attempts()).collect::<std::collections::BTreeSet<_>>();let actual:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id LIMIT 513").fetch_all(&mut *conn).await.map_err(|_|fail("Composite finalized reservation inventory unavailable"))?;if actual.len()>512||expected.iter().any(|id|!actual.contains(id)){return Err(fail("Composite finalized old reservation omitted"));}
  let namespace=self.directories.get(".attachment-upload-admission").ok_or_else(||fail("Composite namespace not held"))?;let actual=prior_capsule_names(namespace)?;if actual.len()>512||!expected.is_subset(&actual){return Err(fail("Composite finalized old capsule omitted"));}
  for scope in &self.roles.native {let attempt=&scope.original.reservation.operation_id;let mut paths=scope.original.files.iter().map(|b|b.role.relative(attempt)).collect::<io::Result<std::collections::BTreeSet<_>>>()?;if let Some(p)=&scope.pending{for b in &p.files{paths.insert(b.role.relative(attempt)?);}}if matches!(scope.original.class,composite::NativeClass::CommittedDirect{..}){paths.insert(format!(".attachment-upload-journal/{attempt}/payload.part"));}for prefix in [format!(".attachment-upload-admission/{attempt}"),format!(".attachment-upload-journal/{attempt}")]{if let Some(dir)=self.directories.get(&prefix){let expected=paths.iter().filter_map(|p|p.strip_prefix(&(prefix.clone()+"/")).map(str::to_owned)).collect::<std::collections::BTreeSet<_>>();let actual=prior_capsule_names(dir)?;if expected!=actual{return Err(fail("Composite finalized operation membership differs"));}}}}
  let inventory=CompositeRuntimeInventory::capture(&self.active)?;let paths=inventory.files.iter().map(|(path,_,_)|path.clone()).collect();let designation=crate::storage::data_root_designation::Designation::capture_held(&self.active.root.absolute,self.active.root.fd())?;
  for record in &self.records {for(path,file,_)in &inventory.files {record.exclude_disposed_payload_file(path,file,designation.as_ref())?;}record.validate_finalized_native_in(&self.active.root.absolute,conn,&paths).await.map_err(|_|fail("Composite finalized native absence/current SQL/reference differs"))?;}
  inventory.fence(&self.active)?;if let Some(d)=&designation{d.fence()?;}self.check()
 }
}
// Read the complete actual root while retaining every observed FD. The original
// SQLite main is recognized before open via fstatat and never reopened/closed.
struct CompositeRuntimeInventory {files:Vec<(String,File,PhysicalStamp)>,directories:Vec<(String,File,PhysicalStamp)>}
impl CompositeRuntimeInventory {
 fn capture(active:&Active)->io::Result<Self>{
  let mut result=Self{files:Vec::new(),directories:Vec::new()};let main=active.sqlite_main.as_ref().map(|f|identity(f)).transpose()?;result.walk(active,active.root.fd(),"",main,0)?;result.fence(active)?;Ok(result)
 }
 fn walk(&mut self,active:&Active,dir:&File,base:&str,main:Option<Identity>,depth:usize)->io::Result<()> {
  if depth>32||self.files.len()+self.directories.len()>16384{return Err(fail("Composite complete runtime census exceeds bound"));}
  let held=dir.try_clone()?;let before=physical_stamp(&held)?;let names=prior_capsule_names(&held)?;
  for name in names {if self.files.len()+self.directories.len()>16384{return Err(fail("Composite runtime entry bound exceeded"));}let path=if base.is_empty(){name.clone()}else{format!("{base}/{name}")};let mut st:libc::stat=unsafe{std::mem::zeroed()};let c=CString::new(name.as_str()).map_err(|_|fail("Composite census component NUL"))?;if unsafe{libc::fstatat(dir.as_raw_fd(),c.as_ptr(),&mut st,libc::AT_SYMLINK_NOFOLLOW)}<0{return Err(io::Error::last_os_error());}let id=(st.st_dev as u64,st.st_ino as u64);
   if Some(id)==main {if path!="starlane-notes.db"||qualified_sqlite_path_identity(&active.root)?!=id{return Err(fail("Composite original main aliased or replaced"));}continue;}
   let kind=st.st_mode as u32&libc::S_IFMT;if kind==libc::S_IFDIR {let child=active.root.directory(&path)?;self.walk(active,&child,&path,main,depth+1)?;}else if kind==libc::S_IFREG {let file=active.root.leaf(&path)?;let stamp=physical_stamp(&file)?;if (stamp.device,stamp.inode)!=id{return Err(fail("Composite census file changed while opening"));}self.files.push((path,file,stamp));}else{return Err(fail("Composite runtime census special file retains review"));}
  }
  if physical_stamp(&held)?!=before{return Err(fail("Composite runtime directory changed while scanning"));}self.directories.push((base.into(),held,before));Ok(())
 }
 fn fence(&self,active:&Active)->io::Result<()> {
  active.check()?;let mut groups=std::collections::BTreeMap::<(u64,u64),(u64,u64)>::new();for(path,file,stamp)in &self.files {if physical_stamp(file)?!=*stamp||physical_stamp(&active.root.leaf(path)?)?!=*stamp{return Err(fail("Composite full runtime file changed"));}let e=groups.entry((stamp.device,stamp.inode)).or_insert((0,stamp.links));e.0+=1;if e.1!=stamp.links{return Err(fail("Composite full runtime link stamps disagree"));}}
  if groups.values().any(|(count,links)|count!=links){return Err(fail("Composite runtime payload has unaccounted foreign alias"));}
  for(path,dir,stamp)in &self.directories {let fresh=if path.is_empty(){active.root.fd().try_clone()?}else{active.root.directory(path)?};if physical_stamp(dir)?!=*stamp||physical_stamp(&fresh)?!=*stamp{return Err(fail("Composite complete directory topology changed"));}}active.check()
 }
}
