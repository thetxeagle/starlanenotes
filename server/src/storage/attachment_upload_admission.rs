//! Native, typed admission ownership. A lease proves only writer exclusion, not SQL absence.
//! Capsule precedes durable quota reservation; no TTL or PID inference grants disposal.
use super::{bytes, decode, digest, fail, json, stage, uuid, UploadRequest};
use serde::{Deserialize, Serialize};
use std::{fs::File, io, os::{fd::AsRawFd, unix::fs::MetadataExt}, path::{Path, PathBuf}, sync::Arc};
const NS: &str = ".attachment-upload-admission";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "producer", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Producer {
    Direct { request: UploadRequest },
    Replay { provider: String, incoming_id: String, actor_id: String, space_id: String,
        document_id: String, asset_id: String, payload_fingerprint: String,
        binary_sha256: String, byte_length: u64,
        #[serde(default,skip_serializing_if="Option::is_none")]
        draft_scope: Option<super::DraftUploadScope> },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Identity { device: u64, inode: u64 }
fn identity(f: &File) -> io::Result<Identity> {
    let m=f.metadata()?; Ok(Identity { device:m.dev(), inode:m.ino() })
}
fn private_directory(f: &File) -> io::Result<()> {
    let m=f.metadata()?;
    if !m.is_dir() || m.mode() & 0o077 != 0 || m.uid()!=unsafe { libc::geteuid() } {
        return Err(fail("Admission directory must be private and owned"));
    } Ok(())
}
fn mkdir(parent: &File, component: &str, exclusive: bool) -> io::Result<File> {
    if component.is_empty() || component.contains('/') || component.contains('\\') || component=="." || component==".." {
        return Err(fail("Invalid admission component"));
    }
    let c=std::ffi::CString::new(component).map_err(|_|fail("NUL admission component"))?;
    // SAFETY: retained parent descriptor and validated single terminated component.
    if unsafe { libc::mkdirat(parent.as_raw_fd(), c.as_ptr(), 0o700) }<0 {
        let e=io::Error::last_os_error();
        if exclusive || e.kind()!=io::ErrorKind::AlreadyExists { return Err(e); }
    }
    parent.sync_all()?;
    let d=stage::open(parent,component,libc::O_RDONLY|libc::O_DIRECTORY)?;
    private_directory(&d)?; Ok(d)
}
fn lock(directory: &File) -> io::Result<()> {
    // SAFETY: retained descriptor; nonblocking kernel lease, released only on last Arc close.
    if unsafe { libc::flock(directory.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB) }<0 {
        return Err(io::Error::last_os_error());
    } Ok(())
}
fn valid_hash(s:&str)->bool {s.len()==64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))}
fn validate(producer:&Producer,maximum:u64)->io::Result<()> {
    if maximum>i64::MAX as u64 {return Err(fail("Admission maximum overflow"));}
    match producer {
        Producer::Direct {request:r} => {
            if let Some(scope) = &r.draft_scope { scope.validate()?; }
            if !uuid(&r.operation_id) || r.origin.is_empty() || r.actor_id.is_empty() || r.space_id.is_empty()
                || r.document_id.is_empty() || r.original_name.trim().is_empty() || r.original_name.len()>255
                || r.mime_type.is_empty() {return Err(fail("Invalid direct admission producer"));}
        },
        Producer::Replay {provider,incoming_id,actor_id,space_id,document_id,asset_id,payload_fingerprint,binary_sha256,byte_length,draft_scope} => {
            if let Some(scope)=draft_scope {scope.validate()?;}
            if [provider,incoming_id,actor_id,space_id,document_id].iter().any(|s|s.is_empty())
                || asset_id.is_empty() || !valid_hash(payload_fingerprint) || !valid_hash(binary_sha256)
                || *byte_length>maximum {return Err(fail("Invalid replay admission producer"));}
        }
    } Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Claim { version:u8, attempt_id:String, producer:Producer, producer_sha256:String,
    maximum_bytes:u64, root:Identity, namespace:Identity, capsule:Identity }
struct Inner { root_path:PathBuf, root:File, namespace:File, directory:File,
    claim_file:File, raw:Vec<u8>, claim:Claim }
/// Clones share the same kernel lease. Keep a clone through the SQL commit/rollback decision,
/// including spawned blocking staging work. Writer cancellation alone is not a disposal decision.
#[derive(Clone)]
pub(crate) struct WriterLease(Arc<Inner>);
impl WriterLease {
    pub(crate) fn begin(root:&Path,attempt:&str,producer:Producer,maximum:u64)->io::Result<Self> {
        if !uuid(attempt) {return Err(fail("Invalid admission attempt"));}
        validate(&producer,maximum)?;
        let _family=crate::storage::native_family_fence::Creation::shared(root)?;
        let root_file=stage::root(root)?;
        let namespace=mkdir(&root_file,NS,false)?;
        let directory=mkdir(&namespace,attempt,true)?;
        lock(&directory)?;
        let claim=Claim {version:1,attempt_id:attempt.into(),producer_sha256:digest(&json(&producer)?),
            producer,maximum_bytes:maximum,root:identity(&root_file)?,namespace:identity(&namespace)?,capsule:identity(&directory)?};
        let raw=json(&claim)?;
        let claim_file=stage::write_envelope(&directory,"claim.json",&raw)?;
        let held=Self(Arc::new(Inner {root_path:root.to_owned(),root:root_file,namespace,directory,claim_file,raw,claim}));
        held.verify()?; Ok(held)
    }
    /// Acquire an idle native capsule. Restored capsules require a separate authenticated recovery
    /// adapter; this never substitutes new inode identities or rewrites original claim bytes.
    pub(crate) fn open_idle(root:&Path,attempt:&str)->io::Result<Self> {
        if !uuid(attempt) {return Err(fail("Invalid admission attempt"));}
        let root_file=stage::root(root)?;
        let namespace=stage::open(&root_file,NS,libc::O_RDONLY|libc::O_DIRECTORY)?;
        private_directory(&namespace)?;
        let directory=stage::open(&namespace,attempt,libc::O_RDONLY|libc::O_DIRECTORY)?;
        private_directory(&directory)?;
        lock(&directory)?;
        let (claim_file,raw)=bytes(&directory,"claim.json")?;
        let claim:Claim=decode(&raw)?;
        if claim.version!=1 || claim.attempt_id!=attempt {return Err(fail("Admission claim differs"));}
        validate(&claim.producer,claim.maximum_bytes)?;
        if claim.producer_sha256!=digest(&json(&claim.producer)?) {return Err(fail("Admission producer digest differs"));}
        let held=Self(Arc::new(Inner {root_path:root.to_owned(),root:root_file,namespace,directory,claim_file,raw,claim}));
        held.verify()?; Ok(held)
    }
    pub(crate) fn verify(&self)->io::Result<()> {
        let h=&self.0;
        let current_root=stage::root(&h.root_path)?;
        let current_ns=stage::open(&current_root,NS,libc::O_RDONLY|libc::O_DIRECTORY)?;
        let current_dir=stage::open(&current_ns,&h.claim.attempt_id,libc::O_RDONLY|libc::O_DIRECTORY)?;
        private_directory(&current_ns)?;private_directory(&current_dir)?;
        if !stage::same(&h.root,&current_root)? || !stage::same(&h.namespace,&current_ns)?
            || !stage::same(&h.directory,&current_dir)? || identity(&h.root)?!=h.claim.root
            || identity(&h.namespace)?!=h.claim.namespace || identity(&h.directory)?!=h.claim.capsule {
            return Err(fail("Admission native custody replaced"));
        }
        let (claim_file,raw)=bytes(&current_dir,"claim.json")?;
        if !stage::same(&claim_file,&h.claim_file)? || raw!=h.raw
            || claim_file.metadata()?.uid()!=unsafe {libc::geteuid()} {return Err(fail("Admission claim replaced"));}
        Ok(())
    }
    pub(crate) fn matches_root(&self,root:&Path)->io::Result<()> {
        if !stage::same(&self.0.root,&stage::root(root)?)? {return Err(fail("Admission stage root differs"));} Ok(())
    }
    pub(crate) fn attempt_id(&self)->&str {&self.0.claim.attempt_id}
    pub(crate) fn producer(&self)->&Producer {&self.0.claim.producer}
    pub(crate) fn producer_sha256(&self)->&str {&self.0.claim.producer_sha256}
    pub(crate) fn claim_sha256(&self)->String {digest(&self.0.raw)}
    pub(crate) fn maximum_bytes(&self)->u64 {self.0.claim.maximum_bytes}
    pub(crate) fn original_claim(&self)->&[u8] {&self.0.raw}
}
#[cfg(test)]
mod tests {
    use super::*;
    fn root()->PathBuf {let p=std::env::temp_dir().join(format!("admission-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&p).unwrap();p}
    fn producer()->Producer {Producer::Direct {request:UploadRequest {draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"browser".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),original_name:"x.bin".into(),mime_type:"application/octet-stream".into()}}}
    #[test] fn malformed_direct_draft_scope_cannot_create_admission() {
        for scope in [
            super::super::DraftUploadScope {copy_source:None,schema_version:1,draft_id:"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),draft_created_at:"created".into()},
            super::super::DraftUploadScope {copy_source:None,schema_version:2,draft_id:"AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA".into(),draft_created_at:"created".into()},
            super::super::DraftUploadScope {copy_source:None,schema_version:2,draft_id:"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),draft_created_at:String::new()},
        ] {
            let r=root();let mut p=producer();
            let Producer::Direct {request}=&mut p else {unreachable!()};request.draft_scope=Some(scope);
            assert!(WriterLease::begin(&r,&uuid::Uuid::new_v4().to_string(),p,8).is_err());
            assert_eq!(std::fs::read_dir(&r).unwrap().count(),0,"Malformed scope must refuse before admission namespace writes");
            std::fs::remove_dir_all(r).unwrap();
        }
    }
    #[test] fn replay_scope_preserves_legacy_bytes_and_validates_new_original_scope() {
        let raw=format!("{{\"producer\":\"replay\",\"provider\":\"provider\",\"incoming_id\":\"incoming\",\"actor_id\":\"actor\",\"space_id\":\"space\",\"document_id\":\"page\",\"asset_id\":\"asset\",\"payload_fingerprint\":\"{}\",\"binary_sha256\":\"{}\",\"byte_length\":1}}", "a".repeat(64), "b".repeat(64));
        let mut producer:Producer=decode(raw.as_bytes()).unwrap();
        validate(&producer,8).unwrap();
        assert_eq!(json(&producer).unwrap(),raw.as_bytes());
        let Producer::Replay {draft_scope,..}=&mut producer else {unreachable!()};
        *draft_scope=Some(super::super::DraftUploadScope{copy_source:None,schema_version:2,draft_id:"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),draft_created_at:"original-created".into()});
        validate(&producer,8).unwrap();
        let original=json(&producer).unwrap();
        assert_eq!(decode::<Producer>(&original).unwrap(),producer);
        let Producer::Replay {draft_scope,..}=&mut producer else {unreachable!()};
        draft_scope.as_mut().unwrap().draft_created_at="another-incarnation".into();
        assert_ne!(json(&producer).unwrap(),original,"Original producer digest includes exact draft incarnation");
        let Producer::Replay {draft_scope,..}=&mut producer else {unreachable!()};
        draft_scope.as_mut().unwrap().schema_version=1;
        assert!(validate(&producer,8).is_err());
    }
    #[test] fn active_and_last_clone_fences() {
        let r=root();let id=uuid::Uuid::new_v4().to_string();let lease=WriterLease::begin(&r,&id,producer(),8).unwrap();
        let held=lease.clone();drop(lease);assert!(WriterLease::open_idle(&r,&id).is_err());
        drop(held);let idle=WriterLease::open_idle(&r,&id).unwrap();idle.verify().unwrap();drop(idle);std::fs::remove_dir_all(r).unwrap();
    }
    #[test] fn equal_byte_claim_replacement_refuses() {
        let r=root();let id=uuid::Uuid::new_v4().to_string();let lease=WriterLease::begin(&r,&id,producer(),8).unwrap();
        let claim=r.join(NS).join(&id).join("claim.json");let raw=std::fs::read(&claim).unwrap();
        std::fs::rename(&claim,claim.with_extension("original")).unwrap();
        use std::os::unix::fs::OpenOptionsExt;
        let mut replacement=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&claim).unwrap();
        use std::io::Write;replacement.write_all(&raw).unwrap();assert!(lease.verify().is_err());
        drop(lease);std::fs::remove_dir_all(r).unwrap();
    }
    #[test] fn substituted_native_capsule_refuses() {
        let r=root();let id=uuid::Uuid::new_v4().to_string();let lease=WriterLease::begin(&r,&id,producer(),8).unwrap();
        let dir=r.join(NS).join(&id);let moved=dir.with_extension("original");std::fs::rename(&dir,&moved).unwrap();
        std::fs::create_dir(&dir).unwrap();use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&dir,std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::copy(moved.join("claim.json"),dir.join("claim.json")).unwrap();
        assert!(lease.verify().is_err());assert!(WriterLease::open_idle(&r,&id).is_err());drop(lease);std::fs::remove_dir_all(r).unwrap();
    }
    #[test] fn typed_producers_and_request_binding() {
        let r=root();let id=uuid::Uuid::new_v4().to_string();let lease=WriterLease::begin(&r,&id,producer(),8).unwrap();
        let other=root();assert!(lease.matches_root(&other).is_err());assert!(WriterLease::begin(&r,&id,producer(),8).is_err());
        let invalid=Producer::Replay {draft_scope:None,provider:"p".into(),incoming_id:"i".into(),actor_id:"a".into(),space_id:"s".into(),document_id:"d".into(),asset_id:uuid::Uuid::new_v4().to_string(),payload_fingerprint:"a".repeat(64),binary_sha256:"b".repeat(64),byte_length:9};
        assert!(WriterLease::begin(&r,&uuid::Uuid::new_v4().to_string(),invalid,8).is_err());drop(lease);std::fs::remove_dir_all(r).unwrap();std::fs::remove_dir_all(other).unwrap();
    }
    #[test] fn process_writer_child() {
        let Ok(r)=std::env::var("STARLANE_ADMISSION_TEST_CHILD_ROOT") else {return};
        let id=std::env::var("STARLANE_ADMISSION_TEST_CHILD_ID").unwrap();let _held=WriterLease::begin(Path::new(&r),&id,producer(),8).unwrap();
        std::fs::write(Path::new(&r).join("ready"),b"lease-held").unwrap();loop {std::thread::park();}
    }
    #[test] fn real_process_kill_releases_kernel_lease() {
        let r=root();let id=uuid::Uuid::new_v4().to_string();let mut child=std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact","storage::attachment_upload::admission::tests::process_writer_child","--nocapture"])
            .env("STARLANE_ADMISSION_TEST_CHILD_ROOT",&r).env("STARLANE_ADMISSION_TEST_CHILD_ID",&id).spawn().unwrap();
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);
        while !r.join("ready").exists() {if std::time::Instant::now()>deadline {let _=child.kill();panic!("child readiness timeout");}std::thread::sleep(std::time::Duration::from_millis(10));}
        assert!(WriterLease::open_idle(&r,&id).is_err());child.kill().unwrap();child.wait().unwrap();
        let idle=WriterLease::open_idle(&r,&id).unwrap();idle.verify().unwrap();drop(idle);std::fs::remove_dir_all(r).unwrap();
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StageOrigin {version:u8,claim_sha256:String,namespace:Identity,directory:Identity,payload:Identity,
    reservation_sha256:String,request_sha256:String}
impl WriterLease {
    /// Called before the first payload append. Original native stage ownership survives a crash.
    pub(super) fn bind_direct_stage(&self,stage:&stage::Stage,request_raw:&[u8])->io::Result<()> {
        self.verify()?;self.matches_root(&stage.root_path)?;
        let Producer::Direct {request}=self.producer() else {return Err(fail("Replay producer cannot bind direct stage"));};
        if json(request)?!=request_raw {return Err(fail("Stage producer request differs"));}
        let ns=stage::open(&self.0.root,stage::NAMESPACE,libc::O_RDONLY|libc::O_DIRECTORY)?;
        let dir=stage::open(&ns,self.attempt_id(),libc::O_RDONLY|libc::O_DIRECTORY)?;
        if !stage::same(&dir,&stage.directory)? {return Err(fail("Direct stage replaced before binding"));}
        let (_,reservation_raw)=bytes(&dir,"reservation.json")?;
        let reservation:stage::Reservation=decode(&reservation_raw)?;
        if reservation.operation_id!=self.attempt_id() || reservation.maximum_bytes!=self.maximum_bytes()
            || reservation.actor_id!=request.actor_id || reservation.space_id!=request.space_id
            || reservation.document_id!=request.document_id || reservation.original_name!=request.original_name
            || reservation.mime_type!=request.mime_type || reservation.version!=1 {return Err(fail("Direct stage reservation differs"));}
        let payload=stage::open(&dir,"payload.part",libc::O_RDONLY|libc::O_NONBLOCK)?;
        let m=payload.metadata()?;
        if !stage::same(&payload,&stage.payload)? || !m.is_file() || m.len()!=0 || m.nlink()!=1 || m.mode()&0o077!=0 {
            return Err(fail("Direct stage binding requires original empty payload"));
        }
        let origin=StageOrigin {version:1,claim_sha256:self.claim_sha256(),namespace:identity(&ns)?,directory:identity(&dir)?,
            payload:identity(&payload)?,reservation_sha256:digest(&reservation_raw),request_sha256:digest(request_raw)};
        stage::write_envelope(&self.0.directory,"stage-origin.json",&json(&origin)?)?;
        self.verify()?;Ok(())
    }
}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
struct Disposal {version:u8,attempt_id:String,claim_json:String,stage_origin_json:Option<String>,
    envelopes:std::collections::BTreeMap<String,String>,payload:Option<Identity>,payload_length:u64,
    payload_sha256:Option<String>,installed_path:Option<String>,request_sha256:String,admission_claim_sha256:String,maximum_bytes:i64}
fn optional_bytes(dir:&File,name:&str)->io::Result<Option<Vec<u8>>> {
    match bytes(dir,name) {Ok((_,raw))=>Ok(Some(raw)),Err(e) if e.kind()==io::ErrorKind::NotFound=>Ok(None),Err(e)=>Err(e)}
}
fn unlink_owned(dir:&File,name:&str,expected:&File)->io::Result<()> {
    let fresh=stage::open(dir,name,libc::O_RDONLY|libc::O_NONBLOCK)?;
    if !stage::same(&fresh,expected)? {return Err(fail("Disposal path replaced before unlink"));}
    let c=std::ffi::CString::new(name).map_err(|_|fail("NUL disposal name"))?;
    if name.is_empty() || name.contains('/') || name.contains('\\') || name=="." || name==".." {return Err(fail("Invalid disposal name"));}
    // SAFETY: held parent FD and validated single component; ownership was verified while lease held.
    if unsafe {libc::unlinkat(dir.as_raw_fd(),c.as_ptr(),0)}<0 {return Err(io::Error::last_os_error());}
    dir.sync_all()
}
fn optional_file(dir:&File,name:&str)->io::Result<Option<File>> {
    match stage::open(dir,name,libc::O_RDONLY|libc::O_NONBLOCK) {Ok(f)=>Ok(Some(f)),Err(e) if e.kind()==io::ErrorKind::NotFound=>Ok(None),Err(e)=>Err(e)}
}
impl WriterLease {
    fn stage_directory(&self)->io::Result<Option<(File,File)>> {
        let ns=match stage::open(&self.0.root,stage::NAMESPACE,libc::O_RDONLY|libc::O_DIRECTORY) {
            Ok(f)=>f,Err(e) if e.kind()==io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(e)};
        let dir=match stage::open(&ns,self.attempt_id(),libc::O_RDONLY|libc::O_DIRECTORY) {
            Ok(f)=>f,Err(e) if e.kind()==io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(e)};
        private_directory(&ns)?;private_directory(&dir)?;Ok(Some((ns,dir)))
    }
    fn stage_origin(&self)->io::Result<Option<Vec<u8>>> {optional_bytes(&self.0.directory,"stage-origin.json")}
    fn verify_origin(&self,ns:&File,dir:&File,raw:&[u8])->io::Result<StageOrigin> {
        let origin:StageOrigin=decode(raw)?;
        if origin.version!=1 || origin.claim_sha256!=self.claim_sha256() || origin.namespace!=identity(ns)? || origin.directory!=identity(dir)? {
            return Err(fail("Disposal original stage custody differs"));
        }
        let (_,rr)=bytes(dir,"reservation.json")?;let (_,rq)=bytes(dir,"request.json")?;
        let Producer::Direct {request}=self.producer() else {return Err(fail("Replay stage needs its typed disposal adapter"));};
        if digest(&rr)!=origin.reservation_sha256 || digest(&rq)!=origin.request_sha256 || rq!=json(request)? {return Err(fail("Disposal stage envelopes differ"));}
        Ok(origin)
    }
    fn plan_direct(&self)->io::Result<Disposal> {
        self.verify()?;
        let Producer::Direct {..}=self.producer() else {return Err(fail("Replay producer requires replay disposal adapter"));};
        let origin_raw=self.stage_origin()?;let mut envelopes=std::collections::BTreeMap::new();
        let mut payload=None;let mut length=0;let mut sha=None;let mut installed_path=None;
        if let Some((ns,dir))=self.stage_directory()? {
            let origin=self.verify_origin(&ns,&dir,origin_raw.as_deref().ok_or_else(||fail("Stage exists without original native admission binding"))?)?;
            for name in ["reservation.json","request.json","seal.json","intent.json","install-proof.json"] {
                if let Some(raw)=optional_bytes(&dir,name)? {envelopes.insert(name.into(),String::from_utf8(raw).map_err(io::Error::other)?);}
            }
            let f=optional_file(&dir,"payload.part")?.ok_or_else(||fail("Payload missing before durable disposal intent"))?;
            let m=f.metadata()?;
            if identity(&f)?!=origin.payload || !m.is_file() || m.mode()&0o077!=0 || m.len()>self.maximum_bytes() {return Err(fail("Disposal payload substituted"));}
            length=m.len();sha=Some(super::hash(&f,length)?);payload=Some(identity(&f)?);
            match (envelopes.get("intent.json"),envelopes.get("install-proof.json")) {
                (None,None)=>if m.nlink()!=1 {return Err(fail("Uninstalled disposal payload has foreign alias"));},
                (Some(ib),Some(pb))=> {
                    let i:super::Intent=decode(ib.as_bytes())?;super::validate(&i)?;
                    let p:super::InstallProof=decode(pb.as_bytes())?;
                    if i.attempt_id!=self.attempt_id() || p.version!=1 || p.intent_sha256!=digest(ib.as_bytes())
                        || (p.original_device,p.original_inode)!=(m.dev(),m.ino())
                        || i.byte_length!=length as i64 || Some(&i.binary_sha256)!=sha.as_ref()
                        || self.producer()!= &(Producer::Direct{request:i.request.clone()}) {return Err(fail("Prepared disposal proof differs"));}
                    let destination=match super::target(&self.0.root,&i.storage_path,false) {Ok((td,n))=>optional_file(&td,&n)?,Err(e) if e.kind()==io::ErrorKind::NotFound=>None,Err(e)=>return Err(e)};
                    match destination {Some(tf)=> {
                        if !stage::same(&f,&tf)? || m.nlink()!=2 {return Err(fail("Installed disposal alias differs"));}
                        installed_path=Some(i.storage_path);
                    },None=>if m.nlink()!=1 {return Err(fail("Prepared disposal payload has foreign alias"));}}
                },_=>return Err(fail("Incomplete prepared disposal proof"))
            }
        } else if origin_raw.is_some() {return Err(fail("Bound stage directory missing"));}
        Ok(Disposal {version:1,attempt_id:self.attempt_id().into(),claim_json:String::from_utf8(self.original_claim().to_vec()).map_err(io::Error::other)?,
            stage_origin_json:origin_raw.map(String::from_utf8).transpose().map_err(io::Error::other)?,envelopes,payload,payload_length:length,payload_sha256:sha,
            installed_path,request_sha256:self.producer_sha256().into(),admission_claim_sha256:self.claim_sha256(),maximum_bytes:self.maximum_bytes() as i64})
    }
    fn complete_direct(&self,plan:&Disposal)->io::Result<()> {self.complete_direct_fenced(plan,false,&mut ||Ok(())).map(|_|())}
    fn complete_direct_fenced(&self,plan:&Disposal,one_only:bool,before:&mut impl FnMut()->io::Result<()>)->io::Result<bool> {
        self.verify()?;
        if plan.version!=1 || plan.attempt_id!=self.attempt_id() || plan.claim_json.as_bytes()!=self.original_claim()
            || plan.request_sha256!=self.producer_sha256() || plan.admission_claim_sha256!=self.claim_sha256()
            || plan.maximum_bytes!=self.maximum_bytes() as i64 || self.stage_origin()?.as_deref()!=plan.stage_origin_json.as_ref().map(|s|s.as_bytes()) {
            return Err(fail("Disposal intent capsule differs"));
        }
        if let Some((ns,dir))=self.stage_directory()? {
            let origin=self.verify_origin(&ns,&dir,plan.stage_origin_json.as_ref().ok_or_else(||fail("Disposal stage origin absent"))?.as_bytes())?;
            for name in ["reservation.json","request.json","seal.json","intent.json","install-proof.json"] {
                let current=optional_bytes(&dir,name)?;
                if current.as_deref()!=plan.envelopes.get(name).map(|s|s.as_bytes()) {return Err(fail("Disposal evidence changed"));}
            }
            let payload=optional_file(&dir,"payload.part")?;
            let installed=if let Some(path)=&plan.installed_path {let (td,n)=super::target(&self.0.root,path,false)?;Some((td,n))}else{None};
            let target=if let Some((td,n))=&installed {optional_file(td,n)?}else{None};
            let count=usize::from(payload.is_some())+usize::from(target.is_some());
            for f in [payload.as_ref(),target.as_ref()].into_iter().flatten() {
                let m=f.metadata()?;
                if Some(identity(f)?)!=plan.payload || identity(f)?!=origin.payload || m.nlink()!=count as u64
                    || m.len()!=plan.payload_length || Some(super::hash(f,m.len())?)!=plan.payload_sha256 {
                    return Err(fail("Disposal held payload or alias differs"));
                }
            }
            // Remove canonical alias first; both parents fsynced before final proof and SQL release.
            if target.is_some() {let (td,n)=installed.as_ref().unwrap();before()?;unlink_owned(td,n,target.as_ref().unwrap())?;disposal_checkpoint("canonical-unlinked",&self.0.root_path);if one_only{return Ok(false)}}
            self.verify()?;
            if payload.is_some() {before()?;unlink_owned(&dir,"payload.part",payload.as_ref().unwrap())?;disposal_checkpoint("payload-unlinked",&self.0.root_path);if one_only{return Ok(false)}}
            // Preserve immutable reservation/request/seal/install evidence; no journal deletion.
        } else if plan.stage_origin_json.is_some() {return Err(fail("Disposal bound stage disappeared"));}
        self.verify()?;before()?;Ok(true)
    }
}
/// Private operator primitive, never a TTL janitor. SQL guard remains held across native disposal
/// and exact durable tombstone; a failed/crashed step retains the original reservation charge.
pub(crate) async fn dispose_direct(pool:&sqlx::AnyPool,root:&Path,attempt:&str)->anyhow::Result<String> {
    let held=WriterLease::open_idle(root,attempt)?;
    let Producer::Direct {request}=held.producer() else {anyhow::bail!("Replay producer requires replay disposal adapter")};
    let mut tx=pool.begin().await?;
    let guarded=sqlx::query("UPDATE attachment_capacity_lock SET revision=revision+1 WHERE id=1 AND revision<9223372036854775807").execute(&mut *tx).await?.rows_affected();
    anyhow::ensure!(guarded==1,"Capacity disposal guard missing or exhausted");
    let row:(String,String,i64,String,Option<String>,Option<String>)=sqlx::query_as("SELECT request_sha256,admission_claim_sha256,maximum_bytes,state,disposition_json,disposition_sha256 FROM attachment_capacity_reservations WHERE operation_id=$1").bind(attempt).fetch_one(&mut *tx).await?;
    anyhow::ensure!(row.0==held.producer_sha256() && row.1==held.claim_sha256() && row.2==held.maximum_bytes() as i64,"Capacity disposal capsule differs");
    anyhow::ensure!(row.3=="reserved" || row.3=="disposed","Committed capacity cannot be disposed");
    let prior=super::receipt(&mut tx,&request.operation_id).await.map_err(anyhow::Error::msg)?;
    anyhow::ensure!(prior.as_ref().is_none_or(|r|r.intent.attempt_id!=attempt),"Winning upload attempt cannot be disposed");
    let any_winner:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachment_upload_receipts WHERE attempt_id=$1").bind(attempt).fetch_one(&mut *tx).await?;
    anyhow::ensure!(any_winner==0,"Original committed upload attempt cannot be disposed");
    let protected:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachment_version_records WHERE upload_attempt_id=$1").bind(attempt).fetch_one(&mut *tx).await?;
    anyhow::ensure!(protected==0,"Retained attachment version attempt cannot be disposed");
    let raw=if let Some(raw)=optional_bytes(&held.0.directory,"disposal-intent.json")? {raw}else{
        let plan=held.plan_direct()?;let raw=json(&plan)?;stage::write_envelope(&held.0.directory,"disposal-intent.json",&raw)?;raw};
    disposal_checkpoint("intent-durable",root);
    let plan:Disposal=decode(&raw)?;
    if let Some(ib)=plan.envelopes.get("intent.json") {
        let intent:super::Intent=decode(ib.as_bytes())?;
        let refs:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachments WHERE id=$1 OR storage_path=$2").bind(&intent.asset_id).bind(&intent.storage_path).fetch_one(&mut *tx).await?;
        anyhow::ensure!(refs==0,"Installed disposal alias retains SQL attachment authority");
        let retired:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachment_retirements WHERE attachment_id=$1").bind(&intent.asset_id).fetch_one(&mut *tx).await?;
        anyhow::ensure!(retired==0,"Retained retirement alias cannot be disposed");
    }
    if row.3=="disposed" {
        let tombstone=optional_bytes(&held.0.directory,"disposal-tombstone.json")?.ok_or_else(||fail("Disposed SQL row lacks original tombstone"))?;
        anyhow::ensure!(tombstone==raw && row.4.as_deref()==Some(std::str::from_utf8(&raw)?) && row.5.as_deref()==Some(digest(&raw).as_str()),"Disposed SQL evidence differs");
        held.complete_direct(&plan)?;tx.rollback().await?;return Ok(digest(&raw));
    }
    held.complete_direct(&plan)?;
    if let Some(existing)=optional_bytes(&held.0.directory,"disposal-tombstone.json")? {anyhow::ensure!(existing==raw,"Disposal tombstone differs");}
    else {stage::write_envelope(&held.0.directory,"disposal-tombstone.json",&raw)?;}
    held.verify()?;
    disposal_checkpoint("tombstone-durable",root);
    let changed=sqlx::query("UPDATE attachment_capacity_reservations SET state='disposed',disposition_json=$1,disposition_sha256=$2 WHERE operation_id=$3 AND request_sha256=$4 AND admission_claim_sha256=$5 AND maximum_bytes=$6 AND state='reserved'")
        .bind(std::str::from_utf8(&raw)?).bind(digest(&raw)).bind(attempt).bind(held.producer_sha256()).bind(held.claim_sha256()).bind(held.maximum_bytes() as i64)
        .execute(&mut *tx).await?.rows_affected();anyhow::ensure!(changed==1,"Capacity disposal reservation changed");
    disposal_checkpoint("sql-cas-uncommitted",root);
    tx.commit().await?;Ok(digest(&raw))
}

include!("attachment_upload_admission_sql_tests.inc");

#[cfg(test)]
fn disposal_checkpoint(name:&str,root:&Path) {
    if std::env::var("STARLANE_DISPOSAL_TEST_CHECKPOINT").as_deref()==Ok(name) {
        std::fs::write(root.join("checkpoint-ready"),name.as_bytes()).unwrap();
        loop {std::thread::park();}
    }
}
#[cfg(not(test))]
fn disposal_checkpoint(_name:&str,_root:&Path) {}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
struct ReplayBlobOrigin {version:u8,claim_sha256:String,path:String,parent:Identity,payload:Identity,
    expected_length:u64,expected_sha256:String}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
struct ReplayRewritePlan {version:u8,claim_sha256:String,op_id:String,actor_id:String,
    space_id:String,document_id:String,provider:String,incoming_id:String,asset_id:String,payload_fingerprint:String,
    source_path:String,source_revision:String,published_stamp:String,revision:String,source_sha256:Option<String>,result_sha256:String,result_length:u64}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
struct ReplayRewriteOrigin {version:u8,claim_sha256:String,plan_sha256:String,op_id:String,namespace:Identity,parent:Identity,payload:Identity,manifest_json:String}
impl WriterLease {
    pub(crate) fn bind_replay_rewrite_original(&self,root:&Path,m:&crate::storage::journal_fs::FileDescriptor,manifest:&[u8],parent:&File,payload:&File)->io::Result<()> {
        self.verify()?;self.matches_root(root)?;let (_,plan_raw)=bytes(&self.0.directory,"replay-rewrite-plan.json")?;let plan:ReplayRewritePlan=decode(&plan_raw)?;
        let _:crate::storage::canonical_rewrite::PreparedRewrite=decode(manifest)?;
        let prepared:serde_json::Value=decode(manifest)?;let descriptor=serde_json::to_value(m).map_err(|_|fail("Invalid canonical descriptor"))?;
        let incoming=&prepared["incoming"];
        if !descriptor.as_object().ok_or_else(||fail("Invalid canonical descriptor object"))?.iter().all(|(k,v)|prepared[k]==*v)
            || m.op_id!=plan.op_id || m.sha256!=plan.result_sha256 || m.length<0 || m.length as u64!=plan.result_length
            || m.space_id!=plan.space_id || m.document_id!=plan.document_id || prepared["actor_id"].as_str()!=Some(plan.actor_id.as_str())
            || prepared["source_path"].as_str()!=Some(plan.source_path.as_str()) || prepared["source_revision"].as_str()!=Some(plan.source_revision.as_str())
            || prepared["published_stamp"].as_str()!=Some(plan.published_stamp.as_str()) || prepared["source_sha256"].as_str()!=plan.source_sha256.as_deref()
            || prepared["reason"].as_str()!=Some("AssetReferenceRewrite") || incoming["provider"].as_str()!=Some(plan.provider.as_str())
            || incoming["operation_id"].as_str()!=Some(plan.incoming_id.as_str()) || incoming["entity_type"].as_str()!=Some("asset")
            || incoming["entity_id"].as_str()!=Some(plan.asset_id.as_str()) || incoming["action"].as_str()!=Some("create")
            || incoming["payload_fingerprint"].as_str()!=Some(plan.payload_fingerprint.as_str()) {
            return Err(fail("Replay original canonical stage differs from typed plan"));
        }
        let ns=stage::open(&self.0.root,".canonical-rewrite-journal",libc::O_RDONLY|libc::O_DIRECTORY)?;let dir=stage::open(&ns,&plan.op_id,libc::O_RDONLY|libc::O_DIRECTORY)?;
        let current=stage::open(&dir,"payload",libc::O_RDONLY|libc::O_NONBLOCK)?;let meta=payload.metadata()?;
        if !stage::same(&dir,parent)? || !stage::same(&current,payload)? || !meta.is_file() || meta.nlink()!=1 || meta.len()!=0 || meta.mode()&0o077!=0 {return Err(fail("Replay rewrite origin requires original exclusive empty payload"));}
        let record=ReplayRewriteOrigin {version:1,claim_sha256:self.claim_sha256(),plan_sha256:digest(&plan_raw),op_id:plan.op_id,namespace:identity(&ns)?,parent:identity(parent)?,payload:identity(payload)?,manifest_json:String::from_utf8(manifest.to_vec()).map_err(|_|fail("NonUTF8 original manifest"))?};
        stage::write_envelope(&self.0.directory,"replay-rewrite-origin.json",&json(&record)?)?;self.verify()
    }
    pub(crate) fn replay_rewrite_checkpoint(&self,name:&str,root:&Path) {disposal_checkpoint(name,root);}
    pub(crate) fn validate_replay_payload(&self,root:&Path,document:&str,bytes:&[u8])->io::Result<()> {
        self.verify()?;self.matches_root(root)?;
        let Producer::Replay {document_id,binary_sha256,byte_length,..}=self.producer() else {return Err(fail("Direct producer cannot stage replay blob"));};
        if document_id!=document || *byte_length!=bytes.len() as u64 || *binary_sha256!=digest(bytes) {return Err(fail("Replay payload typed scope or bytes differ"));}Ok(())
    }
    pub(crate) fn bind_replay_blob_original(&self,root:&Path,path:&str,parent:&File,payload:&File,bytes:&[u8])->io::Result<()> {
        self.verify()?;self.matches_root(root)?;
        let Producer::Replay {document_id,binary_sha256,byte_length,..}=self.producer() else {return Err(fail("Direct producer cannot bind replay blob"));};
        let parts:Vec<_>=path.split('/').collect();
        if parts.len()!=5 || parts[0]!="spaces" || parts[2]!="attachments" || parts[3]!=document_id
            || parts.iter().any(|p|p.is_empty()||*p=="."||*p==".."||p.contains('\\')||p.contains('\0'))
            || bytes.len() as u64!=*byte_length || digest(bytes)!=*binary_sha256 {return Err(fail("Replay blob producer bytes differ"));}
        let (current_parent,name)=super::target(&self.0.root,path,false)?;let current=stage::open(&current_parent,&name,libc::O_RDONLY|libc::O_NONBLOCK)?;
        let m=payload.metadata()?;
        if !stage::same(&current_parent,parent)? || !stage::same(&current,payload)? || !m.is_file() || m.nlink()!=1 || m.len()!=0 || m.mode()&0o077!=0 {
            return Err(fail("Replay blob binding requires original exclusive empty inode"));
        }
        let origin=ReplayBlobOrigin {version:1,claim_sha256:self.claim_sha256(),path:path.into(),parent:identity(parent)?,payload:identity(payload)?,
            expected_length:*byte_length,expected_sha256:binary_sha256.clone()};
        stage::write_envelope(&self.0.directory,"replay-blob-origin.json",&json(&origin)?)?;self.verify()
    }
    pub(crate) fn verify_replay_blob(&self,root:&Path,path:&str)->io::Result<()> {
        self.verify()?;self.matches_root(root)?;let (_,raw)=bytes(&self.0.directory,"replay-blob-origin.json")?;let origin:ReplayBlobOrigin=decode(&raw)?;
        let Producer::Replay {binary_sha256,byte_length,..}=self.producer() else {return Err(fail("Direct producer has no replay blob custody"));};
        let (dir,name)=super::target(&self.0.root,path,false)?;let f=stage::open(&dir,&name,libc::O_RDONLY|libc::O_NONBLOCK)?;let m=f.metadata()?;
        if origin.version!=1 || origin.claim_sha256!=self.claim_sha256() || origin.path!=path || origin.parent!=identity(&dir)? || origin.payload!=identity(&f)?
            || origin.expected_length!=*byte_length || origin.expected_sha256!=*binary_sha256 || m.nlink()!=1 || super::hash(&f,*byte_length)?!=*binary_sha256 {
            return Err(fail("Replay blob original custody differs"));
        } Ok(())
    }
    pub(crate) fn bind_replay_rewrite_plan(&self,root:&Path,op:&str,actor:&str,intent:&crate::storage::canonical_rewrite::RewriteIntent,result:&[u8])->io::Result<()> {
        self.verify()?;self.matches_root(root)?;
        let Producer::Replay {provider,incoming_id,actor_id,space_id,document_id,asset_id,payload_fingerprint,..}=self.producer() else {return Err(fail("Direct producer cannot bind asset reference rewrite"));};
        let i=&intent.incoming;
        if !uuid(op) || actor!=actor_id || intent.space_id!=*space_id || intent.document_id!=*document_id
            || i.provider!=*provider || i.operation_id!=*incoming_id || i.entity_type!="asset" || i.entity_id!=*asset_id
            || i.action!="create" || i.payload_fingerprint!=*payload_fingerprint
            || intent.reason!=crate::storage::canonical_rewrite::RewriteReason::AssetReferenceRewrite {return Err(fail("Replay rewrite typed identity differs"));}
        let plan=ReplayRewritePlan {version:1,claim_sha256:self.claim_sha256(),op_id:op.into(),actor_id:actor.into(),
            space_id:space_id.clone(),document_id:document_id.clone(),provider:provider.clone(),incoming_id:incoming_id.clone(),asset_id:asset_id.clone(),payload_fingerprint:payload_fingerprint.clone(),
            source_path:intent.source_path.clone(),source_revision:intent.source_revision.clone(),published_stamp:intent.published_stamp.clone(),revision:intent.revision.clone(),source_sha256:intent.source_sha256.clone(),result_sha256:digest(result),result_length:result.len() as u64};
        stage::write_envelope(&self.0.directory,"replay-rewrite-plan.json",&json(&plan)?)?;self.verify()
    }
}
#[cfg(test)]
mod replay_hook_tests {
 use super::*;
 fn root()->PathBuf {let p=std::env::temp_dir().join(format!("replay-capsule-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&p).unwrap();p}
 fn producer(bytes:&[u8])->Producer {Producer::Replay {draft_scope:None,provider:"private-provider".into(),incoming_id:"opaque-incoming".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),asset_id:"opaque-client-asset".into(),payload_fingerprint:"a".repeat(64),binary_sha256:digest(bytes),byte_length:bytes.len() as u64}}
 #[test] fn replay_blob_original_inode_and_active_writer() {
  let r=root();let attempt=uuid::Uuid::new_v4().to_string();let l=WriterLease::begin(&r,&attempt,producer(b"REPLAY BINARY"),13).unwrap();
  let blob=crate::storage::asset_blob::create_with_lease(&r,"slug","page","bin",b"REPLAY BINARY",l.clone()).unwrap();blob.verify().unwrap();
  let path=blob.path().to_owned();drop(l);assert!(WriterLease::open_idle(&r,&attempt).is_err());drop(blob);
  let l=WriterLease::open_idle(&r,&attempt).unwrap();l.verify_replay_blob(&r,&path).unwrap();
  let p=r.join(&path);let original=p.with_extension("original");std::fs::rename(&p,&original).unwrap();std::fs::copy(&original,&p).unwrap();
  assert!(l.verify_replay_blob(&r,&path).is_err());assert!(p.exists()&&original.exists());drop(l);std::fs::remove_dir_all(r).unwrap();
 }
 #[test] fn replay_producer_wrong_bytes_and_scope_refuse() {
  let r=root();let l=WriterLease::begin(&r,&uuid::Uuid::new_v4().to_string(),producer(b"owned"),5).unwrap();
  assert!(crate::storage::asset_blob::create_with_lease(&r,"slug","foreign","bin",b"owned",l.clone()).is_err());
  assert!(crate::storage::asset_blob::create_with_lease(&r,"slug","page","bin",b"other",l.clone()).is_err());
  drop(l);std::fs::remove_dir_all(r).unwrap();
 }
 #[test] fn replay_rewrite_plan_exact_typed_link() {
  use crate::storage::canonical_rewrite::{IncomingIdentity,RewriteIntent,RewriteReason};
  let r=root();let l=WriterLease::begin(&r,&uuid::Uuid::new_v4().to_string(),producer(b"owned"),5).unwrap();
  let mut i=RewriteIntent {space_id:"space".into(),document_id:"page".into(),source_path:"spaces/slug/docs/original.md".into(),source_revision:"original".into(),published_stamp:"published".into(),revision:"next".into(),source_sha256:Some("b".repeat(64)),reason:RewriteReason::AssetReferenceRewrite,
   incoming:IncomingIdentity{provider:"private-provider".into(),operation_id:"opaque-incoming".into(),entity_type:"asset".into(),entity_id:"opaque-client-asset".into(),action:"create".into(),payload_fingerprint:"c".repeat(64)}};
  let op=uuid::Uuid::new_v4().to_string();assert!(l.bind_replay_rewrite_plan(&r,&op,"actor",&i,b"result").is_err());
  i.incoming.payload_fingerprint="a".repeat(64);l.bind_replay_rewrite_plan(&r,&op,"actor",&i,b"result").unwrap();
  let (_,raw)=bytes(&l.0.directory,"replay-rewrite-plan.json").unwrap();let plan:ReplayRewritePlan=decode(&raw).unwrap();assert_eq!(plan.op_id,op);assert_eq!(plan.result_sha256,digest(b"result"));assert_eq!(plan.asset_id,"opaque-client-asset");
  assert!(l.bind_replay_rewrite_plan(&r,&uuid::Uuid::new_v4().to_string(),"actor",&i,b"result").is_err());drop(l);std::fs::remove_dir_all(r).unwrap();
 }
}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
struct ReplayDisposal {version:u8,attempt_id:String,claim_json:String,origin_json:Option<String>,rewrite_plan_json:Option<String>,
    rewrite_manifest_json:Option<String>,rewrite_install_proof_json:Option<String>,payload:Option<Identity>,payload_length:u64,payload_sha256:Option<String>,
    #[serde(default,skip_serializing_if="Option::is_none")]
    rewrite_incomplete_disposition_json:Option<String>,
    request_sha256:String,admission_claim_sha256:String,maximum_bytes:i64}
fn scoped_file(root:&File,path:&str)->io::Result<File> {
    let parts:Vec<_>=path.split('/').collect();
    if parts.is_empty()||parts.iter().any(|p|p.is_empty()||*p=="."||*p==".."||p.contains('\\')||p.contains('\0')) {return Err(fail("Invalid scoped evidence path"));}
    let mut dir=root.try_clone()?;for p in &parts[..parts.len()-1] {dir=stage::open(&dir,p,libc::O_RDONLY|libc::O_DIRECTORY)?;}
    stage::open(&dir,parts[parts.len()-1],libc::O_RDONLY|libc::O_NONBLOCK)
}
fn contains_scoped(root:&File,path:&str,needle:&[u8])->io::Result<bool> {
    use std::os::unix::fs::FileExt;
    let f=scoped_file(root,path)?;let m=f.metadata()?;
    if !m.is_file() || m.len()>6_000_000 {return Err(fail("Canonical reference inspection requires bounded regular source"));}
    let mut raw=vec![0;m.len() as usize];f.read_exact_at(&mut raw,0)?;
    let fresh=scoped_file(root,path)?;
    if !stage::same(&f,&fresh)? || f.metadata()?.len()!=m.len() {return Err(fail("Canonical reference source changed"));}
    Ok(raw.windows(needle.len()).any(|v|v==needle))
}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
struct ReplayIncompleteDisposition {version:u8,origin_json:String,journal_manifest_json:Option<String>,payload:Identity,payload_length:u64,payload_sha256:String}
impl WriterLease {
    fn replay_origin(&self)->io::Result<Option<Vec<u8>>> {optional_bytes(&self.0.directory,"replay-blob-origin.json")}
    fn replay_plan(&self)->io::Result<Option<Vec<u8>>> {optional_bytes(&self.0.directory,"replay-rewrite-plan.json")}
    fn verify_replay_origin(&self,raw:&[u8])->io::Result<(ReplayBlobOrigin,File,String)> {
        let origin:ReplayBlobOrigin=decode(raw)?;
        let Producer::Replay {document_id,binary_sha256,byte_length,..}=self.producer() else {return Err(fail("Direct producer cannot claim replay disposal"));};
        let parts:Vec<_>=origin.path.split('/').collect();
        if origin.version!=1 || origin.claim_sha256!=self.claim_sha256() || origin.expected_length!=*byte_length || origin.expected_sha256!=*binary_sha256
            || parts.len()!=5 || parts[3]!=document_id {return Err(fail("Replay disposal original producer differs"));}
        let (dir,name)=super::target(&self.0.root,&origin.path,false)?;
        if identity(&dir)?!=origin.parent {return Err(fail("Replay disposal original parent replaced"));}
        Ok((origin,dir,name))
    }
    async fn classify_incomplete_replay_rewrite(&self,tx:&mut sqlx::AnyConnection,plan:&ReplayRewritePlan)->anyhow::Result<Vec<u8>> {
        let (_,origin_raw)=bytes(&self.0.directory,"replay-rewrite-origin.json")?;let origin:ReplayRewriteOrigin=decode(&origin_raw)?;
        let plan_raw=self.replay_plan()?.ok_or_else(||fail("Incomplete rewrite lacks original typed plan"))?;
        anyhow::ensure!(origin.version==1 && origin.claim_sha256==self.claim_sha256() && origin.plan_sha256==digest(&plan_raw) && origin.op_id==plan.op_id,"Incomplete rewrite original custody differs");
        let m:crate::storage::journal_fs::FileDescriptor=decode(origin.manifest_json.as_bytes())?;
        anyhow::ensure!(m.op_id==plan.op_id && m.sha256==plan.result_sha256 && m.length>=0 && m.length as u64==plan.result_length && m.space_id==plan.space_id && m.document_id==plan.document_id,"Incomplete rewrite original descriptor differs");
        let ns=stage::open(&self.0.root,".canonical-rewrite-journal",libc::O_RDONLY|libc::O_DIRECTORY)?;let dir=stage::open(&ns,&plan.op_id,libc::O_RDONLY|libc::O_DIRECTORY)?;
        anyhow::ensure!(identity(&ns)?==origin.namespace && identity(&dir)?==origin.parent,"Incomplete rewrite original journal namespace or directory replaced");
        anyhow::ensure!(optional_file(&dir,"install-proof.json")?.is_none(),"Incomplete rewrite cannot downgrade original install proof");
        let manifest=optional_bytes(&dir,"manifest.json")?;
        anyhow::ensure!(manifest.as_deref().is_none_or(|r|r==origin.manifest_json.as_bytes()),"Incomplete rewrite original manifest substituted");
        let f=stage::open(&dir,"payload",libc::O_RDONLY|libc::O_NONBLOCK)?;let meta=f.metadata()?;
        anyhow::ensure!(identity(&f)?==origin.payload && meta.is_file() && meta.nlink()==1 && meta.mode()&0o077==0 && meta.len()<=plan.result_length,"Incomplete rewrite original payload substituted or foreign linked");
        let sha=super::hash(&f,meta.len())?;
        anyhow::ensure!(meta.len()!=plan.result_length || sha==plan.result_sha256,"Incomplete rewrite completed original payload checksum differs");
        // No installed result and no SQL reference may be inferred from absent stage proof.
        match scoped_file(&self.0.root,&m.path) {
            Ok(_)=>anyhow::bail!("Incomplete rewrite result path exists without original install proof"),
            Err(e) if e.kind()==io::ErrorKind::NotFound=>(),Err(e)=>return Err(e.into())
        }
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM canonical_revision_receipts WHERE op_id=$1 OR content_path=$2").bind(&plan.op_id).bind(&m.path).fetch_one(&mut *tx).await?;
        anyhow::ensure!(count==0,"Committed canonical receipt protects incomplete rewrite");
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE content_path=$1").bind(&m.path).fetch_one(&mut *tx).await?;
        anyhow::ensure!(count==0,"Current canonical result protects incomplete rewrite");
        let fresh=stage::open(&dir,"payload",libc::O_RDONLY|libc::O_NONBLOCK)?;
        anyhow::ensure!(stage::same(&f,&fresh)? && fresh.metadata()?.len()==meta.len() && super::hash(&fresh,meta.len())?==sha,"Incomplete rewrite observed original payload changed");self.verify()?;
        Ok(json(&ReplayIncompleteDisposition {version:1,origin_json:String::from_utf8(origin_raw)?,journal_manifest_json:manifest.map(String::from_utf8).transpose()?,payload:origin.payload,payload_length:meta.len(),payload_sha256:sha})?)
    }
    async fn classify_replay_rewrite(&self,tx:&mut sqlx::AnyConnection)->anyhow::Result<(Option<Vec<u8>>,Option<Vec<u8>>,Option<Vec<u8>>)> {
        let Some(raw)=self.replay_plan()? else {return Ok((None,None,None))};let plan:ReplayRewritePlan=decode(&raw)?;
        let Producer::Replay {provider,incoming_id,actor_id,space_id,document_id,asset_id,payload_fingerprint,..}=self.producer() else {anyhow::bail!("Direct producer cannot classify replay rewrite")};
        anyhow::ensure!(plan.version==1 && plan.claim_sha256==self.claim_sha256() && uuid(&plan.op_id)
            && plan.provider==*provider && plan.incoming_id==*incoming_id && plan.actor_id==*actor_id && plan.space_id==*space_id
            && plan.document_id==*document_id && plan.asset_id==*asset_id && plan.payload_fingerprint==*payload_fingerprint,"Replay rewrite original typed plan differs");
        let ns=stage::open(&self.0.root,".canonical-rewrite-journal",libc::O_RDONLY|libc::O_DIRECTORY)?;let dir=stage::open(&ns,&plan.op_id,libc::O_RDONLY|libc::O_DIRECTORY)?;
        if optional_file(&dir,"install-proof.json")?.is_none() {return Ok((None,None,Some(self.classify_incomplete_replay_rewrite(tx,&plan).await?)));}
        let inspected=crate::storage::canonical_rewrite::inspect_restoration(&self.0.root_path,&mut *tx,&plan.op_id).await.map_err(|e|anyhow::anyhow!("Replay protected manifest requires review: {e:?}"))?;
        anyhow::ensure!(inspected.class==crate::storage::canonical_rewrite::RestorationOperationClass::PreparedUncommitted && inspected.raw_receipt.is_none(),"Committed rewrite protects replay blob");
        let manifest:serde_json::Value=serde_json::from_slice(&inspected.raw_manifest)?;
        let fields=[("op_id",plan.op_id.as_str()),("actor_id",plan.actor_id.as_str()),("space_id",plan.space_id.as_str()),("document_id",plan.document_id.as_str()),
            ("source_path",plan.source_path.as_str()),("source_revision",plan.source_revision.as_str()),("published_stamp",plan.published_stamp.as_str()),("revision",plan.revision.as_str()),("sha256",plan.result_sha256.as_str())];
        anyhow::ensure!(fields.iter().all(|(k,v)|manifest[*k].as_str()==Some(*v)) && manifest["length"].as_u64()==Some(plan.result_length)
            && manifest["source_sha256"].as_str()==plan.source_sha256.as_deref() && manifest["reason"].as_str()==Some("AssetReferenceRewrite"),"Replay protected manifest differs from original admission plan");
        if let Some(origin_raw)=optional_bytes(&self.0.directory,"replay-rewrite-origin.json")? {
            let origin:ReplayRewriteOrigin=decode(&origin_raw)?;let f=stage::open(&dir,"payload",libc::O_RDONLY|libc::O_NONBLOCK)?;
            anyhow::ensure!(origin.version==1 && origin.claim_sha256==self.claim_sha256() && origin.plan_sha256==digest(&raw) && origin.op_id==plan.op_id
                && origin.namespace==identity(&ns)? && origin.parent==identity(&dir)? && origin.payload==identity(&f)?
                && origin.manifest_json.as_bytes()==inspected.raw_manifest.as_slice() && f.metadata()?.nlink()==1
                && f.metadata()?.len()==plan.result_length && super::hash(&f,plan.result_length)?==plan.result_sha256,"Original replay staged rewrite custody differs");
        }
        crate::storage::canonical_rewrite::verify_prepared_files_in(&self.0.root_path,&inspected.manifest).map_err(|e|anyhow::anyhow!("Replay protected current native result requires review: {e:?}"))?;
        self.verify()?;
        let proof=inspected.install.proof.as_ref().ok_or_else(||fail("Replay protected manifest lacks original native install proof"))?;
        // Inspection authenticates the immutable manifest, original native install proof and owned
        // result group; those bytes remain intact. This record grants no install/replay authority.
        let used:i64=sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE content_path=$1").bind(inspected.manifest.path()).fetch_one(&mut *tx).await?;
        anyhow::ensure!(used==0,"Current canonical source still protects replay blob");
        Ok((Some(inspected.raw_manifest),Some(proof.raw.clone()),None))
    }
    async fn replay_protected_refs(&self,tx:&mut sqlx::AnyConnection,origin:Option<&ReplayBlobOrigin>)->anyhow::Result<()> {
        let Producer::Replay {provider,incoming_id,actor_id,space_id,document_id,asset_id,payload_fingerprint,binary_sha256,byte_length,..}=self.producer() else {anyhow::bail!("Direct producer cannot dispatch replay refs")};
        let Some(origin)=origin else {
            let receipts:i64=sqlx::query_scalar("SELECT COUNT(*) FROM asset_replay_receipts WHERE provider=$1 AND incoming_id=$2").bind(provider).bind(incoming_id).fetch_one(&mut *tx).await?;
            anyhow::ensure!(receipts==0,"Receipt exists without original replay blob native binding");return Ok(())};
        let refs:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachments WHERE storage_path=$1").bind(&origin.path).fetch_one(&mut *tx).await?;
        anyhow::ensure!(refs==0,"SQL attachment protects original replay blob");
        let versions:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachment_version_records WHERE original_storage_path=$1").bind(&origin.path).fetch_one(&mut *tx).await?;
        anyhow::ensure!(versions==0,"Retained version protects original replay blob");
        let aliases:i64=sqlx::query_scalar("SELECT COUNT(*) FROM move_asset_path_aliases WHERE old_path=$1").bind(&origin.path).fetch_one(&mut *tx).await?;
        anyhow::ensure!(aliases==0,"Historical move alias protects original replay blob");
        if let Some(retired)=crate::storage::attachment_retirement::read(&mut *tx,asset_id).await.map_err(anyhow::Error::msg)? {
            anyhow::ensure!(retired.association.4!=origin.path,"Original retained retirement protects replay blob");
        }
        let old_receipt:Option<String>=sqlx::query_scalar("SELECT receipt_json FROM asset_replay_receipts WHERE provider=$1 AND incoming_id=$2").bind(provider).bind(incoming_id).fetch_optional(&mut *tx).await?;
        if let Some(raw)=old_receipt {
            let winner:crate::sync::AssetReplayReceipt=serde_json::from_str(&raw)?;
            anyhow::ensure!(winner.incoming.provider==*provider && winner.incoming.operation_id==*incoming_id && winner.incoming.entity_type=="asset" && winner.incoming.action=="create"
                && winner.incoming.entity_id==*asset_id && winner.incoming.payload_fingerprint==*payload_fingerprint && winner.actor_id==*actor_id && winner.space_id==*space_id
                && winner.document_id==*document_id && winner.binary_sha256==*binary_sha256 && winner.byte_length==*byte_length as i64,"Original replay winner receipt differs from typed producer");
            anyhow::ensure!(crate::sync::asset_receipt_current(&mut *tx,&winner).await.map_err(anyhow::Error::msg)?,"Original replay winning receipt SQL proof differs");
            anyhow::ensure!(winner.storage_path!=origin.path,"Original exact replay winning receipt protects blob");
        }
        let receipts:i64=sqlx::query_scalar("SELECT COUNT(*) FROM asset_replay_receipts WHERE storage_path=$1").bind(&origin.path).fetch_one(&mut *tx).await?;
        anyhow::ensure!(receipts==0,"Original replay winning receipt protects blob");
        let url=format!("/api/v1/files/{}",origin.path);
        let planned_op=self.replay_plan()?.map(|r|decode::<ReplayRewritePlan>(&r)).transpose()?.map(|p|p.op_id);
        let namespace_path=self.0.root_path.join(".canonical-rewrite-journal");
        match std::fs::read_dir(&namespace_path) {
            Ok(entries)=>for entry in entries {
                let entry=entry?;let op=entry.file_name().into_string().map_err(|_|fail("NonUTF8 protected canonical journal"))?;
                anyhow::ensure!(uuid(&op),"Unknown canonical journal component");
                let relative=format!(".canonical-rewrite-journal/{op}/payload");
                match contains_scoped(&self.0.root,&relative,url.as_bytes()) {
                    Ok(true)=>anyhow::ensure!(planned_op.as_deref()==Some(op.as_str()),"Other protected canonical manifest references replay blob"),
                    Ok(false)=>(),Err(e) if e.kind()==io::ErrorKind::NotFound=>(),Err(e)=>return Err(e.into())
                }
            },Err(e) if e.kind()==io::ErrorKind::NotFound=>(),Err(e)=>return Err(e.into())
        }
        self.verify()?;let draft_refs:i64=sqlx::query_scalar("SELECT COUNT(*) FROM document_drafts WHERE document_id=$1 AND space_id=$2 AND content LIKE $3")
            .bind(document_id).bind(space_id).bind(format!("%{url}%")).fetch_one(&mut *tx).await?;
        anyhow::ensure!(draft_refs==0,"Current draft source protects replay blob");
        let paths:Vec<String>=sqlx::query_scalar("SELECT content_path FROM documents WHERE id=$1 AND space_id=$2").bind(document_id).bind(space_id).fetch_all(&mut *tx).await?;
        for path in paths {anyhow::ensure!(!contains_scoped(&self.0.root,&path,url.as_bytes())?,"Current canonical source references replay blob");}
        // Every immutable committed canonical result in the original page remains protected.
        let historical:Vec<String>=sqlx::query_scalar("SELECT content_path FROM canonical_revision_receipts WHERE document_id=$1 AND space_id=$2").bind(document_id).bind(space_id).fetch_all(&mut *tx).await?;
        for path in historical {anyhow::ensure!(!contains_scoped(&self.0.root,&path,url.as_bytes())?,"Historical committed canonical source references replay blob");}
        Ok(())
    }
    async fn plan_replay(&self,tx:&mut sqlx::Transaction<'_,sqlx::Any>)->anyhow::Result<ReplayDisposal> {
        self.verify()?;let raw=self.replay_origin()?;let mut payload=None;let mut length=0;let mut sha=None;
        let origin=raw.as_deref().map(|r|self.verify_replay_origin(r)).transpose()?;
        if let Some((o,dir,name))=&origin {
            let f=optional_file(dir,name)?.ok_or_else(||fail("Replay blob missing before durable disposal intent"))?;let m=f.metadata()?;
            anyhow::ensure!(identity(&f)?==o.payload && m.is_file() && m.mode()&0o077==0 && m.nlink()==1 && m.len()<=o.expected_length,"Replay original blob substituted or foreign linked");
            payload=Some(identity(&f)?);length=m.len();sha=Some(super::hash(&f,length)?);
        }
        self.replay_protected_refs(tx,origin.as_ref().map(|(o,_,_)|o)).await?;
        let (manifest,proof,incomplete)=self.classify_replay_rewrite(tx).await?;
        Ok(ReplayDisposal {version:1,attempt_id:self.attempt_id().into(),claim_json:String::from_utf8(self.original_claim().to_vec())?,origin_json:raw.map(String::from_utf8).transpose()?,
            rewrite_plan_json:self.replay_plan()?.map(String::from_utf8).transpose()?,rewrite_manifest_json:manifest.map(String::from_utf8).transpose()?,rewrite_install_proof_json:proof.map(String::from_utf8).transpose()?,
            rewrite_incomplete_disposition_json:incomplete.map(String::from_utf8).transpose()?,payload,payload_length:length,payload_sha256:sha,request_sha256:self.producer_sha256().into(),admission_claim_sha256:self.claim_sha256(),maximum_bytes:self.maximum_bytes() as i64})
    }
    async fn complete_replay(&self,tx:&mut sqlx::Transaction<'_,sqlx::Any>,plan:&ReplayDisposal)->anyhow::Result<()> {self.complete_replay_fenced(tx,plan,&mut ||Ok(())).await}
    async fn complete_replay_fenced(&self,tx:&mut sqlx::Transaction<'_,sqlx::Any>,plan:&ReplayDisposal,before:&mut impl FnMut()->io::Result<()>)->anyhow::Result<()> {
        self.verify()?;
        anyhow::ensure!(plan.version==1 && plan.attempt_id==self.attempt_id() && plan.claim_json.as_bytes()==self.original_claim()
            && plan.request_sha256==self.producer_sha256() && plan.admission_claim_sha256==self.claim_sha256() && plan.maximum_bytes==self.maximum_bytes() as i64
            && self.replay_origin()?.as_deref()==plan.origin_json.as_ref().map(|s|s.as_bytes()) && self.replay_plan()?.as_deref()==plan.rewrite_plan_json.as_ref().map(|s|s.as_bytes()),"Replay disposal immutable capsule differs");
        let origin=plan.origin_json.as_deref().map(|r|self.verify_replay_origin(r.as_bytes())).transpose()?;
        self.replay_protected_refs(tx,origin.as_ref().map(|(o,_,_)|o)).await?;
        let (manifest,proof,incomplete)=self.classify_replay_rewrite(tx).await?;
        anyhow::ensure!(manifest.as_deref()==plan.rewrite_manifest_json.as_ref().map(|s|s.as_bytes()) && proof.as_deref()==plan.rewrite_install_proof_json.as_ref().map(|s|s.as_bytes()) && incomplete.as_deref()==plan.rewrite_incomplete_disposition_json.as_ref().map(|s|s.as_bytes()),"Replay protected original manifest evidence changed");
        if let Some((o,dir,name))=origin {
            if let Some(f)=optional_file(&dir,&name)? {
                let m=f.metadata()?;
                anyhow::ensure!(Some(identity(&f)?)==plan.payload && identity(&f)?==o.payload && m.nlink()==1 && m.len()==plan.payload_length
                    && Some(super::hash(&f,m.len())?)==plan.payload_sha256,"Replay disposal payload native identity or bytes differ");
                before()?;unlink_owned(&dir,&name,&f)?;disposal_checkpoint("replay-payload-unlinked",&self.0.root_path);
            }
        } else {anyhow::ensure!(plan.payload.is_none(),"Replay payload lacks original native binding");}
        self.verify()?;Ok(())
    }
}
/// Replay-only dispatch. Original uncommitted canonical manifests and proof groups remain intact;
/// their explicit aborted dependency is recorded, never promoted to a serving/commit authority.
pub(crate) async fn dispose_replay(pool:&sqlx::AnyPool,root:&Path,attempt:&str)->anyhow::Result<String> {
    let held=WriterLease::open_idle(root,attempt)?;
    let Producer::Replay {space_id,document_id,..}=held.producer() else {anyhow::bail!("Direct producer requires direct disposal adapter")};
    let mut tx=pool.begin().await?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx,&[space_id.as_str()]).await.map_err(|e|anyhow::anyhow!("Replay disposal original scope guard: {e:?}"))?;
    sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2").bind(document_id).bind(space_id).execute(&mut *tx).await?;
    sqlx::query("UPDATE document_drafts SET id=id WHERE document_id=$1 AND space_id=$2").bind(document_id).bind(space_id).execute(&mut *tx).await?;
    let guard=sqlx::query("UPDATE attachment_capacity_lock SET revision=revision+1 WHERE id=1 AND revision<9223372036854775807").execute(&mut *tx).await?.rows_affected();anyhow::ensure!(guard==1,"Replay capacity guard missing or exhausted");
    let row:(String,String,i64,String,Option<String>,Option<String>)=sqlx::query_as("SELECT request_sha256,admission_claim_sha256,maximum_bytes,state,disposition_json,disposition_sha256 FROM attachment_capacity_reservations WHERE operation_id=$1").bind(attempt).fetch_one(&mut *tx).await?;
    anyhow::ensure!(row.0==held.producer_sha256()&&row.1==held.claim_sha256()&&row.2==held.maximum_bytes() as i64,"Replay SQL reservation original capsule differs");
    anyhow::ensure!(row.3=="reserved"||row.3=="disposed","Committed replay attempt cannot be disposed");
    let raw=if let Some(raw)=optional_bytes(&held.0.directory,"replay-disposal-intent.json")? {raw}else {let plan=held.plan_replay(&mut tx).await?;let raw=json(&plan)?;stage::write_envelope(&held.0.directory,"replay-disposal-intent.json",&raw)?;raw};
    disposal_checkpoint("replay-intent-durable",root);let plan:ReplayDisposal=decode(&raw)?;
    if row.3=="disposed" {let tombstone=optional_bytes(&held.0.directory,"replay-disposal-tombstone.json")?.ok_or_else(||fail("Replay disposed SQL lacks original tombstone"))?;
        anyhow::ensure!(tombstone==raw&&row.4.as_deref()==Some(std::str::from_utf8(&raw)?)&&row.5.as_deref()==Some(digest(&raw).as_str()),"Replay disposed SQL evidence differs");
        held.complete_replay(&mut tx,&plan).await?;tx.rollback().await?;return Ok(digest(&raw));}
    held.complete_replay(&mut tx,&plan).await?;
    if let Some(old)=optional_bytes(&held.0.directory,"replay-disposal-tombstone.json")? {anyhow::ensure!(old==raw,"Replay original tombstone differs");}
    else {stage::write_envelope(&held.0.directory,"replay-disposal-tombstone.json",&raw)?;}
    held.verify()?;disposal_checkpoint("replay-tombstone-durable",root);
    let changed=sqlx::query("UPDATE attachment_capacity_reservations SET state='disposed',disposition_json=$1,disposition_sha256=$2 WHERE operation_id=$3 AND request_sha256=$4 AND admission_claim_sha256=$5 AND maximum_bytes=$6 AND state='reserved'")
        .bind(std::str::from_utf8(&raw)?).bind(digest(&raw)).bind(attempt).bind(held.producer_sha256()).bind(held.claim_sha256()).bind(held.maximum_bytes() as i64).execute(&mut *tx).await?.rows_affected();
    anyhow::ensure!(changed==1,"Replay capacity reservation changed");disposal_checkpoint("replay-sql-cas-uncommitted",root);tx.commit().await?;Ok(digest(&raw))
}

impl WriterLease {
    /// Read-only original proof classification. This emits no install/disposal authority.
    pub(crate) async fn capture_original_scope(&self,tx:&mut sqlx::Transaction<'_,sqlx::Any>)->anyhow::Result<crate::storage::restoration::admission::AdmissionOwnerScope> {
        use crate::storage::restoration::admission::{AdmissionOwnerScope,DirectoryBinding,DirectoryRole,FileBinding,FileRole,Identity as RoleIdentity,ReservationState};
        self.verify()?;
        let mut directories=Vec::new();let mut files=Vec::new();
        let role_id=|f:&File|->io::Result<RoleIdentity>{let id=identity(f)?;Ok(RoleIdentity{device:id.device,inode:id.inode})};
        let mut directory=|role:DirectoryRole,f:&File|->io::Result<()> {let id=role_id(f)?;directories.push(DirectoryBinding{role,original:id.clone(),captured:id.clone(),adopted:id});Ok(())};
        let mut file=|role:FileRole,f:&File|->io::Result<()> {let id=role_id(f)?;let m=f.metadata()?;let sha=super::hash(f,m.len())?;files.push(FileBinding{role,original:id.clone(),captured:id.clone(),adopted:id,length:m.len(),sha256:sha});Ok(())};
        directory(DirectoryRole::DataRoot,&self.0.root)?;directory(DirectoryRole::AdmissionNamespace,&self.0.namespace)?;directory(DirectoryRole::AdmissionCapsule,&self.0.directory)?;file(FileRole::Claim,&self.0.claim_file)?;
        let allowed=["claim.json","stage-origin.json","replay-blob-origin.json","replay-rewrite-plan.json","replay-rewrite-origin.json"];
        for entry in std::fs::read_dir(format!("/proc/self/fd/{}",self.0.directory.as_raw_fd()))? {
            let name=entry?.file_name().into_string().map_err(|_|fail("Non-UTF8 admission record"))?;
            anyhow::ensure!(allowed.contains(&name.as_str()),"Unclassified admission capture entry");
            if name!="claim.json" {let (held,_)=bytes(&self.0.directory,&name)?;file(FileRole::CapsuleRecord{name},&held)?;}
        }
        match self.producer() {
            Producer::Direct{..}=>{
                let plan=self.plan_direct()?;
                anyhow::ensure!(plan.stage_origin_json.is_some(),"Missing pre-payload direct capture origin");
                anyhow::ensure!(plan.installed_path.is_none(),"Installed direct capture requires certified alias roles");
                let (ns,dir)=self.stage_directory()?.ok_or_else(||fail("Captured direct stage missing"))?;
                directory(DirectoryRole::DirectStageNamespace,&ns)?;directory(DirectoryRole::DirectStageDirectory,&dir)?;
                for name in ["reservation.json","request.json","payload.part","seal.json","intent.json","install-proof.json"] {
                    if let Some(held)=optional_file(&dir,name)? {file(FileRole::DirectStage{name:name.into()},&held)?;}
                }
            }
            Producer::Replay{..}=>{
                let plan=self.plan_replay(tx).await?;
                let origin_raw=self.replay_origin()?.ok_or_else(||fail("Missing pre-payload replay capture origin"))?;
                let (origin,parent,name)=self.verify_replay_origin(&origin_raw)?;
                let (relative,_)=origin.path.rsplit_once('/').ok_or_else(||fail("Invalid captured replay path"))?;
                directory(DirectoryRole::ReplayBlobParent{relative:relative.into()},&parent)?;
                let blob=optional_file(&parent,&name)?.ok_or_else(||fail("Replay blob missing before captured disposal intent"))?;
                file(FileRole::ReplayBlob{relative:origin.path.clone()},&blob)?;
                if let Some(raw)=plan.rewrite_plan_json {
                    let rewrite:ReplayRewritePlan=decode(raw.as_bytes())?;
                    anyhow::ensure!(plan.rewrite_install_proof_json.is_none(),"Installed rewrite capture requires certified live result aliases");
                    let ns=stage::open(&self.0.root,".canonical-rewrite-journal",libc::O_RDONLY|libc::O_DIRECTORY)?;
                    let dir=stage::open(&ns,&rewrite.op_id,libc::O_RDONLY|libc::O_DIRECTORY)?;
                    directory(DirectoryRole::CanonicalRewriteNamespace,&ns)?;directory(DirectoryRole::CanonicalRewriteDirectory{operation_id:rewrite.op_id.clone()},&dir)?;
                    for name in ["manifest.json","payload"] {if let Some(held)=optional_file(&dir,name)? {file(FileRole::CanonicalRewrite{operation_id:rewrite.op_id.clone(),name:name.into()},&held)?;}}
                }
            }
        }
        self.verify()?;
        Ok(AdmissionOwnerScope{namespace:NS.into(),attempt_id:self.attempt_id().into(),original_claim_json:String::from_utf8(self.original_claim().to_vec())?,admission_claim_sha256:self.claim_sha256(),request_sha256:self.producer_sha256().into(),maximum_bytes:self.maximum_bytes(),reservation:ReservationState::Reserved,directories,files})
    }
}

impl WriterLease {
    pub(crate) fn verify_captured_scope(&self,scope:&crate::storage::restoration::admission::AdmissionOwnerScope)->io::Result<()> {
        self.verify()?;
        if scope.attempt_id!=self.attempt_id()||scope.original_claim_json.as_bytes()!=self.original_claim() {return Err(fail("Captured source role claim differs"));}
        for binding in &scope.directories {
            let current=if let Some(path)=binding.role.relative(self.attempt_id())? {
                let mut fd=self.0.root.try_clone()?;for name in path.split('/') {fd=stage::open(&fd,name,libc::O_RDONLY|libc::O_DIRECTORY)?;}fd
            }else{self.0.root.try_clone()?};
            let m=current.metadata()?;
            if (m.dev(),m.ino())!=(binding.captured.device,binding.captured.inode)||m.mode()&0o077!=0 {return Err(fail("Captured source admission directory changed"));}
        }
        for binding in &scope.files {
            let current=scoped_file(&self.0.root,&binding.role.relative(self.attempt_id())?)?;let m=current.metadata()?;
            if (m.dev(),m.ino())!=(binding.captured.device,binding.captured.inode)||m.mode()&0o077!=0||m.len()!=binding.length||super::hash(&current,m.len())?!=binding.sha256 {return Err(fail("Captured source admission file changed"));}
        }self.verify()
    }
}

/// Cold proof parsing uses the exact original typed record parsers, never fresh inode rebinding.
/// This first cold adapter accepts only the original partial direct stage, before seal/install.
pub(crate) fn validate_captured_partial_direct(scope:&crate::storage::restoration::admission::AdmissionOwnerScope,raws:&std::collections::BTreeMap<crate::storage::restoration::admission::FileRole,Vec<u8>>)->io::Result<()> {
 use crate::storage::restoration::admission::{DirectoryRole as D,FileRole as F};
 let claim:Claim=decode(scope.original_claim_json.as_bytes())?;
 let Producer::Direct{request}=claim.producer else{return Err(fail("Cold replay needs exact captured protected-alias adapter"));};
 let required=[F::Claim,F::CapsuleRecord{name:"stage-origin.json".into()},F::DirectStage{name:"reservation.json".into()},F::DirectStage{name:"request.json".into()},F::DirectStage{name:"payload.part".into()}];
 if raws.len()!=required.len()||!required.iter().all(|r|raws.contains_key(r)) {return Err(fail("Cold admission requires original partial direct stage only"));}
 let required_dirs=[D::DataRoot,D::AdmissionNamespace,D::AdmissionCapsule,D::DirectStageNamespace,D::DirectStageDirectory];
 if scope.directories.len()!=required_dirs.len()||!required_dirs.iter().all(|r|scope.directories.iter().any(|b|&b.role==r)) {return Err(fail("Cold partial stage directory inventory differs"));}
 if raws[&F::Claim]!=scope.original_claim_json.as_bytes() {return Err(fail("Cold original claim raw bytes differ"));}
 let origin:StageOrigin=decode(&raws[&F::CapsuleRecord{name:"stage-origin.json".into()}])?;
 let directory=|role:D|->io::Result<Identity>{let b=scope.directories.iter().find(|b|b.role==role).ok_or_else(||fail("Missing cold original directory"))?;Ok(Identity{device:b.original.device,inode:b.original.inode})};
 let payload=scope.files.iter().find(|b|b.role==(F::DirectStage{name:"payload.part".into()})).ok_or_else(||fail("Missing cold original payload"))?;
 let rr=&raws[&F::DirectStage{name:"reservation.json".into()}];let rq=&raws[&F::DirectStage{name:"request.json".into()}];
 let reservation:stage::Reservation=decode(rr)?;
 if origin.version!=1||origin.claim_sha256!=scope.admission_claim_sha256
  ||origin.namespace!=directory(D::DirectStageNamespace)?||origin.directory!=directory(D::DirectStageDirectory)?
  ||origin.payload!=(Identity{device:payload.original.device,inode:payload.original.inode})
  ||origin.reservation_sha256!=digest(rr)||origin.request_sha256!=digest(rq)||*rq!=json(&request)?
  ||reservation.version!=1||reservation.operation_id!=scope.attempt_id||reservation.maximum_bytes!=scope.maximum_bytes
  ||reservation.actor_id!=request.actor_id||reservation.space_id!=request.space_id||reservation.document_id!=request.document_id
  ||reservation.original_name!=request.original_name||reservation.mime_type!=request.mime_type
  ||payload.length>scope.maximum_bytes {return Err(fail("Cold original stage origin/request/reservation/payload custody differs"));}
 Ok(())
}

impl WriterLease {
 /// Syntax and original claim association for records preserved without runtime grants.
 /// Unsupported terminal records are never interpreted as disposal permission.
 pub(crate) fn verify_capture_record_shapes(&self)->io::Result<()> {
  self.verify()?;
  for entry in std::fs::read_dir(format!("/proc/self/fd/{}",self.0.directory.as_raw_fd()))? {
   let name=entry?.file_name().into_string().map_err(|_|fail("NonUTF8 captured capsule record"))?;
   if name=="claim.json"{continue}
   let (_,raw)=bytes(&self.0.directory,&name)?;
   let matches=match name.as_str(){
    "stage-origin.json"=>{let r:StageOrigin=decode(&raw)?;r.version==1&&r.claim_sha256==self.claim_sha256()},
    "replay-blob-origin.json"=>{let r:ReplayBlobOrigin=decode(&raw)?;r.version==1&&r.claim_sha256==self.claim_sha256()},
    "replay-rewrite-plan.json"=>{let r:ReplayRewritePlan=decode(&raw)?;r.version==1&&r.claim_sha256==self.claim_sha256()},
    "replay-rewrite-origin.json"=>{let r:ReplayRewriteOrigin=decode(&raw)?;r.version==1&&r.claim_sha256==self.claim_sha256()},
    "disposal-intent.json"|"disposal-tombstone.json"=>{let r:Disposal=decode(&raw)?;r.version==1&&r.attempt_id==self.attempt_id()&&r.claim_json.as_bytes()==self.original_claim()&&r.admission_claim_sha256==self.claim_sha256()&&r.request_sha256==self.producer_sha256()&&r.maximum_bytes==self.maximum_bytes() as i64},
    "replay-disposal-intent.json"|"replay-disposal-tombstone.json"=>{let r:ReplayDisposal=decode(&raw)?;r.version==1&&r.attempt_id==self.attempt_id()&&r.claim_json.as_bytes()==self.original_claim()&&r.admission_claim_sha256==self.claim_sha256()&&r.request_sha256==self.producer_sha256()&&r.maximum_bytes==self.maximum_bytes() as i64},
    _=>return Err(fail("Unknown captured capsule record")),
   };if !matches{return Err(fail("Captured capsule record original claim differs"));}
  }self.verify()
 }
 /// Explicit omission only after full native proof plan validates. Invalid proof is an error.
 pub(crate) async fn capture_supported_scope(&self,tx:&mut sqlx::Transaction<'_,sqlx::Any>)->anyhow::Result<Option<crate::storage::restoration::admission::AdmissionOwnerScope>> {
  self.verify_capture_record_shapes()?;
  let supported=match self.producer(){
   Producer::Direct{..}=>{let plan=self.plan_direct()?;plan.stage_origin_json.is_some()&&plan.installed_path.is_none()&&!plan.envelopes.keys().any(|n|["seal.json","intent.json","install-proof.json"].contains(&n.as_str()))},
   Producer::Replay{..}=>{let plan=self.plan_replay(tx).await?;
    plan.origin_json.is_some()&&plan.payload_length<=64*1024*1024&&plan.rewrite_install_proof_json.is_none()
      &&(plan.rewrite_plan_json.is_none()||plan.rewrite_incomplete_disposition_json.is_some())},
  };
  if supported {
   let scope=self.capture_original_scope(tx).await?;
   if matches!(self.producer(),Producer::Replay{..}) {
    // Emit only a source scope that the cold typed record consumer accepts.
    // Keep Replay/record allocation limits separate from streamed Direct payloads.
    use std::os::unix::fs::FileExt;let mut raws=std::collections::BTreeMap::new();
    for b in &scope.files {anyhow::ensure!(b.length<=64*1024*1024,"Replay captured proof exceeds consumer bound");let path=b.role.relative(&scope.attempt_id)?;let file=scoped_file(&self.0.root,&path)?;let md=file.metadata()?;anyhow::ensure!(md.len()==b.length&&identity(&file)?==(Identity{device:b.original.device,inode:b.original.inode}),"Replay captured source file changed");let mut raw=vec![0;b.length as usize];file.read_exact_at(&mut raw,0)?;anyhow::ensure!(digest(&raw)==b.sha256,"Replay captured source bytes changed");raws.insert(b.role.clone(),raw);}
    validate_captured_replay_custody(&scope,&raws)?;self.verify_captured_scope(&scope)?;
   }
   Ok(Some(scope))
  }else{self.verify()?;Ok(None)}
 }
}

/// Exact source capsule inventory, native file handles and bytes held even for omitted roles.
pub(crate) struct CapturedCapsuleRecords {lease:WriterLease,records:std::collections::BTreeMap<String,(File,Identity,u64,String)>}
impl WriterLease {
 pub(crate) fn capture_record_fence(&self)->io::Result<CapturedCapsuleRecords> {
  self.verify_capture_record_shapes()?;let mut records=std::collections::BTreeMap::new();
  for e in std::fs::read_dir(format!("/proc/self/fd/{}",self.0.directory.as_raw_fd()))? {let name=e?.file_name().into_string().map_err(|_|fail("NonUTF8 capsule record"))?;let (held,raw)=bytes(&self.0.directory,&name)?;let id=identity(&held)?;records.insert(name,(held,id,raw.len() as u64,digest(&raw)));}
  let fence=CapturedCapsuleRecords{lease:self.clone(),records};fence.verify()?;Ok(fence)
 }
}
impl CapturedCapsuleRecords {
 pub(crate) fn verify(&self)->io::Result<()> {
  self.lease.verify_capture_record_shapes()?;
  let names=std::fs::read_dir(format!("/proc/self/fd/{}",self.lease.0.directory.as_raw_fd()))?.map(|e|e?.file_name().into_string().map_err(|_|fail("NonUTF8 capsule record"))).collect::<io::Result<std::collections::BTreeSet<_>>>()?;
  if names!=self.records.keys().cloned().collect(){return Err(fail("Held capture capsule inventory changed"));}
  for (name,(held,id,len,sha)) in &self.records {let (fresh,raw)=bytes(&self.lease.0.directory,name)?;if identity(held)?!=*id||identity(&fresh)?!=*id||held.metadata()?.len()!=*len||super::hash(held,*len)?!=*sha||raw.len() as u64!=*len||digest(&raw)!=*sha {return Err(fail("Held capture capsule inode or raw bytes changed"));}}
  self.lease.verify()
 }
}

include!("attachment_upload_admission_cold_replay.rs");

// Shape/claim authentication for preservation-only capture; never interprets native IDs as current ownership.
pub(crate) fn validate_preserved_capsule_record(name:&str,raw:&[u8],claim_raw:&[u8])->io::Result<()> {
 let claim:Claim=decode(claim_raw)?;validate(&claim.producer,claim.maximum_bytes)?;
 let claim_hash=digest(claim_raw);let producer_hash=digest(&json(&claim.producer)?);
 if claim.version!=1||claim.producer_sha256!=producer_hash{return Err(fail("Malformed preserved original producer"));}
 let valid=match name {
  "claim.json"=>raw==claim_raw,
  "stage-origin.json"=>{let r:StageOrigin=decode(raw)?;r.version==1&&r.claim_sha256==claim_hash},
  "replay-blob-origin.json"=>{let r:ReplayBlobOrigin=decode(raw)?;r.version==1&&r.claim_sha256==claim_hash},
  "replay-rewrite-plan.json"=>{let r:ReplayRewritePlan=decode(raw)?;r.version==1&&r.claim_sha256==claim_hash},
  "replay-rewrite-origin.json"=>{let r:ReplayRewriteOrigin=decode(raw)?;r.version==1&&r.claim_sha256==claim_hash},
  "disposal-intent.json"|"disposal-tombstone.json"=>{let r:Disposal=decode(raw)?;r.version==1&&r.attempt_id==claim.attempt_id&&r.claim_json.as_bytes()==claim_raw&&r.admission_claim_sha256==claim_hash&&r.request_sha256==producer_hash&&r.maximum_bytes==claim.maximum_bytes as i64},
  "replay-disposal-intent.json"|"replay-disposal-tombstone.json"=>{let r:ReplayDisposal=decode(raw)?;r.version==1&&r.attempt_id==claim.attempt_id&&r.claim_json.as_bytes()==claim_raw&&r.admission_claim_sha256==claim_hash&&r.request_sha256==producer_hash&&r.maximum_bytes==claim.maximum_bytes as i64},
  _=>false,
 };if !valid{return Err(fail("Preserved capsule record differs from exact original claim"));}Ok(())
}

/// Full-byte Direct payload proof under an already authenticated held role FD; no identity adoption.
pub(crate) fn verify_captured_direct_payload(file:&File,length:u64,sha256:&str,maximum:u64)->io::Result<()> {
 let before=file.metadata()?;
 if !before.is_file()||before.len()!=length||length>maximum||before.nlink()!=1||before.mode()&0o777!=0o600||before.uid()!=unsafe{libc::geteuid()}{return Err(fail("Captured Direct payload bound/private exclusive topology differs"));}
 let stamp=|m:&std::fs::Metadata|(m.dev(),m.ino(),m.len(),m.nlink(),m.mode(),m.mtime(),m.mtime_nsec(),m.ctime(),m.ctime_nsec());
 if super::hash(file,length)?!=sha256||stamp(&file.metadata()?)!=stamp(&before){return Err(fail("Captured Direct payload full length/hash or held inode changed"));}Ok(())
}

#[path="attachment_committed_erasure.rs"]
pub(crate) mod committed_erasure;

// Read-only terminal observation under an original native idle lease. This is not
// a copied-capsule constructor, release primitive, or serving permission.
impl WriterLease {
 async fn observe_native_disposed_in(&self,conn:&mut sqlx::AnyConnection,raw:&str,sha:&str)->io::Result<()> {
  self.verify()?;self.verify_capture_record_shapes()?;if digest(raw.as_bytes())!=sha{return Err(fail("Native disposed SQL digest differs"));}
  let(intent,tombstone)=match self.producer(){Producer::Direct{..}=>("disposal-intent.json","disposal-tombstone.json"),Producer::Replay{..}=>("replay-disposal-intent.json","replay-disposal-tombstone.json")};
  if optional_bytes(&self.0.directory,intent)?.as_deref()!=Some(raw.as_bytes())||optional_bytes(&self.0.directory,tombstone)?.as_deref()!=Some(raw.as_bytes()){return Err(fail("Native disposed original intent/tombstone differs from exact SQL"));}
  for query in ["SELECT COUNT(*) FROM attachment_upload_receipts WHERE attempt_id=$1","SELECT COUNT(*) FROM attachment_version_records WHERE upload_attempt_id=$1"]{let count:i64=sqlx::query_scalar(query).bind(self.attempt_id()).fetch_one(&mut *conn).await.map_err(io::Error::other)?;if count!=0{return Err(fail("Native disposed scope already has winning receipt"));}}
  match self.producer(){
   Producer::Direct{..}=>{
    let plan:Disposal=decode(raw.as_bytes())?;if self.stage_origin()?.as_deref()!=plan.stage_origin_json.as_ref().map(|s|s.as_bytes()){return Err(fail("Disposed Direct original stage origin differs"));}
    if let Some((ns,dir))=self.stage_directory()?{
     let origin=self.verify_origin(&ns,&dir,plan.stage_origin_json.as_ref().ok_or_else(||fail("Disposed Direct original stage origin absent"))?.as_bytes())?;
     if plan.payload!=Some(origin.payload){return Err(fail("Disposed Direct original payload identity differs"));}
     for name in ["reservation.json","request.json","seal.json","intent.json","install-proof.json"]{if optional_bytes(&dir,name)?.as_deref()!=plan.envelopes.get(name).map(|s|s.as_bytes()){return Err(fail("Disposed Direct immutable stage evidence differs"));}}
     if optional_file(&dir,"payload.part")?.is_some(){return Err(fail("Disposed Direct payload resurrected"));}
    }else if plan.stage_origin_json.is_some()||plan.payload.is_some(){return Err(fail("Disposed Direct original stage disappeared"));}
    if let Some(path)=&plan.installed_path{let(dir,name)=super::target(&self.0.root,path,false)?;if optional_file(&dir,&name)?.is_some(){return Err(fail("Disposed Direct canonical payload resurrected"));}}
   },
   Producer::Replay{..}=>{
    let plan:ReplayDisposal=decode(raw.as_bytes())?;if self.replay_origin()?.as_deref()!=plan.origin_json.as_ref().map(|s|s.as_bytes())||self.replay_plan()?.as_deref()!=plan.rewrite_plan_json.as_ref().map(|s|s.as_bytes()){return Err(fail("Disposed Replay original blob/rewrite plan differs"));}
    let origin=plan.origin_json.as_deref().map(|raw|self.verify_replay_origin(raw.as_bytes())).transpose()?;
    self.replay_protected_refs(conn,origin.as_ref().map(|(o,_,_)|o)).await.map_err(io::Error::other)?;
    let(manifest,proof,incomplete)=self.classify_replay_rewrite(conn).await.map_err(io::Error::other)?;
    if manifest.as_deref()!=plan.rewrite_manifest_json.as_ref().map(|s|s.as_bytes())||proof.as_deref()!=plan.rewrite_install_proof_json.as_ref().map(|s|s.as_bytes())||incomplete.as_deref()!=plan.rewrite_incomplete_disposition_json.as_ref().map(|s|s.as_bytes()){return Err(fail("Disposed Replay protected immutable rewrite evidence differs"));}
    if let Some((o,dir,name))=origin{if plan.payload!=Some(o.payload)||optional_file(&dir,&name)?.is_some(){return Err(fail("Disposed Replay payload identity differs or resurrected"));}}else if plan.payload.is_some(){return Err(fail("Disposed Replay original payload lacks native origin"));}
   }
  }self.verify()
 }
}

// Pure composite record validation; no native authority is created.
include!("attachment_composite_records.rs");
