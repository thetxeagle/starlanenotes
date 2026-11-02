
// Read-only copied-committed proof. These checks authenticate original SQL and
// request bytes; they never construct a native admission lease or serving grant.
const PRESERVED_COMMITTED_TABLES:&[&str]=&["attachments","attachment_upload_receipts","asset_replay_receipts","attachment_families","attachment_version_records","attachment_version_changes","attachment_retirements","attachment_retained_byte_ledger","attachment_capacity_reservations","attachment_audience_origins","attachment_audience_bindings"];
fn preserved_cell<'a>(table:&Table,row:&'a [Cell],column:&str)->anyhow::Result<&'a Cell>{
 ensure!(row.len()==table.columns.len(),"Preserved original row width differs");
 let i=table.columns.iter().position(|c|c.name==column).context("Preserved original column absent")?;Ok(&row[i])
}
fn preserved_rows<'a>(tables:&'a BTreeMap<String,Table>,name:&str,column:&str,value:&str)->anyhow::Result<Vec<&'a Vec<Cell>>>{
 let table=tables.get(name).context("Preserved original table absent")?;
 ensure!(table.rows.len()<=100000,"Preserved original table bound exceeded");
 table.rows.iter().filter_map(|row|match preserved_cell(table,row,column){Ok(Cell::Text(v))if v==value.as_bytes()=>Some(Ok(row)),Ok(_)=>None,Err(e)=>Some(Err(e))}).collect()
}
fn preserved_page_incarnation(tables:&BTreeMap<String,Table>,current:&BTreeMap<String,Table>,space:&str,doc:&str)->anyhow::Result<()>{
 let before=tables.get("documents").context("Original preserved documents absent")?;let after=current.get("documents").context("Current preserved documents absent")?;
 ensure!(before.columns==after.columns&&before.keys==after.keys,"Preserved original document shape differs");
 let old=preserved_rows(tables,"documents","id",doc)?;let now=preserved_rows(current,"documents","id",doc)?;
 ensure!(old.len()==1&&now.len()==1&&text(preserved_cell(before,old[0],"space_id")?)?==space&&text(preserved_cell(after,now[0],"space_id")?)?==space&&preserved_cell(before,old[0],"created_at")?==preserved_cell(after,now[0],"created_at")?,"Preserved original page incarnation/association changed");Ok(())
}
fn preserved_tables_match(original:&BTreeMap<String,Table>,current:&BTreeMap<String,Table>)->anyhow::Result<()>{
 for name in PRESERVED_COMMITTED_TABLES{
  if let Some(before)=original.get(*name){let after=current.get(*name).context("Current preserved table absent")?;ensure!(before.columns==after.columns&&before.keys==after.keys&&pg_sorted_rows(&before.rows)?==pg_sorted_rows(&after.rows)?,"Preserved original immutable SQL differs: table={name}");}
 }Ok(())
}
fn preserved_replay_receipt(producer:&crate::storage::attachment_upload::admission::Producer,reservation:&crate::storage::restoration::admission::CapturedReservation,tables:&BTreeMap<String,Table>)->anyhow::Result<Option<crate::sync::AssetReplayReceipt>>{
 use crate::storage::attachment_upload::admission::Producer;
 ensure!(reservation.state=="committed","Preservation requires committed reservation");
 let Producer::Replay{provider,incoming_id,actor_id,space_id,document_id,asset_id,payload_fingerprint,binary_sha256,byte_length,draft_scope}=producer else{return Ok(None)};
 let receipts=preserved_rows(tables,"asset_replay_receipts","incoming_id",incoming_id)?;
 let table=tables.get("asset_replay_receipts").context("Original Replay receipt table absent")?;
 let matches=receipts.into_iter().filter_map(|row|match preserved_cell(table,row,"provider").and_then(text){Ok(p)if p==provider=>Some(Ok(row)),Ok(_)=>None,Err(e)=>Some(Err(e))}).collect::<anyhow::Result<Vec<_>>>()?;
 ensure!(matches.len()==1,"Original Replay receipt missing or ambiguous");
 let raw=text(preserved_cell(table,matches[0],"receipt_json")?)?;
 let r:crate::sync::AssetReplayReceipt=serde_json::from_str(raw)?;
 ensure!(serde_json::to_string(&r)?==raw&&r.incoming.provider==*provider&&r.incoming.operation_id==*incoming_id&&r.incoming.entity_type=="asset"&&matches!(r.incoming.action.as_str(),"create"|"update")&&r.incoming.entity_id==*asset_id&&r.incoming.payload_fingerprint==*payload_fingerprint&&r.actor_id==*actor_id&&r.space_id==*space_id&&r.document_id==*document_id&&r.binary_sha256==*binary_sha256&&r.byte_length>=0&&r.byte_length as u64==*byte_length&&r.draft_scope==*draft_scope&&r.admission_attempt_id.as_deref()==Some(reservation.operation_id.as_str())&&reservation.committed_asset_id.as_deref()==Some(asset_id.as_str())&&reservation.committed_bytes==Some(r.byte_length),"Original committed Replay request/receipt/reservation differs");
 let ledgers=preserved_rows(tables,"attachment_retained_byte_ledger","asset_id",asset_id)?;ensure!(ledgers.len()==1,"Original Replay accounting row missing or ambiguous");
 let ledger=tables.get("attachment_retained_byte_ledger").context("Original Replay ledger absent")?;let row=ledgers[0];let receipt_sha=digest(raw.as_bytes());
 for(column,expected)in[("evidence_kind","replay"),("original_receipt_json",raw),("original_receipt_sha256",receipt_sha.as_str()),("original_storage_path",r.storage_path.as_str()),("filename",r.filename.as_str()),("original_name",r.original_name.as_str()),("mime_type",r.mime_type.as_str()),("uploader_id",r.actor_id.as_str()),("original_created_at",r.created_at.as_str())]{
  ensure!(text(preserved_cell(ledger,row,column)?)?==expected,"Original Replay ledger metadata/receipt differs: {column}");
 }
 ensure!(preserved_cell(ledger,row,"byte_length")?==&Cell::Integer(r.byte_length),"Original Replay ledger length differs");
 Ok(Some(r))
}
async fn preserved_producer_current(conn:&mut sqlx::AnyConnection,producer:&crate::storage::attachment_upload::admission::Producer,reservation:&crate::storage::restoration::admission::CapturedReservation,original:&BTreeMap<String,Table>,current:&BTreeMap<String,Table>)->anyhow::Result<Option<crate::sync::AssetReplayReceipt>>{
 let(space,doc)=match producer{crate::storage::attachment_upload::admission::Producer::Direct{request}=>(&request.space_id,&request.document_id),crate::storage::attachment_upload::admission::Producer::Replay{space_id,document_id,..}=>(space_id,document_id)};
 preserved_page_incarnation(original,current,space,doc)?;
 let replay=preserved_replay_receipt(producer,reservation,original)?;
 if let Some(r)=&replay{ensure!(crate::sync::asset_receipt_current(conn,r).await.map_err(anyhow::Error::msg)?,"Current original Replay scalar/raw receipt parity differs");}
 Ok(replay)
}
fn preserved_replay_blob(root:&Root,raw:&[u8],claim:&[u8],r:&crate::sync::AssetReplayReceipt,parent:&serde_json::Value,payload:&serde_json::Value)->anyhow::Result<()>{
 let origin:serde_json::Value=serde_json::from_slice(raw)?;
 ensure!(origin["version"]==1&&origin["claim_sha256"]==digest(claim)&&origin["path"]==r.storage_path&&origin["expected_length"].as_u64()==Some(r.byte_length as u64)&&origin["expected_sha256"]==r.binary_sha256&&origin["parent"]==*parent&&origin["payload"]==*payload,"Original Replay blob role/custody differs");
 let held=root.leaf(&r.storage_path)?;let stamp=physical_stamp(&held)?;ensure!(stamp.length==r.byte_length as u64&&planned_hash(&held)?.sha256==r.binary_sha256&&physical_stamp(&held)?==stamp,"Preserved Replay original payload bytes changed");Ok(())
}
// Original Transfer source preservation only. No WriterLease, role, read, cleanup,
// absence, or released-charge constructor is exposed by this token.
struct OriginalTransferAdmissionPreservation {
 proof:Arc<EngineAdmission>,active:Arc<super::Active>,
 rows:Vec<crate::storage::restoration::admission::CapturedReservation>,
 history:(Vec<HistorySqlRow>,Vec<(i64,String)>),
 original_tables:BTreeMap<String,Table>,original_manifest:Vec<u8>,
 namespace:File,namespace_stamp:PhysicalStamp,capsules:Vec<super::PreservedAdmissionCapsule>,
}
impl OriginalTransferAdmissionPreservation {
 async fn select(proof:&Arc<EngineAdmission>,conn:&mut sqlx::AnyConnection)->anyhow::Result<Option<Self>> {
  proof.verify_any_connection(conn).await?;
  let(_,active)=super::registered(proof.path())?;let active=active.context("Transfer preservation lacks original registered activation")?;active.check()?;
  let original=match active.engine.as_deref(){Some(super::EngineAuthority::Transfer(original))=>original,_=>anyhow::bail!("Transfer preservation requires original activated Transfer authority")};
  original.verify_any_connection(conn).await?;proof.verify_any_connection(conn).await?;
  ensure!(original.path()==proof.path()&&identity(original.root.fd())?==identity(proof.root.fd())?&&original.raw_envelope==proof.raw_envelope&&original.records.keys().eq(proof.records.keys()),"Transfer preservation current root/generation proof differs");
  // Both opaque native proofs independently fence every immutable engine/history
  // descriptor and the same actual SQL target relation. No AnyPool-pointer claim.
  original.physical_fence()?;proof.physical_fence()?;
  let rows=super::prior_capture_rows(conn).await?;
  let original_manifest_path=format!(".engine-transfer-custody/{}/original-manifest.json",proof.envelope.transfer_id);
  let held=proof.records.get(&original_manifest_path).context("Transfer original manifest role absent")?;
  let raw=read_ledger(&held.held)?;ensure!(digest(&raw)==held.bytes.sha256,"Transfer original manifest changed");
  let manifest:crate::storage::backup_inventory::Manifest=serde_json::from_slice(&raw)?;
  let original_ids=manifest.files.0.keys().filter_map(|p|p.strip_prefix(".attachment-upload-admission/").and_then(|s|s.split_once('/')).map(|(id,_)|id.to_owned())).collect::<std::collections::BTreeSet<_>>();
  // Class selection comes from the authenticated original source inventory,
  // never mutable current claim identity or a failed native constructor.
  if original_ids.is_empty(){return Ok(None)}
  let database=proof.records.get(&format!(".engine-transfer-custody/{}/original.sqlite",proof.envelope.transfer_id)).context("Original authenticated SQLite role absent")?;
  proof.physical_fence()?;let native=NativeDb::open(&database.held)?;
  let existing=native.rows("SELECT name FROM sqlite_master WHERE type='table'")?.into_iter().map(|row|text(&row[0]).map(str::to_owned)).collect::<anyhow::Result<std::collections::BTreeSet<_>>>()?;
  let mut original_tables=shape_for_profile(pg_profile_any(conn).await?)?;
  original_tables.retain(|name,_|name=="documents"||PRESERVED_COMMITTED_TABLES.contains(&name.as_str()));
  original_tables.retain(|name,_|existing.contains(name));
  let mut total=0usize;
  for(name,table)in &mut original_tables {
   let columns=table.columns.iter().map(|c|quoted(&c.name)).collect::<anyhow::Result<Vec<_>>>()?.join(",");
   table.rows=native.rows(&format!("SELECT {columns} FROM {}",quoted(name)?))?;
   for row in &table.rows{total=total.checked_add(serde_json::to_vec(row)?.len()).context("Original preservation projection overflow")?;ensure!(total<=512*1024*1024,"Original preservation projection exceeds incumbent512MiB bound");}
  }
  native.close()?;proof.physical_fence()?;
  ensure!(original_ids.len()<=512,"Original Transfer preserved capsule inventory exceeds bound");
  for row in &rows {let raw=read_ledger(&active.root.leaf(&format!(".attachment-upload-admission/{}/claim.json",row.operation_id))?)?;row.matches_claim(&raw)?;}
  ensure!(active.ledger.version==1&&active.ledger.admission.is_none()&&active.ledger.completed_absence.is_none()&&active.version2,"Original Transfer preservation requires Ledger1 without owner roles");
  let history=history_rows(conn).await?;let source=HistorySource::new_with_check(&active.root,None,engine_root_fence)?;let closure=history_closure(source,identity(active.root.fd())?,Some((&history.0,&history.1)))?;
  ensure!(closure.current_id==active.ledger.generation_id&&digest(&closure.generations[&closure.current_id].raw)==active.ledger_sha256,"Transfer preservation history differs");closure.source.fence()?;
  // Transfer's actual finalized engine admission, not a fabricated finalized
  // restoration Envelope3, authenticates this distinct original source class.
  proof.verify_any_connection(conn).await?;
  if pg_profile_any(conn).await?.has_erasure() {let completed:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachment_erasure_dispositions").fetch_one(&mut *conn).await?;ensure!(completed==0,"Transfer preservation does not certify completed absence");}
  let ns=active.root.directory(".attachment-upload-admission")?;let ns_stamp=physical_stamp(&ns)?;ensure!(ns_stamp.mode&0o777==0o700&&ns.metadata()?.uid()==unsafe{libc::geteuid()},"Transfer admission namespace not private");
  let ids=rows.iter().map(|r|r.operation_id.clone()).collect::<std::collections::BTreeSet<_>>();ensure!(ids.len()==rows.len()&&super::prior_capsule_names(&ns)?==ids,"Transfer complete capsule/SQL inventory differs");
  ensure!(original_ids==ids,"Transfer preservation only covers complete original copied capsules");
  let mut capsules=Vec::new();let mut total=0usize;
  for row in &rows {
   ensure!(row.state=="committed","Transfer preservation currently supports copied committed producer scopes only");
   let base=format!(".attachment-upload-admission/{}",row.operation_id);let directory=active.root.directory(&base)?;
   if unsafe{libc::flock(directory.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}<0{return Err(io::Error::last_os_error().into())}
   let stamp=physical_stamp(&directory)?;ensure!(stamp.mode&0o777==0o700&&directory.metadata()?.uid()==unsafe{libc::geteuid()},"Transfer preserved capsule not private");
   let expected_names=manifest.files.0.keys().filter_map(|p|p.strip_prefix(&(base.clone()+"/")).map(str::to_owned)).collect::<std::collections::BTreeSet<_>>();let names=super::prior_capsule_names(&directory)?;ensure!(names==expected_names&&names.len()<=32,"Transfer preserved capsule role inventory differs");
   let original_claim=read_ledger(&active.root.leaf(&format!("{base}/claim.json"))?)?;row.matches_claim(&original_claim)?;let claim:serde_json::Value=serde_json::from_slice(&original_claim)?;
   ensure!(claim["root"]==serde_json::to_value(&manifest.source_custody.root)?&&claim["namespace"]==serde_json::to_value(manifest.source_custody.directories.0.get(".attachment-upload-admission").context("Original admission namespace custody absent")?)?&&claim["capsule"]==serde_json::to_value(manifest.source_custody.directories.0.get(&base).context("Original capsule custody absent")?)?,"Transfer copied original claim custody differs");
   let mut records=BTreeMap::new();
   for name in names {let path=format!("{base}/{name}");let file=active.root.leaf(&path)?;let before=physical_stamp(&file)?;let expected=manifest.files.0.get(&path).context("Original copied record absent from authenticated manifest")?;let old=manifest.source_custody.files.0.get(&path).context("Original copied record custody absent")?;
    ensure!(before.length<=8*1024*1024&&before.mode&0o777==0o600&&before.links==1&&old.links==1&&file.metadata()?.uid()==unsafe{libc::geteuid()},"Transfer preserved record bound/private topology differs");let bytes=read_ledger(&file)?;total=total.checked_add(bytes.len()).context("Transfer preservation bytes overflow")?;ensure!(total<=16*1024*1024,"Transfer preservation records exceed bound");ensure!(physical_stamp(&file)?==before&&bytes.len()as u64==expected.size_bytes&&digest(&bytes)==expected.sha256,"Transfer copied record differs from original source manifest");
    crate::storage::attachment_upload::admission::validate_preserved_capsule_record(&name,&bytes,&original_claim)?;records.insert(name,(file,before,bytes));
   }
   capsules.push(super::PreservedAdmissionCapsule{attempt:row.operation_id.clone(),directory,stamp,records});
  }
  let token=Self{proof:proof.clone(),active,rows,history,original_tables,original_manifest:raw,namespace:ns,namespace_stamp:ns_stamp,capsules};token.recheck(conn).await?;Ok(Some(token))
 }
 fn fence(&self)->io::Result<()> {
  self.proof.physical_fence()?;self.active.check()?;let ns=self.active.root.directory(".attachment-upload-admission")?;
  if physical_stamp(&ns)?!=self.namespace_stamp||physical_stamp(&self.namespace)?!=self.namespace_stamp||super::prior_capsule_names(&ns)?!=self.rows.iter().map(|r|r.operation_id.clone()).collect(){return Err(fail("Transfer preserved complete capsule inventory changed"))}
  for c in &self.capsules {let base=format!(".attachment-upload-admission/{}",c.attempt);let fresh=self.active.root.directory(&base)?;if physical_stamp(&fresh)?!=c.stamp||physical_stamp(&c.directory)?!=c.stamp||super::prior_capsule_names(&fresh)?!=c.records.keys().cloned().collect(){return Err(fail("Transfer preserved capsule topology changed"))}for(name,(held,stamp,raw))in &c.records {let now=self.active.root.leaf(&format!("{base}/{name}"))?;if physical_stamp(held)?!=*stamp||physical_stamp(&now)?!=*stamp||read_ledger(held)?!=*raw||read_ledger(&now)?!=*raw{return Err(fail("Transfer preserved original record changed"))}}}
  self.proof.physical_fence()?;self.active.check()
 }
 async fn recheck(&self,conn:&mut sqlx::AnyConnection)->anyhow::Result<()> {
  self.fence()?;self.proof.verify_any_connection(conn).await?;
  ensure!(super::prior_capture_rows(conn).await?==self.rows&&history_rows(conn).await?==self.history,"Transfer complete preservation SQL changed");
  let profile=pg_profile_any(conn).await?;let current=pg_tables_any(conn,&shape_for_profile(profile)?).await?;
  preserved_tables_match(&self.original_tables,&current)?;
  let manifest:crate::storage::backup_inventory::Manifest=serde_json::from_slice(&self.original_manifest)?;
  for row in &self.rows {
   let capsule=self.capsules.iter().find(|c|c.attempt==row.operation_id).context("Preserved original capsule absent")?;
   let raw=&capsule.records.get("claim.json").context("Preserved claim absent")?.2;
   let claim:serde_json::Value=serde_json::from_slice(raw)?;let producer=serde_json::from_value(claim["producer"].clone())?;
   if let Some(replay)=preserved_producer_current(conn,&producer,row,&self.original_tables,&current).await?{
    let origin=&capsule.records.get("replay-blob-origin.json").context("Original Replay blob role absent")?.2;
    let parent=replay.storage_path.rsplit_once('/').context("Original Replay blob path invalid")?.0;
    let directory=manifest.source_custody.directories.0.get(parent).context("Original Replay parent custody absent")?;
    let payload=manifest.source_custody.files.0.get(&replay.storage_path).context("Original Replay payload custody absent")?;
    preserved_replay_blob(&self.active.root,origin,raw,&replay,&serde_json::to_value(directory)?,&serde_json::json!({"device":payload.device,"inode":payload.inode}))?;
   }
  }
  self.fence()?;Ok(())
 }
}

#[cfg(test)]
async fn transfer_preservation_test_attachment_rows(pool:&sqlx::AnyPool,pg:bool)->anyhow::Result<serde_json::Value>{
 use sqlx::Row;let mut result=serde_json::Map::new();for table in ["attachments","attachment_upload_receipts","attachment_families","attachment_version_records","attachment_version_changes","attachment_retirements","attachment_retained_byte_ledger"]{let columns:Vec<String>=if pg{sqlx::query_scalar("SELECT CAST(column_name AS TEXT) FROM information_schema.columns WHERE table_schema=current_schema() AND table_name=$1 ORDER BY column_name").bind(table).fetch_all(pool).await?}else{sqlx::query_scalar("SELECT name FROM pragma_table_info($1) ORDER BY name").bind(table).fetch_all(pool).await?};ensure!(!columns.is_empty(),"Original attachment table shape missing");let query=format!("SELECT {} FROM {table}",columns.iter().map(|name|format!("CAST(\"{name}\" AS TEXT)")).collect::<Vec<_>>().join(","));let rows=sqlx::query(&query).fetch_all(pool).await?;let mut values=Vec::new();for row in rows{values.push((0..columns.len()).map(|i|row.try_get::<Option<String>,_>(i)).collect::<Result<Vec<_>,_>>()?);}values.sort();result.insert(table.into(),serde_json::json!({"columns":columns,"rows":values}));}Ok(serde_json::Value::Object(result))
}
#[cfg(test)]
pub(crate) async fn pg_completed_test_original_transfer_preservation(source:&Path,original:&crate::storage::attachment_upload::Receipt)->anyhow::Result<()> {
 use sqlx::Connection;
 let url=std::env::var("REPLAY_CAPACITY_TEST_PG_URL")?;let parent=source.parent().context("Owned fixture parent required")?;
 let bundle_path=parent.join(format!("original-transfer-preservation-seed-{}",uuid::Uuid::new_v4().simple()));let bundle=super::capture_sqlite_certified(source,&bundle_path,"1.5.3",true).await?;ensure!(bundle.manifest.admission_owner_capture.is_none(),"Committed source omitted native readonly role as required");let original_rows=transfer_preservation_test_attachment_rows(&bundle.db,false).await?;ensure!(original_rows["attachments"]["rows"].as_array().is_some_and(|r|r.len()==1),"Genuine committed source row required");bundle.db.close().await;drop(bundle);
 let target=parent.join(format!("original-transfer-preservation-root-{}",uuid::Uuid::new_v4().simple()));let transfer=parent.join(format!("original-transfer-preservation-import-{}",uuid::Uuid::new_v4().simple()));crate::storage::backup_transfer::run(&bundle_path,&target,&transfer,"1.5.3",&url).await?;
 let serving=super::open_engine_serving(&target,&url).await?;let attempt=&original.intent.attempt_id;ensure!(crate::storage::attachment_upload::admission::WriterLease::open_idle(&target,attempt).is_err(),"Copied original became native writer");
 ensure!(transfer_preservation_test_attachment_rows(&serving.pool,true).await?==original_rows,"Transfer changed original full attachment/version/receipt rows");
 let original_claim=std::fs::read(target.join(format!(".attachment-upload-admission/{attempt}/claim.json")))?;let original_bytes=std::fs::read(source.join(&original.intent.storage_path))?;ensure!(std::fs::read(target.join(&original.intent.storage_path))?==original_bytes,"Transfer lost genuine registered source bytes");
 let mut conn=serving.pool.acquire().await?;serving.proof.verify_any_connection(&mut conn).await?;let mut tx=conn.begin().await?;
 // Physical exclusion remains separate from native ownership.
 let directory=serving.proof.root.directory(&format!(".attachment-upload-admission/{attempt}"))?;ensure!(unsafe{libc::flock(directory.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}==0,"Owned copied capsule exclusion failed");ensure!(OriginalTransferAdmissionPreservation::select(&serving.proof,&mut tx).await.is_err(),"Active capsule admitted capture");drop(directory);
 let held=OriginalTransferAdmissionPreservation::select(&serving.proof,&mut tx).await?.context("Genuine original Transfer copied class unselected")?;held.recheck(&mut tx).await?;
 let mut foreign=sqlx::AnyConnection::connect(&url).await?;ensure!(held.recheck(&mut foreign).await.is_err(),"Foreign connected SQL admitted original source token");foreign.close().await?;
 sqlx::query("UPDATE attachment_capacity_reservations SET maximum_bytes=maximum_bytes+1 WHERE operation_id=$1").bind(attempt).execute(&mut *tx).await?;ensure!(held.recheck(&mut tx).await.is_err(),"Changed original reservation admitted preservation");tx.rollback().await?;drop(held);drop(conn);
 let backup_path=parent.join(format!("original-transfer-preservation-backup-{}",uuid::Uuid::new_v4().simple()));let backup=capture_owned_postgres(&serving,&backup_path,true).await?;backup.fence()?;let manifest:serde_json::Value=serde_json::from_slice(&backup.manifest)?;ensure!(["admission_owner_capture","prior_admission_owner_capture","completed_absence_capture","prior_completed_absence_capture","mixed_completed_absence_capture"].iter().all(|field|manifest.get(field).is_none()),"Preservation minted transport ownership");
 ensure!(std::fs::read(target.join(format!(".attachment-upload-admission/{attempt}/claim.json")))?==original_claim,"Original copied claim rewritten");let raw:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&original.intent.request.operation_id).fetch_one(&serving.pool).await?;ensure!(serde_json::from_str::<serde_json::Value>(&raw)?==serde_json::to_value(original)?,"Original receipt changed");
 let cold=parent.join(format!("original-transfer-preservation-cold-{}",uuid::Uuid::new_v4().simple()));restore_qualified_postgres(&backup_path,&cold,&url,true).await?;let restored=super::open_restored_pg_serving(&cold,&url).await?;ensure!(std::fs::read(cold.join(&original.intent.storage_path))?==original_bytes,"Cold target retained bytes differ");ensure!(std::fs::read(cold.join(format!(".attachment-upload-admission/{attempt}/claim.json")))?==original_claim,"Cold target changed original claim");ensure!(crate::storage::attachment_upload::admission::WriterLease::open_idle(&cold,attempt).is_err(),"Cold preserved scope became native writer");ensure!(transfer_preservation_test_attachment_rows(&restored.pool,true).await?==original_rows,"Cold capture/restore changed original raw attachment/version/receipt rows");std::env::set_var("DATA_DIR",&cold);let report=crate::storage::attachment_capacity::usage_report(&restored.pool).await?;ensure!((report.retained_bytes,report.logical_retained_bytes,report.pending_bytes)==(original_bytes.len()as i64,original_bytes.len()as i64,0),"Preservation changed retained charge");let app_state=crate::AppState{db:restored.pool.clone(),jwt_secret:"owned-erasure-fixture".into(),base_url:"http://localhost".into(),search_index:crate::search::SearchIndex::new(cold.join("search_index"))?,oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};let token=crate::auth::jwt::create_token_pair_for_session("owner","owner@test.invalid","member",&app_state.jwt_secret,Some("owner-session"),30)?.access_token;let router=crate::storage::attachment_versions::router().merge(axum::Router::new().nest("/api/v1",crate::spaces::attachments::router())).with_state(app_state);let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await?;let address=listener.local_addr()?;let task=tokio::spawn(async move{axum::serve(listener,router).await.unwrap()});let client=reqwest::Client::new();let endpoint=format!("http://{address}/spaces/space/documents/winner/attachment-versions/{}",original.intent.asset_id);let response=client.get(&endpoint).bearer_auth(&token).send().await?;ensure!(response.status()==200,"Cold preserved original current version route refused: {}",response.status());let pin:serde_json::Value=response.json().await?;ensure!(pin["version"]["version_id"]==original.intent.asset_id&&pin["current_storage_path"]==original.intent.storage_path,"Cold current pin/operation association differs");let file_endpoint=format!("http://{address}/api/v1/files/{}",original.intent.storage_path.split('/').map(|p|urlencoding::encode(p).into_owned()).collect::<Vec<_>>().join("/"));let file=client.get(&file_endpoint).bearer_auth(&token).send().await?;ensure!(file.status()==200,"Cold current registered file route refused: {}",file.status());ensure!(file.bytes().await?.as_ref()==original_bytes.as_slice(),"Cold current file bytes differ");ensure!(!client.get(&endpoint).send().await?.status().is_success()&&!client.get(&file_endpoint).send().await?.status().is_success(),"Anonymous cold version/file read admitted");task.abort();let _=task.await;println!("ACTUAL_COLD_TRANSFER_PRESERVED_COMMITTED_ORIGINAL_CURRENT_AUTH_VERSION_BYTES_ANONYMOUS_DENY_PASS");restored.pool.close().await;drop(restored);

 // Separate fresh Transfer namespace: a self-consistent claim/SQL identity rewrite
 // cannot select native ownership or bypass the authenticated original inventory.
 let forged_target=parent.join(format!("original-transfer-preservation-forged-{}",uuid::Uuid::new_v4().simple()));let forged_import=parent.join(format!("original-transfer-preservation-forged-import-{}",uuid::Uuid::new_v4().simple()));crate::storage::backup_transfer::run(&bundle_path,&forged_target,&forged_import,"1.5.3",&url).await?;let forged=super::open_engine_serving(&forged_target,&url).await?;let claim_path=forged_target.join(format!(".attachment-upload-admission/{attempt}/claim.json"));let original_forged=std::fs::read(&claim_path)?;std::fs::write(parent.join(format!("forged-control-original-claim-{}.json",uuid::Uuid::new_v4().simple())),&original_forged)?;let mut claim:serde_json::Value=serde_json::from_slice(&original_forged)?;let ns=forged.proof.root.directory(".attachment-upload-admission")?;let capsule=forged.proof.root.directory(&format!(".attachment-upload-admission/{attempt}"))?;let root_id=identity(forged.proof.root.fd())?;let ns_id=identity(&ns)?;let cap_id=identity(&capsule)?;claim["root"]=serde_json::json!({"device":root_id.0,"inode":root_id.1});claim["namespace"]=serde_json::json!({"device":ns_id.0,"inode":ns_id.1});claim["capsule"]=serde_json::json!({"device":cap_id.0,"inode":cap_id.1});let raw=serde_json::to_vec(&claim)?;std::fs::write(&claim_path,&raw)?;let mut forged_conn=forged.pool.acquire().await?;let mut forged_tx=forged_conn.begin().await?;sqlx::query("UPDATE attachment_capacity_reservations SET admission_claim_sha256=$1 WHERE operation_id=$2").bind(digest(&raw)).bind(attempt).execute(&mut *forged_tx).await?;let error=match OriginalTransferAdmissionPreservation::select(&forged.proof,&mut forged_tx).await {Ok(_)=>anyhow::bail!("Self-consistent copied claim/SQL identity rewrite admitted"),Err(e)=>e};ensure!(error.to_string().contains("Transfer copied original claim custody differs"),"Original immutable inventory did not reject rebinding at the expected custody boundary: {error}");forged_tx.rollback().await?;drop(forged_conn);ensure!(std::fs::read(forged_target.join(&original.intent.storage_path))?==original_bytes,"Forged refusal changed retained bytes");forged.pool.close().await;drop(forged);println!("ACTUAL_TRANSFER_SELF_CONSISTENT_COPIED_CLAIM_SQL_NATIVE_ID_REBIND_REFUSAL_NO_FALLBACK_PASS");
 // A held source cannot accept an equal-byte foreign inode or rename-back.
 let mut conn=serving.pool.acquire().await?;let held=OriginalTransferAdmissionPreservation::select(&serving.proof,&mut conn).await?.context("Original source token absent")?;let path=target.join(format!(".attachment-upload-admission/{attempt}/claim.json"));let kept=parent.join(format!("claim-original-preserved-{}",uuid::Uuid::new_v4().simple()));std::fs::rename(&path,&kept)?;use std::os::unix::fs::PermissionsExt;std::fs::write(&path,&original_claim)?;std::fs::set_permissions(&path,std::fs::Permissions::from_mode(0o600))?;ensure!(held.fence().is_err(),"Equal-byte foreign held claim inode accepted");let foreign_kept=parent.join(format!("claim-foreign-preserved-{}",uuid::Uuid::new_v4().simple()));std::fs::rename(&path,&foreign_kept)?;std::fs::rename(&kept,&path)?;ensure!(held.fence().is_err(),"Rename-back laundered held original claim custody");drop(held);drop(conn);serving.pool.close().await;drop(serving);
 println!("ACTUAL_ORIGINAL_REGISTERED_COMMITTED_DIRECT_SQLITE_TRANSFER_PG_STANDARD_CAPTURE_COLD_RETAINED_BYTES_ROWS_CHARGE_NO_ROLE_OR_WRITER_GRANT_ACTIVE_FOREIGN_SQL_RESERVATION_EQUAL_INODE_RENAMEBACK_REFUSALS_PASS");Ok(())
}

#[cfg(test)]
mod copied_committed_replay_proof_tests{
 use super::*;
 fn put(table:&mut Table,values:&[(&str,Cell)]){let mut row=vec![Cell::Null;table.columns.len()];for(name,value)in values{let i=table.columns.iter().position(|c|c.name==*name).unwrap();row[i]=value.clone();}table.rows.push(row);}
 fn t(s:&str)->Cell{Cell::Text(s.as_bytes().to_vec())}
 fn fixture()->(crate::storage::attachment_upload::admission::Producer,crate::storage::restoration::admission::CapturedReservation,BTreeMap<String,Table>){
  let scope=crate::storage::attachment_upload::DraftUploadScope{schema_version:2,draft_id:"aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),draft_created_at:"exact-private-birth".into(),copy_source:None};
  let r=crate::sync::AssetReplayReceipt{incoming:crate::storage::canonical_rewrite::IncomingIdentity{provider:"original-provider".into(),operation_id:"incoming".into(),entity_type:"asset".into(),entity_id:"opaque-asset".into(),action:"create".into(),payload_fingerprint:"a".repeat(64)},actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),storage_path:"spaces/slug/attachments/page/blob.bin".into(),filename:"blob.bin".into(),original_name:"Original.bin".into(),mime_type:"application/octet-stream".into(),created_at:"original-time".into(),byte_length:4,binary_sha256:"b".repeat(64),source_kind:"base64".into(),source_id:None,source_revision:None,body_sha256:None,admission_attempt_id:Some("11111111-1111-4111-8111-111111111111".into()),draft_binding:None,draft_scope:Some(scope.clone())};
  let p=crate::storage::attachment_upload::admission::Producer::Replay{provider:r.incoming.provider.clone(),incoming_id:r.incoming.operation_id.clone(),actor_id:r.actor_id.clone(),space_id:r.space_id.clone(),document_id:r.document_id.clone(),asset_id:r.incoming.entity_id.clone(),payload_fingerprint:r.incoming.payload_fingerprint.clone(),binary_sha256:r.binary_sha256.clone(),byte_length:4,draft_scope:Some(scope)};
  let row=crate::storage::restoration::admission::CapturedReservation{operation_id:r.admission_attempt_id.clone().unwrap(),request_sha256:"c".repeat(64),admission_claim_sha256:"d".repeat(64),maximum_bytes:8,state:"committed".into(),committed_asset_id:Some(r.incoming.entity_id.clone()),committed_bytes:Some(4),disposition_json:None,disposition_sha256:None};
  let raw=serde_json::to_string(&r).unwrap();let mut tables=shape_for_profile(EngineCatalogueProfile::CURRENT).unwrap();
  put(tables.get_mut("asset_replay_receipts").unwrap(),&[("provider",t(&r.incoming.provider)),("incoming_id",t(&r.incoming.operation_id)),("receipt_json",t(&raw))]);
  put(tables.get_mut("attachment_retained_byte_ledger").unwrap(),&[("asset_id",t(&r.incoming.entity_id)),("evidence_kind",t("replay")),("original_receipt_json",t(&raw)),("original_receipt_sha256",t(&digest(raw.as_bytes()))),("original_storage_path",t(&r.storage_path)),("filename",t(&r.filename)),("original_name",t(&r.original_name)),("mime_type",t(&r.mime_type)),("uploader_id",t(&r.actor_id)),("original_created_at",t(&r.created_at)),("byte_length",Cell::Integer(r.byte_length))]);
  put(tables.get_mut("documents").unwrap(),&[("id",t("page")),("space_id",t("space")),("created_at",t("original-page-birth"))]);
  (p,row,tables)
 }
 #[test]fn copied_replay_requires_exact_original_request_scope_receipt_and_ledger(){
  let(p,row,tables)=fixture();assert!(preserved_replay_receipt(&p,&row,&tables).unwrap().is_some());
  let mut changed=p.clone();let crate::storage::attachment_upload::admission::Producer::Replay{draft_scope,..}=&mut changed else{unreachable!()};draft_scope.as_mut().unwrap().draft_created_at="replacement-private-birth".into();assert!(preserved_replay_receipt(&changed,&row,&tables).is_err());
  let mut changed=p.clone();let crate::storage::attachment_upload::admission::Producer::Replay{provider,..}=&mut changed else{unreachable!()};*provider="foreign-provider".into();assert!(preserved_replay_receipt(&changed,&row,&tables).is_err());
  let mut changed=row.clone();changed.committed_bytes=Some(3);assert!(preserved_replay_receipt(&p,&changed,&tables).is_err());
  let mut changed=tables.clone();let ledger=changed.get_mut("attachment_retained_byte_ledger").unwrap();let column=ledger.columns.iter().position(|c|c.name=="original_receipt_sha256").unwrap();ledger.rows[0][column]=t(&"f".repeat(64));assert!(preserved_replay_receipt(&p,&row,&changed).is_err());
  let mut changed=tables.clone();let table=changed.get_mut("asset_replay_receipts").unwrap();let column=table.columns.iter().position(|c|c.name=="receipt_json").unwrap();let raw=text(&table.rows[0][column]).unwrap().to_owned();table.rows[0][column]=t(&format!(" {raw}"));assert!(preserved_replay_receipt(&p,&row,&changed).is_err());
 }
 #[test]fn copied_replay_page_incarnation_and_original_sql_cannot_be_rebased(){
  let(_,_,original)=fixture();preserved_page_incarnation(&original,&original,"space","page").unwrap();preserved_tables_match(&original,&original).unwrap();
  let mut changed=original.clone();let table=changed.get_mut("documents").unwrap();let column=table.columns.iter().position(|c|c.name=="created_at").unwrap();table.rows[0][column]=t("recreated");assert!(preserved_page_incarnation(&original,&changed,"space","page").is_err());
  let mut changed=original.clone();changed.get_mut("asset_replay_receipts").unwrap().rows.clear();assert!(preserved_tables_match(&original,&changed).is_err());
 }
}

#[cfg(test)]
enum CopiedReplayTestOwner {
 Transfer(OriginalTransferAdmissionPreservation),
 Restored(OriginalRestoredAdmissionPreservation),
}
#[cfg(test)]
impl CopiedReplayTestOwner {
 async fn select(serving:&QualifiedPgServing,conn:&mut sqlx::AnyConnection)->anyhow::Result<Self>{
  match serving{
   QualifiedPgServing::Transfer(s)=>Ok(Self::Transfer(OriginalTransferAdmissionPreservation::select(&s.proof,conn).await?.context("Genuine copied Transfer producer owner unselected")?)),
   QualifiedPgServing::PgRestore(s)=>Ok(Self::Restored(OriginalRestoredAdmissionPreservation::select(&s.proof,conn).await?.context("Genuine copied restored producer owner unselected")?)),
  }
 }
 async fn recheck(&self,conn:&mut sqlx::AnyConnection)->anyhow::Result<()>{
  match self{Self::Transfer(owner)=>owner.recheck(conn).await,Self::Restored(owner)=>owner.recheck(conn).await}
 }
}
#[cfg(test)]
async fn copied_replay_test_selected(conn:&mut sqlx::AnyConnection)->anyhow::Result<crate::sync::AssetReplayReceipt>{
 let raws:Vec<String>=sqlx::query_scalar("SELECT receipt_json FROM asset_replay_receipts ORDER BY provider,incoming_id LIMIT 100001").fetch_all(conn).await?;
 ensure!(raws.len()<=100000,"Genuine Replay test inventory exceeds existing bound");
 for raw in raws{let r:crate::sync::AssetReplayReceipt=serde_json::from_str(&raw)?;if r.admission_attempt_id.is_some(){return Ok(r)}}
 bail!("Actual committed Replay receipt required; fixture must not invent one")
}
/// Run before any final physical mutation. Every negative SQL change is rolled
/// back and exact original typed application rows are checked afterward.
#[cfg(test)]
pub(crate) async fn pg_copied_replay_sql_negative_controls(serving:&QualifiedPgServing)->anyhow::Result<()>{
 use sqlx::Connection;
 serving.fence().await?;
 let mut conn=serving.pool().acquire().await?;
 let profile=pg_profile_any(&mut conn).await?;let shape=shape_for_profile(profile)?;
 let original=pg_tables_any(&mut conn,&shape).await?;
 let replay=copied_replay_test_selected(&mut conn).await?;
 let positive=CopiedReplayTestOwner::select(serving,&mut conn).await?;positive.recheck(&mut conn).await?;drop(positive);
 for negative in ["raw_receipt","page_incarnation","ledger_receipt_hash"]{
  let mut tx=conn.begin().await?;
  let held=CopiedReplayTestOwner::select(serving,&mut tx).await?;held.recheck(&mut tx).await?;
  let affected=match negative{
   "raw_receipt"=>sqlx::query("UPDATE asset_replay_receipts SET receipt_json=receipt_json||' ' WHERE provider=$1 AND incoming_id=$2").bind(&replay.incoming.provider).bind(&replay.incoming.operation_id).execute(&mut *tx).await?.rows_affected(),
   "page_incarnation"=>sqlx::query("UPDATE documents SET created_at=created_at||'-foreign-incarnation' WHERE id=$1 AND space_id=$2").bind(&replay.document_id).bind(&replay.space_id).execute(&mut *tx).await?.rows_affected(),
   "ledger_receipt_hash"=>sqlx::query("UPDATE attachment_retained_byte_ledger SET original_receipt_sha256=CASE WHEN original_receipt_sha256=$1 THEN $2 ELSE $1 END WHERE asset_id=$3").bind("a".repeat(64)).bind("b".repeat(64)).bind(&replay.incoming.entity_id).execute(&mut *tx).await?.rows_affected(),
   _=>unreachable!(),
  };
  ensure!(affected==1,"Actual independent negative must alter exactly one original row");
  let refused=held.recheck(&mut tx).await.is_err();drop(held);tx.rollback().await?;
  ensure!(refused,"Genuine copied original accepted independently changed {negative}");
  ensure!(pg_tables_any(&mut conn,&shape).await?==original,"Negative rollback did not preserve exact original SQL");
  let positive=CopiedReplayTestOwner::select(serving,&mut conn).await?;positive.recheck(&mut conn).await?;drop(positive);
 }
 serving.fence().await?;
 println!("GENUINE_COPIED_REPLAY_ORIGINAL_RAW_RECEIPT_PAGE_INCARNATION_LEDGER_HASH_INDEPENDENT_REFUSALS_EXACT_SQL_ROLLBACK_PASS");Ok(())
}
/// MUST run last (or on a dedicated disposable target). Returning the original
/// path cannot erase ctime/history evidence; the selected held owner stays denied.
#[cfg(test)]
pub(crate) async fn pg_copied_replay_final_physical_negative(serving:&QualifiedPgServing,evidence:&Path)->anyhow::Result<()>{
 use std::os::unix::fs::{DirBuilderExt,PermissionsExt,MetadataExt};
 ensure!(evidence.is_absolute(),"Explicit outside-target evidence directory required");
 let root=match serving{QualifiedPgServing::Transfer(s)=>&s.proof.root,QualifiedPgServing::PgRestore(s)=>&s.proof.root};
 ensure!(!evidence.starts_with(&root.absolute),"Preserve negative originals outside selected target");
 std::fs::DirBuilder::new().mode(0o700).create(evidence)?;
 ensure!(std::fs::metadata(evidence)?.dev()==root.fd().metadata()?.dev(),"Physical inode negative evidence must use the same owned filesystem");
 let mut conn=serving.pool().acquire().await?;
 let replay=copied_replay_test_selected(&mut conn).await?;
 let attempt=replay.admission_attempt_id.as_deref().context("Genuine Replay attempt absent")?;
 let held=CopiedReplayTestOwner::select(serving,&mut conn).await?;held.recheck(&mut conn).await?;
 let path=root.absolute.join(format!(".attachment-upload-admission/{attempt}/replay-blob-origin.json"));
 let original=std::fs::read(&path)?;let original_identity=identity(&root.leaf(&format!(".attachment-upload-admission/{attempt}/replay-blob-origin.json"))?)?;
 let kept=evidence.join("original-replay-blob-origin.json");let foreign=evidence.join("foreign-replay-blob-origin.json");
 std::fs::rename(&path,&kept)?;
 let write_result=(||->anyhow::Result<()>{use std::io::Write;let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(&path)?;f.set_permissions(std::fs::Permissions::from_mode(0o600))?;f.write_all(&original)?;f.sync_all()?;Ok(())})();
 if let Err(error)=write_result{if path.exists(){std::fs::rename(&path,&foreign)?;}std::fs::rename(&kept,&path)?;return Err(error)}
 let swapped_identity=identity(&root.leaf(&format!(".attachment-upload-admission/{attempt}/replay-blob-origin.json"))?)?;
 let refused=held.recheck(&mut conn).await.is_err();
 std::fs::rename(&path,&foreign)?;std::fs::rename(&kept,&path)?;
 // Restore the actual original inode to its path; retain a distinct forensic
 // bytes copy outside the target as well as the replaced foreign inode.
 std::fs::write(&kept,&original)?;std::fs::set_permissions(&kept,std::fs::Permissions::from_mode(0o600))?;
 ensure!(original_identity!=swapped_identity,"Foreign equal-byte test must replace the actual inode");
 ensure!(refused,"Genuine copied original accepted equal-byte foreign Replay role inode");
 ensure!(std::fs::read(&path)?==original&&std::fs::read(&foreign)?==original&&std::fs::read(&kept)?==original,"Physical negative must preserve original and foreign equal bytes");
 ensure!(held.recheck(&mut conn).await.is_err(),"Rename-back laundered held original Replay role history");
 println!("GENUINE_COPIED_REPLAY_EQUAL_BYTES_FOREIGN_ROLE_AND_RENAMEBACK_REFUSAL_ORIGINAL_AND_FOREIGN_BYTES_PRESERVED_PASS");Ok(())
}
