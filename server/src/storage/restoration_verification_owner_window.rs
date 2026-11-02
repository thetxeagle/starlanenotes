// The private owner keeps ordinary SQLite's genuine exclusive cached lock
// state, plus a kernel OFD lock that survives unrelated main descriptor closes.
// No replacement lock-state adapter or journal-mode change is used.
fn verification_owner_ofd(file:&File,kind:i16,start:i64,length:i64)->io::Result<()> {
 let mut lock:libc::flock=unsafe{std::mem::zeroed()};lock.l_type=kind;lock.l_whence=libc::SEEK_SET as _;lock.l_start=start;lock.l_len=length;lock.l_pid=0;
 if unsafe{libc::fcntl(file.as_raw_fd(),libc::F_OFD_SETLK,&lock)}<0{return Err(io::Error::last_os_error());}Ok(())
}
// A purpose-separated writer handoff from an actual initialized activated target.
// The old source object and selected inode remain held; copied IDs cannot enter.
struct VerificationRecoveryWriter<'a> {
 selection:OriginalTargetRecoverySelection<'a>,
 original_pool:sqlx::AnyPool,
 original_source:Arc<crate::storage::recovery_records::PoolSource>,
 vfs:Option<VerificationVfs>,
}
impl<'a> VerificationRecoveryWriter<'a> {
 async fn enter(mut selection:OriginalTargetRecoverySelection<'a>)->anyhow::Result<Self> {
  selection.fence()?;
  let vfs=VerificationVfs::new(&selection)?;
  let original_pool=selection.pool.clone();let original_source=selection.pool_source.clone();
  drop(selection._anchor.take());original_pool.close().await;
  selection.fence()?;
  let url=format!("sqlite://{}?mode=rw&vfs={}",vfs.state().main_name,vfs.name());
  // Preserve callbacks on cancellation while SQLx disposes connections. Success
  // closes the pool explicitly before dropping/unregistering this private VFS.
  let mut owner=Self{selection,original_pool,original_source,vfs:Some(vfs)};
  let pool=sqlx::any::AnyPoolOptions::new().min_connections(1).max_connections(1).idle_timeout(None).max_lifetime(None).connect(&url).await?;
  let pager=owner.vfs.as_ref().expect("private active writer");pager.check()?;
  owner.selection.fence()?;
  crate::storage::recovery_records::retain_pool_source(&pool,owner.selection.path());
  let source=crate::storage::recovery_records::pool_source(&pool,owner.selection.path()).ok_or_else(||fail("Admitted writer pool binding unavailable"))?;
  pager.check()?;owner.selection.fence()?;
  // Original held descriptors and actual pre-pager gate authorize this one
  // successor. This never binds an arbitrary pool on observation.
  owner.selection.pool=pool;owner.selection.pool_source=source;
  owner.selection.owner_pager=Some(pager.state().clone());
  owner.fence()?;
  sqlx::query("PRAGMA synchronous=FULL").execute(owner.selection.pool()).await?;
  owner.fence()?;
  let mut held=owner.selection.pool().acquire().await?;
  let mode:String=sqlx::query_scalar("PRAGMA locking_mode=EXCLUSIVE").fetch_one(&mut *held).await?;
  anyhow::ensure!(mode=="exclusive","Actual private Unix connection must retain exclusive lock mode");
  sqlx::query("BEGIN EXCLUSIVE").execute(&mut *held).await?;
  owner.fence()?;
  let pager=owner.vfs.as_ref().expect("private writer");
  let exclusion=open_at(&pager.state().root,"starlane-notes.db",libc::O_RDWR)?;
  anyhow::ensure!(identity(&exclusion)?==pager.state().database,"Private writer exclusion opened foreign main");
  // This strict original descriptor fence also closes its temporary main File:
  // Linux releases the process-owned Unix locks. SQLite's actual exclusive
  // connection remains cached; acquire the durable kernel lease before any DML.
  owner.fence()?;pager.state().initial_database()?;
  verification_owner_ofd(&exclusion,libc::F_WRLCK as _,0x40000000,512)?;
  pager.state().initial_database()?;owner.fence()?;
  *pager.state().owner_exclusion.lock().map_err(|_|fail("Owner exclusion poisoned"))?=Some(exclusion);
  sqlx::query("ROLLBACK").execute(&mut *held).await?;drop(held);
  owner.fence()?;owner.vfs.as_ref().expect("private writer").permit_own_journal()?;owner.fence()?;
  Ok(owner)
 }
 fn selection(&self)->&OriginalTargetRecoverySelection<'a> {&self.selection}
 fn fence(&self)->anyhow::Result<()> {
  self.selection.fence()?;
  anyhow::ensure!(self.original_pool.is_closed(),"Original pool must stay stopped during writer handoff");
  let original=crate::storage::recovery_records::pool_source(&self.original_pool,self.selection.path()).ok_or_else(||fail("Original pre-handoff native source lost"))?;
  anyhow::ensure!(Arc::ptr_eq(&original,&self.original_source),"Original initialized source object changed");Ok(())
 }
 async fn seal(&self,target:&VerificationTarget<'_>,reservation:&mut VerificationReservation,projection:&engine_transfer::VerificationSqlProjection<'_>,ack:&crate::storage::publication::PublicationRecoveryAck<'_,'_>)->anyhow::Result<VerificationWriterSeal> {
  self.fence()?;target.fence(reservation).await?;
  let closure=projection.compare_after_publication_recovery(reservation,target,ack).await?;
  target.fence(reservation).await?;self.fence()?;
  let pager=self.vfs.as_ref().ok_or_else(||fail("Actual writer registration absent"))?;
  anyhow::ensure!(Arc::ptr_eq(&target.active,&pager.state().active),"Writer seal belongs to another actual generation");
  // Exclusive-mode SQLite retains its genuinely owned journal until close.
  // This private pre-close seal cannot admit a reader; close below requires
  // the actual journal retirement callback before returning the successor.
  let held=target.database.try_clone()?;let stamp=physical_stamp(&held)?;let bytes=planned_hash(&held)?;
  let seal=VerificationWriterSeal{closed:false,state:pager.state().clone(),held,stamp,bytes,
    original_sql_sha256:closure.source_sha256().to_owned(),qualified_sql_sha256:closure.qualified_source_sha256().to_owned(),target_sql_sha256:closure.target_sha256().to_owned(),owner_acknowledged_noop:closure.owner_acknowledged_noop()};
  seal.fence()?;self.fence()?;Ok(seal)
 }
 async fn close(mut self,mut seal:VerificationWriterSeal)->anyhow::Result<VerificationWriterSeal> {
  anyhow::ensure!(self.vfs.as_ref().is_some_and(|v|Arc::ptr_eq(v.state(),&seal.state)),"Seal belongs to another actual writer");
  seal.fence()?;
  self.selection.pool.close().await;
  let closed_pager=self.vfs.as_ref().ok_or_else(||fail("Actual writer guard missing"))?;
  anyhow::ensure!(closed_pager.state().journal.lock().map_err(|_|fail("Writer journal poisoned"))?.is_none(),"Actual owner close did not retire its journal");
  seal.fence()?;
  // Downgrade the shared bytes while the exclusive OFD guard is still held,
  // then release only the pending byte. Reserved-start and commit exclusion stay held.
  let pager=self.vfs.as_ref().ok_or_else(||fail("Actual writer guard missing"))?;
  let lock=pager.state().owner_exclusion.lock().map_err(|_|fail("Owner exclusion poisoned"))?;
  let guard=lock.as_ref().ok_or_else(||fail("Actual durable owner exclusion absent"))?;
  verification_owner_ofd(guard,libc::F_RDLCK as _,0x40000001,511)?;
  verification_owner_ofd(guard,libc::F_UNLCK as _,0x40000000,1)?;
  seal.fence()?;drop(lock);
  // No asynchronous connection remains when callbacks/method tables are freed.
  self.vfs.take().ok_or_else(||fail("Actual writer registration missing"))?.retire();seal.fence()?;seal.closed=true;seal.reader_ready()?;Ok(seal)
 }

}
impl Drop for VerificationRecoveryWriter<'_> {
 fn drop(&mut self) {
  // A cancelled/failed SQLx close is unknown. Keep the registered callbacks and
  // native descriptors until process exit; never race their use-after-free.
  if let Some(vfs)=self.vfs.take(){std::mem::forget(vfs);}
 }
}

// Opaque physical handoff only, created after the actual owner acknowledgment and
// complete lossless original SQL qualification. No health/serving record grant.
struct VerificationWriterSeal {
 closed:bool,state:Arc<VerificationVfsState>,held:File,stamp:PhysicalStamp,bytes:crate::storage::backup_inventory::Bytes,
 original_sql_sha256:String,qualified_sql_sha256:String,target_sql_sha256:String,owner_acknowledged_noop:bool,
}
impl VerificationWriterSeal {
 fn reader_ready(&self)->io::Result<()> {
  if !self.closed{return Err(fail("Original writer close acknowledgment absent"));}
  if self.state.journal.lock().map_err(|_|fail("Writer journal poisoned"))?.is_some(){return Err(fail("Original writer journal is still live"));}
  self.fence()
 }
 fn fence(&self)->io::Result<()> {
  self.state.ancestry()?;
  let fresh=open_at(&self.state.root,"starlane-notes.db",libc::O_RDONLY)?;
  if physical_stamp(&self.held)?!=self.stamp||physical_stamp(&fresh)?!=self.stamp||planned_hash(&self.held)?!=self.bytes||planned_hash(&fresh)?!=self.bytes{return Err(fail("Sealed actual writer database changed"));}
  if self.qualified_sql_sha256!=self.target_sql_sha256||((self.original_sql_sha256==self.target_sql_sha256)!=self.owner_acknowledged_noop){return Err(fail("Invalid private original owner SQL closure"));}Ok(())
 }
}
