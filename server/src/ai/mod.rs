mod providers;

use axum::{
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ai/config", get(ai_config))
        .route("/ai/chat", post(chat))
        .route("/ai/complete", post(complete))
        .route("/ai/action", post(doc_action))
}

#[derive(Serialize)]
pub struct AiConfigResponse {
    pub enabled: bool,
    pub default_provider: String,
    pub providers: Vec<String>,
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

fn check_ai_enabled() -> Result<(), AppError> {
    let enabled = std::env::var("AI_ENABLED")
        .map(|v| v == "true")
        .unwrap_or(false);

    if !enabled {
        return Err(AppError::BadRequest(
            "AI features are not enabled. Set AI_ENABLED=true".into(),
        ));
    }
    Ok(())
}

/// GET /api/v1/ai/config
async fn ai_config() -> impl IntoResponse {
    let enabled = std::env::var("AI_ENABLED")
        .map(|v| v == "true")
        .unwrap_or(false);

    let default_provider =
        std::env::var("AI_DEFAULT_PROVIDER").unwrap_or_else(|_| "ollama".to_string());

    let mut available = Vec::new();
    if std::env::var("OLLAMA_BASE_URL").is_ok() {
        available.push("ollama".to_string());
    }
    if std::env::var("OPENAI_API_KEY").is_ok() {
        available.push("openai".to_string());
    }
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        available.push("anthropic".to_string());
    }
    if std::env::var("XAI_API_KEY").is_ok() {
        available.push("xai".to_string());
    }

    Json(AiConfigResponse {
        enabled,
        default_provider,
        providers: available,
    })
}

/// POST /api/v1/ai/chat — Chat sidebar conversation with document context
async fn chat(
    _auth: AuthUser,
    Json(req): Json<ChatRequest>,
) -> Result<impl IntoResponse, AppError> {
    check_ai_enabled()?;

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

    let response = providers::chat_completion(messages)
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
    _auth: AuthUser,
    Json(req): Json<CompleteRequest>,
) -> Result<impl IntoResponse, AppError> {
    check_ai_enabled()?;

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

    let response = providers::chat_completion(messages)
        .await
        .map_err(|e| AppError::Internal(format!("AI provider error: {}", e)))?;

    Ok(Json(serde_json::json!({ "completion": response })))
}

/// POST /api/v1/ai/action — Document-level actions (summarize, translate, etc.)
async fn doc_action(
    _auth: AuthUser,
    Json(req): Json<DocActionRequest>,
) -> Result<impl IntoResponse, AppError> {
    check_ai_enabled()?;

    let messages = providers::build_action_prompt(
        &req.action,
        &req.content,
        req.custom_prompt.as_deref(),
        req.language.as_deref(),
    );

    let response = providers::chat_completion(messages)
        .await
        .map_err(|e| AppError::Internal(format!("AI provider error: {}", e)))?;

    Ok(Json(serde_json::json!({ "result": response })))
}
