import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { BrandLogo, NOTES_BRAND_SUBTITLE } from "./BrandLogo";

describe("BrandLogo", () => {
  it("keeps the default product lockup for setup and auth screens", () => {
    const html = renderToStaticMarkup(<BrandLogo size="lg" />);

    expect(html).toContain("Starlane Notes");
    expect(html).toContain(NOTES_BRAND_SUBTITLE);
  });

  it("uses the same full workspace lockup in the expanded sidebar", () => {
    const html = renderToStaticMarkup(<BrandLogo size="md" variant="sidebar" />);

    expect(html).toContain("Starlane Notes");
    expect(html).toContain(NOTES_BRAND_SUBTITLE);
    expect(html).toContain("text-base");
    expect(html).toContain("text-xs");
    expect(html).toContain("min-w-0");
    expect(html).toContain("flex-1");
  });

  it("uses the full product lockup in the mobile workspace header", () => {
    const html = renderToStaticMarkup(<BrandLogo size="md" variant="mobile-header" />);

    expect(html).toContain("Starlane Notes");
    expect(html).toContain(NOTES_BRAND_SUBTITLE);
    expect(html).toContain("text-base");
    expect(html).toContain("text-xs");
  });
});
