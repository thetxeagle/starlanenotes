use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, env, time::Duration};

use crate::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiProvider {
    Ollama,
    OpenAI,
    Anthropic,
    Xai,
}

impl AiProvider {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "ollama" => Some(Self::Ollama),
            "openai" => Some(Self::OpenAI),
            "anthropic" => Some(Self::Anthropic),
            "xai" => Some(Self::Xai),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Ollama => "ollama",
            Self::OpenAI => "openai",
            Self::Anthropic => "anthropic",
            Self::Xai => "xai",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Ollama => "Ollama",
            Self::OpenAI => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Xai => "xAI",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::Ollama => "http://localhost:11434",
            Self::OpenAI => "https://api.openai.com/v1",
            Self::Anthropic => "https://api.anthropic.com/v1",
            Self::Xai => "https://api.x.ai/v1",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::Ollama => "llama3.2",
            Self::OpenAI => "gpt-4o",
            Self::Anthropic => "claude-sonnet-4-20250514",
            Self::Xai => "grok-2-latest",
        }
    }

    pub(crate) fn base_url_key(self) -> &'static str {
        match self {
            Self::Ollama => "ollama_base_url",
            Self::OpenAI => "openai_base_url",
            Self::Anthropic => "anthropic_base_url",
            Self::Xai => "xai_base_url",
        }
    }

    pub(crate) fn base_url_env(self) -> &'static str {
        match self {
            Self::Ollama => "OLLAMA_BASE_URL",
            Self::OpenAI => "OPENAI_BASE_URL",
            Self::Anthropic => "ANTHROPIC_BASE_URL",
            Self::Xai => "XAI_BASE_URL",
        }
    }

    pub(crate) fn api_key_key(self) -> Option<&'static str> {
        match self {
            Self::Ollama => None,
            Self::OpenAI => Some("openai_api_key"),
            Self::Anthropic => Some("anthropic_api_key"),
            Self::Xai => Some("xai_api_key"),
        }
    }

    pub(crate) fn api_key_env(self) -> Option<&'static str> {
        match self {
            Self::Ollama => None,
            Self::OpenAI => Some("OPENAI_API_KEY"),
            Self::Anthropic => Some("ANTHROPIC_API_KEY"),
            Self::Xai => Some("XAI_API_KEY"),
        }
    }

    pub(crate) fn model_key(self) -> &'static str {
        match self {
            Self::Ollama => "ollama_model",
            Self::OpenAI => "openai_model",
            Self::Anthropic => "anthropic_model",
            Self::Xai => "xai_model",
        }
    }

    pub(crate) fn model_env(self) -> &'static str {
        match self {
            Self::Ollama => "OLLAMA_MODEL",
            Self::OpenAI => "OPENAI_MODEL",
            Self::Anthropic => "ANTHROPIC_MODEL",
            Self::Xai => "XAI_MODEL",
        }
    }

    pub fn model_override_key(self) -> String {
        format!("ai_selected_model_{}", self.id())
    }
}

pub const SUPPORTED_PROVIDERS: [AiProvider; 4] = [
    AiProvider::Ollama,
    AiProvider::OpenAI,
    AiProvider::Anthropic,
    AiProvider::Xai,
];

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    pub provider: AiProvider,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
}

pub async fn stored_config_value(
    state: &AppState,
    key: &str,
) -> Result<Option<String>, anyhow::Error> {
    let stored: Option<(String,)> =
        sqlx::query_as("SELECT value FROM system_config WHERE key = $1")
            .bind(key)
            .fetch_optional(&state.db)
            .await?;
    Ok(stored
        .map(|item| item.0)
        .filter(|value| !value.trim().is_empty()))
}

async fn effective_value(
    state: &AppState,
    key: &str,
    env_var: Option<&str>,
) -> Result<Option<String>, anyhow::Error> {
    if let Some(env_var) = env_var {
        if let Ok(value) = env::var(env_var) {
            if !value.trim().is_empty() {
                return Ok(Some(value));
            }
        }
    }
    stored_config_value(state, key).await
}

pub async fn provider_config(
    state: &AppState,
    provider: AiProvider,
) -> Result<ProviderConfig, anyhow::Error> {
    let base_url = effective_value(
        state,
        provider.base_url_key(),
        Some(provider.base_url_env()),
    )
    .await?
    .unwrap_or_else(|| provider.default_base_url().to_string());
    validate_base_url(&base_url)?;

    let api_key = match (provider.api_key_key(), provider.api_key_env()) {
        (Some(key), Some(env_var)) => effective_value(state, key, Some(env_var)).await?,
        _ => None,
    };
    let selected_model = stored_config_value(state, &provider.model_override_key()).await?;
    let configured_model =
        effective_value(state, provider.model_key(), Some(provider.model_env())).await?;

    Ok(ProviderConfig {
        provider,
        base_url: base_url.trim_end_matches('/').to_string(),
        api_key,
        model: selected_model
            .or(configured_model)
            .unwrap_or_else(|| provider.default_model().to_string()),
    })
}

pub async fn provider_is_configured(
    state: &AppState,
    provider: AiProvider,
) -> Result<bool, anyhow::Error> {
    if provider == AiProvider::Ollama {
        return Ok(env::var(provider.base_url_env()).is_ok()
            || stored_config_value(state, provider.base_url_key())
                .await?
                .is_some());
    }
    Ok(provider_config(state, provider).await?.api_key.is_some())
}

pub async fn selected_provider(state: &AppState) -> Result<Option<AiProvider>, anyhow::Error> {
    if let Some(selected) = stored_config_value(state, "ai_selected_provider").await? {
        if let Some(provider) = AiProvider::parse(&selected) {
            if provider_is_configured(state, provider).await? {
                return Ok(Some(provider));
            }
        }
    }

    if let Ok(selected) = env::var("AI_DEFAULT_PROVIDER") {
        if let Some(provider) = AiProvider::parse(&selected) {
            if provider_is_configured(state, provider).await? {
                return Ok(Some(provider));
            }
        }
    }

    for provider in SUPPORTED_PROVIDERS {
        if provider_is_configured(state, provider).await? {
            return Ok(Some(provider));
        }
    }
    Ok(None)
}

pub async fn ai_enabled(state: &AppState) -> Result<bool, anyhow::Error> {
    if let Ok(value) = env::var("AI_ENABLED") {
        return Ok(value.eq_ignore_ascii_case("true"));
    }
    Ok(stored_config_value(state, "ai_enabled")
        .await?
        .is_some_and(|value| value.eq_ignore_ascii_case("true")))
}

pub fn provider_environment_locked(provider: AiProvider) -> bool {
    env::var(provider.base_url_env()).is_ok()
        || env::var(provider.model_env()).is_ok()
        || provider
            .api_key_env()
            .is_some_and(|env_var| env::var(env_var).is_ok())
}

pub fn provider_has_environment_secret(provider: AiProvider) -> bool {
    provider
        .api_key_env()
        .is_some_and(|env_var| env::var(env_var).is_ok())
}

fn validate_base_url(base_url: &str) -> Result<(), anyhow::Error> {
    let url = reqwest::Url::parse(base_url.trim())
        .map_err(|_| anyhow::anyhow!("Provider base URL must be a valid URL"))?;
    if !matches!(url.scheme(), "http" | "https") {
        anyhow::bail!("Provider base URL must use http or https");
    }
    Ok(())
}

// ============================================================
// Unified request/response
// ============================================================

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

/// Send a chat completion request to the configured provider
pub async fn chat_completion(
    state: &AppState,
    messages: Vec<(String, String)>, // (role, content) pairs
) -> Result<String, anyhow::Error> {
    let provider = selected_provider(state)
        .await?
        .ok_or_else(|| anyhow::anyhow!("No AI provider configured"))?;
    let config = provider_config(state, provider).await?;

    match provider {
        AiProvider::Ollama => ollama_chat(messages, &config).await,
        AiProvider::OpenAI | AiProvider::Xai => openai_chat(messages, &config).await,
        AiProvider::Anthropic => anthropic_chat(messages, &config).await,
    }
}

// ============================================================
// Ollama
// ============================================================

#[derive(Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
}

#[derive(Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
}

#[derive(Deserialize)]
struct OllamaMessage {
    content: String,
}

async fn ollama_chat(
    messages: Vec<(String, String)>,
    config: &ProviderConfig,
) -> Result<String, anyhow::Error> {
    let request = OllamaRequest {
        model: config.model.clone(),
        messages: messages
            .into_iter()
            .map(|(role, content)| Message { role, content })
            .collect(),
        stream: false,
    };

    let client = Client::new();
    let resp: OllamaResponse = client
        .post(format!("{}/api/chat", config.base_url))
        .json(&request)
        .send()
        .await?
        .json()
        .await?;

    Ok(resp.message.content)
}

// ============================================================
// OpenAI-compatible (OpenAI + xAI)
// ============================================================

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    choices: Vec<OpenAIChoice>,
}

#[derive(Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
}

#[derive(Deserialize)]
struct OpenAIMessage {
    content: String,
}

async fn openai_chat(
    messages: Vec<(String, String)>,
    config: &ProviderConfig,
) -> Result<String, anyhow::Error> {
    let api_key = config
        .api_key
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Provider API key is not configured"))?;

    let request = OpenAIRequest {
        model: config.model.clone(),
        messages: messages
            .into_iter()
            .map(|(role, content)| Message { role, content })
            .collect(),
    };

    let client = Client::new();
    let resp: OpenAIResponse = client
        .post(format!("{}/chat/completions", config.base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request)
        .send()
        .await?
        .json()
        .await?;

    resp.choices
        .first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| anyhow::anyhow!("No response from AI provider"))
}

// ============================================================
// Anthropic
// ============================================================

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: String,
}

async fn anthropic_chat(
    messages: Vec<(String, String)>,
    config: &ProviderConfig,
) -> Result<String, anyhow::Error> {
    let api_key = config
        .api_key
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("Provider API key is not configured"))?;
    // Anthropic requires system message to be separate
    let mut system_msg = String::new();
    let mut chat_messages = Vec::new();

    for (role, content) in messages {
        if role == "system" {
            system_msg = content;
        } else {
            chat_messages.push(Message { role, content });
        }
    }

    let request = AnthropicRequest {
        model: config.model.clone(),
        max_tokens: 4096,
        messages: chat_messages,
    };

    let client = Client::new();
    let mut req_builder = client
        .post(format!("{}/messages", config.base_url))
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01");

    if !system_msg.is_empty() {
        // Anthropic expects system as a top-level field
        let mut body = serde_json::to_value(&request)?;
        body["system"] = serde_json::Value::String(system_msg);
        req_builder = req_builder.json(&body);
    } else {
        req_builder = req_builder.json(&request);
    }

    let resp: AnthropicResponse = req_builder.send().await?.json().await?;

    resp.content
        .first()
        .map(|c| c.text.clone())
        .ok_or_else(|| anyhow::anyhow!("No response from Anthropic"))
}

#[derive(Deserialize)]
struct OllamaModelsResponse {
    models: Vec<OllamaModel>,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

#[derive(Deserialize)]
struct ApiModelsResponse {
    data: Vec<ApiModel>,
}

#[derive(Deserialize)]
struct ApiModel {
    id: String,
}

pub async fn discover_models(config: &ProviderConfig) -> Result<Vec<String>, anyhow::Error> {
    let client = Client::builder().timeout(Duration::from_secs(15)).build()?;
    let models = match config.provider {
        AiProvider::Ollama => {
            let response = client
                .get(format!("{}/api/tags", config.base_url))
                .send()
                .await?
                .error_for_status()?
                .json::<OllamaModelsResponse>()
                .await?;
            response
                .models
                .into_iter()
                .map(|model| model.name)
                .collect::<Vec<_>>()
        }
        AiProvider::OpenAI | AiProvider::Xai => {
            let api_key = config
                .api_key
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("Provider API key is not configured"))?;
            client
                .get(format!("{}/models", config.base_url))
                .bearer_auth(api_key)
                .send()
                .await?
                .error_for_status()?
                .json::<ApiModelsResponse>()
                .await?
                .data
                .into_iter()
                .map(|model| model.id)
                .collect::<Vec<_>>()
        }
        AiProvider::Anthropic => {
            let api_key = config
                .api_key
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("Provider API key is not configured"))?;
            client
                .get(format!("{}/models", config.base_url))
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .send()
                .await?
                .error_for_status()?
                .json::<ApiModelsResponse>()
                .await?
                .data
                .into_iter()
                .map(|model| model.id)
                .collect::<Vec<_>>()
        }
    };

    Ok(normalize_models(models, &config.model))
}

fn normalize_models(mut models: Vec<String>, selected_model: &str) -> Vec<String> {
    if !models.iter().any(|model| model == selected_model) {
        models.push(selected_model.to_string());
    }
    models
        .into_iter()
        .filter(|model| !model.trim().is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(500)
        .collect()
}

// ============================================================
// System prompts for doc actions
// ============================================================

pub fn build_action_prompt(
    action: &str,
    content: &str,
    custom_prompt: Option<&str>,
    language: Option<&str>,
) -> Vec<(String, String)> {
    let system = match action {
        "summarize" => "You are a helpful assistant. Summarize the following document concisely, preserving key points and structure. Use markdown formatting.".to_string(),
        "translate" => format!(
            "You are a helpful translator. Translate the following document to {}. Preserve all markdown formatting.",
            language.unwrap_or("English")
        ),
        "improve" => "You are a professional editor. Improve the writing quality of the following document. Fix grammar, improve clarity, and enhance readability. Preserve the original meaning and markdown formatting.".to_string(),
        "extract_actions" => "You are a helpful assistant. Extract all action items, tasks, and to-dos from the following document. Return them as a markdown task list.".to_string(),
        "custom" => custom_prompt.unwrap_or("You are a helpful assistant.").to_string(),
        _ => "You are a helpful assistant.".to_string(),
    };

    vec![
        ("system".to_string(), system),
        ("user".to_string(), content.to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_provider_metadata_is_stable() {
        let providers = SUPPORTED_PROVIDERS
            .into_iter()
            .map(|provider| {
                (
                    provider.id(),
                    provider.label(),
                    provider.default_base_url(),
                    provider.default_model(),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            providers,
            vec![
                ("ollama", "Ollama", "http://localhost:11434", "llama3.2"),
                ("openai", "OpenAI", "https://api.openai.com/v1", "gpt-4o"),
                (
                    "anthropic",
                    "Anthropic",
                    "https://api.anthropic.com/v1",
                    "claude-sonnet-4-20250514"
                ),
                ("xai", "xAI", "https://api.x.ai/v1", "grok-2-latest"),
            ]
        );
    }

    #[test]
    fn provider_parser_rejects_unknown_values() {
        assert_eq!(AiProvider::parse(" OpenAI "), Some(AiProvider::OpenAI));
        assert_eq!(AiProvider::parse("custom"), None);
    }

    #[test]
    fn provider_urls_require_http_transport() {
        assert!(validate_base_url("https://api.example.test/v1").is_ok());
        assert!(validate_base_url("http://ollama:11434").is_ok());
        assert!(validate_base_url("file:///tmp/provider").is_err());
        assert!(validate_base_url("not a url").is_err());
    }

    #[test]
    fn connection_model_results_are_deduplicated_sorted_and_keep_the_selection() {
        assert_eq!(
            normalize_models(
                vec![
                    "zeta".to_string(),
                    "alpha".to_string(),
                    "alpha".to_string(),
                    String::new(),
                ],
                "selected",
            ),
            vec!["alpha", "selected", "zeta"]
        );
    }
}
