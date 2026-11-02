/**
 * Adapted from slash-editor 0.7.0 `packages/core/src/block-drag.ts` (MIT).
 * Copyright 2025-present Nhat Bui.
 *
 * Starlane owns the validation and selection behavior around this narrow
 * transaction primitive so block movement stays compatible with the existing
 * section schema, Markdown serializer, and Automerge history boundary.
 */
import type { EditorState, Transaction } from "@tiptap/pm/state";
import { TextSelection } from "@tiptap/pm/state";

export interface BlockMove {
  /** Position of the block in the transaction's original document. */
  from: number;
  /** Exact node size of the block being moved. */
  size: number;
  /** Destination in the transaction's original document coordinate space. */
  to: number;
}

export interface BlockMoveResult {
  transaction: Transaction;
  movedFrom: number;
  movedTo: number;
}

export interface ResolvedMovableBlock {
  node: import("@tiptap/pm/model").Node;
  from: number;
  to: number;
  index: number;
  parentStart: number;
  parentChildCount: number;
}

/**
 * Resolves the same movable unit used by the gutter: an individual list item,
 * an ordinary direct child of a Starlane section column, or a direct document
 * child for non-section schemas.
 */
export function resolveMovableBlock(state: EditorState): ResolvedMovableBlock | null {
  const { $from } = state.selection;
  if ($from.depth === 0) return null;

  for (let depth = $from.depth; depth >= 1; depth -= 1) {
    const node = $from.node(depth);
    const parent = $from.node(depth - 1);
    const isListItem = node.type.name === "listItem" || node.type.name === "taskItem";
    const isSectionColumnChild = parent.type.name === "sectionColumn";
    const isDocumentChild = depth === 1;
    if (!isListItem && !isSectionColumnChild && !isDocumentChild) continue;

    return {
      node,
      from: $from.before(depth),
      to: $from.after(depth),
      index: $from.index(depth - 1),
      parentStart: $from.start(depth - 1),
      parentChildCount: parent.childCount,
    };
  }

  return null;
}

/**
 * Moves one complete block with one ProseMirror transaction.
 *
 * The destination uses pre-move coordinates. A destination inside the source
 * range, outside the source's immediate container, with a stale node size, or
 * with a schema-invalid insertion is rejected without changing caller state.
 */
export function createBlockMoveTransaction(
  state: EditorState,
  { from, size, to }: BlockMove,
): BlockMoveResult | null {
  const node = state.doc.nodeAt(from);
  const documentEnd = state.doc.content.size;
  const sourceParent = from >= 0 && from <= documentEnd ? state.doc.resolve(from).parent : null;
  const destinationParent = to >= 0 && to <= documentEnd ? state.doc.resolve(to).parent : null;

  if (
    !node ||
    size <= 0 ||
    node.nodeSize !== size ||
    from < 0 ||
    from + size > documentEnd ||
    to < 0 ||
    to > documentEnd ||
    sourceParent !== destinationParent ||
    (to >= from && to <= from + size)
  ) {
    return null;
  }

  const movedTo = to > from ? to - size : to;
  if (movedTo === from) return null;

  const slice = state.doc.slice(from, from + size);
  const transaction = state.tr;

  try {
    transaction.delete(from, from + size);
    transaction.insert(movedTo, slice.content);
  } catch {
    return null;
  }

  const selectionPosition = Math.min(movedTo + 1, transaction.doc.content.size);
  transaction.setSelection(
    TextSelection.near(transaction.doc.resolve(selectionPosition)),
  );
  transaction.scrollIntoView();

  return { transaction, movedFrom: from, movedTo };
}
