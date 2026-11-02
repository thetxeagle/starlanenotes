import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { canSubmitEditorLink, EditorLinkDialog } from "./EditorLinkDialog";

describe("EditorLinkDialog", () => {
  it("requires a non-blank destination before saving", () => {
    expect(canSubmitEditorLink("https://starlane.example")).toBe(true);
    expect(canSubmitEditorLink("starlane.example")).toBe(true);
    expect(canSubmitEditorLink("   ")).toBe(false);
    expect(canSubmitEditorLink("javascript:alert(1)")).toBe(false);
  });

  it("uses the shared accessible dialog shell and explicit actions", () => {
    const html = renderToStaticMarkup(
      <EditorLinkDialog
        text="Starlane"
        href=""
        onTextChange={() => undefined}
        onHrefChange={() => undefined}
        onCancel={() => undefined}
        onSubmit={() => undefined}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="editor-link-title"');
    expect(html).toContain('aria-describedby="editor-link-description"');
    expect(html).toContain("border-b");
    expect(html).toContain("border-t");
    expect(html).toContain("Display text");
    expect(html).toContain("Link URL");
    expect(html).toContain(">Save link<");
    expect(html).toContain(">Cancel<");
  });
});
