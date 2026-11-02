//! Candidate private ingestion custody only. No installation, receipt or cleanup authority.
#![cfg(target_os = "linux")]
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    ffi::CString,
    fs::File,
    io::{self, Write},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::MetadataExt,
    },
    path::{Component, Path, PathBuf},
};
pub const CHUNK: usize = 64 * 1024;
pub const NAMESPACE: &str = ".attachment-upload-journal";
const MAX_ENVELOPE: usize = 16 * 1024;
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Reservation {
    pub version: u8,
    pub operation_id: String,
    pub actor_id: String,
    pub space_id: String,
    pub document_id: String,
    pub original_name: String,
    pub mime_type: String,
    pub maximum_bytes: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Seal {
    pub version: u8,
    pub reservation_sha256: String,
    pub payload_sha256: String,
    pub length: u64,
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn name(s: &str) -> io::Result<CString> {
    if s.is_empty() || s == "." || s == ".." || s.contains('/') || s.contains('\\') {
        return Err(invalid("Invalid single component"));
    }
    CString::new(s).map_err(|_| invalid("NUL path component"))
}
pub(super) fn open(dir: &File, n: &str, flags: i32) -> io::Result<File> {
    let n = name(n)?;
    // SAFETY: retained directory FD and validated NUL-terminated single component.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            n.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful descriptor ownership is transferred exactly once.
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn mkdir(dir: &File, n: &str, exclusive: bool) -> io::Result<File> {
    let c = name(n)?;
    // SAFETY: retained directory FD, validated component and private fixed mode.
    let rc = unsafe { libc::mkdirat(dir.as_raw_fd(), c.as_ptr(), 0o700) };
    if rc < 0 {
        let e = io::Error::last_os_error();
        if exclusive || e.kind() != io::ErrorKind::AlreadyExists {
            return Err(e);
        }
    }
    dir.sync_all()?;
    let result = open(dir, n, libc::O_RDONLY | libc::O_DIRECTORY)?;
    if result.metadata()?.mode() & 0o077 != 0 {
        return Err(invalid("Stage directory must be private"));
    }
    Ok(result)
}
pub(super) fn root(path: &Path) -> io::Result<File> {
    let p = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut held = File::open("/")?;
    for c in p.components() {
        match c {
            Component::RootDir => (),
            Component::Normal(n) => {
                held = open(
                    &held,
                    n.to_str().ok_or_else(|| invalid("Non-UTF8 root"))?,
                    libc::O_RDONLY | libc::O_DIRECTORY,
                )?
            }
            _ => return Err(invalid("Invalid root component")),
        }
    }
    Ok(held)
}
pub(super) fn same(a: &File, b: &File) -> io::Result<bool> {
    let a = a.metadata()?;
    let b = b.metadata()?;
    Ok((a.dev(), a.ino()) == (b.dev(), b.ino()))
}
pub(super) fn write_envelope(dir: &File, n: &str, bytes: &[u8]) -> io::Result<File> {
    if bytes.len() > MAX_ENVELOPE {
        return Err(invalid("Envelope too large"));
    }
    let mut f = open(dir, n, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    dir.sync_all()?;
    Ok(f)
}
fn exact_hash(file: &File, expected: u64) -> io::Result<String> {
    use std::os::unix::fs::FileExt;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.nlink() != 1 || meta.len() != expected || meta.mode() & 0o077 != 0 {
        return Err(invalid("Payload metadata differs"));
    }
    let mut h = Sha256::new();
    let mut b = [0u8; CHUNK];
    let mut offset = 0;
    while offset < expected {
        let wanted = (expected - offset).min(CHUNK as u64) as usize;
        let n = file.read_at(&mut b[..wanted], offset)?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Payload truncated",
            ));
        }
        h.update(&b[..n]);
        offset += n as u64;
    }
    if file.read_at(&mut b[..1], offset)? != 0 || file.metadata()?.len() != expected {
        return Err(invalid("Payload grew"));
    }
    Ok(hex::encode(h.finalize()))
}
/// Failure includes the retained relative operation path; no drop/unlink cleanup.
/// Filesystem work is blocking. An async route must use a bounded blocking I/O adapter.
pub struct Stage {
    pub(super) root_path: PathBuf,
    pub(super) root: File,
    namespace: File,
    pub(super) directory: File,
    reservation_file: File,
    pub(super) payload: File,
    reservation: Reservation,
    reservation_bytes: Vec<u8>,
    digest: Sha256,
    length: u64,
    poisoned: bool,
    #[cfg(test)]
    fault: Option<Fault>,
}
#[cfg(test)]
#[derive(Clone, Copy)]
enum Fault {
    PartialWrite,
    PayloadSync,
    SealSync,
    DirectorySync,
    PayloadDriftAfterSeal,
}
impl Stage {
    pub fn reserve(root_path: &Path, reservation: Reservation) -> io::Result<Self> {
        #[cfg(target_os="linux")]
        let _family=crate::storage::native_family_fence::Creation::shared(root_path)?;
        let relative = format!("{NAMESPACE}/{}", reservation.operation_id);
        (|| {
            let id = uuid::Uuid::parse_str(&reservation.operation_id)
                .map_err(|_| invalid("Operation UUID invalid"))?;
            if id.to_string() != reservation.operation_id
                || reservation.version != 1
                || reservation.maximum_bytes > i64::MAX as u64
                || reservation.actor_id.is_empty()
                || reservation.space_id.is_empty()
                || reservation.document_id.is_empty()
                || reservation.original_name.trim().is_empty()
                || reservation.original_name.len() > 255
            {
                return Err(invalid("Reservation fields invalid"));
            }
            let reservation_bytes = serde_json::to_vec(&reservation).map_err(io::Error::other)?;
            if reservation_bytes.len() > MAX_ENVELOPE {
                return Err(invalid("Envelope too large"));
            }
            let held_root = root(root_path)?;
            let namespace = mkdir(&held_root, NAMESPACE, false)?;
            let directory = mkdir(&namespace, &reservation.operation_id, true)?;
            let reservation_file =
                write_envelope(&directory, "reservation.json", &reservation_bytes)?;
            let payload = open(
                &directory,
                "payload.part",
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
            )?;
            payload.sync_all()?;
            directory.sync_all()?;
            let stage = Self {
                root_path: root_path.into(),
                root: held_root,
                namespace,
                directory,
                reservation_file,
                payload,
                reservation,
                reservation_bytes,
                digest: Sha256::new(),
                length: 0,
                poisoned: false,
                #[cfg(test)]
                fault: None,
            };
            stage.fence()?;
            Ok(stage)
        })()
        .map_err(|e: io::Error| {
            io::Error::new(
                e.kind(),
                format!("Upload stage {relative} retained if created: {e}"),
            )
        })
    }
    pub fn relative_directory(&self) -> String {
        format!("{NAMESPACE}/{}", self.reservation.operation_id)
    }
    fn error(&self, e: io::Error) -> io::Error {
        io::Error::new(
            e.kind(),
            format!("Upload stage {} retained: {e}", self.relative_directory()),
        )
    }
    fn fence(&self) -> io::Result<()> {
        let r = root(&self.root_path)?;
        let ns = open(&r, NAMESPACE, libc::O_RDONLY | libc::O_DIRECTORY)?;
        let d = open(
            &ns,
            &self.reservation.operation_id,
            libc::O_RDONLY | libc::O_DIRECTORY,
        )?;
        if ns.metadata()?.mode() & 0o077 != 0 || d.metadata()?.mode() & 0o077 != 0 {
            return Err(invalid("Stage directory privacy changed"));
        }
        if !same(&r, &self.root)? || !same(&ns, &self.namespace)? || !same(&d, &self.directory)? {
            return Err(invalid("Stage directory identity changed"));
        }
        let p = open(&d, "payload.part", libc::O_RDONLY | libc::O_NONBLOCK)?;
        let reservation = open(&d, "reservation.json", libc::O_RDONLY | libc::O_NONBLOCK)?;
        if !same(&p, &self.payload)? || !same(&reservation, &self.reservation_file)? {
            return Err(invalid("Stage file identity changed"));
        }
        if exact_hash(&reservation, self.reservation_bytes.len() as u64)?
            != hex::encode(Sha256::digest(&self.reservation_bytes))
        {
            return Err(invalid("Reservation bytes changed"));
        }
        Ok(())
    }
    pub fn append(&mut self, bytes: &[u8]) -> io::Result<()> {
        let result = (|| {
            if self.poisoned {
                return Err(invalid("Stage previously failed"));
            }
            if bytes.len() > CHUNK {
                return Err(invalid("Chunk exceeds64KiB"));
            }
            let length = self
                .length
                .checked_add(bytes.len() as u64)
                .ok_or_else(|| invalid("Byte count overflow"))?;
            if length > self.reservation.maximum_bytes {
                return Err(invalid("Configured upload limit exceeded"));
            }
            self.fence()?;
            let meta = self.payload.metadata()?;
            if !meta.is_file()
                || meta.len() != self.length
                || meta.nlink() != 1
                || meta.mode() & 0o077 != 0
            {
                return Err(invalid("Payload metadata changed"));
            }
            #[cfg(test)]
            if matches!(self.fault, Some(Fault::PartialWrite)) {
                self.payload.write_all(&bytes[..bytes.len() / 2])?;
                return Err(io::Error::from_raw_os_error(libc::ENOSPC));
            }
            self.payload.write_all(bytes)?;
            self.digest.update(bytes);
            self.length = length;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result.map_err(|e| self.error(e))
    }
    pub fn finish(self) -> io::Result<SealedStage> {
        let outcome = (|| {
            if self.poisoned {
                return Err(invalid("Stage failed; cannot seal partial upload"));
            }
            self.fence()?;
            #[cfg(test)]
            if matches!(self.fault, Some(Fault::PayloadSync)) {
                return Err(io::Error::from_raw_os_error(libc::EIO));
            }
            self.payload.sync_all()?;
            let sha = hex::encode(self.digest.clone().finalize());
            if exact_hash(&self.payload, self.length)? != sha {
                return Err(invalid("Payload bytes changed"));
            }
            let seal = Seal {
                version: 1,
                reservation_sha256: hex::encode(Sha256::digest(&self.reservation_bytes)),
                payload_sha256: sha,
                length: self.length,
            };
            let seal_bytes = serde_json::to_vec(&seal).map_err(io::Error::other)?;
            let mut seal_file = open(
                &self.directory,
                "seal.json",
                libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
            )?;
            seal_file.write_all(&seal_bytes)?;
            #[cfg(test)]
            if matches!(self.fault, Some(Fault::SealSync)) {
                return Err(io::Error::from_raw_os_error(libc::EIO));
            }
            seal_file.sync_all()?;
            #[cfg(test)]
            if matches!(self.fault, Some(Fault::PayloadDriftAfterSeal)) {
                use std::os::unix::fs::FileExt;
                self.payload.write_at(b"X", 0)?;
            }
            #[cfg(test)]
            if matches!(self.fault, Some(Fault::DirectorySync)) {
                return Err(io::Error::from_raw_os_error(libc::EIO));
            }
            self.directory.sync_all()?;
            self.fence()?;
            if exact_hash(&seal_file, seal_bytes.len() as u64)?
                != hex::encode(Sha256::digest(&seal_bytes))
            {
                return Err(invalid("Seal bytes changed"));
            }
            let fresh = open(
                &self.directory,
                "seal.json",
                libc::O_RDONLY | libc::O_NONBLOCK,
            )?;
            if !same(&fresh, &seal_file)? {
                return Err(invalid("Seal identity changed"));
            }
            if exact_hash(&self.payload, self.length)? != seal.payload_sha256 {
                return Err(invalid("Payload changed during seal I/O"));
            }
            self.fence()?;
            Ok((seal, seal_file))
        })();
        match outcome {
            Ok((seal, seal_file)) => Ok(SealedStage {
                stage: self,
                seal,
                seal_file,
            }),
            Err(e) => Err(self.error(e)),
        }
    }
}
pub struct SealedStage {
    pub(super) stage: Stage,
    seal: Seal,
    seal_file: File,
}
impl SealedStage {
    pub fn seal(&self) -> &Seal {
        &self.seal
    }
    pub fn relative_directory(&self) -> String {
        self.stage.relative_directory()
    }
    pub fn verify(&self) -> io::Result<()> {
        (|| {
            self.stage.fence()?;
            let fresh = open(
                &self.stage.directory,
                "seal.json",
                libc::O_RDONLY | libc::O_NONBLOCK,
            )?;
            let expected = serde_json::to_vec(&self.seal).map_err(io::Error::other)?;
            if !same(&fresh, &self.seal_file)?
                || exact_hash(&fresh, expected.len() as u64)?
                    != hex::encode(Sha256::digest(&expected))
                || exact_hash(&self.stage.payload, self.seal.length)? != self.seal.payload_sha256
            {
                return Err(invalid("Sealed custody changed"));
            }
            self.stage.fence()
        })()
        .map_err(|e| self.stage.error(e))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("upload-stage-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&p).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self(p)
        }
        fn reserve(&self, maximum: u64) -> Stage {
            Stage::reserve(&self.0, reservation(maximum)).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn reservation(maximum: u64) -> Reservation {
        Reservation {
            version: 1,
            operation_id: uuid::Uuid::new_v4().to_string(),
            actor_id: "actor".into(),
            space_id: "space".into(),
            document_id: "page".into(),
            original_name: "Original.txt".into(),
            mime_type: "text/plain".into(),
            maximum_bytes: maximum,
        }
    }
    #[test]
    fn real_private_descriptors_chunks_hash_and_zero_bytes() {
        let f = Fixture::new();
        let mut s = f.reserve((CHUNK * 3) as u64);
        let relative = s.relative_directory();
        for _ in 0..3 {
            s.append(&vec![0x55; CHUNK]).unwrap();
        }
        let held = s.finish().unwrap();
        held.verify().unwrap();
        assert_eq!(held.seal().length, (CHUNK * 3) as u64);
        assert_eq!(
            held.seal().payload_sha256,
            hex::encode(Sha256::digest(vec![0x55; CHUNK * 3]))
        );
        for n in ["reservation.json", "payload.part", "seal.json"] {
            assert_eq!(
                std::fs::metadata(f.0.join(&relative).join(n))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        assert_eq!(
            std::fs::metadata(f.0.join(&relative))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        let zero = f.reserve(0).finish().unwrap();
        zero.verify().unwrap();
        assert_eq!(zero.seal().length, 0);
    }
    #[test]
    fn limit_and_oversized_chunk_fail_before_extra_bytes_and_poison_stage() {
        let f = Fixture::new();
        let mut s = f.reserve(3);
        let p = f.0.join(s.relative_directory());
        s.append(b"abc").unwrap();
        let e = s.append(b"d").unwrap_err();
        assert!(e.to_string().contains(&s.relative_directory()));
        assert_eq!(std::fs::read(p.join("payload.part")).unwrap(), b"abc");
        assert!(s.finish().is_err());
        assert!(!p.join("seal.json").exists());
        let mut s = f.reserve((CHUNK + 1) as u64);
        let p = f.0.join(s.relative_directory());
        assert!(s.append(&vec![0; CHUNK + 1]).is_err());
        assert_eq!(std::fs::metadata(p.join("payload.part")).unwrap().len(), 0);
    }
    #[test]
    fn cancellation_and_injected_partial_disk_write_retain_bytes_without_seal() {
        let f = Fixture::new();
        let mut s = f.reserve(20);
        let p = f.0.join(s.relative_directory());
        s.append(b"prefix").unwrap();
        drop(s);
        assert_eq!(std::fs::read(p.join("payload.part")).unwrap(), b"prefix");
        assert!(!p.join("seal.json").exists());
        let mut s = f.reserve(20);
        let p = f.0.join(s.relative_directory());
        s.append(b"old").unwrap();
        s.fault = Some(Fault::PartialWrite);
        let e = s.append(b"abcdef").unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::StorageFull);
        assert_eq!(std::fs::read(p.join("payload.part")).unwrap(), b"oldabc");
        assert!(s.finish().is_err());
        assert!(!p.join("seal.json").exists());
    }
    #[test]
    fn sync_failures_retain_actionable_complete_or_indeterminate_artifacts() {
        for fault in [Fault::PayloadSync, Fault::SealSync, Fault::DirectorySync] {
            let f = Fixture::new();
            let mut s = f.reserve(4);
            let relative = s.relative_directory();
            let p = f.0.join(&relative);
            s.append(b"data").unwrap();
            s.fault = Some(fault);
            let e = s.finish().err().unwrap();
            assert!(e.to_string().contains(&relative));
            assert_eq!(std::fs::read(p.join("payload.part")).unwrap(), b"data");
            assert_eq!(
                p.join("seal.json").exists(),
                !matches!(fault, Fault::PayloadSync)
            );
        }
    }
    #[test]
    fn same_inode_payload_change_during_seal_io_refuses_completed_custody() {
        let f = Fixture::new();
        let mut s = f.reserve(4);
        let p = f.0.join(s.relative_directory());
        s.append(b"data").unwrap();
        s.fault = Some(Fault::PayloadDriftAfterSeal);
        assert!(s.finish().is_err());
        assert_eq!(std::fs::read(p.join("payload.part")).unwrap(), b"Xata");
        assert!(p.join("seal.json").exists());
    }
    #[test]
    fn collision_symlink_and_replaced_payload_never_modify_unowned_bytes() {
        let f = Fixture::new();
        let r = reservation(12);
        let mut s = Stage::reserve(&f.0, r.clone()).unwrap();
        s.append(b"old").unwrap();
        assert_eq!(
            Stage::reserve(&f.0, r).err().unwrap().kind(),
            io::ErrorKind::AlreadyExists
        );
        let p = f.0.join(s.relative_directory()).join("payload.part");
        std::fs::rename(&p, p.with_extension("held")).unwrap();
        std::fs::write(&p, b"new sentinel").unwrap();
        assert!(s.append(b"x").is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"new sentinel");
        let outside = Fixture::new();
        let alias = f.0.join("alias");
        symlink(&outside.0, &alias).unwrap();
        assert!(Stage::reserve(&alias, reservation(5)).is_err());
        assert!(!outside.0.join(NAMESPACE).exists());
    }
    #[test]
    fn reservation_payload_and_seal_mutation_are_rejected() {
        for artifact in ["reservation.json", "payload.part", "seal.json"] {
            let f = Fixture::new();
            let mut s = f.reserve(3);
            s.append(b"abc").unwrap();
            let h = s.finish().unwrap();
            let p = f.0.join(h.relative_directory()).join(artifact);
            let bytes = std::fs::read(&p).unwrap();
            let mut changed = bytes.clone();
            changed[0] ^= 1;
            std::fs::write(&p, &changed).unwrap();
            assert!(h.verify().is_err());
            assert_eq!(std::fs::read(&p).unwrap(), changed);
        }
    }
    #[test]
    fn moved_operation_and_added_hardlink_fail_closed() {
        let f = Fixture::new();
        let mut s = f.reserve(3);
        s.append(b"abc").unwrap();
        let p = f.0.join(s.relative_directory());
        std::fs::rename(&p, p.with_extension("old")).unwrap();
        std::fs::create_dir(&p).unwrap();
        assert!(s.finish().is_err());
        assert_eq!(std::fs::read_dir(p).unwrap().count(), 0);
        let s = f.reserve(3);
        let p = f.0.join(s.relative_directory()).join("payload.part");
        std::fs::hard_link(&p, f.0.join("foreign-link")).unwrap();
        assert!(s.finish().is_err());
        assert!(f.0.join("foreign-link").exists());
    }
    #[tokio::test]
    async fn real_streamed_multipart_is_captured_without_whole_file_vec() {
        use axum::{
            body::{Body, Bytes},
            extract::{FromRequest, Multipart},
            http::Request,
        };
        use futures::{stream, StreamExt};
        let f = Fixture::new();
        let length = CHUNK * 3 + 17;
        let mut stage = f.reserve(length as u64);
        let header = stream::once(async {
            Ok::<_,io::Error>(Bytes::from_static(b"--b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"x.txt\"\r\nContent-Type: text/plain\r\n\r\n"))
        });
        let payload = stream::unfold(length, |left| async move {
            if left == 0 {
                None
            } else {
                let n = left.min(8192);
                Some((Ok::<_, io::Error>(Bytes::from(vec![b'R'; n])), left - n))
            }
        });
        let trailer =
            stream::once(async { Ok::<_, io::Error>(Bytes::from_static(b"\r\n--b--\r\n")) });
        let request = Request::builder()
            .header("content-type", "multipart/form-data; boundary=b")
            .body(Body::from_stream(header.chain(payload).chain(trailer)))
            .unwrap();
        let mut multipart = Multipart::from_request(request, &()).await.unwrap();
        let mut field = multipart.next_field().await.unwrap().unwrap();
        assert_eq!(field.name(), Some("file"));
        assert_eq!(field.file_name(), Some("x.txt"));
        while let Some(chunk) = field.chunk().await.unwrap() {
            for bounded in chunk.chunks(CHUNK) {
                stage.append(bounded).unwrap();
            }
        }
        let h = stage.finish().unwrap();
        h.verify().unwrap();
        assert_eq!(h.seal().length, length as u64);
        drop(field);
        assert!(multipart.next_field().await.unwrap().is_none());
    }
    #[tokio::test]
    async fn actual_multipart_disconnect_retains_partial_stage_unsealed() {
        use axum::{
            body::{Body, Bytes},
            extract::{FromRequest, Multipart},
            http::Request,
        };
        use futures::{stream, StreamExt};
        let f = Fixture::new();
        let mut s = f.reserve(16384);
        let p = f.0.join(s.relative_directory());
        let frames = stream::unfold(0usize, |i| async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            match i {
                0=>Some((Ok::<_,io::Error>(Bytes::from_static(b"--b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"x.txt\"\r\n\r\n")),1)),
                1=>Some((Ok(Bytes::from(vec![b'P';8192])),2)),
                2=>Some((Err(io::Error::new(io::ErrorKind::ConnectionReset,"actual body source disconnected")),3)),
                _=>None,
            }
        });
        let request = Request::builder()
            .header("content-type", "multipart/form-data; boundary=b")
            .body(Body::from_stream(frames))
            .unwrap();
        let mut m = Multipart::from_request(request, &()).await.unwrap();
        let mut field = m.next_field().await.unwrap().unwrap();
        let mut failed = false;
        loop {
            match field.chunk().await {
                Ok(Some(c)) => {
                    for b in c.chunks(CHUNK) {
                        s.append(b).unwrap();
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }
        assert!(failed);
        drop(field);
        drop(s);
        assert!(!p.join("seal.json").exists());
        let retained = std::fs::read(p.join("payload.part")).unwrap();
        assert!(!retained.is_empty() && retained.len() <= 8192);
        assert!(retained.iter().all(|b| *b == b'P'));
    }
    #[tokio::test]
    async fn actual_multipart_task_abort_retains_private_partial_stage() {
        use axum::{
            body::{Body, Bytes},
            extract::{FromRequest, Multipart},
            http::Request,
        };
        use futures::{stream, StreamExt};
        let f = Fixture::new();
        let mut stage = f.reserve(16384);
        let p = f.0.join(stage.relative_directory());
        let (sent, received) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let header = stream::once(async {
                Ok::<_,io::Error>(Bytes::from_static(b"--b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"x.txt\"\r\n\r\n"))
            });
            let payload = stream::once(async { Ok::<_, io::Error>(Bytes::from(vec![b'C'; 8192])) });
            let never = stream::once(std::future::pending::<Result<Bytes, io::Error>>());
            let request = Request::builder()
                .header("content-type", "multipart/form-data; boundary=b")
                .body(Body::from_stream(header.chain(payload).chain(never)))
                .unwrap();
            let mut m = Multipart::from_request(request, &()).await.unwrap();
            let mut field = m.next_field().await.unwrap().unwrap();
            let chunk = field.chunk().await.unwrap().unwrap();
            for b in chunk.chunks(CHUNK) {
                stage.append(b).unwrap();
            }
            sent.send(()).unwrap();
            let _ = field.chunk().await;
            panic!("Body must stay pending until task cancellation");
        });
        tokio::time::timeout(std::time::Duration::from_secs(2), received)
            .await
            .unwrap()
            .unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let retained = std::fs::read(p.join("payload.part")).unwrap();
        assert!(!retained.is_empty() && retained.len() <= 8192);
        assert!(retained.iter().all(|b| *b == b'C'));
        assert!(!p.join("seal.json").exists());
        assert_eq!(
            std::fs::metadata(p.join("payload.part"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    #[test]
    #[ignore = "Requires owned private64MiB tmpfs STARLANE_UPLOAD_STAGE_CAPACITY_DIR"]
    fn actual_kernel_enospc_retains_partial_private_payload() {
        let base = std::env::var("STARLANE_UPLOAD_STAGE_CAPACITY_DIR")
            .expect("private capacity fixture required");
        let r = PathBuf::from(base).join(uuid::Uuid::new_v4().to_string());
        std::fs::create_dir(&r).unwrap();
        std::fs::set_permissions(&r, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut s = Stage::reserve(&r, reservation(96 * 1024 * 1024)).unwrap();
        let p = r.join(s.relative_directory());
        let chunk = [0x4a; CHUNK];
        let e = loop {
            match s.append(&chunk) {
                Ok(()) => (),
                Err(e) => break e,
            }
        };
        assert_eq!(e.kind(), io::ErrorKind::StorageFull);
        assert!(e.to_string().contains(&s.relative_directory()));
        let length = std::fs::metadata(p.join("payload.part")).unwrap().len();
        assert!(length > 0 && length < 96 * 1024 * 1024);
        assert!(s.finish().is_err());
        assert!(!p.join("seal.json").exists());
        eprintln!("actual ENOSPC retained_private_payload_length={length}");
        std::fs::remove_dir_all(r).unwrap();
    }
}
