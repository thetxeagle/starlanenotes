import { describe, expect, it } from "vitest";
import { isDocumentPanelRoute, MOBILE_DOCUMENT_PANEL_CLASS } from "./AppLayout";

describe("isDocumentPanelRoute", () => {
  it("allows the properties panel only on document routes", () => {
    expect(isDocumentPanelRoute("/spaces/space-1/docs/doc-1")).toBe(true);
    expect(isDocumentPanelRoute("/spaces/space-1/docs/doc-1/")).toBe(true);
  });

  it("blocks the properties panel on non-document app routes", () => {
    expect(isDocumentPanelRoute("/")).toBe(false);
    expect(isDocumentPanelRoute("/spaces/space-1/docs")).toBe(false);
    expect(isDocumentPanelRoute("/spaces/space-1/graph")).toBe(false);
    expect(isDocumentPanelRoute("/spaces/space-1/journal")).toBe(false);
    expect(isDocumentPanelRoute("/spaces/space-1/starred")).toBe(false);
    expect(isDocumentPanelRoute("/spaces/space-1/workflows")).toBe(false);
    expect(isDocumentPanelRoute("/admin")).toBe(false);
    expect(isDocumentPanelRoute("/mobile/profile")).toBe(false);
  });
});

describe("MOBILE_DOCUMENT_PANEL_CLASS", () => {
  it("centers the mobile document properties dialog with viewport-bounded scrolling", () => {
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("left-1/2");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("top-1/2");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("-translate-x-1/2");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("-translate-y-1/2");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("h-[min(42rem,calc(100dvh-2rem))]");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("w-[min(38rem,calc(100vw-1.5rem))]");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("flex-col");
    expect(MOBILE_DOCUMENT_PANEL_CLASS).toContain("overflow-hidden");
  });
});
