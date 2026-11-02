//! Operation-scoped Linux read custody, not content/topology certification.
//!
//! The caller MUST admit each selected mount with a nonblocking policy callback.
//! Leases/inotify do not establish remote-filesystem coherence. Seal before the
//! caller's certified baseline; never reuse a failed set or rebase its evidence.
use std::{
    collections::HashMap,
    ffi::CString,
    fs::{File, OpenOptions},
    io,
    os::{fd::{AsRawFd, FromRawFd}, unix::{fs::{MetadataExt, OpenOptionsExt}}},
    sync::{atomic::{AtomicBool, Ordering}, mpsc, Arc, Mutex},
    thread::{self, JoinHandle},
    time::Duration,
};

pub(crate) type LocalFilesystemPolicy = fn(&File) -> io::Result<()>;

/// Admission prerequisite for these operation-specific local-VFS consumers.
/// Family membership never replaces successful per-inode lease/watch custody.
pub(crate) fn admit_local_filesystem(file: &File) -> io::Result<()> {
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: held descriptor and writable storage for the complete syscall result.
    if unsafe { libc::fstatfs(file.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful fstatfs initialized stat.
    let family = unsafe { stat.assume_init() }.f_type as u32;
    if ![0x9123_683e, 0x0102_1994, 0x0000_ef53, 0x5846_5342].contains(&family) {
        return Err(io::Error::other("File custody requires admitted local filesystem and live lease/watch admission"));
    }
    Ok(())
}

/// Cancellation is nonjoining and may be called from an async future's Drop.
#[derive(Clone)]
pub(crate) struct BrokerCancelHandle {
    wake: Arc<File>,
    poison: Arc<AtomicBool>,
}
impl BrokerCancelHandle {
    pub(crate) fn cancel(&self) {
        self.poison.store(true, Ordering::SeqCst);
        let _ = wake(&self.wake);
    }
}

const TIMEOUT: Duration = Duration::from_secs(5);
const WATCH: u32 = libc::IN_ATTRIB | libc::IN_MODIFY | libc::IN_DELETE_SELF
    | libc::IN_MOVE_SELF | libc::IN_UNMOUNT | libc::IN_IGNORED;
fn refused() -> io::Error { io::Error::other("File read custody unavailable or permanently lost") }
fn cv(result: i32) -> io::Result<i32> {
    if result < 0 { Err(io::Error::last_os_error()) } else { Ok(result) }
}
fn owned(fd: i32) -> io::Result<File> {
    cv(fd)?;
    // SAFETY: this newly created descriptor is transferred exactly once.
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn wake(fd: &File) -> io::Result<()> {
    let value: u64 = 1;
    // SAFETY: eventfd expects exactly one initialized u64.
    let n = unsafe { libc::write(fd.as_raw_fd(), (&value as *const u64).cast(), 8) };
    if n == 8 { Ok(()) } else { Err(io::Error::last_os_error()) }
}
type Reply = mpsc::SyncSender<io::Result<()>>;
enum Command {
    Seal(File, Reply), Fence(Reply),
    #[cfg(test)]
    LoseNotifications(Reply),
}

/// One dedicated, nonmigrating worker per operation, regardless of file count.
/// Policy is required and must not block; there is deliberately no allow-all default.
pub(crate) struct CustodySet {
    tx: Option<mpsc::SyncSender<Command>>,
    wake: Arc<File>,
    poison: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    serial: Mutex<()>,
    worker: Option<JoinHandle<()>>,
}
impl CustodySet {
    pub(crate) fn new(policy: LocalFilesystemPolicy) -> io::Result<Self> {
        // SAFETY: no pointers; nonblocking wakeup descriptor owned by this set.
        let wake = Arc::new(owned(unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) })?);
        let poison = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::sync_channel(1);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let (w, p, s) = (wake.clone(), poison.clone(), stop.clone());
        let worker = thread::Builder::new().name("file-read-custody".into()).spawn(move || {
            // Any panic, startup failure, or disconnect permanently invalidates custody.
            struct Exit(Arc<AtomicBool>);
            impl Drop for Exit { fn drop(&mut self) { self.0.store(true, Ordering::SeqCst); } }
            let _exit = Exit(p.clone());
            match Worker::new(w, p.clone(), s, policy) {
                Ok(mut worker) => {
                    if ready_tx.send(Ok(())).is_ok() { worker.run(rx); }
                }
                Err(e) => { p.store(true, Ordering::SeqCst); let _ = ready_tx.send(Err(e)); }
            }
        })?;
        let mut set = Self { tx: Some(tx), wake, poison, stop, serial: Mutex::new(()), worker: Some(worker) };
        match ready_rx.recv_timeout(TIMEOUT) {
            Ok(Ok(())) if !set.poison.load(Ordering::SeqCst) => Ok(set),
            Ok(Err(e)) => { set.cancel(); Err(e) }
            _ => { set.cancel(); Err(refused()) }
        }
    }
    fn cancel(&mut self) {
        self.poison.store(true, Ordering::SeqCst);
        self.stop.store(true, Ordering::SeqCst);
        self.tx.take();
        let _ = wake(&self.wake);
        if let Some(worker) = self.worker.take() { let _ = worker.join(); }
    }
    fn request(&self, file: Option<&File>) -> io::Result<()> {
        let _serial = self.serial.lock().map_err(|_| {
            self.poison.store(true, Ordering::SeqCst);
            let _ = wake(&self.wake);
            refused()
        })?;
        if self.poison.load(Ordering::SeqCst) { return Err(refused()); }
        let (tx, rx) = mpsc::sync_channel(1);
        let result = (|| {
            let command = match file {
                Some(file) => Command::Seal(file.try_clone()?, tx),
                None => Command::Fence(tx),
            };
            self.tx.as_ref().ok_or_else(refused)?.try_send(command).map_err(|_| refused())?;
            wake(&self.wake)?;
            rx.recv_timeout(TIMEOUT).map_err(|_| refused())??;
            if self.poison.load(Ordering::SeqCst) { return Err(refused()); }
            Ok(())
        })();
        if result.is_err() {
            self.poison.store(true, Ordering::SeqCst);
            let _ = wake(&self.wake);
        }
        result
    }
    pub(crate) fn cancellation(&self) -> BrokerCancelHandle {
        BrokerCancelHandle { wake: self.wake.clone(), poison: self.poison.clone() }
    }
    pub(crate) fn seal(&self, file: &File) -> io::Result<()> { self.request(Some(file)) }
    pub(crate) fn fence(&self) -> io::Result<()> { self.request(None) }
}
impl Drop for CustodySet { fn drop(&mut self) { self.cancel(); } }

struct Worker {
    wake: Arc<File>, signals: File, changes: File,
    poison: Arc<AtomicBool>, stop: Arc<AtomicBool>,
    policy: LocalFilesystemPolicy,
    files: HashMap<(u64, u64), File>,
}
impl Worker {
    fn new(wake: Arc<File>, poison: Arc<AtomicBool>, stop: Arc<AtomicBool>, policy: LocalFilesystemPolicy) -> io::Result<Self> {
        // SAFETY: initialized local signal set; only this dedicated thread's mask
        // changes. No process-wide handler/disposition is installed or modified.
        let signals = unsafe {
            let mut mask: libc::sigset_t = std::mem::zeroed();
            cv(libc::sigemptyset(&mut mask))?;
            cv(libc::sigaddset(&mut mask, libc::SIGIO))?;
            let error = libc::pthread_sigmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut());
            if error != 0 { return Err(io::Error::from_raw_os_error(error)); }
            owned(libc::signalfd(-1, &mask, libc::SFD_CLOEXEC | libc::SFD_NONBLOCK))?
        };
        // SAFETY: no pointers; descriptor owned solely by worker.
        let changes = owned(unsafe { libc::inotify_init1(libc::IN_CLOEXEC | libc::IN_NONBLOCK) })?;
        Ok(Self { wake, signals, changes, poison, stop, policy, files: HashMap::new() })
    }
    fn lose(&mut self) {
        // Publish failure BEFORE consuming evidence or releasing a single lease.
        self.poison.store(true, Ordering::SeqCst);
        for file in self.files.values() {
            // SAFETY: worker exclusively controls these independent OFDs.
            unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLEASE, libc::F_UNLCK); }
        }
        self.files.clear();
    }
    fn notifications(&mut self) -> io::Result<()> {
        if self.poison.load(Ordering::SeqCst) { self.lose(); return Err(refused()); }
        let mut descriptors = [
            libc::pollfd { fd: self.signals.as_raw_fd(), events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: self.changes.as_raw_fd(), events: libc::POLLIN, revents: 0 },
        ];
        // SAFETY: valid initialized pollfd slice, zero-time observation.
        let ready = unsafe { libc::poll(descriptors.as_mut_ptr(), descriptors.len() as _, 0) };
        if ready != 0 {
            self.lose(); // Includes EINTR, watch loss, overflow, and poll errors.
            self.drain_notifications();
            return Err(refused());
        }
        Ok(())
    }
    fn drain_notifications(&self) {
        let mut bytes = [0u8; 8192];
        for file in [&self.signals, &self.changes] {
            // SAFETY: initialized writable buffer; both descriptors nonblocking.
            // Already poisoned/unlocked: one bounded read is sufficient. Never
            // let a continuing attacker event stream delay worker shutdown.
            let _ = unsafe { libc::read(file.as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len()) };
        }
    }
    fn fence(&mut self) -> io::Result<()> {
        self.notifications()?;
        for file in self.files.values() {
            (self.policy)(file)?;
            // SAFETY: held independent OFD, query only. A cancelled break is
            // still loss: F_GETLEASE may already report UNLCK before any write.
            if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETLEASE) } != libc::F_RDLCK {
                return Err(refused());
            }
        }
        self.notifications()
    }
    fn seal(&mut self, original: File) -> io::Result<()> {
        self.notifications()?; // Adding cannot drain/reset an older file's loss.
        let before = original.metadata()?;
        if !before.is_file() { return Err(refused()); }
        let key = (before.dev(), before.ino());
        if self.files.contains_key(&key) { return self.fence(); }
        // /proc/self/fd deliberately follows the held descriptor, never a caller
        // pathname; open creates an independent OFD (dup would share ownership).
        let path = format!("/proc/self/fd/{}", original.as_raw_fd());
        let file = OpenOptions::new().read(true).custom_flags(libc::O_CLOEXEC | libc::O_NONBLOCK).open(&path)?;
        let after = file.metadata()?;
        if !after.is_file() || (after.dev(), after.ino()) != key { return Err(refused()); }
        drop(original);
        (self.policy)(&file)?;
        let path = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd())).map_err(|_| refused())?;
        // SAFETY: valid proc FD path; watch is installed BEFORE the read lease
        // and therefore before any caller-certified baseline.
        cv(unsafe { libc::inotify_add_watch(self.changes.as_raw_fd(), path.as_ptr(), WATCH) })?;
        #[repr(C)] struct Owner { kind: libc::c_int, pid: libc::pid_t }
        // Linux F_SETOWN_EX=15, F_OWNER_TID=0, F_SETSIG=10 (fcntl.h).
        // libc does not expose these constants on every supported Linux target.
        let owner = Owner { kind: 0, pid: unsafe { libc::syscall(libc::SYS_gettid) as libc::pid_t } };
        // SAFETY: live repr(C) f_owner_ex ABI pointer; only worker receives SIGIO.
        cv(unsafe { libc::fcntl(file.as_raw_fd(), 15, &owner) })?;
        cv(unsafe { libc::fcntl(file.as_raw_fd(), 10, libc::SIGIO) })?;
        cv(unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLEASE, libc::F_RDLCK) })?;
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETLEASE) } != libc::F_RDLCK {
            return Err(refused());
        }
        self.files.insert(key, file);
        self.notifications()
    }
    fn run(&mut self, rx: mpsc::Receiver<Command>) {
        loop {
            if self.stop.load(Ordering::SeqCst) { break; }
            if self.notifications().is_err() { break; }
            let mut descriptors = [
                libc::pollfd { fd: self.wake.as_raw_fd(), events: libc::POLLIN, revents: 0 },
                libc::pollfd { fd: self.signals.as_raw_fd(), events: libc::POLLIN, revents: 0 },
                libc::pollfd { fd: self.changes.as_raw_fd(), events: libc::POLLIN, revents: 0 },
            ];
            // 50ms fallback also notices cancellation if the eventfd wake failed.
            if unsafe { libc::poll(descriptors.as_mut_ptr(), descriptors.len() as _, 50) } < 0 { break; }
            if self.notifications().is_err() { break; }
            if descriptors[0].revents & !(libc::POLLIN) != 0 { break; }
            if descriptors[0].revents & libc::POLLIN != 0 {
                let mut value: u64 = 0;
                if unsafe { libc::read(self.wake.as_raw_fd(), (&mut value as *mut u64).cast(), 8) } != 8 { break; }
            }
            match rx.try_recv() {
                Ok(command) => {
                    let (result, reply) = match command {
                        Command::Seal(file, reply) => (self.seal(file), reply),
                        Command::Fence(reply) => (self.fence(), reply),
                        #[cfg(test)]
                        Command::LoseNotifications(reply) => {
                            // Destroy the actual watch descriptor; poll on this
                            // readable non-watch replacement must fail closed.
                            let result = File::open("/dev/null").and_then(|file| {
                                self.changes = file;
                                self.fence()
                            });
                            (result, reply)
                        }
                    };
                    if result.is_err() { self.lose(); }
                    if reply.send(result).is_err() { self.lose(); break; }
                    if self.poison.load(Ordering::SeqCst) { break; }
                }
                Err(mpsc::TryRecvError::Empty) => (),
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        self.lose();
    }
}
impl Drop for Worker { fn drop(&mut self) { self.lose(); } }

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, os::unix::fs::PermissionsExt, path::PathBuf, process::{Child, Command as ProcessCommand}, sync::atomic::AtomicU64, time::Instant};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("read-custody-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            std::fs::create_dir(&path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
            std::fs::write(path.join("selected"), b"original").unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf { self.0.join("selected") }
        fn open(&self) -> File { File::open(self.path()).unwrap() }
    }
    impl Drop for Fixture { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
    // Explicit test policy: these owned fixtures must actually reside on the
    // reviewed host's tmpfs/btrfs. This is not a product filesystem allowlist.
    fn local_fixture(file: &File) -> io::Result<()> {
        let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
        cv(unsafe { libc::fstatfs(file.as_raw_fd(), &mut stat) })?;
        if matches!(stat.f_type as u64, 0x01021994 | 0x9123683e) { Ok(()) } else { Err(refused()) }
    }
    fn wait(child: &mut Child) {
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap() { assert!(status.success(), "owned writer failed: {status}"); return; }
            if start.elapsed() > Duration::from_secs(3) { let _ = child.kill(); let _ = child.wait(); panic!("owned writer stalled past three seconds"); }
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn writer(f: &Fixture, nonblocking: bool) -> Child {
        // Separate actual process; no inherited writable FD and no signal handler.
        ProcessCommand::new("python3").arg("-I").arg("-c").arg(if nonblocking {
            "import os,sys,errno\ntry: os.close(os.open(sys.argv[1],os.O_WRONLY|os.O_NONBLOCK))\nexcept OSError as e: sys.exit(0 if e.errno==errno.EAGAIN else 2)\nsys.exit(0)"
        } else {
            "import os,sys\nf=os.open(sys.argv[1],os.O_WRONLY);os.write(f,b'changed!');os.close(f)"
        }).arg(f.path()).spawn().unwrap()
    }
    #[test]
    fn clean_readers_and_duplicate_seal_preserve_custody() {
        let f = Fixture::new(); let file = f.open();
        let set = CustodySet::new(local_fixture).unwrap();
        set.seal(&file).unwrap();
        let mut reader = f.open(); let mut bytes = String::new(); reader.read_to_string(&mut bytes).unwrap();
        assert_eq!(bytes, "original"); drop(reader);
        set.seal(&file).unwrap(); set.fence().unwrap();
    }
    #[test]
    fn preexisting_writer_and_closed_fd_shared_writable_mapping_refuse() {
        let f = Fixture::new();
        let writer = OpenOptions::new().read(true).write(true).open(f.path()).unwrap();
        let set = CustodySet::new(local_fixture).unwrap();
        assert!(set.seal(&f.open()).is_err()); assert!(set.fence().is_err()); drop(set);
        let address = unsafe { libc::mmap(std::ptr::null_mut(), 8, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, writer.as_raw_fd(), 0) };
        assert_ne!(address, libc::MAP_FAILED); drop(writer);
        let set = CustodySet::new(local_fixture).unwrap();
        assert!(set.seal(&f.open()).is_err()); assert!(set.fence().is_err());
        assert_eq!(unsafe { libc::munmap(address, 8) }, 0);
        assert_eq!(std::fs::read(f.path()).unwrap(), b"original");
    }
    #[test]
    fn cancelled_child_write_open_poison_is_sticky_without_mutation() {
        let f = Fixture::new(); let set = CustodySet::new(local_fixture).unwrap(); set.seal(&f.open()).unwrap();
        wait(&mut writer(&f, true));
        assert!(set.fence().is_err()); assert!(set.seal(&f.open()).is_err()); assert!(set.fence().is_err());
        assert_eq!(std::fs::read(f.path()).unwrap(), b"original");
    }
    #[test]
    fn blocking_writer_break_is_released_promptly_and_never_rearmed() {
        let f = Fixture::new(); let set = CustodySet::new(local_fixture).unwrap(); set.seal(&f.open()).unwrap();
        let start = Instant::now(); wait(&mut writer(&f, false));
        assert!(start.elapsed() < Duration::from_secs(3));
        assert!(set.fence().is_err()); assert!(set.seal(&f.open()).is_err());
        assert_eq!(std::fs::read(f.path()).unwrap(), b"changed!");
    }
    #[test]
    fn hardlink_add_remove_and_new_seal_cannot_reset_old_mutation() {
        let f = Fixture::new(); let set = CustodySet::new(local_fixture).unwrap(); set.seal(&f.open()).unwrap();
        let alias = f.0.join("alias"); std::fs::hard_link(f.path(), &alias).unwrap(); std::fs::remove_file(alias).unwrap();
        std::fs::write(f.0.join("next"), b"next").unwrap();
        assert!(set.seal(&File::open(f.0.join("next")).unwrap()).is_err());
        assert!(set.fence().is_err()); assert_eq!(std::fs::metadata(f.path()).unwrap().nlink(), 1);
        assert_eq!(std::fs::read(f.path()).unwrap(), b"original");
    }
    #[test]
    fn drop_releases_without_mutating_signal_disposition() {
        let f = Fixture::new();
        let mut before: libc::sigaction = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::sigaction(libc::SIGIO, std::ptr::null(), &mut before) }, 0);
        let set = CustodySet::new(local_fixture).unwrap(); set.seal(&f.open()).unwrap(); drop(set);
        let mut after: libc::sigaction = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { libc::sigaction(libc::SIGIO, std::ptr::null(), &mut after) }, 0);
        assert_eq!(before.sa_sigaction, after.sa_sigaction); assert_eq!(before.sa_flags, after.sa_flags);
        wait(&mut writer(&f, false));
    }
    #[test]
    fn policy_failure_is_permanent_and_cancellation_releases() {
        fn deny(_: &File) -> io::Result<()> { Err(refused()) }
        let f = Fixture::new(); let set = CustodySet::new(deny).unwrap();
        assert!(set.seal(&f.open()).is_err()); assert!(set.fence().is_err());
        let set = CustodySet::new(local_fixture).unwrap(); set.seal(&f.open()).unwrap();
        set.poison.store(true, Ordering::SeqCst); wake(&set.wake).unwrap();
        assert!(set.fence().is_err()); wait(&mut writer(&f, false));
    }
    #[test]
    fn lost_reply_and_broker_disconnect_release_all_leases() {
        let f = Fixture::new(); let mut set = CustodySet::new(local_fixture).unwrap(); set.seal(&f.open()).unwrap();
        let (reply, dropped) = mpsc::sync_channel(1); drop(dropped);
        set.tx.as_ref().unwrap().send(Command::Fence(reply)).unwrap(); wake(&set.wake).unwrap();
        set.worker.take().unwrap().join().unwrap();
        assert!(set.poison.load(Ordering::SeqCst));
        wait(&mut writer(&f, false)); assert!(set.fence().is_err());
        set.tx.take(); drop(set);
        let set = CustodySet::new(local_fixture).unwrap(); set.seal(&f.open()).unwrap();
        let (reply, result) = mpsc::sync_channel(1);
        set.tx.as_ref().unwrap().send(Command::LoseNotifications(reply)).unwrap();
        wake(&set.wake).unwrap();
        assert!(result.recv_timeout(TIMEOUT).unwrap().is_err());
        assert!(set.fence().is_err()); wait(&mut writer(&f, false));
    }
}
