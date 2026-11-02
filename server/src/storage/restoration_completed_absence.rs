//! Separate generation-owned completed absence roles. No native writer constructor.
use super::{admission::Identity,fail};
use crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::{self,CompletedAbsenceEnvelope,CompletedAbsenceScope,CompletedDirectoryRole,CompletedFileRole};
use serde::{Deserialize,Serialize};
use std::{collections::{BTreeMap,BTreeSet},io};
#[derive(Clone,Debug,PartialEq,Eq,Serialize)]
pub(crate) struct CompletedOwnerEnvelope{pub version:u32,pub scopes:Vec<CompletedOwnerScope>}
impl<'de> Deserialize<'de> for CompletedOwnerEnvelope {
 fn deserialize<D:serde::Deserializer<'de>>(d:D)->Result<Self,D::Error>{
  #[derive(Deserialize)]#[serde(deny_unknown_fields)]struct Wire{version:u32,scopes:Vec<CompletedOwnerScope>}
  let wire=Wire::deserialize(d)?;
  if wire.version==1&&wire.scopes.iter().any(|s|s.source_basis.is_some())||wire.version==2&&wire.scopes.iter().any(|s|s.source_basis.is_none())||![1,2].contains(&wire.version){return Err(serde::de::Error::custom("Completed owner source basis/version mismatch"));}
  Ok(Self{version:wire.version,scopes:wire.scopes})
 }
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompletedOwnerScope{pub original:CompletedAbsenceScope,pub directories:Vec<CompletedOwnedDirectory>,pub files:Vec<CompletedOwnedFile>,
 #[serde(default,skip_serializing_if="Option::is_none",deserialize_with="source_basis_nonnull")]
 pub source_basis:Option<CompletedSourceBasis>}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompletedSourceBasis {pub root:Identity,pub directories:Vec<CompletedSourceDirectory>,pub files:Vec<CompletedSourceFile>}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompletedSourceDirectory{pub role:CompletedDirectoryRole,pub captured:Identity}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompletedSourceFile{pub role:CompletedFileRole,pub captured:Identity}
fn source_basis_nonnull<'de,D:serde::Deserializer<'de>>(d:D)->Result<Option<CompletedSourceBasis>,D::Error>{CompletedSourceBasis::deserialize(d).map(Some)}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompletedOwnedDirectory{pub role:CompletedDirectoryRole,pub adopted:Identity}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompletedOwnedFile{pub role:CompletedFileRole,pub adopted:Identity}
impl CompletedOwnerEnvelope {
 pub(crate) fn validate(&self,root:&Identity)->io::Result<()> {
  if ![1,2].contains(&self.version){return Err(fail("Unsupported completed owner envelope"));}
  let original=CompletedAbsenceEnvelope{version:1,source_kind:"original_native_direct_v1".into(),scopes:self.scopes.iter().map(|s|s.original.clone()).collect()};
  original.validate_shape().map_err(|_|fail("Malformed completed owner original scopes"))?;
  let mut forward=BTreeMap::new();let mut backward=BTreeMap::new();let mut source_root:Option<&Identity>=None;
  for scope in &self.scopes {
   capture::original_record_metadata(&scope.original).map_err(|_|fail("Malformed completed original records"))?;
   let basis=match(self.version,&scope.source_basis){(1,None)=>None,(2,Some(basis))=>{
    if basis.root.inode==0||source_root.is_some_and(|r|r!=&basis.root){return Err(fail("Completed v2 current source root differs"));}source_root=Some(&basis.root);
    let dirs=basis.directories.iter().map(|b|b.role.clone()).collect::<BTreeSet<_>>();let files=basis.files.iter().map(|b|b.role.clone()).collect::<BTreeSet<_>>();
    if dirs.len()!=basis.directories.len()||files.len()!=basis.files.len()||dirs!=scope.original.directories.iter().map(|b|b.role.clone()).collect()||files!=scope.original.files.iter().map(|b|b.role.clone()).collect()||basis.directories.iter().any(|b|b.captured.inode==0||matches!(b.role,CompletedDirectoryRole::DataRoot)&&b.captured!=basis.root)||basis.files.iter().any(|b|b.captured.inode==0){return Err(fail("Completed v2 source basis incomplete or substituted"));}Some(basis)
   },_=>return Err(fail("Completed owner source basis/version differs"))};

   let mut dirs=BTreeSet::new();let mut files=BTreeSet::new();
   for b in &scope.directories {
    let source=scope.original.directories.iter().find(|s|s.role==b.role).ok_or_else(||fail("Unbound completed owner directory"))?;
    if !dirs.insert(b.role.clone())||b.adopted.inode==0||matches!(b.role,CompletedDirectoryRole::DataRoot)&&&b.adopted!=root{return Err(fail("Completed adopted directory malformed"));}
    let source_identity=if let Some(basis)=basis{&basis.directories.iter().find(|s|s.role==b.role).ok_or_else(||fail("Completed v2 source directory absent"))?.captured}else{&source.captured};let old=(source_identity.device,source_identity.inode);let new=(b.adopted.device,b.adopted.inode);
    if forward.insert(old,new).is_some_and(|v|v!=new)||backward.insert(new,old).is_some_and(|v|v!=old){return Err(fail("Completed directory custody substitution"));}
   }
   for b in &scope.files {
    let source=scope.original.files.iter().find(|s|s.role==b.role).ok_or_else(||fail("Unbound completed owner file"))?;
    if !files.insert(b.role.clone())||b.adopted.inode==0{return Err(fail("Completed adopted file malformed"));}
    let source_identity=if let Some(basis)=basis{&basis.files.iter().find(|s|s.role==b.role).ok_or_else(||fail("Completed v2 source file absent"))?.captured}else{&source.captured};let old=(source_identity.device,source_identity.inode);let new=(b.adopted.device,b.adopted.inode);
    if forward.insert(old,new).is_some_and(|v|v!=new)||backward.insert(new,old).is_some_and(|v|v!=old){return Err(fail("Completed file custody substitution"));}
   }
   if dirs!=scope.original.directories.iter().map(|s|s.role.clone()).collect()||files!=scope.original.files.iter().map(|s|s.role.clone()).collect(){return Err(fail("Completed owner role inventory incomplete"));}
  }if self.version==2&&serde_json::to_vec(self).map_err(|_|fail("Completed owner encoding invalid"))?.len()>8*1024*1024{return Err(fail("Completed owner exceeds sealed bound"));}Ok(())
 }
}
