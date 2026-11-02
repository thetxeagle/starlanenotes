//! Exclusive page-owned replay blobs. No recovery journal or cleanup authority.
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Component, Path},
};
fn invalid() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "Asset path or retained blob differs",
    )
}
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::MetadataExt,
        },
    };
    #[cfg(test)]
    thread_local! {pub(super) static NEXT_BLOB_ID:std::cell::RefCell<Option<String>>=const{std::cell::RefCell::new(None)};}
    fn component(s: &str) -> bool {
        !s.is_empty()
            && s != "."
            && s != ".."
            && !s.contains('/')
            && !s.contains('\\')
            && !s.contains('\0')
    }
    fn open(dir: &File, name: &str, flags: i32) -> std::io::Result<File> {
        let n = CString::new(name).map_err(|_| invalid())?;
        // SAFETY: a retained live directory FD and NUL-terminated single component;
        // openat adds no-follow and transfers its successful new descriptor below.
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                n.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            // SAFETY: this successful descriptor has exactly one owning File.
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn directory(dir: &File, name: &str, create: bool) -> std::io::Result<File> {
        if !component(name) {
            return Err(invalid());
        }
        if create {
            let n = CString::new(name).map_err(|_| invalid())?;
            // SAFETY: retained directory FD, validated component, fixed owner-only mode.
            let rc = unsafe { libc::mkdirat(dir.as_raw_fd(), n.as_ptr(), 0o700) };
            if rc < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(std::io::Error::last_os_error());
            }
            dir.sync_all()?;
        }
        open(dir, name, libc::O_RDONLY | libc::O_DIRECTORY)
    }
    fn root(path: &Path) -> std::io::Result<File> {
        let p = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let mut dir = File::open("/")?;
        for part in p.components() {
            match part {
                Component::RootDir => {}
                Component::Normal(n) => {
                    dir = directory(&dir, n.to_str().ok_or_else(invalid)?, false)?
                }
                _ => return Err(invalid()),
            }
        }
        Ok(dir)
    }
    fn same(a: &File, b: &File) -> std::io::Result<bool> {
        let a = a.metadata()?;
        let b = b.metadata()?;
        Ok(a.dev() == b.dev() && a.ino() == b.ino())
    }
    pub struct HeldBlob {
        lease: Option<crate::storage::attachment_upload::admission::WriterLease>,
        root_path: std::path::PathBuf,
        root: File,
        dir: File,
        file: File,
        parts: Vec<String>,
        path: String,
        sha256: String,
        length: i64,
    }
    impl HeldBlob {
        pub fn path(&self) -> &str {
            &self.path
        }
        pub fn sha256(&self) -> &str {
            &self.sha256
        }
        pub fn length(&self) -> i64 {
            self.length
        }
        pub fn verify(&self) -> std::io::Result<()> {
            if let Some(lease)=&self.lease {lease.verify_replay_blob(&self.root_path,&self.path)?;}
            let root = root(&self.root_path)?;
            if !same(&root, &self.root)? {
                return Err(invalid());
            }
            let mut dir = root;
            for part in &self.parts[..self.parts.len() - 1] {
                dir = directory(&dir, part, false)?;
            }
            if !same(&dir, &self.dir)? {
                return Err(invalid());
            }
            let file = open(
                &dir,
                self.parts.last().ok_or_else(invalid)?,
                libc::O_RDONLY | libc::O_NONBLOCK,
            )?;
            if !same(&file, &self.file)?
                || !file.metadata()?.is_file()
                || file.metadata()?.len() != self.length as u64
            {
                return Err(invalid());
            }
            let mut bytes = Vec::new();
            file.take(self.length as u64 + 1).read_to_end(&mut bytes)?;
            if bytes.len() as i64 != self.length
                || hex::encode(Sha256::digest(&bytes)) != self.sha256
            {
                return Err(invalid());
            }
            Ok(())
        }
    }
    pub fn create(
        root_path: &Path,
        slug: &str,
        document: &str,
        extension: &str,
        bytes: &[u8],
    ) -> std::io::Result<HeldBlob> {
        create_inner(root_path,slug,document,extension,bytes,None)
    }
    pub fn create_with_lease(root_path:&Path,slug:&str,document:&str,extension:&str,bytes:&[u8],lease:crate::storage::attachment_upload::admission::WriterLease)->std::io::Result<HeldBlob> {
        create_inner(root_path,slug,document,extension,bytes,Some(lease))
    }
    fn create_inner(root_path:&Path,slug:&str,document:&str,extension:&str,bytes:&[u8],lease:Option<crate::storage::attachment_upload::admission::WriterLease>)->std::io::Result<HeldBlob> {
        #[cfg(target_os="linux")]
        let _family=crate::storage::native_family_fence::Creation::shared(root_path)?;
        if let Some(held)=&lease {held.validate_replay_payload(root_path,document,bytes)?;}
        if !component(slug)
            || !component(document)
            || extension.is_empty()
            || extension.len() > 16
            || !extension.bytes().all(|b| b.is_ascii_alphanumeric())
            || bytes.is_empty()
            || bytes.len() > 100 * 1024 * 1024
        {
            return Err(invalid());
        }
        #[cfg(test)]
        let id = NEXT_BLOB_ID
            .with(|slot| slot.borrow_mut().take())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        #[cfg(not(test))]
        let id = uuid::Uuid::new_v4().to_string();
        let filename = format!("{}.{}", id, extension);
        let parts = vec![
            "spaces".into(),
            slug.into(),
            "attachments".into(),
            document.into(),
            filename,
        ];
        let held_root = root(root_path)?;
        let mut dir = held_root.try_clone()?;
        for part in &parts[..parts.len() - 1] {
            dir = directory(&dir, part, true)?;
        }
        let mut file = open(
            &dir,
            parts.last().unwrap(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        )?;
        if let Some(held)=&lease {held.bind_replay_blob_original(root_path,&parts.join("/"),&dir,&file,bytes)?;}
        #[cfg(test)]
        if lease.is_some() && std::env::var("STARLANE_DISPOSAL_TEST_CHECKPOINT").as_deref()==Ok("replay-blob-partial") {
            file.write_all(&bytes[..bytes.len()/2])?;file.sync_all()?;
            std::fs::write(root_path.join("checkpoint-ready"),b"replay-blob-partial")?;loop {std::thread::park();}
        }
        #[cfg(test)]
        if lease.is_some() && std::env::var("STARLANE_DISPOSAL_TEST_CHECKPOINT").as_deref()==Ok("replay-blob-origin-durable") {
            std::fs::write(root_path.join("checkpoint-ready"),b"replay-blob-origin-durable")?;loop {std::thread::park();}
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        dir.sync_all()?;
        let blob = HeldBlob {
            lease,
            root_path: root_path.to_path_buf(),
            root: held_root,
            dir,
            file,
            path: parts.join("/"),
            parts,
            sha256: hex::encode(Sha256::digest(bytes)),
            length: bytes.len() as i64,
        };
        blob.verify()?;
        Ok(blob)
    }
}
#[cfg(target_os = "linux")]
pub use linux::{create,create_with_lease};
#[cfg(not(target_os = "linux"))]
pub struct HeldBlob {
    path: String,
    sha256: String,
    length: i64,
}
#[cfg(not(target_os = "linux"))]
impl HeldBlob {
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn length(&self) -> i64 {
        self.length
    }
    pub fn verify(&self) -> std::io::Result<()> {
        Err(invalid())
    }
}
#[cfg(not(target_os = "linux"))]
pub fn create(_: &Path, _: &str, _: &str, _: &str, _: &[u8]) -> std::io::Result<HeldBlob> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Exclusive replay blob installation requires Linux directory-FD support",
    ))
}
/// Exact bounded no-follow read; caller must prove the current scoped published source.
pub(crate) fn read_canonical(path: &str) -> std::io::Result<Vec<u8>> {
    super::journal_fs::source_bytes(&super::data_dir(), path)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    fn fixture() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("asset-blob-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
    #[test]
    fn exclusive_install_retains_original_on_forced_uuid_collision() {
        let root = fixture();
        let id = uuid::Uuid::new_v4().to_string();
        linux::NEXT_BLOB_ID.with(|s| *s.borrow_mut() = Some(id.clone()));
        let b = create(&root, "slug", "legacy:page", "bin", b"ORIGINAL").unwrap();
        linux::NEXT_BLOB_ID.with(|s| *s.borrow_mut() = Some(id));
        assert!(create(&root, "slug", "legacy:page", "bin", b"OVERWRITE").is_err());
        b.verify().unwrap();
        assert_eq!(std::fs::read(root.join(b.path())).unwrap(), b"ORIGINAL");
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn nofollow_parent_and_retained_inode_replacement_are_denied() {
        let root = fixture();
        let outside = fixture();
        std::fs::create_dir_all(root.join("spaces/slug/attachments")).unwrap();
        std::os::unix::fs::symlink(&outside, root.join("spaces/slug/attachments/doc")).unwrap();
        assert!(create(&root, "slug", "doc", "bin", b"BLOCK").is_err());
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        std::fs::remove_file(root.join("spaces/slug/attachments/doc")).unwrap();
        let b = create(&root, "slug", "doc", "bin", b"ORIGINAL").unwrap();
        std::fs::rename(
            root.join("spaces/slug/attachments/doc"),
            root.join("retained"),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("spaces/slug/attachments/doc")).unwrap();
        std::fs::write(root.join(b.path()), b"REPLACED").unwrap();
        assert!(b.verify().is_err());
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }
}
