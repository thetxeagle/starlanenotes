import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { canSavePageIconInput, getInitialPageIconInput, PageIcon } from "./PageIcon";

describe("PageIcon", () => {
  it("normalizes page icon edit input", () => {
    expect(getInitialPageIconInput(" 🚀 ")).toBe("🚀");
    expect(getInitialPageIconInput()).toBe("");
    expect(canSavePageIconInput("📝")).toBe(true);
    expect(canSavePageIconInput("   ")).toBe(false);
  });

  it("renders a Starlane edit affordance instead of using browser prompts", () => {
    const html = renderToStaticMarkup(<PageIcon icon="🚀" onUpdate={() => undefined} />);

    expect(html).toContain('aria-label="Edit page icon"');
    expect(html).toContain("🚀");
    expect(html).not.toContain("prompt");
  });
});
