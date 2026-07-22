import { useEffect, useRef, useState } from "react";
import { Send, Sparkles, Trash2 } from "lucide-react";
import { api } from "@/lib/api";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";

interface Message {
  role: "user" | "assistant";
  content: string;
}

interface AiConfig {
  enabled: boolean;
  default_provider: string;
  providers: string[];
}

interface AiChatPanelProps {
  documentContext?: string;
  documentContextLoading?: boolean;
  documentContextError?: boolean;
  onRetryDocumentContext?: () => void;
}

interface AiChatPromptInputProps {
  value: string;
  loading: boolean;
  onChange: (value: string) => void;
  onSubmit: () => void;
}

interface AiChatMessageBubbleProps {
  message: Message;
}

type AiDocumentAction = "summarize" | "translate" | "improve" | "extract_actions" | "custom";

interface AiDocumentActionOptions {
  customPrompt?: string;
  language?: string;
}

export interface AiDocumentActionRequest {
  action: AiDocumentAction;
  content: string | undefined;
  custom_prompt: string | null;
  language: string | null;
}

interface AiDocumentActionDefinition {
  action: AiDocumentAction;
  label: string;
  promptLabel: string;
}

interface AiDocumentActionButtonsProps {
  disabled?: boolean;
  loadingAction?: AiDocumentAction | null;
  onAction: (action: AiDocumentAction, options?: AiDocumentActionOptions) => void;
}

export const AI_DOCUMENT_ACTIONS: AiDocumentActionDefinition[] = [
  {
    action: "summarize",
    label: "Summarize",
    promptLabel: "Summarize this document",
  },
  {
    action: "improve",
    label: "Improve writing",
    promptLabel: "Improve this document",
  },
  {
    action: "extract_actions",
    label: "Extract actions",
    promptLabel: "Extract action items",
  },
];

export function AiChatPromptInput({
  value,
  loading,
  onChange,
  onSubmit,
}: AiChatPromptInputProps) {
  return (
    <div className="flex gap-1">
      <Input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && onSubmit()}
        placeholder="Ask AI..."
        aria-label="Ask AI"
        disabled={loading}
        className="h-8 flex-1 border-ai/20 bg-bg px-3 py-1.5 text-sm focus:ring-ai focus:ring-offset-0"
      />
      <button
        onClick={onSubmit}
        disabled={loading || !value.trim()}
        className="flex h-8 w-8 items-center justify-center rounded-button bg-ai text-white transition-opacity duration-100 hover:opacity-90 disabled:opacity-40"
        aria-label="Send AI message"
      >
        <Send size={14} />
      </button>
    </div>
  );
}

export function AiChatMessageBubble({ message }: AiChatMessageBubbleProps) {
  return (
    <div
      className={`inline-block max-w-[90%] rounded-card px-3 py-2 text-sm ${
        message.role === "user"
          ? "bg-surface-raised text-text"
          : "border border-ai/20 bg-ai/5 text-text"
      }`}
    >
      <p className="whitespace-pre-wrap">{message.content}</p>
    </div>
  );
}

export function AiDocumentActionButtons({
  disabled = false,
  loadingAction = null,
  onAction,
}: AiDocumentActionButtonsProps) {
  const [translationLanguage, setTranslationLanguage] = useState("English");
  const [customPrompt, setCustomPrompt] = useState("");
  const actionDisabled = disabled || Boolean(loadingAction);
  const translateDisabled = actionDisabled || !translationLanguage.trim();
  const customDisabled = actionDisabled || !customPrompt.trim();

  return (
    <div className="space-y-2">
      {AI_DOCUMENT_ACTIONS.map((item) => {
        const loading = loadingAction === item.action;
        return (
          <button
            key={item.action}
            type="button"
            disabled={actionDisabled}
            onClick={() => onAction(item.action)}
            className="block w-full rounded-button border border-ai/20 px-3 py-1.5 text-left text-xs text-ai transition-colors duration-100 hover:border-ai/40 hover:bg-ai/5 disabled:opacity-50"
            aria-label={`${item.label} document with AI`}
          >
            {loading ? `${item.label}...` : item.label}
          </button>
        );
      })}
      <div className="grid gap-1">
        <Input
          value={translationLanguage}
          onChange={(event) => setTranslationLanguage(event.target.value)}
          aria-label="AI translation language"
          placeholder="Language"
          disabled={actionDisabled}
          className="h-8 border-ai/20 bg-bg px-3 py-1.5 text-xs focus:ring-ai focus:ring-offset-0"
        />
        <button
          type="button"
          disabled={translateDisabled}
          onClick={() => onAction("translate", { language: translationLanguage })}
          className="block w-full rounded-button border border-ai/20 px-3 py-1.5 text-left text-xs text-ai transition-colors duration-100 hover:border-ai/40 hover:bg-ai/5 disabled:opacity-50"
          aria-label="Translate document with AI"
        >
          {loadingAction === "translate" ? "Translating..." : "Translate"}
        </button>
      </div>
      <div className="grid gap-1">
        <Textarea
          value={customPrompt}
          onChange={(event) => setCustomPrompt(event.target.value)}
          aria-label="Custom AI document action"
          placeholder="Custom instruction"
          disabled={actionDisabled}
          className="min-h-16 border-ai/20 bg-bg px-3 py-2 text-xs focus:ring-ai focus:ring-offset-0"
        />
        <button
          type="button"
          disabled={customDisabled}
          onClick={() => onAction("custom", { customPrompt })}
          className="block w-full rounded-button border border-ai/20 px-3 py-1.5 text-left text-xs text-ai transition-colors duration-100 hover:border-ai/40 hover:bg-ai/5 disabled:opacity-50"
          aria-label="Run custom AI document action"
        >
          {loadingAction === "custom" ? "Running custom action..." : "Run custom action"}
        </button>
      </div>
    </div>
  );
}

export function getAiDocumentActionPromptLabel(
  action: AiDocumentAction,
  options: AiDocumentActionOptions = {},
): string {
  if (action === "translate") {
    return `Translate this document to ${options.language?.trim() || "English"}`;
  }
  if (action === "custom") {
    return options.customPrompt?.trim() || "Run custom document action";
  }
  return AI_DOCUMENT_ACTIONS.find((item) => item.action === action)?.promptLabel ?? "Run document action";
}

export function canRunAiDocumentAction(
  action: AiDocumentAction,
  options: AiDocumentActionOptions = {},
  state: {
    hasDocumentContext: boolean;
    loading: boolean;
    actionLoading: AiDocumentAction | null;
  },
): boolean {
  if (!state.hasDocumentContext || state.loading || state.actionLoading) return false;
  if (action === "translate") return Boolean(options.language?.trim());
  if (action === "custom") return Boolean(options.customPrompt?.trim());
  return true;
}

export function buildAiDocumentActionRequest(
  action: AiDocumentAction,
  documentContext: string | undefined,
  options: AiDocumentActionOptions = {},
): AiDocumentActionRequest {
  return {
    action,
    content: documentContext,
    custom_prompt: action === "custom" ? options.customPrompt?.trim() || null : null,
    language: action === "translate" ? options.language?.trim() || null : null,
  };
}

export function AiChatPanel({
  documentContext,
  documentContextLoading = false,
  documentContextError = false,
  onRetryDocumentContext,
}: AiChatPanelProps) {
  const [config, setConfig] = useState<AiConfig | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [actionLoading, setActionLoading] = useState<AiDocumentAction | null>(null);
  const [configLoading, setConfigLoading] = useState(true);
  const [configError, setConfigError] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const hasDocumentContext = Boolean(documentContext?.trim());

  const loadConfig = async () => {
    setConfigLoading(true);
    setConfigError("");
    try {
      setConfig(await api.fetch<AiConfig>("/ai/config"));
    } catch {
      setConfigError("Unable to load AI availability. Check the server connection and try again.");
    } finally {
      setConfigLoading(false);
    }
  };

  useEffect(() => {
    void loadConfig();
  }, []);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [messages]);

  const sendMessage = async () => {
    if (!input.trim() || loading || actionLoading) return;

    const userMsg: Message = { role: "user", content: input };
    const updated = [...messages, userMsg];
    setMessages(updated);
    setInput("");
    setLoading(true);

    try {
      const resp = await api.fetch<{ message: Message }>("/ai/chat", {
        method: "POST",
        body: {
          messages: updated.map((m) => ({ role: m.role, content: m.content })),
          document_context: documentContext,
        },
      });

      setMessages([...updated, resp.message]);
    } catch (err) {
      setMessages([
        ...updated,
        {
          role: "assistant",
          content: err instanceof Error ? err.message : "Failed to get response",
        },
      ]);
    } finally {
      setLoading(false);
    }
  };

  const runDocumentAction = async (action: AiDocumentAction, options: AiDocumentActionOptions = {}) => {
    if (!canRunAiDocumentAction(action, options, { hasDocumentContext, loading, actionLoading })) {
      return;
    }

    const userMsg: Message = {
      role: "user",
      content: getAiDocumentActionPromptLabel(action, options),
    };
    const updated = [...messages, userMsg];
    setMessages(updated);
    setActionLoading(action);

    try {
      const resp = await api.fetch<{ result: string }>("/ai/action", {
        method: "POST",
        body: buildAiDocumentActionRequest(action, documentContext, options),
      });

      setMessages([
        ...updated,
        {
          role: "assistant",
          content: resp.result,
        },
      ]);
    } catch (err) {
      setMessages([
        ...updated,
        {
          role: "assistant",
          content: err instanceof Error ? err.message : "Failed to run document action",
        },
      ]);
    } finally {
      setActionLoading(null);
    }
  };

  if (configLoading) {
    return (
      <div className="flex h-full flex-col items-center justify-center p-4">
        <Sparkles size={24} className="mb-2 text-ai" />
        <p className="text-center text-sm text-text-muted">Loading AI availability...</p>
      </div>
    );
  }

  if (configError) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-3 p-4">
        <Sparkles size={24} className="text-ai" />
        <p className="text-center text-sm text-text-muted">{configError}</p>
        <button
          type="button"
          onClick={() => void loadConfig()}
          className="rounded-button border border-ai/20 px-3 py-1.5 text-xs text-ai transition-colors duration-100 hover:bg-ai/5"
        >
          Retry
        </button>
      </div>
    );
  }

  if (!config?.enabled) {
    return (
      <div className="flex h-full flex-col items-center justify-center p-4">
        <Sparkles size={24} className="mb-2 text-ai" />
        <p className="text-center text-sm text-text-muted">
          AI features are not enabled.
        </p>
        <p className="mt-1 text-center text-xs text-text-muted">
          Set <code className="font-mono text-ai">AI_ENABLED=true</code> and configure a provider.
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-ai/20 bg-ai/5 px-3 py-2">
        <div className="flex items-center gap-1.5">
          <Sparkles size={14} className="text-ai" />
          <span className="text-xs font-medium text-ai">AI Chat</span>
          <span className="text-[10px] text-text-muted">({config.default_provider})</span>
        </div>
        {messages.length > 0 && (
          <button
            onClick={() => setMessages([])}
            className="flex h-5 w-5 items-center justify-center rounded-button text-text-muted transition-colors duration-100 hover:text-text"
            aria-label="Clear chat"
          >
            <Trash2 size={12} />
          </button>
        )}
      </div>

      {/* Messages */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto p-3">
        {messages.length === 0 ? (
          <div className="py-8 text-center">
            <p className="text-sm text-text-muted">Ask AI about this document</p>
            <div className="mt-3 space-y-1">
              {["Summarize this document", "What are the key points?", "Suggest improvements"].map(
                (suggestion) => (
                  <button
                    key={suggestion}
                    onClick={() => {
                      setInput(suggestion);
                    }}
                    className="block w-full rounded-button border border-ai/20 px-3 py-1.5 text-left text-xs text-text-muted transition-colors duration-100 hover:border-ai/40 hover:text-ai"
                  >
                    {suggestion}
                  </button>
                ),
              )}
            </div>
          </div>
        ) : (
          messages.map((msg, i) => (
            <div
              key={i}
              className={`mb-3 ${msg.role === "user" ? "text-right" : ""}`}
            >
              <AiChatMessageBubble message={msg} />
            </div>
          ))
        )}

        {(loading || actionLoading || documentContextLoading) && (
          <div className="mb-3">
            <div className="inline-block rounded-card border border-ai/20 bg-ai/5 px-3 py-2">
              <span className="text-sm text-ai">
                {documentContextLoading
                  ? "Loading current document..."
                  : actionLoading
                    ? "Running document action..."
                    : "Thinking..."}
              </span>
            </div>
          </div>
        )}
      </div>

      {/* Input */}
      <div className="border-t border-ai/20 p-3">
        {hasDocumentContext && (
          <div className="mb-3">
            <p className="mb-2 text-xs font-medium text-ai">Document actions</p>
            <AiDocumentActionButtons
              disabled={documentContextLoading || documentContextError}
              loadingAction={actionLoading}
              onAction={(action, options) => {
                void runDocumentAction(action, options);
              }}
            />
          </div>
        )}
        {documentContextError && (
          <div className="mb-2 flex items-center justify-between gap-2 text-xs text-warning">
            <span>Current document context is unavailable.</span>
            {onRetryDocumentContext && (
              <button
                type="button"
                onClick={onRetryDocumentContext}
                className="rounded-button border border-warning/40 px-2 py-1 text-xs transition-colors duration-100 hover:bg-warning/10"
              >
                Retry
              </button>
            )}
          </div>
        )}
        <AiChatPromptInput
          value={input}
          loading={loading || Boolean(actionLoading) || documentContextLoading}
          onChange={setInput}
          onSubmit={sendMessage}
        />
      </div>
    </div>
  );
}
