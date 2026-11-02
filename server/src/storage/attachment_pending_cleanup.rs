//! Deliberate original-human cleanup of idle original interrupted producers.
//! Current SQL and native ownership remain held together; no age/PID inference.
use super::*;
use super::super::*;
use serde::{Serialize,Deserialize};
#[path="attachment_pending_references.rs"] mod pending_references;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PendingRequest{producer_sha256:String,claim_sha256:String}
#[derive(Debug,Serialize)]
pub(crate) struct PendingOutcome{attempt_id:String,outcome:&'static str,reason:Option<&'static str>,disposition_sha256:Option<String>}
fn review()->AppError{AppError::Conflict("Original interrupted custody requires retained review".into())}
fn clock(auth:&crate::auth::middleware::AuthUser,expiry:chrono::DateTime<chrono::FixedOffset>)->io::Result<()>{
 crate::spaces::access::principal_from_auth(auth).map_err(|_|fail("Current original token unavailable"))?;
 if expiry<=chrono::Utc::now(){return Err(fail("Current original session expired"))}Ok(())
}
async fn authority(held:&mut FencedTransaction,auth:&crate::auth::middleware::AuthUser,space:&str,document:&str)->Result<chrono::DateTime<chrono::FixedOffset>,AppError>{
 held.original_human(auth,&auth.claims.sub,space).await?.check_now()?;
 // This initial bounded adapter supports live original pages only. An absent
 // page/incarnation needs independently captured pending-original witness.
 crate::spaces::attachments::check_attachment_page(&mut held.tx,auth,space,document,crate::spaces::access::PageOperation::Edit).await?;
 let proof=held.original_human(auth,&auth.claims.sub,space).await?;proof.check_now()?;Ok(proof.session_expiry)
}
enum PlanKind{Direct(Disposal),Replay(ReplayDisposal)}
impl PlanKind{
 fn record(&self)->(&'static str,&'static str){match self{Self::Direct(_)=>("disposal-intent.json","disposal-tombstone.json"),Self::Replay(_)=>("replay-disposal-intent.json","replay-disposal-tombstone.json")}}
 fn selection(&self)->Result<(Option<String>,Option<String>,Option<(String,i64)>),AppError>{match self{
  Self::Direct(p)=>{let i=p.envelopes.get("intent.json").map(|raw|decode::<crate::storage::attachment_upload::Intent>(raw.as_bytes())).transpose().map_err(|_|review())?;Ok((i.as_ref().map(|i|i.asset_id.clone()),i.as_ref().map(|i|i.storage_path.clone()),p.installed_path.as_ref().zip(i.as_ref()).map(|(path,i)|(path.clone(),i.byte_length))))},
  Self::Replay(p)=>{let origin=p.origin_json.as_deref().map(|raw|decode::<ReplayBlobOrigin>(raw.as_bytes())).transpose().map_err(|_|review())?;Ok((None,origin.as_ref().map(|o|o.path.clone()),origin.map(|o|(o.path,p.payload_length as i64))))}
 }}
 async fn census(&self,conn:&mut sqlx::AnyConnection,lease:&WriterLease,paths:std::collections::BTreeMap<String,(String,i64)>,finalized:std::collections::BTreeSet<String>,roster:&census::NativeRoster)->io::Result<census::FamilyCensus>{match self{Self::Direct(p)=>census::FamilyCensus::capture_pending_direct(lease,p,paths,finalized,roster),Self::Replay(p)=>census::FamilyCensus::capture_pending_replay(conn,lease,p,paths,finalized,roster).await}}
}
async fn step(pool:&sqlx::AnyPool,auth:&crate::auth::middleware::AuthUser,space:&str,document:&str,attempt:&str,request:&PendingRequest)->Result<Option<String>,AppError>{
 let mut held=FencedTransaction::begin(pool).await?;authority(&mut held,auth,space,document).await?;
 let root=crate::storage::data_dir();let lease=WriterLease::open_idle(&root,attempt).map_err(|_|review())?;
 let(actor,original_space,original_document)=match lease.producer(){Producer::Direct{request}=>(request.actor_id.as_str(),request.space_id.as_str(),request.document_id.as_str()),Producer::Replay{actor_id,space_id,document_id,..}=>(actor_id.as_str(),space_id.as_str(),document_id.as_str())};
 if actor!=auth.claims.sub||original_space!=space||original_document!=document{return Err(AppError::Forbidden("Original interrupted producer belongs to another authority".into()))}
 if request.producer_sha256!=lease.producer_sha256()||request.claim_sha256!=lease.claim_sha256(){return Err(review())}
 let changed=sqlx::query("UPDATE attachment_capacity_lock SET revision=revision+1 WHERE id=1 AND revision<9223372036854775807").execute(&mut *held.tx).await.map_err(AppError::internal)?.rows_affected();if changed!=1{return Err(review())}
 let row:(String,String,i64,String,Option<String>,Option<String>)=sqlx::query_as("SELECT request_sha256,admission_claim_sha256,maximum_bytes,state,disposition_json,disposition_sha256 FROM attachment_capacity_reservations WHERE operation_id=$1").bind(attempt).fetch_one(&mut *held.tx).await.map_err(AppError::internal)?;
 if row.0!=lease.producer_sha256()||row.1!=lease.claim_sha256()||row.2!=lease.maximum_bytes() as i64||!matches!(row.3.as_str(),"reserved"|"disposed"){return Err(review())}
 for sql in ["SELECT COUNT(*) FROM attachment_upload_receipts WHERE attempt_id=$1","SELECT COUNT(*) FROM attachment_version_records WHERE upload_attempt_id=$1"]{if sqlx::query_scalar::<_,i64>(sql).bind(attempt).fetch_one(&mut *held.tx).await.map_err(AppError::internal)?!=0{return Err(review())}}
 let intent_name=match lease.producer(){Producer::Direct{..}=>"disposal-intent.json",Producer::Replay{..}=>"replay-disposal-intent.json"};
 let existing=optional_bytes(&lease.0.directory,intent_name).map_err(|_|review())?;
 let plan=match (lease.producer(),existing.as_deref()){
  (Producer::Direct{..},Some(raw))=>PlanKind::Direct(decode(raw).map_err(|_|review())?),
  (Producer::Replay{..},Some(raw))=>PlanKind::Replay(decode(raw).map_err(|_|review())?),
  (Producer::Direct{..},None)=>PlanKind::Direct(lease.plan_direct().map_err(|_|review())?),
  (Producer::Replay{..},None)=>PlanKind::Replay(lease.plan_replay(&mut held.tx).await.map_err(|_|review())?),
 };
 let raw=if let Some(raw)=existing{raw}else{match &plan{PlanKind::Direct(p)=>json(p),PlanKind::Replay(p)=>json(p)}.map_err(|_|review())?};
 let(mut asset,path,installed)=plan.selection()?;if let Producer::Replay{asset_id,..}=lease.producer(){asset=Some(asset_id.clone())}
 if let Some(path)=&path{for sql in ["SELECT COUNT(*) FROM attachments WHERE storage_path=$1","SELECT COUNT(*) FROM attachment_version_records WHERE original_storage_path=$1","SELECT COUNT(*) FROM move_asset_path_aliases WHERE old_path=$1"]{if sqlx::query_scalar::<_,i64>(sql).bind(path).fetch_one(&mut *held.tx).await.map_err(AppError::internal)?!=0{return Err(review())}}}
 let finalized=pending_references::prove(&mut held.tx,asset.as_deref(),path.as_deref()).await?;
 let mut paths=crate::storage::attachment_capacity::retained_paths_in(&mut held.tx).await.map_err(|_|review())?;
 if let Some((path,length))=installed{let id=asset.clone().ok_or_else(review)?;if let Some(old)=paths.insert(path,(id.clone(),length)){if old!=(id,length){return Err(review())}}}
 let qualified=crate::storage::restoration::qualified_erasure_source_in(&root,&mut held.tx).await.map_err(|_|review())?;
 let roster=census::NativeRoster::capture_qualified(&root,&[lease.clone()],qualified).map_err(|_|review())?;roster.fence_in(&mut held.tx).await.map_err(|_|review())?;
 let observed=plan.census(&mut held.tx,&lease,paths.clone(),finalized.clone(),&roster).await.map_err(|_|review())?;
 let expiry=authority(&mut held,auth,space,document).await?;observed.fence().map_err(|_|review())?;clock(auth,expiry).map_err(|_|review())?;
 if optional_bytes(&lease.0.directory,intent_name).map_err(|_|review())?.is_none(){stage::write_envelope(&lease.0.directory,intent_name,&raw).map_err(|_|review())?;}
 drop(observed);let observed=plan.census(&mut held.tx,&lease,paths.clone(),finalized.clone(),&roster).await.map_err(|_|review())?;
 let tombstone=plan.record().1;
 if row.3=="disposed"{if optional_bytes(&lease.0.directory,tombstone).map_err(|_|review())?.as_deref()!=Some(raw.as_slice())||row.4.as_deref()!=std::str::from_utf8(&raw).ok()||row.5.as_deref()!=Some(digest(&raw).as_str()){return Err(review())}}
 // The held user/session rows and global policy guards remain in this same
 // transaction while awaited Replay classifications run. Clock and full native
 // roster/family observations run immediately before each one-path mutation.
 #[cfg(test)] before_step_test_gate(attempt).await;
 let mut before=||{roster.fence()?;observed.fence()?;clock(auth,expiry)};
 let completed=match &plan{
  PlanKind::Direct(p)=>{if row.3=="disposed"{let pending=lease.stage_directory().map_err(|_|review())?.is_some_and(|(_,d)|optional_file(&d,"payload.part").ok().flatten().is_some())||p.installed_path.as_ref().is_some_and(|path|scoped_file(&lease.0.root,path).is_ok());if pending{return Err(review())}}lease.complete_direct_fenced(p,true,&mut before).map_err(|_|review())?},
  PlanKind::Replay(p)=>{let present=path.as_ref().is_some_and(|path|scoped_file(&lease.0.root,path).is_ok());if row.3=="disposed"&&present{return Err(review())}lease.complete_replay_fenced(&mut held.tx,p,&mut before).await.map_err(|_|review())?;!present}
 };
 if !completed{held.tx.rollback().await.map_err(AppError::internal)?;return Ok(None)}
 before().map_err(|_|review())?;
 if let Some(old)=optional_bytes(&lease.0.directory,tombstone).map_err(|_|review())?{if old!=raw{return Err(review())}}else{stage::write_envelope(&lease.0.directory,tombstone,&raw).map_err(|_|review())?;}
 drop(before);drop(observed);let completed_census=plan.census(&mut held.tx,&lease,paths,finalized,&roster).await.map_err(|_|review())?;completed_census.fence().map_err(|_|review())?;roster.fence().map_err(|_|review())?;
 roster.fence_in(&mut held.tx).await.map_err(|_|review())?; lease.verify().map_err(|_|review())?;authority(&mut held,auth,space,document).await?;clock(auth,expiry).map_err(|_|review())?;
 if row.3=="reserved"{let changed=sqlx::query("UPDATE attachment_capacity_reservations SET state='disposed',disposition_json=$1,disposition_sha256=$2 WHERE operation_id=$3 AND request_sha256=$4 AND admission_claim_sha256=$5 AND maximum_bytes=$6 AND state='reserved'").bind(std::str::from_utf8(&raw).map_err(|_|review())?).bind(digest(&raw)).bind(attempt).bind(lease.producer_sha256()).bind(lease.claim_sha256()).bind(lease.maximum_bytes() as i64).execute(&mut *held.tx).await.map_err(AppError::internal)?.rows_affected();if changed!=1{return Err(review())}clock(auth,expiry).map_err(|_|review())?;held.tx.commit().await.map_err(AppError::internal)?;}else{held.tx.rollback().await.map_err(AppError::internal)?;}
 Ok(Some(digest(&raw)))
}
pub(crate) async fn cleanup_pending(pool:&sqlx::AnyPool,auth:&crate::auth::middleware::AuthUser,space:&str,document:&str,attempt:&str,request:&PendingRequest)->Result<PendingOutcome,AppError>{
 if uuid::Uuid::parse_str(attempt).ok().is_none_or(|u|u.to_string()!=attempt){return Err(review())}
 let mut result=PendingOutcome{attempt_id:attempt.into(),outcome:"pending",reason:Some("bounded_retry_with_original_attempt"),disposition_sha256:None};
 for _ in 0..3{match step(pool,auth,space,document,attempt,request).await{Ok(Some(sha))=>{result.outcome="physically_completed";result.reason=None;result.disposition_sha256=Some(sha);break},Ok(None)=>(),Err(AppError::Conflict(_))=>{result.outcome="retained_review";result.reason=Some("active_unproved_or_protected_original_custody");break},Err(e)=>return Err(e)}}
 let mut final_held=FencedTransaction::begin(pool).await?;let expiry=authority(&mut final_held,auth,space,document).await?;final_held.tx.rollback().await.map_err(AppError::internal)?;clock(auth,expiry).map_err(|_|AppError::Unauthorized("Original cleanup authority expired before response".into()))?;Ok(result)
}

#[cfg(test)]
type PendingGate=(tokio::sync::oneshot::Sender<()>,tokio::sync::oneshot::Receiver<()>);
#[cfg(test)]
fn gates()->&'static std::sync::Mutex<std::collections::BTreeMap<String,PendingGate>>{static G:std::sync::OnceLock<std::sync::Mutex<std::collections::BTreeMap<String,PendingGate>>>=std::sync::OnceLock::new();G.get_or_init(||std::sync::Mutex::new(std::collections::BTreeMap::new()))}
#[cfg(test)]
pub(super) fn pause_before_step(attempt:&str)->(tokio::sync::oneshot::Receiver<()>,tokio::sync::oneshot::Sender<()>){let(a,b)=tokio::sync::oneshot::channel();let(c,d)=tokio::sync::oneshot::channel();assert!(gates().lock().unwrap().insert(attempt.into(),(a,d)).is_none());(b,c)}
#[cfg(test)]
async fn before_step_test_gate(attempt:&str){let gate=gates().lock().unwrap().remove(attempt);if let Some((a,r))=gate{let _=a.send(());let _=r.await;}}

// Read-only original-custody discovery. A listed hash is an observation, never
// a transferable authority; cleanup reopens and proves the exact original.
#[derive(Debug,Serialize)]
pub(crate) struct PendingAttempt{attempt_id:String,producer_sha256:String,claim_sha256:String,producer_kind:&'static str,original_operation_id:String}
#[derive(Debug,Serialize)]
pub(crate) struct PendingDiscovery{availability:&'static str,reason:Option<&'static str>,attempts:Vec<PendingAttempt>}
impl PendingDiscovery{fn unavailable(reason:&'static str)->Self{Self{availability:"not_available",reason:Some(reason),attempts:vec![]}}}
pub(crate) async fn discover_pending(pool:&sqlx::AnyPool,auth:&crate::auth::middleware::AuthUser,space:&str,document:&str)->Result<PendingDiscovery,AppError>{
 let mut held=FencedTransaction::begin(pool).await?;authority(&mut held,auth,space,document).await?;
 let guarded=sqlx::query("UPDATE attachment_capacity_lock SET revision=revision+1 WHERE id=1 AND revision<9223372036854775807").execute(&mut *held.tx).await.map_err(AppError::internal)?.rows_affected();
 if guarded!=1{return Err(review())}
 let rows:Vec<(String,String,String,i64)>=sqlx::query_as("SELECT operation_id,request_sha256,admission_claim_sha256,maximum_bytes FROM attachment_capacity_reservations WHERE state='reserved' ORDER BY operation_id LIMIT 129").fetch_all(&mut *held.tx).await.map_err(AppError::internal)?;
 let mut result=PendingDiscovery{availability:"available",reason:None,attempts:vec![]};
 let roster=if rows.len()>128{result=PendingDiscovery::unavailable("reserved_attempt_inventory_exceeds_bound");None}else{
  let root=crate::storage::data_dir();
  let captured=match crate::storage::restoration::qualified_erasure_source_in(&root,&mut held.tx).await{
   Ok(qualified)=>census::NativeRoster::capture_qualified(&root,&[],qualified),
   Err(_)=>Err(fail("Qualified pending source custody unavailable")),
  };
  match captured{Ok(roster)=>match roster.fence_in(&mut held.tx).await{Ok(())=>Some(roster),Err(_)=>{result=PendingDiscovery::unavailable("active_or_unproved_native_custody");None}},Err(_)=>{result=PendingDiscovery::unavailable("active_or_unproved_native_custody");None}}
 };
 if let Some(roster)=&roster{for(attempt,producer,claim,maximum)in rows{
  if uuid::Uuid::parse_str(&attempt).ok().is_none_or(|u|u.to_string()!=attempt){result=PendingDiscovery::unavailable("reserved_original_evidence_unavailable");break}
  let lease=match roster.lease(&attempt){Ok(lease)=>lease,Err(_)=>{result=PendingDiscovery::unavailable("reserved_original_evidence_unavailable");break}};
  if producer!=lease.producer_sha256()||claim!=lease.claim_sha256()||i64::try_from(lease.maximum_bytes()).ok()!=Some(maximum){result=PendingDiscovery::unavailable("reserved_original_evidence_differs");break}
  let(actor,original_space,original_document,kind,operation)=match lease.producer(){Producer::Direct{request}=>(request.actor_id.as_str(),request.space_id.as_str(),request.document_id.as_str(),"original_direct",request.operation_id.as_str()),Producer::Replay{actor_id,space_id,document_id,incoming_id,..}=>(actor_id.as_str(),space_id.as_str(),document_id.as_str(),"original_replay",incoming_id.as_str())};
  if actor!=auth.claims.sub||original_space!=space||original_document!=document{continue}
  for sql in ["SELECT COUNT(*) FROM attachment_upload_receipts WHERE attempt_id=$1","SELECT COUNT(*) FROM attachment_version_records WHERE upload_attempt_id=$1"]{if sqlx::query_scalar::<_,i64>(sql).bind(&attempt).fetch_one(&mut *held.tx).await.map_err(AppError::internal)?!=0{result=PendingDiscovery::unavailable("reserved_original_evidence_differs");break}}
  if result.availability!="available"{break}
  result.attempts.push(PendingAttempt{attempt_id:attempt,producer_sha256:producer,claim_sha256:claim,producer_kind:kind,original_operation_id:operation.into()});
 }roster.fence_in(&mut held.tx).await.map_err(|_|review())?;}
 let expiry=authority(&mut held,auth,space,document).await?;if let Some(roster)=&roster{roster.fence_in(&mut held.tx).await.map_err(|_|review())?;}held.tx.rollback().await.map_err(AppError::internal)?;clock(auth,expiry).map_err(|_|AppError::Unauthorized("Original discovery authority expired before response".into()))?;Ok(result)
}

// Recovery-only read observation: the original idle lease and original complete
// roster are mandatory. No decoded intent, caller-selected exclusion or SQL row
// can create this value, and it has no unlink/release/serving method.
pub(crate) struct DisposedPendingSource {
 lease:WriterLease,row:crate::storage::restoration::admission::CapturedReservation,
 plan:PlanKind,finalized:std::collections::BTreeSet<String>,census:census::FamilyCensus,
}
impl DisposedPendingSource {
 pub(crate) async fn recheck_in(&self,conn:&mut sqlx::AnyConnection)->anyhow::Result<()> {
  self.lease.verify()?;self.census.fence_in(conn).await?;
  let row=super::super::accounting::capture::reservation_in(conn,self.lease.attempt_id()).await?;
  anyhow::ensure!(row==self.row,"Disposed source exact original SQL row changed");
  self.lease.observe_native_disposed_in(conn,row.disposition_json.as_deref().ok_or_else(||fail("Disposed source raw intent absent"))?,row.disposition_sha256.as_deref().ok_or_else(||fail("Disposed source digest absent"))?).await?;
  let(asset,path,_)=self.plan.selection().map_err(|e|anyhow::anyhow!("{e:?}"))?;
  let final_refs=pending_references::prove(conn,asset.as_deref(),path.as_deref()).await.map_err(|e|anyhow::anyhow!("{e:?}"))?;
  anyhow::ensure!(final_refs==self.finalized,"Disposed source exact global protected references changed");self.census.fence_in(conn).await?;Ok(())
 }
}
pub(crate) async fn capture_disposed_pending_source_in(lease:&WriterLease,conn:&mut sqlx::AnyConnection,paths:std::collections::BTreeMap<String,(String,i64)>,roster:&census::NativeRoster)->anyhow::Result<DisposedPendingSource>{
 lease.verify()?;roster.fence_in(conn).await?;let original=roster.lease(lease.attempt_id())?;anyhow::ensure!(original.claim_sha256()==lease.claim_sha256()&&original.original_claim()==lease.original_claim(),"Disposed source caller is not exact original held roster lease");
 let row=super::super::accounting::capture::reservation_in(conn,lease.attempt_id()).await?;
 anyhow::ensure!(row.state=="disposed"&&row.committed_asset_id.is_none()&&row.committed_bytes.is_none()&&row.request_sha256==lease.producer_sha256()&&row.admission_claim_sha256==lease.claim_sha256()&&row.maximum_bytes==lease.maximum_bytes()as i64,"Disposed source original row/capsule differs");
 let raw=row.disposition_json.as_deref().ok_or_else(||fail("Disposed source intent absent"))?;let sha=row.disposition_sha256.as_deref().ok_or_else(||fail("Disposed source digest absent"))?;lease.observe_native_disposed_in(conn,raw,sha).await?;
 let plan=match lease.producer(){Producer::Direct{..}=>PlanKind::Direct(decode(raw.as_bytes())?),Producer::Replay{..}=>PlanKind::Replay(decode(raw.as_bytes())?)};
 let(version,attempt,claim,request,claim_sha,maximum,length)=match &plan{PlanKind::Direct(p)=>(p.version,p.attempt_id.as_str(),p.claim_json.as_bytes(),p.request_sha256.as_str(),p.admission_claim_sha256.as_str(),p.maximum_bytes,p.payload_length),PlanKind::Replay(p)=>(p.version,p.attempt_id.as_str(),p.claim_json.as_bytes(),p.request_sha256.as_str(),p.admission_claim_sha256.as_str(),p.maximum_bytes,p.payload_length)};
 anyhow::ensure!(version==1&&attempt==lease.attempt_id()&&claim==lease.original_claim()&&request==lease.producer_sha256()&&claim_sha==lease.claim_sha256()&&maximum==lease.maximum_bytes()as i64&&length<=lease.maximum_bytes(),"Disposed source original typed plan binding differs");
 let(asset,path,_)=plan.selection().map_err(|e|anyhow::anyhow!("{e:?}"))?;let finalized=pending_references::prove(conn,asset.as_deref(),path.as_deref()).await.map_err(|e|anyhow::anyhow!("{e:?}"))?;
 let census=plan.census(conn,lease,paths,finalized.clone(),roster).await?;census.fence_in(conn).await?;
 let result=DisposedPendingSource{lease:lease.clone(),row,plan,finalized,census};result.recheck_in(conn).await?;roster.fence_in(conn).await?;Ok(result)
}

// Pure snapshot protected-reference validation. This returns no ownership token;
// caller must separately prove exact cold record/SQL/full census and generation.
pub(crate) async fn check_disposed_snapshot_references_in(root:&std::path::Path,conn:&mut sqlx::AnyConnection,raw:&str,replay:bool)->anyhow::Result<()> {
 let plan=if replay{PlanKind::Replay(decode(raw.as_bytes())?)}else{PlanKind::Direct(decode(raw.as_bytes())?)};
 let(mut asset,path,_)=plan.selection().map_err(|e|anyhow::anyhow!("{e:?}"))?;
 // Replay entity identity comes from the immutable original typed producer;
 // an absent public blob path must not skip entity-reference inspection.
 if let PlanKind::Replay(p)=&plan {let claim:Claim=decode(p.claim_json.as_bytes())?;let Producer::Replay{asset_id,..}=claim.producer else{anyhow::bail!("Composite Replay disposed producer differs")};asset=Some(asset_id);}
 pending_references::prove_in_at(root,conn,asset.as_deref(),path.as_deref()).await.map_err(|e|anyhow::anyhow!("{e:?}"))?;Ok(())
}
