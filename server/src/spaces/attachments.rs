#[cfg(target_os="linux")]
#[path="attachment_upload_body.rs"]
mod upload_body;
#[cfg(target_os="linux")]
pub(crate) struct UploadMultipart { inner:Multipart,maximum:usize }
#[cfg(target_os="linux")]
impl std::ops::Deref for UploadMultipart {type Target=Multipart;fn deref(&self)->&Multipart{&self.inner}}
#[cfg(target_os="linux")]
impl std::ops::DerefMut for UploadMultipart {fn deref_mut(&mut self)->&mut Multipart{&mut self.inner}}
#[cfg(target_os="linux")]
impl axum::extract::FromRequest<AppState> for UploadMultipart {
 type Rejection=AppError;
 async fn from_request(mut request:axum::extract::Request,state:&AppState)->Result<Self,AppError>{
  let maximum=max_upload_size_bytes(state).await?;let body=std::mem::replace(request.body_mut(),axum::body::Body::empty());*request.body_mut()=upload_body::bounded(body,maximum);
  let inner=Multipart::from_request(request,state).await.map_err(|_|AppError::BadRequest("Invalid upload body".into()))?;Ok(Self{inner,maximum})
 }
}
#[cfg(target_os="linux")]
#[path="attachment_copy.rs"]
mod copy;
#[path = "attachment_download.rs"]
mod download;

use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
#[cfg(target_os="linux")]
use axum::extract::FromRequest;
use crate::storage;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/attachments/copy",axum::routing::post(copy_attachment))
        .route("/spaces/{space_id}/documents/{doc_id}/attachment-copy-source/{asset_id}",axum::routing::get(legacy_copy_source))
        .route("/files/{*storage_path}", get(serve_attachment_file))
        .route("/spaces/{space_id}/documents/{doc_id}/attachment-upload-attempts",axum::routing::get(discover_interrupted_attachments))
        .route("/spaces/{space_id}/documents/{doc_id}/attachment-upload-attempts/{attempt_id}/cleanup",axum::routing::post(cleanup_interrupted_attachment))
        .route("/spaces/{space_id}/documents/{doc_id}/attachment-retirements", get(list_attachment_retirements))
        .route(
            "/spaces/{space_id}/attachments",
            get(list_space_attachments),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/attachments",
            get(list_attachments)
                .post(upload_attachment)
                .layer(DefaultBodyLimit::disable()),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/attachments/{attachment_id}",
            axum::routing::delete(delete_attachment),
        )
}

#[cfg(target_os="linux")]
async fn copy_attachment(state:State<AppState>,auth:AuthUser,path:Path<(String,String,String)>,body:Json<copy::CopyRequest>)->Result<Response,AppError>{let (status,body)=copy::copy_attachment(state,auth,path,body).await?;Ok((status,[(header::CACHE_CONTROL,"private, no-store")],body).into_response())}
#[cfg(not(target_os="linux"))]
async fn copy_attachment()->Result<StatusCode,AppError>{Err(AppError::BadRequest("Recoverable attachment copy requires supported platform".into()))}

#[cfg(target_os="linux")]
async fn legacy_copy_source(state:State<AppState>,auth:AuthUser,path:Path<(String,String,String)>)->Result<Response,AppError>{copy::legacy_source(state,auth,path).await}
#[cfg(not(target_os="linux"))]
async fn legacy_copy_source()->Result<StatusCode,AppError>{Err(AppError::BadRequest("Recoverable attachment copy requires supported platform".into()))}

#[cfg(test)]
pub(crate) async fn moved_asset_test_response(
    state: AppState,
    auth: AuthUser,
    path: String,
) -> Result<Response, AppError> {
    serve_attachment_file(State(state), auth, Path(path)).await
}

async fn serve_attachment_file(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(storage_path): Path<String>,
) -> Result<Response, AppError> {
    if !storage::validate_path(&storage_path) || !storage_path.starts_with("spaces/") {
        return Err(AppError::BadRequest("Invalid file path".into()));
    }

    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    // Old body URLs follow a stable asset ID, never historical space authority.
    let alias: Option<String> = sqlx::query_scalar("SELECT asset_id FROM move_asset_path_aliases WHERE old_path=$1 AND asset_kind='attachment'")
        .bind(&storage_path).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let attachment: Option<(String, String, String, String, String)> = if let Some(id) = &alias {
        sqlx::query_as(
            "SELECT id,document_id,space_id,mime_type,storage_path FROM attachments WHERE NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=attachments.id) AND id=$1",
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?
    } else {
        sqlx::query_as("SELECT id,document_id,space_id,mime_type,storage_path FROM attachments WHERE NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=attachments.id) AND storage_path=$1 LIMIT 1")
            .bind(&storage_path).fetch_optional(&mut *conn).await.map_err(AppError::internal)?
    };
    let (asset_id, document_id, space_id, mime_type, current_path) =
        attachment.ok_or_else(attachment_unavailable)?;
    let audience=super::asset_audiences::read(&mut conn,&auth,&space_id,&document_id,&asset_id).await?;
    let page=super::asset_audiences::asset_page(&mut conn,&auth,&space_id,&document_id,&asset_id).await.map_err(|e|match e {AppError::Forbidden(_)|AppError::NotFound(_)=>attachment_unavailable(),e=>e})?;
    let mut proof=super::published_source::capture_source(&mut conn,&auth,&space_id,&document_id,&page.ancestor_ids).await?;
    let alias_query="SELECT * FROM move_asset_path_aliases WHERE old_path=$1 AND asset_kind='attachment'";
    proof.push(super::published_source::SourceProof {query:alias_query.into(),parameters:vec![storage_path.clone()],rows:super::document_deletion::rows(&mut conn,alias_query,&[&storage_path]).await.map_err(AppError::internal)?});
    #[cfg(target_os="linux")]
    let version_binding={
        let (mut binding,absence)=crate::storage::attachment_versions::capture_current_asset(&storage::data_dir(),&mut conn,&asset_id).await.map_err(|_|attachment_unavailable())?;
        proof.push(absence);
        if let Some(b)=binding.as_mut(){if b.space_id!=space_id||b.document_id!=document_id||b.storage_path!=current_path||b.asset_id!=asset_id||!b.matches_requested_path(&storage_path){return Err(attachment_unavailable());}b.append_sql_proofs(&mut proof);}
        binding
    };
    let full_path =
        checked_attachment_path(&mut conn, &space_id, &document_id, &current_path).await?;
    let mut snapshot = download::capture(&full_path)
        .await
        .map_err(download_error)?;
    #[cfg(test)]
    super::shared_pages::reader_wait().await;
    #[cfg(test)]
    if let Some(hook) = BEFORE_FILE_RESPONSE.with(|slot| slot.borrow_mut().take()) {
        hook.await;
    }
    checked_attachment_path(&mut conn, &space_id, &document_id, &current_path).await?;
    let current:Option<i64>=sqlx::query_scalar("SELECT 1 FROM attachments WHERE NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=attachments.id) AND id=$1 AND storage_path=$2 AND document_id=$3 AND space_id=$4 AND mime_type=$5")
        .bind(&asset_id).bind(&current_path).bind(&document_id).bind(&space_id).bind(&mime_type).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let current_alias:Option<String>=sqlx::query_scalar("SELECT asset_id FROM move_asset_path_aliases WHERE old_path=$1 AND asset_kind='attachment'")
        .bind(&storage_path).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if current.is_none() || current_alias != alias {
        return Err(attachment_unavailable());
    }
    // Filesystem reads can wait; do not return bytes using stale credentials/audience.
    super::asset_audiences::asset_page(&mut conn,&auth,&space_id,&document_id,&asset_id).await?;

    #[cfg(test)]
    if let Some(hook) = AFTER_FILE_PAGE_PROOF.with(|slot| slot.borrow_mut().take()) {
        hook.await;
    }
    // The final typed page proof can await. Bind the emitted bytes to the exact
    // current asset and alias again after it; no unrelated wait follows.
    let final_asset:Option<i64>=sqlx::query_scalar("SELECT 1 FROM attachments a JOIN documents d ON d.id=a.document_id AND d.space_id=a.space_id WHERE NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=a.id) AND a.id=$1 AND a.storage_path=$2 AND a.document_id=$3 AND a.space_id=$4 AND a.mime_type=$5 AND d.deleted_at IS NULL AND d.archived_at IS NULL AND CASE WHEN EXISTS(SELECT 1 FROM move_asset_path_aliases WHERE old_path=$6 AND asset_kind='attachment') THEN (SELECT asset_id FROM move_asset_path_aliases WHERE old_path=$6 AND asset_kind='attachment')=$7 ELSE $7 IS NULL END")
        .bind(&asset_id).bind(&current_path).bind(&document_id).bind(&space_id).bind(&mime_type).bind(&storage_path).bind(alias.as_deref()).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if final_asset.is_none() {
        return Err(attachment_unavailable());
    }

    super::published_source::verify_source(&mut conn,&proof).await.map_err(|e|match e {AppError::Conflict(_)=>attachment_unavailable(),e=>e})?;
    if super::asset_audiences::asset_page(&mut conn,&auth,&space_id,&document_id,&asset_id).await?.direct_viewer_grant!=page.direct_viewer_grant {return Err(attachment_unavailable());}
    super::access::require_current_human_read(&mut conn,&auth).await?;
    if !snapshot.matches_source_and_rewind(&full_path).map_err(download_error)? {
        return Err(attachment_unavailable());
    }
    #[cfg(target_os="linux")]
    if let Some(binding)=&version_binding {
        binding.fence_files().map_err(|_|attachment_unavailable())?;
        if !snapshot.matches_sha256(binding.byte_length as u64,&binding.binary_sha256).map_err(download_error)? {return Err(attachment_unavailable());}
        binding.fence_files().map_err(|_|attachment_unavailable())?;
    }
    super::asset_audiences::verify_read(&mut conn,&auth,&space_id,&document_id,&asset_id,&audience).await?;
    #[cfg(target_os="linux")]
    verify_audience_snapshot(&snapshot,&audience)?;
    let length = snapshot.length();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .header(header::CONTENT_LENGTH, length)
        .body(snapshot.into_body())
        .map_err(AppError::internal)
}

#[cfg(target_os="linux")]
pub(crate) async fn serve_historical_version(State(state):State<AppState>,auth:AuthUser,Path((space,doc,logical,id)):Path<(String,String,String,String)>)->Result<Response,AppError>{
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    super::access::require_current_human_read(&mut conn,&auth).await?;
    let audience=super::asset_audiences::read(&mut conn,&auth,&space,&doc,&id).await?;
    let page=super::asset_audiences::asset_page(&mut conn,&auth,&space,&doc,&id).await.map_err(|e|match e{AppError::Forbidden(_)|AppError::NotFound(_)=>attachment_unavailable(),e=>e})?;
    let mut proofs=super::published_source::capture_source(&mut conn,&auth,&space,&doc,&page.ancestor_ids).await?;
    let (version,mut binding,family_proof)=storage::attachment_versions::capture_historical_version(&storage::data_dir(),&mut conn,&logical,&id).await.map_err(|_|attachment_unavailable())?;
    if binding.space_id!=space||binding.document_id!=doc||binding.asset_id!=id{return Err(attachment_unavailable())}
    proofs.push(family_proof);binding.append_sql_proofs(&mut proofs);
    let full=checked_attachment_path(&mut conn,&space,&doc,&binding.storage_path).await?;
    let mut snapshot=download::capture(&full).await.map_err(download_error)?;
    #[cfg(test)] storage::attachment_versions::history_download_wait().await;
    #[cfg(test)] super::shared_pages::reader_wait().await;
    checked_attachment_path(&mut conn,&space,&doc,&binding.storage_path).await?;
    super::asset_audiences::asset_page(&mut conn,&auth,&space,&doc,&id).await?;
    super::published_source::verify_source(&mut conn,&proofs).await.map_err(|e|match e{AppError::Conflict(_)=>attachment_unavailable(),e=>e})?;
    if super::asset_audiences::asset_page(&mut conn,&auth,&space,&doc,&id).await?.direct_viewer_grant!=page.direct_viewer_grant{return Err(attachment_unavailable())}
    super::access::require_current_human_read(&mut conn,&auth).await?;
    if !snapshot.matches_source_and_rewind(&full).map_err(download_error)?{return Err(attachment_unavailable())}
    binding.fence_files().map_err(|_|attachment_unavailable())?;
    if !snapshot.matches_sha256(binding.byte_length as u64,&binding.binary_sha256).map_err(download_error)?{return Err(attachment_unavailable())}
    binding.fence_files().map_err(|_|attachment_unavailable())?;
    super::asset_audiences::verify_read(&mut conn,&auth,&space,&doc,&id,&audience).await?;
    verify_audience_snapshot(&snapshot,&audience)?;
    Response::builder().status(StatusCode::OK).header(header::CONTENT_TYPE,version.mime_type).header(header::CONTENT_LENGTH,snapshot.length()).header(header::CACHE_CONTROL,"private, no-store").body(snapshot.into_body()).map_err(AppError::internal)
}

#[cfg(target_os="linux")]
fn verify_audience_snapshot(snapshot:&download::Snapshot,proof:&super::asset_audiences::ReadProof)->Result<(),AppError>{
    if let Some((length,hash))=proof.expected_binary(){
        if !snapshot.matches_sha256(length,hash).map_err(download_error)? {return Err(attachment_unavailable())}
    }
    Ok(())
}

fn download_error(error: download::DownloadError) -> AppError {
    match error {
        download::DownloadError::Source(error) => {
            tracing::debug!(kind = ?error.kind(), "Attachment download source unavailable");
            attachment_unavailable()
        }
        download::DownloadError::Snapshot(error) => {
            tracing::warn!(kind = ?error.kind(), "Attachment download snapshot I/O failed");
            AppError::Internal(
                "Attachment download snapshot could not be prepared; check temporary storage availability".into(),
            )
        }
    }
}

#[derive(Serialize)]
pub struct AttachmentResponse {
    #[serde(skip_serializing_if="Option::is_none")]
    pub audience: Option<super::asset_audiences::AudienceAcknowledgement>,
    #[serde(skip_serializing_if="Option::is_none")]
    pub logical_attachment_id: Option<String>,
    pub document_id: String,
    pub id: String,
    pub filename: String,
    pub original_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub url: String,
    pub created_at: String,
}

async fn list_attachments(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let page=super::asset_audiences::collection_access(&mut conn,&auth,&space_id,&doc_id).await?;
    let proof=if page.direct_viewer_grant.is_some() {Some(super::published_source::capture_source(&mut conn,&auth,&space_id,&doc_id,&page.ancestor_ids).await?)} else {None};
    let attachments: Vec<(String, String, String, String, i64, String, String)> = sqlx::query_as(
        "SELECT id, filename, original_name, mime_type, size_bytes, storage_path, created_at \
         FROM attachments WHERE NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=attachments.id) AND document_id = $1 AND space_id = $2 ORDER BY created_at",
    )
    .bind(&doc_id)
    .bind(&space_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<AttachmentResponse> = attachments
        .into_iter()
        .map(
            |(id, filename, original, mime, size, path, created)| AttachmentResponse {
                audience:None,
                logical_attachment_id: None,
                document_id: doc_id.clone(),
                url: format!("/api/v1/files/{}", path),
                id,
                filename,
                original_name: original,
                mime_type: mime,
                size_bytes: size,
                created_at: created,
            },
        )
        .collect();

    super::asset_audiences::collection_access(&mut conn,&auth,&space_id,&doc_id).await?;
    #[cfg(test)]
    apply_test_list_change(&mut conn).await?;
    #[cfg(test)]
    super::shared_pages::reader_wait().await;
    let collection_proof=super::asset_audiences::capture_collection(&mut conn,&auth,&space_id).await?;
    let mut current = Vec::new();
    for mut attachment in result {
        attachment.logical_attachment_id=attachment_family_id(&mut conn,&attachment.id).await?;
        if attachment_response_current_for_page(&mut conn, &auth, &space_id, &attachment).await? {
            current.push(attachment);
        }
    }
    super::asset_audiences::collection_access(&mut conn,&auth,&space_id,&doc_id).await?;
    if let Some(proof)=proof.as_ref() {super::published_source::verify_source(&mut conn,proof).await?;}
    super::asset_audiences::collection_access(&mut conn,&auth,&space_id,&doc_id).await?;
    if super::asset_audiences::collection_access(&mut conn,&auth,&space_id,&doc_id).await?.direct_viewer_grant!=page.direct_viewer_grant {return Err(attachment_unavailable());}
    super::access::require_current_human_read(&mut conn,&auth).await?;
    let mut allowed=Vec::new();
    for item in current {if super::asset_audiences::visible(&mut conn,&auth,&space_id,&item.document_id,&item.id).await? {allowed.push(item)}}
    super::asset_audiences::verify_collection(&mut conn,&auth,&collection_proof).await?;
    Ok(Json(allowed))
}

async fn list_space_attachments(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    check_attachment_browser(&mut conn, &auth, &space_id).await?;
    let attachments: Vec<(String, String, String, String, String, i64, String, String)> =
        sqlx::query_as(
            "SELECT id, document_id, filename, original_name, mime_type, size_bytes, storage_path, created_at \
             FROM attachments WHERE NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=attachments.id) AND space_id = $1 ORDER BY created_at DESC",
        )
        .bind(&space_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::internal)?;

    let mut visible = Vec::new();
    for attachment in attachments {
        if attachment_page_visible(&mut conn, &auth, &space_id, &attachment.1).await? {
            visible.push(attachment);
        }
    }
    check_attachment_browser(&mut conn, &auth, &space_id).await?;
    let result: Vec<AttachmentResponse> = visible
        .into_iter()
        .map(
            |(id, document_id, filename, original, mime, size, path, created)| AttachmentResponse {
                audience:None,
                logical_attachment_id: None,
                document_id,
                id,
                filename,
                original_name: original,
                mime_type: mime,
                size_bytes: size,
                url: format!("/api/v1/files/{}", path),
                created_at: created,
            },
        )
        .collect();

    #[cfg(test)]
    apply_test_list_change(&mut conn).await?;
    let collection_proof=super::asset_audiences::capture_collection(&mut conn,&auth,&space_id).await?;
    let mut current = Vec::new();
    for mut attachment in result {
        attachment.logical_attachment_id=attachment_family_id(&mut conn,&attachment.id).await?;
        if attachment_response_current(&mut conn, &auth, &space_id, &attachment).await? {
            current.push(attachment);
        }
    }
    check_attachment_browser(&mut conn, &auth, &space_id).await?;
    let mut allowed=Vec::new();
    for item in current {if super::asset_audiences::visible(&mut conn,&auth,&space_id,&item.document_id,&item.id).await? {allowed.push(item)}}
    super::asset_audiences::verify_collection(&mut conn,&auth,&collection_proof).await?;
    Ok(Json(allowed))
}

#[cfg(not(target_os="linux"))]
async fn upload_attachment(state:State<AppState>,auth:AuthUser,path:Path<(String,String)>,headers:axum::http::HeaderMap,multipart:Multipart)->Result<impl IntoResponse,AppError>{
    if headers.contains_key("x-starlane-upload-operation"){return Err(AppError::BadRequest("Recoverable upload identity requires supported platform".into()));}
    upload_attachment_legacy(state,auth,path,multipart).await
}
#[cfg(target_os="linux")]
async fn upload_attachment(State(state):State<AppState>,auth:AuthUser,Path((space_id,doc_id)):Path<(String,String)>,headers:axum::http::HeaderMap,mut multipart:UploadMultipart)->Result<(StatusCode,Json<AttachmentResponse>),AppError>{
    use futures::TryStreamExt;
    let operation_id=match headers.get("x-starlane-upload-operation") {Some(v)=>{let s=v.to_str().map_err(|_|AppError::BadRequest("Invalid upload operation".into()))?;let id=Uuid::parse_str(s).map_err(|_|AppError::BadRequest("Invalid upload operation".into()))?;if id.to_string()!=s{return Err(AppError::BadRequest("Invalid upload operation".into()));}s.to_owned()},None=>Uuid::new_v4().to_string()};
    let scope={let mut conn=state.db.acquire().await.map_err(AppError::internal)?;super::asset_audiences::capture_upload_scope(&mut conn,&auth,&space_id,&doc_id,&headers).await?};
    let maximum=multipart.maximum;
    let field=multipart.next_field().await.map_err(|_|AppError::BadRequest("Upload body unavailable".into()))?.ok_or_else(||AppError::BadRequest("No file in upload".into()))?;
    let name=validate_attachment_upload_field(field.name(),field.file_name())?.to_owned();let mime=field.content_type().unwrap_or("application/octet-stream").to_owned();
    let request=storage::attachment_upload::UploadRequest{operation_id,origin:state.base_url.clone(),actor_id:auth.claims.sub.clone(),space_id,document_id:doc_id,original_name:name,mime_type:mime,draft_scope:scope};
    let stream=futures::stream::try_unfold(field,|mut field|async move {match field.chunk().await.map_err(|_|AppError::BadRequest("Upload body disconnected; private stage retained".into()))? {Some(chunk)=>Ok(Some((chunk,field))),None=>Ok(None)}}).map_err(|e:AppError|e);
    upload_content(state,auth,headers,request,maximum,Box::pin(stream),None).await
}
#[cfg(target_os="linux")]
async fn completed_upload_ack(state:&AppState,auth:&AuthUser,prior:storage::attachment_upload::Receipt)->Result<(StatusCode,Json<AttachmentResponse>),AppError>{
    use crate::storage::attachment_upload as custody;
    let root=storage::data_dir();let request=&prior.intent.request;let space_id=request.space_id.clone();let doc_id=request.document_id.clone();let draft_scope=request.draft_scope.clone();
        let mut tx=begin_attachment_mutation_scoped(&state,&auth,&space_id,&doc_id,draft_scope.is_some()).await?;
        if custody::receipt(&mut tx,&request.operation_id).await.map_err(AppError::Internal)?.as_ref()!=Some(&prior)||!custody::association(&mut tx,&prior).await.map_err(AppError::Internal)?{return Err(attachment_unavailable());}
        let registry=crate::storage::attachment_versions::prepare_singleton(&root,&mut tx,&prior).await.map_err(AppError::Conflict)?;
        let publication=crate::storage::attachment_versions::publish_singleton(&mut tx,&registry,false).await.map_err(AppError::Conflict)?;
        super::asset_audiences::verify_upload_scope(&mut tx,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;custody::verify(&root,&prior,false).map_err(upload_custody_error)?;
        tx.rollback().await.map_err(AppError::internal)?;
        let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
        super::asset_audiences::verify_upload_scope(&mut conn,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
        if custody::receipt(&mut conn,&request.operation_id).await.map_err(AppError::Internal)?.as_ref()!=Some(&prior)||!custody::association(&mut conn,&prior).await.map_err(AppError::Internal)?{return Err(attachment_unavailable());}
        crate::storage::attachment_versions::verify_singleton_publication(&mut conn,&registry,&publication).await.map_err(AppError::Conflict)?;
        super::asset_audiences::verify_upload_scope(&mut conn,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;custody::verify(&root,&prior,false).map_err(upload_custody_error)?;
        super::asset_audiences::admit_upload(&mut conn,&prior,false).await?;
        super::asset_audiences::verify_upload_scope(&mut conn,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
        custody::verify(&root,&prior,false).map_err(upload_custody_error)?;
        let i=prior.intent;Ok((StatusCode::CREATED,Json(AttachmentResponse{audience:super::asset_audiences::acknowledgement(i.request.draft_scope.as_ref()),logical_attachment_id:Some(i.asset_id.clone()),document_id:doc_id,id:i.asset_id,filename:i.filename,original_name:i.request.original_name,mime_type:i.request.mime_type,size_bytes:i.byte_length,url:format!("/api/v1/files/{}",i.storage_path),created_at:i.created_at})))
}
#[cfg(target_os="linux")]
async fn upload_content(state:AppState,auth:AuthUser,headers:axum::http::HeaderMap,request:storage::attachment_upload::UploadRequest,maximum:usize,mut content:std::pin::Pin<Box<dyn futures::Stream<Item=Result<axum::body::Bytes,AppError>>+Send+'_>>,copy:Option<&copy::CopyGuard>)->Result<(StatusCode,Json<AttachmentResponse>),AppError>{
    use futures::StreamExt;
    use crate::storage::attachment_upload as custody;
    let root=storage::data_dir();let space_id=request.space_id.clone();let doc_id=request.document_id.clone();let name=request.original_name.clone();let draft_scope=request.draft_scope.clone();
    let completed={let mut conn=state.db.acquire().await.map_err(AppError::internal)?;super::asset_audiences::verify_upload_scope(&mut conn,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;custody::receipt(&mut conn,&request.operation_id).await.map_err(AppError::Internal)?};
    if let Some(prior)=completed {
        if prior.intent.request!=request{return Err(AppError::Conflict("Completed upload original request differs".into()));}
        use sha2::{Digest,Sha256};let mut hash=Sha256::new();let mut length=0u64;
        while let Some(chunk)=content.next().await {let chunk=chunk?;length=length.checked_add(chunk.len() as u64).ok_or_else(||AppError::BadRequest("Upload size limit exceeded".into()))?;if length>maximum as u64{return Err(AppError::BadRequest("Upload size limit exceeded".into()));}for bytes in chunk.chunks(custody::CHUNK){hash.update(bytes);}}
        if length!=prior.intent.byte_length as u64||hex::encode(hash.finalize())!=prior.intent.binary_sha256{return Err(AppError::Conflict("Completed upload original bytes differ".into()));}
        return completed_upload_ack(&state,&auth,prior).await;
    }
    let limit=crate::storage::attachment_capacity::configured_limit().map_err(AppError::internal)?;
    let r=root.clone();let producer=custody::admission::Producer::Direct{request:request.clone()};
    let lease=tokio::task::spawn_blocking(move||custody::admission::WriterLease::begin(&r,&Uuid::new_v4().to_string(),producer,maximum as u64)).await.map_err(AppError::internal)?.map_err(upload_custody_error)?;
    crate::storage::attachment_capacity::reserve_authorized(&state.db,&lease,limit,&auth).await.map_err(|e|AppError::Conflict(e.to_string()))?;
    let r=root.clone();let stage_lease=lease.clone();let mut ingest=tokio::task::spawn_blocking(move||custody::Ingest::reserve_with_lease(&r,request,maximum as u64,stage_lease)).await.map_err(AppError::internal)?.map_err(upload_custody_error)?;
    while let Some(chunk)=content.next().await {
        let chunk=chunk?;
        for bytes in chunk.chunks(custody::CHUNK){let bytes=bytes.to_vec();ingest=tokio::task::spawn_blocking(move||{ingest.append(&bytes)?;Ok::<_,std::io::Error>(ingest)}).await.map_err(AppError::internal)?.map_err(upload_custody_error)?;}
    }
    drop(content);
    let captured=tokio::task::spawn_blocking(move||ingest.finish()).await.map_err(AppError::internal)?.map_err(upload_custody_error)?;
    if let Some(source)=copy {let mut conn=state.db.acquire().await.map_err(AppError::internal)?;source.verify(&mut conn,&auth).await?;}
    let mut tx=begin_attachment_mutation_scoped(&state,&auth,&space_id,&doc_id,draft_scope.is_some()).await?;
    if let Some(source)=copy {source.verify(&mut tx,&auth).await?;}
    let prior=custody::receipt(&mut tx,headers.get("x-starlane-upload-operation").and_then(|v|v.to_str().ok()).unwrap_or("" )).await.map_err(|e|AppError::Internal(e))?;
    let fresh_upload = prior.is_none();
    let receipt=if let Some(prior)=prior {
        if !captured.matches(&prior)||!custody::association(&mut tx,&prior).await.map_err(AppError::Internal)?{return Err(AppError::Conflict("Upload identity or current association differs".into()));}
        drop(captured);
        let r=root.clone();let p=prior.clone();tokio::task::spawn_blocking(move||custody::verify(&r,&p,false)).await.map_err(AppError::internal)?.map_err(upload_custody_error)?;prior
    }else{
        let slug:String=sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1").bind(&space_id).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
        if !safe_component(&slug)||!safe_component(&doc_id){return Err(attachment_unavailable());}let ext=safe_attachment_extension(&name).to_owned();
        let prepared=tokio::task::spawn_blocking(move||captured.prepare(&slug,&ext)).await.map_err(AppError::internal)?.map_err(upload_custody_error)?;
        let installed=tokio::task::spawn_blocking(move||prepared.install()).await.map_err(AppError::internal)?.map_err(upload_custody_error)?;
        super::asset_audiences::verify_upload_scope(&mut tx,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
        if let Some(source)=copy {source.verify(&mut tx,&auth).await?;source.matches_receipt(&installed)?;}
        let i=&installed.intent;let q=&i.request;
        sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(&i.asset_id).bind(&doc_id).bind(&space_id).bind(&i.filename).bind(&q.original_name).bind(&q.mime_type).bind(i.byte_length).bind(&i.storage_path).bind(&auth.claims.sub).bind(&i.created_at).execute(&mut *tx).await.map_err(AppError::internal)?;
        custody::insert(&mut tx,&installed).await.map_err(AppError::Internal)?;installed
    };
    if !custody::association(&mut tx,&receipt).await.map_err(AppError::Internal)?{return Err(attachment_unavailable());}
    super::asset_audiences::admit_upload(&mut tx,&receipt,fresh_upload).await?;
    let registry = crate::storage::attachment_versions::prepare_singleton(&root, &mut tx, &receipt).await.map_err(AppError::Conflict)?;
    let registry_admission = crate::storage::attachment_versions::publish_singleton(&mut tx, &registry, fresh_upload).await.map_err(AppError::Conflict)?;
    super::asset_audiences::verify_upload_scope(&mut tx,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
    if receipt.intent.attempt_id==lease.attempt_id(){crate::storage::attachment_capacity::commit_direct_in(&mut tx,&lease,&receipt).await.map_err(AppError::internal)?;}
    super::asset_audiences::verify_upload_scope(&mut tx,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
    if let Some(source)=copy {source.verify(&mut tx,&auth).await?;}
    // Exact final synchronous filesystem fence follows the last policy await; no unrelated await before commit.
    custody::verify(&root,&receipt,false).map_err(upload_custody_error)?;
    if let Some(source)=copy {source.fence()?;}
    #[cfg(test)] UPLOAD_BEFORE_COMMIT.with(|s|{if let Some(f)=s.borrow_mut().take(){f(&receipt);}});
    if let Err(_e)=tx.commit().await {
        // Fresh full receipt decision, never scalar presence or fabricated response.
        let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
        let exact=custody::receipt(&mut conn,&receipt.intent.request.operation_id).await.map_err(AppError::Internal)?;
        if exact.as_ref()!=Some(&receipt)||!custody::association(&mut conn,&receipt).await.map_err(AppError::Internal)?{return Err(AppError::Internal("Upload outcome unknown; retain operation identity and bytes".into()));}
    }
    let losing_attempt=(receipt.intent.attempt_id!=lease.attempt_id()).then(||lease.attempt_id().to_owned());
    drop(lease);
    if let Some(attempt)=losing_attempt{custody::admission::dispose_direct(&state.db,&root,&attempt).await.map_err(AppError::internal)?;}
    #[cfg(test)] UPLOAD_AFTER_COMMIT.with(|s|{if let Some(f)=s.borrow_mut().take(){f(&receipt);}});
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    super::asset_audiences::verify_upload_scope(&mut conn,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
    if custody::receipt(&mut conn,&receipt.intent.request.operation_id).await.map_err(AppError::Internal)?.as_ref()!=Some(&receipt)||!custody::association(&mut conn,&receipt).await.map_err(AppError::Internal)?{return Err(attachment_unavailable());}
    super::asset_audiences::verify_upload_scope(&mut conn,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
    crate::storage::attachment_versions::verify_singleton_publication(&mut conn, &registry, &registry_admission).await.map_err(AppError::Conflict)?;
    super::asset_audiences::verify_upload_scope(&mut conn,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
    custody::verify(&root,&receipt,false).map_err(upload_custody_error)?;
    super::asset_audiences::admit_upload(&mut conn,&receipt,false).await?;
    super::asset_audiences::verify_upload_scope(&mut conn,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
    custody::verify(&root,&receipt,false).map_err(upload_custody_error)?;
    if let Some(source)=copy {source.verify(&mut conn,&auth).await?;source.fence()?;}
    let i=receipt.intent;Ok((StatusCode::CREATED,Json(AttachmentResponse{audience:super::asset_audiences::acknowledgement(i.request.draft_scope.as_ref()),logical_attachment_id:Some(i.asset_id.clone()),document_id:doc_id,id:i.asset_id,filename:i.filename,original_name:i.request.original_name,mime_type:i.request.mime_type,size_bytes:i.byte_length,url:format!("/api/v1/files/{}",i.storage_path),created_at:i.created_at})))
}
#[cfg(target_os="linux")]
pub(crate) async fn upload_attachment_replacement(State(state):State<AppState>,auth:AuthUser,Path((space_id,doc_id,family_id)):Path<(String,String,String)>,headers:axum::http::HeaderMap,mut multipart:UploadMultipart)->Result<impl IntoResponse,AppError>{
    use crate::storage::attachment_upload as custody;
    let operation_id=match headers.get("x-starlane-upload-operation") {Some(v)=>{let s=v.to_str().map_err(|_|AppError::BadRequest("Invalid upload operation".into()))?;let id=Uuid::parse_str(s).map_err(|_|AppError::BadRequest("Invalid upload operation".into()))?;if id.to_string()!=s{return Err(AppError::BadRequest("Invalid upload operation".into()));}s.to_owned()},None=>return Err(AppError::BadRequest("Replacement requires caller-held operation identity".into()))};
    let expected_version=headers.get("x-starlane-attachment-version").and_then(|v|v.to_str().ok()).ok_or_else(||AppError::BadRequest("Expected version required".into()))?.to_owned();
    if Uuid::parse_str(&expected_version).map(|v|v.to_string()).ok().as_deref()!=Some(expected_version.as_str()){return Err(AppError::BadRequest("Expected version invalid".into()));}
    let expected_revision:i64=headers.get("x-starlane-attachment-revision").and_then(|v|v.to_str().ok()).and_then(|v|v.parse().ok()).filter(|v|*v>0).ok_or_else(||AppError::BadRequest("Expected revision required".into()))?;
    let root=storage::data_dir();
    let draft_scope={let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
        super::asset_audiences::capture_upload_scope(&mut conn,&auth,&space_id,&doc_id,&headers).await?};
    let (retry,access_proofs)={let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
        super::asset_audiences::verify_upload_scope(&mut conn,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
        super::asset_audiences::require_change(&mut conn,&auth,&space_id,&doc_id,&expected_version,draft_scope.as_ref()).await?;
        let retry=crate::storage::attachment_versions::capture_completed_replacement_retry(&root,&mut conn,&family_id,&expected_version,expected_revision,&operation_id,&auth.claims.sub,&state.base_url,&space_id,&doc_id).await.map_err(AppError::Conflict)?;
        let mut access_proofs=Vec::new();
        if retry.is_some(){let view=super::shared_pages::asset_view(&mut conn,&auth,&space_id,&doc_id).await?;access_proofs=super::published_source::capture_source(&mut conn,&auth,&space_id,&doc_id,&view.ancestor_ids).await?;}
        else {crate::storage::attachment_versions::require_replacement_target(&root,&mut conn,&family_id,&expected_version,expected_revision,&operation_id,&auth.claims.sub,&state.base_url,&space_id,&doc_id).await.map_err(AppError::Conflict)?;}
        (retry,access_proofs)};
    let maximum=multipart.maximum;
    let mut field=multipart.next_field().await.map_err(|_|AppError::BadRequest("Upload body unavailable".into()))?.ok_or_else(||AppError::BadRequest("No file in upload".into()))?;
    let name=validate_attachment_upload_field(field.name(),field.file_name())?.to_owned();let mime=field.content_type().unwrap_or("application/octet-stream").to_owned();
    if let Some(retry)=retry {
        if retry.draft_scope()!=draft_scope.as_ref(){return Err(AppError::Conflict("Replacement original draft differs".into()))}
        use sha2::{Digest,Sha256};let mut hash=Sha256::new();let mut length=0u64;
        while let Some(chunk)=field.chunk().await.map_err(|_|AppError::BadRequest("Upload retry body disconnected".into()))? {
            length=length.checked_add(chunk.len() as u64).ok_or_else(||AppError::BadRequest("Upload size limit exceeded".into()))?;
            if length>maximum as u64{return Err(AppError::BadRequest("Upload size limit exceeded".into()))}
            for bytes in chunk.chunks(custody::CHUNK){hash.update(bytes);}
        }
        drop(field);retry.validate_body(&name,&mime,length,&hex::encode(hash.finalize())).map_err(AppError::Conflict)?;
        let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
        super::asset_audiences::verify_upload_scope(&mut conn,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
        retry.verify_sql(&mut conn).await.map_err(AppError::Conflict)?;
        super::published_source::verify_source(&mut conn,&access_proofs).await?;
        super::asset_audiences::verify_upload_scope(&mut conn,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
        let audience=super::asset_audiences::acknowledgement(retry.draft_scope());
        let mut response=serde_json::to_value(retry.finish().map_err(AppError::Conflict)?).map_err(AppError::internal)?;
        if let Some(audience)=audience {response["audience"]=serde_json::to_value(audience).map_err(AppError::internal)?;}
        return Ok((StatusCode::CREATED,Json(response)));
    }
    let request=custody::UploadRequest{operation_id,origin:state.base_url.clone(),actor_id:auth.claims.sub.clone(),space_id:space_id.clone(),document_id:doc_id.clone(),original_name:name.clone(),mime_type:mime,draft_scope:draft_scope.clone()};
    let limit=crate::storage::attachment_capacity::configured_limit().map_err(AppError::internal)?;
    let r=root.clone();let producer=custody::admission::Producer::Direct{request:request.clone()};
    let lease=tokio::task::spawn_blocking(move||custody::admission::WriterLease::begin(&r,&Uuid::new_v4().to_string(),producer,maximum as u64)).await.map_err(AppError::internal)?.map_err(upload_custody_error)?;
    crate::storage::attachment_capacity::reserve_authorized(&state.db,&lease,limit,&auth).await.map_err(|e|AppError::Conflict(e.to_string()))?;
    let r=root.clone();let stage_lease=lease.clone();let mut ingest=tokio::task::spawn_blocking(move||custody::Ingest::reserve_with_lease(&r,request,maximum as u64,stage_lease)).await.map_err(AppError::internal)?.map_err(upload_custody_error)?;
    while let Some(chunk)=field.chunk().await.map_err(|_|AppError::BadRequest("Upload body disconnected; private stage retained".into()))? {
        for bytes in chunk.chunks(custody::CHUNK){let bytes=bytes.to_vec();ingest=tokio::task::spawn_blocking(move||{ingest.append(&bytes)?;Ok::<_,std::io::Error>(ingest)}).await.map_err(AppError::internal)?.map_err(upload_custody_error)?;}
    }
    drop(field);
    let captured=tokio::task::spawn_blocking(move||ingest.finish()).await.map_err(AppError::internal)?.map_err(upload_custody_error)?;
    let mut tx=begin_attachment_mutation_scoped(&state,&auth,&space_id,&doc_id,draft_scope.is_some()).await?;
    let prior=custody::receipt(&mut tx,headers.get("x-starlane-upload-operation").and_then(|v|v.to_str().ok()).unwrap_or("" )).await.map_err(|e|AppError::Internal(e))?;
    let fresh_upload = prior.is_none();
    let (receipt,change)=if let Some(prior)=prior {
        if !captured.matches(&prior)||!custody::association(&mut tx,&prior).await.map_err(AppError::Internal)?{return Err(AppError::Conflict("Upload identity or current association differs".into()));}
        drop(captured);
        let r=root.clone();let p=prior.clone();tokio::task::spawn_blocking(move||custody::verify(&r,&p,false)).await.map_err(AppError::internal)?.map_err(upload_custody_error)?;let change=crate::storage::attachment_versions::resolve_replacement(&root,&mut tx,&prior,&family_id,&expected_version,expected_revision).await.map_err(AppError::Conflict)?.ok_or_else(||AppError::Conflict("Existing upload has no replacement authority".into()))?;(prior,change)
    }else{
        let slug:String=sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1").bind(&space_id).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
        if !safe_component(&slug)||!safe_component(&doc_id){return Err(attachment_unavailable());}let ext=safe_attachment_extension(&name).to_owned();
        let prepared=tokio::task::spawn_blocking(move||captured.prepare(&slug,&ext)).await.map_err(AppError::internal)?.map_err(upload_custody_error)?;
        let installed=tokio::task::spawn_blocking(move||prepared.install()).await.map_err(AppError::internal)?.map_err(upload_custody_error)?;
        super::asset_audiences::verify_upload_scope(&mut tx,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
        let i=&installed.intent;let q=&i.request;
        sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)").bind(&i.asset_id).bind(&doc_id).bind(&space_id).bind(&i.filename).bind(&q.original_name).bind(&q.mime_type).bind(i.byte_length).bind(&i.storage_path).bind(&auth.claims.sub).bind(&i.created_at).execute(&mut *tx).await.map_err(AppError::internal)?;
        custody::insert(&mut tx,&installed).await.map_err(AppError::Internal)?;
        let prepared=crate::storage::attachment_versions::prepare_replacement(&root,&mut tx,&installed,&family_id,&expected_version,expected_revision).await.map_err(AppError::Conflict)?;
        crate::storage::attachment_versions::publish_replacement(&mut tx,&prepared).await.map_err(AppError::Conflict)?;let change=prepared.result().clone();(installed,change)
    };
    if !custody::association(&mut tx,&receipt).await.map_err(AppError::Internal)?{return Err(attachment_unavailable());}
    super::asset_audiences::admit_upload(&mut tx,&receipt,fresh_upload).await?;
    let mut prior_binding=crate::storage::attachment_versions::replacement_prior_binding(&root,&mut tx,&change).await.map_err(AppError::Conflict)?;
    let mut prior_proofs=Vec::new();prior_binding.append_sql_proofs(&mut prior_proofs);
    super::published_source::verify_source(&mut tx,&prior_proofs).await?;
    super::asset_audiences::verify_upload_scope(&mut tx,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
    if receipt.intent.attempt_id==lease.attempt_id(){crate::storage::attachment_capacity::commit_direct_in(&mut tx,&lease,&receipt).await.map_err(AppError::internal)?;}
    super::asset_audiences::verify_upload_scope(&mut tx,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
    // Exact final synchronous filesystem fence follows the last policy await; no unrelated await before commit.
    custody::verify(&root,&receipt,false).map_err(upload_custody_error)?;
    prior_binding.fence_files().map_err(AppError::Conflict)?;
    #[cfg(test)] UPLOAD_BEFORE_COMMIT.with(|s|{if let Some(f)=s.borrow_mut().take(){f(&receipt);}});
    if let Err(_e)=tx.commit().await {
        // Fresh full receipt decision, never scalar presence or fabricated response.
        let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
        let exact=custody::receipt(&mut conn,&receipt.intent.request.operation_id).await.map_err(AppError::Internal)?;
        if exact.as_ref()!=Some(&receipt)||!custody::association(&mut conn,&receipt).await.map_err(AppError::Internal)?{return Err(AppError::Internal("Upload outcome unknown; retain operation identity and bytes".into()));}
    }
    let losing_attempt=(receipt.intent.attempt_id!=lease.attempt_id()).then(||lease.attempt_id().to_owned());
    drop(lease);
    if let Some(attempt)=losing_attempt{custody::admission::dispose_direct(&state.db,&root,&attempt).await.map_err(AppError::internal)?;}
    #[cfg(test)] REPLACEMENT_AFTER_COMMIT.with(|s|{if let Some(f)=s.borrow_mut().take(){f(&receipt,&change);}});
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    super::asset_audiences::verify_upload_scope(&mut conn,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
    if custody::receipt(&mut conn,&receipt.intent.request.operation_id).await.map_err(AppError::Internal)?.as_ref()!=Some(&receipt)||!custody::association(&mut conn,&receipt).await.map_err(AppError::Internal)?{return Err(attachment_unavailable());}
    super::asset_audiences::verify_upload_scope(&mut conn,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
    if crate::storage::attachment_versions::resolve_replacement(&root,&mut conn,&receipt,&family_id,&expected_version,expected_revision).await.map_err(AppError::Conflict)?.as_ref()!=Some(&change){return Err(AppError::Conflict("Replacement decision differs".into()));}
    let mut prior_binding=crate::storage::attachment_versions::replacement_prior_binding(&root,&mut conn,&change).await.map_err(AppError::Conflict)?;
    let mut prior_proofs=Vec::new();prior_binding.append_sql_proofs(&mut prior_proofs);
    super::published_source::verify_source(&mut conn,&prior_proofs).await?;
    super::asset_audiences::verify_upload_scope(&mut conn,&auth,&space_id,&doc_id,draft_scope.as_ref()).await?;
    custody::verify(&root,&receipt,false).map_err(upload_custody_error)?;
    prior_binding.fence_files().map_err(AppError::Conflict)?;
    super::asset_audiences::admit_upload(&mut conn,&receipt,false).await?;
    let mut response=serde_json::to_value(change).map_err(AppError::internal)?;
    if let Some(audience)=super::asset_audiences::acknowledgement(receipt.intent.request.draft_scope.as_ref()) {response["audience"]=serde_json::to_value(audience).map_err(AppError::internal)?;}
    Ok((StatusCode::CREATED,Json(response)))
}
#[cfg(target_os="linux")]
fn upload_custody_error(e:std::io::Error)->AppError{tracing::warn!(kind=?e.kind(),"Private upload evidence retained");if e.kind()==std::io::ErrorKind::InvalidData&&e.to_string().contains("limit"){AppError::BadRequest("Upload size limit exceeded".into())}else{AppError::Internal("Upload custody failed; private evidence retained, check storage and retain operation identity".into())}}
#[cfg(all(test,target_os="linux"))]
thread_local!{
static UPLOAD_BEFORE_COMMIT:std::cell::RefCell<Option<Box<dyn FnOnce(&crate::storage::attachment_upload::Receipt)>>> = std::cell::RefCell::new(None);
static UPLOAD_AFTER_COMMIT:std::cell::RefCell<Option<Box<dyn FnOnce(&crate::storage::attachment_upload::Receipt)>>> = std::cell::RefCell::new(None);
}

async fn upload_attachment_legacy(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    super::access::principal_from_auth(&auth)?;
    let max_size = max_upload_size_bytes(&state).await?;

    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Upload error: {}", e)))?
    {
        let original_name =
            validate_attachment_upload_field(field.name(), field.file_name())?.to_string();
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        let mut bytes = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to read file: {}", e)))?
        {
            if bytes.len().saturating_add(chunk.len()) > max_size {
                return Err(AppError::BadRequest(format!(
                    "File too large. Maximum size is {} MB",
                    max_size / 1024 / 1024
                )));
            }
            bytes.extend_from_slice(&chunk);
        }

        let attachment = store_attachment(
            &state,
            &auth,
            &space_id,
            &doc_id,
            &original_name,
            &content_type,
            &bytes,
        )
        .await?;
        return Ok((StatusCode::CREATED, Json(attachment)));
    }

    Err(AppError::BadRequest("No file in upload".into()))
}

pub(crate) async fn max_upload_size_bytes(state: &AppState) -> Result<usize, AppError> {
    if let Ok(value) = std::env::var("MAX_UPLOAD_SIZE_MB") {
        if let Some(size) = parse_upload_size_mb(&value) {
            return Ok(size);
        }
    }

    let stored: Option<(String,)> =
        sqlx::query_as("SELECT value FROM system_config WHERE key = $1")
            .bind("max_upload_size_mb")
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    Ok(stored
        .and_then(|(value,)| parse_upload_size_mb(&value))
        .unwrap_or(100 * 1024 * 1024))
}

fn parse_upload_size_mb(value: &str) -> Option<usize> {
    let mb = value.trim().parse::<usize>().ok()?;
    if mb == 0 {
        return None;
    }
    mb.checked_mul(1024)?.checked_mul(1024)
}

fn safe_attachment_extension(original_name: &str) -> &str {
    let Some((_, extension)) = original_name.rsplit_once('.') else {
        return "bin";
    };
    if extension.is_empty()
        || extension.len() > 16
        || !extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return "bin";
    }
    extension
}

fn validate_attachment_upload_field<'a>(
    field_name: Option<&str>,
    file_name: Option<&'a str>,
) -> Result<&'a str, AppError> {
    if field_name != Some("file") {
        return Err(AppError::BadRequest(
            "Upload must include a file field".into(),
        ));
    }
    let file_name = file_name
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| AppError::BadRequest("Uploaded file must have a filename".into()))?;
    if file_name.len() > 255 {
        return Err(AppError::BadRequest(
            "Uploaded filename must be 255 bytes or fewer".into(),
        ));
    }
    Ok(file_name)
}

async fn delete_attachment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id, attachment_id)): Path<(String, String, String)>,
    headers: axum::http::HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let mut tx = begin_attachment_mutation_scoped(&state, &auth, &space_id, &doc_id,headers.contains_key("x-starlane-upload-draft")).await?;
    #[cfg(target_os="linux")]
    let scope=super::asset_audiences::capture_upload_scope(&mut tx,&auth,&space_id,&doc_id,&headers).await?;
    #[cfg(target_os="linux")]
    super::asset_audiences::require_change(&mut tx,&auth,&space_id,&doc_id,&attachment_id,scope.as_ref()).await?;
    // Lock and re-read the actual scoped association before deleting it.
    let locked =
        sqlx::query("UPDATE attachments SET id=id WHERE id=$1 AND document_id=$2 AND space_id=$3 AND NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=attachments.id)")
            .bind(&attachment_id)
            .bind(&doc_id)
            .bind(&space_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    if locked.rows_affected() != 1 {
        return Err(attachment_unavailable());
    }
    check_attachment_page(
        &mut tx,
        &auth,
        &space_id,
        &doc_id,
        super::access::PageOperation::Edit,
    )
    .await?;
    let original: storage::attachment_retirement::Association = sqlx::query_as("SELECT filename,original_name,mime_type,size_bytes,storage_path,created_by,created_at FROM attachments WHERE NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=attachments.id) AND id=$1 AND document_id=$2 AND space_id=$3")
        .bind(&attachment_id).bind(&doc_id).bind(&space_id).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    let retirement = storage::attachment_retirement::retire(&mut tx,&attachment_id,&auth.claims.sub,&space_id,&doc_id,"direct",&Uuid::new_v4().to_string(),&original).await.map_err(AppError::Conflict)?;
    check_attachment_page(
        &mut tx,
        &auth,
        &space_id,
        &doc_id,
        super::access::PageOperation::Edit,
    )
    .await?;
    if !storage::attachment_retirement::matches_staged_association(&mut tx,&retirement).await.map_err(AppError::Conflict)? {
        return Err(AppError::Conflict("Attachment retirement changed".into()));
    }
    #[cfg(target_os="linux")]
    super::asset_audiences::require_change(&mut tx,&auth,&space_id,&doc_id,&attachment_id,scope.as_ref()).await?;
    tx.commit().await.map_err(AppError::internal)?;
    // Original SQL association, byte custody and immutable receipts remain retained. Keep the inaccessible
    // orphan instead of unlinking an unknown replacement or losing rollback data.
    tracing::warn!(attachment_id=%attachment_id, "Deleted attachment blob retained for future owned cleanup");
    Ok(StatusCode::NO_CONTENT)
}

/// Retained metadata is a current-page read; it supplies no file/reinstallation authority.
async fn list_attachment_retirements(
    State(state): State<AppState>, auth: AuthUser,
    Path((space_id,doc_id)): Path<(String,String)>,
) -> Result<Json<Vec<storage::attachment_retirement::Retirement>>,AppError> {
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    let collection_proof=super::asset_audiences::capture_collection(&mut conn,&auth,&space_id).await?;
    super::access::require_current_human_read(&mut conn,&auth).await?;
    let page=super::shared_pages::asset_view(&mut conn,&auth,&space_id,&doc_id).await?;
    let mut proofs=super::published_source::capture_source(&mut conn,&auth,&space_id,&doc_id,&page.ancestor_ids).await?;
    let query="SELECT r.*,a.id AS current_attachment_id,a.document_id AS current_document_id,a.space_id AS current_space_id,a.filename AS current_filename,a.original_name AS current_original_name,a.mime_type AS current_mime_type,a.size_bytes AS current_size_bytes,a.storage_path AS current_storage_path,a.created_by AS current_created_by,a.created_at AS current_created_at FROM attachment_retirements r JOIN attachments a ON a.id=r.attachment_id WHERE a.space_id=$1 AND a.document_id=$2 ORDER BY r.attachment_id";
    proofs.push(super::published_source::SourceProof {query:query.into(),parameters:vec![space_id.clone(),doc_id.clone()],rows:super::document_deletion::rows(&mut conn,query,&[&space_id,&doc_id]).await.map_err(AppError::internal)?});
    let ids:Vec<String>=sqlx::query_scalar("SELECT r.attachment_id FROM attachment_retirements r JOIN attachments a ON a.id=r.attachment_id WHERE a.space_id=$1 AND a.document_id=$2 ORDER BY r.attachment_id")
        .bind(&space_id).bind(&doc_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let mut result=Vec::with_capacity(ids.len());
    for id in ids {result.push(storage::attachment_retirement::read(&mut conn,&id).await.map_err(AppError::Conflict)?.ok_or_else(||AppError::Conflict("Retirement disappeared".into()))?);}
    super::shared_pages::asset_view(&mut conn,&auth,&space_id,&doc_id).await?;
    super::published_source::verify_source(&mut conn,&proofs).await?;
    super::shared_pages::asset_view(&mut conn,&auth,&space_id,&doc_id).await?;
    super::access::require_current_human_read(&mut conn,&auth).await?;
    let mut allowed=Vec::new();
    for item in result {if super::asset_audiences::visible(&mut conn,&auth,&space_id,&doc_id,&item.attachment_id).await? {allowed.push(item)}}
    super::asset_audiences::verify_collection(&mut conn,&auth,&collection_proof).await?;
    Ok(Json(allowed))
}

async fn begin_attachment_mutation<'a>(
    state: &'a AppState,
    auth: &AuthUser,
    space_id: &str,
    doc_id: &str,
) -> Result<sqlx::Transaction<'a, sqlx::Any>, AppError> {
    begin_attachment_mutation_scoped(state,auth,space_id,doc_id,false).await
}
async fn begin_attachment_mutation_scoped<'a>(state:&'a AppState,auth:&AuthUser,space_id:&str,doc_id:&str,audience:bool)->Result<sqlx::Transaction<'a,sqlx::Any>,AppError>{
    super::access::principal_from_auth(auth)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    if audience {super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;}
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[space_id]).await?;
    check_attachment_page(
        &mut tx,
        auth,
        space_id,
        doc_id,
        super::access::PageOperation::Edit,
    )
    .await?;
    let locked=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND deleted_at IS NULL AND archived_at IS NULL")
        .bind(doc_id).bind(space_id).execute(&mut *tx).await.map_err(AppError::internal)?;
    if locked.rows_affected() != 1 {
        return Err(attachment_unavailable());
    }
    check_attachment_page(
        &mut tx,
        auth,
        space_id,
        doc_id,
        super::access::PageOperation::Edit,
    )
    .await?;
    Ok(tx)
}
async fn store_attachment(
    state: &AppState,
    auth: &AuthUser,
    space_id: &str,
    doc_id: &str,
    original_name: &str,
    content_type: &str,
    bytes: &[u8],
) -> Result<AttachmentResponse, AppError> {
    let mut tx = begin_attachment_mutation(state, auth, space_id, doc_id).await?;
    let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1")
        .bind(space_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if !safe_component(&slug) || !safe_component(doc_id) {
        return Err(attachment_unavailable());
    }
    let attachment_id = Uuid::new_v4().to_string();
    let filename = format!(
        "{}.{}",
        attachment_id,
        safe_attachment_extension(original_name)
    );
    let storage_path = format!("spaces/{slug}/attachments/{doc_id}/{filename}");
    let dir = storage::attachments_dir(&slug).join(doc_id);
    reject_symlink_components(&dir).await?;
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(AppError::internal)?;
    reject_symlink_components(&dir).await?;
    let path = dir.join(&filename);
    write_new_attachment(&path, bytes).await?;
    // Do not clean up an uncertain pathname on any error. Failed DB changes
    // roll back; a retained unassociated blob is inaccessible through /files.
    let staged=async {
        check_attachment_page(&mut tx,auth,space_id,doc_id,super::access::PageOperation::Edit).await?;
        let inserted=sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(&attachment_id).bind(doc_id).bind(space_id).bind(&filename).bind(original_name).bind(content_type).bind(bytes.len() as i64).bind(&storage_path).bind(&auth.claims.sub).execute(&mut *tx).await.map_err(AppError::internal)?;
        if inserted.rows_affected()!=1{return Err(attachment_unavailable());}
        let association:Option<i64>=sqlx::query_scalar("SELECT 1 FROM attachments WHERE NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=attachments.id) AND id=$1 AND document_id=$2 AND space_id=$3 AND storage_path=$4 AND created_by=$5 AND filename=$6 AND original_name=$7 AND mime_type=$8 AND size_bytes=$9")
            .bind(&attachment_id).bind(doc_id).bind(space_id).bind(&storage_path).bind(&auth.claims.sub).bind(&filename).bind(original_name).bind(content_type).bind(bytes.len() as i64).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
        if association.is_none(){return Err(attachment_unavailable());}
        check_attachment_page(&mut tx,auth,space_id,doc_id,super::access::PageOperation::Edit).await?;
        Ok::<(),AppError>(())
    }.await;
    if let Err(error) = staged {
        tx.rollback().await.map_err(AppError::internal)?;
        tracing::warn!(attachment_id=%attachment_id,"Rolled-back attachment blob retained for future owned cleanup");
        return Err(error);
    }
    super::asset_audiences::admit_legacy(&mut tx,&attachment_id,space_id,doc_id).await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(AttachmentResponse {
        audience:None,
        logical_attachment_id: None,
        document_id: doc_id.into(),
        id: attachment_id,
        filename,
        original_name: original_name.into(),
        mime_type: content_type.into(),
        size_bytes: bytes.len() as i64,
        url: format!("/api/v1/files/{storage_path}"),
        created_at: chrono::Utc::now().to_rfc3339(),
    })
}
async fn write_new_attachment(path: &std::path::Path, bytes: &[u8]) -> Result<(), AppError> {
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await
        .map_err(AppError::internal)?;
    file.write_all(bytes).await.map_err(AppError::internal)?;
    file.flush().await.map_err(AppError::internal)
}
#[cfg(test)]
thread_local! {
    static AFTER_FILE_PAGE_PROOF:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=std::cell::RefCell::new(None);
    static BEFORE_FILE_RESPONSE:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=std::cell::RefCell::new(None);
}
fn safe_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.contains('/')
        && !value.contains('\\')
}
async fn reject_symlink_components(path: &std::path::Path) -> Result<(), AppError> {
    let root = storage::data_dir();
    let relative = path
        .strip_prefix(&root)
        .map_err(|_| attachment_unavailable())?;
    let mut current = root;
    for component in std::iter::once(None).chain(relative.components().map(Some)) {
        if let Some(component) = component {
            current.push(component.as_os_str());
        }
        match tokio::fs::symlink_metadata(&current).await {
            Ok(meta) if meta.file_type().is_symlink() => return Err(attachment_unavailable()),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(AppError::internal(error)),
        }
    }
    Ok(())
}
async fn checked_attachment_path(
    conn: &mut sqlx::AnyConnection,
    space_id: &str,
    doc_id: &str,
    path: &str,
) -> Result<std::path::PathBuf, AppError> {
    let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1")
        .bind(space_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if !lexical_attachment_path(&slug, doc_id, path) {
        return Err(attachment_unavailable());
    }
    let full = storage::data_dir().join(path);
    reject_symlink_components(&full).await?;
    Ok(full)
}

#[cfg(test)]
thread_local! {
    static BEFORE_LIST_RESPONSE:std::cell::RefCell<Option<&'static str>>=std::cell::RefCell::new(None);
}
#[cfg(test)]
async fn apply_test_list_change(conn: &mut sqlx::AnyConnection) -> Result<(), AppError> {
    let change = BEFORE_LIST_RESPONSE.with(|slot| slot.borrow_mut().take());
    if let Some(change) = change {
        sqlx::query(change)
            .execute(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    }
    Ok(())
}
fn lexical_attachment_path(slug: &str, doc_id: &str, path: &str) -> bool {
    let prefix = format!("spaces/{slug}/attachments/{doc_id}/");
    safe_component(slug)
        && safe_component(doc_id)
        && path.strip_prefix(&prefix).is_some_and(safe_component)
        && storage::validate_path(path)
}
async fn attachment_response_current(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    attachment: &AttachmentResponse,
) -> Result<bool, AppError> {
    if !super::asset_audiences::visible(conn,auth,space,&attachment.document_id,&attachment.id).await? {return Ok(false)}
    if attachment_family_id(conn,&attachment.id).await?!=attachment.logical_attachment_id{return Ok(false)}
    let Some(path) = attachment.url.strip_prefix("/api/v1/files/") else {
        return Ok(false);
    };
    let slug: Option<String> = sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1")
        .bind(space)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if !slug
        .as_deref()
        .is_some_and(|slug| lexical_attachment_path(slug, &attachment.document_id, path))
    {
        return Ok(false);
    }
    let current:Option<i64>=sqlx::query_scalar("SELECT 1 FROM attachments WHERE NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=attachments.id) AND id=$1 AND document_id=$2 AND space_id=$3 AND storage_path=$4 AND filename=$5 AND original_name=$6 AND mime_type=$7 AND size_bytes=$8 AND created_at=$9")
        .bind(&attachment.id).bind(&attachment.document_id).bind(space).bind(path).bind(&attachment.filename).bind(&attachment.original_name).bind(&attachment.mime_type).bind(attachment.size_bytes).bind(&attachment.created_at).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if current.is_none() {
        return Ok(false);
    }
    attachment_page_visible(conn, auth, space, &attachment.document_id).await
}
async fn attachment_response_current_for_page(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    attachment: &AttachmentResponse,
) -> Result<bool, AppError> {
    if !super::asset_audiences::visible(conn,auth,space,&attachment.document_id,&attachment.id).await? {return Ok(false)}
    if attachment_family_id(conn,&attachment.id).await?!=attachment.logical_attachment_id{return Ok(false)}
    let Some(path) = attachment.url.strip_prefix("/api/v1/files/") else {
        return Ok(false);
    };
    let slug: Option<String> = sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1")
        .bind(space)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if !slug
        .as_deref()
        .is_some_and(|slug| lexical_attachment_path(slug, &attachment.document_id, path))
    {
        return Ok(false);
    }
    let current:Option<i64>=sqlx::query_scalar("SELECT 1 FROM attachments WHERE NOT EXISTS (SELECT 1 FROM attachment_retirements r WHERE r.attachment_id=attachments.id) AND id=$1 AND document_id=$2 AND space_id=$3 AND storage_path=$4 AND filename=$5 AND original_name=$6 AND mime_type=$7 AND size_bytes=$8 AND created_at=$9")
        .bind(&attachment.id).bind(&attachment.document_id).bind(space).bind(path).bind(&attachment.filename).bind(&attachment.original_name).bind(&attachment.mime_type).bind(attachment.size_bytes).bind(&attachment.created_at).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if current.is_none() {
        return Ok(false);
    }
    match super::asset_audiences::asset_page(conn,auth,space,&attachment.document_id,&attachment.id).await {
        Ok(_)=>Ok(true),Err(AppError::Forbidden(_)|AppError::NotFound(_))=>Ok(false),Err(e)=>Err(e),
    }
}
fn attachment_unavailable() -> AppError {
    AppError::NotFound("Attachment/page unavailable".into())
}
pub(crate) async fn check_attachment_page(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space_id: &str,
    doc_id: &str,
    operation: super::access::PageOperation,
) -> Result<(), AppError> {
    let principal = super::access::principal_from_auth(auth)?;
    super::access::resolve_page_access(
        conn,
        principal,
        space_id,
        doc_id,
        operation,
        super::access::Purpose::Routine,
    )
    .await
    .map(|_| ())
    .map_err(|error| match error {
        AppError::NotFound(_) | AppError::Forbidden(_) => attachment_unavailable(),
        other => other,
    })
}
async fn attachment_page_visible(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space_id: &str,
    doc_id: &str,
) -> Result<bool, AppError> {
    match check_attachment_page(
        conn,
        auth,
        space_id,
        doc_id,
        super::access::PageOperation::View,
    )
    .await
    {
        Ok(()) => Ok(true),
        Err(AppError::NotFound(_)) => Ok(false),
        Err(other) => Err(other),
    }
}
async fn check_attachment_browser(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space_id: &str,
) -> Result<(), AppError> {
    let principal = super::access::principal_from_auth(auth)?;
    let access = super::access::resolve_space_access(
        conn,
        principal,
        space_id,
        super::access::Purpose::Routine,
    )
    .await?;
    // Reusable-asset browser retains its existing content-editor restriction.
    if !access.allows(super::access::Permission::EditContent) {
        return Err(attachment_unavailable());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[cfg(target_os="linux")]
    include!("engine_transferred_application_fixture.inc");
    use super::{
        parse_upload_size_mb, safe_attachment_extension, validate_attachment_upload_field,
    };

    async fn fixture() -> crate::AppState {
        let state = crate::spaces::properties::tests::property_test_state().await;
        sqlx::query("UPDATE spaces SET slug=$1 WHERE id='space-a'")
            .bind(format!("attachment-{}", uuid::Uuid::new_v4()))
            .execute(&state.db)
            .await
            .unwrap();
        for user in ["owner-a", "editor-a", "viewer-a"] {
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'hash','2999-01-01T00:00:00Z')").bind(format!("asset-{user}")).bind(user).execute(&state.db).await.unwrap();
        }
        state
    }
    async fn wal_fixture() -> crate::AppState {
        let mut state = fixture().await;
        let path = format!("/tmp/attachment-wal-{}.db", uuid::Uuid::new_v4());
        state.db = sqlx::any::AnyPoolOptions::new()
            .max_connections(2)
            .connect(&format!("sqlite://{path}?mode=rwc"))
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&state.db)
            .await
            .unwrap();
        for (user, role) in [("owner-a", "owner"), ("viewer-a", "viewer")] {
            sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')")
                .bind(user)
                .bind(format!("{user}@example.test"))
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'hash','2999-01-01T00:00:00Z')").bind(format!("asset-{user}")).bind(user).execute(&state.db).await.unwrap();
            if user == "owner-a" {
                sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('space-a','Asset',$1,'private','owner-a')").bind(format!("asset-wal-{}",uuid::Uuid::new_v4())).execute(&state.db).await.unwrap();
            }
            sqlx::query(
                "INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'space-a',$2,$3)",
            )
            .bind(format!("member-{user}"))
            .bind(user)
            .bind(role)
            .execute(&state.db)
            .await
            .unwrap();
        }
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','Asset','','owner-a','owner-a','published')").execute(&state.db).await.unwrap();
        state
    }
    fn auth(user: &str) -> crate::auth::middleware::AuthUser {
        let mut auth = crate::spaces::properties::tests::test_auth(user);
        auth.claims.sid = format!("asset-{user}");
        auth
    }
    async fn asset(state: &crate::AppState) -> super::AttachmentResponse {
        super::store_attachment(
            state,
            &auth("owner-a"),
            "space-a",
            "target-a",
            "unused.pdf",
            "application/pdf",
            b"UNREFERENCED SECRET",
        )
        .await
        .unwrap()
    }
    async fn response_json(response: impl axum::response::IntoResponse) -> serde_json::Value {
        let response = response.into_response();
        serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap()
    }
    async fn private(state: &crate::AppState) {
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,space_id,document_id,owner_id,title,base_updated_at) VALUES('asset-draft','space-a','target-a','owner-a','Private','old')").execute(&state.db).await.unwrap();
    }
    #[tokio::test]
    async fn document_owned_unreferenced_pool_remains_visible_without_uploader_grant() {
        let state = fixture().await;
        let asset = asset(&state).await;
        let body = response_json(
            super::list_attachments(
                axum::extract::State(state.clone()),
                auth("viewer-a"),
                axum::extract::Path(("space-a".into(), "target-a".into())),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["original_name"], "unused.pdf");
        let response = super::serve_attachment_file(
            axum::extract::State(state.clone()),
            auth("viewer-a"),
            axum::extract::Path(asset.url.trim_start_matches("/api/v1/files/").into()),
        )
        .await
        .unwrap();
        assert_eq!(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()[..],
            b"UNREFERENCED SECRET"
        );
        sqlx::query("DELETE FROM space_members WHERE user_id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::list_attachments(
            axum::extract::State(state),
            auth("owner-a"),
            axum::extract::Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn reusable_browser_and_download_hide_private_ancestors_foreign_and_inactive_pages() {
        let state = fixture().await;
        let asset = asset(&state).await;
        private(&state).await;
        let path = asset.url.trim_start_matches("/api/v1/files/");
        assert!(matches!(
            super::serve_attachment_file(
                axum::extract::State(state.clone()),
                auth("editor-a"),
                axum::extract::Path(path.into())
            )
            .await,
            Err(crate::auth::routes::AppError::NotFound(_))
        ));
        let list = response_json(
            super::list_space_attachments(
                axum::extract::State(state.clone()),
                auth("editor-a"),
                axum::extract::Path("space-a".into()),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(list, serde_json::json!([]));
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('asset-draft','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        assert_eq!(
            response_json(
                super::list_space_attachments(
                    axum::extract::State(state.clone()),
                    auth("editor-a"),
                    axum::extract::Path("space-a".into())
                )
                .await
                .unwrap()
            )
            .await
            .as_array()
            .unwrap()
            .len(),
            1
        );
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('private-parent','space-a','Hidden','x','owner-a','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET published_at='published',parent_id='private-parent' WHERE id='target-a'").execute(&state.db).await.unwrap();
        assert_eq!(
            response_json(
                super::list_space_attachments(
                    axum::extract::State(state.clone()),
                    auth("editor-a"),
                    axum::extract::Path("space-a".into())
                )
                .await
                .unwrap()
            )
            .await,
            serde_json::json!([])
        );
        sqlx::query(
            "UPDATE documents SET parent_id=NULL,archived_at='archived' WHERE id='target-a'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert!(super::list_attachments(
            axum::extract::State(state.clone()),
            auth("editor-a"),
            axum::extract::Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_err());
        sqlx::query("UPDATE documents SET archived_at=NULL,space_id='space-b' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            response_json(
                super::list_space_attachments(
                    axum::extract::State(state),
                    auth("editor-a"),
                    axum::extract::Path("space-a".into())
                )
                .await
                .unwrap()
            )
            .await,
            serde_json::json!([])
        );
    }
    #[tokio::test]
    async fn writes_require_current_private_editor_lock_lifecycle_and_credentials() {
        let state = fixture().await;
        private(&state).await;
        for role in ["viewer", "editor"] {
            sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('asset-draft','editor-a',$1,'owner-a') ON CONFLICT(draft_id,user_id) DO UPDATE SET role=excluded.role").bind(role).execute(&state.db).await.unwrap();
            let result = super::store_attachment(
                &state,
                &auth("editor-a"),
                "space-a",
                "target-a",
                "test.bin",
                "application/octet-stream",
                b"test",
            )
            .await;
            assert_eq!(result.is_ok(), role == "editor");
        }
        sqlx::query("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(matches!(
            super::store_attachment(
                &state,
                &auth("editor-a"),
                "space-a",
                "target-a",
                "test.bin",
                "x",
                b"test"
            )
            .await,
            Err(crate::auth::routes::AppError::Conflict(_))
        ));
        assert!(super::list_attachments(
            axum::extract::State(state.clone()),
            auth("editor-a"),
            axum::extract::Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_ok());
        sqlx::query("UPDATE documents SET locked=0,deleted_at='deleted' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::store_attachment(
            &state,
            &auth("owner-a"),
            "space-a",
            "target-a",
            "test.bin",
            "x",
            b"test"
        )
        .await
        .is_err());
        sqlx::query("UPDATE documents SET deleted_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE user_id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(matches!(
            super::store_attachment(
                &state,
                &auth("owner-a"),
                "space-a",
                "target-a",
                "test.bin",
                "x",
                b"test"
            )
            .await,
            Err(crate::auth::routes::AppError::Unauthorized(_))
        ));
    }
    #[tokio::test]
    async fn late_upload_revocation_rolls_back_row_and_keeps_unassociated_blob_unreadable() {
        let state = fixture().await;
        let sessions_before: Vec<String> =
            sqlx::query_scalar("SELECT id FROM sessions WHERE user_id='owner-a' ORDER BY id")
                .fetch_all(&state.db)
                .await
                .unwrap();
        sqlx::query("CREATE TRIGGER revoke_asset_insert AFTER INSERT ON attachments BEGIN DELETE FROM sessions WHERE user_id='owner-a'; END").execute(&state.db).await.unwrap();
        assert!(matches!(
            super::store_attachment(
                &state,
                &auth("owner-a"),
                "space-a",
                "target-a",
                "secret.pdf",
                "application/pdf",
                b"RETAIN"
            )
            .await,
            Err(crate::auth::routes::AppError::Unauthorized(_))
        ));
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let sessions: Vec<String> =
            sqlx::query_scalar("SELECT id FROM sessions WHERE user_id='owner-a' ORDER BY id")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(sessions, sessions_before);
        let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id='space-a'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        let mut files =
            tokio::fs::read_dir(crate::storage::attachments_dir(&slug).join("target-a"))
                .await
                .unwrap();
        let retained = files.next_entry().await.unwrap().unwrap();
        assert_eq!(tokio::fs::read(retained.path()).await.unwrap(), b"RETAIN");
        let path = format!(
            "spaces/{slug}/attachments/target-a/{}",
            retained.file_name().to_string_lossy()
        );
        assert!(matches!(
            super::serve_attachment_file(
                axum::extract::State(state),
                auth("owner-a"),
                axum::extract::Path(path)
            )
            .await,
            Err(crate::auth::routes::AppError::NotFound(_))
        ));
    }
    #[tokio::test]
    async fn delete_sql_failure_and_late_revocation_preserve_canonical_blob() {
        let state = fixture().await;
        let asset = asset(&state).await;
        let path = asset.url.trim_start_matches("/api/v1/files/");
        for trigger in ["CREATE TRIGGER asset_delete BEFORE INSERT ON attachment_retirements BEGIN SELECT RAISE(ABORT,'failure'); END","CREATE TRIGGER asset_delete AFTER INSERT ON attachment_retirements BEGIN DELETE FROM sessions WHERE user_id='owner-a'; END"] {
            sqlx::query(trigger).execute(&state.db).await.unwrap();
            assert!(super::delete_attachment(axum::extract::State(state.clone()),auth("owner-a"),axum::extract::Path(("space-a".into(),"target-a".into(),asset.id.clone())),axum::http::HeaderMap::new()).await.is_err());
            assert_eq!(tokio::fs::read(crate::storage::data_dir().join(path)).await.unwrap(),b"UNREFERENCED SECRET");
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachments").fetch_one(&state.db).await.unwrap(),1);
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_retirements").fetch_one(&state.db).await.unwrap(),0);
            sqlx::query("DROP TRIGGER asset_delete").execute(&state.db).await.unwrap();
        }
        super::delete_attachment(
            axum::extract::State(state.clone()),
            auth("owner-a"),
            axum::extract::Path(("space-a".into(), "target-a".into(), asset.id)),
            axum::http::HeaderMap::new(),
        )
        .await
        .unwrap();
        assert!(tokio::fs::try_exists(crate::storage::data_dir().join(path))
            .await
            .unwrap());
        assert!(matches!(
            super::serve_attachment_file(
                axum::extract::State(state),
                auth("owner-a"),
                axum::extract::Path(path.into())
            )
            .await,
            Err(crate::auth::routes::AppError::NotFound(_))
        ));
    }
    #[tokio::test]
    async fn unsafe_stored_asset_paths_never_read_unowned_bytes() {
        let state = fixture().await;
        let asset = asset(&state).await;
        for path in [
            "spaces/space-b/attachments/target-b/secret.pdf",
            "spaces/../secret.pdf",
            "/etc/passwd",
        ] {
            sqlx::query("UPDATE attachments SET storage_path=$1 WHERE id=$2")
                .bind(path)
                .bind(&asset.id)
                .execute(&state.db)
                .await
                .unwrap();
            assert!(super::serve_attachment_file(
                axum::extract::State(state.clone()),
                auth("owner-a"),
                axum::extract::Path(path.into())
            )
            .await
            .is_err());
        }
    }
    #[tokio::test]
    async fn current_session_is_rechecked_after_file_read_before_bytes_returned() {
        // Real file WAL + max2 permits external revocation during file read.
        let state = wal_fixture().await;
        let asset = asset(&state).await;
        let db = state.db.clone();
        super::BEFORE_FILE_RESPONSE.with(|slot| {
            *slot.borrow_mut() = Some(Box::pin(async move {
                sqlx::query("DELETE FROM sessions WHERE user_id='viewer-a'")
                    .execute(&db)
                    .await
                    .unwrap();
            }))
        });
        assert!(matches!(
            super::serve_attachment_file(
                axum::extract::State(state),
                auth("viewer-a"),
                axum::extract::Path(asset.url.trim_start_matches("/api/v1/files/").into())
            )
            .await,
            Err(crate::auth::routes::AppError::Unauthorized(_))
        ));
    }
    #[tokio::test]
    async fn exclusive_upload_never_overwrites_and_symlink_download_is_denied() {
        let state = fixture().await;
        let asset = asset(&state).await;
        let path = crate::storage::data_dir().join(asset.url.trim_start_matches("/api/v1/files/"));
        assert!(super::write_new_attachment(&path, b"REPLACEMENT")
            .await
            .is_err());
        assert_eq!(
            tokio::fs::read(&path).await.unwrap(),
            b"UNREFERENCED SECRET"
        );
        #[cfg(unix)]
        {
            let outside = path.with_file_name("unowned.pdf");
            tokio::fs::write(&outside, b"UNOWNED").await.unwrap();
            tokio::fs::remove_file(&path).await.unwrap();
            std::os::unix::fs::symlink(&outside, &path).unwrap();
            assert!(super::serve_attachment_file(
                axum::extract::State(state),
                auth("owner-a"),
                axum::extract::Path(asset.url.trim_start_matches("/api/v1/files/").into())
            )
            .await
            .is_err());
            assert_eq!(tokio::fs::read(&outside).await.unwrap(), b"UNOWNED");
        }
    }
    #[tokio::test]
    async fn http_key_method_scope_matches_read_and_write_asset_operations() {
        use tower::ServiceExt;
        let state = fixture().await;
        let asset = asset(&state).await;
        let key = "asset-http-key";
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('asset-key','owner-a','Asset',$1,'prefix','[\"read\"]')").bind(crate::auth::api_keys::hash_api_key(key)).execute(&state.db).await.unwrap();
        let app = super::router().with_state(state.clone());
        let req = |method: &str, uri: &str| {
            axum::http::Request::builder()
                .method(method)
                .uri(uri)
                .header("X-API-Key", key)
                .body(axum::body::Body::empty())
                .unwrap()
        };
        let path = asset.url.trim_start_matches("/api/v1");
        assert_eq!(
            app.clone()
                .oneshot(req("GET", path))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::OK
        );
        let delete = format!(
            "/spaces/space-a/documents/target-a/attachments/{}",
            asset.id
        );
        assert_eq!(
            app.clone()
                .oneshot(req("DELETE", &delete))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::FORBIDDEN
        );
        sqlx::query("UPDATE api_keys SET scopes='[\"write\"]' WHERE id='asset-key'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            app.clone()
                .oneshot(req("GET", path))
                .await
                .unwrap()
                .status(),
            axum::http::StatusCode::FORBIDDEN
        );
        assert_eq!(
            app.oneshot(req("DELETE", &delete)).await.unwrap().status(),
            axum::http::StatusCode::NO_CONTENT
        );
    }
    #[tokio::test]
    async fn upload_rechecks_actual_association_after_insert_trigger() {
        let state = fixture().await;
        sqlx::query("CREATE TRIGGER drift_asset_insert AFTER INSERT ON attachments BEGIN UPDATE attachments SET document_id='target-b',space_id='space-b' WHERE id=NEW.id; END").execute(&state.db).await.unwrap();
        assert!(super::store_attachment(
            &state,
            &auth("owner-a"),
            "space-a",
            "target-a",
            "test.pdf",
            "application/pdf",
            b"KEEP"
        )
        .await
        .is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn final_browser_projection_rechecks_each_private_page_and_owned_url() {
        let state = fixture().await;
        let asset = asset(&state).await;
        private(&state).await;
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('asset-draft','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        super::BEFORE_LIST_RESPONSE.with(|slot| {
            *slot.borrow_mut() = Some("DELETE FROM document_draft_access WHERE user_id='editor-a'")
        });
        let list = response_json(
            super::list_space_attachments(
                axum::extract::State(state.clone()),
                auth("editor-a"),
                axum::extract::Path("space-a".into()),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(list, serde_json::json!([]));
        sqlx::query("UPDATE documents SET published_at='published' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE attachments SET storage_path='spaces/foreign-secret-slug/attachments/foreign-secret-page/file.pdf' WHERE id=$1").bind(asset.id).execute(&state.db).await.unwrap();
        let list = response_json(
            super::list_space_attachments(
                axum::extract::State(state.clone()),
                auth("editor-a"),
                axum::extract::Path("space-a".into()),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(list, serde_json::json!([]));
        let list = response_json(
            super::list_attachments(
                axum::extract::State(state),
                auth("editor-a"),
                axum::extract::Path(("space-a".into(), "target-a".into())),
            )
            .await
            .unwrap(),
        )
        .await;
        assert_eq!(list, serde_json::json!([]));
    }
    #[tokio::test]
    async fn current_space_slug_is_rechecked_after_file_read() {
        let state = wal_fixture().await;
        let asset = asset(&state).await;
        let db = state.db.clone();
        super::BEFORE_FILE_RESPONSE.with(|slot| {
            *slot.borrow_mut() = Some(Box::pin(async move {
                sqlx::query("UPDATE spaces SET slug='changed-after-file-read' WHERE id='space-a'")
                    .execute(&db)
                    .await
                    .unwrap();
            }))
        });
        assert!(matches!(
            super::serve_attachment_file(
                axum::extract::State(state),
                auth("viewer-a"),
                axum::extract::Path(asset.url.trim_start_matches("/api/v1/files/").into())
            )
            .await,
            Err(crate::auth::routes::AppError::NotFound(_))
        ));
    }
    #[test]
    fn parses_upload_size_megabytes() {
        assert_eq!(parse_upload_size_mb("25"), Some(25 * 1024 * 1024));
        assert_eq!(parse_upload_size_mb(" 100 "), Some(100 * 1024 * 1024));
    }

    #[test]
    fn rejects_invalid_upload_size_values() {
        assert_eq!(parse_upload_size_mb("0"), None);
        assert_eq!(parse_upload_size_mb(""), None);
        assert_eq!(parse_upload_size_mb("abc"), None);
    }

    #[test]
    fn attachment_storage_extensions_are_safe_and_bounded() {
        assert_eq!(safe_attachment_extension("notes.pdf"), "pdf");
        assert_eq!(safe_attachment_extension("no-extension"), "bin");
        assert_eq!(safe_attachment_extension("archive.tar.gz"), "gz");
        assert_eq!(safe_attachment_extension("unsafe.dir/name"), "bin");
        assert_eq!(safe_attachment_extension("unsafe.🙃"), "bin");
        assert_eq!(
            safe_attachment_extension(&format!("file.{}", "a".repeat(17))),
            "bin"
        );
    }

    #[test]
    fn attachment_upload_requires_the_documented_file_part_and_filename() {
        assert_eq!(
            validate_attachment_upload_field(Some("file"), Some("notes.pdf")).unwrap(),
            "notes.pdf"
        );
        assert!(validate_attachment_upload_field(Some("metadata"), Some("notes.pdf")).is_err());
        assert!(validate_attachment_upload_field(Some("file"), None).is_err());
        assert!(validate_attachment_upload_field(Some("file"), Some("  ")).is_err());
        assert!(validate_attachment_upload_field(Some("file"), Some(&"a".repeat(256))).is_err());
    }
    #[tokio::test]
    async fn final_asset_alias_projection_denies_external_changes_after_page_proof() {
        for scenario in ["mime", "alias"] {
            let state = wal_fixture().await;
            let asset = asset(&state).await;
            let path = asset.url.trim_start_matches("/api/v1/files/").to_string();
            let db = state.db.clone();
            let id = asset.id.clone();
            let changed_path = path.clone();
            super::AFTER_FILE_PAGE_PROOF.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move{
                if scenario=="mime"{sqlx::query("UPDATE attachments SET mime_type='changed/type' WHERE id=$1").bind(&id).execute(&db).await.unwrap();}
                else{sqlx::query("INSERT INTO move_commit_receipts(op_id,preview_id,actor_id,source_space_id,destination_space_id,request_fingerprint,source_fingerprint,manifest_json,expected_state_json,created_at) VALUES('alias-test','alias-preview','owner-a','space-a','space-a','request','source','{}','{}','now')").execute(&db).await.unwrap();sqlx::query("INSERT INTO move_asset_path_aliases(old_path,asset_id,asset_kind,op_id,created_at) VALUES($1,$2,'attachment','alias-test','now')").bind(&changed_path).bind(&id).execute(&db).await.unwrap();}
            })));
            assert!(
                matches!(
                    super::serve_attachment_file(
                        axum::extract::State(state.clone()),
                        auth("viewer-a"),
                        axum::extract::Path(path)
                    )
                    .await,
                    Err(crate::auth::routes::AppError::NotFound(_))
                ),
                "{scenario}"
            );
            let mut conn = state.db.acquire().await.unwrap();
            super::check_attachment_page(
                &mut conn,
                &auth("viewer-a"),
                "space-a",
                "target-a",
                crate::spaces::access::PageOperation::View,
            )
            .await
            .unwrap();
        }
    }
    #[tokio::test]
    async fn snapshot_download_slow_poll_retains_proven_bytes_after_source_changes() {
        use futures::StreamExt;
        let state = fixture().await;
        let asset = asset(&state).await;
        let path = asset.url.trim_start_matches("/api/v1/files/").to_string();
        let full_path = crate::storage::data_dir().join(&path);
        let bytes: Vec<u8> = (0..64 * 1024 * 3 + 19)
            .map(|index| (index % 256) as u8)
            .collect();
        tokio::fs::write(&full_path, &bytes).await.unwrap();
        sqlx::query("UPDATE attachments SET size_bytes=$1 WHERE id=$2")
            .bind(bytes.len() as i64)
            .bind(&asset.id)
            .execute(&state.db)
            .await
            .unwrap();
        let response = super::serve_attachment_file(
            axum::extract::State(state),
            auth("viewer-a"),
            axum::extract::Path(path),
        )
        .await
        .unwrap();
        assert_eq!(
            response.headers()[axum::http::header::CONTENT_LENGTH],
            bytes.len().to_string()
        );
        let mut stream = response.into_body().into_data_stream();
        let first = stream.next().await.unwrap().unwrap();
        assert!(!first.is_empty());
        #[cfg(target_os = "linux")]
        assert!(first.len() <= 64 * 1024);
        tokio::fs::write(&full_path, vec![99; bytes.len()])
            .await
            .unwrap();
        let mut received = first.to_vec();
        while let Some(chunk) = stream.next().await {
            tokio::task::yield_now().await;
            #[cfg(target_os = "linux")]
            assert!(chunk.as_ref().unwrap().len() <= 64 * 1024);
            received.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(received, bytes);
    }
    #[tokio::test]
    async fn snapshot_download_router_preserves_head_headers_auth_and_missing_behavior() {
        use tower::ServiceExt;
        let state = fixture().await;
        let asset = asset(&state).await;
        let key = "snapshot-router-key";
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('snapshot-key','owner-a','Snapshot',$1,'prefix','[\"read\"]')")
            .bind(crate::auth::api_keys::hash_api_key(key)).execute(&state.db).await.unwrap();
        let app = super::router().with_state(state.clone());
        let path = asset.url.trim_start_matches("/api/v1");
        let request = |method: &str, authenticated: bool| {
            let builder = axum::http::Request::builder().method(method).uri(path);
            let builder = if authenticated {
                builder.header("X-API-Key", key)
            } else {
                builder
            };
            builder.body(axum::body::Body::empty()).unwrap()
        };
        for method in ["GET", "HEAD"] {
            let response = app.clone().oneshot(request(method, true)).await.unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::OK);
            assert_eq!(
                response.headers()[axum::http::header::CONTENT_TYPE],
                "application/pdf"
            );
            assert_eq!(
                response.headers()[axum::http::header::CONTENT_LENGTH],
                b"UNREFERENCED SECRET".len().to_string()
            );
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap();
            assert_eq!(
                body.as_ref(),
                if method == "HEAD" {
                    &b""[..]
                } else {
                    &b"UNREFERENCED SECRET"[..]
                }
            );
        }
        for method in ["GET", "HEAD"] {
            let response = app.clone().oneshot(request(method, false)).await.unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
        }
        tokio::fs::remove_file(
            crate::storage::data_dir().join(asset.url.trim_start_matches("/api/v1/files/")),
        )
        .await
        .unwrap();
        for method in ["GET", "HEAD"] {
            let response = app.clone().oneshot(request(method, true)).await.unwrap();
            assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
        }
    }
    #[cfg(target_os="linux")]
    fn isolated_upload_case(case:&str)->bool {
        if std::env::var("STARLANE_UPLOAD_CANDIDATE_CASE").ok().as_deref()==Some(case){return false;}
        let root=std::env::temp_dir().join(format!("actual-upload-{case}-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();
        use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&root,std::fs::Permissions::from_mode(0o700)).unwrap();
        let out=std::process::Command::new(std::env::current_exe().unwrap()).arg("--exact").arg(format!("spaces::attachments::tests::{case}")).arg("--nocapture").env("DATA_DIR",&root).env("STARLANE_UPLOAD_CANDIDATE_CASE",case).output().unwrap();
        assert!(out.status.success(),"{} {}",String::from_utf8_lossy(&out.stdout),String::from_utf8_lossy(&out.stderr));assert!(String::from_utf8_lossy(&out.stdout).contains("1 passed;"));std::fs::remove_dir_all(root).unwrap();true
    }
    #[cfg(target_os="linux")]
    async fn upload_fixture()->crate::AppState{let s=fixture().await;let _:i64=sqlx::query_scalar("SELECT count(*) FROM attachment_upload_receipts").fetch_one(&s.db).await.unwrap();s}
    #[cfg(target_os="linux")]
    fn actual_upload_request(state:&crate::AppState,id:&str,bytes:&[u8],name:&str)->axum::http::Request<axum::body::Body>{
        let claims=auth("owner-a").claims;let jwt=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();
        let mut body=format!("--upload\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").into_bytes();body.extend_from_slice(bytes);body.extend_from_slice(b"\r\n--upload--\r\n");
        axum::http::Request::builder().method("POST").uri("/spaces/space-a/documents/target-a/attachments").header("authorization",format!("Bearer {jwt}")).header("x-starlane-upload-operation",id).header("content-type","multipart/form-data; boundary=upload").body(axum::body::Body::from(body)).unwrap()
    }
    #[cfg(target_os="linux")]
    #[tokio::test]
    async fn direct_upload_actual_jwt_lost_ack_exact_retry_mismatch_and_private_download(){
        let case="direct_upload_actual_jwt_lost_ack_exact_retry_mismatch_and_private_download";if isolated_upload_case(case){return;}
        use tower::ServiceExt;let state=upload_fixture().await;let app=super::router().with_state(state.clone());let id=uuid::Uuid::new_v4().to_string();
        let first=app.clone().oneshot(actual_upload_request(&state,&id,b"private bytes","x.bin")).await.unwrap();assert_eq!(first.status(),axum::http::StatusCode::CREATED);drop(first); // Actual committed response deliberately not consumed.
        let second=app.clone().oneshot(actual_upload_request(&state,&id,b"private bytes","x.bin")).await.unwrap();assert_eq!(second.status(),axum::http::StatusCode::CREATED);let body=axum::body::to_bytes(second.into_body(),10000).await.unwrap();let response:serde_json::Value=serde_json::from_slice(&body).unwrap();let url=response["url"].as_str().unwrap().trim_start_matches("/api/v1");
        for (b,n) in [(b"different".as_slice(),"x.bin"),(b"private bytes".as_slice(),"renamed.bin")]{let r=app.clone().oneshot(actual_upload_request(&state,&id,b,n)).await.unwrap();assert_eq!(r.status(),axum::http::StatusCode::CONFLICT);}
        for t in ["attachments","attachment_upload_receipts"]{assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT count(*) FROM {t}")).fetch_one(&state.db).await.unwrap(),1);}
        let mut conn=state.db.acquire().await.unwrap();let r=crate::storage::attachment_upload::receipt(&mut conn,&id).await.unwrap().unwrap();assert!(crate::storage::attachment_upload::verify(&crate::storage::data_dir(),&r,false).unwrap());let inspected=crate::storage::attachment_upload::inspect(&crate::storage::data_dir(),&mut conn,&id).await.unwrap();assert_eq!(inspected.class,crate::storage::attachment_upload::UploadClass::CommittedInstalled);let history=crate::storage::attachment_upload::validate_history(&inspected.raw_intent,&inspected.original_proof,&r.intent.attempt_id).unwrap();assert_eq!(history.intent,r.intent);assert_eq!(history.original_identity,inspected.original_identity);assert_eq!(history.allowed_paths,inspected.relative_paths);assert!(crate::storage::attachment_upload::validate_history(&inspected.raw_intent,&inspected.original_proof,&uuid::Uuid::new_v4().to_string()).is_err());let mut altered=inspected.raw_intent.clone();altered.push(b' ');assert!(crate::storage::attachment_upload::validate_history(&altered,&inspected.original_proof,&r.intent.attempt_id).is_err());drop(conn);
        let denied=app.clone().oneshot(axum::http::Request::builder().uri(url).body(axum::body::Body::empty()).unwrap()).await.unwrap();assert_eq!(denied.status(),axum::http::StatusCode::UNAUTHORIZED);
        let claims=auth("owner-a").claims;let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();let download=app.oneshot(axum::http::Request::builder().uri(url).header("authorization",format!("Bearer {token}")).body(axum::body::Body::empty()).unwrap()).await.unwrap();assert_eq!(download.status(),axum::http::StatusCode::OK);assert_eq!(&axum::body::to_bytes(download.into_body(),10000).await.unwrap()[..],b"private bytes");
    }
    #[cfg(target_os="linux")]
    #[tokio::test]
    async fn direct_upload_empty_policy_rollback_historical_never_recreates(){
        let case="direct_upload_empty_policy_rollback_historical_never_recreates";if isolated_upload_case(case){return;}
        use tower::ServiceExt;let state=upload_fixture().await;let app=super::router().with_state(state.clone());let id=uuid::Uuid::new_v4().to_string();let r=app.clone().oneshot(actual_upload_request(&state,&id,b"","empty.txt")).await.unwrap();assert_eq!(r.status(),axum::http::StatusCode::CREATED);
        // A deleted attachment loses its live association; historical receipts never recreate it.
        sqlx::query("DELETE FROM attachments").execute(&state.db).await.unwrap();let r=app.clone().oneshot(actual_upload_request(&state,&id,b"","empty.txt")).await.unwrap();assert_eq!(r.status(),axum::http::StatusCode::NOT_FOUND);assert_eq!(crate::storage::attachment_upload::recover_in(&crate::storage::data_dir(),&state.db).await.unwrap(),0);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachments").fetch_one(&state.db).await.unwrap(),0);
        sqlx::query("CREATE TRIGGER revoke_upload AFTER INSERT ON attachments BEGIN DELETE FROM sessions WHERE user_id='owner-a'; END").execute(&state.db).await.unwrap();let other=uuid::Uuid::new_v4().to_string();let r=app.oneshot(actual_upload_request(&state,&other,b"retained","other.txt")).await.unwrap();assert_eq!(r.status(),axum::http::StatusCode::UNAUTHORIZED);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_upload_receipts").fetch_one(&state.db).await.unwrap(),1);
    }

    #[cfg(target_os="linux")]
    #[tokio::test]
    async fn direct_upload_real_deferred_commit_failure_preserves_uncommitted_group(){
        let case="direct_upload_real_deferred_commit_failure_preserves_uncommitted_group";if isolated_upload_case(case){return;}
        use tower::ServiceExt;let state=upload_fixture().await;
        sqlx::query("PRAGMA foreign_keys=ON").execute(&state.db).await.unwrap();
        sqlx::query("CREATE TABLE upload_commit_failure(id TEXT PRIMARY KEY,parent TEXT REFERENCES users(id) DEFERRABLE INITIALLY DEFERRED)").execute(&state.db).await.unwrap();
        sqlx::query("CREATE TRIGGER fail_upload_commit AFTER INSERT ON attachment_upload_receipts BEGIN INSERT INTO upload_commit_failure VALUES(NEW.operation_id,'nonexistent-parent'); END").execute(&state.db).await.unwrap();
        let id=uuid::Uuid::new_v4().to_string();let response=super::router().with_state(state.clone()).oneshot(actual_upload_request(&state,&id,b"actual committed intent, failed SQL commit","failure.bin")).await.unwrap();assert_eq!(response.status(),axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        for table in ["attachments","attachment_upload_receipts","upload_commit_failure"]{assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT count(*) FROM {table}")).fetch_one(&state.db).await.unwrap(),0);}
        let root=crate::storage::data_dir();let mut conn=state.db.acquire().await.unwrap();let stages=crate::storage::attachment_upload::inspect_namespace(&root,&mut conn).await.unwrap();assert_eq!(stages.len(),1);assert_eq!(stages[0].class,crate::storage::attachment_upload::UploadClass::SealedUncommitted);let op=root.join(".attachment-upload-journal").join(&stages[0].attempt_id);let raw=std::fs::read(op.join("intent.json")).unwrap();let intent:crate::storage::attachment_upload::Intent=serde_json::from_slice(&raw).unwrap();assert_eq!(std::fs::read(root.join(&intent.storage_path)).unwrap(),b"actual committed intent, failed SQL commit");use std::os::unix::fs::MetadataExt;assert_eq!(std::fs::metadata(op.join("payload.part")).unwrap().nlink(),2);drop(conn);assert_eq!(crate::storage::attachment_upload::recover_in(&root,&state.db).await.unwrap(),0);
    }
    #[cfg(target_os="linux")]
    #[tokio::test]
    async fn direct_upload_real_socket_frames_cross_multipart_budget(){
        let case="direct_upload_real_socket_frames_cross_multipart_budget";if isolated_upload_case(case){return;}
        use tokio::io::{AsyncReadExt,AsyncWriteExt};let state=upload_fixture().await;let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();let address=listener.local_addr().unwrap();let (stop,closed)=tokio::sync::oneshot::channel::<()>();let app=super::router().with_state(state.clone());let server=tokio::spawn(async move{axum::serve(listener,app).with_graceful_shutdown(async{let _=closed.await;}).await.unwrap();});
        let id=uuid::Uuid::new_v4().to_string();let claims=auth("owner-a").claims;let jwt=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();let prefix=b"--socket\r\nContent-Disposition: form-data; name=\"file\"; filename=\"socket.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n";let tail=b"\r\n--socket--\r\n";let length=4*1024*1024+13;let mut socket=tokio::net::TcpStream::connect(address).await.unwrap();let headers=format!("POST /spaces/space-a/documents/target-a/attachments HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {jwt}\r\nX-Starlane-Upload-Operation: {id}\r\nContent-Type: multipart/form-data; boundary=socket\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",prefix.len()+length+tail.len());socket.write_all(headers.as_bytes()).await.unwrap();socket.write_all(prefix).await.unwrap();let chunk=vec![0x5a;length];socket.write_all(&chunk).await.unwrap();socket.write_all(tail).await.unwrap();let mut response=vec![];tokio::time::timeout(std::time::Duration::from_secs(30),socket.read_to_end(&mut response)).await.unwrap().unwrap();assert!(response.starts_with(b"HTTP/1.1 201"),"{}",String::from_utf8_lossy(&response));stop.send(()).unwrap();server.await.unwrap();let mut conn=state.db.acquire().await.unwrap();let receipt=crate::storage::attachment_upload::receipt(&mut conn,&id).await.unwrap().unwrap();assert_eq!(receipt.intent.byte_length,length as i64);let bytes=std::fs::read(crate::storage::data_dir().join(&receipt.intent.storage_path)).unwrap();assert_eq!(bytes.len(),length);assert!(bytes.iter().all(|b|*b==0x5a));assert!(crate::storage::attachment_upload::verify(&crate::storage::data_dir(),&receipt,false).unwrap());
    }

    #[cfg(target_os="linux")]
    async fn upload_page_json(state:&crate::AppState,method:&str,uri:&str,value:serde_json::Value)->serde_json::Value {
        use tower::ServiceExt;let claims=auth("owner-a").claims;let jwt=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();let request=axum::http::Request::builder().method(method).uri(uri).header("authorization",format!("Bearer {jwt}")).header("content-type","application/json").body(axum::body::Body::from(serde_json::to_vec(&value).unwrap())).unwrap();let response=crate::api_routes().with_state(state.clone()).oneshot(request).await.unwrap();let code=response.status();let bytes=axum::body::to_bytes(response.into_body(),100000).await.unwrap();assert!(code.is_success(),"actual {method} {uri}: {code} {}",String::from_utf8_lossy(&bytes));if bytes.is_empty(){serde_json::Value::Null}else{serde_json::from_slice(&bytes).unwrap()}
    }
    #[cfg(target_os="linux")]
    async fn persistent_upload_fixture(seed:bool) -> crate::AppState {
        let mut state = fixture().await;
        let path=crate::storage::data_dir().join("starlane-notes.db");
        state.db = sqlx::any::AnyPoolOptions::new()
            .max_connections(2)
            .connect(&format!("sqlite://{}?mode=rwc",path.display()))
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
        sqlx::query("PRAGMA journal_mode=WAL")
            .execute(&state.db)
            .await
            .unwrap();
        if !seed {return state;}
        for (user, role) in [("owner-a", "owner"), ("viewer-a", "viewer")] {
            sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')")
                .bind(user)
                .bind(format!("{user}@example.test"))
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'hash','2999-01-01T00:00:00Z')").bind(format!("asset-{user}")).bind(user).execute(&state.db).await.unwrap();
            if user == "owner-a" {
                sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('space-a','Asset',$1,'private','owner-a')").bind(format!("asset-wal-{}",uuid::Uuid::new_v4())).execute(&state.db).await.unwrap();
            }
            sqlx::query(
                "INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'space-a',$2,$3)",
            )
            .bind(format!("member-{user}"))
            .bind(user)
            .bind(role)
            .execute(&state.db)
            .await
            .unwrap();
        }
        let created=upload_page_json(&state,"POST","/spaces/space-a/documents",serde_json::json!({"source_id":"target-a","title":"Actual upload producer","content":"Initial original page bytes","property_values":[]})).await;
        assert_eq!(created["id"],"target-a");let draft=created["draft_id"].as_str().unwrap();let uri=format!("/spaces/space-a/documents/target-a/drafts/{draft}");let selected=upload_page_json(&state,"GET",&uri,serde_json::Value::Null).await;
        upload_page_json(&state,"PUT",&uri,serde_json::json!({"source_digest":selected["source_digest"],"title":"Actual upload producer","content":"Saved exact published producer bytes"})).await;
        let reviewed=upload_page_json(&state,"GET",&format!("{uri}/publish-source"),serde_json::Value::Null).await;
        upload_page_json(&state,"POST",&format!("{uri}/publish"),serde_json::json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"draft_source_digest":reviewed["draft_source_digest"],"page_source_digest":reviewed["page_source_digest"]})).await;
        state
    }

    #[cfg(target_os="linux")]
    #[tokio::test(flavor="current_thread")]
    async fn direct_upload_restart_child(){
        let Ok(mode)=std::env::var("STARLANE_UPLOAD_RESTART_PHASE") else{return;};
        use tower::ServiceExt;let root=crate::storage::data_dir();assert!(root.is_absolute());let id=std::env::var("STARLANE_UPLOAD_RESTART_OPERATION").unwrap();let state=persistent_upload_fixture(mode.starts_with("writer")).await;
        if mode.starts_with("writer") {
            let observe=move|receipt:&crate::storage::attachment_upload::Receipt|{let path=crate::storage::data_dir().join("producer-observation.json");let mut f=std::fs::File::create(path).unwrap();use std::io::Write;f.write_all(&serde_json::to_vec(receipt).unwrap()).unwrap();f.sync_all().unwrap();std::process::exit(77);};
            if mode=="writer-before" {super::UPLOAD_BEFORE_COMMIT.with(|s|*s.borrow_mut()=Some(Box::new(observe)));}else{super::UPLOAD_AFTER_COMMIT.with(|s|*s.borrow_mut()=Some(Box::new(observe)));}
            let _=super::router().with_state(state.clone()).oneshot(actual_upload_request(&state,&id,b"restart exact producer bytes","restart.bin")).await.unwrap();panic!("writer never reached deliberate exit point");
        }
        let expected:crate::storage::attachment_upload::Receipt=serde_json::from_slice(&std::fs::read(root.join("producer-observation.json")).unwrap()).unwrap();assert_eq!(expected.intent.request.operation_id,id);assert_eq!(std::fs::read(root.join(&expected.intent.storage_path)).unwrap(),b"restart exact producer bytes");
        let mut conn=state.db.acquire().await.unwrap();let before=crate::storage::attachment_upload::receipt(&mut conn,&id).await.unwrap();drop(conn);
        if mode=="recover-before" {assert!(before.is_none());assert_eq!(crate::storage::attachment_upload::recover_in(&root,&state.db).await.unwrap(),0);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachments").fetch_one(&state.db).await.unwrap(),0);let mut conn=state.db.acquire().await.unwrap();let stages=crate::storage::attachment_upload::inspect_namespace(&root,&mut conn).await.unwrap();assert_eq!(stages.len(),1);assert_eq!(stages[0].class,crate::storage::attachment_upload::UploadClass::SealedUncommitted);assert_eq!(std::fs::read(root.join(&expected.intent.storage_path)).unwrap(),b"restart exact producer bytes");return;}
        assert_eq!(before,Some(expected.clone()));let raw_before:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&id).fetch_one(&state.db).await.unwrap();let proof_before=std::fs::read(root.join(".attachment-upload-journal").join(&expected.intent.attempt_id).join("install-proof.json")).unwrap();
        let repairs=if mode=="recover-pending"{std::fs::remove_file(root.join(&expected.intent.storage_path)).unwrap();1}else{0};assert_eq!(crate::storage::attachment_upload::recover_in(&root,&state.db).await.unwrap(),repairs);assert_eq!(crate::storage::attachment_upload::recover_in(&root,&state.db).await.unwrap(),0);assert!(crate::storage::attachment_upload::verify(&root,&expected,false).unwrap());
        let response=super::router().with_state(state.clone()).oneshot(actual_upload_request(&state,&id,b"restart exact producer bytes","restart.bin")).await.unwrap();assert_eq!(response.status(),axum::http::StatusCode::CREATED);let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),10000).await.unwrap()).unwrap();assert_eq!(value["id"],expected.intent.asset_id);
        let raw_after:String=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&id).fetch_one(&state.db).await.unwrap();assert_eq!(raw_before,raw_after);assert_eq!(proof_before,std::fs::read(root.join(".attachment-upload-journal").join(&expected.intent.attempt_id).join("install-proof.json")).unwrap());for t in ["attachments","attachment_upload_receipts"]{assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT count(*) FROM {t}")).fetch_one(&state.db).await.unwrap(),1);}
    }
    #[cfg(target_os="linux")]
    #[test]
    fn direct_upload_process_restart_three_real_writer_recovery_pairs(){
        for (writer,recovery) in [("writer-before","recover-before"),("writer-after","recover-after"),("writer-after","recover-pending")] {
            let root=std::env::temp_dir().join(format!("upload-restart-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&root,std::fs::Permissions::from_mode(0o700)).unwrap();let id=uuid::Uuid::new_v4().to_string();
            for (phase,code) in [(writer,77),(recovery,0)] {let output=std::process::Command::new(std::env::current_exe().unwrap()).arg("--exact").arg("spaces::attachments::tests::direct_upload_restart_child").arg("--nocapture").env("DATA_DIR",&root).env("STARLANE_UPLOAD_RESTART_PHASE",phase).env("STARLANE_UPLOAD_RESTART_OPERATION",&id).output().unwrap();assert_eq!(output.status.code(),Some(code),"phase {phase}: {} {}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));if code==0{assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed;"));}println!("actual upload {phase} exited {code}: {id}");}
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[cfg(target_os="linux")]
    #[tokio::test]
    async fn direct_upload_concurrent_same_operation_exact_and_mismatch(){
        let case="direct_upload_concurrent_same_operation_exact_and_mismatch";if isolated_upload_case(case){return;}
        use tower::ServiceExt;let state=persistent_upload_fixture(true).await;let app=super::router().with_state(state.clone());let id=uuid::Uuid::new_v4().to_string();let (a,b)=tokio::join!(app.clone().oneshot(actual_upload_request(&state,&id,b"same concurrent bytes","same.bin")),app.clone().oneshot(actual_upload_request(&state,&id,b"same concurrent bytes","same.bin")));let a=a.unwrap();let b=b.unwrap();assert_eq!(a.status(),axum::http::StatusCode::CREATED);assert_eq!(b.status(),axum::http::StatusCode::CREATED);let aa:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(a.into_body(),10000).await.unwrap()).unwrap();let bb:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(b.into_body(),10000).await.unwrap()).unwrap();assert_eq!(aa,bb);let changed=app.oneshot(actual_upload_request(&state,&id,b"changed concurrent bytes","same.bin")).await.unwrap();assert_eq!(changed.status(),axum::http::StatusCode::CONFLICT);for t in ["attachments","attachment_upload_receipts"]{assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT count(*) FROM {t}")).fetch_one(&state.db).await.unwrap(),1);}
    }

    #[cfg(target_os="linux")]
    #[tokio::test]
    async fn direct_upload_real_request_abort_retains_only_private_partial_stage(){
        let case="direct_upload_real_request_abort_retains_only_private_partial_stage";if isolated_upload_case(case){return;}
        use tower::ServiceExt;use futures::{stream,StreamExt};let state=upload_fixture().await;let id=uuid::Uuid::new_v4().to_string();let mut request=actual_upload_request(&state,&id,b"","cancel.bin");let mut first=b"--upload\r\nContent-Disposition: form-data; name=\"file\"; filename=\"cancel.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec();first.extend_from_slice(&[0x41;65536]);*request.body_mut()=axum::body::Body::from_stream(stream::once(async move{Ok::<_,std::io::Error>(axum::body::Bytes::from(first))}).chain(stream::pending()));let task=tokio::spawn(super::router().with_state(state.clone()).oneshot(request));let root=crate::storage::data_dir();let mut held=None;for _ in 0..500{if let Ok(entries)=std::fs::read_dir(root.join(".attachment-upload-journal")){for entry in entries{let p=entry.unwrap().path().join("payload.part");if std::fs::metadata(&p).is_ok_and(|m|m.len()>=65536){held=Some(p);break;}}}if held.is_some(){break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}let payload=held.expect("real route never wrote first bounded chunk");task.abort();assert!(task.await.unwrap_err().is_cancelled());tokio::time::sleep(std::time::Duration::from_millis(25)).await;assert_eq!(std::fs::read(&payload).unwrap(),vec![0x41;65536]);assert!(!payload.parent().unwrap().join("seal.json").exists());for table in ["attachments","attachment_upload_receipts"]{assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT count(*) FROM {table}")).fetch_one(&state.db).await.unwrap(),0);}let mut conn=state.db.acquire().await.unwrap();let stages=crate::storage::attachment_upload::inspect_namespace(&root,&mut conn).await.unwrap();assert_eq!(stages.len(),1);assert_eq!(stages[0].class,crate::storage::attachment_upload::UploadClass::Unsealed);
    }
    #[cfg(target_os="linux")]
    #[tokio::test]
    #[ignore="requires owned private64MiB kernel tmpfs, explicit DATA_DIR and capacity assertion"]
    async fn direct_upload_real_kernel_enospc_retains_partial_without_sql_receipt(){
        use tower::ServiceExt;use futures::{stream,StreamExt};let root=crate::storage::data_dir();assert_eq!(root,std::path::PathBuf::from(std::env::var("STARLANE_UPLOAD_CAPACITY_ROOT").unwrap()));assert!(root.is_absolute());let c=std::ffi::CString::new(root.as_os_str().as_encoded_bytes()).unwrap();let mut stat=std::mem::MaybeUninit::<libc::statfs>::uninit();assert_eq!(unsafe{libc::statfs(c.as_ptr(),stat.as_mut_ptr())},0);let stat=unsafe{stat.assume_init()};assert_eq!(stat.f_type,0x01021994);assert_eq!((stat.f_blocks as u64)*(stat.f_bsize as u64),67108864);
        let state=upload_fixture().await;let id=uuid::Uuid::new_v4().to_string();let mut request=actual_upload_request(&state,&id,b"","capacity.bin");let prefix=axum::body::Bytes::from_static(b"--upload\r\nContent-Disposition: form-data; name=\"file\"; filename=\"capacity.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n");let frames=stream::once(async{Ok::<_,std::io::Error>(prefix)}).chain(stream::iter((0..1120).map(|_|Ok::<_,std::io::Error>(axum::body::Bytes::from(vec![0x42;65536]))))).chain(stream::once(async{Ok::<_,std::io::Error>(axum::body::Bytes::from_static(b"\r\n--upload--\r\n"))}));*request.body_mut()=axum::body::Body::from_stream(frames);let response=super::router().with_state(state.clone()).oneshot(request).await.unwrap();assert_eq!(response.status(),axum::http::StatusCode::INTERNAL_SERVER_ERROR);for table in ["attachments","attachment_upload_receipts"]{assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT count(*) FROM {table}")).fetch_one(&state.db).await.unwrap(),0);}let stages=std::fs::read_dir(root.join(".attachment-upload-journal")).unwrap().map(|e|e.unwrap().path()).collect::<Vec<_>>();assert_eq!(stages.len(),1);let length=std::fs::metadata(stages[0].join("payload.part")).unwrap().len();assert!(length>0&&length<70*1024*1024);assert!(!stages[0].join("seal.json").exists());assert!(!stages[0].join("install-proof.json").exists());use std::os::unix::fs::PermissionsExt;assert_eq!(std::fs::metadata(stages[0].join("payload.part")).unwrap().permissions().mode()&0o777,0o600);println!("actual route kernelENOSPC private capacity67108864 retained_partial_bytes={length}, noSQL/noSeal/noProof");
    }

    #[cfg(target_os="linux")]
    #[tokio::test]
    async fn direct_upload_backup_producer_child(){
        let Ok(expected_root)=std::env::var("STARLANE_UPLOAD_PRODUCER_ROOT") else{return;};let root=crate::storage::data_dir();assert_eq!(root,std::path::PathBuf::from(expected_root));assert!(root.is_absolute());let record=std::path::PathBuf::from(std::env::var("STARLANE_UPLOAD_PRODUCER_RECORD").unwrap());assert!(record.is_absolute()&&!record.starts_with(&root));let id=std::env::var("STARLANE_UPLOAD_PRODUCER_OPERATION").unwrap();let state=persistent_upload_fixture(true).await;
        use tower::ServiceExt;let response=crate::api_routes().with_state(state.clone()).oneshot(actual_upload_request(&state,&id,b"\0genuine original immutable upload\xff\r\nbytes","producer.bin")).await.unwrap();assert_eq!(response.status(),axum::http::StatusCode::CREATED);let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),10000).await.unwrap()).unwrap();let mut conn=state.db.acquire().await.unwrap();let inspection=crate::storage::attachment_upload::inspect(&root,&mut conn,&id).await.unwrap();assert_eq!(inspection.class,crate::storage::attachment_upload::UploadClass::CommittedInstalled);let evidence=serde_json::json!({"operation_id":id,"attempt_id":inspection.receipt.intent.attempt_id,"response":value,"original_identity":inspection.original_identity,"required_group":inspection.required_group,"raw_receipt":String::from_utf8(inspection.raw_receipt).unwrap(),"producer":"actual JWT create/draft/source_digest-save/publish + multipart route"});std::fs::write(record,serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();drop(conn);state.db.close().await;
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn direct_upload_snapshot_copies_are_evidence_only_and_tampering_refuses() {
        let case = "direct_upload_snapshot_copies_are_evidence_only_and_tampering_refuses";
        if isolated_upload_case(case) { return; }
        use tower::ServiceExt;
        use std::os::unix::fs::{DirBuilderExt, MetadataExt};
        let state = persistent_upload_fixture(true).await;
        let root = crate::storage::data_dir();
        let id = uuid::Uuid::new_v4().to_string();
        let response = super::router().with_state(state.clone()).oneshot(actual_upload_request(&state, &id, b"snapshot exact bytes", "copy.bin")).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::CREATED);
        let mut conn = state.db.acquire().await.unwrap();
        let original = crate::storage::attachment_upload::inspect(&root, &mut conn, &id).await.unwrap();
        let copy = root.parent().unwrap().join(format!("upload-copy-{}", uuid::Uuid::new_v4()));
        std::fs::DirBuilder::new().mode(0o700).create(&copy).unwrap();
        fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new().recursive(true).mode(0o700).create(to).unwrap();
            for entry in std::fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                let next = to.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() { copy_tree(&entry.path(), &next); }
                else { assert!(entry.file_type().unwrap().is_file()); std::fs::copy(entry.path(), next).unwrap(); }
            }
        }
        copy_tree(&root.join(".attachment-upload-journal"), &copy.join(".attachment-upload-journal"));
        let canonical = copy.join(&original.receipt.intent.storage_path);
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(canonical.parent().unwrap()).unwrap();
        std::fs::copy(root.join(&original.receipt.intent.storage_path), &canonical).unwrap();
        assert_eq!(std::fs::metadata(&canonical).unwrap().nlink(), 1);
        assert!(crate::storage::attachment_upload::inspect(&copy, &mut conn, &id).await.is_err());
        assert!(crate::storage::attachment_upload::verify(&copy, &original.receipt, false).is_err());
        let captured = crate::storage::attachment_upload::inspect_restoration(&copy, &mut conn, &id).await.unwrap();
        assert_eq!(captured.original_proof, original.original_proof);
        assert_eq!(captured.raw_receipt, original.raw_receipt);
        assert_eq!(captured.original_identity, original.original_identity);
        assert_eq!(captured.required_group, original.required_group);
        // Equal bytes and a reconstructed hardlink group are still not original ownership.
        let copied_anchor = copy.join(&original.required_group[0]);
        std::fs::remove_file(&canonical).unwrap();
        std::fs::hard_link(&copied_anchor, &canonical).unwrap();
        assert_eq!(std::fs::metadata(&canonical).unwrap().nlink(), 2);
        assert!(crate::storage::attachment_upload::inspect(&copy, &mut conn, &id).await.is_err());
        assert!(crate::storage::attachment_upload::verify(&copy, &original.receipt, false).is_err());
        assert_eq!(crate::storage::attachment_upload::inspect_restoration(&copy, &mut conn, &id).await.unwrap().original_proof, original.original_proof);
        std::fs::remove_file(&canonical).unwrap();
        std::fs::copy(&copied_anchor, &canonical).unwrap();
        let bytes = std::fs::read(&canonical).unwrap();
        std::fs::write(&canonical, vec![b'X'; bytes.len()]).unwrap();
        assert!(crate::storage::attachment_upload::inspect_restoration(&copy, &mut conn, &id).await.is_err());
        std::fs::write(&canonical, &bytes).unwrap();
        let anchor = copy.join(&original.required_group[0]);
        std::fs::write(&anchor, vec![b'Y'; bytes.len()]).unwrap();
        assert!(crate::storage::attachment_upload::inspect_restoration(&copy, &mut conn, &id).await.is_err());
        assert!(crate::storage::attachment_upload::verify(&root, &original.receipt, false).unwrap());
        std::fs::remove_dir_all(copy).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn direct_upload_deleted_space_historical_recovery_never_reinstalls() {
        let case = "direct_upload_deleted_space_historical_recovery_never_reinstalls";
        if isolated_upload_case(case) { return; }
        use tower::ServiceExt;
        let state = persistent_upload_fixture(true).await;
        let id = uuid::Uuid::new_v4().to_string();
        let response = super::router().with_state(state.clone()).oneshot(actual_upload_request(&state, &id, b"retained historical bytes", "history.bin")).await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::CREATED);
        let mut conn = state.db.acquire().await.unwrap();
        let receipt = crate::storage::attachment_upload::receipt(&mut conn, &id).await.unwrap().unwrap();
        drop(conn);
        upload_page_json(&state, "DELETE", "/spaces/space-a", serde_json::json!({})).await;
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM spaces WHERE id='space-a'").fetch_one(&state.db).await.unwrap(), 0);
        // Space deletion intentionally retains physical files. Remove only this fixture's
        // now-unassociated canonical to prove recovery cannot resurrect historical assets.
        let canonical = crate::storage::data_dir().join(&receipt.intent.storage_path);
        std::fs::remove_file(&canonical).unwrap();
        let anchor = crate::storage::data_dir().join(format!(".attachment-upload-journal/{}/payload.part", receipt.intent.attempt_id));
        let retained = std::fs::read(&anchor).unwrap();
        let raw_before: String = sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&id).fetch_one(&state.db).await.unwrap();
        state.db.close().await;
        let reopened = persistent_upload_fixture(false).await;
        assert_eq!(crate::storage::attachment_upload::recover_in(&crate::storage::data_dir(), &reopened.db).await.unwrap(), 0);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments").fetch_one(&reopened.db).await.unwrap(), 0);
        let mut conn = reopened.db.acquire().await.unwrap();
        assert_eq!(crate::storage::attachment_upload::receipt(&mut conn, &id).await.unwrap().unwrap(), receipt);
        let raw_after: String = sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE operation_id=$1").bind(&id).fetch_one(&reopened.db).await.unwrap();
        assert_eq!(raw_after, raw_before);
        assert_eq!(std::fs::read(anchor).unwrap(), retained);
        assert!(!crate::storage::data_dir().join(&receipt.intent.storage_path).exists());
    }

    #[cfg(target_os="linux")]
    #[tokio::test]
    async fn direct_upload_valid_four_mib_single_body_frame(){let case="direct_upload_valid_four_mib_single_body_frame";if isolated_upload_case(case){return;}use tower::ServiceExt;let state=upload_fixture().await;let id=uuid::Uuid::new_v4().to_string();let bytes=vec![0x51;4*1024*1024+11];let response=super::router().with_state(state.clone()).oneshot(actual_upload_request(&state,&id,&bytes,"single.bin")).await.unwrap();assert_eq!(response.status(),axum::http::StatusCode::CREATED);let mut conn=state.db.acquire().await.unwrap();let receipt=crate::storage::attachment_upload::receipt(&mut conn,&id).await.unwrap().unwrap();assert_eq!(std::fs::read(crate::storage::data_dir().join(&receipt.intent.storage_path)).unwrap(),bytes);assert!(crate::storage::attachment_upload::verify(&crate::storage::data_dir(),&receipt,false).unwrap());}

    #[cfg(target_os="linux")]
    #[tokio::test(flavor="current_thread")]
    #[ignore = "explicit genuine restored root and original producer record required"]
    async fn direct_upload_restored_application_child(){
        use tower::ServiceExt;
        let expected_root=std::env::var("STARLANE_UPLOAD_RESTORED_ROOT").expect("explicit restored root required");
        let root=crate::storage::data_dir();assert_eq!(root,std::path::PathBuf::from(expected_root));assert!(root.is_absolute());
        let record:serde_json::Value=serde_json::from_slice(&std::fs::read(std::env::var("STARLANE_UPLOAD_RESTORED_RECORD").expect("explicit producer record required")).unwrap()).unwrap();
        crate::storage::restoration::bootstrap_gate(&root).unwrap();
        let state=persistent_upload_fixture(false).await;
        crate::storage::restoration::activate_in(&root,&state.db).await.unwrap();
        crate::storage::relocation::recover(&state.db).await.unwrap();
        crate::storage::journal::recover(&state.db).await.unwrap();
        crate::storage::canonical_rewrite::recover(&state.db).await.unwrap();
        crate::storage::publication::recover(&state.db).await.unwrap();
        assert_eq!(crate::storage::attachment_upload::recover_in(&root,&state.db).await.unwrap(),0);
        let id=record["operation_id"].as_str().unwrap();let expected=b"\0genuine original immutable upload\xff\r\nbytes";
        let mut conn=state.db.acquire().await.unwrap();let inspected=crate::storage::attachment_upload::inspect(&root,&mut conn,id).await.unwrap();
        assert_eq!(inspected.class,crate::storage::attachment_upload::UploadClass::CommittedInstalled);
        assert_eq!(String::from_utf8(inspected.raw_receipt.clone()).unwrap(),record["raw_receipt"].as_str().unwrap());
        assert_eq!(inspected.receipt.intent.attempt_id,record["attempt_id"].as_str().unwrap());
        assert_eq!(std::fs::read(root.join(&inspected.receipt.intent.storage_path)).unwrap(),expected);
        let old=(record["original_identity"][0].as_u64().unwrap(),record["original_identity"][1].as_u64().unwrap());assert_eq!(inspected.original_identity,old);
        drop(conn);
        let app=crate::api_routes().with_state(state.clone());
        let retried=app.clone().oneshot(actual_upload_request(&state,id,expected,"producer.bin")).await.unwrap();assert_eq!(retried.status(),axum::http::StatusCode::CREATED);
        let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(retried.into_body(),10000).await.unwrap()).unwrap();assert_eq!(value,record["response"]);
        let url=value["url"].as_str().unwrap().strip_prefix("/api/v1").expect("production attachment API prefix");
        for (user,allowed) in [(Some("owner-a"),true),(Some("viewer-a"),true),(None,false)] {
            let mut builder=axum::http::Request::builder().uri(url);
            if let Some(user)=user {let claims=auth(user).claims;let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();builder=builder.header("authorization",format!("Bearer {token}"));}
            let response=app.clone().oneshot(builder.body(axum::body::Body::empty()).unwrap()).await.unwrap();
            if allowed {assert_eq!(response.status(),axum::http::StatusCode::OK);assert_eq!(&axum::body::to_bytes(response.into_body(),10000).await.unwrap()[..],expected);}else{assert!(!response.status().is_success());}
        }
        let receipt=inspected.receipt;
        std::fs::remove_file(root.join(&receipt.intent.storage_path)).unwrap();
        assert_eq!(crate::storage::attachment_upload::recover_in(&root,&state.db).await.unwrap(),1);
        assert!(crate::storage::attachment_upload::verify(&root,&receipt,false).unwrap());
        assert_eq!(std::fs::read(root.join(&receipt.intent.storage_path)).unwrap(),expected);
        for t in ["attachments","attachment_upload_receipts"] {assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT count(*) FROM {t}")).fetch_one(&state.db).await.unwrap(),1);}
        let changed=app.clone().oneshot(actual_upload_request(&state,id,b"different retained operation bytes","producer.bin")).await.unwrap();assert_eq!(changed.status(),axum::http::StatusCode::CONFLICT);
        if std::env::var("STARLANE_UPLOAD_RESTORED_REVOKE").as_deref()==Ok("1") {
            sqlx::query("DELETE FROM space_members WHERE space_id='space-a' AND user_id='viewer-a'").execute(&state.db).await.unwrap();
            let claims=auth("viewer-a").claims;let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();
            let denied=app.clone().oneshot(axum::http::Request::builder().uri(url).header("authorization",format!("Bearer {token}")).body(axum::body::Body::empty()).unwrap()).await.unwrap();
            assert!(matches!(denied.status(),axum::http::StatusCode::UNAUTHORIZED|axum::http::StatusCode::FORBIDDEN|axum::http::StatusCode::NOT_FOUND));
            println!("genuine restored upload current viewer revocation denied");
        }
        state.db.close().await;
        println!("genuine restored upload original evidence exact retry private/current downloads and canonical recovery verified");
    }

}

#[cfg(all(test,target_os="linux"))]
thread_local! { static REPLACEMENT_AFTER_COMMIT: std::cell::RefCell<Option<Box<dyn FnOnce(&crate::storage::attachment_upload::Receipt, &crate::storage::attachment_versions::ReplacementChange)>>> = std::cell::RefCell::new(None); }

async fn attachment_family_id(conn:&mut sqlx::AnyConnection,id:&str)->Result<Option<String>,AppError>{
    #[cfg(target_os="linux")] {storage::attachment_versions::logical_attachment_id(conn,id).await.map_err(AppError::Conflict)}
    #[cfg(not(target_os="linux"))] {let _=(conn,id);Ok(None)}
}

async fn cleanup_interrupted_attachment(State(state):State<AppState>,auth:AuthUser,Path((space,document,attempt)):Path<(String,String,String)>,axum::Json(request):axum::Json<crate::storage::attachment_upload::admission::committed_erasure::PendingRequest>)->Result<axum::Json<crate::storage::attachment_upload::admission::committed_erasure::PendingOutcome>,AppError>{
 crate::storage::attachment_upload::admission::committed_erasure::cleanup_pending(&state.db,&auth,&space,&document,&attempt,&request).await.map(axum::Json)
}

async fn discover_interrupted_attachments(State(state):State<AppState>,auth:AuthUser,Path((space,document)):Path<(String,String)>)->Result<axum::Json<crate::storage::attachment_upload::admission::committed_erasure::PendingDiscovery>,AppError>{
 crate::storage::attachment_upload::admission::committed_erasure::discover_pending(&state.db,&auth,&space,&document).await.map(axum::Json)
}

// Genuine legacy handler is reachable only inside an explicitly owned test listener.
// No production route, native creator or copied receipt grant is added.
#[cfg(test)]
pub(crate) fn verification_legacy_fixture_router()->Router<AppState>{Router::new().route("/owned-legacy-fixture/{space_id}/{doc_id}",axum::routing::post(upload_attachment_legacy))}
