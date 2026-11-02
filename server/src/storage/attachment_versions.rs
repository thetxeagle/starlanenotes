//! Additive immutable version registry. No replacement, trash, purge or quota authority.
use super::attachment_upload::{self as upload, Receipt};
use crate::{
    auth::{middleware::AuthUser, routes::AppError},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, Row};
use std::path::Path as FsPath;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FamilyHead {
    pub id: String,
    pub space_id: String,
    pub document_id: String,
    pub current_version_id: String,
    pub revision: i64,
    pub state: String,
    pub created_at: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VersionRecord {
    pub version_id: String,
    pub logical_attachment_id: String,
    pub version_number: i64,
    pub original_space_id: String,
    pub original_document_id: String,
    pub uploader_id: String,
    pub original_name: String,
    pub mime_type: String,
    pub byte_length: i64,
    pub binary_sha256: String,
    pub original_created_at: String,
    pub original_storage_path: String,
    pub original_filename: String,
    pub upload_operation_id: String,
    pub upload_attempt_id: String,
    pub raw_receipt_sha256: String,
    pub intent_sha256: String,
    pub install_proof_sha256: String,
    pub provenance: String,
}
impl VersionRecord {
    fn from_receipt(r: &Receipt, raw: &str) -> Self {
        let i = &r.intent;
        let q = &i.request;
        Self {
            version_id: i.asset_id.clone(),
            logical_attachment_id: i.asset_id.clone(),
            version_number: 1,
            original_space_id: q.space_id.clone(),
            original_document_id: q.document_id.clone(),
            uploader_id: q.actor_id.clone(),
            original_name: q.original_name.clone(),
            mime_type: q.mime_type.clone(),
            byte_length: i.byte_length,
            binary_sha256: i.binary_sha256.clone(),
            original_created_at: i.created_at.clone(),
            original_storage_path: i.storage_path.clone(),
            original_filename: i.filename.clone(),
            upload_operation_id: q.operation_id.clone(),
            upload_attempt_id: i.attempt_id.clone(),
            raw_receipt_sha256: hex::encode(Sha256::digest(raw.as_bytes())),
            intent_sha256: r.intent_sha256.clone(),
            install_proof_sha256: r.install_proof_sha256.clone(),
            provenance: "direct_upload_v1".into(),
        }
    }
}
/// Opaque token created only from a current, physically owner-verified upload receipt.
pub(crate) struct PreparedSingleton {
    head: FamilyHead,
    version: VersionRecord,
    receipt: Receipt,
}
impl PreparedSingleton {
    pub(crate) fn head(&self) -> &FamilyHead {
        &self.head
    }
    pub(crate) fn version(&self) -> &VersionRecord {
        &self.version
    }
}
async fn receipt_raw(conn: &mut AnyConnection, r: &Receipt) -> Result<String, String> {
    if upload::receipt(conn, &r.intent.request.operation_id)
        .await?
        .as_ref()
        != Some(r)
    {
        return Err("Registry upload receipt changed".into());
    }
    let raw: String = sqlx::query_scalar(
        "SELECT substr(receipt_json,1,16385) FROM attachment_upload_receipts WHERE operation_id=$1",
    )
    .bind(&r.intent.request.operation_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| e.to_string())?;
    if raw.len() > 16384 {
        return Err("Registry receipt exceeds owner envelope bound".into());
    }
    let exact: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM attachment_upload_receipts WHERE operation_id=$1 AND receipt_json=$2",
    )
    .bind(&r.intent.request.operation_id)
    .bind(&raw)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| e.to_string())?;
    if exact.is_none() {
        return Err("Registry original receipt differs".into());
    }
    Ok(raw)
}
pub(crate) async fn prepare_singleton(
    root: &FsPath,
    conn: &mut AnyConnection,
    r: &Receipt,
) -> Result<PreparedSingleton, String> {
    let before = receipt_raw(conn, r).await?;
    if !upload::association(conn, r).await? {
        return Err("Registry upload association differs".into());
    }
    let held = root.to_path_buf();
    let receipt = r.clone();
    tokio::task::spawn_blocking(move || upload::verify(&held, &receipt, false))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    let after = receipt_raw(conn, r).await?;
    if after != before || !upload::association(conn, r).await? {
        return Err("Registry source changed during preparation".into());
    }
    let version = VersionRecord::from_receipt(r, &after);
    let head = FamilyHead {
        id: version.logical_attachment_id.clone(),
        space_id: r.intent.request.space_id.clone(),
        document_id: r.intent.request.document_id.clone(),
        current_version_id: version.version_id.clone(),
        revision: 1,
        state: "active".into(),
        created_at: r.intent.created_at.clone(),
    };
    Ok(PreparedSingleton {
        head,
        version,
        receipt: r.clone(),
    })
}
async fn family(conn: &mut AnyConnection, id: &str) -> Result<Option<FamilyHead>, String> {
    let row = sqlx::query("SELECT * FROM attachment_families WHERE id=$1")
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(|e| e.to_string())?;
    row.map(|r| {
        Ok(FamilyHead {
            id: r.try_get("id")?,
            space_id: r.try_get("space_id")?,
            document_id: r.try_get("document_id")?,
            current_version_id: r.try_get("current_version_id")?,
            revision: r.try_get("revision")?,
            state: r.try_get("state")?,
            created_at: r.try_get("created_at")?,
        })
    })
    .transpose()
    .map_err(|e: sqlx::Error| e.to_string())
}
async fn version(conn: &mut AnyConnection, id: &str) -> Result<Option<VersionRecord>, String> {
    let row = sqlx::query("SELECT * FROM attachment_version_records WHERE version_id=$1")
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(|e| e.to_string())?;
    row.map(|r| {
        Ok(VersionRecord {
            version_id: r.try_get("version_id")?,
            logical_attachment_id: r.try_get("logical_attachment_id")?,
            version_number: r.try_get("version_number")?,
            original_space_id: r.try_get("original_space_id")?,
            original_document_id: r.try_get("original_document_id")?,
            uploader_id: r.try_get("uploader_id")?,
            original_name: r.try_get("original_name")?,
            mime_type: r.try_get("mime_type")?,
            byte_length: r.try_get("byte_length")?,
            binary_sha256: r.try_get("binary_sha256")?,
            original_created_at: r.try_get("original_created_at")?,
            original_storage_path: r.try_get("original_storage_path")?,
            original_filename: r.try_get("original_filename")?,
            upload_operation_id: r.try_get("upload_operation_id")?,
            upload_attempt_id: r.try_get("upload_attempt_id")?,
            raw_receipt_sha256: r.try_get("raw_receipt_sha256")?,
            intent_sha256: r.try_get("intent_sha256")?,
            install_proof_sha256: r.try_get("install_proof_sha256")?,
            provenance: r.try_get("provenance")?,
        })
    })
    .transpose()
    .map_err(|e: sqlx::Error| e.to_string())
}
/// Call inside the caller's existing page/space mutation transaction, never autocommit.
pub(crate) async fn publish_singleton(
    conn: &mut AnyConnection,
    p: &PreparedSingleton,
    fresh_upload: bool,
) -> Result<SingletonPublication, String> {
    let raw = receipt_raw(conn, &p.receipt).await?;
    if VersionRecord::from_receipt(&p.receipt, &raw) != p.version
        || !upload::association(conn, &p.receipt).await?
    {
        return Err("Registry association or original receipt is no longer current".into());
    }
    let expected_head = match (
        family(conn, &p.head.id).await?,
        version(conn, &p.version.version_id).await?,
    ) {
        (None, None) if !fresh_upload => {
            verify_unregistered_ack(conn, p).await?;
            return Ok(SingletonPublication { registered: false });
        }
        (None, None) => {
            let h = &p.head;
            sqlx::query("INSERT INTO attachment_families(id,space_id,document_id,current_version_id,revision,state,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)")
                .bind(&h.id).bind(&h.space_id).bind(&h.document_id).bind(&h.current_version_id).bind(h.revision).bind(&h.state).bind(&h.created_at).execute(&mut*conn).await.map_err(|e|e.to_string())?;
            let v = &p.version;
            sqlx::query("INSERT INTO attachment_version_records(version_id,logical_attachment_id,version_number,original_space_id,original_document_id,uploader_id,original_name,mime_type,byte_length,binary_sha256,original_created_at,original_storage_path,original_filename,upload_operation_id,upload_attempt_id,raw_receipt_sha256,intent_sha256,install_proof_sha256,provenance) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)")
                .bind(&v.version_id).bind(&v.logical_attachment_id).bind(v.version_number).bind(&v.original_space_id).bind(&v.original_document_id).bind(&v.uploader_id).bind(&v.original_name).bind(&v.mime_type).bind(v.byte_length).bind(&v.binary_sha256).bind(&v.original_created_at).bind(&v.original_storage_path).bind(&v.original_filename).bind(&v.upload_operation_id).bind(&v.upload_attempt_id).bind(&v.raw_receipt_sha256).bind(&v.intent_sha256).bind(&v.install_proof_sha256).bind(&v.provenance).execute(&mut*conn).await.map_err(|e|e.to_string())?;
            p.head.clone()
        }
        (Some(h), Some(v)) if v == p.version => {
            if h.id != p.head.id
                || h.space_id != p.head.space_id
                || h.document_id != p.head.document_id
                || h.state != p.head.state
                || h.created_at != p.head.created_at
                || h.revision < 1
            {
                return Err("Singleton original family identity differs".into());
            }
            let current = history_inventory(conn, &h.id).await?;
            if current.head != h || current.first != p.version {
                return Err("Singleton retry family history differs".into());
            }
            h
        }
        _ => return Err("Registry singleton identity/head differs".into()),
    };
    // Expected head CAS is a lock/fence only: this slice exposes no pointer advance.
    let h = &expected_head;
    if sqlx::query("UPDATE attachment_families SET id=id WHERE id=$1 AND space_id=$2 AND document_id=$3 AND current_version_id=$4 AND revision=$5 AND state=$6 AND created_at=$7")
        .bind(&h.id).bind(&h.space_id).bind(&h.document_id).bind(&h.current_version_id)
        .bind(h.revision).bind(&h.state).bind(&h.created_at)
        .execute(&mut *conn).await.map_err(|e| e.to_string())?.rows_affected() != 1
    {
        return Err("Registry expected current family head changed".into());
    }
    if sqlx::query("UPDATE attachment_version_records SET version_id=version_id WHERE version_id=$1 AND logical_attachment_id=$2 AND raw_receipt_sha256=$3")
        .bind(&p.version.version_id).bind(&p.version.logical_attachment_id).bind(&p.version.raw_receipt_sha256)
        .execute(&mut *conn).await.map_err(|e| e.to_string())?.rows_affected() != 1
    {
        return Err("Registry original version row changed".into());
    }
    verify_singleton(conn, p).await?;
    Ok(SingletonPublication { registered: true })
}
pub(crate) async fn verify_singleton(
    conn: &mut AnyConnection,
    p: &PreparedSingleton,
) -> Result<(), String> {
    let raw = receipt_raw(conn, &p.receipt).await?;
    let current = history_inventory(conn, &p.head.id).await?;
    let h = &current.head;
    if VersionRecord::from_receipt(&p.receipt, &raw) != p.version
        || h.id != p.head.id
        || h.space_id != p.head.space_id
        || h.document_id != p.head.document_id
        || h.state != p.head.state
        || h.created_at != p.head.created_at
        || h.revision < 1
        || current.first != p.version
        || family(conn, &p.head.id).await?.as_ref() != Some(h)
        || version(conn, &p.version.version_id).await?.as_ref() != Some(&p.version)
        || !upload::association(conn, &p.receipt).await?
    {
        return Err("Registry full original metadata or current head changed".into());
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct HistoryInspection {
    head: FamilyHead,
    versions: Vec<VersionRecord>,
    first: VersionRecord,
    current: VersionRecord,
    after: i64,
    limit: i64,
}
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct HistoryQuery {
    after: Option<i64>,
    limit: Option<i64>,
}
#[derive(Debug, Clone, Serialize)]
struct AuthorizedHistoryHead { version_id:String, revision:i64 }
#[derive(Debug, Clone, Serialize)]
struct HistoryPage {
    schema_version:u8,
    current_head:Option<AuthorizedHistoryHead>,
    versions: Vec<VersionRecord>,
    retained_versions: std::collections::BTreeMap<String, super::attachment_retirement::Retirement>,
    total: i64,
    next_after: Option<i64>,
}
async fn history_inventory(
    conn: &mut AnyConnection,
    id: &str,
) -> Result<HistoryInspection, String> {
    history_page(conn, id, 0, 100).await
}
async fn history_page(
    conn: &mut AnyConnection,
    id: &str,
    after: i64,
    limit: i64,
) -> Result<HistoryInspection, String> {
    if after < 0 || limit < 1 || limit > 100 {
        return Err("History cursor or limit invalid".into());
    }
    let h = family(conn, id)
        .await?
        .ok_or("Attachment family unavailable")?;
    if h.state != "active" || h.revision <= 0 {
        return Err("Attachment family state unavailable".into());
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM attachment_version_records WHERE logical_attachment_id=$1",
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| e.to_string())?;
    let range:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachment_version_records WHERE logical_attachment_id=$1 AND version_number>=1 AND version_number<=$2").bind(id).bind(h.revision).fetch_one(&mut*conn).await.map_err(|e|e.to_string())?;
    if count != h.revision || range != h.revision {
        return Err("Registry immutable sequence differs".into());
    }
    let first_id:String=sqlx::query_scalar("SELECT version_id FROM attachment_version_records WHERE logical_attachment_id=$1 AND version_number=1").bind(id).fetch_one(&mut*conn).await.map_err(|e|e.to_string())?;
    let first = version(conn, &first_id)
        .await?
        .ok_or("Initial immutable version absent")?;
    let current = version(conn, &h.current_version_id)
        .await?
        .ok_or("Current immutable version absent")?;
    for v in [&first, &current] {
        validated_version(conn, v).await?;
        if v.logical_attachment_id != h.id
            || v.original_document_id != h.document_id
        {
            return Err("Family immutable owner differs".into());
        }
    }
    if first.original_space_id != h.space_id || first.original_created_at != h.created_at || current.version_number != h.revision {
        return Err("Registry original/current head differs".into());
    }
    let ids:Vec<String>=sqlx::query_scalar("SELECT version_id FROM attachment_version_records WHERE logical_attachment_id=$1 AND version_number>$2 ORDER BY version_number LIMIT $3").bind(id).bind(after).bind(limit).fetch_all(&mut*conn).await.map_err(|e|e.to_string())?;
    let mut versions = Vec::with_capacity(ids.len());
    for id in ids {
        let v = version(conn, &id).await?.ok_or("Version disappeared")?;
        validated_version(conn, &v).await?;
        if v.logical_attachment_id != h.id
            || v.original_document_id != h.document_id
        {
            return Err("Version immutable owner differs".into());
        }
        versions.push(v);
    }
    Ok(HistoryInspection {
        head: h,
        versions,
        first,
        current,
        after,
        limit,
    })
}
// The public history total/cursor describe only currently authorized versions.
// Preserve the existing per-page registry validation and finite registry bound.
async fn complete_history(conn:&mut AnyConnection,id:&str)->Result<HistoryInspection,String> {
    let mut source=history_page(conn,id,0,100).await?;
    if source.head.revision>100000 {return Err("Registry inspection row bound exceeded".into())}
    while source.versions.len() < source.head.revision as usize {
        let after=source.versions.last().ok_or("History inventory made no progress")?.version_number;
        let page=history_page(conn,id,after,100).await?;
        if page.head!=source.head || page.first!=source.first || page.current!=source.current || page.versions.is_empty() {
            return Err("Attachment history source changed".into())
        }
        source.versions.extend(page.versions);
    }
    if source.versions.len()!=source.head.revision as usize {return Err("Registry immutable sequence differs".into())}
    Ok(source)
}
#[cfg(test)]
tokio::task_local! {pub(crate) static HISTORY_BEFORE_FINAL:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;}
#[cfg(test)]
async fn history_before_final() {
    let f = HISTORY_BEFORE_FINAL
        .try_with(|s| s.borrow_mut().take())
        .ok()
        .flatten();
    if let Some(f) = f {
        f.await;
    }
}
pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/spaces/{space}/documents/{doc}/attachment-families/{logical}/versions",
            get(read_history).post(crate::spaces::attachments::upload_attachment_replacement),
        )
        .route("/spaces/{space}/documents/{doc}/attachment-families/{logical}/versions/{version}/file",get(crate::spaces::attachments::serve_historical_version))
        .route("/spaces/{space}/documents/{doc}/attachment-versions/{version}",get(read_pinned_version))
        .layer(axum::extract::DefaultBodyLimit::disable())
}
async fn read_history(
    State(s): State<AppState>, auth: AuthUser,
    Path((space, doc, logical)): Path<(String, String, String)>, Query(query): Query<HistoryQuery>,
) -> Result<Json<HistoryPage>, AppError> {
    let unavailable=||AppError::NotFound("Attachment family unavailable".into());
    let mut conn=s.db.acquire().await.map_err(AppError::internal)?;
    crate::spaces::access::require_current_human_read(&mut conn,&auth).await?;
    let after=query.after.unwrap_or(0);let limit=query.limit.unwrap_or(100);
    if after<0 || !(1..=100).contains(&limit) {return Err(AppError::BadRequest("History cursor or limit invalid".into()))}
    let source=complete_history(&mut conn,&logical).await.map_err(AppError::Conflict)?;
    if source.head.document_id!=doc {return Err(unavailable())}
    let mut visible=Vec::new();let mut audience_proofs=Vec::new();
    for version in &source.versions {
        match crate::spaces::asset_audiences::read(&mut conn,&auth,&space,&doc,&version.version_id).await {
            Ok(proof)=>{visible.push(version.clone());audience_proofs.push((version.version_id.clone(),proof));},
            Err(AppError::NotFound(_)|AppError::Forbidden(_))=>(),
            Err(error)=>return Err(error),
        }
    }
    if visible.is_empty() {return Err(unavailable())}
    let total=visible.len() as i64;
    let versions:Vec<_>=visible.iter().filter(|v|v.version_number>after).take(limit as usize).cloned().collect();
    let next_after=versions.last().map(|v|v.version_number).filter(|last|visible.iter().any(|v|v.version_number>*last));
    let root=crate::storage::data_dir();
    let mut bindings=Vec::new();let mut sql_proofs=Vec::new();let mut seen=std::collections::BTreeSet::new();
    let mut retained=std::collections::BTreeMap::new();
    // Keep old first/current physical/retirement guards even when those rows
    // are hidden. Only authorized emitted rows may enter the response.
    for version in std::iter::once(&source.first).chain(std::iter::once(&source.current)).chain(versions.iter()) {
        if !seen.insert(version.version_id.clone()) {continue}
        if let Some(retirement)=super::attachment_retirement::read(&mut conn,&version.version_id).await.map_err(AppError::Conflict)? {
            retained.insert(version.version_id.clone(),retirement);
        }
        let mut binding=capture_current_version(&root,&mut conn,version).await.map_err(AppError::Conflict)?;
        if binding.space_id!=space || binding.document_id!=doc {return Err(unavailable())}
        binding.append_sql_proofs(&mut sql_proofs);bindings.push(binding);
    }
    #[cfg(test)] history_before_final().await;
    if complete_history(&mut conn,&logical).await.map_err(AppError::Conflict)?!=source {
        return Err(AppError::Conflict("Attachment history source changed".into()))
    }
    for id in &seen {
        if super::attachment_retirement::read(&mut conn,id).await.map_err(AppError::Conflict)?.as_ref()!=retained.get(id) {
            return Err(AppError::Conflict("Retirement history changed".into()))
        }
    }
    crate::spaces::published_source::verify_source(&mut conn,&sql_proofs).await?;
    for (id,proof) in &audience_proofs {
        crate::spaces::asset_audiences::verify_read(&mut conn,&auth,&space,&doc,id,proof).await?;
    }
    crate::spaces::access::require_current_human_read(&mut conn,&auth).await?;
    for binding in &bindings {binding.fence_files().map_err(AppError::Conflict)?;}
    let authorized:std::collections::BTreeSet<_>=visible.iter().map(|v|v.version_id.as_str()).collect();
    let retained_versions=retained.into_iter().filter(|(id,_)|authorized.contains(id.as_str())).collect();
    let current_head=authorized.contains(source.current.version_id.as_str()).then(||AuthorizedHistoryHead {version_id:source.current.version_id.clone(),revision:source.head.revision});
    Ok(Json(HistoryPage{schema_version:2,current_head,retained_versions,versions,total,next_after}))
}

include!("attachment_replacement.inc");

/// Opaque result of fresh-only publication or an exact unregistered legacy ACK.
/// Absence does not distinguish legacy from historical registry deletion.
pub(crate) struct SingletonPublication {
    registered: bool,
}
async fn verify_unregistered_ack(
    conn: &mut AnyConnection,
    p: &PreparedSingleton,
) -> Result<(), String> {
    let raw = receipt_raw(conn, &p.receipt).await?;
    if VersionRecord::from_receipt(&p.receipt, &raw) != p.version
        || !upload::association(conn, &p.receipt).await?
    {
        return Err("Unregistered original upload association differs".into());
    }
    let n:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM attachment_families WHERE id=$1 OR current_version_id=$1)+(SELECT COUNT(*) FROM attachment_version_records WHERE version_id=$1 OR logical_attachment_id=$1 OR upload_operation_id=$2)+(SELECT COUNT(*) FROM attachment_version_changes WHERE family_id=$1 OR operation_id=$2)")
        .bind(&p.version.version_id).bind(&p.version.upload_operation_id).fetch_one(&mut *conn).await.map_err(|e|e.to_string())?;
    if n != 0 {
        return Err("Unregistered upload has partial historical registry evidence".into());
    }
    Ok(())
}
pub(crate) async fn verify_singleton_publication(
    conn: &mut AnyConnection,
    p: &PreparedSingleton,
    admission: &SingletonPublication,
) -> Result<(), String> {
    if admission.registered {
        verify_singleton(conn, p).await
    } else {
        verify_unregistered_ack(conn, p).await
    }
}

include!("attachment_version_current.inc");

include!("attachment_registry_inspection.inc");

#[cfg(test)] tokio::task_local!{pub(crate) static HISTORY_DOWNLOAD_BEFORE_FINAL:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;}
#[cfg(test)] pub(crate) async fn history_download_wait(){let f=HISTORY_DOWNLOAD_BEFORE_FINAL.try_with(|s|s.borrow_mut().take()).ok().flatten();if let Some(f)=f{f.await;}}

pub(crate) async fn logical_attachment_id(conn:&mut AnyConnection,id:&str)->Result<Option<String>,String>{
    let Some(v)=version(conn,id).await? else{return Ok(None)};
    validated_version(conn,&v).await?;
    let h=family(conn,&v.logical_attachment_id).await?.ok_or("Version family absent")?;
    if h.state!="active"||v.version_number<1||v.version_number>h.revision||v.original_document_id!=h.document_id{return Err("Version family metadata differs".into())}
    Ok(Some(v.logical_attachment_id))
}

#[derive(Serialize)]
struct PinnedVersionMetadata {
    logical_attachment_id:String,
    version:VersionRecord,
    current_space_id:String,
    current_document_id:String,
    current_storage_path:String,
    retirement:Option<super::attachment_retirement::Retirement>,
}
async fn read_pinned_version(State(s):State<AppState>,auth:AuthUser,Path((space,doc,id)):Path<(String,String,String)>)->Result<Json<PinnedVersionMetadata>,AppError>{
    let unavailable=||AppError::NotFound("Attachment version unavailable".into());
    let mut conn=s.db.acquire().await.map_err(AppError::internal)?;
    crate::spaces::access::require_current_human_read(&mut conn,&auth).await?;
    let audience=crate::spaces::asset_audiences::read(&mut conn,&auth,&space,&doc,&id).await?;
    let mut proofs=Vec::new();
    let selected=version(&mut conn,&id).await.map_err(|_|unavailable())?.ok_or_else(unavailable)?;
    let (v,mut binding,family_proof)=capture_historical_version(&crate::storage::data_dir(),&mut conn,&selected.logical_attachment_id,&id).await.map_err(|_|unavailable())?;
    if binding.space_id!=space||binding.document_id!=doc||binding.asset_id!=id{return Err(unavailable())}
    let retirement=super::attachment_retirement::read(&mut conn,&id).await.map_err(|_|unavailable())?;
    proofs.push(family_proof);binding.append_sql_proofs(&mut proofs);
    #[cfg(test)] history_before_final().await;
    crate::spaces::published_source::verify_source(&mut conn,&proofs).await.map_err(|e|match e{AppError::Conflict(_)=>unavailable(),e=>e})?;
    crate::spaces::asset_audiences::verify_read(&mut conn,&auth,&space,&doc,&id,&audience).await?;
    crate::spaces::access::require_current_human_read(&mut conn,&auth).await?;
    binding.fence_files().map_err(|_|unavailable())?;
    Ok(Json(PinnedVersionMetadata{logical_attachment_id:v.logical_attachment_id.clone(),version:v,current_space_id:binding.space_id,current_document_id:binding.document_id,current_storage_path:binding.storage_path,retirement}))
}

#[cfg(test)]
pub(crate) async fn test_replacement_receipt_shape_in(conn:&mut AnyConnection,op:&str)->Result<(),String>{replacement_change(conn,op).await?.ok_or_else(||"Original typed replacement shape absent".into()).map(|_|())}

// Snapshot-only original singleton validation: no native/pager/serving holder.
// Wider current/replaced/retired preservation requires its separate full proof.
pub(crate) async fn validate_composite_preserved_singleton_in(conn:&mut AnyConnection,r:&Receipt)->Result<(),String>{
 let raw=receipt_raw(conn,r).await?;let v=VersionRecord::from_receipt(r,&raw);validated_version(conn,&v).await?;let h=history_inventory(conn,&v.logical_attachment_id).await?;
 if h.head.revision!=1||h.first!=v||h.current!=v||h.head.current_version_id!=v.version_id||!upload::association(conn,r).await?{return Err("Composite committed family needs distinct full history preservation proof".into());}Ok(())
}
