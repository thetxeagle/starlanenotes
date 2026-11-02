//! Application-owned restoration custody. Original journal evidence is never rewritten.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{AnyPool, Row};
use std::{fs::File, io, path::Path};

#[path = "restoration_admission.rs"]
pub(crate) mod admission;
#[path="restoration_completed_absence.rs"]
pub(crate) mod completed;
// Wire-only draft: no Ledger4 or manifest parser admission is enabled here.
#[path="restoration_composite_types.rs"]
pub(crate) mod composite;
#[path="restoration_historical_types.rs"]
pub(crate) mod historical;

const MAX_LEDGER: u64 = 16 * 1024 * 1024;
const NAMESPACE: &str = ".restoration-custody";
const LEDGER_NAME: &str = "current.json";
fn fail(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ledger {
    pub version: u32,
    pub generation_id: String,
    pub backup_id: String,
    pub manifest_sha256: String,
    pub database_sha256: String,
    pub root_device: u64,
    pub root_inode: u64,
    pub scopes: Vec<Scope>,
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "admission_nonnull")]
    pub admission: Option<admission::AdmissionOwnerEnvelope>,
    #[serde(default,skip_serializing_if="Option::is_none",deserialize_with="completed_owner_nonnull")]
    pub completed_absence:Option<completed::CompletedOwnerEnvelope>,
    #[serde(default,skip_serializing_if="Option::is_none",deserialize_with="composite_owner_nonnull")]
    pub composite:Option<composite::CompositeOwnerEnvelope>,
    #[serde(default,skip_serializing_if="Option::is_none",deserialize_with="historical_owner_nonnull")]
    pub historical_preservation:Option<historical::HistoricalOwnerEnvelope>,
}
fn historical_owner_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<historical::HistoricalOwnerEnvelope>,D::Error>{historical::HistoricalOwnerEnvelope::deserialize(d).map(Some)}
fn composite_owner_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<composite::CompositeOwnerEnvelope>,D::Error>{composite::CompositeOwnerEnvelope::deserialize(d).map(Some)}
fn completed_owner_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<completed::CompletedOwnerEnvelope>,D::Error>{completed::CompletedOwnerEnvelope::deserialize(d).map(Some)}
fn admission_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<admission::AdmissionOwnerEnvelope>,D::Error> {
    admission::AdmissionOwnerEnvelope::deserialize(d).map(Some)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Scope {
    pub namespace: String,
    pub op_id: String,
    pub evidence_sha256: String,
    pub bindings: Vec<Binding>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Binding {
    pub relative: String,
    pub original_device: u64,
    pub original_inode: u64,
    pub adopted_device: u64,
    pub adopted_inode: u64,
    pub length: u64,
    pub sha256: String,
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn valid_id(s: &str) -> bool {
    uuid::Uuid::parse_str(s).is_ok() && uuid::Uuid::parse_str(s).unwrap().to_string() == s
}
fn valid_relative(s: &str) -> bool {
    !s.is_empty()
        && !s.contains('\0')
        && s.split('/').all(|p| !p.is_empty() && p != "." && p != "..")
        && !Path::new(s).is_absolute()
}
fn validate(ledger: &Ledger) -> io::Result<()> {
    use std::collections::{HashMap, HashSet};
    if !matches!(ledger.version,1|2|3|4|5)
        || !valid_id(&ledger.generation_id)
        || !valid_id(&ledger.backup_id)
        || !valid_hash(&ledger.manifest_sha256)
        || !valid_hash(&ledger.database_sha256)
    {
        return Err(fail("Invalid restoration ledger header"));
    }
    let root=admission::Identity{device:ledger.root_device,inode:ledger.root_inode};
    if let Some(roles)=&ledger.historical_preservation{
        if ledger.version!=5||ledger.admission.is_some()||ledger.completed_absence.is_some()||ledger.composite.is_some(){return Err(fail("Historical Ledger5 owner must be exclusive"));}
        roles.validate(&root)?;
    }else if ledger.version==5{return Err(fail("Historical Ledger5 physical owner absent"));}
    match (ledger.version,&ledger.admission,&ledger.completed_absence,&ledger.composite) {
        (1,None,None,None)=>{},
        (2,Some(a),None,None)=>a.validate(&root)?,
        (3,a,Some(c),None)=>{if let Some(a)=a{a.validate(&root)?;}c.validate(&root)?;},
        (4,None,None,Some(c))=>c.validate(&root)?,
        (5,None,None,None) if ledger.historical_preservation.is_some()=>{},
        _=>return Err(fail("Ledger version and typed owners differ")),
    }
    let mut scopes = HashSet::new();
    for scope in &ledger.scopes {
        if ![
            ".storage-journal",
            ".publication-journal",
            ".canonical-rewrite-journal",
            ".move-relocation-journal",
            ".attachment-upload-journal",
        ]
        .contains(&scope.namespace.as_str())
            || !valid_id(&scope.op_id)
            || !valid_hash(&scope.evidence_sha256)
            || !scopes.insert((&scope.namespace, &scope.op_id))
        {
            return Err(fail("Invalid or duplicate restoration scope"));
        }
        let mut paths = HashSet::new();
        let mut forwards = HashMap::new();
        let mut backwards = HashMap::new();
        for binding in &scope.bindings {
            if !valid_relative(&binding.relative)
                || !valid_hash(&binding.sha256)
                || binding.length > i64::MAX as u64
                || !paths.insert(&binding.relative)
            {
                return Err(fail("Invalid or duplicate restoration binding"));
            }
            let original = (binding.original_device, binding.original_inode);
            let adopted = (binding.adopted_device, binding.adopted_inode);
            let content = (binding.length, binding.sha256.as_str());
            if forwards
                .insert(original, (adopted, content))
                .is_some_and(|old| old != (adopted, content))
                || backwards
                    .insert(adopted, (original, content))
                    .is_some_and(|old| old != (original, content))
            {
                return Err(fail("Restoration hardlink relation changed"));
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
pub(crate) mod linux {
    use super::*;
    use std::{
        collections::HashMap,
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::{FileExt, MetadataExt},
        },
        path::{Component, PathBuf},
        sync::{Arc, Mutex, OnceLock},
    };
    include!("restoration_admission_owner.rs");
    include!("restoration_completed_owner.rs");
    include!("restoration_composite_bundle.rs");
    include!("restoration_composite_finalized.rs");
    include!("restoration_historical_inventory.rs");
    include!("restoration_historical_completed.rs");
    include!("restoration_historical_bundle.rs");
    include!("restoration_historical_usage.rs");
    include!("restoration_historical_readonly.rs");
    include!("restoration_historical_readonly_vfs.rs");
    include!("restoration_historical_readonly_data.rs");
    include!("restoration_historical_readonly_search.rs");
    include!("restoration_historical_http_usage.rs");
    include!("restoration_historical_readonly_worker.rs");
    include!("restoration_historical_readonly_operator.rs");
    #[cfg(test)] pub(crate) mod historical_readonly_child_tests {include!("restoration_historical_readonly_child_tests.rs");}
    #[cfg(test)] pub(crate) mod historical_normal_operator_tests {include!("restoration_historical_normal_operator_tests.rs");}
    #[cfg(test)] pub(crate) mod historical_readonly_tests {include!("restoration_historical_readonly_tests.rs");}
    pub(crate) use crate::storage::attachment_upload::admission::committed_erasure::accounting::composite::historical::HistoricalCompositeSource;
    include!("restoration_completed_prior_capture.rs");
    include!("restoration_qualified_erasure.rs");
    type Identity = (u64, u64);
    fn identity(file: &File) -> io::Result<Identity> {
        let m = file.metadata()?;
        Ok((m.dev(), m.ino()))
    }
    fn open_at(parent: &File, name: &str, flags: i32) -> io::Result<File> {
        let c = CString::new(name).map_err(|_| fail("NUL in rooted path"))?;
        // SAFETY: live directory descriptor and terminated name; returned descriptor has one owner.
        let raw = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                c.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if raw < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(unsafe { File::from_raw_fd(raw) })
        }
    }
    fn exists(parent: &File, name: &str) -> io::Result<bool> {
        let c = CString::new(name).map_err(|_| fail("Invalid marker name"))?;
        let mut st = std::mem::MaybeUninit::<libc::stat>::uninit();
        // SAFETY: valid descriptor/name and output buffer. No-follow detects links as present.
        let rc = unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                c.as_ptr(),
                st.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if rc == 0 {
            Ok(true)
        } else {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::NotFound {
                Ok(false)
            } else {
                Err(e)
            }
        }
    }
    fn markers(root: &File) -> io::Result<()> {
        for marker in [
            ".starlane-restore-incomplete",
            ".starlane-backup-incomplete",
            ".engine-transfer-custody",
            ".pg-restoration-custody",
        ] {
            if exists(root, marker)? {
                return Err(fail("Incomplete storage operation"));
            }
        }
        Ok(())
    }
    struct Root {
        absolute: PathBuf,
        chain: Vec<File>,
    }
    impl Root {
        fn open(path: &Path) -> io::Result<Self> {
            let absolute = if path.is_absolute() {
                path.to_path_buf()
            } else {
                std::env::current_dir()?.join(path)
            };
            if absolute
                .components()
                .any(|c| !matches!(c, Component::RootDir | Component::Normal(_)))
            {
                return Err(fail("Non canonical storage root"));
            }
            let mut chain = vec![File::open("/")?];
            for component in absolute.components() {
                match component {
                    Component::RootDir => (),
                    Component::Normal(n) => {
                        let n = n.to_str().ok_or_else(|| fail("Non UTF8 storage root"))?;
                        chain.push(open_at(
                            chain.last().unwrap(),
                            n,
                            libc::O_RDONLY | libc::O_DIRECTORY,
                        )?);
                    }
                    _ => return Err(fail("Non canonical storage root")),
                }
            }
            Ok(Self { absolute, chain })
        }
        fn fd(&self) -> &File {
            self.chain.last().unwrap()
        }
        fn check(&self) -> io::Result<()> {
            let current = Self::open(&self.absolute)?;
            if current.chain.len() != self.chain.len() {
                return Err(fail("Storage root ancestry changed"));
            }
            for (a, b) in current.chain.iter().zip(&self.chain) {
                if identity(a)? != identity(b)? {
                    return Err(fail("Storage root ancestry changed"));
                }
            }
            markers(self.fd())
        }
        fn directory(&self,relative:&str)->io::Result<File> {
            if !valid_relative(relative)||relative.contains('\\') {return Err(fail("Unsafe restoration directory path"));}
            let mut parent=self.fd().try_clone()?;
            for part in relative.split('/') {parent=open_at(&parent,part,libc::O_RDONLY|libc::O_DIRECTORY)?;}
            if parent.metadata()?.mode()&0o777!=0o700 {return Err(fail("Restoration admission directory not private"));} Ok(parent)
        }
        fn leaf(&self, relative: &str) -> io::Result<File> {
            if !valid_relative(relative) {
                return Err(fail("Unsafe restoration path"));
            }
            let mut parent = self.fd().try_clone()?;
            let parts: Vec<_> = relative.split('/').collect();
            for p in &parts[..parts.len() - 1] {
                parent = open_at(&parent, p, libc::O_RDONLY | libc::O_DIRECTORY)?;
            }
            let leaf = open_at(
                &parent,
                parts[parts.len() - 1],
                libc::O_RDONLY | libc::O_NONBLOCK,
            )?;
            if !leaf.metadata()?.is_file() {
                return Err(fail("Restoration leaf is not regular"));
            }
            Ok(leaf)
        }
    }
    fn hash_file(file: &File, expected: u64) -> io::Result<String> {
        let before = file.metadata()?;
        if !before.is_file() || before.len() != expected {
            return Err(fail("Restoration file size changed"));
        }
        let mut offset = 0;
        let mut buf = [0u8; 65536];
        let mut hash = Sha256::new();
        while offset < expected {
            let wanted = (expected - offset).min(buf.len() as u64) as usize;
            let n = file.read_at(&mut buf[..wanted], offset)?;
            if n == 0 {
                return Err(fail("Restoration file truncated"));
            }
            hash.update(&buf[..n]);
            offset += n as u64;
        }
        let after = file.metadata()?;
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
            return Err(fail("Restoration file changed during verification"));
        }
        Ok(hex::encode(hash.finalize()))
    }
    fn read_ledger(file: &File) -> io::Result<Vec<u8>> {
        let size = file.metadata()?.len();
        if size == 0 || size > MAX_LEDGER {
            return Err(fail("Restoration ledger size invalid"));
        }
        let before = hash_file(file, size)?;
        let mut bytes = vec![0; size as usize];
        let mut offset = 0;
        while offset < bytes.len() {
            let n = file.read_at(&mut bytes[offset..], offset as u64)?;
            if n == 0 {
                return Err(fail("Restoration ledger truncated"));
            }
            offset += n;
        }
        if digest(&bytes) != before || hash_file(file, size)? != before {
            return Err(fail("Restoration ledger changed"));
        }
        Ok(bytes)
    }
    enum EngineAuthority {Transfer(Arc<engine_transfer::EngineAdmission>),PgRestore(Arc<engine_transfer::RestoredPgAdmission>)}
    impl EngineAuthority {fn matches_root(&self,root:&Root)->io::Result<()> {match self{Self::Transfer(p)=>p.matches_root(root),Self::PgRestore(p)=>p.matches_root(root)}}}
    struct Active {
        engine:Option<Arc<EngineAuthority>>,
        root: Root,
        directory: File,
        file: File,
        ledger: Ledger,
        ledger_sha256: String,
        denied: Vec<DeniedReservation>,
        version2: bool,
        sqlite_main: Option<Arc<File>>,
        control_files: std::collections::BTreeMap<String, HistoryHeldRecord>,
        control_directories: std::collections::BTreeMap<String, (File, PhysicalStamp)>,
        control_topology: std::collections::BTreeSet<String>,
    }
    impl Active {
        fn load(root: Root) -> io::Result<Self> {Self::load_with_engine(root,None)}
        fn load_with_engine(root:Root,engine:Option<Arc<EngineAuthority>>)->io::Result<Self> {
            runtime_root_check(&root,engine.as_deref())?;
            let directory = open_at(root.fd(), NAMESPACE, libc::O_RDONLY | libc::O_DIRECTORY)?;
            let dm = directory.metadata()?;
            let file = open_at(&directory, LEDGER_NAME, libc::O_RDONLY | libc::O_NONBLOCK)?;
            let fm = file.metadata()?;
            if dm.mode() & 0o777 != 0o700
                || !fm.is_file()
                || fm.mode() & 0o777 != 0o600
                || fm.nlink() != 1
            {
                return Err(fail("Restoration ledger is not private"));
            }
            let source=if engine.is_some(){HistorySource::new_with_check(&root,None,engine_transfer::engine_root_fence)?}else{HistorySource::new(&root,None)?};
            let closure=history_closure(source,identity(root.fd())?,None)?;
            let current=&closure.generations[&closure.current_id];
            let ledger=current.ledger.clone();
            let ledger_sha256=digest(&current.raw);
            let version2=current.envelope.is_some();
            let denied=current.envelope.as_ref().map(|e|e.denied.clone()).unwrap_or_default();
            closure.source.fence()?;
            let control_topology=closure.source.files;
            let control_directories=closure.source.directories;
            let control_files=closure.source.records;
            let sqlite_main=if engine.is_none()&&exists(root.fd(),"starlane-notes.db")?{
                // Repeated activation reuses the original retained descriptor.
                // Replacing it would close a main FD in a live SQLite process.
                let old=registry().lock().map_err(|_|fail("Restoration registry unavailable"))?.get(&root.absolute).cloned();
                if let Some(old)=old{old.check()?;if old.engine.is_some()||identity(old.root.fd())?!=identity(root.fd())?{return Err(fail("Original SQLite activation root/engine changed"));}Some(old.sqlite_main.as_ref().ok_or_else(||fail("Original SQLite activation main absent"))?.clone())}else{Some(Arc::new(root.leaf("starlane-notes.db")?))}
            }else{None};
            let active=Self{engine,root,directory,file,ledger,ledger_sha256,denied,version2,control_topology,control_directories,control_files,sqlite_main};
            active.check()?;
            Ok(active)
        }
        fn check(&self) -> io::Result<()> {
            runtime_root_check(&self.root,self.engine.as_deref())?;
            if let Some(main)=&self.sqlite_main{if qualified_sqlite_path_identity(&self.root)?!=identity(main)?{return Err(fail("Active original SQLite main inode changed"));}}
            let directory = open_at(
                self.root.fd(),
                NAMESPACE,
                libc::O_RDONLY | libc::O_DIRECTORY,
            )?;
            if identity(&directory)? != identity(&self.directory)?
                || directory.metadata()?.mode() & 0o777 != 0o700
            {
                return Err(fail("Restoration custody directory changed"));
            }
            let file = open_at(&directory, LEDGER_NAME, libc::O_RDONLY | libc::O_NONBLOCK)?;
            let meta = file.metadata()?;
            if identity(&file)? != identity(&self.file)?
                || !meta.is_file()
                || meta.mode() & 0o777 != 0o600
                || meta.nlink() != 1
                || digest(&read_ledger(&file)?) != self.ledger_sha256
            {
                return Err(fail("Restoration custody ledger changed"));
            }
            let (files,dirs)=history_topology(&self.root)?;
            if files!=self.control_topology||dirs.keys().collect::<Vec<_>>()!=self.control_directories.keys().collect::<Vec<_>>() {return Err(fail("Active history topology changed"));}
            if self.version2 {
            for(path,(held,stamp))in &self.control_directories {
                if physical_stamp(held)?!=*stamp||physical_stamp(&dirs[path].0)?!=*stamp{return Err(fail("Active history directory changed"));}
            }
            for(path,record)in &self.control_files {
                let fresh=self.root.leaf(path)?;
                if physical_stamp(&record.held)?!=record.stamp||physical_stamp(&fresh)?!=record.stamp||read_ledger(&fresh)?!=record.raw||physical_stamp(&fresh)?!=record.stamp{return Err(fail("Active history evidence changed"));}
            }
            } else {
                // Legacy v1 fences exact raw bytes/identity as originally shipped;
                // metadata-only restoration to the same held object remains compatible.
                for(path,(held,stamp))in &self.control_directories {
                    if identity(held)?!=(stamp.device,stamp.inode)||identity(&dirs[path].0)?!=(stamp.device,stamp.inode){return Err(fail("Active v1 history directory changed"));}
                }
            }
            runtime_root_check(&self.root,self.engine.as_deref())
        }
    }
    fn runtime_root_check(root:&Root,engine:Option<&EngineAuthority>)->io::Result<()> {match engine {Some(proof)=>proof.matches_root(root),None=>root.check()}}
    fn registry() -> &'static Mutex<HashMap<PathBuf, Arc<Active>>> {
        static REGISTRY: OnceLock<Mutex<HashMap<PathBuf, Arc<Active>>>> = OnceLock::new();
        REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
    }
    fn registered(path: &Path) -> io::Result<(Root, Option<Arc<Active>>)> {
        let root = Root::open(path)?;
        let found = registry()
            .lock()
            .map_err(|_| fail("Restoration registry unavailable"))?
            .get(&root.absolute)
            .cloned();
        if let Some(active) = &found {
            active.check()?;
        } else if exists(root.fd(), NAMESPACE)? {
            return Err(fail("Unregistered restoration custody"));
        }
        runtime_root_check(&root,found.as_ref().and_then(|a|a.engine.as_deref()))?;
        Ok((root, found))
    }
    fn activate_engine_proof(proof:Arc<engine_transfer::EngineAdmission>)->io::Result<()> {
        let root=Root::open(proof.path())?;
        proof.matches_root(&root)?;
        let active=Arc::new(Active::load_with_engine(root,Some(Arc::new(EngineAuthority::Transfer(proof))))?);
        active.check()?;
        let mut map=registry().lock().map_err(|_|fail("Restoration registry unavailable"))?;
        if let Some(old)=map.get(&active.root.absolute) {
            old.check()?;
            if old.engine.is_none()||old.ledger_sha256!=active.ledger_sha256||identity(&old.file)?!=identity(&active.file)?{return Err(fail("Serving engine generation replacement refused"));}
        } else {map.insert(active.root.absolute.clone(),active);}
        Ok(())
    }
    fn activate_pg_restore_proof(proof:Arc<engine_transfer::RestoredPgAdmission>)->io::Result<()> {
        let root=Root::open(proof.path())?;
        proof.matches_root(&root)?;
        let active=Arc::new(Active::load_with_engine(root,Some(Arc::new(EngineAuthority::PgRestore(proof))))?);
        active.check()?;
        let mut map=registry().lock().map_err(|_|fail("Restoration registry unavailable"))?;
        if let Some(old)=map.get(&active.root.absolute) {
            old.check()?;
            if old.engine.is_none()||old.ledger_sha256!=active.ledger_sha256||identity(&old.file)?!=identity(&active.file)?{return Err(fail("Serving engine generation replacement refused"));}
        } else {map.insert(active.root.absolute.clone(),active);}
        Ok(())
    }
    pub(crate) fn bootstrap_gate(path: &Path) -> io::Result<()> {
        match Root::open(path) {
            Ok(root) => {root.check()?;history_bootstrap(&root)},
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
    pub(crate) fn fence(path: &Path) -> io::Result<()> {
        let (root, active) = registered(path)?;
        if let Some(active) = &active {active.check()?;}
        runtime_root_check(&root,active.as_ref().and_then(|a|a.engine.as_deref()))
    }
    /// Existing restored scopes cannot manufacture new immutable install proofs.
    /// Explicit planned pending installs require a separate classifier-owned protocol.
    pub(crate) fn fresh_install_allowed(
        path: &Path,
        namespace: &str,
        op_id: &str,
    ) -> io::Result<bool> {
        let (root, active) = registered(path)?;
        let allowed = active.as_ref().is_none_or(|active| {
            !active.denied.iter().any(|d|d.namespace==namespace&&d.op_id==op_id) && !active
                .ledger
                .scopes
                .iter()
                .any(|scope| scope.namespace == namespace && scope.op_id == op_id)
        });
        if let Some(active) = &active {
            active.check()?;
        }
        runtime_root_check(&root,active.as_ref().and_then(|a|a.engine.as_deref()))?;
        Ok(allowed)
    }
    pub(crate) fn owns(
        path: &Path,
        namespace: &str,
        op_id: &str,
        evidence: &[u8],
        relative: &str,
        original: Identity,
        held: &File,
    ) -> io::Result<bool> {
        let (root, active) = registered(path)?;
        if !valid_relative(relative) {
            return Err(fail("Unsafe restoration binding path"));
        }
        let observed = identity(held)?;
        let result = if let Some(active) = &active {
            if active.denied.iter().any(|d|d.namespace==namespace&&d.op_id==op_id) {false}
            else if let Some(scope) = active
                .ledger
                .scopes
                .iter()
                .find(|s| s.namespace == namespace && s.op_id == op_id)
            {
                if scope.evidence_sha256 != digest(evidence) {
                    false
                } else if let Some(binding) = scope.bindings.iter().find(|b| b.relative == relative)
                {
                    let leaf = root.leaf(relative)?;
                    observed == (binding.adopted_device, binding.adopted_inode)
                        && original == (binding.original_device, binding.original_inode)
                        && identity(&leaf)? == observed
                        && hash_file(held, binding.length)? == binding.sha256
                        && identity(&root.leaf(relative)?)? == observed
                } else {
                    false
                }
            } else {
                observed == original
            }
        } else {
            observed == original
        };
        if let Some(active) = &active {
            active.check()?;
        }
        runtime_root_check(&root,active.as_ref().and_then(|a|a.engine.as_deref()))?;
        Ok(result)
    }
    pub(crate) async fn activate_in(
        path: &Path,
        db: &AnyPool,
    ) -> Result<(), crate::auth::routes::AppError> {
        async fn activate(
            path: &Path,
            db: &AnyPool,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            let root = Root::open(path)?;
            root.check()?;
            let row = sqlx::query("SELECT g.generation_id,g.backup_id,g.manifest_sha256,g.database_sha256,g.ledger_sha256,g.root_device,g.root_inode,g.state FROM storage_restore_current c JOIN storage_restore_generations g ON g.generation_id=c.generation_id WHERE c.singleton=1").fetch_optional(db).await?;
            let pointer_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM storage_restore_current")
                    .fetch_one(db)
                    .await?;
            if pointer_count > 1 || (pointer_count == 1 && row.is_none()) {
                return Err(fail("Invalid restoration pointer").into());
            }
            let present = exists(root.fd(), NAMESPACE)?;
            let Some(row) = row else {
                if present {
                    return Err(fail("Restoration ledger without SQL commitment").into());
                }
                if registry()
                    .lock()
                    .map_err(|_| fail("Restoration registry unavailable"))?
                    .contains_key(&root.absolute)
                {
                    return Err(fail("Active restoration pointer removed").into());
                }
                root.check()?;
                return Ok(());
            };
            if !present {
                return Err(fail("SQL restoration commitment without ledger").into());
            }
            let active = Arc::new(Active::load(root)?);
            // Structural load cannot grant SQL authority. Validate the complete real
            // generation/current snapshot through the shared live adapter before registry use.
            // Legacy v1 serving historically validates only currenttuple. Preserve
            // those older SQL rows; repeated-backup admission remains strict.
            if active.version2 {
            let mut conn=db.acquire().await?;
            let (history_sql,history_pointer)=history_rows(&mut conn).await?;
            let closure=history_closure(HistorySource::new(&active.root,None)?,identity(active.root.fd())?,Some((&history_sql,&history_pointer)))?;
            closure.source.fence()?;
            if history_rows(&mut conn).await?!=(history_sql,history_pointer){return Err(fail("Activation SQL history changed").into());}
            drop(conn);
            }
            let string = |key| -> Result<String, sqlx::Error> { row.try_get(key) };
            if string("state")? != "committed"
                || string("generation_id")? != active.ledger.generation_id
                || string("backup_id")? != active.ledger.backup_id
                || string("manifest_sha256")? != active.ledger.manifest_sha256
                || string("database_sha256")? != active.ledger.database_sha256
                || string("ledger_sha256")? != active.ledger_sha256
                || string("root_device")? != active.ledger.root_device.to_string()
                || string("root_inode")? != active.ledger.root_inode.to_string()
            {
                return Err(fail("Restoration SQL commitment mismatch").into());
            }
            active.check()?;
            let mut map = registry()
                .lock()
                .map_err(|_| fail("Restoration registry unavailable"))?;
            if let Some(old) = map.get(&active.root.absolute) {
                old.check()?;
                if old.ledger_sha256 != active.ledger_sha256
                    || identity(&old.file)? != identity(&active.file)?
                {
                    return Err(
                        fail("Serving process cannot replace restoration generation").into(),
                    );
                }
            } else {
                map.insert(active.root.absolute.clone(), active);
            }
            Ok(())
        }
        activate(path, db)
            .await
            .map_err(crate::auth::routes::AppError::internal)
    }
    /// Commit a classified custody plan only in a new private, explicitly marked restore root.
    /// The application planner owns receipt/evidence classification. This never clears the
    /// marker, starts the application, modifies original journal evidence, or replaces a
    /// registered serving generation. A failed preparation remains non-bootable.
    pub(crate) async fn prepare_in(
        path: &Path,
        db: &AnyPool,
        ledger: &Ledger,
    ) -> Result<(), crate::auth::routes::AppError> {
        async fn prepare(
            path: &Path,
            db: &AnyPool,
            ledger: &Ledger,
        ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
            use std::io::Write;
            validate(ledger)?;
            let root = Root::open(path)?;
            if identity(root.fd())? != (ledger.root_device, ledger.root_inode)
                || root.fd().metadata()?.mode() & 0o777 != 0o700
            {
                return Err(fail("Restoration preparation requires matching private root").into());
            }
            if !exists(root.fd(), ".starlane-restore-incomplete")?
                || exists(root.fd(), ".starlane-backup-incomplete")?
                || exists(root.fd(), NAMESPACE)?
                || registry()
                    .lock()
                    .map_err(|_| fail("Restoration registry unavailable"))?
                    .contains_key(&root.absolute)
            {
                return Err(fail("Restoration preparation root is not fresh and marked").into());
            }
            let marker = open_at(
                root.fd(),
                ".starlane-restore-incomplete",
                libc::O_RDONLY | libc::O_NONBLOCK,
            )?;
            if !marker.metadata()?.is_file() {
                return Err(fail("Restoration marker is not regular").into());
            }
            for scope in &ledger.scopes {
                for binding in &scope.bindings {
                    let held = root.leaf(&binding.relative)?;
                    if identity(&held)? != (binding.adopted_device, binding.adopted_inode)
                        || hash_file(&held, binding.length)? != binding.sha256
                    {
                        return Err(fail("Classified restoration binding changed").into());
                    }
                }
            }
            let bytes = serde_json::to_vec(ledger)?;
            if bytes.len() as u64 > MAX_LEDGER {
                return Err(fail("Restoration ledger too large").into());
            }
            let family=crate::storage::native_family_fence::Creation::shared(path)?;
            let name = CString::new(NAMESPACE)?;
            // SAFETY: held root descriptor and fixed terminated namespace.
            if unsafe { libc::mkdirat(root.fd().as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
                return Err(io::Error::last_os_error().into());
            }
            root.fd().sync_all()?;
            let directory = open_at(root.fd(), NAMESPACE, libc::O_RDONLY | libc::O_DIRECTORY)?;
            let mut file = open_at(
                &directory,
                LEDGER_NAME,
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
            )?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            directory.sync_all()?;
            root.fd().sync_all()?;
            drop(family); // Never carry native creation exclusion into SQL waits.
            let mut tx = db.begin().await?;
            sqlx::query("INSERT INTO storage_restore_generations (generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state,created_at) VALUES (?,?,?,?,?,?,?,'pending',CURRENT_TIMESTAMP)").bind(&ledger.generation_id).bind(&ledger.backup_id).bind(&ledger.manifest_sha256).bind(&ledger.database_sha256).bind(digest(&bytes)).bind(ledger.root_device.to_string()).bind(ledger.root_inode.to_string()).execute(&mut *tx).await?;
            sqlx::query("DELETE FROM storage_restore_current WHERE singleton=1")
                .execute(&mut *tx)
                .await?;
            sqlx::query(
                "INSERT INTO storage_restore_current (singleton,generation_id) VALUES (1,?)",
            )
            .bind(&ledger.generation_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query("UPDATE storage_restore_generations SET state='committed' WHERE generation_id=? AND state='pending'").bind(&ledger.generation_id).execute(&mut *tx).await?;
            // Check the rooted ancestry and exact custody bytes again without bypassing the
            // expected preparation marker. Activation performs the normal unmarked fence.
            let fresh = Root::open(path)?;
            if fresh.chain.len() != root.chain.len() {
                return Err(fail("Restoration root moved during preparation").into());
            }
            for (a, b) in fresh.chain.iter().zip(&root.chain) {
                if identity(a)? != identity(b)? {
                    return Err(fail("Restoration root moved during preparation").into());
                }
            }
            let current_dir = open_at(root.fd(), NAMESPACE, libc::O_RDONLY | libc::O_DIRECTORY)?;
            let current = open_at(&current_dir, LEDGER_NAME, libc::O_RDONLY | libc::O_NONBLOCK)?;
            if identity(&current_dir)? != identity(&directory)?
                || identity(&current)? != identity(&file)?
                || digest(&read_ledger(&current)?) != digest(&bytes)
                || identity(&open_at(
                    root.fd(),
                    ".starlane-restore-incomplete",
                    libc::O_RDONLY | libc::O_NONBLOCK,
                )?)? != identity(&marker)?
            {
                return Err(fail("Restoration preparation custody changed").into());
            }
            tx.commit().await?;
            Ok(())
        }
        prepare(path, db, ledger)
            .await
            .map_err(crate::auth::routes::AppError::internal)
    }

    // Read-only capture lineage; does not register a serving generation.
    include!("restoration_history.rs");
    include!("restoration_verification.rs");
    include!("restoration_verification_spool.rs");
    include!("restoration_verification_pager.rs");
    include!("restoration_verification_owner_window.rs");
    include!("restoration_verification_worker.rs");
    include!("restoration_verification_ordinary_listener.rs");
    include!("restoration_verification_service_observation.rs");
    include!("restoration_verification_normal_worker.rs");
    include!("restoration_verification_listener.rs");
    include!("restoration_verification_audiences.rs");
    include!("restoration_verification_reader.rs");
    include!("restoration_verification_source.rs");
    include!("restoration_verification_probes.rs");
    include!("restoration_verification_drafts.rs");
    include!("restoration_verification_assets.rs");
    include!("restoration_verification_acceptance.rs");
    include!("restoration_sqlite_ordinary_target.rs");
    include!("restoration_sqlite_ordinary_probes.rs");
    include!("restoration_sqlite_ordinary_application.rs");
    pub(crate) type ValidatedPriorLineage = ValidatedHistory;
    pub(crate) use inspect_history as inspect_prior_lineage;
    // Opaque marked destination; no activation or public restore command.
    include!("restoration_destination.rs");
    // Certified groups remain marked and cannot activate the application.
    include!("restoration_materialization.rs");

    include!("restoration_pending_mint.rs");
    include!("restoration_publication_spool.rs");
    include!("restoration_publication_vfs.rs");
    include!("restoration_publication.rs");
    pub(crate) mod engine_transfer { include!("restoration_engine_transfer.rs"); }
    pub(crate) const ENGINE_REQUEST: &str = ".starlane-engine-transfer-request";
    /// Durable refusal intent, created before the original incomplete marker retires.
    pub(crate) fn mark_engine_request(d: &mut PlannedDestination) -> io::Result<()> {
        pending_new_file(d, ENGINE_REQUEST, b"PostgreSQL transfer requested; ordinary SQLite startup forbidden\n")?;
        Ok(())
    }
    /// Server-only admission gate. Private publication continues under its opaque token.
    pub(crate) fn engine_startup_requested(path: &Path) -> io::Result<bool> {
        let present = |name: &str| -> io::Result<bool> {
            match std::fs::symlink_metadata(path.join(name)) {
                Ok(_) => Ok(true),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
                Err(e) => Err(e),
            }
        };
        let engine = present(".engine-transfer-custody")?;
        let pg_restore = present(".pg-restoration-custody")?;
        if present(ENGINE_REQUEST)? && !engine && !pg_restore {return Err(fail("Requested PostgreSQL transfer is incomplete; SQLite fallback refused"));}
        Ok(engine || pg_restore)
    }
    pub(crate) use engine_transfer::{PreparedEngineTransfer,prepare_engine_transfer,EngineServing,open_engine_serving,RestoredPgServing,open_restored_pg_serving,QualifiedPgServing,open_qualified_pg_serving,open_current_qualified_pg_serving};

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::{
            io::{Read, Seek, SeekFrom},
            os::unix::fs::PermissionsExt,
        };
        struct Fixture {
            path: PathBuf,
            db: AnyPool,
            ledger: Ledger,
            evidence: Vec<u8>,
        }
        impl Fixture {
            async fn new() -> Self {
                sqlx::any::install_default_drivers();
                let path =
                    std::env::temp_dir().join(format!("restoration-core-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir(&path).unwrap();
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
                std::fs::create_dir(path.join("docs")).unwrap();
                std::fs::write(path.join("docs/page.md"), b"exact restored bytes").unwrap();
                let root = Root::open(&path).unwrap();
                let file = root.leaf("docs/page.md").unwrap();
                let id = identity(&file).unwrap();
                let evidence = b"original immutable evidence".to_vec();
                let ledger = Ledger { historical_preservation:None, composite:None,completed_absence:None,
                    admission:None, version: 1,
                    generation_id: uuid::Uuid::new_v4().to_string(),
                    backup_id: uuid::Uuid::new_v4().to_string(),
                    manifest_sha256: digest(b"manifest"),
                    database_sha256: digest(b"database"),
                    root_device: identity(root.fd()).unwrap().0,
                    root_inode: identity(root.fd()).unwrap().1,
                    scopes: vec![Scope {
                        namespace: ".storage-journal".into(),
                        op_id: uuid::Uuid::new_v4().to_string(),
                        evidence_sha256: digest(&evidence),
                        bindings: vec![Binding {
                            relative: "docs/page.md".into(),
                            original_device: 77,
                            original_inode: 88,
                            adopted_device: id.0,
                            adopted_inode: id.1,
                            length: 20,
                            sha256: digest(b"exact restored bytes"),
                        }],
                    }],
                };
                let db = sqlx::any::AnyPoolOptions::new()
                    .max_connections(1)
                    .connect("sqlite::memory:")
                    .await
                    .unwrap();
                sqlx::query("CREATE TABLE storage_restore_generations (generation_id TEXT PRIMARY KEY,backup_id TEXT NOT NULL,manifest_sha256 TEXT NOT NULL,database_sha256 TEXT NOT NULL,ledger_sha256 TEXT NOT NULL,root_device TEXT NOT NULL,root_inode TEXT NOT NULL,state TEXT NOT NULL,created_at TEXT NOT NULL)").execute(&db).await.unwrap();
                sqlx::query("CREATE TABLE storage_restore_current (singleton INTEGER PRIMARY KEY,generation_id TEXT NOT NULL)").execute(&db).await.unwrap();
                let result = Self {
                    path,
                    db,
                    ledger,
                    evidence,
                };
                result.write_ledger().unwrap();
                result.commit().await;
                result
            }
            fn write_ledger(&self) -> io::Result<()> {
                let dir = self.path.join(NAMESPACE);
                if !dir.exists() {
                    std::fs::create_dir(&dir)?;
                }
                std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
                std::fs::write(
                    dir.join(LEDGER_NAME),
                    serde_json::to_vec(&self.ledger).unwrap(),
                )?;
                std::fs::set_permissions(
                    dir.join(LEDGER_NAME),
                    std::fs::Permissions::from_mode(0o600),
                )?;
                Ok(())
            }
            async fn commit(&self) {
                sqlx::query("DELETE FROM storage_restore_current")
                    .execute(&self.db)
                    .await
                    .unwrap();
                sqlx::query("DELETE FROM storage_restore_generations")
                    .execute(&self.db)
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO storage_restore_generations VALUES (?,?,?,?,?,?,?,?,?)")
                    .bind(&self.ledger.generation_id)
                    .bind(&self.ledger.backup_id)
                    .bind(&self.ledger.manifest_sha256)
                    .bind(&self.ledger.database_sha256)
                    .bind(digest(
                        &std::fs::read(self.path.join(NAMESPACE).join(LEDGER_NAME)).unwrap(),
                    ))
                    .bind(self.ledger.root_device.to_string())
                    .bind(self.ledger.root_inode.to_string())
                    .bind("committed")
                    .bind("fixture")
                    .execute(&self.db)
                    .await
                    .unwrap();
                sqlx::query("INSERT INTO storage_restore_current VALUES (1,?)")
                    .bind(&self.ledger.generation_id)
                    .execute(&self.db)
                    .await
                    .unwrap();
            }
            fn owns(&self, file: &File) -> io::Result<bool> {
                owns(
                    &self.path,
                    ".storage-journal",
                    &self.ledger.scopes[0].op_id,
                    &self.evidence,
                    "docs/page.md",
                    (77, 88),
                    file,
                )
            }
            fn file(&self) -> File {
                File::open(self.path.join("docs/page.md")).unwrap()
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                registry().lock().unwrap().remove(&self.path);
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
        #[tokio::test]
        async fn private_preparation_preserves_prior_generations_and_marker_until_activation() {
            let f = Fixture::new().await;
            let prior = f.ledger.generation_id.clone();
            std::fs::remove_dir_all(f.path.join(NAMESPACE)).unwrap();
            std::fs::write(f.path.join(".starlane-restore-incomplete"), b"preparing").unwrap();
            let mut ledger = f.ledger.clone();
            ledger.generation_id = uuid::Uuid::new_v4().to_string();
            prepare_in(&f.path, &f.db, &ledger).await.unwrap();
            assert!(activate_in(&f.path, &f.db).await.is_err());
            let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_restore_generations")
                .fetch_one(&f.db)
                .await
                .unwrap();
            assert_eq!(n, 2);
            let old: String = sqlx::query_scalar(
                "SELECT state FROM storage_restore_generations WHERE generation_id=?",
            )
            .bind(prior)
            .fetch_one(&f.db)
            .await
            .unwrap();
            assert_eq!(old, "committed");
            std::fs::remove_file(f.path.join(".starlane-restore-incomplete")).unwrap();
            activate_in(&f.path, &f.db).await.unwrap();
            assert!(f.owns(&f.file()).unwrap());
            assert!(prepare_in(&f.path, &f.db, &ledger).await.is_err());
        }
        #[test]
        fn bootstrap_allows_fresh_absent_root_but_rejects_malformed_and_symlink_ancestry() {
            let path = std::env::temp_dir().join(format!("fresh-root-{}", uuid::Uuid::new_v4()));
            assert!(bootstrap_gate(&path).is_ok());
            assert!(bootstrap_gate(&path.join("../bad")).is_err());
            let link = path.with_extension("link");
            std::os::unix::fs::symlink("/tmp", &link).unwrap();
            assert!(bootstrap_gate(&link.join("missing")).is_err());
            std::fs::remove_file(link).unwrap();
        }
        #[tokio::test]
        async fn fresh_install_denies_all_classified_scopes_and_fences_invalid_custody() {
            let mut f = Fixture::new().await;
            assert!(
                fresh_install_allowed(&f.path, ".storage-journal", &f.ledger.scopes[0].op_id)
                    .is_err()
            );
            activate_in(&f.path, &f.db).await.unwrap();
            assert!(
                !fresh_install_allowed(&f.path, ".storage-journal", &f.ledger.scopes[0].op_id)
                    .unwrap()
            );
            assert!(fresh_install_allowed(
                &f.path,
                ".storage-journal",
                &uuid::Uuid::new_v4().to_string()
            )
            .unwrap());
            std::fs::write(f.path.join(".starlane-restore-incomplete"), b"pending").unwrap();
            assert!(fresh_install_allowed(
                &f.path,
                ".storage-journal",
                &uuid::Uuid::new_v4().to_string()
            )
            .is_err());
            std::fs::remove_file(f.path.join(".starlane-restore-incomplete")).unwrap();
            registry().lock().unwrap().remove(&f.path);
            f.ledger.scopes[0].bindings.clear();
            f.write_ledger().unwrap();
            f.commit().await;
            activate_in(&f.path, &f.db).await.unwrap();
            assert!(
                !fresh_install_allowed(&f.path, ".storage-journal", &f.ledger.scopes[0].op_id)
                    .unwrap()
            );
            registry().lock().unwrap().remove(&f.path);
            std::fs::remove_dir_all(f.path.join(NAMESPACE)).unwrap();
            sqlx::query("DELETE FROM storage_restore_current")
                .execute(&f.db)
                .await
                .unwrap();
            assert!(
                fresh_install_allowed(&f.path, ".storage-journal", &f.ledger.scopes[0].op_id)
                    .unwrap()
            );
        }
        #[tokio::test]
        async fn valid_sql_commit_adopts_scoped_identity_and_preserves_offset() {
            let f = Fixture::new().await;
            activate_in(&f.path, &f.db).await.unwrap();
            activate_in(&f.path, &f.db).await.unwrap();
            let mut held = f.file();
            held.seek(SeekFrom::Start(4)).unwrap();
            assert!(f.owns(&held).unwrap());
            assert_eq!(held.stream_position().unwrap(), 4);
            let mut bytes = [0; 3];
            held.read_exact(&mut bytes).unwrap();
            assert_eq!(&bytes, b"t r");
            assert!(!owns(
                &f.path,
                ".storage-journal",
                &f.ledger.scopes[0].op_id,
                b"different evidence",
                "docs/page.md",
                (77, 88),
                &held
            )
            .unwrap());
            assert!(!owns(
                &f.path,
                ".storage-journal",
                &f.ledger.scopes[0].op_id,
                &f.evidence,
                "docs/missing.md",
                (77, 88),
                &held
            )
            .unwrap());
            assert!(!owns(
                &f.path,
                ".storage-journal",
                &f.ledger.scopes[0].op_id,
                &f.evidence,
                "docs/page.md",
                identity(&held).unwrap(),
                &held
            )
            .unwrap());
            let new = uuid::Uuid::new_v4().to_string();
            assert!(owns(
                &f.path,
                ".storage-journal",
                &new,
                b"new",
                "docs/page.md",
                identity(&held).unwrap(),
                &held
            )
            .unwrap());
            assert!(!owns(
                &f.path,
                ".publication-journal",
                &new,
                b"new",
                "docs/page.md",
                (77, 88),
                &held
            )
            .unwrap());
        }
        #[tokio::test]
        async fn incomplete_markers_block_every_entrypoint_before_and_after_activation() {
            for marker in [
                ".starlane-restore-incomplete",
                ".starlane-backup-incomplete",
            ] {
                let f = Fixture::new().await;
                std::fs::write(f.path.join(marker), b"pending").unwrap();
                assert!(bootstrap_gate(&f.path).is_err());
                assert!(activate_in(&f.path, &f.db).await.is_err());
                assert!(fence(&f.path).is_err());
                assert!(f.owns(&f.file()).is_err());
                std::fs::remove_file(f.path.join(marker)).unwrap();
                activate_in(&f.path, &f.db).await.unwrap();
                std::fs::write(f.path.join(marker), b"pending").unwrap();
                assert!(f.owns(&f.file()).is_err());
                assert!(fence(&f.path).is_err());
            }
        }
        #[tokio::test]
        async fn namespace_and_sql_must_be_present_together() {
            let f = Fixture::new().await;
            assert!(fence(&f.path).is_err());
            assert!(f.owns(&f.file()).is_err());
            sqlx::query("DELETE FROM storage_restore_current")
                .execute(&f.db)
                .await
                .unwrap();
            assert!(activate_in(&f.path, &f.db).await.is_err());
            f.commit().await;
            std::fs::remove_file(f.path.join(NAMESPACE).join(LEDGER_NAME)).unwrap();
            assert!(activate_in(&f.path, &f.db).await.is_err());
            std::fs::remove_dir(f.path.join(NAMESPACE)).unwrap();
            assert!(activate_in(&f.path, &f.db).await.is_err());
            sqlx::query("DELETE FROM storage_restore_current")
                .execute(&f.db)
                .await
                .unwrap();
            activate_in(&f.path, &f.db).await.unwrap();
            assert!(fence(&f.path).is_ok());
            assert!(!f.owns(&f.file()).unwrap());
        }
        #[tokio::test]
        async fn activation_rejects_mismatched_sql_fields_and_pending_state() {
            for field in [
                "generation_id",
                "backup_id",
                "manifest_sha256",
                "database_sha256",
                "ledger_sha256",
                "root_device",
                "root_inode",
                "state",
            ] {
                let f = Fixture::new().await;
                sqlx::query(&format!(
                    "UPDATE storage_restore_generations SET {field}='wrong'"
                ))
                .execute(&f.db)
                .await
                .unwrap();
                assert!(activate_in(&f.path, &f.db).await.is_err(), "{field}");
            }
        }
        #[tokio::test]
        async fn activation_rejects_unknown_fields_duplicate_scope_and_unsafe_paths() {
            let mut f = Fixture::new().await;
            let mut json = serde_json::to_value(&f.ledger).unwrap();
            json["unknown"] = true.into();
            std::fs::write(
                f.path.join(NAMESPACE).join(LEDGER_NAME),
                serde_json::to_vec(&json).unwrap(),
            )
            .unwrap();
            f.commit().await;
            assert!(activate_in(&f.path, &f.db).await.is_err());
            f.write_ledger().unwrap();
            f.ledger.scopes.push(f.ledger.scopes[0].clone());
            f.write_ledger().unwrap();
            f.commit().await;
            assert!(activate_in(&f.path, &f.db).await.is_err());
            f.ledger.scopes.pop();
            f.ledger.scopes[0].bindings[0].relative = "../page.md".into();
            f.write_ledger().unwrap();
            f.commit().await;
            assert!(activate_in(&f.path, &f.db).await.is_err());
        }
        #[tokio::test]
        async fn private_mode_and_oversized_ledger_are_rejected() {
            let f = Fixture::new().await;
            let p = f.path.join(NAMESPACE).join(LEDGER_NAME);
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(activate_in(&f.path, &f.db).await.is_err());
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
            let held = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
            held.set_len(MAX_LEDGER + 1).unwrap();
            assert!(activate_in(&f.path, &f.db).await.is_err());
        }
        #[tokio::test]
        async fn scoped_history_without_binding_never_adopts_even_equal_original_ids() {
            let mut f = Fixture::new().await;
            f.ledger.scopes[0].bindings.clear();
            f.write_ledger().unwrap();
            f.commit().await;
            activate_in(&f.path, &f.db).await.unwrap();
            let held = f.file();
            assert!(!owns(
                &f.path,
                ".storage-journal",
                &f.ledger.scopes[0].op_id,
                &f.evidence,
                "docs/page.md",
                identity(&held).unwrap(),
                &held
            )
            .unwrap());
        }
        #[tokio::test]
        async fn leaf_bytes_copy_and_symlink_changes_fail_closed() {
            let f = Fixture::new().await;
            activate_in(&f.path, &f.db).await.unwrap();
            let held = f.file();
            std::fs::write(f.path.join("docs/page.md"), b"other restored bytes").unwrap();
            assert!(!f.owns(&held).unwrap());
            std::fs::write(f.path.join("docs/page.md"), b"exact restored bytes").unwrap();
            assert!(f.owns(&held).unwrap());
            std::fs::rename(f.path.join("docs/page.md"), f.path.join("docs/old.md")).unwrap();
            std::fs::copy(f.path.join("docs/old.md"), f.path.join("docs/page.md")).unwrap();
            assert!(!f.owns(&held).unwrap());
            assert!(!f.owns(&f.file()).unwrap());
            std::fs::remove_file(f.path.join("docs/page.md")).unwrap();
            std::os::unix::fs::symlink("old.md", f.path.join("docs/page.md")).unwrap();
            assert!(f.owns(&held).is_err());
        }
        #[tokio::test]
        async fn ledger_content_identity_and_directory_replacement_are_fenced() {
            let f = Fixture::new().await;
            activate_in(&f.path, &f.db).await.unwrap();
            let p = f.path.join(NAMESPACE).join(LEDGER_NAME);
            let bytes = std::fs::read(&p).unwrap();
            let mut changed = bytes.clone();
            changed.push(b' ');
            std::fs::write(&p, &changed).unwrap();
            assert!(fence(&f.path).is_err());
            std::fs::write(&p, &bytes).unwrap();
            assert!(fence(&f.path).is_ok());
            let copy = f.path.join("ledger-copy");
            std::fs::copy(&p, &copy).unwrap();
            std::fs::rename(&copy, &p).unwrap();
            assert!(fence(&f.path).is_err());
            assert!(activate_in(&f.path, &f.db).await.is_err());
            let g = Fixture::new().await;
            activate_in(&g.path, &g.db).await.unwrap();
            std::fs::rename(g.path.join(NAMESPACE), g.path.join("old-custody")).unwrap();
            std::fs::create_dir(g.path.join(NAMESPACE)).unwrap();
            std::fs::set_permissions(
                g.path.join(NAMESPACE),
                std::fs::Permissions::from_mode(0o700),
            )
            .unwrap();
            std::fs::copy(
                g.path.join("old-custody/current.json"),
                g.path.join(NAMESPACE).join(LEDGER_NAME),
            )
            .unwrap();
            assert!(fence(&g.path).is_err());
        }
        #[tokio::test]
        async fn root_replacement_and_competing_generation_are_rejected() {
            let mut f = Fixture::new().await;
            activate_in(&f.path, &f.db).await.unwrap();
            f.ledger.generation_id = uuid::Uuid::new_v4().to_string();
            f.write_ledger().unwrap();
            f.commit().await;
            assert!(activate_in(&f.path, &f.db).await.is_err());
            let g = Fixture::new().await;
            activate_in(&g.path, &g.db).await.unwrap();
            let moved = g.path.with_extension("old");
            std::fs::rename(&g.path, &moved).unwrap();
            std::fs::create_dir(&g.path).unwrap();
            assert!(fence(&g.path).is_err());
            std::fs::remove_dir(&g.path).unwrap();
            std::fs::rename(&moved, &g.path).unwrap();
        }
        #[test]
        fn hardlinks_preserve_relation_without_merging_independent_originals() {
            let binding = Binding {
                relative: "a".into(),
                original_device: 1,
                original_inode: 2,
                adopted_device: 3,
                adopted_inode: 4,
                length: 1,
                sha256: digest(b"x"),
            };
            let mut ledger = Ledger { historical_preservation:None, composite:None,completed_absence:None,
                admission:None, version: 1,
                generation_id: uuid::Uuid::new_v4().to_string(),
                backup_id: uuid::Uuid::new_v4().to_string(),
                manifest_sha256: digest(b"m"),
                database_sha256: digest(b"d"),
                root_device: 1,
                root_inode: 1,
                scopes: vec![Scope {
                    namespace: ".storage-journal".into(),
                    op_id: uuid::Uuid::new_v4().to_string(),
                    evidence_sha256: digest(b"e"),
                    bindings: vec![binding.clone()],
                }],
            };
            let mut second = binding;
            second.relative = "b".into();
            ledger.scopes[0].bindings.push(second);
            assert!(validate(&ledger).is_ok());
            ledger.scopes[0].bindings[1].length = 2;
            assert!(validate(&ledger).is_err());
            ledger.scopes[0].bindings[1].length = 1;
            ledger.scopes[0].bindings[1].sha256 = digest(b"y");
            assert!(validate(&ledger).is_err());
            ledger.scopes[0].bindings[1].sha256 = digest(b"x");
            ledger.scopes[0].bindings[1].adopted_inode = 5;
            assert!(validate(&ledger).is_err());
            ledger.scopes[0].bindings[1].adopted_inode = 4;
            ledger.scopes[0].bindings[1].original_inode = 9;
            assert!(validate(&ledger).is_err());
        }
    }
}
#[cfg(target_os = "linux")]
pub(crate) use linux::{
    QualifiedErasureSource,qualified_erasure_source_in,
    engine_startup_requested, mark_engine_request, HistorySnapshot, PreparedEngineTransfer, prepare_engine_transfer, EngineServing, open_engine_serving, RestoredPgServing, open_restored_pg_serving, QualifiedPgServing, open_qualified_pg_serving, open_current_qualified_pg_serving,
    activate_in, bootstrap_gate, fence, fresh_install_allowed, owns, prepare_in,
    seal_publication, commit_publication, retire_publication,
    inspect_prior_lineage, ValidatedPriorLineage, reserve_and_copy, PreparedDestination,
    materialize_groups, PlannedDestination, mint_pending_preproof,
};
#[cfg(all(target_os = "linux", test))]
pub(crate) use linux::materialize_groups_test;
#[cfg(all(target_os = "linux", test))]
pub(crate) use linux::mint_pending_preproof_test;

#[cfg(not(target_os = "linux"))]
pub(crate) fn bootstrap_gate(root: &Path) -> io::Result<()> {
    for marker in [
        ".starlane-engine-transfer-request",
        ".engine-transfer-custody",
        ".pg-restoration-custody",
        ".starlane-restore-incomplete",
        ".starlane-backup-incomplete",
        NAMESPACE,
    ] {
        match std::fs::symlink_metadata(root.join(marker)) {
            Ok(_) => return Err(fail("Restoration custody unavailable on this platform")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn fence(root: &Path) -> io::Result<()> {
    bootstrap_gate(root)
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn owns(
    root: &Path,
    _namespace: &str,
    _op_id: &str,
    _evidence: &[u8],
    _relative: &str,
    _original: (u64, u64),
    _held: &File,
) -> io::Result<bool> {
    fence(root)?;
    Ok(false)
}
#[cfg(not(target_os = "linux"))]
pub(crate) async fn activate_in(
    root: &Path,
    db: &AnyPool,
) -> Result<(), crate::auth::routes::AppError> {
    bootstrap_gate(root).map_err(crate::auth::routes::AppError::internal)?;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_restore_current")
        .fetch_one(db)
        .await
        .map_err(crate::auth::routes::AppError::internal)?;
    if n != 0 {
        return Err(crate::auth::routes::AppError::Internal(
            "Restoration custody unavailable on this platform".into(),
        ));
    }
    Ok(())
}

#[cfg(target_os="linux")]
pub(crate) use linux::{CompletedAbsenceAuthority,acquire_completed_absence_in};

#[cfg(target_os="linux")]
pub(crate) use linux::{PriorCompletedCaptureEvidence,MixedCompletedCaptureEvidence,prior_completed_bundle_roles};

#[cfg(all(test,target_os="linux"))]
pub(crate) use linux::FinalizedCompletedCapture;
