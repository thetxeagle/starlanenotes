import { describe, expect, it, vi } from "vitest";
import { applyEditorBlockTransform, type EditorBlockTransformExecutor } from "./Editor";

function createTransformEditor() {
  const run = vi.fn(() => true);
  const setParagraph = vi.fn(() => ({ run }));
  const setHeading = vi.fn(() => ({ run }));
  const toggleBulletList = vi.fn(() => ({ run }));
  const toggleOrderedList = vi.fn(() => ({ run }));
  const toggleTaskList = vi.fn(() => ({ run }));
  const toggleBlockquote = vi.fn(() => ({ run }));
  const toggleCodeBlock = vi.fn(() => ({ run }));
  const focus = vi.fn(() => ({
    setParagraph,
    setHeading,
    toggleBulletList,
    toggleOrderedList,
    toggleTaskList,
    toggleBlockquote,
    toggleCodeBlock,
  }));
  const chain = vi.fn(() => ({ focus }));

  return {
    editor: { chain } as unknown as EditorBlockTransformExecutor,
    run,
    setParagraph,
    setHeading,
    toggleBulletList,
    toggleOrderedList,
    toggleTaskList,
    toggleBlockquote,
    toggleCodeBlock,
  };
}

describe("applyEditorBlockTransform", () => {
  it("maps each supported transform to a focused portable editor command", () => {
    const targets = [
      ["paragraph", "setParagraph"],
      ["heading-1", "setHeading"],
      ["heading-2", "setHeading"],
      ["heading-3", "setHeading"],
      ["bullet", "toggleBulletList"],
      ["numbered", "toggleOrderedList"],
      ["task", "toggleTaskList"],
      ["quote", "toggleBlockquote"],
      ["code", "toggleCodeBlock"],
    ] as const;

    for (const [target, expectedMethod] of targets) {
      const harness = createTransformEditor();
      expect(applyEditorBlockTransform(harness.editor, target)).toBe(true);
      expect(harness[expectedMethod]).toHaveBeenCalledOnce();
      expect(harness.run).toHaveBeenCalledOnce();
    }
  });

  it("keeps heading levels explicit", () => {
    const harness = createTransformEditor();

    applyEditorBlockTransform(harness.editor, "heading-3");

    expect(harness.setHeading).toHaveBeenCalledWith({ level: 3 });
  });
});
