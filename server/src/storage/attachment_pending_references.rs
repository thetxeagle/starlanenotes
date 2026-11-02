//! Same-held-transaction global reference observation for an original pending producer.
use super::*;
fn review()->AppError{AppError::Conflict("Pending original references require retained review".into())}
fn exclude(source:&str,asset:Option<&str>,path:Option<&str>)->Result<(),AppError>{
 if let Some(asset)=asset{crate::storage::attachment_erasure_source::exclude(source,asset,path.unwrap_or("\0pending-no-public-path")).map_err(|_|review())?;}Ok(())
}
pub(super) async fn prove(conn:&mut sqlx::AnyConnection,asset:Option<&str>,path:Option<&str>)->Result<std::collections::BTreeSet<String>,AppError>{
 prove_in_at(&crate::storage::data_dir(),conn,asset,path).await
}
pub(super) async fn prove_in_at(root:&std::path::Path,conn:&mut sqlx::AnyConnection,asset:Option<&str>,path:Option<&str>)->Result<std::collections::BTreeSet<String>,AppError>{
 let history:(i64,i64)=sqlx::query_as("SELECT count(*),COALESCE(SUM(length(change_set)),0) FROM doc_versions").fetch_one(&mut *conn).await.map_err(AppError::internal)?;
 // The retained CRDT changes are opaque to this bounded adapter. No substring
 // shortcut can establish they do not reference the selected asset.
 if history.0>512||history.1!=0{return Err(review())}
 // The current snapshot survives independently from retained history (for
 // example after legacy imports or pruning). Without a semantic decoder it
 // cannot establish reference absence, even when doc_versions is empty.
 let snapshots:(i64,i64)=sqlx::query_as("SELECT count(*),COALESCE(SUM(COALESCE(length(automerge_state),0)),0) FROM documents").fetch_one(&mut *conn).await.map_err(AppError::internal)?;
 if snapshots.0>512||snapshots.1!=0{return Err(review())}

 // Registered body-bearing consumers also retain references outside the page
 // Markdown: comments, reusable templates, property values and workflow notes.
 let mut total=0usize;
 for table in ["comments","templates","template_drafts","template_revisions","property_values","properties_schema","document_workflow_events","document_workflows","workflow_templates"] {
  let count:i64=sqlx::query_scalar(&format!("SELECT count(*) FROM {table}")).fetch_one(&mut *conn).await.map_err(AppError::internal)?;if count>512{return Err(review())}
  let rows=crate::spaces::document_deletion::rows(&mut *conn,&format!("SELECT * FROM {table} LIMIT 513"),&[]).await.map_err(|_|review())?;
  for row in rows {let raw=serde_json::to_string(&row).map_err(AppError::internal)?;total=total.checked_add(raw.len()).ok_or_else(review)?;if total>8*1024*1024{return Err(review())}exclude(&raw,asset,path)?;}
 }
 let drawings:i64=sqlx::query_scalar("SELECT count(*) FROM excalidraw_files").fetch_one(&mut *conn).await.map_err(AppError::internal)?;
 if drawings!=0{return Err(review())} // Opaque retained drawing payloads are unsupported.
 let(count,size):(i64,i64)=sqlx::query_as("SELECT count(*),COALESCE(SUM(length(content)+COALESCE(length(cover_image),0)),0) FROM document_drafts").fetch_one(&mut *conn).await.map_err(AppError::internal)?;if count>512||size>8*1024*1024{return Err(review())}
 let drafts:Vec<(String,Option<String>)>=sqlx::query_as("SELECT content,cover_image FROM document_drafts LIMIT 513").fetch_all(&mut *conn).await.map_err(AppError::internal)?;if drafts.len()>512{return Err(review())}
 for(content,cover)in drafts{if let Some(cover)=cover{total=total.checked_add(cover.len()).ok_or_else(review)?;if total>8*1024*1024{return Err(review())}exclude(&cover,asset,path)?;}total=total.checked_add(content.len()).ok_or_else(review)?;if total>8*1024*1024{return Err(review())}exclude(&content,asset,path)?;}
 let(count,size):(i64,i64)=sqlx::query_as("SELECT count(*),COALESCE(SUM(COALESCE(length(cover_image),0)),0) FROM documents").fetch_one(&mut *conn).await.map_err(AppError::internal)?;if count>512||size>8*1024*1024{return Err(review())}
 let documents:Vec<(String,String,Option<String>)>=sqlx::query_as("SELECT id,content_path,cover_image FROM documents ORDER BY id LIMIT 513").fetch_all(&mut *conn).await.map_err(AppError::internal)?;if documents.len()>512{return Err(review())}
 for(_,document_path,cover)in documents {if let Some(cover)=cover{total=total.checked_add(cover.len()).ok_or_else(review)?;if total>8*1024*1024{return Err(review())}exclude(&cover,asset,path)?;}let bytes=crate::storage::journal_fs::source_bytes(root,&document_path).map_err(|_|review())?;total=total.checked_add(bytes.len()).ok_or_else(review)?;if total>8*1024*1024{return Err(review())}let source=std::str::from_utf8(&bytes).map_err(|_|review())?;exclude(source,asset,path)?;}
 let mut finalized_journals=std::collections::BTreeSet::new();
 // Completed historical receipts are provenance, not current source grants.
 // Skip their staged sources only after exact existing inspection proves that
 // retry cannot introduce a new publication. Pending/uncommitted operations
 // remain fully inspected by the native family census.
 let rows=crate::spaces::document_deletion::rows(&mut *conn,"SELECT * FROM storage_journal_receipts LIMIT 513",&[]).await.map_err(|_|review())?;if rows.len()>512{return Err(review())}for row in rows{
  let op=row["op_id"].as_str().ok_or_else(review)?;
  let inspected=crate::storage::journal::inspect_restoration(root,&mut *conn,op).await.map_err(|_|review())?;
  if inspected.class==crate::storage::journal::StorageOperationClass::CommittedComplete{finalized_journals.insert(format!(".storage-journal/{op}/"));}
 }
 let rows=crate::spaces::document_deletion::rows(&mut *conn,"SELECT * FROM publication_receipts LIMIT 513",&[]).await.map_err(|_|review())?;if rows.len()>512{return Err(review())}for row in rows{
  let op=row["op_id"].as_str().ok_or_else(review)?;
  let inspected=crate::storage::publication::inspect_restoration(root,&mut *conn,op).await.map_err(|_|review())?;
  if matches!(inspected.class,crate::storage::publication::RestorationOperationClass::CommittedHistorical|crate::storage::publication::RestorationOperationClass::CommittedCurrent){finalized_journals.insert(format!(".publication-journal/{op}/"));}
 }
 let rows=crate::spaces::document_deletion::rows(&mut *conn,"SELECT content_path FROM canonical_revision_receipts LIMIT 513",&[]).await.map_err(|_|review())?;
 if rows.len()>512{return Err(review())}for row in rows{let p=row["content_path"].as_str().ok_or_else(review)?;let raw=crate::storage::journal_fs::source_bytes(root,p).map_err(|_|review())?;total=total.checked_add(raw.len()).ok_or_else(review)?;if total>8*1024*1024{return Err(review())}exclude(std::str::from_utf8(&raw).map_err(|_|review())?,asset,path)?;}
 Ok(finalized_journals)
}
