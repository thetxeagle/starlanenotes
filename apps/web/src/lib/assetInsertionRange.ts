import type { Editor as TipTapEditor } from "@tiptap/react";
import type { Transaction } from "@tiptap/pm/state";

export interface AssetInsertionPosition {
  from: number;
  to: number;
}
export interface AssetInsertionRange {
  current(): AssetInsertionPosition;
  /** Immediately after successful insertion at current(), before any await. */
  advance(): void;
  close(): void;
}
export class AssetInsertionRangeError extends Error {
  constructor() {
    super(
      "The original file insertion position changed or was removed. Retain the file and review any completed upload before inserting again.",
    );
    this.name = "AssetInsertionRangeError";
  }
}

/** Actual editor positions only; these are never Markdown/source cursor offsets. */
export function captureAssetInsertionRange(
  editor: TipTapEditor,
  range?: AssetInsertionPosition,
): AssetInsertionRange {
  let from = range?.from ?? editor.state.selection.to;
  let to = range?.to ?? from;
  let document = editor.state.doc;
  let closed = false;
  let invalid = false;
  let terminal = false;
  let insertionArmed = false;
  const bounded = () =>
    Number.isSafeInteger(from) &&
    Number.isSafeInteger(to) &&
    from >= 0 &&
    from <= to &&
    to <= document.content.size;
  const refuse = (): never => {
    terminal = true;
    throw new AssetInsertionRangeError();
  };
  if (editor.isDestroyed || !bounded()) refuse();

  const mapTransaction = (transaction: Transaction) => {
    if (transaction.before !== document) {
      invalid = true;
      terminal = true;
      return;
    }
    if (!invalid) {
      for (const map of transaction.mapping.maps) {
        if (from === to) {
          const mapped = map.mapResult(from, 1);
          if (mapped.deleted) {
            invalid = true;
            break;
          }
          from = to = mapped.pos;
        } else {
          // Any replacement/deletion of original slash bytes, or insertion
          // inside them, invalidates the original replacement span.
          map.forEach((start, end) => {
            if (
              (start < end && start < to && end > from) ||
              (start === end && start > from && start < to)
            )
              invalid = true;
          });
          if (invalid) break;
          const left = map.mapResult(from, 1),
            right = map.mapResult(to, -1);
          if (left.deleted || right.deleted) {
            invalid = true;
            break;
          }
          from = left.pos;
          to = right.pos;
        }
      }
    }
    document = transaction.doc;
    if (!bounded()) invalid = true;
  };
  const onTransaction = ({
    transaction,
    appendedTransactions = [],
  }: {
    transaction: Transaction;
    appendedTransactions?: readonly Transaction[];
  }) => {
    if (closed || terminal) return;
    // TipTap emits one event AFTER applying the root plus every plugin-appended
    // transaction. Keep the complete ordered before/doc chain, including when
    // our own slash replacement awaits its synchronous advance() acknowledgement.
    for (const applied of [transaction, ...appendedTransactions]) {
      mapTransaction(applied);
      if (terminal) return;
    }
    if (editor.state.doc !== document) {
      invalid = true;
      terminal = true;
    }
  };
  editor.on("transaction", onTransaction);

  const available = () => {
    if (closed || terminal || editor.isDestroyed || editor.state.doc !== document) refuse();
  };
  return {
    current() {
      available();
      if (invalid || !bounded()) refuse();
      // The caller may synchronously replace this span with its OWN successful
      // insertion, then acknowledge with advance(). No await may cross that pair.
      insertionArmed = true;
      return { from, to };
    },
    advance() {
      available();
      if (!insertionArmed) refuse();
      from = to = editor.state.selection.to;
      if (!bounded()) refuse();
      invalid = false;
      insertionArmed = false;
    },
    close() {
      if (closed) return;
      closed = true;
      editor.off("transaction", onTransaction);
    },
  };
}
