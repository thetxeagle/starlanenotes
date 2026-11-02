//! Deliberate original-operation discard custody. A plan is not erasure authority
//! after its held descriptors/transaction are released. No path or PID selection.
use super::*;
// This is an independence check only, never a binary or serving capability.
// Do not open canonical entries: closing another main-inode FD can drop this
// process's inherited unix-VFS POSIX locks, even when the path is later refused.
fn independent_canonical_files(owner:&Owner,request:&Request,payload:Identity,snapshot:&Value)->Result<(),String>{
 for asset in snapshot["assets"].as_array().ok_or("Current canonical assets missing")?{
  if asset["id"]==format!("native-cover-{}",request.operation_id){return Err("Current canonical winner asset protects original operation".into())}
  let Some(path)=asset["vault_path"].as_str()else{continue};let path=std::path::Path::new(path);
  if !path.is_absolute()||!path.starts_with(&owner.root.path){return Err("Canonical asset path cannot be proved independent".into())}
  let parent=Directory::capture(path.parent().ok_or("Canonical native parent missing")?).map_err(err)?;
  let leaf=std::ffi::CString::new(path.file_name().and_then(|s|s.to_str()).ok_or("Canonical native leaf invalid")?).map_err(err)?;
  parent.fence().map_err(err)?;let mut stat=std::mem::MaybeUninit::<libc::stat>::uninit();
  if unsafe{libc::fstatat(parent.held.as_raw_fd(),leaf.as_ptr(),stat.as_mut_ptr(),libc::AT_SYMLINK_NOFOLLOW)}!=0{return Err(err(std::io::Error::last_os_error()))}
  let stat=unsafe{stat.assume_init()};parent.fence().map_err(err)?;
  if stat.st_mode&libc::S_IFMT!=libc::S_IFREG{return Err("Canonical native path is not an independent regular file".into())}
  let named=Identity{device:stat.st_dev,inode:stat.st_ino};
  if named==owner.database_id{return Err("Canonical native path borrows the held SQLite main inode".into())}
  if named==payload{return Err("Current canonical file protects original pending payload".into())}
 }
 Ok(())
}
struct PendingDiscard {
 owner:Owner,request:Request,proof:Proof,dir:Directory,intent:File,payload:File,
 expected:Stamp,acknowledged:Vec<(i64,String)>,members:std::collections::BTreeSet<String>,metadata:Vec<(String,File,Stamp,Vec<u8>)>,
}
impl PendingDiscard {
 fn fence(&self,current:&Value)->Result<(),String>{
  if *current!=self.owner.original||current["accountId"]!="local-user"||self.request.original_binding!=self.owner.binding(){return Err("Original discard actor/selection changed".into())}
  self.owner.fence()?;private(&self.dir)?;
  if identity(&self.dir.held).map_err(err)?!=self.proof.directory||stamp(&self.intent)?!=self.proof.intent||read(&self.intent,65536)?!=serde_json::to_vec(&self.request).map_err(err)?||names(&self.dir)?!=self.members{return Err("Original discard family/intent changed".into())}
  if identity(&self.payload).map_err(err)?!=self.proof.payload_identity||stamp(&self.payload)?!=self.expected||self.expected.links!=1{return Err("Original discard payload custody changed".into())}
  let named=self.dir.open("payload",libc::O_RDONLY).map_err(err)?;if stamp(&named)?!=self.expected{return Err("Original discard payload name changed".into())}
  for(name,file,expected,raw)in &self.metadata{
   let named=self.dir.open(name,libc::O_RDONLY).map_err(err)?;
   if stamp(file)?!=*expected||stamp(&named)?!=*expected||read(file,65536)?!=*raw{return Err("Original discard chunk evidence changed".into())}
  }
  Ok(())
 }
}
/// New creator-owned journal, separate from immutable original operation evidence.
/// The completion file is reserved before any future erase, so a late copied JSON
/// result cannot replace its actual native inode. Descriptor-bound erase below
/// consumes this owner; serialized plans alone cannot reconstruct it.
struct DiscardPlan {held:PendingDiscard,plan:File,plan_stamp:Stamp,raw:Vec<u8>,completion:File,completion_stamp:Stamp}
impl DiscardPlan {
 fn prepare(mut held:PendingDiscard,current:&Value)->Result<Self,String>{
  held.fence(current)?;
  let(completion,completion_stamp)=durable(&held.dir,"discard-completion.json",b"")?;
  held.members.insert("discard-completion.json".into());held.fence(current)?;
  let metadata:Vec<_>=held.metadata.iter().map(|(name,_,stamp,raw)|json!({"name":name,"stamp":stamp,"raw_bytes":raw})).collect();
  let mut hash=Sha256::new();let mut offset=0;let mut buffer=[0u8;65536];while offset<held.expected.length{let n=buffer.len().min((held.expected.length-offset)as usize);held.payload.read_exact_at(&mut buffer[..n],offset).map_err(err)?;hash.update(&buffer[..n]);offset+=n as u64;}held.fence(current)?;let payload_sha256=format!("{:x}",hash.finalize());
  let raw=serde_json::to_vec(&json!({"format":1,"payload_sha256":payload_sha256,"operation_id":held.request.operation_id,"request_json":serde_json::to_string(&held.request).map_err(err)?,"creator":held.proof,"payload":held.expected,"native_metadata":metadata,"acknowledged_sql":held.acknowledged,"completion_initial_stamp":completion_stamp,"binding":held.owner.binding()})).map_err(err)?;
  if raw.len()>32*1024*1024{return Err("Original pending discard plan exceeds bound".into())}
  let(plan,plan_stamp)=durable(&held.dir,"discard-plan.json",&raw)?;held.members.insert("discard-plan.json".into());
  let owner=Self{held,plan,plan_stamp,raw,completion,completion_stamp};owner.fence(current)?;Ok(owner)
 }
 fn fence(&self,current:&Value)->Result<(),String>{
  self.held.fence(current)?;
  for(name,file,expected,bytes)in[("discard-plan.json",&self.plan,&self.plan_stamp,self.raw.as_slice()),("discard-completion.json",&self.completion,&self.completion_stamp,b"".as_slice())]{
   let named=self.held.dir.open(name,libc::O_RDONLY).map_err(err)?;
   if stamp(file)?!=*expected||stamp(&named)?!=*expected||read(file,32*1024*1024)?!=bytes{return Err("Original discard journal creator changed".into())}
  }
  Ok(())
 }
}
async fn bind_plan_in(tx:&mut sqlx::SqliteConnection,plan:&DiscardPlan,current:&Value)->Result<(),String>{
 plan.fence(current)?;
 for statement in [
  "CREATE TABLE IF NOT EXISTS native_local_cover_discard_intents(operation_id TEXT PRIMARY KEY REFERENCES native_local_cover_intents(operation_id),request_sha256 TEXT NOT NULL,plan_json TEXT NOT NULL,plan_native_stamp_json TEXT NOT NULL)",
  "CREATE TABLE IF NOT EXISTS native_local_cover_discard_receipts(operation_id TEXT PRIMARY KEY REFERENCES native_local_cover_discard_intents(operation_id),request_sha256 TEXT NOT NULL,receipt_json TEXT NOT NULL)",
  "CREATE TRIGGER IF NOT EXISTS native_local_cover_discard_intents_no_update BEFORE UPDATE ON native_local_cover_discard_intents BEGIN SELECT RAISE(ABORT,'Original discard intent immutable'); END",
  "CREATE TRIGGER IF NOT EXISTS native_local_cover_discard_intents_no_delete BEFORE DELETE ON native_local_cover_discard_intents BEGIN SELECT RAISE(ABORT,'Original discard intent immutable'); END",
  "CREATE TRIGGER IF NOT EXISTS native_local_cover_discard_receipts_no_update BEFORE UPDATE ON native_local_cover_discard_receipts BEGIN SELECT RAISE(ABORT,'Original discard receipt immutable'); END",
  "CREATE TRIGGER IF NOT EXISTS native_local_cover_discard_receipts_no_delete BEFORE DELETE ON native_local_cover_discard_receipts BEGIN SELECT RAISE(ABORT,'Original discard receipt immutable'); END",
 ]{sqlx::query(statement).execute(&mut *tx).await.map_err(err)?;}
 sqlx::query("INSERT INTO native_local_cover_discard_intents(operation_id,request_sha256,plan_json,plan_native_stamp_json) VALUES(?1,?2,?3,?4)").bind(&plan.held.request.operation_id).bind(&plan.held.proof.request_sha256).bind(String::from_utf8(plan.raw.clone()).map_err(err)?).bind(serde_json::to_string(&plan.plan_stamp).map_err(err)?).execute(&mut *tx).await.map_err(err)?;
 plan.fence(current)?;Ok(())
}
// A reopened plan still needs the actual original native creator, current
// source and held idle payload. SQL/native JSON alone cannot construct a plan.
async fn reopen_prepared_in(owner:Owner,current:&Value,request:Request,tx:&mut sqlx::SqliteConnection)->Result<DiscardPlan,String>{
 request.validate()?;if *current!=owner.original||request.original_binding!=owner.binding()||current["accountId"]!="local-user"{return Err("Original prepared discard actor changed".into())}owner.fence()?;
 let original:Option<(String,String,String)>=sqlx::query_as("SELECT request_sha256,request_json,proof_json FROM native_local_cover_intents WHERE operation_id=?1").bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(err)?;let(hash,intent_raw,proof_raw)=original.ok_or("Original native intent missing")?;
 if intent_raw.len()>65536||proof_raw.len()>65536||hash!=sha(intent_raw.as_bytes())||intent_raw!=serde_json::to_string(&request).map_err(err)?{return Err("Original prepared request differs".into())}
 let proof:Proof=serde_json::from_str(&proof_raw).map_err(err)?;if proof.format!=1||proof.request_sha256!=hash||proof.uid!=unsafe{libc::geteuid()}||proof.root!=identity(&owner.root.held).map_err(err)?||proof.database!=owner.database_id{return Err("Original prepared native root/main differs".into())}
 let recorded:Option<(String,String,String)>=sqlx::query_as("SELECT request_sha256,plan_json,plan_native_stamp_json FROM native_local_cover_discard_intents WHERE operation_id=?1").bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(err)?;let(request_hash,raw,plan_stamp_raw)=recorded.ok_or("Original prepared SQL disposition missing")?;
 if request_hash!=hash||raw.len()>32*1024*1024||plan_stamp_raw.len()>65536{return Err("Prepared original SQL disposition differs".into())}
 let value:Value=serde_json::from_str(&raw).map_err(err)?;if value.as_object().map(|o|o.len())!=Some(10)||value["format"]!=1||value["operation_id"]!=request.operation_id||value["request_json"]!=intent_raw||value["creator"]!=serde_json::to_value(&proof).map_err(err)?||value["binding"]!=owner.binding(){return Err("Original native discard plan binding differs".into())}
 let dir=owner.database.child("local-cover-producers",false).map_err(err)?.child(&request.operation_id,false).map_err(err)?;private(&dir)?;if identity(&dir.held).map_err(err)?!=proof.directory{return Err("Original prepared creator directory differs".into())}
 let intent=dir.open("intent.json",libc::O_RDONLY).map_err(err)?;if stamp(&intent)?!=proof.intent||read(&intent,65536)?!=intent_raw.as_bytes(){return Err("Original prepared intent creator differs".into())}
 let plan=dir.open("discard-plan.json",libc::O_RDONLY).map_err(err)?;let plan_stamp:Stamp=serde_json::from_str(&plan_stamp_raw).map_err(err)?;if stamp(&plan)?!=plan_stamp||read(&plan,32*1024*1024)?!=raw.as_bytes(){return Err("Original native plan creator differs".into())}
 let completion=dir.open("discard-completion.json",libc::O_RDWR).map_err(err)?;let completion_stamp:Stamp=serde_json::from_value(value["completion_initial_stamp"].clone()).map_err(err)?;if stamp(&completion)?!=completion_stamp||completion_stamp.length!=0||completion_stamp.links!=1{return Err("Native completion is no longer pending; inspect qualified original result".into())}
 let payload=dir.open("payload",libc::O_RDWR).map_err(|_|"Original payload absent without a qualified complete observation; retain review".to_string())?;if unsafe{libc::flock(payload.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Err("Original prepared writer remains active".into())}
 let expected:Stamp=serde_json::from_value(value["payload"].clone()).map_err(err)?;if stamp(&payload)?!=expected||identity(&payload).map_err(err)?!=proof.payload_identity||expected.links!=1||expected.length>request.byte_length{return Err("Original prepared payload identity/bytes changed".into())}
 let mut hash_bytes=Sha256::new();let mut offset=0;let mut buffer=[0u8;65536];while offset<expected.length{let n=buffer.len().min((expected.length-offset)as usize);payload.read_exact_at(&mut buffer[..n],offset).map_err(err)?;hash_bytes.update(&buffer[..n]);offset+=n as u64;}if value["payload_sha256"]!=format!("{:x}",hash_bytes.finalize()){return Err("Original prepared payload full digest differs".into())}
 let acknowledged:Vec<(i64,String)>=serde_json::from_value(value["acknowledged_sql"].clone()).map_err(err)?;if acknowledged.len()>480{return Err("Prepared original SQL chunks exceed bound".into())}
 let mut members=std::collections::BTreeSet::from(["intent.json".into(),"payload".into(),"discard-plan.json".into(),"discard-completion.json".into()]);let mut metadata=vec![];
 let rows=value["native_metadata"].as_array().filter(|a|a.len()<=480).ok_or("Original prepared metadata exceeds bound")?;
 for row in rows{let name=row["name"].as_str().filter(|s|s.len()==19&&s.starts_with("chunk-")&&s.ends_with(".json")&&s.as_bytes()[6..14].iter().all(|b|b.is_ascii_digit())).ok_or("Prepared native chunk name invalid")?.to_string();if !members.insert(name.clone()){return Err("Prepared native chunk repeated".into())}let file=dir.open(&name,libc::O_RDONLY).map_err(err)?;let expected:Stamp=serde_json::from_value(row["stamp"].clone()).map_err(err)?;let bytes:Vec<u8>=serde_json::from_value(row["raw_bytes"].clone()).map_err(err)?;if bytes.len()>65536||stamp(&file)?!=expected||read(&file,65536)?!=bytes{return Err("Original prepared native metadata differs".into())}metadata.push((name,file,expected,bytes));}
 let held=PendingDiscard{owner,request,proof,dir,intent,payload,expected,acknowledged,members,metadata};let result=DiscardPlan{held,plan,plan_stamp,raw:raw.into_bytes(),completion,completion_stamp};result.fence(current)?;Ok(result)
}
// First bounded erase consumes the actual prepared native owner, never JSON.
// A restart reader must separately prove the original journal before receiving
// an equivalent capability; no missing-payload fallback is supplied here.
async fn erase_prepared_in(tx:&mut sqlx::SqliteConnection,plan:DiscardPlan,current:&Value)->Result<Value,String>{
 plan.fence(current)?;strict_preflight(tx).await?;
 let recorded:Option<(String,String,String)>=sqlx::query_as("SELECT request_sha256,plan_json,plan_native_stamp_json FROM native_local_cover_discard_intents WHERE operation_id=?1").bind(&plan.held.request.operation_id).fetch_optional(&mut *tx).await.map_err(err)?;
 if recorded!=Some((plan.held.proof.request_sha256.clone(),String::from_utf8(plan.raw.clone()).map_err(err)?,serde_json::to_string(&plan.plan_stamp).map_err(err)?)){return Err("Original prepared SQL/native disposition differs".into())}
 let original:Option<(String,String,String)>=sqlx::query_as("SELECT request_sha256,request_json,proof_json FROM native_local_cover_intents WHERE operation_id=?1").bind(&plan.held.request.operation_id).fetch_optional(&mut *tx).await.map_err(err)?;
 if original!=Some((plan.held.proof.request_sha256.clone(),serde_json::to_string(&plan.held.request).map_err(err)?,serde_json::to_string(&plan.held.proof).map_err(err)?)){return Err("Original discard intent changed before erase".into())}
 let snapshot=read_snapshot_from_connection(tx,&plan.held.owner.provider).await?.ok_or("Current original discard source missing")?;
 let winners:i64=sqlx::query_scalar("SELECT COUNT(*) FROM native_local_cover_receipts WHERE operation_id=?1").bind(&plan.held.request.operation_id).fetch_one(&mut *tx).await.map_err(err)?;
 let pin=format!("starlane-local-attachment://{}",plan.held.request.operation_id);
 if winners!=0||snapshot["canonical_revision"]!=plan.held.owner.revision||sha(&serde_json::to_vec(&target(&snapshot,&plan.held.request)?).map_err(err)?)!=plan.held.proof.original_draft_sha256||protects(&snapshot,&pin){return Err("Fresh current winner/source/reference protects original operation".into())}
 independent_canonical_files(&plan.held.owner,&plan.held.request,plan.held.proof.payload_identity,&snapshot)?;
 // Fresh independence is reproved under the same held canonical transaction.
 let acknowledged:Vec<(i64,String)>=sqlx::query_as("SELECT chunk_index,proof_json FROM native_local_cover_chunks WHERE operation_id=?1 ORDER BY chunk_index").bind(&plan.held.request.operation_id).fetch_all(&mut *tx).await.map_err(err)?;if acknowledged!=plan.held.acknowledged{return Err("Original acknowledged SQL chunks changed before erase".into())}
 if serde_json::to_vec(&plan.held.owner.binding()).map_err(err)?.len()>65536-8192{return Err("Original native completion metadata exceeds bound".into())}
 plan.fence(current)?;
 plan.held.dir.unlink("payload").map_err(err)?;
 let after=stamp(&plan.held.payload)?;if identity(&plan.held.payload).map_err(err)?!=plan.held.proof.payload_identity||after.links!=0||after.length!=plan.held.expected.length{return Err("Original unlink outcome unknown; retain plan/evidence".into())}
 let raw=serde_json::to_vec(&json!({"format":1,"state":"discarded_original_payload","operation_id":plan.held.request.operation_id,"request_sha256":plan.held.proof.request_sha256,"plan_sha256":sha(&plan.raw),"payload_after":after,"binding":plan.held.owner.binding()})).map_err(err)?;
 // Only the original pre-created empty completion inode can receive this result.
 let named=plan.held.dir.open("discard-completion.json",libc::O_RDONLY).map_err(err)?;
 if stamp(&named)?!=plan.completion_stamp||stamp(&plan.completion)?!=plan.completion_stamp{return Err("Original completion creator changed; erasure result unknown".into())}
 plan.completion.write_all_at(&raw,0).map_err(err)?;plan.completion.sync_all().map_err(err)?;plan.held.dir.held.sync_all().map_err(err)?;
 let completion_stamp=stamp(&plan.completion)?;let named=plan.held.dir.open("discard-completion.json",libc::O_RDONLY).map_err(err)?;
 if stamp(&named)?!=completion_stamp||read(&named,65536)?!=raw{return Err("Original native completion readback differs".into())}
 let mut members=plan.held.members.clone();members.remove("payload");if names(&plan.held.dir)?!=members{return Err("Original disposition family changed after erase".into())}
 plan.held.owner.fence()?;private(&plan.held.dir)?;
 if identity(&plan.held.dir.held).map_err(err)?!=plan.held.proof.directory||stamp(&plan.held.intent)?!=plan.held.proof.intent||read(&plan.held.intent,65536)?!=serde_json::to_vec(&plan.held.request).map_err(err)?{return Err("Original retained intent changed after erase".into())}
 let native_plan=plan.held.dir.open("discard-plan.json",libc::O_RDONLY).map_err(err)?;if stamp(&native_plan)?!=plan.plan_stamp||read(&native_plan,32*1024*1024)?!=plan.raw{return Err("Original retained plan changed after erase".into())}
 for(name,file,expected,bytes)in &plan.held.metadata{let named=plan.held.dir.open(name,libc::O_RDONLY).map_err(err)?;if stamp(file)?!=*expected||stamp(&named)?!=*expected||read(&named,65536)?!=*bytes{return Err("Original retained chunk evidence changed after erase".into())}}
 let receipt=json!({"format":1,"state":"discarded_original_payload","operation_id":plan.held.request.operation_id,"native_completion_json":String::from_utf8(raw).map_err(err)?,"native_completion_stamp":completion_stamp});
 sqlx::query("INSERT INTO native_local_cover_discard_receipts(operation_id,request_sha256,receipt_json) VALUES(?1,?2,?3)").bind(&plan.held.request.operation_id).bind(&plan.held.proof.request_sha256).bind(serde_json::to_string(&receipt).map_err(err)?).execute(&mut *tx).await.map_err(err)?;
 plan.held.owner.fence()?;Ok(receipt)
}
// Exact original native completion observation. This returns historical outcome
// only; it grants no file read, serving, import or subsequent unlink capability.
async fn reconcile_completed_in(owner:Owner,current:&Value,request:Request,tx:&mut sqlx::SqliteConnection)->Result<Value,String>{
 request.validate()?;if *current!=owner.original||current["accountId"]!="local-user"||request.original_binding!=owner.binding(){return Err("Current original discard observation context changed".into())}owner.fence()?;strict_preflight(tx).await?;
 let row:Option<(String,String,String)>=sqlx::query_as("SELECT request_sha256,request_json,proof_json FROM native_local_cover_intents WHERE operation_id=?1").bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(err)?;let(hash,intent_raw,proof_raw)=row.ok_or("Original discard intent unavailable")?;
 if intent_raw.len()>65536||proof_raw.len()>65536||hash!=sha(intent_raw.as_bytes())||intent_raw!=serde_json::to_string(&request).map_err(err)?{return Err("Exact original discard intent differs".into())}
 let proof:Proof=serde_json::from_str(&proof_raw).map_err(err)?;if proof.format!=1||proof.request_sha256!=hash||proof.uid!=unsafe{libc::geteuid()}||proof.root!=identity(&owner.root.held).map_err(err)?||proof.database!=owner.database_id{return Err("Original discard observation creator differs".into())}
 let snapshot=read_snapshot_from_connection(tx,&owner.provider).await?.ok_or("Current original discard source unavailable")?;let pin=format!("starlane-local-attachment://{}",request.operation_id);let winners:i64=sqlx::query_scalar("SELECT COUNT(*) FROM native_local_cover_receipts WHERE operation_id=?1").bind(&request.operation_id).fetch_one(&mut *tx).await.map_err(err)?;
 if winners!=0||snapshot["canonical_revision"]!=owner.revision||protects(&snapshot,&pin)||sha(&serde_json::to_vec(&target(&snapshot,&request)?).map_err(err)?)!=proof.original_draft_sha256{return Err("Current canonical source/winner protects original operation".into())}
 let row:Option<(String,String,String)>=sqlx::query_as("SELECT request_sha256,plan_json,plan_native_stamp_json FROM native_local_cover_discard_intents WHERE operation_id=?1").bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(err)?;let(plan_hash,plan_raw,plan_stamp_raw)=row.ok_or("Original discard plan unavailable")?;
 if plan_hash!=hash||plan_raw.len()>32*1024*1024||plan_stamp_raw.len()>65536{return Err("Original discard plan exceeds bound or differs".into())}
 let value:Value=serde_json::from_str(&plan_raw).map_err(err)?;if value.as_object().map(|o|o.len())!=Some(10)||value["format"]!=1||value["operation_id"]!=request.operation_id||value["request_json"]!=intent_raw||value["creator"]!=serde_json::to_value(&proof).map_err(err)?||value["binding"]!=owner.binding(){return Err("Original completed plan binding differs".into())}
 let dir=owner.database.child("local-cover-producers",false).map_err(err)?.child(&request.operation_id,false).map_err(err)?;private(&dir)?;if identity(&dir.held).map_err(err)?!=proof.directory{return Err("Original completed native family changed".into())}
 let intent=dir.open("intent.json",libc::O_RDONLY).map_err(err)?;if stamp(&intent)?!=proof.intent||read(&intent,65536)?!=intent_raw.as_bytes(){return Err("Original retained native intent differs".into())}
 let plan=dir.open("discard-plan.json",libc::O_RDONLY).map_err(err)?;let plan_stamp:Stamp=serde_json::from_str(&plan_stamp_raw).map_err(err)?;if stamp(&plan)?!=plan_stamp||read(&plan,32*1024*1024)?!=plan_raw.as_bytes(){return Err("Original completed native plan creator differs".into())}
 let completion=dir.open("discard-completion.json",libc::O_RDONLY).map_err(err)?;let actual=stamp(&completion)?;let initial:Stamp=serde_json::from_value(value["completion_initial_stamp"].clone()).map_err(err)?;
 if actual.device!=initial.device||actual.inode!=initial.inode||actual.uid!=initial.uid||actual.mode!=initial.mode||actual.links!=1||initial.length!=0||initial.links!=1||actual.length==0||actual.length>65536{return Err("Original complete native result creator unavailable".into())}
 let raw=read(&completion,65536)?;let result:Value=serde_json::from_slice(&raw).map_err(|_|"Original native completion incomplete; retain review")?;let after:Stamp=serde_json::from_value(result["payload_after"].clone()).map_err(err)?;let before:Stamp=serde_json::from_value(value["payload"].clone()).map_err(err)?;
 if result.as_object().map(|o|o.len())!=Some(7)||result["format"]!=1||result["state"]!="discarded_original_payload"||result["operation_id"]!=request.operation_id||result["request_sha256"]!=hash||result["plan_sha256"]!=sha(plan_raw.as_bytes())||result["binding"]!=owner.binding()||before.device!=proof.payload_identity.device||before.inode!=proof.payload_identity.inode||before.uid!=proof.uid||before.links!=1||after.device!=proof.payload_identity.device||after.inode!=proof.payload_identity.inode||after.uid!=proof.uid||after.mode!=before.mode||after.length!=before.length||after.length>request.byte_length||after.links!=0{return Err("Original complete native result binding differs".into())}
 if dir.file_identity("payload").map_err(err)?.is_some(){return Err("Original disposed payload path exists; retain review without deletion".into())}
 let mut members=std::collections::BTreeSet::from(["intent.json".into(),"discard-plan.json".into(),"discard-completion.json".into()]);let mut metadata=vec![];let rows=value["native_metadata"].as_array().filter(|a|a.len()<=480).ok_or("Original completed metadata exceeds bound")?;
 for row in rows{let name=row["name"].as_str().filter(|s|s.len()==19&&s.starts_with("chunk-")&&s.ends_with(".json")&&s.as_bytes()[6..14].iter().all(|b|b.is_ascii_digit())).ok_or("Original completed chunk name invalid")?.to_string();if !members.insert(name.clone()){return Err("Original completed chunk repeated".into())}let file=dir.open(&name,libc::O_RDONLY).map_err(err)?;let expected:Stamp=serde_json::from_value(row["stamp"].clone()).map_err(err)?;let bytes:Vec<u8>=serde_json::from_value(row["raw_bytes"].clone()).map_err(err)?;if bytes.len()>65536||stamp(&file)?!=expected||read(&file,65536)?!=bytes{return Err("Original completed chunk evidence differs".into())}metadata.push((name,file,expected,bytes));}
 let acknowledged:Vec<(i64,String)>=sqlx::query_as("SELECT chunk_index,proof_json FROM native_local_cover_chunks WHERE operation_id=?1 ORDER BY chunk_index").bind(&request.operation_id).fetch_all(&mut *tx).await.map_err(err)?;if serde_json::to_value(&acknowledged).map_err(err)?!=value["acknowledged_sql"]{return Err("Original completed acknowledged SQL evidence differs".into())}
 if names(&dir)?!=members{return Err("Original completed family membership differs".into())}
 let receipt=json!({"format":1,"state":"discarded_original_payload","operation_id":request.operation_id,"native_completion_json":String::from_utf8(raw.clone()).map_err(err)?,"native_completion_stamp":actual});
 let prior:Option<(String,String)>=sqlx::query_as("SELECT request_sha256,receipt_json FROM native_local_cover_discard_receipts WHERE operation_id=?1").bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(err)?;
 if let Some((request_hash,stored))=prior{if request_hash!=hash||stored.len()>65536||serde_json::from_str::<Value>(&stored).map_err(err)?!=receipt{return Err("Original completed SQL/native observation differs".into())}}
 else{sqlx::query("INSERT INTO native_local_cover_discard_receipts(operation_id,request_sha256,receipt_json) VALUES(?1,?2,?3)").bind(&request.operation_id).bind(&hash).bind(serde_json::to_string(&receipt).map_err(err)?).execute(&mut *tx).await.map_err(err)?;}
 owner.fence()?;private(&dir)?;if identity(&dir.held).map_err(err)?!=proof.directory||names(&dir)?!=members||stamp(&intent)?!=proof.intent||stamp(&plan)?!=plan_stamp||stamp(&completion)?!=actual||read(&completion,65536)?!=raw||dir.file_identity("payload").map_err(err)?.is_some(){return Err("Original native completion changed before observation".into())}
 for(name,file,expected,bytes)in metadata{let named=dir.open(&name,libc::O_RDONLY).map_err(err)?;if stamp(&file)?!=expected||stamp(&named)?!=expected||read(&named,65536)?!=bytes{return Err("Original completed evidence changed before observation".into())}}
 Ok(receipt)
}
fn protects(value:&Value,pin:&str)->bool{
 use pulldown_cmark::{Event,Parser,Tag};
 match value{
  Value::String(s)=>s==pin||Parser::new(s).any(|event|matches!(event,Event::Start(Tag::Link{dest_url,..}|Tag::Image{dest_url,..})if dest_url.as_ref()==pin)),
  Value::Array(a)=>a.iter().any(|v|protects(v,pin)),Value::Object(o)=>o.values().any(|v|protects(v,pin)),_=>false,
 }
}
async fn capture_in(owner:Owner,current:&Value,request:Request,tx:&mut sqlx::SqliteConnection)->Result<PendingDiscard,String>{
 request.validate()?;if *current!=owner.original||current["accountId"]!="local-user"||request.original_binding!=owner.binding(){return Err("Exact original pending discard context required".into())}owner.fence()?;strict_preflight(tx).await?;
 let snapshot=read_snapshot_from_connection(tx,&owner.provider).await?.ok_or("Current canonical discard source missing")?;
 if snapshot["canonical_revision"]!=owner.revision{return Err("Original discard canonical revision changed".into())}
 let row:Option<(String,String,String)>=sqlx::query_as("SELECT request_sha256,request_json,proof_json FROM native_local_cover_intents WHERE operation_id=?1").bind(&request.operation_id).fetch_optional(&mut *tx).await.map_err(err)?;
 let(hash,raw,proof_raw)=row.ok_or("Original pending SQL intent missing")?;
 if raw.len()>65536||proof_raw.len()>65536||raw!=serde_json::to_string(&request).map_err(err)?||sha(raw.as_bytes())!=hash{return Err("Original pending SQL request differs".into())}
 let winners:i64=sqlx::query_scalar("SELECT COUNT(*) FROM native_local_cover_receipts WHERE operation_id=?1").bind(&request.operation_id).fetch_one(&mut *tx).await.map_err(err)?;
 if winners!=0{return Err("Winning original operation cannot be discarded".into())}
 let proof:Proof=serde_json::from_str(&proof_raw).map_err(err)?;
 if proof.format!=1||proof.request_sha256!=hash||proof.uid!=unsafe{libc::geteuid()}||proof.root!=identity(&owner.root.held).map_err(err)?||proof.database!=owner.database_id||proof.original_draft_sha256!=sha(&serde_json::to_vec(&target(&snapshot,&request)?).map_err(err)?){return Err("Original pending creator/draft differs".into())}
 let pin=format!("starlane-local-attachment://{}",request.operation_id);if protects(&snapshot,&pin){return Err("Current canonical source protects original pending operation".into())}
 let dir=owner.database.child("local-cover-producers",false).map_err(err)?.child(&request.operation_id,false).map_err(err)?;private(&dir)?;
 if identity(&dir.held).map_err(err)?!=proof.directory{return Err("Original pending native creator directory differs".into())}
 let intent=dir.open("intent.json",libc::O_RDONLY).map_err(err)?;if stamp(&intent)?!=proof.intent||read(&intent,65536)?!=raw.as_bytes(){return Err("Original pending native intent differs".into())}
 let payload=dir.open("payload",libc::O_RDWR).map_err(err)?;
 if unsafe{libc::flock(payload.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}!=0{return Err("Original pending creator remains active".into())}
 let expected=stamp(&payload)?;if identity(&payload).map_err(err)?!=proof.payload_identity||expected.links!=1||expected.length>request.byte_length{return Err("Original pending payload owner/aliases/bound differs".into())}
 independent_canonical_files(&owner,&request,proof.payload_identity,&snapshot)?;
 let rows:Vec<(i64,String)>=sqlx::query_as("SELECT chunk_index,proof_json FROM native_local_cover_chunks WHERE operation_id=?1 ORDER BY chunk_index").bind(&request.operation_id).fetch_all(&mut *tx).await.map_err(err)?;
 if rows.len()>480{return Err("Original pending SQL chunks exceed bound".into())}
 let members=names(&dir)?;let mut metadata=vec![];let mut native_chunks=0usize;
 for name in &members{
  if name=="intent.json"||name=="payload"{continue}
  let index=name.strip_prefix("chunk-").and_then(|s|s.strip_suffix(".json")).filter(|s|s.len()==8&&s.bytes().all(|b|b.is_ascii_digit())).and_then(|s|s.parse::<usize>().ok()).ok_or("Unsupported original pending family member; retain review")?;
  if index>=480{return Err("Original native chunk exceeds bound".into())}native_chunks+=1;
  let file=dir.open(name,libc::O_RDONLY).map_err(err)?;let actual=stamp(&file)?;if actual.links!=1{return Err("Original native chunk evidence has aliases".into())}let raw=read(&file,65536)?;metadata.push((name.clone(),file,actual,raw));
 }
 if native_chunks<rows.len()||!members.contains("intent.json")||!members.contains("payload"){return Err("Original pending family closure differs".into())}
 for(index,(ordinal,raw))in rows.iter().enumerate(){
  if *ordinal!=index as i64||raw.len()>65536{return Err("Original pending acknowledged order differs".into())}let evidence:Value=serde_json::from_str(raw).map_err(err)?;let name=format!("chunk-{index:08}.json");let(_,_,actual,bytes)=metadata.iter().find(|(n,_,_,_)|n==&name).ok_or("Original acknowledged native chunk missing")?;
  if serde_json::to_value(actual).map_err(err)?!=evidence["chunk_file"]||evidence["raw_chunk"].as_str().map(str::as_bytes)!=Some(bytes.as_slice()){return Err("Original pending acknowledged evidence differs".into())}
 }
 let held=PendingDiscard{owner,request,proof,dir,intent,payload,expected,acknowledged:rows,members,metadata};held.fence(current)?;Ok(held)
}

impl Owner {
 pub(crate) async fn discard_local_cover(self,current:Value,request:Request,inspect_only:bool)->Result<Value,String>{
  request.validate()?;if current!=self.original||current["accountId"]!="local-user"||request.original_binding!=self.binding(){return Err("Current original discard selection changed".into())}self.fence()?;
  connection::supervise(async move{
   let _guard=snapshot_commit_lock().lock().await;self.fence()?;let mut db=connection::Database::prepare(self.database.clone(),&self.main).await?;let physical=db.fence_owner();
   let result=async{
    let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;strict_preflight(&mut tx).await?;
    let exists:i64=sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='native_local_cover_discard_intents'").fetch_one(&mut *tx).await.map_err(err)?;
    let known=if exists==1{sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM native_local_cover_discard_intents WHERE operation_id=?1").bind(&request.operation_id).fetch_one(&mut *tx).await.map_err(err)?==1}else{false};
    let receipt=if inspect_only{reconcile_completed_in(self.clone(),&current,request.clone(),&mut tx).await?}
    else if known{
     let directory=self.database.child("local-cover-producers",false).map_err(err)?.child(&request.operation_id,false).map_err(err)?;
     let completion=directory.open("discard-completion.json",libc::O_RDONLY).map_err(err)?;
     if stamp(&completion)?.length>0{reconcile_completed_in(self.clone(),&current,request.clone(),&mut tx).await?}
     else{let prepared=reopen_prepared_in(self.clone(),&current,request.clone(),&mut tx).await?;erase_prepared_in(&mut tx,prepared,&current).await?}
    }else{
     let held=capture_in(self.clone(),&current,request.clone(),&mut tx).await?;let prepared=DiscardPlan::prepare(held,&current)?;bind_plan_in(&mut tx,&prepared,&current).await?;physical.check()?;tx.commit().await.map_err(err)?;
     tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;erase_prepared_in(&mut tx,prepared,&current).await?
    };
    self.fence()?;physical.check()?;tx.commit().await.map_err(err)?;self.fence()?;
    let request_json=serde_json::to_string(&request).map_err(err)?;Ok(json!({"format":1,"state":"discarded_original_payload","operation_id":request.operation_id,"request_sha256":sha(request_json.as_bytes()),"request_json":request_json,"receipt_json":serde_json::to_string(&receipt).map_err(err)?}))
   }.await;db.retire().await.map_err(|error|format!("Original discard connection retirement failed: {error}"))?;result
  }).await
 }
}
#[cfg(test)]mod tests{
 use super::*;
 fn independent_writer(main:&std::path::Path,expected_locked:bool){
  // Separate interpreter/process and ordinary SQLite VFS, not the parent's
  // custom VFS or mutex. A real writer must stay excluded while the caller's
  // inherited unix-VFS POSIX main lock is held.
  let script="import sqlite3,sys\nc=sqlite3.connect(sys.argv[1],timeout=0)\ntry:\n c.execute('BEGIN IMMEDIATE'); c.rollback(); print('writer-admitted')\nexcept sqlite3.OperationalError as e:\n if 'locked' not in str(e): raise\n print('writer-locked')\nfinally: c.close()\n";
  let result=std::process::Command::new("python3").arg("-c").arg(script).arg(main).output().unwrap();
  assert!(result.status.success(),"{}",String::from_utf8_lossy(&result.stderr));
  assert_eq!(String::from_utf8_lossy(&result.stdout).trim(),if expected_locked{"writer-locked"}else{"writer-admitted"});
 }
 #[tokio::test]async fn borrowed_canonical_main_is_refused_without_releasing_the_actual_writer_lock(){
  let(root,owner,request)=super::super::tests::fixture("discard-borrowed-main-lock").await;
  let mut creator=owner.clone().prepare_local_cover(owner.original.clone(),request.clone()).await.unwrap();creator.append(&owner.original,0,&[1,2,3]).unwrap();drop(creator);
  let main=root.join(".starlane/starlane-notes.sqlite3");let check_main=main.clone();
  connection::supervise(async move{let _guard=snapshot_commit_lock().lock().await;let mut db=connection::Database::prepare(owner.database.clone(),&owner.main).await?;
   let result=async{let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;
    let asset=json!({"id":"borrowed-main","name":"ordinary.png","vault_path":check_main});
    sqlx::query("INSERT INTO local_assets(id,name,payload) VALUES('borrowed-main','ordinary.png',?1)").bind(serde_json::to_string(&asset).unwrap()).execute(&mut *tx).await.map_err(err)?;
    independent_writer(&check_main,true);
    let failure=capture_in(owner.clone(),&owner.original,request.clone(),&mut tx).await.err().ok_or("Borrowed main canonical row was accepted")?;
    assert!(failure.contains("held SQLite main inode"),"{failure}");
    independent_writer(&check_main,true);tx.rollback().await.map_err(err)?;Ok::<_,String>(())}.await;
   db.retire().await?;result}).await.unwrap();
  independent_writer(&main,false);assert_eq!(std::fs::read(root.join(".starlane/local-cover-producers/11111111-1111-4111-8111-111111111111/payload")).unwrap(),[1,2,3]);std::fs::remove_dir_all(root).unwrap();
 }
 #[tokio::test]async fn prepared_discard_rechecks_canonical_main_before_unlink_and_preserves_lock(){
  let(root,owner,request)=super::super::tests::fixture("discard-prepared-main-lock").await;
  let mut creator=owner.clone().prepare_local_cover(owner.original.clone(),request.clone()).await.unwrap();creator.append(&owner.original,0,&[1,2,3]).unwrap();drop(creator);prepare(owner.clone(),request.clone()).await.unwrap();
  let main=root.join(".starlane/starlane-notes.sqlite3");let check_main=main.clone();
  connection::supervise(async move{let _guard=snapshot_commit_lock().lock().await;let mut db=connection::Database::prepare(owner.database.clone(),&owner.main).await?;
   let result=async{let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;
    let plan=reopen_prepared_in(owner.clone(),&owner.original,request,&mut tx).await?;
    let asset=json!({"id":"borrowed-main","name":"ordinary.png","vault_path":check_main});
    sqlx::query("INSERT INTO local_assets(id,name,payload) VALUES('borrowed-main','ordinary.png',?1)").bind(serde_json::to_string(&asset).unwrap()).execute(&mut *tx).await.map_err(err)?;
    independent_writer(&check_main,true);let failure=erase_prepared_in(&mut tx,plan,&owner.original).await.unwrap_err();assert!(failure.contains("held SQLite main inode"),"{failure}");independent_writer(&check_main,true);tx.rollback().await.map_err(err)?;Ok::<_,String>(())}.await;
   db.retire().await?;result}).await.unwrap();independent_writer(&main,false);
  let dir=root.join(".starlane/local-cover-producers/11111111-1111-4111-8111-111111111111");assert_eq!(std::fs::read(dir.join("payload")).unwrap(),[1,2,3]);assert_eq!(std::fs::metadata(dir.join("discard-completion.json")).unwrap().len(),0);std::fs::remove_dir_all(root).unwrap();
 }
 #[tokio::test]async fn named_independence_rejects_main_alias_payload_and_symlink_without_one_link_exemptions(){
  let(root,owner,request)=super::super::tests::fixture("discard-named-inode-controls").await;
  let mut creator=owner.clone().prepare_local_cover(owner.original.clone(),request.clone()).await.unwrap();creator.append(&owner.original,0,&[1,2,3]).unwrap();let payload=creator.proof.payload_identity;drop(creator);
  let main=root.join(".starlane/starlane-notes.sqlite3");let alias=root.join("borrowed-any-name");std::fs::hard_link(&main,&alias).unwrap();
  for path in [&main,&alias]{let snapshot=json!({"assets":[{"id":"unrelated","vault_path":path}]});assert!(independent_canonical_files(&owner,&request,payload,&snapshot).unwrap_err().contains("held SQLite main inode"));}
  std::fs::remove_file(&alias).unwrap();let payload_path=root.join(".starlane/local-cover-producers/11111111-1111-4111-8111-111111111111/payload");let snapshot=json!({"assets":[{"id":"unrelated","vault_path":payload_path}]});assert!(independent_canonical_files(&owner,&request,payload,&snapshot).unwrap_err().contains("protects original pending payload"));
  let regular=root.join("genuine-canonical");std::fs::write(&regular,[9,8,7]).unwrap();std::fs::hard_link(&regular,&alias).unwrap();let snapshot=json!({"assets":[{"id":"unrelated","vault_path":alias}]});independent_canonical_files(&owner,&request,payload,&snapshot).unwrap();
  std::fs::remove_file(&alias).unwrap();std::os::unix::fs::symlink(&regular,&alias).unwrap();assert!(independent_canonical_files(&owner,&request,payload,&snapshot).unwrap_err().contains("regular file"));std::fs::remove_dir_all(root).unwrap();
 }
 async fn observe(owner:Owner,request:Request)->Result<u64,String>{
  let current=owner.original.clone();connection::supervise(async move{let _guard=snapshot_commit_lock().lock().await;let mut db=connection::Database::prepare(owner.database.clone(),&owner.main).await?;let physical=db.fence_owner();let result=async{let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;let held=capture_in(owner,&current,request,&mut tx).await?;held.fence(&current)?;physical.check()?;let length=held.expected.length;tx.rollback().await.map_err(err)?;Ok(length)}.await;db.retire().await?;result}).await
 }
 async fn prepare(owner:Owner,request:Request)->Result<(),String>{
  let current=owner.original.clone();connection::supervise(async move{let _guard=snapshot_commit_lock().lock().await;let mut db=connection::Database::prepare(owner.database.clone(),&owner.main).await?;let physical=db.fence_owner();let result=async{let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;let held=capture_in(owner,&current,request,&mut tx).await?;let plan=DiscardPlan::prepare(held,&current)?;bind_plan_in(&mut tx,&plan,&current).await?;plan.fence(&current)?;physical.check()?;tx.commit().await.map_err(err)?;Ok(())}.await;db.retire().await?;result}).await
 }
 #[tokio::test]async fn new_discard_journal_retains_original_raw_chunks_and_reserves_distinct_completion_inode(){
  let(root,owner,request)=super::super::tests::fixture("discard-native-plan").await;let mut creator=owner.clone().prepare_local_cover(owner.original.clone(),request.clone()).await.unwrap();creator.append(&owner.original,0,&[1,2,3]).unwrap();drop(creator);
  let dir=root.join(".starlane/local-cover-producers/11111111-1111-4111-8111-111111111111");let intent=std::fs::read(dir.join("intent.json")).unwrap();let chunk=std::fs::read(dir.join("chunk-00000000.json")).unwrap();prepare(owner.clone(),request.clone()).await.unwrap();
  let plan:Value=serde_json::from_slice(&std::fs::read(dir.join("discard-plan.json")).unwrap()).unwrap();let completion=std::fs::File::open(dir.join("discard-completion.json")).unwrap();assert_eq!(serde_json::to_value(stamp(&completion).unwrap()).unwrap(),plan["completion_initial_stamp"]);assert_eq!(completion.metadata().unwrap().len(),0);assert_eq!(plan["payload_sha256"],sha(&[1,2,3]));
  let pool=sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3"))).await.unwrap();let raw:String=sqlx::query_scalar("SELECT plan_json FROM native_local_cover_discard_intents").fetch_one(&pool).await.unwrap();assert_eq!(serde_json::from_str::<Value>(&raw).unwrap(),plan);assert!(sqlx::query("DELETE FROM native_local_cover_discard_intents").execute(&pool).await.is_err());let winners:i64=sqlx::query_scalar("SELECT COUNT(*) FROM native_local_cover_discard_receipts").fetch_one(&pool).await.unwrap();assert_eq!(winners,0);pool.close().await;
  assert_eq!(std::fs::read(dir.join("intent.json")).unwrap(),intent);assert_eq!(std::fs::read(dir.join("chunk-00000000.json")).unwrap(),chunk);assert_eq!(std::fs::read(dir.join("payload")).unwrap(),[1,2,3]);
  // A prepared but unbound native plan is retained for explicit review. It is
  // neither an erase grant nor a silent restart/reset of the original operation.
  assert!(prepare(owner,request).await.is_err());assert_eq!(std::fs::read(dir.join("payload")).unwrap(),[1,2,3]);std::fs::remove_dir_all(root).unwrap();
 }
 #[tokio::test]#[ignore="fresh owned pending-discard process worker"]async fn fresh_discard_worker(){
  let path=std::path::PathBuf::from(std::env::var("STARLANE_LOCAL_COVER_DISCARD_TEST_CONFIG").unwrap());let config:Value=serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();let root=std::path::PathBuf::from(config["root"].as_str().unwrap());let request:Request=serde_json::from_value(config["request"].clone()).unwrap();let current=request.original_binding["context"].clone();let revision=request.original_binding["revision"].as_str().unwrap();let selection=vault::SnapshotRequestBinding::capture_at(root.with_extension("settings.json"),root.clone(),"none").unwrap();let owner=Owner::capture(selection,"none",revision,current.clone()).unwrap();let phase=config["phase"].as_str().unwrap().to_string();
  if phase=="produce_wait"{let mut creator=owner.prepare_local_cover(current.clone(),request).await.unwrap();for index in 0..3{creator=creator.append_owned(current.clone(),index,vec![7;65536]).await.unwrap().0;}creator.append(&current,3,&vec![7;65536]).unwrap();std::fs::write(path.with_extension("ready"),b"physical4-sql3-original-producer").unwrap();loop{std::thread::sleep(std::time::Duration::from_millis(50));}}
  if phase=="prepare_wait"{prepare(owner,request).await.unwrap();std::fs::write(path.with_extension("ready"),b"original-plan-sql-durable").unwrap();loop{std::thread::sleep(std::time::Duration::from_millis(50));}}
  let receipt=connection::supervise(async move{let _guard=snapshot_commit_lock().lock().await;let mut db=connection::Database::prepare(owner.database.clone(),&owner.main).await?;let physical=db.fence_owner();let result=async{let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;
   let receipt=if phase=="observe"{reconcile_completed_in(owner,&current,request,&mut tx).await?}else{let plan=reopen_prepared_in(owner,&current,request,&mut tx).await?;erase_prepared_in(&mut tx,plan,&current).await?};physical.check()?;
   if phase=="complete_wait_before_sql"{std::fs::write(path.with_extension("ready"),b"native-complete-sql-uncommitted").map_err(err)?;loop{std::thread::sleep(std::time::Duration::from_millis(50));}}
   tx.commit().await.map_err(err)?;Ok::<_,String>(receipt)}.await;db.retire().await?;result}).await.unwrap();assert_eq!(receipt["state"],"discarded_original_payload");println!("ACTUAL_FRESH_ORIGINAL_DISCARD_PROCESS_PASS");
 }
 #[tokio::test]async fn real_sigkill_after_prepared_plan_and_after_native_completion_reconciles_same_original(){
  use std::{io::Write,os::unix::fs::PermissionsExt,os::unix::process::ExitStatusExt,time::{Duration,Instant}};
  for lose_completion_sql in [false,true]{
   let(root,owner,mut request)=super::super::tests::fixture(if lose_completion_sql{"discard-kill-complete"}else{"discard-kill-plan"}).await;request.byte_length=30*1024*1024;drop(owner);
   let config=root.with_extension("discard-worker.json");let ready=config.with_extension("ready");let output=root.with_extension("discard-worker.log");let worker="local_database::attachment_download_owner::native_archive_capture::local_cover::discard::tests::fresh_discard_worker";
   let write_config=|phase:&str|{let mut file=std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(&config).unwrap();std::fs::set_permissions(&config,std::fs::Permissions::from_mode(0o600)).unwrap();file.write_all(serde_json::to_string(&json!({"root":root,"request":request,"phase":phase})).unwrap().as_bytes()).unwrap();file.sync_all().unwrap();};
   let command=||{let mut c=std::process::Command::new(std::env::current_exe().unwrap());c.args([worker,"--exact","--ignored","--nocapture"]).env("STARLANE_LOCAL_COVER_DISCARD_TEST_CONFIG",&config).env("RUST_TEST_THREADS","1");c};
   let kill_checkpoint=|phase:&str|{write_config(phase);let _=std::fs::remove_file(&ready);let file=std::fs::OpenOptions::new().create(true).append(true).open(&output).unwrap();let mut child=command().stdout(file.try_clone().unwrap()).stderr(file).spawn().unwrap();let deadline=Instant::now()+Duration::from_secs(20);while !ready.exists()&&Instant::now()<deadline{if let Some(status)=child.try_wait().unwrap(){panic!("Owned child exited before checkpoint {phase}: {status}; {}",std::fs::read_to_string(&output).unwrap());}std::thread::sleep(Duration::from_millis(10));}if !ready.exists(){child.kill().unwrap();child.wait().unwrap();panic!("No owned discard checkpoint: {}",std::fs::read_to_string(&output).unwrap());}child.kill().unwrap();assert_eq!(child.wait().unwrap().signal(),Some(9));};
   kill_checkpoint("produce_wait");let dir=root.join(".starlane/local-cover-producers/11111111-1111-4111-8111-111111111111");assert_eq!(std::fs::read(dir.join("payload")).unwrap(),vec![7;4*65536]);let sql=sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3"))).await.unwrap();let acknowledged:i64=sqlx::query_scalar("SELECT COUNT(*) FROM native_local_cover_chunks").fetch_one(&sql).await.unwrap();assert_eq!(acknowledged,3);sql.close().await;
   kill_checkpoint("prepare_wait");assert_eq!(std::fs::read(dir.join("payload")).unwrap(),vec![7;4*65536]);
   if lose_completion_sql{kill_checkpoint("complete_wait_before_sql");assert!(!dir.join("payload").exists());let completion:Value=serde_json::from_slice(&std::fs::read(dir.join("discard-completion.json")).unwrap()).unwrap();assert_eq!(completion["state"],"discarded_original_payload");}
   else{write_config("erase");let result=command().output().unwrap();assert!(result.status.success(),"{} {}",String::from_utf8_lossy(&result.stdout),String::from_utf8_lossy(&result.stderr));}
   let original_plan=std::fs::read(dir.join("discard-plan.json")).unwrap();let original_completion=std::fs::read(dir.join("discard-completion.json")).unwrap();for _ in 0..2{write_config("observe");let result=command().output().unwrap();assert!(result.status.success(),"{} {}",String::from_utf8_lossy(&result.stdout),String::from_utf8_lossy(&result.stderr));assert!(String::from_utf8_lossy(&result.stdout).contains("ACTUAL_FRESH_ORIGINAL_DISCARD_PROCESS_PASS"));}
   assert!(!dir.join("payload").exists());assert_eq!(std::fs::read(dir.join("discard-plan.json")).unwrap(),original_plan);assert_eq!(std::fs::read(dir.join("discard-completion.json")).unwrap(),original_completion);assert!(dir.join("intent.json").is_file()&&dir.join("chunk-00000000.json").is_file()&&dir.join("chunk-00000003.json").is_file());
   if lose_completion_sql{let named=dir.join("discard-completion.json");let held=root.join("held-original-completion");std::fs::rename(&named,&held).unwrap();std::fs::write(&named,&original_completion).unwrap();std::fs::set_permissions(&named,std::fs::Permissions::from_mode(0o600)).unwrap();write_config("observe");let result=command().output().unwrap();assert!(!result.status.success(),"Equal-byte foreign completion inode must refuse historical observation");assert_eq!(std::fs::read(&held).unwrap(),original_completion);assert_eq!(std::fs::read(&named).unwrap(),original_completion);}
   println!("ACTUAL_ORIGINAL_DISCARD_CRASH_REPORT {}",json!({"physical_chunks_before":4,"acknowledged_chunks_before":3,"original_request_byte_length":request.byte_length,"disposed_payload_bytes":4*65536,"native_complete_sql_ack_lost":lose_completion_sql,"real_kill_signal":9,"fresh_exact_reconciliations":2,"original_operation_id":request.operation_id,"native_plan_sha256":sha(&original_plan),"native_completion_sha256":sha(&original_completion),"original_chunks_retained":4}));std::fs::remove_dir_all(root).unwrap();std::fs::remove_file(config).unwrap();std::fs::remove_file(ready).unwrap();std::fs::remove_file(output).unwrap();
  }
 }
 #[tokio::test]async fn exact_committed_prepared_plan_reopens_original_idle_inode_and_discards_without_new_operation(){
  let(root,owner,request)=super::super::tests::fixture("discard-reopen-original").await;let mut creator=owner.clone().prepare_local_cover(owner.original.clone(),request.clone()).await.unwrap();creator.append(&owner.original,0,&[1,2,3]).unwrap();drop(creator);prepare(owner.clone(),request.clone()).await.unwrap();
  let current=owner.original.clone();let receipt=connection::supervise(async move{let _guard=snapshot_commit_lock().lock().await;let mut db=connection::Database::prepare(owner.database.clone(),&owner.main).await?;let physical=db.fence_owner();let result=async{let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;let plan=reopen_prepared_in(owner,&current,request,&mut tx).await?;let result=erase_prepared_in(&mut tx,plan,&current).await?;physical.check()?;tx.commit().await.map_err(err)?;Ok::<_,String>(result)}.await;db.retire().await?;result}).await.unwrap();
  assert_eq!(receipt["operation_id"],"11111111-1111-4111-8111-111111111111");assert!(!root.join(".starlane/local-cover-producers/11111111-1111-4111-8111-111111111111/payload").exists());std::fs::remove_dir_all(root).unwrap();
 }
 #[tokio::test]async fn late_original_session_reference_and_foreign_completion_refuse_before_unlink(){
  use std::os::unix::fs::PermissionsExt;
  for fault in ["session","reference","completion"]{
   let(root,owner,request)=super::super::tests::fixture(&format!("discard-late-{fault}")).await;let mut creator=owner.clone().prepare_local_cover(owner.original.clone(),request.clone()).await.unwrap();creator.append(&owner.original,0,&[1,2,3]).unwrap();drop(creator);let current=owner.original.clone();let fixture_root=root.clone();
   let failure=connection::supervise(async move{let _guard=snapshot_commit_lock().lock().await;let mut db=connection::Database::prepare(owner.database.clone(),&owner.main).await?;let physical=db.fence_owner();let result=async{let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;let held=capture_in(owner,&current,request,&mut tx).await?;let plan=DiscardPlan::prepare(held,&current)?;bind_plan_in(&mut tx,&plan,&current).await?;tx.commit().await.map_err(err)?;
    if fault=="completion"{let named=plan.held.dir.path.join("discard-completion.json");std::fs::rename(&named,fixture_root.join("original-completion")).map_err(err)?;std::fs::write(&named,[]).map_err(err)?;std::fs::set_permissions(&named,std::fs::Permissions::from_mode(0o600)).map_err(err)?;}
    let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;
    if fault=="reference"{let mut snapshot=read_snapshot_from_connection(&mut tx,"none").await?.unwrap();snapshot["drafts"][0]["cover_image"]=json!("starlane-local-attachment://11111111-1111-4111-8111-111111111111");crate::local_database::write_snapshot_in_tx(&mut tx,"none",&snapshot).await?;}
    let mut selected=current.clone();if fault=="session"{selected["sessionId"]=json!("foreign");}
    let error=erase_prepared_in(&mut tx,plan,&selected).await.unwrap_err();physical.check()?;tx.rollback().await.map_err(err)?;Ok::<_,String>(error)}.await;db.retire().await?;result}).await.unwrap();assert!(!failure.is_empty());assert_eq!(std::fs::read(root.join(".starlane/local-cover-producers/11111111-1111-4111-8111-111111111111/payload")).unwrap(),[1,2,3]);std::fs::remove_dir_all(root).unwrap();
  }
 }
 #[tokio::test]async fn actual_owned_partial_payload_erasure_preserves_original_metadata_and_commits_separate_receipt(){
  let(root,owner,request)=super::super::tests::fixture("discard-actual-erase").await;let mut creator=owner.clone().prepare_local_cover(owner.original.clone(),request.clone()).await.unwrap();creator.append(&owner.original,0,&[1,2,3]).unwrap();drop(creator);let current=owner.original.clone();
  let receipt=connection::supervise(async move{let _guard=snapshot_commit_lock().lock().await;let mut db=connection::Database::prepare(owner.database.clone(),&owner.main).await?;let physical=db.fence_owner();let result=async{let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;let held=capture_in(owner,&current,request,&mut tx).await?;let plan=DiscardPlan::prepare(held,&current)?;bind_plan_in(&mut tx,&plan,&current).await?;physical.check()?;tx.commit().await.map_err(err)?;
   let mut tx=db.connection().begin_with("BEGIN IMMEDIATE").await.map_err(err)?;let result=erase_prepared_in(&mut tx,plan,&current).await?;physical.check()?;tx.commit().await.map_err(err)?;Ok::<_,String>(result)}.await;db.retire().await?;result}).await.unwrap();
  let dir=root.join(".starlane/local-cover-producers/11111111-1111-4111-8111-111111111111");assert!(!dir.join("payload").exists());assert!(dir.join("intent.json").is_file());assert!(dir.join("chunk-00000000.json").is_file());let actual=std::fs::read(dir.join("discard-completion.json")).unwrap();assert_eq!(actual,receipt["native_completion_json"].as_str().unwrap().as_bytes());let body:Value=serde_json::from_slice(&actual).unwrap();assert_eq!(body["payload_after"]["links"],0);
  let pool=sqlx::sqlite::SqlitePoolOptions::new().max_connections(1).connect_with(sqlx::sqlite::SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3"))).await.unwrap();let stored:String=sqlx::query_scalar("SELECT receipt_json FROM native_local_cover_discard_receipts").fetch_one(&pool).await.unwrap();assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(),receipt);assert!(sqlx::query("DELETE FROM native_local_cover_discard_receipts").execute(&pool).await.is_err());let original:i64=sqlx::query_scalar("SELECT COUNT(*) FROM native_local_cover_intents").fetch_one(&pool).await.unwrap();assert_eq!(original,1);pool.close().await;std::fs::remove_dir_all(root).unwrap();
 }
 #[tokio::test]async fn exact_original_unacknowledged_tail_can_be_observed_without_resume_or_erasure(){
  let(root,owner,request)=super::super::tests::fixture("discard-held-tail").await;let mut creator=owner.clone().prepare_local_cover(owner.original.clone(),request.clone()).await.unwrap();creator.append(&owner.original,0,&[1,2,3]).unwrap();assert!(observe(owner.clone(),request.clone()).await.is_err());drop(creator);
  assert!(owner.clone().resume_local_cover(owner.original.clone(),request.clone()).await.is_err());assert_eq!(observe(owner,request).await.unwrap(),3);assert_eq!(std::fs::read(root.join(".starlane/local-cover-producers/11111111-1111-4111-8111-111111111111/payload")).unwrap(),[1,2,3]);std::fs::remove_dir_all(root).unwrap();
 }
 #[tokio::test]async fn pending_discard_foreign_inode_alias_and_extra_member_are_not_owned(){
  use std::os::unix::fs::PermissionsExt;
  let(root,owner,request)=super::super::tests::fixture("discard-refusal").await;let mut creator=owner.clone().prepare_local_cover(owner.original.clone(),request.clone()).await.unwrap();creator.append(&owner.original,0,&[1,2,3]).unwrap();drop(creator);let dir=root.join(".starlane/local-cover-producers/11111111-1111-4111-8111-111111111111");let payload=dir.join("payload");let original=root.join("held-original");std::fs::rename(&payload,&original).unwrap();std::fs::write(&payload,[1,2,3]).unwrap();std::fs::set_permissions(&payload,std::fs::Permissions::from_mode(0o600)).unwrap();assert!(observe(owner.clone(),request.clone()).await.is_err());assert_eq!(std::fs::read(&original).unwrap(),[1,2,3]);std::fs::remove_file(&payload).unwrap();std::fs::rename(&original,&payload).unwrap();
  let alias=root.join("alias");std::fs::hard_link(&payload,&alias).unwrap();assert!(observe(owner.clone(),request.clone()).await.is_err());std::fs::remove_file(alias).unwrap();std::fs::write(dir.join("unknown"),b"held").unwrap();assert!(observe(owner,request).await.is_err());assert_eq!(std::fs::read(payload).unwrap(),[1,2,3]);std::fs::remove_dir_all(root).unwrap();
 }
}

// Separate archive-only purpose; no original-operation reader or erase grant.
pub(super) mod archive;
