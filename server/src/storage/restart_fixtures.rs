//! Real writer loss and fresh-process startup checks; no fabricated receipt rows.
//! Deliberate exit(77) and public recovery calls are process-restart evidence,
//! not physical power-loss, server-listener or full application restore certification.
use super::{canonical_rewrite as rewrite, journal, publication};
use serde::{Deserialize, Serialize};
use sqlx::AnyPool;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
const OLD: &[u8] = b"PUBLIC OLD";
const NEXT: &[u8] = b"PRIVATE NEXT";
const SOURCE: &str = "spaces/a/docs/old.md";

#[derive(Serialize, Deserialize)]
struct Evidence {
    kind: String,
    phase: String,
    op_id: String,
    path: String,
    manifest: serde_json::Value,
}
fn evidence(root: &Path) -> Evidence {
    serde_json::from_slice(&fs::read(root.join("fixture-evidence.json")).unwrap()).unwrap()
}
fn save(root: &Path, e: &Evidence) {
    fs::write(
        root.join("fixture-evidence.json"),
        serde_json::to_vec(e).unwrap(),
    )
    .unwrap();
}
async fn pool(root: &Path) -> AnyPool {
    sqlx::any::install_default_drivers();
    sqlx::any::AnyPoolOptions::new()
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
        .connect(&format!(
            "sqlite://{}?mode=rwc",
            root.join("database.sqlite").display()
        ))
        .await
        .unwrap()
}
async fn seed(db: &AnyPool, root: &Path, kind: &str) {
    sqlx::migrate!("./migrations").run(db).await.unwrap();
    for q in [
        "INSERT INTO users(id,email,display_name,role) VALUES('owner-a','restart@test','Owner','member')",
        "INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('restart-owner','owner-a','test','2999-01-01T00:00:00Z')",
        "INSERT INTO spaces(id,name,slug,created_by) VALUES('space-a','A','a','owner-a')",
        "INSERT INTO space_members(id,space_id,user_id,role) VALUES('owner','space-a','owner-a','owner')",
    ] { sqlx::query(q).execute(db).await.unwrap(); }
    if kind != "creation" {
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at,published_at) VALUES('target-a','space-a','Old','spaces/a/docs/old.md','owner-a','owner-a','source','published')").execute(db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('restart-draft','target-a','space-a','owner-a','New','PRIVATE NEXT','source')").execute(db).await.unwrap();
        fs::create_dir_all(root.join("spaces/a/docs")).unwrap();
        fs::write(root.join(SOURCE), OLD).unwrap();
    }
}
async fn recover(db: &AnyPool) {
    // Production startup order, before listeners/search migrations.
    super::relocation::recover(db).await.unwrap();
    journal::recover(db).await.unwrap();
    rewrite::recover(db).await.unwrap();
    publication::recover(db).await.unwrap();
}

#[tokio::test]
async fn restart_writer_child() {
    let Ok(root) = std::env::var("STARLANE_RESTART_FIXTURE_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    assert_eq!(super::data_dir(), root);
    let kind = std::env::var("STARLANE_RESTART_FIXTURE_KIND").unwrap();
    let phase = std::env::var("STARLANE_RESTART_FIXTURE_PHASE").unwrap();
    let db = pool(&root).await;
    seed(&db, &root, &kind).await;
    let mut auth = crate::spaces::documents::tests::test_auth("owner-a");
    auth.claims.sid = "restart-owner".into();
    let mut tx = db.begin().await.unwrap();
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &["space-a"])
        .await
        .unwrap();
    let e = match kind.as_str() {
        "creation" => {
            // Save authentic stage identity before deliberately exiting the real API.
            if phase == "prepared" {
                let root = root.clone();
                let kind = kind.clone();
                let phase = phase.clone();
                journal::test_stage_callback(move |m| {
                    save(
                        &root,
                        &Evidence {
                            kind,
                            phase,
                            op_id: m.op_id.clone(),
                            path: m.path.clone(),
                            manifest: serde_json::to_value(m).unwrap(),
                        },
                    );
                    std::process::exit(77);
                });
            }
            let m = journal::stage_document(
                &mut tx,
                "space-a",
                "target-a",
                "restart-draft",
                "new",
                "a",
                "New",
                "",
                "PRIVATE NEXT",
            )
            .await
            .unwrap();
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at) VALUES('target-a','space-a','New',$1,'owner-a','owner-a','new')").bind(&m.path).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('restart-draft','target-a','space-a','owner-a','New','PRIVATE NEXT','new')").execute(&mut *tx).await.unwrap();
            Evidence {
                kind: kind.clone(),
                phase: phase.clone(),
                op_id: m.op_id.clone(),
                path: m.path.clone(),
                manifest: serde_json::to_value(m).unwrap(),
            }
        }
        "publication" => {
            let m = publication::prepare_publication(
                &mut tx,
                &auth,
                publication::PublicationIntent {
                    op_id: uuid::Uuid::new_v4().to_string(),
                    space_id: "space-a".into(),
                    document_id: "target-a".into(),
                    draft_id: "restart-draft".into(),
                    source_path: SOURCE.into(),
                    source_revision: "source".into(),
                    revision: "new".into(),
                    payload_fingerprint: hex::encode(sha2::Sha256::digest(b"restart publish")),
                },
                "",
                "PRIVATE NEXT",
            )
            .await
            .unwrap();
            let e = Evidence {
                kind: kind.clone(),
                phase: phase.clone(),
                op_id: m.op_id().into(),
                path: m.path().into(),
                manifest: serde_json::to_value(&m).unwrap(),
            };
            save(&root, &e);
            if phase == "prepared" {
                std::process::exit(77);
            }
            sqlx::query("UPDATE documents SET content_path=$1,updated_at='new',published_at='new',last_updated_by='owner-a' WHERE id='target-a'").bind(m.path()).execute(&mut *tx).await.unwrap();
            publication::record_publication(&mut tx, &auth, &m)
                .await
                .unwrap();
            e
        }
        "rewrite" => {
            let m = rewrite::prepare_rewrite(
                &mut tx,
                &auth,
                rewrite::RewriteIntent {
                    space_id: "space-a".into(),
                    document_id: "target-a".into(),
                    source_path: SOURCE.into(),
                    source_revision: "source".into(),
                    published_stamp: "published".into(),
                    revision: "new".into(),
                    source_sha256: None,
                    reason: rewrite::RewriteReason::ReplayContent,
                    incoming: rewrite::IncomingIdentity {
                        provider: "server".into(),
                        operation_id: uuid::Uuid::new_v4().to_string(),
                        entity_type: "document".into(),
                        entity_id: "target-a".into(),
                        action: "update_content".into(),
                        payload_fingerprint: hex::encode(sha2::Sha256::digest(b"restart rewrite")),
                    },
                },
                NEXT,
            )
            .await
            .unwrap();
            let e = Evidence {
                kind: kind.clone(),
                phase: phase.clone(),
                op_id: m.op_id().into(),
                path: m.path().into(),
                manifest: serde_json::to_value(&m).unwrap(),
            };
            save(&root, &e);
            if phase == "prepared" {
                std::process::exit(77);
            }
            sqlx::query("UPDATE documents SET content_path=$1,updated_at='new',last_updated_by='owner-a' WHERE id='target-a'").bind(m.path()).execute(&mut *tx).await.unwrap();
            rewrite::record_rewrite(&mut tx, &auth, &m).await.unwrap();
            e
        }
        _ => panic!("unknown fixture kind"),
    };
    save(&root, &e);
    if phase == "sql-uncommitted" {
        std::process::exit(77);
    }
    tx.commit().await.unwrap();
    if kind == "creation" && phase == "installed-pending" {
        // Exit from the real public publication path after no-replace install,
        // before its receipt completion transaction commits.
        super::journal_fs::final_hook(|| std::process::exit(77));
        let manifest: journal::StagedDocument = serde_json::from_value(e.manifest.clone()).unwrap();
        assert!(journal::publish_committed(&db, &manifest).await);
        panic!("installed-pending crash hook did not run");
    }
    if phase == "completed" {
        recover(&db).await;
    }
    std::process::exit(77);
}
use sha2::Digest;

async fn inspect(db: &AnyPool, e: &Evidence, copied: bool) -> serde_json::Value {
    let committed = matches!(
        e.phase.as_str(),
        "committed" | "completed" | "installed-pending"
    );
    let path: Option<String> =
        sqlx::query_scalar("SELECT content_path FROM documents WHERE id='target-a'")
            .fetch_optional(db)
            .await
            .unwrap();
    assert_eq!(
        path.as_deref(),
        if committed {
            Some(e.path.as_str())
        } else if e.kind == "creation" {
            None
        } else {
            Some(SOURCE)
        }
    );
    let root = super::data_dir();
    if e.kind != "creation" {
        assert_eq!(fs::read(root.join(SOURCE)).unwrap(), OLD);
    }
    if committed {
        assert_eq!(fs::read(root.join(&e.path)).unwrap(), NEXT);
    }
    let (table, marker) = match e.kind.as_str() {
        "creation" => ("storage_journal_receipts", "completed_at"),
        "publication" => ("publication_receipts", "verified_at"),
        "rewrite" => ("canonical_revision_receipts", "verified_at"),
        _ => unreachable!(),
    };
    let rows: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE op_id=$1"))
        .bind(&e.op_id)
        .fetch_one(db)
        .await
        .unwrap();
    assert_eq!(rows, i64::from(committed));
    let verified: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM {table} WHERE op_id=$1 AND {marker} IS NOT NULL"
    ))
    .bind(&e.op_id)
    .fetch_one(db)
    .await
    .unwrap();
    assert_eq!(verified, i64::from(committed && !copied));
    if e.kind == "creation" && committed {
        let state: String =
            sqlx::query_scalar("SELECT state FROM storage_journal_receipts WHERE op_id=$1")
                .bind(&e.op_id)
                .fetch_one(db)
                .await
                .unwrap();
        assert_eq!(state, "complete");
    }
    serde_json::json!({"writer_op_id":e.op_id,"kind":e.kind,"phase":e.phase,"receipt_rows":rows,"verified_rows":verified,"canonical_path":path,"copied":copied})
}
#[tokio::test]
async fn restart_recovery_child() {
    let Ok(root) = std::env::var("STARLANE_RESTART_FIXTURE_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    assert_eq!(super::data_dir(), root);
    let copied = std::env::var_os("STARLANE_RESTART_FIXTURE_COPY").is_some();
    let e = evidence(&root);
    let db = pool(&root).await;
    recover(&db).await;
    let result = inspect(&db, &e, copied).await;
    // Repeated startup is idempotent and does not manufacture new receipts.
    recover(&db).await;
    assert_eq!(inspect(&db, &e, copied).await, result);
    db.close().await;
    fs::write(
        root.join("restart-result.json"),
        serde_json::to_vec(&result).unwrap(),
    )
    .unwrap();
}
fn child(root: &Path, name: &str, kind: &str, phase: &str, copied: bool) -> i32 {
    let log = root.join(format!("{name}.log"));
    let file = fs::File::create(log).unwrap();
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args([
        "--exact",
        &format!("storage::restart_fixtures::{name}"),
        "--nocapture",
    ])
    .env("DATA_DIR", root)
    .env("STARLANE_RESTART_FIXTURE_ROOT", root)
    .env("STARLANE_RESTART_FIXTURE_KIND", kind)
    .env("STARLANE_RESTART_FIXTURE_PHASE", phase)
    .stdout(Stdio::from(file.try_clone().unwrap()))
    .stderr(Stdio::from(file));
    if copied {
        cmd.env("STARLANE_RESTART_FIXTURE_COPY", "1");
    } else {
        cmd.env_remove("STARLANE_RESTART_FIXTURE_COPY");
    }
    let mut p = cmd.spawn().unwrap();
    let until = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(s) = p.try_wait().unwrap() {
            return s.code().unwrap_or(-1);
        }
        if Instant::now() >= until {
            p.kill().unwrap();
            p.wait().unwrap();
            panic!(
                "child timed out: {kind}/{phase}/{name}; retained {}",
                root.display()
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn root() -> PathBuf {
    let p = std::env::temp_dir().join(format!("starlane-real-restart-{}", uuid::Uuid::new_v4()));
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(&p).unwrap();
    p
}
fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for e in fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let t = e.file_type().unwrap();
        assert!(!t.is_symlink());
        if t.is_dir() {
            copy_tree(&e.path(), &to.join(e.file_name()));
        } else {
            fs::copy(e.path(), to.join(e.file_name())).unwrap();
        }
    }
}
fn inventory(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, p: &Path, out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>) {
        for e in fs::read_dir(p).unwrap() {
            let e = e.unwrap();
            if e.file_type().unwrap().is_dir() {
                visit(root, &e.path(), out);
            } else {
                out.insert(
                    e.path().strip_prefix(root).unwrap().into(),
                    fs::read(e.path()).unwrap(),
                );
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    visit(root, root, &mut out);
    out
}
// The orchestrating process independently reads persisted bytes and SQL after
// the fresh recovery child exits; its evidence is not only the child's report.
fn parent_verify(root: &Path, e: &Evidence, copied: bool) {
    let committed = matches!(
        e.phase.as_str(),
        "committed" | "completed" | "installed-pending"
    );
    if e.kind != "creation" {
        assert_eq!(fs::read(root.join(SOURCE)).unwrap(), OLD);
    }
    if committed {
        assert_eq!(fs::read(root.join(&e.path)).unwrap(), NEXT);
    }
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        let db = pool(root).await;
        let (table, marker) = match e.kind.as_str() {
            "creation" => ("storage_journal_receipts", "completed_at"),
            "publication" => ("publication_receipts", "verified_at"),
            "rewrite" => ("canonical_revision_receipts", "verified_at"),
            _ => unreachable!(),
        };
        let rows: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE op_id=$1"))
            .bind(&e.op_id).fetch_one(&db).await.unwrap();
        let verified: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE op_id=$1 AND {marker} IS NOT NULL"))
            .bind(&e.op_id).fetch_one(&db).await.unwrap();
        assert_eq!(rows, i64::from(committed));
        assert_eq!(verified, i64::from(committed && !copied));
        let path: Option<String> = sqlx::query_scalar("SELECT content_path FROM documents WHERE id='target-a'")
            .fetch_optional(&db).await.unwrap();
        assert_eq!(path.as_deref(), if committed {Some(e.path.as_str())} else if e.kind=="creation" {None} else {Some(SOURCE)});
        db.close().await;
        println!("restart evidence kind={} phase={} op_id={} copied={} receipt_rows={} verified_rows={}", e.kind,e.phase,e.op_id,copied,rows,verified);
    });
}
#[test]
fn actual_fresh_process_recovery_for_pending_and_completed_public_api_operations() {
    for kind in ["creation", "publication", "rewrite"] {
        for phase in ["prepared", "sql-uncommitted", "committed", "completed"] {
            let r = root();
            assert_eq!(
                child(&r, "restart_writer_child", kind, phase, false),
                77,
                "writer {kind}/{phase} at {}",
                r.display()
            );
            let e = evidence(&r);
            assert_eq!(e.kind, kind);
            assert_eq!(e.phase, phase);
            assert!(uuid::Uuid::parse_str(&e.op_id).is_ok());
            assert_eq!(
                child(&r, "restart_recovery_child", kind, phase, false),
                0,
                "restart {kind}/{phase} at {}",
                r.display()
            );
            let result: serde_json::Value =
                serde_json::from_slice(&fs::read(r.join("restart-result.json")).unwrap()).unwrap();
            assert_eq!(result["writer_op_id"], e.op_id);
            parent_verify(&r, &e, false);
            fs::remove_dir_all(r).unwrap();
        }
    }
}
#[test]
fn actual_restart_refuses_independently_copied_pending_install_proofs_and_preserves_source() {
    // Logical receipt matching does not imply copied physical custody. Restrict
    // this test to committed unverified immutable installs; completed creation
    // intentionally skips physical replay and is not covered by this claim.
    for kind in ["publication", "rewrite"] {
        let source = root();
        assert_eq!(
            child(&source, "restart_writer_child", kind, "committed", false),
            77
        );
        let before = inventory(&source);
        let destination = root();
        copy_tree(&source, &destination);
        assert_eq!(
            child(
                &destination,
                "restart_recovery_child",
                kind,
                "committed",
                true
            ),
            0,
            "copied restart {kind} at {}",
            destination.display()
        );
        assert_eq!(
            inventory(&source),
            before,
            "copy/restart changed original evidence"
        );
        let report: serde_json::Value =
            serde_json::from_slice(&fs::read(destination.join("restart-result.json")).unwrap())
                .unwrap();
        assert_eq!(report["receipt_rows"], 1);
        assert_eq!(report["verified_rows"], 0);
        parent_verify(&destination, &evidence(&destination), true);
        fs::remove_dir_all(source).unwrap();
        fs::remove_dir_all(destination).unwrap();
    }
}

// Destination-adoption contract fixtures intentionally construct ledger and SQL
// generation commitment from real journal operations. They are not a production
// backup/restoration adapter, authentication route or full application certification.
fn operation_namespace(kind: &str) -> &'static str {
    match kind {
        "creation" => ".storage-journal",
        "publication" => ".publication-journal",
        "rewrite" => ".canonical-rewrite-journal",
        _ => unreachable!(),
    }
}
fn descriptor(e: &Evidence) -> super::journal_fs::FileDescriptor {
    // All three authentic immutable envelopes serialize their descriptor fields
    // at the top level; deserialization retains the current descriptor checks.
    serde_json::from_value(e.manifest.clone()).unwrap()
}
fn copy_tree_preserving_hardlinks(from: &Path, to: &Path) {
    use std::os::unix::fs::MetadataExt;
    fn copy(from: &Path, to: &Path, links: &mut std::collections::HashMap<(u64, u64), PathBuf>) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let metadata = entry.metadata().unwrap();
            assert!(!entry.file_type().unwrap().is_symlink());
            let destination = to.join(entry.file_name());
            if metadata.is_dir() {
                copy(&entry.path(), &destination, links);
            } else {
                assert!(metadata.is_file());
                let identity = (metadata.dev(), metadata.ino());
                if let Some(first) = links.get(&identity) {
                    fs::hard_link(first, &destination).unwrap();
                } else {
                    fs::copy(entry.path(), &destination).unwrap();
                    links.insert(identity, destination);
                }
            }
        }
    }
    copy(from, to, &mut std::collections::HashMap::new());
}
async fn commit_adoption_ledger(
    root: &Path,
    db: &AnyPool,
    e: &Evidence,
    scenario: &str,
) -> Vec<u8> {
    use super::restoration::{Binding, Ledger, Scope};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let ns = operation_namespace(&e.kind);
    let stage = format!("{ns}/{}", e.op_id);
    let proof_bytes = fs::read(root.join(&stage).join("install-proof.json")).unwrap();
    let proof: serde_json::Value = serde_json::from_slice(&proof_bytes).unwrap();
    assert_eq!(proof["op_id"], e.op_id);
    let original = (
        proof["dev"].as_u64().unwrap(),
        proof["ino"].as_u64().unwrap(),
    );
    let paths = [
        format!("{stage}/{}", proof["temp"].as_str().unwrap()),
        format!("{stage}/install-anchor"),
        e.path.clone(),
    ];
    let bindings = paths
        .into_iter()
        .map(|relative| {
            let md = fs::metadata(root.join(&relative)).unwrap();
            let bytes = fs::read(root.join(&relative)).unwrap();
            assert_eq!(bytes, NEXT);
            Binding {
                relative,
                original_device: original.0,
                original_inode: original.1,
                adopted_device: md.dev(),
                adopted_inode: md.ino(),
                length: bytes.len() as u64,
                sha256: hex::encode(sha2::Sha256::digest(&bytes)),
            }
        })
        .collect();
    let root_md = fs::metadata(root).unwrap();
    let ledger = Ledger { historical_preservation:None, composite:None,completed_absence:None,
        admission:None, version: 1,
        generation_id: uuid::Uuid::new_v4().to_string(),
        backup_id: uuid::Uuid::new_v4().to_string(),
        manifest_sha256: hex::encode(sha2::Sha256::digest(
            fs::read(root.join(&stage).join("manifest.json")).unwrap(),
        )),
        database_sha256: hex::encode(sha2::Sha256::digest(
            fs::read(root.join("database.sqlite")).unwrap(),
        )),
        root_device: root_md.dev(),
        root_inode: root_md.ino(),
        scopes: vec![Scope {
            namespace: ns.into(),
            op_id: e.op_id.clone(),
            evidence_sha256: hex::encode(sha2::Sha256::digest(&proof_bytes)),
            bindings,
        }],
    };
    let directory = root.join(".restoration-custody");
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    let mut ledger_value = serde_json::to_value(&ledger).unwrap();
    if scenario == "malformed-ledger" {
        ledger_value["unexpected"] = true.into();
    }
    let ledger_bytes = serde_json::to_vec(&ledger_value).unwrap();
    let file = directory.join("current.json");
    fs::write(&file, &ledger_bytes).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    fs::File::open(&file).unwrap().sync_all().unwrap();
    fs::File::open(&directory).unwrap().sync_all().unwrap();
    fs::File::open(root).unwrap().sync_all().unwrap();
    let mut tx = db.begin().await.unwrap();
    sqlx::query("INSERT INTO storage_restore_generations(generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,'committed','fixture')")
        .bind(&ledger.generation_id).bind(&ledger.backup_id).bind(&ledger.manifest_sha256).bind(&ledger.database_sha256).bind(hex::encode(sha2::Sha256::digest(&ledger_bytes))).bind(ledger.root_device.to_string()).bind(if scenario=="sql-mismatch"{(ledger.root_inode+1).to_string()}else{ledger.root_inode.to_string()}).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO storage_restore_current(singleton,generation_id) VALUES(1,$1)")
        .bind(&ledger.generation_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    proof_bytes
}
fn remove_proof_anchor_and_temp(root: &Path, ns: &str, e: &Evidence) {
    let stage = root.join(format!("{ns}/{}", e.op_id));
    let proof: serde_json::Value =
        serde_json::from_slice(&fs::read(stage.join("install-proof.json")).unwrap()).unwrap();
    for name in [
        "install-proof.json",
        "install-anchor",
        proof["temp"].as_str().unwrap(),
    ] {
        fs::remove_file(stage.join(name)).unwrap();
    }
}
fn assert_missing_proof_refuses_without_artifacts(
    root: &Path,
    ns: &str,
    e: &Evidence,
    file: &super::journal_fs::FileDescriptor,
) {
    remove_proof_anchor_and_temp(root, ns, e);
    let stage = root.join(format!("{ns}/{}", e.op_id));
    let before = inventory(&stage);
    assert!(super::journal_fs::publish_file(root, ns, file).is_err());
    assert_eq!(
        inventory(&stage),
        before,
        "rejected publication created staged/proof artifacts"
    );
    assert!(!stage.join("install-proof.json").exists());
    assert!(!stage.join("install-anchor").exists());
    assert_eq!(fs::read(root.join(&e.path)).unwrap(), NEXT);
}
#[tokio::test]
async fn actual_journal_adoption_child() {
    let Ok(root) = std::env::var("STARLANE_RESTART_FIXTURE_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    assert_eq!(super::data_dir(), root);
    let e = evidence(&root);
    let scenario = std::env::var("STARLANE_RESTART_FIXTURE_PHASE").unwrap();
    let db = pool(&root).await;
    let ns = operation_namespace(&e.kind);
    let file = descriptor(&e);
    // A filesystem copy is not vouched for before explicit custody activation.
    assert!(super::journal_fs::verify_file(&root, ns, &file).is_err());
    if scenario == "unregistered-missing-proof" {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .mode(0o700)
            .create(root.join(".restoration-custody"))
            .unwrap();
        assert_missing_proof_refuses_without_artifacts(&root, ns, &e, &file);
        db.close().await;
        return;
    }
    let proof_before = commit_adoption_ledger(&root, &db, &e, &scenario).await;
    // Malformed custody is now refused before opening application state.
    let bootstrap = super::restoration::bootstrap_gate(&root);
    if scenario == "malformed-ledger" {
        assert!(bootstrap.is_err());
        assert!(super::journal_fs::verify_file(&root, ns, &file).is_err());
        assert_eq!(fs::read(root.join(format!("{ns}/{}/install-proof.json", e.op_id))).unwrap(), proof_before);
        db.close().await;
        return;
    }
    bootstrap.unwrap();
    let activation = super::restoration::activate_in(&root, &db).await;
    let rejected = scenario == "sql-mismatch";
    if rejected {
        assert!(activation.is_err());
        assert!(super::journal_fs::verify_file(&root, ns, &file).is_err());
    } else {
        activation.unwrap();
        super::journal_fs::verify_file(&root, ns, &file).unwrap();
        if scenario == "missing-proof" {
            assert_missing_proof_refuses_without_artifacts(&root, ns, &e, &file);
        } else if matches!(scenario.as_str(), "anchor-replaced" | "canonical-replaced") {
            let path = if scenario == "anchor-replaced" {
                root.join(format!("{ns}/{}/install-anchor", e.op_id))
            } else {
                root.join(&e.path)
            };
            let temporary = root.join("same-byte-replacement");
            fs::write(&temporary, fs::read(&path).unwrap()).unwrap();
            fs::rename(&temporary, &path).unwrap();
            assert!(
                super::journal_fs::verify_file(&root, ns, &file).is_err(),
                "same bytes do not prove adopted inode custody"
            );
        } else {
            assert_eq!(scenario, "valid");
            recover(&db).await;
            super::journal_fs::verify_file(&root, ns, &file).unwrap();
            assert_eq!(rewrite::published_source_bytes(&e.path).unwrap(), NEXT);
            let report = inspect(&db, &e, false).await;
            fs::write(
                root.join("adoption-result.json"),
                serde_json::to_vec(&report).unwrap(),
            )
            .unwrap();
        }
    }
    if scenario != "missing-proof" {
        assert_eq!(
            fs::read(root.join(format!("{ns}/{}/install-proof.json", e.op_id))).unwrap(),
            proof_before,
            "original proof bytes were rewritten"
        );
    }
    println!(
        "actual journal custody kind={} scenario={} op_id={} activation_rejected={}",
        e.kind, scenario, e.op_id, rejected
    );
    db.close().await;
}
#[test]
fn actual_journal_destination_adoption_and_tamper_refusal_preserve_original_evidence() {
    for kind in ["creation", "publication", "rewrite"] {
        let source = root();
        let phase = if kind == "creation" {
            "installed-pending"
        } else {
            "committed"
        };
        assert_eq!(
            child(&source, "restart_writer_child", kind, phase, false),
            77
        );
        let original = evidence(&source);
        let before = inventory(&source);
        for scenario in [
            "valid",
            "anchor-replaced",
            "canonical-replaced",
            "malformed-ledger",
            "sql-mismatch",
            "missing-proof",
            "unregistered-missing-proof",
        ] {
            let destination = root();
            copy_tree_preserving_hardlinks(&source, &destination);
            assert_eq!(
                child(
                    &destination,
                    "actual_journal_adoption_child",
                    kind,
                    scenario,
                    false
                ),
                0,
                "adoption {kind}/{scenario} at {}",
                destination.display()
            );
            assert_eq!(
                inventory(&source),
                before,
                "adoption mutated source evidence"
            );
            if scenario == "valid" {
                let report: serde_json::Value = serde_json::from_slice(
                    &fs::read(destination.join("adoption-result.json")).unwrap(),
                )
                .unwrap();
                assert_eq!(report["writer_op_id"], original.op_id);
                parent_verify(&destination, &original, false);
            }
            println!(
                "actual journal adoption validated kind={kind} scenario={scenario} writer_op_id={}",
                original.op_id
            );
            fs::remove_dir_all(destination).unwrap();
        }
        fs::remove_dir_all(source).unwrap();
    }
}
// Guarded Linux child seams. Unset controls are idle, not acceptance.
// ENOSPC is kernel tmpfs capacity exhaustion in the owned private-IPC carrier,
// not a physical device, power-loss, or whole-application recovery certificate.
fn acceptance_root() -> Option<PathBuf> {
    let root = PathBuf::from(std::env::var("STARLANE_SAVE_ACCEPTANCE_ROOT").ok()?);
    assert!(root.is_absolute());
    assert_eq!(super::data_dir(), root);
    let metadata = fs::symlink_metadata(&root).unwrap();
    use std::os::unix::fs::PermissionsExt;
    assert!(metadata.is_dir() && !metadata.file_type().is_symlink());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
    Some(root)
}

#[tokio::test]
async fn actual_sql_commit_failure_child() {
    let Some(root) = acceptance_root() else {
        return;
    };
    let phase = std::env::var("STARLANE_SAVE_ACCEPTANCE_PHASE").unwrap();
    let db = pool(&root).await;
    if phase == "writer" {
        seed(&db, &root, "publication").await;
        let mut conn = db.acquire().await.unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&mut *conn)
            .await
            .unwrap();
        let enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&mut *conn)
            .await
            .unwrap();
        assert_eq!(enabled, 1);
        // The application receipt producer succeeds. Only actual COMMIT detects
        // this deferred FK violation; no explicit rollback substitutes for it.
        for q in [
            "CREATE TABLE acceptance_parent(id TEXT PRIMARY KEY)",
            "CREATE TABLE acceptance_deferred(parent_id TEXT REFERENCES acceptance_parent(id) DEFERRABLE INITIALLY DEFERRED)",
            "CREATE TRIGGER acceptance_commit_failure AFTER INSERT ON storage_journal_receipts BEGIN INSERT INTO acceptance_deferred(parent_id) VALUES('missing-parent'); END",
        ] {sqlx::query(q).execute(&mut *conn).await.unwrap();}
        use sqlx::Acquire;
        let mut tx = (&mut *conn).begin().await.unwrap();
        crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &["space-a"])
            .await
            .unwrap();
        let m = journal::stage_document(
            &mut tx,
            "space-a",
            "new-page",
            "new-draft",
            "new",
            "a",
            "New",
            "",
            "PRIVATE COMMIT FAILURE",
        )
        .await
        .unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at) VALUES('new-page','space-a','New',$1,'owner-a','owner-a','new')").bind(&m.path).execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('new-draft','new-page','space-a','owner-a','New','PRIVATE COMMIT FAILURE','new')").execute(&mut *tx).await.unwrap();
        let n: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM storage_journal_receipts WHERE op_id=$1")
                .bind(&m.op_id)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(n, 1);
        save(
            &root,
            &Evidence {
                kind: "creation".into(),
                phase: "failed-commit".into(),
                op_id: m.op_id.clone(),
                path: m.path.clone(),
                manifest: serde_json::to_value(&m).unwrap(),
            },
        );
        let failure = tx
            .commit()
            .await
            .expect_err("real deferred SQL COMMIT must fail");
        assert!(
            failure
                .as_database_error()
                .is_some_and(|e| e.code().as_deref() == Some("787")),
            "{failure:?}"
        );
        drop(conn);
        db.close().await;
        std::process::exit(77);
    }
    assert_eq!(phase, "recovery");
    let e = evidence(&root);
    let m: journal::StagedDocument = serde_json::from_value(e.manifest.clone()).unwrap();
    assert!(!journal::confirm_commit(&db, &m).await.unwrap());
    journal::recover(&db).await.unwrap();
    for q in [
        "SELECT COUNT(*) FROM documents WHERE id='new-page'",
        "SELECT COUNT(*) FROM document_drafts WHERE id='new-draft'",
        "SELECT COUNT(*) FROM storage_journal_receipts",
        "SELECT COUNT(*) FROM acceptance_deferred",
    ] {
        let n: i64 = sqlx::query_scalar(q).fetch_one(&db).await.unwrap();
        assert_eq!(n, 0, "{q}");
    }
    assert!(!root.join(&e.path).exists());
    assert_eq!(fs::read(root.join(SOURCE)).unwrap(), OLD);
    let payload = root.join(".storage-journal").join(&e.op_id).join("payload");
    assert_eq!(fs::read(&payload).unwrap(), b"PRIVATE COMMIT FAILURE");
    assert_eq!(
        hex::encode(sha2::Sha256::digest(fs::read(&payload).unwrap())),
        m.sha256
    );
    println!("genuine SQL COMMIT failure: original op {} retained, receipt/page/draft absent, original body intact after fresh recovery",e.op_id);
    db.close().await;
}

fn exhaust_private_tmpfs(root: &Path) -> fs::File {
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(root.as_os_str().as_bytes()).unwrap();
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    assert_eq!(unsafe { libc::statfs(path.as_ptr(), stat.as_mut_ptr()) }, 0);
    assert_eq!(unsafe { stat.assume_init() }.f_type, libc::TMPFS_MAGIC);
    assert_eq!(
        std::env::var("STARLANE_SAVE_PRIVATE_TMPFS").unwrap(),
        "owned-container-private-ipc"
    );
    assert!(root.starts_with("/dev/shm"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("capacity-reservation"))
        .unwrap();
    let chunk = [0x52u8; 65536];
    let mut written = 0u64;
    loop {
        match file.write(&chunk) {
            Ok(n) => {
                assert!(n > 0);
                written += n as u64;
                assert!(
                    written <= 128 * 1024 * 1024,
                    "not the bounded private tmpfs"
                );
            }
            Err(e) => {
                assert_eq!(e.raw_os_error(), Some(libc::ENOSPC));
                break;
            }
        }
    }
    assert!(written > 262144);
    // Leave enough space for directory/envelope allocation, but less than the
    // authentic half-MiB payload or installation anchor being written.
    file.set_len(written - 131072).unwrap();
    file.sync_all().unwrap();
    file
}

#[tokio::test]
async fn actual_kernel_enospc_child() {
    let Some(root) = acceptance_root() else {
        return;
    };
    let phase = std::env::var("STARLANE_SAVE_ACCEPTANCE_PHASE").unwrap();
    let disk = PathBuf::from(std::env::var("STARLANE_SAVE_DATABASE_ROOT").unwrap());
    assert!(disk.is_absolute() && !disk.starts_with("/dev/shm"));
    let db = pool(&disk).await;
    if phase == "partial-stage" || phase == "pending-install" {
        seed(&db, &root, "publication").await;
        let body = "Z".repeat(524288);
        let mut tx = db.begin().await.unwrap();
        crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &["space-a"])
            .await
            .unwrap();
        if phase == "partial-stage" {
            let filler = exhaust_private_tmpfs(&root);
            let error = journal::stage_document(
                &mut tx,
                "space-a",
                "new-page",
                "new-draft",
                "new",
                "a",
                "New",
                "",
                &body,
            )
            .await
            .err()
            .expect("kernel ENOSPC must reject stage");
            assert!(format!("{error:?}").contains("No space left"), "{error:?}");
            tx.rollback().await.unwrap();
            filler.set_len(0).unwrap();
            drop(filler);
            let dirs = fs::read_dir(root.join(".storage-journal"))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(dirs.len(), 1);
            let payload = dirs[0].path().join("payload");
            let partial = fs::read(&payload).unwrap();
            assert!(!partial.is_empty() && partial.len() < body.len());
            assert!(partial.iter().all(|b| *b == b'Z'));
            assert!(!dirs[0].path().join("manifest.json").exists());
            fs::write(disk.join("capacity-result.json"),serde_json::to_vec(&serde_json::json!({"phase":phase,"op_id":dirs[0].file_name().to_string_lossy(),"partial_length":partial.len()})).unwrap()).unwrap();
        } else {
            let m = journal::stage_document(
                &mut tx,
                "space-a",
                "new-page",
                "new-draft",
                "new",
                "a",
                "New",
                "",
                &body,
            )
            .await
            .unwrap();
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at) VALUES('new-page','space-a','New',$1,'owner-a','owner-a','new')").bind(&m.path).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('new-draft','new-page','space-a','owner-a','New',$1,'new')").bind(&body).execute(&mut *tx).await.unwrap();
            tx.commit().await.unwrap();
            let filler = exhaust_private_tmpfs(&root);
            assert!(
                !journal::publish_committed(&db, &m).await,
                "full install must remain pending"
            );
            assert!(!root.join(&m.path).exists());
            let state: String =
                sqlx::query_scalar("SELECT state FROM storage_journal_receipts WHERE op_id=$1")
                    .bind(&m.op_id)
                    .fetch_one(&db)
                    .await
                    .unwrap();
            assert_eq!(state, "pending");
            filler.set_len(0).unwrap();
            drop(filler);
            fs::write(disk.join("capacity-result.json"),serde_json::to_vec(&serde_json::json!({"phase":phase,"op_id":m.op_id,"path":m.path,"sha256":m.sha256,"length":m.length})).unwrap()).unwrap();
        }
        assert_eq!(fs::read(root.join(SOURCE)).unwrap(), OLD);
        db.close().await;
        std::process::exit(77);
    }
    assert_eq!(phase, "recovery");
    let result: serde_json::Value =
        serde_json::from_slice(&fs::read(disk.join("capacity-result.json")).unwrap()).unwrap();
    let op = result["op_id"].as_str().unwrap();
    let retained = root.join(".storage-journal").join(op);
    if result["phase"] == "partial-stage" {
        let before = fs::read(retained.join("payload")).unwrap();
        let mut conn = db.acquire().await.unwrap();
        assert!(
            journal::inspect_restoration(&root, &mut conn, op)
                .await
                .is_err(),
            "incomplete envelope cannot establish recovery authority"
        );
        drop(conn);
        // Production recovery retains/reports precommit orphan corruption and
        // keeps the existing deployment available; it must never guess an install.
        journal::recover(&db).await.unwrap();
        assert_eq!(fs::read(retained.join("payload")).unwrap(), before);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM storage_journal_receipts")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE id='new-page'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0);
    } else {
        journal::recover(&db).await.unwrap();
        let bytes = fs::read(root.join(result["path"].as_str().unwrap())).unwrap();
        assert_eq!(bytes, vec![b'Z'; 524288]);
        assert_eq!(
            hex::encode(sha2::Sha256::digest(&bytes)),
            result["sha256"].as_str().unwrap()
        );
        let state: String =
            sqlx::query_scalar("SELECT state FROM storage_journal_receipts WHERE op_id=$1")
                .bind(op)
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(state, "complete");
        assert_eq!(fs::read(retained.join("payload")).unwrap(), bytes);
    }
    assert_eq!(fs::read(root.join(SOURCE)).unwrap(), OLD);
    println!(
        "genuine kernel ENOSPC {}: actual writer op {} preserved through fresh process recovery",
        result["phase"], op
    );
    db.close().await;
}
