import { Extension, type Editor } from "@tiptap/core";
import { Plugin, PluginKey } from "prosemirror-state";
import { Decoration, DecorationSet } from "prosemirror-view";

export interface DraftPeerEditorRange {
  connectionId: string;
  displayName: string;
  anchor: number;
  head: number;
}

const key = new PluginKey<DecorationSet>("starlaneDraftPeerCursors");

export const DraftPeerCursorDecorations = Extension.create({
  name: "starlaneDraftPeerCursors",
  addProseMirrorPlugins() {
    return [new Plugin({
      key,
      state: {
        init: () => DecorationSet.empty,
        apply(transaction,current) {
          const replacement = transaction.getMeta(key) as DraftPeerEditorRange[] | undefined;
          if (!replacement) return current.map(transaction.mapping,transaction.doc);
          const max = transaction.doc.content.size;
          const decorations: Decoration[] = [];
          for (const range of replacement) {
            const anchor = Math.max(0,Math.min(max,range.anchor));
            const head = Math.max(0,Math.min(max,range.head));
            const from = Math.min(anchor,head), to = Math.max(anchor,head);
            if (from < to) decorations.push(Decoration.inline(from,to,{class:"starlane-draft-peer-selection","data-peer-connection":range.connectionId}));
            decorations.push(Decoration.widget(head,() => {
              const caret = document.createElement("span");
              caret.className = "starlane-draft-peer-caret";
              caret.dataset.peerConnection = range.connectionId;
              caret.setAttribute("aria-label",`${range.displayName} cursor`);
              caret.setAttribute("contenteditable","false");
              const label = document.createElement("span");
              label.className = "starlane-draft-peer-caret-label";
              label.textContent = range.displayName;
              caret.append(label);
              return caret;
            },{key:`peer-${range.connectionId}`,side:1}));
          }
          return DecorationSet.create(transaction.doc,decorations);
        },
      },
      props:{decorations(state){return key.getState(state);}},
    })];
  },
});

export function updateDraftPeerCursorDecorations(editor: Editor, ranges: DraftPeerEditorRange[]): void {
  editor.view.dispatch(editor.state.tr.setMeta(key,ranges));
}
