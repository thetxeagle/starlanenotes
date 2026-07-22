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
  Puzzle,
  ArrowLeft,
  ChevronRight,
  ChevronLeft,
} from "lucide-react";
import { api, getApiBase } from "@/lib/api";
import { useAuth, type User } from "@/hooks/useAuth";
import { useRuntimePlatform } from "@/hooks/useRuntimePlatform";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/components/ui/confirm-dialog";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import { getCurrentLaunchMode } from "@/lib/launchMode";
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
import {
  summarizePluginPermissionReview,
  validatePluginManifest,
  validatePluginRegistryIndex,
  type PluginManifestValidationResult,
  type PluginPermissionReviewLevel,
  type PluginRegistryIndexEntry,
  type PluginRegistryIndexValidationResult,
} from "@/lib/pluginManifest";
import {
  appendPluginInstallAuditEvent,
  appendPluginRuntimeAuditEvent,
  filterPluginRuntimeAuditEvents,
  formatPluginInstallAuditAction,
  formatPluginInstallSourceDetails,
  formatPluginRuntimeAuditAction,
  getPluginInstallSourceDetailsForManifest,
  getPluginRuntimeStatus,
  readPluginInstallAuditEvents,
  readPluginInstallRecords,
  readPluginRuntimeAuditEvents,
  removePluginInstallRecord,
  setPluginInstallRecordEnabled,
  setPluginInstallRecordSpaceEnabled,
  serializePluginRuntimeAuditEvents,
  upsertPluginInstallRecord,
  writePluginInstallAuditEvents,
  writePluginInstallRecords,
  writePluginRuntimeAuditEvents,
  type PluginInstallAuditEvent,
  type PluginInstallRecord,
  type PluginRuntimeAuditEvent,
} from "@/lib/pluginInstallRecords";
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
  chooseVaultArchiveExportPath,
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
  created_at: string;
  updated_at: string;
}

interface SpaceGroupItem {
  group_id: string;
  name: string;
  description?: string;
  role: string;
  member_count: number;
  granted_at: string;
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
  space_id?: string;
  is_active: boolean;
  created_at: string;
}

export interface WebhookDeliveryItem {
  id: string;
  event_type: string;
  response_status?: number;
  success: boolean;
  attempt_number: number;
  delivered_at: string;
}

interface AdminWebhookDeliveryHistoryTableProps {
  deliveries: WebhookDeliveryItem[];
  localMode: boolean;
  redeliveringDeliveryId: string | null;
  redeliveryError: string | null;
  onRedeliver: (deliveryId: string) => void;
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

interface ApiKeyItem {
  id: string;
  name: string;
  key_prefix: string;
  is_service_account: boolean;
  scopes: string[];
  last_used_at?: string;
  expires_at?: string;
  created_at: string;
}

interface ApiKeyCreateResult extends ApiKeyItem {
  key: string;
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

const sections: { id: AdminSection; label: string; icon: React.ReactNode }[] = [
  { id: "users", label: "Users", icon: <Users size={16} /> },
  { id: "spaces", label: "Spaces", icon: <Globe size={16} /> },
  { id: "groups", label: "Groups", icon: <UserRoundPlus size={16} /> },
  { id: "auth", label: "Authentication", icon: <Shield size={16} /> },
  { id: "api-keys", label: "API Keys", icon: <Key size={16} /> },
  { id: "webhooks", label: "Webhooks", icon: <Webhook size={16} /> },
  { id: "ai", label: "AI Settings", icon: <Sparkles size={16} /> },
  { id: "audit", label: "Audit Log", icon: <FileText size={16} /> },
  { id: "notifications", label: "Notifications", icon: <Bell size={16} /> },
  { id: "storage", label: "Storage", icon: <HardDrive size={16} /> },
  { id: "search", label: "Search", icon: <Search size={16} /> },
  { id: "plugins", label: "Plugins", icon: <Puzzle size={16} /> },
  { id: "system", label: "System", icon: <Settings size={16} /> },
];

export const ADMIN_MOBILE_HOME_CLASS = "md:hidden min-w-0 flex-1 overflow-y-auto p-3";
export const ADMIN_MOBILE_SECTION_LIST_CLASS = "overflow-hidden rounded-card border border-border bg-surface shadow-card";
export const ADMIN_MOBILE_SECTION_ROW_CLASS =
  "flex min-h-[52px] w-full items-center gap-3 border-b border-border/70 px-3 py-2 text-left last:border-b-0 active:bg-surface-raised";
export const ADMIN_MOBILE_SECTION_ICON_CLASS =
  "flex h-8 w-8 shrink-0 items-center justify-center rounded-card bg-surface-raised text-text-muted";
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
    case "auth":
      return "2FA, signup, OIDC, and login policy.";
    case "api-keys":
      return "User and service API credentials.";
    case "webhooks":
      return "Outbound events and delivery history.";
    case "ai":
      return "AI provider and model settings.";
    case "audit":
      return "Administrative event history.";
    case "notifications":
      return "SMTP and notification channels.";
    case "storage":
      return "Vaults, imports, sync, and recovery.";
    case "search":
      return "Indexing and search maintenance.";
    case "plugins":
      return "Install records and runtime audit.";
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
  "notifications",
  "search",
  "users",
  "webhooks",
]);

export function isAdminSectionAvailable(section: AdminSection, localMode: boolean): boolean {
  return !localMode || !LOCAL_UNAVAILABLE_ADMIN_SECTIONS.has(section);
}

export function getAvailableAdminSections(localMode: boolean) {
  return sections.filter((section) => isAdminSectionAvailable(section.id, localMode));
}

export function AdminPage({ initialSection = "users" }: { initialSection?: AdminSection } = {}) {
  const { user } = useAuth();
  const { open: sidebarOpen } = useContext(AdminSidebarContext);
  const localMode = getCurrentLaunchMode() === "local";
  const [activeSection, setActiveSection] = useState<AdminSection>(() =>
    getInitialAdminSection(initialSection, localMode),
  );
  const [mobileSectionOpen, setMobileSectionOpen] = useState(false);
  const availableSections = getAvailableAdminSections(localMode);
  const activeSectionMeta = sections.find((section) => section.id === activeSection);
  const showSectionContent = mobileSectionOpen ? "block" : "hidden md:block";

  if (!localMode && user?.role !== "admin") {
    return (
      <div className="flex h-full items-center justify-center">
        <p className="text-text-muted text-sm">Admin access required</p>
      </div>
    );
  }

  return (
    <div className="flex h-full min-w-0 flex-col md:flex-row">
      {/* Admin sidebar */}
      {sidebarOpen && (
        <nav className="border-border bg-surface hidden w-full shrink-0 border-b p-2 md:block md:w-48 md:border-b-0 md:border-r">
          <a
            href="/"
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text mb-2 flex items-center gap-2 px-2 py-1.5 text-sm transition-colors duration-100"
          >
            <ArrowLeft size={16} />
            Back to Notes
          </a>
          <h2 className="text-text-muted mb-2 px-2 text-xs font-medium uppercase tracking-wide">
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
        <button
          type="button"
          className={`${ADMIN_MOBILE_BACK_BUTTON_CLASS} md:hidden`}
          onClick={() => setMobileSectionOpen(false)}
        >
          <ChevronLeft size={17} />
          Site Administration
        </button>
        {activeSectionMeta && (
          <div className="mb-4 md:hidden">
            <h1 className="text-text text-lg font-semibold">{activeSectionMeta.label}</h1>
            <p className="text-text-muted mt-1 text-sm">
              {getAdminSectionDescription(activeSectionMeta.id, localMode)}
            </p>
          </div>
        )}
        {localMode && (
          <div className="rounded-card border-border bg-surface text-text-muted mb-4 border px-3 py-2 text-xs">
            Local mode is active. Server-admin actions are unavailable in this mode.
          </div>
        )}
        {activeSection === "users" && <UsersSection localMode={localMode} />}
        {activeSection === "spaces" && <SpacesSection localMode={localMode} />}
        {activeSection === "groups" && <GroupsSection localMode={localMode} />}
        {activeSection === "auth" && <AuthenticationSection localMode={localMode} />}
        {activeSection === "api-keys" && <ApiKeysSection localMode={localMode} />}
        {activeSection === "webhooks" && <WebhooksSection localMode={localMode} />}
        {activeSection === "notifications" && <NotificationsSection localMode={localMode} />}
        {activeSection === "audit" && <AuditSection localMode={localMode} />}
        {activeSection === "storage" && <StorageSection localMode={localMode} user={user} />}
        {activeSection === "search" && <SearchSection localMode={localMode} />}
        {activeSection === "plugins" && <PluginsSection />}
        {activeSection === "system" && <SystemSection localMode={localMode} />}
        {![
          "users",
          "spaces",
          "groups",
          "auth",
          "api-keys",
          "webhooks",
          "notifications",
          "audit",
          "storage",
          "search",
          "plugins",
          "system",
        ].includes(activeSection) && (
          <div className="text-text-muted text-sm">
            {sections.find((s) => s.id === activeSection)?.label} settings
          </div>
        )}
      </div>
    </div>
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
  if (role === "owner") return "Delete/admin";
  if (role === "editor") return "Read/write";
  return "Read";
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
      className="h-9 bg-surface px-2 text-sm"
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
      className={`min-w-0 px-2 text-sm ${className}`}
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
      className={`h-8 px-2 text-sm ${className}`}
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
      className="h-8 px-2 text-sm"
      aria-label="Permission role"
    >
      <option value="viewer">Read</option>
      <option value="editor">Read/write</option>
      <option value="owner">Delete/admin</option>
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
      className="h-8 px-2 text-sm"
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

  const loadUsers = useCallback(() => {
    if (localMode) {
      setUsersLoading(false);
      setUsers([]);
      return;
    }
    setUsersLoading(true);
    setError("");
    api
      .fetch<UserItem[]>("/admin/users")
      .then(setUsers)
      .catch((err) => {
        const failureState = getAdminUserLifecycleFailureState<UserItem, UserSessionItem>();
        setUsers(failureState.users);
        setSelectedUserIds(failureState.selectedUserIds);
        setSessionUser(failureState.sessionUser);
        setSessions(failureState.sessions);
        setError(err instanceof Error ? err.message : "Unable to load users");
      })
      .finally(() => setUsersLoading(false));
  }, [localMode]);

  useEffect(() => {
    loadUsers();
  }, [loadUsers]);

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
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to update user");
      await loadUsers();
    } finally {
      setBusyUserId(null);
    }
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
            disabled={localMode}
            onClick={() => importInputRef.current?.click()}
          >
            Import CSV
          </Button>
          <Button size="sm" disabled={localMode} onClick={() => setInviteOpen((open) => !open)}>
            Invite user
          </Button>
        </div>
      </div>
      {inviteOpen && (
        <form
          onSubmit={(event) => void submitInvite(event)}
          className="rounded-card border-border bg-surface mb-4 grid gap-3 border p-3 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_120px_auto]"
        >
          <label className="min-w-0">
            <span className="text-text-muted mb-1 block text-xs">Email</span>
            <AdminTextInput
              value={inviteEmail}
              onChange={setInviteEmail}
              type="email"
              required
              ariaLabel="Invite user email"
            />
          </label>
          <label className="min-w-0">
            <span className="text-text-muted mb-1 block text-xs">Display name</span>
            <AdminTextInput
              value={inviteName}
              onChange={setInviteName}
              required
              ariaLabel="Invite user display name"
            />
          </label>
          <label>
            <span className="text-text-muted mb-1 block text-xs">Role</span>
            <AdminFormSelect
              value={inviteRole}
              onChange={(value) => setInviteRole(value as "admin" | "member")}
              ariaLabel="Invite role"
            >
              <option value="member">Member</option>
              <option value="admin">Admin</option>
            </AdminFormSelect>
          </label>
          <div className="flex items-end">
            <Button size="sm" type="submit">
              Create
            </Button>
          </div>
        </form>
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
          ["Total", userLifecycleCounts.total],
          ["Active", userLifecycleCounts.active],
          ["Inactive", userLifecycleCounts.inactive],
          ["Admins", userLifecycleCounts.admins],
          ["Members", userLifecycleCounts.members],
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
          {selectedUserIds.size} selected
          {selectedUserIds.size > 0 ? ` · ${selectedVisibleUserIds.length} visible` : ""}
        </span>
        <div className="flex flex-wrap gap-2">
          <Button
            size="sm"
            variant="secondary"
            disabled={localMode || selectedUserIds.size === 0 || bulkLifecycleBusy}
            onClick={() => void runBulkLifecycle("activate")}
          >
            Activate selected
          </Button>
          <Button
            size="sm"
            variant="secondary"
            disabled={localMode || selectedUserIds.size === 0 || bulkLifecycleBusy}
            onClick={() => void runBulkLifecycle("deactivate")}
          >
            Deactivate selected
          </Button>
          <Button
            size="sm"
            variant="ghost"
            disabled={selectedUserIds.size === 0 || bulkLifecycleBusy}
            onClick={() => setSelectedUserIds(new Set())}
          >
            Clear selection
          </Button>
        </div>
      </div>

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
              <th className="px-3 py-2 font-medium">Role</th>
              <th className="px-3 py-2 font-medium">Status</th>
              <th className="px-3 py-2 font-medium">Actions</th>
            </tr>
          </thead>
          <tbody>
            {usersLoading ? (
              <tr>
                <td colSpan={6} className="text-text-muted px-3 py-8 text-center">
                  Loading users...
                </td>
              </tr>
            ) : visibleUsers.length === 0 ? (
              <tr>
                <td colSpan={6} className="text-text-muted px-3 py-8 text-center">
                  {users.length === 0
                    ? "No users to display. Use the Invite or Import buttons to add users."
                    : "No users match the current filters."}
                </td>
              </tr>
            ) : (
              visibleUsers.map((u) => {
                const isSelf = u.id === user?.id;
                const busy = busyUserId === u.id;
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
                    <td className="px-3 py-2">
                      <UserRoleSelect
                        value={u.role}
                        disabled={localMode || busy || isSelf}
                        onChange={(role) => void updateUser(u, { role })}
                      />
                    </td>
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
                        <Button
                          size="sm"
                          variant="secondary"
                          disabled={localMode || busy || isSelf}
                          onClick={() => void updateUser(u, { is_active: !u.is_active })}
                        >
                          {u.is_active ? "Deactivate" : "Reactivate"}
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={localMode}
                          onClick={() => void loadSessions(u)}
                        >
                          Sessions
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
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
            <Button size="sm" variant="ghost" onClick={closeSessions}>
              Close
            </Button>
          </div>
          {sessionRevokeTarget && (
            <div
              role="dialog"
              aria-labelledby="admin-session-revoke-title"
              className="rounded-card border-border bg-bg mb-3 border p-3"
            >
              <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
                <div className="min-w-0">
                  <p id="admin-session-revoke-title" className="text-text text-sm font-medium">
                    Revoke session
                  </p>
                  <p className="text-text-muted mt-1 text-xs leading-5">
                    {getAdminSessionRevokeConfirmationMessage(
                      sessionUser.email,
                      getAdminSessionDeviceLabel(sessionRevokeTarget),
                    )}
                  </p>
                </div>
                <div className="grid grid-cols-2 gap-2 sm:flex">
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => setSessionRevokeTarget(null)}
                  >
                    Cancel
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    onClick={() => {
                      void revokeSession(sessionRevokeTarget);
                    }}
                  >
                    Revoke session
                  </Button>
                </div>
              </div>
            </div>
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
  const [users, setUsers] = useState<UserItem[]>([]);
  const [groups, setGroups] = useState<GroupItem[]>([]);
  const [members, setMembers] = useState<SpaceMemberItem[]>([]);
  const [spaceGroups, setSpaceGroups] = useState<SpaceGroupItem[]>([]);
  const [selectedSpaceId, setSelectedSpaceId] = useState(() => {
    if (typeof window === "undefined") return "";
    return new URLSearchParams(window.location.search).get("spaceId") ?? "";
  });
  const [spaceNameDraft, setSpaceNameDraft] = useState("");
  const [newSpaceName, setNewSpaceName] = useState("");
  const [memberUserId, setMemberUserId] = useState("");
  const [memberRole, setMemberRole] = useState("viewer");
  const [groupId, setGroupId] = useState("");
  const [groupRole, setGroupRole] = useState("viewer");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const selectedSpace = spaces.find((space) => space.id === selectedSpaceId) || spaces[0] || null;

  const loadLocalSpaces = async () => {
    const spaceList = await dataClient.listSpaces();
    setSpaces(sortAdminLocalSpaces(spaceList.map((space) => ({ ...space, visibility: "local" }))));
    setSelectedSpaceId((current) =>
      current && spaceList.some((space) => space.id === current) ? current : spaceList[0]?.id || "",
    );
  };

  const reloadSpaceAccess = async (spaceId: string) => {
    const [memberList, groupList] = await Promise.all([
      api.fetch<SpaceMemberItem[]>(`/spaces/${spaceId}/members`),
      api.fetch<SpaceGroupItem[]>(`/spaces/${spaceId}/groups`),
    ]);
    setMembers(memberList);
    setSpaceGroups(groupList);
  };

  useEffect(() => {
    if (localMode) {
      loadLocalSpaces().catch((err) =>
        setError(err instanceof Error ? err.message : "Failed to load local spaces"),
      );
      setUsers([]);
      setGroups([]);
      return;
    }

    Promise.all([
      api.fetch<SpaceItem[]>("/spaces"),
      api.fetch<UserItem[]>("/admin/users"),
      api.fetch<GroupItem[]>("/admin/groups"),
    ])
      .then(([spaceList, userList, groupList]) => {
        setSpaces(spaceList);
        setUsers(userList);
        setGroups(groupList);
        setSelectedSpaceId((current) => current || spaceList[0]?.id || "");
      })
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load spaces"));
  }, [localMode]);

  useEffect(() => {
    setSpaceNameDraft(selectedSpace?.name ?? "");
  }, [selectedSpace?.id, selectedSpace?.name]);

  useEffect(() => {
    if (localMode || !selectedSpace?.id) {
      setMembers([]);
      setSpaceGroups([]);
      return;
    }

    reloadSpaceAccess(selectedSpace.id).catch((err) => {
      setError(err instanceof Error ? err.message : "Failed to load permissions");
    });
  }, [localMode, selectedSpace?.id]);

  const updateVisibility = async (visibility: string) => {
    if (!selectedSpace) return;
    setMessage(null);
    setError(null);
    try {
      await api.fetch(`/spaces/${selectedSpace.id}`, {
        method: "PUT",
        body: { visibility },
      });
      setSpaces((prev) =>
        prev.map((space) => (space.id === selectedSpace.id ? { ...space, visibility } : space)),
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

  const addMember = async () => {
    if (!selectedSpace || !memberUserId) return;
    setMessage(null);
    setError(null);
    try {
      await api.fetch(`/spaces/${selectedSpace.id}/members`, {
        method: "POST",
        body: { user_id: memberUserId, role: memberRole },
      });
      await reloadSpaceAccess(selectedSpace.id);
      setMemberUserId("");
      setMessage("User permission added.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to add user");
    }
  };

  const updateMemberRole = async (userId: string, role: string) => {
    if (!selectedSpace) return;
    await api.fetch(`/spaces/${selectedSpace.id}/members/${userId}`, {
      method: "PUT",
      body: { user_id: userId, role },
    });
    setMembers((prev) =>
      prev.map((member) => (member.user_id === userId ? { ...member, role } : member)),
    );
  };

  const removeMember = async (userId: string) => {
    if (!selectedSpace) return;
    await api.fetch(`/spaces/${selectedSpace.id}/members/${userId}`, { method: "DELETE" });
    setMembers((prev) => prev.filter((member) => member.user_id !== userId));
  };

  const grantGroup = async () => {
    if (!selectedSpace || !groupId) return;
    setMessage(null);
    setError(null);
    try {
      await api.fetch(`/spaces/${selectedSpace.id}/groups`, {
        method: "POST",
        body: { group_id: groupId, role: groupRole },
      });
      await reloadSpaceAccess(selectedSpace.id);
      setGroupId("");
      setMessage("Group permission added.");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to add group");
    }
  };

  const updateGroupRole = async (id: string, role: string) => {
    if (!selectedSpace) return;
    await api.fetch(`/spaces/${selectedSpace.id}/groups/${id}`, {
      method: "PUT",
      body: { group_id: id, role },
    });
    setSpaceGroups((prev) =>
      prev.map((group) => (group.group_id === id ? { ...group, role } : group)),
    );
  };

  const removeGroup = async (id: string) => {
    if (!selectedSpace) return;
    await api.fetch(`/spaces/${selectedSpace.id}/groups/${id}`, { method: "DELETE" });
    setSpaceGroups((prev) => prev.filter((group) => group.group_id !== id));
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
                    className={`rounded-button block w-full px-2 py-1.5 text-left text-sm transition-colors duration-100 ${
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
          <div className="grid gap-3 lg:grid-cols-[minmax(0,16rem)_minmax(0,1fr)]">
            <div className="rounded-card border-border border p-2">
              {spaces.map((space) => (
                <button
                  key={space.id}
                  type="button"
                  onClick={() => setSelectedSpaceId(space.id)}
                  className={`rounded-button block w-full px-2 py-1.5 text-left text-sm transition-colors duration-100 ${
                    selectedSpace?.id === space.id
                      ? "bg-surface-raised text-text"
                      : "text-text-muted hover:bg-surface-raised hover:text-text"
                  }`}
                >
                  <span className="block truncate">{space.name}</span>
                  <span className="text-text-muted block text-xs">{space.visibility}</span>
                </button>
              ))}
            </div>

            <div className="space-y-4">
              {selectedSpace && (
                <div className="rounded-card border-border border p-3">
                  <div className="mb-3 flex items-center justify-between gap-3">
                    <div>
                      <p className="text-text text-sm font-medium">{selectedSpace.name}</p>
                      <p className="text-text-muted text-xs">{selectedSpace.slug}</p>
                    </div>
                    <AdminInlineSelect
                      value={selectedSpace.visibility}
                      onChange={(value) => {
                        void updateVisibility(value);
                      }}
                      ariaLabel="Space visibility"
                    >
                      <option value="private">Private</option>
                      <option value="internal">Signed-in users</option>
                      <option value="public">Public</option>
                    </AdminInlineSelect>
                  </div>

                  <div className="grid gap-4 xl:grid-cols-2">
                    <div>
                      <div className="mb-2 flex items-center justify-between">
                        <h2 className="text-text text-sm font-medium">Users</h2>
                        <div className="flex gap-2">
                          <AdminInlineSelect
                            value={memberUserId}
                            onChange={setMemberUserId}
                            className="max-w-44"
                            ariaLabel="User to add"
                          >
                            <option value="">Select user</option>
                            {users.map((item) => (
                              <option key={item.id} value={item.id}>
                                {item.email}
                              </option>
                            ))}
                          </AdminInlineSelect>
                          <RoleSelect value={memberRole} onChange={setMemberRole} />
                          <Button
                            size="sm"
                            onClick={() => {
                              void addMember();
                            }}
                          >
                            Add
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
                            <RoleSelect
                              value={member.role}
                              onChange={(role) => {
                                void updateMemberRole(member.user_id, role);
                              }}
                            />
                            <button
                              type="button"
                              onClick={() => {
                                void removeMember(member.user_id);
                              }}
                              className={ADMIN_DANGER_ICON_BUTTON_CLASS}
                              aria-label={`Remove ${member.email}`}
                            >
                              <UserRoundX size={14} />
                            </button>
                          </div>
                        ))}
                      </div>
                    </div>

                    <div>
                      <div className="mb-2 flex items-center justify-between">
                        <h2 className="text-text text-sm font-medium">Groups</h2>
                        <div className="flex gap-2">
                          <AdminInlineSelect
                            value={groupId}
                            onChange={setGroupId}
                            className="max-w-44"
                            ariaLabel="Group to add"
                          >
                            <option value="">Select group</option>
                            {groups.map((item) => (
                              <option key={item.id} value={item.id}>
                                {item.name}
                              </option>
                            ))}
                          </AdminInlineSelect>
                          <RoleSelect value={groupRole} onChange={setGroupRole} />
                          <Button
                            size="sm"
                            onClick={() => {
                              void grantGroup();
                            }}
                          >
                            Add
                          </Button>
                        </div>
                      </div>
                      <div className="rounded-card border-border border">
                        {spaceGroups.map((group) => (
                          <div
                            key={group.group_id}
                            className="border-border flex items-center gap-2 border-b px-3 py-2 last:border-0"
                          >
                            <div className="min-w-0 flex-1">
                              <p className="text-text truncate text-sm">{group.name}</p>
                              <p className="text-text-muted truncate text-xs">
                                {group.member_count} member{group.member_count === 1 ? "" : "s"} ·{" "}
                                {roleLabel(group.role)}
                              </p>
                            </div>
                            <RoleSelect
                              value={group.role}
                              onChange={(role) => {
                                void updateGroupRole(group.group_id, role);
                              }}
                            />
                            <button
                              type="button"
                              onClick={() => {
                                void removeGroup(group.group_id);
                              }}
                              className={ADMIN_DANGER_ICON_BUTTON_CLASS}
                              aria-label={`Remove ${group.name}`}
                            >
                              <UserRoundX size={14} />
                            </button>
                          </div>
                        ))}
                      </div>
                    </div>
                  </div>
                </div>
              )}
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
      )}
    </div>
  );
}

function GroupsSection({ localMode }: { localMode: boolean }) {
  const [groups, setGroups] = useState<GroupItem[]>([]);
  const [selectedGroupId, setSelectedGroupId] = useState("");
  const [members, setMembers] = useState<SpaceMemberItem[]>([]);
  const [users, setUsers] = useState<UserItem[]>([]);
  const [newGroupName, setNewGroupName] = useState("");
  const [newGroupDescription, setNewGroupDescription] = useState("");
  const [userId, setUserId] = useState("");
  const [error, setError] = useState<string | null>(null);

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
      return;
    }
    api
      .fetch<SpaceMemberItem[]>(`/admin/groups/${selectedGroup.id}/members`)
      .then(setMembers)
      .catch((err) => setError(err instanceof Error ? err.message : "Failed to load members"));
  }, [localMode, selectedGroup?.id]);

  const createGroup = async () => {
    if (!newGroupName.trim()) return;
    await api.fetch<GroupItem>("/admin/groups", {
      method: "POST",
      body: {
        name: newGroupName.trim(),
        description: newGroupDescription.trim() || null,
      },
    });
    setNewGroupName("");
    setNewGroupDescription("");
    await reloadGroups();
  };

  const addMember = async () => {
    if (!selectedGroup || !userId) return;
    await api.fetch(`/admin/groups/${selectedGroup.id}/members`, {
      method: "POST",
      body: { user_id: userId },
    });
    setUserId("");
    const refreshed = await api.fetch<SpaceMemberItem[]>(
      `/admin/groups/${selectedGroup.id}/members`,
    );
    setMembers(refreshed);
    await reloadGroups();
  };

  const removeMember = async (id: string) => {
    if (!selectedGroup) return;
    await api.fetch(`/admin/groups/${selectedGroup.id}/members/${id}`, { method: "DELETE" });
    setMembers((prev) => prev.filter((member) => member.user_id !== id));
    await reloadGroups();
  };

  return (
    <div>
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-text text-lg font-medium">Groups</h1>
        <span className="text-text-muted text-xs">Reusable permission groups</span>
      </div>

      {localMode ? (
        <div className="rounded-card border-border text-text-muted border p-3 text-sm">
          Groups are unavailable in local mode.
        </div>
      ) : (
        <div className="grid gap-4 lg:grid-cols-[minmax(0,18rem)_minmax(0,1fr)]">
          <div className="space-y-3">
            <div className="rounded-card border-border border p-3">
              <h2 className="text-text mb-2 text-sm font-medium">Create group</h2>
              <div className="space-y-2">
                <AdminTextInput
                  value={newGroupName}
                  onChange={setNewGroupName}
                  placeholder="Group name"
                  ariaLabel="Group name"
                />
                <AdminTextInput
                  value={newGroupDescription}
                  onChange={setNewGroupDescription}
                  placeholder="Description"
                  ariaLabel="Group description"
                />
                <Button
                  size="sm"
                  onClick={() => {
                    void createGroup();
                  }}
                >
                  Create group
                </Button>
              </div>
            </div>
            <div className="rounded-card border-border border p-2">
              {groups.map((group) => (
                <button
                  key={group.id}
                  type="button"
                  onClick={() => setSelectedGroupId(group.id)}
                  className={`rounded-button block w-full px-2 py-1.5 text-left text-sm transition-colors duration-100 ${
                    selectedGroup?.id === group.id
                      ? "bg-surface-raised text-text"
                      : "text-text-muted hover:bg-surface-raised hover:text-text"
                  }`}
                >
                  <span className="block truncate">{group.name}</span>
                  <span className="text-text-muted block text-xs">
                    {group.member_count} members
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
                  <div className="flex gap-2">
                    <AdminInlineSelect
                      value={userId}
                      onChange={setUserId}
                      className="max-w-56"
                      ariaLabel="User to add to group"
                    >
                      <option value="">Select user</option>
                      {users.map((item) => (
                        <option key={item.id} value={item.id}>
                          {item.email}
                        </option>
                      ))}
                    </AdminInlineSelect>
                    <Button
                      size="sm"
                      onClick={() => {
                        void addMember();
                      }}
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
                        onClick={() => {
                          void removeMember(member.user_id);
                        }}
                        className={ADMIN_DANGER_ICON_BUTTON_CLASS}
                        aria-label={`Remove ${member.email}`}
                      >
                        <UserRoundX size={14} />
                      </button>
                    </div>
                  ))}
                </div>
              </>
            ) : (
              <p className="text-text-muted text-sm">Create a group to add members.</p>
            )}
          </div>
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

function AuthenticationSection({ localMode }: { localMode: boolean }) {
  const [entries, setEntries] = useState<ConfigEntry[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

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
      setError(err instanceof Error ? err.message : "Unable to save authentication setting");
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
      setError(err instanceof Error ? err.message : "Unable to reset authentication setting");
    }
  };

  return (
    <div>
      <div className="mb-4 flex items-center justify-between gap-3">
        <div>
          <h1 className="text-text text-lg font-medium">Authentication</h1>
          <p className="text-text-muted text-xs">
            Configure server authentication requirements and OIDC login behavior.
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

        <div className="rounded-card border-border border p-3">
          <div className="mb-3">
            <p className="text-text text-sm font-medium">Login Policy</p>
            <p className="text-text-muted text-xs">
              Environment-backed values are locked here and must be changed on the server.
            </p>
          </div>
          <div className="space-y-2">
            {entries.length === 0 ? (
              <p className="text-text-muted text-sm">
                {localMode ? "No server settings available." : "Loading authentication settings..."}
              </p>
            ) : (
              entries.map((entry) => (
                <div
                  key={entry.key}
                  className="rounded-card border-border grid gap-2 border p-2 lg:grid-cols-[minmax(0,13rem)_minmax(0,1fr)_auto_auto]"
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

function ApiKeysSection({ localMode }: { localMode: boolean }) {
  const [keys, setKeys] = useState<ApiKeyItem[]>([]);
  const [serviceKeys, setServiceKeys] = useState<ApiKeyItem[]>([]);
  const [name, setName] = useState("");
  const [expiresAt, setExpiresAt] = useState("");
  const [keyScopes, setKeyScopes] = useState<string[]>(["read", "write"]);
  const [serviceName, setServiceName] = useState("");
  const [serviceExpiresAt, setServiceExpiresAt] = useState("");
  const [serviceScopes, setServiceScopes] = useState<string[]>(["admin"]);
  const [createdKey, setCreatedKey] = useState<ApiKeyCreateResult | null>(null);
  const [createdServiceKey, setCreatedServiceKey] = useState<ApiKeyCreateResult | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadKeys = useCallback(async () => {
    if (localMode) {
      setKeys([]);
      setServiceKeys([]);
      return;
    }
    const [items, serviceItems] = await Promise.all([
      api.fetch<ApiKeyItem[]>("/auth/api-keys"),
      api.fetch<ApiKeyItem[]>("/admin/api-keys"),
    ]);
    setKeys(items);
    setServiceKeys(serviceItems);
  }, [localMode]);

  useEffect(() => {
    void loadKeys().catch((err) => {
      setError(err instanceof Error ? err.message : "Unable to load API keys");
    });
  }, [loadKeys]);

  const createKey = async (event: FormEvent) => {
    event.preventDefault();
    if (localMode) return;
    setMessage(null);
    setError(null);
    setCreatedKey(null);
    setCreatedServiceKey(null);
    try {
      const expiry = expiresAt ? new Date(expiresAt).toISOString() : undefined;
      const result = await api.fetch<ApiKeyCreateResult>("/auth/api-keys", {
        method: "POST",
        body: { name, expires_at: expiry, scopes: keyScopes },
      });
      setCreatedKey(result);
      setName("");
      setExpiresAt("");
      setKeyScopes(["read", "write"]);
      setMessage("API key created. Copy it now; it will not be shown again.");
      await loadKeys();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to create API key");
    }
  };

  const toggleKeyScope = (scope: string, enabled: boolean) => {
    setKeyScopes((prev) => {
      const next = enabled ? [...prev, scope] : prev.filter((item) => item !== scope);
      return Array.from(new Set(next));
    });
  };

  const toggleServiceScope = (scope: string, enabled: boolean) => {
    setServiceScopes((prev) => {
      const next = enabled ? [...prev, scope] : prev.filter((item) => item !== scope);
      return Array.from(new Set(next));
    });
  };

  const createServiceKey = async (event: FormEvent) => {
    event.preventDefault();
    if (localMode) return;
    setMessage(null);
    setError(null);
    setCreatedKey(null);
    setCreatedServiceKey(null);
    try {
      const expiry = serviceExpiresAt ? new Date(serviceExpiresAt).toISOString() : undefined;
      const result = await api.fetch<ApiKeyCreateResult>("/admin/api-keys", {
        method: "POST",
        body: { name: serviceName, expires_at: expiry, scopes: serviceScopes },
      });
      setCreatedServiceKey(result);
      setServiceName("");
      setServiceExpiresAt("");
      setServiceScopes(["admin"]);
      setMessage("Service account API key created. Copy it now; it will not be shown again.");
      await loadKeys();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to create service account key");
    }
  };

  const revokeKey = async (key: ApiKeyItem) => {
    if (localMode) return;
    setMessage(null);
    setError(null);
    try {
      await api.fetch(`/auth/api-keys/${encodeURIComponent(key.id)}`, { method: "DELETE" });
      setMessage(`${key.name} revoked.`);
      if (createdKey?.id === key.id) {
        setCreatedKey(null);
      }
      await loadKeys();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to revoke API key");
    }
  };

  const revokeServiceKey = async (key: ApiKeyItem) => {
    if (localMode) return;
    setMessage(null);
    setError(null);
    try {
      await api.fetch(`/admin/api-keys/${encodeURIComponent(key.id)}`, { method: "DELETE" });
      setMessage(`${key.name} revoked.`);
      if (createdServiceKey?.id === key.id) {
        setCreatedServiceKey(null);
      }
      await loadKeys();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to revoke service account key");
    }
  };

  return (
    <div>
      <div className="mb-4 flex items-center justify-between gap-3">
        <div>
          <h1 className="text-text text-lg font-medium">API Keys</h1>
          <p className="text-text-muted text-xs">
            Create and revoke personal API keys for script and integration access.
          </p>
        </div>
        <Button
          size="sm"
          variant="secondary"
          disabled={localMode}
          onClick={() => {
            void loadKeys();
          }}
        >
          <RefreshCw size={14} className="mr-1" />
          Refresh
        </Button>
      </div>

      <div className="space-y-3">
        {localMode && (
          <div className="rounded-card border-border text-text-muted border p-3 text-sm">
            Server API keys are unavailable in local standalone mode.
          </div>
        )}

        <form onSubmit={createKey} className="rounded-card border-border border p-3">
          <div className="mb-3">
            <p className="text-text text-sm font-medium">New API Key</p>
            <p className="text-text-muted text-xs">
              Keys authenticate with the <code>X-API-Key</code> header and are displayed once.
            </p>
          </div>
          <div className="grid gap-2 md:grid-cols-[minmax(0,1fr)_13rem_auto]">
            <AdminTextInput
              value={name}
              onChange={setName}
              disabled={localMode}
              placeholder="Key name"
              ariaLabel="API key name"
            />
            <AdminTextInput
              value={expiresAt}
              onChange={setExpiresAt}
              disabled={localMode}
              type="datetime-local"
              ariaLabel="Expiration date"
            />
            <Button type="submit" disabled={localMode || !name.trim() || keyScopes.length === 0}>
              Create
            </Button>
          </div>
          <fieldset className="mt-3 flex flex-wrap gap-3">
            <legend className="text-text-muted mb-2 text-xs">Scopes</legend>
            {[
              ["read", "Read"],
              ["write", "Write"],
              ["admin", "Admin"],
            ].map(([scope, label]) => (
              <label key={scope} className="text-text-muted flex items-center gap-2 text-xs">
                <input
                  type="checkbox"
                  checked={keyScopes.includes(scope)}
                  disabled={localMode}
                  onChange={(event) => toggleKeyScope(scope, event.target.checked)}
                />
                {label}
              </label>
            ))}
          </fieldset>
        </form>

        <form onSubmit={createServiceKey} className="rounded-card border-border border p-3">
          <div className="mb-3">
            <p className="text-text text-sm font-medium">New Service Account Key</p>
            <p className="text-text-muted text-xs">
              Service account keys are admin-managed and require the admin scope.
            </p>
          </div>
          <div className="grid gap-2 md:grid-cols-[minmax(0,1fr)_13rem_auto]">
            <AdminTextInput
              value={serviceName}
              onChange={setServiceName}
              disabled={localMode}
              placeholder="Service name"
              ariaLabel="Service account key name"
            />
            <AdminTextInput
              value={serviceExpiresAt}
              onChange={setServiceExpiresAt}
              disabled={localMode}
              type="datetime-local"
              ariaLabel="Service account expiration date"
            />
            <Button
              type="submit"
              disabled={localMode || !serviceName.trim() || !serviceScopes.includes("admin")}
            >
              Create service key
            </Button>
          </div>
          <fieldset className="mt-3 flex flex-wrap gap-3">
            <legend className="text-text-muted mb-2 text-xs">Service scopes</legend>
            {[
              ["read", "Read"],
              ["write", "Write"],
              ["admin", "Admin"],
            ].map(([scope, label]) => (
              <label key={scope} className="text-text-muted flex items-center gap-2 text-xs">
                <input
                  type="checkbox"
                  checked={serviceScopes.includes(scope)}
                  disabled={localMode || scope === "admin"}
                  onChange={(event) => toggleServiceScope(scope, event.target.checked)}
                />
                {label}
              </label>
            ))}
          </fieldset>
        </form>

        {createdKey && (
          <div className={ADMIN_SECRET_SUCCESS_CARD_CLASS}>
            <p className={ADMIN_SECRET_SUCCESS_TITLE_CLASS}>Copy this API key now</p>
            <code className="border-border bg-surface text-text mt-2 block overflow-x-auto rounded border p-2 text-xs">
              {createdKey.key}
            </code>
          </div>
        )}

        {createdServiceKey && (
          <div className={ADMIN_SECRET_SUCCESS_CARD_CLASS}>
            <p className={ADMIN_SECRET_SUCCESS_TITLE_CLASS}>Copy this service account key now</p>
            <code className="border-border bg-surface text-text mt-2 block overflow-x-auto rounded border p-2 text-xs">
              {createdServiceKey.key}
            </code>
          </div>
        )}

        <div className="rounded-card border-border border p-3">
          <div className="mb-3">
            <p className="text-text text-sm font-medium">Active Keys</p>
            <p className="text-text-muted text-xs">Secret values are hashed at rest.</p>
          </div>
          {keys.length === 0 ? (
            <p className="text-text-muted text-sm">
              {localMode ? "No server keys available." : "No API keys created."}
            </p>
          ) : (
            <div className="space-y-2">
              {keys.map((key) => (
                <div
                  key={key.id}
                  className="rounded-card border-border grid gap-2 border p-2 lg:grid-cols-[minmax(0,1fr)_9rem_11rem_11rem_auto]"
                >
                  <div className="min-w-0">
                    <p className="text-text truncate text-sm">{key.name}</p>
                    <p className="text-text-muted truncate text-xs">{key.key_prefix}...</p>
                    <p className="text-text-muted mt-1 flex flex-wrap gap-1 text-xs">
                      {key.scopes.map((scope) => (
                        <span key={scope} className="rounded bg-surface-raised px-1.5 py-0.5">
                          {scope}
                        </span>
                      ))}
                    </p>
                  </div>
                  <p className="text-text-muted text-xs">
                    {key.is_service_account ? "Service" : "Personal"}
                  </p>
                  <p className="text-text-muted text-xs">
                    Last used: {key.last_used_at ? new Date(key.last_used_at).toLocaleString() : "Never"}
                  </p>
                  <p className="text-text-muted text-xs">
                    Expires: {key.expires_at ? new Date(key.expires_at).toLocaleString() : "Never"}
                  </p>
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={localMode}
                    onClick={() => {
                      void revokeKey(key);
                    }}
                  >
                    Revoke
                  </Button>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="rounded-card border-border border p-3">
          <div className="mb-3">
            <p className="text-text text-sm font-medium">Service Account Keys</p>
            <p className="text-text-muted text-xs">Use for server-to-server automation only.</p>
          </div>
          {serviceKeys.length === 0 ? (
            <p className="text-text-muted text-sm">
              {localMode ? "No service keys available." : "No service account keys created."}
            </p>
          ) : (
            <div className="space-y-2">
              {serviceKeys.map((key) => (
                <div
                  key={key.id}
                  className="rounded-card border-border grid gap-2 border p-2 lg:grid-cols-[minmax(0,1fr)_11rem_11rem_auto]"
                >
                  <div className="min-w-0">
                    <p className="text-text truncate text-sm">{key.name}</p>
                    <p className="text-text-muted truncate text-xs">{key.key_prefix}...</p>
                    <p className="text-text-muted mt-1 flex flex-wrap gap-1 text-xs">
                      {key.scopes.map((scope) => (
                        <span key={scope} className="rounded bg-surface-raised px-1.5 py-0.5">
                          {scope}
                        </span>
                      ))}
                    </p>
                  </div>
                  <p className="text-text-muted text-xs">
                    Last used: {key.last_used_at ? new Date(key.last_used_at).toLocaleString() : "Never"}
                  </p>
                  <p className="text-text-muted text-xs">
                    Expires: {key.expires_at ? new Date(key.expires_at).toLocaleString() : "Never"}
                  </p>
                  <Button
                    size="sm"
                    variant="ghost"
                    disabled={localMode}
                    onClick={() => {
                      void revokeServiceKey(key);
                    }}
                  >
                    Revoke
                  </Button>
                </div>
              ))}
            </div>
          )}
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

const DEFAULT_WEBHOOK_EVENTS = [
  "document.created",
  "document.updated",
  "document.deleted",
  "workflow.transitioned",
  "workflow.delegated",
  "comment.created",
];

export function AdminWebhookDeliveryHistoryTable({
  deliveries,
  localMode,
  redeliveringDeliveryId,
  redeliveryError,
  onRedeliver,
}: AdminWebhookDeliveryHistoryTableProps) {
  return (
    <div className="space-y-2">
      {redeliveryError && (
        <div className={ADMIN_COMPACT_ERROR_CLASS}>{redeliveryError}</div>
      )}
      <div className="rounded-card border-border overflow-hidden border">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-border text-text-muted border-b text-left text-xs">
              <th className="px-3 py-2 font-medium">Time</th>
              <th className="px-3 py-2 font-medium">Event</th>
              <th className="px-3 py-2 font-medium">Status</th>
              <th className="px-3 py-2 font-medium">Attempts</th>
              <th className="px-3 py-2 font-medium"></th>
            </tr>
          </thead>
          <tbody>
            {deliveries.map((delivery) => {
              const isRedelivering = redeliveringDeliveryId === delivery.id;
              return (
                <tr key={delivery.id} className="border-border border-b last:border-0">
                  <td className="text-text-muted px-3 py-2">
                    {new Date(delivery.delivered_at).toLocaleString()}
                  </td>
                  <td className="px-3 py-2">{delivery.event_type}</td>
                  <td className="px-3 py-2">
                    <span className={getAdminWebhookDeliveryStatusClass(delivery.success)}>
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
                      disabled={localMode || isRedelivering || redeliveringDeliveryId !== null}
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

function WebhooksSection({ localMode }: { localMode: boolean }) {
  const [webhooks, setWebhooks] = useState<WebhookItem[]>([]);
  const [selectedWebhookId, setSelectedWebhookId] = useState<string | null>(null);
  const [deliveries, setDeliveries] = useState<WebhookDeliveryItem[]>([]);
  const [deliveriesLoading, setDeliveriesLoading] = useState(false);
  const [deliveriesError, setDeliveriesError] = useState<string | null>(null);
  const [redeliveringDeliveryId, setRedeliveringDeliveryId] = useState<string | null>(null);
  const [redeliveryError, setRedeliveryError] = useState<string | null>(null);
  const [form, setForm] = useState<WebhookFormState>({
    name: "",
    url: "",
    secret: "",
    events: DEFAULT_WEBHOOK_EVENTS.slice(0, 2).join(", "),
    space_id: "",
  });
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const selectedWebhook = webhooks.find((webhook) => webhook.id === selectedWebhookId) ?? null;

  const loadWebhooks = useCallback(async () => {
    if (localMode) {
      setWebhooks([]);
      setSelectedWebhookId(null);
      setDeliveries([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const items = await api.fetch<WebhookItem[]>("/webhooks");
      setWebhooks(items);
      setSelectedWebhookId((current) =>
        current && items.some((item) => item.id === current) ? current : (items[0]?.id ?? null),
      );
      if (items.length === 0) setDeliveries([]);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to load webhooks");
    } finally {
      setLoading(false);
    }
  }, [localMode]);

  const loadDeliveries = useCallback(
    async (webhookId: string | null) => {
      if (localMode || !webhookId) {
        const resetState = getAdminWebhookDeliveryResetState();
        setDeliveries(resetState.deliveries);
        setDeliveriesError(resetState.deliveriesError);
        setRedeliveryError(resetState.redeliveryError);
        setRedeliveringDeliveryId(resetState.redeliveringDeliveryId);
        setDeliveriesLoading(resetState.deliveriesLoading);
        return;
      }
      const resetState = getAdminWebhookDeliveryResetState();
      setDeliveries(resetState.deliveries);
      setRedeliveryError(resetState.redeliveryError);
      setRedeliveringDeliveryId(resetState.redeliveringDeliveryId);
      setDeliveriesLoading(true);
      setDeliveriesError(resetState.deliveriesError);
      try {
        const items = await api.fetch<WebhookDeliveryItem[]>(
          `/webhooks/${webhookId}/deliveries`,
        );
        setDeliveries(items);
      } catch (err) {
        setDeliveriesError(
          err instanceof Error ? err.message : "Unable to load webhook deliveries",
        );
        setDeliveries([]);
      } finally {
        setDeliveriesLoading(false);
      }
    },
    [localMode],
  );

  useEffect(() => {
    void loadWebhooks();
  }, [loadWebhooks]);

  useEffect(() => {
    void loadDeliveries(selectedWebhookId);
  }, [loadDeliveries, selectedWebhookId]);

  const handleCreateWebhook = async (event: FormEvent) => {
    event.preventDefault();
    if (localMode) {
      setError("Webhook management is unavailable in local mode.");
      return;
    }
    setSaving(true);
    setError(null);
    setMessage(null);
    try {
      const events = parseWebhookEvents(form.events);
      if (!form.name.trim() || !form.url.trim() || events.length === 0) {
        throw new Error("Name, URL, and at least one event are required.");
      }
      const result = await api.fetch<{ id: string }>("/webhooks", {
        method: "POST",
        body: {
          name: form.name.trim(),
          url: form.url.trim(),
          secret: form.secret.trim() || undefined,
          events,
          space_id: form.space_id.trim() || undefined,
        },
      });
      setForm({
        name: "",
        url: "",
        secret: "",
        events: DEFAULT_WEBHOOK_EVENTS.slice(0, 2).join(", "),
        space_id: "",
      });
      setMessage("Webhook created.");
      await loadWebhooks();
      setSelectedWebhookId(result.id);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to create webhook");
    } finally {
      setSaving(false);
    }
  };

  const deleteWebhook = async (webhookId: string) => {
    if (localMode) return;
    setError(null);
    setMessage(null);
    try {
      await api.fetch<void>(`/webhooks/${webhookId}`, { method: "DELETE" });
      setMessage("Webhook deleted.");
      setSelectedWebhookId(null);
      await loadWebhooks();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Unable to delete webhook");
    }
  };

  const redeliver = async (deliveryId: string) => {
    if (localMode || !selectedWebhookId || redeliveringDeliveryId) return;
    setError(null);
    setMessage(null);
    setRedeliveryError(null);
    setRedeliveringDeliveryId(deliveryId);
    try {
      await postNoBody(`/webhooks/${selectedWebhookId}/deliveries/${deliveryId}/redeliver`);
      setMessage("Webhook redelivery queued.");
      await loadDeliveries(selectedWebhookId);
    } catch (err) {
      setRedeliveryError(err instanceof Error ? err.message : "Unable to queue redelivery");
    } finally {
      setRedeliveringDeliveryId(null);
    }
  };

  return (
    <div>
      <div className="mb-4 flex items-center justify-between gap-3">
        <div>
          <h1 className="text-text text-lg font-medium">Webhooks</h1>
          <p className="text-text-muted text-xs">
            Manage outbound event deliveries and inspect recent delivery attempts.
          </p>
        </div>
        <Button
          size="sm"
          variant="secondary"
          disabled={localMode || loading}
          onClick={() => {
            void loadWebhooks();
          }}
        >
          <RefreshCw size={14} className="mr-1" />
          Refresh
        </Button>
      </div>

      <div className="space-y-3">
        {localMode && (
          <div className="rounded-card border-border text-text-muted border p-3 text-sm">
            Webhook delivery is a server-mode feature. Local standalone notes do not dispatch
            outbound webhooks.
          </div>
        )}

        <form
          onSubmit={(event) => {
            void handleCreateWebhook(event);
          }}
          className="rounded-card border-border grid gap-3 border p-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]"
        >
          <label className="block">
            <span className="text-text-muted mb-1 block text-xs">Name</span>
            <AdminTextInput
              value={form.name}
              onChange={(value) => setForm((current) => ({ ...current, name: value }))}
              disabled={localMode || saving}
              placeholder="Release automation"
              ariaLabel="Webhook name"
            />
          </label>
          <label className="block">
            <span className="text-text-muted mb-1 block text-xs">URL</span>
            <AdminTextInput
              value={form.url}
              onChange={(value) => setForm((current) => ({ ...current, url: value }))}
              disabled={localMode || saving}
              placeholder="https://example.com/starlane/webhook"
              ariaLabel="Webhook URL"
            />
          </label>
          <label className="block">
            <span className="text-text-muted mb-1 block text-xs">Events</span>
            <AdminTextInput
              value={form.events}
              onChange={(value) => setForm((current) => ({ ...current, events: value }))}
              disabled={localMode || saving}
              placeholder={DEFAULT_WEBHOOK_EVENTS.join(", ")}
              ariaLabel="Webhook events"
            />
          </label>
          <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
            <label className="block">
              <span className="text-text-muted mb-1 block text-xs">Secret</span>
              <AdminTextInput
                value={form.secret}
                onChange={(value) => setForm((current) => ({ ...current, secret: value }))}
                disabled={localMode || saving}
                placeholder="Optional HMAC secret"
                type="password"
                ariaLabel="Webhook secret"
              />
            </label>
            <label className="block">
              <span className="text-text-muted mb-1 block text-xs">Space ID</span>
              <AdminTextInput
                value={form.space_id}
                onChange={(value) => setForm((current) => ({ ...current, space_id: value }))}
                disabled={localMode || saving}
                placeholder="Optional"
                ariaLabel="Webhook space ID"
              />
            </label>
            <Button size="sm" disabled={localMode || saving} className="self-end">
              Create
            </Button>
          </div>
        </form>

        <div className="grid gap-3 xl:grid-cols-[minmax(18rem,24rem)_minmax(0,1fr)]">
          <div className="rounded-card border-border border">
            {webhooks.length === 0 ? (
              <div className="text-text-muted p-3 text-sm">
                {loading ? "Loading webhooks..." : "No webhooks configured."}
              </div>
            ) : (
              webhooks.map((webhook) => (
                <button
                  key={webhook.id}
                  type="button"
                  onClick={() => setSelectedWebhookId(webhook.id)}
                  className={`border-border block w-full border-b px-3 py-2 text-left last:border-0 ${
                    selectedWebhookId === webhook.id ? "bg-surface-raised" : "hover:bg-surface"
                  }`}
                >
                  <span className="text-text block truncate text-sm font-medium">
                    {webhook.name}
                  </span>
                  <span className="text-text-muted block truncate text-xs">{webhook.url}</span>
                  <span className="text-text-muted mt-1 flex items-center gap-2 text-xs">
                    <span>{webhook.events.length} events</span>
                    <span>{webhook.is_active ? "Active" : "Paused"}</span>
                  </span>
                </button>
              ))
            )}
          </div>

          <div className="rounded-card border-border border p-3">
            {selectedWebhook ? (
              <div className="space-y-3">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <h2 className="text-text truncate text-sm font-medium">
                      {selectedWebhook.name}
                    </h2>
                    <p className="text-text-muted truncate text-xs">{selectedWebhook.url}</p>
                    <div className="mt-2 flex flex-wrap gap-1">
                      {selectedWebhook.events.map((eventName) => (
                        <span
                          key={eventName}
                          className="rounded-button bg-surface-raised text-text-muted px-2 py-0.5 text-xs"
                        >
                          {eventName}
                        </span>
                      ))}
                    </div>
                  </div>
                  <Button
                    size="sm"
                    variant="danger"
                    disabled={localMode}
                    onClick={() => {
                      void deleteWebhook(selectedWebhook.id);
                    }}
                  >
                    <Trash2 size={14} className="mr-1" />
                    Delete
                  </Button>
                </div>

                <div>
                  <h3 className="text-text mb-2 text-sm font-medium">Delivery History</h3>
                  {deliveriesLoading ? (
                    <p className="text-text-muted text-sm">Loading deliveries...</p>
                  ) : deliveriesError ? (
                    <div className="grid gap-3 text-sm text-text-muted">
                      <p>{deliveriesError}</p>
                      <Button
                        variant="secondary"
                        size="sm"
                        className="justify-self-start"
                        onClick={() => void loadDeliveries(selectedWebhook.id)}
                      >
                        Retry deliveries
                      </Button>
                    </div>
                  ) : deliveries.length === 0 ? (
                    <p className="text-text-muted text-sm">No deliveries recorded yet.</p>
                  ) : (
                    <AdminWebhookDeliveryHistoryTable
                      deliveries={deliveries}
                      localMode={localMode}
                      redeliveringDeliveryId={redeliveringDeliveryId}
                      redeliveryError={redeliveryError}
                      onRedeliver={(deliveryId) => {
                        void redeliver(deliveryId);
                      }}
                    />
                  )}
                </div>
              </div>
            ) : (
              <div>
                <h3 className="text-text mb-2 text-sm font-medium">Delivery History</h3>
                <p className="text-text-muted text-sm">
                  Select a webhook to inspect delivery history.
                </p>
              </div>
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

export function parseWebhookEvents(value: string): string[] {
  return value
    .split(",")
    .map((eventName) => eventName.trim())
    .filter(Boolean);
}

async function postNoBody(path: string): Promise<void> {
  const headers: Record<string, string> = {};
  const token = api.getToken();
  if (token) headers.Authorization = `Bearer ${token}`;
  const response = await fetch(`${getApiBase()}${path}`, { method: "POST", headers });
  if (!response.ok) {
    const error = await response.json().catch(() => ({ message: "Request failed" }));
    throw new Error(error.message || `HTTP ${response.status}`);
  }
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
    { id: string; user_email?: string; action: string; target_type?: string; created_at: string }[]
  >([]);

  useEffect(() => {
    if (localMode) {
      setEntries([]);
      return;
    }
    api
      .fetch<typeof entries>("/admin/audit-log?limit=50")
      .then(setEntries)
      .catch(() => {});
  }, [localMode]);

  return (
    <div>
      <div className="mb-4 flex items-center justify-between">
        <h1 className="text-text text-lg font-medium">Audit Log</h1>
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
                <td className="text-text-muted px-3 py-2">{e.target_type || "—"}</td>
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
        const selectedPath = await chooseVaultArchiveExportPath(
          `starlane-vault-${stamp}.starlane-vault`,
        );
        if (!selectedPath) return;
        await dataClient.exportLocalVaultArchive(selectedPath);
        setMessage("Vault archive exported.");
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

  const importVaultArchive = () => {
    setArchiveImportConfirmOpen(true);
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
      const snapshot = await dataClient.exportServerSpaceSnapshot(space.id);
      const bundle = buildStaticSiteBundle(snapshot, getStaticSiteOptions(space.name));
      downloadBlob(bundle.filename, buildStaticSiteZipBlob(bundle));
      setMessage(formatStaticSiteBundleSummary(bundle, `${space.name} static ZIP`));
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to export static ZIP");
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
}: {
  space: SpaceItem;
  checkpoint: SharedSpaceDownloadCheckpoint | null;
  busy: boolean;
  onDownload: (space: SpaceItem) => Promise<void>;
  onExportStatic: (space: SpaceItem) => Promise<void>;
  onExportStaticZip: (space: SpaceItem) => Promise<void>;
}) {
  return (
    <div className="flex items-center gap-3 border-b border-border/70 px-3 py-2 last:border-b-0">
      <Globe size={15} className="shrink-0 text-text-muted" />
      <span className="min-w-0 flex-1">
        <span className="block truncate text-sm text-text">{space.name}</span>
        <span className="block truncate text-xs text-text-muted">{space.slug}</span>
        {checkpoint && (
          <span className="block truncate text-xs text-text-muted">
            {formatSharedSpaceDownloadCheckpoint(checkpoint)}
          </span>
        )}
      </span>
      <div className="flex shrink-0 gap-2">
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
          disabled={busy}
        >
          <Download size={14} />
          ZIP
        </Button>
      </div>
    </div>
  );
}

interface PluginInstallRecordCardProps {
  record: PluginInstallRecord;
  spaces: SpaceItem[];
  onToggle: (id: string, enabled: boolean) => void;
  onRemove: (id: string) => void;
  onToggleSpace: (id: string, spaceId: string, enabled: boolean) => void;
}

function PluginInstallRecordCard({
  record,
  spaces,
  onToggle,
  onRemove,
  onToggleSpace,
}: PluginInstallRecordCardProps) {
  const runtimeStatus = getPluginRuntimeStatus(record);
  return (
    <div className="rounded-input border-border grid gap-2 border px-3 py-2 text-xs">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-text">{record.manifest.name}</p>
          <p className="truncate font-mono text-text-muted">
            {record.id} @ {record.manifest.version}
          </p>
        </div>
        <span className="shrink-0 text-text-muted">{record.enabled ? "Enabled" : "Disabled"}</span>
      </div>
      <div className="rounded-input border border-border px-2 py-1.5">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="text-text">
            Runtime: {runtimeStatus.runtime_state === "disabled" ? "Disabled" : "Blocked"}
          </span>
          <span className="text-text-muted">
            {runtimeStatus.permission_review_level} review / {runtimeStatus.enabled_space_count} spaces
          </span>
        </div>
        <p className="mt-1 text-text-muted">{runtimeStatus.reason}</p>
      </div>
      <div className="rounded-input border border-border px-2 py-1.5">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="text-text">Source: {formatPluginInstallSourceDetails(record.sourceDetails)}</span>
          <span className="text-text-muted">{formatPluginAuditDate(record.sourceDetails.capturedAt)}</span>
        </div>
        {(record.sourceDetails.manifestUrl || record.sourceDetails.sourceUrl || record.sourceDetails.homepage) && (
          <p className="mt-1 truncate font-mono text-text-muted">
            {record.sourceDetails.manifestUrl ?? record.sourceDetails.sourceUrl ?? record.sourceDetails.homepage}
          </p>
        )}
      </div>
      <div className="flex flex-wrap justify-end gap-2">
        <Button size="sm" variant="secondary" onClick={() => onToggle(record.id, !record.enabled)}>
          {record.enabled ? "Disable" : "Enable"}
        </Button>
        <Button size="sm" variant="ghost" onClick={() => onRemove(record.id)}>
          <Trash2 size={14} />
          Remove
        </Button>
      </div>
      <div className="border-border border-t pt-2">
        <p className="mb-2 text-text-muted">Enabled spaces</p>
        {spaces.length === 0 ? (
          <p className="text-text-muted">No spaces available.</p>
        ) : (
          <div className="grid gap-1">
            {spaces.map((space) => (
              <label
                key={space.id}
                className="flex items-center justify-between gap-3 rounded-input border border-border px-2 py-1.5"
              >
                <span className="min-w-0">
                  <span className="block truncate text-text">{space.name}</span>
                  <span className="block truncate font-mono text-text-muted">{space.slug}</span>
                </span>
                <input
                  type="checkbox"
                  checked={record.enabledSpaceIds.includes(space.id)}
                  onChange={(event) => onToggleSpace(record.id, space.id, event.target.checked)}
                  aria-label={`Enable ${record.manifest.name} for ${space.name}`}
                />
              </label>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function formatPluginAuditDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleDateString("en", { month: "short", day: "numeric" });
}

function formatPluginPermissionReviewLevel(level: PluginPermissionReviewLevel): string {
  switch (level) {
    case "high":
      return "High review";
    case "medium":
      return "Medium review";
    case "low":
      return "Low review";
  }
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

const PLUGIN_MANIFEST_SAMPLE = JSON.stringify(
  {
    id: "com.example.my-plugin",
    name: "My Plugin",
    version: "1.0.0",
    description: "Does something useful",
    author: "Jane Smith",
    license: "MIT",
    homepage: "https://github.com/example/my-plugin",
    starlane_version: ">=0.1.0",
    permissions: {
      server: ["read_docs", "http_fetch"],
      client: ["editor_commands", "sidebar_panel"],
    },
    http_fetch: ["api.example.com"],
    settings_schema: [
      {
        key: "api_key",
        label: "API Key",
        type: "secret",
        required: true,
      },
    ],
  },
  null,
  2,
);

const PLUGIN_REGISTRY_SAMPLE = JSON.stringify(
  {
    schema_version: 1,
    generated_at: "2026-07-07T00:00:00.000Z",
    plugins: [
      {
        id: "com.example.my-plugin",
        name: "My Plugin",
        version: "1.0.0",
        description: "Does something useful",
        manifest_url: "https://plugins.example.com/my-plugin/plugin.json",
        source_url: "https://github.com/example/my-plugin",
        categories: ["editor", "automation"],
        verified: false,
      },
    ],
  },
  null,
  2,
);

function buildPluginManifestReviewStub(entry: PluginRegistryIndexEntry) {
  return {
    id: entry.id,
    name: entry.name,
    version: entry.version,
    description: entry.description,
    author: entry.author ?? "Unknown",
    license: entry.license ?? "Unspecified",
    homepage: entry.homepage ?? entry.source_url,
    starlane_version: ">=0.1.0",
    permissions: {
      server: [],
      client: [],
    },
    settings_schema: [],
    review_source: {
      type: "registry-index",
      manifest_url: entry.manifest_url,
      source_url: entry.source_url,
      verified: entry.verified,
      categories: entry.categories,
      note: "Review stub generated from registry metadata only; manifest source was not fetched.",
    },
  };
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

function PluginsSection() {
  const [manifestText, setManifestText] = useState(PLUGIN_MANIFEST_SAMPLE);
  const [result, setResult] = useState<PluginManifestValidationResult | null>(null);
  const [registryText, setRegistryText] = useState(PLUGIN_REGISTRY_SAMPLE);
  const [registryResult, setRegistryResult] =
    useState<PluginRegistryIndexValidationResult | null>(null);
  const [registrySearch, setRegistrySearch] = useState("");
  const [selectedRegistryEntryId, setSelectedRegistryEntryId] = useState<string | null>(null);
  const [installRecords, setInstallRecords] = useState<PluginInstallRecord[]>(() =>
    readPluginInstallRecords(),
  );
  const [auditEvents, setAuditEvents] = useState<PluginInstallAuditEvent[]>(() =>
    readPluginInstallAuditEvents(),
  );
  const [runtimeAuditEvents, setRuntimeAuditEvents] = useState<PluginRuntimeAuditEvent[]>(() =>
    readPluginRuntimeAuditEvents(),
  );
  const [runtimeAuditActionFilter, setRuntimeAuditActionFilter] = useState<
    "all" | PluginRuntimeAuditEvent["action"]
  >("all");
  const [runtimeAuditExportFormat, setRuntimeAuditExportFormat] = useState<"json" | "csv">("json");
  const [spaces, setSpaces] = useState<SpaceItem[]>([]);

  useEffect(() => {
    dataClient
      .listSpaces()
      .then((items) => setSpaces(items as SpaceItem[]))
      .catch(() => setSpaces([]));
  }, []);

  const validateManifest = () => {
    try {
      const parsed = JSON.parse(manifestText) as unknown;
      setResult(validatePluginManifest(parsed));
    } catch (err) {
      setResult({
        manifest: null,
        errors: [err instanceof Error ? err.message : "Manifest JSON could not be parsed."],
        warnings: [],
      });
    }
  };

  const validateRegistry = () => {
    try {
      const parsed = JSON.parse(registryText) as unknown;
      setRegistryResult(validatePluginRegistryIndex(parsed));
    } catch (err) {
      setRegistryResult({
        index: null,
        errors: [err instanceof Error ? err.message : "Registry index JSON could not be parsed."],
        warnings: [],
      });
    }
  };

  const manifest = result?.manifest;
  const registryIndex = registryResult?.index;
  const registryEntries = useMemo(() => {
    if (!registryIndex) return [];
    const query = registrySearch.trim().toLowerCase();
    if (!query) return registryIndex.plugins;
    return registryIndex.plugins.filter((plugin) =>
      [
        plugin.name,
        plugin.id,
        plugin.version,
        plugin.description,
        plugin.author,
        plugin.license,
        ...(plugin.categories ?? []),
      ]
        .filter((value): value is string => Boolean(value))
        .some((value) => value.toLowerCase().includes(query)),
    );
  }, [registryIndex, registrySearch]);
  const selectedRegistryEntry =
    registryEntries.find((plugin) => `${plugin.id}@${plugin.version}` === selectedRegistryEntryId) ??
    registryIndex?.plugins.find((plugin) => `${plugin.id}@${plugin.version}` === selectedRegistryEntryId) ??
    null;
  const permissionReview = manifest ? summarizePluginPermissionReview(manifest) : null;
  const filteredRuntimeAuditEvents = useMemo(
    () =>
      filterPluginRuntimeAuditEvents(
        runtimeAuditEvents,
        runtimeAuditActionFilter === "all" ? {} : { action: runtimeAuditActionFilter },
      ),
    [runtimeAuditActionFilter, runtimeAuditEvents],
  );
  const runtimeAuditExportText = useMemo(
    () =>
      serializePluginRuntimeAuditEvents(
        filteredRuntimeAuditEvents,
        runtimeAuditExportFormat,
      ),
    [filteredRuntimeAuditEvents, runtimeAuditExportFormat],
  );

  const recordAuditEvent = (
    input: Omit<PluginInstallAuditEvent, "id" | "createdAt">,
  ) => {
    const nextEvents = appendPluginInstallAuditEvent(auditEvents, input);
    setAuditEvents(nextEvents);
    writePluginInstallAuditEvents(nextEvents);
  };

  const recordRuntimeAuditEvent = (
    input: Omit<PluginRuntimeAuditEvent, "id" | "createdAt">,
  ) => {
    const nextEvents = appendPluginRuntimeAuditEvent(runtimeAuditEvents, input);
    setRuntimeAuditEvents(nextEvents);
    writePluginRuntimeAuditEvents(nextEvents);
    void api
      .fetch("/admin/plugin-runtime-audit", {
        method: "POST",
        body: {
          plugin_id: input.pluginId,
          plugin_name: input.pluginName,
          action: input.action,
          runtime_state: input.runtimeState,
          reason: input.reason,
          space_id: input.spaceId,
          space_name: input.spaceName,
        },
      })
      .catch(() => {});
  };

  const saveInstallRecord = () => {
    if (!manifest) return;
    const sourceDetails = getPluginInstallSourceDetailsForManifest(manifest, registryIndex?.plugins);
    const nextRecords = upsertPluginInstallRecord(installRecords, manifest, sourceDetails);
    setInstallRecords(nextRecords);
    writePluginInstallRecords(nextRecords);
    recordAuditEvent({
      pluginId: manifest.id,
      pluginName: manifest.name,
      action: "install_record_saved",
    });
  };

  const prepareRegistryEntryReview = (entry: PluginRegistryIndexEntry) => {
    const stub = buildPluginManifestReviewStub(entry);
    setManifestText(JSON.stringify(stub, null, 2));
    setResult(null);
  };

  const toggleInstallRecord = (id: string, enabled: boolean) => {
    const record = installRecords.find((item) => item.id === id);
    const nextRecords = setPluginInstallRecordEnabled(installRecords, id, enabled);
    setInstallRecords(nextRecords);
    writePluginInstallRecords(nextRecords);
    if (record) {
      recordAuditEvent({
        pluginId: record.id,
        pluginName: record.manifest.name,
        action: enabled ? "install_record_enabled" : "install_record_disabled",
      });
      if (enabled) {
        const status = getPluginRuntimeStatus({ ...record, enabled: true });
        recordRuntimeAuditEvent({
          pluginId: record.id,
          pluginName: record.manifest.name,
          action: "runtime_enable_blocked",
          runtimeState: status.runtime_state,
          reason: status.reason,
        });
      }
    }
  };

  const removeInstallRecord = (id: string) => {
    const record = installRecords.find((item) => item.id === id);
    const nextRecords = removePluginInstallRecord(installRecords, id);
    setInstallRecords(nextRecords);
    writePluginInstallRecords(nextRecords);
    if (record) {
      recordAuditEvent({
        pluginId: record.id,
        pluginName: record.manifest.name,
        action: "install_record_removed",
      });
    }
  };

  const toggleInstallRecordSpace = (id: string, spaceId: string, enabled: boolean) => {
    const record = installRecords.find((item) => item.id === id);
    const space = spaces.find((item) => item.id === spaceId);
    const nextRecords = setPluginInstallRecordSpaceEnabled(
      installRecords,
      id,
      spaceId,
      enabled,
    );
    setInstallRecords(nextRecords);
    writePluginInstallRecords(nextRecords);
    if (record) {
      recordAuditEvent({
        pluginId: record.id,
        pluginName: record.manifest.name,
        action: enabled ? "space_enabled" : "space_disabled",
        spaceId,
        spaceName: space?.name,
      });
      if (enabled) {
        const status = getPluginRuntimeStatus({
          ...record,
          enabled: true,
          enabledSpaceIds: [...new Set([...record.enabledSpaceIds, spaceId])],
        });
        recordRuntimeAuditEvent({
          pluginId: record.id,
          pluginName: record.manifest.name,
          action: "runtime_space_enable_blocked",
          runtimeState: status.runtime_state,
          reason: status.reason,
          spaceId,
          spaceName: space?.name,
        });
      }
    }
  };

  return (
    <div>
      <div className="mb-4 flex items-center justify-between gap-3">
        <div>
          <h1 className="text-text text-lg font-medium">Plugins</h1>
          <p className="text-text-muted mt-1 text-xs">
            Validate plugin manifests before install, sandboxing, and runtime execution are enabled.
          </p>
        </div>
        <span className="text-text-muted ring-border shrink-0 rounded px-2 py-1 text-xs ring-1">
          Contract only
        </span>
      </div>

      <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_minmax(18rem,24rem)]">
        <div className="space-y-3">
          <div className="rounded-card border-border border p-3">
            <div className="mb-3 flex items-center justify-between gap-3">
              <p className="text-text text-sm font-medium">plugin.json</p>
              <Button size="sm" onClick={validateManifest}>
                Validate
              </Button>
            </div>
            <PluginJsonTextarea
              value={manifestText}
              onChange={setManifestText}
              className="h-[32rem]"
              ariaLabel="Plugin manifest JSON"
            />
          </div>

          <div className="rounded-card border-border border p-3">
            <div className="mb-3 flex items-center justify-between gap-3">
              <p className="text-text text-sm font-medium">registry.json</p>
              <Button size="sm" onClick={validateRegistry}>
                Validate Registry
              </Button>
            </div>
            <PluginJsonTextarea
              value={registryText}
              onChange={setRegistryText}
              className="h-72"
              ariaLabel="Plugin registry index JSON"
            />
            {!registryResult ? (
              <p className="text-text-muted mt-3 text-sm">No registry index validated yet.</p>
            ) : registryResult.errors.length > 0 ? (
              <div className="mt-3 space-y-2">
                {registryResult.errors.map((error) => (
                  <div key={error} className={ADMIN_PLUGIN_ERROR_CARD_CLASS}>
                    {error}
                  </div>
                ))}
              </div>
            ) : (
              <div className="mt-3 space-y-3">
                <div className={ADMIN_PLUGIN_SUCCESS_CARD_CLASS}>
                  Registry index contract is valid.
                </div>
                {registryIndex && (
                  <div className="grid gap-2 text-xs">
                    <div className="rounded-input border-border flex items-center justify-between gap-3 border px-3 py-2">
                      <span className="text-text-muted">Catalog entries</span>
                      <span className="text-text">{registryIndex.plugins.length}</span>
                    </div>
                    <AdminTextInput
                      value={registrySearch}
                      onChange={setRegistrySearch}
                      placeholder="Filter catalog"
                      className="h-8 text-xs"
                      ariaLabel="Filter plugin registry entries"
                    />
                    <div className="max-h-56 space-y-2 overflow-auto pr-1">
                      {registryEntries.length === 0 ? (
                        <p className="text-text-muted">No registry entries match this filter.</p>
                      ) : (
                        registryEntries.map((plugin) => {
                          const entryKey = `${plugin.id}@${plugin.version}`;
                          const selected = selectedRegistryEntryId === entryKey;
                          return (
                            <button
                              key={entryKey}
                              type="button"
                              onClick={() => setSelectedRegistryEntryId(entryKey)}
                              className={`rounded-input border px-3 py-2 text-left ${
                                selected
                                  ? "border-primary bg-primary/10"
                                  : "border-border bg-surface"
                              }`}
                            >
                              <div className="flex items-start justify-between gap-3">
                                <span className="min-w-0">
                                  <span className="block truncate text-text">{plugin.name}</span>
                                  <span className="block truncate font-mono text-text-muted">
                                    {plugin.id} @ {plugin.version}
                                  </span>
                                </span>
                                <span className="shrink-0 text-text-muted">
                                  {plugin.verified ? "Verified" : "Unverified"}
                                </span>
                              </div>
                              {plugin.categories.length > 0 && (
                                <span className="mt-2 flex flex-wrap gap-1">
                                  {plugin.categories.map((category) => (
                                    <span
                                      key={category}
                                      className="rounded bg-surface-raised px-1.5 py-0.5 text-text-muted"
                                    >
                                      {category}
                                    </span>
                                  ))}
                                </span>
                              )}
                            </button>
                          );
                        })
                      )}
                    </div>
                    {selectedRegistryEntry && (
                      <div className="rounded-input border-border border px-3 py-2">
                        <div className="flex items-start justify-between gap-3">
                          <div className="min-w-0">
                            <p className="truncate text-text">{selectedRegistryEntry.name}</p>
                            <p className="mt-1 text-text-muted">
                              {selectedRegistryEntry.description}
                            </p>
                            <p className="mt-2 truncate font-mono text-text-muted">
                              {selectedRegistryEntry.manifest_url}
                            </p>
                          </div>
                          <Button
                            size="sm"
                            variant="secondary"
                            onClick={() => prepareRegistryEntryReview(selectedRegistryEntry)}
                          >
                            Prepare review
                          </Button>
                        </div>
                      </div>
                    )}
                    <p className="text-text-muted">
                      Registry browsing uses validated catalog metadata only; manifest fetching,
                      install sources, and runtime execution remain disabled.
                    </p>
                  </div>
                )}
              </div>
            )}
            {registryResult && registryResult.warnings.length > 0 && (
              <div className="mt-3 space-y-2">
                {registryResult.warnings.map((warning) => (
                  <div
                    key={warning}
                    className="rounded-input border-border border px-3 py-2 text-xs text-text-muted"
                  >
                    {warning}
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        <div className="space-y-3">
          <div className="rounded-card border-border border p-3">
            <p className="text-text text-sm font-medium">Validation Result</p>
            {!result ? (
              <p className="text-text-muted mt-2 text-sm">No manifest validated yet.</p>
            ) : result.errors.length > 0 ? (
              <div className="mt-3 space-y-2">
                {result.errors.map((error) => (
                  <div key={error} className={ADMIN_PLUGIN_ERROR_CARD_CLASS}>
                    {error}
                  </div>
                ))}
              </div>
            ) : (
              <div className="mt-3 space-y-3">
                <div className={ADMIN_PLUGIN_SUCCESS_CARD_CLASS}>
                  Manifest contract is valid.
                </div>
                {manifest && (
                  <>
                    <dl className="grid gap-2 text-xs">
                      <div>
                        <dt className="text-text-muted">Plugin</dt>
                        <dd className="text-text mt-0.5">{manifest.name}</dd>
                      </div>
                      <div>
                        <dt className="text-text-muted">ID</dt>
                        <dd className="text-text mt-0.5 font-mono">{manifest.id}</dd>
                      </div>
                      <div>
                        <dt className="text-text-muted">Server permissions</dt>
                        <dd className="text-text mt-0.5">
                          {manifest.permissions.server.join(", ") || "None"}
                        </dd>
                      </div>
                      <div>
                        <dt className="text-text-muted">Client permissions</dt>
                        <dd className="text-text mt-0.5">
                          {manifest.permissions.client.join(", ") || "None"}
                        </dd>
                      </div>
                    </dl>
                    {permissionReview && (
                      <div className="rounded-input border-border border p-3 text-xs">
                        <div className="mb-2 flex items-center justify-between gap-3">
                          <p className="text-text font-medium">Permission Review</p>
                          <span
                            className={`shrink-0 rounded px-2 py-1 ${getPluginPermissionReviewClass(permissionReview.level)}`}
                          >
                            {formatPluginPermissionReviewLevel(permissionReview.level)}
                          </span>
                        </div>
                        <div className="grid gap-2">
                          {permissionReview.items.map((item) => (
                            <div
                              key={item.id}
                              className="rounded-input border-border border px-2 py-1.5"
                            >
                              <div className="flex items-center justify-between gap-3">
                                <span className="text-text font-medium">{item.label}</span>
                                <span
                                  className={`shrink-0 rounded px-1.5 py-0.5 ${getPluginPermissionReviewClass(item.level)}`}
                                >
                                  {item.level}
                                </span>
                              </div>
                              <p className="text-text-muted mt-1">{item.description}</p>
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                    <Button size="sm" onClick={saveInstallRecord}>
                      <Puzzle size={14} />
                      Save reviewed install record
                    </Button>
                  </>
                )}
              </div>
            )}
          </div>

          <div className="rounded-card border-border border p-3">
            <div className="mb-3 flex items-center justify-between gap-3">
              <p className="text-text text-sm font-medium">Installed Plugins</p>
              <span className="text-text-muted text-xs">{installRecords.length} saved</span>
            </div>
            {installRecords.length === 0 ? (
              <p className="text-text-muted text-sm">No plugin install records saved.</p>
            ) : (
              <div className="space-y-2">
                {installRecords.map((record) => (
                  <PluginInstallRecordCard
                    key={record.id}
                    record={record}
                    spaces={spaces}
                    onToggle={toggleInstallRecord}
                    onRemove={removeInstallRecord}
                    onToggleSpace={toggleInstallRecordSpace}
                  />
                ))}
              </div>
            )}
          </div>

          <div className="rounded-card border-border border p-3">
            <div className="mb-3 flex items-center justify-between gap-3">
              <p className="text-text text-sm font-medium">Install Audit</p>
              <span className="text-text-muted text-xs">{auditEvents.length} events</span>
            </div>
            {auditEvents.length === 0 ? (
              <p className="text-text-muted text-sm">No plugin install actions recorded.</p>
            ) : (
              <div className="space-y-2">
                {auditEvents.slice(0, 6).map((event) => (
                  <div
                    key={event.id}
                    className="rounded-input border-border border px-3 py-2 text-xs"
                  >
                    <div className="flex items-start justify-between gap-3">
                      <span className="min-w-0">
                        <span className="block truncate text-text">
                          {formatPluginInstallAuditAction(event.action)}
                        </span>
                        <span className="block truncate font-mono text-text-muted">
                          {event.pluginName}
                          {event.spaceName ? ` / ${event.spaceName}` : ""}
                        </span>
                      </span>
                      <span className="shrink-0 text-text-muted">
                        {formatPluginAuditDate(event.createdAt)}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="rounded-card border-border border p-3">
            <p className="text-text text-sm font-medium">Runtime Status</p>
            <div className="mt-3 grid gap-2 text-xs">
              {[
                ["Manifest validation", "Available"],
                ["Registry index validation", "Available"],
                ["Admin install records", "Available"],
                ["Per-space enablement", "Available"],
                ["Local install audit", "Available"],
                ["Local runtime audit", "Available"],
                ["Deno server sandbox", "Planned"],
                ["Client iframe sandbox", "Planned"],
              ].map(([label, value]) => (
                <div
                  key={label}
                  className="rounded-input border-border flex items-center justify-between gap-3 border px-3 py-2"
                >
                  <span className="text-text-muted">{label}</span>
                  <span className="text-text">{value}</span>
                </div>
              ))}
            </div>
          </div>

          <div className="rounded-card border-border border p-3">
            <div className="mb-3 flex items-center justify-between gap-3">
              <p className="text-text text-sm font-medium">Runtime Audit</p>
              <span className="text-text-muted text-xs">
                {filteredRuntimeAuditEvents.length} of {runtimeAuditEvents.length} events
              </span>
            </div>
            <div className="mb-3 grid gap-2 sm:grid-cols-2">
              <AdminCompactSelect
                value={runtimeAuditActionFilter}
                onChange={(value) =>
                  setRuntimeAuditActionFilter(
                    value as "all" | PluginRuntimeAuditEvent["action"],
                  )
                }
                ariaLabel="Plugin runtime audit action filter"
              >
                <option value="all">All runtime events</option>
                <option value="runtime_enable_blocked">Global enable blocked</option>
                <option value="runtime_space_enable_blocked">Space enable blocked</option>
              </AdminCompactSelect>
              <AdminCompactSelect
                value={runtimeAuditExportFormat}
                onChange={(value) => setRuntimeAuditExportFormat(value as "json" | "csv")}
                ariaLabel="Plugin runtime audit export format"
              >
                <option value="json">JSON export</option>
                <option value="csv">CSV export</option>
              </AdminCompactSelect>
            </div>
            {runtimeAuditEvents.length === 0 ? (
              <p className="text-text-muted text-sm">No plugin runtime actions recorded.</p>
            ) : (
              <div className="space-y-3">
                {filteredRuntimeAuditEvents.length === 0 ? (
                  <p className="text-text-muted text-sm">No runtime audit events match the filter.</p>
                ) : (
                  <div className="space-y-2">
                    {filteredRuntimeAuditEvents.slice(0, 6).map((event) => (
                      <div
                        key={event.id}
                        className="rounded-input border-border border px-3 py-2 text-xs"
                      >
                        <div className="flex items-start justify-between gap-3">
                          <span className="min-w-0">
                            <span className="block truncate text-text">
                              {formatPluginRuntimeAuditAction(event.action)}
                            </span>
                            <span className="block truncate font-mono text-text-muted">
                              {event.pluginName}
                              {event.spaceName ? ` / ${event.spaceName}` : ""}
                            </span>
                            <span className="block truncate text-text-muted">{event.reason}</span>
                          </span>
                          <span className="shrink-0 text-text-muted">
                            {formatPluginAuditDate(event.createdAt)}
                          </span>
                        </div>
                      </div>
                    ))}
                  </div>
                )}
                <PluginJsonTextarea
                  value={runtimeAuditExportText}
                  readOnly
                  className="h-32"
                  ariaLabel="Plugin runtime audit export payload"
                />
              </div>
            )}
          </div>

          {result && result.warnings.length > 0 && (
            <div className="rounded-card border-border border p-3">
              <p className="text-text text-sm font-medium">Warnings</p>
              <div className="mt-3 space-y-2">
                {result.warnings.map((warning) => (
                  <div key={warning} className="rounded-input border-border border px-3 py-2 text-xs text-text-muted">
                    {warning}
                  </div>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function SystemSection({ localMode }: { localMode: boolean }) {
  const provider = getSyncProvider();
  const [configEntries, setConfigEntries] = useState<ConfigEntry[]>([]);
  const [configKey, setConfigKey] = useState("");
  const [configValue, setConfigValue] = useState("");
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

  const addConfig = async () => {
    if (!configKey.trim()) return;
    await saveConfig(configKey.trim(), configValue);
    setConfigKey("");
    setConfigValue("");
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
      <h1 className="text-text mb-4 text-lg font-medium">System</h1>
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
            <div className="mb-3 flex items-center justify-between">
              <div>
                <p className="text-text text-sm font-medium">Application Configuration</p>
                <p className="text-text-muted text-xs">
                  Environment-backed values are locked here and must be changed in `.env`.
                </p>
              </div>
            </div>
            <div className="space-y-2">
              {configEntries.map((entry) => (
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
                      setConfigEntries((prev) =>
                        prev.map((item) => (item.key === entry.key ? { ...item, value } : item)),
                      );
                    }}
                    disabled={entry.locked}
                    placeholder={entry.effective_value || "Not set"}
                    ariaLabel={`${entry.label} value`}
                  />
                  <span className="rounded-button border-border text-text-muted flex h-8 items-center border px-2 text-xs">
                    {entry.locked ? "env locked" : entry.source}
                  </span>
                  <div className="flex gap-2">
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={entry.locked || (entry.secret && !entry.value)}
                      onClick={() => {
                        void saveConfig(entry.key, entry.value);
                      }}
                    >
                      Save
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={entry.locked || entry.key === "setup_completed"}
                      onClick={() => {
                        void deleteConfig(entry.key);
                      }}
                    >
                      Remove
                    </Button>
                  </div>
                </div>
              ))}
            </div>
            <div className="mt-3 grid gap-2 lg:grid-cols-[minmax(0,14rem)_minmax(0,1fr)_auto]">
              <AdminTextInput
                value={configKey}
                onChange={setConfigKey}
                placeholder="custom.key"
                ariaLabel="New config key"
              />
              <AdminTextInput
                value={configValue}
                onChange={setConfigValue}
                placeholder="Value"
                ariaLabel="New config value"
              />
              <Button
                size="sm"
                onClick={() => {
                  void addConfig();
                }}
              >
                Add setting
              </Button>
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
                          <span className="rounded bg-surface-raised px-1.5 py-0.5 text-[10px] uppercase tracking-wide">
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
          <span className="text-text text-sm">1.3.0</span>
        </div>
        <div className="rounded-card border-border flex items-center justify-between border p-3">
          <span className="text-text-muted text-sm">Database</span>
          <span className="text-text text-sm">SQLite</span>
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
