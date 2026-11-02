// Held complete cold facts only. This token has no serving/unlink/release API.
struct ClassifiedCompositeBundle {
 bundle:Arc<crate::storage::backup_inventory::ValidatedBundle>,
 evidence:composite::CompositeCaptureEvidence,
 completed:crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::ClassifiedCompletedBundle,
 records:Vec<crate::storage::attachment_upload::admission::CompositeRecordEvidence>,
 pending:Option<admission::ClassifiedAdmissionSource>,
}
impl ClassifiedCompositeBundle {
 async fn from_bundle(bundle:Arc<crate::storage::backup_inventory::ValidatedBundle>)->io::Result<Self>{
  use std::collections::{BTreeMap,BTreeSet};use std::io::Read;
  bundle.fence()?;
  let evidence=bundle.manifest.composite_admission_capture.clone().ok_or_else(||fail("Composite snapshot explicit evidence absent"))?;evidence.validate_shape()?;
  if !bundle.is_qualified_completed_capture(){return Err(fail("Composite snapshot requires exact qualified53"));}
  let mut conn=bundle.db.acquire().await.map_err(|_|fail("Composite snapshot SQL unavailable"))?;
  let capsules=bundle.manifest.source_custody.directories.0.keys().filter_map(|p|p.strip_prefix(".attachment-upload-admission/").filter(|p|!p.contains('/')).map(str::to_owned)).collect();
  evidence.check_complete_reservation_census_in(&mut conn,&capsules).await?;
  let completed=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::ClassifiedCompletedBundle::from_bundle(bundle.clone(),&mut conn).await.map_err(|_|fail("Composite old completion full cold proof invalid"))?;
  crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::check_cold_references(&bundle.data_path(),completed.source_envelope(),&composite_completed_receipts(completed.source_envelope())?,&mut conn).await.map_err(|_|fail("Composite completed snapshot protected references invalid"))?;
  let mut records=Vec::new();let mut pending=Vec::new();
  let mut stages=evidence.prior.roles.scopes.iter().map(|s|s.original.reservation.operation_id.clone()).collect::<BTreeSet<_>>();
  for scope in &evidence.native {
   let attempt=&scope.reservation.operation_id;let mut raws=BTreeMap::new();
   for b in &scope.directories {let source=if let Some(path)=b.role.relative(attempt)?{bundle.manifest.source_custody.directories.0.get(&path).ok_or_else(||fail("Composite original source directory omitted"))?}else{&bundle.manifest.source_custody.root};let captured=evidence.source_directory(scope,&b.role)?;if (source.device,source.inode)!=(captured.device,captured.inode){return Err(fail("Composite original source directory custody differs"));}}
   for b in &scope.files {let path=b.role.relative(attempt)?;let source=bundle.manifest.source_custody.files.0.get(&path).ok_or_else(||fail("Composite original source record omitted"))?;let bytes=bundle.manifest.files.0.get(&path).ok_or_else(||fail("Composite original record inventory omitted"))?;let captured=evidence.source_file(scope,&b.role)?;if source.links!=1||(source.device,source.inode)!=(captured.device,captured.inode)||bytes.size_bytes!=b.length||bytes.sha256!=b.sha256{return Err(fail("Composite source record custody/hash differs"));}let mut raw=Vec::new();bundle.open_owned(&path)?.read_to_end(&mut raw)?;raws.insert(b.role.clone(),raw);}
   let record=crate::storage::attachment_upload::admission::validate_composite_native_records(scope,&raws)?;
   let inventory=bundle.manifest.files.0.keys().cloned().collect();record.validate_snapshot_row_and_paths_in(&mut conn,&inventory).await.map_err(|_|fail("Composite snapshot native full9/raw winner/path differs"))?;
   let mut owned=scope.files.iter().map(|b|b.role.relative(attempt)).collect::<io::Result<BTreeSet<_>>>()?;
   match &scope.class {
    composite::NativeClass::Reserved{original_scope:Some(original)}=>{for b in &original.files {owned.insert(b.role.relative(attempt)?);}pending.push(evidence.pending_scope(scope)?.ok_or_else(||fail("Composite pending capture absent"))?);},
    composite::NativeClass::Reserved{original_scope:None}=>return Err(fail("Composite reserved before-stage class has no supported full cold proof")),
    composite::NativeClass::CommittedDirect{..}=>{record.validate_committed_bundle_in(&bundle,&mut conn).await.map_err(|_|fail("Composite preserved committed singleton proof invalid"))?;owned.insert(format!(".attachment-upload-journal/{attempt}/payload.part"));},
    composite::NativeClass::DisposedDirect{..}|composite::NativeClass::DisposedReplay{..}=>record.validate_disposed_bundle_in(&bundle,&mut conn).await.map_err(|_|fail("Composite disposed complete absence/reference inventory invalid"))?,
   }
   let mut prefixes=vec![format!(".attachment-upload-admission/{attempt}/")];if scope.directories.iter().any(|b|b.role==composite::DirectoryRole::DirectStageDirectory){stages.insert(attempt.clone());prefixes.push(format!(".attachment-upload-journal/{attempt}/"));}
   let actual=bundle.manifest.files.0.keys().filter(|p|prefixes.iter().any(|prefix|p.starts_with(prefix))).cloned().collect::<BTreeSet<_>>();
   if owned!=actual||bundle.manifest.source_custody.directories.0.keys().any(|p|prefixes.iter().any(|prefix|p.starts_with(prefix))){return Err(fail("Composite native complete capsule/stage topology omitted or substituted"));}records.push(record);
  }
  let actual_stages=bundle.manifest.source_custody.directories.0.keys().filter_map(|p|p.strip_prefix(".attachment-upload-journal/").filter(|p|!p.contains('/')).map(str::to_owned)).collect::<BTreeSet<_>>();if actual_stages!=stages{return Err(fail("Composite full native Direct stage census differs"));}
  drop(conn);
  let pending=if pending.is_empty(){None}else{Some(admission::ClassifiedAdmissionSource::cold_scopes(bundle.clone(),admission::AdmissionOwnerEnvelope{version:2,scopes:pending}).await?)};
  let result=Self{bundle,evidence,completed,records,pending};let mut conn=result.bundle.db.acquire().await.map_err(|_|fail("Composite snapshot final SQL unavailable"))?;result.fence_in(&mut conn).await?;drop(conn);Ok(result)
 }
 async fn fence_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()> {
  self.bundle.fence()?;let capsules=self.bundle.manifest.source_custody.directories.0.keys().filter_map(|p|p.strip_prefix(".attachment-upload-admission/").filter(|p|!p.contains('/')).map(str::to_owned)).collect();self.evidence.check_complete_reservation_census_in(conn,&capsules).await?;
  self.completed.fence_in(conn).await.map_err(|_|fail("Composite completed snapshot changed"))?;if let Some(p)=&self.pending {admission_source_sql(conn,p).await?;p.fence()?;}
  for record in &self.records {let paths=self.bundle.manifest.files.0.keys().cloned().collect();record.validate_snapshot_row_and_paths_in(conn,&paths).await.map_err(|_|fail("Composite snapshot native row/path changed"))?;}
  self.bundle.fence()
 }
}
fn composite_completed_receipts(envelope:&crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope)->io::Result<std::collections::BTreeMap<String,crate::storage::attachment_upload::Receipt>> {
 use crate::storage::attachment_upload::admission::committed_erasure::accounting::capture as capture;
 let mut receipts=std::collections::BTreeMap::new();for scope in &envelope.scopes {let raw=capture::original_record_metadata(scope).map_err(|_|fail("Composite original completion metadata invalid"))?;let receipt=capture::validate_original_records(scope,&raw).map_err(|_|fail("Composite original completion receipt invalid"))?;receipts.insert(scope.disposition["asset_id"].clone(),receipt);}Ok(receipts)
}

// Select only non-completed additions. Existing all-completed mixed7 dispatch
// stays byteexact; unsupported unions fail in the held constructor, never fallback.
pub(crate) async fn has_composite_capture_source(path:&Path,conn:&mut sqlx::AnyConnection)->io::Result<bool>{
 let(_,active)=registered(path)?;let Some(active)=active else{return Ok(false)};active.check()?;let Some(roles)=active.ledger.completed_absence.as_ref().or_else(||active.ledger.composite.as_ref().map(|c|&c.completed))else{return Ok(false)};
 if !matches!(active.ledger.version,3|4)||active.ledger.admission.is_some(){return Err(fail("Composite source requires canonical Ledger3/4"));}
 let completed:Vec<String>=sqlx::query_scalar("SELECT original_attempt_id FROM attachment_erasure_dispositions ORDER BY original_attempt_id LIMIT 513").fetch_all(&mut *conn).await.map_err(|_|fail("Composite source completed attempt inventory unavailable"))?;
 let all:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id LIMIT 513").fetch_all(&mut *conn).await.map_err(|_|fail("Composite source reservation inventory unavailable"))?;
 let expected=roles.scopes.iter().map(|s|s.original.reservation.operation_id.as_str()).collect::<std::collections::BTreeSet<_>>();
 if all.len()>512||completed.len()>512||expected.iter().any(|id|!all.iter().any(|v|v==id)||!completed.iter().any(|v|v==id)){return Err(fail("Composite source original old completion inventory missing"));}active.check()?;Ok(active.ledger.version==4||all.iter().any(|id|!completed.contains(id)))
}

struct CertifiedCompositeDestination {roles:composite::CompositeOwnerEnvelope,source:ClassifiedCompositeBundle}
async fn composite_certify_destination(bundle:&crate::storage::backup_inventory::ValidatedBundle,d:&PlannedDestination,conn:&mut sqlx::AnyConnection)->io::Result<Option<CertifiedCompositeDestination>>{
 if bundle.manifest.composite_admission_capture.is_none(){return Ok(None);}
 bundle.fence()?;d.fence()?;
 // A distinct pool avoids recursive acquisition of the outer one-connection
 // read snapshot; canonical bytes and original held roots must still be exact.
 let path=bundle.data_path().parent().ok_or_else(||fail("Composite original bundle parent absent"))?.to_owned();
 let independently_held=Arc::new(crate::storage::backup_inventory::verify(&path).await.map_err(|_|fail("Composite independent bundle hold failed"))?);
 if independently_held.raw_manifest!=bundle.raw_manifest||independently_held.copy_overlap_identities()?!=bundle.copy_overlap_identities()?{return Err(fail("Composite held bundle source differs"));}
 let source=ClassifiedCompositeBundle::from_bundle(independently_held).await?;
 source.fence_in(conn).await?;
 let completed=completed_certify_destination_component(bundle,d,conn).await?.ok_or_else(||fail("Composite completed component certification absent"))?;
 if completed.roles.version!=2{return Err(fail("Composite fresh completed owner must be version2"));}
 let pending=if let Some(p)=&source.pending {
  let classified=admission::ClassifiedAdmissionSource::cold_scopes(source.bundle.clone(),p.source_roles().clone()).await?;
  Some(admission_certify_destination(bundle,d,classified)?)
 }else{None};
 let mut native=Vec::new();
 for original in &source.evidence.native {
  let attempt=&original.reservation.operation_id;let mut directories=Vec::new();let mut files=Vec::new();
  for b in &original.directories {let adopted=if let Some(path)=b.role.relative(attempt)?{let(held,stamp)=d.directories.get(&path).ok_or_else(||fail("Composite fresh certified directory absent"))?;if identity(held)?!=(stamp.device,stamp.inode)||held.metadata()?.mode()&0o777!=0o700{return Err(fail("Composite fresh directory identity/mode differs"));}admission::Identity{device:stamp.device,inode:stamp.inode}}else{admission::Identity{device:d.root_stamp.device,inode:d.root_stamp.inode}};directories.push(composite::OwnedDirectory{role:b.role.clone(),adopted});}
  for b in &original.files {let path=b.role.relative(attempt)?;let held=d.leaves.get(&path).ok_or_else(||fail("Composite fresh certified record absent"))?;if identity(&held.held)?!=(held.stamp.device,held.stamp.inode)||held.bytes.size_bytes!=b.length||held.bytes.sha256!=b.sha256||held.stamp.mode&0o777!=0o600||held.stamp.links!=1{return Err(fail("Composite fresh record identity/bytes/mode differs"));}files.push(composite::OwnedFile{role:b.role.clone(),adopted:admission::Identity{device:held.stamp.device,inode:held.stamp.inode}});}
  let pending=pending.as_ref().and_then(|p|p.roles.scopes.iter().find(|s|s.attempt_id==*attempt)).cloned();
  native.push(composite::NativeOwnerScope{original:original.clone(),source_basis:source.evidence.source_basis(original),directories,files,pending});
 }
 let roles=composite::CompositeOwnerEnvelope{version:source.evidence.version,completed:completed.roles,native};roles.validate(&admission::Identity{device:d.root_stamp.device,inode:d.root_stamp.inode})?;
 source.fence_in(conn).await?;bundle.fence()?;d.fence()?;Ok(Some(CertifiedCompositeDestination{roles,source}))
}

async fn composite_independent_snapshot(bundle:&crate::storage::backup_inventory::ValidatedBundle)->io::Result<ClassifiedCompositeBundle>{
 let path=bundle.data_path().parent().ok_or_else(||fail("Composite snapshot parent missing"))?.to_owned();let held=Arc::new(crate::storage::backup_inventory::verify(&path).await.map_err(|_|fail("Composite independent snapshot failed"))?);if held.raw_manifest!=bundle.raw_manifest||held.copy_overlap_identities()?!=bundle.copy_overlap_identities()?{return Err(fail("Composite original held snapshot mismatch"));}ClassifiedCompositeBundle::from_bundle(held).await
}
pub(crate) async fn inspect_composite_snapshot(bundle:&crate::storage::backup_inventory::ValidatedBundle)->io::Result<()>{let source=composite_independent_snapshot(bundle).await?;let mut conn=source.bundle.db.acquire().await.map_err(|_|fail("Composite snapshot SQL unavailable"))?;source.fence_in(&mut conn).await}
impl ClassifiedCompositeBundle{fn fence(&self)->io::Result<()>{self.bundle.fence()?;self.completed.fence().map_err(|_|fail("Composite completed snapshot changed"))?;if let Some(p)=&self.pending{p.fence()?;}self.bundle.fence()}}
