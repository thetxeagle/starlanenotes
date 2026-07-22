import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { CommentsPanel } from "./CommentsPanel";

describe("CommentsPanel", () => {
  it("uses the shared input control for comment entry", () => {
    const html = renderToStaticMarkup(<CommentsPanel spaceId="space-1" docId="doc-1" />);

    expect(html).toContain('aria-label="New comment"');
    expect(html).toContain("Add a comment...");
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
  });
});
