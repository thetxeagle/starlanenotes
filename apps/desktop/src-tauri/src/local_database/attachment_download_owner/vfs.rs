//! Owner-specific Linux SQLite VFS. Never expose a canonicalized live pathname.
use super::filesystem::{identity, Directory, Identity};
use libsqlite3_sys as sq;
use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::mem::{align_of, size_of};
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileExt;
use std::ptr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

struct State {
    registration: OnceLock<Weak<Registration>>,
    active_files: AtomicUsize,
    closed: tokio::sync::Notify,
    #[cfg(test)]
    before_main_close: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    #[cfg(test)]
    before_open: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    original: usize,
    dir: Directory,
    main: String,
    prefix: String,
    files: Mutex<BTreeMap<String, Option<Identity>>>,
    error: Mutex<Option<String>>,
    #[cfg(test)]
    sidecar_open_hook: Mutex<Option<Box<dyn Fn(&str, bool) + Send + Sync>>>,
    #[cfg(test)]
    before_retire: Mutex<Option<Box<dyn Fn(&str) + Send + Sync>>>,
}
impl State {
    fn error(&self, e: impl ToString) -> i32 {
        if let Ok(mut v) = self.error.lock() {
            *v = Some(e.to_string());
        }
        sq::SQLITE_IOERR
    }
    fn key(&self, name: *const libc::c_char) -> Result<String, String> {
        if name.is_null() {
            return Err("Native download SQLite temporary disk files are unsupported".into());
        }
        let text = unsafe { CStr::from_ptr(name) }
            .to_str()
            .map_err(|_| "Native SQLite filename encoding invalid")?;
        let key = text
            .strip_prefix(&self.prefix)
            .ok_or("Native SQLite filename escaped its physical owner")?;
        if ![
            self.main.clone(),
            format!("{}-journal", self.main),
            format!("{}-wal", self.main),
            format!("{}-shm", self.main),
        ]
        .iter()
        .any(|v| v == key)
        {
            return Err("Native SQLite filename escaped its physical owner".into());
        }
        Ok(key.into())
    }
    fn check(&self, key: &str) -> Result<(), String> {
        self.dir.fence().map_err(|e| e.to_string())?;
        let expected = self
            .files
            .lock()
            .map_err(|_| "Native VFS identity lock failed")?
            .get(key)
            .copied()
            .ok_or("Native SQLite child not admitted")?;
        if self.dir.file_identity(key).map_err(|e| e.to_string())? != expected {
            return Err("Original native database or sidecar identity changed".into());
        }
        if key != self.main {
            self.check(&self.main)?;
        }
        Ok(())
    }
    fn refresh_created(&self, key: &str, held: &File) -> Result<(), String> {
        self.dir.fence().map_err(|e| e.to_string())?;
        let id = self
            .dir
            .file_identity(key)
            .map_err(|e| e.to_string())?
            .ok_or("Native SQLite child creation failed")?;
        if identity(held).map_err(|e| e.to_string())? != id {
            return Err("Exclusive-created native SQLite child was exchanged".into());
        }
        let mut files = self
            .files
            .lock()
            .map_err(|_| "Native VFS identity lock failed")?;
        match files.get(key).copied().flatten() {
            Some(old) if old != id => return Err("Original native SQLite child changed".into()),
            _ => {
                files.insert(key.into(), Some(id));
            }
        }
        Ok(())
    }
    fn retire(&self, key: &str) -> Result<(), String> {
        self.check(key)?;
        let expected = self
            .files
            .lock()
            .map_err(|_| "Native VFS lock failed")?
            .get(key)
            .copied()
            .flatten();
        let Some(expected) = expected else {
            return Ok(());
        };
        let held = self
            .dir
            .open(key, libc::O_RDONLY)
            .map_err(|e| e.to_string())?;
        if identity(&held).map_err(|e| e.to_string())? != expected {
            return Err("Original sidecar changed before retirement".into());
        }
        let retired = self
            .dir
            .child("sqlite-retired", true)
            .map_err(|e| e.to_string())?;
        static NEXT_RETIRED: AtomicU64 = AtomicU64::new(1);
        let name = format!(
            "{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos(),
            NEXT_RETIRED.fetch_add(1, Ordering::Relaxed)
        );
        let old = CString::new(key).map_err(|e| e.to_string())?;
        let new = CString::new(name).unwrap();
        #[cfg(test)]
        if let Some(hook) = self.before_retire.lock().unwrap().as_ref() {
            hook(key);
        }
        // Linux has no inode-CAS unlink. Atomic exclusive rename retains even a
        // displaced foreign entry; compare ownership only after custody transfer.
        let rc = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                self.dir.held.as_raw_fd(),
                old.as_ptr(),
                retired.held.as_raw_fd(),
                new.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if rc != 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        self.dir.held.sync_all().map_err(|e| e.to_string())?;
        retired.held.sync_all().map_err(|e| e.to_string())?;
        let moved = retired
            .file_identity(new.to_str().unwrap())
            .map_err(|e| e.to_string())?;
        if moved != Some(expected) {
            return Err("Sidecar retirement raced; displaced evidence retained".into());
        }
        if self
            .dir
            .file_identity(key)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            return Err("New sidecar appeared during retirement; all evidence retained".into());
        }
        self.dir.fence().map_err(|e| e.to_string())?;
        retired.fence().map_err(|e| e.to_string())?;
        self.files
            .lock()
            .map_err(|_| "Native VFS lock failed")?
            .insert(key.into(), None);
        Ok(())
    }
}
struct Registration {
    table: Box<sq::sqlite3_vfs>,
    _state: Arc<State>,
    _name: CString,
}
unsafe impl Send for Registration {}
unsafe impl Sync for Registration {}
impl Drop for Registration {
    fn drop(&mut self) {
        unsafe {
            sq::sqlite3_vfs_unregister(&mut *self.table);
        }
    }
}
struct TablePointer(*mut sq::sqlite3_vfs);
impl std::ops::Deref for TablePointer {
    type Target = sq::sqlite3_vfs;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.0 }
    }
}
impl std::ops::DerefMut for TablePointer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.0 }
    }
}
pub(super) struct NativeVfs {
    table: TablePointer,
    registration: Arc<Registration>,
    state: Arc<State>,
    name: CString,
}
// The boxed registered table is immutable; callbacks share synchronized state.
// The actual owner must close every connection before dropping this registration.
unsafe impl Send for NativeVfs {}
unsafe impl Sync for NativeVfs {}
impl NativeVfs {
    pub fn new(dir: Directory, main: &str) -> Result<Self, String> {
        dir.fence().map_err(|e| e.to_string())?;
        if unsafe { sq::sqlite3_initialize() } != sq::SQLITE_OK {
            return Err("Native SQLite initialization failed".into());
        }
        let original = unsafe { sq::sqlite3_vfs_find(c"unix".as_ptr()) };
        if original.is_null() {
            return Err("Linux Unix SQLite VFS unavailable".into());
        }
        let mut files = BTreeMap::new();
        for key in [
            main.into(),
            format!("{main}-journal"),
            format!("{main}-wal"),
            format!("{main}-shm"),
        ] {
            files.insert(
                key.clone(),
                dir.file_identity(&key).map_err(|e| e.to_string())?,
            );
        }
        if files.get(main).copied().flatten().is_none() {
            return Err("Original native SQLite main file unavailable".into());
        }
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let prefix = format!("starlane-native-download-{}-{id}/", std::process::id());
        let name = CString::new(format!(
            "starlane-native-download-{}-{id}",
            std::process::id()
        ))
        .unwrap();
        let state = Arc::new(State {
            registration: OnceLock::new(),
            active_files: AtomicUsize::new(0),
            closed: tokio::sync::Notify::new(),
            #[cfg(test)]
            before_main_close: Mutex::new(None),
            #[cfg(test)]
            before_open: Mutex::new(None),
            original: original as usize,
            dir,
            main: main.into(),
            prefix,
            files: Mutex::new(files),
            error: Mutex::new(None),
            #[cfg(test)]
            sidecar_open_hook: Mutex::new(None),
            #[cfg(test)]
            before_retire: Mutex::new(None),
        });
        let mut table = Box::new(unsafe { ptr::read(original) });
        table.pNext = ptr::null_mut();
        table.zName = name.as_ptr();
        table.pAppData = Arc::as_ptr(&state) as *mut libc::c_void;
        table.szOsFile = (inner_offset() + unsafe { (*original).szOsFile as usize }) as i32;
        table.xOpen = Some(open);
        table.xDelete = Some(delete);
        table.xAccess = Some(access);
        table.xFullPathname = Some(full_path);
        table.xDlOpen = Some(dl_open);
        table.xDlError = Some(dl_error);
        table.xDlSym = None;
        table.xDlClose = Some(dl_close);
        table.xSetSystemCall = None;
        table.xGetSystemCall = None;
        table.xNextSystemCall = None;
        table.xRandomness = Some(randomness);
        table.xSleep = Some(sleep);
        table.xCurrentTime = Some(current_time);
        table.xGetLastError = Some(last_error);
        table.xCurrentTimeInt64 = Some(current_time64);
        if unsafe { sq::sqlite3_vfs_register(&mut *table, 0) } != sq::SQLITE_OK {
            return Err("Cannot register physical native SQLite owner".into());
        }
        let pointer = &mut *table as *mut sq::sqlite3_vfs;
        let registration = Arc::new(Registration {
            table,
            _state: state.clone(),
            _name: name,
        });
        state
            .registration
            .set(Arc::downgrade(&registration))
            .map_err(|_| "Native registration already set")?;
        Ok(Self {
            table: TablePointer(pointer),
            registration: registration.clone(),
            state,
            name: registration._name.clone(),
        })
    }
    #[cfg(test)]
    pub fn test_gates(
        &self,
        open: Box<dyn Fn() + Send + Sync>,
        retire: Box<dyn Fn(&str) + Send + Sync>,
    ) {
        *self.state.before_open.lock().unwrap() = Some(open);
        *self.state.before_retire.lock().unwrap() = Some(retire);
    }
    pub async fn wait_closed(&self) {
        loop {
            let notified = self.state.closed.notified();
            if self.state.active_files.load(Ordering::Acquire) == 0 {
                return;
            }
            notified.await;
        }
    }
    #[cfg(test)]
    pub fn test_close_gate(&self, gate: Box<dyn Fn() + Send + Sync>) {
        *self.state.before_main_close.lock().unwrap() = Some(gate);
    }
    pub fn name(&self) -> String {
        self.name.to_string_lossy().into_owned()
    }
    pub fn filename(&self) -> String {
        format!("{}{}", self.state.prefix, self.state.main)
    }
    pub fn check(&self) -> Result<(), String> {
        self.state.check(&self.state.main)
    }
    pub fn last_error(&self) -> Option<String> {
        self.state.error.lock().ok().and_then(|v| v.clone())
    }
}
impl Clone for NativeVfs {
    fn clone(&self) -> Self {
        Self {
            table: TablePointer(self.table.0),
            registration: self.registration.clone(),
            state: self.state.clone(),
            name: self.name.clone(),
        }
    }
}
#[repr(C)]
struct OwnedFilename(sq::sqlite3_filename);
impl Drop for OwnedFilename {
    fn drop(&mut self) {
        unsafe {
            sq::sqlite3_free_filename(self.0);
        }
    }
}
struct FileLease {
    registration: Arc<Registration>,
    state: Arc<State>,
    active: bool,
}
impl FileLease {
    fn new(state: &State) -> Result<Self, String> {
        let registration = state
            .registration
            .get()
            .and_then(Weak::upgrade)
            .ok_or("Native VFS registration unavailable")?;
        state.active_files.fetch_add(1, Ordering::AcqRel);
        Ok(Self {
            state: registration._state.clone(),
            registration,
            active: true,
        })
    }
}
impl Drop for FileLease {
    fn drop(&mut self) {
        if self.active {
            self.state.active_files.fetch_sub(1, Ordering::AcqRel);
            self.state.closed.notify_waiters();
        }
    }
}
#[repr(C)]
struct NativeFile {
    base: sq::sqlite3_file,
    _registration: FileLease,
    state: Arc<State>,
    inner: *mut sq::sqlite3_file,
    sidecar: Option<File>,
    key: String,
    _filename: OwnedFilename,
}
fn inner_offset() -> usize {
    let a = align_of::<sq::sqlite3_file>();
    (size_of::<NativeFile>() + a - 1) & !(a - 1)
}
unsafe fn state(v: *mut sq::sqlite3_vfs) -> &'static State {
    &*((*v).pAppData as *const State)
}
unsafe fn native(f: *mut sq::sqlite3_file) -> &'static mut NativeFile {
    &mut *(f as *mut NativeFile)
}
unsafe fn methods(f: &NativeFile) -> &sq::sqlite3_io_methods {
    &*(*f.inner).pMethods
}
unsafe fn check_file(f: &NativeFile) -> Result<(), String> {
    f.state.check(&f.key)?;
    if let Some(held) = f.sidecar.as_ref() {
        let expected = f
            .state
            .files
            .lock()
            .map_err(|_| "Native VFS identity lock failed")?
            .get(&f.key)
            .copied()
            .flatten()
            .ok_or("Sidecar identity missing")?;
        if identity(held).map_err(|e| e.to_string())? != expected {
            return Err("Actual held native sidecar descriptor changed".into());
        }
        return Ok(());
    }
    let mut moved = 0i32;
    let control = methods(f)
        .xFileControl
        .ok_or("Native Unix file control unavailable")?;
    let rc = control(
        f.inner,
        sq::SQLITE_FCNTL_HAS_MOVED,
        &mut moved as *mut _ as *mut libc::c_void,
    );
    if rc != sq::SQLITE_OK || moved != 0 {
        return Err(format!(
            "Actual opened native SQLite descriptor changed ({rc}, moved={moved}, key={})",
            f.key
        ));
    }
    Ok(())
}
unsafe extern "C" fn open(
    v: *mut sq::sqlite3_vfs,
    name: *const libc::c_char,
    file: *mut sq::sqlite3_file,
    flags: i32,
    out: *mut i32,
) -> i32 {
    (*file).pMethods = ptr::null();
    let s = state(v);
    let result = (|| -> Result<(), String> {
        let key = s.key(name)?;
        #[cfg(test)]
        if key == s.main {
            if let Some(hook) = s.before_open.lock().unwrap().as_ref() {
                hook();
            }
        }
        s.check(&key)?;
        let absent = s
            .files
            .lock()
            .map_err(|_| "Native VFS lock failed")?
            .get(&key)
            .copied()
            .flatten()
            .is_none();
        if absent {
            if flags & sq::SQLITE_OPEN_CREATE == 0 {
                return Err("Original native SQLite child missing".into());
            }
            let f = s
                .dir
                .open(&key, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)
                .map_err(|e| e.to_string())?;
            f.sync_all().map_err(|e| e.to_string())?;
            s.dir.held.sync_all().map_err(|e| e.to_string())?;
            s.refresh_created(&key, &f)?;
        }
        let main = s.dir.proc_name(&s.main).map_err(|e| e.to_string())?;
        let journal = s
            .dir
            .proc_name(&format!("{}-journal", s.main))
            .map_err(|e| e.to_string())?;
        let wal = s
            .dir
            .proc_name(&format!("{}-wal", s.main))
            .map_err(|e| e.to_string())?;
        let filename = OwnedFilename(sq::sqlite3_create_filename(
            main.as_ptr(),
            journal.as_ptr(),
            wal.as_ptr(),
            0,
            ptr::null_mut(),
        ));
        if filename.0.is_null() {
            return Err("Native SQLite filename allocation failed".into());
        }
        let path = if key == s.main {
            filename.0
        } else if key == format!("{}-journal", s.main) {
            sq::sqlite3_filename_journal(filename.0)
        } else if key == format!("{}-wal", s.main) {
            sq::sqlite3_filename_wal(filename.0)
        } else {
            return Err("Direct SQLite SHM open unsupported".into());
        };
        let original = s.original as *mut sq::sqlite3_vfs;
        let inner = (file as *mut u8).add(inner_offset()) as *mut sq::sqlite3_file;
        (*inner).pMethods = ptr::null();
        if key != s.main {
            // Unix no-lock WAL/journal handles have no public descriptor identity
            // proof. Own sidecar I/O directly from the actual openat File.
            #[cfg(test)]
            if let Some(hook) = s.sidecar_open_hook.lock().unwrap().as_ref() {
                hook(&key, false);
            }
            let held = s
                .dir
                .open(
                    &key,
                    if flags & sq::SQLITE_OPEN_READONLY != 0 {
                        libc::O_RDONLY
                    } else {
                        libc::O_RDWR
                    },
                )
                .map_err(|e| e.to_string())?;
            #[cfg(test)]
            if let Some(hook) = s.sidecar_open_hook.lock().unwrap().as_ref() {
                hook(&key, true);
            }
            Arc::increment_strong_count(s as *const State);
            let candidate = NativeFile {
                _registration: FileLease::new(s)?,
                base: sq::sqlite3_file { pMethods: &IO },
                state: Arc::from_raw(s as *const State),
                inner: ptr::null_mut(),
                sidecar: Some(held),
                key,
                _filename: filename,
            };
            check_file(&candidate)?;
            if !out.is_null() {
                *out = flags;
            }
            ptr::write(file as *mut NativeFile, candidate);
            return Ok(());
        }
        let rc = (*original)
            .xOpen
            .ok_or("Original Unix SQLite open unavailable")?(
            original,
            path,
            inner,
            (flags & !sq::SQLITE_OPEN_URI) | sq::SQLITE_OPEN_NOFOLLOW,
            out,
        );
        if rc != sq::SQLITE_OK {
            // Unix may return a failed open with an allocated sqlite3_file.
            // Filename storage must outlive its original xClose even on error.
            if !(*inner).pMethods.is_null() {
                if let Some(close) = (*(*inner).pMethods).xClose {
                    close(inner);
                }
                (*inner).pMethods = ptr::null();
            }
            return Err(format!("Actual native SQLite open failed ({rc})"));
        }
        Arc::increment_strong_count(s as *const State);
        let held = Arc::from_raw(s as *const State);
        let candidate = NativeFile {
            _registration: FileLease::new(s)?,
            base: sq::sqlite3_file { pMethods: &IO },
            state: held,
            inner,
            sidecar: None,
            key,
            _filename: filename,
        };
        if let Err(e) = check_file(&candidate) {
            if let Some(close) = methods(&candidate).xClose {
                close(inner);
            }
            return Err(e);
        }
        ptr::write(file as *mut NativeFile, candidate);
        Ok(())
    })();
    match result {
        Ok(()) => sq::SQLITE_OK,
        Err(e) => {
            s.error(e);
            sq::SQLITE_CANTOPEN
        }
    }
}
unsafe extern "C" fn full_path(
    v: *mut sq::sqlite3_vfs,
    name: *const libc::c_char,
    n: i32,
    out: *mut libc::c_char,
) -> i32 {
    let s = state(v);
    if let Err(e) = s.key(name) {
        return s.error(e);
    }
    let bytes = CStr::from_ptr(name).to_bytes_with_nul();
    if n <= 0 || bytes.len() > n as usize {
        return sq::SQLITE_CANTOPEN;
    }
    ptr::copy_nonoverlapping(bytes.as_ptr(), out as *mut u8, bytes.len());
    sq::SQLITE_OK
}
unsafe extern "C" fn delete(v: *mut sq::sqlite3_vfs, name: *const libc::c_char, _sync: i32) -> i32 {
    let s = state(v);
    let result = (|| -> Result<(), String> {
        let key = s.key(name)?;
        if key == s.main {
            return Err("Native download owner never deletes its database".into());
        }
        s.retire(&key)?;
        Ok(())
    })();
    result.map(|_| sq::SQLITE_OK).unwrap_or_else(|e| s.error(e))
}
unsafe extern "C" fn access(
    v: *mut sq::sqlite3_vfs,
    name: *const libc::c_char,
    _flags: i32,
    out: *mut i32,
) -> i32 {
    let s = state(v);
    let result = (|| -> Result<(), String> {
        let key = s.key(name)?;
        s.check(&key)?;
        *out = i32::from(
            s.files
                .lock()
                .map_err(|_| "Native VFS lock failed")?
                .get(&key)
                .copied()
                .flatten()
                .is_some(),
        );
        Ok(())
    })();
    result.map(|_| sq::SQLITE_OK).unwrap_or_else(|e| s.error(e))
}
unsafe extern "C" fn close(f: *mut sq::sqlite3_file) -> i32 {
    let n = native(f);
    let registration = n._registration.registration.clone();
    #[cfg(test)]
    if n.key == n.state.main {
        if let Some(gate) = n.state.before_main_close.lock().unwrap().as_ref() {
            gate();
        }
    }
    let rc = if n.sidecar.is_some() {
        sq::SQLITE_OK
    } else {
        methods(n)
            .xClose
            .map(|c| c(n.inner))
            .unwrap_or(sq::SQLITE_IOERR_CLOSE)
    };
    let state = n.state.clone();
    n._registration.active = false;
    ptr::drop_in_place(f as *mut NativeFile);
    (*f).pMethods = ptr::null();
    drop(registration);
    state.active_files.fetch_sub(1, Ordering::AcqRel);
    state.closed.notify_waiters();
    rc
}
macro_rules! io_call {($name:ident,$slot:ident,($($arg:ident:$ty:ty),*))=>{
 unsafe extern "C" fn $name(f:*mut sq::sqlite3_file,$($arg:$ty),*)->i32{let n=native(f);if let Err(e)=check_file(n){return n.state.error(e);}methods(n).$slot.map(|c|c(n.inner,$($arg),*)).unwrap_or(sq::SQLITE_IOERR)}
};}
unsafe extern "C" fn read(
    f: *mut sq::sqlite3_file,
    out: *mut libc::c_void,
    count: i32,
    offset: i64,
) -> i32 {
    let n = native(f);
    if let Err(e) = check_file(n) {
        return n.state.error(e);
    }
    let Some(file) = n.sidecar.as_ref() else {
        return methods(n)
            .xRead
            .map(|c| c(n.inner, out, count, offset))
            .unwrap_or(sq::SQLITE_IOERR_READ);
    };
    if count < 0 || offset < 0 {
        return sq::SQLITE_IOERR_READ;
    }
    let bytes = std::slice::from_raw_parts_mut(out as *mut u8, count as usize);
    let mut done = 0;
    while done < bytes.len() {
        match file.read_at(&mut bytes[done..], offset as u64 + done as u64) {
            Ok(0) => {
                bytes[done..].fill(0);
                return sq::SQLITE_IOERR_SHORT_READ;
            }
            Ok(n) => done += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return n.state.error(e),
        }
    }
    check_file(n)
        .map(|_| sq::SQLITE_OK)
        .unwrap_or_else(|e| n.state.error(e))
}
unsafe extern "C" fn write(
    f: *mut sq::sqlite3_file,
    input: *const libc::c_void,
    count: i32,
    offset: i64,
) -> i32 {
    let n = native(f);
    if let Err(e) = check_file(n) {
        return n.state.error(e);
    }
    let Some(file) = n.sidecar.as_ref() else {
        return methods(n)
            .xWrite
            .map(|c| c(n.inner, input, count, offset))
            .unwrap_or(sq::SQLITE_IOERR_WRITE);
    };
    if count < 0 || offset < 0 {
        return sq::SQLITE_IOERR_WRITE;
    }
    let bytes = std::slice::from_raw_parts(input as *const u8, count as usize);
    let mut done = 0;
    while done < bytes.len() {
        match file.write_at(&bytes[done..], offset as u64 + done as u64) {
            Ok(0) => return n.state.error("Sidecar write made no progress"),
            Ok(n) => done += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return n.state.error(e),
        }
    }
    check_file(n)
        .map(|_| sq::SQLITE_OK)
        .unwrap_or_else(|e| n.state.error(e))
}
unsafe extern "C" fn truncate(f: *mut sq::sqlite3_file, length: i64) -> i32 {
    let n = native(f);
    if let Err(e) = check_file(n) {
        return n.state.error(e);
    }
    if length < 0 {
        return sq::SQLITE_IOERR_TRUNCATE;
    }
    if let Some(file) = n.sidecar.as_ref() {
        return file
            .set_len(length as u64)
            .map(|_| sq::SQLITE_OK)
            .unwrap_or_else(|e| n.state.error(e));
    }
    methods(n)
        .xTruncate
        .map(|c| c(n.inner, length))
        .unwrap_or(sq::SQLITE_IOERR_TRUNCATE)
}
unsafe extern "C" fn sync(f: *mut sq::sqlite3_file, flags: i32) -> i32 {
    let n = native(f);
    if let Err(e) = check_file(n) {
        return n.state.error(e);
    }
    if let Some(file) = n.sidecar.as_ref() {
        return file
            .sync_all()
            .map(|_| sq::SQLITE_OK)
            .unwrap_or_else(|e| n.state.error(e));
    }
    methods(n)
        .xSync
        .map(|c| c(n.inner, flags))
        .unwrap_or(sq::SQLITE_IOERR_FSYNC)
}
unsafe extern "C" fn file_size(f: *mut sq::sqlite3_file, out: *mut i64) -> i32 {
    let n = native(f);
    if let Err(e) = check_file(n) {
        return n.state.error(e);
    }
    if let Some(file) = n.sidecar.as_ref() {
        return match file.metadata() {
            Ok(m) if m.len() <= i64::MAX as u64 => {
                *out = m.len() as i64;
                sq::SQLITE_OK
            }
            Ok(_) => sq::SQLITE_IOERR_FSTAT,
            Err(e) => n.state.error(e),
        };
    }
    methods(n)
        .xFileSize
        .map(|c| c(n.inner, out))
        .unwrap_or(sq::SQLITE_IOERR_FSTAT)
}
unsafe extern "C" fn lock(f: *mut sq::sqlite3_file, level: i32) -> i32 {
    let n = native(f);
    if let Err(e) = check_file(n) {
        return n.state.error(e);
    }
    if n.sidecar.is_some() {
        sq::SQLITE_OK
    } else {
        methods(n)
            .xLock
            .map(|c| c(n.inner, level))
            .unwrap_or(sq::SQLITE_IOERR_LOCK)
    }
}
unsafe extern "C" fn reserved(f: *mut sq::sqlite3_file, out: *mut i32) -> i32 {
    let n = native(f);
    if let Err(e) = check_file(n) {
        return n.state.error(e);
    }
    if n.sidecar.is_some() {
        *out = 0;
        sq::SQLITE_OK
    } else {
        methods(n)
            .xCheckReservedLock
            .map(|c| c(n.inner, out))
            .unwrap_or(sq::SQLITE_IOERR_CHECKRESERVEDLOCK)
    }
}
unsafe extern "C" fn control(f: *mut sq::sqlite3_file, op: i32, arg: *mut libc::c_void) -> i32 {
    let n = native(f);
    if let Err(e) = check_file(n) {
        return n.state.error(e);
    }
    if n.sidecar.is_some() {
        sq::SQLITE_NOTFOUND
    } else {
        methods(n)
            .xFileControl
            .map(|c| c(n.inner, op, arg))
            .unwrap_or(sq::SQLITE_NOTFOUND)
    }
}
unsafe extern "C" fn unlock(f: *mut sq::sqlite3_file, level: i32) -> i32 {
    let n = native(f);
    if n.sidecar.is_some() {
        sq::SQLITE_OK
    } else {
        methods(n)
            .xUnlock
            .map(|c| c(n.inner, level))
            .unwrap_or(sq::SQLITE_IOERR_UNLOCK)
    }
}
unsafe extern "C" fn sector(f: *mut sq::sqlite3_file) -> i32 {
    let n = native(f);
    if n.sidecar.is_some() {
        512
    } else {
        methods(n).xSectorSize.map(|c| c(n.inner)).unwrap_or(512)
    }
}
unsafe extern "C" fn characteristics(f: *mut sq::sqlite3_file) -> i32 {
    let n = native(f);
    if n.sidecar.is_some() {
        0
    } else {
        methods(n)
            .xDeviceCharacteristics
            .map(|c| c(n.inner))
            .unwrap_or(0)
    }
}
unsafe extern "C" fn shm_map(
    f: *mut sq::sqlite3_file,
    page: i32,
    size: i32,
    extend: i32,
    out: *mut *mut libc::c_void,
) -> i32 {
    let n = native(f);
    if let Err(e) = check_file(n) {
        return n.state.error(e);
    }
    let _ = (page, size, extend);
    *out = ptr::null_mut();
    n.state
        .error("Native custody requires EXCLUSIVE heap WAL index; disk SHM access refused")
}
unsafe extern "C" fn shm_lock(
    f: *mut sq::sqlite3_file,
    _offset: i32,
    _count: i32,
    _flags: i32,
) -> i32 {
    native(f)
        .state
        .error("Native custody refuses disk SHM locks")
}
unsafe extern "C" fn shm_barrier(_: *mut sq::sqlite3_file) {
    std::sync::atomic::fence(Ordering::SeqCst);
}
unsafe extern "C" fn shm_unmap(f: *mut sq::sqlite3_file, _delete: i32) -> i32 {
    let n = native(f);
    if n.sidecar.is_some() {
        return sq::SQLITE_OK;
    }
    methods(n)
        .xShmUnmap
        .map(|c| c(n.inner, 0))
        .unwrap_or(sq::SQLITE_OK)
}
unsafe extern "C" fn fetch(
    _: *mut sq::sqlite3_file,
    _: i64,
    _: i32,
    out: *mut *mut libc::c_void,
) -> i32 {
    *out = ptr::null_mut();
    sq::SQLITE_OK
}
unsafe extern "C" fn unfetch(_: *mut sq::sqlite3_file, _: i64, _: *mut libc::c_void) -> i32 {
    sq::SQLITE_OK
}
static IO: sq::sqlite3_io_methods = sq::sqlite3_io_methods {
    iVersion: 3,
    xClose: Some(close),
    xRead: Some(read),
    xWrite: Some(write),
    xTruncate: Some(truncate),
    xSync: Some(sync),
    xFileSize: Some(file_size),
    xLock: Some(lock),
    xUnlock: Some(unlock),
    xCheckReservedLock: Some(reserved),
    xFileControl: Some(control),
    xSectorSize: Some(sector),
    xDeviceCharacteristics: Some(characteristics),
    xShmMap: Some(shm_map),
    xShmLock: Some(shm_lock),
    xShmBarrier: Some(shm_barrier),
    xShmUnmap: Some(shm_unmap),
    xFetch: Some(fetch),
    xUnfetch: Some(unfetch),
};
unsafe extern "C" fn dl_open(_: *mut sq::sqlite3_vfs, _: *const libc::c_char) -> *mut libc::c_void {
    ptr::null_mut()
}
unsafe extern "C" fn dl_error(_: *mut sq::sqlite3_vfs, n: i32, out: *mut libc::c_char) {
    if n > 0 {
        *out = 0;
    }
}
unsafe extern "C" fn dl_close(_: *mut sq::sqlite3_vfs, _: *mut libc::c_void) {}
unsafe extern "C" fn randomness(v: *mut sq::sqlite3_vfs, n: i32, out: *mut libc::c_char) -> i32 {
    let o = state(v).original as *mut sq::sqlite3_vfs;
    (*o).xRandomness.map(|c| c(o, n, out)).unwrap_or(0)
}
unsafe extern "C" fn sleep(v: *mut sq::sqlite3_vfs, n: i32) -> i32 {
    let o = state(v).original as *mut sq::sqlite3_vfs;
    (*o).xSleep.map(|c| c(o, n)).unwrap_or(0)
}
unsafe extern "C" fn current_time(v: *mut sq::sqlite3_vfs, out: *mut f64) -> i32 {
    let o = state(v).original as *mut sq::sqlite3_vfs;
    (*o).xCurrentTime
        .map(|c| c(o, out))
        .unwrap_or(sq::SQLITE_IOERR)
}
unsafe extern "C" fn current_time64(v: *mut sq::sqlite3_vfs, out: *mut i64) -> i32 {
    let o = state(v).original as *mut sq::sqlite3_vfs;
    (*o).xCurrentTimeInt64
        .map(|c| c(o, out))
        .unwrap_or(sq::SQLITE_IOERR)
}
unsafe extern "C" fn last_error(v: *mut sq::sqlite3_vfs, n: i32, out: *mut libc::c_char) -> i32 {
    let o = state(v).original as *mut sq::sqlite3_vfs;
    (*o).xGetLastError.map(|c| c(o, n, out)).unwrap_or(0)
}

#[cfg(test)]
mod custody_race_tests {
    use super::super::filesystem::identity;
    use super::*;
    use std::fs;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::FileExt;
    fn fixture() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "native-vfs-race-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&p).unwrap();
        fs::write(p.join("native.sqlite3"), b"").unwrap();
        p
    }
    #[test]
    fn created_sidecar_exchange_must_not_adopt_reopened_foreign_identity() {
        let root = fixture();
        let v = NativeVfs::new(Directory::capture(&root).unwrap(), "native.sqlite3").unwrap();
        let key = "native.sqlite3-journal";
        let held = v
            .state
            .dir
            .open(key, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)
            .unwrap();
        held.sync_all().unwrap();
        let original = identity(&held).unwrap();
        fs::rename(root.join(key), root.join("retained-original-journal")).unwrap();
        fs::write(root.join(key), b"foreign private evidence").unwrap();
        assert_ne!(v.state.dir.file_identity(key).unwrap(), Some(original));
        let result = v.state.refresh_created(key, &held);
        assert!(
            result.is_err(),
            "creation must compare the actual exclusive-created descriptor"
        );
        assert_eq!(
            fs::read(root.join(key)).unwrap(),
            b"foreign private evidence"
        );
        assert_eq!(
            v.state.files.lock().unwrap().get(key).copied().flatten(),
            None
        );
        drop(v);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn retirement_exchange_after_check_must_retain_foreign_bytes() {
        let root = fixture();
        let key = "native.sqlite3-journal";
        fs::write(root.join(key), b"owned old journal").unwrap();
        let mut v = NativeVfs::new(Directory::capture(&root).unwrap(), "native.sqlite3").unwrap();
        let exchanged = root.clone();
        *v.state.before_retire.lock().unwrap() = Some(Box::new(move |key| {
            fs::rename(
                exchanged.join(key),
                exchanged.join("retained-original-journal"),
            )
            .unwrap();
            fs::write(exchanged.join(key), b"foreign private evidence").unwrap();
        }));
        let name = CString::new(format!("{}{}", v.state.prefix, key)).unwrap();
        let rc = unsafe { delete(&mut *v.table, name.as_ptr(), 1) };
        let mut retained = false;
        for directory in [root.clone(), root.join("sqlite-retired")] {
            if !directory.exists() {
                continue;
            }
            for entry in fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_file()
                    && fs::read(entry.path()).unwrap() == b"foreign private evidence"
                {
                    retained = true;
                }
            }
        }
        assert!(retained, "foreign replacement must never be unlinked");
        assert_ne!(rc, sq::SQLITE_OK);
        assert_eq!(
            fs::read(root.join("retained-original-journal")).unwrap(),
            b"owned old journal"
        );
        drop(v);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn actual_normal_wal_pager_refuses_disk_shm_before_unix_access() {
        let root = fixture();
        let v = NativeVfs::new(Directory::capture(&root).unwrap(), "native.sqlite3").unwrap();
        let filename = CString::new(v.filename()).unwrap();
        let name = CString::new(v.name()).unwrap();
        let mut db = ptr::null_mut();
        unsafe {
            assert_eq!(
                sq::sqlite3_open_v2(
                    filename.as_ptr(),
                    &mut db,
                    sq::SQLITE_OPEN_READWRITE,
                    name.as_ptr()
                ),
                sq::SQLITE_OK
            );
            let sql = CString::new("PRAGMA journal_mode=WAL; CREATE TABLE owned(v TEXT);").unwrap();
            assert_ne!(
                sq::sqlite3_exec(db, sql.as_ptr(), None, ptr::null_mut(), ptr::null_mut()),
                sq::SQLITE_OK
            );
            assert_eq!(sq::sqlite3_close(db), sq::SQLITE_OK);
        }
        assert!(!root.join("native.sqlite3-shm").exists());
        assert_eq!(
            v.state
                .files
                .lock()
                .unwrap()
                .get("native.sqlite3-shm")
                .copied()
                .flatten(),
            None
        );
        assert!(v.last_error().unwrap().contains("disk SHM"));
        drop(v);
        fs::remove_dir_all(root).unwrap();
    }
    unsafe fn actual_sidecar_open(
        v: &mut NativeVfs,
        key: &str,
        flags: i32,
    ) -> (*mut sq::sqlite3_file, i32) {
        let file = sq::sqlite3_malloc(v.table.szOsFile) as *mut sq::sqlite3_file;
        assert!(!file.is_null());
        std::ptr::write_bytes(file as *mut u8, 0, v.table.szOsFile as usize);
        let name = CString::new(format!("{}{}", v.state.prefix, key)).unwrap();
        let mut out = 0;
        let rc = v.table.xOpen.unwrap()(&mut *v.table, name.as_ptr(), file, flags, &mut out);
        (file, rc)
    }
    unsafe fn actual_sidecar_close(file: *mut sq::sqlite3_file) {
        if !(*file).pMethods.is_null() {
            assert_eq!((*(*file).pMethods).xClose.unwrap()(file), sq::SQLITE_OK);
        }
        sq::sqlite3_free(file as *mut libc::c_void);
    }
    #[test]
    fn actual_wal_and_journal_open_exchange_then_restore_refuses_foreign_descriptor() {
        for (suffix, kind) in [
            ("wal", sq::SQLITE_OPEN_WAL),
            ("journal", sq::SQLITE_OPEN_MAIN_JOURNAL),
        ] {
            let root = fixture();
            let key = format!("native.sqlite3-{suffix}");
            fs::write(root.join(&key), b"ORIGINAL").unwrap();
            let mut v =
                NativeVfs::new(Directory::capture(&root).unwrap(), "native.sqlite3").unwrap();
            let exchange = root.clone();
            *v.state.sidecar_open_hook.lock().unwrap() = Some(Box::new(move |key, after| {
                if !after {
                    fs::rename(exchange.join(key), exchange.join("saved-original")).unwrap();
                    fs::write(exchange.join(key), b"FOREIGN!").unwrap();
                } else {
                    fs::rename(exchange.join(key), exchange.join("retained-foreign")).unwrap();
                    fs::rename(exchange.join("saved-original"), exchange.join(key)).unwrap();
                }
            }));
            unsafe {
                let (file, rc) =
                    actual_sidecar_open(&mut v, &key, sq::SQLITE_OPEN_READWRITE | kind);
                assert_ne!(
                    rc,
                    sq::SQLITE_OK,
                    "a restored pathname cannot certify the opened foreign descriptor"
                );
                actual_sidecar_close(file);
            }
            assert_eq!(fs::read(root.join(&key)).unwrap(), b"ORIGINAL");
            assert_eq!(
                fs::read(root.join("retained-foreign")).unwrap(),
                b"FOREIGN!"
            );
            drop(v);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn actual_held_sidecar_io_short_read_zeroes_and_exchange_never_writes_foreign() {
        for (suffix, kind) in [
            ("wal", sq::SQLITE_OPEN_WAL),
            ("journal", sq::SQLITE_OPEN_MAIN_JOURNAL),
        ] {
            let root = fixture();
            let key = format!("native.sqlite3-{suffix}");
            fs::write(root.join(&key), b"owned").unwrap();
            let mut v =
                NativeVfs::new(Directory::capture(&root).unwrap(), "native.sqlite3").unwrap();
            unsafe {
                let (file, rc) =
                    actual_sidecar_open(&mut v, &key, sq::SQLITE_OPEN_READWRITE | kind);
                assert_eq!(rc, sq::SQLITE_OK);
                let io = &*(*file).pMethods;
                let mut bytes = [255u8; 8];
                assert_eq!(
                    io.xRead.unwrap()(file, bytes.as_mut_ptr() as *mut libc::c_void, 8, 0),
                    sq::SQLITE_IOERR_SHORT_READ
                );
                assert_eq!(&bytes, b"owned\0\0\0");
                assert_eq!(
                    io.xWrite.unwrap()(file, b"EXACT".as_ptr() as *const libc::c_void, 5, 0),
                    sq::SQLITE_OK
                );
                assert_eq!(io.xTruncate.unwrap()(file, 5), sq::SQLITE_OK);
                assert_eq!(io.xSync.unwrap()(file, sq::SQLITE_SYNC_FULL), sq::SQLITE_OK);
                let mut len = 0;
                assert_eq!(io.xFileSize.unwrap()(file, &mut len), sq::SQLITE_OK);
                assert_eq!(len, 5);
                fs::rename(root.join(&key), root.join("retained-owned")).unwrap();
                fs::write(root.join(&key), b"FOREIGN").unwrap();
                assert_ne!(
                    io.xWrite.unwrap()(file, b"BAD".as_ptr() as *const libc::c_void, 3, 0),
                    sq::SQLITE_OK
                );
                actual_sidecar_close(file);
            }
            assert_eq!(fs::read(root.join(&key)).unwrap(), b"FOREIGN");
            assert_eq!(fs::read(root.join("retained-owned")).unwrap(), b"EXACT");
            drop(v);
            fs::remove_dir_all(root).unwrap();
        }
    }
}
