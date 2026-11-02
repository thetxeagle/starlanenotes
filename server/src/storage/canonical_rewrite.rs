//! Non-draft immutable canonical rewrites. UNWIRED: callers own transaction/ordered
//! space and page locks, conditional canonical SQL, then this same-TX receipt.
//! Never changes published_at, promotes drafts, deletes bytes, or replays history.
use super::journal_fs::{self, FileDescriptor};
use crate::{
    auth::{middleware::AuthUser, routes::AppError},
    spaces::access,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, AnyPool};
use std::path::Path;
use uuid::Uuid;
#[cfg(target_os="linux")]
type AdmissionLease = super::attachment_upload::admission::WriterLease;
#[cfg(not(target_os="linux"))]
struct AdmissionLease;
const NS: &str = ".canonical-rewrite-journal";
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RewriteReason {
    ReplayContent,
    MetadataPresentation,
    AssetReferenceRewrite,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct IncomingIdentity {
    pub provider: String,
    pub operation_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub action: String,
    pub payload_fingerprint: String,
}
#[derive(Clone, Debug)]
pub struct RewriteIntent {
    pub space_id: String,
    pub document_id: String,
    pub source_path: String,
    pub source_revision: String,
    pub published_stamp: String,
    pub revision: String,
    pub source_sha256: Option<String>,
    pub reason: RewriteReason,
    pub incoming: IncomingIdentity,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreparedRewrite {
    #[serde(flatten)]
    file: FileDescriptor,
    kind: String,
    actor_id: String,
    source_path: String,
    source_revision: String,
    published_stamp: String,
    source_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    result_source_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    original_source_digest: Option<String>,
    reason: RewriteReason,
    incoming: IncomingIdentity,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PublishedSourceEvidence {
    pub published_source_digest:String, pub payload_fingerprint:String,
    pub document_id:String, pub space_id:String, pub original_source_digest:String,
}
impl PreparedRewrite {
    pub fn published_source_evidence(&self)->Option<PublishedSourceEvidence> {
        if self.reason!=RewriteReason::ReplayContent {return None;}
        Some(PublishedSourceEvidence{published_source_digest:self.result_source_digest.clone()?,payload_fingerprint:self.incoming.payload_fingerprint.clone(),document_id:self.file.document_id.clone(),space_id:self.file.space_id.clone(),original_source_digest:self.original_source_digest.clone()?})
    }

    pub fn result_source_digest(&self)->Option<&str> {self.result_source_digest.as_deref()}
    pub fn path(&self) -> &str {
        &self.file.path
    }
    pub fn revision(&self) -> &str {
        &self.file.revision
    }
    pub fn op_id(&self) -> &str {
        &self.file.op_id
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitEvidence {
    Committed,
    NotCommitted,
}
fn conflict(s: &str) -> AppError {
    AppError::Conflict(format!("Canonical rewrite needs review: {s}"))
}
fn io(e: std::io::Error) -> AppError {
    conflict(&e.to_string())
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn hex_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}
fn validate(m: &PreparedRewrite) -> Result<(), AppError> {
    if serde_json::to_vec(m).map_err(AppError::internal)?.len() > 65_536 {
        return Err(AppError::BadRequest(
            "Canonical rewrite intent exceeds 65536-byte manifest limit".into(),
        ));
    }
    if m.original_source_digest.is_some()!=m.result_source_digest.is_some()
        || m.original_source_digest.as_ref().is_some_and(|s|!hex_hash(s))
        || m.result_source_digest.as_ref().is_some_and(|s|!hex_hash(s))
        || (m.result_source_digest.is_some() && (m.reason!=RewriteReason::ReplayContent || m.source_sha256.is_none())) {return Err(conflict("invalid published result evidence"));}
    let i = &m.incoming;
    if m.kind != "canonical-rewrite"
        || m.file.version != 3
        || Uuid::parse_str(&m.file.op_id).is_err()
        || m.file.length < 0
        || m.file.length > 6_000_000
        || !hex_hash(&m.file.sha256)
        || !hex_hash(&i.payload_fingerprint)
        || m.actor_id.is_empty()
        || m.file.space_id.is_empty()
        || m.file.document_id.is_empty()
        || m.source_revision.is_empty()
        || m.file.revision.is_empty()
        || m.published_stamp.is_empty()
        || i.provider.is_empty()
        || i.operation_id.is_empty()
        || i.entity_id.is_empty()
        || m.source_sha256.as_ref().is_some_and(|s| !hex_hash(s))
        || matches!(m.reason,RewriteReason::AssetReferenceRewrite | RewriteReason::MetadataPresentation) && m.source_sha256.is_none()
    {
        return Err(conflict("invalid intent"));
    }
    match m.reason {
        RewriteReason::ReplayContent
            if i.entity_type == "document"
                && i.entity_id == m.file.document_id
                && matches!(
                    i.action.as_str(),
                    "update_content"
                        | "resolve_conflict:merge"
                        | "resolve_conflict:keep-local"
                        | "resolve_conflict:keep-remote"
                ) => {}
        RewriteReason::MetadataPresentation if i.entity_type == "document" && i.entity_id == m.file.document_id && i.action == "update" => {},
        RewriteReason::AssetReferenceRewrite
            if i.entity_type == "asset" && i.action == "create" => {}
        _ => return Err(conflict("incoming operation differs from rewrite reason")),
    }
    let slug = m.file.path.split('/').nth(1).unwrap_or_default();
    if !super::publication::is_publication_revision_path(&m.file.path, slug)
        || !m.file.path.ends_with(&format!("/{}.md", m.file.op_id))
        || m.file.path == m.source_path
        || !super::is_safe_relative_path(Path::new(&m.source_path))
    {
        return Err(conflict("invalid immutable path"));
    }
    Ok(())
}
async fn proof(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    m: &PreparedRewrite,
) -> Result<(), AppError> {
    access::resolve_page_access(
        conn,
        access::principal_from_auth(auth)?,
        &m.file.space_id,
        &m.file.document_id,
        access::PageOperation::Edit,
        access::Purpose::Routine,
    )
    .await?;
    Ok(())
}
async fn source(
    conn: &mut AnyConnection,
    space: &str,
    doc: &str,
) -> Result<(String, String, String, String), AppError> {
    sqlx::query_as("SELECT d.content_path,d.updated_at,d.published_at,s.slug FROM documents d JOIN spaces s ON s.id=d.space_id WHERE d.id=$1 AND d.space_id=$2 AND d.published_at IS NOT NULL AND d.archived_at IS NULL AND d.deleted_at IS NULL")
 .bind(doc).bind(space).fetch_optional(conn).await.map_err(AppError::internal)?.ok_or_else(||conflict("source unavailable"))
}
fn source_hash(root: &Path, m: &PreparedRewrite) -> Result<(), AppError> {
    if let Some(expected) = &m.source_sha256 {
        if digest(&journal_fs::source_bytes(root, &m.source_path).map_err(io)?) != *expected {
            return Err(conflict("source bytes changed"));
        }
    }
    Ok(())
}
/// EXACT supplied bytes, with a new internally generated UUID unrelated to legacy IDs.
/// Acquire ordered space/page locks before calling. No canonical association yet.
pub async fn prepare_rewrite(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    intent: RewriteIntent,
    bytes: &[u8],
) -> Result<PreparedRewrite, AppError> {
    prepare_in(&super::data_dir(), conn, auth, intent, bytes).await
}
async fn prepare_in(
    root: &Path,
    conn: &mut AnyConnection,
    auth: &AuthUser,
    i: RewriteIntent,
    bytes: &[u8],
) -> Result<PreparedRewrite, AppError> {
    prepare_source_in(root,conn,auth,i,bytes,None).await
}
/// Exact rooted no-follow original bytes, data only; grants no access.
pub fn published_source_bytes(path:&str)->Result<Vec<u8>,AppError> {journal_fs::source_bytes(&super::data_dir(),path).map_err(io)}
/// New source-bound replay only. Existing metadata/asset and old manifest shapes
/// keep absent optional fields and their original byte representation.
pub async fn prepare_published_replay(conn:&mut AnyConnection,auth:&AuthUser,intent:RewriteIntent,original:&str,bytes:&[u8])->Result<PreparedRewrite,AppError> {
    crate::spaces::published_source::required(Some(&serde_json::json!(original)))?;
    if intent.reason!=RewriteReason::ReplayContent {return Err(conflict("not a published replay"));}
    prepare_source_in(&super::data_dir(),conn,auth,intent,bytes,Some(original)).await
}
async fn prepare_source_in(root:&Path,conn:&mut AnyConnection,auth:&AuthUser,i:RewriteIntent,bytes:&[u8],original:Option<&str>)->Result<PreparedRewrite,AppError> {
    prepare_source_admission_in(root,conn,auth,i,bytes,original,None).await
}
#[cfg(target_os="linux")]
pub(crate) async fn prepare_asset_replay_with_lease(conn:&mut AnyConnection,auth:&AuthUser,intent:RewriteIntent,bytes:&[u8],lease:&super::attachment_upload::admission::WriterLease)->Result<PreparedRewrite,AppError> {
    prepare_source_admission_in(&super::data_dir(),conn,auth,intent,bytes,None,Some(lease)).await
}
async fn prepare_source_admission_in(root:&Path,conn:&mut AnyConnection,auth:&AuthUser,i:RewriteIntent,bytes:&[u8],original:Option<&str>,lease:Option<&AdmissionLease>)->Result<PreparedRewrite,AppError> {
    if bytes.len() > 6_000_000 {
        return Err(AppError::BadRequest(
            "Canonical rewrite exceeds storage journal limit".into(),
        ));
    }
    let (path, revision, published, slug) = source(conn, &i.space_id, &i.document_id).await?;
    if path != i.source_path
        || revision != i.source_revision
        || published != i.published_stamp
        || !path.starts_with(&format!("spaces/{slug}/docs/"))
    {
        return Err(conflict("source identity changed"));
    }
    let result_row=if original.is_some() {Some(crate::spaces::published_source::selected(conn,&i.space_id,&i.document_id).await?)}else{None};
    let op = Uuid::new_v4().to_string();
    #[cfg(target_os="linux")]
    if let Some(held)=lease {held.bind_replay_rewrite_plan(root,&op,&auth.claims.sub,&i,bytes).map_err(io)?;}
    let mut m = PreparedRewrite {
        file: FileDescriptor {
            op_id: op.clone(),
            space_id: i.space_id,
            document_id: i.document_id,
            path: format!("spaces/{slug}/docs/revisions/{op}.md"),
            revision: i.revision,
            sha256: digest(bytes),
            length: bytes.len() as i64,
            version: 3,
        },
        kind: "canonical-rewrite".into(),
        actor_id: auth.claims.sub.clone(),
        source_path: path,
        source_revision: revision,
        published_stamp: published,
        source_sha256: i.source_sha256,
        result_source_digest: None,
        original_source_digest: original.map(str::to_owned),
        reason: i.reason,
        incoming: i.incoming,
    };
    if let Some(row)=result_row {let expected=crate::spaces::published_source::expected(&row,m.path(),&m.actor_id,m.revision());m.result_source_digest=Some(crate::spaces::published_source::digest(&expected,bytes)?);}
    validate(&m)?;
    proof(conn, auth, &m).await?;
    source_hash(root, &m)?;
    if source(conn, &m.file.space_id, &m.file.document_id).await?
        != (
            m.source_path.clone(),
            m.source_revision.clone(),
            m.published_stamp.clone(),
            slug,
        )
    {
        return Err(conflict("source changed after authorization"));
    }
    let raw_manifest=serde_json::to_vec(&m).map_err(AppError::internal)?;
    #[cfg(target_os="linux")]
    if let Some(held)=lease {journal_fs::stage_replay_file_with_lease(root,NS,&m.file,&raw_manifest,bytes,held).map_err(io)?;}
    else {journal_fs::stage_file(root,NS,&m.file,&raw_manifest,bytes).map_err(io)?;}
    #[cfg(not(target_os="linux"))]
    journal_fs::stage_file(root,NS,&m.file,&raw_manifest,bytes).map_err(io)?;
    install(root, &m)?;
    Ok(m)
}
fn load(root: &Path, op: &str) -> Result<PreparedRewrite, AppError> {
    let m: PreparedRewrite =
        serde_json::from_slice(&journal_fs::manifest_bytes(root, NS, op).map_err(io)?)
            .map_err(|_| conflict("corrupt manifest"))?;
    validate(&m)?;
    if m.file.op_id != op {
        return Err(conflict("manifest operation differs"));
    }
    Ok(m)
}
fn install(root: &Path, m: &PreparedRewrite) -> Result<(), AppError> {
    if load(root, m.op_id())? != *m {
        return Err(conflict("manifest differs"));
    }
    journal_fs::publish_file(root, NS, &m.file).map_err(io)
}
async fn canonical(conn: &mut AnyConnection, m: &PreparedRewrite) -> Result<(), AppError> {
    let row:Option<(String,String,String,String)>=sqlx::query_as("SELECT content_path,updated_at,last_updated_by,published_at FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL").bind(&m.file.document_id).bind(&m.file.space_id).fetch_optional(conn).await.map_err(AppError::internal)?;
    if row
        != Some((
            m.file.path.clone(),
            m.file.revision.clone(),
            m.actor_id.clone(),
            m.published_stamp.clone(),
        ))
    {
        return Err(conflict("canonical transition differs"));
    }
    Ok(())
}
/// Pure final filesystem proof for callers which must finish additional scoped
/// association SQL after recording the canonical receipt. Grants no authority.
pub fn verify_prepared_files(m: &PreparedRewrite) -> Result<(), AppError> {
    verify_prepared_files_in(&super::data_dir(), m)
}
pub(crate) fn verify_prepared_files_in(root: &Path, m: &PreparedRewrite) -> Result<(), AppError> {
    validate(m)?;
    if load(root, m.op_id())? != *m {
        return Err(conflict("manifest differs"));
    }
    source_hash(root, m)?;
    journal_fs::verify_file(root, NS, &m.file).map_err(io)?;
    if load(root, m.op_id())? != *m {
        return Err(conflict("manifest differs"));
    }
    Ok(())
}
/// Caller must have conditionally transitioned from expected old path/revision/stamp.
/// This does not perform the transition. Call inside that SAME transaction.
pub async fn record_rewrite(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    m: &PreparedRewrite,
) -> Result<(), AppError> {
    record_in(&super::data_dir(), conn, auth, m).await
}
async fn record_in(
    root: &Path,
    conn: &mut AnyConnection,
    auth: &AuthUser,
    m: &PreparedRewrite,
) -> Result<(), AppError> {
    validate(m)?;
    if auth.claims.sub != m.actor_id {
        return Err(conflict("actor differs"));
    }
    proof(conn, auth, m).await?;
    canonical(conn, m).await?;
    source_hash(root, m)?;
    if let Some(expected)=m.result_source_digest.as_ref() {
        let row=crate::spaces::published_source::selected(conn,&m.file.space_id,&m.file.document_id).await?;
        let bytes=journal_fs::source_bytes(root,m.path()).map_err(io)?;
        if crate::spaces::published_source::digest(&row,&bytes)?!=*expected {return Err(conflict("full result source changed"));}
    }
    install(root, m)?;
    let i = &m.incoming;
    sqlx::query("INSERT INTO canonical_revision_receipts(op_id,space_id,document_id,actor_id,reason,provider,incoming_id,entity_type,entity_id,action,payload_fingerprint,source_path,source_revision,published_stamp,source_sha256,content_path,revision,sha256,byte_length,manifest_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20)")
 .bind(m.op_id()).bind(&m.file.space_id).bind(&m.file.document_id).bind(&m.actor_id).bind(serde_json::to_string(&m.reason).map_err(AppError::internal)?).bind(&i.provider).bind(&i.operation_id).bind(&i.entity_type).bind(&i.entity_id).bind(&i.action).bind(&i.payload_fingerprint).bind(&m.source_path).bind(&m.source_revision).bind(&m.published_stamp).bind(&m.source_sha256).bind(m.path()).bind(m.revision()).bind(&m.file.sha256).bind(m.file.length).bind(serde_json::to_string(m).map_err(AppError::internal)?).execute(&mut *conn).await.map_err(AppError::internal)?;
    proof(conn, auth, m).await?;
    canonical(conn, m).await?;
    source_hash(root, m)?;
    journal_fs::verify_file(root, NS, &m.file).map_err(io)?;
    if receipt(conn, m.op_id()).await?.as_ref() != Some(m) {
        return Err(conflict("receipt changed"));
    }
    // Last check after awaited receipt reads: file projection alone cannot bind
    // actor/source/reason/incoming metadata in the full immutable envelope.
    if load(root, m.op_id())? != *m {
        return Err(conflict("full manifest changed during receipt write"));
    }
    source_hash(root, m)?;
    journal_fs::verify_file(root, NS, &m.file).map_err(io)?;
    if load(root, m.op_id())? != *m {
        return Err(conflict(
            "full manifest changed during final file verification",
        ));
    }
    Ok(())
}
pub async fn verify_recorded(conn:&mut AnyConnection,m:&PreparedRewrite)->Result<(),AppError> {
    if receipt(conn,m.op_id()).await?.as_ref()!=Some(m) {return Err(conflict("receipt changed"));}Ok(())
}
/// Read-only historical classification, not restoration eligibility or access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RestorationOperationClass {
    PreparedUncommitted,
    CommittedCurrent,
    CommittedHistorical,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RestorationInspection {
    pub(super) manifest: PreparedRewrite,
    pub(super) raw_manifest: Vec<u8>,
    pub(super) raw_receipt: Option<String>,
    pub(super) class: RestorationOperationClass,
    pub(super) install: journal_fs::InstallInspection,
}
impl RestorationInspection {
    /// Exact historical dependency only. Does not certify a current source,
    /// publish bytes, acknowledge an asset without its SQL proof, or grant access.
    pub(crate) fn matches_asset_receipt(&self, r: &crate::sync::AssetReplayReceipt) -> bool {
        let m = &self.manifest;
        self.class != RestorationOperationClass::PreparedUncommitted
            && m.reason == RewriteReason::AssetReferenceRewrite
            && m.incoming == r.incoming
            && m.actor_id == r.actor_id
            && m.file.space_id == r.space_id
            && m.file.document_id == r.document_id
            && r.source_kind == "published"
            && r.source_id.as_deref() == Some(m.file.document_id.as_str())
            && r.source_revision.as_deref() == Some(m.file.revision.as_str())
            && r.body_sha256.as_deref() == Some(m.file.sha256.as_str())
    }
}
fn inspection_envelope(raw: &[u8]) -> Result<PreparedRewrite, AppError> {
    let m: PreparedRewrite = serde_json::from_slice(raw).map_err(|_| conflict("corrupt inspection envelope"))?;
    validate(&m)?;
    let value: serde_json::Value = serde_json::from_slice(raw).map_err(|_| conflict("corrupt inspection envelope"))?;
    let object = value.as_object().ok_or_else(|| conflict("invalid inspection envelope"))?;
    const FIELDS: &[&str] = &["op_id", "space_id", "document_id", "path", "revision", "sha256", "length", "version", "kind", "actor_id", "source_path", "source_revision", "published_stamp", "source_sha256", "result_source_digest", "original_source_digest", "reason", "incoming"];
    if object.keys().any(|k| !FIELDS.contains(&k.as_str())) {
        return Err(conflict("unknown inspection envelope field"));
    }
    let incoming = object.get("incoming").and_then(serde_json::Value::as_object)
        .ok_or_else(|| conflict("invalid incoming envelope"))?;
    const INCOMING: &[&str] = &["provider","operation_id","entity_type","entity_id","action","payload_fingerprint"];
    if incoming.keys().any(|k| !INCOMING.contains(&k.as_str())) {
        return Err(conflict("unknown incoming envelope field"));
    }

    Ok(m)
}
async fn inspection_association(conn: &mut AnyConnection, m: &PreparedRewrite)
    -> Result<Option<(String,String,String,String)>,AppError> {
    sqlx::query_as("SELECT content_path,updated_at,last_updated_by,published_at FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(&m.file.document_id).bind(&m.file.space_id).fetch_optional(conn).await.map_err(AppError::internal)
}
/// Requires an authoritative stopped-write snapshot or a caller-guarded
/// transaction. Never installs/repoints files, changes SQL or grants authority.
pub(crate) async fn inspect_restoration(root: &Path, conn: &mut AnyConnection, op: &str)
    -> Result<RestorationInspection, AppError> {
    let raw_manifest = journal_fs::manifest_bytes(root, NS, op).map_err(io)?;
    let m = inspection_envelope(&raw_manifest)?;
    if m.file.op_id != op || load(root, op)? != m {return Err(conflict("inspection manifest identity differs"));}
    let stored = receipt(conn, op).await?;
    let raw_receipt: Option<String> = sqlx::query_scalar("SELECT manifest_json FROM canonical_revision_receipts WHERE op_id=$1")
        .bind(op).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    match (&stored, &raw_receipt) {
        (None,None) => {},
        (Some(stored),Some(raw)) if *stored == m && inspection_envelope(raw.as_bytes())? == m => {},
        _ => return Err(conflict("inspection SQL receipt differs")),
    }
    let association = inspection_association(conn,&m).await?;
    let class = if stored.is_none() {RestorationOperationClass::PreparedUncommitted}
        else if association == Some((m.file.path.clone(),m.file.revision.clone(),m.actor_id.clone(),m.published_stamp.clone())) {
            RestorationOperationClass::CommittedCurrent
        } else {RestorationOperationClass::CommittedHistorical};
    // Immutable result paths are required even for superseded/deleted history;
    // an absent current canonical association never authorizes reconstruction.
    let install = journal_fs::inspect_install(root, NS, &m.file, true).map_err(io)?;
    if stored.is_some() && (install.proof.is_none() || !install.installed_present
        || !install.paths.iter().any(|p| p.relative == format!("{NS}/{op}/install-anchor"))) {
        return Err(conflict("committed immutable install evidence missing"));
    }
    if receipt(conn,op).await? != stored
        || sqlx::query_scalar::<_,String>("SELECT manifest_json FROM canonical_revision_receipts WHERE op_id=$1")
            .bind(op).fetch_optional(&mut *conn).await.map_err(AppError::internal)? != raw_receipt
        || inspection_association(conn,&m).await? != association {
        return Err(conflict("inspection SQL changed"));
    }
    if journal_fs::inspect_install(root, NS, &m.file,true).map_err(io)? != install
        || journal_fs::manifest_bytes(root,NS,op).map_err(io)? != raw_manifest {
        return Err(conflict("inspection immutable evidence changed"));
    }
    Ok(RestorationInspection {manifest:m,raw_manifest,raw_receipt,class,install})
}
async fn receipt(conn: &mut AnyConnection, op: &str) -> Result<Option<PreparedRewrite>, AppError> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT manifest_json FROM canonical_revision_receipts WHERE op_id=$1")
            .bind(op)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    let Some(raw) = raw else { return Ok(None) };
    let m: PreparedRewrite = serde_json::from_str(&raw).map_err(|_| conflict("corrupt receipt"))?;
    validate(&m)?;
    let i = &m.incoming;
    let yes:Option<i64>=sqlx::query_scalar("SELECT 1 FROM canonical_revision_receipts WHERE op_id=$1 AND space_id=$2 AND document_id=$3 AND actor_id=$4 AND reason=$5 AND provider=$6 AND incoming_id=$7 AND entity_type=$8 AND entity_id=$9 AND action=$10 AND payload_fingerprint=$11 AND source_path=$12 AND source_revision=$13 AND published_stamp=$14 AND (source_sha256=$15 OR (source_sha256 IS NULL AND $15 IS NULL)) AND content_path=$16 AND revision=$17 AND sha256=$18 AND byte_length=$19")
 .bind(m.op_id()).bind(&m.file.space_id).bind(&m.file.document_id).bind(&m.actor_id).bind(serde_json::to_string(&m.reason).map_err(AppError::internal)?).bind(&i.provider).bind(&i.operation_id).bind(&i.entity_type).bind(&i.entity_id).bind(&i.action).bind(&i.payload_fingerprint).bind(&m.source_path).bind(&m.source_revision).bind(&m.published_stamp).bind(&m.source_sha256).bind(m.path()).bind(m.revision()).bind(&m.file.sha256).bind(m.file.length).fetch_optional(conn).await.map_err(AppError::internal)?;
    if yes.is_none() {
        return Err(conflict("receipt columns differ"));
    }
    Ok(Some(m))
}
async fn lock_evidence(conn: &mut AnyConnection, m: &PreparedRewrite) -> Result<(), AppError> {
    match crate::spaces::mutation_guards::lock_spaces_for_mutation(conn, &[&m.file.space_id]).await
    {
        Ok(()) | Err(AppError::NotFound(_)) => {}
        Err(e) => return Err(e),
    };
    sqlx::query("UPDATE canonical_revision_receipts SET verified_at=verified_at WHERE op_id=$1")
        .bind(m.op_id())
        .execute(conn)
        .await
        .map_err(AppError::internal)?;
    Ok(())
}
pub async fn confirm_committed(
    db: &AnyPool,
    m: &PreparedRewrite,
) -> Result<CommitEvidence, AppError> {
    let mut tx = db.begin().await.map_err(AppError::internal)?;
    lock_evidence(&mut tx, m).await?;
    let result = match receipt(&mut tx, m.op_id()).await? {
        None => CommitEvidence::NotCommitted,
        Some(s) if s == *m => CommitEvidence::Committed,
        Some(_) => return Err(conflict("receipt identity differs")),
    };
    tx.commit().await.map_err(AppError::internal)?;
    Ok(result)
}
/// A historical match acknowledges only the exact original actor/provider/payload.
/// No current page/draft authority is granted, and no bytes or canonical SQL replayed.
pub async fn committed_incoming(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    incoming: &IncomingIdentity,
) -> Result<Option<PreparedRewrite>, AppError> {
    access::require_current_human_preference_write(conn, auth).await?;
    let op: Option<String> = sqlx::query_scalar(
        "SELECT op_id FROM canonical_revision_receipts WHERE provider=$1 AND incoming_id=$2",
    )
    .bind(&incoming.provider)
    .bind(&incoming.operation_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    let Some(op) = op else {
        access::require_current_human_preference_write(conn, auth).await?;
        return Ok(None);
    };
    let m = receipt(conn, &op)
        .await?
        .ok_or_else(|| conflict("receipt missing"))?;
    if m.actor_id != auth.claims.sub || m.incoming != *incoming {
        return Err(conflict("incoming identity collision"));
    }
    access::require_current_human_preference_write(conn, auth).await?;
    // New result evidence is certified by the independently exclusive full
    // manifest/files, never mutable SQL labels alone. Old receipts stay ACK-only.
    if m.result_source_digest.is_some() {verify_prepared_files(&m)?;}
    Ok(Some(m))
}
#[cfg(test)]
thread_local! {static RECOVERY_VERIFY_HOOK:std::cell::RefCell<Option<Box<dyn FnOnce()>>>=std::cell::RefCell::new(None);}
fn verify_full_install(root: &Path, m: &PreparedRewrite) -> Result<(), AppError> {
    if load(root, m.op_id())? != *m {
        return Err(conflict("full recovery manifest changed"));
    }
    journal_fs::verify_file(root, NS, &m.file).map_err(io)?;
    if load(root, m.op_id())? != *m {
        return Err(conflict(
            "full recovery manifest changed during verification",
        ));
    }
    Ok(())
}
pub async fn recover(db: &AnyPool) -> Result<(), AppError> {
    recover_in(&super::data_dir(), db).await
}
async fn recover_in(root: &Path, db: &AnyPool) -> Result<(), AppError> {
    let pending: Vec<String> = sqlx::query_scalar(
        "SELECT op_id FROM canonical_revision_receipts WHERE verified_at IS NULL",
    )
    .fetch_all(db)
    .await
    .map_err(AppError::internal)?;
    let stages = journal_fs::inventory(root, NS, !pending.is_empty()).map_err(io)?;
    for op in &pending {
        if !stages.contains(op) {
            tracing::warn!(op_id=%op,"Canonical receipt missing stage; retained for review");
        }
    }
    for op in stages {
        let m = match load(root, &op) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(op_id=%op,error=?e,"Canonical stage retained for review");
                continue;
            }
        };
        let mut tx = db.begin().await.map_err(AppError::internal)?;
        lock_evidence(&mut tx, &m).await?;
        let stored = match receipt(&mut tx, &op).await {
            Ok(v) => v,
            Err(AppError::Conflict(e)) => {
                tracing::warn!(op_id=%op,error=%e,"Canonical receipt retained for review");
                tx.commit().await.map_err(AppError::internal)?;
                continue;
            }
            Err(e) => return Err(e),
        };
        if stored.as_ref() == Some(&m) {
            #[cfg(test)]
            RECOVERY_VERIFY_HOOK.with(|hook| {
                if let Some(f) = hook.borrow_mut().take() {
                    f();
                }
            });
            match verify_full_install(root, &m) {
                Ok(()) => {
                    sqlx::query(
                        "UPDATE canonical_revision_receipts SET verified_at=$1 WHERE op_id=$2",
                    )
                    .bind(chrono::Utc::now().to_rfc3339())
                    .bind(&op)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::internal)?;
                    // The verified marker SQL may itself wait. Do not acknowledge
                    // a different full envelope after that wait; retain evidence.
                    if let Err(e) = verify_full_install(root, &m) {
                        tracing::warn!(op_id=%op,error=?e,"Canonical full intent retained for review");
                        tx.rollback().await.map_err(AppError::internal)?;
                        continue;
                    }
                }
                Err(e) => {
                    tracing::warn!(op_id=%op,error=?e,"Canonical install retained for review")
                }
            }
        } else {
            tracing::warn!(op_id=%op,"Canonical orphan/mismatch retained for review");
        }
        tx.commit().await.map_err(AppError::internal)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::AppState;
    use std::fs;
    fn root() -> std::path::PathBuf {
        let r = std::env::temp_dir().join(format!("rewrite-{}", Uuid::new_v4()));
        fs::create_dir_all(&r).unwrap();
        r
    }
    async fn state() -> (AppState, AuthUser, std::path::PathBuf, RewriteIntent) {
        let s = crate::spaces::properties::tests::property_test_state().await;
        let mut a = crate::spaces::documents::tests::test_auth("owner-a");
        a.claims.sid = "rewrite-owner".into();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('rewrite-owner','owner-a','test','2999-01-01T00:00:00Z')").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE spaces SET slug='a' WHERE id='space-a'")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET content_path='spaces/a/docs/old.md',updated_at='source',published_at='published' WHERE id='target-a'").execute(&s.db).await.unwrap();
        let r = root();
        fs::create_dir_all(r.join("spaces/a/docs")).unwrap();
        fs::write(
            r.join("spaces/a/docs/old.md"),
            b"---\ntitle: Old\n---\nPUBLIC OLD",
        )
        .unwrap();
        let i = RewriteIntent {
            space_id: "space-a".into(),
            document_id: "target-a".into(),
            source_path: "spaces/a/docs/old.md".into(),
            source_revision: "source".into(),
            published_stamp: "published".into(),
            revision: "new".into(),
            source_sha256: None,
            reason: RewriteReason::ReplayContent,
            incoming: IncomingIdentity {
                provider: "server".into(),
                operation_id: "legacy:not-a-uuid".into(),
                entity_type: "document".into(),
                entity_id: "target-a".into(),
                action: "update_content".into(),
                payload_fingerprint: digest(b"raw intent"),
            },
        };
        (s, a, r, i)
    }
    async fn begin(s: &AppState) -> sqlx::Transaction<'_, sqlx::Any> {
        let mut tx = s.db.begin().await.unwrap();
        crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &["space-a"])
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET id=id WHERE id='target-a' AND space_id='space-a'")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx
    }
    async fn transition(conn: &mut AnyConnection, m: &PreparedRewrite) {
        let r=sqlx::query("UPDATE documents SET content_path=$1,updated_at=$2,last_updated_by=$3 WHERE id=$4 AND space_id=$5 AND content_path=$6 AND updated_at=$7 AND published_at=$8 AND archived_at IS NULL AND deleted_at IS NULL").bind(m.path()).bind(m.revision()).bind(&m.actor_id).bind(&m.file.document_id).bind(&m.file.space_id).bind(&m.source_path).bind(&m.source_revision).bind(&m.published_stamp).execute(conn).await.unwrap();
        assert_eq!(r.rows_affected(), 1);
    }
    // Real application producer + record paths, not caller-authored custody.
    async fn restoration_fixture(commit: bool) -> (AppState,std::path::PathBuf,PreparedRewrite) {
        let (s,a,r,i)=state().await;
        let mut tx=begin(&s).await;
        let m=prepare_in(&r,&mut tx,&a,i,b"CLASSIFIED REWRITE").await.unwrap();
        if commit {
            transition(&mut tx,&m).await;
            record_in(&r,&mut tx,&a,&m).await.unwrap();
            tx.commit().await.unwrap();
        } else {tx.rollback().await.unwrap();}
        (s,r,m)
    }
    #[tokio::test]
    async fn restoration_real_prepared_rollback_has_no_commit_authority() {
        let (s,r,m)=restoration_fixture(false).await;
        let mut c=s.db.acquire().await.unwrap();
        let before=std::fs::read(r.join(m.path())).unwrap();
        let inspected=inspect_restoration(&r,&mut c,m.op_id()).await.unwrap();
        assert_eq!(inspected.class,RestorationOperationClass::PreparedUncommitted);
        assert_eq!(inspected.manifest,m);
        assert!(inspected.raw_receipt.is_none());
        assert!(inspected.install.installed_present);
        assert!(inspected.install.proof.is_some());
        assert_eq!(std::fs::read(r.join(m.path())).unwrap(),before);
        let n:i64=sqlx::query_scalar("SELECT count(*) FROM canonical_revision_receipts").fetch_one(&mut *c).await.unwrap();
        assert_eq!(n,0);std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn restoration_real_commit_and_superseded_deleted_history_are_read_only() {
        let (s,r,m)=restoration_fixture(true).await;
        let original=journal_fs::manifest_bytes(&r,NS,m.op_id()).unwrap();
        let bytes=std::fs::read(r.join(m.path())).unwrap();
        let mut c=s.db.acquire().await.unwrap();
        let current=inspect_restoration(&r,&mut c,m.op_id()).await.unwrap();
        assert_eq!(current.class,RestorationOperationClass::CommittedCurrent);
        assert_eq!(current.raw_manifest,original);
        assert!(current.raw_receipt.is_some());
        assert_eq!(current.install.required_group.len(),3);
        sqlx::query("UPDATE documents SET content_path='spaces/a/docs/later.md',updated_at='later' WHERE id='target-a'").execute(&mut *c).await.unwrap();
        let historical=inspect_restoration(&r,&mut c,m.op_id()).await.unwrap();
        assert_eq!(historical.class,RestorationOperationClass::CommittedHistorical);
        sqlx::query("DELETE FROM documents WHERE id='target-a'").execute(&mut *c).await.unwrap();
        let deleted=inspect_restoration(&r,&mut c,m.op_id()).await.unwrap();
        assert_eq!(deleted.class,RestorationOperationClass::CommittedHistorical);
        assert_eq!(deleted.raw_manifest,original);
        assert_eq!(std::fs::read(r.join(m.path())).unwrap(),bytes);
        assert!(!r.join("spaces/a/docs/later.md").exists());
        std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn restoration_missing_historical_install_is_refused_never_recreated() {
        let (s,r,m)=restoration_fixture(true).await;
        let mut c=s.db.acquire().await.unwrap();
        sqlx::query("DELETE FROM documents WHERE id='target-a'").execute(&mut *c).await.unwrap();
        std::fs::remove_file(r.join(m.path())).unwrap();
        assert!(inspect_restoration(&r,&mut c,m.op_id()).await.is_err());
        assert!(!r.join(m.path()).exists());
        std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn restoration_receipt_columns_and_full_envelopes_must_match() {
        let (s,r,m)=restoration_fixture(true).await;
        let mut c=s.db.acquire().await.unwrap();
        sqlx::query("UPDATE canonical_revision_receipts SET payload_fingerprint='wrong'").execute(&mut *c).await.unwrap();
        assert!(inspect_restoration(&r,&mut c,m.op_id()).await.is_err());
        sqlx::query("UPDATE canonical_revision_receipts SET payload_fingerprint=$1").bind(&m.incoming.payload_fingerprint).execute(&mut *c).await.unwrap();
        let raw=serde_json::to_string(&m).unwrap();
        let mut unknown=serde_json::to_value(&m).unwrap();unknown["eligible"]=serde_json::json!(true);
        sqlx::query("UPDATE canonical_revision_receipts SET manifest_json=$1").bind(unknown.to_string()).execute(&mut *c).await.unwrap();
        assert!(inspect_restoration(&r,&mut c,m.op_id()).await.is_err());
        sqlx::query("UPDATE canonical_revision_receipts SET manifest_json=$1").bind(&raw).execute(&mut *c).await.unwrap();
        let path=r.join(NS).join(m.op_id()).join("manifest.json");
        std::fs::write(&path,unknown.to_string()).unwrap();
        assert!(inspect_restoration(&r,&mut c,m.op_id()).await.is_err());
        let duplicate=format!("{{\"op_id\":\"{}\",{}",m.op_id(),&raw[1..]);
        std::fs::write(&path,duplicate).unwrap();
        assert!(inspect_restoration(&r,&mut c,m.op_id()).await.is_err());
        std::fs::write(&path,&raw).unwrap();
        assert!(inspect_restoration(&r,&mut c,m.op_id()).await.is_ok());
        std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn restoration_source_bound_real_replay_preserves_optional_certificates() {
        let (s,a,r,mut i)=state().await;
        let mut tx=begin(&s).await;
        let source_bytes=std::fs::read(r.join(&i.source_path)).unwrap();
        i.source_sha256=Some(digest(&source_bytes));
        let row=crate::spaces::published_source::selected(&mut tx,&i.space_id,&i.document_id).await.unwrap();
        let original=crate::spaces::published_source::digest(&row,&source_bytes).unwrap();
        let m=prepare_source_in(&r,&mut tx,&a,i,b"SOURCE BOUND REPLAY",Some(&original)).await.unwrap();
        let certificate=m.published_source_evidence().unwrap();
        transition(&mut tx,&m).await;record_in(&r,&mut tx,&a,&m).await.unwrap();tx.commit().await.unwrap();
        let inspected=inspect_restoration(&r,&mut s.db.acquire().await.unwrap(),m.op_id()).await.unwrap();
        assert_eq!(inspected.class,RestorationOperationClass::CommittedCurrent);
        assert_eq!(inspected.manifest.published_source_evidence().unwrap(),certificate);
        assert_eq!(inspected.raw_manifest,journal_fs::manifest_bytes(&r,NS,m.op_id()).unwrap());
        std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn restoration_asset_dependency_predicate_checks_full_original_identity() {
        // Real canonical AssetReferenceRewrite producer. Asset receipt here is
        // predicate input only; the sync fixture independently covers atomic commit.
        let (s,a,r,mut i)=state().await;
        let mut tx=begin(&s).await;
        i.source_sha256=Some(digest(&std::fs::read(r.join(&i.source_path)).unwrap()));
        i.reason=RewriteReason::AssetReferenceRewrite;
        i.incoming.entity_type="asset".into();i.incoming.entity_id="asset-one".into();i.incoming.action="create".into();
        let m=prepare_in(&r,&mut tx,&a,i,b"ACTUAL ASSET REFERENCE BODY").await.unwrap();
        transition(&mut tx,&m).await;record_in(&r,&mut tx,&a,&m).await.unwrap();tx.commit().await.unwrap();
        let inspected=inspect_restoration(&r,&mut s.db.acquire().await.unwrap(),m.op_id()).await.unwrap();
        let receipt:crate::sync::AssetReplayReceipt=serde_json::from_value(serde_json::json!({
            "incoming":m.incoming,"actor_id":m.actor_id,"space_id":m.file.space_id,"document_id":m.file.document_id,
            "storage_path":"spaces/a/attachments/target-a/blob.png","filename":"blob.png","original_name":"original.png",
            "mime_type":"image/png","created_at":"2026-10-09T00:00:00Z","byte_length":4,"binary_sha256":digest(b"BLOB"),
            "source_kind":"published","source_id":m.file.document_id,"source_revision":m.file.revision,"body_sha256":m.file.sha256
        })).unwrap();
        assert!(inspected.matches_asset_receipt(&receipt));
        let mut prepared=inspected.clone();prepared.class=RestorationOperationClass::PreparedUncommitted;
        assert!(!prepared.matches_asset_receipt(&receipt));
        for field in ["provider","operation_id","entity_type","entity_id","action","payload_fingerprint"] {
            let mut bad=serde_json::to_value(&receipt).unwrap();bad["incoming"][field]=serde_json::json!("different");
            assert!(!inspected.matches_asset_receipt(&serde_json::from_value(bad).unwrap()),"{field}");
        }
        for field in ["actor_id","space_id","document_id","source_kind","source_id","source_revision","body_sha256"] {
            let mut bad=serde_json::to_value(&receipt).unwrap();bad[field]=serde_json::json!("different");
            assert!(!inspected.matches_asset_receipt(&serde_json::from_value(bad).unwrap()),"{field}");
        }
        for field in ["source_id","source_revision","body_sha256"] {
            let mut bad=serde_json::to_value(&receipt).unwrap();bad[field]=serde_json::Value::Null;
            assert!(!inspected.matches_asset_receipt(&serde_json::from_value(bad).unwrap()),"{field}");
        }
        assert!(inspected.manifest.published_source_evidence().is_none());
        std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn restoration_missing_proof_is_not_repaired() {
        let (s,r,m)=restoration_fixture(true).await;
        let path=r.join(NS).join(m.op_id()).join("install-proof.json");
        std::fs::remove_file(&path).unwrap();
        assert!(inspect_restoration(&r,&mut s.db.acquire().await.unwrap(),m.op_id()).await.is_err());
        assert!(!path.exists());std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn old_manifest_absent_result_bytes_and_historical_ack_unchanged() {
        let (s,a,root,intent)=state().await;let incoming=intent.incoming.clone();let mut tx=begin(&s).await;
        let m=prepare_in(&root,&mut tx,&a,intent,b"OLD RECEIPT EXACT").await.unwrap();let original=journal_fs::manifest_bytes(&root,NS,m.op_id()).unwrap();
        let decoded:PreparedRewrite=serde_json::from_slice(&original).unwrap();assert!(decoded.published_source_evidence().is_none());assert_eq!(serde_json::to_vec(&decoded).unwrap(),original);
        assert!(!serde_json::from_slice::<serde_json::Value>(&original).unwrap().as_object().unwrap().contains_key("result_source_digest"));
        transition(&mut tx,&m).await;record_in(&root,&mut tx,&a,&m).await.unwrap();tx.commit().await.unwrap();
        sqlx::query("DELETE FROM documents WHERE id='target-a'").execute(&s.db).await.unwrap();
        assert_eq!(committed_incoming(&mut s.db.acquire().await.unwrap(),&a,&incoming).await.unwrap(),Some(m.clone()));recover_in(&root,&s.db).await.unwrap();
        assert_eq!(journal_fs::manifest_bytes(&root,NS,m.op_id()).unwrap(),original);std::fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn exact_bytes_stamp_no_draft_and_historical_unknown_ack() {
        let (s, a, r, i) = state().await;
        let old = fs::read(r.join(&i.source_path)).unwrap();
        let incoming = i.incoming.clone();
        let mut tx = begin(&s).await;
        let m = prepare_in(&r, &mut tx, &a, i, b"---\ntitle: Exact\n---\nNEW")
            .await
            .unwrap();
        assert_ne!(m.op_id(), incoming.operation_id);
        assert!(!serde_json::to_value(&m)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("draft_id"));
        assert_eq!(fs::read(r.join(&m.source_path)).unwrap(), old);
        transition(&mut tx, &m).await;
        record_in(&r, &mut tx, &a, &m).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            fs::read(r.join(m.path())).unwrap(),
            b"---\ntitle: Exact\n---\nNEW"
        );
        assert_eq!(
            confirm_committed(&s.db, &m).await.unwrap(),
            CommitEvidence::Committed
        );
        let stamp: String =
            sqlx::query_scalar("SELECT published_at FROM documents WHERE id='target-a'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(stamp, "published");
        sqlx::query("DELETE FROM documents WHERE id='target-a'")
            .execute(&s.db)
            .await
            .unwrap();
        let mut c = s.db.acquire().await.unwrap();
        assert_eq!(
            committed_incoming(&mut c, &a, &incoming).await.unwrap(),
            Some(m.clone())
        );
        drop(c);
        recover_in(&r, &s.db).await.unwrap();
        assert_eq!(fs::read(r.join(&m.source_path)).unwrap(), old);
        fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn rollback_and_receipt_uncertainty_do_not_delete_or_repoint() {
        let (s, a, r, i) = state().await;
        let mut tx = begin(&s).await;
        let m = prepare_in(&r, &mut tx, &a, i, b"new").await.unwrap();
        transition(&mut tx, &m).await;
        record_in(&r, &mut tx, &a, &m).await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(
            confirm_committed(&s.db, &m).await.unwrap(),
            CommitEvidence::NotCommitted
        );
        recover_in(&r, &s.db).await.unwrap();
        let p: String =
            sqlx::query_scalar("SELECT content_path FROM documents WHERE id='target-a'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(p, m.source_path);
        assert!(r.join(m.path()).exists());
        s.db.close().await;
        assert!(confirm_committed(&s.db, &m).await.is_err());
        assert!(r.join(m.path()).exists());
        fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn transformation_hash_and_source_identity_fail_before_or_after_install() {
        for after in [false, true] {
            let (s, a, r, mut i) = state().await;
            i.reason = RewriteReason::AssetReferenceRewrite;
            i.incoming.entity_type = "asset".into();
            i.incoming.entity_id = "asset-legacy".into();
            i.incoming.action = "create".into();
            i.source_sha256 = Some(digest(&fs::read(r.join(&i.source_path)).unwrap()));
            let mut tx = begin(&s).await;
            if !after {
                fs::write(r.join(&i.source_path), b"CHANGED").unwrap();
                assert!(prepare_in(&r, &mut tx, &a, i, b"rewrite").await.is_err());
                assert!(!r.join(NS).exists());
            } else {
                let m = prepare_in(&r, &mut tx, &a, i, b"rewrite").await.unwrap();
                fs::write(r.join(&m.source_path), b"CHANGED").unwrap();
                transition(&mut tx, &m).await;
                assert!(record_in(&r, &mut tx, &a, &m).await.is_err());
            }
            tx.rollback().await.unwrap();
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM canonical_revision_receipts")
                .fetch_one(&s.db)
                .await
                .unwrap();
            assert_eq!(count, 0);
            fs::remove_dir_all(r).unwrap();
        }
    }
    #[tokio::test]
    async fn late_current_credentials_lock_stamp_and_receipt_tamper_roll_back() {
        for sql in ["DELETE FROM sessions WHERE id='rewrite-owner'","UPDATE users SET is_active=0 WHERE id='owner-a'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','owner-a')","UPDATE documents SET locked=1,locked_by='editor-a' WHERE id='target-a'","UPDATE documents SET published_at='new' WHERE id='target-a'","UPDATE canonical_revision_receipts SET incoming_id='collision'","UPDATE documents SET archived_at='old' WHERE id='target-a'"] {
 let(s,a,r,i)=state().await;let mut tx=begin(&s).await;let m=prepare_in(&r,&mut tx,&a,i,b"rewrite").await.unwrap();transition(&mut tx,&m).await;
 sqlx::query(&format!("CREATE TRIGGER late_rewrite AFTER INSERT ON canonical_revision_receipts BEGIN {sql}; END")).execute(&mut *tx).await.unwrap();assert!(record_in(&r,&mut tx,&a,&m).await.is_err(),"{sql}");tx.rollback().await.unwrap();
 let count:i64=sqlx::query_scalar("SELECT count(*) FROM canonical_revision_receipts").fetch_one(&s.db).await.unwrap();assert_eq!(count,0);let p:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='target-a'").fetch_one(&s.db).await.unwrap();assert_eq!(p,m.source_path);assert!(fs::read_to_string(r.join(p)).unwrap().contains("PUBLIC OLD"));fs::remove_dir_all(r).unwrap();}
    }
    #[tokio::test]
    async fn prepare_denies_invalid_scope_stamp_private_lock_and_oversize_without_stage() {
        for change in [
            "UPDATE documents SET published_at=NULL WHERE id='target-a'",
            "UPDATE documents SET space_id='space-b' WHERE id='target-a'",
            "UPDATE documents SET parent_id='target-b' WHERE id='target-a'",
            "UPDATE documents SET locked=1,locked_by='editor-a' WHERE id='target-a'",
            "DELETE FROM sessions WHERE id='rewrite-owner'",
        ] {
            let (s, a, r, i) = state().await;
            sqlx::query(change).execute(&s.db).await.unwrap();
            let mut tx = begin(&s).await;
            assert!(prepare_in(&r, &mut tx, &a, i, b"NEW").await.is_err());
            assert!(!r.join(NS).exists());
            tx.rollback().await.unwrap();
            fs::remove_dir_all(r).unwrap();
        }
        let (s, a, r, i) = state().await;
        let mut tx = begin(&s).await;
        assert!(prepare_in(&r, &mut tx, &a, i, &vec![0; 6_000_001])
            .await
            .is_err());
        assert!(!r.join(NS).exists());
        tx.rollback().await.unwrap();
        fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn incoming_receipt_collision_and_corruption_never_acknowledge_other_intent() {
        let (s, a, r, i) = state().await;
        let incoming = i.incoming.clone();
        let mut tx = begin(&s).await;
        let m = prepare_in(&r, &mut tx, &a, i, b"NEW").await.unwrap();
        transition(&mut tx, &m).await;
        record_in(&r, &mut tx, &a, &m).await.unwrap();
        tx.commit().await.unwrap();
        let mut c = s.db.acquire().await.unwrap();
        for changed in ["entity_id", "action", "payload"] {
            let mut q = incoming.clone();
            match changed {
                "entity_id" => q.entity_id = "other".into(),
                "action" => q.action = "other".into(),
                _ => q.payload_fingerprint = digest(b"different"),
            };
            assert!(committed_incoming(&mut c, &a, &q).await.is_err());
        }
        drop(c);
        sqlx::query("UPDATE canonical_revision_receipts SET manifest_json='{}'")
            .execute(&s.db)
            .await
            .unwrap();
        assert!(confirm_committed(&s.db, &m).await.is_err());
        recover_in(&r, &s.db).await.unwrap();
        assert!(r.join(m.path()).exists());
        fs::remove_dir_all(r).unwrap();
    }
    async fn disk(root: &Path) -> (AppState, AuthUser, RewriteIntent) {
        let (mut s, a, temporary, i) = state().await;
        fs::remove_dir_all(temporary).unwrap();
        sqlx::any::install_default_drivers();
        s.db = sqlx::any::AnyPoolOptions::new()
            .max_connections(2)
            .after_connect(|c, _| {
                Box::pin(async move {
                    sqlx::query("PRAGMA journal_mode=WAL")
                        .execute(&mut *c)
                        .await?;
                    sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;
                    Ok(())
                })
            })
            .connect(&format!(
                "sqlite://{}?mode=rwc",
                root.join("database.sqlite").display()
            ))
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&s.db).await.unwrap();
        for q in ["INSERT INTO users(id,email,display_name,role) VALUES('owner-a','owner@rewrite.test','Owner','member'),('editor-a','editor@rewrite.test','Editor','member')","INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('rewrite-owner','owner-a','test','2999-01-01T00:00:00Z')","INSERT INTO spaces(id,name,slug,created_by) VALUES('space-a','A','a','owner-a')","INSERT INTO space_members(id,space_id,user_id,role) VALUES('owner','space-a','owner-a','owner')","INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at,published_at) VALUES('target-a','space-a','Old','spaces/a/docs/old.md','owner-a','owner-a','source','published')"] {sqlx::query(q).execute(&s.db).await.unwrap();}
        fs::create_dir_all(root.join("spaces/a/docs")).unwrap();
        fs::write(root.join(&i.source_path), b"PUBLIC OLD").unwrap();
        (s, a, i)
    }
    #[tokio::test]
    async fn disk_wal_reader_old_before_commit_and_new_after_receipt_commit() {
        let r = root();
        let (s, a, i) = disk(&r).await;
        let mut tx = begin(&s).await;
        let m = prepare_in(&r, &mut tx, &a, i, b"PUBLIC NEXT")
            .await
            .unwrap();
        transition(&mut tx, &m).await;
        record_in(&r, &mut tx, &a, &m).await.unwrap();
        let (path, stamp): (String, String) =
            sqlx::query_as("SELECT content_path,published_at FROM documents WHERE id='target-a'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(path, m.source_path);
        assert_eq!(fs::read(r.join(path)).unwrap(), b"PUBLIC OLD");
        assert_eq!(stamp, "published");
        tx.commit().await.unwrap();
        let (path, stamp): (String, String) =
            sqlx::query_as("SELECT content_path,published_at FROM documents WHERE id='target-a'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(path, m.path());
        assert_eq!(fs::read(r.join(path)).unwrap(), b"PUBLIC NEXT");
        assert_eq!(stamp, "published");
        s.db.close().await;
        fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn disk_wal_current_revocation_after_guard_wait_creates_no_install() {
        let r = root();
        let (s, a, i) = disk(&r).await;
        let mut blocker = begin(&s).await;
        sqlx::query("DELETE FROM sessions WHERE id='rewrite-owner'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let copy = s.clone();
        let rr = r.clone();
        let mut pending = tokio::spawn(async move {
            let mut tx = copy.db.begin().await.unwrap();
            crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &["space-a"]).await?;
            sqlx::query("UPDATE documents SET id=id WHERE id='target-a'")
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
            prepare_in(&rr, &mut tx, &a, i, b"NEXT").await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut pending)
                .await
                .is_err()
        );
        blocker.commit().await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(5), pending)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        assert!(!r.join(NS).exists());
        s.db.close().await;
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn process_crash_child() {
        let Ok(path) = std::env::var("STARLANE_REWRITE_CRASH_ROOT") else {
            return;
        };
        let phase = std::env::var("STARLANE_REWRITE_CRASH_PHASE").unwrap();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async move {
                let r = std::path::PathBuf::from(path);
                let (s, a, i) = disk(&r).await;
                let mut tx = begin(&s).await;
                if phase == "anchor-copy" {
                    journal_fs::anchor_fault(true, || std::process::exit(77));
                }
                let m = prepare_in(&r, &mut tx, &a, i, b"NEXT COMMITTED BYTES")
                    .await
                    .unwrap();
                if phase == "install" {
                    std::process::exit(77)
                };
                transition(&mut tx, &m).await;
                if phase == "sql" {
                    std::process::exit(77)
                };
                record_in(&r, &mut tx, &a, &m).await.unwrap();
                if phase == "receipt" {
                    std::process::exit(77)
                };
                tx.commit().await.unwrap();
                std::process::exit(77);
            });
    }
    #[tokio::test]
    async fn actual_process_crash_restart_verifies_only_without_repoint_or_recreation() {
        for phase in ["anchor-copy", "install", "sql", "receipt", "commit"] {
            let r = root();
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "storage::canonical_rewrite::tests::process_crash_child",
                    "--nocapture",
                ])
                .env("STARLANE_REWRITE_CRASH_ROOT", &r)
                .env("STARLANE_REWRITE_CRASH_PHASE", phase)
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(77), "{phase}");
            sqlx::any::install_default_drivers();
            let db = sqlx::any::AnyPoolOptions::new()
                .max_connections(2)
                .connect(&format!("sqlite://{}", r.join("database.sqlite").display()))
                .await
                .unwrap();
            let before: (String, String) = sqlx::query_as(
                "SELECT content_path,published_at FROM documents WHERE id='target-a'",
            )
            .fetch_one(&db)
            .await
            .unwrap();
            assert_eq!(before.1, "published");
            assert_eq!(before.0 == "spaces/a/docs/old.md", phase != "commit");
            recover_in(&r, &db).await.unwrap();
            let after: (String, String) = sqlx::query_as(
                "SELECT content_path,published_at FROM documents WHERE id='target-a'",
            )
            .fetch_one(&db)
            .await
            .unwrap();
            assert_eq!(after, before);
            assert_eq!(
                fs::read(r.join("spaces/a/docs/old.md")).unwrap(),
                b"PUBLIC OLD"
            );
            assert!(r.join(NS).exists());
            if phase == "commit" {
                let raw: String =
                    sqlx::query_scalar("SELECT manifest_json FROM canonical_revision_receipts")
                        .fetch_one(&db)
                        .await
                        .unwrap();
                let m: PreparedRewrite = serde_json::from_str(&raw).unwrap();
                assert_eq!(
                    confirm_committed(&db, &m).await.unwrap(),
                    CommitEvidence::Committed
                );
                sqlx::query("DELETE FROM documents")
                    .execute(&db)
                    .await
                    .unwrap();
                sqlx::query("DELETE FROM space_members")
                    .execute(&db)
                    .await
                    .unwrap();
                sqlx::query("DELETE FROM spaces")
                    .execute(&db)
                    .await
                    .unwrap();
                fs::remove_dir_all(r.join("spaces")).unwrap();
                recover_in(&r, &db).await.unwrap();
                assert!(!r.join("spaces").exists());
            }
            db.close().await;
            fs::remove_dir_all(r).unwrap();
        }
    }

    #[tokio::test]
    async fn user_key_write_only_is_real_current_credential_and_downgrade_rolls_back() {
        let (s, mut a, r, i) = state().await;
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('rewrite-key','owner-a','Test','hash','prefix','[\"write\"]')").execute(&s.db).await.unwrap();
        a.claims.sid = "api-key:rewrite-key".into();
        a.api_key_scopes = Some(vec!["write".into()]);
        let mut tx = begin(&s).await;
        let m = prepare_in(&r, &mut tx, &a, i.clone(), b"NEW")
            .await
            .unwrap();
        transition(&mut tx, &m).await;
        record_in(&r, &mut tx, &a, &m).await.unwrap();
        tx.rollback().await.unwrap();
        let mut tx = begin(&s).await;
        let m = prepare_in(&r, &mut tx, &a, i, b"NEW").await.unwrap();
        transition(&mut tx, &m).await;
        sqlx::query("CREATE TRIGGER key_revoke AFTER INSERT ON canonical_revision_receipts BEGIN UPDATE api_keys SET scopes='[\"read\"]' WHERE id='rewrite-key'; END").execute(&mut *tx).await.unwrap();
        assert!(record_in(&r, &mut tx, &a, &m).await.is_err());
        tx.rollback().await.unwrap();
        let scopes: String =
            sqlx::query_scalar("SELECT scopes FROM api_keys WHERE id='rewrite-key'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(scopes, "[\"write\"]");
        fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn symlink_source_denies_transformation_and_unowned_revision_collision_is_retained() {
        use std::os::unix::fs::symlink;
        let (s, a, r, mut i) = state().await;
        i.reason = RewriteReason::AssetReferenceRewrite;
        i.incoming.entity_type = "asset".into();
        i.incoming.entity_id = "asset".into();
        i.incoming.action = "create".into();
        i.source_sha256 = Some(digest(b"OUTSIDE"));
        let outside = r.join("outside.md");
        fs::write(&outside, b"OUTSIDE").unwrap();
        fs::remove_file(r.join(&i.source_path)).unwrap();
        symlink(&outside, r.join(&i.source_path)).unwrap();
        let mut tx = begin(&s).await;
        assert!(prepare_in(&r, &mut tx, &a, i, b"NEW").await.is_err());
        assert!(!r.join(NS).exists());
        tx.rollback().await.unwrap();
        fs::remove_dir_all(r).unwrap();
        let (s, a, r, i) = state().await;
        let owned = r.clone();
        journal_fs::hook(move || {
            let entry = fs::read_dir(owned.join(NS))
                .unwrap()
                .next()
                .unwrap()
                .unwrap();
            let m: PreparedRewrite =
                serde_json::from_slice(&fs::read(entry.path().join("manifest.json")).unwrap())
                    .unwrap();
            fs::write(owned.join(m.path()), b"UNOWNED LEAF").unwrap();
        });
        let mut tx = begin(&s).await;
        assert!(prepare_in(&r, &mut tx, &a, i, b"NEW").await.is_err());
        tx.rollback().await.unwrap();
        let entry = fs::read_dir(r.join(NS)).unwrap().next().unwrap().unwrap();
        let m: PreparedRewrite =
            serde_json::from_slice(&fs::read(entry.path().join("manifest.json")).unwrap()).unwrap();
        assert_eq!(fs::read(r.join(m.path())).unwrap(), b"UNOWNED LEAF");
        assert!(fs::read_to_string(r.join(m.source_path))
            .unwrap()
            .contains("PUBLIC OLD"));
        fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn record_denies_non_file_envelope_tamper_at_install_boundary_and_rolls_back() {
        for field in ["actor", "source", "incoming", "stamp"] {
            let (state, auth, root, intent) = state().await;
            let mut tx = begin(&state).await;
            let m = prepare_in(&root, &mut tx, &auth, intent, b"NEXT")
                .await
                .unwrap();
            transition(&mut tx, &m).await;
            let path = root.join(NS).join(m.op_id()).join("manifest.json");
            journal_fs::hook(move || {
                let mut raw: serde_json::Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                match field {
                    "actor" => raw["actor_id"] = serde_json::json!("editor-a"),
                    "source" => raw["source_revision"] = serde_json::json!("foreign-source"),
                    "incoming" => raw["incoming"]["provider"] = serde_json::json!("other-provider"),
                    _ => raw["published_stamp"] = serde_json::json!("other-stamp"),
                };
                fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
            });
            let error = record_in(&root, &mut tx, &auth, &m).await.unwrap_err();
            assert!(
                matches!(error,AppError::Conflict(ref e) if e.contains("full manifest changed")),
                "{field}: {error:?}"
            );
            tx.rollback().await.unwrap();
            let (path, stamp): (String, String) = sqlx::query_as(
                "SELECT content_path,published_at FROM documents WHERE id='target-a'",
            )
            .fetch_one(&state.db)
            .await
            .unwrap();
            assert_eq!(path, m.source_path);
            assert_eq!(stamp, "published");
            let receipts: i64 =
                sqlx::query_scalar("SELECT count(*) FROM canonical_revision_receipts")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(receipts, 0);
            assert!(root.join(m.path()).exists());
            assert!(root.join(NS).join(m.op_id()).exists());
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[tokio::test]
    async fn oversized_envelope_rejected_before_stage_and_real_replay_actions_bound_exactly() {
        for action in [
            "update_content",
            "resolve_conflict:merge",
            "resolve_conflict:keep-local",
            "resolve_conflict:keep-remote",
            "resolve_conflict:other",
            "publish",
        ] {
            let (state, auth, root, mut intent) = state().await;
            intent.incoming.action = action.into();
            let mut tx = begin(&state).await;
            let result = prepare_in(&root, &mut tx, &auth, intent, b"NEXT").await;
            if matches!(action, "publish" | "resolve_conflict:other") {
                assert!(result.is_err());
                assert!(!root.join(NS).exists());
            } else {
                let m = result.unwrap();
                assert_eq!(m.incoming.action, action);
                transition(&mut tx, &m).await;
                record_in(&root, &mut tx, &auth, &m).await.unwrap();
            }
            tx.rollback().await.unwrap();
            fs::remove_dir_all(root).unwrap();
        }
        for large in ["provider", "operation"] {
            let (state, auth, root, mut intent) = state().await;
            if large == "provider" {
                intent.incoming.provider = "x".repeat(65_536);
            } else {
                intent.incoming.operation_id = "🦅".repeat(20_000);
            }
            let mut tx = begin(&state).await;
            assert!(
                matches!(prepare_in(&root,&mut tx,&auth,intent,b"NEXT").await,Err(AppError::BadRequest(ref e)) if e.contains("manifest limit"))
            );
            assert!(!root.join(NS).exists());
            tx.rollback().await.unwrap();
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[tokio::test]
    async fn legacy_manifest_single_read_revalidates_requested_identity_and_full_draft() {
        let (state, auth, root, intent) = state().await;
        let mut tx = begin(&state).await;
        let m = prepare_in(&root, &mut tx, &auth, intent, b"NEXT")
            .await
            .unwrap();
        let path = root.join(NS).join(m.op_id()).join("manifest.json");
        let original = serde_json::to_value(&m).unwrap();
        let mut legacy = original;
        legacy["draft_id"] = serde_json::json!("real-legacy-draft");
        legacy["version"] = serde_json::json!(1);
        for field in ["valid", "op", "version", "length", "path"] {
            let mut raw = legacy.clone();
            match field {
                "op" => raw["op_id"] = serde_json::json!(Uuid::new_v4().to_string()),
                "version" => raw["version"] = serde_json::json!(2),
                "length" => raw["length"] = serde_json::json!(6_000_001),
                "path" => raw["path"] = serde_json::json!("../outside.md"),
                _ => {}
            };
            fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
            let result = journal_fs::manifest(&root, NS, m.op_id(), 1);
            if field == "valid" {
                assert_eq!(result.unwrap().draft_id, "real-legacy-draft");
            } else {
                assert!(result.is_err(), "{field}");
            }
        }
        tx.rollback().await.unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn recovery_full_envelope_after_receipt_wait_retains_tamper_unverified() {
        let (state, auth, root, intent) = state().await;
        let mut tx = begin(&state).await;
        let m = prepare_in(&root, &mut tx, &auth, intent, b"NEXT")
            .await
            .unwrap();
        transition(&mut tx, &m).await;
        record_in(&root, &mut tx, &auth, &m).await.unwrap();
        tx.commit().await.unwrap();
        let file = root.join(NS).join(m.op_id()).join("manifest.json");
        RECOVERY_VERIFY_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                let mut raw: serde_json::Value =
                    serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
                raw["incoming"]["provider"] = serde_json::json!("other-provider");
                fs::write(file, serde_json::to_vec(&raw).unwrap()).unwrap();
            }))
        });
        recover_in(&root, &state.db).await.unwrap();
        let verified: Option<String> = sqlx::query_scalar(
            "SELECT verified_at FROM canonical_revision_receipts WHERE op_id=$1",
        )
        .bind(m.op_id())
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert!(verified.is_none());
        let current: String =
            sqlx::query_scalar("SELECT content_path FROM documents WHERE id='target-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(current, m.path());
        assert!(root.join(m.path()).exists());
        assert!(root.join(NS).join(m.op_id()).exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn pure_final_files_check_rejects_full_manifest_source_and_file_drift() {
        for field in ["manifest", "source", "file"] {
            let (state, auth, root, mut intent) = state().await;
            intent.source_sha256 = Some(digest(&fs::read(root.join(&intent.source_path)).unwrap()));
            let mut tx = begin(&state).await;
            let prepared = prepare_in(&root, &mut tx, &auth, intent, b"FINAL EXACT")
                .await
                .unwrap();
            verify_prepared_files_in(&root, &prepared).unwrap();
            match field {
                "source" => fs::write(root.join(&prepared.source_path), b"SOURCE DRIFT").unwrap(),
                "file" => fs::write(root.join(prepared.path()), b"FILE DRIFT").unwrap(),
                _ => {
                    let path = root.join(NS).join(prepared.op_id()).join("manifest.json");
                    let mut altered = prepared.clone();
                    altered.actor_id = "other".into();
                    fs::write(path, serde_json::to_vec(&altered).unwrap()).unwrap();
                }
            }
            assert!(
                verify_prepared_files_in(&root, &prepared).is_err(),
                "{field}"
            );
            tx.rollback().await.unwrap();
            fs::remove_dir_all(root).unwrap();
        }
    }
}

/// Immutable committed source evidence for byte-identical relocation. This
/// grants no operation/page authority and does not replay or install anything.
pub(crate) async fn committed_source_digest(
    conn: &mut AnyConnection,
    space: &str,
    document: &str,
    path: &str,
    revision: &str,
    published: &str,
) -> Result<Option<String>, AppError> {
    let ops:Vec<String>=sqlx::query_scalar("SELECT op_id FROM canonical_revision_receipts WHERE space_id=$1 AND document_id=$2 AND content_path=$3 AND revision=$4").bind(space).bind(document).bind(path).bind(revision).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let mut result = None;
    for op in ops {
        let m = receipt(conn, &op)
            .await?
            .ok_or_else(|| conflict("source receipt disappeared"))?;
        if m.published_stamp.as_str() == published {
            if result.is_some() {
                return Err(conflict("ambiguous committed source"));
            }
            result = Some(m.file.sha256.clone());
        }
    }
    Ok(result)
}
