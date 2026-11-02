//! Untrusted portable evidence admission. Hash consistency proves no origin or
//! serving authority. Only joint import certifies new destination installation.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Seek};
const MANIFEST: &str = "starlane-native/manifest.json";
const DISPOSITIONS:&str="starlane-native/local-dispositions.json";
mod dispositions;
const GENERAL: &str = "starlane-native/local-imports.json";
mod local_imports;
const LOCAL: &str = "starlane-native/local-covers.json";
const JOINT: &str = "starlane-native/joint-imports.json";
const RECEIPTS: &str = "starlane-native/raw-receipts.json";
const MAX_BINARY: u64 = 100 * 1024 * 1024;
fn sha(b: &[u8]) -> String { format!("{:x}", Sha256::digest(b)) }
fn valid_sha(s: &str) -> bool { s.len()==64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest { format: u32, canonical_revision: String, snapshot_sha256: String, raw_receipts_sha256: String, binaries: Vec<Binary>, #[serde(default)] joint_imports_sha256:Option<String>, #[serde(default)] local_covers_sha256:Option<String>, #[serde(default)] local_dispositions_sha256:Option<String>, #[serde(default)] local_imports_sha256:Option<String> }
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Binary { entry: String, asset: Value, byte_length: u64, sha256: String, native_intent_sha256: Option<String>, #[serde(default)] kind:Option<String>, #[serde(default)] joint_receipt_sha256:Option<String>, #[serde(default)] local_receipt_sha256:Option<String>, #[serde(default)] import_receipt_sha256:Option<String> }
fn bounded<R:Read+Seek>(zip:&mut zip::ZipArchive<R>,name:&str,max:u64)->Result<String,String>{
 let mut entry=zip.by_name(name).map_err(|_|format!("Archive evidence member missing: {name}"))?;
 if entry.size()>max{return Err("Archive evidence exceeds metadata budget".into())}
 let mut text=String::new();entry.by_ref().take(max+1).read_to_string(&mut text).map_err(|e|e.to_string())?;
 if text.len() as u64>max{return Err("Archive evidence exceeds metadata budget".into())}Ok(text)
}
pub(super) fn inspect<R:Read+Seek>(zip:&mut zip::ZipArchive<R>,snapshot_raw:&str)->Result<Option<Value>,String>{
 if zip.len()>4103{return Err("Archive member budget exceeded".into())}
 let mut names=BTreeSet::new();let mut has_sidecar=false;
 for i in 0..zip.len(){let entry=zip.by_index(i).map_err(|e|e.to_string())?;let name=entry.name().to_string();
  if !names.insert(name.clone()){return Err("Duplicate archive member".into())}
  if name.starts_with("starlane-native/"){has_sidecar=true;if name!=MANIFEST&&name!=RECEIPTS&&name!=JOINT&&name!=LOCAL&&name!=DISPOSITIONS&&name!=GENERAL{return Err("Unsupported native archive evidence member".into())}}
 }
 let snapshot:Value=serde_json::from_str(snapshot_raw).map_err(|e|e.to_string())?;
 let assets=snapshot["assets"].as_array().ok_or("Archive assets missing")?;
 if assets.len()>4096{return Err("Archive asset budget exceeded".into())}
 if !has_sidecar{
  if assets.iter().any(|a|a["id"].as_str().is_some_and(|s|s.starts_with("native-download-")||s.starts_with("native-cover-")||s.starts_with("asset-import-"))){return Err("Receipt-backed archive requires complete original provenance sidecars".into())}return Ok(None)
 }
 let manifest_raw=bounded(zip,MANIFEST,16*1024*1024)?;let receipts_raw=bounded(zip,RECEIPTS,32*1024*1024)?;
 let manifest:Manifest=serde_json::from_str(&manifest_raw).map_err(|_|"Native archive manifest malformed")?;
 if !matches!(manifest.format,1|2|3|4|5)||manifest.canonical_revision.is_empty()||manifest.canonical_revision.len()>256||manifest.snapshot_sha256!=sha(snapshot_raw.as_bytes())||manifest.raw_receipts_sha256!=sha(receipts_raw.as_bytes())||manifest.binaries.len()!=assets.len(){return Err("Native archive manifest snapshot/receipt binding differs".into())}
 let joint_raw=if matches!(manifest.format,2|3|4|5){let raw=bounded(zip,JOINT,64*1024*1024)?;if manifest.joint_imports_sha256.as_deref()!=Some(sha(raw.as_bytes()).as_str()){return Err("Portable joint receipt sidecar digest differs".into())}Some(raw)}else{if manifest.joint_imports_sha256.is_some()||names.contains(JOINT){return Err("Format1 cannot claim joint import provenance".into())}None};
 let joints=joint_raw.as_deref().filter(|raw|!(matches!(manifest.format,3|4|5)&&*raw=="[]")).map(|raw|validate_joints(raw,matches!(manifest.format,4|5))).transpose()?.unwrap_or_default();
 let local_raw=if matches!(manifest.format,3|4|5){let raw=bounded(zip,LOCAL,32*1024*1024)?;if manifest.local_covers_sha256.as_deref()!=Some(sha(raw.as_bytes()).as_str()){return Err("Portable local cover sidecar digest differs".into())}Some(raw)}else{if manifest.local_covers_sha256.is_some()||names.contains(LOCAL){return Err("Older archive format cannot claim local cover provenance".into())}None};
 let locals=local_raw.as_deref().map(validate_local_covers).transpose()?.unwrap_or_default();
 let disposition_raw=if matches!(manifest.format,4|5){let raw=bounded(zip,DISPOSITIONS,32*1024*1024)?;if manifest.local_dispositions_sha256.as_deref()!=Some(sha(raw.as_bytes()).as_str()){return Err("Portable original disposition sidecar digest differs".into())}Some(raw)}else{if manifest.local_dispositions_sha256.is_some()||names.contains(DISPOSITIONS){return Err("Older archive cannot claim original disposition provenance".into())}None};
 let disposed=disposition_raw.as_deref().map(dispositions::validate).transpose()?.unwrap_or_default();
 for op in &disposed{let pin=format!("starlane-local-attachment://{op}");if dispositions::protects(&snapshot,&pin)||assets.iter().any(|a|a["id"]==format!("native-cover-{op}")||a["native_local_version_provenance"]["producer_operation_id"]==*op){return Err("Portable discarded original cannot claim a live reference or binary".into())}}
 let general_raw=if manifest.format==5{let raw=bounded(zip,GENERAL,32*1024*1024)?;if manifest.local_imports_sha256.as_deref()!=Some(sha(raw.as_bytes()).as_str()){return Err("Portable general import sidecar digest differs".into())}Some(raw)}else{if manifest.local_imports_sha256.is_some()||names.contains(GENERAL){return Err("Older archive cannot claim general import provenance".into())}None};
 let generals=general_raw.as_deref().map(local_imports::validate).transpose()?.unwrap_or_default();let mut used_generals=BTreeSet::new();
 let receipts:Vec<String>=serde_json::from_str(&receipts_raw).map_err(|_|"Native archive receipts must retain exact original strings")?;
 if receipts.len()>4096{return Err("Native archive receipt budget exceeded".into())}
 let mut by_id=BTreeMap::new();
 for raw in &receipts{
  if raw.len()>65536{return Err("Native archive receipt exceeds budget".into())}
  let receipt:Value=serde_json::from_str(raw).map_err(|_|"Native archive original receipt malformed")?;
  let op=receipt["operation_id"].as_str().filter(|s|!s.is_empty()).ok_or("Archive receipt operation missing")?;
  let asset=&receipt["asset"];let p=&asset["server_version_provenance"];
  let version_raw=p["raw_version_record"].as_str().filter(|s|s.len()<=65536).ok_or("Archive receipt original version missing")?;
  let version:Value=serde_json::from_str(version_raw).map_err(|_|"Archive receipt original version malformed")?;
  if receipt["format"]!=1||receipt["canonical_committed"]!=true||receipt["canonical_revision"].as_str().filter(|s|!s.is_empty()).is_none()||asset["id"]!=format!("native-download-{op}")||asset["source"]!="tauri"||p["format"]!=1||!version.is_object()||version["byte_length"]!=asset["size"]||version["binary_sha256"]!=p["bytes_sha256"]||!p["bytes_sha256"].as_str().is_some_and(valid_sha){return Err("Archive original receipt asset/version binding invalid".into())}
  if by_id.insert(asset["id"].as_str().unwrap().to_string(),receipt).is_some(){return Err("Duplicate original archive receipt operation".into())}
 }
 let mut expected=BTreeSet::from(["vault.json".to_string(),MANIFEST.to_string(),RECEIPTS.to_string()]);if matches!(manifest.format,2|3|4|5){expected.insert(JOINT.into());}if matches!(manifest.format,3|4|5){expected.insert(LOCAL.into());}if matches!(manifest.format,4|5){expected.insert(DISPOSITIONS.into());}if manifest.format==5{expected.insert(GENERAL.into());}let mut ids=BTreeSet::new();
 for (asset,binary) in assets.iter().zip(&manifest.binaries){
  let id=asset["id"].as_str().filter(|s|!s.is_empty()).ok_or("Archive asset identity missing")?;
  let typed:super::VaultAsset=serde_json::from_value(asset.clone()).map_err(|e|e.to_string())?;
  if !ids.insert(id)||asset["source"]!="tauri"||binary.asset!=*asset||binary.entry!=super::archive_asset_path(&typed)||binary.entry.contains("..")||binary.entry.contains('\\')||binary.byte_length>MAX_BINARY||asset["size"].as_u64()!=Some(binary.byte_length)||!valid_sha(&binary.sha256)||!expected.insert(binary.entry.clone()){return Err("Archive binary manifest binding invalid".into())}
  if manifest.format==1 && (binary.kind.is_some()||binary.joint_receipt_sha256.is_some()||binary.local_receipt_sha256.is_some()||binary.import_receipt_sha256.is_some()){return Err("Format1 binary cannot claim imported ownership".into())}
  if binary.kind.as_deref()==Some("original_local_import"){
   if manifest.format!=5||binary.native_intent_sha256.is_some()||binary.joint_receipt_sha256.is_some()||binary.local_receipt_sha256.is_some()||!id.starts_with("asset-import-"){return Err("Portable general import kind differs".into())}
   let hash=binary.import_receipt_sha256.as_deref().filter(|s|valid_sha(s)).ok_or("Portable general import receipt hash missing")?;
   let receipt=generals.get(hash).ok_or("Portable general import receipt missing")?;let seal:Value=serde_json::from_str(receipt["seal_json"].as_str().ok_or("Portable import seal missing")?).map_err(|_|"Portable import seal malformed")?;
   if receipt["asset"]!=*asset||seal["binary_sha256"]!=binary.sha256||!used_generals.insert(hash.to_string()){return Err("Portable general import exact asset/bytes differ".into())}
  }else if binary.import_receipt_sha256.is_some(){return Err("Another binary role cannot claim a general import receipt".into())}
  else if binary.kind.as_deref()==Some("original_local_cover"){
   if !matches!(manifest.format,3|4|5)||binary.native_intent_sha256.is_some()||binary.joint_receipt_sha256.is_some()||!id.starts_with("native-cover-")||asset["native_local_version_provenance"]["binary_sha256"]!=binary.sha256{return Err("Portable local cover provenance kind differs".into())}
   let local_sha=binary.local_receipt_sha256.as_deref().filter(|s|valid_sha(s)).ok_or("Portable local cover receipt hash missing")?;
   if locals.get(local_sha).map(|r|&r["asset"])!=Some(asset){return Err("Portable local cover original asset/receipt differs".into())}
  }else if binary.kind.as_deref()==Some("joint_import"){
   if !matches!(manifest.format,2|3|4|5)||binary.native_intent_sha256.is_some()||binary.local_receipt_sha256.is_some()||!((id.starts_with("native-download-")&&asset["server_version_provenance"]["bytes_sha256"]==binary.sha256)||(matches!(manifest.format,3|4|5)&&id.starts_with("native-cover-")&&asset["native_local_version_provenance"]["binary_sha256"]==binary.sha256)||(!id.starts_with("native-download-")&&!id.starts_with("native-cover-")&&asset["server_version_provenance"].is_null()&&asset["native_local_version_provenance"].is_null())){return Err("Portable joint binary provenance kind differs".into())}
   let raw_sha=binary.joint_receipt_sha256.as_deref().filter(|s|valid_sha(s)).ok_or("Portable joint original receipt hash missing")?;
   let result=joints.get(raw_sha).ok_or("Portable joint original receipt missing")?;
   if !result["candidate"]["assets"].as_array().is_some_and(|a|a.iter().any(|a|a==asset)){return Err("Portable joint receipt current asset differs".into())}
   let creator_raw=result["native_installation"]["manifest_json"].as_str().ok_or("Portable joint creator evidence missing")?;let creator:Value=serde_json::from_str(creator_raw).map_err(|_|"Portable joint creator evidence malformed")?;
   if !creator["binaries"].as_array().is_some_and(|a|a.iter().any(|a|a["asset"]==*asset&&a["length"]==binary.byte_length&&a["sha256"]==binary.sha256)){return Err("Portable joint creator binary binding differs".into())}
  }else if id.starts_with("native-download-"){
   if matches!(manifest.format,2|3|4|5)&&binary.kind.as_deref()!=Some("download"){return Err("Portable native download provenance kind missing".into())}
   if binary.joint_receipt_sha256.is_some()||binary.local_receipt_sha256.is_some()||by_id.get(id).map(|r|&r["asset"])!=Some(asset)||asset["server_version_provenance"]["bytes_sha256"]!=binary.sha256||!binary.native_intent_sha256.as_deref().is_some_and(valid_sha){return Err("Archive current native asset original receipt missing or differs".into())}
  }else if id.starts_with("asset-import-")||id.starts_with("native-cover-")||!asset["native_local_version_provenance"].is_null()||binary.local_receipt_sha256.is_some()||binary.native_intent_sha256.is_some()||binary.joint_receipt_sha256.is_some()||(matches!(manifest.format,2|3|4|5)&&binary.kind.as_deref()!=Some("legacy")){return Err("Legacy asset cannot claim original native authority".into())}
  let mut entry=zip.by_name(&binary.entry).map_err(|_|"Archive exact binary member missing")?;
  if entry.size()!=binary.byte_length{return Err("Archive binary length differs".into())}
  let mut hash=Sha256::new();let mut count=0u64;let mut buffer=[0;65536];loop{let n=entry.read(&mut buffer).map_err(|e|e.to_string())?;if n==0{break}count+=n as u64;if count>binary.byte_length{return Err("Archive binary exceeds declared bound".into())}hash.update(&buffer[..n]);}
  if count!=binary.byte_length||format!("{:x}",hash.finalize())!=binary.sha256{return Err("Archive exact binary digest differs".into())}
 }
 if used_generals.len()!=generals.len(){return Err("Portable general import sidecar contains an unbound receipt family".into())}
 if names!=expected{return Err("Native complete archive has omitted or unrelated members".into())}
 // Strings remain byte-exact; never deserialize/reserialize receipt provenance.
 let mut portable=json!({"format":1,"authority":"portable_provenance_only","manifest_json":manifest_raw,"raw_receipts_json":receipts_raw,"joint_imports_json":joint_raw});
 if let Some(raw)=local_raw{portable["local_covers_json"]=json!(raw);}
 if let Some(raw)=disposition_raw{portable["local_dispositions_json"]=json!(raw);}
 if let Some(raw)=general_raw{portable["local_imports_json"]=json!(raw);}
 Ok(Some(portable))
}
#[derive(Deserialize)]#[serde(deny_unknown_fields)]
struct Joint{format:u32,operation_id:String,request_fingerprint:String,request_json:String,receipt_json:String,receipt_digest:String,#[serde(default)]kind:Option<String>}
fn semantic_sha(value:&Value)->Result<String,String>{Ok(sha(&serde_json::to_vec(value).map_err(|e|e.to_string())?))}
fn validate_joints(raw:&str,allow_legacy_metadata:bool)->Result<BTreeMap<String,Value>,String>{
 let joints:Vec<Joint>=serde_json::from_str(raw).map_err(|_|"Portable joint evidence schema malformed")?;if joints.is_empty()||joints.len()>4096{return Err("Portable joint evidence count exceeds bound".into())}let mut by_hash=BTreeMap::new();let mut operations=BTreeSet::new();
 for joint in joints{
  if joint.format!=1||joint.operation_id.is_empty()||joint.operation_id.len()>128||!operations.insert(joint.operation_id.clone())||!valid_sha(&joint.request_fingerprint)||!valid_sha(&joint.receipt_digest)||joint.request_json.len()>64*1024*1024||joint.receipt_json.len()>64*1024*1024{return Err("Portable joint original request/receipt identity invalid".into())}
  let request:Value=serde_json::from_str(&joint.request_json).map_err(|_|"Portable joint original request malformed")?;let result:Value=serde_json::from_str(&joint.receipt_json).map_err(|_|"Portable joint original receipt malformed")?;
  if semantic_sha(&request)?!=joint.request_fingerprint||semantic_sha(&result)?!=joint.receipt_digest||result["schema_version"]!=1||result["canonical_committed"]!=true||result["operation_id"]!=joint.operation_id||request["plan"]["operation_id"]!=joint.operation_id||result["request_fingerprint"]!=joint.request_fingerprint||result["binding"]!=request["plan"]["binding"]||result["candidate"]!=request["candidate"]||result["reference_changes"]!=request["reference_changes"]||result["versions"]!=request["plan"]["versions"]||result["imported_provenance"]!=request["plan"]["imported_provenance"]{return Err("Portable joint original request/receipt binding differs".into())}
  if let Some(kind)=&joint.kind{if !allow_legacy_metadata||kind!="legacy_canonical_metadata"||!result["native_installation"].is_null(){return Err("Portable canonical metadata role differs".into())}continue}
  let proof=&result["native_installation"];let creator_raw=proof["manifest_json"].as_str().filter(|s|s.len()<=16*1024*1024).ok_or("Portable joint creator manifest missing")?;let creator:Value=serde_json::from_str(creator_raw).map_err(|_|"Portable joint creator manifest malformed")?;
  if proof["format"]!=1||proof["manifest_sha256"]!=sha(creator_raw.as_bytes())||creator["format"]!=1||creator["operation"]!=joint.operation_id||creator["request_fingerprint"]!=joint.request_fingerprint||creator["binding"]!=result["binding"]||creator["canonical_revision"]!=result["canonical_revision"]{return Err("Portable joint creator evidence binding differs".into())}
  // These inode claims are untrusted provenance. Only the new destination's
  // actual creator/install protocol can authorize its imported bytes.
  let raw_hash=sha(joint.receipt_json.as_bytes());if by_hash.insert(raw_hash,result).is_some(){return Err("Duplicate portable joint receipt bytes".into())}
 }Ok(by_hash)
}

// Historical, untrusted source provenance only. The destination must create
// its own held installation and immutable joint SQL receipt before any read.
fn validate_local_covers(raw:&str)->Result<BTreeMap<String,Value>,String>{
 let locals:Vec<Value>=serde_json::from_str(raw).map_err(|_|"Portable local cover sidecar malformed")?;
 if locals.len()>1024{return Err("Portable local cover receipt budget exceeded".into())}
 let mut by_hash=BTreeMap::new();let mut ids=BTreeSet::new();
 for local in locals{
  let op=local["operation_id"].as_str().filter(|s|s.len()==36).ok_or("Portable local operation missing")?;
  let receipt_raw=local["receipt_json"].as_str().filter(|s|s.len()<=65536).ok_or("Portable local receipt exceeds bound")?;
  if local.as_object().map(|o|o.len())!=Some(5)||local["format"]!=1||local["kind"]!="original_local_cover"||local["receipt_sha256"]!=sha(receipt_raw.as_bytes())||!ids.insert(op.to_string()){return Err("Portable local receipt identity/hash differs".into())}
  let receipt:Value=serde_json::from_str(receipt_raw).map_err(|_|"Portable local receipt malformed")?;
  let asset=&receipt["asset"];let p=&asset["native_local_version_provenance"];let request=&receipt["request"];
  let seal_raw=receipt["seal_json"].as_str().filter(|s|s.len()<=65536).ok_or("Portable local seal missing")?;
  let seal:Value=serde_json::from_str(seal_raw).map_err(|_|"Portable local seal malformed")?;
  if receipt["format"]!=1||receipt["canonical_committed"]!=true||receipt["operation_id"]!=op||request["operation_id"]!=op||asset["id"]!=format!("native-cover-{op}")||asset["url"]!=format!("starlane-local-attachment://{op}")||asset["source"]!="tauri"||!asset["server_version_provenance"].is_null()||p["format"]!=1||p["version_id"]!=op||p["producer_operation_id"]!=op||p["byte_length"]!=asset["size"]||p["byte_length"]!=request["byte_length"]||p["original_filename"]!=asset["name"]||p["mime_type"]!=asset["mime_type"]||p["uploader"]!="local-user"||!p["binary_sha256"].as_str().is_some_and(valid_sha)||seal["format"]!=1||seal["operation_id"]!=op||seal["asset"]!=*asset||seal["binary_sha256"]!=p["binary_sha256"]||seal["payload"]["length"]!=asset["size"]{return Err("Portable original local cover binding differs".into())}
  if by_hash.insert(sha(receipt_raw.as_bytes()),receipt).is_some(){return Err("Duplicate portable local receipt".into())}
 }
 Ok(by_hash)
}

#[cfg(test)] mod tests;

// Inert identity reservation parser; callers supply actual current custody.
pub(crate) fn retained_original_disposition_ids(value:&Value)->Result<BTreeSet<String>,String>{dispositions::retained_ids(value)}

pub(crate) fn retained_history_requires_format4(value:&Value)->Result<bool,String>{dispositions::requires_format4(value)}

/// Historical local-import IDs only; no source or serving authority.
pub(crate) fn retained_original_local_import_ids(value:&Value)->Result<BTreeSet<String>,String>{local_imports::retained_ids(value)}
