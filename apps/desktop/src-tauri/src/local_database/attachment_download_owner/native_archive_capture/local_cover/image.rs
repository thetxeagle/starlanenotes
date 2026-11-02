//! Current local canonical/native evidence -> anonymous held GIO image stream.
//! Stable local pin and ephemeral preview are distinct; no asset path reopening.
use super::*;
use std::{collections::HashMap,sync::{Mutex,OnceLock},time::{Duration,Instant}};
use webkit2gtk::{gio,glib,WebContextExt,URISchemeRequestExt,WebViewExt,SecurityManagerExt};
use glib::prelude::*;
const SCHEME:&str="starlane-local-cover-preview";
struct Ticket{owner:Owner,current:Value,operation:String,space:String,page:String,mime:String,view:usize,created:Instant}
fn tickets()->&'static Mutex<HashMap<String,Ticket>>{static T:OnceLock<Mutex<HashMap<String,Ticket>>>=OnceLock::new();T.get_or_init(||Mutex::new(HashMap::new()))}
thread_local!{static CONTEXTS:std::cell::RefCell<HashMap<usize,glib::WeakRef<webkit2gtk::WebContext>>>=std::cell::RefCell::new(HashMap::new());}
fn fail(request:&webkit2gtk::URISchemeRequest){request.finish_error(&mut glib::Error::new(gio::IOErrorEnum::PermissionDenied,"Current original local cover unavailable"));}
fn register(context:&webkit2gtk::WebContext){
 let fresh=CONTEXTS.with(|s|{let mut c=s.borrow_mut();c.retain(|_,v|v.upgrade().is_some());let id=context.as_ptr()as usize;if c.contains_key(&id){false}else{c.insert(id,context.downgrade());true}});if !fresh{return;}
 if let Some(security)=context.security_manager(){security.register_uri_scheme_as_secure(SCHEME);security.register_uri_scheme_as_no_access(SCHEME);}
 context.register_uri_scheme(SCHEME,|request|{
  let Some(uri)=request.uri()else{fail(request);return;};let Some(token)=uri.strip_prefix("starlane-local-cover-preview://image/")else{fail(request);return;};
  if token.len()!=64||!token.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){fail(request);return;}
  let Some(view)=request.web_view()else{fail(request);return;};let ticket={let Ok(mut rows)=tickets().lock()else{fail(request);return;};let Some(row)=rows.get(token)else{fail(request);return;};if row.created.elapsed()>Duration::from_secs(120)||row.view!=view.as_ptr()as usize{fail(request);return;}rows.remove(token).unwrap()};
  let request=request.clone();glib::MainContext::default().spawn_local(async move{
   let mime=ticket.mime;let result=tauri::async_runtime::spawn(async move{ticket.owner.with_local_cover(ticket.current,ticket.operation,move|r|r.stream(&ticket.space,&ticket.page)).await}).await;
   match result{Ok(Ok((file,length)))=>{let stream=unsafe{gio::UnixInputStream::take_fd(file)};request.finish(&stream,length as i64,Some(&mime));},_=>fail(&request)}
  });
 });
}
#[tauri::command]pub(crate) async fn open_original_native_local_cover_image<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,provider:String,context:Value,current:Value,original_binding:Value,operation_id:String,space_id:String,document_id:String)->Result<Value,String>{
 if current!=context||!uuid(&operation_id)||!identifier(&space_id)||!identifier(&document_id){return Err("Current original local cover selection required".into())}
 let owner=transport::owner(&app,&provider,context,&original_binding)?;let space=space_id.clone();let page=document_id.clone();
 let mime=owner.clone().with_local_cover(current.clone(),operation_id.clone(),move|r|{r.image_mime(&space,&page)}).await?;
 let mut bytes=[0u8;32];std::fs::File::open("/dev/urandom").map_err(err)?.read_exact(&mut bytes).map_err(err)?;let token:String=bytes.iter().map(|b|format!("{b:02x}")).collect();let url=format!("{SCHEME}://image/{token}");let(send,receive)=tokio::sync::oneshot::channel::<Result<(),String>>();
 window.with_webview(move|platform|{let view=platform.inner();let result=(||{let context=view.context().ok_or("Native WebKit context unavailable")?;register(&context);let mut rows=tickets().lock().map_err(err)?;rows.retain(|_,r|r.created.elapsed()<Duration::from_secs(120));if rows.len()>=8{return Err("Original local cover previews full".into())}if rows.contains_key(&token){return Err("Original local cover preview token collision".into())}rows.insert(token,Ticket{owner,current,operation:operation_id,space:space_id,page:document_id,mime,view:view.as_ptr()as usize,created:Instant::now()});Ok(())})();let _=send.send(result);}).map_err(err)?;
 receive.await.map_err(err)??;Ok(json!({"format":1,"url":url,"complete_image":true}))
}
