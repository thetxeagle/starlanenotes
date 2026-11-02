//! Unregistered whole-vault destination authority. Existing output always refuses.
use super::{filesystem::{Directory,Identity,identity},native_archive_capture::ArchiveCapture};
use serde::{Serialize,Deserialize};
use serde_json::{Value,json};
use sha2::{Sha256,Digest};
use std::{fs::File,io::{Read,Write,Seek,SeekFrom},path::{Path,PathBuf},os::{fd::AsRawFd,unix::fs::MetadataExt}};
use tauri_plugin_dialog::DialogExt;
fn random_operation()->Result<String,String>{let mut b=[0u8;16];File::open("/dev/urandom").map_err(err)?.read_exact(&mut b).map_err(err)?;b[6]=(b[6]&15)|64;b[8]=(b[8]&63)|128;let h=b.iter().map(|v|format!("{v:02x}")).collect::<String>();Ok(format!("{}-{}-{}-{}-{}",&h[..8],&h[8..12],&h[12..16],&h[16..20],&h[20..]))}
fn err(e:impl ToString)->String{e.to_string()}
fn sha(bytes:&[u8])->String{format!("{:x}",Sha256::digest(bytes))}
fn file_sha(file:&mut File)->Result<String,String>{file.seek(SeekFrom::Start(0)).map_err(err)?;let mut h=Sha256::new();let mut b=[0u8;65536];loop{let n=file.read(&mut b).map_err(err)?;if n==0{break}h.update(&b[..n]);}Ok(format!("{:x}",h.finalize()))}
fn regular(file:&File,links:u64)->Result<(),String>{let m=file.metadata().map_err(err)?;if !m.is_file()||m.nlink()!=links{return Err("Archive output independent regular file proof failed".into())}Ok(())}
fn named(parent:&Directory,leaf:&str)->Result<Option<(Identity,u64,u64)>,String>{
 let name=std::ffi::CString::new(leaf).map_err(err)?;let mut st=std::mem::MaybeUninit::<libc::stat>::uninit();
 if unsafe{libc::fstatat(parent.held.as_raw_fd(),name.as_ptr(),st.as_mut_ptr(),libc::AT_SYMLINK_NOFOLLOW)}!=0{let e=std::io::Error::last_os_error();if e.kind()==std::io::ErrorKind::NotFound{return Ok(None)}return Err(err(e))}
 let st=unsafe{st.assume_init()};if st.st_mode&libc::S_IFMT!=libc::S_IFREG{return Err("Archive leaf is not regular".into())}Ok(Some((Identity{device:st.st_dev,inode:st.st_ino},st.st_size as u64,st.st_nlink)))
}
/// No public path constructor: production authority comes only from actual dialog.
pub(crate) struct Selected {parent:Directory,leaf:String,binding:Value,prior:Option<recovery::SealedOriginal>}
impl Selected {
 pub(super) fn report(&self)->Value{let mut report=json!({"parent_path":self.parent.path,"leaf":self.leaf,"binding":self.binding});if let Some(prior)=&self.prior{report["reexport_provenance"]=json!({"format":1,"original_operation":prior.report()["operation"],"original_archive_sha256":prior.report()["stage"]["sha256"],"byte_length":prior.report()["stage"]["length"],"new_operation_required":true,"original_creator_authority":false,"publication_retry":false});}report}
 fn admit(path:PathBuf,binding:Value)->Result<Self,String>{
  let parent=Directory::capture(path.parent().ok_or("Archive destination parent missing")?).map_err(err)?;
  let leaf=path.file_name().and_then(|n|n.to_str()).filter(|s|!s.is_empty()).ok_or("Archive destination name invalid")?.to_owned();
  let root=Path::new(binding["root"].as_str().ok_or("Archive root binding missing")?);if parent.path.starts_with(root){return Err("Archive destination must be outside vault".into())}
  let original=Directory::capture(root).map_err(err)?;let mut protected=vec![identity(&original.held).map_err(err)?];for leaf in [".starlane","assets","spaces"]{match original.child(leaf,false){Ok(d)=>protected.push(identity(&d.held).map_err(err)?),Err(e)if e.kind()==std::io::ErrorKind::NotFound=>{},Err(e)=>return Err(err(e))}}
  for ancestor in parent.path.ancestors(){let dir=Directory::capture(ancestor).map_err(err)?;if protected.contains(&identity(&dir.held).map_err(err)?){return Err("Archive destination aliases vault storage".into())}}
  if named(&parent,&leaf)?.is_some(){return Err("Existing archive destination refuses; choose a new name".into())}
  recovery::require_clear(&parent,&leaf)?;Ok(Self{parent,leaf,binding,prior:None})
 }
 pub async fn choose<R:tauri::Runtime>(app:tauri::AppHandle<R>,binding:Value,current:impl Fn()->Result<Value,String>+Send+'static)->Result<Option<Self>,String>{
  let original=binding.clone();let selected=tokio::task::spawn_blocking(move||app.dialog().file().set_title("Export complete vault to a new archive").blocking_save_file()).await.map_err(err)?;
  if current()?!=original{return Err("Original archive selection changed while chooser open".into())}
  selected.map(|p|p.into_path().map_err(err).and_then(|p|Self::admit(p,binding))).transpose()
 }
 /// Attach read-only original sealed-byte provenance to a NEW genuine selection.
 /// The report never grants destination, publication, source, or old creator authority.
 pub(super) fn with_prior(mut self,prior:recovery::SealedOriginal)->Result<Self,String>{prior.fence()?;if prior.binding()!=&self.binding{return Err("Current re-export source differs from original sealed bytes".into())}self.prior=Some(prior);Ok(self)}
 fn fence(&self,binding:&Value)->Result<(),String>{if binding!=&self.binding{return Err("Original archive context/revision changed".into())}self.parent.fence().map_err(err)?;Ok(())}
}
#[derive(Serialize,Deserialize,Clone)]
#[serde(deny_unknown_fields)]
struct Journal {format:u8,operation:String,state:String,binding:Value,parent:Identity,leaf:String,stage:Identity,length:u64,sha256:String,manifest_sha256:String}
struct Staged {selected:Selected,dir:Directory,file:File,journal:Journal}
impl Staged {
 fn create(selected:Selected,manifest_sha256:String)->Result<Self,String>{
  selected.fence(&selected.binding)?;if named(&selected.parent,&selected.leaf)?.is_some(){return Err("Archive destination now exists".into())}
  let operation=random_operation()?;let name=format!(".starlane-archive-{operation}");let c=std::ffi::CString::new(name.as_str()).map_err(err)?;
  if unsafe{libc::mkdirat(selected.parent.held.as_raw_fd(),c.as_ptr(),0o700)}!=0{return Err(err(std::io::Error::last_os_error()))}selected.parent.held.sync_all().map_err(err)?;
  let dir=selected.parent.child(&name,false).map_err(err)?;let file=dir.open("archive.zip",libc::O_RDWR|libc::O_CREAT|libc::O_EXCL).map_err(err)?;regular(&file,1)?;
  let journal=Journal{format:1,operation,state:"staging".into(),binding:selected.binding.clone(),parent:identity(&selected.parent.held).map_err(err)?,leaf:selected.leaf.clone(),stage:identity(&file).map_err(err)?,length:0,sha256:String::new(),manifest_sha256};
  let stage=Self{selected,dir,file,journal};stage.record()?;Ok(stage)
 }
 fn record(&self)->Result<(),String>{
  self.dir.fence().map_err(err)?;let raw=serde_json::to_vec(&self.journal).map_err(err)?;if raw.len()>65536{return Err("Archive journal budget exceeded".into())}
  let temp=format!("journal-{}.tmp",random_operation()?);let mut f=self.dir.open(&temp,libc::O_WRONLY|libc::O_CREAT|libc::O_EXCL).map_err(err)?;f.write_all(&raw).map_err(err)?;f.sync_all().map_err(err)?;
  let from=std::ffi::CString::new(temp).map_err(err)?;if unsafe{libc::renameat(self.dir.held.as_raw_fd(),from.as_ptr(),self.dir.held.as_raw_fd(),c"journal.json".as_ptr())}!=0{return Err(err(std::io::Error::last_os_error()))}self.dir.held.sync_all().map_err(err)?;self.dir.fence().map_err(err)
 }
 fn seal(&mut self)->Result<(),String>{self.file.sync_all().map_err(err)?;regular(&self.file,1)?;self.journal.length=self.file.metadata().map_err(err)?.len();self.journal.sha256=file_sha(&mut self.file)?;if let Some(prior)=&self.selected.prior{prior.fence()?;prior.matches(self.journal.length,&self.journal.sha256)?;let mut f=self.dir.open("reexport-provenance.json",libc::O_WRONLY|libc::O_CREAT|libc::O_EXCL).map_err(err)?;let raw=serde_json::to_vec(&json!({"format":1,"new_operation":self.journal.operation,"original_report":prior.report(),"original_creator_authority":false,"publication_retry":false})).map_err(err)?;if raw.len()>65536{return Err("Current re-export provenance exceeds bound".into())}f.write_all(&raw).map_err(err)?;f.sync_all().map_err(err)?;self.dir.held.sync_all().map_err(err)?;}self.journal.state="sealed".into();self.record()}
 fn stage_fence(&mut self)->Result<(),String>{self.dir.fence().map_err(err)?;regular(&self.file,1)?;if named(&self.dir,"archive.zip")?!=Some((self.journal.stage,self.journal.length,1))||identity(&self.file).map_err(err)?!=self.journal.stage||file_sha(&mut self.file)?!=self.journal.sha256{return Err("Original staged archive changed".into())}Ok(())}
 fn publish(mut self,source:impl Fn()->Result<(),String>,current:impl Fn()->Result<Value,String>,hook:impl Fn(&str)->Result<(),String>)->Result<Value,String>{
  hook("sealed")?;self.stage_fence()?;self.selected.fence(&current()?)?;if named(&self.selected.parent,&self.selected.leaf)?.is_some(){return Err("Archive destination appeared; no overwrite".into())}
  self.journal.state="publication_unknown".into();self.record()?;hook("journal")?;
  self.stage_fence()?;source()?;self.selected.fence(&current()?)?;if let Some(prior)=&self.selected.prior{prior.fence()?;prior.matches(self.journal.length,&self.journal.sha256)?;}self.dir.fence().map_err(err)?;
  let to=std::ffi::CString::new(self.selected.leaf.as_str()).map_err(err)?;
  if unsafe{libc::linkat(self.dir.held.as_raw_fd(),c"archive.zip".as_ptr(),self.selected.parent.held.as_raw_fd(),to.as_ptr(),0)}!=0{return Err(format!("Archive publication unresolved; retained operation {}: {}",self.journal.operation,std::io::Error::last_os_error()))}
  hook("link")?;self.selected.parent.held.sync_all().map_err(err)?;hook("fsync")?;
  self.selected.parent.fence().map_err(err)?;self.dir.fence().map_err(err)?;
  if named(&self.selected.parent,&self.selected.leaf)?!=Some((self.journal.stage,self.journal.length,2))||named(&self.dir,"archive.zip")?!=Some((self.journal.stage,self.journal.length,2)){return Err("Archive publication unknown; named evidence differs".into())}
  self.file.sync_all().map_err(err)?;regular(&self.file,2)?;
  if identity(&self.file).map_err(err)?!=self.journal.stage||file_sha(&mut self.file)?!=self.journal.sha256{return Err("Archive publication unknown; selected output complete-byte readback differs".into())}
  self.selected.parent.fence().map_err(err)?;self.dir.fence().map_err(err)?;
  if named(&self.selected.parent,&self.selected.leaf)?!=Some((self.journal.stage,self.journal.length,2))||named(&self.dir,"archive.zip")?!=Some((self.journal.stage,self.journal.length,2)){return Err("Archive publication unknown; selected output changed during readback".into())}
  self.journal.state="published".into();self.record()?;Ok(json!({"format":1,"operation":self.journal.operation,"state":"published","path":self.selected.parent.path.join(&self.selected.leaf),"byte_length":self.journal.length,"sha256":self.journal.sha256}))
 }
}
struct BoundedWriter<'a>(&'a mut File);
impl Write for BoundedWriter<'_>{fn write(&mut self,b:&[u8])->std::io::Result<usize>{for chunk in b.chunks(65536){self.0.write_all(chunk)?;}Ok(b.len())}fn flush(&mut self)->std::io::Result<()>{self.0.flush()}}
impl Seek for BoundedWriter<'_>{fn seek(&mut self,p:SeekFrom)->std::io::Result<u64>{self.0.seek(p)}}
/// Internal composition only. Caller supplies original live context synchronously.
pub(crate) fn compose(capture:&ArchiveCapture,selected:Selected,current:impl Fn()->Result<Value,String>)->Result<Value,String>{compose_controlled(capture,selected,current,|_|Ok(()))}
fn compose_controlled(capture:&ArchiveCapture,selected:Selected,current:impl Fn()->Result<Value,String>,hook:impl Fn(&str)->Result<(),String>)->Result<Value,String>{
 let binding=capture.output_binding();selected.fence(&binding)?;selected.fence(&current()?)?;capture.fence()?;
 let general_format=!capture.raw_local_imports.is_empty();
 let disposition_format=general_format||capture.history_requires_format4()||!capture.raw_local_dispositions.is_empty()||capture.raw_imports.iter().any(|row|row["kind"]=="legacy_canonical_metadata");
 let local_format=disposition_format||!capture.raw_local_covers.is_empty()||capture.binaries.iter().any(|b|!b.asset["native_local_version_provenance"].is_null());
 let mut names=std::collections::BTreeSet::new();let mut binary_manifest=Vec::new();for binary in &capture.binaries{let asset:crate::vault::VaultAsset=serde_json::from_value(binary.asset.clone()).map_err(err)?;let name=crate::vault::archive_asset_path(&asset);if name.contains("..")||!names.insert(name.clone()){return Err("Archive entry names collide or invalid".into())}let mut meta=json!({"entry":name,"asset":binary.asset,"byte_length":binary.byte_length,"sha256":binary.binary_sha256,"native_intent_sha256":binary.native_intent_sha256});if !capture.raw_imports.is_empty()||local_format{meta["kind"]=json!(if binary.imported_provenance().is_some(){"joint_import"}else if binary.local_cover_provenance().is_some(){"original_local_cover"}else if binary.local_import_provenance().is_some(){"original_local_import"}else if binary.native_intent_sha256.is_some(){"download"}else{"legacy"});if let Some(original)=binary.imported_provenance(){meta["joint_receipt_sha256"]=json!(sha(original["receipt_json"].as_str().ok_or("Original joint receipt bytes missing")?.as_bytes()));}if let Some(local)=binary.local_cover_provenance(){meta["local_receipt_sha256"]=local["receipt_sha256"].clone();}if let Some(local)=binary.local_import_provenance(){meta["import_receipt_sha256"]=local["receipt_sha256"].clone();}}binary_manifest.push(meta);}
 let snapshot=serde_json::to_vec(&capture.snapshot).map_err(err)?;let receipts=serde_json::to_vec(&capture.raw_receipts).map_err(err)?;let imports=serde_json::to_vec(&capture.raw_imports).map_err(err)?;let locals=serde_json::to_vec(&capture.raw_local_covers).map_err(err)?;let dispositions=serde_json::to_vec(&capture.raw_local_dispositions).map_err(err)?;let general_imports=serde_json::to_vec(&capture.raw_local_imports).map_err(err)?;let mut manifest_value=json!({"format":if general_format{5}else if disposition_format{4}else if local_format{3}else if capture.raw_imports.is_empty(){1}else{2},"canonical_revision":capture.canonical_revision,"snapshot_sha256":sha(&snapshot),"raw_receipts_sha256":sha(&receipts),"binaries":binary_manifest});if !capture.raw_imports.is_empty()||local_format{manifest_value["joint_imports_sha256"]=json!(sha(&imports));}if local_format{manifest_value["local_covers_sha256"]=json!(sha(&locals));}if disposition_format{manifest_value["local_dispositions_sha256"]=json!(sha(&dispositions));}if general_format{manifest_value["local_imports_sha256"]=json!(sha(&general_imports));}let manifest=serde_json::to_vec(&manifest_value).map_err(err)?;
 if snapshot.len()>16*1024*1024||receipts.len()>32*1024*1024||manifest.len()>16*1024*1024||imports.len()>64*1024*1024||locals.len()>32*1024*1024||dispositions.len()>32*1024*1024||general_imports.len()>32*1024*1024{return Err("Archive metadata budget exceeded".into())}
 let mut stage=Staged::create(selected,sha(&manifest))?;
 {let mut zip=zip::ZipWriter::new(BoundedWriter(&mut stage.file));let options=zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).large_file(true);
 for(name,bytes)in[("vault.json",snapshot.as_slice()),("starlane-native/raw-receipts.json",receipts.as_slice()),("starlane-native/manifest.json",manifest.as_slice())]{zip.start_file(name,options).map_err(err)?;for chunk in bytes.chunks(65536){zip.write_all(chunk).map_err(err)?;}}
 if !capture.raw_imports.is_empty()||local_format{zip.start_file("starlane-native/joint-imports.json",options).map_err(err)?;for chunk in imports.chunks(65536){zip.write_all(chunk).map_err(err)?;}}
 if local_format{zip.start_file("starlane-native/local-covers.json",options).map_err(err)?;for chunk in locals.chunks(65536){zip.write_all(chunk).map_err(err)?;}}
 if disposition_format{zip.start_file("starlane-native/local-dispositions.json",options).map_err(err)?;for chunk in dispositions.chunks(65536){zip.write_all(chunk).map_err(err)?;}}
 if general_format{zip.start_file("starlane-native/local-imports.json",options).map_err(err)?;for chunk in general_imports.chunks(65536){zip.write_all(chunk).map_err(err)?;}}
 for(binary,meta)in capture.binaries.iter().zip(&binary_manifest){zip.start_file(meta["entry"].as_str().unwrap(),options).map_err(err)?;binary.copy_exact(&mut zip)?;}zip.finish().map_err(err)?;}
 stage.seal()?;stage.publish(||capture.fence(),current,hook)
}
/// Read-only restart inspection: never authorizes publication or deletes evidence.
pub(crate) fn inspect(parent_path:&Path,operation:&str,expected_binding:&Value)->Result<Value,String>{
 if operation.len()!=36||!operation.bytes().enumerate().all(|(i,b)|if [8,13,18,23].contains(&i){b==b'-'}else{b.is_ascii_hexdigit()&&!b.is_ascii_uppercase()}){return Err("Archive operation invalid".into())}
 let parent=Directory::capture(parent_path).map_err(err)?;let dir=parent.child(&format!(".starlane-archive-{operation}"),false).map_err(err)?;
 let mut jf=dir.open("journal.json",libc::O_RDONLY).map_err(err)?;regular(&jf,1)?;if jf.metadata().map_err(err)?.len()>65536{return Err("Archive journal too large".into())}let mut raw=Vec::new();jf.read_to_end(&mut raw).map_err(err)?;let j:Journal=serde_json::from_slice(&raw).map_err(err)?;
 if j.format!=1||j.operation!=operation||j.binding!=*expected_binding||j.parent!=identity(&parent.held).map_err(err)?||!matches!(j.state.as_str(),"staging"|"sealed"|"publication_unknown"|"published")||j.leaf.contains('/')||j.leaf=="."||j.leaf==".."||j.leaf.is_empty(){return Err("Archive journal binding invalid".into())}
 let status=if j.state=="staging"{"incomplete"}else{let mut file=dir.open("archive.zip",libc::O_RDONLY).map_err(err)?;let m=file.metadata().map_err(err)?;if !m.is_file()||named(&dir,"archive.zip")?!=Some((j.stage,j.length,m.nlink()))||identity(&file).map_err(err)?!=j.stage||m.len()!=j.length||file_sha(&mut file)?!=j.sha256{return Err("Archive retained evidence changed".into())}
 match named(&parent,&j.leaf)?{None if m.nlink()==1=>if j.state=="publication_unknown"{"unresolved_output_absent"}else if j.state=="published"{"published_output_missing"}else{"sealed_unpublished"},Some((id,len,2))if id==j.stage&&len==j.length&&m.nlink()==2=>"published_verified",_=>"conflict"}};
 parent.fence().map_err(err)?;dir.fence().map_err(err)?;Ok(json!({"format":1,"operation":operation,"state":status,"journal_state":j.state,"automatic_retry":false,"automatic_cleanup":false}))
}
#[cfg(test)] pub(super) fn test_selection(path:PathBuf,binding:Value)->Result<Selected,String>{Selected::admit(path,binding)}
#[cfg(test)] mod tests;

pub(super) mod recovery;
pub(crate) use recovery::{pending,acknowledge_review};

#[cfg(test)] pub(super) fn test_compose_interrupted(capture:&ArchiveCapture,selected:Selected,current:impl Fn()->Result<Value,String>)->Result<Value,String>{compose_controlled(capture,selected,current,|p|if p=="journal"{Err("controlled interruption before publication".into())}else{Ok(())})}
