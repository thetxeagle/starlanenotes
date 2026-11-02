mod providers;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::auth::middleware::{AdminUser, AuthUser};
use crate::auth::routes::AppError;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ai/config", get(ai_config))
        .route("/ai/providers", get(list_providers))
        .route(
            "/ai/providers/{provider}",
            put(save_provider).delete(remove_provider),
        )
        .route("/ai/providers/{provider}/test", post(test_provider))
        .route("/ai/settings", put(save_ai_settings))
        .route("/ai/chat", post(chat))
        .route("/ai/complete", post(complete))
        .route("/ai/action", post(doc_action))
}

#[derive(Serialize)]
pub struct AiConfigResponse {
    pub enabled: bool,
    pub enabled_locked: bool,
    pub default_provider: String,
    pub providers: Vec<String>,
}

#[derive(Serialize)]
pub struct AiProviderResponse {
    pub id: String,
    pub label: String,
    pub configured: bool,
    pub source: String,
    pub locked: bool,
    pub base_url_locked: bool,
    pub base_url: String,
    pub model: String,
    pub has_api_key: bool,
    pub api_key_locked: bool,
    pub removable: bool,
    pub is_default: bool,
}

#[derive(Deserialize)]
pub struct SaveProviderRequest {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
}

#[derive(Deserialize)]
pub struct SaveAiSettingsRequest {
    pub enabled: Option<bool>,
    pub default_provider: Option<String>,
}

#[derive(Serialize)]
pub struct ProviderTestResponse {
    pub connected: bool,
    pub message: String,
    pub models: Vec<String>,
}

#[derive(Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub document_context: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct CompleteRequest {
    pub prompt: String,
    pub context: Option<String>,
}

#[derive(Deserialize)]
pub struct DocActionRequest {
    pub action: String,
    pub content: String,
    pub custom_prompt: Option<String>,
    pub language: Option<String>,
}

async fn check_ai_enabled(state: &AppState) -> Result<(), AppError> {
    if !providers::ai_enabled(state)
        .await
        .map_err(AppError::internal)?
    {
        return Err(AppError::BadRequest(
            "AI features are not enabled. Enable AI in Site Administration or set AI_ENABLED=true"
                .into(),
        ));
    }
    Ok(())
}

/// GET /api/v1/ai/config
async fn ai_config(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let enabled = providers::ai_enabled(&state)
        .await
        .map_err(AppError::internal)?;
    let selected = providers::selected_provider(&state)
        .await
        .map_err(AppError::internal)?;
    let mut available = Vec::new();
    for provider in providers::SUPPORTED_PROVIDERS {
        if providers::provider_is_configured(&state, provider)
            .await
            .map_err(AppError::internal)?
        {
            available.push(provider.id().to_string());
        }
    }

    Ok(Json(AiConfigResponse {
        enabled,
        enabled_locked: std::env::var("AI_ENABLED").is_ok(),
        default_provider: selected
            .map(|provider| provider.id().to_string())
            .unwrap_or_default(),
        providers: available,
    }))
}

async fn list_providers(
    State(state): State<AppState>,
    _admin: AdminUser,
) -> Result<impl IntoResponse, AppError> {
    let selected = providers::selected_provider(&state)
        .await
        .map_err(AppError::internal)?;
    let mut result = Vec::new();
    for provider in providers::SUPPORTED_PROVIDERS {
        let config = providers::provider_config(&state, provider)
            .await
            .map_err(AppError::internal)?;
        let configured = providers::provider_is_configured(&state, provider)
            .await
            .map_err(AppError::internal)?;
        let stored_base_url = providers::stored_config_value(&state, provider.base_url_key())
            .await
            .map_err(AppError::internal)?;
        let stored_api_key = match provider.api_key_key() {
            Some(key) => providers::stored_config_value(&state, key)
                .await
                .map_err(AppError::internal)?,
            None => None,
        };
        let environment_locked = providers::provider_environment_locked(provider);
        let has_database_config = stored_base_url.is_some()
            || stored_api_key.is_some()
            || providers::stored_config_value(&state, &provider.model_override_key())
                .await
                .map_err(AppError::internal)?
                .is_some();

        result.push(AiProviderResponse {
            id: provider.id().to_string(),
            label: provider.label().to_string(),
            configured,
            source: if environment_locked {
                "environment".to_string()
            } else if stored_base_url.is_some() || stored_api_key.is_some() {
                "database".to_string()
            } else {
                "unconfigured".to_string()
            },
            locked: environment_locked,
            base_url_locked: std::env::var(provider.base_url_env()).is_ok(),
            base_url: config.base_url,
            model: config.model,
            has_api_key: config.api_key.is_some(),
            api_key_locked: providers::provider_has_environment_secret(provider),
            removable: has_database_config,
            is_default: selected == Some(provider),
        });
    }
    Ok(Json(result))
}

async fn save_provider(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(provider): Path<String>,
    Json(req): Json<SaveProviderRequest>,
) -> Result<impl IntoResponse, AppError> {
    let provider = parse_provider(&provider)?;
    let base_url = req.base_url.trim();
    if base_url.is_empty() {
        return Err(AppError::BadRequest("Provider base URL is required".into()));
    }
    let model = req.model.trim();
    if model.is_empty() {
        return Err(AppError::BadRequest("Provider model is required".into()));
    }
    if let Some(key) = provider.api_key_key() {
        let has_incoming_key = req
            .api_key
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
        let has_existing_key = std::env::var(provider.api_key_env().unwrap_or_default()).is_ok()
            || providers::stored_config_value(&state, key)
                .await
                .map_err(AppError::internal)?
                .is_some();
        if !has_incoming_key && !has_existing_key {
            return Err(AppError::BadRequest(format!(
                "{} requires an API key",
                provider.label()
            )));
        }
    }

    if std::env::var(provider.base_url_env()).is_err() {
        upsert_config(&state, provider.base_url_key(), base_url).await?;
    }
    if let (Some(key), Some(api_key)) = (provider.api_key_key(), req.api_key.as_deref()) {
        if std::env::var(provider.api_key_env().unwrap_or_default()).is_err()
            && !api_key.trim().is_empty()
        {
            upsert_config(&state, key, api_key.trim()).await?;
        }
    }
    upsert_config(&state, &provider.model_override_key(), model).await?;

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "ai.provider.save",
        Some("ai_provider"),
        Some(provider.id()),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn remove_provider(
    State(state): State<AppState>,
    admin: AdminUser,
    Path(provider): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let provider = parse_provider(&provider)?;
    for key in [
        Some(provider.base_url_key()),
        provider.api_key_key(),
        Some(provider.model_key()),
    ]
    .into_iter()
    .flatten()
    {
        sqlx::query("DELETE FROM system_config WHERE key = $1")
            .bind(key)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }
    sqlx::query("DELETE FROM system_config WHERE key = $1")
        .bind(provider.model_override_key())
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    if providers::stored_config_value(&state, "ai_selected_provider")
        .await
        .map_err(AppError::internal)?
        .as_deref()
        == Some(provider.id())
    {
        sqlx::query("DELETE FROM system_config WHERE key = 'ai_selected_provider'")
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;
    }

    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "ai.provider.remove",
        Some("ai_provider"),
        Some(provider.id()),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn test_provider(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(provider): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let provider = parse_provider(&provider)?;
    if !providers::provider_is_configured(&state, provider)
        .await
        .map_err(AppError::internal)?
    {
        return Err(AppError::BadRequest(
            "Save the provider credentials before testing the connection".into(),
        ));
    }
    let config = providers::provider_config(&state, provider)
        .await
        .map_err(AppError::internal)?;
    let models = providers::discover_models(&config)
        .await
        .map_err(|error| AppError::BadRequest(format!("Connection test failed: {error}")))?;
    Ok(Json(ProviderTestResponse {
        connected: true,
        message: format!(
            "Connected to {} and loaded {} model{}.",
            provider.label(),
            models.len(),
            if models.len() == 1 { "" } else { "s" }
        ),
        models,
    }))
}

async fn save_ai_settings(
    State(state): State<AppState>,
    admin: AdminUser,
    Json(req): Json<SaveAiSettingsRequest>,
) -> Result<impl IntoResponse, AppError> {
    if let Some(enabled) = req.enabled {
        if std::env::var("AI_ENABLED").is_ok() {
            return Err(AppError::BadRequest(
                "AI enablement is locked by AI_ENABLED".into(),
            ));
        }
        upsert_config(&state, "ai_enabled", if enabled { "true" } else { "false" }).await?;
    }
    if let Some(default_provider) = req.default_provider {
        let provider = parse_provider(&default_provider)?;
        if !providers::provider_is_configured(&state, provider)
            .await
            .map_err(AppError::internal)?
        {
            return Err(AppError::BadRequest(
                "Configure the provider before making it the default".into(),
            ));
        }
        upsert_config(&state, "ai_selected_provider", provider.id()).await?;
    }
    crate::audit::log_action(
        &state.db,
        Some(&admin.claims.sub),
        None,
        "ai.settings.save",
        Some("system_config"),
        Some("ai"),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_provider(value: &str) -> Result<providers::AiProvider, AppError> {
    providers::AiProvider::parse(value)
        .ok_or_else(|| AppError::BadRequest("Unsupported AI provider".into()))
}

async fn upsert_config(state: &AppState, key: &str, value: &str) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO system_config (key, value, updated_at) VALUES ($1, $2, $3) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&state.db)
    .await
    .map_err(AppError::internal)?;
    Ok(())
}

/// POST /api/v1/ai/chat — Chat sidebar conversation with document context
async fn chat(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(req): Json<ChatRequest>,
) -> Result<impl IntoResponse, AppError> {
    check_ai_enabled(&state).await?;

    let mut messages: Vec<(String, String)> = Vec::new();

    // Add system prompt with optional document context
    let mut system = "You are a helpful writing assistant for Starlane Notes. You help users with their documents — answering questions, suggesting edits, and providing information.".to_string();
    if let Some(ref ctx) = req.document_context {
        system.push_str(&format!(
            "\n\nThe user is currently working on the following document:\n\n{}",
            ctx
        ));
    }
    messages.push(("system".to_string(), system));

    // Add conversation messages
    for msg in req.messages {
        messages.push((msg.role, msg.content));
    }

    let response = providers::chat_completion(&state, messages)
        .await
        .map_err(|e| AppError::Internal(format!("AI provider error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "message": {
            "role": "assistant",
            "content": response
        }
    })))
}

/// POST /api/v1/ai/complete — Inline /ai slash command completion
async fn complete(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(req): Json<CompleteRequest>,
) -> Result<impl IntoResponse, AppError> {
    check_ai_enabled(&state).await?;

    let mut messages = vec![
        ("system".to_string(), "You are a helpful writing assistant. Complete or generate content based on the user's prompt. Return only the generated content, no explanations.".to_string()),
    ];

    if let Some(ref ctx) = req.context {
        messages.push((
            "user".to_string(),
            format!("Document context:\n{}\n\nRequest: {}", ctx, req.prompt),
        ));
    } else {
        messages.push(("user".to_string(), req.prompt));
    }

    let response = providers::chat_completion(&state, messages)
        .await
        .map_err(|e| AppError::Internal(format!("AI provider error: {}", e)))?;

    Ok(Json(serde_json::json!({ "completion": response })))
}

/// POST /api/v1/ai/action — Document-level actions (summarize, translate, etc.)
async fn doc_action(
    State(state): State<AppState>,
    _auth: AuthUser,
    Json(req): Json<DocActionRequest>,
) -> Result<impl IntoResponse, AppError> {
    check_ai_enabled(&state).await?;

    let messages = providers::build_action_prompt(
        &req.action,
        &req.content,
        req.custom_prompt.as_deref(),
        req.language.as_deref(),
    );

    let response = providers::chat_completion(&state, messages)
        .await
        .map_err(|e| AppError::Internal(format!("AI provider error: {}", e)))?;

    Ok(Json(serde_json::json!({ "result": response })))
}
