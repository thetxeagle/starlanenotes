import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { RefreshCw, Star, Trash2 } from "lucide-react";
import {
  Button,
  RESPONSIVE_TOUCH_ACTION_CLASS,
  RESPONSIVE_TOUCH_ICON_ACTION_CLASS,
} from "@/components/ui/button";
import { DocumentIdentityIcon } from "@/components/DocumentIdentityIcon";
import { SearchInput } from "@/components/ui/search-input";
import { Select } from "@/components/ui/select";
import { formatDateMetadata } from "@/lib/dateMetadata";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import {
  filterStarredDocuments,
  loadStarredDocumentsWithStatus,
  readStarredDocuments,
  toggleStarredDocumentSynced,
  type StarredDocumentRecord,
  type StarredDocumentSortMode,
} from "@/lib/starredDocuments";

export interface StarredDocumentGroup {
  key: string;
  label: string;
  documents: StarredDocumentRecord[];
}

export const STARRED_DOCUMENT_GROUP_HEADING_CLASS = "mb-2 text-sm font-medium text-text";

export type StarredDocumentFeedback = {
  tone: "success" | "warning" | "error";
  text: string;
};

export interface StarredMutationLock {
  scope: string;
  rowKeys: Set<string>;
  bulk: boolean;
  refresh: boolean;
}

function adoptStarredMutationScope(lock: StarredMutationLock, scope: string): void {
  if (lock.scope === scope) return;
  lock.scope = scope;
  lock.rowKeys.clear();
  lock.bulk = false;
  lock.refresh = false;
}

export function claimStarredRowMutation(
  lock: StarredMutationLock,
  scope: string,
  key: string,
): boolean {
  adoptStarredMutationScope(lock, scope);
  if (lock.refresh || lock.bulk || lock.rowKeys.has(key)) return false;
  lock.rowKeys.add(key);
  return true;
}

export function releaseStarredRowMutation(
  lock: StarredMutationLock,
  scope: string,
  key: string,
): void {
  if (lock.scope !== scope) return;
  lock.rowKeys.delete(key);
}

export function claimStarredBulkMutation(lock: StarredMutationLock, scope: string): boolean {
  adoptStarredMutationScope(lock, scope);
  if (lock.refresh || lock.bulk || lock.rowKeys.size > 0) return false;
  lock.bulk = true;
  return true;
}

export function releaseStarredBulkMutation(lock: StarredMutationLock, scope: string): void {
  if (lock.scope !== scope) return;
  lock.bulk = false;
}

export function claimStarredRefresh(lock: StarredMutationLock, scope: string): boolean {
  adoptStarredMutationScope(lock, scope);
  if (lock.refresh || lock.bulk || lock.rowKeys.size > 0) return false;
  lock.refresh = true;
  return true;
}

export function releaseStarredRefresh(lock: StarredMutationLock, scope: string): void {
  if (lock.scope !== scope) return;
  lock.refresh = false;
}

export function isStarredOperationCurrent(scope: string, activeScope: string): boolean {
  return scope === activeScope;
}

export function getStarredDocumentFeedbackClass(tone: StarredDocumentFeedback["tone"]): string {
  if (tone === "success") return "border-success/30 bg-success/10 text-success";
  if (tone === "warning") return "border-accent/40 bg-accent/10 text-text";
  return "border-danger/30 bg-danger/10 text-danger";
}

export function getStarredDocumentPath(spaceId: string, documentId: string): string {
  return buildDocumentRoutePath(spaceId, documentId);
}

export function groupStarredDocumentsBySpace(
  documents: StarredDocumentRecord[],
): StarredDocumentGroup[] {
  const groups = new Map<string, StarredDocumentGroup>();
  documents.forEach((document) => {
    const key = document.spaceId;
    const label = document.spaceName?.trim() || "Unknown space";
    const group = groups.get(key) ?? { key, label, documents: [] };
    group.documents.push(document);
    groups.set(key, group);
  });
  return [...groups.values()].sort((a, b) => a.label.localeCompare(b.label));
}

export type StarredDocumentsListState = "loading" | "empty" | "documents";

export function getStarredDocumentsListState(
  loading: boolean,
  documentCount: number,
  visibleCount: number,
): StarredDocumentsListState {
  if (loading && documentCount === 0) return "loading";
  return visibleCount === 0 ? "empty" : "documents";
}

export function StarredDocumentsPage() {
  const { spaceId } = useParams<{ spaceId: string }>();
  const navigate = useNavigate();
  const pageScope = spaceId ?? "";
  const [query, setQuery] = useState("");
  const [sortMode, setSortMode] = useState<StarredDocumentSortMode>("starred_desc");
  const [starredDocuments, setStarredDocuments] = useState<StarredDocumentRecord[]>(() =>
    readStarredDocuments(),
  );
  const [loading, setLoading] = useState(true);
  const [feedback, setFeedback] = useState<StarredDocumentFeedback | null>(null);
  const [pendingUnstarState, setPendingUnstarState] = useState<{
    scope: string;
    keys: Set<string>;
  }>({ scope: pageScope, keys: new Set() });
  const [unstarVisibleScope, setUnstarVisibleScope] = useState<string | null>(null);
  const refreshRequestIdRef = useRef(0);
  const activePageScopeRef = useRef(pageScope);
  activePageScopeRef.current = pageScope;
  const pendingUnstarKeys =
    pendingUnstarState.scope === pageScope ? pendingUnstarState.keys : new Set<string>();
  const unstarVisiblePending = unstarVisibleScope === pageScope;
  const mutationLockRef = useRef<StarredMutationLock>({
    scope: pageScope,
    rowKeys: new Set(),
    bulk: false,
    refresh: false,
  });

  const refresh = async () => {
    const operationScope = pageScope;
    if (!claimStarredRefresh(mutationLockRef.current, operationScope)) return;
    const requestId = ++refreshRequestIdRef.current;
    setLoading(true);
    setFeedback(null);
    try {
      const result = await loadStarredDocumentsWithStatus(spaceId);
      if (requestId !== refreshRequestIdRef.current) return;
      setStarredDocuments(result.documents);
      if (result.syncStatus === "failed") {
        setFeedback({
          tone: "error",
          text: "Unable to refresh Favorites. Showing the saved copy.",
        });
      }
    } finally {
      releaseStarredRefresh(mutationLockRef.current, operationScope);
      if (requestId === refreshRequestIdRef.current) setLoading(false);
    }
  };

  useEffect(() => {
    void refresh();
    const onChange = () => setStarredDocuments(readStarredDocuments());
    window.addEventListener("starlane:starred-documents-changed", onChange);
    return () => {
      refreshRequestIdRef.current += 1;
      window.removeEventListener("starlane:starred-documents-changed", onChange);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [spaceId]);

  const visibleDocuments = useMemo(
    () => filterStarredDocuments(starredDocuments, query, sortMode),
    [query, sortMode, starredDocuments],
  );
  const groups = useMemo(() => groupStarredDocumentsBySpace(visibleDocuments), [visibleDocuments]);
  const listState = getStarredDocumentsListState(
    loading,
    starredDocuments.length,
    visibleDocuments.length,
  );

  async function unstarDocument(document: StarredDocumentRecord) {
    const key = `${document.spaceId}:${document.id}`;
    const operationScope = pageScope;
    if (!claimStarredRowMutation(mutationLockRef.current, operationScope, key)) return;
    const isCurrent = () =>
      isStarredOperationCurrent(operationScope, activePageScopeRef.current);
    setPendingUnstarState({ scope: operationScope, keys: new Set(mutationLockRef.current.rowKeys) });
    try {
      await toggleStarredDocumentSynced(document);
      if (isCurrent()) {
        setStarredDocuments(readStarredDocuments());
        setFeedback({ tone: "success", text: `Removed star from "${document.title}".` });
      }
    } catch {
      if (isCurrent()) {
        setFeedback({ tone: "error", text: "Unable to sync changes to Favorites." });
      }
    } finally {
      releaseStarredRowMutation(mutationLockRef.current, operationScope, key);
      if (isCurrent()) {
        setPendingUnstarState({
          scope: operationScope,
          keys: new Set(mutationLockRef.current.rowKeys),
        });
      }
    }
  }

  async function unstarVisible() {
    const operationScope = pageScope;
    if (
      visibleDocuments.length === 0 ||
      !claimStarredBulkMutation(mutationLockRef.current, operationScope)
    ) {
      return;
    }
    const isCurrent = () =>
      isStarredOperationCurrent(operationScope, activePageScopeRef.current);
    setUnstarVisibleScope(operationScope);
    let removed = 0;
    try {
      for (const document of visibleDocuments) {
        if (!isCurrent()) break;
        try {
          await toggleStarredDocumentSynced(document);
          removed += 1;
        } catch {
          // Keep processing the rest; the message below reports partial failures.
        }
      }
      if (!isCurrent()) return;
      setStarredDocuments(readStarredDocuments());
      const allRemoved = removed === visibleDocuments.length;
      setFeedback({
        tone: allRemoved ? "success" : "warning",
        text: allRemoved
          ? `Removed ${removed} visible ${removed === 1 ? "star" : "stars"}.`
          : `Removed ${removed} of ${visibleDocuments.length} visible stars.`,
      });
    } finally {
      releaseStarredBulkMutation(mutationLockRef.current, operationScope);
      if (isCurrent()) setUnstarVisibleScope(null);
    }
  }

  return (
    <div className="flex h-full flex-col">
      <header className="border-b border-border px-4 py-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-sm font-semibold text-text">
              <Star size={16} className="text-accent" fill="currentColor" />
              Favorites
            </div>
            <p className="mt-1 text-xs text-text-muted">
              {visibleDocuments.length} of {starredDocuments.length} saved shortcuts
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              variant="secondary"
              size="sm"
              className={RESPONSIVE_TOUCH_ACTION_CLASS}
              onClick={() => void refresh()}
              disabled={loading || unstarVisiblePending || pendingUnstarKeys.size > 0}
              aria-busy={loading}
            >
              <RefreshCw size={14} />
              {loading ? "Refreshing..." : "Refresh"}
            </Button>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              className={RESPONSIVE_TOUCH_ACTION_CLASS}
              disabled={
                visibleDocuments.length === 0 ||
                loading ||
                unstarVisiblePending ||
                pendingUnstarKeys.size > 0
              }
              aria-busy={unstarVisiblePending}
              onClick={() => void unstarVisible()}
            >
              <Trash2 size={14} />
              Remove visible from Favorites
            </Button>
          </div>
        </div>
        <div className="mt-3 grid gap-2 sm:grid-cols-[minmax(0,1fr)_11rem]">
          <SearchInput
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search Favorites"
            aria-label="Search Favorites"
          />
          <Select
            value={sortMode}
            onChange={(event) => setSortMode(event.target.value as StarredDocumentSortMode)}
            aria-label="Sort Favorites"
          >
            <option value="starred_desc">Recently starred</option>
            <option value="updated_desc">Recently updated</option>
            <option value="title_asc">Title A-Z</option>
          </Select>
        </div>
      </header>

      <main className="flex-1 overflow-y-auto p-4">
        {feedback && (
          <div
            role={feedback.tone === "error" ? "alert" : "status"}
            className={`mb-3 break-words rounded-input border px-3 py-2 text-sm ${getStarredDocumentFeedbackClass(feedback.tone)}`}
          >
            {feedback.text}
          </div>
        )}
        {listState === "loading" ? (
          <p className="px-2 py-3 text-sm text-text-muted" role="status">
            Loading Favorites…
          </p>
        ) : listState === "empty" ? (
          <div className="flex h-full min-h-64 items-center justify-center text-center">
            <div>
              <Star size={28} className="mx-auto text-text-muted" />
              <p className="mt-2 text-sm text-text">
                {query.trim() ? "No Favorites match this search" : "No Favorites"}
              </p>
            </div>
          </div>
        ) : (
          <div className="space-y-4">
            {groups.map((group) => (
              <section key={group.key}>
                <h2 className={STARRED_DOCUMENT_GROUP_HEADING_CLASS}>
                  {group.label}
                </h2>
                <div className="overflow-hidden rounded-input border border-border">
                  {group.documents.map((document) => (
                    <div
                      key={`${document.spaceId}:${document.id}`}
                      className="flex items-center gap-3 border-b border-border px-3 py-2 last:border-b-0"
                    >
                      <button
                        type="button"
                        onClick={() => navigate(getStarredDocumentPath(document.spaceId, document.id))}
                        className="flex min-w-0 flex-1 items-center gap-3 text-left"
                      >
                        <DocumentIdentityIcon icon={document.icon} />
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-sm font-medium text-text">
                            {document.title}
                          </span>
                          <span className="block truncate text-xs text-text-muted">
                            {formatDateMetadata(
                              document.starred_at,
                              "Starred",
                              "Star date unavailable",
                            )}
                          </span>
                        </span>
                      </button>
                      <button
                        type="button"
                        onClick={() => void unstarDocument(document)}
                        className={`rounded-button p-1.5 text-accent hover:bg-primary/10 ${RESPONSIVE_TOUCH_ICON_ACTION_CLASS}`}
                        aria-label={`Remove star from ${document.title}`}
                        title="Remove star"
                        disabled={
                          unstarVisiblePending ||
                          loading ||
                          pendingUnstarKeys.has(`${document.spaceId}:${document.id}`)
                        }
                        aria-busy={pendingUnstarKeys.has(`${document.spaceId}:${document.id}`)}
                      >
                        <Star size={15} fill="currentColor" />
                      </button>
                    </div>
                  ))}
                </div>
              </section>
            ))}
          </div>
        )}
      </main>
    </div>
  );
}
