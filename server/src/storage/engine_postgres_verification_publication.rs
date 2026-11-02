// Purpose-separated original selected PG target publication acknowledgment.
// Private constructor derives complete raw manifests from original source SQL.
pub(crate) struct PgPublicationSelection{proof:Arc<RestoredPgAdmission>,expected:BTreeMap<String,String>}
pub(crate) struct PgPublicationPaths{proof:Arc<RestoredPgAdmission>,files:BTreeMap<String,(File,PhysicalStamp,String)>}
impl PgPublicationSelection{
 fn prepare(selected:&RestoredPgServing,original:&Table,current:&Table)->anyhow::Result<Self>{
  ensure!(original.columns==current.columns&&original.keys==current.keys&&original.rows.len()<=256&&current.rows.len()==original.rows.len(),"Original publication receipt shape/count differs");
  let index=|name:&str|original.columns.iter().position(|c|c.name==name).ok_or_else(||fail("Original receipt column absent"));let id=index("op_id")?;let manifest=index("manifest_json")?;let verified=index("verified_at")?;
  let mut rows=BTreeMap::new();for row in &original.rows{ensure!(row.len()==original.columns.len(),"Original receipt width differs");let key=text(&row[id])?.to_owned();ensure!(rows.insert(key,row).is_none(),"Duplicate original receipt");}
  let mut expected=BTreeMap::new();let mut total=0usize;for row in &current.rows{ensure!(row.len()==current.columns.len(),"Target receipt width differs");let key=text(&row[id])?;let source=*rows.get(key).context("Foreign target receipt")?;for i in 0..row.len(){if i!=verified{ensure!(row[i]==source[i],"Target publication immutable identity/manifest differs before owner verification");}}let raw=text(&source[manifest])?;total=total.checked_add(raw.len()).context("Publication manifest overflow")?;ensure!(raw.len()<=1024*1024&&total<=16*1024*1024,"Original receipt manifests exceed supported bounds");ensure!(expected.insert(key.into(),raw.into()).is_none(),"Duplicate target receipt");}
  Ok(Self{proof:selected.proof.clone(),expected})
 }
 pub(crate) fn path(&self)->&Path{&self.proof.root.absolute}
 pub(crate) fn expected(&self)->&BTreeMap<String,String>{&self.expected}
 pub(crate) fn native_fence(&self)->anyhow::Result<()>{Ok(self.proof.physical_fence()?)}
 pub(crate) async fn check_in(&self,conn:&mut sqlx::AnyConnection)->anyhow::Result<()>{self.proof.verify_any_connection(conn).await}
 pub(crate) fn hold_paths(&self,paths:&[String])->anyhow::Result<PgPublicationPaths>{
  self.native_fence()?;ensure!(paths.len()<=1024,"Publication path inventory exceeds1024");let mut files=BTreeMap::new();let mut total=0u64;for path in paths{ensure!(valid_relative(path),"Unsafe original publication path");let file=self.proof.root.leaf(path)?;let stamp=physical_stamp(&file)?;total=total.checked_add(stamp.length).context("Publication path byte overflow")?;ensure!(stamp.length<=16*1024*1024&&total<=64*1024*1024,"Publication path proof exceeds16MiB/file or64MiB");let sha=hash_file(&file,stamp.length)?;ensure!(physical_stamp(&file)?==stamp,"Original publication file changed");files.insert(path.clone(),(file,stamp,sha));}
  let held=PgPublicationPaths{proof:self.proof.clone(),files};held.fence()?;Ok(held)
 }
}
impl PgPublicationPaths{pub(crate) fn fence(&self)->anyhow::Result<()>{self.proof.physical_fence()?;for(path,(held,stamp,sha))in &self.files{let current=self.proof.root.leaf(path)?;ensure!(physical_stamp(held)?==*stamp&&physical_stamp(&current)?==*stamp&&hash_file(&current,stamp.length)?==*sha,"Original publication owned path replaced");}Ok(self.proof.physical_fence()?)}}
async fn pg_original_fenced_work<T>(original:&mut StoppedPgVerificationSource<'_>,work:impl std::future::Future<Output=anyhow::Result<T>>)->anyhow::Result<T>{
 original.fence().await?;tokio::pin!(work);let until=tokio::time::sleep(std::time::Duration::from_secs(120));tokio::pin!(until);let mut pulse=tokio::time::interval(std::time::Duration::from_secs(3));pulse.tick().await;loop{tokio::select!{result=&mut work=>{original.fence().await?;return result;},_=&mut until=>bail!("Original-source-owned publication verification exceeded120s"),_=pulse.tick()=>original.fence().await?}}
}

fn pg_project_publication_owner_ack(table:&mut Table,ack:&crate::storage::publication::PgPublicationRecoveryAck)->anyhow::Result<()>{
 let id=table.columns.iter().position(|c|c.name=="op_id").context("Original publication operation absent")?;let manifest=table.columns.iter().position(|c|c.name=="manifest_json").context("Original raw publication absent")?;let verified=table.columns.iter().position(|c|c.name=="verified_at").context("Original publication verification field absent")?;let mut seen=std::collections::BTreeSet::new();
 for(op,raw,_old,new)in ack.changes(){ensure!(seen.insert(op),"Duplicate actual publication owner acknowledgment");let row=table.rows.iter_mut().find(|r|text(&r[id]).is_ok_and(|id|id==op)).context("Foreign publication owner acknowledgment")?;ensure!(text(&row[manifest])?==raw,"Publication owner acknowledgment original raw manifest differs");row[verified]=Cell::Text(new.as_bytes().to_vec());}
 ensure!(seen.len()==table.rows.len(),"Actual owner did not acknowledge complete original receipt set");Ok(())
}
