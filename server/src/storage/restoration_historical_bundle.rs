// Whole held qualified cold facts for the historical preservation purpose only.
// No current-reference, admission, serving, cleanup, or accounting token is made.
struct ClassifiedHistoricalBundle {
 bundle:Arc<crate::storage::backup_inventory::ValidatedBundle>,
 evidence:historical::HistoricalCompositePreservationEvidence,
 completed:crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope,
 receipts:std::collections::BTreeMap<String,crate::storage::attachment_upload::Receipt>,
 records:Vec<crate::storage::attachment_upload::admission::CompositeRecordEvidence>,
 retained:HistoricalRetainedSnapshot,
}
impl ClassifiedHistoricalBundle {
 async fn from_bundle(bundle:Arc<crate::storage::backup_inventory::ValidatedBundle>)->io::Result<Self>{
  use std::collections::{BTreeMap,BTreeSet};use crate::storage::attachment_upload::admission::committed_erasure::accounting::capture as capture;
  bundle.fence()?;if !bundle.is_qualified_completed_capture(){return Err(fail("Historical physical snapshot requires exact qualified53"));}
  let evidence=bundle.manifest.historical_composite_preservation_capture.clone().ok_or_else(||fail("Historical explicit qualified source evidence absent"))?;evidence.validate_shape()?;
  let mut conn=bundle.db.acquire().await.map_err(|_|fail("Historical source snapshot SQL unavailable"))?;
  let prior=prior_completed_bundle_roles(&bundle,&mut conn,&evidence.prior).await?;
  let completed=capture::CompletedAbsenceEnvelope{version:1,source_kind:"original_native_direct_v1".into(),scopes:prior.scopes.iter().map(|s|s.original.clone()).collect()};completed.validate_shape().map_err(io::Error::other)?;
  let mut receipts=BTreeMap::new();
  for scope in &prior.scopes {
   let original=&scope.original;let attempt=&original.reservation.operation_id;let mut raw=BTreeMap::new();
   for b in &original.directories {
    let source=if let Some(path)=b.role.relative(attempt).map_err(io::Error::other)?{bundle.manifest.source_custody.directories.0.get(&path).ok_or_else(||fail("Historical completed source directory omitted"))?}else{&bundle.manifest.source_custody.root};
    let adopted=scope.directories.iter().find(|d|d.role==b.role).ok_or_else(||fail("Historical prior completed directory role omitted"))?;
    if(source.device,source.inode)!=(adopted.adopted.device,adopted.adopted.inode){return Err(fail("Historical prior completed directory custody differs"));}
   }
   for b in &original.files {
    let path=b.role.relative(attempt).map_err(io::Error::other)?;let source=bundle.manifest.source_custody.files.0.get(&path).ok_or_else(||fail("Historical completed source file omitted"))?;let bytes=bundle.manifest.files.0.get(&path).ok_or_else(||fail("Historical completed file bytes omitted"))?;let adopted=scope.files.iter().find(|f|f.role==b.role).ok_or_else(||fail("Historical prior completed file role omitted"))?;
    if(source.device,source.inode)!=(adopted.adopted.device,adopted.adopted.inode)||source.links!=1||bytes.size_bytes!=b.length||bytes.sha256!=b.sha256||b.length>8*1024*1024{return Err(fail("Historical completed source record identity/bytes/bound differs"));}
    let file=bundle.open_owned(&path)?;let mut bytes=vec![0;b.length as usize];file.read_exact_at(&mut bytes,0)?;if digest(&bytes)!=b.sha256{return Err(fail("Historical completed held record changed"));}raw.insert(b.role.clone(),bytes);
   }
   let receipt=capture::validate_original_records(original,&raw).map_err(io::Error::other)?;
   for role in [capture::CompletedDirectoryRole::AdmissionCapsule,capture::CompletedDirectoryRole::DirectStageDirectory]{let path=role.relative(attempt).map_err(io::Error::other)?.ok_or_else(||fail("Historical completed operation path absent"))?;let prefix=path+"/";let actual=bundle.manifest.files.0.keys().filter_map(|p|p.strip_prefix(&prefix).map(str::to_owned)).collect::<BTreeSet<_>>();let expected=original.files.iter().map(|b|b.role.relative(attempt).map_err(io::Error::other)).collect::<io::Result<Vec<_>>>()?.iter().filter_map(|p|p.strip_prefix(&prefix).map(str::to_owned)).collect::<BTreeSet<_>>();if actual!=expected||bundle.manifest.source_custody.directories.0.keys().any(|p|p.starts_with(&prefix)){return Err(fail("Historical completed operation membership differs"));}}
   for path in [&receipt.intent.storage_path,&format!(".attachment-upload-journal/{attempt}/payload.part")]{if bundle.manifest.files.0.contains_key(path)||bundle.manifest.source_custody.directories.0.contains_key(path){return Err(fail("Historical erased completed path recreated"));}}
   if receipts.insert(original.disposition["asset_id"].clone(),receipt).is_some(){return Err(fail("Historical completed asset duplicated"));}
  }
  capture::check_completed_sql(&completed,&receipts,&mut conn).await.map_err(|_|fail("Historical completed full raw immutable SQL differs"))?;
  let designation=bundle.metadata_designation()?;
  let inventory=bundle.manifest.files.0.iter().filter(|(p,_)|p.as_str()!="starlane-notes.db").map(|(path,bytes)|{let id=bundle.manifest.source_custody.files.0.get(path).ok_or_else(||fail("Historical whole inventory source identity omitted"))?;Ok((path.clone(),id.device,id.inode,bytes.size_bytes,bytes.sha256.clone(),bundle.open_owned(path)?))}).collect::<io::Result<Vec<_>>>()?;
  capture::exclude_completed_payload_inventory(&completed,inventory.iter().map(|(path,d,i,n,h,f)|(*d,*i,*n,h.as_str(),designation.as_ref().map(|token|(token,path.as_str(),f))))).map_err(|_|fail("Historical erased original inode/bytes retained in complete inventory"))?;
  let mut records=Vec::new();let mut stages=completed.scopes.iter().map(|s|s.reservation.operation_id.clone()).collect::<BTreeSet<_>>();let mut attempts=stages.clone();
  for scope in &evidence.native {
   let attempt=&scope.reservation.operation_id;if !attempts.insert(attempt.clone()){return Err(fail("Historical native/completed attempts overlap"));}
   for b in &scope.directories {let source=if let Some(path)=b.role.relative(attempt).map_err(io::Error::other)?{bundle.manifest.source_custody.directories.0.get(&path).ok_or_else(||fail("Historical native source directory omitted"))?}else{&bundle.manifest.source_custody.root};if(source.device,source.inode)!=(b.captured.device,b.captured.inode)||b.original!=b.captured{return Err(fail("Historical native current source directory not original"));}}
   let mut raw=BTreeMap::new();let mut owned=BTreeSet::new();
   for b in &scope.files {let path=b.role.relative(attempt).map_err(io::Error::other)?;let source=bundle.manifest.source_custody.files.0.get(&path).ok_or_else(||fail("Historical native source record identity omitted"))?;let bytes=bundle.manifest.files.0.get(&path).ok_or_else(||fail("Historical native source record bytes omitted"))?;if(source.device,source.inode)!=(b.captured.device,b.captured.inode)||b.original!=b.captured||source.links!=1||bytes.size_bytes!=b.length||bytes.sha256!=b.sha256||b.length>8*1024*1024{return Err(fail("Historical native original record custody/bytes/bound differs"));}let f=bundle.open_owned(&path)?;let mut bytes=vec![0;b.length as usize];f.read_exact_at(&mut bytes,0)?;if digest(&bytes)!=b.sha256{return Err(fail("Historical native held original record changed"));}raw.insert(b.role.clone(),bytes);owned.insert(path);}
   let record=crate::storage::attachment_upload::admission::validate_composite_native_records(scope,&raw)?;let paths=bundle.manifest.files.0.keys().cloned().collect();record.validate_snapshot_row_and_paths_in(&mut conn,&paths).await.map_err(|_|fail("Historical native full9 raw winner/path differs"))?;
   match &scope.class {
    composite::NativeClass::Reserved{original_scope:Some(partial)}=>{
     if partial.attempt_id!=*attempt||partial.original_claim_json!=scope.claim_json||partial.admission_claim_sha256!=scope.reservation.admission_claim_sha256||partial.request_sha256!=scope.reservation.request_sha256||i64::try_from(partial.maximum_bytes).ok()!=Some(scope.reservation.maximum_bytes)||!matches!(partial.reservation,admission::ReservationState::Reserved){return Err(fail("Historical pending exact original claim/producer/row/maximum differs"));}
     let mut partial_raw=BTreeMap::new();
     for b in &partial.files {let path=b.role.relative(attempt).map_err(io::Error::other)?;let id=bundle.manifest.source_custody.files.0.get(&path).ok_or_else(||fail("Historical pending original file identity omitted"))?;let bytes=bundle.manifest.files.0.get(&path).ok_or_else(||fail("Historical pending original bytes omitted"))?;if(id.device,id.inode)!=(b.captured.device,b.captured.inode)||b.original!=b.captured||b.adopted!=b.captured||id.links!=1||bytes.size_bytes!=b.length||bytes.sha256!=b.sha256{return Err(fail("Historical pending original payload/record differs"));}let f=bundle.open_owned(&path)?;let streamed=matches!(&b.role,admission::FileRole::DirectStage{name}if name=="payload.part");let bytes=if streamed{crate::storage::attachment_upload::admission::verify_captured_direct_payload(&f,b.length,&b.sha256,partial.maximum_bytes)?;Vec::new()}else{if b.length>8*1024*1024{return Err(fail("Historical pending metadata exceeds bounded v1 contract"));}let mut bytes=vec![0;b.length as usize];f.read_exact_at(&mut bytes,0)?;if digest(&bytes)!=b.sha256{return Err(fail("Historical pending held metadata changed"));}bytes};partial_raw.insert(b.role.clone(),bytes);owned.insert(path);}
     for b in &partial.directories {let id=if let Some(path)=b.role.relative(attempt).map_err(io::Error::other)?{bundle.manifest.source_custody.directories.0.get(&path).ok_or_else(||fail("Historical pending directory omitted"))?}else{&bundle.manifest.source_custody.root};if(id.device,id.inode)!=(b.captured.device,b.captured.inode)||b.original!=b.captured||b.adopted!=b.captured{return Err(fail("Historical pending original directory differs"));}}
     crate::storage::attachment_upload::admission::validate_captured_partial_direct(partial,&partial_raw)?;
     let winners:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachment_upload_receipts WHERE attempt_id=$1").bind(attempt).fetch_one(&mut *conn).await.map_err(|_|fail("Historical pending winning proof unavailable"))?;if winners!=0||partial.maximum_bytes as i64!=scope.reservation.maximum_bytes{return Err(fail("Historical pending winner/original maximum differs"));}
    },
    composite::NativeClass::CommittedDirect{..}=>{record.validate_committed_bundle_in(&bundle,&mut conn).await.map_err(|_|fail("Historical committed original bytes/winner/singleton proof differs"))?;owned.insert(format!(".attachment-upload-journal/{attempt}/payload.part"));},
    composite::NativeClass::DisposedDirect{..}=>record.validate_historical_disposed_bundle_in(&bundle,&mut conn).await.map_err(|_|fail("Historical disposed full physical absence differs"))?,
    _=>return Err(fail("Historical v1 native class unsupported")),
   }
   let mut prefixes=vec![format!(".attachment-upload-admission/{attempt}/")];if scope.directories.iter().any(|b|b.role==composite::DirectoryRole::DirectStageDirectory){stages.insert(attempt.clone());prefixes.push(format!(".attachment-upload-journal/{attempt}/"));}
   let observed=bundle.manifest.files.0.keys().filter(|p|prefixes.iter().any(|prefix|p.starts_with(prefix))).cloned().collect::<BTreeSet<_>>();if observed!=owned||bundle.manifest.source_custody.directories.0.keys().any(|p|prefixes.iter().any(|prefix|p.starts_with(prefix))){return Err(fail("Historical native complete operation topology differs"));}records.push(record);
  }
  let capsules=bundle.manifest.source_custody.directories.0.keys().filter_map(|p|p.strip_prefix(".attachment-upload-admission/").filter(|p|!p.contains('/')).map(str::to_owned)).collect::<BTreeSet<_>>();let actual_stages=bundle.manifest.source_custody.directories.0.keys().filter_map(|p|p.strip_prefix(".attachment-upload-journal/").filter(|p|!p.contains('/')).map(str::to_owned)).collect::<BTreeSet<_>>();let all:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id LIMIT 513").fetch_all(&mut *conn).await.map_err(|_|fail("Historical complete cold reservation census unavailable"))?;
  if attempts!=capsules||stages!=actual_stages||all.len()!=attempts.len()||all.iter().any(|id|!attempts.contains(id)){return Err(fail("Historical complete disjoint reservation/capsule/stage census differs"));}
  if let Some(d)=&designation{d.fence()?;}drop(conn);let retained=HistoricalRetainedSnapshot::capture(&bundle)?;let result=Self{bundle,evidence,completed,receipts,records,retained};let mut conn=result.bundle.db.acquire().await.map_err(|_|fail("Historical final cold SQL unavailable"))?;result.fence_in(&mut conn).await?;drop(conn);Ok(result)
 }
 async fn fence_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()> {
  use crate::storage::attachment_upload::admission::committed_erasure::accounting::capture as capture;
  self.bundle.fence()?;capture::check_completed_sql(&self.completed,&self.receipts,conn).await.map_err(|_|fail("Historical complete immutable completed SQL changed"))?;
  for asset in self.receipts.keys(){let n:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachments WHERE id=$1").bind(asset).fetch_one(&mut *conn).await.map_err(|_|fail("Historical current association census unavailable"))?;if n!=0{return Err(fail("Historical erased original acquired a live association"));}}
  for record in &self.records{let paths=self.bundle.manifest.files.0.keys().cloned().collect();record.validate_snapshot_row_and_paths_in(conn,&paths).await.map_err(|_|fail("Historical native raw row/path changed"))?;}
  let projection=engine_transfer::historical_application_projection_in(conn,None).await.map_err(|_|fail("Historical full cold application projection unavailable"))?;if projection!=self.evidence.application_projection.sha256{return Err(fail("Historical full opaque application projection differs"));}self.bundle.fence()
 }
}

pub(crate) async fn inspect_historical_snapshot(bundle:&crate::storage::backup_inventory::ValidatedBundle)->io::Result<()> {
 let parent=bundle.data_path().parent().ok_or_else(||fail("Historical snapshot parent absent"))?.to_owned();
 let held=Arc::new(crate::storage::backup_inventory::verify(&parent).await.map_err(|_|fail("Historical independent complete bundle hold failed"))?);
 if held.raw_manifest!=bundle.raw_manifest||held.copy_overlap_identities()?!=bundle.copy_overlap_identities()?{return Err(fail("Historical original held snapshot differs"));}
 let source=ClassifiedHistoricalBundle::from_bundle(held).await?;let mut conn=source.bundle.db.acquire().await.map_err(|_|fail("Historical independent cold SQL unavailable"))?;source.fence_in(&mut conn).await
}

// Created only from held original snapshot and an actual incomplete destination.
// No serialized caller roles and no generic absence/admission/serving conversion.
struct CertifiedHistoricalDestination {
 roles:historical::HistoricalOwnerEnvelope,
 source:ClassifiedHistoricalBundle,
}
fn historical_destination_directory(d:&PlannedDestination,path:Option<String>)->io::Result<admission::Identity>{
 if let Some(path)=path{let(held,stamp)=d.directories.get(&path).ok_or_else(||fail("Historical certified directory absent"))?;if physical_stamp(held)?!=*stamp||stamp.mode&0o777!=0o700{return Err(fail("Historical certified held directory differs"));}Ok(admission::Identity{device:stamp.device,inode:stamp.inode})}
 else{if physical_stamp(d.copied.root.fd())?!=d.root_stamp{return Err(fail("Historical original target root changed"));}Ok(admission::Identity{device:d.root_stamp.device,inode:d.root_stamp.inode})}
}
fn historical_destination_file(d:&PlannedDestination,path:&str,length:u64,sha256:&str,exclusive:bool)->io::Result<admission::Identity>{
 let held=d.leaves.get(path).ok_or_else(||fail("Historical certified file absent"))?;
 if physical_stamp(&held.held)?!=held.stamp||held.stamp.mode&0o777!=0o600||exclusive&&held.stamp.links!=1||held.bytes.size_bytes!=length||held.bytes.sha256!=sha256||planned_hash(&held.held)?!=held.bytes{return Err(fail("Historical actual certified file identity/bytes/mode differs"));}
 Ok(admission::Identity{device:held.stamp.device,inode:held.stamp.inode})
}
async fn historical_independent_bundle(bundle:&crate::storage::backup_inventory::ValidatedBundle)->io::Result<ClassifiedHistoricalBundle>{
 let parent=bundle.data_path().parent().ok_or_else(||fail("Historical original bundle parent absent"))?.to_owned();
 let held=Arc::new(crate::storage::backup_inventory::verify(&parent).await.map_err(|_|fail("Historical independent bundle hold failed"))?);
 if held.raw_manifest!=bundle.raw_manifest||held.copy_overlap_identities()?!=bundle.copy_overlap_identities()?{return Err(fail("Historical actual original bundle custody differs"));}
 ClassifiedHistoricalBundle::from_bundle(held).await
}
async fn historical_certify_destination(bundle:&crate::storage::backup_inventory::ValidatedBundle,d:&PlannedDestination,conn:&mut sqlx::AnyConnection)->io::Result<Option<CertifiedHistoricalDestination>>{
 use crate::storage::attachment_upload::admission::committed_erasure::accounting::capture as capture;
 if bundle.manifest.historical_composite_preservation_capture.is_none(){return Ok(None);}
 bundle.fence()?;d.fence()?;let source=historical_independent_bundle(bundle).await?;source.fence_in(conn).await?;
 let mut completed=Vec::new();
 for old in &source.evidence.prior.roles.scopes{
  let original=&old.original;let attempt=&original.reservation.operation_id;
  let directories=original.directories.iter().map(|b|Ok(completed::CompletedOwnedDirectory{role:b.role.clone(),adopted:historical_destination_directory(d,b.role.relative(attempt).map_err(io::Error::other)?)?})).collect::<io::Result<Vec<_>>>()?;
  let files=original.files.iter().map(|b|Ok(completed::CompletedOwnedFile{role:b.role.clone(),adopted:historical_destination_file(d,&b.role.relative(attempt).map_err(io::Error::other)?,b.length,&b.sha256,true)?})).collect::<io::Result<Vec<_>>>()?;
  let basis=completed::CompletedSourceBasis{root:source.evidence.prior.source_root.clone(),directories:old.directories.iter().map(|b|completed::CompletedSourceDirectory{role:b.role.clone(),captured:b.adopted.clone()}).collect(),files:old.files.iter().map(|b|completed::CompletedSourceFile{role:b.role.clone(),captured:b.adopted.clone()}).collect()};
  completed.push(completed::CompletedOwnerScope{original:original.clone(),directories,files,source_basis:Some(basis)});
 }
 let mut native=Vec::new();
 for original in &source.evidence.native{
  let attempt=&original.reservation.operation_id;
  let directories=original.directories.iter().map(|b|Ok(composite::OwnedDirectory{role:b.role.clone(),adopted:historical_destination_directory(d,b.role.relative(attempt)?)?})).collect::<io::Result<Vec<_>>>()?;
  let files=original.files.iter().map(|b|Ok(composite::OwnedFile{role:b.role.clone(),adopted:historical_destination_file(d,&b.role.relative(attempt)?,b.length,&b.sha256,true)?})).collect::<io::Result<Vec<_>>>()?;
  let mut payloads=Vec::new();let mut pending=None;
  let role=match &original.class{
   composite::NativeClass::Reserved{original_scope:Some(scope)}=>{
    // Wire-only remapping. No ClassifiedAdmissionSource/WriterLease is created.
    let mut scope=scope.clone();
    for b in &mut scope.directories{b.adopted=historical_destination_directory(d,b.role.relative(attempt)?)?;}
    for b in &mut scope.files{b.adopted=historical_destination_file(d,&b.role.relative(attempt)?,b.length,&b.sha256,true)?;}
    pending=Some(scope);historical::HistoricalPhysicalRole::RetainedPending
   },
   composite::NativeClass::CommittedDirect{receipt_json}=>{
    let receipt:crate::storage::attachment_upload::Receipt=serde_json::from_str(receipt_json).map_err(io::Error::other)?;
    for role in [historical::HistoricalPayloadRole::CommittedCanonical,historical::HistoricalPayloadRole::CommittedStagePayload]{
     let path=role.relative(original)?;let id=bundle.manifest.source_custody.files.0.get(&path).ok_or_else(||fail("Historical committed actual original payload custody omitted"))?;
     let bytes=bundle.manifest.files.0.get(&path).ok_or_else(||fail("Historical committed actual payload omitted"))?;
     if receipt.intent.byte_length<0||bytes.size_bytes!=receipt.intent.byte_length as u64||bytes.sha256!=receipt.intent.binary_sha256{return Err(fail("Historical committed payload/winner bytes differ"));}
     payloads.push(historical::HistoricalPayloadBinding{role,source:admission::Identity{device:id.device,inode:id.inode},adopted:historical_destination_file(d,&path,bytes.size_bytes,&bytes.sha256,false)?,length:bytes.size_bytes,sha256:bytes.sha256.clone()});
    }historical::HistoricalPhysicalRole::CommittedPreservation
   },
   composite::NativeClass::DisposedDirect{..}=>historical::HistoricalPhysicalRole::HistoricalDisposedPhysicalAbsence,
   _=>return Err(fail("Historical certified native class unsupported")),
  };
  native.push(historical::HistoricalNativeOwner{role,material:composite::NativeOwnerScope{original:original.clone(),source_basis:composite::NativeSourceBasis::Original,directories,files,pending},payloads});
 }
 // The complete actual rebuilt target must contain neither an erased original
 // inode nor its complete bytes. This check grants no current-reference fact.
 let inventory=d.leaves.iter().filter(|(p,_)|p.as_str()!="starlane-notes.db").map(|(path,leaf)|(path,leaf.stamp.device,leaf.stamp.inode,leaf.bytes.size_bytes,leaf.bytes.sha256.as_str(),&leaf.held)).collect::<Vec<_>>();
 // Metadata exemption remains descriptor-purpose bound to the original source;
 // match only the exact copied path authenticated by that held designation.
 let designation=source.bundle.metadata_designation()?;
 let original_files=inventory.iter().filter(|(path,_,_,_,_,_)|source.bundle.manifest.files.0.contains_key(*path)).map(|(path,_,_,_,_,_)|Ok(((*path).clone(),source.bundle.open_owned(path)?))).collect::<io::Result<std::collections::BTreeMap<_,_>>>()?;
 capture::exclude_completed_payload_inventory(&source.completed,inventory.iter().map(|(path,dev,ino,n,h,_)|(*dev,*ino,*n,*h,designation.as_ref().and_then(|token|original_files.get(*path).map(|f|(token,path.as_str(),f)))))).map_err(|_|fail("Historical rebuilt target retains erased original inode/bytes"))?;
 for receipt in source.receipts.values(){let attempt=&receipt.intent.attempt_id;for path in [&receipt.intent.storage_path,&format!(".attachment-upload-journal/{attempt}/payload.part")]{if d.leaves.contains_key(path)||d.directories.contains_key(path){return Err(fail("Historical target erased path recreated"));}}}
 let roles=historical::HistoricalOwnerEnvelope{version:1,purpose:historical::PURPOSE.into(),source:source.evidence.clone(),completed:historical::HistoricalCompletedOwner{role:historical::HistoricalPhysicalRole::HistoricalCompletedPhysicalAbsence,material:completed::CompletedOwnerEnvelope{version:2,scopes:completed}},native};
 roles.validate(&admission::Identity{device:d.root_stamp.device,inode:d.root_stamp.inode})?;
 source.fence_in(conn).await?;bundle.fence()?;d.fence()?;Ok(Some(CertifiedHistoricalDestination{roles,source}))
}

pub(crate) async fn historical_completed_plan_facts(bundle:&crate::storage::backup_inventory::ValidatedBundle,conn:&mut sqlx::AnyConnection)->io::Result<(crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope,std::collections::BTreeMap<String,crate::storage::attachment_upload::Receipt>)>{
 let source=historical_independent_bundle(bundle).await?;source.fence_in(conn).await?;Ok((source.completed.clone(),source.receipts.clone()))
}
#[cfg(test)]
pub(crate) async fn inspect_historical_certified_target(bundle:&crate::storage::backup_inventory::ValidatedBundle,d:&PlannedDestination)->io::Result<()> {
 let mut conn=bundle.db.acquire().await.map_err(|_|fail("Historical test original snapshot SQL unavailable"))?;
 let cert=historical_certify_destination(bundle,d,&mut conn).await?.ok_or_else(||fail("Historical target certifier absent"))?;
 cert.roles.validate(&admission::Identity{device:d.root_stamp.device,inode:d.root_stamp.inode})?;
 let raw=serde_json::to_vec(&cert.roles).map_err(io::Error::other)?;let round:historical::HistoricalOwnerEnvelope=serde_json::from_slice(&raw).map_err(io::Error::other)?;if round!=cert.roles{return Err(fail("Historical certified roles roundtrip differ"));}
 cert.source.fence_in(&mut conn).await?;d.fence()?;Ok(())
}

impl ClassifiedHistoricalBundle {fn fence(&self)->io::Result<()>{self.bundle.fence()}}
// Nominal, nonserializable witness minted only by the original publication
// VFS disk readback. No decoded ledger/projection can construct this witness.
struct HistoricalPublicationWitness {
 original_rows:Vec<HistorySqlRow>,original_pointer:Vec<(i64,String)>,
 committed_row:HistorySqlRow,committed_pointer:Vec<(i64,String)>,
 postpublication_projection:String,root:admission::Identity,main:PhysicalStamp,
}
impl HistoricalPublicationWitness {
 fn check(&self,d:&PlannedDestination,original:&(Vec<HistorySqlRow>,Vec<(i64,String)>),row:&HistorySqlRow)->io::Result<()>{
  let main=d.leaves.get("starlane-notes.db").ok_or_else(||fail("Historical actual committed main absent"))?;
  if self.original_rows!=original.0||self.original_pointer!=original.1||self.committed_row!=*row||self.committed_pointer!=vec![(1,row.generation_id.clone())]||self.root!=(admission::Identity{device:d.root_stamp.device,inode:d.root_stamp.inode})||physical_stamp(&main.held)?!=self.main||!valid_hash(&self.postpublication_projection){return Err(fail("Historical held publication witness differs"));}Ok(())
 }
}

// Private immutable descriptor snapshot, constructed before any target reader
// reservation. This is not a source/current/serving authority or serialized grant.
struct HistoricalRetainedSnapshot {root:Root,stamp:PhysicalStamp,files:std::collections::BTreeMap<String,PlannedLeaf>,directories:std::collections::BTreeMap<String,(File,PhysicalStamp)>}
impl HistoricalRetainedSnapshot {
 fn capture(bundle:&crate::storage::backup_inventory::ValidatedBundle)->io::Result<Self>{
  bundle.fence()?;let path=bundle.data_path().parent().ok_or_else(||fail("Historical retained source parent absent"))?.to_owned();let root=Root::open(&path)?;
  if identity(root.fd())?!=bundle.copy_overlap_identities()?[0]{return Err(fail("Historical retained original snapshot root differs"));}
  let mut result=Self{stamp:physical_stamp(root.fd())?,root,files:std::collections::BTreeMap::new(),directories:std::collections::BTreeMap::new()};
  fn walk(fd:&File,base:&str,files:&mut std::collections::BTreeMap<String,PlannedLeaf>,dirs:&mut std::collections::BTreeMap<String,(File,PhysicalStamp)>,depth:usize)->io::Result<()>{
   if depth>32||files.len()+dirs.len()>16384{return Err(fail("Historical retained snapshot census bound exceeded"));}let before=physical_stamp(fd)?;
   for name in prior_capsule_names(fd)?{if !valid_relative(&name)||name.contains(['/', '\\']){return Err(fail("Historical retained unsafe component"));}let path=if base.is_empty(){name.clone()}else{format!("{base}/{name}")};let held=open_at(fd,&name,libc::O_RDONLY|libc::O_NONBLOCK)?;
    if held.metadata()?.is_dir(){let stamp=physical_stamp(&held)?;if stamp.mode&0o777!=0o700{return Err(fail("Historical retained directory not private"));}walk(&held,&path,files,dirs,depth+1)?;dirs.insert(path,(held,stamp));}
    else if held.metadata()?.is_file(){let stamp=physical_stamp(&held)?;let bytes=planned_hash(&held)?;files.insert(path,PlannedLeaf{held,stamp,bytes});}
    else{return Err(fail("Historical retained snapshot has special entry"));}
   }if physical_stamp(fd)?!=before{return Err(fail("Historical snapshot changed while retaining original descriptors"));}Ok(())
  }
  walk(result.root.fd(),"",&mut result.files,&mut result.directories,0)?;
  if result.directories.get("data").map(|(_,s)|(s.device,s.inode))!=Some(bundle.copy_overlap_identities()?[1])||read_ledger(&result.files.get("manifest.json").ok_or_else(||fail("Historical retained manifest absent"))?.held)?!=bundle.raw_manifest{return Err(fail("Historical retained actual bundle pairing differs"));}
  bundle.fence()?;result.fence()?;Ok(result)
 }
 fn fence(&self)->io::Result<()>{historical_retained_tree_fence(&self.root,&self.stamp,&self.directories,&self.files)}
}
// No leaf is reopened, duplicated or closed while the caller's transaction is
// alive: exact names/full nofollow stat bind the original held directory/file
// descriptors, and bytes are hashed only through those retained descriptors.
trait HistoricalLeafDescriptor {fn held(&self)->&File;fn stamp(&self)->&PhysicalStamp;fn bytes(&self)->&crate::storage::backup_inventory::Bytes;}
impl HistoricalLeafDescriptor for PlannedLeaf{fn held(&self)->&File{&self.held}fn stamp(&self)->&PhysicalStamp{&self.stamp}fn bytes(&self)->&crate::storage::backup_inventory::Bytes{&self.bytes}}
enum HistoricalFreshLeaf {Owned(PlannedLeaf),OriginalMain{held:Arc<File>,stamp:PhysicalStamp,bytes:crate::storage::backup_inventory::Bytes}}
impl HistoricalLeafDescriptor for HistoricalFreshLeaf{
 fn held(&self)->&File{match self{Self::Owned(l)=>&l.held,Self::OriginalMain{held,..}=>held.as_ref()}}
 fn stamp(&self)->&PhysicalStamp{match self{Self::Owned(l)=>&l.stamp,Self::OriginalMain{stamp,..}=>stamp}}
 fn bytes(&self)->&crate::storage::backup_inventory::Bytes{match self{Self::Owned(l)=>&l.bytes,Self::OriginalMain{bytes,..}=>bytes}}
}
fn historical_retained_tree_fence<L:HistoricalLeafDescriptor>(root:&Root,root_stamp:&PhysicalStamp,directories:&std::collections::BTreeMap<String,(File,PhysicalStamp)>,leaves:&std::collections::BTreeMap<String,L>)->io::Result<()> {
 fn ancestry(root:&Root)->io::Result<()>{let current=Root::open(&root.absolute)?;if current.chain.len()!=root.chain.len(){return Err(fail("Historical held ancestry differs"));}for(a,b)in current.chain.iter().zip(&root.chain){if identity(a)?!=identity(b)?{return Err(fail("Historical original ancestry replaced"));}}Ok(())}
 fn stamp_at(parent:&File,name:&str)->io::Result<PhysicalStamp>{let name=CString::new(name).map_err(|_|fail("Historical census NUL component"))?;let mut st:libc::stat=unsafe{std::mem::zeroed()};if unsafe{libc::fstatat(parent.as_raw_fd(),name.as_ptr(),&mut st,libc::AT_SYMLINK_NOFOLLOW)}<0{return Err(io::Error::last_os_error());}if st.st_size<0{return Err(fail("Historical negative observed length"));}Ok(PhysicalStamp{device:st.st_dev as u64,inode:st.st_ino as u64,length:st.st_size as u64,links:st.st_nlink as u64,mode:st.st_mode as u32,mtime:st.st_mtime,mtime_ns:st.st_mtime_nsec,ctime:st.st_ctime,ctime_ns:st.st_ctime_nsec})}
 fn walk<L:HistoricalLeafDescriptor>(fd:&File,base:&str,directories:&std::collections::BTreeMap<String,(File,PhysicalStamp)>,leaves:&std::collections::BTreeMap<String,L>,files:&mut std::collections::BTreeSet<String>,dirs:&mut std::collections::BTreeSet<String>,depth:usize)->io::Result<()>{
  if depth>32||files.len()+dirs.len()>16384{return Err(fail("Historical complete descriptor census bound exceeded"));}let before=physical_stamp(fd)?;
  for name in prior_capsule_names(fd)?{
   if !valid_relative(&name)||name.contains(['/', '\\']){return Err(fail("Historical unsafe descriptor component"));}let path=if base.is_empty(){name.clone()}else{format!("{base}/{name}")};let current=stamp_at(fd,&name)?;
   if let Some(leaf)=leaves.get(&path){if current!=*leaf.stamp()||current.mode&libc::S_IFMT!=libc::S_IFREG||!files.insert(path){return Err(fail("Historical file name/identity/stamp/alias differs"));}}
   else if let Some((held,expected))=directories.get(&path){if current!=*expected||physical_stamp(held)?!=*expected||current.mode&libc::S_IFMT!=libc::S_IFDIR||!dirs.insert(path.clone()){return Err(fail("Historical retained directory replaced"));}walk(held,&path,directories,leaves,files,dirs,depth+1)?;}
   else{return Err(fail("Historical unknown entry or substituted alias"));}
  }if physical_stamp(fd)?!=before{return Err(fail("Historical directory changed during complete census"));}Ok(())
 }
 ancestry(root)?;if physical_stamp(root.fd())?!=*root_stamp{return Err(fail("Historical held root stamp differs"));}let mut files=std::collections::BTreeSet::new();let mut dirs=std::collections::BTreeSet::new();walk(root.fd(),"",directories,leaves,&mut files,&mut dirs,0)?;
 if files!=leaves.keys().cloned().collect()||dirs!=directories.keys().cloned().collect(){return Err(fail("Historical exhaustive descriptor names differ"));}
 let mut groups=std::collections::BTreeMap::<(u64,u64),(u64,u64)>::new();
 for leaf in leaves.values(){if physical_stamp(leaf.held())?!=*leaf.stamp()||planned_hash(leaf.held())?!=*leaf.bytes(){return Err(fail("Historical original held file changed"));}let e=groups.entry((leaf.stamp().device,leaf.stamp().inode)).or_insert((0,leaf.stamp().links));e.0+=1;if e.1!=leaf.stamp().links{return Err(fail("Historical held link stamps differ"));}}
 if groups.values().any(|(n,links)|n!=links){return Err(fail("Historical original file has unaccounted alias"));}
 for(held,stamp)in directories.values(){if physical_stamp(held)?!=*stamp{return Err(fail("Historical original held directory changed"));}}
 if physical_stamp(root.fd())?!=*root_stamp{return Err(fail("Historical held root changed after complete hashes"));}ancestry(root)
}
fn historical_retired_observation_fence(d:&PlannedDestination)->io::Result<()>{d.group_custody_fence()?;historical_retained_tree_fence(&d.copied.root,&d.root_stamp,&d.directories,&d.leaves)?;d.group_custody_fence()}
/// Original-object physical observation only. This borrows the real publisher's
/// held source/destination/witness; parsed Ledger5 can never construct it.
pub(crate) struct HistoricalPublishedObservation<'a>{ready:&'a BootReadyDestination}
#[derive(Clone,Debug,PartialEq,Eq)]
pub(crate) struct HistoricalPhysicalSummary {
 pub completed_scopes:usize,pub new_committed:usize,pub new_pending:usize,pub new_disposed:usize,
 pub retained_pending_maximum:u64,pub reference_semantics:historical::HistoricalReferenceSemantics,
}
impl<'a> HistoricalPublishedObservation<'a>{
 pub(crate) async fn hold_original_published(ready:&'a BootReadyDestination,conn:&mut sqlx::AnyConnection)->io::Result<Self>{
  let proof=Self{ready};proof.recheck_in(conn).await?;Ok(proof)
 }
 pub(crate) fn summary(&self)->io::Result<HistoricalPhysicalSummary>{
  let source=self.ready.historical_source.as_ref().ok_or_else(||fail("Historical original source facts absent"))?;source.retained.fence()?;
  let mut summary=HistoricalPhysicalSummary{completed_scopes:source.completed.scopes.len(),new_committed:0,new_pending:0,new_disposed:0,retained_pending_maximum:0,reference_semantics:historical::HistoricalReferenceSemantics::NotEstablished};
  for scope in &source.evidence.native{match &scope.class{
   composite::NativeClass::Reserved{original_scope:Some(p)}=>{summary.new_pending+=1;summary.retained_pending_maximum=summary.retained_pending_maximum.checked_add(p.maximum_bytes).ok_or_else(||fail("Historical pending total overflow"))?;},
   composite::NativeClass::CommittedDirect{..}=>summary.new_committed+=1,
   composite::NativeClass::DisposedDirect{..}=>summary.new_disposed+=1,
   _=>return Err(fail("Historical observed native class unsupported")),
  }}source.retained.fence()?;Ok(summary)
 }
 pub(crate) async fn recheck_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()> {
  let ready=self.ready;historical_retired_observation_fence(&ready.destination)?;
  let source=ready.historical_source.as_ref().ok_or_else(||fail("Historical original held source absent"))?;
  let witness=ready.historical_witness.as_ref().ok_or_else(||fail("Historical original publication witness absent"))?;
  source.retained.fence()?;witness.check(&ready.destination,&ready.historical_original,&witness.committed_row)?;
  // Descriptor-purpose check on the actual target. Never open/close the main
  // file incidentally while the caller's SQLite transaction is alive.
  qualified_sqlite_connection(&ready.destination.copied.root,conn).await?;
  if qualified_sqlite_path_identity(&ready.destination.copied.root)?!=(witness.main.device,witness.main.inode){return Err(fail("Historical original connected target main replaced"));}
  let mut expected=witness.original_rows.clone();expected.push(witness.committed_row.clone());expected.sort_by(|a,b|a.generation_id.cmp(&b.generation_id));
  let raw=read_ledger(&ready.destination.leaves.get(&format!("{NAMESPACE}/current.json")).ok_or_else(||fail("Historical retained current ledger absent"))?.held)?;
  if digest(&raw)!=witness.committed_row.ledger_sha256{return Err(fail("Historical current actual committed ledger differs"));}
  let current=history_parse(&raw)?;
  if current.ledger.version!=5||current.envelope.as_ref().is_none_or(|e|e.version!=4)||current.ledger.historical_preservation.as_ref().is_none_or(|h|h.source!=source.evidence){return Err(fail("Historical original purpose/current source facts differ"));}
  if history_rows(conn).await?!=(expected.clone(),witness.committed_pointer.clone()){return Err(fail("Historical observed original/new full row closure differs"));}
  let projection=engine_transfer::historical_application_projection_in(conn,None).await.map_err(|_|fail("Historical original connected full application projection unavailable"))?;
  if projection!=witness.postpublication_projection{return Err(fail("Historical current full opaque application bytes differ"));}
  if history_rows(conn).await?!=(expected,witness.committed_pointer.clone()){return Err(fail("Historical target generation changed during observation"));}
  qualified_sqlite_connection(&ready.destination.copied.root,conn).await?;
  source.retained.fence()?;witness.check(&ready.destination,&ready.historical_original,&witness.committed_row)?;historical_retired_observation_fence(&ready.destination)
 }
}

// Fresh target observation is admitted from the original registered Active,
// never from decoded Ledger5 alone. Its borrowed full-range pager exclusion
// cannot be dropped before this holder; no writer/cleanup/currentref API exists.
struct HistoricalFinalizedObservation<'a>{
 active:Arc<Active>,pager:HistoricalPhysicalPager<'a>,
 roles:historical::HistoricalOwnerEnvelope,
 tree:HistoricalFreshInventory,
 history:(Vec<HistorySqlRow>,Vec<(i64,String)>),
 original_history:(Vec<HistorySqlRow>,Vec<(i64,String)>),
 post_projection:String,
 records:Vec<crate::storage::attachment_upload::admission::CompositeRecordEvidence>,
 receipts:std::collections::BTreeMap<String,crate::storage::attachment_upload::Receipt>,
 designation:Option<crate::storage::data_root_designation::Designation>,
 _family:Arc<crate::storage::native_family_fence::Creation>,
}
struct HistoricalFreshInventory{root:Root,stamp:PhysicalStamp,directories:std::collections::BTreeMap<String,(File,PhysicalStamp)>,files:std::collections::BTreeMap<String,HistoricalFreshLeaf>}
impl HistoricalFreshInventory{
 fn capture(active:&Active)->io::Result<Self>{
  let main=active.sqlite_main.as_ref().ok_or_else(||fail("Historical actual activation main absent"))?.clone();
  let root=Root::open(&active.root.absolute)?;
  if identity(root.fd())?!=identity(active.root.fd())?{return Err(fail("Historical actual activated root differs"));}
  Self::capture_original_descriptors(root,main)
 }
 // Only original activated ownership or authenticated parent kernel receipt calls this.
 // This retains bytes for observation; it does not construct Active or write custody.
 fn capture_original_descriptors(root:Root,main:Arc<File>)->io::Result<Self>{
  let original=identity(main.as_ref())?;
  if qualified_sqlite_path_identity(&root)?!=original{return Err(fail("Historical actual original main differs"));}
  let mut tree=Self{stamp:physical_stamp(root.fd())?,root,directories:Default::default(),files:Default::default()};
  fn walk(fd:&File,base:&str,main:&Arc<File>,main_id:Identity,dirs:&mut std::collections::BTreeMap<String,(File,PhysicalStamp)>,files:&mut std::collections::BTreeMap<String,HistoricalFreshLeaf>,depth:usize)->io::Result<()>{
   if depth>32||files.len()+dirs.len()>16384{return Err(fail("Historical fresh complete census bound exceeded"));}let before=physical_stamp(fd)?;
   for name in prior_capsule_names(fd)?{
    if !valid_relative(&name)||name.contains(['/', '\\']){return Err(fail("Historical fresh unsafe name"));}let path=if base.is_empty(){name.clone()}else{format!("{base}/{name}")};let mut st:libc::stat=unsafe{std::mem::zeroed()};let n=CString::new(name.as_str()).map_err(|_|fail("Historical fresh NUL name"))?;
    if unsafe{libc::fstatat(fd.as_raw_fd(),n.as_ptr(),&mut st,libc::AT_SYMLINK_NOFOLLOW)}<0{return Err(io::Error::last_os_error());}
    if(st.st_dev as u64,st.st_ino as u64)==main_id{
     if path!="starlane-notes.db"||st.st_mode&libc::S_IFMT!=libc::S_IFREG{return Err(fail("Historical actual original main aliased"));}
     let stamp=physical_stamp(main.as_ref())?;let bytes=planned_hash(main.as_ref())?;files.insert(path,HistoricalFreshLeaf::OriginalMain{held:main.clone(),stamp,bytes});continue;
    }
    let held=open_at(fd,&name,libc::O_RDONLY|libc::O_NONBLOCK)?;let stamp=physical_stamp(&held)?;
    if identity(&held)?!=(st.st_dev as u64,st.st_ino as u64){return Err(fail("Historical fresh name changed during retention"));}
    if held.metadata()?.is_dir(){if stamp.mode&0o777!=0o700{return Err(fail("Historical fresh observational v1 directory not private"));}walk(&held,&path,main,main_id,dirs,files,depth+1)?;dirs.insert(path,(held,stamp));}
    else if held.metadata()?.is_file(){let bytes=planned_hash(&held)?;files.insert(path,HistoricalFreshLeaf::Owned(PlannedLeaf{held,stamp,bytes}));}
    else{return Err(fail("Historical fresh special file refused"));}
   }if physical_stamp(fd)?!=before{return Err(fail("Historical fresh directory changed during capture"));}Ok(())
  }
  walk(tree.root.fd(),"",&main,original,&mut tree.directories,&mut tree.files,0)?;tree.fence()?;Ok(tree)
 }
 fn fence(&self)->io::Result<()>{historical_retained_tree_fence(&self.root,&self.stamp,&self.directories,&self.files)}
 fn directory(&self,path:Option<String>)->io::Result<&File>{match path{None=>Ok(self.root.fd()),Some(p)=>self.directories.get(&p).map(|(f,_)|f).ok_or_else(||fail("Historical fresh typed directory absent"))}}
 fn file(&self,path:&str)->io::Result<&File>{self.files.get(path).map(HistoricalLeafDescriptor::held).ok_or_else(||fail("Historical fresh typed file absent"))}
}
impl<'a> HistoricalFinalizedObservation<'a>{
 async fn hold_sqlite_in(active:Arc<Active>,exclusion:&'a SqliteCaptureCommitLease,conn:&mut sqlx::AnyConnection)->io::Result<Self>{
  Self::hold_pager_in(active,HistoricalPhysicalPager::OriginalSource512(exclusion),conn).await
 }
 async fn hold_readonly_in(state:Arc<HistoricalReadonlyReaderState>,conn:&mut sqlx::AnyConnection)->io::Result<Self>{
  state.check()?;let active=state.authority.active()?;Self::hold_pager_in(active,HistoricalPhysicalPager::ActualReadonly511(state),conn).await
 }
 async fn hold_pager_in(active:Arc<Active>,pager:HistoricalPhysicalPager<'a>,conn:&mut sqlx::AnyConnection)->io::Result<Self>{
  use crate::storage::attachment_upload::admission::committed_erasure::accounting::capture as capture;
  if active.engine.is_some()||active.ledger.version!=5||!active.version2{return Err(fail("Historical fresh observation requires actual activated SQLite Ledger5"));}
  let roles=active.ledger.historical_preservation.clone().ok_or_else(||fail("Historical actual activated roles absent"))?;
  roles.validate(&admission::Identity{device:active.ledger.root_device,inode:active.ledger.root_inode})?;
  pager.check(&active)?;pager.connection_in(&active,conn).await?;
  let history=history_rows(conn).await?;
  let closure=history_closure(HistorySource::new(&active.root,None)?,identity(active.root.fd())?,Some((&history.0,&history.1)))?;
  if closure.current_id!=active.ledger.generation_id||digest(&closure.generations[&closure.current_id].raw)!=active.ledger_sha256{return Err(fail("Historical fresh original generation differs"));}
  let current=&closure.generations[&closure.current_id];let envelope=current.envelope.as_ref().ok_or_else(||fail("Historical fresh finalized envelope absent"))?;
  if envelope.version!=4||current.ledger.historical_preservation.as_ref()!=Some(&roles)||envelope.parent.as_ref().is_none_or(|p|p.generation_id!=roles.source.prior.generation_id||p.ledger_sha256!=roles.source.prior.ledger_sha256){return Err(fail("Historical fresh exact authenticated predecessor differs"));}
  let old:Vec<HistorySqlRow>=history.0.iter().filter(|r|r.generation_id!=active.ledger.generation_id).cloned().collect();
  if old.len()+1!=history.0.len()||old.len()+1!=closure.generations.len()||!old.iter().any(|r|r.generation_id==roles.source.prior.generation_id&&r.ledger_sha256==roles.source.prior.ledger_sha256){return Err(fail("Historical fresh full old row cardinality differs"));}
  closure.source.fence()?;
  let family=Arc::new(crate::storage::native_family_fence::Creation::exclusive(&active.root.absolute)?);
  let tree=HistoricalFreshInventory::capture(&active)?;let mut receipts=std::collections::BTreeMap::new();let mut records=Vec::new();let mut attempts=std::collections::BTreeSet::new();let mut stages=std::collections::BTreeSet::new();
  for scope in &roles.completed.material.scopes{
   let attempt=&scope.original.reservation.operation_id;if !attempts.insert(attempt.clone()){return Err(fail("Historical fresh duplicate completed attempt"));}stages.insert(attempt.clone());
   for b in &scope.directories{let held=tree.directory(b.role.relative(attempt).map_err(io::Error::other)?)?;if identity(held)?!=(b.adopted.device,b.adopted.inode){return Err(fail("Historical fresh completed directory substituted"));}}
   let capsule=tree.directory(Some(format!(".attachment-upload-admission/{attempt}")))?;if unsafe{libc::flock(capsule.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}<0{return Err(io::Error::last_os_error());}
   let mut raw=std::collections::BTreeMap::new();for b in &scope.files{let original=scope.original.files.iter().find(|s|s.role==b.role).ok_or_else(||fail("Historical fresh completed original role absent"))?;let held=tree.file(&b.role.relative(attempt).map_err(io::Error::other)?)?;if identity(held)?!=(b.adopted.device,b.adopted.inode)||held.metadata()?.nlink()!=1||hash_file(held,original.length)?!=original.sha256{return Err(fail("Historical fresh completed metadata replaced"));}let mut bytes=vec![0;original.length as usize];held.read_exact_at(&mut bytes,0)?;raw.insert(b.role.clone(),bytes);}
   let receipt=capture::validate_original_records(&scope.original,&raw).map_err(io::Error::other)?;if receipts.insert(scope.original.disposition["asset_id"].clone(),receipt).is_some(){return Err(fail("Historical fresh completed asset duplicated"));}
  }
  for owner in &roles.native{
   let scope=&owner.material;let attempt=&scope.original.reservation.operation_id;if !attempts.insert(attempt.clone()){return Err(fail("Historical fresh native/old scope overlaps"));}
   for b in &scope.directories{let path=b.role.relative(attempt)?;let held=tree.directory(path)?;if identity(held)?!=(b.adopted.device,b.adopted.inode){return Err(fail("Historical fresh native directory substituted"));}if b.role==composite::DirectoryRole::DirectStageDirectory{stages.insert(attempt.clone());}}
   let capsule=tree.directory(Some(format!(".attachment-upload-admission/{attempt}")))?;if unsafe{libc::flock(capsule.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}<0{return Err(io::Error::last_os_error());}
   let mut raw=std::collections::BTreeMap::new();for b in &scope.files{let original=scope.original.files.iter().find(|s|s.role==b.role).ok_or_else(||fail("Historical fresh native original role absent"))?;let held=tree.file(&b.role.relative(attempt)?)?;if identity(held)?!=(b.adopted.device,b.adopted.inode)||held.metadata()?.nlink()!=1||hash_file(held,original.length)?!=original.sha256{return Err(fail("Historical fresh native metadata replaced"));}let mut bytes=vec![0;original.length as usize];held.read_exact_at(&mut bytes,0)?;raw.insert(b.role.clone(),bytes);}
   let record=crate::storage::attachment_upload::admission::validate_composite_native_records(&scope.original,&raw)?;
   if let Some(pending)=&scope.pending{
    let mut raw=std::collections::BTreeMap::new();for b in &pending.directories{if identity(tree.directory(b.role.relative(attempt)?)?)?!=(b.adopted.device,b.adopted.inode){return Err(fail("Historical fresh pending directory replaced"));}}
    for b in &pending.files{let held=tree.file(&b.role.relative(attempt)?)?;if identity(held)?!=(b.adopted.device,b.adopted.inode)||held.metadata()?.nlink()!=1||hash_file(held,b.length)?!=b.sha256{return Err(fail("Historical fresh pending payload/record replaced"));}let stream=matches!(&b.role,admission::FileRole::DirectStage{name}if name=="payload.part");let bytes=if stream{crate::storage::attachment_upload::admission::verify_captured_direct_payload(held,b.length,&b.sha256,pending.maximum_bytes)?;Vec::new()}else{let mut bytes=vec![0;b.length as usize];held.read_exact_at(&mut bytes,0)?;bytes};raw.insert(b.role.clone(),bytes);}
    crate::storage::attachment_upload::admission::validate_captured_partial_direct(pending,&raw)?;
   }
   for payload in &owner.payloads{let held=tree.file(&payload.role.relative(&scope.original)?)?;if identity(held)?!=(payload.adopted.device,payload.adopted.inode)||hash_file(held,payload.length)?!=payload.sha256{return Err(fail("Historical fresh actual preserved committed payload differs"));}}
   records.push(record);
  }
  let actual=prior_capsule_names(tree.directory(Some(".attachment-upload-admission".into()))?)?;let actual_stages=prior_capsule_names(tree.directory(Some(".attachment-upload-journal".into()))?)?;
  if actual!=attempts||actual_stages!=stages{return Err(fail("Historical fresh exhaustive old/native namespace differs"));}
  let designation=crate::storage::data_root_designation::Designation::capture_held(&active.root.absolute,active.root.fd())?;
  let original_history=(old,vec![(1,roles.source.prior.generation_id.clone())]);
  let post_projection=engine_transfer::historical_application_projection_in(conn,None).await.map_err(|_|fail("Historical current complete observed projection unavailable"))?;
  let proof=Self{active,pager,roles,tree,history,original_history,post_projection,records,receipts,designation,_family:family};proof.recheck_in(conn).await?;Ok(proof)
 }
 fn fence_physical(&self)->io::Result<()>{
  self.pager.check(&self.active)?;
  self.tree.fence()?;if let Some(d)=&self.designation{d.fence()?;}Ok(())
 }
 async fn connection_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()>{self.pager.connection_in(&self.active,conn).await?;if qualified_sqlite_path_identity(&self.active.root)?!=identity(self.active.sqlite_main.as_ref().ok_or_else(||fail("Historical original activated main absent"))?.as_ref())?{return Err(fail("Historical fresh connected original main replaced"));}Ok(())}
 async fn recheck_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()>{
  use crate::storage::attachment_upload::admission::committed_erasure::accounting::capture as capture;
  self.fence_physical()?;self.connection_in(conn).await?;if history_rows(conn).await?!=self.history{return Err(fail("Historical exact old/new current rows changed"));}
  let original=engine_transfer::historical_original_projection_for_finalized_in(conn,self).await.map_err(|_|fail("Historical exact full original application projection cannot be reconstructed"))?;
  if original!=self.roles.source.application_projection.sha256{return Err(fail("Historical original full opaque application bytes differ"));}
  if engine_transfer::historical_application_projection_in(conn,None).await.map_err(|_|fail("Historical actual current projection unavailable"))?!=self.post_projection{return Err(fail("Historical complete actual current projection changed"));}
  let completed=capture::CompletedAbsenceEnvelope{version:1,source_kind:"original_native_direct_v1".into(),scopes:self.roles.completed.material.scopes.iter().map(|s|s.original.clone()).collect()};capture::check_completed_sql(&completed,&self.receipts,conn).await.map_err(|_|fail("Historical fresh complete immutable completed SQL differs"))?;
  let paths=self.tree.files.keys().cloned().collect();for record in &self.records{record.validate_snapshot_row_and_paths_in(conn,&paths).await.map_err(|_|fail("Historical fresh exact9 native row/winner/paths differ"))?;for(path,file)in &self.tree.files{if path!="starlane-notes.db"{record.exclude_disposed_payload_file(path,file.held(),self.designation.as_ref())?;}}}
  for receipt in self.receipts.values(){if self.tree.files.contains_key(&receipt.intent.storage_path)||self.tree.files.contains_key(&format!(".attachment-upload-journal/{}/payload.part",receipt.intent.attempt_id)){return Err(fail("Historical fresh completed erased payload recreated"));}let n:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachments WHERE id=$1").bind(&receipt.intent.asset_id).fetch_one(&mut *conn).await.map_err(|_|fail("Historical fresh association census unavailable"))?;if n!=0{return Err(fail("Historical fresh erased asset acquired association"));}}
  capture::exclude_completed_payload_inventory(&completed,self.tree.files.iter().filter(|(p,_)|p.as_str()!="starlane-notes.db").map(|(path,file)|{let stamp=file.stamp();(stamp.device,stamp.inode,stamp.length,file.bytes().sha256.as_str(),self.designation.as_ref().map(|d|(d,path.as_str(),file.held())))})).map_err(|_|fail("Historical fresh erased original inode/content resurrected"))?;
  let all:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id LIMIT 513").fetch_all(&mut *conn).await.map_err(|_|fail("Historical fresh all reservation rows unavailable"))?;let expected=self.roles.completed.material.scopes.iter().map(|s|s.original.reservation.operation_id.clone()).chain(self.roles.native.iter().map(|s|s.material.original.reservation.operation_id.clone())).collect::<std::collections::BTreeSet<_>>();if all.len()!=expected.len()||all.iter().any(|id|!expected.contains(id)){return Err(fail("Historical fresh complete reservation census differs"));}
  if history_rows(conn).await?!=self.history{return Err(fail("Historical actual generation changed during proof"));}self.fence_physical()
 }
 fn summary(&self)->io::Result<HistoricalPhysicalSummary>{self.fence_physical()?;let mut s=HistoricalPhysicalSummary{completed_scopes:self.roles.completed.material.scopes.len(),new_committed:0,new_pending:0,new_disposed:0,retained_pending_maximum:0,reference_semantics:historical::HistoricalReferenceSemantics::NotEstablished};for scope in &self.roles.native{match &scope.material.original.class{composite::NativeClass::Reserved{original_scope:Some(p)}=>{s.new_pending+=1;s.retained_pending_maximum=s.retained_pending_maximum.checked_add(p.maximum_bytes).ok_or_else(||fail("Historical fresh pending total overflow"))?;},composite::NativeClass::CommittedDirect{..}=>s.new_committed+=1,composite::NativeClass::DisposedDirect{..}=>s.new_disposed+=1,_=>return Err(fail("Historical fresh unsupported class")),}}Ok(s)}
}
// Bounded fresh inspection only; it returns no live owner after releasing the
// actual source exclusion. Normal app/accounting/PG observation stay separate.
pub(crate) async fn inspect_finalized_historical_sqlite(path:&Path,writers_stopped:bool)->io::Result<HistoricalPhysicalSummary>{
 #[cfg(test)]{inspect_finalized_historical_sqlite_inner(path,writers_stopped,HistoricalInspectionPurpose::Physical,None).await.map(|v|v.physical)}
 #[cfg(not(test))]{inspect_finalized_historical_sqlite_inner(path,writers_stopped,HistoricalInspectionPurpose::Physical).await.map(|v|v.physical)}
}
#[cfg(test)]pub(crate) async fn inspect_finalized_historical_foreign_connection_for_test(path:&Path,foreign:&Path)->io::Result<HistoricalPhysicalSummary>{inspect_finalized_historical_sqlite_inner(path,true,HistoricalInspectionPurpose::Physical,Some(foreign)).await.map(|v|v.physical)}
async fn inspect_finalized_historical_sqlite_inner(path:&Path,writers_stopped:bool,purpose:HistoricalInspectionPurpose,#[cfg(test)] foreign:Option<&Path>)->io::Result<HistoricalInspectedFacts>{
 use sqlx::{Acquire,Connection};
 if !writers_stopped{return Err(fail("Historical fresh inspection requires stopped writers"));}
 let(root,active)=registered(path)?;let active=active.ok_or_else(||fail("Historical fresh source lacks actual registered activation"))?;
 if active.engine.is_some(){return Err(fail("Historical fresh SQLite inspection cannot admit another engine"));}
 let original_main=active.sqlite_main.as_ref().ok_or_else(||fail("Historical fresh original activated main absent"))?;let expected=physical_stamp(original_main.as_ref())?;
 let database=open_at(root.fd(),"starlane-notes.db",libc::O_RDWR)?;if physical_stamp(&database)?!=expected{return Err(fail("Historical original fresh pager inode/stamp differs"));}
 let transition=database.try_clone()?; // prepared BEFORE the genuine SQL reservation
 let url=format!("sqlite://{}?mode=rw",root.absolute.join("starlane-notes.db").display());let pool=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&url).await.map_err(|_|fail("Historical original fresh SQL unavailable"))?;let mut conn=pool.acquire().await.map_err(|_|fail("Historical fresh SQL connection unavailable"))?;
 sqlx::query("PRAGMA busy_timeout=0").execute(&mut *conn).await.map_err(|_|fail("Historical fresh timeout policy unavailable"))?;let mode:String=sqlx::query_scalar("PRAGMA locking_mode=EXCLUSIVE").fetch_one(&mut *conn).await.map_err(|_|fail("Historical fresh cache exclusion unavailable"))?;if mode!="exclusive"{return Err(fail("Historical fresh cache is not exclusive"));}
 let journal:String=sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&mut *conn).await.map_err(|_|fail("Historical fresh journal unavailable"))?;if journal!="delete"{return Err(fail("Historical fresh v1 initial-target inspection supports DELETE only"));}
 let mut tx=conn.begin_with("BEGIN EXCLUSIVE").await.map_err(|_|fail("Historical original fresh source exclusion unavailable"))?;
 if physical_stamp(&database)?!=expected||qualified_sqlite_path_identity(&root)?!=(expected.device,expected.inode){return Err(fail("Historical fresh source changed before independent exclusion"));}
 // Same original Source1 handoff: release process-owned POSIX locks only
 // after the exclusive pager cache is established, then nonblocking full OFD
 // acquisition and strict original stamp checks with no intervening await.
 drop(transition);
 let exclusion=SqliteCaptureCommitLease::hold(&database,0x40000000,512)?;
 if physical_stamp(&database)?!=expected||qualified_sqlite_path_identity(&root)?!=(expected.device,expected.inode){return Err(fail("Historical fresh original changed during independent exclusion handoff"));}
 #[cfg(test)]let proof=if let Some(path)=foreign{let mut other=sqlx::AnyConnection::connect(&format!("sqlite://{}?mode=ro",path.display())).await.map_err(|_|fail("Owned foreign test connection unavailable"))?;let result=HistoricalFinalizedObservation::hold_sqlite_in(active,&exclusion,&mut other).await;other.close().await.map_err(|_|fail("Owned foreign test connection close failed"))?;result?}else{HistoricalFinalizedObservation::hold_sqlite_in(active,&exclusion,&mut tx).await?};
 #[cfg(not(test))]let proof=HistoricalFinalizedObservation::hold_sqlite_in(active,&exclusion,&mut tx).await?;
 proof.recheck_in(&mut tx).await?;let physical=proof.summary()?;
 let conservative_usage=match purpose{HistoricalInspectionPurpose::Physical=>None,HistoricalInspectionPurpose::ConservativeUsage=>{
  let usage=HistoricalUsageObservation::hold_in(&proof,&mut tx).await?;
  #[cfg(test)]historical_usage_foreign_writer_blocked(&root.absolute)?;
  usage.recheck_in(&mut tx).await?;Some(usage.facts.clone())
 }};
 proof.recheck_in(&mut tx).await?;
 tx.rollback().await.map_err(|_|fail("Historical fresh source read rollback failed"))?;drop(conn);pool.close().await;proof.fence_physical()?;drop(proof);drop(exclusion);Ok(HistoricalInspectedFacts{physical,conservative_usage})
}
