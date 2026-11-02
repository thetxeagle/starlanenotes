//! Private bounded family census. Refusal retains bytes; this observation never grants unlink.
use super::*;
use std::{collections::BTreeSet,io::{Read,Seek,SeekFrom},os::fd::AsRawFd};
const MAX_ENTRIES:usize=8192;
const MAX_DEPTH:usize=12;
const NAMESPACES:&[&str]=&[".attachment-upload-admission",".attachment-upload-journal",".storage-journal",".publication-journal",".canonical-rewrite-journal",".move-relocation-journal",".restoration-custody"];
#[derive(Clone,PartialEq,Eq,Serialize)]struct Stamp(u64,u64,u64,u64,u32,i64,i64,i64,i64);
fn stamp(f:&File)->io::Result<Stamp>{let m=f.metadata()?;Ok(Stamp(m.dev(),m.ino(),m.len(),m.nlink(),m.mode(),m.mtime(),m.mtime_nsec(),m.ctime(),m.ctime_nsec()))}
fn names(f:&File)->io::Result<BTreeSet<String>>{let mut n=BTreeSet::new();for e in std::fs::read_dir(format!("/proc/self/fd/{}",f.as_raw_fd()))?{if n.len()>=MAX_ENTRIES{return Err(fail("Erasure directory census exceeds bound"))}let s=e?.file_name().into_string().map_err(|_|fail("NonUTF8 census name"))?;if s.is_empty()||s=="."||s==".."||s.contains('/')||s.contains('\\'){return Err(fail("Unsafe census component"))}n.insert(s);}Ok(n)}
struct Entry{relative:String,file:File,stamp:Stamp,names:Option<BTreeSet<String>>}
pub(super) struct FamilyCensus{selected_rewrite:Option<SelectedReplayRewrite>,designation:Option<crate::storage::data_root_designation::Designation>,qualified:Option<std::sync::Arc<crate::storage::restoration::QualifiedErasureSource>>,pub(super) guard:std::sync::Arc<crate::storage::native_family_fence::Creation>,entries:Vec<Entry>,leases:Vec<WriterLease>,root_path:PathBuf,root_stamp:Stamp,root_names:BTreeSet<String>,hashed:u64,metadata_bytes:u64,attribution:Option<std::collections::BTreeMap<String,(String,i64)>>,finalized_journals:BTreeSet<String>}
// One exclusive namespace/root observation and capsule roster. Only an exact
// held typed caller lease may be borrowed; every other producer must be idle.
pub(super) struct NativeRoster{guard:std::sync::Arc<crate::storage::native_family_fence::Creation>,root_path:PathBuf,root_stamp:Stamp,root_names:BTreeSet<String>,leases:std::collections::BTreeMap<String,WriterLease>,borrowed:BTreeSet<String>,qualified:Option<std::sync::Arc<crate::storage::restoration::QualifiedErasureSource>>}
impl NativeRoster{
 pub(super) fn capture(root:&Path,borrowed:&[WriterLease])->io::Result<Self>{
  Self::capture_qualified(root,borrowed,None)
 }
 pub(super) fn capture_qualified(root:&Path,borrowed:&[WriterLease],qualified:Option<std::sync::Arc<crate::storage::restoration::QualifiedErasureSource>>)->io::Result<Self>{
  if let Some(q)=&qualified{q.fence()?;}
  let guard=if let Some(held)=qualified.as_ref().and_then(|q|q.family()){held}else{std::sync::Arc::new(crate::storage::native_family_fence::Creation::exclusive(root)?)};
  let mut supplied=std::collections::BTreeMap::new();for lease in borrowed{lease.verify()?;lease.matches_root(root)?;if supplied.insert(lease.attempt_id().to_owned(),lease.clone()).is_some(){return Err(fail("Duplicate borrowed producer lease"));}}
  let borrowed=supplied.keys().cloned().collect();
  let root_stamp=stamp(guard.root())?;let root_names=names(guard.root())?;let mut leases=std::collections::BTreeMap::new();
  if root_names.contains(NS){let ns=stage::open(guard.root(),NS,libc::O_RDONLY|libc::O_DIRECTORY)?;for attempt in names(&ns)?{if qualified.as_ref().is_some_and(|q|q.covers_completed_attempt(&attempt)||q.preserves_composite_attempt(&attempt)){if supplied.contains_key(&attempt){return Err(fail("Copied completed capsule cannot be borrowed as native writer"));}continue;}let lease=if let Some(held)=supplied.remove(&attempt){held}else{WriterLease::open_idle(root,&attempt)?};lease.verify()?;leases.insert(attempt,lease);}}
  if !supplied.is_empty(){return Err(fail("Borrowed caller absent from original namespace"));}
  let held=Self{guard,root_path:root.to_owned(),root_stamp,root_names,leases,borrowed,qualified};held.fence()?;Ok(held)
 }
 pub(super) async fn fence_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()> {
  self.fence()?;if let Some(q)=&self.qualified{q.fence_in(conn).await?;}
  let sql:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id LIMIT 8193").fetch_all(&mut *conn).await.map_err(|_|fail("Native reservation census unavailable"))?;
  let mut expected=self.leases.keys().cloned().collect::<BTreeSet<_>>();if let Some(q)=&self.qualified{expected.extend(q.completed_attempts());expected.extend(q.composite_preserved_attempts());}
  if sql.len()>8192||sql.iter().any(|attempt|!expected.contains(attempt)){return Err(fail("SQL reservation lacks exact original or restored capsule custody"));}
  for(attempt,lease)in &self.leases{
   if !sql.contains(attempt){if !self.borrowed.contains(attempt)||lease.stage_directory()?.is_some(){return Err(fail("Native capsule lacks SQL admission before payload"));}continue;}
   let row=super::accounting::capture::reservation_in(conn,attempt).await.map_err(|_|fail("Native reservation unavailable"))?;
   if row.maximum_bytes<0||row.maximum_bytes as u64!=lease.maximum_bytes()||row.request_sha256!=lease.producer_sha256()||row.admission_claim_sha256!=lease.claim_sha256(){return Err(fail("Native reservation differs from original typed capsule"));}
   if self.qualified.as_ref().is_some_and(|q|q.completed().is_some()) {
    // Original current-root reserved producers remain typed native leases; no
    // committed Replay receipt is converted into a Direct receipt.
    match row.state.as_str(){
     "reserved"=>{if row.committed_asset_id.is_some()||row.committed_bytes.is_some()||row.disposition_json.is_some()||row.disposition_sha256.is_some(){return Err(fail("Mixed native reserved SQL shape differs"));}},
     "committed"=>{let Producer::Direct{request}=lease.producer() else{return Err(fail("Mixed restored committed Replay requires distinct receipt proof"))};let receipt=crate::storage::attachment_upload::receipt(conn,&request.operation_id).await.map_err(|_|fail("Mixed native winning receipt unavailable"))?.ok_or_else(||fail("Mixed native winning receipt absent"))?;
      if receipt.intent.request!=*request||receipt.intent.attempt_id!=*attempt||row.committed_asset_id.as_deref()!=Some(receipt.intent.asset_id.as_str())||row.committed_bytes!=Some(receipt.intent.byte_length)||row.disposition_json.is_some()||row.disposition_sha256.is_some(){return Err(fail("Mixed native exact winning receipt/SQL differs"));}},
     "disposed"=>{if row.committed_asset_id.is_some()||row.committed_bytes.is_some(){return Err(fail("Disposed native SQL carries committed association"));}let raw=row.disposition_json.as_deref().ok_or_else(||fail("Disposed native SQL intent absent"))?;let sha=row.disposition_sha256.as_deref().ok_or_else(||fail("Disposed native SQL digest absent"))?;lease.observe_native_disposed_in(conn,raw,sha).await?;},
     _=>return Err(fail("Mixed restored native terminal class remains retained review")),
    }
   }

  }self.fence()
 }
 pub(super) fn qualified(&self)->Option<std::sync::Arc<crate::storage::restoration::QualifiedErasureSource>>{self.qualified.clone()}
 pub(super) fn lease(&self,attempt:&str)->io::Result<WriterLease>{self.fence()?;self.leases.get(attempt).cloned().ok_or_else(||fail("Original capsule absent from held roster"))}
 pub(super) fn fence(&self)->io::Result<()>{let fresh=stage::root(&self.root_path)?;if stamp(&fresh)?!=self.root_stamp||stamp(self.guard.root())?!=self.root_stamp||names(&fresh)?!=self.root_names{return Err(fail("Held producer roster root changed"));}if self.root_names.contains(NS){let ns=stage::open(&fresh,NS,libc::O_RDONLY|libc::O_DIRECTORY)?;let mut expected=self.leases.keys().cloned().collect::<BTreeSet<_>>();if let Some(q)=&self.qualified{q.fence()?;expected.extend(q.completed_attempts());expected.extend(q.composite_preserved_attempts());}if names(&ns)?!=expected{return Err(fail("Held capsule roster membership changed"));}}for lease in self.leases.values(){lease.verify()?;}Ok(())}
}

// Private subject values come only from a validated original owner/producer plan.
// They are not serialized, inferred from filenames, or exposed as unlink grants.
struct Subject<'a>{lease:&'a WriterLease,payload:Option<&'a Identity>,length:u64,sha:Option<&'a str>,canonical:Option<&'a str>,asset:Option<&'a str>,path:Option<&'a str>,operation:Option<&'a str>,stage_payload:bool,selected_rewrite:Option<&'a SelectedReplayRewrite>}
impl FamilyCensus{
 pub(super) fn capture(owner:&OwnedErasure)->io::Result<Self>{Self::capture_inner(owner,None,BTreeSet::new())}
 pub(super) fn capture_attributed(owner:&OwnedErasure,paths:std::collections::BTreeMap<String,(String,i64)>)->io::Result<Self>{Self::capture_inner(owner,Some(paths),BTreeSet::new())}
 pub(super) fn capture_proved(owner:&OwnedErasure,paths:std::collections::BTreeMap<String,(String,i64)>,finalized_journals:BTreeSet<String>)->io::Result<Self>{Self::capture_inner(owner,Some(paths),finalized_journals)}
 fn capture_inner(owner:&OwnedErasure,attribution:Option<std::collections::BTreeMap<String,(String,i64)>>,finalized_journals:BTreeSet<String>)->io::Result<Self>{
  let roster=NativeRoster::capture(&owner.lease.0.root_path,&[owner.lease.clone()])?;
  Self::capture_roster(owner,attribution,finalized_journals,&roster)
 }
 pub(super) fn capture_roster(owner:&OwnedErasure,attribution:Option<std::collections::BTreeMap<String,(String,i64)>>,finalized_journals:BTreeSet<String>,roster:&NativeRoster)->io::Result<Self>{
  Self::capture_roster_qualified(owner,attribution,finalized_journals,roster,None)
 }
 pub(super) fn capture_qualified(owner:&OwnedErasure,paths:std::collections::BTreeMap<String,(String,i64)>,finalized_journals:BTreeSet<String>,qualified:Option<std::sync::Arc<crate::storage::restoration::QualifiedErasureSource>>)->io::Result<Self>{let roster=NativeRoster::capture_qualified(&owner.lease.0.root_path,&[owner.lease.clone()],qualified.clone())?;Self::capture_roster_qualified(owner,Some(paths),finalized_journals,&roster,qualified)}
 pub(super) fn capture_roster_qualified(owner:&OwnedErasure,attribution:Option<std::collections::BTreeMap<String,(String,i64)>>,finalized_journals:BTreeSet<String>,roster:&NativeRoster,qualified:Option<std::sync::Arc<crate::storage::restoration::QualifiedErasureSource>>)->io::Result<Self>{
  owner.verify()?;
  let selected:crate::storage::attachment_upload::Intent=decode(owner.intent.owned.envelopes.get("intent.json").ok_or_else(||fail("Original asset intent absent"))?.as_bytes())?;
  let p=&owner.intent.owned;let subject=Subject{lease:&owner.lease,payload:p.payload.as_ref(),length:p.payload_length,sha:p.payload_sha256.as_deref(),canonical:p.installed_path.as_deref(),asset:Some(&selected.asset_id),path:Some(&selected.storage_path),operation:Some(&selected.request.operation_id),stage_payload:true,selected_rewrite:None};
  Self::capture_subject_qualified(&subject,attribution,finalized_journals,roster,qualified)
 }
 pub(super) fn capture_pending_direct(lease:&WriterLease,plan:&Disposal,paths:std::collections::BTreeMap<String,(String,i64)>,finalized:BTreeSet<String>,roster:&NativeRoster)->io::Result<Self>{
  lease.verify()?;let Producer::Direct{request}=lease.producer()else{return Err(fail("Pending Direct census requires original producer"))};
  let selected=plan.envelopes.get("intent.json").map(|raw|decode::<crate::storage::attachment_upload::Intent>(raw.as_bytes())).transpose()?;
  let subject=Subject{lease,payload:plan.payload.as_ref(),length:plan.payload_length,sha:plan.payload_sha256.as_deref(),canonical:plan.installed_path.as_deref(),asset:selected.as_ref().map(|i|i.asset_id.as_str()),path:selected.as_ref().map(|i|i.storage_path.as_str()),operation:Some(&request.operation_id),stage_payload:true,selected_rewrite:None};
  Self::capture_subject_qualified(&subject,Some(paths),finalized,roster,roster.qualified())
 }
 pub(super) async fn capture_pending_replay(conn:&mut sqlx::AnyConnection,lease:&WriterLease,plan:&ReplayDisposal,paths:std::collections::BTreeMap<String,(String,i64)>,finalized:BTreeSet<String>,roster:&NativeRoster)->io::Result<Self>{
  lease.verify()?;let Producer::Replay{asset_id,..}=lease.producer()else{return Err(fail("Pending Replay census requires original producer"))};
  let origin=plan.origin_json.as_deref().map(|raw|decode::<ReplayBlobOrigin>(raw.as_bytes())).transpose()?;
  let selected_rewrite=SelectedReplayRewrite::capture_in(conn,lease,plan).await?;
  let subject=Subject{lease,payload:plan.payload.as_ref(),length:plan.payload_length,sha:plan.payload_sha256.as_deref(),canonical:origin.as_ref().map(|o|o.path.as_str()),asset:Some(asset_id),path:origin.as_ref().map(|o|o.path.as_str()),operation:None,stage_payload:false,selected_rewrite:selected_rewrite.as_ref()};
  let mut held=Self::capture_subject_qualified(&subject,Some(paths),finalized,roster,roster.qualified())?;held.selected_rewrite=selected_rewrite;held.fence()?;Ok(held)
 }
 fn capture_subject(subject:&Subject<'_>,attribution:Option<std::collections::BTreeMap<String,(String,i64)>>,finalized_journals:BTreeSet<String>,roster:&NativeRoster)->io::Result<Self>{
  Self::capture_subject_qualified(subject,attribution,finalized_journals,roster,None)
 }
 fn capture_subject_qualified(subject:&Subject<'_>,attribution:Option<std::collections::BTreeMap<String,(String,i64)>>,finalized_journals:BTreeSet<String>,roster:&NativeRoster,qualified:Option<std::sync::Arc<crate::storage::restoration::QualifiedErasureSource>>)->io::Result<Self>{
  if let Some(q)=&qualified{q.fence()?;}
  roster.fence()?;let guard=roster.guard.clone();
  if !stage::same(guard.root(),&subject.lease.0.root)?{return Err(fail("Census original root replaced"))}
  let root_names=names(guard.root())?;let root_stamp=stamp(guard.root())?;
  let designation=crate::storage::data_root_designation::Designation::capture_held(&subject.lease.0.root_path,guard.root())?;
  for name in &root_names {if (name.starts_with('.')&&(name.contains("journal")||name.contains("admission")||name.contains("restore")||name.starts_with(".starlane-")))&&!NAMESPACES.contains(&name.as_str())&&!qualified.as_ref().is_some_and(|q|q.covers_entry(name))&&!designation.as_ref().is_some_and(|d|d.covers_entry(name)){return Err(fail("Unknown native family or restoration marker retains bytes"))}}
  let mut held=Self{selected_rewrite:None,designation,qualified,guard,entries:vec![],leases:vec![],root_path:subject.lease.0.root_path.clone(),root_stamp,root_names,hashed:0,metadata_bytes:0,attribution,finalized_journals};
  for (attempt,lease) in &roster.leases {if attempt==subject.lease.attempt_id(){continue}
   match lease.producer(){Producer::Replay{asset_id,..}if subject.asset.is_some_and(|selected|asset_id.eq_ignore_ascii_case(selected))=>return Err(fail("Selected Replay claim retains asset even without payload")),Producer::Direct{request}if subject.operation==Some(request.operation_id.as_str())=>return Err(fail("Another original request attempt requires retained review")),_=>()}
   held.leases.push(lease.clone());
  }
  if held.root_names.contains(stage::NAMESPACE){let ns=stage::open(held.guard.root(),stage::NAMESPACE,libc::O_RDONLY|libc::O_DIRECTORY)?;
   let mut supported:BTreeSet<String>=held.leases.iter().map(|lease|lease.attempt_id().to_owned()).chain(std::iter::once(subject.lease.attempt_id().to_owned())).collect();
   if let Some(q)=&held.qualified{supported.extend(q.completed_attempts());supported.extend(q.composite_preserved_attempts());}
   if !names(&ns)?.is_subset(&supported){return Err(fail("Upload stage without original admission custody retains bytes"))}
  }
  let mut traversed=NAMESPACES.iter().map(|s|(*s).to_owned()).chain(std::iter::once("spaces".into())).collect::<BTreeSet<_>>();if let Some(q)=&held.qualified{traversed.extend(held.root_names.iter().filter(|n|q.covers_entry(n)).cloned());}
  for name in traversed {if !held.root_names.contains(&name){continue}
   let f=stage::open(held.guard.root(),&name,libc::O_RDONLY|libc::O_NONBLOCK)?;
   if name==".canonical-rewrite-journal"{if let Some(selected)=subject.selected_rewrite{if !selected.covers_namespace(&f)?{return Err(fail("Selected Replay rewrite namespace has unproved siblings"));}}}
   if matches!(name.as_str(),".move-relocation-journal"|".canonical-rewrite-journal"|".restoration-custody")&&!names(&f)?.is_empty()&&!held.qualified.as_ref().is_some_and(|q|q.covers_entry(&name)){
    let selected=name==".canonical-rewrite-journal"&&subject.selected_rewrite.map(|proof|proof.covers_namespace(&f)).transpose()?.unwrap_or(false);
    if !selected{return Err(fail("Unsupported move/rewrite/restoration family retains asset"))}
   }
   held.walk(name,f,0,subject)?;
  }
  held.fence()?;Ok(held)
 }
 fn walk(&mut self,relative:String,file:File,depth:usize,subject:&Subject<'_>)->io::Result<()>{
  if depth>MAX_DEPTH||self.entries.len()>=MAX_ENTRIES{return Err(fail("Native family census exceeds supported bound"))}
  let before=stamp(&file)?;let m=file.metadata()?;
  if m.is_dir(){let members=names(&file)?;self.entries.push(Entry{relative:relative.clone(),file:file.try_clone()?,stamp:before.clone(),names:Some(members.clone())});
   for name in members {let child=stage::open(&file,&name,libc::O_RDONLY|libc::O_NONBLOCK)?;self.walk(format!("{relative}/{name}"),child,depth+1,subject)?;}
  }else if m.is_file(){
   if let Some(q)=&self.qualified{q.exclude_old_payload(&relative,&file,self.designation.as_ref())?;}
   if relative.starts_with("spaces/")&&relative.split('/').nth(2)==Some("attachments") {
    if let Some(paths)=&self.attribution{let(id,length)=paths.get(&relative).ok_or_else(||fail("Unattributed managed attachment retains bytes"))?;
     if *length<0||m.len()!=*length as u64{return Err(fail("Managed attachment differs from retained evidence"))}
     if subject.asset==Some(id.as_str())&&subject.path!=Some(relative.as_str()){return Err(fail("Additional selected asset path retains bytes"))}
    }
   }
   let stage_path=format!("{}/{}/payload.part",stage::NAMESPACE,subject.lease.attempt_id());
   if subject.payload.is_some_and(|p|m.dev()==p.device&&m.ino()==p.inode) {
    if !(subject.stage_payload&&relative==stage_path)&&subject.canonical!=Some(relative.as_str()){return Err(fail("Unreviewed original inode alias retains bytes"))}
   }else if subject.payload.is_some()&&m.len()==subject.length{
    self.hashed=self.hashed.checked_add(m.len()).ok_or_else(||fail("Census hash overflow"))?;if self.hashed>512*1024*1024{return Err(fail("Census binary hash work exceeds bound"))}
    if Some(crate::storage::attachment_upload::hash(&file,m.len())?.as_str())==subject.sha{return Err(fail("Independent same-content native copy requires custody review"))}
   }
   let original_prefixes=[format!("{}/{}/",stage::NAMESPACE,subject.lease.attempt_id()),format!("{}/{}/",NS,subject.lease.attempt_id())];
   if subject.asset.is_some()&&relative.ends_with(".json")&&!original_prefixes.iter().any(|prefix|relative.starts_with(prefix)){
    self.metadata_bytes=self.metadata_bytes.checked_add(m.len()).ok_or_else(||fail("Census metadata overflow"))?;let bound=if self.qualified.as_ref().map(|q|q.owns_control_record(&relative,&file)).transpose()?.unwrap_or(false){8*1024*1024}else{65536};if m.len()>bound||self.metadata_bytes>8*1024*1024{return Err(fail("Opaque native metadata exceeds census bound"))}
    let mut raw=vec![];let mut copy=file.try_clone()?;copy.seek(SeekFrom::Start(0))?;copy.take(bound+1).read_to_end(&mut raw)?;
    let value:serde_json::Value=decode(&raw)?;
    let owned_control=self.qualified.as_ref().map(|q|q.exclude_control_references(&relative,&file,subject.asset.unwrap(),subject.path.unwrap_or("\0pending-no-public-path"))).transpose()?.unwrap_or(false);
    let owned_selected_rewrite=subject.selected_rewrite.map(|proof|proof.matches_manifest(&relative,&file)).transpose()?.unwrap_or(false);
    if !owned_control&&!owned_selected_rewrite{crate::storage::attachment_erasure_source::exclude(&serde_json::to_string(&value).map_err(|_|fail("Native metadata normalization failed"))?,subject.asset.unwrap(),subject.path.unwrap_or("\0pending-no-public-path")).map_err(|_|fail("Selected asset native metadata requires retained review"))?;}
   }
   // A pending document/publication journal may hold a future pin in its
   // staged body rather than JSON metadata. Opaque bytes are unsupported here.
   if subject.asset.is_some()&&[".storage-journal/",".publication-journal/"].iter().any(|prefix|relative.starts_with(prefix))&&!relative.ends_with(".json")&&!self.finalized_journals.iter().any(|prefix|relative.starts_with(prefix)){
    self.metadata_bytes=self.metadata_bytes.checked_add(m.len()).ok_or_else(||fail("Staged source census overflow"))?;
    if m.len()>8*1024*1024||self.metadata_bytes>8*1024*1024{return Err(fail("Staged source census exceeds bound"))}
    let mut raw=vec![];let mut copy=file.try_clone()?;copy.seek(SeekFrom::Start(0))?;copy.take(8*1024*1024+1).read_to_end(&mut raw)?;
    let source=std::str::from_utf8(&raw).map_err(|_|fail("Opaque staged native source retains review"))?;
    crate::storage::attachment_erasure_source::exclude(source,subject.asset.unwrap(),subject.path.unwrap_or("\0pending-no-public-path")).map_err(|error|fail(&error))?;
   }
   self.entries.push(Entry{relative,file:file.try_clone()?,stamp:before.clone(),names:None});
  }else{return Err(fail("Foreign nonregular native object retains bytes"))}
  if stamp(&file)?!=before{return Err(fail("Native family changed during capture"))}Ok(())
 }
 pub(super) fn observation_sha256(&self)->io::Result<String>{
  self.fence()?;let entries:Vec<_>=self.entries.iter().map(|e|serde_json::json!({"relative":e.relative,"stamp":e.stamp,"members":e.names})).collect();
  let leases:Vec<_>=self.leases.iter().map(|lease|serde_json::json!({"attempt":lease.attempt_id(),"claim_sha256":lease.claim_sha256()})).collect();
  let raw=serde_json::to_vec(&serde_json::json!({"version":1,"root":self.root_stamp,"root_members":self.root_names,"entries":entries,"idle_original_claims":leases})).map_err(io::Error::other)?;
  Ok(digest(&raw))
 }
 pub(super) async fn fence_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()>{if let Some(q)=&self.qualified{q.fence_in(conn).await?;}self.fence()}
 pub(super) fn fence(&self)->io::Result<()>{if let Some(r)=&self.selected_rewrite{r.fence()?;}if let Some(d)=&self.designation{d.fence()?;}if let Some(q)=&self.qualified{q.fence()?;}let root=stage::root(&self.root_path)?;if stamp(&root)?!=self.root_stamp||stamp(self.guard.root())?!=self.root_stamp||names(&root)?!=self.root_names{return Err(fail("Erasure family root membership changed"))}
  for e in &self.entries{let mut f=root.try_clone()?;for name in e.relative.split('/'){f=stage::open(&f,name,libc::O_RDONLY|libc::O_NONBLOCK)?;}if stamp(&f)?!=e.stamp||stamp(&e.file)?!=e.stamp||match &e.names{Some(n)=>names(&f)?!=*n,None=>false}{return Err(fail("Native family changed before owned mutation"))}}
  for lease in &self.leases{lease.verify()?}Ok(())
 }
}

#[path="attachment_pending_rewrite_census.rs"] mod pending_rewrite;
use pending_rewrite::SelectedReplayRewrite;
