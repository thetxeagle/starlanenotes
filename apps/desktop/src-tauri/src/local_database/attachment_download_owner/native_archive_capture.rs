//! Canonical-only archive capture, separate from single-version delivery.
//! No chooser, destination token, command registration or JSON-mirror fallback.
use super::*;
use filesystem::{Directory,Identity,identity};
use sha2::{Digest,Sha256};
use sqlx::Connection;
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::{FileExt,MetadataExt};
use std::os::fd::AsRawFd;
const LIMIT:usize=16*1024*1024;
fn err(e:impl ToString)->String{e.to_string()}

pub(crate) struct ArchiveCapture {
    pub snapshot:Value,
    pub canonical_revision:String,
    pub raw_receipts:Vec<String>,
    pub binaries:Vec<Binary>,
    pub raw_imports:Vec<Value>,
    pub raw_local_covers:Vec<Value>,
    pub raw_local_imports:Vec<Value>,
    pub raw_local_dispositions:Vec<Value>,
    dispositions:local_cover::ArchiveDispositions,
    imported_history:Vec<super::native_archive_import::history_archive::Evidence>,
    imported_namespace:super::native_archive_import::history_archive::Namespace,
    original_owner:Owner,
}
impl ArchiveCapture {
    pub(super) fn history_requires_format4(&self)->bool{self.imported_history.iter().any(|held|held.requires_format4())}
    pub(super) fn output_binding(&self)->Value { serde_json::json!({"context":self.original_owner.original,"revision":self.canonical_revision,"root":self.original_owner.root.path}) }

    /// Mandatory synchronous source fence for a future owned publication step.
    /// This is not a destination/session token; renderer/session checks belong
    /// to the separate full-export owner before any publication authority.
    pub fn fence(&self)->Result<(),String>{
        self.original_owner.fence()?;self.dispositions.fence()?;self.imported_namespace.fence()?;for history in &self.imported_history{history.fence()?}
        for binary in &self.binaries{
            if let Some((operation,request_sha,receipt))=&binary.native_receipt{
                let actual=custody::verify_archive_asset(&self.original_owner.selection,&self.snapshot,&binary.asset,operation,request_sha,receipt)?;
                if binary.native_intent_sha256.as_deref()!=Some(&actual){return Err("Original archive private intent bytes changed".into());}
            }
            binary.copy_exact(&mut std::io::sink())?;
        }
        self.dispositions.fence()?;self.imported_namespace.fence()?;for history in &self.imported_history{history.fence()?}self.original_owner.fence()
    }
}
pub(crate) struct Binary {
    pub asset:Value,
    pub byte_length:u64,
    pub binary_sha256:String,
    parent:Directory,leaf:String,file:File,id:Identity,links:u64,
    pub native_intent_sha256:Option<String>,
    native_receipt:Option<(String,String,String)>,
    imported:Option<super::native_archive_import::CaptureEvidence>,
    local_cover:Option<local_cover::reader::CanonicalReader>,
    local_import:Option<local_import::reader::CanonicalReader>,
    canonical_import:Option<super::native_archive_import::canonical_archive::CanonicalEvidence>,
}
impl Binary {
    fn fence(&self)->Result<(),String>{
        if let Some(imported)=&self.imported{imported.reader.fence()?;}
        if let Some(imported)=&self.canonical_import{imported.fence()?;}
        if let Some(local)=&self.local_cover{local.fence()?;}
        if let Some(local)=&self.local_import{local.fence()?;}
        self.parent.fence().map_err(err)?;let m=self.file.metadata().map_err(err)?;
        if !m.is_file()||m.len()!=self.byte_length||m.nlink()!=self.links||identity(&self.file).map_err(err)?!=self.id||self.named_identity()?!=self.id{return Err("Original archive binary identity/length/links changed".into());}Ok(())
    }
    fn named_identity(&self)->Result<Identity,String>{
        // DB file_identity intentionally enforces one link. Binary custody has
        // a different exact link count; inspect without opening an unrelated
        // inode FD, which could release this process's SQLite POSIX locks.
        let leaf=std::ffi::CString::new(self.leaf.as_str()).map_err(err)?;
        let mut stat=std::mem::MaybeUninit::<libc::stat>::uninit();
        if unsafe{libc::fstatat(self.parent.held.as_raw_fd(),leaf.as_ptr(),stat.as_mut_ptr(),libc::AT_SYMLINK_NOFOLLOW)}!=0{return Err(err(std::io::Error::last_os_error()));}
        let stat=unsafe{stat.assume_init()};
        if stat.st_mode&libc::S_IFMT!=libc::S_IFREG||stat.st_nlink!=self.links{return Err("Original archive binary named links changed".into());}
        Ok(Identity{device:stat.st_dev,inode:stat.st_ino})
    }
    pub(super) fn local_import_provenance(&self)->Option<Value>{self.local_import.as_ref().map(|r|r.provenance())}
    pub(super) fn local_cover_provenance(&self)->Option<Value>{self.local_cover.as_ref().map(|r|r.provenance())}
    pub(super) fn imported_provenance(&self)->Option<&Value>{self.imported.as_ref().map(|e|&e.provenance).or_else(||self.canonical_import.as_ref().map(|e|&e.provenance))}
    pub fn copy_exact<W:Write>(&self,output:&mut W)->Result<(),String>{
        self.fence()?;let mut hash=Sha256::new();let mut offset=0;let mut buffer=[0u8;65536];
        while offset<self.byte_length{let limit=buffer.len().min((self.byte_length-offset) as usize);let n=self.file.read_at(&mut buffer[..limit],offset).map_err(err)?;if n==0{return Err("Original archive binary truncated".into());}hash.update(&buffer[..n]);output.write_all(&buffer[..n]).map_err(err)?;offset+=n as u64;}
        if format!("{:x}",hash.finalize())!=self.binary_sha256{return Err("Transferred archive binary digest differs".into());}self.fence()
    }
}
#[derive(Clone)]
pub(crate) struct Owner {
    selection:vault::SnapshotRequestBinding,root:Directory,database:Directory,main:String,
    database_id:Identity,provider:String,revision:String,original:Value,
}
impl Owner {
    /// Synchronous: capture initialized original physical selection before awaits.
    pub fn capture(selection:vault::SnapshotRequestBinding,provider:&str,revision:&str,original:Value)->Result<Self,String>{
        let context=original.as_object().ok_or("Original local archive context missing")?;
        if context.len()!=7||original["version"]!=1||!["serverBase","accountId","sessionId","cacheScope","storageMode","provider"].iter().all(|key|original[*key].as_str().is_some_and(|s|!s.is_empty()))||!matches!(provider,"none"|"icloud-drive"|"google-drive")||revision.is_empty()||revision.len()>256||original["provider"]!=provider||original["storageMode"]!="local"||serde_json::to_vec(&original).map_err(err)?.len()>65536{return Err("Original local archive context/revision required".into());}
        let path=selection.expected_database_path()?;let parent=path.parent().ok_or("Original database parent missing")?;
        let root=Directory::capture(parent.parent().ok_or("Original vault missing")?).map_err(err)?;let database=root.child(".starlane",false).map_err(err)?;
        let main=path.file_name().and_then(|s|s.to_str()).ok_or("Original database name invalid")?.to_string();
        let expected=if provider=="none"{"starlane-notes.sqlite3".into()}else{format!("starlane-vault-{provider}.sqlite3")};
        if main!=expected{return Err("Original archive provider/database differs".into());}
        let database_id=database.file_identity(&main).map_err(err)?.ok_or("Initialized original canonical database required")?;
        let owner=Self{selection,root,database,main,database_id,provider:provider.into(),revision:revision.into(),original};owner.fence()?;Ok(owner)
    }
    pub(super) fn binding(&self)->Value{json!({"context":self.original,"revision":self.revision,"root":self.root.path})}
    pub(super) fn delete_binding(&self)->Result<Value,String>{self.fence()?;Ok(json!({"context":self.original,"revision":self.revision,"root":self.root.path,"custody":{"root":identity(&self.root.held).map_err(err)?,"database_directory":identity(&self.database.held).map_err(err)?,"database":self.database_id}}))}
    pub(super) fn fence(&self)->Result<(),String>{self.selection.verify_selection()?;self.root.fence().map_err(err)?;self.database.fence().map_err(err)?;if self.database.file_identity(&self.main).map_err(err)?!=Some(self.database_id){return Err("Original archive database identity changed".into());}Ok(())}
    /// Internal consumer runs while actual VFS connection, SQL transaction and
    /// canonical mutex remain held. No renderer callback or ambient authority.
    pub async fn with_capture<T:Send+'static,F:FnOnce(&ArchiveCapture)->Result<T,String>+Send+'static>(self,current:Value,consume:F)->Result<T,String>{
        if current!=self.original{return Err("Original archive context changed".into());}self.fence()?;
        connection::supervise(async move {
            let _guard=snapshot_commit_lock().lock().await;self.fence()?;
            let mut db=connection::Database::prepare(self.database.clone(),&self.main).await?;let physical=db.fence_owner();
            let result=async {
                let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;
                strict_preflight(&mut tx).await?;
                let revision:Option<String>=sqlx::query_scalar("SELECT value FROM local_vault_metadata WHERE key='canonical_revision'").fetch_optional(&mut *tx).await.map_err(err)?;
                if revision.as_deref()!=Some(&self.revision){return Err("Original canonical archive revision changed".into());}
                let snapshot=read_snapshot_from_connection(&mut tx,&self.provider).await?.ok_or("Initialized canonical snapshot required")?;
                if snapshot["canonical_revision"]!=self.revision{return Err("Archive reconstruction revision differs".into());}
                let exists:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='native_attachment_download_receipts'").fetch_one(&mut *tx).await.map_err(err)?;
                let rows:Vec<(String,String,String)>=if exists==1{
                    let bytes:i64=sqlx::query_scalar("SELECT COALESCE(SUM(length(CAST(receipt AS BLOB))),0) FROM native_attachment_download_receipts").fetch_one(&mut *tx).await.map_err(err)?;
                    if bytes>LIMIT as i64{return Err("Archive receipts exceed metadata budget".into());}
                    sqlx::query_as("SELECT operation_id,request_sha256,receipt FROM native_attachment_download_receipts ORDER BY operation_id").fetch_all(&mut *tx).await.map_err(err)?
                }else{Vec::new()};
                let import_table:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='native_archive_import_receipts'").fetch_one(&mut *tx).await.map_err(err)?;
                let (import_instance,import_rows):(String,Vec<(String,String,String,String,String,String,String)>)=if import_table==1{
                    let total:i64=sqlx::query_scalar("SELECT COALESCE(SUM(length(CAST(i.request_json AS BLOB))+length(CAST(r.result_json AS BLOB))),0) FROM native_archive_import_intents i JOIN native_archive_import_receipts r ON r.operation_id=i.operation_id AND r.request_fingerprint=i.request_fingerprint").fetch_one(&mut *tx).await.map_err(err)?;
                    let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM native_archive_import_receipts").fetch_one(&mut *tx).await.map_err(err)?;
                    let intents:i64=sqlx::query_scalar("SELECT COUNT(*) FROM native_archive_import_intents").fetch_one(&mut *tx).await.map_err(err)?;if intents<count{return Err("Imported capture receipt chain contains an orphan or conflicting request".into())}if intents>count{return Err("Complete archive retains unresolved original joint import history; review first".into())}
                    if total>64*1024*1024||count>4096{return Err("Imported capture receipt chain exceeds portable metadata budget".into())}
                    let instance:String=sqlx::query_scalar("SELECT instance_id FROM native_archive_instance WHERE singleton=1").fetch_one(&mut *tx).await.map_err(err)?;
                    let rows:Vec<(String,String,String,String,String,String,String)>=sqlx::query_as("SELECT i.operation_id,i.request_fingerprint,i.provider,i.instance_id,i.request_json,r.result_json,r.result_digest FROM native_archive_import_intents i JOIN native_archive_import_receipts r ON r.operation_id=i.operation_id AND r.request_fingerprint=i.request_fingerprint ORDER BY i.operation_id").fetch_all(&mut *tx).await.map_err(err)?;if rows.len()!=count as usize{return Err("Imported capture receipt chain contains an orphan or conflicting request".into())}(instance,rows)
                }else{(String::new(),vec![])};
                // Original local creator authority is captured from the current
                // SQL transaction and held native Reader, never from asset JSON.
                let local_dispositions=local_cover::capture_disposition_archive(self.clone(),&snapshot,&mut tx).await?;
                let mut local_readers=std::collections::BTreeMap::new();
                let local_table:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='native_local_cover_intents'").fetch_one(&mut *tx).await.map_err(err)?;
                if local_table==1{
                    let (attempts,bytes):(i64,i64)=sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(length(CAST(request_json AS BLOB))+length(CAST(proof_json AS BLOB))),0) FROM native_local_cover_intents").fetch_one(&mut *tx).await.map_err(err)?;
                    let pending:i64=sqlx::query_scalar("SELECT COUNT(*) FROM native_local_cover_intents i LEFT JOIN native_local_cover_receipts r ON r.operation_id=i.operation_id AND r.request_sha256=i.request_sha256 WHERE r.operation_id IS NULL").fetch_one(&mut *tx).await.map_err(err)?;
                    if attempts>1024||bytes>8*1024*1024||pending!=local_dispositions.completed_count() as i64{return Err("Complete archive requires review of retained original local cover attempts".into())}
                    for asset in snapshot["assets"].as_array().ok_or("Canonical archive assets missing")?{
                        let Some(operation)=asset["native_local_version_provenance"]["producer_operation_id"].as_str() else{continue};
                        let own:i64=sqlx::query_scalar("SELECT COUNT(*) FROM native_local_cover_receipts WHERE operation_id=?1").bind(operation).fetch_one(&mut *tx).await.map_err(err)?;
                        if own==1{
                            let reader=local_cover::reader::capture_for_archive(&mut tx,self.clone(),&snapshot,asset).await?;
                            if local_readers.insert(asset["id"].as_str().ok_or("Exact local asset identity missing")?.to_string(),reader).is_some(){return Err("Duplicate original local archive asset".into())}
                        }
                    }
                    if local_readers.len()+local_dispositions.completed_count()!=attempts as usize{return Err("Complete archive requires all original local cover creator associations; retain evidence".into())}
                }
                let mut general_readers=std::collections::BTreeMap::new();
                let general_exists:i64=sqlx::query_scalar("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='native_local_import_intents'").fetch_one(&mut *tx).await.map_err(err)?;
                if general_exists==1{
                    let (count,bytes):(i64,i64)=sqlx::query_as("SELECT count(*),COALESCE(SUM(length(CAST(request_json AS BLOB))+length(CAST(proof_json AS BLOB))),0) FROM native_local_import_intents").fetch_one(&mut *tx).await.map_err(err)?;
                    let pending:i64=sqlx::query_scalar("SELECT count(*) FROM native_local_import_intents i LEFT JOIN native_local_import_receipts r ON r.operation_id=i.operation_id AND r.request_sha256=i.request_sha256 WHERE r.operation_id IS NULL").fetch_one(&mut *tx).await.map_err(err)?;
                    if count>1024||bytes>8*1024*1024||pending!=0{return Err("Complete archive requires review of original general import attempts".into())}
                    for asset in snapshot["assets"].as_array().ok_or("Canonical assets missing")?{
                        if let Some(operation)=asset["id"].as_str().and_then(|id|id.strip_prefix("asset-import-")){
                            let own:i64=sqlx::query_scalar("SELECT count(*) FROM native_local_import_receipts WHERE operation_id=?1").bind(operation).fetch_one(&mut *tx).await.map_err(err)?;
                            if own==1{let reader=local_import::reader::capture_for_archive(&mut tx,self.clone(),&snapshot,asset).await?;if general_readers.insert(asset["id"].as_str().unwrap().to_owned(),reader).is_some(){return Err("Duplicate original general import asset".into())}}
                        }
                    }
                    if general_readers.len()!=count as usize{return Err("Complete archive requires every original general import receipt and byte family".into())}
                }
                self.fence()?;physical.check()?;
                let expected_revision=self.revision.clone();
                let value=tokio::task::spawn_blocking(move ||{
                    let capture=self.binaries(snapshot,rows,&import_instance,&import_rows,local_readers,general_readers,local_dispositions,physical.clone())?;capture.fence()?;self.fence()?;physical.check()?;
                    let output=consume(&capture)?;
                    capture.fence()?;
                    self.fence()?;physical.check()?;Ok(output)
                }).await.map_err(err)?;
                let final_revision:Option<String>=sqlx::query_scalar("SELECT value FROM local_vault_metadata WHERE key='canonical_revision'").fetch_optional(&mut *tx).await.map_err(err)?;
                if final_revision.as_deref()!=Some(&expected_revision){return Err("Canonical archive revision changed through consumer fence".into());}
                tx.rollback().await.map_err(err)?;value
            }.await;
            db.retire().await.map_err(|e|format!("Archive capture connection retirement failed: {e}"))?;result
        }).await
    }
    /// Narrow typed deletion with actual original descriptor custody held through SQL commit.
    pub(super) async fn delete_original_trash(self,current:Value,request:super::native_document_delete::Request)->Result<Value,String>{
        request.validate()?;if current!=self.original||request.original_binding!=self.delete_binding()?{return Err("Original native deletion context/binding changed".into())}self.fence()?;
        connection::supervise(async move {
            let _guard=snapshot_commit_lock().lock().await;self.fence()?;
            let mut db=connection::Database::prepare(self.database.clone(),&self.main).await?;let physical=db.fence_owner();
            let result=async {
                let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;
                strict_preflight(&mut tx).await?;self.fence()?;physical.check()?;
                let creator=json!({"root":identity(&self.root.held).map_err(err)?,"database_directory":identity(&self.database.held).map_err(err)?,"database":self.database_id});
                let request_raw=serde_json::to_string(&request).map_err(err)?;let request_sha=format!("{:x}",Sha256::digest(request_raw.as_bytes()));
                let exists:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='native_document_delete_receipts'").fetch_one(&mut *tx).await.map_err(err)?;
                let (retained_count,retained_bytes):(i64,i64)=if exists==1{sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(length(CAST(receipt_json AS BLOB))),0) FROM native_document_delete_receipts").fetch_one(&mut *tx).await.map_err(err)?}else{(0,0)};
                if retained_count>4096||retained_bytes>64*1024*1024{return Err("Native deletion receipt history exceeds bound; retain vault for review".into())}
                let prior:Option<(String,String)>=if exists==1{sqlx::query_as("SELECT request_sha256,receipt_json FROM native_document_delete_receipts WHERE operation_id=?1").bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(err)?}else{None};
                let mut snapshot=read_snapshot_from_connection(&mut tx,&self.provider).await?.ok_or("Initialized canonical snapshot required")?;
                if let Some((sha,raw))=prior{
                    if raw.len()>65536||sha!=request_sha{return Err("Original native deletion receipt/request differs".into())}
                    let receipt:Value=serde_json::from_str(&raw).map_err(|_|"Original deletion receipt malformed")?;
                    if receipt.as_object().map(|o|o.len())!=Some(6)||!receipt["canonical_revision"].as_str().is_some_and(|s|!s.is_empty()&&s.len()<=256)||receipt["format"]!=1||receipt["request"]!=serde_json::to_value(&request).map_err(err)?||receipt["creator"]!=creator||receipt["canonical_committed"]!=true||receipt["operation_id"]!=request.operation_id||snapshot["documents"].as_array().ok_or("Canonical documents missing")?.iter().any(|p|p["id"]==request.document_id){return Err("Original native receipt cannot own a replacement page/database".into())}
                    self.fence()?;physical.check()?;tx.rollback().await.map_err(err)?;
                    return Ok(json!({"format":1,"receipt":raw,"canonical_committed":true,"operation_id":request.operation_id,"reconciled":true,"snapshot":snapshot}));
                }
                if retained_count>=4096{return Err("Native deletion receipt limit reached; retain original page for review".into())}
                if snapshot["canonical_revision"]!=self.revision{return Err("Original native deletion canonical revision changed".into())}
                request.mutate(&mut snapshot)?;
                if serde_json::to_vec(&snapshot).map_err(err)?.len()>LIMIT{return Err("Native deletion canonical candidate exceeds bound".into())}
                let revision=write_snapshot_in_tx(&mut tx,&self.provider,&snapshot).await?;
                let pending:Option<String>=sqlx::query_scalar("SELECT value FROM local_vault_metadata WHERE key='mirror_pending'").fetch_optional(&mut *tx).await.map_err(err)?;if pending.as_deref()!=Some(&revision){return Err("Native deletion mirror fence differs".into())}
                sqlx::query("CREATE TABLE IF NOT EXISTS native_document_delete_receipts(operation_id TEXT PRIMARY KEY,request_sha256 TEXT NOT NULL,receipt_json TEXT NOT NULL)").execute(&mut *tx).await.map_err(err)?;
                for statement in ["CREATE TRIGGER IF NOT EXISTS native_document_delete_no_update BEFORE UPDATE ON native_document_delete_receipts BEGIN SELECT RAISE(ABORT,'Native deletion receipts are immutable'); END","CREATE TRIGGER IF NOT EXISTS native_document_delete_no_delete BEFORE DELETE ON native_document_delete_receipts BEGIN SELECT RAISE(ABORT,'Native deletion receipts are immutable'); END"]{sqlx::query(statement).execute(&mut *tx).await.map_err(err)?;}
                let receipt=json!({"format":1,"request":request,"creator":creator,"canonical_committed":true,"operation_id":request.operation_id,"canonical_revision":revision});
                let raw=serde_json::to_string(&receipt).map_err(err)?;if raw.len()>65536||retained_bytes+raw.len() as i64>64*1024*1024{return Err("Native deletion receipt exceeds retained budget".into())}
                sqlx::query("INSERT INTO native_document_delete_receipts(operation_id,request_sha256,receipt_json) VALUES(?1,?2,?3)").bind(&request.operation_id).bind(request_sha).bind(&raw).execute(&mut *tx).await.map_err(err)?;
                self.fence()?;physical.check()?;tx.commit().await.map_err(err)?;
                snapshot["canonical_revision"]=json!(revision);snapshot["mirror_pending"]=json!(true);
                Ok(json!({"format":1,"receipt":raw,"canonical_committed":true,"operation_id":request.operation_id,"reconciled":false,"snapshot":snapshot}))
            }.await;db.retire().await.map_err(|e|format!("Native deletion connection retirement failed: {e}"))?;result
        }).await
    }
    fn binaries(&self,snapshot:Value,rows:Vec<(String,String,String)>,import_instance:&str,import_rows:&[(String,String,String,String,String,String,String)],mut local_readers:std::collections::BTreeMap<String,local_cover::reader::CanonicalReader>,mut general_readers:std::collections::BTreeMap<String,local_import::reader::CanonicalReader>,dispositions:local_cover::ArchiveDispositions,physical:super::vfs::NativeVfs)->Result<ArchiveCapture,String>{
        let mut receipts=std::collections::BTreeMap::new();let mut raw_receipts=Vec::new();
        for (operation,request_sha,raw) in rows{
            if raw.len()>65536{return Err("Original archive receipt exceeds private budget".into());}
            let ack:Value=serde_json::from_str(&raw).map_err(|_|"Archive immutable receipt malformed")?;
            if ack["format"]!=1||ack["operation_id"]!=operation||ack["canonical_committed"]!=true||!ack["asset"].is_object()||ack["canonical_revision"].as_str().filter(|s|!s.is_empty()).is_none(){return Err("Archive immutable receipt invalid".into());}
            let asset=&ack["asset"];let provenance=&asset["server_version_provenance"];
            let version_raw=provenance["raw_version_record"].as_str().filter(|v|v.len()<=65536).ok_or("Archive receipt raw version missing")?;
            let version:Value=serde_json::from_str(version_raw).map_err(|_|"Archive receipt raw version malformed")?;
            let valid_sha=|s:&str|s.len()==64&&s.bytes().all(|v|v.is_ascii_digit()||(b'a'..=b'f').contains(&v));
            if !valid_sha(&request_sha)||asset["id"]!=format!("native-download-{operation}")||asset["source"]!="tauri"||provenance["format"]!=1||!version.is_object()||version["byte_length"]!=asset["size"]||version["binary_sha256"]!=provenance["bytes_sha256"]||!version["binary_sha256"].as_str().is_some_and(valid_sha){return Err("Archive receipt exact version/asset binding malformed".into());}
            raw_receipts.push(raw.clone());receipts.insert(operation,(request_sha,raw));
        }
        let assets=snapshot["assets"].as_array().ok_or("Canonical archive assets missing")?;
        if assets.len()>4096||serde_json::to_vec(&snapshot).map_err(err)?.len()>LIMIT{return Err("Canonical archive exceeds capture budget".into());}
        let mut binaries=Vec::new();let mut raw_local_covers=Vec::new();let mut raw_local_imports=Vec::new();let mut raw_imports=std::collections::BTreeMap::new();let imported_namespace=super::native_archive_import::history_archive::namespace(self.selection.clone(),import_rows,physical.clone())?;let imported_history=super::native_archive_import::capture_history(self.selection.clone(),&self.provider,self.original.clone(),import_instance,import_rows,physical)?;for held in &imported_history{let evidence=held.provenance();let op=evidence["operation_id"].as_str().ok_or("Imported historical operation missing")?.to_string();raw_imports.insert(op,evidence);}let mut ids=std::collections::BTreeSet::new();
        for asset in assets{
            let id=asset["id"].as_str().filter(|s|!s.is_empty()).ok_or("Canonical archive asset ID missing")?;if !ids.insert(id){return Err("Duplicate canonical archive asset ID".into());}
            if asset["source"]!="tauri"{return Err("Complete native archive requires genuine custody for every binary".into());}
            let path=Path::new(asset["vault_path"].as_str().ok_or("Canonical archive source path missing")?);
            if !id.starts_with("native-download-") && !path.starts_with(self.root.path.join("assets")){return Err("Legacy archive asset must remain in original owned assets directory".into());}
            self.fence()?;
            let relative=path.strip_prefix(&self.root.path).map_err(|_|"Archive source outside original vault refused")?;
            let parts:Vec<_>=relative.components().collect();let mut parent=self.root.clone();
            for part in parts.iter().take(parts.len().saturating_sub(1)){let name=match part{std::path::Component::Normal(v)=>v.to_str().ok_or("Archive source encoding invalid")?,_=>return Err("Archive source traversal refused".into())};parent=parent.child(name,false).map_err(err)?;}
            let leaf=match parts.last(){Some(std::path::Component::Normal(v))=>v.to_str().ok_or("Archive source encoding invalid")?.to_string(),_=>return Err("Archive source leaf invalid".into())};
            let file=parent.open(&leaf,libc::O_RDONLY).map_err(err)?;let metadata=file.metadata().map_err(err)?;
            let length=asset["size"].as_u64().filter(|n|*n<=100*1024*1024).ok_or("Canonical archive asset length invalid")?;
            let mut native_intent_sha256=None;let mut native_receipt=None;let mut imported=None;let mut canonical_import=None;let local_cover=local_readers.remove(id);let local_import=general_readers.remove(id);
            let links=if let Some(local)=&local_cover{
                local.fence()?;if local.byte_length()!=length{return Err("Original local archive length differs".into())}raw_local_covers.push(local.provenance());2
            }else if let Some(local)=&local_import{
                local.fence()?;if local.byte_length()!=length{return Err("Original general import archive length differs".into())}raw_local_imports.push(local.provenance());2
            }else if id.starts_with("asset-import-"){
                let evidence=super::native_archive_import::canonical_archive::try_capture(self.selection.clone(),&self.provider,self.original.clone(),&snapshot,import_instance,import_rows,asset)?.ok_or("Original general import requires its current native receipt or fresh destination import witness")?;
                let op=evidence.provenance["operation_id"].as_str().ok_or("General imported original operation missing")?.to_string();raw_imports.insert(op,evidence.provenance.clone());canonical_import=Some(evidence);2
            }else if id.starts_with("native-cover-")||!asset["native_local_version_provenance"].is_null(){
                let evidence=super::native_archive_import::capture_archive_evidence(self.selection.clone(),&self.provider,self.original.clone(),&snapshot,import_instance,import_rows,asset)?;
                let op=evidence.provenance["operation_id"].as_str().ok_or("Imported local cover operation missing")?.to_string();raw_imports.insert(op,evidence.provenance.clone());imported=Some(evidence);2
            }else if let Some(operation)=id.strip_prefix("native-download-"){
                if let Some((request_sha,receipt))=receipts.get(operation).filter(|(_,raw)|serde_json::from_str::<Value>(raw).ok().is_some_and(|ack|ack["asset"]==*asset)){
                    native_intent_sha256=Some(custody::verify_archive_asset(&self.selection,&snapshot,asset,operation,request_sha,receipt)?);native_receipt=Some((operation.to_string(),request_sha.clone(),receipt.clone()));
                }else{
                    let evidence=super::native_archive_import::capture_archive_evidence(self.selection.clone(),&self.provider,self.original.clone(),&snapshot,import_instance,import_rows,asset)?;
                    let op=evidence.provenance["operation_id"].as_str().ok_or("Imported capture original operation missing")?.to_string();raw_imports.insert(op,evidence.provenance.clone());imported=Some(evidence);
                }2
            }else if let Some(evidence)=super::native_archive_import::canonical_archive::try_capture(self.selection.clone(),&self.provider,self.original.clone(),&snapshot,import_instance,import_rows,asset)?{
                let op=evidence.provenance["operation_id"].as_str().ok_or("Canonical imported operation missing")?.to_string();raw_imports.insert(op,evidence.provenance.clone());canonical_import=Some(evidence);2
            }else{1};
            if !metadata.is_file()||metadata.len()!=length||metadata.nlink()!=links{return Err("Original archive asset file shape differs".into());}
            let mut hash=Sha256::new();let mut buffer=[0u8;65536];let mut offset=0;
            while offset<length{let limit=buffer.len().min((length-offset) as usize);let n=file.read_at(&mut buffer[..limit],offset).map_err(err)?;if n==0{return Err("Original archive binary truncated".into());}hash.update(&buffer[..n]);offset+=n as u64;}
            let binary_sha256=format!("{:x}",hash.finalize());
            if id.starts_with("native-cover-")&&asset["native_local_version_provenance"]["binary_sha256"]!=binary_sha256{return Err("Original local archive exact bytes differ".into());}
            if id.starts_with("native-download-")&&asset["server_version_provenance"]["bytes_sha256"]!=binary_sha256{return Err("Original archive exact receipt bytes differ".into());}
            let binary=Binary{asset:asset.clone(),byte_length:length,binary_sha256,parent,leaf,id:identity(&file).map_err(err)?,file,links,native_intent_sha256,native_receipt,imported,local_cover,local_import,canonical_import};binary.fence()?;binaries.push(binary);
        }
        self.fence()?;Ok(ArchiveCapture{snapshot,canonical_revision:self.revision.clone(),raw_receipts,binaries,raw_imports:raw_imports.into_values().collect(),raw_local_covers,raw_local_imports,raw_local_dispositions:dispositions.provenance(),dispositions,imported_history,imported_namespace,original_owner:self.clone()})
    }
}
pub(super) async fn strict_preflight(connection:&mut sqlx::SqliteConnection)->Result<(),String>{
    let schema:Option<String>=sqlx::query_scalar("SELECT value FROM local_vault_metadata WHERE key='schema_version'").fetch_optional(&mut *connection).await.map_err(err)?;if schema.as_deref()!=Some("1"){return Err("Initialized canonical archive schema required".into());}
    let tables:Vec<String>=sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'local_%' ORDER BY name").fetch_all(&mut *connection).await.map_err(err)?;let mut total=0i64;
    for table in tables{
        let quoted=format!("\"{}\"",table.replace('"',"\"\""));
        let columns:Vec<String>=sqlx::query(&format!("PRAGMA table_info({quoted})")).fetch_all(&mut *connection).await.map_err(err)?.into_iter().map(|r|r.get("name")).collect();if !columns.iter().any(|s|s=="payload"){continue;}
        let (bytes,count):(i64,i64)=sqlx::query_as(&format!("SELECT COALESCE(SUM(length(CAST(payload AS BLOB))),0),COUNT(*) FROM {quoted}")).fetch_one(&mut *connection).await.map_err(err)?;total=total.checked_add(bytes).ok_or("Archive metadata overflow")?;if total>LIMIT as i64||count>65536{return Err("Archive metadata exceeds budget".into());}
        let values:Vec<String>=sqlx::query_scalar(&format!("SELECT payload FROM {quoted}")).fetch_all(&mut *connection).await.map_err(err)?;
        for raw in values{let value:Value=serde_json::from_str(&raw).map_err(|_|"Malformed canonical archive payload; retain vault")?;if !value.is_object(){return Err("Canonical archive payload is not an object".into());}}
    }
    for key in ["sync_state","maintenance_target_catalogs"]{let raw:Option<String>=sqlx::query_scalar("SELECT value FROM local_vault_metadata WHERE key=?1").bind(key).fetch_optional(&mut *connection).await.map_err(err)?;if let Some(raw)=raw{if raw.len()>LIMIT{return Err("Archive state metadata exceeds budget".into());}serde_json::from_str::<Value>(&raw).map_err(|_|"Malformed canonical archive state metadata")?;}}
    Ok(())
}
#[cfg(test)]pub(crate) mod tests;

pub(crate) mod local_cover;

/// Linux-only original general file importer; cover wire is unchanged.
#[cfg(target_os="linux")] pub(crate) mod local_import;
