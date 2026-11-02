//! Linux/WebKitGTK receipt-backed complete image transport. General asset:// is unchanged.
//! App storage I/O uses 64KiB capture chunks and a GIO held-file stream; WebKit's
//! decoder/pixel memory is outside this storage-buffer claim.
use super::*;
use std::collections::{HashMap};
use std::sync::{Mutex,OnceLock};
use std::time::{Duration,Instant};
use webkit2gtk::{gio,glib,WebContextExt,URISchemeRequestExt,WebViewExt,SecurityManagerExt};
use glib::prelude::*;
const SCHEME:&str="starlane-owned-image";
struct Ticket {owner:custody::Captured,current:Value,mime:String,webview:usize,created:Instant}
fn tickets()->&'static Mutex<HashMap<String,Ticket>> {static T:OnceLock<Mutex<HashMap<String,Ticket>>>=OnceLock::new();T.get_or_init(||Mutex::new(HashMap::new()))}
thread_local!{static CONTEXTS:std::cell::RefCell<HashMap<usize,glib::WeakRef<webkit2gtk::WebContext>>>=std::cell::RefCell::new(HashMap::new());}
fn fail(request:&webkit2gtk::URISchemeRequest){request.finish_error(&mut glib::Error::new(gio::IOErrorEnum::PermissionDenied,"Original owned image unavailable"));}
fn register(context:&webkit2gtk::WebContext){
 let fresh=CONTEXTS.with(|s|{let mut contexts=s.borrow_mut();contexts.retain(|_,c|c.upgrade().is_some());let id=context.as_ptr() as usize;if contexts.contains_key(&id){return false;}contexts.insert(id,context.downgrade());true});if !fresh{return;}
 if let Some(security)=context.security_manager(){security.register_uri_scheme_as_secure(SCHEME);security.register_uri_scheme_as_no_access(SCHEME);}
 context.register_uri_scheme(SCHEME,|request|{
  let Some(uri)=request.uri() else {fail(request);return;};
  let Some(token)=uri.strip_prefix("starlane-owned-image://image/") else {fail(request);return;};
  if token.len()!=64||!token.bytes().all(|b|b.is_ascii_hexdigit()&&!b.is_ascii_uppercase()){fail(request);return;}
  let Some(view)=request.web_view() else {fail(request);return;};
  let selected={let Ok(mut held)=tickets().lock() else{fail(request);return;};
   let Some(ticket)=held.get(token) else{fail(request);return;};
   if ticket.created.elapsed()>Duration::from_secs(120)||ticket.webview!=view.as_ptr() as usize{fail(request);return;}
   held.remove(token).unwrap()};
  let request=request.clone();
  glib::MainContext::default().spawn_local(async move {
   let mime=selected.mime;
   let result=tauri::async_runtime::spawn(async move {
    selected.owner.delivery_operation(&selected.current,|_,snapshot|snapshot.into_stream_file().map_err(|e|e.to_string())).await
   }).await;
   match result {
    Ok(Ok((file,length)))=>{
     // Public GIO constructor takes the owned anonymous descriptor. No pathname
     // reopen, whole-file Vec, or data URL. GIO closes the FD with the stream.
     let stream=unsafe{gio::UnixInputStream::take_fd(file)};
     request.finish(&stream,length as i64,Some(&mime));
    },_=>fail(&request)
   }
  });
 });
}
#[tauri::command]
pub(crate) async fn open_native_attachment_image<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,request:Value,current:Value)->Result<Value,String>{
 let owner=caller(&app,request.clone(),&current)?;
 let prior=owner.prepare_fetch(&current).await?.ok_or("Committed native image receipt required")?;
 if prior["state"]!="current"{return Err("Historical receipt cannot create image delivery".into());}
 let raw=request["server_version_raw"].as_str().ok_or("Original image version missing")?;
 let version:Value=serde_json::from_str(raw).map_err(|_|"Original image version invalid")?;
 let mime=version["mime_type"].as_str().ok_or("Original image media type missing")?.to_string();
 if !matches!(mime.as_str(),"image/png"|"image/jpeg"|"image/webp"|"image/gif"|"image/avif"|"image/svg+xml"){return Err("Original version is not a supported image".into());}
 owner.delivery_fence(&current)?;
 let mut random=[0u8;32];use std::io::Read;std::fs::File::open("/dev/urandom").map_err(|e|e.to_string())?.read_exact(&mut random).map_err(|e|e.to_string())?;
 let token=random.iter().map(|b|format!("{b:02x}")).collect::<String>();
 let uri=format!("{SCHEME}://image/{token}");let (send,receive)=tokio::sync::oneshot::channel();
 window.with_webview(move|platform|{
  let view=platform.inner();let result=(||{let context=view.context().ok_or("Native WebKit context unavailable")?;register(&context);
   let mut held=tickets().lock().map_err(|_|"Owned image tickets unavailable")?;held.retain(|_,t|t.created.elapsed()<Duration::from_secs(120));
   if held.len()>=8{return Err("Other native images pending; finish or retry".to_string());}
   held.insert(token,Ticket{owner,current,mime,webview:view.as_ptr() as usize,created:Instant::now()});Ok(())})();let _=send.send(result);
 }).map_err(|e|e.to_string())?;
 receive.await.map_err(|_|"Native image registration did not complete")??;
 Ok(serde_json::json!({"format":1,"url":uri,"complete_image":true}))
}
