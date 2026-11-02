//! Explicit genuine dual-engine direct-upload prerequisite, separate from registry changes.
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::{AnyPool, Connection};
use std::path::{Path, PathBuf};
use tower::ServiceExt;
const JWT: &str = "owned-upload-engine-jwt-secret";
const PASSWORD: &str = "owned-fixture-only-password";
const PAYLOAD: &[u8] = b"\0exact dual-engine pinned upload\xff\r\nbytes";
fn workspace() -> PathBuf {
    PathBuf::from(
        std::env::var("STARLANE_UPLOAD_ENGINE_WORKSPACE").expect("required private workspace"),
    )
}
pub(crate) async fn pool(url: &str, schema: &str) -> AnyPool {
    sqlx::any::install_default_drivers();
    let engine = crate::db::DatabaseEngine::from_url(url).unwrap();
    let schema = schema.to_owned();
    sqlx::any::AnyPoolOptions::new()
        .max_connections(2)
        .after_connect(move |c, _| {
            let schema = schema.clone();
            Box::pin(async move {
                match engine {
                    crate::db::DatabaseEngine::Postgres => {
                        sqlx::query(&format!("SET search_path TO {schema}"))
                            .execute(c)
                            .await?;
                    }
                    crate::db::DatabaseEngine::Sqlite => {
                        sqlx::query("PRAGMA foreign_keys=ON")
                            .execute(&mut *c)
                            .await?;
                        sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;
                    }
                }
                Ok(())
            })
        })
        .connect(url)
        .await
        .unwrap()
}
pub(crate) fn state(db: AnyPool, root: &Path) -> crate::AppState {
    crate::AppState {
        db,
        jwt_secret: JWT.into(),
        base_url: "http://owned-upload.invalid".into(),
        search_index: crate::search::SearchIndex::new(root.join("search_index")).unwrap(),
        oauth_cache: crate::auth::routes::OAuthStateCache::new(),
        ws_rooms: crate::sync::WsRooms::default(),
    }
}
pub(crate) async fn call(
    s: &crate::AppState,
    token: Option<&str>,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    println!("UPLOAD_ENGINE_STEP {method} {uri}");
    let mut r = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = token {
        r = r.header("authorization", format!("Bearer {t}"));
    }
    let mut r = r.body(Body::from(body.to_string())).unwrap();
    r.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:43123".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = crate::api_routes()
        .with_state(s.clone())
        .oneshot(r)
        .await
        .unwrap();
    let code = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (
        code,
        if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(json!({"non_json":true}))
        },
    )
}
pub(crate) async fn ok(
    s: &crate::AppState,
    token: Option<&str>,
    method: &str,
    uri: &str,
    body: Value,
) -> Value {
    let (code, v) = call(s, token, method, uri, body).await;
    assert!(
        code.is_success(),
        "actual {method} {uri}: {code}; response suppressed"
    );
    v
}
pub(crate) async fn login(s: &crate::AppState, user: &str) -> String {
    let v = ok(
        s,
        None,
        "POST",
        "/auth/login",
        json!({"email":format!("{user}@upload.invalid"),"password":PASSWORD}),
    )
    .await;
    v["access_token"].as_str().unwrap().to_owned()
}
pub(crate) async fn seed(db: &AnyPool) {
    sqlx::query("INSERT INTO system_config(key,value) VALUES('max_upload_size_mb','1')").execute(db).await.unwrap();
    for (user, role) in [("upload-owner", "owner"), ("upload-viewer", "viewer")] {
        let hash = crate::auth::password::hash_password(PASSWORD).unwrap();
        sqlx::query("INSERT INTO users(id,email,display_name,role,account_kind,is_active,password_hash) VALUES($1,$2,$1,'member','member',1,$3)").bind(user).bind(format!("{user}@upload.invalid")).bind(hash).execute(db).await.unwrap();
        if role == "owner" {
            sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('upload-space','Upload fixture','upload-space','private','upload-owner')").execute(db).await.unwrap();
        }
        sqlx::query(
            "INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'upload-space',$2,$3)",
        )
        .bind(format!("member-{user}"))
        .bind(user)
        .bind(role)
        .execute(db)
        .await
        .unwrap();
    }
}
pub(crate) async fn upload(s: &crate::AppState, token: &str, id: &str, bytes: &[u8]) -> (StatusCode, Value) {
    let mut body=b"--owned\r\nContent-Disposition: form-data; name=\"file\"; filename=\"same.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec();
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n--owned--\r\n");
    let r = Request::builder()
        .method("POST")
        .uri("/spaces/upload-space/documents/upload-page/attachments")
        .header("authorization", format!("Bearer {token}"))
        .header("x-starlane-upload-operation", id)
        .header("content-type", "multipart/form-data; boundary=owned")
        .body(Body::from(body))
        .unwrap();
    let response = crate::api_routes()
        .with_state(s.clone())
        .oneshot(r)
        .await
        .unwrap();
    let code = response.status();
    let bytes = to_bytes(response.into_body(), 100000).await.unwrap();
    (code, serde_json::from_slice(&bytes).unwrap())
}
pub(crate) async fn download(s: &crate::AppState, token: &str, url: &str) -> (StatusCode, Vec<u8>) {
    let r = Request::builder()
        .uri(url.trim_start_matches("/api/v1"))
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let response = crate::api_routes()
        .with_state(s.clone())
        .oneshot(r)
        .await
        .unwrap();
    let code = response.status();
    (
        code,
        to_bytes(response.into_body(), 100000)
            .await
            .unwrap()
            .to_vec(),
    )
}
async fn flow(s: &crate::AppState) -> Value {
    let owner = login(s, "upload-owner").await;
    let viewer = login(s, "upload-viewer").await;
    let created=ok(s,Some(&owner),"POST","/spaces/upload-space/documents",json!({"source_id":"upload-page","title":"Upload original page","content":"Initial exact source","property_values":[]})).await;
    let draft = created["draft_id"].as_str().unwrap();
    let uri = format!("/spaces/upload-space/documents/upload-page/drafts/{draft}");
    let selected = ok(s, Some(&owner), "GET", &uri, Value::Null).await;
    ok(s,Some(&owner),"PUT",&uri,json!({"source_digest":selected["source_digest"],"title":"Upload original page","content":"Saved exact original page"})).await;
    ok(
        s,
        Some(&owner),
        "POST",
        &format!("{uri}/publish"),
        json!({}),
    )
    .await;
    let id = uuid::Uuid::new_v4().to_string();
    let (code, a) = upload(s, &owner, &id, PAYLOAD).await;
    assert_eq!(code, StatusCode::CREATED, "upload response: {a}");
    let (code, b) = upload(s, &owner, &id, PAYLOAD).await;
    assert_eq!(code, StatusCode::CREATED);
    assert_eq!(a, b);
    let other = uuid::Uuid::new_v4().to_string();
    let (code, c) = upload(s, &owner, &other, PAYLOAD).await;
    assert_eq!(code, StatusCode::CREATED);
    assert_ne!(a["id"], c["id"]);
    assert_eq!(
        upload(s, &owner, &id, b"different").await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        download(s, &viewer, a["url"].as_str().unwrap()).await,
        (StatusCode::OK, PAYLOAD.to_vec())
    );
    let mut conn = s.db.acquire().await.unwrap();
    let r = crate::storage::attachment_upload::receipt(&mut conn, &id)
        .await
        .unwrap()
        .unwrap();
    let inspection =
        crate::storage::attachment_upload::inspect(&crate::storage::data_dir(), &mut conn, &id)
            .await
            .unwrap();
    assert_eq!(r.intent.request.actor_id, "upload-owner");
    assert_eq!(r.intent.request.original_name, "same.bin");
    assert_eq!(r.intent.request.mime_type, "application/octet-stream");
    assert_eq!(r.intent.byte_length, PAYLOAD.len() as i64);
    assert_eq!(r.intent.created_at, a["created_at"]);
    assert!(
        crate::storage::attachment_upload::verify(&crate::storage::data_dir(), &r, false).unwrap()
    );
    let raw = String::from_utf8(inspection.raw_receipt).unwrap();
    drop(conn);
    sqlx::query("DELETE FROM sessions WHERE user_id='upload-viewer'")
        .execute(&s.db)
        .await
        .unwrap();
    assert_ne!(
        download(s, &viewer, a["url"].as_str().unwrap()).await.0,
        StatusCode::OK
    );
    assert_ne!(
        upload(s, &viewer, &uuid::Uuid::new_v4().to_string(), PAYLOAD)
            .await
            .0,
        StatusCode::CREATED
    );
    let refused = uuid::Uuid::new_v4().to_string();
    assert_eq!(upload(s, &owner, &refused, &vec![0x51; 1024 * 1024 + 1]).await.0, StatusCode::BAD_REQUEST);
    let mut conn = s.db.acquire().await.unwrap();
    assert!(crate::storage::attachment_upload::receipt(&mut conn, &refused).await.unwrap().is_none());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM attachments").fetch_one(&mut *conn).await.unwrap();
    assert_eq!(count, 2);
    drop(conn);
    println!("UPLOAD_ENGINE_REAL_LOGIN_PAGE_UPLOAD_RETRY_PIN_DENIAL_PASS");
    json!({"operation":id,"other_operation":other,"owner":owner,"revoked_viewer":viewer,"response":a,"raw_receipt":raw})
}
async fn run(engine: &str) {
    let root = crate::storage::data_dir();
    assert!(root.starts_with(workspace()));
    std::fs::create_dir_all(&root).unwrap();
    let (url, schema) = if engine == "postgres" {
        let url =
            std::env::var("STARLANE_UPLOAD_ENGINE_PG_URL").expect("required owned PostgreSQL URL");
        assert!(url.starts_with("postgres://"));
        let schema = format!("upload_{}", uuid::Uuid::new_v4().simple());
        sqlx::any::install_default_drivers();
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
    let db = pool(&url, &schema).await;
    crate::db::migrate_selected(&db, crate::db::DatabaseEngine::from_url(&url).unwrap())
        .await
        .unwrap();
    seed(&db).await;
    let s = state(db, &root);
    let value = flow(&s).await;
    std::fs::write(workspace().join("expected.json"), value.to_string()).unwrap();
    s.db.close().await;
    drop(s);
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "attachment_upload_engine_fixture::fresh_upload_engine_child",
            "--ignored",
            "--nocapture",
        ])
        .env("STARLANE_UPLOAD_ENGINE_CHILD_URL", url)
        .env("STARLANE_UPLOAD_ENGINE_SCHEMA", schema)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout)
        .contains("UPLOAD_ENGINE_FRESH_OWNERSHIP_RECOVERY_PASS"));
    println!("{}", String::from_utf8_lossy(&out.stdout));
}
#[tokio::test]
#[ignore = "requires explicitly owned real SQLite workspace"]
async fn required_upload_sqlite() {
    run("sqlite").await;
}
#[tokio::test]
#[ignore = "requires explicitly owned real PostgreSQL fixture"]
async fn required_upload_postgres() {
    run("postgres").await;
}
#[tokio::test]
#[ignore = "only genuine producer parent launches this child"]
async fn fresh_upload_engine_child() {
    let root = crate::storage::data_dir();
    assert!(root.starts_with(workspace()));
    let url = std::env::var("STARLANE_UPLOAD_ENGINE_CHILD_URL").unwrap();
    let schema = std::env::var("STARLANE_UPLOAD_ENGINE_SCHEMA").unwrap();
    let db = pool(&url, &schema).await;
    let expected: Value =
        serde_json::from_slice(&std::fs::read(workspace().join("expected.json")).unwrap()).unwrap();
    crate::storage::restoration::bootstrap_gate(&root).unwrap();
    crate::storage::restoration::activate_in(&root, &db).await.unwrap();
    crate::storage::relocation::recover(&db).await.unwrap();
    crate::storage::journal::recover(&db).await.unwrap();
    crate::storage::canonical_rewrite::recover(&db).await.unwrap();
    crate::storage::publication::recover(&db).await.unwrap();
    assert_eq!(
        crate::storage::attachment_upload::recover_in(&root, &db)
            .await
            .unwrap(),
        0
    );
    let s = state(db, &root);
    let owner = expected["owner"].as_str().unwrap();
    let id = expected["operation"].as_str().unwrap();
    let (code, actual) = upload(&s, owner, id, PAYLOAD).await;
    assert_eq!(code, StatusCode::CREATED);
    assert_eq!(actual, expected["response"]);
    assert_eq!(
        download(&s, owner, actual["url"].as_str().unwrap()).await,
        (StatusCode::OK, PAYLOAD.to_vec())
    );
    assert_ne!(
        download(
            &s,
            expected["revoked_viewer"].as_str().unwrap(),
            actual["url"].as_str().unwrap()
        )
        .await
        .0,
        StatusCode::OK
    );
    let mut conn = s.db.acquire().await.unwrap();
    let inspection = crate::storage::attachment_upload::inspect(&root, &mut conn, id)
        .await
        .unwrap();
    assert_eq!(
        String::from_utf8(inspection.raw_receipt).unwrap(),
        expected["raw_receipt"]
    );
    drop(conn);
    s.db.close().await;
    println!("UPLOAD_ENGINE_FRESH_OWNERSHIP_RECOVERY_PASS");
}
