
#[derive(PartialEq,Eq)]
struct OriginalCopiedClaimInventory {
 root:serde_json::Value,directories:BTreeMap<String,serde_json::Value>,
 files:BTreeMap<String,(serde_json::Value,u64,String)>,
}
#[derive(PartialEq,Eq)]
enum OriginalCopiedClaimManifest {Postgres(Vec<u8>,OriginalCopiedClaimInventory),TransferSqlite(Vec<u8>,OriginalCopiedClaimInventory)}
impl OriginalCopiedClaimManifest {
 fn postgres(raw:Vec<u8>)->anyhow::Result<Self>{
  let m:PgSnapshotManifest=serde_json::from_slice(&raw)?;let mut directories=BTreeMap::new();let mut files=BTreeMap::new();
  for(path,entry)in m.files.0{match entry{PgSnapshotEntry::Directory{device,inode}=>{directories.insert(path,serde_json::json!({"device":device,"inode":inode}));},PgSnapshotEntry::File{device,inode,length,sha256,..}=>{files.insert(path,(serde_json::json!({"device":device,"inode":inode}),length,sha256));}}}
  Ok(Self::Postgres(raw,OriginalCopiedClaimInventory{root:serde_json::json!({"device":m.source_root.device,"inode":m.source_root.inode}),directories,files}))
 }
 fn sqlite(raw:Vec<u8>)->anyhow::Result<Self>{
  let m:crate::storage::backup_inventory::Manifest=serde_json::from_slice(&raw)?;
  let directories=m.source_custody.directories.0.into_iter().map(|(path,id)|Ok((path,serde_json::to_value(id)?))).collect::<anyhow::Result<BTreeMap<_,_>>>()?;
  let mut files=BTreeMap::new();
  for(path,bytes)in m.files.0{
   let Some(id)=m.source_custody.files.0.get(&path)else{
    // The certified database is a generated transport projection, not an
    // original inode. Never manufacture its source identity. Every declared
    // admission record remains mandatory, even if its capsule has other files.
    ensure!(!path.starts_with(".attachment-upload-admission/"),"Original SQLite capsule file custody absent");
    continue
   };
   files.insert(path,(serde_json::json!({"device":id.device,"inode":id.inode}),bytes.size_bytes,bytes.sha256));
  }
  Ok(Self::TransferSqlite(raw,OriginalCopiedClaimInventory{root:serde_json::to_value(m.source_custody.root)?,directories,files}))
 }
 fn inventory(&self)->&OriginalCopiedClaimInventory{match self{Self::Postgres(_,i)|Self::TransferSqlite(_,i)=>i}}
 fn directory(&self,path:&str)->anyhow::Result<serde_json::Value>{Ok(self.inventory().directories.get(path).context("Original copied directory absent")?.clone())}
 fn root(&self)->anyhow::Result<serde_json::Value>{Ok(self.inventory().root.clone())}
 fn payload(&self,path:&str)->anyhow::Result<serde_json::Value>{Ok(self.inventory().files.get(path).context("Original copied payload absent")?.0.clone())}
 fn matches(&self,claims:&BTreeMap<String,Vec<u8>>,read:&impl Fn(&str)->anyhow::Result<Vec<u8>>)->anyhow::Result<bool>{
  let inventory=self.inventory();let mut capsules=BTreeMap::<String,Vec<(&String,&(serde_json::Value,u64,String))>>::new();
  for(path,file)in &inventory.files{if let Some((id,_))=path.strip_prefix(".attachment-upload-admission/").and_then(|s|s.split_once('/')){capsules.entry(id.into()).or_default().push((path,file));}}
  if capsules.keys().ne(claims.keys()){return Ok(false)}
  let namespace=self.directory(".attachment-upload-admission")?;let mut count=0usize;let mut total=0u64;
  for(id,raw)in claims{
   let base=format!(".attachment-upload-admission/{id}");let c:serde_json::Value=serde_json::from_slice(raw)?;
   if c["root"]!=inventory.root||c["namespace"]!=namespace||c["capsule"]!=self.directory(&base)?{return Ok(false)}
   for(path,(_,length,sha))in &capsules[id]{
    count+=1;ensure!(count<=512*32&&*length<=8*1024*1024,"Original copied record count/size exceeds bound");
    total=total.checked_add(*length).context("Original copied record total overflow")?;ensure!(total<=16*1024*1024,"Original copied records exceed bound");
    let bytes=read(path)?;if bytes.len()as u64!=*length||digest(&bytes)!=*sha{return Ok(false)}
   }
  }Ok(true)
 }
}
// Caller supplies only its already authenticated held role map. Each hop binds
// exact source class, target, generation and digest; filenames alone grant nothing.
fn original_copied_claim_manifest(raw:&[u8],claims:&BTreeMap<String,Vec<u8>>,read:&impl Fn(&str)->anyhow::Result<Vec<u8>>)->anyhow::Result<OriginalCopiedClaimManifest>{
 fn walk(raw:&[u8],claims:&BTreeMap<String,Vec<u8>>,read:&impl Fn(&str)->anyhow::Result<Vec<u8>>,visited:&mut std::collections::BTreeSet<String>)->anyhow::Result<OriginalCopiedClaimManifest>{
  ensure!(visited.len()<128,"Original copied provenance exceeds bound");
  let m:PgSnapshotManifest=serde_json::from_slice(raw)?;
  ensure!(visited.insert(m.backup_id.clone()),"Original copied provenance cycle");
  let pg=OriginalCopiedClaimManifest::postgres(raw.to_vec())?;if pg.matches(claims,read)?{return Ok(pg)}
  match m.source_authority{
   None=>{
    let engine_raw=read(".engine-transfer-custody/current.json")?;let engine:EngineCommitEnvelope=serde_json::from_slice(&engine_raw)?;
    ensure!(digest(&engine_raw)==m.engine_envelope_sha256&&engine.transfer_id==m.engine_generation&&engine.target==m.source_target,"Original copied Transfer source class detached");
    let dir=format!(".engine-transfer-custody/{}",engine.transfer_id);let intent_raw=read(&format!("{dir}/intent.json"))?;ensure!(digest(&intent_raw)==engine.seal_intent_sha256,"Original copied Transfer seal detached");
    let intent:EngineSealIntent=serde_json::from_slice(&intent_raw)?;ensure!(intent.transfer_id==engine.transfer_id&&intent.target==engine.target,"Original copied Transfer target differs");
    let binding=intent.roles.get("original-manifest.json").context("Original SQLite manifest role absent")?;let source=read(&format!("{dir}/original-manifest.json"))?;
    ensure!(source.len()as u64==binding.length&&digest(&source)==binding.sha256,"Original SQLite manifest digest detached");
    let origin=OriginalCopiedClaimManifest::sqlite(source)?;ensure!(origin.matches(claims,read)?,"Original SQLite claim/record custody differs");Ok(origin)
   },
   Some(PgCapturedSourceAuthority::PgRestore{generation_id,attestation_sha256})=>{
    pg_snapshot_uuid(&generation_id)?;let archived=format!(".pg-restoration-custody/{generation_id}/source-attestation.json");
    let attestation=match read(&archived){Ok(raw)if digest(&raw)==attestation_sha256=>raw,_=>{let raw=read(".pg-restoration-custody/current.json")?;ensure!(digest(&raw)==attestation_sha256,"Original copied predecessor attestation digest differs");raw}};
    let e:PgTargetAttestation=serde_json::from_slice(&attestation)?;ensure!(e.restore_id==generation_id&&e.target==m.source_target&&(e.root_device,e.root_inode)==(m.source_root.device,m.source_root.inode),"Original copied predecessor identity differs");
    let previous=read(&format!(".pg-restoration-custody/{generation_id}/manifest.json"))?;ensure!(digest(&previous)==e.snapshot_sha256,"Original copied predecessor manifest digest differs");
    walk(&previous,claims,read,visited)
   }
  }
 }
 walk(raw,claims,read,&mut std::collections::BTreeSet::new())
}
// Explicit original restored-PG copied-committed preservation. Never a native
// writer, read, cleanup, current-reference or released-charge authority.
struct OriginalRestoredAdmissionPreservation {
 proof:Arc<RestoredPgAdmission>,native:TargetCommittedPreservationOwner,held:super::FinalizedAdmissionPreservationLeases,
 original_manifest:Vec<u8>,origin_manifest:OriginalCopiedClaimManifest,original_tables:BTreeMap<String,Table>,
}
impl OriginalRestoredAdmissionPreservation {
 async fn select(proof:&Arc<RestoredPgAdmission>,conn:&mut sqlx::AnyConnection)->anyhow::Result<Option<Self>> {
  proof.verify_any_connection(conn).await?;let(_,active)=super::registered(proof.path())?;let active=active.context("Restored preservation lacks original registered activation")?;active.check()?;
  let original=match active.engine.as_deref(){Some(super::EngineAuthority::PgRestore(original))=>original,_=>bail!("Restored preservation requires original activated PG authority")};original.verify_any_connection(conn).await?;
  ensure!(original.path()==proof.path()&&identity(original.root.fd())?==identity(proof.root.fd())?&&original.raw_attestation==proof.raw_attestation&&original.records.keys().eq(proof.records.keys()),"Restored preservation original owner differs");original.physical_fence()?;proof.physical_fence()?;
  if active.ledger.version!=1||active.ledger.admission.is_some()||active.ledger.completed_absence.is_some()||!active.version2{return Ok(None)}
  let archive=format!(".pg-restoration-custody/{}",proof.attestation.restore_id);let record=proof.records.get(&format!("{archive}/manifest.json")).context("Restored preservation actual source manifest absent")?;let raw=read_ledger(&record.held)?;
  ensure!(digest(&raw)==record.bytes.sha256&&digest(&raw)==proof.attestation.snapshot_sha256,"Restored preservation authenticated source manifest differs");let manifest:PgSnapshotManifest=serde_json::from_slice(&raw)?;
  ensure!(manifest.backup_id==proof.attestation.backup_id&&manifest.source_target==proof.attestation.source_target,"Restored preservation source manifest association differs");
  if manifest.admission_owner_capture.is_some()||manifest.prior_admission_owner_capture.is_some()||manifest.completed_absence_capture.is_some()||manifest.prior_completed_absence_capture.is_some()||manifest.mixed_completed_absence_capture.is_some()||manifest.composite_admission_capture.is_some(){return Ok(None)}
  let ids=Self::original_ids(&manifest);if ids.is_empty(){return Ok(None)}ensure!(ids.len()<=512,"Restored preservation original capsule bound exceeded");
  let rows=super::prior_capture_rows(conn).await?;ensure!(rows.iter().map(|r|r.operation_id.clone()).collect::<std::collections::BTreeSet<_>>()==ids&&rows.len()==ids.len(),"Restored preservation complete original/current capsule set differs");ensure!(rows.iter().all(|r|r.state=="committed"),"Restored preservation supports copied committed producers only");
  let projected=proof.records.get(&format!("{archive}/application-rows.json")).context("Restored preservation authenticated application rows absent")?;let bytes=read_ledger(&projected.held)?;ensure!(digest(&bytes)==projected.bytes.sha256&&digest(&bytes)==manifest.application_rows_sha256,"Restored preservation original application projection differs");let tables:BTreeMap<String,Table>=serde_json::from_slice(&bytes)?;
  let native=TargetCommittedPreservationOwner::recover(original)?;ensure!(native.held.inventory.operations.iter().cloned().collect::<std::collections::BTreeSet<_>>()==ids,"Original target-native operation set differs");
  let held=super::FinalizedAdmissionPreservationLeases::hold(proof.path(),conn,&rows).await?;let origin_manifest=Self::original_claim_origin(proof,&ids)?;let token=Self{proof:proof.clone(),native,held,original_manifest:raw,origin_manifest,original_tables:tables};token.recheck(conn).await?;Ok(Some(token))
 }
 fn original_ids(manifest:&PgSnapshotManifest)->std::collections::BTreeSet<String>{manifest.files.0.keys().filter_map(|p|p.strip_prefix(".attachment-upload-admission/").and_then(|p|p.split_once('/')).map(|(id,_)|id.to_owned())).collect()}
 fn original_claim_origin(proof:&RestoredPgAdmission,ids:&std::collections::BTreeSet<String>)->anyhow::Result<OriginalCopiedClaimManifest>{
  pg_restore_archived_authority_chain(&proof.records,&proof.attestation)?;
  let claims=ids.iter().map(|id|Ok((id.clone(),read_ledger(&proof.root.leaf(&format!(".attachment-upload-admission/{id}/claim.json"))?)?))).collect::<anyhow::Result<BTreeMap<_,_>>>()?;
  let path=format!(".pg-restoration-custody/{}/manifest.json",proof.attestation.restore_id);
  let raw=read_ledger(&proof.records.get(&path).context("Current original manifest absent")?.held)?;
  ensure!(digest(&raw)==proof.attestation.snapshot_sha256,"Current original manifest digest differs");
  original_copied_claim_manifest(&raw,&claims,&|path|{if path.starts_with(".attachment-upload-admission/"){Ok(read_ledger(&proof.root.leaf(path)?)?)}else{Ok(read_ledger(&proof.records.get(path).context("Linked original copied role absent")?.held)?)}})
 }
 async fn recheck(&self,conn:&mut sqlx::AnyConnection)->anyhow::Result<()> {
  self.proof.verify_any_connection(conn).await?;self.proof.physical_fence()?;self.native.fence()?;self.held.recheck_in(conn).await?;
  let manifest:PgSnapshotManifest=serde_json::from_slice(&self.original_manifest)?;ensure!(digest(&self.original_manifest)==self.proof.attestation.snapshot_sha256,"Restored preservation manifest identity changed");let origin=&self.origin_manifest;ensure!(Self::original_claim_origin(&self.proof,&Self::original_ids(&manifest))?==self.origin_manifest,"Restored preserved original provenance changed");let ids=Self::original_ids(&manifest);let rows=super::prior_capture_rows(conn).await?;ensure!(rows.len()==ids.len()&&rows.iter().map(|r|r.operation_id.clone()).collect::<std::collections::BTreeSet<_>>()==ids,"Restored preservation complete reservation set changed");
  let root=&self.proof.root;let namespace=root.directory(".attachment-upload-admission")?;ensure!(super::prior_capsule_names(&namespace)?==ids,"Restored preservation namespace/current SQL differs");let mut total=0usize;
  let profile=pg_profile_any(conn).await?;let current=pg_tables_any(conn,&shape_for_profile(profile)?).await?;
  preserved_tables_match(&self.original_tables,&current)?;
  for row in &rows {
   ensure!(row.state=="committed","Restored preserved reservation state changed");let base=format!(".attachment-upload-admission/{}",row.operation_id);let raw=read_ledger(&root.leaf(&format!("{base}/claim.json"))?)?;row.matches_claim(&raw)?;let claim:serde_json::Value=serde_json::from_slice(&raw)?;
   ensure!(claim["root"]==origin.root()?&&claim["namespace"]==origin.directory(".attachment-upload-admission")?&&claim["capsule"]==origin.directory(&base)?,"Restored copied original claim custody differs");
   let producer:crate::storage::attachment_upload::admission::Producer=serde_json::from_value(claim["producer"].clone())?;
   if let Some(replay)=preserved_producer_current(conn,&producer,row,&self.original_tables,&current).await?{
    let path=format!("{base}/replay-blob-origin.json");let raw_origin=read_ledger(&root.leaf(&path)?)?;
    let parent=replay.storage_path.rsplit_once('/').context("Original Replay path invalid")?.0;
    let payload=origin.payload(&replay.storage_path)?;
    preserved_replay_blob(root,&raw_origin,&raw,&replay,&origin.directory(parent)?,&payload)?;
   }
   let expected=manifest.files.0.keys().filter_map(|p|p.strip_prefix(&(base.clone()+"/")).map(str::to_owned)).collect::<std::collections::BTreeSet<_>>();let capsule=root.directory(&base)?;let names=super::prior_capsule_names(&capsule)?;ensure!(names==expected&&names.len()<=32,"Restored preserved original record set differs");
   for name in names {let path=format!("{base}/{name}");let file=root.leaf(&path)?;let stamp=physical_stamp(&file)?;let bytes=read_ledger(&file)?;total=total.checked_add(bytes.len()).context("Restored preserved record total overflow")?;ensure!(total<=16*1024*1024,"Restored preserved records exceed16MiB");let PgSnapshotEntry::File{length,sha256,..}=manifest.files.0.get(&path).context("Original preserved file role absent")? else{bail!("Original preserved record role differs")};ensure!(stamp.mode&0o777==0o600&&stamp.links==1&&file.metadata()?.uid()==unsafe{libc::geteuid()}&&bytes.len()as u64==*length&&digest(&bytes)==*sha256&&physical_stamp(&file)?==stamp,"Restored original preserved record changed");crate::storage::attachment_upload::admission::validate_preserved_capsule_record(&name,&bytes,&raw)?;}
  }
  self.held.recheck_in(conn).await?;self.native.fence()?;self.proof.verify_any_connection(conn).await?;self.proof.physical_fence()?;Ok(())
 }
}

#[cfg(test)]
mod sqlite_copied_claim_projection_controls{
 use super::*;
 fn manifest()->(serde_json::Value,BTreeMap<String,Vec<u8>>){
  let root=serde_json::json!({"device":1,"inode":2});let namespace=serde_json::json!({"device":1,"inode":3});let capsule=serde_json::json!({"device":1,"inode":4});
  let base=".attachment-upload-admission/11111111-1111-4111-8111-111111111111";
  let claim=serde_json::to_vec(&serde_json::json!({"root":root,"namespace":namespace,"capsule":capsule})).unwrap();
  let records=BTreeMap::from([(format!("{base}/claim.json"),claim),(format!("{base}/replay-blob-origin.json"),b"preserved original role bytes".to_vec())]);
  let directories=BTreeMap::from([(".attachment-upload-admission".to_owned(),namespace.clone()),(base.to_owned(),capsule.clone())]);
  let mut files=serde_json::Map::new();let mut custody=serde_json::Map::new();
  files.insert("starlane-notes.db".into(),serde_json::json!({"size_bytes":512,"sha256":"a".repeat(64)}));
  for(index,(path,bytes))in records.iter().enumerate(){files.insert(path.clone(),serde_json::json!({"size_bytes":bytes.len(),"sha256":digest(bytes)}));custody.insert(path.clone(),serde_json::json!({"device":1,"inode":10+index,"links":1}));}
  let blob="spaces/slug/attachments/page/blob.bin";files.insert(blob.into(),serde_json::json!({"size_bytes":4,"sha256":"b".repeat(64)}));custody.insert(blob.into(),serde_json::json!({"device":1,"inode":20,"links":1}));
  (serde_json::json!({"format":"starlane-backup-set","format_version":4,"backup_id":"22222222-2222-4222-8222-222222222222","application_version":"fixture","capture_started_at":"start","capture_finished_at":"end","writers_stopped_asserted":true,"database":{"engine":"sqlite","applied_version":55,"user_version":0,"migrations":[]},"files":files,"excluded":[],"configuration_prerequisite":"fixture","recovery_boundary":{"scope":"fixture","ordinary_restore":"fixture","journal_namespaces":[],"journal_directories":[],"receipt_families":[]},"source_custody":{"version":1,"root":root,"directories":directories,"files":custody}}),records)
 }
 #[test]fn generated_sqlite_projection_is_not_an_original_inode_and_all_capsule_roles_remain_mandatory()->anyhow::Result<()>{
  let(value,records)=manifest();let origin=OriginalCopiedClaimManifest::sqlite(serde_json::to_vec(&value)?)?;
  ensure!(origin.payload("starlane-notes.db").is_err(),"Generated projection acquired a fabricated original inode");
  let claim_path=".attachment-upload-admission/11111111-1111-4111-8111-111111111111/claim.json";
  let claims=BTreeMap::from([("11111111-1111-4111-8111-111111111111".into(),records[claim_path].clone())]);
  ensure!(origin.matches(&claims,&|path|records.get(path).cloned().context("Actual selected original role absent"))?,"Generated DB prevented complete genuine-source role matching");
  for path in records.keys(){let mut changed=value.clone();changed["source_custody"]["files"].as_object_mut().unwrap().remove(path);ensure!(OriginalCopiedClaimManifest::sqlite(serde_json::to_vec(&changed)?).is_err(),"Missing declared capsule custody was dropped from complete original role set");}
  let mut changed=value;let blob="spaces/slug/attachments/page/blob.bin";changed["source_custody"]["files"].as_object_mut().unwrap().remove(blob);
  let origin=OriginalCopiedClaimManifest::sqlite(serde_json::to_vec(&changed)?)?;ensure!(origin.payload(blob).is_err(),"Required Replay blob lookup fabricated absent source custody");
  Ok(())
 }
}
