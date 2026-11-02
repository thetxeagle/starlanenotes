// Included inside restoration's Linux implementation, beside Root. This owner
// records observed history only: it is not a read lease, SQL exclusion, hash
// baseline, filesystem qualification, or authority to publish a capture.
mod source_history {
    use super::Root;
    use std::{
        collections::BTreeMap,
        ffi::CString,
        fs::File,
        io,
        os::fd::{AsRawFd, FromRawFd},
        path::Component,
        sync::Mutex,
    };

    const MASK: u32 = libc::IN_CREATE
        | libc::IN_DELETE
        | libc::IN_MOVED_FROM
        | libc::IN_MOVED_TO
        | libc::IN_MODIFY
        | libc::IN_ATTRIB
        | libc::IN_DELETE_SELF
        | libc::IN_MOVE_SELF
        | libc::IN_UNMOUNT
        | libc::IN_IGNORED;
    const LOSS: u32 = libc::IN_Q_OVERFLOW
        | libc::IN_UNMOUNT
        | libc::IN_IGNORED
        | libc::IN_DELETE_SELF
        | libc::IN_MOVE_SELF;

    fn refused() -> io::Error {
        io::Error::other("Original SQLite source history unavailable or changed")
    }
    fn local_namespace(file: &File) -> io::Result<()> {
        let mut status = std::mem::MaybeUninit::<libc::statfs>::uninit();
        if unsafe { libc::fstatfs(file.as_raw_fd(), status.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let family = unsafe { status.assume_init() }.f_type as u32;
        // Ancestor namespaces may be local overlay while selected file data is
        // on admitted tmpfs/ext4/etc. File read-lease policy remains separate.
        // Remote or unknown ancestor/selected directory history is not proven
        // by a watch that cannot observe changes made on another machine.
        if ![0x9123_683e, 0x0102_1994, 0x0000_ef53, 0x5846_5342, 0x794c_7630].contains(&family) {
            return Err(refused());
        }
        Ok(())
    }
    fn ignored(name: &[u8]) -> bool {
        matches!(name, b"logs" | b"logging" | b"search_index")
    }

    enum Rule {
        Ancestor(Vec<u8>),
        Directory { root: bool },
        File,
    }
    impl Rule {
        fn relevant(&self, name: &[u8]) -> bool {
            if name.is_empty() {
                return true;
            }
            match self {
                Self::Ancestor(selected) => name == selected,
                Self::Directory { root } => !*root || !ignored(name),
                Self::File => true,
            }
        }
    }
    struct Watch {
        // Pin the watched inode; never remove/reuse a watch during this owner.
        _held: File,
        rules: Vec<Rule>,
    }
    struct State {
        notifications: File,
        watches: BTreeMap<i32, Watch>,
        poisoned: bool,
    }
    pub(super) struct History {
        state: Mutex<State>,
    }

    impl History {
        pub(super) fn new(root: &Root) -> io::Result<Self> {
            // SAFETY: no pointer arguments; the successful FD is owned once.
            let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }
            let owner = Self {
                state: Mutex::new(State {
                    notifications: unsafe { File::from_raw_fd(fd) },
                    watches: BTreeMap::new(),
                    poisoned: false,
                }),
            };
            let components: Vec<_> = root
                .absolute
                .components()
                .filter_map(|component| match component {
                    Component::Normal(name) => Some(name),
                    _ => None,
                })
                .collect();
            if root.chain.len() != components.len() + 1 {
                return Err(refused());
            }
            use std::os::unix::ffi::OsStrExt;
            // Install from outermost ancestor inward, before any source walk.
            // Other /tmp, home, etc. children cannot poison this selection.
            for (ancestor, selected) in root.chain.iter().zip(&components) {
                owner.add(ancestor, Rule::Ancestor(selected.as_bytes().to_vec()), true)?;
            }
            owner.watch_directory(root.fd(), "")?;
            root.check()?;
            owner.fence()?;
            Ok(owner)
        }

        pub(super) fn watch_directory(&self, file: &File, prefix: &str) -> io::Result<()> {
            let valid = prefix.is_empty()
                || prefix.split('/').all(|part| {
                    !part.is_empty() && part != "." && part != ".." && !part.contains('\\')
                });
            if !valid || (!prefix.is_empty() && ignored(prefix.split('/').next().unwrap().as_bytes())) {
                return self.poison();
            }
            self.add(file, Rule::Directory { root: prefix.is_empty() }, true)
        }

        pub(super) fn watch_file(&self, file: &File) -> io::Result<()> {
            self.add(file, Rule::File, false)
        }

        fn poison<T>(&self) -> io::Result<T> {
            let mut state = self.state.lock().map_err(|_| refused())?;
            state.poisoned = true;
            Err(refused())
        }

        fn add(&self, file: &File, rule: Rule, directory: bool) -> io::Result<()> {
            let mut state = self.state.lock().map_err(|_| refused())?;
            if state.poisoned {
                return Err(refused());
            }
            let result = (|| {
                state.inspect()?;
                let metadata = file.metadata()?;
                if (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {
                    return Err(refused());
                }
                if directory { local_namespace(file)?; }
                let held = file.try_clone()?;
                let path = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd()))?;
                let mask = MASK | libc::IN_MASK_ADD | if directory { libc::IN_ONLYDIR } else { 0 };
                // SAFETY: pin a held inode through procfs; never resolve source names here.
                let wd = unsafe {
                    libc::inotify_add_watch(state.notifications.as_raw_fd(), path.as_ptr(), mask)
                };
                if wd < 0 {
                    return Err(io::Error::last_os_error());
                }
                state.watches.entry(wd).or_insert_with(|| Watch {
                    _held: held,
                    rules: Vec::new(),
                }).rules.push(rule);
                // Adding a watch must not reset mutations queued for older watches.
                state.inspect()
            })();
            if result.is_err() {
                state.poisoned = true;
            }
            result
        }

        pub(super) fn fence(&self) -> io::Result<()> {
            let mut state = self.state.lock().map_err(|_| refused())?;
            if state.poisoned {
                return Err(refused());
            }
            let result = state.inspect();
            if result.is_err() {
                state.poisoned = true;
            }
            result
        }
    }

    impl State {
        fn ready(&self) -> io::Result<bool> {
            let mut poll = libc::pollfd {
                fd: self.notifications.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: valid single pollfd; no waiting/retry or signal changes.
            if unsafe { libc::poll(&mut poll, 1, 0) } < 0 {
                return Err(io::Error::last_os_error());
            }
            if poll.revents & !libc::POLLIN != 0 {
                return Err(refused());
            }
            Ok(poll.revents & libc::POLLIN != 0)
        }

        fn inspect(&self) -> io::Result<()> {
            if !self.ready()? {
                return Ok(());
            }
            let mut available: libc::c_int = 0;
            // SAFETY: initialized writable byte-count output.
            if unsafe { libc::ioctl(self.notifications.as_raw_fd(), libc::FIONREAD, &mut available) } < 0 {
                return Err(io::Error::last_os_error());
            }
            let length = usize::try_from(available).map_err(|_| refused())?;
            if length == 0 {
                return Err(refused());
            }
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(length).map_err(|_| refused())?;
            bytes.resize(length, 0);
            // One queue snapshot only. Continuing producer pressure refuses,
            // rather than keeping a cleanup/fence thread in an unbounded drain.
            let count = unsafe {
                libc::read(self.notifications.as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len())
            };
            if count <= 0 {
                return Err(io::Error::last_os_error());
            }
            self.inspect_bytes(&bytes[..count as usize])?;
            if self.ready()? {
                return Err(refused());
            }
            Ok(())
        }

        fn inspect_bytes(&self, bytes: &[u8]) -> io::Result<()> {
            let header = std::mem::size_of::<libc::inotify_event>();
            let mut offset = 0usize;
            while offset < bytes.len() {
                if bytes.len() - offset < header {
                    return Err(refused());
                }
                // SAFETY: header bounds checked; kernel bytes need not be aligned.
                let event = unsafe {
                    bytes.as_ptr().add(offset).cast::<libc::inotify_event>().read_unaligned()
                };
                let end = offset.checked_add(header)
                    .and_then(|n| n.checked_add(event.len as usize)).ok_or_else(refused)?;
                // IN_ISDIR is an annotation, not an extra subscription.
                if end > bytes.len() || event.mask & LOSS != 0
                    || event.mask & !(MASK | libc::IN_ISDIR) != 0 {
                    return Err(refused());
                }
                let watch = self.watches.get(&event.wd).ok_or_else(refused)?;
                let raw = &bytes[offset + header..end];
                let name = if raw.is_empty() { raw } else {
                    let nul = raw.iter().position(|byte| *byte == 0).ok_or_else(refused)?;
                    &raw[..nul]
                };
                if watch.rules.iter().any(|rule| rule.relevant(name)) {
                    return Err(refused());
                }
                offset = end;
            }
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{fs, io::{Seek, SeekFrom, Write}, os::unix::fs::DirBuilderExt};
        struct Fixture(std::path::PathBuf);
        impl Fixture {
            fn new() -> Self {
                let path = std::env::temp_dir().join(format!("sqlite-source-history-{}", uuid::Uuid::new_v4()));
                fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
                Self(path)
            }
            fn root(&self) -> Root { Root::open(&self.0).unwrap() }
        }
        impl Drop for Fixture { fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); } }

        #[test]
        fn unknown_or_remote_namespace_cannot_be_certified_by_a_watch() {
            assert!(local_namespace(&File::open("/proc").unwrap()).is_err());
        }

        #[test]
        fn file_hardlink_roundtrip_permanently_refuses() {
            let fixture = Fixture::new();
            let file = File::create(fixture.0.join("body")).unwrap();
            let history = History::new(&fixture.root()).unwrap();
            history.watch_file(&file).unwrap();
            let alias = fixture.0.with_extension("outside-alias");
            fs::hard_link(fixture.0.join("body"), &alias).unwrap();
            fs::remove_file(alias).unwrap();
            assert!(history.fence().is_err());
            assert!(history.fence().is_err());
        }

        #[test]
        fn ancestor_selected_directory_rename_return_refuses() {
            let fixture = Fixture::new();
            let history = History::new(&fixture.root()).unwrap();
            let moved = fixture.0.with_extension("moved");
            fs::rename(&fixture.0, &moved).unwrap();
            fs::rename(&moved, &fixture.0).unwrap();
            assert!(history.fence().is_err());
            assert!(history.fence().is_err());
        }

        #[test]
        fn same_bytes_write_revert_and_new_watch_cannot_rebase() {
            let fixture = Fixture::new();
            let mut file = File::create(fixture.0.join("body")).unwrap();
            file.write_all(b"original").unwrap();
            let history = History::new(&fixture.root()).unwrap();
            history.watch_file(&file).unwrap();
            file.seek(SeekFrom::Start(0)).unwrap(); file.write_all(b"different").unwrap();
            file.seek(SeekFrom::Start(0)).unwrap(); file.write_all(b"original").unwrap(); file.set_len(8).unwrap();
            assert!(history.watch_file(&file).is_err());
            assert!(history.fence().is_err());
            assert_eq!(fs::read(fixture.0.join("body")).unwrap(), b"original");
        }

        #[test]
        fn ignored_root_logs_and_unrelated_ancestor_sibling_remain_clean() {
            let fixture = Fixture::new();
            let history = History::new(&fixture.root()).unwrap();
            for name in ["logs", "logging", "search_index"] {
                fs::create_dir(fixture.0.join(name)).unwrap();
                fs::write(fixture.0.join(name).join("activity"), b"ignored").unwrap();
                fs::remove_dir_all(fixture.0.join(name)).unwrap();
            }
            let sibling = fixture.0.with_extension("unrelated");
            fs::write(&sibling, b"not selected").unwrap(); fs::remove_file(sibling).unwrap();
            history.fence().unwrap();
        }

        #[test]
        fn traversed_directory_watches_children_before_enumeration() {
            let fixture = Fixture::new();
            fs::create_dir(fixture.0.join("nested")).unwrap();
            let history = History::new(&fixture.root()).unwrap();
            history.watch_directory(&File::open(fixture.0.join("nested")).unwrap(), "nested").unwrap();
            fs::write(fixture.0.join("nested/new"), b"new").unwrap();
            fs::remove_file(fixture.0.join("nested/new")).unwrap();
            assert!(history.fence().is_err());
        }

        #[test]
        fn injected_queue_overflow_and_watch_loss_refuse() {
            let fixture = Fixture::new();
            for mask in [libc::IN_Q_OVERFLOW, libc::IN_IGNORED] {
                let history = History::new(&fixture.root()).unwrap();
                let mut state = history.state.lock().unwrap();
                let event = libc::inotify_event { wd: -1, mask, cookie: 0, len: 0 };
                let bytes = unsafe { std::slice::from_raw_parts((&event as *const libc::inotify_event).cast::<u8>(), std::mem::size_of_val(&event)) };
                let result = state.inspect_bytes(bytes);
                assert!(result.is_err());
                // Same production poison-on-inspection-error transition.
                if result.is_err() { state.poisoned = true; }
                drop(state);
                assert!(history.fence().is_err());
            }
        }
    }
}
