//! Native local custody is distinct from readonly server version provenance.
use super::filesystem::{identity, Directory, Identity};
use super::*;
use sha2::{Digest, Sha256};
use sqlx::Connection;
use std::fs::File;
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileExt, MetadataExt};
const CHUNK: usize = 64 * 1024;
const LIMIT: u64 = 100 * 1024 * 1024;
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn err(e: impl ToString) -> String {
    e.to_string()
}
fn canonical_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_digit() || (b'a'..=b'f').contains(&c)
            }
        })
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Destination {
    pub root_path: PathBuf,
    pub root: Identity,
    pub database: Identity,
}
impl Destination {
    pub fn capture(selection: &vault::SnapshotRequestBinding) -> Result<Self, String> {
        let path = selection.expected_database_path()?;
        let parent = path.parent().ok_or("Native database parent missing")?;
        let root_path = parent.parent().ok_or("Native root missing")?.to_path_buf();
        let root = Directory::capture(&root_path).map_err(err)?;
        let database = root.child(".starlane", false).map_err(err)?;
        let main = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Native database name invalid")?;
        let database = database
            .file_identity(main)
            .map_err(err)?
            .ok_or("Initialized original native database required")?;
        selection.verify_selection()?;
        root.fence().map_err(err)?;
        Ok(Self {
            root_path,
            root: identity(&root.held).map_err(err)?,
            database,
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Association {
    pub space_id: String,
    pub document_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    pub operation_id: String,
    pub destination: Destination,
    pub origin: Value,
    pub server_version_raw: String,
    pub association: Association,
}
impl Request {
    fn version(&self) -> Result<Value, String> {
        serde_json::from_str(&self.server_version_raw)
            .map_err(|_| "Server provenance JSON malformed".into())
    }
    fn validate(&self) -> Result<Value, String> {
        // Bound the complete persisted envelope, including JSON escaping and
        // origin/path fields; bounding only raw server JSON is insufficient.
        if serde_json::to_vec(self).map_err(err)?.len() > 60 * 1024 {
            return Err("Native request metadata exceeds its private envelope budget".into());
        }
        if !canonical_uuid(&self.operation_id) || self.server_version_raw.len() > 65536 {
            return Err("Native import request invalid".into());
        }
        let origin = self.origin.as_object().ok_or("Native origin invalid")?;
        if origin.len() != 7 || origin.get("version") != Some(&json!(1)) {
            return Err("Native origin invalid".into());
        }
        for key in [
            "serverBase",
            "accountId",
            "sessionId",
            "cacheScope",
            "storageMode",
            "provider",
        ] {
            if origin
                .get(key)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .is_none()
            {
                return Err("Native origin invalid".into());
            }
        }
        if origin["storageMode"] != "server"
            || !matches!(
                origin["provider"].as_str(),
                Some("none" | "icloud-drive" | "google-drive")
            )
        {
            return Err("Native origin is not a connected server download".into());
        }
        if self.association.space_id.is_empty() || self.association.document_id.is_empty() {
            return Err("Application-owned native page association required".into());
        }
        let v = self.version()?;
        let m = v.as_object().ok_or("Server version invalid")?;
        for key in [
            "version_id",
            "logical_attachment_id",
            "original_space_id",
            "original_document_id",
            "uploader_id",
            "original_name",
            "mime_type",
            "binary_sha256",
            "original_created_at",
        ] {
            if m.get(key)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .is_none()
            {
                return Err("Server version metadata incomplete".into());
            }
        }
        let length = v["byte_length"]
            .as_u64()
            .filter(|n| *n <= LIMIT)
            .ok_or("Server attachment exceeds native import limit")?;
        let hash = v["binary_sha256"].as_str().unwrap();
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("Server attachment hash invalid".into());
        }
        if v["version_number"].as_u64().filter(|n| *n > 0).is_none() {
            return Err("Server version ordinal invalid".into());
        }
        let _ = length;
        Ok(v)
    }
    fn current(&self, current: &Value) -> Result<(), String> {
        if current != &self.origin {
            return Err("Original native actor/server/session context changed".into());
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
struct Intent {
    format: u32,
    request: Request,
    root: Identity,
    database: Identity,
    directory: Identity,
    payload: Identity,
    created_at: String,
}
#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
struct Part {
    index: u64,
    length: u64,
    sha256: String,
    identity: Identity,
}
#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
struct Proof {
    format: u32,
    intent_sha256: String,
    payload: Identity,
    length: u64,
    sha256: String,
}
#[derive(Clone)]
pub(super) struct Captured {
    selection: vault::SnapshotRequestBinding,
    root: Directory,
    database: Directory,
    main: String,
    database_id: Identity,
    request: Request,
}
impl Captured {
    /// Synchronous: hold the selected initialized root and DB before network await.
    pub fn capture(
        selection: vault::SnapshotRequestBinding,
        request: Request,
        current: &Value,
    ) -> Result<Self, String> {
        request.validate()?;
        request.current(current)?;
        let path = selection.expected_database_path()?;
        let parent = path.parent().ok_or("Native database parent missing")?;
        let root = parent.parent().ok_or("Native root missing")?;
        let root = Directory::capture(root).map_err(err)?;
        let database = root.child(".starlane", false).map_err(err)?;
        let main = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("Native database name invalid")?
            .to_string();
        let expected_main = match request.origin["provider"].as_str().unwrap() {
            "none" => "starlane-notes.sqlite3".to_string(),
            provider => format!("starlane-vault-{provider}.sqlite3"),
        };
        if main != expected_main {
            return Err("Native provider differs from original database destination".into());
        }
        let database_id = database
            .file_identity(&main)
            .map_err(err)?
            .ok_or("Initialized original native database required")?;
        if request.destination.root_path != root.path
            || request.destination.root != identity(&root.held).map_err(err)?
            || request.destination.database != database_id
        {
            return Err("Original caller-persisted native destination changed".into());
        }
        let result = Self {
            selection,
            root,
            database,
            main,
            database_id,
            request,
        };
        result.fence(current)?;
        Ok(result)
    }
    fn fence(&self, current: &Value) -> Result<(), String> {
        self.request.current(current)?;
        self.selection.verify_selection()?;
        self.root.fence().map_err(err)?;
        self.database.fence().map_err(err)?;
        if self.database.file_identity(&self.main).map_err(err)? != Some(self.database_id) {
            return Err("Original native database identity changed".into());
        }
        Ok(())
    }
    fn directory(&self, create: bool) -> Result<Directory, String> {
        self.database
            .child("attachment-downloads", create)
            .and_then(|d| d.child(&self.request.operation_id, create))
            .map_err(err)
    }
    /// Called before any authenticated byte fetch. Exact SQL retries may already
    /// be resolved; absent receipts require a current application-owned page.
    pub async fn prepare_fetch(&self, current: &Value) -> Result<Option<Value>, String> {
        let owner = self.clone();
        let current = current.clone();
        connection::supervise(async move { owner.prepare_fetch_owned(&current).await }).await
    }
    async fn prepare_fetch_owned(&self, current: &Value) -> Result<Option<Value>, String> {
        let _guard = snapshot_commit_lock().lock().await;
        self.fence(current)?;
        let mut db = connection::Database::prepare(self.database.clone(), &self.main).await?;
        let result = async {
            if let Some(prior) = self.resolve(&mut db, current).await? {
                return Ok(Some(prior));
            }
            let snapshot = read_snapshot_from_connection(
                db.connection(),
                self.request.origin["provider"].as_str().unwrap(),
            )
            .await?
            .ok_or("Original native canonical snapshot missing")?;
            self.require_association(&snapshot)?;
            self.fence(current)?;
            db.fence()?;
            Ok(None)
        }
        .await;
        db.retire()
            .await
            .map_err(|_| "Native worker retirement did not complete".to_string())?;
        result
    }
    fn require_association(&self, snapshot: &Value) -> Result<(), String> {
        if !snapshot["documents"].as_array().is_some_and(|docs| {
            docs.iter().any(|d| {
                d["id"] == self.request.association.document_id
                    && d["space_id"] == self.request.association.space_id
            })
        }) || !snapshot["spaces"].as_array().is_some_and(|spaces| {
            spaces
                .iter()
                .any(|s| s["id"] == self.request.association.space_id)
        }) {
            return Err("Original application-owned native page association changed".into());
        }
        Ok(())
    }
    pub async fn begin(&self, current: &Value) -> Result<(), String> {
        let owner = self.clone();
        let current = current.clone();
        connection::supervise(async move { owner.begin_owned(&current).await }).await
    }
    async fn begin_owned(&self, current: &Value) -> Result<(), String> {
        let _guard = snapshot_commit_lock().lock().await;
        self.fence(current)?;
        let owner = self.clone();
        tokio::task::spawn_blocking(move || owner.begin_disk())
            .await
            .map_err(err)??;
        self.fence(current)
    }
    fn begin_disk(&self) -> Result<(), String> {
        let dir = self.directory(true)?;
        let existing = read_private(&dir, "intent.json", 65536)?;
        if let Some(raw) = existing {
            self.intent(&dir, &raw)?;
            return Ok(());
        }
        // An interrupted unsealed allocation is retained, never adopted as a retry.
        if dir.open("payload.bin", libc::O_RDONLY).is_ok() {
            return Err("Interrupted unsealed native payload retained for review".into());
        }
        let file = dir
            .open("payload.bin", libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)
            .map_err(err)?;
        file.sync_all().map_err(err)?;
        dir.held.sync_all().map_err(err)?;
        let intent = Intent {
            format: 1,
            request: self.request.clone(),
            root: identity(&self.root.held).map_err(err)?,
            database: self.database_id,
            directory: identity(&dir.held).map_err(err)?,
            payload: identity(&file).map_err(err)?,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(err)?
                .as_millis()
                .to_string(),
        };
        persist(
            &dir,
            "intent.json",
            &serde_json::to_vec(&intent).map_err(err)?,
        )?;
        dir.fence().map_err(err)
    }
    fn intent(&self, dir: &Directory, raw: &[u8]) -> Result<Intent, String> {
        let intent: Intent =
            serde_json::from_slice(raw).map_err(|_| "Native original intent malformed")?;
        if intent.format != 1
            || intent.request != self.request
            || intent.root != identity(&self.root.held).map_err(err)?
            || intent.database != self.database_id
            || intent.directory != identity(&dir.held).map_err(err)?
        {
            return Err("Original native operation/root/context binding differs".into());
        }
        let file = dir.open("payload.bin", libc::O_RDONLY).map_err(err)?;
        regular(&file, 2)?;
        if identity(&file).map_err(err)? != intent.payload {
            return Err("Original native payload identity differs".into());
        }
        Ok(intent)
    }
    pub async fn append(&self, current: &Value, index: u64, bytes: Vec<u8>) -> Result<(), String> {
        let owner = self.clone();
        let current = current.clone();
        connection::supervise(async move { owner.append_owned(&current, index, bytes).await }).await
    }
    async fn append_owned(
        &self,
        current: &Value,
        index: u64,
        bytes: Vec<u8>,
    ) -> Result<(), String> {
        self.fence(current)?;
        if bytes.is_empty() || bytes.len() > CHUNK {
            return Err("Native chunk must contain 1..65536 bytes".into());
        }
        let _guard = snapshot_commit_lock().lock().await;
        self.fence(current)?;
        let owner = self.clone();
        tokio::task::spawn_blocking(move || owner.append_disk(index, &bytes))
            .await
            .map_err(err)??;
        self.fence(current)
    }
    fn append_disk(&self, index: u64, bytes: &[u8]) -> Result<(), String> {
        let v = self.request.validate()?;
        let length = v["byte_length"].as_u64().unwrap();
        let offset = index
            .checked_mul(CHUNK as u64)
            .ok_or("Native chunk index overflow")?;
        if offset >= length || bytes.len() as u64 != std::cmp::min(CHUNK as u64, length - offset) {
            return Err("Native chunk does not match declared exact span".into());
        }
        let dir = self.directory(false)?;
        let raw = read_private(&dir, "intent.json", 65536)?.ok_or("Native intent missing")?;
        self.intent(&dir, &raw)?;
        let data = format!("part-{index}.bin");
        let name = format!("part-{index}.json");
        let hash = digest(bytes);
        let f = match read_private(&dir, &name, 4096)? {
            Some(raw) => {
                let part: Part =
                    serde_json::from_slice(&raw).map_err(|_| "Native part proof malformed")?;
                let f = dir.open(&data, libc::O_RDWR).map_err(err)?;
                regular(&f, 1)?;
                if part
                    != (Part {
                        index,
                        length: bytes.len() as u64,
                        sha256: hash.clone(),
                        identity: identity(&f).map_err(err)?,
                    })
                {
                    return Err("Original native chunk identity/bytes differ".into());
                }
                f
            }
            None => {
                let f = dir
                    .open(&data, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)
                    .map_err(err)?;
                f.sync_all().map_err(err)?;
                dir.held.sync_all().map_err(err)?;
                let part = Part {
                    index,
                    length: bytes.len() as u64,
                    sha256: hash,
                    identity: identity(&f).map_err(err)?,
                };
                persist(&dir, &name, &serde_json::to_vec(&part).map_err(err)?)?;
                f
            }
        };
        if f.metadata().map_err(err)?.len() > bytes.len() as u64 {
            return Err("Native part has unexpected tail".into());
        }
        finish_prefix(&f, bytes, 0)?;
        f.sync_all().map_err(err)?;
        let fresh = dir.open(&data, libc::O_RDONLY).map_err(err)?;
        regular(&fresh, 1)?;
        if identity(&fresh).map_err(err)? != identity(&f).map_err(err)?
            || read_bounded(&fresh, CHUNK as u64)? != bytes
        {
            return Err("Native chunk path changed during staging".into());
        }
        dir.fence().map_err(err)
    }
    pub async fn finish(&self, current: &Value) -> Result<Value, String> {
        let owner = self.clone();
        let current = current.clone();
        connection::supervise(async move { owner.finish_owned(&current).await }).await
    }
    async fn finish_owned(&self, current: &Value) -> Result<Value, String> {
        let _guard = snapshot_commit_lock().lock().await;
        self.fence(current)?;
        let mut db = connection::Database::prepare(self.database.clone(), &self.main).await?;
        // Resolve original SQL evidence BEFORE touching staged/final assets. A
        // historical receipt cannot recreate a removed association or physical file.
        let result = async {
            if let Some(prior) = self.resolve(&mut db, current).await? {
                return Ok(prior);
            }
            let owner = self.clone();
            let asset = tokio::task::spawn_blocking(move || owner.install())
                .await
                .map_err(err)??;
            self.fence(current)?;
            self.commit(&mut db, current, asset).await
        }
        .await;
        db.retire()
            .await
            .map_err(|_| "Native worker retirement did not complete".to_string())?;
        result
    }
    async fn resolve(
        &self,
        db: &mut connection::Database,
        current: &Value,
    ) -> Result<Option<Value>, String> {
        let exists:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='native_attachment_download_receipts'").fetch_one(db.connection()).await.map_err(err)?;
        if exists == 0 {
            return Ok(None);
        }
        let prior:Option<(String,String)>=sqlx::query_as("SELECT request_sha256,receipt FROM native_attachment_download_receipts WHERE operation_id=?1").bind(&self.request.operation_id).fetch_optional(db.connection()).await.map_err(err)?;
        let Some((sha, raw)) = prior else {
            return Ok(None);
        };
        if sha != digest(&serde_json::to_vec(&self.request).map_err(err)?) {
            return Err("Native retry request differs".into());
        }
        let ack: Value = serde_json::from_str(&raw).map_err(|_| "Native receipt malformed")?;
        if ack["operation_id"] != self.request.operation_id || ack["canonical_committed"] != true {
            return Err("Native receipt request binding invalid".into());
        }
        let snapshot = read_snapshot_from_connection(
            db.connection(),
            self.request.origin["provider"].as_str().unwrap(),
        )
        .await?
        .unwrap_or_else(|| json!({}));
        let current_asset = snapshot["assets"].as_array().and_then(|a| {
            a.iter()
                .find(|a| a["id"] == format!("native-download-{}", self.request.operation_id))
        });
        self.fence(current)?;
        db.fence()?;
        if let Some(asset) = current_asset {
            if asset != &ack["asset"] {
                return Err("Current local asset differs from immutable native receipt".into());
            }
            let owner = self.clone();
            let expected = tokio::task::spawn_blocking(move || owner.verify_finished())
                .await
                .map_err(err)??;
            if expected != *asset {
                return Err("Native receipt metadata differs from exact original request".into());
            }
            self.fence(current)?;
            db.fence()?;
            Ok(Some(json!({"state":"current","original_ack":ack})))
        } else {
            Ok(Some(json!({"state":"historical","original_ack":ack})))
        }
    }
    fn asset(&self, intent: &Intent, version: &Value) -> Value {
        let path = self
            .database
            .path
            .join("attachment-downloads")
            .join(&self.request.operation_id)
            .join("asset.bin");
        json!({"id":format!("native-download-{}",self.request.operation_id),"name":version["original_name"],"mime_type":version["mime_type"],"size":version["byte_length"],"url":format!("asset://localhost/{}",urlencoding::encode(&path.to_string_lossy())),"vault_path":path,"source":"tauri","space_id":self.request.association.space_id,"document_id":self.request.association.document_id,"created_at":intent.created_at,"server_version_provenance":{"format":1,"original_origin":self.request.origin,"raw_version_record":self.request.server_version_raw,"bytes_sha256":version["binary_sha256"]}})
    }
    // Additive delivery seam; existing ownership/retry/SQL/VFS code is unchanged.
    pub async fn delivery_operation<T:Send+'static,F>(&self,current:&Value,operation:F)->Result<T,String> where F:FnOnce(Self,super::transport::Snapshot)->Result<T,String>+Send+'static {
        let owner=self.clone();let current=current.clone();
        connection::supervise(async move {
            let _guard=snapshot_commit_lock().lock().await;
            owner.fence(&current)?;
            let mut db=connection::Database::prepare(owner.database.clone(),&owner.main).await?;
            let result=async {
                let prior=owner.resolve(&mut db,&current).await?.ok_or("Committed native download receipt required")?;
                if prior["state"]!="current" {return Err("Historical receipt cannot recreate readable asset".into());}
                let snapshot=read_snapshot_from_connection(db.connection(),owner.request.origin["provider"].as_str().unwrap()).await?.ok_or("Current native snapshot required")?;
                owner.require_association(&snapshot)?;
                let held_owner=owner.clone();
                let held=tokio::task::spawn_blocking(move || -> Result<super::transport::Snapshot,String> {
                    held_owner.verify_finished()?;
                    let version=held_owner.request.validate()?;
                    let dir=held_owner.directory(false)?;
                    let source=dir.open("asset.bin",libc::O_RDONLY).map_err(err)?;
                    let payload=dir.open("payload.bin",libc::O_RDONLY).map_err(err)?;
                    regular(&source,2)?;
                    if identity(&source).map_err(err)?!=identity(&payload).map_err(err)? {return Err("Native retained file group differs".into());}
                    let snapshot=super::transport::Snapshot::capture(&source,&dir.held,version["byte_length"].as_u64().unwrap(),version["binary_sha256"].as_str().unwrap()).map_err(err)?;
                    held_owner.verify_finished()?;dir.fence().map_err(err)?;
                    Ok(snapshot)
                }).await.map_err(err)??;
                owner.fence(&current)?;db.fence()?;
                let delivery_owner=owner.clone();
                let result=tokio::task::spawn_blocking(move ||operation(delivery_owner,held)).await.map_err(err)??;
                owner.fence(&current)?;db.fence()?;Ok(result)
            }.await;
            db.retire().await.map_err(|_|"Native delivery worker retirement did not complete".to_string())?;
            result
        }).await
    }
    pub fn delivery_fence(&self,current:&Value)->Result<(),String>{self.fence(current)}
    pub fn delivery_binding(&self)->Result<String,String>{Ok(digest(&serde_json::to_vec(&self.request).map_err(err)?))}
    pub async fn delivery_snapshot(&self,current:&Value)->Result<super::transport::Snapshot,String>{self.delivery_operation(current,|_,snapshot|Ok(snapshot)).await}
    fn verify_finished(&self) -> Result<Value, String> {
        let version = self.request.validate()?;
        let dir = self.directory(false)?;
        let raw = read_private(&dir, "intent.json", 65536)?.ok_or("Native intent missing")?;
        let intent = self.intent(&dir, &raw)?;
        let proof: Proof = serde_json::from_slice(
            &read_private(&dir, "install.json", 4096)?.ok_or("Native install proof missing")?,
        )
        .map_err(|_| "Native install proof malformed")?;
        let length = version["byte_length"].as_u64().unwrap();
        let sha = version["binary_sha256"].as_str().unwrap();
        if proof
            != (Proof {
                format: 1,
                intent_sha256: digest(&raw),
                payload: intent.payload,
                length,
                sha256: sha.into(),
            })
        {
            return Err("Native original install proof differs".into());
        }
        for name in ["payload.bin", "asset.bin"] {
            let f = dir.open(name, libc::O_RDONLY).map_err(err)?;
            regular(&f, 2)?;
            if identity(&f).map_err(err)? != intent.payload
                || f.metadata().map_err(err)?.nlink() != 2
                || hash_file(&f, length)? != sha
            {
                return Err("Native original immutable file binding differs".into());
            }
        }
        dir.fence().map_err(err)?;
        Ok(self.asset(&intent, &version))
    }
    async fn commit(
        &self,
        db: &mut connection::Database,
        current: &Value,
        asset: Value,
    ) -> Result<Value, String> {
        migrate_database_connection(db.connection()).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS native_attachment_download_receipts(operation_id TEXT PRIMARY KEY, request_sha256 TEXT NOT NULL, receipt TEXT NOT NULL)").execute(db.connection()).await.map_err(err)?;
        for statement in ["CREATE TRIGGER IF NOT EXISTS native_download_receipts_no_update BEFORE UPDATE ON native_attachment_download_receipts BEGIN SELECT RAISE(ABORT,'Native download receipts are immutable'); END","CREATE TRIGGER IF NOT EXISTS native_download_receipts_no_delete BEFORE DELETE ON native_attachment_download_receipts BEGIN SELECT RAISE(ABORT,'Native download receipts are immutable'); END"]{sqlx::query(statement).execute(db.connection()).await.map_err(err)?;}
        let request_sha = digest(&serde_json::to_vec(&self.request).map_err(err)?);
        let prior:Option<(String,String)>=sqlx::query_as("SELECT request_sha256,receipt FROM native_attachment_download_receipts WHERE operation_id=?1").bind(&self.request.operation_id).fetch_optional(db.connection()).await.map_err(err)?;
        let snapshot = read_snapshot_from_connection(
            db.connection(),
            self.request.origin["provider"].as_str().unwrap(),
        )
        .await?
        .unwrap_or_else(|| json!({}));
        if let Some((sha, raw)) = prior {
            if sha != request_sha {
                return Err("Native retry request differs".into());
            }
            let ack: Value = serde_json::from_str(&raw).map_err(|_| "Native receipt malformed")?;
            let current_asset = snapshot["assets"]
                .as_array()
                .and_then(|a| a.iter().find(|a| a["id"] == asset["id"]));
            self.fence(current)?;
            db.fence()?;
            return match current_asset {
                Some(a) if a == &asset && ack["asset"] == asset => {
                    Ok(json!({"state":"current","original_ack":ack}))
                }
                None => Ok(json!({"state":"historical","original_ack":ack})),
                _ => Err("Current local asset differs from immutable native receipt".into()),
            };
        }
        self.require_association(&snapshot)?;
        let mut snapshot = snapshot;
        let object = snapshot
            .as_object_mut()
            .ok_or("Native snapshot malformed")?;
        let assets = object
            .entry("assets")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Native asset collection malformed")?;
        if assets.iter().any(|a| a["id"] == asset["id"]) {
            return Err("Native immutable asset ID already exists without owner receipt".into());
        }
        assets.push(asset.clone());
        let fence = db.fence_owner();
        let mut tx = db
            .connection()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(err)?;
        let current_revision: Option<String> = sqlx::query_scalar(
            "SELECT value FROM local_vault_metadata WHERE key='canonical_revision'",
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(err)?;
        if current_revision.as_deref() != snapshot["canonical_revision"].as_str() {
            return Err("Native canonical snapshot changed before metadata commit".into());
        }
        let revision = write_snapshot_in_tx(
            &mut tx,
            self.request.origin["provider"].as_str().unwrap(),
            &snapshot,
        )
        .await?;
        let ack = json!({"format":1,"operation_id":self.request.operation_id,"asset":asset,"canonical_revision":revision,"canonical_committed":true,"mirror_pending":true});
        sqlx::query("INSERT INTO native_attachment_download_receipts(operation_id,request_sha256,receipt) VALUES(?1,?2,?3)").bind(&self.request.operation_id).bind(request_sha).bind(serde_json::to_string(&ack).map_err(err)?).execute(&mut *tx).await.map_err(err)?;
        let owner = self.clone();
        let final_asset = tokio::task::spawn_blocking(move || owner.verify_finished())
            .await
            .map_err(err)??;
        if final_asset != ack["asset"] {
            return Err("Native immutable metadata changed before commit".into());
        }
        self.fence(current)?;
        fence.check()?;
        tx.commit().await.map_err(err)?;
        let owner = self.clone();
        tokio::task::spawn_blocking(move || owner.verify_finished())
            .await
            .map_err(err)?
            .map_err(|e| format!("Native metadata committed; custody requires inspection: {e}"))?;
        self.fence(current)
            .map_err(|e| format!("Native metadata committed; original context changed: {e}"))?;
        fence.check()?;
        Ok(json!({"state":"current","original_ack":ack}))
    }
    fn install(&self) -> Result<Value, String> {
        let version = self.request.validate()?;
        let length = version["byte_length"].as_u64().unwrap();
        let sha = version["binary_sha256"].as_str().unwrap();
        let dir = self.directory(false)?;
        let raw = read_private(&dir, "intent.json", 65536)?.ok_or("Native intent missing")?;
        let intent = self.intent(&dir, &raw)?;
        let payload = dir.open("payload.bin", libc::O_RDWR).map_err(err)?;
        if payload.metadata().map_err(err)?.len() > length {
            return Err("Native payload has unexpected tail".into());
        }
        let mut index = 0;
        let mut offset = 0;
        while offset < length {
            let name = format!("part-{index}");
            let raw = read_private(&dir, &format!("{name}.json"), 4096)?
                .ok_or("Native payload chunk missing")?;
            let part: Part =
                serde_json::from_slice(&raw).map_err(|_| "Native part proof malformed")?;
            let f = dir
                .open(&format!("{name}.bin"), libc::O_RDONLY)
                .map_err(err)?;
            regular(&f, 1)?;
            let bytes = read_bounded(&f, CHUNK as u64)?;
            if part.index != index
                || part.identity != identity(&f).map_err(err)?
                || part.length != bytes.len() as u64
                || part.length != std::cmp::min(CHUNK as u64, length - offset)
                || part.sha256 != digest(&bytes)
            {
                return Err("Native exact chunk proof differs".into());
            }
            finish_prefix(&payload, &bytes, offset)?;
            offset += bytes.len() as u64;
            index += 1;
        }
        payload.sync_all().map_err(err)?;
        if hash_file(&payload, length)? != sha {
            return Err("Native downloaded bytes differ from exact server version".into());
        }
        let proof = Proof {
            format: 1,
            intent_sha256: digest(&raw),
            payload: intent.payload,
            length,
            sha256: sha.into(),
        };
        let proof_raw = serde_json::to_vec(&proof).map_err(err)?;
        match read_private(&dir, "install.json", 4096)? {
            Some(existing) if existing != proof_raw => {
                return Err("Native install proof differs".into())
            }
            Some(_) => {}
            None => persist(&dir, "install.json", &proof_raw)?,
        }
        match dir.open("asset.bin", libc::O_RDONLY) {
            Ok(f) => {
                regular(&f, 2)?;
                if identity(&f).map_err(err)? != intent.payload
                    || f.metadata().map_err(err)?.nlink() != 2
                {
                    return Err("Native final immutable asset identity differs".into());
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                regular(&payload, 1)?;
                if unsafe {
                    libc::linkat(
                        dir.held.as_raw_fd(),
                        c"payload.bin".as_ptr(),
                        dir.held.as_raw_fd(),
                        c"asset.bin".as_ptr(),
                        0,
                    )
                } != 0
                {
                    return Err(err(io::Error::last_os_error()));
                }
                dir.held.sync_all().map_err(err)?;
            }
            Err(e) => return Err(err(e)),
        }
        let final_file = dir.open("asset.bin", libc::O_RDONLY).map_err(err)?;
        if identity(&final_file).map_err(err)? != intent.payload
            || final_file.metadata().map_err(err)?.nlink() != 2
            || hash_file(&final_file, length)? != sha
        {
            return Err("Native exact final custody differs".into());
        }
        dir.fence().map_err(err)?;
        Ok(self.asset(&intent, &version))
    }
}
fn regular(f: &File, max_links: u64) -> Result<(), String> {
    let m = f.metadata().map_err(err)?;
    if !m.is_file() || m.nlink() < 1 || m.nlink() > max_links || m.mode() & 0o077 != 0 {
        return Err("Native owned file type/links/private mode invalid".into());
    }
    Ok(())
}
fn read_bounded(f: &File, limit: u64) -> Result<Vec<u8>, String> {
    let len = f.metadata().map_err(err)?.len();
    if len > limit {
        return Err("Native owned envelope exceeds bound".into());
    }
    let mut v = vec![0; len as usize];
    f.read_exact_at(&mut v, 0).map_err(err)?;
    let mut extra = [0];
    if f.read_at(&mut extra, len).map_err(err)? != 0 {
        return Err("Native owned envelope grew".into());
    }
    Ok(v)
}
fn read_private(dir: &Directory, name: &str, limit: u64) -> Result<Option<Vec<u8>>, String> {
    match dir.open(name, libc::O_RDONLY) {
        Ok(f) => {
            regular(&f, 1)?;
            Ok(Some(read_bounded(&f, limit)?))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(err(e)),
    }
}
fn persist(dir: &Directory, name: &str, bytes: &[u8]) -> Result<(), String> {
    let mut f = dir
        .open(name, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)
        .map_err(err)?;
    f.write_all(bytes).map_err(err)?;
    f.sync_all().map_err(err)?;
    dir.held.sync_all().map_err(err)?;
    dir.fence().map_err(err)
}
fn finish_prefix(f: &File, bytes: &[u8], offset: u64) -> Result<(), String> {
    let len = f.metadata().map_err(err)?.len();
    let end = offset
        .checked_add(bytes.len() as u64)
        .ok_or("Native payload span overflow")?;
    if len < offset {
        return Err("Native payload has a missing prefix".into());
    }
    let existing = std::cmp::min(len - offset, bytes.len() as u64) as usize;
    let mut prior = vec![0; existing];
    f.read_exact_at(&mut prior, offset).map_err(err)?;
    if prior != bytes[..existing] {
        return Err("Native interrupted prefix differs".into());
    }
    if len < end {
        f.write_all_at(&bytes[existing..], offset + existing as u64)
            .map_err(err)?;
    }
    Ok(())
}
fn hash_file(f: &File, length: u64) -> Result<String, String> {
    if f.metadata().map_err(err)?.len() != length {
        return Err("Native exact file length differs".into());
    }
    let mut h = Sha256::new();
    let mut bytes = [0u8; CHUNK];
    let mut offset = 0;
    while offset < length {
        let n = std::cmp::min(CHUNK as u64, length - offset) as usize;
        f.read_exact_at(&mut bytes[..n], offset).map_err(err)?;
        h.update(&bytes[..n]);
        offset += n as u64;
    }
    let mut extra = [0];
    if f.read_at(&mut extra, length).map_err(err)? != 0 {
        return Err("Native exact file grew".into());
    }
    Ok(format!("{:x}", h.finalize()))
}

// Read-only archive receipt proof; no download/import/replay/publication grant.
// The original server origin is checked as persisted provenance, not asserted
// to be a current authenticated server session.
pub(super) fn verify_archive_asset(selection:&vault::SnapshotRequestBinding,snapshot:&Value,asset:&Value,operation:&str,request_sha:&str,receipt:&str)->Result<String,String>{
    let path=selection.expected_database_path()?;
    let database=Directory::capture(path.parent().ok_or("Original database parent missing")?).map_err(err)?;
    let directory=database.child("attachment-downloads",false).and_then(|d|d.child(operation,false)).map_err(err)?;
    let raw=read_private(&directory,"intent.json",65536)?.ok_or("Original native archive intent missing")?;
    let intent:Intent=serde_json::from_slice(&raw).map_err(|_|"Original native archive intent malformed")?;
    let request=intent.request;
    if request.operation_id!=operation||digest(&serde_json::to_vec(&request).map_err(err)?)!=request_sha{return Err("Original archive request differs from SQL receipt".into());}
    let original=request.origin.clone();
    let owner=Captured::capture(selection.clone(),request,&original)?;
    let ack:Value=serde_json::from_str(receipt).map_err(|_|"Original archive SQL receipt malformed")?;
    if ack["asset"]!=*asset{return Err("Current archive asset differs from immutable SQL receipt".into());}
    owner.require_association(snapshot)?;
    if owner.verify_finished()?!=*asset{return Err("Original archive exact binary/intent metadata differs".into());}
    owner.fence(&original)?;Ok(digest(&raw))
}
