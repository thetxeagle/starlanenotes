import { useEffect, useMemo, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import {
  BookOpen,
  CalendarDays,
  Clock3,
  FileText,
  Folder,
  LayoutDashboard,
  ListChecks,
  Plus,
  Workflow,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { dataClient, type DocumentRecord, type FolderRecord } from "@/lib/dataClient";
import { emitGraphDirty } from "@/lib/graphEvents";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";

export type SpaceHomeSection = "journal" | "recent" | "graph" | "workflows";

export function getSpaceHomeDocumentPath(spaceId: string, documentId: string): string {
  return buildDocumentRoutePath(spaceId, documentId);
}

export function getSpaceHomeSectionPath(spaceId: string, section: SpaceHomeSection): string {
  return `/spaces/${encodeURIComponent(spaceId)}/${section}`;
}

export function sortSpaceHomeDocuments(documents: DocumentRecord[]): DocumentRecord[] {
  return [...documents].sort((a, b) => {
    const updatedComparison = (b.updated_at ?? "").localeCompare(a.updated_at ?? "");
    if (updatedComparison !== 0) return updatedComparison;
    return a.title.localeCompare(b.title, undefined, { sensitivity: "base" });
  });
}

export function rootSpaceHomeFolders(folders: FolderRecord[]): FolderRecord[] {
  return [...folders]
    .filter((folder) => !folder.parent_id)
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
}

export function SpaceHomePage() {
  const { spaceId } = useParams<{ spaceId: string }>();
  const navigate = useNavigate();
  const [spaceName, setSpaceName] = useState("Space");
  const [documents, setDocuments] = useState<DocumentRecord[]>([]);
  const [folders, setFolders] = useState<FolderRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const refresh = async () => {
    if (!spaceId) return;
    setLoading(true);
    setError("");
    try {
      const [space, nextDocuments, nextFolders] = await Promise.all([
        dataClient.getSpace(spaceId),
        dataClient.listDocuments(spaceId, { include_nested: true }),
        dataClient.listFolders(spaceId),
      ]);
      setSpaceName(space.name);
      setDocuments(sortSpaceHomeDocuments(nextDocuments));
      setFolders(nextFolders);
    } catch {
      setError("Unable to load this space.");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [spaceId]);

  const recentDocuments = useMemo(() => documents.slice(0, 6), [documents]);
  const rootFolders = useMemo(() => rootSpaceHomeFolders(folders).slice(0, 6), [folders]);

  const createDocument = async () => {
    if (!spaceId) return;
    const doc = await dataClient.createDocument(spaceId, { title: "Untitled" });
    emitGraphDirty(spaceId, "document-created");
    navigate(getSpaceHomeDocumentPath(spaceId, doc.id));
  };

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-text-muted">
        Loading space...
      </div>
    );
  }

  return (
    <div className="min-h-full bg-bg">
      <div className="mx-auto max-w-6xl px-4 py-3 sm:px-6 sm:py-5 lg:px-8">
        {error && (
          <div className="mb-4 rounded-input border border-danger/30 bg-danger/10 px-3 py-2 text-sm text-danger">
            {error}
          </div>
        )}

        <header className="mb-3 flex flex-wrap items-end justify-between gap-2 border-b border-border pb-3 sm:mb-5 sm:items-start sm:gap-3 sm:pb-4">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-xs text-text-muted">
              <LayoutDashboard size={14} />
              Space home
            </div>
            <h1 className="mt-0.5 truncate text-xl font-semibold text-text sm:mt-1 sm:text-2xl">
              {spaceName}
            </h1>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={() => {
                if (spaceId) navigate(getSpaceHomeSectionPath(spaceId, "journal"));
              }}
            >
              <CalendarDays size={14} />
              Journal
            </Button>
            <Button type="button" variant="primary" size="sm" onClick={() => void createDocument()}>
              <Plus size={14} />
              New document
            </Button>
          </div>
        </header>

        <section className="mb-4 grid gap-2 sm:gap-3 md:grid-cols-3">
          <SpaceHomeLink
            icon={Clock3}
            title="Recent"
            detail={`${documents.length} documents`}
            onClick={() => {
              if (spaceId) navigate(getSpaceHomeSectionPath(spaceId, "recent"));
            }}
          />
          <SpaceHomeLink
            icon={Workflow}
            title="Graph"
            detail="Map connections"
            onClick={() => {
              if (spaceId) navigate(getSpaceHomeSectionPath(spaceId, "graph"));
            }}
          />
          <SpaceHomeLink
            icon={ListChecks}
            title="Workflows"
            detail="Review queues"
            onClick={() => {
              if (spaceId) navigate(getSpaceHomeSectionPath(spaceId, "workflows"));
            }}
          />
        </section>

        <div className="grid gap-4 lg:grid-cols-[minmax(0,1fr)_20rem]">
          <section className="rounded-card border border-border bg-surface">
            <div className="flex items-center justify-between border-b border-border px-4 py-3">
              <div>
                <h2 className="text-base font-semibold text-text">Overview</h2>
                <p className="text-xs text-text-muted">A starter space home layout.</p>
              </div>
              <BookOpen size={16} className="text-text-muted" />
            </div>
            <div className="grid gap-3 p-4 md:grid-cols-2">
              <div className="rounded-input border border-border bg-bg p-3">
                <p className="text-sm font-medium text-text">Start here</p>
                <p className="mt-1 text-sm leading-6 text-text-muted">
                  Pin an introduction, team notes, or the current project brief here.
                </p>
              </div>
              <div className="rounded-input border border-border bg-bg p-3">
                <p className="text-sm font-medium text-text">Key links</p>
                <p className="mt-1 text-sm leading-6 text-text-muted">
                  Add shortcuts to important pages, folders, dashboards, or runbooks.
                </p>
              </div>
            </div>
          </section>

          <aside className="rounded-card border border-border bg-surface">
            <div className="border-b border-border px-4 py-3">
              <h2 className="text-base font-semibold text-text">Folders</h2>
              <p className="text-xs text-text-muted">{folders.length} total</p>
            </div>
            <div className="p-2">
              {rootFolders.length === 0 ? (
                <p className="px-2 py-2 text-sm text-text-muted">No folders yet.</p>
              ) : (
                rootFolders.map((folder) => (
                  <div
                    key={folder.id}
                    className="flex items-center gap-2 rounded-button px-2 py-1.5 text-sm text-text-muted"
                  >
                    <Folder size={14} className="shrink-0" />
                    <span className="truncate">{folder.name}</span>
                  </div>
                ))
              )}
            </div>
          </aside>
        </div>

        <section className="mt-4 rounded-card border border-border bg-surface">
          <div className="flex items-center justify-between border-b border-border px-4 py-3">
            <h2 className="text-base font-semibold text-text">Recently updated</h2>
            <button
              type="button"
              onClick={() => {
                if (spaceId) navigate(getSpaceHomeSectionPath(spaceId, "recent"));
              }}
              className="rounded-button px-2 py-1 text-sm text-primary hover:bg-surface-raised"
            >
              View all
            </button>
          </div>
          <div className="divide-y divide-border">
            {recentDocuments.length === 0 ? (
              <p className="px-4 py-4 text-sm text-text-muted">No documents yet.</p>
            ) : (
              recentDocuments.map((document) => (
                <button
                  key={document.id}
                  type="button"
                  onClick={() => {
                    if (spaceId) navigate(getSpaceHomeDocumentPath(spaceId, document.id));
                  }}
                  className="flex w-full items-center gap-3 px-4 py-3 text-left hover:bg-surface-raised"
                >
                  <FileText size={15} className="shrink-0 text-text-muted" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-medium text-text">
                      {document.icon ? `${document.icon} ` : ""}
                      {document.title}
                    </span>
                    <span className="block truncate text-xs text-text-muted">
                      Updated {formatSpaceHomeDate(document.updated_at)}
                    </span>
                  </span>
                </button>
              ))
            )}
          </div>
        </section>
      </div>
    </div>
  );
}

function SpaceHomeLink({
  icon: Icon,
  title,
  detail,
  onClick,
}: {
  icon: typeof Clock3;
  title: string;
  detail: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="rounded-card border border-border bg-surface p-3 text-left transition-colors duration-100 hover:bg-surface-raised sm:p-4"
    >
      <Icon size={17} className="text-primary" />
      <span className="mt-2 block text-sm font-semibold text-text sm:mt-3">{title}</span>
      <span className="mt-1 block text-xs text-text-muted">{detail}</span>
    </button>
  );
}

function formatSpaceHomeDate(value?: string): string {
  if (!value) return "unknown date";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "unknown date";
  return date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
}
