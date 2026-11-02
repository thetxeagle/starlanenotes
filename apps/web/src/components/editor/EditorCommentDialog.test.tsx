import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { canSubmitEditorComment, EditorCommentDialog } from "./EditorCommentDialog";

describe("EditorCommentDialog", () => {
  it("requires non-blank comment content", () => {
    expect(canSubmitEditorComment("   ")).toBe(false);
    expect(canSubmitEditorComment("Please verify this.")).toBe(true);
  });

  it("uses the shared dialog and textarea controls for an anchored comment", () => {
    const html = renderToStaticMarkup(
      createElement(EditorCommentDialog, {
        content: "",
        target: { selectionText: "Selected document text" },
        onCancel: () => undefined,
        onChange: () => undefined,
        onSubmit: () => undefined,
      }),
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('id="editor-comment-title"');
    expect(html).toContain('aria-describedby="editor-comment-description"');
    expect(html).toContain("border-b");
    expect(html).toContain("border-t");
    expect(html).toContain("Selected document text");
    expect(html).toContain('id="editor-comment-content"');
    expect(html).toContain("rounded-input");
    expect(html).toContain("Add comment");
  });
});
