// Deliberate qualified PostgreSQL operator flow. Namespace presence selects a
// validator; only its held SQL-anchored final proof grants serving authority.
pub(crate) enum QualifiedPgServing {Transfer(EngineServing),PgRestore(RestoredPgServing)}
impl QualifiedPgServing {
 pub(crate) fn pool(&self)->&AnyPool {match self{Self::Transfer(s)=>s.pool(),Self::PgRestore(s)=>s.pool()}}
 pub(crate) async fn fence(&self)->anyhow::Result<()> {match self{Self::Transfer(s)=>s.fence().await,Self::PgRestore(s)=>s.fence().await}}
}
pub(crate) async fn open_qualified_pg_serving(path:&Path,url:&str)->anyhow::Result<QualifiedPgServing>{
 let root=Root::open(path)?;engine_root_fence(&root)?;
 // Any PG namespace, including missing/malformed current, forbids falling back
 // to the copied historical engine-transfer generation or ordinary SQLite.
 let serving=if exists(root.fd(),".pg-restoration-custody")? {
  QualifiedPgServing::PgRestore(open_restored_pg_serving(path,url).await?)
 }else{QualifiedPgServing::Transfer(open_engine_serving(path,url).await?)};
 serving.fence().await?;Ok(serving)
}
/// Application startup supports the current target catalogue; old authority remains backup-readable.
pub(crate) async fn open_current_qualified_pg_serving(path:&Path,url:&str)->anyhow::Result<QualifiedPgServing>{
 let serving=open_qualified_pg_serving(path,url).await?;
 let mut conn=serving.pool().acquire().await?;let profile=pg_profile_any(&mut conn).await?;drop(conn);
 if profile!=EngineCatalogueProfile::CURRENT {
  serving.pool().close().await;
  bail!("Qualified earlier serving profiles require explicit restoration into a new current55/9 target; stopped backup remains supported");
 }
 serving.fence().await?;Ok(serving)
}
#[derive(serde::Serialize)]
pub(crate) struct PgBackupReport {
 backup_id:String,bundle:PathBuf,manifest_sha256:String,source_authority:&'static str,
 compatible_catalogue:&'static str,writers_stopped_asserted:bool,
}
pub(crate) async fn backup_qualified_postgres(source:&Path,target:&Path,url:&str,writers_stopped:bool)->anyhow::Result<PgBackupReport>{
 ensure!(writers_stopped,"Stopped writers must be explicitly asserted");
 ensure!(source.is_absolute()&&target.is_absolute(),"Absolute source and new backup paths required");
 let serving=open_qualified_pg_serving(source,url).await?;
 let recording=PgByteBackupStore::configured(&serving,target)?;
 let(kind,backup)=match &serving {
  QualifiedPgServing::Transfer(s)=>("engine_transfer",match &recording{Some(store)=>capture_owned_postgres_recorded(s,target,true,store).await?,None=>capture_owned_postgres(s,target,true).await?}),
  QualifiedPgServing::PgRestore(s)=>("pg_restore",match &recording{Some(store)=>capture_restored_postgres_recorded(s,target,true,store).await?,None=>capture_restored_postgres(s,target,true).await?}),
 };
 backup.fence()?;serving.fence().await?;
 let m:PgSnapshotManifest=serde_json::from_slice(&backup.manifest)?;let mut conn=serving.pool().acquire().await?;let source_profile=pg_profile_any(&mut conn).await?;drop(conn);
 let report=PgBackupReport{backup_id:m.backup_id,bundle:backup.path().to_owned(),manifest_sha256:digest(&backup.manifest),source_authority:kind,compatible_catalogue:source_profile.catalogue_label(),writers_stopped_asserted:true};
 serving.pool().close().await;backup.fence()?;Ok(report)
}
#[derive(serde::Serialize)]
pub(crate) struct PgRestoreReport {
 backup_id:String,restore_id:String,custody_generation_id:String,destination:PathBuf,
 target:EngineTargetIdentity,compatible_catalogue:&'static str,
 fresh_startup_required:bool,derived_search_reindex_required:bool,
}
pub(crate) async fn restore_qualified_postgres(bundle:&Path,target:&Path,url:&str,writers_stopped:bool)->anyhow::Result<PgRestoreReport>{
 ensure!(writers_stopped,"Stopped source writers must be explicitly asserted");
 ensure!(bundle.is_absolute()&&target.is_absolute(),"Absolute bundle and new destination paths required");
 ensure!(url.starts_with("postgres://")||url.starts_with("postgresql://"),"Explicit PostgreSQL configuration required");
 sqlx::any::install_default_drivers();
 let snapshot=inspect_postgres_snapshot(bundle)?;snapshot.held.fence()?;
 let prepared=prepare_postgres_restore(snapshot,target)?;
 let imported=import_postgres_restore(prepared,url).await?;
 let classified=classify_postgres_restore(imported,url).await?;
 let materialized=materialize_postgres_restore(classified)?;
 let archived=archive_postgres_displaced(materialized)?;
 let sealed=seal_postgres_custody(archived)?;
 let committed=commit_postgres_custody(sealed,url).await?;
 let retired=retire_postgres_custody(committed)?;
 let final_ready=finalize_postgres_custody(retired,url).await?;
 let committed=&final_ready.retired.committed;
 let e:PgTargetAttestation=serde_json::from_slice(&committed.attestation)?;
 let imported=&committed.sealed.archived.materialized.owner.imported;
 imported.prepared.snapshot.held.fence()?;imported.prepared.destination.fence()?;
 Ok(PgRestoreReport{backup_id:e.backup_id,restore_id:e.restore_id,custody_generation_id:e.custody_generation_id,destination:imported.prepared.destination.root.absolute.clone(),target:e.target,compatible_catalogue:EngineCatalogueProfile::CURRENT.catalogue_label(),fresh_startup_required:true,derived_search_reindex_required:true})
}
