//! Current ordinary editors may inspect typed publication history; newer viewers
//! do not inherit private historical content. No legacy CRDT or restore adapter.
use crate::{AppState,auth::{middleware::AuthUser,routes::AppError}};
use super::{access,publication_snapshots as core,published_source};
use axum::{extract::{Path,State},routing::get,Json,Router,body::Body,http::{Response,header}};
use serde_json::{json,Value};

pub fn router()->Router<AppState>{Router::new()
    .route("/spaces/{space_id}/documents/{doc_id}/publication-snapshots",get(list))
    .route("/spaces/{space_id}/documents/{doc_id}/publication-snapshots/{snapshot_id}",get(detail))
    .route("/spaces/{space_id}/documents/{doc_id}/publication-snapshots/{snapshot_id}/raw",get(raw))}
fn changed()->AppError{AppError::Conflict("source_conflict: Publication snapshot or current access changed; refresh history".into())}
async fn authority(conn:&mut sqlx::AnyConnection,auth:&AuthUser,space:&str,doc:&str)->Result<access::PageAccess,AppError>{
    access::require_current_human_preference_write(conn,auth).await?;
    access::resolve_page_access(conn,access::principal_from_auth(auth)?,space,doc,access::PageOperation::Edit,access::Purpose::Routine).await
}
async fn snapshot_rows(conn:&mut sqlx::AnyConnection,doc:&str)->Result<Vec<Value>,AppError>{
    super::document_deletion::rows(conn,"SELECT * FROM document_publication_snapshots WHERE document_id=$1",&[doc]).await.map_err(AppError::internal)
}
async fn read(state:&AppState,auth:&AuthUser,space:&str,doc:&str,id:Option<&str>)->Result<Vec<core::Stored>,AppError>{
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    let page=authority(&mut conn,auth,space,doc).await?;
    let mut proof=published_source::capture_source(&mut conn,auth,space,doc,&page.ancestor_ids).await?;
    let schema="SELECT * FROM properties_schema WHERE space_id=$1";
    proof.push(published_source::SourceProof{query:schema.into(),parameters:vec![space.into()],rows:super::document_deletion::rows(&mut conn,schema,&[space]).await.map_err(AppError::internal)?});
    let original=snapshot_rows(&mut conn,doc).await?;
    // Keep the exact already-validated SQL envelopes locally for a final
    // synchronous custody check after all authority/credential awaits.
    let receipt_query="SELECT * FROM draft_publication_receipts WHERE document_id=$1";
    let original_receipts=super::document_deletion::rows(&mut conn,receipt_query,&[doc]).await.map_err(AppError::internal)?;
    let result=core::read_validated(&mut conn,doc).await?;
    #[cfg(test)] pause_before_inventory().await;
    published_source::verify_source(&mut conn,&proof).await?;
    if snapshot_rows(&mut conn,doc).await?!=original{return Err(changed())}
    #[cfg(test)] pause_after_inventory().await;
    authority(&mut conn,auth,space,doc).await?;
    // Shared validation proves all operation-group records and original manifest,
    // not current canonical paths or a later asset audience.
    if core::read_validated(&mut conn,doc).await?!=result{return Err(changed())}
    published_source::verify_source(&mut conn,&proof).await?;
    if snapshot_rows(&mut conn,doc).await?!=original{return Err(changed())}
    if super::document_deletion::rows(&mut conn,receipt_query,&[doc]).await.map_err(AppError::internal)?!=original_receipts{return Err(changed())}
    authority(&mut conn,auth,space,doc).await?;
    access::require_current_human_preference_write(&mut conn,auth).await?;
    // No further I/O awaits: original receipt equality and shared typed group
    // validation precede these final rooted immutable manifest/custody checks.
    for receipt in &original_receipts {
        let manifest=super::draft_save::string(receipt,"storage_manifest_json")?;
        let prepared:crate::storage::publication::PreparedPublication=serde_json::from_str(&manifest).map_err(|_|changed())?;
        crate::storage::publication::verify_snapshot_manifest(&prepared)?;
    }
    if let Some(id)=id {let selected=result.into_iter().filter(|r|r.id==id).collect::<Vec<_>>();if selected.len()!=1{return Err(AppError::NotFound("Publication snapshot unavailable".into()))}Ok(selected)}else{Ok(result)}
}
fn summary(row:&core::Stored)->Result<Value,AppError>{let p=&row.payload;Ok(json!({
    "schema_version":1,"id":row.id,"document_id":p.document_id,"captured_space_id":p.captured_space_id,
    "kind":row.kind,"capture_operation_id":row.operation,"publication_operation_id":if row.kind=="publication"{Some(&row.operation)}else{None},
    "published_at":row.published_at,"captured_at":row.captured_at,"captured_by":row.captured_by,
    "snapshot_digest":core::digest(&json!(p))?,"binding":p.binding,
    "content":{"title":p.content.title,"icon":p.content.icon,"cover_reference":p.content.cover_reference,"extraction":p.content.extraction,"raw_sha256":p.content.raw_sha256,"raw_byte_length":p.content.raw_byte_length}}))}
fn projection(row:&core::Stored,space:&str,doc:&str)->Result<Value,AppError>{
    let mut out=summary(row)?;let p=&row.payload;
    out["content"]["portable_markdown"]=json!(p.content.portable_markdown);
    out["comparison_policy"]=json!(p.comparison_policy);out["asset_policy"]=json!(p.asset_policy);
    out["comparisons"]=json!({
        "properties_schema":p.comparisons.properties_schema.iter().filter(|r|p.comparisons.property_values.iter().any(|v|v["property_id"]==r["id"])).map(|r|json!({"id":r["id"],"name":r["name"],"property_type":r["property_type"],"options_raw":r["options"],"sort_order":r["sort_order"]})).collect::<Vec<_>>(),
        "property_values":p.comparisons.property_values.iter().map(|r|json!({"property_id":r["property_id"],"value":r["value"]})).collect::<Vec<_>>(),
        "attachments":p.comparisons.attachments.iter().map(|r|json!({"id":r["id"],"original_name":r["original_name"],"mime_type":r["mime_type"],"size_bytes":r["size_bytes"]})).collect::<Vec<_>>(),
        "drawings":p.comparisons.drawings.iter().map(|r|json!({"id":r["id"],"filename":r["filename"]})).collect::<Vec<_>>()});
    out["raw_download_url"]=json!(format!("/api/v1/spaces/{}/documents/{}/publication-snapshots/{}/raw",urlencoding::encode(space),urlencoding::encode(doc),urlencoding::encode(&row.id)));
    Ok(out)
}
async fn list(State(s):State<AppState>,a:AuthUser,Path((space,doc)):Path<(String,String)>)->Result<([(header::HeaderName, &'static str); 1], Json<Value>),AppError>{
    let rows=read(&s,&a,&space,&doc,None).await?;let snapshots=rows.iter().map(summary).collect::<Result<Vec<_>,_>>()?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(json!({"schema_version":1,"space_id":space,"document_id":doc,"snapshots":snapshots}))))
}
async fn detail(State(s):State<AppState>,a:AuthUser,Path((space,doc,id)):Path<(String,String,String)>)->Result<([(header::HeaderName, &'static str); 1], Json<Value>),AppError>{
    let rows=read(&s,&a,&space,&doc,Some(&id)).await?;Ok(([(header::CACHE_CONTROL, "no-store")], Json(json!({"schema_version":1,"space_id":space,"document_id":doc,"snapshot":projection(&rows[0],&space,&doc)?}))))
}
async fn raw(State(s):State<AppState>,a:AuthUser,Path((space,doc,id)):Path<(String,String,String)>)->Result<Response<Body>,AppError>{
    let mut rows=read(&s,&a,&space,&doc,Some(&id)).await?;let row=rows.remove(0);
    Response::builder().header(header::CONTENT_TYPE,"application/octet-stream").header(header::CONTENT_DISPOSITION,"attachment; filename=\"publication-snapshot.md\"")
        .header(header::CONTENT_LENGTH,row.raw.len().to_string()).header(header::CACHE_CONTROL,"no-store").header("X-Content-Type-Options","nosniff")
        .header("X-Starlane-Snapshot-Id",&row.id).header("X-Starlane-Snapshot-Sha256",&row.payload.content.raw_sha256)
        .body(Body::from(row.raw)).map_err(AppError::internal)
}
#[cfg(test)] tokio::task_local!{
    static BEFORE_INVENTORY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    static AFTER_INVENTORY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
}
#[cfg(test)] async fn pause_before_inventory(){if let Ok(Some(h))=BEFORE_INVENTORY.try_with(|s|s.borrow_mut().take()){h.await;}}
#[cfg(test)] async fn pause_after_inventory(){if let Ok(Some(h))=AFTER_INVENTORY.try_with(|s|s.borrow_mut().take()){h.await;}}
#[cfg(test)] #[path="publication_snapshot_reader_tests.rs"] mod tests;
