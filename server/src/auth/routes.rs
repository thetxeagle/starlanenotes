use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine as _,
};
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
const OAUTH_STATE_TTL: std::time::Duration = std::time::Duration::from_secs(600);
const MAX_AVATAR_URL_LENGTH: usize = 500;
const MAX_AVATAR_DATA_URL_LENGTH: usize = 1_048_576;
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
        cache.retain(|_, v| v.created_at.elapsed() < OAUTH_STATE_TTL);
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
        cache.retain(|_, v| v.created_at.elapsed() < OAUTH_STATE_TTL);
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
        cache
            .remove(csrf_token)
            .filter(|entry| entry.created_at.elapsed() < OAUTH_STATE_TTL)
            .and_then(|entry| entry.pkce_verifier)
    }

    async fn take_oidc(&self, csrf_token: &str) -> Option<openidconnect::Nonce> {
        let mut cache = self.inner.write().await;
        cache
            .remove(csrf_token)
            .filter(|entry| entry.created_at.elapsed() < OAUTH_STATE_TTL)
            .and_then(|entry| entry.nonce)
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
        .route("/auth/sign-in-options", post(sign_in_options))
        .route("/auth/refresh", post(refresh_token))
        .route("/auth/password-reset/request", post(request_password_reset))
        .route("/auth/password-reset/confirm", post(confirm_password_reset))
        .route("/auth/passkeys/login/start", post(passkey_login_start))
        .route("/auth/passkeys/login/finish", post(passkey_login_finish))
        .route("/auth/native-handoff/start", post(native_handoff_start))
        .route(
            "/auth/native-handoff/exchange",
            post(native_handoff_exchange),
        )
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
        .route(
            "/auth/native-handoff/complete",
            post(native_handoff_complete),
        )
        .route("/auth/me", get(me).put(update_me))
        .route("/auth/sessions", get(list_sessions))
        .route(
            "/auth/sessions/{session_id}",
            axum::routing::put(rename_session).delete(revoke_session),
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
    #[serde(flatten)]
    pub session: SessionMetadata,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub totp_code: Option<String>,
    #[serde(flatten)]
    pub session: SessionMetadata,
}

#[derive(Deserialize)]
pub struct SignInOptionsRequest {
    pub email: String,
}

#[derive(Serialize)]
pub struct SignInOptionsResponse {
    pub password_available: bool,
    pub passkey_available: bool,
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
    pub account_kind: String,
    pub totp_enabled: bool,
}

async fn response_account_kind(state: &AppState, user: &str) -> Result<String, AppError> {
    let kind: String = sqlx::query_scalar("SELECT account_kind FROM users WHERE id=$1")
        .bind(user).fetch_one(&state.db).await.map_err(AppError::internal)?;
    if !matches!(kind.as_str(), "member" | "guest") { return Err(AppError::Unauthorized("Account unavailable".into())); }
    Ok(kind)
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
    #[serde(flatten)]
    pub session: SessionMetadata,
}

#[derive(Deserialize)]
pub struct NativeHandoffStartRequest {
    pub code_challenge: String,
}
#[derive(Serialize)]
pub struct NativeHandoffStartResponse {
    pub handoff_id: String,
    pub authorization_url: String,
}
#[derive(Deserialize)]
pub struct NativeHandoffCompleteRequest {
    pub handoff_id: String,
}
#[derive(Serialize)]
pub struct NativeHandoffCompleteResponse {
    pub handoff_id: String,
    pub status: &'static str,
}
#[derive(Deserialize)]
pub struct NativeHandoffExchangeRequest {
    pub handoff_id: String,
    pub code_verifier: String,
    #[serde(flatten)]
    pub session: SessionMetadata,
}
#[derive(Serialize)]
pub struct NativeHandoffPendingResponse {
    pub handoff_id: String,
    pub status: &'static str,
}

#[derive(Clone, Deserialize)]
pub struct SessionMetadata {
    pub session_kind: Option<String>,
    pub device_name: Option<String>,
    pub client_platform: Option<String>,
}

impl Default for SessionMetadata {
    fn default() -> Self {
        Self {
            session_kind: None,
            device_name: None,
            client_platform: None,
        }
    }
}

#[derive(Deserialize)]
pub struct SessionUpdateRequest {
    pub device_name: String,
}

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
    pub device_name: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub session_kind: String,
    pub client_platform: String,
    pub is_current: bool,
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

const BROWSER_SESSION_INACTIVITY_DAYS: i64 = 30;
const APP_SESSION_INACTIVITY_DAYS: i64 = 180;

async fn persist_session(
    state: &AppState,
    user_id: &str,
    tokens: &jwt::TokenPair,
    metadata: &SessionMetadata,
) -> Result<(), AppError> {
    let session_kind = normalized_session_kind(metadata.session_kind.as_deref());
    let client_platform =
        normalized_client_platform(metadata.client_platform.as_deref(), &session_kind);
    let expires_at = session_expiry_at(&session_kind);
    let requested_name = normalized_device_name(metadata.device_name.as_deref())?
        .unwrap_or_else(|| default_session_name(&client_platform).to_string());
    let device_name = unique_session_name(state, user_id, &requested_name).await?;
    sqlx::query(
        "INSERT INTO sessions (id, user_id, refresh_token_hash, device_name, session_kind, client_platform, expires_at) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&tokens.session_id)
    .bind(user_id)
    .bind(hash_refresh_token(&tokens.refresh_token))
    .bind(device_name)
    .bind(session_kind)
    .bind(client_platform)
    .bind(expires_at)
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;
    Ok(())
}

pub(crate) fn normalized_client_platform(value: Option<&str>, session_kind: &str) -> String {
    match value.map(str::trim) {
        Some("mobile_app") if session_kind == "app" => "mobile_app".to_string(),
        Some("desktop_app") if session_kind == "app" => "desktop_app".to_string(),
        _ if session_kind == "app" => "desktop_app".to_string(),
        _ => "web".to_string(),
    }
}

fn default_session_name(client_platform: &str) -> &'static str {
    match client_platform {
        "mobile_app" => "Starlane Notes mobile app",
        "desktop_app" => "Starlane Notes desktop app",
        _ => "Web browser",
    }
}

async fn unique_session_name(
    state: &AppState,
    user_id: &str,
    requested_name: &str,
) -> Result<String, AppError> {
    let existing: Vec<(Option<String>,)> =
        sqlx::query_as("SELECT device_name FROM sessions WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&state.db)
            .await
            .map_err(AppError::internal)?;
    unique_session_name_from_rows(&existing, requested_name)
}

fn unique_session_name_from_rows(
    existing: &[(Option<String>,)],
    requested_name: &str,
) -> Result<String, AppError> {
    let existing = existing
        .iter()
        .filter_map(|(name,)| name.as_ref())
        .map(|name| name.to_lowercase())
        .collect::<std::collections::HashSet<_>>();
    if !existing.contains(&requested_name.to_lowercase()) {
        return Ok(requested_name.to_string());
    }

    for suffix in 2..=999 {
        let suffix = format!(" ({suffix})");
        let available = 80usize.saturating_sub(suffix.chars().count());
        let base = requested_name.chars().take(available).collect::<String>();
        let candidate = format!("{base}{suffix}");
        if !existing.contains(&candidate.to_lowercase()) {
            return Ok(candidate);
        }
    }
    Err(AppError::Conflict(
        "Unable to assign a unique session name".into(),
    ))
}

pub(crate) fn normalized_session_kind(value: Option<&str>) -> String {
    match value.map(str::trim) {
        Some("app") => "app".to_string(),
        _ => "browser".to_string(),
    }
}

pub(crate) fn session_inactivity_days(session_kind: &str) -> i64 {
    if session_kind == "app" {
        APP_SESSION_INACTIVITY_DAYS
    } else {
        BROWSER_SESSION_INACTIVITY_DAYS
    }
}

pub(crate) fn session_expiry_at(session_kind: &str) -> String {
    (chrono::Utc::now() + chrono::Duration::days(session_inactivity_days(session_kind)))
        .to_rfc3339()
}

fn normalized_device_name(value: Option<&str>) -> Result<Option<String>, AppError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.chars().count() > 80 {
        return Err(AppError::BadRequest(
            "Session name must be at most 80 characters".into(),
        ));
    }
    Ok(Some(value.to_string()))
}

fn create_session_tokens(
    user_id: &str,
    email: &str,
    role: &str,
    secret: &str,
    metadata: &SessionMetadata,
) -> Result<jwt::TokenPair, AppError> {
    let session_kind = normalized_session_kind(metadata.session_kind.as_deref());
    jwt::create_token_pair_for_session(
        user_id,
        email,
        role,
        secret,
        None,
        session_inactivity_days(&session_kind),
    )
    .map_err(|error| AppError::Internal(error.to_string()))
}

async fn remove_expired_sessions(state: &AppState) -> Result<(), AppError> {
    sqlx::query("DELETE FROM sessions WHERE expires_at <= $1")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    Ok(())
}

#[cfg(test)]
tokio::task_local! {pub(crate) static TOTP_RECOVERY_PROGRESS: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;}

fn consumed_totp_recovery_codes(user:&AccountLifecycleUser,code:&str)->Result<Option<String>,AppError>{
    let Some(raw)=user.recovery_codes.as_deref() else{return Ok(None);};
    let mut hashes:Vec<String>=serde_json::from_str(raw).map_err(AppError::internal)?;
    let submitted=hash_totp_recovery_code(code);
    let Some(i)=hashes.iter().position(|h|h==&submitted) else{return Ok(None);};
    hashes.remove(i);
    Ok(Some(serde_json::to_string(&hashes).map_err(AppError::internal)?))
}
async fn finish_totp_password_login(
    state:&AppState,id:&str,email:&str,display_name:&str,avatar_url:&Option<String>,role:&str,
    password_hash:&str,code:&str,metadata:&SessionMetadata,
)->Result<AuthResponse,AppError>{
    let invalid=||AppError::Unauthorized("Invalid credentials".into());
    let source={
        let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
        account_lifecycle_user(&mut conn,id).await?.ok_or_else(invalid)?
    };
    if source.is_active!=1 || source.totp_enabled==0 || source.email!=email
        || source.display_name!=display_name || &source.avatar_url!=avatar_url || source.role!=role
        || source.password_hash.as_deref()!=Some(password_hash)
        || !matches!(source.account_kind.as_str(),"member"|"guest"){return Err(invalid());}
    let mut tx=state.db.begin().await.map_err(AppError::internal)?;
    #[cfg(test)]
    if let Ok(Some(hook))=TOTP_RECOVERY_PROGRESS.try_with(|s|s.borrow_mut().take()){hook.await;}
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    if sqlx::query("UPDATE users SET id=id WHERE id=$1 AND is_active=1 AND password_hash=$2 AND totp_enabled!=0")
        .bind(id).bind(password_hash).execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1
        || account_lifecycle_user(&mut tx,id).await?!=Some(source.clone()){return Err(invalid());}
    let secret=source.totp_secret.as_deref().ok_or_else(||AppError::BadRequest("TOTP not enabled".into()))?;
    let valid=totp::verify_totp_code(secret,code).map_err(AppError::internal)?;
    let mut expected=source.clone();
    if !valid{
        let next=consumed_totp_recovery_codes(&source,code)?.ok_or_else(||AppError::Unauthorized("Invalid 2FA code".into()))?;
        if sqlx::query("UPDATE users SET recovery_codes=$1 WHERE id=$2 AND recovery_codes=$3")
            .bind(&next).bind(id).bind(&source.recovery_codes).execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1{
            return Err(AppError::Unauthorized("Invalid 2FA code".into()));
        }
        expected.recovery_codes=Some(next);
    }
    let kind=normalized_session_kind(metadata.session_kind.as_deref());
    let platform=normalized_client_platform(metadata.client_platform.as_deref(),&kind);
    let requested=normalized_device_name(metadata.device_name.as_deref())?.unwrap_or_else(||default_session_name(&platform).into());
    let mut sessions=initial_session_row(&mut tx,id).await?;
    let names=sessions.iter().map(|r|(r.3.clone(),)).collect::<Vec<_>>();
    let name=unique_session_name_from_rows(&names,&requested)?;
    let tokens=create_session_tokens(id,&source.email,&source.role,&state.jwt_secret,metadata)?;
    let now=chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let session:InitialSessionRow=(tokens.session_id.clone(),id.into(),hash_refresh_token(&tokens.refresh_token),Some(name),None,None,now.clone(),session_expiry_at(&kind),now,kind,platform);
    if sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,device_name,ip_address,user_agent,last_seen_at,expires_at,created_at,session_kind,client_platform) VALUES($1,$2,$3,$4,NULL,NULL,$5,$6,$7,$8,$9)")
        .bind(&session.0).bind(&session.1).bind(&session.2).bind(&session.3).bind(&session.6).bind(&session.7).bind(&session.8).bind(&session.9).bind(&session.10)
        .execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(AppError::Conflict("TOTP session changed".into()));}
    sessions.push(session);sessions.sort_by(|a,b|a.0.cmp(&b.0));
    if account_lifecycle_user(&mut tx,id).await?!=Some(expected)
        || initial_session_row(&mut tx,id).await?!=sessions
        || (valid && !totp::verify_totp_code(secret,code).map_err(AppError::internal)?){
        return Err(AppError::Conflict("TOTP login source changed".into()));
    }
    tx.commit().await.map_err(AppError::internal)?;
    Ok(AuthResponse{
        access_token:tokens.access_token,refresh_token:tokens.refresh_token,expires_in:tokens.expires_in,
        user:UserResponse{account_kind:source.account_kind,id:id.into(),email:source.email,username:None,display_name:source.display_name,avatar_url:source.avatar_url,role:source.role,totp_enabled:true},
    })
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
         VALUES ($1, $2, $3, $4, $5, $6)",
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

#[cfg(test)]
tokio::task_local! {
 pub(crate) static PASSKEY_TAKE_PROGRESS: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
 pub(crate) static PASSKEY_TAKE_COUNT: std::sync::Arc<std::sync::atomic::AtomicUsize>;
}
async fn take_passkey_challenge<T: for<'de> Deserialize<'de>>(
 state:&AppState,challenge_id:&str,expected_kind:&str,expected_owner:Option<&str>,
)->Result<(Option<String>,Option<String>,T),AppError>{
 let row:Option<(Option<String>,Option<String>,String,String,String)>=sqlx::query_as(
  "SELECT user_id,email,kind,state_json,expires_at FROM passkey_challenges WHERE id=$1")
  .bind(challenge_id).fetch_optional(&state.db).await.map_err(AppError::internal)?;
 let(user_id,email,kind,state_json,expires_at)=row.ok_or_else(||AppError::BadRequest("Passkey challenge is invalid or expired".into()))?;
 // A challenge belonging to another account must survive a refused registration request.
 if let(Some(owner),Some(actual))=(expected_owner,user_id.as_deref()){
  if owner!=actual{return Err(AppError::Unauthorized("Passkey registration challenge does not belong to this account".into()));}
 }
 #[cfg(test)] {
  let progress=PASSKEY_TAKE_PROGRESS.try_with(|s|s.borrow_mut().take()).ok().flatten();
  if let Some(progress)=progress{progress.await;}
 }
 let now=chrono::Utc::now().to_rfc3339();
 let expired=expires_at<=now;
 let statement=if expired{
  "DELETE FROM passkey_challenges WHERE id=$1 AND kind=$2 AND state_json=$3 AND expires_at=$4 AND (user_id=$5 OR (user_id IS NULL AND $5 IS NULL)) AND (email=$6 OR (email IS NULL AND $6 IS NULL)) AND expires_at<=$7"
 }else{
  "DELETE FROM passkey_challenges WHERE id=$1 AND kind=$2 AND state_json=$3 AND expires_at=$4 AND (user_id=$5 OR (user_id IS NULL AND $5 IS NULL)) AND (email=$6 OR (email IS NULL AND $6 IS NULL)) AND expires_at>$7"
 };
 let changed=sqlx::query(statement).bind(challenge_id).bind(&kind).bind(&state_json).bind(&expires_at).bind(&user_id).bind(&email).bind(&now).execute(&state.db).await.map_err(AppError::internal)?.rows_affected();
 if expired||changed!=1{return Err(AppError::BadRequest("Passkey challenge is invalid or expired".into()));}
 // Existing wrong-kind retirement is intentional; check after the single guarded consumption.
 if kind!=expected_kind{return Err(AppError::BadRequest("Passkey challenge type does not match this request".into()));}
 let passkey_state=serde_json::from_str(&state_json).map_err(AppError::internal)?;
 #[cfg(test)]let _=PASSKEY_TAKE_COUNT.try_with(|n|n.fetch_add(1,std::sync::atomic::Ordering::SeqCst));
 Ok((user_id,email,passkey_state))
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
    let rows: Vec<(String,)> = sqlx::query_as("SELECT public_key FROM passkeys WHERE user_id = $1")
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
    let _ = sqlx::query("DELETE FROM passkey_challenges WHERE expires_at <= $1")
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.db)
        .await;
}

// ============================================================
// Handlers
// ============================================================

/// Registration-origin evidence names actual freshly verified rows only. It is
/// not an HTTP principal, administrator witness, or reusable permission grant.
pub(crate) struct RegistrationAuditOrigin {
    user_id: String,
    space_id: String,
}
impl RegistrationAuditOrigin {
    pub(crate) fn user_id(&self) -> &str {
        &self.user_id
    }
    pub(crate) fn space_id(&self) -> &str {
        &self.space_id
    }
}
type InitialSessionRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    String,
    String,
);
async fn initial_session_row(
    conn: &mut sqlx::AnyConnection,
    user: &str,
) -> Result<Vec<InitialSessionRow>, AppError> {
    sqlx::query_as("SELECT id,user_id,refresh_token_hash,device_name,ip_address,user_agent,last_seen_at,expires_at,created_at,session_kind,client_platform FROM sessions WHERE user_id=$1 ORDER BY id").bind(user).fetch_all(conn).await.map_err(AppError::internal)
}
async fn registration_config_row(
    conn: &mut sqlx::AnyConnection,
    key: &str,
) -> Result<Option<(String, String, String)>, AppError> {
    sqlx::query_as("SELECT key,value,updated_at FROM system_config WHERE key=$1")
        .bind(key)
        .fetch_optional(conn)
        .await
        .map_err(AppError::internal)
}
fn registration_signups_enabled(stored: &Option<(String, String, String)>) -> bool {
    std::env::var("ALLOW_PUBLIC_SIGNUPS")
        .ok()
        .unwrap_or_else(|| {
            stored
                .as_ref()
                .map(|r| r.1.clone())
                .unwrap_or_else(|| "false".into())
        })
        .trim()
        .eq_ignore_ascii_case("true")
}
#[cfg(test)]
tokio::task_local! {pub(crate) static SIGNUP_BEFORE_FENCE_SIGNAL:std::cell::RefCell<Option<Box<dyn FnOnce()+Send>>>;}

#[cfg(test)]
tokio::task_local! {static REGISTRATION_HASH_HOOK:std::cell::RefCell<Option<Box<dyn FnOnce()+Send>>>;static REGISTRATION_FINAL_HOOK:std::cell::RefCell<Option<Box<dyn FnOnce()+Send>>>; static REGISTRATION_FENCE_HOOK:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;}

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

    // Advisory rejection-only preflight preserves inexpensive legacy denial order.
    // Every allowed request repeats the actual decision under the audience fence.
    let preflight_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db)
        .await
        .map_err(AppError::internal)?;
    if preflight_count > 0 && !public_signups_enabled(&state).await? {
        return Err(AppError::Forbidden(
            "Public signups are disabled for this server. Ask an administrator for an invite."
                .into(),
        ));
    }
    let duplicate: Option<String> = sqlx::query_scalar("SELECT id FROM users WHERE email=$1")
        .bind(&req.email)
        .fetch_optional(&state.db)
        .await
        .map_err(AppError::internal)?;
    if duplicate.is_some() {
        return Err(AppError::Conflict("Email already registered".into()));
    }
    // Invalid session metadata fails before hashing or canonical writes.

    let session_kind = normalized_session_kind(req.session.session_kind.as_deref());
    let client_platform =
        normalized_client_platform(req.session.client_platform.as_deref(), &session_kind);
    let device_name = normalized_device_name(req.session.device_name.as_deref())?
        .unwrap_or_else(|| default_session_name(&client_platform).into());
    #[cfg(test)]
    let _ = REGISTRATION_HASH_HOOK.try_with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
    let password_hash =
        password::hash_password(&req.password).map_err(|e| AppError::Internal(e.to_string()))?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    #[cfg(test)]
    let _=SIGNUP_BEFORE_FENCE_SIGNAL.try_with(|slot|{if let Some(signal)=slot.borrow_mut().take(){signal();}});
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    #[cfg(test)]
    if let Ok(Some(hook)) = REGISTRATION_FENCE_HOOK.try_with(|slot| slot.borrow_mut().take()) {
        hook.await;
    }
    // The first-account decision is serialized with adopted topology writers.
    let mut user_ids: Vec<String> = sqlx::query_scalar("SELECT id FROM users ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    let first = user_ids.is_empty();
    let role = if first { "admin" } else { "member" };
    let signup_source = registration_config_row(&mut tx, "allow_public_signups").await?;
    if !first && !registration_signups_enabled(&signup_source) {
        return Err(AppError::Forbidden(
            "Public signups are disabled for this server. Ask an administrator for an invite."
                .into(),
        ));
    }
    let existing: Option<String> = sqlx::query_scalar("SELECT id FROM users WHERE email=$1")
        .bind(&req.email)
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if existing.is_some() {
        return Err(AppError::Conflict("Email already registered".into()));
    }
    let mut setup_source = registration_config_row(&mut tx, "setup_completed").await?;
    let user_id = Uuid::new_v4().to_string();
    let tokens =
        create_session_tokens(&user_id, &req.email, role, &state.jwt_secret, &req.session)?;
    let prepared = insert_account_with_personal_space(
        &mut tx,
        NewAccountSeed {
            id: user_id.clone(),
            email: req.email.clone(),
            display_name: req.display_name.clone(),
            password_hash: Some(password_hash),
            avatar_url: None,
            role: role.into(),
            is_active: true,
            external_id: None,
        },
    )
    .await?;
    verify_prepared_account(&mut tx, &prepared).await?;
    if first {
        let changed =
            sqlx::query("UPDATE system_config SET value='true' WHERE key='setup_completed'")
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?
                .rows_affected();
        if changed != u64::from(setup_source.is_some()) {
            return Err(AppError::Conflict("Setup state changed".into()));
        }
        if let Some(row) = &mut setup_source {
            row.1 = "true".into();
        }
    }
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let session: InitialSessionRow = (
        tokens.session_id.clone(),
        user_id.clone(),
        hash_refresh_token(&tokens.refresh_token),
        Some(device_name),
        None,
        None,
        now.clone(),
        session_expiry_at(&session_kind),
        now,
        session_kind,
        client_platform,
    );
    if sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,device_name,ip_address,user_agent,last_seen_at,expires_at,created_at,session_kind,client_platform) VALUES($1,$2,$3,$4,NULL,NULL,$5,$6,$7,$8,$9)").bind(&session.0).bind(&session.1).bind(&session.2).bind(&session.3).bind(&session.6).bind(&session.7).bind(&session.8).bind(&session.9).bind(&session.10).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(AppError::Conflict("Initial session changed".into()));}
    verify_prepared_account_sources(&mut tx, &prepared, 1).await?;
    if initial_session_row(&mut tx, &user_id).await? != vec![session.clone()] {
        return Err(AppError::Conflict("Initial session changed".into()));
    }
    let origin = RegistrationAuditOrigin {
        user_id: user_id.clone(),
        space_id: prepared.personal.id.clone(),
    };
    let event = crate::audit::insert_registration_space_action(&mut tx, &origin).await?;
    verify_prepared_account_sources(&mut tx, &prepared, 1).await?;
    if initial_session_row(&mut tx, &user_id).await? != vec![session] {
        return Err(AppError::Conflict("Initial session changed".into()));
    }
    crate::audit::verify_registration_space_action(&mut tx, &event, &origin).await?;
    user_ids.push(user_id.clone());
    user_ids.sort();
    if sqlx::query_scalar::<_, String>("SELECT id FROM users ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?
        != user_ids
        || registration_config_row(&mut tx, "setup_completed").await? != setup_source
    {
        return Err(AppError::Conflict("Registration source changed".into()));
    }
    let final_signup = registration_config_row(&mut tx, "allow_public_signups").await?;
    if final_signup != signup_source {
        return Err(AppError::Conflict("Signup policy changed".into()));
    }
    #[cfg(test)]
    let _ = REGISTRATION_FINAL_HOOK.try_with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
    if !first && !registration_signups_enabled(&final_signup) {
        return Err(AppError::Forbidden(
            "Public signups are disabled for this server. Ask an administrator for an invite."
                .into(),
        ));
    }
    tx.commit().await.map_err(AppError::internal)?;
    // No filesystem or JSONL error can turn a known committed signup into failure.
    if let Err(error) = storage::init_space_dirs(&prepared.personal.slug).await {
        tracing::warn!(%error,"Registration personal-space directory initialization deferred");
    }
    crate::audit::append_actor_action(&event).await;
    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            access_token: tokens.access_token,
            refresh_token: tokens.refresh_token,
            expires_in: tokens.expires_in,
            user: UserResponse {
                account_kind: "member".into(),
                id: user_id,
                email: req.email,
                username: None,
                display_name: req.display_name,
                avatar_url: None,
                role: role.into(),
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
            "SELECT id, email, display_name, avatar_url, role, password_hash, totp_enabled FROM users WHERE email = $1 AND is_active = 1",
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

        return Ok(Json(finish_totp_password_login(&state,&user_id,&email,&display_name,&avatar_url,&role,&hash,code,&req.session).await?));
    }

    // Generate tokens
    let tokens = create_session_tokens(&user_id, &email, &role, &state.jwt_secret, &req.session)?;
    persist_session(&state, &user_id, &tokens, &req.session).await?;

    Ok(Json(AuthResponse {
        access_token: tokens.access_token,
        refresh_token: tokens.refresh_token,
        expires_in: tokens.expires_in,
        user: UserResponse {
            account_kind: response_account_kind(&state, &user_id).await?,
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

/// POST /api/v1/auth/sign-in-options
///
/// Returns the compatible credential methods for the supplied active account.
/// This intentionally lives in the same rate-limited auth router as sign-in.
async fn sign_in_options(
    State(state): State<AppState>,
    Json(req): Json<SignInOptionsRequest>,
) -> Result<Json<SignInOptionsResponse>, AppError> {
    let email = req.email.trim().to_lowercase();
    if email.is_empty() || email.len() > 254 {
        return Ok(Json(SignInOptionsResponse {
            password_available: false,
            passkey_available: false,
        }));
    }

    let account: Option<(Option<String>, i64)> = sqlx::query_as(
        "SELECT users.password_hash, COUNT(passkeys.id) \
         FROM users LEFT JOIN passkeys ON passkeys.user_id = users.id \
         WHERE lower(users.email) = $1 AND users.is_active = 1 \
         GROUP BY users.id, users.password_hash",
    )
    .bind(email)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    let (password_hash, passkey_count) = account.unwrap_or((None, 0));
    Ok(Json(SignInOptionsResponse {
        password_available: password_hash.is_some(),
        passkey_available: passkey_count > 0,
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
        "SELECT session_kind FROM sessions WHERE id = $1 AND user_id = $2 AND refresh_token_hash = $3 AND expires_at > $4",
    )
    .bind(&claims.sid)
    .bind(&claims.sub)
    .bind(hash_refresh_token(&req.refresh_token))
    .bind(chrono::Utc::now().to_rfc3339())
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;
    let Some((session_kind,)) = existing_session else {
        return Err(AppError::Unauthorized(
            "Refresh session is no longer active".into(),
        ));
    };

    // Verify user still exists and is active
    let user: Option<(String, String, String)> =
        sqlx::query_as("SELECT id, email, role FROM users WHERE id = $1 AND is_active = 1")
            .bind(&claims.sub)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    let (user_id, email, role) =
        user.ok_or_else(|| AppError::Unauthorized("User no longer active".into()))?;

    let tokens = jwt::create_token_pair_for_session(
        &user_id,
        &email,
        &role,
        &state.jwt_secret,
        Some(&claims.sid),
        session_inactivity_days(&session_kind),
    )
    .map_err(|error| AppError::Internal(error.to_string()))?;
    sqlx::query("UPDATE sessions SET refresh_token_hash = $1, last_seen_at = $2, expires_at = $3 WHERE id = $4")
        .bind(hash_refresh_token(&tokens.refresh_token))
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(session_expiry_at(&session_kind))
        .bind(&claims.sid)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

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
            sqlx::query_as("SELECT id, email FROM users WHERE lower(email) = $1 AND is_active = 1")
                .bind(&email)
                .fetch_optional(&state.db)
                .await
                .map_err(AppError::internal)?;

        if let Some((user_id, recipient_email)) = user {
            let token = generate_password_reset_token();
            let token_hash = hash_password_reset_token(&token);
            let reset_id = Uuid::new_v4().to_string();

            let mut tx = state.db.begin().await.map_err(AppError::internal)?;
            crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
            sqlx::query("UPDATE users SET id=id WHERE id=$1")
                .bind(&user_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
            let current: Option<(String, String)> = sqlx::query_as(
                "SELECT id,email FROM users WHERE id=$1 AND lower(email)=$2 AND is_active=1",
            )
            .bind(&user_id)
            .bind(&email)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::internal)?;
            if current != Some((user_id.clone(), recipient_email.clone())) {
                return Ok(Json(PasswordResetRequestResponse {
                    message: generic_message.to_string(),
                }));
            }
            let now = chrono::Utc::now().to_rfc3339();
            let expires_at = (chrono::Utc::now() + chrono::Duration::minutes(30))
                .format("%Y-%m-%dT%H:%M:%SZ")
                .to_string();
            sqlx::query(
                "UPDATE password_reset_tokens SET used_at = $1 WHERE user_id = $2 AND used_at IS NULL",
            )
            .bind(&now)
            .bind(&user_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;

            let issuance = sqlx::query(
                "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at) \
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(&reset_id)
            .bind(&user_id)
            .bind(&token_hash)
            .bind(&expires_at)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;

            if issuance.rows_affected() != 1 {
                return Err(AppError::Conflict("Password reset issuance changed".into()));
            }
            let current:Option<(String,String,String,Option<String>)>=sqlx::query_as("SELECT user_id,token_hash,expires_at,used_at FROM password_reset_tokens WHERE id=$1").bind(&reset_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
            if current != Some((user_id.clone(), token_hash, expires_at, None)) {
                return Err(AppError::Conflict("Password reset issuance changed".into()));
            }
            tx.commit().await.map_err(AppError::internal)?;

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

#[cfg(test)]
tokio::task_local! {pub(crate) static RESET_CONFIRM_FENCE_SIGNAL:std::cell::RefCell<Option<Box<dyn FnOnce()+Send>>>;}

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
         WHERE prt.token_hash = $1 \
           AND prt.used_at IS NULL \
           AND prt.expires_at > $2 \
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
    let invalid = || AppError::BadRequest("Password reset token is invalid or expired".into());
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    #[cfg(test)]
    let _ = RESET_CONFIRM_FENCE_SIGNAL.try_with(|slot| {
        if let Some(signal) = slot.borrow_mut().take() {
            signal();
        }
    });
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    if sqlx::query("UPDATE users SET id=id WHERE id=$1")
        .bind(&user_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?
        .rows_affected()
        != 1
    {
        return Err(invalid());
    }
    if sqlx::query("UPDATE password_reset_tokens SET id=id WHERE id=$1 AND user_id=$2")
        .bind(&token_id)
        .bind(&user_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?
        .rows_affected()
        != 1
    {
        return Err(invalid());
    }
    let now = chrono::Utc::now().to_rfc3339();
    let current:Option<(Option<String>,)>=sqlx::query_as("SELECT u.password_hash FROM password_reset_tokens prt JOIN users u ON u.id=prt.user_id WHERE prt.id=$1 AND prt.user_id=$2 AND prt.token_hash=$3 AND prt.used_at IS NULL AND prt.expires_at>$4 AND u.is_active=1").bind(&token_id).bind(&user_id).bind(&token_hash).bind(&now).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    let (old_password,) = current.ok_or_else(invalid)?;
    if sqlx::query("UPDATE password_reset_tokens SET used_at=$1 WHERE id=$2 AND user_id=$3 AND token_hash=$4 AND used_at IS NULL AND expires_at>$1").bind(&now).bind(&token_id).bind(&user_id).bind(&token_hash).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(invalid());}
    let credential_write=sqlx::query("UPDATE users SET password_hash = $1, updated_at = $2 WHERE id = $3 AND is_active=1 AND (password_hash=$4 OR (password_hash IS NULL AND $4 IS NULL))")
        .bind(&password_hash)
        .bind(&now)
        .bind(&user_id)
        .bind(&old_password)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;

    if credential_write.rows_affected() != 1 {
        return Err(AppError::Conflict(
            "Password reset credential changed".into(),
        ));
    }

    sqlx::query("DELETE FROM sessions WHERE user_id = $1")
        .bind(&user_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;

    sqlx::query(
        "UPDATE password_reset_tokens SET used_at = $1 WHERE user_id = $2 AND used_at IS NULL",
    )
    .bind(&now)
    .bind(&user_id)
    .execute(&mut *tx)
    .await
    .map_err(AppError::internal)?;

    let credential: Option<(String, String)> =
        sqlx::query_as("SELECT password_hash,updated_at FROM users WHERE id=$1 AND is_active=1")
            .bind(&user_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    let consumed: Option<(String, String, Option<String>)> =
        sqlx::query_as("SELECT user_id,token_hash,used_at FROM password_reset_tokens WHERE id=$1")
            .bind(&token_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id=$1")
        .bind(&user_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    let open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM password_reset_tokens WHERE user_id=$1 AND used_at IS NULL",
    )
    .bind(&user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::internal)?;
    if credential != Some((password_hash, now.clone()))
        || consumed != Some((user_id.clone(), token_hash, Some(now)))
        || sessions != 0
        || open != 0
    {
        return Err(AppError::Conflict("Password reset state changed".into()));
    }
    tx.commit().await.map_err(AppError::internal)?;

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
    sqlx::query("DELETE FROM sessions WHERE id = $1 AND user_id = $2")
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

    let config = oauth::ProviderConfig::capture(&provider);
    let user_info =
        oauth::exchange_code_with_config(&config, &state.base_url, &params.code, pkce_verifier)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
    complete_oauth_sign_in(
        &state,
        VerifiedProviderOrigin {
            info: user_info,
            config: ExternalConfig::OAuth(config),
            auto_provision: true,
            role_override: None,
        },
    )
    .await
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
    let source = {
        let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
        OidcSource::load(&mut conn).await?
    };
    let config = source.config();
    if !config.enabled {
        return Err(AppError::BadRequest("OIDC is not enabled".into()));
    }

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

    complete_oauth_sign_in(
        &state,
        VerifiedProviderOrigin {
            info: oauth_info,
            config: ExternalConfig::Oidc(source),
            auto_provision: config.auto_provision,
            role_override,
        },
    )
    .await
}

/// GET /api/v1/auth/passkeys
async fn list_passkeys(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, name, created_at, last_used_at FROM passkeys WHERE user_id = $1 ORDER BY created_at DESC",
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
    let user: (String,) = sqlx::query_as("SELECT display_name FROM users WHERE id = $1")
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
async fn passkey_register_finish(State(state):State<AppState>,auth:AuthUser,Json(req):Json<PasskeyRegisterFinishRequest>)->Result<impl IntoResponse,AppError>{
 commit_passkey_registration(&state,&auth,req).await
}

/// DELETE /api/v1/auth/passkeys/{passkey_id}
async fn delete_passkey(State(state):State<AppState>,auth:AuthUser,Path(id):Path<String>)->Result<StatusCode,AppError>{
 commit_passkey_delete(&state,&auth,&id).await
}

/// POST /api/v1/auth/passkeys/login/start
#[cfg(test)]
tokio::task_local!{pub(crate) static NATIVE_BEFORE_FENCE_SIGNAL:std::cell::RefCell<Option<Box<dyn FnOnce()+Send>>>;}

async fn native_handoff_start(
    State(state): State<AppState>,
    Json(req): Json<NativeHandoffStartRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.code_challenge.len() < 43 || req.code_challenge.len() > 128 {
        return Err(AppError::BadRequest("Native sign-in verifier is invalid.".into()));
    }
    let handoff_id = Uuid::new_v4().to_string();
    let expires_at = (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339();
    sqlx::query("INSERT INTO native_auth_handoffs (id, code_challenge, expires_at) VALUES ($1, $2, $3)")
        .bind(&handoff_id).bind(&req.code_challenge).bind(expires_at)
        .execute(&state.db).await.map_err(AppError::internal)?;
    Ok(Json(NativeHandoffStartResponse {
        authorization_url: format!("{}/app-signin/?handoff_id={handoff_id}",state.base_url.trim_end_matches('/')),
        handoff_id,
    }))
}

#[derive(Clone, PartialEq, Eq, sqlx::FromRow)]
struct NativeHandoffSource {
    id: String,
    code_challenge: String,
    user_id: Option<String>,
    code_hash: Option<String>,
    expires_at: String,
    consumed_at: Option<String>,
}
impl NativeHandoffSource {
    fn available(&self) -> bool {
        self.consumed_at.is_none() && chrono::DateTime::parse_from_rfc3339(&self.expires_at)
            .is_ok_and(|expiry| expiry > chrono::Utc::now())
    }
}
async fn native_handoff_source(conn:&mut sqlx::AnyConnection,id:&str)->Result<Option<NativeHandoffSource>,AppError>{
    sqlx::query_as("SELECT id,code_challenge,user_id,code_hash,expires_at,consumed_at FROM native_auth_handoffs WHERE id=$1")
        .bind(id).fetch_optional(conn).await.map_err(AppError::internal)
}
async fn native_handoff_complete(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<NativeHandoffCompleteRequest>,
) -> Result<impl IntoResponse, AppError> {
    let invalid=||AppError::BadRequest("Native sign-in handoff is expired or invalid.".into());
    let mut tx=state.db.begin().await.map_err(AppError::internal)?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    if sqlx::query("UPDATE users SET id=id WHERE id=$1 AND is_active=1").bind(&auth.claims.sub)
        .execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1 {
        return Err(AppError::Unauthorized("Native sign-in account is unavailable.".into()));
    }
    if auth.api_key_scopes.is_none() && sqlx::query("UPDATE sessions SET id=id WHERE id=$1 AND user_id=$2")
        .bind(&auth.claims.sid).bind(&auth.claims.sub).execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1 {
        return Err(AppError::Unauthorized("Native sign-in session is unavailable.".into()));
    }
    let actor=crate::spaces::access::require_current_human_preference_write(&mut tx,&auth).await?;
    let user=account_lifecycle_user(&mut tx,&actor).await?.ok_or_else(invalid)?;
    let source=native_handoff_source(&mut tx,&req.handoff_id).await?.ok_or_else(invalid)?;
    if !source.available() || source.user_id.as_deref().is_some_and(|u|u!=actor){return Err(invalid());}
    if sqlx::query("UPDATE native_auth_handoffs SET id=id WHERE id=$1").bind(&req.handoff_id)
        .execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1
        || native_handoff_source(&mut tx,&req.handoff_id).await?!=Some(source.clone()) {
        return Err(invalid());
    }
    let now=chrono::Utc::now().to_rfc3339();
    if sqlx::query("UPDATE native_auth_handoffs SET user_id=$1 WHERE id=$2 AND code_challenge=$3 AND (user_id=$4 OR (user_id IS NULL AND $4 IS NULL)) AND (code_hash=$5 OR (code_hash IS NULL AND $5 IS NULL)) AND expires_at=$6 AND expires_at>$7 AND consumed_at IS NULL")
        .bind(&actor).bind(&source.id).bind(&source.code_challenge).bind(&source.user_id).bind(&source.code_hash).bind(&source.expires_at).bind(&now)
        .execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(invalid());}
    let mut expected=source;expected.user_id=Some(actor.clone());
    if native_handoff_source(&mut tx,&req.handoff_id).await?!=Some(expected.clone())
        || !expected.available() || account_lifecycle_user(&mut tx,&actor).await?!=Some(user)
        || crate::spaces::access::require_current_human_preference_write(&mut tx,&auth).await?!=actor {
        return Err(AppError::Conflict("Native sign-in approval changed.".into()));
    }
    tx.commit().await.map_err(AppError::internal)?;
    Ok(Json(NativeHandoffCompleteResponse{handoff_id:req.handoff_id,status:"approved"}))
}
async fn native_handoff_exchange(
    State(state): State<AppState>,
    Json(req): Json<NativeHandoffExchangeRequest>,
) -> Result<Response, AppError> {
    let invalid=||AppError::Unauthorized("Native sign-in handoff is expired or invalid.".into());
    if req.code_verifier.len()<43 || req.code_verifier.len()>128 {
        return Err(AppError::Unauthorized("Native sign-in handoff is invalid.".into()));
    }
    let verifier_challenge=URL_SAFE_NO_PAD.encode(Sha256::digest(req.code_verifier.as_bytes()));
    let mut tx=state.db.begin().await.map_err(AppError::internal)?;
    #[cfg(test)]
    let _=NATIVE_BEFORE_FENCE_SIGNAL.try_with(|slot|{if let Some(f)=slot.borrow_mut().take(){f();}});
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    let source=native_handoff_source(&mut tx,&req.handoff_id).await?.ok_or_else(invalid)?;
    if !source.available(){return Err(invalid());}
    if source.code_challenge!=verifier_challenge {
        return Err(AppError::Unauthorized("Native sign-in handoff is invalid.".into()));
    }
    let Some(user_id)=source.user_id.clone() else {
        tx.commit().await.map_err(AppError::internal)?;
        return Ok((StatusCode::ACCEPTED,Json(NativeHandoffPendingResponse{handoff_id:req.handoff_id,status:"pending"})).into_response());
    };
    let session_kind=normalized_session_kind(req.session.session_kind.as_deref());
    let platform=normalized_client_platform(req.session.client_platform.as_deref(),&session_kind);
    let requested=normalized_device_name(req.session.device_name.as_deref())?.unwrap_or_else(||default_session_name(&platform).into());
    let user=account_lifecycle_user(&mut tx,&user_id).await?.filter(|u|u.is_active==1 && matches!(u.account_kind.as_str(),"member"|"guest")).ok_or_else(invalid)?;
    if sqlx::query("UPDATE users SET id=id WHERE id=$1 AND is_active=1").bind(&user_id)
        .execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1
        || account_lifecycle_user(&mut tx,&user_id).await?!=Some(user.clone()){return Err(invalid());}
    if sqlx::query("UPDATE native_auth_handoffs SET id=id WHERE id=$1").bind(&req.handoff_id)
        .execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1
        || native_handoff_source(&mut tx,&req.handoff_id).await?!=Some(source.clone()){return Err(invalid());}
    let mut sessions=initial_session_row(&mut tx,&user_id).await?;
    let names=sessions.iter().map(|r|(r.3.clone(),)).collect::<Vec<_>>();
    let device_name=unique_session_name_from_rows(&names,&requested)?;
    let tokens=create_session_tokens(&user_id,&user.email,&user.role,&state.jwt_secret,&req.session)?;
    let consumed_at=chrono::Utc::now().to_rfc3339();
    if sqlx::query("UPDATE native_auth_handoffs SET consumed_at=$1 WHERE id=$2 AND user_id=$3 AND code_challenge=$4 AND (code_hash=$5 OR (code_hash IS NULL AND $5 IS NULL)) AND expires_at=$6 AND expires_at>$1 AND consumed_at IS NULL")
        .bind(&consumed_at).bind(&source.id).bind(&user_id).bind(&source.code_challenge).bind(&source.code_hash).bind(&source.expires_at)
        .execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(invalid());}
    let now=chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let session:InitialSessionRow=(tokens.session_id.clone(),user_id.clone(),hash_refresh_token(&tokens.refresh_token),Some(device_name),None,None,now.clone(),session_expiry_at(&session_kind),now,session_kind,platform);
    if sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,device_name,ip_address,user_agent,last_seen_at,expires_at,created_at,session_kind,client_platform) VALUES($1,$2,$3,$4,NULL,NULL,$5,$6,$7,$8,$9)")
        .bind(&session.0).bind(&session.1).bind(&session.2).bind(&session.3).bind(&session.6).bind(&session.7).bind(&session.8).bind(&session.9).bind(&session.10)
        .execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(AppError::Conflict("Native sign-in session changed.".into()));}
    sessions.push(session);sessions.sort_by(|a,b|a.0.cmp(&b.0));
    let mut expected=source;expected.consumed_at=Some(consumed_at);
    if native_handoff_source(&mut tx,&req.handoff_id).await?!=Some(expected.clone())
        || account_lifecycle_user(&mut tx,&user_id).await?!=Some(user.clone())
        || initial_session_row(&mut tx,&user_id).await?!=sessions
        || !chrono::DateTime::parse_from_rfc3339(&expected.expires_at).is_ok_and(|expiry|expiry>chrono::Utc::now()){
        return Err(AppError::Conflict("Native sign-in source changed.".into()));
    }
    tx.commit().await.map_err(AppError::internal)?;
    Ok(Json(AuthResponse{
        access_token:tokens.access_token,refresh_token:tokens.refresh_token,expires_in:tokens.expires_in,
        user:UserResponse{account_kind:user.account_kind,id:user_id,email:user.email,username:user.username,display_name:user.display_name,avatar_url:user.avatar_url,role:user.role,totp_enabled:user.totp_enabled!=0},
    }).into_response())
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
        sqlx::query_as("SELECT id, email FROM users WHERE lower(email) = $1 AND is_active = 1")
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
async fn passkey_login_finish(State(state):State<AppState>,Json(req):Json<PasskeyLoginFinishRequest>)->Result<impl IntoResponse,AppError>{
 Ok(Json(commit_passkey_login(&state,req).await?))
}

/// POST /api/v1/auth/2fa/totp/setup
async fn totp_setup(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let existing: (i32,) = sqlx::query_as("SELECT totp_enabled FROM users WHERE id = $1")
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
    sqlx::query("UPDATE users SET totp_secret = $1, recovery_codes = $2 WHERE id = $3")
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
        sqlx::query_as("SELECT totp_secret FROM users WHERE id = $1")
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

    sqlx::query("UPDATE users SET totp_enabled = 1 WHERE id = $1")
        .bind(&auth.claims.sub)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(Json(json!({ "totp_enabled": true })))
}

/// POST /api/v1/auth/2fa/totp/disable

async fn totp_disable(
    State(state):State<AppState>,auth:AuthUser,Json(req):Json<TotpVerifyRequest>,
)->Result<impl IntoResponse,AppError>{
    let mut tx=state.db.begin().await.map_err(AppError::internal)?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    crate::spaces::access::require_current_human_preference_write(&mut tx,&auth).await?;
    let env=std::env::var("REQUIRE_2FA").ok();
    let config=registration_config_row(&mut tx,"require_2fa").await?;
    let value=env.clone().unwrap_or_else(||config.as_ref().map(|r|r.1.clone()).unwrap_or_else(||"false".into()));
    if value.eq_ignore_ascii_case("true"){return Err(AppError::Forbidden("Two-factor authentication is required by this server.".into()));}
    let source=account_lifecycle_user(&mut tx,&auth.claims.sub).await?.ok_or_else(||AppError::Unauthorized("Current account unavailable".into()))?;
    if sqlx::query("UPDATE users SET id=id WHERE id=$1").bind(&source.id).execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1
        || account_lifecycle_user(&mut tx,&source.id).await?!=Some(source.clone()){return Err(AppError::Conflict("TOTP source changed".into()));}
    if source.totp_enabled==0{return Err(AppError::BadRequest("TOTP not enabled".into()));}
    let secret=source.totp_secret.as_deref().ok_or_else(||AppError::BadRequest("TOTP not enabled".into()))?;
    let valid=totp::verify_totp_code(secret,&req.code).map_err(AppError::internal)?;
    if !valid && consumed_totp_recovery_codes(&source,&req.code)?.is_none(){return Err(AppError::Unauthorized("Invalid TOTP code".into()));}
    if sqlx::query("UPDATE users SET totp_enabled=0,totp_secret=NULL,recovery_codes=NULL WHERE id=$1 AND totp_enabled=$2 AND totp_secret=$3 AND (recovery_codes=$4 OR (recovery_codes IS NULL AND $4 IS NULL))")
        .bind(&source.id).bind(source.totp_enabled).bind(&source.totp_secret).bind(&source.recovery_codes).execute(&mut*tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(AppError::Conflict("TOTP source changed".into()));}
    let mut expected=source.clone();expected.totp_enabled=0;expected.totp_secret=None;expected.recovery_codes=None;
    crate::spaces::access::require_current_human_preference_write(&mut tx,&auth).await?;
    if account_lifecycle_user(&mut tx,&source.id).await?!=Some(expected)
        || registration_config_row(&mut tx,"require_2fa").await?!=config
        || std::env::var("REQUIRE_2FA").ok()!=env
        || (valid && !totp::verify_totp_code(secret,&req.code).map_err(AppError::internal)?){return Err(AppError::Conflict("TOTP disable source changed".into()));}
    tx.commit().await.map_err(AppError::internal)?;
    Ok(Json(json!({"totp_enabled":false})))
}

/// GET /api/v1/auth/me
async fn me(State(state): State<AppState>, auth: AuthUser) -> Result<impl IntoResponse, AppError> {
    let user: (String, String, Option<String>, String, Option<String>, String, i32) = sqlx::query_as(
        "SELECT id, email, username, display_name, avatar_url, role, totp_enabled FROM users WHERE id = $1",
    )
    .bind(&auth.claims.sub)
    .fetch_one(&state.db)
    .await
    .map_err(AppError::internal)?;

    Ok(Json(UserResponse {
        account_kind: response_account_kind(&state, &user.0).await?,
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

    let avatar_url = normalize_avatar_url(req.avatar_url.as_deref())
        .map_err(|message| AppError::BadRequest(message.into()))?;
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
            sqlx::query_as("SELECT id FROM users WHERE username = $1 AND id != $2 LIMIT 1")
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
        "UPDATE users SET display_name = $1, avatar_url = $2, username = $3, updated_at = $4 WHERE id = $5",
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

fn normalize_avatar_url(value: Option<&str>) -> Result<Option<String>, &'static str> {
    let value = value.map(str::trim).filter(|value| !value.is_empty());
    let Some(value) = value else {
        return Ok(None);
    };

    if !value.starts_with("data:") {
        if value.len() > MAX_AVATAR_URL_LENGTH {
            return Err("Avatar URL must be at most 500 characters");
        }
        return Ok(Some(value.to_string()));
    }

    if value.len() > MAX_AVATAR_DATA_URL_LENGTH {
        return Err("Profile picture must be smaller than 1 MB");
    }

    let (media_type, encoded) = value
        .split_once(",")
        .ok_or("Profile picture data is invalid")?;
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| "Profile picture data is invalid")?;
    let valid_image = match media_type {
        "data:image/jpeg;base64" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        "data:image/png;base64" => {
            bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a])
        }
        "data:image/webp;base64" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
        _ => return Err("Profile picture must be a JPEG, PNG, or WebP image"),
    };
    if !valid_image {
        return Err("Profile picture data does not match its image type");
    }

    Ok(Some(value.to_string()))
}

/// GET /api/v1/auth/sessions
async fn list_sessions(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    remove_expired_sessions(&state).await?;
    let sessions: Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, device_name, ip_address, user_agent, session_kind, client_platform, last_seen_at, expires_at, created_at FROM sessions WHERE user_id = $1 ORDER BY last_seen_at DESC",
    )
    .bind(&auth.claims.sub)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<SessionResponse> = sessions
        .into_iter()
        .map(
            |(
                id,
                device_name,
                ip_address,
                user_agent,
                session_kind,
                client_platform,
                last_seen_at,
                expires_at,
                created_at,
            )| {
                SessionResponse {
                    is_current: id == auth.claims.sid,
                    id,
                    device_name,
                    ip_address,
                    user_agent,
                    session_kind,
                    client_platform,
                    last_seen_at,
                    expires_at,
                    created_at,
                }
            },
        )
        .collect();

    Ok(Json(result))
}

/// PUT /api/v1/auth/sessions/{session_id}
async fn rename_session(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(session_id): Path<String>,
    Json(req): Json<SessionUpdateRequest>,
) -> Result<impl IntoResponse, AppError> {
    let Some(device_name) = normalized_device_name(Some(&req.device_name))? else {
        return Err(AppError::BadRequest("Session name cannot be empty".into()));
    };
    let duplicate: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM sessions WHERE user_id = $1 AND LOWER(device_name) = LOWER($2) AND id <> $3",
    )
    .bind(&auth.claims.sub)
    .bind(&device_name)
    .bind(&session_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;
    if duplicate.is_some() {
        return Err(AppError::Conflict(
            "Session names must be unique for your account".into(),
        ));
    }
    let result = sqlx::query("UPDATE sessions SET device_name = $1 WHERE id = $2 AND user_id = $3")
        .bind(device_name)
        .bind(&session_id)
        .bind(&auth.claims.sub)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Session not found".into()));
    }
    list_sessions(State(state), auth).await
}

/// DELETE /api/v1/auth/sessions/{session_id}
async fn revoke_session(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(session_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    sqlx::query("DELETE FROM sessions WHERE id = $1 AND user_id = $2")
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
             FROM api_keys WHERE user_id = $1 ORDER BY created_at DESC",
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
         VALUES ($1, $2, $3, $4, $5, 0, $6, $7, $8)",
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
    let result = sqlx::query("DELETE FROM api_keys WHERE id = $1 AND user_id = $2")
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
         VALUES ($1, NULL, $2, $3, $4, 1, $5, $6, $7)",
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
    let result = sqlx::query("DELETE FROM api_keys WHERE id = $1 AND is_service_account = 1")
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
    admin: AdminUser,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    super::middleware::require_current_admin(&mut conn, &admin).await?;
    let users: Vec<(
        String,
        String,
        String,
        Option<String>,
        String,
        i32,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, email, display_name, avatar_url, role, is_active, created_at, updated_at, account_kind \
             FROM users ORDER BY created_at DESC",
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<serde_json::Value> = users
        .into_iter()
        .map(
            |(id, email, display_name, avatar_url, role, is_active, created_at, updated_at, account_kind)| {
                json!({
                    "id": id,
                    "email": email,
                    "display_name": display_name,
                    "avatar_url": avatar_url,
                    "role": role,
                    "account_kind": account_kind,
                    "is_active": is_active != 0,
                    "created_at": created_at,
                    "updated_at": updated_at,
                })
            },
        )
        .collect();

    #[cfg(test)]
    admin_catalog_read_boundary().await;
    super::middleware::require_current_admin(&mut conn, &admin).await?;
    Ok(Json(result))
}

/// Actual account-birth data only, never a principal or permission capability.
/// Separate from unauthenticated registration-origin evidence.
pub(crate) struct AdminAccountBirthAuditOrigin {
    user_id: String,
    space_id: String,
}
impl AdminAccountBirthAuditOrigin {
    pub(crate) fn user_id(&self) -> &str {
        &self.user_id
    }
    pub(crate) fn space_id(&self) -> &str {
        &self.space_id
    }
}
#[derive(Clone, PartialEq, Eq)]
struct AdminBirthPolicy {
    stored: [Option<(String, String, String)>; 2],
    env: [Option<String>; 2],
}
impl AdminBirthPolicy {
    async fn load(conn: &mut sqlx::AnyConnection) -> Result<Self, AppError> {
        Ok(Self {
            stored: [
                registration_config_row(conn, "user_provisioning_allowed_email_domains").await?,
                registration_config_row(conn, "user_provisioning_allowed_roles").await?,
            ],
            env: Self::environment(),
        })
    }
    fn environment() -> [Option<String>; 2] {
        [
            std::env::var("USER_PROVISIONING_ALLOWED_EMAIL_DOMAINS").ok(),
            std::env::var("USER_PROVISIONING_ALLOWED_ROLES").ok(),
        ]
    }
    fn validate(&self, email: &str, role: &str) -> Result<(), AppError> {
        let raw = |n: usize, default: &str| {
            self.env[n].clone().unwrap_or_else(|| {
                self.stored[n]
                    .as_ref()
                    .map(|v| v.1.clone())
                    .unwrap_or_else(|| default.into())
            })
        };
        crate::provisioning::UserProvisioningPolicy::from_config_values(
            &raw(0, ""),
            &raw(1, "admin,member,viewer"),
        )
        .validate(email, role)
        .map_err(AppError::BadRequest)
    }
    fn verify_environment(&self) -> Result<(), AppError> {
        if Self::environment() != self.env {
            return Err(AppError::Conflict(
                "User provisioning policy changed".into(),
            ));
        }
        Ok(())
    }
}
#[cfg(test)]
tokio::task_local! {static ADMIN_BIRTH_HASH_HOOK:std::cell::RefCell<Option<Box<dyn FnOnce()+Send>>>;static ADMIN_BIRTH_FINAL_HOOK:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;}

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
    // Rejection-only preflight keeps known denials ahead of expensive hashing.
    {
        let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
        super::middleware::require_current_admin(&mut conn, &admin).await?;
        let policy = AdminBirthPolicy::load(&mut conn).await?;
        let duplicate: Option<String> =
            sqlx::query_scalar("SELECT id FROM users WHERE lower(email)=$1")
                .bind(&email)
                .fetch_optional(&mut *conn)
                .await
                .map_err(AppError::internal)?;
        super::middleware::require_current_admin(&mut conn, &admin).await?;
        policy.verify_environment()?;
        policy.validate(&email, role)?;
        if duplicate.is_some() {
            return Err(AppError::Conflict("Email already registered".into()));
        }
    }
    let temporary_password = generate_temporary_password();
    #[cfg(test)]
    let _ = ADMIN_BIRTH_HASH_HOOK.try_with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
    let password_hash = password::hash_password(&temporary_password)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    let authority = super::middleware::require_current_admin(&mut tx, &admin).await?;
    let policy = AdminBirthPolicy::load(&mut tx).await?;
    policy.validate(&email, role)?;
    let duplicate: Option<String> =
        sqlx::query_scalar("SELECT id FROM users WHERE lower(email)=$1")
            .bind(&email)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    if duplicate.is_some() {
        return Err(AppError::Conflict("Email already registered".into()));
    }
    let mut user_ids: Vec<String> = sqlx::query_scalar("SELECT id FROM users ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    let prepared = insert_account_with_personal_space(
        &mut tx,
        NewAccountSeed {
            id: Uuid::new_v4().to_string(),
            email: email.clone(),
            display_name: display_name.into(),
            password_hash: Some(password_hash),
            avatar_url: None,
            role: role.into(),
            is_active: true,
            external_id: None,
        },
    )
    .await?;
    verify_prepared_account(&mut tx, &prepared).await?;
    let origin = AdminAccountBirthAuditOrigin {
        user_id: prepared.user_id().into(),
        space_id: prepared.personal.id.clone(),
    };
    let space_event = crate::audit::insert_admin_birth_space_action(&mut tx, &origin).await?;
    let user_event = crate::audit::insert_actor_action(
        &mut tx,
        &authority,
        "user.create",
        "user",
        prepared.user_id(),
    )
    .await?;
    verify_prepared_account(&mut tx, &prepared).await?;
    crate::audit::verify_admin_birth_space_action(&mut tx, &space_event, &origin).await?;
    crate::audit::verify_actor_action(&mut tx, &user_event, &authority).await?;
    user_ids.push(prepared.user_id().into());
    user_ids.sort();
    if sqlx::query_scalar::<_, String>("SELECT id FROM users ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?
        != user_ids
    {
        return Err(AppError::Conflict("Account creation source changed".into()));
    }
    if AdminBirthPolicy::load(&mut tx).await? != policy {
        return Err(AppError::Conflict(
            "User provisioning policy changed".into(),
        ));
    }
    let response = json!({"id":prepared.user_id(),"email":prepared.email(),"display_name":prepared.display_name(),"avatar_url":null,"role":role,"is_active":true,"created_at":prepared.created_at(),"updated_at":prepared.updated_at(),"temporary_password":temporary_password});
    #[cfg(test)]
    if let Ok(Some(hook)) = ADMIN_BIRTH_FINAL_HOOK.try_with(|slot| slot.borrow_mut().take()) {
        hook.await;
    }
    super::middleware::require_current_admin(&mut tx, &admin).await?;
    policy.verify_environment()?;
    tx.commit().await.map_err(AppError::internal)?;
    if let Err(error) = storage::init_space_dirs(&prepared.personal.slug).await {
        tracing::warn!(%error,"Admin account personal-space directory initialization deferred");
    }
    crate::audit::append_actor_action(&space_event).await;
    crate::audit::append_actor_action(&user_event).await;
    Ok((StatusCode::CREATED, Json(response)))
}

/// POST /api/v1/admin/users/{user_id}/password-reset
async fn reset_admin_user_password(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(user_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Rejection-only preflight precedes password hashing. Authority is proved
    // again after acquiring the shared mutation fence.
    {
        let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
        super::middleware::require_current_admin(&mut conn, &admin).await?;
    }
    let temporary_password = generate_temporary_password();
    let password_hash = password::hash_password(&temporary_password)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::middleware::require_current_admin(&mut tx, &admin).await?;
    let source = account_lifecycle_user(&mut tx, &user_id).await?
        .filter(|user| user.is_active == 1)
        .ok_or_else(|| AppError::NotFound("Active user not found".into()))?;
    let sessions = initial_session_row(&mut tx, &user_id).await?;
    let mut expected = source.clone();
    expected.password_hash = Some(password_hash.clone());
    expected.updated_at = chrono::Utc::now().to_rfc3339();
    let affected = sqlx::query(
        "UPDATE users SET password_hash=$1, updated_at=$2 WHERE id=$3 AND is_active=1 \
         AND (password_hash=$4 OR (password_hash IS NULL AND $4 IS NULL)) AND updated_at=$5",
    )
    .bind(&password_hash).bind(&expected.updated_at).bind(&user_id)
    .bind(&source.password_hash).bind(&source.updated_at)
    .execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected();
    if affected != 1 || account_lifecycle_user(&mut tx, &user_id).await? != Some(expected.clone())
        || initial_session_row(&mut tx, &user_id).await? != sessions
    {
        return Err(AppError::Conflict("Password reset source changed".into()));
    }
    // A self-reset intentionally removes the requesting JWT session. Validate
    // its current authority immediately before that removal, then verify only
    // the intended revocation plus its unchanged account/token evidence.
    super::middleware::require_current_admin(&mut tx, &admin).await?;
    let auth = AuthUser { claims: admin.claims.clone(), api_key_scopes: admin.api_key_scopes.clone() };
    let self_session = match crate::spaces::access::principal_from_auth(&auth)? {
        crate::spaces::access::Principal::User { user_id: actor, session_id: Some(sid), .. }
            if actor == user_id => Some(sid.to_owned()),
        _ => None,
    };
    if self_session.as_ref().is_some_and(|sid| !sessions.iter().any(|row| &row.0 == sid)) {
        return Err(AppError::Unauthorized("Admin session unavailable".into()));
    }
    let deleted = sqlx::query("DELETE FROM sessions WHERE user_id=$1")
        .bind(&user_id).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected();
    if deleted != sessions.len() as u64
        || !initial_session_row(&mut tx, &user_id).await?.is_empty()
        || account_lifecycle_user(&mut tx, &user_id).await? != Some(expected)
    {
        return Err(AppError::Conflict("Password reset result changed".into()));
    }
    if self_session.is_some() {
        crate::spaces::access::principal_from_auth(&auth)?;
    } else {
        super::middleware::require_current_admin(&mut tx, &admin).await?;
    }
    tx.commit().await.map_err(AppError::internal)?;
    crate::audit::log_action(&state.db, Some(&admin.claims.sub), None,
        "user.password_reset", Some("user"), Some(&user_id)).await;
    Ok(Json(json!({ "email": source.email, "temporary_password": temporary_password })))
}

/// PUT /api/v1/admin/users/{user_id}
/// Current role/activity decisions and the exact selected rows share one caller
/// transaction. Profile/security fields are compared, never rewritten here.
#[derive(Clone, PartialEq, Eq, sqlx::FromRow)]
struct AccountLifecycleUser {
    account_kind: String,
    id: String,
    email: String,
    display_name: String,
    password_hash: Option<String>,
    avatar_url: Option<String>,
    role: String,
    is_active: i64,
    totp_secret: Option<String>,
    totp_enabled: i64,
    email_otp_enabled: i64,
    recovery_codes: Option<String>,
    created_at: String,
    updated_at: String,
    external_id: Option<String>,
    username: Option<String>,
}
#[derive(PartialEq, Eq)]
struct AccountLifecycleSource {
    spaces: Vec<String>,
    admins: Vec<String>,
    users: Vec<Option<AccountLifecycleUser>>,
}
async fn account_lifecycle_user(
    conn: &mut sqlx::AnyConnection,
    id: &str,
) -> Result<Option<AccountLifecycleUser>, AppError> {
    sqlx::query_as("SELECT account_kind,id,email,display_name,password_hash,avatar_url,role,is_active,totp_secret,totp_enabled,email_otp_enabled,recovery_codes,created_at,updated_at,external_id,username FROM users WHERE id=$1")
        .bind(id).fetch_optional(conn).await.map_err(AppError::internal)
}
async fn account_lifecycle_source(
    conn: &mut sqlx::AnyConnection,
    ids: &[String],
) -> Result<AccountLifecycleSource, AppError> {
    let spaces = sqlx::query_scalar("SELECT id FROM spaces ORDER BY id")
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    let admins = sqlx::query_scalar("SELECT id FROM users WHERE role='admin' ORDER BY id")
        .fetch_all(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    let mut users = Vec::new();
    for id in ids {
        users.push(account_lifecycle_user(conn, id).await?);
    }
    Ok(AccountLifecycleSource {
        spaces,
        admins,
        users,
    })
}
struct NormalizedBulkLifecycle {
    action: String,
    active: bool,
    requested_count: usize,
    targets: Vec<String>,
    skipped: Vec<BulkUserLifecycleSkippedUser>,
}
fn normalize_bulk_lifecycle(
    req: BulkUserLifecycleRequest,
) -> Result<NormalizedBulkLifecycle, AppError> {
    let action = req.action.trim().to_lowercase();
    let active = match action.as_str() {
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
    let mut targets = Vec::new();
    let mut skipped = Vec::new();
    for raw in req.user_ids {
        let user = raw.trim().to_string();
        let reason = if user.is_empty() {
            Some("empty_user_id")
        } else if !seen.insert(user.clone()) {
            Some("duplicate_user_id")
        } else {
            None
        };
        if let Some(reason) = reason {
            skipped.push(BulkUserLifecycleSkippedUser {
                user_id: user,
                reason: reason.into(),
            });
        } else {
            targets.push(user);
        }
    }
    if targets.is_empty() {
        return Err(AppError::BadRequest(
            "At least one non-empty user id is required".into(),
        ));
    }
    if targets.len() > 100 {
        return Err(AppError::BadRequest(
            "Bulk lifecycle requests can include at most 100 unique users".into(),
        ));
    }
    Ok(NormalizedBulkLifecycle {
        action,
        active,
        requested_count,
        targets,
        skipped,
    })
}
enum AccountLifecycleChange<'a> {
    Single {
        target: &'a str,
        request: &'a UserLifecycleRequest,
    },
    Bulk(NormalizedBulkLifecycle),
}
#[cfg(test)]
thread_local! {static ACCOUNT_LIFECYCLE_FINAL_EXPIRY:std::cell::Cell<Option<i64>>=const{std::cell::Cell::new(None)};}
#[cfg(test)]
async fn account_lifecycle_final_pause() {
    if let Some(deadline) = ACCOUNT_LIFECYCLE_FINAL_EXPIRY.with(|slot| slot.take()) {
        while chrono::Utc::now().timestamp() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }
}
async fn mutate_account_lifecycle(
    state: &AppState,
    admin: &AdminUser,
    change: AccountLifecycleChange<'_>,
) -> Result<Option<BulkUserLifecycleResponse>, AppError> {
    use crate::auth::middleware::require_current_admin;
    use crate::spaces::mutation_guards::{lock_audience_for_mutation, lock_spaces_for_mutation};
    let targets: Vec<String> = match &change {
        AccountLifecycleChange::Single { target, .. } => vec![(*target).into()],
        AccountLifecycleChange::Bulk(req) => req.targets.clone(),
    };
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    lock_audience_for_mutation(&mut tx).await?;
    let spaces: Vec<String> = sqlx::query_scalar("SELECT id FROM spaces ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    lock_spaces_for_mutation(
        &mut tx,
        &spaces.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await?;
    require_current_admin(&mut tx, admin).await?;
    // Decide in request order, but acquire all target/admin user rows in global
    // identity order after space guards (compatible with property-user locks).
    let mut ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM users WHERE role='admin' ORDER BY id")
            .fetch_all(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    ids.extend(targets);
    ids.sort();
    ids.dedup();
    for id in &ids {
        sqlx::query("UPDATE users SET id=id WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    }
    require_current_admin(&mut tx, admin).await?;
    let mut expected = account_lifecycle_source(&mut tx, &ids).await?;
    if expected.spaces != spaces || expected.admins.iter().any(|id| !ids.contains(id)) {
        return Err(AppError::Conflict(
            "Account lifecycle source changed".into(),
        ));
    }
    let mut changes: Vec<(String, String, bool)> = Vec::new();
    let response =
        match change {
            AccountLifecycleChange::Single { target, request } => {
                let row = expected
                    .users
                    .iter()
                    .flatten()
                    .find(|row| row.id == target)
                    .ok_or_else(|| AppError::NotFound("User not found".into()))?;
                let next_role = request
                    .role
                    .as_deref()
                    .map(str::trim)
                    .filter(|role| !role.is_empty())
                    .unwrap_or(&row.role)
                    .to_string();
                if next_role != "admin" && next_role != "member" {
                    return Err(AppError::BadRequest(
                        "User role must be admin or member".into(),
                    ));
                }
                if row.account_kind == "guest" && next_role == "admin" {
                    return Err(AppError::BadRequest("Guest accounts cannot be promoted to admin".into()));
                }
                let active = request.is_active.unwrap_or(row.is_active != 0);
                if target == admin.claims.sub && (next_role != "admin" || !active) {
                    return Err(AppError::BadRequest(
                        "You cannot demote or deactivate your own admin account".into(),
                    ));
                }
                if row.role == "admin"
                    && (next_role != "admin" || !active)
                    && !expected.users.iter().flatten().any(|user| {
                        user.id != target && user.role == "admin" && user.is_active != 0
                    })
                {
                    return Err(AppError::BadRequest(
                        "At least one active admin is required".into(),
                    ));
                }
                changes.push((target.into(), next_role, active));
                None
            }
            AccountLifecycleChange::Bulk(mut req) => {
                let mut remaining = expected
                    .users
                    .iter()
                    .flatten()
                    .filter(|row| row.role == "admin" && row.is_active != 0)
                    .count();
                for target in req.targets {
                    let row = expected.users.iter().flatten().find(|row| row.id == target);
                    let reason = match row {
                        None => Some("user_not_found"),
                        Some(row) if (row.is_active != 0) == req.active => Some(if req.active {
                            "already_active"
                        } else {
                            "already_inactive"
                        }),
                        Some(_) if !req.active && target == admin.claims.sub => {
                            Some("cannot_deactivate_self")
                        }
                        Some(row) if !req.active && row.role == "admin" && remaining <= 1 => {
                            Some("last_active_admin_required")
                        }
                        Some(row) => {
                            if !req.active && row.role == "admin" {
                                remaining -= 1;
                            }
                            changes.push((target.clone(), row.role.clone(), req.active));
                            None
                        }
                    };
                    if let Some(reason) = reason {
                        req.skipped.push(BulkUserLifecycleSkippedUser {
                            user_id: target,
                            reason: reason.into(),
                        });
                    }
                }
                Some(BulkUserLifecycleResponse {
                    action: req.action,
                    requested_count: req.requested_count,
                    updated_count: changes.len(),
                    skipped_count: req.skipped.len(),
                    skipped: req.skipped,
                })
            }
        };
    let action = if response.is_some() {
        "user.lifecycle_bulk_update"
    } else {
        "user.lifecycle_update"
    };
    for (id, role, active) in &changes {
        let updated = chrono::Utc::now().to_rfc3339();
        let result = if response.is_some() {
            sqlx::query("UPDATE users SET is_active=$1,updated_at=$2 WHERE id=$3")
                .bind(i64::from(*active))
                .bind(&updated)
                .bind(id)
                .execute(&mut *tx)
                .await
        } else {
            sqlx::query("UPDATE users SET role=$1,is_active=$2,updated_at=$3 WHERE id=$4")
                .bind(role)
                .bind(i64::from(*active))
                .bind(&updated)
                .bind(id)
                .execute(&mut *tx)
                .await
        }
        .map_err(AppError::internal)?;
        if result.rows_affected() != 1 {
            return Err(AppError::Conflict(
                "Account lifecycle source changed".into(),
            ));
        }
        let row = expected
            .users
            .iter_mut()
            .flatten()
            .find(|row| &row.id == id)
            .ok_or_else(|| AppError::Conflict("Account lifecycle source changed".into()))?;
        row.role = role.clone();
        row.is_active = i64::from(*active);
        row.updated_at = updated;
    }
    expected.admins = expected
        .users
        .iter()
        .flatten()
        .filter(|row| row.role == "admin")
        .map(|row| row.id.clone())
        .collect();
    expected.admins.sort();
    let authority = require_current_admin(&mut tx, admin).await?;
    let mut events = Vec::new();
    for (id, _, _) in &changes {
        events.push(
            crate::audit::insert_actor_action(&mut tx, &authority, action, "user", id).await?,
        );
    }
    let authority = require_current_admin(&mut tx, admin).await?;
    if account_lifecycle_source(&mut tx, &ids).await? != expected {
        return Err(AppError::Conflict(
            "Account lifecycle source changed".into(),
        ));
    }
    for event in &events {
        crate::audit::verify_actor_action(&mut tx, event, &authority).await?;
    }
    #[cfg(test)]
    account_lifecycle_final_pause().await;
    require_current_admin(&mut tx, admin).await?;
    tx.commit().await.map_err(AppError::internal)?;
    for event in &events {
        crate::audit::append_actor_action(event).await;
    }
    Ok(response)
}
async fn update_user_lifecycle(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(user): Path<String>,
    Json(req): Json<UserLifecycleRequest>,
) -> Result<impl IntoResponse, AppError> {
    mutate_account_lifecycle(
        &state,
        &admin,
        AccountLifecycleChange::Single {
            target: &user,
            request: &req,
        },
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
/// POST /api/v1/admin/users/bulk-lifecycle
async fn bulk_update_user_lifecycle(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(req): Json<BulkUserLifecycleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let req = normalize_bulk_lifecycle(req)?;
    let response = mutate_account_lifecycle(&state, &admin, AccountLifecycleChange::Bulk(req))
        .await?
        .ok_or_else(|| AppError::Internal("Missing lifecycle response".into()))?;
    Ok(Json(response))
}

/// GET /api/v1/admin/users/{user_id}/sessions
async fn list_admin_user_sessions(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(user_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    super::middleware::require_current_admin(&mut conn, &admin).await?;
    let user_exists: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE id = $1")
        .bind(&user_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if user_exists.is_none() {
        #[cfg(test)]
        admin_catalog_read_boundary().await;
        super::middleware::require_current_admin(&mut conn, &admin).await?;
        return Err(AppError::NotFound("User not found".into()));
    }

    let sessions: Vec<(
        String,
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        String,
        String,
    )> = sqlx::query_as(
        "SELECT id, device_name, ip_address, user_agent, session_kind, client_platform, last_seen_at, expires_at, created_at \
         FROM sessions WHERE user_id = $1 AND expires_at > $2 ORDER BY last_seen_at DESC",
    )
    .bind(&user_id)
    .bind(chrono::Utc::now().to_rfc3339())
    .fetch_all(&mut *conn)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<SessionResponse> = sessions
        .into_iter()
        .map(
            |(
                id,
                device_name,
                ip_address,
                user_agent,
                session_kind,
                client_platform,
                last_seen_at,
                expires_at,
                created_at,
            )| {
                SessionResponse {
                    is_current: user_id == admin.claims.sub && id == admin.claims.sid,
                    id,
                    device_name,
                    ip_address,
                    user_agent,
                    session_kind,
                    client_platform,
                    last_seen_at,
                    expires_at,
                    created_at,
                }
            },
        )
        .collect();

    #[cfg(test)]
    admin_catalog_read_boundary().await;
    super::middleware::require_current_admin(&mut conn, &admin).await?;
    Ok(Json(result))
}

/// DELETE /api/v1/admin/users/{user_id}/sessions/{session_id}
async fn revoke_admin_user_session(
    State(state): State<AppState>,
    admin: AdminUser,
    Path((user_id, session_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let result = sqlx::query("DELETE FROM sessions WHERE id = $1 AND user_id = $2")
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
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    crate::spaces::access::require_current_human_member_read(&mut conn, &auth).await?;
    let users: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, email, display_name, avatar_url FROM users \
         WHERE is_active = 1 ORDER BY display_name, email",
    )
    .fetch_all(&mut *conn)
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

    crate::spaces::access::require_current_human_member_read(&mut conn, &auth).await?;
    crate::spaces::access::principal_from_auth(&auth)?;
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
        "INSERT INTO system_config (key, value, updated_at) VALUES ($1, $2, $3) \
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

    sqlx::query("DELETE FROM system_config WHERE key = $1")
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
        sqlx::query_as("SELECT value FROM system_config WHERE key = $1")
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

// Provider configuration is retained privately as exact exchange evidence; no
// secrets or verified-origin data are serialized, logged or used as AuthUser.
const OIDC_SOURCE_KEYS: [(&str, &str, &str); 10] = [
    ("oidc_enabled", "OIDC_ENABLED", "false"),
    ("oidc_display_name", "OIDC_DISPLAY_NAME", "SSO"),
    ("oidc_issuer_url", "OIDC_ISSUER_URL", ""),
    ("oidc_client_id", "OIDC_CLIENT_ID", ""),
    ("oidc_client_secret", "OIDC_CLIENT_SECRET", ""),
    ("oidc_scopes", "OIDC_SCOPES", "openid,profile,email"),
    ("oidc_user_claim", "OIDC_USER_CLAIM", "email"),
    ("oidc_role_claim", "OIDC_ROLE_CLAIM", ""),
    ("oidc_role_admin_value", "OIDC_ROLE_ADMIN_VALUE", "admin"),
    ("oidc_auto_provision", "OIDC_AUTO_PROVISION", "true"),
];
#[derive(PartialEq, Eq)]
struct OidcSource {
    rows: Vec<Option<(String, String, String)>>,
    env: Vec<Option<String>>,
}
impl OidcSource {
    async fn load(conn: &mut sqlx::AnyConnection) -> Result<Self, AppError> {
        let mut rows = Vec::new();
        for (key, _, _) in OIDC_SOURCE_KEYS {
            rows.push(registration_config_row(conn, key).await?);
        }
        Ok(Self {
            rows,
            env: Self::environment(),
        })
    }
    fn environment() -> Vec<Option<String>> {
        OIDC_SOURCE_KEYS
            .iter()
            .map(|(_, env, _)| std::env::var(env).ok())
            .collect()
    }
    fn value(&self, n: usize) -> String {
        self.env[n].clone().unwrap_or_else(|| {
            self.rows[n]
                .as_ref()
                .map(|r| r.1.clone())
                .unwrap_or_else(|| OIDC_SOURCE_KEYS[n].2.into())
        })
    }
    fn config(&self) -> oidc::OidcConfig {
        oidc::OidcConfig {
            enabled: self.value(0) == "true",
            display_name: self.value(1),
            issuer_url: self.value(2),
            client_id: self.value(3),
            client_secret: self.value(4),
            scopes: self
                .value(5)
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            user_claim: self.value(6),
            role_claim: self.value(7),
            role_admin_value: self.value(8),
            auto_provision: self.value(9) == "true",
        }
    }
    fn environment_current(&self) -> bool {
        self.env == Self::environment()
    }
}
async fn load_oidc_config(state: &AppState) -> Result<oidc::OidcConfig, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    Ok(OidcSource::load(&mut conn).await?.config())
}
enum ExternalConfig {
    OAuth(oauth::ProviderConfig),
    Oidc(OidcSource),
}
impl ExternalConfig {
    async fn require_current(&self, conn: &mut sqlx::AnyConnection) -> Result<(), AppError> {
        let current = match self {
            Self::OAuth(c) => c.is_current(),
            Self::Oidc(c) => c.config().enabled && OidcSource::load(conn).await? == *c,
        };
        if !current {
            return Err(AppError::Unauthorized(
                "Sign-in configuration changed; restart sign-in".into(),
            ));
        }
        Ok(())
    }
    fn environment_current(&self) -> bool {
        match self {
            Self::OAuth(c) => c.is_current(),
            Self::Oidc(c) => c.environment_current(),
        }
    }
}
/// Constructed only by the real callback AFTER successful provider verification.
/// This evidence describes a provider-authenticated subject, not a human/Admin
/// permission capability or an assertion of a new email-verification policy.
struct VerifiedProviderOrigin {
    info: oauth::OAuthUserInfo,
    config: ExternalConfig,
    auto_provision: bool,
    role_override: Option<String>,
}
impl VerifiedProviderOrigin {
    fn bound(&self) -> bool {
        match &self.config {
            ExternalConfig::OAuth(c) => {
                self.info.provider == c.name()
                    && self.auto_provision
                    && self.role_override.is_none()
            }
            ExternalConfig::Oidc(c) => {
                let config = c.config();
                self.info.provider == "oidc"
                    && self.auto_provision == config.auto_provision
                    && (self.role_override.is_none() == config.role_claim.is_empty())
            }
        }
    }
}
/// Fixed, newly verified personal-space birth data. No authentication capability.
pub(crate) struct OAuthAccountBirthAuditOrigin {
    user_id: String,
    space_id: String,
}
impl OAuthAccountBirthAuditOrigin {
    pub(crate) fn user_id(&self) -> &str {
        &self.user_id
    }
    pub(crate) fn space_id(&self) -> &str {
        &self.space_id
    }
}
type OAuthAccountRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    String,
);
async fn oauth_rows(
    conn: &mut sqlx::AnyConnection,
    user: &str,
) -> Result<Vec<OAuthAccountRow>, AppError> {
    sqlx::query_as("SELECT id,user_id,provider,provider_user_id,provider_email,access_token,refresh_token,token_expires_at,created_at,updated_at FROM oauth_accounts WHERE user_id=$1 ORDER BY id").bind(user).fetch_all(conn).await.map_err(AppError::internal)
}
async fn external_session_rows(
    conn: &mut sqlx::AnyConnection,
    user: &str,
) -> Result<Vec<InitialSessionRow>, AppError> {
    sqlx::query_as("SELECT id,user_id,refresh_token_hash,device_name,ip_address,user_agent,last_seen_at,expires_at,created_at,session_kind,client_platform FROM sessions WHERE user_id=$1 ORDER BY id").bind(user).fetch_all(conn).await.map_err(AppError::internal)
}
async fn provider_link(
    conn: &mut sqlx::AnyConnection,
    info: &oauth::OAuthUserInfo,
) -> Result<Option<OAuthAccountRow>, AppError> {
    sqlx::query_as("SELECT id,user_id,provider,provider_user_id,provider_email,access_token,refresh_token,token_expires_at,created_at,updated_at FROM oauth_accounts WHERE provider=$1 AND provider_user_id=$2").bind(&info.provider).bind(&info.provider_user_id).fetch_optional(conn).await.map_err(AppError::internal)
}
async fn verify_prepared_oauth_account(
    conn: &mut sqlx::AnyConnection,
    new: &PreparedAccount,
    links: &[OAuthAccountRow],
    sessions: &[InitialSessionRow],
) -> Result<(), AppError> {
    if links.len() != 1
        || sessions.len() != 1
        || links[0].1 != new.user_id()
        || sessions[0].1 != new.user_id()
    {
        return Err(AppError::Conflict(
            "Initial provider associations changed".into(),
        ));
    }
    require_newborn_owned_resources_empty(conn, new.user_id()).await?;
    verify_prepared_account_sources(conn, new, 2).await
}
/// Unchanged existing-account authority/credential associations. Last-used
/// telemetry for keys/passkeys is excluded; scopes, owner, expiry, credentials
/// and passkey auth counters are included. Raw data is transient, never emitted.
async fn external_account_associations(
    conn: &mut sqlx::AnyConnection,
    user: &str,
) -> Result<[u8; 32], AppError> {
    const GROUPS: &str = "SELECT group_id FROM group_members WHERE user_id=$1";
    const SCOPES:&str="SELECT space_id FROM space_members WHERE user_id=$1 UNION SELECT space_id FROM space_group_permissions WHERE group_id IN (SELECT group_id FROM group_members WHERE user_id=$1) UNION SELECT id FROM spaces WHERE created_by=$1";
    let specs:Vec<(&str,&str,String)>=vec![
 ("spaces","id,name,slug,description,icon,accent_color,visibility,created_by,created_at,updated_at,trash_retention_days",format!("id IN ({SCOPES})")),
 ("space_members","id,space_id,user_id,role,created_at","user_id=$1".into()),
 ("groups","id,name,description,created_at,updated_at,external_id,scim_managed,managed_space_id",format!("id IN ({GROUPS})")),
 ("group_members","id,group_id,user_id,created_at","user_id=$1".into()),
 ("space_group_permissions","id,space_id,group_id,role,created_at,updated_at",format!("group_id IN ({GROUPS})")),
 ("group_global_roles","group_id,role_id,created_at",format!("group_id IN ({GROUPS})")),
 ("global_roles","id,name,description,permissions,built_in,created_at,updated_at",format!("id IN (SELECT role_id FROM group_global_roles WHERE group_id IN ({GROUPS}))")),
 ("space_roles","id,space_id,name,description,permissions,created_by,created_at,updated_at",format!("space_id IN ({SCOPES})")),
 ("space_user_blocks","space_id,user_id,created_by,created_at","user_id=$1".into()),
 ("document_drafts","id,document_id,space_id,owner_id,title,icon,cover_image,content,base_updated_at,created_at,updated_at,previous_folder_id,previous_parent_id","owner_id=$1".into()),
 ("template_drafts","id,template_id,space_id,owner_id,snapshot,created_at,updated_at,last_autosaved_at,lock_owner_id,lock_expires_at","owner_id=$1".into()),
 ("document_draft_access","draft_id,user_id,role,granted_by,created_at,updated_at","user_id=$1 OR draft_id IN (SELECT id FROM document_drafts WHERE owner_id=$1)".into()),
 ("document_access_allowlist","id,document_id,space_id,user_id,group_id,created_by,created_at",format!("user_id=$1 OR group_id IN ({GROUPS})")),
 ("document_access_policies","document_id,space_id,mode,revision,created_by,created_at,updated_at",format!("document_id IN (SELECT document_id FROM document_access_allowlist WHERE user_id=$1 OR group_id IN ({GROUPS}))")),
 ("api_keys","id,user_id,name,key_hash,key_prefix,is_service_account,scopes,expires_at,created_at","user_id=$1".into()),
 ("passkeys","id,user_id,credential_id,public_key,name,sign_count,created_at","user_id=$1".into()),
 ];
    let mut hash = Sha256::new();
    hash.update(b"starlane-oauth-unchanged-associations-v1");
    for (table, columns, scope) in specs {
        let selected = columns
            .split(',')
            .map(|c| format!("CAST({c} AS TEXT)"))
            .collect::<Vec<_>>()
            .join(",");
        let rows = sqlx::query(&format!("SELECT {selected} FROM {table} WHERE {scope}"))
            .bind(user)
            .fetch_all(&mut *conn)
            .await
            .map_err(AppError::internal)?;
        let mut raw: Vec<Vec<Option<String>>> = Vec::new();
        for row in rows {
            let mut values = Vec::new();
            for index in 0..columns.split(',').count() {
                values.push(
                    sqlx::Row::try_get::<Option<String>, _>(&row, index)
                        .map_err(AppError::internal)?,
                );
            }
            raw.push(values);
        }
        raw.sort();
        hash.update(serde_json::to_vec(&(table, raw)).map_err(AppError::internal)?);
    }
    Ok(hash.finalize().into())
}
#[cfg(test)]
tokio::task_local! {static EXTERNAL_SIGNIN_FINAL_HOOK:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;}
async fn complete_oauth_sign_in(
    state: &AppState,
    origin: VerifiedProviderOrigin,
) -> Result<Redirect, AppError> {
    if !origin.bound() {
        return Err(AppError::Unauthorized(
            "Provider identity unavailable".into(),
        ));
    }
    let info = &origin.info;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    let spaces: Vec<String> = sqlx::query_scalar("SELECT id FROM spaces ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(
        &mut tx,
        &spaces.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await?;
    origin.config.require_current(&mut tx).await?;
    let mut user_ids: Vec<String> = sqlx::query_scalar("SELECT id FROM users ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    let linked = provider_link(&mut tx, info).await?;
    let email_user: Option<String> = if linked.is_none() {
        sqlx::query_scalar("SELECT id FROM users WHERE email=$1")
            .bind(&info.email)
            .fetch_optional(&mut *tx)
            .await
            .map_err(AppError::internal)?
    } else {
        None
    };
    let mut prepared = None;
    let user_id = if let Some(link) = &linked {
        link.1.clone()
    } else if let Some(user) = email_user {
        user
    } else {
        require_oauth_auto_provisioning(&info.provider, origin.auto_provision)?;
        let value = insert_account_with_personal_space(
            &mut tx,
            NewAccountSeed {
                id: Uuid::new_v4().to_string(),
                email: info.email.clone(),
                display_name: info.display_name.clone(),
                password_hash: None,
                avatar_url: info.avatar_url.clone(),
                role: "member".into(),
                is_active: true,
                external_id: None,
            },
        )
        .await?;
        verify_prepared_account(&mut tx, &value).await?;
        let user = value.user_id().to_string();
        prepared = Some(value);
        user
    };
    let user = account_lifecycle_user(&mut tx, &user_id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Provider account unavailable".into()))?;
    if user.is_active != 1 {
        return Err(AppError::Unauthorized(
            "Provider account unavailable".into(),
        ));
    }
    let associations = if prepared.is_none() {
        Some(external_account_associations(&mut tx, &user_id).await?)
    } else {
        None
    };
    let mut links = oauth_rows(&mut tx, &user_id).await?;
    let mut sessions = external_session_rows(&mut tx, &user_id).await?;
    if sqlx::query("UPDATE users SET email=email WHERE id=$1 AND is_active=1")
        .bind(&user_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?
        .rows_affected()
        != 1
    {
        return Err(AppError::Unauthorized(
            "Provider account unavailable".into(),
        ));
    }
    if account_lifecycle_user(&mut tx, &user_id).await? != Some(user.clone())
        || oauth_rows(&mut tx, &user_id).await? != links
        || external_session_rows(&mut tx, &user_id).await? != sessions
    {
        return Err(AppError::Conflict("Provider sign-in source changed".into()));
    }
    // The no-op account lock can fire a corrupt trigger. Preserve the exact link
    // selected before it and prohibit a raced-in subject binding on a linking path.
    if provider_link(&mut tx, info).await? != linked {
        return Err(AppError::Conflict("Provider link changed".into()));
    }
    if linked.is_none() {
        let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let link: OAuthAccountRow = (
            Uuid::new_v4().to_string(),
            user_id.clone(),
            info.provider.clone(),
            info.provider_user_id.clone(),
            Some(info.email.clone()),
            None,
            None,
            None,
            now.clone(),
            now,
        );
        if sqlx::query("INSERT INTO oauth_accounts(id,user_id,provider,provider_user_id,provider_email,access_token,refresh_token,token_expires_at,created_at,updated_at) VALUES($1,$2,$3,$4,$5,NULL,NULL,NULL,$6,$7)").bind(&link.0).bind(&link.1).bind(&link.2).bind(&link.3).bind(&link.4).bind(&link.8).bind(&link.9).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(AppError::Conflict("Provider link changed".into()));}
        links.push(link);
        links.sort_by(|a, b| a.0.cmp(&b.0));
    }
    let final_role = if user.account_kind == "guest" { "viewer" } else { origin.role_override.as_deref().unwrap_or(&user.role) };
    let tokens = jwt::create_token_pair(&user_id, &info.email, final_role, &state.jwt_secret)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let names: Vec<(Option<String>,)> = sessions.iter().map(|r| (r.3.clone(),)).collect();
    let device_name = unique_session_name_from_rows(&names, "Web browser")?;
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let session: InitialSessionRow = (
        tokens.session_id.clone(),
        user_id.clone(),
        hash_refresh_token(&tokens.refresh_token),
        Some(device_name),
        None,
        None,
        now.clone(),
        session_expiry_at("browser"),
        now,
        "browser".into(),
        "web".into(),
    );
    if sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,device_name,ip_address,user_agent,last_seen_at,expires_at,created_at,session_kind,client_platform) VALUES($1,$2,$3,$4,NULL,NULL,$5,$6,$7,$8,$9)").bind(&session.0).bind(&session.1).bind(&session.2).bind(&session.3).bind(&session.6).bind(&session.7).bind(&session.8).bind(&session.9).bind(&session.10).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(AppError::Conflict("Provider session changed".into()));}
    sessions.push(session);
    sessions.sort_by(|a, b| a.0.cmp(&b.0));
    let audit = if let Some(new) = &prepared {
        verify_prepared_oauth_account(&mut tx, new, &links, &sessions).await?;
        let birth = OAuthAccountBirthAuditOrigin {
            user_id: user_id.clone(),
            space_id: new.personal.id.clone(),
        };
        let event = crate::audit::insert_oauth_birth_space_action(&mut tx, &birth).await?;
        Some((event, birth))
    } else {
        None
    };
    #[cfg(test)]
    if let Ok(Some(hook)) = EXTERNAL_SIGNIN_FINAL_HOOK.try_with(|s| s.borrow_mut().take()) {
        hook.await;
    }
    if let Some(new) = &prepared {
        verify_prepared_oauth_account(&mut tx, new, &links, &sessions).await?;
    }
    if account_lifecycle_user(&mut tx, &user_id).await? != Some(user)
        || oauth_rows(&mut tx, &user_id).await? != links
        || external_session_rows(&mut tx, &user_id).await? != sessions
    {
        return Err(AppError::Conflict("Provider sign-in source changed".into()));
    }
    let expected_link = links
        .iter()
        .find(|l| l.2 == info.provider && l.3 == info.provider_user_id)
        .cloned();
    if provider_link(&mut tx, info).await? != expected_link {
        return Err(AppError::Conflict("Provider link changed".into()));
    }
    if let Some((event, birth)) = &audit {
        crate::audit::verify_oauth_birth_space_action(&mut tx, event, birth).await?;
    }
    if let Some(expected) = associations {
        if external_account_associations(&mut tx, &user_id).await? != expected {
            return Err(AppError::Conflict(
                "Provider account associations changed".into(),
            ));
        }
    }
    if prepared.is_some() {
        user_ids.push(user_id.clone());
        user_ids.sort();
    }
    if sqlx::query_scalar::<_, String>("SELECT id FROM users ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?
        != user_ids
    {
        return Err(AppError::Conflict(
            "Provider account population changed".into(),
        ));
    }
    // Last awaited local credential/configuration proof. No provider network calls
    // occur under this transaction. There is no cross-provider revocation claim.
    origin.config.require_current(&mut tx).await?;
    if !origin.config.environment_current() {
        return Err(AppError::Unauthorized(
            "Sign-in configuration changed; restart sign-in".into(),
        ));
    }
    let redirect_url = format!(
        "{}/#access_token={}&refresh_token={}",
        state.base_url, tokens.access_token, tokens.refresh_token
    );
    tx.commit().await.map_err(AppError::internal)?;
    if let Some(new) = &prepared {
        if let Err(error) = storage::init_space_dirs(&new.personal.slug).await {
            tracing::warn!(%error,"Provider personal-space directory initialization deferred");
        }
    }
    if let Some((event, _)) = &audit {
        crate::audit::append_actor_action(event).await;
    }
    Ok(Redirect::temporary(&redirect_url))
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

/// Account fields chosen by the real provisioning caller, not an authority.
/// Secrets are never Debug/Serialize; helpers neither authorize nor commit.
pub(crate) struct NewAccountSeed {
    pub(crate) id: String,
    pub(crate) email: String,
    pub(crate) display_name: String,
    pub(crate) password_hash: Option<String>,
    pub(crate) avatar_url: Option<String>,
    pub(crate) role: String,
    pub(crate) is_active: bool,
    pub(crate) external_id: Option<String>,
}
pub(crate) struct PreparedPersonalSpace {
    pub(crate) id: String,
    pub(crate) slug: String,
    expected: (
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
        String,
        String,
        String,
        i32,
    ),
    member: (String, String, String, String, String),
    group: crate::spaces::routes::PreparedManagedSpaceGroup,
}
pub(crate) struct PreparedAccount {
    expected: AccountLifecycleUser,
    pub(crate) personal: PreparedPersonalSpace,
}
impl PreparedAccount {
    pub(crate) fn user_id(&self) -> &str {
        &self.expected.id
    }
    pub(crate) fn email(&self) -> &str {
        &self.expected.email
    }
    pub(crate) fn display_name(&self) -> &str {
        &self.expected.display_name
    }
    pub(crate) fn is_active(&self) -> bool {
        self.expected.is_active != 0
    }
    pub(crate) fn created_at(&self) -> &str {
        &self.expected.created_at
    }
    pub(crate) fn updated_at(&self) -> &str {
        &self.expected.updated_at
    }
    pub(crate) fn external_id(&self) -> Option<&str> {
        self.expected.external_id.as_deref()
    }
}
/// Caller owns a transaction and must hold the audience fence BEFORE invoking
/// this SQL-only helper. No principal, session or mutation capability is invented.
pub(crate) async fn create_personal_space_on_connection(
    conn: &mut sqlx::AnyConnection,
    user_id: &str,
    display_name: &str,
) -> Result<PreparedPersonalSpace, AppError> {
    let kind: String = sqlx::query_scalar("SELECT account_kind FROM users WHERE id=$1")
        .bind(user_id).fetch_one(&mut *conn).await.map_err(AppError::internal)?;
    if kind != "member" { return Err(AppError::Forbidden("Guest accounts cannot create personal spaces".into())); }
    let space_id = Uuid::new_v4().to_string();
    let member_id = Uuid::new_v4().to_string();
    let space_name = format!("{}'s Space", display_name.trim());
    let fallback_slug = format!("personal-{}", user_id);
    let mut slug = storage::sanitize_slug(&space_name.replace(' ', "-"));
    if slug.is_empty() {
        slug = storage::sanitize_slug(&fallback_slug);
    }
    let existing: Option<String> = sqlx::query_scalar("SELECT id FROM spaces WHERE slug=$1")
        .bind(&slug)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if existing.is_some() {
        slug = storage::sanitize_slug(&fallback_slug);
    }
    // Keep the legacy default timestamp format, while binding exact expected bytes.
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    if sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by,created_at,updated_at,description,icon,accent_color,trash_retention_days) VALUES($1,$2,$3,'internal',$4,$5,$5,NULL,NULL,NULL,30)").bind(&space_id).bind(&space_name).bind(&slug).bind(user_id).bind(&now).execute(&mut *conn).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(AppError::Conflict("Personal space changed".into()));}
    if sqlx::query("INSERT INTO space_members(id,space_id,user_id,role,created_at) VALUES($1,$2,$3,'owner',$4)").bind(&member_id).bind(&space_id).bind(user_id).bind(&now).execute(&mut *conn).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(AppError::Conflict("Personal membership changed".into()));}
    let group = crate::spaces::routes::prepare_managed_space_group_on_connection(
        conn,
        &space_id,
        &space_name,
        user_id,
    )
    .await?;
    Ok(PreparedPersonalSpace {
        id: space_id.clone(),
        slug: slug.clone(),
        expected: (
            space_id.clone(),
            space_name,
            slug,
            None,
            None,
            None,
            "internal".into(),
            user_id.into(),
            now.clone(),
            now.clone(),
            30,
        ),
        member: (member_id, space_id, user_id.into(), "owner".into(), now),
        group,
    })
}
pub(crate) async fn verify_prepared_personal_space(
    conn: &mut sqlx::AnyConnection,
    prepared: &PreparedPersonalSpace,
) -> Result<(), AppError> {
    let row:Option<(String,String,String,Option<String>,Option<String>,Option<String>,String,String,String,String,i32)>=sqlx::query_as("SELECT id,name,slug,description,icon,accent_color,visibility,created_by,created_at,updated_at,trash_retention_days FROM spaces WHERE id=$1").bind(&prepared.id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let members:Vec<(String,String,String,String,String)>=sqlx::query_as("SELECT id,space_id,user_id,role,created_at FROM space_members WHERE space_id=$1 ORDER BY id").bind(&prepared.id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let groups: Vec<String> =
        sqlx::query_scalar("SELECT id FROM groups WHERE managed_space_id=$1 ORDER BY id")
            .bind(&prepared.id)
            .fetch_all(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    let grants: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM space_group_permissions WHERE space_id=$1")
            .bind(&prepared.id)
            .fetch_one(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    let extras:i64=sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM documents WHERE space_id=$1)+(SELECT COUNT(*) FROM folders WHERE space_id=$1)+(SELECT COUNT(*) FROM tags WHERE space_id=$1)+(SELECT COUNT(*) FROM properties_schema WHERE space_id=$1)+(SELECT COUNT(*) FROM workflow_templates WHERE space_id=$1)+(SELECT COUNT(*) FROM templates WHERE space_id=$1)+(SELECT COUNT(*) FROM space_user_blocks WHERE space_id=$1)+(SELECT COUNT(*) FROM space_roles WHERE space_id=$1)").bind(&prepared.id).fetch_one(&mut *conn).await.map_err(AppError::internal)?;
    if row != Some(prepared.expected.clone())
        || members != vec![prepared.member.clone()]
        || groups != vec![prepared.group.id.clone()]
        || grants != 1
        || extras != 0
    {
        return Err(AppError::Conflict("Prepared personal space changed".into()));
    }
    prepared.group.verify_new(conn).await
}
/// One consumed account+topology SQL seam. Real caller approval and the initial
/// audience fence precede account insertion; no pool, filesystem or commit here.
pub(crate) async fn insert_account_with_personal_space(
    conn: &mut sqlx::AnyConnection,
    seed: NewAccountSeed,
) -> Result<PreparedAccount, AppError> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let expected = AccountLifecycleUser {
        account_kind: "member".into(),
        id: seed.id,
        email: seed.email,
        display_name: seed.display_name,
        password_hash: seed.password_hash,
        avatar_url: seed.avatar_url,
        role: seed.role,
        is_active: i64::from(seed.is_active),
        totp_secret: None,
        totp_enabled: 0,
        email_otp_enabled: 0,
        recovery_codes: None,
        created_at: now.clone(),
        updated_at: now,
        external_id: seed.external_id,
        username: None,
    };
    let result=sqlx::query("INSERT INTO users(id,email,display_name,password_hash,avatar_url,role,is_active,external_id,created_at,updated_at,totp_secret,totp_enabled,email_otp_enabled,recovery_codes,username) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,NULL,0,0,NULL,NULL)").bind(&expected.id).bind(&expected.email).bind(&expected.display_name).bind(&expected.password_hash).bind(&expected.avatar_url).bind(&expected.role).bind(expected.is_active).bind(&expected.external_id).bind(&expected.created_at).bind(&expected.updated_at).execute(&mut *conn).await.map_err(AppError::internal)?;
    if result.rows_affected() != 1 {
        return Err(AppError::Conflict("Prepared account changed".into()));
    }
    let personal =
        create_personal_space_on_connection(conn, &expected.id, &expected.display_name).await?;
    Ok(PreparedAccount { expected, personal })
}
pub(crate) async fn verify_prepared_account(
    conn: &mut sqlx::AnyConnection,
    prepared: &PreparedAccount,
) -> Result<(), AppError> {
    verify_prepared_account_sources(conn, prepared, 0).await
}
/// Initial-birth integrity only: ownership is real private/draft authority, not attribution.
/// Existing-account logins must preserve legitimate ownership and must not call this.
pub(crate) async fn require_newborn_owned_resources_empty(
    conn: &mut sqlx::AnyConnection,
    user_id: &str,
) -> Result<(), AppError> {
    let count: i64 = sqlx::query_scalar("SELECT (SELECT COUNT(*) FROM document_drafts WHERE owner_id=$1)+(SELECT COUNT(*) FROM template_drafts WHERE owner_id=$1)")
        .bind(user_id).fetch_one(conn).await.map_err(AppError::internal)?;
    if count != 0 {
        return Err(AppError::Conflict(
            "Prepared account owned resources changed".into(),
        ));
    }
    Ok(())
}

/// CSV-only initial editor memberships, chosen and locked by its real Admin caller.
/// This exact-record proof grants nothing and does not relax the generic birth verifier.
pub(crate) type InitialCsvMembership = (String, String, String, String, String);
pub(crate) async fn verify_prepared_account_with_initial_memberships(
    conn: &mut sqlx::AnyConnection,
    prepared: &PreparedAccount,
    extra: &[InitialCsvMembership],
) -> Result<(), AppError> {
    if account_lifecycle_user(conn, prepared.user_id()).await? != Some(prepared.expected.clone()) {
        return Err(AppError::Conflict("Prepared CSV account changed".into()));
    }
    let mut expected = vec![prepared.personal.member.clone()];
    let mut spaces = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();
    for row in extra {
        if row.2 != prepared.user_id()
            || row.3 != "editor"
            || row.1 == prepared.personal.id
            || !spaces.insert(row.1.clone())
            || !ids.insert(row.0.clone())
        {
            return Err(AppError::Conflict("Prepared CSV membership changed".into()));
        }
        expected.push(row.clone());
    }
    expected.sort_by(|a, b| a.0.cmp(&b.0));
    let actual: Vec<InitialCsvMembership> = sqlx::query_as("SELECT id,space_id,user_id,role,created_at FROM space_members WHERE user_id=$1 ORDER BY id")
        .bind(prepared.user_id()).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let closure: (i64,i64,i64)=sqlx::query_as("SELECT (SELECT COUNT(*) FROM spaces WHERE created_by=$1),(SELECT COUNT(*) FROM group_members WHERE user_id=$1),(SELECT COUNT(*) FROM sessions WHERE user_id=$1)+(SELECT COUNT(*) FROM oauth_accounts WHERE user_id=$1)+(SELECT COUNT(*) FROM passkeys WHERE user_id=$1)+(SELECT COUNT(*) FROM api_keys WHERE user_id=$1)+(SELECT COUNT(*) FROM space_user_blocks WHERE user_id=$1)+(SELECT COUNT(*) FROM document_access_allowlist WHERE user_id=$1)+(SELECT COUNT(*) FROM document_draft_access WHERE user_id=$1)+(SELECT COUNT(*) FROM document_drafts WHERE owner_id=$1)+(SELECT COUNT(*) FROM doc_subscriptions WHERE user_id=$1)")
        .bind(prepared.user_id()).fetch_one(&mut *conn).await.map_err(AppError::internal)?;
    if actual != expected || closure != (1, 1, 0) {
        return Err(AppError::Conflict(
            "Prepared CSV account associations changed".into(),
        ));
    }
    require_newborn_owned_resources_empty(conn, prepared.user_id()).await?;
    verify_prepared_personal_space(conn, &prepared.personal).await
}

/// The registration-only path separately proves the complete initial session row.
async fn verify_prepared_account_sources(
    conn: &mut sqlx::AnyConnection,
    prepared: &PreparedAccount,
    expected_sessions: i64,
) -> Result<(), AppError> {
    if account_lifecycle_user(conn, prepared.user_id()).await? != Some(prepared.expected.clone()) {
        return Err(AppError::Conflict("Prepared account changed".into()));
    }
    let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT COUNT(*) FROM spaces WHERE created_by=$1),(SELECT COUNT(*) FROM space_members WHERE user_id=$1),(SELECT COUNT(*) FROM group_members WHERE user_id=$1),(SELECT COUNT(*) FROM sessions WHERE user_id=$1)+(SELECT COUNT(*) FROM oauth_accounts WHERE user_id=$1)+(SELECT COUNT(*) FROM passkeys WHERE user_id=$1)+(SELECT COUNT(*) FROM api_keys WHERE user_id=$1)+(SELECT COUNT(*) FROM space_user_blocks WHERE user_id=$1)+(SELECT COUNT(*) FROM document_access_allowlist WHERE user_id=$1)+(SELECT COUNT(*) FROM document_draft_access WHERE user_id=$1)").bind(prepared.user_id()).fetch_one(&mut *conn).await.map_err(AppError::internal)?;
    if counts != (1, 1, 1, expected_sessions) {
        return Err(AppError::Conflict(
            "Prepared account associations changed".into(),
        ));
    }
    require_newborn_owned_resources_empty(conn, prepared.user_id()).await?;
    verify_prepared_personal_space(conn, &prepared.personal).await
}
pub(crate) async fn create_personal_space(
    state: &AppState,
    user_id: &str,
    display_name: &str,
) -> Result<(), AppError> {
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    let prepared = create_personal_space_on_connection(&mut tx, user_id, display_name).await?;
    verify_prepared_personal_space(&mut tx, &prepared).await?;
    tx.commit().await.map_err(AppError::internal)?;
    storage::init_space_dirs(&prepared.slug)
        .await
        .map_err(|e| {
            AppError::Internal(format!("Failed to create personal space directory: {}", e))
        })?;
    crate::audit::log_action(
        &state.db,
        Some(user_id),
        None,
        "space.personal_create",
        Some("space"),
        Some(&prepared.id),
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
    fn avatar_profile_value_accepts_urls_and_valid_image_data() {
        assert_eq!(
            normalize_avatar_url(Some(" https://example.com/avatar.jpg ")),
            Ok(Some("https://example.com/avatar.jpg".to_string()))
        );

        let jpeg = format!(
            "data:image/jpeg;base64,{}",
            STANDARD.encode([0xff, 0xd8, 0xff, 0xd9])
        );
        assert_eq!(normalize_avatar_url(Some(&jpeg)), Ok(Some(jpeg)));
    }

    #[test]
    fn avatar_profile_value_rejects_invalid_or_oversized_image_data() {
        assert_eq!(
            normalize_avatar_url(Some("data:image/svg+xml;base64,PHN2Zy8+")),
            Err("Profile picture must be a JPEG, PNG, or WebP image")
        );
        assert_eq!(
            normalize_avatar_url(Some("data:image/jpeg;base64,bm90LWEtanBlZw==")),
            Err("Profile picture data does not match its image type")
        );

        let oversized = format!(
            "data:image/jpeg;base64,{}",
            "A".repeat(MAX_AVATAR_DATA_URL_LENGTH)
        );
        assert_eq!(
            normalize_avatar_url(Some(&oversized)),
            Err("Profile picture must be smaller than 1 MB")
        );
    }

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

        sqlx::query("CREATE TABLE audience_mutation_fence(id INTEGER PRIMARY KEY NOT NULL CHECK(id=1))")
            .execute(&db).await.expect("create audience fence");
        sqlx::query("INSERT INTO audience_mutation_fence(id) VALUES(1)")
            .execute(&db).await.expect("seed audience fence");

        sqlx::query(
            "CREATE TABLE sessions (
                id TEXT PRIMARY KEY NOT NULL,
                user_id TEXT NOT NULL,
                refresh_token_hash TEXT NOT NULL,
                device_name TEXT,
                session_kind TEXT NOT NULL DEFAULT 'browser',
                client_platform TEXT NOT NULL DEFAULT 'web',
                ip_address TEXT,
                user_agent TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                last_seen_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                expires_at TEXT NOT NULL
            )",
        )
        .execute(&db)
        .await
        .expect("create sessions");

        sqlx::query(
            "CREATE TABLE users (account_kind TEXT NOT NULL DEFAULT 'member',
                email_otp_enabled INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                external_id TEXT,
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

        sqlx::query(
            "CREATE TABLE native_auth_handoffs (
                id TEXT PRIMARY KEY NOT NULL,
                code_challenge TEXT NOT NULL,
                user_id TEXT,
                code_hash TEXT,
                expires_at TEXT NOT NULL,
                consumed_at TEXT
            )",
        )
        .execute(&db)
        .await
        .expect("create native auth handoffs");

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
            ws_rooms: crate::sync::WsRooms::default(),
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
            "INSERT INTO sessions (id, user_id, refresh_token_hash, expires_at) VALUES ($1, $2, $3, $4)",
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
             VALUES ($1, $2, $3, $4, $5, $6, $7, 1, $8)",
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
        sqlx::query("UPDATE users SET recovery_codes = $1 WHERE id = $2")
            .bind(recovery_code_hashes)
            .bind(user_id)
            .execute(&state.db)
            .await
            .expect("set user recovery codes");
    }

    async fn recovery_code_count(state: &AppState, user_id: &str) -> usize {
        let row: (Option<String>,) =
            sqlx::query_as("SELECT recovery_codes FROM users WHERE id = $1")
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
             VALUES ($1, $2, $3, $4, $5, 0, $6, NULL)",
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
            "INSERT INTO users (id, email, password_hash, is_active, updated_at) VALUES ($1, $2, $3, 1, $4)",
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
                "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, used_at) VALUES ($1, $2, $3, $4, NULL)",
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
    async fn native_handoff_uses_dedicated_browser_page_and_waits_for_approval() {
        let state = logout_test_state().await;
        let verifier = "v".repeat(43);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));

        let start_response = native_handoff_start(
            State(state.clone()),
            Json(NativeHandoffStartRequest {
                code_challenge: challenge,
            }),
        )
        .await
        .expect("start native handoff")
        .into_response();
        let start_body = axum::body::to_bytes(start_response.into_body(), usize::MAX)
            .await
            .expect("read start response");
        let start: serde_json::Value =
            serde_json::from_slice(&start_body).expect("parse start response");
        let handoff_id = start["handoff_id"]
            .as_str()
            .expect("handoff id")
            .to_string();

        assert_eq!(
            start["authorization_url"],
            format!("http://localhost:3000/app-signin/?handoff_id={handoff_id}")
        );

        let pending_response = native_handoff_exchange(
            State(state),
            Json(NativeHandoffExchangeRequest {
                handoff_id,
                code_verifier: verifier,
                session: SessionMetadata::default(),
            }),
        )
        .await
        .expect("pending native handoff");

        assert_eq!(pending_response.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn native_handoff_exchanges_pkce_approval_once_without_callback_tokens() {
        let state = logout_test_state().await;
        insert_login_test_user(
            &state,
            "native-user",
            "eagle@example.com",
            "correct-password",
            None,
            false,
        )
        .await;
        let verifier = "s".repeat(43);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let handoff_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO native_auth_handoffs (id, code_challenge, expires_at) VALUES ($1, $2, $3)",
        )
        .bind(&handoff_id)
        .bind(challenge)
        .bind((chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339())
        .execute(&state.db)
        .await
        .expect("insert native handoff");

        insert_test_session_for_user(&state,"native-user","native-user-session").await;
        let approval_response = native_handoff_complete(
            State(state.clone()),
            test_auth_user("native-user", "eagle@example.com"),
            Json(NativeHandoffCompleteRequest {
                handoff_id: handoff_id.clone(),
            }),
        )
        .await
        .expect("approve native handoff")
        .into_response();
        let approval_body = axum::body::to_bytes(approval_response.into_body(), usize::MAX)
            .await
            .expect("read approval response");
        let approval: serde_json::Value =
            serde_json::from_slice(&approval_body).expect("parse approval response");
        assert_eq!(approval["status"], "approved");
        assert!(approval.get("code").is_none());

        let exchange_response = native_handoff_exchange(
            State(state.clone()),
            Json(NativeHandoffExchangeRequest {
                handoff_id: handoff_id.clone(),
                code_verifier: verifier.clone(),
                session: SessionMetadata {
                    session_kind: Some("app".to_string()),
                    device_name: Some("Eagle's iPhone".to_string()),
                    client_platform: Some("ios_app".to_string()),
                },
            }),
        )
        .await
        .expect("exchange approved handoff");
        assert_eq!(exchange_response.status(), StatusCode::OK);
        let exchange_body = axum::body::to_bytes(exchange_response.into_body(), usize::MAX)
            .await
            .expect("read exchange response");
        let exchange: serde_json::Value =
            serde_json::from_slice(&exchange_body).expect("parse exchange response");
        assert_eq!(exchange["user"]["email"], "eagle@example.com");
        assert!(exchange["access_token"]
            .as_str()
            .is_some_and(|token| !token.is_empty()));
        assert!(exchange["refresh_token"]
            .as_str()
            .is_some_and(|token| !token.is_empty()));

        let replay = native_handoff_exchange(
            State(state),
            Json(NativeHandoffExchangeRequest {
                handoff_id,
                code_verifier: verifier,
                session: SessionMetadata::default(),
            }),
        )
        .await;
        assert!(matches!(replay, Err(AppError::Unauthorized(_))));
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
        let state = crate::spaces::routes::space_creation_fence_tests::setup(false).await;
        sqlx::query(
            "INSERT INTO users (id, email, display_name, role, password_hash, is_active, updated_at) \
             VALUES ('existing-user', 'existing@example.com', 'Existing User', 'admin', 'hash', 1, $1)",
        )
        .bind(chrono::Utc::now().to_rfc3339())
        .execute(&state.db)
        .await
        .expect("insert existing user");

        let before = crate::spaces::documents::tests::move_database_snapshot(&state).await;
        let result = register(
            State(state.clone()),
            Json(RegisterRequest {
                email: "new@example.com".to_string(),
                display_name: "New User".to_string(),
                password: "correct-password".to_string(),
                session: SessionMetadata::default(),
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
        assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&state).await,before);
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

        let current_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE id = $1")
            .bind("session-current")
            .fetch_one(&state.db)
            .await
            .expect("count current session");
        let other_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE id = $1")
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
            take_passkey_challenge(&state, &challenge_id, "registration",None)
                .await
                .expect("take passkey challenge");

        assert_eq!(user_id.as_deref(), Some("user-1"));
        assert_eq!(email.as_deref(), Some("eagle@example.com"));
        assert_eq!(passkey_state["state"], "created");
        assert_eq!(count_passkey_challenges(&state).await, 0);

        let error =
            take_passkey_challenge::<serde_json::Value>(&state, &challenge_id, "registration",None)
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
            take_passkey_challenge::<serde_json::Value>(&state, &challenge_id, "authentication",None)
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
             VALUES ($1, $2, $3, $4, $5, $6)",
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
            "registration",None,
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
             VALUES ($1, $2, $3, $4, $5, $6)",
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
        insert_login_test_user(&state,"user-1","eagle@example.com","owned-test-password",None,false).await;
        insert_test_session_for_user(&state,"user-1","user-1-session").await;
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

        let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM passkeys WHERE id = $1")
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
                session: SessionMetadata::default(),
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
                session: SessionMetadata::default(),
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
                session: SessionMetadata::default(),
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
            sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id = $1")
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
                session: SessionMetadata::default(),
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
                session: SessionMetadata::default(),
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
            sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id = $1")
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
        insert_test_session_for_user(&state, "user-totp", "user-totp-session").await;

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
            "SELECT totp_enabled, totp_secret, recovery_codes FROM users WHERE id = $1",
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
        insert_test_session_for_user(&state, "user-totp", "user-totp-session").await;
        sqlx::query(
            "INSERT INTO system_config (key, value, updated_at) VALUES ('require_2fa', 'true', $1)",
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
            "SELECT totp_enabled, totp_secret, recovery_codes FROM users WHERE id = $1",
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
            "INSERT INTO sessions (id, user_id, refresh_token_hash, expires_at) VALUES ($1, $2, $3, $4)",
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
            sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id = $1")
                .bind("user-1")
                .fetch_one(&state.db)
                .await
                .expect("count reset user sessions");
        let other_session_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id = $1")
                .bind("user-2")
                .fetch_one(&state.db)
                .await
                .expect("count other user sessions");
        let active_reset_count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM password_reset_tokens WHERE user_id = $1 AND used_at IS NULL",
        )
        .bind("user-1")
        .fetch_one(&state.db)
        .await
        .expect("count active reset tokens");

        assert_eq!(user_session_count.0, 0);
        assert_eq!(other_session_count.0, 1);
        assert_eq!(active_reset_count.0, 0);
    }

    async fn admin_reset_atomic_fixture() -> (AppState, AdminUser) {
        let state = logout_test_state().await;
        insert_login_test_user(&state, "admin-1", "admin@example.com", "owned-admin-password", None, false).await;
        insert_test_session_for_user(&state, "admin-1", "admin-session").await;
        insert_login_test_user(&state, "reset-target", "target@example.com", "original-target-password", None, false).await;
        insert_test_session_for_user(&state, "reset-target", "target-session").await;
        let now = chrono::Utc::now();
        let admin = AdminUser { api_key_scopes: None, claims: jwt::Claims {
            sub: "admin-1".into(), sid: "admin-session".into(), email: "admin@example.com".into(),
            role: "admin".into(), exp: (now + chrono::Duration::minutes(15)).timestamp(),
            iat: now.timestamp(), token_type: jwt::TokenType::Access,
        }};
        (state, admin)
    }

    #[tokio::test]
    async fn admin_reset_delete_failure_rolls_back_password_and_sessions() {
        let (state, admin) = admin_reset_atomic_fixture().await;
        let before = account_lifecycle_user(&mut state.db.acquire().await.unwrap(), "reset-target").await.unwrap();
        sqlx::query("CREATE TRIGGER reset_delete_failure BEFORE DELETE ON sessions WHEN OLD.user_id='reset-target' BEGIN SELECT RAISE(ABORT,'owned reset failure'); END")
            .execute(&state.db).await.unwrap();
        assert!(reset_admin_user_password(State(state.clone()), admin, Path("reset-target".into())).await.is_err());
        let after = account_lifecycle_user(&mut state.db.acquire().await.unwrap(), "reset-target").await.unwrap();
        assert!(before == after);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id=$1").bind("reset-target").fetch_one(&state.db).await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn admin_reset_self_revocation_commits_without_reauthorizing_deleted_session() {
        let (state, admin) = admin_reset_atomic_fixture().await;
        let response = reset_admin_user_password(State(state.clone()), admin, Path("admin-1".into())).await.unwrap().into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
        let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1").bind("admin-1").fetch_one(&state.db).await.unwrap();
        assert!(password::verify_password(body["temporary_password"].as_str().unwrap(), &hash).unwrap());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id=$1").bind("admin-1").fetch_one(&state.db).await.unwrap();
        assert_eq!(count, 0);
        let other: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id=$1").bind("reset-target").fetch_one(&state.db).await.unwrap();
        assert_eq!(other, 1);
    }

    #[tokio::test]
    async fn admin_reset_stale_authority_preserves_target() {
        let (state, admin) = admin_reset_atomic_fixture().await;
        sqlx::query("DELETE FROM sessions WHERE id=$1").bind("admin-session").execute(&state.db).await.unwrap();
        let before = account_lifecycle_user(&mut state.db.acquire().await.unwrap(), "reset-target").await.unwrap();
        assert!(reset_admin_user_password(State(state.clone()), admin, Path("reset-target".into())).await.is_err());
        assert!(before == account_lifecycle_user(&mut state.db.acquire().await.unwrap(), "reset-target").await.unwrap());
    }

    #[tokio::test]
    async fn admin_reset_changed_readback_rolls_back_password() {
        let (state, admin) = admin_reset_atomic_fixture().await;
        let before = account_lifecycle_user(&mut state.db.acquire().await.unwrap(), "reset-target").await.unwrap();
        sqlx::query("CREATE TRIGGER reset_readback_drift AFTER UPDATE OF password_hash ON users WHEN NEW.id='reset-target' BEGIN UPDATE users SET display_name='unexpected drift' WHERE id=NEW.id; END").execute(&state.db).await.unwrap();
        assert!(reset_admin_user_password(State(state.clone()), admin, Path("reset-target".into())).await.is_err());
        assert!(before == account_lifecycle_user(&mut state.db.acquire().await.unwrap(), "reset-target").await.unwrap());
    }

    #[tokio::test]
    async fn admin_password_reset_revokes_target_user_sessions() {
        let state = logout_test_state().await;
        insert_login_test_user(&state, "admin-1", "admin@example.com", "owned-admin-password", None, false).await;
        insert_test_session_for_user(&state, "admin-1", "admin-session").await;
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
            api_key_scopes: None,
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
            sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id = $1")
                .bind("user-1")
                .fetch_one(&state.db)
                .await
                .expect("count target sessions");
        let other_session_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM sessions WHERE user_id = $1")
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

    #[test]
    fn session_policy_uses_browser_and_app_inactivity_windows() {
        assert_eq!(normalized_session_kind(Some("app")), "app");
        assert_eq!(normalized_session_kind(Some("unexpected")), "browser");
        assert_eq!(
            normalized_client_platform(Some("mobile_app"), "app"),
            "mobile_app"
        );
        assert_eq!(
            normalized_client_platform(Some("desktop_app"), "app"),
            "desktop_app"
        );
        assert_eq!(
            normalized_client_platform(Some("mobile_app"), "browser"),
            "web"
        );
        assert_eq!(session_inactivity_days("browser"), 30);
        assert_eq!(session_inactivity_days("app"), 180);
        assert_eq!(
            normalized_device_name(Some("  Eagle's iPhone  ")).expect("valid name"),
            Some("Eagle's iPhone".to_string())
        );
        assert!(normalized_device_name(Some(&"x".repeat(81))).is_err());
    }

    #[tokio::test]
    async fn new_session_names_are_disambiguated_for_each_account() {
        let state = logout_test_state().await;
        insert_test_session_for_user(&state, "user-1", "existing-session").await;
        sqlx::query("UPDATE sessions SET device_name = $1 WHERE id = $2")
            .bind("Chrome on desktop")
            .bind("existing-session")
            .execute(&state.db)
            .await
            .expect("name existing session");

        assert_eq!(
            unique_session_name(&state, "user-1", "Chrome on desktop")
                .await
                .expect("unique session name"),
            "Chrome on desktop (2)"
        );
        assert_eq!(
            unique_session_name(&state, "another-user", "Chrome on desktop")
                .await
                .expect("name is scoped to account"),
            "Chrome on desktop"
        );
    }
}

#[cfg(test)]
mod space_creation_bootstrap_tests {
    use super::*;
    use axum::response::IntoResponse;
    #[tokio::test]
    async fn space_creation_actual_first_user_register_keeps_sessionless_bootstrap() {
        let mut state = crate::spaces::routes::catalog_tests::catalog_state(false).await;
        state.db = sqlx::any::AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
        let display = format!("Bootstrap {}", Uuid::new_v4());
        let response = register(
            State(state.clone()),
            Json(RegisterRequest {
                email: "bootstrap-space@example.test".into(),
                display_name: display,
                password: "correct-password".into(),
                session: SessionMetadata::default(),
            }),
        )
        .await
        .unwrap()
        .into_response();
        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["user"]["role"], "admin");
        let user = body["user"]["id"].as_str().unwrap();
        let (space, slug): (String, String) =
            sqlx::query_as("SELECT id,slug FROM spaces WHERE created_by=$1")
                .bind(user)
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT role FROM space_members WHERE space_id=$1 AND user_id=$2"
            )
            .bind(&space)
            .bind(user)
            .fetch_one(&state.db)
            .await
            .unwrap(),
            "owner"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions WHERE user_id=$1")
                .bind(user)
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM group_global_roles")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            0
        );
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM space_group_permissions g JOIN groups m ON m.id=g.group_id WHERE m.managed_space_id=$1 AND g.space_id=$1 AND g.role='owner'").bind(&space).fetch_one(&state.db).await.unwrap(),1);
        tokio::fs::remove_dir_all(storage::space_dir(&slug))
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod account_lifecycle_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use axum::extract::FromRequestParts;
    use tower::ServiceExt;
    async fn setup(wal: bool) -> AppState {
        let s = crate::spaces::routes::space_creation_fence_tests::setup(wal).await;
        for sql in ["UPDATE users SET role='admin' WHERE id='editor-a'","INSERT INTO users(id,email,display_name,role,is_active) VALUES('legacy-viewer','legacy@test','Legacy','viewer',0)"]{sqlx::query(sql).execute(&s.db).await.unwrap();}
        s
    }
    async fn proof(s: &AppState, user: &str, key: Option<&str>, expiry: Option<i64>) -> AdminUser {
        let mut req = axum::http::Request::builder().method("PUT");
        if let Some(key) = key {
            req = req.header("X-API-Key", key);
        } else {
            let mut claims = crate::spaces::routes::catalog_tests::auth(user).claims;
            claims.role = sqlx::query_scalar("SELECT role FROM users WHERE id=$1")
                .bind(user)
                .fetch_one(&s.db)
                .await
                .unwrap();
            if let Some(expiry) = expiry {
                claims.exp = expiry;
            }
            let token = jsonwebtoken::encode(
                &jsonwebtoken::Header::default(),
                &claims,
                &jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes()),
            )
            .unwrap();
            req = req.header("Authorization", format!("Bearer {token}"));
        }
        let (mut parts, _) = req.body(()).unwrap().into_parts();
        AdminUser::from_request_parts(&mut parts, s).await.unwrap()
    }
    async fn key(s: &AppState, id: &str, service: bool, scopes: &str) {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES($1,$2,$1,$3,'test',$4,$5)").bind(id).bind((!service).then_some("owner-a")).bind(crate::auth::api_keys::hash_api_key(id)).bind(i64::from(service)).bind(scopes).execute(&s.db).await.unwrap();
    }
    fn single<'a>(id: &'a str, req: &'a UserLifecycleRequest) -> AccountLifecycleChange<'a> {
        AccountLifecycleChange::Single {
            target: id,
            request: req,
        }
    }
    fn bulk(action: &str, ids: &[&str]) -> AccountLifecycleChange<'static> {
        AccountLifecycleChange::Bulk(
            normalize_bulk_lifecycle(BulkUserLifecycleRequest {
                action: action.into(),
                user_ids: ids.iter().map(|id| (*id).into()).collect(),
            })
            .unwrap(),
        )
    }
    #[tokio::test]
    async fn account_lifecycle_single_preserves_role_activity_fields_noop_and_audit() {
        let s = setup(false).await;
        let p = proof(&s, "owner-a", None, None).await;
        let old = account_lifecycle_user(&mut *s.db.acquire().await.unwrap(), "viewer-a")
            .await
            .unwrap()
            .unwrap();
        let req = UserLifecycleRequest {
            role: Some(" admin ".into()),
            is_active: Some(false),
        };
        mutate_account_lifecycle(&s, &p, single("viewer-a", &req))
            .await
            .unwrap();
        let row = account_lifecycle_user(&mut *s.db.acquire().await.unwrap(), "viewer-a")
            .await
            .unwrap()
            .unwrap();
        let mut expected = old;
        expected.role = "admin".into();
        expected.is_active = 0;
        expected.updated_at = row.updated_at.clone();
        assert!(row == expected);
        let req = UserLifecycleRequest {
            role: None,
            is_active: Some(true),
        };
        mutate_account_lifecycle(&s, &p, single("viewer-a", &req))
            .await
            .unwrap();
        mutate_account_lifecycle(
            &s,
            &p,
            single(
                "viewer-a",
                &UserLifecycleRequest {
                    role: None,
                    is_active: None,
                },
            ),
        )
        .await
        .unwrap();
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE action='user.lifecycle_update' AND target_id='viewer-a' AND actor_kind='user' AND actor_id='owner-a' AND user_id='owner-a'").fetch_one(&s.db).await.unwrap();
        assert_eq!(count, 3);
        let before = move_database_snapshot(&s).await;
        assert!(matches!(
            mutate_account_lifecycle(
                &s,
                &p,
                single(
                    "legacy-viewer",
                    &UserLifecycleRequest {
                        role: None,
                        is_active: None
                    }
                )
            )
            .await,
            Err(AppError::BadRequest(_))
        ));
        assert_eq!(move_database_snapshot(&s).await, before);
    }
    #[tokio::test]
    async fn account_lifecycle_bulk_preserves_request_order_skip_reasons_and_inactive_targets() {
        let s = setup(false).await;
        let p = proof(&s, "owner-a", None, None).await;
        let response = mutate_account_lifecycle(
            &s,
            &p,
            bulk(
                " DeActivate ",
                &[
                    "viewer-a",
                    "viewer-a",
                    "missing",
                    " ",
                    "owner-a",
                    "legacy-viewer",
                    "editor-a",
                ],
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(response.action, "deactivate");
        assert_eq!(response.requested_count, 7);
        assert_eq!(response.updated_count, 2);
        assert_eq!(response.skipped_count, 5);
        assert_eq!(
            response
                .skipped
                .iter()
                .map(|r| (r.user_id.as_str(), r.reason.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("viewer-a", "duplicate_user_id"),
                ("", "empty_user_id"),
                ("missing", "user_not_found"),
                ("owner-a", "cannot_deactivate_self"),
                ("legacy-viewer", "already_inactive")
            ]
        );
        // Batch activation must retain activity-only UPDATE semantics.
        sqlx::query("CREATE TRIGGER reject_role_write BEFORE UPDATE OF role ON users BEGIN SELECT RAISE(ABORT,'unexpected role write'); END")
            .execute(&s.db).await.unwrap();
        let response = mutate_account_lifecycle(
            &s,
            &p,
            bulk(
                "activate",
                &["legacy-viewer", "viewer-a", "missing", "owner-a"],
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(response.updated_count, 2);
        assert_eq!(
            response
                .skipped
                .iter()
                .map(|r| r.reason.as_str())
                .collect::<Vec<_>>(),
            vec!["user_not_found", "already_active"]
        );
        let role: String = sqlx::query_scalar("SELECT role FROM users WHERE id='legacy-viewer'")
            .fetch_one(&s.db)
            .await
            .unwrap();
        assert_eq!(role, "viewer");
        assert!(normalize_bulk_lifecycle(BulkUserLifecycleRequest {
            action: "deactivate".into(),
            user_ids: vec![]
        })
        .is_err());
        assert!(normalize_bulk_lifecycle(BulkUserLifecycleRequest {
            action: "deactivate".into(),
            user_ids: vec![" ".into()]
        })
        .is_err());
        assert!(normalize_bulk_lifecycle(BulkUserLifecycleRequest {
            action: "unknown".into(),
            user_ids: vec!["viewer-a".into()]
        })
        .is_err());
        assert!(normalize_bulk_lifecycle(BulkUserLifecycleRequest {
            action: "activate".into(),
            user_ids: (0..101).map(|i| i.to_string()).collect()
        })
        .is_err());
    }
    #[tokio::test]
    async fn account_lifecycle_self_and_last_admin_bans_and_missing_status_remain() {
        let s = setup(false).await;
        let p = proof(&s, "owner-a", None, None).await;
        let before = move_database_snapshot(&s).await;
        for req in [
            UserLifecycleRequest {
                role: Some("member".into()),
                is_active: None,
            },
            UserLifecycleRequest {
                role: None,
                is_active: Some(false),
            },
            UserLifecycleRequest {
                role: Some("unknown".into()),
                is_active: None,
            },
        ] {
            assert!(matches!(
                mutate_account_lifecycle(&s, &p, single("owner-a", &req)).await,
                Err(AppError::BadRequest(_))
            ));
            assert_eq!(move_database_snapshot(&s).await, before);
        }
        assert!(matches!(
            mutate_account_lifecycle(
                &s,
                &p,
                single(
                    "missing",
                    &UserLifecycleRequest {
                        role: None,
                        is_active: None
                    }
                )
            )
            .await,
            Err(AppError::NotFound(_))
        ));
        assert_eq!(move_database_snapshot(&s).await, before);
        key(&s, "lifecycle-service", true, "[\"admin\"]").await;
        let service = proof(&s, "owner-a", Some("lifecycle-service"), None).await;
        let response =
            mutate_account_lifecycle(&s, &service, bulk("deactivate", &["editor-a", "owner-a"]))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(response.updated_count, 1);
        assert_eq!(response.skipped[0].user_id, "owner-a");
        assert_eq!(response.skipped[0].reason, "last_active_admin_required");
        let before = move_database_snapshot(&s).await;
        assert!(matches!(
            mutate_account_lifecycle(
                &s,
                &service,
                single(
                    "owner-a",
                    &UserLifecycleRequest {
                        role: Some("member".into()),
                        is_active: None
                    }
                )
            )
            .await,
            Err(AppError::BadRequest(_))
        ));
        assert_eq!(move_database_snapshot(&s).await, before);
    }
    #[tokio::test]
    async fn account_lifecycle_http_actual_human_service_and_scope_contracts() {
        for credential in ["human-admin", "service-admin", "read", "write"] {
            let s = setup(false).await;
            key(
                &s,
                credential,
                credential == "service-admin",
                match credential {
                    "read" => "[\"read\"]",
                    "write" => "[\"write\"]",
                    _ => "[\"admin\"]",
                },
            )
            .await;
            for (method, url, body) in [
                ("PUT", "/admin/users/viewer-a", "{\"is_active\":false}"),
                (
                    "POST",
                    "/admin/users/bulk-lifecycle",
                    "{\"action\":\"activate\",\"user_ids\":[\"viewer-a\"]}",
                ),
            ] {
                let response = router()
                    .with_state(s.clone())
                    .oneshot(
                        axum::http::Request::builder()
                            .method(method)
                            .uri(url)
                            .header("X-API-Key", credential)
                            .header("Content-Type", "application/json")
                            .body(axum::body::Body::from(body))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(
                    response.status(),
                    if credential == "read" || credential == "write" {
                        StatusCode::FORBIDDEN
                    } else if method == "PUT" {
                        StatusCode::NO_CONTENT
                    } else {
                        StatusCode::OK
                    }
                );
            }
            if credential == "service-admin" {
                let rows:Vec<(String,String,Option<String>)>=sqlx::query_as("SELECT actor_kind,actor_id,user_id FROM audit_log WHERE action IN ('user.lifecycle_update','user.lifecycle_bulk_update')").fetch_all(&s.db).await.unwrap();
                assert_eq!(rows.len(), 2);
                assert!(rows
                    .iter()
                    .all(|r| r.0 == "service_key" && r.1 == credential && r.2.is_none()));
            }
        }
    }
    #[tokio::test]
    async fn account_lifecycle_late_user_and_audit_changes_roll_back_whole_batch() {
        for batch in [false, true] {
            for trigger in [
            "CREATE TRIGGER late_user AFTER UPDATE OF is_active ON users WHEN NEW.id='viewer-a' BEGIN UPDATE users SET display_name='Injected profile' WHERE id='viewer-a'; END",
            "CREATE TRIGGER late_user AFTER UPDATE OF is_active ON users WHEN NEW.id='viewer-a' BEGIN DELETE FROM sessions WHERE user_id='owner-a'; END",
            "CREATE TRIGGER late_user AFTER UPDATE OF is_active ON users WHEN NEW.id='editor-a' BEGIN SELECT RAISE(ABORT,'second user failure'); END",
            "CREATE TRIGGER late_audit AFTER INSERT ON audit_log BEGIN SELECT RAISE(ABORT,'audit failure'); END",
            "CREATE TRIGGER late_audit AFTER INSERT ON audit_log BEGIN UPDATE audit_log SET before_value='Injected' WHERE id=NEW.id; END",
            "CREATE TRIGGER late_audit AFTER INSERT ON audit_log BEGIN UPDATE users SET role='member' WHERE id='owner-a'; END",
            "CREATE TRIGGER late_audit AFTER INSERT ON audit_log BEGIN INSERT INTO users(id,email,display_name,password_hash,role,is_active,created_at,updated_at) VALUES('injected-admin','injected@example.test','Injected','hash','admin',1,'2000','2000'); END",
        ]{
            if !batch&&trigger.contains("second user failure"){continue;}
            let s=setup(false).await;let p=proof(&s,"owner-a",None,None).await;sqlx::query(trigger).execute(&s.db).await.unwrap();let before=move_database_snapshot(&s).await;let req=UserLifecycleRequest{role:None,is_active:Some(false)};
            assert!(mutate_account_lifecycle(&s,&p,if batch{bulk("deactivate",&["viewer-a","editor-a"])}else{single("viewer-a",&req)}).await.is_err(),"{batch}: {trigger}");assert_eq!(move_database_snapshot(&s).await,before,"{batch}: {trigger}");
        }
        }
    }
    #[tokio::test]
    async fn account_lifecycle_true_wal_fence_wait_denies_current_session_account_and_service_key()
    {
        for batch in [false, true] {
            for mode in 0..3 {
                let s = setup(true).await;
                if mode == 2 {
                    key(&s, "lifecycle-wait", true, "[\"admin\"]").await;
                }
                let p = proof(&s, "owner-a", (mode == 2).then_some("lifecycle-wait"), None).await;
                let mut blocker = s.db.begin().await.unwrap();
                crate::spaces::mutation_guards::lock_audience_for_mutation(&mut blocker)
                    .await
                    .unwrap();
                sqlx::query(match mode {
                    0 => "DELETE FROM sessions WHERE user_id='owner-a'",
                    1 => "UPDATE users SET role='member' WHERE id='owner-a'",
                    _ => "UPDATE api_keys SET scopes='[\"read\"]' WHERE id='lifecycle-wait'",
                })
                .execute(&mut *blocker)
                .await
                .unwrap();
                let c = s.clone();
                let task = tokio::spawn(async move {
                    let req = UserLifecycleRequest {
                        role: None,
                        is_active: Some(false),
                    };
                    mutate_account_lifecycle(
                        &c,
                        &p,
                        if batch {
                            bulk("deactivate", &["viewer-a"])
                        } else {
                            single("viewer-a", &req)
                        },
                    )
                    .await
                    .map(|_| ())
                });
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                assert!(!task.is_finished());
                blocker.commit().await.unwrap();
                let before = move_database_snapshot(&s).await;
                assert!(task.await.unwrap().is_err());
                assert_eq!(move_database_snapshot(&s).await, before);
            }
        }
    }
    #[tokio::test]
    async fn account_lifecycle_competing_admins_cannot_demote_each_other_to_zero_admins() {
        let s = setup(true).await;
        let a = proof(&s, "owner-a", None, None).await;
        let b = proof(&s, "editor-a", None, None).await;
        let c = s.clone();
        let left = tokio::spawn(async move {
            let req = UserLifecycleRequest {
                role: Some("member".into()),
                is_active: None,
            };
            mutate_account_lifecycle(&c, &a, single("editor-a", &req))
                .await
                .map(|_| ())
        });
        let c = s.clone();
        let right = tokio::spawn(async move {
            let req = UserLifecycleRequest {
                role: Some("member".into()),
                is_active: None,
            };
            mutate_account_lifecycle(&c, &b, single("owner-a", &req))
                .await
                .map(|_| ())
        });
        let results = [left.await.unwrap(), right.await.unwrap()];
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role='admin' AND is_active=1")
                .fetch_one(&s.db)
                .await
                .unwrap();
        assert_eq!(count, 1);
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_log WHERE action='user.lifecycle_update'",
        )
        .fetch_one(&s.db)
        .await
        .unwrap();
        assert_eq!(count, 1);
    }
    #[tokio::test]
    async fn account_lifecycle_final_equality_reads_do_not_extend_original_jwt() {
        for batch in [false, true] {
            let s = setup(false).await;
            let expiry = chrono::Utc::now().timestamp() + 2;
            let p = proof(&s, "owner-a", None, Some(expiry)).await;
            let before = move_database_snapshot(&s).await;
            ACCOUNT_LIFECYCLE_FINAL_EXPIRY.with(|h| h.set(Some(expiry)));
            let req = UserLifecycleRequest {
                role: None,
                is_active: Some(false),
            };
            let result = mutate_account_lifecycle(
                &s,
                &p,
                if batch {
                    bulk("deactivate", &["viewer-a"])
                } else {
                    single("viewer-a", &req)
                },
            )
            .await;
            assert!(ACCOUNT_LIFECYCLE_FINAL_EXPIRY.with(|h| h.get()).is_none());
            assert!(matches!(result, Err(AppError::Unauthorized(_))));
            assert_eq!(move_database_snapshot(&s).await, before);
        }
    }
}

#[cfg(test)]
mod registration_atomic_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use serde_json::Value;
    use tower::ServiceExt;
    async fn setup(wal: bool, empty: bool) -> AppState {
        let s = crate::spaces::routes::space_creation_fence_tests::setup(wal).await;
        if empty {
            sqlx::query("DELETE FROM spaces")
                .execute(&s.db)
                .await
                .unwrap();
            sqlx::query("DELETE FROM users")
                .execute(&s.db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO system_config(key,value) VALUES('allow_public_signups','true') ON CONFLICT(key) DO UPDATE SET value='true'").execute(&s.db).await.unwrap();
        s
    }
    fn request(email: &str) -> RegisterRequest {
        RegisterRequest {
            email: email.into(),
            display_name: format!("Registrant {}", Uuid::new_v4()),
            password: "password-atomic-123".into(),
            session: SessionMetadata {
                session_kind: Some("app".into()),
                device_name: Some("  Actual device  ".into()),
                client_platform: Some("mobile_app".into()),
            },
        }
    }
    async fn invoke(
        s: AppState,
        req: RegisterRequest,
    ) -> Result<axum::response::Response, AppError> {
        register(State(s), Json(req))
            .await
            .map(IntoResponse::into_response)
    }
    async fn body(response: axum::response::Response) -> Value {
        serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap()
    }
    async fn clean(s: &AppState, user: &str) {
        let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE created_by=$1")
            .bind(user)
            .fetch_one(&s.db)
            .await
            .unwrap();
        let _ = tokio::fs::remove_dir_all(storage::space_dir(&slug)).await;
    }
    #[tokio::test]
    async fn registration_cheap_denials_preserve_status_and_never_hash_invalid_device() {
        for disabled in [true, false] {
            let s = setup(false, false).await;
            if disabled {
                sqlx::query(
                    "UPDATE system_config SET value='false' WHERE key='allow_public_signups'",
                )
                .execute(&s.db)
                .await
                .unwrap();
            }
            let mut req = request("denial@example.com");
            req.session.device_name = Some("x".repeat(81));
            if !disabled {
                req.email = sqlx::query_scalar("SELECT email FROM users WHERE id='owner-a'")
                    .fetch_one(&s.db)
                    .await
                    .unwrap();
            }
            let before = move_database_snapshot(&s).await;
            let hashes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let copy = hashes.clone();
            let result = REGISTRATION_HASH_HOOK
                .scope(
                    std::cell::RefCell::new(Some(Box::new(move || {
                        copy.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }))),
                    invoke(s.clone(), req),
                )
                .await;
            assert!(if disabled {
                matches!(result, Err(AppError::Forbidden(_)))
            } else {
                matches!(result, Err(AppError::Conflict(_)))
            });
            assert_eq!(hashes.load(std::sync::atomic::Ordering::SeqCst), 0);
            assert_eq!(move_database_snapshot(&s).await, before);
        }
    }
    #[tokio::test]
    async fn registration_actual_route_preserves_raw_fields_tokens_and_initial_session() {
        let s = setup(false, true).await;
        let email = " Mixed.Register@Example.COM ";
        let req = request(email);
        let name = req.display_name.clone();
        let http=axum::http::Request::builder().method("POST").uri("/auth/register").extension(axum::extract::ConnectInfo("127.0.0.1:5199".parse::<std::net::SocketAddr>().unwrap())).header("content-type","application/json").body(axum::body::Body::from(serde_json::json!({"email":email,"display_name":name,"password":req.password,"session_kind":"app","device_name":"  Actual device  ","client_platform":"mobile_app"}).to_string())).unwrap();
        let response = router().with_state(s.clone()).oneshot(http).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let v = body(response).await;
        assert_eq!(v["user"]["email"], email);
        assert_eq!(v["user"]["role"], "admin");
        let user = v["user"]["id"].as_str().unwrap();
        let claims = jwt::validate_token(
            v["access_token"].as_str().unwrap(),
            &s.jwt_secret,
            jwt::TokenType::Access,
        )
        .unwrap();
        assert_eq!(claims.sub, user);
        let stored = initial_session_row(&mut s.db.acquire().await.unwrap(), user)
            .await
            .unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].0, claims.sid);
        assert_eq!(
            stored[0].2,
            hash_refresh_token(v["refresh_token"].as_str().unwrap())
        );
        assert_eq!(stored[0].3.as_deref(), Some("Actual device"));
        assert_eq!(
            (&stored[0].9, &stored[0].10),
            (&"app".to_owned(), &"mobile_app".to_owned())
        );
        let audits: Vec<(Option<String>, Option<String>, Option<String>, String)> = sqlx::query_as(
            "SELECT user_id,actor_kind,actor_id,action FROM audit_log WHERE user_id=$1",
        )
        .bind(user)
        .fetch_all(&s.db)
        .await
        .unwrap();
        assert_eq!(
            audits,
            vec![(
                Some(user.into()),
                Some("user".into()),
                Some(user.into()),
                "space.personal_create".into()
            )]
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT value FROM system_config WHERE key='setup_completed'"
            )
            .fetch_one(&s.db)
            .await
            .unwrap(),
            "true"
        );
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM group_global_roles g JOIN groups x ON x.id=g.group_id JOIN spaces s ON s.id=x.managed_space_id WHERE s.created_by=$1").bind(user).fetch_one(&s.db).await.unwrap(),0);
        clean(&s, user).await;
    }
    #[tokio::test]
    async fn registration_each_constituent_session_setup_audit_failure_is_atomic() {
        for table in [
            "users",
            "spaces",
            "space_members",
            "groups",
            "group_members",
            "space_group_permissions",
            "sessions",
            "audit_log",
        ] {
            let s = setup(false, true).await;
            sqlx::query(&format!("CREATE TRIGGER registration_fail AFTER INSERT ON {table} BEGIN SELECT RAISE(ABORT,'registration constituent failed'); END")).execute(&s.db).await.unwrap();
            let before = move_database_snapshot(&s).await;
            assert!(
                invoke(s.clone(), request("fail@example.com"))
                    .await
                    .is_err(),
                "{table}"
            );
            assert_eq!(move_database_snapshot(&s).await, before, "{table}");
        }
        let s = setup(false, true).await;
        sqlx::query("CREATE TRIGGER registration_setup_fail AFTER UPDATE ON system_config WHEN NEW.key='setup_completed' BEGIN SELECT RAISE(ABORT,'setup failed'); END").execute(&s.db).await.unwrap();
        let before = move_database_snapshot(&s).await;
        assert!(invoke(s.clone(), request("setup@example.com"))
            .await
            .is_err());
        assert_eq!(move_database_snapshot(&s).await, before);
    }
    #[tokio::test]
    async fn registration_late_source_session_audit_policy_drift_rolls_back() {
        for sql in ["CREATE TRIGGER registration_drift AFTER INSERT ON sessions BEGIN UPDATE users SET role='admin' WHERE id=NEW.user_id; END","CREATE TRIGGER registration_drift AFTER INSERT ON audit_log BEGIN UPDATE sessions SET client_platform='tampered'; END","CREATE TRIGGER registration_drift AFTER INSERT ON audit_log BEGIN UPDATE audit_log SET before_value='tampered' WHERE id=NEW.id; END","CREATE TRIGGER registration_drift AFTER INSERT ON audit_log BEGIN UPDATE system_config SET value='false' WHERE key='allow_public_signups'; END","CREATE TRIGGER registration_drift AFTER INSERT ON audit_log BEGIN UPDATE system_config SET value='changed' WHERE key='setup_completed'; END"]{let s=setup(false,false).await;sqlx::query(sql).execute(&s.db).await.unwrap();let before=move_database_snapshot(&s).await;assert!(invoke(s.clone(),request("drift@example.com")).await.is_err(),"{sql}");assert_eq!(move_database_snapshot(&s).await,before,"{sql}");}
    }
    #[tokio::test]
    async fn registration_validation_duplicate_disabled_statuses_leave_database_unchanged() {
        let s = setup(false, false).await;
        for mode in 0..4 {
            let mut req = request("owner-a@example.test");
            match mode {
                0 => req.password = "short".into(),
                1 => req.session.device_name = Some("x".repeat(81)),
                2 => {
                    req.email = sqlx::query_scalar("SELECT email FROM users WHERE id='owner-a'")
                        .fetch_one(&s.db)
                        .await
                        .unwrap()
                }
                _ => {
                    sqlx::query(
                        "UPDATE system_config SET value='false' WHERE key='allow_public_signups'",
                    )
                    .execute(&s.db)
                    .await
                    .unwrap();
                }
            }
            let before = move_database_snapshot(&s).await;
            let error = invoke(s.clone(), req).await.unwrap_err();
            assert!(match mode {
                0 | 1 => matches!(error, AppError::BadRequest(_)),
                2 => matches!(error, AppError::Conflict(_)),
                _ => matches!(error, AppError::Forbidden(_)),
            });
            assert_eq!(move_database_snapshot(&s).await, before);
        }
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn registration_true_wal_two_first_requests_select_one_admin() {
        let s = setup(true, true).await;
        let entered = std::sync::Arc::new(tokio::sync::Notify::new());
        let release = std::sync::Arc::new(tokio::sync::Notify::new());
        let (a, b) = (entered.clone(), release.clone());
        let copy = s.clone();
        let first = tokio::spawn(async move {
            REGISTRATION_FENCE_HOOK
                .scope(
                    std::cell::RefCell::new(Some(Box::pin(async move {
                        a.notify_one();
                        b.notified().await;
                    }))),
                    invoke(copy, request("first@example.com")),
                )
                .await
        });
        entered.notified().await;
        let copy = s.clone();
        let second = tokio::spawn(async move { invoke(copy, request("second@example.com")).await });
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(!second.is_finished());
        release.notify_one();
        let a = body(first.await.unwrap().unwrap()).await;
        let b = body(second.await.unwrap().unwrap()).await;
        assert_eq!(a["user"]["role"], "admin");
        assert_eq!(b["user"]["role"], "member");
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE role='admin'")
                .fetch_one(&s.db)
                .await
                .unwrap(),
            1
        );
        clean(&s, a["user"]["id"].as_str().unwrap()).await;
        clean(&s, b["user"]["id"].as_str().unwrap()).await;
    }
    #[tokio::test]
    async fn registration_true_wal_wait_rechecks_current_signup_policy() {
        let s = setup(true, false).await;
        let mut blocker = s.db.begin().await.unwrap();
        crate::spaces::mutation_guards::lock_audience_for_mutation(&mut blocker)
            .await
            .unwrap();
        let copy = s.clone();
        let pending = tokio::spawn(async move { invoke(copy, request("wait@example.com")).await });
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(!pending.is_finished());
        sqlx::query("UPDATE system_config SET value='false' WHERE key='allow_public_signups'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        blocker.commit().await.unwrap();
        let before = move_database_snapshot(&s).await;
        assert!(matches!(
            pending.await.unwrap(),
            Err(AppError::Forbidden(_))
        ));
        assert_eq!(move_database_snapshot(&s).await, before);
    }
    #[test]
    fn registration_final_environment_policy_rotation_is_isolated() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "auth::routes::registration_atomic_tests::registration_environment_child",
                "--ignored",
                "--test-threads=1",
            ])
            .env("STARLANE_REGISTRATION_CHILD", "1")
            .env("ALLOW_PUBLIC_SIGNUPS", "true")
            .status()
            .unwrap();
        assert!(status.success());
    }
    #[tokio::test]
    #[ignore]
    async fn registration_environment_child() {
        if std::env::var("STARLANE_REGISTRATION_CHILD").ok().as_deref() != Some("1") {
            return;
        }
        let s = setup(false, false).await;
        let before = move_database_snapshot(&s).await;
        let result = REGISTRATION_FINAL_HOOK
            .scope(
                std::cell::RefCell::new(Some(Box::new(|| {
                    std::env::set_var("ALLOW_PUBLIC_SIGNUPS", "false")
                }))),
                invoke(s.clone(), request("env@example.com")),
            )
            .await;
        assert!(matches!(result, Err(AppError::Forbidden(_))));
        assert_eq!(move_database_snapshot(&s).await, before);
    }
}

#[cfg(test)]
mod admin_account_atomic_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use axum::extract::FromRequestParts;
    use tower::ServiceExt;
    async fn setup(wal: bool) -> AppState {
        crate::spaces::routes::space_creation_fence_tests::setup(wal).await
    }
    fn req() -> AdminCreateUserRequest {
        AdminCreateUserRequest {
            email: format!(" Born.{}@Example.COM ", Uuid::new_v4()),
            display_name: format!("  Birth {}  ", Uuid::new_v4()),
            role: None,
        }
    }
    async fn admin(s: &AppState) -> AdminUser {
        crate::spaces::routes::space_creation_fence_tests::admin(s).await
    }
    async fn invoke(
        s: AppState,
        p: AdminUser,
        r: AdminCreateUserRequest,
    ) -> Result<axum::response::Response, AppError> {
        create_admin_user(State(s), p, Json(r))
            .await
            .map(IntoResponse::into_response)
    }
    async fn body(r: axum::response::Response) -> serde_json::Value {
        serde_json::from_slice(
            &axum::body::to_bytes(r.into_body(), 1024 * 1024)
                .await
                .unwrap(),
        )
        .unwrap()
    }
    async fn key(s: &AppState, id: &str, service: bool, scopes: &str) {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES($1,$2,$1,$3,$1,$4,$5)").bind(id).bind(if service{None}else{Some("owner-a")}).bind(crate::auth::api_keys::hash_api_key(id)).bind(if service{1}else{0}).bind(scopes).execute(&s.db).await.unwrap();
    }
    async fn key_proof(s: &AppState, id: &str) -> AdminUser {
        let r = axum::http::Request::builder()
            .method("POST")
            .header("X-API-Key", id)
            .body(())
            .unwrap();
        let (mut parts, _) = r.into_parts();
        AdminUser::from_request_parts(&mut parts, s).await.unwrap()
    }
    async fn cleanup(s: &AppState, user: &str) {
        let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE created_by=$1")
            .bind(user)
            .fetch_one(&s.db)
            .await
            .unwrap();
        let _ = tokio::fs::remove_dir_all(storage::space_dir(&slug)).await;
    }
    #[tokio::test]
    async fn admin_birth_actual_http_keys_preserve_response_and_distinct_audits() {
        for service in [false, true] {
            let s = setup(false).await;
            let k = if service {
                "birth-service"
            } else {
                "birth-human"
            };
            key(&s, k, service, "[\"admin\"]").await;
            let mut r = req();
            if service {
                r.role = Some("admin".into());
            }
            let expected_role = if service { "admin" } else { "member" };
            let expected_email = r.email.trim().to_lowercase();
            let expected_name = r.display_name.trim().to_string();
            let response = router()
                .with_state(s.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri("/admin/users")
                        .header("X-API-Key", k)
                        .header("Content-Type", "application/json")
                        .body(axum::body::Body::from(
                            json!({"email":r.email,"display_name":r.display_name,"role":r.role})
                                .to_string(),
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            let v = body(response).await;
            let id = v["id"].as_str().unwrap();
            assert_eq!(v["email"], expected_email);
            assert_eq!(v["display_name"], expected_name);
            assert_eq!(v["role"], expected_role);
            assert_eq!(v["is_active"], true);
            let temp = v["temporary_password"].as_str().unwrap();
            assert_eq!(temp.len(), 20);
            let hash: String = sqlx::query_scalar("SELECT password_hash FROM users WHERE id=$1")
                .bind(id)
                .fetch_one(&s.db)
                .await
                .unwrap();
            assert!(password::verify_password(temp, &hash).unwrap());
            let sessions: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE user_id=$1")
                    .bind(id)
                    .fetch_one(&s.db)
                    .await
                    .unwrap();
            assert_eq!(sessions, 0);
            let topology:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT COUNT(*) FROM spaces WHERE created_by=$1),(SELECT COUNT(*) FROM space_members WHERE user_id=$1),(SELECT COUNT(*) FROM group_members WHERE user_id=$1),(SELECT COUNT(*) FROM group_global_roles gr JOIN group_members gm ON gm.group_id=gr.group_id WHERE gm.user_id=$1)").bind(id).fetch_one(&s.db).await.unwrap();
            assert_eq!(topology, (1, 1, 1, 0));
            let event:(Option<String>,Option<String>,Option<String>)=sqlx::query_as("SELECT user_id,actor_kind,actor_id FROM audit_log WHERE action='user.create' AND target_id=$1").bind(id).fetch_one(&s.db).await.unwrap();
            assert_eq!(
                event,
                if service {
                    (None, Some("service_key".into()), Some(k.into()))
                } else {
                    (
                        Some("owner-a".into()),
                        Some("user".into()),
                        Some("owner-a".into()),
                    )
                }
            );
            let personal:(String,String,String)=sqlx::query_as("SELECT user_id,actor_kind,actor_id FROM audit_log WHERE action='space.personal_create' AND user_id=$1").bind(id).fetch_one(&s.db).await.unwrap();
            assert_eq!(personal, (id.into(), "user".into(), id.into()));
            cleanup(&s, id).await;
        }
    }
    #[tokio::test]
    async fn admin_birth_http_nonadmin_keys_denied_no_account() {
        for scopes in ["[\"read\"]", "[\"write\"]"] {
            let s = setup(false).await;
            key(&s, "denied-birth", false, scopes).await;
            let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
                .fetch_one(&s.db)
                .await
                .unwrap();
            let response = router()
                .with_state(s.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .method("POST")
                        .uri("/admin/users")
                        .header("X-API-Key", "denied-birth")
                        .header("Content-Type", "application/json")
                        .body(axum::body::Body::from(
                            "{\"email\":\"deny@example.com\",\"display_name\":\"Denied\"}",
                        ))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
                    .fetch_one(&s.db)
                    .await
                    .unwrap(),
                before
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_log")
                    .fetch_one(&s.db)
                    .await
                    .unwrap(),
                0
            );
        }
    }
    #[tokio::test]
    async fn admin_birth_cheap_duplicate_policy_and_current_denials_never_hash() {
        for mode in 0..3 {
            let s = setup(false).await;
            let p = admin(&s).await;
            let mut r = req();
            if mode == 0 {
                r.email = sqlx::query_scalar("SELECT email FROM users WHERE id='owner-a'")
                    .fetch_one(&s.db)
                    .await
                    .unwrap();
            }
            if mode == 1 {
                sqlx::query("INSERT INTO system_config(key,value) VALUES('user_provisioning_allowed_email_domains','other.example')").execute(&s.db).await.unwrap();
            }
            if mode == 2 {
                sqlx::query("DELETE FROM sessions WHERE user_id='owner-a'")
                    .execute(&s.db)
                    .await
                    .unwrap();
            }
            let before = move_database_snapshot(&s).await;
            let hashes = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let c = hashes.clone();
            let result = ADMIN_BIRTH_HASH_HOOK
                .scope(
                    std::cell::RefCell::new(Some(Box::new(move || {
                        c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }))),
                    invoke(s.clone(), p, r),
                )
                .await;
            assert!(result.is_err());
            assert_eq!(hashes.load(std::sync::atomic::Ordering::SeqCst), 0);
            assert_eq!(move_database_snapshot(&s).await, before);
        }
    }
    #[tokio::test]
    async fn admin_birth_each_constituent_and_both_audits_fail_atomically() {
        for table in [
            "users",
            "spaces",
            "space_members",
            "groups",
            "group_members",
            "space_group_permissions",
            "audit_log",
        ] {
            let s = setup(false).await;
            let p = admin(&s).await;
            sqlx::query(&format!("CREATE TRIGGER fail_birth AFTER INSERT ON {table} BEGIN SELECT RAISE(ABORT,'birth failure'); END")).execute(&s.db).await.unwrap();
            let before = move_database_snapshot(&s).await;
            assert!(invoke(s.clone(), p, req()).await.is_err(), "{table}");
            assert_eq!(move_database_snapshot(&s).await, before, "{table}");
        }
        let s = setup(false).await;
        let p = admin(&s).await;
        sqlx::query("CREATE TRIGGER fail_second_audit AFTER INSERT ON audit_log WHEN NEW.action='user.create' BEGIN SELECT RAISE(ABORT,'second audit failure'); END").execute(&s.db).await.unwrap();
        let before = move_database_snapshot(&s).await;
        assert!(invoke(s.clone(), p, req()).await.is_err());
        assert_eq!(move_database_snapshot(&s).await, before);
    }
    #[tokio::test]
    async fn admin_birth_late_source_policy_credential_and_audit_drift_roll_back() {
        for sql in [
 "CREATE TRIGGER drift AFTER INSERT ON audit_log WHEN NEW.action='user.create' BEGIN UPDATE users SET role='admin' WHERE id=NEW.target_id; END",
 "CREATE TRIGGER drift AFTER INSERT ON audit_log WHEN NEW.action='user.create' BEGIN INSERT INTO space_members(id,space_id,user_id,role) SELECT 'phantom-birth',id,NEW.target_id,'owner' FROM spaces WHERE id='space-a'; END",
 "CREATE TRIGGER drift AFTER INSERT ON audit_log WHEN NEW.action='user.create' BEGIN UPDATE system_config SET value='other.example' WHERE key='user_provisioning_allowed_email_domains'; END",
 "CREATE TRIGGER drift AFTER INSERT ON audit_log WHEN NEW.action='user.create' BEGIN UPDATE system_config SET value='admin' WHERE key='user_provisioning_allowed_roles'; END",
 "CREATE TRIGGER drift AFTER INSERT ON audit_log WHEN NEW.action='user.create' BEGIN DELETE FROM sessions WHERE user_id='owner-a'; END",
 "CREATE TRIGGER drift AFTER INSERT ON audit_log WHEN NEW.action='user.create' BEGIN UPDATE audit_log SET actor_id='not-actor' WHERE id=NEW.id; END",
 "CREATE TRIGGER drift AFTER INSERT ON audit_log WHEN NEW.action='user.create' BEGIN UPDATE spaces SET slug='changed-source' WHERE created_by=NEW.target_id; END"
 ]{let s=setup(false).await;let p=admin(&s).await;sqlx::query("INSERT INTO system_config(key,value) VALUES('user_provisioning_allowed_email_domains','example.com'),('user_provisioning_allowed_roles','member,admin')").execute(&s.db).await.unwrap();sqlx::query(sql).execute(&s.db).await.unwrap();let before=move_database_snapshot(&s).await;assert!(invoke(s.clone(),p,req()).await.is_err(),"{sql}");assert_eq!(move_database_snapshot(&s).await,before,"{sql}");}
    }
    #[tokio::test]
    async fn admin_birth_true_wal_fence_wait_rechecks_session_key_and_policy() {
        for mode in 0..3 {
            let s = setup(true).await;
            let p = if mode == 1 {
                key(&s, "birth-wait", true, "[\"admin\"]").await;
                key_proof(&s, "birth-wait").await
            } else {
                admin(&s).await
            };
            let mut blocker = s.db.begin().await.unwrap();
            crate::spaces::mutation_guards::lock_audience_for_mutation(&mut blocker)
                .await
                .unwrap();
            sqlx::query(match mode{0=>"DELETE FROM sessions WHERE user_id='owner-a'",1=>"UPDATE api_keys SET scopes='[\"read\"]' WHERE id='birth-wait'",_=>"INSERT INTO system_config(key,value) VALUES('user_provisioning_allowed_email_domains','other.example')"}).execute(&mut *blocker).await.unwrap();
            let c = s.clone();
            let task = tokio::spawn(async move { invoke(c, p, req()).await.map(|_| ()) });
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            assert!(!task.is_finished());
            blocker.commit().await.unwrap();
            let before = move_database_snapshot(&s).await;
            assert!(task.await.unwrap().is_err());
            assert_eq!(move_database_snapshot(&s).await, before);
        }
    }
    #[tokio::test]
    async fn admin_birth_final_original_jwt_expiry_rolls_back_both_audits() {
        let s = setup(false).await;
        let mut claims = crate::spaces::routes::catalog_tests::auth("owner-a").claims;
        claims.exp = chrono::Utc::now().timestamp() + 5;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes()),
        )
        .unwrap();
        let r = axum::http::Request::builder()
            .method("POST")
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = r.into_parts();
        let p = AdminUser::from_request_parts(&mut parts, &s).await.unwrap();
        let before = move_database_snapshot(&s).await;
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let at_boundary = reached.clone();
        let hook: std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> =
            Box::pin(async move {
                at_boundary.store(true, std::sync::atomic::Ordering::SeqCst);
                while chrono::Utc::now().timestamp() < (claims.exp as i64) {
                    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
                }
            });
        let result = ADMIN_BIRTH_FINAL_HOOK
            .scope(
                std::cell::RefCell::new(Some(hook)),
                invoke(s.clone(), p, req()),
            )
            .await;
        assert!(result.is_err());
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(move_database_snapshot(&s).await, before);
    }
}

#[cfg(test)]
tokio::task_local! {static ADMIN_CATALOG_FINAL_HOOK:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;}
#[cfg(test)]
async fn admin_catalog_read_boundary() {
    if let Ok(Some(hook)) = ADMIN_CATALOG_FINAL_HOOK.try_with(|slot| slot.borrow_mut().take()) { hook.await; }
}

#[cfg(test)]
mod admin_catalog_current_tests {
    use super::*;
    use axum::extract::FromRequestParts;
    use tower::ServiceExt;
    use crate::spaces::documents::tests::move_database_snapshot;

    async fn setup(wal: bool) -> AppState {
        let s=crate::spaces::routes::space_creation_fence_tests::setup(wal).await;
        sqlx::query("UPDATE sessions SET device_name='Browser' WHERE device_name IS NULL").execute(&s.db).await.unwrap();
        s
    }
    async fn key(s: &AppState, id: &str, service: bool, scopes: &str) {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES($1,$2,$1,$3,$1,$4,$5)")
            .bind(id).bind((!service).then_some("owner-a"))
            .bind(crate::auth::api_keys::hash_api_key(id)).bind(i64::from(service)).bind(scopes)
            .execute(&s.db).await.unwrap();
    }
    async fn proof(s: &AppState, key: Option<&str>, exp: Option<i64>) -> AdminUser {
        let mut request = axum::http::Request::builder().method("GET");
        if let Some(key) = key {
            request = request.header("X-API-Key", key);
        } else {
            let mut claims = crate::spaces::routes::catalog_tests::auth("owner-a").claims;
            if let Some(exp) = exp { claims.exp = exp; }
            let token = jsonwebtoken::encode(&jsonwebtoken::Header::default(), &claims,
                &jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes())).unwrap();
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        let (mut parts, _) = request.body(()).unwrap().into_parts();
        AdminUser::from_request_parts(&mut parts, s).await.unwrap()
    }
    async fn invoke(s: AppState, p: AdminUser, route: usize) -> axum::response::Response {
        match route {
            0 => list_users(State(s), p).await.map(IntoResponse::into_response),
            _ => list_admin_user_sessions(State(s), p, Path(if route == 1 {"owner-a"} else {"missing"}.into()))
                .await.map(IntoResponse::into_response),
        }.unwrap_or_else(IntoResponse::into_response)
    }

    #[tokio::test]
    async fn admin_catalog_http_keys_preserve_wire_and_never_purge_expired_rows() {
        for service in [false, true] {
            let s = setup(false).await;
            key(&s, "catalog-admin", service, "[\"admin\"]").await;
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('expired-reader','viewer-a','expired','2000-01-01T00:00:00Z'),('expired-self','owner-a','expired','2000-01-01T00:00:00Z')")
                .execute(&s.db).await.unwrap();
            let before = move_database_snapshot(&s).await;
            for (path, status) in [("/admin/users",StatusCode::OK),("/admin/users/owner-a/sessions",StatusCode::OK),("/admin/users/missing/sessions",StatusCode::NOT_FOUND)] {
                let response = router().with_state(s.clone()).oneshot(axum::http::Request::builder()
                    .uri(path).header("X-API-Key","catalog-admin").body(axum::body::Body::empty()).unwrap()).await.unwrap();
                let actual_status=response.status();
                let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();
                assert_eq!(actual_status,status,"path {path}: {value}");
                if path == "/admin/users" {
                    let owner=value.as_array().unwrap().iter().find(|u|u["id"]=="owner-a").unwrap();
                    assert_eq!(owner["role"],"admin"); assert_eq!(owner["is_active"],true);
                    assert_eq!(owner.as_object().unwrap().len(),9);
                    assert_eq!(owner["account_kind"],"member");
                } else if status==StatusCode::OK {
                    assert_eq!(value.as_array().unwrap().len(),1);
                    assert_eq!(value[0]["id"],"metadata-owner-a");
                    assert_eq!(value[0]["is_current"],false);
                    assert_eq!(value[0].as_object().unwrap().len(),10);
                }
            }
            // The existing real HTTP key extractor updates last_used_at. Prove
            // the handler itself performs no SQL writes with that proof captured
            // before the full database baseline, retaining every table/column.
            let after=move_database_snapshot(&s).await;
            assert_eq!(after.iter().filter(|(table,_)|table!="api_keys").collect::<Vec<_>>(),
                before.iter().filter(|(table,_)|table!="api_keys").collect::<Vec<_>>());
            for route in 0..3 {
                let p=proof(&s,Some("catalog-admin"),None).await;
                let baseline=move_database_snapshot(&s).await;
                let response=invoke(s.clone(),p,route).await;
                assert_eq!(response.status(),if route==2 {StatusCode::NOT_FOUND}else{StatusCode::OK});
                assert_eq!(move_database_snapshot(&s).await,baseline);
            }
            for scopes in ["[\"read\"]","[\"write\"]"] {
                key(&s,"catalog-denied",false,scopes).await;
                for path in ["/admin/users","/admin/users/owner-a/sessions"] {
                    let response=router().with_state(s.clone()).oneshot(axum::http::Request::builder()
                        .uri(path).header("X-API-Key","catalog-denied").body(axum::body::Body::empty()).unwrap()).await.unwrap();
                    assert_eq!(response.status(),StatusCode::FORBIDDEN);
                }
                sqlx::query("DELETE FROM api_keys WHERE id='catalog-denied'").execute(&s.db).await.unwrap();
            }
        }
        let s=setup(false).await;
        let p=proof(&s,None,None).await;
        let response=invoke(s,p,1).await;
        assert_eq!(response.status(),StatusCode::OK);
        let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();
        assert_eq!(value[0]["is_current"],true);
    }

    #[tokio::test]
    async fn admin_catalog_waiting_one_connection_observes_revoked_original_credential() {
        for route in 0..3 {
            let s=setup(false).await;
            let p=proof(&s,None,None).await;
            let mut held=s.db.acquire().await.unwrap();
            let c=s.clone();
            let task=tokio::spawn(async move {invoke(c,p,route).await.status()});
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
            assert!(!task.is_finished());
            sqlx::query("DELETE FROM sessions WHERE user_id='owner-a'").execute(&mut *held).await.unwrap();
            drop(held);
            let before=move_database_snapshot(&s).await;
            let status=task.await.unwrap();
            assert!(status==StatusCode::UNAUTHORIZED || status==StatusCode::FORBIDDEN);
            assert_eq!(move_database_snapshot(&s).await,before);
        }
    }

    #[tokio::test]
    async fn admin_catalog_after_load_real_wal_revocation_denies_rows_and_missing_user() {
        for route in 0..3 {
            for mode in 0..4 {
                let s=setup(true).await;
                let p=if mode==3 {key(&s,"catalog-service",true,"[\"admin\"]").await;proof(&s,Some("catalog-service"),None).await} else {proof(&s,None,None).await};
                let changed=s.clone();
                let hook:std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>=Box::pin(async move {
                    sqlx::query(match mode {0=>"DELETE FROM sessions WHERE user_id='owner-a'",1=>"UPDATE users SET role='member' WHERE id='owner-a'",2=>"UPDATE users SET is_active=0 WHERE id='owner-a'",_=>"UPDATE api_keys SET scopes='[\"read\"]' WHERE id='catalog-service'"})
                        .execute(&changed.db).await.unwrap();
                });
                let response=ADMIN_CATALOG_FINAL_HOOK.scope(std::cell::RefCell::new(Some(hook)),invoke(s.clone(),p,route)).await;
                assert!(response.status()==StatusCode::UNAUTHORIZED || response.status()==StatusCode::FORBIDDEN);
                let value:serde_json::Value=serde_json::from_slice(&axum::body::to_bytes(response.into_body(),1024*1024).await.unwrap()).unwrap();
                assert!(!value.is_array());
                assert_ne!(value["error"],"User not found");
            }
        }
    }

    #[tokio::test]
    async fn admin_catalog_last_proof_checks_original_jwt_expiry_after_data_load() {
        for route in 0..3 {
            let s=setup(false).await;
            let exp=chrono::Utc::now().timestamp()+2;
            let p=proof(&s,None,Some(exp)).await;
            let before=move_database_snapshot(&s).await;
            let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let at=reached.clone();
            let hook:std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>=Box::pin(async move {
                at.store(true,std::sync::atomic::Ordering::SeqCst);
                while chrono::Utc::now().timestamp()<exp {tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
            });
            let response=ADMIN_CATALOG_FINAL_HOOK.scope(std::cell::RefCell::new(Some(hook)),invoke(s.clone(),p,route)).await;
            assert!(reached.load(std::sync::atomic::Ordering::SeqCst));
            assert!(response.status()==StatusCode::UNAUTHORIZED || response.status()==StatusCode::FORBIDDEN);
            assert_eq!(move_database_snapshot(&s).await,before);
        }
    }
}

#[cfg(test)]
mod oauth_state_expiry_tests {
    use super::*;
    async fn age(cache: &OAuthStateCache, key: &str, age: std::time::Duration) {
        cache.inner.write().await.get_mut(key).unwrap().created_at =
            std::time::Instant::now().checked_sub(age).unwrap();
    }
    #[tokio::test]
    async fn oauth_and_oidc_states_expire_on_take_without_another_store() {
        for seconds in [600, 601] {
            let cache=OAuthStateCache::new();
            cache.store_oauth("oauth",oauth2::PkceCodeVerifier::new("verifier".into())).await;
            cache.store_oidc("oidc",openidconnect::Nonce::new("nonce".into())).await;
            age(&cache,"oauth",std::time::Duration::from_secs(seconds)).await;
            age(&cache,"oidc",std::time::Duration::from_secs(seconds)).await;
            assert!(cache.take_oauth("oauth").await.is_none());
            assert!(cache.take_oidc("oidc").await.is_none());
            assert!(cache.inner.read().await.is_empty());
        }
    }
    #[tokio::test]
    async fn fresh_and_just_unexpired_state_preserves_exact_secret_and_single_use() {
        for seconds in [0, 599] {
            let cache=OAuthStateCache::new();
            cache.store_oauth("oauth",oauth2::PkceCodeVerifier::new("exact-verifier".into())).await;
            cache.store_oidc("oidc",openidconnect::Nonce::new("exact-nonce".into())).await;
            age(&cache,"oauth",std::time::Duration::from_secs(seconds)).await;
            age(&cache,"oidc",std::time::Duration::from_secs(seconds)).await;
            assert_eq!(cache.take_oauth("oauth").await.unwrap().secret(),"exact-verifier");
            assert_eq!(cache.take_oidc("oidc").await.unwrap().secret(),"exact-nonce");
            assert!(cache.take_oauth("oauth").await.is_none());
            assert!(cache.take_oidc("oidc").await.is_none());
        }
    }
    #[tokio::test]
    async fn wrong_flow_consumes_state_without_exposing_or_reusing_other_secret() {
        let cache=OAuthStateCache::new();
        cache.store_oauth("oauth",oauth2::PkceCodeVerifier::new("verifier".into())).await;
        cache.store_oidc("oidc",openidconnect::Nonce::new("nonce".into())).await;
        assert!(cache.take_oidc("oauth").await.is_none());
        assert!(cache.take_oauth("oauth").await.is_none());
        assert!(cache.take_oauth("oidc").await.is_none());
        assert!(cache.take_oidc("oidc").await.is_none());
    }
    #[tokio::test(flavor="multi_thread",worker_threads=2)]
    async fn concurrent_callbacks_receive_each_state_at_most_once() {
        for oidc in [false,true] {
            let cache=OAuthStateCache::new();
            if oidc {cache.store_oidc("shared",openidconnect::Nonce::new("nonce".into())).await;}
            else {cache.store_oauth("shared",oauth2::PkceCodeVerifier::new("verifier".into())).await;}
            let mut jobs=Vec::new();
            for _ in 0..16 {
                let cache=cache.clone();
                jobs.push(tokio::spawn(async move {
                    if oidc {cache.take_oidc("shared").await.is_some()}
                    else {cache.take_oauth("shared").await.is_some()}
                }));
            }
            let mut received=0;
            for job in jobs {received+=usize::from(job.await.unwrap());}
            assert_eq!(received,1);
        }
    }
}
#[cfg(test)]
mod newborn_owned_resource_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use tower::ServiceExt;
    async fn setup() -> AppState {
        let s = crate::spaces::routes::space_creation_fence_tests::setup(false).await;
        sqlx::query("INSERT INTO system_config(key,value) VALUES('allow_public_signups','true') ON CONFLICT(key) DO UPDATE SET value='true'").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE spaces SET visibility='public' WHERE id='space-a'")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,created_at,updated_at,published_at) VALUES('existing-private','space-a','Private','pages/private.md','owner-a','owner-a','old','old',NULL)").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO templates(id,space_id,name,content,created_by) VALUES('existing-template','space-a','Template','TEMPLATE','owner-a')").execute(&s.db).await.unwrap();
        s
    }
    async fn inject(s: &AppState, family: &str, kind: &str) {
        let owner = if family == "registration" {
            "NEW.user_id"
        } else {
            "NEW.target_id"
        };
        let action = match family {
            "admin" => "user.create",
            "scim" => "user.scim_create",
            "csv" => "user.import",
            _ => "space.personal_create",
        };
        let insert = if kind == "document" {
            format!("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) SELECT 'unexpected-draft',id,space_id,{owner},'Private owned','PRIVATE BODY',updated_at FROM documents WHERE id='existing-private';")
        } else {
            format!("INSERT INTO template_drafts(id,template_id,space_id,owner_id,snapshot) SELECT 'unexpected-template-draft',id,space_id,{owner},'{{\"name\":\"Owned\",\"content\":\"PRIVATE TEMPLATE\"}}' FROM templates WHERE id='existing-template';")
        };
        sqlx::query(&format!("CREATE TRIGGER inject_newborn_owner AFTER INSERT ON audit_log WHEN NEW.action='{action}' BEGIN {insert} END")).execute(&s.db).await.unwrap();
    }
    async fn admin_key(s: &AppState) {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('newborn-admin','owner-a','Admin',$1,'newborn','[\"admin\"]')").bind(crate::auth::api_keys::hash_api_key("newborn-admin")).execute(&s.db).await.unwrap();
    }
    async fn run(family: &str) {
        for kind in ["document", "template"] {
            let s = setup().await;
            if matches!(family, "admin" | "csv") {
                admin_key(&s).await;
            }
            inject(&s, family, kind).await;
            let before = move_database_snapshot(&s).await;
            let (path,content_type,payload)=match family {
                "csv" => ("/admin/users/import","multipart/form-data; boundary=b", "--b\r\nContent-Disposition: form-data; name=\"file\"; filename=\"users.csv\"\r\nContent-Type: text/csv\r\n\r\nemail,display_name,role,spaces\nnewborn@example.com,Newborn,member,space-a\n\r\n--b--\r\n".to_string()),
                "admin" => ("/admin/users","application/json",json!({"email":"newborn@example.com","display_name":"Newborn","role":"member"}).to_string()),
                "scim" => ("/Users","application/json",json!({"schemas":["urn:ietf:params:scim:schemas:core:2.0:User"],"userName":"newborn@example.com","displayName":"Newborn","active":true}).to_string()),
                _ => ("/auth/register","application/json",json!({"email":"newborn@example.com","display_name":"Newborn","password":"Safe-Newborn-Password-123!"}).to_string()),
            };
            let mut request = axum::http::Request::builder()
                .method("POST")
                .uri(path)
                .extension(axum::extract::ConnectInfo(
                    "127.0.0.1:5199".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .header("Content-Type", content_type);
            if matches!(family, "admin" | "csv") {
                request = request.header("X-API-Key", "newborn-admin");
            }
            if family == "scim" {
                request = request.header("Authorization", "Bearer newborn-isolated-scim");
            }
            let router = match family {
                "csv" => crate::provisioning::router(),
                "scim" => crate::scim::router(),
                _ => router(),
            };
            let response = router
                .with_state(s.clone())
                .oneshot(request.body(axum::body::Body::from(payload)).unwrap())
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                if family == "scim" {
                    StatusCode::INTERNAL_SERVER_ERROR
                } else {
                    StatusCode::CONFLICT
                },
                "{family}/{kind}"
            );
            // The HTTP extractor alone updates this existing actor key's last_used_at.
            // Restore only that known bookkeeping field; every credential/owner/graph/audit row remains compared.
            if matches!(family, "admin" | "csv") {
                sqlx::query("UPDATE api_keys SET last_used_at=NULL WHERE id='newborn-admin'")
                    .execute(&s.db)
                    .await
                    .unwrap();
            }
            let after = move_database_snapshot(&s).await;
            let expected = before;
            assert_eq!(
                after, expected,
                "{family}/{kind}: full account/graph/audit/owned resources rollback"
            );
        }
    }
    #[tokio::test]
    async fn newborn_ownership_admin_injections_roll_back() {
        run("admin").await;
    }
    #[tokio::test]
    async fn newborn_ownership_registration_injections_roll_back() {
        run("registration").await;
    }
    #[tokio::test]
    async fn newborn_ownership_csv_injections_roll_back() {
        run("csv").await;
    }
    #[tokio::test]
    async fn newborn_ownership_scim_injections_roll_back() {
        if std::env::var("STARLANE_NEWBORN_SCIM_CHILD").as_deref() == Ok("1") {
            run("scim").await;
            return;
        }
        let status=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","auth::routes::newborn_owned_resource_tests::newborn_ownership_scim_injections_roll_back","--nocapture"]).env("STARLANE_NEWBORN_SCIM_CHILD","1").env("SCIM_BEARER_TOKEN","newborn-isolated-scim").status().unwrap();
        assert!(status.success());
    }
}

#[cfg(test)]
mod oauth_atomic_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    async fn setup(wal: bool) -> AppState {
        crate::spaces::routes::space_creation_fence_tests::setup(wal).await
    }
    async fn origin(
        s: &AppState,
        provider: &str,
        email: &str,
        subject: &str,
        allow: bool,
        role: Option<&str>,
    ) -> VerifiedProviderOrigin {
        let config = if provider == "oidc" {
            sqlx::query("INSERT INTO system_config(key,value) VALUES('oidc_enabled','true') ON CONFLICT(key) DO UPDATE SET value='true'").execute(&s.db).await.unwrap();
            sqlx::query("INSERT INTO system_config(key,value) VALUES('oidc_auto_provision',$1),('oidc_role_claim',$2) ON CONFLICT(key) DO UPDATE SET value=excluded.value").bind(if allow{"true"}else{"false"}).bind(if role.is_some(){"roles"}else{""}).execute(&s.db).await.unwrap();
            let mut c = s.db.acquire().await.unwrap();
            ExternalConfig::Oidc(OidcSource::load(&mut c).await.unwrap())
        } else {
            ExternalConfig::OAuth(oauth::ProviderConfig::capture(&if provider == "google" {
                oauth::OAuthProvider::Google
            } else {
                oauth::OAuthProvider::GitHub
            }))
        };
        VerifiedProviderOrigin {
            info: oauth::OAuthUserInfo {
                provider: provider.into(),
                provider_user_id: subject.into(),
                email: email.into(),
                display_name: format!("Provider {}", Uuid::new_v4()),
                avatar_url: Some("https://example.test/avatar.png".into()),
            },
            config,
            auto_provision: allow,
            role_override: role.map(str::to_string),
        }
    }
    async fn finish(
        s: &AppState,
        o: VerifiedProviderOrigin,
    ) -> Result<axum::response::Response, AppError> {
        complete_oauth_sign_in(s, o)
            .await
            .map(IntoResponse::into_response)
    }
    fn tokens(s: &AppState, r: &axum::response::Response) -> (jwt::Claims, jwt::Claims) {
        assert_eq!(r.status(), StatusCode::TEMPORARY_REDIRECT);
        let url = r.headers().get("location").unwrap().to_str().unwrap();
        assert!(url.starts_with(&format!("{}/#access_token=", s.base_url)));
        let raw = url.split_once('#').unwrap().1;
        let mut pieces = raw.split('&');
        let access = pieces
            .next()
            .unwrap()
            .strip_prefix("access_token=")
            .unwrap();
        let refresh = pieces
            .next()
            .unwrap()
            .strip_prefix("refresh_token=")
            .unwrap();
        (
            jwt::validate_token(access, &s.jwt_secret, jwt::TokenType::Access).unwrap(),
            jwt::validate_token(refresh, &s.jwt_secret, jwt::TokenType::Refresh).unwrap(),
        )
    }
    async fn email(s: &AppState, id: &str) -> String {
        sqlx::query_scalar("SELECT email FROM users WHERE id=$1")
            .bind(id)
            .fetch_one(&s.db)
            .await
            .unwrap()
    }
    async fn clean(s: &AppState, user: &str) {
        let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE created_by=$1")
            .bind(user)
            .fetch_one(&s.db)
            .await
            .unwrap();
        let _ = tokio::fs::remove_dir_all(storage::space_dir(&slug)).await;
    }
    #[tokio::test]
    async fn provider_new_birth_preserves_raw_profile_tokens_and_exact_topology() {
        for provider in ["google", "github", "oidc"] {
            let s = setup(false).await;
            let raw = format!(" Raw.{}@Example.COM ", Uuid::new_v4());
            let o = origin(
                &s,
                provider,
                &raw,
                "new-subject",
                true,
                (provider == "oidc").then_some("admin"),
            )
            .await;
            let name = o.info.display_name.clone();
            let response = finish(&s, o).await.unwrap();
            let (a, r) = tokens(&s, &response);
            assert_eq!(a.email, raw);
            assert_eq!(a.sid, r.sid);
            assert_eq!(
                a.role,
                if provider == "oidc" {
                    "admin"
                } else {
                    "member"
                }
            );
            assert_eq!(a.exp - a.iat, 900);
            assert_eq!(r.exp - r.iat, 30 * 24 * 3600);
            let user = account_lifecycle_user(&mut *s.db.acquire().await.unwrap(), &a.sub)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(user.email, raw);
            assert_eq!(user.display_name, name);
            assert_eq!(user.role, "member");
            assert_eq!(user.password_hash, None);
            assert_eq!(
                user.avatar_url.as_deref(),
                Some("https://example.test/avatar.png")
            );
            let mut c = s.db.acquire().await.unwrap();
            let links = oauth_rows(&mut c, &a.sub).await.unwrap();
            let sessions = external_session_rows(&mut c, &a.sub).await.unwrap();
            assert_eq!(links.len(), 1);
            assert_eq!(
                (&links[0].2, &links[0].3),
                (&provider.to_string(), &"new-subject".to_string())
            );
            assert!(links[0].5.is_none() && links[0].6.is_none() && links[0].7.is_none());
            assert_eq!(sessions.len(), 1);
            assert_eq!(sessions[0].0, a.sid);
            assert_eq!(sessions[0].3.as_deref(), Some("Web browser"));
            assert_eq!(
                (&sessions[0].9, &sessions[0].10),
                (&"browser".to_string(), &"web".to_string())
            );
            drop(c);
            let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT(SELECT COUNT(*) FROM spaces WHERE created_by=$1),(SELECT COUNT(*) FROM space_members WHERE user_id=$1),(SELECT COUNT(*) FROM group_members WHERE user_id=$1),(SELECT COUNT(*) FROM group_global_roles g JOIN group_members m ON m.group_id=g.group_id WHERE m.user_id=$1)").bind(&a.sub).fetch_one(&s.db).await.unwrap();
            assert_eq!(counts, (1, 1, 1, 0));
            let audit:(String,String,String)=sqlx::query_as("SELECT user_id,actor_kind,actor_id FROM audit_log WHERE action='space.personal_create'").fetch_one(&s.db).await.unwrap();
            assert_eq!(audit, (a.sub.clone(), "user".into(), a.sub.clone()));
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM audit_log WHERE action!='space.personal_create'"
                )
                .fetch_one(&s.db)
                .await
                .unwrap(),
                0
            );
            clean(&s, &a.sub).await;
        }
    }
    #[tokio::test]
    async fn provider_existing_email_and_link_preserve_full_account_and_prior_credentials() {
        let s = setup(false).await;
        let raw = email(&s, "owner-a").await;
        let before = account_lifecycle_user(&mut *s.db.acquire().await.unwrap(), "owner-a")
            .await
            .unwrap();
        let first = finish(
            &s,
            origin(&s, "oidc", &raw, "linked-subject", false, Some("member")).await,
        )
        .await
        .unwrap();
        let (a, _) = tokens(&s, &first);
        assert_eq!(a.sub, "owner-a");
        assert_eq!(a.role, "member");
        let linked_before = oauth_rows(&mut *s.db.acquire().await.unwrap(), "owner-a")
            .await
            .unwrap();
        let old_sessions = external_session_rows(&mut *s.db.acquire().await.unwrap(), "owner-a")
            .await
            .unwrap();
        let second = finish(
            &s,
            origin(
                &s,
                "oidc",
                "different-provider-email@example.test",
                "linked-subject",
                false,
                None,
            )
            .await,
        )
        .await
        .unwrap();
        let (b, _) = tokens(&s, &second);
        assert_eq!(b.sub, "owner-a");
        assert_eq!(b.email, "different-provider-email@example.test");
        assert_eq!(b.role, "admin");
        assert!(
            account_lifecycle_user(&mut *s.db.acquire().await.unwrap(), "owner-a")
                .await
                .unwrap()
                == before
        );
        assert!(
            oauth_rows(&mut *s.db.acquire().await.unwrap(), "owner-a")
                .await
                .unwrap()
                == linked_before
        );
        let sessions = external_session_rows(&mut *s.db.acquire().await.unwrap(), "owner-a")
            .await
            .unwrap();
        assert_eq!(sessions.len(), old_sessions.len() + 1);
        for row in old_sessions {
            assert!(sessions.contains(&row));
        }
        assert_eq!(
            sessions.iter().find(|r| r.0 == b.sid).unwrap().3.as_deref(),
            Some("Web browser (2)")
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_log")
                .fetch_one(&s.db)
                .await
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn provider_case_sensitive_linking_and_unknown_auto_provision_gate_remain() {
        let s = setup(false).await;
        let raw = email(&s, "owner-a").await;
        let uppercase = raw.to_uppercase();
        assert_ne!(raw, uppercase);
        let r = finish(
            &s,
            origin(&s, "google", &uppercase, "case-subject", true, None).await,
        )
        .await
        .unwrap();
        let (a, _) = tokens(&s, &r);
        assert_ne!(a.sub, "owner-a");
        clean(&s, &a.sub).await;
        let o = origin(
            &s,
            "oidc",
            "unknown@example.test",
            "disabled-subject",
            false,
            None,
        )
        .await;
        let before = move_database_snapshot(&s).await;
        assert!(
            matches!(finish(&s,o).await,Err(AppError::Unauthorized(ref m)) if m=="OIDC auto provisioning is disabled; ask an administrator to create or link your account")
        );
        assert_eq!(move_database_snapshot(&s).await, before);
    }
    #[tokio::test]
    async fn provider_inactive_existing_and_disabled_config_never_issue_session() {
        for mode in 0..3 {
            let s = setup(false).await;
            let raw = email(&s, "owner-a").await;
            let o = origin(&s, "oidc", &raw, "inactive-subject", true, None).await;
            if mode == 0 {
                sqlx::query("UPDATE users SET is_active=0 WHERE id='owner-a'")
                    .execute(&s.db)
                    .await
                    .unwrap();
            }
            if mode == 1 {
                sqlx::query("INSERT INTO oauth_accounts(id,user_id,provider,provider_user_id,provider_email) VALUES('inactive-link','owner-a','oidc','inactive-subject','old')").execute(&s.db).await.unwrap();
                sqlx::query("UPDATE users SET is_active=0 WHERE id='owner-a'")
                    .execute(&s.db)
                    .await
                    .unwrap();
            }
            if mode == 2 {
                sqlx::query("UPDATE system_config SET value='false' WHERE key='oidc_enabled'")
                    .execute(&s.db)
                    .await
                    .unwrap();
            }
            let before = move_database_snapshot(&s).await;
            assert!(matches!(
                finish(&s, o).await,
                Err(AppError::Unauthorized(_))
            ));
            assert_eq!(move_database_snapshot(&s).await, before);
        }
    }
    #[tokio::test]
    async fn provider_each_account_topology_link_session_audit_failure_rolls_back() {
        for table in [
            "users",
            "spaces",
            "space_members",
            "groups",
            "group_members",
            "space_group_permissions",
            "oauth_accounts",
            "sessions",
            "audit_log",
        ] {
            let s = setup(false).await;
            let o = origin(
                &s,
                "oidc",
                "atomic-new@example.test",
                "new-subject",
                true,
                None,
            )
            .await;
            sqlx::query(&format!("CREATE TRIGGER fail_birth AFTER INSERT ON {table} BEGIN SELECT RAISE(ABORT,'provider birth failed'); END")).execute(&s.db).await.unwrap();
            let before = move_database_snapshot(&s).await;
            assert!(finish(&s, o).await.is_err(), "{table}");
            assert_eq!(move_database_snapshot(&s).await, before, "{table}");
        }
    }
    #[tokio::test]
    async fn provider_late_full_source_config_audit_and_extra_associations_roll_back() {
        for trigger in [
 "CREATE TRIGGER drift AFTER INSERT ON sessions BEGIN UPDATE users SET avatar_url='unexpected' WHERE id=NEW.user_id; END",
 "CREATE TRIGGER drift AFTER INSERT ON sessions BEGIN UPDATE oauth_accounts SET provider_user_id='unexpected' WHERE user_id=NEW.user_id; END",
 "CREATE TRIGGER drift AFTER INSERT ON sessions BEGIN UPDATE sessions SET refresh_token_hash='unexpected' WHERE id=NEW.id; END",
 "CREATE TRIGGER drift AFTER INSERT ON audit_log BEGIN UPDATE audit_log SET after_value='injected' WHERE id=NEW.id; END",
 "CREATE TRIGGER drift AFTER INSERT ON sessions BEGIN UPDATE system_config SET value='false' WHERE key='oidc_enabled'; END",
 "CREATE TRIGGER drift AFTER INSERT ON sessions BEGIN INSERT INTO oauth_accounts(id,user_id,provider,provider_user_id) VALUES('extra-link',NEW.user_id,'github','extra-subject'); END",
 "CREATE TRIGGER drift AFTER INSERT ON sessions BEGIN INSERT INTO space_members(id,space_id,user_id,role) VALUES('extra-member','space-a',NEW.user_id,'owner'); END"
 ]{let s=setup(false).await;let o=origin(&s,"oidc","atomic-new@example.test","new-subject",true,None).await;sqlx::query(trigger).execute(&s.db).await.unwrap();let before=move_database_snapshot(&s).await;assert!(finish(&s,o).await.is_err(),"{trigger}");assert_eq!(move_database_snapshot(&s).await,before,"{trigger}");}
        let s = setup(false).await;
        let raw = email(&s, "owner-a").await;
        let o = origin(&s, "oidc", &raw, "existing-subject", true, None).await;
        sqlx::query("CREATE TRIGGER drift_lock AFTER UPDATE ON users BEGIN UPDATE sessions SET device_name='changed-before-source' WHERE user_id=NEW.id; END").execute(&s.db).await.unwrap();
        let before = move_database_snapshot(&s).await;
        assert!(finish(&s, o).await.is_err());
        assert_eq!(move_database_snapshot(&s).await, before);
    }
    #[tokio::test]
    async fn provider_true_wal_fence_wait_rechecks_current_account_and_exchange_config() {
        for mode in 0..3 {
            let s = setup(true).await;
            let raw = email(&s, "owner-a").await;
            let o = origin(&s, "oidc", &raw, "wait-subject", true, None).await;
            let mut blocker = s.db.begin().await.unwrap();
            crate::spaces::mutation_guards::lock_audience_for_mutation(&mut blocker)
                .await
                .unwrap();
            sqlx::query(match mode {
                0 => "UPDATE users SET is_active=0 WHERE id='owner-a'",
                1 => "UPDATE system_config SET value='false' WHERE key='oidc_enabled'",
                _ => "UPDATE system_config SET value='changed' WHERE key='oidc_role_claim'",
            })
            .execute(&mut *blocker)
            .await
            .unwrap();
            let c = s.clone();
            let task = tokio::spawn(async move { finish(&c, o).await });
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!task.is_finished());
            blocker.commit().await.unwrap();
            let before = move_database_snapshot(&s).await;
            assert!(task.await.unwrap().is_err());
            assert_eq!(move_database_snapshot(&s).await, before);
        }
    }
    #[tokio::test]
    async fn provider_concurrent_same_subject_creates_one_topology_and_distinct_sessions() {
        let s = setup(true).await;
        let a = origin(
            &s,
            "google",
            "concurrent-new@example.test",
            "same-subject",
            true,
            None,
        )
        .await;
        let b = origin(
            &s,
            "google",
            "concurrent-new@example.test",
            "same-subject",
            true,
            None,
        )
        .await;
        let x = s.clone();
        let y = s.clone();
        let (first, second) = tokio::join!(
            tokio::spawn(async move { finish(&x, a).await }),
            tokio::spawn(async move { finish(&y, b).await })
        );
        let (a, _) = tokens(&s, &first.unwrap().unwrap());
        let (b, _) = tokens(&s, &second.unwrap().unwrap());
        assert_eq!(a.sub, b.sub);
        assert_ne!(a.sid, b.sid);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM users WHERE email='concurrent-new@example.test'"
            )
            .fetch_one(&s.db)
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM spaces WHERE created_by=$1")
                .bind(&a.sub)
                .fetch_one(&s.db)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sessions WHERE user_id=$1")
                .bind(&a.sub)
                .fetch_one(&s.db)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_log WHERE user_id=$1")
                .bind(&a.sub)
                .fetch_one(&s.db)
                .await
                .unwrap(),
            1
        );
        clean(&s, &a.sub).await;
    }
    #[tokio::test]
    async fn provider_invalid_state_and_disabled_callback_have_no_database_mutations() {
        let s = setup(false).await;
        let before = move_database_snapshot(&s).await;
        let result = oauth_callback(
            State(s.clone()),
            Path("google".into()),
            Query(OAuthCallbackParams {
                code: "unused".into(),
                state: "unknown".into(),
            }),
        )
        .await;
        assert!(matches!(result, Err(AppError::Unauthorized(_))));
        assert_eq!(move_database_snapshot(&s).await, before);
        let result = oidc_callback(
            State(s.clone()),
            Query(OAuthCallbackParams {
                code: "unused".into(),
                state: "unknown".into(),
            }),
        )
        .await;
        assert!(matches!(result,Err(AppError::BadRequest(ref m))if m=="OIDC is not enabled"));
        assert_eq!(move_database_snapshot(&s).await, before);
        let _ = origin(&s, "oidc", "x", "y", true, None).await;
        let before = move_database_snapshot(&s).await;
        assert!(matches!(
            oidc_callback(
                State(s.clone()),
                Query(OAuthCallbackParams {
                    code: "unused".into(),
                    state: "unknown".into()
                })
            )
            .await,
            Err(AppError::Unauthorized(_))
        ));
        assert_eq!(move_database_snapshot(&s).await, before);
    }
}

#[cfg(test)]
mod oauth_unchanged_association_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    #[tokio::test]
    async fn existing_provider_sign_in_rejects_phantom_grants_keys_and_role_definition_drift() {
        for trigger in [
  "CREATE TRIGGER drift AFTER INSERT ON sessions BEGIN INSERT INTO space_members(id,space_id,user_id,role) VALUES('phantom-owner','space-b',NEW.user_id,'owner'); END",
  "CREATE TRIGGER drift AFTER INSERT ON sessions BEGIN UPDATE api_keys SET scopes='[\"admin\"]' WHERE user_id=NEW.user_id; END",
  "CREATE TRIGGER drift AFTER INSERT ON sessions BEGIN UPDATE passkeys SET sign_count=999 WHERE user_id=NEW.user_id; END",
  "CREATE TRIGGER drift AFTER INSERT ON sessions BEGIN UPDATE space_roles SET permissions='[\"manage_space\"]' WHERE id='actor-role'; END",
  "CREATE TRIGGER drift AFTER INSERT ON sessions BEGIN UPDATE groups SET description='changed' WHERE id='actor-group'; END",
  "CREATE TRIGGER drift AFTER INSERT ON sessions BEGIN INSERT INTO users(id,email,display_name,role) VALUES('phantom-account','phantom@example.test','Phantom','member'); END"
  ]{let s=crate::spaces::routes::space_creation_fence_tests::setup(false).await;
   sqlx::query("INSERT INTO groups(id,name) VALUES('actor-group','Actor group')").execute(&s.db).await.unwrap();
   sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('actor-member','actor-group','editor-a')").execute(&s.db).await.unwrap();
   sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('actor-role','space-a','Actor role','[\"view\"]','owner-a')").execute(&s.db).await.unwrap();
   sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('actor-key','editor-a','Actor key','test-key-hash','prefix','[\"read\"]')").execute(&s.db).await.unwrap();
   sqlx::query("INSERT INTO passkeys(id,user_id,credential_id,public_key,name,sign_count) VALUES('actor-passkey','editor-a','test-credential','test-public-key','Passkey',1)").execute(&s.db).await.unwrap();
   let email:String=sqlx::query_scalar("SELECT email FROM users WHERE id='editor-a'").fetch_one(&s.db).await.unwrap();
   let origin=VerifiedProviderOrigin{info:oauth::OAuthUserInfo{provider:"google".into(),provider_user_id:"existing-subject".into(),email,display_name:"Unchanged".into(),avatar_url:None},config:ExternalConfig::OAuth(oauth::ProviderConfig::capture(&oauth::OAuthProvider::Google)),auto_provision:true,role_override:None};
   sqlx::query(trigger).execute(&s.db).await.unwrap();let before=move_database_snapshot(&s).await;
   assert!(complete_oauth_sign_in(&s,origin).await.is_err(),"{trigger}");assert_eq!(move_database_snapshot(&s).await,before,"{trigger}");
  }
    }
}

#[cfg(test)]
mod oauth_owned_resource_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    async fn source(s: &AppState) {
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('oauth-private-source','space-a','Private source','space-a/docs/private.md','owner-a','owner-a')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO templates(id,space_id,name,content,created_by) VALUES('oauth-template-source','space-a','Private template','PRIVATE','owner-a')").execute(&s.db).await.unwrap();
    }
    fn config() -> ExternalConfig {
        ExternalConfig::OAuth(oauth::ProviderConfig::capture(
            &oauth::OAuthProvider::Google,
        ))
    }
    fn identity(email: &str) -> oauth::OAuthUserInfo {
        oauth::OAuthUserInfo {
            provider: "google".into(),
            provider_user_id: "ownership-subject".into(),
            email: email.into(),
            display_name: "OAuth ownership".into(),
            avatar_url: None,
        }
    }
    #[tokio::test]
    async fn provider_newborn_injected_document_or_template_ownership_is_rolled_back() {
        for table in ["document", "template"] {
            let s = crate::spaces::routes::space_creation_fence_tests::setup(false).await;
            source(&s).await;
            let trigger = if table == "document" {
                "CREATE TRIGGER inject_owned AFTER INSERT ON audit_log WHEN NEW.action='space.personal_create' BEGIN INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('injected-own','oauth-private-source','space-a',NEW.user_id,'Private','PRIVATE','base'); END"
            } else {
                "CREATE TRIGGER inject_owned AFTER INSERT ON audit_log WHEN NEW.action='space.personal_create' BEGIN INSERT INTO template_drafts(id,template_id,space_id,owner_id,snapshot) VALUES('injected-own','oauth-template-source','space-a',NEW.user_id,'{}'); END"
            };
            sqlx::query(trigger).execute(&s.db).await.unwrap();
            let before = move_database_snapshot(&s).await;
            let o = VerifiedProviderOrigin {
                info: identity("new-ownership@example.test"),
                config: config(),
                auto_provision: true,
                role_override: None,
            };
            assert!(complete_oauth_sign_in(&s, o).await.is_err(), "{table}");
            assert_eq!(move_database_snapshot(&s).await, before, "{table}");
        }
    }
    #[tokio::test]
    async fn provider_existing_owners_preserved_with_real_session_and_ownership_drift_denied() {
        use axum::extract::FromRequestParts;
        for drift in [false, true] {
            let s = crate::spaces::routes::space_creation_fence_tests::setup(false).await;
            source(&s).await;
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('existing-own','oauth-private-source','space-a','editor-a','Private','PRIVATE','base')").execute(&s.db).await.unwrap();
            sqlx::query("INSERT INTO template_drafts(id,template_id,space_id,owner_id,snapshot) VALUES('existing-template-own','oauth-template-source','space-a','editor-a','{}')").execute(&s.db).await.unwrap();
            if drift {
                sqlx::query("CREATE TRIGGER drift_template AFTER INSERT ON sessions BEGIN UPDATE template_drafts SET owner_id='owner-a' WHERE id='existing-template-own'; END").execute(&s.db).await.unwrap();
            }
            let email: String = sqlx::query_scalar("SELECT email FROM users WHERE id='editor-a'")
                .fetch_one(&s.db)
                .await
                .unwrap();
            let before = move_database_snapshot(&s).await;
            let result = complete_oauth_sign_in(
                &s,
                VerifiedProviderOrigin {
                    info: identity(&email),
                    config: config(),
                    auto_provision: true,
                    role_override: None,
                },
            )
            .await;
            if drift {
                assert!(result.is_err());
                assert_eq!(move_database_snapshot(&s).await, before);
            } else {
                let response = result.unwrap().into_response();
                let url = response
                    .headers()
                    .get("location")
                    .unwrap()
                    .to_str()
                    .unwrap();
                let token = url
                    .split_once("#access_token=")
                    .unwrap()
                    .1
                    .split('&')
                    .next()
                    .unwrap();
                let request = axum::http::Request::builder()
                    .header("Authorization", format!("Bearer {token}"))
                    .body(())
                    .unwrap();
                let (mut parts, _) = request.into_parts();
                let auth = AuthUser::from_request_parts(&mut parts, &s).await.unwrap();
                let mut conn = s.db.acquire().await.unwrap();
                assert!(crate::spaces::access::resolve_page_access(
                    &mut conn,
                    crate::spaces::access::principal_from_auth(&auth).unwrap(),
                    "space-a",
                    "oauth-private-source",
                    crate::spaces::access::PageOperation::View,
                    crate::spaces::access::Purpose::Routine
                )
                .await
                .is_ok());
                let doc: String = sqlx::query_scalar(
                    "SELECT owner_id FROM document_drafts WHERE id='existing-own'",
                )
                .fetch_one(&mut *conn)
                .await
                .unwrap();
                let template: String = sqlx::query_scalar(
                    "SELECT owner_id FROM template_drafts WHERE id='existing-template-own'",
                )
                .fetch_one(&mut *conn)
                .await
                .unwrap();
                assert_eq!(doc, "editor-a");
                assert_eq!(template, "editor-a");
            }
        }
    }
}

include!("passkey_commit.rs");

#[cfg(all(test,target_os="linux"))]
include!("legacy_counter_owner.rs");
