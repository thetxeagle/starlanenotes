// Not authority: this comparator is callable only inside the future retained
// selected-target exclusion guard. Original SQL and exercised SID set are held
// producer inputs, never deserialized caller assertions.
#[derive(Debug,serde::Serialize)]
struct PgSessionActivity {session_id:String,original_last_seen_at:String,observed_last_seen_at:String,original_expires_at:String,observed_expires_at:String}
fn pg_sorted_rows(rows:&[Vec<Cell>])->anyhow::Result<Vec<Vec<u8>>>{ensure!(rows.len()<=100000,"Application comparison exceeds100000 rows per table");let mut total=0usize;let mut out=Vec::new();for row in rows{let bytes=serde_json::to_vec(row)?;total=total.checked_add(bytes.len()).context("Application comparison overflow")?;ensure!(total<=128*1024*1024,"Application table comparison exceeds128MiB");out.push(bytes);}out.sort();Ok(out)}
fn pg_compare_observed_application(original:&BTreeMap<String,Table>,observed:&BTreeMap<String,Table>,exercised:&std::collections::BTreeSet<String>,started:chrono::DateTime<chrono::Utc>,finished:chrono::DateTime<chrono::Utc>)->anyhow::Result<Vec<PgSessionActivity>>{
 ensure!(started<=finished&&finished.signed_duration_since(started)<=chrono::Duration::minutes(30),"Application activity interval invalid");
 ensure!(original.keys().eq(observed.keys()),"Application table set differs");let mut activity=Vec::new();
 for(name,expected)in original{let actual=observed.get(name).context("Application table absent")?;ensure!(expected.columns==actual.columns&&expected.keys==actual.keys,"Application schema/keys differ");
  if name!="sessions"{ensure!(pg_sorted_rows(&expected.rows)?==pg_sorted_rows(&actual.rows)?,"Immutable application rows differ");continue;}
  let index=|key:&str|expected.columns.iter().position(|c|c.name==key).ok_or_else(||fail("Original session schema incomplete"));
  let id=index("id")?;let seen=index("last_seen_at")?;let expires=index("expires_at")?;let kind=index("session_kind")?;
  let mut by_id=BTreeMap::new();for row in &expected.rows{ensure!(row.len()==expected.columns.len(),"Original session row width differs");let key=text(&row[id])?.to_owned();ensure!(by_id.insert(key,row).is_none(),"Duplicate original session identity");}
  let mut normalized=Vec::new();let mut current_ids=std::collections::BTreeSet::new();
  for row in &actual.rows{ensure!(row.len()==actual.columns.len(),"Observed session row width differs");let key=text(&row[id])?;ensure!(current_ids.insert(key.to_owned()),"Duplicate observed session identity");let prior=*by_id.get(key).context("Foreign session row inserted")?;
   let mut normalized_row=row.clone();if row[seen]!=prior[seen]||row[expires]!=prior[expires]{ensure!(exercised.contains(key),"Unprobed original session activity changed");let old_seen=text(&prior[seen])?;let now_seen=text(&row[seen])?;let old_expires=text(&prior[expires])?;let now_expires=text(&row[expires])?;let observed_seen=chrono::DateTime::parse_from_rfc3339(now_seen)?.with_timezone(&chrono::Utc);let observed_expiry=chrono::DateTime::parse_from_rfc3339(now_expires)?.with_timezone(&chrono::Utc);let original_seen=chrono::DateTime::parse_from_rfc3339(old_seen)?.with_timezone(&chrono::Utc);chrono::DateTime::parse_from_rfc3339(old_expires)?;
    ensure!(observed_seen>=started&&observed_seen<=finished&&observed_seen>=original_seen,"Observed session activity outside actual probe interval");let session_kind=text(&prior[kind])?;ensure!(matches!(session_kind,"app"|"browser"),"Unsupported original session kind");let duration=chrono::Duration::days(crate::auth::routes::session_inactivity_days(session_kind));ensure!(observed_expiry>=started+duration&&observed_expiry<=finished+duration,"Observed session expiry outside actual kind/probe interval");
    activity.push(PgSessionActivity{session_id:key.into(),original_last_seen_at:old_seen.into(),observed_last_seen_at:now_seen.into(),original_expires_at:old_expires.into(),observed_expires_at:now_expires.into()});normalized_row[seen]=prior[seen].clone();normalized_row[expires]=prior[expires].clone();
   }normalized.push(normalized_row);
  }
  ensure!(current_ids.len()==by_id.len()&&pg_sorted_rows(&expected.rows)?==pg_sorted_rows(&normalized)?,"Session identity/refresh/authority/device or unacknowledged fields differ");
 }
 Ok(activity)
}

struct PgFinalTargetFile{file:File,stamp:PhysicalStamp,sha:String}
// Retains the SAME selected native target, not a later path lookup. Constructed
// only after actual ordinary-child termination by its producer.
struct PgFinalObservedTarget{
 selected:RestoredPgServing,tx:sqlx::Transaction<'static,sqlx::Any>,profile:EngineCatalogueProfile,
 rows:BTreeMap<String,Table>,catalog:Vec<String>,root_stamp:PhysicalStamp,
 files:BTreeMap<String,PgFinalTargetFile>,dirs:BTreeMap<String,(File,PhysicalStamp)>,
 native:Option<crate::storage::restoration::admission::capture::NativeCaptureLeases>,
 prior:Option<super::PriorRestoredCaptureLeases>,preservation:Option<super::FinalizedAdmissionPreservationLeases>,
 completed:Option<std::sync::Arc<crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::NativeCompletedCapture>>,
 prior_completed:Option<super::FinalizedCompletedCapture>,mixed:Option<super::MixedCompletedCapture>,
 compound:Option<crate::storage::restoration::CompletedAbsenceAuthority>,
 certificate:Option<crate::storage::restoration::admission::ClassifiedAdmissionSource>,
 activity:Vec<PgSessionActivity>,publication_ack:crate::storage::publication::PgPublicationRecoveryAck,
}
impl PgFinalObservedTarget{
 async fn hold(selected:RestoredPgServing,original:&mut StoppedPgVerificationSource<'_>,backup:&OwnedPgBackup,exercised:&std::collections::BTreeSet<String>,started:chrono::DateTime<chrono::Utc>,finished:chrono::DateTime<chrono::Utc>)->anyhow::Result<Self>{
  original.fence().await?;backup.fence()?;selected.fence().await?;
  let manifest:serde_json::Value=serde_json::from_slice(&backup.manifest)?;let attestation=&selected.proof.attestation;
  ensure!(manifest["backup_id"].as_str()==Some(attestation.backup_id.as_str())&&attestation.snapshot_sha256==digest(&backup.manifest)&&attestation.source_target==original.serving.proof.attestation.target&&(attestation.source_root_device,attestation.source_root_inode)==original.root_identity()?,"Final target differs from original held source/backup relation");
  let mut tx=selected.pool.begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await?;
  selected.proof.verify_any_connection(&mut tx).await?;let profile=pg_profile_any(&mut tx).await?;ensure!(profile==original.profile,"Final target profile changed across actual ordinary startup");let shape=shape_for_profile(profile)?;
  let names=shape.keys().cloned().chain(["_sqlx_migrations".into(),"pg_restoration_imports".into(),"pg_restoration_generations".into(),"pg_restoration_current".into()]).map(|n|quoted(&n)).collect::<anyhow::Result<Vec<_>>>()?.join(",");sqlx::query(&format!("LOCK TABLE {names} IN SHARE MODE")).execute(&mut *tx).await?;selected.proof.verify_any_connection(&mut tx).await?;
  let path=&selected.proof.root.absolute;
  let compound=pg_hold_final_compound(&selected,&mut tx).await?;
  let mixed=if compound.is_none()&&profile.has_erasure()&&super::has_mixed_completed_capture_source(path,&mut tx).await?{Some(super::MixedCompletedCapture::hold(path,&mut tx).await?)}else{None};
  let prior_completed=if compound.is_none()&&mixed.is_none()&&profile.has_erasure()&&super::has_finalized_completed_capture_source(path)?{Some(super::FinalizedCompletedCapture::hold(path,&mut tx).await?)}else{None};
  let prior=if compound.is_some()||mixed.is_some()||prior_completed.is_some(){None}else{pg_prior_admission_capture(path,&mut tx,profile).await?};
  let preservation=if compound.is_none()&&mixed.is_none()&&prior_completed.is_none()&&prior.is_none()&&profile.has_capacity()&&super::has_finalized_admission_preservation_source(path)?{let rows=super::prior_capture_rows(&mut tx).await?;Some(super::FinalizedAdmissionPreservationLeases::hold(path,&mut tx,&rows).await?)}else{None};
  let native=if compound.is_some()||mixed.is_some()||prior_completed.is_some()||prior.is_some()||preservation.is_some(){None}else{pg_hold_admission_capture(path,&mut tx,profile).await?};
  let completed=if compound.is_some()||mixed.is_some()||prior_completed.is_some()||preservation.is_some(){None}else{pg_hold_completed_capture(path,&mut tx,profile,native.as_ref()).await?};
  let certificate=match(&native,&completed){(Some(n),Some(c))=>Some(n.classify_with_completed(&mut tx,c).await?),(Some(n),None)=>Some(n.classify_supported(&mut tx).await?),(None,None)=>None,_=>bail!("Final target completion has no actual native holder")};
  let before=pg_tables_any(&mut tx,&shape).await?;let mut expected=pg_expected_restore_transition(original.tables,&before,&selected.proof.attestation.custody_generation_id)?;
  let original_receipts=expected.get("publication_receipts").context("Original publication receipts absent")?;let before_receipts=before.get("publication_receipts").context("Selected target publication receipts absent")?;
  let selection=PgPublicationSelection::prepare(&selected,original_receipts,before_receipts)?;
  // Provisional preflight only: exact source identities/manifests/all immutable
  // columns were checked by selection. This cannot construct final evidence.
  let mut preflight=expected.clone();preflight.insert("publication_receipts".into(),before_receipts.clone());pg_compare_observed_application(&preflight,&before,exercised,started,finished)?;
  let publication_ack=pg_original_fenced_work(original,crate::storage::publication::recover_pg_for_verification(selection,&mut tx)).await?;
  pg_project_publication_owner_ack(expected.get_mut("publication_receipts").context("Original publication receipts absent")?,&publication_ack)?;
  let rows=pg_tables_any(&mut tx,&shape).await?;let activity=pg_compare_observed_application(&expected,&rows,exercised,started,finished)?;let catalog=catalog_any(&mut tx,&selected.proof.attestation.target.schema_name).await?;
  let root=&selected.proof.root;let root_stamp=physical_stamp(root.fd())?;let(paths,dirs)=engine_walk(root.fd(),"")?;ensure!(paths.len()<=100000&&dirs.len()<=4096,"Final target inventory exceeds bounds");let mut files=BTreeMap::new();let mut inventory_bytes=0u64;for path in paths{let file=root.leaf(&path)?;let stamp=physical_stamp(&file)?;inventory_bytes=inventory_bytes.checked_add(stamp.length).context("Final target inventory overflow")?;ensure!(stamp.length<=1024*1024*1024&&inventory_bytes<=2*1024*1024*1024,"Final target file inventory exceeds supported1GiB/file or2GiB aggregate");let sha=hash_file(&file,stamp.length)?;ensure!(physical_stamp(&file)?==stamp,"Final target changed while inventoried");files.insert(path,PgFinalTargetFile{file,stamp,sha});}
  original.fence().await?;let mut holder=Self{selected,tx,profile,rows,catalog,root_stamp,files,dirs,native,prior,preservation,completed,prior_completed,mixed,compound,certificate,activity,publication_ack};holder.fence().await?;Ok(holder)
 }
 async fn fence(&mut self)->anyhow::Result<()>{
  self.selected.proof.verify_any_connection(&mut self.tx).await?;self.publication_ack.fence(&mut self.tx).await?;let root=&self.selected.proof.root;ensure!(physical_stamp(root.fd())?==self.root_stamp,"Original final target root changed");
  if let Some(v)=&self.compound{v.check_in(&mut self.tx).await?;}if let Some(v)=&self.native{v.recheck(&mut self.tx).await?;}if let Some(v)=&self.prior{v.recheck_in(&mut self.tx).await?;}if let Some(v)=&self.preservation{v.recheck_in(&mut self.tx).await?;}if let Some(v)=&self.completed{v.fence_in(&mut self.tx).await?;}if let Some(v)=&self.prior_completed{v.recheck_in(&mut self.tx).await?;}if let Some(v)=&self.mixed{v.recheck_in(&mut self.tx).await?;}if let Some(v)=&self.certificate{v.fence()?;}
  ensure!(pg_profile_any(&mut self.tx).await?==self.profile,"Final selected target profile changed");let rows=pg_tables_any(&mut self.tx,&shape_for_profile(self.profile)?).await?;ensure!(rows.keys().eq(self.rows.keys()),"Final selected target table set changed");for(name,table)in &self.rows{let other=&rows[name];ensure!(table.columns==other.columns&&table.keys==other.keys&&pg_sorted_rows(&table.rows)?==pg_sorted_rows(&other.rows)?,"Final selected target SQL changed");}
  ensure!(catalog_any(&mut self.tx,&self.selected.proof.attestation.target.schema_name).await?==self.catalog,"Final selected target catalogue changed");let(paths,dirs)=engine_walk(root.fd(),"")?;ensure!(paths.iter().eq(self.files.keys())&&dirs.keys().eq(self.dirs.keys()),"Final selected target topology changed");
  for(path,value)in &self.files{ensure!(physical_stamp(&value.file)?==value.stamp&&physical_stamp(&root.leaf(path)?)?==value.stamp&&hash_file(&value.file,value.stamp.length)?==value.sha,"Final selected target file exchanged");}
  for(path,(held,stamp))in &self.dirs{ensure!(physical_stamp(held)?==*stamp&&physical_stamp(&destination_directory(root.fd(),path)?)?==*stamp,"Final selected target directory exchanged");}
  self.selected.proof.verify_any_connection(&mut self.tx).await?;ensure!(physical_stamp(root.fd())?==self.root_stamp,"Final target root changed after observation");Ok(())
 }
}

#[cfg(test)]mod pg_target_activity_controls{
 use super::*;
 fn fixture()->anyhow::Result<(BTreeMap<String,Table>,std::collections::BTreeSet<String>,chrono::DateTime<chrono::Utc>,chrono::DateTime<chrono::Utc>)>{
  let mut shape=shape_for_profile(EngineCatalogueProfile::CURRENT)?;let table=shape.get_mut("sessions").context("Session schema absent")?;let started=chrono::Utc::now();let mut row=Vec::new();for column in &table.columns{row.push(match column.name.as_str(){"id"=>Cell::Text(b"original-sid".to_vec()),"session_kind"=>Cell::Text(b"browser".to_vec()),"last_seen_at"=>Cell::Text((started-chrono::Duration::hours(1)).to_rfc3339().into_bytes()),"expires_at"=>Cell::Text((started+chrono::Duration::days(29)).to_rfc3339().into_bytes()),_=>if column.required{Cell::Text(b"original-stable-value".to_vec())}else{Cell::Null}});}table.rows.push(row);Ok((shape,std::collections::BTreeSet::from(["original-sid".into()]),started,started+chrono::Duration::seconds(10)))
 }
 fn changed()->anyhow::Result<(BTreeMap<String,Table>,BTreeMap<String,Table>,std::collections::BTreeSet<String>,chrono::DateTime<chrono::Utc>,chrono::DateTime<chrono::Utc>)>{let(original,exercised,start,end)=fixture()?;let mut observed=original.clone();let table=observed.get_mut("sessions").unwrap();for(i,column)in table.columns.iter().enumerate(){match column.name.as_str(){"last_seen_at"=>table.rows[0][i]=Cell::Text((start+chrono::Duration::seconds(1)).to_rfc3339().into_bytes()),"expires_at"=>table.rows[0][i]=Cell::Text((start+chrono::Duration::days(30)+chrono::Duration::seconds(1)).to_rfc3339().into_bytes()),_=>{}}}Ok((original,observed,exercised,start,end))}
 #[test]fn original_session_named_activity_preserves_every_other_field()->anyhow::Result<()>{let(original,observed,exercised,start,end)=changed()?;ensure!(pg_compare_observed_application(&original,&observed,&exercised,start,end)?.len()==1,"Actual named activity absent");for field in ["id","user_id","refresh_token_hash","device_name","ip_address","user_agent","created_at","session_kind","client_platform"]{let mut foreign=observed.clone();let table=foreign.get_mut("sessions").unwrap();let index=table.columns.iter().position(|c|c.name==field).context("Original session field absent")?;table.rows[0][index]=Cell::Text(b"foreign".to_vec());ensure!(pg_compare_observed_application(&original,&foreign,&exercised,start,end).is_err(),"Foreign session identity/authority field accepted");}Ok(())}
 #[test]fn unprobed_outside_interval_added_missing_or_duplicate_session_refuses()->anyhow::Result<()>{let(original,observed,exercised,start,end)=changed()?;ensure!(pg_compare_observed_application(&original,&observed,&std::collections::BTreeSet::new(),start,end).is_err(),"Unprobed activity accepted");ensure!(pg_compare_observed_application(&original,&observed,&exercised,end,end+chrono::Duration::seconds(10)).is_err(),"Out of interval activity accepted");for mode in 0..3{let mut foreign=observed.clone();let table=foreign.get_mut("sessions").unwrap();match mode{0=>table.rows.clear(),1=>table.rows.push(table.rows[0].clone()),_=>{let i=table.columns.iter().position(|c|c.name=="id").unwrap();table.rows[0][i]=Cell::Text(b"foreign-session".to_vec());}}ensure!(pg_compare_observed_application(&original,&foreign,&exercised,start,end).is_err(),"Foreign session row set accepted");}Ok(())}
 #[test]fn tuple_heap_order_never_grants_content_change()->anyhow::Result<()>{let(original,mut observed,exercised,start,end)=changed()?;let mut second=original["sessions"].rows[0].clone();let id=original["sessions"].columns.iter().position(|c|c.name=="id").unwrap();second[id]=Cell::Text(b"second-original".to_vec());let mut original=original;original.get_mut("sessions").unwrap().rows.push(second.clone());observed.get_mut("sessions").unwrap().rows.insert(0,second);ensure!(pg_compare_observed_application(&original,&observed,&exercised,start,end)?.len()==1,"Heap row ordering mistaken for changed authority");observed.get_mut("documents").unwrap().rows.push(Vec::new());ensure!(pg_compare_observed_application(&original,&observed,&exercised,start,end).is_err(),"Foreign non-session row accepted");Ok(())}
}

// Exactly the owner's authenticated new restore generation is appended. This
// helper does not authenticate it: caller first verifies same held source,
// backup manifest and actual selected-target attestation/control SQL.
fn pg_expected_restore_transition(original:&BTreeMap<String,Table>,target:&BTreeMap<String,Table>,generation:&str)->anyhow::Result<BTreeMap<String,Table>>{
 ensure!(uuid::Uuid::parse_str(generation)?.to_string()==generation,"Final target generation noncanonical");
 let mut expected=original.clone();let old=original.get("storage_restore_generations").context("Original custody history missing")?;let new=target.get("storage_restore_generations").context("Target custody history missing")?;ensure!(old.columns==new.columns&&old.keys==new.keys,"Custody history shape differs");let id=old.columns.iter().position(|c|c.name=="generation_id").context("Custody generation key absent")?;
 ensure!(old.rows.iter().all(|row|row.len()==old.columns.len()&&text(&row[id]).is_ok_and(|s|s!=generation)),"Target generation duplicates original custody history");
 let mut retained=Vec::new();let mut added=0;for row in &new.rows{ensure!(row.len()==new.columns.len(),"Target custody row width differs");if text(&row[id])?==generation{added+=1;}else{retained.push(row.clone());}}
 ensure!(added==1&&pg_sorted_rows(&old.rows)?==pg_sorted_rows(&retained)?,"Target changed original custody history beyond exact new generation");
 let pointer=target.get("storage_restore_current").context("Target custody pointer missing")?;let old_pointer=original.get("storage_restore_current").context("Original custody pointer missing")?;ensure!(pointer.columns==old_pointer.columns&&pointer.keys==old_pointer.keys&&pointer.rows.len()==1&&pointer.columns.len()==2&&pointer.rows[0].len()==2,"Target custody pointer shape differs");let singleton=pointer.columns.iter().position(|c|c.name=="singleton").context("Custody singleton absent")?;let selected=pointer.columns.iter().position(|c|c.name=="generation_id").context("Custody pointer generation absent")?;ensure!(pointer.rows[0][singleton]==Cell::Integer(1)&&text(&pointer.rows[0][selected])?==generation,"Target custody pointer differs from exact authenticated owner generation");expected.insert("storage_restore_generations".into(),new.clone());expected.insert("storage_restore_current".into(),pointer.clone());Ok(expected)
}

#[cfg(test)]mod pg_final_target_history_controls{use super::*;
 #[test]fn exact_authenticated_append_preserves_every_original_history_row()->anyhow::Result<()>{let mut original=shape_for_profile(EngineCatalogueProfile::CURRENT)?;let old_id="11111111-2222-4333-8444-555555555555";let new_id="11111111-2222-4333-8444-666666666666";
  let table=original.get_mut("storage_restore_generations").context("History shape absent")?;let id=table.columns.iter().position(|c|c.name=="generation_id").context("Generation absent")?;let mut row=vec![Cell::Text(b"original-immutable-history".to_vec());table.columns.len()];row[id]=Cell::Text(old_id.as_bytes().to_vec());table.rows.push(row.clone());let mut target=original.clone();row[id]=Cell::Text(new_id.as_bytes().to_vec());target.get_mut("storage_restore_generations").unwrap().rows.push(row);let pointer=target.get_mut("storage_restore_current").unwrap();pointer.rows=vec![pointer.columns.iter().map(|c|if c.name=="singleton"{Cell::Integer(1)}else{Cell::Text(new_id.as_bytes().to_vec())}).collect()];ensure!(pg_expected_restore_transition(&original,&target,new_id).is_ok(),"Exact owner append refused");
  for mode in 0..3{let mut foreign=target.clone();let table=foreign.get_mut("storage_restore_generations").unwrap();match mode{0=>{let field=table.columns.iter().position(|c|c.name=="ledger_sha256").unwrap();table.rows[0][field]=Cell::Text(b"foreign-original-ledger".to_vec());},1=>{table.rows.remove(0);},_=>table.rows.push(table.rows[1].clone())};ensure!(pg_expected_restore_transition(&original,&foreign,new_id).is_err(),"Changed original history or extra owner row accepted");}
  let pointer=target.get_mut("storage_restore_current").unwrap();let i=pointer.columns.iter().position(|c|c.name=="generation_id").unwrap();pointer.rows[0][i]=Cell::Text(old_id.as_bytes().to_vec());ensure!(pg_expected_restore_transition(&original,&target,new_id).is_err(),"Old/foreign pointer accepted");Ok(())
 }
}

// Custody-only compound branch. The original registered native Ledger4 and
// same selected PG proof/SQL relation construct this holder. Serialized roles
// or missing payloads cannot create it. This is not completed application evidence.
async fn pg_hold_final_compound(selected:&RestoredPgServing,tx:&mut sqlx::Transaction<'_,sqlx::Any>)->anyhow::Result<Option<crate::storage::restoration::CompletedAbsenceAuthority>> {
 selected.proof.verify_any_connection(tx).await?;
 let(path,active)=super::registered(&selected.proof.root.absolute)?;
 let Some(active)=active else{return Ok(None)};
 if active.ledger.version!=4{return Ok(None)}
 ensure!(active.ledger.composite.is_some()&&active.ledger.admission.is_none()&&active.ledger.completed_absence.is_none(),"Final compound native class differs");
 ensure!(identity(path.fd())?==identity(selected.proof.root.fd())?&&active.ledger.generation_id==selected.proof.attestation.custody_generation_id&&active.ledger_sha256==selected.proof.attestation.custody_ledger_sha256,"Final compound native generation differs from selected PG owner");
 let Some(super::EngineAuthority::PgRestore(engine))=active.engine.as_deref()else{bail!("Final compound requires actual registered restored PG engine")};
 engine.verify_any_connection(tx).await?;
 ensure!(engine.attestation.restore_id==selected.proof.attestation.restore_id&&digest(&engine.raw_attestation)==digest(&selected.proof.raw_attestation),"Final compound engine owner differs");
 let held=super::acquire_completed_absence_in(&path.absolute,tx).await?.context("Final compound complete native holder absent")?;
 held.check_in(tx).await?;selected.proof.verify_any_connection(tx).await?;Ok(Some(held))
}

#[cfg(test)]mod pg_final_compound_custody_controls {
 use super::*;
 #[tokio::test]#[ignore="fresh genuine compound PG cold target and protected private PG configuration"]
 async fn actual_final_compound_target_full_census_and_late_sql_refusal()->anyhow::Result<()> {
  let root=crate::storage::data_dir();let url=std::env::var("REPLAY_CAPACITY_TEST_PG_URL").context("Actual private PG fixture required")?;
  let selected=open_restored_pg_serving(&root,&url).await?;let mut tx=selected.pool.begin().await?;
  selected.proof.verify_any_connection(&mut tx).await?;let profile=pg_profile_any(&mut tx).await?;ensure!(profile.has_erasure(),"Actual exact compound profile required");let shape=shape_for_profile(profile)?;
  let names=shape.keys().cloned().chain(["_sqlx_migrations".into(),"pg_restoration_imports".into(),"pg_restoration_generations".into(),"pg_restoration_current".into()]).map(|n|quoted(&n)).collect::<anyhow::Result<Vec<_>>>()?.join(",");sqlx::query(&format!("LOCK TABLE {names} IN SHARE MODE")).execute(&mut *tx).await?;
  let before=pg_tables_any(&mut tx,&shape).await?;let held=pg_hold_final_compound(&selected,&mut tx).await?.context("Actual compound holder absent")?;held.check_in(&mut tx).await?;
  let(_,active)=super::super::registered(&root)?;let active=active.context("Actual current native owner absent")?;let roles=active.ledger.composite.as_ref().context("Actual native compound roles absent")?;ensure!(roles.native.len()==4,"Actual four native classes required");
  for scope in &roles.native{ensure!(crate::storage::attachment_upload::admission::WriterLease::open_idle(&root,&scope.original.reservation.operation_id).is_err(),"Copied native operation became writer authority");}
  let after=pg_tables_any(&mut tx,&shape).await?;for(name,table)in &before{let actual=&after[name];ensure!(table.columns==actual.columns&&table.keys==actual.keys&&pg_sorted_rows(&table.rows)?==pg_sorted_rows(&actual.rows)?,"Compound custody observation mutated original rows");}
  let attempt=&roles.native[0].original.reservation.operation_id;sqlx::query("UPDATE attachment_capacity_reservations SET maximum_bytes=maximum_bytes+1 WHERE operation_id=$1").bind(attempt).execute(&mut *tx).await?;
  ensure!(held.check_in(&mut tx).await.is_err(),"Late complete native reservation row change accepted");tx.rollback().await?;selected.fence().await?;
  println!("ACTUAL_FINAL_COMPOUND_TARGET_CURRENT_NATIVE_FULL_CENSUS_COPIED_WRITER_DENIAL_AND_LATE_SQL_REFUSAL_PASS");Ok(())
 }
}
