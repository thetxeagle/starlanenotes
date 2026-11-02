//! Server-side copies create independent target custody, never source grants.
use super::*;
use crate::spaces;
use crate::storage::{attachment_upload as custody, attachment_versions as versions};
use futures::StreamExt;
use serde::{Deserialize,Serialize};
use sha2::{Digest,Sha256};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CopyRequest {
    schema_version:u8,
    operation_id:String,
    source_document_id:String,
    version_id:String,
    expected_source_version:Option<versions::VersionRecord>,
    expected_legacy_source:Option<LegacySource>,
}
#[derive(Clone,Debug,Deserialize,Serialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct LegacySource {
    kind:String,
    asset_id:String,
    space_id:String,
    document_id:String,
    original_name:String,
    mime_type:String,
    size_bytes:i64,
    created_at:String,
    storage_path:String,
    binary_sha256:String,
}
fn unavailable()->AppError{attachment_unavailable()}
fn digest(value:&impl Serialize)->Result<String,AppError>{Ok(hex::encode(Sha256::digest(serde_json::to_vec(value).map_err(AppError::internal)?)))}
enum SourceBinding {Registered(versions::CurrentVersionBinding),Legacy(custody::HeldVersionPath)}
pub(super) struct CopyGuard {
    space:String,doc:String,asset:String,length:i64,hash:String,
    audience:spaces::asset_audiences::ReadProof,
    proofs:Vec<spaces::published_source::SourceProof>,
    binding:SourceBinding,
    path:std::path::PathBuf,
    snapshot:std::sync::Mutex<download::Snapshot>,
}
impl CopyGuard {
    pub(super) async fn verify(&self,conn:&mut sqlx::AnyConnection,auth:&AuthUser)->Result<(),AppError>{
        spaces::published_source::verify_source(conn,&self.proofs).await.map_err(|_|unavailable())?;
        spaces::asset_audiences::verify_read(conn,auth,&self.space,&self.doc,&self.asset,&self.audience).await?;
        spaces::access::require_current_human_read(conn,auth).await?;
        self.fence()
    }
    pub(super) fn fence(&self)->Result<(),AppError>{
        let bound=||match &self.binding{SourceBinding::Registered(b)=>b.fence_files().map_err(|_|unavailable()),SourceBinding::Legacy(b)=>b.fence(&storage::data_dir()).map_err(|_|unavailable())};
        bound()?;
        let mut snapshot=self.snapshot.lock().map_err(|_|unavailable())?;
        if !snapshot.matches_source_and_rewind(&self.path).map_err(download_error)?||!snapshot.matches_sha256(self.length as u64,&self.hash).map_err(download_error)?{return Err(unavailable())}
        verify_audience_snapshot(&snapshot,&self.audience)?;
        bound()
    }
    pub(super) fn matches_receipt(&self,receipt:&custody::Receipt)->Result<(),AppError>{
        if receipt.intent.byte_length!=self.length||receipt.intent.binary_sha256!=self.hash{return Err(AppError::Conflict("Copied source bytes differ from original selection".into()))}Ok(())
    }
}
async fn source_proofs(conn:&mut sqlx::AnyConnection,auth:&AuthUser,space:&str,doc:&str,asset:&str)->Result<Vec<spaces::published_source::SourceProof>,AppError>{
    let page=spaces::asset_audiences::asset_page(conn,auth,space,doc,asset).await?;
    let mut proofs=spaces::published_source::capture_source(conn,auth,space,doc,&page.ancestor_ids).await?;
    // Copying onto the same page adds exactly a new asset. Retain all authority
    // evidence, but bind source association to this selected asset, not its peers.
    proofs.retain(|p|p.query!="SELECT * FROM attachments WHERE document_id=$1");
    let query="SELECT * FROM attachments WHERE id=$1 AND space_id=$2 AND document_id=$3";
    let rows=spaces::document_deletion::rows(conn,query,&[asset,space,doc]).await.map_err(AppError::internal)?;
    if rows.len()!=1{return Err(unavailable())}
    proofs.push(spaces::published_source::SourceProof{query:query.into(),parameters:vec![asset.into(),space.into(),doc.into()],rows});Ok(proofs)
}
async fn capture_legacy(auth:&AuthUser,conn:&mut sqlx::AnyConnection,space:&str,doc:&str,asset:&str,maximum:usize)->Result<(LegacySource,download::Snapshot,CopyGuard),AppError>{
    if !safe_component(asset)||asset.len()>255{return Err(unavailable())}
    let audience=spaces::asset_audiences::read(conn,auth,space,doc,asset).await?;
    let mut proofs=source_proofs(conn,auth,space,doc,asset).await?;
    let (registered,absence)=versions::capture_current_asset(&storage::data_dir(),conn,asset).await.map_err(|_|unavailable())?;
    if registered.is_some(){return Err(AppError::Conflict("Registered asset requires its exact immutable version".into()))}
    proofs.push(absence);
    let row:(String,String,i64,String,String)=sqlx::query_as("SELECT original_name,mime_type,size_bytes,created_at,storage_path FROM attachments WHERE id=$1 AND document_id=$2 AND space_id=$3")
        .bind(asset).bind(doc).bind(space).fetch_one(&mut *conn).await.map_err(|_|unavailable())?;
    if row.2<0||row.2 as u64>maximum as u64{return Err(AppError::BadRequest("Copy exceeds upload size limit".into()))}
    let full=checked_attachment_path(conn,space,doc,&row.4).await?;
    let root=storage::data_dir();let relative=row.4.clone();
    let held=tokio::task::spawn_blocking(move||custody::HeldVersionPath::open(&root,&relative)).await.map_err(AppError::internal)?.map_err(|_|unavailable())?;
    let snapshot=download::capture_bounded(&full,maximum as u64).await.map_err(download_error)?;
    if snapshot.length()!=row.2 as u64{return Err(unavailable())}
    verify_audience_snapshot(&snapshot,&audience)?;
    let hash=snapshot.sha256().map_err(download_error)?;
    let selected=LegacySource{kind:"legacy_snapshot_v1".into(),asset_id:asset.into(),space_id:space.into(),document_id:doc.into(),original_name:row.0,mime_type:row.1,size_bytes:row.2,created_at:row.3,storage_path:row.4,binary_sha256:hash.clone()};
    let guard=CopyGuard{space:space.into(),doc:doc.into(),asset:asset.into(),length:row.2,hash,audience,proofs,binding:SourceBinding::Legacy(held),path:full,snapshot:std::sync::Mutex::new(snapshot.copy_handle().map_err(download_error)?)};
    guard.verify(conn,auth).await?;Ok((selected,snapshot,guard))
}
pub(super) async fn legacy_source(State(state):State<AppState>,auth:AuthUser,Path((space,doc,asset)):Path<(String,String,String)>)->Result<Response,AppError>{
    let maximum=max_upload_size_bytes(&state).await?;
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    let (selected,_snapshot,guard)=capture_legacy(&auth,&mut conn,&space,&doc,&asset,maximum).await?;
    guard.verify(&mut conn,&auth).await?;Ok(([(header::CACHE_CONTROL,"private, no-store")],Json(selected)).into_response())
}
pub(super) async fn copy_attachment(State(state):State<AppState>,auth:AuthUser,Path((space,doc,draft)):Path<(String,String,String)>,Json(body):Json<CopyRequest>)->Result<(StatusCode,Json<AttachmentResponse>),AppError>{
    let canonical=|s:&str|Uuid::parse_str(s).is_ok_and(|id|id.to_string()==s);
    if body.schema_version!=1||!canonical(&body.operation_id)||!canonical(&draft)||body.source_document_id.is_empty(){return Err(AppError::BadRequest("Invalid original attachment copy request".into()))}
    let (kind,source_digest,hash,length,name,mime)=match (&body.expected_source_version,&body.expected_legacy_source){
        (Some(v),None) if canonical(&body.version_id)&&v.version_id==body.version_id=>(None,digest(v)?,v.binary_sha256.clone(),v.byte_length,v.original_name.clone(),v.mime_type.clone()),
        (None,Some(v)) if v.kind=="legacy_snapshot_v1"&&v.asset_id==body.version_id&&v.space_id==space&&v.document_id==body.source_document_id=>(Some(v.kind.clone()),digest(v)?,v.binary_sha256.clone(),v.size_bytes,v.original_name.clone(),v.mime_type.clone()),
        _=>return Err(AppError::BadRequest("Exactly one original attachment copy source is required".into())),
    };
    let witness=custody::CopySourceScope{schema_version:1,source_kind:kind,source_space_id:space.clone(),source_document_id:body.source_document_id.clone(),version_id:body.version_id.clone(),expected_source_version_sha256:source_digest,binary_sha256:hash.clone(),byte_length:length};
    let mut headers=axum::http::HeaderMap::new();headers.insert("x-starlane-upload-operation",body.operation_id.parse().map_err(|_|unavailable())?);headers.insert("x-starlane-upload-draft",draft.parse().map_err(|_|unavailable())?);
    let maximum=max_upload_size_bytes(&state).await?;
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    let mut scope=spaces::asset_audiences::capture_upload_scope(&mut conn,&auth,&space,&doc,&headers).await?.ok_or_else(unavailable)?;
    scope.copy_source=Some(witness);scope.validate().map_err(|_|unavailable())?;
    validate_attachment_upload_field(Some("file"),Some(&name))?;
    let request=custody::UploadRequest{operation_id:body.operation_id.clone(),origin:state.base_url.clone(),actor_id:auth.claims.sub.clone(),space_id:space.clone(),document_id:doc.clone(),original_name:name,mime_type:mime,draft_scope:Some(scope)};
    if let Some(prior)=custody::receipt(&mut conn,&body.operation_id).await.map_err(AppError::Internal)?{
        if prior.intent.request!=request{return Err(AppError::Conflict("Completed copy original request differs".into()))}
        drop(conn);return completed_upload_ack(&state,&auth,prior).await;
    }
    if length<0||length as u64>maximum as u64{return Err(AppError::BadRequest("Copy exceeds upload size limit".into()))}
    let (snapshot,guard)=if let Some(expected)=body.expected_source_version{
        let audience=spaces::asset_audiences::read(&mut conn,&auth,&space,&body.source_document_id,&body.version_id).await?;
        let mut proofs=source_proofs(&mut conn,&auth,&space,&body.source_document_id,&body.version_id).await?;
        let (version,mut binding,family)=versions::capture_historical_version(&storage::data_dir(),&mut conn,&expected.logical_attachment_id,&body.version_id).await.map_err(|_|unavailable())?;
        if version!=expected||binding.space_id!=space||binding.document_id!=body.source_document_id||binding.asset_id!=body.version_id{return Err(unavailable())}
        proofs.push(family);binding.append_sql_proofs(&mut proofs);
        let full=checked_attachment_path(&mut conn,&space,&body.source_document_id,&binding.storage_path).await?;
        let snapshot=download::capture_bounded(&full,maximum as u64).await.map_err(download_error)?;
        if snapshot.length()!=length as u64||!snapshot.matches_sha256(length as u64,&hash).map_err(download_error)?{return Err(unavailable())}
        let guard=CopyGuard{space:space.clone(),doc:body.source_document_id,asset:body.version_id,length,hash,audience,proofs,binding:SourceBinding::Registered(binding),path:full,snapshot:std::sync::Mutex::new(snapshot.copy_handle().map_err(download_error)?)};
        guard.verify(&mut conn,&auth).await?;(snapshot,guard)
    }else{
        let (actual,snapshot,guard)=capture_legacy(&auth,&mut conn,&space,&body.source_document_id,&body.version_id,maximum).await?;
        if Some(actual)!=body.expected_legacy_source{return Err(AppError::Conflict("Original legacy copy source differs".into()))}(snapshot,guard)
    };
    spaces::asset_audiences::verify_upload_scope(&mut conn,&auth,&request.space_id,&doc,request.draft_scope.as_ref()).await?;drop(conn);
    let stream=snapshot.into_body().into_data_stream().map(|chunk|chunk.map_err(|_|AppError::BadRequest("Copied source stream unavailable".into())));
    upload_content(state,auth,headers,request,maximum,Box::pin(stream),Some(&guard)).await
}
