//! Real SQLite reservation boundaries: an error never fabricates disposal or rollback.
use super::{reserve, reserve_observed, Admission};
use crate::storage::{attachment_upload::{admission::{Producer, WriterLease}, UploadRequest}, fixture_root::FixtureRoot};
use sqlx::{any::AnyPoolOptions, AnyPool};
use std::{fs, os::unix::{fs::{MetadataExt, OpenOptionsExt}}, path::PathBuf};

const ORIGINAL: &[u8] = b"ORIGINAL";
const RELATIVE: &str = "spaces/custody/attachments/page/legacy.bin";
const MAXIMUM: u64 = 32;

struct Fixture {
    pool: AnyPool,
    root: PathBuf,
    _scope: FixtureRoot,
}
impl Fixture {
    async fn new() -> Self { Self::with_journal("delete").await }
    async fn with_journal(journal: &str) -> Self {
        assert!(matches!(journal, "delete" | "wal"));
        let scope = FixtureRoot::private();
        let root = crate::storage::data_dir();
        sqlx::any::install_default_drivers();
        let pool = AnyPoolOptions::new().max_connections(2)
            .connect(&format!("sqlite://{}?mode=rwc", root.join("fixture.sqlite").display()))
            .await.unwrap();
        let actual: String = sqlx::query_scalar(&format!("PRAGMA journal_mode={journal}"))
            .fetch_one(&pool).await.unwrap();
        assert_eq!(actual, journal);
        crate::db::selected_migrator(crate::db::DatabaseEngine::Sqlite).run(&pool).await.unwrap();
        for sql in [
            "INSERT INTO users(id,email,display_name) VALUES('owner','capacity-custody@example.invalid','Owner')",
            "INSERT INTO spaces(id,name,slug,created_by) VALUES('space','Custody','custody','owner')",
            "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('page','space','Page','spaces/custody/docs/page.md','owner','owner')",
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        // A migration-retained asset has exact accounting metadata, not an invented content digest.
        sqlx::query("INSERT INTO attachment_retained_byte_ledger(asset_id,byte_length,filename,original_name,mime_type,uploader_id,original_created_at,original_storage_path,evidence_kind) VALUES('legacy',8,'legacy.bin','Legacy.bin','application/octet-stream','owner','original',$1,'legacy_upgrade')")
            .bind(RELATIVE).execute(&pool).await.unwrap();
        let blob = root.join(RELATIVE);
        fs::create_dir_all(blob.parent().unwrap()).unwrap();
        fs::write(&blob, ORIGINAL).unwrap();
        Self { pool, root, _scope: scope }
    }
    fn blob(&self) -> PathBuf { self.root.join(RELATIVE) }
    fn writer(&self) -> WriterLease {
        let request = UploadRequest {draft_scope:None,
            operation_id: uuid::Uuid::new_v4().to_string(), origin: "capacity-custody-fixture".into(),
            actor_id: "owner".into(), space_id: "space".into(), document_id: "page".into(),
            original_name: "New.bin".into(), mime_type: "application/octet-stream".into(),
        };
        WriterLease::begin(&self.root, &uuid::Uuid::new_v4().to_string(), Producer::Direct { request }, MAXIMUM).unwrap()
    }
}
type Reservation = (String, i64, String, Option<String>, Option<i64>, String);

async fn row(pool: &AnyPool, lease: &WriterLease) -> Reservation {
    sqlx::query_as("SELECT request_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,admission_claim_sha256 FROM attachment_capacity_reservations WHERE operation_id=$1")
        .bind(lease.attempt_id()).fetch_one(pool).await.unwrap()
}
async fn assert_retained(fixture: &Fixture, lease: &WriterLease, original_claim: &[u8], capsule: (u64,u64)) {
    assert_eq!(row(&fixture.pool, lease).await, (
        lease.producer_sha256().to_owned(), MAXIMUM as i64, "reserved".into(), None, None, lease.claim_sha256(),
    ));
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_capacity_reservations").fetch_one(&fixture.pool).await.unwrap(), 1);
    for table in ["attachments", "attachment_upload_receipts", "asset_replay_receipts"] {
        assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT count(*) FROM {table}")).fetch_one(&fixture.pool).await.unwrap(), 0, "No asset or winning receipt may be fabricated");
    }
    let directory = fixture.root.join(".attachment-upload-admission").join(lease.attempt_id());
    let metadata = fs::metadata(&directory).unwrap();
    assert_eq!((metadata.dev(), metadata.ino()), capsule);
    assert_eq!(fs::read(directory.join("claim.json")).unwrap(), original_claim);
    lease.verify().unwrap();
}
fn capsule_identity(fixture: &Fixture, lease: &WriterLease) -> (u64,u64) {
    let metadata = fs::metadata(fixture.root.join(".attachment-upload-admission").join(lease.attempt_id())).unwrap();
    (metadata.dev(), metadata.ino())
}

#[tokio::test(flavor="current_thread")]
async fn real_commit_then_retained_mutation_refuses_grant_but_keeps_original_reservation() {
    for mode in ["write-revert", "cancelled-open"] {
        let fixture = Fixture::new().await;
        let lease = fixture.writer();
        let original_claim = lease.original_claim().to_vec();
        let capsule = capsule_identity(&fixture, &lease);
        let blob = fixture.blob();
        let mut after_commit = false;
        let result = reserve_observed(&fixture.pool, &lease, Some(40), None, |phase| {
            if phase == "after-commit" {
                after_commit = true;
                if mode == "write-revert" {
                    fs::write(&blob, b"MUTATED!")?;
                    fs::write(&blob, ORIGINAL)?;
                } else {
                    // No bytes are written even if the broker releases promptly enough for open to succeed.
                    let _attempt = fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(&blob);
                }
            }
            Ok(())
        }).await;
        assert!(after_commit, "The test must cross the actual successful SQL commit");
        let error = result.expect_err("Post-commit custody loss must not return fresh admission");
        assert!(error.to_string().contains("custody"), "Expected physical custody refusal: {error:#}");
        assert_eq!(fs::read(&blob).unwrap(), ORIGINAL);
        assert_retained(&fixture, &lease, &original_claim, capsule).await;
        fixture.pool.close().await;
    }
}

#[tokio::test(flavor="current_thread")]
async fn reserved_retry_rechecks_length_and_honors_original_maximum_after_quota_reduction() {
    let fixture = Fixture::new().await;
    let lease = fixture.writer();
    let original_claim = lease.original_claim().to_vec();
    let capsule = capsule_identity(&fixture, &lease);
    assert_eq!(reserve(&fixture.pool, &lease, Some(40)).await.unwrap(), Admission::Reserved);
    let original_row = row(&fixture.pool, &lease).await;
    fs::write(fixture.blob(), b"wrong length").unwrap();
    assert!(reserve(&fixture.pool, &lease, Some(0)).await.is_err(), "A prior reserved row cannot bypass the fresh physical census");
    assert_eq!(fs::read(fixture.blob()).unwrap(), b"wrong length");
    assert_eq!(row(&fixture.pool, &lease).await, original_row);
    assert_retained(&fixture, &lease, &original_claim, capsule).await;
    fs::write(fixture.blob(), ORIGINAL).unwrap();
    // The legacy source has no digest: this checks fresh length/custody, not invented historical hashing.
    assert_eq!(reserve(&fixture.pool, &lease, Some(0)).await.unwrap(), Admission::Reserved);
    assert_eq!(row(&fixture.pool, &lease).await, original_row);
    assert_retained(&fixture, &lease, &original_claim, capsule).await;
    fixture.pool.close().await;
}

#[tokio::test(flavor="current_thread")]
async fn retry_rollback_wait_cannot_reuse_lost_custody_and_fresh_retry_keeps_same_row() {
    for mode in ["cancelled-open", "alias-return"] {
        let fixture = Fixture::new().await;
        let lease = fixture.writer();
        assert_eq!(reserve(&fixture.pool, &lease, Some(40)).await.unwrap(), Admission::Reserved);
        let original_row = row(&fixture.pool, &lease).await;
        let original_claim = lease.original_claim().to_vec();
        let capsule = capsule_identity(&fixture, &lease);
        let blob = fixture.blob();
        let alias = fixture.root.join("cancelled-retry-alias");
        let mut after_rollback = false;
        let result = reserve_observed(&fixture.pool, &lease, Some(0), None, |phase| {
            if phase == "after-retry-rollback" {
                after_rollback = true;
                if mode == "cancelled-open" {
                    let _attempt = fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(&blob);
                } else {
                    fs::hard_link(&blob, &alias)?;
                    fs::remove_file(&alias)?;
                }
            }
            Ok(())
        }).await;
        assert!(after_rollback, "The test must cross the actual retry SQL rollback");
        let error = result.expect_err("The returned reservation must not inherit a broken physical observation");
        assert!(error.to_string().contains("custody"), "Expected physical custody refusal: {error:#}");
        assert_eq!(fs::read(&blob).unwrap(), ORIGINAL);
        assert_eq!(fs::metadata(&blob).unwrap().nlink(), 1);
        assert!(!alias.exists());
        assert_eq!(row(&fixture.pool, &lease).await, original_row);
        assert_retained(&fixture, &lease, &original_claim, capsule).await;
        // This is a new observation of the SAME original producer, not rearming the poisoned set.
        assert_eq!(reserve(&fixture.pool, &lease, Some(0)).await.unwrap(), Admission::Reserved);
        assert_eq!(row(&fixture.pool, &lease).await, original_row);
        assert_retained(&fixture, &lease, &original_claim, capsule).await;
        fixture.pool.close().await;
    }
}

#[tokio::test(flavor="current_thread")]
async fn delete_and_wal_real_commit_then_original_retry_preserve_one_reservation_under_reduced_quota() {
    for journal in ["delete", "wal"] {
        let fixture = Fixture::with_journal(journal).await;
        let lease = fixture.writer();
        let original_claim = lease.original_claim().to_vec();
        let capsule = capsule_identity(&fixture, &lease);
        let mut committed = false;
        let admitted = reserve_observed(&fixture.pool, &lease, Some(40), None, |phase| {
            if phase == "after-commit" { committed = true; }
            Ok(())
        }).await.unwrap();
        assert!(committed, "Positive {journal} admission must cross a real SQL commit");
        assert_eq!(admitted, Admission::Reserved);
        let original_row = row(&fixture.pool, &lease).await;
        assert_retained(&fixture, &lease, &original_claim, capsule).await;
        let mut rolled_back = false;
        let retried = reserve_observed(&fixture.pool, &lease, Some(0), None, |phase| {
            if phase == "after-retry-rollback" { rolled_back = true; }
            Ok(())
        }).await.unwrap();
        assert!(rolled_back, "Positive {journal} retry must cross the real rollback");
        assert_eq!(retried, Admission::Reserved);
        assert_eq!(row(&fixture.pool, &lease).await, original_row);
        assert_retained(&fixture, &lease, &original_claim, capsule).await;
        assert_eq!(fs::read(fixture.blob()).unwrap(), ORIGINAL);
        let actual: String = sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&fixture.pool).await.unwrap();
        assert_eq!(actual, journal, "Capacity observation must not change the selected journal profile");
        fixture.pool.close().await;
    }
}

#[tokio::test(flavor="current_thread")]
async fn aggregate_charge_rechecks_blob_custody_after_other_proofs() {
    let fixture=Fixture::new().await;
    let mut tx=fixture.pool.begin().await.unwrap();
    let held=super::retained::charge(&mut tx,true).await.unwrap();
    let result=held.test_fence_after_proofs(|| {
        fs::write(fixture.blob(),b"MUTATED!")?;
        fs::write(fixture.blob(),ORIGINAL)?;
        Ok(())
    });
    assert!(result.unwrap_err().to_string().contains("custody"));
    assert_eq!(fs::read(fixture.blob()).unwrap(),ORIGINAL);
    held.release_async().await.unwrap();
    tx.rollback().await.unwrap();
    fixture.pool.close().await;
}


#[tokio::test(flavor="current_thread")]
async fn cancelled_actual_delete_commit_wait_retains_original_request_and_releases_blob_custody() {
    use sqlx::Connection;
    use std::{io::Write, sync::{Arc, atomic::{AtomicBool, Ordering}}, time::Duration};
    let fixture = Fixture::new().await;
    let lease = fixture.writer();
    let original_claim = lease.original_claim().to_vec();
    let capsule = capsule_identity(&fixture, &lease);
    let mut reader = fixture.pool.begin().await.unwrap();
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachment_capacity_reservations")
        .fetch_one(&mut *reader).await.unwrap(), 0);
    // This independent connection distinguishes a queued COMMIT from SQLite's
    // actual PENDING lock: RESERVED allows new readers, PENDING refuses them.
    let mut probe = sqlx::AnyConnection::connect(&format!("sqlite://{}?mode=rw", fixture.root.join("fixture.sqlite").display())).await.unwrap();
    sqlx::query("PRAGMA busy_timeout=0").execute(&mut probe).await.unwrap();
    assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachment_capacity_reservations")
        .fetch_one(&mut probe).await.unwrap(), 0);
    let (before_commit, reached_commit) = tokio::sync::oneshot::channel();
    let mut before_commit = Some(before_commit);
    let positive = Arc::new(AtomicBool::new(false));
    let caller_positive = positive.clone();
    let pool = fixture.pool.clone();
    let caller_lease = lease.clone();
    let caller = tokio::spawn(async move {
        let result = reserve_observed(&pool, &caller_lease, Some(40), None, move |phase| {
            if phase == "before-commit" {
                if let Some(sender) = before_commit.take() { let _ = sender.send(()); }
            }
            Ok(())
        }).await;
        if result.is_ok() { caller_positive.store(true, Ordering::SeqCst); }
        result
    });
    tokio::time::timeout(Duration::from_secs(3), reached_commit).await.unwrap().unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachment_capacity_reservations").fetch_one(&mut probe).await {
                Err(error) if error.as_database_error().is_some_and(|database| {
                    database.code().as_deref() == Some("5") || database.code().as_deref() == Some("SQLITE_BUSY")
                }) => break,
                Ok(0) => tokio::task::yield_now().await,
                other => panic!("Expected original reader or actual SQLite COMMIT PENDING refusal, got {other:?}"),
            }
        }
    }).await.expect("Reservation must reach actual SQLite COMMIT PENDING while original reader holds SHARED");
    assert!(!caller.is_finished(), "COMMIT cannot finish while the original read lock is held");
    assert!(!positive.load(Ordering::SeqCst));
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert!(!positive.load(Ordering::SeqCst), "Canceled caller cannot acknowledge admission");
    // Keep the SQL reader held during this check: file custody must be released
    // by cancellation, not by a later successful COMMIT or rollback acknowledgement.
    let blob = fixture.blob();
    let mut physical_writer = tokio::task::spawn_blocking(move || {
        let mut file = fs::OpenOptions::new().write(true).open(blob)?;
        file.write_all(ORIGINAL)?;
        file.sync_all()
    });
    tokio::time::timeout(Duration::from_secs(3), &mut physical_writer).await
        .expect("Cancellation must promptly release blob read custody during SQL wait").unwrap().unwrap();
    assert_eq!(fs::read(fixture.blob()).unwrap(), ORIGINAL);
    reader.rollback().await.unwrap();
    probe.close().await.unwrap();
    // SQLx may already have queued COMMIT when its caller is canceled. Observe
    // the real outcome; neither branch supplies an invented rollback or receipt.
    let rows: Vec<Reservation> = tokio::time::timeout(Duration::from_secs(5), async {
        sqlx::query_as("SELECT request_sha256,maximum_bytes,state,committed_asset_id,committed_bytes,admission_claim_sha256 FROM attachment_capacity_reservations")
            .fetch_all(&fixture.pool).await
    }).await.unwrap().unwrap();
    assert!(rows.len() <= 1);
    if let Some(observed) = rows.first() {
        assert_eq!(*observed, (lease.producer_sha256().to_owned(), MAXIMUM as i64, "reserved".into(), None, None, lease.claim_sha256()));
        assert_eq!(sqlx::query_scalar::<_, String>("SELECT operation_id FROM attachment_capacity_reservations")
            .fetch_one(&fixture.pool).await.unwrap(), lease.attempt_id());
    }
    for table in ["attachments", "attachment_upload_receipts", "asset_replay_receipts", "attachment_erasure_dispositions"] {
        assert_eq!(sqlx::query_scalar::<_, i64>(&format!("SELECT count(*) FROM {table}"))
            .fetch_one(&fixture.pool).await.unwrap(), 0, "Canceled admission fabricated {table}");
    }
    let directory = fixture.root.join(".attachment-upload-admission").join(lease.attempt_id());
    let metadata = fs::metadata(&directory).unwrap();
    assert_eq!((metadata.dev(), metadata.ino()), capsule);
    assert_eq!(fs::read(directory.join("claim.json")).unwrap(), original_claim);
    lease.verify().unwrap();
    assert!(!positive.load(Ordering::SeqCst));
    println!("ACTUAL_DELETE_COMMIT_PENDING_CANCELLED original_reservation_rows={} positive_admission=false original_capsule_retained=true blob_custody_released=true", rows.len());
    fixture.pool.close().await;
}
