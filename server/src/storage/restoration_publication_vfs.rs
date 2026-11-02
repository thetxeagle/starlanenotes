// Linux pre-pager main-file admission. Requires the already cached SQLite ffi crate.
// Unix xOpen returns before SQLite reads/replays a hot journal. Gate there, never
// after SQLx establishment. No field-layout guesses into unixFile are used.
struct PublicationVfsState {
    original: usize,
    root: File,
    absolute: std::path::PathBuf,
    ancestry: Vec<(File,Identity)>,
    database: Identity,
    initial_database_stamp:PhysicalStamp,
    initial_database_bytes:crate::storage::backup_inventory::Bytes,
    main_name: String,
    bound: std::sync::Mutex<Option<(i32,File)>>,
    journal: std::sync::Mutex<Option<File>>,
    transition_wal:std::sync::atomic::AtomicBool,
    wal:std::sync::Mutex<Option<(File,PhysicalStamp)>>,
    marker:File,marker_stamp:PhysicalStamp,
    guarded_methods:std::sync::Mutex<Vec<Box<libsqlite3_sys::sqlite3_io_methods>>>,
    spool:std::sync::Arc<std::sync::Mutex<PublicationSqlSpool>>,
    writable: std::sync::atomic::AtomicBool,
}
fn publication_vfs_states()->&'static std::sync::Mutex<std::collections::BTreeMap<usize,std::sync::Arc<PublicationVfsState>>> {
    static STATES:std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeMap<usize,std::sync::Arc<PublicationVfsState>>>>=std::sync::OnceLock::new();
    STATES.get_or_init(||std::sync::Mutex::new(std::collections::BTreeMap::new()))
}
fn publication_fd_identity(fd:i32)->io::Result<Identity> {
    let mut stat=std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe{libc::fstat(fd,stat.as_mut_ptr())}!=0{return Err(io::Error::last_os_error());}
    let stat=unsafe{stat.assume_init()};
    if stat.st_mode&libc::S_IFMT!=libc::S_IFREG{return Err(fail("SQLite FD is not regular"));}
    Ok((stat.st_dev,stat.st_ino))
}
fn publication_regular_fds()->io::Result<std::collections::BTreeMap<i32,Identity>> {
    let mut out=std::collections::BTreeMap::new();
    for e in std::fs::read_dir("/proc/self/fd")? {
        let e=e?;let Some(fd)=e.file_name().to_str().and_then(|s|s.parse::<i32>().ok()) else {continue};
        if let Ok(id)=publication_fd_identity(fd){out.insert(fd,id);}
    }Ok(out)
}
impl PublicationVfsState {
    fn ancestry(&self)->io::Result<()> {
        if physical_stamp(&self.marker)?!=self.marker_stamp || physical_stamp(&open_at(&self.root,DESTINATION_MARKER,libc::O_RDONLY)?)?!=self.marker_stamp {return Err(fail("VFS incomplete marker changed"));}
        let fresh=Root::open(&self.absolute)?;
        if fresh.chain.len()!=self.ancestry.len(){return Err(fail("VFS ancestry changed"));}
        for (a,(held,id)) in fresh.chain.iter().zip(&self.ancestry) {
            if identity(a)?!=*id||identity(held)?!=*id{return Err(fail("VFS ancestor exchanged"));}
        }Ok(())
    }
    fn initial_database(&self)->io::Result<()> {
        let current=open_at(&self.root,"starlane-notes.db",libc::O_RDONLY)?;
        if physical_stamp(&current)?!=self.initial_database_stamp||planned_hash(&current)?!=self.initial_database_bytes {
            return Err(fail("SQLite input DB stamp/bytes changed before authorized writes"));
        }Ok(())
    }
    fn check(&self)->io::Result<()> {
        self.ancestry()?;
        let bound=self.bound.lock().map_err(|_|fail("SQLite FD gate poisoned"))?;
        let (fd,pin)=bound.as_ref().ok_or_else(||fail("SQLite FD was not admitted"))?;
        if publication_fd_identity(*fd)?!=self.database||identity(pin)?!=self.database
            ||identity(&open_at(&self.root,"starlane-notes.db",libc::O_RDONLY)?)?!=self.database {
            return Err(fail("Actual SQLite main FD or path differs"));
        }Ok(())
    }
}
struct PublicationVfs {
    vfs:Box<libsqlite3_sys::sqlite3_vfs>,
    _name:CString,
    state:std::sync::Arc<PublicationVfsState>,
}
impl PublicationVfs {
    fn new(d:&PlannedDestination,spool:std::sync::Arc<std::sync::Mutex<PublicationSqlSpool>>)->io::Result<Self> {
        Self::new_database_observation(d,spool,false)
    }
    fn read_after_sql(d:&PlannedDestination,spool:std::sync::Arc<std::sync::Mutex<PublicationSqlSpool>>)->io::Result<Self> {
        // This captures only the already held DB role after controlled close;
        // unrelated stamps remain unchanged and the new SQL audit is mandatory.
        publication_sql_window_fence(d,false)?;
        Self::new_database_observation(d,spool,true)
    }
    fn new_database_observation(d:&PlannedDestination,spool:std::sync::Arc<std::sync::Mutex<PublicationSqlSpool>>,after_sql:bool)->io::Result<Self> {
        let original=unsafe{libsqlite3_sys::sqlite3_vfs_find(c"unix".as_ptr())};
        if original.is_null()||unsafe{(*original).iVersion}!=3{return Err(fail("Unsupported SQLite Unix VFS"));}
        let name=CString::new(format!("starlane-publication-{}",uuid::Uuid::new_v4())).unwrap();
        let root=d.copied.root.fd().try_clone()?;
        let main_name=format!("/proc/self/fd/{}/starlane-notes.db",root.as_raw_fd());
        let ancestry=d.copied.root.chain.iter().map(|f|Ok((f.try_clone()?,identity(f)?))).collect::<io::Result<Vec<_>>>()?;
        let database_leaf=d.leaves.get("starlane-notes.db").ok_or_else(||fail("Missing DB role"))?;
        let (initial_database_stamp,initial_database_bytes)=if after_sql {
            (physical_stamp(&database_leaf.held)?,planned_hash(&database_leaf.held)?)
        } else {(database_leaf.stamp.clone(),database_leaf.bytes.clone())};
        if (initial_database_stamp.device,initial_database_stamp.inode)!=(database_leaf.stamp.device,database_leaf.stamp.inode) {
            return Err(fail("Post-SQL held DB identity differs"));
        }
        let state=std::sync::Arc::new(PublicationVfsState{original:original as usize,root,absolute:d.path().to_owned(),ancestry,
            database:identity(&d.leaves.get("starlane-notes.db").ok_or_else(||fail("Missing DB role"))?.held)?,
            initial_database_stamp,initial_database_bytes,main_name,
            bound:std::sync::Mutex::new(None),journal:std::sync::Mutex::new(None),transition_wal:std::sync::atomic::AtomicBool::new(false),wal:std::sync::Mutex::new(None),marker:d.copied.marker.try_clone()?,marker_stamp:d.marker_stamp.clone(),guarded_methods:std::sync::Mutex::new(Vec::new()),spool,writable:std::sync::atomic::AtomicBool::new(false)});
        let mut vfs=Box::new(unsafe{std::ptr::read(original)});
        vfs.pNext=std::ptr::null_mut();vfs.zName=name.as_ptr();vfs.xOpen=Some(publication_vfs_open);vfs.xDelete=Some(publication_vfs_delete);vfs.xFullPathname=Some(publication_vfs_full_path);vfs.xAccess=Some(publication_vfs_access);
        let key=&*vfs as *const _ as usize;
        publication_vfs_states().lock().map_err(|_|fail("VFS gate poisoned"))?.insert(key,state.clone());
        if unsafe{libsqlite3_sys::sqlite3_vfs_register(&mut *vfs,0)}!=libsqlite3_sys::SQLITE_OK {
            publication_vfs_states().lock().map_err(|_|fail("VFS gate poisoned"))?.remove(&key);
            return Err(fail("Cannot register private SQLite VFS"));
        }
        Ok(Self{vfs,_name:name,state})
    }
    fn name(&self)->&str {self._name.to_str().unwrap()}
    fn check(&self)->io::Result<()> {self.state.check()}
    fn permit_empty_wal_transition(&self)->io::Result<()> {
        self.check()?;self.state.initial_database()?;
        if unsafe{libsqlite3_sys::sqlite3_compileoption_used(c"DEFAULT_SYNCHRONOUS=2".as_ptr())}!=1
            ||unsafe{libsqlite3_sys::sqlite3_compileoption_used(c"DEFAULT_WAL_SYNCHRONOUS=2".as_ptr())}!=1 {return Err(fail("Unsupported SQLite transition durability default"));}
        for name in ["starlane-notes.db-wal","starlane-notes.db-shm","starlane-notes.db-journal"] {if exists(&self.state.root,name)?{return Err(fail("Unowned transition sidecar exists"));}}
        let mut header=[0u8;20];use std::os::unix::fs::FileExt;
        let held=open_at(&self.state.root,"starlane-notes.db",libc::O_RDONLY)?;held.read_exact_at(&mut header,0)?;
        if &header[..16]!=b"SQLite format 3\0" || !matches!(&header[18..20],[1,1]|[2,2]) {return Err(fail("Unsupported SQLite journal header"));}
        self.state.transition_wal.store(header[18]==2,std::sync::atomic::Ordering::SeqCst);Ok(())
    }
    fn end_empty_wal_transition(&self)->io::Result<()> {
        self.check()?;
        if self.state.wal.lock().map_err(|_|fail("WAL gate poisoned"))?.is_some()
            ||exists(&self.state.root,"starlane-notes.db-wal")?||exists(&self.state.root,"starlane-notes.db-shm")? {return Err(fail("Transition WAL not exclusively retired"));}
        let held=open_at(&self.state.root,"starlane-notes.db",libc::O_RDONLY)?;let mut mode=[0u8;2];use std::os::unix::fs::FileExt;held.read_exact_at(&mut mode,18)?;
        if mode!=[1,1]{return Err(fail("SQLite transition header differs"));}
        self.state.transition_wal.store(false,std::sync::atomic::Ordering::SeqCst);Ok(())
    }
    fn permit_own_journal(&self)->io::Result<()> {
        self.check()?;self.state.initial_database()?;
        if exists(&self.state.root,"starlane-notes.db-journal")? {return Err(fail("Unowned journal exists"));}
        self.state.writable.store(true,std::sync::atomic::Ordering::SeqCst);Ok(())
    }
}
impl Drop for PublicationVfs {
    fn drop(&mut self) {
        unsafe{libsqlite3_sys::sqlite3_vfs_unregister(&mut *self.vfs);}
        if let Ok(mut states)=publication_vfs_states().lock(){states.remove(&(&*self.vfs as *const _ as usize));}
    }
}
unsafe extern "C" fn publication_vfs_open(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,
    file:*mut libsqlite3_sys::sqlite3_file,flags:i32,out:*mut i32)->i32 {
    // Never unwind through C, including allocation/poisoned-map failures.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(||publication_vfs_open_inner(vfs,name,file,flags,out)))
        .unwrap_or(libsqlite3_sys::SQLITE_CANTOPEN)
}
fn publication_vfs_open_inner(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,
    file:*mut libsqlite3_sys::sqlite3_file,mut flags:i32,out:*mut i32)->i32 {
    let Some(state)=publication_vfs_states().lock().ok().and_then(|s|s.get(&(vfs as usize)).cloned()) else{return libsqlite3_sys::SQLITE_CANTOPEN};
    let result=(||->io::Result<i32>{
        if name.is_null(){return Err(fail("SQLite temporary files refused"));}
        let name=unsafe{std::ffi::CStr::from_ptr(name)}.to_str().map_err(|_|fail("Invalid SQLite VFS path"))?;
        let main=flags&libsqlite3_sys::SQLITE_OPEN_MAIN_DB!=0;
        if main {
            state.ancestry()?;state.initial_database()?;
            if name!=state.main_name||flags&libsqlite3_sys::SQLITE_OPEN_CREATE!=0
                ||state.bound.lock().map_err(|_|fail("FD gate poisoned"))?.is_some()
                ||identity(&open_at(&state.root,"starlane-notes.db",libc::O_RDONLY)?)?!=state.database {
                return Err(fail("Unowned SQLite main open"));
            }
        } else {
            state.check()?;
            let wal=flags&libsqlite3_sys::SQLITE_OPEN_WAL!=0;
            let leaf=if wal{"starlane-notes.db-wal"}else{"starlane-notes.db-journal"};
            if flags&libsqlite3_sys::SQLITE_OPEN_CREATE==0 || !state.writable.load(std::sync::atomic::Ordering::SeqCst)
                ||exists(&state.root,leaf)? ||exists(&state.root,"starlane-notes.db-shm")?
                ||if wal {flags&libsqlite3_sys::SQLITE_OPEN_MAIN_JOURNAL!=0 ||name!=format!("{}-wal",state.main_name)
                    ||!state.transition_wal.load(std::sync::atomic::Ordering::SeqCst)
                    ||state.wal.lock().map_err(|_|fail("WAL gate poisoned"))?.is_some()}
                  else {flags&libsqlite3_sys::SQLITE_OPEN_MAIN_JOURNAL==0||name!=format!("{}-journal",state.main_name)} {return Err(fail("Unowned SQLite sidecar open"));}
            if wal {state.initial_database()?;}
            flags|=libsqlite3_sys::SQLITE_OPEN_EXCLUSIVE;
        }
        let before=publication_regular_fds()?;
        #[cfg(test)] publication_open_fault_before(name,main,&state.absolute)?;
        #[cfg(test)] if flags&libsqlite3_sys::SQLITE_OPEN_WAL!=0 {publication_wal_fault(name,"open")?;}
        let original=state.original as *mut libsqlite3_sys::sqlite3_vfs;
        let rc=unsafe{(*original).xOpen.ok_or_else(||fail("Missing Unix xOpen"))?(original,
            // Use original SQLite-owned double-NUL/URI filename, never reconstructed text.
            name.as_ptr() as *const libc::c_char,file,flags,out)};
        if rc!=libsqlite3_sys::SQLITE_OK{return Ok(rc);}
        let admitted=(||->io::Result<()> {
            #[cfg(test)] publication_open_fault_after(name,main)?;
            let after=publication_regular_fds()?;
            let changed:Vec<_>=after.iter().filter(|(fd,id)|before.get(fd)!=Some(id)).collect();
            if changed.len()!=1{return Err(fail("Ambiguous SQLite actual FD"));}
            let (fd,id)=changed[0];
            if main {
                if *id!=state.database||identity(&open_at(&state.root,"starlane-notes.db",libc::O_RDONLY)?)?!=state.database {
                    return Err(fail("SQLite opened foreign main FD"));
                }
                state.initial_database()?;
                let pin=File::open(format!("/proc/self/fd/{fd}"))?;
                *state.bound.lock().map_err(|_|fail("FD gate poisoned"))?=Some((*fd,pin));state.check()?;
                let original=unsafe{(*file).pMethods};if original.is_null(){return Err(fail("Missing main IO methods"));}
                let mut methods=Box::new(unsafe{std::ptr::read(original)});methods.xShmMap=Some(publication_deny_shm_map);methods.xShmLock=Some(publication_deny_shm_lock);methods.xShmUnmap=Some(publication_no_shm_unmap);
                let pointer=&*methods as *const _;state.guarded_methods.lock().map_err(|_|fail("IO methods gate poisoned"))?.push(methods);unsafe{(*file).pMethods=pointer;}
            } else {
                let wal=flags&libsqlite3_sys::SQLITE_OPEN_WAL!=0;
                let leaf=if wal{"starlane-notes.db-wal"}else{"starlane-notes.db-journal"};
                let journal=open_at(&state.root,leaf,libc::O_RDONLY)?;let m=journal.metadata()?;
                if identity(&journal)?!=*id||m.nlink()!=1||m.mode()&0o777!=0o600||wal&&m.len()!=0{return Err(fail("Foreign SQLite sidecar FD"));}
                if wal {let stamp=physical_stamp(&journal)?;*state.wal.lock().map_err(|_|fail("WAL gate poisoned"))?=Some((journal,stamp));
                    let original=unsafe{(*file).pMethods};if original.is_null(){return Err(fail("Missing WAL IO methods"));}
                    let mut methods=Box::new(unsafe{std::ptr::read(original)});methods.xWrite=Some(publication_deny_wal_write);methods.xTruncate=Some(publication_deny_wal_truncate);
                    let pointer=&*methods as *const _;state.guarded_methods.lock().map_err(|_|fail("WAL IO gate poisoned"))?.push(methods);unsafe{(*file).pMethods=pointer;}
                    publication_checkpoint("opened-empty-wal");
                }else{*state.journal.lock().map_err(|_|fail("FD gate poisoned"))?=Some(journal);}
            }Ok(())
        })();
        if admitted.is_err() {
            unsafe {if !(*file).pMethods.is_null(){if let Some(close)=(*(*file).pMethods).xClose{close(file);}(*file).pMethods=std::ptr::null();}}
            return Err(fail("Pre-pager SQLite FD admission refused"));
        }
        Ok(rc)
    })();
    result.unwrap_or(libsqlite3_sys::SQLITE_CANTOPEN)
}
unsafe extern "C" fn publication_vfs_delete(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,sync:i32)->i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
        let Some(state)=publication_vfs_states().lock().ok().and_then(|s|s.get(&(vfs as usize)).cloned()) else{return libsqlite3_sys::SQLITE_IOERR_DELETE};
        let checked=(||->io::Result<()>{
            state.check()?;
            if name.is_null(){return Err(fail("Unowned SQLite deletion"));}
            let name=unsafe{std::ffi::CStr::from_ptr(name)}.to_str().map_err(|_|fail("Invalid sidecar deletion"))?;
            let wal=name==format!("{}-wal",state.main_name);
            if !wal&&name!=format!("{}-journal",state.main_name)||wal&&!state.transition_wal.load(std::sync::atomic::Ordering::SeqCst){return Err(fail("Unowned SQLite deletion"));}
            let mut spool=state.spool.lock().map_err(|_|fail("SQL spool poisoned"))?;
            #[cfg(test)] if wal {publication_wal_fault(name,"retire")?;}
            if wal{let mut slot=state.wal.lock().map_err(|_|fail("WAL gate poisoned"))?;let(pin,stamp)=slot.as_ref().ok_or_else(||fail("No owned WAL"))?;
                if physical_stamp(pin)?!=*stamp||physical_stamp(&open_at(&state.root,"starlane-notes.db-wal",libc::O_RDONLY)?)?!=*stamp{return Err(fail("Transition WAL changed before retirement"));}
                spool.retain_empty_wal(&state.root,pin)?;*slot=None;
            }else{let mut slot=state.journal.lock().map_err(|_|fail("Journal gate poisoned"))?;let pin=slot.as_ref().ok_or_else(||fail("No owned journal"))?;spool.retain_journal(&state.root,pin)?;*slot=None;}Ok(())
        })();
        if checked.is_ok(){libsqlite3_sys::SQLITE_OK}else{libsqlite3_sys::SQLITE_IOERR_DELETE}
    })).unwrap_or(libsqlite3_sys::SQLITE_IOERR_DELETE)
}
#[cfg(test)] fn publication_open_fault_before(name:&str,main:bool,absolute:&Path)->io::Result<()> {
    if !main{return Ok(());}
    if let Ok(foreign_parent)=std::env::var("STARLANE_PUBLICATION_FOREIGN_PARENT") {
        let retained=absolute.with_file_name("test-owned-parent.retained");
        std::fs::rename(absolute,&retained)?;
        std::os::unix::fs::symlink(foreign_parent,absolute)?;
        return Ok(());
    }
    let Ok(foreign)=std::env::var("STARLANE_PUBLICATION_FOREIGN_DB") else{return Ok(())};
    let root=Path::new(name).parent().ok_or_else(||fail("Fault DB parent missing"))?;
    let original=root.join("starlane-notes.db");
    let held=root.join("test-original-db.retained");
    std::fs::rename(&original,&held)?;
    if std::env::var("STARLANE_PUBLICATION_FOREIGN_SYMLINK").is_ok() {
        std::os::unix::fs::symlink(&foreign,&original)?;
    } else {
        // Real equal-byte foreign inode is hardlinked, never rewritten or forged.
        std::fs::hard_link(&foreign,&original)?;
    }
    Ok(())
}
#[cfg(test)] fn publication_open_fault_after(name:&str,main:bool)->io::Result<()> {
    if !main||std::env::var("STARLANE_PUBLICATION_FOREIGN_DB").is_err(){return Ok(());}
    let root=Path::new(name).parent().ok_or_else(||fail("Fault DB parent missing"))?;
    // Preserve the foreign entry rather than unlinking it. SQLite's actual main
    // FD still names the foreign inode after the visible original path returns.
    std::fs::rename(root.join("starlane-notes.db"),root.join("test-foreign-open-entry.retained"))?;
    std::fs::rename(root.join("test-original-db.retained"),root.join("starlane-notes.db"))?;
    Ok(())
}

unsafe extern "C" fn publication_vfs_full_path(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,n:i32,out:*mut libc::c_char)->i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
        let Some(state)=publication_vfs_states().lock().ok().and_then(|s|s.get(&(vfs as usize)).cloned()) else{return libsqlite3_sys::SQLITE_CANTOPEN};
        if name.is_null()||out.is_null()||state.ancestry().is_err(){return libsqlite3_sys::SQLITE_CANTOPEN;}
        let raw=unsafe{std::ffi::CStr::from_ptr(name)};
        if raw.to_str().ok()!=Some(state.main_name.as_str())||n<=raw.to_bytes().len() as i32{return libsqlite3_sys::SQLITE_CANTOPEN;}
        // Keep descriptor-rooted filename. Do not resolve it to mutable ancestor paths.
        unsafe{std::ptr::copy_nonoverlapping(raw.as_ptr(),out,raw.to_bytes_with_nul().len());}
        libsqlite3_sys::SQLITE_OK
    })).unwrap_or(libsqlite3_sys::SQLITE_CANTOPEN)
}
unsafe extern "C" fn publication_vfs_access(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,flags:i32,out:*mut i32)->i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
        let Some(state)=publication_vfs_states().lock().ok().and_then(|s|s.get(&(vfs as usize)).cloned()) else{return libsqlite3_sys::SQLITE_IOERR_ACCESS};
        if name.is_null()||out.is_null()||state.ancestry().is_err(){return libsqlite3_sys::SQLITE_IOERR_ACCESS;}
        let Ok(name_str)=(unsafe{std::ffi::CStr::from_ptr(name)}).to_str() else{return libsqlite3_sys::SQLITE_IOERR_ACCESS};
        let leaf=if name_str==state.main_name {"starlane-notes.db"}
            else if name_str==format!("{}-journal",state.main_name){"starlane-notes.db-journal"}
            else if name_str==format!("{}-wal",state.main_name){"starlane-notes.db-wal"}
            else if name_str==format!("{}-shm",state.main_name){"starlane-notes.db-shm"}
            else{return libsqlite3_sys::SQLITE_IOERR_ACCESS};
        let checked=(||->io::Result<()> {
            if !exists(&state.root,leaf)? {unsafe{*out=0;}return Ok(());}
            if leaf=="starlane-notes.db-shm"{return Err(fail("Unexpected SQLite shared memory"));}
            let held=open_at(&state.root,leaf,libc::O_RDONLY)?;
            let expected=if leaf=="starlane-notes.db"{state.database}else if leaf=="starlane-notes.db-wal"{
                if !state.transition_wal.load(std::sync::atomic::Ordering::SeqCst){return Err(fail("Unexpected SQLite WAL"));}
                let wal=state.wal.lock().map_err(|_|fail("WAL gate poisoned"))?;let(pin,stamp)=wal.as_ref().ok_or_else(||fail("Unowned WAL"))?;
                if physical_stamp(pin)?!=*stamp||physical_stamp(&open_at(&state.root,leaf,libc::O_RDONLY)?)?!=*stamp||stamp.length!=0{return Err(fail("Transition WAL bytes differ"));}identity(pin)?
            }else{identity(state.journal.lock().map_err(|_|fail("Journal gate poisoned"))?.as_ref().ok_or_else(||fail("Unowned hot journal"))?)?};
            if identity(&held)?!=expected{return Err(fail("SQLite access path exchanged"));}
            let original=state.original as *mut libsqlite3_sys::sqlite3_vfs;
            let rc=unsafe{(*original).xAccess.ok_or_else(||fail("Missing Unix xAccess"))?(original,name,flags,out)};
            if rc!=libsqlite3_sys::SQLITE_OK{return Err(fail("Unix access failed"));}Ok(())
        })();
        if checked.is_ok(){libsqlite3_sys::SQLITE_OK}else{libsqlite3_sys::SQLITE_IOERR_ACCESS}
    })).unwrap_or(libsqlite3_sys::SQLITE_IOERR_ACCESS)
}

// EXCLUSIVE transition uses heap WAL index. Any request for shared-memory state
// is refused before Unix can create/map/unlink a foreign shared-memory file.
unsafe extern "C" fn publication_deny_shm_map(_: *mut libsqlite3_sys::sqlite3_file,_:i32,_:i32,_:i32,_:*mut *mut libc::c_void)->i32 {libsqlite3_sys::SQLITE_IOERR_SHMMAP}
unsafe extern "C" fn publication_deny_shm_lock(_: *mut libsqlite3_sys::sqlite3_file,_:i32,_:i32,_:i32)->i32 {libsqlite3_sys::SQLITE_IOERR_SHMLOCK}
unsafe extern "C" fn publication_no_shm_unmap(_: *mut libsqlite3_sys::sqlite3_file,_:i32)->i32 {libsqlite3_sys::SQLITE_OK}

unsafe extern "C" fn publication_deny_wal_write(_: *mut libsqlite3_sys::sqlite3_file,_:*const libc::c_void,_:i32,_:i64)->i32 {libsqlite3_sys::SQLITE_IOERR_WRITE}
unsafe extern "C" fn publication_deny_wal_truncate(_: *mut libsqlite3_sys::sqlite3_file,_:i64)->i32 {libsqlite3_sys::SQLITE_IOERR_TRUNCATE}

#[cfg(test)] fn publication_wal_fault(name:&str,phase:&str)->io::Result<()> {
    if std::env::var("STARLANE_PUBLICATION_WAL_FAULT_PHASE").ok().as_deref()!=Some(phase){return Ok(());}
    let foreign=std::env::var("STARLANE_PUBLICATION_FOREIGN_WAL").map_err(|_|fail("WAL fault foreign missing"))?;
    let root=Path::new(name).parent().ok_or_else(||fail("WAL fault parent missing"))?;
    let leaf=root.join("starlane-notes.db-wal");
    if phase=="retire" {std::fs::rename(&leaf,root.join("test-owned-wal.retained"))?;}
    if std::env::var("STARLANE_PUBLICATION_FOREIGN_WAL_SYMLINK").is_ok(){std::os::unix::fs::symlink(foreign,leaf)?;}
    else {std::fs::hard_link(foreign,leaf)?;}
    Ok(())
}
