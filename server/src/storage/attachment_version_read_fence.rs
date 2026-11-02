//! Held readonly reader paths; no installation or inode-adoption authority.
use std::os::unix::fs::MetadataExt;
use std::path::Path as FsPath;

#[derive(Debug, PartialEq, Eq)]
struct FileStamp(u64, u64, u64, u64, u32, i64, i64, i64, i64);
fn file_stamp(f: &std::fs::File) -> std::io::Result<FileStamp> {
    let m = f.metadata()?;
    if !m.is_file() {
        return Err(std::io::Error::other("Version evidence is not regular"));
    }
    Ok(FileStamp(
        m.dev(),
        m.ino(),
        m.len(),
        m.nlink(),
        m.mode(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    ))
}
pub(crate) struct HeldVersionPath {
    root: std::fs::File,
    parents: Vec<std::fs::File>,
    relative: String,
    file: std::fs::File,
    stamp: FileStamp,
}
impl HeldVersionPath {
    pub(crate) fn open(root: &FsPath, relative: &str) -> std::io::Result<Self> {
        use super::stage;
        if !crate::storage::is_safe_relative_path(FsPath::new(relative)) {
            return Err(std::io::Error::other("Unsafe version path"));
        }
        let root = stage::root(root)?;
        let parts: Vec<_> = relative.split('/').collect();
        if parts.is_empty() || parts.len() > 32 {
            return Err(std::io::Error::other("Version path depth exceeded"));
        }
        let mut parents = vec![root.try_clone()?];
        for part in &parts[..parts.len() - 1] {
            parents.push(stage::open(
                parents.last().unwrap(),
                part,
                libc::O_RDONLY | libc::O_DIRECTORY,
            )?);
        }
        let file = stage::open(
            parents.last().unwrap(),
            parts.last().unwrap(),
            libc::O_RDONLY,
        )?;
        let stamp = file_stamp(&file)?;
        Ok(Self {
            root,
            parents,
            relative: relative.into(),
            file,
            stamp,
        })
    }
    /// Captured readonly identity; never grants installation or rebinding authority.
    pub(crate) fn captured_identity(&self) -> (u64, u64) { (self.stamp.0, self.stamp.1) }
    pub(crate) fn fence(&self, root: &FsPath) -> std::io::Result<()> {
        use super::stage;
        let fresh = Self::open(root, &self.relative)?;
        if !stage::same(&fresh.root, &self.root)?
            || fresh.parents.len() != self.parents.len()
            || !stage::same(&fresh.file, &self.file)?
            || fresh.stamp != self.stamp
            || file_stamp(&self.file)? != self.stamp
        {
            return Err(std::io::Error::other(
                "Version held path or bytes metadata changed",
            ));
        }
        for (a, b) in fresh.parents.iter().zip(&self.parents) {
            if !stage::same(a, b)? {
                return Err(std::io::Error::other("Version ancestor changed"));
            }
        }
        Ok(())
    }
    pub(crate) fn bounded_bytes(&self, max: u64) -> std::io::Result<Vec<u8>> {
        use std::io::Read;
        let mut f = self.file.try_clone()?;
        let mut out = Vec::new();
        f.by_ref().take(max + 1).read_to_end(&mut out)?;
        if out.len() as u64 > max {
            return Err(std::io::Error::other("Version evidence bound exceeded"));
        }
        Ok(out)
    }
}
