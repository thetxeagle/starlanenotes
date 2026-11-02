import { buildPageCatalog, PAGE_OPERATION_NODE_LIMIT } from "@/lib/pageHierarchy";
import {
  buildFolderCatalog,
  FOLDER_OPERATION_NODE_LIMIT,
  buildFolderIndex,
  createFolderTraversalBudget,
  getFolderAncestors,
  validateFolderDestination,
} from "@/lib/folderHierarchy";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type InputHTMLAttributes,
  type ReactNode,
  type TouchEvent,
} from "react";
import { useNavigate } from "react-router-dom";
import {
  Bell,
  Boxes,
  CalendarDays,
  Check,
  ChevronDown,
  ChevronRight,
  Cloud,
  Database,
  FileText,
  Folder,
  FolderPlus,
  FolderOpen,
  Globe2,
  HardDrive,
  Info,
  KeyRound,
  Loader2,
  LogOut,
  Monitor,
  Pencil,
  Plus,
  RefreshCw,
  RotateCcw,
  Settings,
  Shield,
  ShieldCheck,
  Smartphone,
  Star,
  Trash2,
  User,
  Wifi,
  X,
} from "lucide-react";
import {
  impactFeedback,
  notificationFeedback,
  selectionFeedback,
  vibrate,
} from "@tauri-apps/plugin-haptics";
import {
  dataClient,
  resetAllLocalAppData,
  type NotificationChannel,
  type NotificationPreferenceRecord,
  type NotificationRecord,
  type PasskeyRecord,
  type TemplateRecord,
  type UserSessionRecord,
} from "@/lib/dataClient";
import {
  DEFAULT_SPACE_ROLE_DEFINITIONS,
  SPACE_PERMISSION_LABELS,
  getSpaceRoleLabel,
  type SpaceRoleDefinition,
} from "@/lib/spacePermissions";
import { createPasskeyCredential, isPasskeySupported } from "@/lib/passkeys";
import { ContentIdentityIcon } from "@/components/ContentIdentityIcon";
import { emitGraphDirty } from "@/lib/graphEvents";
import { buildDocumentRoutePath } from "@/lib/lastViewedDocument";
import { api, getApiBase, isTauri } from "@/lib/api";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import {
  getOfflineServerWorkspaceStatus,
  OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT,
  serverCacheScopeForSession,
} from "@/lib/offlineServerCache";
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
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { SearchModal } from "@/components/SearchModal";
import { Textarea } from "@/components/ui/textarea";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { DialogShell } from "@/components/ui/dialog-shell";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Badge } from "@/components/ui/badge";
import { TemplatePickerDialog } from "@/components/TemplatePickerDialog";
import { AvatarCropDialog } from "@/components/settings/AvatarCropDialog";
import { MobileActionSheet } from "@/components/ui/mobile-action-sheet";
import {
  findDailyTemplate,
  findDailyFolder,
  DAILY_FOLDER_NAME,
  formatDateStamp,
  templatePresentationAsRecord,
  templatePropertiesAsRecord,
} from "@/lib/templates";
import type { PropertySchemaRecord } from "@/lib/dataClient";
import {
  getTemplateExperience,
  type TemplateFieldValues,
} from "@/lib/templateExperience";
import { prepareTemplateCreation } from "@/lib/templateCreation";
import { applyTemplatePageWidth } from "@/lib/templatePresentationContent";
import { useAuth } from "@/hooks/useAuth";
import { useRuntimePlatform } from "@/hooks/useRuntimePlatform";
import type { RuntimePlatform } from "@/hooks/useRuntimePlatform";
import { getPersonalSpaceOwnerLabel } from "@/lib/spaceLabels";
import { recordSystemNotification } from "@/lib/systemNotifications";
import { STARLANE_GITHUB_REPOSITORY, STARLANE_VERSION } from "@/lib/appMetadata";
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
  space_id?: string;
  id: string;
  name: string;
  icon?: string;
  parent_id?: string;
}

export const MOBILE_TREE_TOGGLE_LANE_CLASS = "flex h-8 w-5 shrink-0 items-center justify-center";
export const MOBILE_TREE_ICON_SIZE = 14;

function createDocumentSourceId(): string {
  return (
    globalThis.crypto?.randomUUID?.() ??
    `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
  );
}

interface DocItem {
  space_id?: string;
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
  templateSchemas: PropertySchemaRecord[];
  templatesError?: boolean;
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

export function getMobileDailyDocuments(documents: DocItem[], folders: FolderItem[]): DocItem[] {
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
  scope: "all" | "app" | "account" | "profile" | "notifications" | "roles" | "spaces" | "security",
): string {
  if (runtime === "tauri-mobile") {
    return "mx-auto max-w-xl space-y-5 px-3 py-3 pb-8";
  }

  if (
    scope === "account" ||
    scope === "profile" ||
    scope === "notifications" ||
    scope === "roles" ||
    scope === "spaces" ||
    scope === "security"
  ) {
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
  const safeId = passkeyId
    .trim()
    .replace(/[^a-zA-Z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return `mobile-passkey-delete-${safeId || "passkey"}-help`;
}

export function getMobilePasskeyDateLabel(
  passkey: Pick<PasskeyRecord, "created_at" | "last_used_at">,
): string {
  const dateValue = passkey.last_used_at || passkey.created_at;
  const parsed = new Date(dateValue);
  const dateLabel = Number.isNaN(parsed.getTime()) ? "Unknown date" : parsed.toLocaleDateString();
  return passkey.last_used_at ? `Last used ${dateLabel}` : `Added ${dateLabel}`;
}

export type MobilePasskeyManagementMode = "in-app" | "browser" | "unavailable";

export function getMobilePasskeyManagementMode(
  nativeApp: boolean,
  passkeysSupported: boolean,
): MobilePasskeyManagementMode {
  if (nativeApp) return "browser";
  return passkeysSupported ? "in-app" : "unavailable";
}

export const NATIVE_PASSKEY_BROWSER_MANAGEMENT_COPY =
  "Passkey enrollment and deletion happen in your Starlane server's web browser. Sign in there with your password and two-factor code to manage passkeys; this app uses browser sign-in to use them.";

export function ProfilePasskeyBrowserManagedNotice() {
  return (
    <p
      role="status"
      className="rounded-card border-border bg-surface-raised text-text-muted border px-3 py-2 text-xs leading-5"
    >
      {NATIVE_PASSKEY_BROWSER_MANAGEMENT_COPY}
    </p>
  );
}

export function ProfilePasskeyUnavailableNotice() {
  return (
    <p
      role="status"
      className="rounded-card border-border bg-surface-raised text-text-muted border px-3 py-2 text-xs leading-5"
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
    <div className="rounded-card border-border bg-bg grid gap-2 border px-3 py-2">
      <div className="flex min-h-[48px] items-center gap-3">
        <span className="min-w-0 flex-1">
          <span className="text-text block truncate text-sm">{passkey.name}</span>
          <span className="text-text-muted block truncate text-xs">{dateLabel}</span>
        </span>
        <button
          type="button"
          disabled={Boolean(deleteDisabledReason)}
          aria-describedby={deleteDisabledReason ? helpId : undefined}
          onClick={() => onDelete(passkey)}
          className="rounded-button border-danger text-danger active:bg-danger/10 h-9 shrink-0 border px-2 text-xs transition-colors duration-100 disabled:opacity-50"
        >
          Delete
        </button>
      </div>
      {deleteDisabledReason && (
        <p id={helpId} role="status" className="text-text-muted text-xs leading-5">
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
  return ["New page", "New folder"];
}

export interface MobileDocumentCreationTarget {
  folderId?: string;
  parentId?: string;
}

export function getMobileTemplatePickerTarget(
  target: MobileDocumentCreationTarget,
  title: string,
): MobileDocumentCreationTarget & { title: string } {
  return {
    ...target,
    title: title.trim(),
  };
}

/** Keep the current mobile creation input until the page has a confirmed identity. */
export async function settleMobilePageCreation(create: () => Promise<boolean | undefined>, confirmed: () => void, failed: (message: string) => void): Promise<boolean> {
  try {
    if (await create()) { confirmed(); return true; }
  } catch { /* Retain the pending input for both rejected and uncertain outcomes. */ }
  failed("Page creation was not confirmed. Your title and destination are kept. Check the page list before retrying.");
  return false;
}

interface MobileSpaceNameDialogProps {
  error?: string;
  pending?: boolean;
  title: string;
  description: string;
  inputId: string;
  inputLabel: string;
  inputPlaceholder: string;
  actionLabel: string;
  value: string;
  onChange: (value: string) => void;
  onClose: () => void;
  onSubmit: () => void;
}

export function MobileSpaceNameDialog({
  error,
  pending = false,
  title,
  description,
  inputId,
  inputLabel,
  inputPlaceholder,
  actionLabel,
  value,
  onChange,
  onClose,
  onSubmit,
}: MobileSpaceNameDialogProps) {
  const titleId = `${inputId}-title`;
  const canSubmit = Boolean(value.trim());

  return (
    <DialogShell labelledBy={titleId} maxWidth="sm" onClose={onClose}>
      <form
        className="p-3"
        onSubmit={(event) => {
          event.preventDefault();
          if (canSubmit && !pending) onSubmit();
        }}
      >
        <div className="border-border mb-3 flex items-center justify-between border-b pb-2">
          <div>
            <h2 id={titleId} className="text-text text-sm font-medium">
              {title}
            </h2>
            <p className="text-text-muted text-xs">{description}</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-button text-text-muted active:bg-surface-raised hover:text-text flex h-8 w-8 items-center justify-center transition-colors duration-100"
            aria-label="Close"
          >
            <X size={14} />
          </button>
        </div>
        <div className="grid gap-3">
          <MobileTextInput
            disabled={pending}
            id={inputId}
            value={value}
            onChange={onChange}
            placeholder={inputPlaceholder}
            ariaLabel={inputLabel}
            className="text-sm"
            autoFocus
          />
          {error && <p role="alert" className="text-danger text-sm">{error}</p>}
          <button
            type="submit"
            disabled={!canSubmit || pending}
            className="rounded-button bg-primary text-on-primary h-10 px-3 text-sm font-medium transition-opacity duration-100 disabled:opacity-50"
          >
            {actionLabel}
          </button>
        </div>
      </form>
    </DialogShell>
  );
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

export function sortMobileFolders<T extends FolderItem>(folders: T[]): T[] {
  return [...folders].sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
  );
}

export function sortMobileSpaces(spaces: Space[]): Space[] {
  return [...spaces].sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
  );
}

export function sortMobileDocuments<T extends DocItem>(documents: T[]): T[] {
  return [...documents].sort((a, b) =>
    a.title.localeCompare(b.title, undefined, { sensitivity: "base" }),
  );
}

export function createMobilePageCatalog(documents: DocItem[], spaceId = "mobile-page-scope") {
  const catalog = buildPageCatalog(
    documents
      .map((row) => ({ ...row, space_id: row.space_id ?? spaceId }))
      .filter((row) => row.space_id === spaceId),
  );
  for (const [id, rows] of catalog.children) catalog.children.set(id, sortMobileDocuments(rows));
  return catalog;
}
type MobilePageCatalog = ReturnType<typeof createMobilePageCatalog>;

export function flattenFolderChildren(folders: FolderItem[], parentId?: string): FolderItem[] {
  return sortMobileFolders(folders.filter((f) => f.parent_id === parentId));
}

export function createMobileFolderCatalog(folders: FolderItem[], spaceId = "mobile-folder-scope") {
  const catalog = buildFolderCatalog(
    folders.map((row) => ({ ...row, space_id: row.space_id ?? spaceId })),
  );
  for (const [id, rows] of catalog.children) catalog.children.set(id, sortMobileFolders(rows));
  return catalog;
}
type MobileFolderCatalog = ReturnType<typeof createMobileFolderCatalog>;
function mobileFolderContext(folders: FolderItem[], folderId: string) {
  const spaceId = folders.find((row) => row.id === folderId)?.space_id ?? "mobile-folder-scope";
  return {
    spaceId,
    index: buildFolderIndex(folders.map((row) => ({ ...row, space_id: row.space_id ?? spaceId }))),
  };
}
export function getMobileFolderDepth(folders: FolderItem[], folderId: string): number {
  try {
    const { spaceId, index } = mobileFolderContext(folders, folderId);
    return getFolderAncestors(index, spaceId, folderId).length;
  } catch {
    return 0;
  }
}
export function canCreateMobileChildFolder(folders: FolderItem[], folderId: string): boolean {
  try {
    const { spaceId, index } = mobileFolderContext(folders, folderId),
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
    <div className="text-text-muted mb-2 flex items-center justify-center text-xs">
      <RefreshCw size={12} className={`mr-1 ${isRefreshing ? "animate-spin" : ""}`} />
      {isRefreshing
        ? "Refreshing..."
        : pullDistance >= 42
          ? "Release to refresh"
          : "Pull to refresh"}
    </div>
  );
}

export function MobileSpacesScreen() {
  const navigate = useNavigate();
  const { user } = useAuth();
  const [spaces, setSpaces] = useState<Space[]>([]);
  const [spaceDataMap, setSpaceDataMap] = useState<Record<string, SpaceData>>({});
  const dailyCreateSourceIdsRef = useRef(new Map<string, string>());
  const [activeSpaceId, setActiveSpaceId] = useState<string | null>(null);
  const [starredDocuments, setStarredDocuments] = useState(readStarredDocuments);
  const [expandedFolders, setExpandedFolders] = useState<Record<string, boolean>>({});
  const [expandedDocuments, setExpandedDocuments] = useState<Record<string, boolean>>({});
  const [templatePickerTarget, setTemplatePickerTarget] = useState<{
    folderId?: string;
    parentId?: string;
    title: string;
    date?: Date;
    createDailyFolder?: boolean;
    initialTemplateId?: string;
  } | null>(null);
  const [newDocumentTarget, setNewDocumentTarget] = useState<MobileDocumentCreationTarget | null>(
    null,
  );
  const [newDocumentTitle, setNewDocumentTitle] = useState("");
  const [newDocumentError, setNewDocumentError] = useState("");
  const [newDocumentPending, setNewDocumentPending] = useState(false);
  const newDocumentContextRef = useRef({ target: newDocumentTarget, space: activeSpaceId });
  newDocumentContextRef.current = { target: newDocumentTarget, space: activeSpaceId };
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
        loadedSpaces.find((space) => space.id === storedSpaceId)?.id || loadedSpaces[0]?.id || null;
      setActiveSpaceId(defaultSpaceId);

      const entries = await Promise.all(
        loadedSpaces.map(async (space) => {
          const [folders, documents, templateResult, templateSchemas] = await Promise.all([
            dataClient.listFolders(space.id).catch(() => []),
            dataClient.listDocuments(space.id, { include_nested: true }).catch(() => []),
            dataClient
              .listTemplates(space.id)
              .then((value) => ({ value, error: false }))
              .catch(() => ({ value: [], error: true })),
            dataClient.listPropertySchemas(space.id).catch(() => []),
          ]);
          return [
            space.id,
            {
              folders,
              documents,
              templates: templateResult.value,
              templateSchemas,
              templatesError: templateResult.error,
            },
          ] as const;
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
  const folderCatalog = useMemo(
    () => createMobileFolderCatalog(activeData?.folders ?? [], activeSpaceId ?? undefined),
    [activeData?.folders, activeSpaceId],
  );
  const pageCatalog = useMemo(
    () => createMobilePageCatalog(activeData?.documents ?? [], activeSpaceId ?? undefined),
    [activeData?.documents, activeSpaceId],
  );
  const folderDocuments = useMemo(() => {
    const grouped = new Map<string, DocItem[]>();
    for (const doc of pageCatalog.roots)
      if (doc.folder_id) {
        const rows = grouped.get(doc.folder_id) ?? [];
        rows.push(doc);
        grouped.set(doc.folder_id, rows);
      }
    for (const [id, rows] of grouped) grouped.set(id, sortMobileDocuments(rows));
    return grouped;
  }, [pageCatalog]);
  const rootFolders = sortMobileFolders(folderCatalog.roots);
  const rootDocuments = activeData
    ? sortMobileDocuments(pageCatalog.roots.filter((doc) => !doc.folder_id))
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
    ? starredDocuments.filter((doc) => doc.spaceId === activeSpace.id).slice(0, 5)
    : [];

  const createDocumentFromTemplate = async (
    folderId?: string,
    parentId?: string,
    template?: TemplateRecord,
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
          schemas: activeData?.templateSchemas ?? [],
        })
      : undefined;
    if (prepared?.kind === "held") {
      setError(prepared.message);
      return false;
    }
    const resolved = prepared?.resolved;
    if (resolved?.unresolvedFields.length) return false;
    const title = requestedTitle?.trim() || resolved?.title || template?.name || "Untitled";
    const presentation =
      resolved?.presentation ?? (template ? templatePresentationAsRecord(template.properties) : {});
    const content = resolved
      ? applyTemplatePageWidth(resolved.content, presentation)
      : undefined;
    let targetFolderId = folderId;
    try {
      if (createDailyFolder && !targetFolderId) {
        targetFolderId = (findDailyFolder(activeData?.folders ?? []) ??
          (await dataClient.createFolder(activeSpace.id, DAILY_FOLDER_NAME))).id;
      }
      const created = await dataClient.createDocument(activeSpace.id, {
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
      navigate(getMobileDocumentPath(activeSpace.id, created.id));
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
            navigate(getMobileDocumentPath(activeSpace.id, sourceId));
            return true;
          }
        } catch {}
      }
      return false;
    }
  };

  const openNewDocument = (folderId?: string, parentId?: string) => {
    setNewDocumentError("");
    setNewDocumentTarget({ folderId, parentId });
    setNewDocumentTitle("");
  };

  const confirmNewDocument = async () => {
    if (!newDocumentTarget || newDocumentPending) return;
    const target = getMobileTemplatePickerTarget(newDocumentTarget, newDocumentTitle);
    if (!target.title) return;
    setNewDocumentError("");
    const templates = activeData?.templates || [];
    if (templates.length > 0 || activeData?.templatesError) {
      setTemplatePickerTarget(target);
      setNewDocumentTarget(null);
      return;
    }
    const original = newDocumentTarget, space = activeSpaceId;
    const current = () => newDocumentContextRef.current.target === original && newDocumentContextRef.current.space === space;
    setNewDocumentPending(true);
    try {
      await settleMobilePageCreation(
        () => createDocumentFromTemplate(target.folderId, target.parentId, undefined, target.title),
        () => { if (current()) setNewDocumentTarget(null); },
        message => { if (current()) setNewDocumentError(message); },
      );
    } finally { setNewDocumentPending(false); }
  };

  const closeTemplatePicker = () => setTemplatePickerTarget(null);

  const selectTemplate = async (
    template?: TemplateRecord,
    fieldValues?: TemplateFieldValues,
    sourceId?: string,
  ) => {
    const folderId = templatePickerTarget?.folderId;
    const parentId = templatePickerTarget?.parentId;
    const title = templatePickerTarget?.title;
    const created = await createDocumentFromTemplate(
      folderId,
      parentId,
      template,
      title,
      fieldValues,
      sourceId,
      templatePickerTarget?.date,
      templatePickerTarget?.createDailyFolder,
    );
    if (created) closeTemplatePicker();
    return created;
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
    const dailyFolder = findDailyFolder(activeData.folders);
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
          schemas: activeData.templateSchemas,
        })
      : undefined;
    if (prepared?.kind === "held") {
      setError(prepared.message);
      return;
    }
    const resolved = prepared?.resolved;
    if (resolved?.unresolvedFields.length) {
      setTemplatePickerTarget({
        ...(dailyFolder ? { folderId: dailyFolder.id } : {}),
        title: todayTitle,
        date: today,
        createDailyFolder: !dailyFolder,
        initialTemplateId: dailyTemplate?.id,
      });
      return;
    }
    const operationKey = `${activeSpace.id}:${todayTitle}`;
    const sourceId =
      dailyCreateSourceIdsRef.current.get(operationKey) ?? `doc-${createDocumentSourceId()}`;
    dailyCreateSourceIdsRef.current.set(operationKey, sourceId);
    const created = await createDocumentFromTemplate(
      dailyFolder?.id,
      undefined,
      dailyTemplate,
      todayTitle,
      {},
      sourceId,
      today,
      !dailyFolder,
    );
    if (created) dailyCreateSourceIdsRef.current.delete(operationKey);
    else setError("Failed to create today's note.");
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
        current.map((space) =>
          space.id === updated.id ? { ...space, name: updated.name, slug: updated.slug } : space,
        ),
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
        [created.id]: { folders: [], documents: [], templates: [], templateSchemas: [] },
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
          templateSchemas: [],
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
    setExpandedDocuments((prev) => ({ ...prev, [docId]: !(prev[docId] ?? false) }));
  };

  const pullRefresh = usePullToRefresh(loadData);

  if (loading) {
    return (
      <div className="text-text-muted flex items-center justify-center px-4 py-10 text-sm">
        <Loader2 size={16} className="mr-2 animate-spin" />
        Loading spaces...
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-xl px-3 py-3" {...pullRefresh.bind}>
      <PullToRefreshHint
        pullDistance={pullRefresh.pullDistance}
        isRefreshing={pullRefresh.isRefreshing}
      />
      {error && (
        <div className="rounded-input border-danger/30 bg-danger/10 text-danger mb-3 border px-3 py-2 text-sm">
          {error}
        </div>
      )}

      {templatePickerTarget && (
        <TemplatePickerDialog
          templates={activeData?.templates || []}
          schemas={activeData?.templateSchemas ?? []}
          destination={
            templatePickerTarget.createDailyFolder
              ? `${activeSpace?.name ?? "Space"} / ${DAILY_FOLDER_NAME}`
              : templatePickerTarget.parentId
                ? `${activeSpace?.name ?? "Space"} / ${activeData?.documents.find((doc) => doc.id === templatePickerTarget.parentId)?.title ?? "Page"}`
                : templatePickerTarget.folderId
                  ? `${activeSpace?.name ?? "Space"} / ${activeData?.folders.find((folder) => folder.id === templatePickerTarget.folderId)?.name ?? "Folder"}`
                  : `${activeSpace?.name ?? "Space"} / Root`
          }
          initialTitle={templatePickerTarget.title}
          initialTemplateId={templatePickerTarget.initialTemplateId}
          renderContext={{
            space: activeSpace?.name,
            author: user?.display_name,
            title: templatePickerTarget.title,
          }}
          loading={loading}
          loadError={activeData?.templatesError ? "Templates could not be loaded." : undefined}
          onRetry={() => void loadData()}
          onClose={closeTemplatePicker}
          onSelect={selectTemplate}
        />
      )}

      {newDocumentTarget && (
        <MobileSpaceNameDialog
          title="New page"
          description="Name the page before choosing its starting point."
          inputId="mobile-new-document-title"
          inputLabel="Page title"
          inputPlaceholder="Untitled"
          actionLabel={(activeData?.templates.length ?? 0) > 0 ? "Choose template" : "Create page"}
          value={newDocumentTitle}
          error={newDocumentError}
          pending={newDocumentPending}
          onChange={value => { setNewDocumentTitle(value); setNewDocumentError(""); }}
          onClose={() => {
            setNewDocumentTarget(null);
            setNewDocumentTitle("");
            setNewDocumentError("");
          }}
          onSubmit={() => void confirmNewDocument()}
        />
      )}

      {renameSpaceOpen && (
        <MobileSpaceNameDialog
          title="Rename space"
          description="This changes the space name shown in the app."
          inputId="mobile-rename-space-name"
          inputLabel="Space name"
          inputPlaceholder="Space name"
          actionLabel="Save"
          value={renameSpaceName}
          onChange={setRenameSpaceName}
          onClose={() => setRenameSpaceOpen(false)}
          onSubmit={() => void renameSpace()}
        />
      )}

      {newSpaceOpen && (
        <MobileSpaceNameDialog
          title="New space"
          description="Create another local workspace."
          inputId="mobile-new-space-name"
          inputLabel="New space name"
          inputPlaceholder="Space name"
          actionLabel="Create Space"
          value={newSpaceName}
          onChange={setNewSpaceName}
          onClose={() => setNewSpaceOpen(false)}
          onSubmit={() => void createSpace()}
        />
      )}

      {newFolderOpen && (
        <MobileSpaceNameDialog
          title={newFolderParentId ? "New subfolder" : "New folder"}
          description={
            newFolderParentId
              ? "Create a folder inside this folder."
              : "Create a root folder in this space."
          }
          inputId="mobile-new-folder-name"
          inputLabel="New folder name"
          inputPlaceholder="Folder name"
          actionLabel="Create Folder"
          value={newFolderName}
          onChange={setNewFolderName}
          onClose={() => {
            setNewFolderOpen(false);
            setNewFolderParentId(undefined);
          }}
          onSubmit={() => void createFolder()}
        />
      )}

      <div
        className={`mb-3 flex items-center gap-2 ${showSpaceSwitcher ? "justify-between" : "justify-end"}`}
      >
        {showSpaceSwitcher && <h2 className="text-text text-sm font-semibold">{spacesLabel}</h2>}
        <div className="flex items-center gap-1">
          {showCreateSpace && (
            <button
              type="button"
              onClick={openNewSpace}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex items-center gap-1 px-2 py-1 text-xs transition-colors duration-100"
              aria-label="Create space"
            >
              <Plus size={13} />
              New Space
            </button>
          )}
          <button
            type="button"
            onClick={() => void loadData()}
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex items-center gap-1 px-2 py-1 text-xs transition-colors duration-100"
            aria-label="Refresh spaces"
          >
            <RefreshCw size={13} />
            Refresh
          </button>
        </div>
      </div>

      {showSpaceSwitcher && (
        <div className="mb-4 scrollbar-none overflow-x-auto pb-1">
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
                <span className="inline-flex items-center gap-1.5">
                  {space.icon ? <ContentIdentityIcon value={space.icon} size={14} /> : null}
                  <span>{space.name}</span>
                </span>
              </button>
            ))}
          </div>
        </div>
      )}

      {!activeSpace && (
        <div className="rounded-card border-border bg-surface text-text-muted border p-4 text-sm">
          No spaces found.
        </div>
      )}

      {activeSpace && (
        <div className="rounded-card border-border bg-surface border p-3">
          <div className="mb-3 flex items-center justify-between">
            <div className="min-w-0">
              <p className="text-text truncate text-base font-semibold">{activeSpace.name}</p>
              <p className="text-text-muted truncate text-xs">
                {activeSpaceOwnerLabel || (localMode ? "Local vault" : "Space")}
              </p>
            </div>
            <div className="grid shrink-0 grid-cols-2 gap-1">
              <button
                type="button"
                onClick={() => openNewDocument()}
                className="rounded-button bg-surface-raised text-text flex min-h-11 items-center justify-center gap-1 px-2.5 py-1.5 text-xs transition-colors duration-100 hover:opacity-90"
              >
                <Plus size={12} />
                New page
              </button>
              <button
                type="button"
                onClick={() => openNewFolder()}
                className="rounded-button bg-surface-raised text-text flex min-h-11 items-center justify-center gap-1 px-2.5 py-1.5 text-xs transition-colors duration-100 hover:opacity-90"
              >
                <Folder size={12} />
                New folder
              </button>
            </div>
          </div>
          <div className="mb-3 flex justify-end">
            <button
              type="button"
              onClick={openRenameSpace}
              className="rounded-button text-text-muted active:bg-surface-raised hover:text-text flex items-center gap-1 px-2 py-1 text-xs transition-colors duration-100"
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
              className="rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text flex min-h-12 items-center gap-2 border px-2.5 py-2 text-left text-xs transition-colors duration-100"
            >
              <FileText size={14} className="shrink-0" />
              <span className="min-w-0">
                <span className="text-text block truncate font-medium">Today</span>
                <span className="block truncate">Daily note</span>
              </span>
            </button>
            <button
              type="button"
              onClick={() => navigate(getMobileJournalPath(activeSpace.id))}
              className="rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text flex min-h-12 items-center gap-2 border px-2.5 py-2 text-left text-xs transition-colors duration-100"
            >
              <CalendarDays size={14} className="shrink-0" />
              <span className="min-w-0">
                <span className="text-text block truncate font-medium">Journal</span>
                <span className="block truncate">{currentMonthDailyCount} this month</span>
              </span>
            </button>
          </div>

          {recentDailyDocuments.length > 0 && (
            <div className="rounded-input border-border bg-surface mb-3 border p-2">
              <div className="mb-1 flex items-center justify-between gap-2">
                <p className="text-text truncate text-xs font-medium">Recent daily notes</p>
                <button
                  type="button"
                  onClick={() => navigate(getMobileJournalPath(activeSpace.id))}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text shrink-0 px-1.5 py-1 text-xs"
                >
                  Open
                </button>
              </div>
              <div className="flex scrollbar-none gap-1.5 overflow-x-auto pb-1">
                {recentDailyDocuments.map((doc) => (
                  <button
                    key={doc.id}
                    type="button"
                    onClick={() => navigate(getMobileDocumentPath(activeSpace.id, doc.id))}
                    className="rounded-button border-border bg-surface text-text-muted hover:bg-surface-raised hover:text-text shrink-0 border px-2 py-1 text-xs"
                  >
                    {doc.title.slice(5)}
                  </button>
                ))}
              </div>
            </div>
          )}

          {activeStarredDocuments.length > 0 && (
            <div className="rounded-input border-border bg-surface mb-3 border p-2">
              <div className="mb-1 flex items-center gap-1.5">
                <Star size={12} className="text-accent" fill="currentColor" />
                <p className="text-text truncate text-xs font-medium">Starred</p>
              </div>
              <div className="space-y-1">
                {activeStarredDocuments.map((doc) => (
                  <button
                    key={`${doc.spaceId}:${doc.id}`}
                    type="button"
                    onClick={() => navigate(getMobileDocumentPath(doc.spaceId, doc.id))}
                    className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm transition-colors duration-100"
                  >
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

          <div className="space-y-1" role="tree" aria-label="Pages and folders">
            {rootFolders.map((folder) => (
              <MobileFolderNode
                key={folder.id}
                folder={folder}
                allFolders={activeData?.folders || []}
                folderCatalog={folderCatalog}
                folderDocuments={folderDocuments}
                documents={activeData?.documents || []}
                pageCatalog={pageCatalog}
                expandedFolders={expandedFolders}
                expandedDocuments={expandedDocuments}
                onToggleFolder={toggleFolder}
                onToggleDocument={toggleDocument}
                onOpenDocument={(docId) => navigate(getMobileDocumentPath(activeSpace.id, docId))}
                onCreateDocument={(folderId, parentId) => openNewDocument(folderId, parentId)}
                onCreateFolder={openNewFolder}
              />
            ))}

            {rootDocuments.map((doc) => (
              <MobileDocumentNode
                key={doc.id}
                doc={doc}
                documents={activeData?.documents || []}
                pageCatalog={pageCatalog}
                expandedDocuments={expandedDocuments}
                onToggleDocument={toggleDocument}
                onOpenDocument={(docId) => navigate(getMobileDocumentPath(activeSpace.id, docId))}
                onCreateChild={(parentId) => openNewDocument(undefined, parentId)}
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
  depth = 1,
  allFolders,
  folderCatalog,
  folderDocuments,
  documents,
  pageCatalog,
  expandedFolders,
  expandedDocuments,
  onToggleFolder,
  onToggleDocument,
  onOpenDocument,
  onCreateDocument,
  onCreateFolder,
}: {
  folder: FolderItem;
  depth?: number;
  allFolders: FolderItem[];
  folderCatalog: MobileFolderCatalog;
  folderDocuments: Map<string, DocItem[]>;
  documents: DocItem[];
  pageCatalog: MobilePageCatalog;
  expandedFolders: Record<string, boolean>;
  expandedDocuments: Record<string, boolean>;
  onToggleFolder: (folderId: string) => void;
  onToggleDocument: (docId: string) => void;
  onOpenDocument: (docId: string) => void;
  onCreateDocument: (folderId: string, parentId?: string) => void;
  onCreateFolder: (parentId: string) => void;
}) {
  const childFolders = folderCatalog.children.get(folder.id) ?? [];
  const folderDocs = folderDocuments.get(folder.id) ?? [];
  const expanded = !!expandedFolders[folder.id];
  const folderDepth = folderCatalog.depths.get(folder.id) ?? 0;
  const canCreateChildFolder = folderDepth > 0 && folderDepth < FOLDER_OPERATION_NODE_LIMIT;

  return (
    <div>
      <div className="group flex items-center">
        <button
          type="button"
          onClick={() => onToggleFolder(folder.id)}
          className={`${MOBILE_TREE_TOGGLE_LANE_CLASS} rounded-button text-text-muted hover:bg-surface-raised hover:text-text transition-colors duration-100`}
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
          role="treeitem"
          aria-level={depth}
          className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left text-sm transition-colors duration-100"
          aria-expanded={expanded}
          aria-label={`Toggle folder ${folder.name}`}
        >
          <ContentIdentityIcon
            value={folder.icon}
            size={MOBILE_TREE_ICON_SIZE}
            fallback={<Folder size={MOBILE_TREE_ICON_SIZE} className="shrink-0" />}
          />
          <span className="truncate">{folder.name}</span>
        </button>
        <button
          type="button"
          onClick={() => onCreateDocument(folder.id)}
          className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text ml-1 px-1.5 py-1 text-xs transition-colors duration-100"
          aria-label={`Create page in ${folder.name}`}
        >
          <Plus size={12} />
        </button>
        {canCreateChildFolder && (
          <button
            type="button"
            onClick={() => onCreateFolder(folder.id)}
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text ml-1 px-1.5 py-1 text-xs transition-colors duration-100"
            aria-label={`Create subfolder in ${folder.name}`}
          >
            <FolderPlus size={12} />
          </button>
        )}
      </div>

      {expanded && (
        <div role="group" className={depth < 6 ? "ml-5 space-y-1" : "space-y-1"}>
          {childFolders.map((childFolder) => (
            <MobileFolderNode
              key={childFolder.id}
              folder={childFolder}
              depth={depth + 1}
              allFolders={allFolders}
              folderCatalog={folderCatalog}
              folderDocuments={folderDocuments}
              documents={documents}
              pageCatalog={pageCatalog}
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
              depth={depth + 1}
              documents={documents}
              pageCatalog={pageCatalog}
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
  depth = 1,
  documents,
  pageCatalog,
  expandedDocuments,
  onToggleDocument,
  onOpenDocument,
  onCreateChild,
}: {
  doc: DocItem;
  depth?: number;
  documents: DocItem[];
  pageCatalog: MobilePageCatalog;
  expandedDocuments: Record<string, boolean>;
  onToggleDocument: (docId: string) => void;
  onOpenDocument: (docId: string) => void;
  onCreateChild: (parentId: string) => void;
}) {
  const children = pageCatalog.children.get(doc.id) ?? [];
  const expanded = expandedDocuments[doc.id] ?? false;
  const hasChildren = children.length > 0;
  return (
    <div>
      <div className="group flex items-center">
        {hasChildren ? (
          <button
            type="button"
            onClick={() => onToggleDocument(doc.id)}
            className={`${MOBILE_TREE_TOGGLE_LANE_CLASS} rounded-button text-text-muted hover:bg-surface-raised hover:text-text transition-colors duration-100`}
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
        <button
          type="button"
          onClick={() => onOpenDocument(doc.id)}
          role="treeitem"
          aria-expanded={hasChildren ? expanded : undefined}
          aria-level={depth}
          className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left text-sm transition-colors duration-100"
        >
          <FileText size={MOBILE_TREE_ICON_SIZE} className="shrink-0" />
          <span className="truncate">
            {doc.icon ? `${doc.icon} ` : ""}
            {doc.title}
          </span>
        </button>
        {(pageCatalog.depths.get(doc.id) ?? PAGE_OPERATION_NODE_LIMIT) <
          PAGE_OPERATION_NODE_LIMIT && (
          <button
            type="button"
            onClick={() => onCreateChild(doc.id)}
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text px-1.5 py-1 transition-colors duration-100"
            aria-label={`Create child page under ${doc.title}`}
          >
            <Plus size={12} />
          </button>
        )}
      </div>
      {expanded && children.length > 0 && (
        <div role="group" className={depth < 6 ? "ml-5 space-y-1" : "space-y-1"}>
          {children.map((child) => (
            <MobileDocumentNode
              key={child.id}
              doc={child}
              depth={depth + 1}
              documents={documents}
              pageCatalog={pageCatalog}
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
  const [menuTarget, setMenuTarget] = useState<
    (DocItem & { spaceId: string; spaceName: string }) | null
  >(null);
  const [deleteConfirmTarget, setDeleteConfirmTarget] = useState<
    (DocItem & { spaceId: string; spaceName: string }) | null
  >(null);
  const [starredKeys, setStarredKeys] = useState<Record<string, true>>(
    () =>
      Object.fromEntries(
        readStarredDocuments().map((item) => [getStarredDocumentKey(item), true]),
      ) as Record<string, true>,
  );
  const [statusMessage, setStatusMessage] = useState("");
  const [pendingDelete, setPendingDelete] = useState<
    (DocItem & { spaceId: string; spaceName: string }) | null
  >(null);
  const [pendingDeleteExpiresAt, setPendingDeleteExpiresAt] = useState<number | null>(null);
  const [pendingDeleteSecondsLeft, setPendingDeleteSecondsLeft] = useState(0);
  const longPressTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const pendingDeleteTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
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
          const docs = await dataClient
            .listDocuments(space.id, { include_nested: true })
            .catch(() => []);
          return docs.map((doc) => ({ ...doc, spaceId: space.id, spaceName: space.name }));
        }),
      );

      const merged = data.flat();
      setItems(sortRecent(merged).slice(0, 40));
    } catch {
      setError("Failed to load recent pages.");
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

  const handleRowTouchStart = (
    item: DocItem & { spaceId: string; spaceName: string },
    e: TouchEvent<HTMLButtonElement>,
  ) => {
    rowTouchStartRef.current = {
      x: e.touches[0]?.clientX ?? 0,
      y: e.touches[0]?.clientY ?? 0,
      key: getStarredDocumentKey(item),
    };
    onLongPressStart(item);
  };

  const handleRowTouchEnd = async (
    item: DocItem & { spaceId: string; spaceName: string },
    e: TouchEvent<HTMLButtonElement>,
  ) => {
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
      <div className="text-text-muted flex items-center justify-center px-4 py-10 text-sm">
        <Loader2 size={16} className="mr-2 animate-spin" />
        Loading recent pages...
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-xl px-3 py-3" {...pullRefresh.bind}>
      <PullToRefreshHint
        pullDistance={pullRefresh.pullDistance}
        isRefreshing={pullRefresh.isRefreshing}
      />
      <div className="mb-3 flex items-center justify-between gap-2">
        <h2 className="text-text text-sm font-semibold">Recent</h2>
        <button
          type="button"
          onClick={() => void loadRecent()}
          className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex items-center gap-1 px-2 py-1 text-xs transition-colors duration-100"
          aria-label="Refresh recent pages"
        >
          <RefreshCw size={13} />
          Refresh
        </button>
      </div>

      {error && (
        <div className="rounded-input border-danger/30 bg-danger/10 text-danger mb-3 border px-3 py-2 text-sm">
          {error}
        </div>
      )}
      {statusMessage && (
        <div className="rounded-input border-primary/20 bg-primary/10 text-text mb-3 border px-3 py-2 text-sm">
          <div className="flex items-center justify-between gap-2">
            <span>{statusMessage}</span>
            {pendingDelete && (
              <span className="flex items-center gap-2">
                {pendingDeleteSecondsLeft > 0 && (
                  <span className="text-text-muted text-xs">{pendingDeleteSecondsLeft}s</span>
                )}
                <button
                  type="button"
                  onClick={undoDelete}
                  className="rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text border px-2 py-1 text-xs transition-colors duration-100"
                >
                  Undo
                </button>
              </span>
            )}
          </div>
        </div>
      )}

      <p className="text-text-muted mb-2 px-1 text-[11px]">
        Swipe right to favorite, swipe left to delete.
      </p>

      <div className="rounded-card border-border bg-surface border p-2">
        {items.length === 0 && (
          <p className="text-text-muted px-2 py-3 text-sm">No recent pages yet.</p>
        )}
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
            className="rounded-button hover:bg-surface-raised flex w-full items-start gap-2 px-2 py-2 text-left transition-colors duration-100"
          >
            <FileText size={14} className="text-text-muted mt-0.5 shrink-0" />
            <span className="min-w-0 flex-1">
              <span className="text-text block truncate text-sm">
                {starredKeys[getStarredDocumentKey(item)] && (
                  <Star size={11} className="text-accent mr-1 inline" fill="currentColor" />
                )}
                {item.icon ? `${item.icon} ` : ""}
                {item.title}
              </span>
              <span className="text-text-muted block truncate text-xs">{item.spaceName}</span>
              <span className="text-text-muted block text-[11px]">
                {formatRelativeDate(item.updated_at)}
              </span>
            </span>
          </button>
        ))}
      </div>

      <MobileActionSheet
        open={Boolean(menuTarget)}
        title={menuTarget?.title ?? "Document"}
        labelledBy="recent-document-actions-title"
        onClose={() => setMenuTarget(null)}
      >
        {menuTarget && (
          <>
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                navigate(getMobileDocumentPath(menuTarget.spaceId, menuTarget.id));
                setMenuTarget(null);
              }}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text w-full px-2 py-2 text-left text-sm transition-colors duration-100"
            >
              Open
            </button>
            <button
              type="button"
              role="menuitem"
              onClick={() => void copyDocLink(menuTarget)}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text w-full px-2 py-2 text-left text-sm transition-colors duration-100"
            >
              Copy Link
            </button>
            <button
              type="button"
              role="menuitem"
              onClick={() => void shareDoc(menuTarget)}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text w-full px-2 py-2 text-left text-sm transition-colors duration-100"
            >
              Share
            </button>
            <button
              type="button"
              role="menuitem"
              onClick={() => {
                setDeleteConfirmTarget(menuTarget);
                setMenuTarget(null);
              }}
              className="rounded-button text-danger hover:bg-danger/10 w-full px-2 py-2 text-left text-sm transition-colors duration-100"
            >
              Delete
            </button>
          </>
        )}
      </MobileActionSheet>

      {deleteConfirmTarget && (
        <ConfirmDialog
          labelledBy="recent-document-delete-title"
          title="Delete document"
          message={`Delete "${deleteConfirmTarget.title}"? You can undo this for five seconds before it is permanently deleted.`}
          confirmLabel="Delete"
          destructive
          onCancel={() => setDeleteConfirmTarget(null)}
          onConfirm={() => {
            void queueDelete(deleteConfirmTarget);
            setDeleteConfirmTarget(null);
          }}
        />
      )}
    </div>
  );
}

export function MobileSearchScreen() {
  const navigate = useNavigate();

  return (
    <SearchModal open onClose={() => navigate("/", { replace: true })} presentation="fullscreen" />
  );
}

export function MobileNotificationsScreen() {
  const localMode = getCurrentLaunchMode() === "local";
  const [notifications, setNotifications] = useState<NotificationRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [markingAllRead, setMarkingAllRead] = useState(false);
  const [markingReadIds, setMarkingReadIds] = useState<Set<string>>(new Set());
  const [error, setError] = useState("");
  const [unreadOnly, setUnreadOnly] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const notificationsRequestIdRef = useRef(0);
  const notificationPageRequestIdRef = useRef(0);
  const activeNotificationFilterRef = useRef(unreadOnly);
  activeNotificationFilterRef.current = unreadOnly;

  const unreadCount = notifications.filter((n) => !n.read).length;

  const fetchNotifications = useCallback(async () => {
    const requestId = ++notificationsRequestIdRef.current;
    notificationPageRequestIdRef.current += 1;
    setLoadingMore(false);
    setLoading(true);
    setError("");
    try {
      const data = await dataClient.listNotifications({ limit: 50, unreadOnly });
      if (requestId !== notificationsRequestIdRef.current) return;
      setNotifications(data);
      setHasMore(data.length === 50);
    } catch {
      if (requestId !== notificationsRequestIdRef.current) return;
      setNotifications([]);
      setHasMore(false);
      setError("Failed to load notifications.");
    } finally {
      if (requestId === notificationsRequestIdRef.current) setLoading(false);
    }
  }, [unreadOnly]);

  const loadMoreNotifications = useCallback(async () => {
    const lastNotification = notifications[notifications.length - 1];
    if (!lastNotification || loadingMore) return;

    const requestId = ++notificationPageRequestIdRef.current;
    const requestUnreadOnly = unreadOnly;
    setLoadingMore(true);
    setError("");
    try {
      const data = await dataClient.listNotifications({
        limit: 50,
        unreadOnly,
        before: `${lastNotification.created_at}|${lastNotification.id}`,
      });
      if (
        requestId !== notificationPageRequestIdRef.current ||
        requestUnreadOnly !== activeNotificationFilterRef.current
      ) {
        return;
      }
      setNotifications((current) => mergeMobileNotificationPages(current, data));
      setHasMore(data.length === 50);
    } catch {
      if (
        requestId !== notificationPageRequestIdRef.current ||
        requestUnreadOnly !== activeNotificationFilterRef.current
      ) {
        return;
      }
      setError("Failed to load more notifications.");
    } finally {
      if (requestId === notificationPageRequestIdRef.current) setLoadingMore(false);
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
    return () => {
      notificationsRequestIdRef.current += 1;
      notificationPageRequestIdRef.current += 1;
      clearInterval(interval);
    };
  }, [localMode, refreshNotifications]);

  const pullRefresh = usePullToRefresh(refreshNotifications);

  const markAllRead = async () => {
    if (markingAllRead) return;
    setMarkingAllRead(true);
    setError("");
    try {
      await dataClient.markAllNotificationsRead();
      setNotifications((prev) => prev.map((n) => ({ ...n, read: true })));
      triggerHaptic("tap");
    } catch {
      setError("Failed to mark notifications as read.");
      triggerHaptic("error");
    } finally {
      setMarkingAllRead(false);
    }
  };

  const markRead = async (item: NotificationRecord) => {
    if (item.read || markingReadIds.has(item.id)) return;
    setMarkingReadIds((current) => new Set(current).add(item.id));
    setError("");
    try {
      await dataClient.markNotificationRead(item.id);
      setNotifications((prev) => prev.map((n) => (n.id === item.id ? { ...n, read: true } : n)));
      triggerHaptic("tap");
    } catch {
      setError("Failed to mark notification as read.");
      triggerHaptic("error");
    } finally {
      setMarkingReadIds((current) => {
        const next = new Set(current);
        next.delete(item.id);
        return next;
      });
    }
  };

  return (
    <div className="mx-auto max-w-xl px-3 py-3" {...pullRefresh.bind}>
      <PullToRefreshHint
        pullDistance={pullRefresh.pullDistance}
        isRefreshing={pullRefresh.isRefreshing}
      />
      <div className="mb-3 flex items-center justify-between gap-2">
        <h2 className="text-text text-sm font-semibold">
          Notifications{unreadCount > 0 ? ` (${unreadCount} unread)` : ""}
        </h2>
        {shouldShowMobileNotifications(localMode) && (
          <div className="flex items-center gap-1">
            <button
              type="button"
              onClick={() => void fetchNotifications()}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex items-center gap-1 px-2 py-1 text-xs transition-colors duration-100"
              aria-label="Refresh notifications"
            >
              <RefreshCw size={13} />
              Refresh
            </button>
            <button
              type="button"
              onClick={() => setUnreadOnly((current) => !current)}
              className={`rounded-button border-border hover:bg-surface-raised border px-2 py-1 text-xs transition-colors duration-100 ${
                unreadOnly ? "text-primary" : "text-text-muted"
              }`}
              aria-pressed={unreadOnly}
            >
              Unread
            </button>
            {unreadCount > 0 && (
              <button
                type="button"
                onClick={() => void markAllRead()}
                disabled={markingAllRead}
                aria-busy={markingAllRead}
                className="rounded-button border-border text-text-muted hover:bg-surface-raised hover:text-text border px-2 py-1 text-xs transition-colors duration-100 disabled:pointer-events-none disabled:opacity-60"
              >
                {markingAllRead ? "Marking…" : "Mark all read"}
              </button>
            )}
          </div>
        )}
      </div>

      {!shouldShowMobileNotifications(localMode) && (
        <div className="rounded-card border-border bg-surface text-text-muted border p-3 text-sm">
          Notifications are unavailable in local standalone mode.
        </div>
      )}

      {error && (
        <div
          role="alert"
          className="rounded-input border-danger/30 bg-danger/10 text-danger mb-3 border px-3 py-2 text-sm"
        >
          {error}
        </div>
      )}

      {shouldShowMobileNotifications(localMode) && (
        <div className="rounded-card border-border bg-surface border p-2">
          {loading && <p className="text-text-muted px-2 py-3 text-sm">Loading notifications...</p>}
          {!loading && notifications.length === 0 && (
            <p className="text-text-muted px-2 py-3 text-sm">No notifications.</p>
          )}
          {!loading &&
            notifications.map((item) => (
              <button
                key={item.id}
                type="button"
                onClick={() => void markRead(item)}
                disabled={markingReadIds.has(item.id)}
                aria-busy={markingReadIds.has(item.id)}
                aria-label={`${item.read ? "Read" : "Unread"} notification: ${item.title}`}
                className={`rounded-button border-border hover:bg-surface-raised flex w-full items-start gap-2 border-b px-2 py-2 text-left transition-colors duration-100 last:border-b-0 ${
                  item.read ? "" : "bg-primary/5"
                } disabled:pointer-events-none disabled:opacity-60`}
              >
                {!item.read && (
                  <span
                    className="bg-primary mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full"
                    aria-hidden="true"
                  />
                )}
                <span className="min-w-0 flex-1">
                  <span className="text-text block text-sm">{item.title}</span>
                  {item.body && (
                    <span className="text-text-muted mt-0.5 block text-xs">{item.body}</span>
                  )}
                  <span className="text-text-muted mt-1 block text-[11px]">
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
              className="rounded-button border-border text-primary hover:bg-surface-raised mt-2 w-full border px-2 py-2 text-sm transition-colors duration-100 disabled:cursor-wait disabled:opacity-60"
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
        className={`rounded-card flex h-8 w-8 shrink-0 items-center justify-center ${
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
        <span
          className={`block text-sm ${danger ? "!text-danger" : accent ? "text-primary" : "text-text"}`}
        >
          {label}
        </span>
        {description && (
          <span className="text-text-muted mt-0.5 block truncate text-xs">{description}</span>
        )}
      </span>
      {value && (
        <span className="text-text-muted max-w-[46%] truncate text-right text-sm">{value}</span>
      )}
      {interactive && (
        <ChevronRight
          size={17}
          className={`shrink-0 ${danger ? "!text-danger" : "text-text-muted"}`}
        />
      )}
    </>
  );

  if (!onClick) {
    return (
      <div className="border-border/70 flex min-h-[50px] w-full items-center gap-3 border-b px-3 py-2 text-left last:border-b-0">
        {content}
      </div>
    );
  }

  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className={`border-border/70 flex min-h-[50px] w-full items-center gap-3 border-b px-3 py-2 text-left last:border-b-0 ${
        interactive ? "active:bg-surface-raised" : "cursor-default"
      } ${danger ? "!text-danger hover:bg-danger/10" : accent ? "text-primary hover:bg-primary/10" : ""} ${disabled ? "opacity-50" : ""}`}
    >
      {content}
    </button>
  );
}

function MobileSettingsGroup({ children }: { children: ReactNode }) {
  return (
    <div className="rounded-card border-border bg-surface shadow-card overflow-hidden border">
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
      containerClassName="max-w-[48%]"
      className="bg-surface h-10 max-w-[48%] px-2 text-sm"
      aria-label={ariaLabel}
    >
      {children}
    </Select>
  );
}

function NotificationChannelPicker({
  value,
  disabled = false,
  ariaLabel,
  onChange,
}: {
  value: NotificationChannel;
  disabled?: boolean;
  ariaLabel: string;
  onChange: (value: NotificationChannel) => void;
}) {
  const [open, setOpen] = useState(false);
  const selected =
    notificationChannelOptions.find((option) => option.value === value) ??
    notificationChannelOptions[0];

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          disabled={disabled}
          aria-label={ariaLabel}
          aria-haspopup="listbox"
          aria-expanded={open}
          className="border-border bg-bg text-text hover:bg-surface-raised focus:ring-primary rounded-input flex h-10 min-w-40 items-center justify-between gap-3 border px-3 text-left text-sm transition-colors duration-100 outline-none focus:ring-2 disabled:cursor-not-allowed disabled:opacity-50"
        >
          <span className="truncate">{selected.label}</span>
          <ChevronDown size={14} className="text-text-muted shrink-0" aria-hidden="true" />
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" sideOffset={6}
        className="w-52 p-1"
        role="listbox"
        aria-label={ariaLabel}
      >
        {notificationChannelOptions.map((option) => (
          <button
            key={option.value}
            type="button"
            role="option"
            aria-selected={option.value === value}
            onClick={() => {
              onChange(option.value);
              setOpen(false);
            }}
            className={`rounded-button flex min-h-10 w-full items-center justify-between gap-3 px-3 text-left text-sm transition-colors duration-100 ${
              option.value === value
                ? "bg-primary/10 text-primary"
                : "text-text-muted hover:bg-surface-raised hover:text-text"
            }`}
          >
            {option.label}
            {option.value === value && <Check size={14} aria-hidden="true" />}
          </button>
        ))}
      </PopoverContent>
    </Popover>
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
  disabled,
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
  disabled?: boolean;
}) {
  return (
    <Input
      id={id}
      type={type}
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className={`bg-surface h-11 px-3 text-base ${className}`}
      placeholder={placeholder}
      aria-label={ariaLabel}
      autoFocus={autoFocus}
      autoCorrect={autoCorrect}
      autoCapitalize={autoCapitalize}
      inputMode={inputMode}
      disabled={disabled}
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
export const MOBILE_TOTP_RECOVERY_ACK_CLASS =
  "mt-3 flex items-start gap-2 text-xs leading-5 text-text-muted";
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
      <label className="text-text-muted text-xs font-medium" htmlFor="totp-setup-url">
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
          className="rounded-button border-border bg-surface-raised text-text active:bg-surface h-10 border px-3 text-sm transition-colors duration-100"
        >
          Copy setup URI
        </button>
        {canOpenAuthenticator && (
          <a
            href={setupUrl}
            className="rounded-button border-border bg-surface-raised text-text active:bg-surface flex h-10 items-center justify-center border px-3 text-sm transition-colors duration-100"
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
        <p id="totp-verify-help" role="status" className="text-text-muted text-xs leading-5">
          {verifyDisabledReason}
        </p>
      )}
      {recoveryCodes.length > 0 && (
        <div className={MOBILE_TOTP_RECOVERY_CARD_CLASS}>
          <div className="flex items-start justify-between gap-3">
            <span className="min-w-0">
              <p className="text-text text-xs font-medium">Recovery codes</p>
              <p role="status" className={MOBILE_TOTP_RECOVERY_WARNING_CLASS}>
                {MOBILE_TOTP_RECOVERY_WARNING_COPY}
              </p>
            </span>
            <button
              type="button"
              onClick={onCopyRecoveryCodes}
              className="rounded-button border-border bg-surface text-text active:bg-surface-raised h-8 shrink-0 border px-2 text-xs transition-colors duration-100"
            >
              Copy all
            </button>
          </div>
          <div className="text-text-muted mt-2 grid grid-cols-2 gap-1 text-xs">
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

export function sessionClientLabel(session: UserSessionRecord): string {
  if (session.client_platform === "mobile_app") return "Mobile app";
  if (session.client_platform === "desktop_app") return "Desktop app";
  return "Web browser";
}

export function sessionReference(session: UserSessionRecord): string {
  return `Session ${session.id.slice(0, 12)}`;
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

export function MobileProfileScreen({
  scope = "all",
  embedded = false,
}: {
  scope?: "all" | "app" | "account" | "profile" | "notifications" | "roles" | "spaces" | "security";
  embedded?: boolean;
}) {
  const { user, config, logout, updateCurrentUser, updateProfile } = useAuth();
  const navigate = useNavigate();
  const { runtime } = useRuntimePlatform();
  const [storageSelection, setStorageSelectionState] = useState(() => getStorageSelection());
  const [vaultLocation, setVaultLocationState] = useState<VaultLocation | null>(null);
  const [vaultFolders, setVaultFolders] = useState<VaultFolderSummary[]>([]);
  const [newVaultName, setNewVaultName] = useState("");
  const [serverVaultUrl, setServerVaultUrl] = useState("");
  const [settingsSpace, setSettingsSpace] = useState<Space | null>(null);
  const [vaultPickerOpen, setVaultPickerOpen] = useState(false);
  const [vaultCreateDialogOpen, setVaultCreateDialogOpen] = useState(false);
  const [serverVaultDialogOpen, setServerVaultDialogOpen] = useState(false);
  const [selectedVaultFolder, setSelectedVaultFolder] = useState<VaultFolderSummary | null>(null);
  const [selectedVaultName, setSelectedVaultName] = useState("");
  const [vaultTrashConfirmOpen, setVaultTrashConfirmOpen] = useState(false);
  const [resetAppConfirmOpen, setResetAppConfirmOpen] = useState(false);
  const [serverDeviceCopyReady, setServerDeviceCopyReady] = useState(false);
  const [profileName, setProfileName] = useState(user?.display_name ?? "");
  const [profileAvatarUrl, setProfileAvatarUrl] = useState(user?.avatar_url ?? "");
  const [profileUsername, setProfileUsername] = useState(user?.username ?? "");
  const [profileEditing, setProfileEditing] = useState(false);
  const [avatarCropSource, setAvatarCropSource] = useState<string | null>(null);
  const [accountRoles, setAccountRoles] = useState<
    Array<{ scope: string; role: string; description: string; permissions: string[] }>
  >([]);
  const [accountSpaces, setAccountSpaces] = useState<
    Array<{
      id: string;
      name: string;
      role: string;
      owner: boolean;
      created: number;
      edited: number;
    }>
  >([]);
  const [notificationPreferences, setNotificationPreferences] = useState<
    NotificationPreferenceRecord[]
  >([]);
  const [notificationPreferencesLoading, setNotificationPreferencesLoading] = useState(false);
  const [sessions, setSessions] = useState<UserSessionRecord[]>([]);
  const [sessionsLoading, setSessionsLoading] = useState(false);
  const [sessionRevokeTarget, setSessionRevokeTarget] = useState<UserSessionRecord | null>(null);
  const [sessionRenameTarget, setSessionRenameTarget] = useState<UserSessionRecord | null>(null);
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
      void recordSystemNotification({ title: "Starlane Notes", body: message }).catch(
        () => undefined,
      );
    }
  }, []);
  const setError = useCallback((message: string | null) => {
    setErrorState(message);
    if (message) {
      void recordSystemNotification({ title: "Action needs attention", body: message }).catch(
        () => undefined,
      );
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
  const showAppSettings = scope === "all" || scope === "app";
  const showProfileDetails = scope === "all" || scope === "account" || scope === "profile";
  const showNotificationSettings =
    scope === "all" || scope === "account" || scope === "notifications";
  const showRoleSettings = scope === "all" || scope === "account" || scope === "roles";
  const showSpaceSettings = scope === "all" || scope === "account" || scope === "spaces";
  const showSecuritySettings = scope === "all" || scope === "account" || scope === "security";
  const serverRequiresTwoFactor = Boolean(config?.require_2fa);
  const nativePasskeyApp = runtime !== "web" || isTauri();
  const passkeyManagementMode = getMobilePasskeyManagementMode(
    nativePasskeyApp,
    isPasskeySupported(),
  );
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

  useEffect(() => {
    if (!showAppSettings && !showRoleSettings) return;
    let canceled = false;
    void dataClient
      .listSpaces()
      .then((spaces) => {
        if (canceled) return;
        const activeId = localStorage.getItem(spaceStorageKey());
        setSettingsSpace(spaces.find((space) => space.id === activeId) ?? spaces[0] ?? null);
      })
      .catch(() => {
        if (!canceled) setSettingsSpace(null);
      });
    return () => {
      canceled = true;
    };
  }, [showAppSettings, showRoleSettings]);

  useEffect(() => {
    if (!showRoleSettings) return;
    if (localMode) {
      setAccountRoles([
        {
          scope: "This device",
          role: "owner",
          description: "Full control of the local vault stored on this device.",
          permissions: Object.values(SPACE_PERMISSION_LABELS),
        },
      ]);
      return;
    }

    let canceled = false;
    void dataClient
      .listSpaces()
      .then((spaces) => {
        if (canceled) return;
        const assignedSpaces = spaces.filter((space) => space.member_role);
        if (assignedSpaces.length === 0) {
          setAccountRoles([]);
          return;
        }
        setAccountRoles(
          assignedSpaces.map((space) => {
            const memberRole = space.member_role ?? "viewer";
            const definition: SpaceRoleDefinition =
              DEFAULT_SPACE_ROLE_DEFINITIONS.find((item) => item.id === memberRole) ??
              DEFAULT_SPACE_ROLE_DEFINITIONS[0];
            return {
              scope: space.name,
              role: memberRole,
              description: definition.description,
              permissions: definition.permissions.map(
                (permission) => SPACE_PERMISSION_LABELS[permission],
              ),
            };
          }),
        );
      })
      .catch(() => {
        if (!canceled) setAccountRoles([]);
      });
    return () => {
      canceled = true;
    };
  }, [localMode, showRoleSettings]);

  useEffect(() => {
    if (!showSpaceSettings) return;
    let canceled = false;
    void dataClient
      .listSpaces()
      .then(async (spaces) => {
        const summaries = await Promise.all(
          spaces.map(async (space) => {
            const documents = await dataClient
              .listDocuments(space.id, { include_nested: true })
              .catch(() => []);
            return {
              id: space.id,
              name: space.name,
              role: space.member_role ?? "viewer",
              owner: space.member_role === "owner" || space.created_by === user?.id,
              created: documents.filter((document) => document.created_by === user?.id).length,
              edited: documents.filter((document) => document.last_updated_by === user?.id).length,
            };
          }),
        );
        if (!canceled) setAccountSpaces(summaries);
      })
      .catch(() => {
        if (!canceled) setAccountSpaces([]);
      });
    return () => {
      canceled = true;
    };
  }, [showSpaceSettings, user?.id]);

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
  }, [canUseNativeVault, setError]);

  useEffect(() => {
    void refreshVaultLocation();
  }, [refreshVaultLocation]);

  useEffect(() => {
    if (localMode) {
      setServerDeviceCopyReady(false);
      return;
    }

    const refreshDeviceCopyStatus = () => {
      setServerDeviceCopyReady(
        getOfflineServerWorkspaceStatus(
          getApiBase(),
          serverCacheScopeForSession(api.getToken(), api.getRefreshToken()),
        ).ready,
      );
    };
    refreshDeviceCopyStatus();
    window.addEventListener(OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT, refreshDeviceCopyStatus);
    return () => {
      window.removeEventListener(OFFLINE_SERVER_WORKSPACE_CHANGED_EVENT, refreshDeviceCopyStatus);
    };
  }, [localMode]);

  useEffect(() => {
    setProfileName(user?.display_name ?? "");
    setProfileAvatarUrl(user?.avatar_url ?? "");
    setProfileUsername(user?.username ?? "");
    setTotpEnabled(Boolean(user?.totp_enabled));
  }, [user?.avatar_url, user?.display_name, user?.totp_enabled, user?.username]);

  useEffect(() => {
    if (localMode || !showNotificationSettings) return;

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
  }, [localMode, setError, showNotificationSettings]);

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
  }, [localMode, setError]);

  useEffect(() => {
    void loadSessions();
  }, [loadSessions]);

  const loadPasskeys = useCallback(async () => {
    if (localMode || passkeyManagementMode !== "in-app") return;
    setPasskeysLoading(true);
    try {
      const nextPasskeys = await dataClient.listPasskeys();
      setPasskeys(nextPasskeys);
    } catch {
      setError("Unable to load passkeys.");
    } finally {
      setPasskeysLoading(false);
    }
  }, [localMode, passkeyManagementMode, setError]);

  useEffect(() => {
    if (passkeyManagementMode !== "in-app") {
      setPasskeys([]);
      setPasskeysLoading(false);
      return;
    }
    void loadPasskeys();
  }, [loadPasskeys, passkeyManagementMode]);

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
      setVaultCreateDialogOpen(false);
      setStatus("Vault created. Reloading...");
      triggerHaptic("confirm");
      reloadAfterVaultChange();
    });
  }, [newVaultName, runAction, setStatus]);

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
      setServerVaultDialogOpen(false);
      setStatus("Server vault added. Reloading...");
      triggerHaptic("confirm");
      reloadAfterVaultChange();
    });
  }, [runAction, serverVaultUrl, setError, setStatus]);

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
    [runAction, setStatus],
  );

  const clearVaultLongPressTimer = useCallback(() => {
    if (vaultLongPressTimer.current) {
      window.clearTimeout(vaultLongPressTimer.current);
      vaultLongPressTimer.current = null;
    }
  }, []);

  const openVaultActionDialog = useCallback(
    (folder: VaultFolderSummary) => {
      clearVaultLongPressTimer();
      setSelectedVaultFolder(folder);
      setSelectedVaultName(folder.name);
      setVaultTrashConfirmOpen(false);
      triggerHaptic("tap");
    },
    [clearVaultLongPressTimer],
  );

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
  }, [refreshVaultLocation, runAction, selectedVaultFolder, selectedVaultName, setStatus]);

  const handleSelectedVaultDelete = useCallback(() => {
    const folder = selectedVaultFolder;
    if (!folder) return;
    if (folder.current) {
      setError("Switch to another vault before moving the active vault to trash.");
      triggerHaptic("error");
      return;
    }
    setVaultTrashConfirmOpen(true);
  }, [selectedVaultFolder, setError]);

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
  }, [refreshVaultLocation, runAction, selectedVaultFolder, setStatus]);

  const handleUseDefaultVault = useCallback(() => {
    void runAction("default-vault", async () => {
      const status = vaultLocation ?? (await getVaultLocationStatus());
      const next = await setVaultPath(status.path);
      setVaultLocationState(next);
      setStatus("App folder selected. Reloading...");
      triggerHaptic("confirm");
      reloadAfterVaultChange();
    });
  }, [runAction, setStatus, vaultLocation]);

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
  }, [canUseFolderPicker, runAction, setStatus, vaultName]);

  const handleSaveProfile = useCallback(async () => {
    const displayName = profileName.trim();
    if (!displayName) return;

    await runAction("save-profile", async () => {
      await updateProfile({
        display_name: displayName,
        avatar_url: profileAvatarUrl.trim() || undefined,
        username: profileUsername.trim() || undefined,
      });
      setProfileEditing(false);
      setStatus("Profile updated.");
      triggerHaptic("confirm");
    });
  }, [profileAvatarUrl, profileName, profileUsername, runAction, setStatus, updateProfile]);

  const handleAvatarFileSelected = useCallback(
    (file: File | null) => {
      if (!file) return;
      if (!file.type.startsWith("image/")) {
        setStatus("Choose an image file for your profile picture.");
        return;
      }
      const reader = new FileReader();
      reader.onload = () => {
        if (typeof reader.result === "string") {
          setAvatarCropSource(reader.result);
        }
      };
      reader.onerror = () => setStatus("Unable to read that image.");
      reader.readAsDataURL(file);
    },
    [setStatus],
  );

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
    [notificationPreferences, runAction, setStatus],
  );

  const handleRevokeSession = useCallback((session: UserSessionRecord) => {
    setSessionRevokeTarget(session);
  }, []);

  const handleConfirmRevokeSession = useCallback(() => {
    const session = sessionRevokeTarget;
    if (!session) return;

    void runAction(`revoke-session-${session.id}`, async () => {
      await dataClient.revokeSession(session.id);
      if (session.is_current) {
        setSessionRevokeTarget(null);
        logout();
        return;
      }
      setSessions((current) => current.filter((item) => item.id !== session.id));
      setSessionRevokeTarget(null);
      setStatus("Session revoked.");
      triggerHaptic("confirm");
    });
  }, [logout, runAction, sessionRevokeTarget, setStatus]);

  const closeSessionRevokeDialog = useCallback(() => setSessionRevokeTarget(null), []);

  const startSessionRename = useCallback((session: UserSessionRecord) => {
    setSessionRenameTarget(session);
    setSessionNameDraft(session.device_name || sessionDeviceLabel(session));
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
        setSessionRenameTarget(null);
        setSessionNameDraft("");
        setStatus("Session name updated.");
        triggerHaptic("confirm");
      });
    },
    [runAction, sessionNameDraft, setError, setStatus],
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
  }, [busyAction, runAction, setError, setStatus, totpEnabled, totpSetupUrl]);

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
  }, [busyAction, runAction, setError, setStatus, totpCode, updateCurrentUser]);

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
  }, [busyAction, serverRequiresTwoFactor, setError, totpDisableCode]);

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
  }, [
    busyAction,
    runAction,
    serverRequiresTwoFactor,
    setError,
    setStatus,
    totpDisableCode,
    updateCurrentUser,
  ]);

  const closeTotpDisableDialog = useCallback(() => setTotpDisableConfirmOpen(false), []);

  const handleCopyTotpSetupUrl = useCallback(() => {
    if (!totpSetupUrl) return;
    void (
      navigator.clipboard?.writeText(totpSetupUrl) ??
      Promise.reject(new Error("Clipboard unavailable"))
    )
      .then(() => setStatus("Authenticator setup URI copied."))
      .catch(() => setStatus("Copy unavailable. Select and copy the setup URI manually."));
  }, [setStatus, totpSetupUrl]);

  const handleCopyTotpRecoveryCodes = useCallback(() => {
    if (totpRecoveryCodes.length === 0) return;
    void (
      navigator.clipboard?.writeText(totpRecoveryCodes.join("\n")) ??
      Promise.reject(new Error("Clipboard unavailable"))
    )
      .then(() => setStatus("Recovery codes copied. Save them somewhere secure."))
      .catch(() => setStatus("Copy unavailable. Select and copy the recovery codes manually."));
  }, [setStatus, totpRecoveryCodes]);

  const handleAddPasskey = useCallback(() => {
    if (passkeyManagementMode === "browser") {
      setError(NATIVE_PASSKEY_BROWSER_MANAGEMENT_COPY);
      triggerHaptic("error");
      return;
    }
    if (passkeyManagementMode === "unavailable") {
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
  }, [busyAction, passkeyManagementMode, passkeyName, runAction, setError, setStatus]);

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
    [busyAction, setError],
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
  }, [busyAction, passkeyDeleteTarget, runAction, setError, setStatus]);

  const closePasskeyDeleteDialog = useCallback(() => setPasskeyDeleteTarget(null), []);

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
  }, [canUseFolderPicker, runAction, setStatus, vaultLocation?.path]);

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
  }, [canUseNativeVault, runAction, setStatus, vaultName]);

  const handleResetCache = useCallback(() => {
    void runAction("reset-cache", async () => {
      await resetGraphCache();
      setStatus("Cache reset. Notes and vault files were kept.");
      triggerHaptic("confirm");
    });
  }, [runAction, setStatus]);

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
  }, [canUseNativeVault, runAction, setStatus]);

  const handleOpenRepository = useCallback(() => {
    if (isTauri()) {
      void openUrl(STARLANE_GITHUB_REPOSITORY).catch(() =>
        setError("Unable to open the repository."),
      );
      return;
    }
    window.open(STARLANE_GITHUB_REPOSITORY, "_blank", "noopener,noreferrer");
  }, [setError]);

  return (
    <div className={embedded ? "space-y-5" : getProfileSettingsFrameClass(runtime, scope)}>
      {avatarCropSource && (
        <AvatarCropDialog
          source={avatarCropSource}
          onCancel={() => setAvatarCropSource(null)}
          onConfirm={(croppedImage) => {
            setProfileAvatarUrl(croppedImage);
            setAvatarCropSource(null);
            setStatus("Profile picture cropped. Save profile to apply it.");
          }}
        />
      )}
      {vaultPickerOpen && (
        <DialogShell
          labelledBy="mobile-vault-picker-title"
          maxWidth="md"
          onClose={() => {
            clearVaultLongPressTimer();
            setVaultPickerOpen(false);
          }}
        >
          <div className="flex max-h-[82dvh] flex-col">
            <div className="border-border flex items-center justify-between border-b px-3 py-2">
              <div>
                <h2 id="mobile-vault-picker-title" className="text-text text-sm font-medium">
                  Vaults
                </h2>
                <p className="text-text-muted text-xs">
                  Create, connect, or switch the active vault.
                </p>
              </div>
              <Button
                type="button"
                onClick={() => {
                  clearVaultLongPressTimer();
                  setVaultPickerOpen(false);
                }}
                variant="ghost"
                size="sm"
                className="h-8 w-8 p-0"
                aria-label="Close vault picker"
              >
                <X size={14} />
              </Button>
            </div>

            <div className="min-h-0 flex-1 overflow-y-auto p-3">
              <div className="mb-3 grid grid-cols-2 gap-2">
                <button
                  type="button"
                  onClick={() => {
                    setVaultPickerOpen(false);
                    setNewVaultName(vaultName === "App folder" ? "" : vaultName);
                    setVaultCreateDialogOpen(true);
                  }}
                  className="rounded-button border-border bg-surface-raised text-text active:bg-surface border px-3 py-2 text-sm transition-colors duration-100"
                >
                  Create new vault
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setVaultPickerOpen(false);
                    setServerVaultDialogOpen(true);
                  }}
                  className="rounded-button border-border bg-surface-raised text-text active:bg-surface border px-3 py-2 text-sm transition-colors duration-100"
                >
                  Connect server
                </button>
              </div>

              <div className="rounded-card border-border bg-surface-raised mb-3 border p-3">
                <p className="text-text text-sm font-medium">
                  {hasConfiguredVault ? vaultPathLabel : "No vault selected"}
                </p>
                <p className="text-text-muted mt-1 text-xs leading-5">
                  Server vaults download to this phone before opening when supported. Choosing a
                  local vault switches this device back to standalone mode.
                </p>
              </div>

              <div className="mb-3 flex items-center justify-between gap-3">
                <p className="text-text-muted text-xs font-medium">Available vaults</p>
                <button
                  type="button"
                  className="text-primary text-xs"
                  disabled={busyAction === "refresh-vaults"}
                  onClick={() => void runAction("refresh-vaults", refreshVaultLocation)}
                >
                  Refresh
                </button>
              </div>

              <div className="rounded-card border-border bg-surface overflow-hidden border">
                {vaultFolders.length === 0 ? (
                  <p className="text-text-muted px-3 py-3 text-sm">No vault folders found yet.</p>
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
                      className={`border-border/70 flex w-full items-center gap-3 border-b px-3 py-3 text-left last:border-b-0 ${
                        folder.current ? "bg-surface-raised" : "active:bg-surface-raised"
                      }`}
                    >
                      <Folder size={16} className="text-text-muted shrink-0" />
                      <span className="min-w-0 flex-1">
                        <span className="text-text block truncate text-sm">{folder.name}</span>
                        <span className="text-text-muted block truncate text-[11px]">
                          {folder.path}
                        </span>
                      </span>
                      <span className="text-text-muted text-xs">
                        {folder.current ? "Current" : "Switch"}
                      </span>
                    </button>
                  ))
                )}
              </div>
            </div>
          </div>
        </DialogShell>
      )}

      {vaultCreateDialogOpen && (
        <DialogShell
          labelledBy="mobile-create-vault-title"
          maxWidth="sm"
          onClose={() => setVaultCreateDialogOpen(false)}
        >
          <form
            onSubmit={(event) => {
              event.preventDefault();
              handleCreateVault();
            }}
          >
            <div className="border-border border-b px-4 py-3">
              <h2 id="mobile-create-vault-title" className="text-text text-base font-semibold">
                Create new vault
              </h2>
              <p className="text-text-muted mt-1 text-xs">
                Give this local vault a recognizable name.
              </p>
            </div>
            <div className="p-4">
              <MobileTextInput
                id="mobile-new-vault-name"
                value={newVaultName}
                onChange={setNewVaultName}
                ariaLabel="New vault name"
                placeholder="Vault name"
                autoFocus
              />
            </div>
            <div className="border-border flex justify-end gap-2 border-t px-4 py-3">
              <Button
                type="button"
                variant="secondary"
                onClick={() => setVaultCreateDialogOpen(false)}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                disabled={!newVaultName.trim() || busyAction === "create-vault"}
              >
                {busyAction === "create-vault" ? "Creating…" : "Create vault"}
              </Button>
            </div>
          </form>
        </DialogShell>
      )}

      {serverVaultDialogOpen && (
        <DialogShell
          labelledBy="mobile-connect-vault-title"
          maxWidth="sm"
          onClose={() => setServerVaultDialogOpen(false)}
        >
          <form
            onSubmit={(event) => {
              event.preventDefault();
              handleAddServerVault();
            }}
          >
            <div className="border-border border-b px-4 py-3">
              <h2 id="mobile-connect-vault-title" className="text-text text-base font-semibold">
                Connect server vault
              </h2>
              <p className="text-text-muted mt-1 text-xs">
                The phone keeps a local copy first and syncs with this server when available.
              </p>
            </div>
            <div className="p-4">
              <MobileTextInput
                id="mobile-server-vault-url"
                value={serverVaultUrl}
                onChange={setServerVaultUrl}
                ariaLabel="Server vault URL"
                placeholder="https://notes.example.com"
                type="url"
                autoFocus
                autoCorrect="off"
                autoCapitalize="none"
                inputMode="url"
              />
            </div>
            <div className="border-border flex justify-end gap-2 border-t px-4 py-3">
              <Button
                type="button"
                variant="secondary"
                onClick={() => setServerVaultDialogOpen(false)}
              >
                Cancel
              </Button>
              <Button
                type="submit"
                disabled={!serverVaultUrl.trim() || busyAction === "add-server-vault"}
              >
                {busyAction === "add-server-vault" ? "Connecting…" : "Connect server"}
              </Button>
            </div>
          </form>
        </DialogShell>
      )}

      {selectedVaultFolder && !vaultTrashConfirmOpen && (
        <DialogShell
          labelledBy="mobile-vault-options-title"
          maxWidth="sm"
          onClose={() => {
            setSelectedVaultFolder(null);
            setVaultTrashConfirmOpen(false);
          }}
        >
          <div>
            <div className="border-border flex items-center justify-between border-b px-3 py-2">
              <div className="min-w-0">
                <h2
                  id="mobile-vault-options-title"
                  className="text-text truncate text-sm font-medium"
                >
                  Vault options
                </h2>
                <p className="text-text-muted truncate text-xs">{selectedVaultFolder.path}</p>
              </div>
              <Button
                type="button"
                onClick={() => {
                  setSelectedVaultFolder(null);
                  setVaultTrashConfirmOpen(false);
                }}
                variant="ghost"
                size="sm"
                className="h-8 w-8 p-0"
                aria-label="Close vault options"
              >
                <X size={14} />
              </Button>
            </div>
            <div className="grid gap-3 p-3">
              <div className="grid gap-2">
                <label
                  className="text-text-muted text-xs font-medium"
                  htmlFor="mobile-selected-vault-name"
                >
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
                    className="rounded-button border-border bg-surface-raised text-text h-11 shrink-0 border px-3 text-sm disabled:opacity-50"
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
                className="rounded-button border-danger text-danger active:bg-surface-raised h-11 border px-3 text-sm transition-colors duration-100 disabled:opacity-50"
              >
                {selectedVaultFolder.current
                  ? "Switch away before trashing"
                  : "Move vault to trash"}
              </button>
            </div>
          </div>
        </DialogShell>
      )}

      {selectedVaultFolder && vaultTrashConfirmOpen && !selectedVaultFolder.current && (
        <ConfirmDialog
          labelledBy="mobile-vault-trash-title"
          title="Move vault to trash"
          message={getMobileVaultTrashConfirmationMessage(selectedVaultFolder.name)}
          confirmLabel="Move to trash"
          destructive
          disabled={busyAction === `delete-vault-${selectedVaultFolder.path}`}
          onCancel={() => setVaultTrashConfirmOpen(false)}
          onConfirm={handleConfirmSelectedVaultDelete}
        />
      )}

      {totpDisableConfirmOpen && (
        <ConfirmDialog
          labelledBy="mobile-totp-disable-title"
          title="Disable 2FA"
          message={getMobileTotpDisableConfirmationMessage()}
          confirmLabel="Disable 2FA"
          destructive
          disabled={busyAction === "totp-disable"}
          onCancel={closeTotpDisableDialog}
          onConfirm={handleConfirmDisableTotp}
        />
      )}

      {passkeyDeleteTarget && (
        <ConfirmDialog
          labelledBy="mobile-passkey-delete-title"
          title="Delete passkey"
          message={getMobilePasskeyDeleteConfirmationMessage(passkeyDeleteTarget.name)}
          confirmLabel="Delete passkey"
          destructive
          disabled={Boolean(passkeyDeleteDisabledReason)}
          helperText={passkeyDeleteDisabledReason ?? undefined}
          onCancel={closePasskeyDeleteDialog}
          onConfirm={handleConfirmDeletePasskey}
        />
      )}

      {sessionRevokeTarget && (
        <ConfirmDialog
          labelledBy="mobile-session-revoke-title"
          title={sessionRevokeTarget.is_current ? "Sign out this device" : "Revoke session"}
          message={getMobileSessionRevokeConfirmationMessage(
            sessionDeviceLabel(sessionRevokeTarget),
          )}
          confirmLabel={sessionRevokeTarget.is_current ? "Sign out" : "Revoke session"}
          destructive
          disabled={busyAction === `revoke-session-${sessionRevokeTarget.id}`}
          onCancel={closeSessionRevokeDialog}
          onConfirm={handleConfirmRevokeSession}
        />
      )}

      {sessionRenameTarget && (
        <DialogShell
          labelledBy="mobile-session-rename-title"
          maxWidth="sm"
          onClose={() => {
            setSessionRenameTarget(null);
            setSessionNameDraft("");
          }}
        >
          <form
            className="p-4"
            onSubmit={(event) => {
              event.preventDefault();
              saveSessionRename(sessionRenameTarget);
            }}
          >
            <div className="border-border mb-4 flex items-start justify-between gap-3 border-b pb-3">
              <div className="min-w-0">
                <h2 id="mobile-session-rename-title" className="text-text text-base font-semibold">
                  Rename session
                </h2>
                <p className="text-text-muted mt-1 text-xs">
                  {sessionClientLabel(sessionRenameTarget)} ·{" "}
                  {sessionReference(sessionRenameTarget)}
                </p>
              </div>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-8 w-8 shrink-0 px-0"
                onClick={() => {
                  setSessionRenameTarget(null);
                  setSessionNameDraft("");
                }}
                aria-label="Close rename session dialog"
              >
                <X size={15} />
              </Button>
            </div>
            <div className="grid gap-4">
              <div className="grid gap-1.5">
                <label htmlFor="mobile-session-name" className="text-text text-xs font-medium">
                  Session name
                </label>
                <Input
                  id="mobile-session-name"
                  value={sessionNameDraft}
                  onChange={(event) => setSessionNameDraft(event.target.value)}
                  maxLength={80}
                  autoFocus
                  placeholder="Work MacBook, personal iPhone…"
                />
                <p className="text-text-muted text-xs">
                  Use a unique name so this signed-in device is easy to recognize.
                </p>
              </div>
              <div className="flex justify-end gap-2">
                <Button
                  type="button"
                  variant="secondary"
                  onClick={() => {
                    setSessionRenameTarget(null);
                    setSessionNameDraft("");
                  }}
                >
                  Cancel
                </Button>
                <Button
                  type="submit"
                  disabled={
                    !sessionNameDraft.trim() ||
                    busyAction === `rename-session-${sessionRenameTarget.id}`
                  }
                >
                  Save name
                </Button>
              </div>
            </div>
          </form>
        </DialogShell>
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

      {!embedded && (
        <div className="text-center">
          <h2 className="text-text text-base font-semibold">
            {scope === "app"
              ? "App settings"
              : scope === "account"
                ? "Account settings"
                : scope === "security"
                  ? "Security"
                  : scope === "notifications"
                    ? "Notifications"
                    : scope === "roles"
                      ? "Roles"
                      : scope === "spaces"
                        ? "Spaces"
                        : scope === "profile"
                          ? "Profile settings"
                          : "Settings"}
          </h2>
          <p className="text-text-muted mt-1 text-xs">
            {scope === "security"
              ? "Manage sign-in methods and active sessions."
              : scope === "notifications"
                ? "Choose where account and workflow updates are delivered."
                : scope === "roles"
                  ? "Review every role assigned to this account and its access level."
                  : scope === "spaces"
                    ? "See the spaces you own or contribute to, with your document activity."
                    : scope === "profile"
                      ? "Manage your server account and profile details."
                      : localMode
                        ? "This vault is stored on this device first."
                        : runtime === "web"
                          ? "Manage your server account and delivery preferences."
                          : "Server sync keeps a local device copy first."}
          </p>
        </div>
      )}

      {showProfileDetails && !localMode && (
        <MobileSettingsGroup>
          <div className="border-border/70 px-3 py-4">
            <div className="flex items-center gap-3">
              {profileAvatarUrl ? (
                <img
                  src={profileAvatarUrl}
                  alt=""
                  className="border-border h-16 w-16 rounded-full border object-cover"
                />
              ) : (
                <span className="border-border bg-surface-raised text-text-muted flex h-16 w-16 items-center justify-center rounded-full border text-lg font-medium">
                  {profileName.trim()[0]?.toUpperCase() ||
                    user?.display_name?.[0]?.toUpperCase() ||
                    "U"}
                </span>
              )}
              <div className="min-w-0 flex-1">
                <p className="text-text truncate text-base font-semibold">
                  {user?.display_name || "Signed in"}
                </p>
                <p className="text-text-muted truncate text-sm">
                  {user?.email || "Server account"}
                </p>
              </div>
              {showSiteAdministration && (
                <button
                  type="button"
                  onClick={() => navigate("/admin")}
                  className="rounded-button border-primary bg-primary/10 text-primary h-9 shrink-0 border px-3 text-sm"
                >
                  Site admin
                </button>
              )}
            </div>

            {profileEditing ? (
              <div className="mt-4 grid gap-2">
                <label className="rounded-button border-border bg-surface text-text hover:bg-surface-raised inline-flex h-10 w-fit cursor-pointer items-center gap-2 border px-3 text-sm transition-colors duration-100">
                  <Pencil size={15} />
                  Change picture
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
                  value={profileAvatarUrl.startsWith("data:image/") ? "" : profileAvatarUrl}
                  onChange={setProfileAvatarUrl}
                  placeholder={
                    profileAvatarUrl.startsWith("data:image/")
                      ? "Uploaded profile picture"
                      : "Avatar URL (optional)"
                  }
                  ariaLabel="Avatar URL"
                />
              </div>
            ) : (
              <div className="border-border/70 rounded-card mt-4 overflow-hidden border">
                <MobileSettingsRow icon={User} label="Name" value={user?.display_name || "—"} />
                <MobileSettingsRow icon={Info} label="Email" value={user?.email || "—"} />
                <MobileSettingsRow
                  icon={User}
                  label="Username"
                  value={user?.username || "Not set"}
                />
              </div>
            )}

            <button
              type="button"
              disabled={profileEditing && (!profileName.trim() || busyAction === "save-profile")}
              onClick={() => {
                if (profileEditing) {
                  void handleSaveProfile();
                } else {
                  setProfileEditing(true);
                }
              }}
              className="rounded-button border-primary bg-primary/10 text-primary mt-3 h-10 w-full border px-3 text-sm disabled:opacity-50"
            >
              {busyAction === "save-profile"
                ? "Saving…"
                : profileEditing
                  ? "Save profile"
                  : "Edit profile"}
            </button>
          </div>
          <div className="border-border/70 grid gap-2 border-t px-3 py-3">
            <button
              type="button"
              onClick={() => logout()}
              className="rounded-button border-danger bg-danger/10 text-danger hover:bg-danger/15 flex h-10 w-full items-center justify-center gap-2 border px-3 text-sm transition-colors duration-100"
            >
              <LogOut size={15} />
              Sign out
            </button>
          </div>
        </MobileSettingsGroup>
      )}

      {showRoleSettings && (
        <MobileSettingsGroup>
          <div className="border-border/70 border-b px-3 py-3">
            <div className="flex items-center gap-3">
              <span className="rounded-card bg-surface-raised text-text-muted flex h-8 w-8 shrink-0 items-center justify-center">
                <ShieldCheck size={17} />
              </span>
              <div className="min-w-0">
                <p className="text-text text-sm">Assigned roles</p>
                <p className="text-text-muted mt-0.5 text-xs">
                  Access levels are calculated from your account and each space membership.
                </p>
              </div>
            </div>
          </div>
          <MobileSettingsRow
            icon={Shield}
            label="Site role"
            value={user?.role === "admin" ? "Administrator" : "Member"}
          />
          {accountRoles.length === 0 ? (
            <p className="text-text-muted px-3 py-3 text-sm">No active space role found.</p>
          ) : (
            accountRoles.map((accountRole) => (
              <div
                key={`${accountRole.scope}-${accountRole.role}`}
                className="border-border/70 border-t px-3 py-3"
              >
                <div className="flex items-start justify-between gap-3">
                  <div>
                    <p className="text-text text-sm">{getSpaceRoleLabel(accountRole.role)}</p>
                    <p className="text-text-muted mt-0.5 text-xs">{accountRole.scope}</p>
                  </div>
                  <span className="text-primary text-xs font-medium">Space access</span>
                </div>
                <p className="text-text-muted mt-2 text-xs leading-5">{accountRole.description}</p>
                <div className="mt-2 flex flex-wrap gap-2">
                  {accountRole.permissions.map((permission) => (
                    <span
                      key={permission}
                      className="border-border bg-surface-raised text-text-muted rounded-badge border px-2 py-1 text-xs"
                    >
                      {permission}
                    </span>
                  ))}
                </div>
              </div>
            ))
          )}
        </MobileSettingsGroup>
      )}

      {showSpaceSettings && (
        <MobileSettingsGroup>
          <div className="border-border/70 border-b px-3 py-3">
            <div className="flex items-center gap-3">
              <span className="rounded-card bg-surface-raised text-text-muted flex h-8 w-8 shrink-0 items-center justify-center">
                <Boxes size={17} />
              </span>
              <div className="min-w-0">
                <p className="text-text text-sm">Your spaces</p>
                <p className="text-text-muted mt-0.5 text-xs">
                  Ownership and contribution activity across your spaces.
                </p>
              </div>
            </div>
          </div>
          {accountSpaces.length === 0 ? (
            <p className="text-text-muted px-3 py-3 text-sm">No spaces found.</p>
          ) : (
            accountSpaces.map((space) => (
              <div key={space.id} className="border-border/70 border-t px-3 py-3">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <p className="text-text truncate text-sm font-medium">{space.name}</p>
                    <p className="text-text-muted mt-0.5 text-xs">
                      {space.owner ? "Owner" : getSpaceRoleLabel(space.role)}
                    </p>
                  </div>
                  {space.owner && <span className="text-primary text-xs font-medium">Owned</span>}
                </div>
                <div className="mt-3 grid grid-cols-2 gap-2">
                  <div className="rounded-card border-border bg-surface-raised border px-3 py-2">
                    <p className="text-text text-base font-semibold">{space.created}</p>
                    <p className="text-text-muted text-xs">Pages created</p>
                  </div>
                  <div className="rounded-card border-border bg-surface-raised border px-3 py-2">
                    <p className="text-text text-base font-semibold">{space.edited}</p>
                    <p className="text-text-muted text-xs">Pages edited</p>
                  </div>
                </div>
              </div>
            ))
          )}
        </MobileSettingsGroup>
      )}

      {showSecuritySettings && !localMode && (
        <MobileSettingsGroup>
          <div className="border-border/70 border-b px-3 py-3">
            <div className="flex items-start gap-3">
              <span className="rounded-card bg-surface-raised text-text-muted flex h-8 w-8 shrink-0 items-center justify-center">
                <Shield size={17} />
              </span>
              <div className="min-w-0 flex-1">
                <p className="text-text truncate text-sm">Sign-in security</p>
                <p className="text-text-muted mt-0.5 text-xs leading-5">
                  Protect this server account with an authenticator app and passkeys.
                </p>
              </div>
            </div>
          </div>
          <div className="grid gap-3 px-3 py-3">
            <div className="flex items-center justify-between gap-3">
              <span className="min-w-0">
                <span className="text-text block text-sm">Two-factor authentication</span>
                <span className="text-text-muted block text-xs">
                  {totpEnabled ? "Enabled for password sign-in." : "Not enabled yet."}
                </span>
              </span>
              <Badge variant={totpEnabled ? "success" : "outline"}>
                {totpEnabled ? "Enabled" : "Off"}
              </Badge>
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
                  className="rounded-button border-border bg-surface-raised text-text active:bg-surface h-11 border px-3 text-sm transition-colors duration-100 disabled:opacity-50"
                >
                  Set up authenticator app
                </button>
                {totpSetupDisabledReason && (
                  <p
                    id="totp-setup-help"
                    role="status"
                    className="text-text-muted text-xs leading-5"
                  >
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
              <p role="status" className={MOBILE_REQUIRED_2FA_POLICY_NOTICE_CLASS}>
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
                  className="rounded-button border-danger text-danger active:bg-danger/10 h-11 border px-3 text-sm transition-colors duration-100 disabled:opacity-50"
                >
                  Disable 2FA
                </button>
                {totpDisableDisabledReason && (
                  <p
                    id="totp-disable-help"
                    role="status"
                    className="text-text-muted text-xs leading-5 sm:col-span-2"
                  >
                    {totpDisableDisabledReason}
                  </p>
                )}
              </div>
            )}

            <div className="border-border/70 border-t pt-3">
              <div className="flex items-start gap-3">
                <span className="rounded-card bg-surface-raised text-text-muted mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center">
                  <KeyRound size={16} />
                </span>
                <div className="min-w-0 flex-1">
                  <p className="text-text text-sm">Passkeys</p>
                  <p className="text-text-muted mt-0.5 text-xs leading-5">
                    Use Face ID, Touch ID, device unlock, or a security key for server sign-in.
                  </p>
                </div>
              </div>

              <div className="mt-3 grid gap-2">
                {passkeyManagementMode === "browser" ? (
                  <ProfilePasskeyBrowserManagedNotice />
                ) : passkeysLoading ? (
                  <p className="text-text-muted text-sm">Loading passkeys...</p>
                ) : passkeys.length === 0 ? (
                  <p className="rounded-card border-border bg-bg text-text-muted border px-3 py-2 text-xs">
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

                {passkeyManagementMode === "in-app" ? (
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
                      className="rounded-button border-border bg-surface-raised text-text active:bg-surface h-11 border px-3 text-sm transition-colors duration-100 disabled:opacity-50"
                    >
                      Add passkey
                    </button>
                    <p
                      id="passkey-name-help"
                      className="text-text-muted text-xs leading-5 sm:col-span-2"
                    >
                      Passkey names can be up to 80 characters.
                    </p>
                    {passkeyAddDisabledReason && (
                      <p
                        id="passkey-add-help"
                        role="status"
                        className="text-text-muted text-xs leading-5 sm:col-span-2"
                      >
                        {passkeyAddDisabledReason}
                      </p>
                    )}
                  </div>
                ) : passkeyManagementMode === "unavailable" ? (
                  <ProfilePasskeyUnavailableNotice />
                ) : null}
              </div>
            </div>
          </div>
        </MobileSettingsGroup>
      )}

      {showNotificationSettings && !localMode && (
        <MobileSettingsGroup>
          <div className="border-border/70 border-b px-3 py-3">
            <div className="flex items-center gap-3">
              <span className="rounded-card bg-surface-raised text-text-muted flex h-8 w-8 shrink-0 items-center justify-center">
                <Bell size={17} />
              </span>
              <div className="min-w-0">
                <p className="text-text truncate text-sm">Notification delivery</p>
                <p className="text-text-muted truncate text-xs">
                  Choose inbox and email delivery by event.
                </p>
              </div>
            </div>
          </div>
          {notificationPreferencesLoading ? (
            <p className="text-text-muted px-3 py-3 text-sm">Loading notification preferences...</p>
          ) : (
            notificationPreferences.map((preference) => (
              <div
                key={preference.notification_type}
                className="border-border/70 grid min-h-[72px] grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 border-b px-3 py-3 last:border-b-0"
              >
                <button
                  type="button"
                  role="checkbox"
                  aria-checked={preference.channel !== "none"}
                  aria-label={`${preference.label} notifications`}
                  disabled={busyAction === "save-notification-preferences"}
                  onClick={() =>
                    handleNotificationPreferenceChange(
                      preference.notification_type,
                      preference.channel === "none" ? "both" : "none",
                    )
                  }
                  className={`border-border flex h-5 w-5 shrink-0 items-center justify-center rounded border transition-colors duration-100 ${
                    preference.channel !== "none"
                      ? "bg-primary border-primary text-white"
                      : "bg-bg text-transparent"
                  }`}
                >
                  <Check size={13} aria-hidden="true" />
                </button>
                <span className="min-w-0">
                  <span className="text-text block truncate text-sm font-medium">
                    {preference.label}
                  </span>
                  <span className="text-text-muted mt-0.5 block text-xs">
                    {preference.channel === "none"
                      ? "Disabled"
                      : "Choose where updates are delivered."}
                  </span>
                </span>
                <NotificationChannelPicker
                  value={preference.channel}
                  disabled={busyAction === "save-notification-preferences"}
                  onChange={(value) =>
                    handleNotificationPreferenceChange(
                      preference.notification_type,
                      value as NotificationChannel,
                    )
                  }
                  ariaLabel={`${preference.label} delivery`}
                />
              </div>
            ))
          )}
        </MobileSettingsGroup>
      )}

      {showSecuritySettings && !localMode && (
        <MobileSettingsGroup>
          <div className="border-border/70 border-b px-3 py-3">
            <div className="flex items-center justify-between gap-3">
              <div className="flex min-w-0 items-center gap-3">
                <span className="rounded-card bg-surface-raised text-text-muted flex h-8 w-8 shrink-0 items-center justify-center">
                  <KeyRound size={17} />
                </span>
                <div className="min-w-0">
                  <p className="text-text truncate text-sm">Active sessions</p>
                  <p className="text-text-muted truncate text-xs">
                    Name, identify, and revoke signed-in devices.
                  </p>
                </div>
              </div>
              <button
                type="button"
                disabled={sessionsLoading}
                onClick={() => void loadSessions()}
                className="rounded-button text-text-muted active:bg-surface-raised hover:text-text flex h-8 w-8 shrink-0 items-center justify-center transition-colors duration-100 disabled:opacity-50"
                aria-label="Refresh sessions"
              >
                <RefreshCw size={14} />
              </button>
            </div>
          </div>
          {sessionsLoading ? (
            <p className="text-text-muted px-3 py-3 text-sm">Loading sessions...</p>
          ) : sessions.length === 0 ? (
            <p className="text-text-muted px-3 py-3 text-sm">No active sessions.</p>
          ) : (
            sessions.map((session) => {
              const SessionIcon =
                session.client_platform === "mobile_app"
                  ? Smartphone
                  : session.client_platform === "desktop_app"
                    ? Monitor
                    : Globe2;
              return (
                <div
                  key={session.id}
                  className={`border-b px-3 py-3 last:border-b-0 ${
                    session.is_current ? "border-primary/30 bg-primary/5" : "border-border/70"
                  }`}
                >
                  <div className="flex min-h-[64px] items-start gap-3">
                    <span className="rounded-card border-border bg-surface-raised text-text-muted mt-0.5 flex h-9 w-9 shrink-0 items-center justify-center border">
                      <SessionIcon size={16} />
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="flex flex-wrap items-center gap-2">
                        <span className="text-text truncate text-sm font-medium">
                          {sessionDeviceLabel(session)}
                        </span>
                        {session.is_current && <Badge variant="success">This device</Badge>}
                        <Badge variant="outline">{sessionClientLabel(session)}</Badge>
                      </span>
                      <span className="text-text-muted mt-1 block text-xs">
                        Last active {new Date(session.last_seen_at).toLocaleString()}
                      </span>
                      <span className="text-text-muted mt-0.5 block text-[11px]">
                        {sessionReference(session)} · {session.ip_address || "Unknown IP"} · expires{" "}
                        {new Date(session.expires_at).toLocaleDateString()}
                      </span>
                    </span>
                    <div className="flex shrink-0 gap-1">
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        disabled={busyAction === `rename-session-${session.id}`}
                        onClick={() => startSessionRename(session)}
                        className="h-9 w-9 px-0"
                        aria-label={`Rename ${sessionDeviceLabel(session)}`}
                      >
                        <Pencil size={14} />
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        disabled={busyAction === `revoke-session-${session.id}`}
                        onClick={() => handleRevokeSession(session)}
                        className="text-danger hover:bg-danger/10 hover:text-danger h-9 px-2"
                      >
                        {session.is_current ? "Sign out" : "Revoke"}
                      </Button>
                    </div>
                  </div>
                </div>
              );
            })
          )}
        </MobileSettingsGroup>
      )}

      {showAppSettings && (
        <MobileSettingsGroup>
          <MobileSettingsRow
            icon={User}
            label="Account settings"
            description={user?.email || "Active account"}
            value={user?.display_name || undefined}
            onClick={() => navigate("/profile")}
          />
          <MobileSettingsRow
            icon={Settings}
            label="Space settings"
            description={settingsSpace?.name || "Choose an active space first"}
            value={settingsSpace ? "Manage" : undefined}
            disabled={!settingsSpace}
            onClick={
              settingsSpace
                ? () => navigate(`/spaces/${encodeURIComponent(settingsSpace.id)}/settings`)
                : undefined
            }
          />
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
            <MobileSettingsRow icon={FolderOpen} label="Open folder" onClick={handleOpenVault} />
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
              value={
                vaultLocation?.configured === true && vaultLocation?.using_default
                  ? "Selected"
                  : undefined
              }
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
          {showServerSyncRow && (
            <MobileSettingsRow
              icon={Database}
              label="Device copy"
              description={
                serverDeviceCopyReady
                  ? "Opened server content is cached on this device."
                  : "Open server content once to make it available offline."
              }
              value={serverDeviceCopyReady ? "Available offline" : "Downloads as opened"}
            />
          )}
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
          <MobileSettingsRow
            icon={Info}
            label="Starlane Notes"
            description="Open the project repository"
            value={`v${STARLANE_VERSION}`}
            onClick={handleOpenRepository}
          />
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
