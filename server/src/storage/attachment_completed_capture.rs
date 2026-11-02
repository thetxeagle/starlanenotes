//! Private source-held completed absence transport. No cold/writer constructor exists here.
use super::*;
use serde::{Deserialize,Serialize};

#[derive(Clone,Debug,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(tag="role",rename_all="snake_case",deny_unknown_fields)]
pub(crate) enum CompletedDirectoryRole {DataRoot,AdmissionNamespace,AdmissionCapsule,DirectStageNamespace,DirectStageDirectory,InstalledParent{relative:String}}
impl CompletedDirectoryRole {
 pub(crate) fn relative(&self,attempt:&str)->anyhow::Result<Option<String>>{
  ensure!(uuid(attempt),"Completed attempt identity malformed");
  Ok(match self{Self::DataRoot=>None,Self::AdmissionNamespace=>Some(NS.into()),Self::AdmissionCapsule=>Some(format!("{NS}/{attempt}")),Self::DirectStageNamespace=>Some(stage::NAMESPACE.into()),Self::DirectStageDirectory=>Some(format!("{}/{attempt}",stage::NAMESPACE)),Self::InstalledParent{relative}=>{ensure!(crate::storage::is_safe_relative_path(std::path::Path::new(relative))&&relative.split('/').all(|p|!p.is_empty()&&p!="."&&p!=".."&&!p.contains('\\'))&&!relative.contains('\0')&&!relative.is_empty()&&!relative.starts_with('/'),"Completed installed parent malformed");Some(relative.clone())}})
 }
}
const COLUMNS:&[&str]=&["deletion_operation_id","asset_id","actor_id","space_id","document_id","producer_kind","original_attempt_id","plan_sha256","original_receipt_sha256","admission_claim_sha256","native_intent_json","native_intent_sha256","native_tombstone_json","native_tombstone_sha256","owned_path_set_json","owned_path_set_sha256","disposition","completed_at"];
#[derive(Clone,Debug,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(tag="role",rename_all="snake_case",deny_unknown_fields)]
pub(crate) enum CompletedFileRole {Claim,StageOrigin,CommittedIntent,CommittedTombstone,DirectEnvelope{name:String}}
impl CompletedFileRole {
 pub(crate) fn relative(&self,attempt:&str)->anyhow::Result<String>{
  ensure!(uuid(attempt),"Completed attempt identity malformed");
  let name=match self {Self::Claim=>"claim.json",Self::StageOrigin=>"stage-origin.json",Self::CommittedIntent=>"committed-erasure-intent.json",Self::CommittedTombstone=>"committed-erasure-tombstone.json",Self::DirectEnvelope{name}=>{ensure!(["reservation.json","request.json","seal.json","intent.json","install-proof.json"].contains(&name.as_str()),"Unknown completed stage role");name.as_str()}};
  let namespace=if matches!(self,Self::DirectEnvelope{..}){stage::NAMESPACE}else{NS};
  Ok(format!("{namespace}/{attempt}/{name}"))
 }
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompletedCaptureDirectory{pub role:CompletedDirectoryRole,pub original:crate::storage::restoration::admission::Identity,pub captured:crate::storage::restoration::admission::Identity}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompletedFileBinding{pub role:CompletedFileRole,pub original:crate::storage::restoration::admission::Identity,pub captured:crate::storage::restoration::admission::Identity,pub length:u64,pub sha256:String}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompletedAbsenceScope {
 #[serde(deserialize_with="unique_disposition")]
 pub disposition:BTreeMap<String,String>,pub reservation:crate::storage::restoration::admission::CapturedReservation,pub original_plan_json:String,pub original_claim_json:String,
 pub original_stage_origin_json:String,pub directories:Vec<CompletedCaptureDirectory>,pub files:Vec<CompletedFileBinding>,
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompletedAbsenceEnvelope {pub version:u32,pub source_kind:String,pub scopes:Vec<CompletedAbsenceScope>}
fn unique_disposition<'de,D:serde::Deserializer<'de>>(d:D)->Result<BTreeMap<String,String>,D::Error>{crate::storage::backup_inventory::UniqueMap::<String>::deserialize(d).map(|m|m.0)}
impl CompletedAbsenceEnvelope {
 pub(crate) fn validate_shape(&self)->anyhow::Result<()> {
  ensure!(self.version==1&&self.source_kind=="original_native_direct_v1"&&!self.scopes.is_empty()&&self.scopes.len()<=512,"Unsupported completed capture envelope");
  ensure!(serde_json::to_vec(self)?.len()<=8*1024*1024,"Completed capture exceeds typed bound");
  let mut assets=std::collections::BTreeSet::new();let mut attempts=std::collections::BTreeSet::new();
  for scope in &self.scopes {
   ensure!(scope.disposition.len()==COLUMNS.len()&&COLUMNS.iter().all(|k|scope.disposition.contains_key(*k)),"Completed exact SQL shape differs");
   ensure!(assets.insert(scope.disposition["asset_id"].clone())&&attempts.insert(scope.reservation.operation_id.clone())&&uuid(&scope.reservation.operation_id),"Duplicate or malformed completed scope");
   ensure!(scope.files.len()==9&&scope.directories.len()==6,"Completed role inventory differs");
   for file in &scope.files {file.role.relative(&scope.reservation.operation_id)?;ensure!(file.length<=16384&&valid_hash(&file.sha256),"Completed file bound differs");}
   for dir in &scope.directories {dir.role.relative(&scope.reservation.operation_id)?;}
  }Ok(())
 }
}
// Holds original native leases/census; an envelope cannot construct this token.
pub(crate) struct NativeCompletedCapture {
 held:CompletedAccounting,envelope:CompletedAbsenceEnvelope,
 root:std::path::PathBuf,paths:BTreeMap<String,(String,i64)>,plans:BTreeMap<String,String>,
 old:Option<CompletedAbsenceEnvelope>,
}
async fn rows(conn:&mut AnyConnection)->anyhow::Result<Vec<BTreeMap<String,String>>>{
 let rows=sqlx::query("SELECT * FROM attachment_erasure_dispositions ORDER BY asset_id LIMIT 513").fetch_all(conn).await?;
 ensure!(rows.len()<=512,"Completed transport row bound exceeded");
 rows.into_iter().map(|r|COLUMNS.iter().map(|k|Ok(((*k).into(),r.try_get::<String,_>(*k)?))).collect()).collect()
}
fn source_bindings(owner:&OwnedErasure)->anyhow::Result<(Vec<CompletedCaptureDirectory>,Vec<CompletedFileBinding>)>{
 let c=&owner.lease.0.claim;
 let raw=owner.intent.owned.stage_origin_json.as_ref().context("Completed original stage origin absent")?;
 let o:StageOrigin=decode(raw.as_bytes())?;
 let namespace=stage::open(&owner.lease.0.root,stage::NAMESPACE,libc::O_RDONLY|libc::O_DIRECTORY)?;
 let (_,directory)=owner.lease.stage_directory()?.context("Completed original stage directory absent")?;
 let mut directories=vec![];
 for(role,held,original)in [(CompletedDirectoryRole::DataRoot,&owner.lease.0.root,&c.root),(CompletedDirectoryRole::AdmissionNamespace,&owner.lease.0.namespace,&c.namespace),(CompletedDirectoryRole::AdmissionCapsule,&owner.lease.0.directory,&c.capsule),(CompletedDirectoryRole::DirectStageNamespace,&namespace,&o.namespace),(CompletedDirectoryRole::DirectStageDirectory,&directory,&o.directory)]{
  ensure!(identity(held)?==*original,"Completed original source directory custody differs");let id=crate::storage::restoration::admission::Identity{device:original.device,inode:original.inode};
  directories.push(CompletedCaptureDirectory{role,original:id.clone(),captured:id});
 }
 for (dir,expected)in [(&owner.lease.0.directory,vec!["claim.json","stage-origin.json","committed-erasure-intent.json","committed-erasure-tombstone.json"]),(&directory,vec!["reservation.json","request.json","seal.json","intent.json","install-proof.json"])] {
  use std::os::fd::AsRawFd;
  let names=std::fs::read_dir(format!("/proc/self/fd/{}",dir.as_raw_fd()))?.map(|e|Ok(e?.file_name().into_string().map_err(|_|fail("NonUTF8 completed source entry"))?)).collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
  ensure!(names==expected.into_iter().map(str::to_owned).collect(),"Completed source directory membership differs");
 }
 let (_,_,installed_parent,_,_)=owner.paths()?;let id=identity(&installed_parent)?;let id=crate::storage::restoration::admission::Identity{device:id.device,inode:id.inode};
 let relative=owner.intent.owned.installed_path.as_ref().context("Completed installed path absent")?.rsplit_once('/').context("Completed installed parent absent")?.0.to_owned();
 directories.push(CompletedCaptureDirectory{role:CompletedDirectoryRole::InstalledParent{relative},original:id.clone(),captured:id});
 let mut files=vec![];
 for(name,dir,role)in [
  ("claim.json",&owner.lease.0.directory,CompletedFileRole::Claim),
  ("stage-origin.json",&owner.lease.0.directory,CompletedFileRole::StageOrigin),
  ("committed-erasure-intent.json",&owner.lease.0.directory,CompletedFileRole::CommittedIntent),
  ("committed-erasure-tombstone.json",&owner.lease.0.directory,CompletedFileRole::CommittedTombstone),
  ("reservation.json",&directory,CompletedFileRole::DirectEnvelope{name:"reservation.json".into()}),
  ("request.json",&directory,CompletedFileRole::DirectEnvelope{name:"request.json".into()}),
  ("seal.json",&directory,CompletedFileRole::DirectEnvelope{name:"seal.json".into()}),
  ("intent.json",&directory,CompletedFileRole::DirectEnvelope{name:"intent.json".into()}),
  ("install-proof.json",&directory,CompletedFileRole::DirectEnvelope{name:"install-proof.json".into()}),
 ]{
  let (f,raw)=bytes(dir,name)?;let id=identity(&f)?;let id=crate::storage::restoration::admission::Identity{device:id.device,inode:id.inode};
  files.push(CompletedFileBinding{role,original:id.clone(),captured:id,length:raw.len()as u64,sha256:digest(&raw)});
 }
 owner.verify()?;Ok((directories,files))
}
fn source_raws(owner:&OwnedErasure,scope:&CompletedAbsenceScope)->anyhow::Result<BTreeMap<CompletedFileRole,Vec<u8>>>{
 use std::os::unix::fs::FileExt;
 let mut raws=BTreeMap::new();
 for b in &scope.files {ensure!(b.length<=16384,"Completed source record exceeds typed bound");let path=b.role.relative(&owner.lease.0.claim.attempt_id)?;let f=scoped_file(&owner.lease.0.root,&path)?;let id=identity(&f)?;ensure!(id.device==b.original.device&&id.inode==b.original.inode&&f.metadata()?.len()==b.length,"Completed held source record custody differs");let mut raw=vec![0;b.length as usize];f.read_exact_at(&mut raw,0)?;ensure!(digest(&raw)==b.sha256,"Completed source record hash differs");raws.insert(b.role.clone(),raw);}
 owner.verify()?;Ok(raws)
}
pub(crate) async fn reservation_in(conn:&mut AnyConnection,attempt:&str)->anyhow::Result<crate::storage::restoration::admission::CapturedReservation>{
 let r=sqlx::query("SELECT operation_id,request_sha256,admission_claim_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,disposition_json,disposition_sha256 FROM attachment_capacity_reservations WHERE operation_id=$1").bind(attempt).fetch_one(conn).await?;
 Ok(crate::storage::restoration::admission::CapturedReservation{operation_id:r.try_get(0)?,request_sha256:r.try_get(1)?,admission_claim_sha256:r.try_get(2)?,maximum_bytes:r.try_get(3)?,state:r.try_get(4)?,committed_asset_id:r.try_get(5)?,committed_bytes:r.try_get(6)?,disposition_json:r.try_get(7)?,disposition_sha256:r.try_get(8)?})
}
impl NativeCompletedCapture {
 pub(crate) async fn capture_in(root:&std::path::Path,conn:&mut AnyConnection,paths:BTreeMap<String,(String,i64)>,borrowed:&[WriterLease])->anyhow::Result<Self>{
  Self::capture_inner(root,conn,paths,borrowed,false).await
 }
 pub(crate) async fn capture_native_additions_in(root:&std::path::Path,conn:&mut AnyConnection,paths:BTreeMap<String,(String,i64)>)->anyhow::Result<Self>{Self::capture_inner(root,conn,paths,&[],true).await}
 async fn capture_inner(root:&std::path::Path,conn:&mut AnyConnection,paths:BTreeMap<String,(String,i64)>,borrowed:&[WriterLease],mixed:bool)->anyhow::Result<Self>{
  // Caller keeps the global stopped-writer transaction through producer publication.
  let held=super::capture_in_at(root,conn,paths.clone(),borrowed).await?;held.fence()?;ensure!(mixed==held.restored.is_some(),"Native source class differs; restored scopes require explicit additions proof");
  let old=if mixed{let q=held.roster.as_ref().context("Mixed complete roster absent")?.qualified().context("Mixed original source token absent")?;let prior=q.prior_completed_evidence()?;Some(CompletedAbsenceEnvelope{version:1,source_kind:"original_native_direct_v1".into(),scopes:prior.roles.scopes.iter().map(|s|s.original.clone()).collect()})}else{None};
  let sql=rows(conn).await?.into_iter().filter(|r|!old.as_ref().is_some_and(|e|e.scopes.iter().any(|s|s.disposition["asset_id"]==r["asset_id"]))).collect::<Vec<_>>();ensure!(!mixed||!sql.is_empty(),"Mixed source requires genuine new completed scopes");ensure!(sql.len()==held.observations.len(),"Completed source inventory changed");
  let mut scopes=vec![];let mut plans=BTreeMap::new();let mut transport_bytes=0usize;
  for (row,observation) in sql.into_iter().zip(&held.observations){
   let owner=&observation.owner;owner.lease.matches_root(root)?;let receipt:Receipt=serde_json::from_str(&owner.intent.receipt_json)?;
   ensure!(row["asset_id"]==receipt.intent.asset_id,"Completed source order differs");
   let reference=coordinator::references::prove_in_at(root,conn,None,&row["deletion_operation_id"],&receipt,paths.clone(),false).await.map_err(|e|anyhow::anyhow!("{e:?}"))?;
   transport_bytes=transport_bytes.checked_add(reference.plan_raw.len()).and_then(|n|n.checked_add(row.values().map(String::len).sum::<usize>())).context("Completed transport size overflow")?;ensure!(transport_bytes<=8*1024*1024,"Completed transport exceeds bounded original proof inventory");
   ensure!(digest(reference.plan_raw.as_bytes())==row["plan_sha256"],"Completed source plan changed");plans.insert(row["asset_id"].clone(),reference.plan_raw.clone());
   let claim=String::from_utf8(owner.lease.original_claim().to_vec())?;
   let origin=owner.intent.owned.stage_origin_json.clone().context("Completed original stage origin unavailable")?;
   let (directories,files)=source_bindings(owner)?;
   let reservation=reservation_in(conn,&receipt.intent.attempt_id).await?;
   let scope=CompletedAbsenceScope{disposition:row,reservation,original_plan_json:reference.plan_raw,original_claim_json:claim,original_stage_origin_json:origin,directories,files};
   let raws=source_raws(owner,&scope)?;validate_original_records(&scope,&raws)?;scopes.push(scope);
  }
  let token=Self{held,envelope:CompletedAbsenceEnvelope{version:1,source_kind:"original_native_direct_v1".into(),scopes},root:root.to_owned(),paths,plans,old};token.fence_in(conn).await?;Ok(token)
 }
 pub(crate) async fn mixed_evidence_in(&self,conn:&mut AnyConnection)->anyhow::Result<crate::storage::restoration::linux::MixedCompletedCaptureEvidence>{
  self.fence_in(conn).await?;ensure!(self.old.is_some(),"Native-only token cannot emit mixed evidence");let q=self.held.roster.as_ref().context("Mixed complete roster absent")?.qualified().context("Mixed original source token absent")?;q.fence_in(conn).await?;
  let result=crate::storage::restoration::linux::MixedCompletedCaptureEvidence{version:1,prior:q.prior_completed_evidence()?,native:self.envelope.clone()};result.validate_shape()?;self.fence_in(conn).await?;Ok(result)
 }
 pub(crate) async fn fence_in(&self,conn:&mut AnyConnection)->anyhow::Result<()>{
  self.envelope.validate_shape()?;self.held.fence()?;let mut expected=self.envelope.scopes.iter().map(|s|s.disposition.clone()).collect::<Vec<_>>();if let Some(old)=&self.old{expected.extend(old.scopes.iter().map(|s|s.disposition.clone()));self.held.fence_restored_in(&self.root,conn).await?;}expected.sort_by(|a,b|a["asset_id"].cmp(&b["asset_id"]));ensure!(rows(conn).await?==expected,"Completed source exact SQL changed");
  if let Some(old)=&self.old{let mut attempts=old.scopes.iter().chain(&self.envelope.scopes).map(|s|s.reservation.operation_id.clone()).collect::<Vec<_>>();attempts.sort();ensure!(attempts.windows(2).all(|s|s[0]!=s[1]),"Mixed native scope duplicates restored attempt");let actual:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id").fetch_all(&mut *conn).await?;ensure!(actual==attempts,"Mixed native complete reservation census differs");}
  for (scope,observation) in self.envelope.scopes.iter().zip(&self.held.observations) {
   let raws=source_raws(&observation.owner,scope)?;validate_original_records(scope,&raws)?;
   let (directories,files)=source_bindings(&observation.owner)?;ensure!(directories==scope.directories&&files==scope.files,"Completed native source custody changed");
   ensure!(reservation_in(conn,&scope.reservation.operation_id).await?==scope.reservation,"Completed source full committed reservation changed");
   let asset=&scope.disposition["asset_id"];let reference=coordinator::references::prove_in_at(&self.root,conn,None,&scope.disposition["deletion_operation_id"],&self.held.originals[asset],self.paths.clone(),false).await.map_err(|e|anyhow::anyhow!("{e:?}"))?;
   ensure!(self.plans.get(asset)==Some(&reference.plan_raw),"Completed source protected references/plan changed");
  }self.held.fence()?;Ok(())
 }
 pub(crate) fn fence(&self)->anyhow::Result<()> {
  self.envelope.validate_shape()?;self.held.fence()?;
  for(scope,observation)in self.envelope.scopes.iter().zip(&self.held.observations){
   let raws=source_raws(&observation.owner,scope)?;validate_original_records(scope,&raws)?;
   let(directories,files)=source_bindings(&observation.owner)?;ensure!(directories==scope.directories&&files==scope.files,"Completed held native source roles changed");
  }self.held.fence()?;Ok(())
 }
 pub(crate) fn covers_exact_lease(&self,lease:&WriterLease)->anyhow::Result<bool>{
  self.held.fence()?;
  Ok(self.held.observations.iter().any(|o|std::sync::Arc::ptr_eq(&o.owner.lease.0,&lease.0)))
 }
 pub(crate) async fn envelope_in(&self,conn:&mut AnyConnection)->anyhow::Result<CompletedAbsenceEnvelope>{ensure!(self.old.is_none(),"Mixed token cannot emit native-only transport");self.fence_in(conn).await?;Ok(self.envelope.clone())}
}
// Pure typed-record validation is evidence classification, never a native/cold token.
pub(crate) fn validate_original_records(scope:&CompletedAbsenceScope,raws:&BTreeMap<CompletedFileRole,Vec<u8>>)->anyhow::Result<Receipt>{
 ensure!(scope.disposition.len()==COLUMNS.len()&&COLUMNS.iter().all(|k|scope.disposition.contains_key(*k)),"Completed exact SQL shape differs");
 let row=&scope.disposition;
 let claim:Claim=decode(scope.original_claim_json.as_bytes())?;validate(&claim.producer,claim.maximum_bytes)?;
 ensure!(claim.version==1&&uuid(&claim.attempt_id)&&json(&claim)?==scope.original_claim_json.as_bytes()&&claim.producer_sha256==digest(&json(&claim.producer)?),"Completed original claim malformed");
 let Producer::Direct{request}=&claim.producer else{anyhow::bail!("Unsupported completed producer")};
 let intent:Intent=decode(row["native_intent_json"].as_bytes())?;
 let receipt:Receipt=decode(intent.receipt_json.as_bytes())?;
 crate::storage::attachment_upload::validate(&receipt.intent)?;
 let owned=&intent.owned;
 ensure!(intent.version==1&&json(&intent)?==row["native_intent_json"].as_bytes()&&intent.receipt_json.as_bytes()==json(&receipt)?&&intent.receipt_sha256==digest(intent.receipt_json.as_bytes()),"Completed original intent/receipt encoding differs");
 ensure!(owned.version==1&&owned.attempt_id==claim.attempt_id&&owned.claim_json==scope.original_claim_json&&owned.request_sha256==claim.producer_sha256&&owned.admission_claim_sha256==digest(scope.original_claim_json.as_bytes())&&owned.maximum_bytes==claim.maximum_bytes as i64,"Completed full claim differs");
 ensure!(receipt.intent.attempt_id==claim.attempt_id&&receipt.intent.request==*request&&owned.installed_path.as_deref()==Some(receipt.intent.storage_path.as_str())&&owned.payload_length==receipt.intent.byte_length as u64&&owned.payload_sha256.as_deref()==Some(receipt.intent.binary_sha256.as_str()),"Completed original winning receipt differs");
 let r=&scope.reservation;
 ensure!(r.operation_id==claim.attempt_id&&r.maximum_bytes==claim.maximum_bytes as i64&&r.request_sha256==claim.producer_sha256&&r.admission_claim_sha256==owned.admission_claim_sha256&&r.state=="committed"&&r.committed_asset_id.as_deref()==Some(receipt.intent.asset_id.as_str())&&r.committed_bytes==Some(receipt.intent.byte_length)&&r.disposition_json.is_none()&&r.disposition_sha256.is_none(),"Completed exact original committed SQL reservation differs");
 let origin:StageOrigin=decode(scope.original_stage_origin_json.as_bytes())?;
 ensure!(origin.version==1&&origin.claim_sha256==owned.admission_claim_sha256&&owned.stage_origin_json.as_deref()==Some(scope.original_stage_origin_json.as_str())&&owned.payload.as_ref()==Some(&origin.payload)&&origin.payload.inode!=0,"Completed original stage custody differs");
 let expected=[("asset_id",receipt.intent.asset_id.as_str()),("actor_id",request.actor_id.as_str()),("space_id",request.space_id.as_str()),("document_id",request.document_id.as_str()),("producer_kind","original_direct_v1"),("original_attempt_id",claim.attempt_id.as_str()),("deletion_operation_id",intent.deletion_operation.as_str()),("plan_sha256",intent.deletion_plan_sha256.as_str()),("original_receipt_sha256",intent.receipt_sha256.as_str()),("admission_claim_sha256",owned.admission_claim_sha256.as_str()),("disposition","owned_paths_erased_v1")];
 for(k,v)in expected{ensure!(row[k]==v,"Completed disposition binding differs: {k}");}
 ensure!(uuid(&intent.deletion_operation)&&row["plan_sha256"]==digest(scope.original_plan_json.as_bytes()),"Completed original plan checksum differs");
 let plan:crate::storage::attachment_erasure_plan::Plan=serde_json::from_str(&scope.original_plan_json)?;
 ensure!(plan.format_version==2&&plan.operation_id==intent.deletion_operation&&plan.actor_id==request.actor_id&&plan.space_id==request.space_id&&plan.document_id==request.document_id&&serde_json::to_string(&plan)?==scope.original_plan_json,"Completed original plan differs");
 let completed=chrono::DateTime::parse_from_rfc3339(&row["completed_at"])?;
 ensure!(completed>=chrono::DateTime::parse_from_rfc3339(&plan.created_at)?&&completed<=chrono::Utc::now(),"Completed original time differs");
 ensure!(row["native_tombstone_json"]==row["native_intent_json"]&&row["native_intent_sha256"]==digest(row["native_intent_json"].as_bytes())&&row["native_tombstone_sha256"]==row["native_intent_sha256"],"Completed tombstone differs");
 let expected_roles=[CompletedFileRole::Claim,CompletedFileRole::StageOrigin,CompletedFileRole::CommittedIntent,CompletedFileRole::CommittedTombstone].into_iter().chain(["reservation.json","request.json","seal.json","intent.json","install-proof.json"].into_iter().map(|name|CompletedFileRole::DirectEnvelope{name:name.into()})).collect::<std::collections::BTreeSet<_>>();
 ensure!(raws.keys().cloned().collect::<std::collections::BTreeSet<_>>()==expected_roles&&scope.files.len()==expected_roles.len(),"Completed raw role inventory differs");
 let mut seen=std::collections::BTreeSet::new();
 for binding in &scope.files {ensure!(seen.insert(binding.role.clone())&&expected_roles.contains(&binding.role)&&binding.original.inode!=0&&binding.captured.inode!=0&&binding.captured==binding.original,"Completed file role custody malformed");let raw=&raws[&binding.role];ensure!(raw.len()<=16384&&raw.len()as u64==binding.length&&digest(raw)==binding.sha256,"Completed raw bytes differ");}
 ensure!(raws[&CompletedFileRole::Claim]==scope.original_claim_json.as_bytes()&&raws[&CompletedFileRole::StageOrigin]==scope.original_stage_origin_json.as_bytes()&&raws[&CompletedFileRole::CommittedIntent]==row["native_intent_json"].as_bytes()&&raws[&CompletedFileRole::CommittedTombstone]==row["native_tombstone_json"].as_bytes(),"Completed raw capsule differs");
 for name in ["reservation.json","request.json","seal.json","intent.json","install-proof.json"] {let raw=&raws[&CompletedFileRole::DirectEnvelope{name:name.into()}];ensure!(owned.envelopes.get(name).map(|s|s.as_bytes())==Some(raw.as_slice()),"Completed immutable original stage envelope differs");}
 let rr=&raws[&CompletedFileRole::DirectEnvelope{name:"reservation.json".into()}];let rq=&raws[&CompletedFileRole::DirectEnvelope{name:"request.json".into()}];let seal_raw=&raws[&CompletedFileRole::DirectEnvelope{name:"seal.json".into()}];
 let reservation:stage::Reservation=decode(rr)?;let seal:stage::Seal=decode(seal_raw)?;
 ensure!(reservation.version==1&&reservation.operation_id==claim.attempt_id&&reservation.actor_id==request.actor_id&&reservation.space_id==request.space_id&&reservation.document_id==request.document_id&&reservation.original_name==request.original_name&&reservation.mime_type==request.mime_type&&reservation.maximum_bytes==claim.maximum_bytes&&rq.as_slice()==json(request)?,"Completed reservation/request differs");
 ensure!(origin.reservation_sha256==digest(rr)&&origin.request_sha256==digest(rq)&&seal.version==1&&seal.reservation_sha256==digest(rr)&&seal.payload_sha256==receipt.intent.binary_sha256&&seal.length==owned.payload_length,"Completed original seal differs");
 let ir=&raws[&CompletedFileRole::DirectEnvelope{name:"intent.json".into()}];let pr=&raws[&CompletedFileRole::DirectEnvelope{name:"install-proof.json".into()}];let history=crate::storage::attachment_upload::validate_history(ir,pr,&claim.attempt_id)?;
 ensure!(history.intent==receipt.intent&&history.original_identity==(origin.payload.device,origin.payload.inode)&&receipt.intent_sha256==digest(ir)&&receipt.install_proof_sha256==digest(pr),"Completed original immutable install proof differs");
 let expected_dirs=[(CompletedDirectoryRole::DataRoot,&claim.root),(CompletedDirectoryRole::AdmissionNamespace,&claim.namespace),(CompletedDirectoryRole::AdmissionCapsule,&claim.capsule),(CompletedDirectoryRole::DirectStageNamespace,&origin.namespace),(CompletedDirectoryRole::DirectStageDirectory,&origin.directory)];
 ensure!(scope.directories.len()==expected_dirs.len()+1,"Completed directory role inventory differs");let mut seen_dirs=std::collections::BTreeSet::new();
 for (role,id)in expected_dirs {let b=scope.directories.iter().find(|b|b.role==role).context("Missing completed directory role")?;ensure!(seen_dirs.insert(role)&&b.original.device==id.device&&b.original.inode==id.inode&&id.inode!=0&&b.captured.inode!=0&&b.captured==b.original,"Completed original directory role differs");}
 let parent=receipt.intent.storage_path.rsplit_once('/').context("Completed installed path parent absent")?.0;
 let parent_role=CompletedDirectoryRole::InstalledParent{relative:parent.into()};parent_role.relative(&claim.attempt_id)?;
 let parent=scope.directories.iter().find(|b|b.role==parent_role).context("Missing completed installed parent role")?;
 ensure!(parent.original.inode!=0&&parent.captured==parent.original,"Completed installed parent source custody differs");
 let material:serde_json::Value=serde_json::from_str(&row["owned_path_set_json"])?;let observed=material["family_observation_sha256"].as_str().context("Completed original census hash absent")?;ensure!(valid_hash(observed),"Completed census hash malformed");
 let expected=serde_json::json!({"version":1,"original_claim_sha256":owned.admission_claim_sha256,"original_receipt_sha256":intent.receipt_sha256,"native_intent_sha256":row["native_intent_sha256"],"family_observation_sha256":observed,"original_stage_origin_sha256":digest(scope.original_stage_origin_json.as_bytes()),"paths":[{"relative":receipt.intent.storage_path,"device":origin.payload.device,"inode":origin.payload.inode,"byte_length":owned.payload_length,"sha256":owned.payload_sha256},{"relative":format!("{}/{}/payload.part",stage::NAMESPACE,claim.attempt_id),"device":origin.payload.device,"inode":origin.payload.inode,"byte_length":owned.payload_length,"sha256":owned.payload_sha256}]});
 ensure!(material==expected&&serde_json::to_string(&material)?==row["owned_path_set_json"]&&digest(row["owned_path_set_json"].as_bytes())==row["owned_path_set_sha256"],"Completed complete original path set differs");Ok(receipt)
}

// A strict validation fence only. Callers supply complete admitted, held inventory;
// this helper cannot certify a source, release accounting or mint an owner.
pub(crate) fn exclude_completed_payload_inventory<'a>(envelope:&CompletedAbsenceEnvelope,files:impl IntoIterator<Item=(u64,u64,u64,&'a str,Option<(&'a crate::storage::data_root_designation::Designation,&'a str,&'a std::fs::File)>)>)->anyhow::Result<()> {
 envelope.validate_shape()?;let mut erased=Vec::new();
 for scope in &envelope.scopes {
  let raws=original_record_metadata(scope)?;let receipt=validate_original_records(scope,&raws)?;
  let origin:StageOrigin=decode(scope.original_stage_origin_json.as_bytes())?;
  erased.push((origin.payload.device,origin.payload.inode,receipt.intent.byte_length,receipt.intent.binary_sha256));
 }
 for(device,inode,length,sha,metadata)in files {
  // The original erased inode is always excluded, including metadata roles.
  ensure!(!erased.iter().any(|(d,i,_,_)|*d==device&&*i==inode),"Completed original payload inode retained in full inventory");
  let informational=match metadata {Some((token,path,held))=>token.matches_held_marker(path,held)?,None=>false};
  if !informational {ensure!(!erased.iter().any(|(_,_,n,h)|*n>=0&&*n as u64==length&&h==sha),"Completed independent same-content payload copy retained in full inventory");}
 }Ok(())
}

// A held, fully admitted cold bundle provides classification only. This token is
// deliberately distinct from NativeCompletedCapture and cannot free charge,
// serve payloads, construct WriterLease, or certify runtime adopted identities.
pub(crate) struct ClassifiedCompletedBundle {
 bundle:std::sync::Arc<crate::storage::backup_inventory::ValidatedBundle>,
 envelope:CompletedAbsenceEnvelope,
 receipts:BTreeMap<String,Receipt>,
 designation:Option<crate::storage::data_root_designation::Designation>,
}
impl ClassifiedCompletedBundle {
 pub(crate) async fn from_bundle(bundle:std::sync::Arc<crate::storage::backup_inventory::ValidatedBundle>,conn:&mut AnyConnection)->anyhow::Result<Self>{
  let designation=bundle.metadata_designation()?;let(envelope,receipts)=Self::inspect_for_plan(&bundle,conn).await?;let proof=Self{bundle,envelope,receipts,designation};proof.fence_in(conn).await?;Ok(proof)
 }
 // Read-only planning metadata. Sealing must retain a full from_bundle token.
 pub(crate) async fn inspect_for_plan(bundle:&crate::storage::backup_inventory::ValidatedBundle,conn:&mut AnyConnection)->anyhow::Result<(CompletedAbsenceEnvelope,BTreeMap<String,Receipt>)>{
  bundle.fence()?;ensure!(bundle.is_qualified_capacity_capture(),"Unqualified completed source");
  if let Some(m)=&bundle.manifest.mixed_completed_absence_capture{m.validate_shape()?;}
  let prior_evidence=bundle.manifest.prior_completed_absence_capture.as_ref().or_else(||bundle.manifest.mixed_completed_absence_capture.as_ref().map(|m|&m.prior)).or_else(||bundle.manifest.composite_admission_capture.as_ref().map(|c|&c.prior));
  let prior=if let Some(c)=bundle.manifest.composite_admission_capture.as_ref().filter(|c|c.version==2){Some(crate::storage::restoration::linux::prior_composite_completed_bundle_roles(bundle,conn,c).await?)}else if let Some(e)=prior_evidence{Some(crate::storage::restoration::prior_completed_bundle_roles(bundle,conn,e).await?)}else{None};
  let envelope=if let Some(m)=&bundle.manifest.mixed_completed_absence_capture{m.envelope()?}else if let Some(p)=&prior{CompletedAbsenceEnvelope{version:1,source_kind:"original_native_direct_v1".into(),scopes:p.scopes.iter().map(|s|s.original.clone()).collect()}}else{bundle.manifest.completed_absence_capture.clone().context("Completed source capture proof absent")?};envelope.validate_shape()?;
  if bundle.manifest.composite_admission_capture.is_none()&&(bundle.manifest.mixed_completed_absence_capture.is_some()||bundle.manifest.prior_completed_absence_capture.as_ref().is_some_and(|p|p.roles.version==2)){
   let mut expected=envelope.scopes.iter().map(|s|s.reservation.operation_id.clone()).collect::<Vec<_>>();expected.sort();let actual:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id").fetch_all(&mut *conn).await?;ensure!(actual==expected,"Mixed completed cold full reservation census differs");
   let capsules=bundle.manifest.source_custody.directories.0.keys().filter_map(|path|path.strip_prefix(".attachment-upload-admission/").filter(|p|!p.contains('/')).map(str::to_owned)).collect::<std::collections::BTreeSet<_>>();ensure!(capsules==expected.into_iter().collect(),"Mixed completed cold full capsule census differs");
  }
  let mut receipts=BTreeMap::new();
  for scope in &envelope.scopes {
   let attempt=&scope.reservation.operation_id;let mut raws=BTreeMap::new();
   for binding in &scope.files {
    let path=binding.role.relative(attempt)?;let original=bundle.manifest.source_custody.files.0.get(&path).context("Completed source file custody absent")?;
    let bytes=bundle.manifest.files.0.get(&path).context("Completed source file inventory absent")?;
    let captured=if let Some(p)=prior.as_ref().and_then(|p|p.scopes.iter().find(|s|s.original==*scope)){&p.files.iter().find(|b|b.role==binding.role).context("Prior completed file role absent")?.adopted}else{&binding.captured};
    ensure!(captured.device==original.device&&captured.inode==original.inode&&binding.length==bytes.size_bytes&&binding.sha256==bytes.sha256,"Completed captured file differs");
    let file=bundle.open_owned(&path)?;use std::os::unix::fs::FileExt;ensure!(file.metadata()?.len()==binding.length,"Completed bundle record size differs");let mut raw=vec![0;binding.length as usize];file.read_exact_at(&mut raw,0)?;ensure!(digest(&raw)==binding.sha256,"Completed bundle record bytes differ");raws.insert(binding.role.clone(),raw);
   }
   let receipt=validate_original_records(scope,&raws)?;
   for binding in &scope.directories {
    let original=if let Some(path)=binding.role.relative(attempt)?{bundle.manifest.source_custody.directories.0.get(&path).context("Completed source directory custody absent")?}else{&bundle.manifest.source_custody.root};
    let captured=if let Some(p)=prior.as_ref().and_then(|p|p.scopes.iter().find(|s|s.original==*scope)){&p.directories.iter().find(|b|b.role==binding.role).context("Prior completed directory role absent")?.adopted}else{&binding.captured};
    ensure!(captured.device==original.device&&captured.inode==original.inode,"Completed captured directory differs");
   }
   for(namespace,expected)in[(format!("{NS}/{attempt}"),["claim.json","stage-origin.json","committed-erasure-intent.json","committed-erasure-tombstone.json"].into_iter().map(str::to_owned).collect::<std::collections::BTreeSet<_>>()),(format!("{}/{attempt}",stage::NAMESPACE),["reservation.json","request.json","seal.json","intent.json","install-proof.json"].into_iter().map(str::to_owned).collect())]{
    let prefix=format!("{namespace}/");let names=bundle.manifest.files.0.keys().filter_map(|p|p.strip_prefix(&prefix).map(str::to_owned)).collect::<std::collections::BTreeSet<_>>();
    ensure!(names==expected&&!bundle.manifest.source_custody.directories.0.keys().any(|p|p.starts_with(&prefix)),"Completed cold directory topology differs");
   }
   let origin:StageOrigin=decode(scope.original_stage_origin_json.as_bytes())?;
   for path in [&receipt.intent.storage_path,&format!("{}/{attempt}/payload.part",stage::NAMESPACE)]{
    ensure!(!bundle.manifest.files.0.contains_key(path)&&!bundle.manifest.source_custody.directories.0.contains_key(path),"Completed erased path resurrected");
   }
   ensure!(!bundle.manifest.source_custody.files.0.values().any(|f|f.device==origin.payload.device&&f.inode==origin.payload.inode),"Completed erased original inode retained by copied alias");
   ensure!(receipts.insert(scope.disposition["asset_id"].clone(),receipt).is_none(),"Duplicate completed original");
  }
  // The qualified format declares the derived SQLite snapshot separately;
  // every remaining entry must have its original source identity.
  let captured_inventory=bundle.manifest.files.0.iter().filter(|(path,_)|path.as_str()!="starlane-notes.db").map(|(path,b)|{
   let id=bundle.manifest.source_custody.files.0.get(path).context("Completed full inventory source custody absent")?;
   Ok((id.device,id.inode,b.size_bytes,b.sha256.as_str(),path.as_str(),bundle.open_owned(path)?))
  }).collect::<anyhow::Result<Vec<_>>>()?;
  let designation=bundle.metadata_designation()?;
  exclude_completed_payload_inventory(&envelope,captured_inventory.iter().map(|(d,i,n,h,path,held)|(*d,*i,*n,*h,designation.as_ref().map(|token|(token,*path,held)))))?;
  if let Some(token)=&designation{token.fence()?;}
  check_completed_sql(&envelope,&receipts,conn).await?;bundle.fence()?;Ok((envelope,receipts))
 }
 pub(crate) fn fence(&self)->anyhow::Result<()>{self.bundle.fence()?;if let Some(token)=&self.designation{token.fence()?;}self.bundle.fence()?;Ok(())}
 pub(crate) fn source_envelope(&self)->&CompletedAbsenceEnvelope{&self.envelope}
 pub(crate) fn uses_source_basis(&self)->bool {self.bundle.manifest.composite_admission_capture.is_some()||self.bundle.manifest.mixed_completed_absence_capture.is_some()||self.bundle.manifest.prior_completed_absence_capture.as_ref().is_some_and(|p|p.roles.version==2)}
 pub(crate) fn source_basis(&self,scope:&CompletedAbsenceScope)->anyhow::Result<crate::storage::restoration::completed::CompletedSourceBasis>{
  use crate::storage::restoration::{admission::Identity,completed::{CompletedSourceBasis,CompletedSourceDirectory,CompletedSourceFile}};
  self.fence()?;ensure!(self.envelope.scopes.iter().any(|s|s==scope),"Unbound completed current source basis");
  let attempt=&scope.reservation.operation_id;let custody=&self.bundle.manifest.source_custody;
  let root=Identity{device:custody.root.device,inode:custody.root.inode};
  let directories=scope.directories.iter().map(|b|{let id=if let Some(path)=b.role.relative(attempt)?{custody.directories.0.get(&path).context("Completed source basis directory absent")?}else{&custody.root};Ok(CompletedSourceDirectory{role:b.role.clone(),captured:Identity{device:id.device,inode:id.inode}})}).collect::<anyhow::Result<Vec<_>>>()?;
  let files=scope.files.iter().map(|b|{let path=b.role.relative(attempt)?;let id=custody.files.0.get(&path).context("Completed source basis file absent")?;Ok(CompletedSourceFile{role:b.role.clone(),captured:Identity{device:id.device,inode:id.inode}})}).collect::<anyhow::Result<Vec<_>>>()?;
  self.fence()?;Ok(CompletedSourceBasis{root,directories,files})
 }

 pub(crate) async fn fence_in(&self,conn:&mut AnyConnection)->anyhow::Result<()> {
  self.fence()?;check_completed_sql(&self.envelope,&self.receipts,conn).await?;self.fence()?;Ok(())
 }
}
pub(crate) async fn check_completed_sql(envelope:&CompletedAbsenceEnvelope,receipts:&BTreeMap<String,Receipt>,conn:&mut AnyConnection)->anyhow::Result<()> {
  envelope.validate_shape()?;
  ensure!(rows(conn).await?==envelope.scopes.iter().map(|s|s.disposition.clone()).collect::<Vec<_>>(),"Completed cold full SQL inventory differs");
  check_completed_sql_subset(envelope,receipts,conn).await
}
// Sealed restored-scope token validates only its immutable old rows. Its caller
// must authenticate every remaining reservation/capsule with genuine native custody.
pub(crate) async fn check_completed_sql_subset(envelope:&CompletedAbsenceEnvelope,receipts:&BTreeMap<String,Receipt>,conn:&mut AnyConnection)->anyhow::Result<()> {
 envelope.validate_shape()?;let current=rows(conn).await?;
 for scope in &envelope.scopes {ensure!(current.iter().find(|row|row.get("asset_id")==scope.disposition.get("asset_id"))==Some(&scope.disposition),"Completed immutable restored SQL scope differs");}
  for scope in &envelope.scopes {
   ensure!(reservation_in(conn,&scope.reservation.operation_id).await?==scope.reservation,"Completed cold original reservation differs");
   let receipt=&receipts[&scope.disposition["asset_id"]];
   let actual=crate::storage::attachment_upload::receipt(conn,&receipt.intent.request.operation_id).await.map_err(|_|fail("Completed cold original receipt absent"))?.context("Completed cold original receipt absent")?;
   ensure!(actual==*receipt&&serde_json::to_string(&actual)?==serde_json::to_string(receipt)?,"Completed cold original receipt differs");
   let plan=crate::storage::attachment_erasure_plan::load_for_physical_review(conn,&scope.disposition["deletion_operation_id"]).await.map_err(|_|fail("Completed cold original plan invalid"))?.context("Completed cold original plan absent")?;
   ensure!(serde_json::to_string(&plan)?==scope.original_plan_json,"Completed cold original deletion plan differs");
  }
  Ok(())
}

// Metadata reconstruction is used only to validate sealed envelope syntax.
// Physical classifiers always compare these exact bytes against held files.
pub(crate) fn original_record_metadata(scope:&CompletedAbsenceScope)->anyhow::Result<BTreeMap<CompletedFileRole,Vec<u8>>>{
 let intent:Intent=decode(scope.disposition.get("native_intent_json").context("Completed native intent absent")?.as_bytes())?;
 let mut raws=BTreeMap::new();raws.insert(CompletedFileRole::Claim,scope.original_claim_json.as_bytes().to_vec());raws.insert(CompletedFileRole::StageOrigin,scope.original_stage_origin_json.as_bytes().to_vec());
 raws.insert(CompletedFileRole::CommittedIntent,scope.disposition["native_intent_json"].as_bytes().to_vec());raws.insert(CompletedFileRole::CommittedTombstone,scope.disposition.get("native_tombstone_json").context("Completed native tombstone absent")?.as_bytes().to_vec());
 for name in ["reservation.json","request.json","seal.json","intent.json","install-proof.json"]{raws.insert(CompletedFileRole::DirectEnvelope{name:name.into()},intent.owned.envelopes.get(name).context("Completed original stage metadata absent")?.as_bytes().to_vec());}
 validate_original_records(scope,&raws)?;Ok(raws)
}

// Read-only cold reference validation. No native or finalized authority is returned.
pub(crate) async fn check_cold_references(root:&std::path::Path,envelope:&CompletedAbsenceEnvelope,receipts:&BTreeMap<String,Receipt>,conn:&mut AnyConnection)->anyhow::Result<()> {
 let paths=crate::storage::attachment_capacity::retained_paths_in(conn).await?;
 for scope in &envelope.scopes {let receipt=&receipts[&scope.disposition["asset_id"]];let proof=coordinator::references::prove_in_at(root,conn,None,&scope.disposition["deletion_operation_id"],receipt,paths.clone(),false).await.map_err(|_|fail("Completed cold protected references require review"))?;ensure!(proof.plan_raw==scope.original_plan_json,"Completed cold original plan reference differs");}Ok(())
}

// SQL/reference provenance only. This opaque observation grants no serving,
// physical ownership, cleanup, or accounting reduction.
pub(crate) struct CompletedRegistryAbsences {receipts:BTreeMap<String,Receipt>,retirements:BTreeMap<String,String>}
impl CompletedRegistryAbsences {pub(crate) fn original(&self,id:&str)->Option<&Receipt>{self.receipts.get(id)}
 pub(crate) fn retirement_matches(&self,r:&crate::storage::attachment_retirement::Retirement)->bool{self.retirements.get(&r.attachment_id).is_some_and(|raw|serde_json::to_string(r).ok().as_ref()==Some(raw))}}
pub(crate) async fn completed_registry_absences_in(root:&std::path::Path,envelope:&CompletedAbsenceEnvelope,receipts:&BTreeMap<String,Receipt>,conn:&mut AnyConnection)->anyhow::Result<CompletedRegistryAbsences>{
 check_completed_sql(envelope,receipts,conn).await?;
 check_cold_references(root,envelope,receipts,conn).await?;
 let mut retirements=BTreeMap::new();
 for scope in &envelope.scopes {let plan:crate::storage::attachment_erasure_plan::Plan=serde_json::from_str(&scope.original_plan_json)?;
  for row in plan.references.get("attachment_retirements").context("Completed original retirement roster absent")? {
   let id=row["attachment_id"].as_str().context("Completed original retirement identity absent")?;let raw=row["receipt_json"].as_str().context("Completed original retirement raw receipt absent")?;
   if let Some(old)=retirements.insert(id.to_owned(),raw.to_owned()){ensure!(old==raw,"Completed captured retirement originals conflict");}
  }
 }
 Ok(CompletedRegistryAbsences{receipts:receipts.clone(),retirements})
}
