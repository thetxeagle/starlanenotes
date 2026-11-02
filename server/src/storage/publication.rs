//! Actor-owned draft publication preparation, not a publication authorization adapter.
//! Body rewrites/sync that retain published_at or have no owned draft are NOT
//! supported. A future typed source intent/expected publication stamp is needed.
//! Callers hold ordered space/page locks and current real actor publication proof
//! before/after canonical SQL. No stage or receipt alone publishes a page.
//! Prepare installs a NEW immutable path; record follows the caller's canonical
//! transition in that same transaction. Old files and all stage evidence remain.
use super::{journal::StagedDocument, journal_fs};
use crate::{
    auth::{middleware::AuthUser, routes::AppError},
    spaces::access,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, AnyPool};
use std::path::Path;
use uuid::Uuid;
const NS: &str = ".publication-journal";
#[cfg(test)]
tokio::task_local! {
    pub(crate) static PUBLICATION_AFTER_STAGE: std::cell::RefCell<Option<Box<dyn FnOnce() + Send>>>;
}

#[derive(Clone, Debug)]
pub struct PublicationIntent {
    pub op_id: String,
    pub space_id: String,
    pub document_id: String,
    pub draft_id: String,
    pub source_path: String,
    pub source_revision: String,
    pub revision: String,
    /// Canonical JSON/payload fingerprint chosen by the concrete writer adapter.
    pub payload_fingerprint: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreparedPublication {
    #[serde(flatten)]
    file: StagedDocument,
    kind: String,
    actor_id: String,
    source_path: String,
    source_revision: String,
    payload_fingerprint: String,
}
impl PreparedPublication {
    pub fn path(&self) -> &str {
        &self.file.path
    }
    pub fn op_id(&self) -> &str {
        &self.file.op_id
    }
    pub fn revision(&self) -> &str {
        &self.file.revision
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitEvidence {
    Committed,
    NotCommitted,
}
fn conflict(message: &str) -> AppError {
    AppError::Conflict(format!("Publication needs review: {message}"))
}
fn io(error: std::io::Error) -> AppError {
    conflict(&error.to_string())
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn validate(m: &PreparedPublication) -> Result<(), AppError> {
    if m.kind != "publication"
        || m.file.version != 2
        || Uuid::parse_str(&m.file.op_id).is_err()
        || m.file.length < 0
        || m.file.length > 6_000_000
        || m.file.sha256.len() != 64
        || !m.file.sha256.bytes().all(|c| c.is_ascii_hexdigit())
        || m.payload_fingerprint.len() != 64
        || !m.payload_fingerprint.bytes().all(|c| c.is_ascii_hexdigit())
        || m.actor_id.is_empty()
        || m.source_revision.is_empty()
        || m.file.revision.is_empty()
    {
        return Err(conflict("invalid immutable intent"));
    }
    let components: Vec<_> = m.file.path.split('/').collect();
    let slug = components.get(1).copied().unwrap_or_default();
    if !is_publication_revision_path(&m.file.path, slug)
        || components.last().copied() != Some(format!("{}.md", m.file.op_id).as_str())
        || !super::is_safe_relative_path(Path::new(&m.source_path))
    {
        return Err(conflict("invalid revision path"));
    }
    if m.file.path == m.source_path {
        return Err(conflict("revision would overwrite published source"));
    }
    Ok(())
}
/// Exact reserved immutable revision layout, with safe actual space component
/// and UUID leaf. Malformed/traversing paths are not recognized as this format.
pub(super) fn is_publication_revision_path(path: &str, slug: &str) -> bool {
    if !super::is_safe_relative_path(Path::new(path))
        || slug.is_empty()
        || slug.contains('/')
        || slug.contains('\\')
    {
        return false;
    }
    let p: Vec<_> = path.split('/').collect();
    p.len() == 5
        && p[0] == "spaces"
        && p[1] == slug
        && p[2] == "docs"
        && p[3] == "revisions"
        && p[4]
            .strip_suffix(".md")
            .is_some_and(|id| Uuid::parse_str(id).is_ok())
}
/// Prepare requires a currently editable, actor-owned exact draft as defense in
/// depth. The eventual writer still owns its explicit publish operation policy.
pub async fn prepare_publication(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    intent: PublicationIntent,
    frontmatter: &str,
    content: &str,
) -> Result<PreparedPublication, AppError> {
    prepare_in(&super::data_dir(), conn, auth, intent, frontmatter, content).await
}
async fn prepare_in(
    root: &Path,
    conn: &mut AnyConnection,
    auth: &AuthUser,
    intent: PublicationIntent,
    frontmatter: &str,
    content: &str,
) -> Result<PreparedPublication, AppError> {
    let bytes = super::compose_document_markdown(frontmatter, content).into_bytes();
    if bytes.len() > 6_000_000 {
        return Err(AppError::BadRequest(
            "Composed publication exceeds storage journal limit".into(),
        ));
    }
    let principal = access::principal_from_auth(auth)?;
    let draft = access::resolve_draft_access(
        conn,
        principal,
        &intent.space_id,
        &intent.document_id,
        &intent.draft_id,
        access::DraftOperation::Edit,
        access::Purpose::Routine,
    )
    .await?;
    if draft.participation != access::DraftParticipation::Owner {
        return Err(AppError::Forbidden(
            "Only the draft owner can prepare publication".into(),
        ));
    }
    let current:Option<(String,String,String)>=sqlx::query_as("SELECT d.content_path,d.updated_at,s.slug FROM documents d JOIN spaces s ON s.id=d.space_id WHERE d.id=$1 AND d.space_id=$2 AND d.archived_at IS NULL AND d.deleted_at IS NULL").bind(&intent.document_id).bind(&intent.space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let Some((path, revision, slug)) = current else {
        return Err(conflict("source unavailable"));
    };
    if !path.starts_with(&format!("spaces/{slug}/docs/")) {
        return Err(conflict("source path not owned by actual space"));
    }
    if path != intent.source_path || revision != intent.source_revision {
        return Err(conflict("source revision changed"));
    }
    let m = PreparedPublication {
        file: StagedDocument {
            op_id: intent.op_id.clone(),
            space_id: intent.space_id,
            document_id: intent.document_id,
            draft_id: intent.draft_id,
            path: format!("spaces/{slug}/docs/revisions/{}.md", intent.op_id),
            revision: intent.revision,
            sha256: digest(&bytes),
            length: bytes.len() as i64,
            version: 2,
        },
        kind: "publication".into(),
        actor_id: auth.claims.sub.clone(),
        source_path: path,
        source_revision: revision,
        payload_fingerprint: intent.payload_fingerprint,
    };
    validate(&m)?;
    // Existing operation evidence is immutable. The writer supplies the exact
    // reviewed source and request fingerprint under its current authority locks.
    // A retry reuses the original revision; it never rewrites stage evidence.
    let operations = journal_fs::inventory(root, NS, false).map_err(io)?;
    if operations.contains(&m.file.op_id) {
        let inspected = inspect_restoration(root, conn, &m.file.op_id).await?;
        if inspected.class != RestorationOperationClass::PreparedUncommitted {
            return Err(conflict("existing operation requires original receipt inspection"));
        }
        let original = inspected.manifest;
        let mut expected = m;
        expected.file.revision = original.file.revision.clone();
        if original != expected {
            return Err(conflict("original prepared request/source/payload differs"));
        }
        // This uses the native no-replace/custody backend. Restored scopes cannot
        // manufacture a missing install proof; fresh_install_allowed fences it.
        install(root, &original)?;
        if journal_fs::manifest_bytes(root, NS, original.op_id()).map_err(io)? != inspected.raw_manifest {
            return Err(conflict("original manifest changed during resume"));
        }
        return Ok(original);
    }
    journal_fs::stage(
        root,
        NS,
        &m.file,
        &serde_json::to_vec(&m).map_err(AppError::internal)?,
        &bytes,
    )
    .map_err(io)?;
    #[cfg(test)]
    if let Ok(Some(hook)) = PUBLICATION_AFTER_STAGE.try_with(|slot| slot.borrow_mut().take()) {
        hook();
    }
    // New immutable path only. There is no canonical DB association until the
    // caller's later SQL commit, so /files cannot expose this revision as an asset.
    install(root, &m)?;
    Ok(m)
}
fn load(root: &Path, op: &str) -> Result<PreparedPublication, AppError> {
    let m: PreparedPublication =
        serde_json::from_slice(&journal_fs::manifest_bytes(root, NS, op).map_err(io)?)
            .map_err(AppError::internal)?;
    validate(&m)?;
    if m.file.op_id != op {
        return Err(conflict("manifest operation differs"));
    }
    Ok(m)
}
fn install(root: &Path, m: &PreparedPublication) -> Result<(), AppError> {
    if load(root, &m.file.op_id)? != *m {
        return Err(conflict("immutable manifest mismatch"));
    }
    journal_fs::publish(root, NS, &m.file).map_err(io)
}
/// Call ONLY after the canonical new path/revision/published transition, inside
/// the same transaction, with final current actor publication authority. Receipt
/// insertion never substitutes for the writer's final operation-specific proof.
pub async fn record_publication(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    m: &PreparedPublication,
) -> Result<(), AppError> {
    record_in(&super::data_dir(), conn, auth, m).await
}
async fn record_in(
    root: &Path,
    conn: &mut AnyConnection,
    auth: &AuthUser,
    m: &PreparedPublication,
) -> Result<(), AppError> {
    validate(m)?;
    if auth.claims.sub != m.actor_id {
        return Err(conflict("actor binding differs"));
    }
    access::resolve_page_access(
        conn,
        access::principal_from_auth(auth)?,
        &m.file.space_id,
        &m.file.document_id,
        access::PageOperation::Edit,
        access::Purpose::Routine,
    )
    .await?;
    let current:Option<(String,String,String,String)>=sqlx::query_as("SELECT content_path,updated_at,last_updated_by,published_at FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL").bind(&m.file.document_id).bind(&m.file.space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if current
        != Some((
            m.file.path.clone(),
            m.file.revision.clone(),
            m.actor_id.clone(),
            m.file.revision.clone(),
        ))
    {
        return Err(conflict("canonical transition does not match"));
    }
    install(root, m)?;
    let inserted=sqlx::query("INSERT INTO publication_receipts(op_id,space_id,document_id,draft_id,actor_id,source_path,source_revision,content_path,revision,sha256,byte_length,payload_fingerprint,manifest_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
        .bind(&m.file.op_id).bind(&m.file.space_id).bind(&m.file.document_id).bind(&m.file.draft_id).bind(&m.actor_id).bind(&m.source_path).bind(&m.source_revision).bind(&m.file.path).bind(&m.file.revision).bind(&m.file.sha256).bind(m.file.length).bind(&m.payload_fingerprint).bind(serde_json::to_string(m).map_err(AppError::internal)?).execute(&mut *conn).await.map_err(AppError::internal)?;
    if inserted.rows_affected() != 1 {
        return Err(conflict("receipt insert did not apply"));
    }
    let final_row:Option<(String,String,String,String)>=sqlx::query_as("SELECT content_path,updated_at,last_updated_by,published_at FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL").bind(&m.file.document_id).bind(&m.file.space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if final_row
        != Some((
            m.file.path.clone(),
            m.file.revision.clone(),
            m.actor_id.clone(),
            m.file.revision.clone(),
        ))
    {
        return Err(conflict(
            "canonical transition changed during receipt write",
        ));
    }
    install(root, m)?;
    if receipt(conn, &m.file.op_id).await?.as_ref() != Some(m) {
        return Err(conflict("receipt changed during write"));
    }
    // Any wait in the canonical/receipt SQL is followed by current real proof.
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
/// Exact SQL receipt equality on the caller's connection. This is historical
/// evidence only; the concrete adapter must still prove its full outcome.
pub(crate) async fn verify_recorded_publication(conn:&mut AnyConnection,m:&PreparedPublication)->Result<(),AppError> {
    if receipt(conn,m.op_id()).await?.as_ref()!=Some(m) {return Err(conflict("receipt changed during publication"));}
    Ok(())
}
/// Synchronous LAST filesystem proof, after all caller SQL/current credential
/// awaits and before commit. No installation, cleanup, or database work here.
pub(crate) fn verify_prepared_publication(m:&PreparedPublication)->Result<(),AppError> {
    let root=super::data_dir();
    if load(&root,m.op_id())?!=*m {return Err(conflict("immutable manifest mismatch"));}
    journal_fs::verify_file(&root,NS,&journal_fs::FileDescriptor::from(&m.file)).map_err(io)?;
    if load(&root,m.op_id())?!=*m {return Err(conflict("immutable manifest changed during proof"));}
    Ok(())
}
/// Read original adapter identity before choosing a compatibility fingerprint.
/// This is evidence inspection only; prepare still verifies current source,
/// native installation and custody before accepting any existing operation.
pub(crate) fn retained_publication(op: &str) -> Result<Option<PreparedPublication>, AppError> {
    let root = super::data_dir();
    if !journal_fs::inventory(&root, NS, false).map_err(io)?.iter().any(|id| id == op) { return Ok(None); }
    let raw = journal_fs::manifest_bytes(&root, NS, op).map_err(io)?;
    let original = inspection_envelope(&raw)?;
    if journal_fs::manifest_bytes(&root, NS, op).map_err(io)? != raw {
        return Err(conflict("Original publication identity changed"));
    }
    Ok(Some(original))
}
/// Snapshot-reader proof retains original rooted stage/anchor custody while
/// allowing the installed canonical leaf to have been retired by later work.
pub(crate) fn verify_snapshot_manifest(m: &PreparedPublication) -> Result<(), AppError> {
    let root = super::data_dir();
    let original = journal_fs::manifest_bytes(&root, NS, m.op_id()).map_err(io)?;
    if inspection_envelope(&original)? != *m { return Err(conflict("Snapshot manifest mismatch")); }
    journal_fs::verify_historical_anchor(&root, NS, &journal_fs::FileDescriptor::from(&m.file)).map_err(io)?;
    if journal_fs::manifest_bytes(&root, NS, m.op_id()).map_err(io)? != original {
        return Err(conflict("Snapshot manifest changed during proof"));
    }
    Ok(())
}
/// Read-only historical classification, not restoration eligibility or access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RestorationOperationClass {
    PreparedUncommitted,
    CommittedCurrent,
    CommittedHistorical,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RestorationInspection {
    pub manifest: PreparedPublication,
    pub raw_manifest: Vec<u8>,
    pub raw_receipt: Option<String>,
    pub class: RestorationOperationClass,
    pub install: journal_fs::InstallInspection,
}
fn inspection_envelope(raw: &[u8]) -> Result<PreparedPublication, AppError> {
    let m: PreparedPublication = serde_json::from_slice(raw).map_err(|_| conflict("corrupt inspection envelope"))?;
    validate(&m)?;
    let value: serde_json::Value = serde_json::from_slice(raw).map_err(|_| conflict("corrupt inspection envelope"))?;
    let object = value.as_object().ok_or_else(|| conflict("invalid inspection envelope"))?;
    const FIELDS: &[&str] = &["op_id", "space_id", "document_id", "draft_id", "path", "revision", "sha256", "length", "version", "kind", "actor_id", "source_path", "source_revision", "payload_fingerprint"];
    if object.keys().any(|k| !FIELDS.contains(&k.as_str())) {
        return Err(conflict("unknown inspection envelope field"));
    }
    Ok(m)
}
async fn inspection_association(conn: &mut AnyConnection, m: &PreparedPublication)
    -> Result<Option<(String,String,String,String)>,AppError> {
    sqlx::query_as("SELECT content_path,updated_at,last_updated_by,published_at FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(&m.file.document_id).bind(&m.file.space_id).fetch_optional(conn).await.map_err(AppError::internal)
}
/// Requires an authoritative stopped-write snapshot or a caller-guarded
/// transaction. Never installs/repoints files, changes SQL or grants authority.
pub(super) async fn inspect_restoration(root: &Path, conn: &mut AnyConnection, op: &str)
    -> Result<RestorationInspection, AppError> {
    let raw_manifest = journal_fs::manifest_bytes(root, NS, op).map_err(io)?;
    let m = inspection_envelope(&raw_manifest)?;
    if m.file.op_id != op || load(root, op)? != m {return Err(conflict("inspection manifest identity differs"));}
    let stored = receipt(conn, op).await?;
    let raw_receipt: Option<String> = sqlx::query_scalar("SELECT manifest_json FROM publication_receipts WHERE op_id=$1")
        .bind(op).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    match (&stored, &raw_receipt) {
        (None,None) => {},
        (Some(stored),Some(raw)) if *stored == m && inspection_envelope(raw.as_bytes())? == m => {},
        _ => return Err(conflict("inspection SQL receipt differs")),
    }
    let association = inspection_association(conn,&m).await?;
    let class = if stored.is_none() {RestorationOperationClass::PreparedUncommitted}
        else if association == Some((m.file.path.clone(),m.file.revision.clone(),m.actor_id.clone(),m.file.revision.clone())) {
            RestorationOperationClass::CommittedCurrent
        } else {RestorationOperationClass::CommittedHistorical};
    // Immutable result paths are required even for superseded/deleted history;
    // an absent current canonical association never authorizes reconstruction.
    let install = journal_fs::inspect_install(root, NS, &journal_fs::FileDescriptor::from(&m.file), true).map_err(io)?;
    if stored.is_some() && (install.proof.is_none() || !install.installed_present
        || !install.paths.iter().any(|p| p.relative == format!("{NS}/{op}/install-anchor"))) {
        return Err(conflict("committed immutable install evidence missing"));
    }
    if receipt(conn,op).await? != stored
        || sqlx::query_scalar::<_,String>("SELECT manifest_json FROM publication_receipts WHERE op_id=$1")
            .bind(op).fetch_optional(&mut *conn).await.map_err(AppError::internal)? != raw_receipt
        || inspection_association(conn,&m).await? != association {
        return Err(conflict("inspection SQL changed"));
    }
    if journal_fs::inspect_install(root, NS, &journal_fs::FileDescriptor::from(&m.file),true).map_err(io)? != install
        || journal_fs::manifest_bytes(root,NS,op).map_err(io)? != raw_manifest {
        return Err(conflict("inspection immutable evidence changed"));
    }
    Ok(RestorationInspection {manifest:m,raw_manifest,raw_receipt,class,install})
}
async fn receipt(
    conn: &mut AnyConnection,
    op: &str,
) -> Result<Option<PreparedPublication>, AppError> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT manifest_json FROM publication_receipts WHERE op_id=$1")
            .bind(op)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let m: PreparedPublication =
        serde_json::from_str(&raw).map_err(|_| conflict("corrupt receipt"))?;
    validate(&m)?;
    let matches:Option<i64>=sqlx::query_scalar("SELECT 1 FROM publication_receipts WHERE op_id=$1 AND space_id=$2 AND document_id=$3 AND draft_id=$4 AND actor_id=$5 AND source_path=$6 AND source_revision=$7 AND content_path=$8 AND revision=$9 AND sha256=$10 AND byte_length=$11 AND payload_fingerprint=$12")
      .bind(&m.file.op_id).bind(&m.file.space_id).bind(&m.file.document_id).bind(&m.file.draft_id).bind(&m.actor_id).bind(&m.source_path).bind(&m.source_revision).bind(&m.file.path).bind(&m.file.revision).bind(&m.file.sha256).bind(m.file.length).bind(&m.payload_fingerprint).fetch_optional(conn).await.map_err(AppError::internal)?;
    if matches.is_none() {
        return Err(conflict("immutable receipt columns differ"));
    }
    Ok(Some(m))
}
/// Historical receipts survive canonical deletion. If no space row remains,
/// there is no live space guard to acquire: lock immutable receipt evidence only.
/// This grants no actor authority and performs no canonical replay/repointing.
async fn lock_historical_evidence(
    conn: &mut AnyConnection,
    m: &PreparedPublication,
) -> Result<(), AppError> {
    match crate::spaces::mutation_guards::lock_spaces_for_mutation(conn, &[&m.file.space_id]).await
    {
        Ok(()) | Err(AppError::NotFound(_)) => {}
        Err(error) => return Err(error),
    }
    sqlx::query("UPDATE publication_receipts SET verified_at=verified_at WHERE op_id=$1")
        .bind(&m.file.op_id)
        .execute(conn)
        .await
        .map_err(AppError::internal)?;
    Ok(())
}
/// Fresh historical proof after an unknown commit. Errors stay indeterminate;
/// no filesystem operation/rollback is performed on this path.
pub async fn confirm_committed(
    db: &AnyPool,
    m: &PreparedPublication,
) -> Result<CommitEvidence, AppError> {
    let mut tx = db.begin().await.map_err(AppError::internal)?;
    lock_historical_evidence(&mut tx, m).await?;
    let stored = receipt(&mut tx, &m.file.op_id).await?;
    let result = match stored {
        None => CommitEvidence::NotCommitted,
        Some(stored) if stored == *m => CommitEvidence::Committed,
        Some(_) => return Err(conflict("receipt identity differs")),
    };
    tx.commit().await.map_err(AppError::internal)?;
    Ok(result)
}
#[cfg(not(target_os="linux"))]
struct PublicationRecoveryChange;
#[cfg(not(target_os="linux"))]
fn recovery_observer_fence(_:Option<&()>)->Result<(),AppError>{Ok(())}
/// Conservative normalization fence. A retained orphan, corrupt manifest or
/// uncertain receipt owns its source path until explicit review resolves it.
/// Read-only; no missing evidence is repaired or interpreted as commit proof.
pub async fn has_unresolved_operations(db: &AnyPool) -> Result<bool, AppError> {
    unresolved_in(&super::data_dir(), db).await
}
async fn unresolved_in(root: &Path, db: &AnyPool) -> Result<bool, AppError> {
    let mut operations = match journal_fs::inventory(root, NS, false) {
        Ok(operations) => operations,
        Err(error) => { tracing::warn!(?error, "Publication inventory uncertain; normalization deferred"); return Ok(true); }
    };
    let receipted: Vec<String> = sqlx::query_scalar("SELECT op_id FROM publication_receipts")
        .fetch_all(db).await.map_err(AppError::internal)?;
    operations.extend(receipted);
    operations.sort();
    operations.dedup();
    for operation in operations {
        let manifest = match load(root, &operation) {
            Ok(manifest) => manifest,
            Err(error) => { tracing::warn!(?error, "Publication manifest uncertain; normalization deferred"); return Ok(true); }
        };
        let mut tx = db.begin().await.map_err(AppError::internal)?;
        lock_historical_evidence(&mut tx, &manifest).await?;
        let inspected = inspect_restoration(root, &mut tx, &operation).await;
        tx.commit().await.map_err(AppError::internal)?;
        match inspected {
            Ok(proof) if proof.class != RestorationOperationClass::PreparedUncommitted => {},
            _ => return Ok(true),
        }
    }
    Ok(false)
}
/// Startup only verifies committed immutable installs; never repoints or replays
/// a superseded page. Missing/corrupt/orphan evidence remains for operator review.
pub async fn recover(db: &AnyPool) -> Result<(), AppError> {
    recover_in(&super::data_dir(), db).await
}
async fn recover_in(root:&Path,db:&AnyPool)->Result<(),AppError> {recover_in_observed(root,db,None).await.map(|_|())}
#[cfg(target_os="linux")]
include!("publication_verification_recovery.rs");
async fn recover_in_observed(root: &Path, db: &AnyPool,
 #[cfg(target_os="linux")] observer:Option<&crate::storage::restoration::linux::OriginalTargetRecoverySelection<'_>>,
 #[cfg(not(target_os="linux"))] observer:Option<&()>,
) -> Result<Vec<PublicationRecoveryChange>, AppError> {
    let mut changes=Vec::new();
    recovery_observer_fence(observer)?;
    let pending: Vec<String> =
        sqlx::query_scalar("SELECT op_id FROM publication_receipts WHERE verified_at IS NULL")
            .fetch_all(db)
            .await
            .map_err(AppError::internal)?;
    recovery_observer_fence(observer)?;
    let stages = journal_fs::inventory(root, NS, !pending.is_empty()).map_err(io)?;
    #[cfg(target_os="linux")] if observer.is_some()&&(pending.len()>256||stages.len()>256){return Err(conflict("verification recovery count exceeds256"));}
    for op in &pending {
        if !stages.contains(op) {
            #[cfg(target_os="linux")] if observer.is_some(){return Err(conflict("verification receipt stage missing"));}
            tracing::warn!(op_id=%op,"Publication receipt stage missing; needs review");
        }
    }
    for op in stages {
        recovery_observer_fence(observer)?;
        let m = match load(root, &op) {
            Ok(m) => m,
            Err(e) => {
                #[cfg(target_os="linux")] if observer.is_some(){return Err(e);}
                tracing::warn!(op_id=%op,error=?e,"Publication stage retained; needs review");
                continue;
            }
        };
        let mut tx = db.begin().await.map_err(AppError::internal)?;
        recovery_observer_fence(observer)?;
        lock_historical_evidence(&mut tx, &m).await?;
        recovery_observer_fence(observer)?;
        let stored = match receipt(&mut tx, &op).await {
            Ok(stored) => stored,
            Err(AppError::Conflict(message)) => {
                #[cfg(target_os="linux")] if observer.is_some(){return Err(conflict(&message));}
                tracing::warn!(op_id=%op,%message,"Publication receipt retained; needs review");
                tx.commit().await.map_err(AppError::internal)?;
                continue;
            }
            Err(error) => return Err(error),
        };
        recovery_observer_fence(observer)?;
        if stored.as_ref() != Some(&m) {
            #[cfg(target_os="linux")] if observer.is_some(){return Err(conflict("verification orphan or mismatched receipt"));}
            tracing::warn!(op_id=%op,"Publication orphan or mismatched receipt retained; needs review");
            tx.commit().await.map_err(AppError::internal)?;
            continue;
        }
        // Even superseded history is verified only at its OWN immutable path;
        // no current canonical state is read, restored, or overwritten.
        match journal_fs::verify_install(root, NS, &m.file).map_err(io) {
            Ok(()) => {
                #[cfg(target_os="linux")]
                let before=if let Some(selection)=observer {Some(recovery_change_prepare(selection,&mut tx,&m).await?)}else{None};
                let verified=chrono::Utc::now().to_rfc3339();
                sqlx::query("UPDATE publication_receipts SET verified_at=$1 WHERE op_id=$2")
                    .bind(&verified)
                    .bind(&op)
                    .execute(&mut *tx)
                    .await
                    .map_err(AppError::internal)?;
                recovery_observer_fence(observer)?;
                #[cfg(target_os="linux")] if let Some(before)=before {let change=recovery_change_finish(observer.expect("private observed owner"),&mut tx,before,verified).await?;changes.push(change);}
            }
            Err(e) => {
                #[cfg(target_os="linux")] if observer.is_some(){return Err(e);}
                tracing::warn!(op_id=%op,error=?e,"Publication install retained; needs review")
            }
        }
        tx.commit().await.map_err(AppError::internal)?;
        recovery_observer_fence(observer)?;
    }
    Ok(changes)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use crate::AppState;
    use std::fs;
    fn root() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("publication-{}", Uuid::new_v4()));
        fs::create_dir(&p).unwrap();
        p
    }
    async fn state() -> (AppState, AuthUser, std::path::PathBuf, PublicationIntent) {
        let state = crate::spaces::properties::tests::property_test_state().await;
        let mut auth = crate::spaces::documents::tests::test_auth("owner-a");
        auth.claims.sid = "publication-owner".into();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('publication-owner','owner-a','hash','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('publish-draft','target-a','space-a','owner-a','New','PRIVATE NEXT','base')").execute(&state.db).await.unwrap();
        let r = root();
        let old = "spaces/a/docs/old.md";
        fs::create_dir_all(r.join("spaces/a/docs")).unwrap();
        fs::write(r.join(old), "PUBLIC OLD").unwrap();
        sqlx::query("UPDATE spaces SET slug='a' WHERE id='space-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET content_path=$1,updated_at='old',published_at='old' WHERE id='target-a'").bind(old).execute(&state.db).await.unwrap();
        let i = PublicationIntent {
            op_id: Uuid::new_v4().to_string(),
            space_id: "space-a".into(),
            document_id: "target-a".into(),
            draft_id: "publish-draft".into(),
            source_path: old.into(),
            source_revision: "old".into(),
            revision: "new".into(),
            payload_fingerprint: digest(b"canonical publish payload"),
        };
        (state, auth, r, i)
    }
    async fn begin(state: &AppState) -> sqlx::Transaction<'_, sqlx::Any> {
        let mut tx = state.db.begin().await.unwrap();
        crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &["space-a"])
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET id=id WHERE id='target-a' AND space_id='space-a'")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx
    }
    async fn transition(conn: &mut AnyConnection, m: &PreparedPublication) {
        sqlx::query("UPDATE documents SET content_path=$1,updated_at=$2,published_at=$2,last_updated_by=$3,title='New' WHERE id=$4 AND space_id=$5").bind(m.path()).bind(m.revision()).bind(&m.actor_id).bind(&m.file.document_id).bind(&m.file.space_id).execute(conn).await.unwrap();
    }
    // Real application producer + record paths, not caller-authored custody.
    async fn restoration_fixture(commit: bool) -> (AppState,std::path::PathBuf,PreparedPublication) {
        let (s,a,r,i)=state().await;
        let mut tx=begin(&s).await;
        let m=prepare_in(&r,&mut tx,&a,i,"","CLASSIFIED PUBLICATION").await.unwrap();
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
        let n:i64=sqlx::query_scalar("SELECT count(*) FROM publication_receipts").fetch_one(&mut *c).await.unwrap();
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
        sqlx::query("UPDATE publication_receipts SET payload_fingerprint='wrong'").execute(&mut *c).await.unwrap();
        assert!(inspect_restoration(&r,&mut c,m.op_id()).await.is_err());
        sqlx::query("UPDATE publication_receipts SET payload_fingerprint=$1").bind(&m.payload_fingerprint).execute(&mut *c).await.unwrap();
        let raw=serde_json::to_string(&m).unwrap();
        let mut unknown=serde_json::to_value(&m).unwrap();unknown["eligible"]=serde_json::json!(true);
        sqlx::query("UPDATE publication_receipts SET manifest_json=$1").bind(unknown.to_string()).execute(&mut *c).await.unwrap();
        assert!(inspect_restoration(&r,&mut c,m.op_id()).await.is_err());
        sqlx::query("UPDATE publication_receipts SET manifest_json=$1").bind(&raw).execute(&mut *c).await.unwrap();
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
    async fn restoration_missing_proof_is_not_repaired() {
        let (s,r,m)=restoration_fixture(true).await;
        let path=r.join(NS).join(m.op_id()).join("install-proof.json");
        std::fs::remove_file(&path).unwrap();
        assert!(inspect_restoration(&r,&mut s.db.acquire().await.unwrap(),m.op_id()).await.is_err());
        assert!(!path.exists());std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn immutable_prepare_read_old_until_commit_and_receipt_requires_exact_transition() {
        let (s, a, r, i) = state().await;
        let mut tx = begin(&s).await;
        let m = prepare_in(&r, &mut tx, &a, i, "", "PRIVATE NEXT")
            .await
            .unwrap();
        assert_eq!(
            fs::read_to_string(r.join(&m.source_path)).unwrap(),
            "PUBLIC OLD"
        );
        assert_eq!(
            fs::read_to_string(r.join(m.path())).unwrap(),
            "PRIVATE NEXT"
        );
        let path: String =
            sqlx::query_scalar("SELECT content_path FROM documents WHERE id='target-a'")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(path, m.source_path);
        assert!(record_in(&r, &mut tx, &a, &m).await.is_err());
        transition(&mut tx, &m).await;
        record_in(&r, &mut tx, &a, &m).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            confirm_committed(&s.db, &m).await.unwrap(),
            CommitEvidence::Committed
        );
        recover_in(&r, &s.db).await.unwrap();
        assert_eq!(
            fs::read_to_string(r.join(&m.source_path)).unwrap(),
            "PUBLIC OLD"
        );
        fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn rollback_and_unknown_evidence_retain_private_install_without_receipt_or_canonical_change(
    ) {
        let (s, a, r, i) = state().await;
        let mut tx = begin(&s).await;
        let m = prepare_in(&r, &mut tx, &a, i, "", "PRIVATE NEXT")
            .await
            .unwrap();
        transition(&mut tx, &m).await;
        record_in(&r, &mut tx, &a, &m).await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(
            confirm_committed(&s.db, &m).await.unwrap(),
            CommitEvidence::NotCommitted
        );
        recover_in(&r, &s.db).await.unwrap();
        let path: String =
            sqlx::query_scalar("SELECT content_path FROM documents WHERE id='target-a'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(path, m.source_path);
        assert!(r.join(m.path()).exists());
        assert!(r.join(NS).join(m.op_id()).exists());
        s.db.close().await;
        assert!(confirm_committed(&s.db, &m).await.is_err());
        assert_eq!(
            fs::read_to_string(r.join(&m.source_path)).unwrap(),
            "PUBLIC OLD"
        );
        fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn superseded_receipt_is_history_only_and_corrupt_binding_is_not_success() {
        let (s, a, r, i) = state().await;
        let mut tx = begin(&s).await;
        let m = prepare_in(&r, &mut tx, &a, i, "", "PRIVATE NEXT")
            .await
            .unwrap();
        transition(&mut tx, &m).await;
        record_in(&r, &mut tx, &a, &m).await.unwrap();
        tx.commit().await.unwrap();
        sqlx::query("UPDATE documents SET content_path='later-revision',updated_at='later',published_at='later',title='Later' WHERE id='target-a'").execute(&s.db).await.unwrap();
        recover_in(&r, &s.db).await.unwrap();
        let path: String =
            sqlx::query_scalar("SELECT content_path FROM documents WHERE id='target-a'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(path, "later-revision");
        assert_eq!(
            confirm_committed(&s.db, &m).await.unwrap(),
            CommitEvidence::Committed
        );
        sqlx::query("UPDATE publication_receipts SET actor_id='another' WHERE op_id=$1")
            .bind(m.op_id())
            .execute(&s.db)
            .await
            .unwrap();
        assert!(confirm_committed(&s.db, &m).await.is_err());
        recover_in(&r, &s.db).await.unwrap();
        fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn publication_namespace_collision_symlink_checksum_and_size_are_fail_closed() {
        for case in ["collision", "symlink", "checksum", "oversize"] {
            let (s, a, r, i) = state().await;
            let mut tx = begin(&s).await;
            if case == "oversize" {
                assert!(prepare_in(&r, &mut tx, &a, i, &"h".repeat(6_000_001), "x")
                    .await
                    .is_err());
                assert!(!r.join(NS).exists());
            } else if case == "symlink" {
                std::os::unix::fs::symlink("spaces", r.join(NS)).unwrap();
                assert!(prepare_in(&r, &mut tx, &a, i, "", "NEXT").await.is_err());
            } else if case == "collision" {
                let dest = r.join(format!("spaces/a/docs/revisions/{}.md", i.op_id));
                fs::create_dir_all(dest.parent().unwrap()).unwrap();
                fs::write(&dest, "FOREIGN").unwrap();
                assert!(prepare_in(&r, &mut tx, &a, i, "", "NEXT").await.is_err());
                assert_eq!(fs::read_to_string(dest).unwrap(), "FOREIGN");
            } else {
                let m = prepare_in(&r, &mut tx, &a, i, "", "NEXT").await.unwrap();
                fs::write(r.join(NS).join(m.op_id()).join("payload"), "CORRUPT").unwrap();
                assert!(install(&r, &m).is_err());
            }
            tx.rollback().await.unwrap();
            assert_eq!(
                fs::read_to_string(r.join("spaces/a/docs/old.md")).unwrap(),
                "PUBLIC OLD"
            );
            fs::remove_dir_all(r).unwrap();
        }
    }
    #[tokio::test]
    async fn publication_current_actor_exact_draft_and_late_receipt_failure_rollback() {
        let (s, a, r, i) = state().await;
        let mut tx = begin(&s).await;
        let mut bad = i.clone();
        bad.draft_id = "foreign".into();
        assert!(prepare_in(&r, &mut tx, &a, bad, "", "NEXT").await.is_err());
        assert!(!r.join(NS).exists());
        let m = prepare_in(&r, &mut tx, &a, i, "", "NEXT").await.unwrap();
        transition(&mut tx, &m).await;
        sqlx::query("CREATE TRIGGER publication_revoke AFTER INSERT ON publication_receipts BEGIN DELETE FROM sessions WHERE id='publication-owner'; END").execute(&mut *tx).await.unwrap();
        assert!(record_in(&r, &mut tx, &a, &m).await.is_err());
        tx.rollback().await.unwrap();
        let path: String =
            sqlx::query_scalar("SELECT content_path FROM documents WHERE id='target-a'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(path, m.source_path);
        assert_eq!(
            confirm_committed(&s.db, &m).await.unwrap(),
            CommitEvidence::NotCommitted
        );
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn unsupported_empty_evidence_rule_is_namespace_specific() {
        let r = root();
        assert!(journal_fs::unsupported_inventory(&r, NS, false)
            .unwrap()
            .is_empty());
        assert!(journal_fs::unsupported_inventory(&r, NS, true).is_err());
        fs::create_dir(r.join(NS)).unwrap();
        assert!(journal_fs::unsupported_inventory(&r, NS, false).is_err());
        fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn immutable_revision_is_excluded_from_legacy_rename_and_unassociated_http_files() {
        use tower::ServiceExt;
        let (s, a, r, i) = state().await;
        let mut tx = begin(&s).await;
        let m = prepare_in(&r, &mut tx, &a, i, "", "PRIVATE NEXT")
            .await
            .unwrap();
        transition(&mut tx, &m).await;
        record_in(&r, &mut tx, &a, &m).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            super::super::migrate_document_filenames_in(&r, &s.db)
                .await
                .unwrap(),
            0
        );
        assert!(r.join(m.path()).exists());
        assert_eq!(
            fs::read_to_string(r.join(&m.source_path)).unwrap(),
            "PUBLIC OLD"
        );
        // The HTTP server reads its real DATA_DIR, not this test's journal root.
        // Prove present unassociated bytes are denied, rather than missing-file404.
        let http_path = super::super::data_dir().join(m.path());
        fs::create_dir_all(http_path.parent().unwrap()).unwrap();
        let mut http_file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&http_path)
            .unwrap();
        std::io::Write::write_all(&mut http_file, b"PRIVATE NEXT").unwrap();
        drop(http_file);
        assert_eq!(fs::read_to_string(&http_path).unwrap(), "PRIVATE NEXT");
        let token = crate::auth::jwt::create_token_pair_for_session(
            "owner-a",
            "owner@example.test",
            "owner",
            &s.jwt_secret,
            Some("publication-owner"),
            30,
        )
        .unwrap()
        .access_token;
        let response = crate::spaces::attachments::router()
            .with_state(s.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/files/{}", m.path()))
                    .header("Authorization", format!("Bearer {token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
        fs::remove_file(http_path).unwrap();
        fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn publication_child_crash_phase() {
        let Ok(r) = std::env::var("PUBLICATION_CHILD_ROOT") else {
            return;
        };
        let r = Path::new(&r);
        let m: PreparedPublication =
            serde_json::from_str(&std::env::var("PUBLICATION_CHILD_MANIFEST").unwrap()).unwrap();
        let phase = std::env::var("PUBLICATION_CHILD_PHASE").unwrap();
        journal_fs::stage(
            r,
            NS,
            &m.file,
            &serde_json::to_vec(&m).unwrap(),
            b"PRIVATE NEXT",
        )
        .unwrap();
        if phase == "stage" {
            std::process::exit(77);
        }
        if phase == "partial-anchor" {
            journal_fs::anchor_fault(true, || std::process::exit(77));
        }
        if phase == "anchor-proof" {
            journal_fs::anchor_fault(false, || std::process::exit(77));
        }
        if phase == "installed" {
            journal_fs::final_hook(|| std::process::exit(77));
        }
        install(r, &m).unwrap();
        std::process::exit(77);
    }
    #[tokio::test]
    async fn actual_child_exit_retains_all_precommit_evidence_and_old_body_then_recovers_receipt() {
        for phase in [
            "stage",
            "partial-anchor",
            "anchor-proof",
            "installed",
            "before-sql",
        ] {
            let (s, a, r, i) = state().await;
            let m = PreparedPublication {
                file: StagedDocument {
                    op_id: i.op_id,
                    space_id: i.space_id,
                    document_id: i.document_id,
                    draft_id: i.draft_id,
                    path: String::new(),
                    revision: i.revision,
                    sha256: digest(b"PRIVATE NEXT"),
                    length: 12,
                    version: 2,
                },
                kind: "publication".into(),
                actor_id: a.claims.sub.clone(),
                source_path: i.source_path,
                source_revision: i.source_revision,
                payload_fingerprint: i.payload_fingerprint,
            };
            let mut m = m;
            m.file.path = format!("spaces/a/docs/revisions/{}.md", m.op_id());
            let exit = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "storage::publication::tests::publication_child_crash_phase",
                    "--nocapture",
                ])
                .env("PUBLICATION_CHILD_ROOT", &r)
                .env(
                    "PUBLICATION_CHILD_MANIFEST",
                    serde_json::to_string(&m).unwrap(),
                )
                .env("PUBLICATION_CHILD_PHASE", phase)
                .status()
                .unwrap();
            assert_eq!(exit.code(), Some(77));
            assert_eq!(
                fs::read_to_string(r.join(&m.source_path)).unwrap(),
                "PUBLIC OLD"
            );
            assert_eq!(
                confirm_committed(&s.db, &m).await.unwrap(),
                CommitEvidence::NotCommitted
            );
            recover_in(&r, &s.db).await.unwrap();
            assert!(r.join(NS).join(m.op_id()).exists());
            // A real committed transition (simulating lost acknowledgment) supplies receipt
            // proof; restart resumes no-replace install from the preserved immutable stage.
            install(&r, &m).unwrap();
            let mut tx = begin(&s).await;
            transition(&mut tx, &m).await;
            record_in(&r, &mut tx, &a, &m).await.unwrap();
            tx.commit().await.unwrap();
            recover_in(&r, &s.db).await.unwrap();
            assert_eq!(
                confirm_committed(&s.db, &m).await.unwrap(),
                CommitEvidence::Committed
            );
            assert_eq!(
                fs::read_to_string(r.join(m.path())).unwrap(),
                "PRIVATE NEXT"
            );
            assert_eq!(
                fs::read_to_string(r.join(&m.source_path)).unwrap(),
                "PUBLIC OLD"
            );
            fs::remove_dir_all(r).unwrap();
        }
    }
    #[test]
    fn reserved_revision_path_recognizes_only_safe_actual_space_uuid_leaves() {
        let id = Uuid::new_v4();
        let good = format!("spaces/a/docs/revisions/{id}.md");
        assert!(is_publication_revision_path(&good, "a"));
        for bad in [
            format!("spaces/b/docs/revisions/{id}.md"),
            "spaces/a/docs/revisions/../old.md".into(),
            "spaces/a/docs/revisions/not-uuid.md".into(),
            format!("/spaces/a/docs/revisions/{id}.md"),
            format!("spaces/a/docs/revisions/nested/{id}.md"),
        ] {
            assert!(!is_publication_revision_path(&bad, "a"));
        }
    }
    #[tokio::test]
    async fn immutable_receipt_remains_historical_proof_after_real_space_and_page_deletion() {
        let (s, a, r, i) = state().await;
        let mut tx = begin(&s).await;
        let m = prepare_in(&r, &mut tx, &a, i, "", "PRIVATE NEXT")
            .await
            .unwrap();
        transition(&mut tx, &m).await;
        record_in(&r, &mut tx, &a, &m).await.unwrap();
        tx.commit().await.unwrap();
        sqlx::query("DELETE FROM documents WHERE space_id='space-a'")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("DELETE FROM spaces WHERE id='space-a'")
            .execute(&s.db)
            .await
            .unwrap();
        assert_eq!(
            confirm_committed(&s.db, &m).await.unwrap(),
            CommitEvidence::Committed
        );
        recover_in(&r, &s.db).await.unwrap();
        assert_eq!(
            fs::read_to_string(r.join(m.path())).unwrap(),
            "PRIVATE NEXT"
        );
        let missing: Option<String> =
            sqlx::query_scalar("SELECT id FROM spaces WHERE id='space-a'")
                .fetch_optional(&s.db)
                .await
                .unwrap();
        assert!(missing.is_none());
        assert_eq!(
            fs::read_to_string(r.join(&m.source_path)).unwrap(),
            "PUBLIC OLD"
        );
        fs::remove_dir_all(r.join("spaces/a")).unwrap();
        recover_in(&r, &s.db).await.unwrap();
        assert!(!r.join("spaces/a").exists());
        assert_eq!(
            confirm_committed(&s.db, &m).await.unwrap(),
            CommitEvidence::Committed
        );
        fs::remove_dir_all(r).unwrap();
    }

    #[tokio::test]
    async fn separate_wal_reader_sees_old_body_during_new_canonical_sql_then_new_body_after_commit()
    {
        let (mut s, a, r, i) = state().await;
        let dbfile = r.join("readers.sqlite");
        let pool = sqlx::any::AnyPoolOptions::new()
            .max_connections(2)
            .after_connect(|conn, _| {
                Box::pin(async move {
                    sqlx::query("PRAGMA journal_mode=WAL")
                        .execute(&mut *conn)
                        .await?;
                    sqlx::query("PRAGMA busy_timeout=5000")
                        .execute(conn)
                        .await?;
                    Ok(())
                })
            })
            .connect(&format!("sqlite://{}?mode=rwc", dbfile.display()))
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        s.db = pool;
        for sql in ["INSERT INTO users(id,email,display_name,role) VALUES('owner-a','owner@test','Owner','member')","INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('publication-owner','owner-a','h','2999-01-01T00:00:00Z')","INSERT INTO spaces(id,name,slug,created_by) VALUES('space-a','A','a','owner-a')","INSERT INTO space_members(id,space_id,user_id,role) VALUES('owner','space-a','owner-a','owner')","INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at,published_at) VALUES('target-a','space-a','Old','spaces/a/docs/old.md','owner-a','owner-a','old','old')","INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('publish-draft','target-a','space-a','owner-a','New','PRIVATE NEXT','old')"]{sqlx::query(sql).execute(&s.db).await.unwrap();}
        let mut tx = begin(&s).await;
        let m = prepare_in(&r, &mut tx, &a, i, "", "PRIVATE NEXT")
            .await
            .unwrap();
        transition(&mut tx, &m).await;
        let old: String =
            sqlx::query_scalar("SELECT content_path FROM documents WHERE id='target-a'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(old, m.source_path);
        assert_eq!(fs::read_to_string(r.join(&old)).unwrap(), "PUBLIC OLD");
        record_in(&r, &mut tx, &a, &m).await.unwrap();
        tx.commit().await.unwrap();
        let new: String =
            sqlx::query_scalar("SELECT content_path FROM documents WHERE id='target-a'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(new, m.path());
        assert_eq!(fs::read_to_string(r.join(&new)).unwrap(), "PRIVATE NEXT");
        assert_eq!(fs::read_to_string(r.join(&old)).unwrap(), "PUBLIC OLD");
        s.db.close().await;
        fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn original_resume_after_actual_native_process_exit_keeps_original_manifest() {
        for phase in ["stage", "partial-anchor", "anchor-proof", "installed"] {
            let (s,a,r,i)=state().await;
            let m=PreparedPublication {file:StagedDocument {
                op_id:i.op_id.clone(),space_id:i.space_id.clone(),document_id:i.document_id.clone(),draft_id:i.draft_id.clone(),
                path:format!("spaces/a/docs/revisions/{}.md",i.op_id),revision:i.revision.clone(),
                sha256:digest(b"PRIVATE NEXT"),length:12,version:2,
            },kind:"publication".into(),actor_id:a.claims.sub.clone(),source_path:i.source_path.clone(),source_revision:i.source_revision.clone(),payload_fingerprint:i.payload_fingerprint.clone()};
            let exit=std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact","storage::publication::tests::publication_child_crash_phase","--nocapture"])
                .env("PUBLICATION_CHILD_ROOT",&r).env("PUBLICATION_CHILD_MANIFEST",serde_json::to_string(&m).unwrap())
                .env("PUBLICATION_CHILD_PHASE",phase).status().unwrap();
            assert_eq!(exit.code(),Some(77));
            let original=journal_fs::manifest_bytes(&r,NS,m.op_id()).unwrap();
            let mut retry=i;retry.revision="later restart request".into();
            let mut tx=begin(&s).await;
            let resumed=prepare_in(&r,&mut tx,&a,retry,"","PRIVATE NEXT").await.unwrap();
            assert_eq!(resumed,m,"{phase}");
            assert_eq!(journal_fs::manifest_bytes(&r,NS,m.op_id()).unwrap(),original);
            assert_eq!(fs::read_to_string(r.join(&m.source_path)).unwrap(),"PUBLIC OLD");
            transition(&mut tx,&resumed).await;record_in(&r,&mut tx,&a,&resumed).await.unwrap();
            tx.commit().await.unwrap();
            assert_eq!(confirm_committed(&s.db,&m).await.unwrap(),CommitEvidence::Committed);
            fs::remove_dir_all(r).unwrap();
        }
    }
    #[tokio::test]
    async fn original_precommit_resume_reuses_revision_and_raw_evidence() {
        let (s,a,r,i)=state().await;
        let mut tx=begin(&s).await;
        let m=prepare_in(&r,&mut tx,&a,i.clone(),"","PRIVATE NEXT").await.unwrap();
        tx.rollback().await.unwrap();
        let raw=journal_fs::manifest_bytes(&r,NS,m.op_id()).unwrap();
        assert!(unresolved_in(&r,&s.db).await.unwrap());
        let mut retry=i; retry.revision="later retry timestamp".into();
        let mut tx=begin(&s).await;
        let resumed=prepare_in(&r,&mut tx,&a,retry,"","PRIVATE NEXT").await.unwrap();
        assert_eq!(resumed,m);
        assert_eq!(journal_fs::manifest_bytes(&r,NS,m.op_id()).unwrap(),raw);
        transition(&mut tx,&resumed).await;
        record_in(&r,&mut tx,&a,&resumed).await.unwrap();
        tx.commit().await.unwrap();
        assert!(!unresolved_in(&r,&s.db).await.unwrap());
        assert_eq!(fs::read_to_string(r.join(m.source_path)).unwrap(),"PUBLIC OLD");
        fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn original_resume_mismatch_and_corruption_never_rewrite_evidence() {
        for corrupt in [false,true] {
            let (s,a,r,i)=state().await;
            let mut tx=begin(&s).await;
            let m=prepare_in(&r,&mut tx,&a,i.clone(),"","PRIVATE NEXT").await.unwrap();
            tx.rollback().await.unwrap();
            let raw=journal_fs::manifest_bytes(&r,NS,m.op_id()).unwrap();
            let mut retry=i;retry.revision="later".into();
            if corrupt { fs::write(r.join(NS).join(m.op_id()).join("payload"),"CORRUPT").unwrap(); }
            else {retry.payload_fingerprint=digest(b"different request");}
            let mut tx=begin(&s).await;
            assert!(prepare_in(&r,&mut tx,&a,retry,"","PRIVATE NEXT").await.is_err());
            tx.rollback().await.unwrap();
            assert_eq!(journal_fs::manifest_bytes(&r,NS,m.op_id()).unwrap(),raw);
            assert_eq!(fs::read_to_string(r.join(m.source_path)).unwrap(),"PUBLIC OLD");
            assert!(unresolved_in(&r,&s.db).await.unwrap());
            fs::remove_dir_all(r).unwrap();
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn restored_original_without_install_proof_cannot_resume_or_manufacture_proof() {
        use std::os::unix::fs::{MetadataExt,PermissionsExt};
        let (s,a,r,i)=state().await;
        let mut tx=begin(&s).await;
        let m=prepare_in(&r,&mut tx,&a,i.clone(),"","PRIVATE NEXT").await.unwrap();
        tx.rollback().await.unwrap();
        let dir=r.join(NS).join(m.op_id());
        let raw=journal_fs::manifest_bytes(&r,NS,m.op_id()).unwrap();
        // Model a classified restore carrying only the immutable stage. Existing
        // evidence is preserved; the classifier owns an empty physical group.
        fs::remove_file(r.join(m.path())).unwrap();
        for entry in fs::read_dir(&dir).unwrap() {
            let entry=entry.unwrap(); let name=entry.file_name();
            if name.to_string_lossy().starts_with("anchor-") || name=="install-anchor" || name=="install-proof.json" {
                fs::remove_file(entry.path()).unwrap();
            }
        }
        fs::set_permissions(&r,fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(r.join(".starlane-restore-incomplete"),"fixture restore").unwrap();
        let md=fs::metadata(&r).unwrap();
        let ledger=super::super::restoration::Ledger {
            admission:None,completed_absence:None,composite:None,historical_preservation:None,
            version:1,generation_id:Uuid::new_v4().to_string(),backup_id:Uuid::new_v4().to_string(),
            manifest_sha256:digest(b"backup manifest"),database_sha256:digest(b"backup database"),
            root_device:md.dev(),root_inode:md.ino(),scopes:vec![super::super::restoration::Scope {
                namespace:NS.into(),op_id:m.op_id().into(),evidence_sha256:digest(&raw),bindings:vec![],
            }],
        };
        super::super::restoration::prepare_in(&r,&s.db,&ledger).await.unwrap();
        fs::remove_file(r.join(".starlane-restore-incomplete")).unwrap();
        super::super::restoration::activate_in(&r,&s.db).await.unwrap();
        let mut tx=begin(&s).await;
        assert!(prepare_in(&r,&mut tx,&a,i,"","PRIVATE NEXT").await.is_err());
        tx.rollback().await.unwrap();
        assert!(!dir.join("install-proof.json").exists());
        assert!(!r.join(m.path()).exists());
        assert_eq!(journal_fs::manifest_bytes(&r,NS,m.op_id()).unwrap(),raw);
        assert_eq!(fs::read_to_string(r.join(m.source_path)).unwrap(),"PUBLIC OLD");
        fs::remove_dir_all(r).unwrap();
    }

    #[cfg(target_os="linux")]
    mod historical_proof_tests { include!("publication_history_tests.rs"); }

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
    let ops:Vec<String>=sqlx::query_scalar("SELECT op_id FROM publication_receipts WHERE space_id=$1 AND document_id=$2 AND content_path=$3 AND revision=$4").bind(space).bind(document).bind(path).bind(revision).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let mut result = None;
    for op in ops {
        let m = receipt(conn, &op)
            .await?
            .ok_or_else(|| conflict("source receipt disappeared"))?;
        if m.file.revision.as_str() == published {
            if result.is_some() {
                return Err(conflict("ambiguous committed source"));
            }
            result = Some(m.file.sha256.clone());
        }
    }
    Ok(result)
}

#[cfg(target_os="linux")]
include!("publication_pg_verification_recovery.rs");
