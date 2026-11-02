// Purpose-separated native census support, never serving/disposal/admission rebinding.
pub(crate) struct QualifiedErasureSource {active:Arc<Active>,entries:std::collections::BTreeSet<String>,request:Option<(File,PhysicalStamp,Vec<u8>)>,sqlite:Option<Identity>,family:Option<Arc<crate::storage::native_family_fence::Creation>>,completed:Option<Arc<CompletedAbsenceAuthority>>,root_stamp:PhysicalStamp}
impl QualifiedErasureSource {
 pub(crate) fn prior_completed_evidence(&self)->io::Result<PriorCompletedCaptureEvidence>{self.fence()?;let old=self.completed.as_ref().ok_or_else(||fail("Mixed capture lacks original restored absence token"))?;let root=identity(self.active.root.fd())?;let e=PriorCompletedCaptureEvidence{version:1,generation_id:self.active.ledger.generation_id.clone(),ledger_sha256:self.active.ledger_sha256.clone(),source_root:admission::Identity{device:root.0,inode:root.1},roles:old.roles.clone()};e.validate_shape()?;self.fence()?;Ok(e)}
 pub(crate) fn prior_composite_native(&self)->io::Result<Option<Vec<composite::NativeOwnerScope>>>{self.fence()?;let result=self.completed.as_ref().and_then(|c|c.native.as_ref()).map(|n|n.roles.native.clone());self.fence()?;Ok(result)}
 pub(crate) fn family(&self)->Option<Arc<crate::storage::native_family_fence::Creation>>{self.family.clone()}
 pub(crate) fn covers_completed_attempt(&self,attempt:&str)->bool{self.completed.as_ref().is_some_and(|p|p.covers_completed_attempt(attempt))}
 pub(crate) fn preserves_composite_attempt(&self,attempt:&str)->bool{self.completed.as_ref().and_then(|c|c.native.as_ref()).is_some_and(|n|n.attempts().contains(attempt))}
 pub(crate) fn composite_preserved_attempts(&self)->std::collections::BTreeSet<String>{self.completed.as_ref().and_then(|c|c.native.as_ref()).map(|n|n.attempts()).unwrap_or_default()}
 pub(crate) fn completed_attempts(&self)->std::collections::BTreeSet<String>{self.completed.as_ref().map(|p|p.roles.scopes.iter().map(|s|s.original.reservation.operation_id.clone()).collect()).unwrap_or_default()}
 pub(crate) fn owns_control_record(&self,path:&str,file:&File)->io::Result<bool>{
  self.fence()?;
  let result=if let Some(record)=self.active.control_files.get(path){physical_stamp(file)?==record.stamp&&physical_stamp(&record.held)?==record.stamp&&read_ledger(file)?==record.raw}else if let Some(engine)=&self.active.engine{match &**engine{EngineAuthority::Transfer(p)=>p.census_control_record(path,file)?,EngineAuthority::PgRestore(p)=>p.census_control_record(path,file)?}}else{false};self.fence()?;Ok(result)
 }
 pub(crate) fn exclude_control_references(&self,relative:&str,file:&File,asset:&str,path:&str)->io::Result<bool>{
  if !self.owns_control_record(relative,file)?{return Ok(false);}
  use std::io::{Read,Seek,SeekFrom};let mut raw=Vec::new();let mut copy=file.try_clone()?;copy.seek(SeekFrom::Start(0))?;copy.take(8*1024*1024+1).read_to_end(&mut raw)?;
  if raw.len()>8*1024*1024{return Err(fail("Certified control reference proof exceeds bound"));}
  let raw=std::str::from_utf8(&raw).map_err(|_|fail("Certified control is not UTF8"))?;
  let application_projection=match self.active.engine.as_deref(){Some(EngineAuthority::PgRestore(p))=>p.census_application_projection(relative,file)?,_=>false};
  let result=if application_projection{crate::storage::attachment_erasure_source::exclude_application_projection(raw,asset,path)}else{crate::storage::attachment_erasure_source::exclude_owned_control(raw,asset,path)};
  result.map_err(|e|fail(&format!("Certified historical control {relative} retains selected native asset or exceeds proof bound: {e}")))?;
  if !self.owns_control_record(relative,file)?{return Err(fail("Certified control changed through reference proof"));}Ok(true)
 }
 pub(crate) fn exclude_old_payload(&self,relative:&str,file:&File,designation:Option<&crate::storage::data_root_designation::Designation>)->io::Result<()> {
  let Some(old)=&self.completed else{return Ok(())};old.check()?;
  let metadata=file.metadata()?;let length=metadata.len();let needs_hash=old.roles.scopes.iter().any(|s|s.original.reservation.committed_bytes.is_some_and(|n|n>=0&&n as u64==length));
  let sha=if needs_hash{hash_file(file,length)?}else{String::new()};
  let envelope=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope{version:1,source_kind:"original_native_direct_v1".into(),scopes:old.roles.scopes.iter().map(|s|s.original.clone()).collect()};
  crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::exclude_completed_payload_inventory(&envelope,std::iter::once((metadata.dev(),metadata.ino(),length,sha.as_str(),designation.map(|d|(d,relative,file))))).map_err(|_|fail("Restored completed original payload retained by new native census"))?;old.check()
 }
 pub(crate) fn completed(&self)->Option<Arc<CompletedAbsenceAuthority>>{self.completed.clone()}
 pub(crate) fn covers_entry(&self,name:&str)->bool{self.entries.contains(name)}
 pub(crate) fn fence(&self)->io::Result<()> {self.active.check()?;if physical_stamp(self.active.root.fd())?!=self.root_stamp||physical_stamp(&Root::open(&self.active.root.absolute)?.chain.last().ok_or_else(||fail("Qualified root absent"))?.try_clone()?)?!=self.root_stamp{return Err(fail("Qualified native current root stamp changed"));}if let Some(c)=&self.completed{c.check()?;}if let Some(id)=&self.sqlite{if identity(self.active.sqlite_main.as_ref().ok_or_else(||fail("Qualified original SQLite main FD absent"))?)?!=*id||qualified_sqlite_path_identity(&self.active.root)?!=*id{return Err(fail("Qualified original SQLite main inode differs"));}}if let Some((held,stamp,raw))=&self.request{let fresh=self.active.root.leaf(ENGINE_REQUEST)?;if physical_stamp(held)?!=*stamp||physical_stamp(&fresh)?!=*stamp||read_ledger(&fresh)?!=*raw{return Err(fail("Held qualified engine request changed"));}}self.active.check()}
 pub(crate) async fn fence_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()> {self.fence()?;if let Some(engine)=&self.active.engine {match &**engine{EngineAuthority::Transfer(p)=>p.verify_any_connection(conn).await.map_err(|_|fail("Qualified original engine SQL differs"))?,EngineAuthority::PgRestore(p)=>p.verify_any_connection(conn).await.map_err(|_|fail("Qualified restored engine SQL differs"))?};}else{qualified_sqlite_connection(&self.active.root,conn).await?;}if let Some(c)=&self.completed{c.check_subset_in(conn).await?;}let(rows,current)=history_rows(conn).await?;let source=HistorySource::new_with_check(&self.active.root,None,if self.active.engine.is_some(){engine_transfer::engine_root_fence}else{Root::check})?;let closure=history_closure(source,identity(self.active.root.fd())?,Some((&rows,&current)))?;if closure.current_id!=self.active.ledger.generation_id||digest(&closure.generations[&closure.current_id].raw)!=self.active.ledger_sha256{return Err(fail("Qualified native census generation SQL differs"));}closure.source.fence()?;self.fence()}
}
pub(crate) async fn qualified_erasure_source_in(path:&Path,conn:&mut sqlx::AnyConnection)->io::Result<Option<Arc<QualifiedErasureSource>>> {
 let(root,active)=registered(path)?;let Some(active)=active else{return Ok(None)};
 if ![1,3,4].contains(&active.ledger.version)||active.ledger.admission.is_some()||(active.ledger.version==3)!=active.ledger.completed_absence.is_some()||(active.ledger.version==4)!=active.ledger.composite.is_some(){return Err(fail("Qualified native erasure requires original Ledger1 or finalized completed Ledger3 source"));}
 let mut entries=if let Some(engine)=&active.engine {match &**engine{EngineAuthority::Transfer(p)=>{p.verify_any_connection(conn).await.map_err(|_|fail("Qualified native original connected target differs"))?;p.census_control_entries()?},EngineAuthority::PgRestore(p)=>{p.verify_any_connection(conn).await.map_err(|_|fail("Qualified restored connected target differs"))?;p.census_control_entries()?}}}else{qualified_sqlite_connection(&root,conn).await?;active.control_files.keys().chain(active.control_directories.keys()).filter_map(|p|p.split('/').next().map(str::to_owned)).collect()};
 // Stabilize exact current control SQL through the existing native mutation transaction.
 if let Some(engine)=&active.engine {let lock=match &**engine{EngineAuthority::Transfer(_)=>"LOCK TABLE engine_transfer_import_commitments,engine_transfer_generations,engine_transfer_current,storage_restore_generations,storage_restore_current IN SHARE MODE",EngineAuthority::PgRestore(_)=>"LOCK TABLE pg_restoration_imports,pg_restoration_generations,pg_restoration_current,storage_restore_generations,storage_restore_current IN SHARE MODE"};sqlx::query(lock).execute(&mut *conn).await.map_err(|_|fail("Qualified source control exclusion unavailable"))?;}
 let request=if exists(root.fd(),ENGINE_REQUEST)?{let held=root.leaf(ENGINE_REQUEST)?;let stamp=physical_stamp(&held)?;let raw=read_ledger(&held)?;if stamp.mode&0o777!=0o600||stamp.links!=1||raw!=b"PostgreSQL transfer requested; ordinary SQLite startup forbidden\n"{return Err(fail("Qualified engine refusal marker malformed"));}entries.insert(ENGINE_REQUEST.into());Some((held,stamp,raw))}else{None};
 // Current SQLite bytes are inert under this original engine authority, but remain in alias/hash census.
 if active.engine.is_some()&&exists(root.fd(),"starlane-notes.db")?{entries.insert("starlane-notes.db".into());}
 let sqlite=if active.engine.is_none(){Some(identity(active.sqlite_main.as_ref().ok_or_else(||fail("Qualified original SQLite main FD absent"))?)?)}else{None};
 let family=if matches!(active.ledger.version,3|4){Some(Arc::new(crate::storage::native_family_fence::Creation::exclusive(path)?))}else{None};
 let completed=if let Some(guard)=&family{Some(Arc::new(acquire_completed_absence_subset_in(path,conn,guard.clone()).await?.ok_or_else(||fail("Qualified completed source owner absent"))?))}else{None};
 let root_stamp=physical_stamp(root.fd())?;let proof=Arc::new(QualifiedErasureSource{active,entries,request,sqlite,family,completed,root_stamp});proof.fence_in(conn).await?;Ok(Some(proof))
}

async fn qualified_sqlite_connection(root:&Root,conn:&mut sqlx::AnyConnection)->io::Result<()> {
 use sqlx::Row;
 let rows=sqlx::query("PRAGMA database_list").fetch_all(&mut *conn).await.map_err(|_|fail("Qualified source is not SQLite"))?;
 let main=rows.iter().find(|r|r.try_get::<String,_>("name").ok().as_deref()==Some("main")).ok_or_else(||fail("Qualified SQLite main absent"))?;
 let path:String=main.try_get("file").map_err(|_|fail("Qualified SQLite main path unavailable"))?;
 let expected=root.absolute.join("starlane-notes.db");
 if Path::new(&path)!=expected{return Err(fail("Qualified original connected SQLite target differs"));}Ok(())
}

// fstatat performs no secondary main-file open/close: SQLite POSIX locks belong
// to the process and must not be released by a temporary proof descriptor.
fn qualified_sqlite_path_identity(root:&Root)->io::Result<Identity>{
 let mut stat:libc::stat=unsafe{std::mem::zeroed()};let name=CString::new("starlane-notes.db").map_err(|_|fail("Invalid SQLite main component"))?;
 if unsafe{libc::fstatat(root.fd().as_raw_fd(),name.as_ptr(),&mut stat,libc::AT_SYMLINK_NOFOLLOW)}<0{return Err(io::Error::last_os_error());}
 if stat.st_mode&libc::S_IFMT!=libc::S_IFREG{return Err(fail("Qualified SQLite main is not regular nofollow file"));}Ok((stat.st_dev as u64,stat.st_ino as u64))
}

#[cfg(test)] mod projection_purpose_tests {
 use super::*;
 #[tokio::test]
 #[ignore="genuine closed restored PG source, original active/projection descriptor purpose"]
 async fn actual_pg_projection_foreign_descriptor_and_wrong_role_refuse(){
  sqlx::any::install_default_drivers();let root=crate::storage::data_dir();let url=std::env::var("REPLAY_CAPACITY_TEST_PG_URL").unwrap();let serving=open_restored_pg_serving(&root,&url).await.unwrap();let pool=engine_transfer::pg_completed_test_restored_pool(&serving);let mut tx=pool.begin().await.unwrap();let q=qualified_erasure_source_in(&root,&mut tx).await.unwrap().unwrap();q.fence_in(&mut tx).await.unwrap();let Some(EngineAuthority::PgRestore(p))=q.active.engine.as_deref()else{panic!("Not genuine original restored PG engine")};
  // Use only the original opaque classifier. Traversal names do not confer a role.
  let scratch=std::env::temp_dir().join(format!("projection-purpose-foreign-{}",uuid::Uuid::new_v4().simple()));use std::os::unix::fs::DirBuilderExt;std::fs::DirBuilder::new().mode(0o700).create(&scratch).unwrap();let mut admitted=0;
  let mut candidates=Vec::new();for entry in std::fs::read_dir(root.join(".pg-restoration-custody")).unwrap(){let entry=entry.unwrap();if entry.file_type().unwrap().is_dir(){let candidate=entry.path().join("application-rows.json");if candidate.is_file(){candidates.push(candidate.strip_prefix(&root).unwrap().to_str().unwrap().to_owned());}}}
  for relative in &candidates {
   let held=File::open(root.join(relative)).unwrap();if !p.census_application_projection(relative,&held).unwrap(){continue;}
   assert!(!p.census_application_projection("unowned/application-rows.json",&held).unwrap());
   let foreign=scratch.join(format!("{}.json",admitted));std::fs::copy(root.join(relative),&foreign).unwrap();let other=File::open(&foreign).unwrap();assert_eq!(read_ledger(&held).unwrap(),read_ledger(&other).unwrap());assert!(!p.census_application_projection(relative,&other).unwrap(),"Equal-byte foreign file minted application projection role");
   let manifest=relative.strip_suffix("application-rows.json").unwrap().to_owned()+"manifest.json";let metadata=File::open(root.join(&manifest)).unwrap();assert!(!p.census_application_projection(&manifest,&metadata).unwrap());assert!(!p.census_application_projection(relative,&metadata).unwrap());admitted+=1;
  }
  assert!(admitted>=3,"Genuine repeated source projection chain missing");q.fence_in(&mut tx).await.unwrap();drop(q);tx.rollback().await.unwrap();pool.close().await;println!("ACTUAL_PG_AUTHENTICATED_PROJECTION_ROLE_WRONG_PURPOSE_EQUAL_BYTE_FOREIGN_DESCRIPTOR_REFUSALS_PASS roles={admitted}");
 }
}
