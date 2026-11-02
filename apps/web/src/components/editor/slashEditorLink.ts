import type { EditorState, Transaction } from "prosemirror-state";
import { TextSelection } from "prosemirror-state";

export interface EditorLinkRange {
  from: number;
  to: number;
  text: string;
}

export function normalizeEditorLinkHref(value: string): string {
  const href = value.trim();
  if (!href) return "";
  if (/^(?:\/|#|\.\.?\/)/.test(href)) return href;
  if (/^(?:https?:|mailto:|tel:|starlane:)/i.test(href)) return href;
  if (/^[a-z][a-z0-9+.-]*:/i.test(href)) return "";
  return `https://${href}`;
}

export function createEditorLinkTransaction(
  state: EditorState,
  range: EditorLinkRange,
  hrefValue: string,
): Transaction | null {
  const href = normalizeEditorLinkHref(hrefValue);
  const linkType = state.schema.marks.link;
  if (!href || !linkType) return null;

  const from = Math.max(0, Math.min(range.from, range.to, state.doc.content.size));
  const to = Math.max(from, Math.min(Math.max(range.from, range.to), state.doc.content.size));
  const text = range.text.trim() || href;
  const linkMark = linkType.create({
    href,
    class: "starlane-link cursor-pointer",
    target: "_blank",
    rel: "noopener noreferrer",
  });
  const currentText = state.doc.textBetween(from, to, "", "");

  if (currentText === text && from < to) {
    const transaction = state.tr.removeMark(from, to, linkType).addMark(from, to, linkMark);
    transaction.setSelection(TextSelection.create(transaction.doc, to));
    return transaction.scrollIntoView();
  }

  const sourceMarks = from < state.doc.content.size ? state.doc.resolve(from).marks() : [];
  const preservedMarks = sourceMarks.filter((mark) => mark.type !== linkType);
  const replacement = state.schema.text(text, [...preservedMarks, linkMark]);
  const transaction = state.tr.replaceWith(from, to, replacement);
  transaction.setSelection(TextSelection.create(transaction.doc, from + text.length));
  return transaction.scrollIntoView();
}

export function createEditorUnlinkTransaction(
  state: EditorState,
  range: Pick<EditorLinkRange, "from" | "to">,
): Transaction | null {
  const linkType = state.schema.marks.link;
  if (!linkType) return null;
  const from = Math.max(0, Math.min(range.from, range.to, state.doc.content.size));
  const to = Math.max(from, Math.min(Math.max(range.from, range.to), state.doc.content.size));
  if (from === to) return null;
  const transaction = state.tr.removeMark(from, to, linkType);
  transaction.setSelection(TextSelection.create(transaction.doc, to));
  return transaction.scrollIntoView();
}
