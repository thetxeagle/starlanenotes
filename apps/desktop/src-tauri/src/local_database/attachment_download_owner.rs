//! Linux immutable-download owner with bounded chunks and retained retry evidence.
use super::*;
use std::path::Path;
mod filesystem;
mod vfs;
mod connection;
mod custody;

#[cfg(test)]
mod tests;

// Every command recaptures the original selected destination before awaits.
#[tauri::command]
pub(crate) fn capture_native_attachment_destination<R:tauri::Runtime>(app: AppHandle<R>, provider: String) -> Result<Value,String> {
    let selection=vault::capture_snapshot_request(&app,&provider)?;
    serde_json::to_value(custody::Destination::capture(&selection)?).map_err(|e|e.to_string())
}
fn caller<R:tauri::Runtime>(app:&AppHandle<R>,request:Value,current:&Value)->Result<custody::Captured,String> {
    let request:custody::Request=serde_json::from_value(request).map_err(|_|"Native caller request invalid")?;
    let provider=request.origin["provider"].as_str().ok_or("Native provider missing")?;
    let selection=vault::capture_snapshot_request(app,provider)?;
    custody::Captured::capture(selection,request,current)
}
#[tauri::command]
pub(crate) async fn prepare_native_attachment_download<R:tauri::Runtime>(app:AppHandle<R>,request:Value,current:Value)->Result<Option<Value>,String> {
    let owner=caller(&app,request,&current)?;
    let prior=owner.prepare_fetch(&current).await?;
    if prior.is_none(){owner.begin(&current).await?;}
    Ok(prior)
}
#[tauri::command]
pub(crate) async fn append_native_attachment_download<R:tauri::Runtime>(app:AppHandle<R>,request:Value,current:Value,index:u64,bytes:Vec<u8>)->Result<(),String> {
    let owner=caller(&app,request,&current)?;
    owner.append(&current,index,bytes).await
}
#[tauri::command]
pub(crate) async fn finish_native_attachment_download<R:tauri::Runtime>(app:AppHandle<R>,request:Value,current:Value)->Result<Value,String> {
    let owner=caller(&app,request,&current)?;
    owner.finish(&current).await
}

mod transport;
pub(crate) mod delivery;
pub(crate) use delivery::{read_native_attachment_preview,choose_native_attachment_export_destination,commit_native_attachment_export,discard_native_attachment_export_selection};

pub(crate) mod image_stream;

// Unregistered canonical capture seam; separate archive authority.
#[allow(dead_code)]
pub(crate) mod native_archive_capture;

#[allow(dead_code)]
pub(crate) mod native_archive_output;

#[allow(dead_code)]
pub(crate) mod native_archive_review;

#[allow(dead_code)]
pub(crate) mod native_archive_import;

pub(crate) mod native_archive_context;

pub(crate) mod native_archive_export;

pub(crate) mod native_document_delete;
