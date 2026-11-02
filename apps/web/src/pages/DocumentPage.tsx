import { markOriginalNativeLocalCoverAdopted } from "@/lib/nativeLocalCover";
import { directUploadAcknowledgementId, markDirectUploadAdopted } from "@/lib/directUploadReview";
import { DocumentQueuedDraftCopyReview } from "@/components/DocumentQueuedDraftCopyReview";
import { prepareQueuedDraftCopy, persistQueuedDraftCopyReview, readQueuedDraftCopyReview, listQueuedDraftCopyReviews, type QueuedCopyReviewRecord } from "@/lib/queuedDraftCopy";
import { getPageEditCapability, pageEditBlockedExplanation, pageEditObservationFingerprint, type PageEditCapability } from "@/lib/pageEditCapability";
import { publicationHistoryContextVersion, subscribePublicationHistoryContext } from "@/lib/documentPublishedHistory";
import { exportOriginalSavedDraft } from "@/lib/documentDraftPublicationExport";
import { DocumentDraftPublicationReview } from "@/components/DocumentDraftPublicationReview";
import type { DraftPublicationIntent, DraftPublicationRecord, DraftPublicationReceipt } from "@/lib/documentDraftPublication";
import { DocumentDraftRecoveryReview } from "@/components/DocumentDraftRecoveryReview";
import { api, getApiBase, isApiError } from "@/lib/api";
import { DraftPresence } from "@/components/documents/DraftPresence";
import { createDraftPeerTransport, type DraftPresenceRoster, type DraftPeerFrame } from "@/lib/draftPeerTransport";
import {
  createDraftDiscussionAnchor,
  draftDiscussionScope,
  type DraftDiscussionAnchor,
  type DraftDiscussionTarget,
} from "@/lib/draftDiscussion";
import type { ApplyExternalEditorContent, EditorDraftPeerCursor, EditorSourceSelection } from "@/components/editor/Editor";
import { DocumentHeldDraftSaveReview } from "@/components/DocumentHeldDraftSaveReview";
import { heldDraftQueueService, type HeldDraftQueueItem, type HeldDraftQueueReview } from "@/lib/heldDraftQueue";
import { listOfflineServerMutations, OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT } from "@/lib/offlineServerCache";
import { createDocumentDraftRecoveryActions, type DraftRecoveryIntent, type DraftRecoveryCopyRecord, type DraftRecoveryReplacementRecord } from "@/lib/documentDraftRecoveryActions";
import { loadPageViewerContext } from "@/lib/pageViewerLoad";
import { DraftBirthHeldError, selectedDraftFork } from "@/lib/draftBirth";
import { PageShareDialog } from "@/components/PageShareDialog";
import { stripTemplateGuidance } from "@/lib/templatePublication";
import { PageResponsibilitySummary } from "@/components/PageResponsibilitySummary";
import {
  forwardRef,
  lazy,
  Suspense,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useSyncExternalStore,
  useMemo,
  useRef,
  useState,
} from "react";
import { useBlocker, useLocation, useNavigate, useParams } from "react-router-dom";
import { flushSync } from "react-dom";
import {
  BookOpen,
  List,
  Archive,
  Code2,
  Copy,
  Highlighter,
  LockKeyhole,
  Files,
  MessageSquare,
  MoreHorizontal,
  MoveRight,
  Pencil,
  Send,
  Share2,
  SlidersHorizontal,
  Star,
  Trash2,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { RightPanelControlContext } from "@/components/layout/AppLayout";
import { DocumentPageDetailsRegistrationContext } from "@/components/layout/documentPageDetails";
import { openSidebarDocumentMoveDialog } from "@/components/layout/Sidebar";
import { dataClient, DraftSaveHeldError, isDraftSaveHeldError, captureDraftSaveSelection, verifyDraftSaveSelection, type DraftSaveSelection, type DocumentDraftEditSource } from "@/lib/dataClient";
import { SaveAsTemplateDialog } from "@/components/SaveAsTemplateDialog";
import { getTemplateEditorPath } from "@/lib/templates";
import { selectDocumentDraft, shouldChooseAccessibleDraft } from "@/lib/documentDraftSelection";
import { mergeDraftPublication } from "@/lib/documentDraftPublication";
import {
  documentDraftRecoveryMatchesSaved,
  compareDocumentDraftRecoverySource,
  persistDocumentDraftRecovery,
  readDocumentDraftRecovery,
  clearDocumentDraftRecovery,
  discardDocumentDraftRecovery,
  type DocumentDraftRecoveryScope,
  type DocumentDraftRecoverySnapshot,
} from "@/lib/documentDraftRecovery";
import type {
  DocumentRecord,
  DraftRecord,
  DraftSummaryRecord,
  FolderRecord,
  TemplateRecord,
  UserDirectoryRecord,
} from "@/lib/dataClient";
import { DialogShell } from "@/components/ui/dialog-shell";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Button } from "@/components/ui/button";
import { LoadingState } from "@/components/ui/loading-state";
import { emitGraphDirty } from "@/lib/graphEvents";
import { buildDocumentParentPath } from "@/lib/documentHierarchy";
import {
  addDocumentLocationChangedListener,
  type DocumentLocationChangedDetail,
} from "@/lib/documentLocationEvents";
import { useAutoSave, type SaveStatus } from "@/hooks/useAutoSave";
import { useOnlineStatus } from "@/hooks/useOnlineStatus";
import { useFileDrop } from "@/hooks/useFileDrop";
import { useRuntimePlatform } from "@/hooks/useRuntimePlatform";
import { useAuth } from "@/hooks/useAuth";
import { getDocumentStats } from "@/lib/documentStats";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { replaceCustomCss } from "@/lib/documentContent";
import { inspectPageCssSource, rebuildPageCssSource, type PageCssProvenance } from "@/lib/pageCssProvenance";
import { buildDocumentRoutePath, rememberLastViewedDocumentRoute } from "@/lib/lastViewedDocument";
import { buildSpaceHomePath } from "@/lib/lastViewedSpace";
import {
  isDocumentStarred,
  loadStarredDocuments,
  toggleStarredDocumentSynced,
  updateStarredDocumentMetadata,
} from "@/lib/starredDocuments";
import { Textarea } from "@/components/ui/textarea";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useMenuKeyboardNavigation } from "@/components/ui/useMenuKeyboardNavigation";
import {
  CONTROL_CHROME_ACTION_CLASS,
  CONTROL_CHROME_ACTIVE_ACTION_CLASS,
  CONTROL_CHROME_RAIL_CLASS,
} from "@/components/ui/control-chrome";
import { ContentIdentityIcon } from "@/components/ContentIdentityIcon";
import { emitTransientSystemNotification } from "@/lib/systemNotifications";
import {
  getDocumentPageWidthClass,
  normalizeDocumentPageWidth,
  type DocumentPageWidth,
} from "@/components/editor/DocumentWidthPicker";
import { DocumentContents } from "@/components/editor/DocumentContents";
import type { Editor as TipTapEditor } from "@tiptap/react";

type EditorViewMode = "markdown" | "preview" | "source";
const SYNOPSIS_UPDATED_EVENT = "starlane:synopsis-updated";
export const DOCUMENT_OVERFLOW_VIEW_MODES: readonly EditorViewMode[] = [
  "preview",
  "markdown",
  "source",
];
export const MOBILE_DOCUMENT_CONTROL_RAIL_CLASS =
  "max-w-full overflow-x-auto pb-1 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden";

export type DocumentChromeActionTone = "default" | "primary" | "danger";

export function getDocumentChromeActionTone(
  tone: DocumentChromeActionTone = "default",
  active = false,
): string {
  if (tone === "danger") return "text-danger";
  if (tone === "primary" || active) return "text-primary";
  return "text-text-muted";
}

export function isDocumentMetadataEditable(viewMode: EditorViewMode): boolean {
  return viewMode === "markdown" || viewMode === "source";
}

export function assertDocumentPageEditable(capability:PageEditCapability):void {
  if (!capability.canEditPage) throw new DraftSaveHeldError("source_conflict",pageEditBlockedExplanation(capability.blockedReason) ?? "Review current page Edit access before saving retained edits.");
}

export function canEditDocumentMetadata(
  viewMode: EditorViewMode,
  draftEditable: boolean,
  locked: boolean,
): boolean {
  return isDocumentMetadataEditable(viewMode) && draftEditable && !locked;
}

export function buildDocumentSpaceBreadcrumbHref(spaceId?: string): string | undefined {
  return spaceId ? buildSpaceHomePath(spaceId) : undefined;
}

const EditorCore = lazy(() =>
  import("@/components/editor/Editor").then((m) => ({ default: m.Editor })),
);

function Editor({ content, ...props }: import("@/components/editor/Editor").EditorProps) {
  const needsSyntax = /(^|\n)\s*```/.test(content);
  const [syntaxReady, setSyntaxReady] = useState(!needsSyntax);

  useEffect(() => {
    if (!needsSyntax) {
      setSyntaxReady(true);
      return;
    }

    let active = true;
    setSyntaxReady(false);
    void Promise.all([
      import("@/components/editor/Editor"),
      import("@/components/editor/editorSyntax"),
    ]).then(([editorModule, syntaxModule]) => {
      syntaxModule.registerEditorSyntax(editorModule.editorLowlight);
      if (active) setSyntaxReady(true);
    });
    return () => {
      active = false;
    };
  }, [needsSyntax]);

  if (needsSyntax && !syntaxReady) {
    return (
      <div className="text-text-muted min-h-40 px-2 py-6 text-sm" role="status">
        Preparing code blocks…
      </div>
    );
  }

  return <EditorCore content={content} {...props} />;
}
const MarkdownPreview = lazy(() =>
  import("@/components/editor/MarkdownPreview").then((m) => ({ default: m.MarkdownPreview })),
);
const Breadcrumb = lazy(() =>
  import("@/components/editor/Breadcrumb").then((m) => ({ default: m.Breadcrumb })),
);
const CoverImage = lazy(() =>
  import("@/components/editor/CoverImage").then((m) => ({ default: m.CoverImage })),
);
const DocumentOutlineAccordion = lazy(() =>
  import("@/components/editor/OutlineRail").then((m) => ({
    default: m.DocumentOutlineAccordion,
  })),
);
const PageIcon = lazy(() =>
  import("@/components/editor/PageIcon").then((m) => ({ default: m.PageIcon })),
);

export function getDocumentSaveStatusClass(status: SaveStatus): string {
  if (status === "error") return "text-danger";
  if (status === "saving") return "text-accent";
  return "text-text-muted";
}

export function DocumentDraftStatus({
  accessRole,
  canPublish,
}: {
  accessRole: DraftRecord["access_role"];
  canPublish: boolean;
}) {
  return (
    <div
      className="text-text-muted mb-2 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs"
      data-document-draft-status="true"
    >
      <LockKeyhole size={13} aria-hidden="true" />
      <span>
        Unpublished draft ·{" "}
        {accessRole === "owner" ? "owned by you" : `shared with you as ${accessRole}`}
      </span>
      <span aria-hidden="true">·</span>
      <span>{canPublish ? "Publish when ready" : "Only the owner can publish"}</span>
    </div>
  );
}

interface DocumentData {
  page_edit_allowed?: boolean;
  archived_at?: string;
  deleted_at?: string;
  page_access_source?: "direct_page_grant";
  id: string;
  title: string;
  icon?: string;
  folder_id?: string;
  parent_id?: string;
  cover_image?: string;
  locked: boolean;
  locked_by?: string;
  space_id: string;
  created_by?: string;
  last_updated_by?: string;
  created_at?: string;
  updated_at?: string;
  published_at?: string;
  public?: boolean;
}

interface DocumentContent {
  id: string;
  title: string;
  content: string;
}

interface SpaceInfo {
  name: string;
  slug: string;
  member_role?: string;
}

interface ParsedDocumentContent {
  body: string;
  synopsis: string;
  pageWidth: DocumentPageWidth;
  customCss?: string;
  cssProvenance: PageCssProvenance;
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

export function parseDocumentContent(content: string, title?: string): ParsedDocumentContent {
  const css = inspectPageCssSource(content);
  const fmMatch = css.bodySource.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n)?/);
  const metadata = fmMatch ? parseSimpleFrontmatter(fmMatch[1]) : {};
  const rawBody = fmMatch ? css.bodySource.slice(fmMatch[0].length) : css.bodySource;
  return {
    body: stripLeadingMatchingH1(rawBody, title),
    synopsis: metadata.synopsis || metadata.excerpt || metadata.description || "",
    pageWidth: normalizeDocumentPageWidth(metadata.page_width),
    ...(css.renderCss ? { customCss: css.renderCss } : {}),
    cssProvenance: css.provenance,
  };
}

export function serializeDocumentContent(
  content: string,
  synopsis: string,
  pageWidth: DocumentPageWidth = "standard",
  customCss = "",
  cssProvenance?: PageCssProvenance,
): string {
  const trimmedSynopsis = synopsis.trim();
  const metadata: string[] = [];
  if (trimmedSynopsis) {
    const escaped = trimmedSynopsis.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
    metadata.push(`synopsis: "${escaped}"`);
  }
  if (pageWidth !== "standard") metadata.push(`page_width: ${pageWidth}`);
  // The legacy four-argument API represents deliberate CSS authoring. Loaded
  // editor and recovery state always carry their independently retained source.
  const provenance = cssProvenance ?? inspectPageCssSource(replaceCustomCss("", customCss)).provenance;
  const rebuilt = rebuildPageCssSource(content.trimStart(), provenance);
  if (rebuilt.kind === "held") throw new DraftSaveHeldError("source_conflict",
    "CSS markers need source review. Saving is paused to preserve the original. Keep this editor open and download a device copy before leaving.");
  if (metadata.length === 0) return rebuilt.content;
  return `---\n${metadata.join("\n")}\n---\n\n${rebuilt.content}`;
}

const DRAFT_CURSOR_MARKER = "\uE000";
function oneMarker(value: string): number | null {
  const offset = value.indexOf(DRAFT_CURSOR_MARKER);
  return offset >= 0 && offset === value.lastIndexOf(DRAFT_CURSOR_MARKER) ? offset : null;
}
function utf16Boundary(value: string, offset: number): boolean {
  return !(offset > 0 && offset < value.length && /[\ud800-\udbff]/u.test(value[offset - 1]) && /[\udc00-\udfff]/u.test(value[offset]));
}
export function documentOffsetForBodyOffset(
  body: string,
  offset: number,
  expectedDocument: string,
  synopsis: string,
  pageWidth: DocumentPageWidth,
  customCss: string,
  cssProvenance: PageCssProvenance,
): number | null {
  if (!Number.isSafeInteger(offset) || offset < 0 || offset > body.length || !utf16Boundary(body,offset) || body.includes(DRAFT_CURSOR_MARKER) || expectedDocument.includes(DRAFT_CURSOR_MARKER)) return null;
  let marked: string;
  try { marked = serializeDocumentContent(`${body.slice(0,offset)}${DRAFT_CURSOR_MARKER}${body.slice(offset)}`, synopsis, pageWidth, customCss, cssProvenance); }
  catch { return null; }
  const mapped = oneMarker(marked);
  return mapped !== null && marked.slice(0,mapped) + marked.slice(mapped + 1) === expectedDocument ? mapped : null;
}

export function retainedDraftDiscussionAnchor(
  selection: EditorSourceSelection | null,
  draftSource: { source_digest?: string; content: string } | null,
  body: string,
  synopsis: string,
  pageWidth: DocumentPageWidth,
  customCss: string,
  cssProvenance: PageCssProvenance,
): DraftDiscussionAnchor | null {
  if (!selection || !draftSource?.source_digest || selection.source !== body) return null;
  let exactDocument: string;
  try {
    exactDocument = serializeDocumentContent(
      selection.source,
      synopsis,
      pageWidth,
      customCss,
      cssProvenance,
    );
  } catch {
    return null;
  }
  if (exactDocument !== draftSource.content) return null;
  const anchor = documentOffsetForBodyOffset(
    selection.source,
    selection.anchor,
    draftSource.content,
    synopsis,
    pageWidth,
    customCss,
    cssProvenance,
  );
  const head = documentOffsetForBodyOffset(
    selection.source,
    selection.head,
    draftSource.content,
    synopsis,
    pageWidth,
    customCss,
    cssProvenance,
  );
  if (anchor === null || head === null) return null;
  return createDraftDiscussionAnchor(
    draftSource.content,
    draftSource.source_digest,
    Math.min(anchor, head),
    Math.max(anchor, head),
  );
}
export function bodyOffsetForDocumentOffset(documentSource: string, offset: number, title: string, expectedBody: string): number | null {
  if (!Number.isSafeInteger(offset) || offset < 0 || offset > documentSource.length || !utf16Boundary(documentSource,offset) || documentSource.includes(DRAFT_CURSOR_MARKER) || expectedBody.includes(DRAFT_CURSOR_MARKER)) return null;
  const marked = parseDocumentContent(`${documentSource.slice(0,offset)}${DRAFT_CURSOR_MARKER}${documentSource.slice(offset)}`, title).body;
  const mapped = oneMarker(marked);
  return mapped !== null && marked.slice(0,mapped) + marked.slice(mapped + 1) === expectedBody ? mapped : null;
}

/** Legacy rendering equality cannot establish whether original CSS was deliberately cleared. */
export function recoveryCssProvenance(checkpoint: DocumentDraftRecoverySnapshot): PageCssProvenance | undefined {
  if (checkpoint.cssProvenance) return checkpoint.cssProvenance;
  if (checkpoint.baseDraftSource && inspectPageCssSource(checkpoint.baseDraftSource.content).kind !== "absent") {
    throw new DraftSaveHeldError("source_conflict", "This older device copy has no verified CSS intent. The original source and edited body are retained for review and download; automatic reconstruction is paused.");
  }
  return undefined;
}
/** Reading a retained copy never requires creating a serializable mutation intent. */
export function recoveryCompositionHoldReason(checkpoint: DocumentDraftRecoverySnapshot): string | undefined {
  try { serializeRecoveryCheckpoint(checkpoint); return undefined; }
  catch (error) { return error instanceof Error ? error.message : "Retained CSS source requires review before saving. Keep the device copy."; }
}
export function serializeRecoveryCheckpoint(checkpoint: DocumentDraftRecoverySnapshot): string {
  return serializeDocumentContent(checkpoint.content, checkpoint.synopsis, checkpoint.pageWidth, checkpoint.customCss, recoveryCssProvenance(checkpoint));
}
export function recoveryCheckpointMatchesSaved(checkpoint: DocumentDraftRecoverySnapshot, saved: DraftRecord): boolean {
  try {
    return documentDraftRecoveryMatchesSaved(checkpoint, saved, {title: normalizeDocumentTitleInput(checkpoint.title) || "Untitled", content: serializeRecoveryCheckpoint(checkpoint)});
  } catch { return false; } // Ambiguous/newer evidence must never turn an ACK into a clear.
}
function canvasContentMatchesSaved(canvas: {content: string; synopsis: string; pageWidth: DocumentPageWidth; customCss: string; cssProvenance: PageCssProvenance}, savedContent: string): boolean {
  try { return serializeDocumentContent(canvas.content, canvas.synopsis, canvas.pageWidth, canvas.customCss, canvas.cssProvenance) === savedContent; }
  catch { return false; }
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
  directFolderId?: string | null,
): string[] {
  const folderById = new Map(folders.map((folder) => [folder.id, folder]));
  const docFolderId =
    directFolderId !== undefined
      ? directFolderId
      : documents.find((document) => document.id === docId)?.folder_id;
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
    "Edit view",
    "Markdown view",
    propertiesOpen ? "Hide page details" : "Page details",
    "Delete page",
  ];
}

export function isDocumentEditingMode(mode: EditorViewMode): boolean {
  return mode !== "preview";
}

export function getDocumentPageBottomPaddingClass(
  mobileLayout: boolean,
  mode: EditorViewMode,
): string {
  return mobileLayout && mode === "markdown" ? "pb-40" : "";
}

export function getVisibleDocumentViewModes(mode: EditorViewMode): readonly EditorViewMode[] {
  return mode === "source" ? ["preview", "markdown", "source"] : ["preview", "markdown"];
}

export function getDesktopDocumentViewModes(_mode: EditorViewMode): readonly EditorViewMode[] {
  return ["preview", "markdown"];
}

function getDocumentViewModeAction(
  mode: EditorViewMode,
  labels: string[],
): { label: string; icon: LucideIcon } {
  if (mode === "markdown") return { label: labels[3], icon: Pencil };
  if (mode === "source") return { label: labels[4], icon: Code2 };
  return { label: labels[2], icon: BookOpen };
}

function DocumentActionsMenu({
  starred,
  sourceActive,
  inlineCommentsActive,
  localMode,
  includeSourceAction = true,
  touchSafe = false,
  onToggleStarred,
  onOpenDrafts,
  onShare,
  onOpenSource,
  onToggleInlineComments,
  onMove,
  onDuplicate,
  onSaveAsTemplate,
  onArchive,
  onDelete,
}: {
  starred: boolean;
  sourceActive: boolean;
  inlineCommentsActive: boolean;
  localMode: boolean;
  includeSourceAction?: boolean;
  touchSafe?: boolean;
  onToggleStarred: () => void;
  onOpenDrafts: () => void;
  onShare: () => void;
  onOpenSource: () => void;
  onToggleInlineComments?: () => void;
  onMove: () => void;
  onDuplicate: () => void;
  onSaveAsTemplate?: () => void;
  onArchive: () => void;
  onDelete: () => void;
}) {
  const [open, setOpen] = useState(false);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const menuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const openingShareRef = useRef(false);
  const handleMenuKeyDown = useMenuKeyboardNavigation(menuRef);
  const runAction = (action: () => void) => {
    setOpen(false);
    action();
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className={`${CONTROL_CHROME_ACTION_CLASS} flex items-center justify-center ${
            touchSafe ? "h-11 w-11" : "h-8 w-8"
          }`}
          ref={menuTriggerRef}
          aria-label="More page actions"
          aria-haspopup="menu"
          aria-expanded={open}
          title="More page actions"
        >
          <MoreHorizontal size={14} />
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        side="bottom"
        sideOffset={6}
        className="w-56 p-1"
        role="menu"
        aria-label="Page actions"
        onCloseAutoFocus={(event) => {
          if (openingShareRef.current) {
            event.preventDefault();
            openingShareRef.current = false;
          }
        }}
      >
        <div ref={menuRef} onKeyDown={handleMenuKeyDown}>
          <button
            type="button"
            role="menuitemcheckbox"
            aria-checked={starred}
            className="starlane-menu-item"
            onClick={() => runAction(onToggleStarred)}
          >
            <Star size={14} fill={starred ? "currentColor" : "none"} />
            {starred ? "Remove favorite" : "Add to favorites"}
          </button>
          <button
            type="button"
            role="menuitem"
            className="starlane-menu-item"
            onClick={() => runAction(onOpenDrafts)}
          >
            <Files size={14} />
            Drafts
          </button>
          <button
            type="button"
            role="menuitem"
            className="starlane-menu-item"
            onClick={() => {
              openingShareRef.current = true;
              menuTriggerRef.current?.focus();
              runAction(onShare);
            }}
          >
            <Share2 size={14} />
            Share…
          </button>
          {includeSourceAction && (
            <button
              type="button"
              role="menuitemradio"
              aria-checked={sourceActive}
              className="starlane-menu-item"
              onClick={() => runAction(onOpenSource)}
            >
              <Code2 size={14} />
              View Markdown source
            </button>
          )}
          {!localMode && onToggleInlineComments && (
            <button
              type="button"
              role="menuitemcheckbox"
              aria-checked={inlineCommentsActive}
              className="starlane-menu-item"
              onClick={() => runAction(onToggleInlineComments)}
            >
              <Highlighter size={14} />
              {inlineCommentsActive ? "Hide inline comments" : "Show inline comments"}
            </button>
          )}
          <div className="border-border my-1 border-t" aria-hidden="true" />
          <button
            type="button"
            role="menuitem"
            className="starlane-menu-item"
            onClick={() => runAction(onMove)}
          >
            <MoveRight size={14} />
            Move page
          </button>
          <button
            type="button"
            role="menuitem"
            className="starlane-menu-item"
            onClick={() => runAction(onDuplicate)}
          >
            <Copy size={14} />
            Duplicate page
          </button>
          {onSaveAsTemplate && (
            <button
              type="button"
              role="menuitem"
              className="starlane-menu-item"
              onClick={() => runAction(onSaveAsTemplate)}
            >
              <Copy size={14} />
              Save as template
            </button>
          )}
          <button
            type="button"
            role="menuitem"
            className="starlane-menu-item"
            onClick={() => runAction(onArchive)}
          >
            <Archive size={14} />
            Archive page
          </button>
          <button
            type="button"
            role="menuitem"
            className="starlane-menu-item starlane-menu-item-danger"
            onClick={() => runAction(onDelete)}
          >
            <Trash2 size={14} />
            Delete page
          </button>
        </div>
      </PopoverContent>
    </Popover>
  );
}

export function DocumentSourceTextarea({
  content,
  locked,
  readOnly = false,
  autoFocus = false,
  onChange,
  inputRef,
}: {
  content: string;
  locked: boolean;
  readOnly?: boolean;
  autoFocus?: boolean;
  onChange: (content: string) => void;
  inputRef?: import("react").Ref<HTMLTextAreaElement>;
}) {
  const isReadOnly = locked || readOnly;

  return (
    <Textarea
      id="document-source"
      ref={inputRef}
      value={content}
      onChange={(event) => {
        if (!isReadOnly) onChange(event.target.value);
      }}
      readOnly={isReadOnly}
      autoFocus={autoFocus && !isReadOnly}
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
    readOnly?: boolean;
    onTitleChange: (title: string) => void;
    onCommit: (title: string) => void;
  }
>(({ title, locked, readOnly = false, onTitleChange, onCommit }, ref) => (
  <Textarea
    ref={ref}
    id="document-title"
    rows={1}
    wrap="soft"
    value={title}
    onChange={(event) => {
      if (!locked && !readOnly) {
        onTitleChange(normalizeDocumentTitleInput(event.target.value));
      }
    }}
    onBlur={(event) => {
      if (!locked && !readOnly) onCommit(normalizeDocumentTitleInput(event.currentTarget.value));
    }}
    onKeyDown={(event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        event.currentTarget.blur();
      }
    }}
    placeholder="Untitled"
    disabled={locked}
    readOnly={readOnly}
    className="document-title-input text-text placeholder:text-text-muted/40 h-auto max-h-80 !min-h-12 w-full resize-none overflow-hidden rounded-none !border-0 border-transparent bg-transparent px-0 py-1 leading-[1.1] font-bold [overflow-wrap:anywhere] break-words whitespace-pre-wrap !shadow-none outline-none focus:ring-0 focus:ring-offset-0 disabled:opacity-70"
  />
));

DocumentTitleTextarea.displayName = "DocumentTitleTextarea";

export const DOCUMENT_TITLE_ROW_CLASS =
  "document-title-row relative mb-3 flex min-w-0 flex-col items-start gap-1";
export const DOCUMENT_TITLE_ICON_GUTTER_CLASS =
  "document-title-icon-lane flex size-12 shrink-0 items-center justify-center";
export const DOCUMENT_TITLE_EMPTY_ICON_GUTTER_CLASS =
  "document-title-empty-icon-lane flex size-12 shrink-0 items-center justify-center";

export function shouldBlockDocumentRenderForDraft(
  publishedAt: string | undefined,
  requestedDraftId: string | null,
): boolean {
  return Boolean(requestedDraftId || !publishedAt);
}

export function getDocumentAttributionNames(
  document: { created_by?: string; last_updated_by?: string } | null,
  currentUser: UserDirectoryRecord | null | undefined,
  directory: UserDirectoryRecord[],
): { createdBy: string; updatedBy: string } {
  const displayNameFor = (identity?: string) => {
    if (!identity) return "Unknown user";
    if (currentUser && [currentUser.id, currentUser.email, currentUser.display_name].includes(identity))
      return currentUser.display_name || currentUser.email || currentUser.id;
    const record = directory.find(
      (candidate) =>
        candidate.id === identity || candidate.email === identity || candidate.display_name === identity,
    );
    return record ? record.display_name || record.email || record.id : identity;
  };
  return {
    createdBy: displayNameFor(document?.created_by),
    updatedBy: displayNameFor(document?.last_updated_by),
  };
}

export function DocumentPage() {
  const { spaceId, docId } = useParams<{ spaceId: string; docId: string }>();
  const location = useLocation();
  const navigate = useNavigate();
  const requestedDraftId = new URLSearchParams(location.search).get("draft");
  const { config, user } = useAuth();
  const rightPanel = useContext(RightPanelControlContext);
  const registerPageDetails = useContext(DocumentPageDetailsRegistrationContext);
  const [doc, setDoc] = useState<DocumentData | null>(null);
  const [selectedDraftOnly,setSelectedDraftOnly] = useState(false);
  const [selectedDraftPagePublished,setSelectedDraftPagePublished] = useState(false);
  const [selectedDraftViewStatus,setSelectedDraftViewStatus] = useState<"loading"|"denied"|"unavailable">("loading");
  const [content, setContent] = useState("");
  const [synopsis, setSynopsis] = useState("");
  const [customCss, setCustomCss] = useState("");
  const [cssProvenance, setCssProvenance] = useState<PageCssProvenance>({schemaVersion:1, kind:"absent"});
  const [pageWidth, setPageWidth] = useState<DocumentPageWidth>("standard");
  const [title, setTitle] = useState("");
  const [spaceName, setSpaceName] = useState("");
  const [spaceMemberRole, setSpaceMemberRole] = useState<string | undefined>();
  const [folderPath, setFolderPath] = useState<string[]>([]);
  const [parentPath, setParentPath] = useState<{ id: string; title: string }[]>([]);
  const [templates, setTemplates] = useState<TemplateRecord[]>([]);
  const [userDirectory, setUserDirectory] = useState<UserDirectoryRecord[]>([]);
  const [viewMode, setViewMode] = useState<EditorViewMode>("preview");
  const [activeDraft, setActiveDraftState] = useState<DraftRecord | null>(null);
  const [draftCommentSelection, setDraftCommentSelection] = useState<{
    scope: string;
    anchor: DraftDiscussionAnchor | null;
  } | null>(null);
  const confirmedSynopsisContent = activeDraft?.content, confirmedSynopsisTitle = activeDraft?.title;
  const confirmedSynopsis = useMemo(() => confirmedSynopsisContent !== undefined ? parseDocumentContent(confirmedSynopsisContent, confirmedSynopsisTitle ?? "").synopsis : "", [confirmedSynopsisContent, confirmedSynopsisTitle]);
  const synopsisSavePending = !!activeDraft && synopsis !== confirmedSynopsis;
  // A confirmed source advances synchronously before the next serialized save,
  // rather than waiting for React to commit state from the prior response.
  const confirmedDraftRef = useRef<DraftRecord | null>(null);
  const [peerNotice, setPeerNotice] = useState<"changed" | "unavailable" | "reconnecting" | null>(null);
  const [peerUpdated, setPeerUpdated] = useState<"body" | "metadata" | null>(null);
  const [draftPresence, setDraftPresence] = useState<DraftPresenceRoster | null>(null);
  const draftPresenceRef = useRef<DraftPresenceRoster | null>(null);
  const [draftPeerCursors,setDraftPeerCursors] = useState<EditorDraftPeerCursor[]>([]);
  const [draftPresenceStatus, setDraftPresenceStatus] = useState<"connecting" | "ready" | "reconnecting" | "unavailable">("connecting");
  const [replacementRecords, setReplacementRecords] = useState<DraftRecoveryReplacementRecord[]>([]);
  const [replacementHistoryError, setReplacementHistoryError] = useState("");
  const [replacementArchive, setReplacementArchive] = useState<DraftRecoveryReplacementRecord | null>(null);
  const peerSuspendRef = useRef<() => void>(() => {});
  const [peerReconnect, setPeerReconnect] = useState(0);
  const [peerReviewRequested, setPeerReviewRequested] = useState(false);
  const peerReviewActiveRef = useRef(false);
  const peerReviewTriggerRef = useRef<HTMLButtonElement | null>(null);
  const peerReviewReturnRef = useRef<{trigger:HTMLButtonElement;route:string;generation:number} | null>(null);
  const peerFocusFramesRef = useRef<number[]>([]);
  const cancelPeerFocusRestore = useCallback(() => {
    peerFocusFramesRef.current.forEach(frame => window.cancelAnimationFrame(frame));
    peerFocusFramesRef.current = [];
  }, []);
  useEffect(() => cancelPeerFocusRestore, [cancelPeerFocusRestore]);
  const peerBarrierRef = useRef(false);
  const peerUnavailableRef = useRef(false);
  const peerEditEpochRef = useRef(0);
  const peerPendingSavesRef = useRef(0);
  const peerNeedsFreshAfterSaveRef = useRef(false);
  const peerComposingRef = useRef(false);
  const peerCompositionOwnerRef = useRef<object | null>(null);
  const peerSnapshotRef = useRef<Extract<DraftPeerFrame, {type:"snapshot"}> | null>(null);
  const peerSnapshotHandlerRef = useRef<(frame: Extract<DraftPeerFrame, {type:"snapshot"}>) => void>(() => {});
  const peerCurrentRef = useRef<() => boolean>(() => false);
  const peerTransportRef = useRef<ReturnType<typeof createDraftPeerTransport> | null>(null);
  const externalEditorApplyRef = useRef<ApplyExternalEditorContent | null>(null);
  const registerExternalEditorApply = useCallback((apply: ApplyExternalEditorContent | null) => {
    externalEditorApplyRef.current = apply;
    if (apply) peerTransportRef.current?.resync();
  }, []);
  const publishEditorSelection = useCallback((selection: EditorSourceSelection | null) => {
    if (!selection || peerComposingRef.current) return;
    const source = confirmedDraftRef.current;
    if (!source?.source_digest || source.access_role === "viewer") return;
    let exactDocument: string;
    try { exactDocument = serializeDocumentContent(selection.source,synopsis,pageWidth,customCss,cssProvenance); }
    catch { return; }
    if (selection.source !== content || exactDocument !== source.content) return;
    const anchor = documentOffsetForBodyOffset(selection.source,selection.anchor,source.content,synopsis,pageWidth,customCss,cssProvenance);
    const head = documentOffsetForBodyOffset(selection.source,selection.head,source.content,synopsis,pageWidth,customCss,cssProvenance);
    if (anchor !== null && head !== null) peerTransportRef.current?.cursor(source.source_digest,anchor,head);
  },[content,synopsis,pageWidth,customCss,cssProvenance]);
  const requestDraftComment = useCallback((selection: EditorSourceSelection | null) => {
    const source = confirmedDraftRef.current;
    if (!source?.source_digest || !spaceId || !docId || !user?.id) return;
    const target: DraftDiscussionTarget = {
      spaceId,
      documentId: docId,
      draftId: source.id,
      sourceDigest: source.source_digest,
    };
    setDraftCommentSelection({
      scope: draftDiscussionScope(target),
      anchor: retainedDraftDiscussionAnchor(
        selection,
        source,
        content,
        synopsis,
        pageWidth,
        customCss,
        cssProvenance,
      ),
    });
    rightPanel.openComment();
  }, [content, customCss, cssProvenance, docId, pageWidth, rightPanel, spaceId, synopsis, user?.id]);
  const draftSaveHoldRef = useRef<DraftSaveHeldError | null>(null);
  const draftSaveTailRef = useRef<Promise<void>>(Promise.resolve());
  const draftSelectionGenerationRef = useRef(0);
  const selectedDraftContextRef = useRef<DraftSaveSelection | null>(null);
  const setActiveDraft = useCallback((draft: DraftRecord | null, confirmedSave = false, selection?: DraftSaveSelection) => {
    if (!confirmedSave) {
      draftSelectionGenerationRef.current += 1;
      peerEditEpochRef.current += 1;
      peerBarrierRef.current = false; peerUnavailableRef.current = false;
      peerSnapshotRef.current = null; peerNeedsFreshAfterSaveRef.current = false; peerReviewActiveRef.current = false;
      peerReviewReturnRef.current = null;
      setPeerNotice(null); setPeerUpdated(null); setPeerReviewRequested(false);
      setPeerReconnect(value => value + 1); // Same-ID explicit recovery also needs a new generation-scoped subscription.
    }
    if (confirmedDraftRef.current?.id !== draft?.id) draftSaveHoldRef.current = null;
    confirmedDraftRef.current = draft;
    if (!confirmedSave) selectedDraftContextRef.current = draft ? selection ?? null : null;
    setActiveDraftState(draft);
  }, []);
  const subscribePageContext = useCallback((changed:() => void) => doc ? subscribePublicationHistoryContext(changed) : () => {},[doc?.id]);
  const pageContextVersion = useSyncExternalStore(subscribePageContext,publicationHistoryContextVersion,() => 0);
  const pageCapabilityScope = JSON.stringify([spaceId,docId,user?.id,pageContextVersion,draftSelectionGenerationRef.current]);
  const capabilityScopeRef = useRef(pageCapabilityScope); capabilityScopeRef.current = pageCapabilityScope;
  const [ownerEditProof,setOwnerEditProof] = useState<{scope:string;generation:number;selection:DraftSaveSelection;page:DocumentRecord}|null>(null);
  const [ownerEditRefreshing,setOwnerEditRefreshing] = useState(false);
  const [ownerEditError,setOwnerEditError] = useState<string>();
  const currentPageCapabilityRef = useRef<() => PageEditCapability>(() => getPageEditCapability({mode:"server",page:null,selectionCurrent:false}));
  const readPageCapability = () => {
    const mode = getCurrentLaunchMode();
    let samePage = !!doc && doc.id === docId && doc.space_id === spaceId;
    if (confirmedDraftRef.current && selectedDraftContextRef.current) {
      try { verifyDraftSaveSelection(selectedDraftContextRef.current,user?.id); } catch { samePage = false; }
    }
    let capabilityCurrent = false;
    if (ownerEditProof?.scope === pageCapabilityScope && ownerEditProof.generation === draftSelectionGenerationRef.current && pageEditObservationFingerprint(ownerEditProof.page) === pageEditObservationFingerprint(doc) && publicationHistoryContextVersion() === pageContextVersion) {
      try { verifyDraftSaveSelection(ownerEditProof.selection,user?.id); capabilityCurrent = true; } catch { /* Original authority is no longer current. */ }
    }
    return getPageEditCapability({mode,page:capabilityCurrent ? ownerEditProof!.page : doc,actorId:user?.id,selectionCurrent:samePage,capabilityCurrent});
  };
  currentPageCapabilityRef.current = readPageCapability;
  const pageEditCapability = readPageCapability();
  const pageEditBlocked = !pageEditCapability.canEditPage;
  const pageEditExplanation = pageEditBlockedExplanation(pageEditCapability.blockedReason);
  const refreshOwnerEdit = useCallback(async () => {
    if (!spaceId || !docId || getCurrentLaunchMode() !== "server") return;
    const scope = pageCapabilityScope, generation = draftSelectionGenerationRef.current;
    setOwnerEditRefreshing(true); setOwnerEditError(undefined);
    try {
      // A new session/provider proof cannot admit a canvas bound to its original selected source.
      if (confirmedDraftRef.current && selectedDraftContextRef.current) verifyDraftSaveSelection(selectedDraftContextRef.current,user?.id);
      const selection = captureDraftSaveSelection(user?.id);
      let page:DocumentRecord;
      const selected = confirmedDraftRef.current;
      try {
        if (selectedDraftOnly) throw new Error("Selected draft Edit authority requires no published View.");
        page = await dataClient.getCurrentDocumentEditProjection(spaceId,docId,selection);
      } catch (error) {
        if (!selected || (!selectedDraftOnly && !(isApiError(error) && error.status === 403))) throw error;
        const source = await dataClient.getCurrentDocumentDraftEditSource(spaceId,docId,selected.id,selection,selected);
        page = {id:docId,space_id:spaceId,title:selected.title,locked:source.locked,locked_by:source.locked_by??undefined,page_edit_allowed:source.page_edit_allowed,updated_at:source.page_updated_at};
      }
      verifyDraftSaveSelection(selection,user?.id);
      if (capabilityScopeRef.current !== scope || draftSelectionGenerationRef.current !== generation) return;
      // Refresh authority and lock observation only; never replace editor presentation or draft source.
      setDoc(current => current?.id === docId && current.space_id === spaceId ? {...current,locked:page.locked === true,locked_by:page.locked_by,page_edit_allowed:page.page_edit_allowed,archived_at:page.archived_at,deleted_at:page.deleted_at,updated_at:page.updated_at} : current);
      setOwnerEditProof({scope,generation,selection,page});
    } catch (error) {
      if (capabilityScopeRef.current === scope) { setOwnerEditProof(null); setOwnerEditError(error instanceof Error ? error.message : "Current Edit access is unavailable."); }
    } finally { if (capabilityScopeRef.current === scope) setOwnerEditRefreshing(false); }
  },[spaceId,docId,user?.id,pageCapabilityScope,selectedDraftOnly]);
  useEffect(() => { setOwnerEditProof(null); setOwnerEditRefreshing(false); setOwnerEditError(undefined); },[pageCapabilityScope]);
  const ownerProofMatchesObservation = ownerEditProof?.scope === pageCapabilityScope && pageEditObservationFingerprint(ownerEditProof?.page ?? null) === pageEditObservationFingerprint(doc);
  let verifiedSelectedPageEdit = false;
  if (pageEditCapability.canEditPage && ownerProofMatchesObservation && ownerEditProof?.generation === draftSelectionGenerationRef.current && ownerEditProof.page.page_edit_allowed === true && confirmedDraftRef.current && confirmedDraftRef.current.access_role !== "viewer" && publicationHistoryContextVersion() === pageContextVersion) {
    try { verifyDraftSaveSelection(ownerEditProof.selection,user?.id); if (selectedDraftContextRef.current) verifyDraftSaveSelection(selectedDraftContextRef.current,user?.id); verifiedSelectedPageEdit = true; } catch { /* Role labels cannot grant current authority. */ }
  }
  const spaceRoleBlocksDraftEditing = spaceMemberRole === "viewer" && !verifiedSelectedPageEdit;
  useEffect(() => {
    if (doc?.locked && doc.locked_by === user?.id && getCurrentLaunchMode() === "server" && !ownerProofMatchesObservation && !ownerEditError) void refreshOwnerEdit();
  },[pageCapabilityScope,doc?.locked,doc?.locked_by,doc?.page_edit_allowed,user?.id,refreshOwnerEdit,ownerProofMatchesObservation,ownerEditError]);

  useEffect(() => {
    if (!selectedDraftOnly || !spaceId || !docId || !activeDraft) return;
    const selection = selectedDraftContextRef.current;
    if (!selection) return;
    const generation = draftSelectionGenerationRef.current,route=`${spaceId}/${docId}`;
    let cancelled=false; setSelectedDraftViewStatus("loading");
    const verify=()=>{if(cancelled||routeIdentityRef.current!==route||draftSelectionGenerationRef.current!==generation||recoveryAccountRef.current!==selection.accountId)throw new Error("Selected draft View context changed.");verifyDraftSaveSelection(selection,recoveryAccountRef.current);};
    void dataClient.getCurrentDocumentPublishedView(spaceId,docId,selection).then(({metadata})=>{
      verify();
      // Canonical page context only. Editor title/body/presentation and confirmed CAS source stay intact.
      setDoc(current=>current?.id===docId&&current.space_id===spaceId?{...metadata,space_id:spaceId,locked:metadata.locked===true,icon:current.icon,cover_image:current.cover_image}:current);
      setSelectedDraftOnly(false);
      if (metadata.page_access_source !== "direct_page_grant") {
        // Restore ordinary authorized context asynchronously; source activation and CAS stay independent.
        const verifySupplement = () => { if (routeIdentityRef.current !== route || draftSelectionGenerationRef.current !== generation || recoveryAccountRef.current !== selection.accountId) throw new Error("Published context selection changed."); verifyDraftSaveSelection(selection,recoveryAccountRef.current); };
        const folderRequest = folderPathRefreshRequestIdRef.current;
        void dataClient.getSpace(spaceId).then(space => { verifySupplement(); setSpaceName(space.name); setSpaceMemberRole(space.member_role); }).catch(() => {});
        void Promise.all([dataClient.listTemplates(spaceId).catch(() => []),dataClient.listFolders(spaceId).catch(() => []),dataClient.listDocuments(spaceId,{include_nested:true}).catch(() => [])]).then(([templates,folders,documents]) => { verifySupplement(); setTemplates(templates); if (folderRequest === folderPathRefreshRequestIdRef.current) { setFolderPath(buildDocumentFolderPath(folders,documents,docId,metadata.folder_id)); setParentPath(buildDocumentParentPath(documents,docId,metadata.parent_id)); } }).catch(() => {});
        void dataClient.listDocumentDrafts(spaceId,docId).then(drafts => { verifySupplement(); setDraftChoices(drafts); setDraftListUnavailable(false); }).catch(() => { try { verifySupplement(); setDraftListUnavailable(true); } catch { /* Foreign context is discarded. */ } });
        void loadStarredDocuments(spaceId).then(() => { verifySupplement(); setStarred(isDocumentStarred({spaceId,id:docId})); }).catch(() => {});
      }
    }).catch(error=>{try{verify();setSelectedDraftViewStatus(isApiError(error)&&error.status===403?"denied":"unavailable");}catch{/* A late optional View response has no authority in this selection. */}});
    return()=>{cancelled=true;};
  },[selectedDraftOnly,spaceId,docId,activeDraft,user?.id,pageContextVersion]);

  const [draftChoices, setDraftChoices] = useState<DraftSummaryRecord[]>([]);
  const [draftPromptOpen, setDraftPromptOpen] = useState(false);
  const [shareOpen, setShareOpen] = useState(false);
  const [publishing, setPublishing] = useState(false);
  const [publicationReview, setPublicationReview] = useState<DraftPublicationIntent | null>(null);
  const [publicationRecords, setPublicationRecords] = useState<DraftPublicationRecord[]>([]);
  const [publicationDispatching, setPublicationDispatching] = useState(false);
  const [publicationReload, setPublicationReload] = useState(0);
  const publicationBoundaryRef = useRef<{ route: string; generation: number; account?: string; canvas: string } | null>(null);
  const publicationPending = publicationRecords.some((record) => ["prepared", "dispatched", "unknown"].includes(record.state));
  const publicationPendingRef = useRef(publicationPending);
  publicationPendingRef.current = publicationPending;
  const publicationDispatchRef = useRef<string | null>(null);
  const publicationPreparationRef = useRef<symbol | null>(null);
  const [openingDraft, setOpeningDraft] = useState(false);
  const [draftListUnavailable, setDraftListUnavailable] = useState(false);
  const [saveQueued, setSaveQueued] = useState(false);
  const [draftHasChanges, setDraftHasChanges] = useState(false);
  const [draftError, setDraftError] = useState("");
  const [birthStatus, setBirthStatus] = useState<{ route: string; message: string } | null>(null);
  const [birthRetry, setBirthRetry] = useState<{ operationId: string; route: string; generation: number; canvas: string; selection: DraftSaveSelection } | null>(null);
  const [recoveryCandidate, setRecoveryCandidate] = useState<DocumentDraftRecoverySnapshot | null>(
    null,
  );
  const [recoveryStorageFailed, setRecoveryStorageFailed] = useState(false);
  const [unavailableRecoveryScope, setUnavailableRecoveryScope] = useState<DocumentDraftRecoveryScope | null>(null);
  const [recoveryReviewUnavailable, setRecoveryReviewUnavailable] = useState(false);
  const [recoveryReviewDraft, setRecoveryReviewDraft] = useState<DraftRecord | null>(null);
  const [recoveryReviewOpen, setRecoveryReviewOpen] = useState(false);
  const [recoveryReplaceConfirm, setRecoveryReplaceConfirm] = useState(false);
  const [recoveryDiscardConfirm, setRecoveryDiscardConfirm] = useState(false);
  const recoveryAccountRef = useRef(user?.id);
  recoveryAccountRef.current = user?.id;
  const [recoveryBusy, setRecoveryBusy] = useState(false);
  const recoveryBusyRef = useRef(false);
  const [recoveryReviewError, setRecoveryReviewError] = useState("");
  const [recoveryRestartReady, setRecoveryRestartReady] = useState(false);
  const [recoveryTargetReview, setRecoveryTargetReview] = useState<DraftRecord | null>(null);
  const [recoveryOriginalIntent, setRecoveryOriginalIntent] = useState<DraftRecoveryIntent | null>(null);
  const [recoveryTargetHeld, setRecoveryTargetHeld] = useState(false);
  const [queuedCopyRecords,setQueuedCopyRecords] = useState<QueuedCopyReviewRecord[]>([]);
  const [queuedCopyReview,setQueuedCopyReview] = useState<{record:QueuedCopyReviewRecord;context:Omit<NonNullable<typeof heldQueueReview>,"review">}|null>(null);
  const [queuedCopyRestartIntent,setQueuedCopyRestartIntent] = useState<DraftRecoveryIntent|null>(null);
  const [queuedCopyTargetReview,setQueuedCopyTargetReview] = useState<DraftRecord|null>(null);
  const [queuedCopyOperation,setQueuedCopyOperation] = useState<DraftRecoveryCopyRecord|null>(null);
  const [heldQueueItems,setHeldQueueItems] = useState<HeldDraftQueueItem[]>([]);
  const [heldQueueError,setHeldQueueError] = useState("");
  const [heldQueueBusy,setHeldQueueBusy] = useState(false);
  const heldQueueBusyRef = useRef(false);
  const heldQueueOperationRef = useRef<symbol|null>(null);
  const [heldQueueReview,setHeldQueueReview] = useState<{
    review:HeldDraftQueueReview; selection:DraftSaveSelection; route:string; generation:number;
    draftId:string|null; canvas:string; scope:DocumentDraftRecoveryScope|null; checkpointId:string|null;
  }|null>(null);

  const getRecoveryScope = useCallback(
    (draft: DraftRecord | null, selection = selectedDraftContextRef.current): DocumentDraftRecoveryScope | null => {
      if (!spaceId || !docId || !draft || draft.document_id !== docId || draft.space_id !== spaceId)
        return null;
      if (!selection) return null;
      const storageMode = selection.mode === "local" ? "local" : "server";
      const authIdentity = selection.accountId;
      if (!authIdentity) return null;
      return {
        storageMode,
        authIdentity,
        serverTarget: selection.serverBase,
        spaceId,
        documentId: docId,
        draftId: draft.id,
      };
    },
    [spaceId, docId],
  );
  const recoveryScope = useMemo(
    () => getRecoveryScope(activeDraft) ?? unavailableRecoveryScope,
    [getRecoveryScope, activeDraft, unavailableRecoveryScope],
  );
  const checkDraftRecovery = useCallback(
    (draft: DraftRecord, _locked: boolean, selection = selectedDraftContextRef.current) => {
      const scope = getRecoveryScope(draft, selection);
      setRecoveryCandidate(
        scope
          ? readDocumentDraftRecovery(scope)
          : null,
      );
      setRecoveryStorageFailed(false);
    },
    [getRecoveryScope],
  );
  const [saveAsTemplateOpen, setSaveAsTemplateOpen] = useState(false);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [deleteError, setDeleteError] = useState("");
  const [starred, setStarred] = useState(false);
  const [inlineCommentsVisible, setInlineCommentsVisible] = useState(false);
  const [loading, setLoading] = useState(true);
  const [editorRef, setEditorRef] = useState<TipTapEditor | null>(null);
  const [focusEditorOnMount, setFocusEditorOnMount] = useState(false);
  const titleRef = useRef<HTMLTextAreaElement | null>(null);
  const sourceTextareaRef = useRef<HTMLTextAreaElement | null>(null);
  const documentBodyRef = useRef<HTMLDivElement>(null);
  const [documentBodyReady, setDocumentBodyReady] = useState(false);
  const setDocumentBody = useCallback((node: HTMLDivElement | null) => {
    documentBodyRef.current = node;
    setDocumentBodyReady(Boolean(node));
  }, []);
  const [desktopToolbarHost, setDesktopToolbarHost] = useState<HTMLDivElement | null>(null);
  const [contentsOpen, setContentsOpen] = useState(() => typeof window !== "undefined" && window.matchMedia("(min-width: 1440px)").matches);
  const [headingCount, setHeadingCount] = useState(0);
  const [largeDocumentLayout, setLargeDocumentLayout] = useState(() => typeof window !== "undefined" && window.matchMedia("(min-width: 1920px)").matches);
  const contentsTriggerRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    const wide = window.matchMedia("(min-width: 1440px)");
    const large = window.matchMedia("(min-width: 1920px)");
    const updateWide = () => setContentsOpen(wide.matches);
    const updateLarge = () => setLargeDocumentLayout(large.matches);
    wide.addEventListener("change", updateWide);
    large.addEventListener("change", updateLarge);
    return () => { wide.removeEventListener("change", updateWide); large.removeEventListener("change", updateLarge); };
  }, []);
  useEffect(() => {
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented || !contentsOpen || rightPanel.open) return;
      setContentsOpen(false);
      contentsTriggerRef.current?.focus({ preventScroll: true });
    };
    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [contentsOpen, rightPanel.open]);
  const pageWidthRef = useRef<DocumentPageWidth>("standard");
  const folderPathRefreshRequestIdRef = useRef(0);
  const draftFlowStartedRef = useRef(false);
  const routeIdentityRef = useRef(`${spaceId}/${docId}`);
  routeIdentityRef.current = `${spaceId}/${docId}`;
  const activeDraftIdRef = useRef(activeDraft?.id);
  activeDraftIdRef.current = activeDraft?.id;
  const currentSaveStateRef = useRef({ title, content, synopsis, pageWidth, customCss, cssProvenance, icon: doc?.icon ?? null, coverImage: doc?.cover_image ?? null });
  currentSaveStateRef.current = { title, content, synopsis, pageWidth, customCss, cssProvenance, icon: doc?.icon ?? null, coverImage: doc?.cover_image ?? null };
  const assetContext = useMemo(() => ({ space_id: spaceId, document_id: docId, draft_id:activeDraft?.id }), [spaceId, docId, activeDraft?.id]);
  const { isDragging, dropRef, hasPendingFileDrops, fileDropError } = useFileDrop(editorRef, assetContext, {
    draft_id: activeDraft?.id,
    readOnly: viewMode !== "markdown" || !activeDraft || activeDraft.access_role === "viewer" ||
      doc?.page_access_source === "direct_page_grant" || peerUnavailableRef.current || pageEditBlocked ||
      publishing || publicationPending || Boolean(publicationReview) || openingDraft || Boolean(recoveryCandidate),
  });
  const { isMobileRuntime, isMobileViewport } = useRuntimePlatform();
  const mobileLayout = isMobileRuntime || isMobileViewport;
  const online = useOnlineStatus();
  const previousOnlineRef = useRef(online);
  const lastSaveNotificationAtRef = useRef(0);
  const saveActiveDraft = useCallback((patch: { title?: string; icon?: string; cover_image?: string; content?: string }) => {
    if (peerBarrierRef.current) return Promise.reject(new DraftSaveHeldError("source_conflict", "Shared draft changes need review. Your device edits remain open."));
    if (heldQueueBusyRef.current) return Promise.reject(new DraftSaveHeldError("queued","Review of the original queued save is in progress. Keep your device edits."));
    if (publicationPendingRef.current) return Promise.reject(new Error("Review the original retained publication before saving this draft."));
    const original = confirmedDraftRef.current;
    const originalRoute = `${spaceId}/${docId}`;
    const originalGeneration = draftSelectionGenerationRef.current;
    const selection = selectedDraftContextRef.current;
    try {
      if (!selection) throw new DraftSaveHeldError("source_required", "Review this draft before saving. Your edits remain on this device.");
      verifyDraftSaveSelection(selection, user?.id);
    } catch (error) {
      const held = isDraftSaveHeldError(error) ? error : new DraftSaveHeldError("source_required", "Original draft session is unavailable. Keep your edits and sign in before reviewing it.");
      if (routeIdentityRef.current === originalRoute && draftSelectionGenerationRef.current === originalGeneration) {
        draftSaveHoldRef.current = held;
        setDraftError(held.message);
        setSaveQueued(false);
      }
      return Promise.reject(held);
    }
    const intent = { ...patch };
    peerPendingSavesRef.current += 1;
    const task = draftSaveTailRef.current.then(async () => {
      if (draftSaveHoldRef.current) throw draftSaveHoldRef.current;
      const source = confirmedDraftRef.current;
      if (!spaceId || !docId || !original || !source || source.id !== original.id || draftSelectionGenerationRef.current !== originalGeneration || routeIdentityRef.current !== originalRoute) throw new DraftSaveHeldError("source_conflict", "Draft selection changed. Your unsaved edit requires review.");
      if (peerBarrierRef.current) {
        const held = new DraftSaveHeldError("source_conflict", "Saving paused before dispatch because shared changes need review. Your device proposal remains retained.");
        draftSaveHoldRef.current = held; setDraftError(held.message); setPeerNotice("changed");
        throw held; // Known unsent: never classify this local refusal as an unknown server outcome.
      }
      try {
        verifyDraftSaveSelection(selection, user?.id);
        const capability = currentPageCapabilityRef.current();
        assertDocumentPageEditable(capability);
        const saved = await dataClient.updateDocumentDraft(spaceId, docId, source.id, intent, source.source_digest);
        verifyDraftSaveSelection(selection, user?.id);
        if (routeIdentityRef.current !== originalRoute || confirmedDraftRef.current?.id !== source.id || draftSelectionGenerationRef.current !== originalGeneration) throw new DraftSaveHeldError("outcome_unknown", "Draft selection changed during save; review the retained edit before continuing.");
        setActiveDraft(saved, true);
        const scope = getRecoveryScope(saved, selection);
        const checkpoint = scope ? readDocumentDraftRecovery(scope) : null;
        if (scope && checkpoint && recoveryCheckpointMatchesSaved(checkpoint, saved)) {
          clearDocumentDraftRecovery(scope, checkpoint.snapshotId, "confirmed");
        }
        return saved;
      } catch (error) {
        const heldError = isDraftSaveHeldError(error) ? error : new DraftSaveHeldError("outcome_unknown", "Could not confirm this draft save. Keep your edits and review before saving again.");
        if (routeIdentityRef.current === originalRoute && confirmedDraftRef.current?.id === source.id && draftSelectionGenerationRef.current === originalGeneration) {
          draftSaveHoldRef.current = heldError;
          setDraftError(heldError.message);
          setSaveQueued(heldError.reason === "queued");
          const scope = getRecoveryScope(source, selection);
          if (scope && heldError.reason === "source_conflict" && peerCurrentRef.current()) {
            peerBarrierRef.current = true; peerSuspendRef.current();
            setPeerNotice("changed"); peerTransportRef.current?.resync();
          } else if (scope) setRecoveryCandidate(readDocumentDraftRecovery(scope));
        }
        throw heldError;
      }
    });
    const tracked = task.finally(() => { peerPendingSavesRef.current -= 1; });
    draftSaveTailRef.current = tracked.then(() => undefined, () => undefined);
    return tracked;
  }, [spaceId, docId, setActiveDraft, getRecoveryScope, user?.id]);
  const saveFn = useCallback(
    async (newContent: string) => {
      if (!spaceId || !docId || !activeDraft || activeDraft.access_role === "viewer") return;
      if (recoveryCandidate)
        throw new Error("Choose whether to restore this device's edits before saving.");
      const currentSaveState = currentSaveStateRef.current;
      const savedTitle = normalizeDocumentTitleInput(currentSaveState.title) || "Untitled";
      let savedContent: string;
      try { savedContent = serializeDocumentContent(newContent, currentSaveState.synopsis, pageWidthRef.current, currentSaveState.customCss, currentSaveState.cssProvenance); }
      catch (error) {
        if (isDraftSaveHeldError(error)) {
          draftSaveHoldRef.current = error; setDraftError(error.message);
          if (recoveryScope) setRecoveryCandidate(readDocumentDraftRecovery(recoveryScope));
        }
        throw error;
      }
      const checkpoint = recoveryScope ? readDocumentDraftRecovery(recoveryScope) : null;
      const saved = await saveActiveDraft({
        title: savedTitle,
        content: savedContent,
      });
      if (checkpoint && recoveryScope && recoveryCheckpointMatchesSaved(checkpoint, saved)) clearDocumentDraftRecovery(recoveryScope, checkpoint.snapshotId, "confirmed");
      if (
        routeIdentityRef.current !== `${spaceId}/${docId}` ||
        activeDraftIdRef.current !== activeDraft.id
      )
        return;
      // Held or queued saves throw above and retain the recovery checkpoint.
      setSaveQueued(false);
      const latest = currentSaveStateRef.current;
      if (
        (normalizeDocumentTitleInput(latest.title) || "Untitled") === savedTitle &&
        canvasContentMatchesSaved(latest, savedContent) &&
        (!recoveryScope || !readDocumentDraftRecovery(recoveryScope))
      ) {
        setDraftHasChanges(false);
        setRecoveryStorageFailed(false);
      }
    },
    [spaceId, docId, activeDraft, recoveryScope, recoveryCandidate, saveActiveDraft],
  );

  const { status, scheduleAutoSave, saveNow, resetAutoSave, suspendAutoSave, resumeAutoSave } = useAutoSave(saveFn, undefined, (error) => !isDraftSaveHeldError(error));
  peerSuspendRef.current = suspendAutoSave;
  useLayoutEffect(() => {
    if (!pageEditBlocked || !draftHasChanges || !activeDraft) return;
    suspendAutoSave();
    const held = new DraftSaveHeldError("source_conflict",pageEditExplanation ?? "Review current page access before saving retained edits.");
    draftSaveHoldRef.current = draftSaveHoldRef.current ?? held;
    setDraftError(draftSaveHoldRef.current.message);
    const scope = getRecoveryScope(activeDraft);
    if (scope) setRecoveryCandidate(readDocumentDraftRecovery(scope));
  },[pageEditBlocked,pageEditExplanation,draftHasChanges,activeDraft,content,suspendAutoSave,getRecoveryScope]);
  const navigationBlocker = useBlocker(draftHasChanges && recoveryStorageFailed);
  useEffect(() => {
    if (navigationBlocker.state !== "blocked") return;
    let cancelled = false;
    setOpeningDraft(true);
    void saveNow(currentSaveStateRef.current.content).then((saved) => {
      if (cancelled) return;
      setOpeningDraft(false);
      if (saved) navigationBlocker.proceed();
      else {
        navigationBlocker.reset();
        setDraftError(
          "Could not save your edits or keep a recovery copy. Stay on this page and retry saving before leaving.",
        );
      }
    });
    return () => {
      cancelled = true;
    };
  }, [navigationBlocker, saveNow]);

  useEffect(() => {
    if (!draftHasChanges || !recoveryStorageFailed) return;
    const warnBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", warnBeforeUnload);
    return () => window.removeEventListener("beforeunload", warnBeforeUnload);
  }, [draftHasChanges, recoveryStorageFailed]);

  const checkpointDraft = useCallback(
    (changes: Partial<typeof currentSaveStateRef.current>) => {
      const snapshot = { ...currentSaveStateRef.current, ...changes };
      if (JSON.stringify(snapshot) !== JSON.stringify(currentSaveStateRef.current)) peerEditEpochRef.current += 1;
      currentSaveStateRef.current = snapshot;
      if (!recoveryScope || !activeDraft || activeDraft.access_role === "viewer") return;
      if ((normalizeDocumentTitleInput(snapshot.title) || "Untitled") === activeDraft.title &&
        canvasContentMatchesSaved(snapshot, activeDraft.content) &&
        (snapshot.icon ?? "") === (activeDraft.icon ?? "") && (snapshot.coverImage ?? "") === (activeDraft.cover_image ?? "")) return;
      setRecoveryStorageFailed(
        !persistDocumentDraftRecovery(recoveryScope, snapshot, activeDraft.updated_at, undefined, undefined, {
          draftId: activeDraft.id, sourceDigest: activeDraft.source_digest,
          title: activeDraft.title, content: activeDraft.content,
          icon: activeDraft.icon ?? null, coverImage: activeDraft.cover_image ?? null,
        }),
      );
    },
    [recoveryScope, activeDraft],
  );

  const peerCheckpointRef = useRef(checkpointDraft);
  peerCheckpointRef.current = checkpointDraft;
  const peerRuntimeRef = useRef({ loading, draftHasChanges, saveQueued, recoveryCandidate, publishing, openingDraft, publicationReview, status, viewMode });
  peerRuntimeRef.current = { loading, draftHasChanges, saveQueued, recoveryCandidate, publishing, openingDraft, publicationReview, status, viewMode };
  peerSnapshotHandlerRef.current = (frame) => {
    if (!peerCurrentRef.current()) return;
    peerSnapshotRef.current = frame; // Keep the exact null-preserving raw projection for review/download.
    const original = confirmedDraftRef.current;
    if (!original) return;
    const latest = currentSaveStateRef.current;
    const flags = peerRuntimeRef.current;
    const scope = getRecoveryScope(original);
    const queued = () => !!scope && listOfflineServerMutations(scope.serverTarget).some(entry => entry.path === `/spaces/${scope.spaceId}/documents/${scope.documentId}/drafts/${scope.draftId}`);
    const sameSource = frame.draft.source_digest === original.source_digest && frame.draft.title === original.title && frame.draft.content === original.content && frame.draft.icon === (original.icon ?? null) && frame.draft.cover_image === (original.cover_image ?? null);
    const sameAuthority = frame.draft.access_role === original.access_role && frame.draft.can_publish === original.can_publish;
    if (peerPendingSavesRef.current) { peerNeedsFreshAfterSaveRef.current = true; return; } // Only the actual REST result may settle its original request.
    if (sameSource && sameAuthority) {
      if (peerUnavailableRef.current && (flags.draftHasChanges || !!scope && !!readDocumentDraftRecovery(scope) || !canvasContentMatchesSaved(latest, original.content)
        || (normalizeDocumentTitleInput(latest.title) || "Untitled") !== original.title || (latest.icon ?? null) !== (original.icon ?? null) || (latest.coverImage ?? null) !== (original.cover_image ?? null))) {
        // A renewed grant/lock admission is not permission to replay dirty work.
        // Restore authorized typing, but require an explicit retained-source review.
        peerUnavailableRef.current = false; peerBarrierRef.current = true;
        suspendAutoSave(); setPeerNotice("changed"); return;
      }
      if (!draftSaveHoldRef.current && !flags.saveQueued && !queued() && !peerReviewActiveRef.current && !flags.recoveryCandidate && !publicationPendingRef.current) {
        peerBarrierRef.current = false; peerUnavailableRef.current = false;
        setPeerNotice(null); resumeAutoSave(); // Retain dirty versions and their genuine own-ACK baseline.
      }
      return;
    }
    const hold = () => {
      peerBarrierRef.current = true;
      suspendAutoSave();
      if (!flags.recoveryCandidate && !peerReviewActiveRef.current) checkpointDraft({});
      setPeerNotice("changed");
    };
    const epoch = peerEditEpochRef.current;
    const canvasIdentity = JSON.stringify(latest);
    const originalCanvas = recoveryReviewContent(original);
    const controlsMatch = (canvas: typeof latest) => (!titleRef.current || titleRef.current.value === canvas.title)
      && (flags.viewMode !== "source" || !!sourceTextareaRef.current && sourceTextareaRef.current.value === canvas.content.replace(/\r\n?/g,"\n"));
    const clean = () => controlsMatch(latest) && !document.querySelector('[role="dialog"]') && peerCurrentRef.current() && confirmedDraftRef.current === original && peerEditEpochRef.current === epoch && JSON.stringify(currentSaveStateRef.current) === canvasIdentity
      && !peerPendingSavesRef.current && !hasPendingFileDrops() && !peerComposingRef.current && !draftSaveHoldRef.current && !flags.loading && !flags.draftHasChanges && !flags.saveQueued && flags.status !== "saving"
      && !flags.recoveryCandidate && !recoveryBusyRef.current && !heldQueueBusyRef.current && !peerReviewActiveRef.current && !flags.publishing && !flags.openingDraft
      && !flags.publicationReview && !publicationPendingRef.current && !publicationPreparationRef.current && !publicationDispatchRef.current && !publicationBoundaryRef.current
      && !queued() && !!scope && !readDocumentDraftRecovery(scope)
      && canvasIdentity === JSON.stringify({ title: normalizeDocumentTitleInput(original.title), content: originalCanvas.content, synopsis: originalCanvas.synopsis, pageWidth: originalCanvas.pageWidth,
        customCss: originalCanvas.customCss, cssProvenance: originalCanvas.cssProvenance, icon: original.icon ?? null, coverImage: original.cover_image ?? null });
    if (!clean()) { hold(); return; }
    const fresh: DraftRecord = { ...frame.draft, icon: frame.draft.icon ?? undefined, cover_image: frame.draft.cover_image ?? undefined };
    const parsed = parseDocumentContent(fresh.content, fresh.title);
    if (flags.viewMode === "markdown" && originalCanvas.content !== parsed.body && (!externalEditorApplyRef.current || !externalEditorApplyRef.current(parsed.body, clean, originalCanvas.content))) { hold(); return; }
    if (!clean()) { hold(); return; }
    const nextCanvas = { title: normalizeDocumentTitleInput(fresh.title), content: parsed.body, synopsis: parsed.synopsis, pageWidth: parsed.pageWidth,
      customCss: parsed.customCss ?? "", cssProvenance: parsed.cssProvenance, icon: frame.draft.icon, coverImage: frame.draft.cover_image };
    const focusedControl = [titleRef.current, sourceTextareaRef.current].find(control => control && control === document.activeElement);
    const caret = focusedControl ? { control:focusedControl, head:focusedControl.selectionEnd, top:focusedControl.scrollTop, left:focusedControl.scrollLeft } : null;
    // This handler runs from the native socket callback. Commit controlled title
    // and source DOM synchronously before advancing the confirmed save baseline.
    flushSync(() => {
      setTitle(nextCanvas.title); setContent(parsed.body); setSynopsis(parsed.synopsis);
      setCustomCss(parsed.customCss ?? ""); setCssProvenance(parsed.cssProvenance); pageWidthRef.current = parsed.pageWidth; setPageWidth(parsed.pageWidth);
      setDoc(current => current ? { ...current, icon:fresh.icon, cover_image:fresh.cover_image } : current);
    });
    if (!peerCurrentRef.current() || confirmedDraftRef.current !== original || peerEditEpochRef.current !== epoch || JSON.stringify(currentSaveStateRef.current) !== JSON.stringify(nextCanvas) || !controlsMatch(nextCanvas)) { hold(); return; }
    if (caret?.control.isConnected && document.activeElement === caret.control) {
      // Native textarea caret offsets belong only to this visible control, not
      // the retained full Markdown source or any published peer cursor.
      let head = Math.min(caret.head, caret.control.value.length);
      const before = caret.control.value.charCodeAt(head - 1), after = caret.control.value.charCodeAt(head);
      if (before >= 0xd800 && before <= 0xdbff && after >= 0xdc00 && after <= 0xdfff) head -= 1;
      caret.control.setSelectionRange(head,head);
      caret.control.scrollTop = caret.top; caret.control.scrollLeft = caret.left;
    }
    // A clean observation is not a REST acknowledgement: no queue/checkpoint is cleared.
    flushSync(() => {
      setActiveDraft(fresh,true); resetAutoSave(parsed.body);
      peerBarrierRef.current = false; peerUnavailableRef.current = false;
      setPeerNotice(null); setPeerUpdated(originalCanvas.content !== parsed.body ? "body" : "metadata");
    });
  };
  useEffect(() => {
    if (peerNeedsFreshAfterSaveRef.current && !peerPendingSavesRef.current) {
      // A snapshot captured before our own ACK cannot rebase that newly confirmed
      // source backwards. Inspect again instead of replaying the retained frame.
      peerNeedsFreshAfterSaveRef.current = false;
      peerTransportRef.current?.resync();
    }
  }, [status, draftHasChanges, saveQueued, activeDraft]);
  useEffect(() => {
    const source = confirmedDraftRef.current, selection = selectedDraftContextRef.current;
    if (loading || !source || !selection || selection.mode !== "server" || !spaceId || !docId || !user?.id) return;
    try { verifyDraftSaveSelection(selection, user.id); }
    catch { peerBarrierRef.current = true; peerUnavailableRef.current = true; suspendAutoSave(); setPeerNotice("unavailable"); return; }
    const bearer = api.getToken();
    if (!bearer || typeof WebSocket === "undefined") return;
    const generation = draftSelectionGenerationRef.current, account = user.id, route = `${spaceId}/${docId}`, base = getApiBase();
    let live = true;
    let lastActivity = Date.now(), lastPresence: "active" | "idle" | null = null;
    const current = () => {
      if (!live || routeIdentityRef.current !== route || draftSelectionGenerationRef.current !== generation || recoveryAccountRef.current !== account || confirmedDraftRef.current?.id !== source.id || selectedDraftContextRef.current !== selection || api.getToken() !== bearer || getApiBase() !== base) return false;
      try { verifyDraftSaveSelection(selection, account); return true; } catch { return false; }
    };
    peerCurrentRef.current = current;
    let transport: ReturnType<typeof createDraftPeerTransport>;
    const reportActivity = (force = false) => {
      if (!current()) return;
      const next = document.visibilityState === "visible" && document.hasFocus() && Date.now() - lastActivity < 45000 ? "active" : "idle";
      if ((force || next !== lastPresence) && transport.presence(next)) lastPresence = next;
    };
    const activity = () => { lastActivity = Date.now(); reportActivity(); };
    const visibility = () => {
      // Hidden/suspended pages cannot keep an apparently current roster on return.
      draftPresenceRef.current = null; setDraftPresence(null); setDraftPeerCursors([]); lastPresence = null;
      if (document.visibilityState === "visible") { lastActivity = Date.now(); transport.resync(); }
      reportActivity(true);
    };
    const focus = () => { lastActivity = Date.now(); reportActivity(true); };
    const blur = () => reportActivity(true);
    draftPresenceRef.current = null; setDraftPresence(null); setDraftPeerCursors([]); setDraftPresenceStatus("connecting");
    try { transport = createDraftPeerTransport({ apiBase: base, pageUrl: window.location.href, bearer,
      key: {space_id:spaceId,document_id:docId,draft_id:source.id}, mode: source.access_role === "viewer" ? "view" : "edit", isCurrent:current,
      presence: true,
      onRoster: frame => {
        if (!current()) return;
        const shown = document.visibilityState === "visible" ? frame : null;
        draftPresenceRef.current = shown;
        setDraftPresence(previous => JSON.stringify(previous) === JSON.stringify(shown) ? previous : shown);
        const liveConnections = new Set(shown?.peers.map(peer => peer.connection_id) ?? []);
        setDraftPeerCursors(previous => previous.filter(cursor => liveConnections.has(cursor.connectionId)));
        if (frame) setDraftPresenceStatus("ready");
      },
      onSnapshot: frame => peerSnapshotHandlerRef.current(frame),
      onPeer: frame => {
        if (!current() || frame.type !== "cursor") return;
        const selected = confirmedDraftRef.current, roster = draftPresenceRef.current;
        if (!selected || frame.source_digest !== selected.source_digest || frame.connection_id === roster?.connection_id) return;
        const peer = roster?.peers.find(candidate => candidate.connection_id === frame.connection_id);
        if (!peer) return;
        const expectedBody = parseDocumentContent(selected.content,selected.title).body;
        const anchor = bodyOffsetForDocumentOffset(selected.content,frame.anchor,selected.title,expectedBody);
        const head = bodyOffsetForDocumentOffset(selected.content,frame.head,selected.title,expectedBody);
        if (anchor === null || head === null) return;
        setDraftPeerCursors(previous => [...previous.filter(cursor => cursor.connectionId !== frame.connection_id),{
          connectionId:frame.connection_id,displayName:peer.display_name,source:expectedBody,anchor,head,
        }]);
      },
      onInvalidation: () => { setDraftPeerCursors([]); peerBarrierRef.current = true; suspendAutoSave(); },
      onStatus: next => {
        if (next === "ready") { reportActivity(true); return; }
        draftPresenceRef.current = null; setDraftPresence(null); setDraftPeerCursors([]);
        setDraftPresenceStatus(next === "reconnecting" ? "reconnecting" : next === "failed" || next === "unavailable" ? "unavailable" : "connecting");
        if (next !== "unavailable" && next !== "failed" && next !== "reconnecting") return;
        peerBarrierRef.current = true; suspendAutoSave();
        if (next === "unavailable") peerUnavailableRef.current = true;
        if (!peerRuntimeRef.current.recoveryCandidate && !peerReviewActiveRef.current) peerCheckpointRef.current({});
        setPeerNotice(next === "unavailable" ? "unavailable" : "reconnecting");
      },
    });
    } catch { peerBarrierRef.current = true; suspendAutoSave(); setPeerNotice("reconnecting"); return; }
    peerTransportRef.current = transport; transport.start();
    // Input activity changes a small advisory frame, never the save epoch or canvas.
    document.addEventListener("input", activity, true); document.addEventListener("pointerdown", activity, {passive:true});
    document.addEventListener("visibilitychange", visibility); window.addEventListener("focus", focus); window.addEventListener("blur", blur);
    const activityTimer = window.setInterval(() => reportActivity(true),15000);
    return () => {
      live = false; window.clearInterval(activityTimer);
      document.removeEventListener("input", activity, true); document.removeEventListener("pointerdown", activity);
      document.removeEventListener("visibilitychange", visibility); window.removeEventListener("focus", focus); window.removeEventListener("blur", blur);
      transport.dispose(); draftPresenceRef.current = null; setDraftPresence(null); setDraftPeerCursors([]);
      if (peerTransportRef.current === transport) peerTransportRef.current = null;
    };
  }, [loading, spaceId, docId, activeDraft?.id, activeDraft?.access_role, user?.id, requestedDraftId, peerReconnect, pageContextVersion, suspendAutoSave]);
  useEffect(() => {
    const owns = (target: EventTarget | null) => target instanceof Node && (documentBodyRef.current?.contains(target) || titleRef.current === target);
    const owner = {};
    const release = () => {
      if (peerCompositionOwnerRef.current !== owner) return false;
      peerCompositionOwnerRef.current = null; peerComposingRef.current = false; return true;
    };
    const start = (event: CompositionEvent) => { if (owns(event.target)) { peerCompositionOwnerRef.current = owner; peerComposingRef.current = true; peerEditEpochRef.current += 1; } };
    const end = (event: CompositionEvent) => { if (owns(event.target) && release()) peerTransportRef.current?.resync(); };
    document.addEventListener("compositionstart", start, true); document.addEventListener("compositionend", end, true);
    return () => { document.removeEventListener("compositionstart", start, true); document.removeEventListener("compositionend", end, true); release(); };
  }, [spaceId, docId, activeDraft?.id]);

  // Observe retained queue state; online, unlock and storage events only refresh
  // the list. They never dispatch a held save.
  useEffect(() => {
    const refresh = () => {
      try {
        if (selectedDraftContextRef.current) verifyDraftSaveSelection(selectedDraftContextRef.current,recoveryAccountRef.current);
        setHeldQueueItems(spaceId && docId && getCurrentLaunchMode() === "server" ? heldDraftQueueService.inspect(spaceId,docId) : []);
        const selection=selectedDraftContextRef.current ?? captureDraftSaveSelection(recoveryAccountRef.current);
        try{setQueuedCopyRecords(selection && spaceId && docId && selection.mode==="server" ? listQueuedDraftCopyReviews(localStorage,selection,spaceId,docId) : []);}
        catch(error){setQueuedCopyRecords([]);setHeldQueueError(error instanceof Error?error.message:"Retained proposal inventory is unavailable. Keep its evidence.");}
      } catch { setHeldQueueItems([]); setQueuedCopyRecords([]); }
    };
    refresh();
    window.addEventListener(OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT,refresh);
    window.addEventListener("storage",refresh); window.addEventListener("online",refresh); window.addEventListener("focus",refresh);
    return () => { window.removeEventListener(OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT,refresh); window.removeEventListener("storage",refresh); window.removeEventListener("online",refresh); window.removeEventListener("focus",refresh); };
  },[spaceId,docId,user?.id,activeDraft?.id,pageContextVersion]);
  useEffect(() => {
    heldQueueOperationRef.current = null; heldQueueBusyRef.current = false;
    setHeldQueueBusy(false); setHeldQueueReview(null); setQueuedCopyReview(null); setQueuedCopyOperation(null); setQueuedCopyTargetReview(null); setQueuedCopyRestartIntent(null); setHeldQueueError("");
  },[spaceId,docId,user?.id,activeDraft?.id,pageContextVersion]);
  const heldReviewContextCurrent = (review:Omit<NonNullable<typeof heldQueueReview>,"review">,checkEdits = true) => {
    verifyDraftSaveSelection(review.selection,recoveryAccountRef.current);
    if (routeIdentityRef.current !== review.route || draftSelectionGenerationRef.current !== review.generation || (confirmedDraftRef.current?.id ?? null) !== review.draftId) throw new Error("Original page or draft selection changed. Keep the queued request and review it again.");
    if (checkEdits && (JSON.stringify(currentSaveStateRef.current) !== review.canvas || (review.scope ? readDocumentDraftRecovery(review.scope)?.snapshotId ?? null : null) !== review.checkpointId)) throw new Error("Device edits changed during review. Keep the newest checkpoint and review the original queued save again.");
  };
  const downloadHeldQueueRequest = (item:HeldDraftQueueItem) => {
    const url=URL.createObjectURL(new Blob([JSON.stringify(item,null,2)],{type:"application/json"}));
    const link=document.createElement("a");link.href=url;link.download=`original-queued-save-${item.id}.json`;link.click();URL.revokeObjectURL(url);
  };
  const reviewHeldQueueSave = async (id:string) => {
    if (!spaceId || !docId || heldQueueBusyRef.current || recoveryBusyRef.current || publicationPendingRef.current) return;
    let selection:DraftSaveSelection;
    try{selection=selectedDraftContextRef.current ?? captureDraftSaveSelection(recoveryAccountRef.current);}catch{setHeldQueueError("Open the original draft account before reviewing its queued save.");return;}
    const scope=getRecoveryScope(confirmedDraftRef.current,selection);
    const context={selection,route:`${spaceId}/${docId}`,generation:draftSelectionGenerationRef.current,draftId:confirmedDraftRef.current?.id ?? null,canvas:JSON.stringify(currentSaveStateRef.current),scope,checkpointId:scope ? readDocumentDraftRecovery(scope)?.snapshotId ?? null : null};
    const operation=Symbol("held-save-review");heldQueueOperationRef.current=operation;heldQueueBusyRef.current=true;setHeldQueueBusy(true);setHeldQueueError("");
    // Suspend this canvas, even if the server has since unlocked. Existing local
    // edits stay separate from the immutable original queue request.
    resetAutoSave(currentSaveStateRef.current.content);
    draftSaveHoldRef.current=draftSaveHoldRef.current ?? new DraftSaveHeldError("queued","Original queued work requires deliberate review before saving again.");
    try {
      await draftSaveTailRef.current;
      heldReviewContextCurrent(context);
      const review=await heldDraftQueueService.prepare(id);
      heldReviewContextCurrent(context);
      if (heldQueueOperationRef.current===operation) setHeldQueueReview({...context,review});
    } catch(error) {
      try { heldReviewContextCurrent(context,false); if(heldQueueOperationRef.current===operation)setHeldQueueError(error instanceof Error ? error.message : "Original queued save cannot be reviewed. Keep its request."); } catch { /* Stale account/route responses do not enter this canvas. */ }
    } finally { if(heldQueueOperationRef.current===operation){heldQueueBusyRef.current=false;setHeldQueueBusy(false);} }
  };
  const retryHeldQueueSave = async () => {
    const reviewed=heldQueueReview;
    if(!reviewed || heldQueueBusyRef.current || recoveryBusyRef.current || publicationPendingRef.current)return;
    let originalConfirmed=false;
    const operation=Symbol("held-save-retry");heldQueueOperationRef.current=operation;heldQueueBusyRef.current=true;setHeldQueueBusy(true);setHeldQueueError("");
    try {
      heldReviewContextCurrent(reviewed); await draftSaveTailRef.current; heldReviewContextCurrent(reviewed);
      await heldDraftQueueService.resume(reviewed.review); originalConfirmed=true; heldReviewContextCurrent(reviewed);
      // Only independently current selected source can enter the editor. The
      // historical HTTP acknowledgement itself is never a current baseline.
      const original=reviewed.review.original;
      const path=/^\/spaces\/([^/]+)\/documents\/([^/]+)\/drafts\/([^/]+)$/.exec(original.path);
      if(!path)throw new Error("Original save path is unavailable.");
      const fresh=await dataClient.getCurrentDocumentDraftEditSource(path[1],path[2],path[3],reviewed.selection);
      heldReviewContextCurrent(reviewed);
      setHeldQueueItems(heldDraftQueueService.inspect(path[1],path[2]));setHeldQueueReview(null);
      const checkpoint=reviewed.scope ? readDocumentDraftRecovery(reviewed.scope) : null;
      const saved=fresh.selected_draft;
      const remaining=listOfflineServerMutations(reviewed.selection.serverBase).some(item=>item.path===original.path);
      const canvas=currentSaveStateRef.current;
      const matchesCanvas=(normalizeDocumentTitleInput(canvas.title)||"Untitled")===saved.title && canvasContentMatchesSaved(canvas,saved.content) && (canvas.icon??"")===(saved.icon??"") && (canvas.coverImage??"")===(saved.cover_image??"");
      const matchesCheckpoint=!checkpoint || recoveryCheckpointMatchesSaved(checkpoint,saved);
      if(saved.id!==reviewed.draftId || remaining || !matchesCanvas || !matchesCheckpoint){
        setDraftError("The original queued save was confirmed. Current device edits remain held; review them against the freshly loaded draft before saving.");
        if(checkpoint)setRecoveryCandidate(checkpoint);
        return;
      }
      if(checkpoint && reviewed.scope && !clearDocumentDraftRecovery(reviewed.scope,checkpoint.snapshotId,"confirmed"))throw new Error("The original save is confirmed, but its device checkpoint could not be cleared safely. Keep it for review.");
      // Exact current canvas is already stored. No save or replay is scheduled.
      setActiveDraft(saved,true);draftSaveHoldRef.current=null;setDraftHasChanges(false);setSaveQueued(false);setDraftError("");setRecoveryCandidate(null);
      setDoc(current=>current ? {...current,locked:fresh.locked,locked_by:fresh.locked_by??undefined,page_edit_allowed:true,updated_at:fresh.page_updated_at} : current);
      resetAutoSave(canvas.content);
      setBirthStatus({route:reviewed.route,message:"Original queued save confirmed · unpublished. No later queued work was sent."});
    } catch(error) {
      try { heldReviewContextCurrent(reviewed,false); if(heldQueueOperationRef.current===operation){const message=error instanceof Error ? error.message : "Live draft review is unavailable.";
          if(originalConfirmed){setHeldQueueReview(null);setDraftError(`Original queued save confirmed · current device work remains held. ${message}`);if(reviewed.scope)setRecoveryCandidate(readDocumentDraftRecovery(reviewed.scope));}
          else setHeldQueueError(message);
          setHeldQueueItems(heldDraftQueueService.inspect(spaceId!,docId!));} } catch { /* Original queue evidence remains scoped to its account. */ }
    } finally {if(heldQueueOperationRef.current===operation){heldQueueBusyRef.current=false;setHeldQueueBusy(false);}}
  };

  const openQueuedCopyReview = (record:QueuedCopyReviewRecord) => {
    const selection=selectedDraftContextRef.current ?? captureDraftSaveSelection(recoveryAccountRef.current);
    const scope=getRecoveryScope(confirmedDraftRef.current,selection);
    const context={selection,route:`${spaceId}/${docId}`,generation:draftSelectionGenerationRef.current,draftId:confirmedDraftRef.current?.id??null,canvas:JSON.stringify(currentSaveStateRef.current),scope,checkpointId:scope?readDocumentDraftRecovery(scope)?.snapshotId??null:null};
    heldReviewContextCurrent(context);
    verifyDraftSaveSelection(record.intent.selection,recoveryAccountRef.current);
    const operation=recoveryActions(()=>heldReviewContextCurrent(context)).inspectCopy(record.intent);
    setQueuedCopyRestartIntent(null);setQueuedCopyTargetReview(null);setQueuedCopyOperation(operation);setQueuedCopyReview({record,context});setHeldQueueReview(null);setHeldQueueError("");
  };
  const prepareQueuedRecoveryCopy = async () => {
    const reviewed=heldQueueReview;if(!reviewed||heldQueueBusyRef.current)return;
    const token=Symbol("queued-copy-proposal");heldQueueOperationRef.current=token;heldQueueBusyRef.current=true;setHeldQueueBusy(true);setHeldQueueError("");
    try {
      heldReviewContextCurrent(reviewed);
      // Independent proposal storage never rewrites the ordinary device checkpoint.
      const previous=readQueuedDraftCopyReview(localStorage,reviewed.review.original,reviewed.selection);
      const record=previous??await persistQueuedDraftCopyReview(localStorage,prepareQueuedDraftCopy(reviewed.review,reviewed.selection),reviewed.selection);
      heldReviewContextCurrent(reviewed);
      if(heldQueueOperationRef.current!==token)return;
      openQueuedCopyReview(record);setQueuedCopyRecords(listQueuedDraftCopyReviews(localStorage,reviewed.selection,spaceId!,docId!));
    } catch(error){try{heldReviewContextCurrent(reviewed,false);if(heldQueueOperationRef.current===token)setHeldQueueError(error instanceof Error?error.message:"Keep the original recovery proposal.");}catch{/* Keep late proposal evidence without entering another canvas. */}}
    finally{if(heldQueueOperationRef.current===token){heldQueueBusyRef.current=false;setHeldQueueBusy(false);}}
  };
  const queuedCopyCurrentIntent = (record:QueuedCopyReviewRecord,actions:ReturnType<typeof recoveryActions>) => {
    const retained=actions.inspectCopy(record.intent)?.intent ?? record.intent;
    if(JSON.stringify(retained.patch)!==JSON.stringify(record.intent.patch) || JSON.stringify(retained.scope)!==JSON.stringify(record.intent.scope) || JSON.stringify(retained.checkpoint)!==JSON.stringify(record.intent.checkpoint) || JSON.stringify(retained.selection)!==JSON.stringify(record.intent.selection))throw new Error("Retained recovery operation differs from its original raw proposal. Keep all evidence; no new request is allowed.");
    return retained;
  };
  const createQueuedRecoveryCopy = async () => {
    const reviewed=queuedCopyReview;if(!reviewed||heldQueueBusyRef.current||recoveryBusyRef.current||publicationPendingRef.current)return;
    const token=Symbol("queued-copy-dispatch");heldQueueOperationRef.current=token;heldQueueBusyRef.current=true;setHeldQueueBusy(true);setHeldQueueError("");
    const verify=()=>{heldReviewContextCurrent(reviewed.context);verifyDraftSaveSelection(reviewed.record.intent.selection,recoveryAccountRef.current);};
    const actions=recoveryActions(verify);
    try {
      verify();await draftSaveTailRef.current;verify();
      if(!currentPageCapabilityRef.current().canEditPage)throw new Error(pageEditBlockedExplanation(currentPageCapabilityRef.current().blockedReason));
      const intent=queuedCopyCurrentIntent(reviewed.record,actions),previous=actions.inspectCopy(intent);
      if(previous?.state==="birth-pending"||previous?.state==="birth-held")await actions.retryCopy(intent);
      else await actions.copy(intent);
      verify();if(heldQueueOperationRef.current===token)setQueuedCopyOperation(actions.inspectCopy(reviewed.record.intent));
      // No editor activation, checkpoint removal, queue ACK or publication.
    }catch(error){try{heldReviewContextCurrent(reviewed.context,false);if(heldQueueOperationRef.current===token){setQueuedCopyOperation(actions.inspectCopy(reviewed.record.intent));setHeldQueueError(error instanceof Error?error.message:"Recovery outcome needs review. Keep the original operation.");}}catch{/* Durable L1 operation evidence survives late context changes. */}}
    finally{if(heldQueueOperationRef.current===token){heldQueueBusyRef.current=false;setHeldQueueBusy(false);}}
  };
  const reviewRejectedQueuedCopyBirth = async () => {
    const reviewed=queuedCopyReview;if(!reviewed||heldQueueBusyRef.current)return;
    const token=Symbol("queued-copy-rejected-birth-review");heldQueueOperationRef.current=token;heldQueueBusyRef.current=true;setHeldQueueBusy(true);setHeldQueueError("");
    const actions=recoveryActions(()=>heldReviewContextCurrent(reviewed.context));
    try{
      heldReviewContextCurrent(reviewed.context);
      const original=queuedCopyCurrentIntent(reviewed.record,actions),scope=original.scope;
      const current=await dataClient.getCurrentDocumentDraftEditSource(scope.spaceId,scope.documentId,scope.draftId,original.selection);
      heldReviewContextCurrent(reviewed.context);
      const intent={...original,reviewedDraft:current.selected_draft,published:current.page_published};
      // Only existing exact rejected-birth proof can authorize archiving/restart.
      if(!actions.canRestartCopy(intent))throw new Error("The original birth is not proven rejected. Keep its operation; an uncertain birth cannot be restarted.");
      if(heldQueueOperationRef.current===token)setQueuedCopyRestartIntent(intent);
    }catch(error){try{heldReviewContextCurrent(reviewed.context,false);if(heldQueueOperationRef.current===token)setHeldQueueError(error instanceof Error?error.message:"Rejected birth cannot be reviewed.");}catch{/* Original evidence remains retained. */}}
    finally{if(heldQueueOperationRef.current===token){heldQueueBusyRef.current=false;setHeldQueueBusy(false);}}
  };
  const restartReviewedQueuedCopyBirth = async () => {
    const reviewed=queuedCopyReview,intent=queuedCopyRestartIntent;if(!reviewed||!intent||heldQueueBusyRef.current||publicationPendingRef.current)return;
    const token=Symbol("queued-copy-rejected-birth-restart");heldQueueOperationRef.current=token;heldQueueBusyRef.current=true;setHeldQueueBusy(true);setHeldQueueError("");
    const actions=recoveryActions(()=>heldReviewContextCurrent(reviewed.context));
    try{
      heldReviewContextCurrent(reviewed.context);await draftSaveTailRef.current;heldReviewContextCurrent(reviewed.context);
      if(!actions.canRestartCopy(intent))throw new Error("Rejected birth proof changed; no restart was sent.");
      await actions.restartCopy(intent);heldReviewContextCurrent(reviewed.context);
      if(heldQueueOperationRef.current===token){setQueuedCopyRestartIntent(null);setQueuedCopyOperation(actions.inspectCopy(intent));}
    }catch(error){try{heldReviewContextCurrent(reviewed.context,false);if(heldQueueOperationRef.current===token){setQueuedCopyRestartIntent(null);setQueuedCopyOperation(actions.inspectCopy(intent));setHeldQueueError(error instanceof Error?error.message:"Original restarted operation needs review.");}}catch{/* Archive and new original operation survive late context changes. */}}
    finally{if(heldQueueOperationRef.current===token){heldQueueBusyRef.current=false;setHeldQueueBusy(false);}}
  };
  const reviewQueuedCopyTarget = async () => {
    const reviewed=queuedCopyReview,target=queuedCopyOperation?.target;
    if(!reviewed||!target||heldQueueBusyRef.current)return;
    const token=Symbol("queued-copy-target-review");heldQueueOperationRef.current=token;heldQueueBusyRef.current=true;setHeldQueueBusy(true);setHeldQueueError("");
    try{
      heldReviewContextCurrent(reviewed.context);
      const current=await dataClient.getCurrentDocumentDraftEditSource(target.space_id,target.document_id,target.id,reviewed.record.intent.selection);
      heldReviewContextCurrent(reviewed.context);
      if(heldQueueOperationRef.current===token)setQueuedCopyTargetReview(current.selected_draft);
    }catch(error){try{heldReviewContextCurrent(reviewed.context,false);if(heldQueueOperationRef.current===token)setHeldQueueError(error instanceof Error?error.message:"Current recovery target is unavailable.");}catch{/* Original target stays retained. */}}
    finally{if(heldQueueOperationRef.current===token){heldQueueBusyRef.current=false;setHeldQueueBusy(false);}}
  };
  const replaceQueuedCopyTarget = async () => {
    const reviewed=queuedCopyReview,target=queuedCopyTargetReview;
    if(!reviewed||!target||heldQueueBusyRef.current||publicationPendingRef.current)return;
    const token=Symbol("queued-copy-target-replace");heldQueueOperationRef.current=token;heldQueueBusyRef.current=true;setHeldQueueBusy(true);setHeldQueueError("");
    const actions=recoveryActions(()=>heldReviewContextCurrent(reviewed.context));
    try{
      heldReviewContextCurrent(reviewed.context);await draftSaveTailRef.current;heldReviewContextCurrent(reviewed.context);
      await actions.replaceCopyTarget(queuedCopyCurrentIntent(reviewed.record,actions),target);heldReviewContextCurrent(reviewed.context);
      if(heldQueueOperationRef.current===token){setQueuedCopyOperation(actions.inspectCopy(reviewed.record.intent));setQueuedCopyTargetReview(null);}
    }catch(error){try{heldReviewContextCurrent(reviewed.context,false);if(heldQueueOperationRef.current===token){setQueuedCopyOperation(actions.inspectCopy(reviewed.record.intent));setHeldQueueError(error instanceof Error?error.message:"Recovery target outcome needs review.");}}catch{/* Retained operation survives context change. */}}
    finally{if(heldQueueOperationRef.current===token){heldQueueBusyRef.current=false;setHeldQueueBusy(false);}}
  };
  const downloadQueuedCopy = (record:QueuedCopyReviewRecord) => {
    const url=URL.createObjectURL(new Blob([JSON.stringify(record,null,2)],{type:"application/json"}));const link=document.createElement("a");link.href=url;link.download=`queued-recovery-proposal-${record.original.id}.json`;link.click();URL.revokeObjectURL(url);
  };

  const retainedOriginalSave = () => {
    if (!recoveryScope || recoveryScope.storageMode !== "server") return false;
    const path = `/spaces/${recoveryScope.spaceId}/documents/${recoveryScope.documentId}/drafts/${recoveryScope.draftId}`;
    return listOfflineServerMutations(recoveryScope.serverTarget).some((entry) => entry.path === path && (!entry.origin || entry.origin.accountId === recoveryScope.authIdentity));
  };
  const retainedTargetSave = (target: DraftRecord) => {
    if (!recoveryScope || recoveryScope.storageMode !== "server") return false;
    const path = `/spaces/${target.space_id}/documents/${target.document_id}/drafts/${target.id}`;
    return listOfflineServerMutations(recoveryScope.serverTarget).some((entry) => entry.path === path && (!entry.origin || entry.origin.accountId === recoveryScope.authIdentity));
  };
  const recoveryActions = (verify: () => void) => createDocumentDraftRecoveryActions({ storage: localStorage,
    verifySelection: verify,
    createDraft: (space, document, options, prepared) => dataClient.createDocumentDraft(space, document, options, prepared),
    retryBirth: (space, document, operationId) => dataClient.retryDocumentDraftBirth(space, document, operationId),
    inspectBirth: (space, document, operationId) => dataClient.inspectDocumentDraftBirth(space, document, operationId),
    saveDraft: (space, document, draft, patch, digest) => dataClient.updateDocumentDraft(space, document, draft, patch, digest),
    hasPendingOriginalSave: (intent) => intent.scope.storageMode === "server" && listOfflineServerMutations(intent.scope.serverTarget).some(entry => entry.path === `/spaces/${intent.scope.spaceId}/documents/${intent.scope.documentId}/drafts/${intent.scope.draftId}`),
    // A page-level retained proposal has no ordinary selected checkpoint scope.
    // The operation's own target and server bind the uncertainty barrier.
    hasPendingTargetSave: (target,intent) => intent.scope.storageMode === "server" && listOfflineServerMutations(intent.scope.serverTarget).some(entry => entry.path === `/spaces/${target.space_id}/documents/${target.document_id}/drafts/${target.id}`),
  });
  const recoveryIntent = (scope: DocumentDraftRecoveryScope, checkpoint: DocumentDraftRecoverySnapshot, reviewedDraft: DraftRecord, selection: DraftSaveSelection): DraftRecoveryIntent => ({ scope, checkpoint, reviewedDraft, published: selectedDraftOnly ? selectedDraftPagePublished : Boolean(doc?.published_at), selection,
    patch: { title: normalizeDocumentTitleInput(checkpoint.title) || "Untitled", content: serializeRecoveryCheckpoint(checkpoint),
      icon: checkpoint.icon !== undefined ? checkpoint.icon ?? "" : reviewedDraft.icon ?? "", cover_image: checkpoint.coverImage !== undefined ? checkpoint.coverImage ?? "" : reviewedDraft.cover_image ?? "", },
    holdReason: retainedOriginalSave() ? "outcome_unknown" : draftSaveHoldRef.current?.reason,
  });
  const inspectRecoveryCopy = async (loaded: DraftRecord, selection: DraftSaveSelection, verify: () => void) => {
    if (!recoveryScope || !recoveryCandidate) return;
    setRecoveryRestartReady(false); setRecoveryTargetReview(null); setRecoveryOriginalIntent(null); setRecoveryTargetHeld(false);
    const compositionHold = recoveryCompositionHoldReason(recoveryCandidate);
    if (compositionHold) { verify(); setRecoveryReviewError(compositionHold); return; }
    const actions = recoveryActions(verify);
    const intent = recoveryIntent(recoveryScope, recoveryCandidate, loaded, selection);
    const previous = actions.inspectCopy(intent);
    setRecoveryRestartReady(Boolean(previous && actions.canRestartCopy(intent)));
    setRecoveryTargetReview(null); setRecoveryOriginalIntent(null); setRecoveryTargetHeld(false);
    if (previous?.target && (previous.state === "save-rejected" || previous.state === "save-held" || previous.state === "save-pending")) {
      const target = selectedDraftOnly
        ? (await dataClient.getCurrentDocumentDraftEditSource(recoveryScope.spaceId,recoveryScope.documentId,previous.target.id,selection)).selected_draft
        : await dataClient.getDocumentDraft(recoveryScope.spaceId, recoveryScope.documentId, previous.target.id);
      verify();
      if (target.id !== previous.target.id || target.space_id !== recoveryScope.spaceId || target.document_id !== recoveryScope.documentId || target.access_role !== "owner" || target.owner_id !== recoveryScope.authIdentity) throw new Error("Recovery target is unavailable or no longer owned. Keep the checkpoint and original operation evidence.");
      setRecoveryTargetReview(target); setRecoveryOriginalIntent(previous.intent);
      setRecoveryTargetHeld(previous.state !== "save-rejected" || retainedTargetSave(target));
    }
  };
  const recoveryReviewContent = (draft: DraftRecord) => {
    const parsed = parseDocumentContent(draft.content, draft.title);
    return { title: draft.title, ...parsed, content: parsed.body, customCss: parsed.customCss ?? "", icon: draft.icon ?? null, coverImage: draft.cover_image ?? null };
  };
  const downloadOriginalSavedDraft = (intent: DraftPublicationIntent) => {
    const url = URL.createObjectURL(new Blob([JSON.stringify(exportOriginalSavedDraft(intent), null, 2)], { type: "application/json" }));
    const link = document.createElement("a");
    link.href = url; link.download = `original-saved-draft-${intent.operation_id}.json`;
    link.click(); URL.revokeObjectURL(url);
  };
  const downloadRecovery = () => {
    if (!recoveryCandidate) return;
    const url = URL.createObjectURL(new Blob([JSON.stringify(recoveryCandidate, null, 2)], { type: "application/json" }));
    const link = document.createElement("a");
    link.href = url; link.download = `retained-draft-${recoveryCandidate.snapshotId}.json`;
    link.click(); URL.revokeObjectURL(url);
  };
  const downloadCurrentDeviceCopy = () => {
    const source = confirmedDraftRef.current;
    const payload = { format: "starlane-device-draft-copy", schemaVersion: 1, persisted: false, capturedAt: new Date().toISOString(),
      scope: recoveryScope, canvas: currentSaveStateRef.current,
      baseDraftSource: source ? { draftId: source.id, sourceDigest: source.source_digest, title: source.title, content: source.content, icon: source.icon ?? null, coverImage: source.cover_image ?? null } : null,
      peerDraft: peerSnapshotRef.current?.draft ?? null };
    const url = URL.createObjectURL(new Blob([JSON.stringify(payload, null, 2)], {type:"application/json"}));
    const link = document.createElement("a"); link.href = url; link.download = "unsaved-draft-device-copy.json"; link.click(); URL.revokeObjectURL(url);
  };
  const reviewRecovery = async () => {
    if (!spaceId || !docId || !recoveryScope || !recoveryCandidate || recoveryBusyRef.current) return;
    const selection = selectedDraftContextRef.current;
    const generation = draftSelectionGenerationRef.current;
    const route = `${spaceId}/${docId}`;
    const snapshotId = recoveryCandidate.snapshotId;
    recoveryBusyRef.current = true; setRecoveryBusy(true); setRecoveryReviewError("");
    setRecoveryRestartReady(false); setRecoveryTargetReview(null); setRecoveryOriginalIntent(null); setRecoveryTargetHeld(false);
    const verify = () => {
      if (!selection) throw new Error("Review the original draft session before recovering.");
      verifyDraftSaveSelection(selection, recoveryAccountRef.current);
      if (routeIdentityRef.current !== route || draftSelectionGenerationRef.current !== generation || readDocumentDraftRecovery(recoveryScope)?.snapshotId !== snapshotId) throw new Error("Selection or retained edits changed. Review the newest device copy again.");
    };
    try {
      // Wait for existing writes; reviewing recovery must not dispatch an autosave.
      await draftSaveTailRef.current; verify();
      const loaded = selectedDraftOnly && selection
        ? (await dataClient.getCurrentDocumentDraftEditSource(spaceId,docId,recoveryScope.draftId,selection)).selected_draft
        : await dataClient.getDocumentDraft(spaceId, docId, recoveryScope.draftId);
      verify();
      if (loaded.id !== recoveryScope.draftId || loaded.space_id !== spaceId || loaded.document_id !== docId || loaded.access_role === "viewer") throw new Error("This draft is no longer editable. Download your device copy or review access before recovering.");
      await inspectRecoveryCopy(loaded, selection!, verify); verify();
      setRecoveryReviewUnavailable(false); setRecoveryReviewDraft(loaded); setRecoveryReviewOpen(true);
    } catch (error) {
      setRecoveryReviewError(error instanceof Error ? error.message : "Could not review this draft. Device edits remain retained.");
      try {
        verify();
        const base = recoveryCandidate.baseDraftSource;
        // A missing draft never grants source authority. Published-page birth rechecks
        // current write access; unpublished recovery needs an accessible editable source.
        const unavailable: DraftRecord = { id: recoveryScope.draftId, document_id: docId, space_id: spaceId, owner_id: recoveryScope.authIdentity, owner_name: "Unavailable", document_title: doc?.title ?? "", access_role: "viewer", can_publish: false, title: base?.title ?? "Original source unavailable", content: base?.content ?? "", icon: base?.icon ?? undefined, cover_image: base?.coverImage ?? undefined, source_digest: base?.sourceDigest, created_at: recoveryCandidate.capturedAt, updated_at: recoveryCandidate.baseDraftUpdatedAt, base_updated_at: doc?.updated_at ?? recoveryCandidate.baseDraftUpdatedAt };
        await inspectRecoveryCopy(unavailable, selection!, verify); verify();
        setRecoveryReviewDraft(unavailable);
        setRecoveryReviewUnavailable(true); setRecoveryReviewOpen(true);
      } catch { /* Changed selection cannot reopen another context's checkpoint. */ }
    }
    finally { recoveryBusyRef.current = false; setRecoveryBusy(false); }
  };
  const reviewLivePeerChanges = () => {
    cancelPeerFocusRestore();
    const trigger = peerReviewTriggerRef.current;
    peerReviewReturnRef.current = trigger ? {trigger,route:routeIdentityRef.current,generation:draftSelectionGenerationRef.current} : null;
    const original = confirmedDraftRef.current, selection = selectedDraftContextRef.current;
    if (!original || !selection || peerPendingSavesRef.current || peerComposingRef.current || recoveryBusyRef.current) return;
    try {
      verifyDraftSaveSelection(selection, recoveryAccountRef.current);
      const scope = getRecoveryScope(original, selection);
      if (!scope) throw new Error("Original draft recovery scope is unavailable. Download the device copy.");
      checkpointDraft({});
      const checkpoint = readDocumentDraftRecovery(scope);
      const current = currentSaveStateRef.current;
      if (!checkpoint || JSON.stringify({title:checkpoint.title,content:checkpoint.content,synopsis:checkpoint.synopsis,pageWidth:checkpoint.pageWidth,customCss:checkpoint.customCss,cssProvenance:checkpoint.cssProvenance,icon:checkpoint.icon ?? null,coverImage:checkpoint.coverImage ?? null}) !== JSON.stringify(current)) {
        throw new Error("This device could not retain the exact current proposal for review. Keep typing here or download the unsaved device copy.");
      }
      peerBarrierRef.current = true; suspendAutoSave(); peerReviewActiveRef.current = true;
      setRecoveryCandidate(checkpoint); setPeerReviewRequested(true);
    } catch (error) { setDraftError(error instanceof Error ? error.message : "Could not retain the current proposal. Download the device copy."); }
  };
  const peerReviewRunnerRef = useRef(reviewRecovery);
  peerReviewRunnerRef.current = reviewRecovery;
  useEffect(() => {
    if (!peerReviewRequested || !recoveryCandidate) return;
    setPeerReviewRequested(false); void peerReviewRunnerRef.current();
  }, [peerReviewRequested, recoveryCandidate]);
  const closeRecoveryReview = () => {
    if (recoveryBusyRef.current) return;
    if (peerReviewActiveRef.current) {
      const restore = peerReviewReturnRef.current;
      cancelPeerFocusRestore();
      flushSync(() => {
        setRecoveryReviewOpen(false); peerReviewActiveRef.current = false;
        setRecoveryCandidate(null); setRecoveryReviewDraft(null); setRecoveryReviewError("");
      });
      // The shared overlay cleanup restores its captured opener on the next
      // frame. Restore our original pre-async trigger after that cleanup.
      const first = window.requestAnimationFrame(() => {
        const second = window.requestAnimationFrame(() => {
          peerFocusFramesRef.current = [];
          if (restore && peerReviewReturnRef.current === restore && routeIdentityRef.current === restore.route && draftSelectionGenerationRef.current === restore.generation && restore.trigger.isConnected && !restore.trigger.disabled && !document.querySelector('[role="dialog"]')) restore.trigger.focus({preventScroll:true});
        });
        peerFocusFramesRef.current = [second];
      });
      peerFocusFramesRef.current = [first];
      // The durable checkpoint stays intact. Continued typing still checkpoints
      // against the original source while autosave remains suspended.
    } else setRecoveryReviewOpen(false);
  };
  const replacementHistoryRefreshRef = useRef<() => void>(() => {});
  replacementHistoryRefreshRef.current = () => {
    const original = confirmedDraftRef.current, selection = selectedDraftContextRef.current;
    if (!original || !selection) { setReplacementRecords([]); return; }
    try {
      verifyDraftSaveSelection(selection, recoveryAccountRef.current);
      const scope = getRecoveryScope(original, selection);
      setReplacementRecords(scope ? recoveryActions(() => verifyDraftSaveSelection(selection,recoveryAccountRef.current)).listReplacements(scope,selection) : []);
      setReplacementHistoryError("");
    } catch { setReplacementRecords([]); setReplacementHistoryError("Retained replacement history could not be read in this selection. Keep its device records."); }
  };
  useEffect(() => {
    const refresh = () => replacementHistoryRefreshRef.current();
    setReplacementArchive(null); refresh(); window.addEventListener("storage",refresh); window.addEventListener("focus",refresh);
    return () => { window.removeEventListener("storage",refresh); window.removeEventListener("focus",refresh); };
  },[spaceId,docId,activeDraft?.id,user?.id,pageContextVersion]);
  const verifyReplacementRecord = (record: DraftRecoveryReplacementRecord) => {
    const selection = selectedDraftContextRef.current, original = confirmedDraftRef.current;
    const scope = getRecoveryScope(original, selection);
    if (!selection || !scope || JSON.stringify(scope) !== JSON.stringify(record.intent.scope) || JSON.stringify(selection) !== JSON.stringify(record.intent.selection)) throw new Error("Open the original draft selection before inspecting its retained sources.");
    verifyDraftSaveSelection(selection,recoveryAccountRef.current);
    return selection;
  };
  const inspectPreviousReplacement = (record: DraftRecoveryReplacementRecord) => {
    try {
      const selection = verifyReplacementRecord(record);
      if (!record.previousArchiveKey) return;
      // One bounded device-record lookup per explicit action; no recursive scan
      // or inaccessible chain of pointers masquerading as downloaded evidence.
      const previous = recoveryActions(() => verifyDraftSaveSelection(selection,recoveryAccountRef.current)).inspectReplacement(record.intent,record.previousArchiveKey);
      if (!previous) throw new Error("The earlier retained source record is unavailable. Keep the current record.");
      setReplacementArchive(previous); setReplacementHistoryError("");
    } catch (error) { setReplacementHistoryError(error instanceof Error ? error.message : "Earlier retained sources could not be read."); }
  };
  const downloadReplacementRecord = (record: DraftRecoveryReplacementRecord) => {
    try { verifyReplacementRecord(record); }
    catch { setReplacementHistoryError("Open the original draft account before downloading this retained record."); return; }
    const url = URL.createObjectURL(new Blob([JSON.stringify(record,null,2)], {type:"application/json"}));
    const link = document.createElement("a"); link.href = url; link.download = `reviewed-draft-replacement-${record.intent.checkpoint.snapshotId}.json`; link.click(); URL.revokeObjectURL(url);
  };

  const recoverReviewedDraft = async (action: "copy" | "replace", restart = false) => {
    if (!spaceId || !docId || !recoveryScope || !recoveryCandidate || !recoveryReviewDraft || recoveryBusyRef.current) return;
    const compositionHold = recoveryCompositionHoldReason(recoveryCandidate);
    if (compositionHold) { setRecoveryReviewError(compositionHold); return; }
    const scope = { ...recoveryScope }, checkpoint = structuredClone(recoveryCandidate), reviewedDraft = structuredClone(recoveryReviewDraft);
    const selection = selectedDraftContextRef.current;
    const generation = draftSelectionGenerationRef.current, route = `${spaceId}/${docId}`;
    if (!selection) { setRecoveryReviewError("Original draft session is unavailable. Keep the device copy."); return; }
    recoveryBusyRef.current = true; setRecoveryBusy(true); setRecoveryReviewError("");
    const verify = () => {
      verifyDraftSaveSelection(selection, recoveryAccountRef.current);
      if (routeIdentityRef.current !== route || draftSelectionGenerationRef.current !== generation || readDocumentDraftRecovery(scope)?.snapshotId !== checkpoint.snapshotId) throw new Error("Selection or retained edits changed. The operation evidence and newer edits remain on this device.");
    };
    try {
      await draftSaveTailRef.current; verify();
      if (!currentPageCapabilityRef.current().canEditPage) throw new Error(pageEditBlockedExplanation(currentPageCapabilityRef.current().blockedReason));
      if (recoveryReviewUnavailable && !recoveryTargetReview && (action === "replace" || !(selectedDraftOnly ? selectedDraftPagePublished : doc?.published_at))) throw new Error("The original draft is unavailable. Keep or download the device copy; unpublished recovery needs an accessible editable source.");
      const actions = recoveryActions(verify);
      const intent = recoveryIntent(scope, checkpoint, reviewedDraft, selection);
      const previous = action === "copy" ? actions.inspectCopy(intent) : null;
      const originalIntent = recoveryOriginalIntent ?? previous?.intent ?? intent;
      const saved = action === "replace" ? recoveryTargetReview ? await actions.replaceCopyTarget(originalIntent, recoveryTargetReview) : await actions.replace(intent) : restart ? await actions.restartCopy(intent) : previous?.state === "birth-held" || previous?.state === "birth-pending" ? await actions.retryCopy(originalIntent) : await actions.copy(originalIntent);
      verify();
      if (action === "copy" || recoveryTargetReview) actions.finishCopy(restart ? intent : originalIntent, saved);
      const parsed = parseDocumentContent(saved.content, saved.title);
      if (!clearDocumentDraftRecovery(scope, checkpoint.snapshotId, "confirmed")) throw new Error("Recovery save was confirmed, but the device checkpoint changed or could not be cleared. Its evidence is retained; review before continuing.");
      draftSaveHoldRef.current = null;
      setActiveDraft(saved, false, selection);
      setDraftChoices((current) => [saved, ...current.filter((item) => item.id !== saved.id)]);
      setTitle(normalizeDocumentTitleInput(saved.title)); setContent(parsed.body); setSynopsis(parsed.synopsis); setCustomCss(parsed.customCss ?? ""); setCssProvenance(parsed.cssProvenance);
      pageWidthRef.current = parsed.pageWidth; setPageWidth(parsed.pageWidth); resetAutoSave(parsed.body);
      setDoc((current) => current ? { ...current, icon: saved.icon, cover_image: saved.cover_image } : current);
      setDraftHasChanges(false); setSaveQueued(false); setDraftError(""); setRecoveryCandidate(null); setUnavailableRecoveryScope(null); setRecoveryReviewOpen(false); setRecoveryReviewDraft(null); setRecoveryReviewUnavailable(false);
      setBirthStatus({ route, message: action === "copy" || recoveryTargetReview ? "Recovered into a separate private draft · unpublished. The original draft is unchanged." : "Reviewed draft replaced · unpublished. The published page is unchanged." });
      const query = new URLSearchParams(location.search); query.set("draft", saved.id);
      navigate({ pathname: location.pathname, search: query.toString() }, { replace: true });
    } catch (error) {
      if (isDraftSaveHeldError(error) && action === "replace") draftSaveHoldRef.current = error;
      setRecoveryReviewError(error instanceof Error ? error.message : "Recovery could not be confirmed. Keep the device copy and operation evidence.");
    } finally { recoveryBusyRef.current = false; setRecoveryBusy(false); replacementHistoryRefreshRef.current(); }
  };

  useEffect(() => {
    if (previousOnlineRef.current === online) return;
    previousOnlineRef.current = online;
    if (!online) {
      emitTransientSystemNotification({
        title: "Working offline",
        body: "Changes stay on this device and will sync when the server is reachable again.",
      });
    }
  }, [online]);

  useEffect(() => {
    if (publishing || viewMode === "preview" || (status !== "saved" && status !== "error")) return;
    const now = Date.now();
    if (status === "saved" && now - lastSaveNotificationAtRef.current < 10_000) return;
    lastSaveNotificationAtRef.current = now;
    emitTransientSystemNotification(
      status === "saved"
        ? {
            title: saveQueued ? "Saved on this device" : "Changes saved",
            body: saveQueued
              ? "Waiting to sync with the server."
              : "Your latest draft changes are stored; readers still see the published revision.",
          }
        : {
            title: "Save failed",
            body: draftSaveHoldRef.current
              ? "Your edits remain on this device. Compare or reload the draft before trying again."
              : online
                ? "Starlane Notes will retry after the save window."
                : "You are offline. Your edits will retry when the connection returns.",
          },
    );
  }, [online, publishing, saveQueued, status, viewMode]);

  useEffect(() => {
    if (!spaceId || !docId) return;
    let cancelled = false;
    const unsubscribe = addDocumentLocationChangedListener((detail: DocumentLocationChangedDetail) => {
      if (detail.spaceId !== spaceId || detail.documentId !== docId || doc?.page_access_source === "direct_page_grant") return;

      const requestId = ++folderPathRefreshRequestIdRef.current;
      setParentPath([]);
      setFolderPath([]);
      setDoc((current) =>
        current?.id === docId
          ? {
              ...current,
              folder_id: detail.folderId ?? undefined,
              parent_id: detail.parentId ?? undefined,
            }
          : current,
      );
      void Promise.all([
        dataClient.listFolders(spaceId),
        dataClient.listDocuments(spaceId, { include_nested: true }),
      ])
        .then(([folderData, documentData]) => {
          if (cancelled || requestId !== folderPathRefreshRequestIdRef.current) return;
          setFolderPath(buildDocumentFolderPath(folderData, documentData, docId, detail.folderId));
          setParentPath(buildDocumentParentPath(documentData, docId, detail.parentId));
        })
        .catch((error) => {
          if (cancelled || requestId !== folderPathRefreshRequestIdRef.current) return;
          emitTransientSystemNotification({
            title: "Could not refresh page location",
            body: error instanceof Error ? error.message : "The page breadcrumb may be out of date.",
          });
        });
    });

    return () => {
      cancelled = true;
      unsubscribe();
    };
  }, [spaceId, docId, doc?.page_access_source]);

  // Load document
  useEffect(() => {
    if (!spaceId || !docId) return;
    let selection: DraftSaveSelection;
    try { selection = captureDraftSaveSelection(user?.id); } catch { return; }
    if (draftSaveHoldRef.current && selectedDraftContextRef.current && confirmedDraftRef.current?.space_id === spaceId && confirmedDraftRef.current?.document_id === docId) {
      try { verifyDraftSaveSelection(selectedDraftContextRef.current, user?.id); return; } catch { /* A different original context must load its own draft, never reuse this source. */ }
    }
    const loadGeneration = ++draftSelectionGenerationRef.current;
    let cancelled = false;
    const folderPathRequestId = ++folderPathRefreshRequestIdRef.current;

    setStarred(isDocumentStarred({ spaceId, id: docId }));

    setPublicationReview(null);
    setPublishing(false);
    publicationPreparationRef.current = null;
    publicationBoundaryRef.current = null;
    setDraftError("");
    try { const records = selection.mode === "server" ? dataClient.inspectDocumentDraftPublications(spaceId, docId) : []; publicationPendingRef.current = records.some((record) => ["prepared", "dispatched", "unknown"].includes(record.state)); setPublicationRecords(records); }
    catch (error) { setPublicationRecords([]); publicationPendingRef.current = true; setDraftError(error instanceof Error ? error.message : "Publication evidence could not be inspected."); }
    setLoading(true); setSelectedDraftOnly(false); setSelectedDraftPagePublished(false); setSelectedDraftViewStatus("loading");
    setDraftPromptOpen(false);
    setBirthStatus(null);
    setDraftListUnavailable(false);
    setSaveQueued(false);
    setDraftHasChanges(false);
    setRecoveryCandidate(null);
    setUnavailableRecoveryScope(null);
    setRecoveryReviewOpen(false);
    setRecoveryReviewDraft(null);
    setRecoveryReviewUnavailable(false);
    setRecoveryStorageFailed(false);
    draftFlowStartedRef.current = false;

    let editSource:DocumentDraftEditSource|null = null;
    let sourceOnlyMode = false;
    const sourceFirst = selection.mode === "server" && requestedDraftId
      ? dataClient.getCurrentDocumentDraftEditSource(spaceId,docId,requestedDraftId,selection).then(source => { editSource=source; }).catch(() => {})
      : Promise.resolve();
    sourceFirst.then(() => {
      if (editSource && requestedDraftId) {
        if (cancelled || draftSelectionGenerationRef.current !== loadGeneration) throw new Error("Selected draft context changed.");
        verifyDraftSaveSelection(selection,user?.id);
        const source=editSource; sourceOnlyMode=true;
        const metadata:DocumentData={id:docId,space_id:spaceId,title:source.draft.title,icon:source.draft.icon??undefined,cover_image:source.draft.cover_image??undefined,locked:source.locked,locked_by:source.locked_by??undefined,page_edit_allowed:true,updated_at:source.page_updated_at};
        return {metadata,content:{id:docId,title:source.draft.title,content:source.draft.content},space:null,pageOnly:false,supplementary:Promise.resolve<[TemplateRecord[],FolderRecord[],DocumentRecord[]]>([[],[],[]]),drafts:Promise.resolve([source.selected_draft])};
      }
      // A denied Edit source keeps the established ordinary View/DraftView reader path.
      return loadPageViewerContext({
      metadata: () => dataClient.getDocument(spaceId, docId) as Promise<DocumentData>,
      content: () => dataClient.getDocumentContent(spaceId, docId) as Promise<DocumentContent>,
      space: () => dataClient.getSpace(spaceId) as Promise<SpaceInfo>,
      templates: () => dataClient.listTemplates(spaceId),
      folders: () => dataClient.listFolders(spaceId),
      documents: () => dataClient.listDocuments(spaceId, { include_nested: true }),
      drafts: () => dataClient.listDocumentDrafts(spaceId, docId),
    }, () => {
      if (cancelled || draftSelectionGenerationRef.current !== loadGeneration) throw new Error("Page selection changed.");
      verifyDraftSaveSelection(selection, user?.id);
    });
    }).then(async ({ metadata: docData, content: contentData, space: spaceData, pageOnly, supplementary: supplementaryData, drafts: draftChoicesRequest }) => {
        if (cancelled || draftSelectionGenerationRef.current !== loadGeneration) return;
        verifyDraftSaveSelection(selection, user?.id);
        if (!pageOnly && !sourceOnlyMode) void loadStarredDocuments(spaceId).then(() => {
          if (!cancelled) setStarred(isDocumentStarred({spaceId,id:docId}));
        });
        const sourceOnly = sourceOnlyMode;
        setSelectedDraftOnly(sourceOnly); setSelectedDraftPagePublished(sourceOnly && !!editSource?.page_published);
        const parsed = parseDocumentContent(contentData.content, docData.title);
        setDoc(docData);
        setContent(parsed.body);
        setSynopsis(parsed.synopsis);
        setCustomCss(parsed.customCss ?? ""); setCssProvenance(parsed.cssProvenance);
        pageWidthRef.current = parsed.pageWidth;
        setPageWidth(parsed.pageWidth);
        setTitle(normalizeDocumentTitleInput(docData.title));
        setSpaceName(spaceData?.name ?? "");
        setSpaceMemberRole(spaceData?.member_role);
        setFolderPath([]);
        setParentPath([]);
        setTemplates([]);
        setDraftChoices([]);
        setActiveDraft(null);
        resetAutoSave(parsed.body);
        rememberLastViewedDocumentRoute(spaceId, docId);
        updateStarredDocumentMetadata({
          spaceId,
          id: docId,
          title: docData.title,
          spaceName: spaceData?.name ?? "",
          icon: docData.icon,
          updated_at: docData.updated_at,
        });

        // Folder paths and template actions are useful context, but neither is
        // allowed to hold the document canvas behind server round trips.
        void supplementaryData.then(([templateData, folderData, documentData]) => {
          if (cancelled) return;
          try { verifyDraftSaveSelection(selection, user?.id); } catch { return; }
          setTemplates(templateData);
          if (folderPathRequestId === folderPathRefreshRequestIdRef.current) {
            setFolderPath(
              buildDocumentFolderPath(folderData, documentData, docId, docData.folder_id),
            );
            setParentPath(buildDocumentParentPath(documentData, docId, docData.parent_id));
          }
        });

        if (pageOnly) {
          rightPanel.close();
          setInlineCommentsVisible(false);
          setViewMode("preview");
          if (requestedDraftId) setDraftError("This grant covers the published page only. Private drafts are unavailable.");
          setLoading(false);
          return;
        }
        const requiresDraftBeforeRender = shouldBlockDocumentRenderForDraft(
          docData.published_at,
          requestedDraftId,
        );
        if (!requiresDraftBeforeRender) {
          setViewMode("preview");
          setLoading(false);
        }

        const drafts = await draftChoicesRequest;
        if (cancelled || draftSelectionGenerationRef.current !== loadGeneration + 1) return;
        verifyDraftSaveSelection(selection, user?.id);
        if (!drafts) {
          setDraftListUnavailable(true);
          setDraftError(
            requestedDraftId
              ? "Could not load the requested draft. Retry or return to the published page."
              : "Could not load drafts. Retry, or use Edit to resume your own draft.",
          );
          setViewMode("preview");
          return;
        }
        setDraftChoices(drafts);
        // Published readers stay on the published revision; Edit is explicit.
        const initialDraft =
          requestedDraftId || !docData.published_at
            ? selectDocumentDraft(drafts, requestedDraftId)
            : undefined;
        if (requestedDraftId && !initialDraft) {
          const scope: DocumentDraftRecoveryScope = { storageMode: selection.mode === "local" ? "local" : "server", authIdentity: selection.accountId, serverTarget: selection.serverBase, spaceId, documentId: docId, draftId: requestedDraftId };
          const retained = readDocumentDraftRecovery(scope);
          if (retained) {
            selectedDraftContextRef.current = selection;
            setUnavailableRecoveryScope(scope); setRecoveryCandidate(retained);
          }
          setDraftError(
            "This draft is unavailable or you no longer have access. Return to the published page or retry.",
          );
          setViewMode("preview");
          return;
        }

        if (initialDraft) {
          const draft = sourceOnly ? editSource!.selected_draft : await dataClient
            .getDocumentDraft(spaceId, docId, initialDraft.id)
            .catch(() => null);
          if (cancelled) return;
          if (!draft) {
            setDraftError(
              "Could not load the draft. Your page has not been changed. Retry to resume it.",
            );
            setViewMode("preview");
            return;
          }
          verifyDraftSaveSelection(selection, user?.id);
          if (draftSelectionGenerationRef.current !== loadGeneration + 1) return;
          const draftContent = parseDocumentContent(draft.content, draft.title);
          checkDraftRecovery(draft, docData.locked, selection);
          setActiveDraft(draft, false, selection);
          setTitle(normalizeDocumentTitleInput(draft.title));
          setContent(draftContent.body);
          resetAutoSave(draftContent.body);
          setSynopsis(draftContent.synopsis);
          setCustomCss(draftContent.customCss ?? ""); setCssProvenance(draftContent.cssProvenance);
          pageWidthRef.current = draftContent.pageWidth;
          setPageWidth(draftContent.pageWidth);
          setDoc((current) =>
            current ? { ...current, icon: draft.icon, cover_image: draft.cover_image } : current,
          );
          setViewMode(draft.access_role === "viewer" ? "preview" : "markdown");
        } else if (requiresDraftBeforeRender) {
          setViewMode("preview");
        }
      })
      .catch(() => {
        if (cancelled) return;
        try { verifyDraftSaveSelection(selection, user?.id); } catch { return; }
        if (draftSelectionGenerationRef.current === loadGeneration || draftSelectionGenerationRef.current === loadGeneration + 1) {
          if (requestedDraftId && selection.accountId) {
            const scope: DocumentDraftRecoveryScope = { storageMode: selection.mode === "local" ? "local" : "server", authIdentity: selection.accountId, serverTarget: selection.serverBase, spaceId, documentId: docId, draftId: requestedDraftId };
            const retained = readDocumentDraftRecovery(scope);
            if (retained) {
              selectedDraftContextRef.current = selection;
              setUnavailableRecoveryScope(scope); setRecoveryCandidate(retained);
            }
          }
          setDoc(null);
        }
      })
      .finally(() => {
        if (cancelled) return;
        try { verifyDraftSaveSelection(selection, user?.id); } catch { return; }
        if (draftSelectionGenerationRef.current === loadGeneration || draftSelectionGenerationRef.current === loadGeneration + 1 || confirmedDraftRef.current && selectedDraftContextRef.current === selection) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [spaceId, docId, requestedDraftId, resetAutoSave, checkDraftRecovery, setActiveDraft, user?.id, publicationReload]);

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
    const resizeTitle = () => {
      titleField.style.height = "auto";
      titleField.style.height = `${titleField.scrollHeight}px`;
    };
    resizeTitle();
    // Observe the containing width, not the textarea height we change ourselves.
    const observer = new ResizeObserver(resizeTitle);
    if (titleField.parentElement) observer.observe(titleField.parentElement);
    return () => observer.disconnect();
  }, [title, loading, viewMode, mobileLayout, pageWidth]);

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

  const handleContentChange = (newContent: string) => {
    if (publishing || publicationPending || openingDraft || recoveryCandidate || newContent === currentSaveStateRef.current.content) return;
    checkpointDraft({ content: newContent });
    setContent(newContent);
    setDraftHasChanges(true);
    scheduleAutoSave(newContent);
  };

  const handlePageWidthChange = useCallback(
    (nextWidth: DocumentPageWidth) => {
      checkpointDraft({ pageWidth: nextWidth });
      setDraftHasChanges(true);
      pageWidthRef.current = nextWidth;
      currentSaveStateRef.current = { ...currentSaveStateRef.current, pageWidth: nextWidth };
      setPageWidth(nextWidth);
      if (activeDraft && activeDraft.access_role !== "viewer") {
        void saveNow(content);
        return;
      }
      scheduleAutoSave(content);
    },
    [activeDraft, content, saveNow, scheduleAutoSave, checkpointDraft],
  );

  const handleTitleChange = async (newTitle: string) => {
    const normalizedTitle = newTitle.replace(/\s*\n+\s*/g, " ").trim() || "Untitled";
    checkpointDraft({title: normalizedTitle});
    setTitle(normalizedTitle);
    if (spaceId && docId && activeDraft && activeDraft.access_role !== "viewer") {
      const saved = await saveNow(content);
      if (!saved) {
        setDraftError(draftSaveHoldRef.current?.message ?? "Could not save the page title. Your edits remain open.");
        return;
      }
      updateStarredDocumentMetadata({
        spaceId,
        id: docId,
        title: normalizedTitle,
        spaceName,
        icon: doc?.icon,
        updated_at: doc?.updated_at,
      });
    }
  };

  const activateDraft = async (draftId: string, focusEditor = false) => {
    if (!spaceId || !docId) return;
    const activationRoute = `${spaceId}/${docId}`;
    const selection = captureDraftSaveSelection(user?.id);
    const activationGeneration = ++draftSelectionGenerationRef.current;
    const draft = selectedDraftOnly
      ? (await dataClient.getCurrentDocumentDraftEditSource(spaceId,docId,draftId,selection)).selected_draft
      : await dataClient.getDocumentDraft(spaceId, docId, draftId);
    verifyDraftSaveSelection(selection, user?.id);
    if (routeIdentityRef.current !== activationRoute || draftSelectionGenerationRef.current !== activationGeneration) return;
    const parsed = parseDocumentContent(draft.content, draft.title);
    checkDraftRecovery(draft, Boolean(doc?.locked), selection);
    draftSaveHoldRef.current = null;
    setActiveDraft(draft, false, selection);
    setTitle(normalizeDocumentTitleInput(draft.title));
    setContent(parsed.body);
    resetAutoSave(parsed.body);
    setSaveQueued(false);
    setDraftHasChanges(false);
    setSynopsis(parsed.synopsis);
    setCustomCss(parsed.customCss ?? ""); setCssProvenance(parsed.cssProvenance);
    pageWidthRef.current = parsed.pageWidth;
    setPageWidth(parsed.pageWidth);
    setDoc((current) =>
      current ? { ...current, icon: draft.icon, cover_image: draft.cover_image } : current,
    );
    setDraftPromptOpen(false);
    const draftEditable = draft.access_role !== "viewer";
    setFocusEditorOnMount(focusEditor && draftEditable);
    setViewMode(draftEditable ? "markdown" : "preview");
  };

  const startDraft = async (forceNew = false, focusEditor = false, retry: typeof birthRetry = null) => {
    if (!spaceId || !docId) return;
    if (!currentPageCapabilityRef.current().canEditPage) throw new Error(pageEditBlockedExplanation(currentPageCapabilityRef.current().blockedReason));
    const creationRoute = `${spaceId}/${docId}`;
    const selection = retry?.selection ?? captureDraftSaveSelection(user?.id);
    verifyDraftSaveSelection(selection, user?.id);
    const generation = retry?.generation ?? draftSelectionGenerationRef.current;
    const canvas = retry?.canvas ?? JSON.stringify(currentSaveStateRef.current);
    if (retry && retry.route !== creationRoute) throw new Error("Return to the original page to review its retained draft request");
    const source = confirmedDraftRef.current;
    if (source && selectedDraftContextRef.current) verifyDraftSaveSelection(selectedDraftContextRef.current, user?.id);
    if (source && selection.mode === "server" && !selectedDraftContextRef.current) throw new Error("Review the original selected draft session before creating another draft");
    if (source && (source.space_id !== spaceId || source.document_id !== docId)) throw new Error("Review the selected draft on this page before creating another draft");
    let draft: DraftRecord;
    try {
      draft = retry
        ? await dataClient.retryDocumentDraftBirth(spaceId, docId, retry.operationId)
        : await dataClient.createDocumentDraft(spaceId, docId, {
            force_new: forceNew,
            ...(!(selectedDraftOnly ? selectedDraftPagePublished : doc?.published_at) && selection.mode === "server" ? selectedDraftFork(source) : {}),
          });
    } catch (error) {
      if (error instanceof DraftBirthHeldError) setBirthRetry((current) => current?.operationId === error.operationId ? current : { operationId: error.operationId, route: creationRoute, generation, canvas, selection });
      throw error;
    }
    verifyDraftSaveSelection(selection, user?.id);
    if (routeIdentityRef.current !== creationRoute) return;
    setBirthRetry(null);
    setDraftChoices((current) => [draft, ...current.filter((candidate) => candidate.id !== draft.id)]);
    if (retry || draftSelectionGenerationRef.current !== generation || JSON.stringify(currentSaveStateRef.current) !== canvas) {
      setBirthStatus({ route: creationRoute, message: "The draft request completed. Your newer edits remain open; select the draft from Drafts when ready." });
      return;
    }
    await activateDraft(draft.id, focusEditor);
  };

  const runDraftAction = async (action: () => Promise<void>) => {
    if (openingDraft || publishing || recoveryCandidate) return;
    if (!currentPageCapabilityRef.current().canEditPage) { setDraftError(pageEditBlockedExplanation(currentPageCapabilityRef.current().blockedReason) ?? "Current Edit access is unavailable."); return; }
    setOpeningDraft(true);
    setDraftError("");
    setBirthStatus(null);
    try {
      // Finish in-flight writes before deciding whether another save is needed.
      // An unchanged ambiguous source needs no automatic reconstruction.
      await draftSaveTailRef.current;
      if (publicationPendingRef.current) throw new Error("Review the retained publication before changing drafts.");
      if (activeDraft && activeDraft.access_role !== "viewer" && !pageEditBlocked &&
        (draftHasChanges || saveQueued || status === "saving" || draftSaveHoldRef.current || retainedOriginalSave())) {
        if (!(await saveNow(content)))
          throw new Error(
            draftSaveHoldRef.current?.message ?? "Could not save your current draft. Your edits remain open.",
          );
      }
      await action();
    } catch (error) {
      setDraftError(
        error instanceof Error
          ? error.message
          : "Could not open the draft. Retry when the server is reachable.",
      );
    } finally {
      setOpeningDraft(false);
    }
  };

  const openDraftChoices = () => {
    void runDraftAction(async () => {
      const drafts = await dataClient.listDocumentDrafts(spaceId!, docId!);
      setDraftChoices(drafts);
      setDraftListUnavailable(false);
      setDraftPromptOpen(true);
    });
  };

  const setEditorViewMode = (next: EditorViewMode) => {
    if (selectedDraftOnly && next === "preview") {
      navigate(`/spaces/${encodeURIComponent(spaceId ?? "")}/docs/${encodeURIComponent(docId ?? "")}`);
      return;
    }
    if (
      openingDraft ||
      publishing ||
      (next !== "preview" && (pageEditBlocked || spaceRoleBlocksDraftEditing || doc?.page_access_source === "direct_page_grant"))
    )
      return;
    if (next !== "preview" && (!activeDraft || activeDraft.access_role === "viewer")) {
      draftFlowStartedRef.current = true;
      void runDraftAction(async () => {
        const ownedDraft = selectDocumentDraft(draftChoices);
        if (ownedDraft) await activateDraft(ownedDraft.id, next === "markdown");
        else if (shouldChooseAccessibleDraft(draftChoices)) setDraftPromptOpen(true);
        else await startDraft(false, next === "markdown");
        if (next === "source") setViewMode("source");
      });
      return;
    }
    if (next === "markdown" && activeDraft?.access_role !== "viewer" && !pageEditBlocked) {
      if (viewMode === "markdown" && editorRef && !editorRef.isDestroyed) {
        editorRef.commands.focus();
      } else {
        setFocusEditorOnMount(true);
      }
    } else {
      setFocusEditorOnMount(false);
    }
    setViewMode(next);
    if (next !== "markdown") setEditorRef(null);
  };

  const handleEditorReady = useCallback((editor: TipTapEditor) => {
    setEditorRef(editor);
    setFocusEditorOnMount(false);
  }, []);

  const publishActiveDraft = async () => {
    if (
      !spaceId ||
      !docId ||
      !activeDraft?.can_publish ||
      publishing ||
      openingDraft ||
      pageEditBlocked ||
      recoveryCandidate
    )
      return;
    if (publicationPending) { setDraftError("Review the original retained publication before saving or starting another publication."); return; }
    const preparation = Symbol("publication preparation");
    publicationPreparationRef.current = preparation;
    const publishRoute = `${spaceId}/${docId}`;
    const publishGeneration = draftSelectionGenerationRef.current;
    const publishAccount = recoveryAccountRef.current;
    const originalSelection = selectedDraftContextRef.current;
    const requirePublishSelection = () => {
      if (!originalSelection || routeIdentityRef.current !== publishRoute || draftSelectionGenerationRef.current !== publishGeneration || recoveryAccountRef.current !== publishAccount) throw new Error("Publication selection changed. Keep the original draft and operation evidence.");
      verifyDraftSaveSelection(originalSelection, recoveryAccountRef.current);
      if (!currentPageCapabilityRef.current().canEditPage) throw new Error(pageEditBlockedExplanation(currentPageCapabilityRef.current().blockedReason));
    };
    setPublishing(true);
    setDraftError("");
    setBirthStatus(null);
    try {
      if (!(await saveNow(content)))
        throw new Error(
          draftSaveHoldRef.current?.message ?? "Could not save your draft. Publish was not attempted; your edits remain open.",
        );
      requirePublishSelection();
      const serializedContent = serializeDocumentContent(
        content,
        synopsis,
        pageWidthRef.current,
        customCss,
        cssProvenance,
      );
      // Publishing is a commit boundary. Persist the current editor state first,
      // against the confirmed source so publish cannot race an autosave.
      const savedDraft = await saveActiveDraft({
        title,
        content: serializedContent,
      });
      requirePublishSelection();
      if (!savedDraft) {
        setSaveQueued(true);
        throw new Error(
          "Draft saved on this device. Reconnect and choose Publish changes again; readers still see the published revision.",
        );
      }
      const selection = selectedDraftContextRef.current;
      if (!selection) throw new Error("Publication selection changed. Keep the saved draft and review it again.");
      if (selection.mode === "server") {
        if (publicationPending) throw new Error("Review the original retained publication before starting another request.");
        const boundary = { route: `${spaceId}/${docId}`, generation: draftSelectionGenerationRef.current, account: recoveryAccountRef.current, canvas: JSON.stringify(currentSaveStateRef.current) };
        publicationBoundaryRef.current = boundary;
        const intent = await dataClient.prepareDocumentDraftPublication(spaceId, docId, savedDraft, selection);
        verifyDraftSaveSelection(selection, recoveryAccountRef.current);
        if (routeIdentityRef.current !== boundary.route || draftSelectionGenerationRef.current !== boundary.generation || recoveryAccountRef.current !== boundary.account || JSON.stringify(currentSaveStateRef.current) !== boundary.canvas) throw new Error("Publication review belongs to the previous selection. Its original evidence is retained.");
        publicationPendingRef.current = true;
        setPublicationReview(intent);
        setPublicationRecords(dataClient.inspectDocumentDraftPublications(spaceId, docId));
        return;
      }
      const published = (await dataClient.publishDocumentDraft(spaceId, docId, savedDraft.id)) as DocumentData;
      const publishedContent = stripTemplateGuidance(serializedContent);
      setDoc((current) =>
        current ? mergeDraftPublication(current, savedDraft, published, user?.id ?? "") : current,
      );
      setActiveDraft(null);
      setDraftChoices((current) => current.filter((draft) => draft.id !== savedDraft.id));
      const parsedPublished = parseDocumentContent(publishedContent, savedDraft.title);
      setContent(parsedPublished.body);
      setCustomCss(parsedPublished.customCss ?? ""); setCssProvenance(parsedPublished.cssProvenance);
      resetAutoSave(parsedPublished.body);
      setViewMode("preview");
      emitGraphDirty(spaceId, "document-metadata-updated");
      emitTransientSystemNotification({
        title: "Changes published",
        body: "People with access to this page can read the updated revision.",
      });
    } catch (error) {
      if (routeIdentityRef.current === publishRoute && draftSelectionGenerationRef.current === publishGeneration && recoveryAccountRef.current === publishAccount) setDraftError(error instanceof Error ? error.message : "Unable to publish draft.");
    } finally {
      if (publicationPreparationRef.current === preparation) {
        publicationPreparationRef.current = null;
        if (routeIdentityRef.current === publishRoute && recoveryAccountRef.current === publishAccount) setPublishing(false);
      }
    }
  };

  const refreshPublicationRecords = () => {
    if (!spaceId || !docId) return;
    try { const records = dataClient.inspectDocumentDraftPublications(spaceId, docId); publicationPendingRef.current = records.some((record) => ["prepared", "dispatched", "unknown"].includes(record.state)); setPublicationRecords(records); }
    catch (error) { setDraftError(error instanceof Error ? error.message : "Keep the retained publication evidence."); }
  };
  const reloadPublishedPage = () => {
    draftSelectionGenerationRef.current += 1;
    draftSaveHoldRef.current = null;
    const query = new URLSearchParams(location.search);
    const leavingDraftSelection = query.has("draft");
    query.delete("draft");
    navigate({ pathname: location.pathname, search: query.toString() }, { replace: true });
    // A draft-route exit already reloads through requestedDraftId. Advancing the explicit reload
    // epoch in the same render would briefly reload the consumed draft before navigation commits.
    if (!leavingDraftSelection) setPublicationReload((epoch) => epoch + 1);
  };
  const acceptPublicationReceipt = (receipt: DraftPublicationReceipt, boundary: typeof publicationBoundaryRef.current) => {
    if (routeIdentityRef.current !== `${receipt.space_id}/${receipt.document_id}`) throw new Error("Publication was confirmed for the previous page. Review its retained receipt there.");
    if (boundary && (boundary.generation !== draftSelectionGenerationRef.current || boundary.account !== recoveryAccountRef.current || boundary.canvas !== JSON.stringify(currentSaveStateRef.current))) throw new Error("Publication was confirmed, but the editor selection changed. Keep current edits and review the retained receipt.");
    setPublicationReview(null);
    const scope = getRecoveryScope(confirmedDraftRef.current);
    const retained = scope && readDocumentDraftRecovery(scope);
    if (retained || recoveryCandidate || draftHasChanges) {
      setBirthStatus({ route: `${spaceId}/${docId}`, message: "The original publication is confirmed. Current device edits remain open; review them before leaving this draft." });
      return;
    }
    // Successful publication consumes the reviewed draft. Clear that exact selection before the
    // published-page reload so capability refresh cannot request a save source that no longer exists.
    setActiveDraft(null);
    setDraftChoices((current) => current.filter((draft) => draft.id !== receipt.draft_id));
    reloadPublishedPage();
    emitGraphDirty(receipt.space_id, "document-metadata-updated");
    emitTransientSystemNotification({ title: "Publication confirmed", body: "The current published page is loaded separately from the original publication receipt." });
  };
  const dispatchPublication = async (intent: DraftPublicationIntent, retry = false) => {
    if (publicationDispatchRef.current || !spaceId || !docId) return;
    const route = `${spaceId}/${docId}`;
    let selection: DraftSaveSelection;
    try { selection = captureDraftSaveSelection(recoveryAccountRef.current); } catch (error) { setDraftError(error instanceof Error ? error.message : "Original publication context is unavailable."); return; }
    const boundary = retry ? { route, generation: draftSelectionGenerationRef.current, account: recoveryAccountRef.current, canvas: JSON.stringify(currentSaveStateRef.current) } : publicationBoundaryRef.current;
    publicationDispatchRef.current = intent.operation_id;
    setPublicationDispatching(true); setPublishing(true); setDraftError("");
    try {
      verifyDraftSaveSelection(selection, recoveryAccountRef.current);
      assertDocumentPageEditable(currentPageCapabilityRef.current());
      if (intent.space_id !== spaceId || intent.document_id !== docId || boundary && (boundary.route !== route || boundary.generation !== draftSelectionGenerationRef.current || boundary.account !== recoveryAccountRef.current || boundary.canvas !== JSON.stringify(currentSaveStateRef.current))) throw new Error("Review the retained publication in its original selection before dispatch.");
      const receipt = retry ? await dataClient.retryDocumentDraftPublication(intent.operation_id) : await dataClient.publishPreparedDocumentDraft(intent);
      verifyDraftSaveSelection(selection, recoveryAccountRef.current);
      if (routeIdentityRef.current !== route) throw new Error("Publication receipt belongs to the previous page. Its evidence remains retained.");
      acceptPublicationReceipt(receipt, boundary);
    } catch (error) {
      if (routeIdentityRef.current === route) setDraftError(error instanceof Error ? error.message : "Publication outcome is held. Keep its original request for review.");
    } finally {
      if (publicationDispatchRef.current === intent.operation_id) {
        publicationDispatchRef.current = null;
        if (routeIdentityRef.current === route) { refreshPublicationRecords(); setPublicationDispatching(false); setPublishing(false); }
      }
    }
  };
  const cancelPublicationReview = async () => {
    if (!publicationReview || publicationDispatching) return;
    const original = publicationReview;
    const route = `${original.space_id}/${original.document_id}`;
    try {
      const record = dataClient.inspectDocumentDraftPublications(original.space_id, original.document_id).find((entry) => entry.intent.operation_id === original.operation_id);
      if (!record) throw new Error("Original publication evidence is unavailable. Keep it for review.");
      if (record.state === "prepared") await dataClient.cancelPreparedDocumentDraftPublication(original.operation_id);
      if (routeIdentityRef.current === route && recoveryAccountRef.current === original.origin.accountId) { setPublicationReview(null); publicationBoundaryRef.current = null; refreshPublicationRecords(); }
    }
    catch (error) { if (routeIdentityRef.current === route && recoveryAccountRef.current === original.origin.accountId) setDraftError(error instanceof Error ? error.message : "The original publication remains retained."); }
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
  };

  const deleteDocument = async () => {
    if (!spaceId || !docId) return;
    setDeleting(true);
    setDeleteError("");
    try {
      await dataClient.deleteDocument(spaceId, docId);
      emitGraphDirty(spaceId, "document-deleted");
      navigate(buildSpaceHomePath(spaceId), { replace: true });
    } catch (error) {
      setDeleteError(error instanceof Error ? error.message : "Unable to delete page.");
    } finally {
      setDeleting(false);
    }
  };
  const duplicateDocument = async () => {
    if (!spaceId || !docId) return;
    try {
      const duplicate = await dataClient.duplicateDocument(spaceId, docId);
      emitGraphDirty(spaceId, "document-created");
      navigate(`/spaces/${spaceId}/docs/${duplicate.id}`);
    } catch (error) {
      setDeleteError(error instanceof Error ? error.message : "Unable to duplicate page.");
    }
  };
  const archiveDocument = async () => {
    if (!spaceId || !docId) return;
    try {
      await dataClient.archiveDocument(spaceId, docId);
      emitGraphDirty(spaceId, "document-metadata-updated");
      navigate(buildSpaceHomePath(spaceId), { replace: true });
    } catch (error) {
      setDeleteError(error instanceof Error ? error.message : "Unable to archive page.");
    }
  };
  const isMarkdownMode = viewMode === "markdown";
  const isSourceMode = viewMode === "source";
  const pageOnlyViewer = doc?.page_access_source === "direct_page_grant";
  const draftEditable = !pageOnlyViewer && Boolean(activeDraft && activeDraft.access_role !== "viewer");
  const draftWritable = !peerUnavailableRef.current && !pageEditBlocked && draftEditable && !publishing && !publicationPending && !publicationReview && !openingDraft && !recoveryCandidate;
  const metadataEditable = canEditDocumentMetadata(viewMode, draftWritable, pageEditBlocked);
  const pageDetailsEditable = metadataEditable;
  const pageValuesEditable =
    pageDetailsEditable &&
    (getCurrentLaunchMode() === "local" ||
      ["owner", "space_admin", "admin", "editor", "contributor"].includes(spaceMemberRole ?? ""));
  const coverWritableRef = useRef(pageDetailsEditable);
  coverWritableRef.current = pageDetailsEditable;
  const coverWritableEpoch = useRef({writable:pageDetailsEditable,version:0});
  if (coverWritableEpoch.current.writable !== pageDetailsEditable)
    coverWritableEpoch.current = {writable:pageDetailsEditable,version:coverWritableEpoch.current.version+1};
  const coverWritableVersion = coverWritableEpoch.current.version;
  const coverChoiceEpoch = useRef(0);
  const coverSelectionGeneration = draftSelectionGenerationRef.current;
  const coverSelection = selectedDraftContextRef.current;
  const coverDraftId = activeDraft?.id;
  const coverLifetime = useRef(true);
  useEffect(() => { coverLifetime.current = true; return () => { coverLifetime.current = false; }; }, []);
  const verifyCoverSelection = useCallback(() => {
    if (!coverLifetime.current || !coverWritableRef.current || coverWritableEpoch.current.version !== coverWritableVersion || !coverSelection || !coverDraftId ||
      routeIdentityRef.current !== `${spaceId}/${docId}` || draftSelectionGenerationRef.current !== coverSelectionGeneration ||
      selectedDraftContextRef.current !== coverSelection || confirmedDraftRef.current?.id !== coverDraftId ||
      recoveryAccountRef.current !== user?.id)
      throw new Error("Original cover draft changed; inspect the retained upload before use.");
    verifyDraftSaveSelection(coverSelection,user?.id);
    assertDocumentPageEditable(currentPageCapabilityRef.current());
  }, [coverSelection,coverSelectionGeneration,coverDraftId,coverWritableVersion,spaceId,docId,user?.id]);
  const coverUploadAcknowledgements=useRef(new Map<string,{operation:string;spaceId:string;docId:string;draft:string;native?:boolean;verify:()=>void}>());
  const uploadCoverImage = useCallback(
    async (file: File) => {
      if (!spaceId || !docId) {
        throw new Error("Open a saved document before adding a cover image.");
      }
      const originalRoute=routeIdentityRef.current, draft=confirmedDraftRef.current?.id;
      if (!draft) throw new Error("Open an editable draft before adding a cover.");
      const verify=()=>{verifyCoverSelection();if(routeIdentityRef.current!==originalRoute || originalRoute!==`${spaceId}/${docId}` || !draft || confirmedDraftRef.current?.id!==draft)throw new Error("Original cover draft changed; inspect the retained upload before use.");};
      verify();
      if (getCurrentLaunchMode() !== "local") {
        const asset=await dataClient.saveDocumentFileAsset({name:file.name,bytes:file,space_id:spaceId,document_id:docId,draft_id:draft});
        verify();
        if (!("filename" in asset)) throw new Error("Cover upload is waiting for server acknowledgement; inspect retained uploads.");
        const pin=`starlane-attachment://${asset.id}`, operation=directUploadAcknowledgementId(asset);
        if(operation)coverUploadAcknowledgements.current.set(pin,{operation,spaceId,docId,draft,verify});
        return pin;
      }
      if(dataClient.supportsNativeLocalCover()) {
        const {asset,operationId}=await dataClient.saveNativeLocalDocumentCover({file,space_id:spaceId,document_id:docId,draft_id:draft},verify);verify();
        coverUploadAcknowledgements.current.set(asset.url,{operation:operationId,spaceId,docId,draft,native:true,verify});return asset.url;
      }
      const dataUrl=await new Promise<string>((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(String(reader.result));reader.onerror=()=>reject(reader.error ?? new Error("Could not read image."));reader.readAsDataURL(file);});
      verify();
      const asset=await dataClient.saveDocumentDataAsset({name:file.name,mime_type:file.type,data_url:dataUrl,space_id:spaceId,document_id:docId,draft_id:draft});
      verify();return asset.url;
    },
    [docId, spaceId, verifyCoverSelection],
  );
  const updateCoverImage = useCallback(
    async (url: string | null) => {
      if (!spaceId || !docId || !activeDraft || activeDraft.access_role === "viewer") {
        throw new Error("Open an editable draft before changing the cover image.");
      }
      if (routeIdentityRef.current!==`${spaceId}/${docId}` || confirmedDraftRef.current?.id!==activeDraft.id || confirmedDraftRef.current.access_role==="viewer") throw new Error("Original cover draft changed; retain the acknowledged upload for review.");
      verifyCoverSelection();
      const originalAcknowledgement = url ? coverUploadAcknowledgements.current.get(url) : undefined;
      originalAcknowledgement?.verify();
      const coverChoice = ++coverChoiceEpoch.current;
      checkpointDraft({ coverImage: url });
      if (peerBarrierRef.current) { setDoc(current => current ? {...current, cover_image:url ?? undefined} : current); setDraftHasChanges(true); return; }
      const updated = await saveActiveDraft({
        cover_image: url ?? "",
      });
      verifyCoverSelection();
      if (routeIdentityRef.current !== `${spaceId}/${docId}` || confirmedDraftRef.current?.id !== updated.id) return;
      const acknowledged=url ? coverUploadAcknowledgements.current.get(url) : undefined;
      if(acknowledged && acknowledged.spaceId===spaceId && acknowledged.docId===docId && acknowledged.draft===updated.id && updated.cover_image===url) {
        if(acknowledged.native)await markOriginalNativeLocalCoverAdopted(acknowledged.operation,url!,()=>{verifyCoverSelection();if(routeIdentityRef.current!==`${spaceId}/${docId}`||confirmedDraftRef.current?.id!==updated.id)throw new Error("Original local cover draft changed");});
        else await markDirectUploadAdopted({space_id:spaceId,document_id:docId,draft_id:acknowledged.draft},acknowledged.operation);
        verifyCoverSelection();
        if (coverUploadAcknowledgements.current.get(url!) === acknowledged) coverUploadAcknowledgements.current.delete(url!);
      }
      verifyCoverSelection();
      // A later cover choice owns its checkpoint even while this receipt completes.
      if (coverChoiceEpoch.current !== coverChoice) return;
      setDoc((current) => (current ? { ...current, cover_image: updated.cover_image } : current));
    },
    [activeDraft, docId, spaceId, saveActiveDraft, checkpointDraft, verifyCoverSelection],
  );
  const nativeCoverRecovery = useMemo(() => {
    if(!pageDetailsEditable||!dataClient.supportsNativeLocalCover()||!spaceId||!docId||!activeDraft)return undefined;
    const selected={space_id:spaceId,document_id:docId,draft_id:activeDraft.id};
    const verify=()=>{verifyCoverSelection();if(routeIdentityRef.current!==`${spaceId}/${docId}`||confirmedDraftRef.current?.id!==selected.draft_id||confirmedDraftRef.current.access_role==="viewer")throw new Error("Original cover draft changed; retain the original operation");};
    return {
      load:async()=>{verify();return dataClient.listRetainedNativeLocalDocumentCovers(selected,confirmedDraftRef.current?.cover_image,verify);},
      discard:async(operationId:string)=>{verify();await dataClient.discardNativeLocalDocumentCover(operationId,selected,verify,false);verify();},
      checkDiscard:async(operationId:string)=>{verify();await dataClient.discardNativeLocalDocumentCover(operationId,selected,verify,true);verify();},
      recover:async(operationId:string,mode:"check"|"resume")=>{verify();const result=mode==="resume"?await dataClient.resumeNativeLocalDocumentCover(operationId,selected,verify):await dataClient.reconcileNativeLocalDocumentCover(operationId,selected,verify);verify();coverUploadAcknowledgements.current.set(result.asset.url,{operation:operationId,spaceId,docId,draft:selected.draft_id,native:true,verify});await updateCoverImage(result.asset.url);verify();},
    };
  },[activeDraft,docId,pageDetailsEditable,spaceId,updateCoverImage,verifyCoverSelection]);
  const synopsisEditableRef = useRef(pageDetailsEditable);
  synopsisEditableRef.current = pageDetailsEditable;
  const synopsisGeneration = draftSelectionGenerationRef.current;
  const synopsisSelection = selectedDraftContextRef.current;
  const synopsisDraftId = activeDraft?.id;
  const verifySynopsisEdit = useCallback(() => {
    if (!synopsisEditableRef.current || !synopsisSelection || !synopsisDraftId ||
      routeIdentityRef.current !== `${spaceId}/${docId}` ||
      draftSelectionGenerationRef.current !== synopsisGeneration ||
      selectedDraftContextRef.current !== synopsisSelection ||
      confirmedDraftRef.current?.id !== synopsisDraftId || recoveryAccountRef.current !== user?.id)
      throw new Error("Original synopsis draft changed. Keep its retained edits and review the current selection.");
    verifyDraftSaveSelection(synopsisSelection, user?.id);
    assertDocumentPageEditable(currentPageCapabilityRef.current());
  }, [spaceId, docId, synopsisGeneration, synopsisSelection, synopsisDraftId, user?.id]);
  const changeDraftSynopsis = useCallback((nextSynopsis: string) => {
    verifySynopsisEdit();
    if (currentSaveStateRef.current.synopsis === nextSynopsis) return;
    checkpointDraft({ synopsis: nextSynopsis });
    setSynopsis(nextSynopsis);
    setDraftHasChanges(true);
  }, [verifySynopsisEdit, checkpointDraft]);
  const synopsisCompositionOwner = useMemo(() => ({ verify: verifySynopsisEdit }), [verifySynopsisEdit]);
  const changeSynopsisComposition = useCallback((composing: boolean) => {
    if (!composing) {
      // Releasing this original local owner needs no renewed grant. It must not
      // clear composition subsequently acquired by another control/selection.
      if (peerCompositionOwnerRef.current !== synopsisCompositionOwner) return;
      peerCompositionOwnerRef.current = null; peerComposingRef.current = false;
      try { synopsisCompositionOwner.verify(); } catch { return; }
      peerTransportRef.current?.resync();
      return;
    }
    synopsisCompositionOwner.verify();
    peerCompositionOwnerRef.current = synopsisCompositionOwner;
    peerComposingRef.current = true;
    peerEditEpochRef.current += 1;
  }, [synopsisCompositionOwner]);
  const updateDraftSynopsis = useCallback(async (nextSynopsis: string) => {
    verifySynopsisEdit();
    // Blur may save only the proposal already owned by this exact draft. A stale
    // panel callback cannot bind an earlier value to another selection or edit.
    if (currentSaveStateRef.current.synopsis !== nextSynopsis)
      throw new Error("Synopsis edits changed before saving. Keep the current proposal and review it again.");
    if (!(await saveNow(currentSaveStateRef.current.content)))
      throw new Error("Could not save the synopsis. Your draft edits remain open.");
  }, [verifySynopsisEdit, saveNow]);
  useEffect(() => {
    const draftDiscussionTarget =
      !loading && getCurrentLaunchMode() !== "local" && spaceId && docId && activeDraft?.source_digest && user?.id
        ? {
            spaceId,
            documentId: docId,
            draftId: activeDraft.id,
            sourceDigest: activeDraft.source_digest,
          }
        : null;
    const discussionScope = draftDiscussionTarget
      ? draftDiscussionScope(draftDiscussionTarget)
      : null;
    registerPageDetails({
      draftDiscussion:
        draftDiscussionTarget && user?.id
          ? {
              target: draftDiscussionTarget,
              currentUserId: user.id,
              pendingAnchor:
                draftCommentSelection?.scope === discussionScope
                  ? draftCommentSelection.anchor
                  : null,
              onAnchorConsumed: () =>
                setDraftCommentSelection((current) =>
                  current?.scope === discussionScope ? null : current,
                ),
            }
          : undefined,
      coverImage: doc?.cover_image,
      coverDraftId: activeDraft?.id,
      coverEditable: pageDetailsEditable,
      onCoverUpload: uploadCoverImage,
      nativeCoverRecovery,
      onCoverUpdate: updateCoverImage,
      pageWidth,
      pageWidthEditable: pageDetailsEditable,
      onPageWidthChange: handlePageWidthChange,
      contentMetadataEditable: pageDetailsEditable,
      pageValuesEditable,
      schemaEditable: pageValuesEditable,
      synopsis,
      pageCssSource: !loading && spaceId && docId && doc?.id === docId &&
        (!activeDraft || activeDraft.document_id === docId)
        ? { spaceId, documentId: docId, draftId: activeDraft?.id, provenance: cssProvenance }
        : undefined,
      onSynopsisChange: changeDraftSynopsis,
      onSynopsisCompositionChange: changeSynopsisComposition,
      onSynopsisUpdate: updateDraftSynopsis,
    });
    return () => registerPageDetails(null);
  }, [
    doc?.cover_image,
    activeDraft?.id,
    handlePageWidthChange,
    pageWidth,
    pageDetailsEditable,
    pageValuesEditable,
    registerPageDetails,
    updateCoverImage,
    uploadCoverImage,
    nativeCoverRecovery,
    synopsis,
    cssProvenance,
    loading,
    spaceId,
    docId,
    doc?.id,
    activeDraft,
    draftCommentSelection,
    user?.id,
    updateDraftSynopsis,
    changeDraftSynopsis,
    changeSynopsisComposition,
  ]);
  const localMode = getCurrentLaunchMode() === "local";
  const stats = getDocumentStats(content);
  const { createdBy, updatedBy } = getDocumentAttributionNames(doc, user, userDirectory);
  const documentDate = formatDocumentDate(doc?.updated_at || doc?.created_at);
  const metadataLine = localMode || pageOnlyViewer
    ? `Last modified ${documentDate} • ${stats.words} WORDS • ${stats.readingMinutes} MIN READ`
    : `Created by ${createdBy} • last modified by ${updatedBy} on ${documentDate} • ${stats.words} WORDS • ${stats.readingMinutes} MIN READ`;
  const commentsPanelOpen = rightPanel.open && rightPanel.activeTab === "comments";
  const inlineCommentsPanelOpen = commentsPanelOpen && rightPanel.commentPanelScope === "inline";
  const propertiesPanelOpen = rightPanel.open && rightPanel.activeTab !== "comments";
  const toggleCommentsPanel = () => {
    if (commentsPanelOpen && rightPanel.commentPanelScope === "all") {
      rightPanel.close();
      return;
    }
    rightPanel.openTab("comments");
  };
  const toggleInlineCommentsPanel = () => {
    if (inlineCommentsPanelOpen) {
      rightPanel.close();
      setInlineCommentsVisible(false);
      return;
    }
    setInlineCommentsVisible(true);
    rightPanel.openComment();
  };
  const documentActionLabels = getDocumentActionLabels(starred, propertiesPanelOpen);
  const discussionActionLabel = activeDraft?.source_digest ? "draft discussion" : "page comments";
  const desktopViewModes = pageOnlyViewer ? (["preview"] as const) : getDesktopDocumentViewModes(viewMode);
  const visibleViewModes = pageOnlyViewer ? (["preview"] as const) : getVisibleDocumentViewModes(viewMode);
  const pageWidthClass = getDocumentPageWidthClass(pageWidth);

  if (loading) {
    return <LoadingState label="Loading document" />;
  }

  if (!doc) {
    return (
      <div className="flex h-full items-center justify-center" role="alert">
        <div className="space-y-3 text-center">
          <p className="text-text-muted text-sm">
            This page is unavailable or you no longer have access.
          </p>
          {publicationRecords.filter((record) => ["prepared", "dispatched", "unknown", "confirmed"].includes(record.state)).map((record) => <div key={record.intent.operation_id} className="border-border bg-surface rounded border p-3">
            <p className="text-text-muted text-sm">Original publication evidence remains on this device. Access must be restored before it can be confirmed.</p>
            <p className="text-text-muted text-sm">Download the original saved draft retained on this device. This copy is not a publication receipt.</p>
            <Button variant="secondary" onClick={() => downloadOriginalSavedDraft(record.intent)}>Download original saved draft</Button>
            {record.state !== "prepared" && <Button variant="secondary" disabled={publicationDispatching} onClick={() => void dispatchPublication(record.intent, true)}>Retry original publication</Button>}
          </div>)}
          {recoveryCandidate && <Button variant="secondary" onClick={downloadRecovery}>Download device copy</Button>}
          {draftError && <p className="text-danger text-sm">{draftError}</p>}
          <Button
            variant="secondary"
            onClick={() => navigate(spaceId ? buildSpaceHomePath(spaceId) : "/")}
          >
            Return to space
          </Button>
        </div>
      </div>
    );
  }

  const desktopSaveStatus = activeDraft ? (
                  <span
                    className={getDocumentSaveStatusClass(status)}
                    aria-live="polite"
                    aria-atomic="true"
                  >
                    {peerBarrierRef.current
                      ? peerUnavailableRef.current ? "Draft unavailable · device work retained" : "Edits retained · review shared changes"
                      : synopsisSavePending && status !== "saving" && status !== "error" && !saveQueued
                      ? "Synopsis edits · save pending"
                      : draftHasChanges && status === "idle"
                      ? "Unsaved changes · saving shortly"
                      : status === "saving"
                        ? "Saving..."
                        : status === "saved"
                          ? saveQueued
                            ? "Saved on this device · waiting to sync"
                            : "Draft saved · unpublished"
                          : status === "error"
                            ? "Save failed · edits remain open"
                            : saveQueued
                              ? "Saved on this device · waiting to sync"
                              : "Draft stored · unpublished"}
                  </span>
  ) : null;
  const audienceContext = getCurrentLaunchMode() === "local"
    ? "Local page · stored on this device"
    : pageOnlyViewer ? "Published page · shared with you as viewer"
    : doc.public ? "Published audience: anyone with the public page link"
    : `Published audience: people with access to ${spaceName}`;

  const desktopDocumentControls = (
    <div role="toolbar" aria-label="Document controls" className="flex items-center gap-2">
      {headingCount >= 2 && !isSourceMode && (
        <button ref={contentsTriggerRef} type="button" onClick={() => setContentsOpen(value => !value)}
          data-right-panel-trigger="true" className={`${CONTROL_CHROME_ACTION_CLASS} focus-ring flex h-8 items-center gap-1.5 px-2`}
          aria-label="Contents" aria-pressed={contentsOpen} title="Contents">
          <List size={16} /><span className="hidden xl:inline text-xs">Contents</span>
        </button>
      )}
      <div
        role="group"
        aria-label="View mode"
        className={`${CONTROL_CHROME_RAIL_CLASS} overflow-hidden`}
      >
        {desktopViewModes.map((mode) => {
          const { label, icon: Icon } = getDocumentViewModeAction(mode, documentActionLabels);
          return (
            <button
              key={mode}
              type="button"
              onClick={() => setEditorViewMode(mode)}
              disabled={
                openingDraft ||
                publishing ||
                (mode !== "preview" && (pageEditBlocked || spaceRoleBlocksDraftEditing))
              }
              className={`${CONTROL_CHROME_ACTION_CLASS} ${getDocumentChromeActionTone("default", viewMode === mode)} flex h-8 items-center justify-center gap-1.5 px-2 ${
                viewMode === mode ? CONTROL_CHROME_ACTIVE_ACTION_CLASS : ""
              }`}
              aria-label={label}
              aria-pressed={viewMode === mode}
              title={label}
            >
              <Icon size={14} aria-hidden="true" />
              <span className="text-xs">{mode === "preview" ? "Read" : "Edit"}</span>
            </button>
          );
        })}
      </div>
      <div className={`${CONTROL_CHROME_RAIL_CLASS} overflow-hidden`}>
        {activeDraft?.can_publish && (
          <button
            type="button"
            onClick={() => void publishActiveDraft()}
            disabled={publishing || openingDraft || pageEditBlocked || Boolean(recoveryCandidate)}
            className={`${CONTROL_CHROME_ACTION_CLASS} ${getDocumentChromeActionTone("primary")} flex h-8 items-center justify-center gap-1.5 px-2 text-xs`}
            aria-label={publishing ? "Publishing changes" : "Publish changes"}
            title={publishing ? "Publishing changes" : "Publish changes"}
          >
            <Send size={14} aria-hidden="true" />
            <span>{publishing ? "Publishing…" : "Publish changes"}</span>
          </button>
        )}
        {!localMode && !pageOnlyViewer && (
          <button
            type="button"
            onClick={toggleCommentsPanel}
            data-right-panel-trigger="true"
            className={`${CONTROL_CHROME_ACTION_CLASS} ${getDocumentChromeActionTone("default", commentsPanelOpen)} flex h-8 w-8 items-center justify-center text-xs ${
              commentsPanelOpen ? CONTROL_CHROME_ACTIVE_ACTION_CLASS : ""
            }`}
            aria-label={`${commentsPanelOpen ? "Hide" : "Show"} ${discussionActionLabel}`}
            aria-pressed={commentsPanelOpen}
            title={`${commentsPanelOpen ? "Hide" : "Show"} ${discussionActionLabel}`}
          >
            <MessageSquare size={14} />
          </button>
        )}
        {!pageOnlyViewer && <button
          type="button"
          onClick={() => rightPanel.toggleTab("properties")}
          data-right-panel-trigger="true"
          className={`${CONTROL_CHROME_ACTION_CLASS} ${getDocumentChromeActionTone("default", propertiesPanelOpen)} flex h-8 w-8 items-center justify-center text-xs ${
            propertiesPanelOpen ? CONTROL_CHROME_ACTIVE_ACTION_CLASS : ""
          }`}
          aria-label={documentActionLabels[5]}
          aria-pressed={propertiesPanelOpen}
          title={documentActionLabels[5]}
        >
          <SlidersHorizontal size={14} />
        </button>}
        {!pageOnlyViewer && <DocumentActionsMenu
          starred={starred}
          sourceActive={isSourceMode}
          inlineCommentsActive={inlineCommentsPanelOpen}
          localMode={localMode}
          includeSourceAction
          onToggleStarred={toggleStarred}
          onOpenDrafts={openDraftChoices}
          onShare={() => setShareOpen(true)}
          onOpenSource={() => setEditorViewMode("source")}
          onToggleInlineComments={activeDraft ? undefined : toggleInlineCommentsPanel}
          onMove={openMoveDialog}
          onDuplicate={() => void duplicateDocument()}
          onSaveAsTemplate={
            localMode ||
            user?.role === "admin" ||
            ["owner", "editor", "admin", "space_admin"].includes(spaceMemberRole ?? "")
              ? () => setSaveAsTemplateOpen(true)
              : undefined
          }
          onArchive={() => void archiveDocument()}
          onDelete={() => {
            setDeleteError("");
            setDeleteConfirmOpen(true);
          }}
        />}
      </div>
    </div>
  );

  return (
    <div className={`flex h-full min-h-0 flex-col ${!mobileLayout ? "workspace-document-page" : ""}`}>
      {saveAsTemplateOpen && spaceId && docId && (
        <SaveAsTemplateDialog
          key={`${spaceId}:${docId}`}
          spaceId={spaceId}
          documentId={docId}
          spaceName={spaceName}
          onClose={() => setSaveAsTemplateOpen(false)}
          onSaved={(id) => {
            setSaveAsTemplateOpen(false);
            navigate(getTemplateEditorPath(spaceId, id));
          }}
        />
      )}
      {customCss && <style data-starlane-document-css>{customCss}</style>}
      {!mobileLayout && (
        <header className="workspace-document-header border-border bg-bg shrink-0 border-b" data-document-ui="header">
          <div className="flex min-h-12 items-center justify-between gap-3 px-5 py-2">
            <div className="min-w-0 flex-1">{pageOnlyViewer ? <span className="text-text text-sm">{title}</span> : <Suspense fallback={null}>
              <Breadcrumb spaceName={spaceName} folderPath={folderPath}
                parentPages={parentPath.map(page => ({...page, href: buildDocumentRoutePath(spaceId ?? "", page.id)}))}
                title={title} spaceHref={buildDocumentSpaceBreadcrumbHref(spaceId)} className="mb-0" />
            </Suspense>}</div>
            {desktopDocumentControls}
          </div>
          <div className="text-text-muted flex flex-wrap items-center justify-between gap-x-4 gap-y-1 px-5 pb-2 text-xs">
            <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
              {activeDraft ? <DocumentDraftStatus accessRole={activeDraft.access_role} canPublish={activeDraft.can_publish} /> : <span>Read · {doc.published_at ? "Published page" : "Unpublished page"}</span>}
              {desktopSaveStatus}
              {activeDraft && !localMode && user?.id && <DraftPresence
                peers={peerCurrentRef.current() ? draftPresence?.peers ?? [] : []}
                currentUserId={user.id} ownConnectionId={draftPresence?.connection_id ?? null} status={draftPresenceStatus}
              />}
            </div>
            <span>{audienceContext}{activeDraft ? " · Publishing does not change access" : ""}</span>
          </div>
        </header>
      )}
      {!mobileLayout && isMarkdownMode && draftWritable && !pageEditBlocked && (
        <div ref={setDesktopToolbarHost} className="workspace-document-toolbar border-border bg-bg shrink-0 border-b" data-document-ui="toolbar" />
      )}
      <div className="workspace-document-region relative min-h-0 flex-1" data-inspector-open={rightPanel.open ? "true" : undefined} data-contents-eligible={headingCount >= 2 && !isSourceMode ? "true" : undefined}>
      {/* Editor content area */}
      <div
        ref={dropRef}
        className="starlane-document-viewport relative min-h-0 flex-1 overflow-y-auto"
      >
        {/* Drop overlay */}
        {isDragging && (
          <div className="bg-bg/80 absolute inset-0 z-50 flex items-center justify-center">
            <p className="text-text-muted text-lg font-medium">Drop file to attach</p>
          </div>
        )}

        <div
          data-document-page-width={pageWidth}
          data-document-editing={isMarkdownMode ? "true" : undefined}
          className={`starlane-document-scope mx-auto w-full px-4 pt-5 pb-2 sm:px-6 sm:py-6 xl:py-6 ${pageWidthClass} ${getDocumentPageBottomPaddingClass(mobileLayout, viewMode)}`}
        >
          {mobileLayout && <div className="mb-3 min-w-0">
            {pageOnlyViewer ? <span className="text-text text-sm">{title}</span> : <Suspense fallback={null}>
              <Breadcrumb
                spaceName={spaceName}
                folderPath={folderPath}
                parentPages={parentPath.map((page) => ({
                  ...page,
                  href: buildDocumentRoutePath(spaceId ?? "", page.id),
                }))}
                title={title}
                spaceHref={buildDocumentSpaceBreadcrumbHref(spaceId)}
                className="mb-0"
              />
            </Suspense>}
          </div>}

          {mobileLayout && activeDraft && !localMode && user?.id && <div className="mb-3 flex min-w-0 flex-wrap">
            <DraftPresence peers={peerCurrentRef.current() ? draftPresence?.peers ?? [] : []}
              currentUserId={user.id} ownConnectionId={draftPresence?.connection_id ?? null} status={draftPresenceStatus} />
          </div>}
          {mobileLayout && (
            <div className="mb-3">
              <div className={MOBILE_DOCUMENT_CONTROL_RAIL_CLASS}>
                <div
                  role="toolbar"
                  aria-label="Document controls"
                  className={`${CONTROL_CHROME_RAIL_CLASS} flex w-max min-w-max overflow-hidden`}
                >
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
                        className={`${CONTROL_CHROME_ACTION_CLASS} ${getDocumentChromeActionTone("default", viewMode === mode)} flex h-11 w-11 shrink-0 items-center justify-center ${
                          viewMode === mode ? CONTROL_CHROME_ACTIVE_ACTION_CLASS : ""
                        }`}
                        aria-label={label}
                        aria-pressed={viewMode === mode}
                        title={label}
                      >
                        <Icon size={16} />
                      </button>
                    );
                  })}
                  {activeDraft?.can_publish && (
                    <button
                      type="button"
                      onClick={() => void publishActiveDraft()}
                      disabled={
                        publishing || openingDraft || pageEditBlocked || Boolean(recoveryCandidate)
                      }
                      className={`${CONTROL_CHROME_ACTION_CLASS} ${getDocumentChromeActionTone("primary")} flex h-11 w-11 shrink-0 items-center justify-center`}
                      aria-label={publishing ? "Publishing changes" : "Publish changes"}
                      title={publishing ? "Publishing changes" : "Publish changes"}
                    >
                      <Send size={16} />
                    </button>
                  )}
                  {!localMode && !pageOnlyViewer && (
                    <button
                      type="button"
                      onClick={toggleCommentsPanel}
                      data-right-panel-trigger="true"
                      className={`${CONTROL_CHROME_ACTION_CLASS} ${getDocumentChromeActionTone("default", commentsPanelOpen)} flex h-11 w-11 shrink-0 items-center justify-center ${
                        commentsPanelOpen ? CONTROL_CHROME_ACTIVE_ACTION_CLASS : ""
                      }`}
                      aria-label={`${commentsPanelOpen ? "Hide" : "Show"} ${discussionActionLabel}`}
                      aria-pressed={commentsPanelOpen}
                      title={`${commentsPanelOpen ? "Hide" : "Show"} ${discussionActionLabel}`}
                    >
                      <MessageSquare size={16} />
                    </button>
                  )}
                  {!pageOnlyViewer && <button
                    type="button"
                    onClick={() => rightPanel.toggleTab("properties")}
                    data-right-panel-trigger="true"
                    className={`${CONTROL_CHROME_ACTION_CLASS} ${getDocumentChromeActionTone("default", propertiesPanelOpen)} flex h-11 w-11 shrink-0 items-center justify-center ${
                      propertiesPanelOpen ? CONTROL_CHROME_ACTIVE_ACTION_CLASS : ""
                    }`}
                    aria-label={documentActionLabels[5]}
                    aria-pressed={propertiesPanelOpen}
                    title={documentActionLabels[5]}
                  >
                    <SlidersHorizontal size={16} />
                  </button>}
                  {!pageOnlyViewer && <DocumentActionsMenu
                    starred={starred}
                    sourceActive={isSourceMode}
                    inlineCommentsActive={inlineCommentsPanelOpen}
                    localMode={localMode}
                    includeSourceAction={!isSourceMode}
                    touchSafe
                    onToggleStarred={toggleStarred}
                    onOpenDrafts={openDraftChoices}
                    onShare={() => setShareOpen(true)}
                    onOpenSource={() => setEditorViewMode("source")}
                    onToggleInlineComments={activeDraft ? undefined : toggleInlineCommentsPanel}
                    onMove={openMoveDialog}
                    onDuplicate={() => void duplicateDocument()}
                    onSaveAsTemplate={
                      localMode ||
                      user?.role === "admin" ||
                      ["owner", "editor", "admin", "space_admin"].includes(spaceMemberRole ?? "")
                        ? () => setSaveAsTemplateOpen(true)
                        : undefined
                    }
                    onArchive={() => void archiveDocument()}
                    onDelete={() => {
                      setDeleteError("");
                      setDeleteConfirmOpen(true);
                    }}
                  />}
                </div>
              </div>
            </div>
          )}

          <Suspense fallback={null}>
            <CoverImage
              spaceId={spaceId}
              documentId={docId}
              src={doc.cover_image}
              draftId={activeDraft?.id}
              editable={false}
              showEmptyPlaceholder={false}
              onUpdate={() => undefined}
            />
          </Suspense>

          {mobileLayout && activeDraft ? (
            <DocumentDraftStatus
              accessRole={activeDraft.access_role}
              canPublish={activeDraft.can_publish}
            />
          ) : null}
          {mobileLayout && <p className="text-text-muted mb-2 text-xs">
            {audienceContext}
            {activeDraft ? " · Publishing changes does not change access" : ""}
          </p>}

          {/* Title */}
          <div className={DOCUMENT_TITLE_ROW_CLASS}>
            {metadataEditable ? (
              <div
                className={
                  doc.icon
                    ? DOCUMENT_TITLE_ICON_GUTTER_CLASS
                    : DOCUMENT_TITLE_EMPTY_ICON_GUTTER_CLASS
                }
              >
                <Suspense fallback={<div className="size-12 shrink-0" aria-hidden="true" />}>
                  <PageIcon
                    icon={doc.icon}
                    onUpdate={async (icon) => {
                      if (
                        !spaceId ||
                        !docId ||
                        !activeDraft ||
                        activeDraft.access_role === "viewer"
                      )
                        return;
                      checkpointDraft({ icon: icon ?? null });
                      const previousIcon = doc.icon;
                      setDoc((current) => (current ? { ...current, icon } : current));
                      if (peerBarrierRef.current) { setDraftHasChanges(true); return; }
                      try {
                        const updated = await saveActiveDraft(
                          { icon: icon ?? "" },
                        );
                        if (routeIdentityRef.current !== `${spaceId}/${docId}` || confirmedDraftRef.current?.id !== updated.id) return;
                        setSaveQueued(false);
                        updateStarredDocumentMetadata({
                          spaceId,
                          id: docId,
                          title,
                          spaceName,
                          icon,
                          updated_at: doc?.updated_at,
                        });
                      } catch (error) {
                        if (!peerBarrierRef.current) setDoc((current) =>
                          current && current.icon === icon
                            ? { ...current, icon: previousIcon }
                            : current,
                        );
                        throw error;
                      }
                    }}
                  />
                </Suspense>
              </div>
            ) : doc.icon ? (
              <div className={DOCUMENT_TITLE_ICON_GUTTER_CLASS}>
                <span className="text-text flex size-12 shrink-0 items-center justify-center">
                  <ContentIdentityIcon value={doc.icon} size={28} />
                </span>
              </div>
            ) : null}
            <div className="w-full min-w-0 flex-1">
              {isMarkdownMode || isSourceMode ? (
                <>
                  <label htmlFor="document-title" className="sr-only">
                    Page title
                  </label>
                  <DocumentTitleTextarea
                    ref={titleRef}
                    title={title}
                    locked={pageEditBlocked}
                    readOnly={!draftWritable}
                    onTitleChange={(value) => {
                      checkpointDraft({ title: value });
                      setTitle(value);
                      setDraftHasChanges(true);
                      scheduleAutoSave(content);
                    }}
                    onCommit={handleTitleChange}
                  />
                </>
              ) : (
                <h1 className="document-title text-text py-1 leading-tight font-bold [overflow-wrap:anywhere] break-words">
                  {title || "Untitled"}
                </h1>
              )}
              {!pageOnlyViewer && spaceId && docId && <div className="mt-2 flex min-w-0 flex-wrap items-center gap-1.5">
                <PageResponsibilitySummary key={`${spaceId}/${docId}`} spaceId={spaceId} docId={docId} refreshKey={propertiesPanelOpen} />
              </div>}
              {!isMarkdownMode && !isSourceMode ? (
                <div className="text-text-muted mt-2 flex flex-wrap items-center gap-1.5 text-xs">
                  <span>{metadataLine}</span>
                </div>
              ) : activeDraft && mobileLayout ? (
                <div className="text-text-muted mt-2 flex flex-wrap items-center gap-1.5 text-xs">
                  <span
                    className={getDocumentSaveStatusClass(status)}
                    aria-live="polite"
                    aria-atomic="true"
                  >
                    {peerBarrierRef.current
                      ? peerUnavailableRef.current ? "Draft unavailable · device work retained" : "Edits retained · review shared changes"
                      : synopsisSavePending && status !== "saving" && status !== "error" && !saveQueued
                      ? "Synopsis edits · save pending"
                      : draftHasChanges && status === "idle"
                      ? "Unsaved changes · saving shortly"
                      : status === "saving"
                        ? "Saving..."
                        : status === "saved"
                          ? saveQueued
                            ? "Saved on this device · waiting to sync"
                            : "Draft saved · unpublished"
                          : status === "error"
                            ? "Save failed · edits remain open"
                            : saveQueued
                              ? "Saved on this device · waiting to sync"
                              : "Draft stored · unpublished"}
                  </span>
                </div>
              ) : null}
              {!isMarkdownMode && synopsis.trim() ? (
                <p className="text-text-muted mt-3 text-sm leading-6">{synopsis}</p>
              ) : null}
            </div>
          </div>

          {publicationRecords.filter((record) => record.state === "confirmed" && record.intent.draft_id === requestedDraftId).map((record) => <div key={record.intent.operation_id} role="status" className="border-border bg-surface mb-4 rounded border p-4">
            <p className="text-text font-medium">Original publication confirmed</p><p className="text-text-muted mt-1 text-sm">Its receipt confirms the saved source. Current device edits and later published revisions remain separate.</p><p className="text-text-muted mt-1 text-sm">Download the original saved draft retained on this device. This copy is not a publication receipt.</p><div className="mt-3 flex flex-wrap gap-2"><Button variant="secondary" onClick={() => downloadOriginalSavedDraft(record.intent)}>Download original saved draft</Button><Button variant="secondary" disabled={publicationDispatching} onClick={() => void dispatchPublication(record.intent, true)}>Review original publication receipt</Button><Button variant="secondary" disabled={publicationDispatching} onClick={reloadPublishedPage}>View current published page</Button></div>
          </div>)}
          {publicationRecords.filter((record) => ["prepared", "dispatched", "unknown"].includes(record.state)).map((record) => <div key={record.intent.operation_id} role="status" className="border-border bg-surface mb-4 rounded border p-4">
            <p className="text-text font-medium">Original publication retained</p><p className="text-text-muted mt-1 text-sm">{record.state === "prepared" ? "This saved draft is ready for review. It has not been dispatched." : "The original outcome needs confirmation. Retry uses the same saved source and request, without saving or publishing a newer draft."}</p>
            <p className="text-text-muted mt-1 text-sm">Download the original saved draft retained on this device. This copy is not a publication receipt.</p>
            <div className="mt-3 flex flex-wrap gap-2"><Button variant="secondary" onClick={() => downloadOriginalSavedDraft(record.intent)}>Download original saved draft</Button><Button disabled={publicationDispatching || pageEditBlocked} onClick={() => {
              if (record.state === "prepared") { publicationBoundaryRef.current = { route: `${spaceId}/${docId}`, generation: draftSelectionGenerationRef.current, account: recoveryAccountRef.current, canvas: JSON.stringify(currentSaveStateRef.current) }; setPublicationReview(record.intent); }
              else void dispatchPublication(record.intent, true);
            }}>{record.state === "prepared" ? "Review original publication" : "Retry original publication"}</Button></div>
          </div>)}
          {publicationReview && <DocumentDraftPublicationReview publication={publicationReview.expectedPublication} pagePublished={publicationReview.source.page_published} cancelLabel={publicationRecords.find((record) => record.intent.operation_id === publicationReview.operation_id)?.state === "prepared" ? "Keep draft" : "Close review"} confirmBlocked={pageEditBlocked ? pageEditExplanation : publicationRecords.find((record) => record.intent.operation_id === publicationReview.operation_id)?.state === "prepared" ? undefined : "Close this review to inspect the retained outcome. A rejected request needs a new source review; an uncertain request uses Retry original publication."} busy={publicationDispatching} error={draftError} onConfirm={() => void dispatchPublication(publicationReview)} onCancel={() => void cancelPublicationReview()} />}
          {peerNotice && <section role="status" aria-label="Shared draft changes" className="border-border bg-surface mb-4 rounded border p-4">
            <p className="text-text font-medium">{peerNotice === "unavailable" ? "Selected draft unavailable" : peerNotice === "reconnecting" ? "Shared draft connection needs review" : "Shared draft changed"}</p>
            <p className="text-text-muted mt-1 text-sm">{peerNotice === "unavailable" ? "Access, lock or draft availability changed. Your device work stays here; another draft has not been opened." : peerNotice === "reconnecting" ? "Saving is paused until this selected draft can be inspected again. Current edits and original requests stay separate; keep this page open if the device recovery copy is unavailable." : "Your edits keep their original source. Keep typing, then review the newest device proposal against the current draft before saving."}</p>
            <div className="mt-3 flex flex-wrap gap-2">
              {peerNotice === "changed" && <Button ref={peerReviewTriggerRef} disabled={recoveryBusy || publishing || openingDraft || peerPendingSavesRef.current > 0 || Boolean(recoveryCandidate)} onClick={reviewLivePeerChanges}>Review shared changes</Button>}
              <Button variant="secondary" onClick={downloadCurrentDeviceCopy}>Download current device copy</Button>
              <Button variant="secondary" disabled={publishing || openingDraft} onClick={() => { if (peerNotice === "changed") peerTransportRef.current?.resync(); else setPeerReconnect(value => value + 1); }}>{peerNotice === "changed" ? "Check current draft" : "Retry selected draft connection"}</Button>
            </div>
          </section>}
          {peerUpdated && !peerNotice && <div role="status" className="border-border bg-surface text-text-muted mb-4 flex flex-wrap items-center gap-2 rounded border px-3 py-2 text-sm">
            <p>{peerUpdated === "body" ? "Shared draft refreshed. The rich-text editor starts a new undo history." : "Shared draft presentation refreshed."}</p>
            <Button size="sm" variant="ghost" onClick={() => setPeerUpdated(null)}>Dismiss</Button>
          </div>}
          {replacementRecords.length > 0 && <section aria-label="Retained reviewed draft sources" className="border-border bg-surface mb-4 rounded border p-4">
            <p className="text-text font-medium">Reviewed draft sources retained on this device</p>
            <p className="text-text-muted mt-1 text-sm">These records keep both the device proposal and the exact draft shown before a reviewed replacement. JSON downloads are retained evidence, not an import or publication receipt.</p>
            <div className="mt-3 flex flex-wrap gap-2">{replacementRecords.map(record => <div className="flex flex-wrap gap-2" key={record.intent.checkpoint.snapshotId + record.intent.reviewedDraft.source_digest}>
              <Button variant="secondary" onClick={() => downloadReplacementRecord(record)}>Download reviewed sources · {record.state === "confirmed" ? "replacement confirmed" : "outcome retained"}</Button>
              {record.previousArchiveKey && <Button variant="secondary" onClick={() => inspectPreviousReplacement(record)}>Inspect earlier retained sources</Button>}
            </div>)}</div>
            {replacementArchive && <div className="border-border mt-3 border-t pt-3">
              <p className="text-text-muted break-words text-sm [overflow-wrap:anywhere]">Earlier reviewed source · {replacementArchive.intent.reviewedDraft.title}</p>
              <div className="mt-2 flex flex-wrap gap-2"><Button variant="secondary" onClick={() => downloadReplacementRecord(replacementArchive)}>Download earlier reviewed sources</Button>
                {replacementArchive.previousArchiveKey && <Button variant="secondary" onClick={() => inspectPreviousReplacement(replacementArchive)}>Inspect next earlier sources</Button>}
                <Button variant="ghost" onClick={() => setReplacementArchive(null)}>Close earlier source</Button></div>
            </div>}
          </section>}
          {replacementHistoryError && <p role="alert" className="text-danger mb-4 text-sm">{replacementHistoryError}</p>}
          {recoveryCandidate && recoveryScope && (
            <div role="status" className="border-border bg-surface rounded-card mb-4 border p-4">
              <p className="text-text font-medium">Unsaved edits found on this device</p>
              <p className="text-text-muted mt-1 text-sm">Compare the retained edits with the loaded draft before choosing a separate private draft or a reviewed replacement. Recovery does not publish.</p>
              <div className="mt-3 flex flex-wrap gap-2">
                <Button disabled={recoveryBusy || publishing || openingDraft} onClick={() => void reviewRecovery()}>Review retained edits</Button>
                <Button variant="secondary" onClick={downloadRecovery}>Download device copy</Button>
                <Button variant="secondary" disabled={recoveryBusy} onClick={() => setRecoveryDiscardConfirm(true)}>Keep loaded draft · discard device copy…</Button>
              </div>
              {recoveryReviewError && <p role="alert" className="text-danger mt-3 text-sm">{recoveryReviewError}</p>}
            </div>
          )}
          {heldQueueItems.length > 0 && <section aria-label="Held original queued saves" className="border-border bg-surface-raised mt-4 rounded border p-4">
            <p className="text-text font-medium">Original queued saves need review</p>
            <p className="text-text-muted mt-1 text-sm">Unlocking, reconnecting and reloading do not send these requests. Keep their original bytes while you review current source and access.</p>
            <div className="mt-3 flex flex-wrap gap-2">{heldQueueItems.map(item=><Button key={item.id} variant="secondary" disabled={heldQueueBusy || recoveryBusy || publishing || openingDraft || publicationPending} onClick={()=>void reviewHeldQueueSave(item.id)}>Review {item.replayState === "unknown" ? "uncertain" : "held"} save · {new Date(item.queuedAt).toLocaleString()}</Button>)}</div>
            <div className="mt-3 flex flex-wrap gap-2">{heldQueueItems.map(item=><Button key={item.id} variant="secondary" onClick={()=>downloadHeldQueueRequest(item)}>Download original request · {new Date(item.queuedAt).toLocaleString()}</Button>)}</div>
            {heldQueueError && !heldQueueReview && <p role="alert" className="text-danger mt-3 text-sm">{heldQueueError}</p>}
          </section>}
          {heldQueueReview && <DocumentHeldDraftSaveReview review={heldQueueReview.review} busy={heldQueueBusy} error={heldQueueError}
            retryBlocked={publicationPending ? "Review the retained publication before retrying this save." : undefined}
            onRetry={()=>void retryHeldQueueSave()} onKeep={()=>{if(!heldQueueBusyRef.current){setHeldQueueReview(null);setHeldQueueError("");}}}
            onDownload={()=>downloadHeldQueueRequest(heldQueueReview.review.original)} onSeparateRecovery={()=>void prepareQueuedRecoveryCopy()}
          />}
          {heldQueueError && heldQueueItems.length===0 && !heldQueueReview && !queuedCopyReview && <p role="alert" className="text-danger mt-3 text-sm">{heldQueueError}</p>}
          {queuedCopyRecords.length>0 && <section aria-label="Retained queued recovery proposals" className="border-border bg-surface-raised mt-4 rounded border p-4"><p className="text-text font-medium">Retained recovery proposals</p><p className="text-text-muted mt-1 text-sm">These keep their original raw composition and operation evidence, independently of queued saves and device checkpoints.</p><div className="mt-3 flex flex-wrap gap-2">{queuedCopyRecords.map(record=><Button key={record.original.id} variant="secondary" disabled={heldQueueBusy||recoveryBusy||publishing} onClick={()=>{try{openQueuedCopyReview(record);}catch(error){setHeldQueueError(error instanceof Error?error.message:"Original proposal is unavailable.");}}}>Review retained recovery proposal</Button>)}</div></section>}
          {queuedCopyReview && <DocumentQueuedDraftCopyReview record={queuedCopyReview.record} operation={queuedCopyOperation} busy={heldQueueBusy} error={heldQueueError}
            onCreate={()=>void createQueuedRecoveryCopy()} restartReview={queuedCopyRestartIntent?.reviewedDraft}
            onReviewRejectedBirth={queuedCopyOperation?.state==="birth-held"?()=>void reviewRejectedQueuedCopyBirth():undefined}
            onRestartRejectedBirth={queuedCopyRestartIntent?()=>void restartReviewedQueuedCopyBirth():undefined} targetReview={queuedCopyTargetReview} onReviewTarget={queuedCopyOperation?.target?()=>void reviewQueuedCopyTarget():undefined}
            onReplaceTarget={queuedCopyOperation?.state==="save-rejected"&&queuedCopyTargetReview?()=>void replaceQueuedCopyTarget():undefined} onClose={()=>{if(!heldQueueBusyRef.current){setQueuedCopyReview(null);setHeldQueueError("");}}}
            onDownload={()=>downloadQueuedCopy(queuedCopyReview.record)}
            onOpenTarget={queuedCopyOperation?.target ? ()=>{const target=queuedCopyOperation.target!;window.open(`${buildDocumentRoutePath(target.space_id,target.document_id)}?draft=${encodeURIComponent(target.id)}`,"_blank","noopener,noreferrer");}:undefined}
          />}
          {recoveryReviewOpen && recoveryReviewDraft && recoveryCandidate && (
            <DocumentDraftRecoveryReview
              checkpoint={recoveryCandidate}
              loaded={recoveryReviewContent(recoveryTargetReview ?? recoveryReviewDraft)}
              provenance={compareDocumentDraftRecoverySource(recoveryCandidate, recoveryReviewDraft)}
              busy={recoveryBusy} error={recoveryReviewError} loadedUnavailable={recoveryReviewUnavailable && !recoveryTargetReview}
              replacingRecoveryTarget={Boolean(recoveryTargetReview)} restartCopy={recoveryRestartReady}
              replaceBlocked={recoveryCompositionHoldReason(recoveryCandidate) ?? (pageEditBlocked ? pageEditExplanation : recoveryTargetReview ? recoveryTargetHeld ? "Recovery save outcome is uncertain or queued. Keep its evidence; this target cannot be replaced until the original request is reviewed." : undefined : recoveryScope?.storageMode === "local" ? "Use a separate recovery copy for local drafts; reviewed replacement requires server source protection." : recoveryReviewUnavailable ? "The original draft is unavailable and cannot be replaced." : retainedOriginalSave() || draftSaveHoldRef.current?.reason === "outcome_unknown" || draftSaveHoldRef.current?.reason === "queued" ? "Original save is queued or uncertain. Keep its evidence; recover into a separate draft instead of replacing this draft." : pageEditBlocked ? pageEditExplanation : undefined)}
              copyBlocked={recoveryCompositionHoldReason(recoveryCandidate) ?? (recoveryTargetReview ? "A separate recovery draft already exists. Review that target instead of creating another copy." : pageEditBlocked ? pageEditExplanation : recoveryReviewUnavailable && !(selectedDraftOnly ? selectedDraftPagePublished : doc.published_at) ? "Unpublished recovery needs an accessible editable draft source. Download the device copy while you review access." : undefined)}
              onClose={closeRecoveryReview}
              onCopy={() => void recoverReviewedDraft("copy", recoveryRestartReady)}
              onReplace={() => { const reason = recoveryCompositionHoldReason(recoveryCandidate); if (reason) { setRecoveryReviewError(reason); return; } setRecoveryReviewOpen(false); setRecoveryReplaceConfirm(true); }} onDownload={downloadRecovery}
            />
          )}
          {recoveryDiscardConfirm && recoveryScope && recoveryCandidate && <ConfirmDialog labelledBy="draft-recovery-discard-title" title="Discard this device copy?" message="This removes the retained edits shown on this device. The loaded draft and any queued or uncertain requests remain unchanged. Download the device copy first if you want to keep it." confirmLabel="Discard device copy" destructive onCancel={() => setRecoveryDiscardConfirm(false)} onConfirm={() => {
            setRecoveryDiscardConfirm(false);
            if (discardDocumentDraftRecovery(recoveryScope, recoveryCandidate.snapshotId)) setRecoveryCandidate(null);
            else { setRecoveryCandidate(readDocumentDraftRecovery(recoveryScope)); setDraftError("Device checkpoint changed or could not be removed. Review the retained copy again."); }
          }} />}
          {recoveryReplaceConfirm && <ConfirmDialog labelledBy="draft-recovery-replace-title" title={recoveryTargetReview ? "Replace this reviewed recovery draft?" : "Replace this reviewed draft?"} message={recoveryTargetReview ? "This saves your device copy into the existing recovery draft against the source shown in this review. Later changes are rejected. Your original draft, published page and audience stay unchanged." : "This replaces the reviewed draft content and presentation with your device copy. Other collaborators' changes after this review will be rejected. The published page and its audience stay unchanged."} confirmLabel={recoveryTargetReview ? "Replace reviewed recovery draft" : "Replace reviewed draft"} destructive disabled={recoveryBusy} onCancel={() => { setRecoveryReplaceConfirm(false); setRecoveryReviewOpen(true); }} onConfirm={() => { setRecoveryReplaceConfirm(false); setRecoveryReviewOpen(true); void recoverReviewedDraft("replace"); }} />}
          {fileDropError && <p role="alert" className="text-warning mb-4 text-sm">{fileDropError}</p>}
          {recoveryStorageFailed && draftHasChanges && (
            <div role="alert" className="text-danger mb-4 grid gap-2 text-sm">
              <p>This device could not keep a recovery copy. Keep this page open until you save or download your edits.</p>
              <Button variant="secondary" className="justify-self-start" onClick={downloadCurrentDeviceCopy}>Download unsaved device copy</Button>
            </div>
          )}
          {birthStatus?.route === `${spaceId}/${docId}` && (
            <div className="border-border bg-surface-raised text-text-muted mb-4 rounded-md border px-3 py-2 text-sm" role="status">
              <p>{birthStatus.message}</p>
            </div>
          )}
          {draftError && (
            <div
              className="border-danger/30 bg-danger/5 text-danger mb-4 rounded-md border px-3 py-2 text-sm"
              role="alert"
            >
              <p>{draftError}</p>
              <div className="mt-2 flex flex-wrap gap-2">
                {birthRetry && birthRetry.route === `${spaceId}/${docId}` && (
                  <Button variant="secondary" disabled={openingDraft || publishing} onClick={() => void runDraftAction(() => startDraft(false, false, birthRetry))}>
                    Retry original draft request
                  </Button>
                )}
                <Button
                  variant="secondary"
                  disabled={openingDraft || publishing}
                  onClick={() => {
                    if (activeDraft) void saveNow(content);
                    else window.location.reload();
                  }}
                >
                  {activeDraft ? "Retry saving" : "Retry loading"}
                </Button>
                {requestedDraftId ? (
                  <Button
                    variant="secondary"
                    onClick={() => {
                      const query = new URLSearchParams(location.search);
                      query.delete("draft");
                      void runDraftAction(async () => {
                        navigate(
                          { pathname: location.pathname, search: query.toString() },
                          { replace: true },
                        );
                      });
                    }}
                  >
                    Read page
                  </Button>
                ) : draftListUnavailable ? (
                  <Button variant="secondary" onClick={() => setEditorViewMode("markdown")}>
                    Edit your draft
                  </Button>
                ) : null}
              </div>
            </div>
          )}
          {mobileLayout && viewMode === "preview" && (
            <Suspense fallback={null}>
              <DocumentOutlineAccordion content={content} />
            </Suspense>
          )}
          {selectedDraftOnly && <p role="status" className="border-border bg-surface text-text-muted mb-4 rounded border px-3 py-2 text-sm">You are editing the selected private draft. {selectedDraftPagePublished ? selectedDraftViewStatus === "denied" ? "This page has a published revision, but its published view is unavailable with your current access." : "This page has a published revision. Its published view has not been loaded in this draft selection." : "This page is unpublished."} Saving this draft does not publish it.</p>}
          {(doc.locked || pageEditBlocked && getCurrentLaunchMode() === "server") && (
            <div className="rounded-input border-accent/20 bg-accent/5 text-accent mb-4 border px-3 py-2 text-sm">
              {doc.locked && pageEditCapability.canEditPage ? "This page is locked by you. Your current Edit access allows changes." : pageEditExplanation}
              {getCurrentLaunchMode() === "server" && <Button size="sm" variant="secondary" className="mt-2" disabled={ownerEditRefreshing} onClick={() => void refreshOwnerEdit()}>{ownerEditRefreshing ? "Verifying Edit access…" : "Refresh Edit access"}</Button>}
              {ownerEditError && <p role="alert" className="mt-2">{ownerEditError}</p>}
            </div>
          )}

          <div ref={setDocumentBody} data-document-editor-surface="true">
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
                  readOnly={!draftWritable}
                  autoFocus={focusEditorOnMount && draftEditable && !pageEditBlocked}
                  showBlockChrome={!mobileLayout}
                  showMobileCommandButton={mobileLayout}
                  desktopToolbarHost={!mobileLayout ? desktopToolbarHost : null}
                  templates={templates}
                  templateContext={{ title, space: spaceName, author: user?.display_name }}
                  assetContext={assetContext}
                  coverAttachmentReference={doc.cover_image}
                  attachmentDraftId={activeDraft?.id}
                  spaceId={spaceId}
                  docId={docId}
                  inlineCommentsVisible={!activeDraft && !mobileLayout && inlineCommentsVisible}
                  onDraftCommentRequested={activeDraft?.source_digest ? requestDraftComment : undefined}
                  onCommentCreated={() => {
                    setInlineCommentsVisible(true);
                    rightPanel.openComment();
                  }}
                  onInlineCommentOpen={(commentId) => {
                    setInlineCommentsVisible(true);
                    rightPanel.openComment(commentId);
                  }}
                  onDocumentNavigate={
                    spaceId
                      ? (targetDocumentId) =>
                          navigate(buildDocumentRoutePath(spaceId, targetDocumentId))
                      : undefined
                  }
                  onEditorReady={handleEditorReady}
                  onExternalContentReady={registerExternalEditorApply}
                  onSourceSelectionChange={publishEditorSelection}
                  draftPeerCursors={draftPeerCursors}
                />
              ) : isSourceMode ? (
                <div className="border-border bg-surface rounded-lg border">
                  <label htmlFor="document-source" className="sr-only">
                    Markdown source
                  </label>
                  <DocumentSourceTextarea
                    inputRef={sourceTextareaRef}
                    content={content}
                    locked={pageEditBlocked}
                    readOnly={!draftWritable}
                    autoFocus={!mobileLayout && draftWritable}
                    onChange={handleContentChange}
                  />
                </div>
              ) : (
                <MarkdownPreview
                  content={content}
                  assetContext={assetContext}
                  inlineCommentsVisible={!activeDraft && !mobileLayout && inlineCommentsVisible && !localMode && !pageOnlyViewer}
                  spaceId={localMode || pageOnlyViewer || activeDraft ? undefined : spaceId}
                  docId={localMode || pageOnlyViewer || activeDraft ? undefined : docId}
                  onInlineCommentOpen={(commentId) => {
                    setInlineCommentsVisible(true);
                    rightPanel.openComment(commentId);
                  }}
                  onCommentCreated={() => {
                    setInlineCommentsVisible(true);
                    rightPanel.openComment();
                  }}
                  onDocumentNavigate={
                    spaceId
                      ? (targetDocumentId) =>
                          navigate(buildDocumentRoutePath(spaceId, targetDocumentId))
                      : undefined
                  }
                  selectionCommentsEnabled={!activeDraft && !mobileLayout && !pageOnlyViewer}
                />
              )}
            </Suspense>
          </div>
        </div>
      </div>
        {!mobileLayout && !isSourceMode && (
          <div className="workspace-document-contents-position" data-document-ui="contents" data-right-panel-trigger="true">
            <DocumentContents containerRef={documentBodyRef} contentKey={`${docId}:${activeDraft?.id ?? "page"}:${viewMode}:${loading}:${documentBodyReady}`}
              open={contentsOpen && (!rightPanel.open || largeDocumentLayout)}
              onClose={() => {setContentsOpen(false); contentsTriggerRef.current?.focus({preventScroll:true});}}
              onHeadingsChange={setHeadingCount} />
          </div>
        )}
      </div>
      {shareOpen && spaceId && docId && (
        <PageShareDialog
          key={`${spaceId}/${docId}`}
          spaceId={spaceId}
          pageId={docId}
          accountId={user?.id ?? null}
          localMode={localMode}
          onClose={() => setShareOpen(false)}
        />
      )}
      {draftPromptOpen && (
        <DialogShell
          labelledBy="document-draft-choice-title"
          maxWidth="md"
          onClose={() => setDraftPromptOpen(false)}
        >
          <div className="border-border border-b px-5 py-4">
            <h2 id="document-draft-choice-title" className="text-text text-base font-semibold">
              Drafts
            </h2>
            <p className="text-text-muted mt-1 text-sm">
              Choose another accessible draft or explicitly start a separate draft. Editing normally
              resumes your own work.
            </p>
          </div>
          <div className="max-h-72 space-y-2 overflow-y-auto p-4">
            {draftChoices.map((draft) => (
              <button
                key={draft.id}
                type="button"
                disabled={openingDraft || publishing}
                onClick={() => void runDraftAction(() => activateDraft(draft.id, true))}
                className="focus-ring rounded-card border-border bg-surface-raised hover:bg-hover flex w-full items-center justify-between gap-4 border px-4 py-3 text-left"
              >
                <span className="min-w-0">
                  <span className="text-text block truncate text-sm font-medium">
                    {draft.title}
                  </span>
                  <span className="text-text-muted mt-1 block text-xs">
                    {draft.owner_name} · updated {formatDocumentDate(draft.updated_at)}
                  </span>
                </span>
                <span className="text-primary shrink-0 text-xs font-medium">
                  {draft.access_role === "viewer" ? "Read" : "Resume"}
                </span>
              </button>
            ))}
          </div>
          <div className="border-border flex justify-end gap-2 border-t px-4 py-3">
            <Button variant="secondary" onClick={() => setDraftPromptOpen(false)}>
              Cancel
            </Button>
            <Button
              disabled={openingDraft || publishing || pageEditBlocked || spaceRoleBlocksDraftEditing}
              onClick={() => void runDraftAction(() => startDraft(true, true))}
            >
              Start new draft
            </Button>
          </div>
        </DialogShell>
      )}
      {deleteConfirmOpen && (
        <ConfirmDialog
          labelledBy="delete-document-page-title"
          title="Delete page"
          message={`Move page "${title.trim() || "Untitled"}" to Trash? It can be restored until it is permanently deleted. Trash is not emptied automatically.`}
          confirmLabel={deleting ? "Deleting…" : "Delete"}
          destructive
          disabled={deleting}
          helperText={deleteError || undefined}
          onCancel={() => {
            if (deleting) return;
            setDeleteConfirmOpen(false);
            setDeleteError("");
          }}
          onConfirm={() => void deleteDocument()}
        />
      )}
    </div>
  );
}
