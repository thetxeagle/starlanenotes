// Whole-source observational inventory only. No directory is admitted as a
// control/admission role by this traversal. Exact role constructors remain0700.
// Derived directories are traversed, never skipped by name or modified.
fn historical_observed_directory(active:&Active,path:&str)->io::Result<File>{
 if !valid_relative(path){return Err(fail("Historical observed relative directory invalid"));}let mut current=active.root.fd().try_clone()?;for part in path.split('/') {current=open_at(&current,part,libc::O_RDONLY|libc::O_DIRECTORY)?;}Ok(current)
}
struct HistoricalSourceInventory {files:Vec<(String,File,PhysicalStamp)>,directories:Vec<(String,File,PhysicalStamp)>}
impl HistoricalSourceInventory {
 fn capture(active:&Active)->io::Result<Self>{
  let mut result=Self{files:Vec::new(),directories:Vec::new()};let main=active.sqlite_main.as_ref().map(|f|identity(f)).transpose()?;result.walk(active,active.root.fd(),"",main,0)?;result.fence(active)?;Ok(result)
 }
 fn walk(&mut self,active:&Active,dir:&File,base:&str,main:Option<Identity>,depth:usize)->io::Result<()> {
  if depth>32||self.files.len()+self.directories.len()>16384{return Err(fail("Historical source complete runtime census exceeds bound"));}
  let held=dir.try_clone()?;let m=held.metadata()?;if m.uid()!=unsafe{libc::geteuid()}||m.mode()&0o022!=0{return Err(fail("Historical observed directory ownership/writability differs"));}let before=physical_stamp(&held)?;let names=prior_capsule_names(&held)?;
  for name in names {if self.files.len()+self.directories.len()>16384{return Err(fail("Historical source runtime entry bound exceeded"));}let path=if base.is_empty(){name.clone()}else{format!("{base}/{name}")};let mut st:libc::stat=unsafe{std::mem::zeroed()};let c=CString::new(name.as_str()).map_err(|_|fail("Historical source census component NUL"))?;if unsafe{libc::fstatat(dir.as_raw_fd(),c.as_ptr(),&mut st,libc::AT_SYMLINK_NOFOLLOW)}<0{return Err(io::Error::last_os_error());}let id=(st.st_dev as u64,st.st_ino as u64);
   if Some(id)==main {if path!="starlane-notes.db"||qualified_sqlite_path_identity(&active.root)?!=id{return Err(fail("Historical source original main aliased or replaced"));}continue;}
   let kind=st.st_mode as u32&libc::S_IFMT;if kind==libc::S_IFDIR {let child=historical_observed_directory(active,&path)?;self.walk(active,&child,&path,main,depth+1)?;}else if kind==libc::S_IFREG {let file=active.root.leaf(&path)?;let m=file.metadata()?;if m.uid()!=unsafe{libc::geteuid()}||m.mode()&0o022!=0{return Err(fail("Historical observed file ownership/writability differs"));}let stamp=physical_stamp(&file)?;if (stamp.device,stamp.inode)!=id{return Err(fail("Historical source census file changed while opening"));}self.files.push((path,file,stamp));}else{return Err(fail("Historical source runtime census special file retains review"));}
  }
  if physical_stamp(&held)?!=before{return Err(fail("Historical source runtime directory changed while scanning"));}self.directories.push((base.into(),held,before));Ok(())
 }
 fn fence(&self,active:&Active)->io::Result<()> {
  active.check()?;let mut groups=std::collections::BTreeMap::<(u64,u64),(u64,u64)>::new();for(path,file,stamp)in &self.files {if physical_stamp(file)?!=*stamp||physical_stamp(&active.root.leaf(path)?)?!=*stamp{return Err(fail("Historical source full runtime file changed"));}let e=groups.entry((stamp.device,stamp.inode)).or_insert((0,stamp.links));e.0+=1;if e.1!=stamp.links{return Err(fail("Historical source full runtime link stamps disagree"));}}
  if groups.values().any(|(count,links)|count!=links){return Err(fail("Historical source runtime payload has unaccounted foreign alias"));}
  for(path,dir,stamp)in &self.directories {let fresh=if path.is_empty(){active.root.fd().try_clone()?}else{historical_observed_directory(active,path)?};if physical_stamp(dir)?!=*stamp||physical_stamp(&fresh)?!=*stamp{return Err(fail("Historical source complete directory topology changed"));}}active.check()
 }
}
