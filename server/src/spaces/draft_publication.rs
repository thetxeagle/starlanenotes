//! Exact owner-confirmed publication intent and immutable route receipt.
//! Historical receipts acknowledge only the original operation, never adopt a
//! current page or recreate a consumed draft. Generic journal recovery is separate.
use crate::{auth::{middleware::AuthUser, routes::AppError}, storage};
use super::{access, draft_save, document_deletion, published_source};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::AnyConnection;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub schema_version: u32,
    pub operation_id: String,
    pub draft_source_digest: String,
    pub page_source_digest: String,
}
pub(crate) fn conflict() -> AppError {
    AppError::Conflict("source_conflict: Publication source or receipt changed; retain the original publication intent for explicit review".into())
}
pub(crate) fn required() -> AppError {
    AppError::Conflict("source_required: Publication requires schema_version 1, original operation_id and reviewed draft/page source digests".into())
}
pub(crate) fn validate(request: &Request) -> Result<(), AppError> {
    if request.schema_version != 1 || uuid::Uuid::parse_str(&request.operation_id).is_err() {
        return Err(required());
    }
    for value in [&request.draft_source_digest, &request.page_source_digest] {
        draft_save::required(Some(&json!(value))).map_err(|_| required())?;
    }
    Ok(())
}
pub(crate) fn projection(draft: &Value, content: &str) -> Value {
    json!({"title":draft["title"],"content":content,"icon":draft["icon"],"cover_image":draft["cover_image"]})
}
pub(crate) fn page_digest(page: &Value, bytes: Option<&[u8]>) -> Result<String, AppError> {
    if page["published_at"].is_null() {
        let mut h = Sha256::new();
        h.update(b"starlane-unpublished-publication-page-v1\0");
        h.update(draft_save::digest(page)?.as_bytes());
        Ok(hex::encode(h.finalize()))
    } else {
        published_source::digest(page, bytes.ok_or_else(conflict)?)
    }
}
pub(crate) async fn page_bytes(page: &Value) -> Result<Option<Vec<u8>>, AppError> {
    if page["published_at"].is_null() { return Ok(None); }
    let path = draft_save::string(page, "content_path")?;
    if !path.starts_with("spaces/") || !std::path::Path::new(&path).components().all(|part| matches!(part, std::path::Component::Normal(_))) { return Err(conflict()); }
    let body = storage::read_document(&path).await.map_err(|_| conflict())?;
    Ok(Some(body.into_bytes()))
}
pub(crate) fn verify_page_bytes(page: &Value, bytes: &Option<Vec<u8>>) -> Result<(), AppError> {
    if let Some(bytes) = bytes {
        let path = draft_save::string(page, "content_path")?;
        if std::fs::read(storage::data_dir().join(path)).map_err(|_| conflict())? != *bytes { return Err(conflict()); }
    }
    Ok(())
}
fn fingerprint(auth: &AuthUser, space: &str, document: &str, draft: &str, request: &Request) -> Result<String, AppError> {
    draft_save::digest(&json!({"actor_id":auth.claims.sub,"space_id":space,"document_id":document,"draft_id":draft,"request":request}))
}
pub(crate) fn storage_fingerprint(auth:&AuthUser,space:&str,document:&str,draft_id:&str,request:&Request,draft:&Value)->Result<String,AppError> {
    draft_save::digest(&json!({"publication_request_fingerprint":fingerprint(auth,space,document,draft_id,request)?,"original_draft":draft}))
}
async fn rows(conn: &mut AnyConnection, operation: &str) -> Result<Vec<Value>, AppError> {
    document_deletion::rows(conn, "SELECT * FROM draft_publication_receipts WHERE operation_id=$1", &[operation]).await.map_err(AppError::internal)
}
pub(crate) struct Recorded {
    pub row: Value,
    pub response: Value,
}
pub(crate) async fn record(
    conn: &mut AnyConnection, auth: &AuthUser, space: &str, document: &str, draft_id: &str,
    request: &Request, draft: &Value, page: &Value,
    prepared: &storage::publication::PreparedPublication, content: &str,
) -> Result<Recorded, AppError> {
    let bytes = page_bytes(page).await?.ok_or_else(conflict)?;
    let response = json!({"schema_version":1,"operation_id":request.operation_id,"space_id":space,"document_id":document,"draft_id":draft_id,
        "draft_source_digest":request.draft_source_digest,"page_source_digest":request.page_source_digest,
        "published_source_digest":published_source::digest(page,&bytes)?,"published_at":page["published_at"],
        "publication":projection(draft,content),"disposition":"applied"});
    let row = json!({"operation_id":request.operation_id,"actor_id":auth.claims.sub,"space_id":space,"document_id":document,"draft_id":draft_id,
        "request_fingerprint":fingerprint(auth,space,document,draft_id,request)?,"request_json":serde_json::to_string(request).map_err(AppError::internal)?,
        "draft_json":serde_json::to_string(draft).map_err(AppError::internal)?,"result_page_json":serde_json::to_string(page).map_err(AppError::internal)?,
        "response_json":serde_json::to_string(&response).map_err(AppError::internal)?,"storage_manifest_json":serde_json::to_string(prepared).map_err(AppError::internal)?});
    sqlx::query("INSERT INTO draft_publication_receipts(operation_id,actor_id,space_id,document_id,draft_id,request_fingerprint,request_json,draft_json,result_page_json,response_json,storage_manifest_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
        .bind(&request.operation_id).bind(&auth.claims.sub).bind(space).bind(document).bind(draft_id)
        .bind(draft_save::string(&row,"request_fingerprint")?).bind(draft_save::string(&row,"request_json")?)
        .bind(draft_save::string(&row,"draft_json")?).bind(draft_save::string(&row,"result_page_json")?)
        .bind(draft_save::string(&row,"response_json")?).bind(draft_save::string(&row,"storage_manifest_json")?)
        .execute(&mut *conn).await.map_err(AppError::internal)?;
    verify_record(conn,&row).await?;
    Ok(Recorded { row, response })
}
pub(crate) async fn verify_record(conn: &mut AnyConnection, expected: &Value) -> Result<(), AppError> {
    if rows(conn,&draft_save::string(expected,"operation_id")?).await? != vec![expected.clone()] { return Err(conflict()); }
    Ok(())
}
fn parsed(row: &Value, field: &str) -> Result<Value, AppError> {
    serde_json::from_str(&draft_save::string(row,field)?).map_err(|_| conflict())
}
/// Current page authority substitutes for consumed-draft resolution ONLY after
/// immutable same-actor request/source/storage evidence proves that exact birth.
pub(crate) async fn retry(
    conn: &mut AnyConnection, auth: &AuthUser, space: &str, document: &str, draft_id: &str, request: &Request,
) -> Result<Option<Value>, AppError> {
    let found = rows(conn,&request.operation_id).await?;
    if found.is_empty() { return Ok(None); }
    if found.len()!=1 { return Err(conflict()); }
    let row=&found[0];
    if row["actor_id"]!=auth.claims.sub || row["space_id"]!=space || row["document_id"]!=document || row["draft_id"]!=draft_id ||
        row["request_fingerprint"]!=fingerprint(auth,space,document,draft_id,request)? || parsed(row,"request_json")?!=json!(request) { return Err(conflict()); }
    let draft=parsed(row,"draft_json")?;
    let page=parsed(row,"result_page_json")?;
    let mut response=parsed(row,"response_json")?;
    let prepared:storage::publication::PreparedPublication=serde_json::from_str(&draft_save::string(row,"storage_manifest_json")?).map_err(|_| conflict())?;
    let manifest=json!(prepared);
    if draft["id"]!=draft_id || draft["space_id"]!=space || draft["document_id"]!=document || draft["owner_id"]!=auth.claims.sub || draft_save::digest(&draft)?!=request.draft_source_digest ||
        prepared.op_id()!=request.operation_id || page["id"]!=document || page["space_id"]!=space || page["content_path"]!=prepared.path() || page["updated_at"]!=prepared.revision() || page["published_at"]!=prepared.revision() || page["last_updated_by"]!=auth.claims.sub ||
        manifest["actor_id"]!=auth.claims.sub || manifest["space_id"]!=space || manifest["document_id"]!=document || manifest["draft_id"]!=draft_id ||
        manifest["payload_fingerprint"].as_str().is_none() { return Err(conflict()); }
    // Legacy acknowledgements stay exact and are never retrofitted from current
    // metadata. Fresh v2 receipts require the independently anchored capture.
    let base = storage_fingerprint(auth,space,document,draft_id,request,&draft)?;
    let captures = super::publication_snapshots::read_validated(conn, document).await?;
    let own: Vec<_> = captures.iter().filter(|row| row.operation == request.operation_id).collect();
    if manifest["payload_fingerprint"] != base && !own.iter().any(|row| row.kind == "publication") { return Err(conflict()); }
    let content=crate::template_guidance::strip_template_guidance(&draft_save::string(&draft,"content")?);
    let bytes=page_bytes(&page).await?.ok_or_else(conflict)?;
    let expected=json!({"schema_version":1,"operation_id":request.operation_id,"space_id":space,"document_id":document,"draft_id":draft_id,
        "draft_source_digest":request.draft_source_digest,"page_source_digest":request.page_source_digest,
        "published_source_digest":published_source::digest(&page,&bytes)?,"published_at":page["published_at"],"publication":projection(&draft,&content),"disposition":"applied"});
    if response!=expected || page["title"]!=draft["title"] || page["icon"]!=draft["icon"] || page["cover_image"]!=draft["cover_image"] { return Err(conflict()); }
    storage::publication::verify_recorded_publication(conn,&prepared).await?;
    let principal=access::principal_from_auth(auth)?;
    access::resolve_page_access(conn,principal,space,document,access::PageOperation::Edit,access::Purpose::Routine).await?;
    verify_record(conn,row).await?;
    storage::publication::verify_recorded_publication(conn,&prepared).await?;
    access::resolve_page_access(conn,principal,space,document,access::PageOperation::Edit,access::Purpose::Routine).await?;
    access::require_current_human_preference_write(conn,auth).await?;
    storage::publication::verify_prepared_publication(&prepared)?;
    verify_page_bytes(&page,&Some(bytes))?;
    response["disposition"]=json!("historical");
    Ok(Some(response))
}
