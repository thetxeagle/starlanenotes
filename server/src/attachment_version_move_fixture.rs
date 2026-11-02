//! Explicit actual-producer move acceptance. No alternate owner/authentication implementation.
use crate::attachment_upload_engine_fixture as real;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::{AnyPool, Connection};
use std::path::{Path, PathBuf};
use tower::ServiceExt;
const ORIGINAL: &[u8] = b"\0original immutable bytes\xff\r\n";
const REPLACED: &[u8] = b"\xffdeliberate replacement different immutable bytes\0";
fn work() -> PathBuf {
    PathBuf::from(
        std::env::var("STARLANE_VERSION_MOVE_WORKSPACE")
            .expect("required private fixture workspace"),
    )
}
fn app(s: &crate::AppState) -> axum::Router {
    crate::api_routes()
        .with_state(s.clone())
}
async fn get(s: &crate::AppState, t: &str, uri: &str) -> (StatusCode, Vec<u8>) {
    let r = Request::builder()
        .uri(uri.trim_start_matches("/api/v1"))
        .header("authorization", format!("Bearer {t}"))
        .body(Body::empty())
        .unwrap();
    let response = app(s).oneshot(r).await.unwrap();
    let status = response.status();
    (
        status,
        to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
}
async fn replacement(s: &crate::AppState, t: &str, first: &Value) -> Value {
    let id = uuid::Uuid::new_v4().to_string();
    let mut bytes=b"--owned\r\nContent-Disposition: form-data; name=\"file\"; filename=\"same.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec();
    bytes.extend_from_slice(REPLACED);
    bytes.extend_from_slice(b"\r\n--owned--\r\n");
    let r = Request::builder()
        .method("POST")
        .uri(format!(
            "/spaces/upload-space/documents/upload-page/attachment-families/{}/versions",
            first["id"].as_str().unwrap()
        ))
        .header("authorization", format!("Bearer {t}"))
        .header("content-type", "multipart/form-data; boundary=owned")
        .header("x-starlane-upload-operation", id)
        .header(
            "x-starlane-attachment-version",
            first["id"].as_str().unwrap(),
        )
        .header("x-starlane-attachment-revision", "1")
        .body(Body::from(bytes))
        .unwrap();
    let response = app(s).oneshot(r).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    serde_json::from_slice(&to_bytes(response.into_body(), 100000).await.unwrap()).unwrap()
}
async fn immutable_sql(db: &AnyPool) -> Value {
    let mut c = db.acquire().await.unwrap();
    let mut value = json!({});
    for table in [
        "attachment_upload_receipts",
        "attachment_families",
        "attachment_version_records",
        "attachment_version_changes",
    ] {
        value[table] = json!(crate::spaces::document_deletion::rows(
            &mut c,
            &format!("SELECT * FROM {table} ORDER BY 1"),
            &[]
        )
        .await
        .unwrap());
    }
    value
}
fn evidence(root: &Path) -> Value {
    use sha2::{Digest, Sha256};
    let mut out = json!({});
    for entry in std::fs::read_dir(root.join(".attachment-upload-journal")).unwrap() {
        let entry = entry.unwrap();
        let id = entry.file_name().to_str().unwrap().to_owned();
        for file in std::fs::read_dir(entry.path()).unwrap() {
            let file=file.unwrap();assert!(file.file_type().unwrap().is_file(),"owned journal fixture contains only regular evidence files");
            let name=file.file_name().to_str().unwrap().to_owned();
            out[format!("{id}/{name}")]=json!(hex::encode(Sha256::digest(std::fs::read(file.path()).unwrap())));
        }
    }
    out
}
async fn verify_current(s: &crate::AppState, expected: &Value) {
    let owner = expected["owner"].as_str().unwrap();
    let viewer = expected["viewer"].as_str().unwrap();
    let target = expected["target"].as_str().unwrap();
    for (url, bytes) in [
        (expected["original"]["url"].as_str().unwrap(), ORIGINAL),
        (expected["replacement_url"].as_str().unwrap(), REPLACED),
    ] {
        if expected["retire_original"]==true && url==expected["original"]["url"].as_str().unwrap() {
            assert_ne!(get(s,owner,url).await.0,StatusCode::OK);
            assert_ne!(get(s,viewer,url).await.0,StatusCode::OK);
        } else {
            assert_eq!(get(s, owner, url).await, (StatusCode::OK, bytes.to_vec()));
            assert_eq!(get(s, viewer, url).await, (StatusCode::OK, bytes.to_vec()));
        }
    }
    let uri = format!(
        "/spaces/{target}/documents/upload-page/attachment-families/{}/versions",
        expected["original"]["id"].as_str().unwrap()
    );
    let (code, raw) = get(s, viewer, &uri).await;
    assert_eq!(code, StatusCode::OK);
    let h: Value = serde_json::from_slice(&raw).unwrap();
    assert_eq!(h["total"], 2);
    assert_eq!(h["versions"][0]["version_id"], expected["original"]["id"]);
    assert_eq!(h["versions"][1], expected["replacement"]["version"]);
    if expected["retire_original"]==true {
        assert_eq!(h["retained_versions"][expected["original"]["id"].as_str().unwrap()]["state"],"retained");
        assert_eq!(h["retained_versions"][expected["original"]["id"].as_str().unwrap()]["space_id"],"upload-space");
    }
    for v in h["versions"].as_array().unwrap() {
        assert_eq!(v["original_space_id"], "upload-space");
    }
    assert_eq!(
        get(
            s,
            owner,
            &format!(
                "/spaces/upload-space/documents/upload-page/attachment-families/{}/versions",
                expected["original"]["id"].as_str().unwrap()
            )
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    println!("VERSION_MOVE_REAL_CURRENT_OWNER_VIEWER_HISTORY_ORIGINAL_AND_REPLACEMENT_PINS_PASS");
}
async fn run(engine: &str, retire_original: bool) {
    let root = crate::storage::data_dir();
    assert!(root.is_absolute() && root.starts_with(work()));
    std::fs::create_dir_all(&root).unwrap();
    let (url, schema) = if engine == "postgres" {
        let url = std::env::var("STARLANE_UPLOAD_ENGINE_PG_URL").expect("owned PG URL required");
        sqlx::any::install_default_drivers();
        let schema = format!("version_move_{}", uuid::Uuid::new_v4().simple());
        let mut c = sqlx::AnyConnection::connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&mut c)
            .await
            .unwrap();
        c.close().await.unwrap();
        (url, schema)
    } else {
        (
            format!(
                "sqlite://{}?mode=rwc",
                root.join("starlane-notes.db").display()
            ),
            String::new(),
        )
    };
    let db = real::pool(&url, &schema).await;
    crate::db::migrate_selected(&db, crate::db::DatabaseEngine::from_url(&url).unwrap())
        .await
        .unwrap();
    real::seed(&db).await;
    // Controlled initial destination metadata, matching the existing source fixture seed.
    sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('upload-destination','Destination fixture','upload-destination','private','upload-owner')").execute(&db).await.unwrap();
    sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('destination-owner','upload-destination','upload-owner','owner')").execute(&db).await.unwrap();
    let s = real::state(db, &root);
    let owner = real::login(&s, "upload-owner").await;
    let viewer = real::login(&s, "upload-viewer").await;
    real::ok(
        &s,
        Some(&owner),
        "POST",
        "/spaces/upload-destination/members",
        json!({"user_id":"upload-viewer","role":"viewer"}),
    )
    .await;
    sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('upload-second-destination','Second destination fixture','upload-second-destination','private','upload-owner')").execute(&s.db).await.unwrap();
    sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('second-destination-owner','upload-second-destination','upload-owner','owner')").execute(&s.db).await.unwrap();
    let born=real::ok(&s,Some(&owner),"POST","/spaces/upload-space/documents",json!({"source_id":"upload-page","title":"Pinned immutable versions","content":"Initial","property_values":[]})).await;
    let path = format!(
        "/spaces/upload-space/documents/upload-page/drafts/{}",
        born["draft_id"].as_str().unwrap()
    );
    let selected = real::ok(&s, Some(&owner), "GET", &path, Value::Null).await;
    real::ok(
        &s,
        Some(&owner),
        "PUT",
        &path,
        json!({"source_digest":selected["source_digest"],"content":"Initial saved published page"}),
    )
    .await;
    real::ok(
        &s,
        Some(&owner),
        "POST",
        &format!("{path}/publish"),
        json!({}),
    )
    .await;
    let op = uuid::Uuid::new_v4().to_string();
    let (code, original) = real::upload(&s, &owner, &op, ORIGINAL).await;
    assert_eq!(code, StatusCode::CREATED);
    let replaced = replacement(&s, &owner, &original).await;
    let replacement_url = format!(
        "/api/v1/files/{}",
        replaced["version"]["original_storage_path"]
            .as_str()
            .unwrap()
    );
    let born = real::ok(
        &s,
        Some(&owner),
        "POST",
        "/spaces/upload-space/documents/upload-page/drafts",
        json!({}),
    )
    .await;
    let path = format!(
        "/spaces/upload-space/documents/upload-page/drafts/{}",
        born["id"].as_str().unwrap()
    );
    let selected = real::ok(&s, Some(&owner), "GET", &path, Value::Null).await;
    let body=format!("Original exact pin: [same.bin]({})\nReplacement exact pin: [same.bin]({replacement_url})\n",original["url"].as_str().unwrap());
    real::ok(
        &s,
        Some(&owner),
        "PUT",
        &path,
        json!({"source_digest":selected["source_digest"],"content":body}),
    )
    .await;
    real::ok(
        &s,
        Some(&owner),
        "POST",
        &format!("{path}/publish"),
        json!({}),
    )
    .await;
    if retire_original {
        retirement_original_before_move(&s,&owner,&viewer,&original,crate::db::DatabaseEngine::from_url(&url).unwrap()).await;
    }
    let original_retirement=retirement_rows(&s.db).await;
    let before_sql = immutable_sql(&s.db).await;
    let before_evidence = evidence(&root);
    let old_path: String =
        sqlx::query_scalar("SELECT content_path FROM documents WHERE id='upload-page'")
            .fetch_one(&s.db)
            .await
            .unwrap();
    let published = std::fs::read(root.join(&old_path)).unwrap();
    let preview=real::ok(&s,Some(&owner),"POST","/spaces/upload-space/moves/preview",json!({"root":{"kind":"document","id":"upload-page"},"destination":{"space_id":"upload-destination","folder_id":null,"parent_id":null}})).await;
    assert_eq!(preview["blockers"], json!([]));
    assert_eq!(preview["commit_available"], true);
    let moved=real::ok(&s,Some(&owner),"POST",&format!("/moves/previews/{}/commit",preview["preview_id"].as_str().unwrap()),json!({"fingerprint":preview["fingerprint"],"decisions":[],"public_links":"disable","confirm_audience_change":true,"confirm_draft_space_change":true})).await;
    let new_path:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='upload-page' AND space_id='upload-destination'").fetch_one(&s.db).await.unwrap();
    assert_ne!(new_path, old_path);
    assert_eq!(std::fs::read(root.join(&new_path)).unwrap(), published);
    assert_eq!(std::fs::read(root.join(&old_path)).unwrap(), published);
    assert_eq!(immutable_sql(&s.db).await, before_sql);
    assert_eq!(evidence(&root), before_evidence);
    let mut expected = json!({"url":url,"schema":schema,"owner":owner,"viewer":viewer,"target":"upload-destination","original":original,"replacement":replaced,"replacement_url":replacement_url,"immutable_sql":before_sql,"evidence":before_evidence,"new_path":new_path,"published_hex":hex::encode(&published),"move_ack":moved,"retire_original":retire_original,"original_retirement":original_retirement});
    assert_eq!(retirement_rows(&s.db).await,expected["original_retirement"]);
    verify_current(&s, &expected).await;
    // A SQL-only alias edit cannot mint relocation authority; equal bytes do not suffice.
    let original_alias = expected["original"]["url"]
        .as_str()
        .unwrap()
        .trim_start_matches("/api/v1/files/");
    let prior_asset: String =
        sqlx::query_scalar("SELECT asset_id FROM move_asset_path_aliases WHERE old_path=$1")
            .bind(original_alias)
            .fetch_one(&s.db)
            .await
            .unwrap();
    sqlx::query("UPDATE move_asset_path_aliases SET asset_id=$2 WHERE old_path=$1")
        .bind(original_alias)
        .bind(
            expected["replacement"]["version"]["version_id"]
                .as_str()
                .unwrap(),
        )
        .execute(&s.db)
        .await
        .unwrap();
    assert_ne!(
        get(
            &s,
            expected["owner"].as_str().unwrap(),
            expected["original"]["url"].as_str().unwrap()
        )
        .await
        .0,
        StatusCode::OK
    );
    sqlx::query("UPDATE move_asset_path_aliases SET asset_id=$2 WHERE old_path=$1")
        .bind(original_alias)
        .bind(prior_asset)
        .execute(&s.db)
        .await
        .unwrap();
    let current: String = sqlx::query_scalar("SELECT storage_path FROM attachments WHERE id=$1")
        .bind(
            expected["replacement"]["version"]["version_id"]
                .as_str()
                .unwrap(),
        )
        .fetch_one(&s.db)
        .await
        .unwrap();
    let file = root.join(&current);
    let retained = file.with_extension("retained-owned");
    std::fs::rename(&file, &retained).unwrap();
    std::fs::write(&file, REPLACED).unwrap();
    assert_ne!(
        get(&s, expected["owner"].as_str().unwrap(), &replacement_url)
            .await
            .0,
        StatusCode::OK
    );
    std::fs::remove_file(&file).unwrap();
    std::fs::rename(&retained, &file).unwrap();
    println!("VERSION_MOVE_COUNTERFEIT_ALIAS_EQUAL_BYTE_REPLACEMENT_REFUSED_PASS");
    // Replace a real equal-byte leaf precisely after owner verification, before
    // the final held-descriptor fence. Both original custody and moved custody
    // must refuse this race rather than certifying the replacement descriptor.
    for (phase, relative) in [
        (
            "upload",
            expected["replacement_url"]
                .as_str()
                .unwrap()
                .trim_start_matches("/api/v1/files/")
                .to_owned(),
        ),
        ("relocation", current.clone()),
    ] {
        let raced = root.join(relative);
        let retained = raced.with_extension("race-retained-owned");
        let ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let did_run = ran.clone();
        let replacement_leaf = raced.clone();
        let retained_leaf = retained.clone();
        crate::storage::attachment_versions::set_version_read_race(
            expected["replacement"]["version"]["version_id"]
                .as_str()
                .unwrap()
                .into(),
            phase,
            Box::new(move || {
                std::fs::rename(&replacement_leaf, &retained_leaf).unwrap();
                std::fs::write(&replacement_leaf, REPLACED).unwrap();
                did_run.store(true, std::sync::atomic::Ordering::SeqCst);
            }),
        );
        assert_ne!(
            get(&s, expected["owner"].as_str().unwrap(), &replacement_url)
                .await
                .0,
            StatusCode::OK
        );
        assert!(
            ran.load(std::sync::atomic::Ordering::SeqCst),
            "Real owner race seam must execute"
        );
        std::fs::remove_file(&raced).unwrap();
        std::fs::rename(&retained, &raced).unwrap();
        assert_eq!(
            get(&s, expected["owner"].as_str().unwrap(), &replacement_url)
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(immutable_sql(&s.db).await, before_sql);
        assert_eq!(evidence(&root), before_evidence);
    }
    println!("VERSION_MOVE_HELD_BEFORE_OWNER_AND_RELOCATION_VERIFY_RACES_REFUSED_PASS");
    let v: crate::storage::attachment_versions::VersionRecord =
        serde_json::from_value(expected["replacement"]["version"].clone()).unwrap();
    sqlx::query("DELETE FROM attachment_version_records WHERE version_id=$1")
        .bind(&v.version_id)
        .execute(&s.db)
        .await
        .unwrap();
    assert_ne!(
        get(&s, expected["owner"].as_str().unwrap(), &replacement_url)
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(evidence(&root), before_evidence);
    sqlx::query("INSERT INTO attachment_version_records(version_id,logical_attachment_id,version_number,original_space_id,original_document_id,uploader_id,original_name,mime_type,byte_length,binary_sha256,original_created_at,original_storage_path,original_filename,upload_operation_id,upload_attempt_id,raw_receipt_sha256,intent_sha256,install_proof_sha256,provenance) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)")
        .bind(&v.version_id).bind(&v.logical_attachment_id).bind(v.version_number).bind(&v.original_space_id).bind(&v.original_document_id).bind(&v.uploader_id).bind(&v.original_name).bind(&v.mime_type).bind(v.byte_length).bind(&v.binary_sha256).bind(&v.original_created_at).bind(&v.original_storage_path).bind(&v.original_filename).bind(&v.upload_operation_id).bind(&v.upload_attempt_id).bind(&v.raw_receipt_sha256).bind(&v.intent_sha256).bind(&v.install_proof_sha256).bind(&v.provenance).execute(&s.db).await.unwrap();
    assert_eq!(immutable_sql(&s.db).await, before_sql);
    assert_eq!(
        get(&s, expected["owner"].as_str().unwrap(), &replacement_url)
            .await
            .0,
        StatusCode::OK
    );
    println!("VERSION_MOVE_PARTIAL_REGISTRY_LEGACY_DOWNGRADE_REFUSED_PASS");
    // Ordinary page publication must not invalidate unrelated immutable attachment
    // custody from the same relocation manifest. Produce and save through real APIs.
    let born = real::ok(
        &s,
        Some(&owner),
        "POST",
        "/spaces/upload-destination/documents/upload-page/drafts",
        json!({}),
    )
    .await;
    let draft_path = format!(
        "/spaces/upload-destination/documents/upload-page/drafts/{}",
        born["id"].as_str().unwrap()
    );
    let selected = real::ok(&s, Some(&owner), "GET", &draft_path, Value::Null).await;
    let next_body = format!(
        "Ordinary page publication after move\n\n![original]({})\n\n[replacement]({})\n",
        original["url"].as_str().unwrap(),
        replacement_url
    );
    real::ok(
        &s,
        Some(&owner),
        "PUT",
        &draft_path,
        json!({"source_digest":selected["source_digest"],"content":next_body}),
    )
    .await;
    real::ok(
        &s,
        Some(&owner),
        "POST",
        &format!("{draft_path}/publish"),
        json!({}),
    )
    .await;
    let published_path: String = sqlx::query_scalar("SELECT content_path FROM documents WHERE id='upload-page' AND space_id='upload-destination'").fetch_one(&s.db).await.unwrap();
    assert_ne!(published_path, new_path);
    let edited = std::fs::read(root.join(&published_path)).unwrap();
    assert_ne!(edited, published);
    assert!(String::from_utf8_lossy(&edited).contains(&next_body));
    assert_eq!(std::fs::read(root.join(&new_path)).unwrap(), published);
    assert_eq!(std::fs::read(root.join(&old_path)).unwrap(), published);
    expected["new_path"] = json!(published_path);
    expected["published_hex"] = json!(hex::encode(&edited));
    verify_current(&s, &expected).await;
    assert_eq!(std::fs::read(root.join(&published_path)).unwrap(), edited);
    assert_eq!(immutable_sql(&s.db).await, before_sql);
    assert_eq!(evidence(&root), before_evidence);
    println!("VERSION_MOVE_REAL_DIGEST_SAVE_PUBLICATION_PRESERVES_BOTH_IMMUTABLE_PINS_PASS");

    use std::os::unix::fs::PermissionsExt;
    let expected_path = work().join("expected.json");
    std::fs::write(&expected_path, expected.to_string()).unwrap();
    std::fs::set_permissions(&expected_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    s.db.close().await;
    drop(s);
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "attachment_version_move_fixture::fresh_version_move_child",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert_eq!(
        child.status.code(),
        Some(0),
        "fresh child: {} {}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    assert!(String::from_utf8_lossy(&child.stdout)
        .contains("VERSION_MOVE_FRESH_RECOVERY_CURRENT_REVOCATION_RAW_HISTORY_BYTES_PASS"));
    println!("{}", String::from_utf8_lossy(&child.stdout));
    println!("VERSION_MOVE_REAL_REVIEW_COMMIT_FULL_PUBLISHED_BYTES_IMMUTABLE_RECEIPTS_PASS");
}
#[tokio::test]
#[ignore = "requires absolute process-isolated DATA_DIR and private workspace"]
async fn required_version_move_sqlite() {
    run("sqlite",false).await;
}
#[tokio::test]
#[ignore = "requires disposable owned PostgreSQL and absolute process-isolated DATA_DIR"]
async fn required_version_move_postgres() {
    run("postgres",false).await;
}
#[tokio::test]
#[ignore="requires private migrated SQLite real producer and fresh child"]
async fn required_retirement_move_sqlite(){run("sqlite",true).await;}
#[tokio::test]
#[ignore="requires owned migrated PostgreSQL real producer and fresh child"]
async fn required_retirement_move_postgres(){run("postgres",true).await;}
#[tokio::test]
#[ignore = "fresh-process child of genuine producer; explicit expected fixture required"]
async fn fresh_version_move_child() {
    let root = crate::storage::data_dir();
    assert!(root.is_absolute() && root.starts_with(work()));
    let expected: Value =
        serde_json::from_slice(&std::fs::read(work().join("expected.json")).unwrap()).unwrap();
    let db = real::pool(
        expected["url"].as_str().unwrap(),
        expected["schema"].as_str().unwrap(),
    )
    .await;
    let s = real::state(db, &root);
    crate::storage::restoration::bootstrap_gate(&root).unwrap();
    crate::storage::restoration::activate_in(&root, &s.db)
        .await
        .unwrap();
    crate::storage::relocation::recover(&s.db).await.unwrap();
    crate::storage::journal::recover(&s.db).await.unwrap();
    crate::storage::canonical_rewrite::recover(&s.db)
        .await
        .unwrap();
    crate::storage::publication::recover(&s.db).await.unwrap();
    crate::storage::attachment_upload::recover_in(&root, &s.db)
        .await
        .unwrap();
    verify_current(&s, &expected).await;
    assert_eq!(immutable_sql(&s.db).await, expected["immutable_sql"]);
    assert_eq!(evidence(&root), expected["evidence"]);
    assert_eq!(
        std::fs::read(root.join(expected["new_path"].as_str().unwrap())).unwrap(),
        hex::decode(expected["published_hex"].as_str().unwrap()).unwrap()
    );
    real::ok(
        &s,
        Some(expected["owner"].as_str().unwrap()),
        "DELETE",
        "/spaces/upload-destination/members/upload-viewer",
        Value::Null,
    )
    .await;
    for url in [
        expected["original"]["url"].as_str().unwrap(),
        expected["replacement_url"].as_str().unwrap(),
    ] {
        assert_ne!(
            get(&s, expected["viewer"].as_str().unwrap(), url).await.0,
            StatusCode::OK
        );
    }
    let h = format!(
        "/spaces/upload-destination/documents/upload-page/attachment-families/{}/versions",
        expected["original"]["id"].as_str().unwrap()
    );
    assert_ne!(
        get(&s, expected["viewer"].as_str().unwrap(), &h).await.0,
        StatusCode::OK
    );
    // Source membership survives; it never authorizes historical pins after destination revocation.
    assert!(sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM space_members WHERE space_id='upload-space' AND user_id='upload-viewer'"
    )
    .fetch_optional(&s.db)
    .await
    .unwrap()
    .is_some());
    assert_eq!(immutable_sql(&s.db).await, expected["immutable_sql"]);
    assert_eq!(evidence(&root), expected["evidence"]);
    if expected["retire_original"]==true {
        assert_eq!(retirement_rows(&s.db).await,expected["original_retirement"]);
        retirement_after_move(&s,&expected).await;
        s.db.close().await;
    } else {post_move_replacement_journey(s,&expected).await;}
    println!("VERSION_MOVE_FRESH_RECOVERY_CURRENT_REVOCATION_RAW_HISTORY_BYTES_PASS");
}

include!("attachment_retirement_fixture.inc");

include!("attachment_registry_upgrade_fixture.inc");
