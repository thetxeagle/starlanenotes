/**
 * State boundary inspired by slash-editor 0.7.0 `packages/core/src/slash-command.ts` (MIT).
 * Copyright 2025-present Nhat Bui.
 *
 * Starlane retains its own command catalog and execution paths. This module
 * only defines when an inline slash query is valid and which editor range it
 * owns.
 */
import type { EditorState } from "@tiptap/pm/state";

export interface InlineSlashQuery {
  from: number;
  to: number;
  query: string;
}

export function canStartInlineSlashMenu(state: EditorState, from: number): boolean {
  if (!state.selection.empty || from < 0 || from > state.doc.content.size) return false;
  const resolved = state.doc.resolve(from);
  if (resolved.parent.type.spec.code) return false;
  if (resolved.parentOffset === 0) return true;
  return /\s/.test(resolved.parent.textBetween(resolved.parentOffset - 1, resolved.parentOffset));
}

export function deriveInlineSlashQuery(
  state: EditorState,
  triggerFrom: number,
): InlineSlashQuery | null {
  if (!state.selection.empty) return null;
  const to = state.selection.from;
  if (triggerFrom < 0 || triggerFrom >= to || to > state.doc.content.size) return null;

  const start = state.doc.resolve(triggerFrom);
  const end = state.doc.resolve(to);
  if (!start.sameParent(end) || start.parent.type.spec.code) return null;

  const text = state.doc.textBetween(triggerFrom, to, "", "");
  if (!text.startsWith("/") || /\s/.test(text.slice(1))) return null;
  return { from: triggerFrom, to, query: text.slice(1) };
}

export function findInlineSlashQuery(state: EditorState): InlineSlashQuery | null {
  if (!state.selection.empty) return null;
  const resolved = state.selection.$from;
  if (resolved.parent.type.spec.code) return null;

  const textBeforeCursor = resolved.parent.textBetween(0, resolved.parentOffset, "", "");
  const match = textBeforeCursor.match(/(?:^|\s)(\/[^\s/]*)$/);
  if (!match) return null;

  const slashOffset = match.index! + match[0].lastIndexOf("/");
  return {
    from: resolved.start() + slashOffset,
    to: state.selection.from,
    query: match[1].slice(1),
  };
}
