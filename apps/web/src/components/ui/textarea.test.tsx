import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { Textarea, TEXTAREA_CLASS } from "./textarea";

describe("Textarea", () => {
  it("uses the shared multiline form-control styling contract", () => {
    expect(TEXTAREA_CLASS).toContain("rounded-input");
    expect(TEXTAREA_CLASS).toContain("border-border");
    expect(TEXTAREA_CLASS).toContain("focus:ring-2");
    expect(TEXTAREA_CLASS).toContain("min-h-20");
    expect(TEXTAREA_CLASS).not.toContain("bg-transparent");
  });

  it("renders native textarea content with custom classes preserved", () => {
    const html = renderToStaticMarkup(
      <Textarea aria-label="Synopsis" className="resize-none" defaultValue="Short summary" />,
    );

    expect(html).toContain('aria-label="Synopsis"');
    expect(html).toContain("resize-none");
    expect(html).toContain("rounded-input");
    expect(html).toContain("Short summary");
  });
});
