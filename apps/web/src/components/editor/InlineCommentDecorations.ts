import { Extension, type Editor } from "@tiptap/core";
import { Plugin, PluginKey } from "prosemirror-state";
import { Decoration, DecorationSet } from "prosemirror-view";
import type { DocumentComment } from "@/lib/documentComments";

export interface InlineCommentRange {
  id: string;
  from: number;
  to: number;
}

const inlineCommentPluginKey = new PluginKey<DecorationSet>("starlaneInlineComments");

export const InlineCommentDecorations = Extension.create({
  name: "starlaneInlineComments",

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: inlineCommentPluginKey,
        state: {
          init: () => DecorationSet.empty,
          apply(transaction, current) {
            const replacement = transaction.getMeta(inlineCommentPluginKey) as
              | InlineCommentRange[]
              | undefined;
            if (replacement) {
              const max = transaction.doc.content.size;
              return DecorationSet.create(
                transaction.doc,
                replacement
                  .filter(({ from, to }) => from >= 0 && from < to && to <= max)
                  .map(({ id, from, to }) =>
                    Decoration.inline(from, to, {
                      class: "starlane-inline-comment",
                      "data-inline-comment-id": id,
                    }),
                  ),
              );
            }
            return current.map(transaction.mapping, transaction.doc);
          },
        },
        props: {
          decorations(state) {
            return inlineCommentPluginKey.getState(state);
          },
        },
      }),
    ];
  },
});

export function getInlineCommentRanges(
  comments: DocumentComment[],
  visible: boolean,
): InlineCommentRange[] {
  if (!visible) return [];
  return comments.flatMap((comment) => {
    if (
      comment.resolved ||
      comment.selection_start === undefined ||
      comment.selection_end === undefined
    ) {
      return [];
    }
    return [
      {
        id: comment.id,
        from: comment.selection_start,
        to: comment.selection_end,
      },
    ];
  });
}

export function updateInlineCommentDecorations(
  editor: Editor,
  comments: DocumentComment[],
  visible: boolean,
): void {
  editor.view.dispatch(
    editor.state.tr.setMeta(inlineCommentPluginKey, getInlineCommentRanges(comments, visible)),
  );
}
