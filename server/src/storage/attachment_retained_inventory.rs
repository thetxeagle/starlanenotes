//! Read-only no-follow quota inventory; never a deletion, installation or serving token.
use super::{Asset,LIMIT};
use crate::storage::file_read_custody::{admit_local_filesystem, BrokerCancelHandle, CustodySet};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use anyhow::{ensure,Context};
use std::{collections::{BTreeMap,BTreeSet},ffi::CString,fs::{File,Metadata},io,os::{fd::{AsRawFd,FromRawFd},unix::fs::MetadataExt},path::{Component,Path,PathBuf}};
#[derive(Debug,Clone,PartialEq,Eq)]
struct Stamp(u64,u64,u64,u64,u32,i64,i64,i64,i64);
fn stamp(m:&Metadata)->Stamp{Stamp(m.dev(),m.ino(),m.len(),m.nlink(),m.mode(),m.mtime(),m.mtime_nsec(),m.ctime(),m.ctime_nsec())}
#[derive(Debug,PartialEq,Eq)]
struct RootStamp(u64,u64,u32,u32,u32);
fn root_stamp(m:&Metadata)->RootStamp{RootStamp(m.dev(),m.ino(),m.mode(),m.uid(),m.gid())}
struct RootNamespace{state:Mutex<RootWatch>}
struct RootWatch{file:File,watch:i32,poisoned:bool}
impl RootNamespace{
 fn new(root:&File)->io::Result<Self>{
  admit_local_filesystem(root)?;
  // SAFETY: syscall has no pointers; newly created nonblocking FD is owned once.
  let fd=unsafe{libc::inotify_init1(libc::IN_NONBLOCK|libc::IN_CLOEXEC)};
  if fd<0{return Err(io::Error::last_os_error())}
  let file=unsafe{File::from_raw_fd(fd)};
  let path=CString::new(format!("/proc/self/fd/{}",root.as_raw_fd()))?;
  let mask=libc::IN_CREATE|libc::IN_DELETE|libc::IN_MOVED_FROM|libc::IN_MOVED_TO|libc::IN_ATTRIB|libc::IN_MOVE_SELF|libc::IN_DELETE_SELF|libc::IN_UNMOUNT|libc::IN_IGNORED|libc::IN_ONLYDIR;
  // SAFETY: watch the actual held directory, not a caller-controlled path.
  let watch=unsafe{libc::inotify_add_watch(file.as_raw_fd(),path.as_ptr(),mask)};
  if watch<0{return Err(io::Error::last_os_error())}
  Ok(Self{state:Mutex::new(RootWatch{file,watch,poisoned:false})})
 }
 fn fence(&self)->anyhow::Result<()>{
  let mut state=self.state.lock().map_err(|_|anyhow::anyhow!("Retained root watch unavailable"))?;
  ensure!(!state.poisoned,"Retained root namespace custody lost");
  let result=state.inspect();
  if result.is_err(){state.poisoned=true;}
  result
 }
}
impl RootWatch{
 fn ready(&self)->io::Result<bool>{
  let mut poll=libc::pollfd{fd:self.file.as_raw_fd(),events:libc::POLLIN,revents:0};
  // SAFETY: valid initialized single pollfd, zero-time observation.
  let result=unsafe{libc::poll(&mut poll,1,0)};
  if result<0{return Err(io::Error::last_os_error())}
  if poll.revents & !libc::POLLIN != 0{return Err(io::Error::other("Retained root watch lost"))}
  Ok(result!=0)
 }
 fn inspect(&self)->anyhow::Result<()>{
  if !self.ready()?{return Ok(())}
  let mut available:libc::c_int=0;
  // SAFETY: initialized writable integer for the kernel queue byte count.
  if unsafe{libc::ioctl(self.file.as_raw_fd(),libc::FIONREAD,&mut available)}<0{return Err(io::Error::last_os_error().into())}
  ensure!(available>0,"Retained root notification lost");
  let length=usize::try_from(available)?;let mut bytes=Vec::<u8>::new();bytes.try_reserve_exact(length)?;bytes.resize(length,0);
  // One queue snapshot, never an unbounded drain under a continuing producer.
  let count=unsafe{libc::read(self.file.as_raw_fd(),bytes.as_mut_ptr().cast(),bytes.len())};
  if count<=0{return Err(io::Error::last_os_error().into())}
  let bytes=&bytes[..usize::try_from(count)?];let header=std::mem::size_of::<libc::inotify_event>();let mut offset=0;
  while offset<bytes.len(){
   ensure!(bytes.len()-offset>=header,"Malformed retained root notification");
   // SAFETY: bounds checked above; byte-buffer alignment need not match C struct.
   let event=unsafe{bytes.as_ptr().add(offset).cast::<libc::inotify_event>().read_unaligned()};
   let end=offset.checked_add(header).and_then(|n|n.checked_add(event.len as usize)).context("Retained root notification overflow")?;
   ensure!(end<=bytes.len(),"Truncated retained root notification");
   ensure!(event.mask&libc::IN_Q_OVERFLOW==0&&event.wd==self.watch,"Retained root watch lost or overflowed");
   let raw=&bytes[offset+header..end];let name=&raw[..raw.iter().position(|b|*b==0).unwrap_or(raw.len())];
   // Root self-events and every event for the managed 'spaces' entry matter.
   // Other root children (including legitimate SQL journals) are not authority.
   ensure!(event.len!=0&&name!=b"spaces","Retained root namespace changed");
   ensure!(event.mask&(libc::IN_IGNORED|libc::IN_UNMOUNT|libc::IN_MOVE_SELF|libc::IN_DELETE_SELF)==0,"Retained root watch lost");
   offset=end;
  }
  ensure!(!self.ready()?,"Retained root notification pressure requires retry");Ok(())
 }
}

fn open(parent:&File,name:&str,dir:bool)->io::Result<File>{
 if name.is_empty()||name=="."||name==".."||name.contains('/')||name.contains('\\'){return Err(io::Error::other("Unsafe inventory component"))}
 let c=CString::new(name)?;let flags=libc::O_RDONLY|libc::O_CLOEXEC|libc::O_NOFOLLOW|libc::O_NONBLOCK|if dir{libc::O_DIRECTORY}else{0};
 // SAFETY: held directory FD, validated single component; returned FD becomes one owned File.
 let fd=unsafe{libc::openat(parent.as_raw_fd(),c.as_ptr(),flags)};if fd<0{return Err(io::Error::last_os_error())}Ok(unsafe{File::from_raw_fd(fd)})
}
fn root(path:&Path)->io::Result<File>{let absolute=if path.is_absolute(){path.to_owned()}else{std::env::current_dir()?.join(path)};let mut file=File::open("/")?;for c in absolute.components(){match c{Component::RootDir=>(),Component::Normal(n)=>file=open(&file,n.to_str().ok_or_else(||io::Error::other("Invalid root component"))?,true)?,_=>return Err(io::Error::other("Unsafe inventory root"))}}Ok(file)}
fn at(root:&File,path:&str,dir:bool)->io::Result<File>{let mut held=root.try_clone()?;let parts:Vec<_>=path.split('/').collect();if path.is_empty(){return Ok(held)}for (i,p) in parts.iter().enumerate(){held=open(&held,p,dir||i+1<parts.len())?;}Ok(held)}
fn names(file:&File,cancel:&CaptureContext)->anyhow::Result<BTreeSet<String>>{let mut names=BTreeSet::new();for entry in std::fs::read_dir(format!("/proc/self/fd/{}",file.as_raw_fd()))?{cancel.check()?;ensure!(names.len()<LIMIT,"Retained directory entry bound exceeded");let name=entry?.file_name().into_string().map_err(|_|anyhow::anyhow!("Non-UTF8 managed attachment entry"))?;ensure!(!name.is_empty()&&name!="."&&name!=".."&&!name.contains('\\'),"Invalid managed entry");names.insert(name);}Ok(names)}
struct Directory {relative:String,file:File,stamp:Stamp,names:BTreeSet<String>}
struct Blob {relative:String,asset:String,file:File,stamp:Stamp}
pub(super) struct Inventory {custody:CustodySet,cancel:Arc<CaptureContext>,root_path:PathBuf,root:File,root_watch:RootNamespace,root_stamp:RootStamp,directories:Vec<Directory>,blobs:Vec<Blob>,absent_spaces:bool}
impl Inventory {
 pub(super) fn capture(path:PathBuf,assets:&BTreeMap<String,Asset>)->anyhow::Result<Self>{
  Self::capture_with_context(path,assets,Arc::new(CaptureContext::default()))
 }
 fn capture_with_context(path:PathBuf,assets:&BTreeMap<String,Asset>,cancel:Arc<CaptureContext>)->anyhow::Result<Self>{
  cancel.check()?;
  let custody=CustodySet::new(admit_local_filesystem)?;
  cancel.register(custody.cancellation())?;
  let held=root(&path)?;let root_watch=RootNamespace::new(&held)?;let root_stamp=root_stamp(&held.metadata()?);let mut paths=BTreeMap::new();for(id,a)in assets{cancel.check()?;for p in &a.paths{cancel.check()?;if let Some(old)=paths.insert(p.clone(),(id.clone(),a.metadata.bytes)){ensure!(old==(id.clone(),a.metadata.bytes),"Ambiguous managed attachment path");}}}
  let mut result=Self{custody,cancel,root_path:path,root:held,root_watch,root_stamp,directories:vec![],blobs:vec![],absent_spaces:false};
  let spaces=match open(&result.root,"spaces",true){Ok(f)=>f,Err(e)if e.kind()==io::ErrorKind::NotFound=>{result.absent_spaces=true;result.fence()?;return Ok(result)},Err(e)=>return Err(e.into())};
  let slugs=result.directory("spaces".into(),spaces)?;
  let mut sealed=BTreeSet::new();
  for slug in slugs {result.cancel.check()?;let space_path=format!("spaces/{slug}");let space=at(&result.root,&space_path,true)?;let space_names=result.directory(space_path.clone(),space.try_clone()?)?;if !space_names.contains("attachments"){continue}
   let attachment_path=format!("{space_path}/attachments");let attachment_dir=open(&space,"attachments",true)?;let pages=result.directory(attachment_path.clone(),attachment_dir.try_clone()?)?;
   for page in pages {result.cancel.check()?;let page_path=format!("{attachment_path}/{page}");let page_dir=open(&attachment_dir,&page,true)?;let files=result.directory(page_path.clone(),page_dir.try_clone()?)?;
    for name in files {result.cancel.check()?;ensure!(result.blobs.len()<LIMIT,"Retained blob inventory bound exceeded");let relative=format!("{page_path}/{name}");let(id,length)=paths.get(&relative).context("Unattributed retained attachment bytes; quota admission requires operator review")?;
     let file=open(&page_dir,&name,false)?;let identity=file.metadata()?;
     ensure!(identity.is_file(),"Managed retained blob is not regular");
     if sealed.insert((identity.dev(),identity.ino())){result.cancel.check()?;result.custody.seal(&file)?;result.cancel.check()?;}
     #[cfg(test)] result.cancel.pause_once(TestPhase::Capture);
     result.cancel.check()?;
     let metadata=file.metadata()?;ensure!(metadata.is_file()&&metadata.len()==*length as u64,"Managed retained blob differs from declared accounting length");let captured=stamp(&metadata);result.blobs.push(Blob{relative,asset:id.clone(),file,stamp:captured});
    }
   }
  }
  result.fence()?;Ok(result)
 }
 fn directory(&mut self,relative:String,file:File)->anyhow::Result<BTreeSet<String>>{ensure!(self.directories.len()<LIMIT,"Retained directory inventory bound exceeded");let before=stamp(&file.metadata()?);self.cancel.check()?;let entries=names(&file,&self.cancel)?;ensure!(stamp(&file.metadata()?)==before,"Managed directory changed during inventory");self.directories.push(Directory{relative,file,stamp:before,names:entries.clone()});Ok(entries)}
 pub(super) fn charge(&self,assets:&BTreeMap<String,Asset>)->anyhow::Result<i64>{
  self.fence()?;
  let mut identities=BTreeMap::new();let mut physical:BTreeMap<String,i64>=BTreeMap::new();for b in &self.blobs{self.cancel.check()?;let identity=(b.stamp.0,b.stamp.1);if let Some(old)=identities.insert(identity,b.asset.clone()){ensure!(old==b.asset,"One inode ambiguously claims multiple immutable assets");continue}let bytes=i64::try_from(b.stamp.2)?;let current=physical.entry(b.asset.clone()).or_default();*current=current.checked_add(bytes).context("Physical retained accounting overflow")?;}
  let mut total=0i64;for(id,a)in assets{self.cancel.check()?;let charged=a.metadata.bytes.max(*physical.get(id).unwrap_or(&0));total=total.checked_add(charged).context("Retained byte accounting overflow")?;}self.fence()?;Ok(total)
 }
 pub(super) fn fence(&self)->anyhow::Result<()>{
  self.cancel.check()?;admit_local_filesystem(&self.root)?;self.root_watch.fence()?;self.custody.fence()?;
  let fresh_root=root(&self.root_path)?;ensure!(root_stamp(&self.root.metadata()?)==self.root_stamp&&root_stamp(&fresh_root.metadata()?)==self.root_stamp,"Quota inventory root changed");
  if self.absent_spaces{ensure!(open(&fresh_root,"spaces",true).is_err_and(|e|e.kind()==io::ErrorKind::NotFound),"Managed spaces appeared during quota decision");}
  for d in &self.directories{self.cancel.check()?;let fresh=at(&fresh_root,&d.relative,true)?;ensure!(stamp(&d.file.metadata()?)==d.stamp&&stamp(&fresh.metadata()?)==d.stamp&&names(&fresh,&self.cancel)?==d.names,"Managed attachment directory changed during quota decision");}
  for b in &self.blobs{self.cancel.check()?;let fresh=at(&fresh_root,&b.relative,false)?;ensure!(stamp(&b.file.metadata()?)==b.stamp&&stamp(&fresh.metadata()?)==b.stamp,"Managed retained blob changed during quota decision");}self.cancel.check()?;self.root_watch.fence()?;self.custody.fence()?;Ok(())}
}


#[derive(Default)]
struct CaptureContext {
 cancelled:AtomicBool,
 broker:Mutex<Option<BrokerCancelHandle>>,
 #[cfg(test)] pause:Mutex<Option<(TestPhase,Arc<TestPause>)>>,
}
impl CaptureContext {
 fn check(&self)->anyhow::Result<()>{ensure!(!self.cancelled.load(Ordering::SeqCst),"Retained inventory cancelled");Ok(())}
 fn register(&self,broker:BrokerCancelHandle)->anyhow::Result<()>{
  let mut slot=self.broker.lock().map_err(|_|anyhow::anyhow!("Retained custody registration failed"))?;
  if self.cancelled.load(Ordering::SeqCst){broker.cancel();anyhow::bail!("Retained inventory cancelled")}
  *slot=Some(broker);
  // cancel() sets the flag before taking this same lock, closing both races.
  self.check()
 }
 fn cancel(&self){
  self.cancelled.store(true,Ordering::SeqCst);
  match self.broker.lock(){Ok(slot)=>{if let Some(broker)=slot.as_ref(){broker.cancel();}},Err(e)=>{if let Some(broker)=e.into_inner().as_ref(){broker.cancel();}}}
 }
 #[cfg(test)] fn pause_once(&self,phase:TestPhase){
  let pause={let mut slot=self.pause.lock().unwrap();if slot.as_ref().is_some_and(|(p,_)|*p==phase){slot.take().map(|(_,p)|p)}else{None}};
  if let Some(pause)=pause{pause.wait();}
 }
}
struct CancelOnDrop {context:Arc<CaptureContext>,armed:bool}
impl CancelOnDrop {fn new(context:Arc<CaptureContext>)->Self{Self{context,armed:true}}}
impl Drop for CancelOnDrop {fn drop(&mut self){if self.armed{self.context.cancel();}}}

/// Async ownership boundary. Destruction cancels immediately but never joins the
/// broker on a live async executor. The blocking task owns every temporary FD.
pub(super) struct PhysicalInventory {inventory:Option<Arc<Inventory>>,context:Arc<CaptureContext>}
impl PhysicalInventory {
 pub(super) async fn capture(root:PathBuf,assets:BTreeMap<String,Asset>)->anyhow::Result<(Self,i64)>{
  Self::capture_with_context(root,assets,Arc::new(CaptureContext::default())).await
 }
 async fn capture_with_context(root:PathBuf,assets:BTreeMap<String,Asset>,context:Arc<CaptureContext>)->anyhow::Result<(Self,i64)>{
  let mut guard=CancelOnDrop::new(context.clone());
  let task_context=context.clone();
  let result=tokio::task::spawn_blocking(move||->anyhow::Result<(Self,i64)>{
   let inventory=Inventory::capture_with_context(root,&assets,task_context.clone())?;
   // Construct the cleanup-owning result INSIDE the closure, not after await:
   // JoinHandle abandonment must never drop a bare Inventory on the executor.
   let result=Self{inventory:Some(Arc::new(inventory)),context:task_context};
   let charge=result.inventory.as_ref().unwrap().charge(&assets)?;
   #[cfg(test)] result.context.pause_once(TestPhase::Result);
   result.context.check()?;
   Ok((result,charge))
  }).await??;
  context.check()?;
  guard.armed=false;
  Ok(result)
 }
 pub(super) async fn fence_async(&self)->anyhow::Result<()>{
  let mut guard=CancelOnDrop::new(self.context.clone());
  let retained=self.inventory.as_ref().context("Retained inventory released")?.clone();
  let result=tokio::task::spawn_blocking(move||{
   #[cfg(test)] retained.cancel.pause_once(TestPhase::Fence);
   retained.fence()
  }).await?;
  result?;self.context.check()?;guard.armed=false;Ok(())
 }
 /// Consumes the wrapper in an already blocking cleanup worker. Unlike Drop,
 /// this does not enqueue another cleanup task; a unique inner owner joins here.
 pub(super) fn release_blocking(mut self){self.context.cancel();drop(self.inventory.take());}
 /// Consumes this generation without joining; safe in an async cancellation guard.
 pub(super) fn cancel(&self){self.context.cancel();}
 /// Only for callers already executing in a synchronous/blocking context.
 pub(super) fn fence(&self)->anyhow::Result<()>{self.inventory.as_ref().context("Retained inventory released")?.fence()}
}
impl Drop for PhysicalInventory {
 fn drop(&mut self){
  self.context.cancel();
  if let Some(retained)=self.inventory.take(){
   if let Ok(runtime)=tokio::runtime::Handle::try_current(){
    runtime.spawn_blocking(move||drop(retained));
   }else{drop(retained);}
  }
 }
}

#[cfg(test)]
#[derive(Clone,Copy,PartialEq,Eq)]
enum TestPhase{Capture,Fence,Result}
#[cfg(test)]
#[derive(Default)]
struct TestPause{reached:tokio::sync::Notify,released:Mutex<bool>,release:std::sync::Condvar}
#[cfg(test)]
impl TestPause{
 fn wait(&self){self.reached.notify_one();let mut released=self.released.lock().unwrap();while !*released{released=self.release.wait(released).unwrap();}}
 fn release(&self){*self.released.lock().unwrap()=true;self.release.notify_all();}
}

#[cfg(test)]
mod tests {
 use super::*;
 fn fixture()->(PathBuf,BTreeMap<String,Asset>){let root=std::env::temp_dir().join(format!("retained-inventory-{}",uuid::Uuid::new_v4()));let path="spaces/one/attachments/doc/body.bin";std::fs::create_dir_all(root.join("spaces/one/attachments/doc")).unwrap();std::fs::write(root.join(path),b"BYTES").unwrap();let asset=Asset{metadata:super::super::Metadata{bytes:5,filename:"body.bin".into(),name:"Original.bin".into(),mime:"application/octet-stream".into(),uploader:"user".into(),created:"original".into()},paths:BTreeSet::from([path.into()])};(root,BTreeMap::from([("asset".into(),asset)]))}
 #[test]fn same_inode_alias_charges_once_distinct_accounted_copy_charges_extra(){let(root,mut assets)=fixture();let second="spaces/two/attachments/doc/body.bin";std::fs::create_dir_all(root.join("spaces/two/attachments/doc")).unwrap();std::fs::hard_link(root.join("spaces/one/attachments/doc/body.bin"),root.join(second)).unwrap();assets.get_mut("asset").unwrap().paths.insert(second.into());let held=Inventory::capture(root.clone(),&assets).unwrap();assert_eq!(held.charge(&assets).unwrap(),5);drop(held);std::fs::remove_file(root.join(second)).unwrap();std::fs::write(root.join(second),b"BYTES").unwrap();let held=Inventory::capture(root.clone(),&assets).unwrap();assert_eq!(held.charge(&assets).unwrap(),10);std::fs::remove_dir_all(root).unwrap();}
 #[test]fn held_file_mutation_and_new_unknown_entry_fail_final_fence(){let(root,assets)=fixture();let held=Inventory::capture(root.clone(),&assets).unwrap();std::fs::write(root.join("spaces/one/attachments/doc/body.bin"),b"OTHER").unwrap();assert!(held.fence().is_err());drop(held);let held=Inventory::capture(root.clone(),&assets).unwrap();std::fs::write(root.join("spaces/one/attachments/doc/unknown.bin"),b"X").unwrap();assert!(held.fence().is_err());assert!(Inventory::capture(root.clone(),&assets).is_err());std::fs::remove_dir_all(root).unwrap();}
 #[test]fn symlink_never_follows_and_inode_cannot_claim_two_assets(){let(root,mut assets)=fixture();let second="spaces/two/attachments/doc/body.bin";std::fs::create_dir_all(root.join("spaces/two/attachments/doc")).unwrap();std::os::unix::fs::symlink(root.join("spaces/one/attachments/doc/body.bin"),root.join(second)).unwrap();assets.get_mut("asset").unwrap().paths.insert(second.into());assert!(Inventory::capture(root.clone(),&assets).is_err());std::fs::remove_file(root.join(second)).unwrap();std::fs::hard_link(root.join("spaces/one/attachments/doc/body.bin"),root.join(second)).unwrap();let mut other=assets["asset"].clone();other.paths=BTreeSet::from([second.into()]);assets.get_mut("asset").unwrap().paths.remove(second);assets.insert("other-asset".into(),other);let held=Inventory::capture(root.clone(),&assets).unwrap();assert!(held.charge(&assets).is_err());std::fs::remove_dir_all(root).unwrap();}
 #[test]fn explicit_inventory_count_bound_refuses_before_path_attribution(){let(root,assets)=fixture();for i in 0..LIMIT{std::fs::write(root.join(format!("spaces/one/attachments/doc/entry-{i}")),b"").unwrap();}let error=Inventory::capture(root.clone(),&assets).err().unwrap().to_string();assert!(error.contains("entry bound"),"{error}");std::fs::remove_dir_all(root).unwrap();}
 const BLOB:&str="spaces/one/attachments/doc/body.bin";
 fn child_write_open(path:&Path){
  let mut child=std::process::Command::new("python3").arg("-I").arg("-c").arg("import os,sys,errno\ntry: os.close(os.open(sys.argv[1],os.O_WRONLY|os.O_NONBLOCK))\nexcept OSError as e: sys.exit(0 if e.errno==errno.EAGAIN else 2)\nsys.exit(0)").arg(path).spawn().unwrap();
  let started=std::time::Instant::now();loop{if let Some(status)=child.try_wait().unwrap(){assert!(status.success());break}if started.elapsed()>std::time::Duration::from_secs(3){let _=child.kill();let _=child.wait();panic!("Owned nonblocking writer timed out")}std::thread::sleep(std::time::Duration::from_millis(5));}
 }
 #[test]fn restored_bytes_and_cancelled_write_open_still_refuse_charge(){
  let(root,assets)=fixture();let held=Inventory::capture(root.clone(),&assets).unwrap();
  std::fs::write(root.join(BLOB),b"OTHER").unwrap();std::fs::write(root.join(BLOB),b"BYTES").unwrap();
  assert_eq!(std::fs::read(root.join(BLOB)).unwrap(),b"BYTES");assert!(held.charge(&assets).is_err());assert!(held.fence().is_err());drop(held);
  let held=Inventory::capture(root.clone(),&assets).unwrap();let before=stamp(&std::fs::metadata(root.join(BLOB)).unwrap());
  child_write_open(&root.join(BLOB));
  assert_eq!(stamp(&std::fs::metadata(root.join(BLOB)).unwrap()),before);
  assert!(held.fence().is_err());assert!(held.charge(&assets).is_err());drop(held);std::fs::remove_dir_all(root).unwrap();
 }
 #[test]fn existing_writer_and_shared_writable_mapping_refuse_inventory(){
  let(root,assets)=fixture();let writer=std::fs::OpenOptions::new().read(true).write(true).open(root.join(BLOB)).unwrap();
  assert!(Inventory::capture(root.clone(),&assets).is_err());
  let address=unsafe{libc::mmap(std::ptr::null_mut(),5,libc::PROT_READ|libc::PROT_WRITE,libc::MAP_SHARED,writer.as_raw_fd(),0)};assert_ne!(address,libc::MAP_FAILED);drop(writer);
  assert!(Inventory::capture(root.clone(),&assets).is_err());assert_eq!(unsafe{libc::munmap(address,5)},0);
  let held=Inventory::capture(root.clone(),&assets).unwrap();assert_eq!(held.charge(&assets).unwrap(),5);drop(held);std::fs::remove_dir_all(root).unwrap();
 }
 #[test]fn journal_alias_read_is_valid_but_alias_write_and_relink_poison(){
  let(root,assets)=fixture();let journal=root.join("journal");std::fs::create_dir(&journal).unwrap();let alias=journal.join("alias");std::fs::hard_link(root.join(BLOB),&alias).unwrap();
  let held=Inventory::capture(root.clone(),&assets).unwrap();assert_eq!(std::fs::metadata(&alias).unwrap().nlink(),2);assert_eq!(std::fs::read(&alias).unwrap(),b"BYTES");assert_eq!(held.charge(&assets).unwrap(),5);held.fence().unwrap();
  std::fs::write(&alias,b"OTHER").unwrap();std::fs::write(&alias,b"BYTES").unwrap();assert!(held.fence().is_err());drop(held);
  let held=Inventory::capture(root.clone(),&assets).unwrap();std::fs::remove_file(&alias).unwrap();std::fs::hard_link(root.join(BLOB),&alias).unwrap();
  assert_eq!(std::fs::metadata(&alias).unwrap().nlink(),2);assert!(held.fence().is_err());assert!(held.charge(&assets).is_err());drop(held);std::fs::remove_dir_all(root).unwrap();
 }
 struct ReleasePause(Arc<TestPause>);
 impl Drop for ReleasePause{fn drop(&mut self){self.0.release();}}
 async fn cancelled_writer_works(root:PathBuf){
  tokio::time::timeout(std::time::Duration::from_secs(3),tokio::task::spawn_blocking(move||std::fs::write(root.join(BLOB),b"AFTER"))).await.expect("Cancelled custody retained a writer-blocking lease").unwrap().unwrap();
 }
 #[tokio::test(flavor="current_thread")]
 async fn abandoned_capture_and_prepared_result_cancel_without_blocking_executor(){
  for phase in [TestPhase::Capture,TestPhase::Result]{
   let(root,assets)=fixture();let context=Arc::new(CaptureContext::default());let pause=Arc::new(TestPause::default());let release=ReleasePause(pause.clone());*context.pause.lock().unwrap()=Some((phase,pause.clone()));
   let path=root.clone();let ctx=context.clone();let task=tokio::spawn(async move{PhysicalInventory::capture_with_context(path,assets,ctx).await});
   tokio::time::timeout(std::time::Duration::from_secs(3),pause.reached.notified()).await.expect("Capture did not reach owned pause");
   task.abort();let outcome=tokio::time::timeout(std::time::Duration::from_secs(1),task).await.expect("Capture cancellation blocked async executor");assert!(outcome.is_err_and(|e|e.is_cancelled()));assert!(context.cancelled.load(Ordering::SeqCst));
   // The capture task remains paused and still owns its descriptors here.
   // Cancellation must release leases independently of its eventual cleanup.
   cancelled_writer_works(root.clone()).await;
   drop(release);std::fs::remove_dir_all(root).unwrap();
  }
 }
 #[tokio::test(flavor="current_thread")]
 async fn abandoned_async_fence_cancels_outer_live_inventory(){
  let(root,assets)=fixture();let(held,bytes)=PhysicalInventory::capture(root.clone(),assets).await.unwrap();assert_eq!(bytes,5);let held=Arc::new(held);
  let pause=Arc::new(TestPause::default());let release=ReleasePause(pause.clone());*held.context.pause.lock().unwrap()=Some((TestPhase::Fence,pause.clone()));
  let retained=held.clone();let task=tokio::spawn(async move{retained.fence_async().await});
  tokio::time::timeout(std::time::Duration::from_secs(3),pause.reached.notified()).await.expect("Fence did not reach owned pause");
  task.abort();let outcome=tokio::time::timeout(std::time::Duration::from_secs(1),task).await.expect("Fence cancellation blocked async executor");assert!(outcome.is_err_and(|e|e.is_cancelled()));
  assert!(held.context.cancelled.load(Ordering::SeqCst));cancelled_writer_works(root.clone()).await;assert!(held.fence().is_err());
  drop(held);drop(release);std::fs::remove_dir_all(root).unwrap();
 }
 #[test]fn already_cancelled_registration_never_acquires_inventory(){
  let(root,assets)=fixture();let context=Arc::new(CaptureContext::default());context.cancel();assert!(Inventory::capture_with_context(root.clone(),&assets,context.clone()).is_err());
  let broker=CustodySet::new(admit_local_filesystem).unwrap();let handle=broker.cancellation();assert!(context.register(handle).is_err());assert!(broker.fence().is_err());drop(broker);std::fs::write(root.join(BLOB),b"AFTER").unwrap();std::fs::remove_dir_all(root).unwrap();
 }

 #[test]fn unrelated_root_children_do_not_invalidate_inventory(){
  let(root,assets)=fixture();let held=Inventory::capture(root.clone(),&assets).unwrap();
  std::fs::write(root.join("arbitrary-sidecar"),b"legitimate unrelated root bytes").unwrap();std::fs::rename(root.join("arbitrary-sidecar"),root.join("renamed-sidecar")).unwrap();std::fs::remove_file(root.join("renamed-sidecar")).unwrap();
  held.fence().unwrap();assert_eq!(held.charge(&assets).unwrap(),5);drop(held);std::fs::remove_dir_all(root).unwrap();
 }
 #[test]fn spaces_rename_return_and_root_mode_restore_fail_sticky(){
  let(root,assets)=fixture();let held=Inventory::capture(root.clone(),&assets).unwrap();
  std::fs::rename(root.join("spaces"),root.join("temporary-spaces")).unwrap();std::fs::rename(root.join("temporary-spaces"),root.join("spaces")).unwrap();
  assert!(held.fence().is_err());assert!(held.charge(&assets).is_err());drop(held);
  let held=Inventory::capture(root.clone(),&assets).unwrap();use std::os::unix::fs::PermissionsExt;let mode=std::fs::metadata(&root).unwrap().mode();
  std::fs::set_permissions(&root,std::fs::Permissions::from_mode((mode&0o7777)^0o100)).unwrap();std::fs::set_permissions(&root,std::fs::Permissions::from_mode(mode)).unwrap();
  assert!(held.fence().is_err());assert!(held.fence().is_err());drop(held);std::fs::remove_dir_all(root).unwrap();
 }
 #[test]fn root_identity_replacement_refuses_even_with_original_contents_returned(){
  let(root,assets)=fixture();let held=Inventory::capture(root.clone(),&assets).unwrap();let moved=root.with_extension("moved");std::fs::rename(&root,&moved).unwrap();std::fs::create_dir(&root).unwrap();
  assert!(held.fence().is_err());std::fs::remove_dir(&root).unwrap();std::fs::rename(&moved,&root).unwrap();assert!(held.fence().is_err());drop(held);std::fs::remove_dir_all(root).unwrap();
 }

}
