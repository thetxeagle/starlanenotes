//! Private operational history; completion records are not restore certification.
use serde::Deserialize;
use std::{fs::{self, File, OpenOptions}, io::Read, path::{Path, PathBuf}};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

use std::sync::{Arc, Mutex, OnceLock, Weak};

pub(crate) struct PoolSource { root: PathBuf, directory: File, database: File }
impl PoolSource {
    fn check(&self) -> Result<(), ()> {
        same_current(&self.root, &self.directory)?;
        same_current(&self.root.join("starlane-notes.db"), &self.database)
    }
}
type PoolRegistry = Vec<(Weak<sqlx::any::AnyConnectOptions>, Arc<PoolSource>)>;
fn registry() -> &'static Mutex<PoolRegistry> {
    static REGISTRY: OnceLock<Mutex<PoolRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}
/// Retain native source handles at pool initialization. Never mint a binding
/// lazily from an administrator observation after files may have been replaced.
pub(crate) fn retain_pool_source(pool: &sqlx::AnyPool, root: &Path) {
    let capture = || -> Result<PoolSource, ()> {
        let root = absolute_private_path(root)?;
        let directory = directory(&root)?;
        let database = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW)
            .open(root.join("starlane-notes.db")).map_err(|_| ())?;
        if !database.metadata().map_err(|_| ())?.is_file() { return Err(()); }
        let source = PoolSource { root, directory, database }; source.check()?; Ok(source)
    };
    if let Ok(source) = capture() {
        if let Ok(mut registry) = registry().lock() {
            registry.retain(|(key,_)| key.strong_count()>0);
            let key=pool.connect_options();
            if !registry.iter().any(|(prior,_)| prior.upgrade().is_some_and(|prior| Arc::ptr_eq(&prior,&key))) {
                registry.push((Arc::downgrade(&key),Arc::new(source)));
            }
        }
    }
}
pub(super) fn pool_source(pool: &sqlx::AnyPool, root: &Path) -> Option<Arc<PoolSource>> {
    let key=pool.connect_options(); let root=absolute_private_path(root).ok()?;
    let registry=registry().lock().ok()?;
    let source=registry.iter().find(|(prior,_)| prior.upgrade().is_some_and(|prior| Arc::ptr_eq(&prior,&key)))?.1.clone();
    (source.root==root && source.check().is_ok()).then_some(source)
}
pub(super) fn read_bound(path: &Path, source: &PoolSource) -> Result<Option<String>, ()> {
    source.check()?; let result=read(path,&source.root)?; source.check()?; Ok(result)
}

const MAX_ENTRIES: usize = 256;
const MAX_RECORD_BYTES: u64 = 8192;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    format: String, format_version: u64, operation: String, operation_id: String,
    status: String, scope: String, completed_at: String, application_version: String,
    database_engine: String, source: Source, evidence: Evidence,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source { path: PathBuf, root: Identity, database: Identity }
#[derive(Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Identity { device: u64, inode: u64 }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence { backup_id: String, manifest: Digest }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Digest { size_bytes: u64, sha256: String }
fn identity(md: &fs::Metadata) -> Identity { Identity { device: md.dev(), inode: md.ino() } }
fn stable(md: &fs::Metadata) -> (u64,u64,u64,u32,u32,u64,i64,i64,i64,i64) {
    (md.dev(),md.ino(),md.nlink(),md.mode(),md.uid(),md.len(),md.mtime(),md.mtime_nsec(),md.ctime(),md.ctime_nsec())
}
fn absolute_private_path(path: &Path) -> Result<PathBuf, ()> {
    let absolute = if path.is_absolute() { path.to_owned() } else { std::env::current_dir().map_err(|_| ())?.join(path) };
    if absolute.components().any(|c| matches!(c, std::path::Component::ParentDir)) { return Err(()); }
    for ancestor in absolute.ancestors() {
        if fs::symlink_metadata(ancestor).map_err(|_| ())?.file_type().is_symlink() { return Err(()); }
    }
    Ok(absolute)
}
fn directory(path: &Path) -> Result<File, ()> {
    OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW).open(path).map_err(|_| ())
}
fn same_current(path: &Path, held: &File) -> Result<(), ()> {
    let current=fs::symlink_metadata(path).map_err(|_| ())?;
    if current.file_type().is_symlink() || identity(&current)!=identity(&held.metadata().map_err(|_| ())?) { return Err(()); }
    Ok(())
}
fn valid(record: &Record, name: &str, root: &Path, root_md: &fs::Metadata, db_md: &fs::Metadata) -> Option<chrono::DateTime<chrono::Utc>> {
    let id=uuid::Uuid::parse_str(&record.operation_id).ok()?;
    if id.to_string()!=record.operation_id || name!=format!("backup-{id}.json")
        || record.evidence.backup_id!=record.operation_id
        || record.format!="starlane-storage-operation-record" || record.format_version!=1
        || record.operation!="sqlite_backup" || record.status!="completed"
        || record.scope!="stopped_write_storage_bytes" || record.database_engine!="sqlite"
        || record.application_version.is_empty() || record.application_version.len()>64
        || record.source.path!=root || record.source.root!=identity(root_md) || record.source.database!=identity(db_md)
        || record.evidence.manifest.size_bytes==0 || record.evidence.manifest.size_bytes>4*1024*1024
        || record.evidence.manifest.sha256.len()!=64
        || !record.evidence.manifest.sha256.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) { return None; }
    let time=chrono::DateTime::parse_from_rfc3339(&record.completed_at).ok()?.with_timezone(&chrono::Utc);
    (time<=chrono::Utc::now()).then_some(time)
}
/// Read at most 256 entries and 8 KiB per record. Never write or repair history.
/// The caller has independently bound the connected SQLite database to `root`.
pub(super) fn read(directory_path: &Path, root: &Path) -> Result<Option<String>, ()> {
    let root=absolute_private_path(root)?;
    let path=absolute_private_path(directory_path)?;
    if path.starts_with(&root) || root.starts_with(&path) { return Err(()); }
    let held_root=directory(&root)?;
    let root_md=held_root.metadata().map_err(|_| ())?;
    let db_path=root.join("starlane-notes.db");
    let db=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(&db_path).map_err(|_| ())?;
    let db_md=db.metadata().map_err(|_| ())?;
    if !db_md.is_file() { return Err(()); }
    let held=directory(&path)?;
    let md=held.metadata().map_err(|_| ())?;
    let uid=unsafe { libc::geteuid() };
    if md.mode() & 0o7777 != 0o700 || md.uid()!=uid { return Err(()); }
    // Enumeration and opens stay attached to the retained directory, including
    // during rename. Final native identity checks refuse an exchanged root.
    let held_path=PathBuf::from(format!("/proc/self/fd/{}", std::os::fd::AsRawFd::as_raw_fd(&held)));
    let mut latest=None;
    for (count,entry) in fs::read_dir(&held_path).map_err(|_| ())?.enumerate() {
        if count>=MAX_ENTRIES { return Err(()); }
        let entry=entry.map_err(|_| ())?;
        let name=entry.file_name(); let Some(name)=name.to_str() else { continue };
        if !name.starts_with("backup-") || !name.ends_with(".json") { continue; }
        let pending=held_path.join(format!(".pending-{}", &name[7..]));
        match fs::symlink_metadata(&pending) {
            Ok(_) => continue,
            Err(e) if e.kind()==std::io::ErrorKind::NotFound => (),
            Err(_) => return Err(()),
        }
        let record_path=held_path.join(name);
        let Ok(mut file)=OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(&record_path) else { continue };
        let before=file.metadata().map_err(|_| ())?;
        if !before.is_file() || before.mode() & 0o7777 != 0o600 || before.uid()!=uid || before.nlink()!=1 || before.len()>MAX_RECORD_BYTES { continue; }
        let mut bytes=Vec::new();
        file.by_ref().take(MAX_RECORD_BYTES+1).read_to_end(&mut bytes).map_err(|_| ())?;
        if bytes.len() as u64>MAX_RECORD_BYTES { continue; }
        let Ok(record)=serde_json::from_slice::<Record>(&bytes) else { continue };
        let Some(time)=valid(&record,name,&root,&root_md,&db_md) else { continue };
        let after=file.metadata().map_err(|_| ())?;
        let current=fs::symlink_metadata(&record_path).map_err(|_| ())?;
        if stable(&before)!=stable(&after) || stable(&after)!=stable(&current) { return Err(()); }
        match fs::symlink_metadata(&pending) {
            Err(e) if e.kind()==std::io::ErrorKind::NotFound => (), _ => return Err(()),
        }
        if latest.is_none_or(|prior| time>prior) { latest=Some(time); }
    }
    same_current(&path,&held)?; same_current(&root,&held_root)?; same_current(&db_path,&db)?;
    // Recheck permissions and record-directory metadata, including additions/removals.
    if stable(&md)!=stable(&held.metadata().map_err(|_| ())?) { return Err(()); }
    Ok(latest.map(|time| time.to_rfc3339()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    #[ignore = "requires actual Python-produced backup and explicitly owned private fixture"]
    fn actual_producer_and_refusal_controls() {
        let base=PathBuf::from(std::env::var("STARLANE_RECOVERY_RECORD_FIXTURE_DIR").expect("explicit private fixture required"));
        assert!(base.is_absolute());
        let source=PathBuf::from(std::env::var("STARLANE_RECOVERY_RECORD_SOURCE").expect("actual application source required"));
        let original=base.join("actual-records");
        let files:Vec<_>=fs::read_dir(&original).unwrap().map(|e| e.unwrap().path()).collect();
        assert_eq!(files.len(),1);
        let name=files[0].file_name().unwrap();
        let bytes=fs::read(&files[0]).unwrap();
        assert!(read(&original,&source).unwrap().is_some());
        let controls=base.join(format!("reader-controls-{}",uuid::Uuid::new_v4()));
        fs::create_dir(&controls).unwrap();fs::set_permissions(&controls,fs::Permissions::from_mode(0o700)).unwrap();
        let file=controls.join(name);
        let reset=|| { fs::write(&file,&bytes).unwrap();fs::set_permissions(&file,fs::Permissions::from_mode(0o600)).unwrap(); };
        reset(); assert!(read(&controls,&source).unwrap().is_some());
        for key in ["format", "operation", "scope", "database_engine", "status"] {
            let mut value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();value[key]="foreign".into();
            fs::write(&file,serde_json::to_vec(&value).unwrap()).unwrap();assert_eq!(read(&controls,&source).unwrap(),None,"{key}");
        }
        for key in ["root","database"] {
            let mut value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();value["source"][key]["inode"]=0.into();
            fs::write(&file,serde_json::to_vec(&value).unwrap()).unwrap();assert_eq!(read(&controls,&source).unwrap(),None,"{key}");
        }
        reset(); let pending=controls.join(format!(".pending-{}",name.to_str().unwrap().strip_prefix("backup-").unwrap()));
        fs::write(&pending,b"incomplete").unwrap();assert_eq!(read(&controls,&source).unwrap(),None);fs::remove_file(&pending).unwrap();
        let linked=controls.join("retained-link");fs::hard_link(&file,&linked).unwrap();assert_eq!(read(&controls,&source).unwrap(),None);fs::remove_file(linked).unwrap();
        fs::set_permissions(&file,fs::Permissions::from_mode(0o644)).unwrap();assert_eq!(read(&controls,&source).unwrap(),None);
        reset();fs::remove_file(&file).unwrap();std::os::unix::fs::symlink(&files[0],&file).unwrap();assert_eq!(read(&controls,&source).unwrap(),None);fs::remove_file(&file).unwrap();
        reset();fs::write(&file,vec![b' ';(MAX_RECORD_BYTES+1) as usize]).unwrap();assert_eq!(read(&controls,&source).unwrap(),None);
        reset();let mut value:serde_json::Value=serde_json::from_slice(&bytes).unwrap();value["unexpected"]=true.into();fs::write(&file,serde_json::to_vec(&value).unwrap()).unwrap();assert_eq!(read(&controls,&source).unwrap(),None);
        reset();fs::set_permissions(&controls,fs::Permissions::from_mode(0o755)).unwrap();assert!(read(&controls,&source).is_err());fs::set_permissions(&controls,fs::Permissions::from_mode(0o700)).unwrap();
        for index in 0..MAX_ENTRIES {fs::write(controls.join(format!("bounded-{index}")),b"").unwrap();}
        assert!(read(&controls,&source).is_err());
        assert_eq!(fs::read(&files[0]).unwrap(),bytes);
    }
}
