// Purpose-private READ511 mechanics copied from exact reviewed CORE8 reader; default reader is unchanged.
// Read-only pager authority consumes the actual original historical publication,
// activation and initialized target pool. A filename, copied receipt or report
// cannot construct it. It grants no application/record/cleanup authority.
struct HistoricalReadonlyReaderBinding {fd:i32,pin:File,methods:usize,_unix_name:Vec<u8>}
enum HistoricalReadonlyReaderAuthority { Original(Arc<HistoricalReadonlyAdmission>), Child(Arc<HistoricalReadonlyChildCustody>) }
impl HistoricalReadonlyReaderAuthority {
 fn root(&self)->&File{match self{Self::Original(a)=>a.root(),Self::Child(a)=>a.root.fd()}}
 fn path(&self)->&Path{match self{Self::Original(a)=>a.path(),Self::Child(a)=>&a.root.absolute}}
 fn database(&self)->Identity{match self{Self::Original(a)=>a.database(),Self::Child(a)=>(a.main_stamp.device,a.main_stamp.inode)}}
 fn stamp(&self)->&PhysicalStamp{match self{Self::Original(a)=>a.stamp(),Self::Child(a)=>&a.main_stamp}}
 fn held_main(&self)->&File{match self{Self::Original(a)=>a.held_main(),Self::Child(a)=>a.main.as_ref()}}
 fn io_fence(&self)->io::Result<()>{match self{Self::Original(a)=>a.io_fence(),Self::Child(a)=>a.io_fence()}}
 fn fence(&self)->io::Result<()>{match self{Self::Original(a)=>a.fence(),Self::Child(a)=>a.fence()}}
 fn active(&self)->io::Result<Arc<Active>>{match self{Self::Original(a)=>Ok(a.active.clone()),Self::Child(a)=>a.active.get().cloned().ok_or_else(||fail("Historical child actual activation not yet established"))}}
}
struct HistoricalReadonlyReaderState {
 retiring:std::sync::atomic::AtomicBool,
 closing:std::sync::atomic::AtomicUsize,close_failed:std::sync::atomic::AtomicBool,
 authority:HistoricalReadonlyReaderAuthority,main_name:String,
 bindings:std::sync::Mutex<std::collections::BTreeMap<usize,HistoricalReadonlyReaderBinding>>,
 methods:std::sync::Mutex<Vec<Box<libsqlite3_sys::sqlite3_io_methods>>>,
 original:usize,
}
// Hold the SQLite shared range on the actual data descriptor. SQLite's
// process-owned F_SETLK locks disappear when an unrelated descriptor for the
// same inode closes; this open-file-description lock survives those fences and
// conflicts with an ordinary SQLite writer's exclusive commit lock. It grants
// no identity authority and every original stamp/native fence remains required.
fn historical_readonly_reader_shared_lease(file:&File)->io::Result<()> {
 let mut lock:libc::flock=unsafe{std::mem::zeroed()};
 lock.l_type=libc::F_RDLCK as _;lock.l_whence=libc::SEEK_SET as _;
 lock.l_start=0x40000001;lock.l_len=511;lock.l_pid=0;
 if unsafe{libc::fcntl(file.as_raw_fd(),libc::F_OFD_SETLK,&lock)} == -1 {
  return Err(io::Error::last_os_error());
 }
 Ok(())
}
fn historical_readonly_reader_open_gate()->&'static std::sync::Mutex<()> {
 static GATE:std::sync::Mutex<()>=std::sync::Mutex::new(());&GATE
}
// Synchronization only: this flag never grants native or SQL authority.
// Both actual admission and retirement use the same gate, so a previously
// cloned state cannot register a method table after retirement takes it.
fn historical_readonly_reader_admit_gate(retiring:&std::sync::atomic::AtomicBool,length:impl FnOnce()->io::Result<usize>)->io::Result<std::sync::MutexGuard<'static,()>> {
 let gate=historical_readonly_reader_open_gate().lock().map_err(|_|fail("Readonly admission gate poisoned"))?;
 if retiring.load(std::sync::atomic::Ordering::Acquire){return Err(fail("Readonly reader is retiring"));}
 if length()?>=4{return Err(fail("Readonly native binding bound reached"));}Ok(gate)
}
fn historical_readonly_reader_stop_admissions(retiring:&std::sync::atomic::AtomicBool)->io::Result<()> {
 let _gate=historical_readonly_reader_open_gate().lock().map_err(|_|fail("Readonly admission gate poisoned"))?;
 retiring.store(true,std::sync::atomic::Ordering::Release);Ok(())
}
fn historical_readonly_reader_states()->&'static std::sync::Mutex<std::collections::BTreeMap<usize,Arc<HistoricalReadonlyReaderState>>> {
 static STATES:std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeMap<usize,Arc<HistoricalReadonlyReaderState>>>>=std::sync::OnceLock::new();
 STATES.get_or_init(||std::sync::Mutex::new(Default::default()))
}
impl HistoricalReadonlyReaderState {
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

 fn io_fence(&self,b:&HistoricalReadonlyReaderBinding)->io::Result<()> {
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
struct HistoricalReadonlyReaderVfs {vfs:Box<libsqlite3_sys::sqlite3_vfs>,name:CString,state:Option<Arc<HistoricalReadonlyReaderState>>,retired_methods:Vec<Box<libsqlite3_sys::sqlite3_io_methods>>}
impl HistoricalReadonlyReaderVfs {
 fn from_authority(authority:HistoricalReadonlyReaderAuthority)->io::Result<Self> {
  authority.fence()?;let original=unsafe{libsqlite3_sys::sqlite3_vfs_find(c"unix".as_ptr())};
  if original.is_null()||unsafe{(*original).iVersion}!=3{return Err(fail("Unsupported readonly Unix VFS"));}
  let name=CString::new(format!("starlane-historical-readonly-{}",uuid::Uuid::new_v4())).unwrap();
  let state=Arc::new(HistoricalReadonlyReaderState{retiring:std::sync::atomic::AtomicBool::new(false),closing:std::sync::atomic::AtomicUsize::new(0),close_failed:std::sync::atomic::AtomicBool::new(false),main_name:format!("/proc/self/fd/{}/starlane-notes.db",authority.root().as_raw_fd()),authority,bindings:std::sync::Mutex::new(Default::default()),methods:std::sync::Mutex::new(Vec::new()),original:original as usize});
  let mut vfs=Box::new(unsafe{std::ptr::read(original)});vfs.pNext=std::ptr::null_mut();vfs.zName=name.as_ptr();
  vfs.xOpen=Some(historical_readonly_reader_open);vfs.xDelete=Some(historical_readonly_reader_delete);vfs.xFullPathname=Some(historical_readonly_reader_full_path);vfs.xAccess=Some(historical_readonly_reader_access);
  let key=&*vfs as *const _ as usize;
  historical_readonly_reader_states().lock().map_err(|_|fail("Reader registry poisoned"))?.insert(key,state.clone());
  if unsafe{libsqlite3_sys::sqlite3_vfs_register(&mut *vfs,0)}!=libsqlite3_sys::SQLITE_OK {historical_readonly_reader_states().lock().map_err(|_|fail("Reader registry poisoned"))?.remove(&key);return Err(fail("Cannot register readonly reader"));}
  state.native_fence()?;Ok(Self{vfs,name,state:Some(state),retired_methods:Vec::new()})
 }
}
impl HistoricalReadonlyReaderVfs {
 fn state(&self)->&Arc<HistoricalReadonlyReaderState> {self.state.as_ref().expect("Active readonly VFS state")}
 fn stop_admissions(&self)->io::Result<()> {historical_readonly_reader_stop_admissions(&self.state().retiring)}
 fn retire(mut self) {
  debug_assert!(self.state().retiring.load(std::sync::atomic::Ordering::Acquire));
  // Keep only inert native allocations until process exit. Actual callback
  // drain precedes this point; no data descriptor/actor/lease remains in the
  // retired allocation, and the active callback authority registry is removed.
  unsafe{libsqlite3_sys::sqlite3_vfs_unregister(&mut *self.vfs);}
  if let Ok(mut states)=historical_readonly_reader_states().lock(){states.remove(&(&*self.vfs as *const _ as usize));}
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
struct HistoricalReadonlyReaderCloseAck {state:Arc<HistoricalReadonlyReaderState>,ok:bool}
impl Drop for HistoricalReadonlyReaderCloseAck {
 fn drop(&mut self){if !self.ok{self.state.close_failed.store(true,std::sync::atomic::Ordering::Release);}self.state.closing.fetch_sub(1,std::sync::atomic::Ordering::AcqRel);}
}
impl Drop for HistoricalReadonlyReaderVfs {
 fn drop(&mut self){unsafe{libsqlite3_sys::sqlite3_vfs_unregister(&mut *self.vfs);}if let Ok(mut states)=historical_readonly_reader_states().lock(){states.remove(&(&*self.vfs as *const _ as usize));}}
}
unsafe extern "C" fn historical_readonly_reader_open(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,file:*mut libsqlite3_sys::sqlite3_file,flags:i32,out:*mut i32)->i32 {
 std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let result=(||->io::Result<i32>{
   let state=historical_readonly_reader_states().lock().map_err(|_|fail("Reader registry poisoned"))?.get(&(vfs as usize)).cloned().ok_or_else(||fail("Unknown readonly reader"))?;
   state.native_fence()?;
   if name.is_null()||file.is_null(){return Err(fail("Readonly temporary file refused"));}
   let raw=unsafe{std::ffi::CStr::from_ptr(name)};
   if raw.to_str().ok()!=Some(state.main_name.as_str())||flags&libsqlite3_sys::SQLITE_OPEN_MAIN_DB==0
    ||flags&libsqlite3_sys::SQLITE_OPEN_READONLY==0||flags&(libsqlite3_sys::SQLITE_OPEN_READWRITE|libsqlite3_sys::SQLITE_OPEN_CREATE|libsqlite3_sys::SQLITE_OPEN_DELETEONCLOSE)!=0{return Err(fail("Unowned readonly open refused"));}
   let _gate=historical_readonly_reader_admit_gate(&state.retiring,||Ok(state.bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?.len()))?;
   // The serving descriptor is owned before SQLite sees any pathname. Unix VFS
   // retains its native lock machinery; its inode FD cache is never data authority.
   let pin=open_at(state.authority.root(),"starlane-notes.db",libc::O_RDONLY)?;
   historical_readonly_reader_shared_lease(&pin)?;
   let fd=pin.as_raw_fd();
   // Unix opens with O_NOFOLLOW, so its lock pathname must end in the actual
   // regular leaf under the held root FD. Before/after full root+main stamps
   // refuse any terminal-path exchange; serving bytes still use only pin.
   let mut unix_name=state.main_name.as_bytes().to_vec();unix_name.extend_from_slice(&[0,0]);
   let original=state.original as *mut libsqlite3_sys::sqlite3_vfs;
   let mut binding=HistoricalReadonlyReaderBinding{fd,pin,methods:0,_unix_name:unix_name};state.io_fence(&binding)?;state.native_fence()?;
   let rc=unsafe{(*original).xOpen.ok_or_else(||fail("Unix readonly xOpen missing"))?(original,binding._unix_name.as_ptr().cast(),file,flags&!libsqlite3_sys::SQLITE_OPEN_URI,out)};
   if rc!=libsqlite3_sys::SQLITE_OK{return Ok(rc);}
   let admitted=(||->io::Result<()> {
    let original_methods=unsafe{(*file).pMethods};if original_methods.is_null(){return Err(fail("Readonly IO methods missing"));}
    binding.methods=original_methods as usize;state.io_fence(&binding)?;state.native_fence()?;
    let mut methods=Box::new(unsafe{std::ptr::read(original_methods)});
    methods.xRead=Some(historical_readonly_reader_read);methods.xFileSize=Some(historical_readonly_reader_size);methods.xFileControl=Some(historical_readonly_reader_control);methods.xSync=Some(historical_readonly_reader_no_sync);
    methods.xWrite=Some(historical_readonly_reader_write);methods.xTruncate=Some(historical_readonly_reader_truncate);methods.xClose=Some(historical_readonly_reader_close);
    methods.xFetch=Some(historical_readonly_reader_no_fetch);methods.xUnfetch=Some(historical_readonly_reader_no_unfetch);
    methods.xShmMap=Some(verification_pager_deny_shm_map);methods.xShmLock=Some(verification_pager_deny_shm_lock);methods.xShmUnmap=Some(verification_pager_no_shm_unmap);
    let pointer=&*methods as *const _;state.methods.lock().map_err(|_|fail("Reader methods poisoned"))?.push(methods);
    state.bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?.insert(file as usize,binding);unsafe{(*file).pMethods=pointer;}Ok(())
   })();
   if let Err(error)=admitted{tracing::error!(error=%error,"Actual readonly descriptor admission refused");let methods=unsafe{(*file).pMethods};if !methods.is_null(){if let Some(close)=unsafe{(*methods).xClose}{unsafe{close(file);}}}unsafe{(*file).pMethods=std::ptr::null();}return Err(fail("Readonly FD admission refused"));}Ok(rc)
  })();result.unwrap_or_else(|error|{tracing::error!(error=%error,"Original readonly VFS open refused");libsqlite3_sys::SQLITE_CANTOPEN})
 })).unwrap_or(libsqlite3_sys::SQLITE_CANTOPEN)
}
fn historical_readonly_reader_file_state(file:*mut libsqlite3_sys::sqlite3_file)->io::Result<Arc<HistoricalReadonlyReaderState>> {
 let states=historical_readonly_reader_states().lock().map_err(|_|fail("Reader registry poisoned"))?;
 for state in states.values(){if state.bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?.contains_key(&(file as usize)){return Ok(state.clone());}}Err(fail("Unknown actual readonly file"))
}
unsafe extern "C" fn historical_readonly_reader_read(file:*mut libsqlite3_sys::sqlite3_file,buffer:*mut libc::c_void,length:i32,offset:i64)->i32 {
 std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let result=(||->io::Result<i32>{let state=historical_readonly_reader_file_state(file)?;let bindings=state.bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?;
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
unsafe extern "C" fn historical_readonly_reader_control(file:*mut libsqlite3_sys::sqlite3_file,operation:i32,out:*mut libc::c_void)->i32 {
 if operation!=libsqlite3_sys::SQLITE_FCNTL_HAS_MOVED{return libsqlite3_sys::SQLITE_NOTFOUND;}
 std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let result=(||->io::Result<()> {if out.is_null(){return Err(fail("Readonly moved output absent"));}let state=historical_readonly_reader_file_state(file)?;let bindings=state.bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?;
   state.io_fence(bindings.get(&(file as usize)).ok_or_else(||fail("Readonly moved binding absent"))?)?;unsafe{*out.cast::<i32>()=0;}Ok(())})();if result.is_ok(){libsqlite3_sys::SQLITE_OK}else{libsqlite3_sys::SQLITE_IOERR}
 })).unwrap_or(libsqlite3_sys::SQLITE_IOERR)
}
unsafe extern "C" fn historical_readonly_reader_no_sync(_: *mut libsqlite3_sys::sqlite3_file,_:i32)->i32 {libsqlite3_sys::SQLITE_READONLY}
unsafe extern "C" fn historical_readonly_reader_size(file:*mut libsqlite3_sys::sqlite3_file,out:*mut i64)->i32 {
 std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let result=(||->io::Result<()> {if out.is_null(){return Err(fail("Readonly size output missing"));}let state=historical_readonly_reader_file_state(file)?;
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
unsafe extern "C" fn historical_readonly_reader_no_fetch(_: *mut libsqlite3_sys::sqlite3_file,_:i64,_:i32,out:*mut *mut libc::c_void)->i32 {if out.is_null(){return libsqlite3_sys::SQLITE_IOERR;}unsafe{*out=std::ptr::null_mut();}libsqlite3_sys::SQLITE_OK}
unsafe extern "C" fn historical_readonly_reader_no_unfetch(_: *mut libsqlite3_sys::sqlite3_file,_:i64,_:*mut libc::c_void)->i32 {libsqlite3_sys::SQLITE_OK}
unsafe extern "C" fn historical_readonly_reader_close(file:*mut libsqlite3_sys::sqlite3_file)->i32 {
 std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let result=(||->io::Result<i32>{let state=historical_readonly_reader_file_state(file)?;state.closing.fetch_add(1,std::sync::atomic::Ordering::AcqRel);let mut ack=HistoricalReadonlyReaderCloseAck{state:state.clone(),ok:false};let mut bindings=state.bindings.lock().map_err(|_|fail("Reader bindings poisoned"))?;
   let b=bindings.remove(&(file as usize)).ok_or_else(||fail("Actual readonly close binding missing"))?;
   // Refuse closing a foreign reused kernel FD; no cleanup authority is inferred.
   if verification_pager_fd_identity(b.fd)?!=state.authority.database(){return Err(fail("Actual readonly close FD exchanged"));}
   let methods=b.methods as *const libsqlite3_sys::sqlite3_io_methods;let close=unsafe{(*methods).xClose}.ok_or_else(||fail("Unix readonly xClose missing"))?;drop(bindings);
   let rc=unsafe{close(file)};drop(b);ack.ok=rc==libsqlite3_sys::SQLITE_OK;Ok(rc)})();result.unwrap_or(libsqlite3_sys::SQLITE_IOERR_CLOSE)
 })).unwrap_or(libsqlite3_sys::SQLITE_IOERR_CLOSE)
}
unsafe extern "C" fn historical_readonly_reader_write(_: *mut libsqlite3_sys::sqlite3_file,_:*const libc::c_void,_:i32,_:i64)->i32 {libsqlite3_sys::SQLITE_READONLY}
unsafe extern "C" fn historical_readonly_reader_truncate(_: *mut libsqlite3_sys::sqlite3_file,_:i64)->i32 {libsqlite3_sys::SQLITE_READONLY}
unsafe extern "C" fn historical_readonly_reader_delete(_: *mut libsqlite3_sys::sqlite3_vfs,_:*const libc::c_char,_:i32)->i32 {libsqlite3_sys::SQLITE_IOERR_DELETE}
unsafe extern "C" fn historical_readonly_reader_full_path(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,n:i32,out:*mut libc::c_char)->i32 {
 std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let result=(||->io::Result<()> {let state=historical_readonly_reader_states().lock().map_err(|_|fail("Reader registry poisoned"))?.get(&(vfs as usize)).cloned().ok_or_else(||fail("Unknown readonly VFS"))?;state.name_fence()?;
   if name.is_null()||out.is_null(){return Err(fail("Readonly pathname absent"));}let raw=unsafe{std::ffi::CStr::from_ptr(name)};
   if raw.to_str().ok()!=Some(state.main_name.as_str())||n<=raw.to_bytes().len()as i32{return Err(fail("Unowned readonly pathname"));}
   unsafe{std::ptr::copy_nonoverlapping(raw.as_ptr(),out,raw.to_bytes_with_nul().len());}state.name_fence()?;Ok(())})();if result.is_ok(){libsqlite3_sys::SQLITE_OK}else{libsqlite3_sys::SQLITE_CANTOPEN}
 })).unwrap_or(libsqlite3_sys::SQLITE_CANTOPEN)
}
unsafe extern "C" fn historical_readonly_reader_access(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,flags:i32,out:*mut i32)->i32 {
 std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
  let result=(||->io::Result<i32>{let state=historical_readonly_reader_states().lock().map_err(|_|fail("Reader registry poisoned"))?.get(&(vfs as usize)).cloned().ok_or_else(||fail("Unknown readonly VFS"))?;state.name_fence()?;
   if name.is_null()||out.is_null(){return Err(fail("Readonly access pathname absent"));}let name_str=unsafe{std::ffi::CStr::from_ptr(name)}.to_str().map_err(|_|fail("Readonly access invalid"))?;
   if [format!("{}-journal",state.main_name),format!("{}-wal",state.main_name),format!("{}-shm",state.main_name)].contains(&name_str.to_owned()) {unsafe{*out=0;}state.name_fence()?;return Ok(libsqlite3_sys::SQLITE_OK);}
   if name_str!=state.main_name{return Err(fail("Unowned readonly access"));}
   let original=state.original as *mut libsqlite3_sys::sqlite3_vfs;let result=unsafe{(*original).xAccess.ok_or_else(||fail("Unix readonly xAccess missing"))?(original,name,flags,out)};state.name_fence()?;Ok(result)})();result.unwrap_or(libsqlite3_sys::SQLITE_IOERR_ACCESS)
 })).unwrap_or(libsqlite3_sys::SQLITE_IOERR_ACCESS)
}
struct HistoricalReadonlySealedReader {pool:sqlx::AnyPool,vfs:Option<HistoricalReadonlyReaderVfs>,source:Option<Arc<crate::storage::recovery_records::PoolSource>>}
impl HistoricalReadonlySealedReader {
 async fn enter(authority:Arc<HistoricalReadonlyAdmission>)->anyhow::Result<Self>{Self::enter_vfs(HistoricalReadonlyReaderVfs::from_authority(HistoricalReadonlyReaderAuthority::Original(authority))?).await}
 async fn enter_vfs(vfs:HistoricalReadonlyReaderVfs)->anyhow::Result<Self> {
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
impl Drop for HistoricalReadonlySealedReader {fn drop(&mut self){if let Some(vfs)=self.vfs.take(){std::mem::forget(vfs);}}}

