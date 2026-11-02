import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  MathBlockEditor,
  MathBlockDisplay,
  parseMathBlockDirective,
  serializeMathBlockDirective,
} from "./MathBlock";

describe("math block directives", () => {
  it("round-trips a portable display-math directive", () => {
    const source = serializeMathBlockDirective("E = mc^2");

    expect(source).toBe("$$\nE = mc^2\n$$");
    expect(parseMathBlockDirective(source)).toBe("E = mc^2");
  });

  it("does not treat inline dollar content as a display-math block", () => {
    expect(parseMathBlockDirective("A value of $x$ is inline math.")).toBeNull();
  });

  it("renders KaTeX without trusting user-provided commands", () => {
    const html = renderToStaticMarkup(createElement(MathBlockDisplay, { expression: "\\frac{1}{2}" }));

    expect(html).toContain('data-math-rendered="true"');
    expect(html).toContain('aria-label="Rendered math: \\frac{1}{2}"');
    expect(html).toContain("katex");
  });

  it("gives every editable equation its own labelled field", () => {
    const html = renderToStaticMarkup(
      createElement(
        "div",
        null,
        createElement(MathBlockEditor, { expression: "x", onExpressionChange: () => undefined }),
        createElement(MathBlockEditor, { expression: "y", onExpressionChange: () => undefined }),
      ),
    );
    const ids = Array.from(html.matchAll(/<textarea[^>]*id="([^"]+)"/g)).map((match) => match[1]);

    expect(ids).toHaveLength(2);
    expect(new Set(ids).size).toBe(2);
    ids.forEach((id) => expect(html).toContain(`for="${id}"`));
  });
});
