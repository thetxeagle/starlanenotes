//! Operator-owned offline SQLite restoration into a new private destination.
//! This command never starts services or changes the process DATA_DIR.
use anyhow::Context;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Serialize)]
pub(crate) struct Report {
    backup_id: String,
    generation_id: String,
    destination: PathBuf,
    retained_sql_artifacts: PathBuf,
    fresh_startup_required: bool,
    application_validation_required: bool,
}

#[cfg(target_os = "linux")]
pub(crate) async fn run(bundle_path: &Path, target: &Path, expected_version: &str) -> anyhow::Result<Report> {
    let bundle = super::backup_inventory::verify(bundle_path).await.context("Verify restore bundle")?;
    anyhow::ensure!(bundle.application_version() == expected_version, "Application version differs from the backup; use its recorded version and compatible pinned build");
    let ready = prepare_ready(&bundle, target, false).await?;
    bundle.fence()?;
    let report = Report {
        backup_id: bundle.manifest.backup_id.clone(),
        generation_id: ready.generation_id().to_owned(),
        destination: ready.path().to_owned(),
        retained_sql_artifacts: ready.retained_sql_artifacts().to_owned(),
        fresh_startup_required: true,
        application_validation_required: true,
    };
    bundle.db.close().await;
    Ok(report)
}

#[cfg(not(target_os = "linux"))]
pub(crate) async fn run(_bundle_path: &Path, _target: &Path, _expected_version: &str) -> anyhow::Result<Report> {
    anyhow::bail!("Application-owned portable restoration currently requires Linux")
}

#[cfg(target_os = "linux")]
pub(crate) async fn prepare_ready(bundle: &super::backup_inventory::ValidatedBundle, target: &Path, engine_transfer: bool) -> anyhow::Result<super::restoration::linux::BootReadyDestination> {
    use super::restoration_plan::OperationClass;
    let plan = super::restoration_plan::plan(&bundle).await.context("Classify original restore evidence")?;
    let mut pending = std::collections::BTreeSet::new();
    // These findings may be retained inert or handled by the explicit pending
    // protocol. All other unresolved classifications refuse before copying.
    for issue in plan.unresolved() {
        match issue.reason.as_str() {
            "orphan_stage_preserved_without_authority" | "prior_scope_preserved_without_fresh_classification" => {},
            "pending_without_original_install_proof_requires_explicit_protocol" => {
                let op = issue.operation_id.as_ref().context("Pending operation identity missing")?;
                anyhow::ensure!(issue.namespace == ".storage-journal" && plan.scopes().iter().any(|scope|
                    scope.namespace == issue.namespace && scope.operation_id == *op &&
                    scope.class == OperationClass::CommittedPending && scope.bindings.is_empty()),
                    "Pending operation requires unavailable recovery protocol");
                pending.insert(op.clone());
            },
            _ => anyhow::bail!("Original operation evidence requires review before restoration"),
        }
    }
    let copied = super::restoration::reserve_and_copy(&bundle, target).context("Reserve marked restore destination")?;
    let mut destination = super::restoration::materialize_groups(copied, &bundle, plan).context("Reconstruct certified destination groups")?;
    for op in pending {
        destination = super::restoration::mint_pending_preproof(&bundle, destination, &op).await.context("Prepare exact pending operation")?;
    }
    if engine_transfer {super::restoration::mark_engine_request(&mut destination)?;}
    // Declared capture transport is consumed only through its strict typed cold constructor.
    // Absence preserves the original Ledger1/no-admission behavior.
    let source=if bundle.manifest.admission_owner_capture.is_some()||bundle.manifest.prior_admission_owner_capture.is_some(){
        bundle.fence()?;
        let path=bundle.data_path();let root=path.parent().context("Bundle root absent")?;
        let observed=std::sync::Arc::new(super::backup_inventory::verify(root).await?);
        anyhow::ensure!(observed.raw_manifest==bundle.raw_manifest,"Cold capture manifest changed across observation");bundle.fence()?;observed.fence()?;
        let source=if let Some(e)=&observed.manifest.prior_admission_owner_capture {
            super::restoration::admission::ClassifiedAdmissionSource::cold_prior_bundle(observed.clone(),e).await?
        }else{super::restoration::admission::ClassifiedAdmissionSource::cold_bundle(observed.clone()).await?};
        bundle.fence()?;Some(source)
    }else{None};
    let sealed = super::restoration::linux::seal_publication_admission(&bundle, destination,source).await.context("Seal restore publication intent")?;
    let committed = super::restoration::commit_publication(sealed).await.context("Commit restore generation")?;
    super::restoration::retire_publication(committed).context("Finalize restore generation")

}
