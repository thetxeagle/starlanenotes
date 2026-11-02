// Historical observation only. These records never grant serving, restoration,
// writer, upload or deletion authority and never certify later source edits.
const SQLITE_RECORD_SCOPE:&str="restored-sqlite-ledger1-ordinary-application-v1";
const SQLITE_RECORD_DOMAIN:&[u8]=b"StarlaneNotes/ordinary-sqlite-application/history/v1\0";
#[derive(Clone,PartialEq,Eq,serde::Serialize,serde::Deserialize)]#[serde(deny_unknown_fields)]
struct SqliteHistorySource {root_device:u64,root_inode:u64,main_device:u64,main_inode:u64,designation_device:u64,designation_inode:u64,designation_sha256:String,generation:Option<String>,ledger_sha256:Option<String>}
async fn sqlite_history_source(root:&Root,database:&File,conn:&mut sqlx::AnyConnection)->anyhow::Result<SqliteHistorySource>{
    root.check()?;let root_id=identity(root.fd())?;let main=identity(database)?;
    ordinary_sqlite_selected_main(root,database,main)?;
    let designation=crate::storage::data_root_designation::Designation::capture_held(&root.absolute,root.fd())?.context("Historical SQLite source has no actual informational designation")?;
    let marker=root.leaf(crate::storage::data_root_designation::NAME)?;
    ensure!(designation.matches_held_marker(crate::storage::data_root_designation::NAME,&marker)?,"Actual informational designation differs");
    let marker_id=identity(&marker)?;let marker_sha=hash_file(&marker,marker.metadata()?.len())?;
    let(rows,pointer)=history_rows(conn).await?;
    let(generation,ledger_sha256)=if exists(root.fd(),NAMESPACE)?{
        let closure=history_closure(HistorySource::new(root,None)?,root_id,Some((&rows,&pointer)))?;
        closure.source.fence()?;(Some(closure.current_id.clone()),Some(digest(&closure.generations[&closure.current_id].raw)))
    }else{ensure!(rows.is_empty()&&pointer.is_empty(),"Source SQL history has no native history owner");(None,None)};
    designation.fence()?;root.check()?;ordinary_sqlite_selected_main(root,database,main)?;
    Ok(SqliteHistorySource{root_device:root_id.0,root_inode:root_id.1,main_device:main.0,main_inode:main.1,designation_device:marker_id.0,designation_inode:marker_id.1,designation_sha256:marker_sha,generation,ledger_sha256})
}
#[derive(serde::Serialize,serde::Deserialize)]#[serde(deny_unknown_fields)]
struct SqliteHistoricalChecks {pages:usize,drafts:usize,registered_assets:usize,legacy_assets:usize,exact_binary_bytes:u64,audiences:usize,history_sha256:String,original_application_sha256:String,observed_application_sha256:String,session_activity_sha256:String,observed_sessions:usize,original_inventory_sha256:String,final_inventory_sha256:String}
#[derive(serde::Serialize,serde::Deserialize)]#[serde(deny_unknown_fields)]
struct SqliteHistoricalPayload {version:u32,scope:String,operation:String,completed_at:String,source:SqliteHistorySource,backup_id:String,backup_manifest_sha256:String,tested_root_device:u64,tested_root_inode:u64,tested_main_device:u64,tested_main_inode:u64,tested_generation:String,tested_ledger_sha256:String,operator:String,normal_elf_sha256:String,checks:SqliteHistoricalChecks,store_device:u64,store_inode:u64,file_device:u64,file_inode:u64}
#[derive(serde::Serialize,serde::Deserialize)]#[serde(deny_unknown_fields)]
struct SqliteSealedHistory {payload:SqliteHistoricalPayload,mac:String}
#[derive(Clone,PartialEq,Eq,serde::Serialize,serde::Deserialize)]#[serde(deny_unknown_fields)]
struct SqliteHistoryJournal {version:u32,operation:String,record_sha256:String,record_device:u64,record_inode:u64,store_device:u64,store_inode:u64}

// COMPLETE is a distinct authenticated phase; unsigned intent is never its ACK.
const SQLITE_HISTORY_COMPLETE_DOMAIN:&[u8]=b"StarlaneNotes/sqlite-application-history/complete/v1\0";
#[derive(serde::Serialize,serde::Deserialize)]#[serde(deny_unknown_fields)]
struct SqliteHistoryCompletionFact{version:u32,phase:String,purpose:String,journal:SqliteHistoryJournal}
#[derive(serde::Serialize,serde::Deserialize)]#[serde(deny_unknown_fields)]
struct SqliteHistoryComplete{fact:SqliteHistoryCompletionFact,mac:String}
fn sqlite_history_complete(journal:&SqliteHistoryJournal,key:&[u8])->anyhow::Result<Vec<u8>>{
 use hmac::Mac;ensure!(key.len()>=32,"Application completion key unavailable");
 let fact=SqliteHistoryCompletionFact{version:1,phase:"complete".into(),purpose:SQLITE_RECORD_SCOPE.into(),journal:journal.clone()};
 let raw=serde_json::to_vec(&fact)?;ensure!(raw.len()<=8192,"Application completion fact exceeds bound");
 let mut mac=hmac::Hmac::<sha2::Sha256>::new_from_slice(key).map_err(|_|fail("Application completion key refused"))?;mac.update(SQLITE_HISTORY_COMPLETE_DOMAIN);mac.update(&raw);
 let complete=serde_json::to_vec(&SqliteHistoryComplete{fact,mac:hex::encode(mac.finalize().into_bytes())})?;ensure!(complete.len()<=8192,"Application completion ACK exceeds bound");Ok(complete)
}
fn sqlite_history_verify_complete(raw:&[u8],journal:&SqliteHistoryJournal,key:&[u8])->anyhow::Result<()>{
 use hmac::Mac;ensure!(key.len()>=32&&raw.len()<=8192,"Application completion key/ACK unavailable");
 let complete:SqliteHistoryComplete=serde_json::from_slice(raw)?;ensure!(serde_json::to_vec(&complete)?==raw,"Application completion ACK is not canonical");
 ensure!(complete.fact.version==1&&complete.fact.phase=="complete"&&complete.fact.purpose==SQLITE_RECORD_SCOPE&&complete.fact.journal==*journal,"Application completion phase/purpose/original journal differs");
 ensure!(complete.mac.len()==64&&complete.mac.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b)),"Application completion MAC malformed");
 let mut mac=hmac::Hmac::<sha2::Sha256>::new_from_slice(key).map_err(|_|fail("Application completion key refused"))?;mac.update(SQLITE_HISTORY_COMPLETE_DOMAIN);mac.update(&serde_json::to_vec(&complete.fact)?);mac.verify_slice(&hex::decode(&complete.mac)?).map_err(|_|fail("Application completion ACK authentication unavailable"))?;Ok(())
}
pub(crate) struct SqliteCompleteOrdinaryEvidence {payload:SqliteHistoricalPayload}
impl SqliteCompleteOrdinaryEvidence {
    pub(crate) async fn after_probes(source:&mut StoppedVerificationSource<'_>,target:&mut SqliteFinalObservedTarget<'_>,projection:&VerificationSqlProjection<'_>,pages:&OriginalApplicationPlan<'_>,drafts:&OriginalApplicationDraftPlan<'_>,assets:&OriginalApplicationAssets<'_>,audiences:&OriginalApplicationAudiences<'_>)->anyhow::Result<Self>{
        source.fence().await?;pages.fence(source)?;drafts.fence(source)?;assets.fence(source).await?;audiences.fence(source,pages,drafts).await?;target.fence(projection).await?;
        ensure!(!pages.pages.is_empty()&&!audiences.samples.is_empty()&&target.qualified.activity_count()>0,"Actual ordinary scope/authority/activity evidence absent");
        let selected_source=sqlite_history_source(source.root,source.database,source.conn).await?;
        let mut bytes=0u64;for length in assets.assets.iter().map(|a|a.length).chain(assets.legacy.iter().map(|a|a.length)){bytes=bytes.checked_add(length).context("Ordinary historical byte coverage overflow")?;}
        let root=target.origin.selected.root_identity;let main=target.origin.selected.database_identity;
        let final_inventory=verification_pager_inventory(&target.origin.selected.active.root)?;
        let final_inventory_sha256=verification_worker_inventory_sha(&final_inventory)?;

        let payload=SqliteHistoricalPayload{version:1,scope:SQLITE_RECORD_SCOPE.into(),operation:uuid::Uuid::new_v4().to_string(),completed_at:chrono::Utc::now().to_rfc3339(),source:selected_source,backup_id:target.target.backup_id().into(),backup_manifest_sha256:target.target.manifest_sha256().into(),tested_root_device:root.0,tested_root_inode:root.1,tested_main_device:main.0,tested_main_inode:main.1,tested_generation:target.target.generation_id().into(),tested_ledger_sha256:target.target.ledger_sha256().into(),operator:source.current_human().await?,normal_elf_sha256:target.origin.stopped.executable_sha256.clone(),checks:SqliteHistoricalChecks{pages:pages.pages.len(),drafts:drafts.drafts.len(),registered_assets:assets.assets.len(),legacy_assets:assets.legacy.len(),exact_binary_bytes:bytes,audiences:audiences.samples.len(),history_sha256:projection.ordinary_history_sha256()?,original_application_sha256:target.qualified.closure().source_sha256().into(),observed_application_sha256:target.qualified.closure().target_sha256().into(),session_activity_sha256:target.qualified.activity_sha256().into(),observed_sessions:target.qualified.activity_count(),original_inventory_sha256:verification_worker_inventory_sha(source.inventory)?,final_inventory_sha256},store_device:0,store_inode:0,file_device:0,file_inode:0};
        source.fence().await?;target.fence(projection).await?;Ok(Self{payload})
    }
}
fn sqlite_history_mac(payload:&SqliteHistoricalPayload,key:&[u8])->anyhow::Result<String>{use hmac::Mac;ensure!(key.len()>=32,"Verification history key unavailable");let raw=serde_json::to_vec(payload)?;ensure!(raw.len()<=65536,"Verification history exceeds64KiB");let mut mac=hmac::Hmac::<sha2::Sha256>::new_from_slice(key).map_err(|_|fail("Verification history key refused"))?;mac.update(SQLITE_RECORD_DOMAIN);mac.update(&raw);Ok(hex::encode(mac.finalize().into_bytes()))}
fn sqlite_history_verify(record:&SqliteSealedHistory,key:&[u8])->anyhow::Result<()>{use hmac::Mac;ensure!(key.len()>=32,"Verification history key unavailable");let raw=serde_json::to_vec(&record.payload)?;ensure!(raw.len()<=65536&&record.mac.len()==64,"Verification history payload/MAC bound differs");let supplied=hex::decode(&record.mac).map_err(|_|fail("Verification history MAC malformed"))?;let mut mac=hmac::Hmac::<sha2::Sha256>::new_from_slice(key).map_err(|_|fail("Verification history key refused"))?;mac.update(SQLITE_RECORD_DOMAIN);mac.update(&raw);mac.verify_slice(&supplied).map_err(|_|fail("Verification history authentication unavailable"))?;ensure!(record.payload.version==1&&record.payload.scope==SQLITE_RECORD_SCOPE&&uuid::Uuid::parse_str(&record.payload.operation)?.to_string()==record.payload.operation,"Unsupported SQLite history scope/operation");chrono::DateTime::parse_from_rfc3339(&record.payload.completed_at)?;Ok(())}

pub(crate) struct SqliteOrdinaryHistoryStore{root:Root,id:Identity,key:Vec<u8>,exclusive:bool}
impl SqliteOrdinaryHistoryStore{
 fn selected(path:&Path,source:&Root,workspace:Option<&Root>,key:&str,exclusive:bool)->anyhow::Result<Self>{
  ensure!(path.is_absolute(),"Verification history store must be explicit absolute path");let root=Root::open(path)?;ordinary_listener_root_identity(&root)?;let md=root.fd().metadata()?;ensure!(md.uid()==unsafe{libc::geteuid()}&&md.mode()&0o7777==0o700,"Verification history store must be private0700/current-owner");
  let id=identity(root.fd())?;let source_census=OrdinarySourceDirectories::hold(source)?;source_census.outside(&root)?;if let Some(workspace)=workspace{OrdinarySourceDirectories::hold(workspace)?.outside(&root)?;}
  // Also refuse a store containing either selected root, not only descendants.
  for anchor in std::iter::once(source).chain(workspace){ensure!(!anchor.chain.iter().any(|f|identity(f).ok()==Some(id)),"Verification store contains selected source/target workspace");}
  ensure!(key.len()>=32,"Verification history key unavailable");ensure!(unsafe{libc::flock(root.fd().as_raw_fd(),if exclusive{libc::LOCK_EX|libc::LOCK_NB}else{libc::LOCK_SH|libc::LOCK_NB})}==0,"Verification history store is busy");
  let value=Self{root,id,key:key.as_bytes().to_vec(),exclusive};value.fence()?;value.enumerate()?;Ok(value)
 }
 fn fence(&self)->anyhow::Result<()>{ordinary_listener_root_identity(&self.root)?;let md=self.root.fd().metadata()?;ensure!(identity(self.root.fd())?==self.id&&md.uid()==unsafe{libc::geteuid()}&&md.mode()&0o7777==0o700,"Verification history store replaced or no longer private");Ok(())}
 fn read(&self,name:&str,limit:u64)->anyhow::Result<Vec<u8>>{self.fence()?;let file=self.root.leaf(name)?;let md=file.metadata()?;ensure!(md.is_file()&&md.uid()==unsafe{libc::geteuid()}&&md.mode()&0o7777==0o600&&md.nlink()==1&&md.len()<=limit,"Verification history file not private/bounded/original");let stamp=physical_stamp(&file)?;let mut raw=vec![0;usize::try_from(md.len())?];file.read_exact_at(&mut raw,0)?;ensure!(physical_stamp(&file)?==stamp&&physical_stamp(&self.root.leaf(name)?)?==stamp,"Verification history file changed while read");self.fence()?;Ok(raw)}
 fn enumerate(&self)->anyhow::Result<Vec<String>>{self.fence()?;let mut names=std::collections::BTreeSet::new();for entry in std::fs::read_dir(format!("/proc/self/fd/{}",self.root.fd().as_raw_fd()))?{ensure!(names.len()<768,"Verification history store exceeds256 operations");let e=entry?;ensure!(e.file_type()?.is_file(),"Verification history store has foreign topology");let name=e.file_name().into_string().map_err(|_|fail("Verification history name invalid"))?;let(op,suffix)=name.rsplit_once('.').context("Verification history filename malformed")?;ensure!(uuid::Uuid::parse_str(op)?.to_string()==op&&matches!(suffix,"json"|"unknown"|"complete"),"Verification history filename unsupported");ensure!(names.insert(name),"Duplicate verification history file");}
  let mut out=Vec::new();for name in &names{let(op,_)=name.rsplit_once('.').unwrap();ensure!(names.contains(&format!("{op}.unknown"))&&names.contains(&format!("{op}.complete"))&&names.contains(&format!("{op}.json")),"Unresolved verification publication requires named review; no automatic retry");if !name.ends_with(".json"){continue;}
   let j:SqliteHistoryJournal=serde_json::from_slice(&self.read(&format!("{op}.unknown"),8192)?)?;let complete=self.read(&format!("{op}.complete"),8192)?;ensure!(self.read(&format!("{op}.unknown"),8192)?==serde_json::to_vec(&j)?,"Verification publication completion differs");sqlite_history_verify_complete(&complete,&j,&self.key)?;let raw=self.read(name,131072)?;let record:SqliteSealedHistory=serde_json::from_slice(&raw)?;ensure!(serde_json::to_vec(&record)?==raw,"Verification history canonical bytes changed");sqlite_history_verify(&record,&self.key)?;let file=self.root.leaf(name)?;let p=&record.payload;ensure!(j.version==1&&j.operation==op&&p.operation==op&&j.record_sha256==digest(&raw)&&(j.store_device,j.store_inode)==self.id&&(p.store_device,p.store_inode)==self.id&&(j.record_device,j.record_inode)==identity(&file)?&&(p.file_device,p.file_inode)==identity(&file)?,"Verification history native publication binding differs");out.push(name.clone());}
  self.fence()?;Ok(out)
 }
 fn journal(&self,name:&str,raw:&[u8])->anyhow::Result<()>{self.fence()?;let file=open_at(self.root.fd(),name,libc::O_WRONLY|libc::O_CREAT|libc::O_EXCL)?;ensure!(unsafe{libc::fchmod(file.as_raw_fd(),0o600)}==0,"Verification journal private mode refused");let mut file=file;use std::io::Write;file.write_all(raw)?;file.sync_all()?;self.root.fd().sync_all()?;self.fence()?;Ok(())}
 pub(crate) async fn publish(&self,mut evidence:SqliteCompleteOrdinaryEvidence,original:&mut StoppedVerificationSource<'_>,target:&mut SqliteRetiredObservedTarget<'_>,projection:&VerificationSqlProjection<'_>)->anyhow::Result<serde_json::Value>{
  ensure!(self.exclusive,"Read-only verification store cannot publish");self.enumerate()?;original.fence().await?;target.fence(projection).await?;
  let file=open_at(self.root.fd(),".",libc::O_RDWR|libc::O_TMPFILE)?;ensure!(unsafe{libc::fchmod(file.as_raw_fd(),0o600)}==0&&file.metadata()?.nlink()==0,"Unlinked verification owner unavailable");let fid=identity(&file)?;
  evidence.payload.store_device=self.id.0;evidence.payload.store_inode=self.id.1;evidence.payload.file_device=fid.0;evidence.payload.file_inode=fid.1;ensure!(evidence.payload.source==sqlite_history_source(original.root,original.database,original.conn).await?,"Verification source changed before publication");let op=evidence.payload.operation.clone();let summary=sqlite_history_summary(&evidence.payload);let mac=sqlite_history_mac(&evidence.payload,&self.key)?;let raw=serde_json::to_vec(&SqliteSealedHistory{payload:evidence.payload,mac})?;ensure!(raw.len()<=131072,"Sealed verification record exceeds128KiB");let mut write=file.try_clone()?;use std::io::Write;write.write_all(&raw)?;write.sync_all()?;
  let journal=serde_json::to_vec(&SqliteHistoryJournal{version:1,operation:op.clone(),record_sha256:digest(&raw),record_device:fid.0,record_inode:fid.1,store_device:self.id.0,store_inode:self.id.1})?;
  self.journal(&format!("{op}.unknown"),&journal)?;original.fence().await?;target.fence(projection).await?;self.fence()?;
  let from=CString::new(format!("/proc/self/fd/{}",file.as_raw_fd()))?;let name=CString::new(format!("{op}.json"))?;ensure!(unsafe{libc::linkat(libc::AT_FDCWD,from.as_ptr(),self.root.fd().as_raw_fd(),name.as_ptr(),libc::AT_SYMLINK_FOLLOW)}==0,"Verification publication outcome retained for review; no automatic retry");file.sync_all()?;self.root.fd().sync_all()?;ensure!(file.metadata()?.nlink()==1&&identity(&self.root.leaf(name.to_str()?)?)?==fid&&self.read(name.to_str()?,131072)?==raw,"Verification published readback differs; retained for review");original.fence().await?;target.fence(projection).await?;
  let complete=sqlite_history_complete(&serde_json::from_slice::<SqliteHistoryJournal>(&journal)?,&self.key)?;self.journal(&format!("{op}.complete"),&complete)?;self.enumerate()?;original.fence().await?;target.fence(projection).await?;Ok(summary)
 }
}
impl SqliteOrdinaryHistoryStore {
    pub(crate) fn for_producer(source:&Root,workspace:&Root,key:&str)->anyhow::Result<Self>{let path=std::env::var("VERIFIED_RESTORE_RECORDS_DIR").context("Ordinary application completion requires explicit private VERIFIED_RESTORE_RECORDS_DIR")?;Self::selected(Path::new(&path),source,Some(workspace),key,true)}
    async fn read_source(&self,source:&SqliteHistorySource)->anyhow::Result<Option<SqliteSealedHistory>>{
        let mut last:Option<SqliteSealedHistory>=None;
        for name in self.enumerate()?{let raw=self.read(&name,131072)?;let record:SqliteSealedHistory=serde_json::from_slice(&raw)?;sqlite_history_verify(&record,&self.key)?;ensure!(record.payload.source==*source,"SQLite history belongs to another original native source/generation");if last.as_ref().is_none_or(|prior|prior.payload.completed_at<record.payload.completed_at){last=Some(record);}}
        self.fence()?;Ok(last)
    }
}
fn sqlite_history_summary(payload:&SqliteHistoricalPayload)->serde_json::Value{serde_json::json!({"status":"completed_isolated_test","completed_at":payload.completed_at,"scope":payload.scope,"operation":payload.operation,"pages":payload.checks.pages,"drafts":payload.checks.drafts,"registered_assets":payload.checks.registered_assets,"legacy_assets":payload.checks.legacy_assets,"exact_binary_bytes":payload.checks.exact_binary_bytes.to_string(),"sampled_humans":payload.checks.audiences,"semantics":"historical_completed_isolated_application_test_at_recorded_time","full_supported_restore_verified":false})}
struct SqliteNormalHistoryReader {root:Root,database:File,pool_source:Arc<crate::storage::recovery_records::PoolSource>,source:SqliteHistorySource,store:SqliteOrdinaryHistoryStore}
static SQLITE_NORMAL_HISTORY_READER:std::sync::OnceLock<Arc<SqliteNormalHistoryReader>>=std::sync::OnceLock::new();
pub(crate) async fn retain_sqlite_ordinary_history(state:&crate::AppState)->anyhow::Result<()>{
    if !matches!(crate::db::DatabaseEngine::from_url(state.db.connect_options().database_url.as_str())?,crate::db::DatabaseEngine::Sqlite){return Ok(())}
    let path=match std::env::var("VERIFIED_RESTORE_RECORDS_DIR"){Ok(p)=>p,Err(std::env::VarError::NotPresent)=>return Ok(()),Err(_)=>bail!("SQLite history configuration invalid")};
    let root=Root::open(&crate::storage::data_dir())?;root.check()?;
    let pool_source=crate::storage::recovery_records::pool_source(&state.db,&root.absolute).context("Actual initialized SQLite history pool absent")?;
    let database=root.leaf("starlane-notes.db")?;let mut conn=state.db.acquire().await?;
    let source=sqlite_history_source(&root,&database,&mut conn).await?;drop(conn);
    let store=SqliteOrdinaryHistoryStore::selected(Path::new(&path),&root,None,&state.jwt_secret,false)?;
    ensure!(crate::storage::recovery_records::pool_source(&state.db,&root.absolute).is_some_and(|p|Arc::ptr_eq(&p,&pool_source)),"Actual SQLite initialized history source changed");
    ensure!(SQLITE_NORMAL_HISTORY_READER.set(Arc::new(SqliteNormalHistoryReader{root,database,pool_source,source,store})).is_ok(),"SQLite history startup owner already selected");Ok(())
}
impl SqliteNormalHistoryReader {
    async fn observe(&self,state:&crate::AppState)->anyhow::Result<serde_json::Value>{
        ensure!(self.store.key==state.jwt_secret.as_bytes(),"SQLite history key changed");self.root.check()?;self.store.fence()?;
        ensure!(crate::storage::recovery_records::pool_source(&state.db,&self.root.absolute).is_some_and(|p|Arc::ptr_eq(&p,&self.pool_source)),"Actual initialized SQLite reader source changed");
        let mut conn=state.db.acquire().await?;let current=sqlite_history_source(&self.root,&self.database,&mut conn).await?;ensure!(current==self.source,"SQLite history source incarnation/generation changed");drop(conn);
        let record=self.store.read_source(&current).await?;
        let mut conn=state.db.acquire().await?;ensure!(sqlite_history_source(&self.root,&self.database,&mut conn).await?==current,"SQLite history source changed during record read");drop(conn);
        self.store.fence()?;ensure!(crate::storage::recovery_records::pool_source(&state.db,&self.root.absolute).is_some_and(|p|Arc::ptr_eq(&p,&self.pool_source)),"SQLite history initialized source changed after read");
        Ok(record.map(|r|sqlite_history_summary(&r.payload)).unwrap_or_else(sqlite_history_unavailable))
    }
}
fn sqlite_history_unavailable()->serde_json::Value{serde_json::json!({"status":"unavailable","completed_at":null,"scope":null,"operation":null,"pages":null,"drafts":null,"registered_assets":null,"legacy_assets":null,"exact_binary_bytes":null,"sampled_humans":null,"semantics":"historical_completed_isolated_application_test_at_recorded_time","full_supported_restore_verified":false})}
pub(crate) async fn observe_sqlite_ordinary_history(state:&crate::AppState)->Option<serde_json::Value>{
    let reader=SQLITE_NORMAL_HISTORY_READER.get()?;
    Some(match tokio::time::timeout(std::time::Duration::from_secs(2),reader.observe(state)).await{Ok(Ok(report))=>report,_=>sqlite_history_unavailable()})
}

#[cfg(test)]mod sqlite_ordinary_history_controls{
 use super::*;use std::os::unix::fs::DirBuilderExt;
 fn dirs()->anyhow::Result<(PathBuf,Root,Root,PathBuf)>{let base=std::env::temp_dir().join(format!("verification-history-kernel-{}",uuid::Uuid::new_v4()));std::fs::DirBuilder::new().mode(0o700).create(&base)?;for n in ["source","workspace","store"]{std::fs::DirBuilder::new().mode(0o700).create(base.join(n))?;}let source=Root::open(&base.join("source"))?;let workspace=Root::open(&base.join("workspace"))?;Ok((base.clone(),source,workspace,base.join("store")))}
 #[test]fn history_store_explicit_native_overlap_and_read_only_no_creation()->anyhow::Result<()>{let(base,source,workspace,path)=dirs()?;ensure!(SqliteOrdinaryHistoryStore::selected(&source.absolute,&source,Some(&workspace),"12345678901234567890123456789012",true).is_err(),"Source root granted record store");let absent=base.join("absent");ensure!(SqliteOrdinaryHistoryStore::selected(&absent,&source,None,"12345678901234567890123456789012",false).is_err()&&!absent.exists(),"Reader created missing store");let store=SqliteOrdinaryHistoryStore::selected(&path,&source,Some(&workspace),"12345678901234567890123456789012",true)?;ensure!(store.enumerate()?.is_empty(),"New record store not empty");ensure!(SqliteOrdinaryHistoryStore::selected(&path,&source,None,"12345678901234567890123456789012",false).is_err(),"Independent reader bypassed held publication directory lock");drop(store);ensure!(SqliteOrdinaryHistoryStore::selected(&path,&source,None,"12345678901234567890123456789012",false)?.enumerate()?.is_empty(),"Released store failed read-only selection");Ok(())}
 #[test]fn unknown_outcome_blocks_new_save_without_cleanup()->anyhow::Result<()>{let(_,source,workspace,path)=dirs()?;let store=SqliteOrdinaryHistoryStore::selected(&path,&source,Some(&workspace),"12345678901234567890123456789012",true)?;let name=format!("{}.unknown",uuid::Uuid::new_v4());store.journal(&name,b"unknown original publication")?;let file=store.root.leaf(&name)?;let before=physical_stamp(&file)?;ensure!(store.enumerate().is_err(),"Unknown publication was treated as retry authority");ensure!(store.read(&name,8192)?==b"unknown original publication"&&physical_stamp(&file)?==before,"Unknown evidence was changed or removed");Ok(())}
 #[test]fn equal_content_foreign_store_inode_refuses()->anyhow::Result<()>{let(base,source,workspace,path)=dirs()?;let store=SqliteOrdinaryHistoryStore::selected(&path,&source,Some(&workspace),"12345678901234567890123456789012",true)?;std::fs::rename(&path,base.join("original-store-retained"))?;std::fs::DirBuilder::new().mode(0o700).create(&path)?;ensure!(store.fence().is_err(),"Equal-content replacement directory granted store authority");Ok(())}
 #[test]fn foreign_symlink_topology_refuses()->anyhow::Result<()>{let(_,source,workspace,path)=dirs()?;let store=SqliteOrdinaryHistoryStore::selected(&path,&source,Some(&workspace),"12345678901234567890123456789012",true)?;std::os::unix::fs::symlink("/dev/null",path.join("foreign"))?;ensure!(store.enumerate().is_err(),"Foreign symlink was accepted as history");Ok(())}
}

#[cfg(test)]mod sqlite_history_completion_controls {
 use super::*;use std::os::unix::fs::DirBuilderExt;
 const KEY:&[u8]=b"private-application-completion-control-key32";
 fn journal()->SqliteHistoryJournal{SqliteHistoryJournal{version:1,operation:uuid::Uuid::new_v4().to_string(),record_sha256:"a".repeat(64),record_device:1,record_inode:2,store_device:3,store_inode:4}}
 #[test]fn complete_ack_binds_each_original_journal_field_and_phase()->anyhow::Result<()>{
  let original=journal();let ack=sqlite_history_complete(&original,KEY)?;sqlite_history_verify_complete(&ack,&original,KEY)?;
  ensure!(sqlite_history_verify_complete(&serde_json::to_vec(&original)?,&original,KEY).is_err(),"Unsigned intent became completion");
  let base=serde_json::to_value(&original)?;
  for field in ["version","operation","record_sha256","record_device","record_inode","store_device","store_inode"]{
   let mut changed=base.clone();changed[field]=match field{"operation"=>uuid::Uuid::new_v4().to_string().into(),"record_sha256"=>"b".repeat(64).into(),_=>99.into()};let changed:SqliteHistoryJournal=serde_json::from_value(changed)?;ensure!(sqlite_history_verify_complete(&ack,&changed,KEY).is_err(),"Foreign original field accepted: {field}");
  }
  for field in ["version","phase","purpose"]{let mut changed:serde_json::Value=serde_json::from_slice(&ack)?;changed["fact"][field]=if field=="version"{99.into()}else{"pending-or-foreign-purpose".into()};ensure!(sqlite_history_verify_complete(&serde_json::to_vec(&changed)?,&original,KEY).is_err(),"Foreign completion fact accepted");}
  ensure!(sqlite_history_verify_complete(&ack,&original,b"different-application-completion-key32").is_err(),"Rotated key accepted");
  let mut noncanonical=ack.clone();noncanonical.push(b' ');ensure!(sqlite_history_verify_complete(&noncanonical,&original,KEY).is_err(),"Noncanonical ACK accepted");
  let mut extra:serde_json::Value=serde_json::from_slice(&ack)?;extra["caller_complete"]=true.into();ensure!(sqlite_history_verify_complete(&serde_json::to_vec(&extra)?,&original,KEY).is_err(),"Caller flag accepted");Ok(())
 }
 // Controlled signed-record/native-store fixture only, never a producer/source capability.
 #[test]fn linked_signed_record_requires_authenticated_ack_and_keeps_legacy_bytes()->anyhow::Result<()>{
  let base=std::env::temp_dir().join(format!("application-ack-sqlite-{}",uuid::Uuid::new_v4()));std::fs::DirBuilder::new().mode(0o700).create(&base)?;
  for n in ["source","workspace","records"]{std::fs::DirBuilder::new().mode(0o700).create(base.join(n))?;}
  let source=Root::open(&base.join("source"))?;let workspace=Root::open(&base.join("workspace"))?;let store=SqliteOrdinaryHistoryStore::selected(&base.join("records"),&source,Some(&workspace),std::str::from_utf8(KEY)?,true)?;
  let op=uuid::Uuid::new_v4().to_string();let mut file=open_at(store.root.fd(),&format!("{op}.json"),libc::O_RDWR|libc::O_CREAT|libc::O_EXCL)?;ensure!(unsafe{libc::fchmod(file.as_raw_fd(),0o600)}==0,"Controlled record mode unavailable");let id=identity(&file)?;
  let mut payload:serde_json::Value=serde_json::from_str(r#"{"version":1,"scope":"restored-sqlite-ledger1-ordinary-application-v1","operation":"","completed_at":"2026-10-15T12:00:00+00:00","backup_id":"controlled-backup","backup_manifest_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","operator":"controlled-metadata-only","normal_elf_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","store_device":0,"store_inode":0,"file_device":0,"file_inode":0,"checks":{"pages":1,"drafts":1,"registered_assets":1,"legacy_assets":0,"exact_binary_bytes":1,"audiences":1,"history_sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","original_application_sha256":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","observed_application_sha256":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee","session_activity_sha256":"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff","observed_sessions":1,"original_inventory_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","final_inventory_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"},"source":{"root_device":1,"root_inode":2,"main_device":3,"main_inode":4,"designation_device":5,"designation_inode":6,"designation_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","generation":null,"ledger_sha256":null},"tested_root_device":7,"tested_root_inode":8,"tested_main_device":9,"tested_main_inode":10,"tested_generation":"controlled-generation","tested_ledger_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}"#)?;payload["operation"]=op.clone().into();payload["store_device"]=store.id.0.into();payload["store_inode"]=store.id.1.into();payload["file_device"]=id.0.into();payload["file_inode"]=id.1.into();let payload:SqliteHistoricalPayload=serde_json::from_value(payload)?;let mac=sqlite_history_mac(&payload,KEY)?;let record=SqliteSealedHistory{payload,mac};let raw=serde_json::to_vec(&record)?;use std::io::Write;file.write_all(&raw)?;file.sync_all()?;
  let j=SqliteHistoryJournal{version:1,operation:op.clone(),record_sha256:digest(&raw),record_device:id.0,record_inode:id.1,store_device:store.id.0,store_inode:store.id.1};let intent=serde_json::to_vec(&j)?;let ack=sqlite_history_complete(&j,KEY)?;let mut wrong_domain:serde_json::Value=serde_json::from_slice(&ack)?;wrong_domain["mac"]=record.mac.clone().into();ensure!(sqlite_history_verify_complete(&serde_json::to_vec(&wrong_domain)?,&j,KEY).is_err(),"Payload signature promoted to completion ACK");
  store.journal(&format!("{op}.unknown"),&intent)?;store.journal(&format!("{op}.complete"),&ack)?;ensure!(store.enumerate()?==vec![format!("{op}.json")],"Controlled valid signed ACK unavailable");
  std::fs::write(store.root.absolute.join(format!("{op}.complete")),&intent)?;let before=store.read(&format!("{op}.complete"),8192)?;
  ensure!(store.enumerate().is_err(),"Copied unknown journal falsely completed valid linked signed record");ensure!(store.read(&format!("{op}.complete"),8192)?==before&&before==intent,"Legacy unsigned acknowledgment was repaired or changed");
  ensure!(store.read(&format!("{op}.unknown"),8192)?==intent&&store.read(&format!("{op}.json"),131072)?==raw,"Original journal/record changed on refusal");Ok(())
 }
}
