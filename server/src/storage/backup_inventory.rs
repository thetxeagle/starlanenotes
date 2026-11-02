//! Read-only format-3 legacy and format-4 capacity backup admission. Checksums prove consistency, not origin authentication.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{AnyPool, Row};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io,
    path::Path,
};
const DATABASE: &str = "starlane-notes.db";
const NAMESPACES: [&str; 6] = [
    ".storage-journal",
    ".publication-journal",
    ".canonical-rewrite-journal",
    ".move-relocation-journal",
    ".restoration-custody",
    ".attachment-upload-journal",
];
const CAPACITY_NAMESPACES: [&str; 7] = [".storage-journal", ".publication-journal", ".canonical-rewrite-journal", ".move-relocation-journal", ".restoration-custody", ".attachment-upload-journal", ".attachment-upload-admission"];
fn namespaces(version:u32)->&'static [&'static str] {if version==4 {&CAPACITY_NAMESPACES} else {&NAMESPACES}}
const RECEIPTS: [&str; 8] = [
    "storage_journal_receipts",
    "publication_receipts",
    "canonical_revision_receipts",
    "move_commit_receipts",
    "asset_replay_receipts",
    "storage_restore_generations",
    "storage_restore_current",
    "attachment_upload_receipts",
];
const PREREQUISITE: &str = "Recover the original deployment configuration and secrets separately, securely; this set does not contain external configuration or secret files. Protect the database backup itself: it contains private data and authentication material.";
fn invalid(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s)
}
fn safe(s: &str) -> bool {
    !s.is_empty()
        && !s.contains(['\0', '\\'])
        && s.split('/').all(|p| !p.is_empty() && p != "." && p != "..")
}
fn hex(s: &str, n: usize) -> bool {
    s.len() == n
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
/// Reject duplicate map keys before JSON can erase conflicting inventory evidence.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct UniqueMap<V>(pub BTreeMap<String, V>);
impl<'de, V: Deserialize<'de>> Deserialize<'de> for UniqueMap<V> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor<V>(std::marker::PhantomData<V>);
        impl<'de, V: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<V> {
            type Value = UniqueMap<V>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a map with unique keys")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut m: M,
            ) -> Result<Self::Value, M::Error> {
                let mut out = BTreeMap::new();
                while let Some((k, v)) = m.next_entry::<String, V>()? {
                    if out.insert(k, v).is_some() {
                        return Err(serde::de::Error::custom("duplicate inventory key"));
                    }
                }
                Ok(UniqueMap(out))
            }
        }
        d.deserialize_map(Visitor(std::marker::PhantomData))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity {
    pub device: u64,
    pub inode: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceFile {
    pub device: u64,
    pub inode: u64,
    pub links: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Custody {
    pub version: u32,
    pub root: Identity,
    pub files: UniqueMap<SourceFile>,
    pub directories: UniqueMap<Identity>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Bytes {
    pub size_bytes: u64,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Migration {
    version: i64,
    checksum_sha384: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Schema {
    engine: String,
    applied_version: i64,
    user_version: i64,
    migrations: Vec<Migration>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ReceiptCount {
    table: String,
    rows: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Boundary {
    scope: String,
    ordinary_restore: String,
    journal_namespaces: Vec<String>,
    journal_directories: Vec<String>,
    receipt_families: Vec<ReceiptCount>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    format: String,
    format_version: u32,
    pub backup_id: String,
    application_version: String,
    capture_started_at: String,
    capture_finished_at: String,
    writers_stopped_asserted: bool,
    database: Schema,
    pub files: UniqueMap<Bytes>,
    excluded: Vec<String>,
    configuration_prerequisite: String,
    recovery_boundary: Boundary,
    pub source_custody: Custody,
    #[serde(default,skip_serializing_if="Option::is_none",deserialize_with="admission_capture_nonnull")]
    pub admission_owner_capture:Option<crate::storage::restoration::admission::AdmissionCaptureEnvelope>,
    #[serde(default,skip_serializing_if="Option::is_none",deserialize_with="prior_capture_nonnull")]
    pub prior_admission_owner_capture:Option<crate::storage::restoration::admission::PriorRestoredCaptureEvidence>,
    #[serde(default,skip_serializing_if="Option::is_none",deserialize_with="completed_capture_nonnull")]
    pub completed_absence_capture:Option<crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope>,
    #[serde(default,skip_serializing_if="Option::is_none",deserialize_with="prior_completed_capture_nonnull")]
    pub prior_completed_absence_capture:Option<crate::storage::restoration::PriorCompletedCaptureEvidence>,
    #[serde(default,skip_serializing_if="Option::is_none",deserialize_with="mixed_completed_capture_nonnull")]
    pub mixed_completed_absence_capture:Option<crate::storage::restoration::MixedCompletedCaptureEvidence>,
    #[serde(default,skip_serializing_if="Option::is_none",deserialize_with="composite_capture_nonnull")]
    pub composite_admission_capture:Option<crate::storage::restoration::composite::CompositeCaptureEvidence>,
    #[serde(default,skip_serializing_if="Option::is_none",deserialize_with="crate::storage::restoration::historical::capture_nonnull")]
    pub historical_composite_preservation_capture:Option<crate::storage::restoration::historical::HistoricalCompositePreservationEvidence>,
}
impl Manifest {
    pub(crate) fn compatible_catalogue(&self) -> String {
        format!("{}/{}", self.database.engine, self.database.applied_version)
    }
}
fn admission_capture_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<crate::storage::restoration::admission::AdmissionCaptureEnvelope>,D::Error>{Ok(Some(serde::Deserialize::deserialize(d)?))}
fn prior_capture_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<crate::storage::restoration::admission::PriorRestoredCaptureEvidence>,D::Error>{Ok(Some(serde::Deserialize::deserialize(d)?))}
fn completed_capture_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope>,D::Error>{Ok(Some(serde::Deserialize::deserialize(d)?))}
fn prior_completed_capture_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<crate::storage::restoration::PriorCompletedCaptureEvidence>,D::Error>{Ok(Some(serde::Deserialize::deserialize(d)?))}
fn mixed_completed_capture_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<crate::storage::restoration::MixedCompletedCaptureEvidence>,D::Error>{Ok(Some(serde::Deserialize::deserialize(d)?))}
fn composite_capture_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<crate::storage::restoration::composite::CompositeCaptureEvidence>,D::Error>{Ok(Some(serde::Deserialize::deserialize(d)?))}
fn validate(m: &Manifest) -> io::Result<()> {
    if let Some(h)=&m.historical_composite_preservation_capture{if m.format_version!=4||!matches!(m.database.applied_version,53|54|55|56|57)||m.composite_admission_capture.is_some()||m.mixed_completed_absence_capture.is_some()||m.prior_completed_absence_capture.is_some()||m.completed_absence_capture.is_some()||m.admission_owner_capture.is_some()||m.prior_admission_owner_capture.is_some(){return Err(invalid("Historical capture requires distinct exclusive qualified53 format"));}h.validate_shape()?;}
    if let Some(c)=&m.composite_admission_capture {if m.format_version!=4||!matches!(m.database.applied_version,53|54|55|56|57)||m.mixed_completed_absence_capture.is_some()||m.prior_completed_absence_capture.is_some()||m.completed_absence_capture.is_some()||m.admission_owner_capture.is_some()||m.prior_admission_owner_capture.is_some(){return Err(invalid("Composite capture requires distinct exclusive qualified53 format"));}c.validate_shape()?;}

    if let Some(c)=&m.mixed_completed_absence_capture {if m.format_version!=4||!matches!(m.database.applied_version,53|54|55|56|57)||m.completed_absence_capture.is_some()||m.prior_completed_absence_capture.is_some()||m.admission_owner_capture.is_some()||m.prior_admission_owner_capture.is_some(){return Err(invalid("Mixed completed capture requires exclusive qualified erasure format"));}c.validate_shape()?;}
    if let Some(c)=&m.prior_completed_absence_capture {if m.format_version!=4||!matches!(m.database.applied_version,53|54|55|56|57)||m.completed_absence_capture.is_some()||m.admission_owner_capture.is_some()||m.prior_admission_owner_capture.is_some(){return Err(invalid("Prior completed capture requires exclusive qualified erasure format"));}c.validate_shape()?;}

    if let Some(c)=&m.completed_absence_capture {if m.format_version!=4||!matches!(m.database.applied_version,53|54|55|56|57){return Err(invalid("Completed absence requires qualified erasure format"));}c.validate_shape().map_err(|_|invalid("Malformed completed absence capture"))?;}
    if let Some(e)=&m.prior_admission_owner_capture {if m.format_version!=4||!matches!(m.database.applied_version,51|52|53|54|55|56|57)||m.admission_owner_capture.is_some(){return Err(invalid("Prior capture requires exclusive qualified capacity format"));}e.validate_shape()?;}
    if let Some(c)=&m.admission_owner_capture {if m.format_version!=4||!matches!(m.database.applied_version,51|52|53|54|55|56|57){return Err(invalid("Admission capture requires qualified capacity format"));}c.validate_native_reserved()?;}
    let version = m
        .application_version
        .strip_prefix('v')
        .unwrap_or(&m.application_version);
    let split = version.find(['-', '+']);
    let core = split.map_or(version, |i| &version[..i]);
    let suffix_valid = split.is_none_or(|i| {
        let suffix = &version[i + 1..];
        !suffix.is_empty()
            && suffix
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    });
    if m.format != "starlane-notes-stopped-write-backup"
        || ![3,4].contains(&m.format_version)
        || (m.format_version == 4) != (matches!(m.database.applied_version,51|52|53|54|55|56|57))
        || uuid::Uuid::parse_str(&m.backup_id).is_err()
        || uuid::Uuid::parse_str(&m.backup_id).is_ok_and(|id| id.to_string() != m.backup_id)
        || !m.writers_stopped_asserted
        || m.configuration_prerequisite != PREREQUISITE
        || m.excluded != ["logging", "logs", "search_index"]
        || core.split('.').count() != 3
        || core
            .split('.')
            .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
        || !suffix_valid
        || m.database.engine != "sqlite"
        || m.source_custody.version != 1
        || m.source_custody.root.inode == 0
    {
        return Err(invalid("Unsupported backup contract"));
    }
    let start = chrono::DateTime::parse_from_rfc3339(&m.capture_started_at)
        .map_err(|_| invalid("Invalid capture time"))?;
    let end = chrono::DateTime::parse_from_rfc3339(&m.capture_finished_at)
        .map_err(|_| invalid("Invalid capture time"))?;
    if start.offset().local_minus_utc() != 0 || end.offset().local_minus_utc() != 0 || start > end {
        return Err(invalid("Invalid capture window"));
    }
    let mut expected: BTreeSet<_> = m.files.0.keys().cloned().collect();
    if !expected.remove(DATABASE) || expected != m.source_custody.files.0.keys().cloned().collect()
    {
        return Err(invalid("Source identity inventory differs"));
    }
    let mut groups: BTreeMap<(u64, u64), Vec<&str>> = BTreeMap::new();
    for (path, b) in &m.files.0 {
        if !safe(path) || b.size_bytes > i64::MAX as u64 || !hex(&b.sha256, 64) {
            return Err(invalid("Invalid file inventory"));
        }
        if path != DATABASE
            && path != ".starlane-notes-data-dir"
            && !path.starts_with("spaces/")
            && !path.starts_with("uploads/")
            && !namespaces(m.format_version)
                .iter()
                .any(|n| path.starts_with(&format!("{n}/")))
        {
            return Err(invalid("Unknown owned file"));
        }
    }
    for (path, id) in &m.source_custody.files.0 {
        if id.inode == 0 || id.links == 0 {
            return Err(invalid("Invalid source file identity"));
        }
        groups.entry((id.device, id.inode)).or_default().push(path);
    }
    for group in groups.values() {
        let first = group[0];
        let id = &m.source_custody.files.0[first];
        if id.links < group.len() as u64
            || group.iter().any(|p| {
                m.files.0[*p] != m.files.0[first] || m.source_custody.files.0[*p].links != id.links
            })
        {
            return Err(invalid("Inconsistent source hardlink group"));
        }
    }
    let root = (m.source_custody.root.device, m.source_custody.root.inode);
    if groups.contains_key(&root) {
        return Err(invalid("Conflicting source root identity"));
    }
    let mut dirs = BTreeSet::new();
    for (path, id) in &m.source_custody.directories.0 {
        let key = (id.device, id.inode);
        if !safe(path)
            || !matches!(path.split('/').next(), Some("spaces" | "uploads"))
                && !namespaces(m.format_version)
                    .iter()
                    .any(|n| path == n || path.starts_with(&format!("{n}/")))
            || id.inode == 0
            || key == root
            || groups.contains_key(&key)
            || !dirs.insert(key)
        {
            return Err(invalid("Conflicting source directory identity"));
        }
    }
    Ok(())
}
/// Only the committed, still uninstalled create stage explains a missing mirror.
/// This is admission evidence; it neither publishes bytes nor adopts an inode.
#[cfg(target_os = "linux")]
async fn missing_document_is_pending(
    root: &Path,
    conn: &mut sqlx::AnyConnection,
    document: &str,
    path: &str,
) -> anyhow::Result<bool> {
    if path == DATABASE || !safe(path) {
        return Ok(false);
    }
    // Query failure (including an absent legacy receipt table) fails admission.
    let operations: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT op_id,completed_at FROM storage_journal_receipts WHERE document_id=$1 AND content_path=$2 AND state='pending'",
    )
    .bind(document)
    .bind(path)
    .fetch_all(&mut *conn)
    .await?;
    if operations.len() != 1 || operations[0].1.is_some() {
        return Ok(false);
    }
    let inspected = super::journal::inspect_restoration(root, conn, &operations[0].0)
        .await
        .map_err(|_| invalid("Missing document creation evidence differs"))?;
    let manifest = &inspected.manifest;
    let canonical_op =
        uuid::Uuid::parse_str(&manifest.op_id).is_ok_and(|id| id.to_string() == manifest.op_id);
    Ok(canonical_op
        && manifest.version == 1
        && (0..=6_000_000).contains(&manifest.length)
        && manifest.sha256.len() == 64
        && manifest
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        && inspected.raw_manifest.len() <= 65_536
        && inspected.class == super::journal::StorageOperationClass::CommittedPending
        && inspected.manifest.document_id == document
        && inspected.manifest.path == path
        && inspected.install.proof.is_none()
        && !inspected.install.installed_present)
}
#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::{FileExt, MetadataExt},
        },
        path::{Component, PathBuf},
    };
    fn open(parent: &File, name: &str, flags: i32) -> io::Result<File> {
        let name = CString::new(name).map_err(|_| invalid("Invalid rooted name"))?;
        // SAFETY: valid held directory and terminated component; successful FD has one owner.
        let fd = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0,
            )
        };
        if fd < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(fd) })
        }
    }
    fn identity(f: &File) -> io::Result<(u64, u64)> {
        let m = f.metadata()?;
        Ok((m.dev(), m.ino()))
    }
    type Fingerprint = (u64, u64, u64, u64, i64, i64, i64, i64);
    fn fingerprint(f: &File) -> io::Result<Fingerprint> {
        let m = f.metadata()?;
        Ok((
            m.dev(),
            m.ino(),
            m.len(),
            m.nlink(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec(),
        ))
    }
    struct Root {
        path: PathBuf,
        chain: Vec<File>,
    }
    impl Root {
        fn new(path: &Path) -> io::Result<Self> {
            let path = if path.is_absolute() {
                path.to_owned()
            } else {
                std::env::current_dir()?.join(path)
            };
            if path
                .components()
                .any(|p| !matches!(p, Component::RootDir | Component::Normal(_)))
            {
                return Err(invalid("Unsafe backup root"));
            }
            let mut chain = vec![File::open("/")?];
            for c in path.components() {
                if let Component::Normal(c) = c {
                    chain.push(open(
                        chain.last().unwrap(),
                        c.to_str().ok_or_else(|| invalid("Non-UTF8 root"))?,
                        libc::O_RDONLY | libc::O_DIRECTORY,
                    )?);
                }
            }
            Ok(Self { path, chain })
        }
        fn fd(&self) -> &File {
            self.chain.last().unwrap()
        }
        fn check(&self) -> io::Result<()> {
            let fresh = Self::new(&self.path)?;
            if self.chain.len() != fresh.chain.len() {
                return Err(invalid("Backup ancestry changed"));
            }
            for (a, b) in self.chain.iter().zip(&fresh.chain) {
                if identity(a)? != identity(b)? {
                    return Err(invalid("Backup ancestry changed"));
                }
            }
            Ok(())
        }
        fn leaf(&self, p: &str) -> io::Result<File> {
            if !safe(p) {
                return Err(invalid("Unsafe backup path"));
            }
            let mut fd = self.fd().try_clone()?;
            let v: Vec<_> = p.split('/').collect();
            for c in &v[..v.len() - 1] {
                fd = open(&fd, c, libc::O_RDONLY | libc::O_DIRECTORY)?;
            }
            let f = open(&fd, v[v.len() - 1], libc::O_RDONLY | libc::O_NONBLOCK)?;
            if !f.metadata()?.is_file() {
                return Err(invalid("Backup leaf is not regular"));
            }
            Ok(f)
        }
    }
    fn hash(f: &File) -> io::Result<Bytes> {
        hash_limit(f, u64::MAX)
    }
    fn hash_limit(f: &File, maximum: u64) -> io::Result<Bytes> {
        let before = fingerprint(f)?;
        if before.2 > maximum {
            return Err(invalid("Backup file exceeds read bound"));
        }
        let mut h = Sha256::new();
        let mut offset = 0;
        let mut chunk = [0; 65536];
        while offset < before.2 {
            let wanted = (before.2 - offset).min(65536) as usize;
            let n = f.read_at(&mut chunk[..wanted], offset)?;
            if n == 0 {
                return Err(invalid("Backup file truncated"));
            }
            h.update(&chunk[..n]);
            offset += n as u64;
        }
        if fingerprint(f)? != before {
            return Err(invalid("Backup file changed"));
        }
        Ok(Bytes {
            size_bytes: offset,
            sha256: hex::encode(h.finalize()),
        })
    }
    fn read(f: &File, max: u64) -> io::Result<Vec<u8>> {
        let size = f.metadata()?.len();
        if size == 0 || size > max {
            return Err(invalid("Backup manifest size invalid"));
        }
        let b = hash_limit(f, max)?;
        if b.size_bytes == 0 || b.size_bytes > max {
            return Err(invalid("Backup manifest size invalid"));
        }
        let mut out = vec![0; b.size_bytes as usize];
        let mut off = 0;
        while off < out.len() {
            let n = f.read_at(&mut out[off..], off as u64)?;
            if n == 0 {
                return Err(invalid("Backup manifest truncated"));
            }
            off += n;
        }
        if hex::encode(Sha256::digest(&out)) != b.sha256 || hash_limit(f, max)? != b {
            return Err(invalid("Backup manifest changed"));
        }
        Ok(out)
    }
    fn names(dir: &File) -> io::Result<Vec<String>> {
        std::fs::read_dir(format!("/proc/self/fd/{}", dir.as_raw_fd()))?
            .map(|e| {
                e?.file_name()
                    .into_string()
                    .map_err(|_| invalid("Non-UTF8 backup path"))
            })
            .collect()
    }
    fn walk(
        dir: &File,
        prefix: &str,
        files: &mut BTreeMap<String, Bytes>,
        directories: &mut BTreeSet<String>,
        physical: &mut BTreeMap<String, Fingerprint>,
    ) -> io::Result<()> {
        let before = fingerprint(dir)?;
        for n in names(dir)? {
            if !safe(&n) || n.contains('/') {
                return Err(invalid("Unsafe directory entry"));
            }
            let p = if prefix.is_empty() {
                n.clone()
            } else {
                format!("{prefix}/{n}")
            };
            let f = open(dir, &n, libc::O_RDONLY | libc::O_NONBLOCK)?;
            let md = f.metadata()?;
            if md.is_dir() {
                directories.insert(p.clone());
                physical.insert(p.clone(), fingerprint(&f)?);
                walk(&f, &p, files, directories, physical)?;
            } else if md.is_file() {
                let before = fingerprint(&f)?;
                files.insert(p.clone(), hash(&f)?);
                if fingerprint(&f)? != before {
                    return Err(invalid("Backup leaf changed during inventory"));
                }
                physical.insert(p, before);
            } else {
                return Err(invalid("Special backup file"));
            }
        }
        if fingerprint(dir)? != before {
            return Err(invalid("Backup directory changed during inventory"));
        }
        Ok(())
    }
    pub(crate) struct ValidatedBundle {
        root: Root,
        data: File,
        database: File,
        manifest_file: File,
        database_fingerprint: Fingerprint,
        manifest_fingerprint: Fingerprint,
        root_fingerprint: Fingerprint,
        data_fingerprint: Fingerprint,
        physical_fingerprints: BTreeMap<String, Fingerprint>,
        pub manifest: Manifest,
        pub raw_manifest: Vec<u8>,
        pub db: AnyPool,
    }
    impl ValidatedBundle {
        pub(crate) fn is_qualified_completed_capture(&self)->bool{self.manifest.format_version==4&&matches!(self.manifest.database.applied_version,53|54|55|56|57)}
        pub(crate) fn is_qualified_capacity_capture(&self) -> bool {
            self.manifest.format_version == 4 && matches!(self.manifest.database.applied_version,51|52|53|54|55|56|57)
        }
        /// Held copied metadata classification only; never original admission authority.
        pub(crate) fn metadata_designation(&self)->io::Result<Option<crate::storage::data_root_designation::Designation>> {
            self.fence()?;
            let token=crate::storage::data_root_designation::Designation::capture_held(&self.root.path.join("data"),&self.data)?;
            self.fence()?;Ok(token)
        }
        pub(crate) fn application_version(&self) -> &str {
            &self.manifest.application_version
        }
        /// Physical bundle boundaries for destination overlap checks. The
        /// normalized copied DATA root differs from captured original custody.
        pub(crate) fn copy_overlap_identities(&self) -> io::Result<[(u64,u64);2]> {
            self.root.check()?;
            Ok([identity(self.root.fd())?, identity(&self.data)?])
        }
        /// Pin only an admitted owned leaf; caller verifies its expected bytes
        /// and fences the complete bundle after copying.
        pub(crate) fn open_owned(&self, relative: &str) -> io::Result<File> {
            if !self.manifest.files.0.contains_key(relative) {return Err(invalid("Unowned copy source"));}
            self.root.check()?;
            let file=self.root.leaf(&format!("data/{relative}"))?;
            if self.physical_fingerprints.get(relative)!=Some(&fingerprint(&file)?) {return Err(invalid("Copy source identity or metadata changed"));}
            Ok(file)
        }
        /// Borrow exact original bundle kernel objects for a private worker handoff.
        /// These descriptors grant no source-current or verification record authority.
        pub(crate) fn verification_kernel_files(&self) -> io::Result<Vec<File>> {
            self.fence()?;
            let mut files=self.root.chain.iter().map(File::try_clone).collect::<io::Result<Vec<_>>>()?;
            files.push(self.data.try_clone()?);files.push(self.database.try_clone()?);files.push(self.manifest_file.try_clone()?);
            self.fence()?;Ok(files)
        }
        pub(crate) fn data_path(&self) -> PathBuf {
            self.root.path.join("data")
        }
        /// Recheck all bytes/topology after application receipt inspection and SQL waits.
        pub(crate) fn fence(&self) -> io::Result<()> {
            self.root.check()?;
            if fingerprint(self.root.fd())? != self.root_fingerprint
                || fingerprint(&self.database)? != self.database_fingerprint
                || fingerprint(&self.manifest_file)? != self.manifest_fingerprint
            {
                return Err(invalid("Backup admission metadata changed"));
            }
            let mut top = names(self.root.fd())?;
            top.sort();
            if top != ["data", "manifest.json"] {
                return Err(invalid("Backup top-level changed"));
            }
            let manifest = self.root.leaf("manifest.json")?;
            if identity(&manifest)? != identity(&self.manifest_file)?
                || read(&manifest, 16 * 1024 * 1024)? != self.raw_manifest
            {
                return Err(invalid("Backup manifest changed"));
            }
            let data = open(self.root.fd(), "data", libc::O_RDONLY | libc::O_DIRECTORY)?;
            if fingerprint(&data)? != self.data_fingerprint
                || identity(&data)? != identity(&self.data)?
                || fingerprint(&self.root.leaf("data/starlane-notes.db")?)?
                    != self.database_fingerprint
                || identity(&self.root.leaf("data/starlane-notes.db")?)?
                    != identity(&self.database)?
            {
                return Err(invalid("Backup root/database changed"));
            }
            let mut files = BTreeMap::new();
            let mut dirs = BTreeSet::new();
            let mut physical = BTreeMap::new();
            walk(&data, "", &mut files, &mut dirs, &mut physical)?;
            if files != self.manifest.files.0
                || dirs
                    != self
                        .manifest
                        .source_custody
                        .directories
                        .0
                        .keys()
                        .cloned()
                        .collect()
                || physical != self.physical_fingerprints
            {
                return Err(invalid("Backup bytes/topology changed"));
            }
            if fingerprint(&self.database)? != self.database_fingerprint
                || fingerprint(self.root.fd())? != self.root_fingerprint
            {
                return Err(invalid("Backup changed during final fence"));
            }
            self.root.check()
        }
    }
    pub(crate) async fn verify(path: &Path) -> anyhow::Result<ValidatedBundle> {
        let root = Root::new(path)?;
        let mut top = names(root.fd())?;
        top.sort();
        if top != ["data", "manifest.json"] {
            return Err(invalid("Incomplete backup bundle").into());
        }
        let manifest_file = root.leaf("manifest.json")?;
        let raw_manifest = read(&manifest_file, 16 * 1024 * 1024)?;
        let manifest: Manifest = serde_json::from_slice(&raw_manifest)?;
        validate(&manifest)?;
        let data = open(root.fd(), "data", libc::O_RDONLY | libc::O_DIRECTORY)?;
        let database = root.leaf("data/starlane-notes.db")?;
        // Pin physical observations BEFORE opening SQLite's immutable pager. Bytes
        // changed then restored can otherwise leave stale altered SQL pages cached.
        let database_fingerprint = fingerprint(&database)?;
        let manifest_fingerprint = fingerprint(&manifest_file)?;
        let root_fingerprint = fingerprint(root.fd())?;
        let data_fingerprint = fingerprint(&data)?;
        let mut files = BTreeMap::new();
        let mut dirs = BTreeSet::new();
        let mut physical_fingerprints = BTreeMap::new();
        walk(&data, "", &mut files, &mut dirs, &mut physical_fingerprints)?;
        if files != manifest.files.0
            || dirs
                != manifest
                    .source_custody
                    .directories
                    .0
                    .keys()
                    .cloned()
                    .collect()
            || fingerprint(&database)? != database_fingerprint
            || fingerprint(root.fd())? != root_fingerprint
            || fingerprint(&data)? != data_fingerprint
            || fingerprint(&manifest_file)? != manifest_fingerprint
        {
            return Err(invalid("Backup changed during admission").into());
        }
        sqlx::any::install_default_drivers();
        let db = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&format!(
                "sqlite:///proc/self/fd/{}?mode=ro&immutable=true",
                database.as_raw_fd()
            ))
            .await?;
        let bundle = ValidatedBundle {
            root,
            data,
            database,
            manifest_file,
            database_fingerprint,
            manifest_fingerprint,
            root_fingerprint,
            data_fingerprint,
            physical_fingerprints,
            manifest,
            raw_manifest,
            db,
        };
        bundle.fence()?;
        let mut conn = bundle.db.acquire().await?;
        sqlx::query("PRAGMA query_only=ON")
            .execute(&mut *conn)
            .await?;
        let checks: Vec<String> = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_all(&mut *conn)
            .await?;
        if checks != ["ok"]
            || !sqlx::query("PRAGMA foreign_key_check")
                .fetch_all(&mut *conn)
                .await?
                .is_empty()
        {
            return Err(invalid("Backup database integrity failed").into());
        }
        static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
        let rows=sqlx::query("SELECT version,checksum,CASE WHEN typeof(success)='integer' AND success=1 THEN 1 ELSE 0 END AS succeeded FROM _sqlx_migrations ORDER BY version").fetch_all(&mut *conn).await?;
        if rows.is_empty()
            || rows.len() != bundle.manifest.database.migrations.len()
            || rows.len() > MIGRATOR.iter().count()
        {
            return Err(invalid("Migration history differs").into());
        }
        for ((row, record), known) in rows
            .iter()
            .zip(&bundle.manifest.database.migrations)
            .zip(MIGRATOR.iter())
        {
            let version: i64 = row.try_get("version")?;
            let checksum: Vec<u8> = row.try_get("checksum")?;
            if version != record.version
                || version != known.version
                || row.try_get::<i64, _>("succeeded")? != 1
                || checksum != known.checksum.as_ref()
                || hex::encode(&checksum) != record.checksum_sha384
                || !super::hex(&record.checksum_sha384, 96)
            {
                return Err(invalid("Migration checksum/version differs").into());
            }
        }
        if bundle.manifest.database.applied_version
            != bundle
                .manifest
                .database
                .migrations
                .last()
                .map_or(0, |m| m.version)
            || sqlx::query_scalar::<_, i64>("PRAGMA user_version")
                .fetch_one(&mut *conn)
                .await?
                != bundle.manifest.database.user_version
        {
            return Err(invalid("Database schema metadata differs").into());
        }
        for row in sqlx::query("SELECT id,content_path FROM documents")
            .fetch_all(&mut *conn)
            .await?
        {
            let document: String = row.try_get("id")?;
            let path: String = row.try_get("content_path")?;
            if path == DATABASE
                || !safe(&path)
                || (!bundle.manifest.files.0.contains_key(&path)
                    && !missing_document_is_pending(
                        &bundle.data_path(),
                        &mut conn,
                        &document,
                        &path,
                    )
                    .await?)
            {
                return Err(invalid("Database references missing or unsafe content").into());
            }
        }
        for (table, column) in [
            ("attachments", "storage_path"),
            ("excalidraw_files", "storage_path"),
        ] {
            let paths: Vec<String> = sqlx::query_scalar(&format!("SELECT {column} FROM {table}"))
                .fetch_all(&mut *conn)
                .await?;
            if paths
                .iter()
                .any(|p| p == DATABASE || !safe(p) || !bundle.manifest.files.0.contains_key(p))
            {
                return Err(invalid("Database references missing or unsafe content").into());
            }
        }
        for row in sqlx::query("SELECT storage_path,size_bytes FROM attachments")
            .fetch_all(&mut *conn)
            .await?
        {
            let p: String = row.try_get("storage_path")?;
            let size: i64 = row.try_get("size_bytes")?;
            if size < 0 || bundle.manifest.files.0[&p].size_bytes != size as u64 {
                return Err(invalid("Attachment size differs").into());
            }
        }
        let tables: BTreeSet<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table'")
                .fetch_all(&mut *conn)
                .await?
                .into_iter()
                .collect();
        let mut counts = Vec::new();
        for (table, created_version) in RECEIPTS.into_iter().zip([29, 30, 31, 39, 32, 48, 48, 49]).chain(if bundle.manifest.format_version==4 {vec![("attachment_capacity_reservations",51),("attachment_retained_byte_ledger",51),("attachment_erasure_outbox",52),("attachment_erasure_dispositions",53),("draft_publication_receipts",54),("document_publication_snapshots",54),("snapshot_recovery_receipts",54),("attachment_audience_origins",55),("attachment_audience_bindings",55),("draft_transfer_receipts",56),("draft_discussions",57),("draft_discussion_participants",57),("draft_discussion_comments",57),("draft_discussion_subscriptions",57)]}else{vec![]}) {
            if bundle.manifest.database.applied_version >= created_version
                && !tables.contains(table)
            {
                return Err(invalid("Applied migration receipt table is missing").into());
            }
            if tables.contains(table) {
                let n: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
                    .fetch_one(&mut *conn)
                    .await?;
                if n > 0 {
                    counts.push(ReceiptCount {
                        table: table.into(),
                        rows: n,
                    });
                }
            }
        }
        let mut namespace_inventory: Vec<String> = namespaces(bundle.manifest.format_version)
            .iter()
            .filter(|n| {
                bundle
                    .manifest
                    .source_custody
                    .directories
                    .0
                    .contains_key(**n)
            })
            .map(|n| n.to_string())
            .collect();
        namespace_inventory.sort();
        let dirs: Vec<String> = bundle
            .manifest
            .source_custody
            .directories
            .0
            .keys()
            .filter(|p| {
                namespaces(bundle.manifest.format_version)
                    .iter()
                    .any(|n| *p == n || p.starts_with(&format!("{n}/")))
            })
            .cloned()
            .collect();
        let boundary = &bundle.manifest.recovery_boundary;
        let policy = if namespace_inventory.is_empty() && counts.is_empty() {
            "allowed"
        } else {
            "blocked-identity-bound-evidence"
        };
        if boundary.scope != "storage-bytes-only"
            || boundary.ordinary_restore != policy
            || boundary.journal_namespaces != namespace_inventory
            || boundary.journal_directories != dirs
            || boundary.receipt_families != counts
        {
            return Err(invalid("Captured recovery boundary differs").into());
        }
        drop(conn);
        bundle.fence()?;
        Ok(bundle)
    }
}
#[cfg(target_os = "linux")]
pub(crate) use linux::{verify, ValidatedBundle};
#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use sqlx::Connection;
    use std::{os::unix::fs::MetadataExt, path::PathBuf};
    // DATA_DIR is process-local: the child invokes the real public producer,
    // with actual migrated SQL and commit, without changing the test host env.
    #[tokio::test]
    async fn missing_document_stage_producer_child() {
        let Ok(root) = std::env::var("STARLANE_MISSING_DOCUMENT_FIXTURE_ROOT") else {
            return;
        };
        sqlx::any::install_default_drivers();
        let db = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite://{root}/starlane-notes.db?mode=rwc"))
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let mut tx = db.begin().await.unwrap();
        for query in [
            "INSERT INTO users(id,email,display_name) VALUES('u','missing@test','User')",
            "INSERT INTO spaces(id,name,slug,created_by) VALUES('s','Space','s','u')",
            "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at) VALUES('d','s','Doc','temporary','u','u','initial')",
            "INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('draft','d','s','u','Doc','Private body','initial')",
        ] { sqlx::query(query).execute(&mut *tx).await.unwrap(); }
        let m = crate::storage::journal::stage_document(
            &mut tx,
            "s",
            "d",
            "draft",
            "initial",
            "s",
            "Doc",
            "",
            "Private body",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE documents SET content_path=$1 WHERE id='d'")
            .bind(&m.path)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        db.close().await;
    }
    #[tokio::test]
    async fn missing_document_admission_requires_real_single_uninstalled_pending_stage() {
        use std::{
            os::unix::fs::DirBuilderExt,
            process::{Command, Stdio},
            time::{Duration, Instant},
        };
        let root = std::env::temp_dir().join(format!("missing-document-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new()
            .mode(0o700)
            .create(&root)
            .unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "storage::backup_inventory::tests::missing_document_stage_producer_child",
                "--nocapture",
            ])
            .env("STARLANE_MISSING_DOCUMENT_FIXTURE_ROOT", &root)
            .env("DATA_DIR", &root)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            if start.elapsed() > Duration::from_secs(30) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("Producer child timed out");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        sqlx::any::install_default_drivers();
        let db = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&format!(
                "sqlite://{}?mode=rw",
                root.join(DATABASE).display()
            ))
            .await
            .unwrap();
        let mut conn = db.acquire().await.unwrap();
        let path: String = sqlx::query_scalar("SELECT content_path FROM documents WHERE id='d'")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert!(!root.join(&path).exists());
        assert!(missing_document_is_pending(&root, &mut conn, "d", &path)
            .await
            .unwrap());
        assert!(
            !missing_document_is_pending(&root, &mut conn, "other", &path)
                .await
                .unwrap()
        );
        assert!(
            !missing_document_is_pending(&root, &mut conn, "d", "../unsafe")
                .await
                .unwrap()
        );
        for query in [
            "UPDATE documents SET updated_at='changed' WHERE id='d'",
            "UPDATE spaces SET slug='changed' WHERE id='s'",
            "DELETE FROM document_drafts WHERE id='draft'",
            "UPDATE storage_journal_receipts SET sha256=printf('%064d',0)",
        ] {
            let mut tx = conn.begin().await.unwrap();
            sqlx::query(query).execute(&mut *tx).await.unwrap();
            assert!(
                missing_document_is_pending(&root, &mut tx, "d", &path)
                    .await
                    .is_err(),
                "{query}"
            );
            tx.rollback().await.unwrap();
        }
        for query in [
            "UPDATE storage_journal_receipts SET completed_at='unexpected'",
            "UPDATE storage_journal_receipts SET state='complete'",
            "INSERT INTO storage_journal_receipts(op_id,space_id,document_id,draft_id,content_path,initial_revision,sha256,byte_length,state) SELECT 'second',space_id,document_id,draft_id,content_path,initial_revision,sha256,byte_length,state FROM storage_journal_receipts",
            "INSERT INTO storage_journal_receipts(op_id,space_id,document_id,draft_id,content_path,initial_revision,sha256,byte_length,state,completed_at) SELECT 'second',space_id,document_id,draft_id,content_path,initial_revision,sha256,byte_length,state,'unexpected' FROM storage_journal_receipts",
        ] {
            let mut tx=conn.begin().await.unwrap();
            sqlx::query(query).execute(&mut *tx).await.unwrap();
            assert!(!missing_document_is_pending(&root,&mut tx,"d",&path).await.unwrap(),"{query}");
            tx.rollback().await.unwrap();
        }
        let op: String = sqlx::query_scalar("SELECT op_id FROM storage_journal_receipts")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        let inspected = crate::storage::journal::inspect_restoration(&root, &mut conn, &op)
            .await
            .unwrap();
        crate::storage::journal_fs::publish_file(
            &root,
            ".storage-journal",
            &(&inspected.manifest).into(),
        )
        .unwrap();
        std::fs::remove_file(root.join(&path)).unwrap();
        assert!(!missing_document_is_pending(&root, &mut conn, "d", &path)
            .await
            .unwrap());
        sqlx::query("DROP TABLE storage_journal_receipts")
            .execute(&mut *conn)
            .await
            .unwrap();
        assert!(missing_document_is_pending(&root, &mut conn, "d", &path)
            .await
            .is_err());
        drop(conn);
        db.close().await;
        std::fs::remove_dir_all(root).unwrap();
    }
    struct Fixture {
        root: PathBuf,
        value: serde_json::Value,
    }
    impl Fixture {
        async fn new() -> Self {
            sqlx::any::install_default_drivers();
            let root =
                std::env::temp_dir().join(format!("backup-admission-{}", uuid::Uuid::new_v4()));
            let data = root.join("data");
            std::fs::create_dir_all(data.join("spaces/team/docs")).unwrap();
            let db = sqlx::any::AnyPoolOptions::new()
                .max_connections(1)
                .connect(&format!(
                    "sqlite://{}?mode=rwc",
                    data.join(DATABASE).display()
                ))
                .await
                .unwrap();
            sqlx::migrate!("./migrations").run(&db).await.unwrap();
            db.close().await;
            std::fs::write(
                data.join("spaces/team/docs/page.md"),
                b"Independent fixture bytes",
            )
            .unwrap();
            let mut files = serde_json::Map::new();
            for p in [DATABASE, "spaces/team/docs/page.md"] {
                let bytes = std::fs::read(data.join(p)).unwrap();
                files.insert(p.into(),serde_json::json!({"size_bytes":bytes.len(),"sha256":hex::encode(Sha256::digest(&bytes))}));
            }
            let identity = |p: &Path| {
                let m = std::fs::metadata(p).unwrap();
                serde_json::json!({"device":m.dev(),"inode":m.ino()})
            };
            let mut dirs = serde_json::Map::new();
            for p in ["spaces", "spaces/team", "spaces/team/docs"] {
                dirs.insert(p.into(), identity(&data.join(p)));
            }
            let md = std::fs::metadata(data.join("spaces/team/docs/page.md")).unwrap();
            let migrations:Vec<_>=sqlx::migrate!("./migrations").iter().map(|m|serde_json::json!({"version":m.version,"checksum_sha384":hex::encode(&m.checksum)})).collect();
            let value = serde_json::json!({"format":"starlane-notes-stopped-write-backup","format_version":if matches!(migrations.last().unwrap()["version"].as_i64(),Some(51|52|53|54|55|56|57)){4}else{3},"backup_id":uuid::Uuid::new_v4().to_string(),"application_version":"1.5.0","capture_started_at":"2026-10-09T12:00:00+00:00","capture_finished_at":"2026-10-09T12:01:00+00:00","writers_stopped_asserted":true,"database":{"engine":"sqlite","applied_version":migrations.last().unwrap()["version"],"user_version":0,"migrations":migrations},"files":files,"excluded":["logging","logs","search_index"],"configuration_prerequisite":PREREQUISITE,"recovery_boundary":{"scope":"storage-bytes-only","ordinary_restore":"allowed","journal_namespaces":[],"journal_directories":[],"receipt_families":[]},"source_custody":{"version":1,"root":identity(&data),"files":{"spaces/team/docs/page.md":{"device":md.dev(),"inode":md.ino(),"links":md.nlink()}},"directories":dirs}});
            let f = Self { root, value };
            f.write();
            f
        }
        fn write(&self) {
            std::fs::write(
                self.root.join("manifest.json"),
                serde_json::to_vec(&self.value).unwrap(),
            )
            .unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    #[tokio::test]
    async fn authentic_schema_snapshot_is_read_only_and_late_byte_change_is_fenced() {
        let f = Fixture::new().await;
        let bundle = verify(&f.root).await.unwrap();
        assert!(sqlx::query("CREATE TABLE forbidden_write(id TEXT)")
            .execute(&bundle.db)
            .await
            .is_err());
        bundle.fence().unwrap();
        std::fs::write(
            f.root.join("data/spaces/team/docs/page.md"),
            b"changed content",
        )
        .unwrap();
        assert!(bundle.fence().is_err());
        bundle.db.close().await;
    }
    #[tokio::test]
    async fn bundle_topology_database_identity_and_symlink_changes_are_rejected() {
        let f = Fixture::new().await;
        let b = verify(&f.root).await.unwrap();
        std::fs::write(f.root.join("unexpected"), b"unexpected").unwrap();
        assert!(b.fence().is_err());
        std::fs::remove_file(f.root.join("unexpected")).unwrap();
        let db = f.root.join("data").join(DATABASE);
        std::fs::copy(&db, f.root.join("replacement")).unwrap();
        std::fs::rename(f.root.join("replacement"), &db).unwrap();
        assert!(b.fence().is_err());
        b.db.close().await;
        let g = Fixture::new().await;
        let p = g.root.join("data/spaces/team/docs/page.md");
        std::fs::rename(&p, g.root.join("original")).unwrap();
        std::os::unix::fs::symlink(g.root.join("original"), &p).unwrap();
        assert!(verify(&g.root).await.is_err());
    }
    #[tokio::test]
    async fn full_contract_unknown_fields_bad_history_and_false_boundary_are_rejected() {
        for field in [
            "unknown",
            "history",
            "boundary",
            "links",
            "directory",
            "writers",
        ] {
            let mut f = Fixture::new().await;
            match field {
                "unknown" => f.value["unexpected"] = true.into(),
                "history" => {
                    f.value["database"]["migrations"][0]["checksum_sha384"] = "0".repeat(96).into()
                }
                "boundary" => {
                    f.value["recovery_boundary"]["ordinary_restore"] =
                        "allowed-with-forged-custody".into()
                }
                "links" => {
                    f.value["source_custody"]["files"]["spaces/team/docs/page.md"]["links"] =
                        0.into()
                }
                "directory" => {
                    f.value["source_custody"]["directories"]
                        .as_object_mut()
                        .unwrap()
                        .remove("spaces/team/docs");
                }
                _ => f.value["writers_stopped_asserted"] = false.into(),
            };
            f.write();
            assert!(verify(&f.root).await.is_err(), "{field}");
        }
    }
    #[test]
    fn duplicate_inventory_and_numeric_boolean_identity_are_rejected() {
        assert!(serde_json::from_str::<UniqueMap<Bytes>>(
            r#"{"a":{"size_bytes":0,"sha256":"x"},"a":{"size_bytes":0,"sha256":"x"}}"#
        )
        .is_err());
        assert!(serde_json::from_str::<Identity>(r#"{"device":true,"inode":1}"#).is_err());
    }
    #[tokio::test]
    async fn same_inode_database_change_then_exact_byte_restoration_is_rejected() {
        use std::os::unix::fs::FileExt;
        let f = Fixture::new().await;
        let bundle = verify(&f.root).await.unwrap();
        let p = f.root.join("data").join(DATABASE);
        let original = std::fs::read(&p).unwrap();
        let inode = p.metadata().unwrap().ino();
        let writer = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        writer.write_at(&[original[0] ^ 1], 0).unwrap();
        writer.sync_all().unwrap();
        writer.write_at(&original[..1], 0).unwrap();
        writer.sync_all().unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), original);
        assert_eq!(p.metadata().unwrap().ino(), inode);
        assert!(bundle.fence().is_err());
        bundle.db.close().await;
    }
    #[tokio::test]
    async fn same_bytes_restored_content_and_directory_replacement_are_rejected() {
        let f = Fixture::new().await;
        let bundle = verify(&f.root).await.unwrap();
        let p = f.root.join("data/spaces/team/docs/page.md");
        let original = std::fs::read(&p).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        std::fs::write(&p, b"changed temporarily").unwrap();
        std::fs::write(&p, &original).unwrap();
        assert!(bundle.fence().is_err());
        bundle.db.close().await;
        let g = Fixture::new().await;
        let bundle = verify(&g.root).await.unwrap();
        let parent = g.root.join("data/spaces/team/docs");
        let old = g.root.join("data/spaces/team/old-docs");
        std::fs::rename(&parent, &old).unwrap();
        std::fs::create_dir(&parent).unwrap();
        std::fs::copy(old.join("page.md"), parent.join("page.md")).unwrap();
        std::fs::remove_dir_all(old).unwrap();
        assert!(bundle.fence().is_err());
        bundle.db.close().await;
    }
    #[tokio::test]
    async fn noncanonical_ids_and_invalid_version_suffixes_are_rejected() {
        let mut f = Fixture::new().await;
        for value in [
            uuid::Uuid::new_v4().simple().to_string(),
            uuid::Uuid::new_v4().to_string().to_uppercase(),
        ] {
            f.value["backup_id"] = value.into();
            f.write();
            assert!(verify(&f.root).await.is_err());
        }
        f.value["backup_id"] = uuid::Uuid::new_v4().to_string().into();
        for value in [
            "1.5.0-",
            "1.5.0+",
            "1.5.0+foo+bar",
            "1.5.0-foo+bar",
            "1.5.0.1",
        ] {
            f.value["application_version"] = value.into();
            f.write();
            assert!(verify(&f.root).await.is_err(), "{value}");
        }
        for value in ["v1.5.0", "1.5.0-alpha.1", "1.5.0+build-1"] {
            f.value["application_version"] = value.into();
            f.write();
            let bundle = verify(&f.root).await.unwrap();
            bundle.db.close().await;
        }
    }
    #[tokio::test]
    async fn migration_success_must_be_exact_integer_one() {
        for success in [2, -1, 0] {
            let mut f = Fixture::new().await;
            let path = f.root.join("data").join(DATABASE);
            let db = sqlx::any::AnyPoolOptions::new()
                .max_connections(1)
                .connect(&format!("sqlite://{}?mode=rw", path.display()))
                .await
                .unwrap();
            sqlx::query("UPDATE _sqlx_migrations SET success=? WHERE version=1")
                .bind(success)
                .execute(&db)
                .await
                .unwrap();
            db.close().await;
            let bytes = std::fs::read(&path).unwrap();
            f.value["files"][DATABASE] = serde_json::json!({"size_bytes":bytes.len(),"sha256":hex::encode(Sha256::digest(bytes))});
            f.write();
            assert!(verify(&f.root).await.is_err(), "{success}");
        }
    }
    #[tokio::test]
    async fn oversized_sparse_manifest_rejected_before_any_full_hash() {
        let f = Fixture::new().await;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .open(f.root.join("manifest.json"))
            .unwrap();
        file.set_len(16 * 1024 * 1024 + 1).unwrap();
        assert!(verify(&f.root).await.is_err());
    }

    #[tokio::test]
    async fn dropped_receipt_tables_cannot_masquerade_as_empty_recovery_boundary() {
        for table in RECEIPTS {
            let mut f = Fixture::new().await;
            let path = f.root.join("data").join(DATABASE);
            let db = sqlx::any::AnyPoolOptions::new()
                .max_connections(1)
                .connect(&format!("sqlite://{}?mode=rw", path.display()))
                .await
                .unwrap();
            sqlx::query(&format!("DROP TABLE {table}"))
                .execute(&db)
                .await
                .unwrap();
            db.close().await;
            let bytes = std::fs::read(&path).unwrap();
            f.value["files"][DATABASE] = serde_json::json!({"size_bytes":bytes.len(),"sha256":hex::encode(Sha256::digest(bytes))});
            f.write();
            assert!(verify(&f.root).await.is_err(), "{table}");
        }
    }
    #[tokio::test]
    async fn original_partial_admission_cold_bundle_requires_exact_custody_and_owned_snapshot() {
        use crate::storage::{attachment_upload as upload,attachment_capacity as capacity,restoration::admission};
        use std::os::unix::fs::{PermissionsExt,DirBuilderExt};
        let mut fixture=Fixture::new().await;let data=fixture.root.join("data");std::fs::set_permissions(&data,std::fs::Permissions::from_mode(0o700)).unwrap();
        let url=format!("sqlite://{}?mode=rw",data.join(DATABASE).display());let db=sqlx::any::AnyPoolOptions::new().max_connections(2).connect(&url).await.unwrap();
        let attempt=uuid::Uuid::new_v4().to_string();let req=upload::UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"browser".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),original_name:"x.bin".into(),mime_type:"application/octet-stream".into()};
        let writer=upload::admission::WriterLease::begin(&data,&attempt,upload::admission::Producer::Direct{request:req.clone()},64).unwrap();capacity::reserve(&db,&writer,None).await.unwrap();let mut ingest=upload::Ingest::reserve_with_lease(&data,req,64,writer.clone()).unwrap();ingest.append(b"authentic original partial stage").unwrap();drop(ingest);drop(writer);
        let row=admission::CapturedReservation{operation_id:attempt.clone(),request_sha256:sqlx::query_scalar("SELECT request_sha256 FROM attachment_capacity_reservations").fetch_one(&db).await.unwrap(),admission_claim_sha256:sqlx::query_scalar("SELECT admission_claim_sha256 FROM attachment_capacity_reservations").fetch_one(&db).await.unwrap(),maximum_bytes:64,state:"reserved".into(),committed_asset_id:None,committed_bytes:None,disposition_json:None,disposition_sha256:None};
        let mut tx=db.begin().await.unwrap();let leases=admission::capture::NativeCaptureLeases::hold(&data,&mut *tx,&[row]).await.unwrap();let source=leases.classify(&mut tx).await.unwrap();let capture=source.capture_envelope().unwrap();source.fence().unwrap();drop(source);drop(leases);tx.rollback().await.unwrap();db.close().await;
        fn inventory(base:&Path,current:&Path,files:&mut serde_json::Map<String,serde_json::Value>,custody:&mut serde_json::Map<String,serde_json::Value>,dirs:&mut serde_json::Map<String,serde_json::Value>) {
            for e in std::fs::read_dir(current).unwrap(){let p=e.unwrap().path();let rel=p.strip_prefix(base).unwrap().to_str().unwrap().to_owned();let m=std::fs::metadata(&p).unwrap();if m.is_dir(){dirs.insert(rel,serde_json::json!({"device":m.dev(),"inode":m.ino()}));inventory(base,&p,files,custody,dirs);}else{let raw=std::fs::read(&p).unwrap();files.insert(rel.clone(),serde_json::json!({"size_bytes":raw.len(),"sha256":hex::encode(Sha256::digest(&raw))}));if rel!=DATABASE{custody.insert(rel,serde_json::json!({"device":m.dev(),"inode":m.ino(),"links":m.nlink()}));}}}
        }
        let mut files=serde_json::Map::new();let mut custody=serde_json::Map::new();let mut dirs=serde_json::Map::new();inventory(&data,&data,&mut files,&mut custody,&mut dirs);
        fixture.value["files"]=serde_json::Value::Object(files);fixture.value["source_custody"]["files"]=serde_json::Value::Object(custody);fixture.value["source_custody"]["directories"]=serde_json::Value::Object(dirs.clone());fixture.value["admission_owner_capture"]=serde_json::to_value(&capture).unwrap();
        let namespaces=vec![".attachment-upload-admission",".attachment-upload-journal"];let journal_dirs:Vec<_>=dirs.keys().filter(|p|namespaces.iter().any(|n|p.as_str()==*n||p.starts_with(&format!("{n}/")))).cloned().collect();fixture.value["recovery_boundary"]=serde_json::json!({"scope":"storage-bytes-only","ordinary_restore":"blocked-identity-bound-evidence","journal_namespaces":namespaces,"journal_directories":journal_dirs,"receipt_families":[{"table":"attachment_capacity_reservations","rows":1}]});
        fn copy_tree(src:&Path,dst:&Path){std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dst).unwrap();for e in std::fs::read_dir(src).unwrap(){let p=e.unwrap().path();let target=dst.join(p.file_name().unwrap());if p.is_dir(){copy_tree(&p,&target)}else{std::fs::copy(&p,&target).unwrap();std::fs::set_permissions(&target,std::fs::Permissions::from_mode(0o600)).unwrap();}}}
        let original=std::env::temp_dir().join(format!("cold-admission-original-{}",uuid::Uuid::new_v4()));std::fs::rename(&data,&original).unwrap();copy_tree(&original,&data);std::fs::remove_dir_all(&original).unwrap();fixture.write();
        let bundle=std::sync::Arc::new(verify(&fixture.root).await.unwrap());let cold=admission::ClassifiedAdmissionSource::cold_bundle(bundle.clone()).await.unwrap();cold.fence().unwrap();assert_eq!(cold.source_roles().scopes[0].attempt_id,attempt);
        let payload=data.join(format!(".attachment-upload-journal/{attempt}/payload.part"));let kept=fixture.root.parent().unwrap().join(format!("cold-kept-{}",uuid::Uuid::new_v4()));std::fs::rename(&payload,&kept).unwrap();std::fs::copy(&kept,&payload).unwrap();assert!(cold.fence().is_err(),"equal-byte foreign owned snapshot payload must refuse");std::fs::remove_file(&kept).unwrap();drop(cold);drop(bundle);
        let origin_rel=format!(".attachment-upload-admission/{attempt}/stage-origin.json");let origin_path=data.join(&origin_rel);let mut origin:serde_json::Value=serde_json::from_slice(&std::fs::read(&origin_path).unwrap()).unwrap();origin["payload"]["inode"]=serde_json::json!(999);let raw=serde_json::to_vec(&origin).unwrap();std::fs::write(&origin_path,&raw).unwrap();let sha=hex::encode(Sha256::digest(&raw));fixture.value["files"][&origin_rel]=serde_json::json!({"size_bytes":raw.len(),"sha256":sha});
        for binding in fixture.value["admission_owner_capture"]["scopes"][0]["files"].as_array_mut().unwrap(){if binding["role"]["role"]=="capsule_record"&&binding["role"]["name"]=="stage-origin.json"{binding["length"]=serde_json::json!(raw.len());binding["sha256"]=serde_json::json!(sha);}}
        fixture.write();let malformed=std::sync::Arc::new(verify(&fixture.root).await.unwrap());assert!(admission::ClassifiedAdmissionSource::cold_bundle(malformed).await.is_err(),"self-consistent hashes cannot replace original stage payload custody");
        fixture.value["admission_owner_capture"]["scopes"][0]["directories"][0]["captured"]["inode"]=serde_json::json!(999);fixture.write();assert!(verify(&fixture.root).await.is_err(),"foreign original capture directory refuses before ownership");
    }
    #[tokio::test]
    async fn large_partial_direct_100_mib_cold_bundle_streams_full_proof_and_refuses_foreign_custody() {
        use crate::storage::{attachment_upload as upload,attachment_capacity as capacity,restoration::admission};
        use std::os::unix::fs::{PermissionsExt,DirBuilderExt};
        let mut fixture=Fixture::new().await;let data=fixture.root.join("data");std::fs::set_permissions(&data,std::fs::Permissions::from_mode(0o700)).unwrap();
        let url=format!("sqlite://{}?mode=rw",data.join(DATABASE).display());let db=sqlx::any::AnyPoolOptions::new().max_connections(2).connect(&url).await.unwrap();
        let attempt=uuid::Uuid::new_v4().to_string();let req=upload::UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"browser".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),original_name:"x.bin".into(),mime_type:"application/octet-stream".into()};
        let writer=upload::admission::WriterLease::begin(&data,&attempt,upload::admission::Producer::Direct{request:req.clone()},128*1024*1024).unwrap();capacity::reserve(&db,&writer,None).await.unwrap();let mut ingest=upload::Ingest::reserve_with_lease(&data,req,128*1024*1024,writer.clone()).unwrap();let chunk=[0x5au8;64*1024];for _ in 0..1600 {ingest.append(&chunk).unwrap();}drop(ingest);drop(writer);
        let row=admission::CapturedReservation{operation_id:attempt.clone(),request_sha256:sqlx::query_scalar("SELECT request_sha256 FROM attachment_capacity_reservations").fetch_one(&db).await.unwrap(),admission_claim_sha256:sqlx::query_scalar("SELECT admission_claim_sha256 FROM attachment_capacity_reservations").fetch_one(&db).await.unwrap(),maximum_bytes:128*1024*1024,state:"reserved".into(),committed_asset_id:None,committed_bytes:None,disposition_json:None,disposition_sha256:None};
        let mut tx=db.begin().await.unwrap();let leases=admission::capture::NativeCaptureLeases::hold(&data,&mut *tx,&[row]).await.unwrap();let source=leases.classify(&mut tx).await.unwrap();let capture=source.capture_envelope().unwrap();source.fence().unwrap();drop(source);drop(leases);tx.rollback().await.unwrap();db.close().await;
        fn inventory(base:&Path,current:&Path,files:&mut serde_json::Map<String,serde_json::Value>,custody:&mut serde_json::Map<String,serde_json::Value>,dirs:&mut serde_json::Map<String,serde_json::Value>) {
            for e in std::fs::read_dir(current).unwrap(){let p=e.unwrap().path();let rel=p.strip_prefix(base).unwrap().to_str().unwrap().to_owned();let m=std::fs::metadata(&p).unwrap();if m.is_dir(){dirs.insert(rel,serde_json::json!({"device":m.dev(),"inode":m.ino()}));inventory(base,&p,files,custody,dirs);}else{let raw=std::fs::read(&p).unwrap();files.insert(rel.clone(),serde_json::json!({"size_bytes":raw.len(),"sha256":hex::encode(Sha256::digest(&raw))}));if rel!=DATABASE{custody.insert(rel,serde_json::json!({"device":m.dev(),"inode":m.ino(),"links":m.nlink()}));}}}
        }
        let mut files=serde_json::Map::new();let mut custody=serde_json::Map::new();let mut dirs=serde_json::Map::new();inventory(&data,&data,&mut files,&mut custody,&mut dirs);
        fixture.value["files"]=serde_json::Value::Object(files);fixture.value["source_custody"]["files"]=serde_json::Value::Object(custody);fixture.value["source_custody"]["directories"]=serde_json::Value::Object(dirs.clone());fixture.value["admission_owner_capture"]=serde_json::to_value(&capture).unwrap();
        let namespaces=vec![".attachment-upload-admission",".attachment-upload-journal"];let journal_dirs:Vec<_>=dirs.keys().filter(|p|namespaces.iter().any(|n|p.as_str()==*n||p.starts_with(&format!("{n}/")))).cloned().collect();fixture.value["recovery_boundary"]=serde_json::json!({"scope":"storage-bytes-only","ordinary_restore":"blocked-identity-bound-evidence","journal_namespaces":namespaces,"journal_directories":journal_dirs,"receipt_families":[{"table":"attachment_capacity_reservations","rows":1}]});
        fn copy_tree(src:&Path,dst:&Path){std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dst).unwrap();for e in std::fs::read_dir(src).unwrap(){let p=e.unwrap().path();let target=dst.join(p.file_name().unwrap());if p.is_dir(){copy_tree(&p,&target)}else{std::fs::copy(&p,&target).unwrap();std::fs::set_permissions(&target,std::fs::Permissions::from_mode(0o600)).unwrap();}}}
        let original=std::env::temp_dir().join(format!("cold-admission-original-{}",uuid::Uuid::new_v4()));std::fs::rename(&data,&original).unwrap();copy_tree(&original,&data);std::fs::remove_dir_all(&original).unwrap();fixture.write();
        let bundle=std::sync::Arc::new(verify(&fixture.root).await.unwrap());let cold=admission::ClassifiedAdmissionSource::cold_bundle(bundle.clone()).await.unwrap();cold.fence().unwrap();assert_eq!(cold.source_roles().scopes[0].attempt_id,attempt);
        let payload=data.join(format!(".attachment-upload-journal/{attempt}/payload.part"));let kept=fixture.root.parent().unwrap().join(format!("cold-kept-{}",uuid::Uuid::new_v4()));std::fs::rename(&payload,&kept).unwrap();std::fs::copy(&kept,&payload).unwrap();assert!(cold.fence().is_err(),"equal-byte foreign owned snapshot payload must refuse");std::fs::remove_file(&kept).unwrap();drop(cold);drop(bundle);
        let origin_rel=format!(".attachment-upload-admission/{attempt}/stage-origin.json");let origin_path=data.join(&origin_rel);let mut origin:serde_json::Value=serde_json::from_slice(&std::fs::read(&origin_path).unwrap()).unwrap();origin["payload"]["inode"]=serde_json::json!(999);let raw=serde_json::to_vec(&origin).unwrap();std::fs::write(&origin_path,&raw).unwrap();let sha=hex::encode(Sha256::digest(&raw));fixture.value["files"][&origin_rel]=serde_json::json!({"size_bytes":raw.len(),"sha256":sha});
        for binding in fixture.value["admission_owner_capture"]["scopes"][0]["files"].as_array_mut().unwrap(){if binding["role"]["role"]=="capsule_record"&&binding["role"]["name"]=="stage-origin.json"{binding["length"]=serde_json::json!(raw.len());binding["sha256"]=serde_json::json!(sha);}}
        fixture.write();let malformed=std::sync::Arc::new(verify(&fixture.root).await.unwrap());assert!(admission::ClassifiedAdmissionSource::cold_bundle(malformed).await.is_err(),"self-consistent hashes cannot replace original stage payload custody");
        fixture.value["admission_owner_capture"]["scopes"][0]["directories"][0]["captured"]["inode"]=serde_json::json!(999);fixture.write();assert!(verify(&fixture.root).await.is_err(),"foreign original capture directory refuses before ownership");
    }
    #[tokio::test]
    async fn prior_finalized_partial_admission_cold_bundle_requires_exact_lineage_and_owned_snapshot() {
        use crate::storage::{attachment_upload as upload,attachment_capacity as capacity,restoration::admission};
        use std::os::unix::fs::{PermissionsExt,DirBuilderExt};
        let mut fixture=Fixture::new().await;let data=fixture.root.join("data");std::fs::set_permissions(&data,std::fs::Permissions::from_mode(0o700)).unwrap();
        let url=format!("sqlite://{}?mode=rw",data.join(DATABASE).display());let db=sqlx::any::AnyPoolOptions::new().max_connections(2).connect(&url).await.unwrap();
        let attempt=uuid::Uuid::new_v4().to_string();let req=upload::UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"browser".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),original_name:"x.bin".into(),mime_type:"application/octet-stream".into()};
        let original_source=std::env::temp_dir().join(format!("prior-original-source-{}",uuid::Uuid::new_v4()));std::fs::DirBuilder::new().mode(0o700).create(&original_source).unwrap();let writer=upload::admission::WriterLease::begin(&original_source,&attempt,upload::admission::Producer::Direct{request:req.clone()},64).unwrap();capacity::reserve(&db,&writer,None).await.unwrap();let mut ingest=upload::Ingest::reserve_with_lease(&original_source,req,64,writer.clone()).unwrap();ingest.append(b"authentic original partial stage").unwrap();drop(ingest);drop(writer);
        let row=admission::CapturedReservation{operation_id:attempt.clone(),request_sha256:sqlx::query_scalar("SELECT request_sha256 FROM attachment_capacity_reservations").fetch_one(&db).await.unwrap(),admission_claim_sha256:sqlx::query_scalar("SELECT admission_claim_sha256 FROM attachment_capacity_reservations").fetch_one(&db).await.unwrap(),maximum_bytes:64,state:"reserved".into(),committed_asset_id:None,committed_bytes:None,disposition_json:None,disposition_sha256:None};
        let mut tx=db.begin().await.unwrap();let leases=admission::capture::NativeCaptureLeases::hold(&original_source,&mut *tx,&[row]).await.unwrap();let source=leases.classify(&mut tx).await.unwrap();let roles=source.source_roles().clone();let original_root=roles.scopes[0].directories.iter().find(|b|b.role==admission::DirectoryRole::DataRoot).unwrap().original.clone();
        for scope in &roles.scopes {for binding in &scope.directories {if let Some(path)=binding.role.relative(&attempt).unwrap(){std::fs::DirBuilder::new().recursive(true).mode(0o700).create(data.join(path)).unwrap();}}for binding in &scope.files{let path=binding.role.relative(&attempt).unwrap();std::fs::copy(original_source.join(&path),data.join(&path)).unwrap();std::fs::set_permissions(data.join(path),std::fs::Permissions::from_mode(0o600)).unwrap();}}
        source.fence().unwrap();drop(source);drop(leases);tx.rollback().await.unwrap();std::fs::remove_dir_all(&original_source).unwrap();
        crate::storage::restoration::linux::admission_owner_engine_tests::fixture_finalized(&data,&db,roles,original_root).await;crate::storage::restoration::linux::activate_in(&data,&db).await.unwrap();let mut conn=db.acquire().await.unwrap();let row=admission::CapturedReservation{operation_id:attempt.clone(),request_sha256:sqlx::query_scalar("SELECT request_sha256 FROM attachment_capacity_reservations").fetch_one(&mut *conn).await.unwrap(),admission_claim_sha256:sqlx::query_scalar("SELECT admission_claim_sha256 FROM attachment_capacity_reservations").fetch_one(&mut *conn).await.unwrap(),maximum_bytes:64,state:"reserved".into(),committed_asset_id:None,committed_bytes:None,disposition_json:None,disposition_sha256:None};let held=crate::storage::restoration::linux::PriorRestoredCaptureLeases::hold(&data,&mut conn,&[row]).await.unwrap();let capture=held.lineage_evidence().unwrap();held.recheck_in(&mut conn).await.unwrap();drop(held);drop(conn);db.close().await;
        fn inventory(base:&Path,current:&Path,files:&mut serde_json::Map<String,serde_json::Value>,custody:&mut serde_json::Map<String,serde_json::Value>,dirs:&mut serde_json::Map<String,serde_json::Value>) {
            for e in std::fs::read_dir(current).unwrap(){let p=e.unwrap().path();let rel=p.strip_prefix(base).unwrap().to_str().unwrap().to_owned();let m=std::fs::metadata(&p).unwrap();if m.is_dir(){dirs.insert(rel,serde_json::json!({"device":m.dev(),"inode":m.ino()}));inventory(base,&p,files,custody,dirs);}else{let raw=std::fs::read(&p).unwrap();files.insert(rel.clone(),serde_json::json!({"size_bytes":raw.len(),"sha256":hex::encode(Sha256::digest(&raw))}));if rel!=DATABASE{custody.insert(rel,serde_json::json!({"device":m.dev(),"inode":m.ino(),"links":m.nlink()}));}}}
        }
        let mut files=serde_json::Map::new();let mut custody=serde_json::Map::new();let mut dirs=serde_json::Map::new();inventory(&data,&data,&mut files,&mut custody,&mut dirs);
        fixture.value["files"]=serde_json::Value::Object(files);fixture.value["source_custody"]["files"]=serde_json::Value::Object(custody);fixture.value["source_custody"]["directories"]=serde_json::Value::Object(dirs.clone());
        let namespaces=vec![".attachment-upload-admission",".attachment-upload-journal",".restoration-custody"];let journal_dirs:Vec<_>=dirs.keys().filter(|p|namespaces.iter().any(|n|p.as_str()==*n||p.starts_with(&format!("{n}/")))).cloned().collect();fixture.value["recovery_boundary"]=serde_json::json!({"scope":"storage-bytes-only","ordinary_restore":"blocked-identity-bound-evidence","journal_namespaces":namespaces,"journal_directories":journal_dirs,"receipt_families":[{"table":"storage_restore_generations","rows":1},{"table":"storage_restore_current","rows":1},{"table":"attachment_capacity_reservations","rows":1}]});
        fn copy_tree(src:&Path,dst:&Path){std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dst).unwrap();for e in std::fs::read_dir(src).unwrap(){let p=e.unwrap().path();let target=dst.join(p.file_name().unwrap());if p.is_dir(){copy_tree(&p,&target)}else{std::fs::copy(&p,&target).unwrap();std::fs::set_permissions(&target,std::fs::Permissions::from_mode(0o600)).unwrap();}}}
        let original=std::env::temp_dir().join(format!("cold-admission-original-{}",uuid::Uuid::new_v4()));std::fs::rename(&data,&original).unwrap();copy_tree(&original,&data);std::fs::remove_dir_all(&original).unwrap();fixture.write();
        let bundle=std::sync::Arc::new(verify(&fixture.root).await.unwrap());let cold=admission::ClassifiedAdmissionSource::cold_prior_bundle(bundle.clone(),&capture).await.unwrap();cold.fence().unwrap();assert_eq!(cold.source_roles().scopes[0].attempt_id,attempt);
        let payload=data.join(format!(".attachment-upload-journal/{attempt}/payload.part"));let kept=fixture.root.parent().unwrap().join(format!("cold-kept-{}",uuid::Uuid::new_v4()));std::fs::rename(&payload,&kept).unwrap();std::fs::copy(&kept,&payload).unwrap();assert!(cold.fence().is_err(),"equal-byte foreign owned snapshot payload must refuse");std::fs::remove_file(&kept).unwrap();drop(cold);drop(bundle);
        let mut foreign:crate::storage::restoration::admission::PriorRestoredCaptureEvidence=serde_json::from_slice(&serde_json::to_vec(&capture).unwrap()).unwrap();foreign.scopes[0].files[0].captured.inode=999;let fresh=std::sync::Arc::new(verify(&fixture.root).await.unwrap());assert!(admission::ClassifiedAdmissionSource::cold_prior_bundle(fresh,&foreign).await.is_err(),"Foreign prior captured role cannot replace finalized adopted custody");
        let mut wrong_generation:crate::storage::restoration::admission::PriorRestoredCaptureEvidence=serde_json::from_slice(&serde_json::to_vec(&capture).unwrap()).unwrap();wrong_generation.generation_id=uuid::Uuid::new_v4().to_string();let fresh=std::sync::Arc::new(verify(&fixture.root).await.unwrap());assert!(admission::ClassifiedAdmissionSource::cold_prior_bundle(fresh,&wrong_generation).await.is_err(),"Prior transport must authenticate exact current generation");
        let mut wrong_hash:crate::storage::restoration::admission::PriorRestoredCaptureEvidence=serde_json::from_slice(&serde_json::to_vec(&capture).unwrap()).unwrap();wrong_hash.ledger_sha256="f".repeat(64);let fresh=std::sync::Arc::new(verify(&fixture.root).await.unwrap());assert!(admission::ClassifiedAdmissionSource::cold_prior_bundle(fresh,&wrong_hash).await.is_err(),"Prior transport must authenticate exact finalized canonical bytes");
    }
    #[tokio::test]
    async fn original_replay_blob_cold_bundle_requires_exact_custody_and_owned_snapshot() {
        use crate::storage::{attachment_upload as upload,attachment_capacity as capacity,restoration::admission};
        use std::os::unix::fs::{PermissionsExt,DirBuilderExt};
        let mut fixture=Fixture::new().await;let data=fixture.root.join("data");std::fs::set_permissions(&data,std::fs::Permissions::from_mode(0o700)).unwrap();
        let url=format!("sqlite://{}?mode=rw",data.join(DATABASE).display());let db=sqlx::any::AnyPoolOptions::new().max_connections(2).connect(&url).await.unwrap();
        let attempt=uuid::Uuid::new_v4().to_string();let binary=b"original replay binary";let producer=upload::admission::Producer::Replay{draft_scope:None,provider:"opaque-provider".into(),incoming_id:"opaque-incoming".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),asset_id:"opaque-asset".into(),payload_fingerprint:"a".repeat(64),binary_sha256:hex::encode(Sha256::digest(binary)),byte_length:binary.len() as u64};
        let writer=upload::admission::WriterLease::begin(&data,&attempt,producer,64).unwrap();capacity::reserve(&db,&writer,None).await.unwrap();let blob=crate::storage::asset_blob::create_with_lease(&data,"space","page","bin",binary,writer.clone()).unwrap();let blob_path=blob.path().to_owned();drop(blob);drop(writer);
        let row=admission::CapturedReservation{operation_id:attempt.clone(),request_sha256:sqlx::query_scalar("SELECT request_sha256 FROM attachment_capacity_reservations").fetch_one(&db).await.unwrap(),admission_claim_sha256:sqlx::query_scalar("SELECT admission_claim_sha256 FROM attachment_capacity_reservations").fetch_one(&db).await.unwrap(),maximum_bytes:64,state:"reserved".into(),committed_asset_id:None,committed_bytes:None,disposition_json:None,disposition_sha256:None};
        let mut tx=db.begin().await.unwrap();let leases=admission::capture::NativeCaptureLeases::hold(&data,&mut *tx,&[row]).await.unwrap();let source=leases.classify(&mut tx).await.unwrap();let capture=source.capture_envelope().unwrap();source.fence().unwrap();drop(source);drop(leases);tx.rollback().await.unwrap();db.close().await;
        fn inventory(base:&Path,current:&Path,files:&mut serde_json::Map<String,serde_json::Value>,custody:&mut serde_json::Map<String,serde_json::Value>,dirs:&mut serde_json::Map<String,serde_json::Value>) {
            for e in std::fs::read_dir(current).unwrap(){let p=e.unwrap().path();let rel=p.strip_prefix(base).unwrap().to_str().unwrap().to_owned();let m=std::fs::metadata(&p).unwrap();if m.is_dir(){dirs.insert(rel,serde_json::json!({"device":m.dev(),"inode":m.ino()}));inventory(base,&p,files,custody,dirs);}else{let raw=std::fs::read(&p).unwrap();files.insert(rel.clone(),serde_json::json!({"size_bytes":raw.len(),"sha256":hex::encode(Sha256::digest(&raw))}));if rel!=DATABASE{custody.insert(rel,serde_json::json!({"device":m.dev(),"inode":m.ino(),"links":m.nlink()}));}}}
        }
        let mut files=serde_json::Map::new();let mut custody=serde_json::Map::new();let mut dirs=serde_json::Map::new();inventory(&data,&data,&mut files,&mut custody,&mut dirs);
        fixture.value["files"]=serde_json::Value::Object(files);fixture.value["source_custody"]["files"]=serde_json::Value::Object(custody);fixture.value["source_custody"]["directories"]=serde_json::Value::Object(dirs.clone());fixture.value["admission_owner_capture"]=serde_json::to_value(&capture).unwrap();
        let namespaces=vec![".attachment-upload-admission"];let journal_dirs:Vec<_>=dirs.keys().filter(|p|namespaces.iter().any(|n|p.as_str()==*n||p.starts_with(&format!("{n}/")))).cloned().collect();fixture.value["recovery_boundary"]=serde_json::json!({"scope":"storage-bytes-only","ordinary_restore":"blocked-identity-bound-evidence","journal_namespaces":namespaces,"journal_directories":journal_dirs,"receipt_families":[{"table":"attachment_capacity_reservations","rows":1}]});
        fn copy_tree(src:&Path,dst:&Path){std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dst).unwrap();for e in std::fs::read_dir(src).unwrap(){let p=e.unwrap().path();let target=dst.join(p.file_name().unwrap());if p.is_dir(){copy_tree(&p,&target)}else{std::fs::copy(&p,&target).unwrap();std::fs::set_permissions(&target,std::fs::Permissions::from_mode(0o600)).unwrap();}}}
        let original=std::env::temp_dir().join(format!("cold-admission-original-{}",uuid::Uuid::new_v4()));std::fs::rename(&data,&original).unwrap();copy_tree(&original,&data);std::fs::remove_dir_all(&original).unwrap();fixture.write();
        let bundle=std::sync::Arc::new(verify(&fixture.root).await.unwrap());let cold=admission::ClassifiedAdmissionSource::cold_bundle(bundle.clone()).await.unwrap();cold.fence().unwrap();assert_eq!(cold.source_roles().scopes[0].attempt_id,attempt);
        let payload=data.join(&blob_path);let kept=fixture.root.parent().unwrap().join(format!("cold-kept-{}",uuid::Uuid::new_v4()));std::fs::rename(&payload,&kept).unwrap();std::fs::copy(&kept,&payload).unwrap();assert!(cold.fence().is_err(),"equal-byte foreign owned snapshot payload must refuse");std::fs::remove_file(&kept).unwrap();drop(cold);drop(bundle);
        let origin_rel=format!(".attachment-upload-admission/{attempt}/replay-blob-origin.json");let origin_path=data.join(&origin_rel);let mut origin:serde_json::Value=serde_json::from_slice(&std::fs::read(&origin_path).unwrap()).unwrap();origin["payload"]["inode"]=serde_json::json!(999);let raw=serde_json::to_vec(&origin).unwrap();std::fs::write(&origin_path,&raw).unwrap();let sha=hex::encode(Sha256::digest(&raw));fixture.value["files"][&origin_rel]=serde_json::json!({"size_bytes":raw.len(),"sha256":sha});
        for binding in fixture.value["admission_owner_capture"]["scopes"][0]["files"].as_array_mut().unwrap(){if binding["role"]["role"]=="capsule_record"&&binding["role"]["name"]=="replay-blob-origin.json"{binding["length"]=serde_json::json!(raw.len());binding["sha256"]=serde_json::json!(sha);}}
        fixture.write();let malformed=std::sync::Arc::new(verify(&fixture.root).await.unwrap());assert!(admission::ClassifiedAdmissionSource::cold_bundle(malformed).await.is_err(),"self-consistent hashes cannot replace original Replay blob payload custody");
        fixture.value["admission_owner_capture"]["scopes"][0]["directories"][0]["captured"]["inode"]=serde_json::json!(999);fixture.write();assert!(verify(&fixture.root).await.is_err(),"foreign original capture directory refuses before ownership");
    }

}
