//! Ledger4 composite roles. Decoding this data is never a source/destination grant.
//! Source-held/cold/finalized constructors are separate and must fence complete censuses.
use super::{admission,completed,fail};
use serde::{Serialize,Deserialize};
use std::{collections::{BTreeMap,BTreeSet},io};
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompositeOwnerEnvelope {
 pub version:u32,
 pub completed:completed::CompletedOwnerEnvelope,
 pub native:Vec<NativeOwnerScope>,
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeOwnerScope {
 pub original:NativeScope,
 pub source_basis:NativeSourceBasis,
 pub directories:Vec<OwnedDirectory>,
 pub files:Vec<OwnedFile>,
 pub pending:Option<admission::AdmissionOwnerScope>,
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(tag="basis",deny_unknown_fields,rename_all="snake_case")]
pub(crate) enum NativeSourceBasis {
 Original,
 Finalized{generation_id:String,ledger_sha256:String,root:admission::Identity,directories:Vec<CapturedDirectory>,files:Vec<CapturedFile>},
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NativeScope {
 pub reservation:admission::CapturedReservation,
 pub claim_json:String,
 pub class:NativeClass,
 pub directories:Vec<SourceDirectory>,
 pub files:Vec<SourceFile>,
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(tag="class",deny_unknown_fields,rename_all="snake_case")]
pub(crate) enum NativeClass {
 Reserved{original_scope:Option<admission::AdmissionOwnerScope>},
 CommittedDirect{receipt_json:String},
 DisposedDirect{intent_json:String},
 DisposedReplay{intent_json:String},
}
#[derive(Clone,Debug,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(tag="role",deny_unknown_fields,rename_all="snake_case")]
pub(crate) enum DirectoryRole {
 DataRoot,AdmissionNamespace,AdmissionCapsule,DirectStageNamespace,DirectStageDirectory,
 ReplayBlobParent{path:String},ReplayRewriteNamespace,ReplayRewriteDirectory{operation_id:String},
}
#[derive(Clone,Debug,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(tag="role",deny_unknown_fields,rename_all="snake_case")]
pub(crate) enum FileRole {
 CapsuleRecord{name:CapsuleRecord},DirectStageRecord{name:StageRecord},
 ReplayRewriteManifest{operation_id:String},ReplayRewriteInstallProof{operation_id:String},
}
#[derive(Clone,Debug,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub(crate) enum CapsuleRecord {Claim,StageOrigin,DisposalIntent,DisposalTombstone,ReplayBlobOrigin,ReplayRewritePlan,ReplayRewriteOrigin,ReplayDisposalIntent,ReplayDisposalTombstone}
#[derive(Clone,Debug,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub(crate) enum StageRecord {Reservation,Request,Seal,Intent,InstallProof}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceDirectory{pub role:DirectoryRole,pub original:admission::Identity,pub captured:admission::Identity}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceFile{pub role:FileRole,pub original:admission::Identity,pub captured:admission::Identity,pub length:u64,pub sha256:String}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnedDirectory{pub role:DirectoryRole,pub adopted:admission::Identity}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnedFile{pub role:FileRole,pub adopted:admission::Identity}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CapturedDirectory{pub role:DirectoryRole,pub captured:admission::Identity}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CapturedFile{pub role:FileRole,pub captured:admission::Identity}
fn id(value:&str)->io::Result<()>{let parsed=uuid::Uuid::parse_str(value).map_err(|_|fail("Composite operation role is not UUID"))?;if parsed.hyphenated().to_string()!=value{return Err(fail("Composite operation role is not canonical"));}Ok(())}
fn safe(path:&str)->io::Result<()>{if path.is_empty()||path.starts_with('/')||path.contains('\\')||path.contains('\0')||path.split('/').any(|p|p.is_empty()||p=="."||p==".."){return Err(fail("Unsafe composite role path"));}Ok(())}
impl DirectoryRole {
 pub(crate) fn relative(&self,attempt:&str)->io::Result<Option<String>>{id(attempt)?;Ok(match self{
  Self::DataRoot=>None,Self::AdmissionNamespace=>Some(".attachment-upload-admission".into()),Self::AdmissionCapsule=>Some(format!(".attachment-upload-admission/{attempt}")),Self::DirectStageNamespace=>Some(".attachment-upload-journal".into()),Self::DirectStageDirectory=>Some(format!(".attachment-upload-journal/{attempt}")),
  Self::ReplayBlobParent{path}=>{safe(path)?;if !path.starts_with("spaces/")||!path.contains("/attachments/"){return Err(fail("Composite Replay parent is outside managed assets"));}Some(path.clone())},Self::ReplayRewriteNamespace=>Some(".canonical-rewrite-journal".into()),Self::ReplayRewriteDirectory{operation_id}=>{id(operation_id)?;Some(format!(".canonical-rewrite-journal/{operation_id}"))},})}
}
impl FileRole {
 pub(crate) fn relative(&self,attempt:&str)->io::Result<String>{id(attempt)?;Ok(match self{
  Self::CapsuleRecord{name}=>format!(".attachment-upload-admission/{attempt}/{}",match name{CapsuleRecord::Claim=>"claim.json",CapsuleRecord::StageOrigin=>"stage-origin.json",CapsuleRecord::DisposalIntent=>"disposal-intent.json",CapsuleRecord::DisposalTombstone=>"disposal-tombstone.json",CapsuleRecord::ReplayBlobOrigin=>"replay-blob-origin.json",CapsuleRecord::ReplayRewritePlan=>"replay-rewrite-plan.json",CapsuleRecord::ReplayRewriteOrigin=>"replay-rewrite-origin.json",CapsuleRecord::ReplayDisposalIntent=>"replay-disposal-intent.json",CapsuleRecord::ReplayDisposalTombstone=>"replay-disposal-tombstone.json"}),
  Self::DirectStageRecord{name}=>format!(".attachment-upload-journal/{attempt}/{}",match name{StageRecord::Reservation=>"reservation.json",StageRecord::Request=>"request.json",StageRecord::Seal=>"seal.json",StageRecord::Intent=>"intent.json",StageRecord::InstallProof=>"install-proof.json"}),
  Self::ReplayRewriteManifest{operation_id}=>{id(operation_id)?;format!(".canonical-rewrite-journal/{operation_id}/manifest.json")},Self::ReplayRewriteInstallProof{operation_id}=>{id(operation_id)?;format!(".canonical-rewrite-journal/{operation_id}/install-proof.json")},})}
}
// Full semantic validation (raw typed claims/SQL/protected refs/absence and the
// complete source/target role census) belongs to opaque constructors. These
// wire structures are not registered in Ledger or a manifest parser yet.
impl NativeScope {
 pub(crate) fn validate_shape(&self)->io::Result<()> {
  self.reservation.matches_claim(self.claim_json.as_bytes())?;id(&self.reservation.operation_id)?;
  if self.claim_json.len()>65536||self.directories.len()>16||self.files.len()>32{return Err(fail("Composite native metadata bound exceeded"));}
  let attempt=&self.reservation.operation_id;
  let dirs=self.directories.iter().map(|b|b.role.clone()).collect::<BTreeSet<_>>();let files=self.files.iter().map(|b|b.role.clone()).collect::<BTreeSet<_>>();
  if dirs.len()!=self.directories.len()||files.len()!=self.files.len()||![DirectoryRole::DataRoot,DirectoryRole::AdmissionNamespace,DirectoryRole::AdmissionCapsule].iter().all(|r|dirs.contains(r))||!files.contains(&FileRole::CapsuleRecord{name:CapsuleRecord::Claim}){return Err(fail("Composite native role set incomplete or duplicated"));}
  for b in &self.directories {b.role.relative(attempt)?;if b.original.inode==0||b.captured.inode==0||b.original!=b.captured{return Err(fail("Composite source original directory identity differs"));}}
  for b in &self.files {b.role.relative(attempt)?;if b.original.inode==0||b.captured.inode==0||b.original!=b.captured||b.length>8*1024*1024||!super::valid_hash(&b.sha256){return Err(fail("Composite source record identity/bytes invalid"));}}
  let claim=self.files.iter().find(|b|b.role==(FileRole::CapsuleRecord{name:CapsuleRecord::Claim})).ok_or_else(||fail("Composite claim role absent"))?;
  if claim.length!=self.claim_json.len()as u64||claim.sha256!=super::digest(self.claim_json.as_bytes()){return Err(fail("Composite claim bytes/role differ"));}
  let required=match &self.class{
   NativeClass::Reserved{original_scope}=>{if self.reservation.state!="reserved"{return Err(fail("Composite reserved class/SQL mismatch"));}if let Some(scope)=original_scope {if scope.attempt_id!=*attempt||scope.original_claim_json!=self.claim_json||scope.maximum_bytes!=self.reservation.maximum_bytes as u64||!matches!(&scope.reservation,admission::ReservationState::Reserved){return Err(fail("Composite pending original scope differs"));}let root=self.directories.iter().find(|b|b.role==DirectoryRole::DataRoot).ok_or_else(||fail("Composite pending root absent"))?;admission::AdmissionOwnerEnvelope{version:2,scopes:vec![scope.clone()]}.validate(&root.captured)?;}Vec::new()},
   NativeClass::CommittedDirect{receipt_json}=>{if self.reservation.state!="committed"||receipt_json.len()>65536{return Err(fail("Composite committed class/SQL mismatch"));}Vec::new()},
   NativeClass::DisposedDirect{intent_json}=>{if self.reservation.state!="disposed"||self.reservation.disposition_json.as_deref()!=Some(intent_json.as_str()){return Err(fail("Composite disposed Direct class/SQL mismatch"));}vec![CapsuleRecord::DisposalIntent,CapsuleRecord::DisposalTombstone]},
   NativeClass::DisposedReplay{intent_json}=>{if self.reservation.state!="disposed"||self.reservation.disposition_json.as_deref()!=Some(intent_json.as_str()){return Err(fail("Composite disposed Replay class/SQL mismatch"));}vec![CapsuleRecord::ReplayDisposalIntent,CapsuleRecord::ReplayDisposalTombstone]},
  };
  for name in required {let binding=self.files.iter().find(|b|b.role==(FileRole::CapsuleRecord{name:name.clone()})).ok_or_else(||fail("Composite disposed immutable record role absent"))?;let raw=self.reservation.disposition_json.as_deref().ok_or_else(||fail("Composite disposition absent"))?;if binding.sha256!=super::digest(raw.as_bytes())||binding.length!=raw.len()as u64{return Err(fail("Composite original intent/tombstone role bytes differ"));}}
  Ok(())
 }
}
impl CompositeOwnerEnvelope {
 pub(crate) fn validate(&self,root:&admission::Identity)->io::Result<()> {
  if !matches!(self.version,1|2)||self.completed.version!=2||self.native.is_empty()||root.inode==0{return Err(fail("Composite owner version/components differ"));}
  self.completed.validate(root)?;
  let mut attempts=self.completed.scopes.iter().map(|s|s.original.reservation.operation_id.clone()).collect::<BTreeSet<_>>();
  let mut forward=BTreeMap::new();let mut backward=BTreeMap::new();let mut source_root=None;
  for completed in &self.completed.scopes {let basis=completed.source_basis.as_ref().ok_or_else(||fail("Composite completed current source basis absent"))?;if source_root.as_ref().is_some_and(|r|r!=&basis.root){return Err(fail("Composite completed source roots differ"));}source_root=Some(basis.root.clone());for b in &completed.directories{let old=&basis.directories.iter().find(|s|s.role==b.role).ok_or_else(||fail("Composite completed source directory absent"))?.captured;let old=(old.device,old.inode);let new=(b.adopted.device,b.adopted.inode);if forward.insert(old,new).is_some_and(|v|v!=new)||backward.insert(new,old).is_some_and(|v|v!=old){return Err(fail("Composite completed/shared directory substitution"));}}for b in &completed.files{let old=&basis.files.iter().find(|s|s.role==b.role).ok_or_else(||fail("Composite completed source file absent"))?.captured;let old=(old.device,old.inode);let new=(b.adopted.device,b.adopted.inode);if forward.insert(old,new).is_some_and(|v|v!=new)||backward.insert(new,old).is_some_and(|v|v!=old){return Err(fail("Composite completed/shared file substitution"));}}}
  for scope in &self.native {
   scope.original.validate_shape()?;let attempt=&scope.original.reservation.operation_id;if !attempts.insert(attempt.clone()){return Err(fail("Composite duplicate or cross-class attempt"));}
   let (captured_root,dirs,files)=match &scope.source_basis {
    NativeSourceBasis::Original=>{let root=scope.original.directories.iter().find(|b|b.role==DirectoryRole::DataRoot).ok_or_else(||fail("Composite native root absent"))?.captured.clone();(root,scope.original.directories.iter().map(|b|(b.role.clone(),b.captured.clone())).collect::<BTreeMap<_,_>>(),scope.original.files.iter().map(|b|(b.role.clone(),b.captured.clone())).collect::<BTreeMap<_,_>>())},
    NativeSourceBasis::Finalized{generation_id,ledger_sha256,root,directories,files}=>{id(generation_id)?;if !super::valid_hash(ledger_sha256)||directories.len()!=scope.original.directories.len()||files.len()!=scope.original.files.len(){return Err(fail("Composite finalized source basis incomplete"));}(root.clone(),directories.iter().map(|b|(b.role.clone(),b.captured.clone())).collect(),files.iter().map(|b|(b.role.clone(),b.captured.clone())).collect())},
   };
   let expected_dirs=scope.original.directories.iter().map(|b|b.role.clone()).collect::<BTreeSet<_>>();let expected_files=scope.original.files.iter().map(|b|b.role.clone()).collect::<BTreeSet<_>>();
   if dirs.keys().cloned().collect::<BTreeSet<_>>()!=expected_dirs||files.keys().cloned().collect::<BTreeSet<_>>()!=expected_files{return Err(fail("Composite finalized source role keys substituted"));}
   if source_root.as_ref()!=Some(&captured_root)||dirs.len()!=scope.original.directories.len()||files.len()!=scope.original.files.len()||scope.directories.len()!=dirs.len()||scope.files.len()!=files.len(){return Err(fail("Composite native/current source role inventory differs"));}
   let mut seen_dirs=BTreeSet::new();let mut seen_files=BTreeSet::new();
   for b in &scope.directories{let old=dirs.get(&b.role).ok_or_else(||fail("Unbound composite adopted directory"))?;if !seen_dirs.insert(b.role.clone())||old.inode==0||b.adopted.inode==0||b.role==DirectoryRole::DataRoot&&(&b.adopted!=root||old!=&captured_root){return Err(fail("Composite adopted directory malformed"));}let old=(old.device,old.inode);let new=(b.adopted.device,b.adopted.inode);if forward.insert(old,new).is_some_and(|v|v!=new)||backward.insert(new,old).is_some_and(|v|v!=old){return Err(fail("Composite directory source/target substitution"));}}
   for b in &scope.files{let old=files.get(&b.role).ok_or_else(||fail("Unbound composite adopted record"))?;if !seen_files.insert(b.role.clone())||old.inode==0||b.adopted.inode==0{return Err(fail("Composite adopted record malformed"));}let old=(old.device,old.inode);let new=(b.adopted.device,b.adopted.inode);if forward.insert(old,new).is_some_and(|v|v!=new)||backward.insert(new,old).is_some_and(|v|v!=old){return Err(fail("Composite record source/target substitution"));}}
   match(&scope.original.class,&scope.pending){(NativeClass::Reserved{original_scope:Some(original)},Some(pending))=>{if pending.attempt_id!=*attempt||pending.original_claim_json!=original.original_claim_json{return Err(fail("Composite pending owner differs"));}admission::AdmissionOwnerEnvelope{version:2,scopes:vec![pending.clone()]}.validate(root)?;},(NativeClass::Reserved{original_scope:None},None)=>{},(NativeClass::Reserved{..},_)=>return Err(fail("Composite pending certification omitted or invented")),(_,None)=>{},(_,Some(_))=>return Err(fail("Composite nonpending scope carries admission authority"))}
  }
  if attempts.len()>512||serde_json::to_vec(self).map_err(|_|fail("Composite owner encoding invalid"))?.len()>8*1024*1024{return Err(fail("Composite owner bound exceeded"));}Ok(())
 }
}

/// Portable facts from an original held source, never runtime authority.
/// Version1 preserves existing canonical completed owner-v1/v2 inputs plus genuinely
/// native non-completed additions. Other unions retain/refuse independently.
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompositeCaptureEvidence {
 pub version:u32,
 pub prior:super::linux::PriorCompletedCaptureEvidence,
 pub native:Vec<NativeScope>,
 #[serde(default,skip_serializing_if="Option::is_none",deserialize_with="prior_native_nonnull")]
 pub prior_native:Option<Vec<NativeOwnerScope>>,
}
fn prior_native_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<Vec<NativeOwnerScope>>,D::Error>{Vec::<NativeOwnerScope>::deserialize(d).map(Some)}
impl CompositeCaptureEvidence {
 pub(crate) fn validate_shape(&self)->io::Result<()> {
  self.prior.validate_shape()?;
  if !matches!(self.version,1|2)||(self.version==1)!=self.prior_native.is_none()||!matches!(self.prior.roles.version,1|2)||self.native.is_empty(){return Err(fail("Composite capture version/source components unsupported"));}
  if let Some(old)=&self.prior_native {
   if old.is_empty(){return Err(fail("Repeated composite prior native census empty"));}
   let owner=CompositeOwnerEnvelope{version:2,completed:self.prior.roles.clone(),native:old.clone()};owner.validate(&self.prior.source_root)?;
   let mut seen=BTreeSet::new();for scope in old {let id=&scope.original.reservation.operation_id;if !seen.insert(id)||self.native.iter().find(|n|&n.reservation.operation_id==id)!=Some(&scope.original){return Err(fail("Repeated composite old original changed/omitted"));}}
  }
  let mut attempts=self.prior.roles.scopes.iter().map(|s|s.original.reservation.operation_id.clone()).collect::<BTreeSet<_>>();
  let mut paths=BTreeMap::new();let mut identities=BTreeMap::new();
  // Shared namespace directories must have one exact current-source identity.
  for completed in &self.prior.roles.scopes {
   for b in &completed.directories {let path=b.role.relative(&completed.original.reservation.operation_id).map_err(|_|fail("Composite completed directory role invalid"))?.unwrap_or_default();let id=(b.adopted.device,b.adopted.inode);if paths.insert(path.clone(),id).is_some_and(|old|old!=id)||identities.insert(id,path.clone()).is_some_and(|old|old!=path){return Err(fail("Composite prior directory alias/substitution"));}}
  }
  let mut noncompleted=false;
  for native in &self.native {
   native.validate_shape()?;let attempt=&native.reservation.operation_id;
   if !attempts.insert(attempt.clone()){return Err(fail("Composite capture duplicate/cross-class attempt"));}
   noncompleted|=matches!(native.class,NativeClass::Reserved{..}|NativeClass::DisposedDirect{..}|NativeClass::DisposedReplay{..});
   for b in &native.directories {let path=b.role.relative(attempt)?.unwrap_or_default();let source=self.source_directory(native,&b.role)?;let id=(source.device,source.inode);if b.role==DirectoryRole::DataRoot&&source!=&self.prior.source_root||paths.insert(path.clone(),id).is_some_and(|old|old!=id)||identities.insert(id,path.clone()).is_some_and(|old|old!=path){return Err(fail("Composite native source root/role alias/substitution"));}}
  }
  if !noncompleted||attempts.len()>512||serde_json::to_vec(self).map_err(|_|fail("Composite capture encoding failed"))?.len()>8*1024*1024{return Err(fail("Composite capture bound/class differs; all-completed retains old dispatch"));}Ok(())
 }
 pub(crate) fn prior_scope(&self,scope:&NativeScope)->Option<&NativeOwnerScope>{self.prior_native.as_ref()?.iter().find(|p|p.original.reservation.operation_id==scope.reservation.operation_id)}
 pub(crate) fn source_directory<'a>(&'a self,scope:&'a NativeScope,role:&DirectoryRole)->io::Result<&'a admission::Identity>{if let Some(old)=self.prior_scope(scope){Ok(&old.directories.iter().find(|b|&b.role==role).ok_or_else(||fail("Repeated composite directory role absent"))?.adopted)}else{Ok(&scope.directories.iter().find(|b|&b.role==role).ok_or_else(||fail("Original composite directory role absent"))?.captured)}}
 pub(crate) fn source_file<'a>(&'a self,scope:&'a NativeScope,role:&FileRole)->io::Result<&'a admission::Identity>{if let Some(old)=self.prior_scope(scope){Ok(&old.files.iter().find(|b|&b.role==role).ok_or_else(||fail("Repeated composite file role absent"))?.adopted)}else{Ok(&scope.files.iter().find(|b|&b.role==role).ok_or_else(||fail("Original composite file role absent"))?.captured)}}
 pub(crate) fn source_basis(&self,scope:&NativeScope)->NativeSourceBasis{if let Some(old)=self.prior_scope(scope){NativeSourceBasis::Finalized{generation_id:self.prior.generation_id.clone(),ledger_sha256:self.prior.ledger_sha256.clone(),root:self.prior.source_root.clone(),directories:old.directories.iter().map(|b|CapturedDirectory{role:b.role.clone(),captured:b.adopted.clone()}).collect(),files:old.files.iter().map(|b|CapturedFile{role:b.role.clone(),captured:b.adopted.clone()}).collect()}}else{NativeSourceBasis::Original}}
 pub(crate) fn pending_scope(&self,scope:&NativeScope)->io::Result<Option<admission::AdmissionOwnerScope>>{match &scope.class{NativeClass::Reserved{original_scope:Some(original)}=>{if let Some(old)=self.prior_scope(scope){let mut pending=old.pending.clone().ok_or_else(||fail("Repeated composite old pending role omitted"))?;if pending.original_claim_json!=original.original_claim_json||pending.attempt_id!=original.attempt_id||pending.files.len()!=original.files.len()||pending.directories.len()!=original.directories.len(){return Err(fail("Repeated composite old pending content differs"));}for b in &mut pending.files {let raw=original.files.iter().find(|o|o.role==b.role).ok_or_else(||fail("Repeated pending original file role absent"))?;if b.original!=raw.original||b.length!=raw.length||b.sha256!=raw.sha256{return Err(fail("Repeated pending original bytes differ"));}b.captured=b.adopted.clone();}for b in &mut pending.directories{let raw=original.directories.iter().find(|o|o.role==b.role).ok_or_else(||fail("Repeated pending original directory absent"))?;if b.original!=raw.original{return Err(fail("Repeated pending original directory differs"));}b.captured=b.adopted.clone();}Ok(Some(pending))}else{Ok(Some(original.clone()))}},NativeClass::Reserved{original_scope:None}=>Err(fail("Composite pending original proof absent")),_=>Ok(None)}}
 pub(crate) async fn check_complete_reservation_census_in(&self,conn:&mut sqlx::AnyConnection,capsules:&BTreeSet<String>)->io::Result<()> {
  self.validate_shape()?;
  let expected=self.prior.roles.scopes.iter().map(|s|s.original.reservation.operation_id.clone()).chain(self.native.iter().map(|s|s.reservation.operation_id.clone())).collect::<BTreeSet<_>>();
  let actual:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id LIMIT 513").fetch_all(&mut *conn).await.map_err(|_|fail("Composite complete reservation SQL unavailable"))?;
  if actual.len()!=expected.len()||actual.iter().any(|id|!expected.contains(id))||capsules!=&expected{return Err(fail("Composite full SQL/capsule census omitted or substituted"));}
  for scope in &self.native {let actual=crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::reservation_in(conn,&scope.reservation.operation_id).await.map_err(|_|fail("Composite native reservation unavailable"))?;if actual!=scope.reservation{return Err(fail("Composite native exact9 reservation differs"));}}
  Ok(())
 }
}
