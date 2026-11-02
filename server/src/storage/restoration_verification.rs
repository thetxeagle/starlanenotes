// Original live target custody for a dedicated verification producer.
// This grants no serving, import, cleanup or restore authority.
pub(crate) struct VerificationReservation {
    pool: sqlx::AnyPool,
    conn: sqlx::pool::PoolConnection<sqlx::Any>,
}
impl VerificationReservation {
    pub(crate) async fn begin(pool:&sqlx::AnyPool)->anyhow::Result<Self> {
        anyhow::ensure!(matches!(crate::db::DatabaseEngine::from_url(pool.connect_options().database_url.as_str())?,crate::db::DatabaseEngine::Sqlite),"SQLite original pool required");
        let mut conn=pool.acquire().await?;
        // Cancellation drops this original connection instead of returning a
        // manual transaction to the pool. SQLite closes/rolls back its lock.
        conn.close_on_drop();
        sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
        Ok(Self{pool:pool.clone(),conn})
    }
    pub(crate) fn connection(&mut self)->&mut sqlx::AnyConnection {&mut self.conn}
    pub(crate) async fn rollback(mut self)->anyhow::Result<()> {
        sqlx::query("ROLLBACK").execute(&mut *self.conn).await?;
        // close_on_drop detaches pooled size before the SQLite worker finishes
        // native xClose. Await the real worker acknowledgment while this caller
        // still owns every custom VFS and method table. Cancellation retains the
        // enclosing VFS guard; no detached callback can outlive freed code.
        self.conn.close().await?;Ok(())
    }
}
pub(crate) struct VerificationTarget<'a> {
    bundle: &'a crate::storage::backup_inventory::ValidatedBundle,
    pool: sqlx::AnyPool,
    pool_source: Arc<crate::storage::recovery_records::PoolSource>,
    active: Arc<Active>,
    database: File,
    database_identity: Identity,
    database_stamp: PhysicalStamp,
    root_stamp: PhysicalStamp,
    pager_sidecars: Vec<VerificationPagerSidecar>,
    rows: Vec<HistorySqlRow>,
    pointer: Vec<(i64,String)>,
}
struct VerificationPagerSidecar {relative:&'static str,held:Option<File>,stamp:Option<PhysicalStamp>,bytes_sha256:Option<String>}
impl VerificationPagerSidecar {
    fn hold(root:&Root,relative:&'static str)->io::Result<Self> {
        if !exists(root.fd(),relative)? {return Ok(Self{relative,held:None,stamp:None,bytes_sha256:None});}
        let held=root.leaf(relative)?;let md=held.metadata()?;
        if !md.is_file()||md.nlink()!=1{return Err(fail("Verification pager sidecar is not original regular data"));}
        let stamp=physical_stamp(&held)?;
        // SHM is transient reader coordination. Bind its actual object, not its
        // changing read-mark bytes; WAL/journal contents remain immutable here.
        let bytes_sha256=if relative.ends_with("-shm"){None}else{Some(hash_file(&held,md.len())?)};
        if physical_stamp(&held)?!=stamp{return Err(fail("Verification pager sidecar raced"));}
        Ok(Self{relative,held:Some(held),stamp:Some(stamp),bytes_sha256})
    }
    fn fence(&self,root:&Root)->io::Result<()> {
        match (&self.held,&self.stamp) {
            (None,None)=>if exists(root.fd(),self.relative)?{return Err(fail("Verification pager sidecar unexpectedly appeared"));},
            (Some(held),Some(stamp))=>{
                let fresh=root.leaf(self.relative)?;
                if identity(held)?!=(stamp.device,stamp.inode)||identity(&fresh)?!=(stamp.device,stamp.inode){return Err(fail("Verification pager sidecar exchanged"));}
                if let Some(hash)=&self.bytes_sha256 {
                    if physical_stamp(held)?!=*stamp||physical_stamp(&fresh)?!=*stamp||hash_file(held,stamp.length)?!=*hash||hash_file(&fresh,stamp.length)?!=*hash{return Err(fail("Verification pager content changed"));}
                }
            },
            _=>return Err(fail("Malformed internal verification pager fence")),
        }Ok(())
    }
}
impl VerificationTarget<'_> {
    pub(crate) fn generation_id(&self)->&str {&self.active.ledger.generation_id}
    pub(crate) fn backup_id(&self)->&str {&self.active.ledger.backup_id}
    pub(crate) fn manifest_sha256(&self)->&str {&self.active.ledger.manifest_sha256}
    pub(crate) fn ledger_sha256(&self)->&str {&self.active.ledger_sha256}
    pub(crate) fn bundle(&self)->&crate::storage::backup_inventory::ValidatedBundle {self.bundle}
 pub(crate) fn path(&self)->&Path {&self.active.root.absolute}
    pub(crate) fn root_identity(&self)->io::Result<Identity> {identity(self.active.root.fd())}
    pub(crate) fn database_identity(&self)->Identity {self.database_identity}
    /// Called on the original SQL reservation, including after every awaited
    /// application probe and immediately before publishing completion evidence.
    pub(crate) async fn fence(&self,reservation:&mut VerificationReservation)->anyhow::Result<()> {
        anyhow::ensure!(Arc::ptr_eq(&self.pool.connect_options(),&reservation.pool.connect_options()),"Verification reservation belongs to another pool");
        let conn=reservation.connection();
        self.bundle.fence()?;self.active.check()?;
        anyhow::ensure!(physical_stamp(self.active.root.fd())?==self.root_stamp,"Original verification target directory changed");
        let original=crate::storage::recovery_records::pool_source(&self.pool,self.path())
            .ok_or_else(||fail("Original verification pool source unavailable"))?;
        anyhow::ensure!(Arc::ptr_eq(&original,&self.pool_source),"Verification pool source replaced");
        for sidecar in &self.pager_sidecars {sidecar.fence(&self.active.root)?;}
        let fresh=self.active.root.leaf("starlane-notes.db")?;
        anyhow::ensure!(identity(&fresh)?==self.database_identity&&identity(&self.database)?==self.database_identity,
            "Original verification database identity changed");
        anyhow::ensure!(physical_stamp(&fresh)?==self.database_stamp&&physical_stamp(&self.database)?==self.database_stamp,"Verification original database bytes/metadata changed");
        let current=history_rows(conn).await?;
        anyhow::ensure!(current.0==self.rows&&current.1==self.pointer,"Verification SQL generation changed");
        let closure=history_closure(HistorySource::new(&self.active.root,None)?,identity(self.active.root.fd())?,Some((&current.0,&current.1)))?;
        anyhow::ensure!(closure.current_id==self.generation_id(),"Verification generation retargeted");
        closure.source.fence()?;self.active.check()?;self.bundle.fence()?;
        // Repeat original native source after the final SQL await.
        anyhow::ensure!(crate::storage::recovery_records::pool_source(&self.pool,self.path())
            .is_some_and(|p|Arc::ptr_eq(&p,&self.pool_source)),"Verification source changed after SQL wait");
        for sidecar in &self.pager_sidecars {sidecar.fence(&self.active.root)?;}
        anyhow::ensure!(physical_stamp(&self.database)?==self.database_stamp,"Verification database changed after SQL wait");
        anyhow::ensure!(physical_stamp(self.active.root.fd())?==self.root_stamp,"Original verification target directory changed after SQL wait");
        Ok(())
    }
}
/// Only a genuinely activated finalized SQLite target of this exact authenticated
/// backup is eligible. A matching path, copied ledger or row alone is insufficient.
pub(crate) async fn verification_target<'a>(
    bundle:&'a crate::storage::backup_inventory::ValidatedBundle,
    path:&Path,pool:&sqlx::AnyPool,reservation:&mut VerificationReservation,
)->anyhow::Result<VerificationTarget<'a>> {
    anyhow::ensure!(Arc::ptr_eq(&pool.connect_options(),&reservation.pool.connect_options()),"Original verification reservation required");
    let conn=reservation.connection();
    bundle.fence()?;
    let source_manifest:serde_json::Value=serde_json::from_slice(&bundle.raw_manifest)?;
    anyhow::ensure!(source_manifest["database"]["engine"]=="sqlite","SQLite verification source required");
    anyhow::ensure!(source_manifest["format_version"]==4,"Standard current capture required");
    let (_,active)=registered(path)?;
    let active=active.ok_or_else(||fail("Genuine activated restored target required"))?;
    anyhow::ensure!(active.engine.is_none(),"Engine-transfer verification requires a separate qualified producer");
    anyhow::ensure!(active.version2,"Finalized complete restoration history required");
    let ledger=&active.ledger;
    anyhow::ensure!(ledger.backup_id==bundle.manifest.backup_id
        &&ledger.manifest_sha256==digest(&bundle.raw_manifest)
        &&ledger.database_sha256==bundle.manifest.files.0.get("starlane-notes.db")
            .ok_or_else(||fail("Source database inventory absent"))?.sha256,
        "Restored target belongs to a different original source");
    let pool_source=crate::storage::recovery_records::pool_source(pool,&active.root.absolute)
        .ok_or_else(||fail("Target pool was not bound to this original root/database at initialization"))?;
    let database=active.root.leaf("starlane-notes.db")?;
    let md=database.metadata()?;
    anyhow::ensure!(md.is_file()&&md.nlink()==1,"Verification database is not a single original regular file");
    let database_identity=identity(&database)?;
    let database_stamp=physical_stamp(&database)?;
    let root_stamp=physical_stamp(active.root.fd())?;
    let pager_sidecars=["starlane-notes.db-wal","starlane-notes.db-shm","starlane-notes.db-journal"].into_iter().map(|name|VerificationPagerSidecar::hold(&active.root,name)).collect::<io::Result<Vec<_>>>()?;
    let (rows,pointer)=history_rows(conn).await?;
    let closure=history_closure(HistorySource::new(&active.root,None)?,identity(active.root.fd())?,Some((&rows,&pointer)))?;
    anyhow::ensure!(closure.current_id==ledger.generation_id,"Original verification pointer differs");
    let current=&closure.generations[&closure.current_id];
    anyhow::ensure!(current.envelope.as_ref().is_some_and(|e|e.version==3&&e.finalization.is_some()),
        "Actual finalized publication required");
    anyhow::ensure!(current.source_root.as_ref().is_some_and(|id|id.device==bundle.manifest.source_custody.root.device&&id.inode==bundle.manifest.source_custody.root.inode),
        "Original captured source root differs from target history");
    closure.source.fence()?;
    let mut original_sql=bundle.db.acquire().await?;
    let (original_rows,_)=history_rows(&mut original_sql).await?;
    anyhow::ensure!(rows.len()==original_rows.len()+1&&original_rows.iter().all(|old|rows.iter().any(|new|new==old)),
        "Original historical restoration SQL changed");
    anyhow::ensure!(!original_rows.iter().any(|old|old.generation_id==ledger.generation_id),"Target generation is not this fresh original restore");
    drop(original_sql);bundle.fence()?;
    let target=VerificationTarget{bundle,pool:pool.clone(),pool_source,active,database,database_identity,database_stamp,root_stamp,pager_sidecars,rows,pointer};
    target.fence(reservation).await?;Ok(target)
}

#[cfg(test)]
mod verification_custody_tests {
 use super::*;
 #[test]
 fn pager_fences_refuse_equal_content_inode_and_missing_to_present() -> anyhow::Result<()> {
  use std::os::unix::fs::PermissionsExt;
  let path=std::env::temp_dir().join(format!("verification-pager-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&path)?;std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o700))?;
  let root=Root::open(&path)?;sqlite_capture_write(&root,"starlane-notes.db-wal",b"original exact WAL bytes")?;
  let held=VerificationPagerSidecar::hold(&root,"starlane-notes.db-wal")?;held.fence(&root)?;
  std::fs::rename(path.join("starlane-notes.db-wal"),path.join("original-kept"))?;
  sqlite_capture_write(&root,"starlane-notes.db-wal",b"original exact WAL bytes")?;
  assert!(held.fence(&root).is_err(),"Byte-identical foreign inode cannot renew original pager authority");
  let missing=VerificationPagerSidecar::hold(&root,"starlane-notes.db-journal")?;missing.fence(&root)?;
  sqlite_capture_write(&root,"starlane-notes.db-journal",b"new pager")?;
  assert!(missing.fence(&root).is_err(),"Prior absence alone cannot admit later pager content");
  std::fs::remove_dir_all(path)?;Ok(())
 }
 #[tokio::test]
 async fn original_target_reservation_excludes_writer_and_cancel_closes_lock() -> anyhow::Result<()> {
  sqlx::any::install_default_drivers();
  let path=std::env::temp_dir().join(format!("verification-reservation-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&path)?;
  let pool=sqlx::any::AnyPoolOptions::new().max_connections(2).connect(&format!("sqlite://{}?mode=rwc",path.join("original.db").display())).await?;
  sqlx::query("CREATE TABLE original(value TEXT)").execute(&pool).await?;
  sqlx::query("INSERT INTO original VALUES('original')").execute(&pool).await?;
  let held=VerificationReservation::begin(&pool).await?;
  let mut writer=pool.acquire().await?;sqlx::query("PRAGMA busy_timeout=0").execute(&mut *writer).await?;
  assert!(sqlx::query("UPDATE original SET value='foreign'").execute(&mut *writer).await.is_err(),"Actual writer excluded while original verification reservation held");
  drop(held);
  // PoolConnection drop closes this physical connection asynchronously. Waiting
  // for a fresh BEGIN is bounded; no commit or writes were made by the verifier.
  tokio::time::timeout(std::time::Duration::from_secs(5),async {
   loop {if sqlx::query("BEGIN IMMEDIATE").execute(&mut *writer).await.is_ok(){break;}tokio::task::yield_now().await;}
  }).await?;
  let value:String=sqlx::query_scalar("SELECT value FROM original").fetch_one(&mut *writer).await?;assert_eq!(value,"original");
  sqlx::query("ROLLBACK").execute(&mut *writer).await?;drop(writer);pool.close().await;std::fs::remove_dir_all(path)?;Ok(())
 }
}

// Original activated target retained before a legitimate startup owner runs.
// This contains no actor or mutation grant, and cannot be constructed from a
// copied manifest, path or caller acknowledgment.
pub(crate) struct OriginalTargetRecoverySelection<'a> {
 bundle:&'a crate::storage::backup_inventory::ValidatedBundle,pool:sqlx::AnyPool,
 active:Arc<Active>,pool_source:Arc<crate::storage::recovery_records::PoolSource>,
 root_stamp:PhysicalStamp,database:File,database_stamp:PhysicalStamp,
 _anchor:Option<sqlx::pool::PoolConnection<sqlx::Any>>,
 owner_pager:Option<Arc<VerificationVfsState>>,
}
impl<'a> OriginalTargetRecoverySelection<'a> {
 pub(crate) async fn bind(bundle:&'a crate::storage::backup_inventory::ValidatedBundle,path:&Path,pool:&sqlx::AnyPool)->anyhow::Result<Self> {
  bundle.fence()?;let (_,active)=registered(path)?;let active=active.ok_or_else(||fail("Actual activated target required before recovery"))?;
  anyhow::ensure!(active.engine.is_none()&&active.version2&&active.ledger.backup_id==bundle.manifest.backup_id&&active.ledger.manifest_sha256==digest(&bundle.raw_manifest),"Recovery target belongs to another source");active.check()?;
  let pool_source=crate::storage::recovery_records::pool_source(pool,path).ok_or_else(||fail("Original initialized recovery pool required"))?;
  let database=active.root.leaf("starlane-notes.db")?;let database_stamp=physical_stamp(&database)?;let root_stamp=physical_stamp(active.root.fd())?;
  let anchor=pool.acquire().await?;
  let result=Self{bundle,pool:pool.clone(),active,pool_source,root_stamp,database,database_stamp,_anchor:Some(anchor),owner_pager:None};result.fence()?;Ok(result)
 }
 pub(crate) fn bundle(&self)->&crate::storage::backup_inventory::ValidatedBundle {self.bundle}
 pub(crate) fn path(&self)->&Path {&self.active.root.absolute}
 pub(crate) fn pool(&self)->&sqlx::AnyPool {&self.pool}
 pub(crate) fn fence(&self)->anyhow::Result<()> {
  self.bundle.fence()?;self.active.check()?;
  if let Some(pager)=&self.owner_pager {pager.check()?;} else {
   anyhow::ensure!(physical_stamp(self.active.root.fd())?==self.root_stamp,"Original pre-startup recovery root changed");
   let fresh=self.active.root.leaf("starlane-notes.db")?;
   anyhow::ensure!(physical_stamp(&fresh)?==self.database_stamp&&physical_stamp(&self.database)?==self.database_stamp,"Original pre-startup recovery database changed");
  }
  let actual=crate::storage::recovery_records::pool_source(&self.pool,self.path()).ok_or_else(||fail("Original recovery pool custody unavailable"))?;anyhow::ensure!(Arc::ptr_eq(&actual,&self.pool_source),"Original recovery pool authority replaced");Ok(())
 }
 pub(crate) fn hold_paths(&self,paths:&[String])->anyhow::Result<VerificationRecoveryPaths> {
  self.fence()?;anyhow::ensure!(paths.len()<=16,"Recovery path count exceeded");let mut files=Vec::new();let mut parents=std::collections::BTreeMap::new();
  for path in paths {anyhow::ensure!(valid_relative(path),"Unsafe recovery evidence path");let held=self.active.root.leaf(path)?;let stamp=physical_stamp(&held)?;anyhow::ensure!(stamp.length<=6_000_000,"Recovery held evidence too large");let sha=hash_file(&held,stamp.length)?;files.push((path.clone(),held,stamp,sha));let mut parent=Path::new(path).parent();while let Some(p)=parent {if p.as_os_str().is_empty(){break;}let p=p.to_str().ok_or_else(||fail("Recovery parent not UTF8"))?;if !parents.contains_key(p){let d=self.active.root.directory(p)?;let stamp=physical_stamp(&d)?;parents.insert(p.to_owned(),(d,stamp));}parent=Path::new(p).parent();}}
  self.fence()?;Ok(VerificationRecoveryPaths{files,parents})
 }
}
pub(crate) struct VerificationRecoveryPaths {files:Vec<(String,File,PhysicalStamp,String)>,parents:std::collections::BTreeMap<String,(File,PhysicalStamp)>}
impl VerificationRecoveryPaths {
 pub(crate) fn fence(&self,selection:&OriginalTargetRecoverySelection<'_>)->anyhow::Result<()> {
  selection.fence()?;for (path,(held,stamp)) in &self.parents {let fresh=selection.active.root.directory(path)?;anyhow::ensure!(physical_stamp(held)?==*stamp&&physical_stamp(&fresh)?==*stamp,"Recovery parent directory exchanged");}
  for (path,held,stamp,sha) in &self.files {let fresh=selection.active.root.leaf(path)?;anyhow::ensure!(physical_stamp(held)?==*stamp&&physical_stamp(&fresh)?==*stamp&&hash_file(&fresh,stamp.length)?==*sha,"Recovery immutable evidence exchanged");}selection.fence()?;Ok(())
 }
}
