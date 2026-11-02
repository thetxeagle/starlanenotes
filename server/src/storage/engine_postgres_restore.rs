// Candidate-only PG snapshot -> marked new root + independently migrated private schema.
// Preparation/import tokens cannot become serving authority.
pub(crate) struct PreparedPgRestore {
 snapshot:InspectedPgSnapshot,destination:OwnedPgBackup,restore_id:String,raw_intent:Vec<u8>,archive:String,
}
impl PreparedPgRestore {
 fn fence(&self)->anyhow::Result<()> {self.snapshot.held.fence()?;self.destination.fence()?;
  ensure!(self.destination.roles.contains_key(".starlane-restore-incomplete"),"PG restore marker absent");Ok(())}
 fn path(&self)->&Path{self.destination.path()}
}
struct ImportedPgRestore {prepared:PreparedPgRestore,target:EngineTargetIdentity,raw_commitment:Vec<u8>,expected_target_tables:BTreeMap<String,Table>}
fn pg_restore_owned_dir(root:&Root,dirs:&BTreeMap<String,File>,path:&str)->anyhow::Result<File>{
 let held=dirs.get(path).context("PG restore parent was not created by this preparation")?;
 ensure!(identity(&destination_directory(root.fd(),path)?)?==identity(held)?,"PG restore owned directory replaced");Ok(held.try_clone()?)
}
fn pg_restore_new_write(root:&Root,dirs:&BTreeMap<String,File>,p:&str,raw:&[u8],roles:&mut BTreeMap<String,PgCaptureHeld>)->anyhow::Result<()> {
 let(parent,name)=p.rsplit_once('/').unwrap_or(("",p));let d=pg_restore_owned_dir(root,dirs,parent)?;
 let mut f=open_at(&d,name,libc::O_RDWR|libc::O_CREAT|libc::O_EXCL)?;use std::io::Write;f.write_all(raw)?;f.sync_all()?;d.sync_all()?;
 ensure!(identity(&destination_directory(root.fd(),parent)?)?==identity(&d)?,"PG restore parent changed during private write");
 roles.insert(p.into(),PgCaptureHeld{stamp:physical_stamp(&f)?,bytes:planned_hash(&f)?,held:f});Ok(())
}
fn pg_restore_output_parent(snapshot:&InspectedPgSnapshot,parent:&Root)->anyhow::Result<()> {
 let mut excluded=std::collections::BTreeSet::from([(snapshot.manifest.source_root.device,snapshot.manifest.source_root.inode),snapshot.held.root_identity]);
 for entry in snapshot.manifest.files.0.values(){if let PgSnapshotEntry::Directory{device,inode}=entry{excluded.insert((*device,*inode));}}
 for (held,_) in snapshot.held.directories.values(){excluded.insert(identity(held)?);}
 for held in &parent.chain {ensure!(!excluded.contains(&identity(held)?),"PG restore target ancestry overlaps captured source or held snapshot custody");}
 parent.check()?;snapshot.held.fence()?;Ok(())
}
fn prepare_postgres_restore(snapshot:InspectedPgSnapshot,path:&Path)->anyhow::Result<PreparedPgRestore> {
 snapshot.held.fence()?;ensure!(path.is_absolute()&&!path.starts_with(snapshot.held.path())&&!snapshot.held.path().starts_with(path),"PG restore snapshot/destination overlap");
 let parent=Root::open(path.parent().context("PG restore parent missing")?)?;engine_checkpoint("pg-restore-parent-held")?;pg_restore_output_parent(&snapshot,&parent)?;
 let name=path.file_name().and_then(|s|s.to_str()).context("PG restore name invalid")?.to_owned();
 pg_restore_output_parent(&snapshot,&parent)?;let created=pg_capture_new_dir(parent.fd(),&name)?;pg_restore_output_parent(&snapshot,&parent)?;let root=Root::open(path)?;ensure!(identity(root.fd())?==identity(&created)?,"PG restore root replaced after reservation");
 let root_id=identity(root.fd())?;let restore_id=uuid::Uuid::new_v4().to_string();let archive=format!(".pg-restoration-custody/{restore_id}");
 let mut dirs=BTreeMap::from([("".to_owned(),root.fd().try_clone()?)]);
 let mut roles=BTreeMap::new();pg_restore_new_write(&root,&dirs,".starlane-restore-incomplete",format!("starlane private copy {}\n",snapshot.manifest.backup_id).as_bytes(),&mut roles)?;
 let dir=pg_capture_new_dir(root.fd(),".pg-restoration-custody")?;let archived=pg_capture_new_dir(&dir,&restore_id)?;dirs.insert(".pg-restoration-custody".into(),dir);dirs.insert(archive.clone(),archived);
 let raw_intent=serde_json::to_vec(&serde_json::json!({"version":1,"phase":"marked-pg-restore-before-target-sql","restore_id":restore_id,"backup_id":snapshot.manifest.backup_id,"snapshot_manifest_sha256":digest(&snapshot.held.manifest),"source_target":snapshot.manifest.source_target,"source_root_device":snapshot.manifest.source_root.device,"source_root_inode":snapshot.manifest.source_root.inode,"target_root_device":root_id.0,"target_root_inode":root_id.1,"reserved_target_schema":format!("restore_{}",uuid::Uuid::parse_str(&restore_id)?.simple()),"serving_allowed":false}))?;
 pg_restore_new_write(&root,&dirs,&format!("{archive}/pre-import-intent.json"),&raw_intent,&mut roles)?;
 engine_checkpoint("pg-restore-intent")?;
 // All source descriptor roles were validated before the first target write.
 // Each leaf is copied into a new private inode; captured aliases are NOT authority.
 for(p,e)in &snapshot.manifest.files.0 {
  match e {
   PgSnapshotEntry::Directory{..}=>{
    if dirs.contains_key(p){ensure!(p==".pg-restoration-custody","Captured directory collides with new owned restore intent");pg_restore_owned_dir(&root,&dirs,p)?;}
    else{let(parent,name)=p.rsplit_once('/').unwrap_or(("",p));let d=pg_restore_owned_dir(&root,&dirs,parent)?;let created=pg_capture_new_dir(&d,name)?;dirs.insert(p.clone(),created);}
   },
   PgSnapshotEntry::File{length,sha256,..}=>{
    let held=&snapshot.held.roles[&format!("files/{p}")];let before=physical_stamp(&held.held)?;
    let(parent,name)=p.rsplit_once('/').unwrap_or(("",p));let d=pg_restore_owned_dir(&root,&dirs,parent)?;let mut out=open_at(&d,name,libc::O_RDWR|libc::O_CREAT|libc::O_EXCL)?;
    let mut offset=0;let mut buf=[0u8;65536];while offset<*length{let wanted=(*length-offset).min(buf.len()as u64)as usize;let n=held.held.read_at(&mut buf[..wanted],offset)?;ensure!(n>0,"Snapshot truncated while restoring");use std::io::Write;out.write_all(&buf[..n])?;offset+=n as u64;}
    out.sync_all()?;d.sync_all()?;let bytes=planned_hash(&out)?;ensure!(bytes.size_bytes==*length&&bytes.sha256==*sha256&&physical_stamp(&held.held)?==before,"PG restore copied role differs");roles.insert(p.clone(),PgCaptureHeld{stamp:physical_stamp(&out)?,held:out,bytes});
   }
  }snapshot.held.fence()?;
 }
 for p in ["manifest.json","application-rows.json","postgres-catalog.json","postgres-migrations.json","engine-control-rows.json"] {
  let source=&snapshot.held.roles[p];let raw=pg_snapshot_read(&snapshot.held.root,p,64*1024*1024,&mut BTreeMap::new())?;
  ensure!(digest(&raw)==source.bytes.sha256,"PG archive source role changed");pg_restore_new_write(&root,&dirs,&format!("{archive}/{p}"),&raw,&mut roles)?;
 }
 let(_,directories)=engine_walk(root.fd(),"")?;ensure!(directories.keys().cloned().collect::<std::collections::BTreeSet<_>>()==dirs.keys().filter(|p|!p.is_empty()).cloned().collect(),"Unknown PG restore directory");for(p,(d,_))in &directories{ensure!(identity(d)?==identity(&dirs[p])?,"PG restore created directory replaced");}
 let destination=OwnedPgBackup{root_identity:root_id,root_stamp:physical_stamp(root.fd())?,root,parent,name,manifest:raw_intent.clone(),roles,directories};
 let prepared=PreparedPgRestore{snapshot,destination,restore_id,raw_intent,archive};prepared.fence()?;engine_checkpoint("pg-restore-prepared")?;Ok(prepared)
}
async fn import_postgres_restore(prepared:PreparedPgRestore,url:&str)->anyhow::Result<ImportedPgRestore> {
 prepared.fence()?;let schema=format!("restore_{}",uuid::Uuid::parse_str(&prepared.restore_id)?.simple());let quoted_schema=quoted(&schema)?;
 let mut conn=sqlx::PgConnection::connect(url).await?;
 sqlx::query(&format!("CREATE SCHEMA {quoted_schema}")).execute(&mut conn).await?;sqlx::query(&format!("SET search_path TO {quoted_schema}")).execute(&mut conn).await?;
 let selected=schema.clone();let pool=sqlx::any::AnyPoolOptions::new().max_connections(1).after_connect(move|c,_|{let s=selected.clone();Box::pin(async move{sqlx::query(&format!("SET search_path TO \"{s}\"")).execute(c).await?;Ok(())})}).connect(url).await?;
 crate::db::migrate_selected(&pool,crate::db::DatabaseEngine::Postgres).await?;pool.close().await;
 sqlx::query("CREATE TABLE pg_restoration_imports (restore_id TEXT PRIMARY KEY NOT NULL, commitment_json TEXT NOT NULL, commitment_sha256 TEXT NOT NULL)").execute(&mut conn).await?;
 let target=engine_target_identity(&mut conn,&schema).await?;
 let before_catalog=catalog(&mut conn,&schema).await?;
 let migration_rows:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(version,description,(extract(epoch from installed_on)::numeric)::text,success,encode(checksum,'hex'),execution_time)::text FROM _sqlx_migrations ORDER BY version").fetch_all(&mut conn).await?;let target_profile=authenticated_pg_profile(&migration_rows)?;ensure!(target_profile==EngineCatalogueProfile::CURRENT,"Independent selected target7 required");
 let target_shape=shape_for_profile(target_profile)?;let source_profile=prepared.snapshot.profile;let expected_target_tables=target_import_rows(source_profile,target_profile,&prepared.snapshot.tables,&target_shape)?;target_relation_census(&mut conn,&schema,&target_shape,"pg_restoration_imports").await?;let baseline=pg_tables(&mut conn,&target_shape).await?;
 validate_fresh_import_baseline(target_profile,&baseline)?;
 let foreign:Vec<(String,String,bool,bool)>=sqlx::query_as("SELECT c.relname::text,k.conname::text,k.condeferrable,k.condeferred FROM pg_catalog.pg_constraint k JOIN pg_catalog.pg_class c ON c.oid=k.conrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND k.contype='f' ORDER BY c.relname,k.conname").bind(&schema).fetch_all(&mut conn).await?;
 prepared.fence()?;let mut tx=conn.begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await?;sqlx::query("SET LOCAL synchronous_commit=on").execute(&mut *tx).await?;let all=target_shape.keys().map(|n|quoted(n)).collect::<anyhow::Result<Vec<_>>>()?.join(",");sqlx::query(&format!("LOCK TABLE {all} IN SHARE ROW EXCLUSIVE MODE")).execute(&mut *tx).await?;ensure!(catalog(&mut tx,&schema).await?==before_catalog,"Fresh target catalogue changed before first import mutation");target_relation_census(&mut tx,&schema,&target_shape,"pg_restoration_imports").await?;
 for(t,c,_,_)in &foreign{sqlx::query(&format!("ALTER TABLE {} ALTER CONSTRAINT {} DEFERRABLE INITIALLY DEFERRED",quoted(t)?,quoted(c)?)).execute(&mut *tx).await?;}sqlx::query("SET CONSTRAINTS ALL DEFERRED").execute(&mut *tx).await?;
 for(n,t)in &baseline{if !t.rows.is_empty(){sqlx::query(&format!("DELETE FROM {}",quoted(n)?)).execute(&mut *tx).await?;}}
 for(n,t)in &expected_target_tables {
  let cols=t.columns.iter().map(|c|quoted(&c.name)).collect::<anyhow::Result<Vec<_>>>()?.join(",");let binds=(1..=t.columns.len()).map(|i|format!("${i}")).collect::<Vec<_>>().join(",");let sql=format!("INSERT INTO {} ({cols}) VALUES ({binds})",quoted(n)?);
  for row in &t.rows {let mut q=sqlx::query(&sql);for(c,v)in t.columns.iter().zip(row){q=match v{Cell::Null=>match c.kind.as_str(){"TEXT"=>q.bind(Option::<String>::None),"BIGINT"=>q.bind(Option::<i64>::None),"BYTEA"=>q.bind(Option::<Vec<u8>>::None),_=>bail!("Invalid PG snapshot null class")},Cell::Text(v)=>q.bind(std::str::from_utf8(v)?),Cell::Integer(v)=>q.bind(*v),Cell::Blob(v)=>q.bind(v),Cell::Real(_)=>bail!("Unsupported PG snapshot real")};}q.execute(&mut *tx).await.with_context(||format!("Import actual PG snapshot table {n}"))?;}
 }
 #[cfg(test)] if std::env::var("STARLANE_PG_RESTORE_FAULT").ok().as_deref()==Some("relationship"){let r=sqlx::query("UPDATE attachments SET document_id='missing-pg-restore-reference'").execute(&mut *tx).await?;ensure!(r.rows_affected()>0,"Meaningful FK rollback requires actual uploads");}
 sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *tx).await?;
 for(t,c,d,deferred)in &foreign{let timing=if *d{if *deferred{"DEFERRABLE INITIALLY DEFERRED"}else{"DEFERRABLE INITIALLY IMMEDIATE"}}else{"NOT DEFERRABLE INITIALLY IMMEDIATE"};sqlx::query(&format!("ALTER TABLE {} ALTER CONSTRAINT {} {timing}",quoted(t)?,quoted(c)?)).execute(&mut *tx).await?;}
 let imported=pg_tables(&mut tx,&shape()?).await?;verify_source_target_readback(&prepared.snapshot.tables,&expected_target_tables,&imported)?;ensure!(catalog(&mut tx,&schema).await?==before_catalog,"Transactional PG restore catalogue readback differs");
 let raw_commitment=serde_json::to_vec(&serde_json::json!({"version":1,"phase":"pg-restoration-import-only","restore_id":prepared.restore_id,"snapshot_backup_id":prepared.snapshot.manifest.backup_id,"snapshot_manifest_sha256":digest(&prepared.snapshot.held.manifest),"source_target":prepared.snapshot.manifest.source_target,"target":target,"target_root_device":prepared.destination.root_identity.0,"target_root_inode":prepared.destination.root_identity.1,"pre_import_intent_sha256":digest(&prepared.raw_intent),"imported_typed_sha256":engine_typed_hash(&imported)?,"target_catalogue_sha256":digest(&serde_json::to_vec(&before_catalog)?),"independent_migrations_sha256":digest(&serde_json::to_vec(&migration_rows)?),"serving_allowed":false}))?;
 sqlx::query("INSERT INTO pg_restoration_imports (restore_id,commitment_json,commitment_sha256) VALUES ($1,$2,$3)").bind(&prepared.restore_id).bind(std::str::from_utf8(&raw_commitment)?).bind(digest(&raw_commitment)).execute(&mut *tx).await?;
 ensure!(engine_target_identity(&mut tx,&schema).await?==target,"PG restore target changed before commit");prepared.fence()?;tx.commit().await?;conn.close().await?;engine_checkpoint("pg-restore-import-committed")?;
 let mut read=sqlx::PgConnection::connect(url).await?;sqlx::query(&format!("SET search_path TO {quoted_schema}")).execute(&mut read).await?;let mut rt=read.begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY").execute(&mut *rt).await?;
 ensure!(engine_target_identity(&mut rt,&schema).await?==target&&engine_typed_hash(&pg_tables(&mut rt,&shape()?).await?)?==engine_typed_hash(&expected_target_tables)?&&catalog(&mut rt,&schema).await?==before_catalog,"Fresh PG import readback differs");
 let rows:Vec<(String,String,String)>=sqlx::query_as("SELECT restore_id,commitment_json,commitment_sha256 FROM pg_restoration_imports ORDER BY restore_id").fetch_all(&mut *rt).await?;ensure!(rows==vec![(prepared.restore_id.clone(),std::str::from_utf8(&raw_commitment)?.into(),digest(&raw_commitment))],"Fresh PG restore commitment differs");rt.rollback().await?;read.close().await?;prepared.fence()?;
 Ok(ImportedPgRestore{prepared,target,raw_commitment,expected_target_tables})
}
#[cfg(test)]
mod postgres_restore_tests {
 use super::*;
 fn record(path:&Path,value:serde_json::Value)->anyhow::Result<()> {use std::io::Write;use std::os::unix::fs::OpenOptionsExt;let mut f=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;f.write_all(&serde_json::to_vec(&value)?)?;f.sync_all()?;Ok(())}
 #[tokio::test]
 #[ignore="explicit genuine PG snapshot and new owned root/schema controls required"]
 async fn required_actual_postgres_snapshot_import()->anyhow::Result<()> {
  sqlx::any::install_default_drivers();
  let mode=std::env::var("STARLANE_PG_RESTORE_MODE").expect("explicit import/refuse mode required");
  let root=PathBuf::from(std::env::var("STARLANE_PG_RESTORE_ROOT").expect("new disposable restoration root required"));
  if mode=="held-refuse" {
   let url=std::env::var("STARLANE_ENGINE_PG_URL").expect("owned PG URL required");let fault=std::env::var("STARLANE_PG_RESTORE_FAULT").expect("held fault required");let serving=open_restored_pg_serving(&root,&url).await?;let mut held=serving.pool.acquire().await?;
   let path=root.join(".pg-restoration-custody/current.json");
   if fault=="write-revert"{let bytes=std::fs::read(&path)?;let mut altered=bytes.clone();altered[0]^=1;std::fs::write(&path,altered)?;std::fs::write(&path,bytes)?;}else if fault=="alias"{std::fs::hard_link(&path,root.join("owned-unapproved-proof-alias"))?;}else{bail!("Unknown held fault");}
   ensure!(serving.proof.physical_fence().is_err(),"Retained immutable proof mutation survived");ensure!(serving.proof.verify_any_connection(&mut held).await.is_err(),"Already-held actual connection escaped proof fence");
   drop(held);ensure!(serving.fence().await.is_err(),"Actual AnyPool checkout survived held mutation");ensure!(super::super::fence(&root).is_err(),"Active root fence survived proof mutation");serving.pool.close().await;println!("ACTUAL_HELD_PROOF_AND_CHECKOUT_MUTATION_REFUSED");return Ok(());
  }
  if mode=="custody-admit" {let url=std::env::var("STARLANE_ENGINE_PG_URL").expect("owned PG URL required");let raw=std::fs::read(root.join(".pg-restoration-custody/current.json"))?;let e:PgTargetAttestation=serde_json::from_slice(&raw)?;let mut conn=sqlx::PgConnection::connect(&url).await?;sqlx::query(&format!("SET search_path TO {}",quoted(&e.target.schema_name)?)).execute(&mut conn).await?;let proof=admit_postgres_restore(&root,&mut conn).await?;proof.verify_connection(&mut conn).await?;conn.close().await?;let mut any=sqlx::AnyConnection::connect(&url).await?;sqlx::query(&format!("SET search_path TO {}",quoted(&e.target.schema_name)?)).execute(&mut any).await?;proof.verify_any_connection(&mut any).await?;any.close().await?;ensure!(super::super::bootstrap_gate(&root).is_err(),"Plain namespace admitted");println!("FRESH_OPAQUE_PGRESTORE_NATIVE_AND_ANY_PROOF_PASS_NO_ACTIVE");return Ok(());}
  if mode=="custody-accept" {let url=std::env::var("STARLANE_ENGINE_PG_URL").expect("owned PG URL required");inspect_postgres_restore_custody(&root,&url).await?;ensure!(super::super::bootstrap_gate(&root).is_err(),"Ordinary bootstrap bypassed private PG admission");println!("FRESH_FINALIZED_PG_FULL_HISTORY_QUALIFIED_NO_ACTIVE_GRANT");return Ok(());}
  if mode=="custody-refuse" {let url=std::env::var("STARLANE_ENGINE_PG_URL").expect("owned PG URL required");ensure!(!root.join(".starlane-restore-incomplete").exists(),"Retired-marker fresh fixture required");let error=inspect_postgres_restore_custody(&root,&url).await.expect_err("Missing finalization admitted");ensure!(error.to_string().contains("No such file"),"Wrong fresh finalization refusal: {error:?}");ensure!(super::super::bootstrap_gate(&root).is_err(),"Incomplete target admitted by ordinary bootstrap");println!("FRESH_SQL_COMMITTED_MARKER_RETIRED_MISSING_FINALIZATION_REFUSED");return Ok(());}
  if mode=="refuse" {ensure!(super::super::bootstrap_gate(&root).is_err(),"Incomplete PG root admitted by ordinary fresh bootstrap");ensure!(root.join(".starlane-restore-incomplete").is_file(),"Restoration marker not retained");println!("FRESH_INCOMPLETE_PG_RESTORE_REFUSED");return Ok(());}
  ensure!(mode=="import"||mode=="preparation-refuse","Unknown PG restoration mode");
  let source=PathBuf::from(std::env::var("STARLANE_PG_SNAPSHOT_PATH").expect("genuine retained PG snapshot required"));
  if mode=="preparation-refuse" {
   let snapshot=inspect_postgres_snapshot(&source)?;ensure!(!root.exists(),"Refusal target must be genuinely absent");
   let result=prepare_postgres_restore(snapshot,&root);let error=match result{Err(e)=>e,Ok(_)=>bail!("Unsafe target preparation accepted")};
   let expected=std::env::var("STARLANE_PG_RESTORE_EXPECT_REFUSAL").unwrap_or_else(|_|"PG restore target ancestry overlaps captured source or held snapshot custody".into());
   ensure!(error.to_string().contains(&expected)&&!root.exists(),"Unsafe target refusal changed target or missed intended boundary: {error:?}");
   println!("PHYSICAL_PG_RESTORE_OUTPUT_REFUSED_BEFORE_TARGET_OR_SQL");return Ok(());
  }
  let url=std::env::var("STARLANE_ENGINE_PG_URL").expect("owned PG URL required");
  let output=PathBuf::from(std::env::var("STARLANE_PG_RESTORE_RECORD").expect("new private record required"));
  let snapshot=inspect_postgres_snapshot(&source)?;let prepared=prepare_postgres_restore(snapshot,&root)?;
  let restore_id=prepared.restore_id.clone();let schema=format!("restore_{}",uuid::Uuid::parse_str(&restore_id)?.simple());
  let result=import_postgres_restore(prepared,&url).await;
  if std::env::var("STARLANE_PG_RESTORE_FAULT").ok().as_deref()==Some("relationship") {
   ensure!(result.is_err(),"Invalid relationship was committed");let mut c=sqlx::PgConnection::connect(&url).await?;sqlx::query(&format!("SET search_path TO {}",quoted(&schema)?)).execute(&mut c).await?;
   let count:i64=sqlx::query_scalar("SELECT count(*) FROM pg_restoration_imports").fetch_one(&mut c).await?;let assets:i64=sqlx::query_scalar("SELECT count(*) FROM attachments").fetch_one(&mut c).await?;ensure!(count==0&&assets==0,"Failed PG restoration leaked import rows");
   record(&output,serde_json::json!({"restore_id":restore_id,"schema":schema,"relationship_rollback":true,"serving_allowed":false}))?;c.close().await?;println!("GENUINE_PG_SNAPSHOT_FK_ROLLBACK_PASS");return Ok(());
  }
  let classified=classify_postgres_restore(result?,&url).await?;ensure!(!classified.scopes.is_empty(),"Genuine owner classification required");let owner_scope_count=classified.scopes.len();let owner_binding_count=classified.scopes.iter().map(|s|s.bindings.len()).sum::<usize>();let denied_count=classified.denied.len();
  let archived=archive_postgres_displaced(materialize_postgres_restore(classified)?)?;ensure!(!archived.materialized.displaced.is_empty(),"Genuine certified topology required");let displaced_count=archived.materialized.displaced.len();
  let fault=std::env::var("STARLANE_PG_RESTORE_FAULT").unwrap_or_default();
  if fault=="schema"||fault=="migrations" {
   let mut c=sqlx::PgConnection::connect(&url).await?;let target=&archived.materialized.owner.imported.target;sqlx::query(&format!("SET search_path TO {}",quoted(&target.schema_name)?)).execute(&mut c).await?;let old=pg_custody_rows(&mut c).await?;
   if fault=="schema"{sqlx::query("ALTER TABLE attachments DROP CONSTRAINT attachments_fk_2").execute(&mut c).await?;}else{sqlx::query("UPDATE _sqlx_migrations SET checksum=decode(repeat('00',48),'hex') WHERE version=1").execute(&mut c).await?;}c.close().await?;
   let error=match commit_postgres_custody(seal_postgres_custody(archived)?,&url).await {Err(e)=>e,Ok(_)=>bail!("Altered current PG schema/migrations adopted")};let expected=if fault=="schema"{"Actual imported PG schema changed before custody"}else{"Actual imported independent PG migrations changed before custody"};ensure!(error.to_string().contains(expected),"Wrong importer pin refusal: {error:?}");
   let mut c=sqlx::PgConnection::connect(&url).await?;sqlx::query(&format!("SET search_path TO {}",quoted(&schema)?)).execute(&mut c).await?;ensure!(pg_custody_rows(&mut c).await?==old,"Rejected shape mutation changed custody SQL");let tables:i64=sqlx::query_scalar("SELECT count(*) FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname IN ('pg_restoration_generations','pg_restoration_current')").bind(&schema).fetch_one(&mut c).await?;ensure!(tables==0,"Rejected control shape created serving control tables");c.close().await?;
   ensure!(root.join(".starlane-restore-incomplete").is_file(),"Rejected shape lost marker");record(&output,serde_json::json!({"fault":fault,"custody_sql_unchanged":true,"new_control_tables_absent":true,"serving_allowed":false}))?;println!("GENUINE_PRE_CUSTODY_SCHEMA_MIGRATION_MUTATION_REFUSED");return Ok(());
  }
  let committed=commit_postgres_custody(seal_postgres_custody(archived)?,&url).await?;let retired=retire_postgres_custody(committed)?;
  if std::env::var("STARLANE_PG_RESTORE_FINALIZE").ok().as_deref()==Some("1") {
   let finalized=finalize_postgres_custody(retired,&url).await?;let raw=std::fs::read(root.join(".pg-restoration-custody/current.json"))?;let e:PgTargetAttestation=serde_json::from_slice(&raw)?;let mut conn=sqlx::PgConnection::connect(&url).await?;sqlx::query(&format!("SET search_path TO {}",quoted(&e.target.schema_name)?)).execute(&mut conn).await?;let proof=admit_postgres_restore(&root,&mut conn).await?;proof.physical_fence()?;conn.close().await?;let sealed=&finalized.retired.committed.sealed;let imported=&sealed.archived.materialized.owner.imported;
   imported.prepared.snapshot.held.fence()?;imported.prepared.destination.fence()?;
   for(p,r)in &imported.prepared.snapshot.held.roles{if let Some(relative)=p.strip_prefix("files/"){let target=if relative==".restoration-custody/current.json"{format!(".restoration-custody/archive/{}/ledger.json",imported.prepared.snapshot.history.current_id)}else if relative==".pg-restoration-custody/current.json"{match &imported.prepared.snapshot.manifest.source_authority{Some(PgCapturedSourceAuthority::PgRestore{generation_id,..})=>format!(".pg-restoration-custody/{generation_id}/source-attestation.json"),None=>relative.into()}}else{relative.into()};ensure!(imported.prepared.destination.roles[&target].bytes==r.bytes,"Original raw source evidence changed during finalization");}}
   record(&output,serde_json::json!({"restore_id":restore_id,"target":imported.target,"new_custody_generation":sealed.row.generation_id,"original_database_sha256":sealed.row.database_sha256,"finalization_sha256":digest(&sealed.finalization),"application_bytea_cells":imported.prepared.snapshot.tables.values().flat_map(|t|&t.rows).flat_map(|r|r).filter(|v|matches!(v,Cell::Blob(b)if !b.is_empty())).count(),"genuine_finalization_written":true,"fresh_full_history_qualified":true,"serving_allowed":false}))?;
   println!("GENUINE_PG_FINALIZATION_FULL_HISTORY_PASS_NO_ACTIVE_GRANT");return Ok(());
  }
  let imported=&retired.committed.sealed.archived.materialized.owner.imported;imported.prepared.snapshot.held.fence()?;imported.prepared.destination.fence()?;
  ensure!(!root.join(".starlane-restore-incomplete").exists()&&super::super::bootstrap_gate(&root).is_err(),"Unfinalized target gained ordinary startup");
  let final_path=format!("{}/finalization.json",retired.committed.sealed.base);ensure!(!root.join(&final_path).exists(),"Fixture finalization must remain genuinely absent");
  let error=inspect_postgres_restore_custody(&root,&url).await.expect_err("Unfinalized target qualified");ensure!(error.to_string().contains("No such file"),"Wrong actual closed-history finalization refusal: {error:?}");
  for(p,r)in &imported.prepared.snapshot.held.roles{if let Some(relative)=p.strip_prefix("files/"){let target=if relative==".restoration-custody/current.json"{format!(".restoration-custody/archive/{}/ledger.json",imported.prepared.snapshot.history.current_id)}else if relative==".pg-restoration-custody/current.json"{match &imported.prepared.snapshot.manifest.source_authority{Some(PgCapturedSourceAuthority::PgRestore{generation_id,..})=>format!(".pg-restoration-custody/{generation_id}/source-attestation.json"),None=>relative.into()}}else{relative.into()};ensure!(imported.prepared.destination.roles[&target].bytes==r.bytes,"Original raw source evidence changed during restoration");}}
  record(&output,serde_json::json!({"restore_id":restore_id,"target":imported.target,"commitment_sha256":digest(&imported.raw_commitment),"new_attestation_sha256":digest(&retired.committed.attestation),"new_custody_generation":retired.committed.sealed.row.generation_id,"original_database_sha256":retired.committed.sealed.row.database_sha256,"typed_tables":imported.prepared.snapshot.tables.len(),"displaced_independent_copies":displaced_count,"owner_scopes":owner_scope_count,"owner_bindings":owner_binding_count,"denied_reservations":denied_count,"application_bytea_cells":imported.prepared.snapshot.tables.values().flat_map(|t|&t.rows).flat_map(|r|r).filter(|v|matches!(v,Cell::Blob(b)if !b.is_empty())).count(),"actual_custody_sql_committed":true,"actual_marker_retired":true,"missing_finalization_refused":true,"serving_allowed":false,"ordinary_startup_refused":true}))?;
  println!("ACTUAL_PG_SNAPSHOT_NEW_ROOT_IMPORT_PASS typed72/BYTEA/full raw archived evidence/fresh owner scopes; not serving authority");Ok(())
 }
}
