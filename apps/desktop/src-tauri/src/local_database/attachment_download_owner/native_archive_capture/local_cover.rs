//! Distinct original local File creator. Renderer labels and copied receipts
//! are not current native custody. Files never enter the legacy dataURL writer.
use super::*;
use serde::{Deserialize,Serialize};
use std::io::Read;
use std::os::unix::fs::MetadataExt;
const MAX:u64=30*1024*1024;
const CHUNK:usize=65536;
fn sha(bytes:&[u8])->String {format!("{:x}",Sha256::digest(bytes))}
fn uuid(s:&str)->bool{s.len()==36&&s.bytes().enumerate().all(|(i,b)|if [8,13,18,23].contains(&i){b==b'-'}else{b.is_ascii_digit()||(b'a'..=b'f').contains(&b)})}
fn timestamp()->Result<String,String>{let duration=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(err)?;let seconds=duration.as_secs() as libc::time_t;let mut value=std::mem::MaybeUninit::<libc::tm>::uninit();if unsafe{libc::gmtime_r(&seconds,value.as_mut_ptr())}.is_null(){return Err("Original local clock unavailable".into())}let t=unsafe{value.assume_init()};Ok(format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",t.tm_year+1900,t.tm_mon+1,t.tm_mday,t.tm_hour,t.tm_min,t.tm_sec,duration.subsec_millis()))}
fn identifier(s:&str)->bool{!s.is_empty()&&s.len()<=256&&!s.bytes().any(|b|b==0||b==b'/'||b==b'\\')}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {pub operation_id:String,pub name:String,pub mime_type:String,pub byte_length:u64,pub space_id:String,pub document_id:String,pub draft_id:String,pub original_binding:Value}
impl Request {fn validate(&self)->Result<(),String>{if !uuid(&self.operation_id)||self.name.trim().is_empty()||self.name.len()>255||self.byte_length==0||self.byte_length>MAX||!["image/png","image/jpeg","image/gif","image/webp"].contains(&self.mime_type.as_str())||![&self.space_id,&self.document_id,&self.draft_id].into_iter().all(|s|identifier(s)){return Err("Bounded original local cover request required".into())}Ok(())}}
#[derive(Clone,Serialize,Deserialize,Debug,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
struct Stamp{device:u64,inode:u64,uid:u32,mode:u32,links:u64,length:u64,ctime:i64,ctime_nsec:i64,mtime:i64,mtime_nsec:i64}
fn stamp(file:&File)->Result<Stamp,String>{let m=file.metadata().map_err(err)?;if !m.is_file()||m.uid()!=unsafe{libc::geteuid()}||m.mode()&0o777!=0o600{return Err("Original private local file owner/mode differs".into())}Ok(Stamp{device:m.dev(),inode:m.ino(),uid:m.uid(),mode:m.mode(),links:m.nlink(),length:m.len(),ctime:m.ctime(),ctime_nsec:m.ctime_nsec(),mtime:m.mtime(),mtime_nsec:m.mtime_nsec()})}
#[derive(Clone,Serialize,Deserialize,Debug,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
struct DirectoryStamp{device:u64,inode:u64,uid:u32,mode:u32,links:u64,ctime:i64,ctime_nsec:i64,mtime:i64,mtime_nsec:i64}
fn directory_stamp(dir:&Directory)->Result<DirectoryStamp,String>{private(dir)?;let m=dir.held.metadata().map_err(err)?;Ok(DirectoryStamp{device:m.dev(),inode:m.ino(),uid:m.uid(),mode:m.mode(),links:m.nlink(),ctime:m.ctime(),ctime_nsec:m.ctime_nsec(),mtime:m.mtime(),mtime_nsec:m.mtime_nsec()})}
fn names(dir:&Directory)->Result<std::collections::BTreeSet<String>,String>{dir.fence().map_err(err)?;let mut result=std::collections::BTreeSet::new();for entry in std::fs::read_dir(format!("/proc/self/fd/{}",dir.held.as_raw_fd())).map_err(err)?{let entry=entry.map_err(err)?;let name=entry.file_name().into_string().map_err(|_|"Native creator name encoding invalid")?;if result.len()>=515||!result.insert(name){return Err("Native creator membership exceeds bound".into())}}dir.fence().map_err(err)?;Ok(result)}
fn private(dir:&Directory)->Result<(),String>{dir.fence().map_err(err)?;let m=dir.held.metadata().map_err(err)?;if m.uid()!=unsafe{libc::geteuid()}||m.mode()&0o777!=0o700{return Err("Private original local creator directory required".into())}Ok(())}
fn read(file:&File,limit:usize)->Result<Vec<u8>,String>{let len=file.metadata().map_err(err)?.len();if len>limit as u64{return Err("Local creator metadata exceeds bound".into())}let mut out=vec![0;len as usize];file.read_exact_at(&mut out,0).map_err(err)?;Ok(out)}
fn durable(dir:&Directory,name:&str,bytes:&[u8])->Result<(File,Stamp),String>{let file=dir.open(name,libc::O_RDWR|libc::O_CREAT|libc::O_EXCL).map_err(err)?;file.write_all_at(bytes,0).map_err(err)?;file.sync_all().map_err(err)?;dir.held.sync_all().map_err(err)?;let s=stamp(&file)?;if s.links!=1{return Err("New local creator file has aliases".into())}Ok((file,s))}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {format:u32,request_sha256:String,root:Identity,database:Identity,directory:Identity,intent:Stamp,payload_identity:Identity,initial_payload_stamp:Stamp,uid:u32,original_draft_sha256:String,created_at:String}
#[derive(Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Chunk {format:u32,index:u64,start:u64,length:u64,sha256:String,stamp:Stamp}
#[derive(Clone,Serialize,Deserialize)]
#[serde(deny_unknown_fields)]
struct Seal {format:u32,operation_id:String,request_sha256:String,binary_sha256:String,asset:Value,payload:Stamp,root:Identity,database:Identity,directory:Identity,asset_directory:Identity}
/// Owned exclusive descriptor lives across native chunks. No arbitrary path input.
pub(crate) struct Creator {owner:Owner,request:Request,proof:Proof,dir:Directory,assets:Directory,intent:File,payload:File,expected:Stamp,index:u64,seal:Option<(Seal,File,Stamp)>}
impl Creator {
 fn fence(&self,current:&Value)->Result<(),String>{
  if *current!=self.owner.original||current["accountId"]!="local-user"||self.proof.uid!=unsafe{libc::geteuid()}{return Err("Original native local cover actor/context changed".into())}
  self.owner.fence()?;private(&self.dir)?;private(&self.assets)?;
  if identity(&self.owner.root.held).map_err(err)?!=self.proof.root||self.owner.database_id!=self.proof.database||identity(&self.dir.held).map_err(err)?!=self.proof.directory||stamp(&self.intent)?!=self.proof.intent||read(&self.intent,65536)?!=serde_json::to_vec(&self.request).map_err(err)?||identity(&self.payload).map_err(err)?!=self.proof.payload_identity||stamp(&self.payload)?!=self.expected{return Err("Original local creator identity/stamps changed; retain evidence".into())}
  let mut expected_names=std::collections::BTreeSet::from(["intent.json".into(),"payload".into()]);for index in 0..self.index{expected_names.insert(format!("chunk-{index:08}.json"));}if self.seal.is_some(){expected_names.insert("seal.json".into());}if names(&self.dir)?!=expected_names{return Err("Original local producer family membership changed".into())}
  let named=self.dir.open("payload",libc::O_RDONLY).map_err(err)?;if stamp(&named)?!=self.expected{return Err("Original local payload name changed".into())}
  if let Some((seal,file,s))=&self.seal {if stamp(file)?!=*s||read(file,65536)?!=serde_json::to_vec(seal).map_err(err)?{return Err("Original local seal changed".into())}let installed=self.assets.open(&format!("native-cover-{}.bin",self.request.operation_id),libc::O_RDONLY).map_err(err)?;if stamp(&installed)?!=self.expected{return Err("Original installed local cover differs".into())}}
  Ok(())
 }
 fn initial(owner:Owner,request:Request,draft:Value)->Result<Self,String>{
  request.validate()?;if owner.root.held.metadata().map_err(err)?.uid()!=unsafe{libc::geteuid()}||owner.database.held.metadata().map_err(err)?.uid()!=unsafe{libc::geteuid()}{return Err("Original native local OS owner differs".into())}if request.original_binding!=owner.binding()||owner.original["accountId"]!="local-user"{return Err("Actual original local cover binding required".into())}owner.fence()?;
  let base=owner.database.child("local-cover-producers",true).map_err(err)?;private(&base)?;
  // Existing operation is never truncated or silently borrowed.
  if base.child(&request.operation_id,false).is_ok(){return Err("Original local cover operation exists; inspect its durable receipt".into())}
  let dir=base.child(&request.operation_id,true).map_err(err)?;private(&dir)?;let assets=owner.root.child("assets",true).map_err(err)?.child("local-covers",true).map_err(err)?;private(&assets)?;
  let raw=serde_json::to_vec(&request).map_err(err)?;if raw.len()>65536{return Err("Local request exceeds bound".into())}
  let(intent,intent_stamp)=durable(&dir,"intent.json",&raw)?;let(payload,expected)=durable(&dir,"payload",&[])?;
  if unsafe{libc::flock(payload.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Err("Original local writer lease unavailable".into())}
  let proof=Proof{format:1,request_sha256:sha(&raw),root:identity(&owner.root.held).map_err(err)?,database:owner.database_id,directory:identity(&dir.held).map_err(err)?,intent:intent_stamp,payload_identity:identity(&payload).map_err(err)?,initial_payload_stamp:expected.clone(),uid:unsafe{libc::geteuid()},original_draft_sha256:sha(&serde_json::to_vec(&draft).map_err(err)?),created_at:timestamp()?};
  let creator=Self{owner,request,proof,dir,assets,intent,payload,expected,index:0,seal:None};creator.fence(&creator.owner.original)?;Ok(creator)
 }
 pub(crate) fn append(&mut self,current:&Value,index:u64,bytes:&[u8])->Result<Value,String>{
  self.fence(current)?;if self.seal.is_some()||self.index>=512||bytes.is_empty()||bytes.len()>CHUNK||index!=self.index||self.expected.length+bytes.len() as u64>self.request.byte_length{return Err("Exact sequential original local cover chunk required".into())}
  let start=self.expected.length;self.payload.write_all_at(bytes,start).map_err(err)?;self.payload.sync_all().map_err(err)?;
  let next=stamp(&self.payload)?;let progress=Chunk{format:1,index,start,length:bytes.len() as u64,sha256:sha(bytes),stamp:next.clone()};
  // A crash after payload sync but before progress remains retained review;
  // never truncate unknown bytes to manufacture a successful retry.
  durable(&self.dir,&format!("chunk-{index:08}.json"),&serde_json::to_vec(&progress).map_err(err)?)?;
  self.expected=next;self.index+=1;self.fence(current)?;Ok(json!({"format":1,"index":index,"received":self.expected.length,"operation_id":self.request.operation_id}))
 }
 fn seal(&mut self,current:&Value)->Result<Seal,String>{
  self.fence(current)?;if let Some((seal,_,_))=&self.seal{return Ok(seal.clone())}if self.expected.length!=self.request.byte_length{return Err("Original local cover is incomplete".into())}
  let mut hash=Sha256::new();let mut buffer=[0u8;CHUNK];let mut offset=0;while offset<self.expected.length{let n=self.payload.read_at(&mut buffer[..CHUNK.min((self.expected.length-offset)as usize)],offset).map_err(err)?;if n==0{return Err("Original local cover truncated".into())}hash.update(&buffer[..n]);offset+=n as u64;}
  self.fence(current)?;let leaf=format!("native-cover-{}.bin",self.request.operation_id);let source=std::ffi::CString::new("payload").unwrap();let destination=std::ffi::CString::new(leaf.clone()).unwrap();
  if unsafe{libc::linkat(self.dir.held.as_raw_fd(),source.as_ptr(),self.assets.held.as_raw_fd(),destination.as_ptr(),0)}!=0{return Err("Original local cover install refused; retain stage".into())}self.assets.held.sync_all().map_err(err)?;self.payload.sync_all().map_err(err)?;
  self.expected=stamp(&self.payload)?;if self.expected.links!=2{return Err("Installed local cover alias set differs".into())}let digest=format!("{:x}",hash.finalize());let pin=format!("starlane-local-attachment://{}",self.request.operation_id);
  let asset=json!({"id":format!("native-cover-{}",self.request.operation_id),"name":self.request.name,"mime_type":self.request.mime_type,"size":self.request.byte_length,"url":pin,"vault_path":self.assets.path.join(&leaf),"source":"tauri","space_id":self.request.space_id,"document_id":self.request.document_id,"created_at":self.proof.created_at,"native_local_version_provenance":{"format":1,"version_id":self.request.operation_id,"producer_operation_id":self.request.operation_id,"byte_length":self.request.byte_length,"binary_sha256":digest,"original_filename":self.request.name,"mime_type":self.request.mime_type,"uploader":"local-user"}});
  let seal=Seal{format:1,operation_id:self.request.operation_id.clone(),request_sha256:self.proof.request_sha256.clone(),binary_sha256:digest,asset,payload:self.expected.clone(),root:self.proof.root,database:self.proof.database,directory:self.proof.directory,asset_directory:identity(&self.assets.held).map_err(err)?};let(file,s)=durable(&self.dir,"seal.json",&serde_json::to_vec(&seal).map_err(err)?)?;self.seal=Some((seal.clone(),file,s));self.fence(current)?;Ok(seal)
 }
}
fn target(snapshot:&Value,request:&Request)->Result<Value,String>{
 let page=snapshot["documents"].as_array().and_then(|a|a.iter().find(|r|r["id"]==request.document_id&&r["space_id"]==request.space_id)).ok_or("Original local cover page unavailable")?;
 if page["deleted_at"].as_str().is_some_and(|s|!s.is_empty())||page["archived_at"].as_str().is_some_and(|s|!s.is_empty())||page["locked"]==true{return Err("Original local cover page unavailable".into())}
 let draft=snapshot["drafts"].as_array().and_then(|a|a.iter().find(|r|r["id"]==request.draft_id&&r["document_id"]==request.document_id&&r["space_id"]==request.space_id)).ok_or("Original local cover draft unavailable")?;
 if draft["owner_id"]!="local-user"{return Err("Original local cover draft owner differs".into())}Ok(draft.clone())
}
async fn schema(tx:&mut sqlx::Transaction<'_,sqlx::Sqlite>)->Result<(),String>{
 for sql in ["CREATE TABLE IF NOT EXISTS native_local_cover_chunks(operation_id TEXT NOT NULL,chunk_index INTEGER NOT NULL,proof_json TEXT NOT NULL,PRIMARY KEY(operation_id,chunk_index))","CREATE TRIGGER IF NOT EXISTS native_local_cover_chunks_no_update BEFORE UPDATE ON native_local_cover_chunks BEGIN SELECT RAISE(ABORT,'Local cover chunks immutable'); END","CREATE TRIGGER IF NOT EXISTS native_local_cover_chunks_no_delete BEFORE DELETE ON native_local_cover_chunks BEGIN SELECT RAISE(ABORT,'Local cover chunks immutable'); END","CREATE TABLE IF NOT EXISTS native_local_cover_intents(operation_id TEXT PRIMARY KEY,request_sha256 TEXT NOT NULL,request_json TEXT NOT NULL,proof_json TEXT NOT NULL)","CREATE TABLE IF NOT EXISTS native_local_cover_receipts(operation_id TEXT PRIMARY KEY,request_sha256 TEXT NOT NULL,receipt_json TEXT NOT NULL)","CREATE TRIGGER IF NOT EXISTS native_local_cover_intents_no_update BEFORE UPDATE ON native_local_cover_intents BEGIN SELECT RAISE(ABORT,'Local cover intents immutable'); END","CREATE TRIGGER IF NOT EXISTS native_local_cover_intents_no_delete BEFORE DELETE ON native_local_cover_intents BEGIN SELECT RAISE(ABORT,'Local cover intents immutable'); END","CREATE TRIGGER IF NOT EXISTS native_local_cover_receipts_no_update BEFORE UPDATE ON native_local_cover_receipts BEGIN SELECT RAISE(ABORT,'Local cover receipts immutable'); END","CREATE TRIGGER IF NOT EXISTS native_local_cover_receipts_no_delete BEFORE DELETE ON native_local_cover_receipts BEGIN SELECT RAISE(ABORT,'Local cover receipts immutable'); END"]{sqlx::query(sql).execute(&mut **tx).await.map_err(err)?;}Ok(())
}
impl Owner {
 pub(crate) async fn prepare_local_cover(self,current:Value,request:Request)->Result<Creator,String>{
  request.validate()?;if current!=self.original||request.original_binding!=self.binding(){return Err("Original local cover context/source changed".into())}self.fence()?;
  connection::supervise(async move{
   let _guard=snapshot_commit_lock().lock().await;self.fence()?;let mut db=connection::Database::prepare(self.database.clone(),&self.main).await?;let physical=db.fence_owner();
   let result=async{
    let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;strict_preflight(&mut tx).await?;let snapshot=read_snapshot_from_connection(&mut tx,&self.provider).await?.ok_or("Original canonical local snapshot required")?;
    if snapshot["canonical_revision"]!=self.revision{return Err("Original canonical local cover revision changed".into())}let draft=target(&snapshot,&request)?;
    let retained=crate::local_database::attachment_download_owner::native_archive_import::history_archive::reserve_ids(self.selection.clone(),&self.provider,&self.original,&mut tx,physical.clone()).await?;if retained.contains(&request.operation_id){return Err("Original operation identity is retained as discarded provenance; choose a distinct new operation".into())}
    schema(&mut tx).await?;
    let(count,total):(i64,i64)=sqlx::query_as("SELECT count(*),COALESCE(SUM(length(CAST(request_json AS BLOB))+length(CAST(proof_json AS BLOB))),0) FROM native_local_cover_intents").fetch_one(&mut *tx).await.map_err(err)?;if count>=1024||total>=8*1024*1024{return Err("Retained local cover attempts exceed bound; review them".into())}
    let creator=Creator::initial(self.clone(),request,draft)?;
    sqlx::query("INSERT INTO native_local_cover_intents(operation_id,request_sha256,request_json,proof_json) VALUES(?1,?2,?3,?4)").bind(&creator.request.operation_id).bind(&creator.proof.request_sha256).bind(serde_json::to_string(&creator.request).map_err(err)?).bind(serde_json::to_string(&creator.proof).map_err(err)?).execute(&mut *tx).await.map_err(err)?;
    creator.fence(&current)?;retained.fence()?;physical.check()?;tx.commit().await.map_err(err)?;retained.fence()?;Ok(creator)
   }.await;db.retire().await.map_err(|e|format!("Local cover prepare retirement failed: {e}"))?;result
  }).await
 }
}

impl Creator {
 pub(crate) async fn append_owned(mut self,current:Value,index:u64,bytes:Vec<u8>)->Result<(Self,Value),String>{
  let ack=self.append(&current,index,&bytes)?;
  let file=self.dir.open(&format!("chunk-{index:08}.json"),libc::O_RDONLY).map_err(err)?;let raw=String::from_utf8(read(&file,65536)?).map_err(err)?;
  let evidence=json!({"raw_chunk":raw,"chunk_file":stamp(&file)?});
  let owner=self.owner.clone();
  connection::supervise(async move{
   let _guard=snapshot_commit_lock().lock().await;owner.fence()?;let mut db=connection::Database::prepare(owner.database.clone(),&owner.main).await?;let physical=db.fence_owner();
   let result=async{
    let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;strict_preflight(&mut tx).await?;
    let snapshot=read_snapshot_from_connection(&mut tx,&owner.provider).await?.ok_or("Original local cover snapshot unavailable")?;
    if snapshot["canonical_revision"]!=owner.revision||sha(&serde_json::to_vec(&target(&snapshot,&self.request)?).map_err(err)?)!=self.proof.original_draft_sha256{return Err("Original local cover source changed during chunk; retain bytes".into())}
    let previous:i64=sqlx::query_scalar("SELECT count(*) FROM native_local_cover_chunks WHERE operation_id=?1").bind(&self.request.operation_id).fetch_one(&mut *tx).await.map_err(err)?;if previous!=index as i64{return Err("Original local progress is unresolved; retain evidence".into())}
    sqlx::query("INSERT INTO native_local_cover_chunks(operation_id,chunk_index,proof_json) VALUES(?1,?2,?3)").bind(&self.request.operation_id).bind(index as i64).bind(serde_json::to_string(&evidence).map_err(err)?).execute(&mut *tx).await.map_err(err)?;
    self.fence(&current)?;physical.check()?;tx.commit().await.map_err(err)?;Ok((self,ack))
   }.await;db.retire().await.map_err(|e|format!("Local chunk physical retirement failed: {e}"))?;result
  }).await
 }
 pub(crate) async fn finish(mut self,current:Value)->Result<Value,String>{
  let sealed=self.seal(&current)?;let owner=self.owner.clone();
  connection::supervise(async move{
   let _guard=snapshot_commit_lock().lock().await;owner.fence()?;let mut db=connection::Database::prepare(owner.database.clone(),&owner.main).await?;let physical=db.fence_owner();
   let result=async{
    let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;strict_preflight(&mut tx).await?;
    let mut snapshot=read_snapshot_from_connection(&mut tx,&owner.provider).await?.ok_or("Original local cover snapshot unavailable")?;
    if snapshot["canonical_revision"]!=owner.revision||sha(&serde_json::to_vec(&target(&snapshot,&self.request)?).map_err(err)?)!=self.proof.original_draft_sha256{return Err("Original local cover draft/revision changed; retain staged version".into())}
    let intent:Option<(String,String,String)>=sqlx::query_as("SELECT request_sha256,request_json,proof_json FROM native_local_cover_intents WHERE operation_id=?1").bind(&self.request.operation_id).fetch_optional(&mut *tx).await.map_err(err)?;
    if intent!=Some((self.proof.request_sha256.clone(),serde_json::to_string(&self.request).map_err(err)?,serde_json::to_string(&self.proof).map_err(err)?)){return Err("Original local intent SQL/native binding differs".into())}
    let chunks:Vec<(i64,String)>=sqlx::query_as("SELECT chunk_index,proof_json FROM native_local_cover_chunks WHERE operation_id=?1 ORDER BY chunk_index").bind(&self.request.operation_id).fetch_all(&mut *tx).await.map_err(err)?;
    if chunks.len()!=self.index as usize||chunks.len()>512{return Err("Original local chunk proof closure differs".into())}
    for(index,(ordinal,raw))in chunks.iter().enumerate(){if *ordinal!=index as i64{return Err("Original local chunk order differs".into())}let evidence:Value=serde_json::from_str(raw).map_err(err)?;let file=self.dir.open(&format!("chunk-{index:08}.json"),libc::O_RDONLY).map_err(err)?;if serde_json::to_value(stamp(&file)?).map_err(err)?!=evidence["chunk_file"]||read(&file,65536)?!=evidence["raw_chunk"].as_str().ok_or("Original local chunk bytes missing")?.as_bytes(){return Err("Original local progress creator changed".into())}}
    let assets=snapshot["assets"].as_array_mut().ok_or("Canonical native assets unavailable")?;if assets.len()>=4096||assets.iter().any(|a|a["id"]==sealed.asset["id"]){return Err("Original local version identity collision".into())}assets.push(sealed.asset.clone());
    // Canonical asset and immutable winner receipt share the actual VFS transaction.
    let revision=crate::local_database::write_snapshot_in_tx(&mut tx,&owner.provider,&snapshot).await?;
    let (_,seal_file,seal_stamp)=self.seal.as_ref().ok_or("Original seal missing")?;
    let receipt=json!({"format":1,"operation_id":self.request.operation_id,"request_sha256":self.proof.request_sha256,"request":self.request,"creator":self.proof,"seal_json":String::from_utf8(read(seal_file,65536)?).map_err(err)?,"seal_file":seal_stamp,"creator_directory_stamp":directory_stamp(&self.dir)?,"canonical_revision":revision,"canonical_committed":true,"asset":sealed.asset});
    let raw=serde_json::to_string(&receipt).map_err(err)?;if raw.len()>65536{return Err("Local cover receipt exceeds bound".into())}
    sqlx::query("INSERT INTO native_local_cover_receipts(operation_id,request_sha256,receipt_json) VALUES(?1,?2,?3)").bind(&self.request.operation_id).bind(&self.proof.request_sha256).bind(&raw).execute(&mut *tx).await.map_err(err)?;
    self.fence(&current)?;physical.check()?;tx.commit().await.map_err(err)?;
    // No generic legacy mirror writes here. Its truthful pending state remains derived.
    snapshot["canonical_revision"]=json!(revision);snapshot["mirror_pending"]=json!(true);
    if unsafe{libc::flock(self.payload.as_raw_fd(),libc::LOCK_UN)}!=0{return Err("Committed local cover lease release unknown; inspect original receipt".into())}
    Ok(json!({"format":1,"operation_id":self.request.operation_id,"canonical_committed":true,"asset":sealed.asset,"canonical_snapshot":snapshot,"receipt_json":raw}))
   }.await;db.retire().await.map_err(|e|format!("Local cover winner retirement failed: {e}"))?;result
  }).await
 }
}

pub(crate) mod reader;
pub(crate) mod transport;
pub(crate) mod image;
mod resume;
#[cfg(test)]mod tests;

mod discard;

pub(super) use discard::archive::{capture as capture_disposition_archive,Roster as ArchiveDispositions};
