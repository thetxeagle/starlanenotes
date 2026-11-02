import {
  buildPageCatalog,
  buildPageIndex,
  getPageAncestors,
  validatePageDestination,
  pageCatalogMoveTargets,
  PAGE_OPERATION_NODE_LIMIT,
} from "@/lib/pageHierarchy";
import {
  buildFolderCatalog,
  folderCatalogMoveTargets,
  FOLDER_OPERATION_NODE_LIMIT,
  buildFolderIndex,
  createFolderTraversalBudget,
  getFolderAncestors,
  getFolderSubtree,
  validateFolderDestination,
} from "@/lib/folderHierarchy";
import { canUseHumanAdministration } from "@/lib/guestClassification";
import { getSyncProvider } from "@/lib/storageProviders";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type DragEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from "react";
import { ReviewedMoveDialog } from "./ReviewedMoveDialog";
import { LocalReviewedMoveDialog } from "./LocalReviewedMoveDialog";
import type { LocalMovePreviewIntent } from "@/lib/reviewedMoves";
import type { LocalReviewedMoveSession } from "@/lib/dataClient";
import {
  captureReviewedMoveContext,
  completeReviewedMove,
  type ReviewedMoveContext,
} from "@/lib/reviewedServerMoves";
import type { ServerMoveIntent } from "@starlane-notes/shared-types";
import { createPortal } from "react-dom";
import { useNavigate, useLocation, type Location, type NavigateOptions } from "react-router-dom";
import {
  CalendarDays,
  Archive,
  Check,
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Copy,
  Clock3,
  Command,
  FileText,
  Folder,
  FolderPlus,
  Home,
  ListChecks,
  MoreHorizontal,
  MoveRight,
  Pencil,
  Plus,
  Search,
  Settings,
  Shield,
  LogOut,
  Star,
  Trash2,
  UserCircle,
  X,
} from "lucide-react";
import {
  dataClient,
  captureDraftSaveSelection,
  verifyDraftSaveSelection,
  type PropertySchemaRecord,
  type TemplateRecord,
} from "@/lib/dataClient";
import { addGraphDirtyListener, emitGraphDirty, type GraphDirtyDetail } from "@/lib/graphEvents";
import { emitTransientSystemNotification } from "@/lib/systemNotifications";
import { emitDocumentLocationChanged } from "@/lib/documentLocationEvents";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import { buildSpaceHomePath } from "@/lib/lastViewedSpace";
import { getPersonalSpaceOwnerLabel } from "@/lib/spaceLabels";
import {
  findDailyTemplate,
  findDailyFolder,
  DAILY_FOLDER_NAME,
  formatDateStamp,
  templatePresentationAsRecord,
  templatePropertiesAsRecord,
} from "@/lib/templates";
import {
  getTemplateExperience,
  type TemplateFieldValues,
} from "@/lib/templateExperience";
import { prepareTemplateCreation } from "@/lib/templateCreation";
import { applyTemplatePageWidth } from "@/lib/templatePresentationContent";
import { DialogShell } from "@/components/ui/dialog-shell";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { TemplatePickerDialog } from "@/components/TemplatePickerDialog";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Input } from "@/components/ui/input";
import { SearchInput } from "@/components/ui/search-input";
import {
  CONTROL_CHROME_ACTION_CLASS,
  CONTROL_CHROME_RAIL_CLASS,
} from "@/components/ui/control-chrome";
import { Select } from "@/components/ui/select";
import { useMenuKeyboardNavigation } from "@/components/ui/useMenuKeyboardNavigation";
import { SlashPathPicker, type SlashPathPickerOption } from "./SlashPathPicker";
import { useAuth } from "@/hooks/useAuth";
import { BrandLogo } from "@/components/BrandLogo";
import { ContentIdentityIcon } from "@/components/ContentIdentityIcon";
import { NotificationBell } from "@/components/NotificationBell";
import { useRuntimePlatform } from "@/hooks/useRuntimePlatform";
import {
  getWorkspaceSpaceSections,
  useWorkspaceSpaces,
  WorkspaceSpaceBoundary,
  type WorkspaceSpace,
} from "./WorkspaceSpaces";
import { AppearanceToggle } from "./AppearanceToggle";

export const SIDEBAR_DANGER_MENU_ITEM_CLASS = "text-danger hover:bg-danger/10";
function createDocumentSourceId(): string {
  return (
    globalThis.crypto?.randomUUID?.() ??
    `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
  );
}
export const OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT = "starlane:open-sidebar-move-document";
export const OPEN_SIDEBAR_CREATE_DOCUMENT_EVENT = "starlane:sidebar-create-document";
export const OPEN_SIDEBAR_CREATE_FROM_TEMPLATE_EVENT =
  "starlane:sidebar-create-document-from-template";
export const OPEN_SIDEBAR_CREATE_FOLDER_EVENT = "starlane:sidebar-create-folder";
export const OPEN_SIDEBAR_CREATE_SPACE_EVENT = "starlane:sidebar-create-space";
export const OPEN_SIDEBAR_DAILY_NOTE_EVENT = "starlane:sidebar-open-daily-note";
export const SIDEBAR_ACCOUNT_SETTINGS_PATH = "/profile";
/** @deprecated Use the consolidated account settings destination. */
export const SIDEBAR_PROFILE_SETTINGS_PATH = SIDEBAR_ACCOUNT_SETTINGS_PATH;
/** @deprecated Security now lives inside Account Settings. */
export const SIDEBAR_SECURITY_SETTINGS_PATH = "/profile/security";
export const SIDEBAR_SITE_ADMINISTRATION_PATH = "/admin";
export const SIDEBAR_TREE_TOGGLE_LANE_CLASS = "flex h-7 w-4 shrink-0 items-center justify-center";
export const SIDEBAR_TREE_ICON_SIZE = 14;
export const SIDEBAR_COMPACT_FILES_REVEAL_CLASS = "border-border/60 mt-2 border-t pt-2";
export const SIDEBAR_TREE_NESTED_GROUP_CLASS = "ml-4";

export function shouldRefreshSidebarContents(
  activeSpaceId: string | undefined,
  change: Pick<GraphDirtyDetail, "spaceId" | "reason">,
): boolean {
  return activeSpaceId === change.spaceId && change.reason !== "document-content-updated";
}

export function getSidebarMenuNavigationIndex(
  currentIndex: number,
  itemCount: number,
  key: "ArrowDown" | "ArrowUp" | "Home" | "End",
): number {
  if (itemCount <= 0) return -1;
  if (key === "Home") return 0;
  if (key === "End") return itemCount - 1;
  if (key === "ArrowUp") return currentIndex <= 0 ? itemCount - 1 : currentIndex - 1;
  return currentIndex >= itemCount - 1 ? 0 : currentIndex + 1;
}

export function getSidebarTreeNavigationIndex(
  currentIndex: number,
  itemCount: number,
  key: "ArrowDown" | "ArrowUp" | "Home" | "End",
): number {
  if (itemCount <= 0) return -1;
  if (key === "Home") return 0;
  if (key === "End") return itemCount - 1;
  if (key === "ArrowUp") return Math.max(0, currentIndex - 1);
  return Math.min(itemCount - 1, currentIndex + 1);
}

/**
 * Keeps the Files tree on a single tab stop. Focused items win, followed by
 * the current document and, when no document is open, the first visible item.
 */
export function getSidebarTreeItemTabIndex(
  focusedItemId: string | null,
  itemId: string,
  currentItemId: string | null,
  fallbackItemId: string | null,
): number {
  const tabStopId = focusedItemId ?? currentItemId ?? fallbackItemId;
  return tabStopId === itemId ? 0 : -1;
}

export function getSidebarCurrentTreeItemId(
  documents: DocItem[],
  spaceId: string | undefined,
  currentPath: string,
): string | null {
  if (!spaceId) return null;
  const currentDocument = documents.find((document) =>
    isSidebarRouteActive(currentPath, getSidebarDocumentPath(spaceId, document.id)),
  );
  return currentDocument ? `document:${currentDocument.id}` : null;
}

/** Dialog ownership stays with the caller until creation is confirmed. */
export async function settleSidebarPageCreation(create: () => Promise<boolean | undefined>, confirmed: () => void, failed: (message: string) => void): Promise<boolean> {
  try {
    if (await create()) { confirmed(); return true; }
  } catch { /* An uncertain write is not confirmation to dismiss pending input. */ }
  failed("Page creation was not confirmed. Your title and destination are kept. Check the page list before retrying.");
  return false;
}

export function shouldShowSidebarSiteAdministration(
  localMode: boolean,
  role: string | undefined,
  accountKind?: unknown,
): boolean {
  return localMode || canUseHumanAdministration(role, accountKind);
}

export function shouldQueueSidebarDailyNote(activeSpace: { id: string } | null): boolean {
  return !activeSpace;
}

export function getSidebarDeleteConfirmationTitle(type: "document" | "folder"): string {
  return type === "document" ? "Delete page" : "Delete folder";
}

export function getSidebarRenameEmojiLabel(type: "document" | "folder"): string {
  return type === "document" ? "Page emoji" : "Folder emoji";
}

const SIDEBAR_PAGE_EMOJI_PRESETS = ["📄", "📝", "📘", "📕", "📗", "📙", "💡", "⭐"];
const SIDEBAR_FOLDER_EMOJI_PRESETS = ["📁", "📂", "🗂️", "🗃️", "📚", "🧰", "🏷️", "🚀"];

export function getSidebarRenameEmojiPresets(type: "document" | "folder"): readonly string[] {
  return type === "document" ? SIDEBAR_PAGE_EMOJI_PRESETS : SIDEBAR_FOLDER_EMOJI_PRESETS;
}

export function getSidebarDeleteConfirmationMessage(
  type: "document" | "folder",
  name: string,
): string {
  const trimmed = name.trim();
  if (type === "document") {
    return trimmed
      ? `Move page "${trimmed}" to Trash? You can restore it from Space settings.`
      : "Move this page to Trash? You can restore it from Space settings.";
  }
  return trimmed
    ? `Delete folder "${trimmed}" and its pages? This cannot be undone.`
    : "Delete this folder and its pages? This cannot be undone.";
}

interface FolderItem {
  space_id?: string;
  id: string;
  name: string;
  icon?: string;
  parent_id?: string;
}

interface DocItem {
  space_id?: string;
  id: string;
  title: string;
  icon?: string;
  folder_id?: string;
  parent_id?: string;
}

interface SidebarProps {
  showHeader?: boolean;
  mobileSettingsMenu?: boolean;
  collapsed?: boolean;
  onOpenCommandMenu?: () => void;
  onOpenAppSettings?: () => void;
  onShowFiles?: () => void;
  onMobileOverlayChange?: (open: boolean) => void;
}

type SidebarCreateKind = "document" | "folder";

export interface SidebarCreateDialogState {
  kind: SidebarCreateKind;
  title: string;
  query: string;
  folderId: string | null;
  parentId: string | null;
}

export function getSidebarTemplatePickerTarget(createDialog: SidebarCreateDialogState): {
  folderId?: string;
  parentId?: string;
  title: string;
} {
  return {
    folderId: createDialog.folderId ?? undefined,
    parentId: createDialog.parentId ?? undefined,
    title: createDialog.title.trim(),
  };
}

export function getSidebarFilesCreateActionClass(mobile: boolean): string {
  return `rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex items-center justify-center transition-colors duration-100 disabled:opacity-40 ${
    mobile ? "h-11 w-11" : "h-7 w-7"
  }`;
}

export function getSidebarNavigationTargetClass(mobile: boolean): string {
  return mobile ? "min-h-11 py-2" : "py-1.5";
}

export function getSidebarNavigationItemStateClass(active: boolean): string {
  return active
    ? "border-primary bg-surface-raised text-primary [&>svg]:text-primary"
    : "border-transparent text-text-muted hover:border-primary/40 hover:bg-surface-raised hover:text-text [&>svg]:transition-colors [&>svg]:duration-100 hover:[&>svg]:text-primary";
}

export function getSidebarNavigationAriaCurrent(active: boolean): "page" | undefined {
  return active ? "page" : undefined;
}

export function getSidebarUtilityGroupClass(collapsed: boolean): string {
  return collapsed
    ? `${CONTROL_CHROME_RAIL_CLASS} w-9 flex-col overflow-hidden`
    : `${CONTROL_CHROME_RAIL_CLASS} w-full overflow-hidden`;
}

export function getSidebarUtilityActionClass(collapsed: boolean, mobile = false): string {
  return `${CONTROL_CHROME_ACTION_CLASS} flex items-center justify-center ${
    mobile ? "h-11 min-w-0 flex-1" : collapsed ? "h-9 w-full" : "h-8 min-w-0 flex-1"
  }`;
}

export function getSidebarAccountMenuItemClass(mobile: boolean, destructive = false): string {
  return `starlane-menu-item ${mobile ? "min-h-11" : ""} ${
    destructive ? SIDEBAR_DANGER_MENU_ITEM_CLASS : ""
  }`;
}

export function shouldShowSidebarAppSettings(
  localMode: boolean,
  mobileSettingsMenu: boolean,
): boolean {
  return localMode || mobileSettingsMenu;
}

export function getSidebarTreeRowClass(mobile: boolean): string {
  return mobile ? "min-h-11 py-2 pr-12" : "py-1 pr-8";
}

export function getSidebarTreeToggleClass(mobile: boolean): string {
  return mobile
    ? "flex h-11 w-9 shrink-0 items-center justify-center"
    : SIDEBAR_TREE_TOGGLE_LANE_CLASS;
}

export function getSidebarTreeActionClass(mobile: boolean): string {
  return mobile ? "h-11 w-11" : "h-7 w-7";
}

export function getSidebarSpaceSelectorChevronClass(open: boolean): string {
  return `text-text-muted shrink-0 transition-transform duration-100 ${open ? "rotate-180" : ""}`;
}

interface SidebarLocationOption extends SlashPathPickerOption {
  folderId: string | null;
  parentId: string | null;
}

export function sortSidebarFolders<T extends FolderItem>(folders: T[]): T[] {
  return [...folders].sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
  );
}

export function sortSidebarDocuments<T extends DocItem>(documents: T[]): T[] {
  return [...documents].sort((a, b) =>
    a.title.localeCompare(b.title, undefined, { sensitivity: "base" }),
  );
}

export function getSidebarTreeExpansionStorageKey(spaceId: string): string {
  return `starlane_sidebar_expanded_folders:${spaceId}`;
}

/** Returns the folder branch that must be open for the routed page to be visible. */
export function getSidebarActiveFolderIds(
  folders: FolderItem[],
  documents: DocItem[],
  currentPath: string,
): string[] {
  const activeDocument = documents.find(
    (document) => currentPath.split("/").pop() === encodeURIComponent(document.id),
  );
  if (!activeDocument) return [];
  try {
    const catalog = createSidebarPageCatalog(
      documents,
      activeDocument.space_id ?? "sidebar-page-scope",
    );
    getPageAncestors(
      catalog.index,
      activeDocument.space_id ?? "sidebar-page-scope",
      activeDocument.id,
    );
  } catch {
    return [];
  }

  const folderId = activeDocument?.folder_id;
  if (!folderId) return [];
  try {
    const { spaceId, index } = sidebarFolderContext(folders, folderId);
    return getFolderAncestors(index, spaceId, folderId).map((row) => row.id);
  } catch {
    return [];
  }
}

export function createSidebarFolderCatalog(
  folders: FolderItem[],
  spaceId = "sidebar-folder-scope",
) {
  const catalog = buildFolderCatalog(
    folders.map((row) => ({ ...row, space_id: row.space_id ?? spaceId })),
  );
  for (const [id, rows] of catalog.children) catalog.children.set(id, sortSidebarFolders(rows));
  return catalog;
}
type SidebarFolderCatalog = ReturnType<typeof createSidebarFolderCatalog>;

function sidebarFolderContext(folders: FolderItem[], folderId: string) {
  const spaceId = folders.find((row) => row.id === folderId)?.space_id ?? "sidebar-folder-scope";
  const index = buildFolderIndex(
    folders.map((row) => ({ ...row, space_id: row.space_id ?? spaceId })),
  );
  return { spaceId, index };
}
export function getSidebarFolderDepth(folders: FolderItem[], folderId: string): number {
  try {
    const { spaceId, index } = sidebarFolderContext(folders, folderId);
    return getFolderAncestors(index, spaceId, folderId).length;
  } catch {
    return 0;
  }
}
export function canCreateSidebarChildFolder(folders: FolderItem[], folderId: string): boolean {
  try {
    const { spaceId, index } = sidebarFolderContext(folders, folderId),
      budget = createFolderTraversalBudget();
    validateFolderDestination(index, spaceId, folderId, undefined, budget);
    let proposed = "new-child";
    while (index.byId.has(proposed)) proposed += ":";
    budget.visit(proposed);
    return true;
  } catch {
    return false;
  }
}

export function canMoveSidebarFolderToRoot(
  folder: Pick<FolderItem, "parent_id">,
  localMode: boolean,
): boolean {
  return localMode && Boolean(folder.parent_id);
}

export function getSidebarFolderSubtreeDepth(folders: FolderItem[], folderId: string): number {
  try {
    const { spaceId, index } = sidebarFolderContext(folders, folderId);
    const rows = getFolderSubtree(index, spaceId, folderId),
      depths = new Map<string, number>();
    let maximum = 0;
    for (const row of rows) {
      const depth = row.id === folderId ? 1 : (depths.get(row.parent_id ?? "") ?? 0) + 1;
      depths.set(row.id, depth);
      maximum = Math.max(maximum, depth);
    }
    return maximum;
  } catch {
    return 0;
  }
}
export function isSidebarFolderDescendant(
  folders: FolderItem[],
  ancestorId: string,
  candidateId: string,
): boolean {
  try {
    const { spaceId, index } = sidebarFolderContext(folders, ancestorId);
    return (
      candidateId !== ancestorId &&
      getFolderAncestors(index, spaceId, candidateId).some((row) => row.id === ancestorId)
    );
  } catch {
    return false;
  }
}
export function canMoveSidebarFolderToParent(
  folders: FolderItem[],
  folderId: string,
  parentId: string,
): boolean {
  const folder = folders.find((row) => row.id === folderId);
  if (!folder || folder.parent_id === parentId) return false;
  try {
    const { spaceId, index } = sidebarFolderContext(folders, folderId);
    validateFolderDestination(index, spaceId, parentId, folderId);
    return true;
  } catch {
    return false;
  }
}

export function canMoveSidebarFolderAcrossSpaces(
  sourceFolders: FolderItem[],
  folderId: string,
  targetFolders: FolderItem[],
  parentId: string,
): boolean {
  try {
    const source = sidebarFolderContext(sourceFolders, folderId),
      target = sidebarFolderContext(targetFolders, parentId),
      budget = createFolderTraversalBudget();
    getFolderSubtree(source.index, source.spaceId, folderId, budget);
    getFolderAncestors(target.index, target.spaceId, parentId, budget);
    return true;
  } catch {
    return false;
  }
}

export function canDropSidebarFolderOnParent(
  folders: FolderItem[],
  folderId: string | null | undefined,
  parentId: string,
  localMode: boolean,
): boolean {
  return Boolean(
    localMode && folderId && canMoveSidebarFolderToParent(folders, folderId, parentId),
  );
}

export function canDropSidebarFolderOnRoot(
  folders: FolderItem[],
  folderId: string | null | undefined,
  localMode: boolean,
): boolean {
  const folder = folderId ? folders.find((item) => item.id === folderId) : null;
  return Boolean(folder && canMoveSidebarFolderToRoot(folder, localMode));
}

export function getSidebarFolderMoveTargets(folders: FolderItem[], folderId: string): FolderItem[] {
  try {
    const space = folders.find((row) => row.id === folderId)?.space_id ?? "sidebar-folder-scope";
    const catalog = createSidebarFolderCatalog(folders, space);
    const targets = folderCatalogMoveTargets(catalog, space, folderId, catalog, space);
    return sortSidebarFolders(folders.filter((row) => targets.has(row.id)));
  } catch {
    return [];
  }
}

export function getSidebarFolderPathLabel(folders: FolderItem[], folderId: string): string {
  try {
    const { spaceId, index } = sidebarFolderContext(folders, folderId);
    return getFolderAncestors(index, spaceId, folderId)
      .reverse()
      .map((row) => row.name)
      .join(" / ");
  } catch {
    return "";
  }
}

export function createSidebarPageCatalog(documents: DocItem[], spaceId = "sidebar-page-scope") {
  const catalog = buildPageCatalog(
    documents
      .map((row) => ({ ...row, space_id: row.space_id ?? spaceId }))
      .filter((row) => row.space_id === spaceId),
  );
  for (const [id, rows] of catalog.children) catalog.children.set(id, sortSidebarDocuments(rows));
  return catalog;
}
type SidebarPageCatalog = ReturnType<typeof createSidebarPageCatalog>;
function sidebarPageContext(documents: DocItem[], documentId: string) {
  const spaceId = documents.find((row) => row.id === documentId)?.space_id ?? "sidebar-page-scope";
  return {
    spaceId,
    index: buildPageIndex(documents.map((row) => ({ ...row, space_id: row.space_id ?? spaceId }))),
  };
}
export function getSidebarDocumentDepth(documents: DocItem[], documentId: string): number {
  try {
    const { spaceId, index } = sidebarPageContext(documents, documentId);
    return getPageAncestors(index, spaceId, documentId).length;
  } catch {
    return 0;
  }
}
export function isSidebarDocumentDescendant(
  documents: DocItem[],
  ancestorId: string,
  candidateId: string,
): boolean {
  try {
    const { spaceId, index } = sidebarPageContext(documents, candidateId);
    return (
      candidateId !== ancestorId &&
      getPageAncestors(index, spaceId, candidateId).some((row) => row.id === ancestorId)
    );
  } catch {
    return false;
  }
}
export function canMoveSidebarDocumentToParent(
  documents: DocItem[],
  documentId: string,
  parentId: string,
  folderId?: string,
): boolean {
  try {
    const { spaceId, index } = sidebarPageContext(documents, documentId);
    if (index.byId.get(documentId)?.parent_id === parentId) return false;
    validatePageDestination(index, spaceId, folderId, parentId, documentId);
    return true;
  } catch {
    return false;
  }
}
export function getSidebarDocumentPathLabel(
  folders: FolderItem[],
  documents: DocItem[],
  documentId: string,
  _seen = new Set<string>(),
  catalog?: SidebarFolderCatalog,
): string {
  try {
    const { spaceId, index } = sidebarPageContext(documents, documentId);
    const ancestors = getPageAncestors(index, spaceId, documentId);
    const folderId = ancestors[0].folder_id;
    const folderLabel = folderId
      ? catalog
        ? (catalog.labels.get(folderId) ?? "")
        : getSidebarFolderPathLabel(folders, folderId)
      : "";
    return [
      ...(folderLabel ? [folderLabel] : []),
      ...ancestors.reverse().map((row) => row.title),
    ].join(" / ");
  } catch {
    return "";
  }
}
function sidebarPageLocationLabel(
  folderCatalog: SidebarFolderCatalog,
  pageCatalog: SidebarPageCatalog,
  doc: DocItem,
): string {
  const folder = doc.folder_id ? folderCatalog.labels.get(doc.folder_id) : undefined;
  return [folder, pageCatalog.labels.get(doc.id)].filter(Boolean).join(" / ");
}

export function getSidebarCreateLocationOptions(
  spaceName: string,
  folders: FolderItem[],
  documents: DocItem[],
  kind: SidebarCreateKind,
  catalog = createSidebarFolderCatalog(folders),
  pageCatalog = createSidebarPageCatalog(documents),
): SidebarLocationOption[] {
  const options: SidebarLocationOption[] = [
    {
      id: "root",
      label: `${spaceName} / Root`,
      description: "Top level",
      icon: "folder",
      folderId: null,
      parentId: null,
    },
  ];

  for (const folder of sortSidebarFolders(folders)) {
    if (!catalog.depths.get(folder.id) || (kind === "folder" && catalog.depths.get(folder.id)! >= FOLDER_OPERATION_NODE_LIMIT)) continue;
    options.push({
      id: `folder:${folder.id}`,
      label: `${spaceName} / ${catalog.labels.get(folder.id) ?? folder.name}`,
      description: "Folder",
      icon: "folder",
      folderId: folder.id,
      parentId: null,
    });
  }

  if (kind === "document") {
    for (const doc of sortSidebarDocuments(pageCatalog.order)) {
      if ((pageCatalog.depths.get(doc.id) ?? 0) >= PAGE_OPERATION_NODE_LIMIT) continue;
      options.push({
        id: `document:${doc.id}`,
        label: `${spaceName} / ${sidebarPageLocationLabel(catalog, pageCatalog, doc)}`,
        description: "Child page",
        icon: "document",
        folderId: doc.folder_id ?? null,
        parentId: doc.id,
      });
    }
  }

  return options;
}

export function getSidebarHomePath(spaceId?: string): string {
  return spaceId ? buildSpaceHomePath(spaceId) : "/";
}

export function getSidebarDocumentPath(spaceId: string, documentId: string): string {
  return buildDocumentRoutePath(spaceId, documentId);
}

export function isCurrentSidebarSpaceRequest(
  requestSpaceId: string,
  requestId: number,
  activeSpaceId: string | undefined,
  activeRequestId: number,
): boolean {
  return requestSpaceId === activeSpaceId && requestId === activeRequestId;
}

export function getSidebarRecentPath(spaceId?: string): string {
  return spaceId ? `/spaces/${encodeURIComponent(spaceId)}/recent` : "/";
}

export function getSidebarJournalPath(spaceId?: string): string {
  return spaceId ? `/spaces/${encodeURIComponent(spaceId)}/journal` : "/";
}

export function getSidebarGraphPath(spaceId?: string): string {
  return spaceId ? `/spaces/${encodeURIComponent(spaceId)}/graph` : "/";
}

export function getSidebarWorkflowPath(spaceId?: string): string {
  return spaceId ? `/spaces/${encodeURIComponent(spaceId)}/workflows` : "/";
}

export function getSidebarStarredPath(spaceId?: string): string {
  return spaceId ? `/spaces/${encodeURIComponent(spaceId)}/starred` : "/";
}

export function getSidebarSpaceSettingsPath(spaceId?: string): string {
  return spaceId ? `/spaces/${encodeURIComponent(spaceId)}/settings` : "/";
}

export function getSidebarSettingsNavigationOptions(
  mobileSettingsMenu: boolean,
  location: Location,
): NavigateOptions | undefined {
  return mobileSettingsMenu
    ? undefined
    : {
        state: {
          backgroundLocation: location,
        },
      };
}

export function isSidebarRouteActive(pathname: string, targetPath: string): boolean {
  return pathname.replace(/\/+$/, "") === targetPath.replace(/\/+$/, "");
}

export function openSidebarGlobalSearch(spaceId?: string): void {
  document.dispatchEvent(
    new CustomEvent("starlane:open-search", {
      detail: {
        spaceId,
      },
    }),
  );
}

export function openSidebarDocumentMoveDialog(documentId: string): void {
  document.dispatchEvent(
    new CustomEvent<{ documentId: string }>(OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT, {
      detail: { documentId },
    }),
  );
}

/** Completion uses the original mounted selection after every awaited refresh. */
export async function completeLocalSidebarMove(
  session: LocalReviewedMoveSession,
  current: () => boolean,
  newerLocalChangesRetained: boolean,
  refresh: (verify: () => void) => Promise<void>,
  finish: (retainCanvas: boolean) => void,
): Promise<void> {
  const verify = () => {
    if (!current()) throw new Error("Move selection changed; inspect the original workspace.");
    session.verifyCurrent();
  };
  verify();
  await refresh(verify);
  verify();
  finish(newerLocalChangesRetained);
}

export function Sidebar({ ...props }: SidebarProps = {}) {
  return (
    <WorkspaceSpaceBoundary>
      <SidebarContent {...props} />
    </WorkspaceSpaceBoundary>
  );
}

function SidebarContent({
  showHeader = true,
  mobileSettingsMenu = false,
  collapsed = false,
  onOpenCommandMenu,
  onOpenAppSettings,
  onShowFiles,
  onMobileOverlayChange,
}: SidebarProps = {}) {
  const navigate = useNavigate();
  const location = useLocation();
  const { user, logout } = useAuth();
  const { runtime } = useRuntimePlatform();
  const localMode = getCurrentLaunchMode() === "local";
  const {
    spaces,
    activeSpace,
    recentSpaceIds,
    canCreateSpace,
    selectSpace,
    createSpace: createWorkspaceSpace,
  } = useWorkspaceSpaces();
  const activeSpaceId = activeSpace?.id;
  const activeSpaceIdRef = useRef(activeSpaceId);
  activeSpaceIdRef.current = activeSpaceId;
  const moveRouteRef = useRef(location);
  moveRouteRef.current = location;
  const moveMountedRef = useRef(true);
  useEffect(() => {
    moveMountedRef.current = true;
    return () => {
      moveMountedRef.current = false;
    };
  }, []);
  const localReviewIdentity = useRef<object | null>(null);
  const [localReviewedMove, setLocalReviewedMove] = useState<{
    identity: object;
    source: string;
    intent: LocalMovePreviewIntent;
    session: LocalReviewedMoveSession;
    title: string;
    destinationName: string;
  } | null>(null);
  const spaceContentsRequestIdRef = useRef(0);
  const spaceContentsRefreshRequestIdRef = useRef(0);
  const moveDocumentLookupRequestIdRef = useRef(0);
  const [moveReviewError, setMoveReviewError] = useState("");
  const [reviewedMove, setReviewedMove] = useState<{
    context: ReviewedMoveContext;
    source: string;
    intent: ServerMoveIntent;
    title: string;
    destinationName: string;
  } | null>(null);
  const reviewedMoveRef = useRef(reviewedMove);
  reviewedMoveRef.current = reviewedMove;
  const [folders, setFolders] = useState<FolderItem[]>([]);
  const [documents, setDocuments] = useState<DocItem[]>([]);
  const folderCatalog = useMemo(
    () => createSidebarFolderCatalog(folders, activeSpaceId),
    [folders, activeSpaceId],
  );
  const pageCatalog = useMemo(
    () => createSidebarPageCatalog(documents, activeSpaceId),
    [documents, activeSpaceId],
  );
  const activePageAncestors = useMemo(() => {
    const active = documents.find((row) =>
      isSidebarRouteActive(location.pathname, getSidebarDocumentPath(activeSpaceId ?? "", row.id)),
    );
    if (!active) return new Set<string>();
    try {
      return new Set(
        getPageAncestors(pageCatalog.index, activeSpaceId ?? "sidebar-page-scope", active.id).map(
          (row) => row.id,
        ),
      );
    } catch {
      return new Set<string>();
    }
  }, [documents, location.pathname, activeSpaceId, pageCatalog]);
  const folderDocuments = useMemo(() => {
    const grouped = new Map<string, DocItem[]>();
    for (const doc of pageCatalog.roots)
      if (doc.folder_id) {
        const rows = grouped.get(doc.folder_id) ?? [];
        rows.push(doc);
        grouped.set(doc.folder_id, rows);
      }
    for (const [id, rows] of grouped) grouped.set(id, sortSidebarDocuments(rows));
    return grouped;
  }, [pageCatalog]);
  const [expandedFolderState, setExpandedFolderState] = useState<{
    spaceId: string | null;
    folderIds: string[];
  }>({ spaceId: null, folderIds: [] });
  const expandedFolderIds =
    activeSpace && expandedFolderState.spaceId === activeSpace.id
      ? expandedFolderState.folderIds
      : [];
  const expandedFolderSet = useMemo(
    () =>
      new Set(
        activeSpaceId && expandedFolderState.spaceId === activeSpaceId
          ? expandedFolderState.folderIds
          : [],
      ),
    [activeSpaceId, expandedFolderState],
  );
  const [focusedTreeItemId, setFocusedTreeItemId] = useState<string | null>(null);
  const [templates, setTemplates] = useState<TemplateRecord[]>([]);
  const [templateSchemas, setTemplateSchemas] = useState<PropertySchemaRecord[]>([]);
  const dailyCreateSourceIdsRef = useRef(new Map<string, string>());
  const [spaceMenuOpen, setSpaceMenuOpen] = useState(false);
  const [accountMenuOpen, setAccountMenuOpen] = useState(false);
  const [notificationCenterOpen, setNotificationCenterOpen] = useState(false);
  const [spaceSearchQuery, setSpaceSearchQuery] = useState("");
  const spaceMenuContentRef = useRef<HTMLDivElement>(null);
  const accountMenuContentRef = useRef<HTMLDivElement>(null);
  const spaceMenuTriggerRef = useRef<HTMLButtonElement>(null);
  const accountMenuTriggerRef = useRef<HTMLButtonElement>(null);
  const spaceSearchInputRef = useRef<HTMLInputElement>(null);
  const [templatePickerOpen, setTemplatePickerOpen] = useState(false);
  const [templatePickerAllowsBlank, setTemplatePickerAllowsBlank] = useState(true);
  const [templateFolderId, setTemplateFolderId] = useState<string | undefined>(undefined);
  const [templateParentId, setTemplateParentId] = useState<string | undefined>(undefined);
  const [templateDocumentTitle, setTemplateDocumentTitle] = useState<string | undefined>(undefined);
  const [templatePickerDestination, setTemplatePickerDestination] = useState("Space root");
  const [templatePickerLoading, setTemplatePickerLoading] = useState(false);
  const [templatePickerError, setTemplatePickerError] = useState("");
  const [templatePickerDate, setTemplatePickerDate] = useState<Date | undefined>(undefined);
  const [templatePickerInitialTemplateId, setTemplatePickerInitialTemplateId] = useState<
    string | undefined
  >(undefined);
  const [templatePickerCreatesDailyFolder, setTemplatePickerCreatesDailyFolder] = useState(false);
  const [pendingDailyNote, setPendingDailyNote] = useState(false);

  // Create space
  const [createSpaceOpen, setCreateSpaceOpen] = useState(false);
  const [newSpaceName, setNewSpaceName] = useState("");
  const spaceInputRef = useRef<HTMLInputElement>(null);

  // Create document or folder
  const [createDialog, setCreateDialog] = useState<SidebarCreateDialogState | null>(null);
  const createDialogRef = useRef(createDialog);
  createDialogRef.current = createDialog;
  const [createDialogError, setCreateDialogError] = useState("");
  const [createDialogPending, setCreateDialogPending] = useState(false);
  const createTitleInputRef = useRef<HTMLInputElement>(null);
  const [renameTarget, setRenameTarget] = useState<
    | { type: "folder"; id: string; currentName: string; currentIcon?: string }
    | { type: "document"; id: string; currentName: string; currentIcon?: string }
    | null
  >(null);
  const [deleteTarget, setDeleteTarget] = useState<
    | { type: "folder"; id: string; name: string }
    | { type: "document"; id: string; name: string }
    | null
  >(null);
  const [renameName, setRenameName] = useState("");
  const [renameError, setRenameError] = useState("");
  const [renameIcon, setRenameIcon] = useState("");
  const renameStateRef = useRef({ target: renameTarget, name: renameName, icon: renameIcon });
  renameStateRef.current = { target: renameTarget, name: renameName, icon: renameIcon };
  const renameInputRef = useRef<HTMLInputElement>(null);
  const [movingFolderId, setMovingFolderId] = useState<string | null>(null);
  const [movingDocumentId, setMovingDocumentId] = useState<string | null>(null);
  const [moveTargetSpaceId, setMoveTargetSpaceId] = useState<string>("");
  const [moveLocationQuery, setMoveLocationQuery] = useState("");
  const moveSelectorsRef = useRef("");
  moveSelectorsRef.current = JSON.stringify([movingFolderId, movingDocumentId, moveTargetSpaceId]);
  const [moveTargetFolders, setMoveTargetFolders] = useState<FolderItem[]>([]);
  const moveFolderCatalog = useMemo(
    () => createSidebarFolderCatalog(moveTargetFolders, moveTargetSpaceId || activeSpaceId),
    [moveTargetFolders, moveTargetSpaceId, activeSpaceId],
  );
  const [moveTargetDocuments, setMoveTargetDocuments] = useState<DocItem[]>([]);
  const movePageCatalog = useMemo(
    () => createSidebarPageCatalog(moveTargetDocuments, moveTargetSpaceId || activeSpaceId),
    [moveTargetDocuments, moveTargetSpaceId, activeSpaceId],
  );
  const [draggingFolderId, setDraggingFolderId] = useState<string | null>(null);
  const folderDropTargets = useMemo(() => {
    if (!draggingFolderId || !localMode || !activeSpaceId) return new Set<string>();
    try {
      return folderCatalogMoveTargets(
        folderCatalog,
        activeSpaceId,
        draggingFolderId,
        folderCatalog,
        activeSpaceId,
      );
    } catch {
      return new Set<string>();
    }
  }, [draggingFolderId, localMode, activeSpaceId, folderCatalog]);
  const mobileOverlayOpen =
    mobileSettingsMenu &&
    Boolean(
      templatePickerOpen ||
      createDialog ||
      renameTarget ||
      deleteTarget ||
      movingFolderId ||
      movingDocumentId ||
      localReviewedMove ||
      reviewedMove ||
      createSpaceOpen ||
      notificationCenterOpen,
    );
  useEffect(() => {
    onMobileOverlayChange?.(mobileOverlayOpen);
    return () => onMobileOverlayChange?.(false);
  }, [mobileOverlayOpen, onMobileOverlayChange]);
  const renderMobileOverlay = (overlay: ReactNode) =>
    mobileSettingsMenu && typeof document !== "undefined"
      ? createPortal(overlay, document.body)
      : overlay;

  const persistExpandedFolderIds = useCallback(
    (next: string[]) => {
      if (!activeSpace) return;
      try {
        window.localStorage.setItem(
          getSidebarTreeExpansionStorageKey(activeSpace.id),
          JSON.stringify(next),
        );
      } catch {
        // Tree expansion is a convenience, not critical workspace state.
      }
    },
    [activeSpace],
  );

  const expandFolder = useCallback(
    (folderId: string) => {
      if (!activeSpace) return;
      setExpandedFolderState((current) => {
        const folderIds = current.spaceId === activeSpace.id ? current.folderIds : [];
        if (folderIds.includes(folderId)) return current;
        const next = [...folderIds, folderId];
        persistExpandedFolderIds(next);
        return { spaceId: activeSpace.id, folderIds: next };
      });
    },
    [activeSpace, persistExpandedFolderIds],
  );

  const toggleExpandedFolder = useCallback(
    (folderId: string) => {
      if (!activeSpace) return;
      setExpandedFolderState((current) => {
        const folderIds = current.spaceId === activeSpace.id ? current.folderIds : [];
        const next = folderIds.includes(folderId)
          ? folderIds.filter((id) => id !== folderId)
          : [...folderIds, folderId];
        persistExpandedFolderIds(next);
        return { spaceId: activeSpace.id, folderIds: next };
      });
    },
    [activeSpace, persistExpandedFolderIds],
  );

  useEffect(() => {
    spaceContentsRefreshRequestIdRef.current += 1;
    if (!activeSpaceId) {
      setExpandedFolderState({ spaceId: null, folderIds: [] });
      return;
    }
    try {
      const stored = JSON.parse(
        window.localStorage.getItem(getSidebarTreeExpansionStorageKey(activeSpaceId)) ?? "[]",
      );
      setExpandedFolderState({
        spaceId: activeSpaceId,
        folderIds: Array.isArray(stored)
          ? stored.filter((id): id is string => typeof id === "string")
          : [],
      });
    } catch {
      setExpandedFolderState({ spaceId: activeSpaceId, folderIds: [] });
    }
  }, [activeSpaceId]);

  useEffect(() => {
    const activeFolderIds = getSidebarActiveFolderIds(folders, documents, location.pathname);
    if (!activeSpace || activeFolderIds.length === 0) return;
    setExpandedFolderState((current) => {
      const folderIds = current.spaceId === activeSpace.id ? current.folderIds : [];
      const next = Array.from(new Set([...folderIds, ...activeFolderIds]));
      if (next.length === folderIds.length && current.spaceId === activeSpace.id) return current;
      persistExpandedFolderIds(next);
      return { spaceId: activeSpace.id, folderIds: next };
    });
  }, [activeSpace, documents, folders, location.pathname, persistExpandedFolderIds]);

  const closeSpaceMenuAndRestoreFocus = useCallback(() => {
    setSpaceMenuOpen(false);
    window.requestAnimationFrame(() => spaceMenuTriggerRef.current?.focus());
  }, []);

  useEffect(() => {
    if (!accountMenuOpen) return;
    const frame = window.requestAnimationFrame(() => {
      accountMenuContentRef.current
        ?.querySelector<HTMLButtonElement>("button[role='menuitem']:not(:disabled)")
        ?.focus();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [accountMenuOpen]);

  useEffect(() => {
    if (!spaceMenuOpen) return;
    const frame = window.requestAnimationFrame(() => spaceSearchInputRef.current?.focus());
    return () => window.cancelAnimationFrame(frame);
  }, [spaceMenuOpen]);

  const handleAccountMenuKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (
      event.key !== "ArrowDown" &&
      event.key !== "ArrowUp" &&
      event.key !== "Home" &&
      event.key !== "End"
    ) {
      return;
    }
    const items = Array.from(
      accountMenuContentRef.current?.querySelectorAll<HTMLButtonElement>(
        "button[role='menuitem']:not(:disabled)",
      ) ?? [],
    );
    const nextIndex = getSidebarMenuNavigationIndex(
      items.indexOf(document.activeElement as HTMLButtonElement),
      items.length,
      event.key,
    );
    if (nextIndex < 0) return;
    event.preventDefault();
    items[nextIndex]?.focus();
  };

  const handleSpaceMenuKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    const menuItems = Array.from(
      spaceMenuContentRef.current?.querySelectorAll<HTMLButtonElement>(
        "button[role='menuitemradio']:not(:disabled)",
      ) ?? [],
    );
    const target = event.target as HTMLElement;
    if (target instanceof HTMLInputElement) {
      if (event.key !== "ArrowDown") return;
      const selectedIndex = menuItems.findIndex(
        (item) => item.getAttribute("aria-checked") === "true",
      );
      const firstItem = menuItems[selectedIndex >= 0 ? selectedIndex : 0];
      if (!firstItem) return;
      event.preventDefault();
      firstItem.focus();
      return;
    }
    if (
      event.key !== "ArrowDown" &&
      event.key !== "ArrowUp" &&
      event.key !== "Home" &&
      event.key !== "End"
    ) {
      return;
    }
    const nextIndex = getSidebarMenuNavigationIndex(
      menuItems.indexOf(document.activeElement as HTMLButtonElement),
      menuItems.length,
      event.key,
    );
    if (nextIndex < 0) return;
    event.preventDefault();
    menuItems[nextIndex]?.focus();
  };

  const handleFilesTreeKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (
      event.key !== "ArrowDown" &&
      event.key !== "ArrowUp" &&
      event.key !== "Home" &&
      event.key !== "End"
    ) {
      return;
    }
    const activeTreeItem =
      event.target instanceof HTMLElement
        ? event.target.closest<HTMLElement>("[role='treeitem']")
        : null;
    if (!activeTreeItem) return;
    const treeItems = Array.from(
      event.currentTarget.querySelectorAll<HTMLElement>("[role='treeitem']"),
    );
    const currentIndex = treeItems.indexOf(activeTreeItem);
    if (currentIndex < 0) return;
    event.preventDefault();
    treeItems[
      getSidebarTreeNavigationIndex(
        currentIndex,
        treeItems.length,
        event.key as "ArrowDown" | "ArrowUp" | "Home" | "End",
      )
    ]?.focus();
  };

  useEffect(() => {
    setFocusedTreeItemId(null);
  }, [activeSpace?.id]);

  // Keep Files-tree data scoped to the active space and ignore superseded responses.
  useEffect(() => {
    if (!activeSpaceId) {
      setFolders([]);
      setDocuments([]);
      setTemplates([]);
      setTemplateSchemas([]);
      setTemplatePickerLoading(false);
      setTemplatePickerError("");
      return;
    }

    const requestSpaceId = activeSpaceId;
    const requestId = ++spaceContentsRequestIdRef.current;
    const refreshRequestIdAtStart = spaceContentsRefreshRequestIdRef.current;
    let cancelled = false;
    const isCurrentSpaceRequest = () =>
      !cancelled &&
      isCurrentSidebarSpaceRequest(
        requestSpaceId,
        requestId,
        activeSpaceIdRef.current,
        spaceContentsRequestIdRef.current,
      );
    const isCurrentTreeRequest = () =>
      isCurrentSpaceRequest() &&
      refreshRequestIdAtStart === spaceContentsRefreshRequestIdRef.current;

    setFolders([]);
    setDocuments([]);
    setTemplates([]);
    setTemplateSchemas([]);
    setTemplatePickerLoading(true);
    setTemplatePickerError("");
    void Promise.allSettled([
      dataClient.listFolders(requestSpaceId),
      dataClient.listDocuments(requestSpaceId, { include_nested: true }),
      dataClient.listTemplates(requestSpaceId),
      dataClient.listPropertySchemas(requestSpaceId),
    ])
      .then(([folderResult, documentResult, templateResult, schemaResult]) => {
        if (!isCurrentSpaceRequest()) return;
        if (isCurrentTreeRequest()) {
          setFolders(folderResult.status === "fulfilled" ? folderResult.value : []);
          setDocuments(documentResult.status === "fulfilled" ? documentResult.value : []);
        }
        setTemplates(templateResult.status === "fulfilled" ? templateResult.value : []);
        setTemplateSchemas(schemaResult.status === "fulfilled" ? schemaResult.value : []);
        setTemplatePickerLoading(false);
        setTemplatePickerError(
          templateResult.status === "rejected" ? "Templates could not be loaded." : "",
        );
      })
      .catch(() => {
        if (!isCurrentSpaceRequest()) return;
        if (isCurrentTreeRequest()) {
          setFolders([]);
          setDocuments([]);
        }
        setTemplates([]);
        setTemplatePickerLoading(false);
        setTemplatePickerError("Templates could not be loaded.");
      });

    return () => {
      cancelled = true;
      spaceContentsRefreshRequestIdRef.current += 1;
      if (spaceContentsRequestIdRef.current === requestId) {
        spaceContentsRequestIdRef.current += 1;
      }
    };
  }, [activeSpaceId]);

  useEffect(() => {
    if (!moveTargetSpaceId || (!movingFolderId && !movingDocumentId)) {
      setMoveTargetFolders([]);
      setMoveTargetDocuments([]);
      return;
    }

    let cancelled = false;
    void Promise.all([
      dataClient.listFolders(moveTargetSpaceId),
      dataClient.listDocuments(moveTargetSpaceId, { include_nested: true }),
    ])
      .then(([nextFolders, nextDocuments]) => {
        if (cancelled) return;
        setMoveTargetFolders(nextFolders);
        setMoveTargetDocuments(nextDocuments);
      })
      .catch(() => {
        if (cancelled) return;
        setMoveTargetFolders([]);
        setMoveTargetDocuments([]);
      });

    return () => {
      cancelled = true;
    };
  }, [moveTargetSpaceId, movingDocumentId, movingFolderId]);

  useEffect(() => {
    if (createSpaceOpen) spaceInputRef.current?.focus();
  }, [createSpaceOpen]);

  const createDialogKind = createDialog?.kind;
  useEffect(() => {
    if (createDialogKind) createTitleInputRef.current?.focus();
  }, [createDialogKind]);

  useEffect(() => {
    if (renameTarget) renameInputRef.current?.focus();
  }, [renameTarget]);

  const refreshSpaceContents = useCallback(async (spaceId: string, verify?: () => void) => {
    verify?.();
    const requestId = ++spaceContentsRefreshRequestIdRef.current;
    const [nextFolders, nextDocs] = await Promise.all([
      dataClient.listFolders(spaceId),
      dataClient.listDocuments(spaceId, { include_nested: true }),
    ]);
    verify?.();
    if (
      !isCurrentSidebarSpaceRequest(
        spaceId,
        requestId,
        activeSpaceIdRef.current,
        spaceContentsRefreshRequestIdRef.current,
      )
    ) {
      return;
    }
    setFolders(nextFolders);
    setDocuments(nextDocs);
  }, []);

  useEffect(
    () =>
      addGraphDirtyListener(({ spaceId, reason }) => {
        if (!shouldRefreshSidebarContents(activeSpaceIdRef.current, { spaceId, reason })) return;
        void refreshSpaceContents(spaceId).catch(() => {});
      }),
    [refreshSpaceContents],
  );

  const handleCreateSpace = async () => {
    if (!canCreateSpace) return;
    const name = newSpaceName.trim();
    if (!name) return;
    try {
      await createWorkspaceSpace(name);
      setCreateSpaceOpen(false);
      setNewSpaceName("");
    } catch {
      // Show nothing — keep input open
    }
  };

  const cancelCreateSpace = () => {
    setCreateSpaceOpen(false);
    setNewSpaceName("");
  };

  const handleCreateFolder = async (name: string, parentId: string | null) => {
    if (!activeSpace) return;
    const normalizedName = name.trim();
    if (!normalizedName) return;
    try {
      await dataClient.createFolder(activeSpace.id, normalizedName, {
        parent_id: parentId,
      });
      const updated = await dataClient.listFolders(activeSpace.id);
      setFolders(updated);
      setCreateDialog(null);
    } catch {
      // Keep input open
    }
  };

  const cancelCreateDialog = () => {
    setCreateDialogError("");
    setCreateDialog(null);
  };

  const startCreateFolder = useCallback(
    (parentId?: string) => {
      setCreateDialogError("");
      const selectedFolder = parentId ? folders.find((folder) => folder.id === parentId) : null;
      setCreateDialog({
        kind: "folder",
        title: "",
        query: selectedFolder
          ? `${activeSpace?.name ?? "Space"} / ${getSidebarFolderPathLabel(folders, selectedFolder.id)}`
          : `${activeSpace?.name ?? "Space"} / Root`,
        folderId: null,
        parentId: parentId ?? null,
      });
    },
    [activeSpace?.name, folders],
  );

  const handleSelectSpace = (space: WorkspaceSpace) => {
    setSpaceSearchQuery("");
    setSpaceMenuOpen(false);
    selectSpace(space);
  };

  const createDocumentFromTemplate = useCallback(
    async (
      folderId?: string,
      template?: TemplateRecord,
      parentId?: string,
      requestedTitle?: string,
      fieldValues: TemplateFieldValues = {},
      sourceId?: string,
      contextDate?: Date,
      createDailyFolder = false,
    ) => {
      if (!activeSpace) return;
      const prepared = template
        ? prepareTemplateCreation({
            template,
            config: getTemplateExperience(template),
            context: {
              date: contextDate,
              title: requestedTitle,
              space: activeSpace.name,
              author: user?.display_name,
              properties: templatePropertiesAsRecord(template.properties),
            },
            fieldValues,
            schemas: templateSchemas,
          })
        : undefined;
      if (prepared?.kind === "held") {
        setTemplatePickerError(prepared.message);
        return false;
      }
      const resolved = prepared?.resolved;
      if (resolved?.unresolvedFields.length) return false;
      const title = requestedTitle?.trim() || resolved?.title || template?.name || "Untitled";
      const presentation =
        resolved?.presentation ??
        (template ? templatePresentationAsRecord(template.properties) : {});
      const content = resolved
        ? applyTemplatePageWidth(resolved.content, presentation)
        : undefined;
      let targetFolderId = folderId;
      try {
        if (createDailyFolder && !targetFolderId) {
          targetFolderId = (findDailyFolder(folders) ??
            (await dataClient.createFolder(activeSpace.id, DAILY_FOLDER_NAME))).id;
        }
        const doc = await dataClient.createDocument(activeSpace.id, {
          title,
          folder_id: targetFolderId ?? null,
          parent_id: parentId ?? null,
          content,
          icon: presentation.icon,
          cover_image: presentation.cover_image,
          template_id: template?.id,
          property_values: resolved?.property_values,
          source_id: sourceId,
        });
        emitGraphDirty(activeSpace.id, "document-created");
        // Creation is confirmed even if the subsequent list refresh is unavailable.
        try {
          const docs = await dataClient.listDocuments(activeSpace.id, { include_nested: true });
          setDocuments(docs);
        } catch { /* The routed page can load its own confirmed identity. */ }
        navigate(getSidebarDocumentPath(activeSpace.id, doc.id));
        return true;
      } catch {
        if (sourceId) {
          try {
            const existing = await dataClient.getDocument(activeSpace.id, sourceId);
            if (
              existing.title === title &&
              (existing.folder_id ?? null) === (targetFolderId ?? null) &&
              (existing.parent_id ?? null) === (parentId ?? null)
            ) {
              emitGraphDirty(activeSpace.id, "document-created");
              navigate(getSidebarDocumentPath(activeSpace.id, sourceId));
              return true;
            }
          } catch {}
        }
        return false;
      }
    },
    [activeSpace, folders, navigate, templateSchemas, user?.display_name],
  );

  const handleCreateDocument = useCallback(
    async (folderId?: string, parentId?: string) => {
      setCreateDialogError("");
      setCreateDialog({
        kind: "document",
        title: "",
        query: parentId
          ? `${activeSpace?.name ?? "Space"} / ${getSidebarDocumentPathLabel(folders, documents, parentId)}`
          : folderId
            ? `${activeSpace?.name ?? "Space"} / ${getSidebarFolderPathLabel(folders, folderId)}`
            : `${activeSpace?.name ?? "Space"} / Root`,
        folderId: folderId ?? null,
        parentId: parentId ?? null,
      });
    },
    [activeSpace?.name, documents, folders],
  );

  useEffect(() => {
    const openDocument = () => {
      void handleCreateDocument();
    };
    const openDocumentFromTemplate = () => {
      if (!activeSpace) return;
      setTemplatePickerDestination(`${activeSpace.name} / Root`);
      setTemplatePickerLoading(true);
      setTemplatePickerError("");
      setTemplateFolderId(undefined);
      setTemplateParentId(undefined);
      setTemplateDocumentTitle(undefined);
      setTemplatePickerDate(undefined);
      setTemplatePickerInitialTemplateId(undefined);
      setTemplatePickerCreatesDailyFolder(false);
      setTemplatePickerAllowsBlank(false);
      setTemplatePickerOpen(true);
      void Promise.all([
        dataClient.listTemplates(activeSpace.id),
        dataClient.listPropertySchemas(activeSpace.id),
      ])
        .then(([items, schemas]) => {
          setTemplates(items);
          setTemplateSchemas(schemas);
        })
        .catch(() => setTemplatePickerError("Templates could not be loaded."))
        .finally(() => setTemplatePickerLoading(false));
    };
    const openFolder = () => startCreateFolder();
    const openSpace = () => {
      if (!canCreateSpace) return;
      setSpaceMenuOpen(false);
      setCreateSpaceOpen(true);
    };
    document.addEventListener(OPEN_SIDEBAR_CREATE_DOCUMENT_EVENT, openDocument);
    document.addEventListener(OPEN_SIDEBAR_CREATE_FROM_TEMPLATE_EVENT, openDocumentFromTemplate);
    document.addEventListener(OPEN_SIDEBAR_CREATE_FOLDER_EVENT, openFolder);
    document.addEventListener(OPEN_SIDEBAR_CREATE_SPACE_EVENT, openSpace);
    return () => {
      document.removeEventListener(OPEN_SIDEBAR_CREATE_DOCUMENT_EVENT, openDocument);
      document.removeEventListener(
        OPEN_SIDEBAR_CREATE_FROM_TEMPLATE_EVENT,
        openDocumentFromTemplate,
      );
      document.removeEventListener(OPEN_SIDEBAR_CREATE_FOLDER_EVENT, openFolder);
      document.removeEventListener(OPEN_SIDEBAR_CREATE_SPACE_EVENT, openSpace);
    };
  }, [activeSpace, canCreateSpace, handleCreateDocument, startCreateFolder]);

  const submitCreateDialog = async () => {
    if (!createDialog || createDialogPending) return;
    const title = createDialog.title.trim();
    if (!title) return;
    if (createDialog.kind === "folder") {
      await handleCreateFolder(title, createDialog.parentId);
      return;
    }
    if (templates.length > 0 || templatePickerLoading || templatePickerError) {
      const templateTarget = getSidebarTemplatePickerTarget(createDialog);
      setTemplateFolderId(templateTarget.folderId);
      setTemplateParentId(templateTarget.parentId);
      setTemplateDocumentTitle(templateTarget.title);
      setTemplatePickerDestination(createDialog.query);
      setTemplatePickerAllowsBlank(true);
      setCreateDialog(null);
      setTemplatePickerOpen(true);
      setTemplates([]);
      setTemplateSchemas([]);
      setTemplatePickerLoading(true);
      setTemplatePickerError("");
      if (activeSpace) {
        void Promise.all([
          dataClient.listTemplates(activeSpace.id),
          dataClient.listPropertySchemas(activeSpace.id),
        ])
          .then(([items, schemas]) => {
            setTemplates(items);
            setTemplateSchemas(schemas);
          })
          .catch(() => setTemplatePickerError("Templates could not be loaded."))
          .finally(() => setTemplatePickerLoading(false));
      } else {
        setTemplatePickerLoading(false);
      }
      return;
    }
    const original = createDialog, spaceId = activeSpaceId;
    const current = () => createDialogRef.current === original && activeSpaceIdRef.current === spaceId;
    setCreateDialogError("");
    setCreateDialogPending(true);
    try {
      await settleSidebarPageCreation(
        () => createDocumentFromTemplate(original.folderId ?? undefined, undefined, original.parentId ?? undefined, title),
        () => { if (current()) setCreateDialog(null); },
        message => { if (current()) setCreateDialogError(message); },
      );
    } finally { setCreateDialogPending(false); }
  };

  const handleSelectTemplate = async (
    template?: TemplateRecord,
    fieldValues?: TemplateFieldValues,
    sourceId?: string,
  ) => {
    const folderId = templateFolderId;
    const parentId = templateParentId;
    const title = templateDocumentTitle;
    const created = await createDocumentFromTemplate(
      folderId,
      template,
      parentId,
      title,
      fieldValues,
      sourceId,
      templatePickerDate,
      templatePickerCreatesDailyFolder,
    );
    if (created) closeTemplatePicker();
    return created;
  };

  const closeTemplatePicker = () => {
    setTemplatePickerOpen(false);
    setTemplateFolderId(undefined);
    setTemplateParentId(undefined);
    setTemplateDocumentTitle(undefined);
    setTemplatePickerAllowsBlank(true);
    setTemplatePickerLoading(false);
    setTemplatePickerError("");
    setTemplatePickerDate(undefined);
    setTemplatePickerInitialTemplateId(undefined);
    setTemplatePickerCreatesDailyFolder(false);
  };

  const handleDeleteDocument = (docId: string) => {
    if (!activeSpace) return;
    const document = documents.find((doc) => doc.id === docId);
    setDeleteTarget({ type: "document", id: docId, name: document?.title ?? "" });
  };

  const handleArchiveDocument = async (docId: string) => {
    if (!activeSpace) return;
    try {
      await dataClient.archiveDocument(activeSpace.id, docId);
      emitGraphDirty(activeSpace.id, "document-metadata-updated");
      setDocuments((prev) => prev.filter((document) => document.id !== docId));
      if (location.pathname.includes(docId)) {
        navigate(getSidebarHomePath(activeSpace.id));
      }
    } catch {}
  };

  const handleDuplicateDocument = async (docId: string) => {
    if (!activeSpace) return;
    try {
      const duplicate = await dataClient.duplicateDocument(activeSpace.id, docId);
      emitGraphDirty(activeSpace.id, "document-created");
      await refreshSpaceContents(activeSpace.id);
      navigate(getSidebarDocumentPath(activeSpace.id, duplicate.id));
    } catch {}
  };

  const confirmDeleteDocument = async (docId: string) => {
    if (!activeSpace) return;
    try {
      await dataClient.deleteDocument(activeSpace.id, docId);
      emitGraphDirty(activeSpace.id, "document-deleted");
      setDocuments((prev) => prev.filter((d) => d.id !== docId));
      if (location.pathname.includes(docId)) {
        navigate(getSidebarHomePath(activeSpace.id));
      }
    } catch {
    } finally {
      setDeleteTarget(null);
    }
  };

  const handleRenameDocument = async (
    docId: string,
    currentTitle: string,
    currentIcon?: string,
  ) => {
    if (!activeSpace) return;
    const nextTitle = renameName.trim();
    const nextIcon = renameIcon.trim() || undefined;
    if (!nextTitle || (nextTitle === currentTitle && nextIcon === currentIcon)) return;
    try {
      const selection = captureDraftSaveSelection();
      const draft = await dataClient.createDocumentDraft(activeSpace.id, docId);
      verifyDraftSaveSelection(selection);
      if (
        renameStateRef.current.target?.id !== docId ||
        renameStateRef.current.name.trim() !== nextTitle ||
        (renameStateRef.current.icon.trim() || undefined) !== nextIcon
      )
        throw new Error(
          "Your newer rename remains open. Review the draft request before saving it.",
        );
      await dataClient.updateDocumentDraft(
        activeSpace.id,
        docId,
        draft.id,
        {
          title: nextTitle,
          icon: nextIcon,
        },
        draft.source_digest,
      );
      verifyDraftSaveSelection(selection);
      if (
        renameStateRef.current.target?.id !== docId ||
        renameStateRef.current.name.trim() !== nextTitle ||
        (renameStateRef.current.icon.trim() || undefined) !== nextIcon
      )
        throw new Error("The original rename was stored. Your newer rename remains open.");
      setRenameTarget(null);
      setRenameName("");
      setRenameIcon("");
      navigate(
        `${buildDocumentRoutePath(activeSpace.id, docId)}?draft=${encodeURIComponent(draft.id)}`,
      );
    } catch (error) {
      setRenameError(
        error instanceof Error
          ? error.message
          : "Could not confirm the rename. Your input remains open.",
      );
    }
  };

  const handleRenameFolder = async (
    folderId: string,
    currentName: string,
    currentIcon?: string,
  ) => {
    if (!activeSpace) return;
    const nextName = renameName.trim();
    const nextIcon = renameIcon.trim() || undefined;
    if (!nextName || (nextName === currentName && nextIcon === currentIcon)) return;
    try {
      await dataClient.updateFolder(activeSpace.id, folderId, { name: nextName, icon: nextIcon });
      setFolders((prev) =>
        prev.map((folder) =>
          folder.id === folderId ? { ...folder, name: nextName, icon: nextIcon } : folder,
        ),
      );
      setRenameTarget(null);
      setRenameName("");
      setRenameIcon("");
    } catch {}
  };

  const startRenameDocument = (docId: string, currentTitle: string, currentIcon?: string) => {
    setRenameError("");
    setRenameTarget({ type: "document", id: docId, currentName: currentTitle, currentIcon });
    setRenameName(currentTitle);
    setRenameIcon(currentIcon ?? "");
  };

  const startRenameFolder = (folderId: string, currentName: string, currentIcon?: string) => {
    setRenameTarget({ type: "folder", id: folderId, currentName, currentIcon });
    setRenameName(currentName);
    setRenameIcon(currentIcon ?? "");
  };

  const cancelRename = () => {
    setRenameError("");
    setRenameTarget(null);
    setRenameName("");
    setRenameIcon("");
  };

  const submitRename = async () => {
    if (!renameTarget) return;
    if (renameTarget.type === "folder") {
      await handleRenameFolder(renameTarget.id, renameTarget.currentName, renameTarget.currentIcon);
    } else {
      await handleRenameDocument(
        renameTarget.id,
        renameTarget.currentName,
        renameTarget.currentIcon,
      );
    }
  };

  const openLocalReviewedMove = async (intent: LocalMovePreviewIntent, title: string) => {
    const identity = {};
    localReviewIdentity.current = identity;
    const route = moveRouteRef.current;
    const selectors = moveSelectorsRef.current;
    let reviewed = false;
    const current = () =>
      moveMountedRef.current &&
      localReviewIdentity.current === identity &&
      activeSpaceIdRef.current === intent.source_space_id &&
      moveRouteRef.current === route &&
      (reviewed || moveSelectorsRef.current === selectors);
    try {
      setMoveReviewError("");
      const session = await (getSyncProvider() === "google-drive"
        ? dataClient.reviewGoogleDriveMove(intent, current)
        : dataClient.reviewLocalMove(intent, current));
      session.verifyCurrent();
      if (!current()) throw new Error("The original move selection changed.");
      reviewed = true;
      setLocalReviewedMove({
        identity,
        source: intent.source_space_id,
        intent,
        session,
        title,
        destinationName:
          spaces.find((space) => space.id === intent.destination.space_id)?.name ??
          "Destination space",
      });
      setMovingFolderId(null);
      setMovingDocumentId(null);
    } catch (error) {
      if (current())
        setMoveReviewError(
          error instanceof Error
            ? error.message
            : "The move review is unavailable. Keep this selection and inspect again.",
        );
    }
  };

  const handleMoveFolder = async (folderId: string, parentId: string | null) => {
    if (!activeSpace) return;
    const targetSpaceId = moveTargetSpaceId || activeSpace.id;
    if (localMode && targetSpaceId !== activeSpace.id) {
      await openLocalReviewedMove(
        {
          source_space_id: activeSpace.id,
          root: { kind: "folder", id: folderId },
          destination: { space_id: targetSpaceId, folder_id: null, parent_id: parentId },
        },
        folders.find((folder) => folder.id === folderId)?.name ?? "Folder",
      );
      return;
    }
    if (!localMode && targetSpaceId !== activeSpace.id) {
      let context: ReviewedMoveContext;
      try {
        context = captureReviewedMoveContext();
        setMoveReviewError("");
      } catch {
        setMoveReviewError(
          "The original move workspace is unavailable. Keep this selection and reopen it in the current account.",
        );
        return;
      }
      setReviewedMove({
        context,
        source: activeSpace.id,
        intent: {
          root: { kind: "folder", id: folderId },
          destination: {
            space_id: targetSpaceId,
            folder_id: null,
            parent_id: parentId,
          },
        },
        title: folders.find((f) => f.id === folderId)?.name ?? "Folder",
        destinationName: spaces.find((s) => s.id === targetSpaceId)?.name ?? "Destination space",
      });
      setMovingFolderId(null);
      return;
    }
    try {
      await dataClient.moveFolder(activeSpace.id, folderId, {
        target_space_id: targetSpaceId,
        parent_id: parentId,
      });
      await refreshSpaceContents(activeSpace.id);
      setMovingFolderId(null);
      setMoveTargetSpaceId("");
      setMoveLocationQuery("");
    } catch {}
  };

  const handleMoveDocument = async (
    docId: string,
    targetSpaceId: string,
    folderId: string | null,
    parentId: string | null,
  ) => {
    if (!activeSpace) return;
    if (localMode && targetSpaceId !== activeSpace.id) {
      await openLocalReviewedMove(
        {
          source_space_id: activeSpace.id,
          root: { kind: "document", id: docId },
          destination: { space_id: targetSpaceId, folder_id: folderId, parent_id: parentId },
        },
        documents.find((page) => page.id === docId)?.title ?? "Page",
      );
      return;
    }
    if (!localMode && targetSpaceId !== activeSpace.id) {
      let context: ReviewedMoveContext;
      try {
        context = captureReviewedMoveContext();
        setMoveReviewError("");
      } catch {
        setMoveReviewError(
          "The original move workspace is unavailable. Keep this selection and reopen it in the current account.",
        );
        return;
      }
      setReviewedMove({
        context,
        source: activeSpace.id,
        intent: {
          root: { kind: "document", id: docId },
          destination: {
            space_id: targetSpaceId,
            folder_id: folderId,
            parent_id: parentId,
          },
        },
        title: documents.find((d) => d.id === docId)?.title ?? "Page",
        destinationName: spaces.find((s) => s.id === targetSpaceId)?.name ?? "Destination space",
      });
      setMovingDocumentId(null);
      return;
    }
    try {
      await dataClient.moveDocument(activeSpace.id, docId, {
        target_space_id: targetSpaceId,
        folder_id: folderId,
        parent_id: parentId,
      });
      if (targetSpaceId === activeSpace.id) {
        emitDocumentLocationChanged({
          spaceId: activeSpace.id,
          documentId: docId,
          folderId,
          parentId,
        });
      }
      emitGraphDirty(activeSpace.id, "document-metadata-updated");
      if (targetSpaceId === activeSpace.id) {
        await refreshSpaceContents(activeSpace.id);
        navigate(getSidebarDocumentPath(activeSpace.id, docId));
      } else {
        await refreshSpaceContents(activeSpace.id);
        navigate(getSidebarHomePath(activeSpace.id));
      }
      setMovingDocumentId(null);
      setMoveTargetSpaceId("");
      setMoveLocationQuery("");
    } catch {}
  };

  const handleDropFolderOnParent = (folderId: string, parentId: string) => {
    if (!canDropSidebarFolderOnParent(folders, folderId, parentId, localMode)) return;
    void handleMoveFolder(folderId, parentId);
  };

  const handleDeleteFolder = (folderId: string) => {
    if (!activeSpace) return;
    const folder = folders.find((item) => item.id === folderId);
    setDeleteTarget({ type: "folder", id: folderId, name: folder?.name ?? "" });
  };

  const confirmDeleteFolder = async (folderId: string) => {
    if (!activeSpace) return;
    const nestedFolderIds = new Set<string>([folderId]);
    let changed = true;
    while (changed) {
      changed = false;
      for (const folder of folders) {
        if (
          folder.parent_id &&
          nestedFolderIds.has(folder.parent_id) &&
          !nestedFolderIds.has(folder.id)
        ) {
          nestedFolderIds.add(folder.id);
          changed = true;
        }
      }
    }
    const deletedDocIds = documents
      .filter((doc) => doc.folder_id && nestedFolderIds.has(doc.folder_id))
      .map((doc) => doc.id);
    try {
      await dataClient.deleteFolder(activeSpace.id, folderId);
      emitGraphDirty(activeSpace.id, "document-deleted");
      await refreshSpaceContents(activeSpace.id);
      if (deletedDocIds.some((docId) => location.pathname.includes(docId))) {
        navigate(getSidebarHomePath(activeSpace.id));
      }
    } catch {
    } finally {
      setDeleteTarget(null);
    }
  };

  const submitDeleteTarget = async () => {
    if (!deleteTarget) return;
    if (deleteTarget.type === "document") {
      await confirmDeleteDocument(deleteTarget.id);
    } else {
      await confirmDeleteFolder(deleteTarget.id);
    }
  };

  const handleOpenToday = useCallback(async () => {
    if (!activeSpace) return;
    const today = new Date();
    const todayTitle = formatDateStamp(today);
    try {
      const docs = await dataClient.listDocuments(activeSpace.id, { include_nested: true });
      const existing = docs.find((doc) => doc.title === todayTitle);
      if (existing) {
        navigate(getSidebarDocumentPath(activeSpace.id, existing.id));
        return;
      }

      const dailyTemplate = findDailyTemplate(templates);
      const folders = await dataClient.listFolders(activeSpace.id);
      const dailyFolder = findDailyFolder(folders);
      const prepared = dailyTemplate
        ? prepareTemplateCreation({
            template: dailyTemplate,
            config: getTemplateExperience(dailyTemplate),
            context: {
              date: today,
              title: todayTitle,
              space: activeSpace.name,
              author: user?.display_name,
            },
            schemas: templateSchemas,
          })
        : undefined;
      if (prepared?.kind === "held") {
        setTemplateFolderId(dailyFolder?.id);
        setTemplateDocumentTitle(todayTitle);
        setTemplatePickerDestination(`${activeSpace.name} / ${DAILY_FOLDER_NAME}`);
        setTemplatePickerInitialTemplateId(dailyTemplate?.id);
        setTemplatePickerAllowsBlank(false);
        setTemplatePickerError(prepared.message);
        setTemplatePickerOpen(true);
        return;
      }
      const resolved = prepared?.resolved;
      if (resolved?.unresolvedFields.length) {
        setTemplateFolderId(dailyFolder?.id);
        setTemplateParentId(undefined);
        setTemplateDocumentTitle(todayTitle);
        setTemplatePickerDestination(`${activeSpace.name} / ${DAILY_FOLDER_NAME}`);
        setTemplatePickerDate(today);
        setTemplatePickerInitialTemplateId(dailyTemplate?.id);
        setTemplatePickerCreatesDailyFolder(!dailyFolder);
        setTemplatePickerAllowsBlank(false);
        setTemplatePickerOpen(true);
        return;
      }
      const operationKey = `${activeSpace.id}:${todayTitle}`;
      const sourceId =
        dailyCreateSourceIdsRef.current.get(operationKey) ?? `doc-${createDocumentSourceId()}`;
      dailyCreateSourceIdsRef.current.set(operationKey, sourceId);
      const created = await createDocumentFromTemplate(
        dailyFolder?.id,
        dailyTemplate,
        undefined,
        todayTitle,
        {},
        sourceId,
        today,
        !dailyFolder,
      );
      if (created) {
        dailyCreateSourceIdsRef.current.delete(operationKey);
        await refreshSpaceContents(activeSpace.id);
      }
    } catch {}
  }, [
    activeSpace,
    createDocumentFromTemplate,
    navigate,
    refreshSpaceContents,
    templateSchemas,
    templates,
    user?.display_name,
  ]);

  const requestOpenToday = useCallback(() => {
    if (shouldQueueSidebarDailyNote(activeSpace)) {
      setPendingDailyNote(true);
      return;
    }
    void handleOpenToday();
  }, [activeSpace, handleOpenToday]);

  useEffect(() => {
    if (!pendingDailyNote || shouldQueueSidebarDailyNote(activeSpace)) return;
    setPendingDailyNote(false);
    void handleOpenToday();
  }, [activeSpace, handleOpenToday, pendingDailyNote]);

  useEffect(() => {
    const openDailyNote = () => {
      requestOpenToday();
    };
    document.addEventListener(OPEN_SIDEBAR_DAILY_NOTE_EVENT, openDailyNote);
    return () => document.removeEventListener(OPEN_SIDEBAR_DAILY_NOTE_EVENT, openDailyNote);
  }, [requestOpenToday]);

  const homePath = getSidebarHomePath(activeSpace?.id);
  const recentPath = getSidebarRecentPath(activeSpace?.id);
  const journalPath = getSidebarJournalPath(activeSpace?.id);
  const workflowPath = getSidebarWorkflowPath(activeSpace?.id);
  const starredPath = getSidebarStarredPath(activeSpace?.id);
  const homeActive = Boolean(activeSpace) && isSidebarRouteActive(location.pathname, homePath);
  const recentActive = Boolean(activeSpace) && isSidebarRouteActive(location.pathname, recentPath);
  const journalActive =
    Boolean(activeSpace) && isSidebarRouteActive(location.pathname, journalPath);
  const workflowActive =
    Boolean(activeSpace) && isSidebarRouteActive(location.pathname, workflowPath);
  const starredActive =
    Boolean(activeSpace) && isSidebarRouteActive(location.pathname, starredPath);
  const spaceSettingsActive =
    Boolean(activeSpace) &&
    isSidebarRouteActive(location.pathname, getSidebarSpaceSettingsPath(activeSpace?.id));
  const canAdminSpace = activeSpace?.member_role === "owner";
  const canShowSpaceAdmin = canAdminSpace || localMode;
  const spaceSections = getWorkspaceSpaceSections(spaces, recentSpaceIds, spaceSearchQuery);
  const visibleSpaceCount = spaceSections.reduce(
    (count, section) => count + section.spaces.length,
    0,
  );
  const movingFolder = movingFolderId
    ? (folders.find((folder) => folder.id === movingFolderId) ?? null)
    : null;
  const movingDocument = movingDocumentId
    ? (documents.find((doc) => doc.id === movingDocumentId) ?? null)
    : null;
  const selectedMoveSpace = spaces.find((space) => space.id === moveTargetSpaceId) ?? activeSpace;
  const validMoveTargetFolders = useMemo(() => {
    if (!movingFolder || !selectedMoveSpace || !activeSpaceId) return [];
    try {
      const targets = folderCatalogMoveTargets(
        folderCatalog,
        activeSpaceId,
        movingFolder.id,
        moveFolderCatalog,
        selectedMoveSpace.id,
      );
      return sortSidebarFolders(moveTargetFolders.filter((target) => targets.has(target.id)));
    } catch {
      return [];
    }
  }, [
    movingFolder,
    selectedMoveSpace,
    activeSpaceId,
    folderCatalog,
    moveFolderCatalog,
    moveTargetFolders,
  ]);
  const canMoveSelectedFolderToRoot =
    movingFolder && selectedMoveSpace
      ? selectedMoveSpace.id !== activeSpace?.id || canMoveSidebarFolderToRoot(movingFolder, true)
      : false;
  const moveDocumentFolderTargets = movingDocument
    ? sortSidebarFolders(
        moveTargetFolders.filter(
          (row) =>
            (moveFolderCatalog.depths.get(row.id) ?? 0) > 0 &&
            moveFolderCatalog.depths.get(row.id)! <= FOLDER_OPERATION_NODE_LIMIT,
        ),
      )
    : [];
  const moveDocumentPageTargets = useMemo(() => {
    if (!movingDocument || !selectedMoveSpace || !activeSpaceId) return [];
    try {
      const targets = pageCatalogMoveTargets(
        pageCatalog,
        activeSpaceId,
        movingDocument.id,
        movePageCatalog,
        selectedMoveSpace.id,
      );
      return sortSidebarDocuments(moveTargetDocuments.filter((row) => targets.has(row.id)));
    } catch {
      return [];
    }
  }, [
    movingDocument,
    selectedMoveSpace,
    activeSpaceId,
    pageCatalog,
    movePageCatalog,
    moveTargetDocuments,
  ]);
  const moveDocumentLocationOptions =
    movingDocument && selectedMoveSpace
      ? [
          {
            id: "root",
            label: `${selectedMoveSpace.name} / Root`,
            description: "Top level",
            icon: "document" as const,
            folderId: null,
            parentId: null,
          },
          ...moveDocumentFolderTargets.map((target) => ({
            id: `folder:${target.id}`,
            label: `${selectedMoveSpace.name} / ${moveFolderCatalog.labels.get(target.id) ?? target.name}`,
            description: "Folder",
            icon: "folder" as const,
            folderId: target.id,
            parentId: null,
          })),
          ...moveDocumentPageTargets.map((target) => ({
            id: `document:${target.id}`,
            label: `${selectedMoveSpace.name} / ${sidebarPageLocationLabel(moveFolderCatalog, movePageCatalog, target)}`,
            description: "Child page",
            icon: "document" as const,
            folderId: target.folder_id ?? null,
            parentId: target.id,
          })),
        ]
      : [];
  const moveFolderLocationOptions =
    movingFolder && selectedMoveSpace
      ? [
          ...(canMoveSelectedFolderToRoot
            ? [
                {
                  id: "root",
                  label: `${selectedMoveSpace.name} / Root`,
                  description: "Top level",
                  icon: "folder" as const,
                  folderId: null,
                  parentId: null,
                },
              ]
            : []),
          ...validMoveTargetFolders.map((target) => ({
            id: `folder:${target.id}`,
            label: `${selectedMoveSpace.name} / ${moveFolderCatalog.labels.get(target.id) ?? target.name}`,
            description: "Folder",
            icon: "folder" as const,
            folderId: target.id,
            parentId: null,
          })),
        ]
      : [];
  const canDropFolderOnRoot = canDropSidebarFolderOnRoot(folders, draggingFolderId, localMode);
  const createLocationOptions =
    createDialog && activeSpace
      ? getSidebarCreateLocationOptions(
          activeSpace.name,
          folders,
          documents,
          createDialog.kind,
          folderCatalog,
          pageCatalog,
        )
      : [];
  const selectedCreateLocation = createDialog
    ? (createLocationOptions.find(
        (option) =>
          option.folderId === createDialog.folderId && option.parentId === createDialog.parentId,
      ) ?? createLocationOptions[0])
    : null;

  const handleRootFolderDragOver = (event: DragEvent<HTMLDivElement>) => {
    if (!canDropFolderOnRoot) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
  };

  const handleRootFolderDrop = (event: DragEvent<HTMLDivElement>) => {
    if (!canDropFolderOnRoot || !draggingFolderId) return;
    event.preventDefault();
    void handleMoveFolder(draggingFolderId, null);
    setDraggingFolderId(null);
  };

  const openMoveFolder = (folderId: string) => {
    setMovingDocumentId(null);
    setMovingFolderId(folderId);
    setMoveTargetSpaceId(activeSpace?.id ?? "");
    setMoveLocationQuery("");
  };

  const openMoveDocument = useCallback(
    (docId: string) => {
      setMovingFolderId(null);
      setMovingDocumentId(docId);
      setMoveTargetSpaceId(activeSpace?.id ?? "");
      setMoveLocationQuery("");
      const requestId = ++moveDocumentLookupRequestIdRef.current;
      const sourceSpaceId = activeSpace?.id;
      if (sourceSpaceId && !documents.some((doc) => doc.id === docId)) {
        void dataClient
          .getDocument(sourceSpaceId, docId)
          .then((document) => {
            if (
              requestId !== moveDocumentLookupRequestIdRef.current ||
              activeSpaceIdRef.current !== sourceSpaceId
            ) {
              return;
            }
            const moveSource: DocItem = {
              id: document.id,
              title: document.title,
              icon: document.icon,
              folder_id: document.folder_id,
              parent_id: document.parent_id,
            };
            setDocuments((current) =>
              current.some((item) => item.id === moveSource.id)
                ? current
                : [...current, moveSource],
            );
          })
          .catch((error) => {
            if (
              requestId !== moveDocumentLookupRequestIdRef.current ||
              activeSpaceIdRef.current !== sourceSpaceId
            ) {
              return;
            }
            emitTransientSystemNotification({
              title: "Could not load page for moving",
              body: error instanceof Error ? error.message : "The page could not be loaded.",
            });
          });
      }
    },
    [activeSpace?.id, documents],
  );

  useEffect(() => {
    const handler = (event: Event) => {
      const documentId = (event as CustomEvent<{ documentId?: string }>).detail?.documentId;
      if (!documentId) return;
      openMoveDocument(documentId);
    };
    document.addEventListener(OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT, handler);
    return () => document.removeEventListener(OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT, handler);
  }, [openMoveDocument]);

  const renderSpaceOption = (space: WorkspaceSpace) => {
    const selected = activeSpace?.id === space.id;
    const ownerLabel = getPersonalSpaceOwnerLabel(space);
    return (
      <button
        key={space.id}
        type="button"
        role="menuitemradio"
        aria-checked={selected}
        onClick={() => handleSelectSpace(space)}
        className={`rounded-button flex w-full items-center gap-2 px-2 ${getSidebarNavigationTargetClass(mobileSettingsMenu)} text-left text-sm transition-colors duration-100 ${
          selected
            ? "bg-surface-raised text-text"
            : "text-text-muted hover:bg-surface-raised hover:text-text"
        }`}
      >
        <span className="bg-bg flex h-5 w-5 shrink-0 items-center justify-center rounded text-[10px]">
          <ContentIdentityIcon
            value={space.icon}
            size={14}
            fallback={space.name[0]?.toUpperCase()}
          />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate">{space.name}</span>
          {ownerLabel && (
            <span className="text-text-muted block truncate text-xs">{ownerLabel}</span>
          )}
        </span>
        {selected && <Check size={13} className="shrink-0" />}
      </button>
    );
  };

  const spaceSelector = (
    <Popover
      open={spaceMenuOpen}
      onOpenChange={(open) => {
        setSpaceMenuOpen(open);
        if (!open) setSpaceSearchQuery("");
        if (open) setAccountMenuOpen(false);
      }}
    >
      <PopoverTrigger asChild>
        <button
          type="button"
          ref={spaceMenuTriggerRef}
          className={`rounded-button border-border bg-surface text-text hover:bg-surface-raised flex w-full items-center gap-2 border text-left text-sm transition-colors duration-100 ${
            collapsed
              ? "h-9 justify-center px-0"
              : `${mobileSettingsMenu ? "min-h-11" : ""} px-2 py-2`
          }`}
          aria-label="Select space"
          title={`Select space: ${activeSpace?.name || "No spaces"}`}
        >
          <span className="rounded-button bg-surface-raised flex h-6 w-6 shrink-0 items-center justify-center text-xs">
            {activeSpace?.icon || activeSpace?.name?.[0]?.toUpperCase() || "S"}
          </span>
          {!collapsed && (
            <>
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium">
                  {activeSpace?.name || "No spaces"}
                </span>
                <span className="text-text-muted block truncate text-[11px]">Space</span>
              </span>
              <ChevronDown
                size={14}
                className={getSidebarSpaceSelectorChevronClass(spaceMenuOpen)}
              />
            </>
          )}
        </button>
      </PopoverTrigger>

      <PopoverContent
        align="start"
        side="bottom"
        sideOffset={4}
        className="w-[min(22rem,calc(100vw-2rem))] overflow-hidden p-0"
      >
        <div
          ref={spaceMenuContentRef}
          role="menu"
          aria-label="Spaces"
          onKeyDown={handleSpaceMenuKeyDown}
          className="overflow-hidden"
        >
          <div className="border-border border-b p-1.5">
            <SearchInput
              ref={spaceSearchInputRef}
              value={spaceSearchQuery}
              onChange={(event) => setSpaceSearchQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Escape") {
                  setSpaceSearchQuery("");
                  closeSpaceMenuAndRestoreFocus();
                }
              }}
              placeholder="Search spaces"
              aria-label="Search spaces"
              className="h-8 pl-8 text-xs"
            />
          </div>
          <div className="max-h-64 scrollbar-none overflow-y-auto">
            {spaces.length === 0 ? (
              <div className="text-text-muted px-2 py-2 text-sm">No spaces</div>
            ) : visibleSpaceCount === 0 ? (
              <div className="text-text-muted px-2 py-2 text-sm">No matching spaces</div>
            ) : (
              spaceSections.map((section) => (
                <div key={section.label} className="p-1">
                  <div className="text-text-muted px-1.5 pt-1 pb-1 text-xs font-medium">
                    {section.label}
                  </div>
                  <div className="space-y-0.5">{section.spaces.map(renderSpaceOption)}</div>
                </div>
              ))
            )}
          </div>
          {canCreateSpace && (
            <div className="border-border border-t p-1">
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setSpaceMenuOpen(false);
                  setCreateSpaceOpen(true);
                }}
                className={`rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex w-full items-center gap-2 px-2 ${getSidebarNavigationTargetClass(mobileSettingsMenu)} text-sm transition-colors duration-100`}
              >
                <Plus size={14} />
                New space
              </button>
            </div>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );

  const rootFolders = sortSidebarFolders(folderCatalog.roots);
  const rootDocuments = sortSidebarDocuments(
    pageCatalog.roots.filter((document) => !document.folder_id),
  );
  const currentTreeItemId = getSidebarCurrentTreeItemId(
    documents,
    activeSpace?.id,
    location.pathname,
  );
  const fallbackTreeItemId = rootFolders[0]
    ? `folder:${rootFolders[0].id}`
    : rootDocuments[0]
      ? `document:${rootDocuments[0].id}`
      : null;
  const workspaceUtilities = (
    <div
      role="group"
      aria-label="Workspace utilities"
      className={getSidebarUtilityGroupClass(collapsed)}
    >
      <button
        type="button"
        onClick={() => openSidebarGlobalSearch(activeSpace?.id)}
        className={getSidebarUtilityActionClass(collapsed, mobileSettingsMenu)}
        aria-label="Search documents"
        title="Search documents (⌘K)"
      >
        <Search size={collapsed ? 15 : 14} className="sidebar-icon-blue" />
      </button>
      {!localMode && (
        <NotificationBell
          touchTarget={mobileSettingsMenu}
          presentation="dialog"
          onOpenChange={mobileSettingsMenu ? setNotificationCenterOpen : undefined}
          containerClassName={
            collapsed ? "flex w-full items-center" : "flex min-w-0 flex-1 items-center"
          }
          iconClassName="sidebar-icon-blue-mid"
          className={
            mobileSettingsMenu
              ? `${CONTROL_CHROME_ACTION_CLASS} sidebar-icon-blue-mid !h-11 !w-full`
              : collapsed
                ? `${CONTROL_CHROME_ACTION_CLASS} sidebar-icon-blue-mid !h-9 !w-full`
                : `${CONTROL_CHROME_ACTION_CLASS} sidebar-icon-blue-mid !h-8 !w-full`
          }
        />
      )}
      {onOpenCommandMenu && (
        <button
          type="button"
          onClick={onOpenCommandMenu}
          className={getSidebarUtilityActionClass(collapsed, mobileSettingsMenu)}
          aria-label="Open command menu"
          title="Command menu (⌘P)"
        >
          <Command size={collapsed ? 15 : 14} className="sidebar-icon-blue-deep" />
        </button>
      )}
    </div>
  );
  const accountMenu = mobileSettingsMenu && (
    <Popover
      open={accountMenuOpen}
      onOpenChange={(open) => {
        setAccountMenuOpen(open);
        if (open) setSpaceMenuOpen(false);
      }}
    >
      <PopoverTrigger asChild>
        <button
          type="button"
          ref={accountMenuTriggerRef}
          className={`rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex items-center transition-colors duration-100 ${
            mobileSettingsMenu
              ? "min-h-11 w-full gap-3 px-2"
              : collapsed
                ? "h-9 w-9 justify-center"
                : "h-10 w-full gap-2 px-2 text-left"
          }`}
          aria-label="Open account menu"
          title={collapsed ? "Account" : undefined}
          aria-haspopup="menu"
          aria-expanded={accountMenuOpen}
          aria-controls="sidebar-account-menu"
        >
          {user?.avatar_url ? (
            <img src={user.avatar_url} alt="" className="h-5 w-5 rounded-full object-cover" />
          ) : (
            <span className="bg-primary flex h-5 w-5 items-center justify-center rounded-full text-[10px] font-medium text-white">
              {user?.display_name?.[0]?.toUpperCase() || "S"}
            </span>
          )}
          {!collapsed && (
            <>
              <span className="min-w-0 flex-1">
                <span className="text-text block truncate text-sm font-medium">
                  {user?.display_name || (localMode ? "Local workspace" : "Account")}
                </span>
                <span className="text-text-muted block truncate text-[11px]">
                  {user?.email || "Account"}
                </span>
              </span>
              <ChevronUp size={14} className="shrink-0" />
            </>
          )}
        </button>
      </PopoverTrigger>
      <PopoverContent align="start" side="top" sideOffset={4} className="w-56 p-0">
        <div
          id="sidebar-account-menu"
          ref={accountMenuContentRef}
          role="menu"
          aria-label="Account"
          onKeyDown={handleAccountMenuKeyDown}
          className="overflow-hidden"
        >
          {!localMode && user && (
            <>
              <div className="border-border border-b px-3 py-2">
                <p className="text-text truncate text-sm font-medium">{user.display_name}</p>
                <p className="text-text-muted truncate text-xs">{user.email}</p>
              </div>
              <div className="p-1">
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setAccountMenuOpen(false);
                    navigate(
                      SIDEBAR_ACCOUNT_SETTINGS_PATH,
                      getSidebarSettingsNavigationOptions(mobileSettingsMenu, location),
                    );
                  }}
                  className={getSidebarAccountMenuItemClass(mobileSettingsMenu)}
                >
                  <UserCircle size={14} />
                  Account settings
                </button>
              </div>
            </>
          )}
          {shouldShowSidebarAppSettings(localMode, mobileSettingsMenu) && onOpenAppSettings && (
            <div className="p-1">
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setAccountMenuOpen(false);
                  onOpenAppSettings();
                }}
                className={getSidebarAccountMenuItemClass(mobileSettingsMenu)}
              >
                <Settings size={14} />
                App settings
              </button>
            </div>
          )}
          {!localMode && user && (
            <div className="border-border border-t p-1">
              <button
                type="button"
                role="menuitem"
                onClick={() => {
                  setAccountMenuOpen(false);
                  logout();
                }}
                className={getSidebarAccountMenuItemClass(mobileSettingsMenu, true)}
              >
                <LogOut size={14} />
                Sign out
              </button>
            </div>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
  const workspaceFooter = (
    <footer
      className={`border-border/60 bg-surface shrink-0 border-t ${
        collapsed ? "space-y-1 p-2" : "p-2"
      }`}
      aria-label="Sidebar footer"
    >
      {accountMenu}
      <div className={`flex items-center ${collapsed ? "mt-1 flex-col gap-1" : "mt-1 gap-1"}`}>
        {runtime === "web" && mobileSettingsMenu && <AppearanceToggle touchTarget />}
      </div>
    </footer>
  );

  return (
    <div className="flex h-full min-h-0 flex-col">
      {showHeader && (
        <div className="border-border/60 flex shrink-0 border-b p-2">
          <div
            role="img"
            aria-label="Starlane Notes"
            className={`flex min-w-0 items-center ${
              collapsed ? "h-9 w-9 justify-center" : "px-1 py-1"
            }`}
            title={collapsed ? "Starlane Notes" : undefined}
          >
            <BrandLogo size="md" showText={!collapsed} variant="sidebar" />
          </div>
        </div>
      )}
      {!showHeader && mobileSettingsMenu && (
        <div className="border-border/60 shrink-0 border-b p-2">{workspaceUtilities}</div>
      )}
      <div className="border-border/60 shrink-0 border-b p-2">{spaceSelector}</div>
      {!mobileSettingsMenu && (
        <div className="shrink-0 px-2 pt-2">
          <button
            type="button"
            onClick={() => openSidebarGlobalSearch(activeSpace?.id)}
            className={`focus-ring rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex min-h-9 w-full items-center gap-2 px-2 text-sm ${collapsed ? "justify-center" : ""}`}
            aria-label="Search documents"
            aria-haspopup="dialog"
            aria-keyshortcuts="Meta+K Control+K"
            title="Search documents (⌘K)"
          >
            <Search size={16} aria-hidden="true" />
            {!collapsed && (
              <>
                <span>Search</span>
                <kbd className="ml-auto text-xs">⌘K</kbd>
              </>
            )}
          </button>
        </div>
      )}
      {/* Navigation tree */}
      <nav className="min-h-0 flex-1 overflow-y-auto p-2" aria-label="Space navigation">
        <div className="border-border/60 mb-2 space-y-0.5 border-b pb-2">
          <div role="group" aria-label="Main navigation" className="space-y-0.5">
            <button
              type="button"
              onClick={() => navigate(homePath)}
              disabled={!activeSpace}
              aria-label="Home"
              aria-current={getSidebarNavigationAriaCurrent(homeActive)}
              title={collapsed ? "Home" : undefined}
              className={`rounded-button flex w-full items-center gap-2 border-l-2 px-2 ${getSidebarNavigationTargetClass(mobileSettingsMenu)} text-sm transition-colors duration-100 disabled:opacity-40 ${collapsed ? "justify-center" : ""} ${getSidebarNavigationItemStateClass(homeActive)}`}
            >
              <Home size={14} className="sidebar-icon-blue shrink-0" />
              {!collapsed && "Home"}
            </button>
            {!localMode && (
              <button
                type="button"
                onClick={() => navigate("/shared-with-me")}
                aria-label="Shared with me"
                aria-current={getSidebarNavigationAriaCurrent(
                  location.pathname === "/shared-with-me",
                )}
                title={collapsed ? "Shared with me" : undefined}
                className={`rounded-button flex w-full items-center gap-2 border-l-2 px-2 ${getSidebarNavigationTargetClass(mobileSettingsMenu)} text-sm transition-colors duration-100 ${collapsed ? "justify-center" : ""} ${getSidebarNavigationItemStateClass(location.pathname === "/shared-with-me")}`}
              >
                <UserCircle size={14} className="sidebar-icon-blue shrink-0" />
                {!collapsed && "Shared with me"}
              </button>
            )}
            <button
              type="button"
              onClick={() => navigate(recentPath)}
              disabled={!activeSpace}
              aria-label="Recent"
              aria-current={getSidebarNavigationAriaCurrent(recentActive)}
              title={collapsed ? "Recent" : undefined}
              className={`rounded-button flex w-full items-center gap-2 border-l-2 px-2 ${getSidebarNavigationTargetClass(mobileSettingsMenu)} text-sm transition-colors duration-100 disabled:opacity-40 ${collapsed ? "justify-center" : ""} ${getSidebarNavigationItemStateClass(recentActive)}`}
            >
              <Clock3 size={14} className="sidebar-icon-blue-mid shrink-0" />
              {!collapsed && "Recent"}
            </button>
            <button
              type="button"
              onClick={() => navigate(starredPath)}
              disabled={!activeSpace}
              aria-label="Favorites"
              aria-current={getSidebarNavigationAriaCurrent(starredActive)}
              title={collapsed ? "Favorites" : undefined}
              className={`rounded-button flex w-full items-center gap-2 border-l-2 px-2 ${getSidebarNavigationTargetClass(mobileSettingsMenu)} text-sm transition-colors duration-100 disabled:opacity-40 ${collapsed ? "justify-center" : ""} ${getSidebarNavigationItemStateClass(starredActive)}`}
            >
              <Star size={14} className="sidebar-icon-blue-deep shrink-0" />
              {!collapsed && "Favorites"}
            </button>
            <button
              type="button"
              onClick={() => {
                if (activeSpace) navigate(journalPath);
              }}
              disabled={!activeSpace}
              aria-label="Journal"
              aria-current={getSidebarNavigationAriaCurrent(journalActive)}
              title={collapsed ? "Journal" : undefined}
              className={`rounded-button flex w-full items-center gap-2 border-l-2 px-2 ${getSidebarNavigationTargetClass(mobileSettingsMenu)} text-sm transition-colors duration-100 disabled:opacity-40 ${collapsed ? "justify-center" : ""} ${getSidebarNavigationItemStateClass(journalActive)}`}
            >
              <CalendarDays size={14} className="sidebar-icon-blue-soft" />
              {!collapsed && "Journal"}
            </button>
          </div>
          <div role="group" aria-label="Workspace tools" className="space-y-0.5">
            <button
              type="button"
              onClick={() => {
                if (activeSpace) navigate(workflowPath);
              }}
              disabled={!activeSpace}
              aria-label="Workflows"
              aria-current={getSidebarNavigationAriaCurrent(workflowActive)}
              title={collapsed ? "Workflows" : undefined}
              className={`rounded-button flex w-full items-center gap-2 border-l-2 px-2 ${getSidebarNavigationTargetClass(mobileSettingsMenu)} text-sm transition-colors duration-100 disabled:opacity-40 ${collapsed ? "justify-center" : ""} ${getSidebarNavigationItemStateClass(workflowActive)}`}
            >
              <ListChecks size={14} className="sidebar-icon-blue-deep" />
              {!collapsed && "Workflows"}
            </button>
            <button
              type="button"
              onClick={() => navigate(getSidebarSpaceSettingsPath(activeSpace?.id))}
              disabled={!canShowSpaceAdmin || !activeSpace}
              aria-label="Space Settings"
              aria-current={getSidebarNavigationAriaCurrent(spaceSettingsActive)}
              title={collapsed ? "Space Settings" : undefined}
              className={`rounded-button flex w-full items-center gap-2 border-l-2 px-2 ${getSidebarNavigationTargetClass(mobileSettingsMenu)} text-sm transition-colors duration-100 disabled:opacity-40 ${collapsed ? "justify-center" : ""} ${getSidebarNavigationItemStateClass(spaceSettingsActive)}`}
            >
              <Settings size={14} className="sidebar-icon-blue-mid shrink-0" />
              {!collapsed && "Space Settings"}
            </button>
            {mobileSettingsMenu &&
              shouldShowSidebarSiteAdministration(
                localMode,
                user?.role,
                user?.account_kind,
              ) && (
                <button
                  type="button"
                  onClick={() => navigate(SIDEBAR_SITE_ADMINISTRATION_PATH)}
                  aria-label={localMode ? "Storage Administration" : "Site Administration"}
                  aria-current={getSidebarNavigationAriaCurrent(
                    isSidebarRouteActive(location.pathname, SIDEBAR_SITE_ADMINISTRATION_PATH),
                  )}
                  className={`rounded-button flex w-full items-center gap-2 border-l-2 px-2 ${getSidebarNavigationTargetClass(mobileSettingsMenu)} text-sm transition-colors duration-100 ${getSidebarNavigationItemStateClass(
                    isSidebarRouteActive(location.pathname, SIDEBAR_SITE_ADMINISTRATION_PATH),
                  )}`}
                >
                  <Shield size={14} className="sidebar-icon-blue-mid shrink-0" />
                  {localMode ? "Storage Administration" : "Site Administration"}
                </button>
              )}
          </div>
        </div>

        {templatePickerOpen &&
          renderMobileOverlay(
            <TemplatePickerDialog
              templates={templates}
              schemas={templateSchemas}
              destination={templatePickerDestination}
              initialTitle={templateDocumentTitle}
              initialTemplateId={templatePickerInitialTemplateId}
              renderContext={{
                date: templatePickerDate,
                space: activeSpace?.name,
                author: user?.display_name,
                title: templateDocumentTitle,
              }}
              loading={templatePickerLoading}
              loadError={templatePickerError}
              onRetry={() => {
                if (!activeSpace) return;
                setTemplatePickerLoading(true);
                setTemplatePickerError("");
                void Promise.all([
                  dataClient.listTemplates(activeSpace.id),
                  dataClient.listPropertySchemas(activeSpace.id),
                ])
                  .then(([items, schemas]) => {
                    setTemplates(items);
                    setTemplateSchemas(schemas);
                  })
                  .catch(() => setTemplatePickerError("Templates could not be loaded."))
                  .finally(() => setTemplatePickerLoading(false));
              }}
              allowBlank={templatePickerAllowsBlank}
              onClose={closeTemplatePicker}
              onSelect={handleSelectTemplate}
            />,
          )}

        {collapsed ? (
          <div className={SIDEBAR_COMPACT_FILES_REVEAL_CLASS}>
            <button
              type="button"
              onClick={onShowFiles}
              disabled={!onShowFiles}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex w-full items-center justify-center px-2 py-1.5 text-sm transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-40"
              aria-label="Show pages and folders"
              title="Show pages and folders"
            >
              <Folder size={14} className="sidebar-icon-blue shrink-0" />
            </button>
          </div>
        ) : (
          <div
            onDragOver={handleRootFolderDragOver}
            onDrop={handleRootFolderDrop}
            className={`pt-2 transition-colors duration-100 ${
              canDropFolderOnRoot ? "bg-accent/10" : ""
            }`}
          >
            <div className="mb-1 flex items-center justify-between gap-2 px-2">
              <span
                id="sidebar-files-heading"
                tabIndex={-1}
                className="text-text-muted flex min-w-0 items-center gap-1.5 text-xs font-medium"
              >
                <Folder size={14} className="sidebar-icon-blue" />
                Pages
              </span>
              <span className="flex shrink-0 items-center gap-1">
                <button
                  type="button"
                  onClick={() => handleCreateDocument()}
                  disabled={!activeSpace}
                  className={getSidebarFilesCreateActionClass(mobileSettingsMenu)}
                  aria-label="New page"
                  title="New page"
                >
                  <Plus size={14} className="sidebar-icon-blue-deep" />
                </button>
                <button
                  type="button"
                  onClick={() => startCreateFolder()}
                  disabled={!activeSpace}
                  className={getSidebarFilesCreateActionClass(mobileSettingsMenu)}
                  aria-label="New folder"
                  title="New folder"
                >
                  <FolderPlus size={14} className="sidebar-icon-blue-mid" />
                </button>
              </span>
            </div>
            <div role="tree" aria-label="Pages and folders" onKeyDown={handleFilesTreeKeyDown}>
              {/* Folders */}
              {rootFolders.map((folder) => (
                <FolderNode
                  key={folder.id}
                  folder={folder}
                  allFolders={folders}
                  folderCatalog={folderCatalog}
                  folderDocuments={folderDocuments}
                  folderDropTargets={folderDropTargets}
                  expandedFolderSet={expandedFolderSet}
                  documents={documents}
                  pageCatalog={pageCatalog}
                  activePageAncestors={activePageAncestors}
                  spaceId={activeSpace?.id || ""}
                  navigate={navigate}
                  onCreateDocument={handleCreateDocument}
                  onCreateFolder={startCreateFolder}
                  onMoveFolder={openMoveFolder}
                  draggingFolderId={draggingFolderId}
                  onFolderDragStart={setDraggingFolderId}
                  onFolderDragEnd={() => setDraggingFolderId(null)}
                  onDropFolder={handleDropFolderOnParent}
                  onRenameFolder={startRenameFolder}
                  onDeleteFolder={handleDeleteFolder}
                  onDeleteDocument={handleDeleteDocument}
                  onArchiveDocument={handleArchiveDocument}
                  onDuplicateDocument={handleDuplicateDocument}
                  onRenameDocument={startRenameDocument}
                  onMoveDocument={openMoveDocument}
                  currentPath={location.pathname}
                  touchActions={mobileSettingsMenu}
                  expandedFolderIds={expandedFolderIds}
                  onToggleFolder={toggleExpandedFolder}
                  onExpandFolder={expandFolder}
                  depth={1}
                  currentTreeItemId={currentTreeItemId}
                  focusedTreeItemId={focusedTreeItemId}
                  fallbackTreeItemId={fallbackTreeItemId}
                  onTreeItemFocus={setFocusedTreeItemId}
                />
              ))}

              {/* Root documents (no folder) */}
              {rootDocuments.map((doc) => (
                <DocRow
                  key={doc.id}
                  doc={doc}
                  spaceId={activeSpace?.id || ""}
                  navigate={navigate}
                  onRename={startRenameDocument}
                  onMove={openMoveDocument}
                  onDelete={handleDeleteDocument}
                  onArchive={handleArchiveDocument}
                  onDuplicate={handleDuplicateDocument}
                  onCreateChild={(parentId) => handleCreateDocument(undefined, parentId)}
                  currentPath={location.pathname}
                  touchActions={mobileSettingsMenu}
                  documents={documents}
                  pageCatalog={pageCatalog}
                  activePageAncestors={activePageAncestors}
                  depth={1}
                  currentTreeItemId={currentTreeItemId}
                  focusedTreeItemId={focusedTreeItemId}
                  fallbackTreeItemId={fallbackTreeItemId}
                  onTreeItemFocus={setFocusedTreeItemId}
                />
              ))}
            </div>
          </div>
        )}
      </nav>

      {mobileSettingsMenu && workspaceFooter}

      {createDialog &&
        renderMobileOverlay(
          <DialogShell labelledBy="create-item-title" maxWidth="sm" onClose={cancelCreateDialog}>
            <form
              className="p-3"
              onSubmit={(event) => {
                event.preventDefault();
                void submitCreateDialog();
              }}
            >
              <div className="mb-3 flex items-center justify-between gap-3">
                <div>
                  <h2 id="create-item-title" className="text-text text-sm font-medium">
                    New {createDialog.kind === "document" ? "page" : "folder"}
                  </h2>
                  <p className="text-text-muted mt-0.5 text-xs">Choose a title and destination.</p>
                </div>
                <button
                  type="button"
                  onClick={cancelCreateDialog}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
                  aria-label="Close"
                >
                  <X size={14} />
                </button>
              </div>
              <div className="space-y-3">
                <div>
                  <label
                    htmlFor="create-item-title-input"
                    className="text-text-muted mb-1 block text-xs"
                  >
                    Title
                  </label>
                  <Input
                    ref={createTitleInputRef}
                    id="create-item-title-input"
                    value={createDialog.title}
                    onChange={(e) =>
                      setCreateDialog((current) =>
                        current ? { ...current, title: e.target.value } : current,
                      )
                    }
                    onKeyDown={(e) => {
                      if (e.key === "Escape") cancelCreateDialog();
                    }}
                    placeholder={createDialog.kind === "document" ? "Page title" : "Folder name"}
                    aria-label={createDialog.kind === "document" ? "Page title" : "Folder name"}
                    className="text-sm"
                  />
                </div>
                <div>
                  <label
                    htmlFor="create-item-location-input"
                    className="text-text-muted mb-1 block text-xs"
                  >
                    Location
                  </label>
                  <SlashPathPicker
                    id="create-item-location-input"
                    value={createDialog.query}
                    options={createLocationOptions}
                    selectedOptionId={selectedCreateLocation?.id}
                    onInputChange={(query) =>
                      setCreateDialog((current) => (current ? { ...current, query } : current))
                    }
                    onSelect={(option) => {
                      setCreateDialog((current) =>
                        current
                          ? {
                              ...current,
                              folderId: option.folderId,
                              parentId: option.parentId,
                              query: option.label,
                            }
                          : current,
                      );
                    }}
                    onEscape={cancelCreateDialog}
                    placeholder={
                      createDialog.kind === "document"
                        ? `${activeSpace?.name ?? "Space"} / Folder / Page`
                        : `${activeSpace?.name ?? "Space"} / Folder`
                    }
                    ariaLabel="Search destination"
                    emptyMessage="No matching locations."
                  />
                </div>
              </div>
              {createDialogError && <p role="alert" className="text-danger mt-3 text-sm">{createDialogError}</p>}
              <div className="mt-3 flex justify-end gap-2">
                <button
                  type="button"
                  onClick={cancelCreateDialog}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text px-3 py-1.5 text-sm transition-colors duration-100"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={!createDialog.title.trim() || createDialogPending}
                  className="rounded-button bg-primary px-3 py-1.5 text-sm font-medium text-white transition-opacity duration-100 disabled:opacity-40"
                >
                  {createDialogPending ? "Creating…" : "Create"}
                </button>
              </div>
            </form>
          </DialogShell>,
        )}

      {renameTarget &&
        renderMobileOverlay(
          <DialogShell labelledBy="rename-item-title" maxWidth="sm" onClose={cancelRename}>
            <form
              className="p-3"
              onSubmit={(event) => {
                event.preventDefault();
                void submitRename();
              }}
            >
              <div className="mb-3 flex items-center justify-between gap-3">
                <div>
                  <h2 id="rename-item-title" className="text-text text-sm font-medium">
                    Rename {renameTarget.type}
                  </h2>
                  <p className="text-text-muted mt-0.5 text-xs">Update the visible name.</p>
                </div>
                <button
                  type="button"
                  onClick={cancelRename}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
                  aria-label="Close"
                >
                  <X size={14} />
                </button>
              </div>
              <div className="space-y-3">
                {renameError && (
                  <p className="text-danger text-sm" role="alert">
                    {renameError} Open the page to review its selected draft or retained request.
                  </p>
                )}
                <Input
                  ref={renameInputRef}
                  value={renameName}
                  onChange={(e) => setRenameName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Escape") cancelRename();
                  }}
                  placeholder="Name"
                  aria-label={`${renameTarget.type} name`}
                  className="text-sm"
                />
                <div>
                  <label
                    htmlFor="sidebar-item-icon-input"
                    className="text-text-muted mb-1 block text-xs"
                  >
                    {getSidebarRenameEmojiLabel(renameTarget.type)}
                  </label>
                  <Input
                    id="sidebar-item-icon-input"
                    value={renameIcon}
                    onChange={(e) => setRenameIcon(e.target.value)}
                    placeholder="Optional emoji, for example 📁"
                    aria-label={getSidebarRenameEmojiLabel(renameTarget.type)}
                    autoComplete="off"
                    className="text-sm"
                  />
                  <div
                    role="group"
                    aria-label={`Choose ${getSidebarRenameEmojiLabel(renameTarget.type).toLowerCase()}`}
                    className="mt-2 grid grid-cols-5 gap-1"
                  >
                    {getSidebarRenameEmojiPresets(renameTarget.type).map((emoji) => (
                      <button
                        key={emoji}
                        type="button"
                        onClick={() => setRenameIcon(emoji)}
                        aria-label={`Use ${emoji} as the ${getSidebarRenameEmojiLabel(renameTarget.type).toLowerCase()}`}
                        aria-pressed={renameIcon === emoji}
                        className={`rounded-button flex h-11 items-center justify-center border text-lg transition-colors duration-100 ${
                          renameIcon === emoji
                            ? "border-primary bg-primary/10 text-text"
                            : "border-border text-text-muted hover:bg-surface-raised hover:text-text"
                        }`}
                      >
                        {emoji}
                      </button>
                    ))}
                    <button
                      type="button"
                      onClick={() => setRenameIcon("")}
                      aria-label={`Clear ${getSidebarRenameEmojiLabel(renameTarget.type).toLowerCase()}`}
                      className="rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text flex h-11 items-center justify-center border transition-colors duration-100"
                    >
                      <X size={16} aria-hidden="true" />
                    </button>
                  </div>
                </div>
              </div>
              <div className="mt-3 flex justify-end gap-2">
                <button
                  type="button"
                  onClick={cancelRename}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text px-3 py-1.5 text-sm transition-colors duration-100"
                >
                  Cancel
                </button>
                <button
                  type="submit"
                  disabled={!renameName.trim()}
                  className="rounded-button bg-primary px-3 py-1.5 text-sm font-medium text-white transition-opacity duration-100 disabled:opacity-40"
                >
                  Save
                </button>
              </div>
            </form>
          </DialogShell>,
        )}

      {deleteTarget &&
        renderMobileOverlay(
          <ConfirmDialog
            labelledBy="delete-sidebar-item-title"
            title={getSidebarDeleteConfirmationTitle(deleteTarget.type)}
            message={getSidebarDeleteConfirmationMessage(deleteTarget.type, deleteTarget.name)}
            confirmLabel="Delete"
            destructive
            onCancel={() => setDeleteTarget(null)}
            onConfirm={() => void submitDeleteTarget()}
          />,
        )}

      {localReviewedMove &&
        renderMobileOverlay(
          <LocalReviewedMoveDialog
            session={localReviewedMove.session}
            title={localReviewedMove.title}
            destinationName={localReviewedMove.destinationName}
            onClose={() => {
              localReviewIdentity.current = null;
              setLocalReviewedMove(null);
              setMoveTargetSpaceId("");
              setMoveLocationQuery("");
            }}
            onCommitted={async (result) => {
              const original = localReviewedMove;
              const current = () =>
                moveMountedRef.current &&
                localReviewIdentity.current === original.identity &&
                activeSpaceIdRef.current === original.source;
              await completeLocalSidebarMove(
                original.session,
                current,
                result.newer_local_changes_retained,
                async (verify) => {
                  await refreshSpaceContents(original.source, verify);
                },
                (retainCanvas) => {
                  emitGraphDirty(original.source, "document-metadata-updated");
                  emitGraphDirty(original.intent.destination.space_id, "document-metadata-updated");
                  if (!retainCanvas) navigate(getSidebarHomePath(original.source));
                },
              );
            }}
          />,
        )}

      {reviewedMove &&
        renderMobileOverlay(
          <ReviewedMoveDialog
            context={reviewedMove.context}
            sourceSpaceId={reviewedMove.source}
            intent={reviewedMove.intent}
            title={reviewedMove.title}
            destinationName={reviewedMove.destinationName}
            onClose={() => {
              setReviewedMove(null);
              setMoveTargetSpaceId("");
              setMoveLocationQuery("");
            }}
            onCommitted={async () => {
              const original = reviewedMove;
              const current = () =>
                reviewedMoveRef.current === original &&
                activeSpaceIdRef.current === original.source;
              await completeReviewedMove(
                original.context,
                current,
                async () => {
                  await refreshSpaceContents(original.source, original.context.verify);
                  original.context.verify();
                  if (!current())
                    throw new Error("Move selection changed; reload the destination.");
                  await Promise.all([
                    dataClient.listFolders(original.intent.destination.space_id),
                    dataClient.listDocuments(original.intent.destination.space_id, {
                      include_nested: true,
                    }),
                  ]);
                },
                () => {
                  emitGraphDirty(original.source, "document-metadata-updated");
                  emitGraphDirty(original.intent.destination.space_id, "document-metadata-updated");
                  navigate(getSidebarHomePath(original.source));
                },
              );
            }}
          />,
        )}

      {movingFolder &&
        renderMobileOverlay(
          <DialogShell
            labelledBy="move-folder-title"
            maxWidth="sm"
            onClose={() => {
              setMovingFolderId(null);
              setMoveTargetSpaceId("");
              setMoveLocationQuery("");
            }}
          >
            <div className="p-3">
              <div className="mb-3 flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <h2 id="move-folder-title" className="text-text text-sm font-medium">
                    Move folder
                  </h2>
                  <p className="text-text-muted mt-0.5 truncate text-xs">{movingFolder.name}</p>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    setMovingFolderId(null);
                    setMoveTargetSpaceId("");
                    setMoveLocationQuery("");
                  }}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
                  aria-label="Close"
                >
                  <X size={14} />
                </button>
              </div>
              {moveReviewError && (
                <p role="alert" className="text-danger mb-3 text-sm">
                  {moveReviewError}
                </p>
              )}
              <label className="text-text-muted mb-1 block text-xs" htmlFor="move-folder-space">
                Space
              </label>
              <Select
                id="move-folder-space"
                value={moveTargetSpaceId}
                onChange={(event) => {
                  setMoveTargetSpaceId(event.target.value);
                  setMoveLocationQuery("");
                }}
                className="mb-3 text-sm"
              >
                {spaces.map((space) => (
                  <option key={space.id} value={space.id}>
                    {space.name}
                  </option>
                ))}
              </Select>
              <SlashPathPicker
                id="move-folder-location-input"
                value={moveLocationQuery}
                options={moveFolderLocationOptions}
                onInputChange={setMoveLocationQuery}
                onSelect={(target) => void handleMoveFolder(movingFolder.id, target.folderId)}
                onEscape={() => {
                  setMovingFolderId(null);
                  setMoveTargetSpaceId("");
                  setMoveLocationQuery("");
                }}
                placeholder={`${selectedMoveSpace?.name ?? "Space"} / Folder`}
                ariaLabel="Search move destination"
                emptyMessage="No valid destinations."
                className="mb-2"
              />
            </div>
          </DialogShell>,
        )}

      {movingDocument &&
        selectedMoveSpace &&
        renderMobileOverlay(
          <DialogShell
            labelledBy="move-document-title"
            maxWidth="sm"
            onClose={() => {
              setMovingDocumentId(null);
              setMoveTargetSpaceId("");
              setMoveLocationQuery("");
            }}
          >
            <div className="p-3">
              <div className="mb-3 flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <h2 id="move-document-title" className="text-text text-sm font-medium">
                    Move page
                  </h2>
                  <p className="text-text-muted mt-0.5 truncate text-xs">{movingDocument.title}</p>
                </div>
                <button
                  type="button"
                  onClick={() => {
                    setMovingDocumentId(null);
                    setMoveTargetSpaceId("");
                    setMoveLocationQuery("");
                  }}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
                  aria-label="Close"
                >
                  <X size={14} />
                </button>
              </div>
              {moveReviewError && (
                <p role="alert" className="text-danger mb-3 text-sm">
                  {moveReviewError}
                </p>
              )}
              <label className="text-text-muted mb-1 block text-xs" htmlFor="move-document-space">
                Space
              </label>
              <Select
                id="move-document-space"
                value={moveTargetSpaceId}
                onChange={(event) => {
                  setMoveTargetSpaceId(event.target.value);
                  setMoveLocationQuery("");
                }}
                className="mb-3 text-sm"
              >
                {spaces.map((space) => (
                  <option key={space.id} value={space.id}>
                    {space.name}
                  </option>
                ))}
              </Select>
              <SlashPathPicker
                id="move-document-location-input"
                value={moveLocationQuery}
                options={moveDocumentLocationOptions}
                onInputChange={setMoveLocationQuery}
                onSelect={(target) =>
                  void handleMoveDocument(
                    movingDocument.id,
                    selectedMoveSpace.id,
                    target.folderId,
                    target.parentId,
                  )
                }
                onEscape={() => {
                  setMovingDocumentId(null);
                  setMoveTargetSpaceId("");
                  setMoveLocationQuery("");
                }}
                placeholder={`${selectedMoveSpace.name} / Folder / Page`}
                ariaLabel="Search move destination"
                emptyMessage="No valid destinations."
                className="mb-2"
              />
            </div>
          </DialogShell>,
        )}

      <div className="shrink-0 pb-[calc(env(safe-area-inset-bottom)+0.75rem)]">
        {createSpaceOpen &&
          canCreateSpace &&
          renderMobileOverlay(
            <DialogShell labelledBy="new-space-title" maxWidth="sm" onClose={cancelCreateSpace}>
              <form
                className="p-3"
                onSubmit={(event) => {
                  event.preventDefault();
                  void handleCreateSpace();
                }}
              >
                <div className="mb-3 flex items-center justify-between gap-3">
                  <div>
                    <h2 id="new-space-title" className="text-text text-sm font-medium">
                      New space
                    </h2>
                    <p className="text-text-muted mt-0.5 text-xs">
                      Create a space and switch to it.
                    </p>
                  </div>
                  <button
                    type="button"
                    onClick={cancelCreateSpace}
                    className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
                    aria-label="Close"
                  >
                    <X size={14} />
                  </button>
                </div>
                <label htmlFor="new-space-name" className="sr-only">
                  Space name
                </label>
                <Input
                  id="new-space-name"
                  ref={spaceInputRef}
                  value={newSpaceName}
                  onChange={(e) => setNewSpaceName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Escape") cancelCreateSpace();
                  }}
                  placeholder="Space name"
                  className="px-2 text-sm"
                />
                <div className="mt-3 flex justify-end gap-2">
                  <button
                    type="button"
                    onClick={cancelCreateSpace}
                    className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text px-3 py-1.5 text-sm transition-colors duration-100"
                  >
                    Cancel
                  </button>
                  <button
                    type="submit"
                    disabled={!newSpaceName.trim()}
                    className="rounded-button bg-primary px-3 py-1.5 text-sm font-medium text-white transition-opacity duration-100 disabled:opacity-40"
                  >
                    Create
                  </button>
                </div>
              </form>
            </DialogShell>,
          )}
      </div>
    </div>
  );
}

function DocRow({
  doc,
  spaceId,
  navigate,
  onDelete,
  onArchive,
  onDuplicate,
  onRename,
  onMove,
  onCreateChild,
  currentPath: _currentPath,
  touchActions = false,
  documents,
  pageCatalog,
  activePageAncestors,
  depth,
  currentTreeItemId,
  focusedTreeItemId,
  fallbackTreeItemId,
  onTreeItemFocus,
}: {
  doc: DocItem;
  spaceId: string;
  navigate: (path: string) => void;
  onDelete: (id: string) => void;
  onArchive: (id: string) => void;
  onDuplicate: (id: string) => void;
  onRename: (id: string, currentTitle: string, currentIcon?: string) => void;
  onMove: (id: string) => void;
  onCreateChild: (parentId: string) => void;
  currentPath: string;
  touchActions?: boolean;
  documents: DocItem[];
  pageCatalog: SidebarPageCatalog;
  activePageAncestors: Set<string>;
  depth: number;
  currentTreeItemId: string | null;
  focusedTreeItemId: string | null;
  fallbackTreeItemId: string | null;
  onTreeItemFocus: (itemId: string) => void;
}) {
  const treeItemId = `document:${doc.id}`;
  const active = currentTreeItemId === treeItemId;
  const [menuOpen, setMenuOpen] = useState(false);
  const [expanded, setExpanded] = useState(() => activePageAncestors.has(doc.id));
  useEffect(() => {
    if (activePageAncestors.has(doc.id)) setExpanded(true);
  }, [activePageAncestors, doc.id]);
  const childDocuments = pageCatalog.children.get(doc.id) ?? [];
  const hasChildren = childDocuments.length > 0;
  return (
    <div>
      <div
        className={`rounded-button group relative flex items-center transition-colors duration-100 ${
          active
            ? "bg-surface-raised text-text"
            : "text-text-muted hover:bg-surface-raised hover:text-text"
        }`}
      >
        {hasChildren ? (
          <button
            type="button"
            onClick={() => setExpanded((current) => !current)}
            className={`text-text-muted hover:text-text rounded-button ${getSidebarTreeToggleClass(touchActions)} transition-colors duration-100`}
            aria-label={`${expanded ? "Collapse" : "Expand"} child pages for ${doc.title}`}
            aria-expanded={expanded}
          >
            <ChevronRight
              size={12}
              className={`shrink-0 transition-transform duration-100 ${expanded ? "rotate-90" : ""}`}
            />
          </button>
        ) : (
          <span className={getSidebarTreeToggleClass(touchActions)} aria-hidden="true" />
        )}
        <button
          type="button"
          onClick={() => navigate(getSidebarDocumentPath(spaceId, doc.id))}
          onKeyDown={(event) => {
            if (event.key === "ArrowRight" && hasChildren && !expanded) {
              event.preventDefault();
              setExpanded(true);
            } else if (event.key === "ArrowLeft" && hasChildren && expanded) {
              event.preventDefault();
              setExpanded(false);
            }
          }}
          role="treeitem"
          aria-level={depth}
          aria-expanded={hasChildren ? expanded : undefined}
          aria-current={active ? "page" : undefined}
          aria-selected={active}
          tabIndex={getSidebarTreeItemTabIndex(
            focusedTreeItemId,
            treeItemId,
            currentTreeItemId,
            fallbackTreeItemId,
          )}
          onFocus={() => onTreeItemFocus(treeItemId)}
          className={`rounded-button flex min-w-0 flex-1 items-center gap-2 ${touchActions ? "px-2" : "px-1"} ${getSidebarTreeRowClass(touchActions)} text-sm transition-colors duration-100`}
        >
          {doc.icon ? (
            <span className="flex h-4 w-4 shrink-0 items-center justify-center text-sm leading-none">
              {doc.icon}
            </span>
          ) : (
            <FileText size={SIDEBAR_TREE_ICON_SIZE} className="sidebar-icon-blue-deep shrink-0" />
          )}
          <span className="truncate">{doc.title}</span>
        </button>
        <div className="absolute right-1 flex items-center gap-0.5">
          <RowActionMenu
            open={menuOpen}
            onOpenChange={setMenuOpen}
            label={`Page actions for ${doc.title}`}
            touchActions={touchActions}
            trigger={
              <button
                type="button"
                onClick={(event) => event.stopPropagation()}
                className={`text-text-muted hover:bg-surface hover:text-text rounded-button flex ${getSidebarTreeActionClass(touchActions)} items-center justify-center transition-colors duration-100 ${
                  touchActions
                    ? "pointer-events-auto opacity-100"
                    : "pointer-events-none opacity-0 group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100 focus:pointer-events-auto focus:opacity-100"
                }`}
                aria-label="Page actions"
              >
                <MoreHorizontal size={13} />
              </button>
            }
            items={[
              ...((pageCatalog.depths.get(doc.id) ?? PAGE_OPERATION_NODE_LIMIT) <
              PAGE_OPERATION_NODE_LIMIT
                ? [
                    {
                      label: "New child page",
                      icon: <Plus size={13} />,
                      onSelect: () => onCreateChild(doc.id),
                    },
                  ]
                : []),
              {
                label: "Rename",
                icon: <Pencil size={13} />,
                onSelect: () => onRename(doc.id, doc.title, doc.icon),
              },
              {
                label: "Move...",
                icon: <MoveRight size={13} />,
                onSelect: () => onMove(doc.id),
              },
              {
                label: "Duplicate",
                icon: <Copy size={13} />,
                onSelect: () => void onDuplicate(doc.id),
              },
              {
                label: "Archive",
                icon: <Archive size={13} />,
                onSelect: () => void onArchive(doc.id),
              },
              {
                label: "Delete",
                icon: <Trash2 size={13} />,
                danger: true,
                onSelect: () => onDelete(doc.id),
              },
            ]}
          />
        </div>
      </div>
      {expanded && childDocuments.length > 0 && (
        <div role="group" className={depth < 6 ? SIDEBAR_TREE_NESTED_GROUP_CLASS : ""}>
          {childDocuments.map((child) => (
            <DocRow
              key={child.id}
              doc={child}
              spaceId={spaceId}
              navigate={navigate}
              onDelete={onDelete}
              onArchive={onArchive}
              onDuplicate={onDuplicate}
              onRename={onRename}
              onMove={onMove}
              onCreateChild={onCreateChild}
              currentPath={_currentPath}
              touchActions={touchActions}
              documents={documents}
              pageCatalog={pageCatalog}
              activePageAncestors={activePageAncestors}
              depth={depth + 1}
              currentTreeItemId={currentTreeItemId}
              focusedTreeItemId={focusedTreeItemId}
              fallbackTreeItemId={fallbackTreeItemId}
              onTreeItemFocus={onTreeItemFocus}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function RowActionMenu({
  open,
  onOpenChange,
  label,
  items,
  trigger,
  touchActions = false,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  label: string;
  items: Array<{
    label: string;
    icon: ReactNode;
    danger?: boolean;
    onSelect: () => void;
  }>;
  trigger: ReactNode;
  touchActions?: boolean;
}) {
  const menuRef = useRef<HTMLDivElement | null>(null);
  const handleMenuKeyDown = useMenuKeyboardNavigation(menuRef);

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>{trigger}</PopoverTrigger>
      <PopoverContent
        align="end"
        side="bottom"
        sideOffset={4}
        className={`w-40 p-1 ${touchActions ? "mt-1" : ""}`}
        ref={menuRef}
        role="menu"
        aria-label={label}
        tabIndex={-1}
        onKeyDown={handleMenuKeyDown}
      >
        {items.map((item) => (
          <button
            key={item.label}
            type="button"
            role="menuitem"
            onClick={() => {
              onOpenChange(false);
              item.onSelect();
            }}
            className={`rounded-button flex w-full items-center gap-2 px-2 ${getSidebarNavigationTargetClass(touchActions)} text-left text-sm transition-colors duration-100 ${
              item.danger
                ? SIDEBAR_DANGER_MENU_ITEM_CLASS
                : "text-text-muted hover:bg-surface-raised hover:text-text"
            }`}
          >
            {item.icon}
            {item.label}
          </button>
        ))}
      </PopoverContent>
    </Popover>
  );
}

function FolderNode({
  folder,
  allFolders,
  folderCatalog,
  folderDocuments,
  folderDropTargets,
  expandedFolderSet,
  documents,
  pageCatalog,
  activePageAncestors,
  spaceId,
  navigate,
  onCreateDocument,
  onCreateFolder,
  onMoveFolder,
  draggingFolderId,
  onFolderDragStart,
  onFolderDragEnd,
  onDropFolder,
  onRenameFolder,
  onDeleteFolder,
  onDeleteDocument,
  onArchiveDocument,
  onDuplicateDocument,
  onRenameDocument,
  onMoveDocument,
  currentPath,
  touchActions = false,
  expandedFolderIds,
  onToggleFolder,
  onExpandFolder,
  depth,
  currentTreeItemId,
  focusedTreeItemId,
  fallbackTreeItemId,
  onTreeItemFocus,
}: {
  folder: FolderItem;
  allFolders: FolderItem[];
  folderCatalog: SidebarFolderCatalog;
  folderDocuments: Map<string, DocItem[]>;
  folderDropTargets: Set<string>;
  expandedFolderSet: Set<string>;
  documents: DocItem[];
  pageCatalog: SidebarPageCatalog;
  activePageAncestors: Set<string>;
  spaceId: string;
  navigate: (path: string) => void;
  onCreateDocument: (folderId?: string, parentId?: string) => void;
  onCreateFolder: (parentId: string) => void;
  onMoveFolder: (folderId: string) => void;
  draggingFolderId: string | null;
  onFolderDragStart: (folderId: string) => void;
  onFolderDragEnd: () => void;
  onDropFolder: (folderId: string, parentId: string) => void;
  onRenameFolder: (id: string, currentName: string, currentIcon?: string) => void;
  onDeleteFolder: (id: string) => void;
  onDeleteDocument: (id: string) => void;
  onArchiveDocument: (id: string) => void;
  onDuplicateDocument: (id: string) => void;
  onRenameDocument: (id: string, currentTitle: string, currentIcon?: string) => void;
  onMoveDocument: (id: string) => void;
  currentPath: string;
  touchActions?: boolean;
  expandedFolderIds: string[];
  onToggleFolder: (folderId: string) => void;
  onExpandFolder: (folderId: string) => void;
  depth: number;
  currentTreeItemId: string | null;
  focusedTreeItemId: string | null;
  fallbackTreeItemId: string | null;
  onTreeItemFocus: (itemId: string) => void;
}) {
  const treeItemId = `folder:${folder.id}`;
  const expanded = expandedFolderSet.has(folder.id);
  const [menuOpen, setMenuOpen] = useState(false);
  const folderDepth = folderCatalog.depths.get(folder.id) ?? 0;
  const childFolders = folderCatalog.children.get(folder.id) ?? [];
  const folderDocs = folderDocuments.get(folder.id) ?? [];
  const canCreateChildFolder = folderDepth > 0 && folderDepth < FOLDER_OPERATION_NODE_LIMIT;
  const localMode = getCurrentLaunchMode() === "local";
  // Opening Move is useful across spaces; the dialog owns exact destination admission.
  const canMoveFolder = folderDepth > 0;
  const canAcceptDraggedFolder = localMode && folderDropTargets.has(folder.id);

  const handleFolderDragStart = (event: DragEvent<HTMLButtonElement>) => {
    if (!localMode) return;
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", folder.id);
    onFolderDragStart(folder.id);
  };

  const handleFolderDragOver = (event: DragEvent<HTMLButtonElement>) => {
    if (!canAcceptDraggedFolder) return;
    event.preventDefault();
    event.stopPropagation();
    event.dataTransfer.dropEffect = "move";
  };

  const handleFolderDrop = (event: DragEvent<HTMLButtonElement>) => {
    if (!canAcceptDraggedFolder || !draggingFolderId) return;
    event.preventDefault();
    event.stopPropagation();
    onDropFolder(draggingFolderId, folder.id);
    onFolderDragEnd();
  };

  return (
    <div>
      <div
        className={`rounded-button group relative flex items-center transition-colors duration-100 ${
          canAcceptDraggedFolder
            ? "bg-accent/10 text-text ring-accent/50 ring-1"
            : "text-text-muted hover:bg-surface-raised hover:text-text"
        }`}
      >
        <button
          type="button"
          onClick={() => onToggleFolder(folder.id)}
          className={`text-text-muted hover:text-text rounded-button ${getSidebarTreeToggleClass(touchActions)} transition-colors duration-100`}
          aria-label={`${expanded ? "Collapse" : "Expand"} folder ${folder.name}`}
          aria-expanded={expanded}
        >
          <ChevronRight
            size={12}
            className={`shrink-0 transition-transform duration-100 ${expanded ? "rotate-90" : ""}`}
          />
        </button>
        <button
          type="button"
          draggable={localMode}
          onDragStart={handleFolderDragStart}
          onDragEnd={onFolderDragEnd}
          onDragOver={handleFolderDragOver}
          onDrop={handleFolderDrop}
          onClick={() => onToggleFolder(folder.id)}
          onKeyDown={(event) => {
            if (event.key === "ArrowRight" && !expanded) {
              event.preventDefault();
              onExpandFolder(folder.id);
            } else if (event.key === "ArrowLeft" && expanded) {
              event.preventDefault();
              onToggleFolder(folder.id);
            }
          }}
          role="treeitem"
          aria-level={depth}
          tabIndex={getSidebarTreeItemTabIndex(
            focusedTreeItemId,
            treeItemId,
            currentTreeItemId,
            fallbackTreeItemId,
          )}
          onFocus={() => onTreeItemFocus(treeItemId)}
          className={`rounded-button flex min-w-0 flex-1 items-center gap-2 ${touchActions ? "px-2" : "px-1"} ${getSidebarTreeRowClass(touchActions)} text-sm transition-colors duration-100`}
          aria-expanded={expanded}
        >
          <ContentIdentityIcon
            value={folder.icon}
            size={SIDEBAR_TREE_ICON_SIZE}
            fallback={
              <Folder size={SIDEBAR_TREE_ICON_SIZE} className="sidebar-icon-blue-mid shrink-0" />
            }
          />
          <span className="truncate">{folder.name}</span>
        </button>
        <div className="absolute right-1 flex items-center gap-0.5">
          <RowActionMenu
            open={menuOpen}
            onOpenChange={setMenuOpen}
            label={`Folder actions for ${folder.name}`}
            touchActions={touchActions}
            trigger={
              <button
                type="button"
                onClick={(event) => event.stopPropagation()}
                className={`text-text-muted hover:bg-surface hover:text-text rounded-button flex ${getSidebarTreeActionClass(touchActions)} items-center justify-center transition-colors duration-100 ${
                  touchActions
                    ? "pointer-events-auto opacity-100"
                    : "pointer-events-none opacity-0 group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100 focus:pointer-events-auto focus:opacity-100"
                }`}
                aria-label="Folder actions"
              >
                <MoreHorizontal size={13} />
              </button>
            }
            items={[
              {
                label: "New page",
                icon: <Plus size={13} />,
                onSelect: () => {
                  onExpandFolder(folder.id);
                  onCreateDocument(folder.id);
                },
              },
              ...(canCreateChildFolder
                ? [
                    {
                      label: "New folder",
                      icon: <FolderPlus size={13} />,
                      onSelect: () => {
                        onExpandFolder(folder.id);
                        onCreateFolder(folder.id);
                      },
                    },
                  ]
                : []),
              ...(canMoveFolder
                ? [
                    {
                      label: "Move...",
                      icon: <MoveRight size={13} />,
                      onSelect: () => onMoveFolder(folder.id),
                    },
                  ]
                : []),
              {
                label: "Rename",
                icon: <Pencil size={13} />,
                onSelect: () => onRenameFolder(folder.id, folder.name, folder.icon),
              },
              {
                label: "Delete",
                icon: <Trash2 size={13} />,
                danger: true,
                onSelect: () => onDeleteFolder(folder.id),
              },
            ]}
          />
        </div>
      </div>

      {expanded && (
        <div role="group" className={depth < 6 ? SIDEBAR_TREE_NESTED_GROUP_CLASS : ""}>
          {childFolders.map((child) => (
            <FolderNode
              key={child.id}
              folder={child}
              allFolders={allFolders}
              folderCatalog={folderCatalog}
              folderDocuments={folderDocuments}
              folderDropTargets={folderDropTargets}
              expandedFolderSet={expandedFolderSet}
              documents={documents}
              pageCatalog={pageCatalog}
              activePageAncestors={activePageAncestors}
              spaceId={spaceId}
              navigate={navigate}
              onCreateDocument={onCreateDocument}
              onCreateFolder={onCreateFolder}
              onMoveFolder={onMoveFolder}
              draggingFolderId={draggingFolderId}
              onFolderDragStart={onFolderDragStart}
              onFolderDragEnd={onFolderDragEnd}
              onDropFolder={onDropFolder}
              onRenameFolder={onRenameFolder}
              onDeleteFolder={onDeleteFolder}
              onDeleteDocument={onDeleteDocument}
              onArchiveDocument={onArchiveDocument}
              onDuplicateDocument={onDuplicateDocument}
              onRenameDocument={onRenameDocument}
              onMoveDocument={onMoveDocument}
              currentPath={currentPath}
              touchActions={touchActions}
              expandedFolderIds={expandedFolderIds}
              onToggleFolder={onToggleFolder}
              onExpandFolder={onExpandFolder}
              depth={depth + 1}
              currentTreeItemId={currentTreeItemId}
              focusedTreeItemId={focusedTreeItemId}
              fallbackTreeItemId={fallbackTreeItemId}
              onTreeItemFocus={onTreeItemFocus}
            />
          ))}
          {folderDocs.map((doc) => (
            <DocRow
              key={doc.id}
              doc={doc}
              spaceId={spaceId}
              navigate={navigate}
              onRename={onRenameDocument}
              onMove={onMoveDocument}
              onDelete={onDeleteDocument}
              onArchive={onArchiveDocument}
              onDuplicate={onDuplicateDocument}
              onCreateChild={(parentId) => onCreateDocument(folder.id, parentId)}
              currentPath={currentPath}
              touchActions={touchActions}
              documents={documents}
              pageCatalog={pageCatalog}
              activePageAncestors={activePageAncestors}
              depth={depth + 1}
              currentTreeItemId={currentTreeItemId}
              focusedTreeItemId={focusedTreeItemId}
              fallbackTreeItemId={fallbackTreeItemId}
              onTreeItemFocus={onTreeItemFocus}
            />
          ))}
        </div>
      )}
    </div>
  );
}
