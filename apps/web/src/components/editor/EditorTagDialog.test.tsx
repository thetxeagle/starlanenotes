import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { canSubmitEditorTag, EditorTagDialog } from "./EditorTagDialog";

describe("EditorTagDialog", () => {
  it("requires a normalized tag before inserting", () => {
    expect(canSubmitEditorTag("project-notes")).toBe(true);
    expect(canSubmitEditorTag("")).toBe(false);
  });

  it("uses the shared accessible dialog shell and explicit actions", () => {
    const html = renderToStaticMarkup(
      <EditorTagDialog
        value=""
        normalizedTag="project-notes"
        onChange={() => undefined}
        onCancel={() => undefined}
        onSubmit={() => undefined}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="editor-tag-title"');
    expect(html).toContain('aria-describedby="editor-tag-description"');
    expect(html).toContain("border-b");
    expect(html).toContain("border-t");
    expect(html).toContain("Insert tag");
    expect(html).toContain("#project-notes");
    expect(html).toContain(">Cancel<");
  });
});
