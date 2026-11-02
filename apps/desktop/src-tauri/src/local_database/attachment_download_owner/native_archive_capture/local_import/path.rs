//! Linux original-path source. O_PATH admission never opens/closes the SQLite
//! main inode as a normal FD; doing so could drop this process's POSIX locks.
use super::*;
use std::{ffi::CString,os::fd::FromRawFd,path::{Path,PathBuf}};
#[derive(Clone,Serialize,Deserialize,Debug,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceStamp{device:u64,inode:u64,uid:u32,mode:u32,links:u64,length:u64,ctime:i64,ctime_nsec:i64,mtime:i64,mtime_nsec:i64}
fn source_stamp(file:&File)->Result<SourceStamp,String>{let m=file.metadata().map_err(err)?;if !m.is_file()||m.len()>MAX{return Err("Bounded regular original source required".into())}Ok(SourceStamp{device:m.dev(),inode:m.ino(),uid:m.uid(),mode:m.mode(),links:m.nlink(),length:m.len(),ctime:m.ctime(),ctime_nsec:m.ctime_nsec(),mtime:m.mtime(),mtime_nsec:m.mtime_nsec()})}
#[derive(Clone,Serialize,Deserialize,Debug,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceProof{schema_version:u32,pub(super) source_path:String,stamp:SourceStamp,pub(super) binary_sha256:String}
impl SourceProof{pub(super) fn validate(&self,length:u64)->Result<(),String>{if self.schema_version!=1||self.source_path.len()>4096||!Path::new(&self.source_path).is_absolute()||self.source_path.contains('\0')||self.stamp.length!=length||self.binary_sha256.len()!=64||!self.binary_sha256.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)){return Err("Exact original native path proof required".into())}Ok(())}}
pub(super) struct Source{parent:Directory,leaf:String,pin:File,file:File,pub(super) proof:SourceProof}
fn path_pin(parent:&Directory,leaf:&str)->Result<File,String>{parent.open(leaf,libc::O_PATH).map_err(err)}
fn reject_database_family(owner:&Owner,id:Identity)->Result<(),String>{
 for leaf in [owner.main.clone(),format!("{}-wal",owner.main),format!("{}-shm",owner.main),format!("{}-journal",owner.main)]{if owner.database.file_identity(&leaf).map_err(err)?==Some(id){return Err("Canonical SQLite database family cannot be imported as an attachment".into())}}Ok(())
}

impl Source{
 pub(super) fn capture(owner:&Owner,path:&str,expected:Option<&SourceProof>,length:u64)->Result<Self,String>{
  owner.fence()?;if path.len()>4096{return Err("Original source path exceeds bound".into())}let path=PathBuf::from(path);let parent=Directory::capture(path.parent().ok_or("Original source parent missing")?).map_err(err)?;let leaf=path.file_name().and_then(|s|s.to_str()).ok_or("Original source name invalid")?.to_owned();
  // O_PATH descriptors do not participate in POSIX record-lock close semantics.
  // Reject even a hardlink alias before ANY ordinary open of this inode.
  let pin=path_pin(&parent,&leaf)?;let id=identity(&pin).map_err(err)?;reject_database_family(owner,id)?;let initial=source_stamp(&pin)?;if initial.length!=length{return Err("Original source file length differs".into())}
  if let Some(proof)=expected{proof.validate(length)?;if proof.source_path!=path.to_string_lossy()||proof.stamp!=initial{return Err("Original source path/inode changed; retain prepared operation".into())}}
  let proc=CString::new(format!("/proc/self/fd/{}",pin.as_raw_fd())).map_err(err)?;let fd=unsafe{libc::open(proc.as_ptr(),libc::O_RDONLY|libc::O_CLOEXEC|libc::O_NONBLOCK)};if fd<0{return Err(std::io::Error::last_os_error().to_string())}let file=unsafe{File::from_raw_fd(fd)};
  if source_stamp(&file)?!=initial{return Err("Original opened source differs".into())}let mut hash=Sha256::new();let mut offset=0;let mut buffer=[0;CHUNK];while offset<length{let n=CHUNK.min((length-offset)as usize);file.read_exact_at(&mut buffer[..n],offset).map_err(err)?;hash.update(&buffer[..n]);offset+=n as u64;}
  let proof=SourceProof{schema_version:1,source_path:path.to_string_lossy().into_owned(),stamp:initial,binary_sha256:format!("{:x}",hash.finalize())};if expected.is_some_and(|e|e!=&proof){return Err("Original source bytes changed; retain operation".into())}let source=Self{parent,leaf,pin,file,proof};source.fence()?;owner.fence()?;Ok(source)
 }
 pub(super) fn fence(&self)->Result<(),String>{self.parent.fence().map_err(err)?;let named=path_pin(&self.parent,&self.leaf)?;if source_stamp(&named)?!=self.proof.stamp||source_stamp(&self.pin)?!=self.proof.stamp||source_stamp(&self.file)?!=self.proof.stamp{return Err("Original source identity or stamp changed; retain staged bytes".into())}Ok(())}
 pub(super) async fn run(self,mut creator:Creator,current:Value)->Result<Value,String>{
  if creator.request.path_source.as_ref()!=Some(&self.proof){return Err("Original path producer proof differs".into())}self.fence()?;
  // Verify every previously acknowledged staged chunk still matches this exact
  // source, then continue at the durable offset. No truncation or reselection.
  let mut offset=0;let mut hash=Sha256::new();let mut buffer=[0;CHUNK];while offset<creator.expected.length{let n=CHUNK.min((creator.expected.length-offset)as usize);self.file.read_exact_at(&mut buffer[..n],offset).map_err(err)?;let mut staged=vec![0;n];creator.payload.read_exact_at(&mut staged,offset).map_err(err)?;if staged!=buffer[..n]{return Err("Original staged source bytes differ".into())}hash.update(&buffer[..n]);offset+=n as u64;}
  while offset<creator.request.byte_length{self.fence()?;let n=CHUNK.min((creator.request.byte_length-offset)as usize);self.file.read_exact_at(&mut buffer[..n],offset).map_err(err)?;hash.update(&buffer[..n]);let index=creator.index;let(next,_)=creator.append_owned(current.clone(),index,buffer[..n].to_vec()).await?;creator=next;offset+=n as u64;self.fence()?;}
  if format!("{:x}",hash.finalize())!=self.proof.binary_sha256{return Err("Original source digest differs; retain staged bytes".into())}self.fence()?;let result=creator.finish(current).await?;self.fence()?;Ok(result)
 }
}
impl Owner{
 pub(super) async fn path_request(self,current:Value,base:Request)->Result<Request,String>{
  base.validate()?;if base.path_source.is_some()||base.original_binding!=self.binding()||current!=self.original{return Err("Exact original path base request required".into())}self.fence()?;
  connection::supervise(async move{let _guard=snapshot_commit_lock().lock().await;let mut db=connection::Database::prepare(self.database.clone(),&self.main).await?;let physical=db.fence_owner();let result=async{let row:Option<String>=sqlx::query_scalar("SELECT request_json FROM native_local_import_intents WHERE operation_id=?1 AND length(CAST(request_json AS BLOB))<=65536").bind(&base.operation_id).fetch_optional(db.connection()).await.map_err(err)?;let found:Request=serde_json::from_str(&row.ok_or("Original path prepare is not durably recorded; retain original request")?).map_err(err)?;let mut compared=found.clone();compared.path_source=None;if serde_json::to_value(compared).map_err(err)?!=serde_json::to_value(base).map_err(err)?||found.path_source.is_none(){return Err("Original persisted path request differs".into())}found.validate()?;self.fence()?;physical.check()?;Ok(found)}.await;db.retire().await?;result}).await
 }
}

pub(super) fn metadata(owner:&Owner,path:&str)->Result<Value,String>{
 owner.fence()?;if path.len()>4096{return Err("Original path exceeds bound".into())}let path=Path::new(path);let parent=Directory::capture(path.parent().ok_or("Original source parent missing")?).map_err(err)?;let leaf=path.file_name().and_then(|s|s.to_str()).ok_or("Original source name invalid")?;let pin=path_pin(&parent,leaf)?;reject_database_family(owner,identity(&pin).map_err(err)?)?;let stamp=source_stamp(&pin)?;let source=Source::capture(owner,path.to_str().ok_or("Original source path encoding invalid")?,None,stamp.length)?;owner.fence()?;Ok(json!({"name":leaf,"mime_type":vault::infer_mime_type(path).unwrap_or_else(||"application/octet-stream".into()),"byte_length":stamp.length,"source_selection":source.proof}))
}
