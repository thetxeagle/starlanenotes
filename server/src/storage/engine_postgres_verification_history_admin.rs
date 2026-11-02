// Capability retained only at actual normal startup. HTTP observation never
// constructs a source role, creates a directory, signs evidence or repairs files.
struct PgNormalHistoryReader{serving:std::sync::Arc<QualifiedPgServing>,store:PgVerificationStore}
static PG_NORMAL_HISTORY_READER:std::sync::OnceLock<std::sync::Arc<PgNormalHistoryReader>>=std::sync::OnceLock::new();
#[derive(serde::Serialize)]pub(crate) struct ApplicationRestoreHistoryObservation{
 status:&'static str,completed_at:Option<String>,scope:Option<String>,operation:Option<String>,
 pages:Option<usize>,drafts:Option<usize>,registered_assets:Option<usize>,legacy_assets:Option<usize>,
 exact_binary_bytes:Option<String>,sampled_humans:Option<usize>,
 semantics:&'static str,full_supported_restore_verified:bool,
}
impl ApplicationRestoreHistoryObservation{fn unavailable()->Self{Self{status:"unavailable",completed_at:None,scope:None,operation:None,pages:None,drafts:None,registered_assets:None,legacy_assets:None,exact_binary_bytes:None,sampled_humans:None,semantics:"historical_completed_isolated_application_test_at_recorded_time",full_supported_restore_verified:false}}}
pub(crate) async fn retain_normal_application_history(serving:Option<std::sync::Arc<QualifiedPgServing>>,state:&crate::AppState)->anyhow::Result<()>{
 let path=match std::env::var("VERIFIED_RESTORE_RECORDS_DIR"){Ok(path)=>path,Err(std::env::VarError::NotPresent)=>return Ok(()),Err(_)=>bail!("Application history configuration invalid")};
 let serving=serving.context("Application history has no original qualified source startup owner")?;let QualifiedPgServing::PgRestore(source)=serving.as_ref()else{bail!("Application history source scope unsupported")};
 source.fence().await?;let mut connection=state.db.acquire().await?;source.proof.verify_any_connection(&mut connection).await?;
 let store=PgVerificationStore::selected(Path::new(&path),&source.proof.root,None,&state.jwt_secret,false)?;source.proof.verify_any_connection(&mut connection).await?;drop(connection);source.fence().await?;
 let reader=std::sync::Arc::new(PgNormalHistoryReader{serving,store});ensure!(PG_NORMAL_HISTORY_READER.set(reader).is_ok(),"Application history startup owner already selected");Ok(())
}
impl PgNormalHistoryReader{
 async fn observe(&self,state:&crate::AppState)->anyhow::Result<ApplicationRestoreHistoryObservation>{
  ensure!(self.store.key==state.jwt_secret.as_bytes(),"Application history configuration key changed");let QualifiedPgServing::PgRestore(source)=self.serving.as_ref()else{bail!("Application history source scope unsupported")};
  source.fence().await?;let mut conn=state.db.acquire().await?;source.proof.verify_any_connection(&mut conn).await?;self.store.fence()?;
  let record=self.store.read_original_history(source).await?;source.proof.verify_any_connection(&mut conn).await?;drop(conn);source.fence().await?;self.store.fence()?;
  Ok(match record{None=>ApplicationRestoreHistoryObservation::unavailable(),Some(r)=>ApplicationRestoreHistoryObservation{status:"completed_isolated_test",completed_at:Some(r.payload.completed_at),scope:Some(r.payload.scope),operation:Some(r.payload.operation),pages:Some(r.payload.checks.pages),drafts:Some(r.payload.checks.drafts),registered_assets:Some(r.payload.checks.registered_assets),legacy_assets:Some(r.payload.checks.legacy_assets),exact_binary_bytes:Some(r.payload.checks.exact_binary_bytes.to_string()),sampled_humans:Some(r.payload.checks.audiences),semantics:"historical_completed_isolated_application_test_at_recorded_time",full_supported_restore_verified:false}})
 }
}
pub(crate) async fn observe_normal_application_history(state:&crate::AppState)->ApplicationRestoreHistoryObservation{
 let Some(reader)=PG_NORMAL_HISTORY_READER.get()else{return ApplicationRestoreHistoryObservation::unavailable()};
 match tokio::time::timeout(std::time::Duration::from_secs(2),reader.observe(state)).await{Ok(Ok(report))=>report,_=>ApplicationRestoreHistoryObservation::unavailable()}
}
