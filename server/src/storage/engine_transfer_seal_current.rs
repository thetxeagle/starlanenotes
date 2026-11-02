// Candidate-only private retained engine seal. No SQL admission or marker retirement.
#[derive(Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EngineSealRole {length:u64,sha256:String}
#[derive(Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EngineSealIntent {
 version:u32,phase:String,transfer_id:String,target:EngineTargetIdentity,
 root_device:u64,root_inode:u64,marker_device:u64,marker_inode:u64,
 marker_sha256:String,custody_generation_id:String,custody_ledger_sha256:String,
 original_manifest_sha256:String,original_sqlite_database_sha256:String,
 source_typed_sha256:String,post_custody_typed_sha256:String,
 import_commitment_sha256:String,roles:BTreeMap<String,EngineSealRole>,
 serving_allowed:bool,
}
pub(crate) struct SealedEngineImport<'a> {
 imported:ImportedEngineTransfer<'a>,intent:EngineSealIntent,raw_intent:Vec<u8>,
}
impl SealedEngineImport<'_> {
 pub(crate) fn path(&self)->&Path {self.imported.path()}
 pub(crate) fn fence(&self)->io::Result<()> {self.imported.fence()}
}
pub(crate) fn seal_engine_import<'a>(mut imported:ImportedEngineTransfer<'a>)->anyhow::Result<SealedEngineImport<'a>> {
 imported.fence()?;engine_finalized_inert_gate(&imported.prepared)?;
 let id=imported.prepared.transfer_id.clone();
 let dir=imported.prepared.engine_directory.clone();
 ensure!(dir==format!(".engine-transfer-custody/{id}"),"Engine source reservation changed");
 let source_intent=imported.prepared.destination.leaves.get(&format!("{dir}/source-intent.json"))
   .context("Durable pre-import engine intent absent")?;
 ensure!(read_ledger(&source_intent.held)?==imported.prepared.raw_source_intent,"Pre-import engine intent changed");
 imported.fence()?;
 let raw_manifest=imported.prepared.original.bundle.raw_manifest.clone();
 let raw_commitment=imported.certificate.raw_commitment.clone();
 ensure!(!raw_commitment.is_empty(),"Internal committed SQL bytes absent");
 let original=imported.prepared.original._database.try_clone()?;
 let original_stamp=physical_stamp(&original)?;
 let source_bytes=imported.prepared.original.bundle.manifest.files.0["starlane-notes.db"].clone();
 pending_write_file(&mut imported.prepared.destination,&format!("{dir}/original.sqlite"),
  |out| {
   use std::os::unix::fs::FileExt;use std::io::Write;
   let mut offset=0u64;let mut chunk=[0u8;65536];
   while offset<source_bytes.size_bytes {
    let want=(source_bytes.size_bytes-offset).min(chunk.len() as u64) as usize;
    let n=original.read_at(&mut chunk[..want],offset)?;
    if n==0{return Err(fail("Original SQLite evidence truncated during retained copy"));}
    out.write_all(&chunk[..n])?;offset+=n as u64;
   }
   if physical_stamp(&original)?!=original_stamp{return Err(fail("Original SQLite evidence changed during retained copy"));}
   Ok(())
  },source_bytes.clone())?;
 imported.fence()?;
 pending_new_file(&mut imported.prepared.destination,&format!("{dir}/original-manifest.json"),&raw_manifest)?;
 imported.fence()?;
 pending_new_file(&mut imported.prepared.destination,&format!("{dir}/import-commitment.json"),&raw_commitment)?;
 imported.fence()?;
 let marker_id=identity(&imported.prepared.destination.copied.marker)?;
 let root_id=identity(imported.prepared.destination.copied.root.fd())?;
 let mut roles=BTreeMap::new();
 roles.insert("source-intent.json".into(),EngineSealRole{length:imported.prepared.raw_source_intent.len() as u64,sha256:digest(&imported.prepared.raw_source_intent)});
 roles.insert("original.sqlite".into(),EngineSealRole{length:source_bytes.size_bytes,sha256:source_bytes.sha256.clone()});
 roles.insert("original-manifest.json".into(),EngineSealRole{length:raw_manifest.len()as u64,sha256:digest(&raw_manifest)});
 roles.insert("import-commitment.json".into(),EngineSealRole{length:raw_commitment.len()as u64,sha256:digest(&raw_commitment)});
 let intent=EngineSealIntent{
  version:1,phase:"private-engine-sealed-no-admission".into(),transfer_id:id,
  target:imported.certificate.target.clone(),root_device:root_id.0,root_inode:root_id.1,
  marker_device:marker_id.0,marker_inode:marker_id.1,
  marker_sha256:digest(&imported.prepared.destination.copied.marker_bytes),
  custody_generation_id:imported.prepared.custody_generation_id.clone(),
  custody_ledger_sha256:imported.prepared.custody_ledger_sha256.clone(),
  original_manifest_sha256:digest(&raw_manifest),original_sqlite_database_sha256:source_bytes.sha256,
  source_typed_sha256:imported.prepared.source_typed_sha256.clone(),
  post_custody_typed_sha256:imported.prepared.post_custody_typed_sha256.clone(),
  import_commitment_sha256:digest(&raw_commitment),roles,serving_allowed:false,
 };
 let raw_intent=serde_json::to_vec(&intent)?;
 pending_new_file(&mut imported.prepared.destination,&format!("{dir}/intent.json"),&raw_intent)?;
 imported.fence()?;
 engine_checkpoint("engine-sealed")?;
 Ok(SealedEngineImport{imported,intent,raw_intent})
}
