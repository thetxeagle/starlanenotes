use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone)]
pub enum AiProvider {
    Ollama,
    OpenAI,
    Anthropic,
    Xai,
}

impl AiProvider {
    pub fn from_env() -> Option<Self> {
        let provider = env::var("AI_DEFAULT_PROVIDER").unwrap_or_else(|_| "ollama".to_string());
        match provider.as_str() {
            "ollama" => Some(AiProvider::Ollama),
            "openai" => Some(AiProvider::OpenAI),
            "anthropic" => Some(AiProvider::Anthropic),
            "xai" => Some(AiProvider::Xai),
            _ => None,
        }
    }
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
    messages: Vec<(String, String)>, // (role, content) pairs
) -> Result<String, anyhow::Error> {
    let provider =
        AiProvider::from_env().ok_or_else(|| anyhow::anyhow!("No AI provider configured"))?;

    match provider {
        AiProvider::Ollama => ollama_chat(messages).await,
        AiProvider::OpenAI => {
            openai_chat(messages, "https://api.openai.com/v1", "OPENAI_API_KEY").await
        }
        AiProvider::Anthropic => anthropic_chat(messages).await,
        AiProvider::Xai => openai_chat(messages, "https://api.x.ai/v1", "XAI_API_KEY").await,
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

async fn ollama_chat(messages: Vec<(String, String)>) -> Result<String, anyhow::Error> {
    let base_url =
        env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| "http://localhost:11434".to_string());

    let model = env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2".to_string());

    let request = OllamaRequest {
        model,
        messages: messages
            .into_iter()
            .map(|(role, content)| Message { role, content })
            .collect(),
        stream: false,
    };

    let client = Client::new();
    let resp: OllamaResponse = client
        .post(format!("{}/api/chat", base_url))
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
    base_url: &str,
    api_key_env: &str,
) -> Result<String, anyhow::Error> {
    let api_key = env::var(api_key_env).map_err(|_| anyhow::anyhow!("{} not set", api_key_env))?;

    let model = if api_key_env == "XAI_API_KEY" {
        env::var("XAI_MODEL").unwrap_or_else(|_| "grok-2-latest".to_string())
    } else {
        env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o".to_string())
    };

    let request = OpenAIRequest {
        model,
        messages: messages
            .into_iter()
            .map(|(role, content)| Message { role, content })
            .collect(),
    };

    let client = Client::new();
    let resp: OpenAIResponse = client
        .post(format!("{}/chat/completions", base_url))
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

async fn anthropic_chat(messages: Vec<(String, String)>) -> Result<String, anyhow::Error> {
    let api_key =
        env::var("ANTHROPIC_API_KEY").map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;

    let model =
        env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());

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
        model,
        max_tokens: 4096,
        messages: chat_messages,
    };

    let client = Client::new();
    let mut req_builder = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
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
