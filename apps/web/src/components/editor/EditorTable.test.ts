import { describe, expect, it } from "vitest";
import { EditorState, TextSelection, type Transaction } from "prosemirror-state";
import { Schema } from "prosemirror-model";
import { CellSelection, TableMap, tableNodes } from "prosemirror-tables";
import {
  getTableToolbarTop,
  moveActiveTableColumn,
  selectFirstCellInLastTableRow,
  sortActiveTableByColumn,
  type TipTapEditor,
} from "./Editor";

const defaultTableNodes = tableNodes({
  tableGroup: "block",
  cellContent: "paragraph+",
  cellAttributes: {},
});
const schema = new Schema({
  nodes: {
    doc: { content: "block+" },
    paragraph: { content: "inline*", group: "block" },
    text: { group: "inline" },
    table: { ...defaultTableNodes.table, content: "tableRow+" },
    tableRow: { ...defaultTableNodes.table_row, content: "(tableCell | tableHeader)*" },
    tableCell: defaultTableNodes.table_cell,
    tableHeader: defaultTableNodes.table_header,
  },
});

function cell(value: string, header = false) {
  return schema.nodes[header ? "tableHeader" : "tableCell"].create(
    null,
    schema.nodes.paragraph.create(null, value ? schema.text(value) : undefined),
  );
}

function makeTableDocument() {
  const table = schema.nodes.table.create(null, [
    schema.nodes.tableRow.create(null, [cell("Name", true), cell("Rank", true)]),
    schema.nodes.tableRow.create(null, [cell("Caboose"), cell("10")]),
    schema.nodes.tableRow.create(null, [cell("Church"), cell("2")]),
    schema.nodes.tableRow.create(null, [cell("Simmons"), cell("30")]),
  ]);
  return schema.nodes.doc.create(null, table);
}

function tableRows(state: EditorState): string[][] {
  const table = state.doc.firstChild!;
  return Array.from({ length: table.childCount }, (_, rowIndex) => {
    const row = table.child(rowIndex);
    return Array.from(
      { length: row.childCount },
      (_, cellIndex) => row.child(cellIndex).textContent,
    );
  });
}

describe("selectFirstCellInLastTableRow", () => {
  it("moves the cursor into an appended row's first cell", () => {
    const doc = makeTableDocument();
    const table = doc.firstChild!;
    const map = TableMap.get(table);
    const lastCell = 1 + map.positionAt(table.childCount - 1, 0, table);
    let state = EditorState.create({
      doc,
      selection: TextSelection.near(doc.resolve(lastCell + 2), 1),
    });
    const appendedRow = schema.nodes.tableRow.create(null, [cell(""), cell("")]);
    const transaction = state.tr.insert(table.nodeSize - 1, appendedRow);

    expect(selectFirstCellInLastTableRow(transaction)).toBe(true);
    state = state.apply(transaction);

    expect(state.selection).toBeInstanceOf(TextSelection);
    let rowDepth = -1;
    for (let depth = state.selection.$from.depth; depth > 0; depth--) {
      if (state.selection.$from.node(depth).type.name === "tableRow") {
        rowDepth = depth;
        break;
      }
    }
    expect(rowDepth).toBeGreaterThan(0);
    expect(state.selection.$from.index(rowDepth - 1)).toBe(table.childCount);
    expect(state.selection.$from.index(rowDepth)).toBe(0);
    expect(state.selection.$from.node(rowDepth).textContent).toBe("");
  });
});

describe("table toolbar viewport placement", () => {
  it("keeps the toolbar below the visible workspace header when a table is scrolled above it", () => {
    expect(getTableToolbarTop(20, 48, 800)).toBe(56);
  });

  it("keeps the toolbar anchored above a visible table and inside the viewport", () => {
    expect(getTableToolbarTop(250, 48, 800)).toBe(206);
    expect(getTableToolbarTop(1000, 48, 800)).toBe(748);
  });
});

describe("sortActiveTableByColumn", () => {
  it("sorts an ordinary selected column while preserving the header row", () => {
    const doc = makeTableDocument();
    const table = doc.firstChild!;
    const map = TableMap.get(table);
    let state = EditorState.create({
      doc,
      selection: CellSelection.create(doc, 1 + map.positionAt(1, 1, table)),
    });
    const editor = {
      get state() {
        return state;
      },
      view: {
        dispatch(transaction: Transaction) {
          state = state.apply(transaction);
        },
      },
      commands: {
        setCellSelection({ anchorCell }: { anchorCell: number }) {
          state = state.apply(state.tr.setSelection(CellSelection.create(state.doc, anchorCell)));
          return true;
        },
        focus() {
          return true;
        },
      },
    } as unknown as TipTapEditor;

    expect(sortActiveTableByColumn(editor, "ascending")).toBe(true);
    expect(tableRows(state)).toEqual([
      ["Name", "Rank"],
      ["Church", "2"],
      ["Caboose", "10"],
      ["Simmons", "30"],
    ]);
    expect(state.selection).toBeInstanceOf(CellSelection);
  });
});

describe("moveActiveTableColumn", () => {
  it("moves the selected column and retains the cell selection on its new column", () => {
    const doc = makeTableDocument();
    const table = doc.firstChild!;
    const map = TableMap.get(table);
    let state = EditorState.create({
      doc,
      selection: CellSelection.create(doc, 1 + map.positionAt(1, 1, table)),
    });
    const editor = {
      get state() {
        return state;
      },
      view: {
        dispatch(transaction: Transaction) {
          state = state.apply(transaction);
        },
      },
      commands: {
        setCellSelection({ anchorCell }: { anchorCell: number }) {
          state = state.apply(state.tr.setSelection(CellSelection.create(state.doc, anchorCell)));
          return true;
        },
        focus() {
          return true;
        },
      },
    } as unknown as TipTapEditor;

    expect(moveActiveTableColumn(editor, "left")).toBe(true);
    expect(tableRows(state)).toEqual([
      ["Rank", "Name"],
      ["10", "Caboose"],
      ["2", "Church"],
      ["30", "Simmons"],
    ]);
    expect(state.selection).toBeInstanceOf(CellSelection);
  });
});
