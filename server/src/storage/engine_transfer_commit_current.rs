// Candidate-only actual engine-current commitment. Marker stays incomplete.
#[derive(Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EngineCommitEnvelope {
 version:u32,transfer_id:String,target:EngineTargetIdentity,root_device:u64,root_inode:u64,
 custody_generation_id:String,custody_ledger_sha256:String,seal_intent_sha256:String,
 import_commitment_sha256:String,completion_sha256:String,finalization_sha256:String,
 retired_marker_sha256:String,marker_device:u64,marker_inode:u64,
 custody_sql_sha256:String,custody_shapes:BTreeMap<String,Table>,
 control_catalog_sha256:String,control_relation_ids:BTreeMap<String,i64>,
}
pub(crate) struct CommittedEngineImport<'a> {
 sealed:SealedEngineImport<'a>,envelope:Vec<u8>,raw_finalization:Vec<u8>,
}
impl CommittedEngineImport<'_> {
 pub(crate) fn path(&self)->&Path{self.sealed.path()}
 pub(crate) fn fence(&self)->io::Result<()>{self.sealed.fence()}
}
fn engine_without_control_catalog(rows:Vec<String>)->anyhow::Result<Vec<String>> {
 let mut original=vec![];
 for raw in rows {
  let row:serde_json::Value=serde_json::from_str(&raw)?;
  if !matches!(row.get(1).and_then(|v|v.as_str()),Some("engine_transfer_generations"|"engine_transfer_current")){original.push(raw);}
 }
 Ok(original)
}
async fn engine_import_record(conn:&mut sqlx::PgConnection,sealed:&SealedEngineImport<'_>)->anyhow::Result<()> {
 ensure!(engine_target_identity(conn,&sealed.intent.target.schema_name).await?==sealed.intent.target,"Native target association changed");
 let rows:Vec<(String,String,String)>=sqlx::query_as("SELECT transfer_id,commitment_json,commitment_sha256 FROM engine_transfer_import_commitments ORDER BY transfer_id").fetch_all(&mut *conn).await?;
 let raw=&sealed.imported.certificate.raw_commitment;
 ensure!(rows==vec![(sealed.intent.transfer_id.clone(),std::str::from_utf8(raw)?.into(),digest(raw))],"Actual immutable import record changed");
 let migrations:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(version,description,(extract(epoch from installed_on)::numeric)::text,success,encode(checksum,'hex'),execution_time)::text FROM _sqlx_migrations ORDER BY version").fetch_all(&mut *conn).await?;
 ensure!(digest(&serde_json::to_vec(&migrations)?)==sealed.imported.certificate.postgres_migrations_sha256,"Independent PG migration history changed");
 Ok(())
}
async fn engine_committed_row(conn:&mut sqlx::PgConnection,sealed:&SealedEngineImport<'_>,envelope:&[u8])->anyhow::Result<()> {
 engine_import_record(conn,sealed).await?;
 let rows:Vec<(String,String,String,String)>=sqlx::query_as("SELECT transfer_id,envelope_json,envelope_sha256,state FROM engine_transfer_generations ORDER BY transfer_id").fetch_all(&mut *conn).await?;
 ensure!(rows==vec![(sealed.intent.transfer_id.clone(),std::str::from_utf8(envelope)?.into(),digest(envelope),"committed".into())],"Engine-generation SQL commitment differs");
 let current:Vec<(i64,String)>=sqlx::query_as("SELECT singleton,transfer_id FROM engine_transfer_current ORDER BY singleton").fetch_all(&mut *conn).await?;
 ensure!(current==vec![(1,sealed.intent.transfer_id.clone())],"Engine-current SQL pointer differs");
 Ok(())
}
pub(crate) async fn commit_engine_seal<'a>(mut sealed:SealedEngineImport<'a>,url:&str)->anyhow::Result<CommittedEngineImport<'a>> {
 sealed.fence()?;
 let dir=sealed.imported.prepared.engine_directory.clone();
 let completion=serde_json::to_vec(&serde_json::json!({
  "version":1,"transfer_id":sealed.intent.transfer_id,"seal_intent_sha256":digest(&sealed.raw_intent),
  "import_commitment_sha256":sealed.intent.import_commitment_sha256,"target":sealed.intent.target,
  "root_device":sealed.intent.root_device,"root_inode":sealed.intent.root_inode,
  "phase":"actual-engine-sql-committed-closed-readback"
 }))?;
 let raw_finalization=serde_json::to_vec(&serde_json::json!({
  "version":1,"transfer_id":sealed.intent.transfer_id,"completion_sha256":digest(&completion),
  "retired_marker":format!("{dir}/retired-marker"),"retired_marker_sha256":sealed.intent.marker_sha256,
  "marker_device":sealed.intent.marker_device,"marker_inode":sealed.intent.marker_inode,
  "root_device":sealed.intent.root_device,"root_inode":sealed.intent.root_inode,
  "phase":"exact-marker-retired-and-all-postchecks"
 }))?;
 let schema=&sealed.intent.target.schema_name;let schema_q=quoted(schema)?;
 let mut conn=sqlx::PgConnection::connect(url).await?;
 sqlx::query(&format!("SET search_path TO {schema_q}")).execute(&mut conn).await?;
 engine_import_record(&mut conn,&sealed).await?;
 ensure!(digest(&serde_json::to_vec(&catalog(&mut conn,schema).await?)?)==sealed.imported.certificate.catalog_sha256,"Pre-commit PG catalog changed");
 let mut tx=conn.begin().await?;
 sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await?;
 sqlx::query("SET LOCAL synchronous_commit=on").execute(&mut *tx).await?;
 for table in sealed.imported.prepared.post_tables.keys().chain(["_sqlx_migrations".to_string(),"engine_transfer_import_commitments".to_string()].iter()) {
  sqlx::query(&format!("LOCK TABLE {schema_q}.{} IN SHARE MODE",quoted(table)?)).execute(&mut *tx).await?;
 }
 engine_import_record(&mut tx,&sealed).await?;
 let rows=pg_tables(&mut tx,&sealed.imported.prepared.post_tables).await?;
 ensure!(engine_typed_hash(&rows)?==sealed.imported.prepared.post_custody_typed_sha256,"Pre-serving application rows changed");
 // Deliberately exclusive, not IF NOT EXISTS: no authored control tables are adopted.
 sqlx::query("CREATE TABLE engine_transfer_generations (transfer_id TEXT PRIMARY KEY NOT NULL REFERENCES engine_transfer_import_commitments(transfer_id),envelope_json TEXT NOT NULL,envelope_sha256 TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN ('committed','needs_review')))").execute(&mut *tx).await?;
 sqlx::query("CREATE TABLE engine_transfer_current (singleton BIGINT PRIMARY KEY CHECK(singleton=1),transfer_id TEXT NOT NULL REFERENCES engine_transfer_generations(transfer_id))").execute(&mut *tx).await?;
 ensure!(digest(&serde_json::to_vec(&engine_without_control_catalog(catalog(&mut tx,schema).await?)?)?)==sealed.imported.certificate.catalog_sha256,"Engine control creation changed original catalog");
 engine_control_schema(&mut tx,schema).await?;
 let(control_relation_ids,control_catalog_sha256)=engine_control_identity(&mut tx,schema).await?;
 let custody_tables=sealed.imported.prepared.post_tables.iter().filter(|(name,_)|matches!(name.as_str(),"storage_restore_generations"|"storage_restore_current")).map(|(n,t)|(n.clone(),t.clone())).collect::<BTreeMap<_,_>>();
 ensure!(custody_tables.len()==2,"Exact custody SQL tables required");
 let custody_sql_sha256=engine_typed_hash(&custody_tables)?;
 let custody_shapes=custody_tables.into_iter().map(|(n,mut t)|{t.rows.clear();(n,t)}).collect();
 let description=EngineCommitEnvelope{
  version:1,transfer_id:sealed.intent.transfer_id.clone(),target:sealed.intent.target.clone(),
  custody_sql_sha256,custody_shapes,control_catalog_sha256,control_relation_ids,
  root_device:sealed.intent.root_device,root_inode:sealed.intent.root_inode,
  custody_generation_id:sealed.intent.custody_generation_id.clone(),custody_ledger_sha256:sealed.intent.custody_ledger_sha256.clone(),
  seal_intent_sha256:digest(&sealed.raw_intent),import_commitment_sha256:sealed.intent.import_commitment_sha256.clone(),
  completion_sha256:digest(&completion),finalization_sha256:digest(&raw_finalization),
  retired_marker_sha256:sealed.intent.marker_sha256.clone(),marker_device:sealed.intent.marker_device,marker_inode:sealed.intent.marker_inode,
 };
 let envelope=serde_json::to_vec(&description)?;
 pending_new_file(&mut sealed.imported.prepared.destination,".engine-transfer-custody/current.json",&envelope)?;
 sealed.fence()?;
 sqlx::query("INSERT INTO engine_transfer_generations VALUES($1,$2,$3,'committed')").bind(&sealed.intent.transfer_id).bind(std::str::from_utf8(&envelope)?).bind(digest(&envelope)).execute(&mut *tx).await?;
 sqlx::query("INSERT INTO engine_transfer_current VALUES(1,$1)").bind(&sealed.intent.transfer_id).execute(&mut *tx).await?;
 engine_committed_row(&mut tx,&sealed,&envelope).await?;sealed.fence()?;
 tx.commit().await?;conn.close().await?;
 engine_checkpoint("engine-sql-committed")?;
 // Completion appears only after actual SQL commit, close, and fresh read-only audit.
 let mut read=sqlx::PgConnection::connect(url).await?;
 sqlx::query(&format!("SET search_path TO {schema_q}")).execute(&mut read).await?;
 let mut readtx=read.begin().await?;
 sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY").execute(&mut *readtx).await?;
 engine_committed_row(&mut readtx,&sealed,&envelope).await?;
 let rows=pg_tables(&mut readtx,&sealed.imported.prepared.post_tables).await?;
 ensure!(engine_typed_hash(&rows)?==sealed.imported.prepared.post_custody_typed_sha256,"Closed engine commitment changed application rows");
 ensure!(digest(&serde_json::to_vec(&engine_without_control_catalog(catalog(&mut readtx,schema).await?)?)?)==sealed.imported.certificate.catalog_sha256,"Fresh engine catalog differs");
 readtx.commit().await?;read.close().await?;sealed.fence()?;
 pending_new_file(&mut sealed.imported.prepared.destination,&format!("{dir}/completion.json"),&completion)?;
 sealed.fence()?;engine_checkpoint("engine-completion")?;
 Ok(CommittedEngineImport{sealed,envelope,raw_finalization})
}

async fn engine_control_identity(conn:&mut sqlx::PgConnection,schema:&str)->anyhow::Result<(BTreeMap<String,i64>,String)> {
 let ids:Vec<(String,i64)>=sqlx::query_as("SELECT c.relname::text,c.oid::bigint FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname IN ('engine_transfer_current','engine_transfer_generations') ORDER BY c.relname").bind(schema).fetch_all(&mut *conn).await?;
 ensure!(ids.len()==2&&ids.iter().all(|(_,oid)|*oid>0),"Exact new control relation identities absent");
 let rows=catalog(conn,schema).await?.into_iter().filter(|s|serde_json::from_str::<serde_json::Value>(s).ok().and_then(|v|v.get(1).and_then(|n|n.as_str()).map(str::to_owned)).is_some_and(|n|matches!(n.as_str(),"engine_transfer_current"|"engine_transfer_generations"))).collect::<Vec<_>>();
 ensure!(!rows.is_empty(),"Empty engine control catalog");
 Ok((ids.into_iter().collect(),digest(&serde_json::to_vec(&rows)?)))
}
