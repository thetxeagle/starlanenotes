//! Receipt-backed bounded preview and two-phase complete native export.
use super::*;
use serde_json::json;
use tauri_plugin_dialog::DialogExt;
use std::collections::HashMap;
use std::sync::{Mutex,OnceLock};
use std::time::{Duration,Instant};
struct Selected {
 parent:filesystem::Directory, leaf:String, path:std::path::PathBuf,
 binding:String, created:Instant,
}
fn selections()->&'static Mutex<HashMap<String,Selected>> {static S:OnceLock<Mutex<HashMap<String,Selected>>>=OnceLock::new();S.get_or_init(||Mutex::new(HashMap::new()))}
fn remember(owner:&custody::Captured,path:std::path::PathBuf)->Result<String,String>{
 use std::io::Read;
 let parent=filesystem::Directory::capture(path.parent().ok_or("Native export parent missing")?).map_err(|e|e.to_string())?;
 let leaf=path.file_name().and_then(|n|n.to_str()).ok_or("Native export leaf invalid")?.to_string();
 parent.fence().map_err(|e|e.to_string())?;
 let mut random=[0u8;32];std::fs::File::open("/dev/urandom").map_err(|e|e.to_string())?.read_exact(&mut random).map_err(|e|e.to_string())?;
 let token=random.iter().map(|b|format!("{b:02x}")).collect::<String>();
 let selected=Selected{parent,leaf,path,binding:owner.delivery_binding()?,created:Instant::now()};
 let mut held=selections().lock().map_err(|_|"Native export selections unavailable")?;
 held.retain(|_,s|s.created.elapsed()<Duration::from_secs(600));
 if held.len()>=8 {return Err("Other native export dialogs pending; finish or cancel before retry".into());}
 held.insert(token.clone(),selected);Ok(token)
}
fn take(owner:&custody::Captured,token:&str)->Result<Selected,String>{
 let mut held=selections().lock().map_err(|_|"Native export selections unavailable")?;
 let selected=held.get(token).ok_or("Original native export selection missing; choose again")?;
 if selected.created.elapsed()>=Duration::from_secs(600)||selected.binding!=owner.delivery_binding()? {return Err("Native export selection differs from original request".into());}
 Ok(held.remove(token).unwrap())
}
#[tauri::command]
pub(crate) fn discard_native_attachment_export_selection(token:String)->Result<(),String>{
 selections().lock().map_err(|_|"Native export selections unavailable")?.remove(&token);Ok(())
}
#[tauri::command]
pub(crate) async fn read_native_attachment_preview<R:tauri::Runtime>(app:AppHandle<R>,request:Value,current:Value,offset:u64,requested:usize)->Result<Value,String>{
 if requested>64*1024 {return Err("Native preview range exceeds64KiB".into());}
 let owner=caller(&app,request,&current)?;
 owner.delivery_operation(&current,move |_,snapshot|{
  let bytes=snapshot.read(offset,requested).map_err(|e|e.to_string())?;
  Ok(json!({"format":1,"offset":offset,"bytes":bytes,"preview_only":true}))
 }).await
}
#[tauri::command]
pub(crate) async fn choose_native_attachment_export_destination<R:tauri::Runtime>(app:AppHandle<R>,request:Value,current:Value)->Result<Value,String>{
 let owner=caller(&app,request,&current)?;
 let prior=owner.prepare_fetch(&current).await?.ok_or("Committed native download required")?;
 if prior["state"]!="current" {return Err("Historical receipt cannot create an export".into());}
 let selected=tokio::task::spawn_blocking(move ||app.dialog().file().set_title("Export original attachment version").blocking_save_file()).await.map_err(|e|e.to_string())?;
 let Some(selected)=selected else {return Ok(json!({"format":1,"cancelled":true}));};
 let selected=selected.into_path().map_err(|_|"Native export requires local filesystem destination")?;
 owner.delivery_fence(&current)?;
 let token=remember(&owner,selected)?;
 Ok(json!({"format":1,"cancelled":false,"destination_token":token}))
}
#[tauri::command]
pub(crate) async fn commit_native_attachment_export<R:tauri::Runtime>(app:AppHandle<R>,request:Value,current:Value,destination_token:String)->Result<Value,String>{
 let owner=caller(&app,request,&current)?;
 let selected=take(&owner,&destination_token)?;
 let export_current=current.clone();
 owner.delivery_operation(&current,move |owner,snapshot|{
  owner.delivery_fence(&export_current)?;selected.parent.fence().map_err(|e|e.to_string())?;
  let bytes=snapshot.export_no_clobber(&selected.parent.held,&selected.leaf).map_err(|e|e.to_string())?;
  selected.parent.fence().map_err(|e|format!("Complete export published; destination custody changed: {e}"))?;
  owner.delivery_fence(&export_current).map_err(|e|format!("Complete export published; original selection changed: {e}"))?;
  Ok(json!({"format":1,"cancelled":false,"byte_length":bytes,"path":selected.path}))
 }).await
}
#[cfg(test)]
pub(super) fn test_remember(owner:&custody::Captured,path:std::path::PathBuf)->Result<String,String>{remember(owner,path)}
#[cfg(test)]
pub(super) fn test_take(owner:&custody::Captured,token:&str)->Result<(),String>{take(owner,token).map(|_|())}
