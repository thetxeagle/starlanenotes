//! Native family creation exclusion only; never identity, SQL or deletion authority.
use std::{fs::File,io,os::fd::AsRawFd,path::Path};
pub(crate) struct Creation(File);
impl Creation {
 pub(crate) fn shared(path:&Path)->io::Result<Self>{Self::acquire(path,false)}
 pub(crate) fn exclusive(path:&Path)->io::Result<Self>{Self::acquire(path,true)}
 fn acquire(path:&Path,exclusive:bool)->io::Result<Self>{
  let f=super::attachment_upload::family_root(path)?;
  // SAFETY: retained no-follow root directory; nonblocking kernel flock.
  if unsafe{libc::flock(f.as_raw_fd(),(if exclusive{libc::LOCK_EX}else{libc::LOCK_SH})|libc::LOCK_NB)}<0{return Err(io::Error::last_os_error())}
  Ok(Self(f))
 }
 pub(crate) fn root(&self)->&File{&self.0}
}
