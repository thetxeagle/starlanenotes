//! Candidate direct-upload custody. SQL receipt is the commit decision; no orphan adoption.
#[path = "attachment_upload_stage.rs"]
mod stage;
#[path = "attachment_upload_admission.rs"]
pub(crate) mod admission;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, AnyPool};
pub(crate) use stage::CHUNK;
use std::{
    fs::File,
    io::{self, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{FileExt, MetadataExt},
    },
    path::{Path, PathBuf},
};
const NS: &str = stage::NAMESPACE;
const ENVELOPE: u64 = 16384;
fn fail(s: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s)
}
fn digest(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}
fn uuid(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok_and(|v| v.to_string() == s)
}
/// Original immutable source selected for an explicit copy operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CopySourceScope {
    pub schema_version:u8,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub source_kind:Option<String>,
    pub source_space_id:String,
    pub source_document_id:String,
    pub version_id:String,
    pub expected_source_version_sha256:String,
    pub binary_sha256:String,
    pub byte_length:i64,
}
impl CopySourceScope {
    pub(crate) fn validate(&self)->io::Result<()> {
        let hash=|value:&str|value.len()==64 && value.bytes().all(|b|b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
        let selected=match self.source_kind.as_deref() {
            None=>uuid(&self.version_id),
            Some("legacy_snapshot_v1")=>!self.version_id.is_empty() && self.version_id.len()<=255 && !matches!(self.version_id.as_str(),"."|"..") && !self.version_id.contains(&['/','\\','\0'][..]),
            Some(_)=>false,
        };
        if self.schema_version!=1 || self.source_space_id.trim().is_empty() || self.source_document_id.trim().is_empty()
            || !selected || !hash(&self.expected_source_version_sha256) || !hash(&self.binary_sha256) || self.byte_length<0 {
            return Err(fail("Invalid original attachment copy source"));
        }
        Ok(())
    }
}
/// Immutable original draft identity, distinct from byte-production format v1.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct DraftUploadScope {
    pub schema_version: u8,
    pub draft_id: String,
    pub draft_created_at: String,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub copy_source:Option<CopySourceScope>,
}
impl DraftUploadScope {
    pub(crate) fn validate(&self) -> io::Result<()> {
        if let Some(source)=&self.copy_source {source.validate()?;}
        if self.schema_version != 2 || !uuid(&self.draft_id) || self.draft_created_at.trim().is_empty() {
            return Err(fail("Invalid original draft upload scope"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct UploadRequest {
    pub operation_id: String,
    pub origin: String,
    pub actor_id: String,
    pub space_id: String,
    pub document_id: String,
    pub original_name: String,
    pub mime_type: String,
    // Absent historical fields remain absent on serialization: original hashes
    // and request bytes must not change merely because this binary reads them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_scope: Option<DraftUploadScope>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Intent {
    pub version: u8,
    pub request: UploadRequest,
    pub attempt_id: String,
    pub asset_id: String,
    pub storage_path: String,
    pub filename: String,
    pub created_at: String,
    pub byte_length: i64,
    pub binary_sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstallProof {
    version: u8,
    intent_sha256: String,
    original_device: u64,
    original_inode: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Receipt {
    pub intent: Intent,
    pub intent_sha256: String,
    pub install_proof_sha256: String,
}
fn json<T: Serialize>(v: &T) -> io::Result<Vec<u8>> {
    let b = serde_json::to_vec(v).map_err(io::Error::other)?;
    if b.len() as u64 > ENVELOPE {
        return Err(fail("Upload envelope oversized"));
    }
    Ok(b)
}
fn bytes(d: &File, n: &str) -> io::Result<(File, Vec<u8>)> {
    let f = stage::open(d, n, libc::O_RDONLY | libc::O_NONBLOCK)?;
    let m = f.metadata()?;
    if !m.is_file() || m.nlink() != 1 || m.mode() & 0o077 != 0 || m.len() > ENVELOPE {
        return Err(fail("Invalid upload envelope file"));
    }
    let mut b = vec![0; m.len() as usize];
    f.read_exact_at(&mut b, 0)?;
    if f.metadata()?.len() != m.len() {
        return Err(fail("Envelope changed"));
    }
    Ok((f, b))
}
fn decode<T: for<'de> Deserialize<'de>>(b: &[u8]) -> io::Result<T> {
    serde_json::from_slice(b).map_err(io::Error::other)
}
fn hash(f: &File, len: u64) -> io::Result<String> {
    let m = f.metadata()?;
    if !m.is_file() || m.len() != len || m.mode() & 0o077 != 0 {
        return Err(fail("Invalid payload metadata"));
    }
    let mut b = [0; CHUNK];
    let mut h = Sha256::new();
    let mut p = 0;
    while p < len {
        let n = f.read_at(&mut b[..(len - p).min(CHUNK as u64) as usize], p)?;
        if n == 0 {
            return Err(fail("Truncated upload payload"));
        }
        h.update(&b[..n]);
        p += n as u64;
    }
    if f.read_at(&mut b[..1], p)? != 0 || f.metadata()?.len() != len {
        return Err(fail("Upload payload grew"));
    }
    Ok(hex::encode(h.finalize()))
}
fn directory(d: &File, n: &str, create: bool) -> io::Result<File> {
    if create {
        let c = std::ffi::CString::new(n).map_err(|_| fail("NUL component"))?;
        if n.is_empty() || n == "." || n == ".." || n.contains('/') || n.contains('\\') {
            return Err(fail("Invalid component"));
        } // SAFETY: held directory, single validated terminated name, private fixed mode.
        let rc = unsafe { libc::mkdirat(d.as_raw_fd(), c.as_ptr(), 0o700) };
        if rc < 0 && io::Error::last_os_error().kind() != io::ErrorKind::AlreadyExists {
            return Err(io::Error::last_os_error());
        }
        d.sync_all()?;
    }
    stage::open(d, n, libc::O_RDONLY | libc::O_DIRECTORY)
}
fn target(root: &File, path: &str, create: bool) -> io::Result<(File, String)> {
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() != 5
        || parts[0] != "spaces"
        || parts[2] != "attachments"
        || parts.iter().any(|p| {
            p.is_empty() || *p == "." || *p == ".." || p.contains('\\') || p.contains('\0')
        })
    {
        return Err(fail("Invalid upload destination"));
    }
    let mut d = root.try_clone()?;
    for p in &parts[..4] {
        d = directory(&d, p, create)?;
    }
    Ok((d, parts[4].into()))
}
fn validate(i: &Intent) -> io::Result<()> {
    if let Some(scope) = &i.request.draft_scope { scope.validate()?; }
    if let Some(source)=i.request.draft_scope.as_ref().and_then(|scope|scope.copy_source.as_ref()) {
        if source.byte_length!=i.byte_length || source.binary_sha256!=i.binary_sha256 {return Err(fail("Original copy source bytes differ from upload intent"));}
    }
    let expected_version = if i.request.draft_scope.is_some() { 2 } else { 1 };
    if i.version != expected_version
        || !uuid(&i.request.operation_id)
        || !uuid(&i.attempt_id)
        || !uuid(&i.asset_id)
        || i.byte_length < 0
        || i.request.origin.is_empty()
        || i.request.actor_id.is_empty()
        || i.request.space_id.is_empty()
        || i.request.document_id.is_empty()
        || i.request.original_name.trim().is_empty()
        || i.request.original_name.len() > 255
        || i.binary_sha256.len() != 64
        || !i
            .binary_sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || i.created_at.is_empty()
    {
        return Err(fail("Invalid upload intent"));
    }
    let p: Vec<_> = i.storage_path.split('/').collect();
    if p.len() != 5
        || p[0] != "spaces"
        || p[2] != "attachments"
        || p.iter().any(|p| {
            p.is_empty() || *p == "." || *p == ".." || p.contains('\\') || p.contains('\0')
        })
        || p[3] != i.request.document_id
        || p[4] != i.filename
        || !i
            .filename
            .strip_prefix(&(i.asset_id.clone() + "."))
            .is_some_and(|ext| {
                !ext.is_empty() && ext.len() <= 16 && ext.bytes().all(|b| b.is_ascii_alphanumeric())
            })
    {
        return Err(fail("Upload intent destination differs"));
    }
    Ok(())
}
pub(crate) struct Ingest {
    lease: Option<admission::WriterLease>,
    stage: stage::Stage,
    request: UploadRequest,
    request_bytes: Vec<u8>,
    request_file: File,
}
impl Ingest {
    pub fn reserve(root: &Path, request: UploadRequest, maximum: u64) -> io::Result<Self> {
        Self::reserve_inner(root, request, maximum, None)
    }
    /// Caller retains its own lease clone until the SQL publication decision completes.
    pub fn reserve_with_lease(root: &Path, request: UploadRequest, maximum: u64, lease: admission::WriterLease) -> io::Result<Self> {
        lease.verify()?;
        lease.matches_root(root)?;
        if lease.producer() != &(admission::Producer::Direct { request: request.clone() }) || lease.maximum_bytes() != maximum {
            return Err(fail("Stage admission producer differs"));
        }
        Self::reserve_inner(root, request, maximum, Some(lease))
    }
    fn reserve_inner(root: &Path, request: UploadRequest, maximum: u64, lease: Option<admission::WriterLease>) -> io::Result<Self> {
        if !uuid(&request.operation_id) {
            return Err(fail("Upload operation UUID invalid"));
        }
        if let Some(scope) = &request.draft_scope { scope.validate()?; }
        super::restoration::fence(root)?;
        let request_bytes = json(&request)?;
        let s = stage::Stage::reserve(
            root,
            stage::Reservation {
                version: 1,
                operation_id: lease.as_ref().map(|l|l.attempt_id().to_owned()).unwrap_or_else(||uuid::Uuid::new_v4().to_string()),
                actor_id: request.actor_id.clone(),
                space_id: request.space_id.clone(),
                document_id: request.document_id.clone(),
                original_name: request.original_name.clone(),
                mime_type: request.mime_type.clone(),
                maximum_bytes: maximum,
            },
        )?;
        let request_file = stage::write_envelope(&s.directory, "request.json", &request_bytes)?;
        if let Some(held)=&lease {held.bind_direct_stage(&s,&request_bytes)?;}
        Ok(Self {
            stage: s,
            lease,
            request,
            request_bytes,
            request_file,
        })
    }
    pub fn append(&mut self, b: &[u8]) -> io::Result<()> {
        if let Some(lease)=&self.lease {lease.verify()?;}
        self.stage.append(b)
    }
    pub fn finish(self) -> io::Result<Captured> {
        if let Some(lease)=&self.lease {lease.verify()?;}
        let (_, b) = bytes(&self.stage.directory, "request.json")?;
        let fresh = stage::open(
            &self.stage.directory,
            "request.json",
            libc::O_RDONLY | libc::O_NONBLOCK,
        )?;
        if b != self.request_bytes || !stage::same(&fresh, &self.request_file)? {
            return Err(fail("Upload request changed"));
        }
        let held = self.stage.finish()?;
        Ok(Captured {
            held,
            lease: self.lease,
            request: self.request,
            request_bytes: self.request_bytes,
            request_file: self.request_file,
        })
    }
}
pub(crate) struct Captured {
    lease: Option<admission::WriterLease>,
    held: stage::SealedStage,
    request: UploadRequest,
    request_bytes: Vec<u8>,
    request_file: File,
}
impl Captured {
    pub fn byte_length(&self)->u64 {self.held.seal().length}
    pub fn matches(&self, r: &Receipt) -> bool {
        self.request == r.intent.request
            && self.held.seal().length == r.intent.byte_length as u64
            && self.held.seal().payload_sha256 == r.intent.binary_sha256
    }
    pub fn prepare(self, slug: &str, extension: &str) -> io::Result<Prepared> {
        if let Some(lease)=&self.lease {lease.verify()?;}
        self.held.verify()?;
        if extension.is_empty()
            || extension.len() > 16
            || !extension.bytes().all(|b| b.is_ascii_alphanumeric())
        {
            return Err(fail("Invalid extension"));
        }
        let asset_id = uuid::Uuid::new_v4().to_string();
        let filename = format!("{asset_id}.{extension}");
        let i = Intent {
            version: if self.request.draft_scope.is_some() { 2 } else { 1 },
            request: self.request.clone(),
            attempt_id: self
                .held
                .stage
                .relative_directory()
                .split('/')
                .last()
                .unwrap()
                .into(),
            asset_id,
            storage_path: format!(
                "spaces/{slug}/attachments/{}/{}",
                self.request.document_id, filename
            ),
            filename,
            created_at: chrono::Utc::now().to_rfc3339(),
            byte_length: self.held.seal().length as i64,
            binary_sha256: self.held.seal().payload_sha256.clone(),
        };
        validate(&i)?;
        super::restoration::fence(&self.held.stage.root_path)?;
        if !super::restoration::fresh_install_allowed(
            &self.held.stage.root_path,
            NS,
            &i.attempt_id,
        )? {
            return Err(fail("Classified upload cannot mint proof"));
        }
        let raw = json(&i)?;
        let intent_file = stage::write_envelope(&self.held.stage.directory, "intent.json", &raw)?;
        let m = self.held.stage.payload.metadata()?;
        let proof = InstallProof {
            version: 1,
            intent_sha256: digest(&raw),
            original_device: m.dev(),
            original_inode: m.ino(),
        };
        let proof_bytes = json(&proof)?;
        let proof_file = stage::write_envelope(
            &self.held.stage.directory,
            "install-proof.json",
            &proof_bytes,
        )?;
        let receipt = Receipt {
            intent: i,
            intent_sha256: digest(&raw),
            install_proof_sha256: digest(&proof_bytes),
        };
        Ok(Prepared {
            captured: self,
            receipt,
            intent_file,
            proof_file,
            proof_bytes,
        })
    }
}
pub(crate) struct Prepared {
    captured: Captured,
    pub receipt: Receipt,
    intent_file: File,
    proof_file: File,
    proof_bytes: Vec<u8>,
}
impl Prepared {
    pub fn install(self) -> io::Result<Receipt> {
        if let Some(lease)=&self.captured.lease {lease.verify()?;}
        self.captured.held.verify()?;
        let r = &self.receipt;
        let root = &self.captured.held.stage.root_path;
        super::restoration::fence(root)?;
        let (_, ib) = bytes(&self.captured.held.stage.directory, "intent.json")?;
        let (_, pb) = bytes(&self.captured.held.stage.directory, "install-proof.json")?;
        if digest(&ib) != r.intent_sha256
            || pb != self.proof_bytes
            || !stage::same(
                &stage::open(
                    &self.captured.held.stage.directory,
                    "intent.json",
                    libc::O_RDONLY,
                )?,
                &self.intent_file,
            )?
            || !stage::same(
                &stage::open(
                    &self.captured.held.stage.directory,
                    "install-proof.json",
                    libc::O_RDONLY,
                )?,
                &self.proof_file,
            )?
        {
            return Err(fail("Upload installation evidence changed"));
        }
        let (_, rb) = bytes(&self.captured.held.stage.directory, "request.json")?;
        if rb != self.captured.request_bytes
            || !stage::same(
                &stage::open(
                    &self.captured.held.stage.directory,
                    "request.json",
                    libc::O_RDONLY,
                )?,
                &self.captured.request_file,
            )?
        {
            return Err(fail("Upload request evidence changed"));
        }
        install(root, r)?;
        verify(root, r, false)?;
        Ok(self.receipt)
    }
}
fn operation(root: &File, id: &str) -> io::Result<File> {
    if !uuid(id) {
        return Err(fail("Invalid upload attempt UUID"));
    }
    let ns = directory(root, NS, false)?;
    let op = directory(&ns, id, false)?;
    if ns.metadata()?.mode() & 0o077 != 0 || op.metadata()?.mode() & 0o077 != 0 {
        return Err(fail("Upload operation privacy changed"));
    }
    Ok(op)
}
fn sealed_evidence(op: &File, attempt: &str) -> io::Result<(stage::Seal, UploadRequest)> {
    let (_, raw_reservation) = bytes(op, "reservation.json")?;
    let reservation: stage::Reservation = decode(&raw_reservation)?;
    let (_, raw_seal) = bytes(op, "seal.json")?;
    let seal: stage::Seal = decode(&raw_seal)?;
    let (_, raw_request) = bytes(op, "request.json")?;
    let request: UploadRequest = decode(&raw_request)?;
    if let Some(scope) = &request.draft_scope { scope.validate()?; }
    if reservation.version != 1
        || reservation.operation_id != attempt
        || !uuid(attempt)
        || !uuid(&request.operation_id)
        || seal.version != 1
        || seal.reservation_sha256 != digest(&raw_reservation)
        || seal.length > reservation.maximum_bytes
        || reservation.actor_id != request.actor_id
        || reservation.space_id != request.space_id
        || reservation.document_id != request.document_id
        || reservation.original_name != request.original_name
        || reservation.mime_type != request.mime_type
    {
        return Err(fail("Upload seal reservation/request differs"));
    }
    let payload = stage::open(op, "payload.part", libc::O_RDONLY | libc::O_NONBLOCK)?;
    let links = payload.metadata()?.nlink();
    if !(1..=2).contains(&links) || hash(&payload, seal.length)? != seal.payload_sha256 {
        return Err(fail("Upload sealed payload differs"));
    }
    Ok((seal, request))
}
fn evidence(root: &Path, r: &Receipt) -> io::Result<(File, File, File, InstallProof, Vec<u8>)> {
    validate(&r.intent)?;
    let rt = stage::root(root)?;
    let op = operation(&rt, &r.intent.attempt_id)?;
    let (_, i) = bytes(&op, "intent.json")?;
    let (_, p) = bytes(&op, "install-proof.json")?;
    let (_, q) = bytes(&op, "request.json")?;
    if digest(&i) != r.intent_sha256
        || decode::<Intent>(&i)? != r.intent
        || digest(&p) != r.install_proof_sha256
        || decode::<UploadRequest>(&q)? != r.intent.request
    {
        return Err(fail("Upload original evidence differs"));
    }
    let proof: InstallProof = decode(&p)?;
    if proof.version != 1 || proof.intent_sha256 != r.intent_sha256 || proof.original_inode == 0 {
        return Err(fail("Upload install proof invalid"));
    }
    let (seal, request) = sealed_evidence(&op, &r.intent.attempt_id)?;
    if seal.length != r.intent.byte_length as u64
        || seal.payload_sha256 != r.intent.binary_sha256
        || request != r.intent.request
    {
        return Err(fail("Upload intent/seal differs"));
    }
    let payload = stage::open(&op, "payload.part", libc::O_RDONLY | libc::O_NONBLOCK)?;
    Ok((rt, op, payload, proof, p))
}
fn owned(
    root: &Path,
    r: &Receipt,
    path: &str,
    f: &File,
    p: &InstallProof,
    raw: &[u8],
) -> io::Result<()> {
    if !super::restoration::owns(
        root,
        NS,
        &r.intent.attempt_id,
        raw,
        path,
        (p.original_device, p.original_inode),
        f,
    )? || hash(f, r.intent.byte_length as u64)? != r.intent.binary_sha256
    {
        return Err(fail("Upload retained identity or bytes differ"));
    }
    Ok(())
}
// Retained provenance cannot resurrect a cleanup-owned asset. Presence is
// refusal only, never authority to unlink or subtract accounting charge.
#[cfg(target_os="linux")]
fn refuse_erasure_review(root:&Path,r:&Receipt)->io::Result<()>{
 let rt=stage::root(root)?;
 let ns=match stage::open(&rt,".attachment-upload-admission",libc::O_RDONLY|libc::O_DIRECTORY){Ok(f)=>f,Err(e)if e.kind()==io::ErrorKind::NotFound=>return Ok(()),Err(e)=>return Err(e)};
 let capsule=match stage::open(&ns,&r.intent.attempt_id,libc::O_RDONLY|libc::O_DIRECTORY){Ok(f)=>f,Err(e)if e.kind()==io::ErrorKind::NotFound=>return Ok(()),Err(e)=>return Err(e)};
 for marker in ["committed-erasure-intent.json","committed-erasure-tombstone.json"]{match stage::open(&capsule,marker,libc::O_RDONLY|libc::O_NONBLOCK){Ok(_)=>return Err(fail("Original upload has retained erasure review; historical receipt cannot reinstall or serve")),Err(e)if e.kind()==io::ErrorKind::NotFound=>(),Err(e)=>return Err(e)}}Ok(())
}
fn install(root: &Path, r: &Receipt) -> io::Result<()> {
    #[cfg(target_os="linux")]
        let _family=super::native_family_fence::Creation::shared(root)?;
    #[cfg(target_os="linux")]
    refuse_erasure_review(root,r)?;
    super::restoration::fence(root)?;
    let (rt, op, payload, p, raw) = evidence(root, r)?;
    let anchor = format!("{NS}/{}/payload.part", r.intent.attempt_id);
    owned(root, r, &anchor, &payload, &p, &raw)?;
    let (dir, name) = target(&rt, &r.intent.storage_path, true)?;
    match stage::open(&dir, &name, libc::O_RDONLY | libc::O_NONBLOCK) {
        Ok(final_file) => {
            if !stage::same(&payload, &final_file)? {
                return Err(fail("Upload destination occupied"));
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if payload.metadata()?.nlink() != 1 {
                return Err(fail("Upload anchor has unrelated links"));
            }
            let name = std::ffi::CString::new(name).map_err(|_| fail("NUL filename"))?;
            let old = std::ffi::CString::new("payload.part").unwrap(); // SAFETY: held directory FDs and validated filenames; linkat is exclusive and cannot overwrite.
            if unsafe {
                libc::linkat(
                    op.as_raw_fd(),
                    old.as_ptr(),
                    dir.as_raw_fd(),
                    name.as_ptr(),
                    0,
                )
            } < 0
            {
                return Err(io::Error::last_os_error());
            }
        }
        Err(e) => return Err(e),
    }
    dir.sync_all()?;
    verify(root, r, false).map(|_| ())
}
pub(crate) fn verify(root: &Path, r: &Receipt, missing_allowed: bool) -> io::Result<bool> {
    #[cfg(target_os="linux")]
    refuse_erasure_review(root,r)?;
    super::restoration::fence(root)?;
    let (rt, op, payload, p, raw) = evidence(root, r)?;
    let anchor = format!("{NS}/{}/payload.part", r.intent.attempt_id);
    owned(root, r, &anchor, &payload, &p, &raw)?;
    let (dir, name) = match target(&rt, &r.intent.storage_path, false) {
        Ok(x) => x,
        Err(e) if missing_allowed && e.kind() == io::ErrorKind::NotFound => {
            if payload.metadata()?.nlink() != 1 {
                return Err(fail("Missing upload has foreign links"));
            }
            return Ok(false);
        }
        Err(e) => return Err(e),
    };
    let final_file = match stage::open(&dir, &name, libc::O_RDONLY | libc::O_NONBLOCK) {
        Ok(x) => x,
        Err(e) if missing_allowed && e.kind() == io::ErrorKind::NotFound => {
            if payload.metadata()?.nlink() != 1 {
                return Err(fail("Missing upload has foreign links"));
            }
            return Ok(false);
        }
        Err(e) => return Err(e),
    };
    if payload.metadata()?.nlink() != 2
        || final_file.metadata()?.nlink() != 2
        || !stage::same(&payload, &final_file)?
    {
        return Err(fail("Upload link group differs"));
    }
    owned(root, r, &r.intent.storage_path, &final_file, &p, &raw)?;
    let fresh = stage::root(root)?;
    let fresh_op = operation(&fresh, &r.intent.attempt_id)?;
    let (fresh_dir, fresh_name) = target(&fresh, &r.intent.storage_path, false)?;
    let fresh_payload = stage::open(&fresh_op, "payload.part", libc::O_RDONLY | libc::O_NONBLOCK)?;
    let fresh_final = stage::open(&fresh_dir, &fresh_name, libc::O_RDONLY | libc::O_NONBLOCK)?;
    if !stage::same(&fresh, &rt)?
        || !stage::same(&fresh_op, &op)?
        || !stage::same(&fresh_dir, &dir)?
        || !stage::same(&fresh_payload, &payload)?
        || !stage::same(&fresh_final, &final_file)?
    {
        return Err(fail("Upload live root/path changed"));
    }
    let (_r, _o, again, _p, again_raw) = evidence(root, r)?;
    if again_raw != raw || !stage::same(&again, &payload)? {
        return Err(fail("Upload evidence changed during verification"));
    }
    super::restoration::fence(root)?;
    Ok(true)
}
pub(crate) async fn insert(conn: &mut AnyConnection, r: &Receipt) -> Result<(), String> {
    let i = &r.intent;
    let q = &i.request;
    let raw = String::from_utf8(json(r).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    sqlx::query("INSERT INTO attachment_upload_receipts(operation_id,origin,actor_id,space_id,document_id,asset_id,attempt_id,storage_path,filename,original_name,mime_type,byte_length,binary_sha256,created_at,intent_sha256,install_proof_sha256,receipt_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17)").bind(&q.operation_id).bind(&q.origin).bind(&q.actor_id).bind(&q.space_id).bind(&q.document_id).bind(&i.asset_id).bind(&i.attempt_id).bind(&i.storage_path).bind(&i.filename).bind(&q.original_name).bind(&q.mime_type).bind(i.byte_length).bind(&i.binary_sha256).bind(&i.created_at).bind(&r.intent_sha256).bind(&r.install_proof_sha256).bind(raw).execute(conn).await.map_err(|e|e.to_string())?;
    Ok(())
}
async fn receipt_with_raw(
    conn: &mut AnyConnection,
    id: &str,
) -> Result<Option<(Receipt, Vec<u8>)>, String> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT substr(receipt_json,1,16385) FROM attachment_upload_receipts WHERE operation_id=$1",
    )
    .bind(id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| e.to_string())?;
    let Some(raw) = raw else { return Ok(None) };
    if raw.len() as u64 > ENVELOPE {
        return Err("Upload receipt oversized".into());
    }
    let r: Receipt = decode(raw.as_bytes()).map_err(|e| e.to_string())?;
    validate(&r.intent).map_err(|e| e.to_string())?;
    let i = &r.intent;
    let q = &i.request;
    if q.operation_id != id {
        return Err("Upload receipt ID differs".into());
    }
    let exact:Option<i64>=sqlx::query_scalar("SELECT 1 FROM attachment_upload_receipts WHERE operation_id=$1 AND origin=$2 AND actor_id=$3 AND space_id=$4 AND document_id=$5 AND asset_id=$6 AND attempt_id=$7 AND storage_path=$8 AND filename=$9 AND original_name=$10 AND mime_type=$11 AND byte_length=$12 AND binary_sha256=$13 AND created_at=$14 AND intent_sha256=$15 AND install_proof_sha256=$16 AND receipt_json=$17").bind(id).bind(&q.origin).bind(&q.actor_id).bind(&q.space_id).bind(&q.document_id).bind(&i.asset_id).bind(&i.attempt_id).bind(&i.storage_path).bind(&i.filename).bind(&q.original_name).bind(&q.mime_type).bind(i.byte_length).bind(&i.binary_sha256).bind(&i.created_at).bind(&r.intent_sha256).bind(&r.install_proof_sha256).bind(&raw).fetch_optional(conn).await.map_err(|e|e.to_string())?;
    if exact.is_none() {
        return Err("Upload receipt scalars differ".into());
    }
    Ok(Some((r, raw.into_bytes())))
}
pub(crate) async fn receipt(conn: &mut AnyConnection, id: &str) -> Result<Option<Receipt>, String> {
    Ok(receipt_with_raw(conn, id).await?.map(|(r, _)| r))
}
pub(crate) async fn association(conn: &mut AnyConnection, r: &Receipt) -> Result<bool, String> {
    let i = &r.intent;
    let q = &i.request;
    let exact:Option<i64>=sqlx::query_scalar("SELECT 1 FROM attachments a JOIN documents d ON d.id=a.document_id AND d.space_id=a.space_id JOIN spaces s ON s.id=d.space_id WHERE a.id=$1 AND a.document_id=$2 AND a.space_id=$3 AND a.filename=$4 AND a.original_name=$5 AND a.mime_type=$6 AND a.size_bytes=$7 AND a.storage_path=$8 AND a.created_by=$9 AND a.created_at=$10 AND s.slug=$11").bind(&i.asset_id).bind(&q.document_id).bind(&q.space_id).bind(&i.filename).bind(&q.original_name).bind(&q.mime_type).bind(i.byte_length).bind(&i.storage_path).bind(&q.actor_id).bind(&i.created_at).bind(i.storage_path.split('/').nth(1).unwrap()).fetch_optional(conn).await.map_err(|e|e.to_string())?;
    Ok(exact.is_some())
}
/// Read-only original envelope validation; never grants install/adoption authority.
#[derive(Debug, Clone)]
pub(crate) struct HistoryEvidence {
    pub intent: Intent,
    pub original_identity: (u64, u64),
    pub allowed_paths: Vec<String>,
}
pub(crate) fn validate_history(
    raw_intent: &[u8],
    raw_install_proof: &[u8],
    expected_attempt_id: &str,
) -> io::Result<HistoryEvidence> {
    if raw_intent.len() as u64 > ENVELOPE || raw_install_proof.len() as u64 > ENVELOPE {
        return Err(fail("Upload history envelope oversized"));
    }
    let intent: Intent = decode(raw_intent)?;
    validate(&intent)?;
    let proof: InstallProof = decode(raw_install_proof)?;
    if intent.attempt_id != expected_attempt_id
        || !uuid(expected_attempt_id)
        || proof.version != 1
        || proof.original_inode == 0
        || proof.intent_sha256 != digest(raw_intent)
    {
        return Err(fail("Upload history original evidence differs"));
    }
    let allowed_paths = vec![
        format!("{NS}/{expected_attempt_id}/payload.part"),
        intent.storage_path.clone(),
    ];
    Ok(HistoryEvidence {
        intent,
        original_identity: (proof.original_device, proof.original_inode),
        allowed_paths,
    })
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UploadClass {
    Unsealed,
    SealedUncommitted,
    CommittedPending,
    CommittedInstalled,
    Historical,
}
#[derive(Debug, Clone)]
pub(crate) struct CertifiedBinding {
    pub relative: String,
    pub original_device: u64,
    pub original_inode: u64,
    pub length: u64,
    pub sha256: String,
}
#[derive(Debug)]
pub(crate) struct Inspection {
    pub receipt: Receipt,
    pub class: UploadClass,
    pub original_proof: Vec<u8>,
    pub raw_intent: Vec<u8>,
    pub raw_receipt: Vec<u8>,
    pub original_identity: (u64, u64),
    pub required_group: Vec<String>,
    pub relative_paths: Vec<String>,
    pub bindings: Vec<CertifiedBinding>,
}
pub(crate) async fn inspect(
    root: &Path,
    conn: &mut AnyConnection,
    id: &str,
) -> Result<Inspection, String> {
    inspect_inner(root, conn, id, false).await
}

/// Decodes stopped-copy evidence without granting inode ownership or installation authority.
/// The restoration planner must certify original bindings against capture/lineage evidence.
pub(crate) async fn inspect_restoration(
    root: &Path,
    conn: &mut AnyConnection,
    id: &str,
) -> Result<Inspection, String> {
    inspect_inner(root, conn, id, true).await
}

async fn inspect_inner(
    root: &Path,
    conn: &mut AnyConnection,
    id: &str,
    snapshot: bool,
) -> Result<Inspection, String> {
    let (r, raw_receipt) = receipt_with_raw(conn, id)
        .await?
        .ok_or("Upload receipt absent")?;
    let current = association(conn, &r).await?;
    let (rt, op, payload, p, raw) = evidence(root, &r).map_err(|e| e.to_string())?;
    let (_, raw_intent) = bytes(&op, "intent.json").map_err(|e| e.to_string())?;
    if hash(&payload, r.intent.byte_length as u64).map_err(|e| e.to_string())?
        != r.intent.binary_sha256
    {
        return Err("Upload payload differs".into());
    }
    let anchor = format!("{NS}/{}/payload.part", r.intent.attempt_id);
    if !snapshot {
        owned(root, &r, &anchor, &payload, &p, &raw).map_err(|e| e.to_string())?;
    }
    let mut required_group = vec![anchor.clone()];
    let mut held_canonical = None;
    let present = match target(&rt, &r.intent.storage_path, false)
        .and_then(|(d, n)| stage::open(&d, &n, libc::O_RDONLY | libc::O_NONBLOCK))
    {
        Ok(f) => {
            if hash(&f, r.intent.byte_length as u64).map_err(|e| e.to_string())?
                != r.intent.binary_sha256
                || (!snapshot
                    && (!stage::same(&f, &payload).map_err(|e| e.to_string())?
                        || f.metadata().map_err(|e| e.to_string())?.nlink() != 2
                        || payload.metadata().map_err(|e| e.to_string())?.nlink() != 2))
                || (snapshot && !matches!(f.metadata().map_err(|e| e.to_string())?.nlink(), 1 | 2))
            {
                return Err("Upload retained group differs".into());
            }
            if !snapshot {
                owned(root, &r, &r.intent.storage_path, &f, &p, &raw).map_err(|e| e.to_string())?;
            }
            required_group.push(r.intent.storage_path.clone());
            held_canonical = Some(f);
            true
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if payload.metadata().map_err(|e| e.to_string())?.nlink() != 1 {
                return Err("Pending upload group differs".into());
            }
            false
        }
        Err(e) => return Err(e.to_string()),
    };
    let class = if !current {
        UploadClass::Historical
    } else if present {
        UploadClass::CommittedInstalled
    } else {
        UploadClass::CommittedPending
    };
    let (fresh_rt, fresh_op, fresh_payload, _, fresh_raw) =
        evidence(root, &r).map_err(|e| e.to_string())?;
    let (_, fresh_intent) = bytes(&fresh_op, "intent.json").map_err(|e| e.to_string())?;
    if fresh_raw != raw
        || fresh_intent != raw_intent
        || !stage::same(&fresh_rt, &rt).map_err(|e| e.to_string())?
        || !stage::same(&fresh_op, &op).map_err(|e| e.to_string())?
        || !stage::same(&fresh_payload, &payload).map_err(|e| e.to_string())?
    {
        return Err("Upload snapshot evidence changed".into());
    }
    if hash(&fresh_payload, r.intent.byte_length as u64).map_err(|e| e.to_string())?
        != r.intent.binary_sha256
    {
        return Err("Upload snapshot payload changed".into());
    }
    let fresh_canonical = target(&fresh_rt, &r.intent.storage_path, false)
        .and_then(|(d, n)| stage::open(&d, &n, libc::O_RDONLY | libc::O_NONBLOCK));
    match (held_canonical.as_ref(), fresh_canonical) {
        (Some(held), Ok(fresh)) => {
            if !stage::same(held, &fresh).map_err(|e| e.to_string())?
                || hash(&fresh, r.intent.byte_length as u64).map_err(|e| e.to_string())?
                    != r.intent.binary_sha256
            {
                return Err("Upload snapshot canonical changed".into());
            }
        }
        (None, Err(e)) if e.kind() == io::ErrorKind::NotFound => {}
        _ => return Err("Upload snapshot canonical presence changed".into()),
    }
    if !snapshot {
        super::restoration::fence(root).map_err(|e| e.to_string())?;
    }
    let paths = vec![anchor, r.intent.storage_path.clone()];
    Ok(Inspection {
        receipt: r.clone(),
        class,
        original_proof: raw,
        raw_intent,
        raw_receipt,
        original_identity: (p.original_device, p.original_inode),
        bindings: required_group
            .iter()
            .map(|relative| CertifiedBinding {
                relative: relative.clone(),
                original_device: p.original_device,
                original_inode: p.original_inode,
                length: r.intent.byte_length as u64,
                sha256: r.intent.binary_sha256.clone(),
            })
            .collect(),
        required_group,
        relative_paths: paths,
    })
}

pub(crate) async fn recover_in(root: &Path, db: &AnyPool) -> Result<usize, String> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT operation_id FROM attachment_upload_receipts ORDER BY operation_id",
    )
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;
    let mut count = 0;
    for id in ids {
        let mut tx = db.begin().await.map_err(|e| e.to_string())?;
        let r = receipt(&mut tx, &id).await?.ok_or("Receipt disappeared")?;
        // Historical immutable receipts retain evidence but never reinstall assets.
        // A deleted original space must not prevent recovery of unrelated live operations.
        if !association(&mut tx, &r).await? {
            tx.rollback().await.map_err(|e| e.to_string())?;
            continue;
        }
        crate::spaces::mutation_guards::lock_spaces_for_mutation(
            &mut tx,
            &[&r.intent.request.space_id],
        )
        .await
        .map_err(|e| format!("{e:?}"))?;
        let current = receipt(&mut tx, &id)
            .await?
            .ok_or("Receipt disappeared after lock")?;
        if current != r {
            return Err("Receipt changed during recovery".into());
        }
        if association(&mut tx, &r).await? {
            let held_root = root.to_path_buf();
            let held_receipt = r.clone();
            let installed = tokio::task::spawn_blocking(move || {
                if verify(&held_root, &held_receipt, true)? {
                    Ok(false)
                } else {
                    install(&held_root, &held_receipt)?;
                    Ok::<_, io::Error>(true)
                }
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
            count += usize::from(installed);
            if !association(&mut tx, &r).await? {
                return Err("Association changed during recovery".into());
            }
        }
        tx.rollback().await.map_err(|e| e.to_string())?;
    }
    Ok(count)
}
#[derive(Debug)]
pub(crate) struct StageInspection {
    pub attempt_id: String,
    pub operation_id: Option<String>,
    pub class: UploadClass,
}
pub(crate) async fn inspect_namespace(
    root: &Path,
    conn: &mut AnyConnection,
) -> Result<Vec<StageInspection>, String> {
    let rt = stage::root(root).map_err(|e| e.to_string())?;
    let ns = match directory(&rt, NS, false) {
        Ok(d) => d,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.to_string()),
    };
    let mut result = vec![];
    for e in std::fs::read_dir(root.join(NS)).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let id = e
            .file_name()
            .into_string()
            .map_err(|_| "NonUTF8 upload attempt")?;
        let op = operation(&rt, &id).map_err(|e| e.to_string())?;
        let request = bytes(&op, "request.json").and_then(|(_, b)| decode::<UploadRequest>(&b));
        let class = match bytes(&op, "seal.json") {
            Err(e) if e.kind() == io::ErrorKind::NotFound => UploadClass::Unsealed,
            Err(e) => return Err(e.to_string()),
            Ok((_, b)) => {
                let _: stage::Seal = decode(&b).map_err(|e| e.to_string())?;
                sealed_evidence(&op, &id).map_err(|e| e.to_string())?;
                match request.as_ref() {
                    Ok(q) => match receipt(conn, &q.operation_id).await? {
                        Some(r) if r.intent.attempt_id == id => {
                            inspect(root, conn, &q.operation_id).await?.class
                        }
                        _ => UploadClass::SealedUncommitted,
                    },
                    Err(_) => UploadClass::SealedUncommitted,
                }
            }
        };
        result.push(StageInspection {
            attempt_id: id,
            operation_id: request.ok().map(|q| q.operation_id),
            class,
        });
    }
    let fresh = directory(&stage::root(root).map_err(|e| e.to_string())?, NS, false)
        .map_err(|e| e.to_string())?;
    if !stage::same(&ns, &fresh).map_err(|e| e.to_string())? {
        return Err("Upload namespace changed".into());
    }
    Ok(result)
}

#[path = "attachment_version_read_fence.rs"]
mod version_read_fence;
pub(crate) use version_read_fence::HeldVersionPath;

#[cfg(target_os="linux")]
pub(super) fn family_root(path:&Path)->io::Result<File>{stage::root(path)}

#[cfg(test)]
mod draft_upload_scope_tests {
    use super::*;
    const LEGACY_REQUEST: &str = "{\"operation_id\":\"11111111-1111-4111-8111-111111111111\",\"origin\":\"browser\",\"actor_id\":\"actor\",\"space_id\":\"space\",\"document_id\":\"page\",\"original_name\":\"x.bin\",\"mime_type\":\"application/octet-stream\"}";
    fn request() -> UploadRequest { serde_json::from_str(LEGACY_REQUEST).unwrap() }
    fn scope() -> DraftUploadScope {
        DraftUploadScope {copy_source:None, schema_version: 2, draft_id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(), draft_created_at: "2026-10-16T01:02:03Z".into() }
    }
    fn legacy_intent() -> String {
        format!("{{\"version\":1,\"request\":{LEGACY_REQUEST},\"attempt_id\":\"22222222-2222-4222-8222-222222222222\",\"asset_id\":\"33333333-3333-4333-8333-333333333333\",\"storage_path\":\"spaces/slug/attachments/page/33333333-3333-4333-8333-333333333333.bin\",\"filename\":\"33333333-3333-4333-8333-333333333333.bin\",\"created_at\":\"2026-10-16T01:02:03Z\",\"byte_length\":1,\"binary_sha256\":\"{}\"}}", "a".repeat(64))
    }
    struct OwnedRoot(PathBuf);
    impl OwnedRoot {
        fn new() -> Self {
            use std::os::unix::fs::DirBuilderExt;
            let path = std::env::temp_dir().join(format!("draft-upload-scope-{}", uuid::Uuid::new_v4()));
            std::fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for OwnedRoot { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

    #[test]
    fn legacy_request_intent_and_receipt_keep_exact_serialized_bytes() {
        let request = request();
        assert!(request.draft_scope.is_none());
        assert_eq!(json(&request).unwrap(), LEGACY_REQUEST.as_bytes());
        let raw = legacy_intent();
        let intent: Intent = serde_json::from_str(&raw).unwrap();
        validate(&intent).unwrap();
        assert_eq!(json(&intent).unwrap(), raw.as_bytes());
        let receipt_raw = format!("{{\"intent\":{raw},\"intent_sha256\":\"{}\",\"install_proof_sha256\":\"{}\"}}", digest(raw.as_bytes()), "b".repeat(64));
        let receipt: Receipt = serde_json::from_str(&receipt_raw).unwrap();
        assert_eq!(json(&receipt).unwrap(), receipt_raw.as_bytes());
        let producer = admission::Producer::Direct { request };
        assert_eq!(json(&producer).unwrap(), format!("{{\"producer\":\"direct\",\"request\":{LEGACY_REQUEST}}}").as_bytes());
    }

    #[test]
    fn intent_version_and_typed_scope_must_agree_exactly() {
        let mut intent: Intent = serde_json::from_str(&legacy_intent()).unwrap();
        intent.request.draft_scope = Some(scope());
        assert!(validate(&intent).is_err(), "Version1 may not hide draft scope");
        intent.version = 2;
        validate(&intent).unwrap();
        let raw = json(&intent).unwrap();
        let restored: Intent = decode(&raw).unwrap();
        assert_eq!(restored, intent);
        intent.request.draft_scope = None;
        assert!(validate(&intent).is_err(), "Version2 may not become unscoped legacy");
        let mut value = serde_json::to_value(scope()).unwrap();
        value["unexpected"] = serde_json::json!(true);
        assert!(serde_json::from_value::<DraftUploadScope>(value).is_err());
    }

    #[test]
    fn optional_copy_source_preserves_prior_scope_bytes_and_binds_exact_payload() {
        let mut scope=scope();
        assert_eq!(serde_json::to_string(&scope).unwrap(),"{\"schema_version\":2,\"draft_id\":\"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa\",\"draft_created_at\":\"2026-10-16T01:02:03Z\"}");
        scope.copy_source=Some(CopySourceScope{source_kind:None,schema_version:1,source_space_id:"original-space".into(),source_document_id:"original-page".into(),version_id:"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),expected_source_version_sha256:"c".repeat(64),binary_sha256:"a".repeat(64),byte_length:1});
        scope.validate().unwrap();
        let raw=serde_json::to_vec(&scope).unwrap();assert_eq!(serde_json::from_slice::<DraftUploadScope>(&raw).unwrap(),scope);
        let mut intent:Intent=serde_json::from_str(&legacy_intent()).unwrap();intent.version=2;intent.request.draft_scope=Some(scope.clone());validate(&intent).unwrap();
        intent.request.draft_scope.as_mut().unwrap().copy_source.as_mut().unwrap().byte_length=2;
        assert!(validate(&intent).is_err(),"Copy payload length cannot change independently of selected original source");
        intent.request.draft_scope=Some(scope.clone());intent.request.draft_scope.as_mut().unwrap().copy_source.as_mut().unwrap().binary_sha256="d".repeat(64);
        assert!(validate(&intent).is_err(),"Copy payload digest cannot change independently of selected original source");
        let mut invalid=scope.clone();invalid.copy_source.as_mut().unwrap().schema_version=2;assert!(invalid.validate().is_err());
        let mut bad=scope.clone();bad.copy_source.as_mut().unwrap().byte_length=-1;assert!(bad.validate().is_err());
        let mut bad=scope;bad.copy_source.as_mut().unwrap().expected_source_version_sha256="A".repeat(64);assert!(bad.validate().is_err());
    }

    #[test]
    fn legacy_copy_kind_preserves_opaque_identity_without_relaxing_registered_ids() {
        let mut copy=CopySourceScope{schema_version:1,source_kind:None,source_space_id:"space".into(),source_document_id:"page".into(),version_id:"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),expected_source_version_sha256:"c".repeat(64),binary_sha256:"a".repeat(64),byte_length:1};
        let prior=serde_json::to_value(&copy).unwrap();assert!(prior.get("source_kind").is_none());
        copy.version_id="old-asset_opaque.42".into();assert!(copy.validate().is_err());
        copy.source_kind=Some("legacy_snapshot_v1".into());copy.validate().unwrap();
        let raw=serde_json::to_vec(&copy).unwrap();assert_eq!(serde_json::from_slice::<CopySourceScope>(&raw).unwrap(),copy);
        for bad in ["".to_string(),".".into(),"..".into(),"a/b".into(),"a\\b".into(),"a\0b".into(),"a".repeat(256)] {
            let mut invalid=copy.clone();invalid.version_id=bad;assert!(invalid.validate().is_err());
        }
        copy.source_kind=Some("unknown".into());assert!(copy.validate().is_err());
    }

    #[test]
    fn malformed_scopes_refuse_before_creating_upload_stage() {
        let mut malformed = Vec::new();
        let mut old = scope(); old.schema_version = 1; malformed.push(old);
        let mut upper = scope(); upper.draft_id = upper.draft_id.to_uppercase(); malformed.push(upper);
        let mut empty = scope(); empty.draft_created_at = " \n\t".into(); malformed.push(empty);
        let mut invalid = scope(); invalid.draft_id = "another-draft".into(); malformed.push(invalid);
        for invalid in malformed {
            let root = OwnedRoot::new();
            let mut request = request(); request.draft_scope = Some(invalid);
            assert!(Ingest::reserve(&root.0, request, 8).is_err());
            assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 0);
        }
    }

    #[test]
    fn original_scope_mismatch_cannot_enter_admitted_stage() {
        let root = OwnedRoot::new();
        let mut original = request(); original.draft_scope = Some(scope());
        let lease = admission::WriterLease::begin(&root.0, &uuid::Uuid::new_v4().to_string(), admission::Producer::Direct {request: original.clone()}, 8).unwrap();
        for replacement in [None, Some(DraftUploadScope {draft_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(), ..scope()}), Some(DraftUploadScope {draft_created_at: "2026-10-16T04:05:06Z".into(), ..scope()})] {
            let mut changed = original.clone(); changed.draft_scope = replacement;
            assert!(Ingest::reserve_with_lease(&root.0, changed, 8, lease.clone()).is_err());
            lease.verify().unwrap();
            assert!(!root.0.join(NS).exists(), "Rejected scope cannot create a byte stage");
        }
        let mut ingest = Ingest::reserve_with_lease(&root.0, original.clone(), 8, lease.clone()).unwrap();
        ingest.append(b"x").unwrap();
        let captured = ingest.finish().unwrap();
        let prepared = captured.prepare("slug", "bin").unwrap();
        assert!(prepared.captured.matches(&prepared.receipt));
        let mut changed = prepared.receipt.clone(); changed.intent.request.draft_scope = None;
        assert!(!prepared.captured.matches(&changed), "Same operation and bytes cannot relabel a scoped upload as legacy");
        let mut changed = prepared.receipt.clone();
        changed.intent.request.draft_scope.as_mut().unwrap().draft_created_at = "2026-10-16T04:05:06Z".into();
        assert!(!prepared.captured.matches(&changed), "Same operation and bytes cannot adopt another draft incarnation");
        assert_eq!(prepared.receipt.intent.version, 2);
        assert_eq!(prepared.receipt.intent.request, original);
        let op = operation(&stage::root(&root.0).unwrap(), &prepared.receipt.intent.attempt_id).unwrap();
        let (seal, restored_request) = sealed_evidence(&op, &prepared.receipt.intent.attempt_id).unwrap();
        assert_eq!(seal.version, 1, "Physical byte seal format is unchanged");
        assert_eq!(restored_request, original);
        let receipt = prepared.install().unwrap();
        assert!(verify(&root.0, &receipt, false).unwrap());
    }
}
