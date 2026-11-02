import { Schema } from "@tiptap/pm/model";
import { EditorState } from "@tiptap/pm/state";
import { describe, expect, it } from "vitest";
import {
  collectMovableBlockTargets,
  findRuntimeBlockTarget,
  resolveBlockRectAtPoint,
  resolveSiblingBlockRectAtPoint,
  resolveCaretBlock,
  shouldShowEditorBlockGutter,
  type EditorGutterBlockRect,
} from "./EditorBlockGutter";

const schema = new Schema({
  nodes: {
    doc: { content: "section+" },
    section: { content: "sectionColumn+" },
    sectionColumn: { content: "block+" },
    paragraph: { content: "text*", group: "block", attrs: { id: { default: null } } },
    text: {},
  },
});

function stateWithBlocks() {
  const column = schema.nodes.sectionColumn.create(null, [
    schema.nodes.paragraph.create({ id: "alpha" }, schema.text("Alpha")),
    schema.nodes.paragraph.create({ id: "bravo" }, schema.text("Bravo")),
  ]);
  const section = schema.nodes.section.create(null, column);
  return EditorState.create({ doc: schema.nodes.doc.create(null, section) });
}

describe("findRuntimeBlockTarget", () => {
  it("resolves a runtime ID to its section-column coordinates", () => {
    const target = findRuntimeBlockTarget(stateWithBlocks(), "bravo");

    expect(target).toMatchObject({
      from: 9,
      to: 16,
      index: 1,
      parentStart: 2,
      parentChildCount: 2,
    });
    expect(target?.node.textContent).toBe("Bravo");
  });

  it("does not invent a target when runtime identity is stale", () => {
    expect(findRuntimeBlockTarget(stateWithBlocks(), "missing")).toBeNull();
  });
});

describe("shouldShowEditorBlockGutter", () => {
  it("limits the hover gutter to writable desktop editing", () => {
    expect(
      shouldShowEditorBlockGutter({
        showBlockChrome: true,
        mobileToolbar: false,
        readOnly: false,
      }),
    ).toBe(true);
    expect(
      shouldShowEditorBlockGutter({
        showBlockChrome: true,
        mobileToolbar: true,
        readOnly: false,
      }),
    ).toBe(false);
    expect(
      shouldShowEditorBlockGutter({
        showBlockChrome: true,
        mobileToolbar: false,
        readOnly: true,
      }),
    ).toBe(false);
  });
});

describe("resolveBlockRectAtPoint", () => {
  function blockRect(id: string, blockBounds: DOMRect): EditorGutterBlockRect {
    return {
      target: {
        node: schema.nodes.paragraph.create({ id }),
        from: 2,
        to: 4,
        index: 0,
        parentStart: 2,
        parentChildCount: 1,
      },
      element: {} as HTMLElement,
      blockRect: blockBounds,
      rect: blockBounds,
    };
  }

  it("keeps the complete control gutter connected to its block", () => {
    const block = blockRect("alpha", {
      left: 120,
      right: 540,
      top: 80,
      bottom: 108,
      width: 420,
      height: 28,
    } as DOMRect);
    const editorRect = { left: 120, right: 540, top: 20, bottom: 520 } as DOMRect;

    expect(resolveBlockRectAtPoint([block], editorRect, { x: 53, y: 94 })).toBe(block);
    expect(resolveBlockRectAtPoint([block], editorRect, { x: 51, y: 94 })).toBeNull();
  });

  it("resolves a whole-block drop over nested content to its sibling container", () => {
    const parent = blockRect("parent", {
      left: 120,
      right: 700,
      top: 80,
      bottom: 160,
      height: 80,
    } as DOMRect);
    parent.target.parentStart = 0;
    const nested = blockRect("nested", {
      left: 188,
      right: 400,
      top: 80,
      bottom: 108,
      height: 28,
    } as DOMRect);
    const bounds = { left: 120, right: 700, top: 20, bottom: 520 };
    expect(
      resolveSiblingBlockRectAtPoint([parent, nested], parent.target, bounds, { x: 200, y: 94 }),
    ).toBe(parent);
    expect(
      resolveSiblingBlockRectAtPoint([parent, nested], nested.target, bounds, { x: 200, y: 94 }),
    ).toBe(nested);
  });

  it("prefers the smallest nested movable block", () => {
    const list = blockRect("list", {
      left: 120,
      right: 540,
      top: 80,
      bottom: 200,
      width: 420,
      height: 120,
    } as DOMRect);
    const item = blockRect("item", {
      left: 144,
      right: 540,
      top: 104,
      bottom: 132,
      width: 396,
      height: 28,
    } as DOMRect);

    expect(
      resolveBlockRectAtPoint(
        [list, item],
        { left: 120, right: 540, top: 20, bottom: 520 },
        {
          x: 100,
          y: 118,
        },
      ),
    ).toBe(item);
  });

  it("does not let the left lane steal the right lane or its gutter", () => {
    const left = blockRect("left", {
      left: 188,
      right: 400,
      top: 80,
      bottom: 108,
      height: 28,
    } as DOMRect);
    const right = blockRect("right", {
      left: 488,
      right: 700,
      top: 80,
      bottom: 108,
      height: 28,
    } as DOMRect);
    const bounds = { left: 120, right: 700, top: 20, bottom: 520 };
    expect(resolveBlockRectAtPoint([left, right], bounds, { x: 500, y: 94 })).toBe(right);
    expect(resolveBlockRectAtPoint([left, right], bounds, { x: 430, y: 94 })).toBe(right);
    expect(resolveBlockRectAtPoint([left, right], bounds, { x: 119, y: 94 })).toBeNull();
  });

  it("gives the outside gutter to the columns container, not its first paragraph", () => {
    const columnSchema = new Schema({
      nodes: {
        doc: { content: "block+" },
        columns: { group: "block", content: "paragraph+" },
        paragraph: { group: "block", content: "text*" },
        text: {},
      },
    });
    const columns = blockRect("columns", {
      left: 120,
      right: 700,
      top: 80,
      bottom: 160,
      height: 80,
    } as DOMRect);
    columns.target.node = columnSchema.nodes.columns.create(
      null,
      columnSchema.nodes.paragraph.create(),
    );
    const child = blockRect("child", {
      left: 188,
      right: 400,
      top: 80,
      bottom: 108,
      height: 28,
    } as DOMRect);
    const bounds = { left: 120, right: 700, top: 20, bottom: 520 };
    expect(resolveBlockRectAtPoint([columns, child], bounds, { x: 90, y: 94 })).toBe(columns);
    expect(resolveBlockRectAtPoint([columns, child], bounds, { x: 150, y: 94 })).toBe(child);
  });
});

describe("collectMovableBlockTargets", () => {
  function columnState(lanes: string[][], container = "columns") {
    const columnSchema = new Schema({
      nodes: {
        doc: { content: "block+" },
        columns: { content: "sectionColumn+", group: "block" },
        section: { content: "sectionColumn+", group: "block" },
        sectionColumn: { content: "block+" },
        paragraph: { content: "text*", group: "block" },
        text: {},
      },
    });
    return EditorState.create({
      doc: columnSchema.nodes.doc.create(
        null,
        columnSchema.nodes[container].create(
          null,
          lanes.map((blocks) =>
            columnSchema.nodes.sectionColumn.create(
              null,
              blocks.map((text) =>
                columnSchema.nodes.paragraph.create(null, text ? columnSchema.text(text) : null),
              ),
            ),
          ),
        ),
      ),
    });
  }

  it("gives fresh empty columns just one container target", () => {
    expect(
      collectMovableBlockTargets(columnState([[""], [""]])).map((block) => block.node.type.name),
    ).toEqual(["columns"]);
  });

  it("retains authored content controls without empty-lane placeholder controls", () => {
    expect(
      collectMovableBlockTargets(columnState([[""], ["Right lane"]])).map(
        (block) => block.node.textContent,
      ),
    ).toEqual(["Right lane", "Right lane"]);
  });

  it("preserves extra blank blocks and legacy section paragraph controls", () => {
    expect(collectMovableBlockTargets(columnState([["", ""], [""]])).length).toBe(3);
    expect(collectMovableBlockTargets(columnState([[""], [" more"]], "section")).length).toBe(3);
  });

  it("discovers a root code block without relying on rendered data attributes", () => {
    const rootSchema = new Schema({
      nodes: {
        doc: { content: "block+" },
        paragraph: { content: "text*", group: "block", attrs: { id: { default: null } } },
        codeBlock: { content: "text*", group: "block", attrs: { id: { default: null } } },
        text: {},
      },
    });
    const paragraph = rootSchema.nodes.paragraph.create(
      { id: "paragraph" },
      rootSchema.text("Alpha"),
    );
    const code = rootSchema.nodes.codeBlock.create({ id: "code" }, rootSchema.text("const x = 1"));
    const state = EditorState.create({ doc: rootSchema.nodes.doc.create(null, [paragraph, code]) });
    const blocks = collectMovableBlockTargets(state);

    expect(blocks.map((block) => block.node.type.name)).toEqual(["paragraph", "codeBlock"]);
    expect(blocks[1]?.node.attrs.id).toBe("code");
  });
});

describe("keyboard block action ownership", () => {
  const paragraph = schema.nodes.paragraph.create(null, schema.text("Text"));
  function rect(from: number, to: number, node = paragraph): EditorGutterBlockRect {
    return {
      target: { from, to, node, index: 0, parentStart: 0, parentChildCount: 1 },
      element: {} as HTMLElement,
      blockRect: {} as DOMRect,
      rect: {} as DOMRect,
    };
  }
  it("opens the smallest authored child containing the caret", () => {
    const child = rect(4, 10);
    const container = rect(
      0,
      20,
      schema.nodes.section.create(
        null,
        schema.nodes.sectionColumn.create(null, [paragraph, paragraph]),
      ),
    );
    expect(resolveCaretBlock([container, child], 5, 5)).toBe(child);
  });
  it("keeps a whole-container owner when a selection spans its children", () => {
    const container = rect(0, 20);
    expect(resolveCaretBlock([container, rect(4, 10), rect(12, 18)], 5, 17)).toBe(container);
  });
  it("does not invent an owner outside the editor blocks", () => {
    expect(resolveCaretBlock([rect(4, 10)], 11, 11)).toBeNull();
  });
});
