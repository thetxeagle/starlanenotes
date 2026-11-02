import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import {
  getMobileDocumentPanelLabel,
  MobileDocumentPanel,
  MOBILE_DOCUMENT_PANEL_BODY_CLASS,
  MOBILE_DOCUMENT_PANEL_CLASS,
} from "./MobileDocumentPanel";

vi.mock("./RightPanel", () => ({
  RightPanel: () => <div>Shared document contents</div>,
}));

describe("MobileDocumentPanel", () => {
  it("keeps mobile web and native document tools in one labelled, closable bottom sheet", () => {
    const html = renderToStaticMarkup(
      <MobileDocumentPanel
        open
        onClose={() => undefined}
        activeTab="properties"
        onTabChange={() => undefined}
      />,
    );

    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("inset-x-0 bottom-0");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("h-[min(42rem,calc(100dvh-1rem))]");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("rounded-t-card");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("pb-[env(safe-area-inset-bottom)]");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).not.toContain("left-1/2");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).not.toContain("-translate");
    expect(MOBILE_DOCUMENT_PANEL_BODY_CLASS).toContain("min-h-0");
    expect(MOBILE_DOCUMENT_PANEL_BODY_CLASS).toContain("overflow-hidden");
    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="mobile-document-panel-title"');
    expect(html).toContain('tabindex="-1"');
    expect(html).toContain("Properties panel");
    expect(html.match(/aria-label="Close properties panel"/g)).toHaveLength(2);
    expect(html).toContain("h-11 w-11");
    expect(html).toContain("Shared document contents");
  });

  it("labels comments and fallback document tabs by their active surface", () => {
    expect(getMobileDocumentPanelLabel("comments")).toBe("Comments panel");
    expect(getMobileDocumentPanelLabel("comments", true)).toBe("Draft discussion");
    expect(getMobileDocumentPanelLabel("history")).toBe("Document panel");
  });

  it("labels the comments dialog without adding a visible header", () => {
    const html = renderToStaticMarkup(
      <MobileDocumentPanel
        open
        onClose={() => undefined}
        activeTab="comments"
        onTabChange={() => undefined}
      />,
    );

    expect(html).toContain('aria-labelledby="mobile-document-panel-title"');
    expect(html).toContain('<h2 id="mobile-document-panel-title" class="sr-only">Comments panel</h2>');
    expect(html).not.toContain('class="text-sm font-medium text-text">Comments panel</h2>');
    expect(html).toContain('aria-label="Close comments panel"');
  });

  it("does not mount a closed document panel", () => {
    const html = renderToStaticMarkup(
      <MobileDocumentPanel
        open={false}
        onClose={() => undefined}
        activeTab="properties"
        onTabChange={() => undefined}
      />,
    );

    expect(html).toBe("");
  });
});
