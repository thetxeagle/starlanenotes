//! Resume only the original, SQL-acknowledged native progress after lock loss.
//! Partial filesystem/SQL disagreement is retained review, never truncated.
use super::*;
impl Owner {
 pub(crate) async fn resume_local_import(self,current:Value,request:Request)->Result<Creator,String>{
  request.validate()?;if current!=self.original||current["accountId"]!="local-user"||request.original_binding!=self.binding(){return Err("Original local import source/session changed".into())}self.fence()?;
  connection::supervise(async move{
   let _guard=snapshot_commit_lock().lock().await;self.fence()?;let mut db=connection::Database::prepare(self.database.clone(),&self.main).await?;let physical=db.fence_owner();
   let result=async{
    let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;strict_preflight(&mut tx).await?;
    let snapshot=read_snapshot_from_connection(&mut tx,&self.provider).await?.ok_or("Original local import source unavailable")?;
    let row:Option<(String,String,String)>=sqlx::query_as("SELECT request_sha256,request_json,proof_json FROM native_local_import_intents WHERE operation_id=?1").bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(err)?;
    let(hash,raw,proof_raw)=row.ok_or("Original local import intent missing")?;if raw.len()>65536||proof_raw.len()>65536||raw!=serde_json::to_string(&request).map_err(err)?||hash!=sha(raw.as_bytes()){return Err("Exact original local import request differs".into())}
    let complete:i64=sqlx::query_scalar("SELECT count(*) FROM native_local_import_receipts WHERE operation_id=?1").bind(&request.operation_id).fetch_one(&mut *tx).await.map_err(err)?;if complete!=0{return Err("Original local import already committed; inspect its receipt".into())}
    let proof:Proof=serde_json::from_str(&proof_raw).map_err(err)?;
    if proof.format!=1||proof.request_sha256!=hash||proof.uid!=unsafe{libc::geteuid()}||proof.root!=identity(&self.root.held).map_err(err)?||proof.database!=self.database_id||proof.original_draft_sha256!=sha(&serde_json::to_vec(&incarnation(&snapshot,&request)?).map_err(err)?){return Err("Original local import creator/source differs".into())}
    let dir=self.database.child("local-import-producers",false).map_err(err)?.child(&request.operation_id,false).map_err(err)?;private(&dir)?;let assets=self.root.child("assets",false).map_err(err)?.child("local-imports",false).map_err(err)?;private(&assets)?;
    if identity(&dir.held).map_err(err)?!=proof.directory{return Err("Original local import creator directory differs".into())}
    let intent=dir.open("intent.json",libc::O_RDONLY).map_err(err)?;if stamp(&intent)?!=proof.intent||read(&intent,65536)?!=raw.as_bytes(){return Err("Original local import intent changed".into())}
    let payload=dir.open("payload",libc::O_RDWR).map_err(err)?;if unsafe{libc::flock(payload.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Err("Original native creator remains active; retain for review".into())}
    let mut expected=stamp(&payload)?;if identity(&payload).map_err(err)?!=proof.payload_identity||!matches!(expected.links,1|2){return Err("Original local import payload identity/aliases differ".into())}
    let rows:Vec<(i64,String)>=sqlx::query_as("SELECT chunk_index,proof_json FROM native_local_import_chunks WHERE operation_id=?1 ORDER BY chunk_index LIMIT 1601").bind(&request.operation_id).fetch_all(&mut *tx).await.map_err(err)?;if rows.len()>MAX_CHUNKS as usize{return Err("Original local import progress exceeds bound".into())}
    let mut received=0;let mut last=None;
    for(index,(ordinal,raw))in rows.iter().enumerate(){if *ordinal!=index as i64||raw.len()>65536{return Err("Original local import progress order/budget differs".into())}let row:Value=serde_json::from_str(raw).map_err(err)?;let raw=row["raw_chunk"].as_str().ok_or("Original chunk proof missing")?;let chunk:Chunk=serde_json::from_str(raw).map_err(err)?;let file=dir.open(&format!("chunk-{index:08}.json"),libc::O_RDONLY).map_err(err)?;if serde_json::to_value(stamp(&file)?).map_err(err)?!=row["chunk_file"]||read(&file,65536)?!=raw.as_bytes()||chunk.format!=1||chunk.index!=index as u64||chunk.start!=received||chunk.length==0||chunk.length>CHUNK as u64{return Err("Original native chunk proof differs".into())}
     let mut bytes=vec![0;chunk.length as usize];payload.read_exact_at(&mut bytes,received).map_err(err)?;if sha(&bytes)!=chunk.sha256{return Err("Original native acknowledged chunk bytes differ".into())}received+=chunk.length;last=Some(chunk.stamp);
    }
    let sealed=if names(&dir)?.contains("seal.json"){
      let file=dir.open("seal.json",libc::O_RDONLY).map_err(err)?;let raw=read(&file,65536)?;let seal:Seal=serde_json::from_slice(&raw).map_err(err)?;let seal_stamp=stamp(&file)?;
      if seal.format!=1||seal.operation_id!=request.operation_id||seal.request_sha256!=hash||seal.payload!=expected||seal.payload.links!=2||seal.root!=proof.root||seal.database!=proof.database||seal.directory!=proof.directory||seal.asset_directory!=identity(&assets.held).map_err(err)?||received!=request.byte_length{return Err("Original installed import seal differs; retain bytes".into())}
      let queued_asset=sealed_asset(&request,&proof,&assets,&seal.binary_sha256);
      if seal.asset!=queued_asset{return Err("Original installed asset metadata differs".into())}
      Some((seal,file,seal_stamp))
    }else{None};
    if received!=expected.length||received>request.byte_length{return Err("Original filesystem/SQL progress unresolved; retain for review".into())}
    if sealed.is_none(){
      if rows.is_empty()&&expected!=proof.initial_payload_stamp{return Err("Original empty native payload stamps changed".into())}
      if last.as_ref().is_some_and(|stamp|stamp!=&expected){return Err("Original filesystem/SQL progress unresolved; retain for review".into())}
      if let Some(last)=last{expected=last;}
    }
    let creator=Creator{owner:self.clone(),request,proof,dir,assets,intent,payload,expected,index:rows.len()as u64,seal:sealed};creator.fence(&current)?;physical.check()?;tx.rollback().await.map_err(err)?;Ok(creator)
   }.await;db.retire().await.map_err(|e|format!("Local import resume retirement failed: {e}"))?;result
  }).await
 }
}
