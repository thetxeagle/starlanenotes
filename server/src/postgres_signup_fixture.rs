//! Required real first-user/signup/config producers on both migrated engines.
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
        std::env::var("STARLANE_SIGNUP_WORKSPACE").expect("required owned signup workspace"),
    )
}
async fn pool(url: &str, schema: &str) -> AnyPool {
    sqlx::any::install_default_drivers();
    let engine = crate::db::DatabaseEngine::from_url(url).unwrap();
    let schema = schema.to_owned();
    sqlx::any::AnyPoolOptions::new()
        .max_connections(3)
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
    println!("SIGNUP_STEP {method} {uri}");
    let mut q = Request::builder()
        .method(method)
        .uri(uri)
        .header("Content-Type", "application/json");
    if let Some(t) = token {
        q = q.header("Authorization", format!("Bearer {t}"));
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
fn login_body(email: &str, password: &str) -> Value {
    json!({"email":email,"password":password,"device_name":"Owned browser","session_kind":"browser","client_platform":"web"})
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

const ADMIN_EMAIL: &str = "first@signup.invalid";
const MEMBER_EMAIL: &str = "member@signup.invalid";
fn body(email: &str) -> Value {
    json!({"email":email,"display_name":"Owned signup user","password":PASSWORD,"device_name":"Owned signup browser","session_kind":"browser","client_platform":"web"})
}
async fn reg(s: &crate::AppState, email: &str) -> (StatusCode, Value) {
    call(s, None, "POST", "/auth/register", body(email)).await
}
async fn created(s: &crate::AppState, email: &str) -> Value {
    let (c, v) = reg(s, email).await;
    assert_eq!(
        c,
        StatusCode::CREATED,
        "actual signup producer failed; response suppressed"
    );
    v
}
const DOMAIN: &[&str] = &[
    "users",
    "sessions",
    "spaces",
    "space_members",
    "system_config",
    "audit_log",
    "documents",
    "document_drafts",
    "storage_journal_receipts",
];
async fn domain(s: &crate::AppState) -> BTreeMap<String, Vec<Value>> {
    snapshot(&s.db, DOMAIN).await
}
async fn policy(s: &crate::AppState, a: &Value, value: &str) -> StatusCode {
    call(
        s,
        Some(access(a)),
        "POST",
        "/admin/config",
        json!({"key":"allow_public_signups","value":value}),
    )
    .await
    .0
}
async fn fail_first(s: &crate::AppState, engine: &str) {
    let before = domain(s).await;
    let files = bytes(&crate::storage::data_dir());
    if engine == "sqlite" {
        sqlx::query("CREATE TRIGGER signup_fail BEFORE INSERT ON sessions BEGIN SELECT RAISE(ABORT,'owned signup failure'); END").execute(&s.db).await.unwrap();
    } else {
        sqlx::query("CREATE FUNCTION signup_fail_fn() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'owned signup failure'; END $$").execute(&s.db).await.unwrap();
        sqlx::query("CREATE TRIGGER signup_fail BEFORE INSERT ON sessions FOR EACH ROW EXECUTE FUNCTION signup_fail_fn()").execute(&s.db).await.unwrap();
    }
    assert_eq!(
        reg(s, ADMIN_EMAIL).await.0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert!(
        before == domain(s).await,
        "failed first-user session insert changed account/space/setup/audit"
    );
    assert_eq!(files, bytes(&crate::storage::data_dir()));
    if engine == "sqlite" {
        sqlx::query("DROP TRIGGER signup_fail")
            .execute(&s.db)
            .await
            .unwrap();
    } else {
        sqlx::query("DROP TRIGGER signup_fail ON sessions")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("DROP FUNCTION signup_fail_fn()")
            .execute(&s.db)
            .await
            .unwrap();
    }
    println!("SIGNUP_REAL_FIRST_ROLLBACK_PASS");
}
async fn contended_register(
    s: crate::AppState,
    email: String,
    signal: std::sync::Arc<tokio::sync::Notify>,
) -> (StatusCode, Value) {
    let f: Box<dyn FnOnce() + Send> = Box::new(move || signal.notify_one());
    crate::auth::routes::SIGNUP_BEFORE_FENCE_SIGNAL
        .scope(std::cell::RefCell::new(Some(f)), reg(&s, &email))
        .await
}
async fn first_race(s: &crate::AppState) -> Value {
    let mut held = s.db.begin().await.unwrap();
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut held)
        .await
        .unwrap();
    let n1 = std::sync::Arc::new(tokio::sync::Notify::new());
    let n2 = std::sync::Arc::new(tokio::sync::Notify::new());
    let s1 = s.clone();
    let s2 = s.clone();
    let m1 = n1.clone();
    let m2 = n2.clone();
    let mut a = tokio::spawn(contended_register(s1, "race-a@signup.invalid".into(), m1));
    let mut b = tokio::spawn(contended_register(s2, "race-b@signup.invalid".into(), m2));
    tokio::time::timeout(std::time::Duration::from_secs(5), n1.notified())
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), n2.notified())
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), &mut a)
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), &mut b)
            .await
            .is_err()
    );
    held.commit().await.unwrap();
    let a = a.await.unwrap();
    let b = b.await.unwrap();
    assert!(
        (a.0 == StatusCode::CREATED && b.0 == StatusCode::FORBIDDEN)
            || (b.0 == StatusCode::CREATED && a.0 == StatusCode::FORBIDDEN),
        "first-account race must produce exactly one admin and deny disabled subsequent signup"
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(count, 1);
    println!("SIGNUP_REAL_FIRST_SERIALIZATION_PASS");
    if a.0 == StatusCode::CREATED {
        a.1
    } else {
        b.1
    }
}
async fn wait_policy(s: &crate::AppState, a: &Value) {
    let before = snapshot(
        &s.db,
        &["users", "sessions", "spaces", "space_members", "audit_log"],
    )
    .await;
    let files = bytes(&crate::storage::data_dir());
    let mut held = s.db.begin().await.unwrap();
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut held)
        .await
        .unwrap();
    sqlx::query("UPDATE system_config SET value='false' WHERE key='allow_public_signups'")
        .execute(&mut *held)
        .await
        .unwrap();
    let n = std::sync::Arc::new(tokio::sync::Notify::new());
    let mut waiter = tokio::spawn(contended_register(
        s.clone(),
        "wait-denied@signup.invalid".into(),
        n.clone(),
    ));
    tokio::time::timeout(std::time::Duration::from_secs(5), n.notified())
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(50), &mut waiter)
            .await
            .is_err()
    );
    held.commit().await.unwrap();
    assert_eq!(waiter.await.unwrap().0, StatusCode::FORBIDDEN);
    assert!(
        before
            == snapshot(
                &s.db,
                &["users", "sessions", "spaces", "space_members", "audit_log"]
            )
            .await
    );
    assert_eq!(files, bytes(&crate::storage::data_dir()));
    assert_eq!(policy(s, a, "true").await, StatusCode::NO_CONTENT);
    println!("SIGNUP_REAL_POLICY_RECHECK_PASS");
}
async fn page(s: &crate::AppState, a: &Value, id: &str) -> String {
    let uid = a["user"]["id"].as_str().unwrap();
    let sid: String = sqlx::query_scalar("SELECT id FROM spaces WHERE created_by=$1")
        .bind(uid)
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(
        call(
            s,
            Some(access(a)),
            "PUT",
            &format!("/spaces/{sid}"),
            json!({"visibility":"private"})
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let p=ok(s,Some(access(a)),"POST",&format!("/spaces/{sid}/documents"),json!({"source_id":id,"title":"Signup exact page","content":"EXACT_SIGNUP_PAGE Ω","property_values":[]})).await;
    ok(
        s,
        Some(access(a)),
        "POST",
        &format!(
            "/spaces/{sid}/documents/{id}/drafts/{}/publish",
            p["draft_id"].as_str().unwrap()
        ),
        json!({}),
    )
    .await;
    sid
}
async fn flow(s: &crate::AppState, engine: &str, kind: &str) -> Value {
    let initial: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(
        initial, 0,
        "must exercise actual first-user signup, not seeded authority"
    );
    if kind == "firstrollback" {
        fail_first(s, engine).await;
    }
    let a = if kind == "firstrace" {
        first_race(s).await
    } else {
        created(s, ADMIN_EMAIL).await
    };
    assert_eq!(a["user"]["role"], "admin");
    assert_eq!(a["user"]["account_kind"], "member");
    let setup: String =
        sqlx::query_scalar("SELECT value FROM system_config WHERE key='setup_completed'")
            .fetch_one(&s.db)
            .await
            .unwrap();
    assert_eq!(setup, "true");
    let cfg = ok(s, None, "GET", "/auth/config", Value::Null).await;
    assert_eq!(cfg["setup_completed"], true);
    println!("SIGNUP_REAL_FIRST_ADMIN_PASS");
    let (c, _) = call(
        s,
        Some(access(&a)),
        "POST",
        "/admin/config",
        json!({"key":"site_name","value":"Owned signup site"}),
    )
    .await;
    let c2 = policy(s, &a, "true").await;
    let (c3, _) = call(
        s,
        Some(access(&a)),
        "DELETE",
        "/admin/config/owned-scratch-key",
        Value::Null,
    )
    .await;
    println!("SIGNUP_CONFIG_BASELINE statuses={c}/{c2}/{c3}");
    assert_eq!(c, StatusCode::NO_CONTENT);
    assert_eq!(c2, StatusCode::NO_CONTENT);
    assert_eq!(c3, StatusCode::NO_CONTENT);
    let cfg = ok(s, None, "GET", "/auth/config", Value::Null).await;
    assert_eq!(cfg["allow_public_signups"], true);
    assert_eq!(cfg["site_name"], "Owned signup site");
    if kind == "policywait" {
        wait_policy(s, &a).await;
    }
    let member = created(s, MEMBER_EMAIL).await;
    assert_eq!(member["user"]["role"], "member");
    assert_ne!(member["user"]["id"], a["user"]["id"]);
    let b = domain(s).await;
    let files = bytes(&crate::storage::data_dir());
    assert_eq!(reg(s, MEMBER_EMAIL).await.0, StatusCode::CONFLICT);
    assert!(b == domain(s).await);
    assert_eq!(files, bytes(&crate::storage::data_dir()));
    assert_eq!(policy(s, &a, "false").await, StatusCode::NO_CONTENT);
    let b = domain(s).await;
    assert_eq!(
        reg(s, "disabled@signup.invalid").await.0,
        StatusCode::FORBIDDEN
    );
    assert!(b == domain(s).await);
    let cfg = ok(s, None, "GET", "/auth/config", Value::Null).await;
    assert_eq!(cfg["allow_public_signups"], false);
    let unchanged = snapshot(
        &s.db,
        &["users", "spaces", "space_members", "system_config"],
    )
    .await;
    let files = bytes(&crate::storage::data_dir());
    assert_eq!(
        call(
            s,
            Some(access(&member)),
            "POST",
            "/admin/config",
            json!({"key":"allow_public_signups","value":"true"})
        )
        .await
        .0,
        StatusCode::UNAUTHORIZED
    );
    assert!(
        unchanged
            == snapshot(
                &s.db,
                &["users", "spaces", "space_members", "system_config"]
            )
            .await
    );
    assert_eq!(files, bytes(&crate::storage::data_dir()));
    assert_eq!(
        call(
            s,
            Some(access(&a)),
            "POST",
            "/admin/config",
            json!({"key":"owned_secret","value":"fake-owned-secret"})
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    let list = ok(s, Some(access(&a)), "GET", "/admin/config", Value::Null).await;
    let row = list
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "owned_secret")
        .unwrap();
    assert_eq!(row["value"], "");
    assert_eq!(row["effective_value"], "********");
    assert_eq!(
        call(
            s,
            Some(access(&a)),
            "DELETE",
            "/admin/config/owned_secret",
            Value::Null
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        call(
            s,
            Some(access(&a)),
            "DELETE",
            "/admin/config/setup_completed",
            Value::Null
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    let admin_space = page(s, &a, "signup-admin-page").await;
    let member_space = page(s, &member, "signup-member-page").await;
    let b = snapshot(
        &s.db,
        &[
            "users",
            "spaces",
            "space_members",
            "documents",
            "document_drafts",
            "system_config",
            "storage_journal_receipts",
        ],
    )
    .await;
    let files = bytes(&crate::storage::data_dir());
    assert!(!files.is_empty());
    let (code, v) = call(
        s,
        Some(access(&member)),
        "GET",
        &format!("/spaces/{admin_space}/documents/signup-admin-page/content"),
        Value::Null,
    )
    .await;
    assert!(matches!(
        code,
        StatusCode::FORBIDDEN | StatusCode::NOT_FOUND
    ));
    assert!(!v.to_string().contains("EXACT_SIGNUP_PAGE"));
    assert!(
        b == snapshot(
            &s.db,
            &[
                "users",
                "spaces",
                "space_members",
                "documents",
                "document_drafts",
                "system_config",
                "storage_journal_receipts"
            ]
        )
        .await
    );
    assert_eq!(files, bytes(&crate::storage::data_dir()));
    println!("SIGNUP_ACTUAL_POLICY_BYTES_PASS {kind}");
    json!({"admin":a,"member":member,"admin_space":admin_space,"member_space":member_space,"stable":b,"files":files})
}
async fn run(engine: &str, kind: &str) {
    for key in [
        "ALLOW_PUBLIC_SIGNUPS",
        "SITE_NAME",
        "REQUIRE_2FA",
        "MAX_UPLOAD_SIZE_MB",
    ] {
        assert!(
            std::env::var_os(key).is_none(),
            "owned stored-config fixture must not mask policy using environment"
        );
    }
    let root = crate::storage::data_dir();
    assert!(root.starts_with(workspace()));
    let url = if engine == "postgres" {
        std::env::var("STARLANE_SIGNUP_PG_URL").expect("required owned PG URL")
    } else {
        format!(
            "sqlite://{}?mode=rwc",
            root.join("starlane-notes.db").display()
        )
    };
    std::fs::create_dir_all(&root).unwrap();
    let schema = if engine == "postgres" {
        sqlx::any::install_default_drivers();
        let name = format!("signup_{}", uuid::Uuid::new_v4().simple());
        let mut c = sqlx::AnyConnection::connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE SCHEMA {name}"))
            .execute(&mut c)
            .await
            .unwrap();
        c.close().await.unwrap();
        name
    } else {
        String::new()
    };
    let db = pool(&url, &schema).await;
    crate::db::migrate_selected(&db, crate::db::DatabaseEngine::from_url(&url).unwrap())
        .await
        .unwrap();
    let s = state(db, &root);
    let e = flow(&s, engine, kind).await;
    std::fs::write(workspace().join("expected.json"), e.to_string()).unwrap();
    s.db.close().await;
    drop(s);
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "postgres_signup_fixture::fresh_signup_child",
            "--ignored",
            "--exact",
            "--nocapture",
        ])
        .env("STARLANE_SIGNUP_CHILD_URL", url)
        .env("STARLANE_SIGNUP_SCHEMA", schema)
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    println!(
        "SIGNUP_FRESH_CHILD {}",
        String::from_utf8_lossy(&child.stdout)
    );
}
macro_rules! case {
    ($name:ident,$engine:literal,$kind:literal) => {
        #[tokio::test]
        #[ignore = "required owned migrated signup/config API orchestration"]
        async fn $name() {
            run($engine, $kind).await;
        }
    };
}
case!(required_postgres_flow, "postgres", "flow");
case!(required_sqlite_flow, "sqlite", "flow");
case!(required_postgres_firstrollback, "postgres", "firstrollback");
case!(required_sqlite_firstrollback, "sqlite", "firstrollback");
case!(required_postgres_firstrace, "postgres", "firstrace");
case!(required_sqlite_firstrace, "sqlite", "firstrace");
case!(required_postgres_policywait, "postgres", "policywait");
case!(required_sqlite_policywait, "sqlite", "policywait");
#[tokio::test]
#[ignore = "only genuine signup parent launches"]
async fn fresh_signup_child() {
    let url = std::env::var("STARLANE_SIGNUP_CHILD_URL").expect("required genuine parent URL");
    let schema = std::env::var("STARLANE_SIGNUP_SCHEMA").expect("required genuine parent schema");
    let root = crate::storage::data_dir();
    assert!(root.starts_with(workspace()));
    let e: Value =
        serde_json::from_slice(&std::fs::read(workspace().join("expected.json")).unwrap()).unwrap();
    let db = pool(&url, &schema).await;
    let s = state(db, &root);
    assert_eq!(
        serde_json::to_value(
            snapshot(
                &s.db,
                &[
                    "users",
                    "spaces",
                    "space_members",
                    "documents",
                    "document_drafts",
                    "system_config",
                    "storage_journal_receipts"
                ]
            )
            .await
        )
        .unwrap(),
        e["stable"]
    );
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
    assert_eq!(
        serde_json::to_value(
            snapshot(
                &s.db,
                &[
                    "users",
                    "spaces",
                    "space_members",
                    "documents",
                    "document_drafts",
                    "system_config",
                    "storage_journal_receipts"
                ]
            )
            .await
        )
        .unwrap(),
        e["stable"]
    );
    for (actor, key, id) in [
        ("admin", "admin_space", "signup-admin-page"),
        ("member", "member_space", "signup-member-page"),
    ] {
        let email = e[actor]["user"]["email"].as_str().unwrap();
        let auth = ok(&s, None, "POST", "/auth/login", login_body(email, PASSWORD)).await;
        assert_eq!(auth["user"]["id"], e[actor]["user"]["id"]);
        assert!(ok(
            &s,
            Some(access(&auth)),
            "GET",
            &format!(
                "/spaces/{}/documents/{id}/content",
                e[key].as_str().unwrap()
            ),
            Value::Null
        )
        .await["content"]
            .as_str()
            .unwrap()
            .contains("EXACT_SIGNUP_PAGE Ω"));
    }
    let cfg = ok(&s, None, "GET", "/auth/config", Value::Null).await;
    assert_eq!(cfg["site_name"], "Owned signup site");
    assert_eq!(cfg["allow_public_signups"], false);
    assert_eq!(cfg["setup_completed"], true);
    let before = domain(&s).await;
    assert_eq!(
        reg(&s, "fresh-denied@signup.invalid").await.0,
        StatusCode::FORBIDDEN
    );
    assert!(before == domain(&s).await);
    assert_eq!(serde_json::to_value(bytes(&root)).unwrap(), e["files"]);
    s.db.close().await;
    drop(s);
    println!("SIGNUP_ACTUAL_FRESH_RESTART_PASS");
}
