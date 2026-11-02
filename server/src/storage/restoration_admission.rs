//! Typed admission custody serialization only. No lease or generic file authority is minted here.
use super::{digest, fail, valid_hash, valid_id, valid_relative};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, io};
const NS: &str = ".attachment-upload-admission";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Identity { pub device:u64, pub inode:u64 }
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag="producer",rename_all="snake_case",deny_unknown_fields)]
pub(crate) enum Producer {
    Direct {request:DirectRequest},
    Replay {provider:String,incoming_id:String,actor_id:String,space_id:String,document_id:String,
        asset_id:String,payload_fingerprint:String,binary_sha256:String,byte_length:u64,
        #[serde(default,skip_serializing_if="Option::is_none")]
        draft_scope:Option<DraftUploadScope>},
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct DirectRequest { pub operation_id:String,pub origin:String,pub actor_id:String,
    pub space_id:String,pub document_id:String,pub original_name:String,pub mime_type:String,
    #[serde(default,skip_serializing_if="Option::is_none")]
    pub draft_scope:Option<DraftUploadScope> }
// Portable restoration mirror: attachment_upload itself is Linux-only. Exact
// roundtrip tests below bind these serialized fields to the native producer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct DraftUploadScope {pub schema_version:u8,pub draft_id:String,pub draft_created_at:String,
    #[serde(default,skip_serializing_if="Option::is_none")] pub copy_source:Option<CopySourceScope>}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CopySourceScope {pub schema_version:u8,
    #[serde(default,skip_serializing_if="Option::is_none")] pub source_kind:Option<String>,
    pub source_space_id:String,pub source_document_id:String,
    pub version_id:String,pub expected_source_version_sha256:String,pub binary_sha256:String,pub byte_length:i64}
impl CopySourceScope {
    fn validate(&self)->io::Result<()> {
        let selected=match self.source_kind.as_deref() {
            None=>valid_id(&self.version_id),
            Some("legacy_snapshot_v1")=>!self.version_id.is_empty() && self.version_id.len()<=255 && !matches!(self.version_id.as_str(),"."|"..") && !self.version_id.contains(&['/','\\','\0'][..]),
            Some(_)=>false,
        };
        if self.schema_version!=1 || self.source_space_id.trim().is_empty() || self.source_document_id.trim().is_empty()
            || !selected || !valid_hash(&self.expected_source_version_sha256) || !valid_hash(&self.binary_sha256) || self.byte_length<0 {return Err(fail("Invalid original attachment copy source"));}
        Ok(())
    }
}
impl DraftUploadScope {
    fn validate(&self)->io::Result<()> {
        if let Some(source)=&self.copy_source {source.validate()?;}
        if self.schema_version!=2 || !valid_id(&self.draft_id) || self.draft_created_at.trim().is_empty() {return Err(fail("Invalid original draft upload scope"));}
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct OriginalClaim {version:u8,attempt_id:String,producer:Producer,producer_sha256:String,
    maximum_bytes:u64,root:Identity,namespace:Identity,capsule:Identity}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag="role",rename_all="snake_case",deny_unknown_fields)]
pub(crate) enum DirectoryRole {
    DataRoot,AdmissionNamespace,AdmissionCapsule,DirectStageNamespace,DirectStageDirectory,
    ReplayBlobParent {relative:String},CanonicalRewriteNamespace,CanonicalRewriteDirectory {operation_id:String},
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct DirectoryBinding {pub role:DirectoryRole,pub original:Identity,pub captured:Identity,pub adopted:Identity}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(tag="role",rename_all="snake_case",deny_unknown_fields)]
pub(crate) enum FileRole {Claim, CapsuleRecord {name:String}, DirectStage {name:String},
    ReplayBlob {relative:String}, CanonicalRewrite {operation_id:String,name:String}}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileBinding {pub role:FileRole,pub original:Identity,pub captured:Identity,
    pub adopted:Identity,pub length:u64,pub sha256:String}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag="state",rename_all="snake_case",deny_unknown_fields)]
pub(crate) enum ReservationState {
    Reserved,
    Committed {committed_asset_id:String,committed_bytes:u64,winning_receipt_json:String,winning_receipt_sha256:String},
    Disposed {disposition_json:String,disposition_sha256:String},
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdmissionOwnerScope {
    pub namespace:String,pub attempt_id:String,pub original_claim_json:String,
    pub admission_claim_sha256:String,pub request_sha256:String,pub maximum_bytes:u64,
    pub reservation:ReservationState,pub directories:Vec<DirectoryBinding>,pub files:Vec<FileBinding>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdmissionOwnerEnvelope {pub version:u32,pub scopes:Vec<AdmissionOwnerScope>}
impl DirectoryRole {
    /// Root is a typed slot, never an empty relative-file binding.
    pub(crate) fn relative(&self,attempt:&str)->io::Result<Option<String>> {
        let path=match self {
            Self::DataRoot=>return Ok(None),Self::AdmissionNamespace=>NS.into(),
            Self::AdmissionCapsule=>format!("{NS}/{attempt}"),
            Self::DirectStageNamespace=>".attachment-upload-journal".into(),
            Self::DirectStageDirectory=>format!(".attachment-upload-journal/{attempt}"),
            Self::ReplayBlobParent {relative}=>relative.clone(),
            Self::CanonicalRewriteNamespace=>".canonical-rewrite-journal".into(),
            Self::CanonicalRewriteDirectory {operation_id}=>{
                if !valid_id(operation_id) {return Err(fail("Invalid admission rewrite operation"));}
                format!(".canonical-rewrite-journal/{operation_id}")
            }
        };
        if !valid_relative(&path)||path.contains('\\') {return Err(fail("Invalid admission directory role path"));}
        if matches!(self,Self::ReplayBlobParent{..}) && (!path.starts_with("spaces/") || path.split('/').count()!=4 || path.split('/').nth(2)!=Some("attachments")) {
            return Err(fail("Invalid replay blob parent role"));
        }
        Ok(Some(path))
    }
}
impl FileRole {
    pub(crate) fn relative(&self,attempt:&str)->io::Result<String> {
        let path=match self {
            Self::Claim=>format!("{NS}/{attempt}/claim.json"),
            Self::CapsuleRecord{name}=>{
                if !["stage-origin.json","replay-blob-origin.json","replay-rewrite-plan.json","replay-rewrite-origin.json","disposal-intent.json","disposal-tombstone.json","replay-disposal-intent.json","replay-disposal-tombstone.json"].contains(&name.as_str()) {
                    return Err(fail("Unknown admission capsule record role"));
                } format!("{NS}/{attempt}/{name}")
            }
            Self::DirectStage{name}=>{
                if !["reservation.json","request.json","payload.part","seal.json","intent.json","install-proof.json"].contains(&name.as_str()) {return Err(fail("Unknown direct stage role"));}
                format!(".attachment-upload-journal/{attempt}/{name}")
            }
            Self::ReplayBlob{relative}=>{
                if !relative.starts_with("spaces/") || relative.split('/').count()!=5 || relative.split('/').nth(2)!=Some("attachments") {return Err(fail("Invalid replay blob role"));}
                relative.clone()
            }
            Self::CanonicalRewrite{operation_id,name}=>{
                if !valid_id(operation_id)||!["manifest.json","install-proof.json","payload"].contains(&name.as_str()) {return Err(fail("Invalid rewrite file role"));}
                format!(".canonical-rewrite-journal/{operation_id}/{name}")
            }
        };
        if !valid_relative(&path)||path.contains('\\') {return Err(fail("Invalid admission file role path"));} Ok(path)
    }
}
fn identity_valid(id:&Identity)->bool {id.inode!=0}
fn raw_hash(raw:&str,hash:&str)->bool {valid_hash(hash)&&digest(raw.as_bytes())==hash}
impl AdmissionOwnerEnvelope {
    pub(crate) fn validate(&self,root:&Identity)->io::Result<()> {
        if self.version!=2||self.scopes.is_empty() {return Err(fail("Invalid admission owner envelope"));}
        let mut attempts=BTreeSet::new();
        for s in &self.scopes {
            if s.namespace!=NS||!valid_id(&s.attempt_id)||!attempts.insert(&s.attempt_id)
                ||s.maximum_bytes>i64::MAX as u64||!raw_hash(&s.original_claim_json,&s.admission_claim_sha256)
                ||!valid_hash(&s.request_sha256) {return Err(fail("Invalid admission scope"));}
            let claim:OriginalClaim=serde_json::from_str(&s.original_claim_json).map_err(|_|fail("Malformed original admission claim"))?;
            let producer=serde_json::to_vec(&claim.producer).map_err(io::Error::other)?;
            if claim.version!=1||claim.attempt_id!=s.attempt_id||claim.maximum_bytes!=s.maximum_bytes
                ||claim.producer_sha256!=s.request_sha256||digest(&producer)!=s.request_sha256 {
                return Err(fail("Admission claim and SQL reservation differ"));
            }
            match &claim.producer {
                Producer::Direct{request:r}=>{
                    if let Some(scope)=&r.draft_scope {scope.validate()?;}
                    if !valid_id(&r.operation_id)||[&r.origin,&r.actor_id,&r.space_id,&r.document_id,&r.mime_type].iter().any(|v|v.is_empty())
                        ||r.original_name.trim().is_empty()||r.original_name.len()>255 {return Err(fail("Invalid direct owner producer"));}
                }
                Producer::Replay{provider,incoming_id,actor_id,space_id,document_id,asset_id,payload_fingerprint,binary_sha256,byte_length,draft_scope}=>{
                    if let Some(scope)=draft_scope {scope.validate()?;}
                    if [provider,incoming_id,actor_id,space_id,document_id,asset_id].iter().any(|v|v.is_empty())
                        ||!valid_hash(payload_fingerprint)||!valid_hash(binary_sha256)||*byte_length>s.maximum_bytes {return Err(fail("Invalid replay owner producer"));}
                }
            }
            match &s.reservation {
                ReservationState::Reserved=>{},
                ReservationState::Committed{committed_asset_id,committed_bytes,winning_receipt_json,winning_receipt_sha256}=>{
                    if committed_asset_id.is_empty()||*committed_bytes>s.maximum_bytes||!raw_hash(winning_receipt_json,winning_receipt_sha256) {return Err(fail("Invalid committed admission row evidence"));}
                }
                ReservationState::Disposed{disposition_json,disposition_sha256}=>{
                    if !raw_hash(disposition_json,disposition_sha256) {return Err(fail("Invalid disposed admission row evidence"));}
                }
            }
            let mut roles=BTreeSet::new();let mut paths=BTreeSet::new();
            for b in &s.directories {
                if !roles.insert(&b.role)||!paths.insert(b.role.relative(&s.attempt_id)?)
                    ||![&b.original,&b.captured,&b.adopted].iter().all(|i|identity_valid(i)) {return Err(fail("Invalid admission directory bindings"));}
                let required=match b.role {DirectoryRole::DataRoot=>Some(&claim.root),DirectoryRole::AdmissionNamespace=>Some(&claim.namespace),DirectoryRole::AdmissionCapsule=>Some(&claim.capsule),_=>None};
                if required.is_some_and(|id|id!=&b.original)||(b.role==DirectoryRole::DataRoot&&&b.adopted!=root) {return Err(fail("Original claim directory custody differs"));}
                let direct=matches!(claim.producer,Producer::Direct{..});
                if (matches!(b.role,DirectoryRole::DirectStageNamespace|DirectoryRole::DirectStageDirectory)&&!direct)
                    ||(matches!(b.role,DirectoryRole::ReplayBlobParent{..}|DirectoryRole::CanonicalRewriteNamespace|DirectoryRole::CanonicalRewriteDirectory{..})&&direct) {return Err(fail("Cross-producer directory role"));}
            }
            if ![DirectoryRole::DataRoot,DirectoryRole::AdmissionNamespace,DirectoryRole::AdmissionCapsule].iter().all(|r|roles.contains(r)) {return Err(fail("Missing admission directory custody"));}
            let mut file_roles=BTreeSet::new();let mut file_paths=BTreeSet::new();
            for b in &s.files {
                let path=b.role.relative(&s.attempt_id)?;
                if !file_roles.insert(&b.role)||!file_paths.insert(path)||b.length>i64::MAX as u64||!valid_hash(&b.sha256)
                    ||![&b.original,&b.captured,&b.adopted].iter().all(|i|identity_valid(i)) {return Err(fail("Invalid admission file bindings"));}
                if b.role==FileRole::Claim&&(b.length!=s.original_claim_json.len() as u64||b.sha256!=s.admission_claim_sha256) {return Err(fail("Admission claim file differs"));}
                let direct=matches!(claim.producer,Producer::Direct{..});
                if (matches!(b.role,FileRole::DirectStage{..})&&!direct)||(matches!(b.role,FileRole::ReplayBlob{..}|FileRole::CanonicalRewrite{..})&&direct) {return Err(fail("Cross-producer file role"));}
            }
            if !file_roles.contains(&FileRole::Claim) {return Err(fail("Missing original claim file custody"));}
        } Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    fn id(n:u64)->Identity {Identity{device:1,inode:n}}
    pub(crate) fn fixture()->AdmissionOwnerEnvelope {
        let attempt=uuid::Uuid::new_v4().to_string();
        let producer=Producer::Direct{request:DirectRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"browser".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),original_name:"x.bin".into(),mime_type:"application/octet-stream".into()}};
        let request=digest(&serde_json::to_vec(&producer).unwrap());
        let claim=OriginalClaim{version:1,attempt_id:attempt.clone(),producer,producer_sha256:request.clone(),maximum_bytes:8,root:id(1),namespace:id(2),capsule:id(3)};
        let raw=serde_json::to_string(&claim).unwrap();let sha=digest(raw.as_bytes());
        AdmissionOwnerEnvelope{version:2,scopes:vec![AdmissionOwnerScope{namespace:NS.into(),attempt_id:attempt,original_claim_json:raw.clone(),admission_claim_sha256:sha.clone(),request_sha256:request,maximum_bytes:8,reservation:ReservationState::Reserved,
            directories:vec![(DirectoryRole::DataRoot,1),(DirectoryRole::AdmissionNamespace,2),(DirectoryRole::AdmissionCapsule,3)].into_iter().map(|(role,n)|DirectoryBinding{role,original:id(n),captured:id(n+10),adopted:id(n+20)}).collect(),
            files:vec![FileBinding{role:FileRole::Claim,original:id(4),captured:id(14),adopted:id(24),length:raw.len() as u64,sha256:sha}]}]}
    }
    pub(crate) fn ledger(a:Option<AdmissionOwnerEnvelope>,version:u32)->super::super::Ledger {
        super::super::Ledger{ historical_preservation:None, composite:None,completed_absence:None,version,generation_id:uuid::Uuid::new_v4().to_string(),backup_id:uuid::Uuid::new_v4().to_string(),manifest_sha256:"a".repeat(64),database_sha256:"b".repeat(64),root_device:1,root_inode:21,scopes:vec![],admission:a}
    }
    #[test] fn exact_claim_full_role_roundtrip() {
        let a=fixture();a.validate(&id(21)).unwrap();let raw=serde_json::to_vec(&a).unwrap();
        assert_eq!(serde_json::from_slice::<AdmissionOwnerEnvelope>(&raw).unwrap(),a);
        assert_eq!(DirectoryRole::DataRoot.relative(&a.scopes[0].attempt_id).unwrap(),None);
    }
    #[cfg(target_os="linux")]
    #[test] fn native_and_portable_producer_scope_roundtrip_preserves_exact_bytes() {
        use crate::storage::attachment_upload as native;
        let mut request=native::UploadRequest{draft_scope:None,operation_id:"11111111-1111-4111-8111-111111111111".into(),origin:"origin".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),original_name:"x.bin".into(),mime_type:"application/octet-stream".into()};
        let mut copied=native::DraftUploadScope{copy_source:None,schema_version:2,draft_id:"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),draft_created_at:"exact-created-at".into()};
        copied.copy_source=Some(native::CopySourceScope{source_kind:None,schema_version:1,source_space_id:"source-space".into(),source_document_id:"source-page".into(),version_id:"bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),expected_source_version_sha256:"c".repeat(64),binary_sha256:"b".repeat(64),byte_length:1});
        let mut legacy=copied.clone();legacy.copy_source.as_mut().unwrap().source_kind=Some("legacy_snapshot_v1".into());legacy.copy_source.as_mut().unwrap().version_id="legacy-opaque.asset".into();
        legacy.validate().unwrap();
        let portable:DraftUploadScope=serde_json::from_slice(&serde_json::to_vec(&legacy).unwrap()).unwrap();portable.validate().unwrap();
        let mut bad=portable.clone();bad.copy_source.as_mut().unwrap().source_kind=None;assert!(bad.validate().is_err());
        let mut bad=portable.clone();bad.copy_source.as_mut().unwrap().version_id="../escape".into();assert!(bad.validate().is_err());
        for scope in [None,Some(native::DraftUploadScope{copy_source:None,schema_version:2,draft_id:"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),draft_created_at:"exact-created-at".into()}),Some(copied),Some(legacy)] {
            request.draft_scope=scope.clone();
            let producers=[native::admission::Producer::Direct{request:request.clone()},native::admission::Producer::Replay{provider:"provider".into(),incoming_id:"incoming".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),asset_id:"asset".into(),payload_fingerprint:"a".repeat(64),binary_sha256:"b".repeat(64),byte_length:1,draft_scope:scope}];
            for producer in producers {
                let raw=serde_json::to_vec(&producer).unwrap();
                let portable:Producer=serde_json::from_slice(&raw).unwrap();
                assert_eq!(serde_json::to_vec(&portable).unwrap(),raw,"Portable capture must retain every scoped/legacy original producer byte");
                assert_eq!(serde_json::from_slice::<native::admission::Producer>(&serde_json::to_vec(&portable).unwrap()).unwrap(),producer);
            }
        }
    }
    #[test] fn scoped_original_claim_roundtrip_validates_incarnation_without_rebasing() {
        let mut a=fixture();
        for schema_version in [2,1] {
            let scope=&mut a.scopes[0];
            let mut claim:OriginalClaim=serde_json::from_str(&scope.original_claim_json).unwrap();
            let Producer::Direct{request}=&mut claim.producer else {unreachable!()};
            request.draft_scope=Some(DraftUploadScope{copy_source:None,schema_version,draft_id:"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),draft_created_at:"exact-original-created".into()});
            claim.producer_sha256=digest(&serde_json::to_vec(&claim.producer).unwrap());
            scope.original_claim_json=serde_json::to_string(&claim).unwrap();scope.request_sha256=claim.producer_sha256;
            scope.admission_claim_sha256=digest(scope.original_claim_json.as_bytes());
            let file=scope.files.iter_mut().find(|f|f.role==FileRole::Claim).unwrap();file.length=scope.original_claim_json.len() as u64;file.sha256=scope.admission_claim_sha256.clone();
            if schema_version==2 {
                a.validate(&id(21)).unwrap();
                assert_eq!(serde_json::from_slice::<AdmissionOwnerEnvelope>(&serde_json::to_vec(&a).unwrap()).unwrap(),a);
            } else {assert!(a.validate(&id(21)).is_err(),"Even internally consistent hashes cannot admit an invalid original scope");}
        }
    }
    #[test] fn missing_original_directory_or_claim_refuses() {
        let mut a=fixture();a.scopes[0].directories.pop();assert!(a.validate(&id(21)).is_err());
        let mut a=fixture();a.scopes[0].directories[2].original=id(99);assert!(a.validate(&id(21)).is_err());
        let mut a=fixture();a.scopes[0].files.clear();assert!(a.validate(&id(21)).is_err());
        let a=fixture();assert!(a.validate(&id(999)).is_err());
    }
    #[test] fn full_sql_claim_binding_refuses_substitution() {
        let mut a=fixture();a.scopes[0].request_sha256="c".repeat(64);assert!(a.validate(&id(21)).is_err());
        let mut a=fixture();a.scopes[0].maximum_bytes=9;assert!(a.validate(&id(21)).is_err());
        let mut a=fixture();a.scopes[0].original_claim_json.push(' ');assert!(a.validate(&id(21)).is_err());
        let mut a=fixture();a.scopes[0].attempt_id=uuid::Uuid::new_v4().to_string();assert!(a.validate(&id(21)).is_err());
    }
    #[test] fn terminal_json_hash_and_capacity_bound() {
        let mut a=fixture();a.scopes[0].reservation=ReservationState::Disposed{disposition_json:"{}".into(),disposition_sha256:digest(b"{}")};a.validate(&id(21)).unwrap();
        if let ReservationState::Disposed{disposition_json,..}=&mut a.scopes[0].reservation {disposition_json.push(' ');}
        assert!(a.validate(&id(21)).is_err());
        let mut a=fixture();a.scopes[0].reservation=ReservationState::Committed{committed_asset_id:"asset".into(),committed_bytes:9,winning_receipt_json:"{}".into(),winning_receipt_sha256:digest(b"{}")};assert!(a.validate(&id(21)).is_err());
    }
    #[test] fn duplicate_unknown_cross_producer_roles_refuse() {
        let mut a=fixture();let duplicate=a.scopes[0].directories[0].clone();a.scopes[0].directories.push(duplicate);assert!(a.validate(&id(21)).is_err());
        let mut a=fixture();a.scopes[0].directories.push(DirectoryBinding{role:DirectoryRole::ReplayBlobParent{relative:"spaces/s/attachments/page".into()},original:id(5),captured:id(15),adopted:id(25)});assert!(a.validate(&id(21)).is_err());
        assert!(DirectoryRole::ReplayBlobParent{relative:"spaces/s/../page".into()}.relative("x").is_err());
        assert!(FileRole::CapsuleRecord{name:"foreign.json".into()}.relative("x").is_err());
        let a=fixture();let mut raw=serde_json::to_string(&a).unwrap();raw=raw.replacen("{","{\"unexpected\":0,",1);assert!(serde_json::from_str::<AdmissionOwnerEnvelope>(&raw).is_err());
        let raw=serde_json::to_string(&a).unwrap().replacen("\"version\":2","\"version\":2,\"version\":2",1);assert!(serde_json::from_str::<AdmissionOwnerEnvelope>(&raw).is_err());
    }
    #[test] fn ledger_v1_omits_field_and_null_refuses() {
        let old=ledger(None,1);super::super::validate(&old).unwrap();let raw=serde_json::to_string(&old).unwrap();assert!(!raw.contains("admission"));
        let inserted=raw.replacen("{","{\"admission\":null,",1);assert!(serde_json::from_str::<super::super::Ledger>(&inserted).is_err());
        assert!(super::super::validate(&ledger(Some(fixture()),1)).is_err());
        assert!(super::super::validate(&ledger(None,2)).is_err());
        super::super::validate(&ledger(Some(fixture()),2)).unwrap();
    }
    #[test] fn empty_envelope_and_claim_leaf_bytes_refuse() {
        let mut a=fixture();a.scopes.clear();assert!(a.validate(&id(21)).is_err());
        let mut a=fixture();a.scopes[0].files[0].sha256="f".repeat(64);assert!(a.validate(&id(21)).is_err());
        let mut a=fixture();a.scopes[0].files[0].captured.inode=0;assert!(a.validate(&id(21)).is_err());
    }
}

/// A row projection from the caller's coherent SQL snapshot. All columns are retained,
/// including NULL shape. This cannot authorize adoption or prove a winning receipt.
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CapturedReservation {
    pub operation_id:String,pub request_sha256:String,pub admission_claim_sha256:String,
    pub maximum_bytes:i64,pub state:String,pub committed_asset_id:Option<String>,
    pub committed_bytes:Option<i64>,pub disposition_json:Option<String>,pub disposition_sha256:Option<String>,
}
impl CapturedReservation {
    pub(super) fn matches_claim(&self,raw:&[u8])->io::Result<()> {
        let claim:OriginalClaim=serde_json::from_slice(raw).map_err(|_|fail("Malformed captured claim"))?;
        if claim.version!=1||claim.attempt_id!=self.operation_id||!valid_id(&self.operation_id)
            ||self.maximum_bytes<0||claim.maximum_bytes!=self.maximum_bytes as u64
            ||digest(raw)!=self.admission_claim_sha256||claim.producer_sha256!=self.request_sha256
            ||digest(&serde_json::to_vec(&claim.producer).map_err(io::Error::other)?)!=self.request_sha256 {
            return Err(fail("Captured reservation differs from original claim"));
        }
        let terminal=match self.state.as_str() {
            "reserved"=>self.committed_asset_id.is_none()&&self.committed_bytes.is_none()&&self.disposition_json.is_none()&&self.disposition_sha256.is_none(),
            "committed"=>self.committed_asset_id.as_ref().is_some_and(|s|!s.is_empty())&&self.committed_bytes.is_some_and(|n|n>=0&&n<=self.maximum_bytes)&&self.disposition_json.is_none()&&self.disposition_sha256.is_none(),
            "disposed"=>self.committed_asset_id.is_none()&&self.committed_bytes.is_none()&&match (&self.disposition_json,&self.disposition_sha256) {(Some(raw),Some(hash))=>raw_hash(raw,hash),_=>false},
            _=>false,
        };
        if !terminal {return Err(fail("Captured reservation terminal shape differs"));} Ok(())
    }
}
#[cfg(target_os="linux")]
pub(crate) mod capture {
    use super::*;
    use crate::storage::attachment_upload::admission::WriterLease;
    use sqlx::{AnyConnection,Row};
    use std::path::Path;
    /// Source exclusion only: native capsules are held through snapshot and final readback.
    /// Restored capsules deliberately refuse until prior finalized directory roles are verified.
    /// No lease is obtained on a copied snapshot and no owner token is manufactured.
    pub(crate) struct NativeCaptureLeases {held:Vec<(WriterLease,CapturedReservation)>}
    async fn row(tx:&mut AnyConnection,attempt:&str)->anyhow::Result<CapturedReservation> {
        let r=sqlx::query("SELECT operation_id,request_sha256,admission_claim_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,disposition_json,disposition_sha256 FROM attachment_capacity_reservations WHERE operation_id=$1")
            .bind(attempt).fetch_one(&mut *tx).await?;
        Ok(CapturedReservation{operation_id:r.try_get("operation_id")?,request_sha256:r.try_get("request_sha256")?,admission_claim_sha256:r.try_get("admission_claim_sha256")?,maximum_bytes:r.try_get("maximum_bytes")?,state:r.try_get("state")?,committed_asset_id:r.try_get("committed_asset_id")?,committed_bytes:r.try_get("committed_bytes")?,disposition_json:r.try_get("disposition_json")?,disposition_sha256:r.try_get("disposition_sha256")?})
    }

    #[cfg(test)]
    mod engine_tests {
        use super::*;
        use sqlx::Connection;
        #[tokio::test]
        #[ignore="requires an owned disposable SQLite or PostgreSQL fixture"]
        async fn exact_source_snapshot_and_kernel_lease() {
            let url=std::env::var("STARLANE_CAPACITY_TEST_URL").expect("owned fixture URL");sqlx::any::install_default_drivers();
            let pg=url.starts_with("postgres");let schema=format!("admission_capture_{}",uuid::Uuid::new_v4().simple());
            if pg {let mut c=sqlx::PgConnection::connect(&url).await.unwrap();sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&mut c).await.unwrap();c.close().await.unwrap();}
            let pool=sqlx::any::AnyPoolOptions::new().max_connections(2).after_connect(move|c,_|{let schema=schema.clone();Box::pin(async move{if pg {sqlx::query(&format!("SET search_path TO {schema}")).execute(c).await?;}Ok(())})}).connect(&url).await.unwrap();
            sqlx::raw_sql(include_str!("attachment_capacity_schema.sql")).execute(&pool).await.unwrap();
            let root=std::env::temp_dir().join(format!("native-capture-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();
            let attempt=uuid::Uuid::new_v4().to_string();
            let req=crate::storage::attachment_upload::UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"browser".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),original_name:"x.bin".into(),mime_type:"application/octet-stream".into()};
            let writer=WriterLease::begin(&root,&attempt,crate::storage::attachment_upload::admission::Producer::Direct{request:req},8).unwrap();
            sqlx::query("INSERT INTO attachment_capacity_reservations(operation_id,request_sha256,admission_claim_sha256,maximum_bytes,state) VALUES($1,$2,$3,8,'reserved')").bind(&attempt).bind(writer.producer_sha256()).bind(writer.claim_sha256()).execute(&pool).await.unwrap();
            let mut c=pool.acquire().await.unwrap();let expected=row(&mut c,&attempt).await.unwrap();
            assert!(NativeCaptureLeases::hold(&root,&mut c,&[expected.clone()]).await.is_err(),"active original writer must refuse capture");drop(writer);
            assert!(NativeCaptureLeases::hold(&root,&mut c,&[]).await.is_err(),"missing reservation inventory must refuse");
            let mut wrong=expected.clone();wrong.admission_claim_sha256="f".repeat(64);assert!(NativeCaptureLeases::hold(&root,&mut c,&[wrong]).await.is_err());
            let capture=NativeCaptureLeases::hold(&root,&mut c,&[expected]).await.unwrap();capture.recheck(&mut c).await.unwrap();assert!(WriterLease::open_idle(&root,&attempt).is_err(),"capture retains original flock");
            sqlx::query("UPDATE attachment_capacity_reservations SET maximum_bytes=9 WHERE operation_id=$1").bind(&attempt).execute(&mut *c).await.unwrap();assert!(capture.recheck(&mut c).await.is_err(),"changed exact SQL row must refuse");
            drop(capture);let idle=WriterLease::open_idle(&root,&attempt).unwrap();idle.verify().unwrap();drop(idle);drop(c);pool.close().await;std::fs::remove_dir_all(root).unwrap();
            println!("ADMISSION_NATIVE_SOURCE_CAPTURE_FULL_SQL_ACTIVE_WRITER_HELD_LEASE_REFUSAL_PASS {}",if pg {"Postgres"}else{"SQLite"});
        }
    }
    impl NativeCaptureLeases {
        /// Caller must already own the coherent table snapshot and a full capsule inventory.
        /// No row may be omitted; orphan capsules and row/capsule mismatch are classified separately.
        pub(crate) async fn hold(root:&Path,tx:&mut AnyConnection,expected:&[CapturedReservation])->anyhow::Result<Self> {
            let all:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id").fetch_all(&mut *tx).await?;
            let mut keys:Vec<String>=expected.iter().map(|r|r.operation_id.clone()).collect();keys.sort();
            anyhow::ensure!(all==keys,"Capture reservation inventory incomplete or duplicated");
            let mut seen=BTreeSet::new();let mut held=Vec::new();
            for expected in expected {
                anyhow::ensure!(seen.insert(&expected.operation_id),"Duplicate capture reservation");
                let lease=WriterLease::open_idle(root,&expected.operation_id)?;
                expected.matches_claim(lease.original_claim())?;
                anyhow::ensure!(row(tx,&expected.operation_id).await?==*expected,"Reservation changed before source capture");
                held.push((lease,expected.clone()));
            }
            Ok(Self{held})
        }
        /// Role facts originate only from original native proof under retained source leases.
        /// This first classifier refuses terminal rows and missing pre-payload origin records.
        pub(crate) async fn classify(&self,tx:&mut sqlx::Transaction<'_,sqlx::Any>)->anyhow::Result<ClassifiedAdmissionSource> {
            self.recheck(&mut **tx).await?;
            let mut scopes=Vec::new();let mut reservations=Vec::new();let mut leases=Vec::new();
            for (lease,row) in &self.held {
                anyhow::ensure!(row.state=="reserved","Terminal admission capture requires separate receipt/tombstone classification");
                scopes.push(lease.capture_original_scope(tx).await?);reservations.push(row.clone());leases.push(lease.clone());
            }
            anyhow::ensure!(!scopes.is_empty(),"No classified admission roles");
            let scopes=AdmissionOwnerEnvelope{version:2,scopes};
            let root=scopes.scopes[0].directories.iter().find(|b|b.role==DirectoryRole::DataRoot).ok_or_else(||fail("Captured root role missing"))?.adopted.clone();
            scopes.validate(&root)?;self.recheck(&mut **tx).await?;
            let capsule_fences=leases.iter().map(|l|l.capture_record_fence()).collect::<io::Result<Vec<_>>>()?;
            Ok(ClassifiedAdmissionSource{scopes,reservations,leases,cold_bundle:None,capsule_fences,completed:None})
        }
        /// Supported role subset; exclusion leases and exact SQL rows for ALL capsules remain held.
        /// Terminal, pre-origin and installed cases omit roles only after explicit native validation.
        pub(crate) async fn classify_supported(&self,tx:&mut sqlx::Transaction<'_,sqlx::Any>)->anyhow::Result<ClassifiedAdmissionSource> {
            self.recheck(&mut **tx).await?;let mut scopes=Vec::new();let mut reservations=Vec::new();let mut leases=Vec::new();
            for (lease,row) in &self.held {
                lease.verify_capture_record_shapes()?;
                if row.state=="reserved" {if let Some(scope)=lease.capture_supported_scope(tx).await? {scopes.push(scope);}}
                reservations.push(row.clone());leases.push(lease.clone());
            }
            let scopes=AdmissionOwnerEnvelope{version:2,scopes};
            if let Some(scope)=scopes.scopes.first(){let root=scope.directories.iter().find(|b|b.role==DirectoryRole::DataRoot).ok_or_else(||fail("Missing classified root"))?.captured.clone();scopes.validate(&root)?;}
            let capsule_fences=leases.iter().map(|l|l.capture_record_fence()).collect::<io::Result<Vec<_>>>()?;
            let source=ClassifiedAdmissionSource{scopes,reservations,leases,cold_bundle:None,capsule_fences,completed:None};source.fence()?;self.recheck(&mut **tx).await?;Ok(source)
        }
        /// Completed source proof is borrowed from these very native leases.
        /// No scope metadata or caller-selected exclusion can bypass capture shape checks.
        pub(crate) async fn capture_completed(&self,root:&Path,tx:&mut sqlx::Transaction<'_,sqlx::Any>)->anyhow::Result<std::sync::Arc<crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::NativeCompletedCapture>>{
            self.recheck(&mut **tx).await?;
            let paths=crate::storage::attachment_capacity::retained_paths_in(&mut **tx).await?;
            let borrowed=self.held.iter().map(|(lease,_)|lease.clone()).collect::<Vec<_>>();
            Ok(std::sync::Arc::new(crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::NativeCompletedCapture::capture_in(root,&mut **tx,paths,&borrowed).await?))
        }
        pub(crate) async fn classify_with_completed(&self,tx:&mut sqlx::Transaction<'_,sqlx::Any>,completed:&std::sync::Arc<crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::NativeCompletedCapture>)->anyhow::Result<ClassifiedAdmissionSource>{
            self.recheck(&mut **tx).await?;completed.fence_in(&mut **tx).await?;
            let mut scopes=Vec::new();let mut reservations=Vec::new();let mut leases=Vec::new();let mut capsule_fences=Vec::new();
            for (lease,row) in &self.held {
                if completed.covers_exact_lease(lease)? {anyhow::ensure!(row.state=="committed","Completed original reservation state differs");}
                else {lease.verify_capture_record_shapes()?;if row.state=="reserved"{if let Some(scope)=lease.capture_supported_scope(tx).await?{scopes.push(scope);}}capsule_fences.push(lease.capture_record_fence()?);}
                reservations.push(row.clone());leases.push(lease.clone());
            }
            let scopes=AdmissionOwnerEnvelope{version:2,scopes};
            if let Some(scope)=scopes.scopes.first(){let root=scope.directories.iter().find(|b|b.role==DirectoryRole::DataRoot).ok_or_else(||fail("Missing classified root"))?.captured.clone();scopes.validate(&root)?;}
            let source=ClassifiedAdmissionSource{scopes,reservations,leases,cold_bundle:None,capsule_fences,completed:Some(completed.clone())};source.fence()?;completed.fence_in(&mut **tx).await?;self.recheck(&mut **tx).await?;Ok(source)
        }
        /// Run again after original source inode/inventory and SQL readback while retaining Self.
        pub(crate) async fn recheck(&self,tx:&mut AnyConnection)->anyhow::Result<()> {
            let all:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id").fetch_all(&mut *tx).await?;
            let mut keys:Vec<String>=self.held.iter().map(|(_,r)|r.operation_id.clone()).collect();keys.sort();
            anyhow::ensure!(all==keys,"Capture reservation inventory changed");
            for (lease,expected) in &self.held {
                lease.verify()?;expected.matches_claim(lease.original_claim())?;
                anyhow::ensure!(row(tx,&expected.operation_id).await?==*expected,"Reservation changed during source capture");
            } Ok(())
        }
    }
}

#[cfg(test)]
mod captured_row_tests {
    use super::*;
    fn row()->(CapturedReservation,Vec<u8>) {
        let owner=tests::fixture();let s=&owner.scopes[0];
        (CapturedReservation{operation_id:s.attempt_id.clone(),request_sha256:s.request_sha256.clone(),admission_claim_sha256:s.admission_claim_sha256.clone(),maximum_bytes:8,state:"reserved".into(),committed_asset_id:None,committed_bytes:None,disposition_json:None,disposition_sha256:None},s.original_claim_json.as_bytes().to_vec())
    }
    #[test] fn exact_source_row_null_shape_refuses() {
        let (mut r,raw)=row();r.matches_claim(&raw).unwrap();r.committed_bytes=Some(0);assert!(r.matches_claim(&raw).is_err());
        let (mut r,raw)=row();r.state="disposed".into();r.disposition_json=Some("{}".into());r.disposition_sha256=Some(digest(b"{}"));r.matches_claim(&raw).unwrap();r.disposition_sha256=Some("f".repeat(64));assert!(r.matches_claim(&raw).is_err());
        let (mut r,raw)=row();r.state="committed".into();r.committed_bytes=Some(9);r.committed_asset_id=Some("asset".into());assert!(r.matches_claim(&raw).is_err());
    }
    #[test] fn source_row_wrong_producer_or_claim_refuses() {
        let (mut r,raw)=row();r.request_sha256="f".repeat(64);assert!(r.matches_claim(&raw).is_err());
        let (mut r,raw)=row();r.admission_claim_sha256="f".repeat(64);assert!(r.matches_claim(&raw).is_err());
        let (mut r,raw)=row();r.operation_id=uuid::Uuid::new_v4().to_string();assert!(r.matches_claim(&raw).is_err());
    }
}

#[cfg(target_os="linux")]
pub(crate) struct ClassifiedAdmissionSource {
    pub(super) scopes:AdmissionOwnerEnvelope,
    pub(super) reservations:Vec<CapturedReservation>,
    leases:Vec<crate::storage::attachment_upload::admission::WriterLease>,
    cold_bundle:Option<std::sync::Arc<crate::storage::backup_inventory::ValidatedBundle>>,
    capsule_fences:Vec<crate::storage::attachment_upload::admission::CapturedCapsuleRecords>,
    completed:Option<std::sync::Arc<crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::NativeCompletedCapture>>,
}
#[cfg(target_os="linux")]
impl ClassifiedAdmissionSource {
    pub(crate) fn fence(&self)->io::Result<()> {if let Some(b)=&self.cold_bundle {b.fence()?;}for f in &self.capsule_fences {f.verify()?;}if let Some(c)=&self.completed{c.fence().map_err(|_|fail("Held completed native source changed"))?;}for l in &self.leases {if self.completed.as_ref().map(|c|c.covers_exact_lease(l)).transpose().map_err(|_|fail("Completed source association changed"))?.unwrap_or(false){l.verify()?;}else{l.verify_capture_record_shapes()?;}if let Some(s)=self.scopes.scopes.iter().find(|s|s.attempt_id==l.attempt_id()){l.verify_captured_scope(s)?;}}Ok(())}
    pub(super) async fn completed_sql(&self,conn:&mut sqlx::AnyConnection)->io::Result<()> {if let Some(c)=&self.completed{c.fence_in(conn).await.map_err(|_|fail("Completed source SQL/native proof changed"))?;}Ok(())}
    pub(crate) fn source_roles(&self)->&AdmissionOwnerEnvelope {&self.scopes}
}

/// Portable capture evidence contains no adopted inode and can never mint owner authority.
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdmissionCaptureEnvelope {pub version:u32,pub scopes:Vec<AdmissionCaptureScope>}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AdmissionCaptureScope {
 pub namespace:String,pub attempt_id:String,pub original_claim_json:String,
 pub admission_claim_sha256:String,pub request_sha256:String,pub maximum_bytes:u64,
 pub reservation:ReservationState,pub directories:Vec<CaptureDirectoryBinding>,pub files:Vec<CaptureFileBinding>,
}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CaptureDirectoryBinding {pub role:DirectoryRole,pub original:Identity,pub captured:Identity}
#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct CaptureFileBinding {pub role:FileRole,pub original:Identity,pub captured:Identity,pub length:u64,pub sha256:String}
impl AdmissionCaptureEnvelope {
 /// First transport deliberately supports original native reserved custody only.
 /// Prior restored source needs a separate authenticated finalized lineage classifier.
 pub(crate) fn validate_native_reserved(&self)->io::Result<()> {
  if self.version!=2 {return Err(fail("Invalid admission capture version"));}
  if self.scopes.is_empty(){return Ok(());}
  let roles=self.native_roles()?;
  let root=roles.scopes[0].directories.iter().find(|b|b.role==DirectoryRole::DataRoot).ok_or_else(||fail("Missing captured admission root"))?.captured.clone();
  roles.validate(&root)
 }
 pub(super) fn native_roles(&self)->io::Result<AdmissionOwnerEnvelope> {
  let mut scopes=Vec::new();
  for s in &self.scopes {
   if !matches!(s.reservation,ReservationState::Reserved)
    ||s.directories.iter().any(|b|b.original!=b.captured)
    ||s.files.iter().any(|b|b.original!=b.captured) {return Err(fail("Admission capture lacks original native reserved custody"));}
   scopes.push(AdmissionOwnerScope{namespace:s.namespace.clone(),attempt_id:s.attempt_id.clone(),original_claim_json:s.original_claim_json.clone(),admission_claim_sha256:s.admission_claim_sha256.clone(),request_sha256:s.request_sha256.clone(),maximum_bytes:s.maximum_bytes,reservation:s.reservation.clone(),directories:s.directories.iter().map(|b|DirectoryBinding{role:b.role.clone(),original:b.original.clone(),captured:b.captured.clone(),adopted:b.captured.clone()}).collect(),files:s.files.iter().map(|b|FileBinding{role:b.role.clone(),original:b.original.clone(),captured:b.captured.clone(),adopted:b.captured.clone(),length:b.length,sha256:b.sha256.clone()}).collect()});
  } Ok(AdmissionOwnerEnvelope{version:2,scopes})
 }
}
#[cfg(target_os="linux")]
impl ClassifiedAdmissionSource {
 pub(crate) fn capture_envelope(&self)->io::Result<AdmissionCaptureEnvelope> {
  self.fence()?;
  let envelope=AdmissionCaptureEnvelope{version:2,scopes:self.scopes.scopes.iter().map(|s|AdmissionCaptureScope{namespace:s.namespace.clone(),attempt_id:s.attempt_id.clone(),original_claim_json:s.original_claim_json.clone(),admission_claim_sha256:s.admission_claim_sha256.clone(),request_sha256:s.request_sha256.clone(),maximum_bytes:s.maximum_bytes,reservation:s.reservation.clone(),directories:s.directories.iter().map(|b|CaptureDirectoryBinding{role:b.role.clone(),original:b.original.clone(),captured:b.captured.clone()}).collect(),files:s.files.iter().map(|b|CaptureFileBinding{role:b.role.clone(),original:b.original.clone(),captured:b.captured.clone(),length:b.length,sha256:b.sha256.clone()}).collect()}).collect()};
  envelope.validate_native_reserved()?;self.fence()?;Ok(envelope)
 }
}
#[cfg(test)]
mod capture_transport_tests {
 use super::*;
 fn sample()->AdmissionCaptureEnvelope {
  let a=tests::fixture();let scopes=a.scopes.into_iter().map(|s|AdmissionCaptureScope{namespace:s.namespace,attempt_id:s.attempt_id,original_claim_json:s.original_claim_json,admission_claim_sha256:s.admission_claim_sha256,request_sha256:s.request_sha256,maximum_bytes:s.maximum_bytes,reservation:s.reservation,directories:s.directories.into_iter().map(|b|CaptureDirectoryBinding{role:b.role,original:b.original.clone(),captured:b.original}).collect(),files:s.files.into_iter().map(|b|CaptureFileBinding{role:b.role,original:b.original.clone(),captured:b.original,length:b.length,sha256:b.sha256}).collect()}).collect();AdmissionCaptureEnvelope{version:2,scopes}
 }
 #[test]fn transport_has_no_destination_adoption(){let a=sample();a.validate_native_reserved().unwrap();let raw=serde_json::to_string(&a).unwrap();assert!(!raw.contains("adopted"));assert_eq!(serde_json::from_str::<AdmissionCaptureEnvelope>(&raw).unwrap(),a);let mut value=serde_json::to_value(&a).unwrap();value["scopes"][0]["directories"][0]["adopted"]=serde_json::json!({"device":1,"inode":999});assert!(serde_json::from_value::<AdmissionCaptureEnvelope>(value).is_err());}
 #[test]fn transport_foreign_claim_custody_terminal_refuse(){let mut a=sample();a.scopes[0].directories[0].captured.inode+=1;assert!(a.validate_native_reserved().is_err());let mut a=sample();a.scopes[0].files[0].sha256="f".repeat(64);assert!(a.validate_native_reserved().is_err());let mut a=sample();a.scopes[0].reservation=ReservationState::Disposed{disposition_json:"{}".into(),disposition_sha256:digest(b"{}")};assert!(a.validate_native_reserved().is_err());}
}

#[cfg(target_os="linux")]
impl ClassifiedAdmissionSource {
 /// Requires the qualified fully inventoried owned backup, not caller supplied JSON.
 /// Copies can preserve source facts; only certified destination publication mints ownership.
 pub(crate) async fn cold_bundle(bundle:std::sync::Arc<crate::storage::backup_inventory::ValidatedBundle>)->io::Result<Self> {
  bundle.fence()?;
  let capture=bundle.manifest.admission_owner_capture.as_ref().ok_or_else(||fail("Backup has no classified admission capture; reservation retained"))?;
  capture.validate_native_reserved()?;
  let scopes=capture.native_roles()?;
  if scopes.scopes.is_empty(){return Err(fail("Empty admission evidence grants no authority"));}
  Self::cold_scopes(bundle,scopes).await
 }
 /// Distinct qualified prior-finalized capture; native version2 validation is unchanged.
 pub(crate) async fn cold_prior_bundle(bundle:std::sync::Arc<crate::storage::backup_inventory::ValidatedBundle>,evidence:&super::linux::PriorRestoredCaptureEvidence)->io::Result<Self> {
  bundle.fence()?;let mut conn=bundle.db.acquire().await.map_err(|_|fail("Prior captured SQL unavailable"))?;
  let scopes=super::linux::prior_capture_bundle_roles(&bundle,&mut conn,evidence).await?;drop(conn);
  Self::cold_scopes(bundle,scopes).await
 }
 pub(super) async fn cold_scopes(bundle:std::sync::Arc<crate::storage::backup_inventory::ValidatedBundle>,scopes:AdmissionOwnerEnvelope)->io::Result<Self> {
  bundle.fence()?;
  let mut reservations=Vec::new();let mut replay_proofs=Vec::new();
  for scope in &scopes.scopes {
   for b in &scope.directories {
    let expected=if let Some(path)=b.role.relative(&scope.attempt_id)? {let i=bundle.manifest.source_custody.directories.0.get(&path).ok_or_else(||fail("Cold original captured directory absent"))?;Identity{device:i.device,inode:i.inode}}else{let i=&bundle.manifest.source_custody.root;Identity{device:i.device,inode:i.inode}};
    if b.captured!=expected {return Err(fail("Cold capture directory source custody differs"));}
   }
   let claim:OriginalClaim=serde_json::from_str(&scope.original_claim_json).map_err(|_|fail("Invalid cold original producer"))?;
   let direct=matches!(&claim.producer,Producer::Direct{..});
   let mut raws=std::collections::BTreeMap::new();
   for b in &scope.files {
    let path=b.role.relative(&scope.attempt_id)?;
    let i=bundle.manifest.source_custody.files.0.get(&path).ok_or_else(||fail("Cold original captured file absent"))?;
    let bytes=bundle.manifest.files.0.get(&path).ok_or_else(||fail("Cold captured file bytes absent"))?;
    let stream_direct=direct&&matches!(&b.role,FileRole::DirectStage{name} if name=="payload.part");
    if i.links!=1||b.captured!=(Identity{device:i.device,inode:i.inode})||b.length!=bytes.size_bytes||b.sha256!=bytes.sha256||(!stream_direct&&b.length>64*1024*1024) {return Err(fail("Cold original captured file custody or bound differs"));}
    let mut f=bundle.open_owned(&path)?;
    if stream_direct {crate::storage::attachment_upload::admission::verify_captured_direct_payload(&f,b.length,&b.sha256,scope.maximum_bytes)?;raws.insert(b.role.clone(),Vec::new());}
    else {let mut raw=Vec::new();std::io::Read::read_to_end(&mut f,&mut raw)?;if raw.len() as u64!=b.length||digest(&raw)!=b.sha256 {return Err(fail("Cold captured file changed while parsing"));}raws.insert(b.role.clone(),raw);}
   }
   let owned:std::collections::BTreeSet<String>=scope.files.iter().map(|b|b.role.relative(&scope.attempt_id)).collect::<io::Result<_>>()?;
   let mut prefixes=Vec::new();let mut individual=std::collections::BTreeSet::new();
   for b in &scope.directories {if matches!(b.role,DirectoryRole::AdmissionCapsule|DirectoryRole::DirectStageDirectory|DirectoryRole::CanonicalRewriteDirectory{..}){prefixes.push(format!("{}/",b.role.relative(&scope.attempt_id)?.ok_or_else(||fail("Cold operation directory missing"))?));}}
   for b in &scope.files {if matches!(b.role,FileRole::ReplayBlob{..}){individual.insert(b.role.relative(&scope.attempt_id)?);}}
   let observed:std::collections::BTreeSet<String>=bundle.manifest.files.0.keys().filter(|p|individual.contains(*p)||prefixes.iter().any(|prefix|p.starts_with(prefix))).cloned().collect();
   if owned!=observed{return Err(fail("Cold captured operation inventory omitted a record"));}
   match claim.producer {Producer::Direct{..}=>crate::storage::attachment_upload::admission::validate_captured_partial_direct(scope,&raws)?,Producer::Replay{..}=>replay_proofs.push(crate::storage::attachment_upload::admission::validate_captured_replay_custody(scope,&raws)?),}
   reservations.push(CapturedReservation{operation_id:scope.attempt_id.clone(),request_sha256:scope.request_sha256.clone(),admission_claim_sha256:scope.admission_claim_sha256.clone(),maximum_bytes:scope.maximum_bytes as i64,state:"reserved".into(),committed_asset_id:None,committed_bytes:None,disposition_json:None,disposition_sha256:None});
  }
  let mut conn=bundle.db.acquire().await.map_err(|_|fail("Cold captured SQL unavailable"))?;
  use sqlx::Row;
  let rows=sqlx::query("SELECT operation_id,request_sha256,admission_claim_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,disposition_json,disposition_sha256 FROM attachment_capacity_reservations ORDER BY operation_id").fetch_all(&mut *conn).await.map_err(|_|fail("Cold captured full reservation inventory unavailable"))?;
  let all=rows.iter().map(|r|->Result<CapturedReservation,sqlx::Error>{Ok(CapturedReservation{operation_id:r.try_get(0)?,request_sha256:r.try_get(1)?,admission_claim_sha256:r.try_get(2)?,maximum_bytes:r.try_get(3)?,state:r.try_get(4)?,committed_asset_id:r.try_get(5)?,committed_bytes:r.try_get(6)?,disposition_json:r.try_get(7)?,disposition_sha256:r.try_get(8)?})}).collect::<Result<Vec<_>,_>>().map_err(|_|fail("Invalid cold captured reservation shape"))?;
  for expected in &reservations {if !all.contains(expected){return Err(fail("Cold classified full reservation differs"));}}
  let source=Self{scopes,reservations:all,leases:Vec::new(),cold_bundle:Some(bundle.clone()),capsule_fences:Vec::new(),completed:None};
  super::linux::admission_source_sql(&mut conn,&source).await?;
  let inventory=bundle.manifest.files.0.keys().cloned().collect::<std::collections::BTreeSet<_>>();
  let read=|path:&str|->io::Result<Vec<u8>>{let bytes=bundle.manifest.files.0.get(path).ok_or_else(||io::Error::new(io::ErrorKind::NotFound,"Captured file absent"))?;if bytes.size_bytes>6_000_000{return Err(fail("Captured canonical file exceeds bound"));}let mut f=bundle.open_owned(path)?;let mut raw=Vec::new();std::io::Read::read_to_end(&mut f,&mut raw)?;if raw.len() as u64!=bytes.size_bytes||digest(&raw)!=bytes.sha256{return Err(fail("Captured canonical bytes changed"));}Ok(raw)};
  for proof in &replay_proofs {proof.validate_references(&mut conn,&inventory,&read).await.map_err(|_|fail("Cold Replay protected references or original canonical SQL/bytes differ"))?;}
  for scope in &source.scopes.scopes {let winners:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachment_upload_receipts WHERE attempt_id=$1").bind(&scope.attempt_id).fetch_one(&mut *conn).await.map_err(|_|fail("Cannot inspect cold original winning receipts"))?;if winners!=0{return Err(fail("Cold reserved partial stage already has winning receipt"));}}
  source.fence()?;Ok(source)
 }
}

// Portable transport shape only; finalized source authority remains Linux-private.
#[derive(Debug,Clone,serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PriorRestoredCaptureEvidence {
 pub version:u32,pub generation_id:String,pub ledger_sha256:String,
 pub source_root:Identity,pub scopes:Vec<AdmissionCaptureScope>,
}
impl PriorRestoredCaptureEvidence {
 /// Shape validation only; source authority requires authenticated finalized history.
 pub(crate) fn validate_shape(&self)->io::Result<()> {
  if self.version!=3||!valid_id(&self.generation_id)||!valid_hash(&self.ledger_sha256)||self.source_root.inode==0||self.scopes.is_empty(){return Err(fail("Invalid prior capture transport shape"));}
  let scopes=self.scopes.iter().map(|s|AdmissionOwnerScope{namespace:s.namespace.clone(),attempt_id:s.attempt_id.clone(),original_claim_json:s.original_claim_json.clone(),admission_claim_sha256:s.admission_claim_sha256.clone(),request_sha256:s.request_sha256.clone(),maximum_bytes:s.maximum_bytes,reservation:s.reservation.clone(),directories:s.directories.iter().map(|b|DirectoryBinding{role:b.role.clone(),original:b.original.clone(),captured:b.captured.clone(),adopted:b.captured.clone()}).collect(),files:s.files.iter().map(|b|FileBinding{role:b.role.clone(),original:b.original.clone(),captured:b.captured.clone(),adopted:b.captured.clone(),length:b.length,sha256:b.sha256.clone()}).collect()}).collect();
  AdmissionOwnerEnvelope{version:2,scopes}.validate(&self.source_root)?;
  if self.scopes.iter().any(|s|!matches!(s.reservation,ReservationState::Reserved)){return Err(fail("Prior transport supports reserved scopes only"));}Ok(())
 }
}
