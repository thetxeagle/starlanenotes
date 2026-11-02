//! Operation-bound, enforced read-only ORIGINAL SQLite source, never a copied-source grant.
//!
//! Pinned implementation: libsqlite3-sys-0.30.1/sqlite3/sqlite3.c:
//! - sqlite3WalOpen (~66621) requests READWRITE|CREATE even for a readonly main.
//!   Sidecars use explicit pinned readonly IO, never Unix xOpen (which may
//!   fchown even readonly WAL/journal FDs as root). Main retains genuine Unix IO
//!   lock machinery and native-FD admission; no sidecar can obtain a writable FD.
//! - walIndexPageRealloc (~65687), walIndexReadHdr (~67554), and
//!   walBeginShmUnreliable (~67693) support permanent READONLY_CANTINIT/null SHM
//!   with SQLite's own private heap WAL index. No immutable/nolock URI is used.
//!
//! This is only pager enforcement/SQL lock exclusion. The supplied io_fence must
//! retain the operation's complete sticky history + inode read-lease custody.
//! It must be synchronous, non-reentrant, and must not execute SQLite/SQL. In
//! particular WAL without an existing SHM has no SHM byte-lock exclusion: any
//! attempted foreign writable open consumes custody and MUST refuse certification,
//! even if that foreign writer subsequently commits after lease release.
use libsqlite3_sys as sqlite;
use std::{collections::BTreeMap, ffi::{CStr, CString}, fs::File, io,
    os::{fd::{AsRawFd, FromRawFd}, unix::{fs::{FileExt, MetadataExt}}},
    path::PathBuf, sync::{Arc, Mutex, OnceLock, atomic::{AtomicBool, AtomicUsize, Ordering}}};

const MAIN: &str = "starlane-notes.db";
type Fence = Arc<dyn Fn() -> io::Result<()> + Send + Sync>;
fn failure(message: &str) -> io::Error { io::Error::other(message) }
#[derive(Clone, PartialEq, Eq)]
struct Stamp { dev:u64, ino:u64, mode:u32, uid:u32, gid:u32, links:u64, len:u64, mt:i64, mtn:i64, ct:i64, ctn:i64 }
fn stamp(file:&File)->io::Result<Stamp> { let m=file.metadata()?; Ok(Stamp{dev:m.dev(),ino:m.ino(),mode:m.mode(),uid:m.uid(),gid:m.gid(),links:m.nlink(),len:m.len(),mt:m.mtime(),mtn:m.mtime_nsec(),ct:m.ctime(),ctn:m.ctime_nsec()}) }
fn readonly(file:&File)->io::Result<()> { let flags=unsafe{libc::fcntl(file.as_raw_fd(),libc::F_GETFL)}; if flags<0{return Err(io::Error::last_os_error());} if flags&libc::O_ACCMODE!=libc::O_RDONLY{return Err(failure("Source VFS requires independently readonly descriptors"));} Ok(()) }
fn at(root:&File,name:&str)->io::Result<File> {
    let name=CString::new(name).map_err(|_|failure("Invalid source leaf"))?;
    let fd=unsafe{libc::openat(root.as_raw_fd(),name.as_ptr(),libc::O_RDONLY|libc::O_NONBLOCK|libc::O_NOFOLLOW|libc::O_CLOEXEC)};
    if fd<0{return Err(io::Error::last_os_error());} Ok(unsafe{File::from_raw_fd(fd)})
}
fn shared(file:&File,start:i64,len:i64)->io::Result<()> {
    let mut lock:libc::flock=unsafe{std::mem::zeroed()}; lock.l_type=libc::F_RDLCK as _;lock.l_whence=libc::SEEK_SET as _;lock.l_start=start;lock.l_len=len;
    if unsafe{libc::fcntl(file.as_raw_fd(),libc::F_OFD_SETLK,&lock)}<0{return Err(io::Error::last_os_error());} Ok(())
}
struct Held { file:File, stamp:Stamp, name:String }
impl Held { fn new(file:File,name:&str)->io::Result<Self>{readonly(&file)?;if !file.metadata()?.is_file(){return Err(failure("Nonregular original SQLite source"));}Ok(Self{stamp:stamp(&file)?,file,name:name.into()})} }
fn fd_stamp(fd:i32)->io::Result<Stamp> {
    let mut st=std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe{libc::fstat(fd,st.as_mut_ptr())}!=0{return Err(io::Error::last_os_error());}
    let m=unsafe{st.assume_init()};
    if m.st_mode&libc::S_IFMT!=libc::S_IFREG{return Err(failure("Native source FD is not regular"));}
    Ok(Stamp{dev:m.st_dev,ino:m.st_ino,mode:m.st_mode,uid:m.st_uid,gid:m.st_gid,links:m.st_nlink,len:u64::try_from(m.st_size).map_err(|_|failure("Negative source size"))?,mt:m.st_mtime,mtn:m.st_mtime_nsec,ct:m.st_ctime,ctn:m.st_ctime_nsec})
}
fn regular_fds()->io::Result<BTreeMap<i32,Stamp>> {
    let mut files=BTreeMap::new();
    for entry in std::fs::read_dir("/proc/self/fd")? {
        let entry=entry?;let Some(fd)=entry.file_name().to_str().and_then(|v|v.parse::<i32>().ok()) else {continue};
        if let Ok(metadata)=fd_stamp(fd){files.insert(fd,metadata);}
    }
    Ok(files)
}
enum Binding {
    NativeMain { held:Arc<Held>, original_methods:usize, native_fd:i32, native_pin:File },
    PinnedSidecar { held:Arc<Held>, pin:File },
    // A private zero-length adapter, not an original file or a captured inode.
    VirtualEmptyWal,
}
impl Binding {
    fn held(&self)->Option<&Arc<Held>> {match self {Self::NativeMain{held,..}|Self::PinnedSidecar{held,..}=>Some(held),Self::VirtualEmptyWal=>None}}
    fn fence(&self)->io::Result<()> {
        match self {
            Self::NativeMain{held,native_fd,native_pin,..}=>if fd_stamp(*native_fd)?!=held.stamp||stamp(native_pin)?!=held.stamp{return Err(failure("Original source native FD changed"));},
            Self::PinnedSidecar{held,pin}=>if stamp(&held.file)?!=held.stamp||stamp(pin)?!=held.stamp{return Err(failure("Original source sidecar pin changed"));},
            Self::VirtualEmptyWal=>(),
        }
        Ok(())
    }
}
struct State {
    root:File, root_identity:(u64,u64), absolute:PathBuf, main_name:String,
    main:Arc<Held>,wal_mode:bool,wal:Option<Arc<Held>>,shm:Option<Arc<Held>>,journal:Option<Arc<Held>>,io_fence:Fence,
    original:usize, registration:AtomicUsize, retiring:AtomicBool, closing:AtomicUsize,
    close_failed:AtomicBool, admission:Mutex<()>,bindings:Mutex<BTreeMap<usize,Binding>>,
}
fn registry()->&'static Mutex<BTreeMap<usize,Arc<State>>> {static REGISTRY:OnceLock<Mutex<BTreeMap<usize,Arc<State>>>>=OnceLock::new();REGISTRY.get_or_init(||Mutex::new(BTreeMap::new()))}
impl State {
    fn fence(&self)->io::Result<()> {
        (self.io_fence)()?;
        let root=self.root.metadata()?;
        let named_root=std::fs::symlink_metadata(&self.absolute)?;
        if !named_root.is_dir()||(named_root.dev(),named_root.ino())!=self.root_identity{return Err(failure("Original source absolute root differs"));}
        if (root.dev(),root.ino())!=self.root_identity{return Err(failure("Original source directory exchanged"));}
        for h in std::iter::once(&self.main).chain(self.wal.iter()).chain(self.shm.iter()).chain(self.journal.iter()) {
            if stamp(&h.file)?!=h.stamp||stamp(&at(&self.root,&h.name)?)?!=h.stamp{return Err(failure("Original SQLite source descriptor/path changed"));}
        }
        for (name,present) in [(format!("{MAIN}-wal"),self.wal.is_some()),(format!("{MAIN}-shm"),self.shm.is_some()),(format!("{MAIN}-journal"),self.journal.is_some())] {
            match at(&self.root,&name) {Ok(_) if !present=>return Err(failure("Original SQLite source sidecar appeared")),Err(e) if present||e.kind()!=io::ErrorKind::NotFound=>return Err(e),_=>()}
        }
        (self.io_fence)()
    }
    fn cleanup(&self) {
        if !self.retiring.load(Ordering::Acquire)||self.closing.load(Ordering::Acquire)!=0||self.close_failed.load(Ordering::Acquire){return;}
        if self.bindings.lock().map(|b|b.is_empty()).unwrap_or(false) {
            if let Ok(mut all)=registry().lock(){all.remove(&self.registration.load(Ordering::Acquire));}
        }
    }
}
/// Keep this owner alive through pool creation, original SQL/continuation and
/// pool.close().await + native_close_ack().await. Cancellation Drop stops future
/// admissions; live native bindings keep custody until their actual xClose.
/// Only inert VFS/name/method allocations remain after retirement, never FDs.
pub(crate) struct OriginalSqliteSourceVfs { vfs:Option<Box<sqlite::sqlite3_vfs>>,name:Option<CString>,state:Arc<State> }
impl OriginalSqliteSourceVfs {
    pub(crate) fn new(root:File,absolute:PathBuf,main:File,wal:Option<File>,shm:Option<File>,journal:Option<File>,io_fence:Fence)->io::Result<Self> {
        readonly(&root)?;if !root.metadata()?.is_dir()||!absolute.is_absolute(){return Err(failure("Original source root required"));}
        (io_fence)()?;
        let md=root.metadata()?;let main=Arc::new(Held::new(main,MAIN)?);
        let mut header=[0u8;20];main.file.read_exact_at(&mut header,0)?;
        let wal_mode=header[19]==2;
        (io_fence)()?;
        let wal=wal.map(|f|Held::new(f,&format!("{MAIN}-wal")).map(Arc::new)).transpose()?;
        let shm=shm.map(|f|Held::new(f,&format!("{MAIN}-shm")).map(Arc::new)).transpose()?;
        let journal=journal.map(|f|Held::new(f,&format!("{MAIN}-journal")).map(Arc::new)).transpose()?;
        // These OFD ranges survive unrelated same-inode inspection FD closes.
        // Do not lock RESERVED_BYTE (+1): that would make xCheckReservedLock
        // mistake this reader for a writer and conceal a hot rollback journal.
        shared(&main.file,0x40000002,510)?;
        // SQLite Unix SHM base=((22+SQLITE_SHM_NLOCK)*4)=120; WAL_WRITE=0,
        // WAL_CKPT=1, WAL_RECOVER=2. Read locks do not write the original SHM.
        if let Some(shm)=&shm {shared(&shm.file,120,3)?;shared(&shm.file,128,1)?;}
        let original=unsafe{sqlite::sqlite3_vfs_find(c"unix".as_ptr())};
        if original.is_null()||unsafe{(*original).iVersion}!=3{return Err(failure("Pinned Unix SQLite VFS unavailable"));}
        let main_name=absolute.join(MAIN).to_str().ok_or_else(||failure("NonUTF8 source root"))?.to_owned();
        let name=CString::new(format!("starlane-original-source-{}",uuid::Uuid::new_v4())).map_err(|_|failure("Invalid VFS name"))?;
        let state=Arc::new(State{root,root_identity:(md.dev(),md.ino()),absolute,main_name,main,wal_mode,wal,shm,journal,io_fence,original:original as usize,registration:AtomicUsize::new(0),retiring:AtomicBool::new(false),closing:AtomicUsize::new(0),close_failed:AtomicBool::new(false),admission:Mutex::new(()),bindings:Mutex::new(BTreeMap::new())});
        state.fence()?;
        let mut vfs=Box::new(unsafe{std::ptr::read(original)});vfs.pNext=std::ptr::null_mut();vfs.zName=name.as_ptr();vfs.xOpen=Some(open);vfs.xDelete=Some(delete);vfs.xAccess=Some(access);vfs.xFullPathname=Some(full_path);
        let key=&*vfs as *const _ as usize;state.registration.store(key,Ordering::Release);
        registry().lock().map_err(|_|failure("Source VFS registry poisoned"))?.insert(key,state.clone());
        if unsafe{sqlite::sqlite3_vfs_register(&mut *vfs,0)}!=sqlite::SQLITE_OK {registry().lock().map_err(|_|failure("Source VFS registry poisoned"))?.remove(&key);return Err(failure("Original source VFS registration refused"));}
        let owner=Self{vfs:Some(vfs),name:Some(name),state};owner.fence()?;Ok(owner)
    }
    pub(crate) fn url(&self)->io::Result<String> {
        self.fence()?;
        let path=self.state.absolute.join(MAIN);let path=path.to_str().ok_or_else(||failure("NonUTF8 source root"))?;
        let path=path.split('/').map(|part|urlencoding::encode(part).into_owned()).collect::<Vec<_>>().join("/");
        Ok(format!("sqlite://{path}?mode=ro&vfs={}",self.name.as_ref().ok_or_else(||failure("Retired source VFS"))?.to_str().map_err(|_|failure("Invalid VFS name"))?))
    }
    pub(crate) fn fence(&self)->io::Result<()> { self.state.fence() }
    /// True means independent SQL WAL writer/checkpoint/recovery byte locks exist.
    /// False with WAL present means sticky custody refusal, NOT SQL writer denial.
    pub(crate) fn has_wal_shm_exclusion(&self)->bool {!self.state.wal_mode||self.state.shm.is_some()}
    pub(crate) fn stop_admissions(&self)->io::Result<()> {let _gate=self.state.admission.lock().map_err(|_|failure("Source VFS admission poisoned"))?;self.state.retiring.store(true,Ordering::Release);Ok(())}
    pub(crate) async fn native_close_ack(&self)->io::Result<()> {
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
        loop {
            if self.state.close_failed.load(Ordering::Acquire){return Err(failure("Original source native close failed"));}
            if self.state.bindings.lock().map_err(|_|failure("Source VFS bindings poisoned"))?.is_empty()&&self.state.closing.load(Ordering::Acquire)==0{return Ok(());}
            if std::time::Instant::now()>=deadline{return Err(failure("Original source native close remains unresolved"));}
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }
    pub(crate) fn retire(self)->io::Result<()> {
        self.stop_admissions()?;
        if !self.state.bindings.lock().map_err(|_|failure("Source VFS bindings poisoned"))?.is_empty()||self.state.closing.load(Ordering::Acquire)!=0||self.state.close_failed.load(Ordering::Acquire){return Err(failure("Original source VFS retirement needs native close acknowledgement"));}
        Ok(()) // Drop unregisters and releases descriptors only after native drain.
    }
}
impl Drop for OriginalSqliteSourceVfs {
    fn drop(&mut self) {
        // Gate is shared with xOpen, preventing insertion after retirement.
        if let Ok(_gate)=self.state.admission.lock(){self.state.retiring.store(true,Ordering::Release);}
        if let Some(mut vfs)=self.vfs.take(){unsafe{sqlite::sqlite3_vfs_unregister(&mut *vfs);}let _=Box::leak(vfs);}
        if let Some(name)=self.name.take(){let _=name.into_raw();}
        self.state.cleanup();
    }
}
fn vfs_state(vfs:*mut sqlite::sqlite3_vfs)->io::Result<Arc<State>> {registry().lock().map_err(|_|failure("Source VFS registry poisoned"))?.get(&(vfs as usize)).cloned().ok_or_else(||failure("Unknown original source VFS"))}
fn file_state(file:*mut sqlite::sqlite3_file)->io::Result<Arc<State>> {
    let all:Vec<_>=registry().lock().map_err(|_|failure("Source VFS registry poisoned"))?.values().cloned().collect();
    for state in all {if state.bindings.lock().map_err(|_|failure("Source VFS bindings poisoned"))?.contains_key(&(file as usize)){return Ok(state);}}
    Err(failure("Unknown original source SQLite file"))
}
fn guarded(code:i32,body:impl FnOnce()->io::Result<i32>)->i32 {
 match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
  Ok(Ok(result))=>result,
  Ok(Err(error))=>{#[cfg(test)] eprintln!("ORIGINAL_SOURCE_VFS_REFUSAL code={code}: {error}");let _=error;code},
  Err(_)=>code,
 }
}
unsafe extern "C" fn open(vfs:*mut sqlite::sqlite3_vfs,name:*const libc::c_char,file:*mut sqlite::sqlite3_file,flags:i32,out:*mut i32)->i32 {
    guarded(sqlite::SQLITE_CANTOPEN,||{
        if name.is_null()||file.is_null(){return Err(failure("Original source temp open refused"));}
        unsafe{(*file).pMethods=std::ptr::null();}
        let state=vfs_state(vfs)?;let _gate=state.admission.lock().map_err(|_|failure("Source VFS admission poisoned"))?;
        if state.retiring.load(Ordering::Acquire)||state.bindings.lock().map_err(|_|failure("Source VFS bindings poisoned"))?.len()>=4{return Err(failure("Original source admission closed"));}
        state.fence()?;let path=unsafe{CStr::from_ptr(name)}.to_str().map_err(|_|failure("NonUTF8 source filename"))?;
        let held=if path==state.main_name&&flags&sqlite::SQLITE_OPEN_MAIN_DB!=0&&flags&sqlite::SQLITE_OPEN_READONLY!=0&&flags&(sqlite::SQLITE_OPEN_READWRITE|sqlite::SQLITE_OPEN_CREATE|sqlite::SQLITE_OPEN_DELETEONCLOSE)==0 {Some(state.main.clone())}
        else if path==format!("{}-wal",state.main_name)&&flags&sqlite::SQLITE_OPEN_WAL!=0&&flags&sqlite::SQLITE_OPEN_DELETEONCLOSE==0 {
            if state.wal.is_none()&&!state.wal_mode{return Err(failure("Absent WAL requested for a non-WAL original main"));}
            state.wal.clone()
        }
        else if path==format!("{}-journal",state.main_name)&&flags&sqlite::SQLITE_OPEN_MAIN_JOURNAL!=0&&flags&sqlite::SQLITE_OPEN_DELETEONCLOSE==0 {Some(state.journal.clone().ok_or_else(||failure("Unowned original rollback journal"))?)}
        else{return Err(failure("Unowned original SQLite open"));};
        let forced=(flags&!(sqlite::SQLITE_OPEN_READWRITE|sqlite::SQLITE_OPEN_CREATE|sqlite::SQLITE_OPEN_URI|sqlite::SQLITE_OPEN_DELETEONCLOSE))|sqlite::SQLITE_OPEN_READONLY;
        if held.as_ref().map(|held|held.name.as_str())!=Some(MAIN) {
            // Unix sidecar xOpen can fchown even a readonly FD when euid==0.
            // Never delegate sidecars. An absent WAL-mode log is an explicitly
            // private empty adapter, while original absence stays fenced.
            let binding=if let Some(held)=held {
                readonly(&held.file)?;
                let pin=held.file.try_clone()?;
                let binding=Binding::PinnedSidecar{held,pin};binding.fence()?;binding
            }else{Binding::VirtualEmptyWal};
            state.fence()?;
            let methods=Box::new(sqlite::sqlite3_io_methods {
                iVersion:3,
                xClose:Some(close),xRead:Some(read),xWrite:Some(write),
                xTruncate:Some(truncate),xSync:Some(sync),xFileSize:Some(size),
                xLock:Some(lock),xUnlock:Some(unlock),xCheckReservedLock:Some(reserved),
                xFileControl:Some(control),xSectorSize:Some(sector_size),
                xDeviceCharacteristics:Some(device_characteristics),
                xShmMap:Some(shm_map),xShmLock:Some(shm_lock),
                xShmBarrier:Some(shm_barrier),xShmUnmap:Some(shm_unmap),
                xFetch:Some(no_fetch),xUnfetch:Some(no_unfetch),
            });
            state.bindings.lock().map_err(|_|failure("Source VFS bindings poisoned"))?.insert(file as usize,binding);
            unsafe{(*file).pMethods=Box::leak(methods);if !out.is_null(){*out=forced;}}
            return Ok(sqlite::SQLITE_OK);
        }
        let held=held.ok_or_else(||failure("Original main pin absent"))?;
        let original=state.original as *mut sqlite::sqlite3_vfs;
        let before=regular_fds()?;
        // The original MAIN filename object stays intact for Unix's native
        // descriptor and lock machinery. Sidecars never enter this branch.
        let mut actual=0;let rc=unsafe{(*original).xOpen.ok_or_else(||failure("Unix source open absent"))?(original,name,file,forced,&mut actual)};
        if rc!=sqlite::SQLITE_OK{return Ok(rc);}
        let original_methods=unsafe{(*file).pMethods};
        let admitted=(||->io::Result<()> {
            if original_methods.is_null(){state.close_failed.store(true,Ordering::Release);return Err(failure("Unix original source methods absent"));}
            if actual&sqlite::SQLITE_OPEN_READONLY==0||actual&(sqlite::SQLITE_OPEN_READWRITE|sqlite::SQLITE_OPEN_CREATE)!=0{return Err(failure("Unix original source did not open readonly"));}
            let after=regular_fds()?;
            let changed:Vec<_>=after.iter().filter(|(fd,metadata)|before.get(fd).map(|prior|(prior.dev,prior.ino))!=Some((metadata.dev,metadata.ino))).collect();
            if changed.len()!=1{return Err(failure("Ambiguous original SQLite native FD admission"));}
            let (&native_fd,native_stamp)=changed[0];
            if *native_stamp!=held.stamp{return Err(failure("Unix opened a foreign original SQLite descriptor"));}
            let fd_flags=unsafe{libc::fcntl(native_fd,libc::F_GETFL)};
            if fd_flags<0||fd_flags&libc::O_ACCMODE!=libc::O_RDONLY{return Err(failure("Unix original SQLite descriptor is writable"));}
            let native_pin=File::open(format!("/proc/self/fd/{native_fd}"))?;
            if stamp(&native_pin)?!=held.stamp{return Err(failure("Original SQLite native FD changed before pin"));}
            state.fence()?;
            let mut methods=Box::new(unsafe{std::ptr::read(original_methods)});
            methods.xRead=Some(read);methods.xFileSize=Some(size);methods.xWrite=Some(write);methods.xTruncate=Some(truncate);methods.xSync=Some(sync);methods.xClose=Some(close);
            methods.xFileControl=Some(control);methods.xFetch=Some(no_fetch);methods.xUnfetch=Some(no_unfetch);
            methods.xShmMap=Some(shm_map);methods.xShmLock=Some(shm_lock);methods.xShmBarrier=Some(shm_barrier);methods.xShmUnmap=Some(shm_unmap);
            methods.xLock=Some(lock);methods.xUnlock=Some(unlock);methods.xCheckReservedLock=Some(reserved);
            state.bindings.lock().map_err(|_|failure("Source VFS bindings poisoned"))?.insert(file as usize,Binding::NativeMain{held,original_methods:original_methods as usize,native_fd,native_pin});
            unsafe{(*file).pMethods=Box::leak(methods);if !out.is_null(){*out=actual;}}Ok(())
        })();
        if let Err(error)=admitted {if !original_methods.is_null(){if let Some(close)=unsafe{(*original_methods).xClose}{if unsafe{close(file)}!=sqlite::SQLITE_OK{state.close_failed.store(true,Ordering::Release);}}}unsafe{(*file).pMethods=std::ptr::null();}return Err(error);}
        Ok(sqlite::SQLITE_OK)
    })
}
unsafe extern "C" fn read(file:*mut sqlite::sqlite3_file,buffer:*mut libc::c_void,length:i32,offset:i64)->i32 {
    guarded(sqlite::SQLITE_IOERR_READ,||{
        if buffer.is_null()||length<0||offset<0{return Err(failure("Invalid source read"));}let state=file_state(file)?;state.fence()?;
        let bindings=state.bindings.lock().map_err(|_|failure("Source VFS bindings poisoned"))?;let binding=bindings.get(&(file as usize)).ok_or_else(||failure("Source read binding missing"))?;
        binding.fence()?;
        let bytes=unsafe{std::slice::from_raw_parts_mut(buffer.cast::<u8>(),length as usize)};let mut n=0;
        if let Some(held)=binding.held() {
            while n<bytes.len(){match held.file.read_at(&mut bytes[n..],offset as u64+n as u64){Ok(0)=>break,Ok(count)=>n+=count,Err(e)if e.kind()==io::ErrorKind::Interrupted=>continue,Err(e)=>return Err(e)}}
        }
        drop(bindings);state.fence()?;if n<bytes.len(){bytes[n..].fill(0);Ok(sqlite::SQLITE_IOERR_SHORT_READ)}else{Ok(sqlite::SQLITE_OK)}
    })
}
unsafe extern "C" fn size(file:*mut sqlite::sqlite3_file,out:*mut i64)->i32 {
    guarded(sqlite::SQLITE_IOERR_FSTAT,||{
        if out.is_null(){return Err(failure("Source size output absent"));}let state=file_state(file)?;state.fence()?;
        let bindings=state.bindings.lock().map_err(|_|failure("Source VFS bindings poisoned"))?;let binding=bindings.get(&(file as usize)).ok_or_else(||failure("Source size binding missing"))?;binding.fence()?;
        let length=match binding.held(){Some(held)=>i64::try_from(held.file.metadata()?.len()).map_err(|_|failure("Source size overflow"))?,None=>0};
        drop(bindings);state.fence()?;unsafe{*out=length;}Ok(sqlite::SQLITE_OK)
    })
}
fn native(file:*mut sqlite::sqlite3_file,operation:impl FnOnce(*const sqlite::sqlite3_io_methods)->io::Result<i32>)->io::Result<i32> {
    let state=file_state(file)?;state.fence()?;let bindings=state.bindings.lock().map_err(|_|failure("Source VFS bindings poisoned"))?;
    let binding=bindings.get(&(file as usize)).ok_or_else(||failure("Source native binding missing"))?;binding.fence()?;
    let Binding::NativeMain{original_methods,..}=binding else{return Err(failure("Readonly sidecar has no Unix lock authority"));};
    let rc=operation(*original_methods as *const _)?;drop(bindings);state.fence()?;Ok(rc)
}
unsafe extern "C" fn lock(file:*mut sqlite::sqlite3_file,level:i32)->i32 {guarded(sqlite::SQLITE_IOERR_LOCK,||native(file,|methods|Ok(unsafe{(*methods).xLock.ok_or_else(||failure("Unix source lock absent"))?(file,level)})))}
// Unlock/close must release genuine SQLite locks even after custody is poisoned.
unsafe extern "C" fn unlock(file:*mut sqlite::sqlite3_file,level:i32)->i32 {
    guarded(sqlite::SQLITE_IOERR_UNLOCK,||{
        let state=file_state(file)?;let bindings=state.bindings.lock().map_err(|_|failure("Source VFS bindings poisoned"))?;
        let binding=bindings.get(&(file as usize)).ok_or_else(||failure("Source unlock binding missing"))?;
        if let Binding::NativeMain{original_methods,..}=binding {
            let methods=*original_methods as *const sqlite::sqlite3_io_methods;
            Ok(unsafe{(*methods).xUnlock.ok_or_else(||failure("Unix source unlock absent"))?(file,level)})
        }else{Ok(sqlite::SQLITE_OK)}
    })
}
unsafe extern "C" fn reserved(file:*mut sqlite::sqlite3_file,out:*mut i32)->i32 {guarded(sqlite::SQLITE_IOERR_CHECKRESERVEDLOCK,||{if out.is_null(){return Err(failure("Source reserved output absent"));}native(file,|methods|Ok(unsafe{(*methods).xCheckReservedLock.ok_or_else(||failure("Unix source reserved check absent"))?(file,out)}))})}
unsafe extern "C" fn control(file:*mut sqlite::sqlite3_file,op:i32,out:*mut libc::c_void)->i32 {
    if op!=sqlite::SQLITE_FCNTL_HAS_MOVED{return sqlite::SQLITE_NOTFOUND;}
    guarded(sqlite::SQLITE_IOERR,||{let state=file_state(file)?;state.fence()?;if out.is_null(){return Err(failure("Source moved output absent"));}unsafe{*out.cast::<i32>()=0;}Ok(sqlite::SQLITE_OK)})
}
unsafe extern "C" fn close(file:*mut sqlite::sqlite3_file)->i32 {
    guarded(sqlite::SQLITE_IOERR_CLOSE,||{
        let state=file_state(file)?;state.closing.fetch_add(1,Ordering::AcqRel);
        struct Ack(Arc<State>,bool);impl Drop for Ack{fn drop(&mut self){if !self.1{self.0.close_failed.store(true,Ordering::Release);}self.0.closing.fetch_sub(1,Ordering::AcqRel);self.0.cleanup();}}
        let mut ack=Ack(state.clone(),false);
        let binding=state.bindings.lock().map_err(|_|failure("Source VFS bindings poisoned"))?.remove(&(file as usize)).ok_or_else(||failure("Source close binding missing"))?;
        let rc=if let Binding::NativeMain{held,original_methods,native_fd,..}=&binding {
            let now=fd_stamp(*native_fd)?;
            if (now.dev,now.ino)!=(held.stamp.dev,held.stamp.ino){return Err(failure("Original source close FD exchanged"));}
            let methods=*original_methods as *const sqlite::sqlite3_io_methods;
            unsafe{(*methods).xClose.ok_or_else(||failure("Unix source close absent"))?(file)}
        }else{sqlite::SQLITE_OK};
        drop(binding);ack.1=rc==sqlite::SQLITE_OK;Ok(rc)
    })
}
unsafe extern "C" fn sector_size(_: *mut sqlite::sqlite3_file)->i32 {512}
unsafe extern "C" fn device_characteristics(_: *mut sqlite::sqlite3_file)->i32 {0}
unsafe extern "C" fn shm_map(file:*mut sqlite::sqlite3_file,_page:i32,_size:i32,_extend:i32,out:*mut *mut libc::c_void)->i32 {#[cfg(test)] eprintln!("ORIGINAL_SOURCE_SHM_MAP page={_page} size={_size} extend={_extend}");guarded(sqlite::SQLITE_IOERR_SHMMAP,||{if out.is_null(){return Err(failure("Source SHM output absent"));}unsafe{*out=std::ptr::null_mut();}file_state(file)?.fence()?;Ok(sqlite::SQLITE_READONLY_CANTINIT)})}
unsafe extern "C" fn shm_lock(file:*mut sqlite::sqlite3_file,offset:i32,count:i32,flags:i32)->i32 {
    #[cfg(test)] eprintln!("ORIGINAL_SOURCE_SHM_LOCK offset={offset} count={count} flags={flags}");
    guarded(sqlite::SQLITE_IOERR_SHMLOCK,||{
        if offset<0||count<=0||offset.checked_add(count).is_none_or(|end|end>8){return Err(failure("Invalid original source SHM lock span"));}
        let state=file_state(file)?;
        if flags==(sqlite::SQLITE_SHM_UNLOCK|sqlite::SQLITE_SHM_SHARED) {
            // Logical reader unlock never releases operation-owned kernel
            // guards, and remains possible after poisoning for clean teardown.
            return Ok(sqlite::SQLITE_OK);
        }
        if flags!=(sqlite::SQLITE_SHM_LOCK|sqlite::SQLITE_SHM_SHARED){return Ok(sqlite::SQLITE_READONLY);}
        state.fence()?;
        {
            let bindings=state.bindings.lock().map_err(|_|failure("Source VFS bindings poisoned"))?;
            if !matches!(bindings.get(&(file as usize)),Some(Binding::NativeMain{..})){return Err(failure("Original SHM read lock requires main binding"));}
        }
        // SQLite resets heap mode to normal before walBeginShmUnreliable asks
        // for READ_LOCK(0). Preserve real shared slots until State retirement;
        // do not accidentally unlock permanent WRITE/CKPT/RECOVER/DMS guards.
        if let Some(shm)=&state.shm {shared(&shm.file,120+i64::from(offset),i64::from(count))?;}
        // With absent SHM this is only a private heap-reader protocol step.
        // Complete original custody/absence fencing must refuse any competing
        // writable open; no absent-SHM SQL byte-lock denial is claimed.
        state.fence()?;Ok(sqlite::SQLITE_OK)
    })
}
unsafe extern "C" fn shm_barrier(_: *mut sqlite::sqlite3_file) {std::sync::atomic::fence(Ordering::SeqCst);}
unsafe extern "C" fn shm_unmap(_: *mut sqlite::sqlite3_file,_delete:i32)->i32 {sqlite::SQLITE_OK}
unsafe extern "C" fn no_fetch(_: *mut sqlite::sqlite3_file,_offset:i64,_size:i32,out:*mut *mut libc::c_void)->i32 {if out.is_null(){return sqlite::SQLITE_IOERR;}unsafe{*out=std::ptr::null_mut();}sqlite::SQLITE_OK}
unsafe extern "C" fn no_unfetch(_: *mut sqlite::sqlite3_file,_offset:i64,_pointer:*mut libc::c_void)->i32 {sqlite::SQLITE_OK}
unsafe extern "C" fn write(_: *mut sqlite::sqlite3_file,_bytes:*const libc::c_void,_length:i32,_offset:i64)->i32 {sqlite::SQLITE_READONLY}
unsafe extern "C" fn truncate(_: *mut sqlite::sqlite3_file,_size:i64)->i32 {sqlite::SQLITE_READONLY}
unsafe extern "C" fn sync(_: *mut sqlite::sqlite3_file,_flags:i32)->i32 {sqlite::SQLITE_READONLY}
unsafe extern "C" fn delete(_: *mut sqlite::sqlite3_vfs,_name:*const libc::c_char,_sync:i32)->i32 {sqlite::SQLITE_READONLY}
unsafe extern "C" fn full_path(vfs:*mut sqlite::sqlite3_vfs,name:*const libc::c_char,length:i32,out:*mut libc::c_char)->i32 {guarded(sqlite::SQLITE_CANTOPEN,||{if name.is_null()||out.is_null(){return Err(failure("Source path absent"));}let state=vfs_state(vfs)?;state.fence()?;let raw=unsafe{CStr::from_ptr(name)};if raw.to_bytes()!=state.main_name.as_bytes()||length<=raw.to_bytes().len() as i32{return Err(failure("Unowned source full path"));}unsafe{std::ptr::copy_nonoverlapping(raw.as_ptr(),out,raw.to_bytes_with_nul().len());}state.fence()?;Ok(sqlite::SQLITE_OK)})}
unsafe extern "C" fn access(vfs:*mut sqlite::sqlite3_vfs,name:*const libc::c_char,flags:i32,out:*mut i32)->i32 {guarded(sqlite::SQLITE_IOERR_ACCESS,||{if name.is_null()||out.is_null(){return Err(failure("Source access absent"));}let state=vfs_state(vfs)?;state.fence()?;let path=unsafe{CStr::from_ptr(name)}.to_str().map_err(|_|failure("NonUTF8 source access"))?;let present=if path==state.main_name{true}else if path==format!("{}-wal",state.main_name){state.wal.is_some()}else if path==format!("{}-shm",state.main_name){state.shm.is_some()}else if path==format!("{}-journal",state.main_name){state.journal.as_ref().is_some_and(|journal|flags!=sqlite::SQLITE_ACCESS_EXISTS||journal.stamp.len>0)}else{return Err(failure("Unowned source access"));};unsafe{*out=i32::from(present&&flags!=sqlite::SQLITE_ACCESS_READWRITE);}state.fence()?;Ok(sqlite::SQLITE_OK)})}
