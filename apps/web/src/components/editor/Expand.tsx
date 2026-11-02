import { ChevronDown } from "lucide-react";
import { useState, type ChangeEvent } from "react";
import type { ReactNodeViewProps } from "@tiptap/react";
import { NodeViewContent, NodeViewWrapper } from "@tiptap/react";
import { Input } from "@/components/ui/input";

export const DEFAULT_EXPAND_TITLE = "Click to expand";

export function normalizeExpandTitle(value: string | null | undefined): string {
  return value?.trim().slice(0, 160) || DEFAULT_EXPAND_TITLE;
}

export function parseExpandDirective(value: string): { title: string; content: string } | null {
  const match = value.trim().match(/^:::expand(?:\s+(.+?))?\n([\s\S]*?)\n:::\s*$/i);
  if (!match) return null;
  return { title: normalizeExpandTitle(match[1]), content: match[2].trim() };
}

export function serializeExpandDirective(title: string, content: string): string {
  return `:::expand ${normalizeExpandTitle(title)}\n${content.trim()}\n:::`;
}

export function ExpandNodeView({ node, updateAttributes, editor }: ReactNodeViewProps) {
  const title = normalizeExpandTitle(node.attrs.title);
  const [open, setOpen] = useState(editor.isEditable);

  return (
    <NodeViewWrapper
      data-expand-block="true"
      className="my-3 overflow-hidden rounded-card border border-border bg-surface"
    >
      <div className="flex min-h-11 w-full items-center justify-between gap-3 px-4 py-3 text-left text-sm font-medium text-text">
        {editor.isEditable ? (
          <Input
            aria-label="Expand title"
            value={title}
            onChange={(event: ChangeEvent<HTMLInputElement>) =>
              updateAttributes({ title: normalizeExpandTitle(event.target.value) })}
            onClick={(event) => event.stopPropagation()}
            contentEditable={false}
            className="h-8 bg-transparent text-sm"
          />
        ) : (
          <button
            type="button"
            className="flex min-w-0 flex-1 items-center text-left hover:text-primary"
            aria-expanded={open}
            onClick={() => setOpen((current) => !current)}
          >
            <span>{title}</span>
          </button>
        )}
        <button
          type="button"
          aria-label={open ? "Collapse expand block" : "Expand expand block"}
          aria-expanded={open}
          className="rounded-button p-1 text-text-muted hover:bg-surface-raised hover:text-text"
          onClick={() => setOpen((current) => !current)}
        >
          <ChevronDown
            size={16}
            aria-hidden="true"
            className={`transition-transform duration-150 ${open ? "rotate-180" : ""}`}
          />
        </button>
      </div>
      <div hidden={!open} className="border-t border-border px-4 py-3">
        <NodeViewContent className="text-text min-h-6 outline-none" />
      </div>
    </NodeViewWrapper>
  );
}
