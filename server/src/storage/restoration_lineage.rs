/// Captured prior custody, never a serving registration or restoration permit.
pub(crate) struct ValidatedPriorLineage {
    ledger: Ledger,
    raw_ledger: Vec<u8>,
    captured_files: std::collections::BTreeMap<String, crate::storage::backup_inventory::Bytes>,
    captured_identities: std::collections::BTreeMap<String, crate::storage::backup_inventory::SourceFile>,
}
impl ValidatedPriorLineage {
    pub(crate) fn ledger(&self) -> &Ledger { &self.ledger }
    pub(crate) fn raw_ledger(&self) -> &[u8] { &self.raw_ledger }
    /// Resolve only a freshly classified original proof requirement. A history
    /// scope with no matching binding is deliberately not a native-ID fallback.
    pub(crate) fn resolve_original(&self, namespace: &str, op: &str, evidence: &[u8],
        relative: &str, original: Identity, length: u64, sha256: &str) -> io::Result<Identity> {
        if !valid_relative(relative) || !valid_hash(sha256) {
            return Err(fail("Invalid lineage path requirement"));
        }
        let bytes = self.captured_files.get(relative).ok_or_else(|| fail("Required captured lineage bytes missing"))?;
        let source = self.captured_identities.get(relative).ok_or_else(|| fail("Required captured lineage identity missing"))?;
        if bytes.size_bytes != length || bytes.sha256 != sha256 {
            return Err(fail("Captured lineage bytes differ from original proof"));
        }
        let observed = (source.device, source.inode);
        if let Some(scope) = self.ledger.scopes.iter().find(|s| s.namespace == namespace && s.op_id == op) {
            if scope.evidence_sha256 != digest(evidence) {
                return Err(fail("Prior lineage original evidence differs"));
            }
            let binding = scope.bindings.iter().find(|b| b.relative == relative)
                .ok_or_else(|| fail("Prior lineage required binding missing"))?;
            if (binding.original_device, binding.original_inode) != original
                || (binding.adopted_device, binding.adopted_inode) != observed
                || binding.length != length || binding.sha256 != sha256 {
                return Err(fail("Prior lineage captured binding differs"));
            }
        } else if original != observed {
            return Err(fail("Unclassified lineage operation lacks native ownership"));
        }
        Ok(observed)
    }
}

/// Validate an original v1 ledger against the *captured source* root and SQL,
/// never against the copied bundle's physical root. Original scopes and receipts
/// remain unchanged. Archived/multi-generation lineage needs a future parser.
pub(crate) async fn inspect_prior_lineage(
    bundle: &crate::storage::backup_inventory::ValidatedBundle,
    conn: &mut sqlx::AnyConnection,
) -> io::Result<Option<ValidatedPriorLineage>> {
    bundle.fence()?;
    let tables: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table'")
        .fetch_all(&mut *conn).await.map_err(|_| fail("Cannot inspect lineage schema"))?;
    let present = bundle.manifest.source_custody.directories.0.contains_key(NAMESPACE);
    let generations = tables.iter().any(|t| t == "storage_restore_generations");
    let pointers = tables.iter().any(|t| t == "storage_restore_current");
    if generations != pointers { return Err(fail("Incomplete lineage SQL schema")); }
    if !generations {
        if present { return Err(fail("Captured custody lacks lineage SQL schema")); }
        bundle.fence()?; return Ok(None);
    }
    // Preserve every SQL row; unsupported prior history is never silently dropped.
    let rows: Vec<(String,String,String,String,String,String,String,String)> = sqlx::query_as(
        "SELECT generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state FROM storage_restore_generations ORDER BY generation_id")
        .fetch_all(&mut *conn).await.map_err(|_| fail("Cannot inspect lineage generations"))?;
    let current: Vec<(i64,String)> = sqlx::query_as("SELECT singleton,generation_id FROM storage_restore_current ORDER BY singleton")
        .fetch_all(&mut *conn).await.map_err(|_| fail("Cannot inspect lineage pointer"))?;
    if rows.is_empty() && current.is_empty() && !present { bundle.fence()?; return Ok(None); }
    if rows.len() != 1 { return Err(fail("Prior archived lineage requires resolution")); }
    if current.len() != 1 || current[0].0 != 1 || current[0].1 != rows[0].0 || !present {
        return Err(fail("Captured lineage pointer or custody missing"));
    }
    let expected = format!("{NAMESPACE}/{LEDGER_NAME}");
    if bundle.manifest.files.0.keys().any(|p| p.starts_with(&format!("{NAMESPACE}/")) && *p != expected)
        || bundle.manifest.source_custody.directories.0.keys().any(|p|p.starts_with(&format!("{NAMESPACE}/"))) {
        return Err(fail("Prior archived lineage requires resolution"));
    }
    let root = Root::open(&bundle.data_path())?;
    let held = root.leaf(&expected)?;
    let raw_ledger = read_ledger(&held)?;
    let ledger: Ledger = serde_json::from_slice(&raw_ledger).map_err(|_| fail("Malformed captured lineage ledger"))?;
    validate(&ledger)?;
    let row = &rows[0];
    if row.7 != "committed" || row.0 != ledger.generation_id || row.1 != ledger.backup_id
        || row.2 != ledger.manifest_sha256 || row.3 != ledger.database_sha256
        || row.4 != digest(&raw_ledger) || row.5 != ledger.root_device.to_string()
        || row.6 != ledger.root_inode.to_string()
        || (ledger.root_device,ledger.root_inode) != (bundle.manifest.source_custody.root.device,bundle.manifest.source_custody.root.inode) {
        return Err(fail("Captured lineage ledger SQL or source root differs"));
    }
    // The old database hash identifies that generation's original input. The
    // application can legitimately have changed its database since restoration.
    let again: Vec<(String,String,String,String,String,String,String,String)> = sqlx::query_as(
        "SELECT generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state FROM storage_restore_generations ORDER BY generation_id")
        .fetch_all(&mut *conn).await.map_err(|_| fail("Cannot recheck lineage generations"))?;
    let pointer_again: Vec<(i64,String)> = sqlx::query_as("SELECT singleton,generation_id FROM storage_restore_current ORDER BY singleton")
        .fetch_all(&mut *conn).await.map_err(|_| fail("Cannot recheck lineage pointer"))?;
    if again != rows || pointer_again != current || read_ledger(&root.leaf(&expected)?)? != raw_ledger {
        return Err(fail("Captured lineage changed during inspection"));
    }
    bundle.fence()?;
    Ok(Some(ValidatedPriorLineage { ledger, raw_ledger,
        captured_files: bundle.manifest.files.0.clone(),
        captured_identities: bundle.manifest.source_custody.files.0.clone() }))
}
