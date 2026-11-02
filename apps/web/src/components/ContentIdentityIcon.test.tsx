import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  CONTENT_ICON_CATALOG,
  ContentIdentityIcon,
  getContentIconLabel,
  parseContentIcon,
} from "./ContentIdentityIcon";

describe("ContentIdentityIcon", () => {
  it("preserves legacy emoji values and supports namespaced emoji values", () => {
    expect(parseContentIcon("📘")).toEqual({ source: "legacy", value: "📘" });
    expect(parseContentIcon("emoji:🧠")).toEqual({ source: "emoji", value: "🧠" });
    expect(renderToStaticMarkup(<ContentIdentityIcon value="📘" />)).toContain("📘");
    expect(renderToStaticMarkup(<ContentIdentityIcon value="emoji:🧠" />)).toContain("🧠");
  });

  it("renders curated Lucide values with the shared optical stroke", () => {
    expect(CONTENT_ICON_CATALOG.length).toBeGreaterThanOrEqual(40);
    const html = renderToStaticMarkup(<ContentIdentityIcon value="lucide:book-open" size={20} />);

    expect(html).toContain("<svg");
    expect(html).toContain('width="20"');
    expect(html).toContain('stroke-width="1.75"');
    expect(getContentIconLabel("lucide:book-open")).toBe("Book");
  });

  it("uses the provided fallback for unknown or not-yet-resolved assets", () => {
    expect(
      renderToStaticMarkup(
        <ContentIdentityIcon value="asset:site-example" fallback={<span>Fallback</span>} />,
      ),
    ).toContain("Fallback");
    expect(
      renderToStaticMarkup(
        <ContentIdentityIcon value="lucide:not-in-catalog" fallback={<span>Fallback</span>} />,
      ),
    ).toContain("Fallback");
  });
});
