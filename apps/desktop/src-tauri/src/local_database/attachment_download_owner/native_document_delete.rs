//! Typed canonical original-Trash deletion, separate from archive/export authority.
use super::*;
use serde::{Deserialize,Serialize};
#[derive(Clone,Deserialize,Serialize)]#[serde(deny_unknown_fields)]
pub(super) struct Target { pub created_at:String,pub deleted_at:String }
#[derive(Clone,Deserialize,Serialize)]#[serde(deny_unknown_fields)]
pub(super) struct Request { pub operation_id:String,pub queued_at:String,pub space_id:String,pub document_id:String,pub original_target:Target,pub original_binding:Value }
#[derive(Deserialize)]#[serde(deny_unknown_fields)]
struct PhysicalSelection {root:filesystem::Identity,database_directory:filesystem::Identity,database:filesystem::Identity}
impl Request {
 pub(super) fn validate(&self)->Result<(),String>{
  let selected:PhysicalSelection=serde_json::from_value(self.original_binding["custody"].clone()).map_err(|_|"Original displayed physical Trash selection required")?;let _=(selected.root,selected.database_directory,selected.database);
  if !(self.operation_id.len()==36&&self.operation_id.bytes().enumerate().all(|(i,b)|if [8,13,18,23].contains(&i){b==b'-'}else{b.is_ascii_digit()||(b'a'..=b'f').contains(&b)}))||![self.queued_at.as_str(),self.space_id.as_str(),self.document_id.as_str(),self.original_target.created_at.as_str(),self.original_target.deleted_at.as_str()].iter().all(|v|!v.trim().is_empty()&&v.len()<=4096)||self.space_id.contains('/')||self.document_id.contains('/')||self.original_binding.as_object().map(|o|o.len())!=Some(4)||serde_json::to_vec(self).map_err(|e|e.to_string())?.len()>32768{return Err("Exact original Trash operation and canonical binding required".into())}Ok(())
 }
 pub(super) fn mutate(&self,snapshot:&mut Value)->Result<(),String>{
  self.validate()?;
  let rows=snapshot["documents"].as_array().ok_or("Canonical documents missing")?;
  let page=rows.iter().find(|p|p["id"]==self.document_id).ok_or("Original Trash page absent; only an exact retained receipt can reconcile")?;
  if page["space_id"]!=self.space_id||page["created_at"]!=self.original_target.created_at||page["deleted_at"]!=self.original_target.deleted_at{return Err("Original page incarnation or Trash cycle differs".into())}
  if rows.iter().any(|p|p["parent_id"]==self.document_id){return Err("Original Trash page has children; review them before permanent deletion".into())}
  for collection in ["documents","drafts","doc_tags","property_values","document_workflows","document_workflow_events","search_index"]{
   if let Some(rows)=snapshot.get_mut(collection).and_then(Value::as_array_mut){rows.retain(|row|if collection=="documents"{row["id"]!=self.document_id}else{row["document_id"]!=self.document_id});}
  }
  let queued=json!({"id":self.operation_id,"entity_type":"document","entity_id":self.document_id,"action":"delete","queued_at":self.queued_at,"attempts":0,"last_error":"Original server session is not bound by this local-only deletion; retain for explicit review before server replay","payload":{"id":self.document_id,"space_id":self.space_id,"original_target":{"created_at":self.original_target.created_at,"deleted_at":self.original_target.deleted_at}}});
  if snapshot.get("sync_state").is_none(){snapshot["sync_state"]=json!({"state":"idle","updated_at":self.queued_at,"queued_operations":[]});}
  let rows=snapshot["sync_state"]["queued_operations"].as_array_mut().ok_or("Canonical replay queue missing")?;
  if rows.len()>=65536||rows.iter().any(|r|r["id"]==self.operation_id){return Err("Original deletion queue collision/limit; retain for review".into())}rows.push(queued);Ok(())
 }
}
#[tauri::command]
pub(crate) async fn delete_original_native_trash_document<R:tauri::Runtime>(app:AppHandle<R>,provider:String,context:Value,current:Value,request:Value)->Result<Value,String>{
 let request:Request=serde_json::from_value(request).map_err(|_|"Original native deletion request malformed")?;request.validate()?;
 if context["accountId"]!="local-user"||request.original_binding["context"]!=context{return Err("Actual original local session required".into())}
 let owner=native_archive_capture::Owner::capture(vault::capture_snapshot_request(&app,&provider)?,&provider,request.original_binding["revision"].as_str().ok_or("Original canonical revision missing")?,context)?;
 if owner.delete_binding()?!=request.original_binding{return Err("Original native deletion source differs".into())}owner.delete_original_trash(current,request).await
}
#[cfg(test)]mod tests;
