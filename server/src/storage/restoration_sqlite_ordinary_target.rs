fn ordinary_sqlite_selected_main(root:&Root,held:&File,expected:Identity)->anyhow::Result<()> {
    // Check the name before any main open. Opening/closing a foreign main must
    // not alter the original pager's process-associated locks.
    let name=CString::new("starlane-notes.db")?;let mut named:libc::stat=unsafe{std::mem::zeroed()};
    anyhow::ensure!(unsafe{libc::fstatat(root.fd().as_raw_fd(),name.as_ptr(),&mut named,libc::AT_SYMLINK_NOFOLLOW)}==0, "Ordinary selected main unavailable");
    anyhow::ensure!(identity(held)?==expected && (named.st_dev as u64,named.st_ino as u64)==expected && named.st_mode&libc::S_IFMT==libc::S_IFREG && named.st_nlink==1 && named.st_uid==unsafe{libc::geteuid()} && named.st_mode&0o022==0, "Ordinary selected named main is foreign/aliased/unsafe");
    Ok(())
}
// Distinct ordinary-main phases. Neither a path nor later copied SQL can
// reconstruct the original selected target or a confirmed original child stop.
struct OrdinarySqliteActivityWindow {
    exercised: std::collections::BTreeSet<String>,
    started: chrono::DateTime<chrono::Utc>,
    finished: chrono::DateTime<chrono::Utc>,
}
struct OrdinarySqliteSelectedTarget<'b> {
    bundle: &'b crate::storage::backup_inventory::ValidatedBundle,
    active: Arc<Active>,
    database: File,
    database_identity: Identity,
    root_identity: Identity,
    original_pool: sqlx::AnyPool,
    original_source: Arc<crate::storage::recovery_records::PoolSource>,
    selection: String,
}
impl<'b> OrdinarySqliteSelectedTarget<'b> {
    async fn from_original(mut original: OriginalTargetRecoverySelection<'b>) -> anyhow::Result<Self> {
        original.fence()?;
        anyhow::ensure!(original.owner_pager.is_none(), "Ordinary main selection must precede recovery");
        let database_identity=identity(&original.database)?;
        let root_identity=identity(original.active.root.fd())?;
        drop(original._anchor.take()); original.pool.close().await; original.fence()?;
        let selected=Self{bundle:original.bundle,active:original.active,database:original.database,database_identity,root_identity,original_pool:original.pool,original_source:original.pool_source,selection:uuid::Uuid::new_v4().to_string()};
        selected.fence()?;Ok(selected)
    }
    fn fence(&self) -> anyhow::Result<()> {
        self.bundle.fence()?;self.active.check()?;
        anyhow::ensure!(self.active.engine.is_none() && self.active.version2 && self.original_pool.is_closed(), "Ordinary original selection no longer stopped SQLite");
        let root=&self.active.root;
        anyhow::ensure!(identity(root.fd())?==self.root_identity && identity(&self.database)?==self.database_identity, "Ordinary selected native root/main changed");
        ordinary_sqlite_selected_main(root,&self.database,self.database_identity)?;
        let original=crate::storage::recovery_records::pool_source(&self.original_pool,&root.absolute).ok_or_else(||fail("Original pre-child pool source lost"))?;
        anyhow::ensure!(Arc::ptr_eq(&original,&self.original_source), "Original pre-child pool provenance changed");
        Ok(())
    }
    async fn after_stopped(self, stopped:OrdinarySqliteStoppedChild) -> anyhow::Result<(OriginalTargetRecoverySelection<'b>,OrdinarySqliteFinalOrigin<'b>)> {
        anyhow::ensure!(stopped.selection==self.selection && stopped.root==self.root_identity && stopped.database==self.database_identity, "Stopped child belongs to another original target");
        self.fence()?;stopped.fence()?;
        // This is actual initialized final-phase custody, never a lazy reader
        // minting the old pre-startup pool identity from a pathname.
        let (pool,engine)=crate::db::init_pool().await?;
        anyhow::ensure!(engine==crate::db::DatabaseEngine::Sqlite, "Final ordinary phase requires SQLite");
        self.fence()?;
        let selected_source=crate::storage::recovery_records::pool_source(&pool,&self.active.root.absolute).ok_or_else(||fail("Actual final initialized pool source absent"))?;
        let (_,current)=registered(&self.active.root.absolute)?;
        anyhow::ensure!(current.is_some_and(|a|Arc::ptr_eq(&a,&self.active)), "Original selected activation object replaced");
        let database=self.database.try_clone()?;let database_stamp=physical_stamp(&database)?;let root_stamp=physical_stamp(self.active.root.fd())?;
        let anchor=pool.acquire().await?;self.fence()?;
        let result=OriginalTargetRecoverySelection{bundle:self.bundle,pool,active:self.active.clone(),pool_source:selected_source,root_stamp,database,database_stamp,_anchor:Some(anchor),owner_pager:None};
        result.fence()?;self.fence()?;stopped.fence()?;Ok((result,OrdinarySqliteFinalOrigin{selected:self,stopped}))
    }
}
struct OrdinarySqliteStoppedChild {
    terminal:std::process::ExitStatus,
    selection:String,root:Identity,database:Identity,pid:u32,executable_sha256:String,
    listener:File,listener_identity:Identity,address:std::net::SocketAddr,executable:File,executable_stamp:String,
}
impl OrdinarySqliteStoppedChild {fn fence(&self)->anyhow::Result<()>{ordinary_sqlite_expected_stop(self.terminal)?;anyhow::ensure!(ordinary_listener_identity(&self.listener)?==self.listener_identity && ordinary_listener_address(&self.listener)?==self.address && verification_worker_stamp(&self.executable)?==self.executable_stamp && hash_file(&self.executable,self.executable.metadata()?.len())?==self.executable_sha256,"Original stopped-child transport/ELF changed");Ok(())}}
struct OrdinarySqliteFinalOrigin<'b>{selected:OrdinarySqliteSelectedTarget<'b>,stopped:OrdinarySqliteStoppedChild}
impl OrdinarySqliteFinalOrigin<'_>{fn fence(&self)->anyhow::Result<()>{self.selected.fence()?;self.stopped.fence()?;Ok(())}}
// Native identity and successful probes do not certify an abnormal child exit.
// Keep the actual wait status in the nominal stop token through publication.
fn ordinary_sqlite_expected_stop(status:std::process::ExitStatus)->anyhow::Result<()> {
    use std::os::unix::process::ExitStatusExt;
    anyhow::ensure!(status.success()||status.signal()==Some(libc::SIGTERM),"Original ordinary child terminated abnormally: {status}");Ok(())
}
fn ordinary_sqlite_stop(selected:&OrdinarySqliteSelectedTarget<'_>,listener:&OrdinaryListenerParent<'_>,child:&mut std::process::Child) -> anyhow::Result<OrdinarySqliteStoppedChild> {
    selected.fence()?;listener.fence()?;
    anyhow::ensure!(std::ptr::eq(listener.root,&selected.active.root) && listener.ready, "Ordinary stop has no exact original ready listener");
    anyhow::ensure!(child.try_wait()?.is_none(), "Ordinary child exited before deliberate final stop");
    let pid=child.id();let executable=File::open(format!("/proc/{pid}/exe"))?;
    anyhow::ensure!(verification_worker_stamp(&executable)?==listener.wire.executable_stamp && hash_file(&executable,executable.metadata()?.len())?==listener.wire.executable_sha256, "Original ordinary child executable changed before stop");
    // The original unreaped Child owns its PID; no process lookup guesses it.
    anyhow::ensure!(unsafe{libc::kill(i32::try_from(pid)?,libc::SIGTERM)}==0, "Original ordinary child stop refused");
    let terminal=child.wait()?;ordinary_sqlite_expected_stop(terminal)?;
    selected.fence()?;listener.fence()?;
    Ok(OrdinarySqliteStoppedChild{terminal,selection:selected.selection.clone(),root:selected.root_identity,database:selected.database_identity,pid,executable_sha256:listener.wire.executable_sha256.clone(),listener:listener.listener.try_clone()?,listener_identity:listener.wire.listener_identity,address:listener.wire.address,executable:listener.executable.try_clone()?,executable_stamp:listener.wire.executable_stamp.clone()})
}
impl VerificationRecoveryWriter<'_> {
    async fn seal_ordinary(&self,target:&VerificationTarget<'_>,reservation:&mut VerificationReservation,qualified:&engine_transfer::OrdinarySqliteQualified) -> anyhow::Result<VerificationWriterSeal> {
        self.fence()?;target.fence(reservation).await?;
        let closure=qualified.closure();
        anyhow::ensure!(closure.qualified_source_sha256()==closure.target_sha256(), "Ordinary final closure differs");
        let pager=self.vfs.as_ref().ok_or_else(||fail("Original final writer absent"))?;
        anyhow::ensure!(Arc::ptr_eq(&target.active,&pager.state().active), "Ordinary seal belongs to another actual generation");
        let held=target.database.try_clone()?;let stamp=physical_stamp(&held)?;let bytes=planned_hash(&held)?;
        let seal=VerificationWriterSeal{closed:false,state:pager.state().clone(),held,stamp,bytes,original_sql_sha256:closure.source_sha256().to_owned(),qualified_sql_sha256:closure.qualified_source_sha256().to_owned(),target_sql_sha256:closure.target_sha256().to_owned(),owner_acknowledged_noop:closure.owner_acknowledged_noop()};
        seal.fence()?;target.fence(reservation).await?;self.fence()?;Ok(seal)
    }
}

#[cfg(test)] mod ordinary_sqlite_nominal_controls {
    use super::*;
    #[test] fn exact_selected_main_named_stat_rejects_symlink_and_alias() -> anyhow::Result<()> {
        use std::os::unix::fs::DirBuilderExt;
        let path=std::env::temp_dir().join(format!("ordinary-sqlite-named-{}",uuid::Uuid::new_v4()));std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        let root=Root::open(&path)?;let original=sqlite_capture_write(&root,"starlane-notes.db",b"original native inode")?;
        let original_id=identity(&original)?;
        std::fs::rename(path.join("starlane-notes.db"),path.join("retained-original"))?;
        sqlite_capture_write(&root,"starlane-notes.db",b"original native inode")?;
        anyhow::ensure!(ordinary_sqlite_selected_main(&root,&original,original_id).is_err(),"Equal bytes fabricated original identity");
        std::fs::remove_file(path.join("starlane-notes.db"))?;
        std::fs::rename(path.join("retained-original"),path.join("starlane-notes.db"))?;
        ordinary_sqlite_selected_main(&root,&original,original_id)?;
        std::fs::hard_link(path.join("starlane-notes.db"),path.join("foreign-alias"))?;
        anyhow::ensure!(ordinary_sqlite_selected_main(&root,&original,original_id).is_err(),"Aliased main accepted");
        std::fs::remove_file(path.join("foreign-alias"))?;
        std::fs::rename(path.join("starlane-notes.db"),path.join("retained-original"))?;
        std::os::unix::fs::symlink("retained-original",path.join("starlane-notes.db"))?;
        anyhow::ensure!(ordinary_sqlite_selected_main(&root,&original,original_id).is_err(),"Symlink main accepted");
        Ok(())
    }
}

// First bounded ordinary-record class: the actual finalized Ledger1
// preservation owner. No copied capsule becomes a writer or cleanup capability.
struct SqliteFinalObservedTarget<'b> {
    origin:OrdinarySqliteFinalOrigin<'b>,
    reader:VerificationSealedReader,
    reservation:VerificationReservation,
    target:VerificationTarget<'b>,
    preservation:FinalizedAdmissionPreservationLeases,
    qualified:engine_transfer::OrdinarySqliteQualified,
}
async fn ordinary_sqlite_source_work<T>(source:&mut StoppedVerificationSource<'_>,work:impl std::future::Future<Output=anyhow::Result<T>>)->anyhow::Result<T>{
    source.fence().await?;tokio::pin!(work);
    let deadline=tokio::time::sleep(std::time::Duration::from_secs(120));tokio::pin!(deadline);
    let mut pulse=tokio::time::interval(std::time::Duration::from_secs(3));pulse.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);pulse.tick().await;
    loop {tokio::select!{result=&mut work=>{source.fence().await?;return result;},_=&mut deadline=>anyhow::bail!("Original ordinary source observation exceeded120s"),_=pulse.tick()=>source.fence().await?}}
}
impl<'b> SqliteFinalObservedTarget<'b> {
    async fn hold(selected:OrdinarySqliteSelectedTarget<'b>,stopped:OrdinarySqliteStoppedChild,source:&mut StoppedVerificationSource<'_>,projection:&engine_transfer::VerificationSqlProjection<'_>,window:&OrdinarySqliteActivityWindow)->anyhow::Result<Self>{
        source.fence().await?;
        let (selection,origin)=selected.after_stopped(stopped).await?;origin.fence()?;source.fence().await?;
        let writer=VerificationRecoveryWriter::enter(selection).await?;writer.fence()?;origin.fence()?;
        anyhow::ensure!(has_finalized_admission_preservation_source(writer.selection().path())?, "Ordinary completed record unavailable for this native provenance class; Ledger1 preservation required");
        let preservation={
            let mut borrowed=writer.selection().pool().acquire().await?;writer.fence()?;
            let rows=prior_capture_rows(&mut borrowed).await?;
            let preservation=FinalizedAdmissionPreservationLeases::hold(writer.selection().path(),&mut borrowed,&rows).await?;
            preservation.recheck_in(&mut borrowed).await?;writer.fence()?;
            drop(borrowed);writer.fence()?;preservation
        };
        let prepared=projection.prepare_ordinary_writer(&writer,window).await?;
        let stages=crate::storage::journal_fs::inventory(writer.selection().path(),".publication-journal",!prepared.operations().is_empty())?.into_iter().collect::<std::collections::BTreeSet<_>>();
        anyhow::ensure!(stages==prepared.operations(), "Ordinary COMPLETE source receipt set differs from native publication stage set");
        writer.fence()?;origin.fence()?;source.fence().await?;
        // No final reservation is held here. The actual V90 writer owns durable
        // exclusion; normal owner recovery may use its real one-connection pool.
        let ack=ordinary_sqlite_source_work(source,async{crate::storage::publication::recover_for_verification(writer.selection()).await.map_err(|e|anyhow::anyhow!("Actual final ordinary publication owner refused:{e:?}"))}).await?;
        writer.fence()?;origin.fence()?;
        let mut reservation=VerificationReservation::begin(writer.selection().pool()).await?;
        let target=verification_target(origin.selected.bundle,&origin.selected.active.root.absolute,writer.selection().pool(),&mut reservation).await?;
        preservation.recheck_in(reservation.connection()).await?;
        let qualified=prepared.finish(&mut reservation,&target,&ack).await?;
        let seal=writer.seal_ordinary(&target,&mut reservation,&qualified).await?;
        qualified.fence(projection,&mut reservation,&target).await?;origin.fence()?;source.fence().await?;
        drop(target);reservation.rollback().await?;drop(ack);
        let seal=writer.close(seal).await?;seal.reader_ready()?;
        source.fence().await?;origin.fence()?;
        let reader=VerificationSealedReader::enter(seal).await?;reader.fence()?;
        let mut reservation=VerificationReservation::begin(&reader.pool).await?;
        let target=verification_target(origin.selected.bundle,&origin.selected.active.root.absolute,&reader.pool,&mut reservation).await?;
        let mut final_target=Self{origin,reader,reservation,target,preservation,qualified};
        final_target.fence(projection).await?;source.fence().await?;Ok(final_target)
    }
    async fn fence(&mut self,projection:&engine_transfer::VerificationSqlProjection<'_>)->anyhow::Result<()>{
        self.origin.fence()?;self.reader.fence()?;self.target.fence(&mut self.reservation).await?;
        self.preservation.recheck_in(self.reservation.connection()).await?;
        self.qualified.fence(projection,&mut self.reservation,&self.target).await?;
        self.origin.fence()?;self.reader.fence()?;Ok(())
    }
    async fn retire(self,projection:&engine_transfer::VerificationSqlProjection<'_>,source:&mut StoppedVerificationSource<'_>)->anyhow::Result<SqliteRetiredObservedTarget<'b>>{
        let mut observed=self;observed.fence(projection).await?;source.fence().await?;
        let Self{origin,mut reader,reservation,target,preservation,qualified}=observed;
        let guard=reader.vfs.as_ref().ok_or_else(||fail("Actual final reader registration absent"))?;
        guard.stop_admissions()?;
        let state=guard.state().clone();
        // SQLx close() synchronously marks this pool closed before returning its
        // drain future. No min-connection refill may replace the pinned reader
        // when close_on_drop retires the owned final reservation.
        let closing=reader.pool.close();
        anyhow::ensure!(reader.pool.is_closed(),"Actual final reader did not stop new acquisitions");
        reservation.rollback().await?;closing.await;
        guard.native_close_ack().await?;state.native_fence()?;preservation.fence()?;origin.fence()?;source.fence().await?;
        reader.vfs.take().ok_or_else(||fail("Actual final reader registration lost"))?.retire();
        let retired=SqliteRetiredObservedTarget{origin,target,state,preservation,qualified};
        drop(reader);retired.fence(projection).await?;source.fence().await?;Ok(retired)
    }
}
// Native retirement, not a serializable report, creates this final observation.
// All SQLite callbacks have acknowledged closure; the actual sealed main/OFD,
// full inventory and capsule leases stay held through durable publication.
struct SqliteRetiredObservedTarget<'b>{
 origin:OrdinarySqliteFinalOrigin<'b>,target:VerificationTarget<'b>,
 state:Arc<VerificationReaderState>,preservation:FinalizedAdmissionPreservationLeases,
 qualified:engine_transfer::OrdinarySqliteQualified,
}
impl SqliteRetiredObservedTarget<'_>{
 async fn fence(&self,projection:&engine_transfer::VerificationSqlProjection<'_>)->anyhow::Result<()>{
  anyhow::ensure!(self.target.pool.is_closed()&&self.state.retiring.load(std::sync::atomic::Ordering::Acquire),"Actual final reader is not retired");
  anyhow::ensure!(self.state.bindings.lock().map_err(|_|fail("Retired reader bindings poisoned"))?.is_empty()&&self.state.closing.load(std::sync::atomic::Ordering::Acquire)==0&&!self.state.close_failed.load(std::sync::atomic::Ordering::Acquire),"Actual native reader drain unavailable");
  self.origin.fence()?;self.state.native_fence()?;self.preservation.fence()?;self.target.bundle.fence()?;self.target.active.check()?;
  anyhow::ensure!(projection.ordinary_is_bound_to(self.target.bundle)&&projection.original_source_sha256()?==self.qualified.closure().source_sha256(),"Retired target belongs to another original source");
  anyhow::ensure!(physical_stamp(&self.target.database)?==self.target.database_stamp&&physical_stamp(self.target.active.root.fd())?==self.target.root_stamp&&self.target.database_stamp==*self.state.authority.stamp(),"Retired actual target main/root changed");
  for sidecar in &self.target.pager_sidecars{sidecar.fence(&self.target.active.root)?;}
  let actual=crate::storage::recovery_records::pool_source(&self.target.pool,self.target.path()).ok_or_else(||fail("Retired original pool provenance absent"))?;
  anyhow::ensure!(Arc::ptr_eq(&actual,&self.target.pool_source),"Retired original pool provenance changed");
  self.state.native_fence()?;self.preservation.fence()?;self.origin.fence()?;Ok(())
 }
}
#[cfg(test)]mod ordinary_sqlite_lock_controls {
    use super::*;
    #[tokio::test]async fn alias_refusal_preserves_actual_original_sqlite_lock()->anyhow::Result<()>{
        use std::os::unix::fs::DirBuilderExt;
        sqlx::any::install_default_drivers();
        let path=std::env::temp_dir().join(format!("ordinary-main-lock-{}",uuid::Uuid::new_v4()));std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        let root=Root::open(&path)?;let db=path.join("starlane-notes.db");let pool=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=rwc",db.display())).await?;
        sqlx::query("CREATE TABLE original(value TEXT NOT NULL)").execute(&pool).await?;sqlx::query("INSERT INTO original VALUES('original')").execute(&pool).await?;
        let held=root.leaf("starlane-notes.db")?;let selected=identity(&held)?;
        let mut conn=pool.acquire().await?;sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
        std::fs::hard_link(&db,path.join("borrowed-canonical-alias"))?;
        anyhow::ensure!(ordinary_sqlite_selected_main(&root,&held,selected).is_err(),"Aliased actual main was accepted");
        let script="import sqlite3,sys\nc=sqlite3.connect(sys.argv[1],timeout=0)\ntry:\n c.execute(\"UPDATE original SET value='foreign'\");c.commit()\nexcept sqlite3.OperationalError as e:\n sys.exit(0 if 'locked' in str(e) else 2)\nsys.exit(1)\n";
        let child=std::process::Command::new("/usr/bin/python3").args(["-c",script]).arg(&db).status()?;
        anyhow::ensure!(child.success(),"Original SQLite lock was released by rejected metadata observation");
        sqlx::query("ROLLBACK").execute(&mut *conn).await?;drop(conn);
        let child=std::process::Command::new("/usr/bin/python3").args(["-c","import sqlite3,sys;c=sqlite3.connect(sys.argv[1],timeout=0);c.execute(\"UPDATE original SET value='released'\");c.commit()"]).arg(&db).status()?;
        anyhow::ensure!(child.success(),"Original rollback did not release writer exclusion");
        pool.close().await;Ok(())
    }
}

#[cfg(test)]mod ordinary_sqlite_stop_status_controls {
 use super::*;use std::os::unix::process::ExitStatusExt;
 #[test]fn completed_child_stop_accepts_only_sigterm_or_clean_exit()->anyhow::Result<()> {
  for raw in [0,libc::SIGTERM]{ordinary_sqlite_expected_stop(std::process::ExitStatus::from_raw(raw))?;}
  for raw in [libc::SIGSEGV,libc::SIGSEGV|0x80,libc::SIGKILL,1<<8,127<<8]{
   anyhow::ensure!(ordinary_sqlite_expected_stop(std::process::ExitStatus::from_raw(raw)).is_err(),"Abnormal terminal status accepted:{raw}");
  }Ok(())
 }
}
