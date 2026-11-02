// Complete lossless SQL comparison for the dedicated verifier; never SQL authority.
// The live target is queried on its caller-owned original reservation, including
// WAL content. Do not open an immutable/filename SQLite mirror of the target.
pub(crate) struct VerificationSqlProjection<'a> { original:NativeSnapshot<'a> }
pub(crate) struct VerificationSqlClosure { source_sha256:String,qualified_source_sha256:String,target_sha256:String,tables:usize,owner_acknowledged_noop:bool }
impl VerificationSqlClosure {
 pub(crate) fn source_sha256(&self)->&str {&self.source_sha256}
 pub(crate) fn qualified_source_sha256(&self)->&str {&self.qualified_source_sha256}
 pub(crate) fn target_sha256(&self)->&str {&self.target_sha256}
 pub(crate) fn tables(&self)->usize {self.tables}
 pub(crate) fn owner_acknowledged_noop(&self)->bool {self.owner_acknowledged_noop}
}
async fn verification_live_rows(conn:&mut sqlx::AnyConnection,table:&str,columns:&[Column],maximum_rows:usize)->anyhow::Result<Vec<Vec<Cell>>> {
 use futures::TryStreamExt;use sqlx::Row;
 let projections=columns.iter().map(|c| {
  let q=quoted(&c.name)?;
  Ok(format!("typeof({q}),CASE WHEN typeof({q}) IN ('text','blob') THEN hex(CAST({q} AS BLOB)) ELSE NULL END,CASE WHEN typeof({q})='integer' THEN CAST({q} AS TEXT) ELSE NULL END,CASE WHEN typeof({q})='real' THEN {q} ELSE NULL END"))
 }).collect::<anyhow::Result<Vec<_>>>()?.join(",");
 let query=format!("SELECT {projections} FROM {}",quoted(table)?);let mut stream=sqlx::query(&query).fetch(&mut *conn);let mut rows=Vec::new();let mut bytes=0usize;
 while let Some(row)=stream.try_next().await? {
  ensure!(rows.len()<maximum_rows.min(100000),"Verification original row bound exceeded");let mut cells=Vec::new();
  for (index,column) in columns.iter().enumerate() {
   let offset=index*4;let kind:String=row.try_get(offset)?;
   let cell=match kind.as_str() {
    "null"=>Cell::Null,
    "integer"=>Cell::Integer(row.try_get::<String,_>(offset+2)?.parse()?),
    "text"|"blob"=>{let raw=hex::decode(row.try_get::<String,_>(offset+1)?)?;bytes=bytes.checked_add(raw.len()).context("Verification SQL size overflow")?;ensure!(raw.len()<=16*1024*1024&&bytes<=32*1024*1024,"Verification SQL byte bound exceeded");if kind=="text"{Cell::Text(raw)}else{Cell::Blob(raw)}},
    "real"=>Cell::Real(row.try_get::<f64,_>(offset+3)?.to_bits()),
    _=>bail!("Unexpected live SQLite storage class"),
   };
   validate_cell(&cell,column,table)?;cells.push(cell);
  }
  bytes=bytes.checked_add(serde_json::to_vec(&cells)?.len()).context("Verification typed SQL size overflow")?;ensure!(bytes<=32*1024*1024,"Verification typed SQL byte bound exceeded");
  rows.push(cells);
 }
 Ok(rows)
}
impl<'a> VerificationSqlProjection<'a> {
 pub(crate) fn original(bundle:&'a crate::storage::backup_inventory::ValidatedBundle)->anyhow::Result<Self> {
  let original=preflight(bundle)?;let mut total=0usize;
  for table in original.tables.values(){for row in &table.rows {total=total.checked_add(serde_json::to_vec(row)?.len()).context("Verification source size overflow")?;ensure!(total<=32*1024*1024,"Initial verification source SQL budget exceeded");}}
  Ok(Self{original})
 }
 pub(crate) fn original_source_sha256(&self)->anyhow::Result<String> {
  self.original.bundle.fence()?;
  let original_tables=self.original.tables.iter().filter(|(name,_)|!matches!(name.as_str(),"storage_restore_generations"|"storage_restore_current")).map(|(name,t)|(name.clone(),t.clone())).collect::<BTreeMap<_,_>>();
  engine_typed_hash(&original_tables)
 }
 /// Exact pre-startup comparison. A later recovery transition needs its own
 /// actual-owner acknowledgment witness; this strict path never whitelists it.
 pub(crate) async fn compare_exact(&self,conn:&mut sqlx::AnyConnection)->anyhow::Result<VerificationSqlClosure> {
  self.compare_qualified(conn,None).await
 }
 pub(crate) async fn compare_after_publication_recovery(&self,reservation:&mut VerificationReservation,target:&VerificationTarget<'_>,ack:&crate::storage::publication::PublicationRecoveryAck<'_,'_>)->anyhow::Result<VerificationSqlClosure> {
  ensure!(std::ptr::eq(self.original.bundle,ack.original_bundle())&&std::ptr::eq(self.original.bundle,target.bundle),"Recovery acknowledgment belongs to another original backup");target.fence(reservation).await?;ack.fence(&target.pool,target.path())?;
  let mut result=self.compare_qualified(reservation.connection(),Some(ack)).await?;target.fence(reservation).await?;ack.fence(&target.pool,target.path())?;
  // Only this genuine same-bundle/current-target owner acknowledgment can
  // qualify an unchanged SQL image. Exact reads and copied hashes never do.
  result.owner_acknowledged_noop=ack.changes().next().is_none()&&result.source_sha256==result.qualified_source_sha256&&result.qualified_source_sha256==result.target_sha256;Ok(result)
 }
 async fn compare_qualified(&self,conn:&mut sqlx::AnyConnection,ack:Option<&crate::storage::publication::PublicationRecoveryAck<'_,'_>>)->anyhow::Result<VerificationSqlClosure> {
  use sqlx::Row;
  self.original.bundle.fence()?;
  let mut qualified_tables=self.original.tables.clone();
  if let Some(ack)=ack {
   let table=qualified_tables.get_mut("publication_receipts").context("Original publication table absent")?;
   let column=|name:&str|table.columns.iter().position(|c|c.name==name).ok_or_else(||anyhow::anyhow!("Original publication column absent"));
   let op=column("op_id")?;let raw=column("manifest_json")?;let verified=column("verified_at")?;let mut seen=std::collections::BTreeSet::new();
   for (id,manifest,before,after) in ack.changes() {ensure!(seen.insert(id),"Duplicate actual recovery acknowledgment");let rows=table.rows.iter_mut().filter(|r|r[op]==Cell::Text(id.as_bytes().to_vec())).collect::<Vec<_>>();ensure!(rows.len()==1,"Actual recovery acknowledgment has no exact original row");let row=rows.into_iter().next().context("Original receipt row absent")?;let prior=before.map(|s|Cell::Text(s.as_bytes().to_vec())).unwrap_or(Cell::Null);ensure!(row[raw]==Cell::Text(manifest.as_bytes().to_vec())&&row[verified]==prior,"Actual recovery acknowledgment differs from original raw receipt/timestamp");row[verified]=Cell::Text(after.as_bytes().to_vec());}
  }
  let integrity:Vec<(String,)>=sqlx::query_as("PRAGMA integrity_check").fetch_all(&mut *conn).await?;ensure!(integrity==vec![("ok".into(),)],"Live verification target integrity failed");
  ensure!(sqlx::query("PRAGMA foreign_key_check").fetch_all(&mut *conn).await?.is_empty(),"Live verification target FK violation");
  let schema=sqlx::query("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name").fetch_all(&mut *conn).await?;
  let mut actual=Vec::new();for r in schema {actual.push(vec![Cell::Text(r.try_get::<String,_>(0)?.into_bytes()),Cell::Text(r.try_get::<String,_>(1)?.into_bytes()),Cell::Text(r.try_get::<String,_>(2)?.into_bytes()),match r.try_get::<Option<String>,_>(3)?{Some(s)=>Cell::Text(s.into_bytes()),None=>Cell::Null}]);}
  ensure!(actual==self.original.schema_rows,"Live verification target schema differs from original source");
  let mut target_tables=qualified_tables.iter().filter(|(name,_)|!matches!(name.as_str(),"storage_restore_generations"|"storage_restore_current")).map(|(name,t)|(name.clone(),t.clone())).collect::<BTreeMap<_,_>>();
  let mut target_bytes=0usize;
  for (name,table) in &mut target_tables {
   table.rows=verification_live_rows(conn,name,&table.columns,qualified_tables[name].rows.len()).await?;
   for row in &table.rows {target_bytes=target_bytes.checked_add(serde_json::to_vec(row)?.len()).context("Verification target SQL size overflow")?;ensure!(target_bytes<=32*1024*1024,"Verification whole target SQL byte bound exceeded");}
   ensure!(table.rows.len()==qualified_tables[name].rows.len()&&table_hash(table)?==table_hash(&qualified_tables[name])?,"Live verification original SQL changed: {name}");
  }
  // Raw migration rows include installed_on/checksum/execution_time, not only version.
  let columns=vec![Column{name:"version".into(),kind:"BIGINT".into(),required:true},Column{name:"description".into(),kind:"TEXT".into(),required:true},Column{name:"installed_on".into(),kind:"TEXT".into(),required:true},Column{name:"success".into(),kind:"BIGINT".into(),required:true},Column{name:"checksum".into(),kind:"BYTEA".into(),required:true},Column{name:"execution_time".into(),kind:"BIGINT".into(),required:true}];
  let mut migrations=verification_live_rows(conn,"_sqlx_migrations",&columns,self.original.raw_sqlite_migrations.len()).await?;
  migrations.sort_by_key(|r|match r[0]{Cell::Integer(v)=>v,_=>i64::MIN});
  ensure!(migrations==self.original.raw_sqlite_migrations,"Live verification raw migration rows changed");
  self.original.bundle.fence()?;
  // Exact non-custody table hash. The two generation tables are validated by
  // VerificationTarget, never cloned into an apparent byte-exact SQL claim.
  let source_tables=qualified_tables.iter().filter(|(name,_)|!matches!(name.as_str(),"storage_restore_generations"|"storage_restore_current")).map(|(name,t)|(name.clone(),t.clone())).collect::<BTreeMap<_,_>>();
  let qualified_source_sha256=engine_typed_hash(&source_tables)?;let target_sha256=engine_typed_hash(&target_tables)?;
  ensure!(qualified_source_sha256==target_sha256,"Verification SQL closure differs");
  // Preserve the original source fingerprint separately. Legitimate actual-owner
  // startup transitions never masquerade as byte-exact original SQL equality.
  let original_tables=self.original.tables.iter().filter(|(name,_)|!matches!(name.as_str(),"storage_restore_generations"|"storage_restore_current")).map(|(name,t)|(name.clone(),t.clone())).collect::<BTreeMap<_,_>>();
  let source_sha256=engine_typed_hash(&original_tables)?;
  Ok(VerificationSqlClosure{source_sha256,qualified_source_sha256,target_sha256,tables:target_tables.len(),owner_acknowledged_noop:false})
 }
}

#[cfg(test)]
mod verification_projection_tests {
 use super::*;
 #[tokio::test]
 async fn live_projection_observes_wal_blobs_and_refuses_unqualified_text_classes() -> anyhow::Result<()> {
  sqlx::any::install_default_drivers();
  let base=std::env::temp_dir().join(format!("verification-live-sql-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&base)?;
  let pool=sqlx::any::AnyPoolOptions::new().max_connections(2).connect(&format!("sqlite://{}?mode=rwc",base.join("original.db").display())).await?;
  let mut c=pool.acquire().await?;
  sqlx::query("PRAGMA journal_mode=WAL").execute(&mut *c).await?;
  sqlx::query("CREATE TABLE receipt(raw TEXT NOT NULL, bytes BLOB NOT NULL, n BIGINT NOT NULL)").execute(&mut *c).await?;
  sqlx::query("INSERT INTO receipt VALUES(CAST(X'610062' AS TEXT),X'610062',9223372036854775807)").execute(&mut *c).await?;
  let columns=vec![Column{name:"raw".into(),kind:"TEXT".into(),required:true},Column{name:"bytes".into(),kind:"BYTEA".into(),required:true},Column{name:"n".into(),kind:"BIGINT".into(),required:true}];
  assert!(verification_live_rows(&mut c,"receipt",&columns,1).await.is_err(),"Initial strict native codec cannot silently truncate unqualified NUL text");
  sqlx::query("UPDATE receipt SET raw=CAST(X'6162' AS TEXT)").execute(&mut *c).await?;
  let expected=vec![vec![Cell::Text(b"ab".to_vec()),Cell::Blob(b"a\0b".to_vec()),Cell::Integer(i64::MAX)]];
  assert_eq!(verification_live_rows(&mut c,"receipt",&columns,1).await?,expected);
  sqlx::query("UPDATE receipt SET raw=CAST(X'6163' AS TEXT)").execute(&mut *c).await?;
  assert_ne!(verification_live_rows(&mut c,"receipt",&columns,1).await?,expected,"Actual WAL updates must be observed instead of immutable main-file mirror");
  sqlx::query("UPDATE receipt SET raw=X'6162'").execute(&mut *c).await?;
  assert!(verification_live_rows(&mut c,"receipt",&columns,1).await.is_err(),"Equal bytes with foreign SQLite storage class refuse");
  drop(c);pool.close().await;std::fs::remove_dir_all(&base)?;Ok(())
 }
}
