// Read-only candidate inspection. This type cannot grant restore or serving admission.
use crate::storage::backup_inventory::UniqueMap as PgUniqueMap;
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PgSnapshotRoot {device:u64,inode:u64}
#[derive(serde::Deserialize)]
#[serde(tag="kind",rename_all="lowercase",deny_unknown_fields)]
enum PgSnapshotEntry {
 Directory {device:u64,inode:u64},
 File {device:u64,inode:u64,links:u64,length:u64,sha256:String},
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PgSnapshotManifest {
 format:String,format_version:u32,backup_id:String,writers_stopped_asserted:bool,
 #[serde(default,skip_serializing_if="Option::is_none",deserialize_with="pg_producer_nonnull")]producer:Option<PgBackupProducer>,
 source_target:EngineTargetIdentity,source_root:PgSnapshotRoot,
 #[serde(default)] engine_generation:String,#[serde(default)] engine_envelope_sha256:String,
 #[serde(default)] source_authority:Option<PgCapturedSourceAuthority>,
 application_rows_sha256:String,catalog_sha256:String,migrations_sha256:String,engine_control_rows_sha256:String,
 files:PgUniqueMap<PgSnapshotEntry>,excluded:Vec<String>,restoration_admission:String,serving_allowed:bool,
 #[serde(default)] engine_request:Option<crate::storage::backup_inventory::Bytes>,
 #[serde(default,deserialize_with="pg_admission_capture_nonnull")] admission_owner_capture:Option<crate::storage::restoration::admission::AdmissionCaptureEnvelope>,
 #[serde(default,deserialize_with="pg_prior_capture_nonnull")] prior_admission_owner_capture:Option<crate::storage::restoration::linux::PriorRestoredCaptureEvidence>,
 #[serde(default,deserialize_with="pg_completed_capture_nonnull")] completed_absence_capture:Option<crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::CompletedAbsenceEnvelope>,
 #[serde(default,deserialize_with="pg_prior_completed_capture_nonnull")] prior_completed_absence_capture:Option<crate::storage::restoration::PriorCompletedCaptureEvidence>,
 #[serde(default,deserialize_with="pg_mixed_completed_capture_nonnull")] mixed_completed_absence_capture:Option<crate::storage::restoration::MixedCompletedCaptureEvidence>,
 #[serde(default,deserialize_with="pg_composite_capture_nonnull")] composite_admission_capture:Option<crate::storage::restoration::composite::CompositeCaptureEvidence>,
}
#[derive(serde::Deserialize)]
#[serde(tag="kind",rename_all="snake_case",deny_unknown_fields)]
enum PgCapturedSourceAuthority {PgRestore {generation_id:String,attestation_sha256:String}}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PgSnapshotControls {
 imports:Vec<(String,String,String)>,generations:Vec<(String,String,String,String)>,current:Vec<(i64,String)>,
}
struct InspectedPgSnapshot {
 history:HistorySnapshot,profile:EngineCatalogueProfile,
 held:OwnedPgBackup,manifest:PgSnapshotManifest,tables:BTreeMap<String,Table>,
 // Exact source marker custody is distinct from the new backup file identity.
 captured_ids:BTreeMap<String,Identity>,
}
fn pg_admission_capture_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<crate::storage::restoration::admission::AdmissionCaptureEnvelope>,D::Error>{Ok(Some(serde::Deserialize::deserialize(d)?))}
fn pg_prior_capture_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<crate::storage::restoration::linux::PriorRestoredCaptureEvidence>,D::Error>{Ok(Some(serde::Deserialize::deserialize(d)?))}
fn pg_snapshot_uuid(value:&str)->anyhow::Result<()> {
 ensure!(uuid::Uuid::parse_str(value)?.to_string()==value,"Noncanonical PG snapshot UUID");Ok(())
}
fn pg_snapshot_read(root:&Root,path:&str,limit:u64,roles:&mut BTreeMap<String,PgCaptureHeld>)->anyhow::Result<Vec<u8>> {
 let held=root.leaf(path)?;let before=physical_stamp(&held)?;
 if let Some(old)=roles.get(path){ensure!(physical_stamp(&old.held)?==old.stamp&&before==old.stamp,"PG snapshot repeated role observation changed");}

 ensure!(held.metadata()?.len()<=limit&&held.metadata()?.nlink()==1,"PG snapshot role exceeds bound or is aliased");
 let bytes=planned_hash(&held)?;let mut raw=vec![0u8;bytes.size_bytes as usize];
 let mut offset=0;while offset<raw.len(){let n=held.read_at(&mut raw[offset..],offset as u64)?;ensure!(n>0,"PG snapshot role truncated");offset+=n;}
 ensure!(physical_stamp(&held)?==before&&digest(&raw)==bytes.sha256,"PG snapshot role changed while reading");
 roles.insert(path.into(),PgCaptureHeld{held,stamp:before,bytes});Ok(raw)
}
fn pg_snapshot_valid_hash(s:&str)->bool{s.len()==64&&s.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))}
fn inspect_postgres_snapshot(path:&Path)->anyhow::Result<InspectedPgSnapshot> {
 let parent=Root::open(path.parent().context("PG snapshot parent missing")?)?;parent.check()?;
 let root=Root::open(path)?;let root_stamp=physical_stamp(root.fd())?;
 ensure!(root.fd().metadata()?.mode()&0o777==0o700,"PG snapshot root not private");
 let name=path.file_name().and_then(|s|s.to_str()).context("PG snapshot name invalid")?.to_owned();
 let mut roles=BTreeMap::new();let raw_manifest=pg_snapshot_read(&root,"manifest.json",16*1024*1024,&mut roles)?;
 let manifest:PgSnapshotManifest=serde_json::from_slice(&raw_manifest)?;
 ensure!(manifest.format=="starlane-notes-postgresql-stopped-write-backup"&&[1,2,3,4].contains(&manifest.format_version)&&manifest.writers_stopped_asserted&&!manifest.serving_allowed&&manifest.restoration_admission=="unsupported-until-new-target-attestation","Invalid PG snapshot contract");
 if let Some(c)=&manifest.admission_owner_capture {ensure!(manifest.format_version>=3,"Admission capture requires qualified capacity PG format");c.validate_native_reserved()?;}
 ensure!(manifest.admission_owner_capture.is_none()||manifest.prior_admission_owner_capture.is_none(),"Native and prior capture transports are mutually exclusive");
 if let Some(c)=&manifest.prior_admission_owner_capture {ensure!(manifest.format_version==4&&matches!(manifest.source_authority,Some(PgCapturedSourceAuthority::PgRestore{..})),"Prior admission requires qualified restored-source PG format4");c.validate_shape()?;}
 if let Some(c)=&manifest.mixed_completed_absence_capture {ensure!(manifest.format_version==4&&matches!(manifest.source_authority,Some(PgCapturedSourceAuthority::PgRestore{..}))&&manifest.completed_absence_capture.is_none()&&manifest.prior_completed_absence_capture.is_none()&&manifest.admission_owner_capture.is_none()&&manifest.prior_admission_owner_capture.is_none(),"Mixed completed PG transport requires exclusive restored source");c.validate_shape()?;}
 if let Some(c)=&manifest.prior_completed_absence_capture {ensure!(manifest.format_version==4&&matches!(manifest.source_authority,Some(PgCapturedSourceAuthority::PgRestore{..}))&&manifest.completed_absence_capture.is_none()&&manifest.admission_owner_capture.is_none()&&manifest.prior_admission_owner_capture.is_none(),"Prior completed PG transport requires exclusive restored source");c.validate_shape()?;}
 if let Some(c)=&manifest.completed_absence_capture {ensure!(manifest.format_version>=3&&manifest.admission_owner_capture.is_none()&&manifest.prior_admission_owner_capture.is_none(),"Completed PG original transport cannot mix admission roles");c.validate_shape()?;}
 if let Some(c)=&manifest.composite_admission_capture {ensure!(manifest.format_version==4&&matches!(manifest.source_authority,Some(PgCapturedSourceAuthority::PgRestore{..}))&&manifest.admission_owner_capture.is_none()&&manifest.prior_admission_owner_capture.is_none()&&manifest.completed_absence_capture.is_none()&&manifest.prior_completed_absence_capture.is_none()&&manifest.mixed_completed_absence_capture.is_none(),"Composite PG transport requires distinct exclusive restored format4");c.validate_shape()?;}
 pg_snapshot_uuid(&manifest.backup_id)?;
 match (&manifest.source_authority,manifest.format_version){
  (None,1|3)=>{pg_snapshot_uuid(&manifest.engine_generation)?;ensure!(pg_snapshot_valid_hash(&manifest.engine_envelope_sha256),"Invalid native transfer snapshot authority");},
  (Some(PgCapturedSourceAuthority::PgRestore{generation_id,attestation_sha256}),2|4)=>{pg_snapshot_uuid(generation_id)?;ensure!(pg_snapshot_valid_hash(attestation_sha256)&&manifest.engine_generation.is_empty()&&manifest.engine_envelope_sha256.is_empty(),"Mixed restored/transfer source authority");},
  _=>bail!("PG snapshot source authority/version differs"),
 }
 ensure!(manifest.source_root.inode>0&&manifest.excluded==["logging","logs","search_index"],"Invalid PG snapshot source or exclusions");
 let marker=if exists(root.fd(),".starlane-backup-incomplete")?{".starlane-backup-incomplete"}else{".pg-backup-incomplete"};
 let raw_marker=pg_snapshot_read(&root,marker,1024,&mut roles)?;
 ensure!(raw_marker==b"candidate private PG backup; no serving authority\n","Unknown PG snapshot marker");
 let(files,dirs)=engine_walk(root.fd(),"")?;
 for (_, (dir,_)) in &dirs {ensure!(dir.metadata()?.mode()&0o777==0o700,"PG snapshot directory not private");}
 let mut expected_dirs=std::collections::BTreeSet::from(["files".to_owned()]);
 let mut captured_ids=BTreeMap::new();let mut groups:BTreeMap<Identity,(u64,String)>=BTreeMap::new();
 for(p,e)in &manifest.files.0 {
  ensure!(valid_relative(p)&&!p.split('/').any(|x|["logging","logs","search_index"].contains(&x)&&!p.contains('/')),"Unsafe PG snapshot captured path");
  let out=format!("files/{p}");
  match e {
   PgSnapshotEntry::Directory{device,inode}=>{ensure!(*inode>0&&dirs.contains_key(&out),"Missing captured directory");expected_dirs.insert(out);captured_ids.insert(p.clone(),(*device,*inode));},
   PgSnapshotEntry::File{device,inode,links,length,sha256}=>{
    ensure!(*inode>0&&*links>0&&*length<=i64::MAX as u64&&pg_snapshot_valid_hash(sha256),"Invalid captured physical file");
    let f=root.leaf(&out)?;let stamp=physical_stamp(&f)?;let bytes=planned_hash(&f)?;
    ensure!(f.metadata()?.nlink()==1&&bytes.size_bytes==*length&&bytes.sha256==*sha256,"Captured PG file bytes or private alias differs");
    let group=(*length,sha256.clone());if let Some(old)=groups.insert((*device,*inode),group.clone()){ensure!(old==group,"Captured physical group has inconsistent bytes");}
    captured_ids.insert(p.clone(),(*device,*inode));roles.insert(out,PgCaptureHeld{held:f,stamp,bytes});
   }
  }
 }
 ensure!(dirs.keys().cloned().collect::<std::collections::BTreeSet<_>>()==expected_dirs,"Unclassified PG snapshot directory");
 let mut raws=BTreeMap::new();
 for(p,expected)in [("application-rows.json",&manifest.application_rows_sha256),("postgres-catalog.json",&manifest.catalog_sha256),("postgres-migrations.json",&manifest.migrations_sha256),("engine-control-rows.json",&manifest.engine_control_rows_sha256)] {
  ensure!(pg_snapshot_valid_hash(expected),"Invalid PG snapshot role digest");
  let raw=pg_snapshot_read(&root,p,64*1024*1024,&mut roles)?;ensure!(digest(&raw)==*expected,"PG snapshot role digest differs");raws.insert(p,raw);
 }
 ensure!(files==roles.keys().cloned().collect(),"Unknown PG snapshot file");
 let tables=serde_json::from_slice::<PgUniqueMap<Table>>(&raws["application-rows.json"])?.0;
 let source_profile=authenticated_pg_profile(&serde_json::from_slice::<Vec<String>>(&raws["postgres-migrations.json"])?)?;ensure!((manifest.format_version>=3)==(source_profile.has_capacity()),"Capacity PG snapshot requires distinct profile format");ensure!((manifest.completed_absence_capture.is_none()&&manifest.prior_completed_absence_capture.is_none()&&manifest.mixed_completed_absence_capture.is_none()&&manifest.composite_admission_capture.is_none())||source_profile.has_erasure(),"Completed PG capture requires exact53/7 profile");let expected_shape=shape_for_profile(source_profile)?;ensure!(tables.keys().eq(expected_shape.keys()),"PG snapshot table inventory differs");
 for(n,t)in &tables {
  let expected=&expected_shape[n];ensure!(t.columns==expected.columns&&t.keys==expected.keys,"PG snapshot table definition differs");
  let key_indices=t.keys.iter().map(|k|t.columns.iter().position(|c|&c.name==k).context("Missing snapshot key column")).collect::<anyhow::Result<Vec<_>>>()?;
  let mut keys=std::collections::BTreeSet::new();
  for row in &t.rows {ensure!(row.len()==t.columns.len(),"PG snapshot row width differs");for(c,v)in t.columns.iter().zip(row){validate_cell(v,c,n)?;}
   if !key_indices.is_empty(){let key=serde_json::to_vec(&key_indices.iter().map(|i|&row[*i]).collect::<Vec<_>>())?;ensure!(keys.insert(key),"Duplicate PG snapshot primary key");}
  }
  if n=="documents"||n=="attachments" {let col=if n=="documents"{"content_path"}else{"storage_path"};let index=t.columns.iter().position(|c|c.name==col).context("Missing current reference column")?;for row in &t.rows{if !matches!(row[index],Cell::Null){let p=text(&row[index])?;ensure!(matches!(manifest.files.0.get(p),Some(PgSnapshotEntry::File{..})),"PG current file reference absent");}}}
 }
 for name in ["attachments","excalidraw_files"] {
  let table=&tables[name];let pi=table.columns.iter().position(|c|c.name=="storage_path").context("Missing file reference column")?;
  let li=if name=="attachments"{Some(table.columns.iter().position(|c|c.name=="size_bytes").context("Missing file length")?)}else{None};
  for row in &table.rows{if !matches!(row[pi],Cell::Null){let path=text(&row[pi])?;ensure!(valid_relative(path),"Unsafe snapshot file reference");
   let length=match manifest.files.0.get(path){Some(PgSnapshotEntry::File{length,..})=>*length,_=>bail!("Snapshot referenced external file absent")};
   if let Some(li)=li{ensure!(matches!(row[li],Cell::Integer(n)if n>=0&&n as u64==length),"Snapshot attachment length differs");}
  }}
 }
 let controls:PgSnapshotControls=serde_json::from_slice(&raws["engine-control-rows.json"])?;
 if [2,4].contains(&manifest.format_version) {
  let history=pg_snapshot_restored_closure(&root,&manifest,&tables,&controls,&captured_ids,&mut roles,&raws)?;
  let held=OwnedPgBackup{root_identity:identity(root.fd())?,root,parent,name,manifest:raw_manifest,roles,root_stamp,directories:dirs};held.fence()?;
  return Ok(InspectedPgSnapshot{profile:source_profile,held,manifest,tables,captured_ids,history});
 }

 ensure!(controls.current==vec![(1,manifest.engine_generation.clone())]&&controls.generations.len()==1&&controls.imports.len()==1,"PG snapshot engine control closure differs");
 let(id,envelope_raw,envelope_sha,state)=&controls.generations[0];
 ensure!(id==&manifest.engine_generation&&state=="committed"&&envelope_sha==&manifest.engine_envelope_sha256&&digest(envelope_raw.as_bytes())==*envelope_sha,"PG snapshot committed envelope differs");
 let envelope:EngineCommitEnvelope=serde_json::from_str(envelope_raw)?;
 ensure!(envelope.target==manifest.source_target&&envelope.root_device==manifest.source_root.device&&envelope.root_inode==manifest.source_root.inode,"PG snapshot envelope source identity differs");
 let current_raw=pg_snapshot_read(&root,"files/.engine-transfer-custody/current.json",16*1024*1024,&mut roles)?;
 ensure!(current_raw==envelope_raw.as_bytes(),"PG snapshot raw engine envelope differs");
 match (&manifest.engine_request,manifest.files.0.get(".starlane-engine-transfer-request")) {
  (None,None)=>{},
  (Some(bound),Some(PgSnapshotEntry::File{length,sha256,links,..}))=>{
   ensure!(bound.size_bytes==*length&&bound.sha256==*sha256&&*links==1,"PG snapshot transfer request binding differs");
   let raw=pg_snapshot_read(&root,"files/.starlane-engine-transfer-request",1024,&mut roles)?;
   ensure!(raw==b"PostgreSQL transfer requested; ordinary SQLite startup forbidden\n","PG snapshot transfer request invalid");
  },_=>bail!("PG snapshot transfer request is unbound"),
 }
 let history=pg_snapshot_archived_closure(&root,&manifest,&tables,&controls,&envelope,&captured_ids,&mut roles,&raws)?;
 let held=OwnedPgBackup{root_identity:identity(root.fd())?,root,parent,name,manifest:raw_manifest,roles,root_stamp,directories:dirs};held.fence()?;
 Ok(InspectedPgSnapshot{profile:source_profile,held,manifest,tables,captured_ids,history})
}
#[cfg(test)]
mod postgres_snapshot_tests {
 use super::*;
 #[test]
 #[ignore="explicit genuine retained PG snapshot path required"]
 fn required_fresh_postgres_snapshot_inspection()->anyhow::Result<()> {
  let path=PathBuf::from(std::env::var("STARLANE_PG_SNAPSHOT_PATH").expect("genuine retained PG snapshot required"));
  let snapshot=inspect_postgres_snapshot(&path)?;snapshot.held.fence()?;
  ensure!(snapshot.tables.len()==72&&snapshot.tables.values().flat_map(|t|&t.rows).flat_map(|r|r).filter(|v|matches!(v,Cell::Blob(b)if !b.is_empty())).count()>=2,"Actual application BYTEA required");
  ensure!(!snapshot.captured_ids.is_empty()&&!snapshot.manifest.serving_allowed,"Snapshot accidentally admitted");
  println!("FRESH_PG_SNAPSHOT_INSPECTION_PASS typed rows/file/control binding only; archived history and restore not admitted");Ok(())
 }
}
fn pg_snapshot_archived_closure(root:&Root,m:&PgSnapshotManifest,tables:&BTreeMap<String,Table>,controls:&PgSnapshotControls,e:&EngineCommitEnvelope,ids:&BTreeMap<String,Identity>,roles:&mut BTreeMap<String,PgCaptureHeld>,raws:&BTreeMap<&str,Vec<u8>>)->anyhow::Result<HistorySnapshot> {
 ensure!(e.version==1&&e.transfer_id==m.engine_generation&&e.target.schema_name==format!("transfer_{}",uuid::Uuid::parse_str(&e.transfer_id)?.simple()),"Archived engine target reservation differs");
 let migrations:Vec<String>=serde_json::from_slice(&raws["postgres-migrations.json"])?;
 authenticated_pg_profile(&migrations)?;
 let catalog:Vec<String>=serde_json::from_slice(&raws["postgres-catalog.json"])?;
 let control_catalog=catalog.iter().filter(|r|serde_json::from_str::<serde_json::Value>(r).ok().and_then(|v|v.get(1).and_then(|x|x.as_str()).map(|x|["engine_transfer_current","engine_transfer_generations"].contains(&x))).unwrap_or(false)).cloned().collect::<Vec<_>>();
 ensure!(digest(&serde_json::to_vec(&control_catalog)?)==e.control_catalog_sha256,"Archived engine control catalogue differs");
 let dir=format!(".engine-transfer-custody/{}",e.transfer_id);
 let role=|p:&str|format!("files/{dir}/{p}");
 let raw_intent=pg_snapshot_read(root,&role("intent.json"),16*1024*1024,roles)?;
 ensure!(digest(&raw_intent)==e.seal_intent_sha256,"Archived engine intent digest differs");
 let intent:EngineSealIntent=serde_json::from_slice(&raw_intent)?;
 ensure!(intent.transfer_id==e.transfer_id&&intent.target==e.target&&intent.root_device==e.root_device&&intent.root_inode==e.root_inode&&!intent.serving_allowed&&intent.custody_generation_id==e.custody_generation_id&&intent.custody_ledger_sha256==e.custody_ledger_sha256&&intent.import_commitment_sha256==e.import_commitment_sha256&&intent.marker_device==e.marker_device&&intent.marker_inode==e.marker_inode&&intent.marker_sha256==e.retired_marker_sha256,"Archived engine intent binding differs");
 ensure!(intent.roles.keys().map(String::as_str).collect::<std::collections::BTreeSet<_>>()==["source-intent.json","original.sqlite","original-manifest.json","import-commitment.json"].into_iter().collect(),"Archived engine role closure differs");
 for(n,b)in &intent.roles {let p=role(n);let held=roles.get(&p).context("Archived engine role absent")?;ensure!(held.bytes.size_bytes==b.length&&held.bytes.sha256==b.sha256,"Archived engine evidence bytes differ");}
 let imported=&controls.imports[0];ensure!(imported.0==e.transfer_id&&imported.2==e.import_commitment_sha256&&digest(imported.1.as_bytes())==e.import_commitment_sha256,"Archived immutable import SQL differs");
 let raw_import=pg_snapshot_read(root,&role("import-commitment.json"),16*1024*1024,roles)?;ensure!(raw_import==imported.1.as_bytes(),"Archived import bytes differ from SQL");
 let imported_record:serde_json::Value=serde_json::from_slice(&raw_import)?;
 ensure!(imported_record["version"]==1&&imported_record["phase"]=="post-custody-import-committed-only"&&imported_record["serving_allowed"]==false&&imported_record["target"]==serde_json::to_value(&m.source_target)?&&imported_record["root_device"].as_u64()==Some(e.root_device)&&imported_record["root_inode"].as_u64()==Some(e.root_inode),"Captured original import protocol association differs");
 ensure!(imported_record["target_catalog_sha256"].as_str()==Some(digest(&serde_json::to_vec(&engine_without_control_catalog(catalog.clone())?)?).as_str())&&imported_record["postgres_migrations_sha256"].as_str()==Some(digest(&raws["postgres-migrations.json"]).as_str()),"Captured full application schema or independent migration history differs from genuine import");
 for(field,file)in [("pre_import_intent_sha256","source-intent.json"),("source_manifest_sha256","original-manifest.json"),("source_sqlite_database_sha256","original.sqlite")]{ensure!(imported_record[field].as_str()==Some(roles[&role(file)].bytes.sha256.as_str()),"Captured original SQLite import evidence detached");}
 ensure!(imported_record["custody_generation_id"].as_str()==Some(e.custody_generation_id.as_str())&&imported_record["custody_ledger_sha256"].as_str()==Some(e.custody_ledger_sha256.as_str()),"Captured original custody import association differs");

 let raw_completion=pg_snapshot_read(root,&role("completion.json"),16*1024*1024,roles)?;
 ensure!(digest(&raw_completion)==e.completion_sha256&&serde_json::from_slice::<serde_json::Value>(&raw_completion)?==serde_json::json!({"version":1,"transfer_id":e.transfer_id,"seal_intent_sha256":e.seal_intent_sha256,"import_commitment_sha256":e.import_commitment_sha256,"target":e.target,"root_device":e.root_device,"root_inode":e.root_inode,"phase":"actual-engine-sql-committed-closed-readback"}),"Archived engine completion differs");
 let raw_final=pg_snapshot_read(root,&role("finalization.json"),16*1024*1024,roles)?;
 ensure!(digest(&raw_final)==e.finalization_sha256&&serde_json::from_slice::<serde_json::Value>(&raw_final)?==serde_json::json!({"version":1,"transfer_id":e.transfer_id,"completion_sha256":e.completion_sha256,"retired_marker":format!("{dir}/retired-marker"),"retired_marker_sha256":e.retired_marker_sha256,"marker_device":e.marker_device,"marker_inode":e.marker_inode,"root_device":e.root_device,"root_inode":e.root_inode,"phase":"exact-marker-retired-and-all-postchecks"}),"Archived engine finalization differs");
 let retired=format!("{dir}/retired-marker");ensure!(ids.get(&retired)==Some(&(e.marker_device,e.marker_inode))&&roles[&format!("files/{retired}")].bytes.sha256==e.retired_marker_sha256,"Captured original engine marker differs");
 let engine_files=m.files.0.iter().filter_map(|(p,v)|if p.starts_with(".engine-transfer-custody/")&&matches!(v,PgSnapshotEntry::File{..}){Some(p.clone())}else{None}).collect::<std::collections::BTreeSet<_>>();
 let expected_engine=["source-intent.json","original.sqlite","original-manifest.json","import-commitment.json","intent.json","completion.json","finalization.json","retired-marker"].into_iter().map(|n|format!("{dir}/{n}")).chain([".engine-transfer-custody/current.json".into()]).collect();ensure!(engine_files==expected_engine,"Unknown archived engine evidence");
 let custody=e.custody_shapes.keys().map(|n|Ok((n.clone(),tables.get(n).context("Missing archived custody SQL")?.clone()))).collect::<anyhow::Result<BTreeMap<_,_>>>()?;
 ensure!(custody.len()==2&&custody.contains_key("storage_restore_generations")&&custody.contains_key("storage_restore_current")&&engine_typed_hash(&custody)?==e.custody_sql_sha256,"Archived immutable custody SQL differs");
 for(n,t)in &e.custody_shapes{ensure!(t.rows.is_empty()&&t.columns==custody[n].columns&&t.keys==custody[n].keys,"Archived custody definition differs");}
 let generation=&custody["storage_restore_generations"];let mut rows=vec![];
 for row in &generation.rows {let get=|n:&str|->anyhow::Result<String>{let i=generation.columns.iter().position(|c|c.name==n).context("Missing custody column")?;Ok(text(&row[i])?.into())};rows.push(HistorySqlRow{generation_id:get("generation_id")?,backup_id:get("backup_id")?,manifest_sha256:get("manifest_sha256")?,database_sha256:get("database_sha256")?,ledger_sha256:get("ledger_sha256")?,root_device:get("root_device")?,root_inode:get("root_inode")?,state:get("state")?,created_at:get("created_at")?});}
 // Match the actual immutable SQL reader ordering; all raw typed rows and cells remain unchanged.
 rows.sort_by(|a,b|a.generation_id.cmp(&b.generation_id));
 let current=&custody["storage_restore_current"];let si=current.columns.iter().position(|c|c.name=="singleton").context("Missing singleton")?;let gi=current.columns.iter().position(|c|c.name=="generation_id").context("Missing generation pointer")?;
 let pointers=current.rows.iter().map(|r|match r[si]{Cell::Integer(v)=>Ok((v,text(&r[gi])?.into())),_=>bail!("Invalid history singleton")}).collect::<anyhow::Result<Vec<_>>>()?;
 for(p,v)in &m.files.0{if p.starts_with(".restoration-custody/"){if let PgSnapshotEntry::File{links,..}=v{ensure!(*links==1,"Captured archived custody file aliased");}}}
 let files_root=Root::open(&root.absolute.join("files"))?;let mut source=HistorySource::new_with_check(&files_root,None,engine_root_fence)?;source.captured_ids=Some(ids);
 let closed=history_closure(source,(e.root_device,e.root_inode),Some((&rows,&pointers)))?;
 ensure!(closed.current_id==e.custody_generation_id&&digest(&closed.generations[&closed.current_id].raw)==e.custody_ledger_sha256,"Archived full history association differs");closed.source.fence()?;Ok(HistorySnapshot{current_id:closed.current_id.clone(),raw_generations:closed.generations.iter().map(|(id,g)|(id.clone(),g.raw.clone())).collect(),rows,denied:closed.generations[&closed.current_id].envelope.as_ref().map(|v|v.denied.clone()).unwrap_or_default()})
}


// Read-only captured-source validation. Current authority is its genuine PG
// SQL attestation. The original transfer bytes remain retained historical roles.
fn pg_snapshot_restored_closure(root:&Root,m:&PgSnapshotManifest,tables:&BTreeMap<String,Table>,controls:&PgSnapshotControls,ids:&BTreeMap<String,Identity>,roles:&mut BTreeMap<String,PgCaptureHeld>,raws:&BTreeMap<&str,Vec<u8>>)->anyhow::Result<HistorySnapshot>{
 let (generation,expected_sha)=match m.source_authority.as_ref(){Some(PgCapturedSourceAuthority::PgRestore{generation_id,attestation_sha256})=>(generation_id,attestation_sha256),_=>bail!("Explicit PgRestore source required")};
 ensure!(controls.current==vec![(1,generation.clone())]&&controls.imports.len()==1&&controls.generations.len()==1,"Captured PgRestore control closure differs");
 let(id,raw,sha,state)=&controls.generations[0];
 ensure!(id==generation&&state=="committed"&&sha==expected_sha&&digest(raw.as_bytes())==*sha,"Captured PgRestore committed SQL differs");
 let e:PgTargetAttestation=serde_json::from_str(raw)?;
 ensure!([1,2].contains(&e.version)&&(e.version==2)==e.parent_attestation.is_some()&&e.restore_id==*generation&&e.target==m.source_target&&(e.root_device,e.root_inode)==(m.source_root.device,m.source_root.inode)&&e.target.schema_name==format!("restore_{}",uuid::Uuid::parse_str(generation)?.simple()),"Captured PgRestore target/root association differs");
 let current=pg_snapshot_read(root,"files/.pg-restoration-custody/current.json",16*1024*1024,roles)?;
 ensure!(current==raw.as_bytes(),"Captured PgRestore raw current differs from SQL");
 let import=&controls.imports[0];ensure!(import.0==*generation&&import.2==e.import_commitment_sha256&&digest(import.1.as_bytes())==import.2,"Captured PgRestore import SQL differs");
 let archive=format!(".pg-restoration-custody/{generation}");
 // This genuine publisher stores its immutable import commitment in SQL;
 // the capture binds the exact raw SQL cell. No nonexistent proof file is invented.
 let import_value:serde_json::Value=serde_json::from_str(&import.1)?;
 ensure!(import_value["version"]==1&&import_value["phase"]=="pg-restoration-import-only"&&import_value["serving_allowed"]==false&&import_value["restore_id"].as_str()==Some(generation.as_str())&&import_value["target"]==serde_json::to_value(&e.target)?&&import_value["target_root_device"].as_u64()==Some(e.root_device)&&import_value["target_root_inode"].as_u64()==Some(e.root_inode),"Captured PgRestore immutable import association differs");
 let catalog:Vec<String>=serde_json::from_slice(&raws["postgres-catalog.json"])?;
 ensure!(digest(&serde_json::to_vec(&catalog)?)==e.catalogue_sha256&&digest(&raws["postgres-migrations.json"])==e.independent_migrations_sha256,"Captured PgRestore full schema/migration definition differs");
 let migrations:Vec<String>=serde_json::from_slice(&raws["postgres-migrations.json"])?;
 authenticated_pg_profile(&migrations)?;

 ensure!(!e.retained_roles.0.is_empty(),"Captured PgRestore immutable archive absent");
 for(p,b)in &e.retained_roles.0 {ensure!(valid_relative(p)&&valid_hash(&b.sha256)&&b.size_bytes<=i64::MAX as u64&&(p.starts_with(".pg-restoration-custody/")||p.starts_with(".engine-transfer-custody/")||p==".starlane-engine-transfer-request"),"Captured PgRestore unknown archive role");let held=roles.get(&format!("files/{p}")).context("Captured PgRestore role absent")?;ensure!(held.bytes==*b,"Captured PgRestore immutable role differs");}
 let known_engine=e.retained_roles.0.keys().filter(|p|p.starts_with(".engine-transfer-custody/")).cloned().collect::<std::collections::BTreeSet<_>>();
 let actual_engine=m.files.0.iter().filter_map(|(p,v)|if p.starts_with(".engine-transfer-custody/")&&matches!(v,PgSnapshotEntry::File{..}){Some(p.clone())}else{None}).collect();ensure!(known_engine==actual_engine,"Captured PgRestore unknown original engine role");
 let mut known_pg=e.retained_roles.0.keys().filter(|p|p.starts_with(".pg-restoration-custody/")).cloned().chain([".pg-restoration-custody/current.json".into()]).collect::<std::collections::BTreeSet<_>>();
 if pg_target_final_required(&e){
  let inventory_path=pg_target_native_record(&e.restore_id);let ack_path=pg_target_final_path(&e.restore_id);
  let inventory=pg_snapshot_read(root,&format!("files/{inventory_path}"),PG_TARGET_NATIVE_LIMIT,roles)?;
  let ack=pg_snapshot_read(root,&format!("files/{ack_path}"),PG_TARGET_FINAL_LIMIT as u64,roles)?;
  let id=match m.files.0.get(&inventory_path){Some(PgSnapshotEntry::File{device,inode,..})=>(*device,*inode),_=>bail!("Captured final target inventory identity absent")};
  pg_target_final_verify(&ack,&e,raw.as_bytes(),&inventory,Some(id),&pg_target_final_key()?)?;
  known_pg.insert(ack_path);
 }
 let actual_pg=m.files.0.iter().filter_map(|(p,v)|if p.starts_with(".pg-restoration-custody/")&&matches!(v,PgSnapshotEntry::File{..}){Some(p.clone())}else{None}).collect();ensure!(known_pg==actual_pg,"Captured PgRestore unknown historical role");
 let old_manifest=pg_snapshot_read(root,&format!("files/{archive}/manifest.json"),16*1024*1024,roles)?;ensure!(digest(&old_manifest)==e.snapshot_sha256,"Captured PgRestore original snapshot differs");
 let old:PgSnapshotManifest=serde_json::from_slice(&old_manifest)?;ensure!(import_value["snapshot_backup_id"].as_str()==Some(old.backup_id.as_str())&&import_value["snapshot_manifest_sha256"].as_str()==Some(e.snapshot_sha256.as_str())&&import_value["source_target"]==serde_json::to_value(&old.source_target)?,"Captured immutable import detached from archived source");ensure!(old.backup_id==e.backup_id&&old.source_target==e.source_target&&(old.source_root.device,old.source_root.inode)==(e.source_root_device,e.source_root_inode),"Captured PgRestore old source association differs");
 for(n,sha)in [("application-rows.json",&old.application_rows_sha256),("postgres-catalog.json",&old.catalog_sha256),("postgres-migrations.json",&old.migrations_sha256),("engine-control-rows.json",&old.engine_control_rows_sha256)]{ensure!(roles[&format!("files/{archive}/{n}")].bytes.sha256==*sha,"Captured PgRestore original metadata differs");}
 for(p,b)in &e.retained_roles.0 {if p.starts_with(".engine-transfer-custody/"){ensure!(matches!(old.files.0.get(p),Some(PgSnapshotEntry::File{length,sha256,..})if *length==b.size_bytes&&sha256==&b.sha256),"Captured historical SQLite transfer role detached");}}
 match (&m.engine_request,m.files.0.get(".starlane-engine-transfer-request")){(None,None)=>{},(Some(b),Some(PgSnapshotEntry::File{length,sha256,links,..}))=>{ensure!(b.size_bytes==*length&&b.sha256==*sha256&&*links==1,"Captured request identity differs");let raw=pg_snapshot_read(root,"files/.starlane-engine-transfer-request",1024,roles)?;ensure!(raw==b"PostgreSQL transfer requested; ordinary SQLite startup forbidden\n","Captured request bytes differ");},_=>bail!("Captured request role is unbound")}
 let custody=["storage_restore_generations","storage_restore_current"].into_iter().map(|n|Ok((n.to_owned(),tables.get(n).context("Missing captured history SQL")?.clone()))).collect::<anyhow::Result<BTreeMap<_,_>>>()?;
 ensure!(engine_typed_hash(&custody)?==e.custody_sql_sha256,"Captured PgRestore immutable custody SQL differs");
 let generation=&custody["storage_restore_generations"];let mut rows=vec![];
 for row in &generation.rows {let get=|n:&str|->anyhow::Result<String>{let i=generation.columns.iter().position(|c|c.name==n).context("Missing custody column")?;Ok(text(&row[i])?.into())};rows.push(HistorySqlRow{generation_id:get("generation_id")?,backup_id:get("backup_id")?,manifest_sha256:get("manifest_sha256")?,database_sha256:get("database_sha256")?,ledger_sha256:get("ledger_sha256")?,root_device:get("root_device")?,root_inode:get("root_inode")?,state:get("state")?,created_at:get("created_at")?});}
 // Match the actual immutable SQL reader ordering; all raw typed rows and cells remain unchanged.
 rows.sort_by(|a,b|a.generation_id.cmp(&b.generation_id));
 let current=&custody["storage_restore_current"];let si=current.columns.iter().position(|c|c.name=="singleton").context("Missing singleton")?;let gi=current.columns.iter().position(|c|c.name=="generation_id").context("Missing generation pointer")?;
 let pointers=current.rows.iter().map(|r|match r[si]{Cell::Integer(v)=>Ok((v,text(&r[gi])?.into())),_=>bail!("Invalid history singleton")}).collect::<anyhow::Result<Vec<_>>>()?;
 for(p,v)in &m.files.0{if p.starts_with(".restoration-custody/"){if let PgSnapshotEntry::File{links,..}=v{ensure!(*links==1,"Captured archived custody file aliased");}}}
 let files_root=Root::open(&root.absolute.join("files"))?;let mut source=HistorySource::new_with_check(&files_root,None,engine_root_fence)?;source.captured_ids=Some(ids);
 let closed=history_closure(source,(e.root_device,e.root_inode),Some((&rows,&pointers)))?;
 ensure!(closed.current_id==e.custody_generation_id&&digest(&closed.generations[&closed.current_id].raw)==e.custody_ledger_sha256,"Archived full history association differs");let base=format!("{NAMESPACE}/generations/{}",e.custody_generation_id);
 for(n,sha)in [("intent.json",&e.seal_intent_sha256),("completion.json",&e.completion_sha256),("finalization.json",&e.finalization_sha256)]{ensure!(digest(&closed.source.records[&format!("{base}/{n}")].raw)==*sha,"Captured PgRestore finalized role differs");}
 ensure!(ids.get(&format!("{base}/retired-marker"))==Some(&(e.marker_device,e.marker_inode)),"Captured PgRestore original marker identity differs");

 // The format version describes the explicit source kind, never its generation
 // ordinal. Validate the complete retained predecessor authority at every capture.
 let mut predecessor_records=BTreeMap::new();
 for(p,expected)in &e.retained_roles.0 {
  let r=roles.get(&format!("files/{p}")).context("Captured predecessor role absent")?;
  ensure!(r.bytes==*expected&&physical_stamp(&r.held)?==r.stamp,"Captured predecessor role changed");
  predecessor_records.insert(p.clone(),EngineHeldRecord{held:r.held.try_clone()?,stamp:r.stamp.clone(),bytes:r.bytes.clone()});
 }
 // Its current final ACK deliberately lies outside its own retained_roles.
 // Preserve the exact already verified snapshot descriptor/stamp/bytes for the
 // chain verifier; these copied descriptors are history, not current ownership.
 if pg_target_final_required(&e){
  let path=pg_target_final_path(&e.restore_id);let r=roles.get(&format!("files/{path}")).context("Captured current final acknowledgment absent")?;
  ensure!(r.bytes.size_bytes<=PG_TARGET_FINAL_LIMIT as u64&&physical_stamp(&r.held)?==r.stamp&&planned_hash(&r.held)?==r.bytes,"Captured current final acknowledgment changed");
  ensure!(predecessor_records.insert(path,EngineHeldRecord{held:r.held.try_clone()?,stamp:r.stamp.clone(),bytes:r.bytes.clone()}).is_none(),"Current acknowledgment overlapped retained historical role");
 }
 pg_restore_archived_authority_chain(&predecessor_records,&e)?;
 closed.source.fence()?;Ok(HistorySnapshot{current_id:closed.current_id.clone(),raw_generations:closed.generations.iter().map(|(id,g)|(id.clone(),g.raw.clone())).collect(),rows,denied:closed.generations[&closed.current_id].envelope.as_ref().map(|v|v.denied.clone()).unwrap_or_default()})
}

include!("engine_postgres_admission.rs");

include!("engine_postgres_completed.rs");
include!("engine_postgres_composite.rs");
