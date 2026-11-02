import { Schema } from "@tiptap/pm/model";
import { EditorState, TextSelection } from "@tiptap/pm/state";
import { describe, expect, it } from "vitest";
import { createBlockMoveTransaction, resolveMovableBlock } from "./slashEditorBlockMove";

const schema = new Schema({
  nodes: {
    doc: { content: "section+" },
    section: { content: "sectionColumn+" },
    sectionColumn: { content: "block+" },
    paragraph: { content: "inline*", group: "block", attrs: { id: { default: null } } },
    callout: {
      content: "inline*",
      group: "block",
      attrs: { id: { default: null }, tone: { default: "info" } },
    },
    bulletList: { content: "listItem+", group: "block" },
    listItem: { content: "paragraph+" },
    text: { group: "inline" },
  },
  marks: {
    strong: {},
  },
});

describe("resolveMovableBlock", () => {
  it("targets the current list item instead of moving its complete list", () => {
    const list = schema.nodes.bulletList.create(null, [
      schema.nodes.listItem.create(
        null,
        schema.nodes.paragraph.create({ id: "one" }, schema.text("One")),
      ),
      schema.nodes.listItem.create(
        null,
        schema.nodes.paragraph.create({ id: "two" }, schema.text("Two")),
      ),
    ]);
    const column = schema.nodes.sectionColumn.create(null, list);
    const section = schema.nodes.section.create(null, column);
    const doc = schema.nodes.doc.create(null, section);
    let secondItemPosition = -1;
    doc.descendants((node, position) => {
      if (node.type.name === "listItem" && node.textContent === "Two") {
        secondItemPosition = position;
      }
    });
    const state = EditorState.create({
      doc,
      selection: TextSelection.near(doc.resolve(secondItemPosition + 2)),
    });

    const target = resolveMovableBlock(state);
    expect(target?.node.type.name).toBe("listItem");
    expect(target?.node.textContent).toBe("Two");
    expect(target?.index).toBe(1);
    expect(target?.parentChildCount).toBe(2);
  });
});

function createState() {
  const strong = schema.marks.strong.create();
  const blocks = [
    schema.nodes.paragraph.create({ id: "alpha" }, schema.text("Alpha")),
    schema.nodes.callout.create(
      { id: "bravo", tone: "warning" },
      schema.text("Bravo", [strong]),
    ),
    schema.nodes.paragraph.create({ id: "charlie" }, schema.text("Charlie")),
  ];
  const column = schema.nodes.sectionColumn.create(null, blocks);
  const section = schema.nodes.section.create(null, column);
  return EditorState.create({ doc: schema.nodes.doc.create(null, section) });
}

function columnOf(state: EditorState) {
  return state.doc.firstChild!.firstChild!;
}

function positionOf(state: EditorState, index: number) {
  let position = 2;
  for (let childIndex = 0; childIndex < index; childIndex++) {
    position += columnOf(state).child(childIndex).nodeSize;
  }
  return position;
}

describe("createBlockMoveTransaction", () => {
  it("moves a block up inside a section column without flattening the section schema", () => {
    const state = createState();
    const from = positionOf(state, 1);
    const result = createBlockMoveTransaction(state, {
      from,
      size: columnOf(state).child(1).nodeSize,
      to: positionOf(state, 0),
    });

    expect(result).not.toBeNull();
    const next = state.apply(result!.transaction);
    expect(next.doc.firstChild?.type.name).toBe("section");
    expect(columnOf(next).content.content.map((node) => node.textContent)).toEqual([
      "Bravo",
      "Alpha",
      "Charlie",
    ]);
  });

  it("moves a block down using original-document coordinates", () => {
    const state = createState();
    const from = positionOf(state, 0);
    const nextSibling = columnOf(state).child(1);
    const result = createBlockMoveTransaction(state, {
      from,
      size: columnOf(state).child(0).nodeSize,
      to: positionOf(state, 1) + nextSibling.nodeSize,
    });

    expect(result).not.toBeNull();
    const next = state.apply(result!.transaction);
    expect(columnOf(next).content.content.map((node) => node.textContent)).toEqual([
      "Bravo",
      "Alpha",
      "Charlie",
    ]);
    expect(result?.transaction.steps).toHaveLength(2);
  });

  it("preserves custom block attributes, runtime identity, content, and marks", () => {
    const state = createState();
    const from = positionOf(state, 1);
    const result = createBlockMoveTransaction(state, {
      from,
      size: columnOf(state).child(1).nodeSize,
      to: positionOf(state, 0),
    });

    const moved = columnOf(state.apply(result!.transaction)).child(0);
    expect(moved.type.name).toBe("callout");
    expect(moved.attrs).toMatchObject({ id: "bravo", tone: "warning" });
    expect(moved.textContent).toBe("Bravo");
    expect(moved.firstChild?.marks.map((mark) => mark.type.name)).toEqual(["strong"]);
  });

  it("rejects stale sizes, destinations inside the source, and out-of-range positions", () => {
    const state = createState();
    const from = positionOf(state, 1);
    const size = columnOf(state).child(1).nodeSize;

    expect(createBlockMoveTransaction(state, { from, size: size + 1, to: 2 })).toBeNull();
    expect(createBlockMoveTransaction(state, { from, size, to: from + 1 })).toBeNull();
    expect(
      createBlockMoveTransaction(state, { from, size, to: state.doc.content.size + 1 }),
    ).toBeNull();
  });

  it("rejects a move across structural containers instead of auto-wrapping it", () => {
    const state = createState();
    const from = positionOf(state, 1);

    expect(
      createBlockMoveTransaction(state, {
        from,
        size: columnOf(state).child(1).nodeSize,
        to: 0,
      }),
    ).toBeNull();
    expect(state.doc.toJSON()).toEqual(createState().doc.toJSON());
  });
});
