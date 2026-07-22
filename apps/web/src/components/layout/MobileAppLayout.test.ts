import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import {
  canUseMobileRightPanel,
  MobileAppLayout,
  MOBILE_DOCUMENT_PANEL_BODY_CLASS,
} from "./MobileAppLayout";

vi.mock("@/components/BrandLogo", () => ({
  BrandLogo: () => "Brand logo",
}));

vi.mock("./Sidebar", () => ({
  Sidebar: () => createElement("div", { "aria-label": "Shared sidebar" }, "Shared sidebar"),
}));

describe("canUseMobileRightPanel", () => {
  it("allows the document panel on mobile document routes", () => {
    expect(canUseMobileRightPanel("/spaces/space-1/docs/doc-1")).toBe(true);
    expect(canUseMobileRightPanel("/spaces/space-1/docs/doc-1/")).toBe(true);
  });

  it("blocks the document panel on unrelated mobile and app routes", () => {
    expect(canUseMobileRightPanel("/profile")).toBe(false);
    expect(canUseMobileRightPanel("/mobile/profile")).toBe(false);
    expect(canUseMobileRightPanel("/mobile/search")).toBe(false);
    expect(canUseMobileRightPanel("/mobile/recent")).toBe(false);
    expect(canUseMobileRightPanel("/spaces/space-1/recent")).toBe(false);
    expect(canUseMobileRightPanel("/spaces/space-1/graph")).toBe(false);
  });
});

describe("MobileAppLayout", () => {
  it("keeps the mobile document panel body bounded under the fixed header", () => {
    expect(MOBILE_DOCUMENT_PANEL_BODY_CLASS).toContain("min-h-0");
    expect(MOBILE_DOCUMENT_PANEL_BODY_CLASS).toContain("flex-1");
    expect(MOBILE_DOCUMENT_PANEL_BODY_CLASS).toContain("overflow-hidden");
  });

  it("renders navigation, brand, and search in the persistent mobile header", () => {
    const html = renderToStaticMarkup(
      createElement(
        MemoryRouter,
        null,
        createElement(MobileAppLayout, null, createElement("main", null, "Content")),
      ),
    );

    expect(html).toContain('aria-label="Open navigation"');
    expect(html).toContain('aria-label="Mobile top bar"');
    expect(html).toContain('aria-label="Open search"');
    expect(html).not.toContain('aria-label="Open app settings"');
    expect(html).toContain("Brand logo");
    expect(html).not.toContain("Shared sidebar");
  });
});
