import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Clock3, FileText, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { SearchInput } from "@/components/ui/search-input";
import { dataClient, type DocumentRecord } from "@/lib/dataClient";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";

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

export function RecentDocumentsPage() {
  const { spaceId } = useParams<{ spaceId: string }>();
  const navigate = useNavigate();
  const [spaceName, setSpaceName] = useState("");
  const [documents, setDocuments] = useState<DocumentRecord[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const refresh = async () => {
    if (!spaceId) return;
    setLoading(true);
    setError("");
    try {
      const [space, nextDocuments] = await Promise.all([
        dataClient.getSpace(spaceId),
        dataClient.listDocuments(spaceId, { include_nested: true }),
      ]);
      setSpaceName(space.name);
      setDocuments(sortRecentDocuments(nextDocuments));
    } catch {
      setError("Unable to load recent documents.");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [spaceId]);

  const visibleDocuments = useMemo(
    () => filterRecentDocuments(documents, query),
    [documents, query],
  );

  return (
    <div className="flex h-full flex-col">
      <header className="border-border border-b px-4 py-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <div className="text-text flex items-center gap-2 text-sm font-semibold">
              <Clock3 size={16} className="text-text-muted" />
              Recent documents
            </div>
            <p className="text-text-muted mt-1 text-xs">
              {visibleDocuments.length} of {documents.length} in {spaceName || "this space"}
            </p>
          </div>
          <Button type="button" variant="secondary" size="sm" onClick={() => void refresh()}>
            <RefreshCw size={14} />
            {loading ? "Refreshing..." : "Refresh"}
          </Button>
        </div>
        <SearchInput
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search recent documents"
          aria-label="Search recent documents"
          containerClassName="mt-3"
        />
      </header>

      <main className="flex-1 overflow-y-auto p-4">
        {error && (
          <div className="mb-3 rounded-input border border-danger/30 bg-danger/10 px-3 py-2 text-sm text-danger">
            {error}
          </div>
        )}
        {visibleDocuments.length === 0 ? (
          <div className="flex h-full min-h-64 items-center justify-center text-center">
            <div>
              <Clock3 size={28} className="text-text-muted mx-auto" />
              <p className="text-text mt-2 text-sm">
                {query.trim() ? "No recent documents match this search" : "No recent documents"}
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
                <FileText size={16} className="text-text-muted shrink-0" />
                <span className="min-w-0 flex-1">
                  <span className="text-text block truncate text-sm font-medium">
                    {document.icon ? `${document.icon} ` : ""}
                    {document.title}
                  </span>
                  <span className="text-text-muted block truncate text-xs">
                    Updated {formatRecentDate(document.updated_at)}
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

function formatRecentDate(value?: string): string {
  if (!value) return "unknown date";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "unknown date";
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}
