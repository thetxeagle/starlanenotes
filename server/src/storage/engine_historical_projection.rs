// Read-only canonical typed projection, no ownership or reference-semantics grant.
// Caller must retain its original source/connection and stopped-write guard.
pub(crate) async fn historical_application_projection_in(conn:&mut sqlx::AnyConnection,engine:Option<&RestoredPgAdmission>)->anyhow::Result<String>{
 let tables=historical_application_tables_in(conn,engine).await?;historical_tables_digest(&tables)
}
fn historical_tables_digest(tables:&BTreeMap<String,Table>)->anyhow::Result<String>{let mut tables=tables.clone();for table in tables.values_mut(){let mut rows=table.rows.iter().map(serde_json::to_vec).collect::<Result<Vec<_>,_>>()?;rows.sort();table.rows=rows.into_iter().map(|raw|serde_json::from_slice(&raw)).collect::<Result<Vec<_>,_>>()?;}let raw=serde_json::to_vec(&tables)?;ensure!(raw.len()<=8*1024*1024,"Historical canonical full application projection exceeds bounded v1 contract");Ok(digest(&raw))}
async fn historical_application_tables_in(conn:&mut sqlx::AnyConnection,engine:Option<&RestoredPgAdmission>)->anyhow::Result<BTreeMap<String,Table>>{
 let postgres=engine.is_some();if let Some(engine)=engine{engine.verify_any_connection(conn).await?;}
 let profile=if postgres {pg_profile_any(conn).await?}else{
  static MIGRATOR:sqlx::migrate::Migrator=sqlx::migrate!("./migrations");
  let rows=sqlx::query("SELECT version,checksum,CASE WHEN typeof(success)='integer' AND success=1 THEN 1 ELSE 0 END AS succeeded FROM _sqlx_migrations ORDER BY version").fetch_all(&mut *conn).await?;
  ensure!(matches!(rows.len(),53|54|55|56|57)&&MIGRATOR.iter().count()==57,"Historical qualified SQLite migration profile differs");
  for(row,known)in rows.iter().zip(MIGRATOR.iter()){ensure!(row.try_get::<i64,_>("version")?==known.version&&row.try_get::<Vec<u8>,_>("checksum")?==known.checksum.as_ref()&&row.try_get::<i64,_>("succeeded")?==1,"Historical original SQLite migration bytes differ");}
  match rows.len(){57=>EngineCatalogueProfile::Discussion57Pg11,56=>EngineCatalogueProfile::DraftTransfer56Pg10,55=>EngineCatalogueProfile::Audience55Pg9,54=>EngineCatalogueProfile::Publication54Pg8,_=>EngineCatalogueProfile::Erasure53Pg7}
 };
 ensure!(profile.has_erasure(),"Historical qualified erasure/publication profile required");
 let shape=shape_for_profile(profile)?;
 let actual:Vec<String>=if postgres{sqlx::query_scalar("SELECT c.relname::text FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=current_schema() AND c.relkind='r' ORDER BY c.relname").fetch_all(&mut *conn).await?}else{sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name").fetch_all(&mut *conn).await?};
 // These are not application-table exclusions by name alone. The genuine
 // opaque original PgRestore admission above authenticates their exact target
 // relation IDs/catalogue and all raw import/generation/current rows on THIS
 // connection. No caller boolean/decoded record can enable this branch.
 let mut expected=shape.keys().cloned().chain(std::iter::once("_sqlx_migrations".into())).collect::<std::collections::BTreeSet<_>>();
 if engine.is_some(){expected.extend(["pg_restoration_imports","pg_restoration_generations","pg_restoration_current"].into_iter().map(str::to_owned));}
 ensure!(actual.into_iter().collect::<std::collections::BTreeSet<_>>()==expected,"Historical full application table set differs");
 for(name,table)in &shape{
  let columns:Vec<String>=if postgres{sqlx::query_scalar("SELECT a.attname::text FROM pg_catalog.pg_attribute a JOIN pg_catalog.pg_class c ON c.oid=a.attrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=current_schema() AND c.relname=$1 AND a.attnum>0 AND NOT a.attisdropped ORDER BY a.attnum").bind(name).fetch_all(&mut *conn).await?}else{let rows=sqlx::query(&format!("PRAGMA table_info({})",quoted(name)?)).fetch_all(&mut *conn).await?;rows.iter().map(|r|r.try_get::<String,_>("name")).collect::<Result<Vec<_>,_>>()?};
  ensure!(columns==table.columns.iter().map(|c|c.name.clone()).collect::<Vec<_>>(),"Historical full application column set differs: {name}");
 }
 // Reject an oversized projection before materializing application cells. The
 // SQL engine measures all TEXT/BLOB bytes; every cell/row also consumes a
 // conservative linear node budget. The final canonical encoding has its own
 // 8MiB cap. Opaque bytes remain undecoded; this grants no reference semantics.
 let mut budget=1024u64*1024;
 for(name,table)in &shape{
  let lengths=table.columns.iter().filter(|c|matches!(c.kind.as_str(),"TEXT"|"BYTEA")).map(|c|{let q=quoted(&c.name)?;Ok(if postgres{format!("COALESCE(octet_length({q}),0)")}else{format!("COALESCE(length(CAST({q} AS BLOB)),0)")})}).collect::<anyhow::Result<Vec<_>>>()?;
  let length=if lengths.is_empty(){"0".to_owned()}else{lengths.join("+")};let total=if postgres{format!("CAST(COALESCE(SUM({length}),0) AS BIGINT)")}else{format!("COALESCE(SUM({length}),0)")};
  let row=sqlx::query(&format!("SELECT COUNT(*) AS n,{total} AS bytes FROM {}",quoted(name)?)).fetch_one(&mut *conn).await?;let rows:u64=row.try_get::<i64,_>("n")?.try_into()?;let bytes:u64=row.try_get::<i64,_>("bytes")?.try_into()?;
  budget=budget.checked_add(rows.checked_mul(table.columns.len() as u64).and_then(|n|n.checked_mul(64)).context("Historical row budget overflow")?).and_then(|n|n.checked_add(bytes.checked_mul(4)?)).context("Historical raw byte budget overflow")?;
  ensure!(budget<=8*1024*1024,"Historical full application projection preflight exceeds bounded v1 contract");
 }
 let mut tables=pg_tables_any(conn,&shape).await?;
 for table in tables.values_mut(){let mut rows=table.rows.iter().map(serde_json::to_vec).collect::<Result<Vec<_>,_>>()?;rows.sort();table.rows=rows.into_iter().map(|raw|serde_json::from_slice(&raw)).collect::<Result<Vec<_>,_>>()?;}
 let raw=serde_json::to_vec(&tables)?;ensure!(raw.len()<=8*1024*1024,"Historical canonical full application projection exceeds bounded v1 contract");if let Some(engine)=engine{engine.verify_any_connection(conn).await?;}Ok(tables)
}

// The caller cannot supply rows/normalization flags: only the private holder
// admitted from real finalized Active +exact connection/closure supplies them.
pub(crate) async fn historical_original_projection_for_finalized_in(conn:&mut sqlx::AnyConnection,owner:&super::HistoricalFinalizedObservation<'_>)->anyhow::Result<String>{
 owner.fence_physical()?;owner.connection_in(conn).await?;
 let mut tables=historical_application_tables_in(conn,None).await?;
 let make=|table:&Table,rows:&[HistorySqlRow]|->anyhow::Result<Vec<Vec<Cell>>>{rows.iter().map(|r|table.columns.iter().map(|c|Ok(Cell::Text(match c.name.as_str(){"generation_id"=>&r.generation_id,"backup_id"=>&r.backup_id,"manifest_sha256"=>&r.manifest_sha256,"database_sha256"=>&r.database_sha256,"ledger_sha256"=>&r.ledger_sha256,"root_device"=>&r.root_device,"root_inode"=>&r.root_inode,"state"=>&r.state,"created_at"=>&r.created_at,_=>bail!("Unknown exact historical custody column"),}.as_bytes().to_vec()))).collect()).collect()};
 let sort=|rows:&[Vec<Cell>]|->anyhow::Result<Vec<Vec<u8>>>{let mut v=rows.iter().map(serde_json::to_vec).collect::<Result<Vec<_>,_>>()?;v.sort();Ok(v)};
 let generations=tables.get_mut("storage_restore_generations").context("Historical actual generation table absent")?;
 ensure!(sort(&generations.rows)?==sort(&make(generations,&owner.history.0)?)?,"Historical full actual old/new custody cells differ");
 generations.rows=make(generations,&owner.original_history.0)?;
 let pointer=tables.get_mut("storage_restore_current").context("Historical actual current table absent")?;
 let pointers=|rows:&[(i64,String)]|->anyhow::Result<Vec<Vec<Cell>>>{rows.iter().map(|(singleton,id)|pointer.columns.iter().map(|c|Ok(match c.name.as_str(){"singleton"=>Cell::Integer(*singleton),"generation_id"=>Cell::Text(id.as_bytes().to_vec()),_=>bail!("Unknown exact current custody column"),})).collect()).collect()};
 ensure!(sort(&pointer.rows)?==sort(&pointers(&owner.history.1)?)?,"Historical exact current pointer cells differ");
 let original=pointers(&owner.original_history.1)?;pointer.rows=original;
 let result=historical_tables_digest(&tables)?;owner.fence_physical()?;owner.connection_in(conn).await?;Ok(result)
}
