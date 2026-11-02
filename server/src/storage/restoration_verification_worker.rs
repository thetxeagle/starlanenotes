// Purpose-separated inherited kernel custody. Only a genuine original target
// selection may prepare a handoff. Wire metadata binds received descriptors; it
// never grants source-current authority or immutable verification publication.
#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct VerificationWorkerWire {
 version:u32,nonce:String,path:PathBuf,chain_stamps:Vec<String>,main_stamp:String,
 inventory_sha256:String,executable_stamp:String,executable_sha256:String,
 bundle_path:PathBuf,bundle_stamps:Vec<String>,bundle_inventory_sha256:String,original_sql_sha256:String,
}
fn verification_worker_stamp(file:&File)->io::Result<String> {
 Ok(digest(format!("{:?}",physical_stamp(file)?).as_bytes()))
}
fn verification_worker_inventory_sha(inventory:&SqliteCaptureInventory)->io::Result<String> {
 let dirs:Vec<_>=inventory.directories.iter().map(|(p,(_,s))|(p,format!("{s:?}"))).collect();
 let files:Vec<_>=inventory.files.iter().map(|(p,(_,s,sha))|(p,format!("{s:?}"),sha)).collect();
 let bytes=serde_json::to_vec(&(format!("{:?}",inventory.root_stamp),dirs,files)).map_err(|e|fail(&e.to_string()))?;
 if bytes.len()>32*1024*1024{return Err(fail("Inherited worker inventory exceeds32MiB"));}Ok(digest(&bytes))
}
fn verification_worker_inventory_fence(inventory:&SqliteCaptureInventory,root:&Root)->io::Result<()> {
 root.check()?;let current=verification_pager_inventory(root)?;
 if verification_worker_inventory_sha(&current)?!=verification_worker_inventory_sha(inventory)?{return Err(fail("Original worker full inventory changed"));}
 for(held,stamp)in inventory.directories.values(){if physical_stamp(held)?!=*stamp{return Err(fail("Original worker held directory changed"));}}
 // This cache belongs only to the independent held-descriptor pass.
 let mut held_hashes=Default::default();
 for(held,stamp,sha)in inventory.files.values(){if physical_stamp(held)?!=*stamp||verification_pager_walk_hash(held,stamp,&mut held_hashes)?!=*sha{return Err(fail("Original worker held file changed"));}}
 root.check()
}
struct VerificationWorkerPacket {bytes:Vec<u8>,files:Vec<File>,credentials:libc::ucred}
fn verification_worker_socket(fd:i32)->io::Result<()> {
 for(option,expected)in[(libc::SO_TYPE,libc::SOCK_SEQPACKET),(libc::SO_DOMAIN,libc::AF_UNIX)] {
  let mut actual=0i32;let mut size=std::mem::size_of::<i32>()as libc::socklen_t;
  if unsafe{libc::getsockopt(fd,libc::SOL_SOCKET,option,(&mut actual as *mut i32).cast(),&mut size)}!=0{return Err(io::Error::last_os_error());}
  if actual!=expected||size as usize!=std::mem::size_of::<i32>(){return Err(fail("Worker channel is not private Unix seqpacket"));}
 }
 let yes=1i32;let timeout=libc::timeval{tv_sec:15,tv_usec:0};
 if unsafe{libc::setsockopt(fd,libc::SOL_SOCKET,libc::SO_PASSCRED,(&yes as *const i32).cast(),std::mem::size_of_val(&yes)as libc::socklen_t)}!=0
  ||unsafe{libc::setsockopt(fd,libc::SOL_SOCKET,libc::SO_RCVTIMEO,(&timeout as *const libc::timeval).cast(),std::mem::size_of_val(&timeout)as libc::socklen_t)}!=0{return Err(io::Error::last_os_error());}Ok(())
}
fn verification_worker_pair()->io::Result<(File,File)> {
 use std::os::fd::FromRawFd;let mut fds=[-1;2];
 if unsafe{libc::socketpair(libc::AF_UNIX,libc::SOCK_SEQPACKET|libc::SOCK_CLOEXEC,0,fds.as_mut_ptr())}!=0{return Err(io::Error::last_os_error());}
 let a=unsafe{File::from_raw_fd(fds[0])};let b=unsafe{File::from_raw_fd(fds[1])};
 verification_worker_socket(a.as_raw_fd())?;verification_worker_socket(b.as_raw_fd())?;Ok((a,b))
}
fn verification_worker_send(socket:&File,bytes:&[u8],files:&[&File])->io::Result<()> {
 if bytes.is_empty()||bytes.len()>16*1024||files.len()>32{return Err(fail("Worker packet exceeds bounds"));}
 let mut buffer=[0usize;128];let mut iov=libc::iovec{iov_base:bytes.as_ptr()as *mut _,iov_len:bytes.len()};
 let mut msg:libc::msghdr=unsafe{std::mem::zeroed()};msg.msg_iov=&mut iov;msg.msg_iovlen=1;
 if !files.is_empty(){
  let raw:Vec<i32>=files.iter().map(|f|f.as_raw_fd()).collect();
  msg.msg_control=buffer.as_mut_ptr().cast();msg.msg_controllen=unsafe{libc::CMSG_SPACE((raw.len()*std::mem::size_of::<i32>())as u32)}as usize;
  unsafe{let h=libc::CMSG_FIRSTHDR(&msg);(*h).cmsg_level=libc::SOL_SOCKET;(*h).cmsg_type=libc::SCM_RIGHTS;(*h).cmsg_len=libc::CMSG_LEN((raw.len()*std::mem::size_of::<i32>())as u32)as usize;std::ptr::copy_nonoverlapping(raw.as_ptr(),libc::CMSG_DATA(h).cast::<i32>(),raw.len());}
  let sent=unsafe{libc::sendmsg(socket.as_raw_fd(),&msg,libc::MSG_NOSIGNAL)};
  if sent!=bytes.len()as isize{return Err(if sent<0{io::Error::last_os_error()}else{fail("Worker descriptor send outcome unknown")});}
 }else{
  let sent=unsafe{libc::sendmsg(socket.as_raw_fd(),&msg,libc::MSG_NOSIGNAL)};
  if sent!=bytes.len()as isize{return Err(if sent<0{io::Error::last_os_error()}else{fail("Worker packet send outcome unknown")});}
 }Ok(())
}
fn verification_worker_receive(socket:&File)->io::Result<VerificationWorkerPacket> {
 use std::os::fd::FromRawFd;verification_worker_socket(socket.as_raw_fd())?;
 let mut bytes=vec![0u8;16*1024];let mut buffer=[0usize;128];let mut iov=libc::iovec{iov_base:bytes.as_mut_ptr().cast(),iov_len:bytes.len()};
 let mut msg:libc::msghdr=unsafe{std::mem::zeroed()};msg.msg_iov=&mut iov;msg.msg_iovlen=1;msg.msg_control=buffer.as_mut_ptr().cast();msg.msg_controllen=std::mem::size_of_val(&buffer);
 let received=unsafe{libc::recvmsg(socket.as_raw_fd(),&mut msg,libc::MSG_CMSG_CLOEXEC)};
 if received<0{return Err(io::Error::last_os_error());}
 let mut files=Vec::new();let mut credentials=None;let mut invalid=false;
 unsafe{let mut h=libc::CMSG_FIRSTHDR(&msg);while !h.is_null(){
  let length=(*h).cmsg_len;let base=libc::CMSG_LEN(0)as usize;
  if length<base{invalid=true;break;}
  if (*h).cmsg_level==libc::SOL_SOCKET&&(*h).cmsg_type==libc::SCM_RIGHTS {
   let size=length-base;if size%std::mem::size_of::<i32>()!=0{invalid=true;}
   for i in 0..size/std::mem::size_of::<i32>() {let fd=std::ptr::read_unaligned(libc::CMSG_DATA(h).cast::<i32>().add(i));files.push(File::from_raw_fd(fd));}
  }else if (*h).cmsg_level==libc::SOL_SOCKET&&(*h).cmsg_type==libc::SCM_CREDENTIALS&&length==libc::CMSG_LEN(std::mem::size_of::<libc::ucred>()as u32)as usize {
   if credentials.is_some(){invalid=true;}credentials=Some(std::ptr::read_unaligned(libc::CMSG_DATA(h).cast::<libc::ucred>()));
  }else{invalid=true;}
  h=libc::CMSG_NXTHDR(&msg,h);
 }}
 if invalid||received==0||received as usize>bytes.len()||msg.msg_flags&(libc::MSG_TRUNC|libc::MSG_CTRUNC)!=0||files.len()>32{return Err(fail(&format!("Malformed/truncated worker kernel packet: bytes={received}, invalid_control={invalid}, flags={}, fd_count={}, credentials_present={}",msg.msg_flags,files.len(),credentials.is_some())));}
 bytes.truncate(received as usize);Ok(VerificationWorkerPacket{bytes,files,credentials:credentials.ok_or_else(||fail("Actual worker sender credentials missing"))?})
}
struct VerificationWorkerHandoff<'a> {selection:&'a OriginalTargetRecoverySelection<'a>,executable:File,inventory:SqliteCaptureInventory,bundle_files:Vec<File>,bundle_inventory:SqliteCaptureInventory,wire:VerificationWorkerWire,sent:bool}
impl<'a> VerificationWorkerHandoff<'a> {
 fn prepare(selection:&'a OriginalTargetRecoverySelection<'a>)->anyhow::Result<Self> {
  selection.fence()?;anyhow::ensure!(selection.owner_pager.is_none(),"Worker admission requires untouched original pre-startup target");
  let executable=File::open("/proc/self/exe")?;let md=executable.metadata()?;
  anyhow::ensure!(md.is_file()&&md.uid()==unsafe{libc::geteuid()}&&md.mode()&0o022==0&&md.mode()&0o111!=0&&md.len()<=1024*1024*1024,"Actual worker executable is not exclusively owned bounded code");
  let inventory=verification_pager_inventory(&selection.active.root)?;
  let bundle_path=selection.bundle.data_path().parent().ok_or_else(||fail("Original bundle parent absent"))?.to_owned();
  let bundle_files=selection.bundle.verification_kernel_files()?;
  let bundle_root=Root::open(&bundle_path)?;let bundle_inventory=verification_pager_inventory(&bundle_root)?;
  let wire=VerificationWorkerWire{version:2,nonce:uuid::Uuid::new_v4().to_string(),path:selection.path().to_owned(),chain_stamps:selection.active.root.chain.iter().map(verification_worker_stamp).collect::<io::Result<_>>()?,main_stamp:verification_worker_stamp(&selection.database)?,inventory_sha256:verification_worker_inventory_sha(&inventory)?,executable_stamp:verification_worker_stamp(&executable)?,executable_sha256:hash_file(&executable,md.len())?,bundle_path,bundle_stamps:bundle_files.iter().map(verification_worker_stamp).collect::<io::Result<_>>()?,bundle_inventory_sha256:verification_worker_inventory_sha(&bundle_inventory)?,original_sql_sha256:engine_transfer::VerificationSqlProjection::original(selection.bundle)?.original_source_sha256()?};
  anyhow::ensure!(wire.chain_stamps.len()+2+wire.bundle_stamps.len()<=32,"Original worker ancestry descriptor bound exceeded");
  let result=Self{selection,executable,inventory,bundle_files,bundle_inventory,wire,sent:false};result.fence()?;Ok(result)
 }
 fn fence(&self)->anyhow::Result<()> {
  self.selection.fence()?;verification_worker_inventory_fence(&self.inventory,&self.selection.active.root)?;
  verification_worker_inventory_fence(&self.bundle_inventory,&Root::open(&self.wire.bundle_path)?)?;
  for(file,stamp)in self.bundle_files.iter().zip(&self.wire.bundle_stamps){anyhow::ensure!(verification_worker_stamp(file)?==*stamp,"Original bundle kernel input changed");}
  anyhow::ensure!(verification_worker_stamp(&self.executable)?==self.wire.executable_stamp&&hash_file(&self.executable,self.executable.metadata()?.len())?==self.wire.executable_sha256,"Actual executable changed through worker launch");Ok(())
 }
 fn send_once(&mut self,socket:&File)->anyhow::Result<()> {
  self.fence()?;anyhow::ensure!(!self.sent,"Original worker descriptors already dispatched; no automatic retry");
  let bytes=serde_json::to_vec(&self.wire)?;let mut files:Vec<&File>=self.selection.active.root.chain.iter().collect();files.push(&self.selection.database);files.push(&self.executable);files.extend(self.bundle_files.iter());
  self.sent=true;verification_worker_send(socket,&bytes,&files)?;self.fence()?;Ok(())
 }
}
// Constructor is private to a received original parent's kernel channel. This
// owns initial read custody only; it cannot mint Active, a SQL owner ack or a
// verification record. Normal worker continuation must activate actual ledger
// and qualify original SQL under its separate owner window before publication.
struct InheritedVerificationBundle {root:Root,leaves:Vec<File>,inventory:SqliteCaptureInventory,stamps:Vec<String>}
impl InheritedVerificationBundle {
 fn fence(&self)->io::Result<()> {
  verification_worker_inventory_fence(&self.inventory,&self.root)?;
  for(file,stamp)in self.root.chain.iter().chain(self.leaves.iter()).zip(&self.stamps){if verification_worker_stamp(file)?!=*stamp{return Err(fail("Inherited original bundle object changed"));}}Ok(())
 }
 async fn verify(&self)->anyhow::Result<crate::storage::backup_inventory::ValidatedBundle> {
  self.fence()?;let bundle=crate::storage::backup_inventory::verify(&self.root.absolute).await?;self.fence()?;bundle.fence()?;
  anyhow::ensure!(identity(&bundle.open_owned("starlane-notes.db")?)?==identity(&self.leaves[1])?,"Child bundle pager is not original inherited main");
  self.fence()?;Ok(bundle)
 }
}
// Metadata-only observation of already admitted original kernel objects. This
// scope cannot open SQL, grant a reader/writer, or publish verification.
struct InheritedVerificationAdmission {
 root:Root,database:File,executable:File,parent_executable:File,
 bundle:Root,bundle_leaves:Vec<File>,wire:VerificationWorkerWire,parent:libc::ucred,
}
impl InheritedVerificationAdmission {
 fn fence(&self)->anyhow::Result<()> {
  anyhow::ensure!(self.parent.pid==unsafe{libc::getppid()},"Original admission parent lost");
  self.root.check()?;self.bundle.check()?;
  for(file,stamp)in self.root.chain.iter().zip(&self.wire.chain_stamps){anyhow::ensure!(verification_worker_stamp(file)?==*stamp,"Original admission ancestor exchanged");}
  for(file,stamp)in self.bundle.chain.iter().chain(self.bundle_leaves.iter()).zip(&self.wire.bundle_stamps){anyhow::ensure!(verification_worker_stamp(file)?==*stamp,"Original admission bundle exchanged");}
  anyhow::ensure!(verification_worker_stamp(&self.database)?==self.wire.main_stamp&&physical_stamp(&self.root.leaf("starlane-notes.db")?)?==physical_stamp(&self.database)?,"Original admission database exchanged");
  anyhow::ensure!(identity(&self.bundle.directory("data")?)?==identity(&self.bundle_leaves[0])?&&identity(&self.bundle.leaf("data/starlane-notes.db")?)?==identity(&self.bundle_leaves[1])?&&identity(&self.bundle.leaf("manifest.json")?)?==identity(&self.bundle_leaves[2])?,"Original admission bundle role exchanged");
  let actual=File::open("/proc/self/exe")?;let parent=File::open(format!("/proc/{}/exe",self.parent.pid))?;
  anyhow::ensure!(identity(&actual)?==identity(&self.executable)?&&identity(&parent)?==identity(&self.parent_executable)?&&verification_worker_stamp(&self.executable)?==self.wire.executable_stamp&&verification_worker_stamp(&self.parent_executable)?==self.wire.executable_stamp,"Original admission executable exchanged");Ok(())
 }
}
struct InheritedVerificationCustody {bundle:InheritedVerificationBundle,root:Root,database:File,database_identity:Identity,executable:File,parent_executable:File,inventory:SqliteCaptureInventory,wire:VerificationWorkerWire,parent:libc::ucred}
impl InheritedVerificationCustody {
 fn receive(socket:&File)->anyhow::Result<Self> {Self::receive_observed(socket,|_|Ok(()))}
 fn receive_observed(socket:&File,observe:impl FnOnce(InheritedVerificationAdmission)->anyhow::Result<()>)->anyhow::Result<Self> {
  let mut packet=verification_worker_receive(socket)?;
  anyhow::ensure!(packet.credentials.pid==unsafe{libc::getppid()}&&packet.credentials.uid==unsafe{libc::geteuid()}&&packet.credentials.gid==unsafe{libc::getegid()},"Worker packet is not from actual original parent identity");
  let wire:VerificationWorkerWire=serde_json::from_slice(&packet.bytes)?;
  anyhow::ensure!(wire.version==2&&uuid::Uuid::parse_str(&wire.nonce)?.to_string()==wire.nonce&&wire.chain_stamps.len()>=2&&wire.bundle_stamps.len()>=5&&packet.files.len()==wire.chain_stamps.len()+2+wire.bundle_stamps.len(),"Worker original descriptor envelope malformed");
  let bundle_files=packet.files.split_off(wire.chain_stamps.len()+2);
  let bundle_chain_count=wire.bundle_stamps.len()-3;
  let mut bundle_files=bundle_files;let bundle_leaves=bundle_files.split_off(bundle_chain_count);
  let bundle_root=Root{absolute:wire.bundle_path.clone(),chain:bundle_files};bundle_root.check()?;
  for(file,stamp)in bundle_root.chain.iter().chain(bundle_leaves.iter()).zip(&wire.bundle_stamps){anyhow::ensure!(verification_worker_stamp(file)?==*stamp,"Inherited original bundle stamp differs");}
  anyhow::ensure!(bundle_leaves[0].metadata()?.is_dir()&&bundle_leaves[1].metadata()?.is_file()&&bundle_leaves[2].metadata()?.is_file(),"Inherited bundle object roles differ");
  anyhow::ensure!(identity(&bundle_root.directory("data")?)?==identity(&bundle_leaves[0])?&&identity(&bundle_root.leaf("data/starlane-notes.db")?)?==identity(&bundle_leaves[1])?&&identity(&bundle_root.leaf("manifest.json")?)?==identity(&bundle_leaves[2])?,"Inherited bundle path/exact objects differ");
  let executable=packet.files.pop().ok_or_else(||fail("Held executable absent"))?;let database=packet.files.pop().ok_or_else(||fail("Held database absent"))?;
  let root=Root{absolute:wire.path.clone(),chain:packet.files};root.check()?;
  for(file,expected)in root.chain.iter().zip(&wire.chain_stamps){anyhow::ensure!(file.metadata()?.is_dir()&&verification_worker_stamp(file)?==*expected,"Inherited original ancestor changed");}
  anyhow::ensure!(database.metadata()?.is_file()&&verification_worker_stamp(&database)?==wire.main_stamp&&physical_stamp(&root.leaf("starlane-notes.db")?)?==physical_stamp(&database)?,"Inherited original database/path differs");
  let actual_executable=File::open("/proc/self/exe")?;
  let parent_executable=File::open(format!("/proc/{}/exe",packet.credentials.pid))?;
  anyhow::ensure!(identity(&parent_executable)?==identity(&executable)?&&verification_worker_stamp(&parent_executable)?==wire.executable_stamp,"Original parent is not the same held actual worker code");
  anyhow::ensure!(identity(&actual_executable)?==identity(&executable)?&&verification_worker_stamp(&executable)?==wire.executable_stamp,"Child did not execute held original executable");
  let admission=InheritedVerificationAdmission{
   root:Root{absolute:root.absolute.clone(),chain:root.chain.iter().map(File::try_clone).collect::<io::Result<_>>()?},database:database.try_clone()?,executable:executable.try_clone()?,parent_executable:parent_executable.try_clone()?,
   bundle:Root{absolute:bundle_root.absolute.clone(),chain:bundle_root.chain.iter().map(File::try_clone).collect::<io::Result<_>>()?},bundle_leaves:bundle_leaves.iter().map(File::try_clone).collect::<io::Result<_>>()?,
   wire:serde_json::from_slice(&packet.bytes)?,parent:packet.credentials,
  };admission.fence()?;observe(admission)?;
  // Every original content check remains mandatory before InitialAck.
  anyhow::ensure!(hash_file(&executable,executable.metadata()?.len())?==wire.executable_sha256,"Child held original executable bytes differ");
  let bundle_inventory=verification_pager_inventory(&bundle_root)?;anyhow::ensure!(verification_worker_inventory_sha(&bundle_inventory)?==wire.bundle_inventory_sha256,"Inherited bundle full inventory differs");
  let bundle=InheritedVerificationBundle{root:bundle_root,leaves:bundle_leaves,inventory:bundle_inventory,stamps:wire.bundle_stamps.clone()};bundle.fence()?;
  let inventory=verification_pager_inventory(&root)?;anyhow::ensure!(verification_worker_inventory_sha(&inventory)?==wire.inventory_sha256,"Actual original target inventory changed before child admission");
  let database_identity=identity(&database)?;
  let result=Self{bundle,root,database,database_identity,executable,parent_executable,inventory,wire,parent:packet.credentials};result.fence()?;Ok(result)
 }
 fn io_fence(&self)->io::Result<()> {
  if self.parent.pid!=unsafe{libc::getppid()}{return Err(fail("Original worker parent was lost"));}self.root.check()?;
  for(file,expected)in self.root.chain.iter().zip(&self.wire.chain_stamps){if verification_worker_stamp(file)?!=*expected{return Err(fail("Inherited original ancestor changed"));}}
  if verification_worker_stamp(&self.database)?!=self.wire.main_stamp||physical_stamp(&self.root.leaf("starlane-notes.db")?)?!=physical_stamp(&self.database)?{return Err(fail("Inherited original main changed"));}Ok(())
 }
 fn fence(&self)->anyhow::Result<()> {
  self.bundle.fence()?;
  anyhow::ensure!(self.parent.pid==unsafe{libc::getppid()},"Original worker parent was lost");self.root.check()?;verification_worker_inventory_fence(&self.inventory,&self.root)?;
  for(file,expected)in self.root.chain.iter().zip(&self.wire.chain_stamps){anyhow::ensure!(verification_worker_stamp(file)?==*expected,"Inherited worker ancestor stamp changed");}
  anyhow::ensure!(verification_worker_stamp(&self.database)?==self.wire.main_stamp&&physical_stamp(&self.root.leaf("starlane-notes.db")?)?==physical_stamp(&self.database)?,"Inherited worker database changed");
  anyhow::ensure!(verification_worker_stamp(&self.executable)?==self.wire.executable_stamp,"Inherited worker executable changed");
  let current_parent_executable=File::open(format!("/proc/{}/exe",self.parent.pid))?;
  anyhow::ensure!(identity(&current_parent_executable)?==identity(&self.parent_executable)?&&verification_worker_stamp(&self.parent_executable)?==self.wire.executable_stamp,"Original parent executable changed");Ok(())
 }
}

#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct VerificationWorkerInitialAck {nonce:String,phase:String,database_sha256:String,original_sql_sha256:String}
#[cfg(test)]
impl VerificationWorkerHandoff<'_> {
 async fn actual_queued_refusal(&mut self,mode:u8,foreign:Option<&File>)->anyhow::Result<()> {
  use std::os::unix::process::CommandExt;use std::process::Stdio;
  self.fence()?;let(parent,child)=verification_worker_pair()?;let socket_fd=child.as_raw_fd();let executable_fd=self.executable.as_raw_fd();
  let mut command=std::process::Command::new(format!("/proc/self/fd/{executable_fd}"));
  command.args(["verification_inherited_initial_child_phase","--ignored","--test-threads=1","--nocapture"]).env("STARLANE_PRIVATE_VERIFICATION_CHANNEL_FD",socket_fd.to_string()).stdout(Stdio::piped()).stderr(Stdio::piped());
  unsafe{command.pre_exec(move||{for fd in [socket_fd,executable_fd]{let flags=libc::fcntl(fd,libc::F_GETFD);if flags<0||libc::fcntl(fd,libc::F_SETFD,flags&!libc::FD_CLOEXEC)<0{return Err(io::Error::last_os_error());}}Ok(())});}
  if mode==13 {
   self.send_once(&parent)?;let refused=std::process::Command::new("/nonexistent-starlane-verification-worker-code").spawn();
   anyhow::ensure!(refused.is_err()&&self.sent&&self.send_once(&parent).is_err(),"Failed spawn reopened one-use original handoff");self.fence()?;return Ok(());
  }
  let mut bytes=serde_json::to_value(&self.wire)?;let mut files:Vec<&File>=self.selection.active.root.chain.iter().collect();files.push(&self.selection.database);files.push(&self.executable);files.extend(self.bundle_files.iter());
  match mode {10=>{bytes["version"]=serde_json::json!(99);},11=>{files.remove(self.wire.chain_stamps.len());},12=>{files[self.wire.chain_stamps.len()]=foreign.ok_or_else(||fail("Actual foreign byte-equal main FD absent"))?;},_=>return Err(fail("Unknown queued negative mode").into())}
  self.sent=true;verification_worker_send(&parent,&serde_json::to_vec(&bytes)?,&files)?;self.fence()?;
  let process=command.spawn()?;drop(child);let output=tokio::task::spawn_blocking(move||process.wait_with_output()).await??;
  anyhow::ensure!(!output.status.success()&&!String::from_utf8_lossy(&output.stderr).contains("ACTUAL_FRESH_EXECUTABLE_INHERITED_KERNEL_READ_AND_NATIVE_ACTIVATION_PASS"),"Queued malformed/foreign admission acquired read authority");
  anyhow::ensure!(self.send_once(&parent).is_err(),"Queued refused handoff became retry authority");self.fence()?;Ok(())
 }
 async fn actual_child_initial_read(&mut self)->anyhow::Result<()> {
  use std::os::unix::process::CommandExt;use std::process::Stdio;
  self.fence()?;let(parent,child)=verification_worker_pair()?;let socket_fd=child.as_raw_fd();let executable_fd=self.executable.as_raw_fd();
  let mut command=std::process::Command::new(format!("/proc/self/fd/{executable_fd}"));
  command.args(["verification_inherited_initial_child_phase","--ignored","--test-threads=1","--nocapture"])
   .env("STARLANE_PRIVATE_VERIFICATION_CHANNEL_FD",socket_fd.to_string()).stdout(Stdio::piped()).stderr(Stdio::piped());
  unsafe{command.pre_exec(move||{
   for fd in [socket_fd,executable_fd] {let flags=libc::fcntl(fd,libc::F_GETFD);if flags<0||libc::fcntl(fd,libc::F_SETFD,flags&!libc::FD_CLOEXEC)<0{return Err(io::Error::last_os_error());}}Ok(())
  });}
  let mut process=command.spawn()?;drop(child);let pid=process.id()as i32;
  let admitted=(||->anyhow::Result<()> {
   self.send_once(&parent)?;anyhow::ensure!(self.send_once(&parent).is_err(),"Original kernel handoff must not retry");
   let packet=verification_worker_receive(&parent)?;
   anyhow::ensure!(packet.files.is_empty()&&packet.credentials.pid==pid&&packet.credentials.uid==unsafe{libc::geteuid()}&&packet.credentials.gid==unsafe{libc::getegid()},"Admission ack belongs to another actual process identity");
   let ack:VerificationWorkerInitialAck=serde_json::from_slice(&packet.bytes)?;
   anyhow::ensure!(ack.nonce==self.wire.nonce&&ack.phase=="initial-read-admitted"&&ack.database_sha256==self.inventory.files["starlane-notes.db"].2&&ack.original_sql_sha256==self.wire.original_sql_sha256,"Child initial read is not bound to original kernel handoff");self.fence()?;Ok(())
  })();
  if admitted.is_err(){let _=process.kill();}
  let output=process.wait_with_output()?;
  if !output.status.success(){return Err(anyhow::anyhow!("Actual inherited initial child failed (code {:?}): {}",output.status.code(),String::from_utf8_lossy(&output.stderr)));}
  admitted?;self.fence()?;Ok(())
 }
}
#[cfg(test)]
mod verification_inherited_initial_controls {
 use super::*;
 #[test]
 fn verification_kernel_channel_refuses_same_process_sender_before_sql_authority()->anyhow::Result<()> {
  let(left,right)=verification_worker_pair()?;
  verification_worker_send(&left,b"{}",&[])?;
  let rejected=InheritedVerificationCustody::receive(&right).err().ok_or_else(||fail("Foreign process context acquired custody"))?;
  anyhow::ensure!(rejected.to_string().contains("actual original parent identity"),"Peer refusal must precede wire/path/SQLite admission: {rejected}");Ok(())
 }
 #[test]
 fn verification_kernel_channel_bounds_and_actual_regular_file_refuse()->anyhow::Result<()> {
  let(left,_right)=verification_worker_pair()?;
  anyhow::ensure!(verification_worker_send(&left,&vec![0;16*1024+1],&[]).is_err(),"Oversized kernel frame must refuse before send");
  anyhow::ensure!(verification_worker_send(&left,b"x",&vec![&left;33]).is_err(),"Excess inherited objects must refuse before send");
  let ordinary=File::open("/dev/null")?;
  anyhow::ensure!(verification_worker_socket(ordinary.as_raw_fd()).is_err(),"Ordinary file cannot stand in for private kernel channel");Ok(())
 }

 #[tokio::test]
 #[ignore="one-use original parent kernel descriptor channel only"]
 async fn verification_inherited_initial_child_phase()->anyhow::Result<()> {
  use std::os::fd::FromRawFd;
  let Some(raw)=std::env::var_os("STARLANE_PRIVATE_VERIFICATION_CHANNEL_FD")else{return Ok(())};
  let fd=raw.to_str().ok_or_else(||fail("Private channel number invalid"))?.parse::<i32>()?;
  anyhow::ensure!(fd>=3,"Standard descriptors cannot serve as inherited authority");
  let channel=unsafe{File::from_raw_fd(fd)};let custody=Arc::new(InheritedVerificationCustody::receive(&channel)?);
  // This real fresh executable's initial data opener is admitted before any
  // ordinary pathname SQL pool. It is not a full normal application worker.
  std::env::set_var("DATA_DIR",&custody.root.absolute);sqlx::any::install_default_drivers();
  let reader=VerificationSealedReader::enter_inherited(custody.clone()).await?;
  let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE id='genuine-page'").fetch_one(&reader.pool).await?;
  anyhow::ensure!(count==1,"Fresh actual inherited reader lost original canonical page");reader.fence()?;custody.fence()?;
  let bundle=custody.bundle.verify().await?;
  // Actual process-local native ledger/SQL activation, never a copied Active.
  activate_in(&custody.root.absolute,&reader.pool).await.map_err(|e|anyhow::anyhow!("Inherited actual native activation refused: {e:?}"))?;
  reader.fence()?;let(_,active)=registered(&custody.root.absolute)?;let active=active.ok_or_else(||fail("Actual inherited native activation absent"))?;active.check()?;
  anyhow::ensure!(identity(active.root.fd())?==identity(custody.root.fd())?,"Child activation belongs to another root");
  anyhow::ensure!(active.ledger.backup_id==bundle.manifest.backup_id&&active.ledger.manifest_sha256==digest(&bundle.raw_manifest),"Actual child activation is not original inherited bundle");
  let projection=engine_transfer::VerificationSqlProjection::original(&bundle)?;
  let mut reservation=VerificationReservation::begin(&reader.pool).await?;
  reader.fence()?;custody.fence()?;
  let closure=projection.compare_exact(reservation.connection()).await?;
  anyhow::ensure!(closure.source_sha256()==custody.wire.original_sql_sha256&&closure.target_sha256()==custody.wire.original_sql_sha256,"Actual inherited target original SQL differs");
  reader.fence()?;custody.fence()?;reservation.rollback().await?;
  bundle.db.close().await;bundle.fence()?;custody.bundle.fence()?;
  let ack=VerificationWorkerInitialAck{nonce:custody.wire.nonce.clone(),phase:"initial-read-admitted".into(),database_sha256:hash_file(&custody.database,custody.database.metadata()?.len())?,original_sql_sha256:closure.source_sha256().to_owned()};
  verification_worker_send(&channel,&serde_json::to_vec(&ack)?,&[])?;reader.close().await?;custody.fence()?;
  eprintln!("ACTUAL_FRESH_EXECUTABLE_INHERITED_KERNEL_READ_AND_NATIVE_ACTIVATION_PASS; normal_application_worker=false; complete_verified_record=false");Ok(())
 }
}
