// Actual publication-owner acknowledgment for a dedicated verifier. No serde,
// caller JSON, flag, timestamp or copied receipt constructs this evidence.
struct PublicationRecoveryBefore {
 m:PreparedPublication,raw_manifest:String,old_verified:Option<String>,
 inspection:journal_fs::InstallInspection,
 held:crate::storage::restoration::linux::VerificationRecoveryPaths,
}
pub(crate) struct PublicationRecoveryChange {before:PublicationRecoveryBefore,new_verified:String}
pub(crate) struct PublicationRecoveryAck<'a,'b> {
 selection:&'a crate::storage::restoration::linux::OriginalTargetRecoverySelection<'b>,
 changes:Vec<PublicationRecoveryChange>,
}
fn recovery_observer_fence(selection:Option<&crate::storage::restoration::linux::OriginalTargetRecoverySelection<'_>>)->Result<(),AppError> {
 if let Some(selection)=selection {selection.fence().map_err(AppError::internal)?;}Ok(())
}
async fn recovery_change_prepare(selection:&crate::storage::restoration::linux::OriginalTargetRecoverySelection<'_>,conn:&mut AnyConnection,m:&PreparedPublication)->Result<PublicationRecoveryBefore,AppError> {
 recovery_observer_fence(Some(selection))?;
 let row:(String,Option<String>)=sqlx::query_as("SELECT manifest_json,verified_at FROM publication_receipts WHERE op_id=$1").bind(m.op_id()).fetch_one(&mut *conn).await.map_err(AppError::internal)?;
 recovery_observer_fence(Some(selection))?;
 if receipt(conn,m.op_id()).await?.as_ref()!=Some(m){return Err(conflict("original verification receipt changed"));}
 recovery_observer_fence(Some(selection))?;
 let inspection=journal_fs::inspect_install(selection.path(),NS,&(&m.file).into(),true).map_err(io)?;
 if !inspection.installed_present {return Err(conflict("original verification install unavailable"));}
 let mut paths=inspection.paths.iter().map(|p|p.relative.clone()).collect::<std::collections::BTreeSet<_>>();
 paths.insert(format!("{NS}/{}/manifest.json",m.op_id()));
 if inspection.proof.is_some(){paths.insert(format!("{NS}/{}/install-proof.json",m.op_id()));}
 let held=selection.hold_paths(&paths.into_iter().collect::<Vec<_>>()).map_err(AppError::internal)?;
 journal_fs::verify_install(selection.path(),NS,&m.file).map_err(io)?;
 held.fence(selection).map_err(AppError::internal)?;
 Ok(PublicationRecoveryBefore{m:m.clone(),raw_manifest:row.0,old_verified:row.1,inspection,held})
}
async fn recovery_change_finish(selection:&crate::storage::restoration::linux::OriginalTargetRecoverySelection<'_>,conn:&mut AnyConnection,before:PublicationRecoveryBefore,new_verified:String)->Result<PublicationRecoveryChange,AppError> {
 before.held.fence(selection).map_err(AppError::internal)?;
 let row:(String,Option<String>)=sqlx::query_as("SELECT manifest_json,verified_at FROM publication_receipts WHERE op_id=$1").bind(before.m.op_id()).fetch_one(&mut *conn).await.map_err(AppError::internal)?;
 before.held.fence(selection).map_err(AppError::internal)?;
 if row.0!=before.raw_manifest||row.1.as_deref()!=Some(new_verified.as_str())||receipt(conn,before.m.op_id()).await?.as_ref()!=Some(&before.m){return Err(conflict("exact verification owner receipt update differs"));}
 before.held.fence(selection).map_err(AppError::internal)?;
 if journal_fs::inspect_install(selection.path(),NS,&(&before.m.file).into(),true).map_err(io)?!=before.inspection{return Err(conflict("verification install changed through recovery"));}
 Ok(PublicationRecoveryChange{before,new_verified})
}
pub(crate) async fn recover_for_verification<'a,'b>(selection:&'a crate::storage::restoration::linux::OriginalTargetRecoverySelection<'b>)->Result<PublicationRecoveryAck<'a,'b>,AppError> {
 selection.fence().map_err(AppError::internal)?;
 let changes=recover_in_observed(selection.path(),selection.pool(),Some(selection)).await?;
 let result=PublicationRecoveryAck{selection,changes};result.fence(selection.pool(),selection.path()).map_err(AppError::internal)?;Ok(result)
}
impl PublicationRecoveryAck<'_,'_> {
 pub(crate) fn original_bundle(&self)->&crate::storage::backup_inventory::ValidatedBundle {self.selection.bundle()}
 pub(crate) fn changes(&self)->impl Iterator<Item=(&str,&str,Option<&str>,&str)> {
  self.changes.iter().map(|c|(c.before.m.op_id(),c.before.raw_manifest.as_str(),c.before.old_verified.as_deref(),c.new_verified.as_str()))
 }
 pub(crate) fn fence(&self,pool:&AnyPool,path:&Path)->anyhow::Result<()> {
  anyhow::ensure!(std::sync::Arc::ptr_eq(&pool.connect_options(),&self.selection.pool().connect_options())&&path==self.selection.path(),"Recovery acknowledgment belongs to another original target");self.selection.fence()?;
  for change in &self.changes {change.before.held.fence(self.selection)?;journal_fs::verify_install(path,NS,&change.before.m.file).map_err(|e|anyhow::anyhow!("Original recovery install unavailable: {e}"))?;anyhow::ensure!(journal_fs::inspect_install(path,NS,&(&change.before.m.file).into(),true)?==change.before.inspection,"Original recovery inspection changed");}self.selection.fence()?;Ok(())
 }
}
