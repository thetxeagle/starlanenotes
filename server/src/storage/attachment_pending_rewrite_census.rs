//! Original pending Replay rewrite observation only. No serving, publication or unlink grant.
use super::*;
/// Constructed only from the original held lease and the same guarded SQL connection.
/// The whole family census still holds every namespace/file and rejects foreign siblings.
pub(super) struct SelectedReplayRewrite{
 lease:WriterLease, namespace:File, namespace_stamp:Stamp, directory:File, directory_stamp:Stamp,
 payload:File,payload_stamp:Stamp,manifest:File,manifest_stamp:Stamp,proof:File,proof_stamp:Stamp,
 op:String,manifest_raw:Vec<u8>,proof_raw:Vec<u8>,prepared:crate::storage::canonical_rewrite::PreparedRewrite,
}
impl SelectedReplayRewrite{
 pub(super) async fn capture_in(conn:&mut sqlx::AnyConnection,lease:&WriterLease,plan:&ReplayDisposal)->io::Result<Option<Self>>{
  lease.verify()?;
  if plan.version!=1||plan.attempt_id!=lease.attempt_id()||plan.claim_json.as_bytes()!=lease.original_claim()||plan.request_sha256!=lease.producer_sha256()||plan.admission_claim_sha256!=lease.claim_sha256()||plan.maximum_bytes!=lease.maximum_bytes()as i64||lease.replay_plan()?.as_deref()!=plan.rewrite_plan_json.as_ref().map(|s|s.as_bytes()){return Err(fail("Selected Replay rewrite original plan differs"));}
  let Some(raw)=plan.rewrite_plan_json.as_deref()else{if plan.rewrite_manifest_json.is_some()||plan.rewrite_install_proof_json.is_some()||plan.rewrite_incomplete_disposition_json.is_some(){return Err(fail("Selected Replay rewrite lacks original linkage"));}return Ok(None)};
  let actual=lease.classify_replay_rewrite(conn).await.map_err(io::Error::other)?;
  if actual.0.as_deref()!=plan.rewrite_manifest_json.as_ref().map(|s|s.as_bytes())||actual.1.as_deref()!=plan.rewrite_install_proof_json.as_ref().map(|s|s.as_bytes())||actual.2.as_deref()!=plan.rewrite_incomplete_disposition_json.as_ref().map(|s|s.as_bytes()){return Err(fail("Selected Replay rewrite same-connection proof differs"));}
  // Incomplete rewrite ownership requires its own bounded constructor, never a fallback.
  if actual.2.is_some(){return Err(fail("Incomplete selected Replay rewrite retains review"));}
  let original:ReplayRewritePlan=decode(raw.as_bytes())?;
  let origin_raw=optional_bytes(&lease.0.directory,"replay-rewrite-origin.json")?.ok_or_else(||fail("Selected Replay rewrite native origin absent"))?;
  let origin:ReplayRewriteOrigin=decode(&origin_raw)?;
  if origin.version!=1||origin.claim_sha256!=lease.claim_sha256()||origin.plan_sha256!=digest(raw.as_bytes())||origin.op_id!=original.op_id{return Err(fail("Selected Replay rewrite origin linkage differs"));}
  let namespace=stage::open(&lease.0.root,".canonical-rewrite-journal",libc::O_RDONLY|libc::O_DIRECTORY)?;
  let directory=stage::open(&namespace,&original.op_id,libc::O_RDONLY|libc::O_DIRECTORY)?;
  let payload=stage::open(&directory,"payload",libc::O_RDONLY|libc::O_NONBLOCK)?;
  if identity(&namespace)?!=origin.namespace||identity(&directory)?!=origin.parent||identity(&payload)?!=origin.payload{return Err(fail("Selected Replay rewrite original native custody differs"));}
  let(manifest,manifest_raw)=bytes(&directory,"manifest.json")?;let(proof,proof_raw)=proof_record(&directory)?;
  if Some(manifest_raw.as_slice())!=actual.0.as_deref()||Some(proof_raw.as_slice())!=actual.1.as_deref()||origin.manifest_json.as_bytes()!=manifest_raw{return Err(fail("Selected Replay rewrite original immutable records differ"));}
  let prepared=decode(&manifest_raw)?;
  let held=Self{lease:lease.clone(),namespace_stamp:stamp(&namespace)?,directory_stamp:stamp(&directory)?,payload_stamp:stamp(&payload)?,manifest_stamp:stamp(&manifest)?,proof_stamp:stamp(&proof)?,namespace,directory,payload,manifest,proof,op:original.op_id,manifest_raw,proof_raw,prepared};held.fence()?;Ok(Some(held))
 }
 pub(super) fn covers_namespace(&self,file:&File)->io::Result<bool>{self.fence()?;Ok(stamp(file)?==self.namespace_stamp&&names(file)?==BTreeSet::from([self.op.clone()]))}
 pub(super) fn matches_manifest(&self,path:&str,file:&File)->io::Result<bool>{self.fence()?;Ok(path==format!(".canonical-rewrite-journal/{}/manifest.json",self.op)&&stamp(file)?==self.manifest_stamp)}
 pub(super) fn fence(&self)->io::Result<()>{
  self.lease.verify()?;let ns=stage::open(&self.lease.0.root,".canonical-rewrite-journal",libc::O_RDONLY|libc::O_DIRECTORY)?;let dir=stage::open(&ns,&self.op,libc::O_RDONLY|libc::O_DIRECTORY)?;
  for(f,expected)in[(&self.namespace,&self.namespace_stamp),(&ns,&self.namespace_stamp),(&self.directory,&self.directory_stamp),(&dir,&self.directory_stamp),(&self.payload,&self.payload_stamp),(&self.manifest,&self.manifest_stamp),(&self.proof,&self.proof_stamp)]{if stamp(f)?!=*expected{return Err(fail("Selected Replay rewrite held native record changed"));}}
  let(m,mraw)=bytes(&dir,"manifest.json")?;let(p,praw)=proof_record(&dir)?;let body=stage::open(&dir,"payload",libc::O_RDONLY|libc::O_NONBLOCK)?;
  if stamp(&m)?!=self.manifest_stamp||stamp(&p)?!=self.proof_stamp||stamp(&body)?!=self.payload_stamp||mraw!=self.manifest_raw||praw!=self.proof_raw{return Err(fail("Selected Replay rewrite current native record changed"));}
  crate::storage::canonical_rewrite::verify_prepared_files_in(&self.lease.0.root_path,&self.prepared).map_err(|_|fail("Selected Replay rewrite current alias proof changed"))?;
  let proof:serde_json::Value=decode(&self.proof_raw)?;let temp=proof["temp"].as_str().ok_or_else(||fail("Selected rewrite anchor role absent"))?;
  let anchor=stage::open(&dir,"install-anchor",libc::O_RDONLY|libc::O_NONBLOCK)?;let original=stage::open(&dir,temp,libc::O_RDONLY|libc::O_NONBLOCK)?;let target=scoped_file(&self.lease.0.root,self.prepared.path())?;
  let native=identity(&anchor)?;if native.device!=proof["dev"].as_u64().ok_or_else(||fail("Selected rewrite anchor device absent"))?||native.inode!=proof["ino"].as_u64().ok_or_else(||fail("Selected rewrite anchor inode absent"))?{return Err(fail("Selected rewrite anchor original identity differs"));}
  for file in [&anchor,&original,&target]{let m=file.metadata()?;if !m.is_file()||m.mode()&0o077!=0||m.nlink()!=3||identity(file)?!=native{return Err(fail("Selected rewrite complete result alias group differs"));}}
  Ok(())
 }
}

// The canonical writer intentionally retains its proof temp + stable hardlink.
// Upload-envelope's single-link reader is inappropriate for this distinct role.
// Count every original inode alias in this exact held directory; no external link
// or unrelated record can be silently admitted by the selected rewrite proof.
fn proof_record(directory:&File)->io::Result<(File,Vec<u8>)>{
 use std::os::unix::fs::FileExt;
 let file=stage::open(directory,"install-proof.json",libc::O_RDONLY|libc::O_NONBLOCK)?;let meta=file.metadata()?;
 if !meta.is_file()||meta.mode()&0o077!=0||meta.nlink()!=2||meta.len()>65536{return Err(fail("Selected rewrite proof role is not private complete pair"));}
 let mut raw=vec![0u8;meta.len()as usize];file.read_exact_at(&mut raw,0)?;let proof:serde_json::Value=decode(&raw)?;let temp=proof["temp"].as_str().ok_or_else(||fail("Selected rewrite anchor role absent"))?;
 if !temp.strip_prefix("anchor-").is_some_and(uuid){return Err(fail("Selected rewrite anchor role malformed"));}
 let mut aliases=BTreeSet::new();let members=names(directory)?;for name in &members{let other=stage::open(directory,name,libc::O_RDONLY|libc::O_NONBLOCK)?;if identity(&other)?==identity(&file)?{aliases.insert(name.clone());}}
 if aliases.len()!=2||!aliases.contains("install-proof.json")||!aliases.iter().filter(|n|n.as_str()!="install-proof.json").all(|n|n.strip_prefix("proof-").is_some_and(uuid)){return Err(fail("Selected rewrite proof alias census differs"));}
 let expected=aliases.into_iter().chain(["manifest.json".to_owned(),"payload".into(),"install-anchor".into(),temp.into()]).collect::<BTreeSet<_>>();
 if members!=expected||stamp(&file)?!=stamp(&stage::open(directory,"install-proof.json",libc::O_RDONLY|libc::O_NONBLOCK)?)?{return Err(fail("Selected rewrite directory contains unproved records"));}
 Ok((file,raw))
}
