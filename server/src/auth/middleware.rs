use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts, Method, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use super::api_keys;
use super::jwt::{self, Claims, TokenType};
use crate::AppState;

/// Extractor that validates the JWT access token from the Authorization header
/// or X-API-Key header. Use as a parameter in route handlers to require auth.
///
/// Example:
/// ```
/// async fn protected_route(auth: AuthUser) -> impl IntoResponse {
///     format!("Hello, {}", auth.claims.email)
/// }
/// ```
pub struct AuthUser {
    pub claims: Claims,
    pub api_key_scopes: Option<Vec<String>>,
}

impl AuthUser {
    fn has_api_key_scope(&self, scope: &str) -> bool {
        self.api_key_scopes
            .as_ref()
            .is_some_and(|scopes| scopes.iter().any(|item| item == scope))
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app_state = AppState::from_ref(state);

        // Try Authorization: Bearer <token>
        if let Some(auth_header) = parts.headers.get(AUTHORIZATION) {
            let auth_str = auth_header.to_str().map_err(|_| AuthError::InvalidToken)?;

            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                let claims = jwt::validate_token(token, &app_state.jwt_secret, TokenType::Access)
                    .map_err(|_| AuthError::InvalidToken)?;
                let claims = validate_bearer_session(&app_state, claims).await?;
                return Ok(AuthUser {
                    claims,
                    api_key_scopes: None,
                });
            }
        }

        // Try X-API-Key header
        if let Some(api_key) = parts.headers.get("X-API-Key") {
            let request_method = parts.method.clone();
            let key_str = api_key.to_str().map_err(|_| AuthError::InvalidToken)?;
            let key_hash = api_keys::hash_api_key(key_str);
            let now = chrono::Utc::now();
            let now_timestamp = now.to_rfc3339();
            let key: Option<(
                String,
                String,
                i32,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<i32>,
            )> = sqlx::query_as(
                "SELECT api_keys.id, api_keys.name, api_keys.is_service_account, api_keys.scopes, \
                        users.id, users.email, users.role, users.is_active \
                 FROM api_keys \
                 LEFT JOIN users ON users.id = api_keys.user_id \
                 WHERE api_keys.key_hash = ?1 \
                   AND (api_keys.expires_at IS NULL OR api_keys.expires_at > ?2)",
            )
            .bind(&key_hash)
            .bind(&now_timestamp)
            .fetch_optional(&app_state.db)
            .await
            .map_err(|_| AuthError::InvalidToken)?;

            if let Some((
                key_id,
                key_name,
                is_service_account,
                scopes_json,
                user_id,
                email,
                role,
                is_active,
            )) = key
            {
                let scopes = api_keys::parse_scope_json(scopes_json.as_deref());
                let has_scope = |scope: &str| scopes.iter().any(|item| item == scope);
                if matches!(request_method, Method::GET | Method::HEAD | Method::OPTIONS) {
                    if !has_scope("read") && !has_scope("admin") {
                        return Err(AuthError::InsufficientScope);
                    }
                } else if !has_scope("write") && !has_scope("admin") {
                    return Err(AuthError::InsufficientScope);
                }

                let _ = sqlx::query("UPDATE api_keys SET last_used_at = ?1 WHERE key_hash = ?2")
                    .bind(&now_timestamp)
                    .bind(&key_hash)
                    .execute(&app_state.db)
                    .await;

                if is_service_account != 0 {
                    if !has_scope("admin") {
                        return Err(AuthError::InsufficientScope);
                    }

                    let service_name = key_name
                        .chars()
                        .map(|ch| {
                            if ch.is_ascii_alphanumeric() {
                                ch.to_ascii_lowercase()
                            } else {
                                '-'
                            }
                        })
                        .collect::<String>();

                    return Ok(AuthUser {
                        claims: Claims {
                            sub: format!("service:{}", key_id),
                            sid: format!("service:{}", key_id),
                            email: format!("{}@service-account.starlane.local", service_name),
                            role: "admin".to_string(),
                            exp: (now + chrono::Duration::minutes(15)).timestamp(),
                            iat: now.timestamp(),
                            token_type: TokenType::Access,
                        },
                        api_key_scopes: Some(scopes),
                    });
                }

                let (Some(user_id), Some(email), Some(role), Some(is_active)) =
                    (user_id, email, role, is_active)
                else {
                    return Err(AuthError::InvalidToken);
                };

                if is_active == 0 {
                    return Err(AuthError::InvalidToken);
                }

                return Ok(AuthUser {
                    claims: Claims {
                        sub: user_id,
                        sid: format!("api-key:{}", key_id),
                        email,
                        role,
                        exp: (now + chrono::Duration::minutes(15)).timestamp(),
                        iat: now.timestamp(),
                        token_type: TokenType::Access,
                    },
                    api_key_scopes: Some(scopes),
                });
            }

            return Err(AuthError::InvalidToken);
        }

        Err(AuthError::MissingToken)
    }
}

async fn validate_bearer_session(
    app_state: &AppState,
    claims: Claims,
) -> Result<Claims, AuthError> {
    let now = chrono::Utc::now().to_rfc3339();
    let session: Option<(String, String, String)> = sqlx::query_as(
        "SELECT users.email, users.role, sessions.session_kind \
         FROM sessions \
         JOIN users ON users.id = sessions.user_id \
         WHERE sessions.id = ?1 \
           AND sessions.user_id = ?2 \
           AND sessions.expires_at > ?3 \
           AND users.is_active = 1 \
         LIMIT 1",
    )
    .bind(&claims.sid)
    .bind(&claims.sub)
    .bind(now)
    .fetch_optional(&app_state.db)
    .await
    .map_err(|_| AuthError::InvalidToken)?;

    let Some((email, role, session_kind)) = session else {
        return Err(AuthError::InvalidToken);
    };

    let now = chrono::Utc::now().to_rfc3339();
    let _ = sqlx::query("UPDATE sessions SET last_seen_at = ?1, expires_at = ?2 WHERE id = ?3")
        .bind(&now)
        .bind(super::routes::session_expiry_at(&session_kind))
        .bind(&claims.sid)
        .execute(&app_state.db)
        .await;

    Ok(Claims {
        email,
        role,
        ..claims
    })
}

// Allow extracting AppState from a reference
use axum::extract::FromRef;

#[derive(Debug)]
pub enum AuthError {
    MissingToken,
    InvalidToken,
    InsufficientScope,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error, message) = match self {
            AuthError::MissingToken => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Missing authentication token",
            ),
            AuthError::InvalidToken => (
                StatusCode::UNAUTHORIZED,
                "unauthorized",
                "Invalid or expired token",
            ),
            AuthError::InsufficientScope => (
                StatusCode::FORBIDDEN,
                "forbidden",
                "API key scope is insufficient",
            ),
        };

        (status, Json(json!({ "error": error, "message": message }))).into_response()
    }
}

/// Extractor that requires the authenticated user to have admin role
pub struct AdminUser {
    pub claims: Claims,
}

impl<S> FromRequestParts<S> for AdminUser
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = AuthError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let auth_user = AuthUser::from_request_parts(parts, state).await?;

        if auth_user.claims.role != "admin" {
            return Err(AuthError::InvalidToken); // Returns 401 for non-admins
        }
        if auth_user.api_key_scopes.is_some() && !auth_user.has_api_key_scope("admin") {
            return Err(AuthError::InsufficientScope);
        }

        Ok(AdminUser {
            claims: auth_user.claims,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use sqlx::any::AnyPoolOptions;

    async fn test_state() -> AppState {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");

        sqlx::query(
            "CREATE TABLE users (
                id TEXT PRIMARY KEY NOT NULL,
                email TEXT NOT NULL,
                role TEXT NOT NULL,
                is_active INTEGER NOT NULL DEFAULT 1
            )",
        )
        .execute(&db)
        .await
        .expect("create users");

        sqlx::query(
            "CREATE TABLE sessions (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL,
                refresh_token_hash TEXT NOT NULL,
                session_kind TEXT NOT NULL DEFAULT 'browser',
                expires_at TEXT NOT NULL
            )",
        )
        .execute(&db)
        .await
        .expect("create sessions");

        sqlx::query(
            "CREATE TABLE api_keys (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT,
                name TEXT NOT NULL,
                key_hash TEXT NOT NULL UNIQUE,
                key_prefix TEXT NOT NULL,
                is_service_account INTEGER NOT NULL DEFAULT 0,
                scopes TEXT,
                last_used_at TEXT,
                expires_at TEXT,
                created_at TEXT NOT NULL
            )",
        )
        .execute(&db)
        .await
        .expect("create api_keys");

        let index_path = std::env::temp_dir().join(format!(
            "starlane-notes-service-key-test-{}",
            uuid::Uuid::new_v4()
        ));

        AppState {
            db,
            jwt_secret: "test-secret-with-enough-length-for-jwt".to_string(),
            base_url: "http://localhost:3000".to_string(),
            search_index: crate::search::SearchIndex::new(index_path).expect("search index"),
            oauth_cache: crate::auth::routes::OAuthStateCache::new(),
        }
    }

    async fn insert_service_key(state: &AppState, plaintext_key: &str, scopes: &[&str]) {
        let scopes_json = serde_json::to_string(scopes).expect("scope json");
        sqlx::query(
            "INSERT INTO api_keys (
                id, user_id, name, key_hash, key_prefix, is_service_account, scopes, created_at
            ) VALUES (?1, NULL, ?2, ?3, ?4, 1, ?5, ?6)",
        )
        .bind("service-key-id")
        .bind("Nightly Import")
        .bind(api_keys::hash_api_key(plaintext_key))
        .bind(api_keys::visible_prefix(plaintext_key))
        .bind(scopes_json)
        .bind("2026-07-08T00:00:00Z")
        .execute(&state.db)
        .await
        .expect("insert service key");
    }

    async fn insert_user_session(state: &AppState, user_id: &str, session_id: &str, active: bool) {
        sqlx::query("INSERT INTO users (id, email, role, is_active) VALUES (?1, ?2, ?3, ?4)")
            .bind(user_id)
            .bind("eagle@example.com")
            .bind("admin")
            .bind(if active { 1 } else { 0 })
            .execute(&state.db)
            .await
            .expect("insert user");

        sqlx::query(
            "INSERT INTO sessions (id, user_id, refresh_token_hash, expires_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(session_id)
        .bind(user_id)
        .bind("refresh-hash")
        .bind((chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339())
        .execute(&state.db)
        .await
        .expect("insert session");
    }

    fn bearer_request(token: &str) -> axum::http::request::Parts {
        let request = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/auth/me")
            .header(AUTHORIZATION, format!("Bearer {token}"))
            .body(())
            .expect("request");
        let (parts, _) = request.into_parts();
        parts
    }

    fn access_token_for_session(state: &AppState, user_id: &str, session_id: &str) -> String {
        let now = chrono::Utc::now();
        let claims = Claims {
            sub: user_id.to_string(),
            sid: session_id.to_string(),
            email: "stale@example.com".to_string(),
            role: "member".to_string(),
            exp: (now + chrono::Duration::minutes(15)).timestamp(),
            iat: now.timestamp(),
            token_type: TokenType::Access,
        };
        jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .expect("access token")
    }

    #[tokio::test]
    async fn service_api_key_authenticates_as_admin_principal() {
        let state = test_state().await;
        let plaintext_key = api_keys::generate_api_key();
        insert_service_key(&state, &plaintext_key, &["admin"]).await;

        let request = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/admin/users")
            .header("X-API-Key", plaintext_key)
            .body(())
            .expect("request");
        let (mut parts, _) = request.into_parts();

        let auth = AuthUser::from_request_parts(&mut parts, &state)
            .await
            .expect("authenticated service key");

        assert_eq!(auth.claims.sub, "service:service-key-id");
        assert_eq!(
            auth.claims.email,
            "nightly-import@service-account.starlane.local"
        );
        assert_eq!(auth.claims.role, "admin");
        assert_eq!(auth.api_key_scopes, Some(vec!["admin".to_string()]));
    }

    #[tokio::test]
    async fn service_api_key_without_admin_scope_is_rejected() {
        let state = test_state().await;
        let plaintext_key = api_keys::generate_api_key();
        insert_service_key(&state, &plaintext_key, &["read"]).await;

        let request = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/admin/users")
            .header("X-API-Key", plaintext_key)
            .body(())
            .expect("request");
        let (mut parts, _) = request.into_parts();

        let result = AuthUser::from_request_parts(&mut parts, &state).await;

        assert!(matches!(result, Err(AuthError::InsufficientScope)));
    }

    #[tokio::test]
    async fn auth_error_response_uses_documented_json_envelope() {
        let response = AuthError::InsufficientScope.into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("auth error body");
        let body: serde_json::Value = serde_json::from_slice(&bytes).expect("auth error JSON");

        assert_eq!(body["error"], "forbidden");
        assert_eq!(body["message"], "API key scope is insufficient");
    }

    #[tokio::test]
    async fn bearer_access_token_requires_active_session() {
        let state = test_state().await;
        insert_user_session(&state, "user-1", "session-1", true).await;
        let token = access_token_for_session(&state, "user-1", "session-1");
        let mut parts = bearer_request(&token);

        let auth = AuthUser::from_request_parts(&mut parts, &state)
            .await
            .expect("active session should authenticate");

        assert_eq!(auth.claims.sub, "user-1");
        assert_eq!(auth.claims.sid, "session-1");
        assert_eq!(auth.claims.email, "eagle@example.com");
        assert_eq!(auth.claims.role, "admin");
    }

    #[tokio::test]
    async fn bearer_access_token_rejects_revoked_session() {
        let state = test_state().await;
        insert_user_session(&state, "user-1", "session-1", true).await;
        sqlx::query("DELETE FROM sessions WHERE id = ?1")
            .bind("session-1")
            .execute(&state.db)
            .await
            .expect("delete session");
        let token = access_token_for_session(&state, "user-1", "session-1");
        let mut parts = bearer_request(&token);

        let result = AuthUser::from_request_parts(&mut parts, &state).await;

        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }

    #[tokio::test]
    async fn bearer_access_token_rejects_inactive_user() {
        let state = test_state().await;
        insert_user_session(&state, "user-1", "session-1", false).await;
        let token = access_token_for_session(&state, "user-1", "session-1");
        let mut parts = bearer_request(&token);

        let result = AuthUser::from_request_parts(&mut parts, &state).await;

        assert!(matches!(result, Err(AuthError::InvalidToken)));
    }
}
