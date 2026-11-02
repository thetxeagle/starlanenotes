import { describe, expect, it } from "vitest";
import { markdownToHtml, splitMarkdownSectionSegments } from "@/components/editor/Editor";
import { WORKSPACE_FIXTURE_CONTENT } from "./fixture";

describe("workspace review fixture portability", () => {
  it("parses a populated two-column section without flattening its boundary", () => {
    const columns = splitMarkdownSectionSegments(WORKSPACE_FIXTURE_CONTENT).find(
      (segment) => segment.type === "section" && segment.kind === "columns",
    );
    expect(columns?.type).toBe("section");
    if (columns?.type !== "section") throw new Error("Missing fixture columns");
    expect(columns.columns).toHaveLength(2);
    expect(columns.columns[0]).toContain("### Diagnose");
    expect(columns.columns[1]).toContain("### Recover");
  });

  it("renders supported complex content through the production Markdown parser", () => {
    const source = WORKSPACE_FIXTURE_CONTENT.replace(
      "/workspace-prototype-architecture.svg",
      "http://127.0.0.1:5199/workspace-prototype-architecture.svg",
    );
    const html = markdownToHtml(source);
    expect(html).toContain("<table");
    expect(html).toContain("<pre");
    expect(html).toContain("<img");
    expect(html).toContain("starlane:document/doc-sync-retry-reconciliation-7c2a");
    expect(html).toContain('data-callout="true"');
    expect(html).toContain('data-starlane-columns="true"');
    expect(html).toContain("Prototype fixture");
  });
});
