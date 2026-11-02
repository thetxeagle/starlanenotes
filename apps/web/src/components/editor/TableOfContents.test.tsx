import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  isTableOfContentsDirective,
  TABLE_OF_CONTENTS_DIRECTIVE,
  TableOfContentsList,
} from "./TableOfContents";

describe("TableOfContentsList", () => {
  it("recognizes only the canonical portable directive", () => {
    expect(isTableOfContentsDirective(TABLE_OF_CONTENTS_DIRECTIVE)).toBe(true);
    expect(isTableOfContentsDirective("  [[TOC]] ")).toBe(true);
    expect(isTableOfContentsDirective("[[toc]] extra")).toBe(false);
  });

  it("renders a labelled hierarchy with keyboard-native heading links", () => {
    const html = renderToStaticMarkup(
      createElement(TableOfContentsList, {
        entries: [
          { id: "one", text: "Introduction", level: 1 },
          { id: "two", text: "Details", level: 2 },
          { id: "three", text: "Appendix", level: 4 },
        ],
      }),
    );

    expect(html).toContain('aria-label="Table of contents"');
    expect(html).toContain('aria-label="Go to Introduction"');
    expect(html).toContain('aria-label="Go to Details"');
    expect(html).toContain('aria-label="Go to Appendix"');
    expect(html).toContain("pl-3");
    expect(html).toContain("pl-6");
    expect(html).toContain("rounded-card");
  });
});
