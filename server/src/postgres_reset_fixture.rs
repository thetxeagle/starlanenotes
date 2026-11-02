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
        std::env::var("STARLANE_RESET_WORKSPACE").expect("required owned reset workspace"),
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
    println!("RESET_STEP {method} {uri}");
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

const NEW: &str = "owned-reset-new-password";
async fn request_captured(
    s: &crate::AppState,
    email: &str,
    delivery_failure: bool,
) -> (StatusCode, Value, Vec<(String, String, String)>) {
    let capture = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let save = capture.clone();
    let db = s.db.clone();
    let callback: Box<
        dyn FnMut(
                &str,
                &str,
                &str,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<bool, anyhow::Error>> + Send>,
            > + Send,
    > = Box::new(move |to, subject, body| {
        save.lock()
            .unwrap()
            .push((to.into(), subject.into(), body.into()));
        let hash = hex::encode(Sha256::digest(
            body.split("/reset-password?token=")
                .nth(1)
                .unwrap()
                .lines()
                .next()
                .unwrap()
                .as_bytes(),
        ));
        let db = db.clone();
        Box::pin(async move {
            if delivery_failure {
                let count=sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM password_reset_tokens WHERE token_hash=$1 AND used_at IS NULL").bind(hash).fetch_one(&db).await.unwrap();
                assert_eq!(
                    count, 1,
                    "actual token must already be committed when mail delivery is attempted"
                );
                println!("RESET_DELIVERY_ERROR_COMMITTED_TOKEN_PASS");
                Err(anyhow::anyhow!("owned delivery failure without network"))
            } else {
                Ok(true)
            }
        })
    });
    let (code, v) = crate::notifications::RESET_EMAIL_CAPTURE
        .scope(
            std::cell::RefCell::new(Some(callback)),
            call(
                s,
                None,
                "POST",
                "/auth/password-reset/request",
                json!({"email":email}),
            ),
        )
        .await;
    let mail = capture.lock().unwrap().clone();
    (code, v, mail)
}
async fn request(
    s: &crate::AppState,
    email: &str,
) -> (StatusCode, Value, Vec<(String, String, String)>) {
    request_captured(s, email, false).await
}
async fn token(s: &crate::AppState) -> String {
    let (code, _, mail) = request(s, " OWNER@PASSWORD.INVALID ").await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(mail.len(), 1);
    assert_eq!(mail[0].0, "owner@password.invalid");
    assert_eq!(mail[0].1, "Reset your Starlane Notes password");
    let token = mail[0]
        .2
        .split("/reset-password?token=")
        .nth(1)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .to_string();
    assert!(token.len() >= 32);
    let hash = hex::encode(Sha256::digest(token.as_bytes()));
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM password_reset_tokens WHERE token_hash=$1 AND used_at IS NULL",
    )
    .bind(hash)
    .fetch_one(&s.db)
    .await
    .unwrap();
    assert_eq!(count, 1);
    token
}
async fn confirm(s: &crate::AppState, t: &str, pw: &str) -> StatusCode {
    call(
        s,
        None,
        "POST",
        "/auth/password-reset/confirm",
        json!({"token":t,"password":pw}),
    )
    .await
    .0
}
async fn domain(s: &crate::AppState) -> BTreeMap<String, Vec<Value>> {
    snapshot(&s.db, &["users", "sessions", "password_reset_tokens"]).await
}
async fn fail_trigger(s: &crate::AppState, engine: &str, target: &str, event: &str) {
    if engine == "sqlite" {
        sqlx::query(&format!("CREATE TRIGGER reset_fail BEFORE {event} ON {target} BEGIN SELECT RAISE(ABORT,'owned reset failure'); END")).execute(&s.db).await.unwrap();
    } else {
        sqlx::query("CREATE FUNCTION reset_fail_fn() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'owned reset failure'; END $$").execute(&s.db).await.unwrap();
        sqlx::query(&format!("CREATE TRIGGER reset_fail BEFORE {event} ON {target} FOR EACH ROW EXECUTE FUNCTION reset_fail_fn()")).execute(&s.db).await.unwrap();
    }
}
async fn remove_trigger(s: &crate::AppState, engine: &str, target: &str) {
    let q = if engine == "sqlite" {
        "DROP TRIGGER reset_fail".into()
    } else {
        format!("DROP TRIGGER reset_fail ON {target}")
    };
    sqlx::query(&q).execute(&s.db).await.unwrap();
    if engine == "postgres" {
        sqlx::query("DROP FUNCTION reset_fail_fn()")
            .execute(&s.db)
            .await
            .unwrap();
    }
}
async fn flow(s: &crate::AppState, engine: &str, kind: &str) -> Value {
    let a = login(s).await;
    let b = login(s).await;
    let page=ok(s,Some(access(&a)),"POST","/spaces/password-space/documents",json!({"source_id":"reset-page","title":"Reset exact page","content":"EXACT_RESET_PAGE Ω","property_values":[]})).await;
    ok(
        s,
        Some(access(&a)),
        "POST",
        &format!(
            "/spaces/password-space/documents/reset-page/drafts/{}/publish",
            page["draft_id"].as_str().unwrap()
        ),
        json!({}),
    )
    .await;
    let files = bytes(&crate::storage::data_dir());
    assert!(!files.is_empty());
    let content = snapshot(
        &s.db,
        &[
            "documents",
            "document_drafts",
            "storage_journal_receipts",
            "publication_receipts",
        ],
    )
    .await;
    let t = token(s).await;
    let mut replay_token = t.clone();
    let mut chosen = NEW.to_string();
    match kind {
        "requestrollback" => {
            let before = domain(s).await;
            fail_trigger(s, engine, "password_reset_tokens", "INSERT").await;
            let (code, _, mail) = request(s, "owner@password.invalid").await;
            assert_eq!(code, StatusCode::INTERNAL_SERVER_ERROR);
            assert!(mail.is_empty());
            let after = domain(s).await;
            println!("RESET_REQUEST_FAILURE_ROLLBACK exact={}", before == after);
            assert!(
                before == after,
                "request failed after old-token invalidation: original reset state must survive"
            );
            remove_trigger(s, engine, "password_reset_tokens").await;
            assert_eq!(confirm(s, &t, NEW).await, StatusCode::NO_CONTENT);
        }
        "confirmrollback" => {
            for (target, event) in [
                ("password_reset_tokens", "UPDATE OF used_at"),
                ("users", "UPDATE OF password_hash"),
                ("sessions", "DELETE"),
            ] {
                let before = domain(s).await;
                fail_trigger(s, engine, target, event).await;
                assert_eq!(confirm(s, &t, NEW).await, StatusCode::INTERNAL_SERVER_ERROR);
                let after = domain(s).await;
                println!(
                    "RESET_CONFIRM_FAILURE_ROLLBACK stage={target} exact={}",
                    before == after
                );
                assert!(
                    before == after,
                    "failed constituent write changed reset domain"
                );
                remove_trigger(s, engine, target).await;
            }
            assert_eq!(confirm(s, &t, NEW).await, StatusCode::NO_CONTENT);
        }
        "expirywait" | "inactivewait" => {
            let original = domain(s).await;
            let th = hex::encode(Sha256::digest(t.as_bytes()));
            let expiry: String = sqlx::query_scalar(
                "SELECT expires_at FROM password_reset_tokens WHERE token_hash=$1",
            )
            .bind(&th)
            .fetch_one(&s.db)
            .await
            .unwrap();
            let mut held = s.db.begin().await.unwrap();
            crate::spaces::mutation_guards::lock_audience_for_mutation(&mut held)
                .await
                .unwrap();
            if kind == "expirywait" {
                sqlx::query("UPDATE password_reset_tokens SET expires_at='2000-01-01T00:00:00Z' WHERE token_hash=$1").bind(&th).execute(&mut *held).await.unwrap();
            } else {
                sqlx::query("UPDATE users SET is_active=0 WHERE id='password-owner'")
                    .execute(&mut *held)
                    .await
                    .unwrap();
            }
            let signal = std::sync::Arc::new(tokio::sync::Notify::new());
            let observed = signal.clone();
            let app = s.clone();
            let raw = t.clone();
            let mut waiter = tokio::spawn(async move {
                let f: Box<dyn FnOnce() + Send> = Box::new(move || observed.notify_one());
                crate::auth::routes::RESET_CONFIRM_FENCE_SIGNAL
                    .scope(std::cell::RefCell::new(Some(f)), confirm(&app, &raw, NEW))
                    .await
            });
            tokio::time::timeout(std::time::Duration::from_secs(5), signal.notified())
                .await
                .unwrap();
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(50), &mut waiter)
                    .await
                    .is_err(),
                "must await actual held audience fence"
            );
            held.commit().await.unwrap();
            assert_eq!(waiter.await.unwrap(), StatusCode::BAD_REQUEST);
            let current = domain(s).await;
            assert!(crate::auth::password::verify_password(
                PASSWORD,
                &sqlx::query_scalar::<_, String>(
                    "SELECT password_hash FROM users WHERE id='password-owner'"
                )
                .fetch_one(&s.db)
                .await
                .unwrap()
            )
            .unwrap());
            assert_eq!(current["sessions"], original["sessions"]);
            let hashrow = current["password_reset_tokens"]
                .iter()
                .find(|r| r["token_hash"][1] == th)
                .unwrap();
            assert_eq!(hashrow["used_at"], json!(["null"]));
            if kind == "expirywait" {
                sqlx::query("UPDATE password_reset_tokens SET expires_at=$1 WHERE token_hash=$2")
                    .bind(expiry)
                    .bind(th)
                    .execute(&s.db)
                    .await
                    .unwrap();
            } else {
                sqlx::query("UPDATE users SET is_active=1 WHERE id='password-owner'")
                    .execute(&s.db)
                    .await
                    .unwrap();
            }
            assert!(
                domain(s).await == original,
                "restore only owned injected lifecycle/expiry fault, all other domain cells exact"
            );
            assert_eq!(confirm(s, &t, NEW).await, StatusCode::NO_CONTENT);
            println!("RESET_REAL_FENCE_RECHECK_PASS {kind}");
        }
        "concurrency" => {
            let (left, right) = tokio::join!(
                confirm(s, &t, NEW),
                confirm(s, &t, "owned-reset-race-alternative")
            );
            println!("RESET_CONCURRENT_CONFIRM statuses={left}/{right}");
            assert!(
                (left == StatusCode::NO_CONTENT && right == StatusCode::BAD_REQUEST)
                    || (right == StatusCode::NO_CONTENT && left == StatusCode::BAD_REQUEST)
            );
            if right == StatusCode::NO_CONTENT {
                chosen = "owned-reset-race-alternative".into();
            }
        }
        "flow" => {
            let (code, provider_message, mail) =
                request_captured(s, "provider@password.invalid", true).await;
            assert_eq!(code, StatusCode::OK);
            assert_eq!(mail.len(), 1);
            assert_eq!(mail[0].0, "provider@password.invalid");
            let pt = mail[0]
                .2
                .split("/reset-password?token=")
                .nth(1)
                .unwrap()
                .lines()
                .next()
                .unwrap();
            assert_eq!(confirm(s, pt, NEW).await, StatusCode::NO_CONTENT);
            let provider = ok(
                s,
                None,
                "POST",
                "/auth/login",
                login_body("provider@password.invalid", NEW),
            )
            .await;
            assert_eq!(
                ok(s, Some(access(&provider)), "GET", "/auth/me", Value::Null).await["id"],
                "password-provider"
            );
            ok(
                s,
                Some(access(&provider)),
                "POST",
                "/auth/logout",
                json!({}),
            )
            .await;
            let owner_sessions: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id='password-owner'")
                    .fetch_one(&s.db)
                    .await
                    .unwrap();
            assert_eq!(owner_sessions, 2);
            let owner_open:i64=sqlx::query_scalar("SELECT COUNT(*) FROM password_reset_tokens WHERE token_hash=$1 AND used_at IS NULL").bind(hex::encode(Sha256::digest(t.as_bytes()))).fetch_one(&s.db).await.unwrap();
            assert_eq!(owner_open, 1);
            println!("RESET_NULL_CREDENTIAL_POLICY_PASS");
            let before = domain(s).await;
            let (code, v, mail) = request(s, "missing@password.invalid").await;
            assert_eq!(code, StatusCode::OK);
            assert!(mail.is_empty());
            let (code, w, mail) = request(s, "inactive@password.invalid").await;
            assert_eq!(code, StatusCode::OK);
            assert_eq!(v, w);
            assert_eq!(v, provider_message);
            assert!(mail.is_empty());
            assert!(before == domain(s).await);
            assert_eq!(
                confirm(s, "not-a-valid-token", NEW).await,
                StatusCode::BAD_REQUEST
            );
            assert!(before == domain(s).await);
            let t2 = token(s).await;
            replay_token = t2.clone();
            assert_eq!(confirm(s, &t, NEW).await, StatusCode::BAD_REQUEST);
            assert_eq!(confirm(s, &t2, NEW).await, StatusCode::NO_CONTENT);
        }
        _ => panic!(),
    }
    let done = domain(s).await;
    assert_eq!(
        confirm(s, &replay_token, NEW).await,
        StatusCode::BAD_REQUEST
    );
    assert!(
        done == domain(s).await,
        "lost/consumed confirmation replay mutated domain"
    );
    denied(s, Some(access(&a)), "GET", "/auth/me", Value::Null).await;
    denied(s, Some(access(&b)), "GET", "/auth/me", Value::Null).await;
    denied(
        s,
        None,
        "POST",
        "/auth/login",
        login_body("owner@password.invalid", PASSWORD),
    )
    .await;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let hash: String =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id='password-owner'")
            .fetch_one(&s.db)
            .await
            .unwrap();
    assert!(crate::auth::password::verify_password(&chosen, &hash).unwrap());
    assert!(
        content
            == snapshot(
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
    println!("RESET_ACTUAL_PRODUCER_ATOMIC_PASS {kind}");
    json!({"password":chosen,"revoked":a,"token":replay_token,"domain":done,"files":files})
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

async fn run(engine: &str, kind: &str) {
    let root = crate::storage::data_dir();
    assert!(root.starts_with(workspace()));
    std::fs::create_dir_all(&root).unwrap();
    let (url, schema) = if engine == "postgres" {
        let url = std::env::var("STARLANE_RESET_PG_URL").expect("required owned PostgreSQL URL");
        assert!(url.starts_with("postgres://"));
        sqlx::any::install_default_drivers();
        let schema = format!("reset_{}", uuid::Uuid::new_v4().simple());
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
    let e = flow(&s, engine, kind).await;
    std::fs::write(workspace().join("expected.json"), e.to_string()).unwrap();
    s.db.close().await;
    drop(s);
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "postgres_reset_fixture::fresh_reset_child",
            "--ignored",
            "--exact",
            "--nocapture",
        ])
        .env("STARLANE_RESET_CHILD_URL", url)
        .env("STARLANE_RESET_SCHEMA", schema)
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&child.stdout),
        String::from_utf8_lossy(&child.stderr)
    );
    println!(
        "RESET_FRESH_CHILD {}",
        String::from_utf8_lossy(&child.stdout)
    );
}
macro_rules! case {
    ($name:ident,$engine:literal,$kind:literal) => {
        #[tokio::test]
        #[ignore = "required owned reset orchestration"]
        async fn $name() {
            run($engine, $kind).await;
        }
    };
}
case!(required_postgres_flow, "postgres", "flow");
case!(required_sqlite_flow, "sqlite", "flow");
case!(
    required_postgres_requestrollback,
    "postgres",
    "requestrollback"
);
case!(required_sqlite_requestrollback, "sqlite", "requestrollback");
case!(
    required_postgres_confirmrollback,
    "postgres",
    "confirmrollback"
);
case!(required_sqlite_confirmrollback, "sqlite", "confirmrollback");
case!(required_postgres_concurrency, "postgres", "concurrency");
case!(required_sqlite_concurrency, "sqlite", "concurrency");
case!(required_postgres_expirywait, "postgres", "expirywait");
case!(required_sqlite_expirywait, "sqlite", "expirywait");
case!(required_postgres_inactivewait, "postgres", "inactivewait");
case!(required_sqlite_inactivewait, "sqlite", "inactivewait");
#[tokio::test]
#[ignore = "only genuine parent launches child"]
async fn fresh_reset_child() {
    let url = std::env::var("STARLANE_RESET_CHILD_URL").expect("required genuine parent URL");
    let schema = std::env::var("STARLANE_RESET_SCHEMA").expect("required genuine parent schema");
    let root = crate::storage::data_dir();
    assert!(root.starts_with(workspace()));
    let e: Value =
        serde_json::from_slice(&std::fs::read(workspace().join("expected.json")).unwrap()).unwrap();
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
    let s = state(db, &root);
    assert_eq!(serde_json::to_value(domain(&s).await).unwrap(), e["domain"]);
    assert_eq!(
        confirm(&s, e["token"].as_str().unwrap(), NEW).await,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(serde_json::to_value(domain(&s).await).unwrap(), e["domain"]);
    denied(
        &s,
        Some(access(&e["revoked"])),
        "GET",
        "/auth/me",
        Value::Null,
    )
    .await;
    let n = ok(
        &s,
        None,
        "POST",
        "/auth/login",
        login_body("owner@password.invalid", e["password"].as_str().unwrap()),
    )
    .await;
    assert!(ok(
        &s,
        Some(access(&n)),
        "GET",
        "/spaces/password-space/documents/reset-page/content",
        Value::Null
    )
    .await["content"]
        .as_str()
        .unwrap()
        .contains("EXACT_RESET_PAGE Ω"));
    assert_eq!(serde_json::to_value(bytes(&root)).unwrap(), e["files"]);
    s.db.close().await;
    drop(s);
    println!("RESET_ACTUAL_FRESH_RESTART_PASS");
}
