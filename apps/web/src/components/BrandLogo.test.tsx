import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { BrandLogo } from "./BrandLogo";

describe("BrandLogo", () => {
  it("keeps the default product lockup for setup and auth screens", () => {
    const html = renderToStaticMarkup(<BrandLogo size="lg" />);

    expect(html).toContain("Starlane Notes");
    expect(html).toContain("Open-source notes workspace");
  });

  it("renders a compact sidebar lockup with Starlane above Notes", () => {
    const html = renderToStaticMarkup(<BrandLogo size="sm" variant="sidebar" />);

    expect(html).toContain("Starlane");
    expect(html).toContain("Notes");
    expect(html).toContain("Exo 2");
    expect(html).not.toContain("Open-source notes workspace");
  });
});
