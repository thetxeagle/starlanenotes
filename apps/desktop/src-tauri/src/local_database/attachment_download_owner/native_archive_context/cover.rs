//! Exact current local cover selection -> new imported creator -> held GIO stream.
//! Source creator JSON remains history; it never opens an original local cover.
use super::*;
use std::{collections::HashMap,fs::File,io::Read,sync::{Mutex,OnceLock},time::{Duration,Instant}};
use webkit2gtk::{gio,glib,WebContextExt,URISchemeRequestExt,WebViewExt,SecurityManagerExt};
use glib::prelude::*;
#[derive(Clone,Deserialize)]#[serde(deny_unknown_fields)]
pub(crate) struct CoverSelection{space_id:String,document_id:String,asset_id:String,draft_id:Option<String>,cover_reference:String}
impl Current{
 async fn cover(self,current:Value,selected:CoverSelection,expected:Value,stream:bool)->Result<(Option<File>,u64,String),String>{
  if current!=self.context||[&selected.space_id,&selected.document_id,&selected.asset_id].iter().any(|s|s.is_empty()||s.len()>256)||selected.cover_reference.is_empty()||selected.cover_reference.len()>65536{return Err("Exact current imported cover selection required".into())}self.fence()?;
  connection::supervise(async move{
   let _guard=snapshot_commit_lock().lock().await;self.fence()?;let mut db=connection::Database::prepare(self.database.clone(),&self.main).await?;let physical=db.fence_owner();
   let result=async{
    let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;native_archive_capture::strict_preflight(&mut tx).await?;
    let snapshot=read_snapshot_from_connection(&mut tx,&self.provider).await?.ok_or("Current imported cover source unavailable")?;
    let binding=json!({"context":self.context,"revision":snapshot["canonical_revision"],"root":self.root.path});if binding!=expected{return Err("Exact selected imported cover revision changed".into())}
    let page=snapshot["documents"].as_array().and_then(|rows|rows.iter().find(|p|p["id"]==selected.document_id&&p["space_id"]==selected.space_id)).ok_or("Current imported cover page missing")?;
    let source=if let Some(draft)=&selected.draft_id{snapshot["drafts"].as_array().and_then(|rows|rows.iter().find(|d|d["id"]==*draft&&d["document_id"]==selected.document_id&&d["space_id"]==selected.space_id&&d["owner_id"]=="local-user")).ok_or("Selected owned imported cover draft missing")?}else{page};
    if source["cover_image"]!=selected.cover_reference{return Err("Selected imported cover pin differs".into())}
    let asset=snapshot["assets"].as_array().and_then(|rows|rows.iter().find(|a|a["id"]==selected.asset_id&&a["space_id"]==selected.space_id&&a["document_id"]==selected.document_id&&a["url"]==selected.cover_reference)).ok_or("Exact imported cover asset missing")?.clone();
    let mime=asset["mime_type"].as_str().filter(|m|["image/png","image/jpeg","image/gif","image/webp"].contains(m)).ok_or("Supported imported cover image required")?.to_string();
    let length=asset["size"].as_u64().filter(|n|*n>0&&*n<=30*1024*1024).ok_or("Imported cover exceeds bound")?;
    let (total,count):(i64,i64)=sqlx::query_as("SELECT COALESCE(SUM(length(CAST(i.request_json AS BLOB))+length(CAST(r.result_json AS BLOB))),0),COUNT(*) FROM native_archive_import_intents i JOIN native_archive_import_receipts r ON r.operation_id=i.operation_id AND r.request_fingerprint=i.request_fingerprint").fetch_one(&mut *tx).await.map_err(err)?;
    let receipts:i64=sqlx::query_scalar("SELECT COUNT(*) FROM native_archive_import_receipts").fetch_one(&mut *tx).await.map_err(err)?;
    if total>64*1024*1024||count>4096||count!=receipts{return Err("Imported cover creator evidence exceeds bound or has orphan".into())}
    let instance:String=sqlx::query_scalar("SELECT instance_id FROM native_archive_instance WHERE singleton=1").fetch_one(&mut *tx).await.map_err(err)?;
    let rows:Vec<(String,String,String,String,String,String,String)>=sqlx::query_as("SELECT i.operation_id,i.request_fingerprint,i.provider,i.instance_id,i.request_json,r.result_json,r.result_digest FROM native_archive_import_intents i JOIN native_archive_import_receipts r ON r.operation_id=i.operation_id AND r.request_fingerprint=i.request_fingerprint ORDER BY i.operation_id").fetch_all(&mut *tx).await.map_err(err)?;
    let owned=native_archive_import::capture_evidence(self.selection.clone(),&self.provider,self.context.clone(),&snapshot,&instance,&rows,&asset)?;owned.reader.fence()?;
    let file=if stream{let(file,actual)=owned.reader.image_stream()?;if actual!=length{return Err("Imported image native length differs".into())}Some(file)}else{None};
    owned.reader.fence()?;self.fence()?;physical.check()?;tx.rollback().await.map_err(err)?;Ok((file,length,mime))
   }.await;db.retire().await.map_err(|e|format!("Imported cover source retirement failed: {e}"))?;result
  }).await
 }
}
const SCHEME:&str="starlane-imported-cover-preview";
struct Ticket{owner:Current,current:Value,selected:CoverSelection,binding:Value,view:usize,created:Instant}
fn tickets()->&'static Mutex<HashMap<String,Ticket>>{static T:OnceLock<Mutex<HashMap<String,Ticket>>>=OnceLock::new();T.get_or_init(||Mutex::new(HashMap::new()))}
thread_local!{static CONTEXTS:std::cell::RefCell<HashMap<usize,glib::WeakRef<webkit2gtk::WebContext>>>=std::cell::RefCell::new(HashMap::new());}
fn fail(request:&webkit2gtk::URISchemeRequest){request.finish_error(&mut glib::Error::new(gio::IOErrorEnum::PermissionDenied,"Exact selected imported cover unavailable"));}
fn register(context:&webkit2gtk::WebContext){
 let fresh=CONTEXTS.with(|s|{let mut rows=s.borrow_mut();rows.retain(|_,v|v.upgrade().is_some());let id=context.as_ptr()as usize;if rows.contains_key(&id){false}else{rows.insert(id,context.downgrade());true}});if !fresh{return}
 if let Some(security)=context.security_manager(){security.register_uri_scheme_as_secure(SCHEME);security.register_uri_scheme_as_no_access(SCHEME);}
 context.register_uri_scheme(SCHEME,|request|{
  let Some(uri)=request.uri()else{fail(request);return};let Some(token)=uri.strip_prefix("starlane-imported-cover-preview://image/")else{fail(request);return};
  if token.len()!=64||!token.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){fail(request);return}
  let Some(view)=request.web_view()else{fail(request);return};
  let ticket={let Ok(mut rows)=tickets().lock()else{fail(request);return};let Some(row)=rows.get(token)else{fail(request);return};if row.created.elapsed()>Duration::from_secs(120)||row.view!=view.as_ptr()as usize{fail(request);return}rows.remove(token).unwrap()};
  let request=request.clone();glib::MainContext::default().spawn_local(async move{
   let result=tauri::async_runtime::spawn(async move{ticket.owner.cover(ticket.current,ticket.selected,ticket.binding,true).await}).await;
   match result{Ok(Ok((Some(file),length,mime)))=>{let stream=unsafe{gio::UnixInputStream::take_fd(file)};request.finish(&stream,length as i64,Some(&mime));},_=>fail(&request)}
  });
 });
}
#[tauri::command]
pub(crate) async fn open_current_imported_native_cover_image<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,provider:String,context:Value,current:Value,selection:CoverSelection,original_binding:Value)->Result<Value,String>{
 let owner=Current::capture(vault::capture_snapshot_request(&app,&provider)?,&provider,context)?;
 owner.clone().cover(current.clone(),selection.clone(),original_binding.clone(),false).await?;
 let mut random=[0;32];File::open("/dev/urandom").map_err(err)?.read_exact(&mut random).map_err(err)?;let token:String=random.iter().map(|b|format!("{b:02x}")).collect();let url=format!("{SCHEME}://image/{token}");let(send,receive)=tokio::sync::oneshot::channel::<Result<(),String>>();
 window.with_webview(move|platform|{let view=platform.inner();let result=(||{register(&view.context().ok_or("Native WebKit context unavailable")?);let mut rows=tickets().lock().map_err(err)?;rows.retain(|_,r|r.created.elapsed()<Duration::from_secs(120));if rows.len()>=8||rows.contains_key(&token){return Err("Imported cover preview registry unavailable".into())}rows.insert(token,Ticket{owner,current,selected:selection,binding:original_binding,view:view.as_ptr()as usize,created:Instant::now()});Ok(())})();let _=send.send(result);}).map_err(err)?;
 receive.await.map_err(err)??;Ok(json!({"format":1,"url":url,"complete_image":true}))
}

#[cfg(test)]
pub(crate) async fn test_stream(selection:vault::SnapshotRequestBinding,context:Value,current:Value,binding:Value,selected:Value)->Result<(File,u64,String),String>{
 let result=Current::capture(selection,"none",context)?.cover(current,serde_json::from_value(selected).map_err(err)?,binding,true).await?;
 Ok((result.0.ok_or("Actual imported image stream missing")?,result.1,result.2))
}
