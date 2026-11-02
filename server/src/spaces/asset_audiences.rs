//! Exact-version attachment audiences. Retained provenance is not a current grant.
use super::{access, draft_save, published_source};
use crate::{auth::{middleware::AuthUser, routes::AppError}, storage};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::AnyConnection;
use std::collections::BTreeSet;

fn unavailable() -> AppError { AppError::NotFound("Attachment unavailable".into()) }
fn changed() -> AppError { AppError::Conflict("Attachment audience changed; retain the original operation and review its draft".into()) }
async fn rows(c: &mut AnyConnection, q: &str, args: &[&str]) -> Result<Vec<Value>, AppError> {
    super::document_deletion::rows(c, q, args).await.map_err(AppError::internal)
}
fn text(v: &Value, key: &str) -> Result<String, AppError> { draft_save::string(v, key).map_err(|_| unavailable()) }
fn canonical(id: &str) -> bool { uuid::Uuid::parse_str(id).is_ok_and(|v| v.to_string() == id) }
pub(crate) fn opaque_replay_identity(id: &str) -> bool {
    let Some(suffix) = id.strip_prefix("asset-") else { return false };
    !suffix.is_empty()
        && id.len() <= 160
        && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct AudienceAcknowledgement {
    pub schema_version: u8,
    pub draft_id: String,
    pub draft_created_at: String,
}
#[cfg(target_os = "linux")]
pub(crate) fn acknowledgement(scope: Option<&storage::attachment_upload::DraftUploadScope>) -> Option<AudienceAcknowledgement> {
    scope.map(|s| AudienceAcknowledgement { schema_version: 2, draft_id: s.draft_id.clone(), draft_created_at: s.draft_created_at.clone() })
}
#[cfg(target_os = "linux")]
pub(crate) async fn capture_upload_scope(c: &mut AnyConnection, auth: &AuthUser, space: &str, doc: &str, headers: &axum::http::HeaderMap) -> Result<Option<storage::attachment_upload::DraftUploadScope>, AppError> {
    let Some(id) = headers.get("x-starlane-upload-draft") else { return Ok(None) };
    let id = id.to_str().map_err(|_| unavailable())?;
    if !canonical(id) { return Err(AppError::BadRequest("Original upload draft identity is invalid".into())) }
    access::resolve_draft_access(c, access::principal_from_auth(auth)?, space, doc, id, access::DraftOperation::Edit, access::Purpose::Routine).await?;
    let draft = draft_save::selected(c, space, doc, id).await?;
    let scope = storage::attachment_upload::DraftUploadScope { schema_version: 2, draft_id: id.into(), draft_created_at: text(&draft, "created_at")?,copy_source:None };
    verify_upload_scope(c, auth, space, doc, Some(&scope)).await?;
    Ok(Some(scope))
}
#[cfg(target_os = "linux")]
pub(crate) async fn verify_upload_scope(c: &mut AnyConnection, auth: &AuthUser, space: &str, doc: &str, scope: Option<&storage::attachment_upload::DraftUploadScope>) -> Result<(), AppError> {
    if let Some(scope) = scope {
        scope.validate().map_err(|_| unavailable())?;
        access::resolve_draft_access(c, access::principal_from_auth(auth)?, space, doc, &scope.draft_id, access::DraftOperation::Edit, access::Purpose::Routine).await?;
        let draft = draft_save::selected(c, space, doc, &scope.draft_id).await?;
        if draft["created_at"] != scope.draft_created_at { return Err(changed()) }
    } else {
        super::attachments::check_attachment_page(c, auth, space, doc, access::PageOperation::Edit).await?;
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReuseRequest {schema_version:u8,operation_id:String,version_id:String}
pub(crate) fn router()->axum::Router<crate::AppState>{
    axum::Router::new().route("/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/attachment-bindings",axum::routing::post(reuse))
}
#[cfg(target_os="linux")]
async fn reuse(axum::extract::State(state):axum::extract::State<crate::AppState>,auth:AuthUser,axum::extract::Path((space,doc,draft)):axum::extract::Path<(String,String,String)>,axum::Json(request):axum::Json<ReuseRequest>)->Result<axum::Json<Value>,AppError>{
    if request.schema_version!=1 || !canonical(&request.operation_id) || !canonical(&request.version_id) || !canonical(&draft) {return Err(changed())}
    let mut tx=state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx,&[&space]).await?;
    let mut headers=axum::http::HeaderMap::new();headers.insert("x-starlane-upload-draft",draft.parse().map_err(|_|changed())?);
    let scope=capture_upload_scope(&mut tx,&auth,&space,&doc,&headers).await?.ok_or_else(changed)?;
    let previous=rows(&mut tx,"SELECT * FROM attachment_audience_bindings WHERE source_operation_id=$1",&[&request.operation_id]).await?;
    let binding=if previous.is_empty(){
        let mut collection=capture_collection(&mut tx,&auth,&space).await?;
        let source=read(&mut tx,&auth,&space,&doc,&request.version_id).await?;
        let o=origin(&mut tx,&request.version_id).await?;
        if o["kind"]=="legacy_page_v1" {return Err(AppError::BadRequest("Page-owned files already keep their existing audience; use copy for a new private file".into()))}
        let binding=BindingEvidence{schema_version:1,asset_id:request.version_id.clone(),binding_kind:"draft".into(),target_id:draft.clone(),target_created_at:scope.draft_created_at.clone(),source_operation_id:request.operation_id.clone(),source:json!({"kind":"reuse","schema_version":1,"actor_id":auth.claims.sub,"operation_id":request.operation_id,"asset_id":request.version_id,"space_id":space,"document_id":doc,"draft_scope":scope,"original_source_proof_sha256":source.digest,"origin_sha256":o["scope_sha256"]})};
        let existing=rows(&mut tx,"SELECT * FROM attachment_audience_bindings WHERE asset_id=$1 AND binding_kind='draft' AND target_id=$2",&[&request.version_id,&draft]).await?;
        if !existing.is_empty(){return Err(AppError::Conflict("This version is already bound to the selected draft; retain the original binding operation".into()))}
        insert_binding(&mut tx,&binding,true).await?;
        let raw=serde_json::to_string(&binding).map_err(AppError::internal)?;
        let added=json!({"asset_id":binding.asset_id,"binding_kind":binding.binding_kind,"target_id":binding.target_id,"target_created_at":binding.target_created_at,"source_operation_id":binding.source_operation_id,"evidence_json":raw,"evidence_sha256":draft_save::digest(&serde_json::to_value(&binding).map_err(AppError::internal)?)?});
        let proof=collection.assets.iter_mut().find(|p|p.query.starts_with("SELECT * FROM attachment_audience_bindings ")).ok_or_else(changed)?;
        proof.rows.push(added);proof.rows.sort_by_cached_key(Value::to_string);
        verify_collection(&mut tx,&auth,&collection).await?;

        binding
    }else{
        if previous.len()!=1{return Err(changed())}
        let binding=parse_binding(&previous[0])?;
        if binding.source["kind"]!="reuse" || binding.source["actor_id"]!=auth.claims.sub || binding.asset_id!=request.version_id || binding.target_id!=draft || binding.target_created_at!=scope.draft_created_at || binding.source["space_id"]!=space || binding.source["document_id"]!=doc {return Err(changed())}
        binding
    };
    let o=origin(&mut tx,&request.version_id).await?;
    verify_draft_binding(&mut tx,&o,&binding).await?;
    verify_upload_scope(&mut tx,&auth,&space,&doc,Some(&scope)).await?;
    insert_binding(&mut tx,&binding,false).await?;
    let result=json!({"schema_version":1,"operation_id":request.operation_id,"version_id":request.version_id,"draft_id":draft,"draft_created_at":scope.draft_created_at,"binding_digest":draft_save::digest(&serde_json::to_value(&binding).map_err(AppError::internal)?)?});
    tx.commit().await.map_err(|_|AppError::Conflict("Reuse outcome is unknown; retain the original operation for retry".into()))?;
    Ok(axum::Json(result))
}
#[cfg(not(target_os="linux"))]
async fn reuse()->Result<axum::Json<Value>,AppError>{Err(AppError::BadRequest("Draft attachment reuse requires a supported server platform".into()))}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct BindingEvidence {
    schema_version: u8,
    asset_id: String,
    binding_kind: String,
    target_id: String,
    target_created_at: String,
    source_operation_id: String,
    source: Value,
}
fn binding_row(e:&BindingEvidence)->Result<Value,AppError>{
    Ok(json!({"asset_id":e.asset_id,"binding_kind":e.binding_kind,"target_id":e.target_id,"target_created_at":e.target_created_at,"source_operation_id":e.source_operation_id,"evidence_json":serde_json::to_string(e).map_err(AppError::internal)?,"evidence_sha256":draft_save::digest(&serde_json::to_value(e).map_err(AppError::internal)?)?}))
}
fn admit_expected_binding(proofs:&mut [published_source::SourceProof],e:&BindingEvidence)->Result<(),AppError>{
    let proof=proofs.iter_mut().find(|p|p.query.starts_with("SELECT * FROM attachment_audience_bindings ")).ok_or_else(changed)?;
    let row=binding_row(e)?;
    if !proof.rows.contains(&row){proof.rows.push(row);proof.rows.sort_by_cached_key(Value::to_string)}
    Ok(())
}

async fn insert_binding(c: &mut AnyConnection, e: &BindingEvidence, write: bool) -> Result<(), AppError> {
    let value = serde_json::to_value(e).map_err(AppError::internal)?;
    let raw = serde_json::to_string(e).map_err(AppError::internal)?;
    let digest = draft_save::digest(&value)?;
    if write { sqlx::query("INSERT INTO attachment_audience_bindings(asset_id,binding_kind,target_id,target_created_at,source_operation_id,evidence_json,evidence_sha256) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(asset_id,binding_kind,target_id) DO NOTHING")
        .bind(&e.asset_id).bind(&e.binding_kind).bind(&e.target_id).bind(&e.target_created_at).bind(&e.source_operation_id).bind(&raw).bind(&digest).execute(&mut *c).await.map_err(AppError::internal)?; }
    let actual = rows(c, "SELECT * FROM attachment_audience_bindings WHERE asset_id=$1 AND binding_kind=$2 AND target_id=$3", &[&e.asset_id, &e.binding_kind, &e.target_id]).await?;
    let expected = json!({"asset_id":e.asset_id,"binding_kind":e.binding_kind,"target_id":e.target_id,"target_created_at":e.target_created_at,"source_operation_id":e.source_operation_id,"evidence_json":raw,"evidence_sha256":digest});
    if actual != vec![expected] { return Err(changed()) }
    Ok(())
}
fn parse_binding(row: &Value) -> Result<BindingEvidence, AppError> {
    let raw = text(row, "evidence_json")?;
    if raw.len() > 65536 { return Err(unavailable()) }
    let e: BindingEvidence = serde_json::from_str(&raw).map_err(|_| unavailable())?;
    if e.schema_version != 1 || !(canonical(&e.asset_id) || opaque_replay_identity(&e.asset_id)) || !(if e.binding_kind=="publication" {e.target_id.strip_suffix(":publication").is_some_and(canonical)}else{canonical(&e.target_id)}) || e.target_created_at.is_empty() || e.source_operation_id.is_empty() || !matches!(e.binding_kind.as_str(), "draft" | "publication") { return Err(unavailable()) }
    let expected = json!({"asset_id":e.asset_id,"binding_kind":e.binding_kind,"target_id":e.target_id,"target_created_at":e.target_created_at,"source_operation_id":e.source_operation_id,"evidence_json":raw,"evidence_sha256":draft_save::digest(&serde_json::to_value(&e).map_err(AppError::internal)?)?});
    if *row != expected { return Err(unavailable()) }
    Ok(e)
}

#[cfg(target_os = "linux")]
pub(crate) async fn admit_upload(c: &mut AnyConnection, receipt: &storage::attachment_upload::Receipt, fresh: bool) -> Result<(), AppError> {
    let i = &receipt.intent;
    let q = &i.request;
    let expected = if let Some(scope) = &q.draft_scope {
        scope.validate().map_err(|_| unavailable())?;
        let source = json!({"schema_version":2,"asset_id":i.asset_id,"operation_id":q.operation_id,"actor_id":q.actor_id,"space_id":q.space_id,"document_id":q.document_id,"draft_scope":scope,"intent_sha256":receipt.intent_sha256,"install_proof_sha256":receipt.install_proof_sha256});
        json!({"asset_id":i.asset_id,"kind":"draft_upload_v2","original_space_id":q.space_id,"original_document_id":q.document_id,"draft_id":scope.draft_id,"draft_created_at":scope.draft_created_at,"actor_id":q.actor_id,"operation_id":q.operation_id,"scope_json":serde_json::to_string(&source).map_err(AppError::internal)?,"scope_sha256":draft_save::digest(&source)?})
    } else {
        json!({"asset_id":i.asset_id,"kind":"legacy_page_v1","original_space_id":q.space_id,"original_document_id":q.document_id,"draft_id":null,"draft_created_at":null,"actor_id":null,"operation_id":null,"scope_json":null,"scope_sha256":null})
    };
    if fresh { sqlx::query("INSERT INTO attachment_audience_origins(asset_id,kind,original_space_id,original_document_id,draft_id,draft_created_at,actor_id,operation_id,scope_json,scope_sha256) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(asset_id) DO NOTHING")
        .bind(&i.asset_id).bind(text(&expected,"kind")?).bind(&q.space_id).bind(&q.document_id).bind(expected["draft_id"].as_str()).bind(expected["draft_created_at"].as_str()).bind(expected["actor_id"].as_str()).bind(expected["operation_id"].as_str()).bind(expected["scope_json"].as_str()).bind(expected["scope_sha256"].as_str()).execute(&mut *c).await.map_err(AppError::internal)?; }
    if rows(c,"SELECT * FROM attachment_audience_origins WHERE asset_id=$1",&[&i.asset_id]).await? != vec![expected.clone()] { return Err(changed()) }
    if let Some(scope) = &q.draft_scope {
        insert_binding(c,&BindingEvidence{schema_version:1,asset_id:i.asset_id.clone(),binding_kind:"draft".into(),target_id:scope.draft_id.clone(),target_created_at:scope.draft_created_at.clone(),source_operation_id:q.operation_id.clone(),source:json!({"kind":"upload","origin_sha256":expected["scope_sha256"]})},fresh).await?;
    }
    Ok(())
}

#[cfg(target_os="linux")]
pub(crate) async fn admit_replay(c:&mut AnyConnection,r:&crate::sync::AssetReplayReceipt,fresh:bool)->Result<(),AppError>{
    let id=&r.incoming.entity_id;
    if !crate::sync::asset_receipt_current(c,r).await.map_err(AppError::Internal)? {return Err(changed())}
    let expected=if let Some(scope)=&r.draft_scope {
        scope.validate().map_err(|_|unavailable())?;
        let replay_id_valid=canonical(id) || (r.draft_binding.is_some() && opaque_replay_identity(id));
        if !replay_id_valid||r.admission_attempt_id.is_none(){return Err(unavailable())}
        let source=json!({"schema_version":2,"producer":"replay","receipt":r});
        json!({"asset_id":id,"kind":"draft_replay_v2","original_space_id":r.space_id,"original_document_id":r.document_id,"draft_id":scope.draft_id,"draft_created_at":scope.draft_created_at,"actor_id":r.actor_id,"operation_id":r.incoming.operation_id,"scope_json":serde_json::to_string(&source).map_err(AppError::internal)?,"scope_sha256":draft_save::digest(&source)?})
    }else{json!({"asset_id":id,"kind":"legacy_page_v1","original_space_id":r.space_id,"original_document_id":r.document_id,"draft_id":null,"draft_created_at":null,"actor_id":null,"operation_id":null,"scope_json":null,"scope_sha256":null})};
    if fresh {
        if !crate::sync::asset_association_current(c,r).await.map_err(AppError::Internal)?{return Err(changed())}
        sqlx::query("INSERT INTO attachment_audience_origins(asset_id,kind,original_space_id,original_document_id,draft_id,draft_created_at,actor_id,operation_id,scope_json,scope_sha256) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT(asset_id) DO NOTHING")
            .bind(id).bind(text(&expected,"kind")?).bind(&r.space_id).bind(&r.document_id).bind(expected["draft_id"].as_str()).bind(expected["draft_created_at"].as_str()).bind(expected["actor_id"].as_str()).bind(expected["operation_id"].as_str()).bind(expected["scope_json"].as_str()).bind(expected["scope_sha256"].as_str()).execute(&mut *c).await.map_err(AppError::internal)?;
    }
    if rows(c,"SELECT * FROM attachment_audience_origins WHERE asset_id=$1",&[id]).await?!=vec![expected.clone()]{return Err(changed())}
    if let Some(scope)=&r.draft_scope {
        insert_binding(c,&BindingEvidence{schema_version:1,asset_id:id.clone(),binding_kind:"draft".into(),target_id:scope.draft_id.clone(),target_created_at:scope.draft_created_at.clone(),source_operation_id:r.incoming.operation_id.clone(),source:json!({"kind":"upload","origin_sha256":expected["scope_sha256"]})},fresh).await?;
    }
    Ok(())
}

async fn origin(c: &mut AnyConnection, id: &str) -> Result<Value, AppError> {
    let r = rows(c,"SELECT * FROM attachment_audience_origins WHERE asset_id=$1",&[id]).await?;
    if r.len()!=1 { return Err(unavailable()) }
    let o=&r[0];
    if o["asset_id"]!=id { return Err(unavailable()) }
    match o["kind"].as_str() {
        Some("legacy_page_v1") => {
            for k in ["draft_id","draft_created_at","actor_id","operation_id","scope_json","scope_sha256"] { if !o[k].is_null() {return Err(unavailable())} }
        },
        Some("draft_upload_v2") => {
            let raw=text(o,"scope_json")?;
            if raw.len()>16384 {return Err(unavailable())}
            let scope:Value=serde_json::from_str(&raw).map_err(|_|unavailable())?;
            if draft_save::digest(&scope)?!=text(o,"scope_sha256")? || scope["schema_version"]!=2 || scope["asset_id"]!=id || scope["operation_id"]!=o["operation_id"] || scope["actor_id"]!=o["actor_id"] || scope["space_id"]!=o["original_space_id"] || scope["document_id"]!=o["original_document_id"] || scope["draft_scope"]["draft_id"]!=o["draft_id"] || scope["draft_scope"]["draft_created_at"]!=o["draft_created_at"] {return Err(unavailable())}
            #[cfg(target_os="linux")]
            {
                let receipt=storage::attachment_upload::receipt(c,&text(o,"operation_id")?).await.map_err(|_|unavailable())?.ok_or_else(unavailable)?;
                let q=&receipt.intent.request;
                let ds=q.draft_scope.as_ref().ok_or_else(unavailable)?;
                if receipt.intent.asset_id!=id || q.actor_id!=text(o,"actor_id")? || q.space_id!=text(o,"original_space_id")? || q.document_id!=text(o,"original_document_id")? || ds.draft_id!=text(o,"draft_id")? || ds.draft_created_at!=text(o,"draft_created_at")? || scope["intent_sha256"]!=receipt.intent_sha256 || scope["install_proof_sha256"]!=receipt.install_proof_sha256 {return Err(unavailable())}
            }
        },
        Some("draft_replay_v2")=>{
            let raw=text(o,"scope_json")?;
            if raw.len()>65536{return Err(unavailable())}
            let source:Value=serde_json::from_str(&raw).map_err(|_|unavailable())?;
            if source["schema_version"]!=2 || source["producer"]!="replay" || draft_save::digest(&source)?!=text(o,"scope_sha256")? {return Err(unavailable())}
            #[cfg(target_os="linux")]
            {
                let r:crate::sync::AssetReplayReceipt=serde_json::from_value(source["receipt"].clone()).map_err(|_|unavailable())?;
                let scope=r.draft_scope.as_ref().ok_or_else(unavailable)?;
                scope.validate().map_err(|_|unavailable())?;
                let replay_id_valid=canonical(id) || (r.draft_binding.is_some() && opaque_replay_identity(id));
                if !replay_id_valid || r.incoming.entity_id!=id || r.incoming.operation_id!=text(o,"operation_id")? || r.actor_id!=text(o,"actor_id")? || r.space_id!=text(o,"original_space_id")? || r.document_id!=text(o,"original_document_id")? || scope.draft_id!=text(o,"draft_id")? || scope.draft_created_at!=text(o,"draft_created_at")? || r.admission_attempt_id.is_none() || !crate::sync::asset_receipt_current(c,&r).await.map_err(|_|unavailable())? || !crate::sync::asset_association_current(c,&r).await.map_err(|_|unavailable())? {return Err(unavailable())}
            }
            #[cfg(not(target_os="linux"))]
            {return Err(unavailable())}
        },
        _=>return Err(unavailable()),
    }
    // A missing/corrupt scoped classification never becomes legacy through SQL absence.
    #[cfg(target_os="linux")]
    if o["kind"]=="legacy_page_v1" {
        let operations:Vec<String>=sqlx::query_scalar("SELECT operation_id FROM attachment_upload_receipts WHERE asset_id=$1").bind(id).fetch_all(&mut *c).await.map_err(AppError::internal)?;
        for op in operations {
            let r=storage::attachment_upload::receipt(c,&op).await.map_err(|_|unavailable())?.ok_or_else(unavailable)?;
            if r.intent.request.draft_scope.is_some() {return Err(unavailable())}
        }
    }
    #[cfg(target_os="linux")]
    if o["kind"]=="legacy_page_v1" {
        for row in rows(c,"SELECT receipt_json FROM asset_replay_receipts WHERE entity_id=$1 AND entity_type='asset'",&[id]).await? {
            let r:crate::sync::AssetReplayReceipt=serde_json::from_str(&text(&row,"receipt_json")?).map_err(|_|unavailable())?;
            if r.draft_scope.is_some(){return Err(unavailable())}
        }
    }
    Ok(o.clone())
}

/// Explicit page-owned producer admission. Never called as a reader fallback.
pub(crate) async fn admit_legacy(c:&mut AnyConnection,id:&str,space:&str,doc:&str)->Result<(),AppError>{
    sqlx::query("INSERT INTO attachment_audience_origins(asset_id,kind,original_space_id,original_document_id) VALUES($1,'legacy_page_v1',$2,$3) ON CONFLICT(asset_id) DO NOTHING").bind(id).bind(space).bind(doc).execute(&mut *c).await.map_err(AppError::internal)?;
    let o=origin(c,id).await?;
    if o["kind"]!="legacy_page_v1" || o["original_space_id"]!=space || o["original_document_id"]!=doc {return Err(changed())} Ok(())
}
pub(crate) struct CollectionProof { policy:super::drafts::DraftReaderInventory, assets:Vec<published_source::SourceProof> }
pub(crate) async fn capture_collection(c:&mut AnyConnection,auth:&AuthUser,space:&str)->Result<CollectionProof,AppError>{
    let policy=super::drafts::capture_draft_reader_inventory(c,space,&auth.claims.sub).await?;
    let mut assets=Vec::new();
    for query in ["SELECT * FROM attachments WHERE space_id=$1","SELECT * FROM attachment_audience_origins WHERE asset_id IN(SELECT id FROM attachments WHERE space_id=$1)","SELECT * FROM attachment_audience_bindings WHERE asset_id IN(SELECT id FROM attachments WHERE space_id=$1)"] {
        assets.push(published_source::SourceProof{query:query.into(),parameters:vec![space.into()],rows:rows(c,query,&[space]).await?});
    }
    Ok(CollectionProof{policy,assets})
}
async fn verify_collection_source(c:&mut AnyConnection,proof:&CollectionProof)->Result<(),AppError>{
    published_source::verify_source(c,&proof.assets).await?;
    super::drafts::verify_draft_reader_inventory(c,&proof.policy).await
}
pub(crate) async fn verify_collection(c:&mut AnyConnection,auth:&AuthUser,proof:&CollectionProof)->Result<(),AppError>{
    verify_collection_source(c,proof).await?;
    access::require_current_human_read(c,auth).await?;Ok(())
}
async fn verify_collection_write(c:&mut AnyConnection,auth:&AuthUser,proof:&CollectionProof)->Result<(),AppError>{
    verify_collection_source(c,proof).await?;
    access::require_current_human_preference_write(c,auth).await?;Ok(())
}

pub(crate) async fn collection_access(c:&mut AnyConnection,auth:&AuthUser,space:&str,doc:&str)->Result<access::PageAccess,AppError>{
    match super::shared_pages::asset_view(c,auth,space,doc).await {
        Ok(p)=>Ok(p),
        Err(AppError::NotFound(_)|AppError::Forbidden(_))=>access::resolve_page_access(c,access::principal_from_auth(auth)?,space,doc,access::PageOperation::Edit,access::Purpose::Routine).await,
        Err(e)=>Err(e),
    }
}
pub(crate) async fn asset_page(c:&mut AnyConnection,auth:&AuthUser,space:&str,doc:&str,id:&str)->Result<access::PageAccess,AppError>{
    read(c,auth,space,doc,id).await?;
    collection_access(c,auth,space,doc).await
}

#[cfg(target_os="linux")]
pub(crate) async fn require_change(c:&mut AnyConnection,auth:&AuthUser,space:&str,doc:&str,id:&str,scope:Option<&storage::attachment_upload::DraftUploadScope>)->Result<(),AppError>{
    let o=origin(c,id).await?;
    verify_upload_scope(c,auth,space,doc,scope).await?;
    if o["kind"]=="legacy_page_v1" || is_published(c,doc,id).await? {return Ok(())}
    let selected=scope.ok_or_else(||AppError::Conflict("Private attachment changes require the original selected draft".into()))?;
    let bound=rows(c,"SELECT * FROM attachment_audience_bindings WHERE asset_id=$1 AND binding_kind='draft' AND target_id=$2",&[id,&selected.draft_id]).await?;
    if bound.len()!=1 || parse_binding(&bound[0])?.target_created_at!=selected.draft_created_at {return Err(unavailable())}
    read(c,auth,space,doc,id).await?;Ok(())
}

async fn copy_witness(c:&mut AnyConnection,operation:&str,target:&str)->Result<(String,Value),AppError>{
    let birth=rows(c,"SELECT * FROM draft_birth_receipts WHERE operation_id=$1 AND draft_id=$2",&[operation,target]).await?;
    if birth.len()==1 {
        let result:Value=serde_json::from_str(&text(&birth[0],"result_json")?).map_err(|_|unavailable())?;
        if result["id"]!=target || draft_save::digest(&result)?!=text(&birth[0],"result_digest")? {return Err(unavailable())}
        return Ok(("birth".into(),birth[0].clone()))
    }
    let recovery=rows(c,"SELECT * FROM snapshot_recovery_receipts WHERE operation_id=$1",&[operation]).await?;
    if recovery.len()==1 {
        let evidence:Value=serde_json::from_str(&text(&recovery[0],"evidence_json")?).map_err(|_|unavailable())?;
        if evidence["result"]["id"]!=target || draft_save::digest(&evidence)?!=text(&recovery[0],"evidence_digest")? {return Err(unavailable())}
        return Ok(("recovery".into(),recovery[0].clone()))
    }
    Err(unavailable())
}
async fn verify_draft_binding(c:&mut AnyConnection,o:&Value,e:&BindingEvidence)->Result<(),AppError>{
    if !canonical(&e.asset_id) && (!opaque_replay_identity(&e.asset_id) || o["kind"]!="draft_replay_v2") {return Err(unavailable())}
    match e.source["kind"].as_str() {
        Some("upload")=>{
            if e.target_id!=text(o,"draft_id")? || e.target_created_at!=text(o,"draft_created_at")? || e.source_operation_id!=text(o,"operation_id")? || e.source!=json!({"kind":"upload","origin_sha256":o["scope_sha256"]}) {return Err(unavailable())}
        },
        Some("reuse")=>{
            let proof=e.source["original_source_proof_sha256"].as_str().ok_or_else(unavailable)?;
            if e.source["schema_version"]!=1 || e.source["operation_id"]!=e.source_operation_id || !canonical(&e.source_operation_id) || e.source["asset_id"]!=e.asset_id || e.source["draft_scope"]["schema_version"]!=2 || e.source["draft_scope"]["draft_id"]!=e.target_id || e.source["draft_scope"]["draft_created_at"]!=e.target_created_at || e.source["origin_sha256"]!=o["scope_sha256"] || e.source["actor_id"].as_str().is_none_or(str::is_empty) || proof.len()!=64 || !proof.bytes().all(|b|b.is_ascii_hexdigit()) {return Err(unavailable())}
        },
        Some("draft_copy")=>{
            let (kind,witness)=copy_witness(c,&e.source_operation_id,&e.target_id).await?;
            if e.source["witness_kind"]!=kind || e.source["witness_sha256"]!=draft_save::digest(&witness)? || e.source["origin_sha256"]!=o["scope_sha256"] {return Err(unavailable())}
            let result:Value=if kind=="birth" {serde_json::from_str(&text(&witness,"result_json")?).map_err(|_|unavailable())?}else{let v:Value=serde_json::from_str(&text(&witness,"evidence_json")?).map_err(|_|unavailable())?;v["result"].clone()};
            if result["created_at"]!=e.target_created_at || !inherited_reference_ids(c,&text(&result,"document_id")?,&text(&result,"content")?,draft_save::optional(&result,"cover_image")?.as_deref()).await?.contains(&e.asset_id) {return Err(unavailable())}
        },
        _=>return Err(unavailable()),
    }
    Ok(())
}

#[derive(Clone)]
pub(crate) struct ReadProof { digest: String, expected_binary:Option<(u64,String)> }
impl ReadProof {
    /// Receipt-backed byte identity; absence retains declared old-vault semantics.
    pub(crate) fn expected_binary(&self)->Option<(u64,&str)> {
        self.expected_binary.as_ref().map(|(length,hash)|(*length,hash.as_str()))
    }
}
async fn expected_binary(c:&mut AnyConnection,o:&Value)->Result<Option<(u64,String)>,AppError>{
    let pair:(i64,String)=match o["kind"].as_str(){
        Some("legacy_page_v1")=>return Ok(None),
        #[cfg(target_os="linux")]
        Some("draft_upload_v2")=>{
            let r=storage::attachment_upload::receipt(c,&text(o,"operation_id")?).await.map_err(|_|unavailable())?.ok_or_else(unavailable)?;
            let scope:Value=serde_json::from_str(&text(o,"scope_json")?).map_err(|_|unavailable())?;
            if scope["intent_sha256"]!=r.intent_sha256 || scope["install_proof_sha256"]!=r.install_proof_sha256 || r.intent.asset_id!=text(o,"asset_id")? {return Err(unavailable())}
            (r.intent.byte_length,r.intent.binary_sha256)
        },
        #[cfg(target_os="linux")]
        Some("draft_replay_v2")=>{
            // origin() already checked this exact retained receipt against all
            // current SQL scalars. It supplies byte identity, never a new grant.
            let source:Value=serde_json::from_str(&text(o,"scope_json")?).map_err(|_|unavailable())?;
            let r:crate::sync::AssetReplayReceipt=serde_json::from_value(source["receipt"].clone()).map_err(|_|unavailable())?;
            (r.byte_length,r.binary_sha256)
        },
        _=>return Err(unavailable()),
    };
    if pair.0<0 || pair.1.len()!=64 || !pair.1.bytes().all(|b|b.is_ascii_hexdigit()) {return Err(unavailable())}
    Ok(Some((pair.0 as u64,pair.1)))
}
pub(crate) async fn read(c:&mut AnyConnection,auth:&AuthUser,space:&str,doc:&str,id:&str)->Result<ReadProof,AppError> {
    access::require_current_human_read(c,auth).await?;
    let inventory=super::drafts::capture_draft_reader_inventory(c,space,&auth.claims.sub).await?;
    let association=rows(c,"SELECT id,document_id,space_id FROM attachments WHERE id=$1",&[id]).await?;
    // Registered retained versions can outlive their association but do not acquire a new page.
    let current_doc=if association.len()==1 {text(&association[0],"document_id")?} else {return Err(unavailable())};
    if current_doc!=doc || association[0]["space_id"]!=space {return Err(unavailable())}
    let o=origin(c,id).await?;
    let expected_binary=expected_binary(c,&o).await?;
    let bindings=rows(c,"SELECT * FROM attachment_audience_bindings WHERE asset_id=$1",&[id]).await?;
    if bindings.len()>4096 {return Err(unavailable())}
    let mut authority=None;
    if o["kind"]=="legacy_page_v1" {
        let page=super::shared_pages::asset_view(c,auth,space,doc).await?;
        authority=Some(json!({"kind":"legacy_page_v1","grant":page.direct_viewer_grant}));
    } else {
        for row in &bindings {
            let e=parse_binding(row)?;
            if e.asset_id!=id {return Err(unavailable())}
            if e.binding_kind=="draft" {
                verify_draft_binding(c,&o,&e).await?;
                let drafts=rows(c,"SELECT id,created_at FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3",&[&e.target_id,doc,space]).await?;
                if drafts.len()!=1 || drafts[0]["created_at"]!=e.target_created_at {continue}
                let principal=access::principal_from_auth(auth)?;
                let allowed=match access::resolve_draft_access(c,principal,space,doc,&e.target_id,access::DraftOperation::View,access::Purpose::Routine).await {
                    Ok(a)=>Ok(a),
                    Err(AppError::NotFound(_)|AppError::Forbidden(_))=>access::resolve_draft_access(c,principal,space,doc,&e.target_id,access::DraftOperation::Edit,access::Purpose::Routine).await,
                    Err(e)=>Err(e),
                };
                match allowed {
                    Ok(a)=>{authority=Some(json!({"kind":"draft","id":e.target_id,"created_at":e.target_created_at,"participation":format!("{:?}",a.participation)}));break},
                    Err(AppError::NotFound(_)|AppError::Forbidden(_)|AppError::Conflict(_))=>continue,
                    Err(e)=>return Err(e),
                }
            } else {
                let snapshots=super::publication_snapshots::read_validated(c,doc).await?;
                if let Some(s)=snapshots.iter().find(|s|s.id==e.target_id) {
                    if s.kind!="publication" || s.operation!=e.source_operation_id || s.published_at!=e.target_created_at || e.source!=json!({"kind":"publication","snapshot_digest":super::publication_snapshots::digest(&json!(s.payload))?}) || !snapshot_reference_ids(&s.payload).contains(id) {return Err(unavailable())}
                    match super::shared_pages::asset_view(c,auth,space,doc).await {
                        Ok(page)=>{authority=Some(json!({"kind":"publication","id":e.target_id,"grant":page.direct_viewer_grant}));break},
                        Err(AppError::NotFound(_)|AppError::Forbidden(_))=>continue,
                        Err(e)=>return Err(e),
                    }
                }
            }
        }
    }
    let authority=authority.ok_or_else(unavailable)?;
    if rows(c,"SELECT * FROM attachment_audience_origins WHERE asset_id=$1",&[id]).await?!=vec![o.clone()] || rows(c,"SELECT * FROM attachment_audience_bindings WHERE asset_id=$1",&[id]).await?!=bindings {return Err(changed())}
    super::drafts::verify_draft_reader_inventory(c,&inventory).await?;
    access::require_current_human_read(c,auth).await?;
    Ok(ReadProof{digest:draft_save::digest(&json!({"origin":o,"bindings":bindings,"authority":authority,"association":association,"expected_binary":expected_binary}))?,expected_binary})
}
pub(crate) async fn verify_read(c:&mut AnyConnection,auth:&AuthUser,space:&str,doc:&str,id:&str,proof:&ReadProof)->Result<(),AppError>{
    if read(c,auth,space,doc,id).await?.digest!=proof.digest {return Err(changed())} Ok(())
}
pub(crate) async fn visible(c:&mut AnyConnection,auth:&AuthUser,space:&str,doc:&str,id:&str)->Result<bool,AppError>{
    match read(c,auth,space,doc,id).await {Ok(_)=>Ok(true),Err(AppError::NotFound(_)|AppError::Forbidden(_))=>Ok(false),Err(e)=>Err(e)}
}

/// Actual Markdown link/image destinations only. Code/text strings confer no promotion.
pub(crate) fn references(body:&str,cover:Option<&str>)->Result<BTreeSet<String>,AppError>{
    reference_tokens(body,cover,true)
}
fn reference_tokens(body:&str,cover:Option<&str>,strict:bool)->Result<BTreeSet<String>,AppError>{
    use pulldown_cmark::{Event,Parser,Tag};
    if body.len()>8*1024*1024 {return Err(changed())}
    let mut ids=BTreeSet::new();
    let mut insert=|url:&str|->Result<(),AppError>{
        if let Some(id)=url.strip_prefix("starlane-attachment://") {
            if !(canonical(id) || opaque_replay_identity(id)) {if strict{return Err(changed())}else{return Ok(())}}
            ids.insert(id.to_owned());
            if ids.len()>4096 {return Err(changed())}
        }
        Ok(())
    };
    for (index,event) in Parser::new(body).enumerate(){
        if index>262144 {return Err(changed())}
        if let Event::Start(Tag::Link{dest_url,..}|Tag::Image{dest_url,..})=event {insert(&dest_url)?;}
    }
    if let Some(cover)=cover {insert(cover)?;}
    Ok(ids)
}

async fn reference_ids(c:&mut AnyConnection,doc:&str,body:&str,cover:Option<&str>)->Result<BTreeSet<String>,AppError>{
    resolve_reference_ids(c,doc,body,cover,true).await
}
/// Birth/recovery preserves historical text, including unresolved/opaque links.
/// Only uniquely resolved current versions participate in inherited grants.
async fn inherited_reference_ids(c:&mut AnyConnection,doc:&str,body:&str,cover:Option<&str>)->Result<BTreeSet<String>,AppError>{
    resolve_reference_ids(c,doc,body,cover,false).await
}
async fn resolve_reference_ids(c:&mut AnyConnection,doc:&str,body:&str,cover:Option<&str>,strict:bool)->Result<BTreeSet<String>,AppError>{
    use pulldown_cmark::{Event,Parser,Tag};
    let mut ids=if strict{references(body,cover)?}else{reference_tokens(body,cover,false)?};
    for id in ids.clone() {
        if canonical(&id) {continue}
        let association=rows(c,"SELECT id,document_id,space_id FROM attachments WHERE id=$1 AND document_id=$2",&[&id,doc]).await?;
        let proven=if association.len()==1 {origin(c,&id).await.is_ok_and(|o|o["kind"]=="draft_replay_v2"&&o["original_document_id"]==doc)}else{false};
        if !proven {if strict{return Err(unavailable())}else{ids.remove(&id);}}
    }
    let mut paths=BTreeSet::new();
    let mut collect=|url:&str| {
        if let Some(path)=url.strip_prefix("/api/v1/files/") {paths.insert(path.to_owned());}
    };
    for event in Parser::new(body) {if let Event::Start(Tag::Link{dest_url,..}|Tag::Image{dest_url,..})=event {collect(&dest_url)}}
    if let Some(cover)=cover {collect(cover)}
    if paths.len()>4096{return Err(changed())}
    for path in paths {
        if !storage::validate_path(&path){if strict{return Err(changed())}else{continue}}
        let matched:Vec<String>=sqlx::query_scalar("SELECT DISTINCT a.id FROM attachments a WHERE a.document_id=$1 AND (a.storage_path=$2 OR a.id IN(SELECT asset_id FROM move_asset_path_aliases WHERE old_path=$2 AND asset_kind='attachment'))").bind(doc).bind(&path).fetch_all(&mut *c).await.map_err(AppError::internal)?;
        if matched.len()!=1 {if strict{return Err(AppError::Conflict("Attachment path no longer identifies one exact version; retain its original reference for review".into()))}else{continue}}
        ids.insert(matched[0].clone());
    }
    if ids.len()>4096{return Err(changed())}Ok(ids)
}

/// Historical decisions use their immutable captured association paths. They do not
/// resolve today's paths or impose a new parser on an older publication receipt.
fn snapshot_reference_ids(payload:&super::publication_snapshots::Payload)->BTreeSet<String>{
    use pulldown_cmark::{Event,Parser,Tag};
    let mut ids=BTreeSet::new();
    let mut collect=|url:&str| {
        if let Some(id)=url.strip_prefix("starlane-attachment://") {
            let exact_association=payload.comparisons.attachments.iter().any(|row|row["id"]==id&&row["document_id"]==payload.document_id&&row["space_id"]==payload.captured_space_id);
            if canonical(id)||(opaque_replay_identity(id)&&exact_association){ids.insert(id.to_owned());}
        }
        if let Some(path)=url.strip_prefix("/api/v1/files/") {
            for row in &payload.comparisons.attachments {if row["storage_path"]==path {if let Some(id)=row["id"].as_str(){ids.insert(id.into());}}}
        }
    };
    for event in Parser::new(payload.content.portable_markdown.as_deref().unwrap_or("")){if let Event::Start(Tag::Link{dest_url,..}|Tag::Image{dest_url,..})=event{collect(&dest_url)}}
    if let Some(cover)=payload.content.cover_reference.as_deref(){collect(cover)}
    ids
}

/// Called under the existing publication transaction BEFORE draft consumption.
pub(crate) async fn prepare_publication(c:&mut AnyConnection,auth:&AuthUser,space:&str,doc:&str,draft:&str,body:&str,cover:Option<&str>)->Result<BTreeSet<String>,AppError>{
    access::resolve_draft_access(c,access::principal_from_auth(auth)?,space,doc,draft,access::DraftOperation::Edit,access::Purpose::Routine).await?;
    let ids=reference_ids(c,doc,body,cover).await?;
    let selected=draft_save::selected(c,space,doc,draft).await?;
    for id in &ids {
        // Do not reveal that an unreadable guessed version exists through the
        // distinct review-reuse response for otherwise readable source versions.
        read(c,auth,space,doc,id).await?;
        let o=origin(c,id).await?;
        if o["kind"]=="legacy_page_v1" {continue}
        let bound=rows(c,"SELECT * FROM attachment_audience_bindings WHERE asset_id=$1 AND binding_kind='draft' AND target_id=$2",&[id,draft]).await?;
        if bound.len()!=1 || parse_binding(&bound[0])?.target_created_at!=text(&selected,"created_at")? {return Err(AppError::Conflict("Referenced attachment is not bound to this original draft; review reuse before publishing".into()))}
        verify_draft_binding(c,&o,&parse_binding(&bound[0])?).await?;
        read(c,auth,space,doc,id).await?;
    }
    Ok(ids)
}
async fn publication_records(c:&mut AnyConnection,doc:&str,operation:&str,ids:&BTreeSet<String>,write:bool)->Result<Vec<Value>,AppError>{
    let mut expected_rows=Vec::new();
    let stored=super::publication_snapshots::read_validated(c,doc).await?;
    let s=stored.iter().find(|s|s.kind=="publication"&&s.operation==operation).ok_or_else(changed)?;
    for id in ids {
        let o=match origin(c,id).await {Ok(o)=>o,Err(AppError::NotFound(_)) if !write=>{
            let direct:Vec<String>=sqlx::query_scalar("SELECT receipt_json FROM attachment_upload_receipts WHERE asset_id=$1").bind(id).fetch_all(&mut *c).await.map_err(AppError::internal)?;
            if direct.iter().any(|raw|serde_json::from_str::<Value>(raw).is_ok_and(|r|!r["intent"]["request"]["draft_scope"].is_null())){return Err(unavailable())}
            let replay:Vec<String>=sqlx::query_scalar("SELECT receipt_json FROM asset_replay_receipts WHERE entity_id=$1 AND entity_type='asset'").bind(id).fetch_all(&mut *c).await.map_err(AppError::internal)?;
            if replay.iter().any(|raw|serde_json::from_str::<Value>(raw).is_ok_and(|r|!r["draft_scope"].is_null())){return Err(unavailable())}
            continue
        },Err(e)=>return Err(e)};
        if o["kind"]=="legacy_page_v1" {continue}
        let binding=BindingEvidence{schema_version:1,asset_id:id.clone(),binding_kind:"publication".into(),target_id:s.id.clone(),target_created_at:s.published_at.clone(),source_operation_id:operation.into(),source:json!({"kind":"publication","snapshot_digest":super::publication_snapshots::digest(&json!(s.payload))?})};
        insert_binding(c,&binding,write).await?;
        expected_rows.push(binding_row(&binding)?);
    }
    Ok(expected_rows)
}

pub(crate) async fn record_publication(c:&mut AnyConnection,doc:&str,operation:&str,ids:&BTreeSet<String>)->Result<Vec<Value>,AppError>{publication_records(c,doc,operation,ids,true).await}
pub(crate) async fn verify_publication(c:&mut AnyConnection,doc:&str,operation:&str)->Result<(),AppError>{
    let records=super::publication_snapshots::read_validated(c,doc).await?;
    let s=records.iter().find(|s|s.kind=="publication"&&s.operation==operation).ok_or_else(changed)?;
    let ids=snapshot_reference_ids(&s.payload);
    publication_records(c,doc,operation,&ids,false).await?;Ok(())
}

pub(crate) async fn audience_proofs(c:&mut AnyConnection,doc:&str)->Result<Vec<published_source::SourceProof>,AppError>{
    let mut out=Vec::new();
    for query in ["SELECT * FROM attachment_audience_origins WHERE original_document_id=$1 OR asset_id IN(SELECT id FROM attachments WHERE document_id=$1)","SELECT * FROM attachment_audience_bindings WHERE asset_id IN(SELECT asset_id FROM attachment_audience_origins WHERE original_document_id=$1 OR asset_id IN(SELECT id FROM attachments WHERE document_id=$1))"] {
        out.push(published_source::SourceProof{query:query.into(),parameters:vec![doc.into()],rows:rows(c,query,&[doc]).await?});
    }
    Ok(out)
}

pub(crate) async fn is_published(c:&mut AnyConnection,doc:&str,id:&str)->Result<bool,AppError>{
    let o=origin(c,id).await?;
    if o["kind"]=="legacy_page_v1" {return Ok(true)}
    let bindings=rows(c,"SELECT * FROM attachment_audience_bindings WHERE asset_id=$1 AND binding_kind='publication'",&[id]).await?;
    if bindings.is_empty(){return Ok(false)}
    let snapshots=super::publication_snapshots::read_validated(c,doc).await?;
    for row in bindings {
        let e=parse_binding(&row)?;
        let s=snapshots.iter().find(|s|s.kind=="publication"&&s.id==e.target_id&&s.operation==e.source_operation_id&&s.published_at==e.target_created_at).ok_or_else(unavailable)?;
        if e.source!=json!({"kind":"publication","snapshot_digest":super::publication_snapshots::digest(&json!(s.payload))?}) || !snapshot_reference_ids(&s.payload).contains(id) {return Err(unavailable())}
    }
    Ok(true)
}
pub(crate) async fn needs_copy_witness(c:&mut AnyConnection,doc:&str,target:&Value)->Result<bool,AppError>{
    for id in inherited_reference_ids(c,doc,&text(target,"content")?,draft_save::optional(target,"cover_image")?.as_deref()).await? {
        match origin(c,&id).await {Ok(o)if o["kind"]!="legacy_page_v1"=>return Ok(true),Ok(_)|Err(AppError::NotFound(_))=>(),Err(e)=>return Err(e)}
    }
    Ok(false)
}

/// Deliberate draft birth/recovery may copy proven source authority. Merely saving a pin cannot.
pub(crate) async fn inherit_draft(c:&mut AnyConnection,auth:&AuthUser,space:&str,doc:&str,target:&Value,operation:&str,source_draft:Option<&str>,source_snapshot:Option<&str>)->Result<(),AppError>{
    let mut collection=capture_collection(c,auth,space).await?;
    let target_id=text(target,"id")?;
    let stamp=text(target,"created_at")?;
    let body=text(target,"content")?;
    let cover=draft_save::optional(target,"cover_image")?;
    for id in inherited_reference_ids(c,doc,&body,cover.as_deref()).await? {
        let o=match origin(c,&id).await {Ok(o)=>o,Err(AppError::NotFound(_))=>continue,Err(e)=>return Err(e)};
        if o["kind"]=="legacy_page_v1" {continue}
        let mut permitted=false;
        if let Some(source)=source_draft {
            access::resolve_draft_access(c,access::principal_from_auth(auth)?,space,doc,source,access::DraftOperation::Edit,access::Purpose::Routine).await?;
            let original=draft_save::selected(c,space,doc,source).await?;
            let bound=rows(c,"SELECT * FROM attachment_audience_bindings WHERE asset_id=$1 AND binding_kind='draft' AND target_id=$2",&[&id,source]).await?;
            permitted=bound.len()==1&&parse_binding(&bound[0])?.target_created_at==text(&original,"created_at")?;
            if permitted {verify_draft_binding(c,&o,&parse_binding(&bound[0])?).await?;}
        } else if is_published(c,doc,&id).await? {
            permitted=if let Some(snapshot)=source_snapshot {
                !rows(c,"SELECT * FROM attachment_audience_bindings WHERE asset_id=$1 AND binding_kind='publication' AND target_id=$2",&[&id,snapshot]).await?.is_empty()
            }else{true};
        }
        if permitted {
            let (witness_kind,witness)=copy_witness(c,operation,&target_id).await?;
            let binding=BindingEvidence{schema_version:1,asset_id:id,binding_kind:"draft".into(),target_id:target_id.clone(),target_created_at:stamp.clone(),source_operation_id:operation.into(),source:json!({"kind":"draft_copy","source_draft":source_draft,"source_snapshot":source_snapshot,"origin_sha256":o["scope_sha256"],"witness_kind":witness_kind,"witness_sha256":draft_save::digest(&witness)?})};
            insert_binding(c,&binding,true).await?;
            admit_expected_binding(&mut collection.assets,&binding)?;
        }
    }
    verify_collection_write(c,auth,&collection).await?;
    Ok(())
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn promotion_only_uses_exact_link_image_and_cover_versions(){
        let a="11111111-1111-4111-8111-111111111111";let b="22222222-2222-4222-8222-222222222222";
        let replay="asset-import-33333333-3333-4333-8333-333333333333";
        let body=format!("[file](starlane-attachment://{a})\n\n`starlane-attachment://{b}`\n\n```\n[x](starlane-attachment://{b})\n```\n");
        assert_eq!(references(&body,None).unwrap(),BTreeSet::from([a.into()]));
        assert_eq!(references(&body,Some(&format!("starlane-attachment://{b}"))).unwrap(),BTreeSet::from([a.into(),b.into()]));
        assert_eq!(references(&format!("[replay](starlane-attachment://{replay})"),None).unwrap(),BTreeSet::from([replay.into()]));
        assert!(references("[file](starlane-attachment://not-a-version)",None).is_err());
    }
    #[test] fn opaque_snapshot_reference_requires_exact_captured_association(){
        let asset="asset-import-33333333-3333-4333-8333-333333333333";
        let payload=super::super::publication_snapshots::Payload{
            schema_version:1,
            binding:super::super::publication_snapshots::Binding::CaptureV2,
            document_id:"doc-1".into(),
            captured_space_id:"space-1".into(),
            content:super::super::publication_snapshots::Content{
                title:"Page".into(),icon:None,cover_reference:None,
                portable_markdown:Some(format!("[replay](starlane-attachment://{asset})")),
                extraction:super::super::publication_snapshots::Extraction::ExactPublishedDraft,
                raw_sha256:"0".repeat(64),raw_byte_length:0,
            },
            comparisons:super::super::publication_snapshots::Comparisons{
                properties_schema:vec![],property_values:vec![],drawings:vec![],
                attachments:vec![json!({"id":asset,"document_id":"doc-1","space_id":"space-1"})],
            },
            comparison_policy:"comparison_only_capture_v2_bound".into(),
            asset_policy:"reference_only_current_authorization_and_existence_required".into(),
            baseline_source_page:None,
        };
        assert_eq!(snapshot_reference_ids(&payload),BTreeSet::from([asset.into()]));
        let mut wrong_document=payload;
        wrong_document.comparisons.attachments[0]["document_id"]=json!("doc-2");
        assert!(snapshot_reference_ids(&wrong_document).is_empty());
    }
}
