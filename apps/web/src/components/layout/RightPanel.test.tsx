import { describe, expect, it } from "vitest";
import {
  formatAiDocumentContext,
  getRestoreVersionConfirmationMessage,
  isRightPanelTabAvailable,
  RIGHT_PANEL_SCROLL_CONTENT_CLASS,
} from "./RightPanel";

describe("RightPanel tab availability", () => {
  it("keeps tab content vertically scrollable inside bounded mobile and desktop panels", () => {
    expect(RIGHT_PANEL_SCROLL_CONTENT_CLASS).toContain("flex-1");
    expect(RIGHT_PANEL_SCROLL_CONTENT_CLASS).toContain("overflow-y-auto");
  });

  it("hides server-only panels in local mode", () => {
    expect(isRightPanelTabAvailable("properties", "local")).toBe(true);
    expect(isRightPanelTabAvailable("contents", "local")).toBe(true);
    expect(isRightPanelTabAvailable("backlinks", "local")).toBe(true);
    expect(isRightPanelTabAvailable("history", "local")).toBe(true);
    expect(isRightPanelTabAvailable("comments", "local")).toBe(false);
    expect(isRightPanelTabAvailable("ai", "local")).toBe(false);
  });

  it("keeps comments and AI available in server mode", () => {
    expect(isRightPanelTabAvailable("comments", "server")).toBe(true);
    expect(isRightPanelTabAvailable("ai", "server")).toBe(true);
  });
});

describe("AI document context", () => {
  it("includes a title and bounds oversized document content", () => {
    expect(formatAiDocumentContext("Project brief", "Body")).toBe("# Project brief\n\nBody");
    const context = formatAiDocumentContext("", "x".repeat(24_001));
    expect(context).toContain("# Untitled document");
    expect(context).toContain("[Document context truncated]");
    expect(context).toHaveLength(24_000 + "# Untitled document\n\n".length + "\n\n[Document context truncated]".length);
  });
});

describe("RightPanel history confirmation", () => {
  it("uses clear restore-version confirmation copy", () => {
    expect(getRestoreVersionConfirmationMessage(7)).toBe(
      "Restore version 7? This creates a new history checkpoint.",
    );
  });
});
