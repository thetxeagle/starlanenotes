// Read-only pager authority is consumed from a genuine exact writer seal. A
// filename, copied receipt, JSON report or caller-supplied fingerprint cannot
// construct this type. It grants no completed verification/serving authority.
struct VerificationReaderBinding {fd:i32,pin:File,methods:usize,_unix_name:Vec<u8>}
enum VerificationReaderAuthority {Sealed(Arc<VerificationWriterSeal>),Inherited(Arc<InheritedVerificationCustody>)}
impl VerificationReaderAuthority {
 fn root(&self)->&File {match self{Self::Sealed(s)=>&s.state.root,Self::Inherited(s)=>s.root.fd()}}
 fn path(&self)->&Path {match self{Self::Sealed(s)=>&s.state.absolute,Self::Inherited(s)=>&s.root.absolute}}
 fn database(&self)->Identity {match self{Self::Sealed(s)=>s.state.database,Self::Inherited(s)=>s.database_identity}}
 fn stamp(&self)->&PhysicalStamp {match self{Self::Sealed(s)=>&s.stamp,Self::Inherited(s)=>&s.inventory.files["starlane-notes.db"].1}}
 fn held_main(&self)->&File {match self{Self::Sealed(s)=>&s.held,Self::Inherited(s)=>&s.database}}
 fn fence(&self)->io::Result<()> {match self{Self::Sealed(s)=>s.fence(),Self::Inherited(s)=>s.fence().map_err(|e|fail(&e.to_string()))}}
 fn io_fence(&self)->io::Result<()> {match self{Self::Sealed(s)=>{
   let selected=&s.state;selected.active.root.check()?;
   for(held,stamp)in &selected.parent_stamps {if physical_stamp(held)?!=*stamp{
    #[cfg(test)] eprintln!("READONLY_ANCESTOR_REFUSAL fd={} path={} before={stamp:?} after={:?}",held.as_raw_fd(),std::fs::read_link(format!("/proc/self/fd/{}",held.as_raw_fd()))?.display(),physical_stamp(held)?);
    return Err(fail("Readonly selected ancestor changed"));}}
   if physical_stamp(&selected.root)?!=*selected.root_stamp.lock().map_err(|_|fail("Selected root stamp poisoned"))?{return Err(fail("Readonly original root changed"));}Ok(())
  },Self::Inherited(s)=>s.io_fence()}}
}
struct VerificationReaderState {
 retiring:std::sync::atomic::AtomicBool,
 closing:std::sync::atomic::AtomicUsize,close_failed:std::sync::atomic::AtomicBool,
 authority:VerificationReaderAuthority,main_name:String,
 bindings:std::sync::Mutex<std::collections::BTreeMap<usize,VerificationReaderBinding>>,
 methods:std::sync::Mutex<Vec<Box<libsqlite3_sys::sqlite3_io_methods>>>,
 original:usize,
}
// Hold the SQLite shared range on the actual data descriptor. SQLite's
// process-owned F_SETLK locks disappear when an unrelated descriptor for the
// same inode closes; this open-file-description lock survives those fences and
// conflicts with an ordinary SQLite writer's exclusive commit lock. It grants
// no identity authority and every original stamp/native fence remains required.
fn verification_reader_shared_lease(file:&File)->io::Result<()> {
 let mut lock:libc::flock=unsafe{std::mem::zeroed()};
 lock.l_type=libc::F_RDLCK as _;lock.l_whence=libc::SEEK_SET as _;
 lock.l_start=0x40000001;lock.l_len=511;lock.l_pid=0;
 if unsafe{libc::fcntl(file.as_raw_fd(),libc::F_OFD_SETLK,&lock)} == -1 {
  return Err(io::Error::last_os_error());
 }
 Ok(())
}
fn verification_reader_open_gate()->&'static std::sync::Mutex<()> {
 static GATE:std::sync::Mutex<()>=std::sync::Mutex::new(());&GATE
}
// Synchronization only: this flag never grants native or SQL authority.
// Both actual admission and retirement use the same gate, so a previously
// cloned state cannot register a method table after retirement takes it.
fn verification_reader_admit_gate(retiring:&std::sync::atomic::AtomicBool,length:impl FnOnce()->io::Result<usize>)->io::Result<std::sync::MutexGuard<'static,()>> {
 let gate=verification_reader_open_gate().lock().map_err(|_|fail("Readonly admission gate poisoned"))?;
 if retiring.load(std::sync::atomic::Ordering::Acquire){return Err(fail("Readonly reader is retiring"));}
 if length()?>=4{return Err(fail("Readonly native binding bound reached"));}Ok(gate)
}
fn verification_reader_stop_admissions(retiring:&std::sync::atomic::AtomicBool)->io::Result<()> {
 let _gate=verification_reader_open_gate().lock().map_err(|_|fail("Readonly admission gate poisoned"))?;
 retiring.store(true,std::sync::atomic::Ordering::Release);Ok(())
}
fn verification_reader_states()->&'static std::sync::Mutex<std::collections::BTreeMap<usize,Arc<VerificationReaderState>>> {
 static STATES:std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeMap<usize,Arc<VerificationReaderState>>>>=std::sync::OnceLock::new();
 STATES.get_or_init(||std::sync::Mutex::new(Default::default()))
}
impl VerificationReaderState {
 fn native_fence(&self)->io::Result<()> {self.authority.fence()}
 // A filename callback only maps the already selected native name. It cannot
 // certify full inventory or mint a reader role. Actual admission/final-use
 // native fences remain separate, and no successful hash check is cached.
 fn name_fence(&self)->io::Result<()> {
  self.authority.io_fence()?;
  let held=self.authority.held_main();
  let named=open_at(self.authority.root(),"starlane-notes.db",libc::O_RDONLY|libc::O_NONBLOCK)?;
  if identity(held)?!=self.authority.database()||identity(&named)?!=self.authority.database()
   ||physical_stamp(held)?!=*self.authority.stamp()||physical_stamp(&named)?!=*self.authority.stamp(){return Err(fail("Readonly original filename main identity changed"));}
  for leaf in ["starlane-notes.db-journal","starlane-notes.db-wal","starlane-notes.db-shm"] {
   if exists(self.authority.root(),leaf)?{return Err(fail("Readonly filename has an unowned sidecar"));}
  }
  self.authority.io_fence()?;
  if physical_stamp(held)?!=*self.authority.stamp()||physical_stamp(&named)?!=*self.authority.stamp(){return Err(fail("Readonly original filename main changed during mapping"));}
  Ok(())
 }

 fn io_fence(&self,b:&VerificationReaderBinding)->io::Result<()> {
  self.authority.io_fence()?;let current=open_at(self.authority.root(),"starlane-notes.db",libc::O_RDONLY)?;
  if verification_pager_fd_identity(b.fd)?!=self.authority.database()||identity(&b.pin)?!=self.authority.database()
   ||physical_stamp(&b.pin)?!=*self.authority.stamp()||physical_stamp(&current)?!=*self.authority.stamp(){return Err(fail("Readonly actual selected database changed"));}Ok(())
 }
 fn check(&self)->io::Result<()> {
  self.native_fence()?;let bindings=self.bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?;
  if bindings.is_empty(){return Err(fail("No actual readonly reader admitted"));}
  for b in bindings.values(){self.io_fence(b)?;}Ok(())
 }
}
struct VerificationReaderVfs {vfs:Box<libsqlite3_sys::sqlite3_vfs>,name:CString,state:Option<Arc<VerificationReaderState>>,retired_methods:Vec<Box<libsqlite3_sys::sqlite3_io_methods>>}
impl VerificationReaderVfs {
 fn new(seal:VerificationWriterSeal)->io::Result<Self> {
  seal.reader_ready()?;
  Self::from_authority(VerificationReaderAuthority::Sealed(Arc::new(seal)))
 }
 fn from_authority(authority:VerificationReaderAuthority)->io::Result<Self> {
  authority.fence()?;let original=unsafe{libsqlite3_sys::sqlite3_vfs_find(c"unix".as_ptr())};
  if original.is_null()||unsafe{(*original).iVersion}!=3{return Err(fail("Unsupported readonly Unix VFS"));}
  let name=CString::new(format!("starlane-sealed-reader-{}",uuid::Uuid::new_v4())).unwrap();
  let state=Arc::new(VerificationReaderState{retiring:std::sync::atomic::AtomicBool::new(false),closing:std::sync::atomic::AtomicUsize::new(0),close_failed:std::sync::atomic::AtomicBool::new(false),main_name:format!("/proc/self/fd/{}/starlane-notes.db",authority.root().as_raw_fd()),authority,bindings:std::sync::Mutex::new(Default::default()),methods:std::sync::Mutex::new(Vec::new()),original:original as usize});
  let mut vfs=Box::new(unsafe{std::ptr::read(original)});vfs.pNext=std::ptr::null_mut();vfs.zName=name.as_ptr();
  vfs.xOpen=Some(verification_reader_open);vfs.xDelete=Some(verification_reader_delete);vfs.xFullPathname=Some(verification_reader_full_path);vfs.xAccess=Some(verification_reader_access);
  let key=&*vfs as *const _ as usize;
  verification_reader_states().lock().map_err(|_|fail("Reader registry poisoned"))?.insert(key,state.clone());
  if unsafe{libsqlite3_sys::sqlite3_vfs_register(&mut *vfs,0)}!=libsqlite3_sys::SQLITE_OK {verification_reader_states().lock().map_err(|_|fail("Reader registry poisoned"))?.remove(&key);return Err(fail("Cannot register readonly reader"));}
  state.native_fence()?;Ok(Self{vfs,name,state:Some(state),retired_methods:Vec::new()})
 }
}
impl VerificationReaderVfs {
 fn state(&self)->&Arc<VerificationReaderState> {self.state.as_ref().expect("Active readonly VFS state")}
 fn stop_admissions(&self)->io::Result<()> {verification_reader_stop_admissions(&self.state().retiring)}
 fn retire(mut self) {
  debug_assert!(self.state().retiring.load(std::sync::atomic::Ordering::Acquire));
  // Keep only inert native allocations until process exit. Actual callback
  // drain precedes this point; no data descriptor/actor/lease remains in the
  // retired allocation, and the active callback authority registry is removed.
  unsafe{libsqlite3_sys::sqlite3_vfs_unregister(&mut *self.vfs);}
  if let Ok(mut states)=verification_reader_states().lock(){states.remove(&(&*self.vfs as *const _ as usize));}
  let methods=std::mem::take(&mut *self.state().methods.lock().expect("Drained readonly methods"));self.retired_methods=methods;
  drop(self.state.take());
  std::mem::forget(self);
 }
 async fn native_close_ack(&self)->anyhow::Result<()> {
  let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
  loop {
   anyhow::ensure!(!self.state().close_failed.load(std::sync::atomic::Ordering::Acquire),"Actual readonly native close refused");
   if self.state().bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?.is_empty()&&self.state().closing.load(std::sync::atomic::Ordering::Acquire)==0{return Ok(());}
   anyhow::ensure!(std::time::Instant::now()<deadline,"Actual readonly native callbacks remain unresolved");
   tokio::time::sleep(std::time::Duration::from_millis(10)).await;
  }
 }
}
struct VerificationReaderCloseAck {state:Arc<VerificationReaderState>,ok:bool}
impl Drop for VerificationReaderCloseAck {
 fn drop(&mut self){if !self.ok{self.state.close_failed.store(true,std::sync::atomic::Ordering::Release);}self.state.closing.fetch_sub(1,std::sync::atomic::Ordering::AcqRel);}
}
impl Drop for VerificationReaderVfs {
 fn drop(&mut self){unsafe{libsqlite3_sys::sqlite3_vfs_unregister(&mut *self.vfs);}if let Ok(mut states)=verification_reader_states().lock(){states.remove(&(&*self.vfs as *const _ as usize));}}
}
unsafe extern "C" fn verification_reader_open(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,file:*mut libsqlite3_sys::sqlite3_file,flags:i32,out:*mut i32)->i32 {
 std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let result=(||->io::Result<i32>{
   let state=verification_reader_states().lock().map_err(|_|fail("Reader registry poisoned"))?.get(&(vfs as usize)).cloned().ok_or_else(||fail("Unknown readonly reader"))?;
   state.native_fence()?;
   if name.is_null()||file.is_null(){return Err(fail("Readonly temporary file refused"));}
   let raw=unsafe{std::ffi::CStr::from_ptr(name)};
   if raw.to_str().ok()!=Some(state.main_name.as_str())||flags&libsqlite3_sys::SQLITE_OPEN_MAIN_DB==0
    ||flags&libsqlite3_sys::SQLITE_OPEN_READONLY==0||flags&(libsqlite3_sys::SQLITE_OPEN_READWRITE|libsqlite3_sys::SQLITE_OPEN_CREATE|libsqlite3_sys::SQLITE_OPEN_DELETEONCLOSE)!=0{return Err(fail("Unowned readonly open refused"));}
   let _gate=verification_reader_admit_gate(&state.retiring,||Ok(state.bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?.len()))?;
   // The serving descriptor is owned before SQLite sees any pathname. Unix VFS
   // retains its native lock machinery; its inode FD cache is never data authority.
   let pin=open_at(state.authority.root(),"starlane-notes.db",libc::O_RDONLY)?;
   verification_reader_shared_lease(&pin)?;
   let fd=pin.as_raw_fd();
   // Unix opens with O_NOFOLLOW, so its lock pathname must end in the actual
   // regular leaf under the held root FD. Before/after full root+main stamps
   // refuse any terminal-path exchange; serving bytes still use only pin.
   let mut unix_name=state.main_name.as_bytes().to_vec();unix_name.extend_from_slice(&[0,0]);
   let original=state.original as *mut libsqlite3_sys::sqlite3_vfs;
   let mut binding=VerificationReaderBinding{fd,pin,methods:0,_unix_name:unix_name};state.io_fence(&binding)?;state.native_fence()?;
   let rc=unsafe{(*original).xOpen.ok_or_else(||fail("Unix readonly xOpen missing"))?(original,binding._unix_name.as_ptr().cast(),file,flags&!libsqlite3_sys::SQLITE_OPEN_URI,out)};
   if rc!=libsqlite3_sys::SQLITE_OK{return Ok(rc);}
   let admitted=(||->io::Result<()> {
    let original_methods=unsafe{(*file).pMethods};if original_methods.is_null(){return Err(fail("Readonly IO methods missing"));}
    binding.methods=original_methods as usize;state.io_fence(&binding)?;state.native_fence()?;
    let mut methods=Box::new(unsafe{std::ptr::read(original_methods)});
    methods.xRead=Some(verification_reader_read);methods.xFileSize=Some(verification_reader_size);methods.xFileControl=Some(verification_reader_control);methods.xSync=Some(verification_reader_no_sync);
    methods.xWrite=Some(verification_reader_write);methods.xTruncate=Some(verification_reader_truncate);methods.xClose=Some(verification_reader_close);
    methods.xFetch=Some(verification_reader_no_fetch);methods.xUnfetch=Some(verification_reader_no_unfetch);
    methods.xShmMap=Some(verification_pager_deny_shm_map);methods.xShmLock=Some(verification_pager_deny_shm_lock);methods.xShmUnmap=Some(verification_pager_no_shm_unmap);
    let pointer=&*methods as *const _;state.methods.lock().map_err(|_|fail("Reader methods poisoned"))?.push(methods);
    state.bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?.insert(file as usize,binding);unsafe{(*file).pMethods=pointer;}Ok(())
   })();
   if let Err(error)=admitted{tracing::error!(error=%error,"Actual readonly descriptor admission refused");let methods=unsafe{(*file).pMethods};if !methods.is_null(){if let Some(close)=unsafe{(*methods).xClose}{unsafe{close(file);}}}unsafe{(*file).pMethods=std::ptr::null();}return Err(fail("Readonly FD admission refused"));}Ok(rc)
  })();result.unwrap_or_else(|error|{tracing::error!(error=%error,"Original readonly VFS open refused");libsqlite3_sys::SQLITE_CANTOPEN})
 })).unwrap_or(libsqlite3_sys::SQLITE_CANTOPEN)
}
fn verification_reader_file_state(file:*mut libsqlite3_sys::sqlite3_file)->io::Result<Arc<VerificationReaderState>> {
 let states=verification_reader_states().lock().map_err(|_|fail("Reader registry poisoned"))?;
 for state in states.values(){if state.bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?.contains_key(&(file as usize)){return Ok(state.clone());}}Err(fail("Unknown actual readonly file"))
}
unsafe extern "C" fn verification_reader_read(file:*mut libsqlite3_sys::sqlite3_file,buffer:*mut libc::c_void,length:i32,offset:i64)->i32 {
 std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let result=(||->io::Result<i32>{let state=verification_reader_file_state(file)?;let bindings=state.bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?;
   let b=bindings.get(&(file as usize)).ok_or_else(||fail("Actual readonly binding missing"))?;state.io_fence(b)?;
   if buffer.is_null()||length<0||offset<0{return Err(fail("Readonly invalid read span"));}
   let output=unsafe{std::slice::from_raw_parts_mut(buffer.cast::<u8>(),length as usize)};
   use std::os::unix::fs::FileExt;let mut completed=0usize;
   while completed<output.len(){match b.pin.read_at(&mut output[completed..],offset as u64+completed as u64){Ok(0)=>break,Ok(n)=>completed+=n,Err(e)if e.kind()==io::ErrorKind::Interrupted=>continue,Err(e)=>return Err(e)}}
   state.io_fence(b)?;
   if completed<output.len(){output[completed..].fill(0);Ok(libsqlite3_sys::SQLITE_IOERR_SHORT_READ)}else{Ok(libsqlite3_sys::SQLITE_OK)}
})();result.unwrap_or(libsqlite3_sys::SQLITE_IOERR_READ)
 })).unwrap_or(libsqlite3_sys::SQLITE_IOERR_READ)
}
unsafe extern "C" fn verification_reader_control(file:*mut libsqlite3_sys::sqlite3_file,operation:i32,out:*mut libc::c_void)->i32 {
 if operation!=libsqlite3_sys::SQLITE_FCNTL_HAS_MOVED{return libsqlite3_sys::SQLITE_NOTFOUND;}
 std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let result=(||->io::Result<()> {if out.is_null(){return Err(fail("Readonly moved output absent"));}let state=verification_reader_file_state(file)?;let bindings=state.bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?;
   state.io_fence(bindings.get(&(file as usize)).ok_or_else(||fail("Readonly moved binding absent"))?)?;unsafe{*out.cast::<i32>()=0;}Ok(())})();if result.is_ok(){libsqlite3_sys::SQLITE_OK}else{libsqlite3_sys::SQLITE_IOERR}
 })).unwrap_or(libsqlite3_sys::SQLITE_IOERR)
}
unsafe extern "C" fn verification_reader_no_sync(_: *mut libsqlite3_sys::sqlite3_file,_:i32)->i32 {libsqlite3_sys::SQLITE_READONLY}
unsafe extern "C" fn verification_reader_size(file:*mut libsqlite3_sys::sqlite3_file,out:*mut i64)->i32 {
 std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let result=(||->io::Result<()> {if out.is_null(){return Err(fail("Readonly size output missing"));}let state=verification_reader_file_state(file)?;
   let bindings=state.bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?;let b=bindings.get(&(file as usize)).ok_or_else(||fail("Actual readonly size binding missing"))?;
   state.io_fence(b)?;let size=i64::try_from(b.pin.metadata()?.len()).map_err(|_|fail("Readonly size overflow"))?;state.io_fence(b)?;unsafe{*out=size;}Ok(())})();if let Err(error)=&result {
  #[cfg(test)] eprintln!("ACTUAL_READONLY_SIZE_REFUSAL: {error}");
  let _=error;
 }match result {Ok(())=>libsqlite3_sys::SQLITE_OK,Err(error)=>{
  tracing::error!(error=%error,"Original readonly SQLite size predicate refused");
  libsqlite3_sys::SQLITE_IOERR_FSTAT
 }}
 })).unwrap_or_else(|_|{tracing::error!("Original readonly SQLite size predicate panicked");libsqlite3_sys::SQLITE_IOERR_FSTAT})
}
unsafe extern "C" fn verification_reader_no_fetch(_: *mut libsqlite3_sys::sqlite3_file,_:i64,_:i32,out:*mut *mut libc::c_void)->i32 {if out.is_null(){return libsqlite3_sys::SQLITE_IOERR;}unsafe{*out=std::ptr::null_mut();}libsqlite3_sys::SQLITE_OK}
unsafe extern "C" fn verification_reader_no_unfetch(_: *mut libsqlite3_sys::sqlite3_file,_:i64,_:*mut libc::c_void)->i32 {libsqlite3_sys::SQLITE_OK}
unsafe extern "C" fn verification_reader_close(file:*mut libsqlite3_sys::sqlite3_file)->i32 {
 std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let result=(||->io::Result<i32>{let state=verification_reader_file_state(file)?;state.closing.fetch_add(1,std::sync::atomic::Ordering::AcqRel);let mut ack=VerificationReaderCloseAck{state:state.clone(),ok:false};let mut bindings=state.bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?;
   let b=bindings.remove(&(file as usize)).ok_or_else(||fail("Actual readonly close binding missing"))?;
   // Refuse closing a foreign reused kernel FD; no cleanup authority is inferred.
   if verification_pager_fd_identity(b.fd)?!=state.authority.database(){return Err(fail("Actual readonly close FD exchanged"));}
   let methods=b.methods as *const libsqlite3_sys::sqlite3_io_methods;let close=unsafe{(*methods).xClose}.ok_or_else(||fail("Unix readonly xClose missing"))?;drop(bindings);
   let rc=unsafe{close(file)};drop(b);ack.ok=rc==libsqlite3_sys::SQLITE_OK;Ok(rc)})();result.unwrap_or(libsqlite3_sys::SQLITE_IOERR_CLOSE)
 })).unwrap_or(libsqlite3_sys::SQLITE_IOERR_CLOSE)
}
unsafe extern "C" fn verification_reader_write(_: *mut libsqlite3_sys::sqlite3_file,_:*const libc::c_void,_:i32,_:i64)->i32 {libsqlite3_sys::SQLITE_READONLY}
unsafe extern "C" fn verification_reader_truncate(_: *mut libsqlite3_sys::sqlite3_file,_:i64)->i32 {libsqlite3_sys::SQLITE_READONLY}
unsafe extern "C" fn verification_reader_delete(_: *mut libsqlite3_sys::sqlite3_vfs,_:*const libc::c_char,_:i32)->i32 {libsqlite3_sys::SQLITE_IOERR_DELETE}
unsafe extern "C" fn verification_reader_full_path(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,n:i32,out:*mut libc::c_char)->i32 {
 std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let result=(||->io::Result<()> {let state=verification_reader_states().lock().map_err(|_|fail("Reader registry poisoned"))?.get(&(vfs as usize)).cloned().ok_or_else(||fail("Unknown readonly VFS"))?;state.name_fence()?;
   if name.is_null()||out.is_null(){return Err(fail("Readonly pathname absent"));}let raw=unsafe{std::ffi::CStr::from_ptr(name)};
   if raw.to_str().ok()!=Some(state.main_name.as_str())||n<=raw.to_bytes().len()as i32{return Err(fail("Unowned readonly pathname"));}
   unsafe{std::ptr::copy_nonoverlapping(raw.as_ptr(),out,raw.to_bytes_with_nul().len());}state.name_fence()?;Ok(())})();if result.is_ok(){libsqlite3_sys::SQLITE_OK}else{libsqlite3_sys::SQLITE_CANTOPEN}
 })).unwrap_or(libsqlite3_sys::SQLITE_CANTOPEN)
}
unsafe extern "C" fn verification_reader_access(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,flags:i32,out:*mut i32)->i32 {
 std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let result=(||->io::Result<i32>{let state=verification_reader_states().lock().map_err(|_|fail("Reader registry poisoned"))?.get(&(vfs as usize)).cloned().ok_or_else(||fail("Unknown readonly VFS"))?;state.name_fence()?;
   if name.is_null()||out.is_null(){return Err(fail("Readonly access pathname absent"));}let name_str=unsafe{std::ffi::CStr::from_ptr(name)}.to_str().map_err(|_|fail("Readonly access invalid"))?;
   if [format!("{}-journal",state.main_name),format!("{}-wal",state.main_name),format!("{}-shm",state.main_name)].contains(&name_str.to_owned()) {unsafe{*out=0;}state.name_fence()?;return Ok(libsqlite3_sys::SQLITE_OK);}
   if name_str!=state.main_name{return Err(fail("Unowned readonly access"));}
   let original=state.original as *mut libsqlite3_sys::sqlite3_vfs;let result=unsafe{(*original).xAccess.ok_or_else(||fail("Unix readonly xAccess missing"))?(original,name,flags,out)};state.name_fence()?;Ok(result)})();result.unwrap_or(libsqlite3_sys::SQLITE_IOERR_ACCESS)
 })).unwrap_or(libsqlite3_sys::SQLITE_IOERR_ACCESS)
}
struct VerificationSealedReader {pool:sqlx::AnyPool,vfs:Option<VerificationReaderVfs>,source:Option<Arc<crate::storage::recovery_records::PoolSource>>}
impl VerificationSealedReader {
 async fn enter(seal:VerificationWriterSeal)->anyhow::Result<Self> {
  Self::enter_vfs(VerificationReaderVfs::new(seal)?).await
 }
 async fn enter_inherited(custody:Arc<InheritedVerificationCustody>)->anyhow::Result<Self> {
  Self::enter_vfs(VerificationReaderVfs::from_authority(VerificationReaderAuthority::Inherited(custody))?).await
 }
 async fn enter_vfs(vfs:VerificationReaderVfs)->anyhow::Result<Self> {
  let url=format!("sqlite://{}?mode=ro&vfs={}",vfs.state().main_name,vfs.name.to_str()?);
  // On failed/cancelled initialization callbacks stay alive until process exit.
  let mut reader=Self{pool:sqlx::any::AnyPoolOptions::new().connect_lazy(&url)?,vfs:Some(vfs),source:None};
  let pool=sqlx::any::AnyPoolOptions::new().min_connections(2).max_connections(4).idle_timeout(None).max_lifetime(None).connect(&url).await?;
  let registered=reader.vfs.as_ref().ok_or_else(||fail("Reader registration missing"))?;registered.state().check()?;
  crate::storage::recovery_records::retain_pool_source(&pool,registered.state().authority.path());
  anyhow::ensure!(crate::storage::recovery_records::pool_source(&pool,registered.state().authority.path()).is_some(),"Actual sealed-reader initialized source required");registered.state().check()?;
  reader.source=crate::storage::recovery_records::pool_source(&pool,registered.state().authority.path());reader.pool=pool;reader.fence()?;Ok(reader)
 }
 fn fence(&self)->anyhow::Result<()> {
  let state=self.vfs.as_ref().ok_or_else(||fail("Reader registration absent"))?.state();state.check()?;
  let actual=crate::storage::recovery_records::pool_source(&self.pool,state.authority.path()).ok_or_else(||fail("Original initialized readonly pool source lost"))?;
  anyhow::ensure!(self.source.as_ref().is_some_and(|prior|Arc::ptr_eq(prior,&actual)),"Readonly pool source replaced");state.check()?;Ok(())
 }
 async fn close(mut self)->anyhow::Result<()> {self.fence()?;self.vfs.as_ref().ok_or_else(||fail("Reader registration absent"))?.stop_admissions()?;self.pool.close().await;let guard=self.vfs.as_ref().ok_or_else(||fail("Reader registration absent"))?;guard.native_close_ack().await?;guard.state().native_fence()?;self.vfs.take().ok_or_else(||fail("Reader registration absent"))?.retire();Ok(())}
}
impl Drop for VerificationSealedReader {fn drop(&mut self){if let Some(vfs)=self.vfs.take(){std::mem::forget(vfs);}}}

#[cfg(test)]
async fn verification_reader_actual_name_control(reader:&VerificationSealedReader,target:&Path,mode:u8)->anyhow::Result<()>{
 use anyhow::{ensure,Context};use std::os::unix::fs::{MetadataExt,FileExt};
 let vfs=reader.vfs.as_ref().context("Actual selected reader VFS absent")?;let state=vfs.state();let pointer=&*vfs.vfs as *const _ as *mut libsqlite3_sys::sqlite3_vfs;
 let main=CString::new(state.main_name.clone())?;
 let access=|name:&CString|{let mut out=71;let rc=unsafe{verification_reader_access(pointer,name.as_ptr(),libsqlite3_sys::SQLITE_ACCESS_EXISTS,&mut out)};(rc,out)};
 let full=|name:&CString,n:i32|{let mut output=vec![0i8;4096];let rc=unsafe{verification_reader_full_path(pointer,name.as_ptr(),n,output.as_mut_ptr())};(rc,output)};
 state.name_fence()?;let(rc,value)=access(&main);ensure!(rc==libsqlite3_sys::SQLITE_OK&&value==1,"Exact original main name refused");
 let(rc,out)=full(&main,4096);ensure!(rc==libsqlite3_sys::SQLITE_OK&&unsafe{std::ffi::CStr::from_ptr(out.as_ptr())}.to_bytes()==main.as_bytes(),"Exact original full name changed");
 ensure!(full(&main,main.as_bytes().len()as i32).0!=libsqlite3_sys::SQLITE_OK,"Short full-name buffer accepted");
 let foreign=CString::new(format!("{}-foreign",state.main_name))?;ensure!(access(&foreign).0!=libsqlite3_sys::SQLITE_OK&&full(&foreign,4096).0!=libsqlite3_sys::SQLITE_OK,"Foreign name or alias accepted");
 for suffix in ["-journal","-wal","-shm"]{let side=CString::new(format!("{}{suffix}",state.main_name))?;ensure!(access(&side)==(libsqlite3_sys::SQLITE_OK,0),"Absent owned sidecar was not safely resolved");}
 if mode==29{return Ok(());}
 if (30..=32).contains(&mode){let suffix=["-journal","-wal","-shm"][(mode-30)as usize];std::fs::write(target.join(format!("starlane-notes.db{suffix}")),b"FOREIGN SIDECAR")?;let side=CString::new(format!("{}{suffix}",state.main_name))?;ensure!(access(&side).0!=libsqlite3_sys::SQLITE_OK,"Actual foreign sidecar was reported absent");}
 if mode==33{let path=target.join("starlane-notes.db");let held=target.join("kept-original-main");let foreign=target.join("foreign-equal-main");std::fs::copy(&path,&foreign)?;ensure!(std::fs::read(&path)?==std::fs::read(&foreign)?,"Equal-content actual foreign inode fixture differs");std::fs::rename(&path,&held)?;std::fs::rename(&foreign,&path)?;ensure!(access(&main).0!=libsqlite3_sys::SQLITE_OK&&full(&main,4096).0!=libsqlite3_sys::SQLITE_OK,"Equal-byte foreign main name accepted");std::fs::rename(&path,&foreign)?;std::fs::rename(&held,&path)?;}
 if mode==34{let file=std::fs::OpenOptions::new().read(true).write(true).open(target.join("starlane-notes.db"))?;let before=file.metadata()?;let mut byte=[0u8];file.read_exact_at(&mut byte,100)?;file.write_all_at(&[byte[0]^1],100)?;let times=[libc::timespec{tv_sec:0,tv_nsec:libc::UTIME_OMIT},libc::timespec{tv_sec:before.mtime(),tv_nsec:before.mtime_nsec()}];ensure!(unsafe{libc::futimens(file.as_raw_fd(),times.as_ptr())}==0,"Controlled original mtime restoration failed");let after=file.metadata()?;ensure!(after.ino()==before.ino()&&after.len()==before.len()&&after.mtime()==before.mtime()&&after.mtime_nsec()==before.mtime_nsec()&&(after.ctime(),after.ctime_nsec())!=(before.ctime(),before.ctime_nsec()),"Actual same-inode/restored-mtime control absent");}
 if mode==35||mode==36{let path=if mode==35{let path:String=sqlx::query_scalar("SELECT storage_path FROM attachments LIMIT 1").fetch_one(&reader.pool).await?;ensure!(valid_relative(&path),"Actual unrelated asset path invalid");target.join(path)}else{target.join(NAMESPACE).join(LEDGER_NAME)};let file=std::fs::OpenOptions::new().read(true).write(true).open(path)?;let mut byte=[0u8];file.read_exact_at(&mut byte,0)?;file.write_all_at(&[byte[0]^1],0)?;ensure!(state.name_fence().is_ok(),"Name-only map unexpectedly certified unrelated inventory");ensure!(state.check().is_err(),"Actual fullnative/final-use boundary accepted unrelated asset/control mutation");return Ok(());}
 ensure!(state.name_fence().is_err()&&access(&main).0!=libsqlite3_sys::SQLITE_OK&&full(&main,4096).0!=libsqlite3_sys::SQLITE_OK,"Changed original main/sidecar/return mapping accepted");ensure!(state.check().is_err(),"Actual fullnative use accepted changed original reader");Ok(())
}

#[cfg(test)]mod verification_reader_retirement_controls {
 use super::*;
 #[test]fn simultaneous_admissions_recheck_four_binding_bound_inside_the_actual_gate()->anyhow::Result<()> {
  use std::sync::{Arc,atomic::{AtomicBool,AtomicUsize,Ordering},Barrier};
  let flag=Arc::new(AtomicBool::new(false));let count=Arc::new(AtomicUsize::new(3));let start=Arc::new(Barrier::new(3));
  let mut threads=Vec::new();for _ in 0..2 {let(flag,count,start)=(flag.clone(),count.clone(),start.clone());threads.push(std::thread::spawn(move||{start.wait();let gate=verification_reader_admit_gate(&flag,||Ok(count.load(Ordering::Acquire)));match gate{Ok(_held)=>{count.fetch_add(1,Ordering::Release);true},Err(_)=>false}}));}
  start.wait();let accepted=threads.into_iter().map(|t|t.join().unwrap()).filter(|v|*v).count();
  anyhow::ensure!(accepted==1&&count.load(Ordering::Acquire)==4,"Parallel admissions exceeded actual four-binding bound");Ok(())
 }
 #[test]fn earlier_admission_drains_before_retirement_and_late_cloned_state_refuses()->anyhow::Result<()> {
  use std::sync::{Arc,atomic::{AtomicBool,Ordering},mpsc};
  let flag=Arc::new(AtomicBool::new(false));let first=verification_reader_admit_gate(&flag,||Ok(0))?;
  let cloned=flag.clone();let(done_tx,done_rx)=mpsc::channel();
  let retire=std::thread::spawn(move||{verification_reader_stop_admissions(&cloned).unwrap();done_tx.send(()).unwrap();});
  anyhow::ensure!(done_rx.recv_timeout(std::time::Duration::from_millis(30)).is_err(),"Retirement skipped an admitted registration");
  anyhow::ensure!(!flag.load(Ordering::Acquire),"Retirement changed flag outside admission gate");drop(first);
  done_rx.recv_timeout(std::time::Duration::from_secs(2))?;retire.join().unwrap();
  anyhow::ensure!(flag.load(Ordering::Acquire)&&verification_reader_admit_gate(&flag,||Ok(0)).is_err(),"Previously cloned state admitted after retirement");
  Ok(())
 }
}
