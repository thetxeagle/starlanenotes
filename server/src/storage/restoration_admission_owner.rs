
// Admission owner bridge. Source roles must originate from retained native proof classification.
// Generic owns is unchanged; this bridge does not enable restored disposal.
struct CertifiedAdmissionDestination {roles:admission::AdmissionOwnerEnvelope,source:admission::ClassifiedAdmissionSource}
fn admission_certify_destination(bundle:&crate::storage::backup_inventory::ValidatedBundle,d:&PlannedDestination,source:admission::ClassifiedAdmissionSource)->io::Result<CertifiedAdmissionDestination> {
    bundle.fence()?;d.fence()?;source.fence()?;
    let root=&bundle.manifest.source_custody.root;
    let mut roles=source.source_roles().clone();
    for scope in &mut roles.scopes {
        for b in &mut scope.directories {
            let captured=if let Some(path)=b.role.relative(&scope.attempt_id)? {
                let id=bundle.manifest.source_custody.directories.0.get(&path).ok_or_else(||fail("Admission captured directory custody missing"))?;
                if b.captured!=(admission::Identity{device:id.device,inode:id.inode}) {return Err(fail("Admission source directory capture differs"));}
                let (held,stamp)=d.directories.get(&path).ok_or_else(||fail("Certified admission destination directory missing"))?;
                if identity(held)?!=(stamp.device,stamp.inode)||held.metadata()?.mode()&0o777!=0o700 {return Err(fail("Certified admission destination directory changed"));}
                admission::Identity{device:stamp.device,inode:stamp.inode}
            } else {
                if b.captured!=(admission::Identity{device:root.device,inode:root.inode}) {return Err(fail("Admission source root capture differs"));}
                admission::Identity{device:d.root_stamp.device,inode:d.root_stamp.inode}
            };
            b.adopted=captured;
        }
        for b in &mut scope.files {
            let path=b.role.relative(&scope.attempt_id)?;
            let original=bundle.manifest.source_custody.files.0.get(&path).ok_or_else(||fail("Admission captured file custody missing"))?;
            let bytes=bundle.manifest.files.0.get(&path).ok_or_else(||fail("Admission captured file bytes missing"))?;
            if b.captured!=(admission::Identity{device:original.device,inode:original.inode})||b.length!=bytes.size_bytes||b.sha256!=bytes.sha256 {return Err(fail("Admission source file capture differs"));}
            let adopted=d.leaves.get(&path).ok_or_else(||fail("Certified admission destination file missing"))?;
            if adopted.bytes!=*bytes||identity(&adopted.held)?!=(adopted.stamp.device,adopted.stamp.inode) {return Err(fail("Certified admission destination file changed"));}
            b.adopted=admission::Identity{device:adopted.stamp.device,inode:adopted.stamp.inode};
        }
    }
    roles.validate(&admission::Identity{device:d.root_stamp.device,inode:d.root_stamp.inode})?;
    source.fence()?;bundle.fence()?;d.fence()?;
    Ok(CertifiedAdmissionDestination{roles,source})
}
pub(super) async fn admission_source_sql(conn:&mut sqlx::AnyConnection,source:&admission::ClassifiedAdmissionSource)->io::Result<()> {
    let keys:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_capacity_reservations ORDER BY operation_id").fetch_all(&mut *conn).await.map_err(|_|fail("Cannot inspect admission source reservation inventory"))?;
    let mut expected:Vec<String>=source.reservations.iter().map(|r|r.operation_id.clone()).collect();expected.sort();
    if keys!=expected {return Err(fail("Admission source SQL inventory differs"));}
    for expected in &source.reservations {
        let r=sqlx::query("SELECT operation_id,request_sha256,admission_claim_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,disposition_json,disposition_sha256 FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&expected.operation_id).fetch_one(&mut *conn).await.map_err(|_|fail("Cannot read full admission reservation"))?;
        let row=admission::CapturedReservation{operation_id:r.try_get("operation_id").map_err(|_|fail("Invalid admission SQL operation"))?,request_sha256:r.try_get("request_sha256").map_err(|_|fail("Invalid admission SQL request"))?,admission_claim_sha256:r.try_get("admission_claim_sha256").map_err(|_|fail("Invalid admission SQL claim"))?,maximum_bytes:r.try_get("maximum_bytes").map_err(|_|fail("Invalid admission SQL maximum"))?,state:r.try_get("state").map_err(|_|fail("Invalid admission SQL state"))?,committed_asset_id:r.try_get("committed_asset_id").map_err(|_|fail("Invalid admission SQL asset"))?,committed_bytes:r.try_get("committed_bytes").map_err(|_|fail("Invalid admission SQL committed bytes"))?,disposition_json:r.try_get("disposition_json").map_err(|_|fail("Invalid admission SQL disposition"))?,disposition_sha256:r.try_get("disposition_sha256").map_err(|_|fail("Invalid admission SQL disposition hash"))?};
        if row!=*expected {return Err(fail("Admission full source reservation differs"));}
    } source.completed_sql(conn).await?;source.fence()
}

/// Separate non-serving proof: never enters generic owns, installation, or disposal automatically.
/// Every role remains pinned to the original sealed scope and current finalized generation.
pub(crate) struct AdmissionRoleAuthority {
    active:Arc<Active>,scope:admission::AdmissionOwnerScope,
    directories:std::collections::BTreeMap<admission::DirectoryRole,File>,
    files:std::collections::BTreeMap<admission::FileRole,File>,
}
impl AdmissionRoleAuthority {
    pub(crate) fn check(&self)->io::Result<()> {
        self.active.check()?;
        for b in &self.scope.directories {
            let held=self.directories.get(&b.role).ok_or_else(||fail("Held admission role directory missing"))?;
            let fresh=if let Some(path)=b.role.relative(&self.scope.attempt_id)? {self.active.root.directory(&path)?}else{self.active.root.fd().try_clone()?};
            if identity(held)?!=(b.adopted.device,b.adopted.inode)||identity(&fresh)?!=identity(held)? {return Err(fail("Finalized admission directory role changed"));}
        }
        for b in &self.scope.files {
            let held=self.files.get(&b.role).ok_or_else(||fail("Held admission role file missing"))?;
            let fresh=self.active.root.leaf(&b.role.relative(&self.scope.attempt_id)?)?;
            let m=fresh.metadata()?;
            if identity(held)?!=(b.adopted.device,b.adopted.inode)||identity(&fresh)?!=identity(held)?||m.mode()&0o777!=0o600||m.uid()!=unsafe{libc::geteuid()}
                ||hash_file(held,b.length)?!=b.sha256||hash_file(&fresh,b.length)?!=b.sha256 {return Err(fail("Finalized admission file role changed"));}
        }
        self.active.check()
    }
    pub(crate) async fn check_in(&self,conn:&mut sqlx::AnyConnection)->io::Result<()> {
        self.check()?;
        let (rows,pointer)=history_rows(conn).await?;
        let source=if self.active.engine.is_some(){HistorySource::new_with_check(&self.active.root,None,engine_transfer::engine_root_fence)?}else{HistorySource::new(&self.active.root,None)?};
        let closure=history_closure(source,identity(self.active.root.fd())?,Some((&rows,&pointer)))?;
        if closure.current_id!=self.active.ledger.generation_id||digest(&closure.generations[&closure.current_id].raw)!=self.active.ledger_sha256 {return Err(fail("Admission owner generation SQL differs"));}
        closure.source.fence()?;
        let expected=admission::CapturedReservation{operation_id:self.scope.attempt_id.clone(),request_sha256:self.scope.request_sha256.clone(),admission_claim_sha256:self.scope.admission_claim_sha256.clone(),maximum_bytes:self.scope.maximum_bytes as i64,state:"reserved".into(),committed_asset_id:None,committed_bytes:None,disposition_json:None,disposition_sha256:None};
        let r=sqlx::query("SELECT request_sha256,admission_claim_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,disposition_json,disposition_sha256 FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&self.scope.attempt_id).fetch_one(&mut *conn).await.map_err(|_|fail("Admission owner SQL reservation missing"))?;
        let actual: (String,String,i64,String,Option<String>,Option<i64>,Option<String>,Option<String>)=(r.try_get(0).map_err(|_|fail("Invalid admission request"))?,r.try_get(1).map_err(|_|fail("Invalid admission claim"))?,r.try_get(2).map_err(|_|fail("Invalid admission max"))?,r.try_get(3).map_err(|_|fail("Invalid admission state"))?,r.try_get(4).map_err(|_|fail("Invalid admission asset"))?,r.try_get(5).map_err(|_|fail("Invalid admission committed bytes"))?,r.try_get(6).map_err(|_|fail("Invalid admission disposition"))?,r.try_get(7).map_err(|_|fail("Invalid admission disposition hash"))?);
        if actual!=(expected.request_sha256,expected.admission_claim_sha256,expected.maximum_bytes,expected.state,None,None,None,None) {return Err(fail("Finalized admission full SQL claim/state differs"));}
        if history_rows(conn).await?!=(rows,pointer) {return Err(fail("Admission generation SQL changed during read"));}self.check()
    }
    pub(crate) fn directory(&self,role:&admission::DirectoryRole,original:&admission::Identity)->io::Result<File> {
        self.check()?;
        let b=self.scope.directories.iter().find(|b|&b.role==role).ok_or_else(||fail("Admission directory role absent"))?;
        if &b.original!=original {return Err(fail("Admission original directory proof differs"));}
        self.directories[role].try_clone()
    }
    pub(crate) fn file(&self,role:&admission::FileRole,original:&admission::Identity)->io::Result<File> {
        self.check()?;
        let b=self.scope.files.iter().find(|b|&b.role==role).ok_or_else(||fail("Admission file role absent"))?;
        if &b.original!=original {return Err(fail("Admission original file proof differs"));}
        self.files[role].try_clone()
    }
}
async fn acquire_admission_authority_in(path:&Path,conn:&mut sqlx::AnyConnection,attempt:&str,original_claim:&[u8])->io::Result<AdmissionRoleAuthority> {
    if !valid_id(attempt) {return Err(fail("Invalid admission owner attempt"));}
    let (_,active)=registered(path)?;let active=active.ok_or_else(||fail("Admission lacks registered finalized owner"))?;
    if active.ledger.version!=2 {return Err(fail("Legacy owner grants no admission authority"));}
    let scope=active.ledger.admission.as_ref().ok_or_else(||fail("Admission owner envelope missing"))?.scopes.iter().find(|s|s.attempt_id==attempt).ok_or_else(||fail("Admission owner scope missing"))?.clone();
    if scope.original_claim_json.as_bytes()!=original_claim||!matches!(scope.reservation,admission::ReservationState::Reserved) {return Err(fail("Admission original claim or classification differs"));}
    let mut directories=std::collections::BTreeMap::new();
    for b in &scope.directories {
        let f=if let Some(path)=b.role.relative(attempt)? {active.root.directory(&path)?}else{active.root.fd().try_clone()?};
        if identity(&f)?!=(b.adopted.device,b.adopted.inode) {return Err(fail("Admission adopted directory differs"));}
        directories.insert(b.role.clone(),f);
    }
    let capsule=directories.get(&admission::DirectoryRole::AdmissionCapsule).ok_or_else(||fail("Admission capsule role absent"))?;
    // SAFETY: original retained descriptor; copied/foreign directory cannot bypass sealed role ID.
    if unsafe{libc::flock(capsule.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}<0 {return Err(io::Error::last_os_error());}
    let mut files=std::collections::BTreeMap::new();
    for b in &scope.files {files.insert(b.role.clone(),active.root.leaf(&b.role.relative(attempt)?)?);}
    let proof=AdmissionRoleAuthority{active,scope,directories,files};
    proof.check_in(conn).await?;Ok(proof)
}
pub(crate) async fn acquire_admission_authority(path:&Path,db:&AnyPool,attempt:&str,original_claim:&[u8])->io::Result<AdmissionRoleAuthority> {
 let mut conn=db.acquire().await.map_err(|_|fail("Cannot inspect admission finalized SQL"))?;
 acquire_admission_authority_in(path,&mut conn,attempt,original_claim).await
}

#[cfg(test)]
pub(crate) mod admission_owner_engine_tests {
 use super::*;use sqlx::Connection;
 fn private_dir(p:&Path){use std::os::unix::fs::DirBuilderExt;std::fs::DirBuilder::new().recursive(true).mode(0o700).create(p).unwrap();use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(p,std::fs::Permissions::from_mode(0o700)).unwrap();}
 fn private_write(p:&Path,raw:&[u8]){private_dir(p.parent().unwrap());use std::os::unix::fs::OpenOptionsExt;use std::io::Write;let mut f=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(p).unwrap();f.write_all(raw).unwrap();f.sync_all().unwrap();}
 async fn pool(url:&str,label:&str)->AnyPool {
  let pg=url.starts_with("postgres");let schema=format!("admission_owner_{}_{}",label,uuid::Uuid::new_v4().simple());
  if pg {let mut c=sqlx::PgConnection::connect(url).await.unwrap();sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&mut c).await.unwrap();c.close().await.unwrap();}
  let p=sqlx::any::AnyPoolOptions::new().max_connections(2).after_connect(move|c,_|{let schema=schema.clone();Box::pin(async move{if pg {sqlx::query(&format!("SET search_path TO {schema}")).execute(c).await?;}else{sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;}Ok(())})}).connect(url).await.unwrap();
  let engine=crate::db::DatabaseEngine::from_url(url).unwrap();crate::db::selected_migrator(engine).run(&p).await.unwrap();p
 }
 // Test-only finalized reader fixture. Production ownership comes only from certified publisher.
 pub(crate) async fn fixture_finalized(root:&Path,db:&AnyPool,mut roles:admission::AdmissionOwnerEnvelope,source_root:admission::Identity)->String {
  let r=Root::open(root).unwrap();let target=identity(r.fd()).unwrap();
  for s in &mut roles.scopes {for b in &mut s.directories {let f=if let Some(path)=b.role.relative(&s.attempt_id).unwrap(){r.directory(&path).unwrap()}else{r.fd().try_clone().unwrap()};let id=identity(&f).unwrap();b.adopted=admission::Identity{device:id.0,inode:id.1};}for b in &mut s.files{let f=r.leaf(&b.role.relative(&s.attempt_id).unwrap()).unwrap();let id=identity(&f).unwrap();b.adopted=admission::Identity{device:id.0,inode:id.1};}}
  let gid=uuid::Uuid::new_v4().to_string();let base=format!("{NAMESPACE}/generations/{gid}");
  let runtime=Ledger{ historical_preservation:None, composite:None,completed_absence:None,version:2,generation_id:gid.clone(),backup_id:uuid::Uuid::new_v4().to_string(),manifest_sha256:digest(b"reader fixture captured manifest"),database_sha256:digest(b"reader fixture input database"),root_device:target.0,root_inode:target.1,scopes:vec![],admission:Some(roles)};validate(&runtime).unwrap();
  let marker_bytes=format!("starlane private copy {}\n",runtime.backup_id);let marker=root.join(DESTINATION_MARKER);private_write(&marker,marker_bytes.as_bytes());let marker_id=identity(&File::open(&marker).unwrap()).unwrap();
  let intent=HistoryIntent{version:1,generation_id:gid.clone(),backup_id:runtime.backup_id.clone(),manifest_sha256:runtime.manifest_sha256.clone(),database_sha256:runtime.database_sha256.clone(),source_root:crate::storage::backup_inventory::Identity{device:source_root.device,inode:source_root.inode},target_root:crate::storage::backup_inventory::Identity{device:target.0,inode:target.1},retired_marker:HistoryArtifact{relative:format!("{base}/retired-marker"),sha256:digest(marker_bytes.as_bytes())},runtime_sha256:digest(&serde_json::to_vec(&runtime).unwrap()),parent:None,archives:vec![],denied:vec![],proofs:vec![]};
  let ri=serde_json::to_vec(&intent).unwrap();let completion=HistoryCompletion{version:1,generation_id:gid.clone(),backup_id:runtime.backup_id.clone(),manifest_sha256:runtime.manifest_sha256.clone(),database_sha256:runtime.database_sha256.clone(),intent_sha256:digest(&ri),runtime_sha256:intent.runtime_sha256.clone(),root_device:target.0.to_string(),root_inode:target.1.to_string(),state:"committed".into(),created_at:"2026-10-11T00:00:00+00:00".into()};let rc=serde_json::to_vec(&completion).unwrap();
  let finalization=HistoryFinalization{version:1,generation_id:gid.clone(),backup_id:runtime.backup_id.clone(),target_root:intent.target_root.clone(),original_marker:crate::storage::backup_inventory::Identity{device:marker_id.0,inode:marker_id.1},retired_marker:intent.retired_marker.clone(),intent_sha256:digest(&ri),completion_sha256:digest(&rc)};let rf=serde_json::to_vec(&finalization).unwrap();
  let envelope=HistoryEnvelope{version:3,runtime:runtime.clone(),intent_sha256:digest(&ri),completion_sha256:digest(&rc),finalization:Some(HistoryArtifact{relative:format!("{base}/finalization.json"),sha256:digest(&rf)}),parent:None,archives:vec![],denied:vec![],proofs:vec![]};let raw=serde_json::to_vec(&envelope).unwrap();
  private_write(&root.join(format!("{base}/intent.json")),&ri);private_write(&root.join(format!("{base}/completion.json")),&rc);private_write(&root.join(format!("{base}/finalization.json")),&rf);private_write(&root.join(format!("{NAMESPACE}/current.json")),&raw);
  std::fs::rename(marker,root.join(format!("{base}/retired-marker"))).unwrap();
  sqlx::query("INSERT INTO storage_restore_generations(generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,'committed',$8)").bind(&gid).bind(&runtime.backup_id).bind(&runtime.manifest_sha256).bind(&runtime.database_sha256).bind(digest(&raw)).bind(target.0.to_string()).bind(target.1.to_string()).bind(&completion.created_at).execute(db).await.unwrap();sqlx::query("INSERT INTO storage_restore_current(singleton,generation_id) VALUES(1,$1)").bind(&gid).execute(db).await.unwrap();gid
 }
 pub(crate) async fn fixture_finalized_preservation(root:&Path,db:&AnyPool,source_root:admission::Identity)->String {
  let r=Root::open(root).unwrap();let target=identity(r.fd()).unwrap();
  let gid=uuid::Uuid::new_v4().to_string();let base=format!("{NAMESPACE}/generations/{gid}");
  let runtime=Ledger{ historical_preservation:None, composite:None,completed_absence:None,version:1,generation_id:gid.clone(),backup_id:uuid::Uuid::new_v4().to_string(),manifest_sha256:digest(b"reader fixture captured manifest"),database_sha256:digest(b"reader fixture input database"),root_device:target.0,root_inode:target.1,scopes:vec![],admission:None};validate(&runtime).unwrap();
  let marker_bytes=format!("starlane private copy {}\n",runtime.backup_id);let marker=root.join(DESTINATION_MARKER);private_write(&marker,marker_bytes.as_bytes());let marker_id=identity(&File::open(&marker).unwrap()).unwrap();
  let intent=HistoryIntent{version:1,generation_id:gid.clone(),backup_id:runtime.backup_id.clone(),manifest_sha256:runtime.manifest_sha256.clone(),database_sha256:runtime.database_sha256.clone(),source_root:crate::storage::backup_inventory::Identity{device:source_root.device,inode:source_root.inode},target_root:crate::storage::backup_inventory::Identity{device:target.0,inode:target.1},retired_marker:HistoryArtifact{relative:format!("{base}/retired-marker"),sha256:digest(marker_bytes.as_bytes())},runtime_sha256:digest(&serde_json::to_vec(&runtime).unwrap()),parent:None,archives:vec![],denied:vec![],proofs:vec![]};
  let ri=serde_json::to_vec(&intent).unwrap();let completion=HistoryCompletion{version:1,generation_id:gid.clone(),backup_id:runtime.backup_id.clone(),manifest_sha256:runtime.manifest_sha256.clone(),database_sha256:runtime.database_sha256.clone(),intent_sha256:digest(&ri),runtime_sha256:intent.runtime_sha256.clone(),root_device:target.0.to_string(),root_inode:target.1.to_string(),state:"committed".into(),created_at:"2026-10-11T00:00:00+00:00".into()};let rc=serde_json::to_vec(&completion).unwrap();
  let finalization=HistoryFinalization{version:1,generation_id:gid.clone(),backup_id:runtime.backup_id.clone(),target_root:intent.target_root.clone(),original_marker:crate::storage::backup_inventory::Identity{device:marker_id.0,inode:marker_id.1},retired_marker:intent.retired_marker.clone(),intent_sha256:digest(&ri),completion_sha256:digest(&rc)};let rf=serde_json::to_vec(&finalization).unwrap();
  let envelope=HistoryEnvelope{version:3,runtime:runtime.clone(),intent_sha256:digest(&ri),completion_sha256:digest(&rc),finalization:Some(HistoryArtifact{relative:format!("{base}/finalization.json"),sha256:digest(&rf)}),parent:None,archives:vec![],denied:vec![],proofs:vec![]};let raw=serde_json::to_vec(&envelope).unwrap();
  private_write(&root.join(format!("{base}/intent.json")),&ri);private_write(&root.join(format!("{base}/completion.json")),&rc);private_write(&root.join(format!("{base}/finalization.json")),&rf);private_write(&root.join(format!("{NAMESPACE}/current.json")),&raw);
  std::fs::rename(marker,root.join(format!("{base}/retired-marker"))).unwrap();
  sqlx::query("INSERT INTO storage_restore_generations(generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,'committed',$8)").bind(&gid).bind(&runtime.backup_id).bind(&runtime.manifest_sha256).bind(&runtime.database_sha256).bind(digest(&raw)).bind(target.0.to_string()).bind(target.1.to_string()).bind(&completion.created_at).execute(db).await.unwrap();sqlx::query("INSERT INTO storage_restore_current(singleton,generation_id) VALUES(1,$1)").bind(&gid).execute(db).await.unwrap();gid
 }
 #[tokio::test]
 #[ignore="requires owned isolated SQLite/PostgreSQL fixtures"]
 async fn required_finalized_ledger1_preservation_engine() {
  sqlx::any::install_default_drivers();let url=std::env::var("STARLANE_CAPACITY_TEST_URL").unwrap();let pg=url.starts_with("postgres");let db=pool(&url,"preservation").await;
  let base=std::env::temp_dir().join(format!("ledger1-preservation-{}",uuid::Uuid::new_v4()));let src=base.join("source");let dst=base.join("restored");private_dir(&src);private_dir(&dst);
  use crate::storage::{attachment_upload as upload,attachment_capacity as capacity};let attempt=uuid::Uuid::new_v4().to_string();let req=upload::UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"browser".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),original_name:"unknown.bin".into(),mime_type:"application/octet-stream".into()};
  let writer=upload::admission::WriterLease::begin(&src,&attempt,upload::admission::Producer::Direct{request:req},64).unwrap();capacity::reserve(&db,&writer,None).await.unwrap();let claim=writer.original_claim().to_vec();drop(writer);
  let cap=dst.join(format!(".attachment-upload-admission/{attempt}"));private_write(&cap.join("claim.json"),&claim);private_write(&cap.join("future-evidence.dat"),b"opaque original evidence");
  let mut c=db.acquire().await.unwrap();let rows=prior_capture_rows(&mut c).await.unwrap();assert!(FinalizedAdmissionPreservationLeases::hold(&dst,&mut c,&rows).await.is_err(),"Unregistered copied capsule refuses");drop(c);
  let id=identity(&File::open(&src).unwrap()).unwrap();let gid=fixture_finalized_preservation(&dst,&db,admission::Identity{device:id.0,inode:id.1}).await;let finalization=dst.join(format!("{NAMESPACE}/generations/{gid}/finalization.json"));let held_finalization=base.join("original-finalization");std::fs::rename(&finalization,&held_finalization).unwrap();assert!(activate_in(&dst,&db).await.is_err(),"Missing finalized generation refuses");std::fs::rename(&held_finalization,&finalization).unwrap();activate_in(&dst,&db).await.unwrap();
  let mut c=db.acquire().await.unwrap();assert!(FinalizedAdmissionPreservationLeases::hold(&dst,&mut c,&[]).await.is_err(),"Incomplete inventory refuses");let mut duplicate=rows.clone();duplicate.push(rows[0].clone());assert!(FinalizedAdmissionPreservationLeases::hold(&dst,&mut c,&duplicate).await.is_err());
  private_write(&cap.join("stage-origin.json"),b"{}");assert!(FinalizedAdmissionPreservationLeases::hold(&dst,&mut c,&rows).await.is_err(),"Known malformed proof refuses despite preservation-only classification");std::fs::remove_file(cap.join("stage-origin.json")).unwrap();
  let live=Root::open(&dst).unwrap().directory(&format!(".attachment-upload-admission/{attempt}")).unwrap();assert_eq!(unsafe{libc::flock(live.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)},0);assert!(FinalizedAdmissionPreservationLeases::hold(&dst,&mut c,&rows).await.is_err(),"Existing capsule exclusion refuses");drop(live);
  let held=FinalizedAdmissionPreservationLeases::hold(&dst,&mut c,&rows).await.unwrap();held.recheck_in(&mut c).await.unwrap();drop(c);
  assert!(upload::admission::WriterLease::open_idle(&dst,&attempt).is_err(),"Copied claim never becomes original lease");assert!(acquire_admission_authority(&dst,&db,&attempt,&claim).await.is_err(),"Ledger1 has no admission ownership");
  let competitor=Root::open(&dst).unwrap().directory(&format!(".attachment-upload-admission/{attempt}")).unwrap();assert!(unsafe{libc::flock(competitor.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}<0);drop(competitor);
  sqlx::query("UPDATE attachment_capacity_reservations SET maximum_bytes=65 WHERE operation_id=$1").bind(&attempt).execute(&db).await.unwrap();let mut c=db.acquire().await.unwrap();assert!(held.recheck_in(&mut c).await.is_err(),"Full claim SQL mutation refuses");drop(c);sqlx::query("UPDATE attachment_capacity_reservations SET maximum_bytes=64 WHERE operation_id=$1").bind(&attempt).execute(&db).await.unwrap();
  let record=cap.join("future-evidence.dat");let old=base.join("original-evidence");std::fs::rename(&record,&old).unwrap();private_write(&record,b"opaque original evidence");assert!(held.fence().is_err(),"Equal-byte foreign opaque inode invalidates held preservation custody");std::fs::remove_file(&record).unwrap();std::fs::rename(&old,&record).unwrap();assert!(held.fence().is_err(),"Original inode return does not repair captured stamps");drop(held);
  let mut c=db.acquire().await.unwrap();let held=FinalizedAdmissionPreservationLeases::hold(&dst,&mut c,&rows).await.unwrap();drop(c);sqlx::query("DELETE FROM storage_restore_current").execute(&db).await.unwrap();let mut c=db.acquire().await.unwrap();assert!(held.recheck_in(&mut c).await.is_err(),"Missing current generation refuses");drop(c);drop(held);
  assert_eq!(std::fs::read(cap.join("claim.json")).unwrap(),claim);let charged:i64=sqlx::query_scalar("SELECT maximum_bytes FROM attachment_capacity_reservations WHERE operation_id=$1").bind(&attempt).fetch_one(&db).await.unwrap();assert_eq!(charged,64);db.close().await;
  println!("FINALIZED_LEDGER1_PRESERVATION_OPAQUE_FULL_SQL_KERNEL_FOREIGN_INODE_NO_OWNER_CHARGE_RETAINED_PASS {}",if pg{"Postgres"}else{"SQLite"});
 }

 #[tokio::test]
 #[ignore="requires owned isolated SQLite/PostgreSQL fixtures"]
 async fn required_finalized_admission_reader_engine() {
  sqlx::any::install_default_drivers();let url=std::env::var("STARLANE_CAPACITY_TEST_URL").unwrap();let pg=url.starts_with("postgres");
  let target_url=if pg{url.clone()}else{url.replace(".db?","-target.db?")};let source=pool(&url,"source").await;let target=pool(&target_url,"target").await;
  let base=std::env::temp_dir().join(format!("admission-final-reader-{}",uuid::Uuid::new_v4()));let src=base.join("source");let dst=base.join("target");private_dir(&src);private_dir(&dst);
  use crate::storage::{attachment_upload as upload,attachment_capacity as capacity};let attempt=uuid::Uuid::new_v4().to_string();let req=upload::UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"browser".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),original_name:"x.bin".into(),mime_type:"application/octet-stream".into()};let writer=upload::admission::WriterLease::begin(&src,&attempt,upload::admission::Producer::Direct{request:req.clone()},64).unwrap();capacity::reserve(&source,&writer,None).await.unwrap();let mut ingest=upload::Ingest::reserve_with_lease(&src,req,64,writer.clone()).unwrap();ingest.append(b"original partial bytes").unwrap();let claim=writer.original_claim().to_vec();let request_sha=writer.producer_sha256().to_owned();let claim_sha=writer.claim_sha256();drop(ingest);drop(writer);
  let expected=admission::CapturedReservation{operation_id:attempt.clone(),request_sha256:request_sha.clone(),admission_claim_sha256:claim_sha.clone(),maximum_bytes:64,state:"reserved".into(),committed_asset_id:None,committed_bytes:None,disposition_json:None,disposition_sha256:None};let mut tx=source.begin().await.unwrap();let leases=admission::capture::NativeCaptureLeases::hold(&src,&mut *tx,&[expected]).await.unwrap();let original=leases.classify(&mut tx).await.unwrap();let roles=original.source_roles().clone();
  for s in &roles.scopes {for b in &s.directories {if let Some(path)=b.role.relative(&attempt).unwrap(){private_dir(&dst.join(path));}}for b in &s.files {let path=b.role.relative(&attempt).unwrap();private_write(&dst.join(&path),&std::fs::read(src.join(&path)).unwrap());}}
  sqlx::query("INSERT INTO attachment_capacity_reservations(operation_id,request_sha256,admission_claim_sha256,maximum_bytes,state) VALUES($1,$2,$3,64,'reserved')").bind(&attempt).bind(&request_sha).bind(&claim_sha).execute(&target).await.unwrap();
  assert!(acquire_admission_authority(&dst,&target,&attempt,&claim).await.is_err(),"unregistered copied payload grants nothing");
  let original_root=roles.scopes[0].directories.iter().find(|b|b.role==admission::DirectoryRole::DataRoot).unwrap().original.clone();let gid=fixture_finalized(&dst,&target,roles,original_root).await;
  let finalization=dst.join(format!("{NAMESPACE}/generations/{gid}/finalization.json"));let kept=base.join("held-finalization.json");std::fs::rename(&finalization,&kept).unwrap();assert!(activate_in(&dst,&target).await.is_err(),"missing finalized proof refuses");std::fs::rename(&kept,&finalization).unwrap();
  let diagnostic=Root::open(&dst).unwrap();let mut conn=target.acquire().await.unwrap();let (rows,pointer)=history_rows(&mut conn).await.unwrap();history_closure(HistorySource::new(&diagnostic,None).unwrap(),identity(diagnostic.fd()).unwrap(),Some((&rows,&pointer))).unwrap();drop(conn);Active::load(diagnostic).unwrap();
  activate_in(&dst,&target).await.unwrap();let authority=acquire_admission_authority(&dst,&target,&attempt,&claim).await.unwrap();authority.check().unwrap();assert!(acquire_admission_authority(&dst,&target,&attempt,&claim).await.is_err(),"held runtime capsule lease refuses competing owner");
  let payload=dst.join(format!(".attachment-upload-journal/{attempt}/payload.part"));let retained=base.join("original-payload");std::fs::rename(&payload,&retained).unwrap();private_write(&payload,&std::fs::read(&retained).unwrap());assert!(authority.check().is_err(),"equal-byte foreign destination payload refuses");std::fs::remove_file(&payload).unwrap();std::fs::rename(&retained,&payload).unwrap();authority.check().unwrap();
  sqlx::query("UPDATE attachment_capacity_reservations SET maximum_bytes=65 WHERE operation_id=$1").bind(&attempt).execute(&target).await.unwrap();let mut c=target.acquire().await.unwrap();assert!(authority.check_in(&mut c).await.is_err(),"changed full claim SQL refuses");drop(c);sqlx::query("UPDATE attachment_capacity_reservations SET maximum_bytes=64 WHERE operation_id=$1").bind(&attempt).execute(&target).await.unwrap();
  sqlx::query("DELETE FROM storage_restore_current").execute(&target).await.unwrap();let mut c=target.acquire().await.unwrap();assert!(authority.check_in(&mut c).await.is_err(),"removed current finalized generation refuses");drop(c);
  drop(authority);drop(original);drop(leases);tx.rollback().await.unwrap();source.close().await;target.close().await;
  println!("ADMISSION_FINALIZED_READER_ORIGINAL_SOURCE_ROLES_FULL_SQL_FINALIZATION_CURRENT_FOREIGN_INODE_KERNEL_LEASE_REFUSAL_PASS {}",if pg{"Postgres"}else{"SQLite"});
 }
 #[tokio::test]
 #[ignore="requires owned isolated SQLite/PostgreSQL fixtures"]
 async fn required_prior_restored_capture_complete_engine() {
  sqlx::any::install_default_drivers();let url=std::env::var("STARLANE_CAPACITY_TEST_URL").unwrap();let pg=url.starts_with("postgres");
  let target_url=if pg{url.clone()}else{url.replace(".db?","-target.db?")};let source=pool(&url,"source").await;let target=pool(&target_url,"target").await;
  let base=std::env::temp_dir().join(format!("admission-final-reader-{}",uuid::Uuid::new_v4()));let src=base.join("source");let dst=base.join("target");private_dir(&src);private_dir(&dst);
  use crate::storage::{attachment_upload as upload,attachment_capacity as capacity};let attempt=uuid::Uuid::new_v4().to_string();let req=upload::UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"browser".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),original_name:"x.bin".into(),mime_type:"application/octet-stream".into()};let writer=upload::admission::WriterLease::begin(&src,&attempt,upload::admission::Producer::Direct{request:req.clone()},128*1024*1024).unwrap();capacity::reserve(&source,&writer,None).await.unwrap();let mut ingest=upload::Ingest::reserve_with_lease(&src,req,128*1024*1024,writer.clone()).unwrap();let chunk=[0x3cu8;64*1024];for _ in 0..1600 {ingest.append(&chunk).unwrap();}let claim=writer.original_claim().to_vec();let request_sha=writer.producer_sha256().to_owned();let claim_sha=writer.claim_sha256();drop(ingest);drop(writer);
  let omitted_attempt=uuid::Uuid::new_v4().to_string();let omitted_req=upload::UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"browser".into(),actor_id:"actor".into(),space_id:"space".into(),document_id:"page".into(),original_name:"omitted.bin".into(),mime_type:"application/octet-stream".into()};let omitted=upload::admission::WriterLease::begin(&src,&omitted_attempt,upload::admission::Producer::Direct{request:omitted_req},64).unwrap();capacity::reserve(&source,&omitted,None).await.unwrap();let omitted_row=admission::CapturedReservation{operation_id:omitted_attempt.clone(),request_sha256:omitted.producer_sha256().to_owned(),admission_claim_sha256:omitted.claim_sha256(),maximum_bytes:64,state:"reserved".into(),committed_asset_id:None,committed_bytes:None,disposition_json:None,disposition_sha256:None};let omitted_claim=omitted.original_claim().to_vec();drop(omitted);
  let expected=admission::CapturedReservation{operation_id:attempt.clone(),request_sha256:request_sha.clone(),admission_claim_sha256:claim_sha.clone(),maximum_bytes:128*1024*1024,state:"reserved".into(),committed_asset_id:None,committed_bytes:None,disposition_json:None,disposition_sha256:None};let mut tx=source.begin().await.unwrap();let leases=admission::capture::NativeCaptureLeases::hold(&src,&mut *tx,&[expected,omitted_row.clone()]).await.unwrap();let original=leases.classify_supported(&mut tx).await.unwrap();let roles=original.source_roles().clone();
  for s in &roles.scopes {for b in &s.directories {if let Some(path)=b.role.relative(&attempt).unwrap(){private_dir(&dst.join(path));}}for b in &s.files {let path=b.role.relative(&attempt).unwrap();private_write(&dst.join(&path),&std::fs::read(src.join(&path)).unwrap());}}
  sqlx::query("INSERT INTO attachment_capacity_reservations(operation_id,request_sha256,admission_claim_sha256,maximum_bytes,state) VALUES($1,$2,$3,134217728,'reserved')").bind(&attempt).bind(&request_sha).bind(&claim_sha).execute(&target).await.unwrap();
  private_write(&dst.join(format!(".attachment-upload-admission/{omitted_attempt}/claim.json")),&omitted_claim);sqlx::query("INSERT INTO attachment_capacity_reservations(operation_id,request_sha256,admission_claim_sha256,maximum_bytes,state) VALUES($1,$2,$3,64,'reserved')").bind(&omitted_attempt).bind(&omitted_row.request_sha256).bind(&omitted_row.admission_claim_sha256).execute(&target).await.unwrap();
  assert!(acquire_admission_authority(&dst,&target,&attempt,&claim).await.is_err(),"unregistered copied payload grants nothing");
  let original_root=roles.scopes[0].directories.iter().find(|b|b.role==admission::DirectoryRole::DataRoot).unwrap().original.clone();let gid=fixture_finalized(&dst,&target,roles,original_root).await;
  let finalization=dst.join(format!("{NAMESPACE}/generations/{gid}/finalization.json"));let kept=base.join("held-finalization.json");std::fs::rename(&finalization,&kept).unwrap();assert!(activate_in(&dst,&target).await.is_err(),"missing finalized proof refuses");std::fs::rename(&kept,&finalization).unwrap();
  let diagnostic=Root::open(&dst).unwrap();let mut conn=target.acquire().await.unwrap();let (rows,pointer)=history_rows(&mut conn).await.unwrap();history_closure(HistorySource::new(&diagnostic,None).unwrap(),identity(diagnostic.fd()).unwrap(),Some((&rows,&pointer))).unwrap();drop(conn);Active::load(diagnostic).unwrap();
  activate_in(&dst,&target).await.unwrap();let mut c=target.acquire().await.unwrap();let complete=prior_capture_rows(&mut c).await.unwrap();assert!(PriorRestoredCaptureLeases::hold(&dst,&mut c,&complete[..1]).await.is_err(),"Incomplete reservation inventory cannot omit a capsule");let mut duplicate=complete.clone();duplicate.push(complete[0].clone());assert!(PriorRestoredCaptureLeases::hold(&dst,&mut c,&duplicate).await.is_err(),"Duplicate reservation proof refuses");let malformed_record=dst.join(format!(".attachment-upload-admission/{omitted_attempt}/stage-origin.json"));private_write(&malformed_record,b"{}");assert!(PriorRestoredCaptureLeases::hold(&dst,&mut c,&complete).await.is_err(),"Unsupported state cannot hide malformed original capsule proof");std::fs::remove_file(&malformed_record).unwrap();let authority=PriorRestoredCaptureLeases::hold(&dst,&mut c,&complete).await.unwrap();drop(c);authority.fence().unwrap();let evidence=authority.lineage_evidence().unwrap();assert_eq!(evidence.version,3);assert_eq!(evidence.scopes.len(),1);assert_eq!(evidence.scopes[0].original_claim_json.as_bytes(),claim);assert!(evidence.scopes[0].directories.iter().any(|b|b.original!=b.captured));assert!(upload::admission::WriterLease::open_idle(&dst,&attempt).is_err());assert!(upload::admission::WriterLease::open_idle(&dst,&omitted_attempt).is_err());let omitted_dir=Root::open(&dst).unwrap().directory(&format!(".attachment-upload-admission/{omitted_attempt}")).unwrap();assert!(unsafe{libc::flock(omitted_dir.as_raw_fd(),libc::LOCK_EX|libc::LOCK_NB)}<0,"Unsupported capsule retains source exclusion");assert!(acquire_admission_authority(&dst,&target,&attempt,&claim).await.is_err(),"held runtime capsule lease refuses competing owner");
  let payload=dst.join(format!(".attachment-upload-journal/{attempt}/payload.part"));let retained=base.join("original-payload");std::fs::rename(&payload,&retained).unwrap();private_write(&payload,&std::fs::read(&retained).unwrap());assert!(authority.fence().is_err(),"equal-byte foreign destination payload refuses");std::fs::remove_file(&payload).unwrap();std::fs::rename(&retained,&payload).unwrap();assert!(authority.fence().is_err(),"A mutated capture certificate stays invalid even after restoring original inode");drop(authority);let mut c=target.acquire().await.unwrap();let authority=PriorRestoredCaptureLeases::hold(&dst,&mut c,&complete).await.unwrap();drop(c);authority.fence().unwrap();
  sqlx::query("UPDATE attachment_capacity_reservations SET maximum_bytes=134217729 WHERE operation_id=$1").bind(&attempt).execute(&target).await.unwrap();let mut c=target.acquire().await.unwrap();assert!(authority.recheck_in(&mut c).await.is_err(),"changed full claim SQL refuses");drop(c);sqlx::query("UPDATE attachment_capacity_reservations SET maximum_bytes=134217728 WHERE operation_id=$1").bind(&attempt).execute(&target).await.unwrap();
  sqlx::query("DELETE FROM storage_restore_current").execute(&target).await.unwrap();let mut c=target.acquire().await.unwrap();assert!(authority.recheck_in(&mut c).await.is_err(),"removed current finalized generation refuses");drop(c);
  authority.fence().unwrap();let omitted_path=dst.join(format!(".attachment-upload-admission/{omitted_attempt}"));let old_omitted=base.join("held-omitted-capsule");std::fs::rename(&omitted_path,&old_omitted).unwrap();private_write(&omitted_path.join("claim.json"),&omitted_claim);assert!(authority.fence().is_err(),"Equal-byte foreign unsupported capsule cannot escape the complete held inventory");
  drop(authority);drop(original);drop(leases);tx.rollback().await.unwrap();source.close().await;target.close().await;
  println!("PRIOR_RESTORED_COMPLETE_RESERVATIONS_FINALIZED_ROLE_ORIGINAL_BYTES_UNSUPPORTED_CAPSULE_KERNEL_FOREIGN_INODE_SQL_GENERATION_REFUSAL_PASS {}",if pg{"Postgres"}else{"SQLite"});
 }
}

include!("restoration_admission_prior_capture.rs");

include!("restoration_admission_preservation_capture.rs");

include!("restoration_sqlite_capture.rs");
