//! Whole canonical-vault ownership only. This token has no preview/serving API.
use super::*;
pub(in super::super) struct CanonicalEvidence{reader:Reader,pub(in super::super) provenance:Value}
impl CanonicalEvidence{pub(in super::super) fn fence(&self)->Result<(),String>{self.reader.fence()}}
/// Called only inside the current archive owner's held VFS transaction/revision.
/// Exact current canonical row plus immutable joint membership selects custody;
/// filename, equal bytes and link count never select this capability.
pub(in super::super) fn try_capture(selection:vault::SnapshotRequestBinding,provider:&str,original:Value,snapshot:&Value,instance:&str,rows:&[(String,String,String,String,String,String,String)],asset:&Value)->Result<Option<CanonicalEvidence>,String>{
 context(&original,provider)?;
 if snapshot["assets"].as_array().ok_or("Current canonical assets missing")?.iter().filter(|candidate|*candidate==asset).count()!=1{return Err("Exact current canonical asset membership differs".into())}
 let mut selected=None;
 for(operation,fingerprint,row_provider,row_instance,intent,receipt,digest)in rows{
  if receipt.len()>128*1024*1024||intent.len()>128*1024*1024{return Err("Canonical archive receipt exceeds bound".into())}
  let result:Value=serde_json::from_str(receipt).map_err(err)?;let Some(assets)=result["candidate"]["assets"].as_array() else{continue};
  let indexes:Vec<_>=assets.iter().enumerate().filter(|(_,candidate)|*candidate==asset).map(|(index,_)|index).collect();
  if indexes.is_empty(){continue}if indexes.len()!=1||selected.is_some(){return Err("Canonical asset has ambiguous original joint membership".into())}
  let index=indexes[0];let asset_id=asset["id"].as_str().filter(|id|!id.is_empty()&&id.len()<=65536).ok_or("Canonical imported asset identity missing")?;
  if !safe(operation)||fingerprint.len()!=64||index>=4096{return Err("Canonical imported operation malformed".into())}
  let path=selection.expected_database_path()?;let root=Directory::capture(path.parent().and_then(Path::parent).ok_or("Canonical source root missing")?).map_err(err)?;let database=root.child(".starlane",false).map_err(err)?;let main=path.file_name().and_then(|s|s.to_str()).ok_or("Canonical main database missing")?.to_string();let main_id=database.file_identity(&main).map_err(err)?.ok_or("Canonical main database unavailable")?;
  let owner=PhysicalOwner{selection:selection.clone(),original:original.clone(),root,database,main,main_id,operation:operation.clone(),request_fingerprint:fingerprint.clone(),index,asset_id:asset_id.into()};owner.fence(&original)?;
  let(result,binding)=validate_receipt_for(provider,owner.database.path.join(&owner.main),operation,fingerprint,instance,(row_provider.clone(),row_instance.clone(),intent.clone(),receipt.clone(),digest.clone()))?;
  if result["candidate"]["assets"][index]!=*asset{return Err("Original canonical imported member changed".into())}
  let reader=Reader::open_physical(owner,result["native_installation"].clone(),binding,result["canonical_revision"].clone(),asset.clone())?;
  selected=Some(CanonicalEvidence{reader,provenance:json!({"format":1,"operation_id":operation,"request_fingerprint":fingerprint,"request_json":intent,"receipt_json":receipt,"receipt_digest":digest})});
 }
 Ok(selected)
}
