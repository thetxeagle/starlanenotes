import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { EditorTemplateGuidanceDialog, canSubmitTemplateGuidance } from "./EditorTemplateGuidanceDialog";

describe("EditorTemplateGuidanceDialog", () => {
  it("requires a nonblank instruction and keeps the dialog accessible", () => {
    const html = renderToStaticMarkup(
      <EditorTemplateGuidanceDialog
        content="   "
        onChange={vi.fn()}
        onCancel={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );

    expect(html).toContain('aria-labelledby="editor-template-guidance-title"');
    expect(html).toContain('aria-describedby="editor-template-guidance-description"');
    expect(html).toContain('for="editor-template-guidance-content"');
    expect(html).toContain(">Instruction</label>");
    expect(html).toContain(">Insert instruction</button>");
    expect(html).toContain("disabled");
    expect(canSubmitTemplateGuidance("\n \t")).toBe(false);
    expect(canSubmitTemplateGuidance(" Add context ")).toBe(true);
  });
});
