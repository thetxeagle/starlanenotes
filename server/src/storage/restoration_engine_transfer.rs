// Candidate-only: private post-custody source token. Included in restoration::linux::engine_transfer.
// Native-current codec is the preserved lossless reader updated for the selected49/PG3 catalog.
use super::*;
include!("engine_transfer_native_current.rs");
include!("engine_historical_projection.rs");
include!("engine_verification_projection.rs");
include!("engine_sqlite_ordinary_projection.rs");
include!("engine_sqlite_ordinary_records.rs");

pub(crate) struct PreparedEngineTransfer<'a> {
    destination: PlannedDestination,
    original: NativeSnapshot<'a>,
    post_tables: BTreeMap<String,Table>,
    transfer_id: String,
    custody_generation_id: String,
    custody_ledger_sha256: String,
    source_typed_sha256: String,
    post_custody_typed_sha256: String,
    engine_directory:String,
    raw_source_intent:Vec<u8>,
}
impl PreparedEngineTransfer<'_> {
    pub(crate) fn path(&self)->&Path {self.destination.path()}
    pub(crate) fn transfer_id(&self)->&str {&self.transfer_id}
    pub(crate) fn fence(&self)->io::Result<()> {
        self.original.bundle.fence()?;
        self.destination.fence()
    }
}
fn engine_typed_hash(tables:&BTreeMap<String,Table>)->anyhow::Result<String> {
    let hashes: BTreeMap<_,_>=tables.iter().map(|(n,t)|Ok((n,(t.rows.len(),table_hash(t)?)))).collect::<anyhow::Result<_>>()?;
    Ok(digest(&serde_json::to_vec(&hashes)?))
}
fn engine_post_rows(file:&File,original:&NativeSnapshot<'_>)->anyhow::Result<BTreeMap<String,Table>> {
    let db=NativeDb::open(file)?;
    ensure!(db.rows("PRAGMA integrity_check")?==vec![vec![Cell::Text(b"ok".to_vec())]],"Post-custody integrity failed");
    ensure!(db.rows("PRAGMA foreign_key_check")?.is_empty(),"Post-custody FK violation");
    ensure!(db.rows("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name")?==original.schema_rows,"Publication changed source schema");
    ensure!(db.rows("SELECT version,description,installed_on,success,checksum,execution_time FROM _sqlx_migrations ORDER BY version")?==original.raw_sqlite_migrations,"Publication changed raw SQLite migration rows");
    let mut tables=original.tables.clone();
    for(n,t)in &mut tables {
        let columns=t.columns.iter().map(|c|quoted(&c.name)).collect::<Result<Vec<_>,_>>()?.join(",");
        t.rows=db.rows(&format!("SELECT {columns} FROM {}",quoted(n)?))?;
        for row in &t.rows {
            ensure!(row.len()==t.columns.len(),"Post-custody row width changed");
            for(cell,c)in row.iter().zip(&t.columns){validate_cell(cell,c,n)?;}
        }
        if !matches!(n.as_str(),"storage_restore_generations"|"storage_restore_current") {
            ensure!(table_hash(t)?==table_hash(&original.tables[n])?&&t.rows.len()==original.tables[n].rows.len(),"Unapproved publication row change: {n}");
        }
    }
    db.close()?;
    Ok(tables)
}
fn engine_column<'a>(t:&'a Table,row:&'a [Cell],name:&str)->anyhow::Result<&'a Cell> {
    let i=t.columns.iter().position(|c|c.name==name).context("Missing custody column")?;
    row.get(i).context("Missing custody cell")
}
fn engine_text_column<'a>(t:&'a Table,row:&'a [Cell],name:&str)->anyhow::Result<&'a str> {
    text(engine_column(t,row,name)?)
}
pub(crate) fn prepare_engine_transfer<'a>(
    bundle:&'a crate::storage::backup_inventory::ValidatedBundle,
    ready:BootReadyDestination,
)->anyhow::Result<PreparedEngineTransfer<'a>> {
    ready.fence()?;bundle.fence()?;
    let d=&ready.destination;
    ensure!(d.plan.backup_id()==bundle.manifest.backup_id
        &&d.plan.manifest_sha256()==digest(&bundle.raw_manifest)
        &&d.plan.database_sha256()==bundle.manifest.files.0["starlane-notes.db"].sha256
        &&d.plan.source_root()==&bundle.manifest.source_custody.root,"Completed destination belongs to a different source bundle");
    // First source gate supports completed original SQL. Pending producer SQL deltas
    // require a separately owner-classified extension; do not silently whitelist them.
    let original=preflight(bundle)?;
    let database=d.leaves.get("starlane-notes.db").context("Qualified post-publication DB absent")?;
    let post=engine_post_rows(&database.held,&original)?;
    ready.fence()?;bundle.fence()?;
    let old=&original.tables["storage_restore_generations"];let now=&post["storage_restore_generations"];
    ensure!(now.rows.len()==old.rows.len()+1,"Unexpected custody generation transition");
    let mut remaining:Vec<Vec<u8>>=now.rows.iter().map(serde_json::to_vec).collect::<Result<_,_>>()?;
    for row in &old.rows {
        let raw=serde_json::to_vec(row)?;let i=remaining.iter().position(|v|v==&raw).context("Historical custody SQL row changed")?;remaining.remove(i);
    }
    ensure!(remaining.len()==1,"Ambiguous new custody generation");
    let new=now.rows.iter().find(|row|engine_text_column(now,row,"generation_id").ok()==Some(ready.generation_id())).context("Actual producer generation absent")?;
    let pointer=&post["storage_restore_current"];
    ensure!(pointer.rows==vec![vec![Cell::Integer(1),Cell::Text(ready.generation_id().as_bytes().to_vec())]],"Post-custody current pointer mismatch");
    let raw=read_ledger(&d.leaves.get(".restoration-custody/current.json").context("Qualified current ledger absent")?.held)?;
    let envelope:HistoryEnvelope=serde_json::from_slice(&raw)?;
    ensure!(envelope.version==3,"Real finalized v3 publisher result required");
    let ledger=envelope.runtime;validate(&ledger)?;
    let root_id=identity(d.copied.root.fd())?;
    let raw_hash=digest(&raw);
    for(name,expected)in [
        ("generation_id",ready.generation_id().to_owned()),
        ("backup_id",bundle.manifest.backup_id.clone()),
        ("manifest_sha256",digest(&bundle.raw_manifest)),
        ("database_sha256",bundle.manifest.files.0["starlane-notes.db"].sha256.clone()),
        ("ledger_sha256",raw_hash.clone()),
        ("root_device",root_id.0.to_string()),("root_inode",root_id.1.to_string()),("state","committed".into())
    ] {ensure!(engine_text_column(now,new,name)?==expected,"Producer custody SQL tuple mismatch: {name}");}
    ensure!(ledger.generation_id==ready.generation_id()&&ledger.root_device==root_id.0&&ledger.root_inode==root_id.1,"Producer ledger root mismatch");
    let source_typed_sha256=engine_typed_hash(&original.tables)?;
    let post_custody_typed_sha256=engine_typed_hash(&post)?;
    let gid=ready.generation_id().to_owned();let mut destination=ready.destination;
    publication_retired_fence(&destination)?;
    let transfer_id=uuid::Uuid::new_v4().to_string();
    let marker_bytes=format!("starlane private engine transfer {transfer_id} {} {gid}\n",bundle.manifest.backup_id).into_bytes();
    let mut marker=open_at(destination.copied.root.fd(),DESTINATION_MARKER,libc::O_WRONLY|libc::O_CREAT|libc::O_EXCL)?;
    use std::io::Write;marker.write_all(&marker_bytes)?;marker.sync_all()?;destination.copied.root.fd().sync_all()?;
    let stamp=physical_stamp(&marker)?;
    destination.copied.marker=marker;
    destination.copied.marker_bytes=marker_bytes;
    destination.marker_stamp=stamp;
    pending_directory_delta(&mut destination,"")?;
    destination.fence()?;bundle.fence()?;
    let engine_directory=format!(".engine-transfer-custody/{transfer_id}");
    ensure!(!destination.directories.contains_key(".engine-transfer-custody"),"Prior engine-transfer closure requires explicit composition protocol");
    pending_new_directory(&mut destination,&engine_directory)?;
    let marker_id=identity(&destination.copied.marker)?;
    let raw_source_intent=serde_json::to_vec(&serde_json::json!({
      "version":1,"phase":"engine-source-reserved-before-pg-import",
      "transfer_id":transfer_id,"reserved_schema":format!("transfer_{}",uuid::Uuid::parse_str(&transfer_id)?.simple()),
      "source_backup_id":bundle.manifest.backup_id,"source_manifest_sha256":digest(&bundle.raw_manifest),
      "source_sqlite_database_sha256":bundle.manifest.files.0["starlane-notes.db"].sha256,
      "source_typed_sha256":source_typed_sha256,"post_custody_typed_sha256":post_custody_typed_sha256,
      "custody_generation_id":gid,"custody_ledger_sha256":raw_hash,
      "root_device":root_id.0,"root_inode":root_id.1,
      "marker_device":marker_id.0,"marker_inode":marker_id.1,
      "marker_sha256":digest(&destination.copied.marker_bytes),"serving_allowed":false
    }))?;
    pending_new_file(&mut destination,&format!("{engine_directory}/source-intent.json"),&raw_source_intent)?;
    destination.fence()?;bundle.fence()?;
    engine_checkpoint("source-intent")?;
    Ok(PreparedEngineTransfer{destination,original,post_tables:post,transfer_id,
        custody_generation_id:gid,custody_ledger_sha256:raw_hash,source_typed_sha256,post_custody_typed_sha256,engine_directory,raw_source_intent})
}
#[derive(Debug,Clone,PartialEq,Eq,Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EngineTargetIdentity {
    cluster_system_identifier:String,database_oid:i64,schema_oid:i64,schema_name:String,
}
async fn engine_target_identity(conn:&mut sqlx::PgConnection,schema:&str)->anyhow::Result<EngineTargetIdentity> {
    use sqlx::Row;
    let row=sqlx::query("SELECT s.system_identifier::text AS cluster,d.oid::bigint AS database_oid,n.oid::bigint AS schema_oid,current_schema()::text AS selected,current_setting('server_encoding') AS server_encoding,current_setting('client_encoding') AS client_encoding FROM pg_control_system() s CROSS JOIN pg_catalog.pg_database d JOIN pg_catalog.pg_namespace n ON n.nspname=$1 WHERE d.datname=current_database()")
        .bind(schema).fetch_one(conn).await.context("Target role cannot prove PostgreSQL cluster identity")?;
    ensure!(row.try_get::<String,_>("selected")?==schema&&row.try_get::<String,_>("server_encoding")?=="UTF8"&&row.try_get::<String,_>("client_encoding")?=="UTF8","Engine target schema/encoding mismatch");
    let cluster:String=row.try_get("cluster")?;ensure!(cluster.parse::<u64>()?!=0,"Invalid PG system identifier");
    Ok(EngineTargetIdentity{cluster_system_identifier:cluster,database_oid:row.try_get("database_oid")?,schema_oid:row.try_get("schema_oid")?,schema_name:schema.into()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore="explicit genuine captured source and new workspace required"]
    async fn required_post_custody_source_gate() -> anyhow::Result<()> {
        sqlx::any::install_default_drivers();
        let input=PathBuf::from(std::env::var("STARLANE_ENGINE_SOURCE_BUNDLE").expect("genuine bundle required"));
        let workspace=PathBuf::from(std::env::var("STARLANE_ENGINE_SOURCE_WORKSPACE").expect("new workspace required"));
        let mode=std::env::var("STARLANE_ENGINE_SOURCE_MODE").expect("explicit mode required");
        ensure!(input.is_absolute()&&workspace.is_absolute(),"Absolute fixture roots required");
        ensure!(matches!(mode.as_str(),"positive"|"wrong-pair"|"write-revert"),"Unknown fixture mode");
        std::fs::create_dir(&workspace).unwrap();
        use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&workspace,std::fs::Permissions::from_mode(0o700)).unwrap();
        let bundle=crate::storage::backup_inventory::verify(&input).await.unwrap();
        let plan=crate::storage::restoration_plan::plan(&bundle).await.unwrap();
        let copied=reserve_and_copy(&bundle,&workspace.join("destination")).unwrap();
        let planned=materialize_groups(copied,&bundle,plan).unwrap();
        let sealed=seal_publication(&bundle,planned).await.unwrap();
        let committed=commit_publication(sealed).await.unwrap();
        let ready=retire_publication(committed).unwrap();
        ready.fence().unwrap();
        if mode=="wrong-pair" {
            let other=PathBuf::from(std::env::var("STARLANE_ENGINE_OTHER_BUNDLE").expect("different genuine bundle required"));
            let other=crate::storage::backup_inventory::verify(&other).await.unwrap();
            let error=match prepare_engine_transfer(&other,ready){Ok(_)=>panic!("Wrong genuine source pair admitted"),Err(e)=>e};
            ensure!(format!("{error:#}").contains("different source bundle"),"Wrong refusal");
            ensure!(!workspace.join("destination").join(DESTINATION_MARKER).exists(),"Pairing refusal must precede re-marking");
            other.fence().unwrap();other.db.close().await;
        } else {
            let prepared=prepare_engine_transfer(&bundle,ready).unwrap();
            prepared.fence().unwrap();
            ensure!(bootstrap_gate(prepared.path()).is_err(),"Engine preparation must block ordinary startup");
            let proof=prepared.path().join(".restoration-custody/current.json");
            if mode=="write-revert" {
                let raw=std::fs::read(&proof).unwrap();
                std::fs::write(&proof,&raw).unwrap();
                ensure!(prepared.fence().is_err(),"Same inode/byte restoration must not refresh qualified evidence");
            }
            std::fs::write(workspace.join("prepared-source.json"),serde_json::to_vec_pretty(&serde_json::json!({
                "phase":"actual-post-custody-source-only","transfer_id":prepared.transfer_id(),
                "source_typed_sha256":prepared.source_typed_sha256,
                "post_custody_typed_sha256":prepared.post_custody_typed_sha256,
                "custody_generation_id":prepared.custody_generation_id,
                "custody_ledger_sha256":prepared.custody_ledger_sha256,
                "qualified_tables":prepared.post_tables.len(),
                "marker_retained":true,"pg_writes":0,"serving_allowed":false
            })).unwrap()).unwrap();
        }
        bundle.fence().unwrap();bundle.db.close().await;
        Ok(())
    }
}

include!("engine_transfer_import_current.rs");
include!("engine_transfer_seal_current.rs");
include!("engine_transfer_commit_current.rs");
include!("engine_transfer_admission_current.rs");
include!("engine_transfer_serving_current.rs");

#[cfg(test)]
mod import_tests {
 use super::*;
 #[tokio::test]
 #[ignore="explicit genuine source and private PostgreSQL fixture required"]
 async fn required_post_custody_engine_import() -> anyhow::Result<()> {
  sqlx::any::install_default_drivers();
  let input=PathBuf::from(std::env::var("STARLANE_ENGINE_IMPORT_BUNDLE").expect("genuine source required"));
  let workspace=PathBuf::from(std::env::var("STARLANE_ENGINE_IMPORT_WORKSPACE").expect("new workspace required"));
  let mode=std::env::var("STARLANE_ENGINE_IMPORT_MODE").expect("mode required");
  let url=std::env::var("STARLANE_ENGINE_PG_URL").expect("private owned PostgreSQL URL required");
  ensure!(input.is_absolute()&&workspace.is_absolute()&&matches!(mode.as_str(),"positive"|"relationship"|"write-revert-before-import"|"sealed-write-revert"|"marker-removed-before-import"|"custody-sql-tamper"|"control-schema-tamper"|"foreign-marker-exchange"),"Invalid explicit fixture controls");
  std::fs::create_dir(&workspace)?;
  use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&workspace,std::fs::Permissions::from_mode(0o700))?;
  let bundle=crate::storage::backup_inventory::verify(&input).await?;
  let plan=crate::storage::restoration_plan::plan(&bundle).await?;
  std::fs::write(workspace.join("owner-classification.json"),serde_json::to_vec_pretty(&plan)?)?;
  let copied=reserve_and_copy(&bundle,&workspace.join("destination"))?;
  let planned=materialize_groups(copied,&bundle,plan)?;
  let sealed=seal_publication(&bundle,planned).await?;
  let committed=commit_publication(sealed).await?;
  let ready=retire_publication(committed)?;
  let prepared=prepare_engine_transfer(&bundle,ready)?;prepared.fence()?;
  let artifacts=workspace.join("import-artifacts");std::fs::create_dir(&artifacts)?;std::fs::set_permissions(&artifacts,std::fs::Permissions::from_mode(0o700))?;
  if matches!(mode.as_str(),"write-revert-before-import"|"marker-removed-before-import") {
   let mut control=sqlx::PgConnection::connect(&url).await?;
   let names_before:Vec<String>=sqlx::query_scalar("SELECT nspname::text FROM pg_catalog.pg_namespace WHERE nspname LIKE 'transfer_%' ORDER BY nspname").fetch_all(&mut control).await?;
   if mode=="marker-removed-before-import" {
    std::fs::remove_file(prepared.path().join(DESTINATION_MARKER))?;
    ensure!(bootstrap_gate(prepared.path()).is_err(),"Engine reservation allowed stale SQLite fallback after marker removal");
   } else {let proof=prepared.path().join(".restoration-custody/current.json");let raw=std::fs::read(&proof)?;std::fs::write(&proof,&raw)?;}
   ensure!(prepared.fence().is_err(),"Invalidated source was refreshed");
   ensure!(commit_engine_import(prepared,&url,&artifacts).await.is_err(),"Invalid source imported");
   ensure!(!artifacts.join("retained-import-intent.json").exists(),"Invalidated source reached PG schema reservation");
   let names_after:Vec<String>=sqlx::query_scalar("SELECT nspname::text FROM pg_catalog.pg_namespace WHERE nspname LIKE 'transfer_%' ORDER BY nspname").fetch_all(&mut control).await?;ensure!(names_before==names_after,"Invalidated source created a PostgreSQL schema");control.close().await?;
  } else if mode=="relationship" {
   std::env::set_var("STARLANE_TRANSFER_IMPORT_FAULT","relationship");
   let error=match commit_engine_import(prepared,&url,&artifacts).await {Ok(_)=>panic!("Corrupt relationship committed"),Err(e)=>e};
   let sql_error=error.chain().find_map(|cause|cause.downcast_ref::<sqlx::Error>()).context("SQL refusal missing")?;
   ensure!(sql_error.as_database_error().is_some_and(|db|db.code().as_deref()==Some("23503")),"Expected actual PostgreSQL FK23503 refusal");
   let intent:serde_json::Value=serde_json::from_slice(&std::fs::read(artifacts.join("retained-import-intent.json"))?)?;
   let schema=intent["schema"].as_str().context("Retained schema absent")?;
   let before:serde_json::Value=serde_json::from_slice(&std::fs::read(artifacts.join("catalog-before-import.json"))?)?;
   let mut conn=sqlx::PgConnection::connect(&url).await?;sqlx::query(&format!("SET search_path TO {}",quoted(schema)?)).execute(&mut conn).await?;
   ensure!(serde_json::to_value(catalog(&mut conn,schema).await?)?==before["catalog"],"Rollback changed constraints/catalog");
   let shape=shape()?;let rows=pg_tables(&mut conn,&shape).await?;ensure!(serde_json::to_value(&rows)?==before["baseline"],"Rollback changed any target row");
   let controls:i64=sqlx::query_scalar("SELECT count(*) FROM engine_transfer_import_commitments").fetch_one(&mut conn).await?;ensure!(controls==0,"Rollback retained a false import commitment");
   conn.close().await?;
  } else {
   let imported=commit_engine_import(prepared,&url,&artifacts).await?;imported.fence()?;
   ensure!(bootstrap_gate(imported.path()).is_err(),"Import must retain incomplete marker and refuse ordinary startup");
   ensure!(!imported.certificate.serving_allowed&&!imported.certificate.cutover_allowed,"Import certificate grants authority");
   ensure!(imported.certificate.target_table_hashes.len()==72&&imported.certificate.source_history_rows_preserved>=1,"Actual current catalog/history missing");
   let sealed=seal_engine_import(imported)?;sealed.fence()?;
   let sealed=if matches!(mode.as_str(),"positive"|"custody-sql-tamper"|"control-schema-tamper"|"foreign-marker-exchange") {let committed=commit_engine_seal(sealed,&url).await?;committed.fence()?;ensure!(bootstrap_gate(committed.path()).is_err(),"SQL engine commit grants ordinary startup before finalization");ensure!(committed.path().join(".engine-transfer-custody").join(&committed.sealed.intent.transfer_id).join("completion.json").exists(),"Actual postclose completion missing");if mode=="foreign-marker-exchange" {
 let path=committed.path().to_owned();let schema=committed.sealed.intent.target.schema_name.clone();
 ensure!(retire_engine_commit(committed).is_err(),"Foreign physical marker retirement succeeded");
 let mut read=sqlx::PgConnection::connect(&url).await?;sqlx::query(&format!("SET search_path TO {}",quoted(&schema)?)).execute(&mut read).await?;
 ensure!(admit_engine(&path,&mut read).await.is_err(),"Failed foreign marker retirement admitted fresh reader");read.close().await?;
 std::fs::write(workspace.join("retirement-refusal.json"),b"{\"retirement_returned_error\":true,\"fresh_admission_refused\":true}")?;
 return Ok(());
}
let finalized=retire_engine_commit(committed)?;finalized.fence()?;let schema=finalized.committed.sealed.intent.target.schema_name.clone();let mut reader=sqlx::PgConnection::connect(&url).await?;sqlx::query(&format!("SET search_path TO {}",quoted(&schema)?)).execute(&mut reader).await?;let admitted=admit_engine(finalized.path(),&mut reader).await?;admitted.physical_fence()?;admitted.verify_connection(&mut reader).await?;
if mode=="custody-sql-tamper" {sqlx::query("UPDATE storage_restore_generations SET created_at=created_at || '-changed' WHERE generation_id=$1").bind(&finalized.committed.sealed.intent.custody_generation_id).execute(&mut reader).await?;ensure!(admitted.verify_connection(&mut reader).await.is_err(),"Held reader ignored immutable custody SQL mutation");}
if mode=="control-schema-tamper" {
 sqlx::query("ALTER TABLE engine_transfer_current DROP CONSTRAINT engine_transfer_current_singleton_check").execute(&mut reader).await?;
 ensure!(admitted.verify_connection(&mut reader).await.is_err(),"Held reader ignored changed control definition");
 ensure!(admit_engine(finalized.path(),&mut reader).await.is_err(),"Fresh reader ignored changed control definition");
}
reader.close().await?;finalized.committed.sealed}else{sealed};
   ensure!(bootstrap_gate(sealed.path()).is_err(),"Private engine sealing cannot retire incomplete marker");
   let roles=&sealed.intent.roles;
   ensure!(roles.len()==4&&roles["original.sqlite"].sha256==bundle.manifest.files.0["starlane-notes.db"].sha256,"Original SQLite evidence not retained");
   let manifest=std::fs::read(sealed.path().join(".engine-transfer-custody").join(&sealed.intent.transfer_id).join("original-manifest.json"))?;
   ensure!(manifest==bundle.raw_manifest,"Raw original manifest changed");
   if mode=="sealed-write-revert" {let role=sealed.path().join(".engine-transfer-custody").join(&sealed.intent.transfer_id).join("intent.json");let raw=std::fs::read(&role)?;std::fs::write(&role,&raw)?;ensure!(sealed.fence().is_err(),"Sealed engine evidence write/revert was refreshed");ensure!(bootstrap_gate(sealed.path()).is_err(),"Invalid sealed engine evidence retired marker");}
  }
  bundle.fence()?;bundle.db.close().await;Ok(())
 }
}


#[cfg(test)]
fn engine_checkpoint(phase:&str)->io::Result<()> {
 if std::env::var("STARLANE_ENGINE_PAUSE_PHASE").ok().as_deref()!=Some(phase){return Ok(());}
 let ready=PathBuf::from(std::env::var("STARLANE_ENGINE_CHECKPOINT_READY").map_err(|_|fail("Explicit checkpoint receipt required"))?);
 let workspace=PathBuf::from(std::env::var("STARLANE_ENGINE_IMPORT_WORKSPACE").map_err(|_|fail("Explicit checkpoint workspace required"))?);
 if !ready.is_absolute()||ready.parent()!=Some(workspace.as_path()){return Err(fail("Checkpoint receipt must be in disposable workspace"));}
 use std::io::Write;use std::os::unix::fs::OpenOptionsExt;
 let mut f=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(ready)?;
 f.write_all(phase.as_bytes())?;f.sync_all()?;
 loop {
  let release=workspace.join("phase-release");
  if let Ok(raw)=std::fs::read(&release) {if raw==phase.as_bytes(){return Ok(());}}
  std::thread::park_timeout(std::time::Duration::from_millis(25));
 }
}
#[cfg(not(test))]
fn engine_checkpoint(_phase:&str)->io::Result<()> {Ok(())}

#[cfg(test)]
mod interruption_tests {
 use super::*;
 #[tokio::test]
 #[ignore="fresh explicit interrupted engine root and owned target required"]
 async fn required_interrupted_engine_fresh_gate()->anyhow::Result<()> {
  let root=PathBuf::from(std::env::var("STARLANE_ENGINE_INTERRUPTED_ROOT").expect("root required"));
  let phase=std::env::var("STARLANE_ENGINE_INTERRUPTED_PHASE").expect("phase required");
  let url=std::env::var("STARLANE_ENGINE_PG_URL").expect("owned target required");
  ensure!(root.is_absolute()&&matches!(phase.as_str(),"source-intent"|"pg-import-committed"|"engine-sealed"),"Invalid interruption controls");
  ensure!(!root.join(DESTINATION_MARKER).exists(),"Runner must expose namespace-only fallback refusal");
  ensure!(bootstrap_gate(&root).is_err(),"Interrupted engine root admitted stale SQLite fallback");
  let dirs=std::fs::read_dir(root.join(".engine-transfer-custody"))?.collect::<Result<Vec<_>,_>>()?;
  ensure!(dirs.len()==1,"Ambiguous engine reservation");
  let id=dirs[0].file_name().into_string().map_err(|_|anyhow::anyhow!("NonUTF8 reservation"))?;
  let uuid=uuid::Uuid::parse_str(&id)?;ensure!(uuid.to_string()==id,"Noncanonical reservation UUID");
  let raw=std::fs::read(dirs[0].path().join("source-intent.json"))?;
  let intent:serde_json::Value=serde_json::from_slice(&raw)?;
  let schema=format!("transfer_{}",uuid.simple());
  ensure!(intent["reserved_schema"].as_str()==Some(schema.as_str())&&intent["transfer_id"].as_str()==Some(id.as_str()),"Reservation binding differs");
  let mut conn=sqlx::PgConnection::connect(&url).await?;
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM pg_catalog.pg_namespace WHERE nspname=$1").bind(&schema).fetch_one(&mut conn).await?;
  if phase=="source-intent" {
   ensure!(count==0,"Source-only kill wrote a target schema");
   ensure!(!dirs[0].path().join("intent.json").exists(),"Source-only kill claims sealed engine");
  } else {
   ensure!(count==1,"Committed import schema missing");
   sqlx::query(&format!("SET search_path TO {}",quoted(&schema)?)).execute(&mut conn).await?;
   let rows:Vec<(String,String,String)>=sqlx::query_as("SELECT transfer_id,commitment_json,commitment_sha256 FROM engine_transfer_import_commitments").fetch_all(&mut conn).await?;
   ensure!(rows.len()==1&&rows[0].0==id&&digest(rows[0].1.as_bytes())==rows[0].2,"Actual import commitment not retained");
   let record:serde_json::Value=serde_json::from_str(&rows[0].1)?;
   ensure!(record["pre_import_intent_sha256"].as_str()==Some(digest(&raw).as_str()),"SQL commitment detached from durable source intent");
   let target=engine_target_identity(&mut conn,&schema).await?;
   ensure!(record["target"]==serde_json::to_value(target)?,"Fresh connection target association differs");
   ensure!(record["serving_allowed"]==false,"Interrupted import grants serving");
   ensure!(!dirs[0].path().join("completion.json").exists()&&!dirs[0].path().join("finalization.json").exists(),"Incomplete engine has final authority roles");
   if phase=="engine-sealed" {ensure!(dirs[0].path().join("intent.json").exists(),"Durable seal missing");}
  }
  conn.close().await?;Ok(())
 }
}


#[cfg(test)]
mod finalization_boundary_tests {
 use super::*;
 #[tokio::test]
 #[ignore="fresh explicit genuine interrupted transfer root and owned target required"]
 async fn required_engine_finalization_fresh_gate()->anyhow::Result<()> {
  let root=PathBuf::from(std::env::var("STARLANE_ENGINE_INTERRUPTED_ROOT").expect("root required"));
  let phase=std::env::var("STARLANE_ENGINE_INTERRUPTED_PHASE").expect("phase required");
  let url=std::env::var("STARLANE_ENGINE_PG_URL").expect("owned target required");
  ensure!(root.is_absolute()&&matches!(phase.as_str(),"engine-sql-committed"|"engine-completion"|"engine-marker-retired"|"engine-finalization"|"foreign-marker-exchange"),"Invalid finalization controls");
  ensure!(!root.join(DESTINATION_MARKER).exists(),"Namespace-only fresh gate requires original marker absent");
  ensure!(bootstrap_gate(&root).is_err(),"Ordinary stale SQLite fallback unexpectedly admitted engine root");
  let envelope:EngineCommitEnvelope=serde_json::from_slice(&std::fs::read(root.join(".engine-transfer-custody/current.json"))?)?;
  let dir=root.join(".engine-transfer-custody").join(&envelope.transfer_id);
  ensure!(dir.join("completion.json").exists()==(phase!="engine-sql-committed"),"Completion boundary differs");
  ensure!(dir.join("finalization.json").exists()==(phase=="engine-finalization"),"Finalization boundary differs");
  let mut read=sqlx::PgConnection::connect(&url).await?;
  sqlx::query(&format!("SET search_path TO {}",quoted(&envelope.target.schema_name)?)).execute(&mut read).await?;
  if phase=="engine-finalization" {let held=admit_engine(&root,&mut read).await?;held.physical_fence()?;held.verify_connection(&mut read).await?;}
  else {ensure!(admit_engine(&root,&mut read).await.is_err(),"Incomplete/foreign retirement gained fresh admission");}
  read.close().await?;Ok(())
 }
}

#[cfg(test)]
mod diagnostic_tests {
    use super::*;
    #[test]
    fn retained_diagnostic_directory_rejects_equal_private_replacement() {
        use std::os::unix::fs::PermissionsExt;
        let w=std::env::temp_dir().join(format!("transfer-diagnostics-{}",uuid::Uuid::new_v4()));
        std::fs::create_dir(&w).unwrap();std::fs::set_permissions(&w,std::fs::Permissions::from_mode(0o700)).unwrap();
        let path=w.join("diagnostics");std::fs::create_dir(&path).unwrap();std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o700)).unwrap();
        let root=Root::open(&path).unwrap();engine_diagnostic_write(&root,"first.json",b"original-private-evidence").unwrap();
        assert!(engine_diagnostic_write(&root,"first.json",b"overwrite").is_err());
        assert_eq!(std::fs::read(path.join("first.json")).unwrap(),b"original-private-evidence");
        let retained=w.join("retained");std::fs::rename(&path,&retained).unwrap();std::fs::create_dir(&path).unwrap();std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(engine_diagnostic_write(&root,"next.json",b"do-not-write-foreign").is_err());
        assert!(!path.join("next.json").exists());assert!(!retained.join("next.json").exists());
        assert_eq!(std::fs::read(retained.join("first.json")).unwrap(),b"original-private-evidence");
    }
}

include!("engine_postgres_capture.rs");

include!("engine_postgres_operator.rs");

include!("engine_catalogue_profiles.rs");

#[cfg(test)]
mod publication_bridge_acceptance {
    use super::*;
    const FAMILIES: [&str; 3] = ["draft_publication_receipts", "document_publication_snapshots", "snapshot_recovery_receipts"];
    fn publication_rows(tables: &BTreeMap<String, Table>, populated: bool) -> anyhow::Result<BTreeMap<String, Table>> {
        FAMILIES.iter().map(|name| {
            let table = tables.get(*name).context("Original publication family absent")?;
            if populated { ensure!(!table.rows.is_empty(), "Genuine source must contain populated {name}; test does not manufacture history"); }
            let mut canonical = table.clone();
            canonical.rows.sort_by_cached_key(|row| serde_json::to_vec(row).expect("Serializable typed source cells"));
            Ok((name.to_string(), canonical))
        }).collect()
    }
    async fn serving_tables(serving: &QualifiedPgServing) -> anyhow::Result<BTreeMap<String, Table>> {
        serving.fence().await?;
        let mut conn = serving.pool().acquire().await?;
        let profile = pg_profile_any(&mut conn).await?;
        pg_tables_any(&mut conn, &shape_for_profile(profile)?).await
    }
    fn private_new(path: &Path) -> anyhow::Result<()> {
        use std::os::unix::fs::DirBuilderExt;
        ensure!(path.is_absolute(), "Fixture workspace must be absolute");
        std::fs::DirBuilder::new().mode(0o700).create(path)?;
        Ok(())
    }
    fn owned_url() -> anyhow::Result<String> {
        let url = std::env::var("REPLAY_CAPACITY_TEST_PG_URL")?;
        ensure!(url.starts_with("postgres://") || url.starts_with("postgresql://"), "Explicit owned PostgreSQL URL required");
        Ok(url)
    }
    #[tokio::test]
    #[ignore = "requires genuine populated SQLite54 bundle, new owned workspace and REPLAY_CAPACITY_TEST_PG_URL"]
    async fn required_populated54_transfer_pg8_backup_newtarget_restore() -> anyhow::Result<()> {
        sqlx::any::install_default_drivers();
        let source = PathBuf::from(std::env::var("STARLANE_ENGINE_SOURCE_BUNDLE")?);
        let workspace = PathBuf::from(std::env::var("STARLANE_ENGINE_SOURCE_WORKSPACE")?);
        ensure!(source.is_absolute(), "Genuine source bundle must be absolute");
        let url = owned_url()?;
        private_new(&workspace)?;
        let bundle = crate::storage::backup_inventory::verify(&source).await?;
        ensure!(bundle.manifest.compatible_catalogue() == "sqlite/54", "Source must be genuine canonical54 capture");
        let plan = crate::storage::restoration_plan::plan(&bundle).await?;
        let copied = reserve_and_copy(&bundle, &workspace.join("sqlite-custody"))?;
        let materialized = materialize_groups(copied, &bundle, plan)?;
        let sealed = seal_publication(&bundle, materialized).await?;
        let committed = commit_publication(sealed).await?;
        let ready = retire_publication(committed)?;
        let prepared = prepare_engine_transfer(&bundle, ready)?;
        let expected = publication_rows(&prepared.original.tables, true)?;
        ensure!(expected["document_publication_snapshots"].rows.iter().flatten().any(|cell| matches!(cell,Cell::Blob(bytes) if !bytes.is_empty())), "Source must contain real binary raw snapshot payload");
        let artifacts = workspace.join("transfer-artifacts");
        private_new(&artifacts)?;
        let imported = commit_engine_import(prepared, &url, &artifacts).await?;
        let sealed = seal_engine_import(imported)?;
        let committed = commit_engine_seal(sealed, &url).await?;
        let finalized = retire_engine_commit(committed)?;
        finalized.fence()?;
        let serving = open_current_qualified_pg_serving(finalized.path(), &url).await?;
        let before = serving_tables(&serving).await?;
        ensure!(before.len() == 86 && publication_rows(&before, true)? == expected, "Transfer changed original lifecycle rows or binary payload");
        let backup_path = workspace.join("pg8-backup");
        backup_qualified_postgres(finalized.path(), &backup_path, &url, true).await?;
        ensure!(serving_tables(&serving).await? == before, "Backup changed original source SQL");
        let target = workspace.join("pg8-new-target");
        let report = restore_qualified_postgres(&backup_path, &target, &url, true).await?;
        ensure!(report.compatible_catalogue == EngineCatalogueProfile::CURRENT.catalogue_label(), "New target profile differs");
        let restored = open_current_qualified_pg_serving(&target, &url).await?;
        let after = serving_tables(&restored).await?;
        ensure!(after.len() == 86 && publication_rows(&after, true)? == expected, "New-target restore changed original lifecycle rows or binary payload");
        restored.fence().await?;
        serving.fence().await?;
        bundle.fence()?;
        println!("POPULATED54_TRANSFER_PG8_BACKUP_NEW_TARGET_QUALIFIED_RESTORE_PASS original3families_and_raw_blob_exact source_unchanged no_invented_custody");
        restored.pool().close().await;
        serving.pool().close().await;
        bundle.db.close().await;
        Ok(())
    }
    async fn audience55_restored_http(serving:&QualifiedPgServing,target:&Path)->anyhow::Result<()> {
        // Real restored rows, existing sessions and actual HTTP routes. No seeded
        // grants, substitute receipts, fabricated versions or source mutation.
        let http_started=std::time::Instant::now();
        let pool=serving.pool();
        let raw:String=sqlx::query_scalar("SELECT receipt_json FROM asset_replay_receipts WHERE entity_type='asset' ORDER BY incoming_id LIMIT 1").fetch_one(pool).await?;
        let replay:crate::sync::AssetReplayReceipt=serde_json::from_str(&raw)?;
        ensure!(replay.draft_scope.is_some(),"Actual scoped Replay source required");
        let published:(String,String)=sqlx::query_as("SELECT a.id,a.storage_path FROM attachments a JOIN attachment_audience_bindings b ON b.asset_id=a.id WHERE b.binding_kind='publication' AND a.document_id=$1 AND a.space_id=$2 ORDER BY a.id LIMIT 1")
            .bind(&replay.document_id).bind(&replay.space_id).fetch_one(pool).await?;
        let owner:(String,String,String)=sqlx::query_as("SELECT u.email,u.role,s.id FROM users u JOIN sessions s ON s.user_id=u.id WHERE u.id=$1 ORDER BY s.id LIMIT 1").bind(&replay.actor_id).fetch_one(pool).await?;
        let candidates:Vec<(String,String,String,String)>=sqlx::query_as("SELECT u.id,u.email,u.role,s.id FROM users u JOIN sessions s ON s.user_id=u.id WHERE u.id<>$1 ORDER BY u.id,s.id LIMIT 64").bind(&replay.actor_id).fetch_all(pool).await?;
        std::env::set_var("DATA_DIR",target);
        let secret="owned-audience55-restored-http";
        let state=crate::AppState{db:pool.clone(),jwt_secret:secret.into(),base_url:"http://localhost".into(),
            search_index:crate::search::SearchIndex::new(target.join("search_index"))?,
            oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};
        let token=|id:&str,email:&str,role:&str,session:&str|crate::auth::jwt::create_token_pair_for_session(id,email,role,secret,Some(session),30).map(|pair|pair.access_token);
        let owner_token=token(&replay.actor_id,&owner.0,&owner.1,&owner.2)?;
        let router=axum::Router::new().nest("/api/v1",crate::api_routes()).with_state(state);
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address=listener.local_addr()?;
        let task=tokio::spawn(async move {axum::serve(listener,router).await});
        let check=async {
            let client=reqwest::Client::new();
            let file_url=|path:&str|format!("http://{address}/api/v1/files/{}",path.split('/').map(|p|urlencoding::encode(p).into_owned()).collect::<Vec<_>>().join("/"));
            let private_url=file_url(&replay.storage_path);
            println!("RESTORED55_HTTP_OWNER_REPLAY_BEGIN elapsed_ms={}",http_started.elapsed().as_millis());
            let response=client.get(&private_url).bearer_auth(&owner_token).send().await?;
            ensure!(response.status()==200,"Restored original owner Replay bytes refused: {}",response.status());
            let bytes=response.bytes().await?;
            ensure!(bytes.len()as i64==replay.byte_length&&digest(&bytes)==replay.binary_sha256,"Restored Replay byte identity differs");
            println!("RESTORED55_HTTP_OWNER_REPLAY_EXACT_DONE elapsed_ms={}",http_started.elapsed().as_millis());
            let metadata=format!("http://{address}/api/v1/spaces/{}/documents/{}/attachment-versions/{}",replay.space_id,replay.document_id,replay.incoming.entity_id);
            ensure!(client.get(&metadata).bearer_auth(&owner_token).send().await?.status()==404,"Replay invented a direct immutable version");
            ensure!(client.get(&private_url).send().await?.status()==401,"Anonymous restored Replay read admitted");
            println!("RESTORED55_HTTP_NO_INVENTED_VERSION_AND_ANONYMOUS_DENIAL_DONE elapsed_ms={}",http_started.elapsed().as_millis());
            let mut denied_viewer=false;
            for(id,email,role,session)in candidates{
                let credential=token(&id,&email,&role,&session)?;
                let public=client.get(file_url(&published.1)).bearer_auth(&credential).send().await?;
                if public.status()!=200{continue}
                let private=client.get(&private_url).bearer_auth(&credential).send().await?;
                if private.status()!=404{continue}
                let list=format!("http://{address}/api/v1/spaces/{}/documents/{}/attachments",replay.space_id,replay.document_id);
                let response=client.get(&list).bearer_auth(&credential).send().await?;
                ensure!(response.status()==200,"Restored published reader collection unavailable");
                let rows:serde_json::Value=response.json().await?;
                let rows=rows.as_array().context("Actual restored attachment collection required")?;
                ensure!(!rows.iter().any(|r|r["id"]==replay.incoming.entity_id)&&rows.iter().any(|r|r["id"]==published.0),"Restored collection private/public audience differs");
                println!("RESTORED55_HTTP_PUBLISHED_READER_PRIVATE_DENIAL_DONE elapsed_ms={}",http_started.elapsed().as_millis());
                denied_viewer=true;break
            }
            ensure!(denied_viewer,"Genuine restored published reader with private Replay denial required");
            Ok::<(),anyhow::Error>(())
        }.await;
        task.abort();let _=task.await;
        check?;
        println!("ACTUAL_RESTORED55_HTTP_REPLAY_OWNER_EXACT_PUBLISHED_VIEWER_POSITIVE_PRIVATE_BYTES_AND_COLLECTION_DENIED_NO_INVENTED_VERSION_PASS");
        Ok(())
    }
    #[tokio::test]
    #[ignore = "requires genuine populated SQLite55 scoped direct/replay/registered-copy/legacy-copy bundle; run alone;, new owned workspace and REPLAY_CAPACITY_TEST_PG_URL"]
    async fn required_populated55_audiences_transfer_pg9_backup_newtarget_restore() -> anyhow::Result<()> {
        sqlx::any::install_default_drivers();
        let source = PathBuf::from(std::env::var("STARLANE_ENGINE_SOURCE_BUNDLE")?);
        let workspace = PathBuf::from(std::env::var("STARLANE_ENGINE_SOURCE_WORKSPACE")?);
        ensure!(source.is_absolute(), "Genuine source bundle must be absolute");
        let url = owned_url()?;
        private_new(&workspace)?;
        let bundle = crate::storage::backup_inventory::verify(&source).await?;
        ensure!(bundle.manifest.compatible_catalogue() == "sqlite/55", "Source must be genuine canonical55 capture");
        let plan = crate::storage::restoration_plan::plan(&bundle).await?;
        let copied = reserve_and_copy(&bundle, &workspace.join("sqlite-custody"))?;
        let materialized = materialize_groups(copied, &bundle, plan)?;
        let sealed = seal_publication(&bundle, materialized).await?;
        let committed = commit_publication(sealed).await?;
        let ready = retire_publication(committed)?;
        let prepared = prepare_engine_transfer(&bundle, ready)?;
        publication_rows(&prepared.original.tables,true)?;
        let names=["attachment_audience_origins","attachment_audience_bindings","attachment_upload_receipts",
            "asset_replay_receipts","attachment_version_records","attachment_families","attachment_version_changes",
            "attachment_retirements","attachment_capacity_reservations","attachment_retained_byte_ledger",
            "draft_birth_receipts","draft_transfer_receipts","document_drafts","draft_publication_receipts","document_publication_snapshots",
            "snapshot_recovery_receipts","move_asset_path_aliases","attachments"];
        let expected=names.iter().map(|name|{
            let table=prepared.original.tables.get(*name).context("Original audience/custody table absent")?;
            Ok((name.to_string(),table_hash(table)?))
        }).collect::<anyhow::Result<BTreeMap<_,_>>>()?;
        let values=|name:&str,column:&str|->anyhow::Result<Vec<String>>{
            let table=prepared.original.tables.get(name).context("Original table absent")?;
            let index=table.columns.iter().position(|c|c.name==column).context("Original column absent")?;
            table.rows.iter().map(|row|text(&row[index]).map(str::to_owned)).collect()
        };
        let kinds=values("attachment_audience_origins","kind")?;
        ensure!(kinds.iter().any(|v|v=="draft_upload_v2")&&kinds.iter().any(|v|v=="draft_replay_v2"),"Source requires genuine scoped direct and replay origins");
        let bindings=values("attachment_audience_bindings","binding_kind")?;
        ensure!(bindings.iter().any(|v|v=="draft")&&bindings.iter().any(|v|v=="publication"),"Source requires real private and publication binding receipts");
        let mut direct=false;let mut registered_copy=false;let mut legacy_copy=false;
        let mut physical=Vec::new();
        for raw in values("attachment_upload_receipts","receipt_json")? {
            let receipt:crate::storage::attachment_upload::Receipt=serde_json::from_str(&raw)?;
            if let Some(scope)=&receipt.intent.request.draft_scope {
                scope.validate()?;
                match scope.copy_source.as_ref(){
                    None=>direct=true,
                    Some(copy)if copy.source_kind.as_deref()==Some("legacy_snapshot_v1")=>legacy_copy=true,
                    Some(_)=>registered_copy=true,
                }
                let mut original=bundle.open_owned(&receipt.intent.storage_path)?;
                let mut bytes=Vec::new();std::io::Read::read_to_end(&mut original,&mut bytes)?;
                ensure!(bytes.len()as i64==receipt.intent.byte_length&&digest(&bytes)==receipt.intent.binary_sha256,"Original scoped direct/copy bytes differ");
                physical.push((receipt.intent.storage_path,bytes));
            }
        }
        ensure!(direct&&registered_copy&&legacy_copy,"Source requires real direct upload and registered/legacy copy custody");
        // Replay and legacy files have no direct-upload receipt; compare all
        // current associations against their genuine captured source bytes too.
        let assets=&prepared.original.tables["attachments"];
        let path_column=assets.columns.iter().position(|c|c.name=="storage_path").context("Original attachment path absent")?;
        let size_column=assets.columns.iter().position(|c|c.name=="size_bytes").context("Original attachment size absent")?;
        for row in &assets.rows{
            let path=text(&row[path_column])?.to_owned();
            let mut original=bundle.open_owned(&path)?;
            let mut bytes=Vec::new();std::io::Read::read_to_end(&mut original,&mut bytes)?;
            ensure!(matches!(row[size_column],Cell::Integer(n) if n>=0&&n as usize==bytes.len()),"Original attachment length differs");
            physical.push((path,bytes));
        }
        ensure!(values("asset_replay_receipts","receipt_json")?.iter().any(|raw|serde_json::from_str::<serde_json::Value>(raw).is_ok_and(|r|!r["draft_scope"].is_null())),"Source requires actual scoped replay receipt");
        let artifacts = workspace.join("transfer-artifacts");
        private_new(&artifacts)?;
        let imported = commit_engine_import(prepared, &url, &artifacts).await?;
        let sealed = seal_engine_import(imported)?;
        let committed = commit_engine_seal(sealed, &url).await?;
        let finalized = retire_engine_commit(committed)?;
        finalized.fence()?;
        let serving = open_current_qualified_pg_serving(finalized.path(), &url).await?;
        let before = serving_tables(&serving).await?;
        ensure!(before.len()==87,"Qualified transfer catalogue differs");
        for(name,hash)in &expected{ensure!(table_hash(&before[name])?==*hash,"Transfer changed original scoped evidence: {name}");}
        for(path,bytes)in &physical{ensure!(std::fs::read(finalized.path().join(path))?==*bytes,"Transfer changed original scoped bytes: {path}");}
        pg_copied_replay_sql_negative_controls(&serving).await?;
        let backup_path = workspace.join("pg9-backup");
        backup_qualified_postgres(finalized.path(), &backup_path, &url, true).await?;
        ensure!(serving_tables(&serving).await? == before, "Backup changed original source SQL");
        let target = workspace.join("pg9-new-target");
        let report = restore_qualified_postgres(&backup_path, &target, &url, true).await?;
        ensure!(report.compatible_catalogue == EngineCatalogueProfile::CURRENT.catalogue_label(), "New target profile differs");
        let restored = open_current_qualified_pg_serving(&target, &url).await?;
        let after = serving_tables(&restored).await?;
        ensure!(after.len()==87,"New-target catalogue differs");
        for(name,hash)in &expected{ensure!(table_hash(&after[name])?==*hash,"Restore changed original scoped evidence: {name}");}
        for(path,bytes)in &physical{ensure!(std::fs::read(target.join(path))?==*bytes,"Restore changed original scoped bytes: {path}");}
        restored.fence().await?;
        serving.fence().await?;
        bundle.fence()?;
        audience55_restored_http(&restored,&target).await?;
        pg_copied_replay_sql_negative_controls(&restored).await?;
        // A restored target must be recapturable through its actual native
        // publication; a readable first restore alone does not prove that.
        println!("RESTORED55_SECOND_BACKUP_BEGIN");
        let recapture_before=serving_tables(&restored).await?;
        let recapture_path=workspace.join("pg9-recapture");
        backup_qualified_postgres(&target,&recapture_path,&url,true).await?;
        ensure!(serving_tables(&restored).await?==recapture_before,"Recapture changed first restored target SQL");
        let second_target=workspace.join("pg9-second-target");
        restore_qualified_postgres(&recapture_path,&second_target,&url,true).await?;
        let second=open_current_qualified_pg_serving(&second_target,&url).await?;
        let second_rows=serving_tables(&second).await?;
        ensure!(second_rows.len()==87,"Second restored catalogue differs");
        for(name,hash)in &expected{ensure!(table_hash(&second_rows[name])?==*hash,"Second restore changed original scoped evidence: {name}");}
        for(path,bytes)in &physical{ensure!(std::fs::read(second_target.join(path))?==*bytes,"Second restore changed original scoped bytes: {path}");}
        pg_copied_replay_sql_negative_controls(&second).await?;
        second.fence().await?;restored.fence().await?;serving.fence().await?;bundle.fence()?;
        println!("RESTORED55_SECOND_BACKUP_NEW_TARGET_ORIGINAL_ROWS_AND_BYTES_DONE");
        // These physical controls deliberately invalidate held native proof stamps.
        // Run only after every positive capture/restore/fence; rename-back cannot
        // launder the original proof. No later serving claim uses these targets.
        pg_copied_replay_final_physical_negative(&serving,&workspace.join("transfer-physical-negatives")).await?;
        pg_copied_replay_final_physical_negative(&second,&workspace.join("restored-physical-negatives")).await?;
        bundle.fence()?;
        println!("POPULATED55_AUDIENCES_TRANSFER_PG9_BACKUP_NEW_TARGET_PASS original17families_exact scoped_direct_replay_registered_and_legacy_copy source_unchanged no_invented_grants second_backup_new_target actual_sql_and_physical_negative_controls");
        second.pool().close().await;
        restored.pool().close().await;
        serving.pool().close().await;
        bundle.db.close().await;
        Ok(())
    }
    #[tokio::test]
    #[ignore = "requires genuine qualified PG7 root, new backup/restore targets and REPLAY_CAPACITY_TEST_PG_URL"]
    async fn required_genuine_pg7_refusal_backup_newtarget8() -> anyhow::Result<()> {
        sqlx::any::install_default_drivers();
        let root = PathBuf::from(std::env::var("STARLANE_ENGINE_APP_ROOT")?);
        let backup = PathBuf::from(std::env::var("STARLANE_PG_BACKUP_TARGET")?);
        let target = PathBuf::from(std::env::var("STARLANE_PG_RESTORE_ROOT")?);
        let url = owned_url()?;
        let original = open_qualified_pg_serving(&root, &url).await?;
        let before = serving_tables(&original).await?;
        ensure!(before.len() == 81, "Must use genuine qualified old53/7 source");
        {
            let mut conn = original.pool().acquire().await?;
            ensure!(pg_profile_any(&mut conn).await? == EngineCatalogueProfile::Erasure53Pg7, "Wrong original profile");
        }
        let error = match open_current_qualified_pg_serving(&root, &url).await { Ok(_) => bail!("Old qualified PG7 entered current application startup"), Err(error) => error };
        ensure!(error.to_string().contains("Qualified earlier serving profiles"), "Wrong startup refusal boundary: {error}");
        ensure!(serving_tables(&original).await? == before, "Startup refusal changed original tables");
        original.fence().await?;
        backup_qualified_postgres(&root, &backup, &url, true).await?;
        restore_qualified_postgres(&backup, &target, &url, true).await?;
        let restored = open_current_qualified_pg_serving(&target, &url).await?;
        let after = serving_tables(&restored).await?;
        ensure!(after.len() == 86, "Current new-target catalogue incomplete");
        ensure!(publication_rows(&after, false)?.values().all(|table| table.rows.is_empty()), "Old source must not fabricate publication history");
        for (name, table) in &before {
            if name != "storage_restore_generations" && name != "storage_restore_current" {
                ensure!(after.get(name).map(table_hash).transpose()? == Some(table_hash(table)?), "Old original application rows changed: {name}");
            }
        }
        ensure!(serving_tables(&original).await? == before, "Explicit restore changed old source");
        original.fence().await?;
        restored.fence().await?;
        println!("GENUINE_QUALIFIED_PG7_CURRENT_STARTUP_REFUSAL_BACKUP_NEW_TARGET8_PASS old_rows_preserved new_history_empty");
        restored.pool().close().await;
        original.pool().close().await;
        Ok(())
    }
}
