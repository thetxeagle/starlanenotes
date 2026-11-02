import { describe, expect, it } from "vitest";
import { applyTemplatePageWidth } from "./templatePresentationContent";

describe("applyTemplatePageWidth", () => {
  it("adds page width without changing template body text", () => {
    expect(applyTemplatePageWidth("# Brief\n\nBody", { page_width: "wide" })).toBe(
      "---\npage_width: wide\n---\n\n# Brief\n\nBody",
    );
  });

  it("updates only width and preserves existing frontmatter", () => {
    expect(
      applyTemplatePageWidth("---\nsynopsis: \"Keep me\"\npage_width: standard\n---\n\n# Brief", {
        page_width: "full",
      }),
    ).toBe("---\nsynopsis: \"Keep me\"\npage_width: full\n---\n\n# Brief");
  });

  it("leaves content unchanged when no width is configured", () => {
    expect(applyTemplatePageWidth("# Brief", {})).toBe("# Brief");
  });
});
