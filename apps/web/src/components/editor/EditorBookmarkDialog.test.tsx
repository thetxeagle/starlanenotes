import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { canSubmitEditorBookmark, EditorBookmarkDialog } from "./EditorBookmarkDialog";

describe("EditorBookmarkDialog", () => {
  it("requires a non-blank URL before inserting a bookmark", () => {
    expect(canSubmitEditorBookmark("https://starlane.example")).toBe(true);
    expect(canSubmitEditorBookmark("   ")).toBe(false);
  });

  it("uses the shared accessible dialog shell and explicit actions", () => {
    const html = renderToStaticMarkup(
      <EditorBookmarkDialog
        href=""
        onHrefChange={() => undefined}
        onCancel={() => undefined}
        onSubmit={() => undefined}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="editor-bookmark-title"');
    expect(html).toContain('aria-describedby="editor-bookmark-description"');
    expect(html).toContain("border-b");
    expect(html).toContain("border-t");
    expect(html).toContain("Insert bookmark");
    expect(html).toContain("Paste a URL and we’ll add it to the page.");
    expect(html).not.toContain("Fetch preview");
    expect(html).not.toContain("Description");
    expect(html).toContain(">Cancel<");
  });
});
