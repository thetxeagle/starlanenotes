//! Opt-in reviewed-draft transfer acceptance against actual SQLite/PostgreSQL routes.
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::{AnyPool, Connection};
use tower::ServiceExt;

const SPACE: &str = "transfer-space";
const PAGE: &str = "transfer-page";
const JWT: &str = "owned-draft-transfer-fixture-secret";
const REVIEWED: &str = "# Reviewed local draft\r\n\r\nExact body Ω\r\n\r\n<!-- starlane:custom-css -->\r\n.notice { font-weight: 700; }\r\n<!-- /starlane:custom-css -->\r\n";

fn token(actor: &str) -> String {
    crate::auth::jwt::create_token_pair_for_session(
        actor,
        &format!("{actor}@transfer.invalid"),
        "member",
        JWT,
        Some(&format!("transfer-{actor}")),
        30,
    )
    .unwrap()
    .access_token
}

async fn request(
    state: &crate::AppState,
    actor: &str,
    method: &str,
    path: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = crate::api_routes()
        .with_state(state.clone())
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {}", token(actor)))
                .body(if body.is_null() {
                    Body::empty()
                } else {
                    Body::from(body.to_string())
                })
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 8 * 1024 * 1024)
        .await
        .unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"unparsed": String::from_utf8_lossy(&bytes)}))
    };
    (status, value)
}

async fn call(
    state: &crate::AppState,
    actor: &str,
    method: &str,
    path: &str,
    body: Value,
    expected: StatusCode,
) -> Value {
    let (status, value) = request(state, actor, method, path, body).await;
    assert_eq!(status, expected, "{actor} {method} {path}: {value}");
    value
}

async fn connect(postgres: bool, schema: &str, root: &std::path::Path) -> AnyPool {
    if postgres {
        let url = std::env::var("STARLANE_CORE_PG_URL").expect("required owned PG URL");
        assert!(url.starts_with("postgres://"));
        let selected = schema.to_owned();
        sqlx::any::AnyPoolOptions::new()
            .max_connections(4)
            .after_connect(move |connection, _| {
                let name = selected.clone();
                Box::pin(async move {
                    sqlx::query(&format!("SET search_path TO {name}"))
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect(&url)
            .await
            .unwrap()
    } else {
        sqlx::any::AnyPoolOptions::new()
            .max_connections(4)
            .after_connect(|connection, _| {
                Box::pin(async move {
                    sqlx::query("PRAGMA foreign_keys=ON")
                        .execute(&mut *connection)
                        .await?;
                    sqlx::query("PRAGMA busy_timeout=5000")
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect(&format!(
                "sqlite://{}?mode=rwc",
                root.join("draft-transfer.sqlite").display()
            ))
            .await
            .unwrap()
    }
}

fn state(db: AnyPool, root: &std::path::Path) -> crate::AppState {
    crate::AppState {
        db,
        jwt_secret: JWT.into(),
        base_url: "http://owned.invalid".into(),
        search_index: crate::search::SearchIndex::new(root.join("search-index")).unwrap(),
        oauth_cache: crate::auth::routes::OAuthStateCache::new(),
        ws_rooms: crate::sync::WsRooms::default(),
    }
}

async fn protected(state: &crate::AppState) -> Value {
    let mut connection = state.db.acquire().await.unwrap();
    let mut out = serde_json::Map::new();
    for (name, query) in [
        ("receipts", "SELECT * FROM draft_transfer_receipts"),
        (
            "drafts",
            "SELECT * FROM document_drafts WHERE document_id=$1",
        ),
        (
            "draft_access",
            "SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.document_id=$1",
        ),
        (
            "audit",
            "SELECT * FROM audit_log WHERE action='document.draft_transfer'",
        ),
    ] {
        let args: &[&str] = if query.contains("$1") { &[PAGE] } else { &[] };
        let mut rows = crate::spaces::document_deletion::rows(&mut connection, query, args)
            .await
            .unwrap();
        rows.sort_by_cached_key(Value::to_string);
        out.insert(name.into(), json!(rows));
    }
    Value::Object(out)
}

async fn acceptance(postgres: bool) {
    let owned = std::path::PathBuf::from(
        std::env::var("STARLANE_CORE_WORKSPACE").expect("required existing opt-in workspace"),
    );
    let root = crate::storage::data_dir();
    assert!(owned.is_absolute() && root.starts_with(&owned) && root != owned);
    assert!(!root.exists(), "fixture requires a fresh DATA_DIR");
    std::fs::create_dir_all(&root).unwrap();
    sqlx::any::install_default_drivers();

    let schema = format!("draft_transfer_{}", uuid::Uuid::new_v4().simple());
    if postgres {
        let url = std::env::var("STARLANE_CORE_PG_URL").expect("required owned PG URL");
        let mut control = sqlx::AnyConnection::connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&mut control)
            .await
            .unwrap();
        control.close().await.unwrap();
    }
    let db = connect(postgres, &schema, &root).await;
    crate::db::migrate_selected(
        &db,
        if postgres {
            crate::db::DatabaseEngine::Postgres
        } else {
            crate::db::DatabaseEngine::Sqlite
        },
    )
    .await
    .unwrap();
    for actor in ["owner", "editor", "viewer"] {
        sqlx::query("INSERT INTO users(id,email,display_name,role,account_kind) VALUES($1,$2,$1,'member','member')")
            .bind(actor)
            .bind(format!("{actor}@transfer.invalid"))
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,session_kind) VALUES($1,$2,'not-a-token','2999-01-01T00:00:00Z','browser')")
            .bind(format!("transfer-{actor}"))
            .bind(actor)
            .execute(&db)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,'Transfer acceptance','transfer-acceptance','private','owner')")
        .bind(SPACE)
        .execute(&db)
        .await
        .unwrap();
    for (actor, role) in [("owner", "owner"), ("editor", "editor"), ("viewer", "viewer")] {
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,$2,$3,$4)")
            .bind(format!("transfer-member-{actor}"))
            .bind(SPACE)
            .bind(actor)
            .bind(role)
            .execute(&db)
            .await
            .unwrap();
    }

    let app = state(db, &root);
    let page = call(
        &app,
        "owner",
        "POST",
        &format!("/spaces/{SPACE}/documents"),
        json!({"source_id": PAGE, "title": "Transfer target", "content": "Server source"}),
        StatusCode::CREATED,
    )
    .await;
    let source_draft = page["draft_id"].as_str().unwrap();
    let source_path = format!("/spaces/{SPACE}/documents/{PAGE}/drafts/{source_draft}");
    call(
        &app,
        "owner",
        "PUT",
        &format!("{source_path}/access/editor"),
        json!({"role": "editor"}),
        StatusCode::NO_CONTENT,
    )
    .await;
    let source = crate::spaces::draft_save::selected(
        &mut app.db.acquire().await.unwrap(),
        SPACE,
        PAGE,
        source_draft,
    )
    .await
    .unwrap();
    let operation = uuid::Uuid::new_v4().to_string();
    let reviewed = json!({
        "title": "Reviewed local title",
        "icon": "🛰️",
        "cover_image": Value::Null,
        "content": REVIEWED,
    });
    let transfer = json!({
        "schema_version": 1,
        "operation_id": operation,
        "local_source": {
            "provider": "native-local",
            "root_identity": "owned-local-root",
            "space_id": "local-space",
            "document_id": "local-page",
            "draft_id": "local-draft",
            "draft_created_at": "2026-10-19T20:00:00Z",
            "snapshot_digest": crate::spaces::draft_save::digest(&reviewed).unwrap(),
        },
        "server_source": {
            "kind": "draft",
            "draft_id": source_draft,
            "draft_created_at": crate::spaces::draft_save::string(&source, "created_at").unwrap(),
            "source_digest": crate::spaces::draft_save::digest(&source).unwrap(),
        },
        "reviewed_draft": reviewed,
        "selected_operation_fingerprints": ["a".repeat(64), "b".repeat(64)],
    });
    let transfer_path = format!("/spaces/{SPACE}/documents/{PAGE}/draft-transfers");

    // Treat the first response as a lost acknowledgement, then prove exact HTTP retry.
    let (lost_status, lost_ack) =
        request(&app, "editor", "POST", &transfer_path, transfer.clone()).await;
    assert_eq!(lost_status, StatusCode::CREATED, "lost ACK request: {lost_ack}");
    let after_first = protected(&app).await;
    let retry = call(
        &app,
        "editor",
        "POST",
        &transfer_path,
        transfer.clone(),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(retry, lost_ack);
    assert_eq!(protected(&app).await, after_first);
    let target = retry["target_draft_id"].as_str().unwrap().to_owned();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM document_draft_access WHERE draft_id=$1")
            .bind(&target)
            .fetch_one(&app.db)
            .await
            .unwrap(),
        0,
    );
    let exact: (String, String, Option<String>, Option<String>) = sqlx::query_as(
        "SELECT owner_id,content,icon,cover_image FROM document_drafts WHERE id=$1",
    )
    .bind(&target)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(exact, ("editor".into(), REVIEWED.into(), Some("🛰️".into()), None));
    let (viewer_status, _) = request(
        &app,
        "viewer",
        "GET",
        &format!("/spaces/{SPACE}/documents/{PAGE}/drafts/{target}"),
        Value::Null,
    )
    .await;
    assert!(matches!(viewer_status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND));
    assert_eq!(protected(&app).await, after_first);

    let mut changed = transfer.clone();
    changed["reviewed_draft"]["content"] = json!(format!("{REVIEWED}changed"));
    changed["local_source"]["snapshot_digest"] = json!(
        crate::spaces::draft_save::digest(&changed["reviewed_draft"]).unwrap()
    );
    call(
        &app,
        "editor",
        "POST",
        &transfer_path,
        changed,
        StatusCode::CONFLICT,
    )
    .await;
    assert_eq!(protected(&app).await, after_first);

    // Close and rebuild the pool to prove the receipt survives a process-style reconnect.
    app.db.close().await;
    drop(app);
    let app = state(connect(postgres, &schema, &root).await, &root);
    let cold_retry = call(
        &app,
        "editor",
        "POST",
        &transfer_path,
        transfer.clone(),
        StatusCode::CREATED,
    )
    .await;
    assert_eq!(cold_retry, retry);
    assert_eq!(protected(&app).await, after_first);

    sqlx::query("DELETE FROM space_members WHERE space_id=$1 AND user_id='editor'")
        .bind(SPACE)
        .execute(&app.db)
        .await
        .unwrap();
    let before_denial = protected(&app).await;
    let (status, _) = request(&app, "editor", "POST", &transfer_path, transfer).await;
    assert!(matches!(status, StatusCode::FORBIDDEN | StatusCode::NOT_FOUND));
    assert_eq!(protected(&app).await, before_denial);

    let engine = if postgres { "postgres" } else { "sqlite" };
    std::fs::write(
        owned.join(format!("draft-transfer-{engine}-result.json")),
        serde_json::to_vec_pretty(&json!({
            "engine": engine,
            "schema": schema,
            "actual_http": true,
            "lost_ack_retry": true,
            "cold_reconnect_retry": true,
            "private_target": true,
            "viewer_denied": true,
            "changed_request_conflict": true,
            "authority_loss_denied": true,
            "target_draft_id": target,
        }))
        .unwrap(),
    )
    .unwrap();
    println!("DRAFT_TRANSFER_HTTP_PASS engine={engine} lost_ack cold_reconnect private_target viewer_denied changed_request authority_loss");
    app.db.close().await;
}

#[tokio::test]
#[ignore = "requires owned STARLANE_CORE_WORKSPACE and fresh DATA_DIR; run alone"]
async fn required_sqlite_draft_transfer_http() {
    acceptance(false).await;
}

#[tokio::test]
#[ignore = "requires owned STARLANE_CORE_WORKSPACE, STARLANE_CORE_PG_URL and fresh DATA_DIR; run alone"]
async fn required_postgres_draft_transfer_http() {
    acceptance(true).await;
}
