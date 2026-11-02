import { describe, expect, it, vi } from "vitest";
import type { Editor as TipTapEditor } from "@tiptap/react";
import { Schema } from "@tiptap/pm/model";
import { EditorState, Plugin, TextSelection, type Transaction } from "@tiptap/pm/state";
import { captureAssetInsertionRange, AssetInsertionRangeError } from "./assetInsertionRange";

const schema = new Schema({
  nodes: {
    doc: { content: "paragraph+" },
    paragraph: { content: "text*", group: "block" },
    text: { group: "inline" },
  },
});
type AppliedEvent = { transaction: Transaction; appendedTransactions?: readonly Transaction[] };
function fixture(text = "abcdef", caret = 4, plugins: Plugin[] = []) {
  const doc = schema.node("doc", null, [
    schema.node("paragraph", null, text ? schema.text(text) : undefined),
  ]);
  let state = EditorState.create({
    schema,
    doc,
    selection: TextSelection.create(doc, caret),
    plugins,
  });
  const listeners = new Set<(event: AppliedEvent) => void>();
  const object = {
    isDestroyed: false,
    get state() {
      return state;
    },
    on: vi.fn((_event: string, listener: (event: AppliedEvent) => void) => {
      listeners.add(listener);
    }),
    off: vi.fn((_event: string, listener: (event: AppliedEvent) => void) => {
      listeners.delete(listener);
    }),
  };
  return {
    editor: object as unknown as TipTapEditor,
    listeners,
    apply(transaction: Transaction) {
      const applied = state.applyTransaction(transaction);
      state = applied.state;
      for (const listener of listeners)
        listener({ transaction, appendedTransactions: applied.transactions.slice(1) });
      return applied;
    },
    replaceState() {
      const replacement = schema.node("doc", null, doc.content);
      state = EditorState.create({
        schema,
        doc: replacement,
        selection: TextSelection.create(replacement, caret),
      });
    },
  };
}

describe("original asset insertion positions", () => {
  it("maps continued typing at the collapsed target with association +1", () => {
    const f = fixture(),
      range = captureAssetInsertionRange(f.editor);
    f.apply(f.editor.state.tr.insertText("NEW", 4));
    expect(range.current()).toEqual({ from: 7, to: 7 });
    range.close();
  });
  it("maps intervening edits but a later caret move cannot retarget insertion", () => {
    const f = fixture(),
      range = captureAssetInsertionRange(f.editor);
    f.apply(f.editor.state.tr.setSelection(TextSelection.create(f.editor.state.doc, 1)));
    expect(range.current()).toEqual({ from: 4, to: 4 });
    f.apply(f.editor.state.tr.insertText("X", 1));
    expect(range.current()).toEqual({ from: 5, to: 5 });
    range.close();
  });
  it("permanently refuses deleted caret even after document bytes are restored", () => {
    const f = fixture(),
      range = captureAssetInsertionRange(f.editor);
    f.apply(f.editor.state.tr.delete(2, 6));
    expect(() => range.current()).toThrow(AssetInsertionRangeError);
    f.apply(f.editor.state.tr.insertText("bcde", 2));
    expect(() => range.current()).toThrow(AssetInsertionRangeError);
    expect(() => range.advance()).toThrow(AssetInsertionRangeError);
    range.close();
  });
  it("maps an intact slash span and refuses partial replacement within it", () => {
    const f = fixture("/image suffix", 7),
      range = captureAssetInsertionRange(f.editor, { from: 1, to: 7 });
    f.apply(f.editor.state.tr.insertText("prefix ", 1));
    expect(range.current()).toEqual({ from: 8, to: 14 });
    f.apply(f.editor.state.tr.insertText("different", 10, 12));
    expect(() => range.current()).toThrow(AssetInsertionRangeError);
    range.close();
  });
  it("refuses insertion inside the captured slash span", () => {
    const f = fixture("/image suffix", 7),
      range = captureAssetInsertionRange(f.editor, { from: 1, to: 7 });
    f.apply(f.editor.state.tr.insertText("new", 3));
    expect(() => range.current()).toThrow(AssetInsertionRangeError);
    range.close();
  });
  it("synchronously acknowledges own successful slash replacement and continues the group", () => {
    const f = fixture("/image suffix", 7),
      range = captureAssetInsertionRange(f.editor, { from: 1, to: 7 });
    const target = range.current();
    f.apply(f.editor.state.tr.insertText("asset", target.from, target.to));
    range.advance();
    expect(range.current()).toEqual({ from: 6, to: 6 });
    f.apply(f.editor.state.tr.insertText("second", 6));
    range.advance();
    expect(range.current()).toEqual({ from: 12, to: 12 });
    range.close();
  });
  it("maps a genuine plugin-appended normalization through own insertion and later typing", () => {
    const normalization = new Plugin({
      appendTransaction(transactions, _old, state) {
        if (
          !transactions.some((tr) => tr.getMeta("normalize")) ||
          transactions.some((tr) => tr.getMeta("normalized"))
        )
          return null;
        return state.tr.insertText("!", 1).setMeta("normalized", true);
      },
    });
    const f = fixture("/image suffix", 7, [normalization]);
    const range = captureAssetInsertionRange(f.editor, { from: 1, to: 7 });
    const target = range.current();
    const result = f.apply(
      f.editor.state.tr.insertText("asset", target.from, target.to).setMeta("normalize", true),
    );
    expect(result.transactions).toHaveLength(2);
    expect(result.transactions[0].doc).not.toBe(f.editor.state.doc);
    expect(result.transactions[1].before).toBe(result.transactions[0].doc);
    expect(f.editor.state.doc.textContent).toBe("!asset suffix");
    expect(() => range.advance()).not.toThrow();
    expect(range.current()).toEqual({ from: 7, to: 7 });
    f.apply(f.editor.state.tr.insertText("NEXT", 7));
    expect(range.current()).toEqual({ from: 11, to: 11 });
    range.close();
  });
  it("refuses a target removed by a genuine appended transaction, even if another append restores bytes", () => {
    const replacement = new Plugin({
      appendTransaction(transactions, _old, state) {
        if (!transactions.some((tr) => tr.getMeta("replace-target"))) return null;
        return state.tr.insertText("replacement", 2, 6).setMeta("replaced-target", true);
      },
    });
    const restoration = new Plugin({
      appendTransaction(transactions, _old, state) {
        if (!transactions.some((tr) => tr.getMeta("replaced-target"))) return null;
        return state.tr.insertText("bcde", 2, 13);
      },
    });
    const f = fixture("abcdef", 4, [replacement, restoration]);
    const range = captureAssetInsertionRange(f.editor);
    const result = f.apply(f.editor.state.tr.setMeta("replace-target", true));
    expect(result.transactions).toHaveLength(3);
    expect(f.editor.state.doc.textContent).toBe("abcdef");
    expect(() => range.current()).toThrow(AssetInsertionRangeError);
    expect(() => range.advance()).toThrow(AssetInsertionRangeError);
    range.close();
  });
  it("detects direct editor state replacement, invalid bounds, destroy and listener cleanup", () => {
    const f = fixture();
    expect(() => captureAssetInsertionRange(f.editor, { from: -1, to: 3 })).toThrow(
      AssetInsertionRangeError,
    );
    const range = captureAssetInsertionRange(f.editor);
    expect(f.listeners.size).toBe(1);
    f.replaceState();
    expect(() => range.current()).toThrow(AssetInsertionRangeError);
    range.close();
    range.close();
    expect(f.listeners.size).toBe(0);
    expect(f.editor.off).toHaveBeenCalledOnce();
    expect(() => range.current()).toThrow(AssetInsertionRangeError);
    const next = captureAssetInsertionRange(f.editor);
    Object.assign(f.editor, { isDestroyed: true });
    expect(() => next.current()).toThrow(AssetInsertionRangeError);
    next.close();
  });
});
