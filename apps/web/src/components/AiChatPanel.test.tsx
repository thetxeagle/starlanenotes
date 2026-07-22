import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  AI_DOCUMENT_ACTIONS,
  AiChatMessageBubble,
  AiChatPanel,
  AiChatPromptInput,
  AiDocumentActionButtons,
  buildAiDocumentActionRequest,
  canRunAiDocumentAction,
  getAiDocumentActionPromptLabel,
} from "./AiChatPanel";

describe("AiChatPromptInput", () => {
  it("uses the shared input control for the AI prompt", () => {
    const html = renderToStaticMarkup(
      <AiChatPromptInput
        value=""
        loading={false}
        onChange={() => undefined}
        onSubmit={() => undefined}
      />,
    );

    expect(html).toContain('aria-label="Ask AI"');
    expect(html).toContain("Ask AI...");
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
    expect(html).toContain('aria-label="Send AI message"');
  });
});

describe("AiChatPanel", () => {
  it("does not misrepresent an unrequested AI config as disabled", () => {
    const html = renderToStaticMarkup(<AiChatPanel />);

    expect(html).toContain("Loading AI availability...");
    expect(html).not.toContain("AI features are not enabled.");
  });

  it("renders each assistant response once with AI styling", () => {
    const html = renderToStaticMarkup(
      <AiChatMessageBubble
        message={{ role: "assistant", content: "Summarized document response" }}
      />,
    );

    expect(html.match(/Summarized document response/g)).toHaveLength(1);
    expect(html).toContain("border-ai/20");
    expect(html).toContain("bg-ai/5");
  });

  it("exposes bounded one-click document actions", () => {
    expect(AI_DOCUMENT_ACTIONS.map((item) => item.action)).toEqual([
      "summarize",
      "improve",
      "extract_actions",
    ]);

    const html = renderToStaticMarkup(
      <AiDocumentActionButtons onAction={() => undefined} />,
    );

    expect(html).toContain("Summarize");
    expect(html).toContain("Improve writing");
    expect(html).toContain("Extract actions");
    expect(html).toContain('aria-label="AI translation language"');
    expect(html).toContain("Translate");
    expect(html).toContain('aria-label="Custom AI document action"');
    expect(html).toContain("Run custom action");
    expect(html).toContain("border-ai/20");
    expect(html).toContain("rounded-input");
  });

  it("shows the active document action as loading", () => {
    const html = renderToStaticMarkup(
      <AiDocumentActionButtons
        loadingAction="summarize"
        onAction={() => undefined}
      />,
    );

    expect(html).toContain("Summarize...");
    expect(html).toContain("disabled");
  });

  it("describes translate and custom document actions", () => {
    expect(getAiDocumentActionPromptLabel("translate", { language: "Spanish" })).toBe(
      "Translate this document to Spanish",
    );
    expect(getAiDocumentActionPromptLabel("custom", { customPrompt: "Find contradictions" })).toBe(
      "Find contradictions",
    );
  });

  it("builds bounded AI document-action request payloads", () => {
    expect(
      buildAiDocumentActionRequest("translate", "Document body", { language: " Spanish " }),
    ).toEqual({
      action: "translate",
      content: "Document body",
      custom_prompt: null,
      language: "Spanish",
    });

    expect(
      buildAiDocumentActionRequest("custom", "Document body", {
        customPrompt: " Find contradictions ",
      }),
    ).toEqual({
      action: "custom",
      content: "Document body",
      custom_prompt: "Find contradictions",
      language: null,
    });

    expect(buildAiDocumentActionRequest("summarize", "Document body")).toEqual({
      action: "summarize",
      content: "Document body",
      custom_prompt: null,
      language: null,
    });
  });

  it("guards document actions against missing context, busy state, and blank bounded inputs", () => {
    const ready = { hasDocumentContext: true, loading: false, actionLoading: null };

    expect(canRunAiDocumentAction("summarize", {}, ready)).toBe(true);
    expect(
      canRunAiDocumentAction("summarize", {}, { ...ready, hasDocumentContext: false }),
    ).toBe(false);
    expect(canRunAiDocumentAction("summarize", {}, { ...ready, loading: true })).toBe(false);
    expect(
      canRunAiDocumentAction("summarize", {}, { ...ready, actionLoading: "improve" }),
    ).toBe(false);
    expect(canRunAiDocumentAction("translate", { language: "  " }, ready)).toBe(false);
    expect(canRunAiDocumentAction("translate", { language: "Spanish" }, ready)).toBe(true);
    expect(canRunAiDocumentAction("custom", { customPrompt: "  " }, ready)).toBe(false);
    expect(canRunAiDocumentAction("custom", { customPrompt: "Find contradictions" }, ready)).toBe(
      true,
    );
  });
});
