import { Schema } from "prosemirror-model";
import { EditorState } from "prosemirror-state";
import { describe, expect, it } from "vitest";
import {
  createEditorLinkTransaction,
  createEditorUnlinkTransaction,
  normalizeEditorLinkHref,
} from "./slashEditorLink";

const schema = new Schema({
  nodes: {
    doc: { content: "paragraph+" },
    paragraph: { content: "text*" },
    text: {},
  },
  marks: {
    bold: {},
    italic: {},
    link: {
      attrs: {
        href: {},
        class: { default: null },
        target: { default: null },
        rel: { default: null },
      },
    },
  },
});

function createState() {
  const bold = schema.marks.bold.create();
  const italic = schema.marks.italic.create();
  const paragraph = schema.nodes.paragraph.create(null, [
    schema.text("Star", [bold]),
    schema.text("lane", [italic]),
  ]);
  return EditorState.create({ doc: schema.nodes.doc.create(null, paragraph) });
}

describe("SlashEditor-style link transactions", () => {
  it("normalizes common destinations and rejects executable schemes", () => {
    expect(normalizeEditorLinkHref("example.com")).toBe("https://example.com");
    expect(normalizeEditorLinkHref("/docs/start")).toBe("/docs/start");
    expect(normalizeEditorLinkHref("mailto:eagle@example.com")).toBe(
      "mailto:eagle@example.com",
    );
    expect(normalizeEditorLinkHref("javascript:alert(1)")).toBe("");
    expect(normalizeEditorLinkHref("data:text/html,boom")).toBe("");
  });

  it("updates a link without flattening mixed inline formatting", () => {
    const state = createState();
    const transaction = createEditorLinkTransaction(
      state,
      { from: 1, to: 9, text: "Starlane" },
      "https://starlane.example",
    );

    expect(transaction).not.toBeNull();
    const firstMarks = transaction!.doc.resolve(2).marks().map((mark) => mark.type.name);
    const lastMarks = transaction!.doc.resolve(6).marks().map((mark) => mark.type.name);
    expect(firstMarks).toEqual(["bold", "link"]);
    expect(lastMarks).toEqual(["italic", "link"]);
  });

  it("removes only the link mark and leaves other marks intact", () => {
    const state = createState();
    const linked = createEditorLinkTransaction(
      state,
      { from: 1, to: 9, text: "Starlane" },
      "https://starlane.example",
    );
    const linkedState = state.apply(linked!);
    const unlinked = createEditorUnlinkTransaction(linkedState, { from: 1, to: 9 });

    expect(unlinked).not.toBeNull();
    expect(unlinked!.doc.resolve(2).marks().map((mark) => mark.type.name)).toEqual(["bold"]);
    expect(unlinked!.doc.resolve(6).marks().map((mark) => mark.type.name)).toEqual(["italic"]);
  });
});
