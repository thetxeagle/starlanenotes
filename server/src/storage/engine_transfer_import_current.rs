// Candidate-only owned PostgreSQL import/readback; deliberately no publication or admission.
use sqlx::{Connection,Row};
#[derive(Debug,Serialize)]
struct DestinationReference {device:u64,inode:u64,links:u64,length:u64,sha256:String}
#[derive(Debug,Serialize)]
struct TransferReadbackCertificate{#[serde(skip)] raw_commitment:Vec<u8>,#[serde(skip)] target:EngineTargetIdentity,transfer_id:String,schema:String,database_oid:i64,schema_oid:i64,source:PreflightCertificate,target_table_hashes:BTreeMap<String,(usize,String)>,catalog_sha256:String,postgres_migrations_sha256:String,destination_reference_ids:BTreeMap<String,DestinationReference>,foreign_constraints_restored:usize,source_history_rows_preserved:usize,serving_allowed:bool,cutover_allowed:bool}
async fn catalog(conn:&mut sqlx::PgConnection,schema:&str)->anyhow::Result<Vec<String>>{
 let rows:Vec<String>=sqlx::query_scalar("SELECT value FROM (SELECT jsonb_build_array('column',c.relname,a.attname,a.attnum,format_type(a.atttypid,a.atttypmod),a.attnotnull,coalesce(pg_get_expr(d.adbin,d.adrelid),''),coalesce(col.collname,''),col.collcollate,col.collctype)::text AS value FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace JOIN pg_catalog.pg_attribute a ON a.attrelid=c.oid LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid=c.oid AND d.adnum=a.attnum LEFT JOIN pg_catalog.pg_collation col ON col.oid=a.attcollation WHERE n.nspname=$1 AND c.relkind='r' AND a.attnum>0 AND NOT a.attisdropped UNION ALL SELECT jsonb_build_array('constraint',c.relname,k.conname,k.contype,pg_get_constraintdef(k.oid,false),k.condeferrable,k.condeferred,k.convalidated)::text FROM pg_catalog.pg_constraint k JOIN pg_catalog.pg_class c ON c.oid=k.conrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 UNION ALL SELECT jsonb_build_array('index',c.relname,i.indexrelid::text,pg_get_indexdef(i.indexrelid))::text FROM pg_catalog.pg_index i JOIN pg_catalog.pg_class c ON c.oid=i.indrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 UNION ALL SELECT jsonb_build_array('database-collation',d.oid::text,d.datcollate,d.datctype)::text FROM pg_catalog.pg_database d WHERE d.datname=current_database()) s ORDER BY value").bind(schema).fetch_all(conn).await?;ensure!(!rows.is_empty(),"Empty target catalog");Ok(rows)
}
async fn pg_tables(conn:&mut sqlx::PgConnection,shape:&BTreeMap<String,Table>)->anyhow::Result<BTreeMap<String,Table>>{
 let mut result=shape.clone();for(n,t)in &mut result{t.rows.clear();let projections=t.columns.iter().map(|c|quoted(&c.name)).collect::<Result<Vec<_>,_>>()?.join(",");for row in sqlx::query(&format!("SELECT {projections} FROM {}",quoted(n)?)).fetch_all(&mut *conn).await?{
  let mut cells=vec![];for(i,c)in t.columns.iter().enumerate(){let cell=match c.kind.as_str(){"TEXT"=>row.try_get::<Option<String>,_>(i)?.map(|v|Cell::Text(v.into_bytes())).unwrap_or(Cell::Null),"BIGINT"=>row.try_get::<Option<i64>,_>(i)?.map(Cell::Integer).unwrap_or(Cell::Null),"BYTEA"=>row.try_get::<Option<Vec<u8>>,_>(i)?.map(Cell::Blob).unwrap_or(Cell::Null),_=>bail!("Unknown target type")};validate_cell(&cell,c,n)?;cells.push(cell);}t.rows.push(cells);
 }}Ok(result)
}
async fn association(conn:&mut sqlx::PgConnection,schema:&str,expected:Option<(i64,i64)>)->anyhow::Result<(i64,i64)>{
 let r=sqlx::query("SELECT d.oid::bigint AS database_oid,n.oid::bigint AS schema_oid,current_schema()::text AS current_schema,current_setting('server_encoding')::text AS encoding,current_setting('client_encoding')::text AS client_encoding FROM pg_catalog.pg_database d JOIN pg_catalog.pg_namespace n ON n.nspname=$1 WHERE d.datname=current_database()").bind(schema).fetch_one(conn).await?;
 ensure!(r.try_get::<String,_>("current_schema")?==schema&&r.try_get::<String,_>("encoding")?=="UTF8"&&r.try_get::<String,_>("client_encoding")?=="UTF8","Target association/encoding changed");let pair=(r.try_get("database_oid")?,r.try_get("schema_oid")?);if let Some(e)=expected{ensure!(pair==e,"Target database/schema replaced");}Ok(pair)
}
async fn import_post_custody_owned(prepared:&PreparedEngineTransfer<'_>,url:&str,artifact:&Root)->anyhow::Result<TransferReadbackCertificate>{
 let original=&prepared.original;let snapshot=NativeSnapshot{profile:original.profile,bundle:original.bundle,_database:original._database.try_clone()?,tables:prepared.post_tables.clone(),schema_rows:original.schema_rows.clone(),raw_sqlite_migrations:original.raw_sqlite_migrations.clone(),reference_paths:original.reference_paths.clone()};let snapshot=&snapshot;let planned=&prepared.destination;prepared.fence()?;
 snapshot.bundle.fence()?;planned.fence()?;let source=snapshot.certificate()?;let p=planned.plan();ensure!(p.backup_id()==source.backup_id&&p.manifest_sha256()==source.manifest_sha256&&p.database_sha256()==source.sqlite_database_sha256&&p.source_root()==&snapshot.bundle.manifest.source_custody.root,"Planned destination belongs to a different source bundle");// PreparedEngineTransfer is constructed only from the real finalized publisher;
 // historical/inert diagnostics require exact explicit no-authority closure.
 engine_finalized_inert_gate(prepared)?;prepared.fence()?;native_registry_before_pg_import(prepared).await?;let transfer_id=prepared.transfer_id.clone();let schema=format!("transfer_{}",uuid::Uuid::parse_str(&transfer_id)?.simple());let schema_q=quoted(&schema)?;
 let mut conn=sqlx::PgConnection::connect(url).await.context("Cannot connect owned disposable PostgreSQL")?;
 sqlx::query(&format!("CREATE SCHEMA {schema_q}")).execute(&mut conn).await.context("Reserve new owned schema")?;sqlx::query(&format!("SET search_path TO {schema_q}")).execute(&mut conn).await?;
 // Record retained target immediately. Failed attempts are never silently removed.
 let intent=serde_json::json!({"phase":"data-import-diagnostic-only","transfer_id":transfer_id,"schema":schema,"source":source,"destination":planned.path(),"serving_allowed":false,"cutover_allowed":false});engine_diagnostic_write(artifact,"retained-import-intent.json",&serde_json::to_vec_pretty(&intent)?)?;
 let selected=schema.clone();let pool=sqlx::any::AnyPoolOptions::new().max_connections(1).after_connect(move|c,_|{let schema=selected.clone();Box::pin(async move{sqlx::query(&format!("SET search_path TO \"{schema}\"")).execute(c).await?;Ok(())})}).connect(url).await?;
 crate::db::migrate_selected(&pool,crate::db::DatabaseEngine::Postgres).await?;pool.close().await;
 sqlx::query("CREATE TABLE engine_transfer_import_commitments (transfer_id TEXT PRIMARY KEY NOT NULL, commitment_json TEXT NOT NULL, commitment_sha256 TEXT NOT NULL)").execute(&mut conn).await?;
 let target_identity=engine_target_identity(&mut conn,&schema).await?;let bound=association(&mut conn,&schema,None).await?;let before_catalog=catalog(&mut conn,&schema).await?;let migration_rows:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(version,description,(extract(epoch from installed_on)::numeric)::text,success,encode(checksum,'hex'),execution_time)::text FROM _sqlx_migrations ORDER BY version").fetch_all(&mut conn).await?;let target_profile=authenticated_pg_profile(&migration_rows)?;ensure!(target_profile==EngineCatalogueProfile::CURRENT,"Independent selected target7 required");let target_shape=shape_for_profile(target_profile)?;let source_profile=snapshot.profile;let expected_target_tables=target_import_rows(source_profile,target_profile,&snapshot.tables,&target_shape)?;
 target_relation_census(&mut conn,&schema,&target_shape,"engine_transfer_import_commitments").await?;let baseline=pg_tables(&mut conn,&target_shape).await?;validate_fresh_import_baseline(target_profile,&baseline)?;
 let foreign:Vec<(String,String,bool,bool)>=sqlx::query_as("SELECT c.relname::text,k.conname::text,k.condeferrable,k.condeferred FROM pg_catalog.pg_constraint k JOIN pg_catalog.pg_class c ON c.oid=k.conrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND k.contype='f' ORDER BY c.relname,k.conname").bind(&schema).fetch_all(&mut conn).await?;
 engine_diagnostic_write(artifact,"catalog-before-import.json",&serde_json::to_vec_pretty(&serde_json::json!({"catalog":before_catalog,"baseline":baseline,"migrations":migration_rows,"association":bound}))?)?;snapshot.bundle.fence()?;planned.fence()?;let mut tx=conn.begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await?;sqlx::query("SET LOCAL synchronous_commit=on").execute(&mut *tx).await?;let all=target_shape.keys().map(|n|quoted(n)).collect::<anyhow::Result<Vec<_>>>()?.join(",");sqlx::query(&format!("LOCK TABLE {all} IN SHARE ROW EXCLUSIVE MODE")).execute(&mut *tx).await?;ensure!(catalog(&mut tx,&schema).await?==before_catalog,"Fresh target catalogue changed before first import mutation");target_relation_census(&mut tx,&schema,&target_shape,"engine_transfer_import_commitments").await?;
 for(table,constraint,_,_)in &foreign{sqlx::query(&format!("ALTER TABLE {} ALTER CONSTRAINT {} DEFERRABLE INITIALLY DEFERRED",quoted(table)?,quoted(constraint)?)).execute(&mut *tx).await?;}
 sqlx::query("SET CONSTRAINTS ALL DEFERRED").execute(&mut *tx).await?;
 for(n,t)in &baseline{if !t.rows.is_empty(){sqlx::query(&format!("DELETE FROM {}",quoted(n)?)).execute(&mut *tx).await?;}}
 for(n,t)in &expected_target_tables{let cols=t.columns.iter().map(|c|quoted(&c.name)).collect::<Result<Vec<_>,_>>()?.join(",");let binds=(1..=t.columns.len()).map(|i|format!("${i}")).collect::<Vec<_>>().join(",");let sql=format!("INSERT INTO {} ({cols}) VALUES ({binds})",quoted(n)?);
  for row in &t.rows{let mut query=sqlx::query(&sql);for(cell,c)in row.iter().zip(&t.columns){query=match cell{Cell::Null=>match c.kind.as_str(){"TEXT"=>query.bind(Option::<String>::None),"BIGINT"=>query.bind(Option::<i64>::None),"BYTEA"=>query.bind(Option::<Vec<u8>>::None),_=>bail!("Null unsupported target")},Cell::Text(raw)=>query.bind(std::str::from_utf8(raw)?),Cell::Integer(v)=>query.bind(*v),Cell::Blob(raw)=>query.bind(raw),Cell::Real(_)=>bail!("REAL cannot enter target")};}query.execute(&mut *tx).await.with_context(||format!("Import row into owned table {n}"))?;}
 }
 #[cfg(test)]
 if std::env::var("STARLANE_TRANSFER_IMPORT_FAULT").ok().as_deref()==Some("relationship"){let affected=sqlx::query("UPDATE attachments SET document_id='missing-transfer-reference'").execute(&mut *tx).await?;ensure!(affected.rows_affected()>0,"Meaningful rollback fixture requires real attachment");}
 sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *tx).await?;
 for(table,constraint,deferrable,deferred)in &foreign{let timing=if *deferrable{if *deferred{"DEFERRABLE INITIALLY DEFERRED"}else{"DEFERRABLE INITIALLY IMMEDIATE"}}else{"NOT DEFERRABLE INITIALLY IMMEDIATE"};sqlx::query(&format!("ALTER TABLE {} ALTER CONSTRAINT {} {timing}",quoted(table)?,quoted(constraint)?)).execute(&mut *tx).await?;}
 let imported=pg_tables(&mut tx,&target_shape).await?;for(n,t)in &snapshot.tables{ensure!(table_hash(t)?==table_hash(&imported[n])?&&t.rows.len()==imported[n].rows.len(),"Transactional typed readback differs: {n}");}
 ensure!(catalog(&mut tx,&schema).await?==before_catalog,"Constraint/catalog definition changed");
 let imported_typed_sha256=engine_typed_hash(&imported)?;verify_source_target_readback(&snapshot.tables,&expected_target_tables,&imported)?;
 let root_id=identity(prepared.destination.copied.root.fd())?;
 let raw_commitment=serde_json::to_vec(&serde_json::json!({"version":1,"phase":"post-custody-import-committed-only","transfer_id":transfer_id,"source_backup_id":prepared.original.bundle.manifest.backup_id,"source_manifest_sha256":digest(&prepared.original.bundle.raw_manifest),"source_sqlite_database_sha256":prepared.original.bundle.manifest.files.0["starlane-notes.db"].sha256,"source_typed_sha256":prepared.source_typed_sha256,"post_custody_typed_sha256":prepared.post_custody_typed_sha256,"pre_import_intent_sha256":digest(&prepared.raw_source_intent),"custody_generation_id":prepared.custody_generation_id,"custody_ledger_sha256":prepared.custody_ledger_sha256,"root_device":root_id.0,"root_inode":root_id.1,"target":target_identity,"target_catalog_sha256":digest(&serde_json::to_vec(&before_catalog)?),"postgres_migrations_sha256":digest(&serde_json::to_vec(&migration_rows)?),"imported_typed_sha256":imported_typed_sha256,"serving_allowed":false}))?;
 let commitment_sha256=digest(&raw_commitment);let commitment_text=std::str::from_utf8(&raw_commitment)?;
 sqlx::query("INSERT INTO engine_transfer_import_commitments (transfer_id,commitment_json,commitment_sha256) VALUES ($1,$2,$3)").bind(&transfer_id).bind(commitment_text).bind(&commitment_sha256).execute(&mut *tx).await?;
 ensure!(engine_target_identity(&mut tx,&schema).await?==target_identity,"Target changed before import commitment");prepared.fence()?;tx.commit().await?;conn.close().await?;engine_checkpoint("pg-import-committed")?;
 // Fresh connection verifies committed data/catalog/association, not transaction cache.
 let mut read=sqlx::PgConnection::connect(url).await?;sqlx::query(&format!("SET search_path TO {schema_q}")).execute(&mut read).await?;association(&mut read,&schema,Some(bound)).await?;let mut readtx=read.begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY").execute(&mut *readtx).await?;
 let after=pg_tables(&mut readtx,&target_shape).await?;verify_source_target_readback(&snapshot.tables,&expected_target_tables,&after)?;for(n,t)in &snapshot.tables{ensure!(table_hash(t)?==table_hash(&after[n])?&&t.rows.len()==after[n].rows.len(),"Committed typed readback differs: {n}");}ensure!(catalog(&mut readtx,&schema).await?==before_catalog,"Committed catalog differs");let after_migrations:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(version,description,(extract(epoch from installed_on)::numeric)::text,success,encode(checksum,'hex'),execution_time)::text FROM _sqlx_migrations ORDER BY version").fetch_all(&mut *readtx).await?;ensure!(after_migrations==migration_rows,"Independent PG migration ledger changed");
 ensure!(engine_target_identity(&mut readtx,&schema).await?==target_identity,"Fresh target cluster/association changed");
 let rows:Vec<(String,String,String)>=sqlx::query_as("SELECT transfer_id,commitment_json,commitment_sha256 FROM engine_transfer_import_commitments").fetch_all(&mut *readtx).await?;
 ensure!(rows==vec![(transfer_id.clone(),commitment_text.into(),commitment_sha256.clone())],"Fresh import commitment differs");
 readtx.commit().await?;read.close().await?;prepared.fence()?;
 engine_diagnostic_write(artifact,"actual-import-commitment.json",&raw_commitment)?;
 snapshot.bundle.fence()?;planned.fence()?;let mut ids=BTreeMap::new();for path in &snapshot.reference_paths{let observed=planned.observed(path)?;let captured=&snapshot.bundle.manifest.files.0[path];ids.insert(path.clone(),DestinationReference{device:observed.0,inode:observed.1,links:observed.2,length:captured.size_bytes,sha256:captured.sha256.clone()});}
 let certificate=TransferReadbackCertificate{raw_commitment,target:target_identity,transfer_id,schema,database_oid:bound.0,schema_oid:bound.1,source,target_table_hashes:after.iter().map(|(n,t)|Ok((n.clone(),(t.rows.len(),table_hash(t)?)))).collect::<anyhow::Result<_>>()?,catalog_sha256:digest(&serde_json::to_vec(&before_catalog)?),postgres_migrations_sha256:digest(&serde_json::to_vec(&migration_rows)?),destination_reference_ids:ids,foreign_constraints_restored:foreign.len(),source_history_rows_preserved:snapshot.tables["storage_restore_generations"].rows.len(),serving_allowed:false,cutover_allowed:false};engine_diagnostic_write(artifact,"typed-readback-certificate.json",&serde_json::to_vec_pretty(&certificate)?)?;Ok(certificate)
}

// Opaque imported phase: no serving or finalization conversion is exposed.
pub(crate) struct ImportedEngineTransfer<'a> {prepared:PreparedEngineTransfer<'a>,certificate:TransferReadbackCertificate}
impl ImportedEngineTransfer<'_> {pub(crate) fn fence(&self)->io::Result<()> {self.prepared.fence()} pub(crate) fn path(&self)->&Path {self.prepared.path()}}
pub(crate) async fn commit_engine_import<'a>(prepared:PreparedEngineTransfer<'a>,url:&str,artifact:&Path)->anyhow::Result<ImportedEngineTransfer<'a>> {let root=Root::open(artifact)?;root.check()?;let certificate=import_post_custody_owned(&prepared,url,&root).await?;prepared.fence()?;root.check()?;Ok(ImportedEngineTransfer{prepared,certificate})}

fn engine_finalized_inert_gate(prepared:&PreparedEngineTransfer<'_>)->anyhow::Result<()> {
 prepared.fence()?;
 let p=prepared.destination.plan();
 let raw=read_ledger(&prepared.destination.leaves.get(".restoration-custody/current.json").context("Qualified final custody absent")?.held)?;
 ensure!(digest(&raw)==prepared.custody_ledger_sha256,"Finalized custody hash changed");
 let envelope:HistoryEnvelope=serde_json::from_slice(&raw)?;
 ensure!(envelope.version==3&&envelope.runtime.generation_id==prepared.custody_generation_id,"Finalized owner generation changed");
 for unresolved in p.unresolved() {
  ensure!(matches!(unresolved.reason.as_str(),"orphan_stage_preserved_without_authority"|"prior_scope_preserved_without_fresh_classification"),"Active/foreign/unsafe unresolved scope cannot enter engine import: {}",unresolved.reason);
  let op=unresolved.operation_id.as_deref().context("Inert diagnostic lacks operation")?;
  let scope=p.scopes().iter().find(|s|s.namespace==unresolved.namespace&&s.operation_id==op).context("Inert source scope absent")?;
  ensure!(scope.class==crate::storage::restoration_plan::OperationClass::InertReservation&&scope.bindings.is_empty(),"Unresolved scope retains authority");
  ensure!(envelope.denied.iter().any(|d|d.namespace==unresolved.namespace&&d.op_id==op),"Finalized owner failed to deny unresolved operation");
  ensure!(!envelope.runtime.scopes.iter().any(|s|s.namespace==unresolved.namespace&&s.op_id==op)&&!envelope.proofs.iter().any(|s|s.namespace==unresolved.namespace&&s.op_id==op),"Finalized owner grants unresolved operation");
 }
 prepared.fence()?;Ok(())
}

fn engine_diagnostic_write(root:&Root,name:&str,raw:&[u8])->io::Result<()> {
    use std::io::Write;
    root.check()?;
    if root.fd().metadata()?.mode() & 0o777 != 0o700 {return Err(fail("Transfer diagnostics must be private"));}
    let mut file=open_at(root.fd(),name,libc::O_WRONLY|libc::O_CREAT|libc::O_EXCL)?;
    file.write_all(raw)?;file.sync_all()?;root.fd().sync_all()?;
    if file.metadata()?.nlink()!=1 || file.metadata()?.mode() & 0o777 != 0o600 {return Err(fail("Transfer diagnostic role changed"));}
    root.check()
}

pub(crate) async fn commit_engine_import_bound<'a>(prepared:PreparedEngineTransfer<'a>,url:&str,artifact:&TransferOutputDirectory)->anyhow::Result<ImportedEngineTransfer<'a>> {
    artifact.root.check()?;
    let certificate=import_post_custody_owned(&prepared,url,&artifact.root).await?;
    prepared.fence()?;artifact.root.check()?;
    Ok(ImportedEngineTransfer{prepared,certificate})
}
