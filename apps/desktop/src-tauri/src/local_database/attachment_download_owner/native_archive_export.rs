//! Whole-archive selection authority, distinct from attachment delivery tokens.
//! Registered current selected export; historical observations never grant old creator authority.
use super::*;
use std::{collections::BTreeMap,io::Read,sync::Mutex};
use tauri::Manager;
use native_archive_capture::Owner;
use native_archive_output::Selected;
struct Pending{window:String,owner:Owner,selected:Selected}
#[derive(Default)]
pub(crate) struct Selections(Mutex<BTreeMap<String,Pending>>);
fn err(e:impl ToString)->String{e.to_string()}
fn token()->Result<String,String>{let mut bytes=[0u8;32];std::fs::File::open("/dev/urandom").map_err(err)?.read_exact(&mut bytes).map_err(err)?;Ok(bytes.iter().map(|n|format!("{n:02x}")).collect())}
fn original(selection:vault::SnapshotRequestBinding,provider:&str,context:Value,binding:&Value)->Result<Owner,String>{
 if context["accountId"]!="local-user"||binding.as_object().map(|o|o.len())!=Some(3)||binding["context"]!=context{return Err("Original actual local archive binding required".into())}
 let owner=Owner::capture(selection,provider,binding["revision"].as_str().ok_or("Original archive revision missing")?,context)?;
 if owner.binding()!=*binding{return Err("Original canonical archive source differs".into())}Ok(owner)
}
impl Selections{
 fn remember(&self,window:&str,owner:Owner,selected:Selected)->Result<Value,String>{
  owner.fence()?;let report=selected.report();if report["binding"]!=owner.binding(){return Err("Archive destination/source differs".into())}
  let mut records=self.0.lock().map_err(err)?;if records.len()>=32{return Err("Whole archive chooser selection limit reached; discard prior selections".into())}
  let token=token()?;if records.contains_key(&token){return Err("Whole archive token collision refuses".into())}
  records.insert(token.clone(),Pending{window:window.into(),owner,selected});Ok(json!({"format":1,"selection_token":token,"destination":report}))
 }
 fn take(&self,window:&str,token:&str,current:&Value)->Result<Pending,String>{
  if token.len()!=64||!token.bytes().all(|b|b.is_ascii_hexdigit()){return Err("Exact whole archive selection token required".into())}
  let mut records=self.0.lock().map_err(err)?;let record=records.get(token).ok_or("Original whole archive selection unavailable; never automatically retry publication")?;
  if record.window!=window||record.owner.binding()["context"]!=*current{return Err("Original whole archive window/session differs".into())}
  // Consume before any filesystem/SQL publication work: errors never regrant authority.
  records.remove(token).ok_or("Original whole archive selection missing".into())
 }
}
#[tauri::command]
pub(crate) async fn choose_complete_native_vault_archive<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,provider:String,context:Value,current:Value,original_binding:Value,prior_parent_path:Option<String>,prior_report:Option<Value>)->Result<Option<Value>,String>{
 if app.try_state::<Selections>().is_none(){return Err("Complete native archive owner unavailable".into())}
 if current!=context{return Err("Original local archive session changed".into())}
 let owner=original(vault::capture_snapshot_request(&app,&provider)?,&provider,context,&original_binding)?;
 let prior=match(prior_parent_path,prior_report){(None,None)=>None,(Some(parent),Some(report))=>Some(native_archive_output::recovery::SealedOriginal::capture(Path::new(&parent),&original_binding,&report)?),_=>return Err("Exact original sealed observation pair required".into())};
 // Reservation validation before opening the real chooser; retain this revision, never rebase.
 owner.clone().with_capture(current,|_|Ok(())).await?;
 let guard=owner.clone();let selected=Selected::choose(app.clone(),original_binding,move||{guard.fence()?;Ok(guard.binding())}).await?;
 selected.map(|selected|{let selected=if let Some(prior)=prior{selected.with_prior(prior)?}else{selected};app.try_state::<Selections>().ok_or("Complete native archive owner unavailable")?.remember(window.label(),owner,selected)}).transpose()
}
#[tauri::command]
pub(crate) async fn commit_complete_native_vault_archive<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,selection_token:String,current:Value)->Result<Value,String>{
 let pending=app.try_state::<Selections>().ok_or("Complete native archive owner unavailable")?.take(window.label(),&selection_token,&current)?;
 let guard=pending.owner.clone();let binding=guard.binding();
 pending.owner.with_capture(current,move|capture|{
  let result=native_archive_output::compose(capture,pending.selected,||{guard.fence()?;Ok(guard.binding())})?;
  Ok(json!({"format":1,"binding":binding,"publication":result}))
 }).await
}
#[tauri::command]
pub(crate) fn discard_complete_native_vault_archive<R:tauri::Runtime>(app:AppHandle<R>,window:tauri::WebviewWindow<R>,selection_token:String,current:Value)->Result<Value,String>{
 let _held=app.try_state::<Selections>().ok_or("Complete native archive owner unavailable")?.take(window.label(),&selection_token,&current)?;
 // Descriptor release only. Never unlink destination/staging or grant retry authority.
 Ok(json!({"format":1,"discarded":true,"publication_authority":false,"unlink_authority":false}))
}
#[cfg(test)]mod tests;
