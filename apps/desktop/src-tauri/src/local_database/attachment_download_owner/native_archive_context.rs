//! Current initialized local-vault descriptor and typed imported preview.
//! Local context is declared app provenance, never signed server authorization.
use super::*;
use filesystem::{Directory,Identity};
use serde::Deserialize;
use sqlx::Connection;
fn err(e:impl ToString)->String{e.to_string()}
#[derive(Clone,Deserialize)]#[serde(deny_unknown_fields)]
pub(crate) struct AssetSelection{pub space_id:String,pub document_id:String,pub asset_id:String}
#[derive(Clone)]
struct Current{selection:vault::SnapshotRequestBinding,provider:String,context:Value,root:Directory,database:Directory,main:String,main_id:Identity}
impl Current {
 fn capture(selection:vault::SnapshotRequestBinding,provider:&str,context:Value)->Result<Self,String>{
  if context.as_object().map(|o|o.len())!=Some(7)||context["version"]!=1||context["accountId"]!="local-user"||context["storageMode"]!="local"||context["provider"]!=provider||!matches!(provider,"none"|"icloud-drive"|"google-drive")||!["serverBase","sessionId","cacheScope"].iter().all(|k|context[*k].as_str().is_some_and(|s|!s.is_empty()&&s.len()<=4096))||serde_json::to_vec(&context).map_err(err)?.len()>65536{return Err("Actual local archive session provenance required".into())}
  let path=selection.expected_database_path()?;let root=Directory::capture(path.parent().and_then(Path::parent).ok_or("Original selected vault missing")?).map_err(err)?;let database=root.child(".starlane",false).map_err(err)?;let main=path.file_name().and_then(|s|s.to_str()).ok_or("Original canonical database missing")?.to_string();let main_id=database.file_identity(&main).map_err(err)?.ok_or("Initialized canonical archive database required")?;
  let owner=Self{selection,provider:provider.into(),context,root,database,main,main_id};owner.fence()?;Ok(owner)
 }
 fn fence(&self)->Result<(),String>{self.selection.verify_selection()?;self.root.fence().map_err(err)?;self.database.fence().map_err(err)?;if self.database.file_identity(&self.main).map_err(err)?!=Some(self.main_id){return Err("Original canonical archive context database changed".into())}Ok(())}
 async fn trash(self,current:Value,space:String)->Result<Value,String>{
  if current!=self.context||space.is_empty()||space.len()>4096||space.contains('/') {return Err("Original native Trash selection required".into())}self.fence()?;
  connection::supervise(async move{
   let _guard=snapshot_commit_lock().lock().await;self.fence()?;let mut db=connection::Database::prepare(self.database.clone(),&self.main).await?;let physical=db.fence_owner();
   let result=async{
    let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;
    native_archive_capture::strict_preflight(&mut tx).await?;let snapshot=read_snapshot_from_connection(&mut tx,&self.provider).await?.ok_or("Current canonical Trash source required")?;
    let revision=snapshot["canonical_revision"].as_str().filter(|v|!v.is_empty()&&v.len()<=256).ok_or("Original canonical Trash revision unavailable")?;
    let mut pages=Vec::new();for row in snapshot["documents"].as_array().ok_or("Canonical documents unavailable")? {
     if row["space_id"]==space && row["deleted_at"].as_str().is_some_and(|s|!s.is_empty()) {if !row["created_at"].as_str().is_some_and(|s|!s.is_empty()&&s.len()<=4096)||!row["deleted_at"].as_str().is_some_and(|s|s.len()<=4096){return Err("Exact original Trash witness unavailable".into())}pages.push(row.clone());}
    }
    let output=json!({"format":1,"binding":{"context":self.context,"root":self.root.path,"revision":revision,"custody":{"root":filesystem::identity(&self.root.held).map_err(err)?,"database_directory":filesystem::identity(&self.database.held).map_err(err)?,"database":self.main_id}},"space_id":space,"documents":pages});if serde_json::to_vec(&output).map_err(err)?.len()>16*1024*1024{return Err("Canonical Trash selection exceeds bound".into())}
    self.fence()?;physical.check()?;tx.rollback().await.map_err(err)?;Ok(output)
   }.await;db.retire().await.map_err(|e|format!("Trash source physical retirement failed: {e}"))?;result
  }).await
 }
 async fn run(self,current:Value,selected:Option<AssetSelection>,expected:Option<Value>,offset:u64,length:u64)->Result<Value,String>{
  if current!=self.context{return Err("Original local archive session changed".into())}self.fence()?;
  connection::supervise(async move{
   let _guard=snapshot_commit_lock().lock().await;self.fence()?;let mut db=connection::Database::prepare(self.database.clone(),&self.main).await?;let physical=db.fence_owner();
   let result=async{
    let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;
    let size:Option<i64>=sqlx::query_scalar("SELECT length(CAST(value AS BLOB)) FROM local_vault_metadata WHERE key='canonical_revision'").fetch_optional(&mut *tx).await.map_err(err)?;if !size.is_some_and(|n|(1..=256).contains(&n)){return Err("Initialized canonical revision required".into())}
    let revision:String=sqlx::query_scalar("SELECT value FROM local_vault_metadata WHERE key='canonical_revision'").fetch_one(&mut *tx).await.map_err(err)?;
    let binding=json!({"context":self.context,"revision":revision,"root":self.root.path});
    if expected.as_ref().is_some_and(|original|original!=&binding){return Err("Original canonical imported consumer binding changed".into())}
    let value=if let Some(selected)=selected{
     if length>65536||[&selected.space_id,&selected.document_id,&selected.asset_id].iter().any(|s|s.is_empty()||s.len()>256){return Err("Exact bounded imported attachment selection required".into())}
     native_archive_capture::strict_preflight(&mut tx).await?;let snapshot=read_snapshot_from_connection(&mut tx,&self.provider).await?.ok_or("Current canonical snapshot required")?;
     let asset=snapshot["assets"].as_array().and_then(|a|a.iter().find(|a|a["id"]==selected.asset_id&&a["space_id"]==selected.space_id&&a["document_id"]==selected.document_id)).ok_or("Current imported attachment association missing")?.clone();
     let total:i64=sqlx::query_scalar("SELECT COALESCE(SUM(length(CAST(i.request_json AS BLOB))+length(CAST(r.result_json AS BLOB))),0) FROM native_archive_import_intents i JOIN native_archive_import_receipts r ON r.operation_id=i.operation_id AND r.request_fingerprint=i.request_fingerprint").fetch_one(&mut *tx).await.map_err(err)?;let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM native_archive_import_receipts").fetch_one(&mut *tx).await.map_err(err)?;if total>64*1024*1024||count>4096{return Err("Imported consumer evidence exceeds bound".into())}
     let instance:String=sqlx::query_scalar("SELECT instance_id FROM native_archive_instance WHERE singleton=1").fetch_one(&mut *tx).await.map_err(err)?;
     let rows:Vec<(String,String,String,String,String,String,String)>=sqlx::query_as("SELECT i.operation_id,i.request_fingerprint,i.provider,i.instance_id,i.request_json,r.result_json,r.result_digest FROM native_archive_import_intents i JOIN native_archive_import_receipts r ON r.operation_id=i.operation_id AND r.request_fingerprint=i.request_fingerprint ORDER BY i.operation_id").fetch_all(&mut *tx).await.map_err(err)?;if rows.len()!=count as usize{return Err("Imported consumer receipt chain contains an orphan".into())}
     let owned=native_archive_import::capture_evidence(self.selection.clone(),&self.provider,self.context.clone(),&snapshot,&instance,&rows,&asset)?;
     let bytes=tokio::task::spawn_blocking(move||owned.reader.read_range(offset,length)).await.map_err(err)??;
     json!({"format":1,"binding":binding,"selection":{"space_id":selected.space_id,"document_id":selected.document_id,"asset_id":selected.asset_id},"offset":offset,"bytes":bytes,"byte_length":asset["size"],"raw_version_record":asset["server_version_provenance"]["raw_version_record"]})
    }else{json!({"format":1,"binding":binding,"initialized":true,"authority":"local_archive_provenance","export_authority":false,"retry_authority":false,"serve_authority":false})};
    self.fence()?;physical.check()?;tx.rollback().await.map_err(err)?;Ok(value)
   }.await;db.retire().await.map_err(|e|format!("Archive descriptor physical retirement failed: {e}"))?;result
  }).await
 }
}
#[tauri::command]
pub(crate) async fn describe_current_native_vault_archive<R:tauri::Runtime>(app:AppHandle<R>,provider:String,context:Value,current:Value)->Result<Value,String>{Current::capture(vault::capture_snapshot_request(&app,&provider)?,&provider,context)?.run(current,None,None,0,0).await}
#[tauri::command]
pub(crate) async fn read_imported_native_attachment_preview<R:tauri::Runtime>(app:AppHandle<R>,provider:String,context:Value,current:Value,selection:AssetSelection,original_binding:Value,offset:u64,length:u64)->Result<Value,String>{Current::capture(vault::capture_snapshot_request(&app,&provider)?,&provider,context)?.run(current,Some(selection),Some(original_binding),offset,length).await}
#[cfg(test)]mod tests;

#[tauri::command]
pub(crate) async fn list_original_native_trash_documents<R:tauri::Runtime>(app:AppHandle<R>,provider:String,context:Value,current:Value,space_id:String)->Result<Value,String>{Current::capture(vault::capture_snapshot_request(&app,&provider)?,&provider,context)?.trash(current,space_id).await}

pub(crate) mod cover;
