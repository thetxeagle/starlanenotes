import { ChevronRight, FileText, ListTree } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { dataClient, type DocumentRecord } from "@/lib/dataClient";

export const CHILD_PAGES_DIRECTIVE = "[[children]]";
export const COLLAPSIBLE_CHILD_PAGES_DIRECTIVE = "[[children:collapsed]]";

export type ChildPagesVariant = "list" | "collapsible";

export function parseChildPagesDirective(value: string): ChildPagesVariant | null {
  const normalized = value.trim().toLowerCase();
  if (normalized === CHILD_PAGES_DIRECTIVE) return "list";
  if (normalized === COLLAPSIBLE_CHILD_PAGES_DIRECTIVE) return "collapsible";
  return null;
}

export function isChildPagesDirective(value: string): boolean {
  return parseChildPagesDirective(value) !== null;
}

export function getDirectChildPages(
  documents: readonly Pick<DocumentRecord, "id" | "title" | "parent_id" | "updated_at">[],
  parentId: string,
): DocumentRecord[] {
  return documents
    .filter((document) => document.parent_id === parentId)
    .sort((left, right) => {
      const titleOrder = left.title.localeCompare(right.title);
      if (titleOrder !== 0) return titleOrder;
      return String(left.updated_at ?? "").localeCompare(String(right.updated_at ?? ""));
    }) as DocumentRecord[];
}

function documentPath(spaceId: string, documentId: string): string {
  return `/spaces/${encodeURIComponent(spaceId)}/docs/${encodeURIComponent(documentId)}`;
}

export function renderChildPagesHtml(
  documents: readonly Pick<DocumentRecord, "id" | "title">[],
  spaceId: string,
  variant: ChildPagesVariant,
): string {
  const escape = (value: string) =>
    value.replace(/[&<>"']/g, (character) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[character] || character);
  const items = documents
    .map(
      (document) =>
        `<li><a href="${documentPath(spaceId, document.id)}" class="group focus-ring flex items-center gap-3 rounded-button border border-border bg-surface-raised px-3 py-2 text-sm text-text transition-colors duration-100 hover:bg-hover"><span class="min-w-0 flex-1 truncate">${escape(document.title || "Untitled")}</span><span class="shrink-0 text-text-muted" aria-hidden="true">›</span></a></li>`,
    )
    .join("");
  const body = items || `<p class="mt-3 text-sm text-text-muted">Child pages will appear here when pages are nested under this page.</p>`;
  if (variant === "collapsible") {
    return `<details class="my-4 rounded-card border border-border bg-surface"><summary class="focus-ring flex cursor-pointer list-none items-center px-4 py-3 text-sm font-medium text-text">Child pages</summary><div class="border-t border-border px-4 pb-4">${items ? `<ul class="mt-3 grid gap-2">${items}</ul>` : body}</div></details>`;
  }
  return `<section class="my-4 rounded-card border border-border bg-surface p-4"><div class="text-sm font-medium text-text">Child pages</div>${items ? `<ul class="mt-3 grid gap-2">${items}</ul>` : body}</section>`;
}

export interface ChildPagesBlockProps {
  spaceId?: string;
  docId?: string;
  variant?: ChildPagesVariant;
  interactive?: boolean;
}

export function ChildPagesBlock({
  spaceId,
  docId,
  variant = "list",
  interactive = true,
}: ChildPagesBlockProps) {
  const [documents, setDocuments] = useState<DocumentRecord[]>([]);
  const [loading, setLoading] = useState(Boolean(spaceId && docId));

  useEffect(() => {
    let cancelled = false;
    if (!spaceId || !docId) {
      setDocuments([]);
      setLoading(false);
      return;
    }
    setLoading(true);
    void dataClient
      .listDocuments(spaceId, { include_nested: true })
      .then((next) => {
        if (!cancelled) setDocuments(getDirectChildPages(next, docId));
      })
      .catch(() => {
        if (!cancelled) setDocuments([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [docId, spaceId]);

  const title = variant === "collapsible" ? "Child pages" : "Pages in this page";
  const emptyLabel = loading
    ? "Loading child pages…"
    : "Child pages will appear here when pages are nested under this page.";
  const content = useMemo(
    () =>
      documents.length > 0 ? (
        <ul className="mt-3 grid gap-2">
          {documents.map((document) => (
            <li key={document.id}>
              <a
                href={spaceId ? documentPath(spaceId, document.id) : "#"}
                onClick={(event) => {
                  if (!interactive) event.preventDefault();
                }}
                className="group focus-ring flex items-center gap-3 rounded-button border border-border bg-surface-raised px-3 py-2 text-sm text-text transition-colors duration-100 hover:bg-hover"
              >
                <FileText size={15} className="shrink-0 text-primary" aria-hidden="true" />
                <span className="min-w-0 flex-1 truncate">{document.title || "Untitled"}</span>
                <ChevronRight size={15} className="shrink-0 text-text-muted" aria-hidden="true" />
              </a>
            </li>
          ))}
        </ul>
      ) : (
        <p className="mt-3 text-sm text-text-muted">{emptyLabel}</p>
      ),
    [documents, emptyLabel, interactive, spaceId],
  );

  if (variant === "collapsible") {
    return (
      <details className="my-4 rounded-card border border-border bg-surface" contentEditable={false}>
        <summary className="focus-ring flex cursor-pointer list-none items-center gap-2 px-4 py-3 text-sm font-medium text-text [&::-webkit-details-marker]:hidden">
          <ListTree size={16} className="text-primary" aria-hidden="true" />
          <span className="flex-1">{title}</span>
          <ChevronRight size={16} className="text-text-muted transition-transform [[open]>&]:rotate-90" aria-hidden="true" />
        </summary>
        <div className="border-t border-border px-4 pb-4">{content}</div>
      </details>
    );
  }

  return (
    <section className="my-4 rounded-card border border-border bg-surface p-4" contentEditable={false} aria-label="Child pages">
      <div className="flex items-center gap-2 text-sm font-medium text-text">
        <ListTree size={16} className="text-primary" aria-hidden="true" />
        {title}
      </div>
      {content}
    </section>
  );
}
