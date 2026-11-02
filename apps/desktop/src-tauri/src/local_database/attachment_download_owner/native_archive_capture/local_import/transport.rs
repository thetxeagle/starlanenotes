//! Real invoking WebKit view and random native token own each open creator.
//! An interrupted/unknown operation is retained, never silently restarted.
use super::*;
use std::{collections::BTreeMap,sync::Mutex};
use tauri::Manager;
use webkit2gtk::glib::prelude::*;
struct Pending{label:String,view:usize,creator:Creator,source:Option<path::Source>}
#[derive(Default)]pub(crate) struct Producers(Mutex<BTreeMap<String,Pending>>);
async fn frame<R:tauri::Runtime>(window:&tauri::WebviewWindow<R>)->Result<(String,usize),String>{
 let label=window.label().to_string();let(send,receive)=tokio::sync::oneshot::channel();
 window.with_webview(move|platform|{let _=send.send(platform.inner().as_ptr()as usize);}).map_err(err)?;
 Ok((label,receive.await.map_err(err)?))
}
pub(super) fn owner<R:tauri::Runtime>(app:&AppHandle<R>,provider:&str,context:Value,binding:&Value)->Result<Owner,String>{
 if binding.as_object().map(|v|v.len())!=Some(3)||binding["context"]!=context||context["accountId"]!="local-user"{return Err("Exact original local import binding required".into())}
 let owner=Owner::capture(vault::capture_snapshot_request(app,provider)?,provider,binding["revision"].as_str().ok_or("Current cover revision missing")?,context)?;
 if owner.binding()!=*binding{return Err("Original cover selection changed".into())}Ok(owner)
}
impl Producers{
 fn put(&self,label:String,view:usize,creator:Creator)->Result<Value,String>{
  self.put_source(label,view,creator,None)
 }
 fn put_source(&self,label:String,view:usize,creator:Creator,source:Option<path::Source>)->Result<Value,String>{
  let mut records=self.0.lock().map_err(err)?;if records.len()>=8{return Err("Native local import creators full; retain/review pending attempts".into())}
  let mut random=[0u8;32];std::fs::File::open("/dev/urandom").map_err(err)?.read_exact(&mut random).map_err(err)?;let token:String=random.iter().map(|b|format!("{b:02x}")).collect();
  if records.contains_key(&token){return Err("Local import token collision".into())}let operation=creator.request.operation_id.clone();let prepared=json!({"asset_id":format!("asset-import-{}",operation),"queue_operation_id":format!("syncop-import-{}",operation),"created_at":creator.proof.created_at,"draft_created_at":creator.request.draft_created_at});records.insert(token.clone(),Pending{label,view,creator,source});Ok(json!({"format":1,"operation_id":operation,"producer_token":token,"prepared":prepared}))
 }
 fn recover(&self,frame:&(String,usize),request:&Request,current:&Value)->Result<Option<(Creator,Option<path::Source>)>,String>{
  let mut records=self.0.lock().map_err(err)?;let found=records.iter().find(|(_,pending)|pending.creator.request.operation_id==request.operation_id).map(|(token,_)|token.clone());
  let Some(token)=found else{return Ok(None)};let pending=records.get(&token).ok_or("Original creator unavailable")?;
  if pending.label!=frame.0||pending.view!=frame.1||serde_json::to_value(&pending.creator.request).map_err(err)?!=serde_json::to_value(request).map_err(err)?{return Err("Original local import creator belongs to another invoking frame/request".into())}pending.creator.fence(current)?;
  Ok(records.remove(&token).map(|p|(p.creator,p.source)))
 }
 fn take(&self,frame:&(String,usize),token:&str,current:&Value)->Result<Pending,String>{
  if token.len()!=64||!token.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){return Err("Exact native local import token required".into())}
  let mut records=self.0.lock().map_err(err)?;let pending=records.get(token).ok_or("Original creator unavailable; retain original operation for review")?;
  if pending.label!=frame.0||pending.view!=frame.1{return Err("Original native local import frame changed".into())}pending.creator.fence(current)?;if let Some(source)=&pending.source{source.fence()?;}
  records.remove(token).ok_or("Original cover creator unavailable".into())
 }
}
#[tauri::command]pub(crate) async fn prepare_original_native_local_import<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,provider:String,context:Value,current:Value,request:Request)->Result<Value,String>{
 if request.path_source.is_some(){return Err("Original path imports require their dedicated producer".into())}if current!=context{return Err("Original cover local session changed".into())}let(label,view)=frame(&window).await?;
 let owner=owner(&app,&provider,context,&request.original_binding)?;let creator=owner.prepare_local_import(current,request).await?;
 app.try_state::<Producers>().ok_or("Native cover producer registry unavailable")?.put(label,view,creator)
}
#[tauri::command]pub(crate) async fn resume_original_native_local_import<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,provider:String,context:Value,current:Value,request:Request)->Result<Value,String>{
 if current!=context{return Err("Original local import session changed".into())}let actual=frame(&window).await?;let owner=owner(&app,&provider,context,&request.original_binding)?;let state=app.try_state::<Producers>().ok_or("Native cover producer registry unavailable")?;
 let source=match &request.path_source{Some(proof)=>Some(path::Source::capture(&owner,&proof.source_path,Some(proof),request.byte_length)?),None=>None};
 let(creator,held)=match state.recover(&actual,&request,&current)?{Some(pair)=>pair,None=>(owner.resume_local_import(current,request).await?,None)};let source=held.or(source);let received=creator.expected.length;let next_index=creator.index;
 let mut result=state.put_source(actual.0,actual.1,creator,source)?;result["received"]=json!(received);result["next_index"]=json!(next_index);Ok(result)
}
#[tauri::command]pub(crate) async fn append_original_native_local_import<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,producer_token:String,current:Value,index:u64,bytes:Vec<u8>)->Result<Value,String>{
 if bytes.is_empty()||bytes.len()>CHUNK{return Err("Bounded original File chunk required".into())}let actual=frame(&window).await?;let state=app.try_state::<Producers>().ok_or("Native cover producer registry unavailable")?;let pending=state.take(&actual,&producer_token,&current)?;
 if pending.source.is_some(){return Err("Original path source must be streamed by its native producer".into())}
 let(creator,ack)=pending.creator.append_owned(current,index,bytes).await?;
 // Rotate capability after each durably acknowledged chunk; lost acknowledgement
 // never authorizes a second write or a new operation.
 let mut next=state.put(pending.label,pending.view,creator)?;next["index"]=ack["index"].clone();next["received"]=ack["received"].clone();Ok(next)
}
#[tauri::command]pub(crate) async fn finish_original_native_local_import<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,producer_token:String,current:Value)->Result<Value,String>{
 let actual=frame(&window).await?;let pending=app.try_state::<Producers>().ok_or("Native cover producer registry unavailable")?.take(&actual,&producer_token,&current)?;if pending.source.is_some(){return Err("Original path source requires native stream completion".into())}pending.creator.finish(current).await
}
#[tauri::command]pub(crate) async fn inspect_original_native_local_import<R:tauri::Runtime>(app:AppHandle<R>,provider:String,context:Value,current:Value,original_binding:Value,operation_id:String)->Result<Value,String>{
 owner(&app,&provider,context,&original_binding)?.with_local_import(current,operation_id,|reader|Ok(reader.acknowledged())).await
}

#[cfg(test)]mod tests{
 use super::*;
 #[tokio::test]async fn native_registry_never_borrows_other_view_or_session_and_consumes_exact_token(){
  let(root,owner,request)=super::super::tests::fixture("registry").await;let current=owner.original.clone();let creator=owner.prepare_local_import(current.clone(),request).await.unwrap();let registry=Producers::default();let ack=registry.put("main".into(),41,creator).unwrap();let token=ack["producer_token"].as_str().unwrap();
  assert!(registry.take(&("main".into(),42),token,&current).is_err());assert!(registry.take(&("foreign".into(),41),token,&current).is_err());
  let mut other=current.clone();other["sessionId"]=json!("changed");assert!(registry.take(&("main".into(),41),token,&other).is_err());
  let original=registry.take(&("main".into(),41),token,&current).unwrap();assert!(registry.take(&("main".into(),41),token,&current).is_err());drop(original);std::fs::remove_dir_all(root).unwrap();
 }
}

#[tauri::command]pub(crate) async fn prepare_original_native_local_import_path<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,provider:String,context:Value,current:Value,mut request:Request,source_path:String,expected_source:path::SourceProof)->Result<Value,String>{
 if current!=context||request.path_source.is_some(){return Err("Original path base request/context required".into())}let(label,view)=frame(&window).await?;let owner=owner(&app,&provider,context,&request.original_binding)?;
 let source=path::Source::capture(&owner,&source_path,Some(&expected_source),request.byte_length)?;request.path_source=Some(source.proof.clone());let original=serde_json::to_value(&request).map_err(err)?;let creator=owner.prepare_local_import(current,request).await?;source.fence()?;
 let mut ack=app.try_state::<Producers>().ok_or("Original import registry unavailable")?.put_source(label,view,creator,Some(source))?;ack["request"]=original;Ok(ack)
}
#[tauri::command]pub(crate) async fn run_original_native_local_import_path<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,producer_token:String,current:Value)->Result<Value,String>{
 let actual=frame(&window).await?;let pending=app.try_state::<Producers>().ok_or("Original import registry unavailable")?.take(&actual,&producer_token,&current)?;pending.source.ok_or("Original native path descriptor unavailable")?.run(pending.creator,current).await
}
#[tauri::command]pub(crate) async fn recover_original_native_local_import_path_request<R:tauri::Runtime>(app:AppHandle<R>,provider:String,context:Value,current:Value,request:Request)->Result<Value,String>{
 let owner=owner(&app,&provider,context,&request.original_binding)?;serde_json::to_value(owner.path_request(current,request).await?).map_err(err)
}

#[tauri::command]pub(crate) async fn inspect_original_native_local_import_path_source<R:tauri::Runtime>(app:AppHandle<R>,provider:String,context:Value,current:Value,original_binding:Value,source_path:String)->Result<Value,String>{
 if current!=context{return Err("Original path context changed".into())}path::metadata(&owner(&app,&provider,context,&original_binding)?,&source_path)
}
