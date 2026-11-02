//! Resume only the original, SQL-acknowledged native progress after lock loss.
//! Partial filesystem/SQL disagreement is retained review, never truncated.
use super::*;
impl Owner {
 pub(crate) async fn resume_local_cover(self,current:Value,request:Request)->Result<Creator,String>{
  request.validate()?;if current!=self.original||current["accountId"]!="local-user"||request.original_binding!=self.binding(){return Err("Original local cover source/session changed".into())}self.fence()?;
  connection::supervise(async move{
   let _guard=snapshot_commit_lock().lock().await;self.fence()?;let mut db=connection::Database::prepare(self.database.clone(),&self.main).await?;let physical=db.fence_owner();
   let result=async{
    let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;strict_preflight(&mut tx).await?;
    let snapshot=read_snapshot_from_connection(&mut tx,&self.provider).await?.ok_or("Original local cover source unavailable")?;
    if snapshot["canonical_revision"]!=self.revision{return Err("Original local cover canonical revision changed".into())}
    let row:Option<(String,String,String)>=sqlx::query_as("SELECT request_sha256,request_json,proof_json FROM native_local_cover_intents WHERE operation_id=?1").bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(err)?;
    let(hash,raw,proof_raw)=row.ok_or("Original local cover intent missing")?;if raw.len()>65536||proof_raw.len()>65536||raw!=serde_json::to_string(&request).map_err(err)?||hash!=sha(raw.as_bytes()){return Err("Exact original local cover request differs".into())}
    let complete:i64=sqlx::query_scalar("SELECT count(*) FROM native_local_cover_receipts WHERE operation_id=?1").bind(&request.operation_id).fetch_one(&mut *tx).await.map_err(err)?;if complete!=0{return Err("Original local cover already committed; inspect its receipt".into())}
    let proof:Proof=serde_json::from_str(&proof_raw).map_err(err)?;
    if proof.format!=1||proof.request_sha256!=hash||proof.uid!=unsafe{libc::geteuid()}||proof.root!=identity(&self.root.held).map_err(err)?||proof.database!=self.database_id||proof.original_draft_sha256!=sha(&serde_json::to_vec(&target(&snapshot,&request)?).map_err(err)?){return Err("Original local cover creator/source differs".into())}
    let dir=self.database.child("local-cover-producers",false).map_err(err)?.child(&request.operation_id,false).map_err(err)?;private(&dir)?;let assets=self.root.child("assets",false).map_err(err)?.child("local-covers",false).map_err(err)?;private(&assets)?;
    if identity(&dir.held).map_err(err)?!=proof.directory{return Err("Original local cover creator directory differs".into())}
    let intent=dir.open("intent.json",libc::O_RDONLY).map_err(err)?;if stamp(&intent)?!=proof.intent||read(&intent,65536)?!=raw.as_bytes(){return Err("Original local cover intent changed".into())}
    let payload=dir.open("payload",libc::O_RDWR).map_err(err)?;if unsafe{libc::flock(payload.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Err("Original native creator remains active; retain for review".into())}
    let mut expected=stamp(&payload)?;if identity(&payload).map_err(err)?!=proof.payload_identity||expected.links!=1{return Err("Original local cover payload identity/aliases differ".into())}
    let rows:Vec<(i64,String)>=sqlx::query_as("SELECT chunk_index,proof_json FROM native_local_cover_chunks WHERE operation_id=?1 ORDER BY chunk_index").bind(&request.operation_id).fetch_all(&mut *tx).await.map_err(err)?;if rows.len()>512{return Err("Original local cover progress exceeds bound".into())}
    let mut received=0;let mut last=None;
    for(index,(ordinal,raw))in rows.iter().enumerate(){if *ordinal!=index as i64||raw.len()>65536{return Err("Original local cover progress order/budget differs".into())}let row:Value=serde_json::from_str(raw).map_err(err)?;let raw=row["raw_chunk"].as_str().ok_or("Original chunk proof missing")?;let chunk:Chunk=serde_json::from_str(raw).map_err(err)?;let file=dir.open(&format!("chunk-{index:08}.json"),libc::O_RDONLY).map_err(err)?;if serde_json::to_value(stamp(&file)?).map_err(err)?!=row["chunk_file"]||read(&file,65536)?!=raw.as_bytes()||chunk.format!=1||chunk.index!=index as u64||chunk.start!=received||chunk.length==0||chunk.length>CHUNK as u64{return Err("Original native chunk proof differs".into())}
     let mut bytes=vec![0;chunk.length as usize];payload.read_exact_at(&mut bytes,received).map_err(err)?;if sha(&bytes)!=chunk.sha256{return Err("Original native acknowledged chunk bytes differ".into())}received+=chunk.length;last=Some(chunk.stamp);
    }
    if rows.is_empty()&&expected!=proof.initial_payload_stamp{return Err("Original empty native payload stamps changed".into())}
    if received!=expected.length||received>request.byte_length||last.as_ref().is_some_and(|stamp|stamp!=&expected){return Err("Original filesystem/SQL progress unresolved; retain for review".into())}
    if let Some(last)=last{expected=last;}
    let creator=Creator{owner:self.clone(),request,proof,dir,assets,intent,payload,expected,index:rows.len()as u64,seal:None};creator.fence(&current)?;physical.check()?;tx.rollback().await.map_err(err)?;Ok(creator)
   }.await;db.retire().await.map_err(|e|format!("Local cover resume retirement failed: {e}"))?;result
  }).await
 }
}
