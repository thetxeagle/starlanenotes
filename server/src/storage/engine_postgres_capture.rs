include!("engine_transfer_admission_preservation.rs");
include!("engine_restored_admission_preservation.rs");
include!("engine_postgres_backup_producer.rs");
include!("engine_postgres_backup_records.rs");
async fn pg_prior_admission_capture(root:&Path,conn:&mut sqlx::AnyConnection,profile:EngineCatalogueProfile)->anyhow::Result<Option<crate::storage::restoration::linux::PriorRestoredCaptureLeases>> {
 if !profile.has_capacity()||!crate::storage::restoration::linux::has_finalized_admission_capture_source(root)?{return Ok(None)}
 use sqlx::Row;
 use crate::storage::restoration::admission::CapturedReservation;
 let rows=sqlx::query("SELECT operation_id,request_sha256,admission_claim_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,disposition_json,disposition_sha256 FROM attachment_capacity_reservations ORDER BY operation_id").fetch_all(&mut *conn).await?;
 let mut reservations=Vec::with_capacity(rows.len());
 for r in rows {reservations.push(CapturedReservation{operation_id:r.try_get("operation_id")?,request_sha256:r.try_get("request_sha256")?,admission_claim_sha256:r.try_get("admission_claim_sha256")?,maximum_bytes:r.try_get("maximum_bytes")?,state:r.try_get("state")?,committed_asset_id:r.try_get("committed_asset_id")?,committed_bytes:r.try_get("committed_bytes")?,disposition_json:r.try_get("disposition_json")?,disposition_sha256:r.try_get("disposition_sha256")?});}
 Ok(Some(crate::storage::restoration::linux::PriorRestoredCaptureLeases::hold(root,conn,&reservations).await?))
}

// Only a genuinely held native completion may populate completed transport.
// Existing finalized generations and mixed admission classes remain refused.
async fn pg_hold_completed_capture(root:&Path,tx:&mut sqlx::Transaction<'_,sqlx::Any>,profile:EngineCatalogueProfile,leases:Option<&crate::storage::restoration::admission::capture::NativeCaptureLeases>)->anyhow::Result<Option<std::sync::Arc<crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::NativeCompletedCapture>>> {
 if !profile.has_erasure() {return Ok(None)}
 let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachment_erasure_dispositions").fetch_one(&mut **tx).await?;
 if count==0 {return Ok(None)}
 ensure!(count>0,"Invalid completed disposition census");
 let leases=leases.context("Completed PG capture requires original native custody")?;
 Ok(Some(leases.capture_completed(root,tx).await?))
}

// Capacity capture retains original native writer exclusion through final SQL readback.
// This cannot certify copied/restored capsules or release retained quota.
async fn pg_hold_admission_capture(root:&Path,conn:&mut sqlx::AnyConnection,profile:EngineCatalogueProfile)->anyhow::Result<Option<crate::storage::restoration::admission::capture::NativeCaptureLeases>> {
 if !profile.has_capacity() {return Ok(None);}
 use sqlx::Row;
 use crate::storage::restoration::admission::{CapturedReservation,capture::NativeCaptureLeases};
 let rows=sqlx::query("SELECT operation_id,request_sha256,admission_claim_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,disposition_json,disposition_sha256 FROM attachment_capacity_reservations ORDER BY operation_id").fetch_all(&mut *conn).await?;
 let mut reservations=Vec::with_capacity(rows.len());
 for r in rows {reservations.push(CapturedReservation{operation_id:r.try_get("operation_id")?,request_sha256:r.try_get("request_sha256")?,admission_claim_sha256:r.try_get("admission_claim_sha256")?,maximum_bytes:r.try_get("maximum_bytes")?,state:r.try_get("state")?,committed_asset_id:r.try_get("committed_asset_id")?,committed_bytes:r.try_get("committed_bytes")?,disposition_json:r.try_get("disposition_json")?,disposition_sha256:r.try_get("disposition_sha256")?});}
 Ok(Some(NativeCaptureLeases::hold(root,conn,&reservations).await?))
}

// Candidate-only stopped-writer PG snapshot. A backup token is never serving authority.
struct PgCaptureHeld { held:File, stamp:PhysicalStamp, bytes:crate::storage::backup_inventory::Bytes }
pub(crate) struct OwnedPgBackup {
 root:Root,parent:Root,name:String,root_identity:Identity,
 manifest:Vec<u8>,roles:BTreeMap<String,PgCaptureHeld>,root_stamp:PhysicalStamp,directories:BTreeMap<String,(File,PhysicalStamp)>,
}
impl OwnedPgBackup {
 pub(crate) fn path(&self)->&Path { &self.root.absolute }
 pub(crate) fn fence(&self)->io::Result<()> {
  self.parent.check()?;
  if identity(&open_at(self.parent.fd(),&self.name,libc::O_RDONLY|libc::O_DIRECTORY)?)?!=self.root_identity
   || identity(self.root.fd())?!=self.root_identity {return Err(fail("PG backup root replaced"));}
  if physical_stamp(self.root.fd())?!=self.root_stamp{return Err(fail("PG backup root metadata changed"));}
  let(files,dirs)=engine_walk(self.root.fd(),"")?;if files!=self.roles.keys().cloned().collect()||dirs.keys().collect::<Vec<_>>()!=self.directories.keys().collect::<Vec<_>>(){return Err(fail("PG backup topology changed"));}
  for(p,(held,stamp))in &self.directories{if physical_stamp(held)?!=*stamp||physical_stamp(&destination_directory(self.root.fd(),p)?)?!=*stamp{return Err(fail("PG backup directory changed"));}}
  for(p,r)in &self.roles {
   if physical_stamp(&r.held)?!=r.stamp||physical_stamp(&self.root.leaf(p)?)?!=r.stamp
    ||planned_hash(&r.held)?!=r.bytes {return Err(fail("PG backup role changed"));}
  }Ok(())
 }
}
fn pg_capture_new_dir(parent:&File,name:&str)->io::Result<File> {
 if !valid_relative(name)||name.contains('/') {return Err(fail("Unsafe PG capture directory"));}
 let c=CString::new(name).map_err(|_|fail("NUL capture directory"))?;
 if unsafe{libc::mkdirat(parent.as_raw_fd(),c.as_ptr(),0o700)}!=0{return Err(io::Error::last_os_error());}
 let f=open_at(parent,name,libc::O_RDONLY|libc::O_DIRECTORY)?;parent.sync_all()?;Ok(f)
}
fn pg_capture_write(root:&Root,path:&str,raw:&[u8],roles:&mut BTreeMap<String,PgCaptureHeld>)->io::Result<()> {
 let(parent,name)=path.rsplit_once('/').map(|(p,n)|(p,n)).unwrap_or(("",path));
 let fd=destination_directory(root.fd(),parent)?;let mut f=open_at(&fd,name,libc::O_RDWR|libc::O_CREAT|libc::O_EXCL)?;
 {use std::io::Write;f.write_all(raw)?;}f.sync_all()?;fd.sync_all()?;
 let bytes=planned_hash(&f)?;roles.insert(path.into(),PgCaptureHeld{stamp:physical_stamp(&f)?,held:f,bytes});Ok(())
}

fn pg_capture_source_hash(file:&File)->io::Result<crate::storage::backup_inventory::Bytes>{
 let before=physical_stamp(file)?;let md=file.metadata()?;
 if !md.is_file()||md.len()>i64::MAX as u64{return Err(fail("Invalid regular PG source file"));}
 let mut hash=Sha256::new();let mut offset=0;let mut buffer=[0u8;65536];
 while offset<md.len(){let wanted=(md.len()-offset).min(buffer.len()as u64)as usize;let n=file.read_at(&mut buffer[..wanted],offset)?;if n==0{return Err(fail("PG source truncated"));}hash.update(&buffer[..n]);offset+=n as u64;}
 if physical_stamp(file)?!=before{return Err(fail("PG source file changed while hashing"));}
 Ok(crate::storage::backup_inventory::Bytes{size_bytes:md.len(),sha256:hex::encode(hash.finalize())})
}

fn pg_capture_copy(source:&Root,target:&Root)->anyhow::Result<(serde_json::Value,BTreeMap<String,PgCaptureHeld>,BTreeMap<String,(File,PhysicalStamp)>,BTreeMap<String,(File,PhysicalStamp)>)> {
 fn walk(s:&File,d:&File,prefix:&str,records:&mut BTreeMap<String,serde_json::Value>,target_roles:&mut BTreeMap<String,PgCaptureHeld>,source_files:&mut BTreeMap<String,(File,PhysicalStamp)>,source_dirs:&mut BTreeMap<String,(File,PhysicalStamp)>)->anyhow::Result<()> {
  let before=physical_stamp(s)?;let mut entries=std::fs::read_dir(format!("/proc/self/fd/{}",s.as_raw_fd()))?.map(|e|e.map(|e|e.file_name())).collect::<Result<Vec<_>,_>>()?;entries.sort();
  for name in entries {
   let name=name.into_string().map_err(|_|fail("NonUTF8 PG file name"))?;
   ensure!(valid_relative(&name)&&!name.contains(['/','\\']),"Unsafe PG file path");
   if prefix.is_empty()&&["logging","logs","search_index"].contains(&name.as_str()){continue;}
   let path=if prefix.is_empty(){name.clone()}else{format!("{prefix}/{name}")};
   let held=open_at(s,&name,libc::O_RDONLY|libc::O_NONBLOCK)?;let stamp=physical_stamp(&held)?;
   if held.metadata()?.is_dir(){
    let out=pg_capture_new_dir(d,&name)?;walk(&held,&out,&path,records,target_roles,source_files,source_dirs)?;
    records.insert(path.clone(),serde_json::json!({"kind":"directory","device":identity(&held)?.0,"inode":identity(&held)?.1}));
    source_dirs.insert(path,(held,stamp));out.sync_all()?;
   }else{
    ensure!(held.metadata()?.is_file(),"Nonregular PG storage entry");
    let expected=pg_capture_source_hash(&held)?;let mut out=open_at(d,&name,libc::O_RDWR|libc::O_CREAT|libc::O_EXCL)?;
    let mut offset=0;let mut buffer=[0u8;65536];
    while offset<expected.size_bytes {let wanted=(expected.size_bytes-offset).min(buffer.len()as u64)as usize;let n=held.read_at(&mut buffer[..wanted],offset)?;ensure!(n>0,"Source truncated");use std::io::Write;out.write_all(&buffer[..n])?;offset+=n as u64;}
    out.sync_all()?;d.sync_all()?;
    ensure!(physical_stamp(&held)?==stamp&&pg_capture_source_hash(&held)?==expected&&planned_hash(&out)?==expected,"Source bytes changed during PG capture");
    let md=held.metadata()?;records.insert(path.clone(),serde_json::json!({"kind":"file","device":md.dev(),"inode":md.ino(),"links":md.nlink(),"length":expected.size_bytes,"sha256":expected.sha256}));
    target_roles.insert(format!("files/{path}"),PgCaptureHeld{stamp:physical_stamp(&out)?,held:out,bytes:expected});
    source_files.insert(path,(held,stamp));
   }
  }ensure!(physical_stamp(s)?==before,"Source directory changed during PG capture");Ok(())
 }
 let out=pg_capture_new_dir(target.fd(),"files")?;let mut records=BTreeMap::new();let mut roles=BTreeMap::new();let mut files=BTreeMap::new();let mut dirs=BTreeMap::new();
 walk(source.fd(),&out,"",&mut records,&mut roles,&mut files,&mut dirs)?;
 Ok((serde_json::to_value(records)?,roles,files,dirs))
}
pub(crate) async fn capture_owned_postgres(serving:&EngineServing,target:&Path,writers_stopped:bool)->anyhow::Result<OwnedPgBackup> {capture_owned_postgres_inner(serving,target,writers_stopped,None).await}
async fn capture_owned_postgres_recorded(serving:&EngineServing,target:&Path,writers_stopped:bool,store:&PgByteBackupStore)->anyhow::Result<OwnedPgBackup>{capture_owned_postgres_inner(serving,target,writers_stopped,Some(store)).await}
async fn capture_owned_postgres_inner(serving:&EngineServing,target:&Path,writers_stopped:bool,recording:Option<&PgByteBackupStore>)->anyhow::Result<OwnedPgBackup>{
 ensure!(writers_stopped,"Stopped writers assertion required");
 let producer=PgHeldBackupProducer::hold()?;
 serving.fence().await?;let source=&serving.proof.root;
 let parent_path=target.parent().context("PG backup parent required")?;
 let name=target.file_name().and_then(|s|s.to_str()).context("PG backup leaf required")?.to_owned();
 let parent=Root::open(parent_path)?;parent.check()?;
 ensure!(target.is_absolute()&&!target.starts_with(&source.absolute)&&!source.absolute.starts_with(target),"PG backup source overlap");
 let mut conn=serving.pool.acquire().await?;serving.proof.verify_any_connection(&mut conn).await?;
 let mut tx=conn.begin().await?;
 sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await?;
 let source_profile=pg_profile_any(&mut tx).await?;let shape=shape_for_profile(source_profile)?;
 let transfer_preservation=if source_profile.has_capacity(){OriginalTransferAdmissionPreservation::select(&serving.proof,&mut tx).await?}else{None};
 let admission_leases=if transfer_preservation.is_some(){None}else{pg_hold_admission_capture(&source.absolute,&mut tx,source_profile).await?};
 let names=shape.keys().cloned().chain(["_sqlx_migrations".into(),"engine_transfer_import_commitments".into(),"engine_transfer_generations".into(),"engine_transfer_current".into()]).map(|n|quoted(&n)).collect::<anyhow::Result<Vec<_>>>()?.join(",");
 sqlx::query(&format!("LOCK TABLE {names} IN SHARE MODE")).execute(&mut *tx).await?;
 serving.proof.verify_any_connection(&mut tx).await?;
 // Classify only the cold-supported subset while retaining every original lease and SQL row.
 let completed_capture=pg_hold_completed_capture(&source.absolute,&mut tx,source_profile,admission_leases.as_ref()).await?;
 let admission_certificate=match (&admission_leases,&completed_capture) {
  (Some(leases),Some(completed))=>Some(leases.classify_with_completed(&mut tx,completed).await?),
  (Some(leases),None)=>Some(leases.classify_supported(&mut tx).await?),
  (None,None)=>None,
  (None,Some(_))=>bail!("Completed capture lost original native admission custody"),
 };
 let admission_capture=match &admission_certificate {Some(c)=>{let envelope=c.capture_envelope()?;if envelope.scopes.is_empty(){None}else{Some(envelope)}},None=>None};
 let tables=pg_tables_any(&mut tx,&shape).await?;
 let catalog=catalog_any(&mut tx,&serving.proof.envelope.target.schema_name).await?;
 let migrations:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(version,description,(extract(epoch from installed_on)::numeric)::text,success,encode(checksum,'hex'),execution_time)::text FROM _sqlx_migrations ORDER BY version").fetch_all(&mut *tx).await?;

 let imports:Vec<(String,String,String)>=sqlx::query_as("SELECT transfer_id,commitment_json,commitment_sha256 FROM engine_transfer_import_commitments ORDER BY transfer_id").fetch_all(&mut *tx).await?;
 let generations:Vec<(String,String,String,String)>=sqlx::query_as("SELECT transfer_id,envelope_json,envelope_sha256,state FROM engine_transfer_generations ORDER BY transfer_id").fetch_all(&mut *tx).await?;
 let current:Vec<(i64,String)>=sqlx::query_as("SELECT singleton,transfer_id FROM engine_transfer_current ORDER BY singleton").fetch_all(&mut *tx).await?;
 let raw_controls=serde_json::to_vec(&serde_json::json!({"imports":imports,"generations":generations,"current":current}))?;
 let source_root_stamp=physical_stamp(source.fd())?;
 let root_fd=pg_capture_new_dir(parent.fd(),&name)?;

 let root=Root::open(target)?;ensure!(identity(root.fd())?==identity(&root_fd)?,"PG backup root changed after exclusive creation");
 let mut roles=BTreeMap::new();pg_capture_write(&root,".starlane-backup-incomplete",b"candidate private PG backup; no serving authority\n",&mut roles)?;
 let(custody,copied,source_files,source_dirs)=pg_capture_copy(source,&root)?;roles.extend(copied);
 for(n,t)in &tables {if n=="documents"||n=="attachments"{let col=if n=="documents"{"content_path"}else{"storage_path"};let i=t.columns.iter().position(|c|c.name==col).context("Missing reference column")?;for row in &t.rows{if !matches!(row[i],Cell::Null){let p=text(&row[i])?;ensure!(source_files.contains_key(p),"Current PG file reference absent: {p}");}}}}
 for(p,(f,stamp))in &source_files{ensure!(physical_stamp(f)?==*stamp&&physical_stamp(&source.leaf(p)?)?==*stamp,"PG source leaf changed after copying");}
 for(p,(f,stamp))in &source_dirs{ensure!(physical_stamp(f)?==*stamp&&physical_stamp(&destination_directory(source.fd(),p)?)?==*stamp,"PG source directory changed after copying");}
 ensure!(pg_tables_any(&mut tx,&shape).await?==tables&&catalog_any(&mut tx,&serving.proof.envelope.target.schema_name).await?==catalog,"PG source SQL changed during capture");
 serving.proof.verify_any_connection(&mut tx).await?;
 let raw_tables=serde_json::to_vec(&tables)?;let raw_catalog=serde_json::to_vec(&catalog)?;let raw_migrations=serde_json::to_vec(&migrations)?;
 pg_capture_write(&root,"application-rows.json",&raw_tables,&mut roles)?;
 pg_capture_write(&root,"postgres-catalog.json",&raw_catalog,&mut roles)?;
 pg_capture_write(&root,"postgres-migrations.json",&raw_migrations,&mut roles)?;
 pg_capture_write(&root,"engine-control-rows.json",&raw_controls,&mut roles)?;
 let mut manifest_value=serde_json::json!({"format":"starlane-notes-postgresql-stopped-write-backup","format_version":if source_profile.has_capacity() {3}else{1},"backup_id":uuid::Uuid::new_v4().to_string(),"writers_stopped_asserted":true,"source_target":serving.proof.envelope.target,"source_root":{"device":identity(source.fd())?.0,"inode":identity(source.fd())?.1},"engine_generation":serving.proof.envelope.transfer_id,"engine_envelope_sha256":digest(&serving.proof.raw_envelope),"application_rows_sha256":digest(&raw_tables),"catalog_sha256":digest(&raw_catalog),"migrations_sha256":digest(&raw_migrations),"engine_control_rows_sha256":digest(&raw_controls),"files":custody,"excluded":["logging","logs","search_index"],"restoration_admission":"unsupported-until-new-target-attestation","serving_allowed":false,"engine_request":source_files.get(".starlane-engine-transfer-request").map(|(f,_)|pg_capture_source_hash(f)).transpose()?});
 if let Some(envelope)=&admission_capture {manifest_value.as_object_mut().context("PG manifest object required")?.insert("admission_owner_capture".into(),serde_json::to_value(envelope)?);}
 if let Some(completed)=&completed_capture {
  ensure!(admission_capture.is_none()&&!manifest_value.as_object().context("PG manifest object required")?.contains_key("prior_admission_owner_capture"),"Completed/native/prior transport conflict");
  manifest_value.as_object_mut().context("PG manifest object required")?.insert("completed_absence_capture".into(),serde_json::to_value(completed.envelope_in(&mut tx).await?)?);
 }
 producer.fence()?;manifest_value.as_object_mut().context("PG manifest object required")?.insert("producer".into(),serde_json::to_value(&producer.metadata)?);
 let manifest=serde_json::to_vec(&manifest_value)?;
 if let Some(completed)=&completed_capture {completed.fence_in(&mut tx).await?;}
 if let Some(p)=&transfer_preservation{p.recheck(&mut tx).await?;}
 // Read-only transaction closes before complete manifest. Incomplete marker remains explicit.

 if let Some(store)=recording {
  pg_capture_write(&root,"manifest.json",&manifest,&mut roles)?;let(_,directories)=engine_walk(root.fd(),"")?;let backup=OwnedPgBackup{root_stamp:physical_stamp(root.fd())?,directories,root_identity:identity(root.fd())?,root,parent,name,manifest,roles};backup.fence()?;
  let mut original=PgByteCaptureSource{proof:PgByteSourceProof::Transfer(serving),conn:&mut tx,profile:source_profile,tables:&tables,catalog:&catalog,root_stamp:source_root_stamp.clone(),files:&source_files,dirs:&source_dirs,owners:PgByteCaptureOwners{native:admission_leases.as_ref(),completed:completed_capture.as_ref(),certificate:admission_certificate.as_ref(),transfer:transfer_preservation.as_ref(),..Default::default()},backup:&backup};store.publish(&mut original,&producer).await?;original.fence().await?;drop(original);
  tx.rollback().await?;serving.proof.verify_any_connection(&mut conn).await?;if let Some(p)=&transfer_preservation{p.recheck(&mut conn).await?;}if let Some(x)=&completed_capture{x.fence_in(&mut conn).await?;}if let Some(x)=&admission_leases{x.recheck(&mut conn).await?;}if let Some(x)=&admission_certificate{x.fence()?;}drop(conn);serving.fence().await?;producer.fence()?;backup.fence()?;return Ok(backup);
 }
 tx.rollback().await?;if let Some(p)=&transfer_preservation{p.recheck(&mut conn).await?;}if let Some(completed)=&completed_capture {completed.fence_in(&mut conn).await?;}if let Some(leases)=&admission_leases {leases.recheck(&mut conn).await?;}if let Some(c)=&admission_certificate {c.fence()?;}drop(conn);serving.fence().await?;
 ensure!(physical_stamp(source.fd())?==source_root_stamp,"PG source root changed before manifest");
 for(p,(f,stamp))in &source_files{ensure!(physical_stamp(f)?==*stamp&&physical_stamp(&source.leaf(p)?)?==*stamp,"Source leaf changed before manifest");}
 for(p,(f,stamp))in &source_dirs{ensure!(physical_stamp(f)?==*stamp&&physical_stamp(&destination_directory(source.fd(),p)?)?==*stamp,"Source directory changed before manifest");}

 pg_capture_write(&root,"manifest.json",&manifest,&mut roles)?;
 let(_,directories)=engine_walk(root.fd(),"")?;
 let backup=OwnedPgBackup{root_stamp:physical_stamp(root.fd())?,directories,root_identity:identity(root.fd())?,root,parent,name,manifest,roles};
 backup.fence()?;if let Some(c)=&admission_certificate {c.fence()?;}serving.fence().await?;
 if let Some(completed)=&completed_capture {let mut final_connection=serving.pool.acquire().await?;serving.proof.verify_any_connection(&mut final_connection).await?;completed.fence_in(&mut final_connection).await?;}
 if let Some(p)=&transfer_preservation{let mut final_connection=serving.pool.acquire().await?;p.recheck(&mut final_connection).await?;}
 producer.fence()?;backup.fence()?;Ok(backup)
}

// A restored PostgreSQL source has its own SQL-anchored authority. The older
// SQLite engine transfer remains archived bytes, never the current target.
fn pg_capture_output_parent(source:&Root,parent:&Root)->anyhow::Result<()> {
 engine_root_fence(source)?;parent.check()?;
 let (_,directories)=engine_walk(source.fd(),"")?;
 let mut excluded=std::collections::BTreeSet::from([identity(source.fd())?]);
 for (held,_) in directories.values(){excluded.insert(identity(held)?);}
 for held in &parent.chain {ensure!(!excluded.contains(&identity(held)?),"PG capture target ancestry overlaps held source custody");}
 engine_root_fence(source)?;parent.check()?;Ok(())
}
fn pg_capture_current_references(tables:&BTreeMap<String,Table>,files:&BTreeMap<String,(File,PhysicalStamp)>)->anyhow::Result<()> {
 for name in ["documents","attachments","excalidraw_files"] {
  let table=&tables[name];let column=if name=="documents"{"content_path"}else{"storage_path"};
  let index=table.columns.iter().position(|c|c.name==column).context("Missing current PG reference column")?;
  let length=if name=="attachments"{Some(table.columns.iter().position(|c|c.name=="size_bytes").context("Missing attachment length")?)}else{None};
  for row in &table.rows {if !matches!(row[index],Cell::Null) {
   let path=text(&row[index])?;ensure!(valid_relative(path),"Unsafe current PG file reference");
   let (held,stamp)=files.get(path).context("Current PG file reference absent")?;
   ensure!(physical_stamp(held)?==*stamp,"Current PG file changed during reference admission");
   if let Some(index)=length {let expected=match row[index]{Cell::Integer(v)if v>=0=>v as u64,_=>bail!("Invalid attachment length")};ensure!(held.metadata()?.len()==expected,"PG attachment length differs");}
  }}
 }Ok(())
}
pub(crate) async fn capture_restored_postgres(serving:&RestoredPgServing,target:&Path,writers_stopped:bool)->anyhow::Result<OwnedPgBackup> {
 let (backup,_)=capture_restored_postgres_observed::<(),_>(serving,target,writers_stopped,None,None,|_,_|Box::pin(async{Ok(())})).await?;Ok(backup)
}
pub(crate) async fn capture_restored_postgres_for_verification<T,F>(serving:&RestoredPgServing,target:&Path,writers_stopped:bool,admin:&crate::auth::middleware::AdminUser,observe:F)->anyhow::Result<T>
where F:for<'a> FnOnce(StoppedPgVerificationSource<'a>,&'a OwnedPgBackup)->std::pin::Pin<Box<dyn std::future::Future<Output=anyhow::Result<T>>+Send+'a>> {
 let(_,value)=capture_restored_postgres_observed(serving,target,writers_stopped,Some(admin),None,observe).await?;value.context("Original PG observer did not execute")
}
async fn capture_restored_postgres_recorded(serving:&RestoredPgServing,target:&Path,writers_stopped:bool,store:&PgByteBackupStore)->anyhow::Result<OwnedPgBackup>{let(backup,_)=capture_restored_postgres_observed::<(),_>(serving,target,writers_stopped,None,Some(store),|_,_|Box::pin(async{Ok(())})).await?;Ok(backup)}
async fn capture_restored_postgres_observed<T,F>(serving:&RestoredPgServing,target:&Path,writers_stopped:bool,admin:Option<&crate::auth::middleware::AdminUser>,recording:Option<&PgByteBackupStore>,observe:F)->anyhow::Result<(OwnedPgBackup,Option<T>)>
where F:for<'a> FnOnce(StoppedPgVerificationSource<'a>,&'a OwnedPgBackup)->std::pin::Pin<Box<dyn std::future::Future<Output=anyhow::Result<T>>+Send+'a>> {
 ensure!(writers_stopped,"Stopped writers assertion required");
 let producer=PgHeldBackupProducer::hold()?;
 serving.fence().await?;let source=&serving.proof.root;
 let parent_path=target.parent().context("PG backup parent required")?;
 let name=target.file_name().and_then(|s|s.to_str()).context("PG backup leaf required")?.to_owned();
 let parent=Root::open(parent_path)?;parent.check()?;
 ensure!(target.is_absolute()&&!target.starts_with(&source.absolute)&&!source.absolute.starts_with(target),"PG backup source overlap");
 let mut conn=serving.pool.acquire().await?;serving.proof.verify_any_connection(&mut conn).await?;
 let mut tx=conn.begin().await?;
 sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await?;
 if let Some(admin)=admin {super::verification_source_human(&mut tx,admin).await?;}
 let source_profile=pg_profile_any(&mut tx).await?;let shape=shape_for_profile(source_profile)?;
 let names=shape.keys().cloned().chain(["_sqlx_migrations".into(),"pg_restoration_imports".into(),"pg_restoration_generations".into(),"pg_restoration_current".into()]).map(|n|quoted(&n)).collect::<anyhow::Result<Vec<_>>>()?.join(",");
 sqlx::query(&format!("LOCK TABLE {names} IN SHARE MODE")).execute(&mut *tx).await?;
 serving.proof.verify_any_connection(&mut tx).await?;
 // Select prior completed transport only from a retained genuine same-target
 // token after full profile/table/connection proof. Copied metadata is not a lease.
 // A compound source is admitted only through the genuine original held roster.
 // Constructor errors refuse; no native/prior/mixed fallback may reinterpret it.
 let composite_capture=if source_profile.has_erasure()&&crate::storage::restoration::linux::has_composite_capture_source(&source.absolute,&mut tx).await? {
  Some(crate::storage::attachment_upload::admission::committed_erasure::accounting::composite::CompositeOriginalSource::hold_in(&source.absolute,&mut tx).await?)
 }else{None};
 let mixed_completed_capture=if composite_capture.is_none()&&source_profile.has_erasure()&&crate::storage::restoration::linux::has_mixed_completed_capture_source(&source.absolute,&mut tx).await? {Some(crate::storage::restoration::linux::MixedCompletedCapture::hold(&source.absolute,&mut tx).await?)}else{None};
 let prior_completed_capture=if composite_capture.is_none()&&mixed_completed_capture.is_none()&&source_profile.has_erasure()&&crate::storage::restoration::linux::has_finalized_completed_capture_source(&source.absolute)? {
  Some(crate::storage::restoration::linux::FinalizedCompletedCapture::hold(&source.absolute,&mut tx).await?)
 }else{None};
 let original_preservation=if composite_capture.is_none()&&mixed_completed_capture.is_none()&&prior_completed_capture.is_none()&&source_profile.has_capacity(){OriginalRestoredAdmissionPreservation::select(&serving.proof,&mut tx).await?}else{None};
 let prior_admission_leases=if composite_capture.is_some()||mixed_completed_capture.is_some()||prior_completed_capture.is_some()||original_preservation.is_some(){None}else{pg_prior_admission_capture(&source.absolute,&mut tx,source_profile).await?};
 let admission_leases=if composite_capture.is_some()||mixed_completed_capture.is_some()||prior_completed_capture.is_some()||prior_admission_leases.is_some()||original_preservation.is_some(){None}else{pg_hold_admission_capture(&source.absolute,&mut tx,source_profile).await?};
 // Classify only the cold-supported subset while retaining every original lease and SQL row.
 let completed_capture=if composite_capture.is_some()||mixed_completed_capture.is_some()||prior_completed_capture.is_some()||original_preservation.is_some(){None}else{pg_hold_completed_capture(&source.absolute,&mut tx,source_profile,admission_leases.as_ref()).await?};
 let admission_certificate=match (&admission_leases,&completed_capture) {
  (Some(leases),Some(completed))=>Some(leases.classify_with_completed(&mut tx,completed).await?),
  (Some(leases),None)=>Some(leases.classify_supported(&mut tx).await?),
  (None,None)=>None,
  (None,Some(_))=>bail!("Completed capture lost original native admission custody"),
 };
 let admission_capture=match &admission_certificate {Some(c)=>{let envelope=c.capture_envelope()?;if envelope.scopes.is_empty(){None}else{Some(envelope)}},None=>None};
 let prior_admission_capture=match &prior_admission_leases {Some(leases)=>pg_prior_supported_evidence(leases)?,None=>None};
 let tables=pg_tables_any(&mut tx,&shape).await?;
 let catalog=catalog_any(&mut tx,&serving.proof.attestation.target.schema_name).await?;
 let migrations:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(version,description,(extract(epoch from installed_on)::numeric)::text,success,encode(checksum,'hex'),execution_time)::text FROM _sqlx_migrations ORDER BY version").fetch_all(&mut *tx).await?;

 let imports:Vec<(String,String,String)>=sqlx::query_as("SELECT restore_id,commitment_json,commitment_sha256 FROM pg_restoration_imports ORDER BY restore_id").fetch_all(&mut *tx).await?;
 let generations:Vec<(String,String,String,String)>=sqlx::query_as("SELECT restore_id,envelope_json,envelope_sha256,state FROM pg_restoration_generations ORDER BY restore_id").fetch_all(&mut *tx).await?;
 let current:Vec<(i64,String)>=sqlx::query_as("SELECT singleton,restore_id FROM pg_restoration_current ORDER BY singleton").fetch_all(&mut *tx).await?;
 let raw_controls=serde_json::to_vec(&serde_json::json!({"imports":imports,"generations":generations,"current":current}))?;
 let source_root_stamp=physical_stamp(source.fd())?;
 pg_capture_output_parent(source,&parent)?;serving.proof.physical_fence()?;
 engine_checkpoint("pg-restored-capture-parent-held")?;
 pg_capture_output_parent(source,&parent)?;
 let root_fd=pg_capture_new_dir(parent.fd(),&name)?;
 pg_capture_output_parent(source,&parent)?;serving.proof.physical_fence()?;

 let root=Root::open(target)?;ensure!(identity(root.fd())?==identity(&root_fd)?,"PG backup root changed after exclusive creation");
 let mut roles=BTreeMap::new();pg_capture_write(&root,".starlane-backup-incomplete",b"candidate private PG backup; no serving authority\n",&mut roles)?;
 let(custody,copied,source_files,source_dirs)=pg_capture_copy(source,&root)?;roles.extend(copied);
 pg_capture_current_references(&tables,&source_files)?;
 for(p,(f,stamp))in &source_files{ensure!(physical_stamp(f)?==*stamp&&physical_stamp(&source.leaf(p)?)?==*stamp,"PG source leaf changed after copying");}
 for(p,(f,stamp))in &source_dirs{ensure!(physical_stamp(f)?==*stamp&&physical_stamp(&destination_directory(source.fd(),p)?)?==*stamp,"PG source directory changed after copying");}
 ensure!(pg_tables_any(&mut tx,&shape).await?==tables&&catalog_any(&mut tx,&serving.proof.attestation.target.schema_name).await?==catalog,"PG source SQL changed during capture");
 serving.proof.verify_any_connection(&mut tx).await?;
 let raw_tables=serde_json::to_vec(&tables)?;let raw_catalog=serde_json::to_vec(&catalog)?;let raw_migrations=serde_json::to_vec(&migrations)?;
 pg_capture_write(&root,"application-rows.json",&raw_tables,&mut roles)?;
 pg_capture_write(&root,"postgres-catalog.json",&raw_catalog,&mut roles)?;
 pg_capture_write(&root,"postgres-migrations.json",&raw_migrations,&mut roles)?;
 pg_capture_write(&root,"engine-control-rows.json",&raw_controls,&mut roles)?;
 let mut manifest_value=serde_json::json!({"format":"starlane-notes-postgresql-stopped-write-backup","format_version":if source_profile.has_capacity() {4}else{2},"backup_id":uuid::Uuid::new_v4().to_string(),"writers_stopped_asserted":true,"source_target":serving.proof.attestation.target,"source_root":{"device":identity(source.fd())?.0,"inode":identity(source.fd())?.1},"source_authority":{"kind":"pg_restore","generation_id":serving.proof.attestation.restore_id,"attestation_sha256":digest(&serving.proof.raw_attestation)},"application_rows_sha256":digest(&raw_tables),"catalog_sha256":digest(&raw_catalog),"migrations_sha256":digest(&raw_migrations),"engine_control_rows_sha256":digest(&raw_controls),"files":custody,"excluded":["logging","logs","search_index"],"restoration_admission":"unsupported-until-new-target-attestation","serving_allowed":false,"engine_request":source_files.get(".starlane-engine-transfer-request").map(|(f,_)|pg_capture_source_hash(f)).transpose()?});
 if let Some(envelope)=&admission_capture {manifest_value.as_object_mut().context("PG manifest object required")?.insert("admission_owner_capture".into(),serde_json::to_value(envelope)?);}
 if let Some(evidence)=&prior_admission_capture {ensure!(admission_capture.is_none(),"Prior/native evidence conflict");manifest_value.as_object_mut().context("PG manifest object required")?.insert("prior_admission_owner_capture".into(),serde_json::to_value(evidence)?);}
 if let Some(completed)=&completed_capture {
  ensure!(admission_capture.is_none()&&!manifest_value.as_object().context("PG manifest object required")?.contains_key("prior_admission_owner_capture"),"Completed/native/prior transport conflict");
  manifest_value.as_object_mut().context("PG manifest object required")?.insert("completed_absence_capture".into(),serde_json::to_value(completed.envelope_in(&mut tx).await?)?);
 }
 if let Some(prior)=&prior_completed_capture {
  ensure!(admission_capture.is_none()&&prior_admission_capture.is_none()&&completed_capture.is_none(),"Prior completed transport conflicts with original admission classes");
  manifest_value.as_object_mut().context("PG manifest object required")?.insert("prior_completed_absence_capture".into(),serde_json::to_value(prior.evidence()?)?);
  prior.recheck_in(&mut tx).await?;
 }
 if let Some(mixed)=&mixed_completed_capture {
  ensure!(prior_completed_capture.is_none()&&admission_capture.is_none()&&prior_admission_capture.is_none()&&completed_capture.is_none(),"Mixed completed transport conflicts with original/prior classes");
  manifest_value.as_object_mut().context("PG manifest object required")?.insert("mixed_completed_absence_capture".into(),serde_json::to_value(mixed.evidence()?)?);
  mixed.recheck_in(&mut tx).await?;
 }
 if let Some(composite)=&composite_capture {
  ensure!(mixed_completed_capture.is_none()&&prior_completed_capture.is_none()&&admission_capture.is_none()&&prior_admission_capture.is_none()&&completed_capture.is_none(),"Composite PG transport conflicts with an independent admission role");
  composite.recheck_in(&mut tx).await?;
  manifest_value.as_object_mut().context("PG manifest object required")?.insert("composite_admission_capture".into(),serde_json::to_value(composite.capture_evidence_in(&mut tx).await?)?);
 }
 producer.fence()?;manifest_value.as_object_mut().context("PG manifest object required")?.insert("producer".into(),serde_json::to_value(&producer.metadata)?);
 let manifest=serde_json::to_vec(&manifest_value)?;
 if let Some(completed)=&completed_capture {completed.fence_in(&mut tx).await?;}
 // Read-only transaction closes before complete manifest. Incomplete marker remains explicit.

 if let Some(leases)=&prior_admission_leases {leases.recheck_in(&mut tx).await?;}if let Some(preserved)=&original_preservation{preserved.recheck(&mut tx).await?;}
 if let Some(store)=recording {
  ensure!(admin.is_none(),"Byte-backup completion cannot reuse application observer purpose");pg_capture_write(&root,"manifest.json",&manifest,&mut roles)?;let(_,directories)=engine_walk(root.fd(),"")?;let backup=OwnedPgBackup{root_stamp:physical_stamp(root.fd())?,directories,root_identity:identity(root.fd())?,root,parent,name,manifest,roles};backup.fence()?;
  let mut original=PgByteCaptureSource{proof:PgByteSourceProof::Restored(serving),conn:&mut tx,profile:source_profile,tables:&tables,catalog:&catalog,root_stamp:source_root_stamp.clone(),files:&source_files,dirs:&source_dirs,owners:PgByteCaptureOwners{native:admission_leases.as_ref(),prior:prior_admission_leases.as_ref(),preserved:original_preservation.as_ref(),completed:completed_capture.as_ref(),prior_completed:prior_completed_capture.as_ref(),mixed:mixed_completed_capture.as_ref(),composite:composite_capture.as_ref(),certificate:admission_certificate.as_ref(),..Default::default()},backup:&backup};store.publish(&mut original,&producer).await?;original.fence().await?;drop(original);
  tx.rollback().await?;serving.proof.verify_any_connection(&mut conn).await?;if let Some(x)=&admission_leases{x.recheck(&mut conn).await?;}if let Some(x)=&prior_admission_leases{x.recheck_in(&mut conn).await?;}if let Some(preserved)=&original_preservation{preserved.recheck(&mut conn).await?;}if let Some(x)=&completed_capture{x.fence_in(&mut conn).await?;}if let Some(x)=&prior_completed_capture{x.recheck_in(&mut conn).await?;}if let Some(x)=&mixed_completed_capture{x.recheck_in(&mut conn).await?;}if let Some(x)=&composite_capture{x.recheck_in(&mut conn).await?;}if let Some(x)=&admission_certificate{x.fence()?;}drop(conn);serving.fence().await?;producer.fence()?;backup.fence()?;return Ok((backup,None));
 }
 if let Some(admin)=admin {
  let mut holder=StoppedPgVerificationSource{serving,conn:&mut tx,admin,profile:source_profile,tables:&tables,root_stamp:source_root_stamp.clone(),files:&source_files,dirs:&source_dirs,native:admission_leases.as_ref(),prior:prior_admission_leases.as_ref(),preserved:original_preservation.as_ref(),completed:completed_capture.as_ref(),prior_completed:prior_completed_capture.as_ref(),mixed:mixed_completed_capture.as_ref(),composite:composite_capture.as_ref(),certificate:admission_certificate.as_ref()};
  holder.fence().await?;
  pg_capture_write(&root,"manifest.json",&manifest,&mut roles)?;
  let(_,directories)=engine_walk(root.fd(),"")?;
  let backup=OwnedPgBackup{root_stamp:physical_stamp(root.fd())?,directories,root_identity:identity(root.fd())?,root,parent,name,manifest,roles};backup.fence()?;
  let value=observe(holder,&backup).await?;
  let mut final_holder=StoppedPgVerificationSource{serving,conn:&mut tx,admin,profile:source_profile,tables:&tables,root_stamp:source_root_stamp.clone(),files:&source_files,dirs:&source_dirs,native:admission_leases.as_ref(),prior:prior_admission_leases.as_ref(),preserved:original_preservation.as_ref(),completed:completed_capture.as_ref(),prior_completed:prior_completed_capture.as_ref(),mixed:mixed_completed_capture.as_ref(),composite:composite_capture.as_ref(),certificate:admission_certificate.as_ref()};
  final_holder.fence().await?;drop(final_holder);backup.fence()?;
  tx.rollback().await?;
  super::verification_source_human(&mut conn,admin).await?;serving.proof.verify_any_connection(&mut conn).await?;
  if let Some(composite)=&composite_capture {composite.recheck_in(&mut conn).await?;}if let Some(mixed)=&mixed_completed_capture {mixed.recheck_in(&mut conn).await?;}if let Some(completed)=&completed_capture {completed.fence_in(&mut conn).await?;}if let Some(leases)=&prior_admission_leases {leases.recheck_in(&mut conn).await?;}if let Some(preserved)=&original_preservation{preserved.recheck(&mut conn).await?;}if let Some(leases)=&admission_leases {leases.recheck(&mut conn).await?;}if let Some(prior)=&prior_completed_capture{prior.recheck_in(&mut conn).await?;}if let Some(c)=&admission_certificate{c.fence()?;}
  super::verification_source_human(&mut conn,admin).await?;drop(conn);
  if let Some(composite)=&composite_capture {let mut fresh=serving.pool.acquire().await?;serving.proof.verify_any_connection(&mut fresh).await?;composite.recheck_in(&mut fresh).await?;super::verification_source_human(&mut fresh,admin).await?;serving.proof.verify_any_connection(&mut fresh).await?;}
  serving.fence().await?;producer.fence()?;backup.fence()?;return Ok((backup,Some(value)));
 }
 tx.rollback().await?;if let Some(composite)=&composite_capture {composite.recheck_in(&mut conn).await?;}if let Some(mixed)=&mixed_completed_capture {mixed.recheck_in(&mut conn).await?;}if let Some(completed)=&completed_capture {completed.fence_in(&mut conn).await?;}if let Some(leases)=&prior_admission_leases {leases.recheck_in(&mut conn).await?;}if let Some(preserved)=&original_preservation{preserved.recheck(&mut conn).await?;}if let Some(leases)=&admission_leases {leases.recheck(&mut conn).await?;}if let Some(c)=&admission_certificate {c.fence()?;}drop(conn);
 if let Some(composite)=&composite_capture {let mut fresh=serving.pool.acquire().await?;serving.proof.verify_any_connection(&mut fresh).await?;composite.recheck_in(&mut fresh).await?;serving.proof.verify_any_connection(&mut fresh).await?;}
 if let Some(mixed)=&mixed_completed_capture {let mut fresh=serving.pool.acquire().await?;serving.proof.verify_any_connection(&mut fresh).await?;mixed.recheck_in(&mut fresh).await?;}
 if let Some(prior)=&prior_completed_capture {let mut fresh=serving.pool.acquire().await?;serving.proof.verify_any_connection(&mut fresh).await?;prior.recheck_in(&mut fresh).await?;}
 serving.fence().await?;
 ensure!(physical_stamp(source.fd())?==source_root_stamp,"PG source root changed before manifest");
 for(p,(f,stamp))in &source_files{ensure!(physical_stamp(f)?==*stamp&&physical_stamp(&source.leaf(p)?)?==*stamp,"Source leaf changed before manifest");}
 for(p,(f,stamp))in &source_dirs{ensure!(physical_stamp(f)?==*stamp&&physical_stamp(&destination_directory(source.fd(),p)?)?==*stamp,"Source directory changed before manifest");}

 pg_capture_write(&root,"manifest.json",&manifest,&mut roles)?;
 let(_,directories)=engine_walk(root.fd(),"")?;
 let backup=OwnedPgBackup{root_stamp:physical_stamp(root.fd())?,directories,root_identity:identity(root.fd())?,root,parent,name,manifest,roles};
 backup.fence()?;if let Some(c)=&admission_certificate {c.fence()?;}serving.fence().await?;
 if let Some(completed)=&completed_capture {let mut final_connection=serving.pool.acquire().await?;serving.proof.verify_any_connection(&mut final_connection).await?;completed.fence_in(&mut final_connection).await?;}
 if let Some(prior)=&prior_completed_capture {let mut final_connection=serving.pool.acquire().await?;serving.proof.verify_any_connection(&mut final_connection).await?;prior.recheck_in(&mut final_connection).await?;}
 if let Some(composite)=&composite_capture {let mut final_connection=serving.pool.acquire().await?;serving.proof.verify_any_connection(&mut final_connection).await?;composite.recheck_in(&mut final_connection).await?;serving.proof.verify_any_connection(&mut final_connection).await?;}
 if let Some(mixed)=&mixed_completed_capture {let mut final_connection=serving.pool.acquire().await?;serving.proof.verify_any_connection(&mut final_connection).await?;mixed.recheck_in(&mut final_connection).await?;}
 producer.fence()?;backup.fence()?;Ok((backup,None))
}

#[cfg(test)]
mod postgres_capture_tests {
 use super::*;
 #[tokio::test]
 #[ignore="explicit actual mutable qualified PG root and new capture target required"]
 async fn required_actual_postgres_backup_capture()->anyhow::Result<()> {
  let root=PathBuf::from(std::env::var("STARLANE_ENGINE_APP_ROOT").expect("qualified source root required"));
  let destination=PathBuf::from(std::env::var("STARLANE_PG_BACKUP_TARGET").expect("new backup target required"));
  let url=std::env::var("STARLANE_ENGINE_PG_URL").expect("owned PG URL required");
  let serving=open_engine_serving(&root,&url).await?;
  let mut conn=serving.pool.acquire().await?;
  let source_shape=shape_for_profile(pg_profile_any(&mut conn).await?)?;let before=pg_tables_any(&mut conn,&source_shape).await?;
  ensure!(before["attachments"].rows.len()>=2,"Actual new mutable application upload required");
  ensure!(before.values().flat_map(|t|&t.rows).flat_map(|r|r).any(|c|matches!(c,Cell::Blob(v) if !v.is_empty())),"Genuine nonempty application BYTEA producer required");
  drop(conn);
  let backup=capture_owned_postgres(&serving,&destination,true).await?;
  backup.fence()?;
  let manifest:serde_json::Value=serde_json::from_slice(&backup.manifest)?;
  ensure!(manifest["format"]=="starlane-notes-postgresql-stopped-write-backup"&&manifest["serving_allowed"]==false,"Wrong PG capture contract");
  let raw=std::fs::read(backup.path().join("application-rows.json"))?;
  let captured:BTreeMap<String,Table>=serde_json::from_slice(&raw)?;
  ensure!(captured==before,"Exact mutable PG typed rows not captured");
  let mut conn=serving.pool.acquire().await?;
  ensure!(pg_tables_any(&mut conn,&source_shape).await?==before,"PG capture mutated source");
  drop(conn);serving.fence().await?;
  println!("ACTUAL_PG_BACKUP_CAPTURE_PASS typed72 tables mutable application plus external files; no restore admission");
  Ok(())
 }
}

include!("engine_postgres_snapshot.rs");
include!("engine_postgres_restore.rs");

include!("engine_postgres_owner.rs");

include!("engine_postgres_materialize.rs");

include!("engine_postgres_seal.rs");

#[cfg(test)]
mod restored_postgres_capture_tests {
 use super::*;
 #[tokio::test]
 #[ignore="explicit actual finalized PostgreSQL restoration source and unique backup target required"]
 async fn required_actual_restored_postgres_backup_capture()->anyhow::Result<()> {
  let root=PathBuf::from(std::env::var("STARLANE_ENGINE_APP_ROOT").expect("qualified restored PG source root required"));
  let destination=PathBuf::from(std::env::var("STARLANE_PG_BACKUP_TARGET").expect("new private backup target required"));
  let url=std::env::var("STARLANE_ENGINE_PG_URL").expect("owned PG URL required");
  let serving=open_restored_pg_serving(&root,&url).await?;
  let mut conn=serving.pool.acquire().await?;let source_shape=shape_for_profile(pg_profile_any(&mut conn).await?)?;let before=pg_tables_any(&mut conn,&source_shape).await?;drop(conn);
  ensure!(before.values().flat_map(|t|&t.rows).flat_map(|r|r).filter(|c|matches!(c,Cell::Blob(v)if !v.is_empty())).count()>=2,"Actual populated Automerge cells required");
  let backup=capture_restored_postgres(&serving,&destination,true).await?;backup.fence()?;
  let manifest:serde_json::Value=serde_json::from_slice(&backup.manifest)?;
  ensure!(manifest["format_version"]==2&&manifest["source_authority"]["kind"]=="pg_restore"&&manifest.get("engine_generation").is_none()&&manifest.get("engine_envelope_sha256").is_none(),"Restored source authority mislabeled as transfer");
  let raw=read_ledger(&backup.roles["application-rows.json"].held)?;let captured:BTreeMap<String,Table>=serde_json::from_slice(&raw)?;ensure!(captured==before,"Restored PG captured typed cells differ");
  let raw=read_ledger(&backup.roles["engine-control-rows.json"].held)?;let controls:PgSnapshotControls=serde_json::from_slice(&raw)?;ensure!(controls.current==vec![(1,serving.proof.attestation.restore_id.clone())],"Wrong current PGRestore SQL captured");
  let mut conn=serving.pool.acquire().await?;ensure!(pg_tables_any(&mut conn,&source_shape).await?==before,"Backup changed actual editable PG source");drop(conn);serving.fence().await?;
  println!("ACTUAL_RESTORED_PG_BACKUP_CAPTURE_PASS explicit PgRestore source; original SQLite history preserved; no restore authority");Ok(())
 }
}

#[cfg(test)]
mod supported_pg_admission_journey_tests {
 use super::*;
 #[tokio::test]
 #[ignore="explicit fresh qualified capacity source and new private targets required"]
 async fn required_real_partial_direct_backup_restore_admission()->anyhow::Result<()> {
  use crate::storage::attachment_upload::{self as upload,admission::{WriterLease,Producer}};
  let root=PathBuf::from(std::env::var("STARLANE_ENGINE_APP_ROOT")?);
  let backup_path=PathBuf::from(std::env::var("STARLANE_PG_BACKUP_TARGET")?);
  let target=PathBuf::from(std::env::var("STARLANE_PG_RESTORE_ROOT")?);
  let url=std::env::var("STARLANE_ENGINE_PG_URL")?;
  let serving=open_restored_pg_serving(&root,&url).await?;
  let mut conn=serving.pool.acquire().await?;
  ensure!(pg_profile_any(&mut conn).await?==EngineCatalogueProfile::CURRENT,"Fresh ledger profile required");
  let (document_id,space_id,actor_id):(String,String,String)=sqlx::query_as("SELECT id,space_id,created_by FROM documents WHERE deleted_at IS NULL ORDER BY id LIMIT 1").fetch_one(&mut *conn).await?;
  drop(conn);
  let attempt=uuid::Uuid::new_v4().to_string();
  let request=upload::UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"http://127.0.0.1:generated-pg-role-gate".into(),actor_id,space_id,document_id,original_name:"original-partial.bin".into(),mime_type:"application/octet-stream".into()};
  let writer=WriterLease::begin(&root,&attempt,Producer::Direct{request:request.clone()},64)?;
  crate::storage::attachment_capacity::reserve(&serving.pool,&writer,None).await?;
  let original_claim=std::fs::read(root.join(format!(".attachment-upload-admission/{attempt}/claim.json")))?;
  let body=b"original cold partial direct bytes";
  let mut ingest=upload::Ingest::reserve_with_lease(&root,request,64,writer.clone())?;ingest.append(body)?;drop(ingest);drop(writer);
  serving.fence().await?;
  let backup=capture_restored_postgres(&serving,&backup_path,true).await?;backup.fence()?;
  let manifest:PgSnapshotManifest=serde_json::from_slice(&backup.manifest)?;
  ensure!(manifest.format_version==4,"Capacity PgRestore must use format4");
  let capture=manifest.admission_owner_capture.as_ref().context("Actual supported roles absent")?;
  ensure!(capture.scopes.len()==1&&capture.scopes[0].attempt_id==attempt&&capture.scopes[0].original_claim_json.as_bytes()==original_claim,"Original capture provenance differs");
  let report=restore_qualified_postgres(&backup_path,&target,&url,true).await?;
  ensure!(report.compatible_catalogue==EngineCatalogueProfile::CURRENT.catalogue_label(),"Restored target profile differs");
  let restored=open_restored_pg_serving(&target,&url).await?;
  let roles=crate::storage::restoration::linux::acquire_admission_authority(&target,&restored.pool,&attempt,&original_claim).await?;
  roles.check()?;
  ensure!(WriterLease::open_idle(&target,&attempt).is_err(),"Cold adopted roles must not mint original writer/disposal authority");
  let mut conn=restored.pool.acquire().await?;roles.check_in(&mut conn).await?;
  let reservation:(i64,String)=sqlx::query_as("SELECT maximum_bytes,state FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&attempt).fetch_one(&mut *conn).await?;
  ensure!(reservation==(64,"reserved".into()),"Restore lost original reservation charge");
  drop(conn);drop(roles);restored.fence().await?;restored.pool.close().await;serving.pool.close().await;backup.fence()?;
  println!("ACTUAL_PG_PARTIAL_NATIVE_CAPTURE_COLD_RESTORE_FINALIZED_ADMISSION_ORIGINAL_CLAIM_RETAINED_CHARGE_NO_DISPOSAL_PASS");Ok(())
 }
 #[tokio::test]
 #[ignore="requires explicit owned qualified PostgreSQL root and fresh backup/restored destinations"]
 async fn required_real_partial_direct_100mib_backup_restore_admission()->anyhow::Result<()> {
  use crate::storage::attachment_upload::{self as upload,admission::{WriterLease,Producer}};
  let root=PathBuf::from(std::env::var("STARLANE_ENGINE_APP_ROOT")?);
  let backup_path=PathBuf::from(std::env::var("STARLANE_PG_BACKUP_TARGET")?);
  let target=PathBuf::from(std::env::var("STARLANE_PG_RESTORE_ROOT")?);
  let url=std::env::var("STARLANE_ENGINE_PG_URL")?;
  let serving=open_restored_pg_serving(&root,&url).await?;
  let mut conn=serving.pool.acquire().await?;
  ensure!(pg_profile_any(&mut conn).await?==EngineCatalogueProfile::CURRENT,"Fresh ledger profile required");
  let (document_id,space_id,actor_id):(String,String,String)=sqlx::query_as("SELECT id,space_id,created_by FROM documents WHERE deleted_at IS NULL ORDER BY id LIMIT 1").fetch_one(&mut *conn).await?;
  drop(conn);
  let attempt=uuid::Uuid::new_v4().to_string();
  let request=upload::UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"http://127.0.0.1:generated-pg-role-gate".into(),actor_id,space_id,document_id,original_name:"original-partial.bin".into(),mime_type:"application/octet-stream".into()};
  let writer=WriterLease::begin(&root,&attempt,Producer::Direct{request:request.clone()},128*1024*1024)?;
  crate::storage::attachment_capacity::reserve(&serving.pool,&writer,None).await?;
  let original_claim=std::fs::read(root.join(format!(".attachment-upload-admission/{attempt}/claim.json")))?;
  let body=[0x5au8;64*1024];
  let mut ingest=upload::Ingest::reserve_with_lease(&root,request,128*1024*1024,writer.clone())?;for _ in 0..1600 {ingest.append(&body)?;}drop(ingest);drop(writer);
  serving.fence().await?;
  let backup=capture_restored_postgres(&serving,&backup_path,true).await?;backup.fence()?;
  let manifest:PgSnapshotManifest=serde_json::from_slice(&backup.manifest)?;
  ensure!(manifest.format_version==4,"Capacity PgRestore must use format4");
  let capture=manifest.admission_owner_capture.as_ref().context("Actual supported roles absent")?;
  ensure!(capture.scopes.len()==1&&capture.scopes[0].attempt_id==attempt&&capture.scopes[0].original_claim_json.as_bytes()==original_claim,"Original capture provenance differs");
  let report=restore_qualified_postgres(&backup_path,&target,&url,true).await?;
  ensure!(report.compatible_catalogue==EngineCatalogueProfile::CURRENT.catalogue_label(),"Restored target profile differs");
  let restored=open_restored_pg_serving(&target,&url).await?;
  let roles=crate::storage::restoration::linux::acquire_admission_authority(&target,&restored.pool,&attempt,&original_claim).await?;
  roles.check()?;
  ensure!(WriterLease::open_idle(&target,&attempt).is_err(),"Cold adopted roles must not mint original writer/disposal authority");
  let mut conn=restored.pool.acquire().await?;roles.check_in(&mut conn).await?;
  let reservation:(i64,String)=sqlx::query_as("SELECT maximum_bytes,state FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&attempt).fetch_one(&mut *conn).await?;
  ensure!(reservation==(128*1024*1024,"reserved".into()),"Restore lost original reservation charge");
  drop(conn);drop(roles);restored.fence().await?;restored.pool.close().await;serving.pool.close().await;backup.fence()?;
  println!("ACTUAL_PG_100MIB_PARTIAL_NATIVE_CAPTURE_COLD_RESTORE_FINALIZED_ADMISSION_ORIGINAL_CLAIM_RETAINED_CHARGE_NO_DISPOSAL_PASS");Ok(())
 }
}

#[cfg(test)]
mod prior_pg_admission_journey_tests {
 use super::*;
 #[tokio::test]
 #[ignore="explicit genuine prior finalized capacity source and owned targets required"]
 async fn required_real_prior_finalized_direct_backup_restore_admission()->anyhow::Result<()> {
  use crate::storage::attachment_upload::admission::WriterLease;
  let root=PathBuf::from(std::env::var("STARLANE_ENGINE_APP_ROOT")?);let backup_path=PathBuf::from(std::env::var("STARLANE_PG_BACKUP_TARGET")?);let target=PathBuf::from(std::env::var("STARLANE_PG_RESTORE_ROOT")?);let url=std::env::var("STARLANE_ENGINE_PG_URL")?;
  let serving=open_restored_pg_serving(&root,&url).await?;
  ensure!(crate::storage::restoration::linux::has_finalized_admission_capture_source(&root)?,"Source must already have genuine finalized Ledger2 roles");
  let backup=capture_restored_postgres(&serving,&backup_path,true).await?;backup.fence()?;
  let manifest:PgSnapshotManifest=serde_json::from_slice(&backup.manifest)?;ensure!(manifest.format_version==4&&manifest.admission_owner_capture.is_none(),"Prior writer cannot relabel lineage as native v2");
  let evidence=manifest.prior_admission_owner_capture.as_ref().context("Prior supported roles absent")?;evidence.validate_shape()?;
  let scope=evidence.scopes.first().context("Prior role absent")?;let attempt=scope.attempt_id.clone();let claim=scope.original_claim_json.as_bytes().to_vec();let maximum=scope.maximum_bytes as i64;
  ensure!(scope.directories.iter().any(|b|b.original!=b.captured),"Prior capture erased original/adopted lineage");ensure!(WriterLease::open_idle(&root,&attempt).is_err(),"Prior source roles cannot rebind original WriterLease");
  let mut inspected=inspect_postgres_snapshot(&backup_path)?;pg_prior_admission_roles(&inspected)?;
  let saved=inspected.manifest.prior_admission_owner_capture.as_ref().unwrap().ledger_sha256.clone();inspected.manifest.prior_admission_owner_capture.as_mut().unwrap().ledger_sha256="f".repeat(64);ensure!(pg_prior_admission_roles(&inspected).is_err(),"Foreign finalized digest must refuse");inspected.manifest.prior_admission_owner_capture.as_mut().unwrap().ledger_sha256=saved;
  let saved=inspected.manifest.prior_admission_owner_capture.as_ref().unwrap().scopes[0].directories[0].captured.inode;inspected.manifest.prior_admission_owner_capture.as_mut().unwrap().scopes[0].directories[0].captured.inode=saved+1;ensure!(pg_prior_admission_roles(&inspected).is_err(),"Foreign claimed prior source role must refuse");inspected.manifest.prior_admission_owner_capture.as_mut().unwrap().scopes[0].directories[0].captured.inode=saved;
  pg_prior_admission_roles(&inspected)?;drop(inspected);
  let report=restore_qualified_postgres(&backup_path,&target,&url,true).await?;ensure!(report.compatible_catalogue==EngineCatalogueProfile::CURRENT.catalogue_label(),"Prior cold restore profile differs");
  let restored=open_restored_pg_serving(&target,&url).await?;let authority=crate::storage::restoration::linux::acquire_admission_authority(&target,&restored.pool,&attempt,&claim).await?;authority.check()?;let mut conn=restored.pool.acquire().await?;authority.check_in(&mut conn).await?;
  let reservation:(i64,String)=sqlx::query_as("SELECT maximum_bytes,state FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&attempt).fetch_one(&mut *conn).await?;ensure!(reservation==(maximum,"reserved".into()),"Prior cold transport lost retained charge");ensure!(WriterLease::open_idle(&target,&attempt).is_err(),"Prior cold transport cannot mint original writer/disposal authority");
  drop(conn);drop(authority);restored.fence().await?;restored.pool.close().await;serving.pool.close().await;backup.fence()?;
  println!("ACTUAL_PG_PRIOR_FINALIZED_CAPTURE_COLD_RESTORE_EXACT_GENERATION_ORIGINAL_ROLE_RETAINED_CHARGE_NO_REBOUND_WRITER_PASS");Ok(())
 }
}

// Explicit supported-class selection; all original capsule/role leases stay held.
fn pg_prior_supported_evidence(leases:&crate::storage::restoration::linux::PriorRestoredCaptureLeases)->anyhow::Result<Option<crate::storage::restoration::linux::PriorRestoredCaptureEvidence>> {
 use crate::storage::restoration::admission::FileRole as F;
 let mut evidence=leases.lineage_evidence()?;let mut supported=Vec::new();
 let partial=[F::Claim,F::CapsuleRecord{name:"stage-origin.json".into()},F::DirectStage{name:"reservation.json".into()},F::DirectStage{name:"request.json".into()},F::DirectStage{name:"payload.part".into()}];
 for scope in std::mem::take(&mut evidence.scopes) {
  let claim:serde_json::Value=serde_json::from_str(&scope.original_claim_json)?;let producer:crate::storage::attachment_upload::admission::Producer=serde_json::from_value(claim.get("producer").context("Missing authenticated prior producer")?.clone())?;
  match producer {
   crate::storage::attachment_upload::admission::Producer::Direct{..}=>{if scope.files.len()==partial.len()&&partial.iter().all(|role|scope.files.iter().any(|b|&b.role==role)){supported.push(scope);}},
   crate::storage::attachment_upload::admission::Producer::Replay{..}=>{supported.push(scope);}, // Constructor already holds complete exact source/reference proof; no native lease fallback.
  }
 }
 evidence.scopes=supported;if evidence.scopes.is_empty(){return Ok(None)}evidence.validate_shape()?;Ok(Some(evidence))
}

#[cfg(test)]
mod qualified_pg_replay_journey_tests {
 use super::*;
 fn private_dir(p:&Path)->anyhow::Result<()> {use std::os::unix::fs::DirBuilderExt;std::fs::DirBuilder::new().recursive(true).mode(0o700).create(p)?;Ok(())}
 #[tokio::test]
 #[ignore="host trusted interpreter; explicit unique fresh SQLite seed output"]
 async fn required_prepare_own_sqlite51_replay_seed()->anyhow::Result<()> {
  use std::os::unix::fs::{PermissionsExt,OpenOptionsExt};use std::io::Write;
  sqlx::any::install_default_drivers();let bundle=PathBuf::from(std::env::var("STARLANE_REPLAY_SEED_BUNDLE")?);let base=bundle.parent().context("seed parent")?;let source=base.join("source");ensure!(!source.exists()&&!bundle.exists(),"Fresh seed only");private_dir(&source)?;
  let db=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=rwc",source.join("starlane-notes.db").display())).await?;crate::db::selected_migrator(crate::db::DatabaseEngine::Sqlite).run(&db).await?;
  let path="spaces/seed/docs/page.md";private_dir(source.join("spaces/seed/docs").as_path())?;let mut f=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(source.join(path))?;f.write_all(b"Original published source\n")?;f.sync_all()?;drop(f);
  for q in ["INSERT INTO users(id,email,display_name) VALUES('actor','private-fixture@example.invalid','Fixture')","INSERT INTO spaces(id,name,slug,created_by) VALUES('space','Fixture','seed','actor')","INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at,published_at) VALUES('page','space','Fixture','spaces/seed/docs/page.md','actor','actor','original','published')"]{sqlx::query(q).execute(&db).await?;}
  db.close().await;std::fs::set_permissions(source.join("starlane-notes.db"),std::fs::Permissions::from_mode(0o600))?;let captured=crate::storage::restoration::linux::capture_sqlite_certified(&source,&bundle,"v1.5.1",true).await?;captured.fence()?;ensure!(captured.manifest.admission_owner_capture.is_none()&&captured.manifest.prior_admission_owner_capture.is_none(),"Empty seed must omit role authority");captured.db.close().await;println!("OWN_SQLITE51_REAL_CANONICAL_SEED_STANDARD_PRODUCER_NO_ADMISSION_AUTHORITY_PASS");Ok(())
 }
 #[tokio::test]
 #[ignore="explicit own SQLite seed and unique PostgreSQL schema/output roots"]
 async fn required_real_original_replay_backup_cold_restore_finalized()->anyhow::Result<()> {
  use crate::storage::{attachment_upload::admission::{WriterLease,Producer},attachment_capacity,asset_blob};
  let bundle=PathBuf::from(std::env::var("STARLANE_REPLAY_SEED_BUNDLE")?);let root=PathBuf::from(std::env::var("STARLANE_ENGINE_APP_ROOT")?);let artifacts=PathBuf::from(std::env::var("STARLANE_REPLAY_TRANSFER_ARTIFACTS")?);let backup_path=PathBuf::from(std::env::var("STARLANE_PG_BACKUP_TARGET")?);let target=PathBuf::from(std::env::var("STARLANE_PG_RESTORE_ROOT")?);let url=std::env::var("STARLANE_ENGINE_PG_URL")?;
  crate::storage::backup_transfer::run(&bundle,&root,&artifacts,"v1.5.1",&url).await?;let serving=open_engine_serving(&root,&url).await?;
  let attempt=uuid::Uuid::new_v4().to_string();let body=b"original replay binary";let producer=Producer::Replay{draft_scope:None,provider:"opaque-provider".into(),incoming_id:"opaque-incoming".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),asset_id:"opaque-asset".into(),payload_fingerprint:"a".repeat(64),binary_sha256:digest(body),byte_length:body.len()as u64};let writer=WriterLease::begin(&root,&attempt,producer,64)?;attachment_capacity::reserve(&serving.pool,&writer,None).await?;let original_claim=writer.original_claim().to_vec();let blob=asset_blob::create_with_lease(&root,"seed","page","bin",body,writer.clone())?;let blob_path=blob.path().to_owned();
  let variant=std::env::var("STARLANE_PG_REPLAY_VARIANT").unwrap_or_else(|_|"bare".into());
  if variant=="rewrite" {
   use crate::storage::{canonical_rewrite::{RewriteIntent,RewriteReason,IncomingIdentity,PreparedRewrite},journal_fs::FileDescriptor};
   let incoming=IncomingIdentity{provider:"opaque-provider".into(),operation_id:"opaque-incoming".into(),entity_type:"asset".into(),entity_id:"opaque-asset".into(),action:"create".into(),payload_fingerprint:"a".repeat(64)};
   let result=format!("Original source with ![attachment](/api/v1/files/{blob_path})\n");let intent=RewriteIntent{space_id:"space".into(),document_id:"page".into(),source_path:"spaces/seed/docs/page.md".into(),source_revision:"original".into(),published_stamp:"published".into(),revision:"next".into(),source_sha256:Some(digest(b"Original published source\n")),reason:RewriteReason::AssetReferenceRewrite,incoming:incoming.clone()};let op=uuid::Uuid::new_v4().to_string();writer.bind_replay_rewrite_plan(&root,&op,"actor",&intent,result.as_bytes())?;
   let descriptor=FileDescriptor{op_id:op.clone(),space_id:"space".into(),document_id:"page".into(),path:format!("spaces/seed/docs/revisions/{op}.md"),revision:"next".into(),sha256:digest(result.as_bytes()),length:result.len()as i64,version:3};
   let typed:PreparedRewrite=serde_json::from_value(serde_json::json!({"version":3,"kind":"canonical-rewrite","op_id":op,"space_id":"space","document_id":"page","path":descriptor.path,"revision":"next","sha256":descriptor.sha256,"length":descriptor.length,"actor_id":"actor","source_path":"spaces/seed/docs/page.md","source_revision":"original","published_stamp":"published","source_sha256":intent.source_sha256,"reason":"AssetReferenceRewrite","incoming":incoming}))?;
   let raw=serde_json::to_vec(&typed)?;crate::storage::journal_fs::stage_replay_file_with_lease(&root,".canonical-rewrite-journal",&descriptor,&raw,result.as_bytes(),&writer)?;
  }
  if let Ok(ready)=std::env::var("STARLANE_REPLAY_SIGKILL_READY") {
   use std::os::unix::fs::OpenOptionsExt;use std::io::Write;
   let mut marker=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(ready)?;
   marker.write_all(&serde_json::to_vec(&serde_json::json!({"attempt":attempt,"claim":String::from_utf8(original_claim.clone())?,"blob_path":blob_path,"writer_pid":std::process::id()}))?)?;marker.sync_all()?;
   loop{tokio::time::sleep(std::time::Duration::from_secs(1)).await;}
  }
  ensure!(capture_owned_postgres(&serving,&backup_path,true).await.is_err(),"Active original capsule must refuse capture");drop(blob);drop(writer);
  let backup=capture_owned_postgres(&serving,&backup_path,true).await?;backup.fence()?;let manifest:PgSnapshotManifest=serde_json::from_slice(&backup.manifest)?;let capture=manifest.admission_owner_capture.as_ref().context("Actual Replay role producer omitted supported capture")?;ensure!(capture.scopes.iter().any(|s|s.attempt_id==attempt&&s.original_claim_json.as_bytes()==original_claim),"Replay original typed claim differs");
  // Genuine cold acceptance: close original SQL pools and remove the source path.
  serving.pool.close().await;let kept_source=root.with_file_name("closed-original-source-kept");ensure!(!kept_source.exists(),"Fresh kept source required");std::fs::rename(&root,&kept_source)?;ensure!(!root.exists(),"Original source path must be absent");
  restore_qualified_postgres(&backup_path,&target,&url,true).await?;let restored=open_restored_pg_serving(&target,&url).await?;let authority=crate::storage::restoration::linux::acquire_admission_authority(&target,&restored.pool,&attempt,&original_claim).await?;authority.check()?;ensure!(WriterLease::open_idle(&target,&attempt).is_err(),"Copied Replay roles never rebind original writer");ensure!(std::fs::read(target.join(&blob_path))?==body,"Original Replay payload changed");let mut conn=restored.pool.acquire().await?;authority.check_in(&mut conn).await?;let row:(i64,String)=sqlx::query_as("SELECT maximum_bytes,state FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&attempt).fetch_one(&mut *conn).await?;ensure!(row==(64,"reserved".into()),"Replay original charge lost");drop(conn);
  let active_prior=target.with_file_name("active-prior-refused-backup");ensure!(capture_restored_postgres(&restored,&active_prior,true).await.is_err(),"Held finalized capsule authority excludes competing source capture");ensure!(!active_prior.exists(),"Held finalized authority cannot publish prior snapshot");drop(authority);
  for mode in ["canonical-reference","capsule","blob-inode"] {
   use std::os::unix::fs::PermissionsExt;
   let bad_source=target.with_file_name(format!("prior-source-{mode}"));restore_qualified_postgres(&backup_path,&bad_source,&url,true).await?;let bad=open_restored_pg_serving(&bad_source,&url).await?;
   match mode {
    "canonical-reference"=>std::fs::write(bad_source.join("spaces/seed/docs/page.md"),format!("Protected canonical ![asset](/api/v1/files/{blob_path})\n"))?,
    "capsule"=>{let path=bad_source.join(format!(".attachment-upload-admission/{attempt}/replay-blob-origin.json"));let mut raw:serde_json::Value=serde_json::from_slice(&std::fs::read(&path)?)?;raw["payload"]["inode"]=serde_json::json!(999);std::fs::write(path,serde_json::to_vec(&raw)?)?;},
    "blob-inode"=>{let path=bad_source.join(&blob_path);std::fs::rename(&path,bad_source.with_file_name(format!("prior-original-blob-{variant}")))?;std::fs::write(&path,body)?;std::fs::set_permissions(path,std::fs::Permissions::from_mode(0o600))?;},
    _=>unreachable!(),
   }
   let refused=target.with_file_name(format!("prior-source-{mode}-refused-backup"));let result=capture_restored_postgres(&bad,&refused,true).await;ensure!(result.is_err(),"{mode} cannot emit prior Replay transport");ensure!(!refused.exists(),"{mode} cannot publish backup");
   if mode=="canonical-reference"{let error=format!("{:#}",result.as_ref().err().unwrap());ensure!(error.contains("Prior Replay source reference refusal")&&error.contains("protects Replay blob"),"Prior source must reach typed canonical reference proof: {error}");}
   let charge:i64=sqlx::query_scalar("SELECT maximum_bytes FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&attempt).fetch_one(&bad.pool).await?;ensure!(charge==64,"Prior source refusal preserves original charge");bad.pool.close().await;
  }
  let prior_path=target.with_file_name("prior-replay-backup");let prior=capture_restored_postgres(&restored,&prior_path,true).await?;prior.fence()?;let prior_manifest:PgSnapshotManifest=serde_json::from_slice(&prior.manifest)?;ensure!(prior_manifest.admission_owner_capture.is_none()&&prior_manifest.prior_admission_owner_capture.as_ref().is_some_and(|e|e.scopes.iter().any(|s|s.attempt_id==attempt&&s.original_claim_json.as_bytes()==original_claim)),"Replay prior capture must emit distinct exact finalized evidence");restored.pool.close().await;
  let prior_kept=target.with_file_name("original-prior-restored-source-kept");std::fs::rename(&target,&prior_kept)?;ensure!(!target.exists(),"Prior restored source must be absent before repeated cold transport");
  let target=target.with_file_name("second-cold-restored-replay");restore_qualified_postgres(&prior_path,&target,&url,true).await?;let restored=open_restored_pg_serving(&target,&url).await?;let authority=crate::storage::restoration::linux::acquire_admission_authority(&target,&restored.pool,&attempt,&original_claim).await?;authority.check()?;ensure!(std::fs::read(target.join(&blob_path))?==body,"Repeated cold Replay bytes differ");ensure!(crate::storage::attachment_upload::admission::WriterLease::open_idle(&target,&attempt).is_err(),"Repeated cold proof cannot rebind native writer");
  if variant=="rewrite" {
   use std::os::unix::fs::PermissionsExt;
   let scope=capture.scopes.iter().find(|s|s.attempt_id==attempt).context("Captured Replay scope")?;
   let role=scope.files.iter().find(|b|matches!(&b.role,crate::storage::restoration::admission::FileRole::CanonicalRewrite{name,..} if name=="payload")).context("Captured protected rewrite payload role")?;
   let relative=role.role.relative(&attempt)?;let alternate=target.with_file_name("rewrite-foreign-current-restored");restore_qualified_postgres(&backup_path,&alternate,&url,true).await?;
   let other=open_restored_pg_serving(&alternate,&url).await?;let held=crate::storage::restoration::linux::acquire_admission_authority(&alternate,&other.pool,&attempt,&original_claim).await?;
   let current=alternate.join(relative);let original=alternate.parent().unwrap().join("held-original-rewrite-payload");let raw=std::fs::read(&current)?;std::fs::rename(&current,&original)?;std::fs::write(&current,&raw)?;std::fs::set_permissions(&current,std::fs::Permissions::from_mode(0o600))?;
   ensure!(held.check().is_err(),"Equal-byte foreign adopted canonical rewrite payload cannot retain role authority");drop(held);ensure!(crate::storage::restoration::linux::acquire_admission_authority(&alternate,&other.pool,&attempt,&original_claim).await.is_err(),"Fresh reader cannot adopt foreign protected rewrite payload");let charge:i64=sqlx::query_scalar("SELECT maximum_bytes FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&attempt).fetch_one(&other.pool).await?;ensure!(charge==64,"Foreign protected rewrite refusal preserves original charge");other.pool.close().await;
  }
  use std::os::unix::fs::PermissionsExt;let current=target.join(&blob_path);let original=target.parent().unwrap().join("held-original-replay-blob");std::fs::rename(&current,&original)?;std::fs::write(&current,body)?;std::fs::set_permissions(&current,std::fs::Permissions::from_mode(0o600))?;ensure!(authority.check().is_err(),"Equal-byte foreign adopted Replay inode cannot retain role authority");drop(authority);ensure!(crate::storage::restoration::linux::acquire_admission_authority(&target,&restored.pool,&attempt,&original_claim).await.is_err(),"Fresh finalized reader cannot adopt foreign Replay inode");let charged:i64=sqlx::query_scalar("SELECT maximum_bytes FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&attempt).fetch_one(&restored.pool).await?;ensure!(charged==64,"Refusal must retain original reservation charge");restored.pool.close().await;backup.fence()?;println!("ACTUAL_OWN_PG_REPLAY_NATIVE_{}_STANDARD_CAPTURE_SOURCE_ABSENT_COLD_RESTORE_FINALIZED_ORIGINAL_TYPED_CLAIM_BYTES_CHARGE_FOREIGN_CURRENT_INODE_REFUSAL_NO_WRITER_GRANT_PASS",variant);Ok(())
 }
 #[tokio::test]
 #[ignore="own active/killed producer process, unique PG source and destinations"]
 async fn required_actual_replay_sigkill_capture_and_cold_restore()->anyhow::Result<()> {
  let root=PathBuf::from(std::env::var("STARLANE_ENGINE_APP_ROOT")?);let url=std::env::var("STARLANE_ENGINE_PG_URL")?;let backup=PathBuf::from(std::env::var("STARLANE_PG_BACKUP_TARGET")?);let target=PathBuf::from(std::env::var("STARLANE_PG_RESTORE_ROOT")?);let state:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::var("STARLANE_REPLAY_SIGKILL_READY")?)?)?;
  let attempt=state["attempt"].as_str().context("Crash attempt")?;let claim=state["claim"].as_str().context("Crash original claim")?.as_bytes();let path=state["blob_path"].as_str().context("Crash original blob path")?;let serving=open_engine_serving(&root,&url).await?;
  if std::env::var("STARLANE_REPLAY_SIGKILL_ACTIVE_ONLY").is_ok(){ensure!(capture_owned_postgres(&serving,&backup,true).await.is_err(),"Live original process must retain capture exclusion");ensure!(!backup.exists(),"Live original process cannot publish backup");serving.pool.close().await;println!("ACTUAL_REPLAY_LIVE_PROCESS_CAPTURE_REFUSAL_PASS");return Ok(())}
  let captured=capture_owned_postgres(&serving,&backup,true).await?;captured.fence()?;ensure!(serde_json::from_slice::<PgSnapshotManifest>(&captured.manifest)?.admission_owner_capture.as_ref().is_some_and(|c|c.scopes.iter().any(|s|s.attempt_id==attempt&&s.original_claim_json.as_bytes()==claim)),"Killed native original must retain exact typed capture");serving.pool.close().await;std::fs::rename(&root,root.with_file_name("killed-original-source-kept"))?;ensure!(!root.exists(),"Original killed source absent before cold restore");restore_qualified_postgres(&backup,&target,&url,true).await?;let restored=open_restored_pg_serving(&target,&url).await?;let authority=crate::storage::restoration::linux::acquire_admission_authority(&target,&restored.pool,attempt,claim).await?;authority.check()?;ensure!(std::fs::read(target.join(path))?==b"original replay binary","Killed original Replay bytes changed");let charge:(i64,String)=sqlx::query_as("SELECT maximum_bytes,state FROM attachment_capacity_reservations WHERE operation_id=$1").bind(attempt).fetch_one(&restored.pool).await?;ensure!(charge==(64,"reserved".into()),"Killed original Replay charge lost");ensure!(crate::storage::attachment_upload::admission::WriterLease::open_idle(&target,attempt).is_err(),"Killed source proof cannot rebind copied WriterLease");drop(authority);restored.pool.close().await;println!("ACTUAL_REPLAY_SIGKILL_ORIGINAL_NATIVE_SOURCE_ABSENT_COLD_RESTORE_EXACT_CLAIM_BYTES_CHARGE_NO_WRITER_GRANT_PASS");Ok(())
 }

}

#[cfg(test)]
mod qualified_pg_replay_cold_control_tests {
 use super::*;
 fn copy_private(source:&Path,target:&Path)->anyhow::Result<()> {
  use std::os::unix::fs::{DirBuilderExt,PermissionsExt};std::fs::DirBuilder::new().mode(0o700).create(target)?;
  for entry in std::fs::read_dir(source)?{let p=entry?.path();let q=target.join(p.file_name().context("snapshot file name")?);if p.is_dir(){copy_private(&p,&q)?}else{ensure!(p.is_file(),"Only owned regular snapshot files");std::fs::copy(&p,&q)?;std::fs::set_permissions(q,std::fs::Permissions::from_mode(0o600))?;}}Ok(())
 }
 #[tokio::test]
 #[ignore="exact owned accepted Replay snapshot and unique private output roots"]
 async fn required_actual_cold_replay_snapshot_reference_and_origin_controls()->anyhow::Result<()> {
  let original=PathBuf::from(std::env::var("STARLANE_PG_REPLAY_ACCEPTED_BACKUP")?);let base=PathBuf::from(std::env::var("STARLANE_PG_REPLAY_CONTROL_ROOT")?);let url=std::env::var("STARLANE_ENGINE_PG_URL")?;ensure!(!base.exists(),"Fresh owned control root required");use std::os::unix::fs::DirBuilderExt;std::fs::DirBuilder::new().mode(0o700).create(&base)?;
  let original_manifest:serde_json::Value=serde_json::from_slice(&std::fs::read(original.join("manifest.json"))?)?;let field=if original_manifest["admission_owner_capture"].is_object(){"admission_owner_capture"}else{"prior_admission_owner_capture"};let scope=&original_manifest[field]["scopes"][0];let attempt=scope["attempt_id"].as_str().context("Replay attempt")?.to_owned();let claim=scope["original_claim_json"].as_str().context("Replay original claim")?.as_bytes().to_vec();let blob=scope["files"].as_array().context("Replay files")?.iter().find(|v|v["role"]["role"]=="replay_blob").context("Replay blob role")?["role"]["relative"].as_str().context("Replay path")?.to_owned();
  for mode in ["null","origin","canonical-reference"] {
   let copied=base.join(format!("{mode}-snapshot"));copy_private(&original,&copied)?;let mut m=original_manifest.clone();
   match mode {
    "null"=>m[field]=serde_json::Value::Null,
    "origin"=>{
     let path=format!(".attachment-upload-admission/{attempt}/replay-blob-origin.json");let record=copied.join("files").join(&path);let mut origin:serde_json::Value=serde_json::from_slice(&std::fs::read(&record)?)?;origin["payload"]["inode"]=serde_json::json!(999);let raw=serde_json::to_vec(&origin)?;std::fs::write(record,&raw)?;m["files"][&path]["length"]=serde_json::json!(raw.len());m["files"][&path]["sha256"]=serde_json::json!(digest(&raw));
     for b in m[field]["scopes"][0]["files"].as_array_mut().unwrap(){if b["role"]["role"]=="capsule_record"&&b["role"]["name"]=="replay-blob-origin.json"{b["length"]=serde_json::json!(raw.len());b["sha256"]=serde_json::json!(digest(&raw));}}
    }
    "canonical-reference"=>{let path="spaces/seed/docs/page.md";let raw=format!("Protected current canonical ![asset](/api/v1/files/{blob})\n").into_bytes();std::fs::write(copied.join("files").join(path),&raw)?;m["files"][path]["length"]=serde_json::json!(raw.len());m["files"][path]["sha256"]=serde_json::json!(digest(&raw));},
    _=>unreachable!(),
   }
   std::fs::write(copied.join("manifest.json"),serde_json::to_vec(&m)?)?;let target=base.join(format!("{mode}-refused-target"));let result=restore_qualified_postgres(&copied,&target,&url,true).await;ensure!(result.is_err(),"{mode} must not grant Replay custody");
   if mode=="origin"{ensure!(format!("{:#}",result.as_ref().err().unwrap()).contains(if field=="admission_owner_capture"{"custody"}else{"Prior capture differs from exact finalized original/adopted scope"}),"Self-consistent changed checksums must reach typed original custody refusal");}
   if mode=="canonical-reference"{ensure!(format!("{:#}",result.as_ref().err().unwrap()).contains("protects Replay blob"),"Self-consistent canonical body must reach protected-reference refusal");}
   ensure!(open_restored_pg_serving(&target,&url).await.is_err(),"Refused cold target must not serve");
  }
  let copied=base.join("foreign-held-snapshot");copy_private(&original,&copied)?;let snapshot=inspect_postgres_snapshot(&copied)?;let payload=copied.join("files").join(&blob);let raw=std::fs::read(&payload)?;std::fs::rename(&payload,base.join("original-held-blob"))?;std::fs::write(&payload,&raw)?;use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&payload,std::fs::Permissions::from_mode(0o600))?;ensure!(snapshot.held.fence().is_err(),"Held captured foreign equal-byte inode must refuse");ensure!(prepare_postgres_restore(snapshot,&base.join("foreign-refused-target")).is_err(),"No prepared destination from substituted held snapshot");
  let omitted=base.join("omitted-authority-snapshot");copy_private(&original,&omitted)?;let mut m=original_manifest;m.as_object_mut().unwrap().remove(field);std::fs::write(omitted.join("manifest.json"),serde_json::to_vec(&m)?)?;let target=base.join("omitted-role-restored");restore_qualified_postgres(&omitted,&target,&url,true).await?;let restored=open_restored_pg_serving(&target,&url).await?;ensure!(crate::storage::restoration::linux::acquire_admission_authority(&target,&restored.pool,&attempt,&claim).await.is_err(),"Missing capture is preservation only");ensure!(crate::storage::attachment_upload::admission::WriterLease::open_idle(&target,&attempt).is_err(),"Missing capture cannot construct original writer");let charge:(i64,String)=sqlx::query_as("SELECT maximum_bytes,state FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&attempt).fetch_one(&restored.pool).await?;ensure!(charge==(64,"reserved".into()),"Missing transport must retain original charge");ensure!(std::fs::read(target.join(&blob))?==raw,"Missing transport must preserve original blob bytes");restored.pool.close().await;
  println!("ACTUAL_PG_REPLAY_COLD_NULL_SELFCONSISTENT_ORIGIN_PROTECTED_CANONICAL_REF_HELD_FOREIGN_INODE_REFUSALS_OMISSION_PRESERVES_BYTES_CHARGE_NO_ROLE_WRITER_PASS");Ok(())
 }
}

include!("engine_postgres_verification_source.rs");
