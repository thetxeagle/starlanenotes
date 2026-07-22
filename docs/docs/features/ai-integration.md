# AI Integration

Starlane Notes provides optional AI features for document assistance, inline completion, and document-level actions. AI must be explicitly enabled and requires at least one provider to be configured.

## Enabling AI

Set the following environment variable:

```
AI_ENABLED=true
```

When disabled (default), all `/ai/*` endpoints return `400 Bad Request` with the message "AI features are not enabled."

## Supported Providers

| Provider | Env Variable for Detection | Default Model |
|---|---|---|
| Ollama | `OLLAMA_BASE_URL` | `llama3.2` |
| OpenAI | `OPENAI_API_KEY` | `gpt-4o` |
| Anthropic | `ANTHROPIC_API_KEY` | `claude-sonnet-4-20250514` |
| xAI | `XAI_API_KEY` | `grok-2-latest` |

The active provider is selected by `AI_DEFAULT_PROVIDER` (defaults to `ollama`).

## Environment Variables

| Variable | Default | Description |
|---|---|---|
| `AI_ENABLED` | `false` | Master switch for AI features |
| `AI_DEFAULT_PROVIDER` | `ollama` | Active provider: `ollama`, `openai`, `anthropic`, `xai` |
| `OLLAMA_BASE_URL` | `http://localhost:11434` | Ollama server URL |
| `OLLAMA_MODEL` | `llama3.2` | Ollama model name |
| `OPENAI_API_KEY` | -- | OpenAI API key |
| `OPENAI_MODEL` | `gpt-4o` | OpenAI model name |
| `ANTHROPIC_API_KEY` | -- | Anthropic API key |
| `ANTHROPIC_MODEL` | `claude-sonnet-4-20250514` | Anthropic model name |
| `XAI_API_KEY` | -- | xAI API key |
| `XAI_MODEL` | `grok-2-latest` | xAI model name |

## Endpoints

All AI endpoints require authentication.

### Configuration

```
GET /api/v1/ai/config
```

Returns the current AI configuration without exposing API keys:

```json
{
  "enabled": true,
  "default_provider": "ollama",
  "providers": ["ollama", "openai"]
}
```

The `providers` list includes only providers whose environment variables are set.

### Chat

```
POST /api/v1/ai/chat
```

Conversational chat with optional document context. Used for the AI sidebar.

```json
{
  "messages": [
    { "role": "user", "content": "Summarize the key decisions in this document." }
  ],
  "document_context": "# Sprint Planning\n\nWe decided to..."
}
```

When `document_context` is provided, it is injected into the system prompt so the AI can answer questions about the current document.

In the server-linked document right panel, Starlane Notes fetches the current readable document only when the AI tab opens and sends a title-prefixed context capped at 24,000 characters. The prompt is disabled while that context loads; a failed context fetch is visible and retryable rather than silently presenting document-aware chat without a document.

The current app UI exposes this document-aware chat path in the server-linked document right panel. When document context is available, that panel also exposes one-click document actions for `summarize`, `improve`, and `extract_actions`, plus bounded input controls for `translate` and `custom`, all backed by `/ai/action`. Missing document context, busy state, and blank translate/custom prompts are blocked before dispatch.

**Response:**

```json
{
  "message": {
    "role": "assistant",
    "content": "The document outlines three key decisions..."
  }
}
```

### Inline Completion

```
POST /api/v1/ai/complete
```

Generates content based on a prompt, used for the `/ai` slash command in the editor.

```json
{
  "prompt": "Write a project status update for Q1",
  "context": "Optional surrounding document text for context"
}
```

**Response:**

```json
{
  "completion": "## Q1 Status Update\n\n..."
}
```

### Document Actions

```
POST /api/v1/ai/action
```

Applies a predefined or custom action to document content.

```json
{
  "action": "summarize",
  "content": "The full document text...",
  "custom_prompt": null,
  "language": null
}
```

| Field | Type | Required | Description |
|---|---|---|---|
| `action` | string | yes | Action type (see below) |
| `content` | string | yes | Document content to process |
| `custom_prompt` | string | no | System prompt for `custom` action |
| `language` | string | no | Target language for `translate` action |

#### Built-in Actions

| Action | Description |
|---|---|
| `summarize` | Generate a concise summary preserving key points |
| `translate` | Translate to a target language (set via `language` field) |
| `improve` | Fix grammar, improve clarity and readability |
| `extract_actions` | Extract action items as a markdown task list |
| `custom` | Use the `custom_prompt` field as the system prompt |

**Response:**

```json
{
  "result": "The processed content..."
}
```
