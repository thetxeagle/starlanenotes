// A dedicated OS supervisor owns the helper and original-source custody. Its
// lifetime is independent of the requesting future AND the Tokio runtime.
// Cancellation poisons authority immediately; release follows actual waitpid.
struct SqliteCaptureHelper {
 cancel:Option<std::sync::mpsc::Sender<()>>,
 done:tokio::sync::oneshot::Receiver<io::Result<std::process::ExitStatus>>,
 original:std::sync::Arc<SqliteOriginalSourceCustody>,
 pid:Option<u32>,
 completed:bool,
 #[cfg(test)] reaped:std::sync::Arc<std::sync::atomic::AtomicBool>,
}
// Last-resort unwind safety also follows kill with wait before dropping the
// original. This guard lives on the dedicated OS thread, never an async worker.
struct SqliteCaptureChildGuard {
 child:std::process::Child,
 original:std::sync::Arc<SqliteOriginalSourceCustody>,
}
impl Drop for SqliteCaptureChildGuard {
 fn drop(&mut self){
  if !matches!(self.child.try_wait(),Ok(Some(_))) {
   self.original.cancel();let _=self.child.kill();
   if self.child.wait().is_err(){
    // An unconfirmed wait must not release custody. Keep the failed owner's
    // descriptors until process exit rather than invent a reaping receipt.
    std::mem::forget(self.original.clone());
   }
  }
 }
}
#[cfg(test)]
type SqliteCaptureBeforeReap=std::sync::Arc<dyn Fn()+Send+Sync>;
impl SqliteCaptureHelper {
 fn spawn(command:&mut tokio::process::Command,original:std::sync::Arc<SqliteOriginalSourceCustody>)->io::Result<Self> {
  #[cfg(test)] {Self::spawn_supervised(command,original,None)}
  #[cfg(not(test))] {Self::spawn_supervised(command,original)}
 }
 fn spawn_supervised(command:&mut tokio::process::Command,original:std::sync::Arc<SqliteOriginalSourceCustody>,#[cfg(test)] before_reap:Option<SqliteCaptureBeforeReap>)->io::Result<Self> {
  original.io_fence()?;
  let (handoff,started)=std::sync::mpsc::sync_channel::<(std::process::Child,std::sync::Arc<SqliteOriginalSourceCustody>)>(1);
  let(cancel,cancelled)=std::sync::mpsc::channel();
  let(done,completion)=tokio::sync::oneshot::channel();
  #[cfg(test)] let reaped=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
  #[cfg(test)] let observed_reap=reaped.clone();
  // Start the supervisor before spawning a child. Thread-admission failure
  // therefore cannot orphan a child captured by a rejected thread closure.
  std::thread::Builder::new().name("sqlite-capture-helper".into()).spawn(move || {
   let Ok((child,retained))=started.recv() else{return;};
   let mut guard=SqliteCaptureChildGuard{child,original:retained};
   let result=loop {
    match cancelled.try_recv() {
     Ok(())|Err(std::sync::mpsc::TryRecvError::Disconnected)=>{
      guard.original.cancel();
      #[cfg(test)] if let Some(hook)=&before_reap{hook();}
      let kill=guard.child.kill();let waited=guard.child.wait();
      break match (kill,waited) {
       (_,Err(e))=>Err(e),
       (Err(e),Ok(_)) if e.kind()!=io::ErrorKind::InvalidInput=>Err(e),
       _=>Err(fail("Original SQLite helper cancelled and reaped; private evidence retained")),
      };
     },
     Err(std::sync::mpsc::TryRecvError::Empty)=>(),
    }
    match guard.child.try_wait() {
     Ok(Some(status))=>break Ok(status),
     Ok(None)=>(),
     Err(e)=>{guard.original.cancel();let _=guard.child.kill();let _=guard.child.wait();break Err(e);},
    }
    // This wait runs only on the independent OS supervisor. Cancellation wakes
    // it immediately; the timeout observes natural child exit without busy spin.
    match cancelled.recv_timeout(std::time::Duration::from_millis(10)) {
     Ok(())|Err(std::sync::mpsc::RecvTimeoutError::Disconnected)=>{
      guard.original.cancel();
      #[cfg(test)] if let Some(hook)=&before_reap{hook();}
      let kill=guard.child.kill();let waited=guard.child.wait();
      break match (kill,waited) {
       (_,Err(e))=>Err(e),
       (Err(e),Ok(_)) if e.kind()!=io::ErrorKind::InvalidInput=>Err(e),
       _=>Err(fail("Original SQLite helper cancelled and reaped; private evidence retained")),
      };
     },
     Err(std::sync::mpsc::RecvTimeoutError::Timeout)=>(),
    }
   };
   if done.send(result).is_err(){guard.original.cancel();}
   drop(guard);
   #[cfg(test)] observed_reap.store(true,std::sync::atomic::Ordering::SeqCst);
  })?;
  let child=command.as_std_mut().spawn()?;let pid=Some(child.id());
  if let Err(error)=handoff.send((child,original.clone())) {
   original.cancel();let(mut child,_)=error.0;let _=child.kill();child.wait()?;
   return Err(fail("Original SQLite helper supervisor handoff failed; child reaped"));
  }
  Ok(Self{cancel:Some(cancel),done:completion,original,pid,completed:false,
   #[cfg(test)] reaped,
  })
 }
 fn id(&self)->Option<u32>{self.pid}
 async fn wait(&mut self)->io::Result<std::process::ExitStatus>{
  let result=(&mut self.done).await.map_err(|_|fail("Original SQLite helper supervisor lost"))?;
  self.completed=true;self.cancel.take();result
 }
}
impl Drop for SqliteCaptureHelper {
 fn drop(&mut self){
  if !self.completed {self.original.cancel();if let Some(cancel)=self.cancel.take(){let _=cancel.send(());}}
 }
}

#[cfg(test)]
mod sqlite_capture_helper_owner_tests {
 use super::*;
 #[tokio::test]
 #[ignore="actual trusted host Python and original-source read custody"]
 async fn cancelled_helper_is_killed_and_reaped_before_supervisor_releases_original(){
  use std::os::unix::fs::{DirBuilderExt,OpenOptionsExt};
  let base=std::env::temp_dir().join(format!("sqlite-helper-cancel-{}",uuid::Uuid::new_v4()));
  std::fs::DirBuilder::new().mode(0o700).create(&base).unwrap();
  let root=Root::open(&base).unwrap();
  drop(sqlite_capture_write(&root,"starlane-notes.db",b"original specimen").unwrap());
  let original=SqliteOriginalSourceCustody::hold(&root).unwrap();let weak=std::sync::Arc::downgrade(&original);
  let(entered,at_reap)=std::sync::mpsc::sync_channel(1);
  let(release,proceed)=std::sync::mpsc::sync_channel(1);let proceed=std::sync::Mutex::new(proceed);
  let hook=std::sync::Arc::new(move ||{entered.send(()).unwrap();proceed.lock().unwrap().recv().unwrap();});
  let mut command=tokio::process::Command::new(SQLITE_CAPTURE_PYTHON);
  command.args(["-I","-c","import signal; signal.pause()"]);
  let helper=SqliteCaptureHelper::spawn_supervised(&mut command,original.clone(),Some(hook)).unwrap();
  let pid=helper.id().unwrap();let reaped=helper.reaped.clone();
  drop(original);drop(helper);
  at_reap.recv_timeout(std::time::Duration::from_secs(5)).expect("Actual supervisor cancellation barrier");
  assert!(weak.upgrade().expect("Supervisor alone retains original custody").io_fence().is_err(),"Drop immediately invalidates source authority");
  assert!(!reaped.load(std::sync::atomic::Ordering::SeqCst));
  assert_eq!(unsafe{libc::kill(pid as _,0)},0,"Helper has not yet been killed at the barrier");
  release.send(()).unwrap();
  tokio::time::timeout(std::time::Duration::from_secs(5),async {
   while !reaped.load(std::sync::atomic::Ordering::SeqCst){tokio::task::yield_now().await;}
  }).await.expect("Supervisor must acknowledge actual child wait and custody retirement");
  assert!(weak.upgrade().is_none(),"Supervisor releases custody only after child wait");
  let mut status=0;
  assert_eq!(unsafe{libc::waitpid(pid as _,&mut status,libc::WNOHANG)},-1);
  assert_eq!(io::Error::last_os_error().raw_os_error(),Some(libc::ECHILD),"Supervisor already reaped this child");
  assert_eq!(std::fs::read(base.join("starlane-notes.db")).unwrap(),b"original specimen");
  std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(base.join("starlane-notes.db")).expect("Custody releases after helper retirement");
 }

 #[test]
 #[ignore="actual trusted host Python and original-source read custody after Tokio shutdown"]
 fn supervisor_reaps_and_retires_original_after_runtime_shutdown(){
  use std::os::unix::fs::{DirBuilderExt,OpenOptionsExt};
  let base=std::env::temp_dir().join(format!("sqlite-helper-runtime-shutdown-{}",uuid::Uuid::new_v4()));
  std::fs::DirBuilder::new().mode(0o700).create(&base).unwrap();
  let root=Root::open(&base).unwrap();
  drop(sqlite_capture_write(&root,"starlane-notes.db",b"original runtime shutdown specimen").unwrap());
  let original=SqliteOriginalSourceCustody::hold(&root).unwrap();
  let weak=std::sync::Arc::downgrade(&original);
  let(entered,at_reap)=std::sync::mpsc::sync_channel(1);
  let(release,proceed)=std::sync::mpsc::sync_channel(1);
  let proceed=std::sync::Mutex::new(proceed);
  let hook=std::sync::Arc::new(move ||{
   entered.send(()).unwrap();
   proceed.lock().unwrap().recv().unwrap();
  });
  let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  let helper=runtime.block_on(async {
   let mut command=tokio::process::Command::new(SQLITE_CAPTURE_PYTHON);
   command.args(["-I","-c","import signal; signal.pause()"]);
   SqliteCaptureHelper::spawn_supervised(&mut command,original.clone(),Some(hook)).unwrap()
  });
  let pid=helper.id().unwrap();let reaped=helper.reaped.clone();
  drop(original);
  // The runtime is gone before helper cancellation: no Tokio task/timer can
  // supervise, kill, wait, or retire the sole retained source owner for us.
  drop(runtime);
  assert!(!reaped.load(std::sync::atomic::Ordering::SeqCst));
  assert_eq!(unsafe{libc::kill(pid as _,0)},0,"Actual child survives runtime shutdown");
  drop(helper);
  at_reap.recv_timeout(std::time::Duration::from_secs(5)).expect("OS supervisor reaches cancellation without Tokio");
  assert_eq!(weak.strong_count(),1,"Only the OS supervisor retains original custody at the barrier");
  assert!(weak.upgrade().expect("Supervisor retains original after runtime shutdown").io_fence().is_err(),"Cancellation immediately consumes source authority");
  assert!(!reaped.load(std::sync::atomic::Ordering::SeqCst));
  assert_eq!(unsafe{libc::kill(pid as _,0)},0,"Barrier precedes actual child kill/wait");
  release.send(()).unwrap();
  // Reap receipt is an atomic published only after guard retirement. Bounded
  // OS parking observes that receipt without any live async runtime or spin.
  let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);
  while !reaped.load(std::sync::atomic::Ordering::SeqCst){
   let remaining=deadline.saturating_duration_since(std::time::Instant::now());
   assert!(!remaining.is_zero(),"OS supervisor must reap and retire custody after runtime shutdown");
   std::thread::park_timeout(remaining.min(std::time::Duration::from_millis(10)));
  }
  assert!(weak.upgrade().is_none(),"Actual wait precedes release of supervisor's last source Arc");
  let mut status=0;
  assert_eq!(unsafe{libc::waitpid(pid as _,&mut status,libc::WNOHANG)},-1);
  assert_eq!(io::Error::last_os_error().raw_os_error(),Some(libc::ECHILD),"Independent supervisor already reaped the actual child");
  assert_eq!(std::fs::read(base.join("starlane-notes.db")).unwrap(),b"original runtime shutdown specimen");
  std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(base.join("starlane-notes.db")).expect("Original read custody releases after actual child retirement without Tokio");
 }
}
