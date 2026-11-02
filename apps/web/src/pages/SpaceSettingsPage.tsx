import { PendingDocumentDeletionReview } from "@/components/PendingDocumentDeletionReview";
import type { PendingDocumentDeletion } from "@/lib/pendingDocumentDelete";
import { runDraftOwnerSelection } from "@/lib/draftOwnerSelection";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import {
  Archive,
  Check,
  Copy,
  FileText,
  FilePenLine,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  Share2,
  Settings,
  ShieldCheck,
  ShieldX,
  Trash2,
  Upload,
  UserPlus,
  UserRoundX,
  UsersRound,
  Boxes,
} from "lucide-react";
import { Button, RESPONSIVE_TOUCH_ACTION_CLASS } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { DialogShell } from "@/components/ui/dialog-shell";
import { Input } from "@/components/ui/input";
import { MenuSelect } from "@/components/ui/menu-select";
import { api } from "@/lib/api";
import {
  dataClient,
  captureDraftSaveSelection,
  verifyDraftSaveSelection,
  type DraftSaveSelection,
  type SpaceRecord,
  type DraftSummaryRecord,
  type DraftAccessCandidateRecord,
  type DraftAccessRecord,
  type TemplateRecord,
  type DocumentRecord,
} from "@/lib/dataClient";
import { getTemplateEditorPath } from "@/lib/templates";
import {
  isPreferredJournalTemplate,
  withJournalTemplatePreference,
} from "@/lib/journalTemplatePreference";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import {
  SettingsWorkspace,
  type SettingsDialogSection,
} from "@/components/settings/SettingsDialog";
import { SpaceRolePermissions } from "@/components/settings/SpaceRolePermissions";
import { NotionImportSection } from "@/components/settings/NotionImportSection";
import {
  DEFAULT_SPACE_ROLE_DEFINITIONS,
  getSpaceRoleLabel,
  type SpacePermissionId,
  type SpaceRoleDefinition,
} from "@/lib/spacePermissions";

function selectionCurrent(original: DraftSaveSelection | null | undefined): boolean {
  if (!original) return false;
  try { verifyDraftSaveSelection(original); return true; } catch { return false; }
}

type SpaceSettingsSection =
  | "general"
  | "users"
  | "groups"
  | "roles"
  | "drafts"
  | "archived"
  | "trash"
  | "templates"
  | "import"
  | "local-spaces";

export const SPACE_SETTINGS_GENERAL_LAYOUT_CLASS =
  "rounded-card border-border bg-surface divide-border flex flex-col divide-y border";

const spaceSettingsSections: SettingsDialogSection[] = [
  { id: "general", label: "General", icon: <Settings size={16} />, group: "Space" },
  { id: "users", label: "Users", icon: <UserPlus size={16} />, group: "Access" },
  { id: "groups", label: "Groups", icon: <UsersRound size={16} />, group: "Access" },
  { id: "roles", label: "Roles", icon: <ShieldCheck size={16} />, group: "Access" },
  { id: "templates", label: "Templates", icon: <FileText size={16} />, group: "Content" },
  { id: "import", label: "Import", icon: <Upload size={16} />, group: "Content" },
  { id: "drafts", label: "Drafts", icon: <FilePenLine size={16} />, group: "Content" },
  { id: "archived", label: "Archived", icon: <Archive size={16} />, group: "Content" },
  { id: "trash", label: "Trash", icon: <Trash2 size={16} />, group: "Content" },
  { id: "local-spaces", label: "Local spaces", icon: <Boxes size={16} />, group: "Content" },
];

interface SpaceMemberItem {
  user_id: string;
  email: string;
  display_name: string;
  role: string;
}

export interface SpaceGroupItem {
  group_id: string;
  name: string;
  description?: string;
  role: string;
  member_count: number;
  managed: boolean;
}

interface EffectiveAccessSource {
  kind: "direct" | "group";
  id: string;
  name: string;
  role: string;
}

export interface EffectiveSpaceUserItem {
  user_id: string;
  email: string;
  display_name: string;
  effective_role?: string;
  direct_role?: string;
  blocked: boolean;
  sources: EffectiveAccessSource[];
}

interface RoleEditorState {
  id?: string;
  name: string;
  description: string;
  permissions: SpacePermissionId[];
}

interface SpacePrincipalCatalog {
  users: Array<{ id: string; email: string; display_name: string }>;
  groups: Array<{
    id: string;
    name: string;
    description?: string;
    member_count: number;
    managed_space_id?: string;
  }>;
}

const visibilityOptions = [
  { value: "private", label: "Private" },
  { value: "internal", label: "Everyone signed in" },
  { value: "public", label: "Public web" },
];

const SPACE_PERMISSION_OPTIONS: Record<SpacePermissionId, { label: string; description: string }> =
  {
    view: { label: "View space", description: "Open and read content in this space." },
    comment: { label: "Comment", description: "Read and participate in document comments." },
    edit_content: {
      label: "Edit content",
      description: "Create, edit, move, and organize documents.",
    },
    manage_members: {
      label: "Manage access",
      description: "Assign users, groups, and their roles.",
    },
    manage_space: {
      label: "Manage space",
      description: "Change space settings, roles, and visibility.",
    },
  };

type AccessDialogState =
  | { kind: "user"; mode: "add"; principalId: string; role: string }
  | { kind: "group"; mode: "add"; principalId: string; role: string }
  | {
      kind: "user" | "group";
      mode: "edit";
      principalId: string;
      principalName: string;
      role: string;
    };

export function sortSpacesForSettings(spaces: SpaceRecord[]): SpaceRecord[] {
  return [...spaces].sort((a, b) => a.name.localeCompare(b.name));
}

export function getSpaceSettingsRoute(spaceId: string): string {
  return `/spaces/${encodeURIComponent(spaceId)}/settings`;
}

export function getLocalSpaceEditState(
  space: Pick<SpaceRecord, "id">,
  activeSpaceId?: string | null,
): "editing" | "edit" {
  return space.id === activeSpaceId ? "editing" : "edit";
}

export const SPACE_USERS_DESKTOP_TABLE_CLASS = "hidden overflow-x-auto md:block";
export const SPACE_USERS_MOBILE_LIST_CLASS = "divide-border divide-y md:hidden";
export const SPACE_GROUPS_DESKTOP_TABLE_CLASS = "hidden overflow-x-auto md:block";
export const SPACE_GROUPS_MOBILE_LIST_CLASS = "divide-border divide-y md:hidden";
export const SPACE_DRAFTS_DESKTOP_TABLE_CLASS = "hidden overflow-x-auto md:block";
export const SPACE_DRAFTS_MOBILE_LIST_CLASS = "divide-border divide-y md:hidden";

export function getSpaceDraftActionState(accessRole: DraftSummaryRecord["access_role"]) {
  const canManageDraft = accessRole === "owner";
  return {
    canShare: canManageDraft,
    canDelete: canManageDraft,
  };
}

export function getSpaceDraftEmptyMessage(query: string): string {
  return query ? "No drafts match this search." : "No drafts in this space.";
}

export interface SpaceDraftMobileRowProps {
  draft: DraftSummaryRecord;
  onOpen: (draft: DraftSummaryRecord) => void;
  onShare: (draft: DraftSummaryRecord) => void;
  onDelete: (draft: DraftSummaryRecord) => void;
}

export function SpaceDraftMobileRow({
  draft,
  onOpen,
  onShare,
  onDelete,
}: SpaceDraftMobileRowProps) {
  const actionState = getSpaceDraftActionState(draft.access_role);

  return (
    <article className="min-w-0 px-3 py-3">
      <div className="min-w-0">
        <p className="text-text text-sm font-medium break-words">{draft.title}</p>
        <p className="text-text-muted mt-0.5 text-xs break-words">
          Published page: {draft.document_title}
        </p>
      </div>

      <dl className="mt-3 grid grid-cols-2 gap-x-3 gap-y-2 text-xs">
        <div className="min-w-0">
          <dt className="text-text-muted font-medium">Owner</dt>
          <dd className="text-text mt-1 break-words">{draft.owner_name}</dd>
        </div>
        <div>
          <dt className="text-text-muted font-medium">Access</dt>
          <dd className="text-text mt-1 capitalize">{draft.access_role}</dd>
        </div>
        <div className="col-span-2">
          <dt className="text-text-muted font-medium">Updated</dt>
          <dd className="text-text mt-1 break-words">
            {new Date(draft.updated_at).toLocaleString()}
          </dd>
        </div>
        <div className="col-span-2 min-w-0">
          <dt className="text-text-muted font-medium">Draft ID</dt>
          <dd className="mt-1 flex min-w-0 items-start gap-2">
            <code className="text-text-muted min-w-0 flex-1 font-mono text-xs break-all whitespace-normal">
              {draft.id}
            </code>
            <CopyValueButton value={draft.id} label="Copy draft ID" touchSafe />
          </dd>
        </div>
      </dl>

      <div
        role="group"
        aria-label={`Actions for draft ${draft.title}`}
        className="mt-3 flex flex-wrap gap-2"
      >
        <Button
          type="button"
          size="sm"
          variant="secondary"
          className="min-h-11 sm:min-h-0"
          onClick={() => onOpen(draft)}
        >
          <Pencil size={13} />
          Open
        </Button>
        {actionState.canShare && (
          <Button
            type="button"
            size="sm"
            variant="secondary"
            className="min-h-11 sm:min-h-0"
            onClick={() => onShare(draft)}
          >
            <Share2 size={13} />
            Share
          </Button>
        )}
        {actionState.canDelete && (
          <Button
            type="button"
            size="sm"
            variant="danger"
            className="min-h-11 sm:min-h-0"
            onClick={() => onDelete(draft)}
            aria-label={`Delete draft ${draft.title}`}
          >
            <Trash2 size={13} />
            Delete
          </Button>
        )}
      </div>
    </article>
  );
}

export function getSpaceGroupAccessActionState(group: SpaceGroupItem) {
  return {
    canOpenMembers: group.managed,
    canRemove: !group.managed,
  };
}

export function getSpaceGroupEmptyMessage(totalGroupCount: number): string {
  return totalGroupCount === 0
    ? "No groups are assigned to this space."
    : "No groups match this search.";
}

export interface SpaceGroupAccessMobileRowProps {
  group: SpaceGroupItem;
  roleDefinitions: SpaceRoleDefinition[];
  onOpenMembers: (group: SpaceGroupItem) => void;
  onEditRole: (group: SpaceGroupItem) => void;
  onRemove: (group: SpaceGroupItem) => void;
}

export function SpaceGroupAccessMobileRow({
  group,
  roleDefinitions,
  onOpenMembers,
  onEditRole,
  onRemove,
}: SpaceGroupAccessMobileRowProps) {
  const actionState = getSpaceGroupAccessActionState(group);

  return (
    <article className="min-w-0 px-3 py-3">
      <div className="flex min-w-0 items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-text text-sm font-medium break-words">{group.name}</p>
          <p className="text-text-muted mt-1 text-xs break-words">
            {group.description || "No description"}
          </p>
        </div>
        <span className="text-text-muted max-w-[45%] shrink-0 text-right text-xs break-words">
          {group.managed ? "Dedicated space group" : "Shared group"}
        </span>
      </div>

      <dl className="text-text mt-3 grid grid-cols-2 gap-3 text-xs">
        <div>
          <dt className="text-text-muted font-medium">Members</dt>
          <dd className="mt-1">{group.member_count}</dd>
        </div>
        <div>
          <dt className="text-text-muted font-medium">Role</dt>
          <dd className="mt-1 break-words">{getSpaceRoleLabel(group.role, roleDefinitions)}</dd>
        </div>
      </dl>

      <div
        role="group"
        aria-label={`Actions for ${group.name}`}
        className="mt-3 flex flex-wrap gap-2"
      >
        {actionState.canOpenMembers && (
          <Button
            type="button"
            size="sm"
            variant="secondary"
            className="min-h-11 sm:min-h-0"
            onClick={() => onOpenMembers(group)}
          >
            Members
          </Button>
        )}
        <Button
          type="button"
          size="sm"
          variant="ghost"
          className="min-h-11 sm:min-h-0"
          onClick={() => onEditRole(group)}
          aria-label={`Edit ${group.name} role`}
        >
          <Pencil size={14} />
          Edit role
        </Button>
        {actionState.canRemove && (
          <Button
            type="button"
            size="sm"
            variant="danger"
            className="min-h-11 sm:min-h-0"
            onClick={() => onRemove(group)}
            aria-label={`Remove ${group.name}`}
          >
            <UserRoundX size={14} />
            Remove
          </Button>
        )}
      </div>
    </article>
  );
}

export function getSpaceUserAccessActionState(user: EffectiveSpaceUserItem) {
  const hasEditableDirectRole = Boolean(user.direct_role && user.direct_role !== "owner");
  const canManageUser = user.direct_role !== "owner";
  return {
    canEditDirectRole: hasEditableDirectRole,
    canRevokeDirectAccess: hasEditableDirectRole,
    canToggleBlock: canManageUser,
    blockActionLabel: user.blocked ? "Unblock" : "Block",
  };
}

export interface SpaceUserAccessMobileRowProps {
  user: EffectiveSpaceUserItem;
  roleDefinitions: SpaceRoleDefinition[];
  savingAccess: boolean;
  onEditDirectRole: (user: EffectiveSpaceUserItem) => void;
  onRevokeDirectAccess: (user: EffectiveSpaceUserItem) => void;
  onToggleBlock: (user: EffectiveSpaceUserItem, blocked: boolean) => void;
}

export function SpaceUserAccessMobileRow({
  user,
  roleDefinitions,
  savingAccess,
  onEditDirectRole,
  onRevokeDirectAccess,
  onToggleBlock,
}: SpaceUserAccessMobileRowProps) {
  const actionState = getSpaceUserAccessActionState(user);

  return (
    <article className="min-w-0 px-3 py-3">
      <div className="flex min-w-0 items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="text-text text-sm font-medium break-words">{user.display_name}</p>
          <p className="text-text-muted text-xs break-all">{user.email}</p>
        </div>
        <span
          className={`max-w-[48%] min-w-0 text-right text-xs font-medium break-words ${user.blocked ? "text-danger" : "text-text"}`}
        >
          {user.blocked
            ? "Blocked"
            : user.effective_role
              ? getSpaceRoleLabel(user.effective_role, roleDefinitions)
              : "No access"}
        </span>
      </div>

      <dl className="mt-3 grid gap-2 text-xs">
        <div>
          <dt className="text-text-muted font-medium">Access sources</dt>
          <dd className="mt-1 flex flex-wrap gap-1.5">
            {user.sources.length > 0 ? (
              user.sources.map((source) => (
                <span
                  key={`${source.kind}-${source.id}`}
                  className="rounded-badge border-border bg-surface-raised text-text-muted inline-flex max-w-full flex-wrap border px-2 py-0.5"
                >
                  <span className="break-words">
                    {source.kind === "direct" ? "Direct" : source.name}
                    <span className="text-text-subtle ml-1">
                      · {getSpaceRoleLabel(source.role, roleDefinitions)}
                    </span>
                  </span>
                </span>
              ))
            ) : (
              <span className="text-text-muted">None</span>
            )}
          </dd>
        </div>
        <div>
          <dt className="text-text-muted font-medium">Override</dt>
          <dd className={user.blocked ? "text-danger mt-1" : "text-text-muted mt-1"}>
            {user.blocked ? "Explicit deny" : "None"}
          </dd>
        </div>
      </dl>

      <div
        role="group"
        aria-label={`Actions for ${user.display_name}`}
        className="mt-3 flex flex-wrap gap-2"
      >
        {actionState.canEditDirectRole && (
          <Button
            type="button"
            size="sm"
            variant="secondary"
            className="min-h-11 sm:min-h-0"
            onClick={() => onEditDirectRole(user)}
            aria-label={`Edit ${user.display_name} direct role`}
          >
            <Pencil size={14} />
            Edit role
          </Button>
        )}
        {actionState.canRevokeDirectAccess && (
          <Button
            type="button"
            size="sm"
            variant="danger"
            className="min-h-11 sm:min-h-0"
            onClick={() => onRevokeDirectAccess(user)}
            aria-label={`Revoke ${user.display_name} direct access`}
          >
            <UserRoundX size={14} />
            Revoke access
          </Button>
        )}
        {actionState.canToggleBlock && (
          <Button
            type="button"
            size="sm"
            variant={user.blocked ? "secondary" : "ghost"}
            className={`min-h-11 sm:min-h-0 ${user.blocked ? "" : "text-danger hover:text-danger"}`}
            disabled={savingAccess}
            onClick={() => onToggleBlock(user, !user.blocked)}
          >
            {user.blocked ? <ShieldCheck size={14} /> : <ShieldX size={14} />}
            {actionState.blockActionLabel}
          </Button>
        )}
      </div>
    </article>
  );
}

export function SpaceSettingsPage() {
  const { spaceId } = useParams<{ spaceId: string }>();
  const navigate = useNavigate();
  const location = useLocation();
  const localMode = getCurrentLaunchMode() === "local";
  const requestedSection = new URLSearchParams(location.search).get("section");
  const [activeSection, setActiveSection] = useState<SpaceSettingsSection>(
    spaceSettingsSections.some((section) => section.id === requestedSection)
      ? (requestedSection as SpaceSettingsSection)
      : "general",
  );
  useEffect(() => {
    setActiveSection(
      spaceSettingsSections.some((section) => section.id === requestedSection)
        ? (requestedSection as SpaceSettingsSection)
        : "general",
    );
  }, [requestedSection]);
  const [space, setSpace] = useState<SpaceRecord | null>(null);
  const [localSpaces, setLocalSpaces] = useState<SpaceRecord[]>([]);
  const [nameDraft, setNameDraft] = useState("");
  const [newSpaceName, setNewSpaceName] = useState("");
  const [visibility, setVisibility] = useState("private");
  const [effectiveUsers, setEffectiveUsers] = useState<EffectiveSpaceUserItem[]>([]);
  const [groups, setGroups] = useState<SpaceGroupItem[]>([]);
  const [roleDefinitions, setRoleDefinitions] = useState<SpaceRoleDefinition[]>(
    DEFAULT_SPACE_ROLE_DEFINITIONS,
  );
  const [userQuery, setUserQuery] = useState("");
  const [groupQuery, setGroupQuery] = useState("");
  const [roleQuery, setRoleQuery] = useState("");
  const [roleEditor, setRoleEditor] = useState<RoleEditorState | null>(null);
  const [roleDeleteTarget, setRoleDeleteTarget] = useState<SpaceRoleDefinition | null>(null);
  const [principals, setPrincipals] = useState<SpacePrincipalCatalog>({ users: [], groups: [] });
  const [accessDialog, setAccessDialog] = useState<AccessDialogState | null>(null);
  const [removeAccessTarget, setRemoveAccessTarget] = useState<{
    kind: "user" | "group";
    id: string;
    name: string;
  } | null>(null);
  const [savingAccess, setSavingAccess] = useState(false);
  const [memberGroup, setMemberGroup] = useState<{ id: string; name: string } | null>(null);
  const [groupMembers, setGroupMembers] = useState<SpaceMemberItem[]>([]);
  const [groupMemberUserId, setGroupMemberUserId] = useState("");
  const [templates, setTemplates] = useState<TemplateRecord[]>([]);
  const [drafts, setDrafts] = useState<DraftSummaryRecord[]>([]);
  const [archivedDocuments, setArchivedDocuments] = useState<DocumentRecord[]>([]);
  const [trashedDocuments, setTrashedDocuments] = useState<DocumentRecord[]>([]);
  const [pendingDeletions,setPendingDeletions]=useState<PendingDocumentDeletion[]>([]);
  const [reconcilingDeletion,setReconcilingDeletion]=useState<string|null>(null);
  const [retentionDays, setRetentionDays] = useState(30);
  const [savingRetention, setSavingRetention] = useState(false);
  const [lifecycleQuery, setLifecycleQuery] = useState("");
  const [permanentDeleteTarget, setPermanentDeleteTarget] = useState<DocumentRecord | null>(null);
  const permanentDeleteSelection = useRef<{spaceId: string; target: string; context: DraftSaveSelection} | null>(null);
  const [draftQuery, setDraftQuery] = useState("");
  const [purgingDrafts, setPurgingDrafts] = useState(false);
  const [purgeDraftsConfirmOpen, setPurgeDraftsConfirmOpen] = useState(false);
  const [purgeDraftError, setPurgeDraftError] = useState("");
  const [sharingDraft, setSharingDraft] = useState<DraftSummaryRecord | null>(null);
  const [draftAccess, setDraftAccess] = useState<DraftAccessRecord[]>([]);
  const [draftCandidates, setDraftCandidates] = useState<DraftAccessCandidateRecord[]>([]);
  const [draftShareUserId, setDraftShareUserId] = useState("");
  const [draftShareRole, setDraftShareRole] = useState<"viewer" | "editor">("editor");
  const [draftShareError, setDraftShareError] = useState("");
  const [draftSharePending, setDraftSharePending] = useState(false);
  const draftShareGeneration = useRef(0);
  const draftListLoadGeneration = useRef(0);
  const draftListSelection = useRef<{ spaceId: string; selection: DraftSaveSelection; rows: Map<string, string> } | null>(null);
  const purgeDraftSelection = useRef<{ spaceId: string; selection: DraftSaveSelection; list: NonNullable<typeof draftListSelection.current>; rows: string } | null>(null);
  const purgeDraftPendingOwner = useRef<object | null>(null);
  const purgeDraftReloadRequired = useRef<typeof draftListSelection.current>(null);
  const sharingDraftSelection = useRef<{ spaceId: string; documentId: string; draftId: string; selection: DraftSaveSelection } | null>(null);
  const draftSharePendingOwner = useRef<object | null>(null);
  const draftOwnerState = useRef({ spaceId, sharingDraft, userId: draftShareUserId, role: draftShareRole, drafts });
  draftOwnerState.current = { spaceId, sharingDraft, userId: draftShareUserId, role: draftShareRole, drafts };
  const closeDraftSharing = () => {
    draftShareGeneration.current += 1; sharingDraftSelection.current = null;
    draftSharePendingOwner.current = null; setDraftSharePending(false);
    setSharingDraft(null); setDraftShareError("");
  };
  const loadedDraftSelection = (draft: DraftSummaryRecord): DraftSaveSelection => {
    const original = draftListSelection.current;
    if (!original || original.spaceId !== spaceId || original.rows.get(draft.id) !== JSON.stringify(draft)) throw new Error("Reload Drafts before managing this selected draft");
    return original.selection;
  };


  const [templateDeleteTarget, setTemplateDeleteTarget] = useState<TemplateRecord | null>(null);
  const [deletingTemplate, setDeletingTemplate] = useState(false);
  const [loading, setLoading] = useState(true);
  const [savingName, setSavingName] = useState(false);
  const [creatingSpace, setCreatingSpace] = useState(false);
  const [savingVisibility, setSavingVisibility] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");

  const canManageSpace =
    localMode || ["admin", "owner", "space_admin"].includes(space?.member_role ?? "");
  const canManageTemplates =
    localMode || ["admin", "owner", "space_admin", "editor"].includes(space?.member_role ?? "");
  const canManageLifecycle =
    localMode || ["admin", "owner", "space_admin", "editor"].includes(space?.member_role ?? "");
  const normalizedDraft = nameDraft.trim();
  const nameChanged = Boolean(space && normalizedDraft && normalizedDraft !== space.name);

  const load = useCallback(async () => {
    if (!spaceId) return;
    setLoading(true);
    setError("");
    const generation = ++draftListLoadGeneration.current;
    let selection: DraftSaveSelection | undefined;
    try {
      selection = captureDraftSaveSelection();
      const loadedSpace = await dataClient.getSpace(spaceId);
      verifyDraftSaveSelection(selection);
      if (draftOwnerState.current.spaceId !== spaceId || draftListLoadGeneration.current !== generation) return;
      setSpace(loadedSpace);
      setNameDraft(loadedSpace.name);
      setVisibility(loadedSpace.visibility ?? "private");
      const [templateList, draftList, archivedList, trashList, pendingList] = await Promise.all([
        dataClient.listTemplates(spaceId),
        dataClient.listDocumentDrafts(spaceId),
        dataClient.listArchivedDocuments(spaceId),
        dataClient.listTrashedDocuments(spaceId),
        dataClient.listPendingDocumentDeletions(),
      ]);
      verifyDraftSaveSelection(selection);
      if (draftOwnerState.current.spaceId !== spaceId || draftListLoadGeneration.current !== generation) return;
      draftListSelection.current = { spaceId, selection, rows: new Map(draftList.map(draft => [draft.id, JSON.stringify(draft)])) };
      setTemplates(templateList);
      setDrafts(draftList);
      setArchivedDocuments(archivedList);
      setTrashedDocuments(trashList);
      setPendingDeletions(pendingList);
      if (!localMode) {
        const retention = await api.fetch<{ trash_retention_days: number }>(
          `/spaces/${spaceId}/retention`,
        );
        setRetentionDays(retention.trash_retention_days);
      }
      if (localMode) {
        const spaces = await dataClient.listSpaces();
        const hasCurrentSpace = spaces.some((item) => item.id === loadedSpace.id);
        setLocalSpaces(sortSpacesForSettings(hasCurrentSpace ? spaces : [...spaces, loadedSpace]));
        setEffectiveUsers([]);
        setGroups([]);
        setRoleDefinitions(DEFAULT_SPACE_ROLE_DEFINITIONS);
        setPrincipals({ users: [], groups: [] });
      } else {
        const canManageLoadedSpace = ["admin", "owner", "space_admin"].includes(
          loadedSpace.member_role ?? "",
        );
        if (canManageLoadedSpace) {
          const [groupList, principalList] = await Promise.all([
            api.fetch<SpaceGroupItem[]>(`/spaces/${spaceId}/groups`),
            api.fetch<SpacePrincipalCatalog>(`/spaces/${spaceId}/principals`),
          ]);
          const [effectiveUserList, roles] = await Promise.all([
            api.fetch<EffectiveSpaceUserItem[]>(`/spaces/${spaceId}/access-users`),
            api.fetch<SpaceRoleDefinition[]>(`/spaces/${spaceId}/roles`),
          ]);
          setEffectiveUsers(effectiveUserList);
          setGroups(groupList);
          setRoleDefinitions(roles);
          setPrincipals(principalList);
        } else {
          setEffectiveUsers([]);
          setGroups([]);
          setRoleDefinitions(DEFAULT_SPACE_ROLE_DEFINITIONS);
          setPrincipals({ users: [], groups: [] });
        }
      }
    } catch (err) {
      if (draftOwnerState.current.spaceId !== spaceId || draftListLoadGeneration.current !== generation || selection && !selectionCurrent(selection)) return;
      setError(err instanceof Error ? err.message : "Unable to load space settings.");
    } finally {
      if (draftOwnerState.current.spaceId === spaceId && draftListLoadGeneration.current === generation) setLoading(false);
    }
  }, [localMode, spaceId]);

  const refreshImportedDrafts = useCallback(async () => {
    if (!spaceId) return;
    const selection = captureDraftSaveSelection();
    const generation = ++draftListLoadGeneration.current;
    const rows = await dataClient.listDocumentDrafts(spaceId);
    verifyDraftSaveSelection(selection);
    if (draftOwnerState.current.spaceId !== spaceId || draftListLoadGeneration.current !== generation) return;
    draftListSelection.current = { spaceId, selection, rows: new Map(rows.map(draft => [draft.id, JSON.stringify(draft)])) };
    setDrafts(rows);
  }, [spaceId]);

  useEffect(() => {
    void load();
  }, [load]);

  const saveName = async () => {
    if (!spaceId || !nameChanged) return;
    setSavingName(true);
    setMessage("");
    setError("");
    try {
      const updated = await dataClient.updateSpace(spaceId, { name: normalizedDraft });
      setSpace((current) => (current ? { ...current, ...updated } : updated));
      setNameDraft(updated.name);
      setMessage("Space name saved.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to save space name.");
    } finally {
      setSavingName(false);
    }
  };

  const createLocalSpace = async () => {
    if (!localMode) return;
    const normalizedName = newSpaceName.trim();
    if (!normalizedName) return;
    setCreatingSpace(true);
    setMessage("");
    setError("");
    try {
      const created = await dataClient.createSpace(normalizedName);
      setLocalSpaces((current) => sortSpacesForSettings([...current, created]));
      setNewSpaceName("");
      setMessage("Space created.");
      navigate(getSpaceSettingsRoute(created.id));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to create space.");
    } finally {
      setCreatingSpace(false);
    }
  };

  const saveVisibility = async (nextVisibility: string) => {
    if (!spaceId || localMode) return;
    setVisibility(nextVisibility);
    setSavingVisibility(true);
    setMessage("");
    setError("");
    try {
      await api.fetch(`/spaces/${spaceId}`, {
        method: "PUT",
        body: { visibility: nextVisibility },
      });
      setSpace((current) => (current ? { ...current, visibility: nextVisibility } : current));
      setMessage("Space visibility saved.");
    } catch (err) {
      setVisibility(space?.visibility ?? "private");
      setError(err instanceof Error ? err.message : "Unable to save visibility.");
    } finally {
      setSavingVisibility(false);
    }
  };

  const saveAccess = async () => {
    if (!spaceId || !accessDialog) return;
    setSavingAccess(true);
    setMessage("");
    setError("");
    try {
      const collection = accessDialog.kind === "user" ? "members" : "groups";
      const id = accessDialog.mode === "add" ? accessDialog.principalId : accessDialog.principalId;
      if (!id) return;
      const endpoint =
        accessDialog.mode === "add"
          ? `/spaces/${spaceId}/${collection}`
          : `/spaces/${spaceId}/${collection}/${id}`;
      await api.fetch(endpoint, {
        method: accessDialog.mode === "add" ? "POST" : "PUT",
        body:
          accessDialog.kind === "user"
            ? { user_id: id, role: accessDialog.role }
            : { group_id: id, role: accessDialog.role },
      });
      setAccessDialog(null);
      setMessage(accessDialog.mode === "add" ? "Space access assigned." : "Space role updated.");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to save space access.");
    } finally {
      setSavingAccess(false);
    }
  };

  const removeAccess = async () => {
    if (!spaceId || !removeAccessTarget) return;
    setSavingAccess(true);
    setMessage("");
    setError("");
    try {
      const collection = removeAccessTarget.kind === "user" ? "members" : "groups";
      await api.fetch(`/spaces/${spaceId}/${collection}/${removeAccessTarget.id}`, {
        method: "DELETE",
      });
      setRemoveAccessTarget(null);
      setMessage("Space access removed.");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to remove space access.");
    } finally {
      setSavingAccess(false);
    }
  };

  const setUserBlocked = async (user: EffectiveSpaceUserItem, blocked: boolean) => {
    if (!spaceId) return;
    setSavingAccess(true);
    setMessage("");
    setError("");
    try {
      await api.fetch(`/spaces/${spaceId}/blocks/${user.user_id}`, {
        method: blocked ? "PUT" : "DELETE",
      });
      setMessage(
        blocked
          ? `${user.display_name} is blocked from this space.`
          : `${user.display_name} can use their assigned access again.`,
      );
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to update the access override.");
    } finally {
      setSavingAccess(false);
    }
  };

  const saveRole = async () => {
    if (!spaceId || !roleEditor) return;
    setSavingAccess(true);
    setMessage("");
    setError("");
    try {
      await api.fetch(
        roleEditor.id ? `/spaces/${spaceId}/roles/${roleEditor.id}` : `/spaces/${spaceId}/roles`,
        {
          method: roleEditor.id ? "PUT" : "POST",
          body: {
            name: roleEditor.name,
            description: roleEditor.description,
            permissions: roleEditor.permissions,
          },
        },
      );
      setRoleEditor(null);
      setMessage(roleEditor.id ? "Space role updated." : "Space role created.");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to save the space role.");
    } finally {
      setSavingAccess(false);
    }
  };

  const deleteRole = async () => {
    if (!spaceId || !roleDeleteTarget) return;
    setSavingAccess(true);
    setMessage("");
    setError("");
    try {
      await api.fetch(`/spaces/${spaceId}/roles/${roleDeleteTarget.id}`, { method: "DELETE" });
      setRoleDeleteTarget(null);
      setMessage("Space role deleted.");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to delete the space role.");
    } finally {
      setSavingAccess(false);
    }
  };

  const openGroupMembers = async (group: { id: string; name: string }) => {
    if (!spaceId) return;
    setError("");
    try {
      const memberList = await api.fetch<SpaceMemberItem[]>(
        `/spaces/${spaceId}/groups/${group.id}/members`,
      );
      setGroupMembers(memberList);
      setGroupMemberUserId("");
      setMemberGroup(group);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to load group members.");
    }
  };

  const addManagedGroupMember = async () => {
    if (!spaceId || !memberGroup || !groupMemberUserId) return;
    setSavingAccess(true);
    setError("");
    try {
      await api.fetch(`/spaces/${spaceId}/groups/${memberGroup.id}/members`, {
        method: "POST",
        body: { user_id: groupMemberUserId },
      });
      setGroupMembers(
        await api.fetch<SpaceMemberItem[]>(`/spaces/${spaceId}/groups/${memberGroup.id}/members`),
      );
      setGroupMemberUserId("");
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to add group member.");
    } finally {
      setSavingAccess(false);
    }
  };

  const removeManagedGroupMember = async (userId: string) => {
    if (!spaceId || !memberGroup) return;
    setSavingAccess(true);
    setError("");
    try {
      await api.fetch(`/spaces/${spaceId}/groups/${memberGroup.id}/members/${userId}`, {
        method: "DELETE",
      });
      setGroupMembers((current) => current.filter((member) => member.user_id !== userId));
      await load();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to remove group member.");
    } finally {
      setSavingAccess(false);
    }
  };

  const [journalPreferenceBusy, setJournalPreferenceBusy] = useState(false);
  const setJournalTemplate = async (templateId: string) => {
    if (!spaceId || journalPreferenceBusy || !canManageTemplates) return;
    setJournalPreferenceBusy(true);
    setError("");
    try {
      // Clear old selections before marking the new one. Reload actual state on
      // any failed request so a partially saved preference is never claimed.
      const changes = templates.filter(
        (item) => isPreferredJournalTemplate(item) && item.id !== templateId,
      );
      const selected = templates.find((item) => item.id === templateId);
      if (selected && !isPreferredJournalTemplate(selected)) changes.push(selected);
      for (const item of changes)
        await dataClient.updateTemplate(spaceId, item.id, {
          name: item.name,
          description: item.description,
          content: item.content,
          tags: item.tags,
          properties: withJournalTemplatePreference(item.properties, item.id === templateId),
        });
      setTemplates(await dataClient.listTemplates(spaceId));
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to save Journal template.");
      setTemplates(await dataClient.listTemplates(spaceId).catch(() => templates));
    } finally {
      setJournalPreferenceBusy(false);
    }
  };

  const openTemplateEditor = (template?: TemplateRecord) => {
    if (!spaceId) return;
    navigate(getTemplateEditorPath(spaceId, template?.id));
  };

  const deleteTemplate = async () => {
    if (!spaceId || !templateDeleteTarget) return;
    setDeletingTemplate(true);
    setMessage("");
    setError("");
    try {
      await dataClient.deleteTemplate(spaceId, templateDeleteTarget.id);
      setTemplates((current) =>
        current.filter((template) => template.id !== templateDeleteTarget.id),
      );
      setMessage("Template deleted.");
      setTemplateDeleteTarget(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to delete template.");
    } finally {
      setDeletingTemplate(false);
    }
  };

  const deleteDraft = async (draft: DraftSummaryRecord) => {
    if (!spaceId || draft.access_role !== "owner") return;
    setError("");
    const originalSpace = spaceId;
    let selection: DraftSaveSelection | undefined;
    let dispatched = false;
    try {
      selection = loadedDraftSelection(draft);
      const originalDraft = JSON.stringify(draft);
      await runDraftOwnerSelection(selection, () => { dispatched = true; return dataClient.deleteDocumentDraft(originalSpace, draft.document_id, draft.id); });
      verifyDraftSaveSelection(selection);
      if (draftOwnerState.current.spaceId !== originalSpace || JSON.stringify(draftOwnerState.current.drafts.find(row => row.id === draft.id)) !== originalDraft) return;
      setDrafts((current) => current.filter((candidate) => candidate.id !== draft.id));
      setMessage("Draft deleted.");
    } catch (err) {
      if (draftOwnerState.current.spaceId !== originalSpace) return;
      if (selection && !selectionCurrent(selection)) {
        if (!dispatched) setError("Reload Drafts in the current workspace before managing the original selection.");
        return;
      }
      setError(err instanceof Error ? err.message : "Unable to delete draft.");
    }
  };

  const openPurgeDrafts = () => {
    const original = draftListSelection.current;
    try {
      if (!spaceId || !original || original.spaceId !== spaceId || drafts.length !== original.rows.size ||
          drafts.some(draft => original.rows.get(draft.id) !== JSON.stringify(draft))) throw new Error("Reload Drafts before purging this space.");
      if (purgeDraftReloadRequired.current === original) throw new Error("Reload Drafts before reviewing another purge; the previous outcome was not confirmed.");
      verifyDraftSaveSelection(original.selection);
      purgeDraftSelection.current = { spaceId, selection: original.selection, list: original, rows: JSON.stringify(drafts) };
      setError(""); setPurgeDraftError(""); setPurgeDraftsConfirmOpen(true);
    } catch (err) { setError(err instanceof Error ? err.message : "Reload Drafts before purging this space."); }
  };

  const purgeDrafts = async () => {
    const original = purgeDraftSelection.current;
    if (!original || purgeDraftPendingOwner.current || !canManageLifecycle) return;
    const targetCurrent = () => purgeDraftSelection.current === original && draftOwnerState.current.spaceId === original.spaceId &&
      draftListSelection.current === original.list && JSON.stringify(draftOwnerState.current.drafts) === original.rows;
    if (!targetCurrent()) { setPurgeDraftError("The Drafts list changed. Cancel this confirmation and reload Drafts before reviewing a purge."); return; }
    if (purgeDraftReloadRequired.current === original.list) return;
    const owner = {}; purgeDraftPendingOwner.current = owner;
    setPurgingDrafts(true); setError(""); setPurgeDraftError("");
    let dispatched = false;
    try {
      const deleted = await runDraftOwnerSelection(original.selection, () => { dispatched = true; return dataClient.purgeDocumentDrafts(original.spaceId); });
      if (!targetCurrent()) return;
      setDrafts([]);
      draftListSelection.current = { ...original.list, rows: new Map() };
      purgeDraftSelection.current = null;
      setPurgeDraftsConfirmOpen(false);
      setMessage(`${deleted} draft${deleted === 1 ? "" : "s"} purged.`);
    } catch (err) {
      if (dispatched) purgeDraftReloadRequired.current = original.list;
      if (!targetCurrent()) return;
      if (!selectionCurrent(original.selection)) {
        setPurgeDraftError("The original workspace changed. Keep the rows, cancel this confirmation and reload Drafts before reviewing a purge.");
        return;
      }
      setPurgeDraftError(err instanceof Error ? err.message : "Unable to confirm draft purge. Keep the rows and reload Drafts before reviewing another purge.");
    } finally {
      if (purgeDraftPendingOwner.current === owner) { purgeDraftPendingOwner.current = null; setPurgingDrafts(false); }
    }
  };

  useEffect(() => {
    purgeDraftSelection.current = null; purgeDraftPendingOwner.current = null;
    setPurgeDraftsConfirmOpen(false); setPurgingDrafts(false); setPurgeDraftError("");
  }, [spaceId, localMode]);

  const saveRetention = async (value: string) => {
    if (!spaceId || localMode) return;
    const days = Number(value);
    if (!Number.isInteger(days) || days < 30 || days > 120) return;
    setRetentionDays(days);
    setSavingRetention(true);
    setMessage("");
    setError("");
    try {
      await api.fetch(`/spaces/${spaceId}/retention`, {
        method: "PUT",
        body: { trash_retention_days: days },
      });
      setMessage(`Trash retention set to ${days} days.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to save trash retention.");
    } finally {
      setSavingRetention(false);
    }
  };

  const restoreLifecycleDocument = async (document: DocumentRecord, archived: boolean) => {
    if (!spaceId) return;
    setError("");
    try {
      if (archived) await dataClient.unarchiveDocument(spaceId, document.id);
      else await dataClient.restoreDocument(spaceId, document.id);
      setArchivedDocuments((current) =>
        archived ? current.filter((item) => item.id !== document.id) : current,
      );
      setTrashedDocuments((current) =>
        archived ? current : current.filter((item) => item.id !== document.id),
      );
      setMessage(`“${document.title}” restored.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to restore page.");
    }
  };

  const reconcilePendingDeletion = async (row:PendingDocumentDeletion) => {
    if(reconcilingDeletion!==null)return;
    const original=JSON.stringify(row);let selection:DraftSaveSelection|undefined;
    if(!pendingDeletions.some(r=>r.kind===row.kind&&r.id===row.id&&JSON.stringify(r)===original))return;
    setReconcilingDeletion(row.id);setError("");setMessage("");
    try{selection=captureDraftSaveSelection();verifyDraftSaveSelection(selection);const result=await dataClient.reconcilePendingDocumentDeletion(row);verifyDraftSaveSelection(selection);setMessage(result?.outcome==="physically_completed"?"Original attachment cleanup physically completed.":result?.outcome==="pending"?"Attachment cleanup is pending. Retry the retained original operation.":result?.outcome==="retained_review"?"Attachment bytes and charges remain retained for review.":"Original page deletion reconciled; attachment cleanup may still require review.");await load();}
    catch(error){setError(error instanceof Error?error.message:"Original deletion outcome remains unknown; retain evidence");try{if(!selection)throw new Error("Original review session unavailable");verifyDraftSaveSelection(selection);const rows=await dataClient.listPendingDocumentDeletions();verifyDraftSaveSelection(selection);setPendingDeletions(rows);}catch{/* Original current-session list cannot replace retained evidence after context change. */}}
    finally{setReconcilingDeletion(null);}
  };

  const permanentlyDeleteLifecycleDocument = async () => {
    if (!spaceId || !permanentDeleteTarget) return;
    const target = permanentDeleteTarget;
    const original = permanentDeleteSelection.current;
    setPermanentDeleteTarget(null);
    setError("");
    try {
      if (!original || original.spaceId !== spaceId || original.target !== JSON.stringify(target)) throw new Error("Original deletion selection changed. Reload Trash and review the original page.");
      verifyDraftSaveSelection(original.context);
      await dataClient.permanentlyDeleteDocument(spaceId, target.id, { created_at: target.created_at, deleted_at: target.deleted_at }, target);
      verifyDraftSaveSelection(original.context);
      setTrashedDocuments((current) => current.filter((item) => item.id !== target.id || item.created_at !== target.created_at || item.deleted_at !== target.deleted_at));
      const rows=await dataClient.listPendingDocumentDeletions();verifyDraftSaveSelection(original.context);setPendingDeletions(rows);
      setMessage(`“${target.title}” removed. Attachment cleanup may still require review.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to permanently delete page.");
      try { if(original)verifyDraftSaveSelection(original.context);const rows=await dataClient.listPendingDocumentDeletions();if(original)verifyDraftSaveSelection(original.context);setPendingDeletions(rows); } catch { /* Retain original evidence; changed context cannot publish this list. */ }
    }
  };

  const filteredArchivedDocuments = useMemo(() => {
    const query = lifecycleQuery.trim().toLowerCase();
    if (!query) return archivedDocuments;
    return archivedDocuments.filter((document) => document.title.toLowerCase().includes(query));
  }, [archivedDocuments, lifecycleQuery]);
  const filteredTrashedDocuments = useMemo(() => {
    const query = lifecycleQuery.trim().toLowerCase();
    if (!query) return trashedDocuments;
    return trashedDocuments.filter((document) => document.title.toLowerCase().includes(query));
  }, [lifecycleQuery, trashedDocuments]);

  const openDraftSharing = async (draft: DraftSummaryRecord) => {
    if (!spaceId) return;
    const originalSpace = spaceId;
    const generation = ++draftShareGeneration.current;
    draftSharePendingOwner.current = null; setDraftSharePending(false);
    setError(""); setDraftShareError("");
    let selection: DraftSaveSelection | undefined;
    let dispatched = false;
    try {
      selection = loadedDraftSelection(draft);
      const [access, candidates] = await runDraftOwnerSelection(selection, () => {
        dispatched = true;
        return Promise.all([
          dataClient.listDocumentDraftAccess(originalSpace, draft.document_id, draft.id),
          dataClient.listDocumentDraftAccessCandidates(originalSpace, draft.document_id, draft.id),
        ]);
      });
      if (draftOwnerState.current.spaceId !== originalSpace || draftShareGeneration.current !== generation) return;
      sharingDraftSelection.current = { spaceId: originalSpace, documentId: draft.document_id, draftId: draft.id, selection };
      draftSharePendingOwner.current = null; setDraftSharePending(false);
      setDraftAccess(access); setDraftCandidates(candidates); setDraftShareUserId(""); setSharingDraft(draft);
    } catch (err) {
      if (draftOwnerState.current.spaceId !== originalSpace || draftShareGeneration.current !== generation) return;
      if (selection && !selectionCurrent(selection) && dispatched) return;
      const message = selection && !selectionCurrent(selection) ? "Reload Drafts in the current workspace before managing the original selection." : err instanceof Error ? err.message : "Unable to load draft sharing.";
      setError(message); setDraftShareError(message);
    }
  };

  const grantDraftAccess = async () => {
    if (!spaceId || !sharingDraft || !draftShareUserId || draftSharePendingOwner.current) return;
    const originalSpace = spaceId, originalDraft = sharingDraft, originalUser = draftShareUserId, originalRole = draftShareRole;
    const accepted = sharingDraftSelection.current;
    const originalSelection = accepted?.spaceId === originalSpace && accepted.documentId === originalDraft.document_id && accepted.draftId === originalDraft.id ? accepted.selection : null;
    const generation = draftShareGeneration.current;
    const requestOwner = {};
    draftSharePendingOwner.current = requestOwner;
    const targetCurrent = () => draftOwnerState.current.spaceId === originalSpace && draftOwnerState.current.sharingDraft?.id === originalDraft.id && draftShareGeneration.current === generation;
    const inputCurrent = () => targetCurrent() && draftOwnerState.current.userId === originalUser && draftOwnerState.current.role === originalRole;
    setDraftSharePending(true); setDraftShareError(""); setMessage("");
    let committed = false, dispatched = false;
    try {
      await runDraftOwnerSelection(originalSelection, () => { dispatched = true; return dataClient.grantDocumentDraftAccess(originalSpace, originalDraft.document_id, originalDraft.id, originalUser, originalRole); });
      committed = true;
      if (!inputCurrent()) return;
      const [access, candidates] = await runDraftOwnerSelection(originalSelection, () => Promise.all([
        dataClient.listDocumentDraftAccess(originalSpace, originalDraft.document_id, originalDraft.id),
        dataClient.listDocumentDraftAccessCandidates(originalSpace, originalDraft.document_id, originalDraft.id),
      ]));
      if (!inputCurrent()) return;
      setDraftAccess(access); setDraftCandidates(candidates); setDraftShareUserId(""); setMessage("Draft access saved.");
    } catch (err) {
      if (!targetCurrent()) return;
      if (!selectionCurrent(originalSelection)) {
        if (!dispatched) setDraftShareError("Reload Drafts in the current workspace before managing the original selection.");
        return;
      }
      setDraftShareError(committed ? "Draft access was saved, but participants could not be refreshed. Your selected person and role remain open; inspect again before another change." : err instanceof Error ? err.message : "Draft access could not be confirmed. Your selection remains open.");
    } finally {
      if (draftSharePendingOwner.current === requestOwner) { draftSharePendingOwner.current = null; setDraftSharePending(false); }
    }
  };

  const revokeDraftAccess = async (userId: string) => {
    if (!spaceId || !sharingDraft || draftSharePendingOwner.current) return;
    const originalSpace = spaceId, originalDraft = sharingDraft, generation = draftShareGeneration.current;
    const accepted = sharingDraftSelection.current;
    const originalSelection = accepted?.spaceId === originalSpace && accepted.documentId === originalDraft.document_id && accepted.draftId === originalDraft.id ? accepted.selection : null;
    const requestOwner = {}; draftSharePendingOwner.current = requestOwner;
    const targetCurrent = () => draftOwnerState.current.spaceId === originalSpace && draftOwnerState.current.sharingDraft?.id === originalDraft.id && draftShareGeneration.current === generation;
    setDraftSharePending(true); setDraftShareError(""); setMessage("");
    let dispatched = false;
    try {
      await runDraftOwnerSelection(originalSelection, () => { dispatched = true; return dataClient.revokeDocumentDraftAccess(originalSpace, originalDraft.document_id, originalDraft.id, userId); });
      if (!targetCurrent()) return;
      setDraftAccess((current) => current.filter((access) => access.user_id !== userId)); setMessage("Draft access removed.");
    } catch (err) {
      if (!targetCurrent()) return;
      if (!selectionCurrent(originalSelection)) {
        if (!dispatched) setDraftShareError("Reload Drafts in the current workspace before managing the original selection.");
        return;
      }
      setDraftShareError(err instanceof Error ? err.message : "Draft removal could not be confirmed. Existing participants remain visible.");
    } finally {
      if (draftSharePendingOwner.current === requestOwner) { draftSharePendingOwner.current = null; setDraftSharePending(false); }
    }
  };

  const accessSummary = useMemo(() => {
    if (localMode) return "Local vault access";
    return `${effectiveUsers.length} user${effectiveUsers.length === 1 ? "" : "s"} · ${groups.length} group${groups.length === 1 ? "" : "s"}`;
  }, [effectiveUsers.length, groups.length, localMode]);
  const filteredUsers = useMemo(() => {
    const query = userQuery.trim().toLowerCase();
    if (!query) return effectiveUsers;
    return effectiveUsers.filter((user) =>
      [
        user.display_name,
        user.email,
        user.effective_role ?? "",
        ...user.sources.map((source) => source.name),
      ].some((value) => value.toLowerCase().includes(query)),
    );
  }, [effectiveUsers, userQuery]);
  const filteredGroups = useMemo(() => {
    const query = groupQuery.trim().toLowerCase();
    if (!query) return groups;
    return groups.filter((group) =>
      [group.name, group.description ?? "", group.role].some((value) =>
        value.toLowerCase().includes(query),
      ),
    );
  }, [groupQuery, groups]);
  const filteredRoles = useMemo(() => {
    const query = roleQuery.trim().toLowerCase();
    if (!query) return roleDefinitions;
    return roleDefinitions.filter((role) =>
      [role.name, role.description].some((value) => value.toLowerCase().includes(query)),
    );
  }, [roleDefinitions, roleQuery]);
  const filteredDrafts = useMemo(() => {
    const query = draftQuery.trim().toLowerCase();
    if (!query) return drafts;
    return drafts.filter((draft) =>
      [draft.title, draft.document_title, draft.owner_name, draft.access_role].some((value) =>
        value.toLowerCase().includes(query),
      ),
    );
  }, [draftQuery, drafts]);
  const permittedSettingsSections = useMemo(() => {
    const available = localMode
      ? spaceSettingsSections.filter(
          (section) => !["users", "groups", "roles"].includes(section.id),
        )
      : spaceSettingsSections.filter(
          (section) =>
            section.id !== "local-spaces" &&
            (!["users", "groups", "roles"].includes(section.id) || canManageSpace),
        );
    return available.filter((section) => section.id !== "import" || canManageTemplates);
  }, [canManageSpace, canManageTemplates, localMode]);

  useEffect(() => {
    if (loading) return;
    if (!permittedSettingsSections.some((section) => section.id === activeSection)) {
      setActiveSection("general");
    }
  }, [activeSection, loading, permittedSettingsSections]);

  if (loading) {
    return (
      <div className="text-text-muted flex h-full items-center justify-center text-sm">
        Loading space settings...
      </div>
    );
  }

  return (
    <SettingsWorkspace
      title="Space settings"
      description={`Manage ${space?.name ?? "this space"}, its access, and reusable content.`}
      sections={permittedSettingsSections}
      activeSection={activeSection}
      onSectionChange={(section) => {
        const search = new URLSearchParams(location.search);
        search.set("section", section);
        navigate({ pathname: location.pathname, search: search.toString() });
      }}
    >
      {accessDialog && (
        <DialogShell
          labelledBy="space-access-dialog-title"
          maxWidth="md"
          onClose={() => {
            if (!savingAccess) setAccessDialog(null);
          }}
        >
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void saveAccess();
            }}
          >
            <div className="border-border border-b px-4 py-3">
              <h2 id="space-access-dialog-title" className="text-text text-base font-semibold">
                {accessDialog.mode === "add"
                  ? `Add ${accessDialog.kind}`
                  : `Edit ${accessDialog.principalName}`}
              </h2>
              <p className="text-text-muted mt-1 text-xs">
                Assign one built-in role. Permissions apply immediately across web and apps.
              </p>
            </div>
            <div className="flex flex-col gap-4 p-4">
              {accessDialog.mode === "add" && (
                <div>
                  <label
                    htmlFor="space-access-principal"
                    className="text-text mb-1 block text-sm font-medium"
                  >
                    {accessDialog.kind === "user" ? "User" : "Group"}
                  </label>
                  <MenuSelect
                    id="space-access-principal"
                    value={accessDialog.principalId}
                    onChange={(value) =>
                      setAccessDialog((current) =>
                        current && current.mode === "add"
                          ? { ...current, principalId: value }
                          : current,
                      )
                    }
                    aria-label={accessDialog.kind === "user" ? "User to add" : "Group to assign"}
                    options={[
                      {
                        value: "",
                        label: `Select ${accessDialog.kind === "user" ? "a user" : "a group"}`,
                      },
                      ...(accessDialog.kind === "user" ? principals.users : principals.groups).map(
                        (principal) => ({
                          value: principal.id,
                          label: "email" in principal ? principal.email : principal.name,
                        }),
                      ),
                    ]}
                  />
                </div>
              )}
              <div>
                <label
                  htmlFor="space-access-role"
                  className="text-text mb-1 block text-sm font-medium"
                >
                  Space role
                </label>
                <SpaceRoleSelect
                  id="space-access-role"
                  value={accessDialog.role}
                  definitions={roleDefinitions}
                  onChange={(role) =>
                    setAccessDialog((current) => (current ? { ...current, role } : current))
                  }
                />
              </div>
            </div>
            <div className="border-border flex justify-end gap-2 border-t px-4 py-3">
              <Button
                type="button"
                size="sm"
                variant="ghost"
                className={RESPONSIVE_TOUCH_ACTION_CLASS}
                onClick={() => setAccessDialog(null)}
                disabled={savingAccess}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                size="sm"
                className={RESPONSIVE_TOUCH_ACTION_CLASS}
                disabled={
                  savingAccess || (accessDialog.mode === "add" && !accessDialog.principalId)
                }
              >
                {savingAccess
                  ? "Saving..."
                  : accessDialog.mode === "add"
                    ? "Assign role"
                    : "Save role"}
              </Button>
            </div>
          </form>
        </DialogShell>
      )}
      {sharingDraft && (
        <DialogShell
          labelledBy="draft-sharing-title"
          maxWidth="md"
          onClose={closeDraftSharing}
        >
          <div className="border-border border-b px-4 py-3">
            <h2 id="draft-sharing-title" className="text-text text-base font-semibold">
              Share draft
            </h2>
            <p className="text-text-muted mt-1 text-xs">
              Invite people who already have access to this space. Only you can publish.
            </p>
          </div>
          <div className="space-y-4 p-4">
            {draftShareError && <p className="text-danger text-sm" role="alert">{draftShareError}</p>}
            <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_8rem_auto]">
              <MenuSelect
                value={draftShareUserId}
                onChange={setDraftShareUserId}
                aria-label="Person to share draft with"
                options={[
                  { value: "", label: "Choose a person" },
                  ...draftCandidates.map((candidate) => ({
                    value: candidate.user_id,
                    label: `${candidate.display_name} · ${candidate.email}`,
                  })),
                ]}
              />
              <MenuSelect
                value={draftShareRole}
                onChange={(value) => setDraftShareRole(value as "viewer" | "editor")}
                aria-label="Draft access role"
                options={[
                  { value: "editor", label: "Can edit" },
                  { value: "viewer", label: "Can view" },
                ]}
              />
              <Button
                type="button"
                disabled={!draftShareUserId || draftSharePending}
                onClick={() => void grantDraftAccess()}
              >
                Share
              </Button>
            </div>
            <div className="rounded-card border-border divide-border divide-y border">
              {draftAccess.length === 0 ? (
                <p className="text-text-muted px-3 py-4 text-sm">Only you can access this draft.</p>
              ) : (
                draftAccess.map((access) => (
                  <div
                    key={access.user_id}
                    className="flex items-center justify-between gap-3 px-3 py-2"
                  >
                    <div className="min-w-0">
                      <p className="text-text truncate text-sm font-medium">
                        {access.display_name}
                      </p>
                      <p className="text-text-muted truncate text-xs">
                        {access.email} · {access.role}
                      </p>
                    </div>
                    <Button
                      type="button"
                      size="sm"
                      variant="danger"
                      disabled={draftSharePending}
                      onClick={() => void revokeDraftAccess(access.user_id)}
                    >
                      Remove
                    </Button>
                  </div>
                ))
              )}
            </div>
          </div>
          <div className="border-border flex justify-end border-t px-4 py-3">
            <Button type="button" variant="secondary" onClick={closeDraftSharing}>
              Done
            </Button>
          </div>
        </DialogShell>
      )}

      {removeAccessTarget && (
        <ConfirmDialog
          labelledBy="remove-space-access-title"
          title="Remove space access"
          message={`Remove ${removeAccessTarget.name} from this space? Private spaces will no longer be visible to them unless another group grants access.`}
          confirmLabel={savingAccess ? "Removing..." : "Remove access"}
          destructive
          onCancel={() => setRemoveAccessTarget(null)}
          onConfirm={() => void removeAccess()}
        />
      )}

      {memberGroup && (
        <DialogShell
          labelledBy="space-group-members-title"
          maxWidth="md"
          onClose={() => {
            if (!savingAccess) setMemberGroup(null);
          }}
        >
          <div className="border-border border-b px-4 py-3">
            <h2 id="space-group-members-title" className="text-text text-base font-semibold">
              {memberGroup.name}
            </h2>
            <p className="text-text-muted mt-1 text-xs">
              Members inherit this group&apos;s role in the current space.
            </p>
          </div>
          <div className="flex flex-col gap-4 p-4">
            <div className="flex flex-col gap-2 sm:flex-row">
              <MenuSelect
                value={groupMemberUserId}
                onChange={setGroupMemberUserId}
                aria-label="User to add to dedicated space group"
                className="min-w-0 flex-1"
                options={[
                  { value: "", label: "Select a user" },
                  ...principals.users
                    .filter((user) => !groupMembers.some((member) => member.user_id === user.id))
                    .map((user) => ({ value: user.id, label: user.email })),
                ]}
              />
              <Button
                type="button"
                size="sm"
                className={RESPONSIVE_TOUCH_ACTION_CLASS}
                disabled={!groupMemberUserId || savingAccess}
                onClick={() => void addManagedGroupMember()}
              >
                <UserPlus size={14} />
                Add member
              </Button>
            </div>
            <div className="rounded-card border-border divide-border divide-y border">
              {groupMembers.length === 0 ? (
                <p className="text-text-muted px-3 py-3 text-sm">No group members.</p>
              ) : (
                groupMembers.map((member) => (
                  <div key={member.user_id} className="flex items-center gap-3 px-3 py-2">
                    <div className="min-w-0 flex-1">
                      <p className="text-text truncate text-sm">{member.display_name}</p>
                      <p className="text-text-muted truncate text-xs">{member.email}</p>
                    </div>
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      className="text-danger hover:text-danger min-h-11 min-w-11 px-0 sm:min-h-0 sm:min-w-0 sm:px-2"
                      onClick={() => void removeManagedGroupMember(member.user_id)}
                      disabled={savingAccess}
                      aria-label={`Remove ${member.display_name} from ${memberGroup.name}`}
                    >
                      <UserRoundX size={14} />
                    </Button>
                  </div>
                ))
              )}
            </div>
          </div>
          <div className="border-border flex justify-end border-t px-4 py-3">
            <Button
              type="button"
              size="sm"
              variant="secondary"
              className={RESPONSIVE_TOUCH_ACTION_CLASS}
              onClick={() => setMemberGroup(null)}
              disabled={savingAccess}
            >
              Done
            </Button>
          </div>
        </DialogShell>
      )}

      {roleEditor && (
        <DialogShell
          labelledBy="space-role-editor-title"
          maxWidth="md"
          onClose={() => {
            if (!savingAccess) setRoleEditor(null);
          }}
        >
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void saveRole();
            }}
          >
            <div className="border-border border-b px-4 py-3">
              <h2 id="space-role-editor-title" className="text-text text-base font-semibold">
                {roleEditor.id ? "Edit space role" : "Create space role"}
              </h2>
              <p className="text-text-muted mt-1 text-xs">
                Space-specific roles can be assigned to direct users or groups.
              </p>
            </div>
            <div className="space-y-4 p-4">
              <label className="block">
                <span className="text-text mb-1 block text-sm font-medium">Role name</span>
                <Input
                  value={roleEditor.name}
                  onChange={(event) =>
                    setRoleEditor((current) =>
                      current ? { ...current, name: event.target.value } : current,
                    )
                  }
                  placeholder="Content reviewer"
                  maxLength={80}
                  autoFocus
                />
              </label>
              <label className="block">
                <span className="text-text mb-1 block text-sm font-medium">Description</span>
                <Input
                  value={roleEditor.description}
                  onChange={(event) =>
                    setRoleEditor((current) =>
                      current ? { ...current, description: event.target.value } : current,
                    )
                  }
                  placeholder="Explain what this role is for"
                  maxLength={240}
                />
              </label>
              <fieldset className="rounded-card border-border divide-border divide-y border">
                <legend className="sr-only">Role permissions</legend>
                {(Object.keys(SPACE_PERMISSION_OPTIONS) as SpacePermissionId[]).map(
                  (permission) => {
                    const checked = roleEditor.permissions.includes(permission);
                    const locked = permission === "view";
                    return (
                      <label
                        key={permission}
                        className="flex min-h-11 items-center justify-between gap-3 px-3 py-2"
                      >
                        <span>
                          <span className="text-text block text-sm font-medium">
                            {SPACE_PERMISSION_OPTIONS[permission].label}
                          </span>
                          <span className="text-text-muted block text-xs">
                            {SPACE_PERMISSION_OPTIONS[permission].description}
                          </span>
                        </span>
                        <input
                          type="checkbox"
                          checked={checked}
                          disabled={locked}
                          onChange={(event) =>
                            setRoleEditor((current) => {
                              if (!current) return current;
                              const permissions = event.target.checked
                                ? [...current.permissions, permission]
                                : current.permissions.filter((item) => item !== permission);
                              return { ...current, permissions };
                            })
                          }
                          className="accent-primary h-4 w-4"
                        />
                      </label>
                    );
                  },
                )}
              </fieldset>
            </div>
            <div className="border-border flex justify-end gap-2 border-t px-4 py-3">
              <Button
                type="button"
                size="sm"
                variant="secondary"
                onClick={() => setRoleEditor(null)}
                disabled={savingAccess}
              >
                Cancel
              </Button>
              <Button type="submit" size="sm" disabled={savingAccess || !roleEditor.name.trim()}>
                {savingAccess ? "Saving..." : roleEditor.id ? "Save role" : "Create role"}
              </Button>
            </div>
          </form>
        </DialogShell>
      )}

      {roleDeleteTarget && (
        <ConfirmDialog
          labelledBy="delete-space-role-title"
          title="Delete space role?"
          message={`Delete “${roleDeleteTarget.name}”? Reassign every user and group using it first.`}
          confirmLabel={savingAccess ? "Deleting..." : "Delete role"}
          destructive
          onCancel={() => !savingAccess && setRoleDeleteTarget(null)}
          onConfirm={() => void deleteRole()}
        />
      )}

      <div className="mx-auto max-w-5xl">
        <div className="mb-4 flex items-center justify-between gap-3">
          <div>
            <h2 className="text-text text-lg font-semibold">
              {permittedSettingsSections.find((section) => section.id === activeSection)?.label}
            </h2>
            <p className="text-text-muted mt-1 text-sm">{space?.name ?? "Space"}</p>
          </div>
          <Button type="button" size="sm" variant="secondary" onClick={() => void load()}>
            <RefreshCw size={14} />
            Refresh
          </Button>
        </div>

        {message && (
          <div className="rounded-input border-success/30 bg-success/10 text-success mb-4 border px-3 py-2 text-sm">
            {message}
          </div>
        )}
        {error && (
          <div className="rounded-input border-danger/30 bg-danger/10 text-danger mb-4 border px-3 py-2 text-sm">
            {error}
          </div>
        )}

        {activeSection === "general" && (
          <div className={SPACE_SETTINGS_GENERAL_LAYOUT_CLASS}>
            <section className="p-4">
              <div>
                <h2 className="text-text text-base font-semibold">Identity</h2>
                <p className="text-text-muted text-xs">
                  Rename this space without changing site-wide administration.
                </p>
              </div>
              <div className="mt-4 max-w-2xl space-y-3">
                <label htmlFor="space-name" className="text-text block text-sm font-medium">
                  Space name
                </label>
                <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
                  <Input
                    id="space-name"
                    value={nameDraft}
                    onChange={(event) => setNameDraft(event.target.value)}
                    disabled={!canManageSpace || savingName}
                    aria-label="Space name"
                  />
                  <Button
                    type="button"
                    size="sm"
                    onClick={() => void saveName()}
                    disabled={!canManageSpace || !nameChanged || savingName}
                  >
                    Save name
                  </Button>
                </div>
                {space?.slug && <p className="text-text-muted text-xs">Slug: {space.slug}</p>}
              </div>
            </section>

            <section className="p-4">
              <div>
                <h2 className="text-text text-base font-semibold">Access</h2>
                <p className="text-text-muted text-xs">{accessSummary}</p>
              </div>
              <div className="mt-4 max-w-2xl space-y-3">
                <div>
                  <label
                    htmlFor="space-visibility"
                    className="text-text mb-1 block text-sm font-medium"
                  >
                    Visibility
                  </label>
                  <MenuSelect
                    id="space-visibility"
                    value={visibility}
                    onChange={(value) => void saveVisibility(value)}
                    disabled={localMode || !canManageSpace || savingVisibility}
                    aria-label="Space visibility"
                    options={visibilityOptions}
                  />
                  {!localMode && (
                    <p className="text-text-muted mt-2 text-xs leading-5">
                      {visibility === "private"
                        ? "Only directly assigned users and assigned groups can open this space."
                        : visibility === "internal"
                          ? "Everyone signed in can view. Assigned users and groups receive their configured role."
                          : "Published space entry points may be visible publicly; editing still requires an assigned role."}
                    </p>
                  )}
                </div>
                {localMode && (
                  <p className="text-text-muted text-sm leading-6">
                    This standalone space is stored in the local vault. Server users and groups are
                    not available here.
                  </p>
                )}
              </div>
            </section>
          </div>
        )}

        {!localMode && activeSection === "users" && (
          <section className="rounded-card border-border bg-surface min-w-0 overflow-hidden border">
            <AccessSectionHeader
              icon={<UserPlus size={16} />}
              title="Users"
              description="Effective access includes direct assignments and every assigned group."
              action={
                <Button
                  type="button"
                  size="sm"
                  className={RESPONSIVE_TOUCH_ACTION_CLASS}
                  onClick={() =>
                    setAccessDialog({
                      kind: "user",
                      mode: "add",
                      principalId: "",
                      role: "viewer",
                    })
                  }
                >
                  <UserPlus size={14} />
                  Add user
                </Button>
              }
            />
            <AccessSearch
              value={userQuery}
              onChange={setUserQuery}
              placeholder="Search users, roles, or access sources"
              label="Search space users"
              count={filteredUsers.length}
            />
            <div className={SPACE_USERS_DESKTOP_TABLE_CLASS}>
              <table className="w-full min-w-[52rem] text-left text-sm">
                <thead>
                  <tr className="border-border bg-surface-raised text-text-muted border-y text-xs">
                    <th className="px-4 py-2.5 font-medium">User</th>
                    <th className="px-4 py-2.5 font-medium">Effective role</th>
                    <th className="px-4 py-2.5 font-medium">Access source</th>
                    <th className="px-4 py-2.5 font-medium">Override</th>
                    <th className="px-4 py-2.5 text-right font-medium">Actions</th>
                  </tr>
                </thead>
                <tbody className="divide-border divide-y">
                  {filteredUsers.length === 0 ? (
                    <tr>
                      <td colSpan={5} className="text-text-muted px-4 py-10 text-center">
                        {effectiveUsers.length === 0
                          ? "No users have access to this space."
                          : "No users match this search."}
                      </td>
                    </tr>
                  ) : (
                    filteredUsers.map((user) => {
                      const actionState = getSpaceUserAccessActionState(user);
                      return (
                        <tr key={user.user_id} className="hover:bg-surface-raised/60">
                          <td className="px-4 py-3">
                            <span className="text-text block font-medium">{user.display_name}</span>
                            <span className="text-text-muted block text-xs">{user.email}</span>
                          </td>
                          <td className="px-4 py-3">
                            <span className={user.blocked ? "text-danger" : "text-text"}>
                              {user.blocked
                                ? "Blocked"
                                : user.effective_role
                                  ? getSpaceRoleLabel(user.effective_role, roleDefinitions)
                                  : "No access"}
                            </span>
                          </td>
                          <td className="px-4 py-3">
                            <div className="flex max-w-sm flex-wrap gap-1.5">
                              {user.sources.map((source) => (
                                <span
                                  key={`${source.kind}-${source.id}`}
                                  className="rounded-badge border-border bg-surface-raised text-text-muted inline-flex border px-2 py-0.5 text-xs"
                                >
                                  {source.kind === "direct" ? "Direct" : source.name}
                                  <span className="text-text-subtle ml-1">
                                    · {getSpaceRoleLabel(source.role, roleDefinitions)}
                                  </span>
                                </span>
                              ))}
                            </div>
                          </td>
                          <td className="px-4 py-3">
                            {user.blocked ? (
                              <span className="text-danger inline-flex items-center gap-1 text-xs">
                                <ShieldX size={13} />
                                Explicit deny
                              </span>
                            ) : (
                              <span className="text-text-muted text-xs">None</span>
                            )}
                          </td>
                          <td className="px-4 py-3">
                            <div className="flex flex-wrap justify-end gap-2">
                              {actionState.canEditDirectRole && (
                                <>
                                  <Button
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    onClick={() =>
                                      setAccessDialog({
                                        kind: "user",
                                        mode: "edit",
                                        principalId: user.user_id,
                                        principalName: user.display_name,
                                        role: user.direct_role ?? "viewer",
                                      })
                                    }
                                    aria-label={`Edit ${user.display_name} direct role`}
                                  >
                                    <Pencil size={14} />
                                  </Button>
                                  <Button
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className="text-danger hover:text-danger"
                                    onClick={() =>
                                      setRemoveAccessTarget({
                                        kind: "user",
                                        id: user.user_id,
                                        name: user.display_name,
                                      })
                                    }
                                    aria-label={`Revoke ${user.display_name} direct access`}
                                  >
                                    <UserRoundX size={14} />
                                  </Button>
                                </>
                              )}
                              {actionState.canToggleBlock && (
                                <Button
                                  type="button"
                                  size="sm"
                                  variant={user.blocked ? "secondary" : "ghost"}
                                  className={user.blocked ? "" : "text-danger hover:text-danger"}
                                  disabled={savingAccess}
                                  onClick={() => void setUserBlocked(user, !user.blocked)}
                                >
                                  {user.blocked ? <ShieldCheck size={14} /> : <ShieldX size={14} />}
                                  {user.blocked ? "Unblock" : "Block"}
                                </Button>
                              )}
                            </div>
                          </td>
                        </tr>
                      );
                    })
                  )}
                </tbody>
              </table>
            </div>
            <div role="list" aria-label="Space users" className={SPACE_USERS_MOBILE_LIST_CLASS}>
              {filteredUsers.length === 0 ? (
                <p className="text-text-muted px-4 py-10 text-center text-sm">
                  {effectiveUsers.length === 0
                    ? "No users have access to this space."
                    : "No users match this search."}
                </p>
              ) : (
                filteredUsers.map((user) => (
                  <div role="listitem" key={user.user_id}>
                    <SpaceUserAccessMobileRow
                      user={user}
                      roleDefinitions={roleDefinitions}
                      savingAccess={savingAccess}
                      onEditDirectRole={(target) =>
                        setAccessDialog({
                          kind: "user",
                          mode: "edit",
                          principalId: target.user_id,
                          principalName: target.display_name,
                          role: target.direct_role ?? "viewer",
                        })
                      }
                      onRevokeDirectAccess={(target) =>
                        setRemoveAccessTarget({
                          kind: "user",
                          id: target.user_id,
                          name: target.display_name,
                        })
                      }
                      onToggleBlock={(target, blocked) => void setUserBlocked(target, blocked)}
                    />
                  </div>
                ))
              )}
            </div>
          </section>
        )}

        {!localMode && activeSection === "groups" && (
          <section className="rounded-card border-border bg-surface min-w-0 overflow-hidden border">
            <AccessSectionHeader
              icon={<UsersRound size={16} />}
              title="Groups"
              description="Assign a role once and every group member inherits it."
              action={
                <Button
                  type="button"
                  size="sm"
                  className={RESPONSIVE_TOUCH_ACTION_CLASS}
                  onClick={() =>
                    setAccessDialog({
                      kind: "group",
                      mode: "add",
                      principalId: "",
                      role: "viewer",
                    })
                  }
                >
                  <Plus size={14} />
                  Assign group
                </Button>
              }
            />
            <AccessSearch
              value={groupQuery}
              onChange={setGroupQuery}
              placeholder="Search groups or roles"
              label="Search space groups"
              count={filteredGroups.length}
            />
            <div className={SPACE_GROUPS_DESKTOP_TABLE_CLASS}>
              <table className="w-full min-w-[42rem] text-left text-sm">
                <thead>
                  <tr className="border-border bg-surface-raised text-text-muted border-y text-xs">
                    <th className="px-4 py-2.5 font-medium">Group</th>
                    <th className="px-4 py-2.5 font-medium">Members</th>
                    <th className="px-4 py-2.5 font-medium">Role</th>
                    <th className="px-4 py-2.5 font-medium">Type</th>
                    <th className="px-4 py-2.5 text-right font-medium">Actions</th>
                  </tr>
                </thead>
                <tbody className="divide-border divide-y">
                  {filteredGroups.length === 0 ? (
                    <tr>
                      <td colSpan={5} className="text-text-muted px-4 py-10 text-center">
                        {getSpaceGroupEmptyMessage(groups.length)}
                      </td>
                    </tr>
                  ) : (
                    filteredGroups.map((group) => (
                      <tr key={group.group_id} className="hover:bg-surface-raised/60">
                        <td className="px-4 py-3">
                          <span className="text-text block font-medium">{group.name}</span>
                          <span className="text-text-muted block max-w-md text-xs break-words whitespace-normal">
                            {group.description || "No description"}
                          </span>
                        </td>
                        <td className="text-text px-4 py-3">{group.member_count}</td>
                        <td className="text-text px-4 py-3">
                          {getSpaceRoleLabel(group.role, roleDefinitions)}
                        </td>
                        <td className="text-text-muted px-4 py-3 text-xs">
                          {group.managed ? "Dedicated space group" : "Shared group"}
                        </td>
                        <td className="px-4 py-3">
                          <div className="flex flex-wrap justify-end gap-2">
                            {group.managed && (
                              <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={() =>
                                  void openGroupMembers({
                                    id: group.group_id,
                                    name: group.name,
                                  })
                                }
                              >
                                Members
                              </Button>
                            )}
                            <Button
                              type="button"
                              size="sm"
                              variant="ghost"
                              onClick={() =>
                                setAccessDialog({
                                  kind: "group",
                                  mode: "edit",
                                  principalId: group.group_id,
                                  principalName: group.name,
                                  role: group.role,
                                })
                              }
                              aria-label={`Edit ${group.name} role`}
                            >
                              <Pencil size={14} />
                            </Button>
                            {!group.managed && (
                              <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                className="text-danger hover:text-danger"
                                onClick={() =>
                                  setRemoveAccessTarget({
                                    kind: "group",
                                    id: group.group_id,
                                    name: group.name,
                                  })
                                }
                                aria-label={`Remove ${group.name}`}
                              >
                                <UserRoundX size={14} />
                              </Button>
                            )}
                          </div>
                        </td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>
            <div role="list" aria-label="Space groups" className={SPACE_GROUPS_MOBILE_LIST_CLASS}>
              {filteredGroups.length === 0 ? (
                <p className="text-text-muted px-4 py-10 text-center text-sm">
                  {getSpaceGroupEmptyMessage(groups.length)}
                </p>
              ) : (
                filteredGroups.map((group) => (
                  <div role="listitem" key={group.group_id}>
                    <SpaceGroupAccessMobileRow
                      group={group}
                      roleDefinitions={roleDefinitions}
                      onOpenMembers={(target) =>
                        void openGroupMembers({ id: target.group_id, name: target.name })
                      }
                      onEditRole={(target) =>
                        setAccessDialog({
                          kind: "group",
                          mode: "edit",
                          principalId: target.group_id,
                          principalName: target.name,
                          role: target.role,
                        })
                      }
                      onRemove={(target) =>
                        setRemoveAccessTarget({
                          kind: "group",
                          id: target.group_id,
                          name: target.name,
                        })
                      }
                    />
                  </div>
                ))
              )}
            </div>
          </section>
        )}

        {!localMode && activeSection === "roles" && (
          <SpaceRolePermissions
            definitions={filteredRoles}
            totalCount={roleDefinitions.length}
            query={roleQuery}
            onQueryChange={setRoleQuery}
            onCreate={() =>
              setRoleEditor({
                name: "",
                description: "",
                permissions: ["view"],
              })
            }
            onEdit={(role) =>
              setRoleEditor({
                id: role.id,
                name: role.name,
                description: role.description,
                permissions: [...role.permissions],
              })
            }
            onDelete={setRoleDeleteTarget}
          />
        )}

        {activeSection === "drafts" && (
          <section className="rounded-card border-border bg-surface border">
            <div className="border-border flex flex-wrap items-start justify-between gap-3 border-b px-4 py-3">
              <div className="flex min-w-0 items-start gap-2">
                <FilePenLine size={16} className="text-text-muted mt-0.5 shrink-0" />
                <div>
                  <h2 className="text-text text-base font-semibold">Drafts</h2>
                  <p className="text-text-muted text-xs">
                    Private autosaved changes you own or were invited to review.
                  </p>
                </div>
              </div>
              <div className="relative w-full sm:w-64">
                <Search
                  size={14}
                  className="text-text-muted pointer-events-none absolute top-1/2 left-3 -translate-y-1/2"
                />
                <Input
                  value={draftQuery}
                  onChange={(event) => setDraftQuery(event.target.value)}
                  placeholder="Search drafts"
                  aria-label="Search drafts"
                  className="pl-9"
                />
              </div>
              {canManageLifecycle && (
                <Button
                  type="button"
                  size="sm"
                  variant="danger"
                  disabled={purgingDrafts || drafts.length === 0}
                  onClick={openPurgeDrafts}
                >
                  <Trash2 size={13} />
                  {purgingDrafts ? "Purging…" : "Purge all drafts"}
                </Button>
              )}
            </div>
            <div className="border-border bg-surface-raised text-text-muted border-b px-4 py-3 text-xs">
              Drafts are working copies with their own access.{" "}
              {localMode
                ? "Local drafts are removed after 120 days without an update. "
                : "Server drafts stay until explicitly published or deleted. "}
              Publishing updates the page; deleting a draft cannot be undone.
            </div>
            {filteredDrafts.length === 0 ? (
              <p className="text-text-muted px-4 py-6 text-sm">
                {getSpaceDraftEmptyMessage(draftQuery)}
              </p>
            ) : (
              <>
                <div className={SPACE_DRAFTS_DESKTOP_TABLE_CLASS}>
                  <table className="w-full min-w-[42rem] text-left text-sm">
                    <thead className="bg-surface-raised text-text-muted text-xs">
                      <tr>
                        <th className="px-4 py-2 font-medium">Draft</th>
                        <th className="px-4 py-2 font-medium">Owner</th>
                        <th className="px-4 py-2 font-medium">Access</th>
                        <th className="px-4 py-2 font-medium">Updated</th>
                        <th className="px-4 py-2 font-medium">Draft ID</th>
                        <th className="px-4 py-2 text-right font-medium">Actions</th>
                      </tr>
                    </thead>
                    <tbody className="divide-border divide-y">
                      {filteredDrafts.map((draft) => {
                        const actionState = getSpaceDraftActionState(draft.access_role);
                        return (
                          <tr key={draft.id}>
                            <td className="px-4 py-3">
                              <p className="text-text font-medium">{draft.title}</p>
                              <p className="text-text-muted mt-0.5 text-xs">
                                Published page: {draft.document_title}
                              </p>
                            </td>
                            <td className="text-text-muted px-4 py-3">{draft.owner_name}</td>
                            <td className="text-text-muted px-4 py-3 capitalize">
                              {draft.access_role}
                            </td>
                            <td className="text-text-muted px-4 py-3">
                              {new Date(draft.updated_at).toLocaleString()}
                            </td>
                            <td className="text-text-muted px-4 py-3 font-mono text-xs">
                              <div className="flex min-w-0 items-start gap-2">
                                <code
                                  className="min-w-0 break-all whitespace-normal"
                                  title={draft.id}
                                >
                                  {draft.id}
                                </code>
                                <CopyValueButton value={draft.id} label="Copy draft ID" />
                              </div>
                            </td>
                            <td className="px-4 py-3">
                              <div className="flex flex-wrap justify-end gap-2">
                                <Button
                                  type="button"
                                  size="sm"
                                  variant="secondary"
                                  onClick={() =>
                                    navigate(
                                      `/spaces/${encodeURIComponent(spaceId ?? "")}/documents/${encodeURIComponent(draft.document_id)}?draft=${encodeURIComponent(draft.id)}`,
                                    )
                                  }
                                >
                                  <Pencil size={13} />
                                  Open
                                </Button>
                                {actionState.canShare && (
                                  <Button
                                    type="button"
                                    size="sm"
                                    variant="secondary"
                                    onClick={() => void openDraftSharing(draft)}
                                  >
                                    <Share2 size={13} />
                                    Share
                                  </Button>
                                )}
                                {actionState.canDelete && (
                                  <Button
                                    type="button"
                                    size="sm"
                                    variant="danger"
                                    onClick={() => void deleteDraft(draft)}
                                  >
                                    <Trash2 size={13} />
                                    Delete
                                  </Button>
                                )}
                              </div>
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                </div>
                <div
                  role="list"
                  aria-label="Space drafts"
                  className={SPACE_DRAFTS_MOBILE_LIST_CLASS}
                >
                  {filteredDrafts.map((draft) => (
                    <div role="listitem" key={draft.id}>
                      <SpaceDraftMobileRow
                        draft={draft}
                        onOpen={(target) =>
                          navigate(
                            `/spaces/${encodeURIComponent(spaceId ?? "")}/documents/${encodeURIComponent(target.document_id)}?draft=${encodeURIComponent(target.id)}`,
                          )
                        }
                        onShare={(target) => void openDraftSharing(target)}
                        onDelete={(target) => void deleteDraft(target)}
                      />
                    </div>
                  ))}
                </div>
              </>
            )}
          </section>
        )}

        {(activeSection === "archived" || activeSection === "trash") && (
          <section className="rounded-card border-border bg-surface border">
            <div className="border-border flex flex-wrap items-start justify-between gap-3 border-b px-4 py-3">
              <div className="flex min-w-0 items-start gap-2">
                {activeSection === "archived" ? (
                  <Archive size={16} className="text-text-muted mt-0.5 shrink-0" />
                ) : (
                  <Trash2 size={16} className="text-text-muted mt-0.5 shrink-0" />
                )}
                <div>
                  <h2 className="text-text text-base font-semibold">
                    {activeSection === "archived" ? "Archived pages" : "Trash"}
                  </h2>
                  <p className="text-text-muted text-xs">
                    {activeSection === "archived"
                      ? "Pages hidden from normal navigation but still recoverable."
                      : "Restore a page from Trash or choose Delete permanently. Trash is not emptied automatically."}
                  </p>
                </div>
              </div>
              <div className="relative w-full sm:w-64">
                <Search
                  size={14}
                  className="text-text-muted pointer-events-none absolute top-1/2 left-3 -translate-y-1/2"
                />
                <Input
                  value={lifecycleQuery}
                  onChange={(event) => setLifecycleQuery(event.target.value)}
                  placeholder="Search pages"
                  aria-label="Search lifecycle pages"
                  className="pl-9"
                />
              </div>
            </div>
            {activeSection === "trash" && (
              <div className="border-border bg-surface-raised flex flex-wrap items-center justify-between gap-3 border-b px-4 py-3">
                <div>
                  <p className="text-text text-sm font-medium">Trash retention</p>
                  <p className="text-text-muted mt-0.5 text-xs">
                    Save a retention policy of 30–120 days. Pages are not deleted automatically; permanent
                    deletion requires a separate action.
                  </p>
                </div>
                <MenuSelect
                  id="space-trash-retention"
                  value={String(retentionDays)}
                  onChange={(value) => void saveRetention(value)}
                  disabled={localMode || !canManageSpace || savingRetention}
                  aria-label="Trash retention"
                  className="w-full sm:w-40"
                  options={[30, 60, 90, 120].map((days) => ({
                    value: String(days),
                    label: `${days} days`,
                  }))}
                />
              </div>
            )}
            {activeSection==="trash"&&<PendingDocumentDeletionReview rows={pendingDeletions} busy={reconcilingDeletion} onReconcile={row=>void reconcilePendingDeletion(row)}/>}
            {(activeSection === "archived" ? filteredArchivedDocuments : filteredTrashedDocuments)
              .length === 0 ? (
              <p className="text-text-muted px-4 py-8 text-sm">
                {lifecycleQuery
                  ? "No pages match this search."
                  : activeSection === "archived"
                    ? "No archived pages."
                    : "Trash is empty."}
              </p>
            ) : (
              <div className="divide-border divide-y">
                {(activeSection === "archived"
                  ? filteredArchivedDocuments
                  : filteredTrashedDocuments
                ).map((document) => (
                  <div
                    key={document.id}
                    className="flex flex-wrap items-center justify-between gap-3 px-4 py-3"
                  >
                    <div className="min-w-0">
                      <p className="text-text truncate text-sm font-medium">
                        {document.icon ? `${document.icon} ` : ""}
                        {document.title}
                      </p>
                      <p className="text-text-muted mt-0.5 text-xs">
                        {activeSection === "archived"
                          ? `Archived ${document.archived_at ? new Date(document.archived_at).toLocaleDateString() : ""}`
                          : `Deleted ${document.deleted_at ? new Date(document.deleted_at).toLocaleDateString() : ""}`}
                      </p>
                    </div>
                    <div className="flex shrink-0 gap-2">
                      <Button
                        type="button"
                        size="sm"
                        variant="secondary"
                        disabled={!canManageLifecycle}
                        onClick={() =>
                          void restoreLifecycleDocument(document, activeSection === "archived")
                        }
                      >
                        {activeSection === "archived" ? "Unarchive" : "Restore"}
                      </Button>
                      {activeSection === "trash" && (
                        <Button
                          type="button"
                          size="sm"
                          variant="danger"
                          disabled={!canManageLifecycle}
                          onClick={() => {
                            try {
                              if (!spaceId) return;
                              permanentDeleteSelection.current = {spaceId, target: JSON.stringify(document), context: captureDraftSaveSelection()};
                              setPermanentDeleteTarget(document);
                            } catch (error) { setError(error instanceof Error ? error.message : "Original deletion session unavailable"); }
                          }}
                        >
                          Delete permanently
                        </Button>
                      )}
                    </div>
                  </div>
                ))}
              </div>
            )}
            {localMode && activeSection === "trash" && (
              <p className="border-border text-text-muted border-t px-4 py-3 text-xs">
                Local vault pages are retained on this device. Server trash retention applies only
                to this space's server copy.
              </p>
            )}
          </section>
        )}

        {activeSection === "templates" && (
          <section className="rounded-card border-border bg-surface border">
            <div className="border-border flex flex-wrap items-start justify-between gap-3 border-b px-4 py-3">
              <div className="flex min-w-0 items-start gap-2">
                <FileText size={16} className="text-text-muted mt-0.5 shrink-0" />
                <div>
                  <h2 className="text-text text-base font-semibold">Document templates</h2>
                  <p className="text-text-muted text-xs">
                    Reusable page content, properties, and tags for this space.
                  </p>
                </div>
              </div>
              {canManageTemplates && (
                <Button
                  type="button"
                  size="sm"
                  className={RESPONSIVE_TOUCH_ACTION_CLASS}
                  onClick={() => openTemplateEditor()}
                >
                  <Plus size={14} />
                  New template
                </Button>
              )}
            </div>
            {canManageTemplates && templates.length > 0 && (
              <div className="border-border space-y-2 border-b px-4 py-3">
                <label
                  className="text-text block text-sm font-medium"
                  htmlFor="journal-template-preference"
                >
                  Daily Journal template
                </label>
                <MenuSelect
                  id="journal-template-preference"
                  aria-label="Daily Journal template"
                  value={templates.find(isPreferredJournalTemplate)?.id ?? ""}
                  onChange={(value) => void setJournalTemplate(value)}
                  disabled={journalPreferenceBusy}
                  options={[
                    { value: "", label: "Automatic (legacy name or daily tag)" },
                    ...templates.map((item) => ({ value: item.id, label: item.name })),
                  ]}
                />
                <p className="text-text-muted text-xs">
                  Choose the template for daily entries. Weekly and monthly prompt templates keep
                  their cadence. Existing entries stay unchanged.
                </p>
              </div>
            )}
            {templates.length === 0 ? (
              <p className="text-text-muted px-4 py-4 text-sm">
                No templates yet. Create one to make common pages faster to start.
              </p>
            ) : (
              <div className="divide-border divide-y">
                {templates.map((template) => (
                  <div
                    key={template.id}
                    className="flex flex-wrap items-start justify-between gap-3 px-4 py-3"
                  >
                    <div className="min-w-0">
                      <p className="text-text truncate text-sm font-medium">{template.name}</p>
                      <p className="text-text-muted mt-0.5 line-clamp-2 text-xs">
                        {template.description || "No description"}
                      </p>
                      {template.tags.length > 0 && (
                        <p className="text-text-muted mt-1 truncate text-xs">
                          Tags: {template.tags.join(", ")}
                        </p>
                      )}
                    </div>
                    {canManageTemplates && (
                      <div className="flex shrink-0 gap-2">
                        <Button
                          type="button"
                          size="sm"
                          variant="secondary"
                          className={RESPONSIVE_TOUCH_ACTION_CLASS}
                          onClick={() => openTemplateEditor(template)}
                        >
                          <Pencil size={13} />
                          Edit
                        </Button>
                        <Button
                          type="button"
                          size="sm"
                          variant="ghost"
                          className={RESPONSIVE_TOUCH_ACTION_CLASS}
                          onClick={() => setTemplateDeleteTarget(template)}
                        >
                          <Trash2 size={13} />
                          Delete
                        </Button>
                      </div>
                    )}
                  </div>
                ))}
              </div>
            )}
            {!canManageTemplates && (
              <p className="border-border text-text-muted border-t px-4 py-3 text-xs">
                Owners and editors can manage templates for this space.
              </p>
            )}
          </section>
        )}

        {activeSection === "import" && canManageTemplates && spaceId && (
          <NotionImportSection
            spaceId={spaceId}
            spaceName={space?.name ?? "this space"}
            onImported={refreshImportedDrafts}
          />
        )}

        {localMode && activeSection === "local-spaces" && (
          <section className="rounded-card border-border bg-surface border">
            <div className="border-border border-b px-4 py-3">
              <h2 className="text-text text-base font-semibold">Spaces in this vault</h2>
              <p className="text-text-muted text-xs">
                Create another local space or switch settings to an existing one.
              </p>
            </div>
            <div className="grid gap-4 p-4 lg:grid-cols-[minmax(0,1fr)_20rem]">
              <div>
                <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
                  <Input
                    value={newSpaceName}
                    onChange={(event) => setNewSpaceName(event.target.value)}
                    disabled={creatingSpace}
                    placeholder="New space name"
                    aria-label="New local space name"
                  />
                  <Button
                    type="button"
                    size="sm"
                    onClick={() => void createLocalSpace()}
                    disabled={!newSpaceName.trim() || creatingSpace}
                  >
                    <Plus size={14} />
                    Create space
                  </Button>
                </div>
              </div>
              <div className="rounded-card border-border border">
                {localSpaces.length === 0 ? (
                  <p className="text-text-muted px-3 py-3 text-sm">No local spaces found.</p>
                ) : (
                  <div className="divide-border max-h-56 divide-y overflow-y-auto">
                    {localSpaces.map((item) => {
                      const editState = getLocalSpaceEditState(item, space?.id);
                      const active = editState === "editing";
                      return (
                        <div
                          key={item.id}
                          className={`grid grid-cols-[minmax(0,1fr)_auto] items-center gap-3 px-3 py-2 ${
                            active
                              ? "bg-surface-raised text-text"
                              : "text-text-muted hover:bg-surface-raised hover:text-text"
                          }`}
                          aria-current={active ? "page" : undefined}
                        >
                          <div className="min-w-0">
                            <span className="block truncate text-sm font-medium">{item.name}</span>
                            {item.slug && (
                              <span className="text-text-muted block truncate text-xs">
                                {item.slug}
                              </span>
                            )}
                          </div>
                          {active ? (
                            <span className="text-primary text-xs font-medium">Editing</span>
                          ) : (
                            <Button
                              type="button"
                              size="sm"
                              variant="ghost"
                              onClick={() => navigate(getSpaceSettingsRoute(item.id))}
                              aria-label={`Edit ${item.name} settings`}
                            >
                              <Pencil size={13} />
                              Edit
                            </Button>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
              </div>
            </div>
          </section>
        )}
      </div>
      {templateDeleteTarget && (
        <ConfirmDialog
          labelledBy="delete-template-title"
          title="Delete template?"
          message={`Delete “${templateDeleteTarget.name}”? Existing documents created from it will not change.`}
          confirmLabel={deletingTemplate ? "Deleting..." : "Delete template"}
          destructive
          onCancel={() => !deletingTemplate && setTemplateDeleteTarget(null)}
          onConfirm={() => void deleteTemplate()}
        />
      )}
      {permanentDeleteTarget && (
        <ConfirmDialog
          labelledBy="permanent-delete-page-title"
          title="Delete page permanently?"
          message={`“${permanentDeleteTarget.title}” will be removed from Trash and cannot be restored.`}
          confirmLabel="Delete permanently"
          destructive
          onCancel={() => setPermanentDeleteTarget(null)}
          onConfirm={() => void permanentlyDeleteLifecycleDocument()}
        />
      )}
      {purgeDraftsConfirmOpen && (
        <ConfirmDialog
          labelledBy="purge-drafts-title"
          title="Purge all drafts?"
          message="Every draft in this space will be deleted and cannot be restored."
          confirmLabel={purgingDrafts ? "Purging..." : "Purge all drafts"}
          disabled={purgingDrafts || Boolean(purgeDraftError)}
          helperText={purgeDraftError || undefined}
          destructive
          onCancel={() => { if (!purgeDraftPendingOwner.current) { purgeDraftSelection.current = null; setPurgeDraftsConfirmOpen(false); } }}
          onConfirm={() => void purgeDrafts()}
        />
      )}
    </SettingsWorkspace>
  );
}

function SpaceRoleSelect({
  id,
  value,
  definitions,
  onChange,
}: {
  id?: string;
  value: string;
  definitions: SpaceRoleDefinition[];
  onChange: (role: string) => void;
}) {
  return (
    <MenuSelect
      id={id}
      value={value === "owner" ? "space_admin" : value}
      onChange={onChange}
      aria-label="Space role"
      options={definitions.map((definition) => ({
        value: definition.id,
        label: definition.name,
      }))}
    />
  );
}

function CopyValueButton({
  value,
  label,
  touchSafe = false,
}: {
  value: string;
  label: string;
  touchSafe?: boolean;
}) {
  const [copied, setCopied] = useState(false);

  const copyValue = async () => {
    if (!navigator.clipboard?.writeText) return;
    await navigator.clipboard.writeText(value);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  };

  return (
    <button
      type="button"
      className={`rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text inline-flex shrink-0 items-center justify-center border ${touchSafe ? "min-h-11 min-w-11" : "size-7"}`}
      aria-label={label}
      title={copied ? "Copied" : label}
      onClick={() => void copyValue()}
    >
      {copied ? <Check size={13} aria-hidden="true" /> : <Copy size={13} aria-hidden="true" />}
    </button>
  );
}

function AccessSectionHeader({
  icon,
  title,
  description,
  action,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
  action: React.ReactNode;
}) {
  return (
    <header className="flex flex-wrap items-start justify-between gap-3 px-4 py-3">
      <div className="flex min-w-0 items-start gap-2">
        <span className="text-text-muted mt-0.5 shrink-0" aria-hidden="true">
          {icon}
        </span>
        <div>
          <h2 className="text-text text-base font-semibold">{title}</h2>
          <p className="text-text-muted mt-0.5 text-xs">{description}</p>
        </div>
      </div>
      <div className="shrink-0">{action}</div>
    </header>
  );
}

function AccessSearch({
  value,
  onChange,
  placeholder,
  label,
  count,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  label: string;
  count: number;
}) {
  return (
    <div className="border-border flex flex-wrap items-center justify-between gap-3 border-t px-4 py-2.5">
      <div className="relative min-w-[14rem] flex-1 sm:max-w-md">
        <Search
          size={14}
          className="text-text-subtle pointer-events-none absolute top-1/2 left-3 -translate-y-1/2"
          aria-hidden="true"
        />
        <Input
          value={value}
          onChange={(event) => onChange(event.target.value)}
          placeholder={placeholder}
          aria-label={label}
          className="pl-9"
        />
      </div>
      <span className="text-text-muted text-xs">
        {count} {count === 1 ? "result" : "results"}
      </span>
    </div>
  );
}
