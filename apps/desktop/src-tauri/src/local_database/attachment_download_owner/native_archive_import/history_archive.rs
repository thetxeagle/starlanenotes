//! Current canonical archive metadata custody only. No sentinel page, binary,
//! serving, original producer or physical disposition capability is exposed.
use super::*;
#[derive(Clone,PartialEq,Eq)]struct State{identity:Identity,length:u64,uid:u32,mode:u32,links:u64,ctime:i64,ctime_nsec:i64,mtime:i64,mtime_nsec:i64}
fn state(file:&File)->Result<State,String>{regular(file,1)?;let m=file.metadata().map_err(err)?;Ok(State{identity:identity(file).map_err(err)?,length:m.len(),uid:m.uid(),mode:m.mode(),links:m.nlink(),ctime:m.ctime(),ctime_nsec:m.ctime_nsec(),mtime:m.mtime(),mtime_nsec:m.mtime_nsec()})}
struct Native{assets:Directory,directory:Directory,manifest:Manifest,file:File,file_state:State,raw:String}
pub(in super::super) struct Evidence{selection:vault::SnapshotRequestBinding,root:Directory,database:Directory,main:String,main_id:Identity,physical:crate::local_database::attachment_download_owner::vfs::NativeVfs,native:Option<Native>,portable:Value,requires_format4:bool}
impl Evidence{
 pub(in super::super) fn provenance(&self)->Value{self.portable.clone()}
 pub(in super::super) fn requires_format4(&self)->bool{self.requires_format4}
 pub(in super::super) fn fence(&self)->Result<(),String>{
  self.physical.check()?;self.selection.verify_selection()?;self.root.fence().map_err(err)?;self.database.fence().map_err(err)?;
  if self.database.file_identity(&self.main).map_err(err)?!=Some(self.main_id){return Err("Current canonical history main changed".into())}
  if let Some(n)=&self.native{
   n.assets.fence().map_err(err)?;private_dir(&n.directory)?;
   if identity(&self.root.held).map_err(err)?!=n.manifest.root||identity(&self.database.held).map_err(err)?!=n.manifest.database||self.main_id!=n.manifest.main||identity(&n.assets.held).map_err(err)?!=n.manifest.assets||identity(&n.directory.held).map_err(err)?!=n.manifest.directory{return Err("Current imported history native source changed".into())}
   let named=n.directory.open("manifest.json",libc::O_RDONLY).map_err(err)?;if state(&n.file)?!=n.file_state||state(&named)?!=n.file_state||read_raw(&n.file)?!=n.raw{return Err("Current imported history original manifest changed".into())}
  }
  self.physical.check()
 }
}
fn read_raw(file:&File)->Result<String,String>{let length=file.metadata().map_err(err)?.len();if length>16*1024*1024{return Err("Original history manifest exceeds bound".into())}let mut bytes=vec![0;length as usize];file.read_exact_at(&mut bytes,0).map_err(err)?;String::from_utf8(bytes).map_err(err)}
/// Caller holds the current canonical archive SQL transaction and current actor
/// fence. Original SQL/raw portable labels are checked but never create custody.
pub(super) fn capture(selection:vault::SnapshotRequestBinding,provider:&str,current:&Value,instance:&str,physical:crate::local_database::attachment_download_owner::vfs::NativeVfs,row:&(String,String,String,String,String,String,String))->Result<Evidence,String>{
 context(current,provider)?;selection.verify_selection()?;let(op,fingerprint,row_provider,row_instance,intent,receipt,digest)=row;
 if !safe(op)||fingerprint.len()!=64{return Err("Original history operation/fingerprint missing".into())}let path=selection.expected_database_path()?;
 let(result,binding)=validate_receipt_for(provider,path.clone(),op,fingerprint,instance,(row_provider.clone(),row_instance.clone(),intent.clone(),receipt.clone(),digest.clone()))?;
 let root=Directory::capture(path.parent().and_then(Path::parent).ok_or("Current history source root missing")?).map_err(err)?;let database=root.child(".starlane",false).map_err(err)?;let main=path.file_name().and_then(|s|s.to_str()).ok_or("Current history main name invalid")?.to_string();let main_id=database.file_identity(&main).map_err(err)?.ok_or("Current history main missing")?;
 let mut portable=json!({"format":1,"operation_id":op,"request_fingerprint":fingerprint,"request_json":intent,"receipt_json":receipt,"receipt_digest":digest});
 let proof=&result["native_installation"];
 if proof.is_null(){portable["kind"]=json!("legacy_canonical_metadata");let result=Evidence{selection,root,database,main,main_id,physical,native:None,portable,requires_format4:true};result.fence()?;return Ok(result)}
let raw=proof["manifest_json"].as_str().filter(|s|s.len()<=16*1024*1024).ok_or("Original imported history has no qualified native creator; retain review")?.to_string();
 if proof["format"]!=1||proof["manifest_sha256"]!=sha(raw.as_bytes()){return Err("Original imported history native manifest hash differs".into())}let manifest:Manifest=serde_json::from_str(&raw).map_err(err)?;let expected_id:Identity=serde_json::from_value(proof["manifest_identity"].clone()).map_err(err)?;
 let assets=root.child("assets",false).map_err(err)?;let parent=database.child("archive-imports",false).map_err(err)?;private_dir(&parent)?;let directory=parent.child(op,false).map_err(err)?;private_dir(&directory)?;
 if manifest.format!=1||manifest.operation!=*op||manifest.request_fingerprint!=*fingerprint||manifest.binding!=binding||manifest.canonical_revision!=result["canonical_revision"]||manifest.binaries.len()>4096{return Err("Original imported history manifest/SQL binding differs".into())}
 let file=directory.open("manifest.json",libc::O_RDONLY).map_err(err)?;let file_state=state(&file)?;if file_state.identity!=expected_id||read_raw(&file)?!=raw{return Err("Original imported history actual creator inode/bytes differ".into())}
 let requires_format4=vault::retained_history_requires_format4(&result["imported_provenance"])?;
 let native=Some(Native{assets,directory,manifest,file,file_state,raw});let result=Evidence{selection,root,database,main,main_id,physical,native,portable,requires_format4};result.fence()?;Ok(result)
}

pub(in super::super) struct Reserved{held:Vec<Evidence>,ids:std::collections::BTreeSet<String>}
impl Reserved{
 pub(in super::super) fn contains(&self,operation:&str)->bool{self.ids.contains(operation)}
 pub(in super::super) fn fence(&self)->Result<(),String>{for held in &self.held{held.fence()?}Ok(())}
}
/// Same actual transaction/NativeVfs as original prepare, never pool reentry or
/// a nested canonical lock. Only actual retained identity evidence can block
/// collision; it never constructs original creator or erasure authority.
pub(in super::super) async fn reserve_ids(selection:vault::SnapshotRequestBinding,provider:&str,current:&Value,tx:&mut sqlx::SqliteConnection,physical:crate::local_database::attachment_download_owner::vfs::NativeVfs)->Result<Reserved,String>{
 physical.check()?;let exists:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='native_archive_import_receipts'").fetch_one(&mut *tx).await.map_err(err)?;let mut held=Vec::new();let mut ids=std::collections::BTreeSet::new();
 if exists==1{
  let(count,bytes):(i64,i64)=sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(length(CAST(result_json AS BLOB))),0) FROM native_archive_import_receipts").fetch_one(&mut *tx).await.map_err(err)?;if count>4096||bytes>64*1024*1024{return Err("Retained original history count/budget unavailable".into())}
  let request_bytes:i64=sqlx::query_scalar("SELECT COALESCE(SUM(length(CAST(i.request_json AS BLOB))),0) FROM native_archive_import_intents i JOIN native_archive_import_receipts r ON r.operation_id=i.operation_id AND r.request_fingerprint=i.request_fingerprint").fetch_one(&mut *tx).await.map_err(err)?;if request_bytes>64*1024*1024{return Err("Retained original history request budget exceeded".into())}
  let instance:String=sqlx::query_scalar("SELECT instance_id FROM native_archive_instance WHERE singleton=1").fetch_one(&mut *tx).await.map_err(err)?;let rows:Vec<(String,String,String,String,String,String,String)>=sqlx::query_as("SELECT i.operation_id,i.request_fingerprint,i.provider,i.instance_id,i.request_json,r.result_json,r.result_digest FROM native_archive_import_intents i JOIN native_archive_import_receipts r ON r.operation_id=i.operation_id AND r.request_fingerprint=i.request_fingerprint ORDER BY i.operation_id").fetch_all(&mut *tx).await.map_err(err)?;if rows.len()!=count as usize{return Err("Retained original history has orphan receipts".into())}
  for row in rows{let result:Value=serde_json::from_str(&row.5).map_err(err)?;let mut found=vault::retained_original_disposition_ids(&result["imported_provenance"])?;found.extend(vault::retained_original_local_import_ids(&result["imported_provenance"])?);if found.is_empty(){continue}let proof=capture(selection.clone(),provider,current,&instance,physical.clone(),&row)?;ids.extend(found);if ids.len()>1024{return Err("Retained original identities exceed bound".into())}held.push(proof);}
 }
 let result=Reserved{held,ids};result.fence()?;physical.check()?;Ok(result)
}
#[derive(PartialEq,Eq)]struct DirState{identity:Identity,uid:u32,mode:u32,links:u64,ctime:i64,ctime_nsec:i64,mtime:i64,mtime_nsec:i64}
fn dir_state(dir:&Directory)->Result<DirState,String>{private_dir(dir)?;let m=dir.held.metadata().map_err(err)?;Ok(DirState{identity:identity(&dir.held).map_err(err)?,uid:m.uid(),mode:m.mode(),links:m.nlink(),ctime:m.ctime(),ctime_nsec:m.ctime_nsec(),mtime:m.mtime(),mtime_nsec:m.mtime_nsec()})}
pub(in super::super) struct Namespace{selection:vault::SnapshotRequestBinding,physical:crate::local_database::attachment_download_owner::vfs::NativeVfs,database:Directory,main:String,main_id:Identity,base:Option<Directory>,stamp:Option<DirState>,expected:std::collections::BTreeSet<String>}
fn import_base(database:&Directory)->Result<Option<Directory>,String>{database.fence().map_err(err)?;let name=std::ffi::CString::new("archive-imports").map_err(err)?;let mut stat=std::mem::MaybeUninit::<libc::stat>::uninit();if unsafe{libc::fstatat(database.held.as_raw_fd(),name.as_ptr(),stat.as_mut_ptr(),libc::AT_SYMLINK_NOFOLLOW)}!=0{let e=std::io::Error::last_os_error();if e.raw_os_error()==Some(libc::ENOENT){return Ok(None)}return Err(err(e))}if unsafe{stat.assume_init()}.st_mode&libc::S_IFMT!=libc::S_IFDIR{return Err("Original imported namespace has unsupported node".into())}let dir=database.child("archive-imports",false).map_err(err)?;private_dir(&dir)?;Ok(Some(dir))}
fn import_names(base:&Directory)->Result<std::collections::BTreeSet<String>,String>{private_dir(base)?;let mut names=std::collections::BTreeSet::new();for row in std::fs::read_dir(format!("/proc/self/fd/{}",base.held.as_raw_fd())).map_err(err)?{let name=row.map_err(err)?.file_name().into_string().map_err(|_|"Original import namespace encoding invalid")?;if names.len()>=4096||!safe(&name)||!names.insert(name){return Err("Original imported namespace exceeds bound or is unexplained".into())}}private_dir(base)?;Ok(names)}
impl Namespace{
 pub(in super::super) fn fence(&self)->Result<(),String>{self.physical.check()?;self.selection.verify_selection()?;self.database.fence().map_err(err)?;if self.database.file_identity(&self.main).map_err(err)?!=Some(self.main_id){return Err("Current imported namespace source main changed".into())}if let Some(base)=&self.base{if Some(dir_state(base)?)!=self.stamp||import_names(base)?!=self.expected{return Err("Original imported namespace membership changed".into())}}else if import_base(&self.database)?.is_some(){return Err("Unexplained original import namespace appeared".into())}self.physical.check()}
}
pub(in super::super) fn namespace(selection:vault::SnapshotRequestBinding,rows:&[(String,String,String,String,String,String,String)],physical:crate::local_database::attachment_download_owner::vfs::NativeVfs)->Result<Namespace,String>{
 physical.check()?;selection.verify_selection()?;let path=selection.expected_database_path()?;let database=Directory::capture(path.parent().ok_or("Original import source parent missing")?).map_err(err)?;let main=path.file_name().and_then(|s|s.to_str()).ok_or("Original import source main missing")?.to_string();let main_id=database.file_identity(&main).map_err(err)?.ok_or("Original import source main absent")?;let mut expected=std::collections::BTreeSet::new();for row in rows{let result:Value=serde_json::from_str(&row.5).map_err(err)?;if !result["native_installation"].is_null()&&!expected.insert(row.0.clone()){return Err("Original imported native creator identity repeated".into())}}
 let base=import_base(&database)?;if base.is_none()&&!expected.is_empty(){return Err("Original imported native creator namespace missing".into())}let stamp=base.as_ref().map(dir_state).transpose()?;let result=Namespace{selection,physical,database,main,main_id,base,stamp,expected};result.fence()?;Ok(result)
}
