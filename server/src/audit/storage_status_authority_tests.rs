use super::*;
use crate::spaces::routes::catalog_tests::{auth, catalog_state};
use tower::ServiceExt;

async fn setup() -> AppState {
    let mut state = catalog_state(true).await;
    if let Ok(url) = std::env::var("STARLANE_PRIVATE_ADMIN_STATUS_DATABASE_URL") {
        assert_eq!(crate::db::DatabaseEngine::from_url(&url).unwrap(), crate::db::DatabaseEngine::Postgres);
        let schema = format!("admin_status_{}", uuid::Uuid::new_v4().simple());
        let bootstrap = sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&bootstrap).await.unwrap();
        bootstrap.close().await;
        state.db = sqlx::any::AnyPoolOptions::new().max_connections(2)
            .after_connect(move |conn, _| {
                let statement = format!("SET search_path TO {schema}");
                Box::pin(async move {sqlx::query(&statement).execute(conn).await?; Ok(())})
            }).connect(&url).await.unwrap();
        crate::db::migrate_selected(&state.db, crate::db::DatabaseEngine::Postgres).await.unwrap();
        for actor in ["owner-a", "viewer-a", "editor-a"] {
            sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')")
                .bind(actor).bind(format!("{actor}@admin-status.test")).execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'test','2999-01-01T00:00:00Z')")
                .bind(format!("metadata-{actor}")).bind(actor).execute(&state.db).await.unwrap();
        }
    }
    sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'").execute(&state.db).await.unwrap();
    state
}
fn admin(actor: &str) -> AdminUser {
    let original = auth(actor);
    AdminUser {claims: original.claims, api_key_scopes: original.api_key_scopes}
}
fn token(state: &AppState, actor: &str) -> String {
    // Deliberately stale admin label: current SQL role must decide admission.
    let mut claims = auth(actor).claims;
    claims.role = "admin".into();
    jsonwebtoken::encode(&jsonwebtoken::Header::default(), &claims,
        &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap()
}
async fn key(state: &AppState, id: &str, owner: Option<&str>) {
    sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES($1,$2,$1,$3,$1,$4,$5)")
        .bind(id).bind(owner).bind(crate::auth::api_keys::hash_api_key(id))
        .bind(i64::from(owner.is_none())).bind("[\"admin\"]")
        .execute(&state.db).await.unwrap();
}
async fn http(state: &AppState, path: &str, credential: Option<(&str, bool)>) -> (StatusCode, serde_json::Value) {
    let mut request = axum::http::Request::builder().uri(path);
    if let Some((value, bearer)) = credential {
        request = request.header(if bearer {"Authorization"} else {"X-API-Key"},
            if bearer {format!("Bearer {value}")} else {value.into()});
    }
    let response = router().with_state(state.clone())
        .oneshot(request.body(axum::body::Body::empty()).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024*1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn storage_status_actual_routes_preserve_current_admin_bearer_personal_and_service_keys() {
    let state = setup().await;
    key(&state, "human-admin-key", Some("owner-a")).await;
    key(&state, "service-admin-key", None).await;
    let bearer = token(&state, "owner-a");
    let viewer = token(&state, "viewer-a");
    for path in ["/admin/storage-health", "/admin/storage-integrity"] {
        for credential in [None, Some((viewer.as_str(), true))] {
            let (status, body) = http(&state, path, credential).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{path}: {body}");
            assert!(body.get("database").is_none() && body.get("issues").is_none());
        }
        for credential in [Some((bearer.as_str(), true)), Some(("human-admin-key", false)), Some(("service-admin-key", false))] {
            let (status, body) = http(&state, path, credential).await;
            assert_eq!(status, StatusCode::OK, "{path}: {body}");
            if path == "/admin/storage-health" {
                assert!(body["attachment_capacity"].is_object(), "Normal health capacity absent");
                assert!(body["attachment_capacity"].get("readonly_known_usage").is_none(),
                    "Ordinary human/personal/service health must not acquire historical observation: {body}");
            }
        }
    }
    assert!(current_storage_admin(&state, &admin("owner-a")).await.is_ok());
    sqlx::query("UPDATE users SET role='member' WHERE id='owner-a'").execute(&state.db).await.unwrap();
    assert_eq!(http(&state, "/admin/storage-health", Some((&bearer, true))).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn storage_status_observation_cannot_publish_after_original_authority_changes() {
    for mutation in [
        "UPDATE users SET role='member' WHERE id='owner-a'",
        "UPDATE users SET is_active=0 WHERE id='owner-a'",
        "UPDATE users SET account_kind='guest' WHERE id='owner-a'",
        "DELETE FROM sessions WHERE id='metadata-owner-a'",
        "UPDATE sessions SET expires_at='2000-01-01T00:00:00Z' WHERE id='metadata-owner-a'",
        "UPDATE sessions SET user_id='viewer-a' WHERE id='metadata-owner-a'",
    ] {
        let state = setup().await;
        let original = admin("owner-a");
        let result = observe_storage(&state, &original, async {
            // Executes only after initial current-authority check and before the
            // exact shared publication gate used by both production status routes.
            sqlx::query(mutation).execute(&state.db).await.unwrap();
            Ok("private observation")
        }).await;
        assert!(matches!(result, Err(AppError::Unauthorized(_))), "{mutation}");
    }
}

#[tokio::test]
async fn storage_status_denial_never_starts_observation_and_expired_original_never_rebases() {
    let state = setup().await;
    let ran = std::sync::atomic::AtomicBool::new(false);
    let mut original = admin("owner-a");
    original.claims.exp = chrono::Utc::now().timestamp()-1;
    let result = observe_storage(&state, &original, async {
        ran.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok("private observation")
    }).await;
    assert!(result.is_err());
    assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));
    assert!(observe_storage(&state, &admin("owner-a"), async {Ok("original current session")}).await.is_ok());
}

#[tokio::test]
async fn storage_status_original_keys_cannot_publish_after_revocation_expiry_or_scope_change() {
    use axum::extract::FromRequestParts;
    for service in [false, true] {
        for mutation in [
            "DELETE FROM api_keys WHERE id='original-admin-key'",
            "UPDATE api_keys SET scopes='[\"read\"]' WHERE id='original-admin-key'",
            "UPDATE api_keys SET scopes='[]' WHERE id='original-admin-key'",
            "UPDATE api_keys SET expires_at='2000-01-01T00:00:00Z' WHERE id='original-admin-key'",
            "UPDATE api_keys SET scopes='malformed' WHERE id='original-admin-key'",
        ] {
            let state = setup().await;
            key(&state, "original-admin-key", if service {None} else {Some("owner-a")}).await;
            let (mut parts, _) = axum::http::Request::builder().uri("/admin/storage-health")
                .header("X-API-Key", "original-admin-key").body(()).unwrap().into_parts();
            let original = AdminUser::from_request_parts(&mut parts, &state).await.unwrap();
            assert!(current_storage_admin(&state, &original).await.is_ok());
            let result = observe_storage(&state, &original, async {
                sqlx::query(mutation).execute(&state.db).await.unwrap();
                Ok("private original-key observation")
            }).await;
            assert!(matches!(result, Err(AppError::Unauthorized(_))), "service={service} {mutation}");
        }
    }
}
