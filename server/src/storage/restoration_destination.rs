const DESTINATION_MARKER: &str = ".starlane-restore-incomplete";
/// Private copy stage only. Dropping this token never removes recoverable data.
pub(crate) struct PreparedDestination {
    root: Root,
    parent: Root,
    name: String,
    marker: File,
    marker_bytes: Vec<u8>,
    // Exact admitted source; future consumers must reject cross-paired bundles.
    capture_manifest_sha256: String,
    directories: std::collections::BTreeMap<String, Identity>,
    files: std::collections::BTreeMap<String, (Identity, crate::storage::backup_inventory::Bytes)>,
}
fn destination_directory(root: &File, relative: &str) -> io::Result<File> {
    let mut fd = root.try_clone()?;
    if relative.is_empty() { return Ok(fd); }
    if !valid_relative(relative) || relative.contains('\\') { return Err(fail("Unsafe destination directory")); }
    for part in relative.split('/') { fd = open_at(&fd, part, libc::O_RDONLY | libc::O_DIRECTORY)?; }
    Ok(fd)
}
fn destination_hash(file: &File) -> io::Result<crate::storage::backup_inventory::Bytes> {
    let before = file.metadata()?;
    if !before.is_file() || before.mode() & 0o777 != 0o600 || before.nlink() != 1 {
        return Err(fail("Destination leaf is not private and independent"));
    }
    let mut hash = Sha256::new();let mut offset = 0;let mut buffer = [0u8;65536];
    while offset < before.len() {
        let wanted = (before.len()-offset).min(buffer.len() as u64) as usize;
        let n = file.read_at(&mut buffer[..wanted], offset)?;
        if n == 0 { return Err(fail("Destination bytes truncated")); }
        hash.update(&buffer[..n]);offset += n as u64;
    }
    let after = file.metadata()?;
    if (before.dev(),before.ino(),before.len(),before.mtime(),before.mtime_nsec(),before.ctime(),before.ctime_nsec())
        != (after.dev(),after.ino(),after.len(),after.mtime(),after.mtime_nsec(),after.ctime(),after.ctime_nsec()) {
        return Err(fail("Destination bytes changed"));
    }
    Ok(crate::storage::backup_inventory::Bytes{size_bytes:offset,sha256:hex::encode(hash.finalize())})
}
impl PreparedDestination {
    pub(crate) fn path(&self) -> &Path { &self.root.absolute }
    fn guard(&self) -> io::Result<()> {
        // Root::check rejects the intentionally retained marker, so compare
        // ancestry explicitly and validate that exact marker instead.
        for pinned in [&self.parent,&self.root] {
            let fresh = Root::open(&pinned.absolute)?;
            if pinned.chain.len() != fresh.chain.len() { return Err(fail("Destination ancestry changed")); }
            for (a,b) in pinned.chain.iter().zip(&fresh.chain) {
                if identity(a)? != identity(b)? { return Err(fail("Destination ancestry changed")); }
            }
        }
        if identity(&open_at(self.parent.fd(),&self.name,libc::O_RDONLY|libc::O_DIRECTORY)?)? != identity(self.root.fd())?
            || self.root.fd().metadata()?.mode() & 0o777 != 0o700 {
            return Err(fail("Destination root changed"));
        }
        let marker = open_at(self.root.fd(),DESTINATION_MARKER,libc::O_RDONLY|libc::O_NONBLOCK)?;
        let metadata = marker.metadata()?;
        if identity(&marker)? != identity(&self.marker)? || !metadata.is_file() || metadata.nlink()!=1
            || metadata.mode() & 0o777 != 0o600 || metadata.len()!=self.marker_bytes.len() as u64 {
            return Err(fail("Destination preparation marker changed"));
        }
        let mut bytes=vec![0;self.marker_bytes.len()];let mut offset=0;
        while offset<bytes.len() {let n=marker.read_at(&mut bytes[offset..],offset as u64)?;if n==0{return Err(fail("Destination marker truncated"));}offset+=n;}
        if bytes!=self.marker_bytes || exists(self.root.fd(),".starlane-backup-incomplete")? {
            return Err(fail("Destination preparation marker changed"));
        }
        Ok(())
    }
    /// Exact copy inventory plus one unchanged incomplete marker. No activation.
    pub(crate) fn fence(&self) -> io::Result<()> {
        self.guard()?;
        fn walk(fd:&File,prefix:&str,out:&mut std::collections::BTreeMap<String,(Identity,crate::storage::backup_inventory::Bytes)>,dirs:&mut std::collections::BTreeMap<String,Identity>) -> io::Result<()> {
            if fd.metadata()?.mode() & 0o777 != 0o700 {return Err(fail("Destination directory is not private"));}
            for entry in std::fs::read_dir(format!("/proc/self/fd/{}",fd.as_raw_fd()))? {
                let name=entry?.file_name().into_string().map_err(|_|fail("Non-UTF8 destination entry"))?;
                if name==DESTINATION_MARKER && prefix.is_empty(){continue;}
                if !valid_relative(&name) || name.contains(['/', '\\']) {return Err(fail("Unsafe destination entry"));}
                let path=if prefix.is_empty(){name.clone()}else{format!("{prefix}/{name}")};
                let file=open_at(fd,&name,libc::O_RDONLY|libc::O_NONBLOCK)?;
                if file.metadata()?.is_dir(){dirs.insert(path.clone(),identity(&file)?);walk(&file,&path,out,dirs)?;}
                else {out.insert(path,(identity(&file)?,destination_hash(&file)?));}
            }
            Ok(())
        }
        let mut files=std::collections::BTreeMap::new();let mut dirs=std::collections::BTreeMap::new();
        walk(self.root.fd(),"",&mut files,&mut dirs)?;
        if files!=self.files || dirs!=self.directories {return Err(fail("Destination copy bytes or topology changed"));}
        self.guard()
    }
}
pub(crate) fn reserve_and_copy(bundle:&crate::storage::backup_inventory::ValidatedBundle,target:&Path) -> io::Result<PreparedDestination> {
    reserve_and_copy_inner(bundle,target,||Ok(()))
}
fn reserve_and_copy_inner(bundle:&crate::storage::backup_inventory::ValidatedBundle,target:&Path,after_reservation:impl FnOnce()->io::Result<()>) -> io::Result<PreparedDestination> {
    use std::io::Write;
    bundle.fence()?;
    let absolute=if target.is_absolute(){target.to_owned()}else{std::env::current_dir()?.join(target)};
    if absolute.components().any(|p|!matches!(p,Component::RootDir|Component::Normal(_))) {return Err(fail("Unsafe destination path"));}
    let bundle_root=bundle.data_path().parent().ok_or_else(||fail("Invalid bundle root"))?.to_owned();
    if absolute.starts_with(&bundle_root) || bundle_root.starts_with(&absolute) {return Err(fail("Destination overlaps backup bundle"));}
    let parent=Root::open(absolute.parent().ok_or_else(||fail("Destination parent missing"))?)?;
    parent.check()?;
    let boundaries=bundle.copy_overlap_identities()?;
    let original=(bundle.manifest.source_custody.root.device,bundle.manifest.source_custody.root.inode);
    for fd in &parent.chain {
        let observed=identity(fd)?;
        if boundaries.contains(&observed) || observed==original {
            return Err(fail("Destination parent overlaps backup or captured source"));
        }
    }
    let name=absolute.file_name().and_then(|p|p.to_str()).ok_or_else(||fail("Invalid destination name"))?.to_owned();
    if exists(parent.fd(),&name)? {return Err(io::Error::new(io::ErrorKind::AlreadyExists,"Destination already exists"));}
    let reservation=format!(".starlane-restore-{}.pending",uuid::Uuid::new_v4());
    let cname=CString::new(reservation.as_str()).map_err(|_|fail("Invalid reservation name"))?;
    // SAFETY: held no-follow parent and unique leaf; mkdir is exclusive.
    if unsafe{libc::mkdirat(parent.fd().as_raw_fd(),cname.as_ptr(),0o700)}!=0{return Err(io::Error::last_os_error());}
    let fd=open_at(parent.fd(),&reservation,libc::O_RDONLY|libc::O_DIRECTORY)?;
    let mut chain=Vec::new();for part in &parent.chain {chain.push(part.try_clone()?);}chain.push(fd);
    let root=Root{absolute:parent.absolute.join(&reservation),chain};
    let mut marker=open_at(root.fd(),DESTINATION_MARKER,libc::O_RDWR|libc::O_CREAT|libc::O_EXCL)?;
    let marker_bytes=format!("starlane private copy {}\n",bundle.manifest.backup_id).into_bytes();
    marker.write_all(&marker_bytes)?;marker.sync_all()?;root.fd().sync_all()?;parent.fd().sync_all()?;
    let mut prepared=PreparedDestination{root,parent,name:reservation.clone(),marker,marker_bytes,capture_manifest_sha256:hex::encode(Sha256::digest(&bundle.raw_manifest)),directories:Default::default(),files:Default::default()};
    prepared.guard()?;after_reservation()?;prepared.guard()?;bundle.fence()?;
    let destination_name=CString::new(name.as_str()).map_err(|_|fail("Invalid destination name"))?;
    // SAFETY: both leaves are under the held parent. Linux NOREPLACE publishes
    // the already durably marked reservation without overwriting any entry.
    if unsafe{libc::renameat2(prepared.parent.fd().as_raw_fd(),cname.as_ptr(),prepared.parent.fd().as_raw_fd(),destination_name.as_ptr(),libc::RENAME_NOREPLACE)}!=0 {
        return Err(io::Error::last_os_error());
    }
    prepared.name=name;prepared.root.absolute=absolute;
    prepared.parent.fd().sync_all()?;prepared.guard()?;
    for path in bundle.manifest.source_custody.directories.0.keys() {
        prepared.guard()?;
        let (parent_path,leaf)=path.rsplit_once('/').unwrap_or(("",path));
        let directory=destination_directory(prepared.root.fd(),parent_path)?;
        if !parent_path.is_empty() && prepared.directories.get(parent_path)!=Some(&identity(&directory)?) {return Err(fail("Destination parent directory changed"));}
        let leaf=CString::new(leaf).map_err(|_|fail("Unsafe destination directory"))?;
        // SAFETY: held directory and validated leaf; no existing entry is replaced.
        if unsafe{libc::mkdirat(directory.as_raw_fd(),leaf.as_ptr(),0o700)}!=0{return Err(io::Error::last_os_error());}
        directory.sync_all()?;
        let created=destination_directory(prepared.root.fd(),path)?;
        if created.metadata()?.mode() & 0o777 != 0o700{return Err(fail("Destination directory is not private"));}
        prepared.directories.insert(path.clone(),identity(&created)?);created.sync_all()?;
    }
    for (path,expected) in &bundle.manifest.files.0 {
        prepared.guard()?;
        let source=bundle.open_owned(path)?;
        let before=source.metadata()?;
        if !before.is_file() || before.len()!=expected.size_bytes {return Err(fail("Source copy length changed"));}
        let (parent_path,leaf)=path.rsplit_once('/').unwrap_or(("",path));
        let directory=destination_directory(prepared.root.fd(),parent_path)?;
        if !parent_path.is_empty() && prepared.directories.get(parent_path)!=Some(&identity(&directory)?) {return Err(fail("Destination parent directory changed"));}
        let mut output=open_at(&directory,leaf,libc::O_WRONLY|libc::O_CREAT|libc::O_EXCL)?;
        let mut hash=Sha256::new();let mut offset=0;let mut bytes=[0u8;65536];
        while offset<expected.size_bytes {
            prepared.guard()?;
            let wanted=(expected.size_bytes-offset).min(bytes.len()as u64)as usize;
            let n=source.read_at(&mut bytes[..wanted],offset)?;
            if n==0 {return Err(fail("Source bytes truncated during copy"));}
            output.write_all(&bytes[..n])?;hash.update(&bytes[..n]);offset+=n as u64;
        }
        let after=source.metadata()?;
        if (before.dev(),before.ino(),before.len(),before.mtime(),before.mtime_nsec(),before.ctime(),before.ctime_nsec())
            != (after.dev(),after.ino(),after.len(),after.mtime(),after.mtime_nsec(),after.ctime(),after.ctime_nsec())
            || hex::encode(hash.finalize())!=expected.sha256 {return Err(fail("Source bytes changed during copy"));}
        output.sync_all()?;directory.sync_all()?;
        prepared.files.insert(path.clone(),(identity(&output)?,expected.clone()));prepared.guard()?;
    }
    prepared.root.fd().sync_all()?;prepared.parent.fd().sync_all()?;
    prepared.fence()?;bundle.fence()?;prepared.fence()?;
    Ok(prepared)
}

#[cfg(test)]
mod destination_tests {
    use super::*;
    /// Host fixtures invoke this exact child against genuine captured bundles.
    /// Ordinary test runs leave the private subprocess controls unset.
    #[tokio::test]
    async fn copy_child() {
        let Ok(bundle_path) = std::env::var("STARLANE_DESTINATION_FIXTURE_BUNDLE") else { return; };
        let target = std::path::PathBuf::from(std::env::var("STARLANE_DESTINATION_FIXTURE_TARGET").unwrap());
        let mode = std::env::var("STARLANE_DESTINATION_FIXTURE_MODE").unwrap();
        let bundle = crate::storage::backup_inventory::verify(Path::new(&bundle_path)).await.unwrap();
        let prepared = if mode == "pause-before-publication" {
            reserve_and_copy_inner(&bundle, &target, || {
                use std::io::Write;
                println!("premarked reservation ready");
                std::io::stdout().flush()?;
                let mut line = String::new();
                std::io::stdin().read_line(&mut line)?;
                Ok(())
            }).unwrap()
        } else { reserve_and_copy(&bundle, &target).unwrap() };
        assert_eq!(prepared.path(), target);
        prepared.fence().unwrap(); bundle.fence().unwrap();
        assert!(bootstrap_gate(&target).is_err());
        if mode == "guard-marker" {
            std::fs::write(target.join(DESTINATION_MARKER), b"changed marker").unwrap();
            assert!(prepared.fence().is_err());
        } else if mode == "guard-bytes" {
            let relative = bundle.manifest.files.0.keys().find(|p| p.as_str() != "starlane-notes.db").unwrap();
            std::fs::write(target.join(relative), b"changed bytes").unwrap();
            assert!(prepared.fence().is_err());
        } else { assert!(mode == "copy" || mode == "pause-before-publication"); }
        bundle.db.close().await;
        println!("private marked copy completed; activation remains refused");
    }
}

/// Held no-follow output parent, refused if nested in the bundle or captured source.
/// Validate every output role before any transfer writes, then retain this token.
pub(crate) struct TransferOutputParent {parent:Root,name:String,path:PathBuf}
impl TransferOutputParent {
    pub(crate) fn validate(bundle:&crate::storage::backup_inventory::ValidatedBundle,path:&Path)->io::Result<Self> {
        bundle.fence()?;
        if !path.is_absolute()||path.components().any(|p|!matches!(p,Component::RootDir|Component::Normal(_))) {return Err(fail("Unsafe transfer output path"));}
        let data_path=bundle.data_path();
        let bundle_root=data_path.parent().ok_or_else(||fail("Invalid bundle root"))?;
        if path.starts_with(bundle_root)||bundle_root.starts_with(path){return Err(fail("Transfer output overlaps backup bundle"));}
        let parent=Root::open(path.parent().ok_or_else(||fail("Output parent missing"))?)?;parent.check()?;
        let boundaries=bundle.copy_overlap_identities()?;
        let original=(bundle.manifest.source_custody.root.device,bundle.manifest.source_custody.root.inode);
        for fd in &parent.chain {let id=identity(fd)?;if boundaries.contains(&id)||id==original{return Err(fail("Transfer output parent overlaps backup or captured source"));}}
        let name=path.file_name().and_then(|n|n.to_str()).ok_or_else(||fail("Invalid transfer output name"))?.to_owned();
        if exists(parent.fd(),&name)?{return Err(fail("Transfer output already exists"));}
        Ok(Self{parent,name,path:path.to_owned()})
    }
    pub(crate) fn create_private(self)->io::Result<TransferOutputDirectory> {
        self.parent.check()?;let name=CString::new(self.name.as_str()).map_err(|_|fail("Unsafe transfer output name"))?;
        // SAFETY: held no-follow parent, exclusive new private directory.
        if unsafe{libc::mkdirat(self.parent.fd().as_raw_fd(),name.as_ptr(),0o700)}!=0{return Err(io::Error::last_os_error());}
        let held=open_at(self.parent.fd(),&self.name,libc::O_RDONLY|libc::O_DIRECTORY)?;
        held.sync_all()?;self.parent.fd().sync_all()?;self.parent.check()?;
        let now=Root::open(&self.path)?;
        if identity(now.fd())?!=identity(&held)?{return Err(fail("Transfer output reservation changed"));}
        Ok(TransferOutputDirectory{root:now})
    }
}
pub(crate) struct TransferOutputDirectory {root:Root}
impl TransferOutputDirectory {pub(crate) fn path(&self)->&Path {&self.root.absolute}}
