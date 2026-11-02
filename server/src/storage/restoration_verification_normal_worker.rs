include!("restoration_verification_unpublished.rs");
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NormalVerificationSuppliedCredentials {producer:String,audiences:Vec<String>}
fn normal_verification_supplied_input(mut input:String)->anyhow::Result<NormalVerificationSuppliedCredentials> {
 anyhow::ensure!(input.len()<=82*1024,"Supplied verification credentials exceed bounded input");
 if input.ends_with('\n'){input.pop();if input.ends_with('\r'){input.pop();}}
 if input.trim_start().starts_with('{') {
  let supplied:NormalVerificationSuppliedCredentials=serde_json::from_str(&input)?;
  anyhow::ensure!((1..=8).contains(&supplied.audiences.len()),"Structured verification requires1..8 genuine audience samples");
  Ok(supplied)
 }else{Ok(NormalVerificationSuppliedCredentials{producer:input,audiences:Vec::new()})}
}
// Private diagnostic producer. This runs the real normal executable and real
// startup owners, but cannot publish a completed verification record.
#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NormalVerificationStart {nonce:String,derived:PathBuf,stamps:Vec<String>}
#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NormalVerificationReady {nonce:String,address:std::net::SocketAddr,original_sql:String,qualified_sql:String,target_sql:String}
#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NormalVerificationControl {nonce:String,phase:NormalVerificationControlPhase}
#[derive(serde::Serialize,serde::Deserialize)]
enum NormalVerificationControlPhase {Continue,Finish}
fn normal_parent_progress(channel:&File,nonce:&str)->anyhow::Result<()> {verification_worker_send(channel,&serde_json::to_vec(&NormalVerificationControl{nonce:nonce.into(),phase:NormalVerificationControlPhase::Continue})?,&[])?;Ok(())}
#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NormalStartupProgress {nonce:String,phase:u8,sequence:u64,root:Identity,database:Identity}
fn normal_validate_startup_progress(p:&NormalStartupProgress,nonce:&str,root:Identity,database:Identity,last_sequence:u64,last_phase:u8)->anyhow::Result<()> {
 anyhow::ensure!(p.nonce==nonce&&p.root==root&&p.database==database&&p.sequence==last_sequence+1&&p.sequence<=600&&p.phase>=1&&p.phase>=last_phase&&p.phase<=last_phase+1&&p.phase<=3,"Foreign/replayed startup progress refused");Ok(())
}
async fn normal_admit_startup_progress(packet:&VerificationWorkerPacket,pid:i32,nonce:&str,root:Identity,database:Identity,last_sequence:u64,last_phase:u8,original:&mut StoppedVerificationSource<'_>,plan:&OriginalApplicationPlan<'_>)->anyhow::Result<NormalStartupProgress> {
 anyhow::ensure!(packet.credentials.pid==pid&&packet.credentials.uid==unsafe{libc::geteuid()}&&packet.credentials.gid==unsafe{libc::getegid()}&&packet.files.is_empty(),"Startup observation belongs to foreign child");
 let progress:NormalStartupProgress=serde_json::from_slice(&packet.bytes)?;
 normal_validate_startup_progress(&progress,nonce,root,database,last_sequence,last_phase)?;
 original.fence().await?;plan.fence(original)?;Ok(progress)
}
#[cfg(test)]
fn normal_actual_progress_packet(nonce:&str,root:Identity,database:Identity,replay:bool)->anyhow::Result<(VerificationWorkerPacket,VerificationWorkerPacket,i32)> {
 let first=serde_json::to_vec(&NormalStartupProgress{nonce:"original".into(),phase:1,sequence:1,root,database})?;let bytes=serde_json::to_vec(&NormalStartupProgress{nonce:nonce.into(),phase:1,sequence:if replay{1}else{2},root,database})?;
 normal_actual_wire_packets(&first,&bytes)
}
#[cfg(test)]
fn normal_actual_wire_packets(first:&[u8],bytes:&[u8])->anyhow::Result<(VerificationWorkerPacket,VerificationWorkerPacket,i32)> {
 use std::os::unix::process::CommandExt;
 let(parent,child)=verification_worker_pair()?;let fd=child.as_raw_fd();
 // Test transport uses the same verified installed Python3 selection as the
 // capture owner; do not assume a distro-specific minor-version pathname.
 let(python,_held_python,_stamp,_sha)=sqlite_capture_python()?;
 let mut command=std::process::Command::new(python);command.arg("-I");
 command.args(["-c","import socket,sys; s=socket.socket(fileno=int(sys.argv[1])); s.send(sys.argv[2].encode()); s.send(sys.argv[3].encode())",&fd.to_string(),std::str::from_utf8(&first)?,std::str::from_utf8(&bytes)?]);
 unsafe{command.pre_exec(move||{let flags=libc::fcntl(fd,libc::F_GETFD);if flags<0||libc::fcntl(fd,libc::F_SETFD,flags&!libc::FD_CLOEXEC)<0{return Err(io::Error::last_os_error());}Ok(())});}
 let mut process=command.spawn()?;let pid=process.id()as i32;drop(child);
 let first=verification_worker_receive(&parent)?;let packet=verification_worker_receive(&parent)?;anyhow::ensure!(process.wait()?.success(),"Actual progress sender failed");Ok((first,packet,pid))
}
fn normal_startup_progress(channel:&File,nonce:&str,phase:u8,sequence:&mut u64,root:Identity,database:Identity)->anyhow::Result<()> {
 anyhow::ensure!((1..=3).contains(&phase)&&*sequence<600,"Startup progress bounds exceeded");*sequence+=1;
 verification_worker_send(channel,&serde_json::to_vec(&NormalStartupProgress{nonce:nonce.into(),phase,sequence:*sequence,root,database})?,&[])?;Ok(())
}
async fn normal_owned_startup_phase<T>(channel:&File,nonce:&str,phase:u8,sequence:&mut u64,root:Identity,database:Identity,check:impl Fn()->anyhow::Result<()>,observation:impl std::future::Future<Output=anyhow::Result<T>>)->anyhow::Result<T> {
 tokio::pin!(observation);let mut pulse=tokio::time::interval(std::time::Duration::from_secs(3));pulse.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
 loop{tokio::select!{
  result=&mut observation=>{let value=result?;check()?;normal_startup_progress(channel,nonce,phase,sequence,root,database)?;return Ok(value);},
  _=pulse.tick()=>{check()?;normal_startup_progress(channel,nonce,phase,sequence,root,database)?;}
 }}
}
// A distinct pre-ACK observation, never a Ready message or authority grant.
#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NormalAdmissionProgress {nonce:String,admission_phase:u8,sequence:u64,root:Identity,database:Identity}
fn normal_admission_send(channel:&File,bytes:&[u8])->io::Result<()> {
 if bytes.is_empty()||bytes.len()>16*1024{return Err(fail("Original admission packet exceeds bound"));}
 // Progress must never strand its owned thread when the parent stops reading.
 // This purpose-only send carries no descriptors and does not alter controls.
 let sent=unsafe{libc::send(channel.as_raw_fd(),bytes.as_ptr().cast(),bytes.len(),libc::MSG_NOSIGNAL|libc::MSG_DONTWAIT)};
 if sent!=bytes.len()as isize{return Err(if sent<0{io::Error::last_os_error()}else{fail("Original admission progress send incomplete")});}Ok(())
}
fn normal_validate_admission_progress(p:&NormalAdmissionProgress,nonce:&str,root:Identity,database:Identity,last:u64)->anyhow::Result<()> {
 anyhow::ensure!(p.nonce==nonce&&p.admission_phase==1&&p.sequence==last+1&&p.sequence<=600&&p.root==root&&p.database==database,"Foreign/replayed original admission progress refused");Ok(())
}
fn normal_admit_native_packet(packet:&VerificationWorkerPacket,pid:i32,nonce:&str,root:Identity,database:Identity,last:u64)->anyhow::Result<NormalAdmissionProgress> {
 anyhow::ensure!(packet.credentials.pid==pid&&packet.credentials.uid==unsafe{libc::geteuid()}&&packet.credentials.gid==unsafe{libc::getegid()}&&packet.files.is_empty(),"Original native admission belongs to foreign child");
 let progress:NormalAdmissionProgress=serde_json::from_slice(&packet.bytes)?;normal_validate_admission_progress(&progress,nonce,root,database,last)?;Ok(progress)
}
struct NormalAdmissionPulse {stop:std::sync::mpsc::Sender<()>,thread:Option<std::thread::JoinHandle<anyhow::Result<()>>>}
impl NormalAdmissionPulse {
 fn begin(channel:&File,original:InheritedVerificationAdmission)->anyhow::Result<Self> {
  original.fence()?;let channel=channel.try_clone()?;let(stop,rx)=std::sync::mpsc::channel();
  let thread=std::thread::spawn(move||->anyhow::Result<()> {
   let mut sequence=0u64;
   loop {
    original.fence()?;sequence+=1;anyhow::ensure!(sequence<=600,"Original admission observation exhausted");
    normal_admission_send(&channel,&serde_json::to_vec(&NormalAdmissionProgress{nonce:original.wire.nonce.clone(),admission_phase:1,sequence,root:identity(original.root.fd())?,database:identity(&original.database)?})?)?;
    match rx.recv_timeout(std::time::Duration::from_secs(3)){Ok(())|Err(std::sync::mpsc::RecvTimeoutError::Disconnected)=>return Ok(()),Err(std::sync::mpsc::RecvTimeoutError::Timeout)=>{}}
   }
  });Ok(Self{stop,thread:Some(thread)})
 }
 fn finish(mut self)->anyhow::Result<()> {let _=self.stop.send(());self.thread.take().ok_or_else(||fail("Original admission observer absent"))?.join().map_err(|_|fail("Original admission observer panicked"))??;Ok(())}
}
impl Drop for NormalAdmissionPulse {fn drop(&mut self){let _=self.stop.send(());if let Some(thread)=self.thread.take(){let _=thread.join();}}}

// The channel still refuses 15s of genuinely unobserved silence. Polling lets
// the parent retain current-source/current-human checks before the exact ACK.
async fn normal_initial_ack(channel:&File,process:&mut std::process::Child,handoff:&VerificationWorkerHandoff<'_>,selection:&OriginalTargetRecoverySelection<'_>,source:&mut StoppedVerificationSource<'_>,plan:&OriginalApplicationPlan<'_>,deadline:std::time::Instant)->anyhow::Result<VerificationWorkerInitialAck> {
 let mut last=0u64;let mut heard=std::time::Instant::now();let mut next_fence=std::time::Instant::now();
 loop {
  anyhow::ensure!(std::time::Instant::now()<deadline,"Original normal admission exceeded total bound");
  anyhow::ensure!(process.try_wait()?.is_none(),"Original normal child exited during admission");
  if std::time::Instant::now()>=next_fence{source.fence().await?;plan.fence(source)?;next_fence=std::time::Instant::now()+std::time::Duration::from_secs(3);}
  let mut poll=libc::pollfd{fd:channel.as_raw_fd(),events:libc::POLLIN,revents:0};let ready=unsafe{libc::poll(&mut poll,1,0)};
  if ready<0{return Err(io::Error::last_os_error().into());}
  if ready>0 {
   let packet=verification_worker_receive(channel)?;
   anyhow::ensure!(packet.credentials.pid==process.id()as i32&&packet.credentials.uid==unsafe{libc::geteuid()}&&packet.credentials.gid==unsafe{libc::getegid()}&&packet.files.is_empty(),"Original admission belongs to foreign child");
   if serde_json::from_slice::<NormalAdmissionProgress>(&packet.bytes).is_ok(){
    let progress=normal_admit_native_packet(&packet,process.id()as i32,&handoff.wire.nonce,identity(selection.active.root.fd())?,identity(&selection.database)?,last)?;
    source.fence().await?;plan.fence(source)?;last=progress.sequence;heard=std::time::Instant::now();continue;
   }
   let ack:VerificationWorkerInitialAck=serde_json::from_slice(&packet.bytes)?;anyhow::ensure!(last>0,"Initial ACK skipped original native admission observation");return Ok(ack);
  }
  anyhow::ensure!(heard.elapsed()<std::time::Duration::from_secs(15),"Initial normal child receive exceeded15s unobserved silence");
  tokio::time::sleep(std::time::Duration::from_millis(20)).await;
 }
}
struct NormalPendingSeal {state:Arc<VerificationVfsState>,held:File,stamp:PhysicalStamp,bytes:crate::storage::backup_inventory::Bytes}
impl NormalPendingSeal {
 fn held(seal:&VerificationWriterSeal)->anyhow::Result<Self>{seal.reader_ready()?;Ok(Self{state:seal.state.clone(),held:seal.held.try_clone()?,stamp:seal.stamp.clone(),bytes:seal.bytes.clone()})}
 fn fence(&self)->anyhow::Result<()> {self.state.ancestry()?;let current=open_at(&self.state.root,"starlane-notes.db",libc::O_RDONLY)?;anyhow::ensure!(physical_stamp(&self.held)?==self.stamp&&physical_stamp(&current)?==self.stamp&&planned_hash(&self.held)?==self.bytes&&planned_hash(&current)?==self.bytes,"Pending original sealed data changed");Ok(())}
}
struct NormalVerificationDerived {root:Root,stamps:Vec<String>,logging:File,search:File}
impl NormalVerificationDerived {
 fn receive(channel:&File,custody:&InheritedVerificationCustody)->anyhow::Result<Self> {
  let packet=verification_worker_receive(channel)?;
  anyhow::ensure!(packet.credentials.pid==custody.parent.pid&&packet.credentials.uid==custody.parent.uid&&packet.credentials.gid==custody.parent.gid,"Normal startup permission is not from original parent");
  let start:NormalVerificationStart=serde_json::from_slice(&packet.bytes)?;
  anyhow::ensure!(start.nonce==custody.wire.nonce&&start.stamps.len()>=2&&packet.files.len()==start.stamps.len(),"Normal worker derived descriptor packet malformed");
  let root=Root{absolute:start.derived,chain:packet.files};root.check()?;
  anyhow::ensure!(!root.absolute.starts_with(&custody.root.absolute)&&!root.absolute.starts_with(&custody.bundle.root.absolute),"Derived outputs overlap canonical target/bundle");
  for(file,stamp)in root.chain.iter().zip(&start.stamps){anyhow::ensure!(verification_worker_stamp(file)?==*stamp,"Original derived directory exchanged");}
  let logging=root.directory("logging")?;let search=root.directory("search")?;
  anyhow::ensure!(std::fs::read_dir(root.absolute.join("logging"))?.next().is_none()&&std::fs::read_dir(root.absolute.join("search"))?.next().is_none(),"Derived writer requires genuinely new empty output roles");
  let result=Self{root,stamps:start.stamps,logging,search};result.fence()?;Ok(result)
 }
 fn fence(&self)->io::Result<()> {
  self.root.check()?;
  for(file,stamp)in self.root.chain.iter().zip(&self.stamps){if verification_worker_stamp(file)?!=*stamp{return Err(fail("Normal derived original ancestor changed"));}}
  if identity(&self.logging)?!=identity(&self.root.directory("logging")?)?||identity(&self.search)?!=identity(&self.root.directory("search")?)?{return Err(fail("Normal derived output role exchanged"));}Ok(())
 }
 fn log(&self)->io::Result<NormalVerificationLog> {
  self.fence()?;
  let name=std::ffi::CString::new("normal-worker.log")?;
  let fd=unsafe{libc::openat(self.logging.as_raw_fd(),name.as_ptr(),libc::O_WRONLY|libc::O_CREAT|libc::O_EXCL|libc::O_NOFOLLOW|libc::O_CLOEXEC,0o600)};
  if fd<0{return Err(io::Error::last_os_error());}
  let file=unsafe{File::from_raw_fd(fd)};self.fence()?;
  Ok(NormalVerificationLog(Arc::new(std::sync::Mutex::new((file,0)))))
 }
}
#[derive(Clone)]
struct NormalVerificationLog(Arc<std::sync::Mutex<(File,u64)>>);
impl std::io::Write for NormalVerificationLog {
 fn write(&mut self,bytes:&[u8])->io::Result<usize>{
  use std::io::Write;let mut held=self.0.lock().map_err(|_|fail("Normal log poisoned"))?;
  if held.1.checked_add(bytes.len()as u64).filter(|n|*n<=8*1024*1024).is_none(){return Err(fail("Bounded normal verification log full"));}
  let n=held.0.write(bytes)?;held.1+=n as u64;Ok(n)
 }
 fn flush(&mut self)->io::Result<()>{self.0.lock().map_err(|_|fail("Normal log poisoned"))?.0.sync_all()}
}
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for NormalVerificationLog {type Writer=Self;fn make_writer(&'a self)->Self {self.clone()}}

pub(crate) async fn run_inherited_normal_verification_worker(fd:i32)->anyhow::Result<()> {
 use std::os::fd::FromRawFd;use tracing_subscriber::prelude::*;
 anyhow::ensure!(fd>=3,"Standard descriptors cannot establish worker authority");
 let channel=unsafe{File::from_raw_fd(fd)};eprintln!("NORMAL_WORKER_PHASE kernel-receive fd={fd}");let mut admission_pulse=None;let custody=Arc::new(InheritedVerificationCustody::receive_observed(&channel,|original|{admission_pulse=Some(NormalAdmissionPulse::begin(&channel,original)?);Ok(())})?);eprintln!("NORMAL_WORKER_PHASE original-kernel-admitted");
 std::env::set_var("DATA_DIR",&custody.root.absolute);sqlx::any::install_default_drivers();
 let initial=VerificationSealedReader::enter_inherited(custody.clone()).await?;
 let bundle=custody.bundle.verify().await?;eprintln!("NORMAL_WORKER_PHASE original-bundle-admitted");
 activate_in(&custody.root.absolute,&initial.pool).await.map_err(|e|anyhow::anyhow!("Normal worker original target activation refused: {e:?}"))?;
 let projection=engine_transfer::VerificationSqlProjection::original(&bundle)?;
 let mut reserved=VerificationReservation::begin(&initial.pool).await?;
 let raw=projection.compare_exact(reserved.connection()).await?;
 anyhow::ensure!(raw.source_sha256()==custody.wire.original_sql_sha256&&raw.target_sha256()==custody.wire.original_sql_sha256,"Normal worker original SQL differs");
 reserved.rollback().await?;custody.fence()?;
 let mut selected=OriginalTargetRecoverySelection::bind(&bundle,&custody.root.absolute,&initial.pool).await?;
 drop(selected._anchor.take());initial.close().await?;selected.fence()?;custody.fence()?;
 let ack=VerificationWorkerInitialAck{nonce:custody.wire.nonce.clone(),phase:"initial-read-admitted".into(),database_sha256:hash_file(&custody.database,custody.database.metadata()?.len())?,original_sql_sha256:raw.source_sha256().to_owned()};
 admission_pulse.take().ok_or_else(||fail("Original native admission observer missing"))?.finish()?;
 verification_worker_send(&channel,&serde_json::to_vec(&ack)?,&[])?;
 let mut startup_sequence=0u64;let startup_root=identity(custody.root.fd())?;let startup_database=custody.database_identity;
 eprintln!("NORMAL_WORKER_PHASE initial-ack-sent");let derived=NormalVerificationDerived::receive(&channel,&custody)?;eprintln!("NORMAL_WORKER_PHASE original-derived-admitted");custody.fence()?;derived.fence()?;
 let log=derived.log()?;
 tracing_subscriber::registry().with(tracing_subscriber::fmt::layer().with_ansi(false).with_writer(log)).try_init()?;
 tracing::info!("Original inherited normal worker startup begins");
 eprintln!("NORMAL_WORKER_PHASE logging-initialized");let writer=VerificationRecoveryWriter::enter(selected).await?;eprintln!("NORMAL_WORKER_PHASE startup-writer-entered");writer.fence()?;derived.fence()?;
 let recovered=normal_owned_startup_phase(&channel,&custody.wire.nonce,1,&mut startup_sequence,startup_root,startup_database,||{writer.fence()?;derived.fence()?;Ok(())},async{Ok(crate::startup_recovery::recover_initialized(writer.selection().pool(),Some(writer.selection())).await?.ok_or_else(||fail("Actual normal publication acknowledgment absent"))?)}).await?;
 eprintln!("NORMAL_WORKER_PHASE ordered-recovery-complete");let mut reserved=VerificationReservation::begin(writer.selection().pool()).await?;
 let target=verification_target(&bundle,&custody.root.absolute,writer.selection().pool(),&mut reserved).await?;
 let seal=normal_owned_startup_phase(&channel,&custody.wire.nonce,2,&mut startup_sequence,startup_root,startup_database,||{writer.fence()?;derived.fence()?;Ok(())},writer.seal(&target,&mut reserved,&projection,&recovered)).await?;
 drop(target);reserved.rollback().await?;drop(recovered);
 let sealed=writer.close(seal).await?;eprintln!("NORMAL_WORKER_PHASE writer-closed-sealed");sealed.reader_ready()?;derived.fence()?;
 let ready=NormalVerificationReady{nonce:custody.wire.nonce.clone(),address:"127.0.0.1:0".parse()?,original_sql:sealed.original_sql_sha256.clone(),qualified_sql:sealed.qualified_sql_sha256.clone(),target_sql:sealed.target_sql_sha256.clone()};
 let pending_seal=NormalPendingSeal::held(&sealed)?;let reader=normal_owned_startup_phase(&channel,&custody.wire.nonce,3,&mut startup_sequence,startup_root,startup_database,||{pending_seal.fence()?;derived.fence()?;Ok(())},VerificationSealedReader::enter(sealed)).await?;reader.fence()?;drop(pending_seal);
 let jwt_secret=std::env::var("JWT_SECRET").map_err(|_|fail("Actual configured worker authentication secret absent"))?;
 anyhow::ensure!(jwt_secret.len()>=32,"Actual configured worker authentication secret weak");
 let search_index=crate::search::SearchIndex::new(derived.root.absolute.join("search"))?;derived.fence()?;reader.fence()?;
 let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await?;
 let address=listener.local_addr()?;let base_url=format!("http://{address}");
 let state=crate::AppState{db:reader.pool.clone(),jwt_secret,base_url:base_url.clone(),search_index,oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};
 let app=crate::registered_application(state,&base_url)?;
 derived.fence()?;reader.fence()?;
 let original_listener=normal_duplicate_listener(listener.as_raw_fd())?;
 eprintln!("NORMAL_WORKER_PHASE tcp-ready-before-send");verification_worker_send(&channel,&serde_json::to_vec(&NormalVerificationReady{address,..ready})?,&[&original_listener])?;drop(original_listener);eprintln!("NORMAL_WORKER_PHASE tcp-ready-sent");
 let shutdown_channel=channel.try_clone()?;let expected=custody.parent;let nonce=custody.wire.nonce.clone();
 let (outcome_tx,outcome_rx)=tokio::sync::oneshot::channel::<Result<(),String>>();
 let shutdown=async move {
  let result=tokio::task::spawn_blocking(move||->anyhow::Result<()> {
   let deadline=std::time::Instant::now()+std::time::Duration::from_secs(1800);
   loop {
    anyhow::ensure!(std::time::Instant::now()<deadline,"Bounded normal observation window expired");
    let p=verification_worker_receive(&shutdown_channel)?;
    anyhow::ensure!(p.credentials.pid==expected.pid&&p.credentials.uid==expected.uid&&p.credentials.gid==expected.gid&&p.files.is_empty(),"Normal worker control is not original parent");
    let control:NormalVerificationControl=serde_json::from_slice(&p.bytes)?;
    anyhow::ensure!(control.nonce==nonce,"Normal worker control belongs to another original operation");
    match control.phase {NormalVerificationControlPhase::Continue=>{},NormalVerificationControlPhase::Finish=>return Ok(())}
   }
  }).await.map_err(|e|e.to_string()).and_then(|r|r.map_err(|e|format!("{e:#}")));
  let _=outcome_tx.send(result);
 };
 axum::serve(listener,app.into_make_service_with_connect_info::<std::net::SocketAddr>()).with_graceful_shutdown(shutdown).await?;
 outcome_rx.await.map_err(|_|fail("Normal worker original control outcome unavailable"))?.map_err(|e|fail(&e))?;
 derived.fence()?;reader.fence()?;reader.close().await?;bundle.fence()?;custody.bundle.fence()?;
 tracing::info!("Normal worker diagnostic complete; verified record remains unavailable");Ok(())
}

pub(crate) async fn verify_sqlite_normal_startup(source:&Path,workspace:&Path,version:&str)->anyhow::Result<()> {
 use std::io::Read;use std::os::unix::{fs::DirBuilderExt,process::CommandExt};use std::process::Stdio;
 let mut input=String::new();std::io::stdin().take(82*1024+1).read_to_string(&mut input)?;
 let supplied=normal_verification_supplied_input(input)?;let token=supplied.producer;
 let secret=std::env::var("JWT_SECRET").map_err(|_|fail("Actual configured original authentication secret absent"))?;
 let credential=VerificationHumanCredential::supplied(&token,&secret)?;
 let audiences=supplied.audiences.iter().map(|token|VerificationSuppliedAudience::supplied(token,&secret)).collect::<anyhow::Result<Vec<_>>>()?;
 anyhow::ensure!(workspace.is_absolute()&&!workspace.starts_with(source)&&!source.starts_with(workspace),"New normal verification workspace overlaps original source");
 let mut creator=std::fs::DirBuilder::new();creator.mode(0o700);creator.create(workspace)?;
 let derived_path=workspace.join("derived");creator.create(&derived_path)?;creator.create(derived_path.join("logging"))?;creator.create(derived_path.join("search"))?;
 // Target startup owns a retained spool under its parent. Isolate that
 // genuine creation role below application before any original descriptor
 // stamps are captured; derived outputs and bundle ancestors stay unchanged.
 let application_path=workspace.join("application");creator.create(&application_path)?;
 let bundle_path=workspace.join("bundle");let target_path=application_path.join("target");let version=version.to_owned();
 let result=capture_sqlite_for_verification(source,&bundle_path,&version.clone(),true,credential.admin(),move|mut original,bundle|Box::pin(async move {
  original.current_human().await?;original.fence().await?;
  let plan=original.application_page_plan().await?;let assets=original.application_asset_plan(&plan).await?;let drafts=original.application_draft_plan().await?;
  let audiences=if audiences.is_empty(){None}else{Some(original.application_audience_plan(&plan,&drafts,audiences).await?)};
  let unpublished=OriginalUnpublishedPlan::capture(&mut original,&plan,&drafts,audiences.as_ref()).await?;
  crate::storage::backup_restore::run(bundle.data_path().parent().ok_or_else(||fail("Original bundle parent absent"))?,&target_path,&version).await?;
  std::env::set_var("DATA_DIR",&target_path);
  let(pool,engine)=crate::db::init_pool().await?;crate::db::migrate_selected(&pool,engine).await?;
  activate_in(&target_path,&pool).await.map_err(|e|anyhow::anyhow!("Parent selected target activation refused: {e:?}"))?;
  let mut selection=OriginalTargetRecoverySelection::bind(&bundle,&target_path,&pool).await?;
  drop(selection._anchor.take());pool.close().await;selection.fence()?;original.fence().await?;
  let mut handoff=VerificationWorkerHandoff::prepare(&selection)?;
  let(parent,child)=verification_worker_pair()?;let socket_fd=child.as_raw_fd();let executable_fd=handoff.executable.as_raw_fd();
  let mut command=std::process::Command::new(format!("/proc/self/fd/{executable_fd}"));
  command.args(["verification-normal-worker",&socket_fd.to_string()]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
  unsafe{command.pre_exec(move||{for fd in [socket_fd,executable_fd]{let flags=libc::fcntl(fd,libc::F_GETFD);if flags<0||libc::fcntl(fd,libc::F_SETFD,flags&!libc::FD_CLOEXEC)<0{return Err(io::Error::last_os_error());}}Ok(())});}
  // Complete expensive original byte fences before the child starts its idle
  // clock. The bounded original-parent kernel packet owns genuine FDs while
  // queued; any later refusal consumes it and never permits retry/publication.
  original.fence().await?;handoff.send_once(&parent)?;original.fence().await?;
  let deadline=std::time::Instant::now()+std::time::Duration::from_secs(1800);
  let mut process=command.spawn()?;drop(child);let pid=process.id()as i32;
  let mut retained_listener=None; let mut private_queries=0usize;
  let observed=async {
   let ack=normal_initial_ack(&parent,&mut process,&handoff,&selection,&mut original,&plan,deadline).await?;
   anyhow::ensure!(ack.nonce==handoff.wire.nonce&&ack.phase=="initial-read-admitted"&&ack.original_sql_sha256==handoff.wire.original_sql_sha256&&ack.database_sha256==handoff.inventory.files["starlane-notes.db"].2,"Normal child initial input differs");
   handoff.fence()?;original.fence().await?;
   let derived=Root::open(&derived_path)?;let start=NormalVerificationStart{nonce:handoff.wire.nonce.clone(),derived:derived_path.clone(),stamps:derived.chain.iter().map(verification_worker_stamp).collect::<io::Result<_>>()?};
   verification_worker_send(&parent,&serde_json::to_vec(&start)?,&derived.chain.iter().collect::<Vec<_>>())?;
   let mut last_sequence=0u64;let mut last_phase=0u8;
   let (ready,held_listener)=loop {
    anyhow::ensure!(std::time::Instant::now()<deadline,"Original startup observation exceeded total bound");
    let mut p=verification_worker_receive(&parent).map_err(|e|anyhow::anyhow!("Normal startup readiness receive: {e}"))?;
    anyhow::ensure!(p.credentials.pid==pid&&p.credentials.uid==unsafe{libc::geteuid()}&&p.credentials.gid==unsafe{libc::getegid()},"Startup observation belongs to foreign child");
    if serde_json::from_slice::<NormalStartupProgress>(&p.bytes).is_ok() {
     let progress=normal_admit_startup_progress(&p,pid,&handoff.wire.nonce,identity(selection.active.root.fd())?,identity(&selection.database)?,last_sequence,last_phase,&mut original,&plan).await?;
     last_sequence=progress.sequence;last_phase=progress.phase;continue;
    }
    let ready:NormalVerificationReady=serde_json::from_slice(&p.bytes)?;anyhow::ensure!(last_phase>=3,"Ready skipped actual owner/sealed-reader progress");
    let held_listener=NormalVerificationListener::admit(&mut p,pid,&ready,&handoff.wire.nonce,&handoff.executable)?;break (ready,held_listener);
   };
   retained_listener=Some(held_listener);let held_listener=retained_listener.as_ref().ok_or_else(||fail("Original retained listener absent"))?;
   anyhow::ensure!(ready.nonce==handoff.wire.nonce&&ready.address.ip().is_loopback()&&ready.address.port()!=0&&ready.original_sql==handoff.wire.original_sql_sha256&&ready.qualified_sql==ready.target_sql,"Normal startup qualified closure differs");
   original.fence().await?;plan.fence(&original)?;
   held_listener.fence()?;normal_parent_progress(&parent,&handoff.wire.nonce)?;
   let probe_started=std::time::Instant::now();
   eprintln!("SQLITE_VERIFY_PROBE phase=ready elapsed_ms=0");
   verify_normal_tcp_pages(&mut original,&plan,ready.address,&token,&parent,&handoff.wire.nonce).await?;
   eprintln!("SQLITE_VERIFY_PROBE phase=pages elapsed_ms={}",probe_started.elapsed().as_millis());
   verify_normal_tcp_drafts(&mut original,&plan,&drafts,ready.address,&token,&parent,&handoff.wire.nonce).await?;
   eprintln!("SQLITE_VERIFY_PROBE phase=drafts elapsed_ms={}",probe_started.elapsed().as_millis());
   verify_normal_tcp_exact_assets(&mut original,&plan,&assets,ready.address,&token,&parent,&handoff.wire.nonce).await?;
   eprintln!("SQLITE_VERIFY_PROBE phase=exact-assets elapsed_ms={}",probe_started.elapsed().as_millis());
   verify_normal_sqlite_preindex_private(&mut original,&plan,&drafts,&assets,audiences.as_ref(),&mut process,ready.address,&token,&parent,&handoff.wire.nonce).await?;
   verify_normal_tcp_rebuilt_search(&mut original,&plan,ready.address,&token,&parent,&handoff.wire.nonce).await?;
   eprintln!("SQLITE_VERIFY_PROBE phase=rebuilt-search elapsed_ms={}",probe_started.elapsed().as_millis());
   verify_normal_sqlite_empty_search(&mut original,&plan,&drafts,&assets,&unpublished,audiences.as_ref(),&mut process,ready.address,&token,&parent,&handoff.wire.nonce).await?;
   private_queries=verify_normal_owner_private_search(&mut original,&plan,&drafts,ready.address,&token,&parent,&handoff.wire.nonce).await?;
   eprintln!("SQLITE_VERIFY_PROBE phase=private-search elapsed_ms={}",probe_started.elapsed().as_millis());
   held_listener.fence()?;
   if let Some(audiences)=&audiences{verify_normal_tcp_audiences(&mut original,&plan,&drafts,&assets,audiences,&mut process,ready.address,&parent,&handoff.wire.nonce).await?;}
   eprintln!("SQLITE_VERIFY_PROBE phase=audiences elapsed_ms={}",probe_started.elapsed().as_millis());
   verify_normal_sqlite_unpublished(&mut original,&plan,&drafts,&assets,&unpublished,audiences.as_ref(),&mut process,ready.address,&token,&parent,&handoff.wire.nonce).await?;
   eprintln!("SQLITE_VERIFY_PROBE phase=unpublished elapsed_ms={}",probe_started.elapsed().as_millis());
   held_listener.fence()?;original.fence().await?;plan.fence(&original)?;
   verification_worker_send(&parent,&serde_json::to_vec(&NormalVerificationControl{nonce:handoff.wire.nonce.clone(),phase:NormalVerificationControlPhase::Finish})?,&[])?;
   Ok::<_,anyhow::Error>((plan.pages.len(),ready.original_sql==ready.target_sql))
  }.await;
  let mut before_cleanup=process.try_wait()?;
  if observed.is_err() {
   // Kernel EOF can precede completion of a core-dump exit. Allow the genuine
   // child signal/exit to settle before bounded cleanup; don't overwrite the
   // original crash with SIGKILL and lose its exact diagnostic evidence.
   let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);
   while before_cleanup.is_none()&&std::time::Instant::now()<deadline {
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;before_cleanup=process.try_wait()?;
   }
   if before_cleanup.is_none(){let _=process.kill();}
  }
  let output=tokio::task::spawn_blocking(move||process.wait_with_output()).await??;
  if let Some(listener)=&retained_listener{listener.fence()?;}
  let(pages,startup_sql_unchanged)=observed.map_err(|e|anyhow::anyhow!("Normal worker observation refused: {e:#}; actual child status: {}; terminal_before_cleanup={before_cleanup:?}; stderr: {}",output.status,String::from_utf8_lossy(&output.stderr)))?;anyhow::ensure!(output.status.success(),"Actual normal worker did not complete: {}",String::from_utf8_lossy(&output.stderr));original.fence().await?;
  Ok((pages,assets.assets.len(),assets.assets.iter().map(|a|a.length).chain(assets.legacy.iter().map(|a|a.length)).sum::<u64>(),audiences.as_ref().map(|p|p.samples.len()).unwrap_or(0),private_queries,assets.legacy.len(),unpublished.pages.len(),startup_sql_unchanged))
 })).await?;
 println!("{}",serde_json::json!({"actual_normal_executable_startup":true,"registered_tcp_pages":result.0,"registered_tcp_assets":result.1,"exact_attachment_bytes":result.2,"registered_audience_samples":result.3,"private_draft_only_queries":result.4,"registered_tcp_legacy_assets":result.5,"registered_tcp_unpublished_pages":result.6,"startup_sql_unchanged":result.7,"complete_verified_record":false,"checks_scope":"original-human pages/history/properties/canonical drafts; selected original immutable binary versions, metadata, pins, covers and legacy held bytes; sampled current humans, rebuilt search and anonymous denial; ordinary-main/full completion records/platform support remain incomplete"}));Ok(())
}

async fn normal_tcp_json(client:&reqwest::Client,address:std::net::SocketAddr,token:Option<&str>,uri:&str)->anyhow::Result<(reqwest::StatusCode,serde_json::Value)> {
 let mut request=client.get(format!("http://{address}/api/v1{uri}"));if let Some(token)=token{request=request.bearer_auth(token);}
 let mut response=request.send().await?;let status=response.status();let mut bytes=Vec::new();
 while let Some(chunk)=response.chunk().await?{anyhow::ensure!(bytes.len().checked_add(chunk.len()).is_some_and(|n|n<=8*1024*1024),"Normal TCP probe body exceeds8MiB");bytes.extend_from_slice(&chunk);}
 Ok((status,serde_json::from_slice(&bytes)?))
}
async fn verify_normal_tcp_pages(original:&mut StoppedVerificationSource<'_>,plan:&OriginalApplicationPlan<'_>,address:std::net::SocketAddr,token:&str,channel:&File,nonce:&str)->anyhow::Result<()> {
 let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).redirect(reqwest::redirect::Policy::none()).no_proxy().build()?;
 for (index,page) in plan.pages.iter().enumerate() {
  let uri=format!("/spaces/{}/documents/{}",urlencoding::encode(&page.space),urlencoding::encode(&page.id));
  for(suffix,expected)in [("",page.metadata.clone()),("/content",plan.content[index].clone()),("/versions",serde_json::json!(page.history)),("/properties",serde_json::json!(page.properties))]{
   original.fence().await?;plan.fence(original)?;normal_parent_progress(channel,nonce)?;
   let(status,actual)=normal_source_fenced_observation(original,plan,channel,nonce,normal_tcp_json(&client,address,Some(token),&format!("{uri}{suffix}"))).await?;
   original.fence().await?;plan.fence(original)?;normal_parent_progress(channel,nonce)?;anyhow::ensure!(status==reqwest::StatusCode::OK,"Registered normal TCP route unavailable");
   if suffix.is_empty(){for(k,v)in expected.as_object().ok_or_else(||fail("Malformed internal original metadata"))?{anyhow::ensure!(actual.get(k)==Some(v),"Normal TCP original metadata differs");}}
   else if suffix=="/content"{anyhow::ensure!(actual==expected,"Normal TCP exact body differs");}
   else{anyhow::ensure!(actual==expected,"Normal TCP original history/property presentation differs");}
   let(status,_) = normal_source_fenced_observation(original,plan,channel,nonce,normal_tcp_json(&client,address,None,&format!("{uri}{suffix}"))).await?;
   original.fence().await?;plan.fence(original)?;normal_parent_progress(channel,nonce)?;anyhow::ensure!(matches!(status.as_u16(),401|403|404),"Normal TCP anonymous private audience did not deny");
  }
 }
 Ok(())
}

// Long application observations retain the original source/current actor fence.
// A timeout or failed fence cancels the observation; it never authorizes publication.
async fn normal_source_fenced_observation<T>(source:&mut StoppedVerificationSource<'_>,plan:&OriginalApplicationPlan<'_>,channel:&File,nonce:&str,observation:impl std::future::Future<Output=anyhow::Result<T>>)->anyhow::Result<T> {
 let deadline=tokio::time::sleep(std::time::Duration::from_secs(120));tokio::pin!(deadline);
 tokio::pin!(observation);let mut pulse=tokio::time::interval(std::time::Duration::from_secs(3));pulse.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
 loop {tokio::select! {
  result=&mut observation=>{source.fence().await?;plan.fence(source)?;normal_parent_progress(channel,nonce)?;return result;},
  _=&mut deadline=>return Err(fail("Normal source-fenced application observation exceeded120s").into()),
  _=pulse.tick()=>{source.fence().await?;plan.fence(source)?;normal_parent_progress(channel,nonce)?;}
 }}
}
async fn verify_normal_tcp_rebuilt_search(source:&mut StoppedVerificationSource<'_>,plan:&OriginalApplicationPlan<'_>,address:std::net::SocketAddr,token:&str,channel:&File,nonce:&str)->anyhow::Result<()> {
 let client=reqwest::Client::builder().timeout(std::time::Duration::from_secs(120)).redirect(reqwest::redirect::Policy::none()).no_proxy().build()?;
 let mut queries=Vec::new();
 for page in &plan.pages {
  // Require a meaningful original body token; do not replace a content-index
  // probe with an unindexed SQL listing or filename/head resolution.
  let word=page.body.split(|c:char|!c.is_ascii_alphanumeric()).find(|w|w.len()>=8&&w.len()<=100).ok_or_else(||fail("Original page lacks a bounded literal search body sample"))?;
  let uri=format!("/search?q={}&space_id={}&limit=100",urlencoding::encode(word),urlencoding::encode(&page.space));
  source.fence().await?;plan.fence(source)?;normal_parent_progress(channel,nonce)?;
  let(status,empty)=normal_source_fenced_observation(source,plan,channel,nonce,normal_tcp_json(&client,address,Some(token),&uri)).await?;
  source.fence().await?;plan.fence(source)?;normal_parent_progress(channel,nonce)?;
  anyhow::ensure!(status==reqwest::StatusCode::OK&&empty.as_array().is_some_and(|r|r.is_empty()),"Genuinely new derived index must be empty before registered rebuild");
  queries.push((uri,page.id.clone(),page.space.clone(),word.to_owned()));
 }
 source.fence().await?;plan.fence(source)?;normal_parent_progress(channel,nonce)?;
 let rebuilt=normal_source_fenced_observation(source,plan,channel,nonce,async {
 let mut response=client.post(format!("http://{address}/api/v1/admin/search/reindex")).bearer_auth(token).send().await?;
 anyhow::ensure!(response.status()==reqwest::StatusCode::OK,"Actual registered normal search rebuild refused");
 let mut bytes=Vec::new();while let Some(chunk)=response.chunk().await?{anyhow::ensure!(bytes.len().checked_add(chunk.len()).is_some_and(|n|n<=65536),"Registered rebuild response exceeds bound");bytes.extend_from_slice(&chunk);}let rebuilt:serde_json::Value=serde_json::from_slice(&bytes)?;Ok(rebuilt)
 }).await?;
 source.fence().await?;plan.fence(source)?;normal_parent_progress(channel,nonce)?;
 anyhow::ensure!(rebuilt["status"]=="reindex_complete"&&rebuilt["documents_indexed"].as_u64()==Some(plan.pages.len()as u64),"Normal rebuilt index coverage differs from original canonical published pages");
 for(uri,id,space,word)in queries {
  let(status,found)=normal_source_fenced_observation(source,plan,channel,nonce,normal_tcp_json(&client,address,Some(token),&uri)).await?;
  source.fence().await?;plan.fence(source)?;normal_parent_progress(channel,nonce)?;
  anyhow::ensure!(status==reqwest::StatusCode::OK&&found.as_array().is_some_and(|hits|hits.iter().any(|h|h["document_id"].as_str()==Some(id.as_str())&&h["space_id"].as_str()==Some(space.as_str())&&h["excerpt"].as_str().is_some_and(|s|s.contains(&word)))),"Rebuilt normal search lost original body/page binding");
  let(status,_)=normal_source_fenced_observation(source,plan,channel,nonce,normal_tcp_json(&client,address,None,&uri)).await?;
  source.fence().await?;plan.fence(source)?;normal_parent_progress(channel,nonce)?;
  anyhow::ensure!(matches!(status.as_u16(),401|403|404),"Normal anonymous private search audience did not deny");
 }
 Ok(())
}

#[cfg(test)]mod normal_startup_progress_controls {
 use super::*;
 #[test]fn installed_python3_transport_is_verified_and_isolated()->anyhow::Result<()> {
  let(path,held,stamp,sha)=sqlite_capture_python()?;
  let output=std::process::Command::new(&path).args(["-I","-c","import sys; print(str(sys.version_info.major)+':'+str(sys.flags.isolated))"]).output()?;
  anyhow::ensure!(output.status.success()&&output.stdout==b"3:1\n","Installed Python3 isolated transport probe failed");
  anyhow::ensure!(physical_stamp(&held)?==stamp&&hash_file(&held,held.metadata()?.len())?==sha,"Selected Python3 changed during transport probe");
  Ok(())
 }
 #[test]fn foreign_replayed_or_regressed_phase_never_advances(){
  let root:Identity=(1,2);let database:Identity=(1,3);
  let mut p=NormalStartupProgress{nonce:"original".into(),phase:2,sequence:4,root,database};
  assert!(normal_validate_startup_progress(&p,"original",root,database,3,1).is_ok());
  assert!(normal_validate_startup_progress(&p,"original",root,database,4,2).is_err());
  p.nonce="foreign".into();assert!(normal_validate_startup_progress(&p,"original",root,database,3,1).is_err());
  p.nonce="original".into();p.database.1+=1;assert!(normal_validate_startup_progress(&p,"original",root,database,3,1).is_err());
  p.database=database;p.phase=0;assert!(normal_validate_startup_progress(&p,"original",root,database,3,1).is_err());
  p.phase=3;p.sequence=1;assert!(normal_validate_startup_progress(&p,"original",root,database,0,0).is_err());
 }
}

#[cfg(test)]mod normal_admission_progress_controls {
 use super::*;
 #[test]fn native_observation_cannot_be_ack_ready_or_replayed() {
  let mut p=NormalAdmissionProgress{nonce:"original".into(),admission_phase:1,sequence:1,root:(1,2),database:(1,3)};
  let bytes=serde_json::to_vec(&p).unwrap();
  assert!(serde_json::from_slice::<VerificationWorkerInitialAck>(&bytes).is_err());
  assert!(serde_json::from_slice::<NormalVerificationReady>(&bytes).is_err());
  assert!(serde_json::from_slice::<NormalStartupProgress>(&bytes).is_err());
  assert!(normal_validate_admission_progress(&p,"original",(1,2),(1,3),0).is_ok());
  assert!(normal_validate_admission_progress(&p,"original",(1,2),(1,3),1).is_err());
  p.database=(1,4);assert!(normal_validate_admission_progress(&p,"original",(1,2),(1,3),0).is_err());
  p.database=(1,3);p.nonce="foreign".into();assert!(normal_validate_admission_progress(&p,"original",(1,2),(1,3),0).is_err());
  p.nonce="original".into();p.admission_phase=2;assert!(normal_validate_admission_progress(&p,"original",(1,2),(1,3),0).is_err());
  p.admission_phase=1;p.sequence=601;assert!(normal_validate_admission_progress(&p,"original",(1,2),(1,3),600).is_err());
 }
}

#[cfg(test)]mod normal_admission_kernel_controls {
 use super::*;
 fn full_channel()->(File,File) {
  let(sender,receiver)=verification_worker_pair().unwrap();let size=4096i32;
  assert_eq!(unsafe{libc::setsockopt(sender.as_raw_fd(),libc::SOL_SOCKET,libc::SO_SNDBUF,(&size as *const i32).cast(),std::mem::size_of_val(&size)as libc::socklen_t)},0);
  let bytes=vec![b'x';1024];let mut full=false;
  for _ in 0..1024 {match normal_admission_send(&sender,&bytes){Ok(())=>{},Err(e)=>{assert_eq!(e.kind(),io::ErrorKind::WouldBlock);full=true;break;}}}
  assert!(full,"Actual private kernel channel never filled");(sender,receiver)
 }
 #[test]fn actual_full_channel_refuses_and_owned_finish_drop_retire() {
  for finish in [true,false] {
   let(sender,_receiver)=full_channel();let(stop,_rx)=std::sync::mpsc::channel();
   let thread=std::thread::spawn(move||{normal_admission_send(&sender,&vec![b'x';1024])?;Ok(())});
   let pulse=NormalAdmissionPulse{stop,thread:Some(thread)};let start=std::time::Instant::now();
   if finish{assert!(pulse.finish().is_err());}else{drop(pulse);}
   assert!(start.elapsed()<std::time::Duration::from_secs(2),"Owned observation retirement blocked on full kernel queue");
  }
 }
 #[test]fn actual_kernel_sender_foreign_nonce_and_replay_refuse() {
  let root=(1,2);let database=(1,3);
  let original=NormalAdmissionProgress{nonce:"original".into(),admission_phase:1,sequence:1,root,database};
  let bytes=serde_json::to_vec(&original).unwrap();
  let(first,replay,pid)=normal_actual_wire_packets(&bytes,&bytes).unwrap();
  assert!(normal_admit_native_packet(&first,pid,"original",root,database,0).is_ok());
  assert!(normal_admit_native_packet(&first,pid+1,"original",root,database,0).is_err());
  assert!(normal_admit_native_packet(&first,pid,"foreign",root,database,0).is_err());
  assert!(normal_admit_native_packet(&replay,pid,"original",root,database,1).is_err());
 }
}
