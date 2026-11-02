//! Informational designation custody only. Never asset, serving, or unlink authority.
#![cfg(target_os = "linux")]
use std::{collections::BTreeSet,ffi::CString,fs::File,io::{self,Write},os::{fd::{AsRawFd,FromRawFd},unix::fs::{FileExt,MetadataExt}},path::{Component,Path,PathBuf}};
pub(crate) const NAME:&str=".starlane-notes-data-dir";
const BODY:&[u8]=b"Starlane Notes data directory\n";
const MAX_ENTRIES:usize=8192;
const MAX_DEPTH:usize=12;
#[derive(Clone,Debug,PartialEq,Eq)]
struct Stamp{dev:u64,ino:u64,len:u64,links:u64,mode:u32,uid:u32,gid:u32,mtime:i64,mtime_ns:i64,ctime:i64,ctime_ns:i64}
fn invalid(s:&str)->io::Error{io::Error::new(io::ErrorKind::InvalidData,s)}
fn stamp(f:&File)->io::Result<Stamp>{let m=f.metadata()?;Ok(Stamp{dev:m.dev(),ino:m.ino(),len:m.len(),links:m.nlink(),mode:m.mode(),uid:m.uid(),gid:m.gid(),mtime:m.mtime(),mtime_ns:m.mtime_nsec(),ctime:m.ctime(),ctime_ns:m.ctime_nsec()})}
fn open(parent:&File,name:&str,flags:i32)->io::Result<File>{
 if name.is_empty()||name=="."||name==".."||name.contains('/')||name.contains('\\'){return Err(invalid("Unsafe designation component"));}
 let name=CString::new(name).map_err(|_|invalid("NUL designation component"))?;
 // SAFETY: held directory FD and validated single component; O_NOFOLLOW excludes borrowed targets.
 let fd=unsafe{libc::openat(parent.as_raw_fd(),name.as_ptr(),flags|libc::O_NOFOLLOW|libc::O_CLOEXEC,0o600)};
 if fd<0{return Err(io::Error::last_os_error());}
 // SAFETY: ownership of newly opened descriptor transferred exactly once.
 Ok(unsafe{File::from_raw_fd(fd)})
}
fn walk_root(path:&Path,create:bool)->io::Result<(PathBuf,File)>{
 let path=if path.is_absolute(){path.to_owned()}else{std::env::current_dir()?.join(path)};let mut held=File::open("/")?;
 for component in path.components(){match component{Component::RootDir=>(),Component::Normal(n)=>{let n=n.to_str().ok_or_else(||invalid("NonUTF8 designation root"))?;held=match open(&held,n,libc::O_RDONLY|libc::O_DIRECTORY){Ok(f)=>f,Err(e)if create&&e.kind()==io::ErrorKind::NotFound=>{let c=CString::new(n).map_err(|_|invalid("Unsafe root"))?;
 // SAFETY: held parent and validated component; new directory is private.
 if unsafe{libc::mkdirat(held.as_raw_fd(),c.as_ptr(),0o700)}<0{return Err(io::Error::last_os_error());}held.sync_all()?;open(&held,n,libc::O_RDONLY|libc::O_DIRECTORY)?},Err(e)=>return Err(e)}},_=>return Err(invalid("Unsafe designation root path"))}}
 Ok((path,held))
}
fn root(path:&Path,create:bool)->io::Result<File>{
 let (path,held)=walk_root(path,create)?;
 let m=held.metadata()?;
 // SAFETY: geteuid has no pointer arguments or side effects.
 let effective_uid=unsafe{libc::geteuid()};let permissions=m.mode()&0o7777;
 if !m.is_dir(){return Err(invalid(&format!("Designation root {} is not a directory",path.display())));}
 if m.uid()!=effective_uid||permissions&0o077!=0{return Err(invalid(&format!("Designation root {} is borrowed or not private: owner UID {}, mode {:04o}; expected owner UID {} and no group/other permissions (normally mode 0700)",path.display(),m.uid(),permissions,effective_uid)));}Ok(held)
}
fn names(f:&File)->io::Result<BTreeSet<String>>{let mut names=BTreeSet::new();for entry in std::fs::read_dir(format!("/proc/self/fd/{}",f.as_raw_fd()))?{if names.len()>=MAX_ENTRIES{return Err(invalid("Designation inventory exceeds bound"));}let n=entry?.file_name().into_string().map_err(|_|invalid("NonUTF8 designation inventory"))?;if n.contains('/')||n.contains('\\'){return Err(invalid("Unsafe designation inventory"));}names.insert(n);}Ok(names)}
fn validate(root:&File,marker:&File)->io::Result<Stamp>{let before=stamp(marker)?;let owner=root.metadata()?;
 if !marker.metadata()?.is_file()||before.links!=1||before.uid!=owner.uid()||before.gid!=owner.gid()||!matches!(before.mode&0o7777,0o600|0o644)||before.len!=BODY.len() as u64{return Err(invalid("Malformed, aliased, or borrowed DataRoot designation"));}
 let mut bytes=vec![0;BODY.len()];marker.read_exact_at(&mut bytes,0)?;
 if bytes!=BODY||stamp(marker)?!=before{return Err(invalid("Designation bytes or custody changed"));}Ok(before)
}
struct Directory{relative:Vec<String>,file:File,identity:(u64,u64),members:BTreeSet<String>}
fn scan(root:&File,marker:&Stamp)->io::Result<()> {
 fn walk(dir:&File,relative:Vec<String>,marker:&Stamp,count:&mut usize,dirs:&mut Vec<Directory>)->io::Result<()>{
  if relative.len()>MAX_DEPTH{return Err(invalid("Designation alias depth exceeds bound"));}let before=stamp(dir)?;let members=names(dir)?;
  dirs.push(Directory{relative:relative.clone(),file:dir.try_clone()?,identity:(before.dev,before.ino),members:members.clone()});
  for n in members { *count+=1;if *count>MAX_ENTRIES{return Err(invalid("Designation alias inventory exceeds bound"));}let f=open(dir,&n,libc::O_RDONLY|libc::O_NONBLOCK)?;let m=f.metadata()?;let mut path=relative.clone();path.push(n.clone());
   if m.is_dir(){walk(&f,path,marker,count,dirs)?;}else if m.is_file(){let canonical=relative.is_empty()&&n==NAME;
    if m.dev()==marker.dev&&m.ino()==marker.ino {if !canonical||m.nlink()!=1{return Err(invalid("Designation inode has a foreign alias"));}}
    // A designation is metadata at its exact role path, not a content fingerprint.
    // Arbitrary document/upload bytes equal to BODY neither grant nor deny custody.
    else if n==NAME{return Err(invalid("Foreign designation control role outside original root"));}
   }else{return Err(invalid("Opaque or borrowed object in designation alias inventory"));}
  }
  if stamp(dir)?!=before||names(dir)?!=dirs.iter().find(|d|d.relative==relative).unwrap().members{return Err(invalid("Designation inventory changed during scan"));}Ok(())
 }
 let mut dirs=vec![];walk(root,vec![],marker,&mut 0,&mut dirs)?;
 for d in dirs{let mut fresh=root.try_clone()?;for n in &d.relative{fresh=open(&fresh,n,libc::O_RDONLY|libc::O_DIRECTORY)?;}let m=fresh.metadata()?;let held=d.file.metadata()?;if (m.dev(),m.ino())!=d.identity||(held.dev(),held.ino())!=d.identity||names(&fresh)?!=d.members{return Err(invalid("Designation inventory namespace replaced"));}}Ok(())
}
/// Opaque, nonserializable, root-bound observation held by the native family fence.
/// Canonical existing bytes establish current designation shape, never historical creator or asset authority.
pub(crate) struct Designation{path:PathBuf,root:File,root_identity:(u64,u64,u32,u32,u32),marker:File,stamp:Stamp}
impl Designation{
 pub(crate) fn capture_held(path:&Path,held:&File)->io::Result<Option<Self>>{
  let marker=match open(held,NAME,libc::O_RDONLY|libc::O_NONBLOCK){Ok(f)=>f,Err(e)if e.kind()==io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(e)};
  let actual=root(path,false)?;let m=actual.metadata()?;let h=held.metadata()?;if (m.dev(),m.ino())!=(h.dev(),h.ino()){return Err(invalid("Designation does not bind held native root"));}
  let s=validate(held,&marker)?;scan(held,&s)?;let token=Self{path:path.to_owned(),root:held.try_clone()?,root_identity:(m.dev(),m.ino(),m.uid(),m.gid(),m.mode()),marker,stamp:s};token.fence()?;Ok(Some(token))
 }
 pub(crate) fn covers_entry(&self,name:&str)->bool{name==NAME}
 /// Read-only metadata-purpose classification; never an asset ownership exclusion.
 /// A caller must retain this opaque root-bound token throughout its own proof.
 pub(crate) fn matches_held_marker(&self,relative:&str,file:&File)->io::Result<bool>{
  self.fence()?;
  if relative!=NAME{return Ok(false);}
  Ok(stamp(file)?==self.stamp)
 }
 pub(crate) fn fence(&self)->io::Result<()> {let fresh=root(&self.path,false)?;let m=fresh.metadata()?;let held=self.root.metadata()?;if (m.dev(),m.ino(),m.uid(),m.gid(),m.mode())!=self.root_identity||(held.dev(),held.ino(),held.uid(),held.gid(),held.mode())!=self.root_identity{return Err(invalid("Held designation root changed"));}let current=open(&fresh,NAME,libc::O_RDONLY|libc::O_NONBLOCK)?;if validate(&fresh,&current)?!=self.stamp||validate(&self.root,&self.marker)?!=self.stamp{return Err(invalid("Original designation replaced or rename-restored"));}scan(&fresh,&self.stamp)?;if validate(&fresh,&current)?!=self.stamp||validate(&self.root,&self.marker)?!=self.stamp{return Err(invalid("Designation changed across alias census"));}Ok(())}
}
/// Startup only. Never invoked to designate or mutate a stopped backup source.
pub(crate) fn initialize(path:&Path)->io::Result<()> {
 let held=root(path,true)?;
 // SAFETY: retained root FD, same nonblocking exclusive family creation ordering.
 if unsafe{libc::flock(held.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}<0{return Err(io::Error::last_os_error());}
 match open(&held,NAME,libc::O_RDONLY|libc::O_NONBLOCK){Ok(f)=>{let s=validate(&held,&f)?;scan(&held,&s)?;},Err(e)if e.kind()==io::ErrorKind::NotFound=>{let mut marker=open(&held,NAME,libc::O_WRONLY|libc::O_CREAT|libc::O_EXCL)?;marker.write_all(BODY)?;marker.sync_all()?;held.sync_all()?;},Err(e)=>return Err(e)}
 // Required directories are created private independent of process umask.
 // Existing unsafe directories are retained and refused; this never repairs permissions.
 for name in ["spaces","uploads","search_index"] {
  let directory=match open(&held,name,libc::O_RDONLY|libc::O_DIRECTORY){
   Ok(f)=>f,
   Err(e) if e.kind()==io::ErrorKind::NotFound=>{
    let name=CString::new(name).map_err(|_|invalid("Invalid required directory"))?;
    // SAFETY: held no-follow root and fixed single-component directory designation.
    if unsafe{libc::mkdirat(held.as_raw_fd(),name.as_ptr(),0o700)}<0{return Err(io::Error::last_os_error());}
    held.sync_all()?;
    open(&held,name.to_str().map_err(|_|invalid("Invalid required directory"))?,libc::O_RDONLY|libc::O_DIRECTORY)?
   },
   Err(e)=>return Err(e),
  };
  let m=directory.metadata()?;let owner=held.metadata()?;
  if !m.is_dir()||m.uid()!=owner.uid()||m.gid()!=owner.gid()||m.mode()&0o7777!=0o700{return Err(invalid("Existing required DataRoot directory is unsafe; retained without chmod"));}
 }
 Designation::capture_held(path,&held)?.ok_or_else(||invalid("Startup designation absent"))?.fence()
}
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum StartupLayout{Designated,LegacyMarkerless}
/// Startup compatibility only. Marker-bearing roots remain strict; an existing markerless root
/// keeps the legacy behavior and receives no designation, custody, recovery, or deletion authority.
pub(crate) fn initialize_compatible(path:&Path)->io::Result<StartupLayout>{
 let held=match walk_root(path,false){Ok((_,held))=>held,Err(e)if e.kind()==io::ErrorKind::NotFound=>{initialize(path)?;return Ok(StartupLayout::Designated);},Err(e)=>return Err(e)};
 match open(&held,NAME,libc::O_RDONLY|libc::O_NONBLOCK){
  Ok(_)=>{initialize(path)?;Ok(StartupLayout::Designated)},
  Err(e)if e.kind()==io::ErrorKind::NotFound=>{let m=held.metadata()?;
   // SAFETY: geteuid has no pointer arguments or side effects.
   let effective_uid=unsafe{libc::geteuid()};if m.uid()==effective_uid&&m.mode()&0o077==0{initialize(path)?;Ok(StartupLayout::Designated)}else{Ok(StartupLayout::LegacyMarkerless)}
  },
  Err(e)=>Err(e),
 }
}
#[cfg(test)]mod tests{
 use super::*;use std::os::unix::fs::{symlink,PermissionsExt};
 fn fixture()->PathBuf{static NEXT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);let p=std::env::temp_dir().join(format!("designation-{}-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),NEXT.fetch_add(1,std::sync::atomic::Ordering::Relaxed)));{use std::os::unix::fs::DirBuilderExt;std::fs::DirBuilder::new().mode(0o700).create(&p).unwrap();}p}
 fn capture(p:&Path)->Designation{let f=root(p,false).unwrap();Designation::capture_held(p,&f).unwrap().unwrap()}
 #[test]fn fresh_and_restart_preserve_exact_inode(){let p=fixture();initialize(&p).unwrap();let before=std::fs::metadata(p.join(NAME)).unwrap();initialize(&p).unwrap();let after=std::fs::metadata(p.join(NAME)).unwrap();assert_eq!(before.ino(),after.ino());assert_eq!(before.ctime_nsec(),after.ctime_nsec());capture(&p).fence().unwrap();std::fs::remove_dir_all(p).unwrap();}
 #[test]fn legacy644_shape_preserved_without_rewrite(){let p=fixture();std::fs::write(p.join(NAME),BODY).unwrap();std::fs::set_permissions(p.join(NAME),std::fs::Permissions::from_mode(0o644)).unwrap();let token=capture(&p);initialize(&p).unwrap();token.fence().unwrap();std::fs::remove_dir_all(p).unwrap();}
 #[test]fn borrowed_symlink_never_followed_or_truncated(){let p=fixture();let foreign=p.with_extension("foreign");std::fs::write(&foreign,b"KEEP").unwrap();symlink(&foreign,p.join(NAME)).unwrap();assert!(initialize(&p).is_err());assert_eq!(std::fs::read(&foreign).unwrap(),b"KEEP");std::fs::remove_dir_all(p).unwrap();std::fs::remove_file(foreign).unwrap();}
 #[test]fn malformed_and_aliased_marker_refuse(){let p=fixture();std::fs::write(p.join(NAME),b"WRONG").unwrap();assert!(initialize(&p).is_err());assert_eq!(std::fs::read(p.join(NAME)).unwrap(),b"WRONG");std::fs::remove_file(p.join(NAME)).unwrap();initialize(&p).unwrap();std::fs::hard_link(p.join(NAME),p.join("alias")).unwrap();assert!(initialize(&p).is_err());std::fs::remove_dir_all(p).unwrap();}
 #[test]fn held_equal_byte_replacement_refuses(){let p=fixture();initialize(&p).unwrap();let token=capture(&p);std::fs::rename(p.join(NAME),p.join("old")).unwrap();std::fs::write(p.join(NAME),BODY).unwrap();assert!(token.fence().is_err());std::fs::remove_dir_all(p).unwrap();}
 #[test]fn held_rename_back_refuses_ctime_change(){let p=fixture();initialize(&p).unwrap();let token=capture(&p);std::thread::sleep(std::time::Duration::from_millis(2));std::fs::rename(p.join(NAME),p.join("away")).unwrap();std::fs::rename(p.join("away"),p.join(NAME)).unwrap();assert!(token.fence().is_err());std::fs::remove_dir_all(p).unwrap();}
 #[test]fn arbitrary_body_equal_marker_is_not_designation(){let p=fixture();initialize(&p).unwrap();let token=capture(&p);std::fs::create_dir(p.join("nested")).unwrap();std::fs::write(p.join("nested/copy"),BODY).unwrap();token.fence().unwrap();initialize(&p).unwrap();std::fs::remove_dir_all(p).unwrap();}
 #[test]fn metadata_role_requires_exact_held_descriptor(){let p=fixture();initialize(&p).unwrap();let token=capture(&p);let exact=File::open(p.join(NAME)).unwrap();assert!(token.matches_held_marker(NAME,&exact).unwrap());std::fs::write(p.join("payload"),BODY).unwrap();let payload=File::open(p.join("payload")).unwrap();assert!(!token.matches_held_marker(NAME,&payload).unwrap());assert!(!token.matches_held_marker("payload",&exact).unwrap());std::fs::remove_dir_all(p).unwrap();}
 #[test]fn nested_designation_control_role_refuses(){let p=fixture();initialize(&p).unwrap();let token=capture(&p);std::fs::create_dir(p.join("nested")).unwrap();std::fs::write(p.join("nested").join(NAME),BODY).unwrap();assert!(token.fence().is_err());assert!(initialize(&p).is_err());std::fs::remove_dir_all(p).unwrap();}
 #[test]fn readonly_absent_source_stays_absent_and_root_replacement_refuses(){let p=fixture();let held=root(&p,false).unwrap();assert!(Designation::capture_held(&p,&held).unwrap().is_none());assert!(!p.join(NAME).exists());initialize(&p).unwrap();let token=capture(&p);let away=p.with_extension("away");std::fs::rename(&p,&away).unwrap();std::fs::create_dir(&p).unwrap();std::fs::write(p.join(NAME),BODY).unwrap();assert!(token.fence().is_err());std::fs::remove_dir_all(p).unwrap();std::fs::remove_dir_all(away).unwrap();}
 #[test]fn normal_default_required_dirs_are_private(){let p=fixture();initialize(&p).unwrap();for n in ["spaces","uploads","search_index"]{assert_eq!(std::fs::metadata(p.join(n)).unwrap().mode()&0o7777,0o700);}std::fs::remove_dir_all(p).unwrap();}
 #[test]fn existing_unsafe_required_directory_refuses_without_chmod(){let p=fixture();initialize(&p).unwrap();std::fs::set_permissions(p.join("uploads"),std::fs::Permissions::from_mode(0o755)).unwrap();let before=std::fs::metadata(p.join(NAME)).unwrap();assert!(initialize(&p).is_err());assert_eq!(std::fs::metadata(p.join("uploads")).unwrap().mode()&0o7777,0o755);assert_eq!(std::fs::metadata(p.join(NAME)).unwrap().ino(),before.ino());std::fs::remove_dir_all(p).unwrap();}
 #[test]fn absent_marker_never_adds_designation_to_legacy_root(){let p=fixture();std::fs::set_permissions(&p,std::fs::Permissions::from_mode(0o755)).unwrap();assert!(Designation::capture_held(&p,&std::fs::File::open(&p).unwrap()).unwrap().is_none());assert!(!p.join(NAME).exists());let error=initialize(&p).unwrap_err().to_string();assert!(error.contains(&format!("Designation root {} is borrowed or not private",p.display())));assert!(error.contains("mode 0755"));assert!(error.contains("normally mode 0700"));assert_eq!(std::fs::metadata(&p).unwrap().mode()&0o7777,0o755);std::fs::remove_dir_all(p).unwrap();}
 #[test]fn compatible_startup_retains_nonprivate_markerless_root(){let p=fixture();std::fs::set_permissions(&p,std::fs::Permissions::from_mode(0o2755)).unwrap();assert_eq!(initialize_compatible(&p).unwrap(),StartupLayout::LegacyMarkerless);assert!(!p.join(NAME).exists());assert_eq!(std::fs::metadata(&p).unwrap().mode()&0o7777,0o2755);std::fs::remove_dir_all(p).unwrap();}
 #[test]fn compatible_startup_designates_private_markerless_root(){let p=fixture();assert_eq!(initialize_compatible(&p).unwrap(),StartupLayout::Designated);assert_eq!(std::fs::read(p.join(NAME)).unwrap(),BODY);std::fs::remove_dir_all(p).unwrap();}
 #[test]fn compatible_startup_refuses_marker_bearing_nonprivate_root(){let p=fixture();std::fs::write(p.join(NAME),BODY).unwrap();std::fs::set_permissions(&p,std::fs::Permissions::from_mode(0o755)).unwrap();assert!(initialize_compatible(&p).is_err());assert_eq!(std::fs::read(p.join(NAME)).unwrap(),BODY);std::fs::remove_dir_all(p).unwrap();}
 #[test]fn overdepth_inventory_retains_without_guessing(){let p=fixture();initialize(&p).unwrap();let token=capture(&p);let mut deep=p.clone();for _ in 0..13{deep.push("deep");std::fs::create_dir(&deep).unwrap();}assert!(token.fence().is_err());std::fs::remove_dir_all(p).unwrap();}

}
