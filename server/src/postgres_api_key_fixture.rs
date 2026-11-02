//! Required real password session producers on both migrated engines.
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{AnyPool, Column, Connection, Row};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};
use tower::ServiceExt;
const JWT: &str = "owned-password-fixture-secret";
const PASSWORD: &str = "owned-fixture-password-not-a-real-account";
fn workspace() -> PathBuf {
    PathBuf::from(
        std::env::var("STARLANE_APIKEY_WORKSPACE").expect("required owned password workspace"),
    )
}
async fn pool(url: &str, schema: &str) -> AnyPool {
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
fn state(db: AnyPool, root: &Path) -> crate::AppState {
    crate::AppState {
        db,
        jwt_secret: JWT.into(),
        base_url: "http://owned.invalid".into(),
        search_index: crate::search::SearchIndex::new(root.join("search_index")).unwrap(),
        oauth_cache: crate::auth::routes::OAuthStateCache::new(),
        ws_rooms: crate::sync::WsRooms::default(),
    }
}

async fn call(
    s: &crate::AppState,
    token: Option<&str>,
    method: &str,
    uri: &str,
    body: Value,
) -> (StatusCode, Value) {
    println!("APIKEY_STEP {method} {uri}");
    let mut q = Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json");
    if let Some(t) = token {
        if let Some(key) = t.strip_prefix("key:") {
            q = q.header("X-API-Key", key);
        } else {
            q = q.header("Authorization", format!("Bearer {t}"));
        }
    }
    let mut req = q.body(Body::from(body.to_string())).unwrap();
    req.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:43123".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let r = crate::api_routes()
        .with_state(s.clone())
        .oneshot(req)
        .await
        .unwrap();
    let code = r.status();
    let bytes = to_bytes(r.into_body(), 4 * 1024 * 1024).await.unwrap();
    (
        code,
        if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap_or(json!({"non_json":true}))
        },
    )
}
async fn ok(
    s: &crate::AppState,
    token: Option<&str>,
    method: &str,
    uri: &str,
    body: Value,
) -> Value {
    let (code, v) = call(s, token, method, uri, body).await;
    assert!(
        code.is_success(),
        "actual {method} {uri} status={code}; response suppressed to protect credentials"
    );
    v
}
fn access(v: &Value) -> &str {
    v["access_token"].as_str().unwrap()
}
fn refresh(v: &Value) -> &str {
    v["refresh_token"].as_str().unwrap()
}
fn sid(v: &Value) -> String {
    crate::auth::jwt::validate_token(access(v), JWT, crate::auth::jwt::TokenType::Access)
        .unwrap()
        .sid
}
async fn snapshot(db: &AnyPool, tables: &[&str]) -> BTreeMap<String, Vec<Value>> {
    let mut out = BTreeMap::new();
    for table in tables {
        let mut values = Vec::new();
        for row in sqlx::query(&format!("SELECT * FROM {table}"))
            .fetch_all(db)
            .await
            .unwrap()
        {
            let mut cells = BTreeMap::new();
            for col in row.columns() {
                let n = col.name();
                let v = if let Ok(v) = row.try_get::<Option<String>, _>(n) {
                    v.map(|v| json!(["text", v])).unwrap_or(json!(["null"]))
                } else if let Ok(v) = row.try_get::<Option<i64>, _>(n) {
                    v.map(|v| json!(["integer", v.to_string()]))
                        .unwrap_or(json!(["null"]))
                } else if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(n) {
                    v.map(|v| json!(["blob", hex::encode(v)]))
                        .unwrap_or(json!(["null"]))
                } else {
                    panic!("unsupported cell {table}.{n}")
                };
                cells.insert(n.to_string(), v);
            }
            values.push(serde_json::to_value(cells).unwrap());
        }
        values.sort_by_cached_key(Value::to_string);
        out.insert(table.to_string(), values);
    }
    out
}
async fn denied(s: &crate::AppState, token: Option<&str>, method: &str, uri: &str, body: Value) {
    let before = snapshot(&s.db, &["users", "sessions"]).await;
    let files = bytes(&crate::storage::data_dir());
    let (code, _) = call(s, token, method, uri, body).await;
    assert_eq!(
        code,
        StatusCode::UNAUTHORIZED,
        "actual denial {method} {uri}"
    );
    assert!(
        before == snapshot(&s.db, &["users", "sessions"]).await,
        "denied request changed user/session rows"
    );
    assert_eq!(
        files,
        bytes(&crate::storage::data_dir()),
        "denied request changed owned bytes"
    );
}
fn login_body(email: &str, password: &str) -> Value {
    json!({"email":email,"password":password,"device_name":"Owned browser","session_kind":"browser","client_platform":"web"})
}
async fn login(s: &crate::AppState) -> Value {
    ok(
        s,
        None,
        "POST",
        "/auth/login",
        login_body("owner@password.invalid", PASSWORD),
    )
    .await
}
async fn seed(db: &AnyPool) {
    let hash = crate::auth::password::hash_password(PASSWORD).unwrap();
    for (id, email, active, hash) in [
        ("password-owner", "owner@password.invalid", 1, Some(hash)),
        (
            "password-inactive",
            "inactive@password.invalid",
            0,
            Some(crate::auth::password::hash_password(PASSWORD).unwrap()),
        ),
        ("password-provider", "provider@password.invalid", 1, None),
    ] {
        sqlx::query("INSERT INTO users(id,email,display_name,role,account_kind,is_active,password_hash) VALUES($1,$2,$1,'member','member',$3,$4)").bind(id).bind(email).bind(active).bind(hash).execute(db).await.unwrap();
    }
    sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('password-space','Password content','password-space','private','password-owner')").execute(db).await.unwrap();
    sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('password-member','password-space','password-owner','owner')").execute(db).await.unwrap();
}

async fn key_denied(
    s: &crate::AppState,
    key: &str,
    method: &str,
    uri: &str,
    body: Value,
    code: StatusCode,
) {
    let before = snapshot(
        &s.db,
        &[
            "documents",
            "document_drafts",
            "storage_journal_receipts",
            "publication_receipts",
        ],
    )
    .await;
    let files = bytes(&crate::storage::data_dir());
    let (status, v) = call(s, Some(&format!("key:{key}")), method, uri, body).await;
    assert_eq!(
        status, code,
        "actual key denial {method} {uri}; response suppressed"
    );
    assert!(!v.to_string().contains("EXACT_APIKEY_PAGE"));
    assert_eq!(
        before,
        snapshot(
            &s.db,
            &[
                "documents",
                "document_drafts",
                "storage_journal_receipts",
                "publication_receipts"
            ]
        )
        .await
    );
    assert_eq!(files, bytes(&crate::storage::data_dir()));
}
async fn key_ok(s: &crate::AppState, key: &str, method: &str, uri: &str, body: Value) -> Value {
    ok(s, Some(&format!("key:{key}")), method, uri, body).await
}
async fn flow(s: &crate::AppState) -> Value {
    let jwt = login(s).await;
    let page=ok(s,Some(access(&jwt)),"POST","/spaces/password-space/documents",json!({"source_id":"apikey-page","title":"Key exact page","content":"EXACT_APIKEY_PAGE Ω","property_values":[]})).await;
    ok(
        s,
        Some(access(&jwt)),
        "POST",
        &format!(
            "/spaces/password-space/documents/apikey-page/drafts/{}/publish",
            page["draft_id"].as_str().unwrap()
        ),
        json!({}),
    )
    .await;
    let read = ok(
        s,
        Some(access(&jwt)),
        "POST",
        "/auth/api-keys",
        json!({"name":"Read key","scopes":["read"]}),
    )
    .await;
    let key = read["key"].as_str().unwrap();
    let id = read["id"].as_str().unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT key_hash FROM api_keys WHERE id=$1")
            .bind(id)
            .fetch_one(&s.db)
            .await
            .unwrap(),
        crate::auth::api_keys::hash_api_key(key)
    );
    assert_eq!(read["scopes"], json!(["read"]));
    let listing = ok(s, Some(access(&jwt)), "GET", "/auth/api-keys", Value::Null).await;
    assert_eq!(listing.as_array().unwrap().len(), 1);
    assert!(!listing.to_string().contains(key));
    assert!(key_ok(
        s,
        key,
        "GET",
        "/spaces/password-space/documents/apikey-page/content",
        Value::Null
    )
    .await["content"]
        .as_str()
        .unwrap()
        .contains("EXACT_APIKEY_PAGE Ω"));
    assert!(key_ok(
        s,
        key,
        "POST",
        "/mcp/tools/starlane.documents.get",
        json!({"input":{"space_id":"password-space","document_id":"apikey-page"}})
    )
    .await
    .to_string()
    .contains("EXACT_APIKEY_PAGE"));
    let used: Option<String> = sqlx::query_scalar("SELECT last_used_at FROM api_keys WHERE id=$1")
        .bind(id)
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert!(used.is_some());
    key_denied(
        s,
        key,
        "POST",
        "/spaces/password-space/documents",
        json!({"title":"Denied write","source_id":"denied-key","content":"deny"}),
        StatusCode::FORBIDDEN,
    )
    .await;
    key_denied(
        s,
        key,
        "GET",
        "/admin/api-keys",
        Value::Null,
        StatusCode::UNAUTHORIZED,
    )
    .await;
    let write = ok(
        s,
        Some(access(&jwt)),
        "POST",
        "/auth/api-keys",
        json!({"name":"Write key","scopes":["read","write"]}),
    )
    .await;
    let wk = write["key"].as_str().unwrap();
    let wid = write["id"].as_str().unwrap();
    let draft=key_ok(s,wk,"POST","/spaces/password-space/documents",json!({"source_id":"apikey-created","title":"Created by key","content":"APIKEY_WRITER Ω","property_values":[]})).await;
    assert!(draft["draft_id"].is_string());
    let creator: String = sqlx::query_scalar("SELECT owner_id FROM document_drafts WHERE id=$1")
        .bind(draft["draft_id"].as_str().unwrap())
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(creator, "password-owner");
    let page_creator: String = sqlx::query_scalar("SELECT d.created_by FROM documents d JOIN document_drafts dr ON dr.document_id=d.id WHERE dr.id=$1")
        .bind(draft["draft_id"].as_str().unwrap())
        .fetch_one(&s.db).await.unwrap();
    assert_eq!(page_creator, "password-owner");
    // Deliberately change current stored policy; no fixture-invented key or session.
    sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id=$1")
        .bind(wid)
        .execute(&s.db)
        .await
        .unwrap();
    key_denied(
        s,
        wk,
        "POST",
        "/spaces/password-space/documents",
        json!({"source_id":"denied-changed","title":"Denied"}),
        StatusCode::FORBIDDEN,
    )
    .await;
    sqlx::query("UPDATE users SET is_active=0 WHERE id='password-owner'")
        .execute(&s.db)
        .await
        .unwrap();
    key_denied(
        s,
        key,
        "GET",
        "/spaces/password-space/documents/apikey-page/content",
        Value::Null,
        StatusCode::UNAUTHORIZED,
    )
    .await;
    sqlx::query("UPDATE users SET is_active=1 WHERE id='password-owner'")
        .execute(&s.db)
        .await
        .unwrap();
    sqlx::query("UPDATE api_keys SET expires_at='2000-01-01T00:00:00Z' WHERE id=$1")
        .bind(wid)
        .execute(&s.db)
        .await
        .unwrap();
    key_denied(
        s,
        wk,
        "GET",
        "/spaces/password-space/documents/apikey-page/content",
        Value::Null,
        StatusCode::UNAUTHORIZED,
    )
    .await;
    ok(
        s,
        Some(access(&jwt)),
        "DELETE",
        &format!("/auth/api-keys/{wid}"),
        Value::Null,
    )
    .await;
    key_denied(
        s,
        wk,
        "GET",
        "/spaces/password-space/documents/apikey-page/content",
        Value::Null,
        StatusCode::UNAUTHORIZED,
    )
    .await;
    // Current human admin role remains actual policy; service producer uses its JWT.
    sqlx::query("UPDATE users SET role='admin' WHERE id='password-owner'")
        .execute(&s.db)
        .await
        .unwrap();
    let admin = login(s).await;
    key_denied(
        s,
        key,
        "GET",
        "/admin/api-keys",
        Value::Null,
        StatusCode::FORBIDDEN,
    )
    .await;
    let service = ok(
        s,
        Some(access(&admin)),
        "POST",
        "/admin/api-keys",
        json!({"name":"Owned service","scopes":["admin"]}),
    )
    .await;
    let sk = service["key"].as_str().unwrap();
    let si = service["id"].as_str().unwrap();
    assert_eq!(service["is_service_account"], true);
    let owner: Option<String> = sqlx::query_scalar("SELECT user_id FROM api_keys WHERE id=$1")
        .bind(si)
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert!(owner.is_none());
    assert_eq!(
        key_ok(s, sk, "GET", "/admin/api-keys", Value::Null)
            .await
            .as_array()
            .unwrap()
            .len(),
        1
    );
    sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id=$1")
        .bind(si)
        .execute(&s.db)
        .await
        .unwrap();
    key_denied(
        s,
        sk,
        "GET",
        "/admin/api-keys",
        Value::Null,
        StatusCode::FORBIDDEN,
    )
    .await;
    sqlx::query("UPDATE api_keys SET scopes='[\"admin\"]' WHERE id=$1")
        .bind(si)
        .execute(&s.db)
        .await
        .unwrap();
    ok(
        s,
        Some(access(&admin)),
        "DELETE",
        &format!("/admin/api-keys/{si}"),
        Value::Null,
    )
    .await;
    key_denied(
        s,
        sk,
        "GET",
        "/admin/api-keys",
        Value::Null,
        StatusCode::UNAUTHORIZED,
    )
    .await;
    println!("APIKEY_REAL_PRODUCER_SCOPE_CONSUMER_PASS");
    json!({"read":read,"revoked":write,"service":service,"files":bytes(&crate::storage::data_dir()),"domain":snapshot(&s.db,&["documents","document_drafts","storage_journal_receipts","publication_receipts"]).await})
}
fn bytes(root: &Path) -> BTreeMap<String, String> {
    fn walk(root: &Path, p: &Path, out: &mut BTreeMap<String, String>) {
        let m = std::fs::symlink_metadata(p).unwrap();
        assert!(!m.file_type().is_symlink());
        if m.is_dir() {
            for e in std::fs::read_dir(p).unwrap() {
                walk(root, &e.unwrap().path(), out);
            }
        } else {
            assert!(m.is_file());
            let mut f = std::fs::File::open(p).unwrap();
            let mut h = Sha256::new();
            let mut b = [0u8; 65536];
            loop {
                let n = f.read(&mut b).unwrap();
                if n == 0 {
                    break;
                }
                h.update(&b[..n]);
            }
            out.insert(
                p.strip_prefix(root).unwrap().to_string_lossy().into(),
                hex::encode(h.finalize()),
            );
        }
    }
    let mut out = BTreeMap::new();
    for n in [
        "spaces",
        ".storage-journal",
        ".publication-journal",
        ".canonical-rewrite-journal",
        ".move-relocation-journal",
    ] {
        let p = root.join(n);
        if p.exists() {
            walk(root, &p, &mut out);
        }
    }
    out
}

async fn run(engine: &str) {
    let root = crate::storage::data_dir();
    assert!(root.starts_with(workspace()));
    std::fs::create_dir_all(&root).unwrap();
    let (url, schema) = if engine == "postgres" {
        let url = std::env::var("STARLANE_APIKEY_PG_URL").expect("required owned PostgreSQL URL");
        assert!(url.starts_with("postgres://"));
        sqlx::any::install_default_drivers();
        let schema = format!("apikey_{}", uuid::Uuid::new_v4().simple());
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
    let e = flow(&s).await;
    std::fs::write(workspace().join("expected.json"), e.to_string()).unwrap();
    s.db.close().await;
    drop(s);
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "postgres_api_key_fixture::fresh_api_key_child",
            "--ignored",
            "--exact",
            "--nocapture",
        ])
        .env("STARLANE_APIKEY_CHILD_URL", url)
        .env("STARLANE_APIKEY_SCHEMA", schema)
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    println!(
        "APIKEY_FRESH_CHILD {}",
        String::from_utf8_lossy(&child.stdout)
    );
}

#[tokio::test]
#[ignore = "required owned dual-engine key orchestration"]
async fn required_postgres_api_key() {
    run("postgres").await;
}
#[tokio::test]
#[ignore = "required owned dual-engine key orchestration"]
async fn required_sqlite_api_key() {
    run("sqlite").await;
}
#[tokio::test]
#[ignore = "only genuine parent launches child"]
async fn fresh_api_key_child() {
    let url = std::env::var("STARLANE_APIKEY_CHILD_URL").expect("required genuine parent URL");
    let schema = std::env::var("STARLANE_APIKEY_SCHEMA").expect("required genuine schema");
    let root = crate::storage::data_dir();
    assert!(root.starts_with(workspace()));
    let e: Value =
        serde_json::from_slice(&std::fs::read(workspace().join("expected.json")).unwrap()).unwrap();
    let recovery_started = chrono::Utc::now();
    let db = pool(&url, &schema).await;
    crate::storage::restoration::bootstrap_gate(&root).unwrap();
    crate::storage::restoration::activate_in(&root, &db)
        .await
        .unwrap();
    crate::storage::relocation::recover(&db).await.unwrap();
    crate::storage::journal::recover(&db).await.unwrap();
    crate::storage::canonical_rewrite::recover(&db)
        .await
        .unwrap();
    crate::storage::publication::recover(&db).await.unwrap();
    crate::storage::attachment_upload::recover_in(&root, &db)
        .await
        .unwrap();
    let recovery_finished = chrono::Utc::now();
    let s = state(db, &root);
    let actual_domain = serde_json::to_value(snapshot(&s.db, &["documents", "document_drafts", "storage_journal_receipts", "publication_receipts"]).await).unwrap();
    let mut expected_domain = e["domain"].clone();
    // Actual publication recovery certifies its own immutable installed file.
    // Permit only the documented NULL -> verification timestamp transition.
    for expected in expected_domain["publication_receipts"].as_array_mut().unwrap() {
        let actual = actual_domain["publication_receipts"].as_array().unwrap().iter()
            .find(|row| row["op_id"] == expected["op_id"]).unwrap();
        if expected["verified_at"] == json!(["null"]) {
            assert_eq!(actual["verified_at"][0], "text");
            let verified = chrono::DateTime::parse_from_rfc3339(actual["verified_at"][1].as_str().unwrap()).unwrap().with_timezone(&chrono::Utc);
            assert!(verified >= recovery_started && verified <= recovery_finished);
            expected["verified_at"] = actual["verified_at"].clone();
        }
    }
    assert_eq!(actual_domain, expected_domain);
    let key = e["read"]["key"].as_str().unwrap();
    assert!(key_ok(
        &s,
        key,
        "POST",
        "/mcp/tools/starlane.documents.get",
        json!({"input":{"space_id":"password-space","document_id":"apikey-page"}})
    )
    .await
    .to_string()
    .contains("EXACT_APIKEY_PAGE"));
    key_denied(
        &s,
        e["revoked"]["key"].as_str().unwrap(),
        "GET",
        "/auth/me",
        Value::Null,
        StatusCode::UNAUTHORIZED,
    )
    .await;
    key_denied(
        &s,
        e["service"]["key"].as_str().unwrap(),
        "GET",
        "/admin/api-keys",
        Value::Null,
        StatusCode::UNAUTHORIZED,
    )
    .await;
    let jwt = login(&s).await;
    ok(
        &s,
        Some(access(&jwt)),
        "DELETE",
        &format!("/auth/api-keys/{}", e["read"]["id"].as_str().unwrap()),
        Value::Null,
    )
    .await;
    key_denied(
        &s,
        key,
        "GET",
        "/spaces/password-space/documents/apikey-page/content",
        Value::Null,
        StatusCode::UNAUTHORIZED,
    )
    .await;
    assert_eq!(serde_json::to_value(bytes(&root)).unwrap(), e["files"]);
    s.db.close().await;
    drop(s);
    println!("APIKEY_GENUINE_FRESH_RECOVERY_PASS");
}
