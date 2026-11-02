// Read-only original Replay custody. No runtime lease, serving token or disposal release.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ColdRewriteManifest {
 op_id:String,space_id:String,document_id:String,path:String,revision:String,sha256:String,length:i64,version:u32,
 kind:String,actor_id:String,source_path:String,source_revision:String,published_stamp:String,source_sha256:Option<String>,
 #[serde(default)] result_source_digest:Option<String>,#[serde(default)] original_source_digest:Option<String>,
 reason:crate::storage::canonical_rewrite::RewriteReason,incoming:crate::storage::canonical_rewrite::IncomingIdentity,
}
pub(crate) struct ColdReplayEvidence {producer:Producer,origin:ReplayBlobOrigin,rewrite:Option<(ReplayRewritePlan,ColdRewriteManifest)>}
pub(crate) fn validate_captured_replay_custody(scope:&crate::storage::restoration::admission::AdmissionOwnerScope,raws:&std::collections::BTreeMap<crate::storage::restoration::admission::FileRole,Vec<u8>>)->io::Result<ColdReplayEvidence> {
 use crate::storage::restoration::admission::{DirectoryRole as D,FileRole as F};
 let claim:Claim=decode(scope.original_claim_json.as_bytes())?;
 let Producer::Replay{provider,incoming_id,actor_id,space_id,document_id,asset_id,payload_fingerprint,binary_sha256,byte_length,..}=&claim.producer else{return Err(fail("Direct producer cannot claim cold Replay"));};
 let get=|role:F|->io::Result<&[u8]>{raws.get(&role).map(Vec::as_slice).ok_or_else(||fail("Missing cold original Replay record"))};
 if get(F::Claim)?!=scope.original_claim_json.as_bytes(){return Err(fail("Cold Replay original claim bytes differ"));}
 let origin:ReplayBlobOrigin=decode(get(F::CapsuleRecord{name:"replay-blob-origin.json".into()})?)?;
 let parts:Vec<_>=origin.path.split('/').collect();
 if origin.version!=1||origin.claim_sha256!=scope.admission_claim_sha256||origin.expected_length!=*byte_length||origin.expected_sha256!=*binary_sha256
  ||parts.len()!=5||parts[0]!="spaces"||parts[2]!="attachments"||parts[3]!=document_id {return Err(fail("Cold Replay original blob producer/path differs"));}
 let parent=origin.path.rsplit_once('/').ok_or_else(||fail("Cold Replay original parent path invalid"))?.0;
 let directory=|role:&D|->io::Result<Identity>{let b=scope.directories.iter().find(|b|&b.role==role).ok_or_else(||fail("Missing original Replay directory role"))?;Ok(Identity{device:b.original.device,inode:b.original.inode})};
 let file=|role:&F|->io::Result<Identity>{let b=scope.files.iter().find(|b|&b.role==role).ok_or_else(||fail("Missing original Replay file role"))?;Ok(Identity{device:b.original.device,inode:b.original.inode})};
 let blob_role=F::ReplayBlob{relative:origin.path.clone()};let blob=get(blob_role.clone())?;
 if directory(&D::ReplayBlobParent{relative:parent.into()})?!=origin.parent||file(&blob_role)?!=origin.payload||blob.len() as u64>origin.expected_length
  ||(blob.len() as u64==origin.expected_length&&digest(blob)!=origin.expected_sha256){return Err(fail("Cold Replay original blob custody or completion bytes differ"));}
 let mut dirs=std::collections::BTreeSet::from([D::DataRoot,D::AdmissionNamespace,D::AdmissionCapsule,D::ReplayBlobParent{relative:parent.into()}]);
 let mut files=std::collections::BTreeSet::from([F::Claim,F::CapsuleRecord{name:"replay-blob-origin.json".into()},blob_role]);
 let plan_role=F::CapsuleRecord{name:"replay-rewrite-plan.json".into()};
 let rewrite=if let Some(raw)=raws.get(&plan_role){
  let plan:ReplayRewritePlan=decode(raw)?;
  if plan.version!=1||plan.claim_sha256!=scope.admission_claim_sha256||!uuid(&plan.op_id)||plan.provider!=*provider||plan.incoming_id!=*incoming_id
   ||plan.actor_id!=*actor_id||plan.space_id!=*space_id||plan.document_id!=*document_id||plan.asset_id!=*asset_id||plan.payload_fingerprint!=*payload_fingerprint {return Err(fail("Cold Replay original rewrite plan producer differs"));}
  let origin_role=F::CapsuleRecord{name:"replay-rewrite-origin.json".into()};let rewrite_origin:ReplayRewriteOrigin=decode(get(origin_role.clone())?)?;
  if rewrite_origin.version!=1||rewrite_origin.claim_sha256!=scope.admission_claim_sha256||rewrite_origin.plan_sha256!=digest(raw)||rewrite_origin.op_id!=plan.op_id {return Err(fail("Cold Replay incomplete rewrite original claim/plan differs"));}
  let manifest:ColdRewriteManifest=decode(rewrite_origin.manifest_json.as_bytes())?;let incoming=&manifest.incoming;
  if manifest.version!=3||manifest.kind!="canonical-rewrite"||manifest.op_id!=plan.op_id||manifest.space_id!=plan.space_id||manifest.document_id!=plan.document_id
   ||manifest.actor_id!=plan.actor_id||manifest.source_path!=plan.source_path||manifest.source_revision!=plan.source_revision||manifest.published_stamp!=plan.published_stamp
   ||manifest.revision!=plan.revision||manifest.source_sha256!=plan.source_sha256||manifest.sha256!=plan.result_sha256||manifest.length<0||manifest.length>6_000_000||manifest.length as u64!=plan.result_length
   ||manifest.result_source_digest.is_some()||manifest.original_source_digest.is_some()||manifest.reason!=crate::storage::canonical_rewrite::RewriteReason::AssetReferenceRewrite
   ||incoming.provider!=plan.provider||incoming.operation_id!=plan.incoming_id||incoming.entity_type!="asset"||incoming.entity_id!=plan.asset_id||incoming.action!="create"||incoming.payload_fingerprint!=plan.payload_fingerprint
   ||!manifest.path.starts_with(&format!("spaces/{}/docs/revisions/",parts[1]))||manifest.path!=format!("spaces/{}/docs/revisions/{}.md",parts[1],plan.op_id)
   ||plan.source_revision.is_empty()||plan.published_stamp.is_empty()||plan.revision.is_empty()||plan.source_sha256.as_ref().is_none_or(|h|h.len()!=64||!h.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))) {return Err(fail("Cold Replay incomplete rewrite original manifest/typed producer differs"));}
  let ns=D::CanonicalRewriteNamespace;let dir=D::CanonicalRewriteDirectory{operation_id:plan.op_id.clone()};let payload=F::CanonicalRewrite{operation_id:plan.op_id.clone(),name:"payload".into()};let bytes=get(payload.clone())?;
  if directory(&ns)?!=rewrite_origin.namespace||directory(&dir)?!=rewrite_origin.parent||file(&payload)?!=rewrite_origin.payload||bytes.len() as u64>plan.result_length
   ||(bytes.len() as u64==plan.result_length&&digest(bytes)!=plan.result_sha256){return Err(fail("Cold Replay incomplete rewrite original payload custody differs"));}
  let manifest_role=F::CanonicalRewrite{operation_id:plan.op_id.clone(),name:"manifest.json".into()};
  if let Some(raw)=raws.get(&manifest_role){if raw.as_slice()!=rewrite_origin.manifest_json.as_bytes(){return Err(fail("Cold Replay original protected manifest substituted"));}files.insert(manifest_role);}
  dirs.insert(ns);dirs.insert(dir);files.extend([plan_role,origin_role,payload]);Some((plan,manifest))
 }else{None};
 if scope.directories.iter().map(|b|b.role.clone()).collect::<std::collections::BTreeSet<_>>()!=dirs||raws.keys().cloned().collect::<std::collections::BTreeSet<_>>()!=files {return Err(fail("Cold Replay role/record inventory omitted or added proof"));}
 Ok(ColdReplayEvidence{producer:claim.producer,origin,rewrite})
}
impl ColdReplayEvidence {
 pub(crate) async fn validate_references(&self,conn:&mut sqlx::AnyConnection,inventory:&std::collections::BTreeSet<String>,read:&(dyn Fn(&str)->io::Result<Vec<u8>>+Send+Sync))->anyhow::Result<()> {
  let Producer::Replay{provider,incoming_id,actor_id,space_id,document_id,asset_id,payload_fingerprint,binary_sha256,byte_length,..}=&self.producer else{anyhow::bail!("Wrong cold Replay producer")};
  for (table,column) in [("attachments","storage_path"),("attachment_version_records","original_storage_path"),("move_asset_path_aliases","old_path"),("asset_replay_receipts","storage_path"),("attachment_retained_byte_ledger","original_storage_path")] {
   let count:i64=sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE {column}=$1")).bind(&self.origin.path).fetch_one(&mut *conn).await?;anyhow::ensure!(count==0,"Captured retained/current SQL association protects original Replay blob");
  }
  if let Some(retired)=crate::storage::attachment_retirement::read(conn,asset_id).await.map_err(anyhow::Error::msg)?{anyhow::ensure!(retired.association.4!=self.origin.path,"Captured retirement protects original Replay blob");}
  let prior:Option<String>=sqlx::query_scalar("SELECT receipt_json FROM asset_replay_receipts WHERE provider=$1 AND incoming_id=$2").bind(provider).bind(incoming_id).fetch_optional(&mut *conn).await?;
  if let Some(raw)=prior {let winner:crate::sync::AssetReplayReceipt=serde_json::from_str(&raw)?;anyhow::ensure!(winner.incoming.provider==*provider&&winner.incoming.operation_id==*incoming_id&&winner.incoming.entity_type=="asset"&&winner.incoming.action=="create"&&winner.incoming.entity_id==*asset_id&&winner.incoming.payload_fingerprint==*payload_fingerprint&&winner.actor_id==*actor_id&&winner.space_id==*space_id&&winner.document_id==*document_id&&winner.binary_sha256==*binary_sha256&&winner.byte_length==*byte_length as i64,"Captured Replay winning receipt producer differs");anyhow::ensure!(crate::sync::asset_receipt_current(conn,&winner).await.map_err(anyhow::Error::msg)?&&winner.storage_path!=self.origin.path,"Captured exact winning receipt protects original blob or lacks full SQL proof");}
  let url=format!("/api/v1/files/{}",self.origin.path);
  let contains=|path:&str|->anyhow::Result<bool>{anyhow::ensure!(inventory.contains(path),"Captured canonical reference file absent");let raw=read(path)?;anyhow::ensure!(raw.len()<=6_000_000,"Captured canonical source exceeds bound");Ok(raw.windows(url.len()).any(|w|w==url.as_bytes()))};
  for path in inventory.iter().filter(|p|p.starts_with(".canonical-rewrite-journal/")&&p.ends_with("/payload")) {if contains(path)?{let op=path.split('/').nth(1).ok_or_else(||fail("Invalid protected canonical stage"))?;anyhow::ensure!(self.rewrite.as_ref().is_some_and(|(p,_)|p.op_id==op),"Captured other protected canonical stage references Replay blob");}}
  let drafts:Vec<String>=sqlx::query_scalar("SELECT content FROM document_drafts").fetch_all(&mut *conn).await?;anyhow::ensure!(!drafts.iter().any(|s|s.contains(&url)),"Captured draft source protects Replay blob");
  for query in ["SELECT content_path FROM documents","SELECT content_path FROM canonical_revision_receipts"] {let paths:Vec<String>=sqlx::query_scalar(query).fetch_all(&mut *conn).await?;for path in paths{anyhow::ensure!(!contains(&path)?,"Captured current/historical canonical source protects Replay blob");}}
  if let Some((plan,manifest))=&self.rewrite {
   anyhow::ensure!(!inventory.contains(&manifest.path),"Captured incomplete rewrite result exists without original install proof");
   let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM canonical_revision_receipts WHERE op_id=$1 OR content_path=$2").bind(&plan.op_id).bind(&manifest.path).fetch_one(&mut *conn).await?;anyhow::ensure!(count==0,"Captured canonical receipt protects incomplete rewrite");
   let row:Option<(String,String,String)>=sqlx::query_as("SELECT content_path,updated_at,published_at FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL").bind(document_id).bind(space_id).fetch_optional(&mut *conn).await?;anyhow::ensure!(row==Some((plan.source_path.clone(),plan.source_revision.clone(),plan.published_stamp.clone())),"Captured original canonical source SQL changed");let raw=read(&plan.source_path)?;anyhow::ensure!(Some(digest(&raw))==plan.source_sha256,"Captured original canonical source bytes changed");
  }
  Ok(())
 }
}
