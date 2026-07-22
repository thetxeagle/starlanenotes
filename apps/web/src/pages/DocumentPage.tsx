import {
  forwardRef,
  lazy,
  Suspense,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useParams } from "react-router-dom";
import { BookOpen, Code2, MoreHorizontal, MoveRight, PanelRight, Pencil, Star } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { RightPanelControlContext } from "@/components/layout/AppLayout";
import { openSidebarDocumentMoveDialog } from "@/components/layout/Sidebar";
import { dataClient } from "@/lib/dataClient";
import type {
  DocumentRecord,
  FolderRecord,
  TemplateRecord,
  UserDirectoryRecord,
} from "@/lib/dataClient";
import { emitGraphDirty } from "@/lib/graphEvents";
import { useAutoSave, type SaveStatus } from "@/hooks/useAutoSave";
import { useFileDrop } from "@/hooks/useFileDrop";
import { useRuntimePlatform } from "@/hooks/useRuntimePlatform";
import { useAuth } from "@/hooks/useAuth";
import { getDocumentStats } from "@/lib/documentStats";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { rememberLastViewedDocumentRoute } from "@/lib/lastViewedDocument";
import { buildSpaceHomePath } from "@/lib/lastViewedSpace";
import {
  isDocumentStarred,
  loadStarredDocuments,
  toggleStarredDocumentSynced,
  updateStarredDocumentMetadata,
} from "@/lib/starredDocuments";
import { Textarea } from "@/components/ui/textarea";
import type { Editor as TipTapEditor } from "@tiptap/react";

type EditorViewMode = "markdown" | "preview" | "source";
const SYNOPSIS_UPDATED_EVENT = "starlane:synopsis-updated";
export const DOCUMENT_OVERFLOW_VIEW_MODES: readonly EditorViewMode[] = [
  "preview",
  "markdown",
  "source",
];

export function buildDocumentSpaceBreadcrumbHref(spaceId?: string): string | undefined {
  return spaceId ? buildSpaceHomePath(spaceId) : undefined;
}

const Editor = lazy(() =>
  import("@/components/editor/Editor").then((m) => ({ default: m.Editor })),
);
const MarkdownPreview = lazy(() =>
  import("@/components/editor/MarkdownPreview").then((m) => ({ default: m.MarkdownPreview })),
);
const Breadcrumb = lazy(() =>
  import("@/components/editor/Breadcrumb").then((m) => ({ default: m.Breadcrumb })),
);
const CoverImage = lazy(() =>
  import("@/components/editor/CoverImage").then((m) => ({ default: m.CoverImage })),
);
const OutlineRail = lazy(() =>
  import("@/components/editor/OutlineRail").then((m) => ({ default: m.OutlineRail })),
);
const PageIcon = lazy(() =>
  import("@/components/editor/PageIcon").then((m) => ({ default: m.PageIcon })),
);

export function getDocumentSaveStatusClass(status: SaveStatus): string {
  if (status === "error") return "text-danger";
  if (status === "saving") return "text-accent";
  return "text-text-muted";
}

interface DocumentData {
  id: string;
  title: string;
  icon?: string;
  folder_id?: string;
  cover_image?: string;
  locked: boolean;
  locked_by?: string;
  space_id: string;
  created_by?: string;
  last_updated_by?: string;
  created_at?: string;
  updated_at?: string;
}

interface DocumentContent {
  id: string;
  title: string;
  content: string;
}

interface SpaceInfo {
  name: string;
  slug: string;
}

interface ParsedDocumentContent {
  body: string;
  synopsis: string;
}

function parseSimpleFrontmatter(raw: string): Record<string, string> {
  const values: Record<string, string> = {};
  raw.split(/\r?\n/).forEach((line) => {
    const match = line.match(/^([A-Za-z0-9_-]+):\s*(.*)$/);
    if (!match) return;
    let value = match[2].trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    values[match[1].toLowerCase()] = value.replace(/\\"/g, '"');
  });
  return values;
}

export function stripLeadingMatchingH1(content: string, title?: string): string {
  if (!title) return content.trim();
  const h1Match = content.match(/^#\s+(.+)\n?/);
  const normalizeTitle = (value: string) => value.trim().replace(/\s+/g, " ");
  if (h1Match && normalizeTitle(h1Match[1]) === normalizeTitle(title)) {
    return content.slice(h1Match[0].length).trim();
  }
  return content.trim();
}

function parseDocumentContent(content: string, title?: string): ParsedDocumentContent {
  const fmMatch = content.match(/^---\n([\s\S]*?)\n---\n?/);
  if (!fmMatch) {
    return { body: stripLeadingMatchingH1(content, title), synopsis: "" };
  }
  const metadata = parseSimpleFrontmatter(fmMatch[1]);
  const body = content.slice(fmMatch[0].length);
  return {
    body: stripLeadingMatchingH1(body, title),
    synopsis: metadata.synopsis || metadata.excerpt || metadata.description || "",
  };
}

function serializeDocumentContent(content: string, synopsis: string): string {
  const trimmedSynopsis = synopsis.trim();
  if (!trimmedSynopsis) return content;
  const escaped = trimmedSynopsis.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
  return `---\nsynopsis: "${escaped}"\n---\n\n${content.trimStart()}`;
}

function formatDocumentDate(value?: string): string {
  if (!value) return "unknown date";
  const parsed = new Date(value);
  if (Number.isNaN(parsed.getTime())) return "unknown date";
  return parsed.toISOString().slice(0, 10);
}

export function buildDocumentFolderPath(
  folders: Pick<FolderRecord, "id" | "name" | "parent_id">[],
  documents: Pick<DocumentRecord, "id" | "folder_id">[],
  docId: string,
  directFolderId?: string,
): string[] {
  const folderById = new Map(folders.map((folder) => [folder.id, folder]));
  const docFolderId =
    directFolderId ?? documents.find((document) => document.id === docId)?.folder_id;
  const path: string[] = [];
  const seen = new Set<string>();
  let cursor = docFolderId;

  while (cursor && !seen.has(cursor)) {
    seen.add(cursor);
    const folder = folderById.get(cursor);
    if (!folder) break;
    path.unshift(folder.name);
    cursor = folder.parent_id;
  }

  return path;
}

export function getDocumentActionLabels(starred: boolean, propertiesOpen: boolean): string[] {
  return [
    starred ? "Remove favorite" : "Favorite",
    "Move page",
    "Page view",
    "Edit",
    "Source view",
    propertiesOpen ? "Hide properties" : "Properties",
  ];
}

export function getDocumentPrimaryChromeActions(
  starred: boolean,
): readonly ("favorite" | "view-modes" | "properties")[] {
  return starred ? ["favorite", "view-modes", "properties"] : ["view-modes", "properties"];
}

export function getVisibleDocumentViewModes(mode: EditorViewMode): readonly EditorViewMode[] {
  return DOCUMENT_OVERFLOW_VIEW_MODES.filter(
    (candidate) => candidate !== "source" || mode === "markdown" || mode === "source",
  );
}

export function shouldCloseDocumentActionsForKey(key: string): boolean {
  return key === "Escape";
}

function getDocumentViewModeAction(
  mode: EditorViewMode,
  labels: string[],
): { label: string; icon: LucideIcon } {
  if (mode === "markdown") return { label: labels[3], icon: Pencil };
  if (mode === "source") return { label: labels[4], icon: Code2 };
  return { label: labels[2], icon: BookOpen };
}

export function DocumentSourceTextarea({
  content,
  locked,
  onChange,
}: {
  content: string;
  locked: boolean;
  onChange: (content: string) => void;
}) {
  return (
    <Textarea
      id="document-source"
      value={content}
      onChange={(event) => onChange(event.target.value)}
      readOnly={locked}
      spellCheck={false}
      className="min-h-[56vh] resize-y bg-transparent p-4 font-mono text-sm leading-6 disabled:cursor-not-allowed disabled:opacity-70"
      placeholder="Write Markdown source..."
    />
  );
}

function normalizeDocumentTitleInput(title: string): string {
  return title.replace(/\s*\n+\s*/g, " ");
}

export const DocumentTitleTextarea = forwardRef<
  HTMLTextAreaElement,
  {
    title: string;
    locked: boolean;
    onTitleChange: (title: string) => void;
    onCommit: (title: string) => void;
  }
>(({ title, locked, onTitleChange, onCommit }, ref) => (
  <Textarea
    ref={ref}
    id="document-title"
    rows={1}
    wrap="soft"
    value={title}
    onChange={(event) => onTitleChange(normalizeDocumentTitleInput(event.target.value))}
    onBlur={(event) => onCommit(normalizeDocumentTitleInput(event.currentTarget.value))}
    onKeyDown={(event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        event.currentTarget.blur();
      }
    }}
    placeholder="Untitled"
    disabled={locked}
    className="document-title-input text-text placeholder:text-text-muted/40 h-auto max-h-80 !min-h-0 w-full resize-none overflow-hidden whitespace-pre-wrap break-words rounded-none border-transparent bg-transparent px-0 py-0 font-bold leading-[1.15] outline-none [overflow-wrap:anywhere] focus:ring-0 focus:ring-offset-0 disabled:opacity-70"
  />
));

DocumentTitleTextarea.displayName = "DocumentTitleTextarea";

export function DocumentPage() {
  const { spaceId, docId } = useParams<{ spaceId: string; docId: string }>();
  const { config, user } = useAuth();
  const rightPanel = useContext(RightPanelControlContext);
  const [doc, setDoc] = useState<DocumentData | null>(null);
  const [content, setContent] = useState("");
  const [synopsis, setSynopsis] = useState("");
  const [title, setTitle] = useState("");
  const [spaceName, setSpaceName] = useState("");
  const [folderPath, setFolderPath] = useState<string[]>([]);
  const [templates, setTemplates] = useState<TemplateRecord[]>([]);
  const [userDirectory, setUserDirectory] = useState<UserDirectoryRecord[]>([]);
  const [viewMode, setViewMode] = useState<EditorViewMode>("preview");
  const [starred, setStarred] = useState(false);
  const [documentActionsOpen, setDocumentActionsOpen] = useState(false);
  const [loading, setLoading] = useState(true);
  const [editorRef, setEditorRef] = useState<TipTapEditor | null>(null);
  const titleRef = useRef<HTMLTextAreaElement | null>(null);
  const assetContext = useMemo(() => ({ space_id: spaceId, document_id: docId }), [spaceId, docId]);
  const { isDragging, dropRef } = useFileDrop(editorRef, assetContext);
  const { isMobileRuntime, isMobileViewport } = useRuntimePlatform();
  const mobileLayout = isMobileRuntime || isMobileViewport;

  const saveFn = useCallback(
    async (newContent: string) => {
      if (!spaceId || !docId) return;
      await dataClient.updateDocumentContent(
        spaceId,
        docId,
        serializeDocumentContent(newContent, synopsis),
      );
      emitGraphDirty(spaceId, "document-content-updated");
    },
    [spaceId, docId, synopsis],
  );

  const { status, scheduleAutoSave } = useAutoSave(saveFn);

  // Load document
  useEffect(() => {
    if (!spaceId || !docId) return;
    setStarred(isDocumentStarred({ spaceId, id: docId }));
    void loadStarredDocuments(spaceId).then(() =>
      setStarred(isDocumentStarred({ spaceId, id: docId })),
    );

    setLoading(true);

    Promise.all([
      dataClient.getDocument(spaceId, docId) as Promise<DocumentData>,
      dataClient.getDocumentContent(spaceId, docId) as Promise<DocumentContent>,
      dataClient.getSpace(spaceId) as Promise<SpaceInfo>,
      dataClient.listTemplates(spaceId).catch(() => []),
      dataClient.listFolders(spaceId).catch(() => []),
      dataClient.listDocuments(spaceId, { include_nested: true }).catch(() => []),
    ])
      .then(([docData, contentData, spaceData, templateData, folderData, documentData]) => {
        const parsed = parseDocumentContent(contentData.content, docData.title);
        setDoc(docData);
        setContent(parsed.body);
        setSynopsis(parsed.synopsis);
        setTitle(docData.title);
        setSpaceName(spaceData.name);
        setFolderPath(buildDocumentFolderPath(folderData, documentData, docId, docData.folder_id));
        rememberLastViewedDocumentRoute(spaceId, docId);
        updateStarredDocumentMetadata({
          spaceId,
          id: docId,
          title: docData.title,
          spaceName: spaceData.name,
          icon: docData.icon,
          updated_at: docData.updated_at,
        });
        setTemplates(templateData);
        setViewMode(parsed.body.trim() ? "preview" : "markdown");
      })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [spaceId, docId]);

  useEffect(() => {
    if (!title) return;
    const siteName = config?.site_name ?? "Starlane Notes";
    const parts = [title, spaceName, siteName].filter(Boolean);
    document.title = parts.join(" - ");
  }, [config?.site_name, spaceName, title]);

  useEffect(() => {
    dataClient
      .listUserDirectory()
      .then(setUserDirectory)
      .catch(() => setUserDirectory([]));
  }, []);

  useEffect(() => {
    const titleField = titleRef.current;
    if (!titleField) return;
    titleField.style.height = "auto";
    titleField.style.height = `${titleField.scrollHeight}px`;
  }, [title]);

  useEffect(() => {
    const handleSynopsisUpdated = (event: Event) => {
      const detail = (event as CustomEvent<{ spaceId: string; docId: string; synopsis: string }>)
        .detail;
      if (detail?.spaceId === spaceId && detail.docId === docId) {
        setSynopsis(detail.synopsis);
      }
    };

    window.addEventListener(SYNOPSIS_UPDATED_EVENT, handleSynopsisUpdated);
    return () => window.removeEventListener(SYNOPSIS_UPDATED_EVENT, handleSynopsisUpdated);
  }, [spaceId, docId]);

  useEffect(() => {
    if (!documentActionsOpen) return;
    const handleDocumentActionKeydown = (event: KeyboardEvent) => {
      if (!shouldCloseDocumentActionsForKey(event.key)) return;
      setDocumentActionsOpen(false);
    };
    document.addEventListener("keydown", handleDocumentActionKeydown);
    return () => document.removeEventListener("keydown", handleDocumentActionKeydown);
  }, [documentActionsOpen]);

  const handleContentChange = (newContent: string) => {
    setContent(newContent);
    scheduleAutoSave(newContent);
  };

  const handleTitleChange = async (newTitle: string) => {
    const normalizedTitle = newTitle.replace(/\s*\n+\s*/g, " ").trim() || "Untitled";
    setTitle(normalizedTitle);
    if (spaceId && docId) {
      await dataClient.updateDocument(spaceId, docId, { title: normalizedTitle });
      updateStarredDocumentMetadata({
        spaceId,
        id: docId,
        title: normalizedTitle,
        spaceName,
        icon: doc?.icon,
        updated_at: doc?.updated_at,
      });
      emitGraphDirty(spaceId, "document-metadata-updated");
    }
  };

  const setEditorViewMode = (next: EditorViewMode) => {
    setViewMode(next);
    if (next !== "markdown") setEditorRef(null);
  };

  const toggleStarred = () => {
    if (!spaceId || !docId) return;
    const input = {
      spaceId,
      id: docId,
      title,
      spaceName,
      icon: doc?.icon,
      updated_at: doc?.updated_at,
    };
    setStarred((current) => !current);
    void toggleStarredDocumentSynced(input)
      .then(setStarred)
      .catch(() => setStarred(isDocumentStarred(input)));
  };

  const openMoveDialog = () => {
    if (!docId) return;
    openSidebarDocumentMoveDialog(docId);
    setDocumentActionsOpen(false);
  };
  const isMarkdownMode = viewMode === "markdown";
  const isSourceMode = viewMode === "source";
  const localMode = getCurrentLaunchMode() === "local";
  const stats = getDocumentStats(content);
  const displayNameFor = (identity?: string) => {
    if (!identity) return user?.display_name || "Unknown";
    if (user && [user.id, user.email, user.display_name].includes(identity))
      return user.display_name;
    const directoryRecord = userDirectory.find(
      (record) =>
        record.id === identity || record.email === identity || record.display_name === identity,
    );
    if (directoryRecord) return directoryRecord.display_name;
    if (/^[0-9a-f]{8}-[0-9a-f-]{27,}$/i.test(identity) && user?.display_name) {
      return user.display_name;
    }
    return identity;
  };
  const createdBy = displayNameFor(doc?.created_by);
  const updatedBy = displayNameFor(doc?.last_updated_by || doc?.created_by);
  const documentDate = formatDocumentDate(doc?.updated_at || doc?.created_at);
  const metadataLine = localMode
    ? `Last modified ${documentDate} • ${stats.words} WORDS • ${stats.readingMinutes} MIN READ`
      : `Created by ${createdBy} • last modified by ${updatedBy} on ${documentDate} • ${stats.words} WORDS • ${stats.readingMinutes} MIN READ`;
  const documentActionLabels = getDocumentActionLabels(starred, rightPanel.open);
  const primaryChromeActions = getDocumentPrimaryChromeActions(starred);
  const visibleViewModes = getVisibleDocumentViewModes(viewMode);

  if (loading) {
    return (
      <div className="flex h-full items-center justify-center" role="status" aria-live="polite">
        <span className="text-text-muted text-sm">Loading...</span>
      </div>
    );
  }

  if (!doc) {
    return (
      <div className="flex h-full items-center justify-center" role="alert">
        <span className="text-text-muted text-sm">Document not found</span>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {/* Editor content area */}
      <div ref={dropRef} className="relative flex-1 overflow-y-auto">
        {/* Drop overlay */}
        {isDragging && (
          <div className="bg-bg/80 absolute inset-0 z-50 flex items-center justify-center">
            <p className="text-text-muted text-lg font-medium">Drop file to attach</p>
          </div>
        )}

        <div
          className={`mx-auto w-full max-w-4xl px-4 py-2 sm:px-6 sm:py-6 xl:max-w-5xl xl:py-6 ${
            mobileLayout ? "pb-40" : ""
          }`}
        >
          <div
            className={`mb-3 flex items-center justify-between gap-2 ${
              mobileLayout ? "min-h-8" : "items-start gap-3"
            }`}
          >
            <Suspense fallback={null}>
              <Breadcrumb
                spaceName={spaceName}
                folderPath={folderPath}
                title={title}
                spaceHref={buildDocumentSpaceBreadcrumbHref(spaceId)}
                className={mobileLayout ? "mb-0" : "mb-4"}
              />
            </Suspense>
            <div className="flex shrink-0 items-center gap-1">
              <div className="hidden items-center gap-1 md:flex">
                {primaryChromeActions.includes("favorite") && (
                  <button
                    type="button"
                    onClick={toggleStarred}
                    className={`rounded-button flex h-8 w-8 items-center justify-center text-xs transition-colors duration-100 ${
                      starred
                        ? "bg-primary/10 text-primary"
                        : "text-text-muted hover:bg-surface hover:text-text"
                    }`}
                    aria-label={documentActionLabels[0]}
                    aria-pressed={starred}
                    title={documentActionLabels[0]}
                  >
                    <Star size={14} fill={starred ? "currentColor" : "none"} />
                  </button>
                )}
                {visibleViewModes.map((mode) => {
                  const { label, icon: Icon } = getDocumentViewModeAction(
                    mode,
                    documentActionLabels,
                  );
                  return (
                    <button
                      key={mode}
                      type="button"
                      onClick={() => setEditorViewMode(mode)}
                      className={`rounded-button flex h-8 w-8 items-center justify-center text-xs transition-colors duration-100 ${
                        viewMode === mode
                          ? "bg-surface-raised text-text"
                          : "text-text-muted hover:bg-surface hover:text-text"
                      }`}
                      aria-label={label}
                      aria-pressed={viewMode === mode}
                      title={label}
                    >
                      <Icon size={14} />
                    </button>
                  );
                })}
                <button
                  type="button"
                  onClick={rightPanel.toggle}
                  className={`rounded-button flex h-8 w-8 items-center justify-center text-xs transition-colors duration-100 ${
                    rightPanel.open
                      ? "bg-surface-raised text-text"
                      : "text-text-muted hover:bg-surface hover:text-text"
                  }`}
                  aria-label={documentActionLabels[5]}
                  aria-pressed={rightPanel.open}
                  title={documentActionLabels[5]}
                >
                  <PanelRight size={14} />
                </button>
              </div>
              <div className={`relative ${mobileLayout ? "" : "md:hidden"}`}>
                <button
                  type="button"
                  onClick={() => setDocumentActionsOpen((open) => !open)}
                  className="rounded-button text-text-muted hover:bg-surface hover:text-text flex h-8 w-8 items-center justify-center transition-colors duration-100"
                  aria-label="Document actions"
                  aria-haspopup="menu"
                  aria-expanded={documentActionsOpen}
                >
                  <MoreHorizontal size={16} />
                </button>
                {documentActionsOpen && (
                  <>
                    <button
                      type="button"
                      aria-label="Close document actions"
                      className="fixed inset-0 z-40 cursor-default"
                      onClick={() => setDocumentActionsOpen(false)}
                    />
                    <div
                      role="menu"
                      aria-label="Document actions"
                      className="starlane-menu-surface absolute right-0 top-9 z-50 w-48"
                    >
                      <button
                        type="button"
                        role="menuitem"
                        onClick={openMoveDialog}
                        className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm transition-colors duration-100"
                      >
                        <MoveRight size={14} />
                        {documentActionLabels[1]}
                      </button>
                      <button
                        type="button"
                        role="menuitemcheckbox"
                        aria-checked={starred}
                        onClick={() => {
                          setDocumentActionsOpen(false);
                          toggleStarred();
                        }}
                        className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm transition-colors duration-100"
                      >
                        <Star size={14} fill={starred ? "currentColor" : "none"} />
                        {documentActionLabels[0]}
                      </button>
                      {visibleViewModes.map((mode) => {
                        const { label, icon: Icon } = getDocumentViewModeAction(
                          mode,
                          documentActionLabels,
                        );
                        return (
                          <button
                            key={mode}
                            type="button"
                            role="menuitemradio"
                            aria-checked={viewMode === mode}
                            onClick={() => {
                              setDocumentActionsOpen(false);
                              setEditorViewMode(mode);
                            }}
                            className={`rounded-button flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm transition-colors duration-100 ${
                              viewMode === mode
                                ? "bg-surface-raised text-text"
                                : "text-text-muted hover:bg-surface-raised hover:text-text"
                            }`}
                          >
                            <Icon size={14} />
                            {label}
                          </button>
                        );
                      })}
                      <button
                        type="button"
                        role="menuitemcheckbox"
                        aria-checked={rightPanel.open}
                        onClick={() => {
                          setDocumentActionsOpen(false);
                          rightPanel.toggle();
                        }}
                        className={`rounded-button flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm transition-colors duration-100 ${
                          rightPanel.open
                            ? "bg-surface-raised text-text"
                            : "text-text-muted hover:bg-surface-raised hover:text-text"
                        }`}
                      >
                        <PanelRight size={14} />
                        {documentActionLabels[5]}
                      </button>
                    </div>
                  </>
                )}
              </div>
            </div>
          </div>

          <Suspense fallback={null}>
            <CoverImage
              src={doc.cover_image}
              showEmptyPlaceholder={!mobileLayout}
              onUpdate={(url) => {
                if (spaceId && docId) {
                  dataClient.updateDocument(spaceId, docId, { cover_image: url });
                  emitGraphDirty(spaceId, "document-metadata-updated");
                }
              }}
            />
          </Suspense>

          <Suspense fallback={null}>
            <PageIcon icon={doc.icon} />
          </Suspense>

          {/* Title */}
          <div className="mb-4">
            {isMarkdownMode || isSourceMode ? (
              <>
                <label htmlFor="document-title" className="sr-only">
                  Document title
                </label>
                <DocumentTitleTextarea
                  ref={titleRef}
                  title={title}
                  locked={doc.locked}
                  onTitleChange={setTitle}
                  onCommit={handleTitleChange}
                />
              </>
            ) : (
              <h1 className="document-title text-text break-words font-bold [overflow-wrap:anywhere]">
                {title || "Untitled"}
              </h1>
            )}
            {!isMarkdownMode && !isSourceMode ? (
              <div className="text-text-muted mt-2 flex flex-wrap items-center gap-1.5 text-[10px] uppercase tracking-wide">
                <span>{metadataLine}</span>
              </div>
            ) : status !== "idle" ? (
              <div className="text-text-muted mt-2 flex flex-wrap items-center gap-1.5 text-[10px] uppercase tracking-wide">
                <span
                  className={getDocumentSaveStatusClass(status)}
                  aria-live="polite"
                  aria-atomic="true"
                >
                  {status === "saving" ? "Saving..." : status === "saved" ? "Saved" : "Save failed"}
                </span>
              </div>
            ) : null}
            {!isMarkdownMode && synopsis.trim() ? (
              <p className="text-text-muted mt-3 text-sm leading-6">{synopsis}</p>
            ) : null}
          </div>

          {/* Lock banner */}
          {doc.locked && (
            <div className="rounded-input border-accent/20 bg-accent/5 text-accent mb-4 border px-3 py-2 text-sm">
              This document is locked and cannot be edited.
            </div>
          )}

          <div>
            <Suspense
              fallback={
                <div className="rounded-card border-border bg-surface text-text-muted border px-3 py-4 text-sm">
                  Loading {isMarkdownMode ? "editor" : isSourceMode ? "source" : "page view"}...
                </div>
              }
            >
              {isMarkdownMode ? (
                <Editor
                  content={content}
                  onChange={handleContentChange}
                  readOnly={doc.locked}
                  showBlockChrome={!mobileLayout}
                  showMobileCommandButton={mobileLayout}
                  templates={templates}
                  templateContext={{ title, space: spaceName, author: user?.display_name }}
                  assetContext={assetContext}
                  spaceId={spaceId}
                  onEditorReady={setEditorRef}
                />
              ) : isSourceMode ? (
                <div className="border-border bg-surface rounded-lg border">
                  <label htmlFor="document-source" className="sr-only">
                    Markdown source
                  </label>
                  <DocumentSourceTextarea
                    content={content}
                    locked={doc.locked}
                    onChange={handleContentChange}
                  />
                </div>
              ) : (
                <MarkdownPreview content={content} assetContext={assetContext} />
              )}
            </Suspense>
          </div>
        </div>

        {/* Outline rail */}
        {viewMode === "preview" && (
          <Suspense fallback={null}>
            <OutlineRail content={content} />
          </Suspense>
        )}
      </div>
    </div>
  );
}
