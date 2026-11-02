//! Portable source history only; never a fresh root, producer or serving grant.
use super::*;
#[derive(Deserialize,Serialize)]#[serde(deny_unknown_fields)]
struct Request{operation_id:String,name:String,mime_type:String,byte_length:u64,space_id:String,document_id:String,draft_id:String,draft_created_at:String,original_binding:Value,#[serde(default,skip_serializing_if="Option::is_none")]path_source:Option<PathSource>}
#[derive(Deserialize,Serialize)]#[serde(deny_unknown_fields)]
struct PathStamp{device:u64,inode:u64,uid:u32,mode:u32,links:u64,length:u64,ctime:i64,ctime_nsec:i64,mtime:i64,mtime_nsec:i64}
#[derive(Deserialize,Serialize)]#[serde(deny_unknown_fields)]
struct PathSource{schema_version:u32,source_path:String,stamp:PathStamp,binary_sha256:String}
fn uuid(id:&str)->bool{id.len()==36&&id.bytes().enumerate().all(|(i,b)|if [8,13,18,23].contains(&i){b==b'-'}else{b.is_ascii_digit()||(b'a'..=b'f').contains(&b)})}
pub(super) fn validate(raw:&str)->Result<BTreeMap<String,Value>,String>{
 let records:Vec<Value>=serde_json::from_str(raw).map_err(|_|"Portable general import array malformed")?;if records.len()>1024{return Err("Portable general import receipt count exceeds bound".into())}
 let mut result=BTreeMap::new();let mut operations=BTreeSet::new();
 for record in records{
  let operation=record["operation_id"].as_str().filter(|s|uuid(s)).ok_or("Portable general import operation missing")?;
  let raw=record["receipt_json"].as_str().filter(|s|s.len()<=65536).ok_or("Portable general import receipt unavailable")?;let hash=sha(raw.as_bytes());
  if record.as_object().map(|o|o.len())!=Some(5)||record["format"]!=1||record["kind"]!="original_local_import"||record["receipt_sha256"]!=hash||!operations.insert(operation.to_string()){return Err("Portable general import identity/hash differs".into())}
  let receipt:Value=serde_json::from_str(raw).map_err(|_|"Portable general import receipt malformed")?;
  let request:Request=serde_json::from_value(receipt["request"].clone()).map_err(|_|"Portable general import request shape differs")?;
  if !uuid(&request.operation_id)||request.name.trim().is_empty()||request.name.len()>255||request.mime_type.is_empty()||request.mime_type.len()>255||request.mime_type.bytes().any(|b|b.is_ascii_control())||request.byte_length>MAX_BINARY||request.draft_created_at.is_empty()||request.draft_created_at.len()>256||[&request.space_id,&request.document_id,&request.draft_id].iter().any(|s|s.is_empty()||s.len()>256||s.chars().any(|c|c=='/'||c==char::from(92)||c==char::from(0))){return Err("Portable general import request bounds differ".into())}
  if let Some(source)=&request.path_source{if source.schema_version!=1||!source.source_path.starts_with('/')||source.source_path.len()>4096||source.source_path.contains(char::from(0))||source.stamp.length!=request.byte_length||!valid_sha(&source.binary_sha256){return Err("Portable original path proof differs".into())}}
  let request_sha=sha(&serde_json::to_vec(&request).map_err(|e|e.to_string())?);let creator=&receipt["creator"];let asset=&receipt["asset"];
  let seal:Value=serde_json::from_str(receipt["seal_json"].as_str().filter(|s|s.len()<=65536).ok_or("Portable general import seal missing")?).map_err(|_|"Portable general import seal malformed")?;
  if let Some(source)=&request.path_source{if seal["binary_sha256"]!=source.binary_sha256{return Err("Portable original path bytes differ".into())}}
  let path=asset["vault_path"].as_str().filter(|s|s.len()<=65536).ok_or("Portable general import path missing")?;
  let expected_url=format!("asset://localhost/{}",urlencoding::encode(path));
  let mut payload=asset.clone();let object=payload.as_object_mut().ok_or("Portable general asset must be an object")?;object.insert("draft_id".into(),json!(request.draft_id));object.insert("local_draft_source".into(),json!({"schema_version":1,"space_id":request.space_id,"document_id":request.document_id,"draft_id":request.draft_id,"draft_created_at":request.draft_created_at}));
  let queue=json!({"id":format!("syncop-import-{operation}"),"entity_type":"asset","entity_id":asset["id"],"action":"create","queued_at":creator["created_at"],"attempts":0,"payload":payload});
  if receipt.as_object().map(|o|o.len())!=Some(12)||receipt["format"]!=1||receipt["operation_id"]!=operation||request.operation_id!=operation||receipt["canonical_committed"]!=true||!receipt["canonical_revision"].as_str().is_some_and(|s|!s.is_empty()&&s.len()<=256)||receipt["request_sha256"]!=request_sha||creator["request_sha256"]!=request_sha||creator["format"]!=1||asset["id"]!=format!("asset-import-{operation}")||asset["source"]!="tauri"||asset["size"]!=request.byte_length||asset["name"]!=request.name||asset["mime_type"]!=request.mime_type||asset["space_id"]!=request.space_id||asset["document_id"]!=request.document_id||asset["created_at"]!=creator["created_at"]||asset["url"]!=expected_url||!asset["server_version_provenance"].is_null()||!asset["native_local_version_provenance"].is_null()||seal["format"]!=1||seal["operation_id"]!=operation||seal["request_sha256"]!=request_sha||seal["asset"]!=*asset||seal["payload"]["length"]!=request.byte_length||seal["payload"]["links"]!=2||!seal["binary_sha256"].as_str().is_some_and(valid_sha)||receipt["queued_operation"]!=queue{return Err("Portable general import source/asset/queue binding differs".into())}
  if result.insert(hash,receipt).is_some(){return Err("Duplicate portable general import receipt".into())}
 }
 Ok(result)
}

/// Inert historical namespace/reference classification only. No physical grant.
pub(super) fn retained_ids(portable:&Value)->Result<BTreeSet<String>,String>{
 fn walk(value:&Value,depth:usize,budget:&mut usize,ids:&mut BTreeMap<String,String>)->Result<(),String>{
  if value.is_null(){return Ok(())}if depth>8||value["format"]!=1||value["authority"]!="portable_provenance_only"{return Err("Retained general import history shape/depth differs".into())}
  if let Some(raw)=value["local_imports_json"].as_str(){if raw.len()>*budget{return Err("Retained general import history exceeds bound".into())}*budget-=raw.len();for(hash,receipt)in validate(raw)?{let operation=receipt["operation_id"].as_str().ok_or("Retained original import ID missing")?.to_owned();if ids.get(&operation).is_some_and(|old|old!=&hash){return Err("Conflicting retained original import identity".into())}ids.insert(operation,hash);if ids.len()>1024{return Err("Retained original import identities exceed bound".into())}}}
  if let Some(raw)=value["joint_imports_json"].as_str(){if raw.len()>*budget{return Err("Retained general joint history exceeds bound".into())}*budget-=raw.len();let rows:Vec<Value>=serde_json::from_str(raw).map_err(|_|"Retained joint history malformed")?;if rows.len()>4096{return Err("Retained joint history count exceeds bound".into())}for row in rows{let raw=row["receipt_json"].as_str().filter(|s|s.len()<=64*1024*1024).ok_or("Retained general joint receipt unavailable")?;if raw.len()>*budget{return Err("Retained nested general history exceeds bound".into())}*budget-=raw.len();let receipt:Value=serde_json::from_str(raw).map_err(|_|"Retained nested general receipt malformed")?;walk(&receipt["imported_provenance"],depth+1,budget,ids)?;}}
  Ok(())
 }
 let mut ids=BTreeMap::new();walk(portable,0,&mut(128*1024*1024),&mut ids)?;Ok(ids.into_keys().collect())
}
