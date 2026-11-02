// Exact history preservation is distinct from presentation and binary attachment authority.
// Only the original live PG source can construct this private plan; no record grant.
#[derive(PartialEq,Eq)]
struct PgOriginalHistory {id:String,document:String,version:i64,change_set:Vec<u8>,actor:String,created:String,added:i64,removed:i64}
struct PgCanonicalHistoryPlan {source:Identity,attestation_sha:String,actor:String,session:String,documents:Vec<String>,rows:Vec<PgOriginalHistory>}
fn pg_history_bytes(total:usize,length:usize)->anyhow::Result<usize>{ensure!(length<=8*1024*1024,"Original PG history change exceeds8MiB");let next=total.checked_add(length).context("Original PG history byte overflow")?;ensure!(next<=16*1024*1024,"Original PG history aggregate exceeds16MiB");Ok(next)}
async fn pg_read_history(conn:&mut sqlx::AnyConnection,documents:&[String])->anyhow::Result<Vec<PgOriginalHistory>>{
 use sqlx::Row;ensure!(documents.len()<=256,"Original PG history page bound exceeded");let mut result=Vec::new();let mut total=0;
 for document in documents {let rows=sqlx::query("SELECT id,document_id,version_number,change_set,created_by,created_at,lines_added,lines_removed FROM doc_versions WHERE document_id=$1 ORDER BY version_number DESC,id LIMIT 257").bind(document).fetch_all(&mut *conn).await?;ensure!(rows.len()<=256,"Original PG history page exceeds256 versions");
  for row in rows {ensure!(result.len()<1024,"Original PG history exceeds1024 versions");let change_set:Vec<u8>=row.try_get("change_set")?;total=pg_history_bytes(total,change_set.len())?;let selected=PgOriginalHistory{id:row.try_get("id")?,document:row.try_get("document_id")?,version:row.try_get("version_number")?,change_set,actor:row.try_get("created_by")?,created:row.try_get("created_at")?,added:row.try_get("lines_added")?,removed:row.try_get("lines_removed")?};ensure!(!selected.id.is_empty()&&selected.document==*document&&selected.version>0&&!selected.actor.is_empty(),"Original PG history identity invalid");result.push(selected);}
 }Ok(result)
}
impl StoppedPgVerificationSource<'_>{
 async fn canonical_history_plan(&mut self,pages:&PgCanonicalPagePlan)->anyhow::Result<PgCanonicalHistoryPlan>{
  self.fence().await?;pages.fence(self)?;let documents:Vec<String>=sqlx::query_scalar("SELECT id FROM documents WHERE archived_at IS NULL AND deleted_at IS NULL ORDER BY id LIMIT 257").fetch_all(&mut *self.conn).await?;ensure!(documents.len()<=256,"Original complete active history exceeds256 pages");let rows=pg_read_history(self.conn,&documents).await?;
  for page in &pages.pages {let selected=rows.iter().filter(|r|r.document==page.id).collect::<Vec<_>>();ensure!(selected.len()==page.history.len(),"Original PG history presentation closure differs");for (row,metadata)in selected.iter().zip(&page.history){ensure!(metadata["id"]==row.id&&metadata["version_number"]==row.version&&metadata["created_by"]==row.actor&&metadata["created_at"]==row.created&&metadata["lines_added"]==row.added&&metadata["lines_removed"]==row.removed,"Original PG history raw/presentation binding differs");}}
  self.fence().await?;Ok(PgCanonicalHistoryPlan{source:self.root_identity()?,attestation_sha:digest(&self.serving.proof.raw_attestation),actor:self.admin.claims.sub.clone(),session:self.admin.claims.sid.clone(),documents,rows})
 }
}
impl PgCanonicalHistoryPlan {
 fn fence(&self,original:&StoppedPgVerificationSource<'_>)->anyhow::Result<()>{ensure!(self.source==original.root_identity()?&&self.attestation_sha==digest(&original.serving.proof.raw_attestation)&&self.actor==original.admin.claims.sub&&self.session==original.admin.claims.sid,"Original PG history context differs");original.physical()?;Ok(())}
 fn verify_rows(&self,actual:&[PgOriginalHistory])->anyhow::Result<()>{ensure!(self.rows==actual,"Original PG history exact binary/identity/actor/version metadata differs");Ok(())}
 async fn verify_target(&self,target:&RestoredPgServing)->anyhow::Result<()>{target.fence().await?;let mut conn=target.pool.acquire().await?;target.proof.verify_any_connection(&mut conn).await?;let actual=pg_read_history(&mut conn,&self.documents).await?;self.verify_rows(&actual)?;target.proof.verify_any_connection(&mut conn).await?;drop(conn);target.fence().await?;Ok(())}
}
#[cfg(test)]mod pg_history_controls{use super::*;
 #[test]fn exact_history_binary_actor_and_bounds_refuse(){
  fn row()->PgOriginalHistory{PgOriginalHistory{id:"original-history".into(),document:"original-page".into(),version:1,change_set:vec![0,1,2,255],actor:"original-human".into(),created:"original-time".into(),added:0,removed:0}}
  let plan=PgCanonicalHistoryPlan{source:(0,0),attestation_sha:String::new(),actor:String::new(),session:String::new(),documents:vec!["original-page".into()],rows:vec![row()]};let mut foreign=vec![row()];assert!(plan.verify_rows(&foreign).is_ok());foreign[0].change_set[3]=254;assert!(plan.verify_rows(&foreign).is_err());foreign[0]=row();foreign[0].actor="copied-actor".into();assert!(plan.verify_rows(&foreign).is_err());foreign[0]=row();foreign[0].version=2;assert!(plan.verify_rows(&foreign).is_err());assert!(plan.verify_rows(&[]).is_err());assert!(pg_history_bytes(0,8*1024*1024).is_ok());assert!(pg_history_bytes(0,8*1024*1024+1).is_err());assert!(pg_history_bytes(16*1024*1024,1).is_err());assert!(pg_history_bytes(usize::MAX,1).is_err());
 }
}
