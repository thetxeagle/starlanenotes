//! Whole-operation immutable install, consumed only by reviewed move commits.
//! Never overwrites/unlinks a source or target, and recovery never reapplies SQL.
//! Evidence remains on rollback/uncertain commit. Linux-only descriptor backend.
use crate::auth::routes::AppError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
#[cfg(test)]
thread_local! {static VERIFY_AFTER_HASH:std::cell::RefCell<Option<Box<dyn FnOnce()>>>=std::cell::RefCell::new(None);}
const NS: &str = ".move-relocation-journal";
const MAX_FILE: u64 = 104_857_600;
const MAX_MANIFEST: usize = 4_194_304;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilePlan {
    pub subject_id: String,
    pub kind: String,
    pub source: String,
    pub destination: String,
    pub sha256: Option<String>,
    pub maximum: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledFile {
    pub subject_id: String,
    pub kind: String,
    pub source: String,
    pub destination: String,
    pub sha256: String,
    pub length: u64,
    pub source_device: u64,
    pub source_inode: u64,
    pub destination_device: u64,
    pub destination_inode: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreparedRelocation {
    pub version: u32,
    pub op_id: String,
    pub source_fingerprint: String,
    pub request_fingerprint: String,
    pub files: Vec<InstalledFile>,
}
fn conflict(s: impl std::fmt::Display) -> AppError {
    AppError::Conflict(format!("Move storage needs review: {s}"))
}
fn valid_path(s: &str) -> bool {
    !s.is_empty()
        && crate::storage::is_safe_relative_path(Path::new(s))
        && s.split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".." && !p.contains('\\'))
}
pub(crate) fn valid_manifest(m: &PreparedRelocation) -> bool {
    m.files.len() <= 10_000
        && m.version == 1
        && uuid::Uuid::parse_str(&m.op_id).is_ok()
        && m.source_fingerprint.len() == 64
        && m.source_fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
        && m.request_fingerprint.len() == 64
        && m.request_fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
        && m.files.iter().all(|f| {
            valid_path(&f.source)
                && valid_path(&f.destination)
                && f.source != f.destination
                && !f.subject_id.is_empty()
                && matches!(f.kind.as_str(), "canonical" | "attachment" | "drawing")
                && f.length <= MAX_FILE
                && f.sha256.len() == 64
                && f.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        })
}
pub fn prepare(
    root: &Path,
    op_id: &str,
    source_fingerprint: &str,
    request_fingerprint: &str,
    plans: &[FilePlan],
) -> Result<PreparedRelocation, AppError> {
    if uuid::Uuid::parse_str(op_id).is_err()
        || plans.len() > 10_000
        || source_fingerprint.len() != 64
        || !source_fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
        || request_fingerprint.len() != 64
        || !request_fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
        || plans.iter().any(|p| {
            !valid_path(&p.source)
                || !valid_path(&p.destination)
                || p.source == p.destination
                || p.maximum > MAX_FILE
                || p.subject_id.is_empty()
                || !matches!(p.kind.as_str(), "canonical" | "attachment" | "drawing")
        })
    {
        return Err(conflict("invalid operation"));
    }
    // Bound the worst-case serialized envelope BEFORE any directory/evidence write.
    let bound = PreparedRelocation {
        version: 1,
        op_id: op_id.into(),
        source_fingerprint: source_fingerprint.into(),
        request_fingerprint: request_fingerprint.into(),
        files: plans
            .iter()
            .map(|p| InstalledFile {
                subject_id: p.subject_id.clone(),
                kind: p.kind.clone(),
                source: p.source.clone(),
                destination: p.destination.clone(),
                sha256: "f".repeat(64),
                length: p.maximum,
                source_device: u64::MAX,
                source_inode: u64::MAX,
                destination_device: u64::MAX,
                destination_inode: u64::MAX,
            })
            .collect(),
    };
    if serde_json::to_vec(&bound)
        .map_err(AppError::internal)?
        .len()
        > MAX_MANIFEST
    {
        return Err(AppError::BadRequest(
            "Move file inventory exceeds storage manifest limit".into(),
        ));
    }
    let mut source = std::collections::BTreeSet::new();
    let mut dest = std::collections::BTreeSet::new();
    if plans
        .iter()
        .any(|p| !source.insert(&p.source) || !dest.insert(&p.destination))
    {
        return Err(conflict("ambiguous shared file association"));
    }
    backend::prepare(root, op_id, source_fingerprint, request_fingerprint, plans).map_err(conflict)
}
pub fn verify(root: &Path, m: &PreparedRelocation, verify_sources: bool) -> Result<(), AppError> {
    if !valid_manifest(m) {
        return Err(conflict("invalid manifest"));
    }
    let raw = backend::read_manifest(root, &m.op_id).map_err(conflict)?;
    let actual: PreparedRelocation =
        serde_json::from_slice(&raw).map_err(|_| conflict("corrupt manifest"))?;
    if actual != *m {
        return Err(conflict("full immutable intent changed"));
    }
    backend::verify(root, m, verify_sources).map_err(conflict)?;
    if backend::read_manifest(root, &m.op_id).map_err(conflict)? != raw {
        return Err(conflict("manifest changed during verification"));
    }
    crate::storage::restoration::fence(root).map_err(conflict)?;
    Ok(())
}
/// Read custody for one immutable attachment only. Later page publication may
/// replace other files in the same move without retargeting this attachment.
/// Uses the ORIGINAL full manifest bytes for restoration ownership, never a subset.
pub(crate) fn verify_attachment_pair(
    root: &Path,
    m: &PreparedRelocation,
    file: &InstalledFile,
) -> Result<(), AppError> {
    if !valid_manifest(m)
        || file.kind != "attachment"
        || m.files.iter().filter(|f| *f == file).count() != 1
    {
        return Err(conflict("invalid selected attachment binding"));
    }
    let raw = backend::read_manifest(root, &m.op_id).map_err(conflict)?;
    if serde_json::from_slice::<PreparedRelocation>(&raw)
        .map_err(|_| conflict("corrupt manifest"))?
        != *m
    {
        return Err(conflict("full immutable intent changed"));
    }
    backend::verify_attachment_pair(root, m, file).map_err(conflict)?;
    if backend::read_manifest(root, &m.op_id).map_err(conflict)? != raw {
        return Err(conflict("manifest changed during selected verification"));
    }
    crate::storage::restoration::fence(root).map_err(conflict)?;
    Ok(())
}
pub(crate) fn installed_canonical_bytes(
    root: &Path,
    m: &PreparedRelocation,
    subject: &str,
) -> Result<Vec<u8>, AppError> {
    if !valid_manifest(m) {
        return Err(conflict("invalid installed manifest"));
    }
    let file = m
        .files
        .iter()
        .find(|f| f.kind == "canonical" && f.subject_id == subject)
        .ok_or_else(|| conflict("canonical installed subject missing"))?;
    let original = backend::read_manifest(root, &m.op_id).map_err(conflict)?;
    if serde_json::from_slice::<PreparedRelocation>(&original)
        .map_err(|_| conflict("invalid installed manifest"))?
        != *m
    {
        return Err(conflict("installed manifest changed"));
    }
    let bytes = backend::read_installed(root, &m.op_id, &original, file).map_err(conflict)?;
    if backend::read_manifest(root, &m.op_id).map_err(conflict)? != original {
        return Err(conflict("installed manifest changed during read"));
    }
    crate::storage::restoration::fence(root).map_err(conflict)?;
    Ok(bytes)
}
const MAX_RECEIPT: usize = 8_388_608;
const RECEIPT_FIELDS: [&str; 10] = [
    "op_id",
    "preview_id",
    "actor_id",
    "source_space_id",
    "destination_space_id",
    "request_fingerprint",
    "source_fingerprint",
    "manifest_json",
    "expected_state_json",
    "created_at",
];
fn receipt_manifest(receipt: &serde_json::Value) -> Result<PreparedRelocation, AppError> {
    let object = receipt
        .as_object()
        .ok_or_else(|| conflict("invalid logical receipt"))?;
    if object.len() != 10
        || RECEIPT_FIELDS.iter().any(|field| {
            object
                .get(*field)
                .and_then(serde_json::Value::as_str)
                .is_none()
        })
    {
        return Err(conflict("invalid logical receipt fields"));
    }
    let m: PreparedRelocation = serde_json::from_str(receipt["manifest_json"].as_str().unwrap())
        .map_err(|_| conflict("invalid receipt manifest"))?;
    if !valid_manifest(&m)
        || receipt["op_id"] != m.op_id
        || receipt["request_fingerprint"] != m.request_fingerprint
        || receipt["source_fingerprint"] != m.source_fingerprint
    {
        return Err(conflict("logical receipt/manifest identity changed"));
    }
    Ok(m)
}
pub(crate) async fn logical_receipt(
    conn: &mut sqlx::AnyConnection,
    op: &str,
) -> Result<Option<serde_json::Value>, AppError> {
    let row:Option<(String,String,String,String,String,String,String,String,String,String)>=sqlx::query_as("SELECT op_id,preview_id,actor_id,source_space_id,destination_space_id,request_fingerprint,source_fingerprint,manifest_json,expected_state_json,created_at FROM move_commit_receipts WHERE op_id=$1").bind(op).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    Ok(row.map(|r|serde_json::json!({"op_id":r.0,"preview_id":r.1,"actor_id":r.2,"source_space_id":r.3,"destination_space_id":r.4,"request_fingerprint":r.5,"source_fingerprint":r.6,"manifest_json":r.7,"expected_state_json":r.8,"created_at":r.9})))
}
pub(crate) fn escrow_binding(rows: &[serde_json::Value]) -> Result<serde_json::Value, AppError> {
    const FIELDS: [&str; 7] = [
        "op_id",
        "document_id",
        "family",
        "source_id",
        "disposition",
        "original_json",
        "expected_json",
    ];
    if rows.iter().any(|r| {
        r.as_object().is_none_or(|o| {
            o.len() != 7
                || FIELDS
                    .iter()
                    .any(|f| o.get(*f).and_then(serde_json::Value::as_str).is_none())
        })
    }) {
        return Err(conflict("invalid raw escrow evidence"));
    }
    let mut ordered = rows.to_vec();
    ordered.sort_by_cached_key(serde_json::Value::to_string);
    let sha = hex::encode(Sha256::digest(
        serde_json::to_vec(&ordered).map_err(AppError::internal)?,
    ));
    Ok(serde_json::json!({"escrow_count":ordered.len(),"escrow_fingerprint":sha}))
}
pub(crate) async fn verify_escrow(
    conn: &mut sqlx::AnyConnection,
    receipt: &serde_json::Value,
) -> Result<(), AppError> {
    let expected: serde_json::Value = serde_json::from_str(
        receipt["expected_state_json"]
            .as_str()
            .ok_or_else(|| conflict("invalid escrow receipt"))?,
    )
    .map_err(|_| conflict("invalid escrow binding"))?;
    let rows:Vec<(String,String,String,String,String,String,String)>=sqlx::query_as("SELECT op_id,document_id,family,source_id,disposition,original_json,expected_json FROM move_metadata_escrow WHERE op_id=$1").bind(receipt["op_id"].as_str().ok_or_else(||conflict("invalid escrow operation"))?).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let raw:Vec<serde_json::Value>=rows.into_iter().map(|r|serde_json::json!({"op_id":r.0,"document_id":r.1,"family":r.2,"source_id":r.3,"disposition":r.4,"original_json":r.5,"expected_json":r.6})).collect();
    let binding = escrow_binding(&raw)?;
    if expected["escrow_count"] != binding["escrow_count"]
        || expected["escrow_fingerprint"] != binding["escrow_fingerprint"]
    {
        return Err(conflict("immutable escrow evidence changed"));
    }
    Ok(())
}
pub(crate) fn seal_receipt(root: &Path, receipt: &serde_json::Value) -> Result<(), AppError> {
    let m = receipt_manifest(receipt)?;
    let bytes = serde_json::to_vec(&serde_json::json!({"version":1,"receipt":receipt}))
        .map_err(AppError::internal)?;
    if bytes.len() > MAX_RECEIPT {
        return Err(AppError::BadRequest(
            "Move logical receipt exceeds storage evidence limit".into(),
        ));
    }
    verify(root, &m, true)?;
    backend::write_receipt(root, &m.op_id, &bytes).map_err(conflict)?;
    verify_sealed_receipt(root, receipt)?;
    Ok(())
}
pub(crate) fn verify_sealed_receipt(
    root: &Path,
    receipt: &serde_json::Value,
) -> Result<PreparedRelocation, AppError> {
    let m = receipt_manifest(receipt)?;
    let bytes = backend::read_receipt(root, &m.op_id).map_err(conflict)?;
    let actual: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| conflict("corrupt logical receipt seal"))?;
    if actual != serde_json::json!({"version":1,"receipt":receipt}) {
        return Err(conflict("immutable logical receipt changed"));
    }
    Ok(m)
}
#[cfg(test)]
thread_local! {pub(crate) static BEFORE_RECOVERY_GUARD:std::cell::RefCell<Option<std::sync::Arc<tokio::sync::Notify>>>=std::cell::RefCell::new(None);pub(crate) static AFTER_RECOVERY_SQL:std::cell::RefCell<Option<Box<dyn FnOnce()>>>=std::cell::RefCell::new(None);}
/// Historical VERIFY ONLY. A missing/deleted current space is never recreated.
/// `verified_at` records the last successful check; every receipt is checked on restart.
pub async fn recover(pool: &sqlx::AnyPool) -> Result<(), AppError> {
    recover_in(&crate::storage::data_dir(), pool).await
}
async fn recover_in(root: &Path, pool: &sqlx::AnyPool) -> Result<(), AppError> {
    let mut cursor: Option<String> = None;
    loop {
        let ops: Vec<String> = match cursor.as_deref() {
            Some(after) => sqlx::query_scalar(
                "SELECT op_id FROM move_commit_receipts WHERE op_id>$1 ORDER BY op_id LIMIT 128",
            )
            .bind(after)
            .fetch_all(pool)
            .await,
            None => {
                sqlx::query_scalar(
                    "SELECT op_id FROM move_commit_receipts ORDER BY op_id LIMIT 128",
                )
                .fetch_all(pool)
                .await
            }
        }
        .map_err(AppError::internal)?;
        if ops.is_empty() {
            break;
        }
        cursor = ops.last().cloned();
        for op in ops {
            let mut tx = pool.begin().await.map_err(AppError::internal)?;
            #[cfg(test)]
            if let Some(notify) = BEFORE_RECOVERY_GUARD.with(|slot| slot.borrow_mut().take()) {
                notify.notify_one();
            }
            sqlx::query("UPDATE move_commit_receipts SET verified_at=verified_at WHERE op_id=$1")
                .bind(&op)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
            let Some(receipt) = logical_receipt(&mut tx, &op).await? else {
                continue;
            };
            let m = match verify_sealed_receipt(root, &receipt) {
                Ok(m) => m,
                Err(error) => {
                    tracing::warn!(op_id=%op,?error,"Move logical receipt evidence retained; needs review");
                    continue;
                }
            };
            if let Err(error) = verify(root, &m, false) {
                tracing::warn!(op_id=%op,?error,"Move file evidence retained; needs review");
                continue;
            }
            sqlx::query("UPDATE move_commit_receipts SET verified_at=$1 WHERE op_id=$2")
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(&op)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
            // Complete logical SQL tuple and independent envelopes AFTER all verification SQL waits.
            let Some(current) = logical_receipt(&mut tx, &op).await? else {
                continue;
            };
            if let Err(error) = verify_escrow(&mut tx, &current).await {
                match error {
                    AppError::Conflict(_) => {
                        tracing::warn!(op_id=%op,?error,"Move escrow evidence retained; needs review");
                        continue;
                    }
                    other => return Err(other),
                }
            }
            #[cfg(test)]
            if let Some(hook) = AFTER_RECOVERY_SQL.with(|slot| slot.borrow_mut().take()) {
                hook();
            }
            if current != receipt {
                tracing::warn!(op_id=%op,"Move logical receipt changed during verification; retained");
                continue;
            }
            if let Err(error) = verify_sealed_receipt(root, &current)
                .and_then(|_| verify(root, &m, false))
                .and_then(|_| verify_sealed_receipt(root, &current).map(|_| ()))
            {
                tracing::warn!(op_id=%op,?error,"Move independent evidence changed during verification; retained");
                continue;
            }
            tx.commit().await.map_err(AppError::internal)?;
        }
    }
    Ok(())
}

#[path = "relocation_restoration.rs"]
pub(crate) mod restoration_inspection;

#[cfg(target_os = "linux")]
mod backend {
    use super::*;
    use std::{
        ffi::CString,
        fs::File,
        io::{self, Read, Seek, SeekFrom, Write},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::MetadataExt,
        },
    };
    fn c(s: &str) -> io::Result<CString> {
        CString::new(s).map_err(|_| io::Error::other("NUL path"))
    }
    fn open(parent: &File, name: &str, flags: i32) -> io::Result<File> {
        let n = c(name)?;
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                n.as_ptr(),
                flags | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                0o600,
            )
        };
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn root(path: &Path) -> io::Result<File> {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()?.join(path)
        };
        let mut fd = File::open("/")?;
        for component in path.components() {
            match component {
                std::path::Component::RootDir => {}
                std::path::Component::Normal(s) => {
                    fd = open(
                        &fd,
                        s.to_str()
                            .ok_or_else(|| io::Error::other("non-UTF8 root"))?,
                        libc::O_RDONLY | libc::O_DIRECTORY,
                    )?
                }
                _ => return Err(io::Error::other("unsafe root")),
            }
        }
        Ok(fd)
    }
    fn dir(parent: &File, name: &str, create: bool) -> io::Result<File> {
        if create {
            let n = c(name)?;
            let rc = unsafe { libc::mkdirat(parent.as_raw_fd(), n.as_ptr(), 0o700) };
            if rc < 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
                return Err(io::Error::last_os_error());
            }
            parent.sync_all()?;
        }
        open(parent, name, libc::O_RDONLY | libc::O_DIRECTORY)
    }
    fn parent(root: &File, path: &str, create: bool) -> io::Result<(File, String)> {
        if !valid_path(path) {
            return Err(io::Error::other("unsafe relative path"));
        }
        let mut p = root.try_clone()?;
        let mut parts: Vec<_> = path.split('/').collect();
        let leaf = parts.pop().unwrap().to_string();
        for s in parts {
            p = dir(&p, s, create)?;
        }
        Ok((p, leaf))
    }
    fn identity(f: &File) -> io::Result<(u64, u64)> {
        let m = f.metadata()?;
        if !m.is_file() && !m.is_dir() {
            return Err(io::Error::other("not regular file/directory"));
        }
        Ok((m.dev(), m.ino()))
    }
    fn hash(file: &mut File, limit: u64) -> io::Result<(String, u64)> {
        file.seek(SeekFrom::Start(0))?;
        let mut h = Sha256::new();
        let mut len = 0;
        let mut buf = [0u8; 65536];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            len += n as u64;
            if len > limit {
                return Err(io::Error::other("file exceeds move limit"));
            }
            h.update(&buf[..n]);
        }
        Ok((hex::encode(h.finalize()), len))
    }
    fn copy(file: &mut File, target: &mut File, limit: u64) -> io::Result<(String, u64)> {
        file.seek(SeekFrom::Start(0))?;
        let mut h = Sha256::new();
        let mut len = 0;
        let mut buf = [0u8; 65536];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            len += n as u64;
            if len > limit {
                return Err(io::Error::other("file exceeds move limit"));
            }
            target.write_all(&buf[..n])?;
            h.update(&buf[..n]);
        }
        target.sync_all()?;
        Ok((hex::encode(h.finalize()), len))
    }
    fn link(from: &File, name: &str, to: &File, dest: &str) -> io::Result<()> {
        let n = c(name)?;
        let d = c(dest)?;
        let rc =
            unsafe { libc::linkat(from.as_raw_fd(), n.as_ptr(), to.as_raw_fd(), d.as_ptr(), 0) };
        if rc < 0 {
            return Err(io::Error::last_os_error());
        }
        to.sync_all()?;
        Ok(())
    }
    /// Bounded read-only observations for restoration classification. Original
    /// manifest IDs are returned separately by the adapter, never adopted here.
    pub(super) fn inspect_path(
        path: &Path,
        relative: &str,
    ) -> io::Result<Option<super::restoration_inspection::Observation>> {
        let r = root(path)?;
        let (p, leaf) = match parent(&r, relative, false) {
            Ok(v) => v,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let mut held = match open(&p, &leaf, libc::O_RDONLY | libc::O_NONBLOCK) {
            Ok(v) => v,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let before = held.metadata()?;
        if !before.is_file() || before.len() > MAX_FILE {
            return Err(io::Error::other("invalid inspection file"));
        }
        let (sha256, length) = hash(&mut held, MAX_FILE)?;
        let after = held.metadata()?;
        if (
            before.dev(),
            before.ino(),
            before.len(),
            before.mtime(),
            before.mtime_nsec(),
            before.ctime(),
            before.ctime_nsec(),
        ) != (
            after.dev(),
            after.ino(),
            after.len(),
            after.mtime(),
            after.mtime_nsec(),
            after.ctime(),
            after.ctime_nsec(),
        ) {
            return Err(io::Error::other("inspection file changed"));
        }
        let fresh = root(path)?;
        let (fp, fl) = parent(&fresh, relative, false)?;
        if identity(&fresh)? != identity(&r)?
            || identity(&fp)? != identity(&p)?
            || identity(&open(&fp, &fl, libc::O_RDONLY | libc::O_NONBLOCK)?)? != identity(&held)?
        {
            return Err(io::Error::other("inspection path changed"));
        }
        Ok(Some(super::restoration_inspection::Observation {
            device: before.dev(),
            inode: before.ino(),
            links: before.nlink(),
            length,
            sha256,
        }))
    }

    pub(super) fn prepare(
        path: &Path,
        op: &str,
        source: &str,
        request: &str,
        plans: &[FilePlan],
    ) -> io::Result<PreparedRelocation> {
        #[cfg(target_os="linux")]
        let _family=crate::storage::native_family_fence::Creation::shared(path)?;
        let r = root(path)?;
        let ns = dir(&r, NS, true)?;
        let stage = dir(&ns, op, true)?;
        // An operation never resumes an incomplete stage as if it were the same intent.
        let mut claim = open(
            &stage,
            "claim",
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        )?;
        claim.write_all(request.as_bytes())?;
        claim.sync_all()?;
        stage.sync_all()?;
        let mut files = Vec::new();
        for (index, p) in plans.iter().enumerate() {
            let (sp, sl) = parent(&r, &p.source, false)?;
            let mut sf = open(&sp, &sl, libc::O_RDONLY)?;
            if !sf.metadata()?.is_file() {
                return Err(io::Error::other("source not regular"));
            }
            let (sdev, sino) = identity(&sf)?;
            let proof = format!("payload-{index}");
            let mut pf = open(&stage, &proof, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)?;
            let (sha, len) = copy(&mut sf, &mut pf, p.maximum)?;
            if p.sha256.as_ref().is_some_and(|s| s != &sha) {
                return Err(io::Error::other("source differs from committed receipt"));
            }
            stage.sync_all()?;
            // Install anchor uses an independent inode; canonical mutation cannot alter payload evidence.
            let anchor = format!("install-{index}-{}", uuid::Uuid::new_v4());
            let mut af = open(&stage, &anchor, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)?;
            let copied = copy(&mut pf, &mut af, p.maximum)?;
            if copied != (sha.clone(), len) {
                return Err(io::Error::other("stage copy changed"));
            }
            stage.sync_all()?;
            let (dp, dl) = parent(&r, &p.destination, true)?;
            link(&stage, &anchor, &dp, &dl)?;
            let (dev, ino) = identity(&af)?;
            #[cfg(test)]
            if std::env::var("STARLANE_MOVE_TEST_CHILD_PHASE").as_deref() == Ok("after_install") {
                std::process::exit(77)
            }
            files.push(InstalledFile {
                subject_id: p.subject_id.clone(),
                kind: p.kind.clone(),
                source: p.source.clone(),
                destination: p.destination.clone(),
                sha256: sha,
                length: len,
                source_device: sdev,
                source_inode: sino,
                destination_device: dev,
                destination_inode: ino,
            });
        }
        let m = PreparedRelocation {
            version: 1,
            op_id: op.into(),
            source_fingerprint: source.into(),
            request_fingerprint: request.into(),
            files,
        };
        let raw = serde_json::to_vec(&m).map_err(io::Error::other)?;
        if raw.len() > MAX_MANIFEST {
            return Err(io::Error::other("manifest oversized"));
        }
        let mut mf = open(
            &stage,
            "manifest.json",
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        )?;
        mf.write_all(&raw)?;
        mf.sync_all()?;
        stage.sync_all()?;
        ns.sync_all()?;
        #[cfg(test)]
        if std::env::var("STARLANE_MOVE_TEST_CHILD_PHASE").as_deref() == Ok("after_manifest") {
            std::process::exit(77)
        }
        verify(path, &m, true)?;
        Ok(m)
    }
    pub(super) fn read_installed(
        path: &Path,
        op: &str,
        evidence: &[u8],
        f: &InstalledFile,
    ) -> io::Result<Vec<u8>> {
        let r = root(path)?;
        let (p, leaf) = parent(&r, &f.destination, false)?;
        let mut held = open(&p, &leaf, libc::O_RDONLY)?;
        if !held.metadata()?.is_file()
            || !crate::storage::restoration::owns(
                path,
                NS,
                op,
                evidence,
                &f.destination,
                (f.destination_device, f.destination_inode),
                &held,
            )?
        {
            return Err(io::Error::other("installed canonical identity changed"));
        }
        // Ownership verification may inspect bytes through a cloned descriptor.
        // Always read the already held canonical inode from its beginning.
        held.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut held)
            .take(MAX_FILE + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 != f.length || hex::encode(Sha256::digest(&bytes)) != f.sha256 {
            return Err(io::Error::other("installed canonical bytes changed"));
        }
        let fresh = root(path)?;
        let (fp, fl) = parent(&fresh, &f.destination, false)?;
        if identity(&fresh)? != identity(&r)?
            || identity(&fp)? != identity(&p)?
            || identity(&open(&fp, &fl, libc::O_RDONLY)?)? != identity(&held)?
        {
            return Err(io::Error::other("installed canonical path replaced"));
        }
        crate::storage::restoration::fence(path)?;
        Ok(bytes)
    }
    pub(super) fn write_receipt(path: &Path, op: &str, bytes: &[u8]) -> io::Result<()> {
        if uuid::Uuid::parse_str(op).is_err() || bytes.len() > MAX_RECEIPT {
            return Err(io::Error::other("invalid logical receipt"));
        }
        let r = root(path)?;
        let ns = dir(&r, NS, false)?;
        let stage = dir(&ns, op, false)?;
        let mut file = open(
            &stage,
            "receipt.json",
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        )?;
        file.write_all(bytes)?;
        file.sync_all()?;
        stage.sync_all()?;
        ns.sync_all()?;
        Ok(())
    }
    pub(super) fn read_receipt(path: &Path, op: &str) -> io::Result<Vec<u8>> {
        if uuid::Uuid::parse_str(op).is_err() {
            return Err(io::Error::other("invalid logical receipt operation"));
        }
        let r = root(path)?;
        let ns = dir(&r, NS, false)?;
        let stage = dir(&ns, op, false)?;
        let mut file = open(&stage, "receipt.json", libc::O_RDONLY)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::other("logical receipt not regular"));
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take((MAX_RECEIPT + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_RECEIPT {
            return Err(io::Error::other("logical receipt oversized"));
        }
        let fresh = root(path)?;
        let freshns = dir(&fresh, NS, false)?;
        let freshstage = dir(&freshns, op, false)?;
        if identity(&fresh)? != identity(&r)?
            || identity(&freshstage)? != identity(&stage)?
            || identity(&open(&freshstage, "receipt.json", libc::O_RDONLY)?)? != identity(&file)?
        {
            return Err(io::Error::other("logical receipt path replaced"));
        }
        Ok(bytes)
    }
    pub(super) fn read_manifest(path: &Path, op: &str) -> io::Result<Vec<u8>> {
        if uuid::Uuid::parse_str(op).is_err() {
            return Err(io::Error::other("invalid operation"));
        }
        let r = root(path)?;
        let ns = dir(&r, NS, false)?;
        let s = dir(&ns, op, false)?;
        let mut f = open(&s, "manifest.json", libc::O_RDONLY)?;
        let mut out = Vec::new();
        Read::by_ref(&mut f)
            .take((MAX_MANIFEST + 1) as u64)
            .read_to_end(&mut out)?;
        if out.len() > MAX_MANIFEST {
            return Err(io::Error::other("manifest too large"));
        }
        Ok(out)
    }
    pub(super) fn verify(path: &Path, m: &PreparedRelocation, sources: bool) -> io::Result<()> {
        verify_files(path, m, sources, None)
    }
    pub(super) fn verify_attachment_pair(
        path: &Path,
        m: &PreparedRelocation,
        file: &InstalledFile,
    ) -> io::Result<()> {
        verify_files(path, m, true, Some(file))
    }
    fn verify_files(
        path: &Path,
        m: &PreparedRelocation,
        sources: bool,
        selected: Option<&InstalledFile>,
    ) -> io::Result<()> {
        let evidence = read_manifest(path, &m.op_id)?;
        if serde_json::from_slice::<PreparedRelocation>(&evidence).map_err(io::Error::other)? != *m
        {
            return Err(io::Error::other("full immutable intent changed"));
        }
        let r = root(path)?;
        for f in m
            .files
            .iter()
            .filter(|f| selected.is_none_or(|selected| *f == selected))
        {
            for (p, dev, ino) in [(
                f.destination.as_str(),
                f.destination_device,
                f.destination_inode,
            )]
            .into_iter()
            .chain(sources.then_some((
                f.source.as_str(),
                f.source_device,
                f.source_inode,
            ))) {
                let (d, l) = parent(&r, p, false)?;
                let mut held = open(&d, &l, libc::O_RDONLY)?;
                if !held.metadata()?.is_file()
                    || !crate::storage::restoration::owns(
                        path,
                        NS,
                        &m.op_id,
                        &evidence,
                        p,
                        (dev, ino),
                        &held,
                    )?
                    || hash(&mut held, MAX_FILE)? != (f.sha256.clone(), f.length)
                {
                    return Err(io::Error::other("file identity/bytes changed"));
                }
                #[cfg(test)]
                if let Some(hook) = super::VERIFY_AFTER_HASH.with(|slot| slot.borrow_mut().take()) {
                    hook();
                }
                // Final rooted directory and leaf identities, using the SAME hashed descriptor.
                let fresh = root(path)?;
                if identity(&fresh)? != identity(&r)? {
                    return Err(io::Error::other("root replaced"));
                }
                let (dp, leaf) = parent(&fresh, p, false)?;
                if identity(&dp)? != identity(&d)?
                    || identity(&open(&dp, &leaf, libc::O_RDONLY)?)? != identity(&held)?
                {
                    return Err(io::Error::other("parent/leaf replaced"));
                }
            }
        }
        crate::storage::restoration::fence(path)?;
        Ok(())
    }
}
#[cfg(not(target_os = "linux"))]
mod backend {
    use super::*;
    fn unsupported() -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Move relocation requires Linux descriptor backend",
        )
    }
    pub(super) fn read_installed(
        _: &Path,
        _: &str,
        _: &[u8],
        _: &InstalledFile,
    ) -> std::io::Result<Vec<u8>> {
        Err(unsupported())
    }
    pub(super) fn write_receipt(_: &Path, _: &str, _: &[u8]) -> std::io::Result<()> {
        Err(unsupported())
    }
    pub(super) fn read_receipt(_: &Path, _: &str) -> std::io::Result<Vec<u8>> {
        Err(unsupported())
    }
    pub(super) fn prepare(
        _: &Path,
        _: &str,
        _: &str,
        _: &str,
        _: &[FilePlan],
    ) -> std::io::Result<PreparedRelocation> {
        Err(unsupported())
    }
    pub(super) fn read_manifest(_: &Path, _: &str) -> std::io::Result<Vec<u8>> {
        Err(unsupported())
    }
    pub(super) fn verify(_: &Path, _: &PreparedRelocation, _: bool) -> std::io::Result<()> {
        Err(unsupported())
    }
    pub(super) fn verify_attachment_pair(
        _: &Path,
        _: &PreparedRelocation,
        _: &InstalledFile,
    ) -> std::io::Result<()> {
        Err(unsupported())
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    fn fixture() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("move-storage-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(p.join("spaces/source/docs")).unwrap();
        p
    }
    fn plan() -> FilePlan {
        FilePlan {
            subject_id: "page".into(),
            kind: "canonical".into(),
            source: "spaces/source/docs/page.md".into(),
            destination: format!(
                "spaces/destination/docs/revisions/{}.md",
                uuid::Uuid::new_v4()
            ),
            sha256: None,
            maximum: 6_000_000,
        }
    }
    #[test]
    fn exact_bytes_independent_payload_no_overwrite_and_source_identity() {
        let root = fixture();
        std::fs::write(
            root.join("spaces/source/docs/page.md"),
            b"---\nraw: yes\n---\nUnicode \xe2\x98\x83",
        )
        .unwrap();
        let p = plan();
        let m = prepare(
            &root,
            &uuid::Uuid::new_v4().to_string(),
            &"a".repeat(64),
            &"b".repeat(64),
            &[p.clone()],
        )
        .unwrap();
        verify(&root, &m, true).unwrap();
        assert_eq!(
            std::fs::read(root.join(&p.source)).unwrap(),
            std::fs::read(root.join(&p.destination)).unwrap()
        );
        assert!(prepare(
            &root,
            &uuid::Uuid::new_v4().to_string(),
            &"a".repeat(64),
            &"b".repeat(64),
            &[p.clone()]
        )
        .is_err());
        std::fs::write(root.join(&p.destination), b"changed").unwrap();
        assert!(verify(&root, &m, false).is_err());
        assert_eq!(
            std::fs::read(root.join(NS).join(&m.op_id).join("payload-0")).unwrap(),
            std::fs::read(root.join(&p.source)).unwrap()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn source_digest_mismatch_and_oversized_intent_fail_before_install() {
        let root = fixture();
        std::fs::write(root.join("spaces/source/docs/page.md"), "original").unwrap();
        let mut p = plan();
        p.sha256 = Some("0".repeat(64));
        assert!(prepare(
            &root,
            &uuid::Uuid::new_v4().to_string(),
            &"a".repeat(64),
            &"b".repeat(64),
            &[p.clone()]
        )
        .is_err());
        assert!(!root.join(&p.destination).exists());
        let op = uuid::Uuid::new_v4().to_string();
        p.destination = format!("spaces/{}/file", "x".repeat(MAX_MANIFEST));
        assert!(prepare(&root, &op, &"a".repeat(64), &"b".repeat(64), &[p]).is_err());
        assert!(!root.join(NS).join(op).exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn symlink_source_and_destination_ancestors_never_followed() {
        use std::os::unix::fs::symlink;
        let root = fixture();
        let outside = fixture();
        std::fs::write(outside.join("secret"), "secret").unwrap();
        symlink(
            outside.join("secret"),
            root.join("spaces/source/docs/page.md"),
        )
        .unwrap();
        let p = plan();
        assert!(prepare(
            &root,
            &uuid::Uuid::new_v4().to_string(),
            &"a".repeat(64),
            &"b".repeat(64),
            &[p]
        )
        .is_err());
        std::fs::remove_file(root.join("spaces/source/docs/page.md")).unwrap();
        std::fs::write(root.join("spaces/source/docs/page.md"), "safe").unwrap();
        symlink(&outside, root.join("spaces/destination")).unwrap();
        let p = plan();
        assert!(prepare(
            &root,
            &uuid::Uuid::new_v4().to_string(),
            &"a".repeat(64),
            &"b".repeat(64),
            &[p]
        )
        .is_err());
        assert!(!outside.join("docs").exists());
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }
    #[test]
    fn same_bytes_source_leaf_replacement_is_not_same_owned_source() {
        let root = fixture();
        std::fs::write(root.join("spaces/source/docs/page.md"), "original").unwrap();
        let p = plan();
        let m = prepare(
            &root,
            &uuid::Uuid::new_v4().to_string(),
            &"a".repeat(64),
            &"b".repeat(64),
            &[p.clone()],
        )
        .unwrap();
        std::fs::rename(root.join(&p.source), root.join("old-source")).unwrap();
        std::fs::write(root.join(&p.source), "original").unwrap();
        assert!(verify(&root, &m, true).is_err());
        verify(&root, &m, false).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn hundred_megabyte_asset_stream_preserves_exact_length_and_digest() {
        use std::io::Write;
        let root = fixture();
        let mut f = std::fs::File::create(root.join("spaces/source/docs/page.md")).unwrap();
        let block = [0x5au8; 65536];
        for _ in 0..1600 {
            f.write_all(&block).unwrap()
        }
        f.sync_all().unwrap();
        let mut p = plan();
        p.maximum = MAX_FILE;
        let m = prepare(
            &root,
            &uuid::Uuid::new_v4().to_string(),
            &"a".repeat(64),
            &"b".repeat(64),
            &[p],
        )
        .unwrap();
        assert_eq!(m.files[0].length, MAX_FILE);
        verify(&root, &m, true).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn crash_child() {
        let Ok(root) = std::env::var("STARLANE_MOVE_TEST_CHILD_ROOT") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        let op = std::env::var("STARLANE_MOVE_TEST_CHILD_OP").unwrap();
        let mut p = plan();
        p.destination = "spaces/destination/docs/crash.md".into();
        prepare(&root, &op, &"a".repeat(64), &"b".repeat(64), &[p]).unwrap();
        panic!("child fault was not reached")
    }
    #[test]
    fn real_process_crash_keeps_old_source_and_unassociated_owned_evidence() {
        for phase in ["after_install", "after_manifest"] {
            let root = fixture();
            std::fs::write(
                root.join("spaces/source/docs/page.md"),
                "committed old bytes",
            )
            .unwrap();
            let op = uuid::Uuid::new_v4().to_string();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "storage::relocation::tests::crash_child",
                    "--test-threads=1",
                ])
                .env("STARLANE_MOVE_TEST_CHILD_ROOT", &root)
                .env("STARLANE_MOVE_TEST_CHILD_OP", &op)
                .env("STARLANE_MOVE_TEST_CHILD_PHASE", phase)
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(77));
            assert_eq!(
                std::fs::read(root.join("spaces/source/docs/page.md")).unwrap(),
                b"committed old bytes"
            );
            assert_eq!(
                std::fs::read(root.join("spaces/destination/docs/crash.md")).unwrap(),
                b"committed old bytes"
            );
            let manifest = root.join(NS).join(&op).join("manifest.json");
            assert_eq!(manifest.exists(), phase == "after_manifest");
            if manifest.exists() {
                let m: PreparedRelocation =
                    serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
                verify(&root, &m, true).unwrap();
            }
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn late_leaf_parent_and_nonfile_manifest_replacements_fail_final_owned_proof() {
        for phase in ["leaf", "parent", "manifest"] {
            let root = fixture();
            std::fs::write(root.join("spaces/source/docs/page.md"), b"original").unwrap();
            let p = plan();
            let m = prepare(
                &root,
                &uuid::Uuid::new_v4().to_string(),
                &"a".repeat(64),
                &"b".repeat(64),
                &[p.clone()],
            )
            .unwrap();
            let captured_root = root.clone();
            let captured = m.clone();
            let destination = root.join(&p.destination);
            VERIFY_AFTER_HASH.with(|slot| {
                *slot.borrow_mut() = Some(Box::new(move || match phase {
                    "leaf" => {
                        std::fs::rename(&destination, captured_root.join("replaced-leaf")).unwrap();
                        std::fs::write(&destination, b"original").unwrap();
                    }
                    "parent" => {
                        let parent = destination.parent().unwrap();
                        std::fs::rename(parent, captured_root.join("replaced-parent")).unwrap();
                        std::fs::create_dir(parent).unwrap();
                        std::fs::write(&destination, b"original").unwrap();
                    }
                    _ => {
                        let mut altered = captured.clone();
                        altered.request_fingerprint = "c".repeat(64);
                        std::fs::write(
                            captured_root
                                .join(NS)
                                .join(&captured.op_id)
                                .join("manifest.json"),
                            serde_json::to_vec(&altered).unwrap(),
                        )
                        .unwrap();
                    }
                }))
            });
            assert!(verify(&root, &m, false).is_err(), "{phase}");
            assert_eq!(std::fs::read(root.join(&p.source)).unwrap(), b"original");
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn independent_logical_receipt_is_bounded_exclusive_and_never_reconstructed() {
        let root = fixture();
        std::fs::write(root.join("spaces/source/docs/page.md"), b"original").unwrap();
        let m = prepare(
            &root,
            &uuid::Uuid::new_v4().to_string(),
            &"a".repeat(64),
            &"b".repeat(64),
            &[plan()],
        )
        .unwrap();
        let mut receipt = serde_json::json!({"op_id":m.op_id,"preview_id":"preview","actor_id":"actor","source_space_id":"source","destination_space_id":"destination","source_fingerprint":m.source_fingerprint,"request_fingerprint":m.request_fingerprint,"manifest_json":serde_json::to_string(&m).unwrap(),"expected_state_json":"state","created_at":"now"});
        receipt["expected_state_json"] = serde_json::json!("x".repeat(MAX_RECEIPT));
        assert!(matches!(
            seal_receipt(&root, &receipt),
            Err(AppError::BadRequest(_))
        ));
        let path = root.join(NS).join(&m.op_id).join("receipt.json");
        assert!(!path.exists());
        receipt["expected_state_json"] = serde_json::json!("state");
        seal_receipt(&root, &receipt).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(seal_receipt(&root, &receipt).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        verify_sealed_receipt(&root, &receipt).unwrap();
        receipt["created_at"] = serde_json::json!("altered");
        assert!(verify_sealed_receipt(&root, &receipt).is_err());
        std::fs::remove_file(&path).unwrap();
        assert!(verify_sealed_receipt(&root, &receipt).is_err());
        assert!(!path.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn escrow_digest_canonicalizes_row_order_but_preserves_exact_raw_strings() {
        let a = serde_json::json!({"op_id":"op","document_id":"page","family":"tag","source_id":"a","disposition":"map","original_json":"{\"a\":1,\"b\":2}","expected_json":"{}"});
        let mut b = a.clone();
        b["source_id"] = serde_json::json!("b");
        assert_eq!(
            escrow_binding(&[a.clone(), b.clone()]).unwrap(),
            escrow_binding(&[b, a.clone()]).unwrap()
        );
        let mut reordered = a.clone();
        reordered["original_json"] = serde_json::json!("{\"b\":2,\"a\":1}");
        assert_ne!(
            escrow_binding(&[a]).unwrap(),
            escrow_binding(&[reordered]).unwrap()
        );
    }
}
