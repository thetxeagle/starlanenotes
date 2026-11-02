import { Schema } from "@tiptap/pm/model";
import { EditorState, TextSelection } from "@tiptap/pm/state";
import { describe, expect, it } from "vitest";
import {
  canStartInlineSlashMenu,
  deriveInlineSlashQuery,
  findInlineSlashQuery,
} from "./slashEditorSlashState";

const schema = new Schema({
  nodes: {
    doc: { content: "block+" },
    paragraph: { content: "text*", group: "block" },
    codeBlock: { content: "text*", group: "block", code: true },
    text: {},
  },
});

function stateFor(text: string, cursor: number, type: "paragraph" | "codeBlock" = "paragraph") {
  const block = schema.nodes[type].create(null, text ? schema.text(text) : undefined);
  const doc = schema.nodes.doc.create(null, block);
  return EditorState.create({ doc, selection: TextSelection.create(doc, cursor) });
}

describe("inline SlashEditor-style command state", () => {
  it("opens at the start of a paragraph or after whitespace", () => {
    expect(canStartInlineSlashMenu(stateFor("", 1), 1)).toBe(true);
    expect(canStartInlineSlashMenu(stateFor("Alpha ", 7), 7)).toBe(true);
    expect(canStartInlineSlashMenu(stateFor("Alpha", 6), 6)).toBe(false);
  });

  it("owns the slash and contiguous query as one command range", () => {
    expect(deriveInlineSlashQuery(stateFor("/callout", 9), 1)).toEqual({
      from: 1,
      to: 9,
      query: "callout",
    });
  });

  it("recovers the active query from editor state after browser text input", () => {
    expect(findInlineSlashQuery(stateFor("/columns", 9))).toEqual({
      from: 1,
      to: 9,
      query: "columns",
    });
    expect(findInlineSlashQuery(stateFor("Alpha /col", 11))).toEqual({
      from: 7,
      to: 11,
      query: "col",
    });
    expect(findInlineSlashQuery(stateFor("Alpha/col", 10))).toBeNull();
  });

  it("closes for spaces, removed triggers, selections, and code blocks", () => {
    expect(deriveInlineSlashQuery(stateFor("/call out", 10), 1)).toBeNull();
    expect(deriveInlineSlashQuery(stateFor("callout", 8), 1)).toBeNull();
    expect(canStartInlineSlashMenu(stateFor("", 1, "codeBlock"), 1)).toBe(false);
    expect(findInlineSlashQuery(stateFor("/callout", 9, "codeBlock"))).toBeNull();

    const selected = stateFor("/callout", 9);
    const withSelection = selected.apply(
      selected.tr.setSelection(TextSelection.create(selected.doc, 2, 5)),
    );
    expect(deriveInlineSlashQuery(withSelection, 1)).toBeNull();
  });
});
