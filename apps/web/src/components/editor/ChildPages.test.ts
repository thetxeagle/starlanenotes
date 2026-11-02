import { describe, expect, it } from "vitest";
import {
  CHILD_PAGES_DIRECTIVE,
  COLLAPSIBLE_CHILD_PAGES_DIRECTIVE,
  getDirectChildPages,
  parseChildPagesDirective,
} from "./ChildPages";
import { splitMarkdownDataviewBlocks } from "./MarkdownPreview";

describe("child pages blocks", () => {
  it("parses both portable directives", () => {
    expect(parseChildPagesDirective(CHILD_PAGES_DIRECTIVE)).toBe("list");
    expect(parseChildPagesDirective(COLLAPSIBLE_CHILD_PAGES_DIRECTIVE)).toBe("collapsible");
    expect(parseChildPagesDirective("[[toc]]")).toBeNull();
  });

  it("keeps only direct descendants and sorts them by title", () => {
    const children = getDirectChildPages(
      [
        { id: "nested", title: "Nested", parent_id: "child", updated_at: "2" },
        { id: "z", title: "Zebra", parent_id: "root", updated_at: "1" },
        { id: "a", title: "Alpha", parent_id: "root", updated_at: "1" },
      ],
      "root",
    );
    expect(children.map((child) => child.id)).toEqual(["a", "z"]);
  });

  it("recognizes child page directives in preview block splitting", () => {
    expect(splitMarkdownDataviewBlocks("before\n\n[[children:collapsed]]\n\nafter")).toEqual([
      { type: "markdown", content: "before\n", key: "markdown-0" },
      { type: "child-pages", variant: "collapsible", key: "child-pages-1" },
      { type: "markdown", content: "\nafter", key: "markdown-2" },
    ]);
  });
});
