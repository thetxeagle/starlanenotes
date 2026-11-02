import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  canSubmitEditorUrl,
  EditorUrlDialog,
  getEditorUrlDialogLabel,
  getEditorUrlDialogPlaceholder,
  getEditorUrlDialogTitle,
} from "./EditorUrlDialog";

describe("EditorUrlDialog", () => {
  it("uses explicit copy for link and image URL insertion", () => {
    expect(getEditorUrlDialogTitle("link")).toBe("Insert link");
    expect(getEditorUrlDialogTitle("image")).toBe("Insert image");
    expect(getEditorUrlDialogLabel("link")).toBe("Link URL");
    expect(getEditorUrlDialogLabel("image")).toBe("Image URL");
    expect(getEditorUrlDialogPlaceholder("link")).toBe("https://example.com");
    expect(getEditorUrlDialogPlaceholder("image")).toBe("https://example.com/image.jpg");
    expect(canSubmitEditorUrl("https://example.com")).toBe(true);
    expect(canSubmitEditorUrl("   ")).toBe(false);
  });

  it("renders the shared Starlane URL entry dialog", () => {
    const html = renderToStaticMarkup(
      <EditorUrlDialog
        kind="link"
        value=""
        onChange={() => undefined}
        onCancel={() => undefined}
        onSubmit={() => undefined}
      />,
    );

    expect(html).toContain("Insert link");
    expect(html).toContain("Link URL");
    expect(html).toContain("https://example.com");
    expect(html).toContain("rounded-input");
    expect(html).toContain("border-b");
    expect(html).toContain("border-t");
    expect(html).toContain("bg-surface");
    expect(html).toContain(">Cancel<");
    expect(html).not.toContain("prompt");
  });
});
