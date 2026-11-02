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
        let operation = ApiKeyRequestOperation::for_method(&parts.method);
        authenticate_request(parts, &app_state, operation).await
    }
}

/// An operation adapter may request a read ceiling explicitly. Ordinary routes
/// continue to derive their API-key ceiling from the actual HTTP method.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ApiKeyRequestOperation {
    Read,
    Write,
}
impl ApiKeyRequestOperation {
    pub(crate) fn for_method(method: &Method) -> Self {
        if matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS) {
            Self::Read
        } else {
            Self::Write
        }
    }
}
pub(crate) async fn authenticate_request(
    parts: &mut Parts,
    app_state: &AppState,
    operation: ApiKeyRequestOperation,
) -> Result<AuthUser, AuthError> {
    // Try Authorization: Bearer <token>
    if let Some(auth_header) = parts.headers.get(AUTHORIZATION) {
        let auth_str = auth_header.to_str().map_err(|_| AuthError::InvalidToken)?;

        if let Some(token) = auth_str.strip_prefix("Bearer ") {
            let claims = jwt::validate_token(token, &app_state.jwt_secret, TokenType::Access)
                .map_err(|_| AuthError::InvalidToken)?;
            let claims = validate_bearer_session(app_state, claims).await?;
            return Ok(AuthUser {
                claims,
                api_key_scopes: None,
            });
        }
    }

    // Try X-API-Key header
    if let Some(api_key) = parts.headers.get("X-API-Key") {
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
            Option<String>,
        )> = sqlx::query_as(
            "SELECT api_keys.id, api_keys.name, api_keys.is_service_account, api_keys.scopes, \
                    users.id, users.email, users.role, users.is_active, users.account_kind \
             FROM api_keys \
             LEFT JOIN users ON users.id = api_keys.user_id \
             WHERE api_keys.key_hash = $1 \
               AND (api_keys.expires_at IS NULL OR api_keys.expires_at > $2)",
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
            account_kind,
        )) = key
        {
            // Service accounts were introduced with explicit scopes. They
            // cannot inherit the legacy NULL user-key administrator fallback.
            if is_service_account != 0 && scopes_json.is_none() {
                return Err(AuthError::InsufficientScope);
            }
            let scopes = api_keys::parse_scope_json(scopes_json.as_deref());
            let has_scope = |scope: &str| scopes.iter().any(|item| item == scope);
            if operation == ApiKeyRequestOperation::Read {
                if !has_scope("read") && !has_scope("admin") {
                    return Err(AuthError::InsufficientScope);
                }
            } else if !has_scope("write") && !has_scope("admin") {
                return Err(AuthError::InsufficientScope);
            }

            let _ = sqlx::query("UPDATE api_keys SET last_used_at = $1 WHERE key_hash = $2")
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

            if is_active == 0 || !matches!(account_kind.as_deref(), Some("member" | "guest")) {
                return Err(AuthError::InvalidToken);
            }
            // Claims are presentation/extractor hints, never classification authority.
            // A retained guest role cannot create a human administrator extractor.
            let role = if account_kind.as_deref() == Some("guest") { "viewer".into() } else { role };

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

pub(crate) async fn validate_bearer_session(
    app_state: &AppState,
    claims: Claims,
) -> Result<Claims, AuthError> {
    let now = chrono::Utc::now().to_rfc3339();
    let session: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT users.email, users.role, sessions.session_kind, users.account_kind \
         FROM sessions \
         JOIN users ON users.id = sessions.user_id \
         WHERE sessions.id = $1 \
           AND sessions.user_id = $2 \
           AND sessions.expires_at > $3 \
           AND users.is_active = 1 \
         LIMIT 1",
    )
    .bind(&claims.sid)
    .bind(&claims.sub)
    .bind(now)
    .fetch_optional(&app_state.db)
    .await
    .map_err(|_| AuthError::InvalidToken)?;

    let Some((email, role, session_kind, account_kind)) = session else {
        return Err(AuthError::InvalidToken);
    };

    if !matches!(account_kind.as_str(), "member" | "guest") { return Err(AuthError::InvalidToken); }
    let role = if account_kind == "guest" { "viewer".into() } else { role };

    let now = chrono::Utc::now().to_rfc3339();
    #[cfg(target_os="linux")]
    let historical_observation=crate::storage::restoration::linux::historical_readonly_auth_observation(&app_state.db).map_err(|_|AuthError::InvalidToken)?;
    #[cfg(not(target_os="linux"))]
    let historical_observation:Option<()>=None;
    if historical_observation.is_none() {
    let _ = sqlx::query("UPDATE sessions SET last_seen_at = $1, expires_at = $2 WHERE id = $3")
        .bind(&now)
        .bind(super::routes::session_expiry_at(&session_kind))
        .bind(&claims.sid)
        .execute(&app_state.db)
        .await;
    }

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
    /// Exact evidence retained from the verified extractor; current helpers
    /// reread the bound key rather than trusting these possibly stale scopes.
    pub api_key_scopes: Option<Vec<String>>,
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
            api_key_scopes: auth_user.api_key_scopes,
        })
    }
}

/// Instance administration and service administration are separate authorities.
/// Neither this proof nor a service key grants document/private participation.
#[derive(Clone, Debug)]
pub enum CurrentAdminAuthority {
    HumanInstanceAdmin { user_id: String },
    ServiceAdminKey(crate::spaces::access::ServiceKeyCapability),
}

/// Revalidate only already verified AdminUser evidence on the caller connection.
/// Consumers must call this after waits and before commit/response; existing
/// handlers have not yet adopted this foundation's transaction checks.
pub async fn require_current_admin(
    conn: &mut sqlx::AnyConnection,
    admin: &AdminUser,
) -> Result<CurrentAdminAuthority, crate::auth::routes::AppError> {
    use crate::spaces::access;
    // Preserve the exact verified identity/key evidence. No caller may construct
    // a human from a service identity or a stale role/scope label here.
    let auth = AuthUser {
        claims: admin.claims.clone(),
        api_key_scopes: admin.api_key_scopes.clone(),
    };
    let authority = match access::principal_from_auth(&auth)? {
        access::Principal::ServiceKey { key_id } => {
            match access::resolve_current_key(conn, key_id).await? {
                access::CurrentKeyCapability::Service(capability) => {
                    Ok(CurrentAdminAuthority::ServiceAdminKey(capability))
                }
                _ => Err(crate::auth::routes::AppError::Unauthorized(
                    "Admin credential unavailable".into(),
                )),
            }
        }
        _ => Ok(CurrentAdminAuthority::HumanInstanceAdmin {
            user_id: access::require_current_human_instance_admin(conn, &auth).await?,
        }),
    }?;
    // No further DB wait follows this final original JWT/key-token expiry check.
    access::principal_from_auth(&auth)?;
    Ok(authority)
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
            "CREATE TABLE users (account_kind TEXT NOT NULL DEFAULT 'member',
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
                client_platform TEXT NOT NULL DEFAULT 'web',
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
            ws_rooms: crate::sync::WsRooms::default(),
        }
    }

    async fn insert_service_key(state: &AppState, plaintext_key: &str, scopes: &[&str]) {
        let scopes_json = serde_json::to_string(scopes).expect("scope json");
        sqlx::query(
            "INSERT INTO api_keys (
                id, user_id, name, key_hash, key_prefix, is_service_account, scopes, created_at
            ) VALUES ($1, NULL, $2, $3, $4, 1, $5, $6)",
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
        sqlx::query("INSERT INTO users (id, email, role, is_active) VALUES ($1, $2, $3, $4)")
            .bind(user_id)
            .bind("eagle@example.com")
            .bind("admin")
            .bind(if active { 1 } else { 0 })
            .execute(&state.db)
            .await
            .expect("insert user");

        sqlx::query(
            "INSERT INTO sessions (id, user_id, refresh_token_hash, expires_at) VALUES ($1, $2, $3, $4)",
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
    async fn corrupt_or_missing_service_scope_policy_cannot_authenticate_as_admin() {
        for raw in [
            Some("{"),
            Some("null"),
            Some("[1]"),
            Some("[\"admin\",\"unknown\"]"),
            None,
        ] {
            let state = test_state().await;
            let plaintext_key = api_keys::generate_api_key();
            insert_service_key(&state, &plaintext_key, &["admin"]).await;
            sqlx::query("UPDATE api_keys SET scopes=$1 WHERE id='service-key-id'")
                .bind(raw)
                .execute(&state.db)
                .await
                .unwrap();
            let (mut parts, _) = Request::builder()
                .uri("/api/v1/admin/system")
                .header("X-API-Key", &plaintext_key)
                .body(())
                .unwrap()
                .into_parts();
            assert!(
                matches!(
                    AuthUser::from_request_parts(&mut parts, &state).await,
                    Err(AuthError::InsufficientScope)
                ),
                "{raw:?}"
            );
        }
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
        sqlx::query("DELETE FROM sessions WHERE id = $1")
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

#[cfg(test)]
mod current_admin_tests {
    use super::*;
    use crate::spaces::routes::catalog_tests::{auth, catalog_state};
    use axum::{extract::State, routing::get, Json, Router};
    use tower::ServiceExt;
    async fn setup(wal: bool) -> AppState {
        let state = catalog_state(wal).await;
        sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        state
    }
    fn token(state: &AppState, actor: &str) -> String {
        jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &auth(actor).claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap()
    }
    async fn key(
        state: &AppState,
        id: &str,
        owner: Option<&str>,
        service: bool,
        scopes: Option<&str>,
    ) {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES($1,$2,$1,$3,$1,$4,$5)").bind(id).bind(owner).bind(crate::auth::api_keys::hash_api_key(id)).bind(i64::from(service)).bind(scopes).execute(&state.db).await.unwrap();
    }
    async fn extract(state: &AppState, id: &str, bearer: bool) -> Result<AdminUser, AuthError> {
        let request = axum::http::Request::builder()
            .uri("/admin-proof")
            .header(
                if bearer { "Authorization" } else { "X-API-Key" },
                if bearer {
                    format!("Bearer {id}")
                } else {
                    id.into()
                },
            )
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();
        AdminUser::from_request_parts(&mut parts, state).await
    }
    async fn endpoint(
        State(state): State<AppState>,
        admin: AdminUser,
    ) -> Result<Json<serde_json::Value>, crate::auth::routes::AppError> {
        let mut conn = state
            .db
            .acquire()
            .await
            .map_err(crate::auth::routes::AppError::internal)?;
        let captured = admin.api_key_scopes.is_some();
        let value = match require_current_admin(&mut conn, &admin).await? {
            CurrentAdminAuthority::HumanInstanceAdmin { user_id } => {
                serde_json::json!({"kind":"human","id":user_id,"key_evidence":captured})
            }
            CurrentAdminAuthority::ServiceAdminKey(cap) => {
                serde_json::json!({"kind":"service","id":cap.key_id,"key_evidence":captured})
            }
        };
        Ok(Json(value))
    }
    async fn http(
        state: &AppState,
        method: &str,
        id: &str,
        bearer: bool,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        let request = axum::http::Request::builder()
            .method(method)
            .uri("/admin-proof")
            .header(
                if bearer { "Authorization" } else { "X-API-Key" },
                if bearer {
                    format!("Bearer {id}")
                } else {
                    id.into()
                },
            )
            .body(axum::body::Body::empty())
            .unwrap();
        let response = Router::new()
            .route("/admin-proof", get(endpoint).post(endpoint))
            .with_state(state.clone())
            .oneshot(request)
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }
    #[tokio::test]
    async fn current_admin_actual_http_preserves_status_methods_and_distinct_service() {
        let state = setup(false).await;
        let bearer = token(&state, "owner-a");
        key(
            &state,
            "human-admin",
            Some("owner-a"),
            false,
            Some("[\"admin\"]"),
        )
        .await;
        key(
            &state,
            "human-write",
            Some("owner-a"),
            false,
            Some("[\"write\"]"),
        )
        .await;
        key(
            &state,
            "human-read",
            Some("owner-a"),
            false,
            Some("[\"read\"]"),
        )
        .await;
        key(&state, "service-admin", None, true, Some("[\"admin\"]")).await;
        for method in ["GET", "POST"] {
            let (status, body) = http(&state, method, &bearer, true).await;
            assert_eq!(status, axum::http::StatusCode::OK);
            assert_eq!(body["kind"], "human");
            assert_eq!(body["key_evidence"], false);
            let (status, body) = http(&state, method, "human-admin", false).await;
            assert_eq!(status, axum::http::StatusCode::OK);
            assert_eq!(body["id"], "owner-a");
            assert_eq!(body["key_evidence"], true);
            let (status, body) = http(&state, method, "service-admin", false).await;
            assert_eq!(status, axum::http::StatusCode::OK);
            assert_eq!(body["kind"], "service");
            assert_eq!(body["id"], "service-admin");
            assert_eq!(body["key_evidence"], true);
            for id in ["human-write", "human-read"] {
                assert_eq!(
                    http(&state, method, id, false).await.0,
                    axum::http::StatusCode::FORBIDDEN
                );
            }
        }
        assert_eq!(
            http(&state, "GET", &token(&state, "editor-a"), true)
                .await
                .0,
            axum::http::StatusCode::UNAUTHORIZED
        );
    }
    #[tokio::test]
    async fn current_admin_retained_key_evidence_observes_live_owner_scope_and_type() {
        for update in [
            "UPDATE api_keys SET scopes='[\"read\"]' WHERE id='bound-admin'",
            "UPDATE api_keys SET user_id='editor-a' WHERE id='bound-admin'",
            "UPDATE api_keys SET is_service_account=1 WHERE id='bound-admin'",
            "UPDATE api_keys SET expires_at='2000-01-01T00:00:00Z' WHERE id='bound-admin'",
            "DELETE FROM api_keys WHERE id='bound-admin'",
            "UPDATE users SET role='member' WHERE id='owner-a'",
            "UPDATE users SET is_active=0 WHERE id='owner-a'",
        ] {
            let state = setup(false).await;
            key(
                &state,
                "bound-admin",
                Some("owner-a"),
                false,
                Some("[\"admin\"]"),
            )
            .await;
            let admin = extract(&state, "bound-admin", false).await.unwrap();
            assert!(admin.api_key_scopes.is_some());
            sqlx::query(update).execute(&state.db).await.unwrap();
            let mut conn = state.db.acquire().await.unwrap();
            assert!(
                require_current_admin(&mut conn, &admin).await.is_err(),
                "{update}"
            );
        }
    }
    #[tokio::test]
    async fn current_admin_service_proof_never_becomes_human_or_private_participant() {
        let state = setup(false).await;
        key(&state, "service-bound", None, true, Some("[\"admin\"]")).await;
        let admin = extract(&state, "service-bound", false).await.unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        assert!(matches!(
            require_current_admin(&mut conn, &admin).await.unwrap(),
            CurrentAdminAuthority::ServiceAdminKey(_)
        ));
        let actual = AuthUser {
            claims: admin.claims.clone(),
            api_key_scopes: admin.api_key_scopes.clone(),
        };
        assert!(crate::spaces::access::resolve_page_access(
            &mut conn,
            crate::spaces::access::principal_from_auth(&actual).unwrap(),
            "space-a",
            "missing-page",
            crate::spaces::access::PageOperation::View,
            crate::spaces::access::Purpose::Routine
        )
        .await
        .is_err());
        drop(conn);
        for raw in [
            None,
            Some("malformed"),
            Some("[\"read\"]"),
            Some("[\"admin\",\"unknown\"]"),
        ] {
            sqlx::query("UPDATE api_keys SET scopes=$1 WHERE id='service-bound'")
                .bind(raw)
                .execute(&state.db)
                .await
                .unwrap();
            let mut conn = state.db.acquire().await.unwrap();
            assert!(require_current_admin(&mut conn, &admin).await.is_err());
        }
        sqlx::query("UPDATE api_keys SET scopes='[\"admin\"]',is_service_account=0,user_id='owner-a' WHERE id='service-bound'").execute(&state.db).await.unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        assert!(require_current_admin(&mut conn, &admin).await.is_err());
    }
    #[tokio::test]
    async fn current_admin_actual_global_owner_role_does_not_grant_instance_admin() {
        let state = setup(false).await;
        let mut admin = extract(&state, &token(&state, "owner-a"), true)
            .await
            .unwrap();
        sqlx::query("UPDATE users SET role='member' WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES('admin-label','Admin label')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('admin-label-member','admin-label','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query(
            "INSERT INTO group_global_roles(group_id,role_id) VALUES('admin-label','owner')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        assert!(require_current_admin(&mut conn, &admin).await.is_err());
        drop(conn);
        sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'")
            .execute(&state.db)
            .await
            .unwrap();
        admin.claims.exp = 1;
        let mut conn = state.db.acquire().await.unwrap();
        assert!(require_current_admin(&mut conn, &admin).await.is_err());
    }
    #[tokio::test]
    async fn current_admin_same_transaction_observes_local_revocation_without_pool_reentry() {
        let state = setup(false).await;
        let admin = extract(&state, &token(&state, "owner-a"), true)
            .await
            .unwrap();
        let mut tx = state.db.begin().await.unwrap();
        require_current_admin(&mut tx, &admin).await.unwrap();
        sqlx::query("UPDATE users SET role='member' WHERE id='owner-a'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(require_current_admin(&mut tx, &admin).await.is_err());
        tx.rollback().await.unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        assert!(require_current_admin(&mut conn, &admin).await.is_ok());
    }
    #[tokio::test]
    async fn current_admin_true_wal_wait_rechecks_session_key_and_account() {
        for (mode, sql) in [
            ("bearer", "DELETE FROM sessions WHERE user_id='owner-a'"),
            (
                "bearer",
                "UPDATE users SET role='member' WHERE id='owner-a'",
            ),
            ("bearer", "UPDATE users SET is_active=0 WHERE id='owner-a'"),
            (
                "key",
                "UPDATE api_keys SET scopes='[\"read\"]' WHERE id='wait-key'",
            ),
            ("service", "DELETE FROM api_keys WHERE id='wait-key'"),
        ] {
            let state = setup(true).await;
            let admin = if mode == "bearer" {
                extract(&state, &token(&state, "owner-a"), true)
                    .await
                    .unwrap()
            } else {
                key(
                    &state,
                    "wait-key",
                    (mode == "key").then_some("owner-a"),
                    mode == "service",
                    Some("[\"admin\"]"),
                )
                .await;
                extract(&state, "wait-key", false).await.unwrap()
            };
            let mut blocker = state.db.begin().await.unwrap();
            sqlx::query("UPDATE spaces SET id=id WHERE id='space-a'")
                .execute(&mut *blocker)
                .await
                .unwrap();
            sqlx::query(sql).execute(&mut *blocker).await.unwrap();
            let copy = state.clone();
            let task = tokio::spawn(async move {
                let mut tx = copy.db.begin().await.unwrap();
                crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &["space-a"])
                    .await
                    .unwrap();
                let result = require_current_admin(&mut tx, &admin).await;
                tx.rollback().await.unwrap();
                result
            });
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!task.is_finished());
            blocker.commit().await.unwrap();
            assert!(task.await.unwrap().is_err(), "{mode}: {sql}");
        }
    }
    #[tokio::test]
    async fn current_admin_true_wait_rejects_original_token_expiry() {
        let state = setup(true).await;
        let mut claims = auth("owner-a").claims;
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let admin = extract(&state, &token, true).await.unwrap();
        let mut blocker = state.db.begin().await.unwrap();
        sqlx::query("UPDATE spaces SET id=id WHERE id='space-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let copy = state.clone();
        let task = tokio::spawn(async move {
            let mut tx = copy.db.begin().await.unwrap();
            crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &["space-a"])
                .await
                .unwrap();
            let result = require_current_admin(&mut tx, &admin).await;
            tx.rollback().await.unwrap();
            result
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!task.is_finished());
        while chrono::Utc::now().timestamp() <= claims.exp {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        blocker.commit().await.unwrap();
        assert!(task.await.unwrap().is_err());
        let session: i64=sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id='owner-a' AND expires_at > CURRENT_TIMESTAMP").fetch_one(&state.db).await.unwrap();
        assert_eq!(
            session, 1,
            "session survives, original short-lived token expired"
        );
    }
    #[tokio::test]
    async fn current_admin_legacy_human_null_scopes_preserved_and_current_helper_read_only() {
        let state = setup(false).await;
        key(&state, "legacy-admin", Some("owner-a"), false, None).await;
        let admin = extract(&state, "legacy-admin", false).await.unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let before: String=sqlx::query_scalar("SELECT json_object('role',u.role,'active',u.is_active,'owner',k.user_id,'scopes',k.scopes,'last_used',k.last_used_at,'service',k.is_service_account) FROM api_keys k JOIN users u ON u.id=k.user_id WHERE k.id='legacy-admin'").fetch_one(&mut *conn).await.unwrap();
        assert!(matches!(
            require_current_admin(&mut conn, &admin).await.unwrap(),
            CurrentAdminAuthority::HumanInstanceAdmin { .. }
        ));
        let after: String=sqlx::query_scalar("SELECT json_object('role',u.role,'active',u.is_active,'owner',k.user_id,'scopes',k.scopes,'last_used',k.last_used_at,'service',k.is_service_account) FROM api_keys k JOIN users u ON u.id=k.user_id WHERE k.id='legacy-admin'").fetch_one(&mut *conn).await.unwrap();
        assert_eq!(before, after);
        drop(conn);
        key(&state, "service-null", None, true, None).await;
        assert!(extract(&state, "service-null", false).await.is_err());
    }
}
