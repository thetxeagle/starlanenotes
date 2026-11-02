use super::*;
use crate::spaces::routes::catalog_tests::{auth, catalog_state};
use axum::{
    body::{Body, Bytes},
    http::{HeaderMap, Request},
    routing::post,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::sync::mpsc;
use tower::ServiceExt;

struct Received {
    headers: HeaderMap,
    body: Bytes,
}
#[derive(Clone)]
struct ReceiverState {
    tx: mpsc::UnboundedSender<Received>,
    count: Arc<AtomicUsize>,
    failures: usize,
    body_size: usize,
}
struct Receiver {
    url: String,
    rx: mpsc::UnboundedReceiver<Received>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Receiver {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn receive(
    State(state): State<ReceiverState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let count = state.count.fetch_add(1, Ordering::SeqCst);
    state.tx.send(Received { headers, body }).unwrap();
    (
        if count < state.failures {
            StatusCode::INTERNAL_SERVER_ERROR
        } else {
            StatusCode::OK
        },
        "x".repeat(state.body_size),
    )
}
async fn receiver(failures: usize, body_size: usize) -> Receiver {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/hook", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::unbounded_channel();
    let app = Router::new()
        .route("/hook", post(receive))
        .with_state(ReceiverState {
            tx,
            count: Arc::new(AtomicUsize::new(0)),
            failures,
            body_size,
        });
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Receiver { url, rx, task }
}
async fn next(receiver: &mut Receiver) -> Received {
    tokio::time::timeout(std::time::Duration::from_secs(10), receiver.rx.recv())
        .await
        .unwrap()
        .unwrap()
}
async fn setup() -> AppState {
    let state = catalog_state(false).await;
    sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'")
        .execute(&state.db)
        .await
        .unwrap();
    state
}
async fn public_page(state: &AppState) {
    sqlx::query("INSERT INTO documents(id,space_id,title,content_path,public,created_by,last_updated_by,published_at) VALUES('target-a','space-a','Public fixture','','1','owner-a','owner-a','2026-10-21T20:00:00Z')")
        .execute(&state.db)
        .await
        .unwrap();
}
fn token(state: &AppState, actor: &str) -> String {
    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &auth(actor).claims,
        &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    )
    .unwrap()
}
async fn http(
    state: &AppState,
    method: &str,
    path: &str,
    credential: &str,
    key: bool,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .header("Content-Type", "application/json")
        .header(
            if key { "X-API-Key" } else { "Authorization" },
            if key {
                credential.to_owned()
            } else {
                format!("Bearer {credential}")
            },
        )
        .body(Body::from(body.to_string()))
        .unwrap();
    let response = router()
        .with_state(state.clone())
        .oneshot(request)
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null),
    )
}
async fn create(
    state: &AppState,
    receiver: &Receiver,
    scope: Option<&str>,
    _events: &[&str],
) -> String {
    let (status, body) = http(state, "POST", "/webhooks", &token(state, "owner-a"), false,
        serde_json::json!({"name":"Fixture receiver","url":receiver.url,"secret":"fixture-secret","events":[dispatch::PUBLIC_PAGE_PUBLISHED_EVENT],"space_id":scope})).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["id"].as_str().unwrap().to_owned()
}
async fn count(state: &AppState, id: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM webhook_deliveries WHERE webhook_id=$1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .unwrap()
}
async fn recorded(state: &AppState, id: &str, minimum: i64) {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while count(state, id).await < minimum {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
fn signature(received: &Received, secret: &str) {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(hash_webhook_secret(secret).as_bytes()).unwrap();
    mac.update(&received.body);
    assert_eq!(
        received.headers["X-Starlane-Signature"],
        format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
    );
}
async fn key(state: &AppState, id: &str, owner: Option<&str>, service: bool, scopes: &[&str]) {
    sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES($1,$2,$1,$3,'fixture',$4,$5)")
        .bind(id).bind(owner).bind(crate::auth::api_keys::hash_api_key(id)).bind(i64::from(service))
        .bind(serde_json::to_string(scopes).unwrap()).execute(&state.db).await.unwrap();
}

#[tokio::test]
async fn migrated_router_test_is_admin_only_neutral_signed_and_correlated() {
    let state = setup().await;
    let mut receiver = receiver(0, 100_000).await;
    let id = create(&state, &receiver, Some("space-a"), &["fixture.updated"]).await;
    let path = format!("/webhooks/{id}/test");
    for actor in ["viewer-a", "editor-a"] {
        assert_eq!(
            http(
                &state,
                "POST",
                &path,
                &token(&state, actor),
                false,
                serde_json::json!({})
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
    }
    for (id, owner, service, scopes, expected) in [
        (
            "read-key",
            Some("owner-a"),
            false,
            vec!["read"],
            StatusCode::FORBIDDEN,
        ),
        (
            "write-key",
            Some("owner-a"),
            false,
            vec!["write"],
            StatusCode::FORBIDDEN,
        ),
        (
            "viewer-admin",
            Some("viewer-a"),
            false,
            vec!["admin"],
            StatusCode::UNAUTHORIZED,
        ),
        (
            "service-read",
            None,
            true,
            vec!["read"],
            StatusCode::FORBIDDEN,
        ),
        (
            "service-write",
            None,
            true,
            vec!["write"],
            StatusCode::FORBIDDEN,
        ),
    ] {
        key(&state, id, owner, service, &scopes).await;
        assert_eq!(
            http(&state, "POST", &path, id, true, serde_json::json!({}))
                .await
                .0,
            expected
        );
    }
    assert_eq!(count(&state, &id).await, 0);
    let ignored = serde_json::json!({"url":"http://forbidden.invalid", "secret":"never-forward", "content":"PRIVATE DRAFT"});
    for (credential, is_key) in [
        (token(&state, "owner-a"), false),
        ("admin-key".into(), true),
        ("service-admin".into(), true),
    ] {
        if is_key {
            key(
                &state,
                &credential,
                if credential == "admin-key" {
                    Some("owner-a")
                } else {
                    None
                },
                credential == "service-admin",
                &["admin"],
            )
            .await;
        }
        let (status, body) =
            http(&state, "POST", &path, &credential, is_key, ignored.clone()).await;
        assert_eq!(status, StatusCode::ACCEPTED, "{body}");
        assert_eq!(body["status"], "queued");
        let received = next(&mut receiver).await;
        signature(&received, "fixture-secret");
        assert_eq!(received.headers["X-Starlane-Event"], "webhook.test");
        assert_eq!(
            received.headers["X-Starlane-Delivery"],
            body["delivery_id"].as_str().unwrap()
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&received.body).unwrap(),
            serde_json::json!({"event":"webhook.test","test":true,"test_id":body["delivery_id"]})
        );
    }
    recorded(&state, &id, 3).await;
    let max: i64 = sqlx::query_scalar(
        "SELECT MAX(length(response_body)) FROM webhook_deliveries WHERE webhook_id=$1",
    )
    .bind(&id)
    .fetch_one(&state.db)
    .await
    .unwrap();
    assert_eq!(max, 16 * 1024);
    let (status, body) = http(
        &state,
        "GET",
        &format!("/webhooks/{id}"),
        &token(&state, "owner-a"),
        false,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["created_by"], "owner-a");
    assert_eq!(body["space_id"], "space-a");
    assert!(body.get("secret").is_none());
    assert!(body.get("secret_hash").is_none());
    sqlx::query("UPDATE webhooks SET is_active=0 WHERE id=$1")
        .bind(&id)
        .execute(&state.db)
        .await
        .unwrap();
    assert_eq!(
        http(
            &state,
            "POST",
            &path,
            &token(&state, "owner-a"),
            false,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        http(
            &state,
            "POST",
            "/webhooks/missing/test",
            &token(&state, "owner-a"),
            false,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn migrated_dispatch_matches_scope_and_exact_subscriptions() {
    let state = setup().await;
    public_page(&state).await;
    let mut global = receiver(0, 0).await;
    let mut scoped = receiver(0, 0).await;
    let mut foreign = receiver(0, 0).await;
    let mut other = receiver(0, 0).await;
    let global_id = create(&state, &global, None, &[dispatch::PUBLIC_PAGE_PUBLISHED_EVENT]).await;
    let scoped_id = create(&state, &scoped, Some("space-a"), &[dispatch::PUBLIC_PAGE_PUBLISHED_EVENT]).await;
    let foreign_id = create(&state, &foreign, Some("space-b"), &[dispatch::PUBLIC_PAGE_PUBLISHED_EVENT]).await;
    let other_id = create(&state, &other, None, &[dispatch::PUBLIC_PAGE_PUBLISHED_EVENT]).await;
    sqlx::query("UPDATE webhooks SET events='[\"fixture.other\"]' WHERE id=$1")
        .bind(&other_id).execute(&state.db).await.unwrap();
    dispatch::dispatch_public_page_published(&state.db, "space-a", "target-a").await;
    signature(&next(&mut global).await, "fixture-secret");
    signature(&next(&mut scoped).await, "fixture-secret");
    recorded(&state, &global_id, 1).await;
    recorded(&state, &scoped_id, 1).await;
    let delivery_id: String = sqlx::query_scalar(
        "SELECT id FROM webhook_deliveries WHERE webhook_id=$1 AND event_type=$2 LIMIT 1",
    )
    .bind(&global_id)
    .bind(dispatch::PUBLIC_PAGE_PUBLISHED_EVENT)
    .fetch_one(&state.db)
    .await
    .unwrap();
    sqlx::query("UPDATE documents SET public=0 WHERE id='target-a' AND space_id='space-a'")
        .execute(&state.db)
        .await
        .unwrap();
    assert_eq!(
        http(
            &state,
            "POST",
            &format!("/webhooks/{global_id}/deliveries/{delivery_id}/redeliver"),
            &token(&state, "owner-a"),
            false,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    dispatch::dispatch_public_page_published(&state.db, "space-a", "target-a").await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert_eq!(count(&state, &global_id).await, 1);
    assert_eq!(count(&state, &foreign_id).await, 0);
    assert_eq!(count(&state, &other_id).await, 0);
    assert!(foreign.rx.try_recv().is_err());
    assert!(other.rx.try_recv().is_err());
}

#[tokio::test]
async fn migrated_delivery_retries_three_times_and_redelivers_recorded_payload() {
    let state = setup().await;
    let mut receiver = receiver(2, 0).await;
    let id = create(&state, &receiver, None, &["*"]).await;
    let (status, queued) = http(
        &state,
        "POST",
        &format!("/webhooks/{id}/test"),
        &token(&state, "owner-a"),
        false,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let first = next(&mut receiver).await;
    let second = next(&mut receiver).await;
    let third = next(&mut receiver).await;
    assert_ne!(
        first.headers["X-Starlane-Delivery"],
        second.headers["X-Starlane-Delivery"]
    );
    assert_eq!(first.body, third.body);
    recorded(&state, &id, 3).await;
    let (status, history) = http(
        &state,
        "GET",
        &format!("/webhooks/{id}/deliveries"),
        &token(&state, "owner-a"),
        false,
        serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let entries = history.as_array().unwrap();
    assert_eq!(entries.len(), 3);
    assert!(entries
        .iter()
        .all(|e| e["test_id"] == queued["delivery_id"]));
    assert!(entries.iter().any(|e| e["test_id"] == queued["delivery_id"]
        && e["attempt_number"] == 3
        && e["success"] == true));
    assert_eq!(
        http(
            &state,
            "POST",
            &format!(
                "/webhooks/{id}/deliveries/{}/redeliver",
                queued["delivery_id"].as_str().unwrap()
            ),
            &token(&state, "owner-a"),
            false,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );
    assert_eq!(next(&mut receiver).await.body, first.body);
    recorded(&state, &id, 4).await;
    assert_eq!(
        http(
            &state,
            "POST",
            &format!("/webhooks/{id}/deliveries/missing/redeliver"),
            &token(&state, "owner-a"),
            false,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn migrated_retry_rereads_endpoint_secret_and_stops_after_disconnect() {
    let state = setup().await;
    let mut original = receiver(99, 0).await;
    let mut replacement = receiver(99, 0).await;
    let id = create(&state, &original, None, &["*"]).await;
    assert_eq!(
        http(
            &state,
            "POST",
            &format!("/webhooks/{id}/test"),
            &token(&state, "owner-a"),
            false,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::ACCEPTED
    );
    next(&mut original).await;
    recorded(&state, &id, 1).await;
    assert_eq!(http(&state,"PUT",&format!("/webhooks/{id}"),&token(&state,"owner-a"),false,serde_json::json!({"name":"Changed","url":replacement.url,"secret":"rotated-fixture","events":[dispatch::PUBLIC_PAGE_PUBLISHED_EVENT]})).await.0,StatusCode::NO_CONTENT);
    let retry = next(&mut replacement).await;
    signature(&retry, "rotated-fixture");
    recorded(&state, &id, 2).await;
    assert_eq!(
        http(
            &state,
            "DELETE",
            &format!("/webhooks/{id}"),
            &token(&state, "owner-a"),
            false,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    tokio::time::sleep(std::time::Duration::from_millis(4300)).await;
    assert!(original.rx.try_recv().is_err());
    assert!(replacement.rx.try_recv().is_err());
    assert_eq!(
        http(
            &state,
            "GET",
            &format!("/webhooks/{id}"),
            &token(&state, "owner-a"),
            false,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn migrated_retry_stops_when_inactive_unsubscribed_or_no_longer_public() {
    for stop in ["inactive", "unsubscribed", "unpublished"] {
        let state = setup().await;
        public_page(&state).await;
        let mut receiver = receiver(99, 0).await;
        let id = create(&state, &receiver, None, &[dispatch::PUBLIC_PAGE_PUBLISHED_EVENT]).await;
        dispatch::dispatch_public_page_published(&state.db, "space-a", "target-a").await;
        next(&mut receiver).await;
        recorded(&state, &id, 1).await;
        if stop == "unpublished" {
            sqlx::query("UPDATE documents SET public=0 WHERE id='target-a'")
                .execute(&state.db)
                .await
                .unwrap();
        } else {
            let statement = if stop == "inactive" {
                "UPDATE webhooks SET is_active=0 WHERE id=$1"
            } else {
                "UPDATE webhooks SET events='[\"fixture.other\"]' WHERE id=$1"
            };
            sqlx::query(statement)
                .bind(&id)
                .execute(&state.db)
                .await
                .unwrap();
        }
        tokio::time::sleep(std::time::Duration::from_millis(2300)).await;
        assert_eq!(count(&state, &id).await, 1);
        assert!(receiver.rx.try_recv().is_err());
    }
}

#[test]
fn synthetic_history_correlation_rejects_legacy_malformed_and_non_test_payloads() {
    let id = Uuid::new_v4().to_string();
    let payload = serde_json::json!({"test_id":id,"secret":"never-exposed"}).to_string();
    assert_eq!(delivery_test_id("webhook.test", &payload), Some(id));
    assert!(delivery_test_id("fixture.updated", &payload).is_none());
    for body in [
        "{",
        "{}",
        "{\"test_id\":null}",
        "{\"test_id\":\"invalid\"}",
        "{\"test_id\":123}",
    ] {
        assert!(delivery_test_id("webhook.test", body).is_none());
    }
}

#[tokio::test]
async fn migrated_inactive_redelivery_rejects_without_sending() {
    let state = setup().await;
    let mut receiver = receiver(0, 0).await;
    let id = create(&state, &receiver, None, &["*"]).await;
    let (_, queued) = http(
        &state,
        "POST",
        &format!("/webhooks/{id}/test"),
        &token(&state, "owner-a"),
        false,
        serde_json::json!({}),
    )
    .await;
    next(&mut receiver).await;
    recorded(&state, &id, 1).await;
    sqlx::query("UPDATE webhooks SET is_active=0 WHERE id=$1")
        .bind(&id)
        .execute(&state.db)
        .await
        .unwrap();
    assert_eq!(
        http(
            &state,
            "POST",
            &format!(
                "/webhooks/{id}/deliveries/{}/redeliver",
                queued["delivery_id"].as_str().unwrap()
            ),
            &token(&state, "owner-a"),
            false,
            serde_json::json!({})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(count(&state, &id).await, 1);
    assert!(receiver.rx.try_recv().is_err());
}

#[tokio::test]
async fn migrated_retained_admin_demoted_or_key_revoked_during_pool_wait_cannot_mutate() {
    use axum::extract::FromRequestParts;
    for operation in ["test", "redeliver", "delete"] {
        for revoke_key in [false, true] {
            let state = setup().await;
            let mut receiver = receiver(0, 0).await;
            let id = create(&state, &receiver, None, &["*"]).await;
            let (_, queued) = http(
                &state,
                "POST",
                &format!("/webhooks/{id}/test"),
                &token(&state, "owner-a"),
                false,
                serde_json::json!({}),
            )
            .await;
            next(&mut receiver).await;
            recorded(&state, &id, 1).await;
            let request = if revoke_key {
                key(&state, "bound-admin", Some("owner-a"), false, &["admin"]).await;
                Request::builder()
                    .method("POST")
                    .uri("/webhooks")
                    .header("X-API-Key", "bound-admin")
                    .body(Body::empty())
                    .unwrap()
            } else {
                Request::builder()
                    .method("POST")
                    .uri("/webhooks")
                    .header(
                        "Authorization",
                        format!("Bearer {}", token(&state, "owner-a")),
                    )
                    .body(Body::empty())
                    .unwrap()
            };
            let (mut parts, _) = request.into_parts();
            let retained = AdminUser::from_request_parts(&mut parts, &state)
                .await
                .unwrap();
            // Keep the only migrated DB connection busy AFTER the original extractor succeeds.
            let mut held = state.db.acquire().await.unwrap();
            let handler_state = state.clone();
            let handler_id = id.clone();
            let delivery_id = queued["delivery_id"].as_str().unwrap().to_owned();
            let task = tokio::spawn(async move {
                match operation {
                    "test" => test_webhook(State(handler_state), retained, Path(handler_id))
                        .await
                        .map(|r| r.into_response()),
                    "redeliver" => redeliver(
                        State(handler_state),
                        retained,
                        Path((handler_id, delivery_id)),
                    )
                    .await
                    .map(|r| r.into_response()),
                    _ => delete_webhook(State(handler_state), retained, Path(handler_id))
                        .await
                        .map(|r| r.into_response()),
                }
            });
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            assert!(!task.is_finished());
            sqlx::query(if revoke_key {
                "DELETE FROM api_keys WHERE id='bound-admin'"
            } else {
                "UPDATE users SET role='member' WHERE id='owner-a'"
            })
            .execute(&mut *held)
            .await
            .unwrap();
            drop(held);
            assert!(task.await.unwrap().is_err());
            let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM webhooks WHERE id=$1")
                .bind(&id)
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(exists, 1);
            assert_eq!(count(&state, &id).await, 1);
            assert!(receiver.rx.try_recv().is_err());
        }
    }
}

#[tokio::test]
async fn migrated_delete_wal_writer_demotion_never_deletes_from_stale_snapshot() {
    use axum::extract::FromRequestParts;
    let state = catalog_state(true).await;
    sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'")
        .execute(&state.db)
        .await
        .unwrap();
    let receiver = receiver(0, 0).await;
    let id = create(&state, &receiver, None, &["*"]).await;
    let request = Request::builder()
        .method("DELETE")
        .uri("/webhooks")
        .header(
            "Authorization",
            format!("Bearer {}", token(&state, "owner-a")),
        )
        .body(Body::empty())
        .unwrap();
    let (mut parts, _) = request.into_parts();
    let retained = AdminUser::from_request_parts(&mut parts, &state)
        .await
        .unwrap();
    let mut writer = state.db.begin().await.unwrap();
    sqlx::query("UPDATE users SET role='member' WHERE id='owner-a'")
        .execute(&mut *writer)
        .await
        .unwrap();
    let handler_state = state.clone();
    let handler_id = id.clone();
    let task = tokio::spawn(async move {
        delete_webhook(State(handler_state), retained, Path(handler_id))
            .await
            .map(|r| r.into_response())
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // A stale SQLite read snapshot cannot upgrade over this writer. Either it rejects
    // on the lock or observes the committed demotion; neither outcome may delete.
    writer.commit().await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(6), task)
            .await
            .unwrap()
            .unwrap()
            .is_err()
    );
    let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM webhooks WHERE id=$1")
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(exists, 1);
}

#[path = "postgres_synthetic_transport_fixture.rs"]
mod postgres_synthetic_transport_fixture;
