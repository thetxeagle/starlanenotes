// Linux open-file-description range lease, independent of SQLite's
// process-associated locks. Ordinary open/read/close physical fences must not
// release source commit exclusion. Ranges match the bundled Unix SQLite VFS.
struct SqliteCaptureCommitLease { file: File, start: libc::off_t, length: libc::off_t }
impl SqliteCaptureCommitLease {
 fn hold(file:&File,start:libc::off_t,length:libc::off_t)->io::Result<Self> {
  let md=file.metadata()?;
  if !md.is_file()||md.uid()!=unsafe{libc::geteuid()}||md.nlink()!=1{return Err(fail("Source pager lease requires the original exclusive regular file"));}
  let file=file.try_clone()?;
  let lock=libc::flock{l_type:libc::F_WRLCK as _,l_whence:libc::SEEK_SET as _,l_start:start,l_len:length,l_pid:0};
  if unsafe{libc::fcntl(file.as_raw_fd(),libc::F_OFD_SETLK,&lock)}<0{return Err(io::Error::last_os_error());}
  Ok(Self{file,start,length})
 }
}
impl Drop for SqliteCaptureCommitLease {
 fn drop(&mut self) {
  let lock=libc::flock{l_type:libc::F_UNLCK as _,l_whence:libc::SEEK_SET as _,l_start:self.start,l_len:self.length,l_pid:0};
  // The owned File remains alive through unlock; all acquisition/failure paths
  // also release automatically when its last description closes.
  unsafe{libc::fcntl(self.file.as_raw_fd(),libc::F_OFD_SETLK,&lock);}
 }
}

include!("restoration_sqlite_source_history.rs");
include!("restoration_sqlite_source_lock_tests.rs");
include!("restoration_sqlite_capture_helper_owner.rs");

// Original stopped source only. Prepared/published destination inventories must
// remain ordinary observations: their manifest/marker/rename writes are legitimate.
struct SqliteOriginalSourceCustody {
 cancelled:std::sync::atomic::AtomicBool,
 history: source_history::History,
 reads: crate::storage::file_read_custody::CustodySet,
 inventory: SqliteCaptureInventory,
}
impl SqliteOriginalSourceCustody {
 fn hold(root:&Root)->io::Result<std::sync::Arc<Self>> {
  let history=source_history::History::new(root)?;
  let reads=crate::storage::file_read_custody::CustodySet::new(crate::storage::file_read_custody::admit_local_filesystem)?;
  let inventory=sqlite_capture_inventory_observed(root,Some((&history,&reads)))?;
  let out=std::sync::Arc::new(Self{cancelled:std::sync::atomic::AtomicBool::new(false),history,reads,inventory});
  out.fence(root)?;Ok(out)
 }
 // Suitable for SQLite callbacks: no SQL, pathname baseline or hash walk, and
 // every invocation consults the original sticky kernel observation channels.
 fn cancel(&self) {self.cancelled.store(true,std::sync::atomic::Ordering::SeqCst);}
 fn io_fence(&self)->io::Result<()> {
  if self.cancelled.load(std::sync::atomic::Ordering::SeqCst){return Err(fail("Original SQLite capture cancelled"));}
  self.history.fence()?;self.reads.fence()?;self.history.fence()?;
  if self.cancelled.load(std::sync::atomic::Ordering::SeqCst){return Err(fail("Original SQLite capture cancelled"));}Ok(())
 }
 fn fence(&self,root:&Root)->io::Result<()> {self.io_fence()?;self.inventory.fence(root)?;self.io_fence()}
}

// Private standard SQLite producer. Capture transport never mints runtime ownership.
const SQLITE_CAPTURE_HELPER: &[u8] = include_bytes!("sqlite_backup_capture_helper.py");
const SQLITE_CAPTURE_PYTHON: &str = "/usr/bin/python3";
struct SqliteCaptureInventory {
 root_stamp:PhysicalStamp,
 directories:std::collections::BTreeMap<String,(File,PhysicalStamp)>,
 files:std::collections::BTreeMap<String,(File,PhysicalStamp,String)>,
}
// Prepared bundle directories deliberately retain an incomplete marker. This private
// inventory observation checks ancestry only; source authority uses strict Root::check.
fn sqlite_capture_ancestry(root:&Root)->io::Result<()> {
 let observed=Root::open(&root.absolute)?;
 if root.chain.len()!=observed.chain.len(){return Err(fail("Private capture ancestry changed"));}
 for(a,b)in root.chain.iter().zip(&observed.chain){if identity(a)?!=identity(b)?{return Err(fail("Private capture ancestry changed"));}}
 Ok(())
}
fn sqlite_capture_inventory(root:&Root)->io::Result<SqliteCaptureInventory> {
 sqlite_capture_inventory_observed(root,None)
}
fn sqlite_capture_inventory_observed(root:&Root,custody:Option<(&source_history::History,&crate::storage::file_read_custody::CustodySet)>)->io::Result<SqliteCaptureInventory> {
 fn walk(fd:&File,prefix:&str,out:&mut SqliteCaptureInventory,hashes:&mut std::collections::BTreeMap<Identity,(PhysicalStamp,String)>,custody:Option<(&source_history::History,&crate::storage::file_read_custody::CustodySet)>)->io::Result<()> {
  if let Some((history,_))=custody {history.watch_directory(fd,prefix)?;}
  for e in std::fs::read_dir(format!("/proc/self/fd/{}",fd.as_raw_fd()))? {
   let name=e?.file_name().into_string().map_err(|_|fail("NonUTF8 capture source"))?;
   if !valid_relative(&name)||name.contains(['/','\\']){return Err(fail("Unsafe capture source name"));}
   if prefix.is_empty()&&["logging","logs","search_index"].contains(&name.as_str()){continue;}
   let path=if prefix.is_empty(){name.clone()}else{format!("{prefix}/{name}")};
   let f=open_at(fd,&name,libc::O_RDONLY|libc::O_NONBLOCK)?;let md=f.metadata()?;
   if let Some((history,reads))=custody {if md.is_file(){history.watch_file(&f)?;reads.seal(&f)?;}else if md.is_dir(){history.watch_directory(&f,&path)?;}}
   let stamp=physical_stamp(&f)?;
   if md.uid()!=unsafe{libc::geteuid()}{return Err(fail("Capture source owner differs"));}
   if md.is_dir(){if md.mode()&0o777!=0o700{return Err(fail("Capture source directory is not private"));}walk(&f,&path,out,hashes,custody)?;out.directories.insert(path,(f,stamp));}
   else if md.is_file(){if md.mode()&0o777!=0o600||md.nlink()==0{return Err(fail("Capture source file is not private/exclusive"));}let sha=verification_pager_walk_hash(&f,&stamp,hashes)?;out.files.insert(path,(f,stamp,sha));}
   else{return Err(fail("Capture source has nonregular evidence"));}
  }Ok(())
 }
 let mut out=SqliteCaptureInventory{root_stamp:physical_stamp(root.fd())?,directories:Default::default(),files:Default::default()};walk(root.fd(),"",&mut out,&mut Default::default(),custody)?;
 // Notes deliberately links canonical files to native journal anchors. Every link
 // must be included in this held source census; an external alias is not exclusive.
 let mut groups:std::collections::BTreeMap<(u64,u64),Vec<(&File,&PhysicalStamp,&String)>>=Default::default();
 for(held,stamp,sha)in out.files.values(){groups.entry(identity(held)?).or_default().push((held,stamp,sha));}
 for group in groups.values(){
  let(first,stamp,sha)=group[0];let links=first.metadata()?.nlink();
  if links!=group.len()as u64{return Err(fail("Capture source hardlink group has unaccounted links"));}
  for(held,other,other_sha)in group{if physical_stamp(held)?!=**other||*other!=stamp||*other_sha!=sha||held.metadata()?.nlink()!=links{return Err(fail("Capture source hardlink group changed"));}}
 }
 sqlite_capture_ancestry(root)?;Ok(out)
}
impl SqliteCaptureInventory {
 fn fence(&self,root:&Root)->io::Result<()> {self.fence_source(root,Root::check)}
 fn fence_source(&self,root:&Root,check:fn(&Root)->io::Result<()>)->io::Result<()> {
  check(root)?;if physical_stamp(root.fd())?!=self.root_stamp{return Err(fail("Capture source root changed"));}
  let observed=sqlite_capture_inventory(root)?;
  if observed.directories.keys().ne(self.directories.keys())||observed.files.keys().ne(self.files.keys()){return Err(fail("Capture source inventory changed"));}
  for(p,(held,stamp))in &self.directories{if physical_stamp(held)?!=*stamp||observed.directories[p].1!=*stamp{return Err(fail("Capture source directory changed"));}}
  // Independent held-descriptor pass: no named-pass or previous-fence cache.
  let mut held_hashes=Default::default();
  for(p,(held,stamp,sha))in &self.files{if physical_stamp(held)?!=*stamp||observed.files[p].1!=*stamp||observed.files[p].2!=*sha||verification_pager_walk_hash(held,stamp,&mut held_hashes)?!=*sha{return Err(fail(&format!("Capture source original file changed: {p}; held={:?}; observed={:?}; expected={stamp:?}; hash_equal={}",physical_stamp(held)?,observed.files[p].1,observed.files[p].2==*sha)));}}
  check(root)
 }
}
fn sqlite_capture_workspace(parent:&Root,name:&str)->io::Result<Root> {
 let n=CString::new(name).map_err(|_|fail("Unsafe capture workspace"))?;
 if unsafe{libc::mkdirat(parent.fd().as_raw_fd(),n.as_ptr(),0o700)}!=0{return Err(io::Error::last_os_error());}
 let file=open_at(parent.fd(),name,libc::O_RDONLY|libc::O_DIRECTORY)?;
 let mut chain=Vec::new();for f in &parent.chain{chain.push(f.try_clone()?);}chain.push(file);parent.fd().sync_all()?;
 Ok(Root{absolute:parent.absolute.join(name),chain})
}
fn sqlite_capture_write(root:&Root,name:&str,bytes:&[u8])->io::Result<File> {
 use std::io::Write;root.check()?;let mut f=open_at(root.fd(),name,libc::O_RDWR|libc::O_CREAT|libc::O_EXCL)?;f.write_all(bytes)?;f.sync_all()?;root.fd().sync_all()?;Ok(f)
}
fn sqlite_capture_python()->io::Result<(PathBuf,File,PhysicalStamp,String)> {
 let path=std::fs::canonicalize(SQLITE_CAPTURE_PYTHON)?;let parent=Root::open(path.parent().ok_or_else(||fail("Invalid Python executable path"))?)?;
 let file=open_at(parent.fd(),path.file_name().and_then(|s|s.to_str()).ok_or_else(||fail("Invalid Python executable name"))?,libc::O_RDONLY|libc::O_NONBLOCK)?;let md=file.metadata()?;
 if !md.is_file()||md.uid()!=0||md.mode()&0o022!=0||md.mode()&0o111==0{return Err(fail("Capture Python executable is not trusted root-owned code"));}
 let stamp=physical_stamp(&file)?;let sha=hash_file(&file,md.len())?;if physical_stamp(&file)?!=stamp{return Err(fail("Python executable raced"));}Ok((path,file,stamp,sha))
}
/// No caller-selected helper path, executable or admission JSON is accepted.
pub(crate) async fn capture_sqlite_certified(source:&Path,target:&Path,version:&str,writers_stopped:bool)->anyhow::Result<std::sync::Arc<crate::storage::backup_inventory::ValidatedBundle>> {
 capture_sqlite_observed(source,target,version,writers_stopped,None,|_source,bundle|Box::pin(async move{Ok(bundle)})).await
}
/// Dedicated producer continuation retains the actual original stopped source,
/// pager transaction and native leases until verification publication returns.
pub(crate) async fn capture_sqlite_for_verification<T,F>(source:&Path,target:&Path,version:&str,writers_stopped:bool,admin:&crate::auth::middleware::AdminUser,observe:F)->anyhow::Result<T>
where F:for<'a> FnOnce(StoppedVerificationSource<'a>,std::sync::Arc<crate::storage::backup_inventory::ValidatedBundle>)->futures::future::LocalBoxFuture<'a,anyhow::Result<T>> {
 capture_sqlite_observed(source,target,version,writers_stopped,Some(admin),observe).await
}
// Local physical-preservation producer only. No custom observer/current-human
// or verification callback is exposed by this overload. Its historical source
// proof stays separate from the existing stopped-source authority type.
pub(crate) async fn capture_sqlite_historical_preservation(source:&Path,target:&Path,version:&str,writers_stopped:bool)->anyhow::Result<std::sync::Arc<crate::storage::backup_inventory::ValidatedBundle>>{
 capture_sqlite_mode(source,target,version,writers_stopped,None,true,|_source,bundle|Box::pin(async move{Ok(bundle)})).await
}
async fn capture_sqlite_observed<T,F>(source:&Path,target:&Path,version:&str,writers_stopped:bool,admin:Option<&crate::auth::middleware::AdminUser>,observe:F)->anyhow::Result<T>
where F:for<'a> FnOnce(StoppedVerificationSource<'a>,std::sync::Arc<crate::storage::backup_inventory::ValidatedBundle>)->futures::future::LocalBoxFuture<'a,anyhow::Result<T>> {
 capture_sqlite_mode(source,target,version,writers_stopped,admin,false,observe).await
}
async fn capture_sqlite_mode<T,F>(source:&Path,target:&Path,version:&str,writers_stopped:bool,admin:Option<&crate::auth::middleware::AdminUser>,historical_mode:bool,observe:F)->anyhow::Result<T>
where F:for<'a> FnOnce(StoppedVerificationSource<'a>,std::sync::Arc<crate::storage::backup_inventory::ValidatedBundle>)->futures::future::LocalBoxFuture<'a,anyhow::Result<T>> {
 use anyhow::{ensure,Context};use sqlx::Connection;use std::process::Stdio;
 ensure!(writers_stopped,"Certified capture requires every application/background/sync writer stopped");
 let source=Root::open(source)?;source.check()?;
 let absolute=if target.is_absolute(){target.to_owned()}else{std::env::current_dir()?.join(target)};
 ensure!(!absolute.starts_with(&source.absolute)&&!source.absolute.starts_with(&absolute),"Capture destination overlaps source");
 let parent=Root::open(absolute.parent().context("Capture destination parent absent")?)?;
 for fd in &parent.chain{ensure!(identity(fd)?!=identity(source.fd())?,"Capture parent aliases source");}
 let name=absolute.file_name().and_then(|s|s.to_str()).context("Invalid capture destination")?.to_owned();ensure!(!exists(parent.fd(),&name)?,"Capture destination already exists");
 // Establish complete original custody BEFORE any source SQL/native baseline.
 let original=SqliteOriginalSourceCustody::hold(&source)?;
 #[cfg(test)] sqlite_capture_test_checkpoint("source-sealed",&source.absolute,&parent.absolute,None);
 let database=open_at(source.fd(),"starlane-notes.db",libc::O_RDONLY)?;let original_database=physical_stamp(&database)?;
 let sidecar=|name:&str|->io::Result<Option<File>>{if exists(source.fd(),name)?{Ok(Some(open_at(source.fd(),name,libc::O_RDONLY|libc::O_NONBLOCK)?))}else{Ok(None)}};
 let io_owner=original.clone();
 let reader=crate::storage::restoration_sqlite_source_vfs::OriginalSqliteSourceVfs::new(
  source.fd().try_clone()?,source.absolute.clone(),database.try_clone()?,
  sidecar("starlane-notes.db-wal")?,sidecar("starlane-notes.db-shm")?,sidecar("starlane-notes.db-journal")?,
  std::sync::Arc::new(move||io_owner.io_fence()))?;
 sqlx::any::install_default_drivers();
 let pool=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&reader.url()?).await?;let mut conn=pool.acquire().await?;
 sqlx::query("PRAGMA busy_timeout=0").execute(&mut *conn).await?;
 let opened:String=sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name='main'").fetch_one(&mut *conn).await?;
 ensure!(Path::new(&opened)==source.absolute.join("starlane-notes.db"),"SQLite pager opened a different original source database path");
 let journal:String=sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&mut *conn).await?;
 ensure!(["delete","truncate","persist","wal"].contains(&journal.as_str()),"Unsupported durable source journal mode");
 // Genuine Unix read locks plus independent OFD shared locks remain live. A
 // foreign writable open permanently consumes custody, even if no SQL commits.
 let mut tx=conn.begin_with("BEGIN").await?;
 original.fence(&source)?;reader.fence()?;
 if let Some(admin)=admin {verification_source_human(&mut tx,admin).await?;}
 let rows=prior_capture_rows(&mut tx).await?;
 let namespace=if exists(source.fd(),".attachment-upload-admission")?{Some(source.directory(".attachment-upload-admission")?)}else{None};
 let actual=match &namespace{Some(fd)=>prior_capsule_names(fd)?,None=>Default::default()};ensure!(actual==rows.iter().map(|r|r.operation_id.clone()).collect(),"Full capsule inventory differs from complete SQL");
 let historical=if historical_mode{Some(HistoricalCompositeSource::hold_in(&source.absolute,&mut tx).await?)}else{None};
 let composite=if historical.is_none()&&has_composite_capture_source(&source.absolute,&mut tx).await?{Some(crate::storage::attachment_upload::admission::committed_erasure::accounting::composite::CompositeOriginalSource::hold_in(&source.absolute,&mut tx).await?)}else{None};
 let mixed_completed=if historical.is_none()&&composite.is_none()&&has_mixed_completed_capture_source(&source.absolute,&mut tx).await?{Some(MixedCompletedCapture::hold(&source.absolute,&mut tx).await?)}else{None};
 let prior_completed=if historical.is_none()&&composite.is_none()&&mixed_completed.is_none()&&has_finalized_completed_capture_source(&source.absolute)?{Some(FinalizedCompletedCapture::hold(&source.absolute,&mut tx).await?)}else{None};
 let prior=if historical.is_none()&&has_finalized_admission_capture_source(&source.absolute)?{Some(PriorRestoredCaptureLeases::hold(&source.absolute,&mut tx,&rows).await?)}else{None};
 let preservation=if historical.is_none()&&prior.is_none()&&has_finalized_admission_preservation_source(&source.absolute)?{Some(FinalizedAdmissionPreservationLeases::hold(&source.absolute,&mut tx,&rows).await?)}else{None};
 let native=if historical.is_none()&&prior.is_none()&&preservation.is_none()&&prior_completed.is_none()&&mixed_completed.is_none()&&composite.is_none(){Some(admission::capture::NativeCaptureLeases::hold(&source.absolute,&mut tx,&rows).await?)}else{None};
 let completion_table:i64=sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='attachment_erasure_dispositions'").fetch_one(&mut *tx).await?;
 let completed_count:i64=if completion_table==1{sqlx::query_scalar("SELECT count(*) FROM attachment_erasure_dispositions").fetch_one(&mut *tx).await?}else{0};
 let completed=if historical.is_none()&&completed_count>0&&prior_completed.is_none()&&mixed_completed.is_none()&&composite.is_none(){Some(native.as_ref().context("Completed absence requires original native source proof; prior completion transport unsupported")?.capture_completed(&source.absolute,&mut tx).await?)}else{None};
 let historical_evidence=match &historical{Some(h)=>Some(h.capture_evidence_in(&mut *tx).await?),None=>None};
 let composite_evidence=match &composite{Some(c)=>Some(c.capture_evidence_in(&mut *tx).await?),None=>None};
 let mixed_completed_evidence=match &mixed_completed{Some(c)=>Some(c.evidence()?),None=>None};
 let prior_completed_evidence=match &prior_completed{Some(c)=>Some(c.evidence()?),None=>None};
 let completed_evidence=match &completed{Some(c)=>Some(c.envelope_in(&mut *tx).await?),None=>None};
 let certificate=match &native{Some(n)=>Some(match &completed{Some(c)=>n.classify_with_completed(&mut tx,c).await?,None=>n.classify_supported(&mut tx).await?}),None=>None};
 let native_evidence=match &certificate{Some(c)=>{let e=c.capture_envelope()?;if e.scopes.is_empty(){None}else{Some(e)}},None=>None};
 let prior_evidence=match &prior{Some(p)=>{let e=p.lineage_evidence()?;if e.scopes.is_empty(){None}else{Some(e)}},None=>None};
 let inventory=&original.inventory;
 let workspace_name=format!(".starlane-sqlite-capture-{}.pending",uuid::Uuid::new_v4());let workspace=sqlite_capture_workspace(&parent,&workspace_name)?;let workspace_id=identity(workspace.fd())?;
 let mut helper=sqlite_capture_write(&workspace,"helper.py",SQLITE_CAPTURE_HELPER)?;let helper_stamp=physical_stamp(&helper)?;let helper_sha=digest(SQLITE_CAPTURE_HELPER);
 let stdout=sqlite_capture_write(&workspace,"helper-stdout.log",b"")?;let stderr=sqlite_capture_write(&workspace,"helper-stderr.log",b"")?;
 let (python_path,python,python_stamp,python_sha)=sqlite_capture_python()?;
 let intermediate=workspace.absolute.join("bundle");
 let code="import sys; g={'__name__':'starlane_capture_helper'}; exec(compile(sys.stdin.read(),'compiled-in-starlane-helper','exec'),g); g['_create_with_admission_leases'](g['Path'](sys.argv[1]),g['Path'](sys.argv[2]),sys.argv[3],True)";
 {use std::io::Seek;helper.seek(std::io::SeekFrom::Start(0))?;}
 let mut command=tokio::process::Command::new(&python_path);
 command.args(["-I","-c",code]).arg(&source.absolute).arg(&intermediate).arg(version).stdin(Stdio::from(helper.try_clone()?)).stdout(Stdio::from(stdout)).stderr(Stdio::from(stderr));
 let mut child=SqliteCaptureHelper::spawn(&mut command,original.clone())?;
 #[cfg(test)] sqlite_capture_test_checkpoint("helper-spawned",&source.absolute,&workspace.absolute,child.id());
 let status=child.wait().await?;
 // All failure paths retain the native-created private workspace and every partial helper file.
 workspace.check()?;parent.check()?;ensure!(identity(workspace.fd())?==workspace_id&&identity(&parent.directory(&workspace_name)?)?==workspace_id,"Private helper intermediate was substituted");
 ensure!(physical_stamp(&helper)?==helper_stamp&&physical_stamp(&workspace.leaf("helper.py")?)?==helper_stamp&&hash_file(&helper,helper_stamp.length)?==helper_sha,"Compiled helper source changed");
 ensure!(physical_stamp(&python)?==python_stamp&&physical_stamp(&File::open(&python_path)?)?==python_stamp&&hash_file(&python,python_stamp.length)?==python_sha,"Trusted Python executable changed");
 ensure!(status.success(),"Trusted helper failed; private evidence retained");
 original.fence(&source)?;
 let first=crate::storage::backup_inventory::verify(&intermediate).await?;ensure!(first.is_qualified_capacity_capture(),"Private certified producer currently requires qualified capacity51");
 let mut snapshot=first.db.acquire().await?;ensure!(prior_capture_rows(&mut snapshot).await?==rows,"Helper snapshot changed exact reservations");drop(snapshot);
 first.fence()?;let mut manifest:serde_json::Value=serde_json::from_slice(&first.raw_manifest)?;let first_manifest=workspace.directory("bundle")?;let mf=open_at(&first_manifest,"manifest.json",libc::O_RDWR)?;let mf_id=identity(&mf)?;drop(first);
 if let Some(e)=&native_evidence{manifest.as_object_mut().context("Manifest must be object")?.insert("admission_owner_capture".into(),serde_json::to_value(e)?);}
 if let Some(e)=&prior_evidence{ensure!(native_evidence.is_none(),"Prior/native evidence conflict");manifest.as_object_mut().context("Manifest must be object")?.insert("prior_admission_owner_capture".into(),serde_json::to_value(e)?);}
 if let Some(e)=&completed_evidence{manifest.as_object_mut().context("Manifest must be object")?.insert("completed_absence_capture".into(),serde_json::to_value(e)?);}
 if let Some(e)=&mixed_completed_evidence{ensure!(completed_evidence.is_none()&&prior_completed_evidence.is_none()&&native_evidence.is_none()&&prior_evidence.is_none(),"Mixed completed capture evidence conflict");manifest.as_object_mut().context("Manifest must be object")?.insert("mixed_completed_absence_capture".into(),serde_json::to_value(e)?);}
 if let Some(e)=&prior_completed_evidence{ensure!(completed_evidence.is_none()&&native_evidence.is_none()&&prior_evidence.is_none(),"Prior completed capture evidence conflict");manifest.as_object_mut().context("Manifest must be object")?.insert("prior_completed_absence_capture".into(),serde_json::to_value(e)?);}
 if let Some(e)=&composite_evidence{ensure!(native_evidence.is_none()&&prior_evidence.is_none()&&completed_evidence.is_none()&&prior_completed_evidence.is_none()&&mixed_completed_evidence.is_none(),"Composite source selection conflicts with legacy evidence");manifest.as_object_mut().context("Composite manifest not object")?.insert("composite_admission_capture".into(),serde_json::to_value(e)?);}
 if let Some(e)=&historical_evidence{manifest.as_object_mut().context("Historical manifest not object")?.insert("historical_composite_preservation_capture".into(),serde_json::to_value(e)?);}
 let raw=serde_json::to_vec(&manifest)?;let mut mf=mf;{use std::io::{Write,Seek};mf.set_len(0)?;mf.seek(std::io::SeekFrom::Start(0))?;mf.write_all(&raw)?;mf.sync_all()?;}first_manifest.sync_all()?;ensure!(identity(&open_at(&first_manifest,"manifest.json",libc::O_RDONLY)?)?==mf_id,"Private manifest replaced");
 let validated=std::sync::Arc::new(crate::storage::backup_inventory::verify(&intermediate).await?);
 let historical_snapshot=if historical_evidence.is_some(){Some(ClassifiedHistoricalBundle::from_bundle(validated.clone()).await?)}else{None};
 let composite_snapshot=if composite_evidence.is_some(){Some(ClassifiedCompositeBundle::from_bundle(validated.clone()).await?)}else{None};
 if completed_evidence.is_some()||prior_completed_evidence.is_some()||mixed_completed_evidence.is_some(){let mut copy=validated.db.acquire().await?;crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::ClassifiedCompletedBundle::from_bundle(validated.clone(),&mut copy).await?.fence_in(&mut copy).await?;}
 if native_evidence.is_some(){admission::ClassifiedAdmissionSource::cold_bundle(validated.clone()).await?.fence()?;}
 if let Some(e)=&prior_evidence{admission::ClassifiedAdmissionSource::cold_prior_bundle(validated.clone(),e).await?.fence()?;}
 if let Some(p)=&prior{p.recheck_in(&mut tx).await?;}if let Some(p)=&preservation{p.recheck_in(&mut tx).await?;}if let Some(n)=&native{n.recheck(&mut tx).await?;}if let Some(c)=&certificate{c.fence()?;}
 if let Some(c)=&completed{c.fence_in(&mut *tx).await?;}if let Some(c)=&prior_completed{c.recheck_in(&mut tx).await?;}if let Some(c)=&mixed_completed{c.recheck_in(&mut tx).await?;}if let Some(c)=&composite{c.recheck_in(&mut *tx).await?;}if let Some(h)=&historical{h.recheck_in(&mut *tx).await?;}
 ensure!(prior_capture_rows(&mut tx).await?==rows,"Final source reservation SQL changed");original.fence(&source)?;validated.fence()?;workspace.check()?;parent.check()?;
 if let Some(c)=&composite_snapshot{let mut copy=c.bundle.db.acquire().await?;c.fence_in(&mut copy).await?;}
 if let Some(h)=&historical_snapshot{let mut copy=h.bundle.db.acquire().await?;h.fence_in(&mut copy).await?;}
 drop(historical_snapshot);
 drop(composite_snapshot); // Path-bound cold facts end before the existing descriptor-certified rename.
 let intermediate_root=Root::open(&intermediate)?;
 let publication_marker=sqlite_capture_write(&intermediate_root,".starlane-backup-incomplete",b"Certified source final publication pending; preserve evidence\n")?;
 let marker_stamp=physical_stamp(&publication_marker)?;
 let sealed=sqlite_capture_inventory(&intermediate_root)?;
 let old=CString::new("bundle")?;let new=CString::new(name)?;
 ensure!(unsafe{libc::renameat2(workspace.fd().as_raw_fd(),old.as_ptr(),parent.fd().as_raw_fd(),new.as_ptr(),libc::RENAME_NOREPLACE)}==0,"Exclusive final publication refused; private evidence retained");
 parent.fd().sync_all()?;workspace.fd().sync_all()?;
 let final_root=Root::open(&absolute)?;ensure!(identity(final_root.fd())?==identity(intermediate_root.fd())?,"Final published root differs");
 let observed=sqlite_capture_inventory(&final_root)?;
 ensure!(observed.files.keys().eq(sealed.files.keys())&&observed.directories.keys().eq(sealed.directories.keys()),"Final private bundle inventory differs");
 for(p,(held,stamp,sha))in &sealed.files{ensure!(physical_stamp(held)?==*stamp&&observed.files[p].1==*stamp&&observed.files[p].2==*sha,"Final private bundle file differs");}
 for(p,(held,stamp))in &sealed.directories{ensure!(physical_stamp(held)?==*stamp&&observed.directories[p].1==*stamp,"Final private bundle directory differs");}
 original.fence(&source)?;
 if let Some(p)=&prior{p.recheck_in(&mut tx).await?;}if let Some(p)=&preservation{p.recheck_in(&mut tx).await?;}if let Some(n)=&native{n.recheck(&mut tx).await?;}
 if let Some(c)=&completed{c.fence_in(&mut *tx).await?;}if let Some(c)=&prior_completed{c.recheck_in(&mut tx).await?;}if let Some(c)=&mixed_completed{c.recheck_in(&mut tx).await?;}if let Some(c)=&composite{c.recheck_in(&mut *tx).await?;}if let Some(h)=&historical{h.recheck_in(&mut *tx).await?;}
 ensure!(prior_capture_rows(&mut tx).await?==rows,"Final publication source SQL differs");
 ensure!(physical_stamp(&publication_marker)?==marker_stamp&&physical_stamp(&final_root.leaf(".starlane-backup-incomplete")?)?==marker_stamp,"Final publication marker differs");
 let marker_name=CString::new(".starlane-backup-incomplete")?;ensure!(unsafe{libc::unlinkat(final_root.fd().as_raw_fd(),marker_name.as_ptr(),0)}==0,"Final marker retirement failed");final_root.fd().sync_all()?;parent.fd().sync_all()?;
 let result=std::sync::Arc::new(crate::storage::backup_inventory::verify(&absolute).await?);result.fence()?;
 let published_historical=if historical_evidence.is_some(){Some(ClassifiedHistoricalBundle::from_bundle(result.clone()).await?)}else{None};
 let published_composite=if composite_evidence.is_some(){Some(ClassifiedCompositeBundle::from_bundle(result.clone()).await?)}else{None};
 let mut held=StoppedVerificationSource{root:&source,database:&database,database_stamp:original_database,inventory,custody:&original,rows:&rows,
    prior:prior.as_ref(),preservation:preservation.as_ref(),native:native.as_ref(),certificate:certificate.as_ref(),completed:completed.as_ref(),prior_completed:prior_completed.as_ref(),mixed_completed:mixed_completed.as_ref(),composite:composite.as_ref(),historical:historical.as_ref(),conn:&mut tx,admin};
 held.fence().await?;
 let output=observe(held,result.clone()).await?;
 if let Some(h)=&published_historical{let mut copy=h.bundle.db.acquire().await?;h.fence_in(&mut copy).await?;}
 if let Some(h)=&historical{h.recheck_in(&mut *tx).await?;}
 if let Some(c)=&published_composite{let mut copy=c.bundle.db.acquire().await?;c.fence_in(&mut copy).await?;}
 // Keep the existing capture source closure intact after the continuation.
 original.fence(&source)?;result.fence()?;
 if let Some(p)=&prior{p.recheck_in(&mut tx).await?;}if let Some(p)=&preservation{p.recheck_in(&mut tx).await?;}if let Some(n)=&native{n.recheck(&mut tx).await?;}if let Some(c)=&certificate{c.fence()?;}
 if let Some(c)=&completed{c.fence_in(&mut *tx).await?;}if let Some(c)=&prior_completed{c.recheck_in(&mut tx).await?;}if let Some(c)=&mixed_completed{c.recheck_in(&mut tx).await?;}if let Some(c)=&composite{c.recheck_in(&mut *tx).await?;}if let Some(h)=&historical{h.recheck_in(&mut *tx).await?;}
 ensure!(prior_capture_rows(&mut tx).await?==rows,"Source SQL changed after verification continuation");
 if let Some(admin)=admin {verification_source_human(&mut tx,admin).await?;}
 original.fence(&source)?;tx.rollback().await?;original.fence(&source)?;
 reader.stop_admissions()?;drop(conn);pool.close().await;reader.native_close_ack().await?;
 #[cfg(test)] sqlite_capture_test_checkpoint("pager-closed",&source.absolute,&workspace.absolute,None);
 original.fence(&source)?;reader.fence()?;result.fence()?;reader.retire()?;original.fence(&source)?;Ok(output)
}

#[cfg(test)]
type SqliteCaptureTestHook=Box<dyn Fn(&str,&Path,&Path,Option<u32>)+Send>;
#[cfg(test)]
fn sqlite_capture_test_hook()->&'static Mutex<Option<SqliteCaptureTestHook>> {static HOOK:OnceLock<Mutex<Option<SqliteCaptureTestHook>>>=OnceLock::new();HOOK.get_or_init(||Mutex::new(None))}
#[cfg(test)]
fn sqlite_capture_test_checkpoint(phase:&str,source:&Path,workspace:&Path,pid:Option<u32>){if let Some(h)=sqlite_capture_test_hook().lock().unwrap().as_ref(){h(phase,source,workspace,pid);}}

#[cfg(test)]
mod sqlite_certified_capture_tests {
 use super::*;use crate::storage::{attachment_upload as upload,attachment_capacity as capacity};
 fn private_dir(p:&Path){use std::os::unix::fs::{DirBuilderExt,PermissionsExt};std::fs::DirBuilder::new().recursive(true).mode(0o700).create(p).unwrap();std::fs::set_permissions(p,std::fs::Permissions::from_mode(0o700)).unwrap();}
 async fn fixture(length:usize)->(PathBuf,AnyPool,String,Vec<u8>){
  sqlx::any::install_default_drivers();let base=std::env::temp_dir().join(format!("certified-sqlite-producer-{}",uuid::Uuid::new_v4()));let source=base.join("source");private_dir(&source);
  let url=format!("sqlite://{}?mode=rwc",source.join("starlane-notes.db").display());let pool=sqlx::any::AnyPoolOptions::new().max_connections(2).connect(&url).await.unwrap();crate::db::selected_migrator(crate::db::DatabaseEngine::Sqlite).run(&pool).await.unwrap();
  let attempt=uuid::Uuid::new_v4().to_string();let req=upload::UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"browser".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),original_name:"partial.bin".into(),mime_type:"application/octet-stream".into()};
  let writer=upload::admission::WriterLease::begin(&source,&attempt,upload::admission::Producer::Direct{request:req.clone()},(length+1024)as u64).unwrap();capacity::reserve(&pool,&writer,None).await.unwrap();let mut ingest=upload::Ingest::reserve_with_lease(&source,req,(length+1024)as u64,writer.clone()).unwrap();let chunk=[0x6au8;64*1024];for _ in 0..length/chunk.len(){ingest.append(&chunk).unwrap();}let claim=writer.original_claim().to_vec();drop(ingest);drop(writer);
  use std::os::unix::fs::PermissionsExt;for item in std::fs::read_dir(&source).unwrap(){let p=item.unwrap().path();if p.is_file(){std::fs::set_permissions(p,std::fs::Permissions::from_mode(0o600)).unwrap();}}
  (base,pool,attempt,claim)
 }
 #[test]
 fn source_inventory_accounts_for_complete_owned_hardlinks_and_refuses_external_alias(){
  let base=std::env::temp_dir().join(format!("sqlite-capture-links-{}",uuid::Uuid::new_v4()));private_dir(&base);
  let source=base.join("source");private_dir(&source);let root=Root::open(&source).unwrap();
  let original=sqlite_capture_write(&root,"canonical",b"original canonical bytes").unwrap();
  std::fs::hard_link(source.join("canonical"),source.join("install-anchor")).unwrap();
  drop(original);
  let held=SqliteOriginalSourceCustody::hold(&root).unwrap();held.fence(&root).unwrap();
  let alias=base.join("external-alias");std::fs::hard_link(source.join("canonical"),&alias).unwrap();
  assert!(sqlite_capture_inventory(&root).is_err());assert!(held.fence(&root).is_err());
  std::fs::remove_file(alias).unwrap();assert!(held.fence(&root).is_err(),"Original history remains invalid after link return, even if ctime collides");
  drop(held);
  let current=SqliteOriginalSourceCustody::hold(&root).unwrap();current.fence(&root).unwrap();
  std::fs::rename(source.join("install-anchor"),base.join("unaccounted-anchor")).unwrap();
  assert!(sqlite_capture_inventory(&root).is_err());assert!(current.fence(&root).is_err());
 }
 #[test]
 fn original_source_custody_refuses_retained_writer_and_cancelled_write_open(){
  use std::os::unix::fs::OpenOptionsExt;
  let base=std::env::temp_dir().join(format!("sqlite-capture-original-custody-{}",uuid::Uuid::new_v4()));private_dir(&base);
  let source=base.join("source");private_dir(&source);let root=Root::open(&source).unwrap();
  let writer=sqlite_capture_write(&root,"starlane-notes.db",b"original database specimen").unwrap();
  assert!(SqliteOriginalSourceCustody::hold(&root).is_err(),"A retained writable descriptor prevents the original certified baseline");
  drop(writer);
  let held=SqliteOriginalSourceCustody::hold(&root).unwrap();held.fence(&root).unwrap();
  let attempted=std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(source.join("starlane-notes.db"));
  // The worker may release its lease before open returns; either outcome must
  // permanently consume this original operation's authority, never its bytes.
  drop(attempted);
  assert!(held.fence(&root).is_err(),"Write-open invalidates the held original even without a write");
  assert!(held.fence(&root).is_err(),"Restoring the final state cannot reacquire original custody");
  assert_eq!(std::fs::read(source.join("starlane-notes.db")).unwrap(),b"original database specimen");
 }
 #[tokio::test]
 #[ignore="actual trusted Python; original read-only DELETE/TRUNCATE/PERSIST/populated WAL custody"]
 async fn original_readonly_capture_preserves_all_journal_modes_and_nonempty_wal(){
  for mode in ["delete","truncate","persist","wal-empty","wal-shm","wal-no-shm"] {
   let(base,pool,_,_)=fixture(64*1024).await;let source=base.join("source");
   let journal=if mode.starts_with("wal"){"wal"}else{mode};
   let mut setup=pool.acquire().await.unwrap();
   let actual:String=sqlx::query_scalar(&format!("PRAGMA journal_mode={journal}")).fetch_one(&mut *setup).await.unwrap();assert_eq!(actual,journal);
   sqlx::query("UPDATE _sqlx_migrations SET execution_time=execution_time+1 WHERE version=1").execute(&mut *setup).await.unwrap();
   let mut expected:i64=sqlx::query_scalar("SELECT execution_time FROM _sqlx_migrations WHERE version=1").fetch_one(&mut *setup).await.unwrap();drop(setup);pool.close().await;
   if mode.starts_with("wal")&&mode!="wal-empty" {
    let code="import sqlite3,os,sys; c=sqlite3.connect(sys.argv[1]); c.execute('PRAGMA wal_autocheckpoint=0'); c.execute('UPDATE _sqlx_migrations SET execution_time=execution_time+1 WHERE version=1'); c.commit(); os._exit(0)";
    let child=std::process::Command::new(SQLITE_CAPTURE_PYTHON).args(["-I","-c",code]).arg(source.join("starlane-notes.db")).status().unwrap();assert!(child.success());expected+=1;
    assert!(std::fs::metadata(source.join("starlane-notes.db-wal")).unwrap().len()>32,"Committed data must remain in original WAL");
    assert!(source.join("starlane-notes.db-shm").exists());
    if mode=="wal-no-shm"{std::fs::remove_file(source.join("starlane-notes.db-shm")).unwrap();}
   }
   use std::os::unix::fs::PermissionsExt;
   for e in std::fs::read_dir(&source).unwrap(){let path=e.unwrap().path();if path.is_file(){std::fs::set_permissions(path,std::fs::Permissions::from_mode(0o600)).unwrap();}}
   if mode=="wal-empty"{assert!(!source.join("starlane-notes.db-wal").exists()&&!source.join("starlane-notes.db-shm").exists(),"Checkpointed WAL source must stay free of sidecars");}
   if mode=="truncate" {assert_eq!(std::fs::metadata(source.join("starlane-notes.db-journal")).unwrap().len(),0);}
   if mode=="persist" {let raw=std::fs::read(source.join("starlane-notes.db-journal")).unwrap();assert!(raw.len()>28);assert_eq!(&raw[..28],&[0u8;28]);}
   let root=Root::open(&source).unwrap();let before=sqlite_capture_inventory(&root).unwrap();
   let captured=capture_sqlite_certified(&source,&base.join("bundle"),"v1.5.4",true).await.unwrap_or_else(|e|panic!("Original mode {mode} failed: {e:#}"));
   let observed:i64=sqlx::query_scalar("SELECT execution_time FROM _sqlx_migrations WHERE version=1").fetch_one(&captured.db).await.unwrap();assert_eq!(observed,expected,"Backup includes exact committed WAL tail");
   before.fence(&root).unwrap();captured.fence().unwrap();captured.db.close().await;
   println!("ORIGINAL_READONLY_CAPTURE_MODE_{mode}_EXACT_FILES_AND_ROWS_PASS");
  }
 }
 #[tokio::test]
 #[ignore="actual trusted Python; whole original history across source/helper/observer/native-close boundaries"]
 async fn original_capture_history_refuses_at_all_continuation_boundaries(){
  fn mutate(source:&Path,base:&Path,kind:&str){
   if kind=="directory" {
    std::fs::rename(source.join("spaces/custody-specimen"),base.join("held-directory")).unwrap();
    std::fs::rename(base.join("held-directory"),source.join("spaces/custody-specimen")).unwrap();
   }else{
    let selected=match kind {"main"=>source.join("starlane-notes.db"),"wal"=>source.join("starlane-notes.db-wal"),"shm"=>source.join("starlane-notes.db-shm"),_=>source.join("spaces/custody-specimen/nested/body")};
    std::fs::hard_link(&selected,base.join("external-alias")).unwrap();std::fs::remove_file(base.join("external-alias")).unwrap();
   }
  }
  for phase in ["source-sealed","helper-spawned","observer","pager-closed"] {
   for kind in ["main","payload","directory","wal","shm"] {
    let(base,pool,_,_)=fixture(64*1024).await;let source=base.join("source");pool.close().await;
    if matches!(kind,"wal"|"shm") {
     let code="import sqlite3,os,sys; c=sqlite3.connect(sys.argv[1]); c.execute('PRAGMA journal_mode=WAL'); c.execute('PRAGMA wal_autocheckpoint=0'); c.execute('UPDATE _sqlx_migrations SET execution_time=execution_time+1 WHERE version=1'); c.commit(); os._exit(0)";
     assert!(std::process::Command::new(SQLITE_CAPTURE_PYTHON).args(["-I","-c",code]).arg(source.join("starlane-notes.db")).status().unwrap().success());
     assert!(std::fs::metadata(source.join("starlane-notes.db-wal")).unwrap().len()>32);
     assert!(source.join("starlane-notes.db-shm").exists());
    }
    let nested=source.join("spaces/custody-specimen/nested");private_dir(&nested);let root=Root::open(&nested).unwrap();drop(sqlite_capture_write(&root,"body",b"exact original specimen").unwrap());
    let before=std::fs::read(source.join("starlane-notes.db")).unwrap();
    let observed=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let seen=observed.clone();
    let selected=source.clone();let selected_base=base.clone();
    *sqlite_capture_test_hook().lock().unwrap()=Some(Box::new(move|at,path,_,_|{
     if at==phase&&path==selected {mutate(path,&selected_base,kind);seen.store(true,std::sync::atomic::Ordering::SeqCst);}
    }));
    let observer_source=source.clone();let observer_base=base.clone();let observer_seen=observed.clone();
    let result=capture_sqlite_observed(&source,&base.join("bundle"),"v1.5.4",true,None,move|mut held,bundle|Box::pin(async move{
     if phase=="observer" {mutate(&observer_source,&observer_base,kind);observer_seen.store(true,std::sync::atomic::Ordering::SeqCst);held.fence().await?;}
     Ok(bundle)
    })).await;
    *sqlite_capture_test_hook().lock().unwrap()=None;
    assert!(observed.load(std::sync::atomic::Ordering::SeqCst),"Required mutation barrier not reached: {phase}/{kind}; result={:?}",result.as_ref().err());
    assert!(result.is_err(),"Alias/directory restoration cannot resurrect original capture authority: {phase}/{kind}");
    assert_eq!(std::fs::read(source.join("starlane-notes.db")).unwrap(),before);
    assert_eq!(std::fs::read(nested.join("body")).unwrap(),b"exact original specimen");
    println!("ORIGINAL_CAPTURE_STICKY_HISTORY_{phase}_{kind}_PERMANENT_REFUSAL_PASS");
   }
  }
 }
 #[tokio::test]
 #[ignore="requires host trusted Python and uniquely owned real SQLite source"]
 async fn private_original_capture_and_cold_consumer(){
  let(base,pool,attempt,claim)=fixture(2*1024*1024).await;let source=base.join("source");let target=base.join("backup");
  assert!(capture_sqlite_certified(&source,&target,"v1.5.1",false).await.is_err());
  let live=upload::admission::WriterLease::open_idle(&source,&attempt).unwrap();assert!(capture_sqlite_certified(&source,&target,"v1.5.1",true).await.is_err(),"Active original capsule excludes capture");drop(live);
  let mut db_writer=pool.acquire().await.unwrap();use sqlx::Connection;let lock=db_writer.begin_with("BEGIN IMMEDIATE").await.unwrap();assert!(capture_sqlite_certified(&source,&target,"v1.5.1",true).await.is_err(),"Actual concurrent SQLite writer exclusion refuses");lock.rollback().await.unwrap();drop(db_writer);pool.close().await;
  let backup=capture_sqlite_certified(&source,&target,"v1.5.1",true).await.unwrap();backup.fence().unwrap();assert_eq!(backup.manifest.admission_owner_capture.as_ref().unwrap().scopes[0].original_claim_json.as_bytes(),claim);let cold=admission::ClassifiedAdmissionSource::cold_bundle(backup.clone()).await.unwrap();cold.fence().unwrap();
  let mut c=backup.db.acquire().await.unwrap();let rows=prior_capture_rows(&mut c).await.unwrap();assert_eq!(rows[0].maximum_bytes,2*1024*1024+1024);drop(c);assert!(upload::admission::WriterLease::open_idle(&backup.data_path(),&attempt).is_err(),"Copied bundle cannot become original writer");assert!(capture_sqlite_certified(&source,&target,"v1.5.1",true).await.is_err(),"Existing final destination is never replaced");
  let restored=base.join("restored");crate::storage::backup_restore::run(&target,&restored,"v1.5.1").await.unwrap();
  let restored_pool=sqlx::any::AnyPoolOptions::new().max_connections(2).connect(&format!("sqlite://{}?mode=rw",restored.join("starlane-notes.db").display())).await.unwrap();activate_in(&restored,&restored_pool).await.unwrap();
  assert!(upload::admission::WriterLease::open_idle(&restored,&attempt).is_err());let owner=acquire_admission_authority(&restored,&restored_pool,&attempt,&claim).await.unwrap();owner.check().unwrap();drop(owner);restored_pool.close().await;
  let again=base.join("backup-again");let prior=capture_sqlite_certified(&restored,&again,"v1.5.1",true).await.unwrap();assert!(prior.manifest.admission_owner_capture.is_none());assert!(prior.manifest.prior_admission_owner_capture.is_some());
  let rerestored=base.join("re-restored");crate::storage::backup_restore::run(&again,&rerestored,"v1.5.1").await.unwrap();let final_pool=sqlx::any::AnyPoolOptions::new().connect(&format!("sqlite://{}?mode=rw",rerestored.join("starlane-notes.db").display())).await.unwrap();activate_in(&rerestored,&final_pool).await.unwrap();let final_owner=acquire_admission_authority(&rerestored,&final_pool,&attempt,&claim).await.unwrap();final_owner.check().unwrap();assert!(upload::admission::WriterLease::open_idle(&rerestored,&attempt).is_err());drop(final_owner);
  let mut c=final_pool.acquire().await.unwrap();assert_eq!(prior_capture_rows(&mut c).await.unwrap()[0].maximum_bytes,2*1024*1024+1024);drop(c);final_pool.close().await;restored_pool.close().await;
  let original_prior=prior.raw_manifest.clone();prior.db.close().await;backup.db.close().await;
  for mode in ["null","both","legacy","ledger"] {
   let mut value:serde_json::Value=serde_json::from_slice(&original_prior).unwrap();match mode {
    "null"=>value["prior_admission_owner_capture"]=serde_json::Value::Null,
    "both"=>value["admission_owner_capture"]=serde_json::to_value(backup.manifest.admission_owner_capture.as_ref().unwrap()).unwrap(),
    "legacy"=>value["format_version"]=serde_json::json!(3),
    "ledger"=>value["prior_admission_owner_capture"]["ledger_sha256"]=serde_json::json!(digest(b"foreign ledger")),
    _=>unreachable!(),
   }
   std::fs::write(again.join("manifest.json"),serde_json::to_vec(&value).unwrap()).unwrap();let observed=crate::storage::backup_inventory::verify(&again).await;
   if mode=="ledger" {let altered=std::sync::Arc::new(observed.unwrap());assert!(admission::ClassifiedAdmissionSource::cold_prior_bundle(altered.clone(),altered.manifest.prior_admission_owner_capture.as_ref().unwrap()).await.is_err(),"Changed generation ledger cannot grant prior authority");altered.db.close().await;}else{assert!(observed.is_err(),"{mode} prior transport must refuse");}
  }
  std::fs::write(again.join("manifest.json"),original_prior).unwrap();
  println!("SQLITE_PRIVATE_STANDARD_PRODUCER_NATIVE_CAPTURE_RESTORE_PRIOR_REBACKUP_SECONDRESTORE_FULL_BYTES_CHARGE_NO_REBOUND_NULL_DUAL_LEGACY_FOREIGN_LEDGER_REFUSAL_PASS");
 }
 #[tokio::test]
 #[ignore="requires host trusted Python and uniquely owned real SQLite source"]
 async fn private_ledger1_preservation_backup_rerestore_without_roles(){
  let(base,pool,attempt,claim)=fixture(64*1024).await;pool.close().await;
  let source=base.join("source");let original=base.join("ordinary-no-role-backup");
  let captured=capture_sqlite_certified(&source,&original,"v1.5.1",true).await.unwrap();captured.db.close().await;drop(captured);
  // Explicit authority-omission fixture: the qualified bytes/census are unchanged.
  // This models ordinary legacy capture, never counterfeit native or prior transport.
  let manifest=original.join("manifest.json");let mut value:serde_json::Value=serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();value.as_object_mut().unwrap().remove("admission_owner_capture");std::fs::write(&manifest,serde_json::to_vec_pretty(&value).unwrap()).unwrap();
  let first=base.join("ledger1-restored");crate::storage::backup_restore::run(&original,&first,"v1.5.1").await.unwrap();
  let db=sqlx::any::AnyPoolOptions::new().connect(&format!("sqlite://{}?mode=rw",first.join("starlane-notes.db").display())).await.unwrap();activate_in(&first,&db).await.unwrap();
  assert!(has_finalized_admission_preservation_source(&first).unwrap());assert!(acquire_admission_authority(&first,&db,&attempt,&claim).await.is_err());assert!(upload::admission::WriterLease::open_idle(&first,&attempt).is_err());
  let opaque=first.join(format!(".attachment-upload-admission/{attempt}/future-evidence.dat"));sqlite_capture_write(&Root::open(opaque.parent().unwrap()).unwrap(),"future-evidence.dat",b"unknown retained evidence").unwrap();
  db.close().await;let again=base.join("preserved-backup");let backup=capture_sqlite_certified(&first,&again,"v1.5.1",true).await.unwrap();assert!(backup.manifest.admission_owner_capture.is_none()&&backup.manifest.prior_admission_owner_capture.is_none());
  let second=base.join("preserved-second-restore");crate::storage::backup_restore::run(&again,&second,"v1.5.1").await.unwrap();let second_db=sqlx::any::AnyPoolOptions::new().connect(&format!("sqlite://{}?mode=rw",second.join("starlane-notes.db").display())).await.unwrap();activate_in(&second,&second_db).await.unwrap();
  assert!(has_finalized_admission_preservation_source(&second).unwrap());assert!(acquire_admission_authority(&second,&second_db,&attempt,&claim).await.is_err());assert!(upload::admission::WriterLease::open_idle(&second,&attempt).is_err());assert_eq!(std::fs::read(second.join(format!(".attachment-upload-admission/{attempt}/future-evidence.dat"))).unwrap(),b"unknown retained evidence");assert_eq!(std::fs::read(second.join(format!(".attachment-upload-admission/{attempt}/claim.json"))).unwrap(),claim);
  let mut c=second_db.acquire().await.unwrap();let rows=prior_capture_rows(&mut c).await.unwrap();assert_eq!(rows[0].maximum_bytes,64*1024+1024);let held=FinalizedAdmissionPreservationLeases::hold(&second,&mut c,&rows).await.unwrap();held.fence().unwrap();drop(held);drop(c);second_db.close().await;db.close().await;backup.db.close().await;
  println!("SQLITE_LEDGER1_OMITTED_AUTHORITY_BACKUP_RESTORE_REBACKUP_SECONDRESTORE_UNKNOWN_BYTES_ORIGINAL_CLAIM_CHARGE_PRESERVED_NO_ROLE_OR_WRITER_GRANT_PASS");
 }
 #[tokio::test]
 #[ignore="requires host trusted Python and uniquely owned real SQLite source"]
 async fn private_helper_kill_and_source_intermediate_substitution_refuse(){
  use std::os::unix::fs::PermissionsExt;
  for mode in ["kill","database","helper","workspace"] {
   let(base,pool,attempt,claim)=fixture(64*1024).await;pool.close().await;let source=base.join("source");let target=base.join("backup");
   *sqlite_capture_test_hook().lock().unwrap()=Some(Box::new(move|phase,source,workspace,pid|{
    if phase!="helper-spawned"{return;}
    match mode {
     "kill"=>{assert_eq!(unsafe{libc::kill(pid.unwrap() as i32,libc::SIGKILL)},0);},
     "database"=>{let p=source.join("starlane-notes.db");let raw=std::fs::read(&p).unwrap();std::fs::rename(&p,source.parent().unwrap().join("original-db")).unwrap();std::fs::write(&p,raw).unwrap();std::fs::set_permissions(&p,std::fs::Permissions::from_mode(0o600)).unwrap();},
     "helper"=>{let p=workspace.join("helper.py");let raw=std::fs::read(&p).unwrap();std::fs::rename(&p,workspace.join("original-helper.py")).unwrap();std::fs::write(&p,raw).unwrap();std::fs::set_permissions(&p,std::fs::Permissions::from_mode(0o600)).unwrap();},
     "workspace"=>{std::fs::rename(workspace,workspace.with_extension("held-original")).unwrap();private_dir(workspace);},
     _=>unreachable!(),
    }
   }));
   let result=capture_sqlite_certified(&source,&target,"v1.5.1",true).await;*sqlite_capture_test_hook().lock().unwrap()=None;assert!(result.is_err(),"{mode} cannot grant publication");assert!(!target.exists(),"{mode} must not publish final destination");
   let original=if mode=="database"{base.join("original-db")}else{source.join("starlane-notes.db")};let db=sqlx::any::AnyPoolOptions::new().connect(&format!("sqlite://{}?mode=rw&immutable=1",original.display())).await.unwrap();let charged:i64=sqlx::query_scalar("SELECT maximum_bytes FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&attempt).fetch_one(&db).await.unwrap();assert_eq!(charged,64*1024+1024);db.close().await;
   assert_eq!(std::fs::read(source.join(format!(".attachment-upload-admission/{attempt}/claim.json"))).unwrap(),claim);assert!(std::fs::read_dir(&base).unwrap().any(|p|p.unwrap().file_name().to_string_lossy().starts_with(".starlane-sqlite-capture-")),"Failed private intermediate remains");
  }
  println!("SQLITE_PRIVATE_PRODUCER_REAL_HELPER_SIGKILL_DATABASE_FOREIGN_EQUAL_BYTES_HELPER_INODE_WORKSPACE_SUBSTITUTION_NO_SUCCESS_EVIDENCE_AND_CHARGE_RETAINED_PASS");
 }

}

#[cfg(test)]
mod sqlite_native_replay_producer_tests {
 use super::*;use anyhow::ensure;
 #[tokio::test]
 #[ignore="host trusted Python; exact native original Replay stopped source"]
 async fn required_standard_sqlite_original_replay_backup_cold_restore()->anyhow::Result<()> {
  use crate::storage::{attachment_upload::admission::{WriterLease,Producer},attachment_capacity,asset_blob};
  use std::os::unix::fs::{DirBuilderExt,PermissionsExt,OpenOptionsExt};use std::io::Write;
  sqlx::any::install_default_drivers();let base=std::env::var("STARLANE_SQLITE_REPLAY_SIGKILL_BASE").map(PathBuf::from).unwrap_or_else(|_|std::env::temp_dir().join(format!("sqlite-native-replay-producer-{}",uuid::Uuid::new_v4())));let source=base.join("source");std::fs::DirBuilder::new().recursive(true).mode(0o700).create(source.join("spaces/seed/docs"))?;
  let pool=sqlx::any::AnyPoolOptions::new().max_connections(2).connect(&format!("sqlite://{}?mode=rwc",source.join("starlane-notes.db").display())).await?;crate::db::selected_migrator(crate::db::DatabaseEngine::Sqlite).run(&pool).await?;
  let mut file=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(source.join("spaces/seed/docs/page.md"))?;file.write_all(b"Original published source\n")?;file.sync_all()?;drop(file);
  for q in ["INSERT INTO users(id,email,display_name) VALUES('actor','private-fixture@example.invalid','Fixture')","INSERT INTO spaces(id,name,slug,created_by) VALUES('space','Fixture','seed','actor')","INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at,published_at) VALUES('page','space','Fixture','spaces/seed/docs/page.md','actor','actor','original','published')"]{sqlx::query(q).execute(&pool).await?;}
  let attempt=uuid::Uuid::new_v4().to_string();let body=b"original native SQLite replay binary";let writer=WriterLease::begin(&source,&attempt,Producer::Replay{draft_scope:None,provider:"opaque-provider".into(),incoming_id:"opaque-incoming".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),asset_id:"opaque-asset".into(),payload_fingerprint:"a".repeat(64),binary_sha256:digest(body),byte_length:body.len()as u64},64)?;attachment_capacity::reserve(&pool,&writer,None).await?;let claim=writer.original_claim().to_vec();let blob=asset_blob::create_with_lease(&source,"seed","page","bin",body,writer.clone())?;let path=blob.path().to_owned();let backup=base.join("backup");
  std::fs::set_permissions(source.join("starlane-notes.db"),std::fs::Permissions::from_mode(0o600))?;
  if let Ok(ready)=std::env::var("STARLANE_SQLITE_REPLAY_SIGKILL_READY") {
   let mut marker=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(ready)?;marker.write_all(&serde_json::to_vec(&serde_json::json!({"attempt":attempt,"claim":String::from_utf8(claim.clone())?,"blob_path":path,"writer_pid":std::process::id()}))?)?;marker.sync_all()?;loop{tokio::time::sleep(std::time::Duration::from_secs(1)).await;}
  }
  ensure!(capture_sqlite_certified(&source,&backup,"v1.5.1",true).await.is_err(),"Actual active native Replay excludes standard SQLite capture");drop(blob);drop(writer);pool.close().await;std::fs::set_permissions(source.join("starlane-notes.db"),std::fs::Permissions::from_mode(0o600))?;
  let captured=capture_sqlite_certified(&source,&backup,"v1.5.1",true).await?;captured.fence()?;ensure!(captured.manifest.admission_owner_capture.as_ref().is_some_and(|c|c.scopes.iter().any(|s|s.attempt_id==attempt&&s.original_claim_json.as_bytes()==claim)),"Native standard producer must emit original bounded Replay proof");captured.db.close().await;drop(captured);std::fs::rename(&source,base.join("original-source-kept"))?;ensure!(!source.exists(),"Genuine original source absent");
  let target=base.join("cold-restored");crate::storage::backup_restore::run(&backup,&target,"v1.5.1").await?;let restored=sqlx::any::AnyPoolOptions::new().max_connections(2).connect(&format!("sqlite://{}?mode=rw",target.join("starlane-notes.db").display())).await?;activate_in(&target,&restored).await.map_err(|_|anyhow::anyhow!("Fresh restored source activation refused"))?;
  let owner=acquire_admission_authority(&target,&restored,&attempt,&claim).await?;owner.check()?;let mut conn=restored.acquire().await?;owner.check_in(&mut conn).await?;drop(conn);ensure!(WriterLease::open_idle(&target,&attempt).is_err(),"No copied writer grant");ensure!(std::fs::read(target.join(&path))?==body,"Exact Replay original bytes retained");let charged:(i64,String)=sqlx::query_as("SELECT maximum_bytes,state FROM attachment_capacity_reservations WHERE operation_id=?").bind(&attempt).fetch_one(&restored).await?;ensure!(charged==(64,"reserved".into()),"Original reserved charge retained");drop(owner);
  restored.close().await;
  let prior_backup=base.join("prior-backup");let prior=capture_sqlite_certified(&target,&prior_backup,"v1.5.1",true).await?;ensure!(prior.manifest.admission_owner_capture.is_none()&&prior.manifest.prior_admission_owner_capture.as_ref().is_some_and(|c|c.scopes.iter().any(|s|s.attempt_id==attempt)),"Prior finalized Replay must keep distinct generation evidence");prior.db.close().await;drop(prior);
  let second=base.join("second-cold-restored");crate::storage::backup_restore::run(&prior_backup,&second,"v1.5.1").await?;let second_pool=sqlx::any::AnyPoolOptions::new().connect(&format!("sqlite://{}?mode=rw",second.join("starlane-notes.db").display())).await?;activate_in(&second,&second_pool).await.map_err(|_|anyhow::anyhow!("Second fresh activation refused"))?;let repeated=acquire_admission_authority(&second,&second_pool,&attempt,&claim).await?;repeated.check()?;ensure!(WriterLease::open_idle(&second,&attempt).is_err(),"Repeated restoration cannot construct original writer");ensure!(std::fs::read(second.join(&path))?==body,"Repeated original Replay bytes differ");drop(repeated);second_pool.close().await;
  std::fs::write(target.join("spaces/seed/docs/page.md"),format!("Protected current canonical ![asset](/api/v1/files/{path})\n"))?;
  restored.close().await;let refused=base.join("protected-prior-backup");let result=capture_sqlite_certified(&target,&refused,"v1.5.1",true).await;ensure!(result.is_err(),"Prior source protected canonical reference must refuse capture");let refusal=format!("{:#}",result.as_ref().err().unwrap());ensure!(refusal.contains("Prior Replay source reference refusal")&&refusal.contains("protects Replay blob"),"Prior source must refuse through exact typed protected-reference proof: {refusal}");ensure!(!refused.exists(),"Protected source cannot publish a prior backup");let restored=sqlx::any::AnyPoolOptions::new().connect(&format!("sqlite://{}?mode=rw",target.join("starlane-notes.db").display())).await?;let charge:i64=sqlx::query_scalar("SELECT maximum_bytes FROM attachment_capacity_reservations WHERE operation_id=?").bind(&attempt).fetch_one(&restored).await?;ensure!(charge==64,"Prior capture refusal retains original reserved charge");restored.close().await;
  println!("ACTUAL_STANDARD_SQLITE_NATIVE_REPLAY_CAPTURE_ORIGINAL_SOURCE_ABSENT_COLD_RESTORE_FINALIZED_EXACT_BYTES_CHARGE_NO_WRITER_GRANT_PASS");Ok(())
 }
 #[tokio::test]
 #[ignore="genuine owned live/SIGKILL SQLite Replay producer process"]
 async fn required_actual_sqlite_replay_sigkill_capture_cold_restore()->anyhow::Result<()> {
  let base=PathBuf::from(std::env::var("STARLANE_SQLITE_REPLAY_SIGKILL_BASE")?);let source=base.join("source");let state:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::var("STARLANE_SQLITE_REPLAY_SIGKILL_READY")?)?)?;let attempt=state["attempt"].as_str().ok_or_else(||anyhow::anyhow!("Original crash attempt missing"))?;let claim=state["claim"].as_str().ok_or_else(||anyhow::anyhow!("Original crash claim missing"))?.as_bytes();let path=state["blob_path"].as_str().ok_or_else(||anyhow::anyhow!("Original crash blob path missing"))?;
  let backup=base.join(if std::env::var("STARLANE_REPLAY_SIGKILL_ACTIVE_ONLY").is_ok(){"active-refused-backup"}else{"sigkill-backup"});
  if std::env::var("STARLANE_REPLAY_SIGKILL_ACTIVE_ONLY").is_ok(){ensure!(capture_sqlite_certified(&source,&backup,"v1.5.1",true).await.is_err(),"Live original SQLite writer must retain capture exclusion");ensure!(!backup.exists(),"Active SQLite writer must prevent publication");println!("ACTUAL_SQLITE_REPLAY_LIVE_PROCESS_CAPTURE_REFUSAL_PASS");return Ok(())}
  let captured=capture_sqlite_certified(&source,&backup,"v1.5.1",true).await?;captured.fence()?;ensure!(captured.manifest.admission_owner_capture.as_ref().is_some_and(|c|c.scopes.iter().any(|s|s.attempt_id==attempt&&s.original_claim_json.as_bytes()==claim)),"Killed SQLite source must preserve exact native claim");captured.db.close().await;drop(captured);std::fs::rename(&source,base.join("sigkill-original-source-kept"))?;ensure!(!source.exists(),"Genuine killed original source absent before cold restore");
  let target=base.join("sigkill-cold-restored");crate::storage::backup_restore::run(&backup,&target,"v1.5.1").await?;let restored=sqlx::any::AnyPoolOptions::new().connect(&format!("sqlite://{}?mode=rw",target.join("starlane-notes.db").display())).await?;activate_in(&target,&restored).await.map_err(|_|anyhow::anyhow!("Fresh killed SQLite restoration activation refused"))?;let authority=acquire_admission_authority(&target,&restored,attempt,claim).await?;authority.check()?;ensure!(std::fs::read(target.join(path))?==b"original native SQLite replay binary","Killed SQLite original bytes changed");let charge:(i64,String)=sqlx::query_as("SELECT maximum_bytes,state FROM attachment_capacity_reservations WHERE operation_id=?").bind(attempt).fetch_one(&restored).await?;ensure!(charge==(64,"reserved".into()),"Killed SQLite source must retain original charge");ensure!(crate::storage::attachment_upload::admission::WriterLease::open_idle(&target,attempt).is_err(),"Killed SQLite source cannot rebind copied writer");drop(authority);restored.close().await;println!("ACTUAL_SQLITE_REPLAY_SIGKILL_SOURCE_ABSENT_STANDARD_CAPTURE_COLD_RESTORE_EXACT_CLAIM_BYTES_CHARGE_NO_WRITER_PASS");Ok(())
 }

}


#[cfg(test)]
mod original_capture_process_exclusion_tests {
 use super::*;
 #[tokio::test]
 #[ignore="requires trusted host Python and unique owned real SQLite capture source"]
 async fn original_capture_refuses_actual_foreign_write_open_after_physical_fences() { run_source_process_gate("delete").await; }
 #[tokio::test]
 #[ignore="requires trusted host Python and unique owned WAL capture source"]
 async fn original_wal_capture_refuses_actual_foreign_write_open_after_physical_fences() { run_source_process_gate("wal").await; }
 async fn run_source_process_gate(journal:&str) {
  use std::os::unix::fs::{DirBuilderExt,PermissionsExt};
  sqlx::any::install_default_drivers();
  let base=std::env::temp_dir().join(format!("original-source-process-fence-{}",uuid::Uuid::new_v4()));
  let source=base.join("source");std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&source).unwrap();
  let url=format!("sqlite://{}?mode=rwc",source.join("starlane-notes.db").display());
  let pool=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&url).await.unwrap();crate::db::selected_migrator(crate::db::DatabaseEngine::Sqlite).run(&pool).await.unwrap();
  let mode:String=sqlx::query_scalar(&format!("PRAGMA journal_mode={journal}")).fetch_one(&pool).await.unwrap();assert_eq!(mode,journal);pool.close().await;
  std::fs::set_permissions(source.join("starlane-notes.db"),std::fs::Permissions::from_mode(0o600)).unwrap();
  let seen=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let observed=seen.clone();let original_source=source.clone();
  *sqlite_capture_test_hook().lock().unwrap()=Some(Box::new(move|phase,path,_,_| {
   if phase!="helper-spawned"||path!=original_source{return;}
   let code=r#"import sqlite3,sys
c=sqlite3.connect(sys.argv[1],timeout=0.05)
try:
 c.execute('BEGIN IMMEDIATE')
 c.execute('UPDATE _sqlx_migrations SET execution_time=execution_time+1 WHERE version=1')
 c.commit()
 print('FOREIGN_PROCESS_COMMIT_ADMITTED')
 sys.exit(42)
except sqlite3.OperationalError as e:
 if 'locked' not in str(e).lower():raise
 c.rollback()
 print('FOREIGN_PROCESS_COMMIT_BLOCKED')
 sys.exit(10)
"#;
   let q=std::process::Command::new(SQLITE_CAPTURE_PYTHON).args(["-I","-c",code]).arg(path.join("starlane-notes.db")).output().unwrap();
   println!("ORIGINAL_SOURCE_FOREIGN_WRITER_CODE={:?} result={} stderr={}",q.status.code(),String::from_utf8_lossy(&q.stdout),String::from_utf8_lossy(&q.stderr));
   observed.store(matches!(q.status.code(),Some(10)|Some(42)),std::sync::atomic::Ordering::SeqCst);
  }));
  let result=capture_sqlite_certified(&source,&base.join("bundle"),"v1.5.1",true).await;
  println!("ORIGINAL_SOURCE_CAPTURE_RESULT journal={journal} success={} error={:?}",result.is_ok(),result.as_ref().err());
  *sqlite_capture_test_hook().lock().unwrap()=None;
  assert!(seen.load(std::sync::atomic::Ordering::SeqCst),"Actual foreign writable connection must reach its real SQL commit/lock result");
  assert!(result.is_err(),"A writable-open attempt permanently invalidates capture, even when SQL later reports locked");
  assert!(!base.join("bundle").exists(),"No source authority may publish after the helper-phase write-open");
  println!("ACTUAL_ORIGINAL_CAPTURE_SOURCE_WRITE_OPEN_PERMANENT_REFUSAL_PASS journal={journal}");
 }
}
