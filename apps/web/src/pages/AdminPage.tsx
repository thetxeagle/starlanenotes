import {NativeArchiveOutcomeReviewControl} from "@/components/NativeArchiveOutcomeReviewControl";
import {exportCurrentCompleteNativeVaultArchive} from "@/lib/nativeCompleteVaultArchive";
import {saveBrowserServerSpaceStaticArchive,saveBrowserServerSpaceArchive,pendingBrowserPortableArchives,acknowledgeBrowserPortableArchiveReview,supportsBrowserPortableArchive} from "@/lib/browserPortableArchiveSave";
import { StorageHealthPanel } from "@/components/admin/StorageHealthPanel";
import { ReviewedDraftTransferPanel } from "@/components/admin/ReviewedDraftTransferPanel";
import { GuestAccountControls } from "@/components/GuestAccountControls";
import { accountKindLabel, canUseHumanAdministration, captureGuestClassificationTransport } from "@/lib/guestClassification";
import { nativeJsonValueEquals } from "@/lib/nativeLocalDatabase";
import type { NativeArchiveBinding, NativeArchiveInventoryRow } from "@/lib/nativeArchiveReferences";
import {
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type InputHTMLAttributes,
  type ReactNode,
} from "react";
import {
  Users,
  Globe,
  Key,
  Webhook,
  Shield,
  Bell,
  HardDrive,
  Search,
  Settings,
  FileText,
  Sparkles,
  UserRoundPlus,
  UserRoundX,
  FolderOpen,
  RotateCcw,
  RefreshCw,
  ExternalLink,
  Download,
  Upload,
  Trash2,
  Pencil,
  ChevronRight,
  Building2,
  LockKeyhole,
  LogIn,
  Plus,
  Bot,
  CheckCircle2,
  CircleAlert,
  PlugZap,
  ServerCog,
} from "lucide-react";
import { api, getApiBase } from "@/lib/api";
import { useAuth, type User } from "@/hooks/useAuth";
import { useRuntimePlatform } from "@/hooks/useRuntimePlatform";
import {
  Button,
  RESPONSIVE_TOUCH_ACTION_CLASS,
  RESPONSIVE_TOUCH_ICON_ACTION_CLASS,
} from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { DialogShell } from "@/components/ui/dialog-shell";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { Badge } from "@/components/ui/badge";
import { SettingsWorkspace } from "@/components/settings/SettingsDialog";
import {
  AdminApiKeyManagementCard,
  type ApiKeyDisplayItem,
} from "@/components/admin/ApiKeyManagementCard";
import {
  ApiKeyCreateDialog,
  type ApiKeyCreateInput,
  type ApiKeyCreateResult,
} from "@/components/admin/ApiKeyCreateDialog";
import { useDesktopSettingsDialog } from "@/hooks/useDesktopSettingsDialog";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import {
  consumePendingNativeCallback,
  NATIVE_GOOGLE_DRIVE_CALLBACK_EVENT,
} from "@/lib/nativeDeepLinkCallbacks";
import { getSpaceRoleLabel } from "@/lib/spacePermissions";
import { STARLANE_VERSION } from "@/lib/appMetadata";
import {
  clearStorageSelection,
  getSyncProvider,
  setStorageMode,
  setSyncProvider,
} from "@/lib/storageProviders";
import {
  buildGoogleDriveManualConnectionPlan,
  buildGoogleDriveManualSetupPlan,
  buildGoogleDriveProductionOAuthEvidenceUpdate,
  completeGoogleDriveManualProviderConnectionFromCallback,
  createGoogleDriveApiFetchTransport,
  createGoogleDriveOAuthFetchTransport,
  createGoogleDriveOAuthBootstrap,
  describeGoogleDriveManualConnectionReadiness,
  readProviderDefaultSyncValidationEvidence,
  writeProviderDefaultSyncValidationEvidence,
  type GoogleDriveApiResponse,
  type GoogleDriveOAuthBootstrap,
  type GoogleDriveOAuthCallbackValidationResult,
  type GoogleDriveManualConnectionPlan,
  type GoogleDriveManualSetupPlan,
  type GoogleDriveManualConnectionReadiness,
  type ProviderOAuthTokenResponse,
  type ProviderRemoteSyncEnablementRecord,
} from "@/lib/providerSession";
import {
  dataClient,
  type LocalProviderSyncReadinessRecord,
  type LocalSyncReplayHistoryManifestRecord,
  type LocalSyncReplayServerResultRecord,
} from "@/lib/dataClient";
import type { VaultQueuedSyncOperation } from "@/lib/localVault";
import { formatBackgroundReplayPartialResult } from "@/lib/backgroundReplay";
import {
  isProviderRemoteImportRecoveryRequiredOperation,
  isProviderRemoteImportServerOnlyOperation,
} from "@/lib/providerAdapters";
import type {
  ProviderDefaultSyncPolicy,
  ProviderRemoteImportApplyPlan,
  ProviderRemoteImportApplyExecutionResult,
  ProviderRemoteImportDryRun,
  ProviderRemoteMutationAttempt,
  ProviderRemoteContract,
  ProviderRemoteEnablementPlan,
  ProviderSyncResult,
} from "@/lib/providerAdapters";
import {
  buildStaticSiteBundle,
  buildStaticSiteHtml,
  buildStaticSiteZipBlob,
  formatStaticSiteBundleSummary,
  type StaticSiteTheme,
} from "@/lib/staticSiteExport";
import {
  formatVaultImportPreflight,
  summarizeVaultImportSnapshot,
  type VaultImportPreflightSummary,
} from "@/lib/vaultImportPreflight";
import type { PluginPermissionReviewLevel } from "@/lib/pluginManifest";
import { PluginArchiveSection } from "@/components/admin/PluginArchiveSection";
import {
  findSharedSpaceDownloadCheckpoint,
  formatSharedSpaceDownloadCheckpoint,
  readSharedSpaceDownloadCheckpoints,
  recordSharedSpaceDownloadCheckpoint,
  type SharedSpaceDownloadCheckpoint,
} from "@/lib/sharedSpaceDownloads";
import { AdminSidebarContext } from "@/components/layout/AppLayout";
import {
  canUseNativeVaultControls,
  chooseVaultArchiveImportPath,
  chooseVaultFolder,
  clearVaultPath,
  createVaultFolder,
  getVaultFolderName,
  getVaultLocation,
  listVaultFolders,
  revealVaultFolder,
  setVaultFolderName,
  setVaultPath,
  type VaultFolderSummary,
  type VaultLocation,
} from "@/lib/tauriVault";

type AdminSection =
  | "users"
  | "spaces"
  | "groups"
  | "roles"
  | "auth"
  | "api-keys"
  | "webhooks"
  | "ai"
  | "audit"
  | "notifications"
  | "storage"
  | "search"
  | "plugins"
  | "system";

interface UserItem {
  id: string;
  email: string;
  display_name: string;
  role: string;
  is_active: boolean;
  created_at: string;
  account_kind?: "member" | "guest";
}

interface UserInviteResult extends UserItem {
  temporary_password: string;
}

interface UserPasswordResetResult {
  email: string;
  temporary_password: string;
}

interface UserBulkLifecycleResult {
  action: "activate" | "deactivate";
  requested_count: number;
  updated_count: number;
  skipped_count: number;
  skipped: { user_id: string; reason: string }[];
}

interface AdminAiConfig {
  enabled: boolean;
  enabled_locked: boolean;
  default_provider: string;
  providers: string[];
}

type AdminAiProviderId = "ollama" | "openai" | "anthropic" | "xai";

interface AdminAiProvider {
  id: AdminAiProviderId;
  label: string;
  configured: boolean;
  source: "environment" | "database" | "unconfigured";
  locked: boolean;
  base_url_locked: boolean;
  base_url: string;
  model: string;
  has_api_key: boolean;
  api_key_locked: boolean;
  removable: boolean;
  is_default: boolean;
}

interface AdminAiProviderDraft {
  id: AdminAiProviderId;
  base_url: string;
  api_key: string;
  model: string;
}

interface AdminAiProviderTest {
  connected: boolean;
  message: string;
  models: string[];
}

export type AdminUserBulkLifecycleAction = "activate" | "deactivate";

export interface AdminUserBulkLifecycleRequest {
  user_ids: string[];
  action: AdminUserBulkLifecycleAction;
}

export interface AdminUserBulkLifecycleSummary {
  action: AdminUserBulkLifecycleAction;
  updated_count: number;
  skipped_count: number;
  skipped: { reason: string }[];
}

export interface AdminUserLifecycleCounts {
  total: number;
  active: number;
  inactive: number;
  admins: number;
  members: number;
}

export function buildAdminUserBulkLifecycleRequest(
  userIds: string[],
  action: AdminUserBulkLifecycleAction,
): AdminUserBulkLifecycleRequest {
  return {
    user_ids: userIds,
    action,
  };
}

export function formatAdminUserBulkLifecycleMessage(
  result: AdminUserBulkLifecycleSummary,
): string {
  const actionLabel = result.action === "activate" ? "activated" : "deactivated";
  const skippedText =
    result.skipped_count > 0
      ? ` ${result.skipped_count} skipped: ${result.skipped.map((item) => item.reason).join(", ")}.`
      : "";

  return `${result.updated_count} user${result.updated_count === 1 ? "" : "s"} ${actionLabel}.${skippedText}`;
}

export function getAdminUserLifecycleCounts(
  users: { is_active: boolean; role: string }[],
): AdminUserLifecycleCounts {
  return {
    total: users.length,
    active: users.filter((item) => item.is_active).length,
    inactive: users.filter((item) => !item.is_active).length,
    admins: users.filter((item) => item.role === "admin").length,
    members: users.filter((item) => item.role === "member").length,
  };
}

export type UserRoleFilter = "all" | "admin" | "member";
export type UserStatusFilter = "all" | "active" | "inactive";

export interface AdminUserFilterState {
  query: string;
  role: UserRoleFilter;
  status: UserStatusFilter;
}

export function filterAdminUsers<
  TUser extends { email: string; display_name: string; role: string; is_active: boolean },
>(users: TUser[], filters: AdminUserFilterState): TUser[] {
  const query = filters.query.trim().toLowerCase();

  return users.filter((item) => {
    if (filters.role !== "all" && item.role !== filters.role) return false;
    if (filters.status === "active" && !item.is_active) return false;
    if (filters.status === "inactive" && item.is_active) return false;
    if (!query) return true;

    return [item.email, item.display_name, item.role].join(" ").toLowerCase().includes(query);
  });
}

export interface AdminUserVisibleSelectionState {
  selectedVisibleUserIds: string[];
  allVisibleUsersSelected: boolean;
}

export function getAdminUserVisibleSelectionState<TUser extends { id: string }>(
  visibleUsers: TUser[],
  selectedUserIds: Set<string>,
): AdminUserVisibleSelectionState {
  const selectedVisibleUserIds = visibleUsers
    .filter((item) => selectedUserIds.has(item.id))
    .map((item) => item.id);

  return {
    selectedVisibleUserIds,
    allVisibleUsersSelected:
      visibleUsers.length > 0 && selectedVisibleUserIds.length === visibleUsers.length,
  };
}

export function getAdminUserSelectionAfterVisibleToggle<TUser extends { id: string }>(
  visibleUsers: TUser[],
  selectedUserIds: Set<string>,
  selected: boolean,
): Set<string> {
  const next = new Set(selectedUserIds);
  for (const item of visibleUsers) {
    if (selected) {
      next.add(item.id);
    } else {
      next.delete(item.id);
    }
  }
  return next;
}

interface UserSessionItem {
  id: string;
  device_name?: string;
  ip_address?: string;
  user_agent?: string;
  last_seen_at: string;
  expires_at: string;
  created_at: string;
}

export function getAdminSessionDeviceLabel(session: Pick<UserSessionItem, "device_name" | "user_agent">): string {
  return session.device_name || session.user_agent || "Unknown device";
}

export function getAdminSessionRevokeConfirmationMessage(
  userEmail: string,
  deviceLabel: string,
): string {
  const target = userEmail.trim() || "this user";
  const device = deviceLabel.trim() || "this device";
  return `Revoke the session for ${target} on "${device}"? That device will need to sign in again.`;
}

export function getAdminStorageResetConfirmationMessage(): string {
  return "Reset storage setup and return to the vault chooser?";
}

export function getAdminQueuedSyncClearConfirmationMessage(count?: number): string {
  if (typeof count === "number") {
    return `Clear ${count} selected queued replay operation${count === 1 ? "" : "s"}?`;
  }
  return "Clear all queued local sync operations?";
}

export function getAdminQueuedSyncReplayConfirmationMessage(count?: number): string {
  if (typeof count === "number") {
    return `Retry ${count} failed or unsupported queued operation${count === 1 ? "" : "s"} against the linked server?`;
  }
  return "Replay supported queued local operations to the linked server? Unsupported or failed operations will remain queued.";
}

export interface AdminArchiveRecoverySession {
  operationId: string;
  abandon(): Promise<void>;
}
export async function createAdminArchiveRecoverySession(binding: NativeArchiveBinding, row: NativeArchiveInventoryRow, verifyCurrent: () => void): Promise<AdminArchiveRecoverySession> {
  verifyCurrent();
  const original = JSON.parse(JSON.stringify({ binding, row })) as { binding: NativeArchiveBinding; row: NativeArchiveInventoryRow };
  const inspected = await dataClient.inspectLocalVaultArchiveImport(original.binding, original.row.operation_id, original.row.request_fingerprint);
  verifyCurrent();
  if (!nativeJsonValueEquals(inspected.intent, original.row.intent) || inspected.receipt || inspected.resolution) throw new Error("Archive operation is no longer unresolved; inspect its original evidence again");
  let dispatched = false;
  return {
    operationId: original.row.operation_id,
    abandon: async () => {
      verifyCurrent();
      if (dispatched) throw new Error("Retain this resolution request; inspect its original acknowledgement before another action");
      dispatched = true;
      await dataClient.abandonLocalVaultArchiveImport(original.binding, original.row.operation_id, original.row.request_fingerprint);
      verifyCurrent();
    },
  };
}

export function getAdminVaultArchiveImportConfirmationMessage(): string {
  return "Importing this vault will replace the current local vault.";
}

export function getAdminVaultSnapshotImportConfirmationMessage(
  summary: VaultImportPreflightSummary,
): string {
  return `${formatVaultImportPreflight(summary)}\n\n${getAdminVaultArchiveImportConfirmationMessage()}`;
}

export function getAdminProviderSpaceRecoveryConfirmationMessage(
  count: number,
  spaceIds: string,
): string {
  return `Recover provider deletion for local space${count === 1 ? "" : "s"} ${spaceIds}? This permanently removes the selected local space data.`;
}

export interface AdminUserLifecycleFailureState<TUser, TSession> {
  users: TUser[];
  selectedUserIds: Set<string>;
  sessionUser: TUser | null;
  sessions: TSession[];
}

export function getAdminUserLifecycleFailureState<
  TUser,
  TSession,
>(): AdminUserLifecycleFailureState<TUser, TSession> {
  return {
    users: [],
    selectedUserIds: new Set<string>(),
    sessionUser: null,
    sessions: [],
  };
}

export interface AdminUserSessionPanelClosedState<TUser, TSession> {
  sessionUser: TUser | null;
  sessions: TSession[];
  sessionsLoading: boolean;
}

export function getAdminUserSessionPanelClosedState<
  TUser,
  TSession,
>(): AdminUserSessionPanelClosedState<TUser, TSession> {
  return {
    sessionUser: null,
    sessions: [],
    sessionsLoading: false,
  };
}

export interface AdminUserPasswordResetSessionState<TSession> {
  sessions: TSession[];
  sessionsLoading: boolean;
}

export function getAdminUserPasswordResetSessionState<
  TUser extends { id: string },
  TSession,
>(
  sessionUser: TUser | null,
  target: TUser,
  currentSessions: TSession[],
  sessionsLoading: boolean,
): AdminUserPasswordResetSessionState<TSession> {
  if (sessionUser?.id !== target.id) {
    return {
      sessions: currentSessions,
      sessionsLoading,
    };
  }

  return {
    sessions: [],
    sessionsLoading: false,
  };
}

export function getAdminUserStatusBadgeClass(isActive: boolean): string {
  return isActive ? "bg-success/10 text-success" : "bg-danger/10 text-danger";
}

export function getAdminWebhookDeliveryStatusClass(success: boolean): string {
  return success ? "text-success" : "text-danger";
}

export function getAdminProviderReadinessBadgeClass(ready: boolean | undefined): string {
  return ready ? "bg-success/10 text-success" : "bg-warning/10 text-warning";
}

export const ADMIN_PROVIDER_BLOCKER_TEXT_CLASS = "text-xs text-warning";

export const ADMIN_PROVIDER_SUCCESS_TEXT_CLASS = "text-xs text-success";

export const ADMIN_ERROR_CARD_CLASS =
  "rounded-card border border-danger/30 px-3 py-2 text-sm text-danger";

export const ADMIN_COMPACT_ERROR_CLASS =
  "mb-3 rounded-input border border-danger/30 bg-danger/10 px-3 py-2 text-xs text-danger";

export const ADMIN_SUCCESS_CARD_CLASS =
  "rounded-card border border-success/30 px-3 py-2 text-sm text-success";

export const ADMIN_SECRET_SUCCESS_CARD_CLASS = "rounded-card border border-success/30 p-3";

export const ADMIN_SECRET_SUCCESS_TITLE_CLASS = "text-sm font-medium text-success";

export const ADMIN_PLUGIN_SUCCESS_CARD_CLASS =
  "rounded-input border border-success/30 bg-success/10 px-3 py-2 text-xs text-success";

export const ADMIN_PLUGIN_ERROR_CARD_CLASS =
  "rounded-input border border-danger/30 bg-danger/10 px-3 py-2 text-xs text-danger";

export const ADMIN_LOCAL_SYNC_ERROR_CARD_CLASS =
  "rounded-card mt-3 border border-danger/30 px-3 py-2 text-xs text-danger";

export const ADMIN_ACTIVE_SPACE_NAME_LABEL = "Active space name";
export const ADMIN_RENAME_ACTIVE_SPACE_LABEL = "Rename Active Space";

export const ADMIN_DANGER_ICON_BUTTON_CLASS =
  "rounded-button text-text-muted hover:bg-surface-raised flex h-8 w-8 items-center justify-center hover:text-danger";

export interface AdminUserImportInput {
  email: string;
  display_name: string;
  role: "admin" | "member";
}

interface SpaceItem {
  id: string;
  name: string;
  slug: string;
  visibility: string;
  member_role?: string;
}

export function sortAdminLocalSpaces<T extends Pick<SpaceItem, "name">>(spaces: T[]): T[] {
  return [...spaces].sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { sensitivity: "base" }),
  );
}

interface SpaceMemberItem {
  user_id: string;
  email: string;
  display_name: string;
  role: string;
  joined_at: string;
}

interface GroupItem {
  id: string;
  name: string;
  description?: string;
  member_count: number;
  space_grant_count: number;
  scim_managed: boolean;
  managed_space_id?: string;
  created_at: string;
  updated_at: string;
}

interface GroupSpaceGrantItem {
  space_id: string;
  name: string;
  slug: string;
  role: string;
}

interface GlobalRoleItem {
  id: string;
  name: string;
  description: string;
  permissions: string[];
  built_in: boolean;
}

export function getAdminGroupDeleteConfirmationMessage(
  group: Pick<GroupItem, "name" | "member_count" | "space_grant_count">,
): string {
  return `Delete ${group.name}? This removes ${group.member_count} group membership${group.member_count === 1 ? "" : "s"} and ${group.space_grant_count} space access grant${group.space_grant_count === 1 ? "" : "s"}. It does not delete users, spaces, or documents.`;
}

interface ConfigEntry {
  key: string;
  label: string;
  value: string;
  effective_value: string;
  source: "env" | "database" | "default";
  env_var?: string;
  locked: boolean;
  secret: boolean;
}

interface WebhookItem {
  id: string;
  name: string;
  url: string;
  events: string[];
  space_id?: string | null;
  is_active: boolean;
  created_at: string;
  created_by?: string;
}

export interface WebhookDeliveryItem {
  id: string;
  event_type: string;
  response_status?: number | null;
  success: boolean;
  attempt_number: number;
  delivered_at: string;
  test_id?: string | null;
}

export interface AdminWebhookHealth {
  label: "Configured" | "Healthy" | "Retrying" | "Needs attention" | "Inactive";
  detail: string;
  className: string;
}

export function getAdminWebhookHealth(
  active: boolean,
  deliveries: WebhookDeliveryItem[],
): AdminWebhookHealth {
  if (!active) {
    return {
      label: "Inactive",
      detail: "This connection is not accepting new deliveries.",
      className: "border-border text-text-muted",
    };
  }
  const latest = [...deliveries].sort(
    (left, right) => Date.parse(right.delivered_at) - Date.parse(left.delivered_at),
  )[0];
  if (!latest) {
    return {
      label: "Configured",
      detail: "No receiver result is recorded yet. Run a test before relying on this connection.",
      className: "border-border text-text-muted",
    };
  }
  if (latest.success && latest.response_status !== null && latest.response_status !== undefined) {
    return {
      label: "Healthy",
      detail: `Latest recorded attempt was accepted with HTTP ${latest.response_status}.`,
      className: "border-success/30 bg-success/10 text-success",
    };
  }
  if (latest.attempt_number < 3) {
    return {
      label: "Retrying",
      detail: "The latest attempt failed and may still be retried. Delivery is not confirmed.",
      className: "border-warning/30 bg-warning/10 text-warning",
    };
  }
  return {
    label: "Needs attention",
    detail: "The latest delivery exhausted its recorded attempts. Inspect the receiver and activity.",
    className: "border-danger/30 bg-danger/10 text-danger",
  };
}

interface AdminWebhookDeliveryHistoryTableProps {
  deliveries: WebhookDeliveryItem[];
  localMode: boolean;
  redeliveringDeliveryId: string | null;
  redeliveryError: string | null;
  onRedeliver: (deliveryId: string) => void;
  disableRedelivery?: boolean;
}

export interface AdminWebhookDeliveryResetState {
  deliveries: WebhookDeliveryItem[];
  deliveriesError: string | null;
  redeliveryError: string | null;
  redeliveringDeliveryId: string | null;
  deliveriesLoading: boolean;
}

export function getAdminWebhookDeliveryResetState(): AdminWebhookDeliveryResetState {
  return {
    deliveries: [],
    deliveriesError: null,
    redeliveryError: null,
    redeliveringDeliveryId: null,
    deliveriesLoading: false,
  };
}

interface WebhookFormState {
  name: string;
  url: string;
  secret: string;
  events: string;
  space_id: string;
}

interface LocalVaultStats {
  spaces: number;
  folders: number;
  documents: number;
  assets: number;
  asset_bytes: number;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function downloadJsonFile(filename: string, value: unknown): void {
  const blob = new Blob([JSON.stringify(value, null, 2)], { type: "application/json" });
  downloadBlob(filename, blob);
}

function downloadTextFile(filename: string, value: string, type = "text/plain;charset=utf-8"): void {
  downloadBlob(filename, new Blob([value], { type }));
}

function downloadBlob(filename: string, blob: Blob): void {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

function readLinkedServerUrl(): string {
  if (typeof window === "undefined") return "";
  return window.localStorage.getItem("server_url") ?? "";
}

export function canRevealAdminVaultFolder(
  nativeAvailable: boolean,
  location?: Pick<VaultLocation, "path"> | null,
): boolean {
  return nativeAvailable && Boolean(location?.path?.trim());
}

function readCheckpointServerUrl(): string | undefined {
  const linkedServerUrl = readLinkedServerUrl().trim().replace(/\/+$/, "");
  if (linkedServerUrl) return linkedServerUrl;
  if (typeof window === "undefined" || !window.location?.origin) return undefined;
  return window.location.origin === "null" ? undefined : window.location.origin;
}

function readGoogleDrivePublicClientId(): string {
  return (import.meta.env.VITE_GOOGLE_DRIVE_CLIENT_ID as string | undefined)?.trim() ?? "";
}

function readGoogleDriveBrowserClientSecret(): string {
  return (import.meta.env.VITE_GOOGLE_DRIVE_CLIENT_SECRET as string | undefined)?.trim() ?? "";
}

function buildGoogleDriveRedirectUri(): string {
  if (typeof window === "undefined" || !window.location?.origin || window.location.origin === "null") {
    return "starlane://provider/google-drive/callback";
  }
  return `${window.location.origin.replace(/\/+$/, "")}/provider/google-drive/callback`;
}

function parseJsonObject(value: string, label: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(value) as unknown;
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      throw new Error(`${label} must be a JSON object`);
    }
    return parsed as Record<string, unknown>;
  } catch (error) {
    if (error instanceof SyntaxError) {
      throw new Error(`${label} must be valid JSON`);
    }
    throw error;
  }
}

export function parseGoogleDriveTokenFixture(value: string): ProviderOAuthTokenResponse {
  const parsed = parseJsonObject(value, "Google Drive token response");
  const accessToken = parsed.access_token;
  const expiresIn = parsed.expires_in;
  if (typeof accessToken !== "string" || accessToken.trim().length === 0) {
    throw new Error("Google Drive token response requires access_token");
  }
  if (typeof expiresIn !== "number" || !Number.isFinite(expiresIn) || expiresIn <= 0) {
    throw new Error("Google Drive token response requires positive expires_in");
  }
  return {
    access_token: accessToken,
    refresh_token: typeof parsed.refresh_token === "string" ? parsed.refresh_token : undefined,
    expires_in: expiresIn,
    scope: typeof parsed.scope === "string" ? parsed.scope : undefined,
    issued_at: typeof parsed.issued_at === "string" ? parsed.issued_at : undefined,
    account_label: typeof parsed.account_label === "string" ? parsed.account_label : undefined,
    provider_root_id:
      typeof parsed.provider_root_id === "string" ? parsed.provider_root_id : undefined,
    provider_root_name:
      typeof parsed.provider_root_name === "string" ? parsed.provider_root_name : undefined,
  };
}

export function validateGoogleDriveOAuthLaunchUrl(value: string | undefined): string {
  if (!value) throw new Error("Generate a Google Drive setup preview before opening OAuth.");
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    throw new Error("Google Drive OAuth authorization URL is invalid.");
  }
  if (url.origin !== "https://accounts.google.com" || url.pathname !== "/o/oauth2/v2/auth") {
    throw new Error("Google Drive OAuth authorization URL must target Google Accounts.");
  }
  if (url.searchParams.get("response_type") !== "code") {
    throw new Error("Google Drive OAuth authorization URL must request an authorization code.");
  }
  if (url.searchParams.get("code_challenge_method") !== "S256") {
    throw new Error("Google Drive OAuth authorization URL must use PKCE S256.");
  }
  if (!url.searchParams.get("client_id")?.trim()) {
    throw new Error("Google Drive OAuth authorization URL must include a client ID.");
  }
  if (!url.searchParams.get("redirect_uri")?.trim()) {
    throw new Error("Google Drive OAuth authorization URL must include a redirect URI.");
  }
  if (!url.searchParams.get("state")?.trim()) {
    throw new Error("Google Drive OAuth authorization URL must include a state value.");
  }
  if (!url.searchParams.get("code_challenge")?.trim()) {
    throw new Error("Google Drive OAuth authorization URL must include a PKCE code challenge.");
  }
  const scopes = new Set(
    (url.searchParams.get("scope") ?? "")
      .split(/\s+/)
      .map((scope) => scope.trim())
      .filter(Boolean),
  );
  for (const scope of ["drive.file", "drive.metadata.readonly", "offline_access"]) {
    if (!scopes.has(scope)) {
      throw new Error(`Google Drive OAuth authorization URL must include ${scope} scope.`);
    }
  }
  if (url.searchParams.get("access_type") !== "offline") {
    throw new Error("Google Drive OAuth authorization URL must request offline access.");
  }
  if (url.searchParams.get("prompt") !== "consent") {
    throw new Error("Google Drive OAuth authorization URL must request consent for refresh access.");
  }
  return url.toString();
}

export type GoogleDriveManualSetupStepState = "ready" | "blocked" | "next" | "pending";

export function getGoogleDriveManualSetupStepStates({
  setupPlanStatus,
  hasBootstrap,
  callbackStatus,
  connectionPlanStatus,
}: {
  setupPlanStatus?: GoogleDriveManualSetupPlan["status"];
  hasBootstrap: boolean;
  callbackStatus?: GoogleDriveOAuthCallbackValidationResult["status"];
  connectionPlanStatus?: GoogleDriveManualConnectionPlan["status"];
}): GoogleDriveManualSetupStepState[] {
  const previewReady = setupPlanStatus === "ready_to_start" && hasBootstrap;
  const callbackReady = callbackStatus === "ready_for_exchange";
  const completionReady = connectionPlanStatus === "ready_to_complete";
  return [
    setupPlanStatus === "blocked" ? "blocked" : previewReady ? "ready" : "next",
    previewReady ? (callbackStatus ? "ready" : "next") : "pending",
    callbackStatus === "blocked" ? "blocked" : callbackReady ? "ready" : "pending",
    connectionPlanStatus === "blocked" ? "blocked" : completionReady ? "next" : "pending",
  ];
}

export function getGoogleDriveManualSetupNextStepLabel(
  states: GoogleDriveManualSetupStepState[],
): string {
  const labels = [
    "Generate setup preview",
    "Open Google OAuth",
    "Validate callback",
    "Complete connection",
  ];
  const blockedIndex = states.findIndex((state) => state === "blocked");
  if (blockedIndex >= 0) return `${labels[blockedIndex]} blocked`;
  const nextIndex = states.findIndex((state) => state === "next");
  if (nextIndex >= 0) return labels[nextIndex];
  return "Setup complete";
}

function formatGoogleDriveManualSetupStepState(state: GoogleDriveManualSetupStepState): string {
  switch (state) {
    case "ready":
      return "Ready";
    case "blocked":
      return "Blocked";
    case "next":
      return "Next";
    case "pending":
    default:
      return "Pending";
  }
}

export function getGoogleDriveFixtureDisclosureLabel(showFixtures: boolean): string {
  return showFixtures ? "Hide Fixture Inputs" : "Show Fixture Inputs";
}

type ProviderImportReviewOperation =
  | ProviderRemoteImportDryRun["operations"][number]
  | ProviderRemoteImportApplyPlan["operations"][number]
  | ProviderRemoteImportApplyExecutionResult["results"][number];

export function formatProviderImportOperationLabel(
  operation: Pick<ProviderImportReviewOperation, "entity_type" | "action" | "entity_id">,
): string {
  return `${operation.entity_type}:${operation.action} ${operation.entity_id}`;
}

export function formatProviderImportOperationStatus(
  operation: {
    status?: string;
    apply_status?: string;
    entity_type?: string;
    action?: string;
    message: string;
  },
): string {
  const status = operation.apply_status ?? operation.status ?? "unknown";
  const isServerOnlyImport = isProviderImportServerOnlyOperation(operation);
  const isRecoveryRequiredImport = isProviderImportRecoveryRequiredOperation(operation);
  const labels: Record<string, string> = {
    applied: "Applied",
    blocked: "Blocked",
    conflict: "Conflict",
    failed: "Failed",
    selected: "Ready to apply",
    skipped: "Skipped",
    skipped_conflict: "Conflict skipped",
    skipped_failed: "Unavailable payload skipped",
    skipped_unsupported: "Unsupported skipped",
    unsupported: "Unsupported",
    would_apply: "Ready to apply",
  };
  if (isServerOnlyImport && (status === "unsupported" || status === "skipped_unsupported")) {
    labels[status] = status === "skipped_unsupported" ? "Server-only skipped" : "Server-only";
  }
  if (
    isRecoveryRequiredImport &&
    (status === "unsupported" || status === "skipped_unsupported")
  ) {
    labels[status] =
      status === "skipped_unsupported" ? "Recovery required skipped" : "Recovery required";
  }
  return `${labels[status] ?? status.replace(/_/g, " ")} - ${operation.message}`;
}

function isProviderImportServerOnlyOperation(operation: {
  status?: string;
  apply_status?: string;
  entity_type?: string;
  action?: string;
  message: string;
}): boolean {
  const status = operation.apply_status ?? operation.status ?? "unknown";
  return (
    (status === "unsupported" || status === "skipped_unsupported") &&
    ((typeof operation.entity_type === "string" &&
      typeof operation.action === "string" &&
      isProviderRemoteImportServerOnlyOperation(operation.entity_type, operation.action)) ||
      operation.message.includes("is server-only and cannot be imported"))
  );
}

function isProviderImportRecoveryRequiredOperation(operation: {
  status?: string;
  apply_status?: string;
  entity_type?: string;
  action?: string;
  message: string;
}): boolean {
  const status = operation.apply_status ?? operation.status ?? "unknown";
  return (
    (status === "unsupported" || status === "skipped_unsupported") &&
    ((typeof operation.entity_type === "string" &&
      typeof operation.action === "string" &&
      isProviderRemoteImportRecoveryRequiredOperation(operation.entity_type, operation.action)) ||
      operation.message.includes("must be handled by an explicit vault recovery flow"))
  );
}

function formatProviderImportUnsupportedParts(
  unsupportedCount: number | undefined,
  operations?: Array<{
    status?: string;
    apply_status?: string;
    entity_type?: string;
    action?: string;
    message: string;
  }>,
): string[] {
  if (!unsupportedCount) return [];
  const serverOnlyCount = operations?.filter(isProviderImportServerOnlyOperation).length ?? 0;
  const recoveryRequiredCount =
    operations?.filter(isProviderImportRecoveryRequiredOperation).length ?? 0;
  const genericUnsupportedCount = Math.max(
    0,
    unsupportedCount - serverOnlyCount - recoveryRequiredCount,
  );
  return [
    serverOnlyCount
      ? `${serverOnlyCount} server-only`
      : null,
    recoveryRequiredCount
      ? `${recoveryRequiredCount} recovery-required`
      : null,
    genericUnsupportedCount
      ? `${genericUnsupportedCount} unsupported`
      : null,
  ].filter(Boolean) as string[];
}

function isProviderImportRelatedEntityConflict(operation: {
  status?: string;
  apply_status?: string;
  message: string;
}): boolean {
  const status = operation.apply_status ?? operation.status ?? "unknown";
  return (
    (status === "conflict" || status === "skipped_conflict") &&
    operation.message.includes("targets a related entity")
  );
}

function isProviderImportDocumentConflict(operation: {
  status?: string;
  apply_status?: string;
  message: string;
}): boolean {
  const status = operation.apply_status ?? operation.status ?? "unknown";
  return (
    (status === "conflict" || status === "skipped_conflict") &&
    operation.message.includes("targets the same document")
  );
}

function formatProviderImportConflictParts(
  conflictCount: number | undefined,
  operations?: Array<{ status?: string; apply_status?: string; message: string }>,
): string[] {
  if (!conflictCount) return [];
  const relatedEntityCount =
    operations?.filter(isProviderImportRelatedEntityConflict).length ?? 0;
  const documentConflictCount =
    operations?.filter(isProviderImportDocumentConflict).length ?? 0;
  const genericConflictCount = Math.max(
    0,
    conflictCount - relatedEntityCount - documentConflictCount,
  );
  return [
    relatedEntityCount
      ? `${relatedEntityCount} related-entity conflict${relatedEntityCount === 1 ? "" : "s"}`
      : null,
    documentConflictCount
      ? `${documentConflictCount} document conflict${documentConflictCount === 1 ? "" : "s"}`
      : null,
    genericConflictCount
      ? `${genericConflictCount} conflict${genericConflictCount === 1 ? "" : "s"}`
      : null,
  ].filter(Boolean) as string[];
}

export function formatProviderImportUnsupportedSummary(
  unsupportedCount: number | undefined,
  operations?: Array<{
    status?: string;
    apply_status?: string;
    entity_type?: string;
    action?: string;
    message: string;
  }>,
): string {
  const parts = formatProviderImportUnsupportedParts(unsupportedCount, operations);
  return parts.length ? parts.join(", ") : "0 unsupported";
}

export function getProviderImportOperationStatusClass(
  operation: { status?: string; apply_status?: string },
): string {
  const status = operation.apply_status ?? operation.status ?? "unknown";
  if (status === "applied" || status === "selected" || status === "would_apply") {
    return "text-success";
  }
  if (status === "failed" || status === "skipped_failed") {
    return "text-danger";
  }
  if (
    status === "blocked" ||
    status === "conflict" ||
    status === "skipped_conflict" ||
    status === "skipped_unsupported" ||
    status === "unsupported"
  ) {
    return "text-warning";
  }
  return "text-text-muted";
}

export function formatProviderValidationEvidenceLabel(
  verified: boolean,
  verifiedAt?: string,
): string {
  if (!verified) return "not verified";
  if (!verifiedAt) return "verified";
  const date = new Date(verifiedAt);
  if (Number.isNaN(date.getTime())) return `verified at ${verifiedAt}`;
  return `verified at ${date.toLocaleString()}`;
}

export function formatGoogleDriveOAuthRedirectEnvironment(
  environment: GoogleDriveManualSetupPlan["oauth_redirect_environment"],
): string {
  switch (environment) {
    case "production_https":
      return "production HTTPS";
    case "native_custom_scheme":
      return "native callback";
    case "local_development":
      return "local/dev";
    case "unconfigured":
    default:
      return "missing";
  }
}

export function formatGoogleDriveProductionOAuthEvidenceAction(
  setupPlan: Pick<
    GoogleDriveManualSetupPlan,
    "status" | "production_oauth_candidate" | "oauth_redirect_environment"
  >,
): string {
  if (setupPlan.status !== "ready_to_start") {
    return "Resolve setup blockers before OAuth evidence can be recorded";
  }
  if (setupPlan.production_oauth_candidate) {
    return "Live Google API completion will record production OAuth evidence";
  }
  return `Live completion will persist the session, but ${formatGoogleDriveOAuthRedirectEnvironment(
    setupPlan.oauth_redirect_environment,
  )} OAuth does not count as production validation`;
}

export function formatProviderRemoteSyncEnablementLabel(
  enablement: ProviderRemoteSyncEnablementRecord | null,
): string {
  if (!enablement) return "not checked";
  const state = enablement.enabled ? "enabled" : "disabled";
  if (!enablement.reason?.trim()) return state;
  return `${state} (${enablement.reason.trim()})`;
}

export function formatProviderImportHiddenOperationCount(
  totalCount: number,
  visibleLimit = 5,
): string | null {
  const hiddenCount = totalCount - visibleLimit;
  if (hiddenCount <= 0) return null;
  return `${hiddenCount} more operation${hiddenCount === 1 ? "" : "s"} not shown`;
}

export function formatHiddenBlockerCount(
  totalCount: number,
  visibleLimit = 3,
): string | null {
  const hiddenCount = totalCount - visibleLimit;
  if (hiddenCount <= 0) return null;
  return `${hiddenCount} more blocker${hiddenCount === 1 ? "" : "s"} not shown`;
}

export function formatProviderImportHiddenBlockerCount(
  totalCount: number,
  visibleLimit = 3,
): string | null {
  return formatHiddenBlockerCount(totalCount, visibleLimit);
}

export function ProviderImportOperationReviewList({
  title,
  operations,
  visibleLimit = 5,
}: {
  title: string;
  operations: ProviderImportReviewOperation[];
  visibleLimit?: number;
}) {
  if (operations.length === 0) return null;

  return (
    <div className="space-y-2">
      <p className="text-text text-xs font-medium">{title}</p>
      <div className="rounded-card border-border overflow-hidden border">
        {operations.slice(0, visibleLimit).map((operation) => (
          <div
            key={operation.operation_id}
            className="border-border/70 grid gap-1 border-b px-3 py-2 text-xs last:border-b-0 md:grid-cols-[minmax(0,1fr)_minmax(0,1.35fr)]"
          >
            <span className="space-y-1 break-all">
              <span className="text-text block">
                {formatProviderImportOperationLabel(operation)}
              </span>
              <span className="text-text-muted block font-mono">
                Operation ID: {operation.operation_id}
              </span>
            </span>
            <span
              className={`${getProviderImportOperationStatusClass(operation)} break-words`}
            >
              {formatProviderImportOperationStatus(operation)}
            </span>
          </div>
        ))}
        {formatProviderImportHiddenOperationCount(operations.length, visibleLimit) ? (
          <div className="px-3 py-2 text-xs text-text-muted">
            {formatProviderImportHiddenOperationCount(operations.length, visibleLimit)}
          </div>
        ) : null}
      </div>
    </div>
  );
}

export function getProviderImportApplyReviewState(
  attempt:
    | {
        provider_sync_manifest?: unknown;
        import_apply_plan?: Pick<
          ProviderRemoteImportApplyPlan,
          "selected_operation_count" | "skipped_operation_count"
        >;
        import_dry_run?: Pick<
          ProviderRemoteImportDryRun,
          "conflict_count" | "failed_count" | "unsupported_count"
        > & {
          operations?: Array<{ status?: string; message: string }>;
        };
      }
    | null
    | undefined,
  applyResult?: Pick<ProviderRemoteImportApplyExecutionResult, "status"> | null,
): { canApply: boolean; message: string } {
  if (applyResult?.status === "completed") {
    return {
      canApply: false,
      message: "Last provider import completed. Run Test Pull to review a new manifest.",
    };
  }

  if (!attempt?.provider_sync_manifest) {
    return {
      canApply: false,
      message: "Run Test Pull to download and review a provider-sync manifest.",
    };
  }

  const plan = attempt.import_apply_plan;
  if (!plan) {
    return {
      canApply: false,
      message: "Pull did not produce an import apply plan.",
    };
  }

  if (plan.selected_operation_count > 0) {
    return {
      canApply: true,
      message: `${plan.selected_operation_count} remote operation${plan.selected_operation_count === 1 ? "" : "s"} ready to apply.`,
    };
  }

  const dryRun = attempt.import_dry_run;
  const reasons = [
    ...formatProviderImportConflictParts(dryRun?.conflict_count, dryRun?.operations),
    dryRun?.failed_count ? `${dryRun.failed_count} failed` : null,
    ...formatProviderImportUnsupportedParts(dryRun?.unsupported_count, dryRun?.operations),
    plan.skipped_operation_count ? `${plan.skipped_operation_count} skipped` : null,
  ].filter(Boolean);

  return {
    canApply: false,
    message: reasons.length
      ? `No eligible remote operations: ${reasons.join(", ")}.`
      : "No eligible remote operations to apply.",
  };
}

export function formatProviderImportApplyPlanSummary(
  plan:
    | Pick<
        ProviderRemoteImportApplyPlan,
        "selected_operation_count" | "skipped_operation_count"
      >
    | null
    | undefined,
): string {
  if (!plan) return "not planned";
  return `${plan.selected_operation_count} selected, ${plan.skipped_operation_count} skipped`;
}

export function getProviderImportNextActionLabel(
  attempt:
    | {
        provider_sync_manifest?: unknown;
        import_apply_plan?: Pick<ProviderRemoteImportApplyPlan, "selected_operation_count">;
      }
    | null
    | undefined,
  applyResult?: Pick<ProviderRemoteImportApplyExecutionResult, "status"> | null,
): string {
  if (applyResult?.status === "completed") return "Run Test Pull";
  if (!attempt?.provider_sync_manifest) return "Run Test Pull";
  if ((attempt.import_apply_plan?.selected_operation_count ?? 0) > 0) return "Apply Last Import";
  return "Review blockers";
}

export function getProviderImportReviewStateAfterSync(
  direction: "pull" | "push",
  result: Pick<ProviderSyncResult, "remote_mutation_attempt">,
): {
  providerImportAttempt?: ProviderRemoteMutationAttempt | null;
  providerImportApplyResult?: ProviderRemoteImportApplyExecutionResult | null;
} {
  if (direction !== "pull") return {};
  return {
    providerImportAttempt: result.remote_mutation_attempt ?? null,
    providerImportApplyResult: null,
  };
}

export function getProviderImportReviewStateAfterProviderConnectionChange(): {
  providerImportAttempt: null;
  providerImportApplyResult: null;
} {
  return {
    providerImportAttempt: null,
    providerImportApplyResult: null,
  };
}

export function formatProviderImportPullReviewSummary(
  attempt: {
    import_apply_plan?: Pick<
      ProviderRemoteImportApplyPlan,
      "selected_operation_count" | "skipped_operation_count"
    >;
    import_dry_run?: Pick<
      ProviderRemoteImportDryRun,
      "conflict_count" | "failed_count" | "unsupported_count"
    > & {
      operations?: Array<{ status?: string; message: string }>;
    };
  },
): string {
  const plan = attempt.import_apply_plan;
  if (!plan) return "no apply plan";

  const dryRun = attempt.import_dry_run;
  const parts = [
    `${plan.selected_operation_count} selected`,
    plan.skipped_operation_count ? `${plan.skipped_operation_count} skipped` : null,
    ...formatProviderImportConflictParts(dryRun?.conflict_count, dryRun?.operations),
    dryRun?.failed_count ? `${dryRun.failed_count} failed` : null,
    ...formatProviderImportUnsupportedParts(dryRun?.unsupported_count, dryRun?.operations),
  ].filter(Boolean);

  return parts.join(", ");
}

export function formatProviderImportAttemptProvenance(
  attempt: Pick<
    ProviderRemoteMutationAttempt,
    "provider" | "direction" | "mutation_id" | "status" | "transport_kind" | "attempted_at"
  >,
): string {
  const direction = attempt.direction === "pull" ? "Pull" : "Push";
  return [
    `${direction} review`,
    attempt.provider.replace(/-/g, " "),
    attempt.transport_kind.replace(/-/g, " "),
    attempt.status.replace(/_/g, " "),
    attempt.attempted_at,
    attempt.mutation_id,
  ].join(" / ");
}

export function formatProviderImportApplyResultSummary(
  result: Pick<
    ProviderRemoteImportApplyExecutionResult,
    | "attempted_operation_count"
    | "applied_operation_count"
    | "skipped_operation_count"
    | "failed_operation_count"
    | "results"
  > &
    Partial<Pick<ProviderRemoteImportApplyExecutionResult, "blocked_operation_count">>,
): string {
  const blockedOperationCount =
    result.blocked_operation_count ?? getProviderImportApplyBlockedOperationCount(result.results);
  return [
    `${result.attempted_operation_count} attempted`,
    `${result.applied_operation_count} applied`,
    `${result.skipped_operation_count} skipped`,
    `${result.failed_operation_count} failed`,
    `${blockedOperationCount} blocked`,
  ].join(", ");
}

export function getProviderImportApplyBlockedOperationCount(
  results: Array<Pick<ProviderRemoteImportApplyExecutionResult["results"][number], "status">>,
): number {
  return results.filter((operation) => operation.status === "blocked").length;
}

export function formatAdminReplayResultSummary(
  result: Pick<
    LocalSyncReplayServerResultRecord,
    | "attempted_operation_count"
    | "applied_operation_count"
    | "skipped_operation_count"
    | "failed_operation_count"
    | "unsupported_operation_count"
  >,
): string {
  return formatBackgroundReplayPartialResult(result);
}

function fixtureGoogleDriveApiResponse(
  payload: Record<string, unknown>,
  status = 200,
): GoogleDriveApiResponse {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => payload,
    text: async () => JSON.stringify(payload),
  };
}

const sections: { id: AdminSection; label: string; icon: React.ReactNode; group: string }[] = [
  { id: "users", label: "Users", icon: <Users size={16} />, group: "Access" },
  { id: "spaces", label: "Spaces", icon: <Globe size={16} />, group: "Access" },
  { id: "groups", label: "Groups", icon: <UserRoundPlus size={16} />, group: "Access" },
  { id: "roles", label: "Roles", icon: <Shield size={16} />, group: "Access" },
  { id: "auth", label: "Authentication", icon: <Shield size={16} />, group: "Security" },
  { id: "api-keys", label: "API Keys", icon: <Key size={16} />, group: "Security" },
  { id: "webhooks", label: "Webhooks", icon: <Webhook size={16} />, group: "Integrations" },
  { id: "ai", label: "AI Settings", icon: <Sparkles size={16} />, group: "Integrations" },
  { id: "audit", label: "Audit Log", icon: <FileText size={16} />, group: "Operations" },
  { id: "notifications", label: "Notifications", icon: <Bell size={16} />, group: "Operations" },
  { id: "storage", label: "Storage", icon: <HardDrive size={16} />, group: "Operations" },
  { id: "search", label: "Search", icon: <Search size={16} />, group: "Operations" },
  { id: "plugins", label: "Legacy plugin archive", icon: <FileText size={16} />, group: "System" },
  { id: "system", label: "System", icon: <Settings size={16} />, group: "System" },
];

export const ADMIN_MOBILE_HOME_CLASS = "md:hidden min-w-0 flex-1 overflow-y-auto p-3";
export const ADMIN_MOBILE_SECTION_LIST_CLASS = "overflow-hidden rounded-card border border-border bg-surface shadow-card";
export const ADMIN_MOBILE_SECTION_ROW_CLASS =
  "flex min-h-[52px] w-full items-center gap-3 border-b border-border/70 px-3 py-2 text-left last:border-b-0 active:bg-surface-raised";
export const ADMIN_MOBILE_SECTION_ICON_CLASS =
  "flex h-8 w-8 shrink-0 items-center justify-center rounded-card bg-surface-raised text-text-muted";
// Kept exported for existing admin layout tests and downstream consumers.
export const ADMIN_MOBILE_BACK_BUTTON_CLASS =
  "mb-3 flex h-10 items-center gap-2 rounded-button px-2 text-sm text-primary active:bg-surface-raised";
export function getAdminSectionDescription(section: AdminSection, localMode: boolean): string {
  if (!isAdminSectionAvailable(section, localMode)) return "Unavailable in local standalone mode.";
  switch (section) {
    case "users":
      return "Invite, reset, activate, and review sessions.";
    case "spaces":
      return "Manage shared spaces and access.";
    case "groups":
      return "Group membership and permissions.";
    case "roles":
      return "Reusable access roles and their capabilities.";
    case "auth":
      return "2FA, signup, OIDC, and login policy.";
    case "api-keys":
      return "User and service API credentials.";
    case "webhooks":
      return "Outbound events and delivery history.";
    case "ai":
      return "AI provider availability and environment configuration.";
    case "audit":
      return "Administrative event history.";
    case "notifications":
      return "SMTP and notification channels.";
    case "storage":
      return "Vaults, imports, sync, and recovery.";
    case "search":
      return "Indexing and search maintenance.";
    case "plugins":
      return "Read-only retained browser records.";
    case "system":
      return "Application configuration.";
  }
}

const LOCAL_UNAVAILABLE_ADMIN_SECTIONS = new Set<AdminSection>([
  "ai",
  "api-keys",
  "audit",
  "auth",
  "groups",
  "roles",
  "notifications",
  "search",
  "users",
  "webhooks",
]);

export function isAdminSectionAvailable(section: AdminSection, localMode: boolean): boolean {
  return !localMode || !LOCAL_UNAVAILABLE_ADMIN_SECTIONS.has(section);
}

export function getAvailableAdminSections(localMode: boolean) {
  return sections.filter((section) => section.id !== "plugins" && isAdminSectionAvailable(section.id, localMode));
}

export function AdminPage({ initialSection = "users" }: { initialSection?: AdminSection } = {}) {
  const { user } = useAuth();
  const desktopLayout = useDesktopSettingsDialog();
  const { open: sidebarOpen } = useContext(AdminSidebarContext);
  const localMode = getCurrentLaunchMode() === "local";
  const [activeSection, setActiveSection] = useState<AdminSection>(() =>
    getInitialAdminSection(initialSection, localMode),
  );
  const [mobileSectionOpen, setMobileSectionOpen] = useState(() =>
    getInitialAdminSection(initialSection, localMode) === "plugins",
  );
  const availableSections = getAvailableAdminSections(localMode);
  const showSectionContent = mobileSectionOpen ? "block" : "hidden md:block";
  const legacyPluginSection = sections.find((section) => section.id === "plugins")!;
  const workspaceSections = activeSection === "plugins"
    ? [...availableSections, legacyPluginSection]
    : availableSections;
  const openPluginArchive = () => {
    setActiveSection("plugins");
    setMobileSectionOpen(true);
  };

  if (!localMode && !canUseHumanAdministration(user?.role, user?.account_kind)) {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-text-muted text-sm">Admin access required</p>
      </div>
    );
  }

  const sectionContent = (
    <>
      {localMode && (
        <div className="rounded-card border-border bg-surface text-text-muted mb-4 border px-3 py-2 text-xs">
          Local mode is active. Server-admin actions are unavailable in this mode.
        </div>
      )}
      {activeSection === "users" && <UsersSection localMode={localMode} />}
      {activeSection === "spaces" && <SpacesSection localMode={localMode} />}
      {activeSection === "groups" && <GroupsSection localMode={localMode} />}
      {activeSection === "roles" && <RolesSection localMode={localMode} />}
      {activeSection === "auth" && <AuthenticationSection localMode={localMode} />}
      {activeSection === "api-keys" && <ApiKeysSection localMode={localMode} />}
      {activeSection === "webhooks" && <WebhooksSection localMode={localMode} />}
      {activeSection === "ai" && <AiSettingsSection localMode={localMode} />}
      {activeSection === "notifications" && <NotificationsSection localMode={localMode} />}
      {activeSection === "audit" && <AuditSection localMode={localMode} />}
      {activeSection === "storage" && <StorageSection localMode={localMode} user={user} />}
      {activeSection === "search" && <SearchSection localMode={localMode} />}
      {activeSection === "plugins" && (
        <div className="space-y-4">
          <Button variant="secondary" onClick={() => {
            setActiveSection("system");
            setMobileSectionOpen(false);
          }}>Back to Administration</Button>
          <PluginArchiveSection />
        </div>
      )}
      {activeSection === "system" && <SystemSection localMode={localMode} onOpenPluginArchive={openPluginArchive} />}
    </>
  );

  if (desktopLayout) {
    return (
      <SettingsWorkspace
        title="Site Administration"
        description="Manage access, authentication, integrations, and server operations."
        sections={workspaceSections}
        activeSection={activeSection}
        onSectionChange={(section) => setActiveSection(section as AdminSection)}
      >
        {sectionContent}
      </SettingsWorkspace>
    );
  }

  return (
    <div className="flex h-full min-w-0 flex-col md:flex-row">
      {/* Admin sidebar */}
      {sidebarOpen && (
        <nav className="border-border bg-surface hidden w-full shrink-0 border-b p-2 md:block md:w-48 md:border-b-0 md:border-r">
          <h2 className="text-text-muted mb-2 px-2 text-xs font-medium">
            Admin
          </h2>
          {availableSections.map((s) => (
            <button
              key={s.id}
              onClick={() => setActiveSection(s.id)}
              className={`rounded-button flex w-full items-center gap-2 px-2 py-1.5 text-sm transition-colors duration-100 ${
                activeSection === s.id
                  ? "bg-surface-raised text-text"
                  : "text-text-muted hover:bg-surface-raised hover:text-text"
              }`}
            >
              {s.icon}
              {s.label}
            </button>
          ))}
        </nav>
      )}

      <div className={mobileSectionOpen ? "hidden md:hidden" : ADMIN_MOBILE_HOME_CLASS}>
        <div className="mb-4">
          <h1 className="text-text text-lg font-semibold">Site Administration</h1>
          <p className="text-text-muted mt-1 text-sm">
            Choose a settings area. Each row opens as its own mobile subpage.
          </p>
        </div>
        {localMode && (
          <div className="rounded-card border-border bg-surface text-text-muted mb-4 border px-3 py-2 text-xs">
            Local mode is active. Server-admin actions are unavailable in this mode.
          </div>
        )}
        <div className={ADMIN_MOBILE_SECTION_LIST_CLASS}>
          {availableSections.map((section) => (
            <button
              key={section.id}
              type="button"
              className={ADMIN_MOBILE_SECTION_ROW_CLASS}
              onClick={() => {
                setActiveSection(section.id);
                setMobileSectionOpen(true);
              }}
            >
              <span className={ADMIN_MOBILE_SECTION_ICON_CLASS}>{section.icon}</span>
              <span className="min-w-0 flex-1">
                <span className="block truncate text-sm text-text">{section.label}</span>
                <span className="mt-0.5 block truncate text-xs text-text-muted">
                  {getAdminSectionDescription(section.id, localMode)}
                </span>
              </span>
              <ChevronRight size={17} className="shrink-0 text-text-muted" />
            </button>
          ))}
        </div>
      </div>

      {/* Content */}
      <div className={`${showSectionContent} min-w-0 flex-1 overflow-y-auto p-3 md:p-6`}>
        {sectionContent}
      </div>
    </div>
  );
}

const ADMIN_AI_PROVIDER_DEFAULTS: Record<
  AdminAiProviderId,
  { label: string; baseUrl: string; model: string; requiresKey: boolean }
> = {
  ollama: {
    label: "Ollama",
    baseUrl: "http://localhost:11434",
    model: "llama3.2",
    requiresKey: false,
  },
  openai: {
    label: "OpenAI",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-4o",
    requiresKey: true,
  },
  anthropic: {
    label: "Anthropic",
    baseUrl: "https://api.anthropic.com/v1",
    model: "claude-sonnet-4-20250514",
    requiresKey: true,
  },
  xai: {
    label: "xAI",
    baseUrl: "https://api.x.ai/v1",
    model: "grok-2-latest",
    requiresKey: true,
  },
};

export function getAdminAiProviderDraft(
  provider: AdminAiProvider | AdminAiProviderId,
): AdminAiProviderDraft {
  const id = typeof provider === "string" ? provider : provider.id;
  const defaults = ADMIN_AI_PROVIDER_DEFAULTS[id];
  return {
    id,
    base_url: typeof provider === "string" ? defaults.baseUrl : provider.base_url,
    api_key: "",
    model: typeof provider === "string" ? defaults.model : provider.model,
  };
}

export function getAdminAiProviderStatus(
  provider: AdminAiProvider,
): { label: string; variant: "outline" | "success" | "warning" } {
  if (!provider.configured) return { label: "Not configured", variant: "outline" };
  if (provider.is_default) return { label: "Default", variant: "success" };
  return { label: "Available", variant: "warning" };
}

function AiSettingsSection({ localMode }: { localMode: boolean }) {
  const [config, setConfig] = useState<AdminAiConfig | null>(null);
  const [providers, setProviders] = useState<AdminAiProvider[]>([]);
  const [providerDraft, setProviderDraft] = useState<AdminAiProviderDraft | null>(null);
  const [modelProvider, setModelProvider] = useState<AdminAiProvider | null>(null);
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [selectedModel, setSelectedModel] = useState("");
  const [removeTarget, setRemoveTarget] = useState<AdminAiProvider | null>(null);
  const [loading, setLoading] = useState(true);
  const [busyProvider, setBusyProvider] = useState<AdminAiProviderId | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const loadConfig = useCallback(async () => {
    if (localMode) return;
    setLoading(true);
    setError(null);
    try {
      const [nextConfig, nextProviders] = await Promise.all([
        api.fetch<AdminAiConfig>("/ai/config"),
        api.fetch<AdminAiProvider[]>("/ai/providers"),
      ]);
      setConfig(nextConfig);
      setProviders(nextProviders);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to load AI configuration");
      setConfig(null);
      setProviders([]);
    } finally {
      setLoading(false);
    }
  }, [localMode]);

  useEffect(() => {
    void loadConfig();
  }, [loadConfig]);

  if (localMode) return null;

  const configuredProviders = providers.filter((provider) => provider.configured);
  const unconfiguredProviders = providers.filter((provider) => !provider.configured);
  const defaultProvider = configuredProviders.find((provider) => provider.is_default);

  const saveProvider = async () => {
    if (!providerDraft) return;
    setSaving(true);
    setError(null);
    setMessage(null);
    try {
      await api.fetch(`/ai/providers/${providerDraft.id}`, {
        method: "PUT",
        body: {
          base_url: providerDraft.base_url,
          api_key: providerDraft.api_key || null,
          model: providerDraft.model,
        },
      });
      setProviderDraft(null);
      setMessage(`${ADMIN_AI_PROVIDER_DEFAULTS[providerDraft.id].label} saved.`);
      await loadConfig();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to save the AI provider");
    } finally {
      setSaving(false);
    }
  };

  const testProvider = async (provider: AdminAiProvider) => {
    setBusyProvider(provider.id);
    setError(null);
    setMessage(null);
    try {
      const result = await api.fetch<AdminAiProviderTest>(
        `/ai/providers/${provider.id}/test`,
        { method: "POST" },
      );
      setAvailableModels(result.models);
      setSelectedModel(provider.model);
      setModelProvider(provider);
      setMessage(result.message);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to connect to the AI provider");
    } finally {
      setBusyProvider(null);
    }
  };

  const saveModel = async () => {
    if (!modelProvider || !selectedModel.trim()) return;
    setSaving(true);
    setError(null);
    try {
      await api.fetch(`/ai/providers/${modelProvider.id}`, {
        method: "PUT",
        body: {
          base_url: modelProvider.base_url,
          api_key: null,
          model: selectedModel.trim(),
        },
      });
      setModelProvider(null);
      setMessage(`${modelProvider.label} will use ${selectedModel.trim()}.`);
      await loadConfig();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to save the selected model");
    } finally {
      setSaving(false);
    }
  };

  const setDefaultProvider = async (provider: AdminAiProvider) => {
    setBusyProvider(provider.id);
    setError(null);
    setMessage(null);
    try {
      await api.fetch("/ai/settings", {
        method: "PUT",
        body: { default_provider: provider.id },
      });
      setMessage(`${provider.label} is now the default AI provider.`);
      await loadConfig();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to change the default provider");
    } finally {
      setBusyProvider(null);
    }
  };

  const setAiEnabled = async (enabled: boolean) => {
    setError(null);
    setMessage(null);
    try {
      await api.fetch("/ai/settings", { method: "PUT", body: { enabled } });
      setMessage(`AI features ${enabled ? "enabled" : "disabled"}.`);
      await loadConfig();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to change AI availability");
    }
  };

  const removeProvider = async () => {
    if (!removeTarget) return;
    setSaving(true);
    setError(null);
    try {
      await api.fetch(`/ai/providers/${removeTarget.id}`, { method: "DELETE" });
      setMessage(`${removeTarget.label} application configuration removed.`);
      setRemoveTarget(null);
      await loadConfig();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to remove the AI provider");
    } finally {
      setSaving(false);
    }
  };

  return (
    <section aria-label="AI Settings" className="flex flex-col gap-4">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="flex items-center gap-2">
            <Sparkles size={18} className="text-ai" aria-hidden="true" />
            <h1 className="hidden text-lg font-semibold text-text md:block">AI Settings</h1>
          </div>
          <p className="hidden mt-1 text-sm text-text-muted md:block">
            Connect providers, verify access, load models, and choose the server default.
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button
            type="button"
            variant="secondary"
            size="sm"
            onClick={() => void loadConfig()}
            disabled={loading}
          >
            <RefreshCw size={14} className={loading ? "animate-spin" : undefined} />
            Refresh
          </Button>
          <Button
            type="button"
            size="sm"
            onClick={() => {
              const first = unconfiguredProviders[0];
              if (first) setProviderDraft(getAdminAiProviderDraft(first.id));
            }}
            disabled={loading || unconfiguredProviders.length === 0}
            className="border-ai bg-ai text-white hover:bg-ai/90"
          >
            <Plus size={14} />
            Add provider
          </Button>
        </div>
      </header>

      {error && (
        <div
          className="rounded-card border border-danger/50 bg-danger/5 px-3 py-2 text-sm text-text"
          role="alert"
        >
          <div className="flex items-start gap-2">
            <CircleAlert size={16} className="mt-0.5 shrink-0 text-danger" aria-hidden="true" />
            <span>{error}</span>
          </div>
        </div>
      )}
      {message && (
        <div
          className="rounded-card border border-success/40 bg-success/5 px-3 py-2 text-sm text-text"
          role="status"
        >
          <div className="flex items-start gap-2">
            <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-success" aria-hidden="true" />
            <span>{message}</span>
          </div>
        </div>
      )}

      <div className="rounded-card border border-border bg-surface shadow-card">
        <div className="flex flex-col divide-y divide-border">
          <div className="flex flex-wrap items-center justify-between gap-4 px-4 py-3">
            <div>
              <p className="text-sm font-medium text-text">AI availability</p>
              <p className="mt-0.5 text-xs text-text-muted">
                Enable document assistance for authenticated users.
                {config?.enabled_locked ? " Controlled by AI_ENABLED." : ""}
              </p>
            </div>
            <AuthenticationToggle
              checked={config?.enabled ?? false}
              disabled={loading || config?.enabled_locked}
              label="Enable AI features"
              onChange={(enabled) => void setAiEnabled(enabled)}
            />
          </div>
          <div className="flex flex-wrap items-center justify-between gap-4 px-4 py-3">
            <div>
              <p className="text-sm font-medium text-text">Default route</p>
              <p className="mt-0.5 text-xs text-text-muted">
                {defaultProvider
                  ? `${defaultProvider.label} · ${defaultProvider.model}`
                  : "No configured provider is available yet."}
              </p>
            </div>
            {defaultProvider && (
              <Badge variant="success">
                <Bot size={12} aria-hidden="true" />
                {defaultProvider.label}
              </Badge>
            )}
          </div>
        </div>
      </div>

      <div className="rounded-card border border-border bg-surface shadow-card">
        <div className="flex flex-wrap items-start justify-between gap-3 border-b border-border px-4 py-3">
          <div>
            <h2 className="text-sm font-medium text-text">Providers</h2>
            <p className="mt-0.5 text-xs text-text-muted">
              Environment credentials remain masked and locked. Application providers can be
              edited or removed here.
            </p>
          </div>
          <span className="text-xs text-text-muted">
            {configuredProviders.length} configured
          </span>
        </div>

        {loading ? (
          <p className="px-4 py-8 text-center text-sm text-text-muted">
            Loading AI providers...
          </p>
        ) : configuredProviders.length === 0 ? (
          <div className="flex flex-col items-center gap-2 px-4 py-10 text-center">
            <ServerCog size={24} className="text-ai" aria-hidden="true" />
            <p className="text-sm font-medium text-text">No AI provider configured</p>
            <p className="max-w-md text-xs text-text-muted">
              Add Ollama, OpenAI, Anthropic, or xAI, then test the connection and choose a model.
            </p>
          </div>
        ) : (
          <div className="divide-y divide-border">
            {configuredProviders.map((provider) => {
              const status = getAdminAiProviderStatus(provider);
              return (
                <article
                  key={provider.id}
                  className="flex flex-col gap-3 px-4 py-3 lg:flex-row lg:items-center"
                >
                  <div className="flex min-w-0 flex-1 items-start gap-3">
                    <span
                      className="flex size-9 shrink-0 items-center justify-center rounded-input border border-ai/20 bg-ai/5 text-ai"
                      aria-hidden="true"
                    >
                      <Sparkles size={16} />
                    </span>
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <h3 className="text-sm font-medium text-text">{provider.label}</h3>
                        <Badge variant={status.variant}>{status.label}</Badge>
                        <Badge variant="outline">
                          {provider.source === "environment" ? "Environment" : "Application"}
                        </Badge>
                      </div>
                      <p className="mt-1 truncate text-xs text-text-muted">{provider.base_url}</p>
                      <p className="mt-0.5 text-xs text-text-muted">
                        Model: <span className="font-mono text-text">{provider.model}</span>
                      </p>
                    </div>
                  </div>
                  <div className="flex flex-wrap items-center gap-2 lg:justify-end">
                    <Button
                      type="button"
                      size="sm"
                      variant="secondary"
                      disabled={busyProvider === provider.id}
                      onClick={() => void testProvider(provider)}
                    >
                      <PlugZap size={14} />
                      {busyProvider === provider.id ? "Testing..." : "Test & load models"}
                    </Button>
                    {!provider.is_default && (
                      <Button
                        type="button"
                        size="sm"
                        variant="secondary"
                        disabled={busyProvider === provider.id}
                        onClick={() => void setDefaultProvider(provider)}
                      >
                        Set default
                      </Button>
                    )}
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      onClick={() => setProviderDraft(getAdminAiProviderDraft(provider))}
                    >
                      <Pencil size={14} />
                      Edit
                    </Button>
                    {provider.removable && (
                      <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        className="text-danger hover:text-danger"
                        onClick={() => setRemoveTarget(provider)}
                      >
                        <Trash2 size={14} />
                        Remove
                      </Button>
                    )}
                  </div>
                </article>
              );
            })}
          </div>
        )}
      </div>

      {providerDraft && (
        <AiProviderDialog
          draft={providerDraft}
          providers={providers}
          saving={saving}
          onChange={setProviderDraft}
          onClose={() => setProviderDraft(null)}
          onSave={() => void saveProvider()}
        />
      )}

      {modelProvider && (
        <DialogShell
          labelledBy="ai-model-picker-title"
          maxWidth="md"
          onClose={() => setModelProvider(null)}
        >
          <div className="flex flex-col">
            <div className="border-b border-border px-4 py-3">
              <h2 id="ai-model-picker-title" className="text-base font-semibold text-text">
                Choose {modelProvider.label} model
              </h2>
              <p className="mt-0.5 text-xs text-text-muted">
                The connection succeeded. Select a discovered model or enter an exact model ID.
              </p>
            </div>
            <div className="flex flex-col gap-3 p-4">
              <label htmlFor="ai-provider-model" className="text-sm font-medium text-text">
                Model
              </label>
              <Select
                id="ai-provider-model"
                value={selectedModel}
                onChange={(event) => setSelectedModel(event.target.value)}
              >
                {!availableModels.includes(selectedModel) && selectedModel && (
                  <option value={selectedModel}>{selectedModel}</option>
                )}
                {availableModels.map((model) => (
                  <option key={model} value={model}>
                    {model}
                  </option>
                ))}
              </Select>
              <Input
                value={selectedModel}
                onChange={(event) => setSelectedModel(event.target.value)}
                placeholder="Exact model ID"
                aria-label="Exact model ID"
              />
            </div>
            <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
              <Button type="button" variant="ghost" onClick={() => setModelProvider(null)}>
                Cancel
              </Button>
              <Button
                type="button"
                disabled={saving || !selectedModel.trim()}
                onClick={() => void saveModel()}
                className="border-ai bg-ai text-white hover:bg-ai/90"
              >
                {saving ? "Saving..." : "Use model"}
              </Button>
            </div>
          </div>
        </DialogShell>
      )}

      {removeTarget && (
        <ConfirmDialog
          labelledBy="remove-ai-provider-title"
          title={`Remove ${removeTarget.label}?`}
          message="Application-stored credentials and model selection will be removed. Environment configuration, if present, remains active."
          confirmLabel={saving ? "Removing..." : "Remove provider"}
          disabled={saving}
          destructive
          onConfirm={() => void removeProvider()}
          onCancel={() => setRemoveTarget(null)}
        />
      )}
    </section>
  );
}

function AiProviderDialog({
  draft,
  providers,
  saving,
  onChange,
  onSave,
  onClose,
}: {
  draft: AdminAiProviderDraft;
  providers: AdminAiProvider[];
  saving: boolean;
  onChange: (draft: AdminAiProviderDraft) => void;
  onSave: () => void;
  onClose: () => void;
}) {
  const existing = providers.find((provider) => provider.id === draft.id);
  const defaults = ADMIN_AI_PROVIDER_DEFAULTS[draft.id];
  const creating = !existing?.configured;
  const selectableProviders = providers.filter(
    (provider) => !provider.configured || provider.id === draft.id,
  );

  return (
    <DialogShell labelledBy="ai-provider-dialog-title" maxWidth="lg" onClose={onClose}>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          onSave();
        }}
        className="flex flex-col"
      >
        <div className="border-b border-border px-4 py-3">
          <h2 id="ai-provider-dialog-title" className="text-base font-semibold text-text">
            {creating ? "Add AI provider" : `Edit ${existing?.label ?? defaults.label}`}
          </h2>
          <p className="mt-0.5 text-xs text-text-muted">
            Credentials are sent only to the Starlane server and are never returned by the API.
          </p>
        </div>
        <div className="flex max-h-[calc(100dvh-10rem)] flex-col gap-4 overflow-y-auto p-4">
          <div className="flex flex-col gap-1.5">
            <label htmlFor="ai-provider-kind" className="text-sm font-medium text-text">
              Provider
            </label>
            <Select
              id="ai-provider-kind"
              value={draft.id}
              disabled={!creating}
              onChange={(event) =>
                onChange(getAdminAiProviderDraft(event.target.value as AdminAiProviderId))
              }
            >
              {selectableProviders.map((provider) => (
                <option key={provider.id} value={provider.id}>
                  {provider.label}
                </option>
              ))}
            </Select>
          </div>
          <div className="flex flex-col gap-1.5">
            <label htmlFor="ai-provider-base-url" className="text-sm font-medium text-text">
              Base URL
            </label>
            <Input
              id="ai-provider-base-url"
              type="url"
              value={draft.base_url}
              disabled={existing?.base_url_locked}
              onChange={(event) => onChange({ ...draft, base_url: event.target.value })}
              required
            />
            {existing?.base_url_locked && (
              <p className="text-xs text-text-muted">The environment controls this endpoint.</p>
            )}
          </div>
          {defaults.requiresKey && (
            <div className="flex flex-col gap-1.5">
              <label htmlFor="ai-provider-api-key" className="text-sm font-medium text-text">
                API key
              </label>
              <Input
                id="ai-provider-api-key"
                type="password"
                value={draft.api_key}
                disabled={existing?.api_key_locked}
                onChange={(event) => onChange({ ...draft, api_key: event.target.value })}
                placeholder={
                  existing?.has_api_key ? "Leave blank to keep the saved key" : "Enter API key"
                }
                required={!existing?.has_api_key}
                autoComplete="new-password"
              />
              <p className="text-xs text-text-muted">
                {existing?.api_key_locked
                  ? "The environment supplies this credential."
                  : "The saved key is write-only and remains masked after saving."}
              </p>
            </div>
          )}
          <div className="flex flex-col gap-1.5">
            <label htmlFor="ai-provider-default-model" className="text-sm font-medium text-text">
              Initial model
            </label>
            <Input
              id="ai-provider-default-model"
              value={draft.model}
              onChange={(event) => onChange({ ...draft, model: event.target.value })}
              required
              placeholder={defaults.model}
            />
            <p className="text-xs text-text-muted">
              Save first, then use Test &amp; load models to retrieve the provider&apos;s catalog.
            </p>
          </div>
        </div>
        <div className="flex justify-end gap-2 border-t border-border px-4 py-3">
          <Button type="button" variant="ghost" onClick={onClose} disabled={saving}>
            Cancel
          </Button>
          <Button
            type="submit"
            disabled={
              saving ||
              !draft.base_url.trim() ||
              !draft.model.trim() ||
              (defaults.requiresKey && !existing?.has_api_key && !draft.api_key.trim())
            }
            className="border-ai bg-ai text-white hover:bg-ai/90"
          >
            {saving ? "Saving..." : creating ? "Add provider" : "Save provider"}
          </Button>
        </div>
      </form>
    </DialogShell>
  );
}

function getInitialAdminSection(fallback: AdminSection, localMode = false): AdminSection {
  const safeFallback = isAdminSectionAvailable(fallback, localMode) ? fallback : "storage";
  if (typeof window === "undefined") return safeFallback;
  const section = new URLSearchParams(window.location.search).get("section");
  return sections.some(
    (item) => item.id === section && isAdminSectionAvailable(item.id, localMode),
  )
    ? (section as AdminSection)
    : safeFallback;
}

function roleLabel(role: string) {
  return getSpaceRoleLabel(role);
}

export function AdminFormSelect({
  value,
  onChange,
  ariaLabel,
  children,
}: {
  value: string;
  onChange: (value: string) => void;
  ariaLabel: string;
  children: ReactNode;
}) {
  return (
    <Select
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className="h-9 min-h-11 bg-surface px-2 text-sm sm:min-h-0"
      aria-label={ariaLabel}
    >
      {children}
    </Select>
  );
}

export function AdminTextInput({
  value,
  onChange,
  ariaLabel,
  placeholder,
  type = "text",
  className = "",
  required = false,
  disabled = false,
  spellCheck,
  id,
}: {
  value: string;
  onChange: (value: string) => void;
  ariaLabel: string;
  placeholder?: string;
  type?: InputHTMLAttributes<HTMLInputElement>["type"];
  className?: string;
  required?: boolean;
  disabled?: boolean;
  spellCheck?: InputHTMLAttributes<HTMLInputElement>["spellCheck"];
  id?: string;
}) {
  return (
    <Input
      id={id}
      value={value}
      onChange={(event) => onChange(event.target.value)}
      type={type}
      required={required}
      disabled={disabled}
      placeholder={placeholder}
      spellCheck={spellCheck}
      className={`min-h-11 min-w-0 px-2 text-sm sm:min-h-0 ${className}`}
      aria-label={ariaLabel}
    />
  );
}

export function AdminInlineSelect({
  value,
  onChange,
  ariaLabel,
  children,
  className = "",
}: {
  value: string;
  onChange: (value: string) => void;
  ariaLabel: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <Select
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className={`h-8 min-h-11 px-2 text-sm sm:min-h-0 ${className}`}
      aria-label={ariaLabel}
    >
      {children}
    </Select>
  );
}

export function RoleSelect({
  value,
  onChange,
  disabled,
}: {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  return (
    <Select
      value={value}
      onChange={(event) => onChange(event.target.value)}
      disabled={disabled}
      className="h-8 min-h-11 px-2 text-sm sm:min-h-0"
      aria-label="Permission role"
    >
      <option value="viewer">Viewer</option>
      <option value="commenter">Commenter</option>
      <option value="editor">Editor</option>
      <option value="space_admin">Space admin</option>
    </Select>
  );
}

export function UserRoleSelect({
  value,
  onChange,
  disabled,
}: {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  return (
    <Select
      value={value}
      onChange={(event) => onChange(event.target.value)}
      disabled={disabled}
      className="h-8 min-h-11 px-2 text-sm sm:min-h-0"
      aria-label="User role"
    >
      <option value="member">Member</option>
      <option value="admin">Admin</option>
    </Select>
  );
}

function parseCsvLine(line: string): string[] {
  const cells: string[] = [];
  let current = "";
  let quoted = false;
  for (let index = 0; index < line.length; index += 1) {
    const char = line[index];
    const next = line[index + 1];
    if (char === '"' && quoted && next === '"') {
      current += '"';
      index += 1;
    } else if (char === '"') {
      quoted = !quoted;
    } else if (char === "," && !quoted) {
      cells.push(current.trim());
      current = "";
    } else {
      current += char;
    }
  }
  cells.push(current.trim());
  return cells;
}

export function parseAdminUserCsv(text: string): AdminUserImportInput[] {
  const rows = text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map(parseCsvLine);
  if (rows.length === 0) return [];

  const first = rows[0].map((cell) => cell.toLowerCase().replace(/[\s_-]+/g, ""));
  const hasHeader = first.includes("email") || first.includes("displayname") || first.includes("name");
  const emailIndex = hasHeader ? first.indexOf("email") : 0;
  const nameIndex = hasHeader
    ? first.findIndex((cell) => cell === "displayname" || cell === "name")
    : 1;
  const roleIndex = hasHeader ? first.indexOf("role") : 2;
  const dataRows = hasHeader ? rows.slice(1) : rows;

  return dataRows.map((row, index) => {
    const email = row[emailIndex]?.trim().toLowerCase() ?? "";
    const displayName = row[nameIndex]?.trim() || email.split("@")[0] || "";
    const roleValue = (roleIndex >= 0 ? row[roleIndex] : "member")?.trim().toLowerCase() || "member";
    if (!email) {
      throw new Error(`CSV row ${index + 1} is missing an email address.`);
    }
    if (roleValue !== "admin" && roleValue !== "member") {
      throw new Error(`CSV row ${index + 1} has an invalid role.`);
    }
    return {
      email,
      display_name: displayName,
      role: roleValue,
    };
  });
}

function UsersSection({ localMode }: { localMode: boolean }) {
  const { user } = useAuth();
  const [users, setUsers] = useState<UserItem[]>([]);
  const [usersLoading, setUsersLoading] = useState(!localMode);
  const [inviteOpen, setInviteOpen] = useState(false);
  const [inviteEmail, setInviteEmail] = useState("");
  const [inviteName, setInviteName] = useState("");
  const [inviteRole, setInviteRole] = useState<"admin" | "member">("member");
  const [temporaryPasswords, setTemporaryPasswords] = useState<{ email: string; password: string }[]>([]);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [busyUserId, setBusyUserId] = useState<string | null>(null);
  const [userQuery, setUserQuery] = useState("");
  const [userRoleFilter, setUserRoleFilter] = useState<UserRoleFilter>("all");
  const [userStatusFilter, setUserStatusFilter] = useState<UserStatusFilter>("all");
  const [selectedUserIds, setSelectedUserIds] = useState<Set<string>>(() => new Set());
  const [bulkLifecycleBusy, setBulkLifecycleBusy] = useState(false);
  const [sessionUser, setSessionUser] = useState<UserItem | null>(null);
  const [sessions, setSessions] = useState<UserSessionItem[]>([]);
  const [sessionsLoading, setSessionsLoading] = useState(false);
  const [sessionRevokeTarget, setSessionRevokeTarget] = useState<UserSessionItem | null>(null);
  const [editingUser, setEditingUser] = useState<UserItem | null>(null);
  const [editingUserRole, setEditingUserRole] = useState<"admin" | "member">("member");
  const importInputRef = useRef<HTMLInputElement | null>(null);

  const userLifecycleCounts = useMemo(() => getAdminUserLifecycleCounts(users), [users]);

  const visibleUsers = useMemo(
    () =>
      filterAdminUsers(users, {
        query: userQuery,
        role: userRoleFilter,
        status: userStatusFilter,
      }),
    [users, userQuery, userRoleFilter, userStatusFilter],
  );

  const visibleSelectionState = useMemo(
    () => getAdminUserVisibleSelectionState(visibleUsers, selectedUserIds),
    [visibleUsers, selectedUserIds],
  );
  const { selectedVisibleUserIds, allVisibleUsersSelected } = visibleSelectionState;

  const usersMounted=useRef(false),usersGeneration=useRef(0),usersOrigin=useRef<(()=>void)|null>(null);
  const usersCurrent=useRef(users),actorCurrent=useRef(user?.id);
  usersCurrent.current=users;actorCurrent.current=user?.id;
  const [usersContextKey,setUsersContextKey]=useState("");
  const loadUsers = useCallback(async () => {
    const request=++usersGeneration.current;
    usersOrigin.current=null;
    if (localMode) {setUsersLoading(false);setUsers([]);return;}
    setUsersLoading(true);setError("");
    const originalActor=actorCurrent.current,originalBase=getApiBase();
    const current=()=>{if(!usersMounted.current||request!==usersGeneration.current||actorCurrent.current!==originalActor)throw new Error("Administrator changed. Refresh users.");};
    let originalVerify=current;
    try {
      const transport=captureGuestClassificationTransport(current);
      originalVerify=transport.verify;
      const items=await transport.users();transport.verify();
      usersOrigin.current=transport.verify;setUsers(items);setUsersContextKey(`${originalActor??""}/${originalBase}`);
    }catch(err) {
      try {originalVerify();} catch {return;}
      const failureState=getAdminUserLifecycleFailureState<UserItem,UserSessionItem>();
      setUsers(failureState.users);setSelectedUserIds(failureState.selectedUserIds);setSessionUser(failureState.sessionUser);setSessions(failureState.sessions);
      setError(err instanceof Error?err.message:"Unable to load users");
    }finally {if(usersMounted.current&&request===usersGeneration.current)setUsersLoading(false);}
  }, [localMode]);
  useEffect(()=>{const lifecycleGeneration=usersGeneration;usersMounted.current=true;return()=>{usersMounted.current=false;lifecycleGeneration.current++;usersOrigin.current=null;};},[]);

  useEffect(() => {
    loadUsers();
  }, [loadUsers, user?.id, user?.account_kind]);

  let usersSourceCurrent=localMode;
  try {if(usersOrigin.current){usersOrigin.current();usersSourceCurrent=true;}} catch {/* Suppress stale original-account labels until a fresh load. */}
  const displayedUsers=usersSourceCurrent?visibleUsers:[];
  const displayedLifecycleCounts=usersSourceCurrent?userLifecycleCounts:getAdminUserLifecycleCounts([]);

  const updateUser = async (target: UserItem, patch: Partial<Pick<UserItem, "role" | "is_active">>) => {
    setBusyUserId(target.id);
    setMessage("");
    setError("");
    try {
      await api.fetch(`/admin/users/${target.id}`, {
        method: "PUT",
        body: patch,
      });
      setUsers((prev) => prev.map((item) => (item.id === target.id ? { ...item, ...patch } : item)));
      setMessage("User updated.");
      return true;
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to update user");
      await loadUsers();
      return false;
    } finally {
      setBusyUserId(null);
    }
  };

  const openUserEditor = (target: UserItem) => {
    setEditingUser(target);
    setEditingUserRole(target.role === "admin" ? "admin" : "member");
  };

  const saveUserEditor = async () => {
    if (!editingUser) return;
    const updated = await updateUser(editingUser, { role: editingUserRole });
    if (updated) setEditingUser(null);
  };

  const toggleUserSelection = (userId: string, selected: boolean) => {
    setSelectedUserIds((prev) => {
      const next = new Set(prev);
      if (selected) {
        next.add(userId);
      } else {
        next.delete(userId);
      }
      return next;
    });
  };

  const toggleVisibleUserSelection = (selected: boolean) => {
    setSelectedUserIds((prev) => {
      return getAdminUserSelectionAfterVisibleToggle(visibleUsers, prev, selected);
    });
  };

  const runBulkLifecycle = async (action: AdminUserBulkLifecycleAction) => {
    const userIds = Array.from(selectedUserIds);
    if (userIds.length === 0) return;
    setBulkLifecycleBusy(true);
    setMessage("");
    setError("");
    try {
      const result = await api.fetch<UserBulkLifecycleResult>("/admin/users/bulk-lifecycle", {
        method: "POST",
        body: buildAdminUserBulkLifecycleRequest(userIds, action),
      });
      const skippedIds = new Set(result.skipped.map((item) => item.user_id).filter(Boolean));
      setUsers((prev) =>
        prev.map((item) =>
          selectedUserIds.has(item.id) && !skippedIds.has(item.id)
            ? { ...item, is_active: action === "activate" }
            : item,
        ),
      );
      setSelectedUserIds(new Set());
      setMessage(formatAdminUserBulkLifecycleMessage(result));
    } catch (err) {
      setError(err instanceof Error ? err.message : `Unable to ${action} selected users`);
      await loadUsers();
    } finally {
      setBulkLifecycleBusy(false);
    }
  };

  const loadSessions = async (target: UserItem) => {
    if (localMode) return;
    setSessionUser(target);
    setSessions([]);
    setSessionsLoading(true);
    setMessage("");
    setError("");
    try {
      const items = await api.fetch<UserSessionItem[]>(`/admin/users/${target.id}/sessions`);
      setSessions(items);
    } catch (err) {
      setSessions([]);
      setError(err instanceof Error ? err.message : "Unable to load sessions");
    } finally {
      setSessionsLoading(false);
    }
  };

  const closeSessions = () => {
    const closedState = getAdminUserSessionPanelClosedState<UserItem, UserSessionItem>();
    setSessionUser(closedState.sessionUser);
    setSessions(closedState.sessions);
    setSessionsLoading(closedState.sessionsLoading);
    setSessionRevokeTarget(null);
  };

  const revokeSession = async (session: UserSessionItem) => {
    if (!sessionUser) return;
    setMessage("");
    setError("");
    try {
      await api.fetch(
        `/admin/users/${sessionUser.id}/sessions/${encodeURIComponent(session.id)}`,
        { method: "DELETE" },
      );
      setSessions((prev) => prev.filter((item) => item.id !== session.id));
      setSessionRevokeTarget(null);
      setMessage("Session revoked.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to revoke session");
    }
  };

  const resetPassword = async (target: UserItem) => {
    setBusyUserId(target.id);
    setMessage("");
    setError("");
    try {
      const result = await api.fetch<UserPasswordResetResult>(
        `/admin/users/${target.id}/password-reset`,
        { method: "POST" },
      );
      setTemporaryPasswords((prev) => [
        { email: result.email, password: result.temporary_password },
        ...prev,
      ].slice(0, 10));
      setMessage(`Reset password for ${result.email}. Copy the temporary password before leaving this page.`);
      const sessionState = getAdminUserPasswordResetSessionState(
        sessionUser,
        target,
        sessions,
        sessionsLoading,
      );
      setSessions(sessionState.sessions);
      setSessionsLoading(sessionState.sessionsLoading);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to reset password");
    } finally {
      setBusyUserId(null);
    }
  };

  const createUser = async (input: AdminUserImportInput) => {
    const created = await api.fetch<UserInviteResult>("/admin/users", {
      method: "POST",
      body: input,
    });
    setUsers((prev) => [created, ...prev.filter((item) => item.id !== created.id)]);
    setTemporaryPasswords((prev) => [
      { email: created.email, password: created.temporary_password },
      ...prev,
    ].slice(0, 10));
    return created;
  };

  const submitInvite = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setMessage("");
    setError("");
    try {
      const created = await createUser({
        email: inviteEmail,
        display_name: inviteName,
        role: inviteRole,
      });
      setInviteEmail("");
      setInviteName("");
      setInviteRole("member");
      setInviteOpen(false);
      setMessage(`Created ${created.email}. Copy the temporary password before leaving this page.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to create user");
    }
  };

  const importCsv = async (file: File) => {
    setMessage("");
    setError("");
    try {
      const rows = parseAdminUserCsv(await file.text());
      if (rows.length === 0) {
        setError("CSV file did not contain any users.");
        return;
      }
      for (const row of rows) {
        await createUser(row);
      }
      setMessage(`Imported ${rows.length} user${rows.length === 1 ? "" : "s"}. Copy temporary passwords before leaving this page.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to import users");
      await loadUsers();
    } finally {
      if (importInputRef.current) importInputRef.current.value = "";
    }
  };

  return (
    <div>
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-text text-lg font-medium">Users</h1>
        <div className="flex gap-2">
          <input
            ref={importInputRef}
            type="file"
            accept=".csv,text/csv"
            className="hidden"
            onChange={(event) => {
              const file = event.target.files?.[0];
              if (file) void importCsv(file);
            }}
          />
          <Button
            size="sm"
            variant="secondary"
            className={RESPONSIVE_TOUCH_ACTION_CLASS}
            disabled={localMode}
            onClick={() => importInputRef.current?.click()}
          >
            Import CSV
          </Button>
          <Button
            size="sm"
            className={RESPONSIVE_TOUCH_ACTION_CLASS}
            disabled={localMode}
            onClick={() => setInviteOpen(true)}
          >
            Create user
          </Button>
        </div>
      </div>
      {inviteOpen && (
        <DialogShell labelledBy="create-user-title" maxWidth="md" onClose={() => setInviteOpen(false)}>
          <form onSubmit={(event) => void submitInvite(event)} className="flex flex-col">
            <div className="border-border border-b px-4 py-3 sm:px-6">
              <h2 id="create-user-title" className="text-text text-heading-lg font-semibold">
                Create user
              </h2>
              <p className="text-text-muted mt-1 text-sm">
                Add a user and assign their initial access level.
              </p>
            </div>
            <div className="grid gap-4 p-4 sm:p-6">
              <label className="min-w-0">
                <span className="text-text mb-1 block text-sm font-medium">Email</span>
                <span className="text-text-muted mb-2 block text-xs">Used for sign-in and invitations.</span>
                <AdminTextInput
                  value={inviteEmail}
                  onChange={setInviteEmail}
                  type="email"
                  required
                  ariaLabel="Invite user email"
                />
              </label>
              <label className="min-w-0">
                <span className="text-text mb-1 block text-sm font-medium">Display name</span>
                <span className="text-text-muted mb-2 block text-xs">Shown in spaces and activity.</span>
                <AdminTextInput
                  value={inviteName}
                  onChange={setInviteName}
                  required
                  ariaLabel="Invite user display name"
                />
              </label>
              <label>
                <span className="text-text mb-1 block text-sm font-medium">Site role</span>
                <span className="text-text-muted mb-2 block text-xs">Admins can manage this Starlane instance.</span>
                <AdminFormSelect
                  value={inviteRole}
                  onChange={(value) => setInviteRole(value as "admin" | "member")}
                  ariaLabel="Invite role"
                >
                  <option value="member">Member</option>
                  <option value="admin">Administrator</option>
                </AdminFormSelect>
              </label>
            </div>
            <div className="border-border flex flex-wrap justify-end gap-2 border-t px-4 py-3 sm:px-6">
              <Button type="button" variant="secondary" onClick={() => setInviteOpen(false)}>
                Cancel
              </Button>
              <Button type="submit">Create user</Button>
            </div>
          </form>
        </DialogShell>
      )}
      {editingUser && (
        <DialogShell labelledBy="edit-user-title" maxWidth="md" onClose={() => setEditingUser(null)}>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void saveUserEditor();
            }}
            className="flex flex-col"
          >
            <div className="border-border border-b px-4 py-3 sm:px-6">
              <h2 id="edit-user-title" className="text-text text-heading-lg font-semibold">
                Edit user
              </h2>
              <p className="text-text-muted mt-1 text-sm">
                Change site access for {editingUser.display_name || editingUser.email}.
              </p>
            </div>
            <div className="grid gap-4 p-4 sm:p-6">
              <div className="rounded-input border-border bg-surface-raised border px-3 py-2">
                <p className="text-text text-sm font-medium">{editingUser.display_name || editingUser.email}</p>
                <p className="text-text-muted mt-0.5 text-xs">{editingUser.email}</p>
              </div>
              <label>
                <span className="text-text mb-1 block text-sm font-medium">Site role</span>
                <span className="text-text-muted mb-2 block text-xs">
                  Administrators can manage users, spaces, and site configuration.
                </span>
                <AdminFormSelect
                  value={editingUserRole}
                  onChange={(value) => setEditingUserRole(value as "admin" | "member")}
                  ariaLabel={`Site role for ${editingUser.email}`}
                >
                  <option value="member">Member</option>
                  <option value="admin">Administrator</option>
                </AdminFormSelect>
              </label>
            </div>
            <div className="border-border flex flex-wrap justify-end gap-2 border-t px-4 py-3 sm:px-6">
              <Button type="button" variant="secondary" onClick={() => setEditingUser(null)}>
                Cancel
              </Button>
              <Button type="submit" disabled={busyUserId === editingUser.id}>
                Save changes
              </Button>
            </div>
          </form>
        </DialogShell>
      )}
      {message && (
        <div className="border-border bg-surface-raised text-text-muted mb-3 rounded-input border px-3 py-2 text-xs">
          {message}
        </div>
      )}
      {temporaryPasswords.length > 0 && (
        <div className="border-border bg-surface mb-3 rounded-input border px-3 py-2 text-xs">
          <div className="text-text mb-2 font-medium">Temporary passwords</div>
          <div className="grid gap-1">
            {temporaryPasswords.map((item) => (
              <div key={`${item.email}-${item.password}`} className="text-text-muted flex flex-wrap gap-2">
                <span>{item.email}</span>
                <code className="rounded bg-surface-raised px-1.5 py-0.5 font-mono text-text">
                  {item.password}
                </code>
              </div>
            ))}
          </div>
        </div>
      )}
      {error && (
        <div className={ADMIN_COMPACT_ERROR_CLASS}>
          {error}
        </div>
      )}

      <div className="mb-3 grid gap-2 sm:grid-cols-2 lg:grid-cols-5">
        {[
          ["Total", displayedLifecycleCounts.total],
          ["Active", displayedLifecycleCounts.active],
          ["Inactive", displayedLifecycleCounts.inactive],
          ["Admins", displayedLifecycleCounts.admins],
          ["Member role", displayedLifecycleCounts.members],
        ].map(([label, value]) => (
          <div key={label} className="rounded-card border-border bg-surface border px-3 py-2">
            <p className="text-text-muted text-xs">{label}</p>
            <p className="text-text text-lg font-medium">{value}</p>
          </div>
        ))}
      </div>

      <div className="mb-3 grid gap-2 md:grid-cols-[minmax(0,1fr)_10rem_10rem]">
        <label className="min-w-0">
          <span className="text-text-muted mb-1 block text-xs">Search users</span>
          <AdminTextInput
            value={userQuery}
            onChange={setUserQuery}
            placeholder="Email, name, or role"
            ariaLabel="Search users"
          />
        </label>
        <label>
          <span className="text-text-muted mb-1 block text-xs">Role</span>
          <AdminFormSelect
            value={userRoleFilter}
            onChange={(value) => setUserRoleFilter(value as UserRoleFilter)}
            ariaLabel="User role filter"
          >
            <option value="all">All roles</option>
            <option value="admin">Admins</option>
            <option value="member">Members</option>
          </AdminFormSelect>
        </label>
        <label>
          <span className="text-text-muted mb-1 block text-xs">Status</span>
          <AdminFormSelect
            value={userStatusFilter}
            onChange={(value) => setUserStatusFilter(value as UserStatusFilter)}
            ariaLabel="User status filter"
          >
            <option value="all">All statuses</option>
            <option value="active">Active</option>
            <option value="inactive">Inactive</option>
          </AdminFormSelect>
        </label>
      </div>

      <div className="border-border bg-surface mb-3 flex flex-wrap items-center justify-between gap-2 rounded-card border px-3 py-2">
        <span className="text-text-muted text-xs">
          {usersSourceCurrent ? selectedUserIds.size : 0} selected
          {usersSourceCurrent && selectedUserIds.size > 0 ? ` · ${selectedVisibleUserIds.length} visible` : ""}
        </span>
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            variant="secondary"
            className={RESPONSIVE_TOUCH_ACTION_CLASS}
            disabled={localMode || !usersSourceCurrent || selectedUserIds.size === 0 || bulkLifecycleBusy}
            onClick={() => void runBulkLifecycle("activate")}
          >
            Activate selected
          </Button>
          <Button
            size="sm"
            variant="secondary"
            className={RESPONSIVE_TOUCH_ACTION_CLASS}
            disabled={localMode || !usersSourceCurrent || selectedUserIds.size === 0 || bulkLifecycleBusy}
            onClick={() => void runBulkLifecycle("deactivate")}
          >
            Deactivate selected
          </Button>
          <Button
            size="sm"
            variant="ghost"
            className={RESPONSIVE_TOUCH_ACTION_CLASS}
            disabled={selectedUserIds.size === 0 || bulkLifecycleBusy}
            onClick={() => setSelectedUserIds(new Set())}
          >
            Clear selection
          </Button>
        </div>
      </div>

      <p className="text-text-muted mb-2 text-xs">Guest account kind limits effective access even when retained site roles or memberships remain.</p>
      <Button variant="secondary" size="sm" disabled={localMode || usersLoading} onClick={()=>void loadUsers()}>Refresh users</Button>
      <div className="rounded-card border-border border">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-border text-text-muted border-b text-left text-xs">
              <th className="px-3 py-2 font-medium">
                <input
                  type="checkbox"
                  aria-label="Select all visible users"
                  checked={allVisibleUsersSelected}
                  disabled={visibleUsers.length === 0}
                  onChange={(event) => toggleVisibleUserSelection(event.target.checked)}
                />
              </th>
              <th className="px-3 py-2 font-medium">Email</th>
              <th className="px-3 py-2 font-medium">Name</th>
              <th className="px-3 py-2 font-medium">Site role</th>
              <th className="px-3 py-2 font-medium">Account kind</th>
              <th className="px-3 py-2 font-medium">Status</th>
              <th className="px-3 py-2 font-medium">Actions</th>
            </tr>
          </thead>
          <tbody>
            {usersLoading ? (
              <tr>
                <td colSpan={7} className="text-text-muted px-3 py-8 text-center">
                  Loading users...
                </td>
              </tr>
            ) : displayedUsers.length === 0 ? (
              <tr>
                <td colSpan={7} className="text-text-muted px-3 py-8 text-center">
                  {users.length === 0
                    ? "No users to display. Use the Invite or Import buttons to add users."
                    : "No users match the current filters."}
                </td>
              </tr>
            ) : (
              displayedUsers.map((u) => {
                const isSelf = u.id === user?.id;
                const busy = busyUserId === u.id;
                const acceptedOrigin=usersOrigin.current;
                return (
                  <tr key={u.id} className="border-border border-b last:border-0">
                    <td className="px-3 py-2">
                      <input
                        type="checkbox"
                        aria-label={`Select ${u.email}`}
                        checked={selectedUserIds.has(u.id)}
                        onChange={(event) => toggleUserSelection(u.id, event.target.checked)}
                      />
                    </td>
                    <td className="px-3 py-2">{u.email}</td>
                    <td className="px-3 py-2">{u.display_name}</td>
                    <td className="px-3 py-2">{u.role === "admin" ? "Administrator" : u.role === "member" ? "Member" : u.role === "viewer" ? "Viewer" : "Unavailable"}</td>
                    <td className="px-3 py-2">{accountKindLabel(u.account_kind)}</td>
                    <td className="px-3 py-2">
                      <span
                        className={`inline-block rounded px-1.5 py-0.5 text-xs ${
                          getAdminUserStatusBadgeClass(u.is_active)
                        }`}
                      >
                        {u.is_active ? "Active" : "Inactive"}
                      </span>
                    </td>
                    <td className="px-3 py-2">
                      <div className="flex flex-wrap gap-2">
                        {!localMode && !isSelf && <GuestAccountControls
                          target={u} actorId={user?.id} contextKey={usersContextKey}
                          assertCurrent={()=>{
                            if(!acceptedOrigin)throw new Error("Refresh users before reviewing this account.");
                            acceptedOrigin();
                            const latest=usersCurrent.current.find(item=>item.id===u.id);
                            if(!latest||latest.role!==u.role||latest.account_kind!==u.account_kind||latest.is_active!==u.is_active||latest.display_name!==u.display_name||latest.email!==u.email)throw new Error("Selected account changed. Refresh users.");
                          }}
                          onConfirmed={id=>{setUsers(old=>old.map(item=>item.id===id?{...item,account_kind:"guest"}:item));setMessage("Guest access confirmed. Share a published page separately to grant access.");}}
                        />}
                        <Button
                          size="sm"
                          variant="ghost"
                          className={RESPONSIVE_TOUCH_ACTION_CLASS}
                          disabled={localMode || busy || isSelf || u.account_kind === "guest"}
                          onClick={() => openUserEditor(u)}
                        >
                          <Pencil size={14} aria-hidden="true" />
                          Edit
                        </Button>
                        <Button
                          size="sm"
                          variant="secondary"
                          className={RESPONSIVE_TOUCH_ACTION_CLASS}
                          disabled={localMode || busy || isSelf}
                          onClick={() => void updateUser(u, { is_active: !u.is_active })}
                        >
                          {u.is_active ? "Deactivate" : "Reactivate"}
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          className={RESPONSIVE_TOUCH_ACTION_CLASS}
                          disabled={localMode}
                          onClick={() => void loadSessions(u)}
                        >
                          Sessions
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          className={RESPONSIVE_TOUCH_ACTION_CLASS}
                          disabled={localMode || busy}
                          onClick={() => void resetPassword(u)}
                        >
                          Reset password
                        </Button>
                      </div>
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      {sessionUser && (
        <div className="rounded-card border-border mt-4 border p-3">
          <div className="mb-3 flex items-center justify-between gap-3">
            <div>
              <p className="text-text text-sm font-medium">{sessionUser.email} sessions</p>
              <p className="text-text-muted text-xs">
                Revoke stale or suspicious refresh sessions for this user.
              </p>
            </div>
            <Button
              size="sm"
              variant="ghost"
              className={RESPONSIVE_TOUCH_ACTION_CLASS}
              onClick={closeSessions}
            >
              Close
            </Button>
          </div>
          {sessionRevokeTarget && (
            <ConfirmDialog
              labelledBy="admin-session-revoke-title"
              title="Revoke session"
              message={getAdminSessionRevokeConfirmationMessage(
                sessionUser.email,
                getAdminSessionDeviceLabel(sessionRevokeTarget),
              )}
              confirmLabel="Revoke session"
              destructive
              onCancel={() => setSessionRevokeTarget(null)}
              onConfirm={() => {
                void revokeSession(sessionRevokeTarget);
              }}
            />
          )}
          {sessionsLoading ? (
            <p className="text-text-muted text-sm">Loading sessions...</p>
          ) : sessions.length === 0 ? (
            <p className="text-text-muted text-sm">No active sessions.</p>
          ) : (
            <div className="space-y-2">
              {sessions.map((session) => (
                <div
                  key={session.id}
                  className="rounded-card border-border grid gap-2 border p-2 lg:grid-cols-[minmax(0,1fr)_10rem_10rem_auto]"
                >
                  <div className="min-w-0">
                    <p className="text-text truncate text-sm">
                      {getAdminSessionDeviceLabel(session)}
                    </p>
                    <p className="text-text-muted truncate text-xs">
                      {session.ip_address || "Unknown IP"} · {session.id}
                    </p>
                  </div>
                  <p className="text-text-muted text-xs">
                    Last seen: {new Date(session.last_seen_at).toLocaleString()}
                  </p>
                  <p className="text-text-muted text-xs">
                    Expires: {new Date(session.expires_at).toLocaleString()}
                  </p>
                  <Button
                    size="sm"
                    variant="danger"
                    className={RESPONSIVE_TOUCH_ACTION_CLASS}
                    onClick={() => {
                      setSessionRevokeTarget(session);
                    }}
                  >
                    Revoke
                  </Button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function SpacesSection({ localMode }: { localMode: boolean }) {
  const [spaces, setSpaces] = useState<SpaceItem[]>([]);
  const [spaceQuery, setSpaceQuery] = useState("");
  const [selectedSpaceId, setSelectedSpaceId] = useState(() => {
    if (typeof window === "undefined") return "";
    return new URLSearchParams(window.location.search).get("spaceId") ?? "";
  });
  const [spaceNameDraft, setSpaceNameDraft] = useState("");
  const [newSpaceName, setNewSpaceName] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const selectedSpace = spaces.find((space) => space.id === selectedSpaceId) || spaces[0] || null;
  const visibleSpaces = useMemo(() => {
    const query = spaceQuery.trim().toLowerCase();
    if (!query) return spaces;
    return spaces.filter((space) =>
      [space.name, space.slug, space.visibility].some((value) =>
        value.toLowerCase().includes(query),
      ),
    );
  }, [spaceQuery, spaces]);

  const loadLocalSpaces = async () => {
    const spaceList = await dataClient.listSpaces();
    setSpaces(sortAdminLocalSpaces(spaceList.map((space) => ({ ...space, visibility: "local" }))));
    setSelectedSpaceId((current) =>
      current && spaceList.some((space) => space.id === current) ? current : spaceList[0]?.id || "",
    );
  };

  useEffect(() => {
    if (localMode) {
      loadLocalSpaces().catch((err) =>
        setError(err instanceof Error ? err.message : "Failed to load local spaces"),
      );
      return;
    }

    api
      .fetch<SpaceItem[]>("/spaces")
      .then((spaceList) => {
        setSpaces(spaceList);
        setSelectedSpaceId((current) => current || spaceList[0]?.id || "");
      })
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load spaces"));
  }, [localMode]);

  useEffect(() => {
    setSpaceNameDraft(selectedSpace?.name ?? "");
  }, [selectedSpace?.id, selectedSpace?.name]);

  const updateVisibility = async (visibility: string, targetSpace = selectedSpace) => {
    if (!targetSpace) return;
    setMessage(null);
    setError(null);
    try {
      await api.fetch(`/spaces/${targetSpace.id}`, {
        method: "PUT",
        body: { visibility },
      });
      setSpaces((prev) =>
        prev.map((space) => (space.id === targetSpace.id ? { ...space, visibility } : space)),
      );
      setMessage("Space visibility updated.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to update visibility");
    }
  };

  const renameLocalSpace = async () => {
    if (!selectedSpace) return;
    const name = spaceNameDraft.trim();
    if (!name || name === selectedSpace.name) return;

    setMessage(null);
    setError(null);
    try {
      const updated = await dataClient.updateSpace(selectedSpace.id, { name });
      setSpaces((prev) =>
        sortAdminLocalSpaces(
          prev.map((space) =>
            space.id === updated.id ? { ...space, ...updated, visibility: "local" } : space,
          ),
        ),
      );
      setSpaceNameDraft(updated.name);
      setMessage("Space renamed.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to rename local space");
    }
  };

  const createLocalSpace = async () => {
    const name = newSpaceName.trim();
    if (!name) return;

    setMessage(null);
    setError(null);
    try {
      const created = await dataClient.createSpace(name);
      const next = { ...created, visibility: "local" };
      setSpaces((prev) => sortAdminLocalSpaces([...prev, next]));
      setSelectedSpaceId(created.id);
      setNewSpaceName("");
      setMessage("Space created.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create local space");
    }
  };

  return (
    <div>
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-text text-lg font-medium">Spaces</h1>
        <span className="text-text-muted text-xs">Visibility, users, and groups</span>
      </div>

      {localMode ? (
        <div className="space-y-4">
          <div className="grid gap-3 lg:grid-cols-[minmax(0,16rem)_minmax(0,1fr)]">
            <div className="rounded-card border-border border p-2">
              {spaces.length === 0 ? (
                <p className="px-2 py-2 text-sm text-text-muted">No local spaces found.</p>
              ) : (
                spaces.map((space) => (
                  <button
                    key={space.id}
                    type="button"
                    onClick={() => setSelectedSpaceId(space.id)}
                    className={`rounded-button block w-full min-h-11 px-2 py-1.5 text-left text-sm transition-colors duration-100 sm:min-h-0 ${
                      selectedSpace?.id === space.id
                        ? "bg-surface-raised text-text"
                        : "text-text-muted hover:bg-surface-raised hover:text-text"
                    }`}
                  >
                    <span className="block truncate">{space.name}</span>
                    <span className="text-text-muted block text-xs">{space.slug}</span>
                  </button>
                ))
              )}
            </div>

            <div className="space-y-4">
              <div className="rounded-card border-border border p-3">
                <div className="mb-3">
                  <p className="text-text text-sm font-medium">Local space settings</p>
                  <p className="text-text-muted mt-1 text-xs">
                    Create and rename standalone spaces stored in this local vault.
                  </p>
                </div>
                {selectedSpace && (
                  <div>
                    <label htmlFor="active-space-name" className="text-text-muted mb-1 block text-xs">
                      {ADMIN_ACTIVE_SPACE_NAME_LABEL}
                    </label>
                    <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
                      <AdminTextInput
                        id="active-space-name"
                        value={spaceNameDraft}
                        onChange={setSpaceNameDraft}
                        placeholder={ADMIN_ACTIVE_SPACE_NAME_LABEL}
                        ariaLabel={ADMIN_ACTIVE_SPACE_NAME_LABEL}
                      />
                      <Button
                        size="sm"
                        className={RESPONSIVE_TOUCH_ACTION_CLASS}
                        onClick={() => void renameLocalSpace()}
                        disabled={!spaceNameDraft.trim() || spaceNameDraft.trim() === selectedSpace.name}
                      >
                        {ADMIN_RENAME_ACTIVE_SPACE_LABEL}
                      </Button>
                    </div>
                  </div>
                )}
              </div>

              <div className="rounded-card border-border border p-3">
                <div className="mb-3">
                  <p className="text-text text-sm font-medium">Create Space</p>
                  <p className="text-text-muted mt-1 text-xs">
                    Add another local space to this vault and switch to it here.
                  </p>
                </div>
                <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
                  <AdminTextInput
                    value={newSpaceName}
                    onChange={setNewSpaceName}
                    placeholder="New space name"
                    ariaLabel="New space name"
                  />
                  <Button
                    size="sm"
                    className={RESPONSIVE_TOUCH_ACTION_CLASS}
                    onClick={() => void createLocalSpace()}
                    disabled={!newSpaceName.trim()}
                  >
                    Create Space
                  </Button>
                </div>
              </div>
            </div>
          </div>

          {message && (
            <div className="rounded-card border-border text-text-muted border px-3 py-2 text-sm">
              {message}
            </div>
          )}
          {error && (
            <div className={ADMIN_ERROR_CARD_CLASS}>
              {error}
            </div>
          )}
        </div>
      ) : (
        <div className="space-y-4">
          <section className="rounded-card border-border bg-surface overflow-hidden border">
            <header className="flex flex-wrap items-start justify-between gap-3 px-4 py-3">
              <div>
                <h2 className="text-text text-base font-semibold">Spaces</h2>
                <p className="text-text-muted mt-0.5 text-xs">
                  Visibility and access use the same settings workspace as Space admins.
                </p>
              </div>
              <span className="text-text-muted text-xs">
                {visibleSpaces.length} of {spaces.length}
              </span>
            </header>
            <div className="border-border border-t px-4 py-2.5">
              <AdminTextInput
                value={spaceQuery}
                onChange={setSpaceQuery}
                placeholder="Search spaces, slugs, or visibility"
                ariaLabel="Search spaces"
              />
            </div>
            <div className="overflow-x-auto">
              <table className="w-full min-w-[42rem] text-left text-sm">
                <thead>
                  <tr className="border-border bg-surface-raised text-text-muted border-y text-xs">
                    <th className="px-4 py-2.5 font-medium">Space</th>
                    <th className="px-4 py-2.5 font-medium">Visibility</th>
                    <th className="px-4 py-2.5 font-medium">Role</th>
                    <th className="px-4 py-2.5 text-right font-medium">Actions</th>
                  </tr>
                </thead>
                <tbody className="divide-border divide-y">
                  {visibleSpaces.length === 0 ? (
                    <tr>
                      <td colSpan={4} className="text-text-muted px-4 py-10 text-center">
                        {spaces.length === 0
                          ? "No spaces to display."
                          : "No spaces match this search."}
                      </td>
                    </tr>
                  ) : (
                    visibleSpaces.map((space) => (
                      <tr key={space.id} className="hover:bg-surface-raised/60">
                        <td className="px-4 py-3">
                          <span className="text-text block font-medium">{space.name}</span>
                          <span className="text-text-muted block text-xs">{space.slug}</span>
                        </td>
                        <td className="px-4 py-3">
                          <AdminInlineSelect
                            value={space.visibility}
                            onChange={(value) => {
                              setSelectedSpaceId(space.id);
                              void updateVisibility(value, space);
                            }}
                            ariaLabel={`Visibility for ${space.name}`}
                          >
                            <option value="private">Private</option>
                            <option value="internal">Everyone signed in</option>
                            <option value="public">Public web</option>
                          </AdminInlineSelect>
                        </td>
                        <td className="text-text-muted px-4 py-3 text-xs">
                          {roleLabel(space.member_role ?? "viewer")}
                        </td>
                        <td className="px-4 py-3">
                          <div className="flex justify-end">
                            <Button
                              type="button"
                              size="sm"
                              variant="secondary"
                              onClick={() => {
                                window.location.href = `/spaces/${encodeURIComponent(space.id)}/settings`;
                              }}
                            >
                              <Settings size={14} />
                              Manage access
                            </Button>
                          </div>
                        </td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>
          </section>
          {message && (
            <div className="rounded-card border-border text-text-muted border px-3 py-2 text-sm">
              {message}
            </div>
          )}
          {error && (
            <div className={ADMIN_ERROR_CARD_CLASS}>
              {error}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function RolesSection({ localMode }: { localMode: boolean }) {
  const [roles, setRoles] = useState<GlobalRoleItem[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (localMode) return;
    api.fetch<GlobalRoleItem[]>("/admin/roles")
      .then(setRoles)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load roles"));
  }, [localMode]);

  return (
    <div>
      <div className="mb-4 flex items-center justify-between">
        <div>
          <h1 className="text-text text-lg font-medium">Roles</h1>
          <p className="text-text-muted mt-1 text-sm">Reusable roles can be assigned to groups and apply across spaces.</p>
        </div>
      </div>
      {localMode ? (
        <div className="rounded-card border-border text-text-muted border p-3 text-sm">Roles are unavailable in local mode.</div>
      ) : error ? (
        <div className={ADMIN_ERROR_CARD_CLASS}>{error}</div>
      ) : (
        <div className="grid gap-3 md:grid-cols-2">
          {roles.map((role) => (
            <div key={role.id} className="rounded-card border-border bg-surface border p-4">
              <div className="flex items-start justify-between gap-3">
                <div>
                  <h2 className="text-text font-medium">{role.name}</h2>
                  <p className="text-text-muted mt-1 text-sm">{role.description}</p>
                </div>
                {role.built_in && <Badge variant="secondary">Built-in</Badge>}
              </div>
              <div className="mt-3 flex flex-wrap gap-1.5">
                {role.permissions.map((permission) => (
                  <Badge key={permission} variant="outline">{permission.replace(/_/g, " ")}</Badge>
                ))}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function GroupsSection({ localMode }: { localMode: boolean }) {
  const [groups, setGroups] = useState<GroupItem[]>([]);
  const [selectedGroupId, setSelectedGroupId] = useState("");
  const [members, setMembers] = useState<SpaceMemberItem[]>([]);
  const [spaceGrants, setSpaceGrants] = useState<GroupSpaceGrantItem[]>([]);
  const [globalRoles, setGlobalRoles] = useState<GlobalRoleItem[]>([]);
  const [groupRoles, setGroupRoles] = useState<GlobalRoleItem[]>([]);
  const [assignRoleOpen, setAssignRoleOpen] = useState(false);
  const [users, setUsers] = useState<UserItem[]>([]);
  const [newGroupName, setNewGroupName] = useState("");
  const [newGroupDescription, setNewGroupDescription] = useState("");
  const [groupNameDraft, setGroupNameDraft] = useState("");
  const [groupDescriptionDraft, setGroupDescriptionDraft] = useState("");
  const [userId, setUserId] = useState("");
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [createGroupOpen, setCreateGroupOpen] = useState(false);
  const [addMemberOpen, setAddMemberOpen] = useState(false);
  const [memberQuery, setMemberQuery] = useState("");
  const [memberToRemove, setMemberToRemove] = useState<SpaceMemberItem | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const selectedGroup = groups.find((group) => group.id === selectedGroupId) || groups[0] || null;

  const reloadGroups = async () => {
    const [groupList, userList] = await Promise.all([
      api.fetch<GroupItem[]>("/admin/groups"),
      api.fetch<UserItem[]>("/admin/users"),
    ]);
    setGroups(groupList);
    setUsers(userList);
    setSelectedGroupId((current) => current || groupList[0]?.id || "");
  };

  useEffect(() => {
    if (localMode) return;
    reloadGroups().catch((err) =>
      setError(err instanceof Error ? err.message : "Failed to load groups"),
    );
  }, [localMode]);

  useEffect(() => {
    if (localMode || !selectedGroup?.id) {
      setMembers([]);
      setSpaceGrants([]);
      setGroupRoles([]);
      return;
    }
    setGroupNameDraft(selectedGroup.name);
    setGroupDescriptionDraft(selectedGroup.description || "");
    Promise.all([
      api.fetch<SpaceMemberItem[]>(`/admin/groups/${selectedGroup.id}/members`),
      api.fetch<GroupSpaceGrantItem[]>(`/admin/groups/${selectedGroup.id}/spaces`),
      api.fetch<GlobalRoleItem[]>(`/admin/groups/${selectedGroup.id}/roles`),
    ])
      .then(([memberList, grantList, roleList]) => {
        setMembers(memberList);
        setSpaceGrants(grantList);
        setGroupRoles(roleList);
      })
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load members"));
  }, [localMode, selectedGroup?.id, selectedGroup?.name, selectedGroup?.description]);

  useEffect(() => {
    if (localMode) return;
    api.fetch<GlobalRoleItem[]>("/admin/roles")
      .then(setGlobalRoles)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load roles"));
  }, [localMode]);

  const assignRole = async (roleId: string) => {
    if (!selectedGroup) return;
    setSaving(true);
    setError(null);
    try {
      await api.fetch(`/admin/groups/${selectedGroup.id}/roles`, { method: "POST", body: { role_id: roleId } });
      setGroupRoles(await api.fetch<GlobalRoleItem[]>(`/admin/groups/${selectedGroup.id}/roles`));
      setAssignRoleOpen(false);
      setMessage("Role assigned to the group.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to assign role");
    } finally {
      setSaving(false);
    }
  };

  const removeRole = async (role: GlobalRoleItem) => {
    if (!selectedGroup) return;
    setSaving(true);
    setError(null);
    try {
      await api.fetch(`/admin/groups/${selectedGroup.id}/roles/${role.id}`, { method: "DELETE" });
      setGroupRoles((current) => current.filter((item) => item.id !== role.id));
      setMessage(`${role.name} removed from the group.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to remove role");
    } finally {
      setSaving(false);
    }
  };

  const createGroup = async () => {
    if (!newGroupName.trim()) return;
    setSaving(true);
    setError(null);
    try {
      const created = await api.fetch<GroupItem>("/admin/groups", {
        method: "POST",
        body: {
          name: newGroupName.trim(),
          description: newGroupDescription.trim() || null,
        },
      });
      setNewGroupName("");
      setNewGroupDescription("");
      setCreateGroupOpen(false);
      await reloadGroups();
      setSelectedGroupId(created.id);
      setMessage(`Created group “${created.name}”.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to create group");
    } finally {
      setSaving(false);
    }
  };

  const addMember = async (memberId = userId) => {
    if (!selectedGroup || !memberId) return;
    setSaving(true);
    setError(null);
    try {
      await api.fetch(`/admin/groups/${selectedGroup.id}/members`, {
        method: "POST",
        body: { user_id: memberId },
      });
      setUserId("");
      const refreshed = await api.fetch<SpaceMemberItem[]>(
        `/admin/groups/${selectedGroup.id}/members`,
      );
      setMembers(refreshed);
      await reloadGroups();
      setAddMemberOpen(false);
      setMemberQuery("");
      setMessage("Member added to the group.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to add group member");
    } finally {
      setSaving(false);
    }
  };

  const confirmRemoveMember = async () => {
    if (!selectedGroup || !memberToRemove) return;
    setSaving(true);
    setError(null);
    try {
      await api.fetch(`/admin/groups/${selectedGroup.id}/members/${memberToRemove.user_id}`, { method: "DELETE" });
      setMembers((prev) => prev.filter((member) => member.user_id !== memberToRemove.user_id));
      await reloadGroups();
      setMessage(`${memberToRemove.display_name || memberToRemove.email} removed from the group.`);
      setMemberToRemove(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to remove group member");
    } finally {
      setSaving(false);
    }
  };

  const availableMembers = users.filter((item) =>
    !members.some((member) => member.user_id === item.id) &&
    `${item.display_name} ${item.email}`.toLowerCase().includes(memberQuery.trim().toLowerCase()),
  );

  const saveGroup = async () => {
    if (
      !selectedGroup ||
      selectedGroup.scim_managed ||
      selectedGroup.managed_space_id ||
      !groupNameDraft.trim()
    )
      return;
    setSaving(true);
    setError(null);
    try {
      await api.fetch(`/admin/groups/${selectedGroup.id}`, {
        method: "PUT",
        body: {
          name: groupNameDraft.trim(),
          description: groupDescriptionDraft.trim() || null,
        },
      });
      await reloadGroups();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to update group");
    } finally {
      setSaving(false);
    }
  };

  const deleteGroup = async () => {
    if (!selectedGroup || selectedGroup.scim_managed || selectedGroup.managed_space_id) return;
    setSaving(true);
    setError(null);
    try {
      await api.fetch(`/admin/groups/${selectedGroup.id}`, { method: "DELETE" });
      setDeleteConfirmOpen(false);
      setSelectedGroupId("");
      setMembers([]);
      setSpaceGrants([]);
      await reloadGroups();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to delete group");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div>
      {deleteConfirmOpen && selectedGroup && (
        <ConfirmDialog
          labelledBy="admin-delete-group-title"
          title="Delete group"
          message={getAdminGroupDeleteConfirmationMessage(selectedGroup)}
          confirmLabel={saving ? "Deleting..." : "Delete group"}
          destructive
          onCancel={() => setDeleteConfirmOpen(false)}
          onConfirm={() => void deleteGroup()}
        />
      )}
      {memberToRemove && (
        <ConfirmDialog
          labelledBy="admin-remove-group-member-title"
          title="Remove group member"
          message={`Remove ${memberToRemove.display_name || memberToRemove.email} from ${selectedGroup?.name ?? "this group"}?`}
          confirmLabel={saving ? "Removing..." : "Remove member"}
          destructive
          onCancel={() => !saving && setMemberToRemove(null)}
          onConfirm={() => void confirmRemoveMember()}
        />
      )}
      {createGroupOpen && (
        <DialogShell labelledBy="admin-create-group-title" maxWidth="md" onClose={() => !saving && setCreateGroupOpen(false)}>
          <form onSubmit={(event) => { event.preventDefault(); void createGroup(); }} className="flex flex-col">
            <div className="border-border border-b px-4 py-3 sm:px-6">
              <h2 id="admin-create-group-title" className="text-text text-heading-lg font-semibold">Create group</h2>
              <p className="text-text-muted mt-1 text-sm">Create a reusable permission group for spaces and users.</p>
            </div>
            <div className="grid gap-4 p-4 sm:p-6">
              <label>
                <span className="text-text mb-1 block text-sm font-medium">Group name</span>
                <AdminTextInput value={newGroupName} onChange={setNewGroupName} placeholder="e.g. Product team" ariaLabel="Group name" required />
              </label>
              <label>
                <span className="text-text mb-1 block text-sm font-medium">Description</span>
                <Textarea value={newGroupDescription} onChange={(event) => setNewGroupDescription(event.target.value)} placeholder="What access does this group represent?" aria-label="Group description" className="min-h-20 text-sm" />
              </label>
            </div>
            <div className="border-border flex justify-end gap-2 border-t px-4 py-3 sm:px-6">
              <Button type="button" variant="secondary" onClick={() => setCreateGroupOpen(false)}>Cancel</Button>
              <Button type="submit" disabled={saving || !newGroupName.trim()}>{saving ? "Creating..." : "Create group"}</Button>
            </div>
          </form>
        </DialogShell>
      )}
      {addMemberOpen && selectedGroup && (
        <DialogShell labelledBy="admin-add-group-member-title" maxWidth="md" onClose={() => !saving && setAddMemberOpen(false)}>
          <div className="flex flex-col">
            <div className="border-border border-b px-4 py-3 sm:px-6">
              <h2 id="admin-add-group-member-title" className="text-text text-heading-lg font-semibold">Add member</h2>
              <p className="text-text-muted mt-1 text-sm">Choose a user to add to {selectedGroup.name}.</p>
            </div>
            <div className="grid gap-3 p-4 sm:p-6">
              <Input value={memberQuery} onChange={(event) => setMemberQuery(event.target.value)} placeholder="Search by name or email" aria-label="Search users to add" autoFocus />
              <div className="border-border max-h-64 overflow-y-auto rounded-input border">
                {availableMembers.length === 0 ? (
                  <p className="text-text-muted px-3 py-4 text-sm">No available users match this search.</p>
                ) : availableMembers.map((item) => (
                  <button key={item.id} type="button" className="border-border hover:bg-surface-raised flex w-full items-center justify-between border-b px-3 py-2 text-left last:border-0" onClick={() => { setAddMemberOpen(false); void addMember(item.id); }}>
                    <span className="min-w-0"><span className="text-text block truncate text-sm">{item.display_name || item.email}</span><span className="text-text-muted block truncate text-xs">{item.email}</span></span>
                    <span className="text-primary text-xs font-medium">Add</span>
                  </button>
                ))}
              </div>
            </div>
            <div className="border-border flex justify-end border-t px-4 py-3 sm:px-6"><Button type="button" variant="secondary" onClick={() => setAddMemberOpen(false)}>Cancel</Button></div>
          </div>
        </DialogShell>
      )}
      {assignRoleOpen && selectedGroup && (
        <DialogShell labelledBy="admin-assign-group-role-title" maxWidth="md" onClose={() => !saving && setAssignRoleOpen(false)}>
          <div className="flex flex-col">
            <div className="border-border border-b px-4 py-3 sm:px-6">
              <h2 id="admin-assign-group-role-title" className="text-text text-heading-lg font-semibold">Assign role</h2>
              <p className="text-text-muted mt-1 text-sm">Choose a reusable role for {selectedGroup.name}.</p>
            </div>
            <div className="grid gap-2 p-4 sm:p-6">
              {globalRoles.filter((role) => !groupRoles.some((current) => current.id === role.id)).map((role) => (
                <button key={role.id} type="button" className="border-border hover:bg-surface-raised flex items-center justify-between rounded-input border px-3 py-3 text-left" onClick={() => void assignRole(role.id)}>
                  <span><span className="text-text block text-sm font-medium">{role.name}</span><span className="text-text-muted block text-xs">{role.description}</span></span>
                  <span className="text-primary text-xs font-medium">Assign</span>
                </button>
              ))}
            </div>
            <div className="border-border flex justify-end border-t px-4 py-3 sm:px-6"><Button type="button" variant="secondary" onClick={() => setAssignRoleOpen(false)}>Cancel</Button></div>
          </div>
        </DialogShell>
      )}
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-text text-lg font-medium">Groups</h1>
        <Button size="sm" className={RESPONSIVE_TOUCH_ACTION_CLASS} disabled={localMode} onClick={() => setCreateGroupOpen(true)}>Create group</Button>
      </div>
      {message && <div className="border-success/30 bg-success/10 text-success mb-3 rounded-input border px-3 py-2 text-sm">{message}</div>}
      {error && <div className={ADMIN_ERROR_CARD_CLASS}>{error}</div>}

      {localMode ? (
        <div className="rounded-card border-border text-text-muted border p-3 text-sm">
          Groups are unavailable in local mode.
        </div>
      ) : (
        <div className="grid gap-4 lg:grid-cols-[minmax(0,18rem)_minmax(0,1fr)]">
          <div className="space-y-3">
            <div className="rounded-card border-border border p-2">
              {groups.map((group) => (
                <button
                  key={group.id}
                  type="button"
                  onClick={() => setSelectedGroupId(group.id)}
                  className={`rounded-button block w-full min-h-11 px-2 py-1.5 text-left text-sm transition-colors duration-100 sm:min-h-0 ${
                    selectedGroup?.id === group.id
                      ? "bg-surface-raised text-text"
                      : "text-text-muted hover:bg-surface-raised hover:text-text"
                  }`}
                >
                  <span className="block truncate">{group.name}</span>
                  <span className="text-text-muted block text-xs">
                    {group.member_count} members · {group.space_grant_count} spaces
                  </span>
                </button>
              ))}
            </div>
          </div>

          <div className="rounded-card border-border border p-3">
            {selectedGroup ? (
              <>
                <div className="mb-3 flex items-center justify-between gap-3">
                  <div>
                    <p className="text-text text-sm font-medium">{selectedGroup.name}</p>
                    {selectedGroup.description && (
                      <p className="text-text-muted text-xs">{selectedGroup.description}</p>
                    )}
                  </div>
                  {selectedGroup.scim_managed && (
                    <span className="border-warning/40 bg-warning/10 text-warning rounded-button border px-2 py-1 text-xs">
                      Managed by SCIM
                    </span>
                  )}
                  {selectedGroup.managed_space_id && (
                    <span className="border-primary/30 bg-primary/10 text-primary rounded-button border px-2 py-1 text-xs">
                      Dedicated space group
                    </span>
                  )}
                </div>
                {selectedGroup.scim_managed || selectedGroup.managed_space_id ? (
                  <div className="rounded-input border-border bg-surface-raised mb-3 border p-3 text-sm">
                    <p className="text-text font-medium">
                      {selectedGroup.scim_managed
                        ? "Managed by your identity provider"
                        : "Managed by its space"}
                    </p>
                    <p className="text-text-muted mt-1 text-xs">
                      {selectedGroup.scim_managed
                        ? "Rename, membership, and deletion are locked here so SCIM remains authoritative."
                        : "Its identity and space assignment stay fixed. Global admins and that space's admins can still manage membership."}
                    </p>
                  </div>
                ) : (
                  <div className="rounded-input border-border mb-3 border p-3">
                    <div className="mb-2 flex items-center justify-between gap-3">
                      <h2 className="text-text text-sm font-medium">Group details</h2>
                      <Button
                        size="sm"
                        variant="secondary"
                        className={RESPONSIVE_TOUCH_ACTION_CLASS}
                        disabled={saving || !groupNameDraft.trim()}
                        onClick={() => void saveGroup()}
                      >
                        {saving ? "Saving..." : "Save changes"}
                      </Button>
                    </div>
                    <div className="space-y-2">
                      <AdminTextInput
                        value={groupNameDraft}
                        onChange={setGroupNameDraft}
                        placeholder="Group name"
                        ariaLabel="Edit group name"
                      />
                      <Textarea
                        value={groupDescriptionDraft}
                        onChange={(event) => setGroupDescriptionDraft(event.target.value)}
                        placeholder="Description"
                        aria-label="Edit group description"
                        className="min-h-20 text-sm"
                      />
                    </div>
                  </div>
                )}
                <div className="rounded-input border-border mb-3 border">
                  <div className="border-border flex items-center justify-between border-b px-3 py-2">
                    <div>
                      <h2 className="text-text text-sm font-medium">Space access</h2>
                      <p className="text-text-muted text-xs">Spaces this group can open.</p>
                    </div>
                    <span className="text-text-muted text-xs">{spaceGrants.length}</span>
                  </div>
                  {spaceGrants.length ? (
                    spaceGrants.map((grant) => (
                      <div
                        key={grant.space_id}
                        className="border-border flex items-center justify-between gap-3 border-b px-3 py-2 last:border-0"
                      >
                        <div className="min-w-0">
                          <p className="text-text truncate text-sm">{grant.name}</p>
                          <p className="text-text-muted truncate text-xs">/{grant.slug}</p>
                        </div>
                        <span className="text-text-muted text-xs capitalize">{grant.role}</span>
                      </div>
                    ))
                  ) : (
                    <p className="text-text-muted px-3 py-3 text-sm">No space access grants.</p>
                  )}
                </div>
                <div className="rounded-input border-border mb-3 border">
                  <div className="border-border flex items-center justify-between border-b px-3 py-2">
                    <div><h2 className="text-text text-sm font-medium">Global roles</h2><p className="text-text-muted text-xs">Reusable access applied wherever this group is assigned.</p></div>
                    <Button size="sm" variant="secondary" disabled={selectedGroup.scim_managed} onClick={() => setAssignRoleOpen(true)}>Assign role</Button>
                  </div>
                  {groupRoles.length ? groupRoles.map((role) => (
                    <div key={role.id} className="border-border flex items-center justify-between gap-3 border-b px-3 py-2 last:border-0">
                      <div><p className="text-text text-sm">{role.name}</p><p className="text-text-muted text-xs">{role.description}</p></div>
                      <button type="button" disabled={saving || selectedGroup.managed_space_id != null} onClick={() => void removeRole(role)} className="text-danger text-xs">Remove</button>
                    </div>
                  )) : <p className="text-text-muted px-3 py-3 text-sm">No global roles assigned.</p>}
                </div>
                <div className="mb-3 flex items-center justify-between gap-3">
                  <h2 className="text-text text-sm font-medium">Members</h2>
                  <div className="flex gap-2">
                    <Button
                      size="sm"
                      className={RESPONSIVE_TOUCH_ACTION_CLASS}
                      disabled={selectedGroup.scim_managed}
                      onClick={() => setAddMemberOpen(true)}
                    >
                      Add member
                    </Button>
                  </div>
                </div>
                <div className="rounded-card border-border border">
                  {members.map((member) => (
                    <div
                      key={member.user_id}
                      className="border-border flex items-center gap-2 border-b px-3 py-2 last:border-0"
                    >
                      <div className="min-w-0 flex-1">
                        <p className="text-text truncate text-sm">{member.display_name}</p>
                        <p className="text-text-muted truncate text-xs">{member.email}</p>
                      </div>
                      <button
                        type="button"
                        disabled={selectedGroup.scim_managed}
                        onClick={() => {
                          setMemberToRemove(member);
                        }}
                        className={`${ADMIN_DANGER_ICON_BUTTON_CLASS} ${RESPONSIVE_TOUCH_ICON_ACTION_CLASS}`}
                        aria-label={`Remove ${member.email}`}
                      >
                        <UserRoundX size={14} />
                      </button>
                    </div>
                  ))}
                </div>
                {!selectedGroup.scim_managed && !selectedGroup.managed_space_id && (
                  <div className="border-danger/30 bg-danger/10 mt-3 flex items-center justify-between gap-3 rounded-input border p-3">
                    <div>
                      <p className="text-text text-sm font-medium">Delete group</p>
                      <p className="text-text-muted text-xs">Users and spaces stay intact.</p>
                    </div>
                    <Button
                      size="sm"
                      variant="danger"
                      className={RESPONSIVE_TOUCH_ACTION_CLASS}
                      disabled={saving}
                      onClick={() => setDeleteConfirmOpen(true)}
                    >
                      Delete group
                    </Button>
                  </div>
                )}
              </>
            ) : (
              <p className="text-text-muted text-sm">Create a group to add members.</p>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

const AUTH_CONFIG_KEYS = new Set([
  "allow_public_signups",
  "require_2fa",
  "oidc_enabled",
  "oidc_display_name",
  "oidc_issuer_url",
  "oidc_client_id",
  "oidc_client_secret",
  "oidc_scopes",
  "oidc_user_claim",
  "oidc_role_claim",
  "oidc_role_admin_value",
  "oidc_auto_provision",
]);

const OIDC_CONFIG_KEYS = [
  "oidc_enabled",
  "oidc_display_name",
  "oidc_issuer_url",
  "oidc_client_id",
  "oidc_client_secret",
  "oidc_scopes",
  "oidc_user_claim",
  "oidc_role_claim",
  "oidc_role_admin_value",
  "oidc_auto_provision",
] as const;

function configValue(entry: ConfigEntry | undefined): string {
  return entry?.value || entry?.effective_value || "";
}

function configBoolean(entry: ConfigEntry | undefined): boolean {
  return configValue(entry).toLowerCase() === "true";
}

function AuthenticationToggle({
  checked,
  disabled,
  label,
  onChange,
}: {
  checked: boolean;
  disabled?: boolean;
  label: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative h-6 w-11 shrink-0 rounded-full border transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-50 ${
        checked ? "border-primary bg-primary" : "border-border bg-surface-raised"
      }`}
    >
      <span
        className={`absolute left-0 top-0.5 size-4 rounded-full bg-white transition-transform duration-100 ${
          checked ? "translate-x-5" : "translate-x-0.5"
        }`}
      />
    </button>
  );
}

function AuthenticationSection({ localMode }: { localMode: boolean }) {
  const [entries, setEntries] = useState<ConfigEntry[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [providerDialogOpen, setProviderDialogOpen] = useState(false);
  const [savingProvider, setSavingProvider] = useState(false);

  const loadConfig = useCallback(async () => {
    if (localMode) {
      setEntries([]);
      return;
    }
    const allEntries = await api.fetch<ConfigEntry[]>("/admin/config");
    setEntries(allEntries.filter((entry) => AUTH_CONFIG_KEYS.has(entry.key)));
  }, [localMode]);

  useEffect(() => {
    void loadConfig().catch((err) => {
      setError(err instanceof Error ? err.message : "Unable to load authentication settings");
    });
  }, [loadConfig]);

  const saveConfig = async (entry: ConfigEntry, value = entry.value) => {
    if (entry.locked) return;
    setMessage(null);
    setError(null);
    try {
      await api.fetch("/admin/config", {
        method: "POST",
        body: { key: entry.key, value },
      });
      setMessage(`${entry.label} saved.`);
      await loadConfig();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to save authentication setting");
    }
  };

  const updateEntry = (key: string, value: string) => {
    setEntries((current) =>
      current.map((entry) => (entry.key === key ? { ...entry, value } : entry)),
    );
  };

  const entryByKey = useCallback(
    (key: string) => entries.find((entry) => entry.key === key),
    [entries],
  );
  const oidcEnabled = configBoolean(entryByKey("oidc_enabled"));
  const oidcName = configValue(entryByKey("oidc_display_name")) || "Single sign-on";
  const oidcIssuer = configValue(entryByKey("oidc_issuer_url"));
  const oidcConfigured = Boolean(oidcIssuer && configValue(entryByKey("oidc_client_id")));
  const oidcLocked = OIDC_CONFIG_KEYS.some((key) => entryByKey(key)?.locked);

  const saveProvider = async () => {
    setSavingProvider(true);
    setMessage(null);
    setError(null);
    try {
      for (const key of OIDC_CONFIG_KEYS) {
        const entry = entryByKey(key);
        if (!entry || entry.locked) continue;
        if (entry.secret && !entry.value) continue;
        await api.fetch("/admin/config", {
          method: "POST",
          body: { key: entry.key, value: entry.value },
        });
      }
      setProviderDialogOpen(false);
      setMessage("Single sign-on provider saved.");
      await loadConfig();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to save the SSO provider");
    } finally {
      setSavingProvider(false);
    }
  };

  const policyEntries = entries.filter((entry) =>
    ["allow_public_signups", "require_2fa"].includes(entry.key),
  );

  return (
    <div>
      <div className="mb-4 flex items-center justify-between gap-3">
        <div>
          <h1 className="text-text text-lg font-medium">Authentication</h1>
          <p className="text-text-muted text-xs">
            Configure login policy and your organization&apos;s single sign-on provider.
          </p>
        </div>
        <Button
          size="sm"
          variant="secondary"
          disabled={localMode}
          onClick={() => {
            void loadConfig();
          }}
        >
          <RefreshCw size={14} className="mr-1" />
          Refresh
        </Button>
      </div>

      <div className="space-y-3">
        {localMode && (
          <div className="rounded-card border-border text-text-muted border p-3 text-sm">
            Server authentication settings are unavailable in local standalone mode.
          </div>
        )}

        <section className="rounded-card border-border bg-surface border shadow-card">
          <div className="border-border border-b px-4 py-3">
            <div className="flex items-center gap-2">
              <LogIn size={16} className="text-text-muted" />
              <p className="text-text text-sm font-medium">Login policy</p>
            </div>
            <p className="text-text-muted mt-1 text-xs">
              Control account creation and the minimum sign-in protection for this server.
            </p>
          </div>
          <div className="divide-border divide-y">
            {policyEntries.length === 0 ? (
              <p className="text-text-muted text-sm">
                {localMode ? "No server settings available." : "Loading authentication settings..."}
              </p>
            ) : (
              policyEntries.map((entry) => (
                <div
                  key={entry.key}
                  className="flex min-h-16 items-center justify-between gap-4 px-4 py-3"
                >
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <p className="text-text text-sm font-medium">{entry.label}</p>
                      {entry.locked && (
                        <span className="border-border bg-surface-raised text-text-muted inline-flex items-center gap-1 rounded-badge border px-1.5 py-0.5 text-[11px]">
                          <LockKeyhole size={11} />
                          Environment
                        </span>
                      )}
                    </div>
                    <p className="text-text-muted mt-0.5 text-xs">
                      {entry.key === "require_2fa"
                        ? "Require an authenticator code after password sign-in."
                        : "Allow visitors to create their own server account."}
                    </p>
                  </div>
                  <AuthenticationToggle
                    checked={configBoolean(entry)}
                    disabled={localMode || entry.locked}
                    label={entry.label}
                    onChange={(checked) => void saveConfig(entry, String(checked))}
                  />
                </div>
              ))
            )}
          </div>
        </section>

        <section className="rounded-card border-border bg-surface border shadow-card">
          <div className="border-border flex flex-wrap items-start justify-between gap-3 border-b px-4 py-3">
            <div>
              <div className="flex items-center gap-2">
                <Building2 size={16} className="text-text-muted" />
                <p className="text-text text-sm font-medium">Single sign-on</p>
              </div>
              <p className="text-text-muted mt-1 text-xs">
                Connect one OpenID Connect provider for organization-managed sign-in.
              </p>
            </div>
            <Button
              type="button"
              size="sm"
              variant="secondary"
              disabled={localMode || entries.length === 0}
              onClick={() => setProviderDialogOpen(true)}
            >
              {oidcConfigured ? <Pencil size={14} /> : <Plus size={14} />}
              {oidcConfigured ? "Edit provider" : "Configure provider"}
            </Button>
          </div>
          <div className="grid gap-4 p-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
            <div className="flex min-w-0 items-start gap-3">
              <span className="border-border bg-surface-raised text-text flex size-10 shrink-0 items-center justify-center rounded-card border">
                <Building2 size={18} />
              </span>
              <div className="min-w-0">
                <div className="flex flex-wrap items-center gap-2">
                  <p className="text-text truncate text-sm font-medium">{oidcName}</p>
                  <span
                    className={`rounded-badge border px-2 py-0.5 text-xs ${
                      oidcEnabled
                        ? "border-success/30 bg-success/10 text-success"
                        : "border-border bg-surface-raised text-text-muted"
                    }`}
                  >
                    {oidcEnabled ? "Enabled" : "Disabled"}
                  </span>
                  {oidcLocked && (
                    <span className="border-border bg-surface-raised text-text-muted inline-flex items-center gap-1 rounded-badge border px-2 py-0.5 text-xs">
                      <LockKeyhole size={11} />
                      Partly environment managed
                    </span>
                  )}
                </div>
                <p className="text-text-muted mt-1 truncate text-xs">
                  {oidcConfigured ? oidcIssuer : "No provider configured yet."}
                </p>
              </div>
            </div>
            <span className="text-text-muted text-xs">OpenID Connect</span>
          </div>
        </section>

        {message && (
          <div className={ADMIN_SUCCESS_CARD_CLASS}>
            {message}
          </div>
        )}
        {error && (
          <div className={ADMIN_ERROR_CARD_CLASS}>
            {error}
          </div>
        )}
      </div>

      {providerDialogOpen && (
        <DialogShell
          labelledBy="oidc-provider-dialog-title"
          maxWidth="lg"
          onClose={() => setProviderDialogOpen(false)}
        >
          <div className="flex max-h-[min(46rem,calc(100dvh-2rem))] flex-col">
            <header className="border-border flex items-start justify-between gap-4 border-b px-5 py-4">
              <div>
                <h2 id="oidc-provider-dialog-title" className="text-text text-base font-semibold">
                  Configure single sign-on
                </h2>
                <p className="text-text-muted mt-1 text-sm">
                  Connect an OpenID Connect identity provider to this Starlane server.
                </p>
              </div>
              {oidcLocked && (
                <span className="border-border bg-surface-raised text-text-muted inline-flex shrink-0 items-center gap-1 rounded-badge border px-2 py-1 text-xs">
                  <LockKeyhole size={12} />
                  Environment values locked
                </span>
              )}
            </header>

            <div className="min-h-0 space-y-5 overflow-y-auto px-5 py-4">
              <div className="rounded-card border-border bg-surface-raised/40 flex items-center justify-between gap-4 border p-3">
                <div>
                  <p className="text-text text-sm font-medium">Enable provider</p>
                  <p className="text-text-muted mt-0.5 text-xs">
                    Show this provider as a sign-in option.
                  </p>
                </div>
                <AuthenticationToggle
                  checked={oidcEnabled}
                  disabled={entryByKey("oidc_enabled")?.locked}
                  label="Enable OIDC provider"
                  onChange={(checked) => updateEntry("oidc_enabled", String(checked))}
                />
              </div>

              <div className="grid gap-4 sm:grid-cols-2">
                <label className="grid gap-1.5">
                  <span className="text-text text-sm font-medium">Provider name</span>
                  <Input
                    value={entryByKey("oidc_display_name")?.value ?? ""}
                    onChange={(event) => updateEntry("oidc_display_name", event.target.value)}
                    placeholder="Company SSO"
                    disabled={entryByKey("oidc_display_name")?.locked}
                  />
                </label>
                <label className="grid gap-1.5">
                  <span className="text-text text-sm font-medium">Issuer URL</span>
                  <Input
                    value={entryByKey("oidc_issuer_url")?.value ?? ""}
                    onChange={(event) => updateEntry("oidc_issuer_url", event.target.value)}
                    placeholder="https://identity.example.com"
                    inputMode="url"
                    disabled={entryByKey("oidc_issuer_url")?.locked}
                  />
                </label>
                <label className="grid gap-1.5">
                  <span className="text-text text-sm font-medium">Client ID</span>
                  <Input
                    value={entryByKey("oidc_client_id")?.value ?? ""}
                    onChange={(event) => updateEntry("oidc_client_id", event.target.value)}
                    placeholder="starlane-notes"
                    disabled={entryByKey("oidc_client_id")?.locked}
                  />
                </label>
                <label className="grid gap-1.5">
                  <span className="text-text text-sm font-medium">Client secret</span>
                  <Input
                    value={entryByKey("oidc_client_secret")?.value ?? ""}
                    onChange={(event) => updateEntry("oidc_client_secret", event.target.value)}
                    placeholder={
                      entryByKey("oidc_client_secret")?.effective_value
                        ? "Leave blank to keep the current secret"
                        : "Enter client secret"
                    }
                    type="password"
                    autoComplete="new-password"
                    disabled={entryByKey("oidc_client_secret")?.locked}
                  />
                </label>
              </div>

              <div className="border-border border-t pt-4">
                <h3 className="text-text text-sm font-medium">Claims and provisioning</h3>
                <p className="text-text-muted mt-1 text-xs">
                  Map identity claims and choose whether first-time users are created automatically.
                </p>
                <div className="mt-3 grid gap-4 sm:grid-cols-2">
                  <label className="grid gap-1.5 sm:col-span-2">
                    <span className="text-text text-sm font-medium">Scopes</span>
                    <Input
                      value={entryByKey("oidc_scopes")?.value ?? ""}
                      onChange={(event) => updateEntry("oidc_scopes", event.target.value)}
                      placeholder="openid profile email"
                      disabled={entryByKey("oidc_scopes")?.locked}
                    />
                  </label>
                  <label className="grid gap-1.5">
                    <span className="text-text text-sm font-medium">User claim</span>
                    <Input
                      value={entryByKey("oidc_user_claim")?.value ?? ""}
                      onChange={(event) => updateEntry("oidc_user_claim", event.target.value)}
                      placeholder="email"
                      disabled={entryByKey("oidc_user_claim")?.locked}
                    />
                  </label>
                  <label className="grid gap-1.5">
                    <span className="text-text text-sm font-medium">Role claim</span>
                    <Input
                      value={entryByKey("oidc_role_claim")?.value ?? ""}
                      onChange={(event) => updateEntry("oidc_role_claim", event.target.value)}
                      placeholder="roles"
                      disabled={entryByKey("oidc_role_claim")?.locked}
                    />
                  </label>
                  <label className="grid gap-1.5">
                    <span className="text-text text-sm font-medium">Admin role value</span>
                    <Input
                      value={entryByKey("oidc_role_admin_value")?.value ?? ""}
                      onChange={(event) => updateEntry("oidc_role_admin_value", event.target.value)}
                      placeholder="admin"
                      disabled={entryByKey("oidc_role_admin_value")?.locked}
                    />
                  </label>
                  <div className="rounded-card border-border flex items-center justify-between gap-3 border px-3 py-2">
                    <div>
                      <p className="text-text text-sm">Just-in-time provisioning</p>
                      <p className="text-text-muted text-xs">Create users after their first SSO login.</p>
                    </div>
                    <AuthenticationToggle
                      checked={configBoolean(entryByKey("oidc_auto_provision"))}
                      disabled={entryByKey("oidc_auto_provision")?.locked}
                      label="Just-in-time provisioning"
                      onChange={(checked) =>
                        updateEntry("oidc_auto_provision", String(checked))
                      }
                    />
                  </div>
                </div>
              </div>
            </div>

            <footer className="border-border flex justify-end gap-2 border-t px-5 py-3">
              <Button
                type="button"
                variant="secondary"
                onClick={() => setProviderDialogOpen(false)}
                disabled={savingProvider}
              >
                Cancel
              </Button>
              <Button type="button" onClick={() => void saveProvider()} disabled={savingProvider}>
                {savingProvider ? "Saving..." : "Save provider"}
              </Button>
            </footer>
          </div>
        </DialogShell>
      )}
    </div>
  );
}

function ApiKeysSection({ localMode }: { localMode: boolean }) {
  const [keys, setKeys] = useState<ApiKeyDisplayItem[]>([]);
  const [serviceKeys, setServiceKeys] = useState<ApiKeyDisplayItem[]>([]);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);
  const [revokeTarget, setRevokeTarget] = useState<ApiKeyDisplayItem | null>(null);
  const [loadingKeys, setLoadingKeys] = useState(false);
  const [revoking, setRevoking] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadKeys = useCallback(async () => {
    if (localMode) {
      setKeys([]);
      setServiceKeys([]);
      return;
    }
    setLoadingKeys(true);
    try {
      const [items, serviceItems] = await Promise.all([
        api.fetch<ApiKeyDisplayItem[]>("/auth/api-keys"),
        api.fetch<ApiKeyDisplayItem[]>("/admin/api-keys"),
      ]);
      setKeys(items);
      setServiceKeys(serviceItems);
    } finally {
      setLoadingKeys(false);
    }
  }, [localMode]);

  useEffect(() => {
    void loadKeys().catch((err) => {
      setError(err instanceof Error ? err.message : "Unable to load API keys");
    });
  }, [loadKeys]);

  const createKey = async (input: ApiKeyCreateInput): Promise<ApiKeyCreateResult> => {
    if (localMode) {
      throw new Error("API keys are unavailable in local standalone mode.");
    }
    setMessage(null);
    setError(null);
    const endpoint = input.kind === "service" ? "/admin/api-keys" : "/auth/api-keys";
    const result = await api.fetch<ApiKeyCreateResult>(endpoint, {
      method: "POST",
      body: {
        name: input.name,
        expires_at: input.expiresAt,
        scopes: input.scopes,
      },
    });
    setMessage(`${result.name} created.`);
    try {
      await loadKeys();
    } catch (loadError) {
      setError(
        loadError instanceof Error
          ? loadError.message
          : "The key was created, but the key list could not be refreshed.",
      );
    }
    return result;
  };

  const revokeKey = async () => {
    if (localMode || !revokeTarget || revoking) return;
    setMessage(null);
    setError(null);
    setRevoking(true);
    try {
      const endpoint = revokeTarget.is_service_account ? "/admin/api-keys" : "/auth/api-keys";
      await api.fetch(`${endpoint}/${encodeURIComponent(revokeTarget.id)}`, { method: "DELETE" });
      setMessage(`${revokeTarget.name} revoked.`);
      await loadKeys();
      setRevokeTarget(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to revoke API key");
    } finally {
      setRevoking(false);
    }
  };

  const allKeys = useMemo(() => {
    const keysById = new Map<string, ApiKeyDisplayItem>();
    for (const key of [...keys, ...serviceKeys]) {
      keysById.set(key.id, key);
    }
    return Array.from(keysById.values()).sort(
      (left, right) => Date.parse(right.created_at) - Date.parse(left.created_at),
    );
  }, [keys, serviceKeys]);

  return (
    <div>
      <AdminApiKeyManagementCard
        keys={allKeys}
        localMode={localMode}
        loading={loadingKeys}
        onCreate={() => setCreateDialogOpen(true)}
        onRefresh={() => {
          setError(null);
          void loadKeys().catch((err) => {
            setError(err instanceof Error ? err.message : "Unable to load API keys");
          });
        }}
        onRevoke={setRevokeTarget}
      />

      {message && <div className={`${ADMIN_SUCCESS_CARD_CLASS} mt-3`}>{message}</div>}
      {error && <div className={`${ADMIN_ERROR_CARD_CLASS} mt-3`}>{error}</div>}

      {createDialogOpen && (
        <ApiKeyCreateDialog
          onCreate={createKey}
          onClose={() => setCreateDialogOpen(false)}
        />
      )}

      {revokeTarget && (
        <ConfirmDialog
          labelledBy="revoke-api-key-title"
          title={`Revoke ${revokeTarget.name}?`}
          message="This key will stop authenticating immediately. This action cannot be undone."
          helperText={`Key prefix: ${revokeTarget.key_prefix}`}
          confirmLabel={revoking ? "Revoking..." : "Revoke key"}
          destructive
          disabled={revoking}
          onConfirm={() => void revokeKey()}
          onCancel={() => {
            if (!revoking) setRevokeTarget(null);
          }}
        />
      )}
    </div>
  );
}

const DEFAULT_WEBHOOK_EVENTS = ["public_page.published"];

export function AdminWebhookDeliveryHistoryTable({
  deliveries,
  localMode,
  redeliveringDeliveryId,
  redeliveryError,
  onRedeliver,
  disableRedelivery = false,
}: AdminWebhookDeliveryHistoryTableProps) {
  return (
    <div className="space-y-2">
      {redeliveryError && (
        <div className={ADMIN_COMPACT_ERROR_CLASS}>{redeliveryError}</div>
      )}
      <div
        role="region"
        aria-label="Webhook delivery attempts"
        tabIndex={0}
        className="rounded-card border-border overflow-x-auto border focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary"
      >
        <table className="w-full min-w-[36rem] text-sm">
          <thead>
            <tr className="border-border text-text-muted border-b text-left text-xs">
              <th className="px-3 py-2 font-medium">Time</th>
              <th className="px-3 py-2 font-medium">Event</th>
              <th className="px-3 py-2 font-medium">Status</th>
              <th className="px-3 py-2 font-medium">Attempt</th>
              <th className="px-3 py-2 font-medium"></th>
            </tr>
          </thead>
          <tbody>
            {deliveries.map((delivery) => {
              const isRedelivering = redeliveringDeliveryId === delivery.id;
              return (
                <tr
                  key={delivery.id}
                  className="border-border border-b last:border-0"
                >
                  <td className="text-text-muted px-3 py-2">
                    {new Date(delivery.delivered_at).toLocaleString()}
                  </td>
                  <td className="px-3 py-2">
                    {delivery.event_type}
                    {delivery.test_id && (
                      <span className="text-text-muted mt-1 block break-all text-xs">
                        Test ID: {delivery.test_id}
                      </span>
                    )}
                  </td>
                  <td className="px-3 py-2">
                    <span
                      className={getAdminWebhookDeliveryStatusClass(
                        delivery.success,
                      )}
                    >
                      {delivery.response_status ?? "No response"}
                    </span>
                  </td>
                  <td className="text-text-muted px-3 py-2">
                    {delivery.attempt_number}
                  </td>
                  <td className="px-3 py-2 text-right">
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={
                        localMode ||
                        disableRedelivery ||
                        isRedelivering ||
                        redeliveringDeliveryId !== null
                      }
                      onClick={() => {
                        onRedeliver(delivery.id);
                      }}
                    >
                      {isRedelivering ? "Queueing..." : "Redeliver"}
                    </Button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

export type WebhookTestResult = "waiting" | "retrying" | "succeeded" | "failed";

export function getWebhookTestResult(
  deliveries: WebhookDeliveryItem[],
  testId: string,
): WebhookTestResult {
  const attempts = deliveries.filter(
    (delivery) =>
      delivery.event_type === "webhook.test" && delivery.test_id === testId,
  );
  if (
    attempts.some(
      (delivery) =>
        delivery.success &&
        typeof delivery.response_status === "number" &&
        delivery.response_status >= 200 &&
        delivery.response_status < 300,
    )
  )
    return "succeeded";
  if (attempts.some((delivery) => delivery.attempt_number >= 3))
    return "failed";
  return attempts.length ? "retrying" : "waiting";
}

interface WebhookRequestContext {
  base: string;
  token: string | null;
}

// Administration needs a fresh server observation, not api.fetch's offline GET fallback.
async function requestWebhook<T>(
  context: WebhookRequestContext,
  path: string,
  method = "GET",
  body?: unknown,
  expectedStatus?: number,
): Promise<T> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 10_000);
  try {
    const headers: Record<string, string> = {};
    if (context.token) headers.Authorization = `Bearer ${context.token}`;
    if (body !== undefined) headers["Content-Type"] = "application/json";
    const response = await fetch(`${context.base}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: controller.signal,
      cache: "no-store",
    });
    if (!response.ok) {
      const error = await response
        .json()
        .catch(() => ({ message: `HTTP ${response.status}` }));
      throw new Error(
        typeof error.message === "string"
          ? error.message
          : `HTTP ${response.status}`,
      );
    }
    if (expectedStatus !== undefined && response.status !== expectedStatus)
      throw new Error(
        "The server did not confirm that this request was queued.",
      );
    if (response.status === 204) return undefined as T;
    const text = await response.text();
    return (text.trim() ? JSON.parse(text) : undefined) as T;
  } catch (error) {
    if (error instanceof Error && error.name === "AbortError")
      throw new Error(
        method === "GET"
          ? "Fresh history request timed out. Refresh to check recorded deliveries."
          : "Request timed out. Its outcome is unconfirmed; refresh before retrying.",
      );
    if (error instanceof TypeError)
      throw new Error(
        method === "GET"
          ? "Unable to reach the server for a fresh observation."
          : "Server response unavailable. The outcome is unconfirmed; refresh before retrying.",
      );
    throw error;
  } finally {
    clearTimeout(timeout);
  }
}

const newWebhookForm = (): WebhookFormState => ({
  name: "",
  url: "",
  secret: "",
  events: DEFAULT_WEBHOOK_EVENTS.join(", "),
  space_id: "",
});
const WEBHOOK_TEST_WAIT_MS = 60_000;

export function WebhooksSection({ localMode }: { localMode: boolean }) {
  const base = getApiBase();
  const token = typeof localStorage === "undefined" ? null : api.getToken();
  const context = useMemo(() => ({ base, token }), [base, token]);
  const scope = JSON.stringify([base, token, localMode]);
  const scopeRef = useRef(scope);
  scopeRef.current = scope;
  const mounted = useRef(true);
  const listRequest = useRef(0);
  const historyRequest = useRef(0);
  const selectionGeneration = useRef(0);
  const selectedRef = useRef<string | null>(null);
  const actionRef = useRef<string | null>(null);
  const [action, setAction] = useState<string | null>(null);
  const [webhooks, setWebhooks] = useState<WebhookItem[]>([]);
  const [selectedWebhookId, setSelectedWebhookId] = useState<string | null>(
    null,
  );
  const [deliveries, setDeliveries] = useState<WebhookDeliveryItem[]>([]);
  const [deliveriesLoading, setDeliveriesLoading] = useState(false);
  const [deliveriesError, setDeliveriesError] = useState<string | null>(null);
  const [redeliveringDeliveryId, setRedeliveringDeliveryId] = useState<
    string | null
  >(null);
  const [redeliveryError, setRedeliveryError] = useState<string | null>(null);
  const [form, setForm] = useState<WebhookFormState>(newWebhookForm);
  const [createOpen, setCreateOpen] = useState(false);
  const [disconnectTarget, setDisconnectTarget] = useState<WebhookItem | null>(
    null,
  );
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [testRun, setTestRun] = useState<{
    webhookId: string;
    testId: string;
    startedAt: number;
  } | null>(null);
  const [testStatus, setTestStatus] = useState<
    WebhookTestResult | "unconfirmed" | null
  >(null);
  selectedRef.current = selectedWebhookId;
  const selectedWebhook =
    webhooks.find((webhook) => webhook.id === selectedWebhookId) ?? null;
  const selectedWebhookHealth = selectedWebhook
    ? getAdminWebhookHealth(selectedWebhook.is_active, deliveries)
    : null;
  const testPending =
    testRun !== null && (testStatus === "waiting" || testStatus === "retrying");

  const isCurrent = useCallback(
    () =>
      mounted.current &&
      scopeRef.current === scope &&
      getApiBase() === context.base &&
      api.getToken() === context.token &&
      !localMode,
    [scope, context, localMode],
  );
  const selectWebhook = useCallback((id: string | null) => {
    if (selectedRef.current === id) return;
    selectedRef.current = id;
    selectionGeneration.current += 1;
    historyRequest.current += 1;
    setSelectedWebhookId(id);
    setDeliveries([]);
    setDeliveriesError(null);
    setDeliveriesLoading(false);
    setRedeliveryError(null);
    setMessage(null);
    setError(null);
    setTestRun(null);
    setTestStatus(null);
    setDisconnectTarget(null);
  }, []);

  useEffect(() => {
    mounted.current = true;
    listRequest.current += 1;
    historyRequest.current += 1;
    selectionGeneration.current += 1;
    selectedRef.current = null;
    setSelectedWebhookId(null);
    setWebhooks([]);
    setDeliveries([]);
    setDeliveriesError(null);
    setTestRun(null);
    setTestStatus(null);
    setMessage(null);
    setError(null);
    setRedeliveryError(null);
    setDeliveriesLoading(false);
    setCreateOpen(false);
    setDisconnectTarget(null);
    return () => {
      mounted.current = false;
      listRequest.current += 1;
      historyRequest.current += 1;
      selectionGeneration.current += 1;
    };
  }, [scope]);

  const loadWebhooks = useCallback(async () => {
    if (!isCurrent()) return null;
    const request = ++listRequest.current;
    setLoading(true);
    setError(null);
    try {
      const items = await requestWebhook<WebhookItem[]>(context, "/webhooks");
      if (!isCurrent() || listRequest.current !== request) return null;
      setWebhooks(items);
      const current = selectedRef.current;
      if (!current || !items.some((item) => item.id === current))
        selectWebhook(items[0]?.id ?? null);
      return items;
    } catch (err) {
      if (isCurrent() && listRequest.current === request)
        setError(
          err instanceof Error ? err.message : "Unable to load webhooks",
        );
      return null;
    } finally {
      if (isCurrent() && listRequest.current === request) setLoading(false);
    }
  }, [isCurrent, context, selectWebhook]);

  const loadDeliveries = useCallback(
    async (
      webhookId: string | null,
      preserve = false,
    ): Promise<WebhookDeliveryItem[] | null | undefined> => {
      if (!webhookId || !isCurrent() || selectedRef.current !== webhookId)
        return undefined;
      const generation = selectionGeneration.current;
      const request = ++historyRequest.current;
      const current = () =>
        isCurrent() &&
        generation === selectionGeneration.current &&
        selectedRef.current === webhookId &&
        request === historyRequest.current;
      if (!preserve) {
        setDeliveries([]);
        setDeliveriesLoading(true);
      }
      setDeliveriesError(null);
      try {
        const items = await requestWebhook<WebhookDeliveryItem[]>(
          context,
          `/webhooks/${encodeURIComponent(webhookId)}/deliveries`,
        );
        if (!current()) return undefined;
        setDeliveries(items);
        return items;
      } catch (err) {
        if (!current()) return undefined;
        setDeliveriesError(
          err instanceof Error
            ? err.message
            : "Unable to load webhook deliveries",
        );
        return null;
      } finally {
        if (current()) setDeliveriesLoading(false);
      }
    },
    [context, isCurrent],
  );

  useEffect(() => {
    void loadWebhooks();
  }, [loadWebhooks]);
  useEffect(() => {
    if (selectedWebhookId) void loadDeliveries(selectedWebhookId);
  }, [loadDeliveries, selectedWebhookId]);

  useEffect(() => {
    if (!testRun) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const generation = selectionGeneration.current;
    const current = () =>
      !cancelled &&
      isCurrent() &&
      selectedRef.current === testRun.webhookId &&
      selectionGeneration.current === generation;
    const poll = async () => {
      if (!current()) return;
      if (Date.now() - testRun.startedAt >= WEBHOOK_TEST_WAIT_MS) {
        setTestStatus("unconfirmed");
        return;
      }
      const items = await loadDeliveries(testRun.webhookId, true);
      if (!current()) return;
      if (items === undefined) {
        timer = setTimeout(() => void poll(), 1_000);
        return;
      }
      if (items === null) {
        setTestStatus("unconfirmed");
        return;
      }
      const outcome = getWebhookTestResult(items, testRun.testId);
      setTestStatus(outcome);
      if (outcome === "succeeded" || outcome === "failed") return;
      if (Date.now() - testRun.startedAt >= WEBHOOK_TEST_WAIT_MS) {
        setTestStatus("unconfirmed");
        return;
      }
      timer = setTimeout(() => void poll(), 1_000);
    };
    timer = setTimeout(() => void poll(), 250);
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, [testRun, isCurrent, loadDeliveries]);

  const beginAction = (name: string): string | null => {
    if (!isCurrent() || actionRef.current) return null;
    const lock = `${name}:${Date.now()}:${Math.random()}`;
    actionRef.current = lock;
    setAction(name);
    return lock;
  };
  const endAction = (lock: string) => {
    if (actionRef.current !== lock) return;
    actionRef.current = null;
    if (mounted.current) {
      setAction(null);
      setRedeliveringDeliveryId(null);
    }
  };

  const handleCreateWebhook = async (event: FormEvent) => {
    event.preventDefault();
    const lock = beginAction("creating");
    if (!lock) return;
    setError(null);
    setMessage(null);
    try {
      const events = parseWebhookEvents(form.events);
      if (!form.name.trim() || !form.url.trim() || events.length === 0)
        throw new Error("Name, URL, and at least one event are required.");
      const result = await requestWebhook<{ id: string }>(
        context,
        "/webhooks",
        "POST",
        {
          name: form.name.trim(),
          url: form.url.trim(),
          secret: form.secret.trim() || undefined,
          events,
          space_id: form.space_id.trim() || undefined,
        },
        201,
      );
      if (!isCurrent()) return;
      setForm(newWebhookForm());
      setCreateOpen(false);
      const items = await loadWebhooks();
      if (isCurrent()) {
        if (items?.some((item) => item.id === result.id))
          selectWebhook(result.id);
        setMessage(
          items
            ? "Webhook created."
            : "Webhook created. Refresh connections to confirm the updated list.",
        );
      }
    } catch (err) {
      if (isCurrent())
        setError(
          err instanceof Error ? err.message : "Unable to create webhook",
        );
    } finally {
      endAction(lock);
    }
  };

  const testWebhook = async () => {
    if (!selectedWebhook || !selectedWebhook.is_active || testPending) return;
    const id = selectedWebhook.id;
    const generation = selectionGeneration.current;
    const lock = beginAction("testing");
    if (!lock) return;
    const current = () =>
      isCurrent() &&
      selectedRef.current === id &&
      selectionGeneration.current === generation;
    setError(null);
    setMessage(null);
    setTestRun(null);
    setTestStatus(null);
    try {
      const result = await requestWebhook<{
        status: string;
        delivery_id: string;
      }>(
        context,
        `/webhooks/${encodeURIComponent(id)}/test`,
        "POST",
        undefined,
        202,
      );
      if (!current()) return;
      if (
        result?.status !== "queued" ||
        typeof result.delivery_id !== "string" ||
        !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(
          result.delivery_id,
        )
      )
        throw new Error(
          "The server did not provide a valid test confirmation.",
        );
      setTestRun({
        webhookId: id,
        testId: result.delivery_id,
        startedAt: Date.now(),
      });
      setTestStatus("waiting");
    } catch (err) {
      if (current())
        setError(
          err instanceof Error ? err.message : "Unable to queue webhook test",
        );
    } finally {
      endAction(lock);
    }
  };

  const disconnect = async () => {
    const target = disconnectTarget;
    if (!target || selectedRef.current !== target.id) return;
    const generation = selectionGeneration.current;
    const lock = beginAction("disconnecting");
    if (!lock) return;
    const current = () =>
      isCurrent() &&
      selectedRef.current === target.id &&
      selectionGeneration.current === generation;
    setError(null);
    setMessage(null);
    try {
      await requestWebhook<void>(
        context,
        `/webhooks/${encodeURIComponent(target.id)}`,
        "DELETE",
        undefined,
        204,
      );
      if (!isCurrent()) return;
      if (current()) {
        setDisconnectTarget(null);
        selectWebhook(null);
        setMessage(
          "Webhook disconnected. Its subscription and delivery history were deleted.",
        );
      }
      await loadWebhooks();
    } catch (err) {
      if (current())
        setError(
          err instanceof Error ? err.message : "Unable to disconnect webhook",
        );
    } finally {
      endAction(lock);
    }
  };

  const redeliver = async (deliveryId: string) => {
    if (!selectedWebhook || !selectedWebhook.is_active || testPending) return;
    const id = selectedWebhook.id;
    const generation = selectionGeneration.current;
    const lock = beginAction("redelivering");
    if (!lock) return;
    const current = () =>
      isCurrent() &&
      selectedRef.current === id &&
      selectionGeneration.current === generation;
    setRedeliveryError(null);
    setMessage(null);
    setTestRun(null);
    setTestStatus(null);
    setRedeliveringDeliveryId(deliveryId);
    try {
      await requestWebhook<void>(
        context,
        `/webhooks/${encodeURIComponent(id)}/deliveries/${encodeURIComponent(deliveryId)}/redeliver`,
        "POST",
        undefined,
        202,
      );
      if (!current()) return;
      setMessage(
        "Redelivery queued. Refresh history to inspect new recorded attempts; queueing does not confirm delivery.",
      );
      await loadDeliveries(id, true);
    } catch (err) {
      if (current())
        setRedeliveryError(
          err instanceof Error ? err.message : "Unable to queue redelivery",
        );
    } finally {
      endAction(lock);
    }
  };

  const refreshHistory = async () => {
    const items = await loadDeliveries(selectedWebhookId, true);
    if (
      items &&
      testRun &&
      selectedRef.current === testRun.webhookId &&
      isCurrent()
    ) {
      const outcome = getWebhookTestResult(items, testRun.testId);
      if (outcome === "succeeded" || outcome === "failed")
        setTestStatus(outcome);
    }
  };

  const testText =
    testStatus === "succeeded"
      ? "Test delivered: the receiver returned HTTP 2xx for this test."
      : testStatus === "failed"
        ? "Test failed after three recorded attempts. Check the receiver and delivery history."
        : testStatus === "retrying"
          ? "A test attempt failed. The server may retry; delivery is not confirmed yet."
          : testStatus === "unconfirmed"
            ? "Final delivery not confirmed. Refresh history to check recorded attempts."
            : "Test queued. Waiting for a recorded receiver response.";

  return (
    <div className="min-w-0">
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-text text-lg font-medium">Webhooks</h1>
          <p className="text-text-muted text-xs">
            Manage outbound event deliveries and inspect recent delivery
            attempts.
          </p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            variant="secondary"
            disabled={localMode || loading}
            onClick={() => {
              void loadWebhooks();
              void refreshHistory();
            }}
          >
            <RefreshCw size={14} className="mr-1" />
            Refresh
          </Button>
          <Button
            size="sm"
            disabled={localMode || action !== null}
            onClick={() => {
              setError(null);
              setCreateOpen(true);
            }}
          >
            <Plus size={14} className="mr-1" />
            Create webhook
          </Button>
        </div>
      </div>
      <div className="space-y-3">
        {localMode && (
          <div className="rounded-card border-border text-text-muted border p-3 text-sm">
            Webhook delivery is a server-mode feature. Local standalone notes do
            not dispatch outbound webhooks.
          </div>
        )}
        <p className="text-text-muted text-xs">
          Ordinary connections send minimal deliberately public page publication
          metadata to the stored endpoint. A synthetic test sends no page or
          draft content.
        </p>
        <div className="grid min-w-0 gap-3 xl:grid-cols-[minmax(18rem,24rem)_minmax(0,1fr)]">
          <div className="rounded-card border-border min-w-0 border">
            {webhooks.length === 0 ? (
              <div className="text-text-muted p-3 text-sm">
                {loading ? "Loading webhooks..." : "No webhooks configured."}
              </div>
            ) : (
              webhooks.map((webhook) => (
                <button
                  key={webhook.id}
                  type="button"
                  aria-pressed={selectedWebhookId === webhook.id}
                  onClick={() => selectWebhook(webhook.id)}
                  className={`border-border block min-h-11 w-full border-b px-3 py-2 text-left last:border-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary ${selectedWebhookId === webhook.id ? "bg-surface-raised" : "hover:bg-surface"}`}
                >
                  <span className="text-text block truncate text-sm font-medium">
                    {webhook.name}
                  </span>
                  <span className="text-text-muted block truncate text-xs">
                    {webhook.url}
                  </span>
                  <span className="text-text-muted mt-1 flex flex-wrap gap-2 text-xs">
                    <span>{webhook.events.length} events</span>
                    <span>{webhook.is_active ? "Configured" : "Inactive"}</span>
                    <span>
                      {webhook.space_id
                        ? `Space: ${webhook.space_id}`
                        : "Global scope"}
                    </span>
                  </span>
                </button>
              ))
            )}
          </div>
          <div className="rounded-card border-border min-w-0 border p-3">
            {selectedWebhook ? (
              <div className="space-y-4">
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0 basis-full sm:basis-auto sm:flex-1">
                    <h2 className="text-text break-words text-sm font-medium">
                      {selectedWebhook.name}
                    </h2>
                    <p className="text-text-muted break-all text-xs">
                      {selectedWebhook.url}
                    </p>
                    <dl className="text-text-muted mt-2 grid gap-1 text-xs">
                      <div className="flex flex-wrap gap-x-2">
                        <dt>Created by</dt>
                        <dd className="text-text break-all">
                          {selectedWebhook.created_by || "Not available"}
                        </dd>
                      </div>
                      <div className="flex flex-wrap gap-x-2">
                        <dt>Scope</dt>
                        <dd className="text-text break-all">
                          {selectedWebhook.space_id
                            ? `Space ${selectedWebhook.space_id}`
                            : "Global: matching events from all spaces"}
                        </dd>
                      </div>
                      <div className="flex flex-wrap gap-x-2">
                        <dt>Configuration</dt>
                        <dd className="text-text">
                          {selectedWebhook.is_active
                            ? "Active subscription"
                            : "Inactive subscription"}
                        </dd>
                      </div>
                      <div className="flex flex-wrap gap-x-2">
                        <dt>Data</dt>
                        <dd className="text-text">
                          Public page title, IDs, and publication time only
                        </dd>
                      </div>
                    </dl>
                    <div className="mt-2 flex flex-wrap gap-1">
                      {selectedWebhook.events.map((eventName, index) => (
                        <span
                          key={`${eventName}:${index}`}
                          className="rounded-button bg-surface-raised text-text-muted break-all px-2 py-0.5 text-xs"
                        >
                          {eventName}
                        </span>
                      ))}
                    </div>
                  </div>
                  <div className="flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={
                        localMode ||
                        !selectedWebhook.is_active ||
                        action !== null ||
                        testPending
                      }
                      onClick={() => void testWebhook()}
                    >
                      {action === "testing"
                        ? "Queueing test..."
                        : "Test saved endpoint"}
                    </Button>
                    <Button
                      size="sm"
                      variant="danger"
                      disabled={localMode || action !== null}
                      onClick={() => {
                        setError(null);
                        setDisconnectTarget(selectedWebhook);
                      }}
                    >
                      <Trash2 size={14} className="mr-1" />
                      Disconnect
                    </Button>
                  </div>
                </div>
                {selectedWebhookHealth && (
                  <div
                    role="status"
                    className={`rounded-input border px-3 py-2 text-sm ${selectedWebhookHealth.className}`}
                  >
                    <p className="font-medium">{selectedWebhookHealth.label}</p>
                    <p className="mt-1 text-xs">{selectedWebhookHealth.detail}</p>
                  </div>
                )}
                {testRun && (
                  <div
                    role="status"
                    aria-live="polite"
                    className={`rounded-input border px-3 py-2 text-sm ${testStatus === "succeeded" ? "border-success/30 bg-success/10 text-success" : testStatus === "failed" ? "border-danger/30 bg-danger/10 text-danger" : "border-border text-text-muted"}`}
                  >
                    <p>{testText}</p>
                    <p className="mt-1 break-all text-xs">
                      Test ID: {testRun.testId}
                    </p>
                    <p className="mt-1 text-xs">
                      This records one test, not ongoing endpoint health.
                    </p>
                  </div>
                )}
                <div>
                  <div className="mb-2 flex flex-wrap items-center justify-between gap-2">
                    <h3 className="text-text text-sm font-medium">
                      Delivery History
                    </h3>
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={localMode || deliveriesLoading}
                      onClick={() => void refreshHistory()}
                    >
                      Refresh history
                    </Button>
                  </div>
                  {deliveriesLoading ? (
                    <p className="text-text-muted text-sm">
                      Loading deliveries...
                    </p>
                  ) : deliveriesError ? (
                    <div className="text-text-muted grid gap-3 text-sm">
                      <p>{deliveriesError}</p>
                      <Button
                        variant="secondary"
                        size="sm"
                        className="justify-self-start"
                        onClick={() => void refreshHistory()}
                      >
                        Retry deliveries
                      </Button>
                    </div>
                  ) : deliveries.length === 0 ? (
                    <p className="text-text-muted text-sm">
                      No deliveries recorded yet.
                    </p>
                  ) : (
                    <AdminWebhookDeliveryHistoryTable
                      deliveries={deliveries}
                      localMode={localMode}
                      redeliveringDeliveryId={redeliveringDeliveryId}
                      redeliveryError={redeliveryError}
                      disableRedelivery={
                        !selectedWebhook.is_active ||
                        action !== null ||
                        testPending
                      }
                      onRedeliver={(deliveryId) => void redeliver(deliveryId)}
                    />
                  )}
                  {redeliveryError && deliveries.length === 0 && (
                    <div className={ADMIN_COMPACT_ERROR_CLASS}>
                      {redeliveryError}
                    </div>
                  )}
                </div>
              </div>
            ) : (
              <div>
                <h3 className="text-text mb-2 text-sm font-medium">
                  Delivery History
                </h3>
                <p className="text-text-muted text-sm">
                  Select a webhook to inspect delivery history.
                </p>
              </div>
            )}
          </div>
        </div>
        {message && (
          <div
            role="status"
            className="rounded-input border-border text-text-muted border px-3 py-2 text-sm"
          >
            {message}
          </div>
        )}
        {error && !createOpen && !disconnectTarget && (
          <div role="alert" className={ADMIN_ERROR_CARD_CLASS}>
            {error}
          </div>
        )}
      </div>
      {createOpen && (
        <DialogShell
          labelledBy="create-webhook-title"
          maxWidth="lg"
          onClose={() => {
            if (action !== "creating") setCreateOpen(false);
          }}
        >
          <form
            onSubmit={(event) => void handleCreateWebhook(event)}
            className="space-y-4 p-4"
          >
            <div>
              <h2
                id="create-webhook-title"
                className="text-text text-base font-semibold"
              >
                Create webhook
              </h2>
              <p className="text-text-muted mt-1 text-sm">
                Connect a trusted receiver. Test the saved endpoint after
                creation.
              </p>
            </div>
            <div className="grid gap-3 sm:grid-cols-2">
              <label className="block">
                <span className="text-text-muted mb-1 block text-xs">Name</span>
                <AdminTextInput
                  value={form.name}
                  onChange={(value) =>
                    setForm((current) => ({ ...current, name: value }))
                  }
                  disabled={action !== null}
                  placeholder="Release automation"
                  ariaLabel="Webhook name"
                />
              </label>
              <label className="block">
                <span className="text-text-muted mb-1 block text-xs">URL</span>
                <AdminTextInput
                  value={form.url}
                  onChange={(value) =>
                    setForm((current) => ({ ...current, url: value }))
                  }
                  disabled={action !== null}
                  placeholder="https://example.com/starlane/webhook"
                  ariaLabel="Webhook URL"
                />
              </label>
              <label className="block sm:col-span-2">
                <span className="text-text-muted mb-1 block text-xs">
                  Events
                </span>
                <AdminTextInput
                  value={form.events}
                  onChange={(value) =>
                    setForm((current) => ({ ...current, events: value }))
                  }
                  disabled={action !== null}
                  placeholder={DEFAULT_WEBHOOK_EVENTS.join(", ")}
                  ariaLabel="Webhook events"
                />
                <span className="text-text-muted mt-1 block text-xs">
                  Ordinary connections export only deliberately public page
                  publication metadata. Private resources and wildcards are refused.
                </span>
              </label>
              <label className="block">
                <span className="text-text-muted mb-1 block text-xs">
                  Secret
                </span>
                <AdminTextInput
                  value={form.secret}
                  onChange={(value) =>
                    setForm((current) => ({ ...current, secret: value }))
                  }
                  disabled={action !== null}
                  placeholder="Optional HMAC secret"
                  type="password"
                  ariaLabel="Webhook secret"
                />
              </label>
              <label className="block">
                <span className="text-text-muted mb-1 block text-xs">
                  Space ID
                </span>
                <AdminTextInput
                  value={form.space_id}
                  onChange={(value) =>
                    setForm((current) => ({ ...current, space_id: value }))
                  }
                  disabled={action !== null}
                  placeholder="Optional: blank means global"
                  ariaLabel="Webhook space ID"
                />
              </label>
            </div>
            {error && (
              <div role="alert" className={ADMIN_ERROR_CARD_CLASS}>
                {error}
              </div>
            )}
            <div className="flex flex-wrap justify-end gap-2">
              <Button
                type="button"
                variant="secondary"
                disabled={action !== null}
                onClick={() => setCreateOpen(false)}
              >
                Cancel
              </Button>
              <Button disabled={action !== null}>
                {action === "creating" ? "Creating..." : "Create webhook"}
              </Button>
            </div>
          </form>
        </DialogShell>
      )}
      {disconnectTarget && (
        <ConfirmDialog
          labelledBy="disconnect-webhook-title"
          title={`Disconnect ${disconnectTarget.name}?`}
          message="This permanently deletes the subscription and its delivery history. Future sends and retries stop; an already in-flight request may still finish."
          helperText={error || disconnectTarget.url}
          confirmLabel={
            action === "disconnecting"
              ? "Disconnecting..."
              : "Disconnect webhook"
          }
          destructive
          disabled={action !== null}
          onConfirm={() => void disconnect()}
          onCancel={() => {
            if (action !== "disconnecting") {
              setDisconnectTarget(null);
              setError(null);
            }
          }}
        />
      )}
    </div>
  );
}

export function parseWebhookEvents(value: string): string[] {
  return value
    .split(",")
    .map((eventName) => eventName.trim())
    .filter(Boolean);
}

const NOTIFICATION_CONFIG_KEYS = new Set([
  "smtp_host",
  "smtp_port",
  "smtp_user",
  "smtp_password",
  "smtp_from",
]);

function NotificationsSection({ localMode }: { localMode: boolean }) {
  const [entries, setEntries] = useState<ConfigEntry[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadConfig = useCallback(async () => {
    if (localMode) {
      setEntries([]);
      return;
    }
    const allEntries = await api.fetch<ConfigEntry[]>("/admin/config");
    setEntries(allEntries.filter((entry) => NOTIFICATION_CONFIG_KEYS.has(entry.key)));
  }, [localMode]);

  useEffect(() => {
    void loadConfig().catch((err) => {
      setError(err instanceof Error ? err.message : "Unable to load notification settings");
    });
  }, [loadConfig]);

  const saveConfig = async (entry: ConfigEntry) => {
    if (entry.locked) return;
    setMessage(null);
    setError(null);
    try {
      await api.fetch("/admin/config", {
        method: "POST",
        body: { key: entry.key, value: entry.value },
      });
      setMessage(`${entry.label} saved.`);
      await loadConfig();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to save notification setting");
    }
  };

  const removeConfig = async (entry: ConfigEntry) => {
    if (entry.locked) return;
    setMessage(null);
    setError(null);
    try {
      await api.fetch(`/admin/config/${encodeURIComponent(entry.key)}`, { method: "DELETE" });
      setMessage(`${entry.label} reset.`);
      await loadConfig();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to reset notification setting");
    }
  };

  return (
    <div>
      <div className="mb-4 flex items-center justify-between gap-3">
        <div>
          <h1 className="text-text text-lg font-medium">Notifications</h1>
          <p className="text-text-muted text-xs">
            Configure SMTP delivery for workflow and document notification emails.
          </p>
        </div>
        <Button
          size="sm"
          variant="secondary"
          disabled={localMode}
          onClick={() => {
            void loadConfig();
          }}
        >
          <RefreshCw size={14} className="mr-1" />
          Refresh
        </Button>
      </div>

      <div className="space-y-3">
        {localMode && (
          <div className="rounded-card border-border text-text-muted border p-3 text-sm">
            Server email delivery is unavailable in local standalone mode. Local notifications stay
            in-app only.
          </div>
        )}

        <div className="rounded-card border-border border p-3">
          <div className="mb-3">
            <p className="text-text text-sm font-medium">Email Delivery</p>
            <p className="text-text-muted text-xs">
              Environment-backed SMTP values are locked here and must be changed on the server.
            </p>
          </div>
          <div className="space-y-2">
            {entries.length === 0 ? (
              <p className="text-text-muted text-sm">
                {localMode ? "No server settings available." : "Loading notification settings..."}
              </p>
            ) : (
              entries.map((entry) => (
                <div
                  key={entry.key}
                  className="rounded-card border-border grid gap-2 border p-2 lg:grid-cols-[minmax(0,12rem)_minmax(0,1fr)_auto_auto]"
                >
                  <div className="min-w-0">
                    <p className="text-text truncate text-sm">{entry.label}</p>
                    <p className="text-text-muted truncate text-xs">
                      {entry.key}
                      {entry.env_var ? ` · ${entry.env_var}` : ""}
                    </p>
                  </div>
                  <AdminTextInput
                    value={entry.value}
                    onChange={(value) => {
                      setEntries((prev) =>
                        prev.map((item) => (item.key === entry.key ? { ...item, value } : item)),
                      );
                    }}
                    disabled={localMode || entry.locked}
                    placeholder={entry.effective_value || "Not set"}
                    type={entry.secret ? "password" : "text"}
                    ariaLabel={`${entry.label} value`}
                  />
                  <span className="rounded-button border-border text-text-muted flex h-8 items-center border px-2 text-xs">
                    {entry.locked ? "env locked" : entry.source}
                  </span>
                  <div className="flex gap-2">
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={localMode || entry.locked || (entry.secret && !entry.value)}
                      onClick={() => {
                        void saveConfig(entry);
                      }}
                    >
                      Save
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={localMode || entry.locked}
                      onClick={() => {
                        void removeConfig(entry);
                      }}
                    >
                      Reset
                    </Button>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>

        {message && (
          <div className={ADMIN_SUCCESS_CARD_CLASS}>
            {message}
          </div>
        )}
        {error && (
          <div className={ADMIN_ERROR_CARD_CLASS}>
            {error}
          </div>
        )}
      </div>
    </div>
  );
}

function AuditSection({ localMode }: { localMode: boolean }) {
  const [entries, setEntries] = useState<
    {
      id: string;
      user_email?: string;
      action: string;
      target_type?: string;
      target_id?: string;
      created_at: string;
    }[]
  >([]);
  const [loading, setLoading] = useState(false);
  const [actionFilter, setActionFilter] = useState("");

  const loadEntries = useCallback(() => {
    if (localMode) {
      setEntries([]);
      return;
    }
    setLoading(true);
    api
      .fetch<typeof entries>(
        `/admin/audit-log?limit=100${actionFilter.trim() ? `&action=${encodeURIComponent(actionFilter.trim())}` : ""}`,
      )
      .then(setEntries)
      .catch(() => setEntries([]))
      .finally(() => setLoading(false));
  }, [actionFilter, localMode]);

  useEffect(() => {
    loadEntries();
  }, [loadEntries]);

  return (
    <div>
      <div className="mb-4 flex items-center justify-between">
        <div>
          <h1 className="text-text text-lg font-medium">Audit Log</h1>
          <p className="text-text-muted mt-1 text-xs">
            Page, draft, publish, archive, trash, and security events. Lifecycle JSONL files are
            retained for 30 days under the vault logging folder for ingestion.
          </p>
        </div>
        <Button
          size="sm"
          variant="secondary"
          disabled={localMode}
          onClick={() => {
            window.open("/api/v1/admin/audit-log/export", "_blank");
          }}
        >
          Export CSV
        </Button>
        <Button size="sm" variant="ghost" disabled={localMode || loading} onClick={loadEntries}>
          {loading ? "Refreshing…" : "Refresh"}
        </Button>
      </div>

      <div className="mb-3 flex items-center gap-2">
        <Input
          value={actionFilter}
          onChange={(event) => setActionFilter(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") loadEntries();
          }}
          placeholder="Filter actions (for example document.draft)"
          aria-label="Filter audit actions"
          className="max-w-md"
        />
        <Button size="sm" variant="secondary" disabled={localMode || loading} onClick={loadEntries}>
          Apply filter
        </Button>
      </div>

      <div className="rounded-card border-border border">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-border text-text-muted border-b text-left text-xs">
              <th className="px-3 py-2 font-medium">Time</th>
              <th className="px-3 py-2 font-medium">User</th>
              <th className="px-3 py-2 font-medium">Action</th>
              <th className="px-3 py-2 font-medium">Target</th>
            </tr>
          </thead>
          <tbody>
            {entries.map((e) => (
              <tr key={e.id} className="border-border border-b last:border-0">
                <td className="text-text-muted px-3 py-2">
                  {new Date(e.created_at).toLocaleString()}
                </td>
                <td className="px-3 py-2">{e.user_email || "—"}</td>
                <td className="px-3 py-2">{e.action}</td>
                <td className="text-text-muted px-3 py-2">
                  {e.target_type || "—"}
                  {e.target_id ? ` · ${e.target_id}` : ""}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function SearchSection({ localMode }: { localMode: boolean }) {
  const [reindexing, setReindexing] = useState(false);
  const [result, setResult] = useState<{
    status: string;
    documents_indexed: number;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleReindex = async () => {
    if (localMode) {
      setError("Search reindex is unavailable in local mode.");
      return;
    }
    setReindexing(true);
    setResult(null);
    setError(null);
    try {
      const res = await api.fetch<{ status: string; documents_indexed: number }>(
        "/admin/search/reindex",
        { method: "POST" },
      );
      setResult(res);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Reindex failed");
    } finally {
      setReindexing(false);
    }
  };

  return (
    <div>
      <h1 className="text-text mb-4 text-lg font-medium">Search</h1>
      <div className="space-y-3">
        {localMode && (
          <div className="rounded-card border-border text-text-muted border p-3 text-sm">
            Search indexing is local-only in this mode and does not need a server reindex.
          </div>
        )}
        <div className="rounded-card border-border flex items-center justify-between border p-3">
          <div>
            <p className="text-text text-sm">Rebuild Search Index</p>
            <p className="text-text-muted text-xs">
              Re-indexes all documents. Use if search results seem stale or incomplete.
            </p>
          </div>
          <Button size="sm" onClick={handleReindex} disabled={reindexing || localMode}>
            {reindexing ? "Reindexing..." : "Reindex"}
          </Button>
        </div>
        {result && (
          <div className="rounded-card border-border text-text-muted border p-3 text-sm">
            Reindex complete — {result.documents_indexed} document
            {result.documents_indexed !== 1 ? "s" : ""} indexed.
          </div>
        )}
        {error && (
          <div className={ADMIN_ERROR_CARD_CLASS}>
            {error}
          </div>
        )}
      </div>
    </div>
  );
}

function StorageSection({ localMode, user }: { localMode: boolean; user: User | null }) {
  const { runtime, isMobileRuntime } = useRuntimePlatform();
  const nativeAvailable = canUseNativeVaultControls(runtime);
  const importInputRef = useRef<HTMLInputElement>(null);
  const [location, setLocation] = useState<VaultLocation | null>(null);
  const [vaultFolders, setVaultFolders] = useState<VaultFolderSummary[]>([]);
  const [activeVaultName, setActiveVaultName] = useState("");
  const [newVaultName, setNewVaultName] = useState("");
  const [customPath, setCustomPath] = useState("");
  const [serverUrl, setServerUrl] = useState(() => readLinkedServerUrl());
  const [stats, setStats] = useState<LocalVaultStats | null>(null);
  const [staticSiteTitle, setStaticSiteTitle] = useState("");
  const [staticSiteDescription, setStaticSiteDescription] = useState("");
  const [staticSiteTheme, setStaticSiteTheme] = useState<StaticSiteTheme>("light");
  const [queuedOperationCount, setQueuedOperationCount] = useState(0);
  const [queuedOperations, setQueuedOperations] = useState<VaultQueuedSyncOperation[]>([]);
  const [replayHistory, setReplayHistory] = useState<LocalSyncReplayHistoryManifestRecord[]>([]);
  const [lastReplayResult, setLastReplayResult] =
    useState<LocalSyncReplayServerResultRecord | null>(null);
  const [selectedReplayOperationIds, setSelectedReplayOperationIds] = useState<Set<string>>(
    () => new Set(),
  );
  const [serverSpaces, setServerSpaces] = useState<SpaceItem[]>([]);
  const [serverSpaceCheckpoints, setServerSpaceCheckpoints] = useState<
    SharedSpaceDownloadCheckpoint[]
  >(() => readSharedSpaceDownloadCheckpoints());
  const [resetStorageConfirmOpen, setResetStorageConfirmOpen] = useState(false);
  const [clearQueueConfirmTarget, setClearQueueConfirmTarget] = useState<
    "selected" | "all" | null
  >(null);
  const [archiveImportConfirmOpen, setArchiveImportConfirmOpen] = useState(false);
  const [archiveRecovery, setArchiveRecovery] = useState<AdminArchiveRecoverySession | null>(null);
  const [pendingSnapshotImport, setPendingSnapshotImport] = useState<{
    snapshot: unknown;
    summary: VaultImportPreflightSummary;
  } | null>(null);
  const [replayConfirmOperationIds, setReplayConfirmOperationIds] = useState<string[] | null>(
    null,
  );
  const [replayConfirmOpen, setReplayConfirmOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refreshLocation = useCallback(async () => {
    if (!nativeAvailable) return;
    const [next, folders] = await Promise.all([
      getVaultLocation(),
      listVaultFolders().catch(() => []),
    ]);
    setLocation(next);
    setVaultFolders(folders);
    setCustomPath(next.path);
    setActiveVaultName(folders.find((folder) => folder.current)?.name ?? getVaultFolderName(next.path));
  }, [nativeAvailable]);

  useEffect(() => {
    refreshLocation().catch((err) => {
      setError(err instanceof Error ? err.message : "Failed to load vault location");
    });
  }, [refreshLocation]);

  const refreshStats = useCallback(async () => {
    if (!localMode) {
      setStats(null);
      return;
    }
    const next = await dataClient.getLocalVaultStats();
    setStats(next);
  }, [localMode]);

  useEffect(() => {
    refreshStats().catch((err) => {
      setError(err instanceof Error ? err.message : "Failed to load vault contents");
    });
  }, [refreshStats]);

  const refreshQueuedOperations = useCallback(async () => {
    if (!localMode) {
      setQueuedOperationCount(0);
      setQueuedOperations([]);
      return;
    }
    const operations = await dataClient.listQueuedSyncOperations();
    setQueuedOperationCount(operations.length);
    setQueuedOperations(operations);
  }, [localMode]);

  useEffect(() => {
    refreshQueuedOperations().catch((err) => {
      setError(err instanceof Error ? err.message : "Failed to load queued sync operations");
    });
  }, [refreshQueuedOperations]);

  const refreshReplayHistory = useCallback(async () => {
    if (!localMode) {
      setReplayHistory([]);
      return;
    }
    const history = await dataClient.listQueuedSyncReplayHistory();
    setReplayHistory(history.manifests);
  }, [localMode]);

  useEffect(() => {
    refreshReplayHistory().catch(() => {
      setReplayHistory([]);
    });
  }, [refreshReplayHistory]);

  const refreshServerSpaces = useCallback(async () => {
    if (localMode) {
      setServerSpaces([]);
      return;
    }
    const spaces = await dataClient.listSpaces();
    setServerSpaces(spaces.map((space) => ({ ...space, visibility: "server" })));
  }, [localMode]);

  useEffect(() => {
    refreshServerSpaces().catch((err) => {
      setError(err instanceof Error ? err.message : "Failed to load server spaces");
    });
  }, [refreshServerSpaces]);

  const recoverableReplayResults = useMemo(() => {
    const currentResults =
      lastReplayResult?.results.filter(
        (item) => item.status === "failed" || item.status === "unsupported",
      ) ?? [];
    const currentIds = new Set(currentResults.map((item) => item.operation_id));
    return [
      ...currentResults,
      ...queuedOperations
        .filter((operation) => operation.last_error && !currentIds.has(operation.id))
        .map((operation) => ({
          operation_id: operation.id,
          entity_type: operation.entity_type,
          action: operation.action,
          status: "failed",
          message: operation.last_error ?? "Previous replay failed.",
        })),
    ];
  }, [lastReplayResult, queuedOperations]);

  const selectedReplayOperationIdList = useMemo(
    () => Array.from(selectedReplayOperationIds),
    [selectedReplayOperationIds],
  );

  useEffect(() => {
    setSelectedReplayOperationIds(
      new Set(recoverableReplayResults.map((item) => item.operation_id)),
    );
  }, [recoverableReplayResults]);

  const chooseFolder = async () => {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const selectedPath = await chooseVaultFolder(location?.path);
      if (!selectedPath) return;
      const next = await setVaultPath(selectedPath);
      setLocation(next);
      setCustomPath(next.path);
      await refreshStats();
      setMessage("Vault folder updated.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to choose vault folder");
    } finally {
      setBusy(false);
    }
  };

  const savePath = async () => {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const next = await setVaultPath(customPath);
      setLocation(next);
      setCustomPath(next.path);
      await refreshStats();
      setMessage("Vault folder updated.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to update vault folder");
    } finally {
      setBusy(false);
    }
  };

  const resetPath = async () => {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const next = await clearVaultPath();
      setLocation(next);
      setCustomPath(next.path);
      await refreshStats();
      setMessage("Vault folder reset to the app default.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to reset vault folder");
    } finally {
      setBusy(false);
    }
  };

  const switchVault = async (folder: VaultFolderSummary) => {
    if (folder.current) return;

    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const next = await setVaultPath(folder.path);
      setLocation(next);
      setCustomPath(next.path);
      await Promise.all([refreshStats(), refreshLocation()]);
      setMessage(`Switched to ${folder.name}. Reload the app if stale local data remains visible.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to switch vault");
    } finally {
      setBusy(false);
    }
  };

  const renameActiveVault = async () => {
    const name = activeVaultName.trim();
    if (!name) return;

    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const next = await setVaultFolderName(name);
      setLocation(next);
      setCustomPath(next.path);
      setActiveVaultName(getVaultFolderName(next.path));
      await Promise.all([refreshStats(), refreshLocation()]);
      setMessage(`Renamed active vault to ${name}. Reload the app if stale local data remains visible.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to rename active vault");
    } finally {
      setBusy(false);
    }
  };

  const createVault = async () => {
    const name = newVaultName.trim();
    if (!name) return;

    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const next = await createVaultFolder(name);
      setLocation(next);
      setCustomPath(next.path);
      setNewVaultName("");
      await Promise.all([refreshStats(), refreshLocation()]);
      setMessage(`Created and switched to ${name}. Reload the app if stale local data remains visible.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to create vault");
    } finally {
      setBusy(false);
    }
  };

  const revealFolder = async () => {
    if (!canRevealAdminVaultFolder(nativeAvailable, location)) return;
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      await revealVaultFolder();
      setMessage("Vault folder opened.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to open vault folder");
    } finally {
      setBusy(false);
    }
  };

  const exportVault = async () => {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const stamp = new Date().toISOString().slice(0, 10);
      if (nativeAvailable) {
        const result = await exportCurrentCompleteNativeVaultArchive();
        if (result.cancelled) return;
        setMessage(`Vault archive exported to ${result.path}.`);
      } else {
        const snapshot = await dataClient.exportLocalVaultSnapshot();
        downloadJsonFile(`starlane-vault-${stamp}.json`, snapshot);
        setMessage("Vault JSON export prepared.");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to export vault");
    } finally {
      setBusy(false);
    }
  };

  const getStaticSiteOptions = (fallbackTitle?: string) => ({
    title: staticSiteTitle.trim() || fallbackTitle,
    description: staticSiteDescription.trim() || undefined,
    theme: staticSiteTheme,
  });

  const exportStaticVault = async () => {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const snapshot = await dataClient.hydrateStaticSiteExportAssets(
        await dataClient.exportLocalVaultSnapshot(),
      );
      const site = buildStaticSiteHtml(snapshot, getStaticSiteOptions());
      downloadTextFile(site.filename, site.html, "text/html;charset=utf-8");
      setMessage(`Static HTML export prepared with ${site.document_count} document${site.document_count === 1 ? "" : "s"}.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to export static HTML");
    } finally {
      setBusy(false);
    }
  };

  const exportStaticVaultZip = async () => {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const snapshot = await dataClient.hydrateStaticSiteExportAssets(
        await dataClient.exportLocalVaultSnapshot(),
      );
      const bundle = buildStaticSiteBundle(snapshot, getStaticSiteOptions());
      downloadBlob(bundle.filename, buildStaticSiteZipBlob(bundle));
      setMessage(formatStaticSiteBundleSummary(bundle));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to export static ZIP");
    } finally {
      setBusy(false);
    }
  };

  const importVaultArchive = async () => {
    if (busy) return;
    setBusy(true); setError(null); setMessage(null);
    const mode=getCurrentLaunchMode(), provider=getSyncProvider(), base=getApiBase(), token=api.getToken();
    const verifyCurrent=() => {
      if (getCurrentLaunchMode()!==mode || getSyncProvider()!==provider || getApiBase()!==base || api.getToken()!==token) throw new Error("Original archive recovery workspace changed; keep its evidence for review");
    };
    try {
      const inventory=await dataClient.listLocalVaultArchiveImports();verifyCurrent();
      const pending=inventory.operations.find(row=>!row.canonical_committed && !row.retained_abandoned);
      if (pending) {
        if (!inventory.binding) throw new Error("Original pending archive database is unavailable");
        const session=await createAdminArchiveRecoverySession(inventory.binding,pending,verifyCurrent);verifyCurrent();setArchiveRecovery(session);
      } else setArchiveImportConfirmOpen(true);
    } catch (error) { setError(error instanceof Error ? error.message : "Archive evidence inspection failed"); }
    finally {setBusy(false);}
  };
  const confirmAbandonArchiveImport = async () => {
    if (busy || !archiveRecovery) return;
    const original=archiveRecovery;setBusy(true);setError(null);setMessage(null);
    try {
      await original.abandon();setArchiveRecovery(null);
      setMessage("Import attempt retained and abandoned. Its archive, unique asset versions and database evidence remain available. Choose Import again to start a new attempt.");
    } catch (error) {setError(error instanceof Error ? error.message : "Archive resolution acknowledgement is unknown; inspect the original evidence again");setArchiveRecovery(null);}
    finally {setBusy(false);}
  };

  const confirmImportVaultArchive = async () => {
    setArchiveImportConfirmOpen(false);
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const selectedPath = await chooseVaultArchiveImportPath();
      if (!selectedPath) return;
      await dataClient.importLocalVaultArchive(selectedPath);
      await refreshStats();
      setMessage("Vault archive imported.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to import vault");
    } finally {
      setBusy(false);
    }
  };

  const importVaultFile = async (file: File) => {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const text = await file.text();
      const snapshot = JSON.parse(text) as unknown;
      const summary = summarizeVaultImportSnapshot(snapshot);
      setPendingSnapshotImport({ snapshot, summary });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to import vault");
    } finally {
      setBusy(false);
      if (importInputRef.current) importInputRef.current.value = "";
    }
  };

  const confirmImportVaultSnapshot = async () => {
    const pendingImport = pendingSnapshotImport;
    if (!pendingImport) return;

    setPendingSnapshotImport(null);
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      await dataClient.importLocalVaultSnapshot(pendingImport.snapshot);
      await Promise.all([refreshStats(), refreshQueuedOperations()]);
      setMessage("Vault imported.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to import vault");
    } finally {
      setBusy(false);
    }
  };

  const exportQueuedReplayManifest = async () => {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const manifest = await dataClient.exportQueuedSyncReplayManifest();
      const stamp = new Date().toISOString().replace(/[:.]/g, "-");
      downloadJsonFile(`starlane-sync-replay-${stamp}.json`, manifest);
      const missingPayloads = manifest.operation_summary?.missing_payload_count ?? 0;
      const unavailablePayloads = manifest.operation_summary?.unavailable_payloads ?? [];
      const unavailableSummary =
        unavailablePayloads.length > 0
          ? ` Missing: ${unavailablePayloads
              .slice(0, 3)
              .map(
                (payload) =>
                  payload.asset_name ??
                  `${payload.entity_type}:${payload.action}:${payload.entity_id}`,
              )
              .join(", ")}${unavailablePayloads.length > 3 ? "..." : ""}.`
          : "";
      setMessage(
        `Queued sync replay manifest exported with ${manifest.operation_count} operation${manifest.operation_count === 1 ? "" : "s"}, ${missingPayloads} missing payload${missingPayloads === 1 ? "" : "s"}, and ${manifest.recovery_checkpoint_count} recovery checkpoint${manifest.recovery_checkpoint_count === 1 ? "" : "s"}.${unavailableSummary}`,
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to export queued sync replay manifest");
    } finally {
      setBusy(false);
    }
  };

  const preflightQueuedReplayManifest = async () => {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      await api.fetch<{ status: string }>("/health");
      const result = await dataClient.preflightQueuedSyncReplayManifest();
      if (result.ready) {
        setMessage(
          `Linked server is reachable. Replay preflight passed for ${result.ready_operation_count} operation${result.ready_operation_count === 1 ? "" : "s"} against ${result.target_backends.join("/")} server backends.`,
        );
      } else {
        setError(
          `Replay preflight blocked ${result.blocked_operation_count} operation${result.blocked_operation_count === 1 ? "" : "s"}: ${result.blockers.join("; ")}`,
        );
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to preflight queued sync replay");
    } finally {
      setBusy(false);
    }
  };

  const runQueuedReplay = (operationIds?: string[]) => {
    setReplayConfirmOperationIds(operationIds ?? null);
    setReplayConfirmOpen(true);
  };

  const confirmRunQueuedReplay = async () => {
    const operationIds = replayConfirmOperationIds ?? undefined;
    setReplayConfirmOpen(false);
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      await api.fetch<{ status: string }>("/health");
      const preflight = await dataClient.preflightQueuedSyncReplayManifest(operationIds);
      if (!preflight.ready) {
        setError(
          `Replay preflight blocked ${preflight.blocked_operation_count} operation${preflight.blocked_operation_count === 1 ? "" : "s"}: ${preflight.blockers.join("; ")}`,
        );
        return;
      }
      const result = await dataClient.runQueuedSyncReplay(operationIds);
      setLastReplayResult(result);
      const clearableIds = result.results
        .filter((item) => item.status === "applied" || item.status === "skipped")
        .map((item) => item.operation_id);
      if (clearableIds.length > 0) {
        await dataClient.clearQueuedSyncOperations(clearableIds);
      }
      await Promise.all([refreshQueuedOperations(), refreshReplayHistory()]);
      const summary = formatAdminReplayResultSummary(result);
      if (result.failed_operation_count > 0 || result.unsupported_operation_count > 0) {
        setError(`Replay completed partially: ${summary}.`);
      } else {
        setMessage(`Replay completed: ${summary}.`);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to run queued sync replay");
    } finally {
      setBusy(false);
    }
  };

  const toggleReplayRecoverySelection = (operationId: string, selected: boolean) => {
    setSelectedReplayOperationIds((prev) => {
      const next = new Set(prev);
      if (selected) {
        next.add(operationId);
      } else {
        next.delete(operationId);
      }
      return next;
    });
  };

  const clearSelectedReplayOperations = async () => {
    if (selectedReplayOperationIdList.length === 0) return;
    setClearQueueConfirmTarget("selected");
  };

  const confirmClearSelectedReplayOperations = async () => {
    if (selectedReplayOperationIdList.length === 0) return;
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      await dataClient.clearQueuedSyncOperations(selectedReplayOperationIdList);
      setSelectedReplayOperationIds(new Set());
      await refreshQueuedOperations();
      setMessage(
        `Cleared ${selectedReplayOperationIdList.length} selected queued replay operation${selectedReplayOperationIdList.length === 1 ? "" : "s"}.`,
      );
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to clear selected queued sync operations",
      );
    } finally {
      setBusy(false);
    }
  };

  const clearQueuedOperations = () => {
    setClearQueueConfirmTarget("all");
  };

  const confirmClearQueuedOperations = async () => {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      await dataClient.clearQueuedSyncOperations();
      await refreshQueuedOperations();
      setMessage("Queued sync operations cleared.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to clear queued sync operations");
    } finally {
      setBusy(false);
    }
  };

  const confirmClearQueue = () => {
    const target = clearQueueConfirmTarget;
    setClearQueueConfirmTarget(null);
    if (target === "selected") {
      void confirmClearSelectedReplayOperations();
    } else if (target === "all") {
      void confirmClearQueuedOperations();
    }
  };

  const savePortableServerSpace = async (space:SpaceItem) => {
    setBusy(true);setMessage(null);setError(null);
    try {await saveBrowserServerSpaceArchive(space.id);setMessage(`${space.name} portable Markdown ZIP saved.`);}
    catch(error){setError(error instanceof Error?error.message:"Portable archive Save failed.");}
    finally{setBusy(false);}
  };
  const reviewPortableServerSpace = async (space:SpaceItem) => {
    setMessage(null);setError(null);
    try{acknowledgeBrowserPortableArchiveReview(space.id);setMessage("Archive destination review recorded.");}
    catch(error){setError(error instanceof Error?error.message:"Archive destination review refused.");}
  };

  const downloadServerSpace = async (space: SpaceItem) => {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const snapshot = await dataClient.hydrateStaticSiteExportAssets(
        await dataClient.exportServerSpaceSnapshot(space.id),
      );
      const stamp = new Date().toISOString().slice(0, 10);
      downloadJsonFile(`starlane-space-${space.slug || space.id}-${stamp}.json`, snapshot);
      setServerSpaceCheckpoints(
        recordSharedSpaceDownloadCheckpoint({
          spaceId: space.id,
          spaceName: space.name,
          slug: space.slug,
          sourceServerUrl: readCheckpointServerUrl(),
          downloadedByUserId: user?.id,
          downloadedByDisplayName: user?.display_name,
          downloadedAt: new Date().toISOString(),
          documentCount: snapshot.documents.length,
          folderCount: snapshot.folders.length,
          tagCount: snapshot.tags?.length ?? 0,
          propertyCount: snapshot.property_schemas?.length ?? 0,
        }),
      );
      setMessage(`${space.name} snapshot prepared for local import.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to download server space");
    } finally {
      setBusy(false);
    }
  };

  const exportStaticServerSpace = async (space: SpaceItem) => {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      const snapshot = await dataClient.hydrateStaticSiteExportAssets(
        await dataClient.exportServerSpaceSnapshot(space.id),
      );
      const site = buildStaticSiteHtml(snapshot, getStaticSiteOptions(space.name));
      downloadTextFile(site.filename, site.html, "text/html;charset=utf-8");
      setMessage(`${space.name} static HTML prepared with ${site.document_count} document${site.document_count === 1 ? "" : "s"}.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to export static HTML");
    } finally {
      setBusy(false);
    }
  };

  const exportStaticServerSpaceZip = async (space: SpaceItem) => {
    setBusy(true);
    setMessage(null);
    setError(null);
    try {
      await saveBrowserServerSpaceStaticArchive(space.id,getStaticSiteOptions(space.name));
      setMessage(`${space.name} static ZIP saved.`);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save static ZIP");
    } finally {
      setBusy(false);
    }
  };

  const saveServerUrl = () => {
    const trimmed = serverUrl.trim().replace(/\/+$/, "");
    setMessage(null);
    setError(null);
    if (!trimmed) {
      setError("Server URL is required.");
      return;
    }
    try {
      new URL(trimmed);
    } catch {
      setError("Enter a valid server URL.");
      return;
    }
    if (typeof window !== "undefined") {
      window.localStorage.setItem("server_url", trimmed);
    }
    setStorageMode("server");
    setSyncProvider("none");
    setServerUrl(trimmed);
    setMessage("Linked server updated. Reload the app to reconnect to the new server.");
  };

  const resetStorageSetup = () => {
    setResetStorageConfirmOpen(true);
  };

  const confirmResetStorageSetup = () => {
    clearStorageSelection();
    if (typeof window !== "undefined") {
      window.localStorage.removeItem("server_url");
    }
    setServerUrl("");
    setResetStorageConfirmOpen(false);
    setMessage("Storage setup reset. Reload the app to choose local or server mode again.");
  };

  return (
    <div>
      {resetStorageConfirmOpen && (
        <ConfirmDialog
          labelledBy="admin-storage-reset-title"
          title="Reset storage setup"
          message={getAdminStorageResetConfirmationMessage()}
          confirmLabel="Reset setup"
          destructive
          onCancel={() => setResetStorageConfirmOpen(false)}
          onConfirm={confirmResetStorageSetup}
        />
      )}
      {clearQueueConfirmTarget && (
        <ConfirmDialog
          labelledBy="admin-clear-queue-title"
          title={clearQueueConfirmTarget === "selected" ? "Clear selected queue" : "Clear queue"}
          message={getAdminQueuedSyncClearConfirmationMessage(
            clearQueueConfirmTarget === "selected"
              ? selectedReplayOperationIdList.length
              : undefined,
          )}
          confirmLabel="Clear"
          destructive
          onCancel={() => setClearQueueConfirmTarget(null)}
          onConfirm={confirmClearQueue}
        />
      )}
      {replayConfirmOpen && (
        <ConfirmDialog
          labelledBy="admin-run-replay-title"
          title={replayConfirmOperationIds ? "Retry selected operations" : "Run queued replay"}
          message={getAdminQueuedSyncReplayConfirmationMessage(
            replayConfirmOperationIds ? replayConfirmOperationIds.length : undefined,
          )}
          confirmLabel={replayConfirmOperationIds ? "Retry" : "Run replay"}
          onCancel={() => setReplayConfirmOpen(false)}
          onConfirm={() => void confirmRunQueuedReplay()}
        />
      )}
      {archiveRecovery && (
        <ConfirmDialog
          labelledBy="admin-retain-archive-attempt-title"
          title="Retain and abandon import attempt"
          message={`No canonical import receipt was found for operation ${archiveRecovery.operationId}. Abandon this attempt so another import can begin? The original archive, staged or installed unique assets and database evidence will remain. This does not claim the import succeeded or restore a previous snapshot.`}
          confirmLabel="Retain and abandon"
          onCancel={() => setArchiveRecovery(null)}
          onConfirm={() => void confirmAbandonArchiveImport()}
        />
      )}
      {archiveImportConfirmOpen && (
        <ConfirmDialog
          labelledBy="admin-import-vault-archive-title"
          title="Import vault archive"
          message={getAdminVaultArchiveImportConfirmationMessage()}
          confirmLabel="Import"
          destructive
          onCancel={() => setArchiveImportConfirmOpen(false)}
          onConfirm={() => void confirmImportVaultArchive()}
        />
      )}
      {pendingSnapshotImport && (
        <ConfirmDialog
          labelledBy="admin-import-vault-snapshot-title"
          title="Import vault snapshot"
          message={getAdminVaultSnapshotImportConfirmationMessage(pendingSnapshotImport.summary)}
          confirmLabel="Import"
          destructive
          onCancel={() => setPendingSnapshotImport(null)}
          onConfirm={() => void confirmImportVaultSnapshot()}
        />
      )}
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-text text-lg font-medium">Storage</h1>
        <span className="text-text-muted text-xs">
          {localMode ? "Standalone vault" : "Server-backed"}
        </span>
      </div>

      <div className="space-y-3">
        {!localMode && canUseHumanAdministration(user?.role, user?.account_kind) && <StorageHealthPanel key={`${getApiBase()}|${user?.id}`} />}
        {!localMode && (
          <div className="rounded-card border-border border p-3">
            <div className="mb-3 flex items-start justify-between gap-3">
              <div>
                <p className="text-text text-sm font-medium">Linked Server</p>
                <p className="text-text-muted mt-1 text-xs">
                  Switch the server-backed app shell by updating the self-hosted server URL.
                </p>
              </div>
              <span className="text-text-muted ring-border shrink-0 rounded px-2 py-1 text-xs ring-1">
                Server mode
              </span>
            </div>

            <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto_auto]">
              <AdminTextInput
                value={serverUrl}
                onChange={setServerUrl}
                placeholder="https://notes.example.com"
                ariaLabel="Linked server URL"
                spellCheck={false}
              />
              <Button size="sm" onClick={saveServerUrl} disabled={busy || !serverUrl.trim()}>
                Save Server
              </Button>
              <Button size="sm" variant="secondary" onClick={resetStorageSetup} disabled={busy}>
                <RotateCcw size={14} />
                Reset Setup
              </Button>
            </div>
          </div>
        )}
        {localMode && (
          <div className="rounded-card border-border border p-3">
            <div className="mb-3 flex items-start justify-between gap-3">
              <div>
                <p className="text-text text-sm font-medium">Sync Server Vault</p>
                <p className="text-text-muted mt-1 text-xs">
                  Add a self-hosted server URL when this local vault should reconnect through the
                  server-backed app shell.
                </p>
              </div>
              <span className="text-text-muted ring-border shrink-0 rounded px-2 py-1 text-xs ring-1">
                Optional
              </span>
            </div>

            <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
              <AdminTextInput
                value={serverUrl}
                onChange={setServerUrl}
                placeholder="https://notes.example.com"
                ariaLabel="Sync server URL"
                spellCheck={false}
              />
              <Button size="sm" onClick={saveServerUrl} disabled={busy || !serverUrl.trim()}>
                Add Server Vault
              </Button>
            </div>
          </div>
        )}

        <div className="rounded-card border-border border p-3">
          <div className="mb-3 flex items-start justify-between gap-3">
            <div>
              <p className="text-text text-sm font-medium">Vault Folder</p>
              <p className="text-text-muted mt-1 text-xs">
                Local desktop notes are written to a Starlane vault snapshot in this folder.
              </p>
            </div>
            <span className="text-text-muted ring-border shrink-0 rounded px-2 py-1 text-xs ring-1">
              {location?.using_default === false ? "Custom" : "Default"}
            </span>
          </div>

          {nativeAvailable ? (
            <div className="space-y-3">
              <div>
                <label htmlFor="vault-path" className="text-text-muted mb-1 block text-xs">
                  Folder path
                </label>
                <AdminTextInput
                  id="vault-path"
                  value={customPath}
                  onChange={setCustomPath}
                  ariaLabel="Vault folder path"
                  className="font-mono text-xs"
                  spellCheck={false}
                />
              </div>
              <div className="flex flex-wrap gap-2">
                <Button size="sm" variant="secondary" onClick={chooseFolder} disabled={busy}>
                  <FolderOpen size={14} />
                  Choose Folder
                </Button>
                <Button size="sm" onClick={savePath} disabled={busy || !customPath.trim()}>
                  Save Path
                </Button>
                <Button size="sm" variant="secondary" onClick={resetPath} disabled={busy}>
                  <RotateCcw size={14} />
                  Use Default
                </Button>
              </div>
            </div>
          ) : isMobileRuntime ? (
            <div className="rounded-card border-border bg-surface text-text-muted border px-3 py-2 text-sm">
              Mobile standalone notes use the app sandbox. Folder picking and full vault archives
              will use mobile document providers in a later pass.
            </div>
          ) : (
            <div className="rounded-card border-border bg-surface text-text-muted border px-3 py-2 text-sm">
              Native vault folders are available in the desktop app. Browser local mode uses
              IndexedDB.
            </div>
          )}
        </div>

        <div className="rounded-card border-border border p-3">
          <div className="mb-3 flex items-start justify-between gap-3">
            <div>
              <p className="text-text text-sm font-medium">Static Site Settings</p>
              <p className="text-text-muted mt-1 text-xs">
                Optional title, description, and theme for HTML exports.
              </p>
            </div>
            <span className="text-text-muted ring-border shrink-0 rounded px-2 py-1 text-xs ring-1">
              HTML
            </span>
          </div>
          <div className="grid gap-2 lg:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)_180px]">
            <AdminTextInput
              value={staticSiteTitle}
              onChange={setStaticSiteTitle}
              placeholder="Export title"
              ariaLabel="Static site export title"
            />
            <AdminTextInput
              value={staticSiteDescription}
              onChange={setStaticSiteDescription}
              placeholder="Export description"
              ariaLabel="Static site export description"
            />
            <AdminFormSelect
              value={staticSiteTheme}
              onChange={(value) => setStaticSiteTheme(value as StaticSiteTheme)}
              ariaLabel="Static site export theme"
            >
              <option value="light">Light theme</option>
              <option value="dark">Dark theme</option>
              <option value="paper">Paper theme</option>
            </AdminFormSelect>
          </div>
        </div>

        <div className="rounded-card border-border border p-3">
          <div className="mb-3 flex items-start justify-between gap-3">
            <div>
              <p className="text-text text-sm font-medium">Vault Switcher</p>
              <p className="text-text-muted mt-1 text-xs">
                Desktop app vault folders discovered beside the active vault.
              </p>
            </div>
            {nativeAvailable && (
              <Button
                size="sm"
                variant="secondary"
                onClick={() => void refreshLocation()}
                disabled={busy}
              >
                <RefreshCw size={14} />
                Refresh
              </Button>
            )}
          </div>

          {nativeAvailable ? (
            <div className="space-y-3">
              <div className="overflow-hidden rounded-card border border-border bg-surface">
                {vaultFolders.length === 0 ? (
                  <p className="px-3 py-3 text-sm text-text-muted">No sibling vault folders found.</p>
                ) : (
                  vaultFolders.map((folder) => (
                    <button
                      key={folder.path}
                      type="button"
                      disabled={busy || folder.current}
                      onClick={() => void switchVault(folder)}
                      className={`flex w-full items-center gap-3 border-b border-border/70 px-3 py-2 text-left last:border-b-0 ${
                        folder.current ? "bg-surface-raised" : "hover:bg-surface-raised"
                      } disabled:cursor-default`}
                    >
                      <FolderOpen size={15} className="shrink-0 text-text-muted" />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm text-text">{folder.name}</span>
                        <span className="block truncate text-xs text-text-muted">{folder.path}</span>
                      </span>
                      <span className="text-xs text-text-muted">
                        {folder.current ? "Current" : "Switch"}
                      </span>
                    </button>
                  ))
                )}
              </div>

              <div className="rounded-card border border-border bg-surface-raised p-3">
                <label htmlFor="active-vault-name" className="text-text-muted mb-1 block text-xs">
                  Active vault folder name
                </label>
                <p className="text-text-muted mb-2 text-xs">
                  Renames the active vault folder on this device when the native filesystem allows it.
                </p>
                <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
                  <AdminTextInput
                    id="active-vault-name"
                    value={activeVaultName}
                    onChange={setActiveVaultName}
                    placeholder="Active vault folder name"
                    ariaLabel="Active vault folder name"
                  />
                  <Button
                    size="sm"
                    onClick={() => void renameActiveVault()}
                    disabled={busy || !activeVaultName.trim()}
                  >
                    Rename Vault Folder
                  </Button>
                </div>
                {location?.path && (
                  <p className="text-text-muted mt-2 truncate text-xs">{location.path}</p>
                )}
              </div>

              <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
                <AdminTextInput
                  value={newVaultName}
                  onChange={setNewVaultName}
                  placeholder="New vault name"
                  ariaLabel="New vault name"
                />
                <Button size="sm" onClick={() => void createVault()} disabled={busy || !newVaultName.trim()}>
                  Create Vault
                </Button>
              </div>
            </div>
          ) : (
            <div className="rounded-card border-border bg-surface text-text-muted border px-3 py-2 text-sm">
              Vault folder switching is available in the desktop app. Browser local mode still uses
              IndexedDB-backed local snapshots.
            </div>
          )}
        </div>

        <div className="rounded-card border-border border p-3">
          <div className="mb-3 flex items-start justify-between gap-3">
            <div>
              <p className="text-text text-sm font-medium">Vault Contents</p>
              <p className="text-text-muted mt-1 text-xs">
                Snapshot counts for the local standalone vault.
              </p>
            </div>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => void refreshStats()}
              disabled={!localMode || busy}
            >
              <RefreshCw size={14} />
              Refresh
            </Button>
          </div>

          <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-5">
            {[
              ["Spaces", stats?.spaces ?? 0],
              ["Folders", stats?.folders ?? 0],
              ["Documents", stats?.documents ?? 0],
              ["Assets", stats?.assets ?? 0],
              ["Asset size", formatBytes(stats?.asset_bytes ?? 0)],
            ].map(([label, value]) => (
              <div key={label} className="rounded-card border-border border px-3 py-2">
                <p className="text-text-muted text-xs">{label}</p>
                <p className="text-text mt-1 text-sm font-medium">{value}</p>
              </div>
            ))}
          </div>

          {canRevealAdminVaultFolder(nativeAvailable, location) && (
            <div className="mt-3 flex flex-wrap justify-end gap-2">
              <Button size="sm" variant="secondary" onClick={revealFolder} disabled={busy}>
                <ExternalLink size={14} />
                Open Folder
              </Button>
            </div>
          )}

          {nativeAvailable && localMode && <NativeArchiveOutcomeReviewControl busy={busy} />}
          <div className="border-border mt-3 flex flex-wrap justify-end gap-2 border-t pt-3">
            <input
              ref={importInputRef}
              type="file"
              accept="application/json,.json"
              className="hidden"
              onChange={(event) => {
                const file = event.target.files?.[0];
                if (file) void importVaultFile(file);
              }}
            />
            <Button
              size="sm"
              variant="secondary"
              onClick={() => void exportVault()}
              disabled={!localMode || busy}
            >
              <Download size={14} />
              {nativeAvailable ? "Export Vault" : "Export JSON"}
            </Button>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => void exportStaticVault()}
              disabled={!localMode || busy}
            >
              <Download size={14} />
              Export Static HTML
            </Button>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => void exportStaticVaultZip()}
              disabled={!localMode || busy}
            >
              <Download size={14} />
              Export Static ZIP
            </Button>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => {
                if (nativeAvailable) {
                  void importVaultArchive();
                  return;
                }
                importInputRef.current?.click();
              }}
              disabled={!localMode || busy}
            >
              <Upload size={14} />
              {nativeAvailable ? "Import Vault" : "Import JSON"}
            </Button>
          </div>
        </div>

        {localMode ? (
          <div className="rounded-card border-border border p-3">
            <div className="mb-3 flex items-start justify-between gap-3">
              <div>
                <p className="text-text text-sm font-medium">Queued Sync Replay</p>
                <p className="text-text-muted mt-1 text-xs">
                  Native app writes land locally first, then replay to the linked server when reachable. Applied and idempotently skipped writes are cleared automatically; failed or unsupported writes stay queued for retry or selected recovery.
                </p>
              </div>
              <span className="text-text-muted ring-border shrink-0 rounded px-2 py-1 text-xs ring-1">
                {queuedOperationCount} queued
              </span>
            </div>
            <div className="flex flex-wrap justify-end gap-2">
              <Button
                size="sm"
                variant="secondary"
                onClick={() => void refreshQueuedOperations()}
                disabled={busy}
              >
                <RefreshCw size={14} />
                Refresh Queue
              </Button>
              <Button
                size="sm"
                variant="secondary"
                onClick={() => void exportQueuedReplayManifest()}
                disabled={busy || queuedOperationCount === 0}
              >
                <Download size={14} />
                Export Replay
              </Button>
              <Button
                size="sm"
                variant="secondary"
                onClick={() => void preflightQueuedReplayManifest()}
                disabled={busy || queuedOperationCount === 0}
              >
                <Shield size={14} />
                Check Server
              </Button>
              <Button
                size="sm"
                variant="secondary"
                onClick={() => runQueuedReplay()}
                disabled={busy || queuedOperationCount === 0}
              >
                <Upload size={14} />
                Run Replay
              </Button>
              {lastReplayResult &&
              recoverableReplayResults.length > 0 ? (
                <Button
                  size="sm"
                  variant="secondary"
	                  onClick={() =>
	                    runQueuedReplay(
                      selectedReplayOperationIdList.length > 0
                        ? selectedReplayOperationIdList
                        : recoverableReplayResults.map((item) => item.operation_id),
                    )
                  }
                  disabled={busy || queuedOperationCount === 0}
                >
                  <RotateCcw size={14} />
                  Retry Selected
                </Button>
              ) : null}
              {recoverableReplayResults.length > 0 ? (
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => void clearSelectedReplayOperations()}
                  disabled={busy || selectedReplayOperationIdList.length === 0}
                >
                  <Trash2 size={14} />
                  Clear Selected
                </Button>
              ) : null}
              <Button
                size="sm"
                variant="ghost"
                onClick={() => void clearQueuedOperations()}
                disabled={busy || queuedOperationCount === 0}
              >
                <Trash2 size={14} />
                Clear Queue
              </Button>
            </div>
            {lastReplayResult ? (
              <div className="border-border mt-3 border-t pt-3">
                <p className="text-text text-xs font-medium">Last Replay Result</p>
                <p className="text-text-muted mt-1 text-xs">
                  {formatAdminReplayResultSummary(lastReplayResult)}
                </p>
                {recoverableReplayResults.length > 0 ? (
                  <div className="mt-2 space-y-1">
                    <p className="text-text-muted text-xs">
                      {selectedReplayOperationIdList.length} selected for retry or clear.
                    </p>
                    {recoverableReplayResults.slice(0, 5).map((item) => (
                      <label
                        key={item.operation_id}
                        className="text-text-muted flex items-start gap-2 text-xs"
                      >
                        <input
                          type="checkbox"
                          className="mt-0.5"
                          checked={selectedReplayOperationIds.has(item.operation_id)}
                          onChange={(event) =>
                            toggleReplayRecoverySelection(
                              item.operation_id,
                              event.target.checked,
                            )
                          }
                          aria-label={`Select replay operation ${item.operation_id}`}
                        />
                        <span>
                          <span className="text-text font-medium">{item.status}</span>{" "}
                          {item.entity_type}:{item.action} {item.operation_id} - {item.message}
                        </span>
                      </label>
                    ))}
                  </div>
                ) : null}
              </div>
            ) : null}
            <div className="border-border mt-3 border-t pt-3">
              <div className="mb-2 flex items-center justify-between gap-3">
                <p className="text-text text-xs font-medium">Recent Replay History</p>
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => void refreshReplayHistory()}
                  disabled={busy}
                >
                  <RefreshCw size={14} />
                  Refresh History
                </Button>
              </div>
              {replayHistory.length === 0 ? (
                <p className="text-text-muted text-xs">No replay history recorded yet.</p>
              ) : (
                <div className="space-y-2">
                  {replayHistory.slice(0, 3).map((run) => (
                    <div key={run.id} className="text-text-muted text-xs">
                      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                        <span className="text-text font-medium">{run.status}</span>
                        <span>{new Date(run.created_at).toLocaleString()}</span>
                        <span>{formatAdminReplayResultSummary(run)}</span>
                        <span>{run.recovery_checkpoint_count} checkpoint{run.recovery_checkpoint_count === 1 ? "" : "s"}</span>
                      </div>
                      {run.checkpoints.length > 0 ? (
                        <div className="mt-1 flex flex-wrap gap-x-3 gap-y-1">
                          {run.checkpoints.slice(0, 3).map((checkpoint) => (
                            <span key={checkpoint.id}>
                              {checkpoint.space_id ?? "unknown space"}
                              {checkpoint.source_server_url ? ` from ${checkpoint.source_server_url}` : ""}
                            </span>
                          ))}
                        </div>
                      ) : null}
                    </div>
                  ))}
                </div>
              )}
            </div>
            <ReviewedDraftTransferPanel disabled={busy} />
          </div>
        ) : (
          <div className="rounded-card border-border border p-3">
            <div className="mb-3 flex items-start justify-between gap-3">
              <div>
                <p className="text-text text-sm font-medium">Shared Space Downloads</p>
                <p className="text-text-muted mt-1 text-xs">
                  Download selected server spaces as standalone vault snapshots for local-first use.
                </p>
              </div>
              <Button
                size="sm"
                variant="secondary"
                onClick={() => void refreshServerSpaces()}
                disabled={busy}
              >
                <RefreshCw size={14} />
                Refresh
              </Button>
            </div>
            <div className="overflow-hidden rounded-card border border-border bg-surface">
              {serverSpaces.length === 0 ? (
                <p className="px-3 py-3 text-sm text-text-muted">No shared spaces available.</p>
              ) : (
                serverSpaces.map((space) => (
                  <ServerSpaceDownloadRow
                    key={space.id}
                    space={space}
                    checkpoint={findSharedSpaceDownloadCheckpoint(
                      serverSpaceCheckpoints,
                      space.id,
                    )}
                    busy={busy}
                    onDownload={downloadServerSpace}
                    onExportStatic={exportStaticServerSpace}
                    onExportStaticZip={exportStaticServerSpaceZip}
                    onSavePortable={savePortableServerSpace}
                    onReviewPortable={reviewPortableServerSpace}
                  />
                ))
              )}
            </div>
          </div>
        )}

        <div className="rounded-card border-border border p-3">
          <p className="text-text text-sm font-medium">Current Mode</p>
          <div className="mt-3 grid gap-2 sm:grid-cols-2">
            <div className="rounded-card border-border border px-3 py-2">
              <p className="text-text-muted text-xs">Launch mode</p>
              <p className="text-text mt-1 text-sm">
                {localMode ? "Local standalone" : "Linked server"}
              </p>
            </div>
            <div className="rounded-card border-border border px-3 py-2">
              <p className="text-text-muted text-xs">Sync provider</p>
              <p className="text-text mt-1 text-sm">{getSyncProvider()}</p>
            </div>
          </div>
        </div>

        {message && (
          <div className="rounded-card border-border text-text-muted border px-3 py-2 text-sm">
            {message}
          </div>
        )}
        {error && (
          <div className={ADMIN_ERROR_CARD_CLASS}>
            {error}
          </div>
        )}
      </div>
    </div>
  );
}

function ServerSpaceDownloadRow({
  space,
  checkpoint,
  busy,
  onDownload,
  onExportStatic,
  onExportStaticZip,
  onSavePortable,
  onReviewPortable,
}: {
  space: SpaceItem;
  checkpoint: SharedSpaceDownloadCheckpoint | null;
  busy: boolean;
  onDownload: (space: SpaceItem) => Promise<void>;
  onExportStatic: (space: SpaceItem) => Promise<void>;
  onExportStaticZip: (space: SpaceItem) => Promise<void>;
  onSavePortable: (space: SpaceItem) => Promise<void>;
  onReviewPortable: (space: SpaceItem) => Promise<void>;
}) {
  const archivePending=(()=>{try{return pendingBrowserPortableArchives(space.id);}catch{return [];}})();
  return (
    <div className="flex flex-wrap items-center gap-3 border-b border-border/70 px-3 py-2 last:border-b-0">
      <Globe size={15} className="shrink-0 text-text-muted" />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm text-text" title={space.name}>{space.name}</span>
        <span className="block truncate text-xs text-text-muted">{space.slug}</span>
        {checkpoint && (
          <span className="block truncate text-xs text-text-muted">
            {formatSharedSpaceDownloadCheckpoint(checkpoint)}
          </span>
        )}
      </span>
      <div className="flex w-full flex-wrap gap-2 sm:w-auto">
        <Button
          size="sm"
          variant="secondary"
          onClick={() => void onDownload(space)}
          disabled={busy}
        >
          <Download size={14} />
          Download
        </Button>
        <Button
          size="sm"
          variant="secondary"
          onClick={() => void onExportStatic(space)}
          disabled={busy}
        >
          <Download size={14} />
          HTML
        </Button>
        <Button
          size="sm"
          variant="secondary"
          onClick={() => void onExportStaticZip(space)}
          disabled={busy || !supportsBrowserPortableArchive() || archivePending.length>0}
        >
          <Download size={14} />
          Static ZIP
        </Button>
        <Button size="sm" variant="secondary" disabled={busy || !supportsBrowserPortableArchive() || archivePending.length>0} onClick={()=>void onSavePortable(space)}>
          <Download size={14}/> Markdown ZIP
        </Button>

      </div>
        {archivePending.length>0?<div className="w-full space-y-2 break-words text-sm text-text-muted">
          <p>Inspect archive destination files: {archivePending.map(record=>record.name).join(", ")}</p>
          <Button size="sm" variant="secondary" disabled={busy || archivePending.some(record=>!record.id)} onClick={()=>void onReviewPortable(space)}>Reviewed destination files</Button>
        </div>:null}
        {!supportsBrowserPortableArchive()?<p className="w-full text-xs text-text-muted">Markdown ZIP and Static ZIP require a browser with direct file streaming. Native archive Save is unavailable.</p>:null}
    </div>
  );
}

export function getPluginPermissionReviewClass(level: PluginPermissionReviewLevel): string {
  switch (level) {
    case "high":
      return "border-warning/30 bg-warning/10 text-warning";
    case "medium":
      return "border-accent/30 bg-accent/10 text-accent";
    case "low":
      return "border-success/30 bg-success/10 text-success";
  }
}

export function AdminJsonTextarea({
  value,
  onChange,
  readOnly = false,
  ariaLabel,
  className = "",
  placeholder,
}: {
  value: string;
  onChange?: (value: string) => void;
  readOnly?: boolean;
  ariaLabel: string;
  className?: string;
  placeholder?: string;
}) {
  return (
    <Textarea
      value={value}
      onChange={(event) => onChange?.(event.target.value)}
      readOnly={readOnly}
      spellCheck={false}
      className={`resize-y bg-surface p-3 font-mono text-xs ${className}`}
      aria-label={ariaLabel}
      placeholder={placeholder}
    />
  );
}

export function PluginJsonTextarea(props: Parameters<typeof AdminJsonTextarea>[0]) {
  return <AdminJsonTextarea {...props} />;
}

export function AdminCompactSelect({
  value,
  onChange,
  ariaLabel,
  children,
}: {
  value: string;
  onChange: (value: string) => void;
  ariaLabel: string;
  children: ReactNode;
}) {
  return (
    <Select
      value={value}
      onChange={(event) => onChange(event.target.value)}
      className="px-3 py-2 text-xs"
      aria-label={ariaLabel}
    >
      {children}
    </Select>
  );
}

function SystemSection({ localMode, onOpenPluginArchive }: { localMode: boolean; onOpenPluginArchive: () => void }) {
  const provider = getSyncProvider();
  const [configEntries, setConfigEntries] = useState<ConfigEntry[]>([]);
  const [configDialogMode, setConfigDialogMode] = useState<"create" | "edit" | null>(null);
  const [configDraftKey, setConfigDraftKey] = useState("");
  const [configDraftValue, setConfigDraftValue] = useState("");
  const [configDeleteTarget, setConfigDeleteTarget] = useState<ConfigEntry | null>(null);
  const [syncActionBusy, setSyncActionBusy] = useState(false);
  const [syncMessage, setSyncMessage] = useState<string | null>(null);
  const [syncError, setSyncError] = useState<string | null>(null);
  const [providerConnected, setProviderConnected] = useState<boolean>(false);
  const [providerReadiness, setProviderReadiness] =
    useState<LocalProviderSyncReadinessRecord | null>(null);
  const [providerRemoteContract, setProviderRemoteContract] =
    useState<ProviderRemoteContract | null>(null);
  const [providerRemoteEnablement, setProviderRemoteEnablement] =
    useState<ProviderRemoteEnablementPlan | null>(null);
  const [providerRemoteSyncEnablement, setProviderRemoteSyncEnablement] =
    useState<ProviderRemoteSyncEnablementRecord | null>(null);
  const [providerDefaultSyncPolicy, setProviderDefaultSyncPolicy] =
    useState<ProviderDefaultSyncPolicy | null>(null);
  const [providerImportAttempt, setProviderImportAttempt] =
    useState<ProviderRemoteMutationAttempt | null>(null);
  const [providerImportApplyResult, setProviderImportApplyResult] =
    useState<ProviderRemoteImportApplyExecutionResult | null>(null);
  const [providerSpaceRecoveryConfirmOpen, setProviderSpaceRecoveryConfirmOpen] = useState(false);
  const providerImportApplyReview = getProviderImportApplyReviewState(
    providerImportAttempt,
    providerImportApplyResult,
  );
  const providerSpaceRecoveryTargets = useMemo(
    () =>
      providerImportAttempt?.import_dry_run?.operations.filter(
        (operation) =>
          operation.entity_type === "space" &&
          operation.action === "delete" &&
          operation.status === "unsupported",
      ) ?? [],
    [providerImportAttempt],
  );
  const [googleDriveReadiness, setGoogleDriveReadiness] =
    useState<GoogleDriveManualConnectionReadiness | null>(() =>
      localMode && provider === "google-drive"
        ? describeGoogleDriveManualConnectionReadiness()
        : null,
    );
  const [googleDriveSetupPlan, setGoogleDriveSetupPlan] =
    useState<GoogleDriveManualSetupPlan | null>(() =>
      localMode && provider === "google-drive"
        ? buildGoogleDriveManualSetupPlan({
            clientId: readGoogleDrivePublicClientId(),
            clientSecret: readGoogleDriveBrowserClientSecret(),
            redirectUri: buildGoogleDriveRedirectUri(),
          })
        : null,
    );
  const [googleDriveBootstrap, setGoogleDriveBootstrap] =
    useState<GoogleDriveOAuthBootstrap | null>(null);
  const [googleDriveCallbackUrl, setGoogleDriveCallbackUrl] = useState("");
  const [googleDriveCallbackResult, setGoogleDriveCallbackResult] =
    useState<GoogleDriveOAuthCallbackValidationResult | null>(null);
  const [googleDriveConnectionPlan, setGoogleDriveConnectionPlan] =
    useState<GoogleDriveManualConnectionPlan | null>(null);
  const [googleDriveTokenFixture, setGoogleDriveTokenFixture] = useState("");
  const [googleDriveRootLookupFixture, setGoogleDriveRootLookupFixture] = useState("");
  const [googleDriveRootCreateFixture, setGoogleDriveRootCreateFixture] = useState("");
  const [showGoogleDriveFixtureInputs, setShowGoogleDriveFixtureInputs] = useState(false);
  const [googleDriveCompletionSummary, setGoogleDriveCompletionSummary] = useState<string | null>(null);

  useEffect(() => {
    const applyCallback = (url: string) => {
      consumePendingNativeCallback("google-drive");
      setGoogleDriveCallbackUrl(url);
      setSyncError(null);
      setSyncMessage("Google Drive OAuth callback received from the system browser.");
    };
    const receiveCallback = (event: Event) => {
      applyCallback((event as CustomEvent<string>).detail);
    };

    document.addEventListener(
      NATIVE_GOOGLE_DRIVE_CALLBACK_EVENT,
      receiveCallback,
    );
    const pendingUrl = consumePendingNativeCallback("google-drive");
    if (pendingUrl) applyCallback(pendingUrl);
    return () => {
      document.removeEventListener(
        NATIVE_GOOGLE_DRIVE_CALLBACK_EVENT,
        receiveCallback,
      );
    };
  }, []);

  const refreshProviderReadiness = useCallback(async () => {
    if (!localMode) return;
    const [status, readiness] = await Promise.all([
      dataClient.getProviderStatus(),
      dataClient.getProviderSyncReadiness(),
    ]);
    setProviderConnected(status.connected);
    setProviderRemoteContract(status.remote_contract);
    setProviderRemoteEnablement(status.remote_enablement);
    setProviderRemoteSyncEnablement(status.remote_sync_enablement);
    setProviderDefaultSyncPolicy(status.default_sync_policy);
    setProviderReadiness(readiness);
    setGoogleDriveReadiness(
      provider === "google-drive" ? describeGoogleDriveManualConnectionReadiness() : null,
    );
    setGoogleDriveSetupPlan(
      provider === "google-drive"
        ? buildGoogleDriveManualSetupPlan({
            clientId: readGoogleDrivePublicClientId(),
            clientSecret: readGoogleDriveBrowserClientSecret(),
            redirectUri: buildGoogleDriveRedirectUri(),
          })
        : null,
    );
  }, [localMode, provider]);

  useEffect(() => {
    refreshProviderReadiness().catch(() => {
      setProviderConnected(false);
      setProviderRemoteContract(null);
      setProviderRemoteEnablement(null);
      setProviderRemoteSyncEnablement(null);
      setProviderDefaultSyncPolicy(null);
      setProviderReadiness(null);
    });
  }, [refreshProviderReadiness]);

  const loadConfig = useCallback(async () => {
    if (localMode) return;
    const entries = await api.fetch<ConfigEntry[]>("/admin/config");
    setConfigEntries(entries);
  }, [localMode]);

  useEffect(() => {
    void loadConfig().catch(() => setConfigEntries([]));
  }, [loadConfig]);

  const saveConfig = async (key: string, value: string) => {
    await api.fetch("/admin/config", {
      method: "POST",
      body: { key, value },
    });
    await loadConfig();
  };

  const deleteConfig = async (key: string) => {
    await api.fetch(`/admin/config/${encodeURIComponent(key)}`, { method: "DELETE" });
    await loadConfig();
  };

  const openConfigCreate = () => {
    setConfigDraftKey("");
    setConfigDraftValue("");
    setConfigDialogMode("create");
  };

  const openConfigEdit = (entry: ConfigEntry) => {
    setConfigDraftKey(entry.key);
    setConfigDraftValue(entry.value);
    setConfigDialogMode("edit");
  };

  const saveConfigDialog = async () => {
    const key = configDraftKey.trim();
    if (!key) return;
    await saveConfig(key, configDraftValue);
    setConfigDialogMode(null);
  };

  const runSync = async (direction: "pull" | "push") => {
    setSyncActionBusy(true);
    setSyncMessage(null);
    setSyncError(null);
    try {
      const result = await dataClient.syncWithProvider(direction);
      const nextImportReview = getProviderImportReviewStateAfterSync(direction, result);
      if ("providerImportAttempt" in nextImportReview) {
        setProviderImportAttempt(nextImportReview.providerImportAttempt ?? null);
      }
      if ("providerImportApplyResult" in nextImportReview) {
        setProviderImportApplyResult(nextImportReview.providerImportApplyResult ?? null);
      }
      if (result.remote_mutation_attempt?.status === "blocked") {
        setSyncError(
          `${direction.toUpperCase()} blocked: ${result.remote_mutation_attempt.blockers.join("; ")}`,
        );
      } else if (
        direction === "pull" &&
        result.remote_mutation_attempt?.import_apply_plan
      ) {
        setSyncMessage(
          `PULL review ready: ${formatProviderImportPullReviewSummary(result.remote_mutation_attempt)}.`,
        );
      } else {
        setSyncMessage(
          `${direction.toUpperCase()} complete: pulled ${result.pulled}, pushed ${result.pushed}, conflicts ${result.conflicts}.`,
        );
      }
      await refreshProviderReadiness();
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : `${direction.toUpperCase()} failed`);
    } finally {
      setSyncActionBusy(false);
    }
  };

  const applyLastProviderImport = async () => {
    setSyncActionBusy(true);
    setSyncMessage(null);
    setSyncError(null);
    try {
      if (!providerImportAttempt?.provider_sync_manifest) {
        throw new Error("Run a provider pull that downloads a provider-sync manifest first.");
      }
      const result = await dataClient.applyProviderRemoteImportManifest(
        providerImportAttempt.provider_sync_manifest,
      );
      setProviderImportApplyResult(result);
      await refreshProviderReadiness();
      const summary = formatProviderImportApplyResultSummary(result);
      if (result.status === "completed") {
        setSyncMessage(`Provider import completed: ${summary}.`);
      } else {
        setSyncError(
          `Provider import blocked: ${summary}. ${result.blockers.join("; ") || "No operations applied."}`,
        );
      }
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Failed to apply provider import");
    } finally {
      setSyncActionBusy(false);
    }
  };

  const recoverDeletedProviderSpaces = async () => {
    if (providerSpaceRecoveryTargets.length === 0 || !providerImportAttempt?.provider_sync_manifest) return;
    setProviderSpaceRecoveryConfirmOpen(true);
  };

  const confirmRecoverDeletedProviderSpaces = async () => {
    if (providerSpaceRecoveryTargets.length === 0 || !providerImportAttempt?.provider_sync_manifest) return;
    setProviderSpaceRecoveryConfirmOpen(false);
    setSyncActionBusy(true);
    setSyncMessage(null);
    setSyncError(null);
    try {
      const result = await dataClient.applyProviderRemoteImportManifest(
        providerImportAttempt.provider_sync_manifest,
        { allowDestructiveSpaceRecovery: true },
      );
      setProviderImportApplyResult(result);
      await refreshProviderReadiness();
      const summary = formatProviderImportApplyResultSummary(result);
      if (result.status === "completed") {
        setSyncMessage(`Provider space recovery completed: ${summary}.`);
      } else {
        setSyncError(`Provider space recovery blocked: ${summary}. ${result.blockers.join("; ")}`);
      }
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Failed to recover provider-deleted space");
    } finally {
      setSyncActionBusy(false);
    }
  };

  const simulateConflict = async () => {
    setSyncActionBusy(true);
    setSyncMessage(null);
    setSyncError(null);
    try {
      await dataClient.simulateSyncConflict();
      await refreshProviderReadiness();
      setSyncMessage("Sync conflict simulated. Check top-bar sync status.");
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Failed to simulate conflict");
    } finally {
      setSyncActionBusy(false);
    }
  };

  const clearSyncIssue = async () => {
    setSyncActionBusy(true);
    setSyncMessage(null);
    setSyncError(null);
    try {
      await dataClient.clearSyncIssue();
      await refreshProviderReadiness();
      setSyncMessage("Sync issue cleared.");
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Failed to clear sync issue");
    } finally {
      setSyncActionBusy(false);
    }
  };

  const connectProvider = async () => {
    setSyncActionBusy(true);
    setSyncMessage(null);
    setSyncError(null);
    try {
      const status = await dataClient.connectProvider();
      setProviderConnected(status.connected);
      const nextImportReview = getProviderImportReviewStateAfterProviderConnectionChange();
      setProviderImportAttempt(nextImportReview.providerImportAttempt);
      setProviderImportApplyResult(nextImportReview.providerImportApplyResult);
      await refreshProviderReadiness();
      setSyncMessage("Provider connected.");
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Failed to connect provider");
    } finally {
      setSyncActionBusy(false);
    }
  };

  const disconnectProvider = async () => {
    setSyncActionBusy(true);
    setSyncMessage(null);
    setSyncError(null);
    try {
      const status = await dataClient.disconnectProvider();
      setProviderConnected(status.connected);
      const nextImportReview = getProviderImportReviewStateAfterProviderConnectionChange();
      setProviderImportAttempt(nextImportReview.providerImportAttempt);
      setProviderImportApplyResult(nextImportReview.providerImportApplyResult);
      await refreshProviderReadiness();
      setSyncMessage("Provider disconnected.");
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Failed to disconnect provider");
    } finally {
      setSyncActionBusy(false);
    }
  };

  const enableProviderRemoteSync = async () => {
    setSyncActionBusy(true);
    setSyncMessage(null);
    setSyncError(null);
    try {
      const status = await dataClient.enableProviderRemoteSync();
      setProviderRemoteContract(status.remote_contract);
      setProviderRemoteEnablement(status.remote_enablement);
      setProviderRemoteSyncEnablement(status.remote_sync_enablement);
      setProviderDefaultSyncPolicy(status.default_sync_policy);
      await refreshProviderReadiness();
      setSyncMessage("Provider remote sync explicitly enabled.");
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Failed to enable provider remote sync");
    } finally {
      setSyncActionBusy(false);
    }
  };

  const disableProviderRemoteSync = async () => {
    setSyncActionBusy(true);
    setSyncMessage(null);
    setSyncError(null);
    try {
      const status = await dataClient.disableProviderRemoteSync("disabled from Admin System");
      setProviderRemoteContract(status.remote_contract);
      setProviderRemoteEnablement(status.remote_enablement);
      setProviderRemoteSyncEnablement(status.remote_sync_enablement);
      setProviderDefaultSyncPolicy(status.default_sync_policy);
      await refreshProviderReadiness();
      setSyncMessage("Provider remote sync disabled.");
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Failed to disable provider remote sync");
    } finally {
      setSyncActionBusy(false);
    }
  };

  const resolveConflict = async (strategy: "merge" | "keep-local" | "keep-remote") => {
    setSyncActionBusy(true);
    setSyncMessage(null);
    setSyncError(null);
    try {
      await dataClient.resolveSyncConflict(strategy);
      await refreshProviderReadiness();
      setSyncMessage(`Conflict resolved using '${strategy}'.`);
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Failed to resolve conflict");
    } finally {
      setSyncActionBusy(false);
    }
  };

  const generateGoogleDriveSetupPreview = async () => {
    setSyncActionBusy(true);
    setSyncMessage(null);
    setSyncError(null);
    try {
      const bootstrap = await createGoogleDriveOAuthBootstrap();
      const plan = buildGoogleDriveManualSetupPlan({
        clientId: readGoogleDrivePublicClientId(),
        clientSecret: readGoogleDriveBrowserClientSecret(),
        redirectUri: buildGoogleDriveRedirectUri(),
        state: bootstrap.state,
        codeChallenge: bootstrap.code_challenge,
      });
      setGoogleDriveBootstrap(bootstrap);
      setGoogleDriveSetupPlan(plan);
      setGoogleDriveCallbackResult(null);
      setGoogleDriveConnectionPlan(null);
      setGoogleDriveCompletionSummary(null);
      setSyncMessage(
        plan.status === "ready_to_start"
          ? "Google Drive setup preview generated."
          : "Google Drive setup preview generated with blockers.",
      );
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Failed to generate Google Drive setup preview");
    } finally {
      setSyncActionBusy(false);
    }
  };

  const validateGoogleDriveSetupCallback = () => {
    setSyncMessage(null);
    setSyncError(null);
    if (!googleDriveBootstrap) {
      setSyncError("Generate a Google Drive setup preview before validating a callback.");
      return;
    }
    const connectionPlan = buildGoogleDriveManualConnectionPlan({
      callbackUrl: googleDriveCallbackUrl,
      expectedState: googleDriveBootstrap.state,
      clientId: readGoogleDrivePublicClientId(),
      redirectUri: buildGoogleDriveRedirectUri(),
      codeVerifier: googleDriveBootstrap.code_verifier,
    });
    setGoogleDriveCallbackResult(connectionPlan.callback_result);
    setGoogleDriveConnectionPlan(connectionPlan);
    setShowGoogleDriveFixtureInputs(false);
    setGoogleDriveCompletionSummary(null);
    setSyncMessage(
      connectionPlan.callback_result.status === "ready_for_exchange"
        ? "Google Drive callback is ready for injected token exchange."
        : "Google Drive callback validation found blockers.",
    );
  };

  const openGoogleDriveAuthorization = () => {
    setSyncMessage(null);
    setSyncError(null);
    try {
      const url = validateGoogleDriveOAuthLaunchUrl(
        googleDriveSetupPlan?.oauth_authorization_url,
      );
      const opened = window.open(url, "_blank", "noopener,noreferrer");
      if (!opened) {
        throw new Error("Google Drive OAuth window was blocked by the browser.");
      }
      setSyncMessage("Google OAuth opened. Paste the callback URL here after approval.");
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Failed to open Google OAuth");
    }
  };

  const completeGoogleDriveSetupFromFixtures = async () => {
    setSyncActionBusy(true);
    setSyncMessage(null);
    setSyncError(null);
    setGoogleDriveCompletionSummary(null);
    try {
      if (!googleDriveBootstrap) {
        throw new Error("Generate a Google Drive setup preview before completing setup.");
      }
      if (!googleDriveCallbackUrl.trim()) {
        throw new Error("Paste the Google Drive OAuth callback URL before completing setup.");
      }
      if (!googleDriveTokenFixture.trim()) {
        throw new Error("Paste a Google Drive token response fixture before completing setup.");
      }
      const tokenResponse = parseGoogleDriveTokenFixture(googleDriveTokenFixture);
      const lookupFixture = googleDriveRootLookupFixture.trim()
        ? parseJsonObject(googleDriveRootLookupFixture, "Google Drive root lookup fixture")
        : null;
      const createFixture = googleDriveRootCreateFixture.trim()
        ? parseJsonObject(googleDriveRootCreateFixture, "Google Drive root create fixture")
        : null;
      const rootTransport =
        lookupFixture || createFixture
          ? {
              fetch: async (_input: string, init: RequestInit): Promise<GoogleDriveApiResponse> => {
                if (init.method === "GET") {
                  return fixtureGoogleDriveApiResponse(lookupFixture ?? { files: [] });
                }
                if (!createFixture) {
                  return fixtureGoogleDriveApiResponse(
                    { error: "Google Drive root create fixture missing" },
                    400,
                  );
                }
                return fixtureGoogleDriveApiResponse(createFixture);
              },
            }
          : undefined;
      const result = await completeGoogleDriveManualProviderConnectionFromCallback({
        callbackUrl: googleDriveCallbackUrl,
        expectedState: googleDriveBootstrap.state,
        clientId: readGoogleDrivePublicClientId(),
        redirectUri: buildGoogleDriveRedirectUri(),
        codeVerifier: googleDriveBootstrap.code_verifier,
        tokenTransport: {
          exchangeGoogleDriveOAuthCode: async () => tokenResponse,
        },
        rootTransport,
      });
      if (result.status === "blocked") {
        throw new Error(
          result.blockers.length
            ? result.blockers.join("; ")
            : "Google Drive setup completion was blocked",
        );
      }
      setGoogleDriveCompletionSummary(
        result.connection_result?.root_result?.selection
          ? `Connected ${result.session?.auth?.account_label ?? "Google Drive"} using root ${result.connection_result.root_result.selection.name}.`
          : `Connected ${result.session?.auth?.account_label ?? "Google Drive"}.`,
      );
      await refreshProviderReadiness();
      setSyncMessage("Google Drive manual provider session persisted. Default sync remains disabled.");
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Failed to complete Google Drive setup");
    } finally {
      setSyncActionBusy(false);
    }
  };

  const completeGoogleDriveSetupWithGoogleApis = async () => {
    setSyncActionBusy(true);
    setSyncMessage(null);
    setSyncError(null);
    setGoogleDriveCompletionSummary(null);
    try {
      if (!googleDriveBootstrap) {
        throw new Error("Generate a Google Drive setup preview before completing setup.");
      }
      if (!googleDriveCallbackUrl.trim()) {
        throw new Error("Paste the Google Drive OAuth callback URL before completing setup.");
      }
      if (!googleDriveSetupPlan) {
        throw new Error("Generate a Google Drive setup preview before completing setup.");
      }
      const result = await completeGoogleDriveManualProviderConnectionFromCallback({
        callbackUrl: googleDriveCallbackUrl,
        expectedState: googleDriveBootstrap.state,
        clientId: readGoogleDrivePublicClientId(),
        redirectUri: buildGoogleDriveRedirectUri(),
        codeVerifier: googleDriveBootstrap.code_verifier,
        tokenTransport: createGoogleDriveOAuthFetchTransport(),
        rootTransport: createGoogleDriveApiFetchTransport(),
      });
      if (result.status === "blocked") {
        throw new Error(
          result.blockers.length
            ? result.blockers.join("; ")
            : "Google Drive setup completion was blocked",
        );
      }
      setGoogleDriveCompletionSummary(
        result.connection_result?.root_result?.selection
          ? `Connected ${result.session?.auth?.account_label ?? "Google Drive"} using root ${result.connection_result.root_result.selection.name}.`
          : `Connected ${result.session?.auth?.account_label ?? "Google Drive"}.`,
      );
      const verifiedAt = new Date().toISOString();
      const existingEvidence = readProviderDefaultSyncValidationEvidence("google-drive");
      const evidenceUpdate = buildGoogleDriveProductionOAuthEvidenceUpdate({
        setupPlan: googleDriveSetupPlan,
        existingEvidence,
        verifiedAt,
      });
      writeProviderDefaultSyncValidationEvidence("google-drive", evidenceUpdate.evidence);
      await refreshProviderReadiness();
      setSyncMessage(
        evidenceUpdate.productionVerifiedNow
          ? "Google Drive manual provider session persisted and production OAuth evidence recorded. Default sync remains disabled."
          : "Google Drive manual provider session persisted. This redirect is not production-validating, so default sync remains blocked.",
      );
    } catch (err) {
      setSyncError(err instanceof Error ? err.message : "Failed to complete Google Drive setup");
    } finally {
      setSyncActionBusy(false);
    }
  };

  return (
    <div>
      {providerSpaceRecoveryConfirmOpen && (
        <ConfirmDialog
          labelledBy="admin-provider-space-recovery-title"
          title="Recover provider deletion"
          message={getAdminProviderSpaceRecoveryConfirmationMessage(
            providerSpaceRecoveryTargets.length,
            providerSpaceRecoveryTargets.map((operation) => operation.entity_id).join(", "),
          )}
          confirmLabel="Recover"
          destructive
          onCancel={() => setProviderSpaceRecoveryConfirmOpen(false)}
          onConfirm={() => void confirmRecoverDeletedProviderSpaces()}
        />
      )}
      {configDeleteTarget && (
        <ConfirmDialog
          labelledBy="remove-configuration-title"
          title="Remove configuration value"
          message={`Remove the database value for ${configDeleteTarget.label}? Starlane will use its default value unless an environment variable is set.`}
          confirmLabel="Remove"
          destructive
          onCancel={() => setConfigDeleteTarget(null)}
          onConfirm={() => {
            void deleteConfig(configDeleteTarget.key).then(() => setConfigDeleteTarget(null));
          }}
        />
      )}
      {configDialogMode && (
        <DialogShell
          labelledBy="configuration-editor-title"
          maxWidth="md"
          onClose={() => setConfigDialogMode(null)}
        >
          <form
            className="flex flex-col"
            onSubmit={(event) => {
              event.preventDefault();
              void saveConfigDialog();
            }}
          >
            <div className="border-border border-b px-4 py-3 sm:px-6">
              <h2 id="configuration-editor-title" className="text-text text-heading-lg font-semibold">
                {configDialogMode === "create" ? "Add configuration" : "Edit configuration"}
              </h2>
              <p className="text-text-muted mt-1 text-sm">
                Database values apply only when the matching Docker environment variable is not set.
              </p>
            </div>
            <div className="grid gap-4 p-4 sm:p-6">
              <label>
                <span className="text-text mb-1 block text-sm font-medium">Configuration key</span>
                <span className="text-text-muted mb-2 block text-xs">
                  Use lowercase letters, numbers, dots, dashes, or underscores.
                </span>
                <AdminTextInput
                  value={configDraftKey}
                  onChange={setConfigDraftKey}
                  disabled={configDialogMode === "edit"}
                  placeholder="custom.key"
                  ariaLabel="Configuration key"
                />
              </label>
              <label>
                <span className="text-text mb-1 block text-sm font-medium">Value</span>
                <span className="text-text-muted mb-2 block text-xs">
                  Secrets are never returned after saving; enter a replacement value to change one.
                </span>
                <AdminTextInput
                  value={configDraftValue}
                  onChange={setConfigDraftValue}
                  placeholder="Value"
                  ariaLabel="Configuration value"
                />
              </label>
            </div>
            <div className="border-border flex flex-wrap justify-end gap-2 border-t px-4 py-3 sm:px-6">
              <Button type="button" variant="secondary" onClick={() => setConfigDialogMode(null)}>
                Cancel
              </Button>
              <Button type="submit" disabled={!configDraftKey.trim()}>
                {configDialogMode === "create" ? "Add setting" : "Save changes"}
              </Button>
            </div>
          </form>
        </DialogShell>
      )}
      <h1 className="text-text mb-4 text-lg font-medium">System</h1>
      <div className="border-border mb-4 space-y-2 border-b pb-4">
        <p className="text-text-muted text-sm">Retained plugin records can be inspected without enabling or executing code.</p>
        <Button variant="secondary" onClick={onOpenPluginArchive}>View legacy plugin archive</Button>
      </div>
      <div className="space-y-3">
        <div className="rounded-card border-border flex items-center justify-between border p-3">
          <span className="text-text-muted text-sm">Launch Mode</span>
          <span className="text-text text-sm">{localMode ? "Local" : "Server"}</span>
        </div>
        <div className="rounded-card border-border flex items-center justify-between border p-3">
          <span className="text-text-muted text-sm">Sync Provider</span>
          <span className="text-text text-sm">{provider}</span>
        </div>
        {!localMode && (
          <div className="rounded-card border-border border p-3">
            <div className="mb-3 flex flex-wrap items-start justify-between gap-3">
              <div>
                <p className="text-text text-sm font-medium">Application Configuration</p>
                <p className="text-text-muted text-xs">
                  Docker environment values are locked. Unset values can be managed here and persist in the database.
                </p>
              </div>
              <Button size="sm" className={RESPONSIVE_TOUCH_ACTION_CLASS} onClick={openConfigCreate}>
                Add setting
              </Button>
            </div>
            <div className="overflow-x-auto rounded-card border-border border">
              <table className="w-full min-w-[42rem] text-sm">
                <thead>
                  <tr className="border-border text-text-muted border-b text-left text-xs">
                    <th className="px-3 py-2 font-medium">Setting</th>
                    <th className="px-3 py-2 font-medium">Effective value</th>
                    <th className="px-3 py-2 font-medium">Source</th>
                    <th className="px-3 py-2 text-right font-medium">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {configEntries.map((entry) => (
                    <tr key={entry.key} className="border-border border-b last:border-0">
                      <td className="px-3 py-2">
                        <p className="text-text font-medium">{entry.label}</p>
                        <p className="text-text-muted font-mono text-xs">
                          {entry.key}{entry.env_var ? ` · ${entry.env_var}` : ""}
                        </p>
                      </td>
                      <td className="text-text-muted max-w-80 truncate px-3 py-2">
                        {entry.effective_value || "Not set"}
                      </td>
                      <td className="px-3 py-2">
                        <span className="rounded-button border-border text-text-muted inline-flex border px-2 py-1 text-xs">
                          {entry.locked ? "Docker env · locked" : entry.source}
                        </span>
                      </td>
                      <td className="px-3 py-2">
                        <div className="flex justify-end gap-2">
                          <Button
                            size="sm"
                            variant="ghost"
                            className={RESPONSIVE_TOUCH_ACTION_CLASS}
                            disabled={entry.locked}
                            onClick={() => openConfigEdit(entry)}
                          >
                            <Pencil size={14} aria-hidden="true" />
                            Edit
                          </Button>
                          <Button
                            size="sm"
                            variant="ghost"
                            className={RESPONSIVE_TOUCH_ACTION_CLASS}
                            disabled={entry.locked || entry.key === "setup_completed"}
                            onClick={() => setConfigDeleteTarget(entry)}
                          >
                            Remove
                          </Button>
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
        {localMode && (
          <div className="rounded-card border-border flex items-center justify-between border p-3">
            <span className="text-text-muted text-sm">Provider Connection</span>
            <span className="text-text text-sm">
              {providerConnected ? "Connected" : "Disconnected"}
            </span>
          </div>
        )}
        {localMode && (
          <div className="rounded-card border-border border p-3">
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="text-text text-sm font-medium">Provider Readiness</p>
                <p className="text-text-muted mt-1 text-xs">
                  Checks provider connection, queued replay payload readiness, and unresolved conflicts before provider sync runs.
                </p>
              </div>
              <span
                className={`shrink-0 rounded px-2 py-1 text-xs ${
                  getAdminProviderReadinessBadgeClass(providerReadiness?.ready)
                }`}
              >
                {providerReadiness?.ready ? "Ready" : "Blocked"}
              </span>
            </div>
            {providerReadiness ? (
              <div className="text-text-muted mt-3 grid gap-2 text-xs md:grid-cols-4">
                <span>{providerReadiness.queued_operation_count} queued writes</span>
                <span>{providerReadiness.ready_replay_operation_count} replay-ready</span>
                <span>{providerReadiness.missing_payload_count} missing payloads</span>
                <span>{providerReadiness.pending_conflict_count} conflicts</span>
              </div>
            ) : (
              <p className="text-text-muted mt-3 text-xs">Readiness has not been checked yet.</p>
            )}
            {providerReadiness && providerReadiness.blockers.length > 0 ? (
              <div className="mt-2 space-y-1">
                {providerReadiness.blockers.slice(0, 3).map((blocker) => (
                  <p key={blocker} className={ADMIN_PROVIDER_BLOCKER_TEXT_CLASS}>
                    {blocker}
                  </p>
                ))}
              </div>
            ) : null}
          </div>
        )}
        {localMode && (
          <div className="rounded-card border-border border p-3">
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="text-text text-sm font-medium">Provider Remote Contract</p>
                <p className="text-text-muted mt-1 text-xs">
                  Remote provider writes stay behind explicit root, auth, write, and conflict boundaries. Default provider sync is still disabled.
                </p>
              </div>
              <span className="text-text-muted ring-border shrink-0 rounded px-2 py-1 text-xs ring-1">
                {providerRemoteContract?.status.replace(/_/g, " ") ?? "not checked"}
              </span>
            </div>
            {providerRemoteContract ? (
              <div className="mt-3 space-y-3">
                <div className="text-text-muted grid gap-2 text-xs md:grid-cols-2">
                  <span>Root: {providerRemoteContract.root_strategy}</span>
                  <span>Auth: {providerRemoteContract.auth_model}</span>
                  <span>Writes: {providerRemoteContract.write_model}</span>
                  <span>Conflicts: {providerRemoteContract.conflict_model}</span>
                </div>
                {providerReadiness?.remote_mutation_plans.length ? (
                  <div className="space-y-1">
                    {providerReadiness.remote_mutation_plans.map((plan) => (
                      <div
                        key={`${plan.provider}-${plan.direction}`}
                        className="text-text-muted flex items-center justify-between gap-3 text-xs"
                      >
                        <span>{plan.direction === "pull" ? "Pull" : "Push"} mutation</span>
                        <span>{plan.status.replace(/_/g, " ")}</span>
                      </div>
                    ))}
                  </div>
                ) : null}
              </div>
            ) : (
              <p className="text-text-muted mt-3 text-xs">
                Remote contract details load after provider status is checked.
              </p>
            )}
            <div className="border-border mt-3 border-t pt-3">
              <div className="flex items-center justify-between gap-3">
                <p className="text-text text-xs font-medium">Remote Enablement Plan</p>
                <span className="text-text-muted text-xs">
                  {providerRemoteEnablement?.status.replace(/_/g, " ") ?? "not checked"}
                </span>
              </div>
              {providerRemoteEnablement ? (
                <>
                  <div className="text-text-muted mt-2 grid gap-2 text-xs md:grid-cols-2">
                    <span>Transport: {providerRemoteEnablement.transport_kind}</span>
                    <span>Boundary: {providerRemoteEnablement.enablement_boundary.replace(/_/g, " ")}</span>
                    <span>
                      Remote sync: {formatProviderRemoteSyncEnablementLabel(providerRemoteSyncEnablement)}
                    </span>
                    <span>
                      Default sync policy:{" "}
                      {providerDefaultSyncPolicy?.status.replace(/_/g, " ") ?? "not checked"}
                    </span>
                    <span>
                      Auto enable:{" "}
                      {providerDefaultSyncPolicy?.automatic_enablement_allowed ? "allowed" : "blocked"}
                    </span>
                    <span>Required scopes: {providerRemoteEnablement.required_scopes.join(", ") || "none"}</span>
                  </div>
                  {providerRemoteEnablement.blockers.length > 0 ? (
                    <div className="mt-2 space-y-1">
                      {providerRemoteEnablement.blockers.slice(0, 3).map((blocker) => (
                        <p key={blocker} className={ADMIN_PROVIDER_BLOCKER_TEXT_CLASS}>
                          {blocker}
                        </p>
                      ))}
                      {formatHiddenBlockerCount(providerRemoteEnablement.blockers.length) ? (
                        <p className="text-xs text-text-muted">
                          {formatHiddenBlockerCount(providerRemoteEnablement.blockers.length)}
                        </p>
                      ) : null}
                    </div>
                  ) : null}
                  {providerDefaultSyncPolicy && providerDefaultSyncPolicy.blockers.length > 0 ? (
                    <div className="mt-2 space-y-1">
                      {providerDefaultSyncPolicy.blockers.slice(0, 2).map((blocker) => (
                        <p key={blocker} className={ADMIN_PROVIDER_BLOCKER_TEXT_CLASS}>
                          {blocker}
                        </p>
                      ))}
                      {formatHiddenBlockerCount(providerDefaultSyncPolicy.blockers.length, 2) ? (
                        <p className="text-xs text-text-muted">
                          {formatHiddenBlockerCount(providerDefaultSyncPolicy.blockers.length, 2)}
                        </p>
                      ) : null}
                    </div>
                  ) : null}
                  {providerDefaultSyncPolicy && providerDefaultSyncPolicy.readiness_gates.length > 0 ? (
                    <div className="text-text-muted mt-2 grid gap-1 text-xs md:grid-cols-2">
                      {providerDefaultSyncPolicy.readiness_gates.map((gate) => (
                        <span key={gate.gate}>
                          {gate.gate.replace(/_/g, " ")}: {gate.passed ? "ready" : "blocked"}
                        </span>
                      ))}
                    </div>
                  ) : null}
                  {providerDefaultSyncPolicy ? (
                    <div className="mt-2 space-y-1">
                      <div className="text-text-muted grid gap-1 text-xs md:grid-cols-2">
                        <span>
                          Production OAuth:{" "}
                          {formatProviderValidationEvidenceLabel(
                            providerDefaultSyncPolicy.validation_evidence.production_oauth_verified,
                            providerDefaultSyncPolicy.validation_evidence.production_oauth_verified_at,
                          )}
                        </span>
                        <span>
                          Device background wake:{" "}
                          {formatProviderValidationEvidenceLabel(
                            providerDefaultSyncPolicy.validation_evidence.device_background_wake_verified,
                            providerDefaultSyncPolicy.validation_evidence.device_background_wake_verified_at,
                          )}
                        </span>
                      </div>
                      {providerDefaultSyncPolicy.validation_evidence.notes.length > 0 ? (
                        <div className="space-y-1">
                          {providerDefaultSyncPolicy.validation_evidence.notes.slice(0, 2).map((note) => (
                            <p key={note} className="text-xs text-text-muted">
                              {note}
                            </p>
                          ))}
                        </div>
                      ) : null}
                    </div>
                  ) : null}
                  <div className="mt-3 flex flex-wrap gap-2">
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={
                        syncActionBusy ||
                        providerRemoteEnablement.status !== "ready_to_enable" ||
                        providerRemoteSyncEnablement?.enabled === true
                      }
                      onClick={() => {
                        void enableProviderRemoteSync();
                      }}
                    >
                      Enable Remote Sync
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={syncActionBusy || providerRemoteSyncEnablement?.enabled !== true}
                      onClick={() => {
                        void disableProviderRemoteSync();
                      }}
                    >
                      Disable Remote Sync
                    </Button>
                  </div>
                </>
              ) : (
                <p className="text-text-muted mt-2 text-xs">
                  Remote enablement details load after provider status is checked.
                </p>
              )}
            </div>
          </div>
        )}
        {localMode && provider === "google-drive" && (
          <div className="rounded-card border-border border p-3">
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="text-text text-sm font-medium">Google Drive Manual Connection</p>
                <p className="text-text-muted mt-1 text-xs">
                  OAuth exchange and Drive root setup can run only through explicit injected transports; this panel does not start OAuth or enable automatic provider sync.
                </p>
              </div>
              <span
                className={`shrink-0 rounded px-2 py-1 text-xs ${
                  getAdminProviderReadinessBadgeClass(
                    googleDriveReadiness?.ready_for_explicit_api_transport,
                  )
                }`}
              >
                {googleDriveReadiness?.ready_for_explicit_api_transport
                  ? "API ready"
                  : "Setup needed"}
              </span>
            </div>
            {googleDriveReadiness ? (
              <div className="mt-3 space-y-3">
                <div className="text-text-muted grid gap-2 text-xs md:grid-cols-4">
                  <span>OAuth exchange boundary: ready</span>
                  <span>Root execution boundary: ready</span>
                  <span>Default sync: disabled</span>
                  <span>Manual OAuth session: {googleDriveReadiness.manual_session ? "ready" : "needed"}</span>
                  <span>Refresh token: {googleDriveReadiness.refresh_token_ready ? "ready" : "needed"}</span>
                  <span>Required scopes: {googleDriveReadiness.required_scopes_ready ? "ready" : "needed"}</span>
                  <span>Selected root: {googleDriveReadiness.selected_root_ready ? "ready" : "needed"}</span>
                </div>
                {googleDriveReadiness.blockers.length > 0 ? (
                  <div className="space-y-1">
                    {googleDriveReadiness.blockers.slice(0, 3).map((blocker) => (
                      <p key={blocker} className={ADMIN_PROVIDER_BLOCKER_TEXT_CLASS}>
                        {blocker}
                      </p>
                    ))}
                    {formatHiddenBlockerCount(googleDriveReadiness.blockers.length) ? (
                      <p className="text-xs text-text-muted">
                        {formatHiddenBlockerCount(googleDriveReadiness.blockers.length)}
                      </p>
                    ) : null}
                  </div>
                ) : null}
              </div>
            ) : (
              <p className="text-text-muted mt-3 text-xs">
                Google Drive readiness loads after provider status is checked.
              </p>
            )}
            {googleDriveSetupPlan ? (
              <div className="border-border mt-3 border-t pt-3">
                {(() => {
                  const setupStepStates = getGoogleDriveManualSetupStepStates({
                    setupPlanStatus: googleDriveSetupPlan.status,
                    hasBootstrap: Boolean(googleDriveBootstrap),
                    callbackStatus: googleDriveCallbackResult?.status,
                    connectionPlanStatus: googleDriveConnectionPlan?.status,
                  });
                  return (
                    <div className="flex items-center justify-between gap-3">
                      <p className="text-text text-xs font-medium">Manual Setup Plan</p>
                      <div className="text-right">
                        <span className="block text-xs text-text-muted">
                          {googleDriveSetupPlan.status === "ready_to_start" ? "OAuth-ready" : "Blocked"}
                        </span>
                        <span className="block text-xs text-text">
                          Next: {getGoogleDriveManualSetupNextStepLabel(setupStepStates)}
                        </span>
                      </div>
                    </div>
                  );
                })()}
                <div className="text-text-muted mt-2 grid gap-2 text-xs md:grid-cols-2">
                  <span>OAuth client: public PKCE</span>
                  <span>Public client ID: {googleDriveSetupPlan.client_id_configured ? "configured" : "missing"}</span>
                  <span>
                    Client secret:{" "}
                    {googleDriveSetupPlan.client_secret_configured ? "blocked in browser" : "not configured"}
                  </span>
                  <span>Redirect URI: {googleDriveSetupPlan.redirect_uri_configured ? "configured" : "missing"}</span>
                  <span>
                    Redirect environment:{" "}
                    {formatGoogleDriveOAuthRedirectEnvironment(
                      googleDriveSetupPlan.oauth_redirect_environment,
                    )}
                  </span>
                  <span>
                    Production candidate:{" "}
                    {googleDriveSetupPlan.production_oauth_candidate ? "yes" : "no"}
                  </span>
                  <span>
                    Production evidence:{" "}
                    {formatGoogleDriveProductionOAuthEvidenceAction(googleDriveSetupPlan)}
                  </span>
                  <span>Callback handling: state-validated request</span>
                  <span>Token exchange: injected transport without secret</span>
                  <span>Root setup: injected API transport</span>
                  <span>Session persistence: provider record</span>
                  <span>Default sync: disabled</span>
                </div>
                <ol className="border-border mt-3 grid gap-2 border-t pt-3 text-xs text-text-muted md:grid-cols-2">
                  {[
                    {
                      label: "1. Generate preview",
                      description: "Build a state-bound PKCE authorization URL.",
                    },
                    {
                      label: "2. Open Google OAuth",
                      description: "Approve access, then paste the callback URL.",
                    },
                    {
                      label: "3. Validate callback",
                      description: "Confirm state and authorization code before exchange.",
                    },
                    {
                      label: "4. Complete connection",
                      description:
                        "Exchange tokens, select/create the Drive root, and persist the manual session.",
                    },
                  ].map((step, index) => {
                    const state = getGoogleDriveManualSetupStepStates({
                      setupPlanStatus: googleDriveSetupPlan.status,
                      hasBootstrap: Boolean(googleDriveBootstrap),
                      callbackStatus: googleDriveCallbackResult?.status,
                      connectionPlanStatus: googleDriveConnectionPlan?.status,
                    })[index] ?? "pending";
                    return (
                      <li key={step.label} className="rounded-input border-border border px-2 py-1.5">
                        <div className="flex items-center justify-between gap-2">
                          <span className="font-medium text-text">{step.label}</span>
                          <span className="rounded bg-surface-raised px-1.5 py-0.5 text-xs">
                            {formatGoogleDriveManualSetupStepState(state)}
                          </span>
                        </div>
                        <span className="mt-0.5 block">{step.description}</span>
                      </li>
                    );
                  })}
                </ol>
                <div className="mt-3 flex flex-wrap gap-2">
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={syncActionBusy}
                    onClick={() => {
                      void generateGoogleDriveSetupPreview();
                    }}
                  >
                    Generate Setup Preview
                  </Button>
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={syncActionBusy || !googleDriveSetupPlan.oauth_authorization_url}
                    onClick={openGoogleDriveAuthorization}
                  >
                    Open Google OAuth
                  </Button>
                </div>
                {googleDriveSetupPlan.blockers.length > 0 ? (
                  <div className="mt-2 space-y-1">
                    {googleDriveSetupPlan.blockers.slice(0, 3).map((blocker) => (
                      <p key={blocker} className={ADMIN_PROVIDER_BLOCKER_TEXT_CLASS}>
                        {blocker}
                      </p>
                    ))}
                    {formatHiddenBlockerCount(googleDriveSetupPlan.blockers.length) ? (
                      <p className="text-xs text-text-muted">
                        {formatHiddenBlockerCount(googleDriveSetupPlan.blockers.length)}
                      </p>
                    ) : null}
                  </div>
                ) : null}
                {googleDriveBootstrap ? (
                  <div className="text-text-muted mt-2 space-y-1 text-xs">
                    <p>PKCE challenge generated at {googleDriveBootstrap.created_at}.</p>
                    {googleDriveSetupPlan.oauth_authorization_url ? (
                      <p className="break-all font-mono">
                        {googleDriveSetupPlan.oauth_authorization_url}
                      </p>
                    ) : null}
                  </div>
                ) : null}
                <div className="mt-3 grid gap-2 lg:grid-cols-[minmax(0,1fr)_auto]">
                  <AdminTextInput
                    value={googleDriveCallbackUrl}
                    onChange={setGoogleDriveCallbackUrl}
                    placeholder="starlane://provider/google-drive/callback?code=..."
                    className="h-8 text-xs"
                    ariaLabel="Google Drive OAuth callback URL"
                    spellCheck={false}
                  />
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={syncActionBusy || !googleDriveBootstrap}
                    onClick={validateGoogleDriveSetupCallback}
                  >
                    Validate Callback
                  </Button>
                </div>
                {googleDriveCallbackResult ? (
                  <div className="text-text-muted mt-2 space-y-1 text-xs">
                    <p>
                      Callback validation:{" "}
                      {googleDriveCallbackResult.status === "ready_for_exchange"
                        ? "ready for injected exchange"
                        : "blocked"}
                    </p>
                    {googleDriveCallbackResult.blockers.slice(0, 3).map((blocker) => (
                      <p key={blocker} className={ADMIN_PROVIDER_BLOCKER_TEXT_CLASS}>
                        {blocker}
                      </p>
                    ))}
                    {formatHiddenBlockerCount(googleDriveCallbackResult.blockers.length) ? (
                      <p className="text-xs text-text-muted">
                        {formatHiddenBlockerCount(googleDriveCallbackResult.blockers.length)}
                      </p>
                    ) : null}
                  </div>
                ) : null}
                {googleDriveConnectionPlan ? (
                  <div className="border-border mt-3 border-t pt-3">
                    <div className="flex items-center justify-between gap-3">
                      <p className="text-text text-xs font-medium">Manual Completion Plan</p>
                      <span className="text-text-muted text-xs">
                        {googleDriveConnectionPlan.status === "ready_to_complete"
                          ? "Ready"
                          : "Blocked"}
                      </span>
                    </div>
                    <div className="text-text-muted mt-2 grid gap-2 text-xs md:grid-cols-2">
                      <span>Token exchange: injected transport</span>
                      <span>Root setup: injected API transport</span>
                      <span>Persistence: provider session record</span>
                      <span>
                        Production evidence:{" "}
                        {formatGoogleDriveProductionOAuthEvidenceAction(googleDriveSetupPlan)}
                      </span>
                      <span>Default sync: disabled</span>
                    </div>
                    {googleDriveConnectionPlan.blockers.length > 0 ? (
                      <div className="mt-2 space-y-1">
                        {googleDriveConnectionPlan.blockers.slice(0, 3).map((blocker) => (
                          <p key={blocker} className={ADMIN_PROVIDER_BLOCKER_TEXT_CLASS}>
                            {blocker}
                          </p>
                        ))}
                        {formatHiddenBlockerCount(googleDriveConnectionPlan.blockers.length) ? (
                          <p className="text-xs text-text-muted">
                            {formatHiddenBlockerCount(googleDriveConnectionPlan.blockers.length)}
                          </p>
                        ) : null}
                      </div>
                    ) : null}
                    <div className="mt-3 grid gap-2">
                      {googleDriveCompletionSummary ? (
                        <p className={ADMIN_PROVIDER_SUCCESS_TEXT_CLASS}>
                          {googleDriveCompletionSummary}
                        </p>
                      ) : null}
                      <div className="flex flex-wrap gap-2">
                        <Button
                          size="sm"
                          variant="secondary"
                          disabled={
                            syncActionBusy ||
                            googleDriveConnectionPlan.status !== "ready_to_complete"
                          }
                          onClick={() => {
                            void completeGoogleDriveSetupWithGoogleApis();
                          }}
                        >
                          Complete With Google APIs
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          onClick={() => {
                            setShowGoogleDriveFixtureInputs((value) => !value);
                          }}
                        >
                          {getGoogleDriveFixtureDisclosureLabel(showGoogleDriveFixtureInputs)}
                        </Button>
                        {showGoogleDriveFixtureInputs ? (
                          <Button
                            size="sm"
                            variant="ghost"
                            disabled={
                              syncActionBusy ||
                              googleDriveConnectionPlan.status !== "ready_to_complete" ||
                              !googleDriveTokenFixture.trim()
                            }
                            onClick={() => {
                              void completeGoogleDriveSetupFromFixtures();
                            }}
                          >
                            Complete From Fixtures
                          </Button>
                        ) : null}
                      </div>
                      {showGoogleDriveFixtureInputs ? (
                        <div className="grid gap-2">
                          <AdminJsonTextarea
                            value={googleDriveTokenFixture}
                            onChange={setGoogleDriveTokenFixture}
                            placeholder='{"access_token":"...","refresh_token":"...","expires_in":3600,"scope":"drive.file drive.metadata.readonly offline_access","account_label":"name@example.com"}'
                            className="min-h-20 px-2 py-2"
                            ariaLabel="Google Drive token response fixture"
                          />
                          <div className="grid gap-2 md:grid-cols-2">
                            <AdminJsonTextarea
                              value={googleDriveRootLookupFixture}
                              onChange={setGoogleDriveRootLookupFixture}
                              placeholder='{"files":[]}'
                              className="min-h-16 px-2 py-2"
                              ariaLabel="Google Drive root lookup fixture"
                            />
                            <AdminJsonTextarea
                              value={googleDriveRootCreateFixture}
                              onChange={setGoogleDriveRootCreateFixture}
                              placeholder='{"id":"drive-root-id","name":"Starlane Notes"}'
                              className="min-h-16 px-2 py-2"
                              ariaLabel="Google Drive root create fixture"
                            />
                          </div>
                        </div>
                      ) : null}
                      {showGoogleDriveFixtureInputs ? (
                        <p className="text-xs text-text-muted">
                          Fixture completion is for injected transport validation. Live Google APIs
                          remain the primary setup path.
                        </p>
                      ) : null}
                    </div>
                  </div>
                ) : null}
              </div>
            ) : null}
          </div>
        )}
        {localMode && (
          <div className="rounded-card border-border border p-3">
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="text-text text-sm font-medium">Provider Remote Import Review</p>
                <p className="text-text-muted mt-1 text-xs">
                  Latest provider pull review, dry-run, and explicit local apply results.
                </p>
              </div>
              <span className="text-text-muted ring-border shrink-0 rounded px-2 py-1 text-xs ring-1">
                {providerImportAttempt?.status.replace(/_/g, " ") ?? "not pulled"}
              </span>
            </div>
            {providerImportAttempt ? (
              <div className="mt-3 space-y-3">
                <div className="text-text-muted grid gap-2 text-xs md:grid-cols-4">
                  <span>
                    Discovery: {providerImportAttempt.pull_discovery?.status ?? "not checked"}
                  </span>
                  <span>
                    Preflight: {providerImportAttempt.import_preflight?.status ?? "not checked"}
                  </span>
                  <span>
                    Dry run:{" "}
                    {providerImportAttempt.import_dry_run
                      ? formatProviderImportPullReviewSummary({
                          import_apply_plan: providerImportAttempt.import_apply_plan,
                          import_dry_run: providerImportAttempt.import_dry_run,
                        })
                      : "not checked"}
                  </span>
                  <span>
                    Apply plan:{" "}
                    {formatProviderImportApplyPlanSummary(
                      providerImportAttempt.import_apply_plan,
                    )}
                  </span>
                </div>
                <p className="break-all text-xs text-text-muted">
                  Review source: {formatProviderImportAttemptProvenance(providerImportAttempt)}
                </p>
                {providerImportAttempt.import_preflight?.blockers.length ? (
                  <div className="space-y-1">
                    {providerImportAttempt.import_preflight.blockers.slice(0, 3).map((blocker) => (
                      <p key={blocker} className={ADMIN_PROVIDER_BLOCKER_TEXT_CLASS}>
                        {blocker}
                      </p>
                    ))}
                    {formatProviderImportHiddenBlockerCount(
                      providerImportAttempt.import_preflight.blockers.length,
                    ) ? (
                      <p className="text-xs text-text-muted">
                        {formatProviderImportHiddenBlockerCount(
                          providerImportAttempt.import_preflight.blockers.length,
                        )}
                      </p>
                    ) : null}
                  </div>
                ) : null}
                {providerImportAttempt.import_dry_run ? (
                  <>
                    <div className="text-text-muted grid gap-2 text-xs md:grid-cols-4">
                      <span>{providerImportAttempt.import_dry_run.operation_count} remote ops</span>
                      <span>{providerImportAttempt.import_dry_run.failed_count} failed</span>
                      <span>
                        {formatProviderImportUnsupportedSummary(
                          providerImportAttempt.import_dry_run.unsupported_count,
                          providerImportAttempt.import_dry_run.operations,
                        )}
                      </span>
                      <span>{providerImportAttempt.import_dry_run.conflict_count} conflicts</span>
                    </div>
                    <ProviderImportOperationReviewList
                      title="Dry-run outcomes"
                      operations={providerImportAttempt.import_dry_run.operations}
                    />
                  </>
                ) : null}
                <ProviderImportOperationReviewList
                  title="Apply-plan decisions"
                  operations={providerImportAttempt.import_apply_plan?.operations ?? []}
                />
                {providerImportApplyResult ? (
                  <>
                    <div className="text-text-muted grid gap-2 text-xs md:grid-cols-6">
                      <span>Apply: {providerImportApplyResult.status}</span>
                      <span>{providerImportApplyResult.attempted_operation_count} attempted</span>
                      <span>{providerImportApplyResult.applied_operation_count} applied</span>
                      <span>{providerImportApplyResult.skipped_operation_count} skipped</span>
                      <span>{providerImportApplyResult.failed_operation_count} failed</span>
                      <span>
                        {providerImportApplyResult.blocked_operation_count ??
                          getProviderImportApplyBlockedOperationCount(providerImportApplyResult.results)}{" "}
                        blocked
                      </span>
                    </div>
                    {providerImportApplyResult.blockers.length > 0 ? (
                      <div className="space-y-1">
                        {providerImportApplyResult.blockers.slice(0, 3).map((blocker) => (
                          <p key={blocker} className={ADMIN_PROVIDER_BLOCKER_TEXT_CLASS}>
                            {blocker}
                          </p>
                        ))}
                        {formatProviderImportHiddenBlockerCount(
                          providerImportApplyResult.blockers.length,
                        ) ? (
                          <p className="text-xs text-text-muted">
                            {formatProviderImportHiddenBlockerCount(
                              providerImportApplyResult.blockers.length,
                            )}
                          </p>
                        ) : null}
                      </div>
                    ) : null}
                    <ProviderImportOperationReviewList
                      title="Apply execution results"
                      operations={providerImportApplyResult.results}
                    />
                  </>
                ) : null}
                <p
                  className={
                    providerImportApplyReview.canApply
                      ? ADMIN_PROVIDER_SUCCESS_TEXT_CLASS
                      : "text-xs text-text-muted"
                  }
                >
                  Apply readiness: {providerImportApplyReview.message}
                </p>
                <p className="text-xs text-text-muted">
                  Next import action:{" "}
                  {getProviderImportNextActionLabel(
                    providerImportAttempt,
                    providerImportApplyResult,
                  )}
                </p>
                <div className="flex flex-wrap gap-2">
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={syncActionBusy}
                    onClick={() => {
                      void runSync("pull");
                    }}
                  >
                    Run Test Pull
                  </Button>
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={syncActionBusy || !providerImportApplyReview.canApply}
                    onClick={() => {
                      void applyLastProviderImport();
                    }}
                  >
                    Apply Last Import
                  </Button>
                  {(providerImportAttempt.import_dry_run?.operations ?? []).some(
                    (operation) =>
                      operation.entity_type === "space" &&
                      operation.action === "delete" &&
                      operation.status === "unsupported",
                  ) ? (
                    <Button
                      size="sm"
                      variant="danger"
                      disabled={syncActionBusy}
                      onClick={() => {
                        void recoverDeletedProviderSpaces();
                      }}
                    >
                      Recover Deleted Space
                    </Button>
                  ) : null}
                </div>
              </div>
            ) : (
              <div className="mt-3 space-y-3">
                <p className="text-text-muted text-xs">
                  {providerImportApplyReview.message}
                </p>
                <p className="text-xs text-text-muted">
                  Next import action: {getProviderImportNextActionLabel(providerImportAttempt)}
                </p>
                <div className="flex flex-wrap gap-2">
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={syncActionBusy}
                    onClick={() => {
                      void runSync("pull");
                    }}
                  >
                    Run Test Pull
                  </Button>
                  <Button size="sm" variant="secondary" disabled>
                    Apply Last Import
                  </Button>
                </div>
              </div>
            )}
          </div>
        )}
        {localMode && (
          <div className="rounded-card border-border border p-3">
            <p className="text-text mb-2 text-sm">Local Sync Controls</p>
            <p className="text-text-muted mb-3 text-xs">
              Use these controls to exercise provider sync contracts and conflict-state behavior.
            </p>
            <div className="flex flex-wrap gap-2">
              <Button
                size="sm"
                variant="secondary"
                disabled={syncActionBusy}
                onClick={() => {
                  void connectProvider();
                }}
              >
                Connect Provider
              </Button>
              <Button
                size="sm"
                variant="secondary"
                disabled={syncActionBusy}
                onClick={() => {
                  void disconnectProvider();
                }}
              >
                Disconnect Provider
              </Button>
              <Button
                size="sm"
                variant="secondary"
                disabled={syncActionBusy}
                onClick={() => {
                  void runSync("pull");
                }}
              >
                Test Pull
              </Button>
              <Button
                size="sm"
                variant="secondary"
                disabled={syncActionBusy}
                onClick={() => {
                  void runSync("push");
                }}
              >
                Test Push
              </Button>
              <Button
                size="sm"
                variant="secondary"
                disabled={syncActionBusy}
                onClick={() => {
                  void simulateConflict();
                }}
              >
                Simulate Conflict
              </Button>
              <Button
                size="sm"
                variant="secondary"
                disabled={syncActionBusy}
                onClick={() => {
                  void clearSyncIssue();
                }}
              >
                Clear Sync State
              </Button>
              <Button
                size="sm"
                variant="secondary"
                disabled={syncActionBusy}
                onClick={() => {
                  void resolveConflict("merge");
                }}
              >
                Resolve: Merge
              </Button>
              <Button
                size="sm"
                variant="secondary"
                disabled={syncActionBusy}
                onClick={() => {
                  void resolveConflict("keep-local");
                }}
              >
                Resolve: Keep Local
              </Button>
              <Button
                size="sm"
                variant="secondary"
                disabled={syncActionBusy}
                onClick={() => {
                  void resolveConflict("keep-remote");
                }}
              >
                Resolve: Keep Remote
              </Button>
            </div>
            {syncMessage && (
              <div className="rounded-card border-border text-text-muted mt-3 border px-3 py-2 text-xs">
                {syncMessage}
              </div>
            )}
            {syncError && (
              <div className={ADMIN_LOCAL_SYNC_ERROR_CARD_CLASS}>{syncError}</div>
            )}
          </div>
        )}
        <div className="rounded-card border-border flex items-center justify-between border p-3">
          <span className="text-text-muted text-sm">Version</span>
          <span className="text-text text-sm">{STARLANE_VERSION}</span>
        </div>
        <div className="rounded-card border-border flex items-center justify-between border p-3">
          <span className="text-text-muted text-sm">Database</span>
          <span className="text-text text-sm">{localMode ? "SQLite" : "Server-managed"}</span>
        </div>
        <div className="rounded-card border-border flex items-center justify-between border p-3">
          <div>
            <span className="text-text-muted text-sm">API Documentation</span>
          </div>
          <a href="/api/docs" className="text-primary text-sm hover:underline">
            Open Scalar
          </a>
        </div>
      </div>
    </div>
  );
}
