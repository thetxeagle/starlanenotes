// Qualified cold native partial-direct admission roles. Snapshot + destination tokens are private.
// Capture evidence cannot mint generic owns or disposal authority.
fn pg_certified_admission_roles(snapshot:&InspectedPgSnapshot,d:&OwnedPgBackup,replay:Option<&PgValidatedReplayCapture>)->anyhow::Result<Option<crate::storage::restoration::admission::AdmissionOwnerEnvelope>> {
 if snapshot.manifest.admission_owner_capture.is_none()&&snapshot.manifest.prior_admission_owner_capture.is_none(){return Ok(None)}
 snapshot.held.fence()?;d.fence()?;
 ensure!(snapshot.profile.has_capacity()&&snapshot.manifest.format_version>=3,"Admission requires qualified capacity snapshot");
 let scopes=if let Some(capture)=&snapshot.manifest.admission_owner_capture {capture.validate_native_reserved()?;if capture.scopes.is_empty(){return Ok(None)}capture.native_roles()?}else{pg_prior_admission_roles(snapshot)?};
 pg_certified_admission_scope_set(snapshot,d,replay,scopes)
}
fn pg_certified_admission_scope_set(snapshot:&InspectedPgSnapshot,d:&OwnedPgBackup,replay:Option<&PgValidatedReplayCapture>,mut scopes:crate::storage::restoration::admission::AdmissionOwnerEnvelope)->anyhow::Result<Option<crate::storage::restoration::admission::AdmissionOwnerEnvelope>>{
 use crate::storage::restoration::admission::{DirectoryRole,Identity as AdmissionIdentity,ReservationState};snapshot.held.fence()?;d.fence()?;
 let source_scopes_sha256=digest(&serde_json::to_vec(&scopes)?);
 let table=snapshot.tables.get("attachment_capacity_reservations").context("Missing captured admission reservation table")?;
 let column=|name:&str|->anyhow::Result<usize>{table.columns.iter().position(|c|c.name==name).context("Missing captured admission reservation column")};
 let key=column("operation_id")?;let request=column("request_sha256")?;let claim=column("admission_claim_sha256")?;let maximum=column("maximum_bytes")?;let state=column("state")?;let optional=[column("committed_asset_id")?,column("committed_bytes")?,column("disposition_json")?,column("disposition_sha256")?];
 let actual=table.rows.iter().map(|row|text(&row[key]).map(str::to_owned)).collect::<anyhow::Result<std::collections::BTreeSet<_>>>()?;
 let expected=scopes.scopes.iter().map(|s|s.attempt_id.clone()).collect::<std::collections::BTreeSet<_>>();ensure!(expected.is_subset(&actual)&&actual.len()==table.rows.len(),"Captured admission SQL role subset/inventory differs");
 for scope in &mut scopes.scopes {
  ensure!(matches!(scope.reservation,ReservationState::Reserved),"Cold admission terminal requires separate proof");
  let row=table.rows.iter().find(|r|text(&r[key]).is_ok_and(|id|id==scope.attempt_id)).context("Missing captured admission row")?;
  ensure!(text(&row[request])?==scope.request_sha256&&text(&row[claim])?==scope.admission_claim_sha256&&matches!(row[maximum],Cell::Integer(n) if n==scope.maximum_bytes as i64)&&text(&row[state])?=="reserved"&&optional.iter().all(|i|matches!(row[*i],Cell::Null)),"Captured admission full SQL claim/state differs");
  let receipts=snapshot.tables.get("attachment_upload_receipts").context("Missing captured upload receipts")?;let attempt_column=receipts.columns.iter().position(|c|c.name=="attempt_id").context("Missing captured winner attempt column")?;ensure!(!receipts.rows.iter().any(|r|text(&r[attempt_column]).is_ok_and(|id|id==scope.attempt_id)),"Captured reserved stage already has winning receipt");
  let claim:serde_json::Value=serde_json::from_str(&scope.original_claim_json)?;let producer:crate::storage::attachment_upload::admission::Producer=serde_json::from_value(claim.get("producer").context("Missing captured producer")?.clone())?;let direct=matches!(producer,crate::storage::attachment_upload::admission::Producer::Direct{..});
  let mut raws=BTreeMap::new();
  let owned=scope.files.iter().filter(|b|!matches!(b.role,crate::storage::restoration::admission::FileRole::ReplayBlob{..})).map(|b|b.role.relative(&scope.attempt_id)).collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
  let capsule_prefix=format!("{}/{}/",scope.namespace,scope.attempt_id);let stage_prefix=format!(".attachment-upload-journal/{}/",scope.attempt_id);
  let rewrite_prefixes=scope.directories.iter().filter_map(|b|if let DirectoryRole::CanonicalRewriteDirectory{operation_id}=&b.role{Some(format!(".canonical-rewrite-journal/{operation_id}/"))}else{None}).collect::<Vec<_>>();
  let observed=snapshot.manifest.files.0.iter().filter(|(p,e)|matches!(e,PgSnapshotEntry::File{..})&&(p.starts_with(&capsule_prefix)||p.starts_with(&stage_prefix)||rewrite_prefixes.iter().any(|prefix|p.starts_with(prefix)))).map(|(p,_)|p.clone()).collect::<std::collections::BTreeSet<_>>();ensure!(owned==observed,"Captured admission directory inventory omitted record");
  for b in &mut scope.files {
   let path=b.role.relative(&scope.attempt_id)?;
   let Some(PgSnapshotEntry::File{device,inode,length,sha256,..})=snapshot.manifest.files.0.get(&path)else{bail!("Missing captured admission file")};
   let stream_direct=direct&&matches!(&b.role,crate::storage::restoration::admission::FileRole::DirectStage{name} if name=="payload.part");
   ensure!(b.captured==(AdmissionIdentity{device:*device,inode:*inode})&&b.length==*length&&b.sha256==*sha256&&(stream_direct||b.length<=64*1024*1024),"Captured admission original file differs");
   let captured=snapshot.held.roles.get(&format!("files/{path}")).context("Held captured admission file absent")?;
   if stream_direct {crate::storage::attachment_upload::admission::verify_captured_direct_payload(&captured.held,b.length,&b.sha256,scope.maximum_bytes)?;raws.insert(b.role.clone(),Vec::new());}
   else {let mut raw=vec![0;b.length as usize];let mut offset=0;while offset<raw.len(){let n=captured.held.read_at(&mut raw[offset..],offset as u64)?;ensure!(n>0,"Captured admission truncated");offset+=n;}ensure!(digest(&raw)==b.sha256,"Captured admission bytes changed");raws.insert(b.role.clone(),raw);}
   let dest=d.roles.get(&path).context("Certified admission destination leaf absent")?;ensure!(dest.bytes==captured.bytes&&physical_stamp(&dest.held)?==dest.stamp,"Certified admission destination leaf differs");b.adopted=AdmissionIdentity{device:dest.stamp.device,inode:dest.stamp.inode};
  }
  match producer {
   crate::storage::attachment_upload::admission::Producer::Direct{..}=>crate::storage::attachment_upload::admission::validate_captured_partial_direct(scope,&raws)?,
   crate::storage::attachment_upload::admission::Producer::Replay{..}=>{
    let token=replay.context("Captured Replay requires exact private imported SQL/reference proof")?;
    ensure!(token.manifest_sha256==digest(&snapshot.held.manifest)&&token.scopes_sha256==source_scopes_sha256,"Captured Replay private proof differs from exact inspected snapshot");
    crate::storage::attachment_upload::admission::validate_captured_replay_custody(scope,&raws)?;
   }
  }
  for b in &mut scope.directories {
   if let Some(path)=b.role.relative(&scope.attempt_id)? {
    let Some(PgSnapshotEntry::Directory{device,inode})=snapshot.manifest.files.0.get(&path)else{bail!("Missing captured admission directory")};ensure!(b.captured==(AdmissionIdentity{device:*device,inode:*inode}),"Captured admission directory differs");let (held,stamp)=d.directories.get(&path).context("Certified admission destination directory absent")?;ensure!(physical_stamp(held)?==*stamp,"Certified admission destination directory changed");b.adopted=AdmissionIdentity{device:stamp.device,inode:stamp.inode};
   }else{ensure!(b.role==DirectoryRole::DataRoot&&b.captured==(AdmissionIdentity{device:snapshot.manifest.source_root.device,inode:snapshot.manifest.source_root.inode}),"Captured admission original root differs");b.adopted=AdmissionIdentity{device:d.root_identity.0,inode:d.root_identity.1};}
  }
 }
 scopes.validate(&AdmissionIdentity{device:d.root_identity.0,inode:d.root_identity.1})?;snapshot.held.fence()?;d.fence()?;Ok(Some(scopes))
}

// Only an inspected snapshot token supplies predecessor authority; caller JSON cannot.
fn pg_prior_admission_roles(snapshot:&InspectedPgSnapshot)->anyhow::Result<crate::storage::restoration::admission::AdmissionOwnerEnvelope> {
 use crate::storage::restoration::admission::{AdmissionCaptureScope,CaptureDirectoryBinding,CaptureFileBinding,ReservationState};
 let evidence=snapshot.manifest.prior_admission_owner_capture.as_ref().context("Missing prior capture")?;evidence.validate_shape()?;
 ensure!(snapshot.profile.has_capacity()&&snapshot.manifest.format_version==4&&matches!(snapshot.manifest.source_authority,Some(PgCapturedSourceAuthority::PgRestore{..})),"Prior admission requires capacity restored-source snapshot");
 ensure!(evidence.source_root.device==snapshot.manifest.source_root.device&&evidence.source_root.inode==snapshot.manifest.source_root.inode&&evidence.generation_id==snapshot.history.current_id,"Prior captured source root/current generation differs");
 let raw=snapshot.history.raw_generations.get(&snapshot.history.current_id).context("Missing authenticated prior finalized history")?;ensure!(digest(raw)==evidence.ledger_sha256,"Prior finalized ledger digest differs");let old:HistoryEnvelope=serde_json::from_slice(raw)?;ensure!(old.version==3&&old.runtime.version==2,"Prior admission requires finalized Ledger2");let original=old.runtime.admission.as_ref().context("Prior finalized admission roles absent")?;
 let mut scopes=Vec::new();let mut seen=std::collections::BTreeSet::new();
 for capture in &evidence.scopes {
  ensure!(seen.insert(capture.attempt_id.clone()),"Duplicate prior capture attempt");let s=original.scopes.iter().find(|s|s.attempt_id==capture.attempt_id).context("Prior captured scope absent from finalized generation")?;
  let expected=AdmissionCaptureScope{namespace:s.namespace.clone(),attempt_id:s.attempt_id.clone(),original_claim_json:s.original_claim_json.clone(),admission_claim_sha256:s.admission_claim_sha256.clone(),request_sha256:s.request_sha256.clone(),maximum_bytes:s.maximum_bytes,reservation:s.reservation.clone(),directories:s.directories.iter().map(|b|CaptureDirectoryBinding{role:b.role.clone(),original:b.original.clone(),captured:b.adopted.clone()}).collect(),files:s.files.iter().map(|b|CaptureFileBinding{role:b.role.clone(),original:b.original.clone(),captured:b.adopted.clone(),length:b.length,sha256:b.sha256.clone()}).collect()};
  ensure!(*capture==expected&&matches!(s.reservation,ReservationState::Reserved),"Prior capture differs from exact finalized original/adopted scope");let mut s=s.clone();for b in &mut s.directories{b.captured=b.adopted.clone();}for b in &mut s.files{b.captured=b.adopted.clone();}scopes.push(s);
 }
 let envelope=crate::storage::restoration::admission::AdmissionOwnerEnvelope{version:2,scopes};envelope.validate(&evidence.source_root)?;snapshot.held.fence()?;Ok(envelope)
}

// Private token minted only in exact imported target's held read-only transaction.
// No caller JSON or captured checksum alone constructs this proof.
struct PgValidatedReplayCapture {manifest_sha256:String,scopes_sha256:String}
async fn pg_validate_replay_capture(snapshot:&InspectedPgSnapshot,tx:&mut sqlx::Transaction<'_,sqlx::Any>)->anyhow::Result<Option<PgValidatedReplayCapture>> {
 use crate::storage::attachment_upload::admission::{Producer,validate_captured_replay_custody};
 if snapshot.manifest.admission_owner_capture.is_none()&&snapshot.manifest.prior_admission_owner_capture.is_none(){return Ok(None)}
 snapshot.held.fence()?;
 let scopes=if let Some(c)=&snapshot.manifest.admission_owner_capture{c.validate_native_reserved()?;c.native_roles()?}else{pg_prior_admission_roles(snapshot)?};
 let inventory=snapshot.manifest.files.0.iter().filter(|(_,e)|matches!(e,PgSnapshotEntry::File{..})).map(|(p,_)|p.clone()).collect::<std::collections::BTreeSet<_>>();
 let read_bound=|path:&str,bound:u64|->std::io::Result<Vec<u8>> {
  let Some(PgSnapshotEntry::File{length,sha256,..})=snapshot.manifest.files.0.get(path)else{return Err(fail("Captured Replay reference absent"))};
  if *length>bound{return Err(fail("Captured Replay bounded proof exceeded"))}
  let held=snapshot.held.roles.get(&format!("files/{path}")).ok_or_else(||fail("Captured Replay held reference absent"))?;
  let mut raw=vec![0;*length as usize];let mut offset=0;while offset<raw.len(){let n=held.held.read_at(&mut raw[offset..],offset as u64)?;if n==0{return Err(fail("Captured Replay reference truncated"))}offset+=n;}
  if digest(&raw)!=*sha256{return Err(fail("Captured Replay reference bytes changed"))}Ok(raw)
 };
 let read=|path:&str|read_bound(path,6_000_000);
 let mut any=false;
 for scope in &scopes.scopes {
  let claim:serde_json::Value=serde_json::from_str(&scope.original_claim_json)?;let producer:Producer=serde_json::from_value(claim.get("producer").context("Missing Replay producer")?.clone())?;
  if matches!(producer,Producer::Replay{..}){
   ensure!(snapshot.profile.has_capacity()&&snapshot.manifest.format_version>=3,"Replay proof requires qualified capacity snapshot");
   let mut raws=BTreeMap::new();for b in &scope.files{let path=b.role.relative(&scope.attempt_id)?;let raw=read_bound(&path,64*1024*1024)?;ensure!(raw.len()as u64==b.length&&digest(&raw)==b.sha256,"Replay captured role bytes differ");raws.insert(b.role.clone(),raw);}
   let proof=validate_captured_replay_custody(scope,&raws)?;proof.validate_references(&mut **tx,&inventory,&read).await?;any=true;
  }
 }
 snapshot.held.fence()?;Ok(any.then(||PgValidatedReplayCapture{manifest_sha256:digest(&snapshot.held.manifest),scopes_sha256:digest(&serde_json::to_vec(&scopes).expect("Typed captured scopes serialize"))}))
}
