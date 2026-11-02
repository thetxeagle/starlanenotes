//! Upload owner evidence adapter; capture identity resolution is still mandatory.
use super::{bound, hash, AssociationEvidence, CustodyPlan, OperationClass, PlannedScope, Unresolved};
use crate::storage::{attachment_upload::{self, UploadClass}, backup_inventory::Manifest, restoration::ValidatedPriorLineage};
use sqlx::AnyConnection;
use std::{collections::{BTreeMap, BTreeSet}, path::Path};
const NS: &str = ".attachment-upload-journal";

pub(super) async fn append(root: &Path, manifest: &Manifest, prior: Option<&ValidatedPriorLineage>, conn: &mut AnyConnection, plan: &mut CustodyPlan, table_exists: bool,completed:Option<&(crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope,BTreeMap<String,attachment_upload::Receipt>)>) -> anyhow::Result<()> {
    let rows: Vec<(String, String)> = if table_exists { sqlx::query_as("SELECT attempt_id,operation_id FROM attachment_upload_receipts ORDER BY attempt_id").fetch_all(&mut *conn).await? } else { Vec::new() };
    let mut receipts = BTreeMap::new();
    for (attempt, operation) in rows {
        anyhow::ensure!(receipts.insert(attempt, operation).is_none(), "Duplicate upload receipt attempt identity");
    }
    let prefix = format!("{NS}/");
    let mut attempts: BTreeSet<String> = manifest.source_custody.directories.0.keys().filter_map(|p|p.strip_prefix(&prefix)).filter(|p|!p.contains('/')).map(str::to_owned).collect();
    attempts.extend(receipts.keys().cloned());
    for attempt in attempts {
        let valid = uuid::Uuid::parse_str(&attempt).is_ok_and(|v|v.to_string()==attempt);
        if !valid {
            plan.unresolved.push(Unresolved { namespace:NS.into(),operation_id:Some(attempt),reason:"invalid_upload_attempt_identity".into() });
            continue;
        }
        let retained_paths: Vec<String> = manifest.files.0.keys().filter(|p|p.starts_with(&format!("{NS}/{attempt}/"))).cloned().collect();
        let Some(operation) = receipts.get(&attempt) else {
            plan.scopes.push(PlannedScope { namespace:NS.into(),operation_id:attempt.clone(),class:OperationClass::InertReservation,evidence:None,evidence_sha256:None,raw_manifest:None,raw_sql_receipt:None,receipt_seal:None,logical_receipt:None,escrow_rows:Vec::new(),retained_paths,associations:Vec::new(),bindings:Vec::new() });
            plan.unresolved.push(Unresolved { namespace:NS.into(),operation_id:Some(attempt),reason:"orphan_stage_preserved_without_authority".into() });
            continue;
        };
        if let Some((envelope,originals))=completed {
            if let Some(scope)=envelope.scopes.iter().find(|s|s.reservation.operation_id==attempt){
                let owner=&originals[&scope.disposition["asset_id"]];
                anyhow::ensure!(owner.intent.request.operation_id==*operation&&owner.intent.attempt_id==attempt,"Completed original upload operation differs");
                let actual=attachment_upload::receipt(conn,operation).await.map_err(anyhow::Error::msg)?.ok_or_else(||anyhow::anyhow!("Completed original upload receipt absent"))?;
                anyhow::ensure!(actual==*owner,"Completed original upload receipt changed during planning");
                let proof=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::original_record_metadata(scope)?;
                let raw=proof[&crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedFileRole::DirectEnvelope{name:"install-proof.json".into()}].clone();
                plan.scopes.push(PlannedScope{namespace:NS.into(),operation_id:attempt.clone(),class:OperationClass::CommittedErased,evidence_sha256:Some(hash(&raw)),evidence:Some(raw),raw_manifest:Some(proof[&crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedFileRole::DirectEnvelope{name:"intent.json".into()}].clone()),raw_sql_receipt:Some(serde_json::to_string(owner)?),receipt_seal:None,logical_receipt:Some(serde_json::to_value(owner)?),escrow_rows:Vec::new(),retained_paths,associations:vec![AssociationEvidence{kind:"attachment-upload".into(),subject_id:owner.intent.asset_id.clone(),current_document_id:None}],bindings:Vec::new()});
                continue;
            }
        }
        let inspected = async {
            let owner = attachment_upload::inspect_restoration(root, conn, operation).await.map_err(anyhow::Error::msg)?;
            anyhow::ensure!(owner.receipt.intent.attempt_id==attempt, "Upload owner attempt differs");
            anyhow::ensure!(owner.bindings.len()==owner.required_group.len() && !owner.bindings.is_empty(), "Upload owner required group differs");
            let mut bindings = Vec::new();
            for binding in &owner.bindings {
                anyhow::ensure!(owner.required_group.contains(&binding.relative) && (binding.original_device,binding.original_inode)==owner.original_identity, "Upload original proof group differs");
                bindings.push(bound(manifest,prior,NS,&attempt,&owner.original_proof,&binding.relative,owner.original_identity,binding.length,&binding.sha256)?);
            }
            let class = match owner.class {
                UploadClass::CommittedInstalled=>OperationClass::CommittedComplete,
                UploadClass::CommittedPending=>anyhow::bail!("Pending upload must finish source recovery before capture; canonical installation authority is absent"),
                UploadClass::Historical=>OperationClass::CommittedHistorical,
                _=>anyhow::bail!("Receipt has uncommitted upload classification"),
            };
            let current = owner.class != UploadClass::Historical;
            let intent = &owner.receipt.intent;
            let associations = vec![AssociationEvidence {kind:"attachment-upload".into(),subject_id:intent.asset_id.clone(),current_document_id:current.then(||intent.request.document_id.clone())}];
            Ok::<_,anyhow::Error>(PlannedScope {namespace:NS.into(),operation_id:attempt.clone(),class,evidence_sha256:Some(hash(&owner.original_proof)),evidence:Some(owner.original_proof),raw_manifest:Some(owner.raw_intent),raw_sql_receipt:Some(String::from_utf8(owner.raw_receipt)?),receipt_seal:None,logical_receipt:Some(serde_json::to_value(&owner.receipt)?),escrow_rows:Vec::new(),retained_paths,associations,bindings})
        }.await;
        match inspected {
            Ok(scope)=>plan.scopes.push(scope),
            Err(_)=>plan.unresolved.push(Unresolved {namespace:NS.into(),operation_id:Some(attempt),reason:"upload_committed_evidence_requires_review".into()}),
        }
    }
    if manifest.files.0.keys().any(|p|p.strip_prefix(&prefix).is_some_and(|s|!s.contains('/'))) {
        plan.unresolved.push(Unresolved {namespace:NS.into(),operation_id:None,reason:"unclassified_namespace_entry".into()});
    }
    Ok(())
}
