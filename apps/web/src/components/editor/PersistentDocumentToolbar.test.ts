import { describe, expect, it } from "vitest";
import type { Editor as TipTapEditor } from "@tiptap/react";
import { applyPersistentToolbarHeading } from "./PersistentDocumentToolbar";

function createEditorRecorder() {
  const calls: unknown[][] = [];
  const chain = {
    focus() {
      calls.push(["focus"]);
      return this;
    },
    setTextSelection(selection: { from: number; to: number }) {
      calls.push(["setTextSelection", selection]);
      return this;
    },
    setParagraph() {
      calls.push(["setParagraph"]);
      return this;
    },
    setHeading(attributes: { level: 1 | 2 | 3 | 4 | 5 | 6 }) {
      calls.push(["setHeading", attributes]);
      return this;
    },
    run() {
      calls.push(["run"]);
      return true;
    },
  };
  return { calls, editor: { chain: () => chain } as unknown as TipTapEditor };
}

describe("PersistentDocumentToolbar", () => {
  it("restores the captured text selection before applying a heading", () => {
    const { calls, editor } = createEditorRecorder();
    const selection = { from: 12, to: 28 };

    expect(applyPersistentToolbarHeading(editor, "2", selection)).toBe(true);
    expect(calls).toEqual([
      ["focus"],
      ["setTextSelection", selection],
      ["setHeading", { level: 2 }],
      ["run"],
    ]);
  });

  it("applies paragraph style at the editor selection when no text range was captured", () => {
    const { calls, editor } = createEditorRecorder();

    expect(applyPersistentToolbarHeading(editor, "paragraph", null)).toBe(true);
    expect(calls).toEqual([["focus"], ["setParagraph"], ["run"]]);
  });

  it("supports Heading 6 while restoring the selected text", () => {
    const { calls, editor } = createEditorRecorder();
    const selection = { from: 4, to: 9 };

    expect(applyPersistentToolbarHeading(editor, "6", selection)).toBe(true);
    expect(calls).toEqual([
      ["focus"],
      ["setTextSelection", selection],
      ["setHeading", { level: 6 }],
      ["run"],
    ]);
  });
});
