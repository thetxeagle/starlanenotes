use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use openidconnect::url::Url;
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use webauthn_rs::prelude::{
    CredentialID, Passkey, PasskeyAuthentication, PasskeyRegistration, PublicKeyCredential,
    RegisterPublicKeyCredential, Webauthn, WebauthnBuilder,
};

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{
    api_keys, jwt,
    middleware::{AdminUser, AuthUser},
    oauth, oidc, password, totp,
};
use crate::storage;
use crate::AppState;

const STARLANE_WEBAUTHN_ORIGINS_ENV: &str = "STARLANE_WEBAUTHN_ORIGINS";
const NATIVE_APP_WEBAUTHN_ORIGINS: &[&str] = &[
    "http://tauri.localhost",
    "https://tauri.localhost",
    "tauri://localhost",
];

/// In-memory cache for OAuth/OIDC state (CSRF tokens, PKCE verifiers, nonces).
/// Entries expire after 10 minutes.
#[derive(Clone, Default)]
pub struct OAuthStateCache {
    inner: Arc<RwLock<HashMap<String, OAuthStateEntry>>>,
}

struct OAuthStateEntry {
    pkce_verifier: Option<oauth2::PkceCodeVerifier>,
    nonce: Option<openidconnect::Nonce>,
    created_at: std::time::Instant,
}

impl OAuthStateCache {
    pub fn new() -> Self {
        Self::default()
    }

    async fn store_oauth(&self, csrf_token: &str, pkce_verifier: oauth2::PkceCodeVerifier) {
        let mut cache = self.inner.write().await;
        // Clean expired entries (older than 10 minutes)
        cache.retain(|_, v| v.created_at.elapsed().as_secs() < 600);
        cache.insert(
            csrf_token.to_string(),
            OAuthStateEntry {
                pkce_verifier: Some(pkce_verifier),
                nonce: None,
                created_at: std::time::Instant::now(),
            },
        );
    }

    async fn store_oidc(&self, csrf_token: &str, nonce: openidconnect::Nonce) {
        let mut cache = self.inner.write().await;
        cache.retain(|_, v| v.created_at.elapsed().as_secs() < 600);
        cache.insert(
            csrf_token.to_string(),
            OAuthStateEntry {
                pkce_verifier: None,
                nonce: Some(nonce),
                created_at: std::time::Instant::now(),
            },
        );
    }

    async fn take_oauth(&self, csrf_token: &str) -> Option<oauth2::PkceCodeVerifier> {
        let mut cache = self.inner.write().await;
        cache.remove(csrf_token).and_then(|e| e.pkce_verifier)
    }

    async fn take_oidc(&self, csrf_token: &str) -> Option<openidconnect::Nonce> {
        let mut cache = self.inner.write().await;
        cache.remove(csrf_token).and_then(|e| e.nonce)
    }
}

// ============================================================
// Route Definitions
// ============================================================

pub fn router() -> Router<AppState> {
    // Rate limit: 10 burst, refill every 6 seconds per IP for auth endpoints
    let auth_governor_conf = tower_governor::governor::GovernorConfigBuilder::default()
        .const_per_second(6)
        .const_burst_size(10)
        .key_extractor(tower_governor::key_extractor::SmartIpKeyExtractor)
        .finish()
        .expect("Invalid rate limiter config");

    // Rate-limited auth routes (login, register, refresh, password reset, 2FA verify)
    let rate_limited_routes = Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh_token))
        .route("/auth/password-reset/request", post(request_password_reset))
        .route("/auth/password-reset/confirm", post(confirm_password_reset))
        .route("/auth/passkeys/login/start", post(passkey_login_start))
        .route("/auth/passkeys/login/finish", post(passkey_login_finish))
        .route("/auth/native-handoff/start", post(native_handoff_start))
        .route("/auth/native-handoff/exchange", post(native_handoff_exchange))
        .route("/auth/2fa/totp/verify", post(totp_verify))
        .layer(
            tower_governor::GovernorLayer::new(auth_governor_conf)
                .error_handler(auth_rate_limit_error_response),
        );

    // Admin routes
    let admin_routes = Router::new()
        .route("/admin/users", get(list_users).post(create_admin_user))
        .route(
            "/admin/users/bulk-lifecycle",
            post(bulk_update_user_lifecycle),
        )
        .route("/admin/users/{user_id}", put(update_user_lifecycle))
        .route(
            "/admin/users/{user_id}/password-reset",
            post(reset_admin_user_password),
        )
        .route(
            "/admin/users/{user_id}/sessions",
            get(list_admin_user_sessions),
        )
        .route(
            "/admin/users/{user_id}/sessions/{session_id}",
            axum::routing::delete(revoke_admin_user_session),
        )
        .route(
            "/admin/api-keys",
            get(list_service_api_keys).post(create_service_api_key),
        )
        .route(
            "/admin/api-keys/{key_id}",
            axum::routing::delete(revoke_service_api_key),
        )
        .route(
            "/admin/config",
            get(list_admin_config).post(upsert_admin_config),
        )
        .route(
            "/admin/config/{key}",
            axum::routing::delete(delete_admin_config),
        );

    // Non-rate-limited auth routes
    let auth_routes = Router::new()
        .route("/auth/logout", post(logout))
        .route("/auth/oauth/{provider}", get(oauth_redirect))
        .route("/auth/oauth/{provider}/callback", get(oauth_callback))
        .route("/auth/oidc", get(oidc_redirect))
        .route("/auth/oidc/callback", get(oidc_callback))
        .route("/auth/2fa/totp/setup", post(totp_setup))
        .route("/auth/2fa/totp/disable", post(totp_disable))
        .route("/auth/passkeys", get(list_passkeys))
        .route(
            "/auth/passkeys/register/start",
            post(passkey_register_start),
        )
        .route(
            "/auth/passkeys/register/finish",
            post(passkey_register_finish),
        )
        .route("/auth/passkeys/{passkey_id}", delete(delete_passkey))
        .route("/auth/native-handoff/complete", post(native_handoff_complete))
        .route("/auth/me", get(me).put(update_me))
        .route("/auth/sessions", get(list_sessions))
        .route(
            "/auth/sessions/{session_id}",
            axum::routing::delete(revoke_session),
        )
        .route("/auth/api-keys", get(list_api_keys).post(create_api_key))
        .route(
            "/auth/api-keys/{key_id}",
            axum::routing::delete(revoke_api_key),
        )
        .route("/auth/config", get(auth_config))
        .route("/users/directory", get(list_user_directory));

    rate_limited_routes.merge(auth_routes).merge(admin_routes)
}

// ============================================================
// Request/Response Types
// ============================================================

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub display_name: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub totp_code: Option<String>,
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Deserialize)]
pub struct PasswordResetRequest {
    pub email: String,
}

#[derive(Deserialize)]
pub struct PasswordResetConfirmRequest {
    pub token: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct PasswordResetRequestResponse {
    pub message: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
    pub user: UserResponse,
}

#[derive(Serialize)]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub username: Option<String>,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub role: String,
    pub totp_enabled: bool,
}

#[derive(Deserialize)]
pub struct UpdateProfileRequest {
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub username: Option<String>,
}

#[derive(Deserialize)]
pub struct OAuthCallbackParams {
    pub code: String,
    pub state: String,
}

#[derive(Deserialize)]
pub struct TotpVerifyRequest {
    pub code: String,
}

#[derive(Serialize)]
pub struct AuthConfigResponse {
    pub google_enabled: bool,
    pub github_enabled: bool,
    pub oidc_enabled: bool,
    pub oidc_display_name: String,
    pub require_2fa: bool,
    pub allow_public_signups: bool,
    pub setup_completed: bool,
    pub site_name: String,
    pub max_upload_size_mb: usize,
}

#[derive(Deserialize)]
pub struct ConfigUpsertRequest {
    pub key: String,
    pub value: String,
}

#[derive(Deserialize)]
pub struct UserLifecycleRequest {
    pub role: Option<String>,
    pub is_active: Option<bool>,
}

#[derive(Deserialize)]
pub struct BulkUserLifecycleRequest {
    pub user_ids: Vec<String>,
    pub action: String,
}

#[derive(Serialize)]
pub struct BulkUserLifecycleSkippedUser {
    pub user_id: String,
    pub reason: String,
}

#[derive(Serialize)]
pub struct BulkUserLifecycleResponse {
    pub action: String,
    pub requested_count: usize,
    pub updated_count: usize,
    pub skipped_count: usize,
    pub skipped: Vec<BulkUserLifecycleSkippedUser>,
}

#[derive(Deserialize)]
pub struct AdminCreateUserRequest {
    pub email: String,
    pub display_name: String,
    pub role: Option<String>,
}

#[derive(Deserialize)]
pub struct ApiKeyCreateRequest {
    pub name: String,
    pub expires_at: Option<String>,
    pub scopes: Option<Vec<String>>,
}

#[derive(Serialize)]
pub struct ApiKeyResponse {
    pub id: String,
    pub name: String,
    pub key_prefix: String,
    pub is_service_account: bool,
    pub scopes: Vec<String>,
    pub last_used_at: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct ApiKeyCreateResponse {
    #[serde(flatten)]
    pub api_key: ApiKeyResponse,
    pub key: String,
}

#[derive(Deserialize)]
pub struct PasskeyRegisterStartRequest {
    pub name: Option<String>,
}

#[derive(Serialize)]
pub struct PasskeyChallengeResponse {
    pub challenge_id: String,
    pub public_key: Value,
}

#[derive(Deserialize)]
pub struct PasskeyRegisterFinishRequest {
    pub challenge_id: String,
    pub name: Option<String>,
    pub credential: Value,
}

#[derive(Deserialize)]
pub struct PasskeyLoginStartRequest {
    pub email: String,
}

#[derive(Deserialize)]
pub struct PasskeyLoginFinishRequest {
    pub challenge_id: String,
    pub credential: Value,
}

#[derive(Deserialize)]
pub struct NativeHandoffStartRequest { pub code_challenge: String }
#[derive(Serialize)]
pub struct NativeHandoffStartResponse { pub handoff_id: String, pub authorization_url: String }
#[derive(Deserialize)]
pub struct NativeHandoffCompleteRequest { pub handoff_id: String }
#[derive(Serialize)]
pub struct NativeHandoffCompleteResponse { pub handoff_id: String, pub code: String }
#[derive(Deserialize)]
pub struct NativeHandoffExchangeRequest { pub handoff_id: String, pub code: String, pub code_verifier: String }

#[derive(Serialize)]
pub struct PasskeyResponse {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
}

fn normalize_service_api_key_scopes(scopes: Option<Vec<String>>) -> Result<Vec<String>, AppError> {
    let scopes = api_keys::normalize_scopes(scopes.or_else(|| Some(vec!["admin".to_string()])))
        .map_err(AppError::BadRequest)?;
    if !scopes.iter().any(|scope| scope == "admin") {
        return Err(AppError::BadRequest(
            "Service account API keys require the admin scope".into(),
        ));
    }
    Ok(scopes)
}

#[derive(Serialize)]
pub struct SessionResponse {
    pub id: String,
    pub device_name: Option<String>,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub last_seen_at: String,
    pub expires_at: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct ConfigEntryResponse {
    pub key: String,
    pub label: String,
    pub value: String,
    pub effective_value: String,
    pub source: String,
    pub env_var: Option<String>,
    pub locked: bool,
    pub secret: bool,
}

pub(crate) fn generate_temporary_password() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(20)
        .map(char::from)
        .collect()
}

fn generate_password_reset_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(48)
        .map(char::from)
        .collect()
}

fn hash_password_reset_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    hex::encode(digest)
}

fn normalize_totp_recovery_code(code: &str) -> String {
    let trimmed = code.trim().to_ascii_uppercase();
    let digits = trimmed
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>();
    if digits.len() == 8 {
        format!("{}-{}", &digits[..4], &digits[4..])
    } else {
        trimmed
    }
}

fn hash_totp_recovery_code(code: &str) -> String {
    let normalized = normalize_totp_recovery_code(code);
    let digest = Sha256::digest(normalized.as_bytes());
    hex::encode(digest)
}

fn hash_totp_recovery_codes(codes: &[String]) -> Vec<String> {
    codes
        .iter()
        .map(|code| hash_totp_recovery_code(code))
        .collect()
}

fn hash_refresh_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

async fn persist_session(
    state: &AppState,
    user_id: &str,
    tokens: &jwt::TokenPair,
) -> Result<(), AppError> {
    let expires_at = (chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339();
    sqlx::query(
        "INSERT INTO sessions (id, user_id, refresh_token_hash, expires_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(&tokens.session_id)
    .bind(user_id)
    .bind(hash_refresh_token(&tokens.refresh_token))
    .bind(expires_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;
    Ok(())
}

async fn consume_totp_recovery_code(
    state: &AppState,
    user_id: &str,
    code: &str,
) -> Result<bool, AppError> {
    let Some((recovery_codes_json,)) =
        sqlx::query_as::<_, (Option<String>,)>("SELECT recovery_codes FROM users WHERE id = ?1")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?
    else {
        return Ok(false);
    };
    let Some(recovery_codes_json) = recovery_codes_json else {
        return Ok(false);
    };
    let mut recovery_code_hashes: Vec<String> =
        serde_json::from_str(&recovery_codes_json).map_err(AppError::internal)?;
    let submitted_hash = hash_totp_recovery_code(code);
    let Some(index) = recovery_code_hashes
        .iter()
        .position(|stored_hash| stored_hash == &submitted_hash)
    else {
        return Ok(false);
    };

    recovery_code_hashes.remove(index);
    let next_codes_json =
        serde_json::to_string(&recovery_code_hashes).map_err(AppError::internal)?;
    sqlx::query("UPDATE users SET recovery_codes = ?1 WHERE id = ?2")
        .bind(next_codes_json)
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(true)
}

fn webauthn_for_state(state: &AppState) -> Result<Webauthn, AppError> {
    let origin = Url::parse(&state.base_url).map_err(|_| {
        AppError::Internal("BASE_URL must be a valid origin for passkey support".into())
    })?;
    let rp_id = webauthn_rp_id(&origin)?;
    let mut builder = WebauthnBuilder::new(rp_id, &origin)
        .map_err(|error| AppError::Internal(format!("Invalid passkey configuration: {error}")))?;
    for allowed_origin in webauthn_allowed_origins(
        &state.base_url,
        std::env::var(STARLANE_WEBAUTHN_ORIGINS_ENV).ok(),
    )? {
        if allowed_origin != origin {
            builder = builder.append_allowed_origin(&allowed_origin);
        }
    }
    builder
        .rp_name("Starlane Notes")
        .build()
        .map_err(|error| AppError::Internal(format!("Invalid passkey configuration: {error}")))
}

fn webauthn_rp_id(origin: &Url) -> Result<&str, AppError> {
    origin
        .host_str()
        .ok_or_else(|| AppError::Internal("BASE_URL must include a valid host".into()))
}

fn webauthn_allowed_origins(
    base_url: &str,
    extra_origins: Option<String>,
) -> Result<Vec<Url>, AppError> {
    let mut origins = Vec::new();
    push_webauthn_origin(&mut origins, base_url)?;
    for origin in NATIVE_APP_WEBAUTHN_ORIGINS {
        push_webauthn_origin(&mut origins, origin)?;
    }
    if let Some(extra_origins) = extra_origins {
        for origin in extra_origins.split(',') {
            let origin = origin.trim();
            if origin.is_empty() {
                continue;
            }
            push_webauthn_origin(&mut origins, origin)?;
        }
    }
    Ok(origins)
}

fn push_webauthn_origin(origins: &mut Vec<Url>, value: &str) -> Result<(), AppError> {
    let origin = normalize_webauthn_origin(value)?;
    if origins.iter().any(|existing| existing == &origin) {
        return Ok(());
    }
    origins.push(origin);
    Ok(())
}

fn normalize_webauthn_origin(value: &str) -> Result<Url, AppError> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(AppError::Internal(
            "Passkey origin configuration cannot include an empty origin".into(),
        ));
    }
    if trimmed == "*" {
        return Err(AppError::Internal(format!(
            "{STARLANE_WEBAUTHN_ORIGINS_ENV} must list explicit origins; wildcard '*' is not allowed"
        )));
    }
    Url::parse(trimmed)
        .map_err(|error| AppError::Internal(format!("Invalid passkey origin '{trimmed}': {error}")))
}

fn credential_id_to_string(credential_id: &CredentialID) -> Result<String, AppError> {
    serde_json::to_value(credential_id)
        .map_err(AppError::internal)?
        .as_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| AppError::Internal("Unable to serialize passkey credential id".into()))
}

async fn store_passkey_challenge<T: Serialize>(
    state: &AppState,
    user_id: Option<&str>,
    email: Option<&str>,
    kind: &str,
    passkey_state: &T,
) -> Result<String, AppError> {
    let challenge_id = Uuid::new_v4().to_string();
    let state_json = serde_json::to_string(passkey_state).map_err(AppError::internal)?;
    let expires_at = (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339();

    sqlx::query(
        "INSERT INTO passkey_challenges (id, user_id, email, kind, state_json, expires_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&challenge_id)
    .bind(user_id)
    .bind(email)
    .bind(kind)
    .bind(state_json)
    .bind(expires_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(challenge_id)
}

async fn take_passkey_challenge<T: for<'de> Deserialize<'de>>(
    state: &AppState,
    challenge_id: &str,
    expected_kind: &str,
) -> Result<(Option<String>, Option<String>, T), AppError> {
    let row: Option<(Option<String>, Option<String>, String, String)> = sqlx::query_as(
        "SELECT user_id, email, kind, state_json FROM passkey_challenges \
         WHERE id = ?1 AND expires_at > ?2",
    )
    .bind(challenge_id)
    .bind(chrono::Utc::now().to_rfc3339())
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    sqlx::query("DELETE FROM passkey_challenges WHERE id = ?1")
        .bind(challenge_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    let (user_id, email, kind, state_json) =
        row.ok_or_else(|| AppError::BadRequest("Passkey challenge is invalid or expired".into()))?;
    if kind != expected_kind {
        return Err(AppError::BadRequest(
            "Passkey challenge type does not match this request".into(),
        ));
    }
    let passkey_state = serde_json::from_str(&state_json).map_err(AppError::internal)?;
    Ok((user_id, email, passkey_state))
}

fn validate_passkey_registration_challenge_user(
    challenge_user_id: Option<String>,
    authenticated_user_id: &str,
) -> Result<(), AppError> {
    match challenge_user_id {
        Some(user_id) if user_id == authenticated_user_id => Ok(()),
        Some(_) => Err(AppError::Unauthorized(
            "Passkey registration challenge does not belong to this account".into(),
        )),
        None => Err(AppError::BadRequest(
            "Passkey registration challenge is missing account state".into(),
        )),
    }
}

async fn load_user_passkeys(state: &AppState, user_id: &str) -> Result<Vec<Passkey>, AppError> {
    let rows: Vec<(String,)> = sqlx::query_as("SELECT public_key FROM passkeys WHERE user_id = ?1")
        .bind(user_id)
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?;

    rows.into_iter()
        .map(|(public_key_json,)| {
            serde_json::from_str(&public_key_json).map_err(AppError::internal)
        })
        .collect()
}

async fn delete_expired_passkey_challenges(state: &AppState) {
    let _ = sqlx::query("DELETE FROM passkey_challenges WHERE expires_at <= ?1")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.db)
        .await;
}

// ============================================================
// Handlers
// ============================================================

/// POST /api/v1/auth/register
async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Validate input
    if req.email.is_empty() || req.password.is_empty() || req.display_name.is_empty() {
        return Err(AppError::BadRequest("All fields are required".into()));
    }

    if req.email.len() > 254 {
        return Err(AppError::BadRequest(
            "Email must be at most 254 characters".into(),
        ));
    }

    if req.password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters".into(),
        ));
    }

    if req.password.len() > 128 {
        return Err(AppError::BadRequest(
            "Password must be at most 128 characters".into(),
        ));
    }

    if req.display_name.len() > 100 {
        return Err(AppError::BadRequest(
            "Display name must be at most 100 characters".into(),
        ));
    }

    // Check if this is the first user (they become admin)
    let user_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
        .map_err(AppError::internal)?;

    let role = if user_count.0 == 0 { "admin" } else { "member" };

    if user_count.0 > 0 && !public_signups_enabled(&state).await? {
        return Err(AppError::Forbidden(
            "Public signups are disabled for this server. Ask an administrator for an invite."
                .into(),
        ));
    }

    // Check for existing user
    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?1")
        .bind(&req.email)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;

    if existing.is_some() {
        return Err(AppError::Conflict("Email already registered".into()));
    }

    // Hash password and create user
    let password_hash =
        password::hash_password(&req.password).map_err(|e| AppError::Internal(e.to_string()))?;

    let user_id = Uuid::new_v4().to_string();

    sqlx::query(
        "INSERT INTO users (id, email, display_name, password_hash, role) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&user_id)
    .bind(&req.email)
    .bind(&req.display_name)
    .bind(&password_hash)
    .bind(role)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    create_personal_space(&state, &user_id, &req.display_name).await?;

    // If first user, mark setup as started
    if role == "admin" {
        sqlx::query("UPDATE system_config SET value = 'true' WHERE key = 'setup_completed'")
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    // Generate tokens
    let tokens = jwt::create_token_pair(&user_id, &req.email, role, &state.jwt_secret)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    persist_session(&state, &user_id, &tokens).await?;

    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_in: tokens.expires_in,
            user: UserResponse {
                id: user_id,
                email: req.email,
                username: None,
                display_name: req.display_name,
                avatar_url: None,
                role: role.to_string(),
                totp_enabled: false,
            },
        }),
    ))
}

/// POST /api/v1/auth/login
async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.email.len() > 254 {
        return Err(AppError::BadRequest(
            "Email must be at most 254 characters".into(),
        ));
    }

    if req.password.len() > 128 {
        return Err(AppError::BadRequest(
            "Password must be at most 128 characters".into(),
        ));
    }

    // Find user
    let user: Option<(String, String, String, Option<String>, String, Option<String>, i32)> =
        sqlx::query_as(
            "SELECT id, email, display_name, avatar_url, role, password_hash, totp_enabled FROM users WHERE email = ?1 AND is_active = 1",
        )
        .bind(&req.email)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;

    let (user_id, email, display_name, avatar_url, role, password_hash, totp_enabled_int) =
        user.ok_or_else(|| AppError::Unauthorized("Invalid credentials".into()))?;
    let totp_enabled = totp_enabled_int != 0;

    // Verify password
    let hash = password_hash
        .ok_or_else(|| AppError::Unauthorized("This account uses OAuth/SSO login".into()))?;

    let valid = password::verify_password(&req.password, &hash)
        .map_err(|_| AppError::Unauthorized("Invalid credentials".into()))?;

    if !valid {
        return Err(AppError::Unauthorized("Invalid credentials".into()));
    }

    // Check 2FA if enabled
    if totp_enabled {
        let code = req
            .totp_code
            .as_deref()
            .ok_or_else(|| AppError::TwoFactorRequired("2FA code required".into()))?;

        let totp_secret: (String,) = sqlx::query_as("SELECT totp_secret FROM users WHERE id = ?1")
            .bind(&user_id)
            .fetch_one(&state.db)
            .await
            .map_err(AppError::internal)?;

        let valid_totp = totp::verify_totp_code(&totp_secret.0, code)
            .map_err(|e| AppError::Internal(e.to_string()))?;

        if !valid_totp && !consume_totp_recovery_code(&state, &user_id, code).await? {
            return Err(AppError::Unauthorized("Invalid 2FA code".into()));
        }
    }

    // Generate tokens
    let tokens = jwt::create_token_pair(&user_id, &email, &role, &state.jwt_secret)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    persist_session(&state, &user_id, &tokens).await?;

    Ok(Json(AuthResponse {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_in: tokens.expires_in,
        user: UserResponse {
            id: user_id,
            email,
            username: None,
            display_name,
            avatar_url,
            role,
            totp_enabled,
        },
    }))
}

/// POST /api/v1/auth/refresh
async fn refresh_token(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<impl IntoResponse, AppError> {
    let claims = jwt::validate_token(
        &req.refresh_token,
        &state.jwt_secret,
        jwt::TokenType::Refresh,
    )
    .map_err(|_| AppError::Unauthorized("Invalid refresh token".into()))?;

    let existing_session: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM sessions WHERE id = ?1 AND user_id = ?2 AND refresh_token_hash = ?3 AND expires_at > ?4",
    )
    .bind(&claims.sid)
    .bind(&claims.sub)
    .bind(hash_refresh_token(&req.refresh_token))
    .bind(chrono::Utc::now().to_rfc3339())
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;
    if existing_session.is_none() {
        return Err(AppError::Unauthorized(
            "Refresh session is no longer active".into(),
        ));
    }

    // Verify user still exists and is active
    let user: Option<(String, String, String)> =
        sqlx::query_as("SELECT id, email, role FROM users WHERE id = ?1 AND is_active = 1")
            .bind(&claims.sub)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    let (user_id, email, role) =
        user.ok_or_else(|| AppError::Unauthorized("User no longer active".into()))?;

    let tokens = jwt::create_token_pair(&user_id, &email, &role, &state.jwt_secret)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    sqlx::query("DELETE FROM sessions WHERE id = ?1")
        .bind(&claims.sid)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    persist_session(&state, &user_id, &tokens).await?;

    Ok(Json(json!({
        "access_token": tokens.access_token,
        "refresh_token": tokens.refresh_token,
        "expires_in": tokens.expires_in,
    })))
}

/// POST /api/v1/auth/password-reset/request
async fn request_password_reset(
    State(state): State<AppState>,
    Json(req): Json<PasswordResetRequest>,
) -> Result<impl IntoResponse, AppError> {
    let email = req.email.trim().to_lowercase();
    if email.len() > 254 {
        return Err(AppError::BadRequest(
            "Email must be at most 254 characters".into(),
        ));
    }

    let generic_message =
        "If an active account exists for that email, a password reset link has been sent.";
    if !email.is_empty() {
        let user: Option<(String, String)> =
            sqlx::query_as("SELECT id, email FROM users WHERE lower(email) = ?1 AND is_active = 1")
                .bind(&email)
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?;

        if let Some((user_id, recipient_email)) = user {
            let token = generate_password_reset_token();
            let token_hash = hash_password_reset_token(&token);
            let expires_at = (chrono::Utc::now() + chrono::Duration::minutes(30))
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string();
            let reset_id = Uuid::new_v4().to_string();
            let now = chrono::Utc::now().to_rfc3339();

            sqlx::query(
                "UPDATE password_reset_tokens SET used_at = ?1 WHERE user_id = ?2 AND used_at IS NULL",
            )
            .bind(&now)
            .bind(&user_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;

            sqlx::query(
                "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at) \
                 VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(&reset_id)
            .bind(&user_id)
            .bind(&token_hash)
            .bind(&expires_at)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;

            let reset_url = format!(
                "{}/reset-password?token={}",
                state.base_url.trim_end_matches('/'),
                token
            );
            let body = format!(
                "A password reset was requested for your Starlane Notes account.\n\n\
                 Reset your password within 30 minutes:\n{}\n\n\
                 If you did not request this, you can ignore this email.",
                reset_url
            );
            match crate::notifications::send_direct_email(
                &state.db,
                &recipient_email,
                "Reset your Starlane Notes password",
                &body,
            )
            .await
            {
                Ok(true) => {}
                Ok(false) => tracing::warn!(
                    "Password reset requested for {}, but SMTP is not configured",
                    recipient_email
                ),
                Err(error) => tracing::warn!(
                    "Failed to send password reset email to {}: {}",
                    recipient_email,
                    error
                ),
            }

            crate::audit::log_action(
                &state.db,
                Some(&user_id),
                None,
                "user.password_reset_request",
                Some("user"),
                Some(&user_id),
            )
            .await;
        }
    }

    Ok(Json(PasswordResetRequestResponse {
        message: generic_message.to_string(),
    }))
}

/// POST /api/v1/auth/password-reset/confirm
async fn confirm_password_reset(
    State(state): State<AppState>,
    Json(req): Json<PasswordResetConfirmRequest>,
) -> Result<impl IntoResponse, AppError> {
    let token = validate_password_reset_confirm_input(&req)?;

    let token_hash = hash_password_reset_token(token);
    let now = chrono::Utc::now().to_rfc3339();
    let token_row: Option<(String, String)> = sqlx::query_as(
        "SELECT prt.id, prt.user_id \
         FROM password_reset_tokens prt \
         JOIN users u ON u.id = prt.user_id \
         WHERE prt.token_hash = ?1 \
           AND prt.used_at IS NULL \
           AND prt.expires_at > ?2 \
           AND u.is_active = 1",
    )
    .bind(&token_hash)
    .bind(&now)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;
    let (token_id, user_id) = token_row
        .ok_or_else(|| AppError::BadRequest("Password reset token is invalid or expired".into()))?;

    let password_hash =
        password::hash_password(&req.password).map_err(|e| AppError::Internal(e.to_string()))?;
    sqlx::query("UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(&password_hash)
        .bind(&now)
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    sqlx::query("DELETE FROM sessions WHERE user_id = ?1")
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    sqlx::query(
        "UPDATE password_reset_tokens SET used_at = ?1 WHERE user_id = ?2 AND used_at IS NULL",
    )
    .bind(&now)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&user_id),
        None,
        "user.password_reset_confirm",
        Some("password_reset_token"),
        Some(&token_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

fn validate_password_reset_confirm_input(
    req: &PasswordResetConfirmRequest,
) -> Result<&str, AppError> {
    let token = req.token.trim();
    if token.len() < 32 || token.len() > 128 {
        return Err(AppError::BadRequest(
            "Password reset token is invalid or expired".into(),
        ));
    }
    if req.password.len() < 8 {
        return Err(AppError::BadRequest(
            "Password must be at least 8 characters".into(),
        ));
    }
    if req.password.len() > 128 {
        return Err(AppError::BadRequest(
            "Password must be at most 128 characters".into(),
        ));
    }
    Ok(token)
}

/// POST /api/v1/auth/logout
async fn logout(State(state): State<AppState>, auth: AuthUser) -> Result<StatusCode, AppError> {
    sqlx::query("DELETE FROM sessions WHERE id = ?1 AND user_id = ?2")
        .bind(&auth.claims.sid)
        .bind(&auth.claims.sub)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "user.session_logout",
        Some("session"),
        Some(&auth.claims.sid),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/auth/oauth/{provider}
async fn oauth_redirect(
    State(state): State<AppState>,
    Path(provider_name): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let provider = match provider_name.as_str() {
        "google" => oauth::OAuthProvider::Google,
        "github" => oauth::OAuthProvider::GitHub,
        _ => return Err(AppError::BadRequest("Unknown OAuth provider".into())),
    };

    if !provider.is_configured() {
        return Err(AppError::BadRequest(format!(
            "{} OAuth is not configured",
            provider.name()
        )));
    }

    let (auth_url, csrf_token, pkce_verifier) = oauth::get_auth_url(&provider, &state.base_url)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Store CSRF token and PKCE verifier for validation on callback
    state
        .oauth_cache
        .store_oauth(csrf_token.secret(), pkce_verifier)
        .await;

    Ok(Redirect::temporary(&auth_url))
}

/// GET /api/v1/auth/oauth/{provider}/callback
async fn oauth_callback(
    State(state): State<AppState>,
    Path(provider_name): Path<String>,
    Query(params): Query<OAuthCallbackParams>,
) -> Result<impl IntoResponse, AppError> {
    let provider = match provider_name.as_str() {
        "google" => oauth::OAuthProvider::Google,
        "github" => oauth::OAuthProvider::GitHub,
        _ => return Err(AppError::BadRequest("Unknown OAuth provider".into())),
    };

    // Validate CSRF token and retrieve stored PKCE verifier
    let pkce_verifier = state
        .oauth_cache
        .take_oauth(&params.state)
        .await
        .ok_or_else(|| AppError::Unauthorized("Invalid or expired OAuth state".into()))?;

    let user_info = oauth::exchange_code(&provider, &state.base_url, &params.code, pkce_verifier)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let (user_id, role) = find_or_create_oauth_user(&state, &user_info, true).await?;

    let tokens = jwt::create_token_pair(&user_id, &user_info.email, &role, &state.jwt_secret)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    persist_session(&state, &user_id, &tokens).await?;

    // Redirect to frontend with tokens in URL fragment (not query string)
    // Fragment is not sent to the server in subsequent requests, not logged in access logs
    let redirect_url = format!(
        "{}/#access_token={}&refresh_token={}",
        state.base_url, tokens.access_token, tokens.refresh_token
    );

    Ok(Redirect::temporary(&redirect_url))
}

/// GET /api/v1/auth/oidc
async fn oidc_redirect(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let config = load_oidc_config(&state).await?;
    if !config.enabled {
        return Err(AppError::BadRequest("OIDC is not enabled".into()));
    }

    let (auth_url, csrf_token, nonce) = oidc::get_auth_url(&config, &state.base_url)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Store CSRF token and nonce for validation on callback
    state
        .oauth_cache
        .store_oidc(csrf_token.secret(), nonce)
        .await;

    Ok(Redirect::temporary(&auth_url))
}

/// GET /api/v1/auth/oidc/callback
async fn oidc_callback(
    State(state): State<AppState>,
    Query(params): Query<OAuthCallbackParams>,
) -> Result<impl IntoResponse, AppError> {
    let config = load_oidc_config(&state).await?;

    // Validate CSRF token and retrieve stored nonce
    let nonce = state
        .oauth_cache
        .take_oidc(&params.state)
        .await
        .ok_or_else(|| AppError::Unauthorized("Invalid or expired OIDC state".into()))?;

    let oidc_user = oidc::exchange_code(&config, &state.base_url, &params.code, nonce)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let oauth_info = oidc_user.to_oauth_user_info();
    let role_override = if !config.role_claim.is_empty() {
        Some(oidc_user.resolve_role(&config.role_admin_value).to_string())
    } else {
        None
    };

    let (user_id, role) =
        find_or_create_oauth_user(&state, &oauth_info, config.auto_provision).await?;

    // Sync role from OIDC provider if role_claim is configured
    let final_role = role_override.unwrap_or(role);

    let tokens =
        jwt::create_token_pair(&user_id, &oauth_info.email, &final_role, &state.jwt_secret)
            .map_err(|e| AppError::Internal(e.to_string()))?;
    persist_session(&state, &user_id, &tokens).await?;

    let redirect_url = format!(
        "{}/#access_token={}&refresh_token={}",
        state.base_url, tokens.access_token, tokens.refresh_token
    );

    Ok(Redirect::temporary(&redirect_url))
}

/// GET /api/v1/auth/passkeys
async fn list_passkeys(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, created_at, last_used_at FROM passkeys WHERE user_id = ?1 ORDER BY created_at DESC",
    )
    .bind(&auth.claims.sub)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(Json(
        rows.into_iter()
            .map(|(id, name, created_at, last_used_at)| PasskeyResponse {
                id,
                name,
                created_at,
                last_used_at,
            })
            .collect::<Vec<_>>(),
    ))
}

/// POST /api/v1/auth/passkeys/register/start
async fn passkey_register_start(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<PasskeyRegisterStartRequest>,
) -> Result<impl IntoResponse, AppError> {
    delete_expired_passkey_challenges(&state).await;
    let _requested_name = req.name.as_deref().unwrap_or("Passkey").trim();
    let webauthn = webauthn_for_state(&state)?;
    let user_uuid = Uuid::parse_str(&auth.claims.sub)
        .map_err(|_| AppError::Internal("User id is not a valid UUID".into()))?;
    let user: (String,) = sqlx::query_as("SELECT display_name FROM users WHERE id = ?1")
        .bind(&auth.claims.sub)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::internal)?;
    let exclude_credentials = load_user_passkeys(&state, &auth.claims.sub)
        .await?
        .into_iter()
        .map(|passkey| passkey.cred_id().clone())
        .collect::<Vec<CredentialID>>();

    let (creation, registration_state) = webauthn
        .start_passkey_registration(
            user_uuid,
            &auth.claims.email,
            &user.0,
            Some(exclude_credentials),
        )
        .map_err(|error| AppError::BadRequest(format!("Unable to start passkey setup: {error}")))?;
    let challenge_id = store_passkey_challenge(
        &state,
        Some(&auth.claims.sub),
        Some(&auth.claims.email),
        "registration",
        &registration_state,
    )
    .await?;

    Ok(Json(PasskeyChallengeResponse {
        challenge_id,
        public_key: serde_json::to_value(creation).map_err(AppError::internal)?,
    }))
}

/// POST /api/v1/auth/passkeys/register/finish
async fn passkey_register_finish(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<PasskeyRegisterFinishRequest>,
) -> Result<impl IntoResponse, AppError> {
    let (challenge_user_id, _, registration_state): (
        Option<String>,
        Option<String>,
        PasskeyRegistration,
    ) = take_passkey_challenge(&state, &req.challenge_id, "registration").await?;
    validate_passkey_registration_challenge_user(challenge_user_id, &auth.claims.sub)?;
    let webauthn = webauthn_for_state(&state)?;
    let credential: RegisterPublicKeyCredential = serde_json::from_value(req.credential)
        .map_err(|_| AppError::BadRequest("Passkey registration response is malformed".into()))?;
    let passkey = webauthn
        .finish_passkey_registration(&credential, &registration_state)
        .map_err(|error| AppError::Unauthorized(format!("Passkey registration failed: {error}")))?;
    let credential_id = credential_id_to_string(passkey.cred_id())?;
    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM passkeys WHERE credential_id = ?1")
            .bind(&credential_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    if existing.is_some() {
        return Err(AppError::Conflict(
            "This passkey is already registered.".into(),
        ));
    }

    let passkey_id = Uuid::new_v4().to_string();
    let name = req
        .name
        .unwrap_or_else(|| "Passkey".to_string())
        .trim()
        .chars()
        .take(80)
        .collect::<String>();
    let passkey_json = serde_json::to_string(&passkey).map_err(AppError::internal)?;

    sqlx::query(
        "INSERT INTO passkeys (id, user_id, credential_id, public_key, name, sign_count) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(&passkey_id)
    .bind(&auth.claims.sub)
    .bind(&credential_id)
    .bind(passkey_json)
    .bind(if name.is_empty() { "Passkey" } else { &name })
    .bind(0_i64)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "user.passkey_create",
        Some("passkey"),
        Some(&passkey_id),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(PasskeyResponse {
            id: passkey_id,
            name: if name.is_empty() {
                "Passkey".into()
            } else {
                name
            },
            created_at: chrono::Utc::now().to_rfc3339(),
            last_used_at: None,
        }),
    ))
}

/// DELETE /api/v1/auth/passkeys/{passkey_id}
async fn delete_passkey(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(passkey_id): Path<String>,
) -> Result<StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM passkeys WHERE id = ?1 AND user_id = ?2")
        .bind(&passkey_id)
        .bind(&auth.claims.sub)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Passkey not found".into()));
    }
    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "user.passkey_delete",
        Some("passkey"),
        Some(&passkey_id),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/auth/passkeys/login/start
async fn native_handoff_start(
    State(state): State<AppState>,
    Json(req): Json<NativeHandoffStartRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.code_challenge.len() < 43 || req.code_challenge.len() > 128 {
        return Err(AppError::BadRequest("Native sign-in verifier is invalid.".into()));
    }
    let handoff_id = Uuid::new_v4().to_string();
    let expires_at = (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339();
    sqlx::query("INSERT INTO native_auth_handoffs (id, code_challenge, expires_at) VALUES (?1, ?2, ?3)")
        .bind(&handoff_id).bind(&req.code_challenge).bind(expires_at).execute(&state.db).await.map_err(AppError::internal)?;
    Ok(Json(NativeHandoffStartResponse { handoff_id: handoff_id.clone(), authorization_url: format!("{}/login?native_handoff={handoff_id}", state.base_url.trim_end_matches('/')) }))
}

async fn native_handoff_complete(
    State(state): State<AppState>, auth: AuthUser, Json(req): Json<NativeHandoffCompleteRequest>,
) -> Result<impl IntoResponse, AppError> {
    let code: String = rand::thread_rng().sample_iter(&Alphanumeric).take(48).map(char::from).collect();
    let result = sqlx::query("UPDATE native_auth_handoffs SET user_id = ?1, code_hash = ?2 WHERE id = ?3 AND user_id IS NULL AND consumed_at IS NULL AND expires_at > ?4")
        .bind(&auth.claims.sub).bind(hash_password_reset_token(&code)).bind(&req.handoff_id).bind(chrono::Utc::now().to_rfc3339()).execute(&state.db).await.map_err(AppError::internal)?;
    if result.rows_affected() != 1 { return Err(AppError::BadRequest("Native sign-in handoff is expired or invalid.".into())); }
    Ok(Json(NativeHandoffCompleteResponse { handoff_id: req.handoff_id, code }))
}

async fn native_handoff_exchange(
    State(state): State<AppState>, Json(req): Json<NativeHandoffExchangeRequest>,
) -> Result<impl IntoResponse, AppError> {
    let verifier_challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(req.code_verifier.as_bytes()));
    let row: Option<(Option<String>, String, Option<String>)> = sqlx::query_as("SELECT user_id, code_challenge, code_hash FROM native_auth_handoffs WHERE id = ?1 AND consumed_at IS NULL AND expires_at > ?2")
        .bind(&req.handoff_id).bind(chrono::Utc::now().to_rfc3339()).fetch_optional(&state.db).await.map_err(AppError::internal)?;
    let Some((Some(user_id), challenge, Some(code_hash))) = row else { return Err(AppError::Unauthorized("Native sign-in handoff is expired or invalid.".into())); };
    if challenge != verifier_challenge || code_hash != hash_password_reset_token(&req.code) { return Err(AppError::Unauthorized("Native sign-in handoff is invalid.".into())); }
    sqlx::query("UPDATE native_auth_handoffs SET consumed_at = ?1 WHERE id = ?2").bind(chrono::Utc::now().to_rfc3339()).bind(&req.handoff_id).execute(&state.db).await.map_err(AppError::internal)?;
    let user: (String, Option<String>, String, Option<String>, String, i32) = sqlx::query_as("SELECT email, username, display_name, avatar_url, role, totp_enabled FROM users WHERE id = ?1 AND is_active = 1").bind(&user_id).fetch_one(&state.db).await.map_err(AppError::internal)?;
    let tokens = jwt::create_token_pair(&user_id, &user.0, &user.4, &state.jwt_secret).map_err(|e| AppError::internal(e.to_string()))?;
    persist_session(&state, &user_id, &tokens).await?;
    Ok(Json(AuthResponse { access_token: tokens.access_token, refresh_token: tokens.refresh_token, expires_in: tokens.expires_in, user: UserResponse { id: user_id, email: user.0, username: user.1, display_name: user.2, avatar_url: user.3, role: user.4, totp_enabled: user.5 != 0 } }))
}

/// POST /api/v1/auth/passkeys/login/start
async fn passkey_login_start(
    State(state): State<AppState>,
    Json(req): Json<PasskeyLoginStartRequest>,
) -> Result<impl IntoResponse, AppError> {
    delete_expired_passkey_challenges(&state).await;
    let email = req.email.trim().to_lowercase();
    if email.is_empty() || email.len() > 254 {
        return Err(AppError::BadRequest("Enter a valid email address.".into()));
    }

    let user: Option<(String, String)> =
        sqlx::query_as("SELECT id, email FROM users WHERE lower(email) = ?1 AND is_active = 1")
            .bind(&email)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    let (user_id, canonical_email) = user.ok_or_else(|| {
        AppError::Unauthorized("Passkey sign-in is unavailable for this account.".into())
    })?;
    let passkeys = load_user_passkeys(&state, &user_id).await?;
    if passkeys.is_empty() {
        return Err(AppError::Unauthorized(
            "Passkey sign-in is unavailable for this account.".into(),
        ));
    }

    let webauthn = webauthn_for_state(&state)?;
    let (request, authentication_state) = webauthn
        .start_passkey_authentication(&passkeys)
        .map_err(|error| {
            AppError::BadRequest(format!("Unable to start passkey sign-in: {error}"))
        })?;
    let challenge_id = store_passkey_challenge(
        &state,
        Some(&user_id),
        Some(&canonical_email),
        "authentication",
        &authentication_state,
    )
    .await?;

    Ok(Json(PasskeyChallengeResponse {
        challenge_id,
        public_key: serde_json::to_value(request).map_err(AppError::internal)?,
    }))
}

/// POST /api/v1/auth/passkeys/login/finish
async fn passkey_login_finish(
    State(state): State<AppState>,
    Json(req): Json<PasskeyLoginFinishRequest>,
) -> Result<impl IntoResponse, AppError> {
    let (challenge_user_id, _, authentication_state): (
        Option<String>,
        Option<String>,
        PasskeyAuthentication,
    ) = take_passkey_challenge(&state, &req.challenge_id, "authentication").await?;
    let user_id = challenge_user_id
        .ok_or_else(|| AppError::BadRequest("Passkey challenge is missing account state".into()))?;
    let webauthn = webauthn_for_state(&state)?;
    let credential: PublicKeyCredential = serde_json::from_value(req.credential)
        .map_err(|_| AppError::BadRequest("Passkey sign-in response is malformed".into()))?;
    let result = webauthn
        .finish_passkey_authentication(&credential, &authentication_state)
        .map_err(|error| AppError::Unauthorized(format!("Passkey sign-in failed: {error}")))?;
    let credential_id = credential_id_to_string(result.cred_id())?;

    let passkey_row: Option<(String, String)> = sqlx::query_as(
        "SELECT id, public_key FROM passkeys WHERE user_id = ?1 AND credential_id = ?2",
    )
    .bind(&user_id)
    .bind(&credential_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;
    let (passkey_id, passkey_json) = passkey_row.ok_or_else(|| {
        AppError::Unauthorized("Passkey is not registered for this account".into())
    })?;

    let mut passkey: Passkey = serde_json::from_str(&passkey_json).map_err(AppError::internal)?;
    let mut updated_passkey_json = None;
    if result.needs_update() {
        passkey.update_credential(&result);
        updated_passkey_json = Some(serde_json::to_string(&passkey).map_err(AppError::internal)?);
    }
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(updated) = updated_passkey_json {
        sqlx::query(
            "UPDATE passkeys SET public_key = ?1, sign_count = ?2, last_used_at = ?3 WHERE id = ?4",
        )
        .bind(updated)
        .bind(i64::from(result.counter()))
        .bind(&now)
        .bind(&passkey_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    } else {
        sqlx::query("UPDATE passkeys SET last_used_at = ?1 WHERE id = ?2")
            .bind(&now)
            .bind(&passkey_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    let user: (String, Option<String>, String, Option<String>, String, i32) = sqlx::query_as(
        "SELECT email, username, display_name, avatar_url, role, totp_enabled FROM users WHERE id = ?1 AND is_active = 1",
    )
    .bind(&user_id)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::internal)?;
    let tokens = jwt::create_token_pair(&user_id, &user.0, &user.4, &state.jwt_secret)
        .map_err(|error| AppError::Internal(error.to_string()))?;
    persist_session(&state, &user_id, &tokens).await?;

    Ok(Json(AuthResponse {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_in: tokens.expires_in,
        user: UserResponse {
            id: user_id,
            email: user.0,
            username: user.1,
            display_name: user.2,
            avatar_url: user.3,
            role: user.4,
            totp_enabled: user.5 != 0,
        },
    }))
}

/// POST /api/v1/auth/2fa/totp/setup
async fn totp_setup(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let existing: (i32,) = sqlx::query_as("SELECT totp_enabled FROM users WHERE id = ?1")
        .bind(&auth.claims.sub)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::internal)?;
    if existing.0 != 0 {
        return Err(AppError::Conflict(
            "TOTP is already enabled; disable it before setting up a new secret".into(),
        ));
    }

    let setup = totp::generate_totp_setup(&auth.claims.email)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let recovery_code_hashes =
        serde_json::to_string(&hash_totp_recovery_codes(&setup.recovery_codes))
            .map_err(AppError::internal)?;

    // Store secret temporarily — will be confirmed on first verify
    sqlx::query("UPDATE users SET totp_secret = ?1, recovery_codes = ?2 WHERE id = ?3")
        .bind(&setup.secret)
        .bind(recovery_code_hashes)
        .bind(&auth.claims.sub)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(Json(json!({
        "qr_code_url": setup.qr_code_url,
        "recovery_codes": setup.recovery_codes,
    })))
}

/// POST /api/v1/auth/2fa/totp/verify (enables TOTP after verifying first code)
async fn totp_verify(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<TotpVerifyRequest>,
) -> Result<impl IntoResponse, AppError> {
    let secret: Option<(Option<String>,)> =
        sqlx::query_as("SELECT totp_secret FROM users WHERE id = ?1")
            .bind(&auth.claims.sub)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    let totp_secret = secret
        .and_then(|s| s.0)
        .ok_or_else(|| AppError::BadRequest("TOTP not set up".into()))?;

    let valid = totp::verify_totp_code(&totp_secret, &req.code)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if !valid {
        return Err(AppError::Unauthorized("Invalid TOTP code".into()));
    }

    sqlx::query("UPDATE users SET totp_enabled = 1 WHERE id = ?1")
        .bind(&auth.claims.sub)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(Json(json!({ "totp_enabled": true })))
}

/// POST /api/v1/auth/2fa/totp/disable
async fn totp_disable(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<TotpVerifyRequest>,
) -> Result<impl IntoResponse, AppError> {
    let require_2fa = config_value(&state, "require_2fa", Some("REQUIRE_2FA"), "false")
        .await?
        .eq_ignore_ascii_case("true");
    if require_2fa {
        return Err(AppError::Forbidden(
            "Two-factor authentication is required by this server.".into(),
        ));
    }

    // Require a valid code to disable
    let secret: (Option<String>, i32) =
        sqlx::query_as("SELECT totp_secret, totp_enabled FROM users WHERE id = ?1")
            .bind(&auth.claims.sub)
            .fetch_one(&state.db)
            .await
            .map_err(AppError::internal)?;

    if secret.1 == 0 {
        return Err(AppError::BadRequest("TOTP not enabled".into()));
    }

    let totp_secret = secret
        .0
        .ok_or_else(|| AppError::BadRequest("TOTP not enabled".into()))?;

    let valid_totp = totp::verify_totp_code(&totp_secret, &req.code)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if !valid_totp && !consume_totp_recovery_code(&state, &auth.claims.sub, &req.code).await? {
        return Err(AppError::Unauthorized("Invalid TOTP code".into()));
    }

    sqlx::query("UPDATE users SET totp_enabled = 0, totp_secret = NULL, recovery_codes = NULL WHERE id = ?1")
        .bind(&auth.claims.sub)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(Json(json!({ "totp_enabled": false })))
}

/// GET /api/v1/auth/me
async fn me(State(state): State<AppState>, auth: AuthUser) -> Result<impl IntoResponse, AppError> {
    let user: (String, String, Option<String>, String, Option<String>, String, i32) = sqlx::query_as(
        "SELECT id, email, username, display_name, avatar_url, role, totp_enabled FROM users WHERE id = ?1",
    )
    .bind(&auth.claims.sub)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(Json(UserResponse {
        id: user.0,
        email: user.1,
        username: user.2,
        display_name: user.3,
        avatar_url: user.4,
        role: user.5,
        totp_enabled: user.6 != 0,
    }))
}

/// PUT /api/v1/auth/me
async fn update_me(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<UpdateProfileRequest>,
) -> Result<impl IntoResponse, AppError> {
    let display_name = req.display_name.trim();
    if display_name.is_empty() {
        return Err(AppError::BadRequest("Display name is required".into()));
    }
    if display_name.len() > 100 {
        return Err(AppError::BadRequest(
            "Display name must be at most 100 characters".into(),
        ));
    }

    let avatar_url = req
        .avatar_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if avatar_url.as_deref().is_some_and(|value| value.len() > 500) {
        return Err(AppError::BadRequest(
            "Avatar URL must be at most 500 characters".into(),
        ));
    }
    let username = req
        .username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if username
        .as_ref()
        .is_some_and(|value| !is_valid_username(value))
    {
        return Err(AppError::BadRequest(
            "Username must use 1-64 letters, numbers, hyphens, or underscores".into(),
        ));
    }
    if let Some(username) = &username {
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT id FROM users WHERE username = ?1 AND id != ?2 LIMIT 1")
                .bind(username)
                .bind(&auth.claims.sub)
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?;
        if existing.is_some() {
            return Err(AppError::Conflict("Username is already in use".into()));
        }
    }

    sqlx::query(
        "UPDATE users SET display_name = ?1, avatar_url = ?2, username = ?3, updated_at = ?4 WHERE id = ?5",
    )
    .bind(display_name)
    .bind(&avatar_url)
    .bind(&username)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(&auth.claims.sub)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "profile.update",
        Some("user"),
        Some(&auth.claims.sub),
    )
    .await;

    me(State(state), auth).await
}

fn is_valid_username(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// GET /api/v1/auth/sessions
async fn list_sessions(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let sessions: Vec<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, device_name, ip_address, user_agent, last_seen_at, expires_at, created_at FROM sessions WHERE user_id = ?1 ORDER BY last_seen_at DESC",
    )
    .bind(&auth.claims.sub)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<SessionResponse> = sessions
        .into_iter()
        .map(
            |(id, device_name, ip_address, user_agent, last_seen_at, expires_at, created_at)| {
                SessionResponse {
                    id,
                    device_name,
                    ip_address,
                    user_agent,
                    last_seen_at,
                    expires_at,
                    created_at,
                }
            },
        )
        .collect();

    Ok(Json(result))
}

/// DELETE /api/v1/auth/sessions/{session_id}
async fn revoke_session(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(session_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    sqlx::query("DELETE FROM sessions WHERE id = ?1 AND user_id = ?2")
        .bind(&session_id)
        .bind(&auth.claims.sub)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/auth/api-keys
async fn list_api_keys(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<(String, String, String, i32, Option<String>, Option<String>, Option<String>, String)> =
        sqlx::query_as(
            "SELECT id, name, key_prefix, is_service_account, scopes, last_used_at, expires_at, created_at \
             FROM api_keys WHERE user_id = ?1 ORDER BY created_at DESC",
        )
        .bind(&auth.claims.sub)
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?;

    let result: Vec<ApiKeyResponse> = rows
        .into_iter()
        .map(
            |(
                id,
                name,
                key_prefix,
                is_service_account,
                scopes,
                last_used_at,
                expires_at,
                created_at,
            )| {
                ApiKeyResponse {
                    id,
                    name,
                    key_prefix,
                    is_service_account: is_service_account != 0,
                    scopes: api_keys::parse_scope_json(scopes.as_deref()),
                    last_used_at,
                    expires_at,
                    created_at,
                }
            },
        )
        .collect();

    Ok(Json(result))
}

/// POST /api/v1/auth/api-keys
async fn create_api_key(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<ApiKeyCreateRequest>,
) -> Result<impl IntoResponse, AppError> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("API key name is required".into()));
    }
    if name.len() > 100 {
        return Err(AppError::BadRequest(
            "API key name must be at most 100 characters".into(),
        ));
    }

    let expires_at = req
        .expires_at
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            chrono::DateTime::parse_from_rfc3339(value)
                .map(|dt| dt.with_timezone(&chrono::Utc).to_rfc3339())
                .map_err(|_| AppError::BadRequest("expires_at must be an RFC3339 timestamp".into()))
        })
        .transpose()?;

    if let Some(expiry) = expires_at.as_deref() {
        let expiry = chrono::DateTime::parse_from_rfc3339(expiry)
            .map_err(|_| AppError::BadRequest("expires_at must be an RFC3339 timestamp".into()))?
            .with_timezone(&chrono::Utc);
        if expiry <= chrono::Utc::now() {
            return Err(AppError::BadRequest(
                "expires_at must be in the future".into(),
            ));
        }
    }

    let plaintext_key = api_keys::generate_api_key();
    let key_hash = api_keys::hash_api_key(&plaintext_key);
    let key_prefix = api_keys::visible_prefix(&plaintext_key);
    let scopes = api_keys::normalize_scopes(req.scopes).map_err(AppError::BadRequest)?;
    let scopes_json =
        api_keys::scope_json(&scopes).map_err(|e| AppError::Internal(e.to_string()))?;
    let key_id = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO api_keys (id, user_id, name, key_hash, key_prefix, is_service_account, scopes, expires_at, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, ?7, ?8)",
    )
    .bind(&key_id)
    .bind(&auth.claims.sub)
    .bind(name)
    .bind(&key_hash)
    .bind(&key_prefix)
    .bind(&scopes_json)
    .bind(&expires_at)
    .bind(&created_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "api_key.create",
        Some("api_key"),
        Some(&key_id),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(ApiKeyCreateResponse {
            api_key: ApiKeyResponse {
                id: key_id,
                name: name.to_string(),
                key_prefix,
                is_service_account: false,
                scopes,
                last_used_at: None,
                expires_at,
                created_at,
            },
            key: plaintext_key,
        }),
    ))
}

/// DELETE /api/v1/auth/api-keys/{key_id}
async fn revoke_api_key(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(key_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let result = sqlx::query("DELETE FROM api_keys WHERE id = ?1 AND user_id = ?2")
        .bind(&key_id)
        .bind(&auth.claims.sub)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("API key not found".into()));
    }

    crate::audit::log_action(
        &state.db,
        Some(&auth.claims.sub),
        None,
        "api_key.revoke",
        Some("api_key"),
        Some(&key_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/admin/api-keys
async fn list_service_api_keys(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<(String, String, String, i32, Option<String>, Option<String>, Option<String>, String)> =
        sqlx::query_as(
            "SELECT id, name, key_prefix, is_service_account, scopes, last_used_at, expires_at, created_at \
             FROM api_keys WHERE is_service_account = 1 ORDER BY created_at DESC",
        )
        .fetch_all(&state.db)
        .await
        .map_err(AppError::internal)?;

    let result: Vec<ApiKeyResponse> = rows
        .into_iter()
        .map(
            |(
                id,
                name,
                key_prefix,
                is_service_account,
                scopes,
                last_used_at,
                expires_at,
                created_at,
            )| {
                ApiKeyResponse {
                    id,
                    name,
                    key_prefix,
                    is_service_account: is_service_account != 0,
                    scopes: api_keys::parse_scope_json(scopes.as_deref()),
                    last_used_at,
                    expires_at,
                    created_at,
                }
            },
        )
        .collect();

    Ok(Json(result))
}

/// POST /api/v1/admin/api-keys
async fn create_service_api_key(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(req): Json<ApiKeyCreateRequest>,
) -> Result<impl IntoResponse, AppError> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("API key name is required".into()));
    }
    if name.len() > 100 {
        return Err(AppError::BadRequest(
            "API key name must be at most 100 characters".into(),
        ));
    }

    let expires_at = req
        .expires_at
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            chrono::DateTime::parse_from_rfc3339(value)
                .map(|dt| dt.with_timezone(&chrono::Utc).to_rfc3339())
                .map_err(|_| AppError::BadRequest("expires_at must be an RFC3339 timestamp".into()))
        })
        .transpose()?;

    if let Some(expiry) = expires_at.as_deref() {
        let expiry = chrono::DateTime::parse_from_rfc3339(expiry)
            .map_err(|_| AppError::BadRequest("expires_at must be an RFC3339 timestamp".into()))?
            .with_timezone(&chrono::Utc);
        if expiry <= chrono::Utc::now() {
            return Err(AppError::BadRequest(
                "expires_at must be in the future".into(),
            ));
        }
    }

    let plaintext_key = api_keys::generate_api_key();
    let key_hash = api_keys::hash_api_key(&plaintext_key);
    let key_prefix = api_keys::visible_prefix(&plaintext_key);
    let scopes = normalize_service_api_key_scopes(req.scopes)?;
    let scopes_json =
        api_keys::scope_json(&scopes).map_err(|e| AppError::Internal(e.to_string()))?;
    let key_id = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO api_keys (id, user_id, name, key_hash, key_prefix, is_service_account, scopes, expires_at, created_at) \
         VALUES (?1, NULL, ?2, ?3, ?4, 1, ?5, ?6, ?7)",
    )
    .bind(&key_id)
    .bind(name)
    .bind(&key_hash)
    .bind(&key_prefix)
    .bind(&scopes_json)
    .bind(&expires_at)
    .bind(&created_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "api_key.service_create",
        Some("api_key"),
        Some(&key_id),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(ApiKeyCreateResponse {
            api_key: ApiKeyResponse {
                id: key_id,
                name: name.to_string(),
                key_prefix,
                is_service_account: true,
                scopes,
                last_used_at: None,
                expires_at,
                created_at,
            },
            key: plaintext_key,
        }),
    ))
}

/// DELETE /api/v1/admin/api-keys/{key_id}
async fn revoke_service_api_key(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(key_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let result = sqlx::query("DELETE FROM api_keys WHERE id = ?1 AND is_service_account = 1")
        .bind(&key_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("API key not found".into()));
    }

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "api_key.service_revoke",
        Some("api_key"),
        Some(&key_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/auth/config (public — no auth required)
async fn auth_config(State(state): State<AppState>) -> impl IntoResponse {
    let oidc_config = load_oidc_config(&state)
        .await
        .unwrap_or_else(|_| oidc::OidcConfig::from_env());
    let require_2fa = config_value(&state, "require_2fa", Some("REQUIRE_2FA"), "false")
        .await
        .map(|v| v == "true")
        .unwrap_or(false);
    let allow_public_signups = public_signups_enabled(&state).await.unwrap_or(false);
    let site_name = config_value(&state, "site_name", Some("SITE_NAME"), "Starlane Notes")
        .await
        .unwrap_or_else(|_| "Starlane Notes".to_string());
    let max_upload_size_mb = config_value(
        &state,
        "max_upload_size_mb",
        Some("MAX_UPLOAD_SIZE_MB"),
        "100",
    )
    .await
    .ok()
    .and_then(|value| value.trim().parse::<usize>().ok())
    .filter(|value| *value > 0)
    .unwrap_or(100);

    let setup_completed: Option<(String,)> =
        sqlx::query_as("SELECT value FROM system_config WHERE key = 'setup_completed'")
            .fetch_optional(&state.db)
            .await
            .unwrap_or(None);

    Json(AuthConfigResponse {
        google_enabled: oauth::OAuthProvider::Google.is_configured(),
        github_enabled: oauth::OAuthProvider::GitHub.is_configured(),
        oidc_enabled: oidc_config.enabled,
        oidc_display_name: oidc_config.display_name,
        require_2fa,
        allow_public_signups,
        setup_completed: setup_completed.map(|s| s.0 == "true").unwrap_or(false),
        site_name,
        max_upload_size_mb,
    })
}

/// GET /api/v1/admin/users
async fn list_users(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, AppError> {
    let users: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        i32,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, email, display_name, avatar_url, role, is_active, created_at, updated_at \
             FROM users ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<serde_json::Value> = users
        .into_iter()
        .map(
            |(id, email, display_name, avatar_url, role, is_active, created_at, updated_at)| {
                json!({
                    "id": id,
                    "email": email,
                    "display_name": display_name,
                    "avatar_url": avatar_url,
                    "role": role,
                    "is_active": is_active != 0,
                    "created_at": created_at,
                    "updated_at": updated_at,
                })
            },
        )
        .collect();

    Ok(Json(result))
}

/// POST /api/v1/admin/users
async fn create_admin_user(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(req): Json<AdminCreateUserRequest>,
) -> Result<impl IntoResponse, AppError> {
    let email = req.email.trim().to_lowercase();
    let display_name = req.display_name.trim();
    let role = req
        .role
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("member");

    if email.is_empty() || display_name.is_empty() {
        return Err(AppError::BadRequest(
            "Email and display name are required".into(),
        ));
    }
    if email.len() > 254 {
        return Err(AppError::BadRequest(
            "Email must be at most 254 characters".into(),
        ));
    }
    if display_name.len() > 100 {
        return Err(AppError::BadRequest(
            "Display name must be at most 100 characters".into(),
        ));
    }
    if role != "admin" && role != "member" {
        return Err(AppError::BadRequest(
            "User role must be admin or member".into(),
        ));
    }
    crate::provisioning::validate_user_provisioning_policy(&state, &email, role).await?;

    let existing: Option<(String,)> =
        sqlx::query_as("SELECT id FROM users WHERE lower(email) = ?1")
            .bind(&email)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    if existing.is_some() {
        return Err(AppError::Conflict("Email already registered".into()));
    }

    let temporary_password = generate_temporary_password();
    let password_hash = password::hash_password(&temporary_password)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let user_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO users (id, email, display_name, password_hash, role) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&user_id)
    .bind(&email)
    .bind(display_name)
    .bind(&password_hash)
    .bind(role)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    create_personal_space(&state, &user_id, display_name).await?;

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "user.create",
        Some("user"),
        Some(&user_id),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": user_id,
            "email": email,
            "display_name": display_name,
            "avatar_url": null,
            "role": role,
            "is_active": true,
            "created_at": now,
            "updated_at": now,
            "temporary_password": temporary_password,
        })),
    ))
}

/// POST /api/v1/admin/users/{user_id}/password-reset
async fn reset_admin_user_password(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(user_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user: Option<(String,)> =
        sqlx::query_as("SELECT email FROM users WHERE id = ?1 AND is_active = 1")
            .bind(&user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    let (email,) = user.ok_or_else(|| AppError::NotFound("Active user not found".into()))?;

    let temporary_password = generate_temporary_password();
    let password_hash = password::hash_password(&temporary_password)
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE users SET password_hash = ?1, updated_at = ?2 WHERE id = ?3")
        .bind(&password_hash)
        .bind(&now)
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    sqlx::query("DELETE FROM sessions WHERE user_id = ?1")
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "user.password_reset",
        Some("user"),
        Some(&user_id),
    )
    .await;

    Ok(Json(json!({
        "email": email,
        "temporary_password": temporary_password,
    })))
}

/// PUT /api/v1/admin/users/{user_id}
async fn update_user_lifecycle(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(user_id): Path<String>,
    Json(req): Json<UserLifecycleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let existing: Option<(String, i32)> =
        sqlx::query_as("SELECT role, is_active FROM users WHERE id = ?1")
            .bind(&user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    let (current_role, current_active) =
        existing.ok_or_else(|| AppError::NotFound("User not found".into()))?;

    let next_role = req
        .role
        .as_deref()
        .map(str::trim)
        .filter(|role| !role.is_empty())
        .unwrap_or(&current_role);
    if next_role != "admin" && next_role != "member" {
        return Err(AppError::BadRequest(
            "User role must be admin or member".into(),
        ));
    }

    let next_active = req.is_active.unwrap_or(current_active != 0);
    if user_id == admin.claims.sub && (next_role != "admin" || !next_active) {
        return Err(AppError::BadRequest(
            "You cannot demote or deactivate your own admin account".into(),
        ));
    }

    if current_role == "admin" && (next_role != "admin" || !next_active) {
        let other_active_admins: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM users WHERE id != ?1 AND role = 'admin' AND is_active = 1",
        )
        .bind(&user_id)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::internal)?;
        if other_active_admins.0 == 0 {
            return Err(AppError::BadRequest(
                "At least one active admin is required".into(),
            ));
        }
    }

    let updated_at = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE users SET role = ?1, is_active = ?2, updated_at = ?3 WHERE id = ?4")
        .bind(next_role)
        .bind(if next_active { 1 } else { 0 })
        .bind(&updated_at)
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "user.lifecycle_update",
        Some("user"),
        Some(&user_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/admin/users/bulk-lifecycle
async fn bulk_update_user_lifecycle(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(req): Json<BulkUserLifecycleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let action = req.action.trim().to_lowercase();
    let next_active = match action.as_str() {
        "activate" => true,
        "deactivate" => false,
        _ => {
            return Err(AppError::BadRequest(
                "Bulk lifecycle action must be activate or deactivate".into(),
            ))
        }
    };

    let requested_count = req.user_ids.len();
    if requested_count == 0 {
        return Err(AppError::BadRequest(
            "At least one user id is required".into(),
        ));
    }

    let mut seen = HashSet::new();
    let mut target_user_ids = Vec::new();
    let mut skipped = Vec::new();
    for raw_user_id in req.user_ids {
        let user_id = raw_user_id.trim().to_string();
        if user_id.is_empty() {
            skipped.push(BulkUserLifecycleSkippedUser {
                user_id,
                reason: "empty_user_id".into(),
            });
            continue;
        }
        if !seen.insert(user_id.clone()) {
            skipped.push(BulkUserLifecycleSkippedUser {
                user_id,
                reason: "duplicate_user_id".into(),
            });
            continue;
        }
        target_user_ids.push(user_id);
    }

    if target_user_ids.is_empty() {
        return Err(AppError::BadRequest(
            "At least one non-empty user id is required".into(),
        ));
    }
    if target_user_ids.len() > 100 {
        return Err(AppError::BadRequest(
            "Bulk lifecycle requests can include at most 100 unique users".into(),
        ));
    }

    let active_admins: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE role = 'admin' AND is_active = 1")
            .fetch_one(&state.db)
            .await
            .map_err(AppError::internal)?;
    let mut active_admins_remaining = active_admins.0;
    let mut updated_count = 0usize;

    for user_id in target_user_ids {
        let existing: Option<(String, i32)> =
            sqlx::query_as("SELECT role, is_active FROM users WHERE id = ?1")
                .bind(&user_id)
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?;
        let Some((current_role, current_active)) = existing else {
            skipped.push(BulkUserLifecycleSkippedUser {
                user_id,
                reason: "user_not_found".into(),
            });
            continue;
        };

        let is_currently_active = current_active != 0;
        if is_currently_active == next_active {
            skipped.push(BulkUserLifecycleSkippedUser {
                user_id,
                reason: if next_active {
                    "already_active".into()
                } else {
                    "already_inactive".into()
                },
            });
            continue;
        }

        if !next_active && user_id == admin.claims.sub {
            skipped.push(BulkUserLifecycleSkippedUser {
                user_id,
                reason: "cannot_deactivate_self".into(),
            });
            continue;
        }

        if !next_active && current_role == "admin" {
            if active_admins_remaining <= 1 {
                skipped.push(BulkUserLifecycleSkippedUser {
                    user_id,
                    reason: "last_active_admin_required".into(),
                });
                continue;
            }
            active_admins_remaining -= 1;
        }

        let updated_at = chrono::Utc::now().to_rfc3339();
        sqlx::query("UPDATE users SET is_active = ?1, updated_at = ?2 WHERE id = ?3")
            .bind(if next_active { 1 } else { 0 })
            .bind(&updated_at)
            .bind(&user_id)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;

        crate::audit::log_action(
            &state.db,
            Some(&admin.claims.sub),
            None,
            "user.lifecycle_bulk_update",
            Some("user"),
            Some(&user_id),
        )
        .await;

        updated_count += 1;
    }

    Ok(Json(BulkUserLifecycleResponse {
        action,
        requested_count,
        updated_count,
        skipped_count: skipped.len(),
        skipped,
    }))
}

/// GET /api/v1/admin/users/{user_id}/sessions
async fn list_admin_user_sessions(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(user_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_exists: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE id = ?1")
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;
    if user_exists.is_none() {
        return Err(AppError::NotFound("User not found".into()));
    }

    let sessions: Vec<(
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, device_name, ip_address, user_agent, last_seen_at, expires_at, created_at \
         FROM sessions WHERE user_id = ?1 ORDER BY last_seen_at DESC",
    )
    .bind(&user_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<SessionResponse> = sessions
        .into_iter()
        .map(
            |(id, device_name, ip_address, user_agent, last_seen_at, expires_at, created_at)| {
                SessionResponse {
                    id,
                    device_name,
                    ip_address,
                    user_agent,
                    last_seen_at,
                    expires_at,
                    created_at,
                }
            },
        )
        .collect();

    Ok(Json(result))
}

/// DELETE /api/v1/admin/users/{user_id}/sessions/{session_id}
async fn revoke_admin_user_session(
    State(state): State<AppState>,
    admin: AdminUser,
    Path((user_id, session_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let result = sqlx::query("DELETE FROM sessions WHERE id = ?1 AND user_id = ?2")
        .bind(&session_id)
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Session not found".into()));
    }

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "user.session_revoke",
        Some("session"),
        Some(&session_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/v1/users/directory
async fn list_user_directory(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let users: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, email, display_name, avatar_url FROM users \
         WHERE is_active = 1 ORDER BY display_name, email",
    )
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<serde_json::Value> = users
        .into_iter()
        .map(|(id, email, display_name, avatar_url)| {
            json!({
                "id": id,
                "email": email,
                "display_name": display_name,
                "avatar_url": avatar_url,
            })
        })
        .collect();

    Ok(Json(result))
}

/// GET /api/v1/admin/config
async fn list_admin_config(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, AppError> {
    let stored: Vec<(String, String)> =
        sqlx::query_as("SELECT key, value FROM system_config ORDER BY key")
            .fetch_all(&state.db)
            .await
            .map_err(AppError::internal)?;
    let stored_map: HashMap<String, String> = stored.into_iter().collect();

    let known = known_config_entries();
    let mut entries: Vec<ConfigEntryResponse> = known
        .iter()
        .map(|entry| {
            let db_value = stored_map
                .get(entry.key)
                .cloned()
                .unwrap_or_else(|| entry.default.to_string());
            let env_value = entry.env_var.and_then(|env| std::env::var(env).ok());
            let effective_value = env_value.clone().unwrap_or_else(|| db_value.clone());
            ConfigEntryResponse {
                key: entry.key.to_string(),
                label: entry.label.to_string(),
                value: if entry.secret {
                    String::new()
                } else {
                    db_value
                },
                effective_value: if entry.secret {
                    mask_secret(&effective_value)
                } else {
                    effective_value
                },
                source: if env_value.is_some() {
                    "env".to_string()
                } else if stored_map.contains_key(entry.key) {
                    "database".to_string()
                } else {
                    "default".to_string()
                },
                env_var: entry.env_var.map(str::to_string),
                locked: env_value.is_some(),
                secret: entry.secret,
            }
        })
        .collect();

    for (key, value) in stored_map {
        if known.iter().any(|entry| entry.key == key) {
            continue;
        }

        entries.push(ConfigEntryResponse {
            key: key.clone(),
            label: key.replace('_', " "),
            value: if is_secret_key(&key) {
                String::new()
            } else {
                value.clone()
            },
            effective_value: if is_secret_key(&key) {
                mask_secret(&value)
            } else {
                value
            },
            source: "database".to_string(),
            env_var: None,
            locked: false,
            secret: is_secret_key(&key),
        });
    }

    entries.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(Json(entries))
}

/// POST /api/v1/admin/config
async fn upsert_admin_config(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(req): Json<ConfigUpsertRequest>,
) -> Result<impl IntoResponse, AppError> {
    let key = req.key.trim().to_lowercase();
    if !is_valid_config_key(&key) {
        return Err(AppError::BadRequest(
            "Config key must use lowercase letters, numbers, dots, dashes, or underscores".into(),
        ));
    }

    if config_env_var(&key)
        .and_then(|env| std::env::var(env).ok())
        .is_some()
    {
        return Err(AppError::BadRequest(
            "This config key is locked by an environment variable".into(),
        ));
    }

    let updated_at = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO system_config (key, value, updated_at) VALUES (?1, ?2, ?3) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(&key)
    .bind(&req.value)
    .bind(&updated_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "config.upsert",
        Some("system_config"),
        Some(&key),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/v1/admin/config/{key}
async fn delete_admin_config(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(key): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    if key == "setup_completed" {
        return Err(AppError::BadRequest(
            "setup_completed cannot be deleted".into(),
        ));
    }

    if config_env_var(&key)
        .and_then(|env| std::env::var(env).ok())
        .is_some()
    {
        return Err(AppError::BadRequest(
            "This config key is locked by an environment variable".into(),
        ));
    }

    sqlx::query("DELETE FROM system_config WHERE key = ?1")
        .bind(&key)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "config.delete",
        Some("system_config"),
        Some(&key),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

// ============================================================
// Helpers
// ============================================================

struct KnownConfigEntry {
    key: &'static str,
    label: &'static str,
    default: &'static str,
    env_var: Option<&'static str>,
    secret: bool,
}

fn known_config_entries() -> Vec<KnownConfigEntry> {
    vec![
        KnownConfigEntry {
            key: "site_name",
            label: "Site name",
            default: "Starlane Notes",
            env_var: Some("SITE_NAME"),
            secret: false,
        },
        KnownConfigEntry {
            key: "require_2fa",
            label: "Require 2FA",
            default: "false",
            env_var: Some("REQUIRE_2FA"),
            secret: false,
        },
        KnownConfigEntry {
            key: "allow_public_signups",
            label: "Allow public signups",
            default: "false",
            env_var: Some("ALLOW_PUBLIC_SIGNUPS"),
            secret: false,
        },
        KnownConfigEntry {
            key: "max_upload_size_mb",
            label: "Max upload size (MB)",
            default: "100",
            env_var: Some("MAX_UPLOAD_SIZE_MB"),
            secret: false,
        },
        KnownConfigEntry {
            key: "smtp_host",
            label: "SMTP host",
            default: "",
            env_var: Some("SMTP_HOST"),
            secret: false,
        },
        KnownConfigEntry {
            key: "smtp_port",
            label: "SMTP port",
            default: "587",
            env_var: Some("SMTP_PORT"),
            secret: false,
        },
        KnownConfigEntry {
            key: "smtp_user",
            label: "SMTP user",
            default: "",
            env_var: Some("SMTP_USER"),
            secret: false,
        },
        KnownConfigEntry {
            key: "smtp_password",
            label: "SMTP password",
            default: "",
            env_var: Some("SMTP_PASSWORD"),
            secret: true,
        },
        KnownConfigEntry {
            key: "smtp_from",
            label: "SMTP from address",
            default: "",
            env_var: Some("SMTP_FROM"),
            secret: false,
        },
        KnownConfigEntry {
            key: "oidc_enabled",
            label: "OIDC enabled",
            default: "false",
            env_var: Some("OIDC_ENABLED"),
            secret: false,
        },
        KnownConfigEntry {
            key: "oidc_display_name",
            label: "OIDC display name",
            default: "SSO",
            env_var: Some("OIDC_DISPLAY_NAME"),
            secret: false,
        },
        KnownConfigEntry {
            key: "oidc_issuer_url",
            label: "OIDC issuer URL",
            default: "",
            env_var: Some("OIDC_ISSUER_URL"),
            secret: false,
        },
        KnownConfigEntry {
            key: "oidc_client_id",
            label: "OIDC client ID",
            default: "",
            env_var: Some("OIDC_CLIENT_ID"),
            secret: false,
        },
        KnownConfigEntry {
            key: "oidc_client_secret",
            label: "OIDC client secret",
            default: "",
            env_var: Some("OIDC_CLIENT_SECRET"),
            secret: true,
        },
        KnownConfigEntry {
            key: "oidc_scopes",
            label: "OIDC scopes",
            default: "openid,profile,email",
            env_var: Some("OIDC_SCOPES"),
            secret: false,
        },
        KnownConfigEntry {
            key: "oidc_user_claim",
            label: "OIDC user claim",
            default: "email",
            env_var: Some("OIDC_USER_CLAIM"),
            secret: false,
        },
        KnownConfigEntry {
            key: "oidc_role_claim",
            label: "OIDC role claim",
            default: "",
            env_var: Some("OIDC_ROLE_CLAIM"),
            secret: false,
        },
        KnownConfigEntry {
            key: "oidc_role_admin_value",
            label: "OIDC admin role value",
            default: "admin",
            env_var: Some("OIDC_ROLE_ADMIN_VALUE"),
            secret: false,
        },
        KnownConfigEntry {
            key: "oidc_auto_provision",
            label: "OIDC auto provision",
            default: "true",
            env_var: Some("OIDC_AUTO_PROVISION"),
            secret: false,
        },
        KnownConfigEntry {
            key: "user_provisioning_allowed_email_domains",
            label: "Allowed provisioning email domains",
            default: "",
            env_var: Some("USER_PROVISIONING_ALLOWED_EMAIL_DOMAINS"),
            secret: false,
        },
        KnownConfigEntry {
            key: "user_provisioning_allowed_roles",
            label: "Allowed provisioning roles",
            default: "admin,member,viewer",
            env_var: Some("USER_PROVISIONING_ALLOWED_ROLES"),
            secret: false,
        },
    ]
}

fn config_env_var(key: &str) -> Option<&'static str> {
    known_config_entries()
        .into_iter()
        .find(|entry| entry.key == key)
        .and_then(|entry| entry.env_var)
}

async fn config_value(
    state: &AppState,
    key: &str,
    env_var: Option<&str>,
    default: &str,
) -> Result<String, AppError> {
    if let Some(env) = env_var {
        if let Ok(value) = std::env::var(env) {
            return Ok(value);
        }
    }

    let stored: Option<(String,)> =
        sqlx::query_as("SELECT value FROM system_config WHERE key = ?1")
            .bind(key)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    Ok(stored
        .map(|item| item.0)
        .unwrap_or_else(|| default.to_string()))
}

async fn public_signups_enabled(state: &AppState) -> Result<bool, AppError> {
    config_value(
        state,
        "allow_public_signups",
        Some("ALLOW_PUBLIC_SIGNUPS"),
        "false",
    )
    .await
    .map(|value| value.trim().eq_ignore_ascii_case("true"))
}

fn is_valid_config_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 100
        && key.chars().all(|c| {
            c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-' || c == '.'
        })
}

fn is_secret_key(key: &str) -> bool {
    let lowered = key.to_lowercase();
    lowered.contains("secret")
        || lowered.contains("password")
        || lowered.contains("token")
        || lowered.contains("key")
}

fn mask_secret(value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else {
        "********".to_string()
    }
}

async fn load_oidc_config(state: &AppState) -> Result<oidc::OidcConfig, AppError> {
    let scopes = config_value(
        state,
        "oidc_scopes",
        Some("OIDC_SCOPES"),
        "openid,profile,email",
    )
    .await?
    .split(',')
    .map(|scope| scope.trim().to_string())
    .filter(|scope| !scope.is_empty())
    .collect();

    Ok(oidc::OidcConfig {
        enabled: config_value(state, "oidc_enabled", Some("OIDC_ENABLED"), "false").await?
            == "true",
        display_name: config_value(state, "oidc_display_name", Some("OIDC_DISPLAY_NAME"), "SSO")
            .await?,
        issuer_url: config_value(state, "oidc_issuer_url", Some("OIDC_ISSUER_URL"), "").await?,
        client_id: config_value(state, "oidc_client_id", Some("OIDC_CLIENT_ID"), "").await?,
        client_secret: config_value(state, "oidc_client_secret", Some("OIDC_CLIENT_SECRET"), "")
            .await?,
        scopes,
        user_claim: config_value(state, "oidc_user_claim", Some("OIDC_USER_CLAIM"), "email")
            .await?,
        role_claim: config_value(state, "oidc_role_claim", Some("OIDC_ROLE_CLAIM"), "").await?,
        role_admin_value: config_value(
            state,
            "oidc_role_admin_value",
            Some("OIDC_ROLE_ADMIN_VALUE"),
            "admin",
        )
        .await?,
        auto_provision: config_value(
            state,
            "oidc_auto_provision",
            Some("OIDC_AUTO_PROVISION"),
            "true",
        )
        .await?
            == "true",
    })
}

/// Find existing user by OAuth account or create a new one
async fn find_or_create_oauth_user(
    state: &AppState,
    info: &oauth::OAuthUserInfo,
    allow_auto_provision: bool,
) -> Result<(String, String), AppError> {
    // Check if OAuth account already linked
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT user_id FROM oauth_accounts WHERE provider = ?1 AND provider_user_id = ?2",
    )
    .bind(&info.provider)
    .bind(&info.provider_user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    if let Some((user_id,)) = existing {
        // Get current role
        let role: (String,) = sqlx::query_as("SELECT role FROM users WHERE id = ?1")
            .bind(&user_id)
            .fetch_one(&state.db)
            .await
            .map_err(AppError::internal)?;

        return Ok((user_id, role.0));
    }

    // Check if user exists by email (link OAuth to existing account)
    let existing_user: Option<(String, String)> =
        sqlx::query_as("SELECT id, role FROM users WHERE email = ?1")
            .bind(&info.email)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    let (user_id, role) = if let Some((uid, role)) = existing_user {
        (uid, role)
    } else {
        require_oauth_auto_provisioning(&info.provider, allow_auto_provision)?;

        // Create new user
        let user_id = Uuid::new_v4().to_string();
        let role = "member".to_string();

        sqlx::query(
            "INSERT INTO users (id, email, display_name, avatar_url, role) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(&user_id)
        .bind(&info.email)
        .bind(&info.display_name)
        .bind(&info.avatar_url)
        .bind(&role)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

        create_personal_space(state, &user_id, &info.display_name).await?;

        (user_id, role)
    };

    // Link OAuth account
    let oauth_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO oauth_accounts (id, user_id, provider, provider_user_id, provider_email) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&oauth_id)
    .bind(&user_id)
    .bind(&info.provider)
    .bind(&info.provider_user_id)
    .bind(&info.email)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok((user_id, role))
}

fn require_oauth_auto_provisioning(
    provider: &str,
    allow_auto_provision: bool,
) -> Result<(), AppError> {
    if allow_auto_provision {
        return Ok(());
    }

    Err(AppError::Unauthorized(format!(
        "{} auto provisioning is disabled; ask an administrator to create or link your account",
        provider.to_uppercase()
    )))
}

pub(crate) async fn create_personal_space(
    state: &AppState,
    user_id: &str,
    display_name: &str,
) -> Result<(), AppError> {
    let space_id = Uuid::new_v4().to_string();
    let member_id = Uuid::new_v4().to_string();
    let space_name = format!("{}'s Space", display_name.trim());
    let fallback_slug = format!("personal-{}", user_id);
    let mut slug = storage::sanitize_slug(&space_name.replace(' ', "-"));

    if slug.is_empty() {
        slug = storage::sanitize_slug(&fallback_slug);
    }

    let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM spaces WHERE slug = ?1")
        .bind(&slug)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;

    if existing.is_some() {
        slug = storage::sanitize_slug(&fallback_slug);
    }

    sqlx::query(
        "INSERT INTO spaces (id, name, slug, visibility, created_by) VALUES (?1, ?2, ?3, 'private', ?4)",
    )
    .bind(&space_id)
    .bind(&space_name)
    .bind(&slug)
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    sqlx::query(
        "INSERT INTO space_members (id, space_id, user_id, role) VALUES (?1, ?2, ?3, 'owner')",
    )
    .bind(&member_id)
    .bind(&space_id)
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;

    storage::init_space_dirs(&slug).await.map_err(|e| {
        AppError::Internal(format!("Failed to create personal space directory: {}", e))
    })?;

    crate::audit::log_action(
        &state.db,
        Some(user_id),
        None,
        "space.personal_create",
        Some("space"),
        Some(&space_id),
    )
    .await;

    Ok(())
}

// ============================================================
// Error Types
// ============================================================

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    TwoFactorRequired(String),
    NotFound(String),
    Conflict(String),
    Internal(String),
}

impl AppError {
    pub fn internal(e: impl std::fmt::Display) -> Self {
        tracing::error!("Internal error: {}", e);
        AppError::Internal("Internal server error".into())
    }
}

fn auth_rate_limit_error_response(error: tower_governor::GovernorError) -> Response {
    let (status, error_code, message, headers) = match error {
        tower_governor::GovernorError::TooManyRequests { headers, .. } => (
            StatusCode::TOO_MANY_REQUESTS,
            "too_many_requests",
            "Rate limit exceeded".to_string(),
            headers,
        ),
        tower_governor::GovernorError::UnableToExtractKey => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Unable to apply rate limit".to_string(),
            None,
        ),
        tower_governor::GovernorError::Other { code, msg, headers } => (
            code,
            "internal_error",
            msg.unwrap_or_else(|| "Rate limiter error".to_string()),
            headers,
        ),
    };

    let mut response = (
        status,
        Json(json!({ "error": error_code, "message": message })),
    )
        .into_response();
    if let Some(headers) = headers {
        response.headers_mut().extend(headers);
    }
    response
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error, message) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "unauthorized", msg),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, "forbidden", msg),
            AppError::TwoFactorRequired(msg) => (StatusCode::FORBIDDEN, "2fa_required", msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, "not_found", msg),
            AppError::Conflict(msg) => (StatusCode::CONFLICT, "conflict", msg),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error", msg),
        };

        (status, Json(json!({ "error": error, "message": message }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::any::AnyPoolOptions;
    use totp_rs::{Algorithm, Secret, TOTP};

    #[test]
    fn webauthn_rp_id_uses_base_url_host_including_localhost() {
        let localhost = Url::parse("http://localhost:3000").expect("localhost URL");
        let production = Url::parse("https://notes.example.com").expect("production URL");

        assert_eq!(
            webauthn_rp_id(&localhost).expect("localhost rp id"),
            "localhost"
        );
        assert_eq!(
            webauthn_rp_id(&production).expect("production rp id"),
            "notes.example.com"
        );
    }

    #[test]
    fn webauthn_origins_include_base_native_and_extra_origins() {
        let origins = webauthn_allowed_origins(
            "https://notes.example.com/",
            Some("https://mobile.example.com/, http://tauri.localhost".to_string()),
        )
        .expect("webauthn origins");
        let origins = origins
            .into_iter()
            .map(|origin| origin.as_str().trim_end_matches('/').to_string())
            .collect::<Vec<_>>();

        assert!(origins.contains(&"https://notes.example.com".to_string()));
        assert!(origins.contains(&"http://tauri.localhost".to_string()));
        assert!(origins.contains(&"https://tauri.localhost".to_string()));
        assert!(origins.contains(&"tauri://localhost".to_string()));
        assert!(origins.contains(&"https://mobile.example.com".to_string()));
        assert_eq!(
            origins
                .iter()
                .filter(|origin| origin.as_str() == "http://tauri.localhost")
                .count(),
            1,
        );
    }

    #[test]
    fn webauthn_origins_reject_wildcards() {
        assert!(
            webauthn_allowed_origins("https://notes.example.com", Some("*".to_string())).is_err()
        );
    }

    async fn logout_test_state() -> AppState {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");

        sqlx::query(
            "CREATE TABLE sessions (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL,
                refresh_token_hash TEXT NOT NULL,
                expires_at TEXT NOT NULL
            )",
        )
        .execute(&db)
        .await
        .expect("create sessions");

        sqlx::query(
            "CREATE TABLE users (
                id TEXT PRIMARY KEY NOT NULL,
                email TEXT NOT NULL,
                username TEXT,
                display_name TEXT NOT NULL DEFAULT 'Test User',
                avatar_url TEXT,
                role TEXT NOT NULL DEFAULT 'member',
                password_hash TEXT,
                totp_secret TEXT,
                totp_enabled INTEGER NOT NULL DEFAULT 0,
                recovery_codes TEXT,
                is_active INTEGER NOT NULL DEFAULT 1,
                updated_at TEXT
            )",
        )
        .execute(&db)
        .await
        .expect("create users");

        sqlx::query(
            "CREATE TABLE password_reset_tokens (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL,
                token_hash TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                used_at TEXT
            )",
        )
        .execute(&db)
        .await
        .expect("create password reset tokens");

        sqlx::query(
            "CREATE TABLE system_config (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT
            )",
        )
        .execute(&db)
        .await
        .expect("create system config");

        sqlx::query(
            "CREATE TABLE passkey_challenges (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT,
                email TEXT,
                kind TEXT NOT NULL,
                state_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                expires_at TEXT NOT NULL
            )",
        )
        .execute(&db)
        .await
        .expect("create passkey challenges");

        sqlx::query(
            "CREATE TABLE passkeys (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL,
                credential_id TEXT NOT NULL UNIQUE,
                public_key TEXT NOT NULL,
                name TEXT NOT NULL,
                sign_count INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                last_used_at TEXT
            )",
        )
        .execute(&db)
        .await
        .expect("create passkeys");

        let index_path = std::env::temp_dir().join(format!(
            "starlane-notes-logout-route-test-{}",
            Uuid::new_v4()
        ));

        AppState {
            db,
            jwt_secret: "test-secret-with-enough-length-for-jwt".to_string(),
            base_url: "http://localhost:3000".to_string(),
            search_index: crate::search::SearchIndex::new(index_path).expect("search index"),
            oauth_cache: OAuthStateCache::new(),
        }
    }

    fn test_auth_user(user_id: &str, email: &str) -> AuthUser {
        let now = chrono::Utc::now();
        AuthUser {
            claims: jwt::Claims {
                sub: user_id.to_string(),
                sid: format!("{user_id}-session"),
                email: email.to_string(),
                role: "admin".to_string(),
                exp: (now + chrono::Duration::minutes(15)).timestamp(),
                iat: now.timestamp(),
                token_type: jwt::TokenType::Access,
            },
            api_key_scopes: None,
        }
    }

    async fn insert_logout_test_session(state: &AppState, session_id: &str) {
        insert_test_session_for_user(state, "user-1", session_id).await;
    }

    async fn insert_test_session_for_user(state: &AppState, user_id: &str, session_id: &str) {
        sqlx::query(
            "INSERT INTO sessions (id, user_id, refresh_token_hash, expires_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(session_id)
        .bind(user_id)
        .bind(format!("{session_id}-hash"))
        .bind((chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339())
        .execute(&state.db)
        .await
            .expect("insert session");
    }

    async fn insert_login_test_user(
        state: &AppState,
        user_id: &str,
        email: &str,
        password: &str,
        totp_secret: Option<&str>,
        totp_enabled: bool,
    ) {
        let password_hash = password::hash_password(password).expect("hash test password");
        sqlx::query(
            "INSERT INTO users \
             (id, email, display_name, role, password_hash, totp_secret, totp_enabled, is_active, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8)",
        )
        .bind(user_id)
        .bind(email)
        .bind("The Eagle")
        .bind("admin")
        .bind(password_hash)
        .bind(totp_secret)
        .bind(if totp_enabled { 1 } else { 0 })
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.db)
        .await
        .expect("insert login test user");
    }

    fn current_totp_code(secret: &str) -> String {
        let secret = Secret::Encoded(secret.to_string());
        let totp = TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            secret.to_bytes().expect("decode TOTP secret"),
            None,
            String::new(),
        )
        .expect("build test TOTP");
        totp.generate_current().expect("generate current TOTP")
    }

    async fn set_user_recovery_codes(state: &AppState, user_id: &str, codes: &[String]) {
        let recovery_code_hashes =
            serde_json::to_string(&hash_totp_recovery_codes(codes)).expect("serialize codes");
        sqlx::query("UPDATE users SET recovery_codes = ?1 WHERE id = ?2")
            .bind(recovery_code_hashes)
            .bind(user_id)
            .execute(&state.db)
            .await
            .expect("set user recovery codes");
    }

    async fn recovery_code_count(state: &AppState, user_id: &str) -> usize {
        let row: (Option<String>,) =
            sqlx::query_as("SELECT recovery_codes FROM users WHERE id = ?1")
                .bind(user_id)
                .fetch_one(&state.db)
                .await
                .expect("load recovery codes");
        row.0
            .map(|json| {
                serde_json::from_str::<Vec<String>>(&json)
                    .expect("parse codes")
                    .len()
            })
            .unwrap_or(0)
    }

    async fn insert_test_passkey(
        state: &AppState,
        passkey_id: &str,
        user_id: &str,
        name: &str,
        created_at: &str,
    ) {
        sqlx::query(
            "INSERT INTO passkeys \
             (id, user_id, credential_id, public_key, name, sign_count, created_at, last_used_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6, NULL)",
        )
        .bind(passkey_id)
        .bind(user_id)
        .bind(format!("{passkey_id}-credential"))
        .bind(r#"{"placeholder":true}"#)
        .bind(name)
        .bind(created_at)
        .execute(&state.db)
        .await
        .expect("insert passkey");
    }

    async fn count_passkey_challenges(state: &AppState) -> i64 {
        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM passkey_challenges")
            .fetch_one(&state.db)
            .await
            .expect("count passkey challenges");
        count.0
    }

    async fn insert_password_reset_user(state: &AppState, user_id: &str, token: &str) {
        sqlx::query(
            "INSERT INTO users (id, email, password_hash, is_active, updated_at) VALUES (?1, ?2, ?3, 1, ?4)",
        )
        .bind(user_id)
        .bind(format!("{user_id}@example.com"))
        .bind("old-password-hash")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.db)
        .await
        .expect("insert user");

        let expires_at = (chrono::Utc::now() + chrono::Duration::minutes(30)).to_rfc3339();
        for suffix in ["current", "other"] {
            sqlx::query(
                "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, used_at) VALUES (?1, ?2, ?3, ?4, NULL)",
            )
            .bind(format!("{user_id}-{suffix}"))
            .bind(user_id)
            .bind(if suffix == "current" {
                hash_password_reset_token(token)
            } else {
                hash_password_reset_token(&format!("{token}-{suffix}"))
            })
            .bind(&expires_at)
            .execute(&state.db)
            .await
            .expect("insert reset token");
        }
    }

    #[test]
    fn password_reset_token_hash_is_stable_and_non_plaintext() {
        let token = "sample-reset-token-for-test";
        let hash = hash_password_reset_token(token);

        assert_eq!(hash.len(), 64);
        assert_eq!(hash, hash_password_reset_token(token));
        assert_ne!(hash, token);
    }

    #[test]
    fn refresh_token_hash_is_stable_and_non_plaintext() {
        let token = "refresh-token-value";
        let hash = hash_refresh_token(token);
        assert_ne!(hash, token);
        assert_eq!(hash, hash_refresh_token(token));
    }

    #[test]
    fn totp_recovery_code_hash_normalizes_dashless_input() {
        assert_eq!(normalize_totp_recovery_code("12345678"), "1234-5678");
        assert_eq!(normalize_totp_recovery_code(" 1234-5678 "), "1234-5678");
        assert_eq!(
            hash_totp_recovery_code("12345678"),
            hash_totp_recovery_code("1234-5678"),
        );
    }

    #[tokio::test]
    async fn auth_rate_limit_error_response_uses_json_error_envelope() {
        let response =
            auth_rate_limit_error_response(tower_governor::GovernorError::TooManyRequests {
                wait_time: 3,
                headers: None,
            });

        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("rate-limit response body");
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).expect("rate-limit response JSON");

        assert_eq!(body["error"], "too_many_requests");
        assert_eq!(body["message"], "Rate limit exceeded");
    }

    #[tokio::test]
    async fn app_error_forbidden_uses_documented_json_error_envelope() {
        let response = AppError::Forbidden("Admin access required".into()).into_response();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("forbidden response body");
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).expect("forbidden response JSON");

        assert_eq!(body["error"], "forbidden");
        assert_eq!(body["message"], "Admin access required");
    }

    #[test]
    fn auth_config_entries_include_opt_in_public_signups() {
        let entry = known_config_entries()
            .into_iter()
            .find(|entry| entry.key == "allow_public_signups")
            .expect("allow_public_signups config entry");

        assert_eq!(entry.label, "Allow public signups");
        assert_eq!(entry.default, "false");
        assert_eq!(entry.env_var, Some("ALLOW_PUBLIC_SIGNUPS"));
        assert!(!entry.secret);
    }

    #[tokio::test]
    async fn register_blocks_additional_users_when_public_signups_are_disabled() {
        let state = logout_test_state().await;
        sqlx::query(
            "INSERT INTO users (id, email, display_name, role, password_hash, is_active, updated_at) \
             VALUES ('existing-user', 'existing@example.com', 'Existing User', 'admin', 'hash', 1, ?1)",
        )
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.db)
        .await
        .expect("insert existing user");

        let result = register(
            State(state),
            Json(RegisterRequest {
                email: "new@example.com".to_string(),
                display_name: "New User".to_string(),
                password: "correct-password".to_string(),
            }),
        )
        .await;

        match result {
            Err(AppError::Forbidden(message)) => assert!(
                message.contains("Public signups are disabled"),
                "unexpected message: {message}"
            ),
            Ok(_) => panic!("registration should be blocked"),
            Err(other) => panic!("expected forbidden public signup error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn logout_deletes_current_server_session_only() {
        let state = logout_test_state().await;
        insert_logout_test_session(&state, "session-current").await;
        insert_logout_test_session(&state, "session-other").await;
        let now = chrono::Utc::now();
        let auth = AuthUser {
            claims: jwt::Claims {
                sub: "user-1".to_string(),
                sid: "session-current".to_string(),
                email: "eagle@example.com".to_string(),
                role: "admin".to_string(),
                exp: (now + chrono::Duration::minutes(15)).timestamp(),
                iat: now.timestamp(),
                token_type: jwt::TokenType::Access,
            },
            api_key_scopes: None,
        };

        let status = logout(State(state.clone()), auth).await.expect("logout");

        assert_eq!(status, StatusCode::NO_CONTENT);

        let current_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE id = ?1")
            .bind("session-current")
            .fetch_one(&state.db)
            .await
            .expect("count current session");
        let other_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE id = ?1")
            .bind("session-other")
            .fetch_one(&state.db)
            .await
            .expect("count other session");

        assert_eq!(current_count.0, 0);
        assert_eq!(other_count.0, 1);
    }

    #[tokio::test]
    async fn passkey_challenge_take_is_single_use_and_preserves_metadata() {
        let state = logout_test_state().await;
        let challenge_id = store_passkey_challenge(
            &state,
            Some("user-1"),
            Some("eagle@example.com"),
            "registration",
            &serde_json::json!({ "state": "created" }),
        )
        .await
        .expect("store passkey challenge");

        let (user_id, email, passkey_state): (Option<String>, Option<String>, serde_json::Value) =
            take_passkey_challenge(&state, &challenge_id, "registration")
                .await
                .expect("take passkey challenge");

        assert_eq!(user_id.as_deref(), Some("user-1"));
        assert_eq!(email.as_deref(), Some("eagle@example.com"));
        assert_eq!(passkey_state["state"], "created");
        assert_eq!(count_passkey_challenges(&state).await, 0);

        let error =
            take_passkey_challenge::<serde_json::Value>(&state, &challenge_id, "registration")
                .await
                .expect_err("challenge should be single-use");

        match error {
            AppError::BadRequest(message) => {
                assert_eq!(message, "Passkey challenge is invalid or expired");
            }
            other => panic!("expected single-use rejection, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn passkey_challenge_kind_mismatch_consumes_challenge() {
        let state = logout_test_state().await;
        let challenge_id = store_passkey_challenge(
            &state,
            Some("user-1"),
            Some("eagle@example.com"),
            "registration",
            &serde_json::json!({ "state": "created" }),
        )
        .await
        .expect("store passkey challenge");

        let error =
            take_passkey_challenge::<serde_json::Value>(&state, &challenge_id, "authentication")
                .await
                .expect_err("wrong challenge kind should fail");

        match error {
            AppError::BadRequest(message) => {
                assert_eq!(
                    message,
                    "Passkey challenge type does not match this request"
                );
            }
            other => panic!("expected kind mismatch rejection, got {other:?}"),
        }
        assert_eq!(count_passkey_challenges(&state).await, 0);
    }

    #[tokio::test]
    async fn passkey_challenge_expiry_blocks_and_cleans_up_take() {
        let state = logout_test_state().await;
        sqlx::query(
            "INSERT INTO passkey_challenges (id, user_id, email, kind, state_json, expires_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind("expired-challenge")
        .bind("user-1")
        .bind("eagle@example.com")
        .bind("registration")
        .bind(r#"{"state":"expired"}"#)
        .bind((chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339())
        .execute(&state.db)
        .await
        .expect("insert expired passkey challenge");

        let error = take_passkey_challenge::<serde_json::Value>(
            &state,
            "expired-challenge",
            "registration",
        )
        .await
        .expect_err("expired challenge should fail");

        match error {
            AppError::BadRequest(message) => {
                assert_eq!(message, "Passkey challenge is invalid or expired");
            }
            other => panic!("expected expired rejection, got {other:?}"),
        }
        assert_eq!(count_passkey_challenges(&state).await, 0);
    }

    #[tokio::test]
    async fn expired_passkey_challenge_cleanup_preserves_live_challenges() {
        let state = logout_test_state().await;
        let live_challenge = store_passkey_challenge(
            &state,
            Some("user-1"),
            Some("eagle@example.com"),
            "authentication",
            &serde_json::json!({ "state": "live" }),
        )
        .await
        .expect("store live passkey challenge");
        sqlx::query(
            "INSERT INTO passkey_challenges (id, user_id, email, kind, state_json, expires_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind("expired-challenge")
        .bind("user-1")
        .bind("eagle@example.com")
        .bind("registration")
        .bind(r#"{"state":"expired"}"#)
        .bind((chrono::Utc::now() - chrono::Duration::minutes(1)).to_rfc3339())
        .execute(&state.db)
        .await
        .expect("insert expired passkey challenge");

        delete_expired_passkey_challenges(&state).await;

        let ids: Vec<(String,)> = sqlx::query_as("SELECT id FROM passkey_challenges")
            .fetch_all(&state.db)
            .await
            .expect("list passkey challenges");
        assert_eq!(ids, vec![(live_challenge,)]);
    }

    #[test]
    fn passkey_registration_challenge_must_belong_to_authenticated_user() {
        assert!(
            validate_passkey_registration_challenge_user(Some("user-1".to_string()), "user-1")
                .is_ok()
        );

        match validate_passkey_registration_challenge_user(Some("user-1".to_string()), "user-2")
            .expect_err("mismatched registration challenge should fail")
        {
            AppError::Unauthorized(message) => assert_eq!(
                message,
                "Passkey registration challenge does not belong to this account"
            ),
            other => panic!("expected unauthorized challenge owner rejection, got {other:?}"),
        }

        match validate_passkey_registration_challenge_user(None, "user-1")
            .expect_err("missing registration challenge user should fail")
        {
            AppError::BadRequest(message) => assert_eq!(
                message,
                "Passkey registration challenge is missing account state"
            ),
            other => panic!("expected missing challenge owner rejection, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_passkeys_only_returns_authenticated_user_passkeys() {
        let state = logout_test_state().await;
        insert_test_passkey(
            &state,
            "passkey-eagle",
            "user-1",
            "Eagle phone",
            "2026-07-16T00:00:00Z",
        )
        .await;
        insert_test_passkey(
            &state,
            "passkey-caboose",
            "user-2",
            "Caboose keyboard",
            "2026-07-16T00:01:00Z",
        )
        .await;

        let response = list_passkeys(
            State(state.clone()),
            test_auth_user("user-1", "eagle@example.com"),
        )
        .await
        .expect("list passkeys")
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("passkey list body");
        let body: serde_json::Value = serde_json::from_slice(&bytes).expect("passkey list JSON");

        assert_eq!(body.as_array().expect("passkey array").len(), 1);
        assert_eq!(body[0]["id"], "passkey-eagle");
        assert_eq!(body[0]["name"], "Eagle phone");
        assert_eq!(body[0]["last_used_at"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn delete_passkey_rejects_other_users_passkeys_without_deleting_them() {
        let state = logout_test_state().await;
        insert_test_passkey(
            &state,
            "passkey-caboose",
            "user-2",
            "Caboose keyboard",
            "2026-07-16T00:01:00Z",
        )
        .await;

        let error = delete_passkey(
            State(state.clone()),
            test_auth_user("user-1", "eagle@example.com"),
            Path("passkey-caboose".to_string()),
        )
        .await
        .expect_err("other user's passkey should not be deleted");

        match error {
            AppError::NotFound(message) => assert_eq!(message, "Passkey not found"),
            other => panic!("expected not found for another user's passkey, got {other:?}"),
        }

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM passkeys WHERE id = ?1")
            .bind("passkey-caboose")
            .fetch_one(&state.db)
            .await
            .expect("count other user's passkey");
        assert_eq!(count.0, 1);
    }

    #[tokio::test]
    async fn login_requires_totp_code_when_two_factor_is_enabled() {
        let state = logout_test_state().await;
        let setup = totp::generate_totp_setup("eagle@example.com").expect("totp setup");
        insert_login_test_user(
            &state,
            "user-totp",
            "eagle@example.com",
            "correct-password",
            Some(&setup.secret),
            true,
        )
        .await;

        let result = login(
            State(state.clone()),
            Json(LoginRequest {
                email: "eagle@example.com".to_string(),
                password: "correct-password".to_string(),
                totp_code: None,
            }),
        )
        .await;
        let error = match result {
            Ok(_) => panic!("missing TOTP should fail"),
            Err(error) => error,
        };

        match error {
            AppError::TwoFactorRequired(message) => assert_eq!(message, "2FA code required"),
            other => panic!("expected 2FA requirement, got {other:?}"),
        }

        let session_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sessions")
            .fetch_one(&state.db)
            .await
            .expect("count sessions");
        assert_eq!(session_count.0, 0);
    }

    #[tokio::test]
    async fn login_rejects_invalid_totp_code_without_creating_session() {
        let state = logout_test_state().await;
        let setup = totp::generate_totp_setup("eagle@example.com").expect("totp setup");
        insert_login_test_user(
            &state,
            "user-totp",
            "eagle@example.com",
            "correct-password",
            Some(&setup.secret),
            true,
        )
        .await;

        let result = login(
            State(state.clone()),
            Json(LoginRequest {
                email: "eagle@example.com".to_string(),
                password: "correct-password".to_string(),
                totp_code: Some("000000".to_string()),
            }),
        )
        .await;
        let error = match result {
            Ok(_) => panic!("invalid TOTP should fail"),
            Err(error) => error,
        };

        match error {
            AppError::Unauthorized(message) => assert_eq!(message, "Invalid 2FA code"),
            other => panic!("expected invalid TOTP rejection, got {other:?}"),
        }

        let session_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sessions")
            .fetch_one(&state.db)
            .await
            .expect("count sessions");
        assert_eq!(session_count.0, 0);
    }

    #[tokio::test]
    async fn login_with_valid_totp_code_issues_tokens_and_persists_session() {
        let state = logout_test_state().await;
        let setup = totp::generate_totp_setup("eagle@example.com").expect("totp setup");
        insert_login_test_user(
            &state,
            "user-totp",
            "eagle@example.com",
            "correct-password",
            Some(&setup.secret),
            true,
        )
        .await;

        let response = login(
            State(state.clone()),
            Json(LoginRequest {
                email: "eagle@example.com".to_string(),
                password: "correct-password".to_string(),
                totp_code: Some(current_totp_code(&setup.secret)),
            }),
        )
        .await
        .expect("valid TOTP login")
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("login body");
        let body: serde_json::Value = serde_json::from_slice(&body_bytes).expect("login JSON");
        assert!(body["access_token"]
            .as_str()
            .is_some_and(|token| !token.is_empty()));
        assert!(body["refresh_token"]
            .as_str()
            .is_some_and(|token| !token.is_empty()));
        assert_eq!(body["user"]["email"], "eagle@example.com");
        assert_eq!(body["user"]["totp_enabled"], true);

        let session_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id = ?1")
                .bind("user-totp")
                .fetch_one(&state.db)
                .await
                .expect("count user sessions");
        assert_eq!(session_count.0, 1);
    }

    #[tokio::test]
    async fn login_accepts_recovery_code_once_and_consumes_it() {
        let state = logout_test_state().await;
        let setup = totp::generate_totp_setup("eagle@example.com").expect("totp setup");
        insert_login_test_user(
            &state,
            "user-totp",
            "eagle@example.com",
            "correct-password",
            Some(&setup.secret),
            true,
        )
        .await;
        set_user_recovery_codes(&state, "user-totp", &setup.recovery_codes).await;
        let recovery_code_without_dash = setup.recovery_codes[0].replace('-', "");

        let response = login(
            State(state.clone()),
            Json(LoginRequest {
                email: "eagle@example.com".to_string(),
                password: "correct-password".to_string(),
                totp_code: Some(recovery_code_without_dash),
            }),
        )
        .await
        .expect("recovery code login")
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(recovery_code_count(&state, "user-totp").await, 7);

        let result = login(
            State(state.clone()),
            Json(LoginRequest {
                email: "eagle@example.com".to_string(),
                password: "correct-password".to_string(),
                totp_code: Some(setup.recovery_codes[0].clone()),
            }),
        )
        .await;
        let error = match result {
            Ok(_) => panic!("reused recovery code should fail"),
            Err(error) => error,
        };

        match error {
            AppError::Unauthorized(message) => assert_eq!(message, "Invalid 2FA code"),
            other => panic!("expected reused recovery code rejection, got {other:?}"),
        }
        assert_eq!(recovery_code_count(&state, "user-totp").await, 7);

        let session_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id = ?1")
                .bind("user-totp")
                .fetch_one(&state.db)
                .await
                .expect("count user sessions");
        assert_eq!(session_count.0, 1);
    }

    #[tokio::test]
    async fn totp_disable_accepts_recovery_code_and_clears_secret_and_codes() {
        let state = logout_test_state().await;
        let setup = totp::generate_totp_setup("eagle@example.com").expect("totp setup");
        insert_login_test_user(
            &state,
            "user-totp",
            "eagle@example.com",
            "correct-password",
            Some(&setup.secret),
            true,
        )
        .await;
        set_user_recovery_codes(&state, "user-totp", &setup.recovery_codes).await;

        let response = totp_disable(
            State(state.clone()),
            test_auth_user("user-totp", "eagle@example.com"),
            Json(TotpVerifyRequest {
                code: setup.recovery_codes[0].replace('-', ""),
            }),
        )
        .await
        .expect("disable with recovery code")
        .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let user: (i32, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT totp_enabled, totp_secret, recovery_codes FROM users WHERE id = ?1",
        )
        .bind("user-totp")
        .fetch_one(&state.db)
        .await
        .expect("load user TOTP state");
        assert_eq!(user.0, 0);
        assert!(user.1.is_none());
        assert!(user.2.is_none());
    }

    #[tokio::test]
    async fn totp_disable_is_blocked_when_two_factor_is_required_by_server() {
        let state = logout_test_state().await;
        let setup = totp::generate_totp_setup("eagle@example.com").expect("totp setup");
        insert_login_test_user(
            &state,
            "user-totp",
            "eagle@example.com",
            "correct-password",
            Some(&setup.secret),
            true,
        )
        .await;
        set_user_recovery_codes(&state, "user-totp", &setup.recovery_codes).await;
        sqlx::query(
            "INSERT INTO system_config (key, value, updated_at) VALUES ('require_2fa', 'true', ?1)",
        )
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.db)
        .await
        .expect("enable required 2FA");

        let result = totp_disable(
            State(state.clone()),
            test_auth_user("user-totp", "eagle@example.com"),
            Json(TotpVerifyRequest {
                code: setup.recovery_codes[0].replace('-', ""),
            }),
        )
        .await;

        match result {
            Ok(_) => panic!("required 2FA should block disable"),
            Err(AppError::Forbidden(message)) => assert_eq!(
                message,
                "Two-factor authentication is required by this server."
            ),
            Err(other) => panic!("expected required 2FA forbidden error, got {other:?}"),
        }

        let user: (i32, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT totp_enabled, totp_secret, recovery_codes FROM users WHERE id = ?1",
        )
        .bind("user-totp")
        .fetch_one(&state.db)
        .await
        .expect("load user TOTP state");
        assert_eq!(user.0, 1);
        assert!(user.1.is_some());
        assert!(user.2.is_some());
    }

    #[tokio::test]
    async fn password_reset_confirm_revokes_user_sessions_and_consumes_reset_tokens() {
        let state = logout_test_state().await;
        let token = "reset-token-value-with-enough-length-123";
        insert_password_reset_user(&state, "user-1", token).await;
        insert_password_reset_user(
            &state,
            "user-2",
            "other-reset-token-value-with-enough-length-123",
        )
        .await;
        insert_logout_test_session(&state, "session-user-1-a").await;
        insert_logout_test_session(&state, "session-user-1-b").await;
        sqlx::query(
            "INSERT INTO sessions (id, user_id, refresh_token_hash, expires_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind("session-user-2")
        .bind("user-2")
        .bind("session-user-2-hash")
        .bind((chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339())
        .execute(&state.db)
        .await
        .expect("insert other user session");

        let status = confirm_password_reset(
            State(state.clone()),
            Json(PasswordResetConfirmRequest {
                token: token.to_string(),
                password: "new-password".to_string(),
            }),
        )
        .await
        .expect("confirm reset")
        .into_response()
        .status();

        assert_eq!(status, StatusCode::NO_CONTENT);

        let user_session_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id = ?1")
                .bind("user-1")
                .fetch_one(&state.db)
                .await
                .expect("count reset user sessions");
        let other_session_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id = ?1")
                .bind("user-2")
                .fetch_one(&state.db)
                .await
                .expect("count other user sessions");
        let active_reset_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM password_reset_tokens WHERE user_id = ?1 AND used_at IS NULL",
        )
        .bind("user-1")
        .fetch_one(&state.db)
        .await
        .expect("count active reset tokens");

        assert_eq!(user_session_count.0, 0);
        assert_eq!(other_session_count.0, 1);
        assert_eq!(active_reset_count.0, 0);
    }

    #[tokio::test]
    async fn admin_password_reset_revokes_target_user_sessions() {
        let state = logout_test_state().await;
        insert_password_reset_user(
            &state,
            "user-1",
            "target-reset-token-value-with-enough-length-123",
        )
        .await;
        insert_password_reset_user(
            &state,
            "user-2",
            "other-reset-token-value-with-enough-length-123",
        )
        .await;
        insert_test_session_for_user(&state, "user-1", "target-session-a").await;
        insert_test_session_for_user(&state, "user-1", "target-session-b").await;
        insert_test_session_for_user(&state, "user-2", "other-session").await;
        let now = chrono::Utc::now();
        let admin = AdminUser {
            claims: jwt::Claims {
                sub: "admin-1".to_string(),
                sid: "admin-session".to_string(),
                email: "admin@example.com".to_string(),
                role: "admin".to_string(),
                exp: (now + chrono::Duration::minutes(15)).timestamp(),
                iat: now.timestamp(),
                token_type: jwt::TokenType::Access,
            },
        };

        let response =
            reset_admin_user_password(State(state.clone()), admin, Path("user-1".to_string()))
                .await
                .expect("admin reset")
                .into_response();

        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("admin reset body");
        let body: serde_json::Value =
            serde_json::from_slice(&body_bytes).expect("admin reset JSON");
        assert_eq!(body["email"], "user-1@example.com");
        assert!(body["temporary_password"]
            .as_str()
            .is_some_and(|password| password.len() == 20));

        let target_session_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id = ?1")
                .bind("user-1")
                .fetch_one(&state.db)
                .await
                .expect("count target sessions");
        let other_session_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id = ?1")
                .bind("user-2")
                .fetch_one(&state.db)
                .await
                .expect("count other sessions");

        assert_eq!(target_session_count.0, 0);
        assert_eq!(other_session_count.0, 1);
    }

    #[test]
    fn password_reset_confirm_validation_bounds_token_and_password() {
        let valid = PasswordResetConfirmRequest {
            token: format!("  {}  ", "a".repeat(32)),
            password: "new-password".to_string(),
        };
        assert_eq!(
            validate_password_reset_confirm_input(&valid).expect("valid reset input"),
            "a".repeat(32)
        );

        let short_token = PasswordResetConfirmRequest {
            token: "too-short".to_string(),
            password: "new-password".to_string(),
        };
        match validate_password_reset_confirm_input(&short_token) {
            Err(AppError::BadRequest(message)) => {
                assert_eq!(message, "Password reset token is invalid or expired");
            }
            other => panic!("expected short token rejection, got {other:?}"),
        }

        let short_password = PasswordResetConfirmRequest {
            token: "b".repeat(32),
            password: "short".to_string(),
        };
        match validate_password_reset_confirm_input(&short_password) {
            Err(AppError::BadRequest(message)) => {
                assert_eq!(message, "Password must be at least 8 characters");
            }
            other => panic!("expected short password rejection, got {other:?}"),
        }

        let long_password = PasswordResetConfirmRequest {
            token: "c".repeat(32),
            password: "p".repeat(129),
        };
        match validate_password_reset_confirm_input(&long_password) {
            Err(AppError::BadRequest(message)) => {
                assert_eq!(message, "Password must be at most 128 characters");
            }
            other => panic!("expected long password rejection, got {other:?}"),
        }
    }

    #[test]
    fn oauth_auto_provisioning_gate_rejects_unknown_users_when_disabled() {
        assert!(require_oauth_auto_provisioning("oidc", true).is_ok());
        match require_oauth_auto_provisioning("oidc", false) {
            Err(AppError::Unauthorized(message)) => {
                assert_eq!(
                    message,
                    "OIDC auto provisioning is disabled; ask an administrator to create or link your account"
                );
            }
            other => panic!("expected an auto-provisioning rejection, got {other:?}"),
        }
    }

    #[test]
    fn usernames_are_bounded_and_path_safe() {
        assert!(is_valid_username("eagle_117"));
        assert!(is_valid_username("blue-team"));
        assert!(!is_valid_username(""));
        assert!(!is_valid_username("not allowed"));
        assert!(!is_valid_username(&"a".repeat(65)));
    }
}
