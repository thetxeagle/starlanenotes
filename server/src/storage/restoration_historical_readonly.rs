// Independent historical read-only admission. No wire/path/report constructs it.
pub(crate) struct StoppedHistoricalVerificationSource<'a>{original:StoppedVerificationSource<'a>}
impl StoppedHistoricalVerificationSource<'_>{
 pub(crate) fn path(&self)->&Path{self.original.path()}
 pub(crate) async fn current_human(&mut self)->anyhow::Result<String>{self.original.current_human().await}
 pub(crate) async fn fence(&mut self)->anyhow::Result<()>{anyhow::ensure!(self.original.historical.is_some()&&self.original.admin.is_some(),"Original historical continuation provenance absent");self.original.fence().await}
}
pub(crate) async fn capture_sqlite_historical_for_observation<T,F>(source:&Path,target:&Path,version:&str,writers_stopped:bool,admin:&crate::auth::middleware::AdminUser,observe:F)->anyhow::Result<T>
where F:for<'a> FnOnce(StoppedHistoricalVerificationSource<'a>,Arc<crate::storage::backup_inventory::ValidatedBundle>)->futures::future::LocalBoxFuture<'a,anyhow::Result<T>>{
 capture_sqlite_mode(source,target,version,writers_stopped,Some(admin),true,move|original,bundle|observe(StoppedHistoricalVerificationSource{original},bundle)).await
}
struct HistoricalReadonlyAdmission {
 ready:BootReadyDestination,active:Arc<Active>,main:Arc<File>,main_stamp:PhysicalStamp,
 lease:File,parents:Vec<(File,PhysicalStamp)>,original_pool:sqlx::AnyPool,
 original_source:Arc<crate::storage::recovery_records::PoolSource>,
}
impl HistoricalReadonlyAdmission{
 async fn select_original(ready:BootReadyDestination,pool:&sqlx::AnyPool)->anyhow::Result<Arc<Self>>{
  ready.fence()?;let(root,active)=registered(ready.path())?;let active=active.ok_or_else(||fail("Actual historical target activation absent"))?;
  anyhow::ensure!(active.engine.is_none()&&active.version2&&active.ledger.version==5&&active.ledger.generation_id==ready.generation_id,"Original readonly selection is not actual SQLite Ledger5");
  let source=crate::storage::recovery_records::pool_source(pool,ready.path()).ok_or_else(||fail("Actual initialized target pool provenance absent"))?;
  let main=active.sqlite_main.as_ref().ok_or_else(||fail("Actual readonly target main absent"))?.clone();let stamp=physical_stamp(main.as_ref())?;
  {let mut c=pool.acquire().await?;qualified_sqlite_connection(&root,&mut c).await?;let observed=HistoricalPublishedObservation::hold_original_published(&ready,&mut c).await?;observed.recheck_in(&mut c).await?;}
  let lease=open_at(root.fd(),"starlane-notes.db",libc::O_RDONLY|libc::O_NONBLOCK)?;
  anyhow::ensure!(physical_stamp(&lease)?==stamp,"Readonly independent exclusion selected foreign main");historical_readonly_reader_shared_lease(&lease)?;
  let parents=active.root.chain.iter().map(|f|Ok((f.try_clone()?,physical_stamp(f)?))).collect::<io::Result<Vec<_>>>()?;
  let held=Arc::new(Self{ready,active,main,main_stamp:stamp,lease,parents,original_pool:pool.clone(),original_source:source});held.io_fence()?;pool.close().await;held.fence()?;Ok(held)
 }
 fn root(&self)->&File{self.active.root.fd()}
 fn path(&self)->&Path{&self.active.root.absolute}
 fn database(&self)->Identity{(self.main_stamp.device,self.main_stamp.inode)}
 fn stamp(&self)->&PhysicalStamp{&self.main_stamp}
 fn held_main(&self)->&File{self.main.as_ref()}
 fn io_fence(&self)->io::Result<()>{
  self.active.root.check()?;for(f,stamp)in &self.parents{if physical_stamp(f)?!=*stamp{return Err(fail("Historical readonly original ancestor changed"));}}
  if physical_stamp(self.main.as_ref())?!=self.main_stamp||physical_stamp(&self.lease)?!=self.main_stamp||qualified_sqlite_path_identity(&self.active.root)?!=self.database(){return Err(fail("Historical readonly original main/exclusion changed"));}Ok(())
 }
 fn fence(&self)->io::Result<()>{
  if !self.original_pool.is_closed(){return Err(fail("Historical original target pool has not actually closed"));}
  self.io_fence()?;self.active.check()?;
  let actual=crate::storage::recovery_records::pool_source(&self.original_pool,self.path()).ok_or_else(||fail("Historical original initialized pool source lost"))?;
  if !Arc::ptr_eq(&actual,&self.original_source){return Err(fail("Historical original readonly selection pool replaced"));}
  historical_retired_observation_fence(&self.ready.destination)?;
  let source=self.ready.historical_source.as_ref().ok_or_else(||fail("Original historical source facts absent"))?;source.retained.fence()?;
  let witness=self.ready.historical_witness.as_ref().ok_or_else(||fail("Original historical publication witness absent"))?;witness.check(&self.ready.destination,&self.ready.historical_original,&witness.committed_row)?;self.io_fence()
 }
}
enum HistoricalPhysicalPager<'a>{OriginalSource512(&'a SqliteCaptureCommitLease),ActualReadonly511(Arc<HistoricalReadonlyReaderState>)}
impl HistoricalPhysicalPager<'_>{
 fn check(&self,active:&Arc<Active>)->io::Result<()>{match self{
  Self::OriginalSource512(exclusion)=>{if exclusion.start!=0x40000000||exclusion.length!=512||identity(&exclusion.file)?!=identity(active.sqlite_main.as_ref().ok_or_else(||fail("Historical original main lost"))?.as_ref())?{return Err(fail("Historical original full512 exclusion changed"));}Ok(())},
  Self::ActualReadonly511(state)=>{if !Arc::ptr_eq(active,&state.authority.active()?){return Err(fail("Historical actual readonly activation replaced"));}state.native_fence()},
 }}
 async fn connection_in(&self,active:&Arc<Active>,conn:&mut sqlx::AnyConnection)->io::Result<()>{match self{
  Self::OriginalSource512(_)=>qualified_sqlite_connection(&active.root,conn).await,
  Self::ActualReadonly511(state)=>{use sqlx::Row;state.check()?;let rows=sqlx::query("PRAGMA database_list").fetch_all(&mut *conn).await.map_err(|_|fail("Readonly actual SQLite connection absent"))?;let main=rows.iter().find(|r|r.try_get::<String,_>("name").ok().as_deref()==Some("main")).ok_or_else(||fail("Readonly connected main absent"))?;if main.try_get::<String,_>("file").ok().as_deref()!=Some(&state.main_name){return Err(fail("Historical actual readonly connected target differs"));}state.check()},
 }}
}
struct HistoricalReadonlySelectedTarget{reader:HistoricalReadonlySealedReader}
impl HistoricalReadonlySelectedTarget{
 async fn enter(ready:BootReadyDestination,pool:&sqlx::AnyPool)->anyhow::Result<Self>{let authority=HistoricalReadonlyAdmission::select_original(ready,pool).await?;let reader=HistoricalReadonlySealedReader::enter(authority).await?;reader.fence()?;Ok(Self{reader})}
 async fn observe(&self)->anyhow::Result<HistoricalFinalizedObservation<'static>>{self.reader.fence()?;let state=self.reader.vfs.as_ref().ok_or_else(||fail("Actual readonly VFS absent"))?.state().clone();let mut conn=self.reader.pool.acquire().await?;let held=HistoricalFinalizedObservation::hold_readonly_in(state,&mut conn).await?;held.recheck_in(&mut conn).await?;self.reader.fence()?;Ok(held)}
 async fn retire(self,physical:HistoricalFinalizedObservation<'static>)->anyhow::Result<HistoricalReadonlyRetiredTarget>{
  self.reader.fence()?;let state=self.reader.vfs.as_ref().ok_or_else(||fail("Actual retiring readonly VFS absent"))?.state().clone();
  anyhow::ensure!(matches!(&physical.pager,HistoricalPhysicalPager::ActualReadonly511(s)if Arc::ptr_eq(s,&state)),"Physical observation belongs to another actual reader");
  {let mut conn=self.reader.pool.acquire().await?;physical.recheck_in(&mut conn).await?;}
  self.reader.close().await?;let retired=HistoricalReadonlyRetiredTarget{physical,state};retired.fence()?;Ok(retired)
 }
}
struct HistoricalReadonlyRetiredTarget{physical:HistoricalFinalizedObservation<'static>,state:Arc<HistoricalReadonlyReaderState>}
impl HistoricalReadonlyRetiredTarget{
 fn fence(&self)->io::Result<()>{if !self.state.retiring.load(std::sync::atomic::Ordering::Acquire)||self.state.closing.load(std::sync::atomic::Ordering::Acquire)!=0||self.state.close_failed.load(std::sync::atomic::Ordering::Acquire)||!self.state.bindings.lock().map_err(|_|fail("Readonly retirement bindings poisoned"))?.is_empty(){return Err(fail("Historical actual readonly callbacks are not retired"));}self.physical.fence_physical()?;self.state.native_fence()}
}
