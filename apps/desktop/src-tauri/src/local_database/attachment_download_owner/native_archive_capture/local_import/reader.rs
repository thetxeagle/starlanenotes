//! Read-only current local asset evidence. Completion JSON alone is insufficient.
use super::*;
pub(crate) struct Reader {owner:Owner,snapshot:Value,proof:Proof,request:Request,seal:Seal,dir:Directory,dir_stamp:DirectoryStamp,assets:Directory,intent:File,seal_file:File,seal_stamp:Stamp,payload:File,raw_receipt:String,chunks:Vec<(File,Stamp,String)>}
impl Reader {
 pub(crate) fn open(owner:Owner,snapshot:&Value,asset:&Value,request_raw:&str,proof_raw:&str,receipt_raw:&str,chunk_rows:&[(i64,String)])->Result<Self,String>{Self::open_purpose(owner,snapshot,asset,request_raw,proof_raw,receipt_raw,chunk_rows,false)}
 fn open_purpose(owner:Owner,snapshot:&Value,asset:&Value,request_raw:&str,proof_raw:&str,receipt_raw:&str,chunk_rows:&[(i64,String)],canonical:bool)->Result<Self,String>{
  if request_raw.len()>65536||proof_raw.len()>65536||receipt_raw.len()>65536{return Err("Local reader metadata budget exceeded".into())}
  let request:Request=serde_json::from_str(request_raw).map_err(err)?;request.validate()?;let proof:Proof=serde_json::from_str(proof_raw).map_err(err)?;let receipt:Value=serde_json::from_str(receipt_raw).map_err(err)?;
  let original_context=&request.original_binding["context"];let archive_context=canonical&&original_context["accountId"]=="local-user"&&original_context["storageMode"]=="local"&&original_context["provider"]==owner.provider;
  if owner.original["accountId"]!="local-user"||(!archive_context&&request.original_binding["context"]!=owner.original)||request.original_binding["root"]!=owner.binding()["root"]||proof.format!=1||proof.request_sha256!=sha(request_raw.as_bytes())||proof.uid!=unsafe{libc::geteuid()}||proof.root!=identity(&owner.root.held).map_err(err)?||proof.database!=owner.database_id||receipt["format"]!=1||receipt["operation_id"]!=request.operation_id||receipt["request_sha256"]!=proof.request_sha256||receipt["request"]!=serde_json::to_value(&request).map_err(err)?||receipt["creator"]!=serde_json::to_value(&proof).map_err(err)?||receipt["canonical_committed"]!=true||receipt["asset"]!=*asset||!snapshot["assets"].as_array().is_some_and(|a|a.iter().any(|a|a==asset))||(!canonical&&!snapshot["documents"].as_array().is_some_and(|a|a.iter().any(|a|a["id"]==request.document_id&&a["space_id"]==request.space_id)))||(canonical&&snapshot["assets"].as_array().map(|a|a.iter().filter(|a|*a==asset).count())!=Some(1)){return Err("Original local SQL/native asset proof differs".into())}
  let dir=owner.database.child("local-import-producers",false).map_err(err)?.child(&request.operation_id,false).map_err(err)?;private(&dir)?;let assets=owner.root.child("assets",false).map_err(err)?.child("local-imports",false).map_err(err)?;private(&assets)?;
  let intent=dir.open("intent.json",libc::O_RDONLY).map_err(err)?;if stamp(&intent)?!=proof.intent||read(&intent,65536)?!=request_raw.as_bytes(){return Err("Original local intent creator changed".into())}
  let seal_file=dir.open("seal.json",libc::O_RDONLY).map_err(err)?;let seal_raw=receipt["seal_json"].as_str().ok_or("Original local seal bytes missing")?;if read(&seal_file,65536)?!=seal_raw.as_bytes(){return Err("Original local seal bytes differ".into())}
  let seal:Seal=serde_json::from_str(seal_raw).map_err(err)?;let seal_stamp:Stamp=serde_json::from_value(receipt["seal_file"].clone()).map_err(err)?;
  if seal.format!=1||seal.operation_id!=request.operation_id||seal.request_sha256!=proof.request_sha256||seal.asset!=*asset||seal.root!=proof.root||seal.database!=proof.database||seal.directory!=proof.directory||seal.asset_directory!=identity(&assets.held).map_err(err)?||identity(&dir.held).map_err(err)?!=proof.directory||stamp(&seal_file)?!=seal_stamp||asset["url"]!=original_asset_url(&assets.path.join(format!("asset-import-{}.bin",request.operation_id)))||asset["id"]!=format!("asset-import-{}",request.operation_id)||asset["vault_path"]!=assets.path.join(format!("asset-import-{}.bin",request.operation_id)).to_string_lossy().as_ref()||!asset["native_local_version_provenance"].is_null()||!asset["server_version_provenance"].is_null()||asset["name"]!=request.name||asset["mime_type"]!=request.mime_type||asset["size"]!=request.byte_length||asset["document_id"]!=request.document_id||asset["space_id"]!=request.space_id||asset["source"]!="tauri"||asset["created_at"]!=proof.created_at{return Err("Original local sealed path/creator/metadata differs".into())}
  let queued=original_queue(&request,&proof,asset);
  if receipt.as_object().map(|o|o.len())!=Some(12)||!receipt["canonical_revision"].as_str().is_some_and(|s|!s.is_empty()&&s.len()<=256)||receipt["queued_operation"]!=queued||snapshot["sync_state"]["queued_operations"].as_array().is_some_and(|rows|rows.iter().any(|row|row["id"]==queued["id"]&&queue_intent(row)!=queue_intent(&queued))){return Err("Original local import queue/receipt differs".into())}
  if chunk_rows.len()>MAX_CHUNKS as usize{return Err("Local chunk evidence exceeds bound".into())}let mut chunks=Vec::new();let mut received=0;
  for(index,(ordinal,raw))in chunk_rows.iter().enumerate(){if *ordinal!=index as i64||raw.len()>65536{return Err("Local chunk proof order/budget differs".into())}let row:Value=serde_json::from_str(raw).map_err(err)?;let raw=row["raw_chunk"].as_str().ok_or("Local chunk raw bytes missing")?.to_string();let proof:Stamp=serde_json::from_value(row["chunk_file"].clone()).map_err(err)?;let chunk:Chunk=serde_json::from_str(&raw).map_err(err)?;if chunk.format!=1||chunk.index!=index as u64||chunk.start!=received||chunk.length==0||chunk.length>CHUNK as u64{return Err("Local chunk original transfer differs".into())}received+=chunk.length;let file=dir.open(&format!("chunk-{index:08}.json"),libc::O_RDONLY).map_err(err)?;if stamp(&file)?!=proof||read(&file,65536)?!=raw.as_bytes(){return Err("Original local chunk creator changed".into())}chunks.push((file,proof,raw));}
  if received!=request.byte_length{return Err("Local complete chunk length differs".into())}
  let dir_stamp:DirectoryStamp=serde_json::from_value(receipt["creator_directory_stamp"].clone()).map_err(err)?;
  if directory_stamp(&dir)?!=dir_stamp{return Err("Original complete creator directory changed".into())}
  let payload=dir.open("payload",libc::O_RDONLY).map_err(err)?;if unsafe{libc::flock(payload.as_raw_fd(),libc::LOCK_SH|libc::LOCK_NB)}!=0{return Err("Original local creator is still active; retain for review".into())}
  let reader=Self{owner,snapshot:snapshot.clone(),proof,request,seal,dir,dir_stamp,assets,intent,seal_file,seal_stamp,payload,raw_receipt:receipt_raw.into(),chunks};reader.fence()?;reader.copy_exact(&mut std::io::sink())?;Ok(reader)
 }
 pub(crate) fn fence(&self)->Result<(),String>{
  self.owner.fence()?;private(&self.dir)?;private(&self.assets)?;if directory_stamp(&self.dir)?!=self.dir_stamp{return Err("Original complete creator directory stamps changed".into())}
  if self.proof.uid!=unsafe{libc::geteuid()}||identity(&self.dir.held).map_err(err)?!=self.proof.directory||stamp(&self.intent)?!=self.proof.intent||read(&self.intent,65536)?!=serde_json::to_vec(&self.request).map_err(err)?||stamp(&self.seal_file)?!=self.seal_stamp||read(&self.seal_file,65536)?!=serde_json::to_vec(&self.seal).map_err(err)?||stamp(&self.payload)?!=self.seal.payload||self.seal.payload.links!=2{return Err("Original local complete creator changed".into())}
  let mut expected_names=std::collections::BTreeSet::from(["intent.json".into(),"payload".into(),"seal.json".into()]);for(index,(file,expected,raw))in self.chunks.iter().enumerate(){expected_names.insert(format!("chunk-{index:08}.json"));let named=self.dir.open(&format!("chunk-{index:08}.json"),libc::O_RDONLY).map_err(err)?;if stamp(file)?!=*expected||stamp(&named)?!=*expected||read(file,65536)?!=raw.as_bytes(){return Err("Original local complete progress changed".into())}}if names(&self.dir)?!=expected_names{return Err("Original local complete family membership differs".into())}
  for file in [self.dir.open("payload",libc::O_RDONLY).map_err(err)?,self.assets.open(&format!("asset-import-{}.bin",self.request.operation_id),libc::O_RDONLY).map_err(err)?]{if stamp(&file)?!=self.seal.payload{return Err("Original local owned alias set differs".into())}}
  Ok(())
 }
 pub(crate) fn copy_exact<W:Write>(&self,output:&mut W)->Result<u64,String>{
  self.fence()?;let mut offset=0;let mut hash=Sha256::new();let mut buffer=[0;CHUNK];while offset<self.seal.payload.length{let n=CHUNK.min((self.seal.payload.length-offset)as usize);self.payload.read_exact_at(&mut buffer[..n],offset).map_err(err)?;hash.update(&buffer[..n]);output.write_all(&buffer[..n]).map_err(err)?;offset+=n as u64;}if format!("{:x}",hash.finalize())!=self.seal.binary_sha256{return Err("Original local import byte digest differs".into())}self.fence()?;Ok(offset)
 }
 pub(crate) fn stream(&self,space:&str,page:&str)->Result<(File,u64),String>{
  if self.request.space_id!=space||self.request.document_id!=page{return Err("Exact original local import page association differs".into())}self.fence()?;let snapshot=crate::local_database::attachment_download_owner::transport::Snapshot::capture(&self.payload,&self.dir.held,self.seal.payload.length,&self.seal.binary_sha256).map_err(err)?;self.fence()?;snapshot.into_stream_file().map_err(err)
 }
 pub(crate) fn image_mime(&self,space:&str,page:&str)->Result<String,String>{if self.request.space_id!=space||self.request.document_id!=page{return Err("Exact original local import page association differs".into())}self.fence()?;Ok(self.request.mime_type.clone())}
 pub(crate) fn acknowledged(&self)->Value{json!({"format":1,"operation_id":self.request.operation_id,"canonical_committed":true,"asset":self.seal.asset,"canonical_snapshot":self.snapshot,"receipt_json":self.raw_receipt})}
 pub(crate) fn provenance(&self)->Value{json!({"format":1,"kind":"original_local_import","operation_id":self.request.operation_id,"receipt_json":self.raw_receipt,"receipt_sha256":sha(self.raw_receipt.as_bytes())})}
 pub(crate) fn byte_length(&self)->u64{self.seal.payload.length}
}
pub(crate) async fn capture(tx:&mut sqlx::Transaction<'_,sqlx::Sqlite>,owner:Owner,snapshot:&Value,asset:&Value)->Result<Reader,String>{capture_purpose(tx,owner,snapshot,asset,false).await}
 async fn capture_purpose(tx:&mut sqlx::Transaction<'_,sqlx::Sqlite>,owner:Owner,snapshot:&Value,asset:&Value,canonical:bool)->Result<Reader,String>{
 let id=asset["id"].as_str().and_then(|id|id.strip_prefix("asset-import-")).filter(|s|uuid(s)).ok_or("Exact local import producer ID required")?;
 let row:Option<(String,String,String,String,String)>=sqlx::query_as("SELECT i.request_sha256,i.request_json,i.proof_json,r.request_sha256,r.receipt_json FROM native_local_import_intents i JOIN native_local_import_receipts r ON r.operation_id=i.operation_id WHERE i.operation_id=?1").bind(id).fetch_optional(&mut **tx).await.map_err(err)?;
 let(a,request,proof,b,receipt)=row.ok_or("Original local receipt unavailable; copied data is not creator authority")?;if a!=b||a!=sha(request.as_bytes()){return Err("Original local receipt/request differs".into())}let chunks:Vec<(i64,String)>=sqlx::query_as("SELECT chunk_index,proof_json FROM native_local_import_chunks WHERE operation_id=?1 ORDER BY chunk_index LIMIT 1601").bind(id).fetch_all(&mut **tx).await.map_err(err)?;if canonical{Reader::open_purpose(owner,snapshot,asset,&request,&proof,&receipt,&chunks,true)}else{Reader::open(owner,snapshot,asset,&request,&proof,&receipt,&chunks)}
}

impl Owner {
 /// Keep original selection, VFS transaction, revision and local actor held
 /// through the consumer. Neither a raw receipt nor an asset URL opens a path.
 pub(crate) async fn with_local_import<T:Send+'static,F:FnOnce(&Reader)->Result<T,String>+Send+'static>(self,current:Value,operation:String,consume:F)->Result<T,String>{
  if current!=self.original||current["accountId"]!="local-user"||!uuid(&operation){return Err("Current original local import context required".into())}self.fence()?;
  connection::supervise(async move{
   let _guard=snapshot_commit_lock().lock().await;self.fence()?;
   let mut db=connection::Database::prepare(self.database.clone(),&self.main).await?;let physical=db.fence_owner();
   let result=async{
    let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;strict_preflight(&mut tx).await?;
    let snapshot=read_snapshot_from_connection(&mut tx,&self.provider).await?.ok_or("Current canonical local import source required")?;
    let asset=snapshot["assets"].as_array().and_then(|a|a.iter().find(|a|a["id"]==format!("asset-import-{operation}"))).ok_or("Exact canonical local import version missing")?;
    let reader=capture(&mut tx,self.clone(),&snapshot,asset).await?;reader.fence()?;physical.check()?;
    let output=consume(&reader)?;reader.fence()?;self.fence()?;physical.check()?;tx.rollback().await.map_err(err)?;Ok(output)
   }.await;
   db.retire().await.map_err(|e|format!("Local import reader retirement failed: {e}"))?;result
  }).await
 }
}

/// Whole-canonical-vault purpose only. It cannot be converted into a preview,
/// interactive stream or original producer/cleanup owner.
pub(in super::super) struct CanonicalReader{reader:Reader}
impl CanonicalReader{
 pub(in super::super) fn fence(&self)->Result<(),String>{self.reader.fence()}
 pub(in super::super) fn copy_exact<W:Write>(&self,output:&mut W)->Result<u64,String>{self.reader.copy_exact(output)}
 pub(in super::super) fn byte_length(&self)->u64{self.reader.byte_length()}
 pub(in super::super) fn provenance(&self)->Value{self.reader.provenance()}
}
pub(in super::super) async fn capture_for_archive(tx:&mut sqlx::Transaction<'_,sqlx::Sqlite>,owner:Owner,snapshot:&Value,asset:&Value)->Result<CanonicalReader,String>{Ok(CanonicalReader{reader:capture_purpose(tx,owner,snapshot,asset,true).await?})}
