import type { Editor as TipTapEditor } from "@tiptap/core";
import { useEditorState } from "@tiptap/react";
import type { Node as ProseMirrorNode } from "prosemirror-model";

export const TABLE_OF_CONTENTS_DIRECTIVE = "[[toc]]";

export interface TableOfContentsEntry {
  id: string;
  text: string;
  level: number;
}

export function isTableOfContentsDirective(value: string): boolean {
  return value.trim().toLowerCase() === TABLE_OF_CONTENTS_DIRECTIVE;
}

export function getEditorTableOfContentsEntries(doc: ProseMirrorNode): TableOfContentsEntry[] {
  const entries: TableOfContentsEntry[] = [];

  doc.descendants((node, position) => {
    if (node.type.name !== "heading") return;
    const text = node.textContent.trim();
    if (!text) return;
    entries.push({
      id: `heading-${position}`,
      text,
      level: Number(node.attrs.level) || 1,
    });
  });

  return entries;
}

function indentationClass(level: number): string {
  if (level <= 1) return "";
  if (level === 2) return "pl-3";
  return "pl-6";
}

export function TableOfContentsList({
  entries,
  onSelect,
}: {
  entries: TableOfContentsEntry[];
  onSelect?: (entry: TableOfContentsEntry) => void;
}) {
  return (
    <nav
      aria-label="Table of contents"
      className="my-4 rounded-card border border-border bg-surface p-4"
      contentEditable={false}
    >
      <p className="text-sm font-medium text-text">On this page</p>
      {entries.length > 0 ? (
        <ol className="mt-2 flex flex-col gap-1">
          {entries.map((entry) => (
            <li key={entry.id} className={indentationClass(entry.level)}>
              <button
                type="button"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => onSelect?.(entry)}
                className="rounded-button block w-full truncate px-2 py-1 text-left text-sm text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text focus-ring"
                aria-label={`Go to ${entry.text}`}
              >
                {entry.text}
              </button>
            </li>
          ))}
        </ol>
      ) : (
        <p className="mt-2 text-sm text-text-muted">Add headings to build this table of contents.</p>
      )}
    </nav>
  );
}

export function TableOfContentsEditorBlock({ editor }: { editor: TipTapEditor }) {
  const entries = useEditorState({
    editor,
    selector: ({ editor: currentEditor }) => getEditorTableOfContentsEntries(currentEditor.state.doc),
  });

  return (
    <TableOfContentsList
      entries={entries}
      onSelect={(entry) => {
        const position = Number(entry.id.replace("heading-", ""));
        if (!Number.isInteger(position)) return;
        editor.chain().focus().setTextSelection(position + 1).scrollIntoView().run();
      }}
    />
  );
}
