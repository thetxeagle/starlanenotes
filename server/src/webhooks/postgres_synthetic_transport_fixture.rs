//! Opt-in real PostgreSQL router/loopback acceptance. Only generated neutral events leave this process.
use super::{next, receiver, signature, AppState, Body, Request, StatusCode};
use serde_json::{json, Value};
use sqlx::{AnyPool, Connection};
use std::path::PathBuf;
use tower::ServiceExt;

async fn request(state: &AppState, method: &str, path: &str, token: Option<&str>, body: Value) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(path)
        .extension(axum::extract::ConnectInfo("127.0.0.1:5199".parse::<std::net::SocketAddr>().unwrap()))
        .header("Content-Type", "application/json");
    if let Some(token) = token { builder = builder.header("Authorization", format!("Bearer {token}")); }
    let response = crate::api_routes().with_state(state.clone())
        .oneshot(builder.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await.unwrap();
    let parsed: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if !status.is_success() && path == "/auth/login" {
        // Never print a successful authentication response or token fields.
        let error = parsed.get("error").and_then(Value::as_str).unwrap_or("unparsed error").chars().take(256).collect::<String>();
        let message = parsed.get("message").and_then(Value::as_str).unwrap_or("no message").chars().take(512).collect::<String>();
        eprintln!("Owned PostgreSQL fixture login failed: status={status}, error={error:?}, message={message:?}");
    }
    (status, parsed)
}

async fn history(state: &AppState, token: &str, hook: &str, test_id: &str, success: bool) -> Value {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let (status, body) = request(state,"GET",&format!("/webhooks/{hook}/deliveries"),Some(token),json!({})).await;
            assert_eq!(status,StatusCode::OK,"actual PostgreSQL history route failed: {body}");
            if body.as_array().unwrap().iter().any(|row| row["test_id"] == test_id && row["success"] == success) { return body; }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }).await.expect("correlated PostgreSQL history was not recorded")
}

async fn create(state: &AppState, token: &str, url: &str) -> String {
    let (status, body) = request(state,"POST","/webhooks",Some(token),json!({
        "name":"Owned PostgreSQL loopback fixture", "url":url, "secret":"fixture-secret", "events":[super::dispatch::PUBLIC_PAGE_PUBLISHED_EVENT]
    })).await;
    assert_eq!(status,StatusCode::CREATED,"actual PostgreSQL create failed: {body}");
    let id = body["id"].as_str().unwrap().to_owned();
    let (read_status, stored) = request(state,"GET",&format!("/webhooks/{id}"),Some(token),json!({})).await;
    assert_eq!(read_status,StatusCode::OK,"actual stored PostgreSQL metadata read failed");
    assert_eq!(stored["created_by"],"pg-synthetic-admin");
    id
}

async fn queue(state: &AppState, token: &str, hook: &str) -> String {
    let (status, body) = request(state,"POST",&format!("/webhooks/{hook}/test"),Some(token),json!({})).await;
    assert_eq!(status,StatusCode::ACCEPTED,"actual PostgreSQL synthetic route failed: {body}");
    assert_eq!(body["status"],"queued");
    let id = body["delivery_id"].as_str().unwrap();
    uuid::Uuid::parse_str(id).unwrap();
    id.to_owned()
}

#[tokio::test]
#[ignore = "requires STARLANE_AUDIT_WEBHOOK_PG_URL and owned STARLANE_AUDIT_WEBHOOK_WORKSPACE"]
async fn required_postgres_synthetic_transport_history_disconnect() {
    let owned = PathBuf::from(std::env::var("STARLANE_AUDIT_WEBHOOK_WORKSPACE").expect("required owned workspace"));
    let root = crate::storage::data_dir();
    assert!(owned.is_absolute() && root.starts_with(&owned) && root != owned,"DATA_DIR must be inside owned fixture workspace");
    std::fs::create_dir_all(&root).unwrap();
    let url = std::env::var("STARLANE_AUDIT_WEBHOOK_PG_URL").expect("required owned PostgreSQL URL");
    assert!(url.starts_with("postgres://"));
    sqlx::any::install_default_drivers();
    let schema = format!("aw_transport_{}",uuid::Uuid::new_v4().simple());
    let mut control = sqlx::AnyConnection::connect(&url).await.unwrap();
    sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&mut control).await.unwrap();
    control.close().await.unwrap();
    let selected_schema = schema.clone();
    let db: AnyPool = sqlx::any::AnyPoolOptions::new().max_connections(4)
        .after_connect(move |connection,_| { let schema=selected_schema.clone(); Box::pin(async move {
            sqlx::query(&format!("SET search_path TO {schema}")).execute(connection).await?; Ok(())
        }) }).connect(&url).await.unwrap();
    crate::db::migrate_selected(&db,crate::db::DatabaseEngine::Postgres).await.unwrap();
    let password = "Owned-fixture-password-42!";
    let hash = crate::auth::password::hash_password(password).unwrap();
    sqlx::query("INSERT INTO users(id,email,display_name,role,account_kind,password_hash) VALUES('pg-synthetic-admin','pg-synthetic@fixture.invalid','Owned synthetic admin','admin','member',$1)")
        .bind(hash).execute(&db).await.unwrap();
    sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('pg-webhook-space','Webhook fixture','pg-webhook-space','private','pg-synthetic-admin')")
        .execute(&db).await.unwrap();
    sqlx::query("INSERT INTO documents(id,space_id,title,content_path,public,created_by,last_updated_by,published_at) VALUES('pg-public-page','pg-webhook-space','Public receiver fixture','',1,'pg-synthetic-admin','pg-synthetic-admin','2026-10-21T20:00:00Z')")
        .execute(&db).await.unwrap();
    let state = AppState { db:db.clone(),jwt_secret:"owned-postgres-synthetic-fixture-jwt-secret".into(),base_url:"http://owned.invalid".into(),
        search_index:crate::search::SearchIndex::new(root.join(format!("search-{schema}"))).unwrap(),
        oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default() };
    let (status, login) = request(&state,"POST","/auth/login",None,json!({"email":"pg-synthetic@fixture.invalid","password":password})).await;
    assert_eq!(status,StatusCode::OK,"actual login failed");
    assert_eq!(login["user"]["account_kind"],"member");
    let token = login["access_token"].as_str().unwrap();
    let mut accepted = receiver(1,64).await;
    let hook = create(&state,token,&accepted.url).await;
    let test_id = queue(&state,token,&hook).await;
    for _ in 0..2 {
        let delivery = next(&mut accepted).await;
        signature(&delivery,"fixture-secret");
        let payload:Value=serde_json::from_slice(&delivery.body).unwrap();
        assert_eq!(payload,json!({"event":"webhook.test","test":true,"test_id":test_id}));
    }
    let rows = history(&state,token,&hook,&test_id,true).await;
    let matching:Vec<_> = rows.as_array().unwrap().iter().filter(|row| row["test_id"]==test_id).collect();
    assert_eq!(matching.len(),2);
    assert!(matching.iter().any(|row| row["success"]==false && row["response_status"]==500 && row["attempt_number"]==1));
    assert!(matching.iter().any(|row| row["success"]==true && row["response_status"]==200 && row["attempt_number"]==2));
    super::dispatch::dispatch_public_page_published(&db,"pg-webhook-space","pg-public-page").await;
    let published = next(&mut accepted).await;
    signature(&published,"fixture-secret");
    assert_eq!(published.headers["X-Starlane-Event"],super::dispatch::PUBLIC_PAGE_PUBLISHED_EVENT);
    assert_eq!(serde_json::from_slice::<Value>(&published.body).unwrap(),json!({
        "schema_version":1,
        "event":super::dispatch::PUBLIC_PAGE_PUBLISHED_EVENT,
        "space_id":"pg-webhook-space",
        "document_id":"pg-public-page",
        "title":"Public receiver fixture",
        "published_at":"2026-10-21T20:00:00Z"
    }));
    let public_delivery_id:String=tokio::time::timeout(std::time::Duration::from_secs(10),async {
        loop {
            if let Some(id)=sqlx::query_scalar("SELECT id FROM webhook_deliveries WHERE webhook_id=$1 AND event_type=$2 ORDER BY delivered_at DESC LIMIT 1")
                .bind(&hook).bind(super::dispatch::PUBLIC_PAGE_PUBLISHED_EVENT).fetch_optional(&db).await.unwrap(){break id;}
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }).await.expect("public PostgreSQL delivery history was not recorded");
    sqlx::query("UPDATE documents SET public=0 WHERE id='pg-public-page' AND space_id='pg-webhook-space'").execute(&db).await.unwrap();
    assert_eq!(request(&state,"POST",&format!("/webhooks/{hook}/deliveries/{public_delivery_id}/redeliver"),Some(token),json!({})).await.0,StatusCode::NOT_FOUND);
    let persisted:i64=sqlx::query_scalar("SELECT COUNT(*) FROM webhook_deliveries WHERE webhook_id=$1 AND event_type='webhook.test'")
        .bind(&hook).fetch_one(&db).await.unwrap();
    assert_eq!(persisted,2,"history must come from PostgreSQL records");
    assert_eq!(request(&state,"DELETE",&format!("/webhooks/{hook}"),Some(token),json!({})).await.0,StatusCode::NO_CONTENT);
    let mut failing = receiver(10,32).await;
    let disconnected = create(&state,token,&failing.url).await;
    let stopped_test = queue(&state,token,&disconnected).await;
    signature(&next(&mut failing).await,"fixture-secret");
    history(&state,token,&disconnected,&stopped_test,false).await;
    assert_eq!(request(&state,"DELETE",&format!("/webhooks/{disconnected}"),Some(token),json!({})).await.0,StatusCode::NO_CONTENT);
    assert!(tokio::time::timeout(std::time::Duration::from_millis(4500),failing.rx.recv()).await.is_err(),"disconnect must stop the next scheduled attempt");
    assert_eq!(request(&state,"GET",&format!("/webhooks/{disconnected}"),Some(token),json!({})).await.0,StatusCode::NOT_FOUND);
    assert_eq!(request(&state,"POST",&format!("/webhooks/{disconnected}/test"),Some(token),json!({})).await.0,StatusCode::NOT_FOUND);
    println!("POSTGRES_SYNTHETIC_TRANSPORT_PASS actual-login member-admin create+202 signed-neutral failure500+retry200 correlated-history2 signed-public-publication revoked-redelivery-denied disconnect-stops-retry; generated-loopback-only");
    drop(state);
    db.close().await;
}
