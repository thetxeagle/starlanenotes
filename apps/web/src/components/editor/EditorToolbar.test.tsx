import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { Editor } from "@tiptap/react";
import { EDITOR_TOOLBAR_ERROR_STATUS_CLASS, EditorToolbar } from "./EditorToolbar";
import { MobileEditorToolbar } from "./MobileEditorToolbar";

function createToolbarEditor(): Editor {
  const can = {
    sinkListItem: () => true,
    liftListItem: () => true,
  };

  return {
    can: () => can,
    chain: () => ({
      focus: () => ({
        undo: () => ({ run: () => true }),
        redo: () => ({ run: () => true }),
        sinkListItem: () => ({ run: () => true }),
        liftListItem: () => ({ run: () => true }),
        toggleBold: () => ({ run: () => true }),
        toggleItalic: () => ({ run: () => true }),
        toggleStrike: () => ({ run: () => true }),
        toggleCode: () => ({ run: () => true }),
        toggleBulletList: () => ({ run: () => true }),
        toggleOrderedList: () => ({ run: () => true }),
        toggleTaskList: () => ({ run: () => true }),
        toggleBlockquote: () => ({ run: () => true }),
        setHorizontalRule: () => ({ run: () => true }),
        insertTable: () => ({ run: () => true }),
        addRowBefore: () => ({ run: () => true }),
        addRowAfter: () => ({ run: () => true }),
        addColumnBefore: () => ({ run: () => true }),
        addColumnAfter: () => ({ run: () => true }),
        deleteRow: () => ({ run: () => true }),
        deleteColumn: () => ({ run: () => true }),
        deleteTable: () => ({ run: () => true }),
        toggleCodeBlock: () => ({ run: () => true }),
        updateAttributes: () => ({ run: () => true }),
        setLink: () => ({ run: () => true }),
        setImage: () => ({ run: () => true }),
      }),
    }),
    getAttributes: (name: string) => (name === "codeBlock" ? { language: "typescript" } : {}),
    isActive: (name: string) => name === "codeBlock",
  } as unknown as Editor;
}

describe("EditorToolbar", () => {
  it("uses semantic danger tokens for failed save status", () => {
    expect(EDITOR_TOOLBAR_ERROR_STATUS_CLASS).toBe("text-danger");
    expect(EDITOR_TOOLBAR_ERROR_STATUS_CLASS).not.toContain("red-");
  });

  it("uses the shared select control for code block language", () => {
    const html = renderToStaticMarkup(
      <EditorToolbar editor={createToolbarEditor()} status="saved" locked={false} />,
    );

    expect(html).toContain('aria-label="Code block language"');
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
    expect(html).toContain("TypeScript");
  });

  it("renders link and image buttons without browser prompt copy", () => {
    const html = renderToStaticMarkup(
      <EditorToolbar editor={createToolbarEditor()} status="saved" locked={false} />,
    );

    expect(html).toContain('aria-label="Link"');
    expect(html).toContain('aria-label="Bookmark"');
    expect(html).toContain('aria-label="Image"');
    expect(html).not.toContain("Enter URL");
    expect(html).not.toContain("Enter image URL");
  });

  it("keeps mobile toolbar link and image insertion off browser prompts", () => {
    const html = renderToStaticMarkup(
      <MobileEditorToolbar editor={createToolbarEditor()} locked={false} />,
    );

    expect(html).toContain('aria-label="Link"');
    expect(html).toContain('aria-label="Bookmark"');
    expect(html).toContain('aria-label="Image"');
    expect(html).not.toContain("Enter URL");
    expect(html).not.toContain("Enter image URL");
  });
});
