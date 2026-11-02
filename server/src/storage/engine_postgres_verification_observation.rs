// Current original source and actual child observation; progress grants no authority.
async fn pg_live_observation<T>(source:&mut StoppedPgVerificationSource<'_>,process:&mut std::process::Child,work:impl std::future::Future<Output=anyhow::Result<T>>)->anyhow::Result<T>{
 source.fence().await?;tokio::pin!(work);let until=tokio::time::sleep(std::time::Duration::from_secs(120));tokio::pin!(until);let mut pulse=tokio::time::interval(std::time::Duration::from_secs(3));pulse.tick().await;
 loop{tokio::select!{value=&mut work=>{source.fence().await?;ensure!(process.try_wait()?.is_none(),"Actual PG ordinary process exited during observation");return value;},_=&mut until=>bail!("Original source-fenced PG observation exceeded120s"),_=pulse.tick()=>{
  #[cfg(test)]pg_pending_original_change(source).await?;
  source.fence().await?;ensure!(process.try_wait()?.is_none(),"Actual PG ordinary process exited while pending");
 }}}
}
#[cfg(test)]tokio::task_local!{static PG_PENDING_ORIGINAL_CHANGE:std::cell::Cell<u8>;}
#[cfg(test)]async fn pg_pending_original_change(source:&mut StoppedPgVerificationSource<'_>)->anyhow::Result<()>{let mode=PG_PENDING_ORIGINAL_CHANGE.try_with(|v|v.replace(0)).unwrap_or(0);match mode{0=>{},1=>{let actor=source.current_human().await?;sqlx::query("UPDATE users SET role='member' WHERE id=$1").bind(actor).execute(&mut *source.conn).await?;},2=>{sqlx::query("DELETE FROM sessions WHERE id=$1").bind(&source.admin.claims.sid).execute(&mut *source.conn).await?;},_=>bail!("Invalid original PG pending test change")};Ok(())}
