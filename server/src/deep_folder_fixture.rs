//! Opt-in real migrated SQLite/PostgreSQL folder routes, synthetic owned data only.
use axum::{body::{to_bytes, Body}, http::{Request, StatusCode}};
use serde_json::{json, Value};
use sqlx::{AnyPool, Connection};
use tower::ServiceExt;

const SPACE: &str = "deep-folder-source";
const TARGET: &str = "deep-folder-target";
const JWT: &str = "fictional-deep-folder-fixture-secret";

fn token(actor: &str) -> String {
    crate::auth::jwt::create_token_pair_for_session(actor, &format!("{actor}@deep.invalid"),
        "member", JWT, Some(&format!("deep-{actor}")), 30).unwrap().access_token
}
async fn request(s: &crate::AppState, actor: &str, method: &str, url: &str, body: Value) -> (StatusCode, Value) {
    let response = crate::api_routes().with_state(s.clone()).oneshot(Request::builder()
        .method(method).uri(url).header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token(actor)))
        .body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 16 * 1024 * 1024).await.unwrap();
    let value = if bytes.is_empty() { Value::Null } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({"unparsed_error":String::from_utf8_lossy(&bytes)}))
    };
    (status, value)
}
async fn call(s: &crate::AppState, actor: &str, method: &str, url: &str, body: Value, expected: StatusCode) -> Value {
    let (status, result) = request(s, actor, method, url, body).await;
    assert_eq!(status, expected, "{actor} {method} {url}: {result}"); result
}
fn folder(space: &str, id: &str) -> String { format!("/spaces/{space}/folders/{id}") }
async fn create(s: &crate::AppState, id: &str, parent: Option<&str>) {
    call(s, "editor", "POST", &format!("/spaces/{SPACE}/folders"),
        json!({"name":id,"source_id":id,"parent_id":parent}), StatusCode::CREATED).await;
}
async fn domain(s: &crate::AppState) -> Value {
    let mut conn = s.db.acquire().await.unwrap();
    let mut result = serde_json::Map::new();
    for table in ["folders", "users", "space_members", "documents", "document_drafts", "document_publication_snapshots", "draft_publication_receipts", "snapshot_recovery_receipts", "audit_log"] {
        let rows = crate::spaces::document_deletion::rows(&mut conn, &format!("SELECT * FROM {table}"), &[]).await.unwrap();
        let mut rows = rows; rows.sort_by_cached_key(Value::to_string);
        result.insert(table.into(), json!(rows));
    }
    Value::Object(result)
}
fn replay(id: &str, action: &str, parent: Option<&str>) -> Value {
    json!({"schema_version":1,"provider":"self-hosted","operation_count":1,"recovery_checkpoint_count":0,"recovery_checkpoints":[],
        "replay_contract":{"schema_version":1,"operation_order":"queued_at_ascending","idempotency_key":"operation.id","transport":"server-api","server_backends":["sqlite","postgresql"],"required_payload_status":"available","conflict_policy":"server_validates_version_and_permissions","web_canonical_store":"server-api"},
        "operations":[{"id":format!("deep-replay-{action}-{id}"),"entity_type":"folder","entity_id":id,"action":action,"queued_at":"2026-10-15T12:00:00Z","payload_status":"available","payload":{"id":id,"space_id":SPACE,"name":id,"parent_id":parent,"icon":null,"sort_order":17,"created_at":"2026-10-15T12:00:00Z","updated_at":"2026-10-15T12:00:00Z"}}]})
}
async fn preview(s: &crate::AppState, root: &str, destination_space: &str, parent: Option<&str>) -> Value {
    call(s,"owner","POST",&format!("/spaces/{SPACE}/moves/preview"),json!({"root":{"kind":"folder","id":root},"destination":{"space_id":destination_space,"folder_id":null,"parent_id":parent}}),StatusCode::OK).await
}
async fn setup(postgres: bool) -> (crate::AppState, String) {
    let owned = std::path::PathBuf::from(std::env::var("STARLANE_CORE_WORKSPACE").expect("required owned workspace"));
    let root = crate::storage::data_dir();
    assert!(owned.is_absolute() && root.starts_with(&owned) && root != owned);
    assert!(!root.exists(), "fixture requires a fresh DATA_DIR");
    std::fs::create_dir_all(&root).unwrap();
    sqlx::any::install_default_drivers();
    let schema = format!("deep_folder_{}", uuid::Uuid::new_v4().simple());
    let db: AnyPool = if postgres {
        let url = std::env::var("STARLANE_CORE_PG_URL").expect("required owned PG URL");
        assert!(url.starts_with("postgres://"));
        let mut control = sqlx::AnyConnection::connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&mut control).await.unwrap();
        control.close().await.unwrap();
        let selected = schema.clone();
        sqlx::any::AnyPoolOptions::new().max_connections(4).after_connect(move |c,_| {
            let name=selected.clone(); Box::pin(async move {sqlx::query(&format!("SET search_path TO {name}")).execute(c).await?;Ok(())})
        }).connect(&url).await.unwrap()
    } else {
        sqlx::any::AnyPoolOptions::new().max_connections(4).after_connect(|c,_| Box::pin(async move {
            sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *c).await?;
            sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?; Ok(())
        })).connect(&format!("sqlite://{}?mode=rwc",root.join("deep-folders.sqlite").display())).await.unwrap()
    };
    crate::db::migrate_selected(&db, if postgres {crate::db::DatabaseEngine::Postgres} else {crate::db::DatabaseEngine::Sqlite}).await.unwrap();
    for actor in ["owner","editor","viewer"] {
        sqlx::query("INSERT INTO users(id,email,display_name,role,account_kind) VALUES($1,$2,$1,'member','member')").bind(actor).bind(format!("{actor}@deep.invalid")).execute(&db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,session_kind) VALUES($1,$2,'fictional','2999-01-01T00:00:00Z','browser')").bind(format!("deep-{actor}")).bind(actor).execute(&db).await.unwrap();
    }
    for space in [SPACE,TARGET] {
        sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,$1,$1,'private','owner')").bind(space).execute(&db).await.unwrap();
        for (actor,role) in [("owner","owner"),("editor","editor"),("viewer","viewer")] {
            sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,$2,$3,$4)").bind(format!("deep-{space}-{actor}")).bind(space).bind(actor).bind(role).execute(&db).await.unwrap();
        }
    }
    let s=crate::AppState{db,jwt_secret:JWT.into(),base_url:"http://deep.invalid".into(),search_index:crate::search::SearchIndex::new(root.join("search-index")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};
    (s,schema)
}
async fn matrix(postgres: bool) {
    let (s,schema)=setup(postgres).await;
    // Actual registered creation routes, not seeded depth fixtures.
    for depth in 1..=256 {
        let id=format!("deep-{depth}");let parent=(depth>1).then(||format!("deep-{}",depth-1));
        create(&s,&id,parent.as_deref()).await;
        if [4,32,256].contains(&depth) {
            let current=call(&s,"editor","GET",&folder(SPACE,&id),Value::Null,StatusCode::OK).await;
            assert_eq!(current["parent_id"],json!(parent));
            call(&s,"editor","PUT",&folder(SPACE,&id),json!({"name":format!("Renamed {depth}")}),StatusCode::NO_CONTENT).await;
            call(&s,"editor","PUT",&format!("{}/reorder",folder(SPACE,&id)),json!({"sort_order":depth}),StatusCode::NO_CONTENT).await;
            println!("DEEP_FOLDER_ACTUAL_ROUTE depth={depth} rename/reorder/get PASS postgres={postgres}");
        }
    }
    let catalog=call(&s,"viewer","GET",&format!("/spaces/{SPACE}/folders"),Value::Null,StatusCode::OK).await;
    assert_eq!(catalog.as_array().unwrap().len(),256);
    let imported=replay("deep-replayed","create",Some("deep-256"));
    let first=call(&s,"editor","POST","/sync/replay",imported.clone(),StatusCode::OK).await;
    assert_eq!(first["applied_operation_count"],1,"{first}");
    let retry=call(&s,"editor","POST","/sync/replay",imported,StatusCode::OK).await;
    assert_eq!(retry["skipped_operation_count"],1,"{retry}");
    let reparent=call(&s,"editor","POST","/sync/replay",replay("deep-replayed","update",Some("deep-32")),StatusCode::OK).await;
    assert_eq!(reparent["applied_operation_count"],1,"{reparent}");
    let deleted=call(&s,"editor","POST","/sync/replay",replay("deep-replayed","delete",None),StatusCode::OK).await;
    assert_eq!(deleted["applied_operation_count"],1,"{deleted}");
    // A page may occupy a deep folder; page parent_id policy remains separate.
    let created=call(&s,"editor","POST",&format!("/spaces/{SPACE}/documents"),json!({"source_id":"deep-page","title":"Deep folder page","folder_id":"deep-256","content":"Owned retained content"}),StatusCode::CREATED).await;
    assert!(created["draft_id"].is_string());
    let before=domain(&s).await;
    for (method,url,body) in [
        ("PUT",folder(SPACE,"deep-1"),json!({"parent_id":"deep-256","name":"must rollback"})),
        ("POST",format!("{}/move",folder(SPACE,"deep-1")),json!({"target_space_id":SPACE,"parent_id":"deep-256"})),
        ("POST",format!("/spaces/{SPACE}/folders"),json!({"name":"Missing","parent_id":"missing"})),
    ] {
        let(status,_)=request(&s,"editor",method,&url,body).await;assert!(status.is_client_error());assert_eq!(domain(&s).await,before);
    }
    call(&s,"owner","POST",&format!("/spaces/{TARGET}/folders"),json!({"source_id":"foreign-folder","name":"Foreign"}),StatusCode::CREATED).await;
    let before=domain(&s).await;
    let(status,_)=request(&s,"editor","PUT",&folder(SPACE,"deep-256"),json!({"parent_id":"foreign-folder"})).await;
    assert!(status.is_client_error());assert_eq!(domain(&s).await,before);
    // Current credential denial with the original session token must leave rows exact.
    sqlx::query("UPDATE space_members SET role='viewer' WHERE space_id=$1 AND user_id='editor'").bind(SPACE).execute(&s.db).await.unwrap();
    let before=domain(&s).await;
    let(status,_)=request(&s,"editor","PUT",&folder(SPACE,"deep-256"),json!({"name":"revoked"})).await;
    assert!(status.is_client_error());assert_eq!(domain(&s).await,before);
    sqlx::query("UPDATE space_members SET role='editor' WHERE space_id=$1 AND user_id='editor'").bind(SPACE).execute(&s.db).await.unwrap();
    // Broad real-route subtree and reviewed cross-space relocation, stable IDs.
    create(&s,"broad-root",None).await;
    for n in 0..64 {create(&s,&format!("broad-{n}"),Some("broad-root")).await;}
    let p=preview(&s,"broad-root",TARGET,Some("foreign-folder")).await;
    assert_eq!(p["blockers"],json!([]),"{p}");assert_eq!(p["commit_available"],true,"{p}");
    let commit=json!({"fingerprint":p["fingerprint"],"decisions":[],"public_links":"preserve","confirm_audience_change":true,"confirm_draft_space_change":true});
    let commit_url=format!("/moves/previews/{}/commit",p["preview_id"].as_str().unwrap());
    let done=call(&s,"owner","POST",&commit_url,commit.clone(),StatusCode::OK).await;assert_eq!(done["committed"],true);
    let prior=domain(&s).await;call(&s,"owner","POST",&commit_url,commit,StatusCode::OK).await;assert_eq!(domain(&s).await,prior);
    let moved=call(&s,"owner","GET",&folder(TARGET,"broad-63"),Value::Null,StatusCode::OK).await;assert_eq!(moved["parent_id"],"broad-root");
    // Deep same-space subtree move is allowed, independently of folder depth.
    create(&s,"moving-root",None).await;create(&s,"moving-child",Some("moving-root")).await;
    call(&s,"editor","POST",&format!("{}/move",folder(SPACE,"moving-root")),json!({"target_space_id":SPACE,"parent_id":"deep-256"}),StatusCode::NO_CONTENT).await;
    let preview_deep=preview(&s,"moving-root",TARGET,Some("foreign-folder")).await;
    assert_eq!(preview_deep["blockers"],json!([]),"{preview_deep}");
    // Final-write authority changes are rolled back with all folder/page/audit rows.
    if postgres {
        sqlx::query("CREATE FUNCTION deep_late_authority() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE space_members SET role='viewer' WHERE user_id='editor'; RETURN NEW; END $$").execute(&s.db).await.unwrap();
        sqlx::query("CREATE TRIGGER deep_late_authority AFTER UPDATE OF name ON folders FOR EACH ROW EXECUTE FUNCTION deep_late_authority()").execute(&s.db).await.unwrap();
    } else {
        sqlx::query("CREATE TRIGGER deep_late_authority AFTER UPDATE OF name ON folders BEGIN UPDATE space_members SET role='viewer' WHERE user_id='editor'; END").execute(&s.db).await.unwrap();
    }
    let prior=domain(&s).await;
    let(status,_)=request(&s,"editor","PUT",&folder(SPACE,"deep-256"),json!({"name":"late failure"})).await;
    assert!(status.is_client_error(),"final credential revocation returned {status}");assert_eq!(domain(&s).await,prior);
    sqlx::query(if postgres {"DROP TRIGGER deep_late_authority ON folders"} else {"DROP TRIGGER deep_late_authority"}).execute(&s.db).await.unwrap();
    // Over-budget broad input: seeded resource fixture, mutation itself uses actual route.
    create(&s,"budget-root",None).await;
    let mut tx=s.db.begin().await.unwrap();
    for chunk in 0..100 {
        // Any's QueryBuilder uses SQLite-style bind syntax; spell portable
        // numbered PostgreSQL/SQLite placeholders explicitly in this fixture.
        let values = (0..100).map(|n|format!("(${},$1,$2,${})",n+3,n+3)).collect::<Vec<_>>().join(",");
        let sql = format!("INSERT INTO folders(id,space_id,parent_id,name) VALUES {values}");
        let mut query = sqlx::query(&sql).bind(SPACE).bind("budget-root");
        for n in 0..100 { query = query.bind(format!("budget-{}",chunk*100+n)); }
        query.execute(&mut *tx).await.unwrap();
    }
    tx.commit().await.unwrap();
    let prior=domain(&s).await;
    let(status,error)=request(&s,"editor","PUT",&folder(SPACE,"budget-root"),json!({"name":"over budget"})).await;
    assert_eq!(status,StatusCode::BAD_REQUEST,"{error}");assert!(error.to_string().contains("10000-node"),"{error}");assert_eq!(domain(&s).await,prior);
    println!("DEEP_FOLDER_MATRIX_PASS postgres={postgres} depth=4/32/256 broad=64 replay/import/retry/update/delete reviewed_commit/retry revoked/cycle/foreign/finalwrite/budget exact_rollback schema={schema}");
    s.db.close().await;
}
#[tokio::test]
#[ignore="requires owned STARLANE_CORE_WORKSPACE and fresh DATA_DIR; run alone"]
async fn required_sqlite_deep_folder_runtime_matrix(){matrix(false).await}
#[tokio::test]
#[ignore="requires owned STARLANE_CORE_WORKSPACE, STARLANE_CORE_PG_URL and fresh DATA_DIR; run alone"]
async fn required_postgres_deep_folder_runtime_matrix(){matrix(true).await}
