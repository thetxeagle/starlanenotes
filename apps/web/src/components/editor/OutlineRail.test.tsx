import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { DocumentOutlineAccordion } from "./OutlineRail";
import { parseDocumentHeadings } from "@/lib/documentOutline";

describe("DocumentOutlineAccordion", () => {
  it("renders a closed, in-flow accordion for useful outlines", () => {
    const html = renderToStaticMarkup(
      createElement(DocumentOutlineAccordion, {
        content: "# Introduction\n\n## Details\n\n### Appendix",
      }),
    );

    expect(html).toContain('aria-expanded="false"');
    expect(html).toContain('aria-controls="document-outline-accordion"');
    expect(html).toContain("On this page");
    expect(html).toContain("border-border");
    expect(html).not.toContain("fixed");
    expect(html).not.toContain("top-1/2");
    expect(html).not.toContain("Introduction");
  });

  it("stays hidden for documents without a useful outline", () => {
    expect(
      renderToStaticMarkup(createElement(DocumentOutlineAccordion, { content: "# Only heading" })),
    ).toBe("");
  });

  it("keeps inline HTML marks out of heading labels", () => {
    const headings = parseDocumentHeadings(
      "# Intro\n\n## <span style=\"color: red\">App</span>",
    );

    expect(headings[0].text).toBe("Intro");
    expect(headings[1].text).toBe("App");
  });
});
