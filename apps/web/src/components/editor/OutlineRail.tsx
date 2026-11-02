import { useMemo, useState } from "react";
import { ChevronDown, ListTree } from "lucide-react";
import { cn } from "@/lib/utils";
import { parseDocumentHeadings } from "@/lib/documentOutline";
import type { TableOfContentsEntry } from "@/components/editor/TableOfContents";

interface DocumentOutlineAccordionProps {
  content: string;
}

function buildOutlineEntries(content: string): TableOfContentsEntry[] {
  return parseDocumentHeadings(content).map((heading) => ({
    id: heading.id,
    text: heading.text,
    level: heading.level,
  }));
}

function focusOutlineEntry(entries: TableOfContentsEntry[], entry: TableOfContentsEntry): void {
  const root = document.querySelector(".markdown-preview");
  if (!root) return;
  const entryIndex = entries.findIndex((candidate) => candidate.id === entry.id);
  const headings = Array.from(root.querySelectorAll<HTMLElement>("h1, h2, h3, h4, h5, h6"));
  const target = headings[entryIndex];
  if (!target) return;
  target.tabIndex = -1;
  target.scrollIntoView({ block: "start" });
  target.focus({ preventScroll: true });
}

function outlineIndent(level: number): string {
  if (level <= 1) return "";
  if (level === 2) return "pl-3";
  return "pl-6";
}

/** A small, closed-by-default table of contents that stays in document flow. */
export function DocumentOutlineAccordion({ content }: DocumentOutlineAccordionProps) {
  const [open, setOpen] = useState(false);
  const entries = useMemo(() => buildOutlineEntries(content), [content]);

  if (entries.length < 2) return null;

  return (
    <section className="border-border bg-surface mb-4 overflow-hidden rounded-md border">
      <button
        type="button"
        className="focus-ring text-text hover:bg-surface-raised flex min-h-10 w-full items-center justify-between gap-3 px-3 py-2 text-left text-sm"
        aria-expanded={open}
        aria-controls="document-outline-accordion"
        onClick={() => setOpen((current) => !current)}
      >
        <span className="flex min-w-0 items-center gap-2 font-medium">
          <ListTree size={15} aria-hidden="true" />
          <span>On this page</span>
        </span>
        <ChevronDown
          size={15}
          aria-hidden="true"
          className={cn("shrink-0 transition-transform duration-150", open && "rotate-180")}
        />
      </button>
      {open && (
        <nav
          id="document-outline-accordion"
          aria-label="Table of contents"
          className="border-border border-t px-2 py-2"
        >
          <ol className="flex max-h-64 flex-col gap-0.5 overflow-y-auto">
            {entries.map((entry) => (
              <li key={entry.id} className={outlineIndent(entry.level)}>
                <button
                  type="button"
                  onClick={() => focusOutlineEntry(entries, entry)}
                  className="focus-ring text-text-muted hover:bg-surface-raised hover:text-text flex min-h-9 w-full items-center truncate rounded px-2 py-1.5 text-left text-sm"
                  aria-label={`Go to ${entry.text}`}
                >
                  {entry.text}
                </button>
              </li>
            ))}
          </ol>
        </nav>
      )}
    </section>
  );
}

// Kept as a compatibility export for consumers that still lazy-load this module.
export const OutlineRail = DocumentOutlineAccordion;
export const MobileDocumentOutline = DocumentOutlineAccordion;
