import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Clock3, RefreshCw } from "lucide-react";
import { Button, RESPONSIVE_TOUCH_ACTION_CLASS } from "@/components/ui/button";
import { DocumentIdentityIcon } from "@/components/DocumentIdentityIcon";
import { SearchInput } from "@/components/ui/search-input";
import { dataClient, type DocumentRecord } from "@/lib/dataClient";
import { formatDateMetadata } from "@/lib/dateMetadata";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";

const EMPTY_RECENT_DOCUMENTS: DocumentRecord[] = [];

export function getRecentDocumentPath(spaceId: string, documentId: string): string {
  return buildDocumentRoutePath(spaceId, documentId);
}

export function sortRecentDocuments(documents: DocumentRecord[]): DocumentRecord[] {
  return [...documents].sort((a, b) => {
    const updatedComparison = (b.updated_at ?? "").localeCompare(a.updated_at ?? "");
    if (updatedComparison !== 0) return updatedComparison;
    return a.title.localeCompare(b.title, undefined, { sensitivity: "base" });
  });
}

export function filterRecentDocuments(
  documents: DocumentRecord[],
  query: string,
): DocumentRecord[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return documents;
  return documents.filter((document) => document.title.toLowerCase().includes(normalized));
}

export type RecentDocumentsListState = "loading" | "error" | "empty" | "documents";

export interface RecentDocumentsSnapshot {
  spaceId: string;
  spaceName: string;
  documents: DocumentRecord[];
}

export function getRecentDocumentsSnapshotForSpace(
  snapshot: RecentDocumentsSnapshot | null,
  spaceId: string | undefined,
): RecentDocumentsSnapshot | null {
  return snapshot?.spaceId === spaceId ? snapshot : null;
}

export function isCurrentRecentDocumentsRequest(
  requestSpaceId: string,
  requestId: number,
  currentSpaceId: string | undefined,
  currentRequestId: number,
): boolean {
  return requestSpaceId === currentSpaceId && requestId === currentRequestId;
}

export function getRecentDocumentsListState(
  loading: boolean,
  error: string,
  documentCount: number,
  visibleCount: number,
): RecentDocumentsListState {
  if (loading && documentCount === 0) return "loading";
  if (error && documentCount === 0) return "error";
  if (visibleCount === 0) return "empty";
  return "documents";
}

export function RecentDocumentsPage() {
  const { spaceId } = useParams<{ spaceId: string }>();
  const navigate = useNavigate();
  const [snapshot, setSnapshot] = useState<RecentDocumentsSnapshot | null>(null);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<{ spaceId: string; message: string } | null>(null);
  const requestIdRef = useRef(0);
  const currentSpaceIdRef = useRef(spaceId);
  currentSpaceIdRef.current = spaceId;

  const refresh = useCallback(async () => {
    if (!spaceId) return;
    const requestId = ++requestIdRef.current;
    setLoading(true);
    setError(null);
    try {
      const [space, nextDocuments] = await Promise.all([
        dataClient.getSpace(spaceId),
        dataClient.listDocuments(spaceId, { include_nested: true }),
      ]);
      if (
        !isCurrentRecentDocumentsRequest(
          spaceId,
          requestId,
          currentSpaceIdRef.current,
          requestIdRef.current,
        )
      ) {
        return;
      }
      setSnapshot({ spaceId, spaceName: space.name, documents: sortRecentDocuments(nextDocuments) });
    } catch {
      if (
        isCurrentRecentDocumentsRequest(
          spaceId,
          requestId,
          currentSpaceIdRef.current,
          requestIdRef.current,
        )
      ) {
        setError({ spaceId, message: "Unable to load recent pages." });
      }
    } finally {
      if (
        isCurrentRecentDocumentsRequest(
          spaceId,
          requestId,
          currentSpaceIdRef.current,
          requestIdRef.current,
        )
      ) {
        setLoading(false);
      }
    }
  }, [spaceId]);

  useEffect(() => {
    void refresh();
    return () => {
      requestIdRef.current += 1;
    };
  }, [refresh]);

  const currentSnapshot = getRecentDocumentsSnapshotForSpace(snapshot, spaceId);
  const documents = currentSnapshot?.documents ?? EMPTY_RECENT_DOCUMENTS;
  const currentError = error && error.spaceId === spaceId ? error.message : "";
  const currentLoading = loading || (Boolean(spaceId) && !currentSnapshot && !currentError);

  const visibleDocuments = useMemo(
    () => filterRecentDocuments(documents, query),
    [documents, query],
  );
  const listState = getRecentDocumentsListState(
    currentLoading,
    currentError,
    documents.length,
    visibleDocuments.length,
  );

  return (
    <div className="flex h-full flex-col">
      <header className="border-border border-b px-4 py-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="text-text flex items-center gap-2 text-sm font-semibold">
              <Clock3 size={16} className="text-text-muted" />
              Recent pages
            </div>
            <p className="text-text-muted mt-1 text-xs">
              {visibleDocuments.length} of {documents.length} in{" "}
              {currentSnapshot?.spaceName || "this space"}
            </p>
          </div>
          <Button
            type="button"
            variant="secondary"
            size="sm"
            className={RESPONSIVE_TOUCH_ACTION_CLASS}
            onClick={() => void refresh()}
          >
            <RefreshCw size={14} />
            {currentLoading ? "Refreshing..." : "Refresh"}
          </Button>
        </div>
        <SearchInput
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search recent pages"
          aria-label="Search recent pages"
          containerClassName="mt-3"
        />
      </header>

      <main className="flex-1 overflow-y-auto p-4">
        {currentError && (
          <div
            role="alert"
            className="mb-3 break-words rounded-input border border-danger/30 bg-danger/10 px-3 py-2 text-sm text-danger"
          >
            {currentError}
          </div>
        )}
        {listState === "loading" ? (
          <p className="text-text-muted px-2 py-3 text-sm" role="status">
            Loading recent pages…
          </p>
        ) : listState === "error" ? null : listState === "empty" ? (
          <div className="flex h-full min-h-64 items-center justify-center text-center">
            <div>
              <Clock3 size={28} className="text-text-muted mx-auto" />
              <p className="text-text mt-2 text-sm">
                {query.trim() ? "No recent pages match this search" : "No recent pages"}
              </p>
            </div>
          </div>
        ) : (
          <div className="rounded-input border-border overflow-hidden border">
            {visibleDocuments.map((document) => (
              <button
                key={document.id}
                type="button"
                onClick={() => {
                  if (spaceId) navigate(getRecentDocumentPath(spaceId, document.id));
                }}
                className="border-border hover:bg-surface-raised flex w-full items-center gap-3 border-b px-3 py-2 text-left last:border-b-0"
              >
                <DocumentIdentityIcon icon={document.icon} />
                <span className="min-w-0 flex-1">
                  <span className="text-text block truncate text-sm font-medium">
                    {document.title}
                  </span>
                  <span className="text-text-muted block truncate text-xs">
                    {formatDateMetadata(
                      document.updated_at,
                      "Updated",
                      "Update date unavailable",
                    )}
                  </span>
                </span>
              </button>
            ))}
          </div>
        )}
      </main>
    </div>
  );
}
