// Original-native completed absence transported by a held qualified PG snapshot.
// This private proof is not a native writer, cleanup or generic serving token.
fn pg_completed_capture_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope>,D::Error>{
 use serde::Deserialize;crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope::deserialize(d).map(Some)
}
fn pg_prior_completed_capture_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<crate::storage::restoration::PriorCompletedCaptureEvidence>,D::Error>{use serde::Deserialize;crate::storage::restoration::PriorCompletedCaptureEvidence::deserialize(d).map(Some)}
fn pg_mixed_completed_capture_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<crate::storage::restoration::MixedCompletedCaptureEvidence>,D::Error>{use serde::Deserialize;crate::storage::restoration::MixedCompletedCaptureEvidence::deserialize(d).map(Some)}
fn pg_prior_completed_roles(snapshot:&InspectedPgSnapshot)->anyhow::Result<completed::CompletedOwnerEnvelope> {
 let e=snapshot.manifest.prior_completed_absence_capture.as_ref().or_else(||snapshot.manifest.mixed_completed_absence_capture.as_ref().map(|m|&m.prior)).or_else(||snapshot.manifest.composite_admission_capture.as_ref().map(|c|&c.prior)).context("Prior completed PG capture absent")?;e.validate_shape()?;snapshot.held.fence()?;
 ensure!(snapshot.profile.has_erasure()&&snapshot.manifest.format_version==4&&matches!(snapshot.manifest.source_authority,Some(PgCapturedSourceAuthority::PgRestore{..}))&&e.source_root.device==snapshot.manifest.source_root.device&&e.source_root.inode==snapshot.manifest.source_root.inode&&e.generation_id==snapshot.history.current_id,"Prior completed PG source identity/generation differs");
 let raw=snapshot.history.raw_generations.get(&snapshot.history.current_id).context("Prior completed PG captured finalized generation absent")?;ensure!(digest(raw)==e.ledger_sha256,"Prior completed PG canonical generation digest differs");let old:HistoryEnvelope=serde_json::from_slice(raw)?;
 if let Some(c)=snapshot.manifest.composite_admission_capture.as_ref().filter(|c|c.version==2){let roles=old.runtime.composite.as_ref().context("Repeated composite PG canonical Ledger4 roles absent")?;ensure!(old.version==3&&old.runtime.version==4&&old.runtime.admission.is_none()&&roles.completed==e.roles&&c.prior_native.as_ref()==Some(&roles.native),"Repeated composite PG canonical source lineage differs");roles.validate(&e.source_root)?;}else{ensure!(old.version==3&&old.runtime.version==3&&old.runtime.admission.is_none()&&old.runtime.completed_absence.as_ref()==Some(&e.roles),"Prior completed PG exact finalized Ledger3 roles differ");}snapshot.held.fence()?;Ok(e.roles.clone())
}
fn pg_completed_envelope(snapshot:&InspectedPgSnapshot)->anyhow::Result<Option<crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope>> {
 if let Some(c)=&snapshot.manifest.composite_admission_capture{c.validate_shape()?;let roles=pg_prior_completed_roles(snapshot)?;ensure!(roles==c.prior.roles,"Composite prior canonical roles differ");return Ok(Some(crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope{version:1,source_kind:"original_native_direct_v1".into(),scopes:roles.scopes.into_iter().map(|s|s.original).collect()}));}
 if let Some(m)=&snapshot.manifest.mixed_completed_absence_capture{m.validate_shape()?;let roles=pg_prior_completed_roles(snapshot)?;ensure!(roles==m.prior.roles,"Mixed prior canonical roles differ");return Ok(Some(m.envelope()?));}
 if snapshot.manifest.prior_completed_absence_capture.is_some(){ensure!(snapshot.manifest.completed_absence_capture.is_none(),"Original/prior completed PG capture conflict");let roles=pg_prior_completed_roles(snapshot)?;Ok(Some(crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope{version:1,source_kind:"original_native_direct_v1".into(),scopes:roles.scopes.into_iter().map(|s|s.original).collect()}))}else{Ok(snapshot.manifest.completed_absence_capture.clone())}
}
struct PgValidatedCompletedCapture {manifest_sha256:String,envelope_sha256:String,attempts:std::collections::BTreeSet<String>,registry_absences:crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedRegistryAbsences}
fn pg_completed_records(snapshot:&InspectedPgSnapshot)->anyhow::Result<Option<BTreeMap<String,crate::storage::attachment_upload::Receipt>>>{
 use crate::storage::attachment_upload::admission::committed_erasure::accounting::capture as capture;
 let Some(envelope)=pg_completed_envelope(snapshot)? else{return Ok(None)};
 let prior=if snapshot.manifest.prior_completed_absence_capture.is_some()||snapshot.manifest.mixed_completed_absence_capture.is_some()||snapshot.manifest.composite_admission_capture.is_some(){Some(pg_prior_completed_roles(snapshot)?)}else{None};
 snapshot.held.fence()?;envelope.validate_shape()?;
 ensure!(snapshot.profile.has_erasure()&&snapshot.manifest.format_version>=3&&snapshot.manifest.admission_owner_capture.is_none()&&snapshot.manifest.prior_admission_owner_capture.is_none(),"Completed original PG transport profile differs");
 let mut receipts=BTreeMap::new();
 for scope in &envelope.scopes {
  let attempt=&scope.reservation.operation_id;let mut raws=BTreeMap::new();
  for b in &scope.files {
   let path=b.role.relative(attempt)?;
   let Some(PgSnapshotEntry::File{device,inode,links,length,sha256})=snapshot.manifest.files.0.get(&path)else{bail!("Completed PG record absent")};
   let captured=if let Some(p)=prior.as_ref().and_then(|p|p.scopes.iter().find(|s|s.original==*scope)){&p.files.iter().find(|s|s.role==b.role).context("Prior completed PG file role absent")?.adopted}else{&b.captured};
   ensure!(captured.device==*device&&captured.inode==*inode&&*links==1&&b.length==*length&&b.sha256==*sha256,"Completed PG record original custody differs");
   let held=snapshot.held.roles.get(&format!("files/{path}")).context("Completed PG held record absent")?;
   let mut raw=vec![0;b.length as usize];held.held.read_exact_at(&mut raw,0)?;
   ensure!(digest(&raw)==b.sha256&&physical_stamp(&held.held)?==held.stamp,"Completed PG record changed");raws.insert(b.role.clone(),raw);
  }
  let receipt=capture::validate_original_records(scope,&raws)?;
  for b in &scope.directories {
   let captured=if let Some(p)=prior.as_ref().and_then(|p|p.scopes.iter().find(|s|s.original==*scope)){&p.directories.iter().find(|s|s.role==b.role).context("Prior completed PG directory role absent")?.adopted}else{&b.captured};
   if let Some(path)=b.role.relative(attempt)? {
    let Some(PgSnapshotEntry::Directory{device,inode})=snapshot.manifest.files.0.get(&path)else{bail!("Completed PG directory absent")};
    ensure!(captured.device==*device&&captured.inode==*inode,"Completed PG directory custody differs");
   }else{ensure!(captured.device==snapshot.manifest.source_root.device&&captured.inode==snapshot.manifest.source_root.inode,"Completed PG original root differs");}
  }
  for namespace in [format!(".attachment-upload-admission/{attempt}"),format!(".attachment-upload-journal/{attempt}")] {
   let prefix=format!("{namespace}/");
   let expected=scope.files.iter().map(|b|b.role.relative(attempt)).collect::<anyhow::Result<Vec<_>>>()?.into_iter().filter(|p|p.starts_with(&prefix)).collect::<std::collections::BTreeSet<_>>();
   let actual=snapshot.manifest.files.0.keys().filter(|p|p.starts_with(&prefix)).cloned().collect::<std::collections::BTreeSet<_>>();
   ensure!(actual==expected,"Completed PG record topology differs");
  }
  for path in [&receipt.intent.storage_path,&format!(".attachment-upload-journal/{attempt}/payload.part")] {ensure!(!snapshot.manifest.files.0.contains_key(path),"Completed PG erased path resurrected");}
  let origin:serde_json::Value=serde_json::from_str(&scope.original_stage_origin_json)?;
  let device=origin["payload"]["device"].as_u64().context("Completed erased payload device absent")?;let inode=origin["payload"]["inode"].as_u64().context("Completed erased payload inode absent")?;
  ensure!(!snapshot.manifest.files.0.values().any(|entry|matches!(entry,PgSnapshotEntry::File{device:d,inode:i,..} if *d==device&&*i==inode)),"Completed PG erased original inode retained by alias");
  ensure!(receipts.insert(scope.disposition["asset_id"].clone(),receipt).is_none(),"Completed PG duplicate original");
 }
 let expected=envelope.scopes.iter().map(|s|s.reservation.operation_id.clone()).collect::<std::collections::BTreeSet<_>>();
 let actual=snapshot.manifest.files.0.iter().filter_map(|(p,e)|if matches!(e,PgSnapshotEntry::Directory{..}){p.strip_prefix(".attachment-upload-admission/").filter(|n|!n.contains('/')).map(str::to_owned)}else{None}).collect::<std::collections::BTreeSet<_>>();
 let copied_root=Root::open(&snapshot.held.root.absolute.join("files"))?;
 let designation=crate::storage::data_root_designation::Designation::capture_held(&copied_root.absolute,copied_root.fd())?;
 let inventory=snapshot.manifest.files.0.iter().filter_map(|(path,entry)|match entry{PgSnapshotEntry::File{device,inode,length,sha256,..}=>Some((path,*device,*inode,*length,sha256)),_=>None}).map(|(path,d,i,n,h)|{
  let held=&snapshot.held.roles.get(&format!("files/{path}")).context("Completed PG full held inventory role absent")?.held;
  Ok((d,i,n,h.as_str(),designation.as_ref().map(|token|(token,path.as_str(),held))))
 }).collect::<anyhow::Result<Vec<_>>>()?;
 capture::exclude_completed_payload_inventory(&envelope,inventory)?;if let Some(token)=&designation{token.fence()?;}
 snapshot.held.fence()?;
 ensure!(snapshot.manifest.composite_admission_capture.is_some()||actual==expected,"Mixed completed PG capsule classes unsupported");snapshot.held.fence()?;Ok(Some(receipts))
}
async fn pg_validate_completed_capture(snapshot:&InspectedPgSnapshot,tx:&mut sqlx::Transaction<'_,sqlx::Any>)->anyhow::Result<Option<PgValidatedCompletedCapture>> {
 use crate::storage::attachment_upload::admission::committed_erasure::accounting::capture as capture;
 let disposition=snapshot.tables.get("attachment_erasure_dispositions");
 if snapshot.manifest.completed_absence_capture.is_none()&&snapshot.manifest.prior_completed_absence_capture.is_none()&&snapshot.manifest.mixed_completed_absence_capture.is_none()&&snapshot.manifest.composite_admission_capture.is_none(){ensure!(disposition.is_none_or(|t|t.rows.is_empty()),"Nonempty53 completed PG SQL lacks native absence transport");return Ok(None)}
 let receipts=pg_completed_records(snapshot)?.context("Completed PG original proof absent")?;
 let envelope=pg_completed_envelope(snapshot)?.context("Completed PG original proof absent")?;
 capture::check_completed_sql(&envelope,&receipts,&mut **tx).await?;
 let mut expected=envelope.scopes.iter().map(|s|s.reservation.operation_id.clone()).collect::<Vec<_>>();expected.sort();
 let actual:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id").fetch_all(&mut **tx).await?;
 ensure!(snapshot.manifest.composite_admission_capture.is_some()||actual==expected,"Mixed completed PG reservation classes unsupported");
 capture::check_cold_references(&snapshot.held.root.absolute.join("files"),&envelope,&receipts,&mut **tx).await?;
 let registry_absences=capture::completed_registry_absences_in(&snapshot.held.root.absolute.join("files"),&envelope,&receipts,&mut **tx).await?;
 snapshot.held.fence()?;Ok(Some(PgValidatedCompletedCapture{registry_absences,manifest_sha256:digest(&snapshot.held.manifest),envelope_sha256:digest(&serde_json::to_vec(&envelope)?),attempts:expected.into_iter().collect()}))
}
fn pg_certified_completed_roles(snapshot:&InspectedPgSnapshot,d:&OwnedPgBackup,proof:Option<&PgValidatedCompletedCapture>)->anyhow::Result<Option<completed::CompletedOwnerEnvelope>> {
 let Some(envelope)=pg_completed_envelope(snapshot)? else{ensure!(proof.is_none(),"Unexpected completed PG proof");return Ok(None)};
 let proof=proof.context("Completed PG requires private imported full SQL/reference proof")?;
 ensure!(proof.manifest_sha256==digest(&snapshot.held.manifest)&&proof.envelope_sha256==digest(&serde_json::to_vec(&envelope)?),"Completed PG private proof source differs");
 pg_completed_records(snapshot)?.context("Completed PG source absent")?;d.fence()?;
 let source_basis_required=snapshot.manifest.composite_admission_capture.is_some()||snapshot.manifest.mixed_completed_absence_capture.is_some()||snapshot.manifest.prior_completed_absence_capture.as_ref().is_some_and(|p|p.roles.version==2);
 let mut scopes=Vec::new();
 for original in &envelope.scopes {
  let attempt=&original.reservation.operation_id;let mut directories=Vec::new();let mut files=Vec::new();
  for b in &original.directories {
   let adopted=if let Some(path)=b.role.relative(attempt)?{let(held,stamp)=d.directories.get(&path).context("Completed certified PG directory absent")?;ensure!(physical_stamp(held)?==*stamp,"Completed certified PG directory changed");admission::Identity{device:stamp.device,inode:stamp.inode}}else{admission::Identity{device:d.root_identity.0,inode:d.root_identity.1}};
   directories.push(completed::CompletedOwnedDirectory{role:b.role.clone(),adopted});
  }
  for b in &original.files {
   let path=b.role.relative(attempt)?;let source=&snapshot.held.roles[&format!("files/{path}")];let target=d.roles.get(&path).context("Completed certified PG record absent")?;
   ensure!(target.bytes==source.bytes&&physical_stamp(&target.held)?==target.stamp,"Completed certified PG record differs");
   files.push(completed::CompletedOwnedFile{role:b.role.clone(),adopted:admission::Identity{device:target.stamp.device,inode:target.stamp.inode}});
  }
  let raws=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::original_record_metadata(original)?;
  let receipt=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::validate_original_records(original,&raws)?;
  for path in [&receipt.intent.storage_path,&format!(".attachment-upload-journal/{attempt}/payload.part")] {
   ensure!(!d.roles.contains_key(path)&&!d.directories.contains_key(path),"Completed certified PG erased path resurrected");
   let(parent,name)=path.rsplit_once('/').context("Completed erased PG parent absent")?;let(held,_)=d.directories.get(parent).context("Completed erased PG parent not held")?;ensure!(!exists(held,name)?,"Completed certified PG erased leaf exists");
  }
  let source_basis=if source_basis_required{
   let root=admission::Identity{device:snapshot.manifest.source_root.device,inode:snapshot.manifest.source_root.inode};
   let source_directories=original.directories.iter().map(|b|{let captured=if let Some(path)=b.role.relative(attempt)?{let Some(PgSnapshotEntry::Directory{device,inode})=snapshot.manifest.files.0.get(&path)else{bail!("Completed PG source basis directory absent")};admission::Identity{device:*device,inode:*inode}}else{root.clone()};Ok(completed::CompletedSourceDirectory{role:b.role.clone(),captured})}).collect::<anyhow::Result<Vec<_>>>()?;
   let source_files=original.files.iter().map(|b|{let path=b.role.relative(attempt)?;let Some(PgSnapshotEntry::File{device,inode,..})=snapshot.manifest.files.0.get(&path)else{bail!("Completed PG source basis file absent")};Ok(completed::CompletedSourceFile{role:b.role.clone(),captured:admission::Identity{device:*device,inode:*inode}})}).collect::<anyhow::Result<Vec<_>>>()?;
   Some(completed::CompletedSourceBasis{root,directories:source_directories,files:source_files})
  }else{None};
  scopes.push(completed::CompletedOwnerScope{original:original.clone(),directories,files,source_basis});
 }
 let roles=completed::CompletedOwnerEnvelope{version:if source_basis_required{2}else{1},scopes};roles.validate(&admission::Identity{device:d.root_identity.0,inode:d.root_identity.1})?;snapshot.held.fence()?;d.fence()?;Ok(Some(roles))
}

#[cfg(test)]
pub(crate) fn pg_completed_test_native_pool(s:&EngineServing)->sqlx::AnyPool{s.pool.clone()}
#[cfg(test)]
pub(crate) fn pg_completed_test_restored_pool(s:&RestoredPgServing)->sqlx::AnyPool{s.pool.clone()}

#[cfg(test)]
mod pg_completed_transport_tests {
 use super::*;
 #[derive(serde::Deserialize)]
 #[serde(deny_unknown_fields)]
 struct Optional {#[serde(default,deserialize_with="pg_completed_capture_nonnull")] completed_absence_capture:Option<crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope>}
 #[test]
 fn strict_optional_completed_transport(){
  assert!(serde_json::from_str::<Optional>("{}").unwrap().completed_absence_capture.is_none());
  for raw in [r#"{"completed_absence_capture":null}"#,r#"{"completed_absence_capture":false}"#,r#"{"completed_absence_capture":{"version":1,"source_kind":"original_native_direct_v1","scopes":[],"foreign":1}}"#,r#"{"completed_absence_capture":{"version":1,"source_kind":"original_native_direct_v1","scopes":[]},"completed_absence_capture":{"version":1,"source_kind":"original_native_direct_v1","scopes":[]}}"#]{assert!(serde_json::from_str::<Optional>(raw).is_err());}
  let value=serde_json::from_str::<Optional>(r#"{"completed_absence_capture":{"version":1,"source_kind":"original_native_direct_v1","scopes":[]}}"#).unwrap();assert!(value.completed_absence_capture.unwrap().validate_shape().is_err());
 }
}

#[cfg(test)]
pub(crate) async fn pg_completed_test_registry_controls(path:&Path,pool:&sqlx::AnyPool)->anyhow::Result<()> {
 let snapshot=inspect_postgres_snapshot(path)?;let mut tx=pool.begin().await?;
 let completed=pg_validate_completed_capture(&snapshot,&mut tx).await?.context("Genuine completed test proof absent")?;
 ensure!(crate::storage::attachment_versions::inspect_registry_snapshot(&mut tx).await.is_err(),"Ordinary registry parser admitted absent current association");
 let good=crate::storage::attachment_versions::inspect_completed_registry_snapshot(&mut tx,&completed.registry_absences).await.map_err(|e|anyhow!(e))?;
 ensure!(!good.completed_absences().is_empty()&&good.current_associations().is_empty(),"Completed registry fabricated current serving association");
 let id=good.completed_absences().iter().next().unwrap().clone();
 let name:String=sqlx::query_scalar("SELECT original_name FROM attachment_version_records WHERE version_id=$1").bind(&id).fetch_one(&mut *tx).await?;
 sqlx::query("UPDATE attachment_version_records SET original_name='FOREIGN ORIGINAL' WHERE version_id=$1").bind(&id).execute(&mut *tx).await?;
 ensure!(crate::storage::attachment_versions::inspect_completed_registry_snapshot(&mut tx,&completed.registry_absences).await.is_err(),"Substituted immutable registry original admitted");
 sqlx::query("UPDATE attachment_version_records SET original_name=$1 WHERE version_id=$2").bind(name).bind(&id).execute(&mut *tx).await?;
 // A self-consistent altered predecessor receipt/scalar must not substitute history.
 if let Some((op,raw,expected))=sqlx::query_as::<_,(String,String,String)>("SELECT operation_id,receipt_json,expected_version_id FROM attachment_version_changes ORDER BY operation_id LIMIT 1").fetch_optional(&mut *tx).await? {
  let mut value:crate::storage::attachment_versions::ReplacementChange=serde_json::from_str(&raw)?;let foreign=uuid::Uuid::new_v4().to_string();value.expected.current_version_id=foreign.clone();let changed=serde_json::to_string(&value)?;
  sqlx::query("UPDATE attachment_version_changes SET receipt_json=$1,expected_version_id=$2 WHERE operation_id=$3").bind(changed).bind(&foreign).bind(&op).execute(&mut *tx).await?;
  crate::storage::attachment_versions::test_replacement_receipt_shape_in(&mut tx,&op).await.map_err(|e|anyhow!(e))?;
  ensure!(crate::storage::attachment_versions::inspect_completed_registry_snapshot(&mut tx,&completed.registry_absences).await.is_err(),"Self-consistent foreign predecessor admitted completed family");
  sqlx::query("UPDATE attachment_version_changes SET receipt_json=$1,expected_version_id=$2 WHERE operation_id=$3").bind(raw).bind(expected).bind(op).execute(&mut *tx).await?;
 }
 // Retained direct retirement is exact captured Plan2 evidence, not replacement metadata.
 if let Some((retired,raw,association))=sqlx::query_as::<_,(String,String,String)>("SELECT attachment_id,receipt_json,association_json FROM attachment_retirements ORDER BY attachment_id LIMIT 1").fetch_optional(&mut *tx).await? {
  let mut value:crate::storage::attachment_retirement::Retirement=serde_json::from_str(&raw)?;value.association.4="spaces/foreign/attachments/foreign.bin".into();let changed=serde_json::to_string(&value)?;let association_changed=serde_json::to_string(&value.association)?;
  sqlx::query("UPDATE attachment_retirements SET receipt_json=$1,association_json=$2 WHERE attachment_id=$3").bind(changed).bind(association_changed).bind(&retired).execute(&mut *tx).await?;
  ensure!(crate::storage::attachment_retirement::read(&mut tx,&retired).await.map_err(|e|anyhow!(e))?.as_ref()==Some(&value),"Canonical retirement control must pass ordinary typed scalar/receipt parsing");
  ensure!(crate::storage::attachment_versions::inspect_completed_registry_snapshot(&mut tx,&completed.registry_absences).await.is_err(),"Self-consistent foreign retirement original admitted");
  sqlx::query("UPDATE attachment_retirements SET receipt_json=$1,association_json=$2 WHERE attachment_id=$3").bind(raw).bind(association).bind(retired).execute(&mut *tx).await?;
 }
 let raw:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE asset_id=$1").bind(&id).fetch_one(&mut *tx).await?;
 sqlx::query("UPDATE attachment_upload_receipts SET receipt_json=$1 WHERE asset_id=$2").bind(format!("{raw} ")).bind(&id).execute(&mut *tx).await?;
 ensure!(crate::storage::attachment_versions::inspect_completed_registry_snapshot(&mut tx,&completed.registry_absences).await.is_err(),"Original receipt raw byte substitution admitted");
 tx.rollback().await?;snapshot.held.fence()?;println!("ACTUAL_CANONICAL_TYPED_PREDECESSOR_RETIREMENT_SUBSTITUTION_REFUSALS_PASS");println!("ACTUAL_COMPLETED_REGISTRY_ORDINARY_REFUSAL_NO_CURRENT_ASSOCIATION_IMMUTABLE_ORIGIN_RAW_RECEIPT_CONTROLS_PASS");Ok(())
}

#[cfg(test)]
mod prior_completed_cold_control_tests {
 use super::*;
 #[tokio::test]
 #[ignore="closed genuine root-produced prior PG snapshot, unique private destinations and protected URL required"]
 async fn actual_prior_completed_pg_cold_controls()->anyhow::Result<()> {
  use std::os::unix::fs::{MetadataExt,PermissionsExt,DirBuilderExt};
  let original=PathBuf::from(std::env::var("COMPLETED_PRIOR_PG_SNAPSHOT_PATH")?);
  let url=std::env::var("REPLAY_CAPACITY_TEST_PG_URL")?;
  let baseline=inspect_postgres_snapshot(&original)?;ensure!(baseline.manifest.prior_completed_absence_capture.is_some(),"Actual prior producer evidence required");ensure!(pg_completed_records(&baseline)?.is_some(),"Actual original3 records required");baseline.held.fence()?;
  let raw:serde_json::Value=serde_json::from_slice(&std::fs::read(original.join("manifest.json"))?)?;
  let base=std::env::temp_dir().join(format!("prior-completed-pg-cold-controls-{}",uuid::Uuid::new_v4().simple()));std::fs::DirBuilder::new().mode(0o700).create(&base)?;
  for mode in ["ledger","generation","missing_scope","adopted_inode","capsule_bytes","foreign_payload","omitted"] {
   let copied=base.join(mode);let status=std::process::Command::new("/usr/bin/python3").args(["-I","-c","import shutil,sys; shutil.copytree(sys.argv[1],sys.argv[2])"]).arg(&original).arg(&copied).status()?;ensure!(status.success(),"Private negative fixture copy failed");let mut m=raw.clone();
   match mode {
    "ledger"=>m["prior_completed_absence_capture"]["ledger_sha256"]="0".repeat(64).into(),
    "generation"=>m["prior_completed_absence_capture"]["generation_id"]=uuid::Uuid::new_v4().to_string().into(),
    "missing_scope"=>{m["prior_completed_absence_capture"]["roles"]["scopes"].as_array_mut().context("Actual scopes required")?.pop();},
    "adopted_inode"=>{let highest=m["files"].as_object().context("Actual file inventory required")?.values().filter_map(|v|v["inode"].as_u64()).max().context("Actual source inodes required")?;let high=highest.checked_add(1_000_000).context("Fixture inode overflow")?;m["prior_completed_absence_capture"]["roles"]["scopes"][0]["files"][0]["adopted"]["inode"]=high.into();},
    "capsule_bytes"=>{
     let scope=&baseline.manifest.prior_completed_absence_capture.as_ref().unwrap().roles.scopes[0];let role=&scope.original.files[0].role;let path=role.relative(&scope.original.reservation.operation_id)?;let file=copied.join("files").join(&path);let mut bytes=std::fs::read(&file)?;bytes.push(b' ');std::fs::write(&file,&bytes)?;m["files"][&path]["length"]=(bytes.len()as u64).into();m["files"][&path]["sha256"]=digest(&bytes).into();
    },
    "foreign_payload"=>{
     let body=b"ORIGINAL NATIVE WINNER";let receipts=pg_completed_records(&baseline)?.unwrap();ensure!(receipts.values().any(|r|r.intent.binary_sha256==digest(body)),"Genuine original payload body required");let path="spaces/outbox/attachments/foreign-retained-copy.bin";let file=copied.join("files").join(path);std::fs::write(&file,body)?;std::fs::set_permissions(&file,std::fs::Permissions::from_mode(0o600))?;let md=std::fs::metadata(&file)?;m["files"][path]=serde_json::json!({"kind":"file","device":md.dev(),"inode":md.ino(),"links":md.nlink(),"length":md.len(),"sha256":digest(body)});
    },
    "omitted"=>{m.as_object_mut().context("Actual manifest object required")?.remove("prior_completed_absence_capture");},
    _=>unreachable!(),
   }
   std::fs::write(copied.join("manifest.json"),serde_json::to_vec(&m)?)?;
   let inspected=inspect_postgres_snapshot(&copied)?;inspected.held.fence()?;
   if mode=="omitted" {ensure!(pg_completed_records(&inspected)?.is_none(),"Missing role minted completed proof");}
   else{ensure!(pg_completed_records(&inspected).is_err(),"{mode} minted completed original proof");}
   let target=base.join(format!("{mode}-refused-target"));ensure!(restore_qualified_postgres(&copied,&target,&url,true).await.is_err(),"{mode} admitted completed absence/charge release");ensure!(std::fs::read(copied.join("manifest.json"))?==serde_json::to_vec(&m)?,"Negative source evidence changed");baseline.held.fence()?;
   println!("ACTUAL_PRIOR_COMPLETED_PG_SELFCONSISTENT_COLD_CONTROL_PASS {mode}");
  }
  baseline.held.fence()?;println!("ACTUAL_PRIOR_COMPLETED_PG_COLD_LEDGER_GENERATION_SCOPE_ADOPTED_INODE_CAPSULE_PAYLOAD_OMISSION_NO_ABSENCE_ADMISSION_PASS");Ok(())
 }
}
