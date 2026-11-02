//! Linux held delivery primitive. The caller must establish receipt, original
//! context, page association, and original file identity before capture.
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::Path;
const CHUNK: usize = 64 * 1024;
const LIMIT: u64 = 100 * 1024 * 1024;

pub(super) struct Snapshot { held: File, length: u64 }
impl Snapshot {
    pub(super) fn capture(source: &File, private_directory: &File, length: u64, sha256: &str) -> io::Result<Self> {
        if length > LIMIT || !source.metadata()?.is_file() || source.metadata()?.len() != length {
            return Err(io::Error::other("Original receipt length differs"));
        }
        let mut held = OpenOptions::new().read(true).write(true).mode(0o600)
            .custom_flags(libc::O_TMPFILE | libc::O_CLOEXEC).open(format!("/proc/self/fd/{}",std::os::fd::AsRawFd::as_raw_fd(private_directory)))?;
        let mut buffer = [0u8; CHUNK];
        let mut digest = Sha256::new();
        let mut offset = 0;
        while offset < length {
            let wanted = ((length - offset) as usize).min(CHUNK);
            let count = source.read_at(&mut buffer[..wanted], offset)?;
            if count == 0 { return Err(io::Error::new(io::ErrorKind::UnexpectedEof,"Original receipt bytes truncated")); }
            digest.update(&buffer[..count]);
            held.write_all(&buffer[..count])?;
            offset += count as u64;
        }
        if source.read_at(&mut buffer[..1], length)? != 0 || format!("{:x}", digest.finalize()) != sha256 {
            return Err(io::Error::other("Original receipt bytes differ"));
        }
        // No persistent authority is minted. An anonymous snapshot is discarded
        // on capture failure, cancellation, or drop, including ENOSPC.
        Ok(Self { held, length })
    }
    pub(super) fn into_stream_file(mut self)->io::Result<(File,u64)>{use std::io::{Seek,SeekFrom};self.held.seek(SeekFrom::Start(0))?;Ok((self.held,self.length))}
    pub(super) fn read(&self, offset: u64, requested: usize) -> io::Result<Vec<u8>> {
        if requested > CHUNK || offset > self.length { return Err(io::Error::other("Bounded snapshot range invalid")); }
        let length = requested.min((self.length-offset) as usize);
        let mut bytes = vec![0; length];
        self.held.read_exact_at(&mut bytes, offset)?;
        Ok(bytes)
    }
    /// Publish complete bytes without clobbering a user-selected destination.
    /// Parent must be a held and fenced native-dialog directory; final_name must
    /// remain the original selected leaf. Failed prepublication leaves no file.
    pub(super) fn export_no_clobber(&self, parent: &File, final_name: &str) -> io::Result<u64> {
        use std::os::fd::AsRawFd;
        if !parent.metadata()?.is_dir() || final_name.is_empty() || final_name == "." || final_name == ".." || final_name.contains('/') {
            return Err(io::Error::other("Export destination leaf invalid"));
        }
        let name=std::ffi::CString::new(final_name).map_err(|_|io::Error::other("Export name invalid"))?;
        let mut destination=OpenOptions::new().read(true).write(true).mode(0o600)
            .custom_flags(libc::O_TMPFILE|libc::O_CLOEXEC).open(format!("/proc/self/fd/{}",parent.as_raw_fd()))?;
        let copied=self.copy_to_new(&mut destination)?;
        let source=std::ffi::CString::new(format!("/proc/self/fd/{}",destination.as_raw_fd())).unwrap();
        // Public linkat interface; proc FD follows the held anonymous file. EEXIST
        // preserves every existing target, including symlinks, unchanged.
        if unsafe {libc::linkat(libc::AT_FDCWD,source.as_ptr(),parent.as_raw_fd(),name.as_ptr(),libc::AT_SYMLINK_FOLLOW)} != 0 {
            return Err(io::Error::last_os_error());
        }
        parent.sync_all().map_err(|e|io::Error::new(e.kind(),format!("Complete export published; directory durability uncertain: {e}")))?;
        Ok(copied)
    }
    /// Complete delivery into a separately held, exclusively created destination.
    /// Destination selection/publication and parent-directory sync are the caller's
    /// explicit responsibilities. Never pass a mutable user file here.
    pub(super) fn copy_to_new(&self, destination: &mut File) -> io::Result<u64> {
        if !destination.metadata()?.is_file() || destination.metadata()?.len() != 0 {
            return Err(io::Error::other("Exclusive empty export staging file required"));
        }
        let mut offset=0;
        while offset < self.length {
            let bytes=self.read(offset, CHUNK)?;
            destination.write_all(&bytes)?;
            offset += bytes.len() as u64;
        }
        destination.sync_all()?;
        Ok(offset)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;
    fn root()->std::path::PathBuf {
        let p=std::env::temp_dir().join(format!("held-delivery-{}",format!("{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())));
        std::fs::create_dir(&p).unwrap();p
    }
    #[test]
    fn anonymous_snapshot_bounded_ranges_and_full_export_survive_source_change() {
        let root=root();let input=root.join("input");
        let bytes=vec![17;100*1024*1024];
        std::fs::write(&input,&bytes).unwrap();let file=File::open(&input).unwrap();
        let snapshot=Snapshot::capture(&file,&File::open(&root).unwrap(),bytes.len() as u64,&format!("{:x}",Sha256::digest(&bytes))).unwrap();
        assert_eq!(snapshot.held.metadata().unwrap().nlink(),0);
        assert_eq!(snapshot.held.metadata().unwrap().mode()&0o777,0o600);
        assert!(snapshot.read(0,CHUNK+1).is_err());
        std::fs::write(&input,b"changed").unwrap();
        assert_eq!(snapshot.read(0,CHUNK).unwrap(),bytes[..CHUNK]);
        let output=root.join("export");let mut export=OpenOptions::new().write(true).create_new(true).mode(0o600).open(&output).unwrap();
        assert_eq!(snapshot.copy_to_new(&mut export).unwrap(),bytes.len() as u64);
        let mut digest=Sha256::new();let file=File::open(output).unwrap();let mut offset=0;
        loop {let mut b=[0;CHUNK];let n=file.read_at(&mut b,offset).unwrap();if n==0{break}digest.update(&b[..n]);offset+=n as u64;}
        assert_eq!(format!("{:x}",digest.finalize()),format!("{:x}",Sha256::digest(&bytes)));
        drop(snapshot);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn anonymous_export_publishes_complete_private_bytes_without_clobber() {
        let root=root();let input=root.join("input");std::fs::write(&input,b"original").unwrap();
        let file=File::open(&input).unwrap();let snapshot=Snapshot::capture(&file,&File::open(&root).unwrap(),8,&format!("{:x}",Sha256::digest(b"original"))).unwrap();
        let parent=File::open(&root).unwrap();
        assert_eq!(snapshot.export_no_clobber(&parent,"selected.bin").unwrap(),8);
        assert_eq!(std::fs::read(root.join("selected.bin")).unwrap(),b"original");
        assert_eq!(std::fs::metadata(root.join("selected.bin")).unwrap().mode()&0o777,0o600);
        assert_eq!(snapshot.export_no_clobber(&parent,"selected.bin").unwrap_err().kind(),io::ErrorKind::AlreadyExists);
        std::os::unix::fs::symlink("input",root.join("symlink")).unwrap();
        assert_eq!(snapshot.export_no_clobber(&parent,"symlink").unwrap_err().kind(),io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(),3);
        assert!(snapshot.export_no_clobber(&parent,"../escape").is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn mismatching_receipt_rejects_without_named_snapshot_or_overwriting_export() {
        let root=root();let input=root.join("input");std::fs::write(&input,b"original").unwrap();
        let file=File::open(&input).unwrap();
        assert!(Snapshot::capture(&file,&File::open(&root).unwrap(),7,"invalid").is_err());
        assert!(Snapshot::capture(&file,&File::open(&root).unwrap(),8,"invalid").is_err());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(),1);
        let snapshot=Snapshot::capture(&file,&File::open(&root).unwrap(),8,&format!("{:x}",Sha256::digest(b"original"))).unwrap();
        let mut output=OpenOptions::new().write(true).open(input).unwrap();
        assert!(snapshot.copy_to_new(&mut output).is_err());std::fs::remove_dir_all(root).unwrap();
    }
}
