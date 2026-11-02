fn ordinary_sqlite_receipts(original_receipts:&Table,current:&Table)->anyhow::Result<BTreeMap<String,(String,Option<String>)>> {
    ensure!(original_receipts.columns == current.columns && original_receipts.keys == current.keys && original_receipts.rows.len() <= 256 && current.rows.len() == original_receipts.rows.len(), "Ordinary publication set/shape differs");
    let index = |name: &str| original_receipts.columns.iter().position(|c| c.name == name).ok_or_else(|| fail("Original publication schema incomplete"));
    let id=index("op_id")?; let raw=index("manifest_json")?; let verified=index("verified_at")?;
    let mut original = BTreeMap::new();
    for row in &original_receipts.rows { ensure!(original.insert(text(&row[id])?.to_owned(), row).is_none(), "Duplicate original receipt"); }
    let mut publication = BTreeMap::new(); let mut bytes=0usize;
    for row in &current.rows {
        let key=text(&row[id])?; let prior=*original.get(key).context("Foreign target publication receipt")?;
        for i in 0..row.len() { if i != verified { ensure!(row[i] == prior[i], "Ordinary immutable publication column changed"); } }
        let manifest=text(&prior[raw])?.to_owned(); bytes=bytes.checked_add(manifest.len()).context("Publication bytes overflow")?;
        ensure!(manifest.len()<=1024*1024 && bytes<=16*1024*1024, "Original publication set exceeds bounds");
        let before=match &row[verified] { Cell::Null=>None, Cell::Text(v)=>{let s=std::str::from_utf8(v)?.to_owned();chrono::DateTime::parse_from_rfc3339(&s)?;Some(s)}, _=>bail!("Foreign publication verification storage class") };
        ensure!(publication.insert(key.to_owned(),(manifest,before)).is_none(), "Duplicate target receipt");
    }
    Ok(publication)
}
// Ordinary-main observation is not a writer grant. The original producer owns
// the source, selected child and measured SID window; this module only qualifies
// exact rows under its final actual target reservation and owner acknowledgment.
pub(crate) struct OrdinarySqlitePreparedProjection<'s, 'b> {
    source: &'s VerificationSqlProjection<'b>,
    expected: BTreeMap<String, Table>,
    publication: BTreeMap<String, (String, Option<String>)>,
    activity_sha256: String,
    activity_count: usize,
}
pub(crate) struct OrdinarySqliteQualified {
    closure: VerificationSqlClosure,
    activity_sha256: String,
    activity_count: usize,
}
impl OrdinarySqliteQualified {
    pub(crate) fn closure(&self) -> &VerificationSqlClosure { &self.closure }
    pub(crate) fn activity_sha256(&self) -> &str { &self.activity_sha256 }
    pub(crate) fn activity_count(&self) -> usize { self.activity_count }
}
impl VerificationSqlProjection<'_> {
    async fn ordinary_live_tables(&self, conn: &mut sqlx::AnyConnection) -> anyhow::Result<BTreeMap<String, Table>> {
        use sqlx::Row;
        self.original.bundle.fence()?;
        let integrity: Vec<(String,)> = sqlx::query_as("PRAGMA integrity_check").fetch_all(&mut *conn).await?;
        ensure!(integrity == vec![("ok".into(),)], "Ordinary final target integrity failed");
        ensure!(sqlx::query("PRAGMA foreign_key_check").fetch_all(&mut *conn).await?.is_empty(), "Ordinary final target FK violation");
        let rows = sqlx::query("SELECT type,name,tbl_name,sql FROM sqlite_master ORDER BY type,name").fetch_all(&mut *conn).await?;
        let mut schema = Vec::new();
        for row in rows {
            schema.push(vec![Cell::Text(row.try_get::<String,_>(0)?.into_bytes()), Cell::Text(row.try_get::<String,_>(1)?.into_bytes()), Cell::Text(row.try_get::<String,_>(2)?.into_bytes()), match row.try_get::<Option<String>,_>(3)? { Some(s) => Cell::Text(s.into_bytes()), None => Cell::Null }]);
        }
        ensure!(schema == self.original.schema_rows, "Ordinary final target schema differs");
        let columns = vec![Column{name:"version".into(),kind:"BIGINT".into(),required:true},Column{name:"description".into(),kind:"TEXT".into(),required:true},Column{name:"installed_on".into(),kind:"TEXT".into(),required:true},Column{name:"success".into(),kind:"BIGINT".into(),required:true},Column{name:"checksum".into(),kind:"BYTEA".into(),required:true},Column{name:"execution_time".into(),kind:"BIGINT".into(),required:true}];
        let mut migrations = verification_live_rows(conn, "_sqlx_migrations", &columns, self.original.raw_sqlite_migrations.len()).await?;
        migrations.sort_by_key(|r| match r[0] { Cell::Integer(n) => n, _ => i64::MIN });
        ensure!(migrations == self.original.raw_sqlite_migrations, "Ordinary final raw migration rows changed");
        let mut tables = self.original.tables.iter().filter(|(n,_)| !matches!(n.as_str(), "storage_restore_generations" | "storage_restore_current")).map(|(n,t)|(n.clone(),t.clone())).collect::<BTreeMap<_,_>>();
        let mut total = 0usize;
        for (name, table) in &mut tables {
            table.rows = verification_live_rows(conn, name, &table.columns, self.original.tables[name].rows.len()).await?;
            for row in &table.rows {
                total = total.checked_add(serde_json::to_vec(row)?.len()).context("Ordinary SQL size overflow")?;
                ensure!(total <= 32*1024*1024, "Ordinary whole SQL projection exceeds32MiB");
            }
        }
        self.original.bundle.fence()?;
        Ok(tables)
    }
}
impl<'b> VerificationSqlProjection<'b> {
    pub(crate) async fn prepare_ordinary_writer<'s>(&'s self, writer:&VerificationRecoveryWriter<'_>, window: &OrdinarySqliteActivityWindow) -> anyhow::Result<OrdinarySqlitePreparedProjection<'s,'b>> {
        ensure!(std::ptr::eq(self.original.bundle, writer.selection().bundle()), "Ordinary projection belongs to another bundle");
        writer.fence()?;
        // The actual writer holds durable whole-main OFD exclusion. Borrow its
        // original pinned connection for SELECT-only preflight, without a manual
        // transaction or close_on_drop. A cancelled SELECT can return safely;
        // cancelling the enclosing writer retains its registered VFS callbacks.
        let mut connection=writer.selection().pool().acquire().await?;
        writer.fence()?;
        let mut expected = self.original.tables.iter().filter(|(n,_)| !matches!(n.as_str(), "storage_restore_generations" | "storage_restore_current")).map(|(n,t)|(n.clone(),t.clone())).collect::<BTreeMap<_,_>>();
        let actual = self.ordinary_live_tables(&mut connection).await?;
        let original_receipts = expected.get("publication_receipts").context("Original publication receipts absent")?;
        let current = actual.get("publication_receipts").context("Current publication receipts absent")?;
        let publication=ordinary_sqlite_receipts(original_receipts,current)?;
        // Preflight acknowledges NO receipt transition. It only proves immutable
        // identity before the actual original owner performs re-verification.
        expected.insert("publication_receipts".into(), current.clone());
        let activity=pg_compare_observed_application(&expected, &actual, &window.exercised, window.started, window.finished)?;
        expected.insert("sessions".into(), actual.get("sessions").context("Observed sessions absent")?.clone());
        writer.fence()?;drop(connection);writer.fence()?;
        Ok(OrdinarySqlitePreparedProjection{source:self,expected,publication,activity_sha256:digest(&serde_json::to_vec(&activity)?),activity_count:activity.len()})
    }
}
impl OrdinarySqlitePreparedProjection<'_,'_> {
    pub(crate) fn operations(&self) -> std::collections::BTreeSet<String> { self.publication.keys().cloned().collect() }
    pub(crate) async fn finish(&self, reservation: &mut VerificationReservation, target: &VerificationTarget<'_>, ack: &crate::storage::publication::PublicationRecoveryAck<'_,'_>) -> anyhow::Result<OrdinarySqliteQualified> {
        ensure!(std::ptr::eq(self.source.original.bundle, target.bundle()) && std::ptr::eq(self.source.original.bundle, ack.original_bundle()), "Ordinary owner acknowledgment belongs to another backup");
        target.fence(reservation).await?; ack.fence(&target.pool, target.path())?;
        let mut expected=self.expected.clone(); let receipts=expected.get_mut("publication_receipts").context("Expected receipt set missing")?;
        let id=receipts.columns.iter().position(|c|c.name=="op_id").context("Receipt identity absent")?;
        let verified=receipts.columns.iter().position(|c|c.name=="verified_at").context("Receipt verification field absent")?;
        let mut seen=std::collections::BTreeSet::new();
        for (op,raw,before,after) in ack.changes() {
            ensure!(seen.insert(op.to_owned()), "Duplicate actual owner acknowledgment");
            let expected_change=self.publication.get(op).context("Foreign actual owner receipt")?;
            ensure!(raw==expected_change.0 && before==expected_change.1.as_deref(), "Actual owner did not start from the exact preflight raw receipt");
            let row=receipts.rows.iter_mut().find(|r|text(&r[id]).is_ok_and(|v|v==op)).context("Actual owner receipt absent")?;
            row[verified]=Cell::Text(after.as_bytes().to_vec());
        }
        ensure!(seen==self.operations(), "Actual owner did not acknowledge COMPLETE original receipt set");
        let actual=self.source.ordinary_live_tables(reservation.connection()).await?;
        ensure!(actual.keys().eq(expected.keys()), "Final ordinary table set differs");
        for (name,table) in &expected { let row=&actual[name];ensure!(row.columns==table.columns && row.keys==table.keys && pg_sorted_rows(&row.rows)?==pg_sorted_rows(&table.rows)?, "Final ordinary SQL changed beyond qualified activity/owner: {name}"); }
        let original=self.source.original.tables.iter().filter(|(n,_)|!matches!(n.as_str(),"storage_restore_generations"|"storage_restore_current")).map(|(n,t)|(n.clone(),t.clone())).collect::<BTreeMap<_,_>>();
        let source_sha256=engine_typed_hash(&original)?;let qualified_source_sha256=engine_typed_hash(&expected)?;let target_sha256=engine_typed_hash(&actual)?;
        ensure!(qualified_source_sha256==target_sha256, "Final ordinary typed closure differs");
        let closure=if source_sha256==target_sha256 {
            // Only the registry-owned ACTUAL acknowledgment method may create
            // its private no-op fact. Equality here is never such authority.
            self.source.compare_after_publication_recovery(reservation,target,ack).await?
        }else{VerificationSqlClosure{source_sha256,qualified_source_sha256,target_sha256,tables:actual.len(),owner_acknowledged_noop:false}};
        target.fence(reservation).await?;ack.fence(&target.pool,target.path())?;
        Ok(OrdinarySqliteQualified{closure,activity_sha256:self.activity_sha256.clone(),activity_count:self.activity_count})
    }
}

#[cfg(test)] mod ordinary_sqlite_receipt_controls {
    use super::*;
    fn original()->Table {Table{columns:vec![Column{name:"op_id".into(),kind:"TEXT".into(),required:true},Column{name:"manifest_json".into(),kind:"TEXT".into(),required:true},Column{name:"verified_at".into(),kind:"TEXT".into(),required:false},Column{name:"user_id".into(),kind:"TEXT".into(),required:true}],keys:vec!["op_id".into()],rows:vec![vec![Cell::Text(b"original-operation".to_vec()),Cell::Text(b"{\"raw\":\"original\"}".to_vec()),Cell::Null,Cell::Text(b"original-human".to_vec())]]}}
    #[test] fn receipt_preflight_only_observes_timestamp_and_keeps_complete_raw_identity()->anyhow::Result<()> {let source=original();let mut actual=source.clone();actual.rows[0][2]=Cell::Text(b"2026-10-15T00:01:02+00:00".to_vec());let selected=ordinary_sqlite_receipts(&source,&actual)?;ensure!(selected.len()==1&&selected["original-operation"].0=="{\"raw\":\"original\"}"&&selected["original-operation"].1.as_deref()==Some("2026-10-15T00:01:02+00:00"),"Raw receipt observation differs");Ok(())}
    #[test] fn receipt_preflight_refuses_changed_raw_actor_identity_and_missing_or_duplicate_set()->anyhow::Result<()> {let source=original();for index in [0,1,3]{let mut changed=source.clone();changed.rows[0][index]=Cell::Text(b"foreign".to_vec());ensure!(ordinary_sqlite_receipts(&source,&changed).is_err(),"Foreign immutable receipt column accepted");}let mut missing=source.clone();missing.rows.clear();ensure!(ordinary_sqlite_receipts(&source,&missing).is_err(),"Missing already-verified receipt accepted");let mut duplicate=source.clone();duplicate.rows.push(duplicate.rows[0].clone());ensure!(ordinary_sqlite_receipts(&source,&duplicate).is_err(),"Duplicate receipt accepted");let mut malformed=source.clone();malformed.rows[0][2]=Cell::Blob(b"2026-10-15T00:01:02+00:00".to_vec());ensure!(ordinary_sqlite_receipts(&source,&malformed).is_err(),"Foreign verification storage class accepted");Ok(())}
}
impl OrdinarySqliteQualified {
    pub(crate) async fn fence(&self, projection:&VerificationSqlProjection<'_>, reservation:&mut VerificationReservation, target:&VerificationTarget<'_>) -> anyhow::Result<()> {
        ensure!(std::ptr::eq(projection.original.bundle,target.bundle()), "Final ordinary qualification belongs to another source");
        target.fence(reservation).await?;
        ensure!(projection.original_source_sha256()?==self.closure.source_sha256(), "Original ordinary SQL source changed");
        let actual=projection.ordinary_live_tables(reservation.connection()).await?;
        ensure!(engine_typed_hash(&actual)?==self.closure.target_sha256(), "Sealed ordinary final SQL changed");
        target.fence(reservation).await?;Ok(())
    }
}
pub(crate) fn reserve_sqlite_ordinary_workspace(source:&Root,path:&Path)->anyhow::Result<Root>{ordinary_reserve_workspace(source,path)}
pub(crate) async fn sqlite_ordinary_socket(address:std::net::SocketAddr,bearer:Option<&str>,actor:Option<&str>,document:&str,allowed:bool)->anyhow::Result<Option<Vec<u8>>>{pg_socket_observe(address,bearer,actor,document,allowed).await}
impl VerificationSqlProjection<'_>{pub(crate) fn ordinary_history_sha256(&self)->anyhow::Result<String>{self.original.bundle.fence()?;Ok(digest(&serde_json::to_vec(self.original.tables.get("doc_versions").context("Original application history absent")?)?))}}

impl VerificationSqlProjection<'_>{pub(crate) fn ordinary_is_bound_to(&self,bundle:&crate::storage::backup_inventory::ValidatedBundle)->bool{std::ptr::eq(self.original.bundle,bundle)}}
