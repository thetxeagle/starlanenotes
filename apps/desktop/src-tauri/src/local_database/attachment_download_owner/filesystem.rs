use serde::{Deserialize, Serialize};
use std::ffi::CString;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Identity {
    pub device: u64,
    pub inode: u64,
}
pub(super) fn identity(f: &File) -> io::Result<Identity> {
    let m = f.metadata()?;
    Ok(Identity {
        device: m.dev(),
        inode: m.ino(),
    })
}
fn name(s: &str) -> io::Result<CString> {
    if s.is_empty() || s == "." || s == ".." || s.contains('/') {
        return Err(io::Error::other("Invalid owned child name"));
    }
    CString::new(s).map_err(|_| io::Error::other("Invalid owned child name"))
}
pub(super) fn open_at(parent: &File, child: &str, flags: i32) -> io::Result<File> {
    let child = name(child)?;
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            child.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o600,
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
#[derive(Clone)]
struct Node {
    parent: Arc<File>,
    name: String,
    held: Arc<File>,
    id: Identity,
}
#[derive(Clone)]
pub(super) struct Directory {
    pub held: Arc<File>,
    pub path: PathBuf,
    nodes: Vec<Node>,
}
impl Directory {
    pub fn capture(path: &Path) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(io::Error::other("Original native root must be absolute"));
        }
        let fd = unsafe {
            libc::open(
                c"/".as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut held = Arc::new(unsafe { File::from_raw_fd(fd) });
        let mut nodes = Vec::new();
        let mut walked = PathBuf::from("/");
        for part in path.components() {
            match part {
                Component::RootDir => {}
                Component::Normal(v) => {
                    let child = v
                        .to_str()
                        .ok_or_else(|| {
                            io::Error::other("Original native path encoding unsupported")
                        })?
                        .to_owned();
                    let next =
                        Arc::new(open_at(&held, &child, libc::O_RDONLY | libc::O_DIRECTORY)?);
                    let id = identity(&next)?;
                    nodes.push(Node {
                        parent: held,
                        name: child.clone(),
                        held: next.clone(),
                        id,
                    });
                    held = next;
                    walked.push(child);
                }
                _ => {
                    return Err(io::Error::other(
                        "Original native path must contain only normal components",
                    ))
                }
            }
        }
        let result = Self {
            held,
            path: walked,
            nodes,
        };
        result.fence()?;
        Ok(result)
    }
    pub fn fence(&self) -> io::Result<()> {
        for n in &self.nodes {
            if identity(&n.held)? != n.id
                || identity(&open_at(
                    &n.parent,
                    &n.name,
                    libc::O_RDONLY | libc::O_DIRECTORY,
                )?)? != n.id
            {
                return Err(io::Error::other("Original native directory changed"));
            }
        }
        Ok(())
    }
    pub fn child(&self, child: &str, create: bool) -> io::Result<Self> {
        self.fence()?;
        let held = match open_at(&self.held, child, libc::O_RDONLY | libc::O_DIRECTORY) {
            Ok(f) => f,
            Err(e) if create && e.kind() == io::ErrorKind::NotFound => {
                let n = name(child)?;
                let rc = unsafe { libc::mkdirat(self.held.as_raw_fd(), n.as_ptr(), 0o700) };
                if rc < 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
                    return Err(io::Error::last_os_error());
                }
                self.held.sync_all()?;
                open_at(&self.held, child, libc::O_RDONLY | libc::O_DIRECTORY)?
            }
            Err(e) => return Err(e),
        };
        let held = Arc::new(held);
        let id = identity(&held)?;
        let mut nodes = self.nodes.clone();
        nodes.push(Node {
            parent: self.held.clone(),
            name: child.into(),
            held: held.clone(),
            id,
        });
        let result = Self {
            held,
            path: self.path.join(child),
            nodes,
        };
        result.fence()?;
        Ok(result)
    }
    pub fn open(&self, child: &str, flags: i32) -> io::Result<File> {
        self.fence()?;
        let f = open_at(&self.held, child, flags)?;
        self.fence()?;
        Ok(f)
    }
    pub fn file_identity(&self, child: &str) -> io::Result<Option<Identity>> {
        self.fence()?;
        let child = name(child)?;
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        // Closing any additional same-inode FD releases this process's POSIX
        // SQLite locks. Inspect the held-directory entry without opening it.
        let rc = unsafe {
            libc::fstatat(
                self.held.as_raw_fd(),
                child.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if rc != 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::NotFound {
                Ok(None)
            } else {
                Err(error)
            };
        }
        let stat = unsafe { stat.assume_init() };
        if stat.st_mode & libc::S_IFMT != libc::S_IFREG || stat.st_nlink != 1 {
            return Err(io::Error::other(
                "Native database child is not an independent regular file",
            ));
        }
        self.fence()?;
        Ok(Some(Identity {
            device: stat.st_dev,
            inode: stat.st_ino,
        }))
    }
    pub fn unlink(&self, child: &str) -> io::Result<()> {
        self.fence()?;
        let n = name(child)?;
        if unsafe { libc::unlinkat(self.held.as_raw_fd(), n.as_ptr(), 0) } < 0 {
            return Err(io::Error::last_os_error());
        }
        self.held.sync_all()?;
        self.fence()
    }
    pub fn proc_name(&self, child: &str) -> io::Result<CString> {
        name(child)?;
        CString::new(format!("/proc/self/fd/{}/{}", self.held.as_raw_fd(), child))
            .map_err(|_| io::Error::other("Invalid native child"))
    }
}
