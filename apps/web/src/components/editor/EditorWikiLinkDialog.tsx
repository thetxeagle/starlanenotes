import { useRef } from "react";
import { FileText, RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";
import { RESPONSIVE_MENU_OPTION_HEIGHT_CLASS } from "@/components/ui/form-control";
import { Input } from "@/components/ui/input";
import { useListboxKeyboardNavigation } from "@/components/ui/useListboxKeyboardNavigation";

export interface EditorWikiLinkDocument {
  id: string;
  title: string;
  icon?: string;
  folderName?: string;
  updatedAt?: string;
}

export function getWikiLinkDocumentIdLabel(
  document: EditorWikiLinkDocument,
  documents: readonly EditorWikiLinkDocument[],
): string | null {
  const duplicateTitles = documents.filter(
    (candidate) => candidate.title.trim().toLowerCase() === document.title.trim().toLowerCase(),
  );
  if (duplicateTitles.length < 2) return null;

  const normalizedId = document.id.toLowerCase();
  let prefixLength = Math.min(4, normalizedId.length);
  while (
    prefixLength < normalizedId.length &&
    duplicateTitles.some(
      (candidate) =>
        candidate.id !== document.id &&
        candidate.id.toLowerCase().slice(0, prefixLength) === normalizedId.slice(0, prefixLength),
    )
  ) {
    prefixLength += 1;
  }

  return `Page ID ${document.id.slice(0, prefixLength)}`;
}

export function filterEditorWikiLinkDocuments(
  documents: readonly EditorWikiLinkDocument[],
  query: string,
  currentDocumentId?: string,
): EditorWikiLinkDocument[] {
  const normalizedQuery = query.trim().toLowerCase();
  return documents
    .filter((document) => document.id !== currentDocumentId)
    .filter((document) => document.title.toLowerCase().includes(normalizedQuery))
    .sort((left, right) => {
      const leftTitle = left.title.toLowerCase();
      const rightTitle = right.title.toLowerCase();
      const leftRank = !normalizedQuery
        ? 0
        : leftTitle === normalizedQuery
          ? 0
          : leftTitle.startsWith(normalizedQuery)
            ? 1
            : 2 + leftTitle.indexOf(normalizedQuery);
      const rightRank =
        !normalizedQuery
          ? 0
          : rightTitle === normalizedQuery
            ? 0
            : rightTitle.startsWith(normalizedQuery)
              ? 1
              : 2 + rightTitle.indexOf(normalizedQuery);
      return leftRank - rightRank || leftTitle.localeCompare(rightTitle);
    });
}

export function formatWikiLinkUpdatedAt(value?: string): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return `Updated ${new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(date)}`;
}

interface EditorWikiLinkDialogProps {
  documents: readonly EditorWikiLinkDocument[];
  currentDocumentId?: string;
  query: string;
  loading: boolean;
  error: boolean;
  onQueryChange: (query: string) => void;
  onSelect: (document: EditorWikiLinkDocument) => void;
  onRetry: () => void;
  onCancel: () => void;
}

export function EditorWikiLinkDialog({
  documents,
  currentDocumentId,
  query,
  loading,
  error,
  onQueryChange,
  onSelect,
  onRetry,
  onCancel,
}: EditorWikiLinkDialogProps) {
  const filteredDocuments = filterEditorWikiLinkDocuments(documents, query, currentDocumentId);
  const listboxRef = useRef<HTMLDivElement | null>(null);
  const handleListboxKeyDown = useListboxKeyboardNavigation(listboxRef);

  return (
    <DialogShell
      labelledBy="editor-wiki-link-title"
      describedBy="editor-wiki-link-description"
      maxWidth="sm"
      onClose={onCancel}
    >
      <DialogHeader>
        <div>
          <DialogTitle id="editor-wiki-link-title">Link to document</DialogTitle>
          <DialogDescription id="editor-wiki-link-description">
            Choose a page in this space to keep a portable wiki link.
          </DialogDescription>
        </div>
      </DialogHeader>
      <DialogBody>
        <label htmlFor="editor-wiki-link-search" className="block text-sm font-medium text-text">
          Search documents
        </label>
        <Input
          id="editor-wiki-link-search"
          value={query}
          onChange={(event) => onQueryChange(event.target.value)}
          className="mt-1 h-8 bg-surface text-sm"
          placeholder="Search documents"
          autoFocus
          autoComplete="off"
          onKeyDown={handleListboxKeyDown}
        />

        <div
          ref={listboxRef}
          role="listbox"
          aria-label="Pages in this space"
          aria-busy={loading}
          onKeyDown={handleListboxKeyDown}
          className="mt-3 max-h-56 overflow-y-auto rounded-input border border-border p-1"
        >
          {loading ? (
            <p role="status" className="px-2 py-2 text-sm text-text-muted">
              Loading pages…
            </p>
          ) : error ? (
            <div className="grid justify-items-start gap-2 px-2 py-2">
              <p className="text-sm text-danger">Unable to load pages.</p>
              <Button type="button" variant="secondary" size="sm" onClick={onRetry}>
                <RotateCcw size={14} aria-hidden="true" />
                Retry
              </Button>
            </div>
          ) : filteredDocuments.length ? (
            filteredDocuments.map((document, index) => {
              const updatedAt = formatWikiLinkUpdatedAt(document.updatedAt);
              const documentIdLabel = getWikiLinkDocumentIdLabel(document, filteredDocuments);
              return (
                <button
                  key={document.id}
                  type="button"
                  role="option"
                  aria-selected="false"
                  tabIndex={index === 0 ? 0 : -1}
                  className={`rounded-button flex w-full items-center gap-2 px-2 py-2 text-left text-sm text-text transition-colors duration-100 hover:bg-surface-raised hover:text-primary ${RESPONSIVE_MENU_OPTION_HEIGHT_CLASS}`}
                  onClick={() => onSelect(document)}
                >
                  <span
                    className="flex h-7 w-7 shrink-0 items-center justify-center text-base"
                    aria-hidden="true"
                  >
                    {document.icon || <FileText size={15} className="text-text-muted" />}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">{document.title}</span>
                    <span className="text-text-muted block truncate text-xs">
                      {document.folderName || "Space root"}
                      {documentIdLabel ? ` · ${documentIdLabel}` : ""}
                      {updatedAt ? ` · ${updatedAt}` : ""}
                    </span>
                  </span>
                </button>
              );
            })
          ) : (
            <p className="px-2 py-2 text-sm text-text-muted">
              {query.trim()
                ? `No pages match “${query.trim()}”.`
                : "No other pages are available to link."}
            </p>
          )}
        </div>
      </DialogBody>
      <DialogFooter>
        <Button type="button" variant="secondary" size="sm" onClick={onCancel}>
          Cancel
        </Button>
      </DialogFooter>
    </DialogShell>
  );
}
