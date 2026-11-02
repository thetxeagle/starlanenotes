// The original child sends its genuine listening socket over the authenticated
// private channel. Retaining that socket prevents another process rebinding the
// advertised port after child death while a supplied bearer request is pending.
struct NormalVerificationListener {
    held: File,
    native: Identity,
    address: std::net::SocketAddr,
}
fn normal_listener_address(file: &File) -> io::Result<std::net::SocketAddr> {
    for (option, expected) in [(libc::SO_TYPE, libc::SOCK_STREAM),
        (libc::SO_DOMAIN, libc::AF_INET), (libc::SO_ACCEPTCONN, 1), (libc::SO_REUSEPORT, 0)] {
        let mut actual = 0i32;
        let mut size = std::mem::size_of::<i32>() as libc::socklen_t;
        if unsafe { libc::getsockopt(file.as_raw_fd(), libc::SOL_SOCKET, option,
            (&mut actual as *mut i32).cast(), &mut size) } != 0 { return Err(io::Error::last_os_error()); }
        if actual != expected || size as usize != std::mem::size_of::<i32>() {
            return Err(fail("Original normal endpoint is not a listening IPv4 TCP socket"));
        }
    }
    let mut address: libc::sockaddr_in = unsafe { std::mem::zeroed() };
    let mut size = std::mem::size_of_val(&address) as libc::socklen_t;
    if unsafe { libc::getsockname(file.as_raw_fd(), (&mut address as *mut libc::sockaddr_in).cast(), &mut size) } != 0 {
        return Err(io::Error::last_os_error());
    }
    if address.sin_family as i32 != libc::AF_INET || size as usize != std::mem::size_of::<libc::sockaddr_in>() {
        return Err(fail("Original normal endpoint address role differs"));
    }
    let ip = std::net::Ipv4Addr::from(u32::from_be(address.sin_addr.s_addr));
    let port = u16::from_be(address.sin_port);
    if !ip.is_loopback() || port == 0 { return Err(fail("Original normal endpoint must be bound loopback")); }
    Ok(std::net::SocketAddr::from((ip, port)))
}
fn normal_duplicate_listener(fd: i32) -> io::Result<File> {
    use std::os::fd::FromRawFd;
    let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    if duplicate < 0 { return Err(io::Error::last_os_error()); }
    let file = unsafe { File::from_raw_fd(duplicate) };
    normal_listener_address(&file)?; Ok(file)
}
impl NormalVerificationListener {
    fn admit(packet: &mut VerificationWorkerPacket, pid: i32, ready: &NormalVerificationReady,
        nonce: &str, executable: &File) -> anyhow::Result<Self> {
        anyhow::ensure!(packet.credentials.pid == pid && packet.credentials.uid == unsafe { libc::geteuid() }
            && packet.credentials.gid == unsafe { libc::getegid() } && packet.files.len() == 1 && ready.nonce == nonce,
            "Original normal listener is not from the selected child");
        let actual = File::open(format!("/proc/{pid}/exe"))?;
        anyhow::ensure!(identity(&actual)? == identity(executable)?
            && verification_worker_stamp(&actual)? == verification_worker_stamp(executable)?,
            "Original normal listener sender executable differs");
        let held = packet.files.pop().ok_or_else(|| fail("Original normal listener descriptor absent"))?;
        anyhow::ensure!(normal_listener_address(&held)? == ready.address, "Ready address differs from original native listener");
        let result = Self { native: identity(&held)?, held, address: ready.address };
        result.fence()?; Ok(result)
    }
    fn fence(&self) -> io::Result<()> {
        if identity(&self.held)? != self.native || normal_listener_address(&self.held)? != self.address {
            return Err(fail("Original retained normal listener changed"));
        }
        Ok(())
    }
}

#[cfg(test)] mod normal_listener_controls {
    use super::*;
    #[test] fn retained_listener_blocks_port_reuse_until_original_fd_released() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let held = normal_duplicate_listener(listener.as_raw_fd()).unwrap();
        assert_eq!(normal_listener_address(&held).unwrap(), address);
        drop(listener);
        assert_eq!(std::net::TcpListener::bind(address).unwrap_err().kind(), io::ErrorKind::AddrInUse);
        drop(held);
        assert!(std::net::TcpListener::bind(address).is_ok());
        assert!(normal_listener_address(&File::open("/dev/null").unwrap()).is_err());
        let (pair, _) = verification_worker_pair().unwrap();
        assert!(normal_listener_address(&pair).is_err());
    }
    #[test] fn reusable_port_listener_cannot_be_admitted() -> anyhow::Result<()> {
        use std::os::fd::FromRawFd;
        let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_CLOEXEC, 0) };
        anyhow::ensure!(fd >= 0, "Native socket allocation failed");
        let file = unsafe { File::from_raw_fd(fd) };
        let enabled = 1i32;
        anyhow::ensure!(unsafe { libc::setsockopt(fd, libc::SOL_SOCKET, libc::SO_REUSEPORT,
            (&enabled as *const i32).cast(), std::mem::size_of_val(&enabled) as libc::socklen_t) } == 0,
            "Native reusable-port option failed");
        let mut address: libc::sockaddr_in = unsafe { std::mem::zeroed() };
        address.sin_family = libc::AF_INET as libc::sa_family_t;
        address.sin_addr.s_addr = u32::from(std::net::Ipv4Addr::LOCALHOST).to_be();
        anyhow::ensure!(unsafe { libc::bind(fd, (&address as *const libc::sockaddr_in).cast(),
            std::mem::size_of_val(&address) as libc::socklen_t) } == 0 && unsafe { libc::listen(fd, 16) } == 0,
            "Native reusable-port listen failed");
        anyhow::ensure!(normal_listener_address(&file).is_err(), "Reusable-port listener acquired retained endpoint authority");
        Ok(())
    }
    #[test] fn genuine_kernel_listener_transfer_survives_actual_sender_exit() -> anyhow::Result<()> {
        use std::os::unix::process::CommandExt;
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let (parent, child) = verification_worker_pair()?;
        let socket = child.as_raw_fd(); let native = listener.as_raw_fd();
        // Reuse the verified installed Python3 rather than a distro minor path.
        let (python, _held_python, _stamp, _sha) = sqlite_capture_python()?;
        let mut command = std::process::Command::new(python);
        command.arg("-I");
        command.args(["-c", "import socket,array,sys; s=socket.socket(fileno=int(sys.argv[1])); s.sendmsg([b'original-listener'],[(socket.SOL_SOCKET,socket.SCM_RIGHTS,array.array('i',[int(sys.argv[2])]))]); s.recv(32)", &socket.to_string(), &native.to_string()]);
        unsafe { command.pre_exec(move || {
            for fd in [socket, native] {
                let flags = libc::fcntl(fd, libc::F_GETFD);
                if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 { return Err(io::Error::last_os_error()); }
            }
            Ok(())
        }); }
        let mut process = command.spawn()?; let pid = process.id() as i32; drop(child);
        let mut packet = verification_worker_receive(&parent)?;
        anyhow::ensure!(packet.credentials.pid == pid && packet.files.len() == 1, "Actual listener sender differs");
        let held = packet.files.pop().ok_or_else(|| fail("Original transferred listener absent"))?;
        anyhow::ensure!(normal_listener_address(&held)? == address, "Actual transferred listener differs");
        verification_worker_send(&parent, b"release", &[])?;
        anyhow::ensure!(process.wait()?.success(), "Actual listener sender did not exit");
        drop(listener);
        anyhow::ensure!(std::net::TcpListener::bind(address).is_err(), "Exited child port was rebound while original listener retained");
        drop(held);
        anyhow::ensure!(std::net::TcpListener::bind(address).is_ok(), "Original listener did not release after retirement"); Ok(())
    }
}
