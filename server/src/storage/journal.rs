//! Create-only file journal. Call stage_document only while holding the caller's
//! ordered space mutation guard. Receipt insertion belongs to that SAME canonical
//! transaction. Never delete a canonical path on rollback or an uncertain commit.
//! Linux directory-FD traversal and hard-link publication deliberately fail closed
//! on unsupported hosts/filesystems. Lifecycle replacements are outside this API.
use crate::auth::routes::AppError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, AnyPool};
use std::path::Path;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StagedDocument {
    pub op_id: String,
    pub space_id: String,
    pub document_id: String,
    pub draft_id: String,
    pub path: String,
    pub revision: String,
    pub sha256: String,
    pub length: i64,
    pub(super) version: u32,
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn error(e: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("Storage journal: {e}"))
}

#[cfg(test)]
thread_local! {static STAGE_CALLBACK: std::cell::RefCell<Option<Box<dyn FnOnce(&StagedDocument)>>> = std::cell::RefCell::new(None);}
#[cfg(test)]
pub fn test_stage_callback(f: impl FnOnce(&StagedDocument) + 'static) {
    STAGE_CALLBACK.with(|h| *h.borrow_mut() = Some(Box::new(f)));
}

/// The stage contains private content: directories 0700 and regular files 0600.
/// Any interrupted partial stage is retained for operator review, never guessed.
pub async fn stage_document(
    conn: &mut AnyConnection,
    space_id: &str,
    document_id: &str,
    draft_id: &str,
    revision: &str,
    slug: &str,
    title: &str,
    frontmatter: &str,
    content: &str,
) -> Result<StagedDocument, AppError> {
    stage_document_in(&super::data_dir(), conn, space_id, document_id, draft_id,
        revision, slug, title, frontmatter, content).await
}
async fn stage_document_in(
    root: &Path, conn: &mut AnyConnection, space_id: &str, document_id: &str,
    draft_id: &str, revision: &str, slug: &str, title: &str,
    frontmatter: &str, content: &str,
) -> Result<StagedDocument, AppError> {
    let bytes = super::compose_document_markdown(frontmatter, content).into_bytes();
    // Bound the WHOLE composed mirror (including portable headers) before any
    // stage write or receipt. Legacy copy sources may exceed creation limits.
    if bytes.len() > 6_000_000 {
        return Err(AppError::BadRequest(
            "Composed document exceeds storage journal limit".into(),
        ));
    }
    let manifest = StagedDocument {
        op_id: Uuid::new_v4().to_string(),
        space_id: space_id.into(),
        document_id: document_id.into(),
        draft_id: draft_id.into(),
        revision: revision.into(),
        path: format!(
            "spaces/{slug}/docs/{}",
            super::document_filename(title, document_id)
        ),
        sha256: digest(&bytes),
        length: bytes.len() as i64,
        version: 1,
    };
    fsops::stage(root, &manifest, &bytes).map_err(error)?;
    #[cfg(test)]
    STAGE_CALLBACK.with(|h| {
        if let Some(f) = h.borrow_mut().take() {
            f(&manifest)
        }
    });
    // Immutable identity/digest fields; only completion state may later change.
    sqlx::query("INSERT INTO storage_journal_receipts(op_id,space_id,document_id,draft_id,content_path,initial_revision,sha256,byte_length,state) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'pending')")
        .bind(&manifest.op_id).bind(space_id).bind(document_id).bind(draft_id).bind(&manifest.path)
        .bind(revision).bind(&manifest.sha256).bind(manifest.length).execute(conn).await.map_err(AppError::internal)?;
    Ok(manifest)
}
#[cfg(all(test, target_os = "linux"))]
pub fn test_stage_verify_callback(f: impl FnOnce() + 'static) {
    super::journal_fs::stage_verify_hook(f);
}
/// Pure final precommit proof for a private, uninstalled v1 stage. No SQL,
/// authentication, grant, publication or recovery state changes.
pub(crate) fn verify_staged_files(manifest: &StagedDocument) -> Result<(), AppError> {
    verify_staged_files_in(&super::data_dir(), manifest)
}
fn verify_staged_files_in(root: &Path, manifest: &StagedDocument) -> Result<(), AppError> {
    if fsops::manifest(root, &manifest.op_id).map_err(error)? != *manifest {
        return Err(error("Stage manifest identity changed"));
    }
    super::journal_fs::verify_stage_file(root, ".storage-journal", &manifest.into())
        .map_err(error)?;
    if fsops::manifest(root, &manifest.op_id).map_err(error)? != *manifest {
        return Err(error("Stage manifest identity changed"));
    }
    Ok(())
}
async fn receipt(conn: &mut AnyConnection, op: &str) -> Result<Option<StagedDocument>, AppError> {
    let row: Option<(String,String,String,String,String,String,i64)> = sqlx::query_as("SELECT space_id,document_id,draft_id,content_path,initial_revision,sha256,byte_length FROM storage_journal_receipts WHERE op_id=$1")
        .bind(op).fetch_optional(conn).await.map_err(AppError::internal)?;
    Ok(row.map(
        |(space_id, document_id, draft_id, path, revision, sha256, length)| StagedDocument {
            op_id: op.into(),
            space_id,
            document_id,
            draft_id,
            path,
            revision,
            sha256,
            length,
            version: 1,
        },
    ))
}
async fn inspect_pending_association(conn: &mut AnyConnection, manifest: &StagedDocument)
    -> Result<(), AppError> {
        let current: Option<(String,String,String,String)> = sqlx::query_as(
            "SELECT d.space_id,d.content_path,d.updated_at,s.slug FROM documents d JOIN spaces s ON s.id=d.space_id WHERE d.id=$1 AND d.space_id=$2 AND d.deleted_at IS NULL AND d.archived_at IS NULL")
            .bind(&manifest.document_id).bind(&manifest.space_id)
            .fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        let draft: Option<String> = sqlx::query_scalar(
            "SELECT id FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3")
            .bind(&manifest.draft_id).bind(&manifest.document_id).bind(&manifest.space_id)
            .fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        if current.as_ref().is_none_or(|(space,path,revision,slug)| {
            space != &manifest.space_id || path != &manifest.path || revision != &manifest.revision
                || slug.is_empty() || slug.contains('/') || slug.contains('\\')
                || !manifest.path.starts_with(&format!("spaces/{slug}/docs/"))
        }) || draft.is_none() {return Err(error("Pending canonical ownership/revision differs"));}
    Ok(())
}
/// Classification is historical SQL evidence and filesystem requirements only.
/// It is never a caller-authored eligibility override or actor authorization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StorageOperationClass {
    PreparedUncommitted,
    CommittedPending,
    CommittedComplete,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StorageInspection {
    pub manifest: StagedDocument,
    pub raw_manifest: Vec<u8>,
    pub class: StorageOperationClass,
    pub install: super::journal_fs::InstallInspection,
}
/// Use only with an authoritative stopped-write snapshot, or a caller-held
/// transaction/guards. No UPDATE, publication, receipt creation or cleanup.
pub(super) async fn inspect_restoration(
    root: &Path, conn: &mut AnyConnection, op: &str,
) -> Result<StorageInspection, AppError> {
    let raw_manifest = super::journal_fs::manifest_bytes(root, ".storage-journal", op)
        .map_err(error)?;
    let manifest = fsops::manifest(root, op).map_err(error)?;
    let decoded: StagedDocument = serde_json::from_slice(&raw_manifest).map_err(error)?;
    let object: serde_json::Value = serde_json::from_slice(&raw_manifest).map_err(error)?;
    if decoded != manifest || object.as_object().is_none_or(|o| o.len() != 9)
        || [&manifest.space_id, &manifest.document_id, &manifest.draft_id, &manifest.revision]
            .iter().any(|s| s.is_empty())
        || manifest.path.split('/').any(|p| p.is_empty() || p == "." || p == ".." || p.contains('\\'))
        || manifest.sha256.len() != 64
        || !manifest.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(error("Invalid full restoration manifest"));
    }
    let stored = receipt(conn, op).await?;
    let state: Option<String> = sqlx::query_scalar(
        "SELECT state FROM storage_journal_receipts WHERE op_id=$1")
        .bind(op).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let class = match (stored.as_ref(), state.as_deref()) {
        (None, None) => StorageOperationClass::PreparedUncommitted,
        (Some(m), Some("pending")) if *m == manifest => StorageOperationClass::CommittedPending,
        (Some(m), Some("complete")) if *m == manifest => StorageOperationClass::CommittedComplete,
        _ => return Err(error("Restoration receipt identity/state mismatch")),
    };
    if class == StorageOperationClass::CommittedPending {
        inspect_pending_association(conn, &manifest).await?;
    }
    let install = super::journal_fs::inspect_install(root, ".storage-journal", &(&manifest).into(),
        class == StorageOperationClass::CommittedPending).map_err(error)?;
    // Last authoritative SQL observations, then exact immutable envelope reread.
    if receipt(conn, op).await? != stored
        || sqlx::query_scalar::<_,String>("SELECT state FROM storage_journal_receipts WHERE op_id=$1")
            .bind(op).fetch_optional(&mut *conn).await.map_err(AppError::internal)? != state
        || super::journal_fs::manifest_bytes(root, ".storage-journal", op).map_err(error)? != raw_manifest {
        return Err(error("Restoration evidence changed during classification"));
    }
    if class == StorageOperationClass::CommittedPending {
        inspect_pending_association(conn, &manifest).await?;
    }
    if super::journal_fs::inspect_install(root, ".storage-journal", &(&manifest).into(),
        class == StorageOperationClass::CommittedPending).map_err(error)? != install
        || super::journal_fs::manifest_bytes(root, ".storage-journal", op).map_err(error)? != raw_manifest {
        return Err(error("Restoration files changed after SQL observations"));
    }
    Ok(StorageInspection {manifest, raw_manifest, class, install})
}
async fn publish_on_connection(
    root: &Path,
    conn: &mut AnyConnection,
    manifest: &StagedDocument,
) -> Result<(), AppError> {
    if receipt(conn, &manifest.op_id).await?.as_ref() != Some(manifest) {
        return Err(AppError::Conflict(
            "Storage receipt identity mismatch; needs review".into(),
        ));
    }
    let locked = sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2")
        .bind(&manifest.document_id)
        .bind(&manifest.space_id)
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if locked.rows_affected() != 1 {
        return Err(AppError::Conflict(
            "Storage canonical page unavailable; needs review".into(),
        ));
    }
    let current: Option<(String,String,String)> = sqlx::query_as("SELECT space_id,content_path,updated_at FROM documents WHERE id=$1 AND deleted_at IS NULL AND archived_at IS NULL")
        .bind(&manifest.document_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let draft: Option<String> = sqlx::query_scalar(
        "SELECT id FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3",
    )
    .bind(&manifest.draft_id)
    .bind(&manifest.document_id)
    .bind(&manifest.space_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(AppError::internal)?;
    if current
        != Some((
            manifest.space_id.clone(),
            manifest.path.clone(),
            manifest.revision.clone(),
        ))
        || draft.is_none()
    {
        return Err(AppError::Conflict(
            "Storage canonical ownership/revision changed; needs review".into(),
        ));
    }
    fsops::publish(root, manifest)
        .map_err(|e| AppError::Conflict(format!("Storage needs review: {e}")))?;
    sqlx::query(
        "UPDATE storage_journal_receipts SET state='complete',completed_at=$1 WHERE op_id=$2",
    )
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(&manifest.op_id)
    .execute(conn)
    .await
    .map_err(AppError::internal)?;
    Ok(())
}
/// Returns false for a committed page whose publication still needs recovery.
/// Never turn known canonical success into a rollback-shaped HTTP error.
pub async fn publish_committed(db: &AnyPool, manifest: &StagedDocument) -> bool {
    match recover_one(&super::data_dir(), db, manifest, false).await {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(op_id=%manifest.op_id,error=?e,"Committed page storage pending; retained stage");
            false
        }
    }
}
/// A fresh ordered guard after an errored commit avoids reusing an uncertain
/// transaction/connection. Exact immutable receipt identity proves historical
/// canonical success even if later legitimate mutations changed the page.
pub async fn confirm_commit(db: &AnyPool, manifest: &StagedDocument) -> Result<bool, AppError> {
    let mut tx = db.begin().await.map_err(AppError::internal)?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&manifest.space_id])
        .await?;
    let matches = receipt(&mut tx, &manifest.op_id).await?.as_ref() == Some(manifest);
    tx.commit().await.map_err(AppError::internal)?;
    Ok(matches)
}
async fn recover_one(
    root: &Path,
    db: &AnyPool,
    manifest: &StagedDocument,
    allow_rollback: bool,
) -> Result<(), AppError> {
    let mut tx = db.begin().await.map_err(AppError::internal)?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&manifest.space_id])
        .await?;
    let row = sqlx::query("UPDATE storage_journal_receipts SET state=state WHERE op_id=$1")
        .bind(&manifest.op_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if row.rows_affected() == 0 {
        if !allow_rollback {
            return Err(error("Committed receipt unavailable; retain stage"));
        }
        // The guard waited for any adopted creator: absent receipt now proves its
        // canonical transaction did not commit. Retain this private stage.
        // A mutable stage leaf can be exchanged between validation and unlink.
        // First slice retains verified rollback stages rather than deleting any
        // potentially replaced inode. They contain no canonical published file.
        tracing::warn!(op_id=%manifest.op_id,"Rollback confirmed; private stage retained for review");
        tx.commit().await.map_err(AppError::internal)?;
        return Ok(());
    }
    let state: String =
        sqlx::query_scalar("SELECT state FROM storage_journal_receipts WHERE op_id=$1")
            .bind(&manifest.op_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    // Exact historical identity is required even when current body replay is skipped.
    if receipt(&mut tx, &manifest.op_id).await?.as_ref() != Some(manifest) {
        return Err(AppError::Conflict(
            "Storage receipt identity mismatch; needs review".into(),
        ));
    }
    if state == "complete" {
        tx.commit().await.map_err(AppError::internal)?;
        return Ok(());
    }
    publish_on_connection(root, &mut tx, manifest).await?;
    tx.commit().await.map_err(AppError::internal)?;
    // Keep the installed inode anchor until a later slice defines safe completed
    // stage compaction. Pending retries retain their ownership proof; complete receipts skip replay.
    Ok(())
}
/// Must run after migrations and before legacy filename migration/search/listeners.
/// Invalid/partial stages remain private and are logged for review. DB failures
/// propagate: never infer rollback when the authoritative backend is unavailable.
pub async fn recover(db: &AnyPool) -> Result<(), AppError> {
    recover_in(&super::data_dir(), db).await
}
async fn recover_in(root: &Path, db: &AnyPool) -> Result<(), AppError> {
    let pending: Vec<String> =
        sqlx::query_scalar("SELECT op_id FROM storage_journal_receipts WHERE state='pending'")
            .fetch_all(db)
            .await
            .map_err(AppError::internal)?;
    let stages = fsops::inventory(root, !pending.is_empty()).map_err(error)?;
    for op in &pending {
        if !stages.contains(op) {
            tracing::warn!(op_id=%op,"Committed storage receipt has no stage; needs review");
        }
    }
    for op in stages {
        let state: Option<String> =
            sqlx::query_scalar("SELECT state FROM storage_journal_receipts WHERE op_id=$1")
                .bind(&op)
                .fetch_optional(db)
                .await
                .map_err(AppError::internal)?;
        if state.as_deref() == Some("complete") {
            continue;
        }
        match fsops::manifest(root, &op) {
            Ok(manifest) => {
                if let Err(e) = recover_one(root, db, &manifest, true).await {
                    if matches!(e, AppError::Internal(_)) {
                        return Err(e);
                    }
                    tracing::warn!(op_id=%op,error=?e,"Storage stage retained; needs review");
                }
            }
            Err(e) => {
                tracing::warn!(op_id=%op,%e,"Partial/corrupt storage stage retained; needs review")
            }
        }
    }
    Ok(())
}

// Pure readonly evidence check, also exercised on Linux without pretending to
// run a macOS/Windows server. Empty installations retain startup compatibility.
#[cfg(any(not(target_os = "linux"), test))]
fn unsupported_startup_inventory(root: &Path, pending: bool) -> std::io::Result<Vec<String>> {
    match std::fs::symlink_metadata(root.join(".storage-journal")) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && !pending => Ok(vec![]),
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Existing storage journal evidence requires Linux recovery backend",
        )),
    }
}

mod fsops {
    use super::*;
    const NS: &str = ".storage-journal";
    pub fn stage(path: &Path, m: &StagedDocument, bytes: &[u8]) -> std::io::Result<()> {
        super::super::journal_fs::stage(
            path,
            NS,
            m,
            &serde_json::to_vec(m).map_err(std::io::Error::other)?,
            bytes,
        )
    }
    pub fn manifest(path: &Path, op: &str) -> std::io::Result<StagedDocument> {
        super::super::journal_fs::manifest(path, NS, op, 1)
    }
    pub fn publish(path: &Path, m: &StagedDocument) -> std::io::Result<()> {
        // Preserve create-only v1 parsing even though the shared I/O backend
        // also handles independently namespaced publication v2 manifests.
        if manifest(path, &m.op_id)? != *m {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Manifest mismatch",
            ));
        }
        super::super::journal_fs::publish(path, NS, m)
    }
    pub fn inventory(path: &Path, pending: bool) -> std::io::Result<Vec<String>> {
        super::super::journal_fs::inventory(path, NS, pending)
    }
    #[cfg(all(test, target_os = "linux"))]
    pub(super) use super::super::journal_fs::{anchor_fault, final_hook, hook};
}

#[cfg(all(test, target_os = "linux"))]
mod version_boundary_tests {
    use super::*;
    #[test]
    fn create_namespace_rejects_publication_version_before_install() {
        let root = std::env::temp_dir().join(format!("create-version-{}", Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let m = StagedDocument {
            op_id: Uuid::new_v4().to_string(),
            space_id: "space".into(),
            document_id: "page".into(),
            draft_id: "draft".into(),
            path: "spaces/space/docs/page.md".into(),
            revision: "r".into(),
            sha256: digest(b"body"),
            length: 4,
            version: 2,
        };
        fsops::stage(&root, &m, b"body").unwrap();
        assert!(fsops::manifest(&root, &m.op_id).is_err());
        assert!(fsops::publish(&root, &m).is_err());
        assert!(!root.join(&m.path).exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use sqlx::any::AnyPoolOptions;
    use std::{os::unix::fs::symlink, process::Command};
    fn root() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("starlane-journal-{}", Uuid::new_v4()));
        std::fs::create_dir(&p).unwrap();
        p
    }
    fn intent() -> StagedDocument {
        StagedDocument {
            op_id: Uuid::new_v4().to_string(),
            space_id: "s".into(),
            document_id: "d".into(),
            draft_id: "draft".into(),
            path: "spaces/s/docs/doc.md".into(),
            revision: "initial".into(),
            sha256: digest(b"Private body"),
            length: 12,
            version: 1,
        }
    }
    async fn db(url: &str) -> AnyPool {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(if url == "sqlite::memory:" { 1 } else { 2 })
            .connect(url)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        db
    }
    async fn seed(db: &AnyPool) {
        sqlx::query("INSERT INTO users(id,email,display_name) VALUES('u','u@test','User')")
            .execute(db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO spaces(id,name,slug,created_by) VALUES('s','Space','s','u')")
            .execute(db)
            .await
            .unwrap();
    }
    async fn committed(conn: &mut AnyConnection, m: &StagedDocument) {
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at) VALUES('d','s','Doc',$1,'u','u','initial')").bind(&m.path).execute(&mut *conn).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('draft','d','s','u','Doc','Private body','initial')").execute(&mut *conn).await.unwrap();
        sqlx::query("INSERT INTO storage_journal_receipts(op_id,space_id,document_id,draft_id,content_path,initial_revision,sha256,byte_length,state) VALUES($1,'s','d','draft',$2,'initial',$3,$4,'pending')").bind(&m.op_id).bind(&m.path).bind(&m.sha256).bind(m.length).execute(conn).await.unwrap();
    }
    // These fixtures run the actual stage producer and commit/rollback SQL,
    // rather than supplying a caller-authored eligible binding.
    async fn restoration_fixture(commit: bool) -> (std::path::PathBuf, AnyPool, StagedDocument) {
        let r = root(); let pool = db("sqlite::memory:").await; seed(&pool).await;
        let mut tx = pool.begin().await.unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at) VALUES('d','s','Doc','temporary','u','u','initial')").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('draft','d','s','u','Doc','Private body','initial')").execute(&mut *tx).await.unwrap();
        let m = stage_document_in(&r,&mut tx,"s","d","draft","initial","s","Doc","","Private body").await.unwrap();
        sqlx::query("UPDATE documents SET content_path=$1 WHERE id='d'").bind(&m.path).execute(&mut *tx).await.unwrap();
        if commit {tx.commit().await.unwrap();} else {tx.rollback().await.unwrap();}
        (r,pool,m)
    }
    #[tokio::test]
    async fn restoration_classifies_actual_rolled_back_preparation_without_writes() {
        let (r,pool,m) = restoration_fixture(false).await;
        let mut c = pool.acquire().await.unwrap();
        let inspected = inspect_restoration(&r,&mut c,&m.op_id).await.unwrap();
        assert_eq!(inspected.class,StorageOperationClass::PreparedUncommitted);
        assert_eq!(inspected.manifest,m);
        assert!(inspected.install.proof.is_none());
        assert!(!r.join(&m.path).exists());
        let n: i64=sqlx::query_scalar("SELECT count(*) FROM storage_journal_receipts").fetch_one(&mut *c).await.unwrap();
        assert_eq!(n,0); std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn restoration_classifies_real_pending_before_and_after_install() {
        let (r,pool,m)=restoration_fixture(true).await;
        let mut c=pool.acquire().await.unwrap();
        let before=inspect_restoration(&r,&mut c,&m.op_id).await.unwrap();
        assert_eq!(before.class,StorageOperationClass::CommittedPending);
        assert!(before.install.proof.is_none());
        fsops::publish(&r,&m).unwrap();
        let after=inspect_restoration(&r,&mut c,&m.op_id).await.unwrap();
        assert!(after.install.installed_present);
        let proof=after.install.proof.as_ref().unwrap();
        assert_eq!(after.install.required_group.len(),3);
        for path in &after.install.paths {
            if after.install.required_group.contains(&path.relative) {assert_eq!(path.observed,proof.original);}
        }
        let state:String=sqlx::query_scalar("SELECT state FROM storage_journal_receipts").fetch_one(&mut *c).await.unwrap();
        assert_eq!(state,"pending");std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn restoration_complete_superseded_and_deleted_never_require_old_canonical() {
        let (r,pool,m)=restoration_fixture(true).await;
        recover_one(&r,&pool,&m,false).await.unwrap();
        std::fs::remove_file(r.join(&m.path)).unwrap();
        let mut c=pool.acquire().await.unwrap();
        sqlx::query("UPDATE documents SET content_path='spaces/s/docs/later.md',updated_at='later',deleted_at='later' WHERE id='d'").execute(&mut *c).await.unwrap();
        let inspected=inspect_restoration(&r,&mut c,&m.op_id).await.unwrap();
        assert_eq!(inspected.class,StorageOperationClass::CommittedComplete);
        assert!(!inspected.install.installed_present);
        assert!(!inspected.install.required_group.contains(&m.path));
        assert!(!r.join(&m.path).exists());std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn restoration_rejects_mismatched_receipt_manifest_and_pending_association() {
        let (r,pool,m)=restoration_fixture(true).await;
        let mut c=pool.acquire().await.unwrap();
        sqlx::query("UPDATE storage_journal_receipts SET draft_id='wrong'").execute(&mut *c).await.unwrap();
        assert!(inspect_restoration(&r,&mut c,&m.op_id).await.is_err());
        sqlx::query("UPDATE storage_journal_receipts SET draft_id='draft'").execute(&mut *c).await.unwrap();
        sqlx::query("UPDATE documents SET updated_at='later'").execute(&mut *c).await.unwrap();
        assert!(inspect_restoration(&r,&mut c,&m.op_id).await.is_err());
        sqlx::query("UPDATE documents SET updated_at='initial'").execute(&mut *c).await.unwrap();
        let path=r.join(".storage-journal").join(&m.op_id).join("manifest.json");
        std::fs::write(&path,b"{broken").unwrap();
        assert!(inspect_restoration(&r,&mut c,&m.op_id).await.is_err());
        std::fs::write(&path,serde_json::to_vec(&m).unwrap()).unwrap();
        assert!(inspect_restoration(&r,&mut c,&m.op_id).await.is_ok());
        std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn restoration_rejects_extra_and_duplicate_manifest_fields() {
        let (r,pool,m)=restoration_fixture(true).await;
        let mut c=pool.acquire().await.unwrap();
        let path=r.join(".storage-journal").join(&m.op_id).join("manifest.json");
        let mut extra=serde_json::to_value(&m).unwrap();
        extra["eligible"]=serde_json::json!(true);
        std::fs::write(&path,serde_json::to_vec(&extra).unwrap()).unwrap();
        assert!(inspect_restoration(&r,&mut c,&m.op_id).await.is_err());
        let raw=serde_json::to_string(&m).unwrap();
        let duplicate=format!("{{\"version\":1,{}",&raw[1..]);
        std::fs::write(&path,duplicate).unwrap();
        assert!(inspect_restoration(&r,&mut c,&m.op_id).await.is_err());
        std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn restoration_rejects_unowned_anchor_and_malformed_proof() {
        let (r,pool,m)=restoration_fixture(true).await;
        let mut c=pool.acquire().await.unwrap();
        let dir=r.join(".storage-journal").join(&m.op_id);
        std::fs::write(dir.join("install-anchor"),b"unowned").unwrap();
        assert!(inspect_restoration(&r,&mut c,&m.op_id).await.is_err());
        std::fs::remove_file(dir.join("install-anchor")).unwrap();
        fsops::publish(&r,&m).unwrap();
        let raw=std::fs::read(dir.join("install-proof.json")).unwrap();
        std::fs::write(dir.join("install-proof.json"),b"{}").unwrap();
        assert!(inspect_restoration(&r,&mut c,&m.op_id).await.is_err());
        std::fs::write(dir.join("install-proof.json"),raw).unwrap();
        assert!(inspect_restoration(&r,&mut c,&m.op_id).await.is_ok());
        std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn restoration_requirements_do_not_authorize_identical_replacement_inode() {
        let (r,pool,m)=restoration_fixture(true).await; fsops::publish(&r,&m).unwrap();
        let bytes=std::fs::read(r.join(&m.path)).unwrap();
        std::fs::remove_file(r.join(&m.path)).unwrap();
        std::fs::write(r.join(&m.path),bytes).unwrap();
        let mut c=pool.acquire().await.unwrap();
        let inspection=inspect_restoration(&r,&mut c,&m.op_id).await.unwrap();
        let proof=inspection.install.proof.unwrap();
        let observed=inspection.install.paths.iter().find(|p|p.relative==m.path).unwrap();
        assert_ne!(observed.observed,proof.original);
        // This read-only adapter exposes the requirement, not an owns exemption.
        assert!(inspection.install.required_group.contains(&m.path));
        assert!(fsops::publish(&r,&m).is_err());
        std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn rollback_recovery_and_missing_receipt_never_touch_canonical() {
        let r = root();
        let m = intent();
        let db = db("sqlite::memory:").await;
        seed(&db).await;
        fsops::stage(&r, &m, b"Private body").unwrap();
        {
            let mut tx = db.begin().await.unwrap();
            crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &["s"])
                .await
                .unwrap();
            committed(&mut tx, &m).await;
            tx.rollback().await.unwrap();
        }
        // Known-commit publisher does not accept absent receipt as success.
        assert!(recover_one(&r, &db, &m, false).await.is_err());
        assert!(r.join(".storage-journal").join(&m.op_id).exists());
        recover_in(&r, &db).await.unwrap();
        assert!(!r.join(&m.path).exists());
        assert!(r.join(".storage-journal").join(&m.op_id).exists());
        db.close().await;
        std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn committed_receipt_publishes_and_repeated_recovery_matches_anchor() {
        let r = root();
        let m = intent();
        let db = db("sqlite::memory:").await;
        seed(&db).await;
        fsops::stage(&r, &m, b"Private body").unwrap();
        let mut tx = db.begin().await.unwrap();
        committed(&mut tx, &m).await;
        tx.commit().await.unwrap();
        recover_in(&r, &db).await.unwrap();
        recover_in(&r, &db).await.unwrap();
        assert_eq!(std::fs::read(r.join(&m.path)).unwrap(), b"Private body");
        let state: String = sqlx::query_scalar("SELECT state FROM storage_journal_receipts")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(state, "complete");
        db.close().await;
        std::fs::remove_dir_all(r).unwrap();
    }
    #[tokio::test]
    async fn collisions_changed_revision_and_payload_retain_without_overwrite() {
        for kind in [
            "same-bytes-unowned",
            "different-bytes",
            "changed-revision",
            "corrupt-payload",
            "pending-canonical-edit",
        ] {
            let r = root();
            let m = intent();
            let db = db("sqlite::memory:").await;
            seed(&db).await;
            fsops::stage(&r, &m, b"Private body").unwrap();
            let mut tx = db.begin().await.unwrap();
            committed(&mut tx, &m).await;
            tx.commit().await.unwrap();
            if kind.contains("bytes") {
                std::fs::create_dir_all(r.join("spaces/s/docs")).unwrap();
                std::fs::write(
                    r.join(&m.path),
                    if kind == "same-bytes-unowned" {
                        b"Private body".as_slice()
                    } else {
                        b"Someone else's data".as_slice()
                    },
                )
                .unwrap();
            } else if kind == "pending-canonical-edit" {
                fsops::publish(&r, &m).unwrap();
                std::fs::write(r.join(&m.path), b"New canonical content").unwrap();
            } else if kind == "changed-revision" {
                sqlx::query("UPDATE documents SET updated_at='newer'")
                    .execute(&db)
                    .await
                    .unwrap();
            } else {
                std::fs::write(
                    r.join(".storage-journal").join(&m.op_id).join("payload"),
                    b"tampered",
                )
                .unwrap();
            }
            let before = std::fs::read(r.join(&m.path)).ok();
            recover_in(&r, &db).await.unwrap();
            assert_eq!(std::fs::read(r.join(&m.path)).ok(), before);
            let state: String = sqlx::query_scalar("SELECT state FROM storage_journal_receipts")
                .fetch_one(&db)
                .await
                .unwrap();
            assert_eq!(state, "pending");
            assert!(r.join(".storage-journal").join(&m.op_id).exists());
            db.close().await;
            std::fs::remove_dir_all(r).unwrap();
        }
    }
    #[test]
    fn symlink_leaf_parent_and_journal_never_follow_or_overwrite() {
        for kind in ["leaf", "parent", "journal"] {
            let r = root();
            let outside = root();
            let m = intent();
            std::fs::write(outside.join("secret"), b"unowned").unwrap();
            if kind == "journal" {
                symlink(&outside, r.join(".storage-journal")).unwrap();
                assert!(fsops::stage(&r, &m, b"Private body").is_err());
            } else {
                fsops::stage(&r, &m, b"Private body").unwrap();
                std::fs::create_dir_all(r.join("spaces/s")).unwrap();
                if kind == "parent" {
                    symlink(&outside, r.join("spaces/s/docs")).unwrap();
                } else {
                    std::fs::create_dir(r.join("spaces/s/docs")).unwrap();
                    symlink(outside.join("secret"), r.join(&m.path)).unwrap();
                }
                assert!(fsops::publish(&r, &m).is_err());
            }
            assert_eq!(std::fs::read(outside.join("secret")).unwrap(), b"unowned");
            std::fs::remove_dir_all(r).unwrap();
            std::fs::remove_dir_all(outside).unwrap();
        }
    }
    #[test]
    fn partial_and_corrupt_manifest_stages_are_retained() {
        let r = root();
        let m = intent();
        fsops::stage(&r, &m, b"Private body").unwrap();
        let dir = r.join(".storage-journal").join(&m.op_id);
        std::fs::write(dir.join("manifest.json"), b"bad").unwrap();
        assert!(fsops::manifest(&r, &m.op_id).is_err());
        assert!(dir.join("payload").exists());
        std::fs::remove_dir_all(r).unwrap();
    }

    #[tokio::test]
    async fn recovery_waits_for_inflight_space_guard_then_rereads_commit_or_rollback() {
        for commit in [true, false] {
            let r = root();
            let url = format!("sqlite://{}?mode=rwc", r.join("db.sqlite").display());
            let db = db(&url).await;
            seed(&db).await;
            let m = intent();
            let mut tx = db.begin().await.unwrap();
            crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &["s"])
                .await
                .unwrap();
            fsops::stage(&r, &m, b"Private body").unwrap();
            committed(&mut tx, &m).await;
            let recovery_db = db.clone();
            let recovery_root = r.clone();
            let worker =
                tokio::spawn(async move { recover_in(&recovery_root, &recovery_db).await });
            tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            assert!(
                !worker.is_finished(),
                "recovery must wait instead of inferring missing receipt"
            );
            if commit {
                tx.commit().await.unwrap();
            } else {
                tx.rollback().await.unwrap();
            }
            tokio::time::timeout(std::time::Duration::from_secs(8), worker)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(r.join(&m.path).exists(), commit);
            assert!(r.join(".storage-journal").join(&m.op_id).exists());
            db.close().await;
            std::fs::remove_dir_all(r).unwrap();
        }
    }

    #[tokio::test]
    async fn completed_receipts_do_not_replay_after_legitimate_edit_rename_or_delete() {
        for change in ["edit", "rename", "delete"] {
            let r = root();
            let m = intent();
            let db = db("sqlite::memory:").await;
            seed(&db).await;
            fsops::stage(&r, &m, b"Private body").unwrap();
            let mut tx = db.begin().await.unwrap();
            committed(&mut tx, &m).await;
            tx.commit().await.unwrap();
            recover_in(&r, &db).await.unwrap();
            match change {
                "edit" => {
                    std::fs::write(r.join(&m.path), b"New content").unwrap();
                    sqlx::query("UPDATE documents SET updated_at='new'")
                        .execute(&db)
                        .await
                        .unwrap();
                }
                "rename" => {
                    std::fs::rename(r.join(&m.path), r.join("spaces/s/docs/renamed.md")).unwrap();
                    sqlx::query("UPDATE documents SET content_path='spaces/s/docs/renamed.md',updated_at='new'").execute(&db).await.unwrap();
                }
                _ => {
                    sqlx::query("DELETE FROM documents")
                        .execute(&db)
                        .await
                        .unwrap();
                    std::fs::remove_file(r.join(&m.path)).unwrap();
                }
            }
            recover_in(&r, &db).await.unwrap();
            assert_eq!(
                std::fs::read(r.join(".storage-journal").join(&m.op_id).join("payload")).unwrap(),
                b"Private body"
            );
            if change == "edit" {
                assert_eq!(std::fs::read(r.join(&m.path)).unwrap(), b"New content");
            } else {
                assert!(!r.join(&m.path).exists());
            }
            db.close().await;
            std::fs::remove_dir_all(r).unwrap();
        }
    }
    #[test]
    fn ancestor_and_leaf_exchange_are_detected_without_overwriting_replacement() {
        for change in ["ancestor", "leaf", "late-ancestor"] {
            let r = root();
            let m = intent();
            fsops::stage(&r, &m, b"Private body").unwrap();
            let replacement = r.clone();
            let destination = m.path.clone();
            if change == "ancestor" {
                fsops::hook(move || {
                    std::fs::rename(
                        replacement.join("spaces/s/docs"),
                        replacement.join("spaces/s/old-docs"),
                    )
                    .unwrap();
                    std::fs::create_dir(replacement.join("spaces/s/docs")).unwrap();
                    std::fs::write(replacement.join(destination), b"Unowned replacement").unwrap();
                });
            } else if change == "late-ancestor" {
                fsops::final_hook(move || {
                    std::fs::rename(
                        replacement.join("spaces/s/docs"),
                        replacement.join("spaces/s/old-docs"),
                    )
                    .unwrap();
                    std::fs::create_dir(replacement.join("spaces/s/docs")).unwrap();
                    std::fs::write(replacement.join(destination), b"Unowned replacement").unwrap();
                });
            } else {
                fsops::final_hook(move || {
                    std::fs::rename(
                        replacement.join(&destination),
                        replacement.join("spaces/s/docs/old.md"),
                    )
                    .unwrap();
                    std::fs::write(replacement.join(destination), b"Private body").unwrap();
                });
            }
            assert!(fsops::publish(&r, &m).is_err());
            assert_eq!(
                std::fs::read(r.join(&m.path)).unwrap(),
                if change.contains("ancestor") {
                    b"Unowned replacement".as_slice()
                } else {
                    b"Private body".as_slice()
                }
            );
            std::fs::remove_dir_all(r).unwrap();
        }
    }
    #[tokio::test]
    async fn lost_ack_receipt_check_distinguishes_commit_rollback_and_unavailable_backend() {
        for outcome in ["committed", "rollback", "unavailable"] {
            let r = root();
            let m = intent();
            let db = db("sqlite::memory:").await;
            seed(&db).await;
            fsops::stage(&r, &m, b"Private body").unwrap();
            let mut tx = db.begin().await.unwrap();
            committed(&mut tx, &m).await;
            if outcome == "committed" {
                tx.commit().await.unwrap();
                assert!(confirm_commit(&db, &m).await.unwrap());
            } else {
                tx.rollback().await.unwrap();
                if outcome == "unavailable" {
                    db.close().await;
                    assert!(confirm_commit(&db, &m).await.is_err());
                } else {
                    assert!(!confirm_commit(&db, &m).await.unwrap());
                }
            }
            assert!(r
                .join(".storage-journal")
                .join(&m.op_id)
                .join("payload")
                .exists());
            db.close().await;
            std::fs::remove_dir_all(r).unwrap();
        }
    }

    #[tokio::test]
    async fn missing_or_corrupt_pending_stage_and_mismatched_receipt_are_retained_nonfatally() {
        for failure in ["missing", "manifest", "receipt"] {
            let r = root();
            let m = intent();
            let db = db("sqlite::memory:").await;
            seed(&db).await;
            if failure != "missing" {
                fsops::stage(&r, &m, b"Private body").unwrap();
            }
            let mut tx = db.begin().await.unwrap();
            committed(&mut tx, &m).await;
            tx.commit().await.unwrap();
            if failure == "manifest" {
                std::fs::write(
                    r.join(".storage-journal")
                        .join(&m.op_id)
                        .join("manifest.json"),
                    b"corrupt",
                )
                .unwrap();
            } else if failure == "receipt" {
                sqlx::query("UPDATE storage_journal_receipts SET sha256='wrong'")
                    .execute(&db)
                    .await
                    .unwrap();
            }
            recover_in(&r, &db).await.unwrap();
            let state: String = sqlx::query_scalar("SELECT state FROM storage_journal_receipts")
                .fetch_one(&db)
                .await
                .unwrap();
            assert_eq!(state, "pending");
            assert!(!r.join(&m.path).exists());
            db.close().await;
            std::fs::remove_dir_all(r).unwrap();
        }
    }

    #[tokio::test]
    async fn oversized_composed_body_or_header_fails_before_stage_and_receipt() {
        for header in [false, true] {
            let pool = db("sqlite::memory:").await;
            seed(&pool).await;
            let mut tx = pool.begin().await.unwrap();
            crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &["s"])
                .await
                .unwrap();
            let large = "x".repeat(6_000_001);
            let slug = format!("journal-oversized-{}", Uuid::new_v4());
            let result = stage_document(
                &mut tx,
                "s",
                "big-d",
                "big-draft",
                "initial",
                &slug,
                "Big",
                if header { &large } else { "" },
                if header { "Small" } else { &large },
            )
            .await;
            assert!(matches!(result, Err(AppError::BadRequest(_))));
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM storage_journal_receipts")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
            assert_eq!(count, 0);
            assert!(!super::super::space_dir(&slug).exists());
            tx.rollback().await.unwrap();
            pool.close().await;
        }
    }

    #[tokio::test]
    async fn completed_receipt_success_still_requires_exact_immutable_identity() {
        let r = root();
        let m = intent();
        let pool = db("sqlite::memory:").await;
        seed(&pool).await;
        fsops::stage(&r, &m, b"Private body").unwrap();
        let mut tx = pool.begin().await.unwrap();
        committed(&mut tx, &m).await;
        tx.commit().await.unwrap();
        recover_one(&r, &pool, &m, false).await.unwrap();
        sqlx::query("UPDATE storage_journal_receipts SET sha256='mismatch'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            recover_one(&r, &pool, &m, false).await,
            Err(AppError::Conflict(_))
        ));
        assert!(!confirm_commit(&pool, &m).await.unwrap());
        pool.close().await;
        std::fs::remove_dir_all(r).unwrap();
    }
    #[test]
    fn unsupported_backend_starts_only_with_absent_stage_and_pending_receipt_evidence() {
        let r = root();
        assert!(unsupported_startup_inventory(&r, false).unwrap().is_empty());
        assert!(unsupported_startup_inventory(&r, true).is_err());
        let outside = root();
        symlink(&outside, r.join(".storage-journal")).unwrap();
        assert!(unsupported_startup_inventory(&r, false).is_err());
        std::fs::remove_file(r.join(".storage-journal")).unwrap();
        std::fs::create_dir(r.join(".storage-journal")).unwrap();
        assert!(unsupported_startup_inventory(&r, false).is_err());
        std::fs::remove_dir_all(r).unwrap();
        std::fs::remove_dir_all(outside).unwrap();
    }
    // Parent spawns this exact test binary, and exit(77) simulates process loss
    // without unwinding a transaction or running any cleanup destructor.
    #[tokio::test]
    async fn journal_fault_child() {
        let Ok(r) = std::env::var("STARLANE_JOURNAL_TEST_ROOT") else {
            return;
        };
        let r = Path::new(&r);
        let phase = std::env::var("STARLANE_JOURNAL_TEST_PHASE").unwrap();
        let db = db(&format!(
            "sqlite://{}?mode=rwc",
            r.join("db.sqlite").display()
        ))
        .await;
        let m = intent();
        fsops::stage(r, &m, b"Private body").unwrap();
        if phase == "stage" {
            std::process::exit(77)
        }
        let mut tx = db.begin().await.unwrap();
        crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &["s"])
            .await
            .unwrap();
        committed(&mut tx, &m).await;
        if phase == "sql-uncommitted" {
            std::process::exit(77)
        }
        tx.commit().await.unwrap();
        if phase == "committed" {
            std::process::exit(77)
        }
        if phase == "anchor-copy" {
            fsops::anchor_fault(true, || std::process::exit(77));
        } else if phase == "anchor-ready" {
            fsops::anchor_fault(false, || std::process::exit(77));
        }
        fsops::publish(r, &m).unwrap();
        if phase == "installed" {
            std::process::exit(77)
        }
        recover_one(r, &db, &m, false).await.unwrap();
        std::process::exit(77)
    }
    #[tokio::test]
    async fn process_restart_recovers_stage_transaction_commit_install_and_completion() {
        for phase in [
            "stage",
            "sql-uncommitted",
            "committed",
            "anchor-copy",
            "anchor-ready",
            "installed",
            "complete",
        ] {
            let r = root();
            let url = format!("sqlite://{}?mode=rwc", r.join("db.sqlite").display());
            let pool = db(&url).await;
            seed(&pool).await;
            pool.close().await;
            let status = Command::new(std::env::current_exe().unwrap())
                .args([
                    "storage::journal::tests::journal_fault_child",
                    "--exact",
                    "--nocapture",
                ])
                .env("STARLANE_JOURNAL_TEST_ROOT", &r)
                .env("STARLANE_JOURNAL_TEST_PHASE", phase)
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(77));
            let pool = db(&url).await;
            recover_in(&r, &pool).await.unwrap();
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM documents")
                .fetch_one(&pool)
                .await
                .unwrap();
            if matches!(phase, "stage" | "sql-uncommitted") {
                assert_eq!(count, 0);
                assert!(!r.join("spaces/s/docs/doc.md").exists());
                assert_eq!(fsops::inventory(&r, false).unwrap().len(), 1);
            } else {
                assert_eq!(count, 1);
                assert_eq!(
                    std::fs::read(r.join("spaces/s/docs/doc.md")).unwrap(),
                    b"Private body"
                );
            }
            pool.close().await;
            std::fs::remove_dir_all(r).unwrap();
        }
    }
}
