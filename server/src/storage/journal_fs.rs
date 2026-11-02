//! Shared Linux no-follow directory-FD journal I/O. Callers own manifest-kind
//! validation and SQL receipt policy; namespaces never mix recovery evidence.
use super::journal::StagedDocument;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{Read, Write},
    path::{Component, Path},
};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct FileDescriptor {
    pub op_id: String,
    pub space_id: String,
    pub document_id: String,
    pub path: String,
    pub revision: String,
    pub sha256: String,
    pub length: i64,
    pub version: u32,
}
impl From<&StagedDocument> for FileDescriptor {
    fn from(m: &StagedDocument) -> Self {
        Self {
            op_id: m.op_id.clone(),
            space_id: m.space_id.clone(),
            document_id: m.document_id.clone(),
            path: m.path.clone(),
            revision: m.revision.clone(),
            sha256: m.sha256.clone(),
            length: m.length,
            version: m.version,
        }
    }
}
/// Read-only restoration requirements, never installation or ledger authority.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InstallProofInspection {
    pub op_id: String,
    pub temp: String,
    pub original: (u64, u64),
    pub raw: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InspectedPath {
    pub relative: String,
    pub observed: (u64, u64),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InstallInspection {
    pub proof: Option<InstallProofInspection>,
    pub paths: Vec<InspectedPath>,
    /// Original proof requires these paths to be one owned physical group.
    /// Observation here does not authenticate a backup's source custody.
    pub required_group: Vec<String>,
    pub installed_present: bool,
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
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
    fn fail(msg: &str) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::InvalidData, msg)
    }
    fn name(s: &str) -> std::io::Result<CString> {
        CString::new(s).map_err(|_| fail("NUL in path"))
    }
    fn open(parent: &File, n: &str, flags: i32) -> std::io::Result<File> {
        let n = name(n)?;
        // SAFETY: live directory FD, NUL-terminated name, fixed mode for O_CREAT;
        // ownership of a successful unique returned descriptor transfers to File.
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                n.as_ptr(),
                flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                0o600,
            )
        };
        if fd < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn directory(parent: &File, n: &str, create: bool) -> std::io::Result<File> {
        if create {
            let n = name(n)?;
            let rc = unsafe { libc::mkdirat(parent.as_raw_fd(), n.as_ptr(), 0o700) };
            if rc < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(std::io::Error::last_os_error());
            }
            parent.sync_all()?;
        }
        open(parent, n, libc::O_RDONLY | libc::O_DIRECTORY)
    }
    fn root(path: &Path) -> std::io::Result<File> {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let mut fd = File::open("/")?;
        for component in absolute.components() {
            match component {
                Component::RootDir => {}
                Component::Normal(n) => {
                    fd = directory(&fd, n.to_str().ok_or_else(|| fail("Non UTF8 root"))?, false)?
                }
                _ => return Err(fail("Unsafe root")),
            }
        }
        Ok(fd)
    }
    fn relative(path: &str) -> std::io::Result<Vec<&str>> {
        if !super::super::is_safe_relative_path(Path::new(path)) {
            return Err(fail("Unsafe relative path"));
        }
        Ok(path.split('/').collect())
    }
    fn stage_dir(root: &File, namespace: &str, op: &str, create: bool) -> std::io::Result<File> {
        if Uuid::parse_str(op).is_err() {
            return Err(fail("Invalid operation ID"));
        }
        if !matches!(
            namespace,
            ".storage-journal" | ".publication-journal" | ".canonical-rewrite-journal"
        ) {
            return Err(fail("Invalid journal namespace"));
        }
        let journal = directory(root, namespace, create)?;
        if create {
            let n = name(op)?;
            if unsafe { libc::mkdirat(journal.as_raw_fd(), n.as_ptr(), 0o700) } < 0 {
                return Err(std::io::Error::last_os_error());
            }
            journal.sync_all()?;
        }
        directory(&journal, op, false)
    }
    fn write(dir: &File, n: &str, bytes: &[u8]) -> std::io::Result<()> {
        let mut f = open(dir, n, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        dir.sync_all()
    }
    fn read(dir: &File, n: &str, limit: u64) -> std::io::Result<Vec<u8>> {
        let f = open(dir, n, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let metadata = f.metadata()?;
        if !metadata.is_file() || metadata.len() > limit {
            return Err(fail("Invalid stage file"));
        }
        let mut b = Vec::new();
        f.take(limit + 1).read_to_end(&mut b)?;
        if b.len() as u64 > limit {
            return Err(fail("Oversized stage file"));
        }
        Ok(b)
    }
    pub fn stage_file(
        path: &Path,
        namespace: &str,
        m: &FileDescriptor,
        manifest_bytes: &[u8],
        bytes: &[u8],
    ) -> std::io::Result<()> {
        #[cfg(target_os="linux")]
        let _family=crate::storage::native_family_fence::Creation::shared(path)?;
        relative(&m.path)?;
        let r = root(path)?;
        let proc_fd = std::fs::metadata(format!("/proc/self/fd/{}", r.as_raw_fd()))?;
        if proc_fd.ino() != r.metadata()?.ino() || proc_fd.dev() != r.metadata()?.dev() {
            return Err(fail("Kernel directory FD backend unavailable"));
        }
        let dir = stage_dir(&r, namespace, &m.op_id, true)?;
        write(&dir, "payload", bytes)?;
        write(&dir, "manifest.json", manifest_bytes)?;
        Ok(())
    }
    /// New replay-only path: original empty payload ownership is durable before result bytes.
    pub fn stage_replay_file_with_lease(path:&Path,namespace:&str,m:&FileDescriptor,manifest_bytes:&[u8],bytes:&[u8],lease:&super::super::attachment_upload::admission::WriterLease)->std::io::Result<()> {
        #[cfg(target_os="linux")]
        let _family=crate::storage::native_family_fence::Creation::shared(path)?;
        if namespace!=".canonical-rewrite-journal" {return Err(fail("Replay lease cannot own another journal namespace"));}
        relative(&m.path)?;lease.verify()?;lease.matches_root(path)?;
        let r=root(path)?;let dir=stage_dir(&r,namespace,&m.op_id,true)?;
        let mut payload=open(&dir,"payload",libc::O_WRONLY|libc::O_CREAT|libc::O_EXCL)?;
        payload.sync_all()?;dir.sync_all()?;
        lease.bind_replay_rewrite_original(path,m,manifest_bytes,&dir,&payload)?;
        lease.replay_rewrite_checkpoint("replay-rewrite-origin-durable",path);
        #[cfg(test)] if std::env::var("STARLANE_DISPOSAL_TEST_CHECKPOINT").as_deref()==Ok("replay-rewrite-partial") {
            payload.write_all(&bytes[..bytes.len()/2])?;payload.sync_all()?;dir.sync_all()?;
            lease.replay_rewrite_checkpoint("replay-rewrite-partial",path);
        }
        payload.write_all(bytes)?;payload.sync_all()?;dir.sync_all()?;
        lease.verify()?;write(&dir,"manifest.json",manifest_bytes)?;
        lease.replay_rewrite_checkpoint("replay-rewrite-manifest-durable",path);Ok(())
    }
    pub fn file_manifest(
        path: &Path,
        namespace: &str,
        op: &str,
        version: u32,
    ) -> std::io::Result<FileDescriptor> {
        let r = root(path)?;
        let dir = stage_dir(&r, namespace, op, false)?;
        let m: FileDescriptor = serde_json::from_slice(&read(&dir, "manifest.json", 65536)?)
            .map_err(std::io::Error::other)?;
        if m.version != version || m.op_id != op || m.length < 0 || m.length > 6_000_000 {
            return Err(fail("Invalid manifest identity"));
        }
        relative(&m.path)?;
        Ok(m)
    }
    fn verified(dir: &File, m: &FileDescriptor) -> std::io::Result<File> {
        let mut f = open(dir, "payload", libc::O_RDONLY | libc::O_NONBLOCK)?;
        let md = f.metadata()?;
        if !md.is_file() || md.len() != m.length as u64 || md.len() > 6_000_000 {
            return Err(fail("Invalid payload inode"));
        }
        let mut bytes = Vec::new();
        (&mut f).take(6_000_001).read_to_end(&mut bytes)?;
        if bytes.len() as i64 != m.length || digest(&bytes) != m.sha256 {
            return Err(fail("Stage checksum mismatch"));
        }
        Ok(f)
    }
    pub(crate) fn validate_descriptor(file: &mut File, m: &FileDescriptor) -> std::io::Result<()> {
        use std::io::{Seek, SeekFrom};
        let md = file.metadata()?;
        if !md.is_file() || md.len() != m.length as u64 || md.len() > 6_000_000 {
            return Err(fail("Invalid install inode"));
        }
        file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        file.take(6_000_001).read_to_end(&mut bytes)?;
        if bytes.len() as i64 != m.length || digest(&bytes) != m.sha256 {
            return Err(fail("Install checksum changed"));
        }
        Ok(())
    }
    #[cfg(test)]
    thread_local! {static PUBLICATION_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = std::cell::RefCell::new(None);}
    #[cfg(test)]
    pub fn hook(f: impl FnOnce() + 'static) {
        PUBLICATION_HOOK.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }
    #[cfg(test)]
    thread_local! {static FINAL_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = std::cell::RefCell::new(None);}
    #[cfg(test)]
    pub fn final_hook(f: impl FnOnce() + 'static) {
        FINAL_HOOK.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }

    #[derive(Serialize, Deserialize)]
    struct InstallProof {
        op_id: String,
        temp: String,
        dev: u64,
        ino: u64,
    }
    #[cfg(test)]
    thread_local! {static ANCHOR_HOOK: std::cell::RefCell<Option<(bool,Box<dyn FnOnce()>)>> = std::cell::RefCell::new(None);}
    #[cfg(test)]
    pub fn anchor_fault(during_copy: bool, f: impl FnOnce() + 'static) {
        ANCHOR_HOOK.with(|h| *h.borrow_mut() = Some((during_copy, Box::new(f))));
    }
    pub(crate) fn link_descriptor(source: &File, parent: &File, leaf: &str) -> std::io::Result<()> {
        let src = name(&format!("/proc/self/fd/{}", source.as_raw_fd()))?;
        let dest = name(leaf)?;
        if unsafe {
            libc::linkat(
                libc::AT_FDCWD,
                src.as_ptr(),
                parent.as_raw_fd(),
                dest.as_ptr(),
                libc::AT_SYMLINK_FOLLOW,
            )
        } < 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
    // Serialization only; no path access, proof mint eligibility or fence bypass.
    pub(crate) fn encode_install_proof(
        m: &FileDescriptor,
        temp: &str,
        anchor: &mut File,
    ) -> std::io::Result<Vec<u8>> {
        if m.version != 1
            || Uuid::parse_str(&m.op_id)
                .map(|id| id.to_string() != m.op_id)
                .unwrap_or(true)
        {
            return Err(fail("Invalid new proof operation"));
        }
        if !temp.starts_with("anchor-") || Uuid::parse_str(&temp[7..]).is_err() {
            return Err(fail("Invalid new anchor name"));
        }
        validate_descriptor(anchor, m)?;
        let md = anchor.metadata()?;
        serde_json::to_vec(&InstallProof {
            op_id: m.op_id.clone(),
            temp: temp.into(),
            dev: md.dev(),
            ino: md.ino(),
        })
        .map_err(std::io::Error::other)
    }
    fn install_anchor(
        path: &Path,
        namespace: &str,
        dir: &File,
        m: &FileDescriptor,
        payload: &mut File,
    ) -> std::io::Result<File> {
        use std::io::{Seek, SeekFrom};
        super::super::restoration::fence(path)?;
        let (proof, proof_bytes) = match read(dir, "install-proof.json", 65536) {
            Ok(bytes) => {
                let proof = serde_json::from_slice::<InstallProof>(&bytes)
                    .map_err(std::io::Error::other)?;
                (proof, bytes)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if !super::super::restoration::fresh_install_allowed(path, namespace, &m.op_id)? {
                    return Err(fail("Classified restoration operation cannot create a new proof"));
                }
                // A stable anchor with no durable proof is not ours to replace.
                match open(dir, "install-anchor", libc::O_RDONLY | libc::O_NONBLOCK) {
                    Ok(_) => return Err(fail("Unowned install anchor")),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e),
                }
                let temp = format!("anchor-{}", Uuid::new_v4());
                let mut file = open(dir, &temp, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)?;
                payload.seek(SeekFrom::Start(0))?;
                #[cfg(test)]
                let split =
                    ANCHOR_HOOK.with(|h| h.borrow().as_ref().is_some_and(|(during, _)| *during));
                #[cfg(test)]
                if split {
                    let mut first = [0u8; 6];
                    let count = payload.read(&mut first)?;
                    file.write_all(&first[..count])?;
                    file.sync_all()?;
                    ANCHOR_HOOK.with(|h| {
                        if let Some((_, f)) = h.borrow_mut().take() {
                            f()
                        }
                    });
                }
                std::io::copy(payload, &mut file)?;
                validate_descriptor(&mut file, m)?;
                file.sync_all()?;
                dir.sync_all()?;
                let md = file.metadata()?;
                let proof = InstallProof {
                    op_id: m.op_id.clone(),
                    temp,
                    dev: md.dev(),
                    ino: md.ino(),
                };
                // Stable proof itself appears only after a fully synced random
                // temp file. Partial temp/proof files are retained, never reused.
                let proof_temp = format!("proof-{}", Uuid::new_v4());
                let proof_bytes = serde_json::to_vec(&proof).map_err(std::io::Error::other)?;
                write(dir, &proof_temp, &proof_bytes)?;
                let proof_file = open(dir, &proof_temp, libc::O_RDONLY | libc::O_NONBLOCK)?;
                link_descriptor(&proof_file, dir, "install-proof.json")?;
                dir.sync_all()?;
                (proof, proof_bytes)
            }
            Err(e) => return Err(e),
        };
        if proof.op_id != m.op_id
            || !proof.temp.starts_with("anchor-")
            || Uuid::parse_str(&proof.temp[7..]).is_err()
        {
            return Err(fail("Invalid install proof"));
        }
        let mut source = open(dir, &proof.temp, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let source_md = source.metadata()?;
        if !super::super::restoration::owns(
            path,
            namespace,
            &m.op_id,
            &proof_bytes,
            &format!("{namespace}/{}/{}", m.op_id, proof.temp),
            (proof.dev, proof.ino),
            &source,
        )? {
            return Err(fail("Install source identity changed"));
        }
        validate_descriptor(&mut source, m)?;
        #[cfg(test)]
        ANCHOR_HOOK.with(|h| {
            if let Some((_, f)) = h.borrow_mut().take() {
                f()
            }
        });
        if let Err(e) = link_descriptor(&source, dir, "install-anchor") {
            if e.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(e);
            }
        }
        let mut stable = open(dir, "install-anchor", libc::O_RDONLY | libc::O_NONBLOCK)?;
        let md = stable.metadata()?;
        if md.dev() != source_md.dev()
            || md.ino() != source_md.ino()
            || !super::super::restoration::owns(
                path,
                namespace,
                &m.op_id,
                &proof_bytes,
                &format!("{namespace}/{}/install-anchor", m.op_id),
                (proof.dev, proof.ino),
                &stable,
            )?
        {
            return Err(fail("Unowned install anchor collision"));
        }
        validate_descriptor(&mut stable, m)?;
        dir.sync_all()?;
        super::super::restoration::fence(path)?;
        Ok(stable)
    }
    pub fn publish_file(path: &Path, namespace: &str, m: &FileDescriptor) -> std::io::Result<()> {
        #[cfg(target_os="linux")]
        let _family=crate::storage::native_family_fence::Creation::shared(path)?;
        super::super::restoration::fence(path)?;
        if file_manifest(path, namespace, &m.op_id, m.version)? != *m {
            return Err(fail("Manifest mismatch"));
        }
        let r = root(path)?;
        let dir = stage_dir(&r, namespace, &m.op_id, false)?;
        let mut payload = verified(&dir, m)?;
        // The immutable payload is never linked into mutable canonical storage.
        // A separate owned anchor allows legitimate later document writes while
        // retaining original stage bytes for pending recovery/review.
        let mut anchor = install_anchor(path, namespace, &dir, m, &mut payload)?;
        validate_descriptor(&mut anchor, m)?;
        let components = relative(&m.path)?;
        let mut parent = r.try_clone()?;
        for c in &components[..components.len() - 1] {
            parent = directory(&parent, c, true)?;
        }
        let leaf = components.last().ok_or_else(|| fail("Missing leaf"))?;
        #[cfg(test)]
        PUBLICATION_HOOK.with(|h| {
            if let Some(f) = h.borrow_mut().take() {
                f()
            }
        });
        let src = name(&format!("/proc/self/fd/{}", anchor.as_raw_fd()))?;
        let dest = name(leaf)?;
        // Resolve the verified HELD descriptor through Linux procfs, not a stage
        // leaf that can be exchanged after checksum verification. This explicit
        // procfs link follows only the kernel FD reference, not storage symlinks.
        // linkat never replaces an existing destination; payload remains anchor.
        let rc = unsafe {
            libc::linkat(
                libc::AT_FDCWD,
                src.as_ptr(),
                parent.as_raw_fd(),
                dest.as_ptr(),
                libc::AT_SYMLINK_FOLLOW,
            )
        };
        if rc < 0 && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists {
            return Err(std::io::Error::last_os_error());
        }
        let mut existing = open(&parent, leaf, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let a = anchor.metadata()?;
        let b = existing.metadata()?;
        if !b.is_file() || a.dev() != b.dev() || a.ino() != b.ino() {
            return Err(fail("Unowned destination collision"));
        }
        validate_descriptor(&mut existing, m)?;
        existing.sync_all()?;
        parent.sync_all()?;
        // Ensure a renamed ancestor did not detach publication from its current
        // canonical path. Never follow a replacement symlink during this check.
        let mut current = root(path)?;
        for c in &components[..components.len() - 1] {
            current = directory(&current, c, false)?;
        }
        if current.metadata()?.ino() != parent.metadata()?.ino()
            || current.metadata()?.dev() != parent.metadata()?.dev()
        {
            return Err(fail("Destination directory changed"));
        }
        #[cfg(test)]
        FINAL_HOOK.with(|h| {
            if let Some(f) = h.borrow_mut().take() {
                f()
            }
        });
        let mut current_leaf = open(&current, leaf, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let c = current_leaf.metadata()?;
        if c.dev() != b.dev() || c.ino() != b.ino() {
            return Err(fail("Destination leaf changed"));
        }
        validate_descriptor(&mut current_leaf, m)?;
        // Reresolve after verification too: a late ancestor exchange must not
        // acknowledge a file installed only in a detached directory.
        let mut final_parent = root(path)?;
        for c in &components[..components.len() - 1] {
            final_parent = directory(&final_parent, c, false)?;
        }
        let final_parent_md = final_parent.metadata()?;
        if final_parent_md.dev() != parent.metadata()?.dev()
            || final_parent_md.ino() != parent.metadata()?.ino()
        {
            return Err(fail("Final destination ancestor changed"));
        }
        let mut final_leaf = open(&final_parent, leaf, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let final_md = final_leaf.metadata()?;
        if final_md.dev() != b.dev() || final_md.ino() != b.ino() {
            return Err(fail("Final destination leaf changed"));
        }
        validate_descriptor(&mut final_leaf, m)?;
        super::super::restoration::fence(path)
    }
    /// Inspect immutable bytes and declared proof requirements without rebinding,
    /// publishing, or assuming copied files retain original device/inode values.
    pub fn inspect_install(
        path: &Path, namespace: &str, m: &FileDescriptor, inspect_target: bool,
    ) -> std::io::Result<InstallInspection> {
        let same = |a: &File, b: &File| -> std::io::Result<bool> {
            let a = a.metadata()?; let b = b.metadata()?;
            Ok(a.dev() == b.dev() && a.ino() == b.ino())
        };
        let evidence = manifest_bytes(path, namespace, &m.op_id)?;
        if file_manifest(path, namespace, &m.op_id, m.version)? != *m {
            return Err(fail("Manifest mismatch"));
        }
        let r = root(path)?;
        let dir = stage_dir(&r, namespace, &m.op_id, false)?;
        let mut held = Vec::new();
        let mut absent = Vec::new();
        let prefix = format!("{namespace}/{}", m.op_id);
        let mut payload = verified(&dir, m)?;
        validate_descriptor(&mut payload, m)?;
        held.push((format!("{prefix}/payload"), payload));
        let proof = match read(&dir, "install-proof.json", 65536) {
            Ok(raw) => {
                let proof: InstallProof = serde_json::from_slice(&raw)
                    .map_err(std::io::Error::other)?;
                let object: serde_json::Value = serde_json::from_slice(&raw)
                    .map_err(std::io::Error::other)?;
                if object.as_object().is_none_or(|o| o.len() != 4)
                    || proof.op_id != m.op_id || proof.ino == 0
                    || !proof.temp.starts_with("anchor-")
                    || Uuid::parse_str(&proof.temp[7..]).is_err() {
                    return Err(fail("Invalid install proof"));
                }
                let mut temp = open(&dir, &proof.temp, libc::O_RDONLY | libc::O_NONBLOCK)?;
                validate_descriptor(&mut temp, m)?;
                held.push((format!("{prefix}/{}", proof.temp), temp));
                // A durable proof can precede linking the stable anchor.
                match open(&dir, "install-anchor", libc::O_RDONLY | libc::O_NONBLOCK) {
                    Ok(mut anchor) => {
                        validate_descriptor(&mut anchor, m)?;
                        held.push((format!("{prefix}/install-anchor"), anchor));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        absent.push(format!("{prefix}/install-anchor"));
                    }
                    Err(e) => return Err(e),
                }
                Some(InstallProofInspection {op_id: proof.op_id, temp: proof.temp,
                    original: (proof.dev, proof.ino), raw})
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                match open(&dir, "install-anchor", libc::O_RDONLY | libc::O_NONBLOCK) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        absent.push(format!("{prefix}/install-anchor"));
                    }
                    Err(e) => return Err(e),
                    Ok(_) => return Err(fail("Unowned install anchor")),
                }
                None
            }
            Err(e) => return Err(e),
        };
        let mut installed_present = false;
        if inspect_target {
            let components = relative(&m.path)?;
            let target = (|| {
                let mut parent = r.try_clone()?;
                for component in &components[..components.len()-1] {
                    parent = directory(&parent, component, false)?;
                }
                open(&parent, components.last().ok_or_else(|| fail("Missing leaf"))?,
                    libc::O_RDONLY | libc::O_NONBLOCK)
            })();
            match target {
                Ok(mut file) => {
                    if proof.is_none() { return Err(fail("Unowned destination collision")); }
                    validate_descriptor(&mut file, m)?;
                    held.push((m.path.clone(), file));
                    installed_present = true;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {absent.push(m.path.clone());}
                Err(e) => return Err(e),
            }
        }
        let mut paths = Vec::new();
        for (relative_path, file) in &mut held {
            let md = file.metadata()?;
            let mut fresh = root(path)?;
            let components = relative(relative_path)?;
            for component in &components[..components.len()-1] {
                fresh = directory(&fresh, component, false)?;
            }
            let mut leaf = open(&fresh, components.last().ok_or_else(|| fail("Missing leaf"))?,
                libc::O_RDONLY | libc::O_NONBLOCK)?;
            if !same(file, &leaf)? {return Err(fail("Inspection path changed"));}
            validate_descriptor(&mut leaf, m)?;
            paths.push(InspectedPath {relative: relative_path.clone(), observed: (md.dev(),md.ino())});
        }
        for absent_path in absent {
            let components = relative(&absent_path)?;
            let reopened = (|| {
                let mut parent = root(path)?;
                for component in &components[..components.len()-1] {
                    parent = directory(&parent, component, false)?;
                }
                open(&parent, components.last().ok_or_else(|| fail("Missing leaf"))?,
                    libc::O_RDONLY | libc::O_NONBLOCK)
            })();
            match reopened {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
                Ok(_) => return Err(fail("Inspection absent path appeared")),
            }
        }
        let fresh_root = root(path)?;
        let fresh_dir = stage_dir(&fresh_root, namespace, &m.op_id, false)?;
        if !same(&r, &fresh_root)? || !same(&dir, &fresh_dir)?
            || manifest_bytes(path, namespace, &m.op_id)? != evidence {
            return Err(fail("Inspection evidence changed"));
        }
        match &proof {
            Some(p) if read(&fresh_dir, "install-proof.json", 65536)? != p.raw =>
                return Err(fail("Inspection proof changed")),
            None => match read(&fresh_dir, "install-proof.json", 65536) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
                Ok(_) => return Err(fail("Inspection proof appeared")),
            },
            _ => {}
        }
        let required_group = if proof.is_some() {
            paths.iter().filter(|p| p.relative != format!("{prefix}/payload"))
                .map(|p| p.relative.clone()).collect()
        } else {Vec::new()};
        Ok(InstallInspection {proof, paths, required_group, installed_present})
    }
    /// Verify an already installed owned immutable inode; never recreate paths,
    /// anchors or files. Publication startup uses this after historical deletion.
    pub fn verify_file(path: &Path, namespace: &str, m: &FileDescriptor) -> std::io::Result<()> {
        if file_manifest(path, namespace, &m.op_id, m.version)? != *m {
            return Err(fail("Manifest mismatch"));
        }
        let r = root(path)?;
        let dir = stage_dir(&r, namespace, &m.op_id, false)?;
        verified(&dir, m)?;
        let proof_bytes = read(&dir, "install-proof.json", 65536)?;
        let proof: InstallProof =
            serde_json::from_slice(&proof_bytes).map_err(std::io::Error::other)?;
        if proof.op_id != m.op_id
            || !proof.temp.starts_with("anchor-")
            || Uuid::parse_str(&proof.temp[7..]).is_err()
        {
            return Err(fail("Invalid install proof"));
        }
        let mut anchor = open(&dir, "install-anchor", libc::O_RDONLY | libc::O_NONBLOCK)?;
        let anchor_md = anchor.metadata()?;
        if !super::super::restoration::owns(
            path,
            namespace,
            &m.op_id,
            &proof_bytes,
            &format!("{namespace}/{}/install-anchor", m.op_id),
            (proof.dev, proof.ino),
            &anchor,
        )? {
            return Err(fail("Install anchor identity changed"));
        }
        validate_descriptor(&mut anchor, m)?;
        let components = relative(&m.path)?;
        let mut parent = r.try_clone()?;
        for component in &components[..components.len() - 1] {
            parent = directory(&parent, component, false)?;
        }
        let leaf = components.last().ok_or_else(|| fail("Missing leaf"))?;
        let mut installed = open(&parent, leaf, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let installed_md = installed.metadata()?;
        if installed_md.dev() != anchor_md.dev()
            || installed_md.ino() != anchor_md.ino()
            || !super::super::restoration::owns(
                path,
                namespace,
                &m.op_id,
                &proof_bytes,
                &m.path,
                (proof.dev, proof.ino),
                &installed,
            )?
        {
            return Err(fail("Unowned immutable install"));
        }
        validate_descriptor(&mut installed, m)?;
        let mut current = root(path)?;
        for component in &components[..components.len() - 1] {
            current = directory(&current, component, false)?;
        }
        if current.metadata()?.dev() != parent.metadata()?.dev()
            || current.metadata()?.ino() != parent.metadata()?.ino()
        {
            return Err(fail("Immutable parent changed"));
        }
        let mut final_leaf = open(&current, leaf, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let final_md = final_leaf.metadata()?;
        if final_md.dev() != installed_md.dev()
            || final_md.ino() != installed_md.ino()
            || !super::super::restoration::owns(
                path,
                namespace,
                &m.op_id,
                &proof_bytes,
                &m.path,
                (proof.dev, proof.ino),
                &final_leaf,
            )?
        {
            return Err(fail("Immutable leaf changed"));
        }
        validate_descriptor(&mut final_leaf, m)?;
        super::super::restoration::fence(path)
    }
    /// Read-only history proof for the retained stage and owned install anchor.
    /// Unlike writer verification, historical snapshots do not require a retired
    /// canonical leaf. No files, proofs or custody bindings are repaired here.
    pub fn verify_historical_anchor(path: &Path, namespace: &str, m: &FileDescriptor) -> std::io::Result<()> {
        if file_manifest(path, namespace, &m.op_id, m.version)? != *m {
            return Err(fail("Historical manifest mismatch"));
        }
        let r = root(path)?;
        let dir = stage_dir(&r, namespace, &m.op_id, false)?;
        let manifest = read(&dir, "manifest.json", 65536)?;
        verified(&dir, m)?;
        let proof_bytes = read(&dir, "install-proof.json", 65536)?;
        let proof: InstallProof = serde_json::from_slice(&proof_bytes).map_err(std::io::Error::other)?;
        if proof.op_id != m.op_id || !proof.temp.starts_with("anchor-") || Uuid::parse_str(&proof.temp[7..]).is_err() {
            return Err(fail("Invalid historical install proof"));
        }
        let mut anchor = open(&dir, "install-anchor", libc::O_RDONLY | libc::O_NONBLOCK)?;
        let anchor_md = anchor.metadata()?;
        let relative_anchor = format!("{namespace}/{}/install-anchor", m.op_id);
        if !super::super::restoration::owns(path, namespace, &m.op_id, &proof_bytes, &relative_anchor, (proof.dev, proof.ino), &anchor)? {
            return Err(fail("Historical anchor ownership changed"));
        }
        validate_descriptor(&mut anchor, m)?;
        let fresh_root = root(path)?;
        let fresh_dir = stage_dir(&fresh_root, namespace, &m.op_id, false)?;
        let initial_root = r.metadata()?;
        let current_root = fresh_root.metadata()?;
        let initial_dir = dir.metadata()?;
        let current_dir = fresh_dir.metadata()?;
        if (initial_root.dev(), initial_root.ino()) != (current_root.dev(), current_root.ino()) ||
            (initial_dir.dev(), initial_dir.ino()) != (current_dir.dev(), current_dir.ino()) ||
            read(&fresh_dir, "manifest.json", 65536)? != manifest || read(&fresh_dir, "install-proof.json", 65536)? != proof_bytes {
            return Err(fail("Historical evidence path changed"));
        }
        let mut final_anchor = open(&fresh_dir, "install-anchor", libc::O_RDONLY | libc::O_NONBLOCK)?;
        let final_md = final_anchor.metadata()?;
        if (final_md.dev(), final_md.ino()) != (anchor_md.dev(), anchor_md.ino()) ||
            !super::super::restoration::owns(path, namespace, &m.op_id, &proof_bytes, &relative_anchor, (proof.dev, proof.ino), &final_anchor)? {
            return Err(fail("Historical anchor changed"));
        }
        validate_descriptor(&mut final_anchor, m)?;
        let hashed_payload = verified(&fresh_dir, m)?;
        // Rewalk after hashing, so detached handles cannot authorize replacement
        // evidence installed at the original pathname during those reads.
        let last_root = root(path)?;
        let last_dir = stage_dir(&last_root, namespace, &m.op_id, false)?;
        let last_root_md = last_root.metadata()?;
        let last_dir_md = last_dir.metadata()?;
        if (last_root_md.dev(), last_root_md.ino()) != (initial_root.dev(), initial_root.ino()) ||
            (last_dir_md.dev(), last_dir_md.ino()) != (initial_dir.dev(), initial_dir.ino()) ||
            read(&last_dir, "manifest.json", 65536)? != manifest || read(&last_dir, "install-proof.json", 65536)? != proof_bytes {
            return Err(fail("Historical evidence changed after hashing"));
        }
        let last_anchor = open(&last_dir, "install-anchor", libc::O_RDONLY | libc::O_NONBLOCK)?;
        let last_anchor_md = last_anchor.metadata()?;
        let last_payload = open(&last_dir, "payload", libc::O_RDONLY | libc::O_NONBLOCK)?;
        let last_payload_md = last_payload.metadata()?;
        let hashed_payload_md = hashed_payload.metadata()?;
        if (last_anchor_md.dev(), last_anchor_md.ino()) != (anchor_md.dev(), anchor_md.ino()) ||
            (last_payload_md.dev(), last_payload_md.ino()) != (hashed_payload_md.dev(), hashed_payload_md.ino()) ||
            !super::super::restoration::owns(path, namespace, &m.op_id, &proof_bytes, &relative_anchor, (proof.dev, proof.ino), &last_anchor)? {
            return Err(fail("Historical physical evidence changed after hashing"));
        }
        if file_manifest(path, namespace, &m.op_id, m.version)? != *m {
            return Err(fail("Historical manifest changed"));
        }
        super::super::restoration::fence(path)
    }
    pub fn inventory(path: &Path, namespace: &str, _pending: bool) -> std::io::Result<Vec<String>> {
        let r = root(path)?;
        let j = match directory(&r, namespace, false) {
            Ok(j) => j,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e),
        };
        // Linux procfs enumerates the already validated held FD, avoiding a new
        // path-based traversal of replaceable storage ancestors.
        std::fs::read_dir(format!("/proc/self/fd/{}", j.as_raw_fd()))?
            .map(|entry| {
                let entry = entry?;
                Ok(entry.file_name().to_string_lossy().into_owned())
            })
            .collect()
    }
    pub fn source_bytes(path: &Path, relative_path: &str) -> std::io::Result<Vec<u8>> {
        let r = root(path)?;
        let components = relative(relative_path)?;
        let mut parent = r.try_clone()?;
        for c in &components[..components.len() - 1] {
            parent = directory(&parent, c, false)?;
        }
        let leaf = components
            .last()
            .ok_or_else(|| fail("Missing source leaf"))?;
        let mut f = open(&parent, leaf, libc::O_RDONLY | libc::O_NONBLOCK)?;
        let md = f.metadata()?;
        if !md.is_file() || md.len() > 6_000_000 {
            return Err(fail("Invalid source bytes"));
        }
        let mut bytes = Vec::new();
        (&mut f).take(6_000_001).read_to_end(&mut bytes)?;
        if bytes.len() > 6_000_000 {
            return Err(fail("Source exceeds limit"));
        }
        let fresh = root(path)?;
        let mut current = fresh.try_clone()?;
        for c in &components[..components.len() - 1] {
            current = directory(&current, c, false)?;
        }
        let now = open(&current, leaf, libc::O_RDONLY | libc::O_NONBLOCK)?.metadata()?;
        if now.dev() != md.dev() || now.ino() != md.ino() {
            return Err(fail("Source path changed"));
        }
        Ok(bytes)
    }
    #[cfg(test)]
    thread_local! {static STAGE_VERIFY_HOOK:std::cell::RefCell<Option<Box<dyn FnOnce()>>>=std::cell::RefCell::new(None);}
    #[cfg(test)]
    pub fn stage_verify_hook(f: impl FnOnce() + 'static) {
        STAGE_VERIFY_HOOK.with(|h| *h.borrow_mut() = Some(Box::new(f)));
    }
    /// Pure uninstalled stage proof. Hold the payload inode and rewalk rooted
    /// no-follow directories before accepting current pathname/hash identity.
    pub fn verify_stage_file(
        path: &Path,
        namespace: &str,
        m: &FileDescriptor,
    ) -> std::io::Result<()> {
        let held_root = root(path)?;
        let held_dir = stage_dir(&held_root, namespace, &m.op_id, false)?;
        let mut payload = verified(&held_dir, m)?;
        validate_descriptor(&mut payload, m)?;
        #[cfg(test)]
        STAGE_VERIFY_HOOK.with(|h| {
            if let Some(f) = h.borrow_mut().take() {
                f()
            }
        });
        // The last checksum can read a detached inode. Resolve current rooted
        // directory/leaf ownership only AFTER hashing, with no later hash read.
        let fresh_root = root(path)?;
        let same = |a: &File, b: &File| -> std::io::Result<bool> {
            let a = a.metadata()?;
            let b = b.metadata()?;
            Ok(a.dev() == b.dev() && a.ino() == b.ino())
        };
        if !same(&held_root, &fresh_root)? {
            return Err(fail("Stage root changed"));
        }
        let fresh_dir = stage_dir(&fresh_root, namespace, &m.op_id, false)?;
        if !same(&held_dir, &fresh_dir)? {
            return Err(fail("Stage directory changed"));
        }
        let fresh_payload = open(&fresh_dir, "payload", libc::O_RDONLY | libc::O_NONBLOCK)?;
        if !same(&payload, &fresh_payload)? {
            return Err(fail("Stage payload inode changed"));
        }
        Ok(())
    }
    pub fn manifest_bytes(path: &Path, namespace: &str, op: &str) -> std::io::Result<Vec<u8>> {
        let r = root(path)?;
        let dir = stage_dir(&r, namespace, op, false)?;
        read(&dir, "manifest.json", 65536)
    }
}
#[cfg(target_os = "linux")]
pub(super) use linux::*;
#[cfg(any(not(target_os = "linux"), test))]
pub(super) fn unsupported_inventory(
    root: &Path,
    namespace: &str,
    pending: bool,
) -> std::io::Result<Vec<String>> {
    match std::fs::symlink_metadata(root.join(namespace)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !pending => Ok(vec![]),
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Existing journal evidence requires Linux recovery backend",
        )),
    }
}
#[cfg(not(target_os = "linux"))]
mod unsupported {
    use super::*;
    fn error() -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Journal requires Linux directory-FD backend",
        )
    }
    pub fn stage_file(
        _: &Path,
        _: &str,
        _: &FileDescriptor,
        _: &[u8],
        _: &[u8],
    ) -> std::io::Result<()> {
        Err(error())
    }
    pub fn file_manifest(_: &Path, _: &str, _: &str, _: u32) -> std::io::Result<FileDescriptor> {
        Err(error())
    }
    pub fn source_bytes(_: &Path, _: &str) -> std::io::Result<Vec<u8>> {
        Err(error())
    }
    pub fn manifest_bytes(_: &Path, _: &str, _: &str) -> std::io::Result<Vec<u8>> {
        Err(error())
    }
    pub fn publish_file(_: &Path, _: &str, _: &FileDescriptor) -> std::io::Result<()> {
        Err(error())
    }
    pub fn verify_stage_file(_: &Path, _: &str, _: &FileDescriptor) -> std::io::Result<()> {
        Err(error())
    }
    pub fn inspect_install(_: &Path, _: &str, _: &FileDescriptor, _: bool)
        -> std::io::Result<InstallInspection> {Err(error())}
    pub fn verify_historical_anchor(_: &Path, _: &str, _: &FileDescriptor) -> std::io::Result<()> {
        Err(error())
    }
    pub fn verify_file(_: &Path, _: &str, _: &FileDescriptor) -> std::io::Result<()> {
        Err(error())
    }
    pub fn inventory(root: &Path, namespace: &str, pending: bool) -> std::io::Result<Vec<String>> {
        super::unsupported_inventory(root, namespace, pending)
    }
}
#[cfg(not(target_os = "linux"))]
pub(super) use unsupported::*;

// Legacy adapters retain exact full draft-bearing identity before filesystem projection.
pub(super) fn stage(
    root: &Path,
    ns: &str,
    m: &StagedDocument,
    manifest: &[u8],
    bytes: &[u8],
) -> std::io::Result<()> {
    stage_file(root, ns, &m.into(), manifest, bytes)
}
pub(super) fn manifest(
    root: &Path,
    ns: &str,
    op: &str,
    version: u32,
) -> std::io::Result<StagedDocument> {
    // Deserialize and validate ONE read, including the full legacy draft binding.
    let m: StagedDocument =
        serde_json::from_slice(&manifest_bytes(root, ns, op)?).map_err(std::io::Error::other)?;
    if m.version != version
        || m.op_id != op
        || m.length < 0
        || m.length > 6_000_000
        || !super::is_safe_relative_path(Path::new(&m.path))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Invalid manifest identity",
        ));
    }
    Ok(m)
}
fn legacy_match(root: &Path, ns: &str, m: &StagedDocument) -> std::io::Result<()> {
    if manifest(root, ns, &m.op_id, m.version)? != *m {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Manifest mismatch",
        ));
    }
    Ok(())
}
pub(super) fn publish(root: &Path, ns: &str, m: &StagedDocument) -> std::io::Result<()> {
    legacy_match(root, ns, m)?;
    publish_file(root, ns, &m.into())
}
pub(super) fn verify_install(root: &Path, ns: &str, m: &StagedDocument) -> std::io::Result<()> {
    legacy_match(root, ns, m)?;
    verify_file(root, ns, &m.into())
}
