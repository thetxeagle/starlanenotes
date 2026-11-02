use anyhow::Context as _;
// Listener transport only. This never creates source/SQL/native/record authority.
const ORDINARY_LISTENER_CHANNEL: &str = "STARLANE_PRIVATE_ORDINARY_LISTENER_CHANNEL";
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OrdinaryListenerWire {
    purpose: String, version: u32, nonce: String, root: PathBuf,
    ancestry: Vec<String>, executable_stamp: String, executable_sha256: String,
    listener_identity: (u64,u64), address: std::net::SocketAddr,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct OrdinaryListenerReady {
    purpose: String, version: u32, nonce: String,
    listener_identity: (u64,u64), address: std::net::SocketAddr,
}
// Ancestry only: listener transport cannot classify engine markers or grant SQL.
// Original typed target owner and unchanged ordinary bootstrap do that separately.
fn ordinary_listener_root_identity(root:&Root)->io::Result<()> {
    let current=Root::open(&root.absolute)?;
    if current.chain.len()!=root.chain.len(){return Err(fail("Ordinary listener root ancestry changed"));}
    for (held,now) in root.chain.iter().zip(&current.chain){if !held.metadata()?.is_dir()||identity(held)?!=identity(now)?{return Err(fail("Ordinary listener original root replaced"));}}
    Ok(())
}
fn ordinary_listener_identity(file:&File)->io::Result<(u64,u64)> {
    let md=file.metadata()?; Ok((md.dev(),md.ino()))
}
fn ordinary_listener_address(file:&File)->io::Result<std::net::SocketAddr> {
    for (option,expected) in [(libc::SO_TYPE,libc::SOCK_STREAM),(libc::SO_DOMAIN,libc::AF_INET),(libc::SO_ACCEPTCONN,1),(libc::SO_REUSEPORT,0)] {
        let mut actual=0i32; let mut length=std::mem::size_of::<i32>() as libc::socklen_t;
        if unsafe {libc::getsockopt(file.as_raw_fd(),libc::SOL_SOCKET,option,(&mut actual as *mut i32).cast(),&mut length)}!=0 {return Err(io::Error::last_os_error());}
        if actual!=expected || length as usize!=std::mem::size_of::<i32>() {return Err(fail("Ordinary listener native role differs"));}
    }
    let mut address:libc::sockaddr_in=unsafe {std::mem::zeroed()}; let mut length=std::mem::size_of_val(&address) as libc::socklen_t;
    if unsafe {libc::getsockname(file.as_raw_fd(),(&mut address as *mut libc::sockaddr_in).cast(),&mut length)}!=0 {return Err(io::Error::last_os_error());}
    let ip=std::net::Ipv4Addr::from(address.sin_addr.s_addr.to_ne_bytes()); let port=u16::from_be(address.sin_port);
    if length as usize!=std::mem::size_of_val(&address)||address.sin_family as i32!=libc::AF_INET||ip!=std::net::Ipv4Addr::LOCALHOST||port==0 {return Err(fail("Ordinary listener must be exact bound IPv4 loopback"));}
    Ok(std::net::SocketAddr::from((ip,port)))
}
// Purpose-owned bounded send. Queue saturation consumes the attempt; no join or
// publication can wait on a hostile peer filling a blocking socket buffer.
fn ordinary_listener_send(socket:&File,bytes:&[u8],files:&[&File])->io::Result<()> {
    if bytes.is_empty()||bytes.len()>16384||files.len()>32 {return Err(fail("Ordinary listener packet exceeds bounds"));}
    let mut control=[0usize;128];let mut iov=libc::iovec{iov_base:bytes.as_ptr() as *mut _,iov_len:bytes.len()};
    let mut message:libc::msghdr=unsafe{std::mem::zeroed()};message.msg_iov=&mut iov;message.msg_iovlen=1;
    let descriptors:Vec<i32>=files.iter().map(|f|f.as_raw_fd()).collect();
    if !descriptors.is_empty() {message.msg_control=control.as_mut_ptr().cast();message.msg_controllen=unsafe{libc::CMSG_SPACE((descriptors.len()*std::mem::size_of::<i32>())as u32)}as usize;
        unsafe{let h=libc::CMSG_FIRSTHDR(&message);(*h).cmsg_level=libc::SOL_SOCKET;(*h).cmsg_type=libc::SCM_RIGHTS;(*h).cmsg_len=libc::CMSG_LEN((descriptors.len()*std::mem::size_of::<i32>())as u32)as usize;std::ptr::copy_nonoverlapping(descriptors.as_ptr(),libc::CMSG_DATA(h).cast::<i32>(),descriptors.len());}}
    let sent=unsafe{libc::sendmsg(socket.as_raw_fd(),&message,libc::MSG_NOSIGNAL|libc::MSG_DONTWAIT)};
    if sent!=bytes.len()as isize {return Err(if sent<0 {io::Error::last_os_error()} else {fail("Ordinary listener send outcome unknown")});} Ok(())
}
// Readiness can refuse transport only; it never authenticates custody. No
// retry/wait: even EINTR consumes the caller's one admission attempt.
fn ordinary_listener_queue_ready(channel: &File) -> io::Result<()> {
    let mut poll = libc::pollfd { fd: channel.as_raw_fd(), events: libc::POLLOUT, revents: 0 };
    let status = unsafe { libc::poll(&mut poll, 1, 0) };
    if status < 0 { return Err(io::Error::last_os_error()); }
    if poll.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
        return Err(fail("Ordinary listener channel unavailable"));
    }
    if status == 0 || poll.revents & libc::POLLOUT == 0 {
        return Err(io::Error::from(io::ErrorKind::WouldBlock));
    }
    Ok(())
}
// Observational only: no collector, clock reads, or phase guards in production.
#[cfg(test)]
#[derive(Debug, Default)]
struct OrdinaryQueueTiming {
    executable_bytes: u64,
    filled_packets: usize,
    readiness: Option<std::time::Duration>,
    readiness_error: Option<(io::ErrorKind, Option<i32>)>,
    pre_fence: Option<std::time::Duration>,
    ancestry_serialization: Option<std::time::Duration>,
    send: Option<std::time::Duration>,
    post_fence: Option<std::time::Duration>,
    send_error: Option<(io::ErrorKind, Option<i32>)>,
}
#[cfg(test)]
std::thread_local! {
    static ORDINARY_QUEUE_AFTER_READINESS: std::cell::RefCell<Option<Box<dyn FnOnce(&File) -> anyhow::Result<()>>>> = const { std::cell::RefCell::new(None) };
    static ORDINARY_QUEUE_TIMING: std::cell::RefCell<Option<OrdinaryQueueTiming>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
enum OrdinaryQueuePhase { Readiness, PreFence, AncestrySerialization, Send, PostFence }
#[cfg(test)]
struct OrdinaryQueuePhaseTimer {
    phase: OrdinaryQueuePhase,
    began: Option<std::time::Instant>,
}
#[cfg(test)]
impl OrdinaryQueuePhaseTimer {
    fn start(phase: OrdinaryQueuePhase) -> Self {
        let began = ORDINARY_QUEUE_TIMING.with(|timing| timing.borrow().as_ref().map(|_| std::time::Instant::now()));
        Self { phase, began }
    }
}
#[cfg(test)]
impl Drop for OrdinaryQueuePhaseTimer {
    fn drop(&mut self) {
        let Some(began) = self.began else { return };
        let elapsed = began.elapsed();
        ORDINARY_QUEUE_TIMING.with(|timing| {
            if let Some(timing) = timing.borrow_mut().as_mut() {
                let slot = match self.phase {
                    OrdinaryQueuePhase::Readiness => &mut timing.readiness,
                    OrdinaryQueuePhase::PreFence => &mut timing.pre_fence,
                    OrdinaryQueuePhase::AncestrySerialization => &mut timing.ancestry_serialization,
                    OrdinaryQueuePhase::Send => &mut timing.send,
                    OrdinaryQueuePhase::PostFence => &mut timing.post_fence,
                };
                *slot = Some(elapsed);
            }
        });
    }
}
struct OrdinaryListenerParent<'a> {
    root:&'a Root, listener:File, executable:File, wire:OrdinaryListenerWire, dispatched:bool, ready:bool,
}
impl<'a> OrdinaryListenerParent<'a> {
    fn prepare(root:&'a Root)->anyhow::Result<Self> {
        use std::os::fd::{IntoRawFd,FromRawFd};ordinary_listener_root_identity(root)?;
        let tcp=std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0))?;
        let listener=unsafe{File::from_raw_fd(tcp.into_raw_fd())};let address=ordinary_listener_address(&listener)?;
        let executable=File::open("/proc/self/exe")?;let md=executable.metadata()?;
        anyhow::ensure!(md.is_file()&&md.uid()==unsafe{libc::geteuid()}&&md.mode()&0o022==0&&md.mode()&0o111!=0&&md.len()<=1024*1024*1024,"Ordinary selected executable not owned/bounded");
        anyhow::ensure!(root.chain.len()>=2&&root.chain.len()+2<=32,"Ordinary target ancestry exceeds descriptor bounds");
        let wire=OrdinaryListenerWire{purpose:"starlane-ordinary-listener-admission".into(),version:1,nonce:uuid::Uuid::new_v4().to_string(),root:root.absolute.clone(),ancestry:root.chain.iter().map(verification_worker_stamp).collect::<io::Result<_>>()?,executable_stamp:verification_worker_stamp(&executable)?,executable_sha256:hash_file(&executable,md.len())?,listener_identity:ordinary_listener_identity(&listener)?,address};
        let value=Self{root,listener,executable,wire,dispatched:false,ready:false};value.fence()?;Ok(value)
    }
    fn fence(&self)->anyhow::Result<()> {
        ordinary_listener_root_identity(&self.root)?;anyhow::ensure!(ordinary_listener_identity(&self.listener)?==self.wire.listener_identity&&ordinary_listener_address(&self.listener)?==self.wire.address,"Original parent listener exchanged");
        anyhow::ensure!(verification_worker_stamp(&self.executable)?==self.wire.executable_stamp&&hash_file(&self.executable,self.executable.metadata()?.len())?==self.wire.executable_sha256,"Ordinary original ELF changed");Ok(())
    }
    fn queue_once(&mut self,channel:&File)->anyhow::Result<()> {
        anyhow::ensure!(!self.dispatched,"Ordinary listener attempt consumed; no retry");
        // Fail closed: every entry attempt, including readiness/fence refusal,
        // consumes this selection. Capacity returning cannot revive it.
        self.dispatched=true;
        #[cfg(test)]
        let readiness = OrdinaryQueuePhaseTimer::start(OrdinaryQueuePhase::Readiness);
        let available = ordinary_listener_queue_ready(channel);
        #[cfg(test)]
        drop(readiness);
        #[cfg(test)]
        ORDINARY_QUEUE_TIMING.with(|timing| {
            if let Some(timing) = timing.borrow_mut().as_mut() {
                timing.readiness_error = available.as_ref().err().map(|error| (error.kind(), error.raw_os_error()));
            }
        });
        available?;
        #[cfg(test)]
        {
            let hook = ORDINARY_QUEUE_AFTER_READINESS.with(|hook| hook.borrow_mut().take());
            if let Some(hook) = hook { hook(channel)?; }
        }
        #[cfg(test)]
        let pre_fence = OrdinaryQueuePhaseTimer::start(OrdinaryQueuePhase::PreFence);
        self.fence()?;
        #[cfg(test)]
        drop(pre_fence);
        #[cfg(test)]
        let ancestry_serialization = OrdinaryQueuePhaseTimer::start(OrdinaryQueuePhase::AncestrySerialization);
        for (file,stamp) in self.root.chain.iter().zip(&self.wire.ancestry) {anyhow::ensure!(verification_worker_stamp(file)?==*stamp,"Ordinary original target changed before dispatch");}
        let mut files:Vec<&File>=self.root.chain.iter().collect();files.extend([&self.listener,&self.executable]);
        let bytes = serde_json::to_vec(&self.wire)?;
        #[cfg(test)]
        drop(ancestry_serialization);
        #[cfg(test)]
        let send = OrdinaryQueuePhaseTimer::start(OrdinaryQueuePhase::Send);
        let sent = ordinary_listener_send(channel,&bytes,&files);
        #[cfg(test)]
        drop(send);
        #[cfg(test)]
        ORDINARY_QUEUE_TIMING.with(|timing| {
            if let Some(timing) = timing.borrow_mut().as_mut() {
                timing.send_error = sent.as_ref().err().map(|error| (error.kind(), error.raw_os_error()));
            }
        });
        sent?;
        #[cfg(test)]
        let _post_fence = OrdinaryQueuePhaseTimer::start(OrdinaryQueuePhase::PostFence);
        self.fence()
    }
    fn receive_ready_once(&mut self,channel:&File,child:&mut std::process::Child)->anyhow::Result<()> {
        anyhow::ensure!(self.dispatched&&!self.ready,"Ordinary readiness attempt unavailable");self.ready=true;self.fence()?;
        anyhow::ensure!(child.try_wait()?.is_none(),"Ordinary child exited before readiness");
        let flags=unsafe{libc::fcntl(channel.as_raw_fd(),libc::F_GETFL)};
        anyhow::ensure!(flags>=0&&unsafe{libc::fcntl(channel.as_raw_fd(),libc::F_SETFL,flags|libc::O_NONBLOCK)}==0,"Ordinary readiness nonblocking channel unavailable");
        let packet=verification_worker_receive(channel)?;
        anyhow::ensure!(packet.files.is_empty()&&packet.credentials.pid==i32::try_from(child.id())?&&packet.credentials.uid==unsafe{libc::geteuid()}&&packet.credentials.gid==unsafe{libc::getegid()},"Ordinary readiness is not actual selected child");
        let ready:OrdinaryListenerReady=serde_json::from_slice(&packet.bytes)?;
        anyhow::ensure!(ready.purpose=="starlane-ordinary-listener-ready"&&ready.version==1&&ready.nonce==self.wire.nonce&&ready.listener_identity==self.wire.listener_identity&&ready.address==self.wire.address,"Ordinary readiness original selection differs");
        let actual=File::open(format!("/proc/{}/exe",child.id()))?;anyhow::ensure!(verification_worker_stamp(&actual)?==self.wire.executable_stamp&&hash_file(&actual,actual.metadata()?.len())?==self.wire.executable_sha256,"Ordinary actual child ELF differs");
        anyhow::ensure!(child.try_wait()?.is_none(),"Ordinary child exited during readiness");self.fence()
    }
}
pub(crate) struct InheritedOrdinaryListener {
    root:Root, listener:File, executable:File, parent_executable:File,
    channel:File, wire:OrdinaryListenerWire, parent:libc::ucred,
}
impl InheritedOrdinaryListener {
    fn receive(channel:File)->anyhow::Result<Self> {
        let mut packet=verification_worker_receive(&channel)?;
        anyhow::ensure!(packet.credentials.pid==unsafe{libc::getppid()}&&packet.credentials.uid==unsafe{libc::geteuid()}&&packet.credentials.gid==unsafe{libc::getegid()},"Ordinary admission not original current parent");
        let wire:OrdinaryListenerWire=serde_json::from_slice(&packet.bytes)?;
        anyhow::ensure!(wire.purpose=="starlane-ordinary-listener-admission"&&wire.version==1&&uuid::Uuid::parse_str(&wire.nonce)?.to_string()==wire.nonce&&wire.ancestry.len()>=2&&packet.files.len()==wire.ancestry.len()+2,"Ordinary purpose/descriptor envelope invalid");
        let executable=packet.files.pop().context("Ordinary executable absent")?;let listener=packet.files.pop().context("Ordinary listener absent")?;
        let root=Root{absolute:wire.root.clone(),chain:packet.files};ordinary_listener_root_identity(&root)?;
        for (file,stamp) in root.chain.iter().zip(&wire.ancestry) {anyhow::ensure!(file.metadata()?.is_dir()&&verification_worker_stamp(file)?==*stamp,"Ordinary inherited target ancestry differs");}
        anyhow::ensure!(Root::open(&crate::storage::data_dir())?.absolute==root.absolute,"Ordinary configured root differs from inherited selection");
        let actual=File::open("/proc/self/exe")?;let parent_executable=File::open(format!("/proc/{}/exe",packet.credentials.pid))?;
        anyhow::ensure!(verification_worker_stamp(&actual)?==wire.executable_stamp&&verification_worker_stamp(&parent_executable)?==wire.executable_stamp&&verification_worker_stamp(&executable)?==wire.executable_stamp&&hash_file(&executable,executable.metadata()?.len())?==wire.executable_sha256,"Ordinary original parent/child are not same held ELF");
        let value=Self{root,listener,executable,parent_executable,channel,wire,parent:packet.credentials};value.fence()?;Ok(value)
    }
    fn fence(&self)->anyhow::Result<()> {
        anyhow::ensure!(unsafe{libc::getppid()}==self.parent.pid,"Ordinary original parent lost");ordinary_listener_root_identity(&self.root)?;
        anyhow::ensure!(ordinary_listener_identity(&self.listener)?==self.wire.listener_identity&&ordinary_listener_address(&self.listener)?==self.wire.address,"Ordinary inherited listener changed");
        anyhow::ensure!(verification_worker_stamp(&self.executable)?==self.wire.executable_stamp&&verification_worker_stamp(&self.parent_executable)?==self.wire.executable_stamp&&verification_worker_stamp(&File::open(format!("/proc/{}/exe",self.parent.pid))?)?==self.wire.executable_stamp,"Ordinary held ELF/parent changed");Ok(())
    }
    pub(crate) fn ready(self)->anyhow::Result<(tokio::net::TcpListener,OrdinaryListenerLifetime)> {
        use std::os::fd::{IntoRawFd,FromRawFd};self.fence()?;
        let ready=OrdinaryListenerReady{purpose:"starlane-ordinary-listener-ready".into(),version:1,nonce:self.wire.nonce.clone(),listener_identity:self.wire.listener_identity,address:self.wire.address};
        // Convert the original received socket; never bind a replacement port.
        let listener=unsafe{std::net::TcpListener::from_raw_fd(self.listener.try_clone()?.into_raw_fd())};listener.set_nonblocking(true)?;
        let listener=tokio::net::TcpListener::from_std(listener)?;self.fence()?;
        ordinary_listener_send(&self.channel,&serde_json::to_vec(&ready)?,&[])?;
        Ok((listener,OrdinaryListenerLifetime(self)))
    }
}
// Keep original root/parent/executable/transport handles alive through serve.
pub(crate) struct OrdinaryListenerLifetime(InheritedOrdinaryListener);
pub(crate) fn take_inherited_ordinary_listener()->anyhow::Result<Option<InheritedOrdinaryListener>> {
    use std::os::fd::FromRawFd;
    let Some(raw)=std::env::var_os(ORDINARY_LISTENER_CHANNEL) else{return Ok(None)};
    std::env::remove_var(ORDINARY_LISTENER_CHANNEL);
    let fd=raw.to_str().context("Ordinary channel locator invalid")?.parse::<i32>()?;
    anyhow::ensure!(fd>=3,"Ordinary channel cannot borrow standard input/output");
    // The locator grants nothing. Native seqpacket credentials/rights and same
    // actual parent/child ELF admission above are mandatory before service writes.
    anyhow::ensure!(unsafe{libc::fcntl(fd,libc::F_GETFD)}>=0,"Ordinary channel descriptor absent");
    verification_worker_socket(fd)?;let channel=unsafe{File::from_raw_fd(fd)};
    Ok(Some(InheritedOrdinaryListener::receive(channel)?))
}

#[cfg(test)]
mod ordinary_listener_controls {
    use super::*;
    use std::os::unix::{fs::PermissionsExt,process::CommandExt};
    const CHILD:&str="storage::restoration::linux::ordinary_listener_controls::ordinary_listener_same_elf_kernel_child";
    #[tokio::test]
    #[ignore="purpose-owned same-test-ELF transport subprocess only; not ordinary service or verified record"]
    async fn ordinary_listener_same_elf_kernel_child()->anyhow::Result<()> {
        let selected=take_inherited_ordinary_listener()?.context("Actual inherited listener absent")?;
        let (listener,_lifetime)=selected.ready()?;
        let (mut connection,_)=tokio::time::timeout(std::time::Duration::from_secs(15),listener.accept()).await??;
        use tokio::io::{AsyncWriteExt,AsyncReadExt};connection.write_all(b"ACTUAL SAME ELF KERNEL CHILD").await?;let mut done=[0u8;1];anyhow::ensure!(tokio::time::timeout(std::time::Duration::from_secs(15),connection.read(&mut done)).await??==0,"Unexpected transport probe data");Ok(())
    }
    fn private_root()->anyhow::Result<(PathBuf,Root)> {
        let path=std::env::temp_dir().join(format!("ordinary-listener-owned-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&path)?;std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o700))?;
        Ok((path.clone(),Root::open(&path)?))
    }
    fn spawn(channel:&File,path:&Path)->anyhow::Result<std::process::Child> {
        let fd=channel.as_raw_fd();let mut command=std::process::Command::new("/proc/self/exe");
        command.args(["--exact",CHILD,"--ignored","--nocapture","--test-threads=1"]).env(ORDINARY_LISTENER_CHANNEL,fd.to_string()).env("DATA_DIR",path).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        unsafe{command.pre_exec(move||{let flags=libc::fcntl(fd,libc::F_GETFD);if flags<0||libc::fcntl(fd,libc::F_SETFD,flags&!libc::FD_CLOEXEC)<0{return Err(io::Error::last_os_error());}Ok(())});}
        Ok(command.spawn()?)
    }
    fn readable(channel:&File,child:&mut std::process::Child)->anyhow::Result<()> {
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
        loop {anyhow::ensure!(child.try_wait()?.is_none(),"Actual transport child exited before readiness");let mut poll=libc::pollfd{fd:channel.as_raw_fd(),events:libc::POLLIN,revents:0};let status=unsafe{libc::poll(&mut poll,1,100)};anyhow::ensure!(status>=0,"Owned readiness poll failed");if status>0{return Ok(())}anyhow::ensure!(std::time::Instant::now()<deadline,"Owned transport readiness timed out");}
    }
    #[test]
    fn ordinary_listener_actual_same_elf_ready_retains_port_after_child_exit()->anyhow::Result<()> {
        use std::io::Read;
        let (_path,root)=private_root()?;let mut parent=OrdinaryListenerParent::prepare(&root)?;let (a,b)=verification_worker_pair()?;parent.queue_once(&a)?;
        let mut child=spawn(&b,&root.absolute)?;drop(b);
        let result=(||->anyhow::Result<()>{readable(&a,&mut child)?;parent.receive_ready_once(&a,&mut child)?;anyhow::ensure!(parent.receive_ready_once(&a,&mut child).is_err(),"Consumed readiness was replayed");let address=parent.wire.address;
            let mut connection=std::net::TcpStream::connect(address)?;connection.set_read_timeout(Some(std::time::Duration::from_secs(15)))?;let mut bytes=[0u8;b"ACTUAL SAME ELF KERNEL CHILD".len()];connection.read_exact(&mut bytes)?;anyhow::ensure!(&bytes==b"ACTUAL SAME ELF KERNEL CHILD","Actual child TCP bytes differ");drop(connection);
            let status=child.wait()?;anyhow::ensure!(status.success(),"Actual transport child failed");
            anyhow::ensure!(std::net::TcpListener::bind(address).is_err(),"Foreign replacement rebound original parent-retained port");
            anyhow::ensure!(parent.queue_once(&a).is_err(),"Consumed attempt reused");drop(parent);let replacement=std::net::TcpListener::bind(address)?;drop(replacement);Ok(())})();
        if child.try_wait()?.is_none(){child.kill()?;let _=child.wait()?;}result
    }
    fn fill_queue(channel: &File) -> anyhow::Result<usize> {
        let payload = [7u8; 16384];
        let mut count = 0;
        loop {
            match ordinary_listener_send(channel, &payload, &[]) {
                Ok(()) => { count += 1; anyhow::ensure!(count < 4096, "Kernel queue did not enforce bounded pressure"); }
                Err(error) => { anyhow::ensure!(error.kind() == io::ErrorKind::WouldBlock, "Unexpected bounded queue failure"); return Ok(count); }
            }
        }
    }
    fn nonblocking_receive(channel: &File) -> io::Result<VerificationWorkerPacket> {
        let flags = unsafe { libc::fcntl(channel.as_raw_fd(), libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(channel.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        verification_worker_receive(channel)
    }
    fn assert_empty_channel(channel: &File) -> anyhow::Result<()> {
        match nonblocking_receive(channel) {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(()),
            _ => anyhow::bail!("Refused attempt delivered a packet or channel inspection failed"),
        }
    }
    fn drain_queue(channel: &File, count: usize) -> anyhow::Result<()> {
        for _ in 0..count {
            let packet = nonblocking_receive(channel)?;
            anyhow::ensure!(packet.files.is_empty() && packet.bytes == [7u8; 16384], "Pressure queue contains custody rights or unexpected packet");
        }
        assert_empty_channel(channel)
    }
    fn assert_no_custody_phases(timing: &OrdinaryQueueTiming) -> anyhow::Result<()> {
        anyhow::ensure!(timing.pre_fence.is_none() && timing.ancestry_serialization.is_none() && timing.send.is_none() && timing.post_fence.is_none(), "Early refusal entered custody phases: {timing:?}");
        Ok(())
    }
    #[test]
    fn ordinary_listener_foreign_socket_and_queue_pressure_refuse()->anyhow::Result<()> {
        let (_path,root)=private_root()?;let mut parent=OrdinaryListenerParent::prepare(&root)?;
        let null=File::open("/dev/null")?;anyhow::ensure!(ordinary_listener_address(&null).is_err(),"Foreign non-socket accepted");
        let tcp=std::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED,0))?;
        use std::os::fd::{IntoRawFd,FromRawFd};let foreign=unsafe{File::from_raw_fd(tcp.into_raw_fd())};anyhow::ensure!(ordinary_listener_address(&foreign).is_err(),"Non-loopback listener accepted");
        let (a,b)=verification_worker_pair()?;
        let count = fill_queue(&a)?;
        let executable_bytes = parent.executable.metadata()?.len();
        ORDINARY_QUEUE_TIMING.with(|timing| *timing.borrow_mut() = Some(OrdinaryQueueTiming {
            executable_bytes, filled_packets: count, ..Default::default()
        }));
        let began=std::time::Instant::now();
        let queued = parent.queue_once(&a);
        let total = began.elapsed();
        let timing = ORDINARY_QUEUE_TIMING.with(|timing| timing.borrow_mut().take());
        // Read these explicitly so the diagnostic-only fields remain checked.
        let size_and_pressure = timing.as_ref().map(|timing| (timing.executable_bytes, timing.filled_packets));
        eprintln!("Ordinary listener queue observation: executable_bytes/filled_packets={size_and_pressure:?}; total={total:?}; phases={timing:?}");
        anyhow::ensure!(queued.is_err(),"Full queue admitted attempt; executable_bytes/filled_packets={size_and_pressure:?}; total={total:?}; phases={timing:?}");
        anyhow::ensure!(total<std::time::Duration::from_secs(3),"Full queue blocked custody retirement; executable_bytes/filled_packets={size_and_pressure:?}; total={total:?}; phases={timing:?}");
        let observed = timing.as_ref().context("Queue observation missing")?;
        anyhow::ensure!(observed.readiness.is_some() && observed.readiness_error == Some((io::ErrorKind::WouldBlock, None)), "Saturated channel did not refuse at readiness: {observed:?}");
        assert_no_custody_phases(observed)?;
        drain_queue(&b, count)?;
        anyhow::ensure!(ordinary_listener_queue_ready(&a).is_ok(), "Draining did not restore actual capacity");
        anyhow::ensure!(parent.queue_once(&a).is_err(),"Failed send attempt retried");
        assert_empty_channel(&b)?;
        Ok(())
    }
    #[test]
    fn ordinary_listener_closed_peer_consumes_without_custody_checks() -> anyhow::Result<()> {
        let (_path, root) = private_root()?;
        let mut parent = OrdinaryListenerParent::prepare(&root)?;
        let (a, b) = verification_worker_pair()?;
        drop(b);
        ORDINARY_QUEUE_TIMING.with(|timing| *timing.borrow_mut() = Some(Default::default()));
        let result = parent.queue_once(&a);
        let timing = ORDINARY_QUEUE_TIMING.with(|timing| timing.borrow_mut().take()).context("Queue observation missing")?;
        anyhow::ensure!(result.is_err() && parent.dispatched && timing.readiness.is_some() && timing.readiness_error.is_some(), "Closed peer did not consume/refuse admission");
        assert_no_custody_phases(&timing)?;
        anyhow::ensure!(parent.queue_once(&a).is_err(), "Closed-peer attempt retried");
        Ok(())
    }
    #[test]
    fn ordinary_listener_ready_channel_retains_all_identity_fences() -> anyhow::Result<()> {
        for changed in ["root", "listener", "executable_stamp", "executable_digest"] {
            let (path, root) = private_root()?;
            let mut parent = OrdinaryListenerParent::prepare(&root)?;
            let (a, b) = verification_worker_pair()?;
            match changed {
                "root" => {
                    std::fs::rename(&path, path.with_extension("original"))?;
                    std::fs::create_dir(&path)?;
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
                }
                "listener" => {
                    use std::os::fd::{IntoRawFd, FromRawFd};
                    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
                    parent.listener = unsafe { File::from_raw_fd(listener.into_raw_fd()) };
                }
                "executable_stamp" => parent.wire.executable_stamp.push_str("-changed"),
                "executable_digest" => parent.wire.executable_sha256.push_str("-changed"),
                _ => unreachable!(),
            }
            anyhow::ensure!(ordinary_listener_queue_ready(&a).is_ok(), "Fixture channel is not writable");
            ORDINARY_QUEUE_TIMING.with(|timing| *timing.borrow_mut() = Some(Default::default()));
            let result = parent.queue_once(&a);
            let timing = ORDINARY_QUEUE_TIMING.with(|timing| timing.borrow_mut().take()).context("Queue observation missing")?;
            anyhow::ensure!(result.is_err() && parent.dispatched && timing.readiness_error.is_none() && timing.pre_fence.is_some() && timing.send.is_none(), "Ready channel bypassed original {changed} fence: {timing:?}");
            assert_empty_channel(&b)?;
            anyhow::ensure!(parent.queue_once(&a).is_err(), "Rejected {changed} attempt retried");
            assert_empty_channel(&b)?;
        }
        Ok(())
    }
    #[test]
    fn ordinary_listener_capacity_lost_after_readiness_still_uses_nonblocking_send() -> anyhow::Result<()> {
        let (_path, root) = private_root()?;
        let mut parent = OrdinaryListenerParent::prepare(&root)?;
        let (a, b) = verification_worker_pair()?;
        let filled = std::rc::Rc::new(std::cell::Cell::new(0));
        let hook_count = filled.clone();
        ORDINARY_QUEUE_AFTER_READINESS.with(|hook| *hook.borrow_mut() = Some(Box::new(move |channel| {
            hook_count.set(fill_queue(channel)?);
            Ok(())
        })));
        ORDINARY_QUEUE_TIMING.with(|timing| *timing.borrow_mut() = Some(Default::default()));
        let result = parent.queue_once(&a);
        let timing = ORDINARY_QUEUE_TIMING.with(|timing| timing.borrow_mut().take()).context("Queue observation missing")?;
        anyhow::ensure!(result.is_err() && parent.dispatched && filled.get() > 0, "Post-readiness pressure admitted custody");
        anyhow::ensure!(timing.readiness_error.is_none() && timing.pre_fence.is_some() && timing.ancestry_serialization.is_some() && timing.post_fence.is_none(), "Post-readiness race skipped custody checks: {timing:?}");
        anyhow::ensure!(matches!(timing.send_error, Some((io::ErrorKind::WouldBlock, Some(code))) if code == libc::EAGAIN || code == libc::EWOULDBLOCK), "Post-readiness send did not report native WouldBlock: {timing:?}");
        anyhow::ensure!(timing.send.is_some_and(|elapsed| elapsed < std::time::Duration::from_secs(3)), "Post-readiness send blocked: {timing:?}");
        drain_queue(&b, filled.get())?;
        anyhow::ensure!(parent.queue_once(&a).is_err(), "Post-readiness failed attempt retried");
        assert_empty_channel(&b)?;
        Ok(())
    }
    #[test]
    fn ordinary_listener_wrong_purpose_and_foreign_descriptor_child_refuse()->anyhow::Result<()> {
        for foreign in [false,true] {
            let (_path,root)=private_root()?;let parent=OrdinaryListenerParent::prepare(&root)?;let (a,b)=verification_worker_pair()?;
            let mut wire=serde_json::to_value(&parent.wire)?;if !foreign {wire["purpose"]="starlane-ordinary-listener-ready".into();}
            let null=File::open("/dev/null")?;let mut files:Vec<&File>=root.chain.iter().collect();files.push(if foreign{&null}else{&parent.listener});files.push(&parent.executable);
            ordinary_listener_send(&a,&serde_json::to_vec(&wire)?,&files)?;let mut child=spawn(&b,&root.absolute)?;drop(b);let status=child.wait()?;anyhow::ensure!(!status.success(),"Wrong purpose/foreign listener granted readiness");
        } Ok(())
    }
    #[test]
    fn ordinary_listener_failed_spawn_consumes_original_attempt()->anyhow::Result<()> {
        let (_path,root)=private_root()?;let mut parent=OrdinaryListenerParent::prepare(&root)?;let(a,_b)=verification_worker_pair()?;parent.queue_once(&a)?;
        anyhow::ensure!(std::process::Command::new(root.absolute.join("missing-original-executable")).spawn().is_err(),"Missing executable unexpectedly spawned");
        anyhow::ensure!(parent.queue_once(&a).is_err(),"Failed-spawn original attempt reused");Ok(())
    }
    #[test]
    fn ordinary_listener_actual_reuse_port_socket_refuses()->anyhow::Result<()> {
        use std::os::fd::FromRawFd;
        let raw=unsafe{libc::socket(libc::AF_INET,libc::SOCK_STREAM|libc::SOCK_CLOEXEC,0)};anyhow::ensure!(raw>=0,"Owned reusable socket creation failed");let socket=unsafe{File::from_raw_fd(raw)};let yes=1i32;
        anyhow::ensure!(unsafe{libc::setsockopt(raw,libc::SOL_SOCKET,libc::SO_REUSEPORT,(&yes as *const i32).cast(),std::mem::size_of_val(&yes)as libc::socklen_t)}==0,"Owned reusable option setup failed");
        let address=libc::sockaddr_in{sin_family:libc::AF_INET as _,sin_port:0,sin_addr:libc::in_addr{s_addr:u32::from_ne_bytes([127,0,0,1])},sin_zero:[0;8]};
        anyhow::ensure!(unsafe{libc::bind(raw,(&address as *const libc::sockaddr_in).cast(),std::mem::size_of_val(&address)as libc::socklen_t)}==0&&unsafe{libc::listen(raw,8)}==0,"Owned reusable loopback bind/listen failed");
        anyhow::ensure!(ordinary_listener_address(&socket).is_err(),"Reusable listening port granted exclusive destination authority");Ok(())
    }
    #[test]
    fn ordinary_listener_actual_child_nonce_mismatch_consumes_readiness()->anyhow::Result<()> {
        let (_path,root)=private_root()?;let mut parent=OrdinaryListenerParent::prepare(&root)?;let(a,b)=verification_worker_pair()?;
        let mut wire=serde_json::to_value(&parent.wire)?;wire["nonce"]=uuid::Uuid::new_v4().to_string().into();let mut files:Vec<&File>=root.chain.iter().collect();files.extend([&parent.listener,&parent.executable]);ordinary_listener_send(&a,&serde_json::to_vec(&wire)?,&files)?;parent.dispatched=true;
        let mut child=spawn(&b,&root.absolute)?;drop(b);let result=(||->anyhow::Result<()>{readable(&a,&mut child)?;anyhow::ensure!(parent.receive_ready_once(&a,&mut child).is_err(),"Actual child foreign nonce accepted");anyhow::ensure!(parent.receive_ready_once(&a,&mut child).is_err(),"Rejected readiness attempt retried");Ok(())})();let _=child.kill();let _=child.wait();result
    }

}
