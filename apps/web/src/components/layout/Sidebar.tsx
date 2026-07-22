import { useCallback, useEffect, useRef, useState, type DragEvent, type ReactNode } from "react";
import { useNavigate, useLocation, useParams } from "react-router-dom";
import {
  CalendarDays,
  Check,
  ChevronDown,
  ChevronRight,
  Clock3,
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
  Star,
  Trash2,
  UserCircle,
  Workflow,
  X,
} from "lucide-react";
import { dataClient, type TemplateRecord } from "@/lib/dataClient";
import { emitGraphDirty } from "@/lib/graphEvents";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import { buildSpaceHomePath } from "@/lib/lastViewedSpace";
import { getPersonalSpaceOwnerLabel } from "@/lib/spaceLabels";
import {
  loadStarredDocuments,
  readStarredDocuments,
  type StarredDocumentRecord,
} from "@/lib/starredDocuments";
import {
  findDailyTemplate,
  findDailyFolder,
  DAILY_FOLDER_NAME,
  formatDateStamp,
  renderDailyTemplateContent,
  renderTemplateContent,
  templatePropertiesAsRecord,
} from "@/lib/templates";
import { DialogShell } from "@/components/ui/dialog-shell";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Input } from "@/components/ui/input";
import { SearchInput } from "@/components/ui/search-input";
import { Select } from "@/components/ui/select";
import { SlashPathPicker, type SlashPathPickerOption } from "./SlashPathPicker";
import { useAuth } from "@/hooks/useAuth";
import { BrandLogo } from "@/components/BrandLogo";

export const SIDEBAR_DANGER_MENU_ITEM_CLASS = "text-danger hover:bg-danger/10";
export const OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT = "starlane:open-sidebar-move-document";
export const SIDEBAR_PROFILE_SETTINGS_PATH = "/profile";
export const SIDEBAR_SITE_ADMINISTRATION_PATH = "/admin";
export const SIDEBAR_TREE_TOGGLE_LANE_CLASS =
  "flex h-7 w-5 shrink-0 items-center justify-center";
export const SIDEBAR_TREE_ICON_SIZE = 14;

export function shouldShowSidebarSiteAdministration(
  localMode: boolean,
  role: string | undefined,
): boolean {
  return !localMode && role === "admin";
}

export function getSidebarDeleteConfirmationTitle(type: "document" | "folder"): string {
  return type === "document" ? "Delete page" : "Delete folder";
}

export function getSidebarDeleteConfirmationMessage(
  type: "document" | "folder",
  name: string,
): string {
  const trimmed = name.trim();
  if (type === "document") {
    return trimmed
      ? `Delete page "${trimmed}"? This cannot be undone.`
      : "Delete this page? This cannot be undone.";
  }
  return trimmed
    ? `Delete folder "${trimmed}" and its pages? This cannot be undone.`
    : "Delete this folder and its pages? This cannot be undone.";
}

interface Space {
  id: string;
  name: string;
  slug: string;
  icon?: string;
  accent_color?: string;
  member_role?: string;
  owner_username?: string;
  owner_display_name?: string;
  created_by?: string;
}

interface FolderItem {
  id: string;
  name: string;
  icon?: string;
  parent_id?: string;
}

interface DocItem {
  id: string;
  title: string;
  icon?: string;
  folder_id?: string;
  parent_id?: string;
}

interface SidebarProps {
  showHeader?: boolean;
  mobileSettingsMenu?: boolean;
}

type SidebarCreateKind = "document" | "folder";

interface SidebarCreateDialogState {
  kind: SidebarCreateKind;
  title: string;
  query: string;
  folderId: string | null;
  parentId: string | null;
}

interface SidebarLocationOption extends SlashPathPickerOption {
  folderId: string | null;
  parentId: string | null;
}

export function sortSidebarFolders(folders: FolderItem[]): FolderItem[] {
  return [...folders].sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
  );
}

export function sortSidebarDocuments(documents: DocItem[]): DocItem[] {
  return [...documents].sort((a, b) =>
    a.title.localeCompare(b.title, undefined, { sensitivity: "base" }),
  );
}

export function getSidebarFolderDepth(
  folders: FolderItem[],
  folderId: string,
  seen = new Set<string>(),
): number {
  if (seen.has(folderId)) return 0;
  seen.add(folderId);
  const folder = folders.find((item) => item.id === folderId);
  if (!folder) return 0;
  if (!folder.parent_id) return 1;
  const parentDepth = getSidebarFolderDepth(folders, folder.parent_id, seen);
  return parentDepth > 0 ? parentDepth + 1 : 0;
}

export function canCreateSidebarChildFolder(folders: FolderItem[], folderId: string): boolean {
  const depth = getSidebarFolderDepth(folders, folderId);
  return depth > 0 && depth < 3;
}

export function canMoveSidebarFolderToRoot(
  folder: Pick<FolderItem, "parent_id">,
  localMode: boolean,
): boolean {
  return localMode && Boolean(folder.parent_id);
}

export function getSidebarFolderSubtreeDepth(
  folders: FolderItem[],
  folderId: string,
  seen = new Set<string>(),
): number {
  if (seen.has(folderId)) return 0;
  seen.add(folderId);
  let maxDepth = 1;
  for (const folder of folders) {
    if (folder.parent_id !== folderId) continue;
    maxDepth = Math.max(
      maxDepth,
      getSidebarFolderSubtreeDepth(folders, folder.id, new Set(seen)) + 1,
    );
  }
  return maxDepth;
}

export function isSidebarFolderDescendant(
  folders: FolderItem[],
  ancestorId: string,
  candidateId: string,
  seen = new Set<string>(),
): boolean {
  if (seen.has(candidateId)) return false;
  seen.add(candidateId);
  const candidate = folders.find((folder) => folder.id === candidateId);
  if (!candidate?.parent_id) return false;
  if (candidate.parent_id === ancestorId) return true;
  return isSidebarFolderDescendant(folders, ancestorId, candidate.parent_id, seen);
}

export function canMoveSidebarFolderToParent(
  folders: FolderItem[],
  folderId: string,
  parentId: string,
): boolean {
  const folder = folders.find((item) => item.id === folderId);
  const parent = folders.find((item) => item.id === parentId);
  if (!folder || !parent) return false;
  if (folder.id === parent.id || folder.parent_id === parent.id) return false;
  if (isSidebarFolderDescendant(folders, folder.id, parent.id)) return false;
  const parentDepth = getSidebarFolderDepth(folders, parent.id);
  const subtreeDepth = getSidebarFolderSubtreeDepth(folders, folder.id);
  return parentDepth > 0 && subtreeDepth > 0 && parentDepth + subtreeDepth <= 3;
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
  return sortSidebarFolders(
    folders.filter((folder) => canMoveSidebarFolderToParent(folders, folderId, folder.id)),
  );
}

export function getSidebarFolderPathLabel(
  folders: FolderItem[],
  folderId: string,
  seen = new Set<string>(),
): string {
  if (seen.has(folderId)) return "";
  seen.add(folderId);
  const folder = folders.find((item) => item.id === folderId);
  if (!folder) return "";
  if (!folder.parent_id) return folder.name;
  const parentLabel = getSidebarFolderPathLabel(folders, folder.parent_id, seen);
  return parentLabel ? `${parentLabel} / ${folder.name}` : folder.name;
}

export function getSidebarDocumentDepth(
  documents: DocItem[],
  documentId: string,
  seen = new Set<string>(),
): number {
  if (seen.has(documentId)) return 0;
  seen.add(documentId);
  const doc = documents.find((item) => item.id === documentId);
  if (!doc) return 0;
  if (!doc.parent_id) return 1;
  const parentDepth = getSidebarDocumentDepth(documents, doc.parent_id, seen);
  return parentDepth > 0 ? parentDepth + 1 : 0;
}

export function isSidebarDocumentDescendant(
  documents: DocItem[],
  ancestorId: string,
  candidateId: string,
  seen = new Set<string>(),
): boolean {
  if (seen.has(candidateId)) return false;
  seen.add(candidateId);
  const candidate = documents.find((doc) => doc.id === candidateId);
  if (!candidate?.parent_id) return false;
  if (candidate.parent_id === ancestorId) return true;
  return isSidebarDocumentDescendant(documents, ancestorId, candidate.parent_id, seen);
}

export function canMoveSidebarDocumentToParent(
  documents: DocItem[],
  documentId: string,
  parentId: string,
  folderId?: string,
): boolean {
  const doc = documents.find((item) => item.id === documentId);
  const parent = documents.find((item) => item.id === parentId);
  if (!doc || !parent) return false;
  if (doc.id === parent.id || doc.parent_id === parent.id) return false;
  if (parent.folder_id !== folderId) return false;
  if (isSidebarDocumentDescendant(documents, doc.id, parent.id)) return false;
  const parentDepth = getSidebarDocumentDepth(documents, parent.id);
  return parentDepth > 0 && parentDepth < 3;
}

export function getSidebarDocumentPathLabel(
  folders: FolderItem[],
  documents: DocItem[],
  documentId: string,
  seen = new Set<string>(),
): string {
  if (seen.has(documentId)) return "";
  seen.add(documentId);
  const doc = documents.find((item) => item.id === documentId);
  if (!doc) return "";
  const parentLabel = doc.parent_id
    ? getSidebarDocumentPathLabel(folders, documents, doc.parent_id, seen)
    : doc.folder_id
      ? getSidebarFolderPathLabel(folders, doc.folder_id)
      : "";
  return parentLabel ? `${parentLabel} / ${doc.title}` : doc.title;
}

export function getSidebarCreateLocationOptions(
  spaceName: string,
  folders: FolderItem[],
  documents: DocItem[],
  kind: SidebarCreateKind,
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
    options.push({
      id: `folder:${folder.id}`,
      label: `${spaceName} / ${getSidebarFolderPathLabel(folders, folder.id)}`,
      description: "Folder",
      icon: "folder",
      folderId: folder.id,
      parentId: null,
    });
  }

  if (kind === "document") {
    for (const doc of sortSidebarDocuments(documents)) {
      options.push({
        id: `document:${doc.id}`,
        label: `${spaceName} / ${getSidebarDocumentPathLabel(folders, documents, doc.id)}`,
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

const RECENT_SPACE_STORAGE_KEY = "starlane_recent_space_ids";
const SPACE_SELECTOR_LIMIT = 25;
const RECENT_SPACE_LIMIT = 5;
const OWNED_SPACE_LIMIT = 10;

export function Sidebar({ showHeader = true, mobileSettingsMenu = false }: SidebarProps = {}) {
  const navigate = useNavigate();
  const location = useLocation();
  const { spaceId: routeSpaceId } = useParams<{ spaceId: string }>();
  const { user } = useAuth();
  const localMode = getCurrentLaunchMode() === "local";
  const canCreateSpace = localMode || user?.role === "admin";
  const [spaces, setSpaces] = useState<Space[]>([]);
  const [activeSpace, setActiveSpace] = useState<Space | null>(null);
  const [folders, setFolders] = useState<FolderItem[]>([]);
  const [documents, setDocuments] = useState<DocItem[]>([]);
  const [templates, setTemplates] = useState<TemplateRecord[]>([]);
  const [spaceMenuOpen, setSpaceMenuOpen] = useState(false);
  const [settingsMenuOpen, setSettingsMenuOpen] = useState(false);
  const [spaceSearchQuery, setSpaceSearchQuery] = useState("");
  const [recentSpaceIds, setRecentSpaceIds] = useState<string[]>([]);
  const [starredDocuments, setStarredDocuments] = useState<StarredDocumentRecord[]>(() =>
    readStarredDocuments(),
  );
  const spaceMenuRef = useRef<HTMLDivElement>(null);
  const spaceSearchInputRef = useRef<HTMLInputElement>(null);
  const [templatePickerOpen, setTemplatePickerOpen] = useState(false);
  const [templateFolderId, setTemplateFolderId] = useState<string | undefined>(undefined);
  const [templateParentId, setTemplateParentId] = useState<string | undefined>(undefined);

  // Create space
  const [createSpaceOpen, setCreateSpaceOpen] = useState(false);
  const [newSpaceName, setNewSpaceName] = useState("");
  const spaceInputRef = useRef<HTMLInputElement>(null);

  // Create document or folder
  const [createDialog, setCreateDialog] = useState<SidebarCreateDialogState | null>(null);
  const createTitleInputRef = useRef<HTMLInputElement>(null);
  const [renameTarget, setRenameTarget] = useState<
    | { type: "folder"; id: string; currentName: string }
    | { type: "document"; id: string; currentName: string }
    | null
  >(null);
  const [deleteTarget, setDeleteTarget] = useState<
    | { type: "folder"; id: string; name: string }
    | { type: "document"; id: string; name: string }
    | null
  >(null);
  const [renameName, setRenameName] = useState("");
  const renameInputRef = useRef<HTMLInputElement>(null);
  const [movingFolderId, setMovingFolderId] = useState<string | null>(null);
  const [movingDocumentId, setMovingDocumentId] = useState<string | null>(null);
  const [moveTargetSpaceId, setMoveTargetSpaceId] = useState<string>("");
  const [moveLocationQuery, setMoveLocationQuery] = useState("");
  const [moveTargetFolders, setMoveTargetFolders] = useState<FolderItem[]>([]);
  const [moveTargetDocuments, setMoveTargetDocuments] = useState<DocItem[]>([]);
  const [draggingFolderId, setDraggingFolderId] = useState<string | null>(null);

  const updateRecentSpaceIds = useCallback((spaceId: string) => {
    setRecentSpaceIds((prev) => {
      const next = [spaceId, ...prev.filter((id) => id !== spaceId)].slice(0, SPACE_SELECTOR_LIMIT);
      try {
        window.localStorage.setItem(RECENT_SPACE_STORAGE_KEY, JSON.stringify(next));
      } catch {
        // Recent spaces are a convenience, not critical state.
      }
      return next;
    });
  }, []);

  useEffect(() => {
    try {
      const stored = JSON.parse(window.localStorage.getItem(RECENT_SPACE_STORAGE_KEY) || "[]");
      if (Array.isArray(stored)) {
        setRecentSpaceIds(stored.filter((id): id is string => typeof id === "string"));
      }
    } catch {
      setRecentSpaceIds([]);
    }
  }, []);

  // Load spaces
  useEffect(() => {
    dataClient
      .listSpaces()
      .then((data) => {
        setSpaces(data);
        if (data.length > 0) {
          const routeSpace = data.find((space) => space.id === routeSpaceId);
          setActiveSpace((prev) => routeSpace ?? prev ?? data[0]);
        }
      })
      .catch(() => {});
  }, [routeSpaceId]);

  useEffect(() => {
    if (!routeSpaceId) return;
    const routeSpace = spaces.find((space) => space.id === routeSpaceId);
    if (routeSpace) setActiveSpace(routeSpace);
  }, [routeSpaceId, spaces]);

  useEffect(() => {
    if (activeSpace?.id) updateRecentSpaceIds(activeSpace.id);
  }, [activeSpace?.id, updateRecentSpaceIds]);

  // Load folders and docs when space changes
  useEffect(() => {
    if (!activeSpace) return;
    dataClient
      .listFolders(activeSpace.id)
      .then(setFolders)
      .catch(() => {});
    dataClient
      .listDocuments(activeSpace.id)
      .then(setDocuments)
      .catch(() => {});
    dataClient
      .listTemplates(activeSpace.id)
      .then(setTemplates)
      .catch(() => setTemplates([]));
  }, [activeSpace]);

  useEffect(() => {
    const refreshStarredDocuments = () => setStarredDocuments(readStarredDocuments());
    window.addEventListener("starlane:starred-documents-changed", refreshStarredDocuments);
    return () =>
      window.removeEventListener("starlane:starred-documents-changed", refreshStarredDocuments);
  }, []);

  useEffect(() => {
    if (!activeSpace?.id) return;
    void loadStarredDocuments(activeSpace.id).then(setStarredDocuments);
  }, [activeSpace?.id]);

  useEffect(() => {
    if (!moveTargetSpaceId || (!movingFolderId && !movingDocumentId)) {
      setMoveTargetFolders([]);
      setMoveTargetDocuments([]);
      return;
    }

    let cancelled = false;
    void Promise.all([
      dataClient.listFolders(moveTargetSpaceId),
      dataClient.listDocuments(moveTargetSpaceId),
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

  useEffect(() => {
    if (createDialog) createTitleInputRef.current?.focus();
  }, [createDialog]);

  useEffect(() => {
    if (renameTarget) renameInputRef.current?.focus();
  }, [renameTarget]);

  useEffect(() => {
    const handler = (event: MouseEvent) => {
      if (spaceMenuRef.current && !spaceMenuRef.current.contains(event.target as Node)) {
        setSpaceMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  const refreshSpaceContents = async (spaceId: string) => {
    const [nextFolders, nextDocs] = await Promise.all([
      dataClient.listFolders(spaceId),
      dataClient.listDocuments(spaceId),
    ]);
    setFolders(nextFolders);
    setDocuments(nextDocs);
  };

  const handleCreateSpace = async () => {
    if (!canCreateSpace) return;
    const name = newSpaceName.trim();
    if (!name) return;
    try {
      const space = await dataClient.createSpace(name);
      const updated = await dataClient.listSpaces();
      setSpaces(updated);
      setActiveSpace(space);
      updateRecentSpaceIds(space.id);
      setCreateSpaceOpen(false);
      setNewSpaceName("");
      navigate(getSidebarHomePath(space.id));
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
    setCreateDialog(null);
  };

  const startCreateFolder = (parentId?: string) => {
    const selectedFolder = parentId ? folders.find((folder) => folder.id === parentId) : null;
    setCreateDialog({
      kind: "folder",
      title: "Untitled folder",
      query: selectedFolder
        ? `${activeSpace?.name ?? "Space"} / ${getSidebarFolderPathLabel(folders, selectedFolder.id)}`
        : `${activeSpace?.name ?? "Space"} / Root`,
      folderId: null,
      parentId: parentId ?? null,
    });
  };

  const handleSelectSpace = (space: Space) => {
    setActiveSpace(space);
    updateRecentSpaceIds(space.id);
    setSpaceSearchQuery("");
    setSpaceMenuOpen(false);
    navigate(getSidebarHomePath(space.id));
  };

  const createDocumentFromTemplate = async (
    folderId?: string,
    template?: TemplateRecord,
    parentId?: string,
    requestedTitle?: string,
  ) => {
    if (!activeSpace) return;
    const title = requestedTitle?.trim() || template?.name || "Untitled";
    const rendered = template
      ? renderTemplateContent(template.content, {
          title,
          space: activeSpace.name,
          author: user?.display_name,
          properties: templatePropertiesAsRecord(template.properties),
        })
      : undefined;
    try {
      const doc = await dataClient.createDocument(activeSpace.id, {
        title,
        folder_id: folderId ?? null,
        parent_id: parentId ?? null,
        content: rendered?.content,
        template_id: template?.id,
      });
      emitGraphDirty(activeSpace.id, "document-created");
      const docs = await dataClient.listDocuments(activeSpace.id);
      setDocuments(docs);
      navigate(getSidebarDocumentPath(activeSpace.id, doc.id));
    } catch {}
  };

  const handleCreateDocument = async (folderId?: string, parentId?: string) => {
    setCreateDialog({
      kind: "document",
      title: "Untitled",
      query: parentId
        ? `${activeSpace?.name ?? "Space"} / ${getSidebarDocumentPathLabel(folders, documents, parentId)}`
        : folderId
          ? `${activeSpace?.name ?? "Space"} / ${getSidebarFolderPathLabel(folders, folderId)}`
          : `${activeSpace?.name ?? "Space"} / Root`,
      folderId: folderId ?? null,
      parentId: parentId ?? null,
    });
  };

  const submitCreateDialog = async () => {
    if (!createDialog) return;
    const title = createDialog.title.trim();
    if (!title) return;
    if (createDialog.kind === "folder") {
      await handleCreateFolder(title, createDialog.parentId);
      return;
    }
    if (templates.length > 0) {
      setTemplateFolderId(createDialog.folderId ?? undefined);
      setTemplateParentId(createDialog.parentId ?? undefined);
      setTemplatePickerOpen(true);
      return;
    }
    await createDocumentFromTemplate(
      createDialog.folderId ?? undefined,
      undefined,
      createDialog.parentId ?? undefined,
      title,
    );
  };

  const handleSelectTemplate = async (template?: TemplateRecord) => {
    const folderId = templateFolderId;
    const parentId = templateParentId;
    const title = createDialog?.title;
    setTemplatePickerOpen(false);
    setTemplateFolderId(undefined);
    setTemplateParentId(undefined);
    setCreateDialog(null);
    await createDocumentFromTemplate(folderId, template, parentId, title);
  };

  const handleDeleteDocument = (docId: string) => {
    if (!activeSpace) return;
    const document = documents.find((doc) => doc.id === docId);
    setDeleteTarget({ type: "document", id: docId, name: document?.title ?? "" });
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

  const handleRenameDocument = async (docId: string, currentTitle: string) => {
    if (!activeSpace) return;
    const nextTitle = renameName.trim();
    if (!nextTitle || nextTitle === currentTitle) return;
    try {
      await dataClient.updateDocument(activeSpace.id, docId, { title: nextTitle });
      emitGraphDirty(activeSpace.id, "document-metadata-updated");
      setDocuments((prev) =>
        prev.map((doc) => (doc.id === docId ? { ...doc, title: nextTitle } : doc)),
      );
      setRenameTarget(null);
      setRenameName("");
    } catch {}
  };

  const handleRenameFolder = async (folderId: string, currentName: string) => {
    if (!activeSpace) return;
    const nextName = renameName.trim();
    if (!nextName || nextName === currentName) return;
    try {
      await dataClient.updateFolder(activeSpace.id, folderId, { name: nextName });
      setFolders((prev) =>
        prev.map((folder) => (folder.id === folderId ? { ...folder, name: nextName } : folder)),
      );
      setRenameTarget(null);
      setRenameName("");
    } catch {}
  };

  const startRenameDocument = (docId: string, currentTitle: string) => {
    setRenameTarget({ type: "document", id: docId, currentName: currentTitle });
    setRenameName(currentTitle);
  };

  const startRenameFolder = (folderId: string, currentName: string) => {
    setRenameTarget({ type: "folder", id: folderId, currentName });
    setRenameName(currentName);
  };

  const cancelRename = () => {
    setRenameTarget(null);
    setRenameName("");
  };

  const submitRename = async () => {
    if (!renameTarget) return;
    if (renameTarget.type === "folder") {
      await handleRenameFolder(renameTarget.id, renameTarget.currentName);
    } else {
      await handleRenameDocument(renameTarget.id, renameTarget.currentName);
    }
  };

  const handleMoveFolder = async (folderId: string, parentId: string | null) => {
    if (!activeSpace) return;
    const targetSpaceId = moveTargetSpaceId || activeSpace.id;
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
    try {
      await dataClient.moveDocument(activeSpace.id, docId, {
        target_space_id: targetSpaceId,
        folder_id: folderId,
        parent_id: parentId,
      });
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

  const handleOpenToday = async () => {
    if (!activeSpace) return;
    const today = new Date();
    const todayTitle = formatDateStamp(today);
    try {
      const docs = await dataClient.listDocuments(activeSpace.id);
      const existing = docs.find((doc) => doc.title === todayTitle);
      if (existing) {
        navigate(getSidebarDocumentPath(activeSpace.id, existing.id));
        return;
      }

      const dailyTemplate = findDailyTemplate(templates);
      const folders = await dataClient.listFolders(activeSpace.id);
      const dailyFolder =
        findDailyFolder(folders) ??
        (await dataClient.createFolder(activeSpace.id, DAILY_FOLDER_NAME));
      const created = await dataClient.createDocument(activeSpace.id, {
        title: todayTitle,
        folder_id: dailyFolder.id,
        content: dailyTemplate ? renderDailyTemplateContent(dailyTemplate, today) : undefined,
      });
      emitGraphDirty(activeSpace.id, "document-created");
      await refreshSpaceContents(activeSpace.id);
      navigate(getSidebarDocumentPath(activeSpace.id, created.id));
    } catch {}
  };

  const homePath = getSidebarHomePath(activeSpace?.id);
  const recentPath = getSidebarRecentPath(activeSpace?.id);
  const journalPath = getSidebarJournalPath(activeSpace?.id);
  const graphPath = getSidebarGraphPath(activeSpace?.id);
  const workflowPath = getSidebarWorkflowPath(activeSpace?.id);
  const starredPath = getSidebarStarredPath(activeSpace?.id);
  const homeActive = isSidebarRouteActive(location.pathname, homePath);
  const recentActive = isSidebarRouteActive(location.pathname, recentPath);
  const journalActive = isSidebarRouteActive(location.pathname, journalPath);
  const graphActive = isSidebarRouteActive(location.pathname, graphPath);
  const workflowActive = isSidebarRouteActive(location.pathname, workflowPath);
  const activeStarredDocuments = activeSpace
    ? starredDocuments.filter((doc) => doc.spaceId === activeSpace.id).slice(0, 8)
    : [];
  const canAdminSpace = activeSpace?.member_role === "owner";
  const canShowSpaceAdmin = canAdminSpace || localMode;
  const normalizedSpaceSearch = spaceSearchQuery.trim().toLowerCase();
  const recentSpaceSet = new Set(recentSpaceIds);
  const recentSpaces = recentSpaceIds
    .map((id) => spaces.find((space) => space.id === id))
    .filter((space): space is Space => Boolean(space))
    .slice(0, RECENT_SPACE_LIMIT);
  const ownedSpaces = spaces
    .filter((space) => space.member_role === "owner" && !recentSpaceSet.has(space.id))
    .slice(0, OWNED_SPACE_LIMIT);
  const pinnedSpaceIds = new Set([...recentSpaces, ...ownedSpaces].map((space) => space.id));
  const remainingSpaceLimit = Math.max(
    0,
    SPACE_SELECTOR_LIMIT - recentSpaces.length - ownedSpaces.length,
  );
  const remainingSpaces = spaces
    .filter((space) => !pinnedSpaceIds.has(space.id))
    .slice(0, remainingSpaceLimit);
  const searchedSpaces = normalizedSpaceSearch
    ? spaces
        .filter(
          (space) =>
            space.name.toLowerCase().includes(normalizedSpaceSearch) ||
            space.slug.toLowerCase().includes(normalizedSpaceSearch) ||
            (getPersonalSpaceOwnerLabel(space)?.toLowerCase().includes(normalizedSpaceSearch) ??
              false),
        )
        .slice(0, SPACE_SELECTOR_LIMIT)
    : [];
  const spaceSections = normalizedSpaceSearch
    ? [{ label: "Search results", spaces: searchedSpaces }]
    : [
        { label: "Recent", spaces: recentSpaces },
        { label: "Owned", spaces: ownedSpaces },
        { label: "All spaces", spaces: remainingSpaces },
      ].filter((section) => section.spaces.length > 0);
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
  const moveFolderSubtreeDepth = movingFolder
    ? getSidebarFolderSubtreeDepth(folders, movingFolder.id)
    : 1;
  const validMoveTargetFolders =
    movingFolder && selectedMoveSpace
      ? sortSidebarFolders(
          moveTargetFolders.filter((target) => {
            if (target.id === movingFolder.id) return false;
            if (selectedMoveSpace.id === activeSpace?.id) {
              return canMoveSidebarFolderToParent(folders, movingFolder.id, target.id);
            }
            const targetDepth = getSidebarFolderDepth(moveTargetFolders, target.id);
            return targetDepth > 0 && targetDepth + moveFolderSubtreeDepth <= 3;
          }),
        )
      : [];
  const canMoveSelectedFolderToRoot =
    movingFolder && selectedMoveSpace
      ? selectedMoveSpace.id !== activeSpace?.id || canMoveSidebarFolderToRoot(movingFolder, true)
      : false;
  const moveDocumentFolderTargets = movingDocument ? sortSidebarFolders(moveTargetFolders) : [];
  const moveDocumentPageTargets =
    movingDocument && selectedMoveSpace
      ? sortSidebarDocuments(
          moveTargetDocuments.filter((target) => {
            if (target.id === movingDocument.id) return false;
            if (selectedMoveSpace.id === activeSpace?.id) {
              return canMoveSidebarDocumentToParent(
                documents,
                movingDocument.id,
                target.id,
                target.folder_id,
              );
            }
            return getSidebarDocumentDepth(moveTargetDocuments, target.id) < 3;
          }),
        )
      : [];
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
            label: `${selectedMoveSpace.name} / ${getSidebarFolderPathLabel(moveTargetFolders, target.id)}`,
            description: "Folder",
            icon: "folder" as const,
            folderId: target.id,
            parentId: null,
          })),
          ...moveDocumentPageTargets.map((target) => ({
            id: `document:${target.id}`,
            label: `${selectedMoveSpace.name} / ${getSidebarDocumentPathLabel(
              moveTargetFolders,
              moveTargetDocuments,
              target.id,
            )}`,
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
            label: `${selectedMoveSpace.name} / ${getSidebarFolderPathLabel(moveTargetFolders, target.id)}`,
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
      ? getSidebarCreateLocationOptions(activeSpace.name, folders, documents, createDialog.kind)
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

  const openMoveDocument = useCallback((docId: string) => {
    setMovingFolderId(null);
    setMovingDocumentId(docId);
    setMoveTargetSpaceId(activeSpace?.id ?? "");
    setMoveLocationQuery("");
  }, [activeSpace?.id]);

  useEffect(() => {
    const handler = (event: Event) => {
      const documentId = (event as CustomEvent<{ documentId?: string }>).detail?.documentId;
      if (!documentId) return;
      openMoveDocument(documentId);
    };
    document.addEventListener(OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT, handler);
    return () => document.removeEventListener(OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT, handler);
  }, [openMoveDocument]);

  const renderSpaceOption = (space: Space) => {
    const selected = activeSpace?.id === space.id;
    const ownerLabel = getPersonalSpaceOwnerLabel(space);
    return (
      <button
        key={space.id}
        type="button"
        role="menuitemradio"
        aria-checked={selected}
        onClick={() => handleSelectSpace(space)}
        className={`rounded-button flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm transition-colors duration-100 ${
          selected
            ? "bg-surface-raised text-text"
            : "text-text-muted hover:bg-surface-raised hover:text-text"
        }`}
      >
        <span className="bg-bg flex h-5 w-5 shrink-0 items-center justify-center rounded text-[10px]">
          {space.icon || space.name[0]?.toUpperCase()}
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

  return (
    <div className="flex h-full min-h-0 flex-col">
      {showHeader && (
        <div className="border-border/60 flex shrink-0 items-center justify-between gap-2 border-b p-3">
          <button
            type="button"
            onClick={() => navigate(getSidebarHomePath(activeSpace?.id))}
            disabled={!activeSpace}
            className="rounded-button hover:bg-surface-raised flex min-w-0 items-center text-left transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-60"
            aria-label="Open Starlane Notes home"
          >
            <BrandLogo size="sm" variant="sidebar" />
          </button>
          <button
            type="button"
            onClick={() => openSidebarGlobalSearch(activeSpace?.id)}
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-8 w-8 shrink-0 items-center justify-center transition-colors duration-100"
            aria-label="Search documents"
          >
            <Search size={15} />
          </button>
        </div>
      )}
      {/* Navigation tree */}
      <nav className="min-h-0 flex-1 overflow-y-auto p-2" aria-label="Space navigation">
        <div className="border-border/60 mb-2 space-y-0.5 border-b pb-2">
          <button
            type="button"
            onClick={() => navigate(homePath)}
            disabled={!activeSpace}
            className={`rounded-button flex w-full items-center gap-2 px-2 py-1.5 text-sm transition-colors duration-100 disabled:opacity-40 ${
              homeActive
                ? "bg-surface-raised text-text"
                : "text-text-muted hover:bg-surface-raised hover:text-text"
            }`}
          >
            <Home size={14} className="shrink-0" />
            Home
          </button>
          <button
            type="button"
            onClick={() => navigate(recentPath)}
            disabled={!activeSpace}
            className={`rounded-button flex w-full items-center gap-2 px-2 py-1.5 text-sm transition-colors duration-100 disabled:opacity-40 ${
              recentActive
                ? "bg-surface-raised text-text"
                : "text-text-muted hover:bg-surface-raised hover:text-text"
            }`}
          >
            <Clock3 size={14} className="shrink-0" />
            Recent
          </button>
          <button
            type="button"
            onClick={() => {
              if (activeSpace) navigate(journalPath);
            }}
            disabled={!activeSpace}
            className={`rounded-button flex w-full items-center gap-2 px-2 py-1.5 text-sm transition-colors duration-100 disabled:opacity-40 ${
              journalActive
                ? "bg-surface-raised text-text"
                : "text-text-muted hover:bg-surface-raised hover:text-text"
            }`}
          >
            <CalendarDays size={14} />
            Journal
          </button>
          <button
            type="button"
            onClick={() => {
              if (activeSpace) navigate(graphPath);
            }}
            disabled={!activeSpace}
            className={`rounded-button flex w-full items-center gap-2 px-2 py-1.5 text-sm transition-colors duration-100 disabled:opacity-40 ${
              graphActive
                ? "bg-surface-raised text-text"
                : "text-text-muted hover:bg-surface-raised hover:text-text"
            }`}
          >
            <Workflow size={14} />
            Graph
          </button>
          <button
            type="button"
            onClick={() => {
              if (activeSpace) navigate(workflowPath);
            }}
            disabled={!activeSpace}
            className={`rounded-button flex w-full items-center gap-2 px-2 py-1.5 text-sm transition-colors duration-100 disabled:opacity-40 ${
              workflowActive
                ? "bg-surface-raised text-text"
                : "text-text-muted hover:bg-surface-raised hover:text-text"
            }`}
          >
            <ListChecks size={14} />
            Workflows
          </button>
        </div>

        <div className="mb-2 grid grid-cols-2 gap-1">
          <button
            type="button"
            onClick={() => handleCreateDocument()}
            disabled={!activeSpace}
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex min-w-0 items-center justify-center gap-1.5 px-2 py-1.5 text-sm transition-colors duration-100 disabled:opacity-40"
          >
            <Plus size={14} className="shrink-0" />
            <span className="truncate">New Doc</span>
          </button>
          <button
            type="button"
            onClick={() => startCreateFolder()}
            disabled={!activeSpace}
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex min-w-0 items-center justify-center gap-1.5 px-2 py-1.5 text-sm transition-colors duration-100 disabled:opacity-40"
          >
            <FolderPlus size={14} className="shrink-0" />
            <span className="truncate">New Folder</span>
          </button>
        </div>

        {templatePickerOpen && (
          <DialogShell
            labelledBy="template-picker-title"
            maxWidth="sm"
            onClose={() => setTemplatePickerOpen(false)}
          >
            <div className="p-3">
              <div className="mb-3 flex items-center justify-between gap-3">
                <div>
                  <h2 id="template-picker-title" className="text-text text-sm font-medium">
                    New document
                  </h2>
                  <p className="text-text-muted mt-0.5 text-xs">Choose a starting point.</p>
                </div>
                <button
                  type="button"
                  onClick={() => setTemplatePickerOpen(false)}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
                  aria-label="Close"
                >
                  <X size={14} />
                </button>
              </div>
              <div className="space-y-1">
                <button
                  type="button"
                  onClick={() => void handleSelectTemplate()}
                  className="rounded-button text-text hover:bg-surface-raised flex w-full items-center gap-2 px-2 py-2 text-left text-sm transition-colors duration-100"
                >
                  <FileText size={14} className="shrink-0" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">Blank document</span>
                    <span className="text-text-muted block truncate text-xs">Untitled</span>
                  </span>
                </button>
                {templates.map((template) => (
                  <button
                    key={template.id}
                    type="button"
                    onClick={() => void handleSelectTemplate(template)}
                    className="rounded-button text-text hover:bg-surface-raised flex w-full items-center gap-2 px-2 py-2 text-left text-sm transition-colors duration-100"
                  >
                    <FileText size={14} className="shrink-0" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-medium">{template.name}</span>
                      <span className="text-text-muted block truncate text-xs">
                        {template.description || "Template"}
                      </span>
                    </span>
                  </button>
                ))}
              </div>
            </div>
          </DialogShell>
        )}

        <button
          type="button"
          onClick={handleOpenToday}
          disabled={!activeSpace}
          className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text mb-1 flex w-full items-center gap-2 px-2 py-1.5 text-sm transition-colors duration-100 disabled:opacity-40"
        >
          <span className={SIDEBAR_TREE_TOGGLE_LANE_CLASS} aria-hidden="true" />
          <CalendarDays size={SIDEBAR_TREE_ICON_SIZE} className="shrink-0" />
          <span className="truncate">Daily note (Today)</span>
        </button>

        {activeSpace && activeStarredDocuments.length > 0 && (
          <div className="border-border/60 mb-2 border-b pb-2">
            <div className="text-text-muted mb-1 flex items-center justify-between gap-2 px-2 text-[10px] font-medium uppercase tracking-wide">
              <span className="flex min-w-0 items-center gap-1.5">
                <Star size={11} className="text-accent" fill="currentColor" />
                Starred
              </span>
              <button
                type="button"
                onClick={() => navigate(starredPath)}
                className="rounded-input text-text-muted hover:bg-surface-raised hover:text-text px-1.5 py-0.5 text-[10px] normal-case tracking-normal"
              >
                Manage
              </button>
            </div>
            <div className="space-y-0.5">
              {activeStarredDocuments.map((doc) => (
                <button
                  key={`${doc.spaceId}:${doc.id}`}
                  type="button"
                  onClick={() => navigate(getSidebarDocumentPath(doc.spaceId, doc.id))}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm transition-colors duration-100"
                >
                  <span
                    className={SIDEBAR_TREE_TOGGLE_LANE_CLASS}
                    aria-hidden="true"
                  />
                  <Star size={12} className="text-accent shrink-0" fill="currentColor" />
                  <span className="min-w-0 flex-1 truncate">
                    {doc.icon ? `${doc.icon} ` : ""}
                    {doc.title}
                  </span>
                </button>
              ))}
            </div>
          </div>
        )}

        <div
          onDragOver={handleRootFolderDragOver}
          onDrop={handleRootFolderDrop}
          className={`rounded-card transition-colors duration-100 ${
            canDropFolderOnRoot ? "bg-accent/10 ring-accent/50 ring-1" : ""
          }`}
        >
          {/* Folders */}
          {sortSidebarFolders(folders.filter((f) => !f.parent_id)).map((folder) => (
            <FolderNode
              key={folder.id}
              folder={folder}
              allFolders={folders}
              documents={documents}
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
              onRenameDocument={startRenameDocument}
              onMoveDocument={openMoveDocument}
              currentPath={location.pathname}
              touchActions={mobileSettingsMenu}
            />
          ))}

          {/* Root documents (no folder) */}
          {sortSidebarDocuments(documents.filter((d) => !d.folder_id && !d.parent_id)).map(
            (doc) => (
              <DocRow
                key={doc.id}
                doc={doc}
                spaceId={activeSpace?.id || ""}
                navigate={navigate}
                onRename={startRenameDocument}
                onMove={openMoveDocument}
                onDelete={handleDeleteDocument}
                onCreateChild={(parentId) => handleCreateDocument(undefined, parentId)}
                currentPath={location.pathname}
                touchActions={mobileSettingsMenu}
                documents={documents}
              />
            ),
          )}
        </div>
      </nav>

      {createDialog && (
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
                  New {createDialog.kind === "document" ? "document" : "folder"}
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
                  placeholder={createDialog.kind === "document" ? "Document title" : "Folder name"}
                  aria-label={createDialog.kind === "document" ? "Document title" : "Folder name"}
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
                disabled={!createDialog.title.trim()}
                className="rounded-button bg-primary px-3 py-1.5 text-sm font-medium text-white transition-opacity duration-100 disabled:opacity-40"
              >
                Create
              </button>
            </div>
          </form>
        </DialogShell>
      )}

      {renameTarget && (
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
        </DialogShell>
      )}

      {deleteTarget && (
        <ConfirmDialog
          labelledBy="delete-sidebar-item-title"
          title={getSidebarDeleteConfirmationTitle(deleteTarget.type)}
          message={getSidebarDeleteConfirmationMessage(deleteTarget.type, deleteTarget.name)}
          confirmLabel="Delete"
          destructive
          onCancel={() => setDeleteTarget(null)}
          onConfirm={() => void submitDeleteTarget()}
        />
      )}

      {movingFolder && (
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
        </DialogShell>
      )}

      {movingDocument && selectedMoveSpace && (
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
        </DialogShell>
      )}

      <div className="border-border shrink-0 border-t p-2 pb-[calc(env(safe-area-inset-bottom)+0.75rem)]">
        <div className="flex items-center gap-1">
          <div className="relative min-w-0 flex-1" ref={spaceMenuRef}>
            <button
              type="button"
              onClick={() => setSpaceMenuOpen((prev) => !prev)}
              className="rounded-button border-border bg-surface text-text hover:bg-surface-raised flex w-full items-center gap-2 border px-2 py-2 text-left text-sm transition-colors duration-100"
              aria-haspopup="menu"
              aria-expanded={spaceMenuOpen}
            >
              <span className="rounded-button bg-surface-raised flex h-6 w-6 shrink-0 items-center justify-center text-xs">
                {activeSpace?.icon || activeSpace?.name?.[0]?.toUpperCase() || "S"}
              </span>
              <span className="min-w-0 flex-1">
                <span className="block truncate font-medium">
                  {activeSpace?.name || "No spaces"}
                </span>
                <span className="text-text-muted block truncate text-[11px]">Space</span>
              </span>
              <ChevronDown
                size={14}
                className={`text-text-muted shrink-0 transition-transform duration-100 ${
                  spaceMenuOpen ? "" : "rotate-180"
                }`}
              />
            </button>

            {spaceMenuOpen && (
              <div
                role="menu"
                aria-label="Spaces"
                className="starlane-menu-surface absolute bottom-full left-0 right-0 z-40 mb-1 overflow-hidden"
              >
                <div className="border-border border-b p-1.5">
                  <SearchInput
                    ref={spaceSearchInputRef}
                    value={spaceSearchQuery}
                    onChange={(event) => setSpaceSearchQuery(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Escape") {
                        setSpaceSearchQuery("");
                        setSpaceMenuOpen(false);
                      }
                    }}
                    placeholder="Search spaces"
                    aria-label="Search spaces"
                    className="h-8 pl-8 text-xs"
                  />
                </div>
                <div className="scrollbar-none max-h-64 overflow-y-auto">
                  {spaces.length === 0 ? (
                    <div className="text-text-muted px-2 py-2 text-sm">No spaces</div>
                  ) : visibleSpaceCount === 0 ? (
                    <div className="text-text-muted px-2 py-2 text-sm">No matching spaces</div>
                  ) : (
                    spaceSections.map((section) => (
                      <div key={section.label} className="p-1">
                        <div className="text-text-muted px-1.5 pb-1 pt-1 text-[10px] font-medium uppercase tracking-wide">
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
                      className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex w-full items-center gap-2 px-2 py-1.5 text-sm transition-colors duration-100"
                    >
                      <Plus size={14} />
                      New space
                    </button>
                  </div>
                )}
              </div>
            )}
          </div>

          {mobileSettingsMenu ? (
            <div className="relative shrink-0">
              <button
                type="button"
                onClick={() => setSettingsMenuOpen((open) => !open)}
                className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-10 w-10 items-center justify-center transition-colors duration-100"
                aria-label="Open settings menu"
                aria-haspopup="menu"
                aria-expanded={settingsMenuOpen}
              >
                <Settings size={14} />
              </button>
              {settingsMenuOpen && (
                <div
                  role="menu"
                  aria-label="Settings"
                  className="starlane-menu-surface absolute bottom-full right-0 z-40 mb-1 w-48"
                >
                  <button
                    type="button"
                    role="menuitem"
                    onClick={() => {
                      setSettingsMenuOpen(false);
                      navigate(getSidebarSpaceSettingsPath(activeSpace?.id));
                    }}
                    disabled={!canShowSpaceAdmin || !activeSpace}
                    className="starlane-menu-item"
                  >
                    <Settings size={14} />
                    Space settings
                  </button>
                  <button
                    type="button"
                    role="menuitem"
                    onClick={() => {
                      setSettingsMenuOpen(false);
                      navigate("/mobile/app-settings");
                    }}
                    className="starlane-menu-item"
                  >
                    <Settings size={14} />
                    App settings
                  </button>
                  {!localMode && user && (
                    <button
                      type="button"
                      role="menuitem"
                      onClick={() => {
                        setSettingsMenuOpen(false);
                        navigate(SIDEBAR_PROFILE_SETTINGS_PATH);
                      }}
                      className="starlane-menu-item"
                    >
                      <UserCircle size={14} />
                      Profile settings
                    </button>
                  )}
                  {shouldShowSidebarSiteAdministration(localMode, user?.role) && (
                    <button
                      type="button"
                      role="menuitem"
                      onClick={() => {
                        setSettingsMenuOpen(false);
                        navigate(SIDEBAR_SITE_ADMINISTRATION_PATH);
                      }}
                      className="starlane-menu-item"
                    >
                      <Settings size={14} />
                      Site administration
                    </button>
                  )}
                </div>
              )}
            </div>
          ) : (
            <button
              type="button"
              onClick={() => navigate(getSidebarSpaceSettingsPath(activeSpace?.id))}
              disabled={!canShowSpaceAdmin || !activeSpace}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-10 w-10 shrink-0 items-center justify-center transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-40"
              aria-label="Open space settings"
            >
              <Settings size={14} />
            </button>
          )}
        </div>

        {createSpaceOpen && canCreateSpace && (
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
                  <p className="text-text-muted mt-0.5 text-xs">Create a space and switch to it.</p>
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
          </DialogShell>
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
  onRename,
  onMove,
  onCreateChild,
  currentPath,
  touchActions = false,
  documents,
}: {
  doc: DocItem;
  spaceId: string;
  navigate: (path: string) => void;
  onDelete: (id: string) => void;
  onRename: (id: string, currentTitle: string) => void;
  onMove: (id: string) => void;
  onCreateChild: (parentId: string) => void;
  currentPath: string;
  touchActions?: boolean;
  documents: DocItem[];
}) {
  const active = currentPath.includes(doc.id);
  const [menuOpen, setMenuOpen] = useState(false);
  const [expanded, setExpanded] = useState(true);
  const childDocuments = sortSidebarDocuments(
    documents.filter((candidate) => candidate.parent_id === doc.id),
  );
  const hasChildren = childDocuments.length > 0;
  return (
    <div>
      <div
        className={`group relative flex items-center rounded-button transition-colors duration-100 ${
          active ? "bg-surface-raised text-text" : "text-text-muted hover:bg-surface-raised hover:text-text"
        }`}
      >
        {hasChildren ? (
          <button
            type="button"
            onClick={() => setExpanded((current) => !current)}
            className={`text-text-muted hover:text-text rounded-button ${SIDEBAR_TREE_TOGGLE_LANE_CLASS} transition-colors duration-100`}
            aria-label={`${expanded ? "Collapse" : "Expand"} child pages for ${doc.title}`}
            aria-expanded={expanded}
          >
            <ChevronRight
              size={12}
              className={`shrink-0 transition-transform duration-100 ${expanded ? "rotate-90" : ""}`}
            />
          </button>
        ) : (
          <span className={SIDEBAR_TREE_TOGGLE_LANE_CLASS} aria-hidden="true" />
        )}
        <button
          type="button"
          onClick={() => navigate(getSidebarDocumentPath(spaceId, doc.id))}
          className="flex min-w-0 flex-1 items-center gap-2 rounded-button px-2 py-1 pr-8 text-sm transition-colors duration-100"
        >
          <FileText size={SIDEBAR_TREE_ICON_SIZE} className="shrink-0" />
          <span className="truncate">
            {doc.icon || ""} {doc.title}
          </span>
        </button>
        <div className="absolute right-1 flex items-center gap-0.5">
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              setMenuOpen((prev) => !prev);
            }}
            className={`text-text-muted hover:bg-surface hover:text-text rounded-button flex h-7 w-7 items-center justify-center transition-colors duration-100 ${
              touchActions
                ? "pointer-events-auto opacity-100"
                : "pointer-events-none opacity-0 focus:pointer-events-auto focus:opacity-100 group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100"
            }`}
            aria-label="Document actions"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
          >
            <MoreHorizontal size={13} />
          </button>
          {menuOpen && (
            <RowActionMenu
              onClose={() => setMenuOpen(false)}
              items={[
                {
                  label: "New child page",
                  icon: <Plus size={13} />,
                  onSelect: () => onCreateChild(doc.id),
                },
                {
                  label: "Rename",
                  icon: <Pencil size={13} />,
                  onSelect: () => onRename(doc.id, doc.title),
                },
                {
                  label: "Move...",
                  icon: <MoveRight size={13} />,
                  onSelect: () => onMove(doc.id),
                },
                {
                  label: "Delete",
                  icon: <Trash2 size={13} />,
                  danger: true,
                  onSelect: () => onDelete(doc.id),
                },
              ]}
            />
          )}
        </div>
      </div>
      {expanded && childDocuments.length > 0 && (
        <div className="ml-5">
          {childDocuments.map((child) => (
            <DocRow
              key={child.id}
              doc={child}
              spaceId={spaceId}
              navigate={navigate}
              onDelete={onDelete}
              onRename={onRename}
              onMove={onMove}
              onCreateChild={onCreateChild}
              currentPath={currentPath}
              touchActions={touchActions}
              documents={documents}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function RowActionMenu({
  items,
  onClose,
}: {
  items: Array<{
    label: string;
    icon: ReactNode;
    danger?: boolean;
    onSelect: () => void;
  }>;
  onClose: () => void;
}) {
  return (
    <>
      <button
        type="button"
        aria-label="Close row actions"
        className="fixed inset-0 z-40 cursor-default"
        onClick={onClose}
      />
      <div role="menu" className="starlane-menu-surface absolute right-0 top-6 z-50 w-40">
        {items.map((item) => (
          <button
            key={item.label}
            type="button"
            role="menuitem"
            onClick={() => {
              onClose();
              item.onSelect();
            }}
            className={`rounded-button flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm transition-colors duration-100 ${
              item.danger
                ? SIDEBAR_DANGER_MENU_ITEM_CLASS
                : "text-text-muted hover:bg-surface-raised hover:text-text"
            }`}
          >
            {item.icon}
            {item.label}
          </button>
        ))}
      </div>
    </>
  );
}

function FolderNode({
  folder,
  allFolders,
  documents,
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
  onRenameDocument,
  onMoveDocument,
  currentPath,
  touchActions = false,
}: {
  folder: FolderItem;
  allFolders: FolderItem[];
  documents: DocItem[];
  spaceId: string;
  navigate: (path: string) => void;
  onCreateDocument: (folderId?: string, parentId?: string) => void;
  onCreateFolder: (parentId: string) => void;
  onMoveFolder: (folderId: string) => void;
  draggingFolderId: string | null;
  onFolderDragStart: (folderId: string) => void;
  onFolderDragEnd: () => void;
  onDropFolder: (folderId: string, parentId: string) => void;
  onRenameFolder: (id: string, currentName: string) => void;
  onDeleteFolder: (id: string) => void;
  onDeleteDocument: (id: string) => void;
  onRenameDocument: (id: string, currentTitle: string) => void;
  onMoveDocument: (id: string) => void;
  currentPath: string;
  touchActions?: boolean;
}) {
  const [expanded, setExpanded] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const childFolders = sortSidebarFolders(allFolders.filter((f) => f.parent_id === folder.id));
  const folderDocs = sortSidebarDocuments(
    documents.filter((d) => d.folder_id === folder.id && !d.parent_id),
  );
  const canCreateChildFolder = canCreateSidebarChildFolder(allFolders, folder.id);
  const localMode = getCurrentLaunchMode() === "local";
  const canMoveFolder =
    localMode ||
    canMoveSidebarFolderToRoot(folder, true) ||
    getSidebarFolderMoveTargets(allFolders, folder.id).length > 0;
  const canAcceptDraggedFolder = canDropSidebarFolderOnParent(
    allFolders,
    draggingFolderId,
    folder.id,
    localMode,
  );

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
        className={`group relative flex items-center rounded-button transition-colors duration-100 ${
          canAcceptDraggedFolder
            ? "bg-accent/10 text-text ring-accent/50 ring-1"
            : "text-text-muted hover:bg-surface-raised hover:text-text"
        }`}
      >
        <button
          type="button"
          onClick={() => setExpanded(!expanded)}
          className={`text-text-muted hover:text-text rounded-button ${SIDEBAR_TREE_TOGGLE_LANE_CLASS} transition-colors duration-100`}
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
          onClick={() => setExpanded(!expanded)}
          className="flex min-w-0 flex-1 items-center gap-2 rounded-button px-2 py-1 pr-8 text-sm transition-colors duration-100"
          aria-expanded={expanded}
        >
          <Folder size={SIDEBAR_TREE_ICON_SIZE} className="shrink-0" />
          <span className="truncate">
            {folder.icon || ""} {folder.name}
          </span>
        </button>
        <div className="absolute right-1 flex items-center gap-0.5">
          <button
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              setMenuOpen((prev) => !prev);
            }}
            className={`text-text-muted hover:bg-surface hover:text-text rounded-button flex h-7 w-7 items-center justify-center transition-colors duration-100 ${
              touchActions
                ? "pointer-events-auto opacity-100"
                : "pointer-events-none opacity-0 focus:pointer-events-auto focus:opacity-100 group-focus-within:pointer-events-auto group-focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100"
            }`}
            aria-label="Folder actions"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
          >
            <MoreHorizontal size={13} />
          </button>
          {menuOpen && (
            <RowActionMenu
              onClose={() => setMenuOpen(false)}
              items={[
                {
                  label: "New document",
                  icon: <Plus size={13} />,
                  onSelect: () => {
                    setExpanded(true);
                    onCreateDocument(folder.id);
                  },
                },
                ...(canCreateChildFolder
                  ? [
                      {
                        label: "New folder",
                        icon: <FolderPlus size={13} />,
                        onSelect: () => {
                          setExpanded(true);
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
                  onSelect: () => onRenameFolder(folder.id, folder.name),
                },
                {
                  label: "Delete",
                  icon: <Trash2 size={13} />,
                  danger: true,
                  onSelect: () => onDeleteFolder(folder.id),
                },
              ]}
            />
          )}
        </div>
      </div>

      {expanded && (
        <div className="ml-5">
          {childFolders.map((child) => (
            <FolderNode
              key={child.id}
              folder={child}
              allFolders={allFolders}
              documents={documents}
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
              onRenameDocument={onRenameDocument}
              onMoveDocument={onMoveDocument}
              currentPath={currentPath}
              touchActions={touchActions}
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
              onCreateChild={(parentId) => onCreateDocument(folder.id, parentId)}
              currentPath={currentPath}
              touchActions={touchActions}
              documents={documents}
            />
          ))}
        </div>
      )}
    </div>
  );
}
