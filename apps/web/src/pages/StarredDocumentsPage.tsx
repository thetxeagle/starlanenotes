import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { FileText, RefreshCw, Star, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { SearchInput } from "@/components/ui/search-input";
import { Select } from "@/components/ui/select";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import {
  filterStarredDocuments,
  loadStarredDocuments,
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

export function StarredDocumentsPage() {
  const { spaceId } = useParams<{ spaceId: string }>();
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [sortMode, setSortMode] = useState<StarredDocumentSortMode>("starred_desc");
  const [starredDocuments, setStarredDocuments] = useState<StarredDocumentRecord[]>(() =>
    readStarredDocuments(),
  );
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState("");

  const refresh = async () => {
    setLoading(true);
    setMessage("");
    const next = await loadStarredDocuments(spaceId);
    setStarredDocuments(next);
    setLoading(false);
  };

  useEffect(() => {
    void refresh();
    const onChange = () => setStarredDocuments(readStarredDocuments());
    window.addEventListener("starlane:starred-documents-changed", onChange);
    return () => window.removeEventListener("starlane:starred-documents-changed", onChange);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [spaceId]);

  const visibleDocuments = useMemo(
    () => filterStarredDocuments(starredDocuments, query, sortMode),
    [query, sortMode, starredDocuments],
  );
  const groups = useMemo(() => groupStarredDocumentsBySpace(visibleDocuments), [visibleDocuments]);

  async function unstarDocument(document: StarredDocumentRecord) {
    try {
      await toggleStarredDocumentSynced(document);
      setStarredDocuments(readStarredDocuments());
      setMessage(`Removed star from "${document.title}".`);
    } catch {
      setMessage("Unable to sync starred document changes.");
    }
  }

  async function unstarVisible() {
    if (visibleDocuments.length === 0) return;
    let removed = 0;
    for (const document of visibleDocuments) {
      try {
        await toggleStarredDocumentSynced(document);
        removed += 1;
      } catch {
        // Keep processing the rest; the message below reports partial failures.
      }
    }
    setStarredDocuments(readStarredDocuments());
    setMessage(
      removed === visibleDocuments.length
        ? `Removed ${removed} visible ${removed === 1 ? "star" : "stars"}.`
        : `Removed ${removed} of ${visibleDocuments.length} visible stars.`,
    );
  }

  return (
    <div className="flex h-full flex-col">
      <header className="border-b border-border px-4 py-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-sm font-semibold text-text">
              <Star size={16} className="text-accent" fill="currentColor" />
              Starred documents
            </div>
            <p className="mt-1 text-xs text-text-muted">
              {visibleDocuments.length} of {starredDocuments.length} saved shortcuts
            </p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button type="button" variant="secondary" size="sm" onClick={() => void refresh()}>
              <RefreshCw size={14} />
              {loading ? "Refreshing..." : "Refresh"}
            </Button>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              disabled={visibleDocuments.length === 0}
              onClick={() => void unstarVisible()}
            >
              <Trash2 size={14} />
              Unstar visible
            </Button>
          </div>
        </div>
        <div className="mt-3 grid gap-2 sm:grid-cols-[minmax(0,1fr)_11rem]">
          <SearchInput
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search starred documents"
            aria-label="Search starred documents"
          />
          <Select
            value={sortMode}
            onChange={(event) => setSortMode(event.target.value as StarredDocumentSortMode)}
            aria-label="Sort starred documents"
          >
            <option value="starred_desc">Recently starred</option>
            <option value="updated_desc">Recently updated</option>
            <option value="title_asc">Title A-Z</option>
          </Select>
        </div>
      </header>

      <main className="flex-1 overflow-y-auto p-4">
        {message && (
          <div className="mb-3 rounded-input border border-border bg-surface px-3 py-2 text-sm text-text">
            {message}
          </div>
        )}
        {visibleDocuments.length === 0 ? (
          <div className="flex h-full min-h-64 items-center justify-center text-center">
            <div>
              <Star size={28} className="mx-auto text-text-muted" />
              <p className="mt-2 text-sm text-text">
                {query.trim() ? "No starred documents match this search" : "No starred documents"}
              </p>
            </div>
          </div>
        ) : (
          <div className="space-y-4">
            {groups.map((group) => (
              <section key={group.key}>
                <h2 className="mb-2 text-xs font-medium uppercase text-text-muted">
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
                        <FileText size={16} className="shrink-0 text-text-muted" />
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-sm font-medium text-text">
                            {document.icon ? `${document.icon} ` : ""}
                            {document.title}
                          </span>
                          <span className="block truncate text-xs text-text-muted">
                            Starred {formatStarredDate(document.starred_at)}
                          </span>
                        </span>
                      </button>
                      <button
                        type="button"
                        onClick={() => void unstarDocument(document)}
                        className="rounded-button p-1.5 text-accent hover:bg-primary/10"
                        aria-label={`Remove star from ${document.title}`}
                        title="Remove star"
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

function formatStarredDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "unknown date";
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}
