//! Read-only custody planning with validated prior lineage. This is not an installation grant.
#![cfg(target_os = "linux")]
use super::{
    backup_inventory::{Manifest, ValidatedBundle},
    journal_fs::InstallInspection,
};
#[path = "restoration_upload.rs"]
mod upload;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Serialize)]
pub(crate) struct CustodyPlan {
    backup_id: String,
    source_root: super::backup_inventory::Identity,
    manifest_sha256: String,
    database_sha256: String,
    scopes: Vec<PlannedScope>,
    groups: Vec<CapturedGroup>,
    unresolved: Vec<Unresolved>,
    assets: Vec<AssetEvidence>,
    #[serde(skip_serializing)]
    validated_history: Option<super::restoration::HistorySnapshot>,
    prior_lineage: Option<super::restoration::Ledger>,
    raw_prior_ledger: Option<Vec<u8>>,
    prior_generation_rows: Vec<serde_json::Value>,
    prior_current_rows: Vec<serde_json::Value>,
    portable_restore_available: bool,
}
impl CustodyPlan {
    pub(crate) fn backup_id(&self) -> &str {
        &self.backup_id
    }
    pub(crate) fn source_root(&self) -> &super::backup_inventory::Identity {
        &self.source_root
    }
    pub(crate) fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
    pub(crate) fn database_sha256(&self) -> &str {
        &self.database_sha256
    }
    pub(crate) fn scopes(&self) -> &[PlannedScope] {
        &self.scopes
    }
    pub(crate) fn groups(&self) -> &[CapturedGroup] {
        &self.groups
    }
    pub(crate) fn unresolved(&self) -> &[Unresolved] {
        &self.unresolved
    }
    pub(crate) fn assets(&self) -> &[AssetEvidence] {
        &self.assets
    }
    pub(crate) fn validated_history(&self) -> Option<&super::restoration::HistorySnapshot> {self.validated_history.as_ref()}
    pub(crate) fn prior_lineage(&self) -> Option<&super::restoration::Ledger> {
        self.prior_lineage.as_ref()
    }
    pub(crate) fn raw_prior_ledger(&self) -> Option<&[u8]> {
        self.raw_prior_ledger.as_deref()
    }
    pub(crate) fn prior_generation_rows(&self) -> &[serde_json::Value] {
        &self.prior_generation_rows
    }
    pub(crate) fn prior_current_rows(&self) -> &[serde_json::Value] {
        &self.prior_current_rows
    }
    pub(crate) fn portable_restore_available(&self) -> bool {
        self.portable_restore_available
    }
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) enum OperationClass {
    PreparedUncommitted,
    CommittedPending,
    CommittedComplete,
    CommittedCurrent,
    CommittedHistorical,
    Committed,
    InertReservation,
    CommittedErased,
}
#[derive(Debug, Serialize)]
pub(crate) struct PlannedScope {
    pub namespace: String,
    pub operation_id: String,
    pub class: OperationClass,
    /// Exact original bytes; None means an inert reservation, never made-up evidence.
    pub evidence: Option<Vec<u8>>,
    pub evidence_sha256: Option<String>,
    pub raw_manifest: Option<Vec<u8>>,
    pub raw_sql_receipt: Option<String>,
    pub receipt_seal: Option<Vec<u8>>,
    pub logical_receipt: Option<serde_json::Value>,
    pub escrow_rows: Vec<serde_json::Value>,
    pub retained_paths: Vec<String>,
    pub associations: Vec<AssociationEvidence>,
    pub bindings: Vec<PlannedBinding>,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) enum IdentityResolution {
    OriginalSource,
    PriorLedger,
    NativeAfterPrior,
}
#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct PlannedBinding {
    pub relative: String,
    pub original_device: u64,
    pub original_inode: u64,
    pub captured_device: u64,
    pub captured_inode: u64,
    pub resolution: IdentityResolution,
    pub length: u64,
    pub sha256: String,
}
#[derive(Debug, Serialize)]
pub(crate) struct AssociationEvidence {
    pub kind: String,
    pub subject_id: String,
    pub current_document_id: Option<String>,
}
#[derive(Debug, Serialize)]
pub(crate) struct CapturedGroup {
    pub captured_device: u64,
    pub captured_inode: u64,
    pub captured_links: u64,
    pub length: u64,
    pub sha256: String,
    pub members: Vec<String>,
    /// These paths have independent application evidence; other group members do not.
    pub binding_members: Vec<String>,
}
#[derive(Debug, Serialize)]
pub(crate) struct Unresolved {
    pub namespace: String,
    pub operation_id: Option<String>,
    pub reason: String,
}
#[derive(Debug, Serialize)]
pub(crate) struct AssetEvidence {
    pub provider: String,
    pub operation_id: String,
    pub raw_receipt: String,
    pub current_attachment: bool,
}
// Manifests expose serialization but keep their file fields private to their
// owner modules. Read only the already owner-validated typed output; do not
// parse untrusted raw evidence again or author a replacement manifest.
fn operation_bytes(m: &impl Serialize) -> anyhow::Result<(i64, String)> {
    let v = serde_json::to_value(m)?;
    Ok((
        v["length"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("Typed manifest has no length"))?,
        v["sha256"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Typed manifest has no digest"))?
            .to_owned(),
    ))
}
fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn bound(
    m: &Manifest,
    prior: Option<&super::restoration::ValidatedPriorLineage>,
    namespace: &str,
    op: &str,
    evidence: &[u8],
    relative: &str,
    original: (u64, u64),
    length: u64,
    digest: &str,
) -> anyhow::Result<PlannedBinding> {
    let captured =
        m.source_custody.files.0.get(relative).ok_or_else(|| {
            anyhow::anyhow!("Required path absent from source custody: {relative}")
        })?;
    let bytes =
        m.files.0.get(relative).ok_or_else(|| {
            anyhow::anyhow!("Required path absent from byte inventory: {relative}")
        })?;
    let (physical, resolution) = if let Some(prior) = prior {
        let physical =
            prior.resolve_original(namespace, op, evidence, relative, original, length, digest)?;
        let scoped = prior
            .ledger()
            .scopes
            .iter()
            .any(|s| s.namespace == namespace && s.op_id == op);
        (
            physical,
            if scoped {
                IdentityResolution::PriorLedger
            } else {
                IdentityResolution::NativeAfterPrior
            },
        )
    } else {
        anyhow::ensure!(
            (captured.device, captured.inode) == original,
            "Original proof identity differs from captured source custody: {relative}"
        );
        (original, IdentityResolution::OriginalSource)
    };
    anyhow::ensure!(
        bytes.size_bytes == length && bytes.sha256 == digest,
        "Operation bytes differ from captured source inventory: {relative}"
    );
    Ok(PlannedBinding {
        relative: relative.into(),
        original_device: original.0,
        original_inode: original.1,
        captured_device: physical.0,
        captured_inode: physical.1,
        resolution,
        length,
        sha256: digest.into(),
    })
}
fn install_scope(
    m: &Manifest,
    prior: Option<&super::restoration::ValidatedPriorLineage>,
    namespace: &str,
    op: &str,
    class: OperationClass,
    raw: Vec<u8>,
    sql: Option<String>,
    i: InstallInspection,
    length: i64,
    digest: &str,
    committed: bool,
) -> anyhow::Result<PlannedScope> {
    anyhow::ensure!(length >= 0, "Negative operation length");
    let evidence = i
        .proof
        .as_ref()
        .map(|p| p.raw.clone())
        .unwrap_or_else(|| raw.clone());
    let mut bindings = Vec::new();
    if let Some(proof) = &i.proof {
        // Observed copied inode IDs intentionally do not participate here. The
        // original proof must match capture provenance, including equal-byte impostors.
        for relative in &i.required_group {
            let binding = bound(
                m,
                prior,
                namespace,
                op,
                &evidence,
                relative,
                proof.original,
                length as u64,
                digest,
            )?;
            if committed {
                bindings.push(binding);
            }
        }
        anyhow::ensure!(
            !i.required_group.is_empty(),
            "Proof has no captured installation group"
        );
    }
    let evidence = Some(evidence);
    Ok(PlannedScope {
        namespace: namespace.into(),
        operation_id: op.into(),
        class,
        evidence_sha256: evidence.as_ref().map(|b| hash(b)),
        evidence,
        raw_manifest: Some(raw),
        raw_sql_receipt: sql,
        receipt_seal: None,
        logical_receipt: None,
        escrow_rows: Vec::new(),
        retained_paths: Vec::new(),
        associations: Vec::new(),
        bindings,
    })
}
fn groups(m: &Manifest, scopes: &[PlannedScope]) -> anyhow::Result<Vec<CapturedGroup>> {
    let mut out: BTreeMap<(u64, u64), CapturedGroup> = BTreeMap::new();
    for (path, id) in &m.source_custody.files.0 {
        let bytes = &m.files.0[path];
        let g = out
            .entry((id.device, id.inode))
            .or_insert_with(|| CapturedGroup {
                captured_device: id.device,
                captured_inode: id.inode,
                captured_links: id.links,
                length: bytes.size_bytes,
                sha256: bytes.sha256.clone(),
                members: Vec::new(),
                binding_members: Vec::new(),
            });
        anyhow::ensure!(
            g.captured_links == id.links
                && g.length == bytes.size_bytes
                && g.sha256 == bytes.sha256,
            "Inconsistent captured original hardlink group"
        );
        g.members.push(path.clone());
    }
    for scope in scopes {
        let mut original_to_captured = BTreeMap::new();
        let mut captured_to_original = BTreeMap::new();
        for binding in &scope.bindings {
            let original = (binding.original_device, binding.original_inode);
            let physical = (binding.captured_device, binding.captured_inode);
            if let Some(old) = original_to_captured.insert(original, physical) {
                anyhow::ensure!(
                    old == physical,
                    "Operation original group split across captured groups"
                );
            }
            if let Some(old) = captured_to_original.insert(physical, original) {
                anyhow::ensure!(
                    old == original,
                    "Independent originals merged within operation"
                );
            }
            let g = out
                .get_mut(&(binding.captured_device, binding.captured_inode))
                .ok_or_else(|| anyhow::anyhow!("Binding has no captured original group"))?;
            anyhow::ensure!(
                g.length == binding.length
                    && g.sha256 == binding.sha256
                    && g.members.contains(&binding.relative),
                "Cross-scope original group mismatch"
            );
            if !g.binding_members.contains(&binding.relative) {
                g.binding_members.push(binding.relative.clone());
            }
        }
    }
    for g in out.values_mut() {
        anyhow::ensure!(
            g.captured_links >= g.members.len() as u64,
            "Captured group exceeds original link count"
        );
        g.binding_members.sort();
    }
    Ok(out.into_values().collect())
}
/// Admits a verified stopped-writer snapshot with zero or one validated prior generation.
/// Partial/orphan reservations remain inert. Unresolved entries block any future
/// publication protocol; there is deliberately no restore-enabled result here.
pub(crate) async fn plan(bundle: &ValidatedBundle) -> anyhow::Result<CustodyPlan> {
    anyhow::ensure!(bundle.manifest.historical_composite_preservation_capture.is_none(),"Historical destination certifier is not yet qualified; no generic role fallback");
    plan_inner(bundle,false).await
}
// Purpose-explicit observational plan only. Actual owner publication remains a
// separate certifier transition; parsed historical bytes grant no generic path.
pub(crate) async fn plan_historical_preservation(bundle:&ValidatedBundle)->anyhow::Result<CustodyPlan>{
    anyhow::ensure!(bundle.manifest.historical_composite_preservation_capture.is_some(),"Explicit historical physical evidence absent");
    super::restoration::linux::inspect_historical_snapshot(bundle).await?;
    plan_inner(bundle,true).await
}
async fn plan_inner(bundle:&ValidatedBundle,historical:bool)->anyhow::Result<CustodyPlan>{
    bundle.fence()?;
    let root = bundle.data_path();
    let m = &bundle.manifest;
    let mut conn = bundle.db.acquire().await?;
    let tables: BTreeSet<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table'")
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .collect();
    let prior = super::restoration::inspect_prior_lineage(bundle, &mut conn).await?;
    let mut prior_generation_rows = Vec::new();
    let mut prior_current_rows = Vec::new();
    if prior.is_some() {
        use sqlx::Row;
        for row in sqlx::query("SELECT generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state,created_at FROM storage_restore_generations ORDER BY generation_id").fetch_all(&mut *conn).await? {
            let mut value = serde_json::Map::new();
            for key in ["generation_id","backup_id","manifest_sha256","database_sha256","ledger_sha256","root_device","root_inode","state","created_at"] {value.insert(key.into(),serde_json::json!(row.try_get::<String,_>(key)?));}
            prior_generation_rows.push(serde_json::Value::Object(value));
        }
        for (singleton, generation_id) in sqlx::query_as::<_, (i64, String)>(
            "SELECT singleton,generation_id FROM storage_restore_current ORDER BY singleton",
        )
        .fetch_all(&mut *conn)
        .await?
        {
            prior_current_rows
                .push(serde_json::json!({"singleton":singleton,"generation_id":generation_id}));
        }
    }
    let mut result = CustodyPlan {
        backup_id: m.backup_id.clone(),
        source_root: m.source_custody.root.clone(),
        manifest_sha256: hash(&bundle.raw_manifest),
        database_sha256: m.files.0["starlane-notes.db"].sha256.clone(),
        scopes: Vec::new(),
        groups: Vec::new(),
        unresolved: Vec::new(),
        assets: Vec::new(),
        validated_history: prior.as_ref().map(|p|p.snapshot()),
        prior_lineage: prior.as_ref().map(|p| p.ledger().clone()),
        raw_prior_ledger: prior.as_ref().map(|p| p.raw_ledger().to_vec()),
        prior_generation_rows,
        prior_current_rows,
        portable_restore_available: false,
    };
    let mut canonical = Vec::new();
    for (namespace, table) in [
        (".storage-journal", "storage_journal_receipts"),
        (".publication-journal", "publication_receipts"),
        (".canonical-rewrite-journal", "canonical_revision_receipts"),
        (".move-relocation-journal", "move_commit_receipts"),
    ] {
        let prefix = format!("{namespace}/");
        let mut ops: BTreeSet<String> = m
            .source_custody
            .directories
            .0
            .keys()
            .filter_map(|p| p.strip_prefix(&prefix))
            .filter(|p| !p.contains('/'))
            .map(str::to_owned)
            .collect();
        let committed: BTreeSet<String> = if tables.contains(table) {
            sqlx::query_scalar::<_, String>(&format!("SELECT op_id FROM {table}"))
                .fetch_all(&mut *conn)
                .await?
                .into_iter()
                .collect()
        } else {
            BTreeSet::new()
        };
        ops.extend(committed.iter().cloned());
        for op in ops {
            let inspected=match namespace {
                ".storage-journal"=>super::journal::inspect_restoration(&root,&mut conn,&op).await.map(|i| {
                    let committed=i.class!=super::journal::StorageOperationClass::PreparedUncommitted;
                    install_scope(m,prior.as_ref(),namespace,&op,match i.class {super::journal::StorageOperationClass::PreparedUncommitted=>OperationClass::PreparedUncommitted,super::journal::StorageOperationClass::CommittedPending=>OperationClass::CommittedPending,super::journal::StorageOperationClass::CommittedComplete=>OperationClass::CommittedComplete},i.raw_manifest,None,i.install,i.manifest.length,&i.manifest.sha256,committed)
                }),
                ".publication-journal"=>super::publication::inspect_restoration(&root,&mut conn,&op).await.map(|i| {
                    let committed=i.class!=super::publication::RestorationOperationClass::PreparedUncommitted;
                    let (length,digest)=operation_bytes(&i.manifest)?;
                    install_scope(m,prior.as_ref(),namespace,&op,match i.class {super::publication::RestorationOperationClass::PreparedUncommitted=>OperationClass::PreparedUncommitted,super::publication::RestorationOperationClass::CommittedCurrent=>OperationClass::CommittedCurrent,super::publication::RestorationOperationClass::CommittedHistorical=>OperationClass::CommittedHistorical},i.raw_manifest,i.raw_receipt,i.install,length,&digest,committed)
                }),
                ".canonical-rewrite-journal"=>super::canonical_rewrite::inspect_restoration(&root,&mut conn,&op).await.map(|i| {
                    let committed=i.class!=super::canonical_rewrite::RestorationOperationClass::PreparedUncommitted;
                    let (length,digest)=operation_bytes(&i.manifest)?;
                    let scope=install_scope(m,prior.as_ref(),namespace,&op,match i.class {super::canonical_rewrite::RestorationOperationClass::PreparedUncommitted=>OperationClass::PreparedUncommitted,super::canonical_rewrite::RestorationOperationClass::CommittedCurrent=>OperationClass::CommittedCurrent,super::canonical_rewrite::RestorationOperationClass::CommittedHistorical=>OperationClass::CommittedHistorical},i.raw_manifest.clone(),i.raw_receipt.clone(),i.install.clone(),length,&digest,committed);
                    if scope.is_ok() {canonical.push(i);}scope
                }),
                _=>super::relocation::restoration_inspection::inspect(&root,&mut conn,&op).await.map(|i| {
                    let mut bindings=Vec::new();
                    for p in &i.paths {if let (Some(original),Some(observed))=(p.original_identity,p.observed.as_ref()) {
                        // Optional changed historical source has no binding. Required
                        // destinations must match both proof and capture exactly.
                        let exact=observed.length==p.expected_length && observed.sha256==p.expected_sha256;
                        let binding = bound(m,prior.as_ref(),namespace,&op,&i.manifest_bytes,&p.relative,original,p.expected_length,&p.expected_sha256);
                        if p.required || exact && binding.is_ok() {
                            let binding=binding?;
                            if i.state==super::relocation::restoration_inspection::CommitState::Committed {bindings.push(binding);}
                        }
                    }}
                    let raw=i.manifest_bytes;
                    Ok(PlannedScope{namespace:namespace.into(),operation_id:op.clone(),class:match i.state {super::relocation::restoration_inspection::CommitState::PreparedUncommitted=>OperationClass::PreparedUncommitted,super::relocation::restoration_inspection::CommitState::Committed=>OperationClass::Committed},evidence_sha256:Some(hash(&raw)),evidence:Some(raw.clone()),raw_manifest:Some(raw),raw_sql_receipt:i.logical_receipt.as_ref().and_then(|r|r.get("manifest_json")).and_then(serde_json::Value::as_str).map(str::to_owned),receipt_seal:i.receipt_bytes,logical_receipt:i.logical_receipt,escrow_rows:i.escrow_rows,retained_paths:Vec::new(),associations:i.associations.into_iter().map(|(kind,subject_id,a)|AssociationEvidence {kind,subject_id,current_document_id:match a {super::relocation::restoration_inspection::Association::Current {document_id}=>Some(document_id),super::relocation::restoration_inspection::Association::Historical=>None}}).collect(),bindings})
                }),
            };
            match inspected {
                Ok(scope) => {
                    let mut scope = scope?;
                    scope.retained_paths = m
                        .files
                        .0
                        .keys()
                        .filter(|p| p.starts_with(&format!("{namespace}/{op}/")))
                        .cloned()
                        .collect();
                    if scope.class == OperationClass::CommittedPending && scope.bindings.is_empty()
                    {
                        result.unresolved.push(Unresolved {
                            namespace: namespace.into(),
                            operation_id: Some(op.clone()),
                            reason:
                                "pending_without_original_install_proof_requires_explicit_protocol"
                                    .into(),
                        });
                    }
                    result.scopes.push(scope);
                }
                Err(_) => {
                    result.unresolved.push(Unresolved {
                        namespace: namespace.into(),
                        operation_id: Some(op.clone()),
                        reason: if committed.contains(&op) {
                            "committed_evidence_requires_review"
                        } else {
                            "orphan_stage_preserved_without_authority"
                        }
                        .into(),
                    });
                    result.scopes.push(PlannedScope {
                        namespace: namespace.into(),
                        operation_id: op.clone(),
                        class: OperationClass::InertReservation,
                        evidence: None,
                        evidence_sha256: None,
                        raw_manifest: None,
                        raw_sql_receipt: None,
                        receipt_seal: None,
                        logical_receipt: None,
                        escrow_rows: Vec::new(),
                        retained_paths: m
                            .files
                            .0
                            .keys()
                            .filter(|p| p.starts_with(&format!("{namespace}/{op}/")))
                            .cloned()
                            .collect(),
                        associations: Vec::new(),
                        bindings: Vec::new(),
                    });
                }
            }
        }
        // Every byte is retained by the inventory; non-operation namespace files
        // cannot silently become operation evidence.
        if m.files
            .0
            .keys()
            .any(|p| p.strip_prefix(&prefix).is_some_and(|s| !s.contains('/')))
        {
            result.unresolved.push(Unresolved {
                namespace: namespace.into(),
                operation_id: None,
                reason: "unclassified_namespace_root_file".into(),
            });
        }
    }
    if m.composite_admission_capture.is_some(){super::restoration::linux::inspect_composite_snapshot(bundle).await?;}
    let completed=if historical{Some(super::restoration::linux::historical_completed_plan_facts(bundle,&mut conn).await?)}else if (m.completed_absence_capture.is_some()||m.prior_completed_absence_capture.is_some()||m.mixed_completed_absence_capture.is_some()||m.composite_admission_capture.is_some()){Some(super::attachment_upload::admission::committed_erasure::accounting::capture::ClassifiedCompletedBundle::inspect_for_plan(bundle,&mut conn).await?)}else{None};
    if tables.contains("attachment_erasure_dispositions")&&completed.is_none(){
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_erasure_dispositions").fetch_one(&mut *conn).await?;
        if count>0{result.unresolved.push(Unresolved{namespace:"attachment_erasure_dispositions".into(),operation_id:None,reason:"completed_absence_capture_missing".into()});}
    }
    if tables.contains("attachment_upload_receipts") || m.source_custody.directories.0.contains_key(".attachment-upload-journal") {
        upload::append(&root, m, prior.as_ref(), &mut conn, &mut result, tables.contains("attachment_upload_receipts"),completed.as_ref()).await?;
    }
    if tables.contains("asset_replay_receipts") {
        let assets = crate::sync::restoration_assets::classify_at(&root, &mut conn)
            .await
            .map_err(anyhow::Error::msg)?;
        for a in assets {
            if a.requires_canonical_evidence
                && !canonical
                    .iter()
                    .any(|i| i.matches_asset_receipt(&a.receipt))
            {
                result.unresolved.push(Unresolved {
                    namespace: "asset_replay_receipts".into(),
                    operation_id: Some(a.receipt.incoming.operation_id.clone()),
                    reason: "published_source_dependency_requires_resolution".into(),
                });
            }
            result.assets.push(AssetEvidence {
                provider: a.receipt.incoming.provider,
                operation_id: a.receipt.incoming.operation_id,
                raw_receipt: a.raw_receipt,
                current_attachment: a.current_attachment,
            });
        }
    }
    if let Some(prior) = &prior {
        for old in &prior.ledger().scopes {
            if !result
                .scopes
                .iter()
                .any(|s| s.namespace == old.namespace && s.operation_id == old.op_id)
            {
                result.unresolved.push(Unresolved {
                    namespace: old.namespace.clone(),
                    operation_id: Some(old.op_id.clone()),
                    reason: "prior_scope_preserved_without_fresh_classification".into(),
                });
                result.scopes.push(PlannedScope {
                    namespace: old.namespace.clone(),
                    operation_id: old.op_id.clone(),
                    class: OperationClass::InertReservation,
                    evidence: None,
                    evidence_sha256: Some(old.evidence_sha256.clone()),
                    raw_manifest: None,
                    raw_sql_receipt: None,
                    receipt_seal: None,
                    logical_receipt: None,
                    escrow_rows: Vec::new(),
                    retained_paths: Vec::new(),
                    associations: Vec::new(),
                    bindings: Vec::new(),
                });
            }
        }
    }
    result.groups = groups(m, &result.scopes)?;
    drop(conn);
    bundle.fence()?;
    Ok(result)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::path::PathBuf;
    // Public producers read process-wide DATA_DIR. Run genuine fixtures in
    // separate processes so parallel Cargo tests never share that state.
    fn isolated_fixture(name: &str) -> bool {
        if std::env::var("STARLANE_CUSTODY_PLAN_TEST_CASE").as_deref() == Ok(name) {
            return false;
        }
        let exact = format!("storage::restoration_plan::tests::{name}");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &exact, "--nocapture", "--test-threads=1"])
            .env("STARLANE_CUSTODY_PLAN_TEST_CASE", name)
            .output()
            .expect("Launch isolated custody fixture");
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        true
    }
    struct Fixture {
        source: PathBuf,
        bundle: PathBuf,
        manifest: super::super::journal::StagedDocument,
    }
    impl Fixture {
        async fn new(complete: bool, rollback: bool) -> Self {
            sqlx::any::install_default_drivers();
            let base = std::env::temp_dir().join(format!("custody-plan-{}", uuid::Uuid::new_v4()));
            let source = base.join("source");
            let bundle = base.join("bundle");
            std::fs::create_dir_all(&source).unwrap();
            std::env::set_var("DATA_DIR", &source);
            let db = sqlx::any::AnyPoolOptions::new()
                .max_connections(1)
                .connect(&format!(
                    "sqlite://{}?mode=rwc",
                    source.join("starlane-notes.db").display()
                ))
                .await
                .unwrap();
            sqlx::migrate!("./migrations").run(&db).await.unwrap();
            sqlx::query("INSERT INTO users(id,email,display_name) VALUES('u','u@test','User')")
                .execute(&db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO spaces(id,name,slug,created_by) VALUES('s','Space','s','u')")
                .execute(&db)
                .await
                .unwrap();
            let mut tx = db.begin().await.unwrap();
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at) VALUES('d','s','Doc','temporary','u','u','initial')").execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('draft','d','s','u','Doc','Private body','initial')").execute(&mut *tx).await.unwrap();
            let manifest = super::super::journal::stage_document(
                &mut tx,
                "s",
                "d",
                "draft",
                "initial",
                "s",
                "Doc",
                "",
                "Private body",
            )
            .await
            .unwrap();
            sqlx::query("UPDATE documents SET content_path=$1 WHERE id='d'")
                .bind(&manifest.path)
                .execute(&mut *tx)
                .await
                .unwrap();
            if rollback {
                tx.rollback().await.unwrap();
            } else {
                tx.commit().await.unwrap();
            }
            if complete {
                assert!(super::super::journal::publish_committed(&db, &manifest).await);
            }
            db.close().await;
            Self {
                source,
                bundle,
                manifest,
            }
        }
        fn capture(&self) {
            let out = std::process::Command::new("python3")
                .arg(std::env::var_os("STARLANE_RESTORATION_BACKUP_SCRIPT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/test-support/backup_set.py"))))
                .arg("create")
                .arg(&self.source)
                .arg(&self.bundle)
                .args(["--application-version", "1.5.1", "--writers-stopped"])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        fn proof(&self) -> PathBuf {
            self.source.join(format!(
                ".storage-journal/{}/install-proof.json",
                self.manifest.op_id
            ))
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.source.parent().unwrap());
        }
    }
    #[tokio::test]
    async fn actual_completed_producer_maps_capture_and_preserves_exact_proof() {
        if isolated_fixture("actual_completed_producer_maps_capture_and_preserves_exact_proof") { return; }
        let f = Fixture::new(true, false).await;
        let proof = std::fs::read(f.proof()).unwrap();
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let plan = plan(&b).await.unwrap();
        assert!(plan.unresolved.is_empty(), "{:?}", plan.unresolved);
        let s = &plan.scopes[0];
        assert_eq!(s.class, OperationClass::CommittedComplete);
        assert_eq!(s.evidence.as_deref(), Some(proof.as_slice()));
        assert_eq!(s.evidence_sha256.as_deref(), Some(hash(&proof).as_str()));
        assert_eq!(s.bindings.len(), 2); // retained anchor + original temporary anchor; completed mutable canonical excluded
        assert!(s
            .bindings
            .iter()
            .all(|p| p.relative.starts_with(".storage-journal/")));
        let g = plan
            .groups
            .iter()
            .find(|g| g.binding_members.len() == 2)
            .unwrap();
        assert_eq!(g.members.len(), 3);
        assert!(g.members.contains(&f.manifest.path));
        assert_eq!(
            std::fs::read(f.bundle.join(format!(
                "data/.storage-journal/{}/install-proof.json",
                f.manifest.op_id
            )))
            .unwrap(),
            proof
        );
        assert!(!plan.portable_restore_available);
        b.db.close().await;
    }
    #[tokio::test]
    async fn proof_identity_tamper_rejected_despite_exact_bytes_and_valid_inventory() {
        if isolated_fixture("proof_identity_tamper_rejected_despite_exact_bytes_and_valid_inventory") { return; }
        let f = Fixture::new(true, false).await;
        let mut proof: serde_json::Value =
            serde_json::from_slice(&std::fs::read(f.proof()).unwrap()).unwrap();
        proof["dev"] = serde_json::json!(proof["dev"].as_u64().unwrap() + 1);
        std::fs::write(f.proof(), serde_json::to_vec(&proof).unwrap()).unwrap();
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let err = plan(&b).await.unwrap_err();
        assert!(err.to_string().contains("Original proof identity differs"));
        b.db.close().await;
    }
    #[tokio::test]
    async fn equal_byte_wrong_original_anchor_is_rejected() {
        if isolated_fixture("equal_byte_wrong_original_anchor_is_rejected") { return; }
        let f = Fixture::new(true, false).await;
        let anchor = f.source.join(format!(
            ".storage-journal/{}/install-anchor",
            f.manifest.op_id
        ));
        let bytes = std::fs::read(&anchor).unwrap();
        std::fs::remove_file(&anchor).unwrap();
        std::fs::write(&anchor, bytes).unwrap();
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        assert!(plan(&b)
            .await
            .unwrap_err()
            .to_string()
            .contains("Original proof identity differs"));
        b.db.close().await;
    }
    #[tokio::test]
    async fn actual_rollback_orphan_reserves_scope_without_authority() {
        if isolated_fixture("actual_rollback_orphan_reserves_scope_without_authority") { return; }
        let f = Fixture::new(false, true).await;
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&b).await.unwrap();
        assert_eq!(p.scopes[0].class, OperationClass::PreparedUncommitted);
        assert!(p.scopes[0].bindings.is_empty());
        assert!(p.scopes[0].evidence.is_some());
        b.db.close().await;
    }
    #[tokio::test]
    async fn actual_pending_without_proof_captures_and_plans_with_explicit_unresolved_creation() {
        if isolated_fixture("actual_pending_without_proof_captures_and_plans_with_explicit_unresolved_creation") { return; }
        let f = Fixture::new(false, false).await;
        let db = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&format!(
                "sqlite://{}?mode=ro",
                f.source.join("starlane-notes.db").display()
            ))
            .await
            .unwrap();
        let mut conn = db.acquire().await.unwrap();
        let i = super::super::journal::inspect_restoration(&f.source, &mut conn, &f.manifest.op_id)
            .await
            .unwrap();
        assert_eq!(
            i.class,
            super::super::journal::StorageOperationClass::CommittedPending
        );
        assert!(i.install.proof.is_none());
        drop(conn);
        db.close().await;
        let out = std::process::Command::new("python3")
            .arg(std::env::var_os("STARLANE_RESTORATION_BACKUP_SCRIPT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/test-support/backup_set.py"))))
            .arg("create")
            .arg(&f.source)
            .arg(&f.bundle)
            .args(["--application-version", "1.5.1", "--writers-stopped"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(!f.proof().exists());
        assert!(!f.source.join(&f.manifest.path).exists());
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&b).await.unwrap();
        assert_eq!(p.scopes[0].class, OperationClass::CommittedPending);
        assert!(p.scopes[0].bindings.is_empty());
        assert!(p.unresolved.iter().any(
            |u| u.reason == "pending_without_original_install_proof_requires_explicit_protocol"
        ));
        assert_eq!(
            p.scopes[0].raw_manifest.as_ref(),
            Some(
                &std::fs::read(f.source.join(format!(
                    ".storage-journal/{}/manifest.json",
                    f.manifest.op_id
                )))
                .unwrap()
            )
        );
        assert!(!b.data_path().join(&f.manifest.path).exists());
        assert!(!p.portable_restore_available);
        b.db.close().await;
        if std::env::var_os("PLANNER_RETAIN_PENDING").is_some() {
            println!(
                "Retained actual pending source={} bundle={}",
                f.source.display(),
                f.bundle.display()
            );
            std::mem::forget(f);
        }
    }
    #[tokio::test]
    async fn prior_custody_namespace_explicitly_refused() {
        if isolated_fixture("prior_custody_namespace_explicitly_refused") { return; }
        let f = Fixture::new(false, true).await;
        std::fs::create_dir(f.source.join(".restoration-custody")).unwrap();
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        assert!(plan(&b).await.unwrap_err().to_string().contains("lineage"));
        b.db.close().await;
    }
    #[tokio::test]
    async fn malformed_proof_and_partial_stage_retained_inert() {
        if isolated_fixture("malformed_proof_and_partial_stage_retained_inert") { return; }
        let f = Fixture::new(true, false).await;
        std::fs::write(f.proof(), b"{}").unwrap();
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&b).await.unwrap();
        assert_eq!(p.scopes[0].class, OperationClass::InertReservation);
        assert!(p.scopes[0].bindings.is_empty());
        assert!(p.scopes[0].evidence.is_none());
        assert!(!p.scopes[0].retained_paths.is_empty());
        assert!(!p.unresolved.is_empty());
        b.db.close().await;
    }
    async fn immutable_fixture(
        rewrite: bool,
        lose_historical_target: bool,
    ) -> (PathBuf, PathBuf, String, String, Vec<u8>) {
        let base = std::env::temp_dir().join(format!("immutable-plan-{}", uuid::Uuid::new_v4()));
        let source = base.join("source");
        let bundle = base.join("bundle");
        std::fs::create_dir_all(&source).unwrap();
        let mut state = crate::spaces::properties::tests::property_test_state().await;
        state.db.close().await;
        state.db = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&format!(
                "sqlite://{}?mode=rwc",
                source.join("starlane-notes.db").display()
            ))
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
        for statement in ["INSERT INTO users(id,email,display_name,role) VALUES('owner-a','owner@plan.test','Owner','member')", "INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('space-a','A','a','private','owner-a'),('space-b','B','b','private','owner-a')", "INSERT INTO space_members(id,space_id,user_id,role) VALUES('member-owner','space-a','owner-a','owner')", "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','Target','','owner-a','owner-a','old'),('target-b','space-b','Other','','owner-a','owner-a','old')"] {sqlx::query(statement).execute(&state.db).await.unwrap();}
        let mut auth = crate::spaces::documents::tests::test_auth("owner-a");
        auth.claims.sid = "planner-owner".into();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('planner-owner','owner-a','test','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE spaces SET slug='a' WHERE id='space-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET content_path='spaces/a/docs/old.md',updated_at='old',published_at='old' WHERE id='target-a'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('publish-draft','target-a','space-a','owner-a','New','PRIVATE NEXT','old')").execute(&state.db).await.unwrap();
        std::fs::create_dir_all(source.join("spaces/a/docs")).unwrap();
        let old = b"PUBLIC PREVIOUS LEGITIMATE BYTES".to_vec();
        std::fs::write(source.join("spaces/a/docs/old.md"), &old).unwrap();
        std::env::set_var("DATA_DIR", &source);
        let mut tx = state.db.begin().await.unwrap();
        crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &["space-a"])
            .await
            .unwrap();
        let (namespace, op, target) = if rewrite {
            let m = super::super::canonical_rewrite::prepare_rewrite(
                &mut tx,
                &auth,
                super::super::canonical_rewrite::RewriteIntent {
                    space_id: "space-a".into(),
                    document_id: "target-a".into(),
                    source_path: "spaces/a/docs/old.md".into(),
                    source_revision: "old".into(),
                    published_stamp: "old".into(),
                    revision: "new".into(),
                    source_sha256: None,
                    reason: super::super::canonical_rewrite::RewriteReason::ReplayContent,
                    incoming: super::super::canonical_rewrite::IncomingIdentity {
                        provider: "server".into(),
                        operation_id: "legacy:planner".into(),
                        entity_type: "document".into(),
                        entity_id: "target-a".into(),
                        action: "update_content".into(),
                        payload_fingerprint: hash(b"original request"),
                    },
                },
                b"PUBLIC REWRITE",
            )
            .await
            .unwrap();
            sqlx::query("UPDATE documents SET content_path=$1,updated_at='new',last_updated_by='owner-a' WHERE id='target-a'").bind(m.path()).execute(&mut *tx).await.unwrap();
            super::super::canonical_rewrite::record_rewrite(&mut tx, &auth, &m)
                .await
                .unwrap();
            (
                ".canonical-rewrite-journal",
                m.op_id().to_owned(),
                m.path().to_owned(),
            )
        } else {
            let m = super::super::publication::prepare_publication(
                &mut tx,
                &auth,
                super::super::publication::PublicationIntent {
                    op_id: uuid::Uuid::new_v4().to_string(),
                    space_id: "space-a".into(),
                    document_id: "target-a".into(),
                    draft_id: "publish-draft".into(),
                    source_path: "spaces/a/docs/old.md".into(),
                    source_revision: "old".into(),
                    revision: "new".into(),
                    payload_fingerprint: hash(b"original publication request"),
                },
                "",
                "PUBLIC NEXT",
            )
            .await
            .unwrap();
            sqlx::query("UPDATE documents SET content_path=$1,updated_at='new',published_at='new',last_updated_by='owner-a' WHERE id='target-a'").bind(m.path()).execute(&mut *tx).await.unwrap();
            super::super::publication::record_publication(&mut tx, &auth, &m)
                .await
                .unwrap();
            (
                ".publication-journal",
                m.op_id().to_owned(),
                m.path().to_owned(),
            )
        };
        // The actual producer has installed before SQL commit, never after it.
        assert!(source.join(&target).is_file());
        tx.commit().await.unwrap();
        if lose_historical_target {
            sqlx::query("UPDATE documents SET content_path='spaces/a/docs/old.md',updated_at='later' WHERE id='target-a'").execute(&state.db).await.unwrap();
            std::fs::remove_file(source.join(&target)).unwrap();
        }
        let documents: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT d.id,d.content_path,s.slug FROM documents d JOIN spaces s ON s.id=d.space_id",
        )
        .fetch_all(&state.db)
        .await
        .unwrap();
        for (id, path, slug) in documents {
            let path = if path.is_empty() {
                let path = format!("spaces/{slug}/docs/{id}.md");
                sqlx::query("UPDATE documents SET content_path=$1 WHERE id=$2")
                    .bind(&path)
                    .bind(&id)
                    .execute(&state.db)
                    .await
                    .unwrap();
                path
            } else {
                path
            };
            let file = source.join(path);
            if !file.exists() {
                std::fs::create_dir_all(file.parent().unwrap()).unwrap();
                std::fs::write(file, b"UNRELATED EXISTING PAGE").unwrap();
            }
        }
        state.db.close().await;
        assert!(source.join("starlane-notes.db").is_file());
        let out = std::process::Command::new("python3")
            .arg(std::env::var_os("STARLANE_RESTORATION_BACKUP_SCRIPT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/test-support/backup_set.py"))))
            .arg("create")
            .arg(&source)
            .arg(&bundle)
            .args(["--application-version", "1.5.1", "--writers-stopped"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        (source, bundle, namespace.into(), op, old)
    }
    #[tokio::test]
    async fn actual_publication_and_rewrite_commits_keep_previous_source_and_map_only_owned_group()
    {
        if isolated_fixture("actual_publication_and_rewrite_commits_keep_previous_source_and_map_only_owned_group") { return; }
        for rewrite in [false, true] {
            let (source, path, namespace, op, old) = immutable_fixture(rewrite, false).await;
            let b = super::super::backup_inventory::verify(&path).await.unwrap();
            let p = plan(&b).await.unwrap();
            let s = p
                .scopes
                .iter()
                .find(|s| s.namespace == namespace && s.operation_id == op)
                .unwrap();
            assert_eq!(s.class, OperationClass::CommittedCurrent);
            assert_eq!(s.bindings.len(), 3);
            assert!(p.unresolved.is_empty(), "{:?}", p.unresolved);
            assert_eq!(
                std::fs::read(source.join("spaces/a/docs/old.md")).unwrap(),
                old
            );
            assert!(!s
                .bindings
                .iter()
                .any(|b| b.relative == "spaces/a/docs/old.md"));
            let proof =
                std::fs::read(source.join(format!("{namespace}/{op}/install-proof.json"))).unwrap();
            assert_eq!(s.evidence.as_ref(), Some(&proof));
            b.db.close().await;
            std::fs::remove_dir_all(source.parent().unwrap()).unwrap();
        }
    }
    #[tokio::test]
    async fn historical_committed_immutable_loss_is_unresolved_never_recreated() {
        if isolated_fixture("historical_committed_immutable_loss_is_unresolved_never_recreated") { return; }
        for rewrite in [false, true] {
            let (source, path, namespace, op, old) = immutable_fixture(rewrite, true).await;
            let b = super::super::backup_inventory::verify(&path).await.unwrap();
            let p = plan(&b).await.unwrap();
            let s = p
                .scopes
                .iter()
                .find(|s| s.namespace == namespace && s.operation_id == op)
                .unwrap();
            assert_eq!(s.class, OperationClass::InertReservation);
            assert!(s.bindings.is_empty());
            assert!(p
                .unresolved
                .iter()
                .any(|u| u.reason == "committed_evidence_requires_review"));
            assert_eq!(
                std::fs::read(source.join("spaces/a/docs/old.md")).unwrap(),
                old
            );
            b.db.close().await;
            std::fs::remove_dir_all(source.parent().unwrap()).unwrap();
        }
    }

    #[tokio::test]
    async fn actual_move_optional_equal_byte_new_inode_source_has_no_binding() {
        if isolated_fixture("actual_move_optional_equal_byte_new_inode_source_has_no_binding") { return; }
        let f = Fixture::new(true, false).await;
        let db = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&format!(
                "sqlite://{}?mode=rw",
                f.source.join("starlane-notes.db").display()
            ))
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO spaces(id,name,slug,created_by) VALUES('dest','Destination','dest','u')",
        )
        .execute(&db)
        .await
        .unwrap();
        let destination = format!("spaces/dest/docs/revisions/{}.md", uuid::Uuid::new_v4());
        let m = super::super::relocation::prepare(
            &f.source,
            &uuid::Uuid::new_v4().to_string(),
            &"a".repeat(64),
            &"b".repeat(64),
            &[super::super::relocation::FilePlan {
                subject_id: "d".into(),
                kind: "canonical".into(),
                source: f.manifest.path.clone(),
                destination: destination.clone(),
                sha256: None,
                maximum: 6_000_000,
            }],
        )
        .unwrap();
        let expected = super::super::relocation::escrow_binding(&[]).unwrap();
        let receipt = serde_json::json!({"op_id":m.op_id,"preview_id":uuid::Uuid::new_v4().to_string(),"actor_id":"u","source_space_id":"s","destination_space_id":"dest","request_fingerprint":m.request_fingerprint,"source_fingerprint":m.source_fingerprint,"manifest_json":serde_json::to_string(&m).unwrap(),"expected_state_json":expected.to_string(),"created_at":"2026-10-10T00:00:00Z"});
        let mut tx = db.begin().await.unwrap();
        super::super::relocation::seal_receipt(&f.source, &receipt).unwrap();
        sqlx::query("UPDATE documents SET space_id='dest',content_path=$1 WHERE id='d'")
            .bind(&destination)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO move_commit_receipts(op_id,preview_id,actor_id,source_space_id,destination_space_id,request_fingerprint,source_fingerprint,manifest_json,expected_state_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(receipt["op_id"].as_str().unwrap()).bind(receipt["preview_id"].as_str().unwrap()).bind("u").bind("s").bind("dest").bind(receipt["request_fingerprint"].as_str().unwrap()).bind(receipt["source_fingerprint"].as_str().unwrap()).bind(receipt["manifest_json"].as_str().unwrap()).bind(receipt["expected_state_json"].as_str().unwrap()).bind(receipt["created_at"].as_str().unwrap()).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        db.close().await;
        let bytes = std::fs::read(f.source.join(&f.manifest.path)).unwrap();
        std::fs::remove_file(f.source.join(&f.manifest.path)).unwrap();
        std::fs::write(f.source.join(&f.manifest.path), &bytes).unwrap();
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&b).await.unwrap();
        let s = p
            .scopes
            .iter()
            .find(|s| s.namespace == ".move-relocation-journal")
            .unwrap();
        assert_eq!(s.class, OperationClass::Committed);
        assert_eq!(s.bindings.len(), 1);
        assert_eq!(s.bindings[0].relative, destination);
        assert!(!s.bindings.iter().any(|p| p.relative == f.manifest.path));
        assert_eq!(
            s.evidence.as_ref(),
            Some(
                &std::fs::read(f.source.join(format!(
                    ".move-relocation-journal/{}/manifest.json",
                    m.op_id
                )))
                .unwrap()
            )
        );
        assert!(s.receipt_seal.is_some());
        assert!(s.logical_receipt.is_some());
        assert_eq!(
            std::fs::read(b.data_path().join(&f.manifest.path)).unwrap(),
            bytes
        );
        b.db.close().await;
    }
    // Mechanical private-root fixture built only from genuine producer evidence.
    // This is not the future application publication protocol.
    async fn prior_fixture(remove_binding: bool, extra_scope: bool) -> (Fixture, Vec<u8>) {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let mut f = Fixture::new(true, false).await;
        let proof = std::fs::read(f.proof()).unwrap();
        f.capture();
        let original = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let first = plan(&original).await.unwrap();
        original.db.close().await;
        let adopted = f.source.parent().unwrap().join("adopted");
        std::fs::create_dir(&adopted).unwrap();
        std::fs::set_permissions(&adopted, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut links = BTreeMap::new();
        fn copy_tree(
            from: &std::path::Path,
            to: &std::path::Path,
            links: &mut BTreeMap<(u64, u64), PathBuf>,
        ) {
            use std::os::unix::fs::MetadataExt;
            for e in std::fs::read_dir(from).unwrap() {
                let e = e.unwrap();
                let src = e.path();
                let dest = to.join(e.file_name());
                let md = e.metadata().unwrap();
                if md.is_dir() {
                    std::fs::create_dir(&dest).unwrap();
                    copy_tree(&src, &dest, links);
                    std::fs::set_permissions(&dest, md.permissions()).unwrap();
                } else {
                    let id = (md.dev(), md.ino());
                    if let Some(existing) = links.get(&id) {
                        std::fs::hard_link(existing, &dest).unwrap();
                    } else {
                        std::fs::copy(&src, &dest).unwrap();
                        links.insert(id, dest);
                    }
                }
            }
        }
        copy_tree(&f.source, &adopted, &mut links);
        let root = std::fs::metadata(&adopted).unwrap();
        let mut scopes = Vec::new();
        for s in &first.scopes {
            scopes.push(super::super::restoration::Scope {
                namespace: s.namespace.clone(),
                op_id: s.operation_id.clone(),
                evidence_sha256: s.evidence_sha256.clone().unwrap(),
                bindings: s
                    .bindings
                    .iter()
                    .filter(|_| !remove_binding)
                    .map(|b| {
                        let md = std::fs::metadata(adopted.join(&b.relative)).unwrap();
                        super::super::restoration::Binding {
                            relative: b.relative.clone(),
                            original_device: b.original_device,
                            original_inode: b.original_inode,
                            adopted_device: md.dev(),
                            adopted_inode: md.ino(),
                            length: b.length,
                            sha256: b.sha256.clone(),
                        }
                    })
                    .collect(),
            });
        }
        if extra_scope {
            scopes.push(super::super::restoration::Scope {
                namespace: ".storage-journal".into(),
                op_id: uuid::Uuid::new_v4().to_string(),
                evidence_sha256: "d".repeat(64),
                bindings: Vec::new(),
            });
        }
        let ledger = super::super::restoration::Ledger { historical_preservation:None, composite:None,completed_absence:None,
            admission:None, version: 1,
            generation_id: uuid::Uuid::new_v4().to_string(),
            backup_id: first.backup_id,
            manifest_sha256: first.manifest_sha256,
            database_sha256: first.database_sha256,
            root_device: root.dev(),
            root_inode: root.ino(),
            scopes,
        };
        std::fs::write(
            adopted.join(".starlane-restore-incomplete"),
            b"private fixture",
        )
        .unwrap();
        let db = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&format!(
                "sqlite://{}?mode=rw",
                adopted.join("starlane-notes.db").display()
            ))
            .await
            .unwrap();
        super::super::restoration::prepare_in(&adopted, &db, &ledger)
            .await
            .unwrap();
        std::fs::remove_file(adopted.join(".starlane-restore-incomplete")).unwrap();
        super::super::restoration::activate_in(&adopted, &db)
            .await
            .unwrap();
        db.close().await;
        f.source = adopted;
        f.bundle = f.bundle.parent().unwrap().join("repeat-bundle");
        std::env::set_var("DATA_DIR", &f.source);
        (f, proof)
    }
    #[tokio::test]
    async fn prior_exact_ledger_resolves_original_ids_and_keeps_raw_history() {
        if isolated_fixture("prior_exact_ledger_resolves_original_ids_and_keeps_raw_history") { return; }
        let (f, proof) = prior_fixture(false, true).await;
        let old = std::fs::read(f.source.join(".restoration-custody/current.json")).unwrap();
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&b).await.unwrap();
        assert_eq!(p.raw_prior_ledger.as_ref(), Some(&old));
        assert_eq!(p.prior_generation_rows.len(), 1);
        assert_eq!(p.prior_current_rows.len(), 1);
        let scope = p
            .scopes
            .iter()
            .find(|s| s.operation_id == f.manifest.op_id)
            .unwrap();
        assert_eq!(scope.evidence.as_ref(), Some(&proof));
        assert_eq!(scope.bindings.len(), 2);
        assert!(scope
            .bindings
            .iter()
            .all(|b| b.resolution == IdentityResolution::PriorLedger
                && b.original_inode != b.captured_inode));
        assert!(p
            .scopes
            .iter()
            .any(|s| s.evidence_sha256.as_deref() == Some(&"d".repeat(64))
                && s.bindings.is_empty()
                && s.evidence.is_none()));
        assert_eq!(std::fs::read(f.proof()).unwrap(), proof);
        b.db.close().await;
    }
    #[tokio::test]
    async fn prior_scope_missing_binding_never_falls_back_to_bytes_or_native_ids() {
        if isolated_fixture("prior_scope_missing_binding_never_falls_back_to_bytes_or_native_ids") { return; }
        let (f, _) = prior_fixture(true, false).await;
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        assert!(plan(&b)
            .await
            .unwrap_err()
            .to_string()
            .contains("required binding missing"));
        b.db.close().await;
    }
    #[tokio::test]
    async fn prior_original_evidence_tamper_is_rejected_after_genuine_capture() {
        if isolated_fixture("prior_original_evidence_tamper_is_rejected_after_genuine_capture") { return; }
        let (f, _) = prior_fixture(false, false).await;
        let mut proof: serde_json::Value =
            serde_json::from_slice(&std::fs::read(f.proof()).unwrap()).unwrap();
        proof["dev"] = serde_json::json!(proof["dev"].as_u64().unwrap() + 1);
        std::fs::write(f.proof(), serde_json::to_vec(&proof).unwrap()).unwrap();
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        assert!(plan(&b)
            .await
            .unwrap_err()
            .to_string()
            .contains("original evidence differs"));
        b.db.close().await;
    }
    #[tokio::test]
    async fn native_move_after_prior_can_share_captured_group_with_old_original() {
        if isolated_fixture("native_move_after_prior_can_share_captured_group_with_old_original") { return; }
        let (f, _) = prior_fixture(false, false).await;
        let db = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&format!(
                "sqlite://{}?mode=rw",
                f.source.join("starlane-notes.db").display()
            ))
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO spaces(id,name,slug,created_by) VALUES('dest','Destination','dest','u')",
        )
        .execute(&db)
        .await
        .unwrap();
        let destination = format!("spaces/dest/docs/revisions/{}.md", uuid::Uuid::new_v4());
        let m = super::super::relocation::prepare(
            &f.source,
            &uuid::Uuid::new_v4().to_string(),
            &"a".repeat(64),
            &"b".repeat(64),
            &[super::super::relocation::FilePlan {
                subject_id: "d".into(),
                kind: "canonical".into(),
                source: f.manifest.path.clone(),
                destination: destination.clone(),
                sha256: None,
                maximum: 6_000_000,
            }],
        )
        .unwrap();
        let expected = super::super::relocation::escrow_binding(&[]).unwrap();
        let receipt = serde_json::json!({"op_id":m.op_id,"preview_id":uuid::Uuid::new_v4().to_string(),"actor_id":"u","source_space_id":"s","destination_space_id":"dest","request_fingerprint":m.request_fingerprint,"source_fingerprint":m.source_fingerprint,"manifest_json":serde_json::to_string(&m).unwrap(),"expected_state_json":expected.to_string(),"created_at":"2026-10-10T00:00:00Z"});
        let mut tx = db.begin().await.unwrap();
        super::super::relocation::seal_receipt(&f.source, &receipt).unwrap();
        sqlx::query("UPDATE documents SET space_id='dest',content_path=$1 WHERE id='d'")
            .bind(&destination)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO move_commit_receipts(op_id,preview_id,actor_id,source_space_id,destination_space_id,request_fingerprint,source_fingerprint,manifest_json,expected_state_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(receipt["op_id"].as_str().unwrap()).bind(receipt["preview_id"].as_str().unwrap()).bind("u").bind("s").bind("dest").bind(receipt["request_fingerprint"].as_str().unwrap()).bind(receipt["source_fingerprint"].as_str().unwrap()).bind(receipt["manifest_json"].as_str().unwrap()).bind(receipt["expected_state_json"].as_str().unwrap()).bind(receipt["created_at"].as_str().unwrap()).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        db.close().await;
        let bytes = std::fs::read(f.source.join(&f.manifest.path)).unwrap();
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&b).await.unwrap();
        let s = p
            .scopes
            .iter()
            .find(|s| s.namespace == ".move-relocation-journal")
            .unwrap();
        assert_eq!(s.class, OperationClass::Committed);
        assert_eq!(s.bindings.len(), 2);
        assert!(s.bindings.iter().any(|b| b.relative == destination));
        let native = s
            .bindings
            .iter()
            .find(|p| p.relative == f.manifest.path)
            .unwrap();
        assert_eq!(native.resolution, IdentityResolution::NativeAfterPrior);
        let old = p
            .scopes
            .iter()
            .find(|s| s.namespace == ".storage-journal")
            .unwrap();
        assert!(old
            .bindings
            .iter()
            .any(|b| b.captured_inode == native.captured_inode
                && b.original_inode != native.original_inode));
        assert!(p.unresolved.is_empty(), "{:?}", p.unresolved);
        assert_eq!(
            s.evidence.as_ref(),
            Some(
                &std::fs::read(f.source.join(format!(
                    ".move-relocation-journal/{}/manifest.json",
                    m.op_id
                )))
                .unwrap()
            )
        );
        assert!(s.receipt_seal.is_some());
        assert!(s.logical_receipt.is_some());
        assert_eq!(
            std::fs::read(b.data_path().join(&f.manifest.path)).unwrap(),
            bytes
        );
        b.db.close().await;
    }
    #[tokio::test]
    async fn prior_zero_binding_scope_denies_even_numeric_native_identity() {
        if isolated_fixture("prior_zero_binding_scope_denies_even_numeric_native_identity") { return; }
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let mut f = Fixture::new(true, false).await;
        let proof = std::fs::read(f.proof()).unwrap();
        f.capture();
        let first = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let md = std::fs::metadata(&f.source).unwrap();
        let ledger = super::super::restoration::Ledger { historical_preservation:None, composite:None,completed_absence:None,
            admission:None, version: 1,
            generation_id: uuid::Uuid::new_v4().to_string(),
            backup_id: first.manifest.backup_id.clone(),
            manifest_sha256: hash(&first.raw_manifest),
            database_sha256: first.manifest.files.0["starlane-notes.db"].sha256.clone(),
            root_device: md.dev(),
            root_inode: md.ino(),
            scopes: vec![super::super::restoration::Scope {
                namespace: ".storage-journal".into(),
                op_id: f.manifest.op_id.clone(),
                evidence_sha256: hash(&proof),
                bindings: Vec::new(),
            }],
        };
        first.db.close().await;
        std::fs::set_permissions(&f.source, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(
            f.source.join(".starlane-restore-incomplete"),
            b"private fixture",
        )
        .unwrap();
        let db = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&format!(
                "sqlite://{}?mode=rw",
                f.source.join("starlane-notes.db").display()
            ))
            .await
            .unwrap();
        super::super::restoration::prepare_in(&f.source, &db, &ledger)
            .await
            .unwrap();
        std::fs::remove_file(f.source.join(".starlane-restore-incomplete")).unwrap();
        super::super::restoration::activate_in(&f.source, &db)
            .await
            .unwrap();
        db.close().await;
        f.bundle = f.bundle.parent().unwrap().join("native-id-repeat");
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let original: serde_json::Value = serde_json::from_slice(&proof).unwrap();
        let anchor = b
            .manifest
            .source_custody
            .files
            .0
            .get(&format!(
                ".storage-journal/{}/install-anchor",
                f.manifest.op_id
            ))
            .unwrap();
        assert_eq!(
            (anchor.device, anchor.inode),
            (
                original["dev"].as_u64().unwrap(),
                original["ino"].as_u64().unwrap()
            )
        );
        assert!(plan(&b)
            .await
            .unwrap_err()
            .to_string()
            .contains("required binding missing"));
        assert_eq!(std::fs::read(f.proof()).unwrap(), proof);
        b.db.close().await;
    }
    #[tokio::test]
    async fn multiple_prior_generations_are_explicitly_refused_not_dropped() {
        if isolated_fixture("multiple_prior_generations_are_explicitly_refused_not_dropped") { return; }
        let (f, _) = prior_fixture(false, false).await;
        let db = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&format!(
                "sqlite://{}?mode=rw",
                f.source.join("starlane-notes.db").display()
            ))
            .await
            .unwrap();
        sqlx::query("INSERT INTO storage_restore_generations(generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state,created_at) SELECT ?,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state,created_at FROM storage_restore_generations").bind(uuid::Uuid::new_v4().to_string()).execute(&db).await.unwrap();
        db.close().await;
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        assert!(plan(&b)
            .await
            .unwrap_err()
            .to_string()
            .contains("Prior archived lineage requires resolution"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM storage_restore_generations")
                .fetch_one(&b.db)
                .await
                .unwrap(),
            2
        );
        b.db.close().await;
    }
    async fn add_native_move(f: &Fixture) -> String {
        let db = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&format!(
                "sqlite://{}?mode=rw",
                f.source.join("starlane-notes.db").display()
            ))
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO spaces(id,name,slug,created_by) VALUES('dest','Destination','dest','u')",
        )
        .execute(&db)
        .await
        .unwrap();
        let destination = format!("spaces/dest/docs/revisions/{}.md", uuid::Uuid::new_v4());
        let m = super::super::relocation::prepare(
            &f.source,
            &uuid::Uuid::new_v4().to_string(),
            &"a".repeat(64),
            &"b".repeat(64),
            &[super::super::relocation::FilePlan {
                subject_id: "d".into(),
                kind: "canonical".into(),
                source: f.manifest.path.clone(),
                destination: destination.clone(),
                sha256: None,
                maximum: 6_000_000,
            }],
        )
        .unwrap();
        let expected = super::super::relocation::escrow_binding(&[]).unwrap();
        let receipt = serde_json::json!({"op_id":m.op_id,"preview_id":uuid::Uuid::new_v4().to_string(),"actor_id":"u","source_space_id":"s","destination_space_id":"dest","request_fingerprint":m.request_fingerprint,"source_fingerprint":m.source_fingerprint,"manifest_json":serde_json::to_string(&m).unwrap(),"expected_state_json":expected.to_string(),"created_at":"2026-10-10T00:00:00Z"});
        let mut tx = db.begin().await.unwrap();
        super::super::relocation::seal_receipt(&f.source, &receipt).unwrap();
        sqlx::query("UPDATE documents SET space_id='dest',content_path=$1 WHERE id='d'")
            .bind(&destination)
            .execute(&mut *tx)
            .await
            .unwrap();
        sqlx::query("INSERT INTO move_commit_receipts(op_id,preview_id,actor_id,source_space_id,destination_space_id,request_fingerprint,source_fingerprint,manifest_json,expected_state_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(receipt["op_id"].as_str().unwrap()).bind(receipt["preview_id"].as_str().unwrap()).bind("u").bind("s").bind("dest").bind(receipt["request_fingerprint"].as_str().unwrap()).bind(receipt["source_fingerprint"].as_str().unwrap()).bind(receipt["manifest_json"].as_str().unwrap()).bind(receipt["expected_state_json"].as_str().unwrap()).bind(receipt["created_at"].as_str().unwrap()).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        db.close().await;
        destination
    }

    fn file_id(path: &std::path::Path) -> (u64, u64, u64) {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::metadata(path).unwrap();
        (m.dev(), m.ino(), m.nlink())
    }
    #[tokio::test]
    async fn materialization_completed_canonical_and_equal_bytes_remain_independent() {
        if isolated_fixture("materialization_completed_canonical_and_equal_bytes_remain_independent") { return; }
        let f = Fixture::new(true, false).await;
        let original = std::fs::read(f.source.join(&f.manifest.path)).unwrap();
        std::fs::create_dir(f.source.join("uploads")).unwrap();
        std::fs::write(f.source.join("uploads/unrelated.bin"), &original).unwrap();
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&b).await.unwrap();
        assert!(!p
            .scopes()
            .iter()
            .flat_map(|s| &s.bindings)
            .any(|b| b.relative == f.manifest.path));
        let binding = p.scopes()[0].bindings[0].relative.clone();
        let proof = std::fs::read(f.proof()).unwrap();
        let target = f.source.parent().unwrap().join("materialized");
        let copy = super::super::restoration::reserve_and_copy(&b, &target).unwrap();
        copy.fence().unwrap();
        let done = super::super::restoration::materialize_groups(copy, &b, p).unwrap();
        done.fence().unwrap();
        assert_eq!(done.observed(&binding).unwrap().2, 2);
        assert_eq!(done.observed(&f.manifest.path).unwrap().2, 1);
        assert_ne!(
            done.observed(&binding).unwrap().1,
            done.observed(&f.manifest.path).unwrap().1
        );
        assert_ne!(
            done.observed("uploads/unrelated.bin").unwrap().1,
            done.observed(&f.manifest.path).unwrap().1
        );
        assert_eq!(
            std::fs::read(target.join(format!(
                ".storage-journal/{}/install-proof.json",
                f.manifest.op_id
            )))
            .unwrap(),
            proof
        );
        std::fs::write(
            target.join(&f.manifest.path),
            b"independently changed canonical",
        )
        .unwrap();
        assert_eq!(std::fs::read(target.join(&binding)).unwrap(), original);
        assert!(done.fence().is_err());
        assert!(target.join(".starlane-restore-incomplete").is_file());
        assert!(super::super::restoration::bootstrap_gate(&target).is_err());
        b.db.close().await;
    }
    #[tokio::test]
    async fn materialization_prior_and_native_scope_share_only_certified_capture_group() {
        if isolated_fixture("materialization_prior_and_native_scope_share_only_certified_capture_group") { return; }
        let (f, proof) = prior_fixture(false, false).await;
        let destination = add_native_move(&f).await;
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&b).await.unwrap();
        let oldledger = p.raw_prior_ledger().unwrap().to_vec();
        let initialdb = std::fs::read(b.data_path().join("starlane-notes.db")).unwrap();
        let old = p
            .scopes()
            .iter()
            .find(|s| s.namespace == ".storage-journal")
            .unwrap();
        let oldpath = old.bindings[0].relative.clone();
        assert!(old
            .bindings
            .iter()
            .all(|b| b.original_inode != b.captured_inode));
        let target = f.source.parent().unwrap().join("native-materialized");
        let copy = super::super::restoration::reserve_and_copy(&b, &target).unwrap();
        let done = super::super::restoration::materialize_groups(copy, &b, p).unwrap();
        done.fence().unwrap();
        let oldid = done.observed(&oldpath).unwrap();
        let native = done.observed(&f.manifest.path).unwrap();
        assert_eq!(oldid, native);
        assert_eq!(native.2, 3);
        assert_ne!(done.observed(&destination).unwrap().1, native.1);
        assert_eq!(
            std::fs::read(target.join(".restoration-custody/current.json")).unwrap(),
            oldledger
        );
        assert_eq!(
            std::fs::read(target.join("starlane-notes.db")).unwrap(),
            initialdb
        );
        assert_eq!(
            std::fs::read(target.join(format!(
                ".storage-journal/{}/install-proof.json",
                f.manifest.op_id
            )))
            .unwrap(),
            proof
        );
        assert!(super::super::restoration::bootstrap_gate(&target).is_err());
        b.db.close().await;
    }
    #[tokio::test]
    async fn materialization_genuine_other_bundle_plan_is_rejected_before_mutation() {
        if isolated_fixture("materialization_genuine_other_bundle_plan_is_rejected_before_mutation") { return; }
        let f = Fixture::new(true, false).await;
        f.capture();
        let first = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&first).await.unwrap();
        let second = Fixture::new(true, false).await;
        second.capture();
        let b = super::super::backup_inventory::verify(&second.bundle)
            .await
            .unwrap();
        let target = second.source.parent().unwrap().join("wrong-plan");
        let copy = super::super::restoration::reserve_and_copy(&b, &target).unwrap();
        let rawdb = std::fs::read(target.join("starlane-notes.db")).unwrap();
        let anchor = target.join(format!(
            ".storage-journal/{}/install-anchor",
            second.manifest.op_id
        ));
        let before = file_id(&anchor);
        let result = super::super::restoration::materialize_groups(copy, &b, p);
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("Plan and copied source differ"));
        assert_eq!(file_id(&anchor), before);
        assert_eq!(
            std::fs::read(target.join("starlane-notes.db")).unwrap(),
            rawdb
        );
        assert!(target.join(".starlane-restore-incomplete").is_file());
        first.db.close().await;
        b.db.close().await;
    }
    #[tokio::test]
    async fn materialization_unapproved_alias_before_start_is_rejected() {
        if isolated_fixture("materialization_unapproved_alias_before_start_is_rejected") { return; }
        let f = Fixture::new(true, false).await;
        let body = std::fs::read(f.source.join(&f.manifest.path)).unwrap();
        std::fs::create_dir(f.source.join("uploads")).unwrap();
        std::fs::write(f.source.join("uploads/unrelated.bin"), body).unwrap();
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&b).await.unwrap();
        let target = f.source.parent().unwrap().join("unapproved");
        let copy = super::super::restoration::reserve_and_copy(&b, &target).unwrap();
        std::fs::remove_file(target.join("uploads/unrelated.bin")).unwrap();
        std::fs::hard_link(
            target.join(&f.manifest.path),
            target.join("uploads/unrelated.bin"),
        )
        .unwrap();
        assert!(super::super::restoration::materialize_groups(copy, &b, p).is_err());
        assert!(target.join(".starlane-restore-incomplete").is_file());
        b.db.close().await;
    }
    #[tokio::test]
    async fn materialization_interruption_retains_marker_raw_proof_and_database() {
        if isolated_fixture("materialization_interruption_retains_marker_raw_proof_and_database") { return; }
        for phase in [
            "after-link-before-exchange",
            "after-exchange-before-cleanup",
        ] {
            let f = Fixture::new(true, false).await;
            f.capture();
            let b = super::super::backup_inventory::verify(&f.bundle)
                .await
                .unwrap();
            let p = plan(&b).await.unwrap();
            let target = f.source.parent().unwrap().join("interrupted");
            let copy = super::super::restoration::reserve_and_copy(&b, &target).unwrap();
            let proofpath = format!(".storage-journal/{}/install-proof.json", f.manifest.op_id);
            let proof = std::fs::read(target.join(&proofpath)).unwrap();
            let db = std::fs::read(target.join("starlane-notes.db")).unwrap();
            let result =
                super::super::restoration::materialize_groups_test(copy, &b, p, |at, _| {
                    if at == phase {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::Interrupted,
                            "genuine injected interruption",
                        ))
                    } else {
                        Ok(())
                    }
                });
            assert!(result.is_err());
            assert!(target.join(".starlane-restore-incomplete").is_file());
            assert_eq!(std::fs::read(target.join(&proofpath)).unwrap(), proof);
            assert_eq!(std::fs::read(target.join("starlane-notes.db")).unwrap(), db);
            assert!(super::super::restoration::bootstrap_gate(&target).is_err());
            b.db.close().await;
        }
    }
    #[tokio::test]
    async fn materialization_exchange_equal_byte_leaf_replacement_race_is_rejected() {
        if isolated_fixture("materialization_exchange_equal_byte_leaf_replacement_race_is_rejected") { return; }
        let f = Fixture::new(true, false).await;
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&b).await.unwrap();
        let target = f.source.parent().unwrap().join("raced");
        let copy = super::super::restoration::reserve_and_copy(&b, &target).unwrap();
        let result = super::super::restoration::materialize_groups_test(copy, &b, p, |at, path| {
            if at == "after-link-before-exchange" {
                let leaf = target.join(path);
                let bytes = std::fs::read(&leaf)?;
                let permissions = std::fs::metadata(&leaf)?.permissions();
                std::fs::remove_file(&leaf)?;
                std::fs::write(&leaf, bytes)?;
                std::fs::set_permissions(&leaf, permissions)?;
            }
            Ok(())
        });
        assert!(result.err().unwrap().to_string().contains("raced"));
        assert!(target.join(".starlane-restore-incomplete").is_file());
        b.db.close().await;
    }
    #[tokio::test]
    async fn materialization_raw_proof_same_inode_change_then_exact_restore_is_rejected() {
        if isolated_fixture("materialization_raw_proof_same_inode_change_then_exact_restore_is_rejected") { return; }
        let f = Fixture::new(true, false).await;
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&b).await.unwrap();
        let target = f.source.parent().unwrap().join("proof-raced");
        let copy = super::super::restoration::reserve_and_copy(&b, &target).unwrap();
        let proof = target.join(format!(
            ".storage-journal/{}/install-proof.json",
            f.manifest.op_id
        ));
        let bytes = std::fs::read(&proof).unwrap();
        let result = super::super::restoration::materialize_groups_test(copy, &b, p, |at, _| {
            if at == "after-link-before-exchange" {
                std::fs::write(&proof, b"temporarily changed")?;
                std::fs::write(&proof, &bytes)?;
            }
            Ok(())
        });
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("Unapproved copied evidence changed"));
        assert_eq!(std::fs::read(proof).unwrap(), bytes);
        assert!(target.join(".starlane-restore-incomplete").is_file());
        b.db.close().await;
    }
    #[tokio::test]
    async fn materialization_final_fence_rejects_temporary_byte_change_and_marker_replacement() {
        if isolated_fixture("materialization_final_fence_rejects_temporary_byte_change_and_marker_replacement") { return; }
        for mode in ["bytes", "marker", "alias"] {
            let f = Fixture::new(true, false).await;
            f.capture();
            let b = super::super::backup_inventory::verify(&f.bundle)
                .await
                .unwrap();
            let p = plan(&b).await.unwrap();
            let target = f.source.parent().unwrap().join("fenced");
            let copy = super::super::restoration::reserve_and_copy(&b, &target).unwrap();
            let done = super::super::restoration::materialize_groups(copy, &b, p).unwrap();
            done.fence().unwrap();
            let path = if mode == "marker" {
                target.join(".starlane-restore-incomplete")
            } else {
                target.join(&f.manifest.path)
            };
            let bytes = std::fs::read(&path).unwrap();
            if mode == "marker" {
                let permission = std::fs::metadata(&path).unwrap().permissions();
                std::fs::remove_file(&path).unwrap();
                std::fs::write(&path, &bytes).unwrap();
                std::fs::set_permissions(&path, permission).unwrap();
            } else if mode == "bytes" {
                std::fs::write(&path, b"temporary mutation").unwrap();
                std::fs::write(&path, &bytes).unwrap();
            } else {
                std::fs::hard_link(&path, target.join("unapproved-link")).unwrap();
                std::fs::remove_file(target.join("unapproved-link")).unwrap();
            }
            assert!(done.fence().is_err());
            assert!(target.join(".starlane-restore-incomplete").is_file());
            b.db.close().await;
        }
    }
    #[tokio::test]
    async fn materialization_postgroup_same_inode_change_then_restore_is_rejected() {
        if isolated_fixture("materialization_postgroup_same_inode_change_then_restore_is_rejected") { return; }
        for mode in ["bytes", "alias"] {
            let f = Fixture::new(true, false).await;
            f.capture();
            let b = super::super::backup_inventory::verify(&f.bundle)
                .await
                .unwrap();
            let p = plan(&b).await.unwrap();
            let target = f.source.parent().unwrap().join("assembly-race");
            let copy = super::super::restoration::reserve_and_copy(&b, &target).unwrap();
            let result =
                super::super::restoration::materialize_groups_test(copy, &b, p, |at, path| {
                    if at == "after-certified-group" {
                        let leaf = target.join(path);
                        if mode == "bytes" {
                            let bytes = std::fs::read(&leaf)?;
                            std::fs::write(&leaf, b"temporary authorized-node mutation")?;
                            std::fs::write(&leaf, bytes)?;
                        } else {
                            let extra = target.join("extra-alias");
                            std::fs::hard_link(&leaf, &extra)?;
                            std::fs::remove_file(extra)?;
                        }
                    }
                    Ok(())
                });
            assert!(result.is_err());
            assert!(target.join(".starlane-restore-incomplete").is_file());
            b.db.close().await;
        }
    }
    #[tokio::test]
    async fn materialization_postgroup_cancelled_write_open_consumes_certification() {
        if isolated_fixture("materialization_postgroup_cancelled_write_open_consumes_certification") { return; }
        use std::os::unix::fs::OpenOptionsExt;
        let fixture = Fixture::new(true, false).await;
        fixture.capture();
        let bundle = super::super::backup_inventory::verify(&fixture.bundle).await.unwrap();
        let custody_plan = plan(&bundle).await.unwrap();
        let target = fixture.source.parent().unwrap().join("cancelled-group-write-open");
        let copied = super::super::restoration::reserve_and_copy(&bundle, &target).unwrap();
        let mut attempted = false;
        let result = super::super::restoration::materialize_groups_test(
            copied, &bundle, custody_plan, |at, path| {
                if at == "after-certified-group" {
                    let leaf = target.join(path);
                    let before = std::fs::read(&leaf)?;
                    // A successful open after prompt broker release is also a
                    // consumed lease. Neither outcome writes a single byte.
                    let _cancelled = std::fs::OpenOptions::new().write(true)
                        .custom_flags(libc::O_NONBLOCK).open(&leaf);
                    assert_eq!(std::fs::read(&leaf)?, before);
                    attempted = true;
                }
                Ok(())
            },
        );
        assert!(attempted);
        assert!(result.is_err(), "An attempted writer cannot restore the same certification even when bytes are unchanged");
        assert!(target.join(".starlane-restore-incomplete").is_file());
        bundle.db.close().await;
    }
    #[tokio::test]
    async fn materialization_parent_moved_after_link_refuses_exchange_and_preserves_bytes() {
        if isolated_fixture("materialization_parent_moved_after_link_refuses_exchange_and_preserves_bytes") { return; }
        let f = Fixture::new(true, false).await;
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&b).await.unwrap();
        let target = f.source.parent().unwrap().join("parent-race");
        let copy = super::super::restoration::reserve_and_copy(&b, &target).unwrap();
        let moved = f.source.parent().unwrap().join("moved-private-operation");
        let proof = std::fs::read(f.proof()).unwrap();
        let db = std::fs::read(target.join("starlane-notes.db")).unwrap();
        let result = super::super::restoration::materialize_groups_test(copy, &b, p, |at, path| {
            if at == "after-link-before-exchange" {
                std::fs::rename(target.join(path).parent().unwrap(), &moved)?;
            }
            Ok(())
        });
        assert!(result.is_err());
        assert!(target.join(".starlane-restore-incomplete").is_file());
        assert_eq!(
            std::fs::read(moved.join("install-proof.json")).unwrap(),
            proof
        );
        assert_eq!(std::fs::read(target.join("starlane-notes.db")).unwrap(), db);
        b.fence().unwrap();
        b.db.close().await;
    }
    #[tokio::test]
    async fn materialization_late_marker_same_inode_change_then_exact_restore_is_rejected() {
        if isolated_fixture("materialization_late_marker_same_inode_change_then_exact_restore_is_rejected") { return; }
        let f = Fixture::new(true, false).await;
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&b).await.unwrap();
        let target = f.source.parent().unwrap().join("late-marker-race");
        let copy = super::super::restoration::reserve_and_copy(&b, &target).unwrap();
        let marker = target.join(".starlane-restore-incomplete");
        let bytes = std::fs::read(&marker).unwrap();
        let before = file_id(&marker);
        let result = super::super::restoration::materialize_groups_test(copy, &b, p, |phase, _| {
            if phase == "before-token-assembly" {
                std::fs::write(&marker, b"transient marker mutation")?;
                std::fs::write(&marker, &bytes)?;
            }
            Ok(())
        });
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("Planned root or marker changed"));
        assert_eq!(file_id(&marker), before);
        assert_eq!(std::fs::read(&marker).unwrap(), bytes);
        assert!(super::super::restoration::bootstrap_gate(&target).is_err());
        b.fence().unwrap();
        b.db.close().await;
    }

    async fn add_pending_operation(f: &Fixture, id: &str) -> super::super::journal::StagedDocument {
        std::env::set_var("DATA_DIR", &f.source);
        let db = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect(&format!(
                "sqlite://{}?mode=rw",
                f.source.join("starlane-notes.db").display()
            ))
            .await
            .unwrap();
        let mut tx = db.begin().await.unwrap();
        let draft = format!("draft-{id}");
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at) VALUES($1,'s','Pending','temporary','u','u','initial')").bind(id).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES($1,$2,'s','u','Pending','Private body','initial')").bind(&draft).bind(id).execute(&mut *tx).await.unwrap();
        let m = super::super::journal::stage_document(
            &mut tx,
            "s",
            id,
            &draft,
            "initial",
            "s",
            "Pending",
            "",
            "Private body",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE documents SET content_path=$1 WHERE id=$2")
            .bind(&m.path)
            .bind(id)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        db.close().await;
        m
    }
    async fn mint_prepared(
        f: &Fixture,
    ) -> (
        super::super::backup_inventory::ValidatedBundle,
        super::super::restoration::PlannedDestination,
        PathBuf,
    ) {
        f.capture();
        let b = super::super::backup_inventory::verify(&f.bundle)
            .await
            .unwrap();
        let p = plan(&b).await.unwrap();
        let target = f.source.parent().unwrap().join("mint-target");
        let c = super::super::restoration::reserve_and_copy(&b, &target).unwrap();
        let d = super::super::restoration::materialize_groups(c, &b, p).unwrap();
        (b, d, target)
    }
    #[tokio::test]
    async fn pending_mint_mixed_completed_and_pending_preserves_old_raw_evidence() {
        if isolated_fixture("pending_mint_mixed_completed_and_pending_preserves_old_raw_evidence") { return; }
        let f = Fixture::new(true, false).await;
        let m = add_pending_operation(&f, "pending-mixed").await;
        let (b, d, target) = mint_prepared(&f).await;
        let old_proof = std::fs::read(f.proof()).unwrap();
        let db = std::fs::read(target.join("starlane-notes.db")).unwrap();
        let original_manifest =
            std::fs::read(target.join(format!(".storage-journal/{}/manifest.json", m.op_id)))
                .unwrap();
        let old_ids = d
            .plan()
            .groups()
            .iter()
            .flat_map(|g| g.binding_members.iter())
            .map(|p| (p.clone(), file_id(&target.join(p))))
            .collect::<Vec<_>>();
        let unresolved = d.plan().unresolved().len();
        let d = super::super::restoration::mint_pending_preproof(&b, d, &m.op_id)
            .await
            .unwrap();
        assert_eq!(d.minted_count(), 1);
        assert_eq!(d.plan().unresolved().len(), unresolved);
        assert_eq!(std::fs::read(target.join("starlane-notes.db")).unwrap(), db);
        assert_eq!(
            std::fs::read(target.join(format!(".storage-journal/{}/manifest.json", m.op_id)))
                .unwrap(),
            original_manifest
        );
        assert_eq!(
            std::fs::read(target.join(format!(
                ".storage-journal/{}/install-proof.json",
                f.manifest.op_id
            )))
            .unwrap(),
            old_proof
        );
        for (p, id) in old_ids {
            assert_eq!(file_id(&target.join(p)), id);
        }
        let canonical = file_id(&target.join(&m.path));
        let inspected = super::super::journal_fs::inspect_install(
            &target,
            ".storage-journal",
            &super::super::journal_fs::FileDescriptor::from(&m),
            true,
        )
        .unwrap();
        assert!(inspected.installed_present);
        assert_eq!(inspected.required_group.len(), 3);
        assert_eq!(
            inspected.proof.unwrap().original,
            (canonical.0, canonical.1)
        );
        assert_eq!(canonical.2, 3);
        assert_eq!(
            file_id(&target.join(format!(".storage-journal/{}/install-anchor", m.op_id))),
            canonical
        );
        assert_eq!(
            file_id(&target.join(format!(".storage-journal/{}/install-proof.json", m.op_id))).2,
            2
        );
        assert_eq!(
            file_id(&target.join(format!(".storage-journal/{}/payload", m.op_id))).2,
            1
        );
        assert_ne!(canonical.1, file_id(&target.join(&f.manifest.path)).1);
        d.fence().unwrap();
        b.fence().unwrap();
        assert!(target.join(".starlane-restore-incomplete").exists());
        assert!(super::super::restoration::bootstrap_gate(&target).is_err());
    }
    #[tokio::test]
    async fn pending_mint_two_sequential_operations_keep_distinct_equal_byte_groups() {
        if isolated_fixture("pending_mint_two_sequential_operations_keep_distinct_equal_byte_groups") { return; }
        let f = Fixture::new(false, false).await;
        let second = add_pending_operation(&f, "pending-second").await;
        let (b, d, target) = mint_prepared(&f).await;
        let d = super::super::restoration::mint_pending_preproof(&b, d, &f.manifest.op_id)
            .await
            .unwrap();
        let first = file_id(&target.join(&f.manifest.path));
        let proof_path = target.join(format!(
            ".storage-journal/{}/install-proof.json",
            f.manifest.op_id
        ));
        let proof = std::fs::read(&proof_path).unwrap();
        let proof_id = file_id(&proof_path);
        let d = super::super::restoration::mint_pending_preproof(&b, d, &second.op_id)
            .await
            .unwrap();
        assert_eq!(d.minted_count(), 2);
        assert_eq!(file_id(&target.join(&f.manifest.path)), first);
        assert_eq!(file_id(&proof_path), proof_id);
        assert_eq!(std::fs::read(&proof_path).unwrap(), proof);
        let other = file_id(&target.join(&second.path));
        assert_eq!(other.2, 3);
        assert_ne!((other.0, other.1), (first.0, first.1));
        assert_eq!(
            std::fs::read(target.join(&second.path)).unwrap(),
            std::fs::read(target.join(&f.manifest.path)).unwrap()
        );
        d.fence().unwrap();
    }
    #[tokio::test]
    async fn pending_mint_duplicate_operation_refuses_before_new_artifacts() {
        if isolated_fixture("pending_mint_duplicate_operation_refuses_before_new_artifacts") { return; }
        let f = Fixture::new(false, false).await;
        let (b, d, target) = mint_prepared(&f).await;
        let d = super::super::restoration::mint_pending_preproof(&b, d, &f.manifest.op_id)
            .await
            .unwrap();
        let proof_path = target.join(format!(
            ".storage-journal/{}/install-proof.json",
            f.manifest.op_id
        ));
        let proof = std::fs::read(&proof_path).unwrap();
        let count = std::fs::read_dir(target.join(".restoration-custody"))
            .unwrap()
            .count();
        assert!(
            super::super::restoration::mint_pending_preproof(&b, d, &f.manifest.op_id)
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(&proof_path).unwrap(), proof);
        assert_eq!(
            std::fs::read_dir(target.join(".restoration-custody"))
                .unwrap()
                .count(),
            count
        );
        assert!(target.join(".starlane-restore-incomplete").exists());
    }
    #[tokio::test]
    async fn pending_mint_between_operations_alias_and_write_restore_are_not_rebased() {
        if isolated_fixture("pending_mint_between_operations_alias_and_write_restore_are_not_rebased") { return; }
        for attack in [
            "alias",
            "canonical",
            "database",
            "marker",
            "original-manifest",
        ] {
            let f = Fixture::new(true, false).await;
            let first = add_pending_operation(&f, "pending-first").await;
            let second = add_pending_operation(&f, "pending-second").await;
            let (b, d, target) = mint_prepared(&f).await;
            let d = super::super::restoration::mint_pending_preproof(&b, d, &first.op_id)
                .await
                .unwrap();
            let file = match attack {
                "alias" => target.join(format!(
                    ".storage-journal/{}/install-anchor",
                    f.manifest.op_id
                )),
                "canonical" => target.join(&first.path),
                "database" => target.join("starlane-notes.db"),
                "marker" => target.join(".starlane-restore-incomplete"),
                _ => target.join(format!(".storage-journal/{}/manifest.json", second.op_id)),
            };
            let id = file_id(&file);
            let raw = std::fs::read(&file).unwrap();
            if attack == "alias" {
                let alias = target.parent().unwrap().join("escaped-link");
                std::fs::hard_link(&file, &alias).unwrap();
                std::fs::remove_file(alias).unwrap();
            } else {
                std::fs::write(&file, b"changed").unwrap();
                std::fs::write(&file, &raw).unwrap();
            }
            assert_eq!(file_id(&file), id);
            assert_eq!(std::fs::read(&file).unwrap(), raw);
            assert!(
                super::super::restoration::mint_pending_preproof(&b, d, &second.op_id)
                    .await
                    .is_err(),
                "{attack}"
            );
            assert!(!target.join(&second.path).exists());
            assert!(target.join(".starlane-restore-incomplete").exists());
            b.fence().unwrap();
        }
    }
    #[tokio::test]
    async fn pending_mint_interruption_keeps_first_mint_and_all_original_inputs() {
        if isolated_fixture("pending_mint_interruption_keeps_first_mint_and_all_original_inputs") { return; }
        for stop in ["intent", "anchor", "proof", "canonical"] {
            let f = Fixture::new(true, false).await;
            let first = add_pending_operation(&f, "pending-first").await;
            let second = add_pending_operation(&f, "pending-second").await;
            let (b, d, target) = mint_prepared(&f).await;
            let d = super::super::restoration::mint_pending_preproof(&b, d, &first.op_id)
                .await
                .unwrap();
            let db = std::fs::read(target.join("starlane-notes.db")).unwrap();
            let old_proof = std::fs::read(f.proof()).unwrap();
            let firstproof = target.join(format!(
                ".storage-journal/{}/install-proof.json",
                first.op_id
            ));
            let firstbytes = std::fs::read(&firstproof).unwrap();
            let err = super::super::restoration::mint_pending_preproof_test(
                &b,
                d,
                &second.op_id,
                |stage| {
                    if stage == stop {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::Interrupted,
                            "fixture interruption",
                        ))
                    } else {
                        Ok(())
                    }
                },
            )
            .await
            .err()
            .unwrap();
            assert_eq!(err.kind(), std::io::ErrorKind::Interrupted);
            assert_eq!(std::fs::read(target.join("starlane-notes.db")).unwrap(), db);
            assert_eq!(
                std::fs::read(target.join(format!(
                    ".storage-journal/{}/install-proof.json",
                    f.manifest.op_id
                )))
                .unwrap(),
                old_proof
            );
            assert_eq!(std::fs::read(firstproof).unwrap(), firstbytes);
            assert_eq!(
                std::fs::read_dir(target.join(".restoration-custody"))
                    .unwrap()
                    .count(),
                2
            );
            assert!(target.join(".starlane-restore-incomplete").exists());
            assert!(super::super::restoration::bootstrap_gate(&target).is_err());
            b.fence().unwrap();
        }
    }
    #[tokio::test]
    async fn pending_mint_mid_transition_marker_write_restore_fails_closed() {
        if isolated_fixture("pending_mint_mid_transition_marker_write_restore_fails_closed") { return; }
        let f = Fixture::new(false, false).await;
        let (b, d, target) = mint_prepared(&f).await;
        let marker = target.join(".starlane-restore-incomplete");
        let raw = std::fs::read(&marker).unwrap();
        let id = file_id(&marker);
        let mut fired = false;
        assert!(super::super::restoration::mint_pending_preproof_test(
            &b,
            d,
            &f.manifest.op_id,
            |phase| {
                if phase == "anchor" {
                    fired = true;
                    std::fs::write(&marker, b"changed")?;
                    std::fs::write(&marker, &raw)?;
                }
                Ok(())
            }
        )
        .await
        .is_err());
        assert!(fired, "Actual anchor boundary was not reached");
        assert_eq!(file_id(&marker), id);
        assert_eq!(std::fs::read(&marker).unwrap(), raw);
        assert!(!target.join(&f.manifest.path).exists());
        b.fence().unwrap();
    }
    #[tokio::test]
    async fn pending_mint_genuine_foreign_bundle_cannot_authorize_destination() {
        if isolated_fixture("pending_mint_genuine_foreign_bundle_cannot_authorize_destination") { return; }
        let first = Fixture::new(false, false).await;
        let other = Fixture::new(false, false).await;
        let (b, d, target) = mint_prepared(&first).await;
        other.capture();
        let foreign = super::super::backup_inventory::verify(&other.bundle)
            .await
            .unwrap();
        assert!(super::super::restoration::mint_pending_preproof(
            &foreign,
            d,
            &first.manifest.op_id
        )
        .await
        .is_err());
        assert!(!target.join(".restoration-custody").exists());
        assert!(!target.join(&first.manifest.path).exists());
        b.fence().unwrap();
        foreign.fence().unwrap();
    }
    #[tokio::test]
    async fn pending_mint_prior_native_sequential_preserves_raw_lineage_and_refuses_rewrite() {
        if isolated_fixture("pending_mint_prior_native_sequential_preserves_raw_lineage_and_refuses_rewrite") { return; }
        for tamper in [false, true] {
            let (f, oldproof) = prior_fixture(false, false).await;
            let first = add_pending_operation(&f, "prior-pending-first").await;
            let second = add_pending_operation(&f, "prior-pending-second").await;
            let oldledger =
                std::fs::read(f.source.join(".restoration-custody/current.json")).unwrap();
            let (b, d, target) = mint_prepared(&f).await;
            let rows = serde_json::to_vec(d.plan().prior_generation_rows()).unwrap();
            let pointers = serde_json::to_vec(d.plan().prior_current_rows()).unwrap();
            let db = std::fs::read(target.join("starlane-notes.db")).unwrap();
            let d = super::super::restoration::mint_pending_preproof(&b, d, &first.op_id)
                .await
                .unwrap();
            assert_eq!(
                std::fs::read(target.join(".restoration-custody/current.json")).unwrap(),
                oldledger
            );
            assert_eq!(
                std::fs::read(target.join(format!(
                    ".storage-journal/{}/install-proof.json",
                    f.manifest.op_id
                )))
                .unwrap(),
                oldproof
            );
            if tamper {
                let file = target.join(".restoration-custody/current.json");
                let id = file_id(&file);
                std::fs::write(&file, b"changed").unwrap();
                std::fs::write(&file, &oldledger).unwrap();
                assert_eq!(file_id(&file), id);
                assert!(
                    super::super::restoration::mint_pending_preproof(&b, d, &second.op_id)
                        .await
                        .is_err()
                );
                assert!(!target.join(&second.path).exists());
            } else {
                let d = super::super::restoration::mint_pending_preproof(&b, d, &second.op_id)
                    .await
                    .unwrap();
                assert_eq!(d.minted_count(), 2);
                assert_eq!(
                    serde_json::to_vec(d.plan().prior_generation_rows()).unwrap(),
                    rows
                );
                assert_eq!(
                    serde_json::to_vec(d.plan().prior_current_rows()).unwrap(),
                    pointers
                );
                d.fence().unwrap();
            }
            assert_eq!(std::fs::read(target.join("starlane-notes.db")).unwrap(), db);
            assert_eq!(
                std::fs::read(target.join(".restoration-custody/current.json")).unwrap(),
                oldledger
            );
            assert!(target.join(".starlane-restore-incomplete").exists());
            b.fence().unwrap();
        }
    }

    include!("restoration_publication_tests.inc");

}
