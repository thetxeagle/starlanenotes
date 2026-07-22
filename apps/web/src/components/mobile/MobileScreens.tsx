import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type InputHTMLAttributes,
  type ReactNode,
  type TouchEvent,
} from "react";
import { useNavigate } from "react-router-dom";
import {
  Bell,
  CalendarDays,
  ChevronRight,
  Cloud,
  Database,
  FileText,
  Folder,
  FolderPlus,
  FolderOpen,
  HardDrive,
  Info,
  KeyRound,
  Loader2,
  LogOut,
  Pencil,
  Plus,
  RefreshCw,
  RotateCcw,
  Shield,
  SlidersHorizontal,
  Star,
  Trash2,
  User,
  Wifi,
  X,
} from "lucide-react";
import { impactFeedback, notificationFeedback, selectionFeedback, vibrate } from "@tauri-apps/plugin-haptics";
import {
  dataClient,
  resetAllLocalAppData,
  type NotificationChannel,
  type NotificationPreferenceRecord,
  type NotificationRecord,
  type PasskeyRecord,
  type SearchFilters,
  type SearchResultRecord,
  type TemplateRecord,
  type UserSessionRecord,
} from "@/lib/dataClient";
import { createPasskeyCredential, isPasskeySupported } from "@/lib/passkeys";
import { ThemeModeSwitch } from "@/components/ThemeModeSwitch";
import { emitGraphDirty } from "@/lib/graphEvents";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import { isTauri } from "@/lib/api";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import {
  clearStarredDocuments,
  getStarredDocumentKey,
  loadStarredDocuments,
  readStarredDocuments,
  toggleStarredDocumentSynced,
} from "@/lib/starredDocuments";
import {
  getStorageSelection,
  clearStorageSelection,
  setStorageMode,
  setSyncProvider,
} from "@/lib/storageProviders";
import {
  canUseNativeVaultControls,
  chooseVaultFolder,
  clearVaultPathWithoutInitializing,
  createVaultFolder,
  getVaultFolderName,
  getVaultLocationStatus,
  listVaultFolders,
  moveVaultToICloud,
  revealVaultFolder,
  deleteVaultFolderAt,
  renameVaultFolderAt,
  setVaultPath,
  type VaultFolderSummary,
  type VaultLocation,
} from "@/lib/tauriVault";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { SearchInput } from "@/components/ui/search-input";
import { Textarea } from "@/components/ui/textarea";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import {
  findDailyTemplate,
  findDailyFolder,
  DAILY_FOLDER_NAME,
  formatDateStamp,
  renderDailyTemplateContent,
  renderTemplateContent,
  templatePropertiesAsRecord,
} from "@/lib/templates";
import { useAuth } from "@/hooks/useAuth";
import { useRuntimePlatform } from "@/hooks/useRuntimePlatform";
import type { RuntimePlatform } from "@/hooks/useRuntimePlatform";
import { getPersonalSpaceOwnerLabel } from "@/lib/spaceLabels";
import { recordSystemNotification } from "@/lib/systemNotifications";
import { getSwipeAction, sortRecentItems } from "./mobileRecentUtils";

interface Space {
  id: string;
  name: string;
  slug: string;
  icon?: string;
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

export const MOBILE_TREE_TOGGLE_LANE_CLASS =
  "flex h-8 w-5 shrink-0 items-center justify-center";
export const MOBILE_TREE_ICON_SIZE = 14;

interface DocItem {
  id: string;
  title: string;
  icon?: string;
  folder_id?: string;
  parent_id?: string;
  updated_at?: string;
}

interface SpaceData {
  folders: FolderItem[];
  documents: DocItem[];
  templates: TemplateRecord[];
}

export function isDailyDocumentTitle(title: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(title)) return false;
  const date = new Date(`${title}T00:00:00.000Z`);
  return !Number.isNaN(date.getTime()) && date.toISOString().slice(0, 10) === title;
}

export function getMobileDocumentPath(spaceId: string, documentId: string): string {
  return buildDocumentRoutePath(spaceId, documentId);
}

export function getMobileDocumentUrl(origin: string, spaceId: string, documentId: string): string {
  return `${origin}${getMobileDocumentPath(spaceId, documentId)}`;
}

export function getMobileTotpUserPatch(totpEnabled: boolean): { totp_enabled: boolean } {
  return { totp_enabled: totpEnabled };
}

export function isOpenableMobileTotpSetupUri(setupUrl: string): boolean {
  try {
    return new URL(setupUrl).protocol === "otpauth:";
  } catch {
    return false;
  }
}

export function getMobileJournalPath(spaceId: string): string {
  return `/spaces/${encodeURIComponent(spaceId)}/journal`;
}

export function getMobileDailyDocuments(
  documents: DocItem[],
  folders: FolderItem[],
): DocItem[] {
  const dailyFolder = findDailyFolder(folders);
  return documents
    .filter((doc) =>
      dailyFolder ? doc.folder_id === dailyFolder.id : isDailyDocumentTitle(doc.title),
    )
    .filter((doc) => isDailyDocumentTitle(doc.title))
    .sort((a, b) => b.title.localeCompare(a.title));
}

export interface ProfileSettingsVisibility {
  showDeviceStorageControls: boolean;
  showServerSyncRow: boolean;
  showServerVaultSwitchRow: boolean;
  showResetControls: boolean;
}

export interface ActiveVaultSettingsRowCopy {
  value: string;
  description?: string;
}

export function getProfileSettingsVisibility(
  runtime: RuntimePlatform,
  localMode: boolean,
  nativeVaultAvailable = runtime !== "web",
): ProfileSettingsVisibility {
  const webProfile = runtime === "web";
  const showServerModeControls = !localMode;
  return {
    showDeviceStorageControls: localMode && nativeVaultAvailable,
    showServerSyncRow: showServerModeControls,
    showServerVaultSwitchRow: showServerModeControls,
    showResetControls: !webProfile || nativeVaultAvailable,
  };
}

export function shouldShowServerVaultSwitchInAppSettings(
  visibility: Pick<ProfileSettingsVisibility, "showServerSyncRow" | "showServerVaultSwitchRow">,
): boolean {
  return visibility.showServerVaultSwitchRow || visibility.showServerSyncRow;
}

export function shouldShowMobileSiteAdministration(
  localMode: boolean,
  role: string | undefined,
): boolean {
  return !localMode && role === "admin";
}

export const SERVER_MODE_VAULT_SWITCH_ROW_COPY: Required<
  Pick<MobileSettingsRowProps, "label" | "description" | "value">
> = {
  label: "Vaults",
  description: "Create, open, or connect another vault.",
  value: "Switch",
};

export function canRefreshNativeVaultFolders(
  runtime: RuntimePlatform,
  nativeVaultAvailable = runtime !== "web",
): boolean {
  return nativeVaultAvailable || runtime === "tauri-desktop" || runtime === "tauri-mobile";
}

export function canStartMobileTotpSetup(
  totpEnabled: boolean,
  setupUrl: string,
  busyAction?: string | null,
): boolean {
  return !getMobileTotpSetupDisabledReason(totpEnabled, setupUrl, busyAction);
}

export function getMobileTotpSetupDisabledReason(
  totpEnabled: boolean,
  setupUrl: string,
  busyAction?: string | null,
): string | null {
  if (totpEnabled) return "Two-factor authentication is already enabled.";
  if (setupUrl) return "Finish verifying the current authenticator setup first.";
  if (busyAction === "totp-setup") return "Authenticator setup is already in progress.";
  if (busyAction) return "Finish the current action before setting up an authenticator app.";
  return null;
}

export function canVerifyMobileTotpSetup(
  code: string,
  busyAction?: string | null,
  recoveryCodesSaved = true,
): boolean {
  return !getMobileTotpVerifyDisabledReason(code, busyAction, recoveryCodesSaved);
}

export function getMobileTotpVerifyDisabledReason(
  code: string,
  busyAction?: string | null,
  recoveryCodesSaved = true,
): string | null {
  if (busyAction === "totp-verify") return "Verification is already in progress.";
  if (busyAction) return "Finish the current security action before verifying setup.";
  const trimmedCode = code.trim();
  if (!trimmedCode) return "Enter an authenticator code to verify setup.";
  if (!/^\d{6}$/u.test(trimmedCode)) return "Enter the 6-digit authenticator code.";
  if (!recoveryCodesSaved) return "Save your recovery codes before verifying setup.";
  return null;
}

export function canDisableMobileTotp(
  code: string,
  busyAction?: string | null,
  serverRequiresTwoFactor = false,
): boolean {
  return !getMobileTotpDisableDisabledReason(code, busyAction, serverRequiresTwoFactor);
}

export function isMobileTotpDisableCodeFormat(code: string): boolean {
  return /^\d{6}$/u.test(code) || /^\d{8}$/u.test(code) || /^\d{4}-\d{4}$/u.test(code);
}

export function getMobileTotpDisableDisabledReason(
  code: string,
  busyAction?: string | null,
  serverRequiresTwoFactor = false,
): string | null {
  if (serverRequiresTwoFactor) {
    return "This server requires 2FA, so an admin must turn off the requirement before disabling it.";
  }
  if (busyAction === "totp-disable") return "Disable request is already in progress.";
  if (busyAction) return "Finish the current security action before disabling 2FA.";
  const trimmedCode = code.trim();
  if (!trimmedCode) return "Enter an authenticator or recovery code to disable 2FA.";
  if (!isMobileTotpDisableCodeFormat(trimmedCode)) {
    return "Enter a 6-digit authenticator code or recovery code like 1234-5678.";
  }
  return null;
}

export const MOBILE_REQUIRED_2FA_ENROLLMENT_NOTICE_COPY =
  "This server requires two-factor authentication. Set up an authenticator app before relying on this account for shared vault access.";

export const MOBILE_REQUIRED_2FA_POLICY_NOTICE_CLASS =
  "rounded-card border border-warning/30 bg-warning/10 px-3 py-2 text-xs leading-5 text-warning";

export function getProfileSettingsFrameClass(
  runtime: RuntimePlatform,
  scope: "all" | "app" | "profile",
): string {
  if (runtime === "tauri-mobile") {
    return "mx-auto max-w-xl space-y-5 px-3 py-3 pb-8";
  }

  if (scope === "profile") {
    return "mx-auto max-w-3xl space-y-5 px-4 py-4 pb-8 sm:px-6 sm:py-6";
  }

  return "mx-auto max-w-xl space-y-5 px-3 py-3 pb-8";
}

export function getActiveVaultSettingsRowCopy(
  localMode: boolean,
  canRenameOrSwitch: boolean,
  vaultLabel: string,
): ActiveVaultSettingsRowCopy {
  if (localMode && canRenameOrSwitch) {
    return {
      value: "Rename / Switch",
      description: vaultLabel,
    };
  }

  return { value: vaultLabel };
}

export function normalizeMobileServerVaultUrl(value: string): string | null {
  const trimmed = value.trim().replace(/\/+$/, "");
  if (!trimmed) return null;

  try {
    return new URL(trimmed).toString().replace(/\/+$/, "");
  } catch {
    return null;
  }
}

export function getMobilePasskeyDeleteConfirmationMessage(name: string): string {
  const trimmed = name.trim();
  return trimmed
    ? `Delete passkey "${trimmed}"? You can add it again later from this device.`
    : "Delete this passkey? You can add it again later from this device.";
}

export function getMobilePasskeyAddDisabledReason(
  name: string,
  busyAction?: string | null,
): string | null {
  if (busyAction === "passkey-add") return "Passkey setup is already in progress.";
  if (busyAction) return "Finish the current action before adding a passkey.";
  const trimmedName = name.trim();
  if (!trimmedName) return "Name this passkey before adding it.";
  if (trimmedName.length > 80) return "Passkey names can be up to 80 characters.";
  return null;
}

export function getMobilePasskeyDeleteDisabledReason(
  passkeyId: string,
  busyAction?: string | null,
): string | null {
  if (busyAction === `passkey-delete-${passkeyId}`) {
    return "Passkey deletion is already in progress.";
  }
  if (busyAction) return "Finish the current action before deleting a passkey.";
  return null;
}

export function getMobilePasskeyDeleteHelpId(passkeyId: string): string {
  const safeId = passkeyId.trim().replace(/[^a-zA-Z0-9_-]+/g, "-").replace(/^-+|-+$/g, "");
  return `mobile-passkey-delete-${safeId || "passkey"}-help`;
}

export function getMobilePasskeyDateLabel(passkey: Pick<PasskeyRecord, "created_at" | "last_used_at">): string {
  const dateValue = passkey.last_used_at || passkey.created_at;
  const parsed = new Date(dateValue);
  const dateLabel = Number.isNaN(parsed.getTime()) ? "Unknown date" : parsed.toLocaleDateString();
  return passkey.last_used_at ? `Last used ${dateLabel}` : `Added ${dateLabel}`;
}

export function ProfilePasskeyUnavailableNotice() {
  return (
    <p
      role="status"
      className="rounded-card border border-border bg-surface-raised px-3 py-2 text-xs leading-5 text-text-muted"
    >
      Passkeys are unavailable in this browser or webview. Use password sign-in with two-factor
      authentication on this device.
    </p>
  );
}

export interface MobilePasskeyRowProps {
  passkey: PasskeyRecord;
  busyAction: string | null;
  onDelete: (passkey: PasskeyRecord) => void;
}

export function MobilePasskeyRow({ passkey, busyAction, onDelete }: MobilePasskeyRowProps) {
  const deleteDisabledReason = getMobilePasskeyDeleteDisabledReason(passkey.id, busyAction);
  const helpId = getMobilePasskeyDeleteHelpId(passkey.id);
  const dateLabel = getMobilePasskeyDateLabel(passkey);

  return (
    <div className="grid gap-2 rounded-card border border-border bg-bg px-3 py-2">
      <div className="flex min-h-[48px] items-center gap-3">
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm text-text">{passkey.name}</span>
          <span className="block truncate text-xs text-text-muted">{dateLabel}</span>
        </span>
        <button
          type="button"
          disabled={Boolean(deleteDisabledReason)}
          aria-describedby={deleteDisabledReason ? helpId : undefined}
          onClick={() => onDelete(passkey)}
          className="h-9 shrink-0 rounded-button border border-danger px-2 text-xs text-danger transition-colors duration-100 active:bg-danger/10 disabled:opacity-50"
        >
          Delete
        </button>
      </div>
      {deleteDisabledReason && (
        <p id={helpId} role="status" className="text-xs leading-5 text-text-muted">
          {deleteDisabledReason}
        </p>
      )}
    </div>
  );
}

export function getMobileSessionRevokeConfirmationMessage(deviceLabel: string): string {
  const trimmed = deviceLabel.trim();
  return trimmed
    ? `Revoke the session for "${trimmed}"? That device will need to sign in again.`
    : "Revoke this session? That device will need to sign in again.";
}

export function getMobileTotpDisableConfirmationMessage(): string {
  return "Disable two-factor authentication? Password sign-in will no longer require an authenticator or recovery code.";
}

export function getMobileVaultTrashConfirmationMessage(name: string): string {
  const trimmed = name.trim();
  return trimmed
    ? `Move "${trimmed}" to trash? Vault files are not permanently deleted.`
    : "Move this vault to trash? Vault files are not permanently deleted.";
}

export function getMobileResetAppConfirmationMessage(): string {
  return "Reset Starlane Notes on this device? This removes local notes and sign-in data.";
}

export function formatMobileActionError(err: unknown, fallback = "Action failed."): string {
  if (err instanceof Error && err.message.trim()) {
    return err.message;
  }

  if (typeof err === "string" && err.trim()) {
    return err;
  }

  if (err && typeof err === "object") {
    const payload = err as { message?: unknown; error?: unknown; code?: unknown };

    if (typeof payload.message === "string" && payload.message.trim()) {
      return payload.message;
    }

    if (typeof payload.error === "string" && payload.error.trim()) {
      return payload.error;
    }

    if (typeof payload.code === "string" && payload.code.trim()) {
      return `${fallback} (${payload.code})`;
    }
  }

  return fallback;
}

export function shouldShowProfileOpenFolderRow(
  canRevealVaultFolder: boolean,
  vaultLocation?: Pick<VaultLocation, "path"> | null,
): boolean {
  return Boolean(canRevealVaultFolder && vaultLocation?.path);
}

export function shouldShowProfileUseAppFolderRow(localMode: boolean): boolean {
  return localMode;
}

export function shouldShowMobileSpaceSwitcher(spaceCount: number): boolean {
  return spaceCount > 1;
}

export function shouldShowMobileCreateSpace(localMode: boolean): boolean {
  return localMode;
}

export function getMobilePrimaryCreateActions(): string[] {
  return ["New Doc", "New Folder"];
}

export function shouldShowMobileNotifications(localMode: boolean): boolean {
  return !localMode;
}

export function mergeMobileNotificationPages(
  current: NotificationRecord[],
  nextPage: NotificationRecord[],
): NotificationRecord[] {
  return [
    ...current,
    ...nextPage.filter((item) => !current.some((existing) => existing.id === item.id)),
  ];
}

function spaceStorageKey() {
  return "mobile_active_space_id";
}

function formatRelativeDate(value?: string): string {
  if (!value) return "Unknown time";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "Unknown time";
  return date.toLocaleString();
}

type MobileHapticKind = "tap" | "action" | "confirm" | "error";

function fallbackHapticPattern(kind: MobileHapticKind): number | number[] {
  switch (kind) {
    case "action":
      return [12, 16, 10];
    case "confirm":
      return [12, 20, 12];
    case "error":
      return [30, 20, 30];
    case "tap":
    default:
      return 10;
  }
}

async function triggerNativeHaptic(kind: MobileHapticKind): Promise<boolean> {
  try {
    switch (kind) {
      case "action": {
        const result = await impactFeedback("light");
        return result.status === "ok";
      }
      case "confirm": {
        const result = await notificationFeedback("success");
        return result.status === "ok";
      }
      case "error": {
        const result = await notificationFeedback("error");
        return result.status === "ok";
      }
      case "tap":
      default: {
        const result = await selectionFeedback();
        return result.status === "ok";
      }
    }
  } catch {
    return false;
  }
}

function triggerHaptic(kind: MobileHapticKind = "tap") {
  void triggerNativeHaptic(kind).then((nativeTriggered) => {
    if (nativeTriggered) return;

    const pattern = fallbackHapticPattern(kind);
    try {
      if ("vibrate" in navigator) {
        navigator.vibrate(pattern);
      } else if (typeof pattern === "number") {
        void vibrate(pattern);
      }
    } catch {
      // Ignore if unsupported.
    }
  });
}

export function sortMobileFolders(folders: FolderItem[]): FolderItem[] {
  return [...folders].sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
  );
}

export function sortMobileSpaces(spaces: Space[]): Space[] {
  return [...spaces].sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
  );
}

export function sortMobileDocuments(documents: DocItem[]): DocItem[] {
  return [...documents].sort((a, b) =>
    a.title.localeCompare(b.title, undefined, { sensitivity: "base" }),
  );
}

export function flattenFolderChildren(folders: FolderItem[], parentId?: string): FolderItem[] {
  return sortMobileFolders(folders.filter((f) => f.parent_id === parentId));
}

export function getMobileFolderDepth(
  folders: FolderItem[],
  folderId: string,
  seen = new Set<string>(),
): number {
  if (seen.has(folderId)) return 0;
  seen.add(folderId);
  const folder = folders.find((item) => item.id === folderId);
  if (!folder) return 0;
  if (!folder.parent_id) return 1;
  const parentDepth = getMobileFolderDepth(folders, folder.parent_id, seen);
  return parentDepth > 0 ? parentDepth + 1 : 0;
}

export function canCreateMobileChildFolder(folders: FolderItem[], folderId: string): boolean {
  const depth = getMobileFolderDepth(folders, folderId);
  return depth > 0 && depth < 3;
}

function usePullToRefresh(onRefresh: () => Promise<void>) {
  const [pullDistance, setPullDistance] = useState(0);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const startYRef = useRef<number | null>(null);

  const onTouchStart = useCallback((e: TouchEvent<HTMLElement>) => {
    if (window.scrollY > 0) {
      startYRef.current = null;
      return;
    }
    startYRef.current = e.touches[0]?.clientY ?? null;
  }, []);

  const onTouchMove = useCallback((e: TouchEvent<HTMLElement>) => {
    const startY = startYRef.current;
    if (startY == null) return;

    const delta = (e.touches[0]?.clientY ?? 0) - startY;
    if (delta <= 0) {
      setPullDistance(0);
      return;
    }

    const next = Math.min(80, delta * 0.5);
    setPullDistance(next);
  }, []);

  const onTouchEnd = useCallback(async () => {
    const shouldRefresh = pullDistance >= 42;
    setPullDistance(0);
    startYRef.current = null;

    if (!shouldRefresh || isRefreshing) return;

    setIsRefreshing(true);
    try {
      await onRefresh();
    } finally {
      setIsRefreshing(false);
    }
  }, [isRefreshing, onRefresh, pullDistance]);

  return {
    pullDistance,
    isRefreshing,
    bind: {
      onTouchStart,
      onTouchMove,
      onTouchEnd,
      onTouchCancel: onTouchEnd,
    },
  };
}

function PullToRefreshHint({
  pullDistance,
  isRefreshing,
}: {
  pullDistance: number;
  isRefreshing: boolean;
}) {
  const visible = pullDistance > 0 || isRefreshing;
  if (!visible) return null;

  return (
    <div className="mb-2 flex items-center justify-center text-xs text-text-muted">
      <RefreshCw size={12} className={`mr-1 ${isRefreshing ? "animate-spin" : ""}`} />
      {isRefreshing ? "Refreshing..." : pullDistance >= 42 ? "Release to refresh" : "Pull to refresh"}
    </div>
  );
}

export function MobileSpacesScreen() {
  const navigate = useNavigate();
  const { user } = useAuth();
  const [spaces, setSpaces] = useState<Space[]>([]);
  const [spaceDataMap, setSpaceDataMap] = useState<Record<string, SpaceData>>({});
  const [activeSpaceId, setActiveSpaceId] = useState<string | null>(null);
  const [starredDocuments, setStarredDocuments] = useState(readStarredDocuments);
  const [expandedFolders, setExpandedFolders] = useState<Record<string, boolean>>({});
  const [expandedDocuments, setExpandedDocuments] = useState<Record<string, boolean>>({});
  const [templatePickerTarget, setTemplatePickerTarget] = useState<{
    folderId?: string;
    parentId?: string;
  } | null>(
    null,
  );
  const [renameSpaceOpen, setRenameSpaceOpen] = useState(false);
  const [renameSpaceName, setRenameSpaceName] = useState("");
  const [newSpaceOpen, setNewSpaceOpen] = useState(false);
  const [newSpaceName, setNewSpaceName] = useState("");
  const [newFolderOpen, setNewFolderOpen] = useState(false);
  const [newFolderName, setNewFolderName] = useState("");
  const [newFolderParentId, setNewFolderParentId] = useState<string | undefined>(undefined);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const localMode = getCurrentLaunchMode() === "local";
  const spacesLabel = localMode ? "Space" : "Spaces";
  const showCreateSpace = shouldShowMobileCreateSpace(localMode);

  const loadData = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const loadedSpaces = sortMobileSpaces(await dataClient.listSpaces());
      setSpaces(loadedSpaces);

      const storedSpaceId = localStorage.getItem(spaceStorageKey());
      const defaultSpaceId =
        loadedSpaces.find((space) => space.id === storedSpaceId)?.id ||
        loadedSpaces[0]?.id ||
        null;
      setActiveSpaceId(defaultSpaceId);

      const entries = await Promise.all(
        loadedSpaces.map(async (space) => {
          const [folders, documents, templates] = await Promise.all([
            dataClient.listFolders(space.id).catch(() => []),
            dataClient.listDocuments(space.id).catch(() => []),
            dataClient.listTemplates(space.id).catch(() => []),
          ]);
          return [space.id, { folders, documents, templates }] as const;
        }),
      );

      setSpaceDataMap(Object.fromEntries(entries));
    } catch {
      setError("Failed to load spaces.");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadData();
  }, [loadData]);

  useEffect(() => {
    if (!activeSpaceId) return;
    localStorage.setItem(spaceStorageKey(), activeSpaceId);
  }, [activeSpaceId]);

  useEffect(() => {
    const refreshStarredDocuments = () => setStarredDocuments(readStarredDocuments());
    window.addEventListener("starlane:starred-documents-changed", refreshStarredDocuments);
    return () =>
      window.removeEventListener("starlane:starred-documents-changed", refreshStarredDocuments);
  }, []);

  useEffect(() => {
    if (!activeSpaceId) return;
    void loadStarredDocuments(activeSpaceId).then(setStarredDocuments);
  }, [activeSpaceId]);

  const activeSpace = spaces.find((space) => space.id === activeSpaceId) || null;
  const activeSpaceOwnerLabel = activeSpace ? getPersonalSpaceOwnerLabel(activeSpace) : null;
  const activeData = activeSpaceId ? spaceDataMap[activeSpaceId] : undefined;
  const rootFolders = activeData ? flattenFolderChildren(activeData.folders, undefined) : [];
  const rootDocuments = activeData
    ? sortMobileDocuments(activeData.documents.filter((doc) => !doc.folder_id && !doc.parent_id))
    : [];
  const showSpaceSwitcher = shouldShowMobileSpaceSwitcher(spaces.length);
  const dailyDocuments = activeData
    ? getMobileDailyDocuments(activeData.documents, activeData.folders)
    : [];
  const recentDailyDocuments = dailyDocuments.slice(0, 5);
  const currentMonth = formatDateStamp(new Date()).slice(0, 7);
  const currentMonthDailyCount = dailyDocuments.filter((doc) =>
    doc.title.startsWith(currentMonth),
  ).length;
  const activeStarredDocuments = activeSpace
    ? starredDocuments
        .filter((doc) => doc.spaceId === activeSpace.id)
        .slice(0, 5)
    : [];

  const createDocumentFromTemplate = async (
    folderId?: string,
    parentId?: string,
    template?: TemplateRecord,
  ) => {
    if (!activeSpace) return;
    const title = template?.name ?? "Untitled";
    const rendered = template
      ? renderTemplateContent(template.content, {
          title,
          space: activeSpace.name,
          author: user?.display_name,
          properties: templatePropertiesAsRecord(template.properties),
        })
      : undefined;
    try {
      const created = await dataClient.createDocument(activeSpace.id, {
        title,
        folder_id: folderId ?? null,
        parent_id: parentId ?? null,
        content: rendered?.content,
        template_id: template?.id,
      });
      emitGraphDirty(activeSpace.id, "document-created");
      navigate(getMobileDocumentPath(activeSpace.id, created.id));
    } catch {
      setError("Failed to create a new document.");
    }
  };

  const createDocument = async (folderId?: string, parentId?: string) => {
    const templates = activeData?.templates || [];
    if (templates.length > 0) {
      setTemplatePickerTarget({ folderId, parentId });
      return;
    }
    await createDocumentFromTemplate(folderId, parentId);
  };

  const closeTemplatePicker = () => setTemplatePickerTarget(null);

  const selectTemplate = async (template?: TemplateRecord) => {
    const folderId = templatePickerTarget?.folderId;
    const parentId = templatePickerTarget?.parentId;
    closeTemplatePicker();
    await createDocumentFromTemplate(folderId, parentId, template);
  };

  const openToday = async () => {
    if (!activeSpace || !activeData) return;
    const today = new Date();
    const todayTitle = formatDateStamp(today);
    const existing = activeData.documents.find((doc) => doc.title === todayTitle);
    if (existing) {
      navigate(getMobileDocumentPath(activeSpace.id, existing.id));
      return;
    }

    const dailyTemplate = findDailyTemplate(activeData.templates);
    try {
      const dailyFolder =
        findDailyFolder(activeData.folders) ??
        (await dataClient.createFolder(activeSpace.id, DAILY_FOLDER_NAME));
      const created = await dataClient.createDocument(activeSpace.id, {
        title: todayTitle,
        folder_id: dailyFolder.id,
        content: dailyTemplate ? renderDailyTemplateContent(dailyTemplate, today) : undefined,
      });
      emitGraphDirty(activeSpace.id, "document-created");
      navigate(getMobileDocumentPath(activeSpace.id, created.id));
    } catch {
      setError("Failed to create today's note.");
    }
  };

  const openRenameSpace = () => {
    if (!activeSpace) return;
    setRenameSpaceName(activeSpace.name);
    setRenameSpaceOpen(true);
  };

  const renameSpace = async () => {
    if (!activeSpace) return;
    const nextName = renameSpaceName.trim();
    if (!nextName || nextName === activeSpace.name) {
      setRenameSpaceOpen(false);
      return;
    }
    try {
      const updated = await dataClient.updateSpace(activeSpace.id, { name: nextName });
      setSpaces((current) =>
        current.map((space) => (space.id === updated.id ? { ...space, name: updated.name, slug: updated.slug } : space)),
      );
      setRenameSpaceOpen(false);
    } catch {
      setError("Failed to rename space.");
    }
  };

  const openNewSpace = () => {
    setNewSpaceName("");
    setNewSpaceOpen(true);
  };

  const createSpace = async () => {
    const name = newSpaceName.trim();
    if (!name) return;
    try {
      const created = await dataClient.createSpace(name);
      setSpaces((current) => sortMobileSpaces([...current, created]));
      setSpaceDataMap((current) => ({
        ...current,
        [created.id]: { folders: [], documents: [], templates: [] },
      }));
      setActiveSpaceId(created.id);
      setNewSpaceOpen(false);
      setNewSpaceName("");
    } catch {
      setError("Failed to create space.");
    }
  };

  const openNewFolder = (parentId?: string) => {
    setNewFolderName("");
    setNewFolderParentId(parentId);
    setNewFolderOpen(true);
  };

  const createFolder = async () => {
    if (!activeSpace) return;
    const name = newFolderName.trim();
    if (!name) return;
    try {
      const created = await dataClient.createFolder(activeSpace.id, name, {
        parent_id: newFolderParentId ?? null,
      });
      setSpaceDataMap((current) => {
        const currentData = current[activeSpace.id] ?? {
          folders: [],
          documents: [],
          templates: [],
        };
        return {
          ...current,
          [activeSpace.id]: {
            ...currentData,
            folders: sortMobileFolders([...currentData.folders, created]),
          },
        };
      });
      setExpandedFolders((current) => ({ ...current, [created.id]: true }));
      setNewFolderOpen(false);
      setNewFolderName("");
      setNewFolderParentId(undefined);
    } catch {
      setError("Failed to create folder.");
    }
  };

  const toggleFolder = (folderId: string) => {
    setExpandedFolders((prev) => ({ ...prev, [folderId]: !prev[folderId] }));
  };

  const toggleDocument = (docId: string) => {
    setExpandedDocuments((prev) => ({ ...prev, [docId]: !(prev[docId] ?? true) }));
  };

  const pullRefresh = usePullToRefresh(loadData);

  if (loading) {
    return (
      <div className="flex items-center justify-center px-4 py-10 text-sm text-text-muted">
        <Loader2 size={16} className="mr-2 animate-spin" />
        Loading spaces...
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-xl px-3 py-3" {...pullRefresh.bind}>
      <PullToRefreshHint pullDistance={pullRefresh.pullDistance} isRefreshing={pullRefresh.isRefreshing} />
      {error && (
        <div className="mb-3 rounded-input border border-danger/30 bg-danger/10 px-3 py-2 text-sm text-danger">
          {error}
        </div>
      )}

      {templatePickerTarget && (
        <div className="fixed inset-0 z-50 flex items-end bg-black/30 px-3 pb-4 pt-20">
          <div className="max-h-[75vh] w-full overflow-hidden rounded-card border border-border bg-surface shadow-card">
            <div className="flex items-center justify-between border-b border-border px-3 py-2">
              <div>
                <h2 className="text-sm font-medium text-text">New document</h2>
                <p className="text-xs text-text-muted">Choose a starting point.</p>
              </div>
              <button
                type="button"
                onClick={closeTemplatePicker}
                className="flex h-8 w-8 items-center justify-center rounded-button text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
                aria-label="Close"
              >
                <X size={14} />
              </button>
            </div>
            <div className="max-h-[60vh] overflow-y-auto p-2">
              <button
                type="button"
                onClick={() => void selectTemplate()}
                className="flex w-full items-center gap-2 rounded-button px-2 py-2 text-left text-sm text-text transition-colors duration-100 hover:bg-surface-raised"
              >
                <FileText size={14} className="shrink-0" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate font-medium">Blank document</span>
                  <span className="block truncate text-xs text-text-muted">Untitled</span>
                </span>
              </button>
              {(activeData?.templates || []).map((template) => (
                <button
                  key={template.id}
                  type="button"
                  onClick={() => void selectTemplate(template)}
                  className="flex w-full items-center gap-2 rounded-button px-2 py-2 text-left text-sm text-text transition-colors duration-100 hover:bg-surface-raised"
                >
                  <FileText size={14} className="shrink-0" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate font-medium">{template.name}</span>
                    <span className="block truncate text-xs text-text-muted">
                      {template.description || "Template"}
                    </span>
                  </span>
                </button>
              ))}
            </div>
          </div>
        </div>
      )}

      {renameSpaceOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/35 px-4">
          <div className="w-full max-w-sm overflow-hidden rounded-card border border-border bg-surface shadow-card">
            <div className="flex items-center justify-between border-b border-border px-3 py-2">
              <div>
                <h2 className="text-sm font-medium text-text">Rename space</h2>
                <p className="text-xs text-text-muted">This changes the space name shown in the app.</p>
              </div>
              <button
                type="button"
                onClick={() => setRenameSpaceOpen(false)}
                className="flex h-8 w-8 items-center justify-center rounded-button text-text-muted transition-colors duration-100 active:bg-surface-raised hover:text-text"
                aria-label="Close"
              >
                <X size={14} />
              </button>
            </div>
            <div className="grid gap-3 p-3">
              <MobileTextInput
                value={renameSpaceName}
                onChange={setRenameSpaceName}
                placeholder="Space name"
                ariaLabel="Space name"
                className="text-sm"
                autoFocus
              />
              <button
                type="button"
                onClick={() => void renameSpace()}
                disabled={!renameSpaceName.trim()}
                className="h-10 rounded-button bg-primary px-3 text-sm font-medium text-on-primary transition-opacity duration-100 disabled:opacity-50"
              >
                Save
              </button>
            </div>
          </div>
        </div>
      )}

      {newSpaceOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/35 px-4">
          <div className="w-full max-w-sm overflow-hidden rounded-card border border-border bg-surface shadow-card">
            <div className="flex items-center justify-between border-b border-border px-3 py-2">
              <div>
                <h2 className="text-sm font-medium text-text">New space</h2>
                <p className="text-xs text-text-muted">Create another local workspace.</p>
              </div>
              <button
                type="button"
                onClick={() => setNewSpaceOpen(false)}
                className="flex h-8 w-8 items-center justify-center rounded-button text-text-muted transition-colors duration-100 active:bg-surface-raised hover:text-text"
                aria-label="Close"
              >
                <X size={14} />
              </button>
            </div>
            <div className="grid gap-3 p-3">
              <MobileTextInput
                value={newSpaceName}
                onChange={setNewSpaceName}
                placeholder="Space name"
                ariaLabel="New space name"
                className="text-sm"
                autoFocus
              />
              <button
                type="button"
                onClick={() => void createSpace()}
                disabled={!newSpaceName.trim()}
                className="h-10 rounded-button bg-primary px-3 text-sm font-medium text-on-primary transition-opacity duration-100 disabled:opacity-50"
              >
                Create Space
              </button>
            </div>
          </div>
        </div>
      )}

      {newFolderOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/35 px-4">
          <div className="w-full max-w-sm overflow-hidden rounded-card border border-border bg-surface shadow-card">
            <div className="flex items-center justify-between border-b border-border px-3 py-2">
              <div>
                <h2 className="text-sm font-medium text-text">
                  {newFolderParentId ? "New subfolder" : "New folder"}
                </h2>
                <p className="text-xs text-text-muted">
                  {newFolderParentId
                    ? "Create a folder inside this folder."
                    : "Create a root folder in this space."}
                </p>
              </div>
              <button
                type="button"
                onClick={() => {
                  setNewFolderOpen(false);
                  setNewFolderParentId(undefined);
                }}
                className="flex h-8 w-8 items-center justify-center rounded-button text-text-muted transition-colors duration-100 active:bg-surface-raised hover:text-text"
                aria-label="Close"
              >
                <X size={14} />
              </button>
            </div>
            <div className="grid gap-3 p-3">
              <MobileTextInput
                value={newFolderName}
                onChange={setNewFolderName}
                placeholder="Folder name"
                ariaLabel="New folder name"
                className="text-sm"
                autoFocus
              />
              <button
                type="button"
                onClick={() => void createFolder()}
                disabled={!newFolderName.trim()}
                className="h-10 rounded-button bg-primary px-3 text-sm font-medium text-on-primary transition-opacity duration-100 disabled:opacity-50"
              >
                Create Folder
              </button>
            </div>
          </div>
        </div>
      )}

      <div className={`mb-3 flex items-center gap-2 ${showSpaceSwitcher ? "justify-between" : "justify-end"}`}>
        {showSpaceSwitcher && <h2 className="text-sm font-semibold text-text">{spacesLabel}</h2>}
        <div className="flex items-center gap-1">
          {showCreateSpace && (
            <button
              type="button"
              onClick={openNewSpace}
              className="flex items-center gap-1 rounded-button px-2 py-1 text-xs text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
              aria-label="Create space"
            >
              <Plus size={13} />
              New Space
            </button>
          )}
          <button
            type="button"
            onClick={() => void loadData()}
            className="flex items-center gap-1 rounded-button px-2 py-1 text-xs text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
            aria-label="Refresh spaces"
          >
            <RefreshCw size={13} />
            Refresh
          </button>
        </div>
      </div>

      {showSpaceSwitcher && (
        <div className="scrollbar-none mb-4 overflow-x-auto pb-1">
          <div className="flex min-w-max gap-2">
            {spaces.map((space) => (
              <button
                key={space.id}
                type="button"
                onClick={() => setActiveSpaceId(space.id)}
                className={`rounded-button border px-3 py-1.5 text-xs transition-colors duration-100 ${
                  space.id === activeSpaceId
                    ? "border-primary bg-primary/10 text-text"
                    : "border-border bg-surface text-text-muted hover:text-text"
                }`}
                aria-current={space.id === activeSpaceId ? "page" : undefined}
              >
                {space.icon ? `${space.icon} ` : ""}
                {space.name}
              </button>
            ))}
          </div>
        </div>
      )}

      {!activeSpace && (
        <div className="rounded-card border border-border bg-surface p-4 text-sm text-text-muted">
          No spaces found.
        </div>
      )}

      {activeSpace && (
        <div className="rounded-card border border-border bg-surface p-3">
          <div className="mb-3 flex items-center justify-between">
            <div className="min-w-0">
              <p className="truncate text-base font-semibold text-text">{activeSpace.name}</p>
              <p className="truncate text-xs text-text-muted">
                {activeSpaceOwnerLabel || (localMode ? "Local vault" : "Space")}
              </p>
            </div>
            <div className="grid shrink-0 grid-cols-2 gap-1">
              <button
                type="button"
                onClick={() => void createDocument()}
                className="flex min-h-8 items-center justify-center gap-1 rounded-button bg-surface-raised px-2.5 py-1.5 text-xs text-text transition-colors duration-100 hover:opacity-90"
              >
                <Plus size={12} />
                New Doc
              </button>
              <button
                type="button"
                onClick={() => openNewFolder()}
                className="flex min-h-8 items-center justify-center gap-1 rounded-button bg-surface-raised px-2.5 py-1.5 text-xs text-text transition-colors duration-100 hover:opacity-90"
              >
                <Folder size={12} />
                New Folder
              </button>
            </div>
          </div>
          <div className="mb-3 flex justify-end">
            <button
              type="button"
              onClick={openRenameSpace}
              className="flex items-center gap-1 rounded-button px-2 py-1 text-xs text-text-muted transition-colors duration-100 active:bg-surface-raised hover:text-text"
              aria-label="Rename space"
              title="Rename space"
            >
              <Pencil size={13} />
              Rename
            </button>
          </div>
          <div className="mb-3 grid grid-cols-2 gap-2">
            <button
              type="button"
              onClick={() => void openToday()}
              className="flex min-h-12 items-center gap-2 rounded-button border border-border px-2.5 py-2 text-left text-xs text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
            >
              <FileText size={14} className="shrink-0" />
              <span className="min-w-0">
                <span className="block truncate font-medium text-text">Today</span>
                <span className="block truncate">Daily note</span>
              </span>
            </button>
            <button
              type="button"
              onClick={() => navigate(getMobileJournalPath(activeSpace.id))}
              className="flex min-h-12 items-center gap-2 rounded-button border border-border px-2.5 py-2 text-left text-xs text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
            >
              <CalendarDays size={14} className="shrink-0" />
              <span className="min-w-0">
                <span className="block truncate font-medium text-text">Journal</span>
                <span className="block truncate">{currentMonthDailyCount} this month</span>
              </span>
            </button>
          </div>

          {recentDailyDocuments.length > 0 && (
            <div className="mb-3 rounded-input border border-border bg-surface p-2">
              <div className="mb-1 flex items-center justify-between gap-2">
                <p className="truncate text-xs font-medium text-text">Recent daily notes</p>
                <button
                  type="button"
                  onClick={() => navigate(getMobileJournalPath(activeSpace.id))}
                  className="shrink-0 rounded-button px-1.5 py-1 text-xs text-text-muted hover:bg-surface-raised hover:text-text"
                >
                  Open
                </button>
              </div>
              <div className="scrollbar-none flex gap-1.5 overflow-x-auto pb-1">
                {recentDailyDocuments.map((doc) => (
                  <button
                    key={doc.id}
                    type="button"
                    onClick={() => navigate(getMobileDocumentPath(activeSpace.id, doc.id))}
                    className="shrink-0 rounded-button border border-border bg-surface px-2 py-1 text-xs text-text-muted hover:bg-surface-raised hover:text-text"
                  >
                    {doc.title.slice(5)}
                  </button>
                ))}
              </div>
            </div>
          )}

          {activeStarredDocuments.length > 0 && (
            <div className="mb-3 rounded-input border border-border bg-surface p-2">
              <div className="mb-1 flex items-center gap-1.5">
                <Star size={12} className="text-accent" fill="currentColor" />
                <p className="truncate text-xs font-medium text-text">Starred</p>
              </div>
              <div className="space-y-1">
                {activeStarredDocuments.map((doc) => (
                  <button
                    key={`${doc.spaceId}:${doc.id}`}
                    type="button"
                    onClick={() => navigate(getMobileDocumentPath(doc.spaceId, doc.id))}
                    className="flex w-full items-center gap-2 rounded-button px-2 py-1.5 text-left text-sm text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
                  >
                    <Star size={12} className="shrink-0 text-accent" fill="currentColor" />
                    <span className="min-w-0 flex-1 truncate">
                      {doc.icon ? `${doc.icon} ` : ""}
                      {doc.title}
                    </span>
                  </button>
                ))}
              </div>
            </div>
          )}

          <div className="space-y-1">
            {rootFolders.map((folder) => (
              <MobileFolderNode
                key={folder.id}
                folder={folder}
                allFolders={activeData?.folders || []}
                documents={activeData?.documents || []}
                expandedFolders={expandedFolders}
                expandedDocuments={expandedDocuments}
                onToggleFolder={toggleFolder}
                onToggleDocument={toggleDocument}
                onOpenDocument={(docId) => navigate(getMobileDocumentPath(activeSpace.id, docId))}
                onCreateDocument={(folderId, parentId) => void createDocument(folderId, parentId)}
                onCreateFolder={openNewFolder}
              />
            ))}

            {rootDocuments.map((doc) => (
              <MobileDocumentNode
                key={doc.id}
                doc={doc}
                documents={activeData?.documents || []}
                expandedDocuments={expandedDocuments}
                onToggleDocument={toggleDocument}
                onOpenDocument={(docId) => navigate(getMobileDocumentPath(activeSpace.id, docId))}
                onCreateChild={(parentId) => void createDocument(undefined, parentId)}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function MobileFolderNode({
  folder,
  allFolders,
  documents,
  expandedFolders,
  expandedDocuments,
  onToggleFolder,
  onToggleDocument,
  onOpenDocument,
  onCreateDocument,
  onCreateFolder,
}: {
  folder: FolderItem;
  allFolders: FolderItem[];
  documents: DocItem[];
  expandedFolders: Record<string, boolean>;
  expandedDocuments: Record<string, boolean>;
  onToggleFolder: (folderId: string) => void;
  onToggleDocument: (docId: string) => void;
  onOpenDocument: (docId: string) => void;
  onCreateDocument: (folderId: string, parentId?: string) => void;
  onCreateFolder: (parentId: string) => void;
}) {
  const childFolders = flattenFolderChildren(allFolders, folder.id);
  const folderDocs = sortMobileDocuments(
    documents.filter((doc) => doc.folder_id === folder.id && !doc.parent_id),
  );
  const expanded = !!expandedFolders[folder.id];
  const canCreateChildFolder = canCreateMobileChildFolder(allFolders, folder.id);

  return (
    <div>
      <div className="group flex items-center">
        <button
          type="button"
          onClick={() => onToggleFolder(folder.id)}
          className={`${MOBILE_TREE_TOGGLE_LANE_CLASS} rounded-button text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text`}
          aria-expanded={expanded}
          aria-label={`${expanded ? "Collapse" : "Expand"} folder ${folder.name}`}
        >
          <ChevronRight
            size={13}
            className={`shrink-0 transition-transform duration-100 ${expanded ? "rotate-90" : ""}`}
          />
        </button>
        <button
          type="button"
          onClick={() => onToggleFolder(folder.id)}
          className="flex min-w-0 flex-1 items-center gap-2 rounded-button px-2 py-1.5 text-left text-sm text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
          aria-expanded={expanded}
          aria-label={`Toggle folder ${folder.name}`}
        >
          <Folder size={MOBILE_TREE_ICON_SIZE} className="shrink-0" />
          <span className="truncate">{folder.icon ? `${folder.icon} ` : ""}{folder.name}</span>
        </button>
        <button
          type="button"
          onClick={() => onCreateDocument(folder.id)}
          className="ml-1 rounded-button px-1.5 py-1 text-xs text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
          aria-label={`Create document in ${folder.name}`}
        >
          <Plus size={12} />
        </button>
        {canCreateChildFolder && (
          <button
            type="button"
            onClick={() => onCreateFolder(folder.id)}
            className="ml-1 rounded-button px-1.5 py-1 text-xs text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
            aria-label={`Create subfolder in ${folder.name}`}
          >
            <FolderPlus size={12} />
          </button>
        )}
      </div>

      {expanded && (
        <div className="ml-5 space-y-1">
          {childFolders.map((childFolder) => (
            <MobileFolderNode
              key={childFolder.id}
              folder={childFolder}
              allFolders={allFolders}
              documents={documents}
              expandedFolders={expandedFolders}
              expandedDocuments={expandedDocuments}
              onToggleFolder={onToggleFolder}
              onToggleDocument={onToggleDocument}
              onOpenDocument={onOpenDocument}
              onCreateDocument={onCreateDocument}
              onCreateFolder={onCreateFolder}
            />
          ))}
          {folderDocs.map((doc) => (
            <MobileDocumentNode
              key={doc.id}
              doc={doc}
              documents={documents}
              expandedDocuments={expandedDocuments}
              onToggleDocument={onToggleDocument}
              onOpenDocument={onOpenDocument}
              onCreateChild={(parentId) => onCreateDocument(folder.id, parentId)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function MobileDocumentNode({
  doc,
  documents,
  expandedDocuments,
  onToggleDocument,
  onOpenDocument,
  onCreateChild,
}: {
  doc: DocItem;
  documents: DocItem[];
  expandedDocuments: Record<string, boolean>;
  onToggleDocument: (docId: string) => void;
  onOpenDocument: (docId: string) => void;
  onCreateChild: (parentId: string) => void;
}) {
  const children = sortMobileDocuments(documents.filter((candidate) => candidate.parent_id === doc.id));
  const expanded = expandedDocuments[doc.id] ?? true;
  const hasChildren = children.length > 0;
  return (
    <div>
      <div className="group flex items-center">
        {hasChildren ? (
          <button
            type="button"
            onClick={() => onToggleDocument(doc.id)}
            className={`${MOBILE_TREE_TOGGLE_LANE_CLASS} rounded-button text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text`}
            aria-label={`${expanded ? "Collapse" : "Expand"} child pages for ${doc.title}`}
            aria-expanded={expanded}
          >
            <ChevronRight
              size={13}
              className={`shrink-0 transition-transform duration-100 ${expanded ? "rotate-90" : ""}`}
            />
          </button>
        ) : (
          <span className={MOBILE_TREE_TOGGLE_LANE_CLASS} aria-hidden="true" />
        )}
        <button type="button" onClick={() => onOpenDocument(doc.id)} className="flex min-w-0 flex-1 items-center gap-2 rounded-button px-2 py-1.5 text-left text-sm text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text">
          <FileText size={MOBILE_TREE_ICON_SIZE} className="shrink-0" />
          <span className="truncate">{doc.icon ? `${doc.icon} ` : ""}{doc.title}</span>
        </button>
        <button type="button" onClick={() => onCreateChild(doc.id)} className="rounded-button px-1.5 py-1 text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text" aria-label={`Create child page under ${doc.title}`}>
          <Plus size={12} />
        </button>
      </div>
      {expanded && children.length > 0 && (
        <div className="ml-5 space-y-1">
          {children.map((child) => (
            <MobileDocumentNode
              key={child.id}
              doc={child}
              documents={documents}
              expandedDocuments={expandedDocuments}
              onToggleDocument={onToggleDocument}
              onOpenDocument={onOpenDocument}
              onCreateChild={onCreateChild}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function MobileRecentScreen() {
  const navigate = useNavigate();
  const [items, setItems] = useState<Array<DocItem & { spaceId: string; spaceName: string }>>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [menuTarget, setMenuTarget] = useState<(DocItem & { spaceId: string; spaceName: string }) | null>(null);
  const [starredKeys, setStarredKeys] = useState<Record<string, true>>(() =>
    Object.fromEntries(
      readStarredDocuments().map((item) => [getStarredDocumentKey(item), true]),
    ) as Record<string, true>,
  );
  const [statusMessage, setStatusMessage] = useState("");
  const [pendingDelete, setPendingDelete] = useState<(DocItem & { spaceId: string; spaceName: string }) | null>(null);
  const [pendingDeleteExpiresAt, setPendingDeleteExpiresAt] = useState<number | null>(null);
  const [pendingDeleteSecondsLeft, setPendingDeleteSecondsLeft] = useState(0);
  const longPressTimerRef = useRef<ReturnType<typeof setTimeout>>();
  const pendingDeleteTimerRef = useRef<ReturnType<typeof setTimeout>>();
  const rowTouchStartRef = useRef<{ x: number; y: number; key: string } | null>(null);
  const suppressClickUntilRef = useRef(0);

  const sortRecent = useCallback(
    (list: Array<DocItem & { spaceId: string; spaceName: string }>) => {
      return sortRecentItems(list, starredKeys);
    },
    [starredKeys],
  );

  const loadRecent = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const spaces = await dataClient.listSpaces();
      const data = await Promise.all(
        spaces.map(async (space) => {
          const docs = await dataClient.listDocuments(space.id).catch(() => []);
          return docs.map((doc) => ({ ...doc, spaceId: space.id, spaceName: space.name }));
        }),
      );

      const merged = data.flat();
      setItems(sortRecent(merged).slice(0, 40));
    } catch {
      setError("Failed to load recent documents.");
    } finally {
      setLoading(false);
    }
  }, [sortRecent]);

  useEffect(() => {
    void loadRecent();
  }, [loadRecent]);

  useEffect(() => {
    const refreshStarred = () => {
      const nextStarredKeys = Object.fromEntries(
        readStarredDocuments().map((item) => [getStarredDocumentKey(item), true]),
      ) as Record<string, true>;
      setStarredKeys(nextStarredKeys);
      setItems((current) => sortRecentItems(current, nextStarredKeys));
    };
    window.addEventListener("starlane:starred-documents-changed", refreshStarred);
    return () => window.removeEventListener("starlane:starred-documents-changed", refreshStarred);
  }, []);

  useEffect(() => {
    return () => {
      clearLongPress();
      if (pendingDeleteTimerRef.current) {
        clearTimeout(pendingDeleteTimerRef.current);
      }
    };
  }, []);

  useEffect(() => {
    if (!pendingDeleteExpiresAt || !pendingDelete) {
      setPendingDeleteSecondsLeft(0);
      return;
    }

    const refreshCountdown = () => {
      const msLeft = Math.max(0, pendingDeleteExpiresAt - Date.now());
      setPendingDeleteSecondsLeft(Math.ceil(msLeft / 1000));
    };

    refreshCountdown();
    const interval = setInterval(refreshCountdown, 250);
    return () => clearInterval(interval);
  }, [pendingDelete, pendingDeleteExpiresAt]);

  const pullRefresh = usePullToRefresh(loadRecent);

  const clearLongPress = () => {
    if (longPressTimerRef.current) {
      clearTimeout(longPressTimerRef.current);
      longPressTimerRef.current = undefined;
    }
  };

  const onLongPressStart = (item: DocItem & { spaceId: string; spaceName: string }) => {
    clearLongPress();
    longPressTimerRef.current = setTimeout(() => {
      setMenuTarget(item);
    }, 450);
  };

  const copyDocLink = async (item: DocItem & { spaceId: string; spaceName: string }) => {
    const url = getMobileDocumentUrl(window.location.origin, item.spaceId, item.id);
    try {
      await navigator.clipboard.writeText(url);
      setStatusMessage("Link copied");
      setMenuTarget(null);
      triggerHaptic("tap");
    } catch {
      setError("Failed to copy link.");
      triggerHaptic("error");
    }
  };

  const shareDoc = async (item: DocItem & { spaceId: string; spaceName: string }) => {
    const url = getMobileDocumentUrl(window.location.origin, item.spaceId, item.id);
    try {
      if (navigator.share) {
        await navigator.share({ title: item.title, url });
        setStatusMessage("Share sheet opened");
      } else {
        await navigator.clipboard.writeText(url);
        setStatusMessage("Link copied");
      }
      setMenuTarget(null);
      triggerHaptic("tap");
    } catch {
      setError("Failed to share document.");
      triggerHaptic("error");
    }
  };

  const commitDelete = async (item: DocItem & { spaceId: string; spaceName: string }) => {
    try {
      await dataClient.deleteDocument(item.spaceId, item.id);
      emitGraphDirty(item.spaceId, "document-deleted");
      triggerHaptic("confirm");
      setStatusMessage("Document deleted permanently");
    } catch {
      setError("Failed to delete document.");
      triggerHaptic("error");
    }
  };

  const queueDelete = async (item: DocItem & { spaceId: string; spaceName: string }) => {
    if (pendingDelete) {
      if (pendingDeleteTimerRef.current) {
        clearTimeout(pendingDeleteTimerRef.current);
      }
      await commitDelete(pendingDelete);
    }

    setItems((prev) => prev.filter((doc) => !(doc.spaceId === item.spaceId && doc.id === item.id)));
    setPendingDelete(item);
    setStatusMessage("Document removed. Undo before permanent delete.");
    setMenuTarget(null);
    triggerHaptic("action");
    const expiresAt = Date.now() + 5000;
    setPendingDeleteExpiresAt(expiresAt);
    setPendingDeleteSecondsLeft(5);

    pendingDeleteTimerRef.current = setTimeout(() => {
      void commitDelete(item);
      setPendingDelete((current) =>
        current && getStarredDocumentKey(current) === getStarredDocumentKey(item) ? null : current,
      );
      setPendingDeleteExpiresAt((current) => (current === expiresAt ? null : current));
      setPendingDeleteSecondsLeft(0);
    }, 5000);
  };

  const undoDelete = () => {
    if (!pendingDelete) return;
    if (pendingDeleteTimerRef.current) {
      clearTimeout(pendingDeleteTimerRef.current);
      pendingDeleteTimerRef.current = undefined;
    }

    setItems((prev) => sortRecent([...prev, pendingDelete]));
    setStatusMessage("Delete undone");
    setPendingDelete(null);
    setPendingDeleteExpiresAt(null);
    setPendingDeleteSecondsLeft(0);
    triggerHaptic("tap");
  };

  const togglePinned = (item: DocItem & { spaceId: string; spaceName: string }) => {
    const key = getStarredDocumentKey(item);
    const nextStarred = !starredKeys[key];
    const nextStarredKeys = Object.fromEntries(
      readStarredDocuments().map((starred) => [getStarredDocumentKey(starred), true]),
    ) as Record<string, true>;
    if (nextStarred) {
      nextStarredKeys[key] = true;
    } else {
      delete nextStarredKeys[key];
    }
    setStarredKeys(nextStarredKeys);
    setStatusMessage(nextStarred ? "Starred document" : "Removed star");
    triggerHaptic("tap");
    setItems((prev) => sortRecentItems(prev, nextStarredKeys));
    void toggleStarredDocumentSynced(item).then(() => {
      const reconciledStarredKeys = Object.fromEntries(
        readStarredDocuments().map((starred) => [getStarredDocumentKey(starred), true]),
      ) as Record<string, true>;
      setStarredKeys(reconciledStarredKeys);
      setItems((prev) => sortRecentItems(prev, reconciledStarredKeys));
    });
  };

  const handleRowTouchStart = (item: DocItem & { spaceId: string; spaceName: string }, e: TouchEvent<HTMLButtonElement>) => {
    rowTouchStartRef.current = {
      x: e.touches[0]?.clientX ?? 0,
      y: e.touches[0]?.clientY ?? 0,
      key: getStarredDocumentKey(item),
    };
    onLongPressStart(item);
  };

  const handleRowTouchEnd = async (item: DocItem & { spaceId: string; spaceName: string }, e: TouchEvent<HTMLButtonElement>) => {
    const start = rowTouchStartRef.current;
    clearLongPress();
    rowTouchStartRef.current = null;
    if (!start || start.key !== getStarredDocumentKey(item)) return;

    const endX = e.changedTouches[0]?.clientX ?? start.x;
    const endY = e.changedTouches[0]?.clientY ?? start.y;
    const dx = endX - start.x;
    const dy = endY - start.y;

    if (Math.abs(dx) < 70 || Math.abs(dx) <= Math.abs(dy)) return;

    suppressClickUntilRef.current = Date.now() + 500;
    const action = getSwipeAction(dx, dy);
    if (!action) return;

    if (action === "pin") {
      togglePinned(item);
      return;
    }

    await queueDelete(item);
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center px-4 py-10 text-sm text-text-muted">
        <Loader2 size={16} className="mr-2 animate-spin" />
        Loading recent documents...
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-xl px-3 py-3" {...pullRefresh.bind}>
      <PullToRefreshHint pullDistance={pullRefresh.pullDistance} isRefreshing={pullRefresh.isRefreshing} />
      <div className="mb-3 flex items-center justify-between gap-2">
        <h2 className="text-sm font-semibold text-text">Recent</h2>
        <button
          type="button"
          onClick={() => void loadRecent()}
          className="flex items-center gap-1 rounded-button px-2 py-1 text-xs text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
          aria-label="Refresh recent documents"
        >
          <RefreshCw size={13} />
          Refresh
        </button>
      </div>

      {error && (
        <div className="mb-3 rounded-input border border-danger/30 bg-danger/10 px-3 py-2 text-sm text-danger">
          {error}
        </div>
      )}
      {statusMessage && (
        <div className="mb-3 rounded-input border border-primary/20 bg-primary/10 px-3 py-2 text-sm text-text">
          <div className="flex items-center justify-between gap-2">
            <span>{statusMessage}</span>
            {pendingDelete && (
              <span className="flex items-center gap-2">
                {pendingDeleteSecondsLeft > 0 && (
                  <span className="text-xs text-text-muted">{pendingDeleteSecondsLeft}s</span>
                )}
                <button
                  type="button"
                  onClick={undoDelete}
                  className="rounded-button border border-border px-2 py-1 text-xs text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
                >
                  Undo
                </button>
              </span>
            )}
          </div>
        </div>
      )}

      <p className="mb-2 px-1 text-[11px] text-text-muted">
        Swipe right to star, swipe left to delete.
      </p>

      <div className="rounded-card border border-border bg-surface p-2">
        {items.length === 0 && <p className="px-2 py-3 text-sm text-text-muted">No recent documents yet.</p>}
        {items.map((item) => (
          <button
            key={`${item.spaceId}:${item.id}`}
            type="button"
            onClick={() => {
              if (Date.now() < suppressClickUntilRef.current) return;
              navigate(getMobileDocumentPath(item.spaceId, item.id));
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              setMenuTarget(item);
            }}
            onTouchStart={(e) => handleRowTouchStart(item, e)}
            onTouchEnd={(e) => {
              void handleRowTouchEnd(item, e);
            }}
            onTouchMove={clearLongPress}
            className="flex w-full items-start gap-2 rounded-button px-2 py-2 text-left transition-colors duration-100 hover:bg-surface-raised"
          >
            <FileText size={14} className="mt-0.5 shrink-0 text-text-muted" />
            <span className="min-w-0 flex-1">
              <span className="block truncate text-sm text-text">
                {starredKeys[getStarredDocumentKey(item)] && (
                  <Star size={11} className="mr-1 inline text-accent" fill="currentColor" />
                )}
                {item.icon ? `${item.icon} ` : ""}
                {item.title}
              </span>
              <span className="block truncate text-xs text-text-muted">{item.spaceName}</span>
              <span className="block text-[11px] text-text-muted">{formatRelativeDate(item.updated_at)}</span>
            </span>
          </button>
        ))}
      </div>

      {menuTarget && (
        <>
          <div className="fixed inset-0 z-40 bg-black/35" onClick={() => setMenuTarget(null)} />
          <div className="fixed inset-x-3 bottom-[calc(env(safe-area-inset-bottom)+4.2rem)] z-50 rounded-card border border-border bg-surface p-2 shadow-card">
            <p className="px-2 py-1 text-xs text-text-muted">
              {menuTarget.title}
            </p>
            <button
              type="button"
              onClick={() => {
                navigate(getMobileDocumentPath(menuTarget.spaceId, menuTarget.id));
                setMenuTarget(null);
              }}
              className="w-full rounded-button px-2 py-2 text-left text-sm text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
            >
              Open
            </button>
            <button
              type="button"
              onClick={() => void copyDocLink(menuTarget)}
              className="w-full rounded-button px-2 py-2 text-left text-sm text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
            >
              Copy Link
            </button>
            <button
              type="button"
              onClick={() => void shareDoc(menuTarget)}
              className="w-full rounded-button px-2 py-2 text-left text-sm text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
            >
              Share
            </button>
            <button
              type="button"
              onClick={() => void queueDelete(menuTarget)}
              className="w-full rounded-button px-2 py-2 text-left text-sm text-danger transition-colors duration-100 hover:bg-danger/10"
            >
              Delete
            </button>
            <button
              type="button"
              onClick={() => setMenuTarget(null)}
              className="mt-1 w-full rounded-button border border-border px-2 py-2 text-left text-sm text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
            >
              Cancel
            </button>
          </div>
        </>
      )}
    </div>
  );
}

export function MobileSearchScreen() {
  const navigate = useNavigate();
  const debounceRef = useRef<ReturnType<typeof setTimeout>>();
  const [query, setQuery] = useState("");
  const [filters, setFilters] = useState<SearchFilters>({});
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [loading, setLoading] = useState(false);
  const [results, setResults] = useState<SearchResultRecord[]>([]);
  const hasFilters = Object.values(filters).some((filterValue) => Boolean(filterValue?.trim()));

  const runSearch = useCallback((value: string, nextFilters: SearchFilters = filters) => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    const nextHasFilters = Object.values(nextFilters).some((filterValue) => Boolean(filterValue?.trim()));
    if (value.trim().length < 2 && !nextHasFilters) {
      setResults([]);
      return;
    }

    debounceRef.current = setTimeout(async () => {
      setLoading(true);
      try {
        const data = await dataClient.searchDocuments(value.trim(), 25, nextFilters);
        setResults(data);
      } catch {
        setResults([]);
      } finally {
        setLoading(false);
      }
    }, 220);
  }, [filters]);

  const updateFilter = (key: keyof SearchFilters, value: string) => {
    const nextFilters = { ...filters, [key]: value };
    if (!value.trim()) delete nextFilters[key];
    setFilters(nextFilters);
    runSearch(query, nextFilters);
  };

  useEffect(() => () => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
  }, []);

  return (
    <div className="mx-auto max-w-xl px-3 py-3">
      <div className="mb-3 rounded-card border border-border bg-surface p-2 shadow-card">
        <div className="flex items-center gap-2">
          <SearchInput
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              runSearch(e.target.value);
            }}
            placeholder="Search documents..."
            aria-label="Search documents"
            containerClassName="min-w-0 flex-1"
            className="h-11 text-base"
            autoCorrect="off"
            autoCapitalize="none"
          />
          <button
            type="button"
            onClick={() => setFiltersOpen((open) => !open)}
            className={`flex h-11 w-11 shrink-0 items-center justify-center rounded-input border border-border transition-colors duration-100 ${
              filtersOpen || hasFilters
                ? "bg-primary/15 text-primary"
                : "text-text-muted active:bg-surface-raised hover:text-text"
            }`}
            aria-label="Search filters"
            aria-expanded={filtersOpen}
          >
            <SlidersHorizontal size={15} />
          </button>
        </div>

        {filtersOpen && (
          <div className="mt-2 grid gap-2 border-t border-border/70 pt-2">
            <label className="grid gap-1 text-xs text-text-muted">
              Tag
              <MobileTextInput
                value={filters.tag ?? ""}
                onChange={(value) => updateFilter("tag", value)}
                placeholder="#tag"
                ariaLabel="Search tag filter"
                className="h-10 min-w-0"
                autoCorrect="off"
                autoCapitalize="none"
              />
            </label>
            <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
              <label className="grid gap-1 text-xs text-text-muted">
                Property
                <MobileTextInput
                  value={filters.property_name ?? ""}
                  onChange={(value) => updateFilter("property_name", value)}
                  placeholder="Property"
                  ariaLabel="Search property filter"
                  className="h-10 min-w-0"
                  autoCorrect="off"
                  autoCapitalize="none"
                />
              </label>
              <label className="grid gap-1 text-xs text-text-muted">
                Value
                <MobileTextInput
                  value={filters.property_value ?? ""}
                  onChange={(value) => updateFilter("property_value", value)}
                  placeholder="Value"
                  ariaLabel="Search property value filter"
                  className="h-10 min-w-0"
                  autoCorrect="off"
                  autoCapitalize="none"
                />
              </label>
            </div>
          </div>
        )}
      </div>

      {loading && (
        <div className="px-1 py-2 text-sm text-text-muted">Searching...</div>
      )}

      {!loading &&
        (query.trim().length >= 2 || hasFilters) &&
        results.length === 0 && (
        <div className="px-1 py-2 text-sm text-text-muted">No results found.</div>
      )}

      {results.length > 0 && (
        <div className="rounded-card border border-border bg-surface p-2">
          {results.map((result) => (
            <button
              key={`${result.space_id}:${result.document_id}`}
              type="button"
              onClick={() => navigate(getMobileDocumentPath(result.space_id, result.document_id))}
              className="flex w-full items-start gap-2 rounded-button px-2 py-2 text-left transition-colors duration-100 hover:bg-surface-raised"
            >
              <FileText size={14} className="mt-0.5 shrink-0 text-text-muted" />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm text-text">{result.title}</span>
                <span className="block truncate text-xs text-text-muted">{result.space_name}</span>
                {result.excerpt && (
                  <span className="block truncate text-[11px] text-text-muted">{result.excerpt}</span>
                )}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function MobileNotificationsScreen() {
  const localMode = getCurrentLaunchMode() === "local";
  const [notifications, setNotifications] = useState<NotificationRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState("");
  const [unreadOnly, setUnreadOnly] = useState(false);
  const [hasMore, setHasMore] = useState(false);

  const unreadCount = notifications.filter((n) => !n.read).length;

  const fetchNotifications = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const data = await dataClient.listNotifications({ limit: 50, unreadOnly });
      setNotifications(data);
      setHasMore(data.length === 50);
    } catch {
      setNotifications([]);
      setHasMore(false);
      setError("Failed to load notifications.");
    } finally {
      setLoading(false);
    }
  }, [unreadOnly]);

  const loadMoreNotifications = useCallback(async () => {
    const lastNotification = notifications[notifications.length - 1];
    if (!lastNotification || loadingMore) return;

    setLoadingMore(true);
    setError("");
    try {
      const data = await dataClient.listNotifications({
        limit: 50,
        unreadOnly,
        before: `${lastNotification.created_at}|${lastNotification.id}`,
      });
      setNotifications((current) => mergeMobileNotificationPages(current, data));
      setHasMore(data.length === 50);
    } catch {
      setError("Failed to load more notifications.");
    } finally {
      setLoadingMore(false);
    }
  }, [loadingMore, notifications, unreadOnly]);

  const refreshNotifications = useCallback(async () => {
    if (!shouldShowMobileNotifications(localMode)) {
      setLoading(false);
      return;
    }
    await fetchNotifications();
  }, [fetchNotifications, localMode]);

  useEffect(() => {
    void refreshNotifications();
    if (!shouldShowMobileNotifications(localMode)) return;
    const interval = setInterval(() => {
      void refreshNotifications();
    }, 30_000);
    return () => clearInterval(interval);
  }, [localMode, refreshNotifications]);

  const pullRefresh = usePullToRefresh(refreshNotifications);

  const markAllRead = async () => {
    try {
      await dataClient.markAllNotificationsRead();
      setNotifications((prev) => prev.map((n) => ({ ...n, read: true })));
      triggerHaptic("tap");
    } catch {
      setError("Failed to mark notifications as read.");
      triggerHaptic("error");
    }
  };

  const markRead = async (item: NotificationRecord) => {
    if (item.read) return;
    try {
      await dataClient.markNotificationRead(item.id);
      setNotifications((prev) =>
        prev.map((n) => (n.id === item.id ? { ...n, read: true } : n)),
      );
      triggerHaptic("tap");
    } catch {
      setError("Failed to mark notification as read.");
      triggerHaptic("error");
    }
  };

  return (
    <div className="mx-auto max-w-xl px-3 py-3" {...pullRefresh.bind}>
      <PullToRefreshHint pullDistance={pullRefresh.pullDistance} isRefreshing={pullRefresh.isRefreshing} />
      <div className="mb-3 flex items-center justify-between gap-2">
        <h2 className="text-sm font-semibold text-text">
          Notifications{unreadCount > 0 ? ` (${unreadCount} unread)` : ""}
        </h2>
        {shouldShowMobileNotifications(localMode) && (
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={() => void fetchNotifications()}
              className="flex items-center gap-1 rounded-button px-2 py-1 text-xs text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
              aria-label="Refresh notifications"
            >
              <RefreshCw size={13} />
              Refresh
            </button>
            <button
              type="button"
              onClick={() => setUnreadOnly((current) => !current)}
              className={`rounded-button border border-border px-2 py-1 text-xs transition-colors duration-100 hover:bg-surface-raised ${
                unreadOnly ? "text-primary" : "text-text-muted"
              }`}
              aria-pressed={unreadOnly}
            >
              Unread
            </button>
            {unreadCount > 0 && (
              <button
                type="button"
                onClick={markAllRead}
                className="rounded-button border border-border px-2 py-1 text-xs text-text-muted transition-colors duration-100 hover:bg-surface-raised hover:text-text"
              >
                Mark all read
              </button>
            )}
          </div>
        )}
      </div>

      {!shouldShowMobileNotifications(localMode) && (
        <div className="rounded-card border border-border bg-surface p-3 text-sm text-text-muted">
          Notifications are unavailable in local standalone mode.
        </div>
      )}

      {error && (
        <div className="mb-3 rounded-input border border-danger/30 bg-danger/10 px-3 py-2 text-sm text-danger">
          {error}
        </div>
      )}

      {shouldShowMobileNotifications(localMode) && (
        <div className="rounded-card border border-border bg-surface p-2">
          {loading && (
            <p className="px-2 py-3 text-sm text-text-muted">Loading notifications...</p>
          )}
          {!loading && notifications.length === 0 && (
            <p className="px-2 py-3 text-sm text-text-muted">No notifications.</p>
          )}
          {!loading &&
            notifications.map((item) => (
              <button
                key={item.id}
                type="button"
                onClick={() => void markRead(item)}
                className={`flex w-full items-start gap-2 rounded-button border-b border-border px-2 py-2 text-left transition-colors duration-100 last:border-b-0 hover:bg-surface-raised ${
                  item.read ? "" : "bg-primary/5"
                }`}
              >
                {!item.read && (
                  <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-primary" />
                )}
                <span className="min-w-0 flex-1">
                  <span className="block text-sm text-text">{item.title}</span>
                  {item.body && (
                    <span className="mt-0.5 block text-xs text-text-muted">{item.body}</span>
                  )}
                  <span className="mt-1 block text-[11px] text-text-muted">
                    {new Date(item.created_at).toLocaleString()}
                  </span>
                </span>
              </button>
            ))}
          {!loading && hasMore && (
            <button
              type="button"
              onClick={() => void loadMoreNotifications()}
              disabled={loadingMore}
              className="mt-2 w-full rounded-button border border-border px-2 py-2 text-sm text-primary transition-colors duration-100 hover:bg-surface-raised disabled:cursor-wait disabled:opacity-60"
            >
              {loadingMore ? "Loading…" : "Load more"}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

type MobileSettingsIcon = typeof User;

interface MobileSettingsRowProps {
  icon: MobileSettingsIcon;
  label: string;
  description?: string;
  value?: string;
  danger?: boolean;
  accent?: boolean;
  disabled?: boolean;
  onClick?: () => void;
}

function MobileSettingsRow({
  icon: Icon,
  label,
  description,
  value,
  danger = false,
  accent = false,
  disabled = false,
  onClick,
}: MobileSettingsRowProps) {
  const interactive = Boolean(onClick) && !disabled;
  const content = (
    <>
      <span
        className={`flex h-8 w-8 shrink-0 items-center justify-center rounded-card ${
          danger
            ? "!bg-danger/15 !text-danger"
            : accent
              ? "bg-primary/15 text-primary"
              : "bg-surface-raised text-text-muted"
        }`}
      >
        <Icon size={17} />
      </span>
      <span className="min-w-0 flex-1">
        <span className={`block text-sm ${danger ? "!text-danger" : accent ? "text-primary" : "text-text"}`}>{label}</span>
        {description && (
          <span className="mt-0.5 block truncate text-xs text-text-muted">{description}</span>
        )}
      </span>
      {value && <span className="max-w-[46%] truncate text-right text-sm text-text-muted">{value}</span>}
      {interactive && <ChevronRight size={17} className={`shrink-0 ${danger ? "!text-danger" : "text-text-muted"}`} />}
    </>
  );

  if (!onClick) {
    return (
      <div className="flex min-h-[50px] w-full items-center gap-3 border-b border-border/70 px-3 py-2 text-left last:border-b-0">
        {content}
      </div>
    );
  }

  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className={`flex min-h-[50px] w-full items-center gap-3 border-b border-border/70 px-3 py-2 text-left last:border-b-0 ${
        interactive ? "active:bg-surface-raised" : "cursor-default"
      } ${danger ? "!text-danger hover:bg-danger/10" : accent ? "text-primary hover:bg-primary/10" : ""} ${disabled ? "opacity-50" : ""}`}
    >
      {content}
    </button>
  );
}

function MobileSettingsGroup({ children }: { children: ReactNode }) {
  return (
    <div className="overflow-hidden rounded-card border border-border bg-surface shadow-card">
      {children}
    </div>
  );
}

const notificationChannelOptions: Array<{ value: NotificationChannel; label: string }> = [
  { value: "both", label: "In-app + email" },
  { value: "in_app", label: "In-app only" },
  { value: "email", label: "Email only" },
  { value: "none", label: "Off" },
];

export function MobilePreferenceSelect({
  value,
  disabled = false,
  ariaLabel,
  onChange,
  children,
}: {
  value: string;
  disabled?: boolean;
  ariaLabel: string;
  onChange: (value: string) => void;
  children: ReactNode;
}) {
  return (
    <Select
      value={value}
      disabled={disabled}
      onChange={(event) => onChange(event.target.value)}
      className="h-10 max-w-[48%] bg-surface px-2 text-sm"
      aria-label={ariaLabel}
    >
      {children}
    </Select>
  );
}

export function MobileTextInput({
  value,
  ariaLabel,
  placeholder,
  onChange,
  id,
  className = "",
  autoFocus = false,
  type = "text",
  autoCorrect,
  autoCapitalize,
  inputMode,
}: {
  value: string;
  ariaLabel: string;
  placeholder: string;
  onChange: (value: string) => void;
  id?: string;
  className?: InputHTMLAttributes<HTMLInputElement>["className"];
  autoFocus?: boolean;
  type?: InputHTMLAttributes<HTMLInputElement>["type"];
  autoCorrect?: InputHTMLAttributes<HTMLInputElement>["autoCorrect"];
  autoCapitalize?: InputHTMLAttributes<HTMLInputElement>["autoCapitalize"];
  inputMode?: InputHTMLAttributes<HTMLInputElement>["inputMode"];
}) {
  return (
    <Input
      id={id}
      type={type}
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className={`h-11 bg-surface px-3 text-base ${className}`}
      placeholder={placeholder}
      aria-label={ariaLabel}
      autoFocus={autoFocus}
      autoCorrect={autoCorrect}
      autoCapitalize={autoCapitalize}
      inputMode={inputMode}
    />
  );
}

export interface MobileTotpSetupPanelProps {
  setupUrl: string;
  code: string;
  recoveryCodes: string[];
  busyAction: string | null;
  onCopySetupUrl: () => void;
  onCopyRecoveryCodes: () => void;
  onCodeChange: (code: string) => void;
  onVerify: () => void;
}

export const MOBILE_TOTP_SETUP_PANEL_CLASS = "grid gap-2";
export const MOBILE_TOTP_SETUP_ACTION_GRID_CLASS = "grid gap-2 sm:grid-cols-2";
export const MOBILE_TOTP_VERIFY_GRID_CLASS = "grid gap-2 sm:grid-cols-[1fr_auto]";
export const MOBILE_TOTP_VERIFY_BUTTON_CLASS =
  "h-11 rounded-button bg-primary px-3 text-sm font-medium text-on-primary disabled:opacity-50";
export const MOBILE_TOTP_RECOVERY_CARD_CLASS =
  "rounded-card border border-border bg-surface-raised p-3";
export const MOBILE_TOTP_RECOVERY_WARNING_COPY =
  "Save these one-time codes now. Starlane only shows them during setup.";
export const MOBILE_TOTP_RECOVERY_WARNING_CLASS = "mt-1 text-xs leading-5 text-warning";
export const MOBILE_TOTP_RECOVERY_ACK_COPY = "I saved these recovery codes somewhere secure.";
export const MOBILE_TOTP_RECOVERY_ACK_CLASS = "mt-3 flex items-start gap-2 text-xs leading-5 text-text-muted";
export const MOBILE_TOTP_RECOVERY_ACK_INPUT_CLASS =
  "mt-0.5 h-4 w-4 rounded border-border bg-bg text-primary focus-ring";

export function MobileTotpSetupPanel({
  setupUrl,
  code,
  recoveryCodes,
  busyAction,
  onCopySetupUrl,
  onCopyRecoveryCodes,
  onCodeChange,
  onVerify,
}: MobileTotpSetupPanelProps) {
  const canOpenAuthenticator = isOpenableMobileTotpSetupUri(setupUrl);
  const [recoveryCodesSaved, setRecoveryCodesSaved] = useState(recoveryCodes.length === 0);
  const recoveryCodesReady = recoveryCodes.length === 0 || recoveryCodesSaved;
  const verifyDisabledReason = getMobileTotpVerifyDisabledReason(
    code,
    busyAction,
    recoveryCodesReady,
  );
  return (
    <div className={MOBILE_TOTP_SETUP_PANEL_CLASS}>
      <label className="text-xs font-medium text-text-muted" htmlFor="totp-setup-url">
        Authenticator setup URI
      </label>
      <Textarea
        id="totp-setup-url"
        value={setupUrl}
        readOnly
        rows={3}
        className="min-h-20 resize-none text-xs"
      />
      <div className={MOBILE_TOTP_SETUP_ACTION_GRID_CLASS}>
        <button
          type="button"
          onClick={onCopySetupUrl}
          className="h-10 rounded-button border border-border bg-surface-raised px-3 text-sm text-text transition-colors duration-100 active:bg-surface"
        >
          Copy setup URI
        </button>
        {canOpenAuthenticator && (
          <a
            href={setupUrl}
            className="flex h-10 items-center justify-center rounded-button border border-border bg-surface-raised px-3 text-sm text-text transition-colors duration-100 active:bg-surface"
          >
            Open in authenticator app
          </a>
        )}
      </div>
      <div className={MOBILE_TOTP_VERIFY_GRID_CLASS}>
        <Input
          value={code}
          onChange={(event) => onCodeChange(event.target.value)}
          placeholder="Authenticator code"
          aria-label="Authenticator verification code"
          inputMode="numeric"
          autoComplete="one-time-code"
          maxLength={6}
          className="h-11"
        />
        <button
          type="button"
          disabled={Boolean(verifyDisabledReason)}
          aria-describedby={verifyDisabledReason ? "totp-verify-help" : undefined}
          onClick={onVerify}
          className={MOBILE_TOTP_VERIFY_BUTTON_CLASS}
        >
          Verify
        </button>
      </div>
      {verifyDisabledReason && (
        <p id="totp-verify-help" role="status" className="text-xs leading-5 text-text-muted">
          {verifyDisabledReason}
        </p>
      )}
      {recoveryCodes.length > 0 && (
        <div className={MOBILE_TOTP_RECOVERY_CARD_CLASS}>
          <div className="flex items-start justify-between gap-3">
            <span className="min-w-0">
              <p className="text-xs font-medium text-text">Recovery codes</p>
              <p role="status" className={MOBILE_TOTP_RECOVERY_WARNING_CLASS}>
                {MOBILE_TOTP_RECOVERY_WARNING_COPY}
              </p>
            </span>
            <button
              type="button"
              onClick={onCopyRecoveryCodes}
              className="h-8 shrink-0 rounded-button border border-border bg-surface px-2 text-xs text-text transition-colors duration-100 active:bg-surface-raised"
            >
              Copy all
            </button>
          </div>
          <div className="mt-2 grid grid-cols-2 gap-1 text-xs text-text-muted">
            {recoveryCodes.map((recoveryCode) => (
              <code key={recoveryCode} className="rounded-input bg-bg px-2 py-1">
                {recoveryCode}
              </code>
            ))}
          </div>
          <label className={MOBILE_TOTP_RECOVERY_ACK_CLASS}>
            <input
              type="checkbox"
              checked={recoveryCodesSaved}
              onChange={(event) => setRecoveryCodesSaved(event.target.checked)}
              className={MOBILE_TOTP_RECOVERY_ACK_INPUT_CLASS}
            />
            <span>{MOBILE_TOTP_RECOVERY_ACK_COPY}</span>
          </label>
        </div>
      )}
    </div>
  );
}

export function sessionDeviceLabel(session: UserSessionRecord): string {
  if (session.device_name) return session.device_name;
  const userAgent = session.user_agent ?? "";
  if (/iphone/i.test(userAgent)) return "Browser on iPhone";
  if (/ipad/i.test(userAgent)) return "Browser on iPad";
  if (/android/i.test(userAgent)) return "Browser on Android device";
  if (/safari/i.test(userAgent)) return "Safari browser";
  if (/chrome|crios/i.test(userAgent)) return "Chrome browser";
  if (/firefox/i.test(userAgent)) return "Firefox browser";
  return session.session_kind === "app" ? "Starlane Notes app" : "Browser session";
}

async function resetGraphCache(): Promise<void> {
  localStorage.removeItem("mobile_active_space_id");
  clearStarredDocuments();

  if (!("indexedDB" in window)) return;

  await new Promise<void>((resolve) => {
    const request = indexedDB.deleteDatabase("starlane-graph-cache");
    request.onsuccess = () => resolve();
    request.onerror = () => resolve();
    request.onblocked = () => resolve();
  });
}

async function deleteAppDatabase(name: string): Promise<void> {
  if (!("indexedDB" in window)) return;
  await new Promise<void>((resolve) => {
    const request = indexedDB.deleteDatabase(name);
    request.onsuccess = () => resolve();
    request.onerror = () => resolve();
    request.onblocked = () => resolve();
  });
}

function reloadAfterVaultChange(): void {
  window.setTimeout(() => window.location.replace("/"), 350);
}

export function MobileProfileScreen({ scope = "all" }: { scope?: "all" | "app" | "profile" }) {
  const { user, config, logout, updateCurrentUser, updateProfile } = useAuth();
  const navigate = useNavigate();
  const { runtime } = useRuntimePlatform();
  const [storageSelection, setStorageSelectionState] = useState(() => getStorageSelection());
  const [vaultLocation, setVaultLocationState] = useState<VaultLocation | null>(null);
  const [vaultFolders, setVaultFolders] = useState<VaultFolderSummary[]>([]);
  const [newVaultName, setNewVaultName] = useState("");
  const [serverVaultUrl, setServerVaultUrl] = useState("");
  const [vaultPickerOpen, setVaultPickerOpen] = useState(false);
  const [selectedVaultFolder, setSelectedVaultFolder] = useState<VaultFolderSummary | null>(null);
  const [selectedVaultName, setSelectedVaultName] = useState("");
  const [vaultTrashConfirmOpen, setVaultTrashConfirmOpen] = useState(false);
  const [resetAppConfirmOpen, setResetAppConfirmOpen] = useState(false);
  const [profileName, setProfileName] = useState(user?.display_name ?? "");
  const [profileAvatarUrl, setProfileAvatarUrl] = useState(user?.avatar_url ?? "");
  const [profileUsername, setProfileUsername] = useState(user?.username ?? "");
  const [notificationPreferences, setNotificationPreferences] = useState<NotificationPreferenceRecord[]>([]);
  const [notificationPreferencesLoading, setNotificationPreferencesLoading] = useState(false);
  const [sessions, setSessions] = useState<UserSessionRecord[]>([]);
  const [sessionsLoading, setSessionsLoading] = useState(false);
  const [sessionRevokeTarget, setSessionRevokeTarget] = useState<UserSessionRecord | null>(null);
  const [renamingSessionId, setRenamingSessionId] = useState<string | null>(null);
  const [sessionNameDraft, setSessionNameDraft] = useState("");
  const [totpEnabled, setTotpEnabled] = useState(Boolean(user?.totp_enabled));
  const [totpSetupUrl, setTotpSetupUrl] = useState("");
  const [totpRecoveryCodes, setTotpRecoveryCodes] = useState<string[]>([]);
  const [totpCode, setTotpCode] = useState("");
  const [totpDisableCode, setTotpDisableCode] = useState("");
  const [totpDisableConfirmOpen, setTotpDisableConfirmOpen] = useState(false);
  const [passkeys, setPasskeys] = useState<PasskeyRecord[]>([]);
  const [passkeyName, setPasskeyName] = useState("");
  const [passkeyDeleteTarget, setPasskeyDeleteTarget] = useState<PasskeyRecord | null>(null);
  const [passkeysLoading, setPasskeysLoading] = useState(false);
  const [status, setStatusState] = useState<string | null>(null);
  const [error, setErrorState] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const setStatus = useCallback((message: string | null) => {
    setStatusState(message);
    if (message) {
      void recordSystemNotification({ title: "Starlane Notes", body: message }).catch(() => undefined);
    }
  }, []);
  const setError = useCallback((message: string | null) => {
    setErrorState(message);
    if (message) {
      void recordSystemNotification({ title: "Action needs attention", body: message }).catch(() => undefined);
    }
  }, []);
  const totpSetupDisabledReason = getMobileTotpSetupDisabledReason(
    totpEnabled,
    totpSetupUrl,
    busyAction,
  );
  const passkeyAddDisabledReason = getMobilePasskeyAddDisabledReason(passkeyName, busyAction);
  const passkeyDeleteDisabledReason = passkeyDeleteTarget
    ? getMobilePasskeyDeleteDisabledReason(passkeyDeleteTarget.id, busyAction)
    : null;
  const vaultLongPressTimer = useRef<number | null>(null);
  const launchMode = getCurrentLaunchMode();
  const localMode = launchMode === "local";
  const showAppSettings = scope !== "profile";
  const showProfileSettings = scope !== "app";
  const serverRequiresTwoFactor = Boolean(config?.require_2fa);
  const requireTotpEnrollment = serverRequiresTwoFactor && !totpEnabled;
  const nativeVaultAvailable = isTauri() || runtime !== "web";
  const vaultName = getVaultFolderName(vaultLocation?.path);
  const totpDisableDisabledReason = getMobileTotpDisableDisabledReason(
    totpDisableCode,
    busyAction,
    serverRequiresTwoFactor,
  );
  const hasConfiguredVault = vaultLocation?.configured !== false;
  const vaultPathLabel = hasConfiguredVault
    ? vaultLocation?.using_default
      ? "App folder"
      : vaultName
    : "No vault selected";
  const canUseNativeVault = canRefreshNativeVaultFolders(runtime, nativeVaultAvailable);
  const canUseFolderPicker = canUseNativeVaultControls(runtime);
  const activeVaultRow = hasConfiguredVault
    ? getActiveVaultSettingsRowCopy(localMode, canUseNativeVault, vaultPathLabel)
    : {
        value: "Choose / Create",
        description: "No vault selected",
      };
  const storageFolderLabel =
    runtime === "tauri-mobile"
      ? !hasConfiguredVault
        ? "Choose storage"
        : storageSelection.provider === "icloud-drive"
        ? "iCloud Drive"
        : "Files app"
      : vaultLocation?.path || "Preparing";
  const {
    showDeviceStorageControls,
    showServerSyncRow,
    showServerVaultSwitchRow,
    showResetControls,
  } = getProfileSettingsVisibility(runtime, localMode, nativeVaultAvailable);
  const shouldShowServerVaultSwitchRow = shouldShowServerVaultSwitchInAppSettings({
    showServerSyncRow,
    showServerVaultSwitchRow,
  });
  const showSiteAdministration = shouldShowMobileSiteAdministration(localMode, user?.role);

  const refreshVaultLocation = useCallback(async () => {
    if (!canUseNativeVault) return;
    try {
      const nextLocation = await getVaultLocationStatus();
      setVaultLocationState(nextLocation);
      const nextFolders = await listVaultFolders().catch(() => []);
      setVaultFolders(nextFolders);
    } catch (err) {
      setError(formatMobileActionError(err, "Unable to read storage folder."));
    }
  }, [canUseNativeVault]);

  useEffect(() => {
    void refreshVaultLocation();
  }, [refreshVaultLocation]);

  useEffect(() => {
    setProfileName(user?.display_name ?? "");
    setProfileAvatarUrl(user?.avatar_url ?? "");
    setProfileUsername(user?.username ?? "");
    setTotpEnabled(Boolean(user?.totp_enabled));
  }, [user?.avatar_url, user?.display_name, user?.totp_enabled, user?.username]);

  useEffect(() => {
    if (localMode) return;

    let canceled = false;
    setNotificationPreferencesLoading(true);
    dataClient
      .listNotificationPreferences()
      .then((preferences) => {
        if (!canceled) setNotificationPreferences(preferences);
      })
      .catch(() => {
        if (!canceled) setError("Unable to load notification preferences.");
      })
      .finally(() => {
        if (!canceled) setNotificationPreferencesLoading(false);
      });

    return () => {
      canceled = true;
    };
  }, [localMode]);

  const loadSessions = useCallback(async () => {
    if (localMode) return;
    setSessionsLoading(true);
    try {
      const nextSessions = await dataClient.listSessions();
      setSessions(nextSessions);
    } catch {
      setError("Unable to load active sessions.");
    } finally {
      setSessionsLoading(false);
    }
  }, [localMode]);

  useEffect(() => {
    void loadSessions();
  }, [loadSessions]);

  const loadPasskeys = useCallback(async () => {
    if (localMode) return;
    setPasskeysLoading(true);
    try {
      const nextPasskeys = await dataClient.listPasskeys();
      setPasskeys(nextPasskeys);
    } catch {
      setError("Unable to load passkeys.");
    } finally {
      setPasskeysLoading(false);
    }
  }, [localMode]);

  useEffect(() => {
    void loadPasskeys();
  }, [loadPasskeys]);

  const openVaultPicker = useCallback(() => {
    setVaultPickerOpen(true);
    void refreshVaultLocation();
  }, [refreshVaultLocation]);

  const runAction = useCallback(
    async (action: string, callback: () => Promise<void>) => {
      setBusyAction(action);
      setStatus(null);
      setError(null);
      try {
        await callback();
      } catch (err) {
        setError(formatMobileActionError(err));
        triggerHaptic("error");
      } finally {
        setBusyAction(null);
      }
    },
    [setError, setStatus],
  );

  const handleCreateVault = useCallback(() => {
    const name = newVaultName.trim();
    if (!name) return;

    void runAction("create-vault", async () => {
      const next = await createVaultFolder(name);
      setStorageMode("local");
      setSyncProvider("none");
      setStorageSelectionState(getStorageSelection());
      setVaultLocationState(next);
      setNewVaultName("");
      setVaultPickerOpen(false);
      setStatus("Vault created. Reloading...");
      triggerHaptic("confirm");
      reloadAfterVaultChange();
    });
  }, [newVaultName, runAction]);

  const handleAddServerVault = useCallback(() => {
    const normalizedUrl = normalizeMobileServerVaultUrl(serverVaultUrl);
    if (!normalizedUrl) {
      setError("Enter a valid server URL.");
      triggerHaptic("error");
      return;
    }

    void runAction("add-server-vault", async () => {
      window.localStorage.setItem("server_url", normalizedUrl);
      setStorageMode("server");
      setSyncProvider("none");
      setStorageSelectionState(getStorageSelection());
      setServerVaultUrl(normalizedUrl);
      setStatus("Server vault added. Reloading...");
      triggerHaptic("confirm");
      reloadAfterVaultChange();
    });
  }, [runAction, serverVaultUrl]);

  const handleSwitchVault = useCallback(
    (folder: VaultFolderSummary) => {
      if (folder.current) return;

      void runAction(`switch-vault-${folder.path}`, async () => {
        const next = await setVaultPath(folder.path);
        setStorageMode("local");
        setSyncProvider("none");
        setStorageSelectionState(getStorageSelection());
        setVaultLocationState(next);
        setVaultPickerOpen(false);
        setStatus(`Switched to ${folder.name}. Reloading...`);
        triggerHaptic("confirm");
        reloadAfterVaultChange();
      });
    },
    [runAction],
  );

  const clearVaultLongPressTimer = useCallback(() => {
    if (vaultLongPressTimer.current) {
      window.clearTimeout(vaultLongPressTimer.current);
      vaultLongPressTimer.current = null;
    }
  }, []);

  const openVaultActionDialog = useCallback((folder: VaultFolderSummary) => {
    clearVaultLongPressTimer();
    setSelectedVaultFolder(folder);
    setSelectedVaultName(folder.name);
    setVaultTrashConfirmOpen(false);
    triggerHaptic("tap");
  }, [clearVaultLongPressTimer]);

  const handleSelectedVaultRename = useCallback(() => {
    const folder = selectedVaultFolder;
    const name = selectedVaultName.trim();
    if (!folder || !name || name === folder.name) return;

    void runAction(`rename-vault-${folder.path}`, async () => {
      await renameVaultFolderAt(folder.path, name);
      setSelectedVaultFolder(null);
      setSelectedVaultName("");
      setVaultTrashConfirmOpen(false);
      await refreshVaultLocation();
      setStatus("Vault renamed.");
      triggerHaptic("confirm");
    });
  }, [refreshVaultLocation, runAction, selectedVaultFolder, selectedVaultName]);

  const handleSelectedVaultDelete = useCallback(() => {
    const folder = selectedVaultFolder;
    if (!folder) return;
    if (folder.current) {
      setError("Switch to another vault before moving the active vault to trash.");
      triggerHaptic("error");
      return;
    }
    setVaultTrashConfirmOpen(true);
  }, [selectedVaultFolder]);

  const handleConfirmSelectedVaultDelete = useCallback(() => {
    const folder = selectedVaultFolder;
    if (!folder || folder.current) return;
    void runAction(`delete-vault-${folder.path}`, async () => {
      await deleteVaultFolderAt(folder.path);
      setSelectedVaultFolder(null);
      setSelectedVaultName("");
      setVaultTrashConfirmOpen(false);
      await refreshVaultLocation();
      setStatus("Vault moved to trash.");
      triggerHaptic("confirm");
    });
  }, [refreshVaultLocation, runAction, selectedVaultFolder]);

  const handleUseDefaultVault = useCallback(() => {
    void runAction("default-vault", async () => {
      const status = vaultLocation ?? (await getVaultLocationStatus());
      const next = await setVaultPath(status.path);
      setVaultLocationState(next);
      setStatus("App folder selected. Reloading...");
      triggerHaptic("confirm");
      reloadAfterVaultChange();
    });
  }, [runAction, vaultLocation]);

  const handleOpenVault = useCallback(() => {
    void runAction("open-vault", async () => {
      if (!canUseFolderPicker) {
        setStatus(`Open Files, then On My iPhone > Starlane Notes > ${vaultName}.`);
        return;
      }
      try {
        await revealVaultFolder();
        setStatus("Storage folder opened.");
      } catch {
        setStatus(`Open Files, then On My iPhone > Starlane Notes > ${vaultName}.`);
      }
    });
  }, [canUseFolderPicker, runAction, vaultName]);

  const handleSaveProfile = useCallback(() => {
    const displayName = profileName.trim();
    if (!displayName) return;

    void runAction("save-profile", async () => {
      await updateProfile({
        display_name: displayName,
        avatar_url: profileAvatarUrl.trim() || undefined,
        username: profileUsername.trim() || undefined,
      });
      setStatus("Profile updated.");
      triggerHaptic("confirm");
    });
  }, [profileAvatarUrl, profileName, profileUsername, runAction, updateProfile]);

  const handleAvatarFileSelected = useCallback((file: File | null) => {
    if (!file) return;
    if (!file.type.startsWith("image/")) {
      setStatus("Choose an image file for your profile picture.");
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result === "string") {
        setProfileAvatarUrl(reader.result);
        setStatus("Profile picture selected. Save profile to apply it.");
      }
    };
    reader.onerror = () => setStatus("Unable to read that image.");
    reader.readAsDataURL(file);
  }, []);

  const handleNotificationPreferenceChange = useCallback(
    (notificationType: string, channel: NotificationChannel) => {
      const nextPreferences = notificationPreferences.map((preference) =>
        preference.notification_type === notificationType ? { ...preference, channel } : preference,
      );
      setNotificationPreferences(nextPreferences);

      void runAction("save-notification-preferences", async () => {
        const saved = await dataClient.updateNotificationPreferences(
          nextPreferences.map((preference) => ({
            notification_type: preference.notification_type,
            channel: preference.channel,
          })),
        );
        setNotificationPreferences(saved);
        setStatus("Notification preferences updated.");
        triggerHaptic("confirm");
      });
    },
    [notificationPreferences, runAction],
  );

  const handleRevokeSession = useCallback(
    (session: UserSessionRecord) => {
      setSessionRevokeTarget(session);
    },
    [],
  );

  const handleConfirmRevokeSession = useCallback(() => {
    const session = sessionRevokeTarget;
    if (!session) return;

      void runAction(`revoke-session-${session.id}`, async () => {
        await dataClient.revokeSession(session.id);
        setSessions((current) => current.filter((item) => item.id !== session.id));
        setSessionRevokeTarget(null);
        setStatus("Session revoked.");
        triggerHaptic("confirm");
      });
  }, [runAction, sessionRevokeTarget]);

  const closeSessionRevokeDialog = useCallback(() => setSessionRevokeTarget(null), []);

  const startSessionRename = useCallback((session: UserSessionRecord) => {
    setRenamingSessionId(session.id);
    setSessionNameDraft(session.device_name ?? sessionDeviceLabel(session));
  }, []);

  const saveSessionRename = useCallback(
    (session: UserSessionRecord) => {
      const deviceName = sessionNameDraft.trim();
      if (!deviceName) {
        setError("Enter a session name.");
        return;
      }
      void runAction(`rename-session-${session.id}`, async () => {
        const nextSessions = await dataClient.renameSession(session.id, deviceName);
        setSessions(nextSessions);
        setRenamingSessionId(null);
        setSessionNameDraft("");
        setStatus("Session name updated.");
        triggerHaptic("confirm");
      });
    },
    [runAction, sessionNameDraft],
  );

  const handleStartTotpSetup = useCallback(() => {
    const disabledReason = getMobileTotpSetupDisabledReason(totpEnabled, totpSetupUrl, busyAction);
    if (disabledReason) {
      setError(disabledReason);
      triggerHaptic("error");
      return;
    }

    void runAction("totp-setup", async () => {
      const setup = await dataClient.setupTotp();
      setTotpSetupUrl(setup.qr_code_url);
      setTotpRecoveryCodes(setup.recovery_codes);
      setTotpCode("");
      setStatus("Authenticator setup started. Add the setup URI, then enter a code to verify.");
      triggerHaptic("confirm");
    });
  }, [busyAction, runAction, totpEnabled, totpSetupUrl]);

  const handleVerifyTotp = useCallback(() => {
    const code = totpCode.trim();
    const disabledReason = getMobileTotpVerifyDisabledReason(code, busyAction);
    if (disabledReason) {
      setError(disabledReason);
      triggerHaptic("error");
      return;
    }

    void runAction("totp-verify", async () => {
      const result = await dataClient.verifyTotp(code);
      setTotpEnabled(result.totp_enabled);
      updateCurrentUser(getMobileTotpUserPatch(result.totp_enabled));
      setTotpSetupUrl("");
      setTotpCode("");
      setStatus("Two-factor authentication enabled.");
      triggerHaptic("confirm");
    });
  }, [busyAction, runAction, totpCode, updateCurrentUser]);

  const handleDisableTotp = useCallback(() => {
    const code = totpDisableCode.trim();
    const disabledReason = getMobileTotpDisableDisabledReason(
      code,
      busyAction,
      serverRequiresTwoFactor,
    );
    if (disabledReason) {
      setError(disabledReason);
      triggerHaptic("error");
      return;
    }
    setTotpDisableConfirmOpen(true);
  }, [busyAction, serverRequiresTwoFactor, totpDisableCode]);

  const handleConfirmDisableTotp = useCallback(() => {
    const code = totpDisableCode.trim();
    const disabledReason = getMobileTotpDisableDisabledReason(
      code,
      busyAction,
      serverRequiresTwoFactor,
    );
    if (disabledReason) {
      setError(disabledReason);
      triggerHaptic("error");
      return;
    }

    void runAction("totp-disable", async () => {
      const result = await dataClient.disableTotp(code);
      setTotpEnabled(result.totp_enabled);
      updateCurrentUser(getMobileTotpUserPatch(result.totp_enabled));
      setTotpDisableCode("");
      setTotpRecoveryCodes([]);
      setTotpDisableConfirmOpen(false);
      setStatus("Two-factor authentication disabled.");
      triggerHaptic("confirm");
    });
  }, [busyAction, runAction, serverRequiresTwoFactor, totpDisableCode, updateCurrentUser]);

  const closeTotpDisableDialog = useCallback(() => setTotpDisableConfirmOpen(false), []);

  const handleCopyTotpSetupUrl = useCallback(() => {
    if (!totpSetupUrl) return;
    void (navigator.clipboard?.writeText(totpSetupUrl) ??
      Promise.reject(new Error("Clipboard unavailable")))
      .then(() => setStatus("Authenticator setup URI copied."))
      .catch(() => setStatus("Copy unavailable. Select and copy the setup URI manually."));
  }, [totpSetupUrl]);

  const handleCopyTotpRecoveryCodes = useCallback(() => {
    if (totpRecoveryCodes.length === 0) return;
    void (navigator.clipboard?.writeText(totpRecoveryCodes.join("\n")) ??
      Promise.reject(new Error("Clipboard unavailable")))
      .then(() => setStatus("Recovery codes copied. Save them somewhere secure."))
      .catch(() => setStatus("Copy unavailable. Select and copy the recovery codes manually."));
  }, [totpRecoveryCodes]);

  const handleAddPasskey = useCallback(() => {
    if (!isPasskeySupported()) {
      setError("Passkeys are not supported by this browser or webview.");
      triggerHaptic("error");
      return;
    }

    const disabledReason = getMobilePasskeyAddDisabledReason(passkeyName, busyAction);
    if (disabledReason) {
      setError(disabledReason);
      triggerHaptic("error");
      return;
    }

    const name = passkeyName.trim() || "Passkey";
    void runAction("passkey-add", async () => {
      const challenge = await dataClient.startPasskeyRegistration(name);
      const credential = await createPasskeyCredential(challenge.public_key);
      const created = await dataClient.finishPasskeyRegistration(
        challenge.challenge_id,
        credential,
        name,
      );
      setPasskeys((current) => [created, ...current]);
      setPasskeyName("");
      setStatus("Passkey added.");
      triggerHaptic("confirm");
    });
  }, [busyAction, passkeyName, runAction]);

  const handleDeletePasskey = useCallback(
    (passkey: PasskeyRecord) => {
      const disabledReason = getMobilePasskeyDeleteDisabledReason(passkey.id, busyAction);
      if (disabledReason) {
        setError(disabledReason);
        triggerHaptic("error");
        return;
      }
      setPasskeyDeleteTarget(passkey);
    },
    [busyAction],
  );

  const handleConfirmDeletePasskey = useCallback(() => {
    const passkey = passkeyDeleteTarget;
    if (!passkey) return;
    const disabledReason = getMobilePasskeyDeleteDisabledReason(passkey.id, busyAction);
    if (disabledReason) {
      setError(disabledReason);
      triggerHaptic("error");
      return;
    }

    void runAction(`passkey-delete-${passkey.id}`, async () => {
      await dataClient.deletePasskey(passkey.id);
      setPasskeys((current) => current.filter((item) => item.id !== passkey.id));
      setPasskeyDeleteTarget(null);
      setStatus("Passkey deleted.");
      triggerHaptic("confirm");
    });
  }, [busyAction, passkeyDeleteTarget, runAction]);

  const closePasskeyDeleteDialog = useCallback(
    () => setPasskeyDeleteTarget(null),
    [],
  );

  const handleChooseVaultFolder = useCallback(() => {
    void runAction("choose-vault", async () => {
      if (!canUseFolderPicker) {
        setStatus("Folder picker is desktop-only on iOS right now. Use the app folder in Files.");
        return;
      }
      const selected = await chooseVaultFolder(vaultLocation?.path);
      if (!selected) {
        setStatus("Folder selection canceled.");
        return;
      }

      const next = await setVaultPath(selected);
      setVaultLocationState(next);
      setStatus("Vault folder updated. Reloading...");
      triggerHaptic("confirm");
      reloadAfterVaultChange();
    });
  }, [canUseFolderPicker, runAction, vaultLocation?.path]);

  const handleUseICloud = useCallback(() => {
    void runAction("icloud-vault", async () => {
      setStorageMode("local");
      setSyncProvider("icloud-drive");
      setStorageSelectionState(getStorageSelection());

      if (!canUseNativeVault) {
        setStatus("iCloud selected. Native iCloud vault folders are unavailable in this runtime.");
        triggerHaptic("confirm");
        return;
      }

      const next = await moveVaultToICloud(vaultName);
      setVaultLocationState(next);
      setStatus("Vault moved to iCloud. Reloading...");
      triggerHaptic("confirm");
      reloadAfterVaultChange();
    });
  }, [canUseNativeVault, runAction, vaultName]);

  const handleResetCache = useCallback(() => {
    void runAction("reset-cache", async () => {
      await resetGraphCache();
      setStatus("Cache reset. Notes and vault files were kept.");
      triggerHaptic("confirm");
    });
  }, [runAction]);

  const handleResetApp = useCallback(() => {
    setResetAppConfirmOpen(true);
  }, []);

  const handleConfirmResetApp = useCallback(() => {
    void runAction("reset-app", async () => {
      await resetAllLocalAppData();
      if (canUseNativeVault) {
        await clearVaultPathWithoutInitializing().catch(() => undefined);
      }
      await resetGraphCache();
      await deleteAppDatabase("starlane-local-vault");
      clearStorageSelection();
      localStorage.clear();
      setStatus("App reset complete. Returning to setup.");
      triggerHaptic("confirm");
      window.setTimeout(() => window.location.replace("/"), 300);
    });
  }, [canUseNativeVault, runAction]);

  return (
    <div className={getProfileSettingsFrameClass(runtime, scope)}>
      {vaultPickerOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 px-4">
          <div className="max-h-[82vh] w-full max-w-md overflow-hidden rounded-card border border-border bg-surface shadow-card">
            <div className="flex items-center justify-between border-b border-border px-3 py-2">
              <div>
                <h2 className="text-sm font-medium text-text">Vaults</h2>
                <p className="text-xs text-text-muted">
                  Create, connect, or switch the active vault.
                </p>
              </div>
              <button
                type="button"
                onClick={() => {
                  clearVaultLongPressTimer();
                  setVaultPickerOpen(false);
                }}
                className="flex h-8 w-8 items-center justify-center rounded-button text-text-muted transition-colors duration-100 active:bg-surface-raised hover:text-text"
                aria-label="Close vault picker"
              >
                <X size={14} />
              </button>
            </div>

            <div className="max-h-[60vh] overflow-y-auto p-3">
              <div className="mb-3 grid grid-cols-2 gap-2">
                <button
                  type="button"
                  onClick={() => {
                    setNewVaultName(vaultName === "App folder" ? "" : vaultName);
                    document.getElementById("mobile-new-vault-name")?.scrollIntoView({
                      block: "center",
                    });
                  }}
                  className="rounded-button border border-border bg-surface-raised px-3 py-2 text-sm text-text transition-colors duration-100 active:bg-surface"
                >
                  Create new vault
                </button>
                <button
                  type="button"
                  onClick={() => {
                    document.getElementById("mobile-server-vault-url")?.scrollIntoView({
                      block: "center",
                    });
                  }}
                  className="rounded-button border border-border bg-surface-raised px-3 py-2 text-sm text-text transition-colors duration-100 active:bg-surface"
                >
                  Connect server
                </button>
              </div>

              <div className="mb-3 rounded-card border border-border bg-surface-raised p-3">
                <p className="text-sm font-medium text-text">
                  {hasConfiguredVault ? vaultPathLabel : "No vault selected"}
                </p>
                <p className="mt-1 text-xs leading-5 text-text-muted">
                  Server vaults download to this phone before opening when supported. Choosing a
                  local vault switches this device back to standalone mode.
                </p>
              </div>

              <div className="mb-3 flex items-center justify-between gap-3">
                <p className="text-xs font-medium uppercase tracking-wide text-text-muted">Available vaults</p>
                <button
                  type="button"
                  className="text-xs text-primary"
                  disabled={busyAction === "refresh-vaults"}
                  onClick={() => void runAction("refresh-vaults", refreshVaultLocation)}
                >
                  Refresh
                </button>
              </div>

              <div className="overflow-hidden rounded-card border border-border bg-surface">
                {vaultFolders.length === 0 ? (
                  <p className="px-3 py-3 text-sm text-text-muted">No vault folders found yet.</p>
                ) : (
                  vaultFolders.map((folder) => (
                    <button
                      key={folder.path}
                      type="button"
                      disabled={busyAction === `switch-vault-${folder.path}`}
                      onClick={() =>
                        folder.current ? openVaultActionDialog(folder) : handleSwitchVault(folder)
                      }
                      onContextMenu={(event) => {
                        event.preventDefault();
                        openVaultActionDialog(folder);
                      }}
                      onTouchStart={() => {
                        clearVaultLongPressTimer();
                        vaultLongPressTimer.current = window.setTimeout(
                          () => openVaultActionDialog(folder),
                          500,
                        );
                      }}
                      onTouchEnd={clearVaultLongPressTimer}
                      onTouchCancel={clearVaultLongPressTimer}
                      className={`flex w-full items-center gap-3 border-b border-border/70 px-3 py-3 text-left last:border-b-0 ${
                        folder.current ? "bg-surface-raised" : "active:bg-surface-raised"
                      }`}
                    >
                      <Folder size={16} className="shrink-0 text-text-muted" />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm text-text">{folder.name}</span>
                        <span className="block truncate text-[11px] text-text-muted">{folder.path}</span>
                      </span>
                      <span className="text-xs text-text-muted">
                        {folder.current ? "Current" : "Switch"}
                      </span>
                    </button>
                  ))
                )}
              </div>

              <div className="mt-3 grid gap-2">
                <label className="text-xs font-medium text-text-muted" htmlFor="mobile-new-vault-name">
                  New vault
                </label>
                <div className="flex gap-2">
                  <MobileTextInput
                    id="mobile-new-vault-name"
                    value={newVaultName}
                    onChange={setNewVaultName}
                    ariaLabel="New vault name"
                    placeholder="Vault name"
                    className="min-w-0 flex-1"
                  />
                  <button
                    type="button"
                    disabled={!newVaultName.trim() || busyAction === "create-vault"}
                    onClick={handleCreateVault}
                    className="flex h-11 w-11 shrink-0 items-center justify-center rounded-button border border-border bg-surface-raised text-text disabled:opacity-50"
                    aria-label="Create vault"
                  >
                    <Plus size={16} />
                  </button>
                </div>
              </div>

              <div className="mt-4 grid gap-2 border-t border-border pt-3">
                <label className="text-xs font-medium text-text-muted" htmlFor="mobile-server-vault-url">
                  Server vault
                </label>
                <p className="text-xs leading-5 text-text-muted">
                  Connecting to a server will use the local copy first, then download from the
                  server when recovery is needed.
                </p>
                <div className="flex gap-2">
                  <MobileTextInput
                    id="mobile-server-vault-url"
                    value={serverVaultUrl}
                    onChange={setServerVaultUrl}
                    ariaLabel="Server vault URL"
                    placeholder="https://notes.example.com"
                    className="min-w-0 flex-1"
                    type="url"
                    autoCorrect="off"
                    autoCapitalize="none"
                    inputMode="url"
                  />
                  <button
                    type="button"
                    disabled={!serverVaultUrl.trim() || busyAction === "add-server-vault"}
                    onClick={handleAddServerVault}
                    className="h-11 shrink-0 rounded-button border border-border bg-surface-raised px-3 text-sm text-text disabled:opacity-50"
                  >
                    Add Server
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>
      )}

      {selectedVaultFolder && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 px-4">
          <div className="w-full max-w-sm rounded-card border border-border bg-surface shadow-card">
            <div className="flex items-center justify-between border-b border-border px-3 py-2">
              <div className="min-w-0">
                <h2 className="truncate text-sm font-medium text-text">Vault options</h2>
                <p className="truncate text-xs text-text-muted">{selectedVaultFolder.path}</p>
              </div>
              <button
                type="button"
                onClick={() => {
                  setSelectedVaultFolder(null);
                  setVaultTrashConfirmOpen(false);
                }}
                className="flex h-8 w-8 items-center justify-center rounded-button text-text-muted transition-colors duration-100 active:bg-surface-raised hover:text-text"
                aria-label="Close vault options"
              >
                <X size={14} />
              </button>
            </div>
            <div className="grid gap-3 p-3">
              <div className="grid gap-2">
                <label className="text-xs font-medium text-text-muted" htmlFor="mobile-selected-vault-name">
                  Rename vault
                </label>
                <div className="flex gap-2">
                  <MobileTextInput
                    id="mobile-selected-vault-name"
                    value={selectedVaultName}
                    onChange={setSelectedVaultName}
                    ariaLabel="Selected vault name"
                    placeholder="Vault name"
                    className="min-w-0 flex-1"
                  />
                  <button
                    type="button"
                    disabled={
                      !selectedVaultName.trim() ||
                      selectedVaultName.trim() === selectedVaultFolder.name ||
                      busyAction === `rename-vault-${selectedVaultFolder.path}`
                    }
                    onClick={handleSelectedVaultRename}
                    className="h-11 shrink-0 rounded-button border border-border bg-surface-raised px-3 text-sm text-text disabled:opacity-50"
                  >
                    Save
                  </button>
                </div>
              </div>
              <button
                type="button"
                disabled={
                  selectedVaultFolder.current ||
                  busyAction === `delete-vault-${selectedVaultFolder.path}`
                }
                onClick={handleSelectedVaultDelete}
                className="h-11 rounded-button border border-danger px-3 text-sm text-danger transition-colors duration-100 active:bg-surface-raised disabled:opacity-50"
              >
                {selectedVaultFolder.current ? "Switch away before trashing" : "Move vault to trash"}
              </button>
              {vaultTrashConfirmOpen && !selectedVaultFolder.current && (
                <div
                  role="dialog"
                  aria-labelledby="mobile-vault-trash-title"
                  className="rounded-card border border-border bg-bg p-3"
                >
                  <p id="mobile-vault-trash-title" className="text-sm font-medium text-text">
                    Move vault to trash
                  </p>
                  <p className="mt-1 text-xs leading-5 text-text-muted">
                    {getMobileVaultTrashConfirmationMessage(selectedVaultFolder.name)}
                  </p>
                  <div className="mt-3 grid grid-cols-2 gap-2">
                    <button
                      type="button"
                      disabled={busyAction === `delete-vault-${selectedVaultFolder.path}`}
                      onClick={() => setVaultTrashConfirmOpen(false)}
                      className="h-10 rounded-button border border-border bg-surface-raised px-3 text-sm text-text transition-colors duration-100 active:bg-surface disabled:opacity-50"
                    >
                      Cancel
                    </button>
                    <button
                      type="button"
                      disabled={busyAction === `delete-vault-${selectedVaultFolder.path}`}
                      onClick={handleConfirmSelectedVaultDelete}
                      className="h-10 rounded-button border border-danger px-3 text-sm text-danger transition-colors duration-100 active:bg-surface-raised disabled:opacity-50"
                    >
                      Move to trash
                    </button>
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {totpDisableConfirmOpen && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 px-4">
          <div
            aria-labelledby="mobile-totp-disable-title"
            aria-modal="true"
            role="dialog"
            className="w-full max-w-sm rounded-card border border-border bg-surface shadow-card"
          >
            <div className="flex items-center justify-between border-b border-border px-3 py-2">
              <div className="min-w-0">
                <h2 id="mobile-totp-disable-title" className="truncate text-sm font-medium text-text">
                  Disable 2FA
                </h2>
                <p className="truncate text-xs text-text-muted">Authenticator protection</p>
              </div>
              <button
                type="button"
                onClick={closeTotpDisableDialog}
                className="flex h-8 w-8 items-center justify-center rounded-button text-text-muted transition-colors duration-100 active:bg-surface-raised hover:text-text"
                aria-label="Close two-factor disable confirmation"
              >
                <X size={14} />
              </button>
            </div>
            <div className="grid gap-3 p-3">
              <p className="text-sm leading-6 text-text-muted">
                {getMobileTotpDisableConfirmationMessage()}
              </p>
              <div className="grid grid-cols-2 gap-2">
                <button
                  type="button"
                  disabled={busyAction === "totp-disable"}
                  onClick={closeTotpDisableDialog}
                  className="h-11 rounded-button border border-border bg-surface-raised px-3 text-sm text-text transition-colors duration-100 active:bg-surface disabled:opacity-50"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  disabled={busyAction === "totp-disable"}
                  onClick={handleConfirmDisableTotp}
                  className="h-11 rounded-button border border-danger px-3 text-sm text-danger transition-colors duration-100 active:bg-surface-raised disabled:opacity-50"
                >
                  Disable 2FA
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {passkeyDeleteTarget && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 px-4">
          <div
            aria-labelledby="mobile-passkey-delete-title"
            aria-modal="true"
            role="dialog"
            className="w-full max-w-sm rounded-card border border-border bg-surface shadow-card"
          >
            <div className="flex items-center justify-between border-b border-border px-3 py-2">
              <div className="min-w-0">
                <h2 id="mobile-passkey-delete-title" className="truncate text-sm font-medium text-text">
                  Delete passkey
                </h2>
                <p className="truncate text-xs text-text-muted">{passkeyDeleteTarget.name}</p>
              </div>
              <button
                type="button"
                onClick={closePasskeyDeleteDialog}
                className="flex h-8 w-8 items-center justify-center rounded-button text-text-muted transition-colors duration-100 active:bg-surface-raised hover:text-text"
                aria-label="Close passkey delete confirmation"
              >
                <X size={14} />
              </button>
            </div>
            <div className="grid gap-3 p-3">
              <p className="text-sm leading-6 text-text-muted">
                {getMobilePasskeyDeleteConfirmationMessage(passkeyDeleteTarget.name)}
              </p>
              <div className="grid grid-cols-2 gap-2">
                <button
                  type="button"
                  disabled={Boolean(passkeyDeleteDisabledReason)}
                  onClick={closePasskeyDeleteDialog}
                  className="h-11 rounded-button border border-border bg-surface-raised px-3 text-sm text-text transition-colors duration-100 active:bg-surface disabled:opacity-50"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  disabled={Boolean(passkeyDeleteDisabledReason)}
                  aria-describedby={passkeyDeleteDisabledReason ? "passkey-delete-help" : undefined}
                  onClick={handleConfirmDeletePasskey}
                  className="h-11 rounded-button border border-danger px-3 text-sm text-danger transition-colors duration-100 active:bg-surface-raised disabled:opacity-50"
                >
                  Delete passkey
                </button>
              </div>
              {passkeyDeleteDisabledReason && (
                <p id="passkey-delete-help" role="status" className="text-xs leading-5 text-text-muted">
                  {passkeyDeleteDisabledReason}
                </p>
              )}
            </div>
          </div>
        </div>
      )}

      {sessionRevokeTarget && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center bg-black/40 px-4">
          <div
            aria-labelledby="mobile-session-revoke-title"
            aria-modal="true"
            role="dialog"
            className="w-full max-w-sm rounded-card border border-border bg-surface shadow-card"
          >
            <div className="flex items-center justify-between border-b border-border px-3 py-2">
              <div className="min-w-0">
                <h2 id="mobile-session-revoke-title" className="truncate text-sm font-medium text-text">
                  Revoke session
                </h2>
                <p className="truncate text-xs text-text-muted">
                  {sessionDeviceLabel(sessionRevokeTarget)}
                </p>
              </div>
              <button
                type="button"
                onClick={closeSessionRevokeDialog}
                className="flex h-8 w-8 items-center justify-center rounded-button text-text-muted transition-colors duration-100 active:bg-surface-raised hover:text-text"
                aria-label="Close session revoke confirmation"
              >
                <X size={14} />
              </button>
            </div>
            <div className="grid gap-3 p-3">
              <p className="text-sm leading-6 text-text-muted">
                {getMobileSessionRevokeConfirmationMessage(sessionDeviceLabel(sessionRevokeTarget))}
              </p>
              <div className="grid grid-cols-2 gap-2">
                <button
                  type="button"
                  disabled={busyAction === `revoke-session-${sessionRevokeTarget.id}`}
                  onClick={closeSessionRevokeDialog}
                  className="h-11 rounded-button border border-border bg-surface-raised px-3 text-sm text-text transition-colors duration-100 active:bg-surface disabled:opacity-50"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  disabled={busyAction === `revoke-session-${sessionRevokeTarget.id}`}
                  onClick={handleConfirmRevokeSession}
                  className="h-11 rounded-button border border-danger px-3 text-sm text-danger transition-colors duration-100 active:bg-surface-raised disabled:opacity-50"
                >
                  Revoke session
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {resetAppConfirmOpen && (
        <ConfirmDialog
          labelledBy="mobile-reset-app-title"
          title="Reset app"
          message={getMobileResetAppConfirmationMessage()}
          confirmLabel="Reset app"
          destructive
          onCancel={() => setResetAppConfirmOpen(false)}
          onConfirm={() => {
            setResetAppConfirmOpen(false);
            handleConfirmResetApp();
          }}
        />
      )}

      <div className="text-center">
        <h2 className="text-base font-semibold text-text">
          {scope === "app" ? "App settings" : scope === "profile" ? "Profile settings" : "Settings"}
        </h2>
        <p className="mt-1 text-xs text-text-muted">
          {scope === "profile"
            ? "Manage your server account and delivery preferences."
            : localMode
            ? "This vault is stored on this device first."
            : runtime === "web"
              ? "Manage your server account and delivery preferences."
              : "Server sync keeps a local device copy first."}
        </p>
      </div>

      {showProfileSettings && !localMode && (
        <MobileSettingsGroup>
          <div className="border-b border-border/70 px-3 py-3">
            <div className="mb-2 flex items-center gap-3">
              <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-card bg-surface-raised text-text-muted">
                <User size={17} />
              </span>
              <div className="min-w-0">
                <p className="truncate text-sm text-text">Profile</p>
                <p className="truncate text-xs text-text-muted">{user?.email || "Server account"}</p>
              </div>
            </div>
            <div className="grid gap-2">
              <div className="flex items-center gap-3">
                {profileAvatarUrl ? (
                  <img
                    src={profileAvatarUrl}
                    alt=""
                    className="h-12 w-12 rounded-full border border-border object-cover"
                  />
                ) : (
                  <span className="flex h-12 w-12 items-center justify-center rounded-full border border-border bg-surface-raised text-sm font-medium text-text-muted">
                    {profileName.trim()[0]?.toUpperCase() || user?.display_name?.[0]?.toUpperCase() || "U"}
                  </span>
                )}
                <label className="inline-flex h-10 cursor-pointer items-center rounded-button border border-border bg-surface px-3 text-sm text-text transition-colors duration-100 hover:bg-surface-raised">
                  Upload picture
                  <input
                    type="file"
                    accept="image/*"
                    className="sr-only"
                    onChange={(event) => {
                      handleAvatarFileSelected(event.target.files?.[0] ?? null);
                      event.currentTarget.value = "";
                    }}
                  />
                </label>
              </div>
              <MobileTextInput
                value={profileName}
                onChange={setProfileName}
                placeholder="Display name"
                ariaLabel="Display name"
              />
              <MobileTextInput
                value={profileUsername}
                onChange={setProfileUsername}
                placeholder="Username (optional)"
                ariaLabel="Username"
              />
              <MobileTextInput
                value={profileAvatarUrl}
                onChange={setProfileAvatarUrl}
                placeholder="Avatar URL"
                ariaLabel="Avatar URL"
              />
              <button
                type="button"
                disabled={!profileName.trim() || busyAction === "save-profile"}
                onClick={handleSaveProfile}
                className="h-11 rounded-button border border-border bg-surface-raised px-3 text-sm text-text disabled:opacity-50"
              >
                Save profile
              </button>
            </div>
          </div>
          <MobileSettingsRow
            icon={User}
            label={user?.display_name || "Signed in"}
            value={user?.email || "Server account"}
          />
          <MobileSettingsRow icon={Shield} label="Role" value={user?.role || "Member"} />
          <MobileSettingsRow icon={LogOut} label="Sign out" danger onClick={() => logout()} />
        </MobileSettingsGroup>
      )}

      {showProfileSettings && !localMode && (
        <MobileSettingsGroup>
          <div className="border-b border-border/70 px-3 py-3">
            <div className="flex items-start gap-3">
              <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-card bg-surface-raised text-text-muted">
                <Shield size={17} />
              </span>
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm text-text">Security</p>
                <p className="mt-0.5 text-xs leading-5 text-text-muted">
                  Protect this server account with an authenticator app and passkeys.
                </p>
              </div>
            </div>
          </div>
          <div className="grid gap-3 px-3 py-3">
            <div className="flex items-center justify-between gap-3">
              <span className="min-w-0">
                <span className="block text-sm text-text">Two-factor authentication</span>
                <span className="block text-xs text-text-muted">
                  {totpEnabled ? "Enabled for password sign-in." : "Not enabled yet."}
                </span>
              </span>
              <span
                className={`shrink-0 rounded-full border px-2 py-1 text-xs ${
                  totpEnabled
                    ? "border-success/30 bg-success/10 text-success"
                    : "border-border bg-surface-raised text-text-muted"
                }`}
              >
                {totpEnabled ? "Enabled" : "Off"}
              </span>
            </div>

            {requireTotpEnrollment && (
              <div role="status" className={MOBILE_REQUIRED_2FA_POLICY_NOTICE_CLASS}>
                {MOBILE_REQUIRED_2FA_ENROLLMENT_NOTICE_COPY}
              </div>
            )}

            {!totpEnabled && !totpSetupUrl && (
              <>
                <button
                  type="button"
                  disabled={Boolean(totpSetupDisabledReason)}
                  aria-describedby={totpSetupDisabledReason ? "totp-setup-help" : undefined}
                  onClick={handleStartTotpSetup}
                  className="h-11 rounded-button border border-border bg-surface-raised px-3 text-sm text-text transition-colors duration-100 active:bg-surface disabled:opacity-50"
                >
                  Set up authenticator app
                </button>
                {totpSetupDisabledReason && (
                  <p id="totp-setup-help" role="status" className="text-xs leading-5 text-text-muted">
                    {totpSetupDisabledReason}
                  </p>
                )}
              </>
            )}

            {!totpEnabled && totpSetupUrl && (
              <MobileTotpSetupPanel
                setupUrl={totpSetupUrl}
                code={totpCode}
                recoveryCodes={totpRecoveryCodes}
                busyAction={busyAction}
                onCopySetupUrl={handleCopyTotpSetupUrl}
                onCopyRecoveryCodes={handleCopyTotpRecoveryCodes}
                onCodeChange={setTotpCode}
                onVerify={handleVerifyTotp}
              />
            )}

            {totpEnabled && serverRequiresTwoFactor && (
              <p
                role="status"
                className={MOBILE_REQUIRED_2FA_POLICY_NOTICE_CLASS}
              >
                {totpDisableDisabledReason}
              </p>
            )}

            {totpEnabled && !serverRequiresTwoFactor && (
              <div className="grid gap-2 sm:grid-cols-[1fr_auto]">
                <Input
                  value={totpDisableCode}
                  onChange={(event) => setTotpDisableCode(event.target.value)}
                  placeholder="123456 or 1234-5678"
                  aria-label="Authenticator or recovery code to disable two-factor authentication"
                  inputMode="text"
                  autoComplete="one-time-code"
                  autoCapitalize="none"
                  autoCorrect="off"
                  maxLength={9}
                  className="h-11"
                />
                <button
                  type="button"
                  disabled={Boolean(totpDisableDisabledReason)}
                  aria-describedby={totpDisableDisabledReason ? "totp-disable-help" : undefined}
                  onClick={handleDisableTotp}
                  className="h-11 rounded-button border border-danger px-3 text-sm text-danger transition-colors duration-100 active:bg-danger/10 disabled:opacity-50"
                >
                  Disable 2FA
                </button>
                {totpDisableDisabledReason && (
                  <p id="totp-disable-help" role="status" className="text-xs leading-5 text-text-muted sm:col-span-2">
                    {totpDisableDisabledReason}
                  </p>
                )}
              </div>
            )}

            <div className="border-t border-border/70 pt-3">
              <div className="flex items-start gap-3">
                <span className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-card bg-surface-raised text-text-muted">
                  <KeyRound size={16} />
                </span>
                <div className="min-w-0 flex-1">
                  <p className="text-sm text-text">Passkeys</p>
                  <p className="mt-0.5 text-xs leading-5 text-text-muted">
                    Use Face ID, Touch ID, device unlock, or a security key for server sign-in.
                  </p>
                </div>
              </div>

              <div className="mt-3 grid gap-2">
                {passkeysLoading ? (
                  <p className="text-sm text-text-muted">Loading passkeys...</p>
                ) : passkeys.length === 0 ? (
                  <p className="rounded-card border border-border bg-bg px-3 py-2 text-xs text-text-muted">
                    No passkeys added yet.
                  </p>
                ) : (
                  passkeys.map((passkey) => {
                    return (
                      <MobilePasskeyRow
                        key={passkey.id}
                        passkey={passkey}
                        busyAction={busyAction}
                        onDelete={handleDeletePasskey}
                      />
                    );
                  })
                )}

                {isPasskeySupported() ? (
                  <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
                    <Input
                      value={passkeyName}
                      onChange={(event) => setPasskeyName(event.target.value)}
                      placeholder="Passkey name"
                      aria-label="Passkey name"
                      aria-describedby="passkey-name-help"
                      autoComplete="off"
                      maxLength={80}
                      className="h-11"
                    />
                    <button
                      type="button"
                      disabled={Boolean(passkeyAddDisabledReason)}
                      aria-describedby={passkeyAddDisabledReason ? "passkey-add-help" : undefined}
                      onClick={handleAddPasskey}
                      className="h-11 rounded-button border border-border bg-surface-raised px-3 text-sm text-text transition-colors duration-100 active:bg-surface disabled:opacity-50"
                    >
                      Add passkey
                    </button>
                    <p id="passkey-name-help" className="text-xs leading-5 text-text-muted sm:col-span-2">
                      Passkey names can be up to 80 characters.
                    </p>
                    {passkeyAddDisabledReason && (
                      <p id="passkey-add-help" role="status" className="text-xs leading-5 text-text-muted sm:col-span-2">
                        {passkeyAddDisabledReason}
                      </p>
                    )}
                  </div>
                ) : (
                  <ProfilePasskeyUnavailableNotice />
                )}
              </div>
            </div>
          </div>
        </MobileSettingsGroup>
      )}

      {showProfileSettings && !localMode && (
        <MobileSettingsGroup>
          <div className="border-b border-border/70 px-3 py-3">
            <div className="flex items-center gap-3">
              <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-card bg-surface-raised text-text-muted">
                <Bell size={17} />
              </span>
              <div className="min-w-0">
                <p className="truncate text-sm text-text">Notification delivery</p>
                <p className="truncate text-xs text-text-muted">Choose inbox and email delivery by event.</p>
              </div>
            </div>
          </div>
          {notificationPreferencesLoading ? (
            <p className="px-3 py-3 text-sm text-text-muted">Loading notification preferences...</p>
          ) : (
            notificationPreferences.map((preference) => (
              <label
                key={preference.notification_type}
                className="flex min-h-[54px] items-center gap-3 border-b border-border/70 px-3 py-2 last:border-b-0"
              >
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm text-text">{preference.label}</span>
                </span>
                <MobilePreferenceSelect
                  value={preference.channel}
                  disabled={busyAction === "save-notification-preferences"}
                  onChange={(value) =>
                    handleNotificationPreferenceChange(
                      preference.notification_type,
                      value as NotificationChannel,
                    )
                  }
                  ariaLabel={`${preference.label} delivery`}
                >
                  {notificationChannelOptions.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </MobilePreferenceSelect>
              </label>
            ))
          )}
        </MobileSettingsGroup>
      )}

      {showProfileSettings && !localMode && (
        <MobileSettingsGroup>
          <div className="border-b border-border/70 px-3 py-3">
            <div className="flex items-center justify-between gap-3">
              <div className="flex min-w-0 items-center gap-3">
                <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-card bg-surface-raised text-text-muted">
                  <Shield size={17} />
                </span>
                <div className="min-w-0">
                  <p className="truncate text-sm text-text">Active sessions</p>
                  <p className="truncate text-xs text-text-muted">Review and revoke signed-in devices.</p>
                </div>
              </div>
              <button
                type="button"
                disabled={sessionsLoading}
                onClick={() => void loadSessions()}
                className="flex h-8 w-8 shrink-0 items-center justify-center rounded-button text-text-muted transition-colors duration-100 active:bg-surface-raised hover:text-text disabled:opacity-50"
                aria-label="Refresh sessions"
              >
                <RefreshCw size={14} />
              </button>
            </div>
          </div>
          {sessionsLoading ? (
            <p className="px-3 py-3 text-sm text-text-muted">Loading sessions...</p>
          ) : sessions.length === 0 ? (
            <p className="px-3 py-3 text-sm text-text-muted">No active sessions.</p>
          ) : (
            sessions.map((session) => (
              <div
                key={session.id}
                className="border-b border-border/70 px-3 py-2 last:border-b-0"
              >
                <div className="flex min-h-[64px] items-center gap-3">
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm text-text">{sessionDeviceLabel(session)}</span>
                    <span className="block truncate text-xs text-text-muted">
                      {session.session_kind === "app" ? "App session" : "Browser session"} · {new Date(session.last_seen_at).toLocaleString()}
                    </span>
                    <span className="block truncate text-[11px] text-text-muted">
                      {session.ip_address || "Unknown IP"} · expires {new Date(session.expires_at).toLocaleDateString()}
                    </span>
                  </span>
                  <div className="flex shrink-0 gap-2">
                    <button
                      type="button"
                      disabled={busyAction === `rename-session-${session.id}`}
                      onClick={() => startSessionRename(session)}
                      className="flex h-9 w-9 items-center justify-center rounded-button border border-border text-text-muted transition-colors duration-100 active:bg-surface-raised hover:text-text disabled:opacity-50"
                      aria-label={`Rename ${sessionDeviceLabel(session)}`}
                    >
                      <Pencil size={14} />
                    </button>
                    <button
                      type="button"
                      disabled={busyAction === `revoke-session-${session.id}`}
                      onClick={() => handleRevokeSession(session)}
                      className="h-9 rounded-button border border-danger px-2 text-xs text-danger transition-colors duration-100 active:bg-surface-raised disabled:opacity-50"
                    >
                      Revoke
                    </button>
                  </div>
                </div>
                {renamingSessionId === session.id && (
                  <div className="mt-2 flex gap-2 border-t border-border/70 pt-2">
                    <Input
                      value={sessionNameDraft}
                      onChange={(event) => setSessionNameDraft(event.target.value)}
                      maxLength={80}
                      aria-label="Session name"
                    />
                    <button
                      type="button"
                      disabled={busyAction === `rename-session-${session.id}`}
                      onClick={() => saveSessionRename(session)}
                      className="h-9 shrink-0 rounded-button border border-primary px-3 text-xs text-primary transition-colors duration-100 active:bg-surface-raised disabled:opacity-50"
                    >
                      Save
                    </button>
                    <button
                      type="button"
                      onClick={() => { setRenamingSessionId(null); setSessionNameDraft(""); }}
                      className="h-9 shrink-0 rounded-button border border-border px-3 text-xs text-text-muted transition-colors duration-100 active:bg-surface-raised"
                    >
                      Cancel
                    </button>
                  </div>
                )}
              </div>
            ))
          )}
        </MobileSettingsGroup>
      )}

      {showAppSettings && showDeviceStorageControls && (
        <MobileSettingsGroup>
          <MobileSettingsRow
            icon={HardDrive}
            label={localMode ? "Active vault" : "Device copy"}
            description={localMode ? activeVaultRow.description : undefined}
            value={activeVaultRow.value}
            onClick={localMode && canUseNativeVault ? openVaultPicker : undefined}
          />
          <MobileSettingsRow icon={Database} label="Vault location" value={storageFolderLabel} />
          {localMode && canUseFolderPicker && (
            <MobileSettingsRow
              icon={FolderOpen}
              label="Choose folder"
              disabled={busyAction === "choose-vault"}
              onClick={handleChooseVaultFolder}
            />
          )}
          {shouldShowProfileOpenFolderRow(canUseFolderPicker, vaultLocation) && (
            <MobileSettingsRow
              icon={FolderOpen}
              label="Open folder"
              onClick={handleOpenVault}
            />
          )}
          {localMode && (
            <MobileSettingsRow
              icon={Cloud}
              label="Use iCloud"
              value={storageSelection.provider === "icloud-drive" ? "Selected" : undefined}
              disabled={!canUseNativeVault || busyAction === "icloud-vault"}
              onClick={handleUseICloud}
            />
          )}
          {shouldShowProfileUseAppFolderRow(localMode) && (
            <MobileSettingsRow
              icon={RotateCcw}
              label="Switch to app folder"
              value={vaultLocation?.configured === true && vaultLocation?.using_default ? "Selected" : undefined}
              disabled={
                !canUseNativeVault ||
                busyAction === "default-vault" ||
                (vaultLocation?.configured === true && vaultLocation?.using_default === true)
              }
              onClick={handleUseDefaultVault}
            />
          )}
        </MobileSettingsGroup>
      )}

      {showAppSettings && (showServerSyncRow || shouldShowServerVaultSwitchRow) && (
        <MobileSettingsGroup>
          {showServerSyncRow && <MobileSettingsRow icon={Wifi} label="Sync" value="Server" />}
          {shouldShowServerVaultSwitchRow && (
            <MobileSettingsRow
              icon={HardDrive}
              label={SERVER_MODE_VAULT_SWITCH_ROW_COPY.label}
              description={SERVER_MODE_VAULT_SWITCH_ROW_COPY.description}
              value={SERVER_MODE_VAULT_SWITCH_ROW_COPY.value}
              onClick={openVaultPicker}
            />
          )}
        </MobileSettingsGroup>
      )}

      {showAppSettings && showSiteAdministration && (
        <MobileSettingsGroup>
          <MobileSettingsRow
            icon={Shield}
            label="Site administration"
            description="Manage users, access, authentication, and server settings."
            value="Manage"
            onClick={() => navigate("/admin")}
          />
        </MobileSettingsGroup>
      )}

      {showAppSettings && (
        <MobileSettingsGroup>
          <div className="flex min-h-[50px] items-center gap-3 border-b border-border/70 px-3 py-2">
            <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-card bg-surface-raised text-text-muted">
              <Info size={17} />
            </span>
            <span className="min-w-0 flex-1 text-sm text-text">Appearance</span>
            <ThemeModeSwitch className="shrink-0" />
          </div>
          {showResetControls && (
            <>
              <MobileSettingsRow
                icon={Trash2}
                label="Reset cache"
                accent
                disabled={busyAction === "reset-cache"}
                onClick={handleResetCache}
              />
              <MobileSettingsRow
                icon={Trash2}
                label="Reset app"
                danger
                disabled={busyAction === "reset-app"}
                onClick={handleResetApp}
              />
            </>
          )}
        </MobileSettingsGroup>
      )}

      {(status || error || busyAction) && (
        <div
          className={`rounded-card border px-3 py-2 text-xs ${
            error
              ? "border-danger/30 bg-danger/10 text-danger"
              : "border-border bg-surface text-text-muted"
          }`}
        >
          {busyAction ? "Working..." : error || status}
        </div>
      )}
    </div>
  );
}
