import { describe, expect, it } from "vitest";
import { parseDocumentHeadings } from "./documentOutline";

describe("document outline", () => {
  it("parses markdown headings with levels and line positions", () => {
    const headings = parseDocumentHeadings("# One\n\nParagraph\n\n## Two\n### Three");

    expect(headings).toEqual([
      { id: "heading-0", text: "One", level: 1, line: 0, position: 0 },
      { id: "heading-4", text: "Two", level: 2, line: 4, position: 0.8 },
      { id: "heading-5", text: "Three", level: 3, line: 5, position: 1 },
    ]);
  });

  it("uses rendered labels for emphasized and linked headings", () => {
    expect(parseDocumentHeadings("# **Hella Glitchy**\n## [Page edits](https://example.com)"))
      .toMatchObject([
        { text: "Hella Glitchy" },
        { text: "Page edits" },
      ]);
  });
});
