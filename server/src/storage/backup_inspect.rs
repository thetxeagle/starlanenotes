//! Operator-only, read-only backup evidence report. Never restores or starts services.
use serde::Serialize;
use std::{collections::BTreeSet,path::Path};
#[derive(Serialize)]
pub(crate) struct Report {
 pub backup_id:String,
 pub inventory_and_database_verified:bool,
 pub portable_restore_available:bool,
 pub operations:Vec<Operation>,
 pub issues:Vec<Issue>,
}
#[derive(Serialize)]
pub(crate) struct Operation {family:String,operation_id:String,class:String,physical_group_paths:Vec<String>,provider:Option<String>}
#[derive(Serialize)]
pub(crate) struct Issue {family:String,operation_id:Option<String>,code:String}
impl Report {
 fn issue(&mut self,family:&str,op:Option<&str>,code:&str){self.issues.push(Issue{family:family.into(),operation_id:op.map(str::to_owned),code:code.into()});}
 fn operation(&mut self,family:&str,op:&str,class:String,group:Vec<String>){self.operations.push(Operation{family:family.into(),operation_id:op.into(),class,physical_group_paths:group,provider:None});}
}
#[cfg(target_os="linux")]
pub(crate) async fn run(path:&Path)->anyhow::Result<Report> {
 let bundle=super::backup_inventory::verify(path).await?;
 let root=bundle.data_path();let mut conn=bundle.db.acquire().await?;
 let tables:BTreeSet<String>=sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table'").fetch_all(&mut *conn).await?.into_iter().collect();
 let mut canonical_evidence=Vec::new();
 let mut report=Report{backup_id:bundle.manifest.backup_id.clone(),inventory_and_database_verified:true,portable_restore_available:false,operations:Vec::new(),issues:Vec::new()};
 for (namespace,table) in [(".storage-journal","storage_journal_receipts"),(".publication-journal","publication_receipts"),(".canonical-rewrite-journal","canonical_revision_receipts"),(".move-relocation-journal","move_commit_receipts")] {
  let mut ops:BTreeSet<String>=bundle.manifest.source_custody.directories.0.keys().filter_map(|p|p.strip_prefix(&format!("{namespace}/")).filter(|p|!p.contains('/')).map(str::to_owned)).collect();
  let mut committed=BTreeSet::new();if tables.contains(table){committed=sqlx::query_scalar::<_,String>(&format!("SELECT op_id FROM {table}")).fetch_all(&mut *conn).await?.into_iter().collect();ops.extend(committed.iter().cloned());}
  for op in ops {
   if uuid::Uuid::parse_str(&op).is_err(){report.issue(namespace,Some(&op),"invalid_operation_identity");continue;}
   let result=match namespace {
    ".storage-journal"=>super::journal::inspect_restoration(&root,&mut conn,&op).await.map(|i|(format!("{:?}",i.class),i.install.required_group)),
    ".publication-journal"=>super::publication::inspect_restoration(&root,&mut conn,&op).await.map(|i|(format!("{:?}",i.class),i.install.required_group)),
    ".canonical-rewrite-journal"=>super::canonical_rewrite::inspect_restoration(&root,&mut conn,&op).await.map(|i|{let result=(format!("{:?}",i.class),i.install.required_group.clone());canonical_evidence.push(i);result}),
    _=>super::relocation::restoration_inspection::inspect(&root,&mut conn,&op).await.map(|i|(format!("{:?}",i.state),i.paths.into_iter().filter(|p|p.required&&p.original_identity.is_some()).map(|p|p.relative).collect())),
   };
   match result {Ok((class,group))=>report.operation(namespace,&op,class,group),Err(_)=>report.issue(namespace,Some(&op),if committed.contains(&op){"committed_evidence_requires_review"}else{"uncommitted_stage_retained_for_review"})}
  }
 }
 if tables.contains("attachment_upload_receipts") {
  let rows: Vec<(String,String)>=sqlx::query_as("SELECT attempt_id,operation_id FROM attachment_upload_receipts ORDER BY attempt_id").fetch_all(&mut *conn).await?;
  for (attempt,operation) in rows {
   match super::attachment_upload::inspect_restoration(&root,&mut conn,&operation).await {
    Ok(i) if i.receipt.intent.attempt_id==attempt=>report.operation(".attachment-upload-journal",&attempt,format!("{:?}",i.class),i.required_group),
    _=>report.issue(".attachment-upload-journal",Some(&attempt),"committed_evidence_requires_review"),
   }
  }
 }
 if tables.contains("asset_replay_receipts") {
  match crate::sync::restoration_assets::classify_at(&root,&mut conn).await {
   Ok(receipts)=>for r in receipts {let op=&r.receipt.incoming.operation_id;report.operation("asset_replay_receipts",op,if r.current_attachment{"CurrentAttachment"}else{"Historical"}.into(),Vec::new());if let Some(last)=report.operations.last_mut(){last.provider=Some(r.receipt.incoming.provider.clone());}if r.requires_canonical_evidence && !canonical_evidence.iter().any(|i|i.matches_asset_receipt(&r.receipt)){report.issue("asset_replay_receipts",Some(op),"published_source_dependency_requires_resolution");}},
   Err(_)=>report.issue("asset_replay_receipts",None,"asset_receipt_evidence_requires_review"),
  }
 }
 if bundle.manifest.source_custody.directories.0.contains_key(".restoration-custody") || tables.contains("storage_restore_current") && sqlx::query_scalar::<_,i64>("SELECT count(*) FROM storage_restore_current").fetch_one(&mut *conn).await?>0 || tables.contains("storage_restore_generations") && sqlx::query_scalar::<_,i64>("SELECT count(*) FROM storage_restore_generations").fetch_one(&mut *conn).await?>0 {
  report.issue(".restoration-custody",None,"prior_generation_provenance_requires_resolution");
 }
 for namespace in [".storage-journal", ".publication-journal", ".canonical-rewrite-journal", ".move-relocation-journal", ".attachment-upload-journal"] {
  if bundle.manifest.source_custody.directories.0.contains_key(namespace) {
   // Typed operation inspection does not authenticate all captured namespace
   // bytes or establish original hardlink ownership. Never treat this report
   // as a custody plan, including root-only/extra retained evidence.
   report.issue(namespace,None,"namespace_inventory_and_physical_custody_require_resolution");
  }
 }
 drop(conn);bundle.fence()?;bundle.db.close().await;Ok(report)
}
#[cfg(not(target_os="linux"))]
pub(crate) async fn run(_: &Path)->anyhow::Result<Report>{anyhow::bail!("Application backup inspection currently requires Linux")}
