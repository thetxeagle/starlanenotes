// Linux pre-pager main-file admission. Requires the already cached SQLite ffi crate.
// Unix xOpen returns before SQLite reads/replays a hot journal. Gate there, never
// after SQLx establishment. No field-layout guesses into unixFile are used.
struct VerificationVfsState {
    #[cfg(test)] open_fault:std::sync::Mutex<Option<PathBuf>>,
    #[cfg(test)] rejected_foreign_main:std::sync::atomic::AtomicBool,
    owner_exclusion:std::sync::Mutex<Option<File>>,
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
    active:Arc<Active>, inventory:SqliteCaptureInventory, root_stamp:std::sync::Mutex<PhysicalStamp>, parent_stamps:Vec<(File,PhysicalStamp)>,
    guarded_methods:std::sync::Mutex<Vec<Box<libsqlite3_sys::sqlite3_io_methods>>>,
    spool:std::sync::Arc<std::sync::Mutex<VerificationSqlSpool>>,
    writable: std::sync::atomic::AtomicBool,
}
fn verification_pager_vfs_states()->&'static std::sync::Mutex<std::collections::BTreeMap<usize,std::sync::Arc<VerificationVfsState>>> {
    static STATES:std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeMap<usize,std::sync::Arc<VerificationVfsState>>>>=std::sync::OnceLock::new();
    STATES.get_or_init(||std::sync::Mutex::new(std::collections::BTreeMap::new()))
}
fn verification_pager_fd_identity(fd:i32)->io::Result<Identity> {
    let mut stat=std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe{libc::fstat(fd,stat.as_mut_ptr())}!=0{return Err(io::Error::last_os_error());}
    let stat=unsafe{stat.assume_init()};
    if stat.st_mode&libc::S_IFMT!=libc::S_IFREG{return Err(fail("SQLite FD is not regular"));}
    Ok((stat.st_dev,stat.st_ino))
}
fn verification_pager_regular_fds()->io::Result<std::collections::BTreeMap<i32,Identity>> {
    let mut out=std::collections::BTreeMap::new();
    for e in std::fs::read_dir("/proc/self/fd")? {
        let e=e?;let Some(fd)=e.file_name().to_str().and_then(|s|s.parse::<i32>().ok()) else {continue};
        if let Ok(id)=verification_pager_fd_identity(fd){out.insert(fd,id);}
    }Ok(out)
}
impl VerificationVfsState {
    fn ancestry(&self)->io::Result<()> {
        self.active.check()?;
        self.spool.lock().map_err(|_|fail("Owned SQL spool poisoned"))?.fence()?;
        for (held,stamp) in &self.parent_stamps {if physical_stamp(held)?!=*stamp{return Err(fail("Activated pager ancestor changed"));}}
        self.inventory_fence()?;
        let fresh=Root::open(&self.absolute)?;
        if fresh.chain.len()!=self.ancestry.len(){return Err(fail("VFS ancestry changed"));}
        for (a,(held,id)) in fresh.chain.iter().zip(&self.ancestry) {
            if identity(a)?!=*id||identity(held)?!=*id{return Err(fail("VFS ancestor exchanged"));}
        }Ok(())
    }
    fn inventory_fence_inner(&self,allow_root_change:bool)->io::Result<()> {
        let current=verification_pager_inventory(&self.active.root)?;
        if !allow_root_change && current.root_stamp!=*self.root_stamp.lock().map_err(|_|fail("Pager root stamp poisoned"))?{return Err(fail("Activated pager original root changed"));}
        if current.directories.keys().ne(self.inventory.directories.keys()){return Err(fail("Activated pager directories changed"));}
        for (p,(held,stamp)) in &self.inventory.directories {if physical_stamp(held)?!=*stamp||current.directories[p].1!=*stamp{return Err(fail("Activated pager unrelated directory changed"));}}
        let expected:std::collections::BTreeSet<_>=self.inventory.files.keys().filter(|p|p.as_str()!="starlane-notes.db").cloned().collect();
        let observed:std::collections::BTreeSet<_>=current.files.keys().filter(|p|!matches!(p.as_str(),"starlane-notes.db"|"starlane-notes.db-journal")).cloned().collect();
        if observed!=expected{return Err(fail("Activated pager unrelated inventory changed"));}
        for p in &expected {let(held,stamp,sha)=&self.inventory.files[p];let(_,now,hash)=&current.files[p];if physical_stamp(held)?!=*stamp||now!=stamp||hash!=sha{return Err(fail("Activated pager unrelated original file changed"));}}
        let journal=self.journal.lock().map_err(|_|fail("Pager journal poisoned"))?;
        match (&*journal,current.files.get("starlane-notes.db-journal")) {
            (None,None)=>{},(Some(held),Some((fresh,_,_)))=>if identity(held)?!=identity(fresh)?{return Err(fail("Activated journal path exchanged"));},
            _=>return Err(fail("Activated pager unowned journal presence")),
        }
        Ok(())
    }
    fn inventory_fence(&self)->io::Result<()> {self.inventory_fence_inner(false)}
    fn advance_root(&self)->io::Result<()> {
        self.active.check()?;
        for (held,stamp) in &self.parent_stamps {if physical_stamp(held)?!=*stamp{return Err(fail("Activated pager ancestor changed during journal lifecycle"));}}
        self.inventory_fence_inner(true)?;
        let now=physical_stamp(&self.root)?;let mut original=self.root_stamp.lock().map_err(|_|fail("Pager root stamp poisoned"))?;
        if (now.device,now.inode,now.mode,now.links)!=(original.device,original.inode,original.mode,original.links){return Err(fail("Activated pager root authority changed"));}
        *original=now;Ok(())
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
        if verification_pager_fd_identity(*fd)?!=self.database||identity(pin)?!=self.database
            ||identity(&open_at(&self.root,"starlane-notes.db",libc::O_RDONLY)?)?!=self.database {
            return Err(fail("Actual SQLite main FD or path differs"));
        }
        let main=physical_stamp(pin)?;
        if (main.device,main.inode,main.links,main.mode)!=(self.initial_database_stamp.device,self.initial_database_stamp.inode,self.initial_database_stamp.links,self.initial_database_stamp.mode){return Err(fail("Activated main custody metadata changed"));}
        Ok(())
    }
}
struct VerificationVfs {
    vfs:Box<libsqlite3_sys::sqlite3_vfs>,
    _name:CString,
    state:Option<std::sync::Arc<VerificationVfsState>>,retired_methods:Vec<Box<libsqlite3_sys::sqlite3_io_methods>>,
}
impl VerificationVfs {
    fn new(selection:&OriginalTargetRecoverySelection<'_>)->io::Result<Self> {
        selection.fence().map_err(|e|fail(&e.to_string()))?;
        for name in ["starlane-notes.db-wal","starlane-notes.db-shm","starlane-notes.db-journal"] {if exists(selection.active.root.fd(),name)?{return Err(fail("Activated pager requires original clean DELETE database"));}}
        let mut header=[0u8;20];use std::os::unix::fs::FileExt;selection.database.read_exact_at(&mut header,0)?;
        if &header[..16]!=b"SQLite format 3\0"||header[18..20]!=[1,1]{return Err(fail("Activated pager preserves DELETE mode only"));}
        let spool=std::sync::Arc::new(std::sync::Mutex::new(VerificationSqlSpool::create(selection)?));
        let original=unsafe{libsqlite3_sys::sqlite3_vfs_find(c"unix".as_ptr())};
        if original.is_null()||unsafe{(*original).iVersion}!=3{return Err(fail("Unsupported SQLite Unix VFS"));}
        let name=CString::new(format!("starlane-activated-pager-{}",uuid::Uuid::new_v4())).unwrap();
        let root=selection.active.root.fd().try_clone()?;
        let main_name=format!("/proc/self/fd/{}/starlane-notes.db",root.as_raw_fd());
        let ancestry=selection.active.root.chain.iter().map(|f|Ok((f.try_clone()?,identity(f)?))).collect::<io::Result<Vec<_>>>()?;
        let parent_stamps=selection.active.root.chain.iter().take(selection.active.root.chain.len()-1).map(|f|Ok((f.try_clone()?,physical_stamp(f)?))).collect::<io::Result<Vec<_>>>()?;
        let state=std::sync::Arc::new(VerificationVfsState{
            #[cfg(test)] open_fault:std::sync::Mutex::new(None),
            #[cfg(test)] rejected_foreign_main:std::sync::atomic::AtomicBool::new(false),
            original:original as usize,root,absolute:selection.path().to_owned(),ancestry,
            database:identity(&selection.database)?,initial_database_stamp:selection.database_stamp.clone(),initial_database_bytes:planned_hash(&selection.database)?,main_name,
            owner_exclusion:std::sync::Mutex::new(None),bound:std::sync::Mutex::new(None),journal:std::sync::Mutex::new(None),transition_wal:std::sync::atomic::AtomicBool::new(false),wal:std::sync::Mutex::new(None),
            active:selection.active.clone(),inventory:verification_pager_inventory(&selection.active.root)?,root_stamp:std::sync::Mutex::new(selection.root_stamp.clone()),parent_stamps,
            guarded_methods:std::sync::Mutex::new(Vec::new()),spool,writable:std::sync::atomic::AtomicBool::new(false)});
        let mut vfs=Box::new(unsafe{std::ptr::read(original)});
        vfs.pNext=std::ptr::null_mut();vfs.zName=name.as_ptr();vfs.xOpen=Some(verification_pager_vfs_open);vfs.xDelete=Some(verification_pager_vfs_delete);vfs.xFullPathname=Some(verification_pager_vfs_full_path);vfs.xAccess=Some(verification_pager_vfs_access);
        let key=&*vfs as *const _ as usize;
        verification_pager_vfs_states().lock().map_err(|_|fail("VFS gate poisoned"))?.insert(key,state.clone());
        if unsafe{libsqlite3_sys::sqlite3_vfs_register(&mut *vfs,0)}!=libsqlite3_sys::SQLITE_OK {verification_pager_vfs_states().lock().map_err(|_|fail("VFS gate poisoned"))?.remove(&key);return Err(fail("Cannot register activated SQLite VFS"));}
        selection.fence().map_err(|e|fail(&e.to_string()))?;Ok(Self{vfs,_name:name,state:Some(state),retired_methods:Vec::new()})
    }
    fn state(&self)->&Arc<VerificationVfsState> {self.state.as_ref().expect("Active writer VFS state")}
    fn retire(mut self) {
        // Native method/name allocation is inert after pool and journal close.
        // The exact source/lease state remains only in the real typed seal,
        // not in this process-lifetime retired function storage.
        unsafe{libsqlite3_sys::sqlite3_vfs_unregister(&mut *self.vfs);}
        if let Ok(mut states)=verification_pager_vfs_states().lock(){states.remove(&(&*self.vfs as *const _ as usize));}
        let methods=std::mem::take(&mut *self.state().guarded_methods.lock().expect("Closed writer methods"));self.retired_methods=methods;
        drop(self.state.take());std::mem::forget(self);
    }
    fn name(&self)->&str {self._name.to_str().unwrap()}
    fn check(&self)->io::Result<()> {self.state().check()}
    fn permit_own_journal(&self)->io::Result<()> {
        self.check()?;self.state().initial_database()?;
        if exists(&self.state().root,"starlane-notes.db-journal")? {return Err(fail("Unowned journal exists"));}
        self.state().writable.store(true,std::sync::atomic::Ordering::SeqCst);Ok(())
    }
}
impl Drop for VerificationVfs {
    fn drop(&mut self) {
        unsafe{libsqlite3_sys::sqlite3_vfs_unregister(&mut *self.vfs);}
        if let Ok(mut states)=verification_pager_vfs_states().lock(){states.remove(&(&*self.vfs as *const _ as usize));}
    }
}
unsafe extern "C" fn verification_pager_vfs_open(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,
    file:*mut libsqlite3_sys::sqlite3_file,flags:i32,out:*mut i32)->i32 {
    // Never unwind through C, including allocation/poisoned-map failures.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(||verification_pager_vfs_open_inner(vfs,name,file,flags,out)))
        .unwrap_or(libsqlite3_sys::SQLITE_CANTOPEN)
}
fn verification_pager_vfs_open_inner(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,
    file:*mut libsqlite3_sys::sqlite3_file,mut flags:i32,out:*mut i32)->i32 {
    let Some(state)=verification_pager_vfs_states().lock().ok().and_then(|s|s.get(&(vfs as usize)).cloned()) else{return libsqlite3_sys::SQLITE_CANTOPEN};
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
            if wal{return Err(fail("Activated WAL creation refused"));}
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
        let before=verification_pager_regular_fds()?;
        #[cfg(test)] if main {verification_pager_foreign_before(&state)?;}

        let original=state.original as *mut libsqlite3_sys::sqlite3_vfs;
        let rc=unsafe{(*original).xOpen.ok_or_else(||fail("Missing Unix xOpen"))?(original,
            // Use original SQLite-owned double-NUL/URI filename, never reconstructed text.
            name.as_ptr() as *const libc::c_char,file,flags,out)};
        if rc!=libsqlite3_sys::SQLITE_OK{return Ok(rc);}
        let admitted=(||->io::Result<()> {
                #[cfg(test)] if main {verification_pager_foreign_after(&state)?;}
            let after=verification_pager_regular_fds()?;
            let changed:Vec<_>=after.iter().filter(|(fd,id)|before.get(fd)!=Some(id)).collect();
            if changed.len()!=1{return Err(fail("Ambiguous SQLite actual FD"));}
            let (fd,id)=changed[0];
            if main {
                if *id!=state.database||identity(&open_at(&state.root,"starlane-notes.db",libc::O_RDONLY)?)?!=state.database {
                    #[cfg(test)] state.rejected_foreign_main.store(true,std::sync::atomic::Ordering::SeqCst);
                    return Err(fail("SQLite opened foreign main FD"));
                }
                state.initial_database()?;
                let pin=File::open(format!("/proc/self/fd/{fd}"))?;
                *state.bound.lock().map_err(|_|fail("FD gate poisoned"))?=Some((*fd,pin));state.check()?;
                let original=unsafe{(*file).pMethods};if original.is_null(){return Err(fail("Missing main IO methods"));}
                let mut methods=Box::new(unsafe{std::ptr::read(original)});methods.xShmMap=Some(verification_pager_deny_shm_map);methods.xShmLock=Some(verification_pager_deny_shm_lock);methods.xShmUnmap=Some(verification_pager_no_shm_unmap);
                let pointer=&*methods as *const _;state.guarded_methods.lock().map_err(|_|fail("IO methods gate poisoned"))?.push(methods);unsafe{(*file).pMethods=pointer;}
            } else {
                let wal=flags&libsqlite3_sys::SQLITE_OPEN_WAL!=0;
                let leaf=if wal{"starlane-notes.db-wal"}else{"starlane-notes.db-journal"};
                let journal=open_at(&state.root,leaf,libc::O_RDONLY)?;let m=journal.metadata()?;
                if identity(&journal)?!=*id||m.nlink()!=1||m.mode()&0o777!=0o600||wal&&m.len()!=0{return Err(fail("Foreign SQLite sidecar FD"));}
                if wal {let stamp=physical_stamp(&journal)?;*state.wal.lock().map_err(|_|fail("WAL gate poisoned"))?=Some((journal,stamp));
                    let original=unsafe{(*file).pMethods};if original.is_null(){return Err(fail("Missing WAL IO methods"));}
                    let mut methods=Box::new(unsafe{std::ptr::read(original)});methods.xWrite=Some(verification_pager_deny_wal_write);methods.xTruncate=Some(verification_pager_deny_wal_truncate);
                    let pointer=&*methods as *const _;state.guarded_methods.lock().map_err(|_|fail("WAL IO gate poisoned"))?.push(methods);unsafe{(*file).pMethods=pointer;}

                }else{*state.journal.lock().map_err(|_|fail("FD gate poisoned"))?=Some(journal);state.advance_root()?;}
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
unsafe extern "C" fn verification_pager_vfs_delete(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,sync:i32)->i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
        let Some(state)=verification_pager_vfs_states().lock().ok().and_then(|s|s.get(&(vfs as usize)).cloned()) else{return libsqlite3_sys::SQLITE_IOERR_DELETE};
        let checked=(||->io::Result<()>{
            state.check()?;
            if name.is_null(){return Err(fail("Unowned SQLite deletion"));}
            let name=unsafe{std::ffi::CStr::from_ptr(name)}.to_str().map_err(|_|fail("Invalid sidecar deletion"))?;
            let wal=name==format!("{}-wal",state.main_name);
            if !wal&&name!=format!("{}-journal",state.main_name)||wal&&!state.transition_wal.load(std::sync::atomic::Ordering::SeqCst){return Err(fail("Unowned SQLite deletion"));}
            let mut spool=state.spool.lock().map_err(|_|fail("SQL spool poisoned"))?;
                if wal{let mut slot=state.wal.lock().map_err(|_|fail("WAL gate poisoned"))?;let(pin,stamp)=slot.as_ref().ok_or_else(||fail("No owned WAL"))?;
                if physical_stamp(pin)?!=*stamp||physical_stamp(&open_at(&state.root,"starlane-notes.db-wal",libc::O_RDONLY)?)?!=*stamp{return Err(fail("Transition WAL changed before retirement"));}
                spool.retain_empty_wal(&state.root,pin)?;*slot=None;
            }else{let mut slot=state.journal.lock().map_err(|_|fail("Journal gate poisoned"))?;let pin=slot.as_ref().ok_or_else(||fail("No owned journal"))?;spool.retain_journal(&state.root,pin)?;*slot=None;drop(slot);state.advance_root()?;}Ok(())
        })();
        if checked.is_ok(){libsqlite3_sys::SQLITE_OK}else{libsqlite3_sys::SQLITE_IOERR_DELETE}
    })).unwrap_or(libsqlite3_sys::SQLITE_IOERR_DELETE)
}
unsafe extern "C" fn verification_pager_vfs_full_path(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,n:i32,out:*mut libc::c_char)->i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
        let Some(state)=verification_pager_vfs_states().lock().ok().and_then(|s|s.get(&(vfs as usize)).cloned()) else{return libsqlite3_sys::SQLITE_CANTOPEN};
        if name.is_null()||out.is_null()||state.ancestry().is_err(){return libsqlite3_sys::SQLITE_CANTOPEN;}
        let raw=unsafe{std::ffi::CStr::from_ptr(name)};
        if raw.to_str().ok()!=Some(state.main_name.as_str())||n<=raw.to_bytes().len() as i32{return libsqlite3_sys::SQLITE_CANTOPEN;}
        // Keep descriptor-rooted filename. Do not resolve it to mutable ancestor paths.
        unsafe{std::ptr::copy_nonoverlapping(raw.as_ptr(),out,raw.to_bytes_with_nul().len());}
        libsqlite3_sys::SQLITE_OK
    })).unwrap_or(libsqlite3_sys::SQLITE_CANTOPEN)
}
unsafe extern "C" fn verification_pager_vfs_access(vfs:*mut libsqlite3_sys::sqlite3_vfs,name:*const libc::c_char,flags:i32,out:*mut i32)->i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(||{
        let Some(state)=verification_pager_vfs_states().lock().ok().and_then(|s|s.get(&(vfs as usize)).cloned()) else{return libsqlite3_sys::SQLITE_IOERR_ACCESS};
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
unsafe extern "C" fn verification_pager_deny_shm_map(_: *mut libsqlite3_sys::sqlite3_file,_:i32,_:i32,_:i32,_:*mut *mut libc::c_void)->i32 {libsqlite3_sys::SQLITE_IOERR_SHMMAP}
unsafe extern "C" fn verification_pager_deny_shm_lock(_: *mut libsqlite3_sys::sqlite3_file,_:i32,_:i32,_:i32)->i32 {libsqlite3_sys::SQLITE_IOERR_SHMLOCK}
unsafe extern "C" fn verification_pager_no_shm_unmap(_: *mut libsqlite3_sys::sqlite3_file,_:i32)->i32 {libsqlite3_sys::SQLITE_OK}

unsafe extern "C" fn verification_pager_deny_wal_write(_: *mut libsqlite3_sys::sqlite3_file,_:*const libc::c_void,_:i32,_:i64)->i32 {libsqlite3_sys::SQLITE_IOERR_WRITE}
unsafe extern "C" fn verification_pager_deny_wal_truncate(_: *mut libsqlite3_sys::sqlite3_file,_:i64)->i32 {libsqlite3_sys::SQLITE_IOERR_TRUNCATE}


// One fresh full-byte hash per inode per inventory walk. The cache never
// survives a walk. Every alias still has its full metadata stamp checked before
// and after selection, and the final census checks every held alias and nlink.
// A changed stamp refuses rather than treating old hash bytes as current.
fn verification_pager_walk_hash(
 file:&File,stamp:&PhysicalStamp,
 hashes:&mut std::collections::BTreeMap<Identity,(PhysicalStamp,String)>,
)->io::Result<String> {
 let id=identity(file)?;
 if let Some((first,sha))=hashes.get(&id) {
  if first!=stamp||physical_stamp(file)?!=*first{return Err(fail("Capture source alias changed during inventory"));}
  return Ok(sha.clone());
 }
 let sha=hash_file(file,stamp.length)?;
 if physical_stamp(file)?!=*stamp{return Err(fail("Capture source file raced"));}
 hashes.insert(id,(stamp.clone(),sha.clone()));Ok(sha)
}
fn verification_pager_inventory(root:&Root)->io::Result<SqliteCaptureInventory> {
 fn walk(fd:&File,prefix:&str,out:&mut SqliteCaptureInventory,hashes:&mut std::collections::BTreeMap<Identity,(PhysicalStamp,String)>)->io::Result<()> {
  for e in std::fs::read_dir(format!("/proc/self/fd/{}",fd.as_raw_fd()))? {
   let name=e?.file_name().into_string().map_err(|_|fail("NonUTF8 capture source"))?;
   if !valid_relative(&name)||name.contains(['/','\\']){return Err(fail("Unsafe capture source name"));}
   if out.files.len()+out.directories.len()>=100_000{return Err(fail("Activated pager inventory bound exceeded"));}
   let path=if prefix.is_empty(){name.clone()}else{format!("{prefix}/{name}")};
   let f=open_at(fd,&name,libc::O_RDONLY|libc::O_NONBLOCK)?;let md=f.metadata()?;let stamp=physical_stamp(&f)?;
   if md.uid()!=unsafe{libc::geteuid()}{return Err(fail("Capture source owner differs"));}
   if md.is_dir(){if md.mode()&0o022!=0{return Err(fail("Capture source directory is not private"));}walk(&f,&path,out,hashes)?;out.directories.insert(path,(f,stamp));}
   else if md.is_file(){if md.mode()&0o022!=0||md.nlink()==0{return Err(fail("Capture source file is not private/exclusive"));}let sha=verification_pager_walk_hash(&f,&stamp,hashes)?;out.files.insert(path,(f,stamp,sha));}
   else{return Err(fail("Capture source has nonregular evidence"));}
  }Ok(())
 }
 let mut out=SqliteCaptureInventory{root_stamp:physical_stamp(root.fd())?,directories:Default::default(),files:Default::default()};walk(root.fd(),"",&mut out,&mut Default::default())?;
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

#[cfg(test)] fn verification_pager_foreign_before(state:&VerificationVfsState)->io::Result<()> {
 let fault=state.open_fault.lock().map_err(|_|fail("Owned fault poisoned"))?;
 if let Some(foreign)=&*fault {let base=PathBuf::from(format!("/proc/self/fd/{}",state.root.as_raw_fd()));std::fs::rename(base.join("starlane-notes.db"),base.join("test-original-main.retained"))?;std::fs::hard_link(foreign,base.join("starlane-notes.db"))?;}Ok(())
}
#[cfg(test)] fn verification_pager_foreign_after(state:&VerificationVfsState)->io::Result<()> {
 let fault=state.open_fault.lock().map_err(|_|fail("Owned fault poisoned"))?;
 if fault.is_some() {let base=PathBuf::from(format!("/proc/self/fd/{}",state.root.as_raw_fd()));std::fs::rename(base.join("starlane-notes.db"),base.join("test-foreign-main.retained"))?;std::fs::rename(base.join("test-original-main.retained"),base.join("starlane-notes.db"))?;}Ok(())
}

#[cfg(test)] mod verification_pager_walk_hash_tests {
 use super::*;
 use std::io::Write;
 #[test] fn same_inode_alias_refuses_changed_bytes_and_fresh_walk_rehashes() {
  let dir=std::env::temp_dir().join(format!("pager-hash-{}",uuid::Uuid::new_v4()));
  std::fs::create_dir(&dir).unwrap();let original=dir.join("original");let alias=dir.join("alias");
  std::fs::write(&original,b"original payload").unwrap();std::fs::hard_link(&original,&alias).unwrap();
  let first=File::open(&original).unwrap();let other=File::open(&alias).unwrap();let stamp=physical_stamp(&first).unwrap();
  let mut hashes=Default::default();let sha=verification_pager_walk_hash(&first,&stamp,&mut hashes).unwrap();
  assert_eq!(verification_pager_walk_hash(&other,&physical_stamp(&other).unwrap(),&mut hashes).unwrap(),sha);
  assert_eq!(hashes.len(),1);
  // Write through a different real alias, preserving byte length and mtime.
  // ctime is part of the required stamp and cannot be restored by file set_times.
  std::thread::sleep(std::time::Duration::from_millis(2));
  let before=other.metadata().unwrap();let mut writer=std::fs::OpenOptions::new().write(true).open(&alias).unwrap();
  writer.write_all(b"replaced payload").unwrap();writer.sync_all().unwrap();
  writer.set_times(std::fs::FileTimes::new().set_modified(before.modified().unwrap())).unwrap();
  let changed=physical_stamp(&other).unwrap();assert_eq!(changed.length,stamp.length);assert_eq!(changed.mtime,stamp.mtime);assert_eq!(changed.mtime_ns,stamp.mtime_ns);
  assert!(verification_pager_walk_hash(&other,&changed,&mut hashes).is_err());
  assert_ne!(verification_pager_walk_hash(&other,&changed,&mut Default::default()).unwrap(),sha);
  std::fs::remove_dir_all(dir).unwrap();
 }
 #[test] fn equal_bytes_foreign_inode_is_a_separate_full_hash_entry() {
  let dir=std::env::temp_dir().join(format!("pager-hash-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&dir).unwrap();
  let a=dir.join("a");let b=dir.join("b");std::fs::write(&a,b"equal").unwrap();std::fs::write(&b,b"equal").unwrap();
  let a=File::open(a).unwrap();let b=File::open(b).unwrap();assert_ne!(identity(&a).unwrap(),identity(&b).unwrap());
  let mut hashes=Default::default();let ah=verification_pager_walk_hash(&a,&physical_stamp(&a).unwrap(),&mut hashes).unwrap();
  assert_eq!(ah,verification_pager_walk_hash(&b,&physical_stamp(&b).unwrap(),&mut hashes).unwrap());assert_eq!(hashes.len(),2);
  std::fs::remove_dir_all(dir).unwrap();
 }

 #[test] fn independent_named_and_held_inventory_passes_reject_alias_write_restore() {
  use std::os::unix::fs::PermissionsExt;
  let path=std::env::temp_dir().join(format!("inventory-alias-{}",uuid::Uuid::new_v4()));
  std::fs::create_dir(&path).unwrap();std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o700)).unwrap();
  let original=path.join("original");let alias=path.join("alias");std::fs::write(&original,b"original payload").unwrap();
  std::fs::set_permissions(&original,std::fs::Permissions::from_mode(0o600)).unwrap();std::fs::hard_link(&original,&alias).unwrap();
  let root=Root::open(&path).unwrap();let inventory=sqlite_capture_inventory(&root).unwrap();
  assert_eq!(inventory.files.len(),2);assert_eq!(inventory.files["original"].2,inventory.files["alias"].2);
  inventory.fence(&root).unwrap();verification_worker_inventory_fence(&inventory,&root).unwrap();
  let before=std::fs::metadata(&alias).unwrap();std::thread::sleep(std::time::Duration::from_millis(2));
  let mut writer=std::fs::OpenOptions::new().write(true).open(&alias).unwrap();writer.write_all(b"replaced payload").unwrap();writer.sync_all().unwrap();
  use std::io::{Seek,SeekFrom};writer.seek(SeekFrom::Start(0)).unwrap();writer.write_all(b"original payload").unwrap();writer.sync_all().unwrap();
  writer.set_times(std::fs::FileTimes::new().set_modified(before.modified().unwrap())).unwrap();
  assert_eq!(std::fs::read(&original).unwrap(),b"original payload");let old=&inventory.files["original"].1;let now=physical_stamp(&File::open(&alias).unwrap()).unwrap();
  assert_eq!(now.length,old.length);assert_eq!((now.mtime,now.mtime_ns),(old.mtime,old.mtime_ns));assert_ne!((now.ctime,now.ctime_ns),(old.ctime,old.ctime_ns));
  assert!(inventory.fence(&root).is_err());assert!(verification_worker_inventory_fence(&inventory,&root).is_err());
  std::fs::remove_dir_all(path).unwrap();
 }
}
