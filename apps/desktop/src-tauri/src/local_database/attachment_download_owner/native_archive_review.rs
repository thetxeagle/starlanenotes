//! Unregistered current-vault review adapter. No export/serve/retry authority.
use super::*;
use std::path::PathBuf;
struct Caller {selection:vault::SnapshotRequestBinding,destination:custody::Destination,original:Value,parent:filesystem::Directory,canonical:filesystem::Directory}
impl Caller {
 fn capture(selection:vault::SnapshotRequestBinding,provider:&str,original:Value,parent_path:PathBuf,current:&Value)->Result<Self,String>{
  if original.as_object().map(|o|o.len())!=Some(3)||original["context"]!=*current||current.as_object().map(|o|o.len())!=Some(7)||current["version"]!=1||current["storageMode"]!="local"||current["provider"]!=provider||!matches!(provider,"none"|"icloud-drive"|"google-drive")||!["serverBase","accountId","sessionId","cacheScope"].iter().all(|k|current[*k].as_str().is_some_and(|s|!s.is_empty()))||original["revision"].as_str().is_none_or(|s|s.is_empty()||s.len()>256)||serde_json::to_vec(&original).map_err(|e|e.to_string())?.len()>65536{return Err("Original local archive review context invalid".into())}
  let destination=custody::Destination::capture(&selection)?;
  if original["root"].as_str()!=destination.root_path.to_str(){return Err("Archive review belongs to its original selected vault".into())}
  let parent=filesystem::Directory::capture(&parent_path).map_err(|e|e.to_string())?;
  if parent.path.starts_with(&destination.root_path){return Err("Archive review destination cannot be inside selected vault".into())}
  let canonical=filesystem::Directory::capture(&destination.root_path).map_err(|e|e.to_string())?.child(".starlane",false).map_err(|e|e.to_string())?;
  let caller=Self{selection,destination,original,parent,canonical};caller.fence(current)?;Ok(caller)
 }
 fn fence(&self,current:&Value)->Result<(),String>{
  if self.original["context"]!=*current{return Err("Original archive review session changed".into())}
  self.selection.verify_selection()?;self.canonical.fence().map_err(|e|e.to_string())?;self.parent.fence().map_err(|e|e.to_string())?;
  if custody::Destination::capture(&self.selection)?!=self.destination{return Err("Original archive review physical vault changed".into())}Ok(())
 }
 async fn list(self,current:Value)->Result<Value,String>{
  self.fence(&current)?;connection::supervise(async move{let _guard=snapshot_commit_lock().lock().await;tokio::task::spawn_blocking(move||{self.fence(&current)?;let rows=native_archive_output::pending(&self.parent.path,&self.original)?;self.fence(&current)?;Ok(serde_json::json!({"format":1,"binding":self.original,"parent_path":self.parent.path,"inventory":rows}))}).await.map_err(|e|e.to_string())?}).await
 }
 async fn review(self,current:Value,operation:String,original_report:Value)->Result<Value,String>{
  self.fence(&current)?;if serde_json::to_vec(&original_report).map_err(|e|e.to_string())?.len()>65536{return Err("Archive review report exceeds bound".into())}
  connection::supervise(async move{let _guard=snapshot_commit_lock().lock().await;tokio::task::spawn_blocking(move||{
   self.fence(&current)?;let result=native_archive_output::recovery::acknowledge_review_guarded(&self.parent.path,&operation,&self.original,&original_report,||self.fence(&current))?;
   self.fence(&current).map_err(|e|format!("Archive review acknowledgment unknown; retained original evidence: {e}"))?;
   Ok(serde_json::json!({"format":1,"binding":self.original,"parent_path":self.parent.path,"operation":operation,"result":result,"retry_authority":false,"cleanup_authority":false,"serve_authority":false}))
  }).await.map_err(|e|e.to_string())?}).await
 }
}
fn caller<R:tauri::Runtime>(app:&AppHandle<R>,provider:&str,original_binding:Value,parent_path:String,current:&Value)->Result<Caller,String>{Caller::capture(vault::capture_snapshot_request(app,provider)?,provider,original_binding,parent_path.into(),current)}
#[tauri::command]
pub(crate) async fn list_native_vault_archive_export_outcomes<R:tauri::Runtime>(app:AppHandle<R>,provider:String,original_binding:Value,parent_path:String,current:Value)->Result<Value,String>{caller(&app,&provider,original_binding,parent_path,&current)?.list(current).await}
#[tauri::command]
pub(crate) async fn acknowledge_native_vault_archive_export_review<R:tauri::Runtime>(app:AppHandle<R>,provider:String,original_binding:Value,parent_path:String,current:Value,operation:String,original_report:Value)->Result<Value,String>{caller(&app,&provider,original_binding,parent_path,&current)?.review(current,operation,original_report).await}
#[cfg(test)] mod tests;
