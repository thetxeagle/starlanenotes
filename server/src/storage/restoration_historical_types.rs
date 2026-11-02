//! Wire facts for a distinct preservation purpose. Parsing never constructs custody.
use super::{admission,composite,fail};
use serde::{Serialize,Deserialize};
use std::io;
pub(crate) const PURPOSE:&str="historical_physical_preservation_v1";
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub(crate) enum HistoricalReferenceSemantics {NotEstablished}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ApplicationProjection {pub version:u32,pub sha256:String,pub reference_semantics:HistoricalReferenceSemantics}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoricalCompositePreservationEvidence {
 pub version:u32,pub purpose:String,pub prior:super::linux::PriorCompletedCaptureEvidence,
 pub native:Vec<composite::NativeScope>,pub application_projection:ApplicationProjection,
}
impl HistoricalCompositePreservationEvidence {
 pub(crate) fn validate_shape(&self)->io::Result<()> {
  self.prior.validate_shape()?;
  if self.version!=1||self.purpose!=PURPOSE||self.native.is_empty()||self.native.len()+self.prior.roles.scopes.len()>512||self.application_projection.version!=1||self.application_projection.sha256.len()!=64||!self.application_projection.sha256.bytes().all(|c|c.is_ascii_digit()||(b'a'..=b'f').contains(&c))||serde_json::to_vec(self).map_err(io::Error::other)?.len()>8*1024*1024{return Err(fail("Historical preservation version/purpose/projection/bounds differ"));}
  let mut attempts=self.prior.roles.scopes.iter().map(|s|s.original.reservation.operation_id.clone()).collect::<std::collections::BTreeSet<_>>();
  for scope in &self.native {
   scope.validate_shape()?;if !attempts.insert(scope.reservation.operation_id.clone()){return Err(fail("Historical preservation scope duplicated or replayed"));}
   let claim:serde_json::Value=serde_json::from_str(&scope.claim_json).map_err(io::Error::other)?;
   let producer:crate::storage::attachment_upload::admission::Producer=serde_json::from_value(claim.get("producer").ok_or_else(||fail("Historical typed producer absent"))?.clone()).map_err(io::Error::other)?;
   if !matches!(producer,crate::storage::attachment_upload::admission::Producer::Direct{..})||!matches!(&scope.class,composite::NativeClass::Reserved{original_scope:Some(_)}|composite::NativeClass::CommittedDirect{..}|composite::NativeClass::DisposedDirect{..}){return Err(fail("Historical v1 excludes unsupported native producer/class"));}
   if scope.directories.iter().find(|b|b.role==composite::DirectoryRole::DataRoot).map(|b|&b.captured)!=Some(&self.prior.source_root){return Err(fail("Historical genuine native source root differs"));}
  }Ok(())
 }
}
pub(crate) fn capture_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<HistoricalCompositePreservationEvidence>,D::Error>{HistoricalCompositePreservationEvidence::deserialize(d).map(Some)}
#[cfg(test)]mod tests {
 use super::*;
 #[derive(Deserialize)]#[serde(deny_unknown_fields)]struct Envelope{#[serde(default,deserialize_with="capture_nonnull")]historical_composite_preservation_capture:Option<HistoricalCompositePreservationEvidence>}
 #[test]fn strict_historical_optional_transport(){assert!(serde_json::from_str::<Envelope>("{}").unwrap().historical_composite_preservation_capture.is_none());for raw in [r#"{"historical_composite_preservation_capture":null}"#,r#"{"historical_composite_preservation_capture":false}"#,r#"{"historical_composite_preservation_capture":{}}"#,r#"{"historical_composite_preservation_capture":{},"historical_composite_preservation_capture":{}}"#,r#"{"historical_composite_preservation_capture":{"version":1,"foreign":true}}"#,r#"{"unknown":false}"#]{assert!(serde_json::from_str::<Envelope>(raw).is_err());}assert!(serde_json::from_str::<HistoricalReferenceSemantics>("\"current_references_verified\"").is_err());}
}

// Distinct destination wire material only. Decoding this type is not a live
// physical owner, current-reference proof, admission scope or serving grant.
#[derive(Clone,Copy,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub(crate) enum HistoricalPhysicalRole {
 HistoricalCompletedPhysicalAbsence,
 RetainedPending,
 CommittedPreservation,
 HistoricalDisposedPhysicalAbsence,
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoricalCompletedOwner {
 pub role:HistoricalPhysicalRole,
 pub material:super::completed::CompletedOwnerEnvelope,
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoricalNativeOwner {
 pub role:HistoricalPhysicalRole,
 pub material:composite::NativeOwnerScope,
 pub payloads:Vec<HistoricalPayloadBinding>,
}
#[derive(Clone,Copy,Debug,PartialEq,Eq,PartialOrd,Ord,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub(crate) enum HistoricalPayloadRole {CommittedCanonical,CommittedStagePayload}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoricalPayloadBinding {
 pub role:HistoricalPayloadRole,pub source:admission::Identity,pub adopted:admission::Identity,
 pub length:u64,pub sha256:String,
}
impl HistoricalPayloadRole {
 pub(crate) fn relative(&self,scope:&composite::NativeScope)->io::Result<String>{
  let composite::NativeClass::CommittedDirect{receipt_json}=&scope.class else{return Err(fail("Historical payload outside committed physical class"))};
  let receipt:crate::storage::attachment_upload::Receipt=serde_json::from_str(receipt_json).map_err(io::Error::other)?;
  let path=match self{Self::CommittedCanonical=>receipt.intent.storage_path,Self::CommittedStagePayload=>format!(".attachment-upload-journal/{}/payload.part",scope.reservation.operation_id)};
  if !super::valid_relative(&path)||path.contains('\\'){return Err(fail("Historical committed physical path malformed"));}Ok(path)
 }
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoricalOwnerEnvelope {
 pub version:u32,
 pub purpose:String,
 pub source:HistoricalCompositePreservationEvidence,
 pub completed:HistoricalCompletedOwner,
 pub native:Vec<HistoricalNativeOwner>,
}
impl HistoricalOwnerEnvelope {
 pub(crate) fn validate(&self,root:&admission::Identity)->io::Result<()> {
  use super::completed::{CompletedSourceDirectory,CompletedSourceFile};
  self.source.validate_shape()?;
  if self.version!=1||self.purpose!=PURPOSE||self.completed.role!=HistoricalPhysicalRole::HistoricalCompletedPhysicalAbsence||self.completed.material.version!=2||self.native.len()!=self.source.native.len()||self.completed.material.scopes.len()!=self.source.prior.roles.scopes.len(){return Err(fail("Historical fresh destination purpose/classes/census differ"));}
  self.completed.material.validate(root)?;
  for(scope,source)in self.completed.material.scopes.iter().zip(&self.source.prior.roles.scopes){
   if scope.original!=source.original{return Err(fail("Historical fresh completion original records differ"));}
   let basis=scope.source_basis.as_ref().ok_or_else(||fail("Historical fresh completion current source basis absent"))?;
   let directories=source.directories.iter().map(|b|CompletedSourceDirectory{role:b.role.clone(),captured:b.adopted.clone()}).collect::<Vec<_>>();
   let files=source.files.iter().map(|b|CompletedSourceFile{role:b.role.clone(),captured:b.adopted.clone()}).collect::<Vec<_>>();
   if basis.root!=self.source.prior.source_root||basis.directories!=directories||basis.files!=files{return Err(fail("Historical fresh completion source custody basis differs"));}
  }
  for(scope,source)in self.native.iter().zip(&self.source.native){
   let role=match &source.class {
    composite::NativeClass::Reserved{original_scope:Some(_)}=>HistoricalPhysicalRole::RetainedPending,
    composite::NativeClass::CommittedDirect{..}=>HistoricalPhysicalRole::CommittedPreservation,
    composite::NativeClass::DisposedDirect{..}=>HistoricalPhysicalRole::HistoricalDisposedPhysicalAbsence,
    _=>return Err(fail("Historical fresh native class unsupported")),
   };
   if scope.role!=role||scope.material.original!=*source||scope.material.source_basis!=composite::NativeSourceBasis::Original{return Err(fail("Historical fresh native purpose/original custody differs"));}
   if let composite::NativeClass::CommittedDirect{receipt_json}=&source.class {
    let receipt:crate::storage::attachment_upload::Receipt=serde_json::from_str(receipt_json).map_err(io::Error::other)?;
    if scope.payloads.len()!=2||scope.payloads.iter().map(|b|b.role).collect::<std::collections::BTreeSet<_>>()!=[HistoricalPayloadRole::CommittedCanonical,HistoricalPayloadRole::CommittedStagePayload].into_iter().collect(){return Err(fail("Historical committed physical payload census incomplete or duplicated"));}
    let first=&scope.payloads[0];
    for b in &scope.payloads{b.role.relative(source)?;if b.source.inode==0||b.adopted.inode==0||b.length!=receipt.intent.byte_length as u64||b.sha256!=receipt.intent.binary_sha256||b.source!=first.source||b.adopted!=first.adopted{return Err(fail("Historical committed original/fresh payload identity/bytes differ"));}}
   }else if !scope.payloads.is_empty(){return Err(fail("Historical payload bindings outside committed class"));}

  }
  // Reuse only the existing wire identity/role topology validation. This local
  // temporary value is never stored in Ledger.composite or converted into its
  // live owner; the new historical field/constructor must stay purpose separate.
  composite::CompositeOwnerEnvelope{version:1,completed:self.completed.material.clone(),native:self.native.iter().map(|s|s.material.clone()).collect()}.validate(root)?;
  if serde_json::to_vec(self).map_err(io::Error::other)?.len()>16*1024*1024{return Err(fail("Historical destination owner exceeds bounded ledger"));}Ok(())
 }
}
