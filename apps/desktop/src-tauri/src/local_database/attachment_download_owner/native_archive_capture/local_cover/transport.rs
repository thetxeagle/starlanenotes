//! Real invoking WebKit view and random native token own each open creator.
//! An interrupted/unknown operation is retained, never silently restarted.
use super::*;
use std::{collections::BTreeMap,sync::Mutex};
use tauri::Manager;
use webkit2gtk::glib::prelude::*;
struct Pending{label:String,view:usize,creator:Creator}
#[derive(Default)]pub(crate) struct Producers(Mutex<BTreeMap<String,Pending>>);
async fn frame<R:tauri::Runtime>(window:&tauri::WebviewWindow<R>)->Result<(String,usize),String>{
 let label=window.label().to_string();let(send,receive)=tokio::sync::oneshot::channel();
 window.with_webview(move|platform|{let _=send.send(platform.inner().as_ptr()as usize);}).map_err(err)?;
 Ok((label,receive.await.map_err(err)?))
}
pub(super) fn owner<R:tauri::Runtime>(app:&AppHandle<R>,provider:&str,context:Value,binding:&Value)->Result<Owner,String>{
 if binding.as_object().map(|v|v.len())!=Some(3)||binding["context"]!=context||context["accountId"]!="local-user"{return Err("Exact original local cover binding required".into())}
 let owner=Owner::capture(vault::capture_snapshot_request(app,provider)?,provider,binding["revision"].as_str().ok_or("Current cover revision missing")?,context)?;
 if owner.binding()!=*binding{return Err("Original cover selection changed".into())}Ok(owner)
}
impl Producers{
 fn put(&self,label:String,view:usize,creator:Creator)->Result<Value,String>{
  let mut records=self.0.lock().map_err(err)?;if records.len()>=8{return Err("Native local cover creators full; retain/review pending attempts".into())}
  let mut random=[0u8;32];std::fs::File::open("/dev/urandom").map_err(err)?.read_exact(&mut random).map_err(err)?;let token:String=random.iter().map(|b|format!("{b:02x}")).collect();
  if records.contains_key(&token){return Err("Local cover token collision".into())}let operation=creator.request.operation_id.clone();records.insert(token.clone(),Pending{label,view,creator});Ok(json!({"format":1,"operation_id":operation,"producer_token":token}))
 }
 fn recover(&self,frame:&(String,usize),request:&Request,current:&Value)->Result<Option<Creator>,String>{
  let mut records=self.0.lock().map_err(err)?;let found=records.iter().find(|(_,pending)|pending.creator.request.operation_id==request.operation_id).map(|(token,_)|token.clone());
  let Some(token)=found else{return Ok(None)};let pending=records.get(&token).ok_or("Original creator unavailable")?;
  if pending.label!=frame.0||pending.view!=frame.1||serde_json::to_value(&pending.creator.request).map_err(err)?!=serde_json::to_value(request).map_err(err)?{return Err("Original local cover creator belongs to another invoking frame/request".into())}pending.creator.fence(current)?;
  Ok(records.remove(&token).map(|p|p.creator))
 }
 fn take(&self,frame:&(String,usize),token:&str,current:&Value)->Result<Pending,String>{
  if token.len()!=64||!token.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){return Err("Exact native local cover token required".into())}
  let mut records=self.0.lock().map_err(err)?;let pending=records.get(token).ok_or("Original creator unavailable; retain original operation for review")?;
  if pending.label!=frame.0||pending.view!=frame.1{return Err("Original native local cover frame changed".into())}pending.creator.fence(current)?;
  records.remove(token).ok_or("Original cover creator unavailable".into())
 }
}
#[tauri::command]pub(crate) async fn prepare_original_native_local_cover<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,provider:String,context:Value,current:Value,request:Request)->Result<Value,String>{
 if current!=context{return Err("Original cover local session changed".into())}let(label,view)=frame(&window).await?;
 let owner=owner(&app,&provider,context,&request.original_binding)?;let creator=owner.prepare_local_cover(current,request).await?;
 app.try_state::<Producers>().ok_or("Native cover producer registry unavailable")?.put(label,view,creator)
}
#[tauri::command]pub(crate) async fn resume_original_native_local_cover<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,provider:String,context:Value,current:Value,request:Request)->Result<Value,String>{
 if current!=context{return Err("Original local cover session changed".into())}let actual=frame(&window).await?;let owner=owner(&app,&provider,context,&request.original_binding)?;let state=app.try_state::<Producers>().ok_or("Native cover producer registry unavailable")?;
 let creator=match state.recover(&actual,&request,&current)?{Some(creator)=>creator,None=>owner.resume_local_cover(current,request).await?};let received=creator.expected.length;let next_index=creator.index;
 let mut result=state.put(actual.0,actual.1,creator)?;result["received"]=json!(received);result["next_index"]=json!(next_index);Ok(result)
}
#[tauri::command]pub(crate) async fn append_original_native_local_cover<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,producer_token:String,current:Value,index:u64,bytes:Vec<u8>)->Result<Value,String>{
 if bytes.is_empty()||bytes.len()>CHUNK{return Err("Bounded original File chunk required".into())}let actual=frame(&window).await?;let state=app.try_state::<Producers>().ok_or("Native cover producer registry unavailable")?;let pending=state.take(&actual,&producer_token,&current)?;
 let(creator,ack)=pending.creator.append_owned(current,index,bytes).await?;
 // Rotate capability after each durably acknowledged chunk; lost acknowledgement
 // never authorizes a second write or a new operation.
 let mut next=state.put(pending.label,pending.view,creator)?;next["index"]=ack["index"].clone();next["received"]=ack["received"].clone();Ok(next)
}
#[tauri::command]pub(crate) async fn finish_original_native_local_cover<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,producer_token:String,current:Value)->Result<Value,String>{
 let actual=frame(&window).await?;let pending=app.try_state::<Producers>().ok_or("Native cover producer registry unavailable")?.take(&actual,&producer_token,&current)?;pending.creator.finish(current).await
}
/// Explicit original-operation action. Current native view/source and the real
/// original native capsule remain required; client journal labels grant nothing.
#[tauri::command]pub(crate) async fn discard_original_native_local_cover<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,provider:String,context:Value,current:Value,request:Request)->Result<Value,String>{
 if current!=context{return Err("Original discard local session changed".into())}let _invoking_frame=frame(&window).await?;
 owner(&app,&provider,context,&request.original_binding)?.discard_local_cover(current,request,false).await.map_err(|error|format!("Original discard unresolved; retain operation evidence: {error}"))
}
#[tauri::command]pub(crate) async fn inspect_original_native_local_cover_discard<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,provider:String,context:Value,current:Value,request:Request)->Result<Value,String>{
 if current!=context{return Err("Original discard local session changed".into())}let _invoking_frame=frame(&window).await?;
 owner(&app,&provider,context,&request.original_binding)?.discard_local_cover(current,request,true).await.map_err(|error|format!("Original discard result unresolved; retain operation evidence: {error}"))
}
#[tauri::command]pub(crate) async fn inspect_original_native_local_cover<R:tauri::Runtime>(app:AppHandle<R>,provider:String,context:Value,current:Value,original_binding:Value,operation_id:String)->Result<Value,String>{
 owner(&app,&provider,context,&original_binding)?.with_local_cover(current,operation_id,|reader|Ok(reader.acknowledged())).await
}

#[cfg(test)]mod tests{
 use super::*;
 #[tokio::test]async fn native_registry_never_borrows_other_view_or_session_and_consumes_exact_token(){
  let(root,owner,request)=super::super::tests::fixture("registry").await;let current=owner.original.clone();let creator=owner.prepare_local_cover(current.clone(),request).await.unwrap();let registry=Producers::default();let ack=registry.put("main".into(),41,creator).unwrap();let token=ack["producer_token"].as_str().unwrap();
  assert!(registry.take(&("main".into(),42),token,&current).is_err());assert!(registry.take(&("foreign".into(),41),token,&current).is_err());
  let mut other=current.clone();other["sessionId"]=json!("changed");assert!(registry.take(&("main".into(),41),token,&other).is_err());
  let original=registry.take(&("main".into(),41),token,&current).unwrap();assert!(registry.take(&("main".into(),41),token,&current).is_err());drop(original);std::fs::remove_dir_all(root).unwrap();
 }
}
