use super::*;
use sqlx::sqlite::SqliteJournalMode;
use std::fs;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

fn fixture(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "starlane-native-download-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    root
}
fn database(root: &Path) -> filesystem::Directory {
    let dir = filesystem::Directory::capture(root).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(root.join("native.sqlite3"))
        .unwrap()
        .sync_all()
        .unwrap();
    dir.held.sync_all().unwrap();
    dir
}
async fn connect(v: &vfs::NativeVfs) -> Result<Pool<Sqlite>, sqlx::Error> {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(v.filename())
                .vfs(v.name())
                .journal_mode(SqliteJournalMode::Delete)
                .synchronous(SqliteSynchronous::Full),
        )
        .await
}
#[tokio::test]
async fn physical_vfs_runs_actual_native_catalog_and_snapshot_transaction() {
    let root = fixture("catalog");
    let v = vfs::NativeVfs::new(database(&root), "native.sqlite3").unwrap();
    let pool = connect(&v)
        .await
        .unwrap_or_else(|e| panic!("{e}; {:?}", v.last_error()));
    migrate_database(&pool).await.unwrap();
    let snapshot = json!({"spaces":[],"documents":[],"assets":[{"id":"immutable-local","filename":"original.bin","future_sidecar":{"original_server_version":"version-1","raw_receipt":"exact raw"}}]});
    let mut tx = pool.begin().await.unwrap();
    write_snapshot_in_tx(&mut tx, "none", &snapshot)
        .await
        .unwrap();
    v.check().unwrap();
    tx.commit().await.unwrap();
    let read = read_snapshot_from_pool(&pool, "none")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(read["assets"], snapshot["assets"]);
    pool.close().await;
    drop(v);
    assert!(root.join("native.sqlite3").metadata().unwrap().len() > 0);
    fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn physical_vfs_refuses_exchanged_root_before_database_open() {
    let root = fixture("root-before");
    let v = vfs::NativeVfs::new(database(&root), "native.sqlite3").unwrap();
    let moved = root.with_extension("original");
    fs::rename(&root, &moved).unwrap();
    fs::create_dir(&root).unwrap();
    fs::copy(moved.join("native.sqlite3"), root.join("native.sqlite3")).unwrap();
    let before = fs::read(root.join("native.sqlite3")).unwrap();
    assert!(connect(&v).await.is_err());
    assert_eq!(fs::read(root.join("native.sqlite3")).unwrap(), before);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    drop(v);
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(moved).unwrap();
}
#[tokio::test]
async fn physical_vfs_refuses_exchanged_root_after_pager_open() {
    let root = fixture("root-open");
    let v = vfs::NativeVfs::new(database(&root), "native.sqlite3").unwrap();
    let pool = connect(&v).await.unwrap();
    sqlx::query("CREATE TABLE original(value TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    let moved = root.with_extension("original");
    fs::rename(&root, &moved).unwrap();
    fs::create_dir(&root).unwrap();
    fs::copy(moved.join("native.sqlite3"), root.join("native.sqlite3")).unwrap();
    let before = fs::read(root.join("native.sqlite3")).unwrap();
    assert!(
        sqlx::query("INSERT INTO original VALUES ('must-not-write')")
            .execute(&pool)
            .await
            .is_err()
    );
    assert_eq!(fs::read(root.join("native.sqlite3")).unwrap(), before);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
    pool.close().await;
    drop(v);
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(moved).unwrap();
}
#[tokio::test]
async fn physical_vfs_refuses_equal_byte_database_replacement() {
    let root = fixture("db-copy");
    let v = vfs::NativeVfs::new(database(&root), "native.sqlite3").unwrap();
    let pool = connect(&v).await.unwrap();
    sqlx::query("CREATE TABLE original(value TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    fs::copy(root.join("native.sqlite3"), root.join("copied.sqlite3")).unwrap();
    fs::rename(root.join("copied.sqlite3"), root.join("native.sqlite3")).unwrap();
    let before = fs::read(root.join("native.sqlite3")).unwrap();
    assert!(
        sqlx::query("INSERT INTO original VALUES ('must-not-write')")
            .execute(&pool)
            .await
            .is_err()
    );
    assert_eq!(fs::read(root.join("native.sqlite3")).unwrap(), before);
    pool.close().await;
    drop(v);
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn physical_connection_retirement_waits_for_actual_borrowed_sqlite_connection() {
    let root = fixture("retirement");
    let mut owner = connection::Database::prepare(database(&root), "native.sqlite3")
        .await
        .unwrap();
    let borrowed = owner.connection();
    sqlx::query("CREATE TABLE original(value TEXT)")
        .execute(&mut *borrowed)
        .await
        .unwrap();
    sqlx::query("INSERT INTO original VALUES ('held-worker')")
        .execute(&mut *borrowed)
        .await
        .unwrap();
    owner.retire().await.unwrap();
    let plain = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(root.join("native.sqlite3")))
        .await
        .unwrap();
    let value: String = sqlx::query_scalar("SELECT value FROM original")
        .fetch_one(&plain)
        .await
        .unwrap();
    assert_eq!(value, "held-worker");
    plain.close().await;
    fs::remove_dir_all(root).unwrap();
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn physical_connection_canceled_establishment_retires_safely() {
    use std::sync::{mpsc, Arc, Condvar, Mutex};
    let root = fixture("supervised-cancel");
    let directory = database(&root);
    let open = Arc::new((Mutex::new(false), Condvar::new()));
    let closing = Arc::new((Mutex::new(false), Condvar::new()));
    let (opened_send, opened) = mpsc::channel();
    let (close_send, close_entered) = mpsc::channel();
    let (finished_send, finished) = tokio::sync::oneshot::channel();
    let open_gate = open.clone();
    let close_gate = closing.clone();
    let retirement_armed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let hook_armed = retirement_armed.clone();
    let caller = tokio::spawn(async move {
        connection::supervise(async move {
        let _snapshot=snapshot_commit_lock().lock().await;
        let mut owner=connection::Database::prepare_gated(directory,"native.sqlite3",Box::new(move||{opened_send.send(()).unwrap();let (lock,condition)=&*open_gate;let mut ready=lock.lock().unwrap();while !*ready{ready=condition.wait(ready).unwrap();}}),Box::new(move|key|{if key.ends_with("-journal") && hook_armed.load(std::sync::atomic::Ordering::Acquire){close_send.send(()).unwrap();let(lock,condition)=&*close_gate;let mut ready=lock.lock().unwrap();while !*ready{ready=condition.wait(ready).unwrap();}}})).await?;
        let result=async {let borrowed=owner.connection();sqlx::query("CREATE TABLE original(value TEXT); INSERT INTO original VALUES('preserved'); BEGIN IMMEDIATE; UPDATE original SET value='uncommitted';").execute(&mut *borrowed).await.map_err(|e|e.to_string())?;Ok::<_,String>(())}.await;
        retirement_armed.store(true,std::sync::atomic::Ordering::Release);owner.retire().await.map_err(|e|e.to_string())?;drop(_snapshot);let _=finished_send.send(());result
    }).await
    });
    opened
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert!(snapshot_commit_lock().try_lock().is_err());
    {
        let (lock, condition) = &*open;
        *lock.lock().unwrap() = true;
        condition.notify_all();
    }
    close_entered
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    assert!(snapshot_commit_lock().try_lock().is_err());
    {
        let (lock, condition) = &*closing;
        *lock.lock().unwrap() = true;
        condition.notify_all();
    }
    tokio::time::timeout(std::time::Duration::from_secs(10), finished)
        .await
        .unwrap()
        .unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(root.join("native.sqlite3")))
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT value FROM original")
            .fetch_one(&pool)
            .await
            .unwrap(),
        "preserved"
    );
    pool.close().await;
    println!("ACTUAL_CANCELED_REPLY_DELAYED_XOPEN_AND_SQLITE_CLOSE_MUTEX_RETENTION_PASS");
    fs::remove_dir_all(root).unwrap();
}

fn import_request(root: &Path, bytes: &[u8]) -> custody::Request {
    use sha2::{Digest, Sha256};
    let selection = vault::SnapshotRequestBinding::capture_at(
        root.with_extension("settings.json"),
        root.into(),
        "none",
    )
    .unwrap();
    let destination = custody::Destination::capture(&selection).unwrap();
    custody::Request{destination,association:custody::Association{space_id:"local-space".into(),document_id:"prior-page".into()},operation_id:"7d0e0c9f-66b8-4cb0-8cc9-1b1495e18575".into(),origin:json!({"version":1,"serverBase":"https://fixture.invalid","accountId":"native-fixture-account","sessionId":"original-session","cacheScope":"session-native-fixture","storageMode":"server","provider":"none"}),server_version_raw:serde_json::to_string(&json!({"version_id":"original-immutable-version","logical_attachment_id":"original-family","version_number":2,"original_space_id":"original-space","original_document_id":"original-page","uploader_id":"original-uploader","original_name":"same-name.bin","mime_type":"application/octet-stream","byte_length":bytes.len(),"binary_sha256":format!("{:x}",Sha256::digest(bytes)),"original_created_at":"2026-10-10T12:00:00.000Z"})).unwrap()}
}
async fn initialized(root: &Path) {
    fs::create_dir(root.join(".starlane")).unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(root.join(".starlane/starlane-notes.sqlite3"))
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Delete),
        )
        .await
        .unwrap();
    migrate_database(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    write_snapshot_in_tx(&mut tx,"none",&json!({"spaces":[{"id":"local-space","name":"Actual local page space"}],"assets":[],"documents":[{"id":"prior-page","space_id":"local-space","title":"Preserved prior page"}]})).await.unwrap();
    tx.commit().await.unwrap();
    pool.close().await;
}
fn capture(root: &Path, request: custody::Request) -> custody::Captured {
    let selection = vault::SnapshotRequestBinding::capture_at(
        root.with_extension("settings.json"),
        root.into(),
        "none",
    )
    .unwrap();
    let current = request.origin.clone();
    custody::Captured::capture(selection, request, &current).unwrap()
}
#[tokio::test]
async fn native_import_exact_retry_keeps_original_ack_and_readonly_server_provenance() {
    let root = fixture("receipt");
    initialized(&root).await;
    let bytes = vec![19; 150001];
    let request = import_request(&root, &bytes);
    let owner = capture(&root, request.clone());
    assert!(owner
        .prepare_fetch(&request.origin)
        .await
        .unwrap()
        .is_none());
    owner.begin(&request.origin).await.unwrap();
    for (index, part) in bytes.chunks(65536).enumerate() {
        owner
            .append(&request.origin, index as u64, part.to_vec())
            .await
            .unwrap();
    }
    let ack = owner.finish(&request.origin).await.unwrap();
    assert_eq!(ack["state"], "current");
    assert_ne!(
        ack["original_ack"]["asset"]["id"],
        "original-immutable-version"
    );
    assert_eq!(
        ack["original_ack"]["asset"]["server_version_provenance"]["raw_version_record"],
        request.server_version_raw
    );
    let dir = root
        .join(".starlane/attachment-downloads")
        .join(&request.operation_id);
    assert_eq!(fs::read(dir.join("asset.bin")).unwrap(), bytes);
    let fresh = capture(&root, request.clone());
    fresh.begin(&request.origin).await.unwrap();
    assert_eq!(
        fresh.prepare_fetch(&request.origin).await.unwrap(),
        Some(ack.clone())
    );
    assert_eq!(fresh.finish(&request.origin).await.unwrap(), ack);
    let mut other = request.origin.clone();
    other["sessionId"] = json!("changed");
    assert!(fresh
        .finish(&other)
        .await
        .unwrap_err()
        .contains("context changed"));
    assert_eq!(fs::read(dir.join("asset.bin")).unwrap(), bytes);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3")),
        )
        .await
        .unwrap();
    let snapshot = read_snapshot_from_pool(&pool, "none")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot["documents"][0]["title"], "Preserved prior page");
    assert_eq!(snapshot["assets"].as_array().unwrap().len(), 1);
    pool.close().await;
    fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn native_import_real_database_failure_retains_bytes_and_rolls_back_metadata() {
    let root = fixture("rollback");
    initialized(&root).await;
    let bytes = b"bytes retained after real SQL failure".to_vec();
    let request = import_request(&root, &bytes);
    let owner = capture(&root, request.clone());
    owner.begin(&request.origin).await.unwrap();
    owner
        .append(&request.origin, 0, bytes.clone())
        .await
        .unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3")),
        )
        .await
        .unwrap();
    let before = read_snapshot_from_pool(&pool, "none")
        .await
        .unwrap()
        .unwrap();
    sqlx::query("CREATE TRIGGER fail_owned_import BEFORE INSERT ON local_assets BEGIN SELECT RAISE(ABORT,'deliberate native metadata failure'); END").execute(&pool).await.unwrap();
    pool.close().await;
    assert!(owner
        .finish(&request.origin)
        .await
        .unwrap_err()
        .contains("deliberate native metadata failure"));
    let path = root
        .join(".starlane/attachment-downloads")
        .join(&request.operation_id);
    assert_eq!(fs::read(path.join("asset.bin")).unwrap(), bytes);
    assert!(path.join("install.json").exists());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3")),
        )
        .await
        .unwrap();
    assert_eq!(
        read_snapshot_from_pool(&pool, "none")
            .await
            .unwrap()
            .unwrap(),
        before
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM native_attachment_download_receipts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("DROP TRIGGER fail_owned_import")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    assert_eq!(
        owner.finish(&request.origin).await.unwrap()["state"],
        "current"
    );
    fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn native_import_changed_selection_refuses_before_other_root_writes() {
    let root = fixture("selection");
    let other = fixture("selection-other");
    initialized(&root).await;
    initialized(&other).await;
    let request = import_request(&root, b"original");
    let owner = capture(&root, request.clone());
    fs::write(
        root.with_extension("settings.json"),
        serde_json::to_vec(&json!({"vault_path":other})).unwrap(),
    )
    .unwrap();
    let before = fs::read(other.join(".starlane/starlane-notes.sqlite3")).unwrap();
    assert!(owner
        .begin(&request.origin)
        .await
        .unwrap_err()
        .contains("selection changed"));
    assert_eq!(
        fs::read(other.join(".starlane/starlane-notes.sqlite3")).unwrap(),
        before
    );
    assert!(!other.join(".starlane/attachment-downloads").exists());
    assert!(!root.join(".starlane/attachment-downloads").exists());
    fs::remove_file(root.with_extension("settings.json")).unwrap();
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(other).unwrap();
}
#[tokio::test]
async fn native_import_historical_receipt_never_recreates_missing_asset_or_file() {
    let root = fixture("historical");
    initialized(&root).await;
    let bytes = b"historical bytes".to_vec();
    let request = import_request(&root, &bytes);
    let owner = capture(&root, request.clone());
    owner.begin(&request.origin).await.unwrap();
    owner.append(&request.origin, 0, bytes).await.unwrap();
    let ack = owner.finish(&request.origin).await.unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3")),
        )
        .await
        .unwrap();
    let mut snapshot = read_snapshot_from_pool(&pool, "none")
        .await
        .unwrap()
        .unwrap();
    snapshot["assets"] = json!([]);
    let mut tx = pool.begin().await.unwrap();
    write_snapshot_in_tx(&mut tx, "none", &snapshot)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    pool.close().await;
    let final_path = root
        .join(".starlane/attachment-downloads")
        .join(&request.operation_id)
        .join("asset.bin");
    fs::remove_file(&final_path).unwrap();
    let fresh = capture(&root, request.clone());
    let historical = fresh.finish(&request.origin).await.unwrap();
    assert_eq!(historical["state"], "historical");
    assert_eq!(historical["original_ack"], ack["original_ack"]);
    assert!(!final_path.exists());
    fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn native_import_restart_child() {
    let Ok(root) = std::env::var("STARLANE_NATIVE_IMPORT_FIXTURE_ROOT") else {
        return;
    };
    let mode = std::env::var("STARLANE_NATIVE_IMPORT_FIXTURE_PHASE").unwrap();
    let root = PathBuf::from(root);
    assert!(root.is_absolute());
    let bytes = vec![71; 70000];
    let request_path = std::env::var("STARLANE_NATIVE_IMPORT_FIXTURE_REQUEST").unwrap();
    let request: custody::Request =
        serde_json::from_slice(&fs::read(request_path).unwrap()).unwrap();
    if mode == "changed-root" {
        let selection = vault::SnapshotRequestBinding::capture_at(
            root.with_extension("settings.json"),
            root.clone(),
            "none",
        )
        .unwrap();
        assert!(
            custody::Captured::capture(selection, request.clone(), &request.origin)
                .err()
                .unwrap()
                .contains("caller-persisted native destination changed")
        );
        assert!(!root.join(".starlane/attachment-downloads").exists());
        return;
    }
    if mode == "changed-actor" {
        let mut current = request.origin.clone();
        current["accountId"] = json!("different-caller-account");
        let selection = vault::SnapshotRequestBinding::capture_at(
            root.with_extension("settings.json"),
            root.clone(),
            "none",
        )
        .unwrap();
        assert!(
            custody::Captured::capture(selection, request.clone(), &current)
                .err()
                .unwrap()
                .contains("context changed")
        );
        return;
    }
    let owner = capture(&root, request.clone());
    owner.begin(&request.origin).await.unwrap();
    if mode == "writer" {
        for (i, p) in bytes.chunks(65536).enumerate() {
            owner
                .append(&request.origin, i as u64, p.to_vec())
                .await
                .unwrap();
        }
        let ack = owner.finish(&request.origin).await.unwrap();
        fs::write(
            root.join("expected-native-ack.json"),
            serde_json::to_vec(&ack).unwrap(),
        )
        .unwrap();
        std::process::exit(77);
    }
    assert_eq!(mode, "restart");
    let expected: Value =
        serde_json::from_slice(&fs::read(root.join("expected-native-ack.json")).unwrap()).unwrap();
    assert_eq!(owner.finish(&request.origin).await.unwrap(), expected);
}
#[test]
fn native_import_real_process_restart_resolves_lost_ack() {
    real_process_restart(false);
}
#[test]
fn native_import_existing_wal_real_restart_resolves_original_lost_ack_preserving_shm() {
    real_process_restart(true);
}
fn real_process_restart(wal: bool) {
    let root = fixture("process");
    let other = fixture("process-other");
    let runtime = tokio::runtime::Runtime::new().unwrap();
    if wal {
        runtime.block_on(persist_actual_wal_sidecars(&root));
    } else {
        runtime.block_on(initialized(&root));
    }
    let original_shm =
        wal.then(|| fs::read(root.join(".starlane/starlane-notes.sqlite3-shm")).unwrap());
    runtime.block_on(initialized(&other));
    let request = import_request(&root, &vec![71; 70000]);
    let request_path = root.join("original-caller-request.json");
    let mut persisted = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&request_path)
        .unwrap();
    std::io::Write::write_all(&mut persisted, &serde_json::to_vec(&request).unwrap()).unwrap();
    persisted.sync_all().unwrap();
    fs::File::open(&root).unwrap().sync_all().unwrap();
    drop(persisted);
    let selector = "local_database::attachment_download_owner::tests::native_import_restart_child";
    let child = |phase: &str, destination: &Path| {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", selector, "--nocapture"])
            .env("STARLANE_NATIVE_IMPORT_FIXTURE_ROOT", destination)
            .env("STARLANE_NATIVE_IMPORT_FIXTURE_PHASE", phase)
            .env("STARLANE_NATIVE_IMPORT_FIXTURE_REQUEST", &request_path)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if std::time::Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("Owned native fixture child timed out: {phase}");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        child.wait_with_output().unwrap()
    };
    let writer = child("writer", &root);
    assert_eq!(
        writer.status.code(),
        Some(77),
        "{}{}",
        String::from_utf8_lossy(&writer.stdout),
        String::from_utf8_lossy(&writer.stderr)
    );
    println!("Actual native writer committed original receipt then exited 77 before caller acknowledgement");
    for (phase, destination) in [
        ("restart", root.as_path()),
        ("changed-root", other.as_path()),
        ("changed-actor", root.as_path()),
    ] {
        let result = child(phase, destination);
        assert!(
            result.status.success(),
            "{phase}: {}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(String::from_utf8_lossy(&result.stdout).contains("1 passed"));
        println!("Actual native fresh child {phase}: one guarded assertion test passed, exit0");
    }
    assert!(!other.join(".starlane/attachment-downloads").exists());
    if let Some(original) = original_shm {
        assert_eq!(
            fs::read(root.join(".starlane/starlane-notes.sqlite3-shm")).unwrap(),
            original
        );
        assert_eq!(
            &fs::read(root.join(".starlane/starlane-notes.sqlite3")).unwrap()[18..20],
            &[2, 2]
        );
    }
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(other).unwrap();
}

#[tokio::test]
async fn native_import_changed_operation_tuple_and_chunk_refuse_without_overwrite() {
    let root = fixture("retry-conflict");
    initialized(&root).await;
    let bytes = b"immutable original".to_vec();
    let request = import_request(&root, &bytes);
    let owner = capture(&root, request.clone());
    owner.begin(&request.origin).await.unwrap();
    owner
        .append(&request.origin, 0, bytes.clone())
        .await
        .unwrap();
    let changed = b"different original".to_vec();
    assert_eq!(changed.len(), bytes.len());
    assert!(owner
        .append(&request.origin, 0, changed)
        .await
        .unwrap_err()
        .contains("chunk identity/bytes differ"));
    for key in ["accountId", "serverBase", "sessionId"] {
        let mut current = request.origin.clone();
        current[key] = json!("changed-context");
        assert!(owner
            .finish(&current)
            .await
            .unwrap_err()
            .contains("context changed"));
    }
    let mut changed = request.clone();
    let mut version: Value = serde_json::from_str(&changed.server_version_raw).unwrap();
    version["original_name"] = json!("new name");
    changed.server_version_raw = serde_json::to_string(&version).unwrap();
    let other = capture(&root, changed.clone());
    assert!(other
        .begin(&changed.origin)
        .await
        .unwrap_err()
        .contains("binding differs"));
    assert_eq!(
        fs::read(
            root.join(".starlane/attachment-downloads")
                .join(&request.operation_id)
                .join("part-0.bin")
        )
        .unwrap(),
        bytes
    );
    assert!(!root
        .join(".starlane/attachment-downloads")
        .join(&request.operation_id)
        .join("asset.bin")
        .exists());
    fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn native_import_actual_existing_wal_header_is_preserved_with_heap_index() {
    let root = fixture("wal-header");
    initialized(&root).await;
    let path = root.join(".starlane/starlane-notes.sqlite3");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .journal_mode(SqliteJournalMode::Wal),
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO local_vault_metadata(key,value) VALUES('wal-fixture','original')")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    assert_eq!(&fs::read(&path).unwrap()[18..20], &[2, 2]);
    let bytes = b"wal owned version".to_vec();
    let request = import_request(&root, &bytes);
    let owner = capture(&root, request.clone());
    owner.begin(&request.origin).await.unwrap();
    owner.append(&request.origin, 0, bytes).await.unwrap();
    assert_eq!(
        owner.finish(&request.origin).await.unwrap()["state"],
        "current"
    );
    assert_eq!(&fs::read(&path).unwrap()[18..20], &[2, 2]);
    assert!(!path.with_file_name("starlane-notes.sqlite3-shm").exists());
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_import_fresh_process_input_cannot_rebind_original_uuid_to_other_vault() {
    let root = fixture("pin-original");
    let other = fixture("pin-other");
    initialized(&root).await;
    initialized(&other).await;
    let request = import_request(&root, b"original");
    let first = capture(&root, request.clone());
    first.begin(&request.origin).await.unwrap();
    drop(first);
    let original_intent = fs::read(
        root.join(".starlane/attachment-downloads")
            .join(&request.operation_id)
            .join("intent.json"),
    )
    .unwrap();
    let selection = vault::SnapshotRequestBinding::capture_at(
        other.with_extension("settings.json"),
        other.clone(),
        "none",
    )
    .unwrap();
    assert!(
        custody::Captured::capture(selection, request.clone(), &request.origin)
            .err()
            .unwrap()
            .contains("caller-persisted native destination changed")
    );
    assert!(!other.join(".starlane/attachment-downloads").exists());
    assert_eq!(
        fs::read(
            root.join(".starlane/attachment-downloads")
                .join(&request.operation_id)
                .join("intent.json")
        )
        .unwrap(),
        original_intent
    );
    fs::remove_dir_all(root).unwrap();
    fs::remove_dir_all(other).unwrap();
}

#[tokio::test]
async fn native_import_kernel_enospc_child() {
    use std::ffi::CString;
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    let Ok(evidence) = std::env::var("STARLANE_NATIVE_IMPORT_CAPACITY_EVIDENCE") else {
        return;
    };
    let root = fixture("kernel-enospc");
    let available = || {
        let name = CString::new(root.as_os_str().as_bytes()).unwrap();
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        assert_eq!(
            unsafe { libc::statvfs(name.as_ptr(), stat.as_mut_ptr()) },
            0
        );
        let stat = unsafe { stat.assume_init() };
        assert_eq!(
            stat.f_blocks * stat.f_frsize,
            67108864,
            "Requires uniquely owned 64MiB tmpfs, never shared host storage"
        );
        stat.f_bavail * stat.f_frsize
    };
    let _ = available();
    initialized(&root).await;
    let bytes = vec![37; 131072];
    let request = import_request(&root, &bytes);
    let owner = capture(&root, request.clone());
    owner.begin(&request.origin).await.unwrap();
    let filler_path = root.join("owned-capacity-filler");
    let mut filler = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&filler_path)
        .unwrap();
    let block = [0u8; 65536];
    while available() > 32768 {
        let n = std::cmp::min(65536, available() - 32768) as usize;
        filler.write_all(&block[..n]).unwrap();
    }
    filler.sync_all().unwrap();
    drop(filler);
    let failure = owner
        .append(&request.origin, 0, bytes[..65536].to_vec())
        .await
        .unwrap_err();
    assert!(
        failure.contains("No space left on device") || failure.contains("no space left on device"),
        "Expected real kernel ENOSPC, got {failure}"
    );
    let dir = root
        .join(".starlane/attachment-downloads")
        .join(&request.operation_id);
    let prefix = fs::read(dir.join("part-0.bin")).unwrap();
    assert!(!prefix.is_empty() && prefix.len() < 65536);
    assert_eq!(prefix, bytes[..prefix.len()]);
    assert!(dir.join("part-0.json").exists());
    assert!(!dir.join("asset.bin").exists());
    let evidence = PathBuf::from(evidence);
    assert!(evidence.is_absolute());
    fs::write(evidence.join("retained-prefix.bin"), &prefix).unwrap();
    for name in ["intent.json", "part-0.json"] {
        fs::copy(dir.join(name), evidence.join(name)).unwrap();
    }
    fs::remove_file(filler_path).unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3")),
        )
        .await
        .unwrap();
    let prior = read_snapshot_from_pool(&pool, "none")
        .await
        .unwrap()
        .unwrap();
    assert!(prior["assets"].as_array().unwrap().is_empty());
    assert_eq!(prior["documents"][0]["title"], "Preserved prior page");
    pool.close().await;
    owner
        .append(&request.origin, 0, bytes[..65536].to_vec())
        .await
        .unwrap();
    owner
        .append(&request.origin, 1, bytes[65536..].to_vec())
        .await
        .unwrap();
    let ack = owner.finish(&request.origin).await.unwrap();
    assert_eq!(fs::read(dir.join("asset.bin")).unwrap(), bytes);
    fs::write(evidence.join("kernel-enospc-results.json"),serde_json::to_vec_pretty(&json!({"private_capacity_bytes":67108864,"kernel_error":failure,"retained_partial_bytes":prefix.len(),"original_metadata_usable":true,"exact_retry_completed":true,"original_ack":ack})).unwrap()).unwrap();
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_import_provider_and_total_envelope_refuse_before_stage() {
    let root = fixture("metadata-budget");
    initialized(&root).await;
    let request = import_request(&root, b"source");
    let mut wrong = request.clone();
    wrong.origin["provider"] = json!("google-drive");
    let selection = vault::SnapshotRequestBinding::capture_at(
        root.with_extension("settings.json"),
        root.clone(),
        "none",
    )
    .unwrap();
    assert!(
        custody::Captured::capture(selection, wrong.clone(), &wrong.origin)
            .err()
            .unwrap()
            .contains("provider differs")
    );
    let mut oversized = request;
    oversized.origin["accountId"] = json!("escaped\\\"".repeat(20000));
    let selection = vault::SnapshotRequestBinding::capture_at(
        root.with_extension("settings.json"),
        root.clone(),
        "none",
    )
    .unwrap();
    assert!(
        custody::Captured::capture(selection, oversized.clone(), &oversized.origin)
            .err()
            .unwrap()
            .contains("envelope budget")
    );
    assert!(!root.join(".starlane/attachment-downloads").exists());
    fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn native_import_same_operation_concurrent_retry_returns_one_original_receipt() {
    let root = fixture("concurrent");
    initialized(&root).await;
    let request = import_request(&root, b"same operation bytes");
    let owner = capture(&root, request.clone());
    owner.begin(&request.origin).await.unwrap();
    let (a, b) = tokio::join!(
        owner.append(&request.origin, 0, b"same operation bytes".to_vec()),
        owner.append(&request.origin, 0, b"same operation bytes".to_vec())
    );
    a.unwrap();
    b.unwrap();
    let (a, b) = tokio::join!(owner.finish(&request.origin), owner.finish(&request.origin));
    let a = a.unwrap();
    assert_eq!(a, b.unwrap());
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3")),
        )
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM native_attachment_download_receipts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let mut snapshot = read_snapshot_from_pool(&pool, "none")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot["assets"].as_array().unwrap().len(), 1);
    snapshot["documents"][0]["title"] = json!("Later ordinary page edit");
    let mut tx = pool.begin().await.unwrap();
    let later = write_snapshot_in_tx(&mut tx, "none", &snapshot)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_ne!(
        later,
        a["original_ack"]["canonical_revision"].as_str().unwrap()
    );
    pool.close().await;
    assert_eq!(owner.finish(&request.origin).await.unwrap(), a);
    fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn native_import_zero_byte_version_is_exact_and_does_not_require_fake_chunk() {
    let root = fixture("zero");
    initialized(&root).await;
    let request = import_request(&root, b"");
    let owner = capture(&root, request.clone());
    owner.begin(&request.origin).await.unwrap();
    assert!(owner.append(&request.origin, 0, vec![]).await.is_err());
    let ack = owner.finish(&request.origin).await.unwrap();
    assert_eq!(ack["original_ack"]["asset"]["size"], 0);
    let dir = root
        .join(".starlane/attachment-downloads")
        .join(&request.operation_id);
    assert!(fs::read(dir.join("asset.bin")).unwrap().is_empty());
    assert!(!dir.join("part-0.bin").exists());
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_caller_association_refuses_removed_or_moved_local_page_without_row_recreation() {
    let root = fixture("caller-association");
    initialized(&root).await;
    let bytes = b"actual selected bytes";
    let request = import_request(&root, bytes);
    let owner = capture(&root, request.clone());
    owner.begin(&request.origin).await.unwrap();
    owner
        .append(&request.origin, 0, bytes.to_vec())
        .await
        .unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3")),
        )
        .await
        .unwrap();
    let mut snapshot = read_snapshot_from_pool(&pool, "none")
        .await
        .unwrap()
        .unwrap();
    snapshot["documents"][0]["space_id"] = json!("other-space");
    let mut tx = pool.begin().await.unwrap();
    write_snapshot_in_tx(&mut tx, "none", &snapshot)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    pool.close().await;
    assert!(owner
        .prepare_fetch(&request.origin)
        .await
        .unwrap_err()
        .contains("page association changed"));
    assert!(owner
        .finish(&request.origin)
        .await
        .unwrap_err()
        .contains("page association changed"));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3")),
        )
        .await
        .unwrap();
    let final_snapshot = read_snapshot_from_pool(&pool, "none")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(final_snapshot["documents"][0]["space_id"], "other-space");
    assert!(final_snapshot["assets"].as_array().unwrap().is_empty());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM native_attachment_download_receipts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    pool.close().await;
    assert_eq!(
        fs::read(
            root.join(".starlane/attachment-downloads")
                .join(&request.operation_id)
                .join("asset.bin")
        )
        .unwrap(),
        bytes
    );
    fs::remove_dir_all(root).unwrap();
}

#[tauri::command]
fn joint_ordinary_delete_guard<R: tauri::Runtime>(
    app: AppHandle<R>,
    vault_path: String,
) -> Result<(), String> {
    let selected = vault::capture_snapshot_request(&app, "none")?;
    selected.verify_selection()?;
    let db = selected.expected_database_path()?;
    let root = db
        .parent()
        .and_then(Path::parent)
        .ok_or("Native root missing")?;
    vault::delete_vault_asset_file(root, Path::new(&vault_path))
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_joint_actual_tauri_ipc_child() {
    let Ok(work) = std::env::var("STARLANE_NATIVE_JOINT_WORK") else {
        return;
    };
    let work = PathBuf::from(work);
    assert!(work.is_absolute());
    let phase = std::env::var("STARLANE_NATIVE_JOINT_PHASE").unwrap();
    let app = tauri::test::mock_builder()
        .invoke_handler(tauri::generate_handler![
            capture_native_attachment_destination,
            prepare_native_attachment_download,
            append_native_attachment_download,
            finish_native_attachment_download,
            joint_ordinary_delete_guard
        ])
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
    let window = tauri::WebviewWindowBuilder::new(&app, "joint", Default::default())
        .build()
        .unwrap();
    let invoke = |cmd: &str, args: Value| -> Result<Value, Value> {
        tauri::test::get_ipc_response(
            &window,
            tauri::webview::InvokeRequest {
                cmd: cmd.into(),
                callback: tauri::ipc::CallbackFn(0),
                error: tauri::ipc::CallbackFn(1),
                url: "http://tauri.localhost".parse().unwrap(),
                body: tauri::ipc::InvokeBody::Json(args),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.into(),
            },
        )
        .map(|body| body.deserialize::<Value>().unwrap())
    };
    let root = work.join("native-root");
    let settings = tauri_plugin_store::resolve_store_path(app.handle(), "settings.json").unwrap();
    assert!(
        settings.starts_with(&work),
        "test must use private XDG path: {}",
        settings.display()
    );
    let persisted = work.join("original-caller-request.json");
    if phase == "prepare" {
        fs::create_dir(&root).unwrap();
        initialized(&root).await;
        fs::create_dir_all(settings.parent().unwrap()).unwrap();
        fs::write(
            &settings,
            serde_json::to_vec(&json!({"vault_path":root})).unwrap(),
        )
        .unwrap();
        let server: Value =
            serde_json::from_slice(&fs::read(work.join("server-ready.json")).unwrap()).unwrap();
        let context: Value =
            serde_json::from_slice(&fs::read(work.join("original-context.json")).unwrap()).unwrap();
        let destination = invoke(
            "capture_native_attachment_destination",
            json!({"provider":"none"}),
        )
        .unwrap();
        let request = json!({"operation_id":"eb8762d8-a47a-4030-871b-42c5a31dd3ce","destination":destination,"origin":context,"server_version_raw":server["raw_version"],"association":{"space_id":"local-space","document_id":"prior-page"}});
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&persisted)
            .unwrap();
        file.write_all(&serde_json::to_vec(&request).unwrap())
            .unwrap();
        file.sync_all().unwrap();
        assert_eq!(
            invoke(
                "prepare_native_attachment_download",
                json!({"request":request,"current":context})
            )
            .unwrap(),
            Value::Null
        );
        assert!(!work.join("http-body.bin").exists());
        println!("NATIVE_JOINT_ACTUAL_IPC_ORIGINAL_REQUEST_PERSISTED_BEFORE_HTTP_PASS");
        return;
    }
    let request: Value = serde_json::from_slice(&fs::read(&persisted).unwrap()).unwrap();
    let context = request["origin"].clone();
    if phase == "writer" {
        let body = fs::read(work.join("http-body.bin")).unwrap();
        assert!(invoke(
            "prepare_native_attachment_download",
            json!({"request":request,"current":context})
        )
        .unwrap()
        .is_null());
        for (index, part) in body.chunks(65536).enumerate() {
            invoke(
                "append_native_attachment_download",
                json!({"request":request,"current":context,"index":index,"bytes":part}),
            )
            .unwrap();
        }
        let ack = invoke(
            "finish_native_attachment_download",
            json!({"request":request,"current":context}),
        )
        .unwrap();
        assert_eq!(ack["state"], "current");
        assert_eq!(ack["original_ack"]["asset"]["space_id"], "local-space");
        assert_eq!(ack["original_ack"]["asset"]["document_id"], "prior-page");
        assert_eq!(
            ack["original_ack"]["asset"]["server_version_provenance"]["raw_version_record"],
            request["server_version_raw"]
        );
        use std::io::Write;
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(work.join("original-ack.json"))
            .unwrap();
        f.write_all(&serde_json::to_vec(&ack).unwrap()).unwrap();
        f.sync_all().unwrap();
        println!("NATIVE_JOINT_ACTUAL_IPC_COMMIT_THEN_EXIT77_BEFORE_ACK");
        std::process::exit(77);
    }
    if phase == "retry" {
        let ack: Value =
            serde_json::from_slice(&fs::read(work.join("original-ack.json")).unwrap()).unwrap();
        assert_eq!(
            invoke(
                "prepare_native_attachment_download",
                json!({"request":request,"current":context})
            )
            .unwrap(),
            ack
        );
        assert_eq!(
            invoke(
                "finish_native_attachment_download",
                json!({"request":request,"current":context})
            )
            .unwrap(),
            ack
        );
        println!("NATIVE_JOINT_ACTUAL_FRESH_IPC_EXACT_ORIGINAL_ACK_PASS");
        return;
    }
    if phase == "delete" {
        let ack: Value =
            serde_json::from_slice(&fs::read(work.join("original-ack.json")).unwrap()).unwrap();
        let path = PathBuf::from(ack["original_ack"]["asset"]["vault_path"].as_str().unwrap());
        let bytes = fs::read(&path).unwrap();
        let error = invoke("joint_ordinary_delete_guard", json!({"vaultPath":path})).unwrap_err();
        assert!(error.as_str().unwrap().contains("retention lifecycle"));
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(
            invoke(
                "prepare_native_attachment_download",
                json!({"request":request,"current":context})
            )
            .unwrap(),
            ack
        );
        println!("NATIVE_JOINT_ACTUAL_DELETE_GUARD_RETAINS_BYTES_ACK_PASS");
        return;
    }
    if phase == "context" {
        let mut changed = context;
        changed["sessionId"] = json!("different-actual-input-session");
        let error = invoke(
            "prepare_native_attachment_download",
            json!({"request":request,"current":changed}),
        )
        .unwrap_err();
        assert!(error.as_str().unwrap().contains("context changed"));
        println!("NATIVE_JOINT_IPC_CHANGED_CONTEXT_REFUSAL_PASS");
        return;
    }
    if phase == "destination" {
        let other = work.join("other-native-root");
        fs::create_dir(&other).unwrap();
        initialized(&other).await;
        fs::write(
            &settings,
            serde_json::to_vec(&json!({"vault_path":other})).unwrap(),
        )
        .unwrap();
        let prior = fs::read(other.join(".starlane/starlane-notes.sqlite3")).unwrap();
        assert!(invoke(
            "prepare_native_attachment_download",
            json!({"request":request,"current":context})
        )
        .is_err());
        assert_eq!(
            fs::read(other.join(".starlane/starlane-notes.sqlite3")).unwrap(),
            prior
        );
        assert!(!other.join(".starlane/attachment-downloads").exists());
        fs::write(
            &settings,
            serde_json::to_vec(&json!({"vault_path":root})).unwrap(),
        )
        .unwrap();
        println!("NATIVE_JOINT_IPC_ORIGINAL_DESTINATION_REFUSAL_NO_OTHER_WRITES_PASS");
        return;
    }
    panic!("unknown explicit joint phase");
}

async fn persist_actual_wal_sidecars(root: &Path) -> PathBuf {
    initialized(root).await;
    let path = root.join(".starlane/starlane-notes.sqlite3");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .journal_mode(SqliteJournalMode::Wal),
        )
        .await
        .unwrap();
    let mut connection = pool.acquire().await.unwrap();
    {
        let mut handle = connection.lock_handle().await.unwrap();
        let mut flag: i32 = 1;
        assert_eq!(
            unsafe {
                libsqlite3_sys::sqlite3_file_control(
                    handle.as_raw_handle().as_ptr(),
                    c"main".as_ptr(),
                    libsqlite3_sys::SQLITE_FCNTL_PERSIST_WAL,
                    &mut flag as *mut _ as *mut libc::c_void,
                )
            },
            libsqlite3_sys::SQLITE_OK
        );
    }
    sqlx::query(
        "INSERT INTO local_vault_metadata(key,value) VALUES('original-wal-proof','original')",
    )
    .execute(&mut *connection)
    .await
    .unwrap();
    drop(connection);
    pool.close().await;
    assert_eq!(&fs::read(&path).unwrap()[18..20], &[2, 2]);
    assert!(path.with_file_name("starlane-notes.sqlite3-shm").exists());
    assert!(path.with_file_name("starlane-notes.sqlite3-wal").exists());
    path
}
#[tokio::test]
async fn native_import_existing_wal_original_shm_is_retained_without_shared_path_access() {
    let root = fixture("retained-wal-shm");
    let path = persist_actual_wal_sidecars(&root).await;
    // Actual shipped native options preserve and read this WAL configuration.
    let ordinary = open_database(&path).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT value FROM local_vault_metadata WHERE key='original-wal-proof'"
        )
        .fetch_one(&ordinary)
        .await
        .unwrap(),
        "original"
    );
    ordinary.close().await;
    // The preceding ordinary connection may clean default sidecars; retain the
    // genuine persist-WAL producer policy again on its actual existing DB.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&path))
        .await
        .unwrap();
    let mut conn = pool.acquire().await.unwrap();
    {
        let mut handle = conn.lock_handle().await.unwrap();
        let mut flag: i32 = 1;
        assert_eq!(
            unsafe {
                libsqlite3_sys::sqlite3_file_control(
                    handle.as_raw_handle().as_ptr(),
                    c"main".as_ptr(),
                    libsqlite3_sys::SQLITE_FCNTL_PERSIST_WAL,
                    &mut flag as *mut _ as *mut libc::c_void,
                )
            },
            libsqlite3_sys::SQLITE_OK
        );
    }
    sqlx::query("UPDATE local_vault_metadata SET value='original' WHERE key='original-wal-proof'")
        .execute(&mut *conn)
        .await
        .unwrap();
    drop(conn);
    pool.close().await;
    let shm = path.with_file_name("starlane-notes.sqlite3-shm");
    let before = fs::read(&shm).unwrap();
    let inode = fs::metadata(&shm).unwrap().ino();
    let bytes = b"real existing WAL root native version";
    let request = import_request(&root, bytes);
    let owner = capture(&root, request.clone());
    assert!(owner
        .prepare_fetch(&request.origin)
        .await
        .unwrap()
        .is_none());
    owner.begin(&request.origin).await.unwrap();
    owner
        .append(&request.origin, 0, bytes.to_vec())
        .await
        .unwrap();
    let ack = owner.finish(&request.origin).await.unwrap();
    assert_eq!(ack["state"], "current");
    assert_eq!(&fs::read(&path).unwrap()[18..20], &[2, 2]);
    assert_eq!(fs::read(&shm).unwrap(), before);
    assert_eq!(fs::metadata(&shm).unwrap().ino(), inode);
    assert_eq!(
        owner.prepare_fetch(&request.origin).await.unwrap(),
        Some(ack)
    );
    assert_eq!(fs::read(&shm).unwrap(), before);
    fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn native_wal_exclusive_contender_child() {
    let Ok(path) = std::env::var("STARLANE_NATIVE_WAL_CONTENDER_DATABASE") else {
        return;
    };
    let path = PathBuf::from(path);
    assert!(path.is_absolute());
    let mode = std::env::var("STARLANE_NATIVE_WAL_CONTENDER_PHASE").unwrap();
    let pool = open_database(&path).await.unwrap();
    let result = sqlx::query_scalar::<_, String>(
        "SELECT value FROM local_vault_metadata WHERE key='original-wal-proof'",
    )
    .fetch_one(&pool)
    .await;
    if mode == "busy" {
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("locked") || error.contains("busy"),
            "{error}"
        );
        println!("ACTUAL_FRESH_WAL_NATIVE_CONTENDER_BUSY_PASS");
    } else {
        assert_eq!(result.unwrap(), "original");
        println!("ACTUAL_FRESH_WAL_NATIVE_REOPEN_ORIGINAL_DATA_PASS");
    }
    pool.close().await;
}
#[tokio::test]
async fn native_wal_exclusive_owner_blocks_actual_fresh_contender_then_preserves_reopen() {
    let root = fixture("wal-contender");
    let path = persist_actual_wal_sidecars(&root).await;
    let directory = filesystem::Directory::capture(&root.join(".starlane")).unwrap();
    let mut owner = connection::Database::prepare(directory, "starlane-notes.sqlite3")
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("PRAGMA locking_mode")
            .fetch_one(owner.connection())
            .await
            .unwrap(),
        "exclusive"
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT value FROM local_vault_metadata WHERE key='original-wal-proof'"
        )
        .fetch_one(owner.connection())
        .await
        .unwrap(),
        "original"
    );
    sqlx::query("UPDATE local_vault_metadata SET value='original' WHERE key='original-wal-proof'")
        .execute(owner.connection())
        .await
        .unwrap();
    let child = |phase: &str| {
        std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","local_database::attachment_download_owner::tests::native_wal_exclusive_contender_child","--nocapture"]).env("STARLANE_NATIVE_WAL_CONTENDER_DATABASE",&path).env("STARLANE_NATIVE_WAL_CONTENDER_PHASE",phase).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().unwrap()
    };
    let child = |phase: &str| {
        let mut process = child(phase);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while process.try_wait().unwrap().is_none() {
            if std::time::Instant::now() > deadline {
                process.kill().unwrap();
                process.wait().unwrap();
                panic!("Owned WAL contender timed out");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        process.wait_with_output().unwrap()
    };
    let blocked = child("busy");
    assert_eq!(
        blocked.status.code(),
        Some(0),
        "{} {}",
        String::from_utf8_lossy(&blocked.stdout),
        String::from_utf8_lossy(&blocked.stderr)
    );
    assert!(String::from_utf8_lossy(&blocked.stdout)
        .contains("ACTUAL_FRESH_WAL_NATIVE_CONTENDER_BUSY_PASS"));
    owner.retire().await.unwrap();
    let reopened = child("read");
    assert_eq!(
        reopened.status.code(),
        Some(0),
        "{} {}",
        String::from_utf8_lossy(&reopened.stdout),
        String::from_utf8_lossy(&reopened.stderr)
    );
    assert!(String::from_utf8_lossy(&reopened.stdout)
        .contains("ACTUAL_FRESH_WAL_NATIVE_REOPEN_ORIGINAL_DATA_PASS"));
    assert_eq!(&fs::read(&path).unwrap()[18..20], &[2, 2]);
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_reset_retains_pending_partial_without_sql_receipt() {
    let root = fixture("reset-partial");
    initialized(&root).await;
    let path = root.join(".starlane/starlane-notes.sqlite3");
    joint_archive::reset_allowed(&path).await.unwrap();
    let request = import_request(&root, b"actual retained partial");
    let owner = capture(&root, request.clone());
    owner.begin(&request.origin).await.unwrap();
    owner
        .append(&request.origin, 0, b"actual retained partial".to_vec())
        .await
        .unwrap();
    let before = fs::read(&path).unwrap();
    let error = joint_archive::reset_allowed(&path).await.unwrap_err();
    assert!(error.contains("pending download"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(
        fs::read(
            root.join(".starlane/attachment-downloads")
                .join(&request.operation_id)
                .join("part-0.bin")
        )
        .unwrap(),
        b"actual retained partial"
    );
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_reset_retains_committed_receipt_even_without_payload_directory() {
    let root = fixture("reset-receipt");
    initialized(&root).await;
    let request = import_request(&root, b"");
    let owner = capture(&root, request.clone());
    owner.begin(&request.origin).await.unwrap();
    let original = owner.finish(&request.origin).await.unwrap();
    // Simulate missing external payload evidence; resetting SQL would lose the
    // only remaining historical acknowledgement and cannot repair the payload.
    fs::remove_dir_all(root.join(".starlane/attachment-downloads")).unwrap();
    let path = root.join(".starlane/starlane-notes.sqlite3");
    let error = joint_archive::reset_allowed(&path).await.unwrap_err();
    assert!(error.contains("receipts"), "{error}");
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&path)
                .create_if_missing(false),
        )
        .await
        .unwrap();
    let raw: String = sqlx::query_scalar(
        "SELECT receipt FROM native_attachment_download_receipts WHERE operation_id=?1",
    )
    .bind(&request.operation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&raw).unwrap(),
        original["original_ack"]
    );
    pool.close().await;
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn physical_connection_pragma_error_waits_for_actual_main_close() {
    use std::sync::{mpsc, Arc, Condvar, Mutex};
    let root = fixture("pragma-close");
    let directory = database(&root);
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let release = gate.clone();
    let (entered_send, entered) = mpsc::channel();
    let (done_send, done) = tokio::sync::oneshot::channel();
    let caller = tokio::spawn(async move {
        connection::supervise(async move {
            let _snapshot = snapshot_commit_lock().lock().await;
            let result = connection::Database::prepare_invalid_pragma(
                directory,
                "native.sqlite3",
                Box::new(move |name| {
                    let c = std::ffi::CString::new(name).unwrap();
                    assert!(!unsafe { libsqlite3_sys::sqlite3_vfs_find(c.as_ptr()) }.is_null());
                    entered_send.send(name.to_owned()).unwrap();
                    let (lock, condition) = &*release;
                    let mut ready = lock.lock().unwrap();
                    while !*ready {
                        ready = condition.wait(ready).unwrap();
                    }
                }),
            )
            .await;
            assert!(result.is_err());
            drop(_snapshot);
            let _ = done_send.send(());
            Ok(())
        })
        .await
    });
    let registered_name = entered
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert!(snapshot_commit_lock().try_lock().is_err());
    {
        let (lock, condition) = &*gate;
        *lock.lock().unwrap() = true;
        condition.notify_all();
    }
    tokio::time::timeout(std::time::Duration::from_secs(10), done)
        .await
        .unwrap()
        .unwrap();
    let c = std::ffi::CString::new(registered_name).unwrap();
    assert!(unsafe { libsqlite3_sys::sqlite3_vfs_find(c.as_ptr()) }.is_null());
    println!("ACTUAL_PRAGMA_ERROR_FINAL_MAIN_CLOSE_AND_MUTEX_RETENTION_PASS");
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn native_delivery_snapshot_requires_real_committed_receipt_and_current_page() {
    let root=fixture("delivery-receipt");initialized(&root).await;
    let bytes=vec![41;150001];let request=import_request(&root,&bytes);let owner=capture(&root,request.clone());
    assert!(owner.delivery_snapshot(&request.origin).await.is_err());
    owner.begin(&request.origin).await.unwrap();
    for (i,b) in bytes.chunks(65536).enumerate(){owner.append(&request.origin,i as u64,b.to_vec()).await.unwrap();}
    owner.finish(&request.origin).await.unwrap();
    let snapshot=owner.delivery_snapshot(&request.origin).await.unwrap();
    assert_eq!(snapshot.read(65536,65536).unwrap(),bytes[65536..131072]);
    assert!(snapshot.read(0,65537).is_err());
    // Genuine current native page removal, retained immutable local receipt.
    let pool=SqlitePoolOptions::new().max_connections(1).connect_with(SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3"))).await.unwrap();
    let mut canonical=read_snapshot_from_pool(&pool,"none").await.unwrap().unwrap();canonical["documents"]=json!([]);let mut tx=pool.begin().await.unwrap();write_snapshot_in_tx(&mut tx,"none",&canonical).await.unwrap();tx.commit().await.unwrap();pool.close().await;
    assert!(owner.delivery_snapshot(&request.origin).await.is_err());
    assert_eq!(snapshot.read(0,17).unwrap(),bytes[..17]);
    fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn native_delivery_refuses_changed_bytes_and_original_destination() {
    let root=fixture("delivery-change");initialized(&root).await;
    let bytes=b"original version";let request=import_request(&root,bytes);let owner=capture(&root,request.clone());
    owner.begin(&request.origin).await.unwrap();owner.append(&request.origin,0,bytes.to_vec()).await.unwrap();owner.finish(&request.origin).await.unwrap();
    let changed=fixture("delivery-other");initialized(&changed).await;
    let mut other=request.clone();other.destination=custody::Destination::capture(&vault::SnapshotRequestBinding::capture_at(changed.with_extension("settings.json"),changed.clone(),"none").unwrap()).unwrap();
    let other_owner=capture(&changed,other);assert!(other_owner.delivery_snapshot(&request.origin).await.is_err());
    fs::write(root.join(".starlane/attachment-downloads").join(&request.operation_id).join("asset.bin"),b"mutated version").unwrap();
    assert!(owner.delivery_snapshot(&request.origin).await.is_err());
    fs::remove_dir_all(root).unwrap();fs::remove_dir_all(changed).unwrap();
}

#[tokio::test]
async fn native_delivery_post_dialog_revalidates_removed_page_or_asset_before_publication() {
 for removed in ["documents","assets"] {
  let root=fixture(&format!("delivery-dialog-{removed}"));initialized(&root).await;
  let request=import_request(&root,b"selected bytes");let owner=capture(&root,request.clone());
  owner.begin(&request.origin).await.unwrap();owner.append(&request.origin,0,b"selected bytes".to_vec()).await.unwrap();owner.finish(&request.origin).await.unwrap();
  assert_eq!(owner.prepare_fetch(&request.origin).await.unwrap().unwrap()["state"],"current");
  // Deterministic dialog-delay seam: actual canonical writer runs between initial
  // eligibility and supervised post-dialog delivery. No sleeps or fabricated rows.
  let pool=SqlitePoolOptions::new().max_connections(1).connect_with(SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3"))).await.unwrap();
  let mut canonical=read_snapshot_from_pool(&pool,"none").await.unwrap().unwrap();canonical[removed]=json!([]);
  let mut tx=pool.begin().await.unwrap();write_snapshot_in_tx(&mut tx,"none",&canonical).await.unwrap();tx.commit().await.unwrap();pool.close().await;
  let target=root.join("selected-export.bin");let selected=target.clone();
  let result=owner.delivery_operation(&request.origin,move |_,snapshot| {
    snapshot.export_no_clobber(&std::fs::File::open(selected.parent().unwrap()).unwrap(),"selected-export.bin").map_err(|e|e.to_string())
  }).await;
  assert!(result.is_err());assert!(!target.exists());fs::remove_dir_all(root).unwrap();
 }
}

#[tokio::test]
async fn native_delivery_actual_owner_exports_advertised_100mib_without_whole_file_buffer() {
    use sha2::{Digest,Sha256};
    let root=fixture("delivery-full-size");initialized(&root).await;
    let block=vec![53u8;65536];let length=100*1024*1024;
    let mut hash=Sha256::new();for _ in 0..1600 {hash.update(&block);}let expected=format!("{:x}",hash.finalize());
    // Native-layer typed provenance fixture, not remote authorization evidence.
    // Intent, parts, install proof, current metadata and SQL receipt are produced
    // by actual native owner APIs; no install/receipt rows are fabricated.
    let mut request=import_request(&root,&block);let mut version:Value=serde_json::from_str(&request.server_version_raw).unwrap();version["byte_length"]=json!(length);version["binary_sha256"]=json!(expected);request.server_version_raw=serde_json::to_string(&version).unwrap();
    let owner=capture(&root,request.clone());owner.begin(&request.origin).await.unwrap();
    for index in 0..1600 {owner.append(&request.origin,index,block.clone()).await.unwrap();}
    owner.finish(&request.origin).await.unwrap();
    let parent=std::fs::File::open(&root).unwrap();
    let copied=owner.delivery_operation(&request.origin,move |_,snapshot|snapshot.export_no_clobber(&parent,"full-original.bin").map_err(|e|e.to_string())).await.unwrap();
    assert_eq!(copied,length);
    let file=std::fs::File::open(root.join("full-original.bin")).unwrap();
    use std::os::unix::fs::FileExt;
    let mut hash=Sha256::new();let mut offset=0;let mut buffer=[0u8;65536];
    loop {let n=file.read_at(&mut buffer,offset).unwrap();if n==0{break;}hash.update(&buffer[..n]);offset+=n as u64;}
    assert_eq!(offset,length);assert_eq!(format!("{:x}",hash.finalize()),expected);
    fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn native_delivery_opaque_destination_is_bound_to_exact_original_request_and_once_only() {
 let root=fixture("delivery-token");initialized(&root).await;let request=import_request(&root,b"selected");let owner=capture(&root,request.clone());
 let token=delivery::test_remember(&owner,root.join("selected.bin")).unwrap();assert_eq!(token.len(),64);
 let mut other=request.clone();other.operation_id="9d1be85e-9b16-4a18-a572-6d2d7c34a9a6".into();let other_owner=capture(&root,other);
 assert!(delivery::test_take(&other_owner,&token).is_err());assert!(!root.join("selected.bin").exists());
 delivery::test_take(&owner,&token).unwrap();assert!(delivery::test_take(&owner,&token).is_err());
 let token=delivery::test_remember(&owner,root.join("selected.bin")).unwrap();delivery::discard_native_attachment_export_selection(token.clone()).unwrap();assert!(delivery::test_take(&owner,&token).is_err());
 fs::remove_dir_all(root).unwrap();
}
